//! Requests and epics (`docs/SPEC.md` sections 5.2, 5.16, and ADR 0013): the rules that take a
//! request from `draft` through triage and refining to `ready`.

use std::sync::Arc;

use catervas_core::branch::task_branch;
use catervas_core::contract::{
    ExitCriterion, Role, TaskContract, TaskId, TaskKind, TaskStatus, VerificationWire,
};
use catervas_core::folders::{is_plan_task, plan_stem, with_format};
use catervas_core::governor::done::{CriterionResult, RunBy, catervas_verifies_alone};
use catervas_core::governor::transition::TransitionRequest;
use catervas_core::governor::transition_table::TransitionActor;
use catervas_core::team::{Agent, AgentStatus, Team, changes_code, task_private_folder};
use catervas_protocol::event::{
    CatervasEvent, ContractEvaluatedBodyGate, CriterionRecordedBody, CriterionRecordedBodyRunBy,
    EventBody, EventKind, NoteWrittenBodyKind, ReviewRecordedBody,
};
use catervas_store::TaskProjection;
use catervas_store::files::FilesError;
use catervas_store::git::{GitError, held_document};
use catervas_store::product_plan::is_the_product_plan;
use catervas_store::repos::TaskRepository;

use super::messages::{
    breakdown_message, close_out_message, epic_accept_message, epic_review_message,
    given_task_message, plan_task_message, plan_tasks_message, refine_message, triage_message,
};
use super::rules::{Waiting, acted, active, has_room, refused_since_entering, spent, unavailable};
use super::session::{SessionAsk, TRIAGE_TOOL, run_session};
use super::verify::{
    GOVERNOR, append, context, decide_alone, escalate, failed, fails_the_criterion,
    governor_results, history, is_mechanical, ran_criteria, read_only, record_review, reject,
    reviewer_results, since_verifying, unanswered,
};
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, task_repository_of};
use crate::catervafication::{FileDue, catervafication_of, file_due};
use crate::criteria::{CriterionOutcome, remove_base_worktree, run_criteria};
use crate::session::SessionPurpose;
use crate::tools::ToolDeps;
use crate::transitions::{
    TransitionAsk, TransitionOutcome, integration_branch, last_move_into, refining_began,
    refusal_details, result_accepted, the_human_reviews_the_epic,
};

/// The human, as the reviewer of an epic the Product Manager holds (5.16 item 4).
pub(super) const HUMAN: &str = "human";

/// The Product Manager: the first active agent of that role in team-file order.
pub(super) fn product_manager(team: &Team) -> Option<&Agent> {
    team.active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
}

/// The Scrum Master: the first active agent of that role in team-file order.
fn scrum_master(team: &Team) -> Option<&Agent> {
    team.active_agents()
        .find(|agent| Role::from(agent.role) == Role::ScrumMaster)
}

/// Who triages a request: the Scrum Master when the team has an active one, else the Product
/// Manager (5.16).
pub(super) fn triager(team: &Team) -> Option<&Agent> {
    scrum_master(team).or_else(|| product_manager(team))
}

/// Rule 10: a `draft`. Untriaged, it gets the triager's triage session; triaged, it is moved to
/// `refining` on the Product Manager's behalf, with no session, unless that move was refused since
/// the task became a draft.
pub(super) async fn draft(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    if !row.triaged {
        let triager = triager(team).unwrap_or(pm);
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        if spent(deps, team, &contract, &mut waiting.day_spent)?
            | unavailable(deps, triager, &mut waiting.slept)?
        {
            return Ok(None);
        }
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: triager,
                contract: Some(&contract),
                purpose: SessionPurpose::Triage,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: false,
                only_tool: Some(TRIAGE_TOOL),
                tools: None,
                in_reply_to: None,
                thread: None,
                initial_prompt: triage_message(&contract),
            },
        )
        .await?;
        return Ok(Some(acted(row, triager, "triage", &end)));
    }
    if refused_since_entering(deps, &row.task_id, TaskStatus::Draft, TaskStatus::Refining)? {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(
        &catervas_core::governor::transition::TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Refining,
            actor: TransitionActor::ProductManager,
            agent_id: Some(pm.id.to_string()),
        },
        &TransitionAsk::default(),
        team,
    )?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("started refining it for {}", pm.id.as_str()),
        }),
        TransitionOutcome::Refused(_) => None,
    })
}

/// Rule 9: a contract `refining`. A contract written since refining began and not judged since,
/// or one filed whole (a breakdown's child, or a contract the human holds) and not judged since
/// refining began, is judged by the governor's rules alone: `escalated` asked as the governor
/// first, whose rows open on three readiness failures, then `ready`. No agent judges a contract
/// (ADR 0061). Otherwise the Product Manager gets a refine session.
pub(super) async fn refining(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let given = given_agent(deps, team, &contract)?;
    if matches!(given, Given::No) && product_manager(team).is_none() {
        return Ok(None);
    }
    let history = history(deps, &row.task_id)?;
    // A new project plans nothing but its product plan until the plan is approved (5.3): the
    // contract waits, with no session and no governor ask to spend an attempt or escalate.
    if !is_the_product_plan(&history) && deps.tools.transitions.awaits_the_product_plan()? {
        return Ok(None);
    }
    let began = refining_began(&history);
    if is_to_be_judged(&contract, &history, began) {
        return judge(deps, team, row).map(Some);
    }
    // The agent a task was given to in its chat writes its contract (ADR 0063); a paused one's
    // task waits.
    let (writer, lead) = match given {
        Given::Active(agent) => (agent, Some(agent)),
        Given::Waits => return Ok(None),
        Given::No => match product_manager(team) {
            Some(pm) => (pm, None),
            None => return Ok(None),
        },
    };
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | unavailable(deps, writer, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let asked = history
        .iter()
        .any(|event| event.envelope.seq > began && event.body.kind() == EventKind::QuestionAsked);
    let failures = last_readiness_failures(&history, began);
    let initial_prompt = match lead {
        Some(agent) => given_task_message(&contract, agent, !asked, &failures),
        None => refine_message(&contract, !asked, &failures),
    };
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: writer,
            contract: Some(&contract),
            purpose: SessionPurpose::Refine,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            only_tool: None,
            tools: None,
            in_reply_to: None,
            thread: None,
            initial_prompt,
        },
    )
    .await?;
    Ok(Some(acted(row, writer, "refine", &end)))
}

/// Whose task a contract is, when the owner gave it to an agent in its chat (ADR 0063).
pub(super) enum Given<'a> {
    /// Not given to an agent, or no longer: the agent is not on the team or has retired, or the
    /// human changed the contract's assignee role.
    No,
    /// Given to this active agent.
    Active(&'a Agent),
    /// Given to a paused agent, whose task waits.
    Waits,
}

/// Whose task `contract` is: the agent its `task.created` names in `given_to`, while that agent is
/// on the team, has not retired, and is still the contract's assignee role.
pub(super) fn given_agent<'a>(
    deps: &OrchestratorDeps,
    team: &'a Team,
    contract: &TaskContract,
) -> Result<Given<'a>, OrchestratorError> {
    let Some(given) = catervas_store::requests::given_to(&deps.tools.log, &contract.id)? else {
        return Ok(Given::No);
    };
    let Some(agent) = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == given && agent.status != AgentStatus::Retired)
    else {
        return Ok(Given::No);
    };
    if Role::from(agent.role) != contract.assignee_role {
        return Ok(Given::No);
    }
    Ok(match agent.status {
        AgentStatus::Active => Given::Active(agent),
        _ => Given::Waits,
    })
}

/// Asks the governor to judge a contract: `escalated` first, because asking for `ready` on a
/// passing epic fails on its missing approval and would count as a readiness failure, then `ready`.
fn judge(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<TickReport, OrchestratorError> {
    let ask = |to: TaskStatus| {
        deps.tools.transitions.request(
            &catervas_core::governor::transition::TransitionRequest {
                task_id: row.task_id.clone(),
                to,
                actor: TransitionActor::Governor,
                agent_id: None,
            },
            &TransitionAsk::default(),
            team,
        )
    };
    let what = match ask(TaskStatus::Escalated)? {
        TransitionOutcome::Moved(_) => "escalated its contract to the human".to_string(),
        TransitionOutcome::Refused(_) => match ask(TaskStatus::Ready)? {
            TransitionOutcome::Moved(_) => "judged its contract ready".to_string(),
            TransitionOutcome::Refused(refusal) => format!(
                "judged its contract, which is not ready: {}",
                refusal_details(&refusal).join("; ")
            ),
        },
    };
    Ok(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    })
}

/// Whether the contract is judged now: written since refining began with no refusal by the
/// governor since that write, or, with no such write, filed whole (a breakdown's task, a contract
/// the human holds, a twin's re-derive, an owner's change or rewrite, a new project's product
/// plan, a Catervafication's epic) and not refused since refining began. A raw request is not
/// judged: its brief would fail and spend one of the three attempts.
fn is_to_be_judged(contract: &TaskContract, history: &[CatervasEvent], began: u64) -> bool {
    let refused_after = |after: u64| {
        history.iter().any(|event| {
            event.envelope.seq > after
                && matches!(&event.body, EventBody::TransitionRefused(body)
                    if body.from.to_string() == "refining" && body.requested_by == "governor")
        })
    };
    let written = history
        .iter()
        .rev()
        .find(|event| event.envelope.seq > began && event.body.kind() == EventKind::ContractWritten)
        .map(|event| event.envelope.seq);
    let is_a_rederive = history.iter().any(
        |event| matches!(&event.body, EventBody::TaskCreated(body) if body.rederives.is_some() || body.filed_as.is_some()),
    );
    // The product plan's task and the Catervafication's epic are Catervas's contracts, filed
    // whole: no refine session rewrites them.
    let filed_whole = contract.parent.is_some()
        || contract.locked
        || is_a_rederive
        || is_the_product_plan(history)
        || catervafication_of(history).is_some();
    match written {
        Some(written) => !refused_after(written),
        None => filed_whole && !refused_after(began),
    }
}

/// The failures of the last Definition of Ready judged since refining began, when it failed.
fn last_readiness_failures(history: &[CatervasEvent], began: u64) -> Vec<String> {
    history
        .iter()
        .rev()
        .filter(|event| event.envelope.seq > began)
        .find_map(|event| match &event.body {
            EventBody::ContractEvaluated(body)
                if body.gate == ContractEvaluatedBodyGate::DefinitionOfReady =>
            {
                Some(if body.passed {
                    Vec::new()
                } else {
                    body.failures.clone()
                })
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether `row` is an epic, which rules 5 to 8 give the epic's own rules.
pub(super) fn is_epic(row: &TaskProjection) -> bool {
    row.kind == TaskKind::Epic
}

/// Rule 8 for an epic `ready`: with an active Scrum Master, assigned to it on its behalf with the
/// Product Manager as reviewer; without one, to the Product Manager on its behalf with no reviewer
/// (the human reviews it). No session, and only when the assignee holds fewer open tasks than the
/// WIP limit, counted as the assignment gate counts them, and every dependency is accepted;
/// otherwise passed over, so that the run
/// idles rather than being refused on every tick.
pub(super) fn ready_epic(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    let (assignee, actor, reviewer) = match scrum_master(team) {
        Some(sm) => (sm, TransitionActor::ScrumMaster, Some(pm.id.to_string())),
        None => (pm, TransitionActor::ProductManager, None),
    };
    if !has_room(team, board, assignee, None)
        || refused_since_entering(deps, &row.task_id, TaskStatus::Ready, TaskStatus::Assigned)?
    {
        return Ok(None);
    }
    let request = TransitionRequest {
        task_id: row.task_id.clone(),
        to: TaskStatus::Assigned,
        actor,
        agent_id: Some(assignee.id.to_string()),
    };
    let ask = TransitionAsk {
        assignee_id: Some(assignee.id.to_string()),
        reviewer_id: reviewer,
        ..TransitionAsk::default()
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let context = deps.tools.transitions.context(&request, &ask, team)?;
    if !context.assignment.is_some_and(|assignment| {
        !assignment.private_folder_busy
            && assignment.dependencies.len() == contract.dependencies.len()
            && assignment
                .dependencies
                .iter()
                .all(|state| state.status == TaskStatus::Accepted)
    }) {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(&request, &ask, team)?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("assigned the epic to {}", assignee.id.as_str()),
        }),
        TransitionOutcome::Refused(_) => None,
    })
}

/// Rule 7 for an epic `assigned`: moved to `in_progress` as its assignee asks, with no worktree,
/// since an epic has no branch of its own.
pub(super) fn assigned_epic(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    if refused_since_entering(
        deps,
        &row.task_id,
        TaskStatus::Assigned,
        TaskStatus::InProgress,
    )? {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(
        &catervas_core::governor::transition::TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::InProgress,
            actor: catervas_core::governor::transition_table::TransitionActor::Assignee,
            agent_id: Some(assignee.id.to_string()),
        },
        &TransitionAsk::default(),
        team,
    )?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("started the epic for {}", assignee.id.as_str()),
        }),
        TransitionOutcome::Refused(_) => None,
    })
}

/// What a task under an epic says of the epic's progress.
struct Child {
    task_id: TaskId,
    status: TaskStatus,
    /// Its `allowed_paths` are exactly its `plan`: it writes the epic's plan.
    plan_task: bool,
}

/// What an epic needs next, read from its tasks.
#[derive(Debug, PartialEq, Eq)]
enum EpicStep {
    Nothing,
    BreakDown,
    WriteThePlansTasks,
    CloseOut,
}

/// Every task cancelled, or none: break the epic down. Any task neither accepted nor cancelled:
/// nothing, its tasks being worked. When every task not cancelled is a plan task, the Architect's
/// plan is written: its tasks come next once the plan task is accepted, integrated or not
/// (ADR 0061). Otherwise the epic's work is done and it is closed out.
fn epic_step(children: &[Child]) -> EpicStep {
    let live = || {
        children
            .iter()
            .filter(|c| c.status != TaskStatus::Cancelled)
    };
    if live().next().is_none() {
        EpicStep::BreakDown
    } else if live().any(|c| c.status != TaskStatus::Accepted) {
        EpicStep::Nothing
    } else if !live().all(|c| c.plan_task) {
        EpicStep::CloseOut
    } else {
        EpicStep::WriteThePlansTasks
    }
}

/// The branches a task's worktree is made from, the first being its base (ADR 0061): each
/// dependency that is accepted and still awaiting integration, in the order the contract lists
/// them, then, for a task of an epic's plan, the plan task's branch while the plan is not on the
/// integration branch. A branch that does not exist, or that is listed twice, is left out. With
/// nothing unmerged to start from, the integration branch.
///
/// # Errors
///
/// What reading a contract or git refuses.
pub(super) fn start_point(
    deps: &OrchestratorDeps,
    board: &[TaskProjection],
    contract: &TaskContract,
    repository: &TaskRepository,
) -> Result<Vec<String>, OrchestratorError> {
    let mine = |theirs: &TaskContract| theirs.repo == contract.repo;
    let mut branches = Vec::new();
    for dependency in &contract.dependencies {
        let Some(row) = board
            .iter()
            .find(|row| row.task_id.as_str() == dependency.as_str())
        else {
            continue;
        };
        if row.status != TaskStatus::Accepted || !row.awaiting_integration {
            continue;
        }
        let theirs = match deps.tools.files.read_contract(&row.task_id) {
            Ok(theirs) => theirs,
            Err(FilesError::NotFound { .. }) => continue,
            Err(error) => return Err(error.into()),
        };
        if task_private_folder(&theirs).is_none() && mine(&theirs) {
            branches.push(task_branch(&theirs));
        }
    }
    if let (Some(plan), Some(parent)) = (&contract.plan, &contract.parent)
        && !is_plan_task(contract)
    {
        let mut plan_branch = None;
        for row in board.iter().filter(|row| {
            row.parent
                .as_ref()
                .is_some_and(|own| own.to_string() == parent.to_string())
                && row.status == TaskStatus::Accepted
        }) {
            let theirs = match deps.tools.files.read_contract(&row.task_id) {
                Ok(theirs) => theirs,
                Err(FilesError::NotFound { .. }) => continue,
                Err(error) => return Err(error.into()),
            };
            if is_plan_task(&theirs) && theirs.plan.as_ref() == Some(plan) && mine(&theirs) {
                plan_branch = Some(task_branch(&theirs));
            }
        }
        if let Some(branch) = plan_branch {
            let on_the_branch = repository
                .git
                .has_file(&repository.integration_branch, &plan.to_string())
                .unwrap_or(false);
            if !on_the_branch {
                branches.push(branch);
            }
        }
    }
    let mut starts: Vec<String> = Vec::new();
    for branch in branches {
        if !starts.contains(&branch) && repository.git.has_local_branch(&branch)? {
            starts.push(branch);
        }
    }
    if starts.is_empty() {
        starts.push(repository.integration_branch.clone());
    }
    Ok(starts)
}

/// What each of `tasks`, the tasks under an epic, says of the epic's progress.
fn children_of_epic(
    deps: &OrchestratorDeps,
    tasks: &[&TaskProjection],
) -> Result<Vec<Child>, OrchestratorError> {
    tasks
        .iter()
        .map(|task| {
            let plan_task = match deps.tools.files.read_contract(&task.task_id) {
                Ok(theirs) => is_plan_task(&theirs),
                Err(FilesError::NotFound { .. }) => false,
                Err(error) => return Err(error.into()),
            };
            Ok(Child {
                task_id: task.task_id.clone(),
                status: task.status,
                plan_task,
            })
        })
        .collect()
}

/// The plan at `stem` on `branch` of `git` and its text, in the format it was written in. A branch
/// that does not exist holds no plan.
fn plan_on(
    git: &catervas_store::Git,
    branch: &str,
    stem: &str,
    team: &Team,
) -> Result<Option<(String, String)>, OrchestratorError> {
    match held_document(git, branch, stem, team.document_format()) {
        Ok(Some(plan)) => {
            let text = git.file_at(branch, &plan)?;
            Ok(Some((plan, text)))
        }
        Ok(None) | Err(GitError::CommandFailed { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// The Product Manager's message for an epic that is to be broken down, or whose plan is to be
/// written out as tasks (`WriteThePlansTasks`): once the plan is on the integration branch, its
/// tasks; with no plan there, the plan task again with an active Architect, else the tasks in
/// lanes.
fn product_managers_message(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    step: &EpicStep,
    listed: &[(String, String, String)],
    plan_task: Option<&TaskId>,
) -> Result<String, OrchestratorError> {
    let unextended = plan_stem(contract.id.as_str());
    if *step == EpicStep::WriteThePlansTasks {
        let git = &deps.tools.git;
        let branch = integration_branch(team, git)?;
        // The plan is read in the format it was written in: on the integration branch, else on the
        // branch of the accepted plan task that has not been integrated yet.
        if let Some(found) = plan_on(git, &branch, &unextended, team)? {
            return Ok(plan_tasks_message(contract, &found.0, &found.1, listed));
        }
        if let Some(plan_task) = plan_task
            && let Ok(theirs) = deps.tools.files.read_contract(plan_task)
            && let Ok(repository) = task_repository_of(deps, team, &theirs)
            && let Some(found) = plan_on(&repository.git, &task_branch(&theirs), &unextended, team)?
        {
            return Ok(plan_tasks_message(contract, &found.0, &found.1, listed));
        }
    }
    // A new plan is named in the team's format.
    let plan = with_format(&unextended, team.document_format());
    let builders = team
        .active_agents()
        .filter(|agent| changes_code(Role::from(agent.role)))
        .count();
    Ok(if team.has_active(Role::Architect) {
        plan_task_message(contract, &plan, builders)
    } else {
        breakdown_message(contract, builders)
    })
}

/// Rule 6 for an epic `in_progress`, by `epic_step`: the Product Manager's plan session to break
/// it down (with an active Architect, to file the task that writes its plan; without one, its
/// tasks in lanes), or to write the plan's tasks once the plan is on the integration branch; its
/// assignee's plan session to close it out when every task under it is done; otherwise no rule,
/// its tasks being worked.
pub(super) async fn in_progress_epic(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    let tasks: Vec<&TaskProjection> = board
        .iter()
        .filter(|other| other.parent.as_ref() == Some(&row.task_id))
        .collect();
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    // The Catervafication's epic is broken down by Catervas: it files each role's task from its
    // template, with no session, and is never broken down by the Product Manager.
    let catervafication = catervafication_of(&history(deps, &row.task_id)?);
    if let Some(project) = catervafication {
        match file_due(&deps.tools, team, board, row, project).map_err(|error| {
            OrchestratorError::Refused {
                reason: format!("catervafication_failed: {error}"),
            }
        })? {
            FileDue::Filed(filed) => {
                let ids: Vec<&str> = filed.iter().map(|id| id.as_str()).collect();
                return Ok(Some(TickReport::Acted {
                    task_id: row.task_id.clone(),
                    what: format!("filed the Catervafication's tasks: {}", ids.join(", ")),
                }));
            }
            FileDue::Nothing => {}
        }
    }
    let children = children_of_epic(deps, &tasks)?;
    let listed: Vec<(String, String, String)> = tasks
        .iter()
        .map(|other| {
            (
                other.task_id.to_string(),
                other.title.clone(),
                other.status.to_string(),
            )
        })
        .collect();
    let (agent, initial_prompt) = match epic_step(&children) {
        EpicStep::Nothing => return Ok(None),
        // Catervas files this epic's tasks itself, so it has no breakdown for a session to make.
        EpicStep::BreakDown if catervafication.is_some() => return Ok(None),
        EpicStep::CloseOut => {
            let rejection = last_rejection(deps, &row.task_id, &tasks)?;
            (
                assignee,
                close_out_message(
                    &contract,
                    &listed,
                    rejection
                        .as_ref()
                        .map(|(failed, reasons)| (failed.as_slice(), reasons.as_str())),
                ),
            )
        }
        step => {
            let Some(pm) = product_manager(team) else {
                return Ok(None);
            };
            (
                pm,
                product_managers_message(
                    deps,
                    team,
                    &contract,
                    &step,
                    &listed,
                    children
                        .iter()
                        .rfind(|child| child.plan_task && child.status != TaskStatus::Cancelled)
                        .map(|child| &child.task_id),
                )?,
            )
        }
    };
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | unavailable(deps, agent, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent,
            contract: Some(&contract),
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            only_tool: None,
            tools: None,
            in_reply_to: None,
            thread: None,
            initial_prompt,
        },
    )
    .await?;
    Ok(Some(acted(row, agent, "plan", &end)))
}

/// The failed criteria and the reasons, the review note among them, of the epic's last move into
/// `rejected`, while none of `tasks`, the tasks under it, was created after that move: once the
/// fixing tasks are filed, the rejection is answered.
fn last_rejection(
    deps: &OrchestratorDeps,
    epic: &TaskId,
    tasks: &[&TaskProjection],
) -> Result<Option<(Vec<String>, String)>, OrchestratorError> {
    let epic_history = history(deps, epic)?;
    let Some(moved) = last_move_into(&epic_history, TaskStatus::Rejected) else {
        return Ok(None);
    };
    for task in tasks {
        if history(deps, &task.task_id)?.iter().any(|event| {
            event.envelope.seq > moved.envelope.seq && event.body.kind() == EventKind::TaskCreated
        }) {
            return Ok(None);
        }
    }
    Ok(match &moved.body {
        EventBody::TaskTransitioned(body) => body.rejection.as_ref().map(|rejection| {
            (
                rejection.failed_criterion_ids.clone(),
                rejection.reasons.clone(),
            )
        }),
        _ => None,
    })
}

/// The assigner of a task under an epic: the epic's assignee, when it is active.
pub(super) fn epic_assignee<'a>(
    team: &'a Team,
    board: &[TaskProjection],
    row: &TaskProjection,
) -> Option<&'a Agent> {
    let parent = row.parent.as_ref()?;
    let epic = board.iter().find(|other| &other.task_id == parent)?;
    active(team, epic.assignee_id.as_deref())
}

/// Rule 5 for an epic (ADR 0013): nothing while a task under it awaits integration; then Catervas
/// runs each mechanical criterion it has not run in this verification, on the integration branch's
/// head. Then, for an epic whose row names a reviewer (the Product Manager, fixed when the Scrum
/// Master was assigned it, whatever the team is now), its review; for one the human reviews,
/// nothing until the human accepts, then the Product Manager's `verify` session, told the human
/// accepted, after which the epic's review is recorded.
pub(super) async fn verifying_epic(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    contract: &TaskContract,
    history: &[CatervasEvent],
    since: u64,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let board = deps.tools.projections.board()?;
    if board
        .iter()
        .any(|other| other.parent.as_ref() == Some(&row.task_id) && other.awaiting_integration)
    {
        return Ok(None);
    }
    let ran = governor_results(history, since);
    let pending: Vec<ExitCriterion> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| is_mechanical(criterion))
        .filter(|criterion| {
            !ran.iter()
                .any(|result| result.criterion_id == criterion.id.as_str())
        })
        .cloned()
        .collect();
    if !pending.is_empty() {
        return match run_on_the_integration_branch(orchestrator, team, contract, pending).await? {
            EpicRan::Criteria(count) => Ok(ran_criteria(row, count)),
            EpicRan::Unrunnable(why) => escalate(deps, team, row, &why),
        };
    }
    // An epic whose every criterion Catervas ran is accepted or rejected by the governor, with no
    // Product Manager's session and no human (ADR 0061).
    if catervas_verifies_alone(contract) {
        return decide_alone(deps, team, row, contract, 0);
    }
    if !the_human_reviews_the_epic(contract.kind, row.reviewer_id.as_deref()) {
        return reviewed_by_the_product_manager(
            deps, team, &board, row, contract, history, waiting,
        )
        .await;
    }
    if result_accepted(history).is_none() {
        return Ok(None);
    }
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    if spent(deps, team, contract, &mut waiting.day_spent)?
        | unavailable(deps, pm, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let end = run_session(
        deps,
        team,
        read_only(
            contract,
            pm,
            deps.tools.files.root().to_path_buf(),
            epic_accept_message(contract, &ran),
        ),
    )
    .await?;
    record_epic_review(deps, team, contract, &end.session_id, since)?;
    Ok(Some(acted(row, pm, "verify", &end)))
}

/// Rule 5 for an epic the Product Manager reviews, once Catervas's runs are recorded, judged on the
/// governor's own context for `verifying -> accepted`: the Product Manager's read-only `verify`
/// session in the project root while there is no review note, after which its review is recorded;
/// the rejection in its name when its results hold a failure; its review session again, told
/// which, while a criterion is unanswered; nothing until the human accepts; then its session
/// again, told the human accepted.
async fn reviewed_by_the_product_manager(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
    contract: &TaskContract,
    history: &[CatervasEvent],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = active(team, row.reviewer_id.as_deref()) else {
        return Ok(None);
    };
    let context = context(deps, team, &row.task_id)?;
    let answers = reviewer_results(&context);
    let review_note = context.done.review_note.clone();
    // A review to run, naming what is still unanswered, or none when the review is complete.
    let to_review = match &review_note {
        None => Some(Vec::new()),
        Some(review_note) => {
            let failed = failed(contract, &answers);
            if !failed.is_empty() {
                return reject(deps, team, row, pm, &failed, review_note.clone()).map(Some);
            }
            let unanswered = unanswered(contract, &answers);
            if unanswered.is_empty() && result_accepted(history).is_none() {
                return Ok(None);
            }
            (!unanswered.is_empty()).then_some(unanswered)
        }
    };
    if spent(deps, team, contract, &mut waiting.day_spent)?
        | unavailable(deps, pm, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let since = since_verifying(history);
    let ran = governor_results(history, since);
    let initial_prompt = match &to_review {
        Some(unanswered) => {
            epic_review_message(contract, &ran, &tasks_under(deps, board, row)?, unanswered)
        }
        None => epic_accept_message(contract, &ran),
    };
    let end = run_session(
        deps,
        team,
        read_only(
            contract,
            pm,
            deps.tools.files.root().to_path_buf(),
            initial_prompt,
        ),
    )
    .await?;
    if to_review.is_some() {
        record_review(deps, team, contract, pm, &end.session_id, since)?;
    }
    Ok(Some(acted(row, pm, "verify", &end)))
}

/// Each task under the epic, with its last completion note when it has one.
fn tasks_under(
    deps: &OrchestratorDeps,
    board: &[TaskProjection],
    row: &TaskProjection,
) -> Result<Vec<(TaskProjection, Option<String>)>, OrchestratorError> {
    board
        .iter()
        .filter(|other| other.parent.as_ref() == Some(&row.task_id))
        .map(|other| {
            let note = history(deps, &other.task_id)?
                .iter()
                .rev()
                .find_map(|event| match &event.body {
                    EventBody::NoteWritten(body)
                        if body.kind == NoteWrittenBodyKind::Completion =>
                    {
                        Some(body.text.clone())
                    }
                    _ => None,
                });
            Ok((other.clone(), note))
        })
        .collect()
}

/// What Catervas's runs of an epic's criteria came to.
enum EpicRan {
    /// This many criteria were run and recorded.
    Criteria(usize),
    /// One could not be run, for a reason that is not the work's, in these words.
    Unrunnable(String),
}

/// Runs `pending` one at a time on a detached worktree `<id>-base` at the integration branch's
/// head (any left over removed first), in a sandbox from `create_base` with the network off, each
/// result recorded as the reviewer's run, recorded by the governor, before the next one runs, its
/// evidence opening with the sha it ran at. A `test` criterion's copy has `new_tests_required`
/// cleared, since an epic has no branch to compare (ADR 0013). The sandbox is discarded and the
/// worktree removed on every way out.
async fn run_on_the_integration_branch(
    orchestrator: &Orchestrator,
    team: &Team,
    contract: &TaskContract,
    pending: Vec<ExitCriterion>,
) -> Result<EpicRan, OrchestratorError> {
    let deps = &orchestrator.deps;
    let git = &deps.tools.git;
    let into = integration_branch(team, git)?;
    let sha = git.merge_base(&into, &into)?;
    let tools = Arc::clone(&deps.tools);
    let factory = Arc::clone(&deps.sandboxes);
    let contract = contract.clone();
    tokio::task::spawn_blocking(move || -> Result<EpicRan, OrchestratorError> {
        let git = &tools.git;
        let worktree = tools
            .files
            .root()
            .join(".catervas/local/worktrees")
            .join(format!("{}-base", contract.id.as_str()));
        let unrunnable = |id: &str, error: &dyn std::fmt::Display| {
            EpicRan::Unrunnable(format!(
                "Catervas could not run {id} on the integration branch for its reviewer: {error}"
            ))
        };
        if let Err(error) = remove_base_worktree(git, &worktree).and_then(|()| {
            git.create_detached_worktree(&worktree, &sha)
                .map_err(Into::into)
        }) {
            return Ok(unrunnable(pending[0].id.as_str(), &error));
        }
        let sandbox = match factory.create_base(&tools.ids.project_id, &contract.id, &worktree) {
            Ok(sandbox) => sandbox,
            Err(error) => {
                let _ = remove_base_worktree(git, &worktree);
                return Err(error.into());
            }
        };
        let mut count = 0;
        let mut outcome = Ok(());
        for criterion in &pending {
            let mut alone = contract.clone();
            let mut copy = criterion.clone();
            if let VerificationWire::Variant1 {
                new_tests_required, ..
            } = &mut copy.verification
            {
                *new_tests_required = false;
            }
            alone.exit_criteria = vec![copy];
            let result = match run_criteria(&alone, sandbox.as_ref(), RunBy::Reviewer, None) {
                Ok(outcomes) => outcomes.into_iter().find_map(|outcome| match outcome {
                    CriterionOutcome::Result(result) => Some(result),
                    _ => None,
                }),
                Err(error) if fails_the_criterion(&error) => Some(CriterionResult {
                    criterion_id: criterion.id.to_string(),
                    passed: false,
                    evidence: error.to_string(),
                    run_by: RunBy::Reviewer,
                }),
                Err(error) => {
                    outcome = Err(unrunnable(criterion.id.as_str(), &error));
                    break;
                }
            };
            if let Some(result) = result {
                if let Err(error) = record_governor_result(&tools, &contract.id, &sha, result) {
                    let _ = sandbox.discard();
                    let _ = remove_base_worktree(git, &worktree);
                    return Err(error);
                }
                count += 1;
            }
        }
        let discarded = sandbox.discard();
        let removed = remove_base_worktree(git, &worktree);
        if let Err(unrunnable) = outcome {
            return Ok(unrunnable);
        }
        discarded?;
        removed.map_err(|error| {
            OrchestratorError::Files(FilesError::Io {
                path: worktree.display().to_string(),
                detail: error.to_string(),
            })
        })?;
        Ok(EpicRan::Criteria(count))
    })
    .await
    .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))
}

/// Records one of Catervas's runs of an epic's criterion as the reviewer's, recorded by the governor,
/// with no agent on its envelope, its evidence opening with the sha it ran at.
fn record_governor_result(
    tools: &ToolDeps,
    task_id: &TaskId,
    sha: &str,
    result: CriterionResult,
) -> Result<(), OrchestratorError> {
    append(
        tools,
        task_id,
        None,
        None,
        EventBody::CriterionRecorded(CriterionRecordedBody {
            criterion_id: result.criterion_id,
            passed: result.passed,
            evidence: format!("at {sha}\n{}", result.evidence),
            run_by: CriterionRecordedBodyRunBy::Reviewer,
            recorded_by: GOVERNOR.to_string(),
        }),
    )
}

/// The epic's `review.recorded`, once per verification, with the human as reviewer: the criteria
/// with a reviewer's or the human's result, all passed, since the human accepted only once
/// Catervas's runs passed. The one review not recorded at a reviewer session's end.
fn record_epic_review(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    session_id: &str,
    since: u64,
) -> Result<(), OrchestratorError> {
    let history = history(deps, &contract.id)?;
    if history.iter().any(|event| {
        event.envelope.seq > since && matches!(event.body, EventBody::ReviewRecorded(_))
    }) {
        return Ok(());
    }
    let context = context(deps, team, &contract.id)?;
    let run = contract
        .exit_criteria
        .iter()
        .filter(|criterion| {
            context.done.results.iter().any(|result| {
                result.criterion_id == criterion.id.as_str()
                    && matches!(result.run_by, RunBy::Reviewer | RunBy::Human)
            })
        })
        .count();
    append(
        &deps.tools,
        &contract.id,
        None,
        Some(session_id.to_string()),
        EventBody::ReviewRecorded(ReviewRecordedBody {
            reviewer: HUMAN.to_string(),
            criteria_run: u32::try_from(run).unwrap_or(u32::MAX),
            passed: true,
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use catervas_core::contract::{TaskKind, TaskStatus};
    use catervas_core::team::{AgentStatus, Effort};
    use catervas_protocol::command::{AcceptSubject, Command, RequestSize};
    use catervas_protocol::event::{
        CatervasEvent, CriterionRecordedBodyRunBy, EscalationRaisedBodyReason, EventBody,
        EventKind, NewEvent, NoteWrittenBodyKind, RequestTriagedBodySize, TransitionActorWire,
        event_from_value,
    };
    use catervas_store::git::fixtures::git_output_in;
    use serde_json::{Value, json};

    use crate::exec::ExecError;
    use crate::orchestrator::TRIAGE_MODEL;
    use crate::orchestrator::fixtures::{
        BrokenSandboxFactory, CountingSandboxFactory, ExecutorWitness, Harness, waits_for,
    };
    use crate::orchestrator::{CommandError, Orchestrator, TickReport};
    use crate::recorded::fixtures::{
        accept_ctv_1, chat_answers_with_a_request, implement_finishes_ctv_1, plan_assigns_ctv_1,
        plan_assigns_ctv_2, plan_breaks_down_ctv_1, plan_closes_epic_ctv_1, refine_asks_ctv_1,
        refine_writes_epic_ctv_1, refine_writes_task_ctv_1, replays_catervas_read_board,
        review_epic_ctv_1, review_epic_fails_ctv_1, triage_by_sm_ctv_1, triage_ctv_1_large,
    };
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;

    #[test]
    fn reads_an_epics_next_step() {
        use super::{
            Child,
            EpicStep::{BreakDown, CloseOut, Nothing, WriteThePlansTasks},
            epic_step,
        };
        let child = |status, plan_task| Child {
            task_id: "CTV-2".parse().expect("a task id"),
            status,
            plan_task,
        };
        let plan_task = |status| child(status, true);
        let built = |status| child(status, false);
        assert_eq!(epic_step(&[]), BreakDown);
        assert_eq!(epic_step(&[plan_task(TaskStatus::Cancelled)]), BreakDown);
        assert_eq!(epic_step(&[plan_task(TaskStatus::InProgress)]), Nothing);
        assert_eq!(
            epic_step(&[plan_task(TaskStatus::Accepted)]),
            WriteThePlansTasks
        );
        assert_eq!(
            epic_step(&[
                plan_task(TaskStatus::Cancelled),
                plan_task(TaskStatus::Accepted)
            ]),
            WriteThePlansTasks
        );
        assert_eq!(
            epic_step(&[plan_task(TaskStatus::Accepted), built(TaskStatus::Accepted)]),
            CloseOut
        );
        assert_eq!(epic_step(&[built(TaskStatus::Accepted)]), CloseOut);
        // A cancelled task that is no plan task does not stop the plan's tasks from being written.
        assert_eq!(
            epic_step(&[
                plan_task(TaskStatus::Accepted),
                built(TaskStatus::Cancelled)
            ]),
            WriteThePlansTasks
        );
        assert_eq!(
            epic_step(&[plan_task(TaskStatus::Accepted), built(TaskStatus::Ready)]),
            Nothing
        );
    }

    fn task(id: &str) -> catervas_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    fn last(harness: &Harness, kind: EventKind) -> Option<CatervasEvent> {
        harness.events(&[kind]).pop()
    }

    fn is_idle(report: &TickReport) -> bool {
        matches!(report, TickReport::Idle { .. })
    }

    /// Each move of `id`, as `from -> to`, in order.
    fn moves_of(harness: &Harness, id: &str) -> Vec<String> {
        harness
            .events(&[EventKind::TaskTransitioned])
            .iter()
            .filter(|event| event.envelope.ids.task_id == Some(task(id)))
            .map(|event| match &event.body {
                EventBody::TaskTransitioned(body) => format!("{} -> {}", body.from, body.to),
                other => panic!("a move, got {other:?}"),
            })
            .collect()
    }

    /// A request filed by the human and sized by them, `size`, as `catervas triage` does.
    async fn a_sized_request(harness: &Harness, orchestrator: &Orchestrator, size: RequestSize) {
        harness.a_request("Add done.txt and its check");
        orchestrator
            .handle(Command::RequestTriage {
                task_id: task("CTV-1"),
                size,
                reason: "Sized by the human.".to_string(),
            })
            .await
            .expect("the human sizes a draft");
    }

    /// A request sized `size` and moved to `refining` by a tick.
    async fn refining(
        harness: &Harness,
        orchestrator: &Orchestrator,
        size: RequestSize,
    ) -> TickReport {
        a_sized_request(harness, orchestrator, size).await;
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Refining);
        report
    }

    /// Calls a Catervas tool as `agent` on `task`, as a session of theirs would.
    async fn call(
        harness: &Harness,
        agent: &str,
        task: Option<&str>,
        tool: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::tools::ToolError> {
        crate::tools::call_tool(&harness.project.context(agent, task), tool, input).await
    }

    /// The failing write the tests seed: a budget of 50 dollars, over a cap of 5 dollars where a
    /// test sets one.
    async fn write_over_the_cap(harness: &Harness) {
        call(
            harness,
            "pm",
            Some("CTV-1"),
            "catervas_write_contract",
            json!({ "fields": { "budget": { "max_cost_usd": 50 } } }),
        )
        .await
        .expect("the schema allows the write");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_given_agent_refines_its_own_task() {
        use crate::orchestrator::messages::GIVEN_TASK_LEAD;
        use crate::recorded::fixtures::implement_stops_early;

        let harness = Harness::with_the_suggested_six("req-given-refine");
        harness.file_given("CTV-1", "refining", "dev", |_| {});
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let who: Vec<_> = started
            .iter()
            .map(|spec| (spec.agent_id.as_str(), spec.purpose))
            .collect();
        assert_eq!(who, [("dev", SessionPurpose::Refine)]);
        let lead = GIVEN_TASK_LEAD
            .replace("{name}", "dev")
            .replace("{role}", "software_developer");
        assert!(
            started[0].initial_prompt.starts_with(&lead),
            "{}",
            started[0].initial_prompt
        );

        // A paused agent's task waits: no session starts.
        let harness = Harness::with_the_suggested_six("req-given-refine-paused");
        harness.file_given("CTV-1", "refining", "dev", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(vec![implement_stops_early()]));
        orchestrator
            .handle(Command::AgentUpdate {
                agent_id: "dev".to_string(),
                status: AgentStatus::Paused,
            })
            .await
            .expect("the agent is paused");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn triages_a_request_on_the_cheaper_model() {
        let harness = Harness::new("req-triage", |_| {});
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![triage_ctv_1_large()]);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());

        orchestrator.tick().await.expect("the tick runs");
        // The daemon holds the session to its one tool, whatever the agent's tiers allow.
        assert_eq!(
            witness.given_tools(),
            vec![vec!["catervas_triage_request".to_string()]]
        );

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Triage);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.model, TRIAGE_MODEL);
        assert_eq!(spec.effort, Effort::Low);
        assert_eq!(
            spec.catervas_tools,
            vec!["catervas_triage_request".to_string()]
        );
        assert_eq!(
            listed_tools(&spec.system_prompt),
            vec!["catervas_triage_request"]
        );
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
        let triaged = last(&harness, EventKind::RequestTriaged).expect("the triage");
        let EventBody::RequestTriaged(body) = &triaged.body else {
            panic!("a triage");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Large);
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(harness.row("CTV-1").kind, TaskKind::Epic);
    }

    /// The Catervas tools the prompt's `Your tools` section lists, in order.
    fn listed_tools(prompt: &str) -> Vec<&str> {
        let start = prompt.find("## Your tools\n").expect("a tools section");
        let section = &prompt[start + 1..];
        let section = &section[..section.find("\n## ").unwrap_or(section.len())];
        section
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .filter_map(|line| line.split(' ').next())
            .filter(|name| name.starts_with("catervas_"))
            .collect()
    }

    /// Adds the active Scrum Master `sam` to the team's wire.
    fn with_a_scrum_master(wire: &mut Value) {
        wire["agents"]
            .as_array_mut()
            .expect("a list of agents")
            .push(json!({
                "id": "sam",
                "display_name": "sam",
                "role": "scrum_master",
                "status": "active"
            }));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn readies_a_written_contract_without_a_judgment() {
        let harness = Harness::new("req-no-judgment", |wire| {
            with_a_scrum_master(wire);
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(catervas_core::team::fixtures::an_agent_wire(
                    "ari",
                    "architect",
                ));
            wire["policy"]["judgment"] = json!({
                "required": "always",
                "questions": ["Is the plan small enough to review?"],
                "judge": "architect"
            });
        });
        let adapter = harness.recorded(vec![refine_writes_task_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started().len(), 1);
        assert!(last(&harness, EventKind::ContractJudged).is_none());
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Ready);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn triages_with_the_scrum_master_when_there_is_one() {
        let harness = Harness::new("req-triage-sm", with_a_scrum_master);
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![triage_by_sm_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Triage);
        assert_eq!(spec.agent_id, "sam");
        assert_eq!(spec.model, TRIAGE_MODEL);
        assert_eq!(
            spec.catervas_tools,
            vec!["catervas_triage_request".to_string()]
        );
        assert_eq!(
            listed_tools(&spec.system_prompt),
            vec!["catervas_triage_request"]
        );
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
        let triaged = last(&harness, EventKind::RequestTriaged).expect("the triage");
        let EventBody::RequestTriaged(body) = &triaged.body else {
            panic!("a triage");
        };
        assert_eq!(body.triaged_by, "sam");
        assert_eq!(body.size, RequestTriagedBodySize::Small);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_refining_a_triaged_request_without_a_session() {
        let harness = Harness::new("req-draft-to-refining", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        refining(&harness, &orchestrator, RequestSize::Large).await;

        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("draft".to_string(), "refining".to_string())
        );
        assert_eq!(body.actor, TransitionActorWire::ProductManager);
        assert_eq!(body.requested_by, "pm");
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn holds_a_new_projects_contracts_until_the_plan() {
        let harness = Harness::new("req-new-project-holds", |_| {});
        harness.started();
        let adapter = harness.recorded(vec![
            chat_answers_with_a_request(),
            refine_writes_task_ctv_1(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        a_sized_request(&harness, &orchestrator, RequestSize::Small).await;
        // The first tick opens the interview (its one session); the next triages and refines.
        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Refining);
        assert_eq!(adapter.started().len(), 1);
        let watched = [EventKind::ContractEvaluated, EventKind::TransitionRefused];
        let before = harness.events(&watched).len();

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert_eq!(adapter.started().len(), 1, "{:?}", adapter.started());
        assert_eq!(harness.events(&watched).len(), before);
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Refining);

        harness.approve_the_product_plan();
        // The new project's Catervafication starts first (4.1), with no session; the next tick
        // refines the request against the plan.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            harness.events(&[EventKind::CatervaficationStarted]).len(),
            1
        );
        assert_eq!(adapter.started().len(), 1);
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 2);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_the_product_plan_task_as_filed_and_sends_a_refused_one_to_the_product_manager()
    {
        // No Architect on the team, so the governor refuses the filed contract's reviewer.
        let harness = Harness::new("req-product-plan-filed", |_| {});
        harness.started();
        let deps = &harness.project.deps;
        crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "pm".to_string(),
                author: "pm".to_string(),
                text: "Hello! Who are your customers?".to_string(),
                in_reply_to: None,
                request: None,
                heard: None,
                session_id: None,
                as_task: false,
            },
        )
        .expect("the greeting is recorded");
        catervas_store::requests::file_product_plan_request(
            &deps.files,
            &deps.log,
            catervas_store::requests::product_plan_request(
                catervas_core::contract::Role::Architect,
                5.0,
                catervas_core::folders::DocumentFormat::Html,
            ),
            (at(), &deps.ids),
        )
        .expect("the owner's word files it");
        let adapter = harness.recorded(vec![refine_writes_task_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");
        // Judged as filed: the governor was asked, refused, and no session ran.
        assert!(adapter.started().is_empty());
        // The governor was asked for `escalated` first, then for `ready`, and refused both.
        assert_eq!(harness.events(&[EventKind::TransitionRefused]).len(), 2);
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Refining);

        // Refused, it goes to the Product Manager as any contract does.
        orchestrator.tick().await.expect("the tick runs");
        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            (started[0].agent_id.as_str(), started[0].purpose),
            ("pm", SessionPurpose::Refine)
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn acts_for_no_agent_that_is_paused() {
        // A team always has an active Product Manager; the one that acts is the first active one.
        let harness = Harness::new("req-paused", |wire| {
            wire["agents"][0]["status"] = json!("paused");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(json!({
                    "id": "pm-2",
                    "display_name": "pm-2",
                    "role": "product_manager",
                    "status": "active"
                }));
        });
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        a_sized_request(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the tick runs");
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        assert!(matches!(
            &moved.body,
            EventBody::TaskTransitioned(body) if body.requested_by == "pm-2"
        ));

        let harness = Harness::new("req-paused-assignee", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        harness.assigned("CTV-2", "dev-a", "dev-b");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Assigned);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_first_when_refining_an_epic() {
        let harness = Harness::new("req-asks-first", |_| {});
        let adapter = harness.recorded(vec![refine_asks_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Refine);
        assert!(
            started[0].initial_prompt.contains("catervas_ask_human"),
            "{}",
            started[0].initial_prompt
        );
        assert!(last(&harness, EventKind::QuestionAsked).is_some());
        assert!(harness.row("CTV-1").waiting_on_human);
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_the_answer_to_the_next_refine_session() {
        let harness = Harness::new("req-answer-handed", |_| {});
        let adapter = harness.recorded(vec![
            refine_asks_ctv_1(),
            replays_catervas_read_board(),
            replays_catervas_read_board(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the question is asked");
        let n = last(&harness, EventKind::QuestionAsked)
            .expect("the question")
            .envelope
            .seq;
        orchestrator
            .handle(Command::QuestionAnswer {
                question_id: n,
                answer: "Yes.".to_string(),
            })
            .await
            .expect("the human answers");

        orchestrator.tick().await.expect("the tick runs");
        let prompt = adapter.started()[1].system_prompt.clone();
        let question = prompt
            .find(&format!("Question {n}:"))
            .unwrap_or_else(|| panic!("the question's id: {prompt}"));
        let wrapped = prompt
            .find("<untrusted source=\"question\">")
            .expect("the question as the agent's words");
        let asked = prompt
            .find("Should done.txt be empty?")
            .expect("the question");
        let answer = prompt.find("Answer: Yes.").expect("the answer, unwrapped");
        assert!(
            question < wrapped && wrapped < asked && asked < answer,
            "{prompt}"
        );
        // Its questions asked, the epic's refine session is not told to ask first again.
        let first = &adapter.started()[1].initial_prompt;
        assert!(!first.contains("ask the user every question"), "{first}");

        orchestrator.tick().await.expect("the tick runs");
        let prompt = adapter.started()[2].system_prompt.clone();
        assert_eq!(adapter.started()[2].purpose, SessionPurpose::Refine);
        assert!(!prompt.contains(&format!("Question {n}:")), "{prompt}");
        assert!(!prompt.contains("Answer: Yes."), "{prompt}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_written_contract_before_refining_again() {
        let harness = Harness::new("req-judges", |_| {});
        let adapter = harness.recorded(vec![refine_writes_task_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");
        assert!(last(&harness, EventKind::ContractWritten).is_some());

        orchestrator.tick().await.expect("the tick runs");

        let evaluated = last(&harness, EventKind::ContractEvaluated).expect("the judgement");
        assert!(
            matches!(&evaluated.body, EventBody::ContractEvaluated(body) if body.passed),
            "{evaluated:?}"
        );
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (
                body.from.to_string(),
                body.to.to_string(),
                body.requested_by.as_str()
            ),
            ("refining".to_string(), "ready".to_string(), "governor")
        );
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_structurally_broken_contract_without_a_session() {
        let harness = Harness::new("req-sm-broken", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        call(
            &harness,
            "pm",
            Some("CTV-1"),
            "catervas_write_contract",
            json!({ "fields": { "scope": { "in_scope": ["done.txt"], "out_of_scope": [" "] } } }),
        )
        .await
        .expect("the schema allows the write");

        orchestrator.tick().await.expect("the tick runs");

        assert!(adapter.started().is_empty());
        let refused = last(&harness, EventKind::TransitionRefused).expect("the refusal");
        let EventBody::TransitionRefused(body) = &refused.body else {
            panic!("a refusal");
        };
        assert!(
            body.details
                .iter()
                .any(|detail| detail.contains("out_of_scope")),
            "{:?}",
            body.details
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn readies_a_written_contract_whatever_the_day_cost_without_a_daily_budget() {
        let harness = Harness::new("req-no-day", |wire| wire["budgets"] = json!({}));
        harness.spent(None, "s-0", 25.0);
        let adapter = harness.recorded(vec![refine_writes_task_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Ready);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refines_a_contract_larger_than_the_open_sprints_remainder_once() {
        let harness = Harness::new("req-sprint-left", |_| {});
        // S1 has one dollar and holds nothing; the contract refined meanwhile asks for five.
        harness.project.open_sprint("S1", Some(1.0), &[]);
        let adapter = harness.recorded(vec![refine_writes_task_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Ready);
        let evaluated = last(&harness, EventKind::ContractEvaluated).expect("the judgement");
        assert!(
            matches!(&evaluated.body, EventBody::ContractEvaluated(body) if body.passed),
            "{:?}",
            evaluated.body
        );
        let refines = adapter
            .started()
            .iter()
            .filter(|spec| spec.purpose == SessionPurpose::Refine)
            .count();
        assert_eq!(refines, 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn returns_a_failing_contract_with_its_failures() {
        let harness = Harness::new("req-failing", |wire| {
            wire["rules"]["max_task_budget_usd"] = json!(5);
        });
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        write_over_the_cap(&harness).await;

        orchestrator.tick().await.expect("the tick runs");
        let evaluated = last(&harness, EventKind::ContractEvaluated).expect("the judgement");
        let EventBody::ContractEvaluated(body) = &evaluated.body else {
            panic!("a judgement");
        };
        assert!(!body.passed);
        assert!(
            body.failures
                .iter()
                .any(|failure| failure.contains("exceeds the team's cap of 5 USD")),
            "{:?}",
            body.failures
        );
        assert!(adapter.started().is_empty());
        // A later judgement of another gate is not the contract's readiness.
        harness.project.record(
            "CTV-1",
            "contract.evaluated",
            &json!({ "gate": "definition_of_done", "passed": false, "failures": ["not done"] }),
        );

        orchestrator.tick().await.expect("the tick runs");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Refine);
        assert!(
            started[0]
                .initial_prompt
                .contains("exceeds the team's cap of 5 USD"),
            "{}",
            started[0].initial_prompt
        );
        assert!(
            !started[0].initial_prompt.contains("not done"),
            "{}",
            started[0].initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_contract_written_again_after_a_failure() {
        let harness = Harness::new("req-judges-again", |wire| {
            wire["rules"]["max_task_budget_usd"] = json!(5);
        });
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        write_over_the_cap(&harness).await;
        orchestrator
            .tick()
            .await
            .expect("the first write is judged");
        write_over_the_cap(&harness).await;

        orchestrator.tick().await.expect("the tick runs");

        let judged = harness
            .events(&[EventKind::ContractEvaluated])
            .iter()
            .filter(
                |event| matches!(&event.body, EventBody::ContractEvaluated(body) if !body.passed),
            )
            .count();
        assert_eq!(judged, 2);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_write_whose_later_refusal_was_not_the_governors() {
        let harness = Harness::new("req-judges-others-refusal", |wire| {
            wire["rules"]["max_task_budget_usd"] = json!(5);
        });
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        write_over_the_cap(&harness).await;
        harness.project.record(
            "CTV-1",
            "transition.refused",
            &json!({
                "from": "refining",
                "to": "ready",
                "actor": "product_manager",
                "requested_by": "pm",
                "refusal": "gate_failed",
                "details": ["the gate did not open"]
            }),
        );

        orchestrator.tick().await.expect("the tick runs");

        assert!(last(&harness, EventKind::ContractEvaluated).is_some());
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_contract_the_human_holds_without_a_session() {
        let harness = Harness::new("req-judges-locked", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        a_sized_request(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator
            .handle(Command::ContractLock {
                task_id: task("CTV-1"),
            })
            .await
            .expect("the human takes the contract");
        orchestrator.tick().await.expect("refining starts");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Refining);

        orchestrator.tick().await.expect("the tick runs");

        assert!(last(&harness, EventKind::ContractEvaluated).is_some());
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_draft_whose_move_to_refining_was_refused() {
        let harness = Harness::new("req-draft-refused", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        a_sized_request(&harness, &orchestrator, RequestSize::Small).await;
        harness.project.record(
            "CTV-1",
            "transition.refused",
            &json!({
                "from": "draft",
                "to": "refining",
                "actor": "product_manager",
                "requested_by": "pm",
                "refusal": "gate_failed",
                "details": ["the gate did not open"]
            }),
        );

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Draft);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn readies_an_epic_without_asking_the_owner() {
        let harness = Harness::new("req-approval", |_| {});
        let adapter = harness.recorded(vec![refine_writes_epic_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the epic is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "CTV-1").last().map(String::as_str),
            Some("refining -> ready")
        );
        assert!(
            harness.events(&[EventKind::EscalationRaised]).is_empty(),
            "no escalation asks the owner to approve the epic"
        );
        assert!(
            harness.events(&[EventKind::ContractEvaluated]).iter().all(
                |event| matches!(&event.body, EventBody::ContractEvaluated(body) if body.passed)
            )
        );
        assert!(!harness.row("CTV-1").awaiting_approval);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_contract_that_failed_three_times() {
        let harness = Harness::new("req-three-failures", |wire| {
            wire["rules"]["max_task_budget_usd"] = json!(5);
        });
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        refining(&harness, &orchestrator, RequestSize::Small).await;
        for _ in 0..3 {
            harness.project.record(
                "CTV-1",
                "contract.evaluated",
                &json!({ "gate": "definition_of_ready", "passed": false, "failures": ["no"] }),
            );
        }
        write_over_the_cap(&harness).await;

        orchestrator.tick().await.expect("the tick runs");

        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        assert!(matches!(
            &escalation.body,
            EventBody::EscalationRaised(body)
                if body.reason == EscalationRaisedBodyReason::ReadinessFailures
        ));
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Escalated);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_rederive_as_filed_whole() {
        use catervas_core::contract::Role;
        use catervas_store::requests::{file_rederive_request, rederive_request};

        let harness = Harness::new(
            "req-rederive-whole",
            crate::tools::fixtures::with_the_designer,
        );
        let deps = &harness.project.deps;
        let human = "docs/catervas/product/roadmap.md";
        let twin = "docs/catervas/product/roadmap.agent.md";
        file_rederive_request(
            &deps.files,
            &deps.log,
            rederive_request(human, twin, Role::ProductManager, Role::Architect, 2.0),
            twin,
            (deps.clock.now(), &deps.ids),
        )
        .expect("the re-derive is filed");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "CTV-1"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter.started().is_empty(),
            "no refine session rewrites it"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_owners_change_is_judged_without_refining() {
        use catervas_core::contract::Role;
        use catervas_protocol::event::TaskCreatedBodyFiledAs;
        use catervas_store::requests::{change_request, file_small_request};

        let harness = Harness::new(
            "req-owner-change-whole",
            crate::tools::fixtures::with_the_designer,
        );
        let deps = &harness.project.deps;
        let path = "docs/catervas/architecture/overview.html";
        file_small_request(
            &deps.files,
            &deps.log,
            change_request(
                path,
                None,
                "Overview",
                "Say less.",
                Role::Architect,
                Role::ProductManager,
                2.0,
            ),
            "human",
            TaskCreatedBodyFiledAs::OwnerChange,
            "the owner's change of Overview",
            (deps.clock.now(), &deps.ids),
        )
        .expect("the change is filed");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "CTV-1"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter.started().is_empty(),
            "no refine session rewrites it"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_the_catervafication_epic_as_filed_whole() {
        use crate::catervafication::{Project, epic_contract};
        use catervas_store::requests::file_catervafication_epic;

        let harness = Harness::new("req-catervafication-whole", |_| {});
        let deps = &harness.project.deps;
        file_catervafication_epic(
            &deps.files,
            &deps.log,
            epic_contract(
                Project::New,
                2.0,
                catervas_core::folders::DocumentFormat::Markdown,
                1,
            ),
            "catervas",
            Project::New,
            (deps.clock.now(), &deps.ids),
        )
        .expect("the epic is filed");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        // Rule 10 moves it to refining on the Product Manager's behalf; the next tick judges it as
        // filed, and the governor readies it, with no refine session to rewrite the template.
        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "CTV-1"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter.started().is_empty(),
            "no refine session rewrites it"
        );
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
    }

    /// The suggested six's Catervafication started, and run until its epic CTV-1 is `in_progress`.
    async fn a_catervafication_in_progress(
        harness: &Harness,
    ) -> (Orchestrator, Arc<crate::recorded::RecordedAdapter>) {
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator
            .handle(Command::CatervaficationStart)
            .await
            .expect("the owner starts it");
        for _ in 0..3 {
            orchestrator.tick().await.expect("the tick runs");
        }
        assert_eq!(harness.row("CTV-1").status, TaskStatus::InProgress);
        (orchestrator, adapter)
    }

    /// The role each task under CTV-1 is for, in the order they were filed.
    fn roles_under_the_epic(harness: &Harness) -> Vec<String> {
        harness
            .project
            .deps
            .files
            .list_contracts()
            .expect("the contracts list")
            .iter()
            .map(|id| {
                harness
                    .project
                    .deps
                    .files
                    .read_contract(id)
                    .expect("a contract")
            })
            .filter(|contract| {
                contract.parent.as_ref().map(|parent| parent.as_str()) == Some("CTV-1")
            })
            .map(|contract| contract.assignee_role.to_string())
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn files_every_task_in_the_first_tick() {
        let harness = Harness::with_the_suggested_six("req-catervafication-files");
        let (orchestrator, adapter) = a_catervafication_in_progress(&harness).await;

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Acted {
                task_id: task("CTV-1"),
                what: "filed the Catervafication's tasks: CTV-2, CTV-3, CTV-4, CTV-5, CTV-6, CTV-7, CTV-8"
                    .to_string()
            }
        );
        assert_eq!(
            roles_under_the_epic(&harness),
            [
                "product_manager",
                "architect",
                "software_developer",
                "ui_ux_designer",
                "marketing_specialist",
                "software_developer",
                "ui_ux_designer"
            ]
        );
        for id in [
            "CTV-2", "CTV-3", "CTV-4", "CTV-5", "CTV-6", "CTV-7", "CTV-8",
        ] {
            let row = harness.row(id);
            assert_eq!(row.parent, Some(task("CTV-1")), "{id}");
            let created = harness
                .events(&[EventKind::TaskCreated])
                .into_iter()
                .find(|event| event.envelope.ids.task_id == Some(task(id)))
                .expect("a task.created");
            assert!(
                matches!(&created.body, EventBody::TaskCreated(body) if body.created_by == "catervas"),
                "{id}"
            );
        }
        let contract = |id: &str| {
            harness
                .project
                .deps
                .files
                .read_contract(&task(id))
                .expect("a contract")
        };
        assert!(contract("CTV-6").dependencies.is_empty());
        assert!(catervas_core::folders::is_hunt(&contract("CTV-7")));
        assert!(catervas_core::folders::is_hunt(&contract("CTV-8")));
        assert!(!catervas_core::folders::is_hunt(&contract("CTV-4")));
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        // The team writes HTML (the default): the Architect is asked for pages, a diagram among them.
        let architects = serde_json::to_string(&contract("CTV-3")).expect("a contract is json");
        assert!(
            architects.contains("architecture/overview.html") && !architects.contains(".md"),
            "{architects}"
        );
        // A second tick files nothing.
        let report = orchestrator.tick().await.expect("the tick runs");
        assert!(
            !matches!(&report, TickReport::Acted { what, .. } if what.starts_with("filed the Catervafication")),
            "{report:?}"
        );
        assert_eq!(roles_under_the_epic(&harness).len(), 7);

        // A Designer whose browser is not ready (no preview can run) gets no task and no hunt.
        let unready = Harness::new(
            "req-catervafication-files-unready",
            crate::tools::fixtures::with_the_suggested_six,
        );
        let (orchestrator, _) = a_catervafication_in_progress(&unready).await;
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            roles_under_the_epic(&unready),
            [
                "product_manager",
                "architect",
                "software_developer",
                "marketing_specialist",
                "software_developer"
            ]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn files_a_hunt_per_member_in_the_first_tick() {
        let (mut harness, members) = Harness::project_folder_with(
            "req-catervafication-folder",
            &["api", "web"],
            crate::tools::fixtures::with_the_suggested_six,
        );
        harness.previews = Arc::new(crate::preview::fixtures::FakePreviews::ready());
        // `web` can start its app; `api` cannot.
        members[1].write("package.json", r#"{ "scripts": { "dev": "vite" } }"#);
        // The scan names the package manager by its lock file.
        members[1].write("package-lock.json", "{}");
        members[1].commit("add the package");
        let (orchestrator, _adapter) = a_catervafication_in_progress(&harness).await;

        orchestrator.tick().await.expect("the tick runs");

        let under_the_epic = |harness: &Harness| -> Vec<(String, Option<String>)> {
            let files = &harness.project.deps.files;
            files
                .list_contracts()
                .expect("the contracts list")
                .iter()
                .map(|id| files.read_contract(id).expect("a contract"))
                .filter(|contract| contract.parent.as_ref().map(|p| p.as_str()) == Some("CTV-1"))
                .map(|contract| {
                    (
                        contract.assignee_role.to_string(),
                        contract.repo.as_ref().map(|repo| repo.as_str().to_string()),
                    )
                })
                .collect()
        };
        let hunt = |role: &str, repo: &str| (role.to_string(), Some(repo.to_string()));
        let filed = under_the_epic(&harness);
        assert_eq!(filed.len(), 8, "{filed:?}");
        assert_eq!(
            filed[5..],
            [
                hunt("software_developer", "api"),
                hunt("software_developer", "web"),
                hunt("ui_ux_designer", "web"),
            ]
        );
        let notes = |harness: &Harness| -> Vec<String> {
            harness
                .events(&[EventKind::NoteWritten])
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::NoteWritten(body)
                        if body.kind == NoteWrittenBodyKind::Progress
                            && event.envelope.ids.task_id == Some(task("CTV-1")) =>
                    {
                        assert_eq!(body.written_by, "catervas");
                        Some(body.text.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            notes(&harness),
            ["No UI bug hunt for api: Catervas found no way to start its app."]
        );
        let budget = harness
            .project
            .deps
            .files
            .read_contract(&task("CTV-1"))
            .expect("the epic")
            .budget
            .max_cost_usd;
        assert!(
            (budget
                - 9.0
                    * catervas_store::requests::placeholder_budget_usd(
                        &harness
                            .project
                            .deps
                            .files
                            .read_team()
                            .expect("a team")
                            .rules()
                    ))
            .abs()
                < 1e-9,
            "{budget}"
        );

        // A second tick files nothing and writes no second note.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(under_the_epic(&harness).len(), 8);
        assert_eq!(notes(&harness).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_epic_without_the_hunts_requirement_files_no_hunt() {
        let harness = Harness::with_the_suggested_six("req-catervafication-no-hunts");
        let (orchestrator, _adapter) = a_catervafication_in_progress(&harness).await;
        // An epic filed before hunts existed: R1 alone.
        let files = &harness.project.deps.files;
        let mut wire = serde_json::to_value(files.read_contract(&task("CTV-1")).expect("the epic"))
            .expect("a contract is json");
        wire["requirements"] = json!([wire["requirements"][0].clone()]);
        wire["exit_criteria"][0]["satisfies"] = json!(["R1"]);
        files
            .write_contract(&catervas_core::contract::validate_contract(&wire).expect("a contract"))
            .expect("the epic is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(roles_under_the_epic(&harness).len(), 5);
        assert!(!roles_under_the_epic(&harness).iter().skip(5).any(|_| true));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn files_no_task_twice() {
        let harness = Harness::with_the_suggested_six("req-catervafication-twice");
        // The Designer is paused at first: it has no task.
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        let mut wire = serde_json::to_value(&team).expect("the team is JSON");
        wire["agents"][4]["status"] = json!("paused");
        harness
            .project
            .deps
            .files
            .write_team(&catervas_core::team::validate_team(&wire).expect("a team"))
            .expect("the team is written");
        let (orchestrator, _adapter) = a_catervafication_in_progress(&harness).await;
        let epic_only = crate::orchestrator::TickScope {
            task_id: Some(task("CTV-1")),
            rules: crate::orchestrator::TickRules::All,
        };
        orchestrator
            .tick_within(&epic_only)
            .await
            .expect("the tick files the tasks");
        assert_eq!(
            roles_under_the_epic(&harness),
            [
                "product_manager",
                "architect",
                "software_developer",
                "marketing_specialist",
                "software_developer"
            ]
        );

        // The owner cancels the Architect's task: it is not filed again.
        harness.project.moved(
            "CTV-3",
            "draft",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        let report = orchestrator
            .tick_within(&epic_only)
            .await
            .expect("the tick runs");
        assert!(is_idle(&report), "{report:?}");
        assert_eq!(roles_under_the_epic(&harness).len(), 5);

        // A Designer who joins later gets its task on the next tick.
        wire["agents"][4]["status"] = json!("active");
        harness
            .project
            .deps
            .files
            .write_team(&catervas_core::team::validate_team(&wire).expect("a team"))
            .expect("the team is written");
        let report = orchestrator
            .tick_within(&epic_only)
            .await
            .expect("the tick runs");
        assert!(
            matches!(&report, TickReport::Acted { what, .. } if what == "filed the Catervafication's tasks: CTV-7, CTV-8"),
            "{report:?}"
        );
        assert_eq!(
            roles_under_the_epic(&harness).last().map(String::as_str),
            Some("ui_ux_designer")
        );
        assert_eq!(roles_under_the_epic(&harness).len(), 7);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_child_filed_whole_without_a_session() {
        let harness = Harness::new("req-child-whole", |_| {});
        harness
            .project
            .filed_with("CTV-1", "in_progress", "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["allowed_paths"] = json!(["done.txt"]);
            });
        harness.project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        // The call `plan_breaks_down_ctv_1` makes, made here without its session.
        let mut contract = Harness::request_fields("Add done.txt");
        contract["budget"] = json!({ "max_cost_usd": 2 });
        call(
            &harness,
            "pm",
            None,
            "catervas_create_task",
            json!({ "parent": "CTV-1", "contract": contract }),
        )
        .await
        .expect("the epic's assignee files its task");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "CTV-2"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.task_id != Some(task("CTV-2")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_waiting_on_the_human() {
        let harness = Harness::new("req-waiting", |_| {});
        harness.ready("CTV-1");
        harness.project.record(
            "CTV-1",
            "question.asked",
            &json!({ "question": "Which file?", "asked_by": "pm" }),
        );
        harness.ready("CTV-2");
        let adapter = harness.recorded(vec![plan_assigns_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started()[0].task_id, Some(task("CTV-2")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catches_up_with_another_process_before_each_tick() {
        let harness = Harness::new("req-catch-up", |_| {});
        let adapter = harness.recorded(vec![refine_asks_ctv_1(), replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the question is asked");
        let n = last(&harness, EventKind::QuestionAsked)
            .expect("the question")
            .envelope
            .seq;
        let wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "catervas",
            "project_id": "catervas",
            "task_id": "CTV-1",
            "kind": "question.answered",
            "body": { "question_id": n, "answer": "Yes.", "answered_by": "human" },
        });
        let event = event_from_value(&wire).expect("the answer is schema-valid");
        harness
            .project
            .deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("another process appends the answer");

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(started[1].purpose, SessionPurpose::Refine);
        assert_eq!(started[1].task_id, Some(task("CTV-1")));
    }

    /// Epic CTV-1 as the Product Manager wrote it for the human to review, filed in `status`: C1
    /// (`command`, `test -f done.txt`) and C2 (`review`), `done.txt` its one path, 5 dollars.
    fn an_epic(harness: &Harness, id: &str, status: &str, change: impl FnOnce(&mut Value)) {
        harness
            .project
            .filed_with(id, status, "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["allowed_paths"] = json!(["done.txt"]);
                wire["exit_criteria"] = json!([
                    {
                        "id": "C1",
                        "text": "done.txt exists.",
                        "satisfies": ["R1"],
                        "verification": {
                            "method": "command",
                            "command": "test -f done.txt",
                            "expect": { "exit_code": 0 }
                        }
                    },
                    {
                        "id": "C2",
                        "text": "done.txt says what the request asked.",
                        "satisfies": ["R1"],
                        "verification": {
                            "method": "review",
                            "rubric": ["Does done.txt say what the request asked?"]
                        }
                    }
                ]);
                change(wire);
            });
    }

    /// Epic CTV-1 moved to `in_progress`, held by `pm`.
    fn an_epic_in_progress(harness: &Harness, change: impl FnOnce(&mut Value)) {
        an_epic(harness, "CTV-1", "ready", change);
        let people = json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "pm" });
        harness.project.moved("CTV-1", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-1", "assigned", "in_progress", &people);
    }

    /// CTV-2 under CTV-1, filed `ready`, moved through to `status` as `dev-a`'s reviewed by
    /// `dev-b`, its worktree made when it is in progress.
    fn a_child(harness: &Harness, status: &str) {
        harness.file_under("CTV-2", "ready", Some("CTV-1"), |wire| {
            wire["title"] = json!("Add done.txt");
            wire["budget"] = json!({ "max_cost_usd": 2 });
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        let path = ["ready", "assigned", "in_progress", "verifying", "accepted"];
        let cancelled = status == "cancelled";
        let last = if cancelled { "ready" } else { status };
        for pair in path.windows(2) {
            if pair[0] == last {
                break;
            }
            harness.project.moved("CTV-2", pair[0], pair[1], &people);
            if pair[1] == "in_progress" {
                harness
                    .project
                    .deps
                    .git
                    .create_worktree(&harness.worktree("CTV-2"), &harness.branch("CTV-2"), "main")
                    .expect("the child's worktree is made");
            }
        }
        if cancelled {
            harness
                .project
                .moved("CTV-2", "ready", "cancelled", &json!({}));
        }
        if matches!(status, "accepted" | "cancelled") && harness.worktree("CTV-2").exists() {
            harness
                .project
                .deps
                .git
                .remove_worktree(&harness.worktree("CTV-2"))
                .expect("the child's worktree is removed");
        }
    }

    /// CTV-2 recorded as integrated into `main`, with `done.txt` committed there when `with_done`.
    fn integrated(harness: &Harness, with_done: bool) {
        if with_done {
            harness.commit_at_root("done.txt", "", "Add done.txt");
        }
        harness.project.record(
            "CTV-2",
            "task.integrated",
            &json!({ "sha": "abc", "into": "main", "integrated_by": "governor" }),
        );
    }

    /// Epic CTV-1 `verifying` after its child CTV-2 was accepted, with its completion note.
    fn an_epic_verifying(harness: &Harness, change: impl FnOnce(&mut Value)) {
        an_epic_in_progress(harness, change);
        a_child(harness, "accepted");
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "completion", "text": "CTV-2 added done.txt; nothing left out.", "written_by": "pm" }),
        );
        harness.project.moved(
            "CTV-1",
            "in_progress",
            "verifying",
            &json!({ "actor": "assignee", "requested_by": "pm", "assignee": "pm" }),
        );
    }

    fn main_head(harness: &Harness) -> String {
        git_output_in(&harness.project.repo.path, &["rev-parse", "main"])
    }

    fn governor_runs(harness: &Harness, id: &str) -> Vec<(String, bool, String)> {
        harness
            .events(&[EventKind::CriterionRecorded])
            .iter()
            .filter(|event| event.envelope.ids.task_id == Some(task(id)))
            .filter_map(|event| match &event.body {
                EventBody::CriterionRecorded(body) if body.recorded_by == "governor" => Some((
                    body.criterion_id.clone(),
                    body.passed,
                    body.evidence.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catervas_accepts_an_epic_once_its_tasks_are_integrated() {
        let harness = Harness::new("epic-governor-accepts", |_| {});
        an_epic_verifying(&harness, |wire| {
            wire["exit_criteria"] = json!([{
                "id": "C1",
                "text": "The check passes.",
                "satisfies": ["R1"],
                "verification": { "method": "command", "command": "true", "expect": { "exit_code": 0 } }
            }]);
        });
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        // Its task is accepted but not yet integrated: nothing happens to the epic.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Verifying);
        assert!(governor_runs(&harness, "CTV-1").is_empty());

        integrated(&harness, true);
        orchestrator.tick().await.expect("the criteria run");
        orchestrator.tick().await.expect("the epic is decided");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
        assert!(adapter.started().is_empty());
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(body.actor, TransitionActorWire::Governor);
        assert_eq!(moved.envelope.ids.agent_id, None);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_an_approved_epic_to_its_product_manager() {
        let harness = Harness::new("epic-assigns", |_| {});
        an_epic(&harness, "CTV-1", "ready", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("ready".to_string(), "assigned".to_string())
        );
        assert_eq!(body.actor, TransitionActorWire::ProductManager);
        assert_eq!(body.assignee.as_deref(), Some("pm"));
        assert_eq!(body.reviewer, None);

        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            moves_of(&harness, "CTV-1").last().map(String::as_str),
            Some("assigned -> in_progress")
        );
        assert!(!harness.worktree("CTV-1").exists());
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_to_assign_a_second_epic_while_the_first_is_open() {
        let harness = Harness::new("epic-wip", |_| {});
        an_epic(&harness, "CTV-1", "ready", |_| {});
        an_epic(&harness, "CTV-2", "ready", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let first = orchestrator.tick().await.expect("the tick runs");
        let second = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::InProgress);
        // CTV-1's breakdown has begun and its one task waits on a block, so rule 6 has nothing
        // to do for it, and rule 8 is reached.
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |_| {});
        harness.project.moved(
            "CTV-3",
            "in_progress",
            "blocked",
            &json!({
                "assignee": "dev-a",
                "reviewer": "dev-b",
                "blocker": { "description": "the API is down", "needed": "the API" }
            }),
        );
        let third = orchestrator.tick().await.expect("the tick runs");

        assert!(matches!(&first, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"));
        assert!(
            matches!(&second, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1")
        );
        assert!(is_idle(&third), "{third:?}");
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Ready);
        assert!(
            harness
                .events(&[EventKind::TransitionRefused])
                .iter()
                .all(|event| event.envelope.ids.task_id != Some(task("CTV-2")))
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_an_epic_down_in_a_plan_session() {
        let harness = Harness::new("epic-breakdown", |_| {});
        an_epic_in_progress(&harness, |_| {});
        let adapter = harness.recorded(vec![plan_breaks_down_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert!(
            spec.initial_prompt.contains("catervas_create_task"),
            "{}",
            spec.initial_prompt
        );
        let child = harness.row("CTV-2");
        assert_eq!(child.parent, Some(task("CTV-1")));
        assert!(child.triaged);
        assert_eq!(child.status, TaskStatus::Draft);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_an_epics_task_through_its_assignee() {
        let harness = Harness::new("epic-assigns-child", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "ready");
        let adapter = harness.recorded(vec![plan_assigns_ctv_2()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.task_id, Some(task("CTV-2")));
        let child = harness.row("CTV-2");
        assert_eq!(child.status, TaskStatus::Assigned);
        assert_eq!(child.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(child.reviewer_id.as_deref(), Some("dev-b"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_epic_alone_while_its_tasks_are_open() {
        let harness = Harness::new("epic-open-tasks", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "in_progress");
        let adapter = harness.recorded(vec![implement_finishes_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-2")
        );
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.task_id != Some(task("CTV-1")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_an_epic_whose_tasks_are_done() {
        let harness = Harness::new("epic-closes", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "accepted");
        integrated(&harness, true);
        let adapter = harness.recorded(vec![plan_closes_epic_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        assert!(
            spec.initial_prompt.contains("CTV-2") && spec.initial_prompt.contains("accepted"),
            "{}",
            spec.initial_prompt
        );
        let note = last(&harness, EventKind::NoteWritten).expect("the completion note");
        assert!(matches!(
            &note.body,
            EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Completion
        ));
        assert_eq!(note.envelope.ids.task_id, Some(task("CTV-1")));
        assert_eq!(
            moves_of(&harness, "CTV-1").last().map(String::as_str),
            Some("in_progress -> verifying")
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_down_again_when_every_task_was_cancelled() {
        let harness = Harness::new("epic-breaks-down-again", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "cancelled");
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        // The breakdown's message, not the close-out's, which names `catervas_create_task` too.
        assert!(
            spec.initial_prompt.starts_with("Break the epic CTV-1 down"),
            "{}",
            spec.initial_prompt
        );
        assert!(
            !spec.initial_prompt.contains("Every task under the epic"),
            "{}",
            spec.initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_an_epics_tasks_to_be_integrated() {
        let harness = Harness::new("epic-waits-integration", |_| {});
        an_epic_verifying(&harness, |_| {});
        harness.unmerged_branch("CTV-2");
        assert!(harness.row("CTV-2").awaiting_integration);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert!(governor_runs(&harness, "CTV-1").is_empty());
        assert!(!harness.worktree("CTV-1-base").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_an_epics_criteria_on_the_integration_branch() {
        let harness = Harness::new("epic-runs", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator =
            harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes.clone());

        orchestrator.tick().await.expect("the tick runs");

        let runs = governor_runs(&harness, "CTV-1");
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert_eq!((runs[0].0.as_str(), runs[0].1), ("C1", true));
        let recorded = last(&harness, EventKind::CriterionRecorded).expect("the run");
        assert!(matches!(
            &recorded.body,
            EventBody::CriterionRecorded(body) if body.run_by == CriterionRecordedBodyRunBy::Reviewer
        ));
        assert_eq!(sandboxes.based("CTV-1"), 1);
        assert!(!harness.worktree("CTV-1-base").exists());
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(governor_runs(&harness, "CTV-1").len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_the_humans_acceptance_to_the_product_manager() {
        let harness = Harness::new("epic-accepted", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let adapter = harness.recorded(vec![accept_ctv_1()]);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());
        orchestrator.tick().await.expect("Catervas runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("Both look right.".to_string()),
            })
            .await
            .expect("the human accepts the epic");

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert_eq!(witness.had_executor(), vec![false]);
        assert!(
            spec.system_prompt
                .contains("The human, accepting the result: Both look right."),
            "{}",
            spec.system_prompt
        );
        let evidence = &governor_runs(&harness, "CTV-1")[0].2;
        let results = spec
            .initial_prompt
            .find("<untrusted source=\"results\">")
            .expect("Catervas's results, marked");
        assert!(
            spec.initial_prompt[results..].contains(evidence.as_str()),
            "{}",
            spec.initial_prompt
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
        let review = last(&harness, EventKind::ReviewRecorded).expect("the review");
        assert_eq!(review.envelope.ids.task_id, Some(task("CTV-1")));
        assert!(matches!(
            &review.body,
            EventBody::ReviewRecorded(body)
                if body.reviewer == "human" && body.criteria_run == 2 && body.passed
        ));
        assert!(
            harness
                .events(&[EventKind::TaskIntegrated])
                .iter()
                .all(|event| event.envelope.ids.task_id != Some(task("CTV-1")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_an_epics_test_criterion_without_the_new_tests_check() {
        let harness = Harness::new("epic-test-criterion", |wire| {
            wire["rules"]["require_new_tests"] = json!(true);
        });
        an_epic_verifying(&harness, |wire| {
            wire["exit_criteria"][0]["verification"] = json!({
                "method": "test",
                "command": "test -f done.txt",
                "new_tests_required": true
            });
        });
        integrated(&harness, true);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let runs = governor_runs(&harness, "CTV-1");
        assert_eq!(runs.len(), 1, "{runs:?}");
        let (id, passed, evidence) = &runs[0];
        assert_eq!((id.as_str(), *passed), ("C1", true), "{evidence}");
        assert!(
            evidence.starts_with(&format!("at {}", main_head(&harness))),
            "{evidence}"
        );
        assert!(!evidence.contains("base-branch check"), "{evidence}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_a_failed_epic_back_for_more_work() {
        let harness = Harness::new("epic-failed", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, false);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");
        assert!(
            !governor_runs(&harness, "CTV-1")[0].1,
            "done.txt is not on main"
        );

        let failed = orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string()),
            })
            .await;
        assert!(
            matches!(&failed, Err(CommandError::Refused { reason }) if reason.starts_with("criterion_failed")),
            "{failed:?}"
        );
        orchestrator
            .handle(Command::TaskTransition {
                task_id: task("CTV-1"),
                to: TaskStatus::Escalated,
                reason: "C1 failed".to_string(),
            })
            .await
            .expect("the human escalates the epic");
        orchestrator
            .handle(Command::EscalationResolve {
                task_id: task("CTV-1"),
                to: TaskStatus::InProgress,
                message: "Add the missing file.".to_string(),
                extra_tries: None,
                extra_room: false,
            })
            .await
            .expect("the human sends it back");

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "pm")
        );
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        assert!(
            spec.system_prompt.contains("Add the missing file."),
            "{}",
            spec.system_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_an_epic_whose_assignee_is_paused() {
        let harness = Harness::new("epic-assignee-paused", |wire| {
            wire["agents"][0]["status"] = json!("paused");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(json!({
                    "id": "pm-2",
                    "display_name": "pm-2",
                    "role": "product_manager",
                    "status": "active"
                }));
        });
        an_epic_in_progress(&harness, |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_epic_open_while_a_task_under_it_is_escalated() {
        let harness = Harness::new("epic-escalated-child", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(2);
        });
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "accepted");
        integrated(&harness, true);
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |_| {});
        harness
            .project
            .moved("CTV-3", "ready", "escalated", &json!({}));
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_an_epic_beside_work_the_product_manager_finished() {
        let harness = Harness::new("epic-wip-finished", |_| {});
        let people = json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "pm" });
        for (id, last) in [("CTV-2", "accepted"), ("CTV-3", "cancelled")] {
            an_epic(&harness, id, "ready", |_| {});
            harness.project.moved(id, "ready", "assigned", &people);
            harness
                .project
                .moved(id, "assigned", "in_progress", &people);
            harness.project.moved(id, "in_progress", last, &people);
        }
        harness.settle_delivery();
        an_epic(&harness, "CTV-4", "ready", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(moves_of(&harness, "CTV-4"), ["ready -> assigned"]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_an_epic_whose_criterion_catervas_could_not_run() {
        let harness = Harness::new("epic-unrunnable", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let sandboxes = Arc::new(BrokenSandboxFactory::for_base(ExecError::SpawnFailed {
            detail: "no shell".to_string(),
        }));
        let orchestrator = harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Escalated);
        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        let EventBody::EscalationRaised(body) = &escalation.body else {
            panic!("an escalation");
        };
        assert!(body.detail.contains("C1"), "{}", body.detail);
        assert!(body.detail.contains("no shell"), "{}", body.detail);
        assert!(governor_runs(&harness, "CTV-1").is_empty());
        assert!(!harness.worktree("CTV-1-base").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_an_epics_review_once_with_its_human_criterion() {
        let harness = Harness::new("epic-review-once", |_| {});
        an_epic_verifying(&harness, |wire| {
            wire["exit_criteria"]
                .as_array_mut()
                .expect("a list of criteria")
                .push(json!({
                    "id": "C3",
                    "text": "The founder read it.",
                    "satisfies": ["R1"],
                    "verification": { "method": "human", "question": "Is it right?" }
                }));
        });
        integrated(&harness, true);
        let adapter = harness.recorded(vec![
            replays_catervas_read_board(),
            replays_catervas_read_board(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("All three hold.".to_string()),
            })
            .await
            .expect("the human accepts the epic");

        // Two verify sessions that do not ask for `accepted`: the review is recorded once.
        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started().len(), 2);
        let reviews = harness.events(&[EventKind::ReviewRecorded]);
        assert_eq!(reviews.len(), 1, "{reviews:?}");
        assert!(matches!(
            &reviews[0].body,
            EventBody::ReviewRecorded(body) if body.criteria_run == 3 && body.passed
        ));
    }

    /// Epic CTV-1 filed `ready` on a team with the active Scrum Master `sam`, for the Scrum Master
    /// to hold and the Product Manager to review.
    fn a_scrum_masters_epic(harness: &Harness) {
        an_epic(harness, "CTV-1", "ready", |wire| {
            wire["assignee_role"] = json!("scrum_master");
            wire["reviewer_role"] = json!("product_manager");
        });
    }

    /// The moves of the Scrum Master's epic: asked by `sam`, held by `sam`, reviewed by `pm`.
    fn the_scrum_masters() -> Value {
        json!({ "actor": "scrum_master", "requested_by": "sam", "assignee": "sam", "reviewer": "pm" })
    }

    /// The Scrum Master's epic CTV-1 moved to `in_progress`.
    fn a_scrum_masters_epic_in_progress(harness: &Harness) {
        a_scrum_masters_epic(harness);
        let people = the_scrum_masters();
        harness.project.moved("CTV-1", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-1", "assigned", "in_progress", &people);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_an_approved_epic_to_the_scrum_master() {
        let harness = Harness::new("epic-assigns-sm", with_a_scrum_master);
        a_scrum_masters_epic(&harness);
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("ready".to_string(), "assigned".to_string())
        );
        assert_eq!(body.actor, TransitionActorWire::ScrumMaster);
        assert_eq!(body.requested_by, "sam");
        assert_eq!(body.assignee.as_deref(), Some("sam"));
        assert_eq!(body.reviewer.as_deref(), Some("pm"));
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_an_epic_down_in_lanes_without_an_architect() {
        let harness = Harness::new("epic-breakdown-sm", with_a_scrum_master);
        a_scrum_masters_epic_in_progress(&harness);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "pm")
        );
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        assert!(
            spec.initial_prompt.starts_with("Break the epic CTV-1 down"),
            "{}",
            spec.initial_prompt
        );
        assert!(
            spec.initial_prompt.contains("lanes 1 to 2"),
            "{}",
            spec.initial_prompt
        );
    }

    /// CTV-2 under `pm`'s epic CTV-1 in lane 1, accepted, `dev-a`'s and reviewed by `dev-b`; CTV-3,
    /// lane 1 too, `ready`.
    fn lane_1_built_by_dev_a(harness: &Harness) {
        harness.file_under("CTV-2", "ready", Some("CTV-1"), |wire| {
            wire["lane"] = json!(1);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        for pair in ["ready", "assigned", "in_progress", "verifying", "accepted"].windows(2) {
            harness.project.moved("CTV-2", pair[0], pair[1], &people);
        }
        harness.unmerged_branch("CTV-2");
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |wire| {
            wire["lane"] = json!(1);
        });
    }

    /// What the plan session for CTV-3 is told of the agents with room for it.
    async fn those_with_room(harness: &Harness) -> String {
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick runs");
        let started = adapter.started();
        let spec = started
            .iter()
            .find(|spec| spec.task_id == Some(task("CTV-3")))
            .expect("a plan session for CTV-3");
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        spec.initial_prompt.clone()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn offers_a_lanes_task_to_its_builder_alone() {
        let harness = Harness::new("lane-builder-offer", |_| {});
        an_epic_in_progress(&harness, |_| {});
        lane_1_built_by_dev_a(&harness);

        let message = those_with_room(&harness).await;

        assert!(
            message.contains("with room for it: dev-a. The agents of its reviewer role"),
            "{message}"
        );
        assert!(
            message.contains("lane 1 of CTV-1, which dev-a builds"),
            "{message}"
        );

        // With dev-a paused the lane has no builder: the other developer is offered it.
        let harness = Harness::new("lane-builder-paused", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        an_epic_in_progress(&harness, |_| {});
        lane_1_built_by_dev_a(&harness);

        let message = those_with_room(&harness).await;

        assert!(
            message.contains("with room for it: dev-b. The agents of its reviewer role"),
            "{message}"
        );
        assert!(message.contains("first of lane 1"), "{message}");
    }

    /// `with_a_scrum_master`, and an Architect, `ada`.
    fn with_a_scrum_master_and_an_architect(wire: &mut Value) {
        with_a_scrum_master(wire);
        wire["agents"]
            .as_array_mut()
            .expect("a list of agents")
            .push(catervas_core::team::fixtures::an_agent_wire(
                "ada",
                "architect",
            ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn files_the_plan_task_with_the_architect() {
        // The plan is named as the team writes it: Markdown here.
        let harness = Harness::new("epic-plan-task", |wire| {
            with_a_scrum_master_and_an_architect(wire);
            wire["policy"]["document_format"] = json!("markdown");
        });
        a_scrum_masters_epic_in_progress(&harness);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str(), spec.task_id.clone()),
            (SessionPurpose::Plan, "pm", Some(task("CTV-1")))
        );
        assert!(
            spec.initial_prompt
                .starts_with("Plan the epic CTV-1 with the Architect"),
            "{}",
            spec.initial_prompt
        );
        for held in ["docs/catervas/architecture/plans/CTV-1.md", "architect"] {
            assert!(
                spec.initial_prompt.contains(held),
                "{}",
                spec.initial_prompt
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn names_a_new_plan_in_the_teams_html() {
        // No document_format: the default team writes HTML.
        let harness = Harness::new("epic-plan-task-html", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let prompt = &adapter.started()[0].initial_prompt;
        assert!(
            prompt.contains("docs/catervas/architecture/plans/CTV-1.html"),
            "{prompt}"
        );
        assert!(!prompt.contains("plans/CTV-1.md"), "{prompt}");
    }

    /// CTV-2 under the Scrum Master's epic CTV-1: the plan task, accepted, with `plan` and its one
    /// allowed path both the epic's plan.
    fn an_accepted_plan_task(harness: &Harness) {
        an_accepted_plan_task_at(harness, "docs/catervas/architecture/plans/CTV-1.md");
    }

    /// `an_accepted_plan_task`, the plan at `plan`.
    fn an_accepted_plan_task_at(harness: &Harness, plan: &str) {
        harness.file_under("CTV-2", "ready", Some("CTV-1"), |wire| {
            wire["plan"] = json!(plan);
            wire["allowed_paths"] = json!([plan]);
            wire["assignee_role"] = json!("architect");
            wire["reviewer_role"] = json!("product_manager");
        });
        let people = json!({ "assignee": "ada", "reviewer": "pm" });
        for pair in ["ready", "assigned", "in_progress", "verifying", "accepted"].windows(2) {
            harness.project.moved("CTV-2", pair[0], pair[1], &people);
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn writes_the_plans_tasks_from_the_plan_tasks_branch() {
        let harness = Harness::new("epic-plans-tasks", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        // The plan is on CTV-2's branch alone: `main` has none, and CTV-2 awaits integration.
        branch_with(
            &harness,
            "docs/CTV-2",
            "main",
            &[(
                THE_PLAN,
                "# Plan for CTV-1: Add done.txt\n\n## Lane 1\n\n### 1.1 Write it\n",
            )],
        );
        assert!(harness.row("CTV-2").awaiting_integration);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let spec = started
            .iter()
            .find(|spec| spec.task_id == Some(task("CTV-1")))
            .expect("a session about the epic");
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "pm")
        );
        for held in [
            "<untrusted source=\"plan\">\n# Plan for CTV-1: Add done.txt",
            "## Lane 1",
            "CTV-2",
            "`lane`",
        ] {
            assert!(
                spec.initial_prompt.contains(held),
                "{held}: {}",
                spec.initial_prompt
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_plan_is_read_in_either_format() {
        let harness = Harness::new("epic-plan-html", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task_at(&harness, "docs/catervas/architecture/plans/CTV-1.html");
        std::fs::create_dir_all(
            harness
                .project
                .repo
                .path
                .join("docs/catervas/architecture/plans"),
        )
        .expect("the plans folder is made");
        harness.commit_at_root(
            "docs/catervas/architecture/plans/CTV-1.html",
            "<h1>Plan for CTV-1: Add done.txt</h1>\n<h2>Lane 1</h2>\n",
            "docs: plan CTV-1",
        );
        if harness
            .project
            .deps
            .projections
            .board()
            .expect("the board")
            .iter()
            .any(|row| row.task_id == task("CTV-2") && row.awaiting_integration)
        {
            integrated(&harness, false);
        }
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let spec = started
            .iter()
            .find(|spec| spec.task_id == Some(task("CTV-1")))
            .expect("a session about the epic");
        assert!(
            spec.initial_prompt
                .contains("<untrusted source=\"plan\">\n<h1>Plan for CTV-1: Add done.txt</h1>"),
            "{}",
            spec.initial_prompt
        );
    }

    /// Commits `files` on a new branch `branch` made from `from`, in a worktree of its own that is
    /// removed again: the branch is left holding the work.
    fn branch_with(harness: &Harness, branch: &str, from: &str, files: &[(&str, &str)]) {
        let git = &harness.project.deps.git;
        let path = harness
            .project
            .repo
            .path
            .join(".catervas/local/worktrees")
            .join(format!("scratch-{}", branch.replace('/', "-")));
        git.create_worktree(&path, branch, from)
            .expect("the scratch worktree is made");
        let mut paths = Vec::new();
        for (file, text) in files {
            let target = path.join(file);
            std::fs::create_dir_all(target.parent().expect("a folder")).expect("made");
            std::fs::write(target, text).expect("written");
            paths.push((*file).to_string());
        }
        git.commit(&path, "The branch's work", &paths)
            .expect("committed");
        git.remove_worktree(&path)
            .expect("the scratch worktree goes");
    }

    const THE_PLAN: &str = "docs/catervas/architecture/plans/CTV-1.md";

    /// CTV-`n` under the epic CTV-1, a Software Developer's task of the plan's lane `lane`,
    /// `dev-a`'s, assigned, depending on `needs`.
    fn a_lane_task_assigned(harness: &Harness, n: u32, lane: u32, needs: &[&str]) {
        let id = format!("CTV-{n}");
        harness.file_under(&id, "ready", Some("CTV-1"), |wire| {
            wire["plan"] = json!(THE_PLAN);
            wire["lane"] = json!(lane);
            wire["allowed_paths"] = json!(["site/**"]);
            wire["dependencies"] = json!(needs);
        });
        harness.project.moved(
            &id,
            "ready",
            "assigned",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_task_starts_from_its_unmerged_dependencies_and_its_plan() {
        let harness = Harness::new("epic-start-points", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        branch_with(
            &harness,
            "docs/CTV-2",
            "main",
            &[(THE_PLAN, "# Plan for CTV-1\n")],
        );
        let git = &harness.project.deps.git;
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let tick_until_started = |id: &'static str| {
            let harness = &harness;
            let orchestrator = &orchestrator;
            async move {
                for _ in 0..4 {
                    orchestrator.tick().await.expect("the tick runs");
                    if harness.worktree(id).is_dir() {
                        return;
                    }
                }
            }
        };

        // A lane task starts from the plan's branch, the plan being on no integration branch.
        a_lane_task_assigned(&harness, 3, 1, &[]);
        tick_until_started("CTV-3").await;
        assert!(harness.worktree("CTV-3").join(THE_PLAN).is_file());

        // A task that depends on an accepted, unmerged task starts from its branch.
        std::fs::create_dir_all(harness.worktree("CTV-3").join("site")).expect("made");
        std::fs::write(harness.worktree("CTV-3").join("site/a.txt"), "a\n").expect("written");
        git.commit(
            &harness.worktree("CTV-3"),
            "Add site/a.txt",
            &["site/a.txt".to_string()],
        )
        .expect("committed");
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        harness.project.record_by(
            Some("dev-a"),
            at(),
            "CTV-3",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "ok",
                     "run_by": "assignee", "recorded_by": "dev-a" }),
        );
        harness
            .project
            .moved("CTV-3", "in_progress", "verifying", &people);
        harness
            .project
            .moved("CTV-3", "verifying", "accepted", &people);
        assert!(harness.row("CTV-3").awaiting_integration);
        a_lane_task_assigned(&harness, 4, 2, &["CTV-3"]);
        tick_until_started("CTV-4").await;
        assert!(harness.worktree("CTV-4").join("site/a.txt").is_file());
        assert!(harness.worktree("CTV-4").join(THE_PLAN).is_file());

        // A task with nothing unmerged starts from the integration branch.
        harness
            .project
            .moved("CTV-4", "in_progress", "verifying", &people);
        harness
            .project
            .moved("CTV-4", "verifying", "accepted", &people);
        harness.file_under("CTV-5", "ready", Some("CTV-1"), |_| {});
        harness.project.moved("CTV-5", "ready", "assigned", &people);
        tick_until_started("CTV-5").await;
        assert_eq!(git.commit_count("main", &harness.branch("CTV-5")), Ok(0));
        assert_eq!(
            catervas_store::git::task_base(git, "main", &task("CTV-5")),
            "main"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_task_that_conflicts_with_its_start_waits_without_a_worktree() {
        let harness = Harness::new("epic-start-conflict", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        branch_with(
            &harness,
            "docs/CTV-2",
            "main",
            &[(THE_PLAN, "the plan's line\n")],
        );
        // CTV-3 was built from `main` and changed the plan's file differently.
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |wire| {
            wire["allowed_paths"] = json!(["site/**"]);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        for pair in ["ready", "assigned", "in_progress", "verifying", "accepted"].windows(2) {
            harness.project.moved("CTV-3", pair[0], pair[1], &people);
        }
        branch_with(
            &harness,
            "feature/CTV-3",
            "main",
            &[(THE_PLAN, "another line\n")],
        );
        a_lane_task_assigned(&harness, 4, 2, &["CTV-3"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        for _ in 0..3 {
            orchestrator.tick().await.expect("the tick runs");
        }

        assert!(!harness.worktree("CTV-4").exists());
        assert_eq!(
            harness
                .project
                .deps
                .git
                .has_local_branch(&harness.branch("CTV-4")),
            Ok(false)
        );
        assert_eq!(harness.row("CTV-4").status, TaskStatus::Assigned);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_lane_task_is_judged_by_its_own_work_not_the_plans() {
        let harness = Harness::new("epic-start-diff", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        branch_with(
            &harness,
            "docs/CTV-2",
            "main",
            &[(THE_PLAN, "# Plan for CTV-1\n")],
        );
        a_lane_task_assigned(&harness, 3, 1, &[]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");
        let worktree = harness.worktree("CTV-3");
        let git = &harness.project.deps.git;
        std::fs::create_dir_all(worktree.join("site")).expect("made");
        std::fs::write(worktree.join("site/b.txt"), "b\n").expect("written");
        git.commit(&worktree, "Add site/b.txt", &["site/b.txt".to_string()])
            .expect("committed");
        harness.project.record(
            "CTV-3",
            "note.written",
            &json!({ "kind": "completion", "text": "Added it.", "written_by": "dev-a" }),
        );
        harness.project.record_by(
            Some("dev-a"),
            at(),
            "CTV-3",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "ok",
                     "run_by": "assignee", "recorded_by": "dev-a" }),
        );

        let base = catervas_store::git::task_base(git, "main", &task("CTV-3"));
        assert_eq!(base, "refs/catervas/start/CTV-3");
        assert_eq!(
            git.changed_paths(&base, &harness.branch("CTV-3")),
            Ok(vec!["site/b.txt".to_string()])
        );
        // The plan is outside the task's allowed paths, and not in its diff.
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        let request = catervas_core::governor::transition::TransitionRequest {
            task_id: task("CTV-3"),
            to: TaskStatus::Verifying,
            actor: catervas_core::governor::transition_table::TransitionActor::Assignee,
            agent_id: Some("dev-a".to_string()),
        };
        let context = harness
            .project
            .deps
            .transitions
            .context(
                &request,
                &crate::transitions::TransitionAsk::default(),
                &team,
            )
            .expect("the context reads");
        assert_eq!(context.done.changed_paths, vec!["site/b.txt".to_string()]);
        let outcome = harness
            .project
            .deps
            .transitions
            .request(
                &request,
                &crate::transitions::TransitionAsk::default(),
                &team,
            )
            .expect("the governor answers");
        assert!(
            matches!(outcome, crate::transitions::TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_an_epic_whose_plans_tasks_are_done() {
        let harness = Harness::new(
            "epic-plans-tasks-done",
            with_a_scrum_master_and_an_architect,
        );
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        // CTV-3 builds in the plan's lane: it names the plan, but its path is a lane's, not the plan.
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |wire| {
            wire["plan"] = json!("docs/catervas/architecture/plans/CTV-1.md");
            wire["lane"] = json!(1);
            wire["allowed_paths"] = json!(["src/**"]);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        for pair in ["ready", "assigned", "in_progress", "verifying", "accepted"].windows(2) {
            harness.project.moved("CTV-3", pair[0], pair[1], &people);
        }
        integrated(&harness, false);
        harness.project.record(
            "CTV-3",
            "task.integrated",
            &json!({ "sha": "abd", "into": "main", "integrated_by": "governor" }),
        );
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let about_the_epic: Vec<_> = started
            .iter()
            .filter(|spec| spec.task_id == Some(task("CTV-1")))
            .map(|spec| (spec.agent_id.as_str(), spec.purpose))
            .collect();
        assert_eq!(about_the_epic, [("sam", SessionPurpose::Plan)]);
        let spec = started
            .iter()
            .find(|spec| spec.task_id == Some(task("CTV-1")))
            .expect("a session about the epic");
        assert!(
            spec.initial_prompt
                .starts_with("Every task under the epic CTV-1 is done"),
            "{}",
            spec.initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_down_again_when_the_plan_is_missing() {
        let harness = Harness::new("epic-plan-missing", with_a_scrum_master_and_an_architect);
        a_scrum_masters_epic_in_progress(&harness);
        an_accepted_plan_task(&harness);
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        // The plan task is accepted, and its plan is on no branch.
        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let spec = started
            .iter()
            .find(|spec| spec.task_id == Some(task("CTV-1")))
            .expect("a session about the epic");
        assert!(
            spec.initial_prompt
                .starts_with("Plan the epic CTV-1 with the Architect"),
            "{}",
            spec.initial_prompt
        );
    }

    /// The Scrum Master's epic CTV-1 `verifying` after its child CTV-2 was accepted, each with its
    /// completion note.
    fn a_scrum_masters_epic_verifying(harness: &Harness) {
        a_scrum_masters_epic_in_progress(harness);
        a_child(harness, "accepted");
        harness.project.record(
            "CTV-2",
            "note.written",
            &json!({ "kind": "completion", "text": "Wrote done.txt at the root.", "written_by": "dev-a" }),
        );
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "completion", "text": "CTV-2 added done.txt; nothing left out.", "written_by": "sam" }),
        );
        let mut people = the_scrum_masters();
        people["actor"] = json!("assignee");
        harness
            .project
            .moved("CTV-1", "in_progress", "verifying", &people);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_the_humans_acceptance_of_an_epic_before_catervas_ran_its_criteria() {
        let harness = Harness::new("epic-sm-accept-early", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let refused = orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string()),
            })
            .await;

        assert!(
            matches!(&refused, Err(CommandError::Refused { reason })
                if reason.starts_with("criteria_not_run") && reason.contains("C1")),
            "{refused:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn has_the_product_manager_review_an_epic() {
        let harness = Harness::new("epic-sm-review", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        let adapter = harness.recorded(vec![review_epic_ctv_1(), accept_ctv_1()]);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());

        orchestrator.tick().await.expect("Catervas runs C1");
        let runs = governor_runs(&harness, "CTV-1");
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert!(
            runs[0]
                .2
                .starts_with(&format!("at {}", main_head(&harness)))
        );
        assert!(adapter.started().is_empty());

        orchestrator.tick().await.expect("the tick runs");
        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
        assert_eq!(spec.task_id, Some(task("CTV-1")));
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert_eq!(witness.had_executor(), vec![false]);
        for held in ["CTV-2", "accepted", "Wrote done.txt at the root."] {
            assert!(
                spec.initial_prompt.contains(held),
                "{held}: {}",
                spec.initial_prompt
            );
        }
        assert!(
            !spec.initial_prompt.contains("<untrusted source=\"diff\">"),
            "{}",
            spec.initial_prompt
        );

        // The review passed; the epic waits for the human.
        let waiting = orchestrator.tick().await.expect("the tick runs");
        assert!(is_idle(&waiting), "{waiting:?}");
        assert_eq!(adapter.started().len(), 1);
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("It reads right.".to_string()),
            })
            .await
            .expect("the human accepts the epic");

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(
            (started[1].purpose, started[1].agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
        assert!(
            started[1].initial_prompt.contains("Request `accepted`"),
            "{}",
            started[1].initial_prompt
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
        let reviews = harness.events(&[EventKind::ReviewRecorded]);
        assert_eq!(reviews.len(), 1, "{reviews:?}");
        assert!(matches!(
            &reviews[0].body,
            EventBody::ReviewRecorded(body)
                if body.reviewer == "pm" && body.criteria_run == 2 && body.passed
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn rejects_an_epic_whose_criterion_the_product_manager_failed() {
        let harness = Harness::new("epic-sm-rejects", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        let adapter = harness.recorded(vec![
            review_epic_fails_ctv_1(),
            replays_catervas_read_board(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");
        orchestrator
            .tick()
            .await
            .expect("the Product Manager reviews");

        orchestrator.tick().await.expect("the tick runs");

        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("verifying".to_string(), "rejected".to_string())
        );
        assert_eq!(body.actor, TransitionActorWire::Reviewer);
        assert_eq!(body.requested_by, "pm");
        let rejection = body.rejection.as_ref().expect("the rejection");
        assert_eq!(rejection.failed_criterion_ids, vec!["C2".to_string()]);

        orchestrator.tick().await.expect("the governor returns it");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::InProgress);
        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        let spec = &started[1];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "sam")
        );
        for held in [
            "C2",
            "done.txt is empty, and the request asked it to say the run is done.",
            "file the tasks that fix it",
        ] {
            assert!(
                spec.initial_prompt.contains(held),
                "{held}: {}",
                spec.initial_prompt
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_the_product_manager_again_for_an_unanswered_epic_criterion() {
        let harness = Harness::new("epic-sm-unanswered", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        // The Product Manager's review note, with no result for the `review` criterion C2.
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "It reads right.", "written_by": "pm" }),
        );
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
        assert!(
            spec.initial_prompt.contains("Still unanswered: C2."),
            "{}",
            spec.initial_prompt
        );
        assert!(
            !spec.initial_prompt.contains("Request `accepted`"),
            "{}",
            spec.initial_prompt
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Verifying);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_an_epics_reviewer_when_the_scrum_master_is_paused() {
        // The epic was assigned while `sam` was active; `sam` is paused since.
        let harness = Harness::new("epic-sm-paused", |wire| {
            with_a_scrum_master(wire);
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .last_mut()
                .expect("sam")["status"] = json!("paused");
        });
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        let adapter = harness.recorded(vec![review_epic_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(
            started.len(),
            1,
            "the Product Manager reviews it, not the human"
        );
        assert_eq!(
            (started[0].purpose, started[0].agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
        assert!(
            started[0].initial_prompt.contains("The tasks under it"),
            "{}",
            started[0].initial_prompt
        );
        let reviews = harness.events(&[EventKind::ReviewRecorded]);
        assert!(
            matches!(&reviews[..], [review] if matches!(&review.body,
                EventBody::ReviewRecorded(body) if body.reviewer == "pm")),
            "{reviews:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_a_human_reviewed_epic_with_the_human_once_a_scrum_master_activates() {
        // The epic was assigned with no active Scrum Master, so the human reviews it; `sam`
        // joins the team paused, and is activated only once the human has accepted the epic.
        let harness = Harness::new("epic-human-sm-later", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(json!({
                    "id": "sam",
                    "display_name": "sam",
                    "role": "scrum_master",
                    "status": "paused"
                }));
        });
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let adapter = harness.recorded(vec![accept_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string()),
            })
            .await
            .expect("the human accepts the epic");

        orchestrator
            .handle(Command::AgentUpdate {
                agent_id: "sam".to_string(),
                status: AgentStatus::Active,
            })
            .await
            .expect("sam activates");
        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(
            started.len(),
            1,
            "the human's own accepted epic still gets its Product Manager verify session"
        );
        assert_eq!(
            (started[0].purpose, started[0].agent_id.as_str()),
            (SessionPurpose::Verify, "pm")
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_the_epics_review_once_the_reasked_criterion_is_answered() {
        let harness = Harness::new("epic-sm-reasked-recorded", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        // The Product Manager's review note, with no result yet for the `review` criterion C2.
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "It reads right.", "written_by": "pm" }),
        );
        let adapter = harness.recorded(vec![review_epic_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Catervas runs C1");

        orchestrator.tick().await.expect("the re-ask answers C2");

        let reviews = harness.events(&[EventKind::ReviewRecorded]);
        assert!(
            matches!(&reviews[..], [review] if matches!(&review.body,
                EventBody::ReviewRecorded(body) if body.reviewer == "pm")),
            "{reviews:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_out_plainly_once_the_fixing_tasks_are_filed() {
        let harness = Harness::new("epic-sm-fixed", with_a_scrum_master);
        a_scrum_masters_epic_in_progress(&harness);
        a_child(&harness, "accepted");
        integrated(&harness, true);
        let people = the_scrum_masters();
        harness
            .project
            .moved("CTV-1", "in_progress", "verifying", &people);
        let mut rejected = people.clone();
        rejected["actor"] = json!("reviewer");
        rejected["requested_by"] = json!("pm");
        rejected["rejection"] =
            json!({ "failed_criterion_ids": ["C2"], "reasons": "done.txt is empty." });
        harness
            .project
            .moved("CTV-1", "verifying", "rejected", &rejected);
        let mut returned = people.clone();
        returned["iteration"] = json!(1);
        harness
            .project
            .moved("CTV-1", "rejected", "in_progress", &returned);
        // The fixing task, filed after the rejection and accepted.
        harness.file_under("CTV-3", "ready", Some("CTV-1"), |_| {});
        let child = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        for pair in ["ready", "assigned", "in_progress", "verifying", "accepted"].windows(2) {
            harness.project.moved("CTV-3", pair[0], pair[1], &child);
        }
        harness.project.record(
            "CTV-3",
            "task.integrated",
            &json!({ "sha": "abd", "into": "main", "integrated_by": "governor" }),
        );
        let adapter = harness.recorded(vec![replays_catervas_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "sam")
        );
        assert!(
            spec.initial_prompt
                .starts_with("Every task under the epic CTV-1 is done"),
            "{}",
            spec.initial_prompt
        );
        for gone in ["<untrusted source=\"rejection\">", "done.txt is empty."] {
            assert!(
                !spec.initial_prompt.contains(gone),
                "{gone}: {}",
                spec.initial_prompt
            );
        }
    }

    /// An hour after the fixtures' now.
    fn in_an_hour() -> chrono::DateTime<chrono::Utc> {
        crate::tools::fixtures::at() + chrono::Duration::hours(1)
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_triager() {
        let harness = Harness::new("req-sleep-triage", |_| {});
        harness.a_request("Add done.txt and its check");
        harness.asleep("pm", in_an_hour());
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "pm", in_an_hour()).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_product_manager_to_refine() {
        let harness = Harness::new("req-sleep-refine", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        refining(&harness, &orchestrator, RequestSize::Small).await;
        harness.asleep("pm", in_an_hour());

        waits_for(&orchestrator, "pm", in_an_hour()).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_epic_assignee() {
        let harness = Harness::new("req-sleep-epic", |_| {});
        an_epic_in_progress(&harness, |_| {});
        harness.asleep("pm", in_an_hour());
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "pm", in_an_hour()).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_product_manager_to_accept_an_epic() {
        let harness = Harness::new("req-sleep-epic-accept", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("Catervas runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("CTV-1"),
                subject: AcceptSubject::Result,
                message: Some("Both look right.".to_string()),
            })
            .await
            .expect("the human accepts the epic");
        harness.asleep("pm", in_an_hour());

        waits_for(&orchestrator, "pm", in_an_hour()).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_product_manager_to_review_an_epic() {
        let harness = Harness::new("req-sleep-epic-review", with_a_scrum_master);
        a_scrum_masters_epic_verifying(&harness);
        integrated(&harness, true);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("Catervas runs C1");
        harness.asleep("pm", in_an_hour());

        waits_for(&orchestrator, "pm", in_an_hour()).await;
    }
}
