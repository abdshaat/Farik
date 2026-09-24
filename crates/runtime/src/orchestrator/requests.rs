//! Requests and epics (`docs/SPEC.md` sections 5.2, 5.16, and ADR 0013): the rules that take a
//! request from `draft` through triage and refining to `ready`.

use std::sync::Arc;

use farik_core::contract::{
    ExitCriterion, Role, TaskContract, TaskId, TaskKind, TaskStatus, VerificationWire, wire_method,
};
use farik_core::governor::done::{CriterionResult, RunBy};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_protocol::event::{
    ContractEvaluatedBodyGate, CriterionRecordedBody, CriterionRecordedBodyRunBy, EventBody,
    EventKind, FarikEvent, ReviewRecordedBody,
};
use farik_store::files::FilesError;
use farik_store::{EventQuery, TaskProjection};

use super::messages::{
    breakdown_message, close_out_message, epic_accept_message, refine_message, triage_message,
};
use super::rules::{Room, acted, active, refused_since_entering, room};
use super::session::{SessionAsk, run_session};
use super::verify::{
    GOVERNOR, append, escalate, fails_the_criterion, governor_results, ran_criteria, read_only,
};
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport};
use crate::criteria::{CriterionOutcome, remove_base_worktree, run_criteria};
use crate::session::SessionPurpose;
use crate::tools::ToolDeps;
use crate::transitions::{
    TransitionAsk, TransitionOutcome, integration_branch, refusal_details, result_accepted,
};

/// The human, as the reviewer of an epic the Product Manager broke down (5.16 item 4).
const HUMAN: &str = "human";

/// The Product Manager: the first active agent of that role in team-file order.
pub(super) fn product_manager(team: &Team) -> Option<&Agent> {
    team.active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
}

/// Rule 10: a `draft`. Untriaged, it gets the Product Manager's triage session; triaged, it is
/// moved to `refining` on the Product Manager's behalf, with no session, unless that move was
/// refused since the task became a draft.
pub(super) async fn draft(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    if !row.triaged {
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        if room(deps, team, &contract, day_spent)? != Room::Free {
            return Ok(None);
        }
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: pm,
                contract: &contract,
                purpose: SessionPurpose::Triage,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: false,
                initial_prompt: triage_message(&contract),
            },
        )
        .await?;
        return Ok(Some(acted(row, pm, "triage", &end)));
    }
    if refused_since_entering(deps, &row.task_id, TaskStatus::Draft, TaskStatus::Refining)? {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
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
/// refining began, is judged: `escalated` asked as the governor first, whose rows open on three
/// readiness failures or on a passing contract the human must approve, then `ready`. Otherwise the
/// Product Manager gets a refine session.
pub(super) async fn refining(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(row.task_id.clone()),
        ..EventQuery::default()
    })?;
    let began = refining_began(&history);
    if is_to_be_judged(&contract, &history, began) {
        return judge(deps, team, row).map(Some);
    }
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(None);
    }
    let asked = history
        .iter()
        .any(|event| event.envelope.seq > began && event.body.kind() == EventKind::QuestionAsked);
    let failures = last_readiness_failures(&history, began);
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: pm,
            contract: &contract,
            purpose: SessionPurpose::Refine,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            initial_prompt: refine_message(&contract, !asked, &failures),
        },
    )
    .await?;
    Ok(Some(acted(row, pm, "refine", &end)))
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
            &TransitionRequest {
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

/// Where refining last began: the later of the task's last move into `refining` and its last
/// triage, or 0.
fn refining_began(history: &[FarikEvent]) -> u64 {
    history
        .iter()
        .filter(|event| match &event.body {
            EventBody::TaskTransitioned(body) => body.to.to_string() == "refining",
            EventBody::RequestTriaged(_) => true,
            _ => false,
        })
        .map(|event| event.envelope.seq)
        .max()
        .unwrap_or(0)
}

/// Whether the contract is judged now: written since refining began with no refusal by the
/// governor since that write, or, with no such write, filed whole and not refused since refining
/// began. A raw request is not judged: its brief would fail and spend one of the three attempts.
fn is_to_be_judged(contract: &TaskContract, history: &[FarikEvent], began: u64) -> bool {
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
    match written {
        Some(written) => !refused_after(written),
        None => (contract.parent.is_some() || contract.locked) && !refused_after(began),
    }
}

/// The failures of the last Definition of Ready judged since refining began, when it failed.
fn last_readiness_failures(history: &[FarikEvent], began: u64) -> Vec<String> {
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

/// Rule 8 for an epic `ready`: assigned to the Product Manager on its behalf, with no reviewer
/// (the human reviews it) and no session, when it holds fewer open tasks than the WIP limit,
/// counted as the assignment gate counts them; otherwise passed over, so that the run idles rather
/// than being refused on every tick.
pub(super) fn ready_epic(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    let held = board
        .iter()
        .filter(|other| other.assignee_id.as_deref() == Some(pm.id.as_str()))
        .filter(|other| !matches!(other.status, TaskStatus::Accepted | TaskStatus::Cancelled))
        .count();
    if u64::try_from(held).unwrap_or(u64::MAX)
        >= u64::try_from(team.policy.wip_limit_per_agent).unwrap_or(0)
        || refused_since_entering(deps, &row.task_id, TaskStatus::Ready, TaskStatus::Assigned)?
    {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Assigned,
            actor: TransitionActor::ProductManager,
            agent_id: Some(pm.id.to_string()),
        },
        &TransitionAsk {
            assignee_id: Some(pm.id.to_string()),
            ..TransitionAsk::default()
        },
        team,
    )?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("assigned the epic to {}", pm.id.as_str()),
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
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::InProgress,
            actor: TransitionActor::Assignee,
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

/// Rule 6 for an epic `in_progress`: its assignee's plan session to close it out when every task
/// under it is accepted or cancelled and one is accepted, or to break it down when it has no task
/// but cancelled ones; otherwise no rule, its tasks being worked.
pub(super) async fn in_progress_epic(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    let tasks: Vec<&TaskProjection> = board
        .iter()
        .filter(|other| other.parent.as_ref() == Some(&row.task_id))
        .collect();
    let done = |other: &&TaskProjection| {
        matches!(other.status, TaskStatus::Accepted | TaskStatus::Cancelled)
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let initial_prompt = if tasks.iter().all(done)
        && tasks
            .iter()
            .any(|other| other.status == TaskStatus::Accepted)
    {
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
        close_out_message(&contract, &listed)
    } else if tasks
        .iter()
        .all(|other| other.status == TaskStatus::Cancelled)
    {
        breakdown_message(&contract)
    } else {
        return Ok(None);
    };
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(None);
    }
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assignee,
            contract: &contract,
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            initial_prompt,
        },
    )
    .await?;
    Ok(Some(acted(row, assignee, "plan", &end)))
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

/// Rule 5 for an epic the human reviews (ADR 0013): nothing while a task under it awaits
/// integration; then Farik runs each mechanical criterion it has not run in this verification, on
/// the integration branch's head; then nothing until the human accepts; then the Product Manager's
/// `verify` session, told the human accepted, after which the epic's review is recorded.
pub(super) async fn verifying_epic(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    contract: &TaskContract,
    history: &[FarikEvent],
    since: u64,
    day_spent: &mut bool,
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
        .filter(|criterion| {
            matches!(
                wire_method(&criterion.verification),
                Some("command" | "test" | "artifact")
            )
        })
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
    if result_accepted(history).is_none() {
        return Ok(None);
    }
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    if room(deps, team, contract, day_spent)? != Room::Free {
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

/// What Farik's runs of an epic's criteria came to.
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
            .join(".farik/local/worktrees")
            .join(format!("{}-base", contract.id.as_str()));
        let unrunnable = |id: &str, error: &dyn std::fmt::Display| {
            EpicRan::Unrunnable(format!(
                "Farik could not run {id} on the integration branch for the human: {error}"
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

/// Records one of Farik's runs of an epic's criterion as the reviewer's, recorded by the governor,
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
/// Farik's runs passed. The one review not recorded at a reviewer session's end.
fn record_epic_review(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    session_id: &str,
    since: u64,
) -> Result<(), OrchestratorError> {
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(contract.id.clone()),
        ..EventQuery::default()
    })?;
    if history.iter().any(|event| {
        event.envelope.seq > since && matches!(event.body, EventBody::ReviewRecorded(_))
    }) {
        return Ok(());
    }
    let context = deps.tools.transitions.context(
        &TransitionRequest {
            task_id: contract.id.clone(),
            to: TaskStatus::Accepted,
            actor: TransitionActor::ProductManager,
            agent_id: None,
        },
        &TransitionAsk::default(),
        team,
    )?;
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

    use farik_core::contract::{TaskKind, TaskStatus};
    use farik_core::team::Effort;
    use farik_protocol::command::{AcceptSubject, Command, RequestSize};
    use farik_protocol::event::{
        CriterionRecordedBodyRunBy, EscalationRaisedBodyReason, EventBody, EventKind, FarikEvent,
        NewEvent, NoteWrittenBodyKind, RequestTriagedBodySize, TransitionActorWire,
        event_from_value,
    };
    use farik_store::git::fixtures::git_output_in;
    use serde_json::{Value, json};

    use crate::exec::ExecError;
    use crate::orchestrator::TRIAGE_MODEL;
    use crate::orchestrator::fixtures::{
        BrokenSandboxFactory, CountingSandboxFactory, ExecutorWitness, Harness,
    };
    use crate::orchestrator::{CommandError, Orchestrator, TickReport};
    use crate::recorded::fixtures::{
        accept_frk_1, implement_finishes_frk_1, plan_assigns_frk_1, plan_assigns_frk_2,
        plan_breaks_down_frk_1, plan_closes_epic_frk_1, refine_asks_frk_1,
        refine_writes_epic_frk_1, refine_writes_task_frk_1, replays_farik_read_board,
        triage_frk_1_large,
    };
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;

    fn task(id: &str) -> farik_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    fn last(harness: &Harness, kind: EventKind) -> Option<FarikEvent> {
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

    /// A request filed by the human and sized by them, `size`, as `farik triage` does.
    async fn a_sized_request(harness: &Harness, orchestrator: &Orchestrator, size: RequestSize) {
        harness.a_request("Add done.txt and its check");
        orchestrator
            .handle(Command::RequestTriage {
                task_id: task("FRK-1"),
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
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Refining);
        report
    }

    /// Calls a Farik tool as `agent` on `task`, as a session of theirs would.
    async fn call(
        harness: &Harness,
        agent: &str,
        task: Option<&str>,
        tool: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::tools::ToolError> {
        crate::tools::call_tool(&harness.project.context(agent, task), tool, input).await
    }

    /// The failing write the tests seed: a budget of 50 dollars, over the 20 the team's day has and
    /// over a cap of 5 dollars where a test sets one.
    async fn write_over_the_cap(harness: &Harness) {
        call(
            harness,
            "pm",
            Some("FRK-1"),
            "farik_write_contract",
            json!({ "fields": { "budget": { "max_cost_usd": 50 } } }),
        )
        .await
        .expect("the schema allows the write");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn triages_a_request_on_the_cheaper_model() {
        let harness = Harness::new("req-triage", |_| {});
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![triage_frk_1_large()]);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());

        orchestrator.tick().await.expect("the tick runs");
        // The daemon holds the session to its one tool, whatever the agent's tiers allow.
        assert_eq!(
            witness.given_tools(),
            vec![vec!["farik_triage_request".to_string()]]
        );

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Triage);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.model, TRIAGE_MODEL);
        assert_eq!(spec.effort, Effort::Low);
        assert_eq!(spec.farik_tools, vec!["farik_triage_request".to_string()]);
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
        let triaged = last(&harness, EventKind::RequestTriaged).expect("the triage");
        let EventBody::RequestTriaged(body) = &triaged.body else {
            panic!("a triage");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Large);
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(harness.row("FRK-1").kind, TaskKind::Epic);
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
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Assigned);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_first_when_refining_an_epic() {
        let harness = Harness::new("req-asks-first", |_| {});
        let adapter = harness.recorded(vec![refine_asks_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Refine);
        assert!(
            started[0].initial_prompt.contains("farik_ask_human"),
            "{}",
            started[0].initial_prompt
        );
        assert!(last(&harness, EventKind::QuestionAsked).is_some());
        assert!(harness.row("FRK-1").waiting_on_human);
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_the_answer_to_the_next_refine_session() {
        let harness = Harness::new("req-answer-handed", |_| {});
        let adapter = harness.recorded(vec![
            refine_asks_frk_1(),
            replays_farik_read_board(),
            replays_farik_read_board(),
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
        let adapter = harness.recorded(vec![refine_writes_task_frk_1()]);
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
    async fn readies_a_written_contract_whatever_the_day_cost_without_a_daily_budget() {
        let harness = Harness::new("req-no-day", |wire| wire["budgets"] = json!({}));
        harness.spent(None, "s-0", 25.0);
        let adapter = harness.recorded(vec![refine_writes_task_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Ready);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn returns_a_failing_contract_with_its_failures() {
        let harness = Harness::new("req-failing", |wire| {
            wire["rules"]["max_task_budget_usd"] = json!(5);
        });
        let adapter = harness.recorded(vec![replays_farik_read_board()]);
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
            "FRK-1",
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
        let harness = Harness::new("req-judges-again", |_| {});
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
        let harness = Harness::new("req-judges-others-refusal", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        write_over_the_cap(&harness).await;
        harness.project.record(
            "FRK-1",
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
                task_id: task("FRK-1"),
            })
            .await
            .expect("the human takes the contract");
        orchestrator.tick().await.expect("refining starts");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Refining);

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
            "FRK-1",
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
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Draft);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_an_epic_for_approval_once_it_passes() {
        let harness = Harness::new("req-approval", |_| {});
        let adapter = harness.recorded(vec![refine_writes_epic_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the epic is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "FRK-1").last().map(String::as_str),
            Some("refining -> escalated")
        );
        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        assert!(matches!(
            &escalation.body,
            EventBody::EscalationRaised(body) if body.reason == EscalationRaisedBodyReason::Approval
        ));
        assert!(
            harness.events(&[EventKind::ContractEvaluated]).iter().all(
                |event| matches!(&event.body, EventBody::ContractEvaluated(body) if body.passed)
            )
        );
        assert!(harness.row("FRK-1").awaiting_approval);
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_contract_that_failed_three_times() {
        let harness = Harness::new("req-three-failures", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        refining(&harness, &orchestrator, RequestSize::Small).await;
        for _ in 0..3 {
            harness.project.record(
                "FRK-1",
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
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_child_filed_whole_without_a_session() {
        let harness = Harness::new("req-child-whole", |_| {});
        harness
            .project
            .filed_with("FRK-1", "in_progress", "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["allowed_paths"] = json!(["done.txt"]);
            });
        harness.project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        // The call `plan_breaks_down_frk_1` makes, made here without its session.
        let mut contract = Harness::request_fields("Add done.txt");
        contract["budget"] = json!({ "max_cost_usd": 2 });
        call(
            &harness,
            "pm",
            None,
            "farik_create_task",
            json!({ "parent": "FRK-1", "contract": contract }),
        )
        .await
        .expect("the epic's assignee files its task");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "FRK-2"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.task_id != Some(task("FRK-2")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_waiting_on_the_human() {
        let harness = Harness::new("req-waiting", |_| {});
        harness.ready("FRK-1");
        harness.project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Which file?", "asked_by": "pm" }),
        );
        harness.ready("FRK-2");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started()[0].task_id, Some(task("FRK-2")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catches_up_with_another_process_before_each_tick() {
        let harness = Harness::new("req-catch-up", |_| {});
        let adapter = harness.recorded(vec![refine_asks_frk_1(), replays_farik_read_board()]);
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
            "team_id": "farik",
            "project_id": "farik",
            "task_id": "FRK-1",
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
        assert_eq!(started[1].task_id, Some(task("FRK-1")));
    }

    /// Epic FRK-1 as the Product Manager wrote it for the human to review, filed in `status`: C1
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

    /// Epic FRK-1 moved to `in_progress`, held by `pm`.
    fn an_epic_in_progress(harness: &Harness, change: impl FnOnce(&mut Value)) {
        an_epic(harness, "FRK-1", "ready", change);
        let people = json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "pm" });
        harness.project.moved("FRK-1", "ready", "assigned", &people);
        harness
            .project
            .moved("FRK-1", "assigned", "in_progress", &people);
    }

    /// FRK-2 under FRK-1, filed `ready`, moved through to `status` as `dev-a`'s reviewed by
    /// `dev-b`, its worktree made when it is in progress.
    fn a_child(harness: &Harness, status: &str) {
        harness.file_under("FRK-2", "ready", Some("FRK-1"), |wire| {
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
            harness.project.moved("FRK-2", pair[0], pair[1], &people);
            if pair[1] == "in_progress" {
                harness
                    .project
                    .deps
                    .git
                    .create_worktree(&harness.worktree("FRK-2"), "farik/FRK-2", "main")
                    .expect("the child's worktree is made");
            }
        }
        if cancelled {
            harness
                .project
                .moved("FRK-2", "ready", "cancelled", &json!({}));
        }
        if matches!(status, "accepted" | "cancelled") && harness.worktree("FRK-2").exists() {
            harness
                .project
                .deps
                .git
                .remove_worktree(&harness.worktree("FRK-2"))
                .expect("the child's worktree is removed");
        }
    }

    /// FRK-2 recorded as integrated into `main`, with `done.txt` committed there when `with_done`.
    fn integrated(harness: &Harness, with_done: bool) {
        if with_done {
            harness.commit_at_root("done.txt", "", "Add done.txt");
        }
        harness.project.record(
            "FRK-2",
            "task.integrated",
            &json!({ "sha": "abc", "into": "main", "integrated_by": "governor" }),
        );
    }

    /// Epic FRK-1 `verifying` after its child FRK-2 was accepted, with its completion note.
    fn an_epic_verifying(harness: &Harness, change: impl FnOnce(&mut Value)) {
        an_epic_in_progress(harness, change);
        a_child(harness, "accepted");
        harness.project.record(
            "FRK-1",
            "note.written",
            &json!({ "kind": "completion", "text": "FRK-2 added done.txt; nothing left out.", "written_by": "pm" }),
        );
        harness.project.moved(
            "FRK-1",
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
    async fn assigns_an_approved_epic_to_its_product_manager() {
        let harness = Harness::new("epic-assigns", |_| {});
        an_epic(&harness, "FRK-1", "ready", |_| {});
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
            moves_of(&harness, "FRK-1").last().map(String::as_str),
            Some("assigned -> in_progress")
        );
        assert!(!harness.worktree("FRK-1").exists());
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_to_assign_a_second_epic_while_the_first_is_open() {
        let harness = Harness::new("epic-wip", |_| {});
        an_epic(&harness, "FRK-1", "ready", |_| {});
        an_epic(&harness, "FRK-2", "ready", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let first = orchestrator.tick().await.expect("the tick runs");
        let second = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        // FRK-1's breakdown has begun and its one task waits on a block, so rule 6 has nothing
        // to do for it, and rule 8 is reached.
        harness.file_under("FRK-3", "ready", Some("FRK-1"), |_| {});
        harness.project.moved(
            "FRK-3",
            "in_progress",
            "blocked",
            &json!({
                "assignee": "dev-a",
                "reviewer": "dev-b",
                "blocker": { "description": "the API is down", "needed": "the API" }
            }),
        );
        let third = orchestrator.tick().await.expect("the tick runs");

        assert!(matches!(&first, TickReport::Acted { task_id, .. } if task_id.as_str() == "FRK-1"));
        assert!(
            matches!(&second, TickReport::Acted { task_id, .. } if task_id.as_str() == "FRK-1")
        );
        assert!(is_idle(&third), "{third:?}");
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Ready);
        assert!(
            harness
                .events(&[EventKind::TransitionRefused])
                .iter()
                .all(|event| event.envelope.ids.task_id != Some(task("FRK-2")))
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_an_epic_down_in_a_plan_session() {
        let harness = Harness::new("epic-breakdown", |_| {});
        an_epic_in_progress(&harness, |_| {});
        let adapter = harness.recorded(vec![plan_breaks_down_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.task_id, Some(task("FRK-1")));
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert!(
            spec.initial_prompt.contains("farik_create_task"),
            "{}",
            spec.initial_prompt
        );
        let child = harness.row("FRK-2");
        assert_eq!(child.parent, Some(task("FRK-1")));
        assert!(child.triaged);
        assert_eq!(child.status, TaskStatus::Draft);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_an_epics_task_through_its_assignee() {
        let harness = Harness::new("epic-assigns-child", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "ready");
        let adapter = harness.recorded(vec![plan_assigns_frk_2()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.task_id, Some(task("FRK-2")));
        let child = harness.row("FRK-2");
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
        let adapter = harness.recorded(vec![implement_finishes_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "FRK-2")
        );
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.task_id != Some(task("FRK-1")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_an_epic_whose_tasks_are_done() {
        let harness = Harness::new("epic-closes", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "accepted");
        integrated(&harness, true);
        let adapter = harness.recorded(vec![plan_closes_epic_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.task_id, Some(task("FRK-1")));
        assert!(
            spec.initial_prompt.contains("FRK-2") && spec.initial_prompt.contains("accepted"),
            "{}",
            spec.initial_prompt
        );
        let note = last(&harness, EventKind::NoteWritten).expect("the completion note");
        assert!(matches!(
            &note.body,
            EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Completion
        ));
        assert_eq!(note.envelope.ids.task_id, Some(task("FRK-1")));
        assert_eq!(
            moves_of(&harness, "FRK-1").last().map(String::as_str),
            Some("in_progress -> verifying")
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_down_again_when_every_task_was_cancelled() {
        let harness = Harness::new("epic-breaks-down-again", |_| {});
        an_epic_in_progress(&harness, |_| {});
        a_child(&harness, "cancelled");
        let adapter = harness.recorded(vec![replays_farik_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.task_id, Some(task("FRK-1")));
        // The breakdown's message, not the close-out's, which names `farik_create_task` too.
        assert!(
            spec.initial_prompt.starts_with("Break the epic FRK-1 down"),
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
        assert!(harness.row("FRK-2").awaiting_integration);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(is_idle(&report), "{report:?}");
        assert!(governor_runs(&harness, "FRK-1").is_empty());
        assert!(!harness.worktree("FRK-1-base").exists());
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

        let runs = governor_runs(&harness, "FRK-1");
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert_eq!((runs[0].0.as_str(), runs[0].1), ("C1", true));
        let recorded = last(&harness, EventKind::CriterionRecorded).expect("the run");
        assert!(matches!(
            &recorded.body,
            EventBody::CriterionRecorded(body) if body.run_by == CriterionRecordedBodyRunBy::Reviewer
        ));
        assert_eq!(sandboxes.based("FRK-1"), 1);
        assert!(!harness.worktree("FRK-1-base").exists());
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(governor_runs(&harness, "FRK-1").len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_the_humans_acceptance_to_the_product_manager() {
        let harness = Harness::new("epic-accepted", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let adapter = harness.recorded(vec![accept_frk_1()]);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());
        orchestrator.tick().await.expect("Farik runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("FRK-1"),
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
        let evidence = &governor_runs(&harness, "FRK-1")[0].2;
        let results = spec
            .initial_prompt
            .find("<untrusted source=\"results\">")
            .expect("Farik's results, marked");
        assert!(
            spec.initial_prompt[results..].contains(evidence.as_str()),
            "{}",
            spec.initial_prompt
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
        let review = last(&harness, EventKind::ReviewRecorded).expect("the review");
        assert_eq!(review.envelope.ids.task_id, Some(task("FRK-1")));
        assert!(matches!(
            &review.body,
            EventBody::ReviewRecorded(body)
                if body.reviewer == "human" && body.criteria_run == 2 && body.passed
        ));
        assert!(
            harness
                .events(&[EventKind::TaskIntegrated])
                .iter()
                .all(|event| event.envelope.ids.task_id != Some(task("FRK-1")))
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

        let runs = governor_runs(&harness, "FRK-1");
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
        let adapter = harness.recorded(vec![replays_farik_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Farik runs C1");
        assert!(
            !governor_runs(&harness, "FRK-1")[0].1,
            "done.txt is not on main"
        );

        let failed = orchestrator
            .handle(Command::HumanAccept {
                task_id: task("FRK-1"),
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
                task_id: task("FRK-1"),
                to: TaskStatus::Escalated,
                reason: "C1 failed".to_string(),
            })
            .await
            .expect("the human escalates the epic");
        orchestrator
            .handle(Command::EscalationResolve {
                task_id: task("FRK-1"),
                to: TaskStatus::InProgress,
                message: "Add the missing file.".to_string(),
            })
            .await
            .expect("the human sends it back");

        orchestrator.tick().await.expect("the tick runs");

        let spec = &adapter.started()[0];
        assert_eq!(
            (spec.purpose, spec.agent_id.as_str()),
            (SessionPurpose::Plan, "pm")
        );
        assert_eq!(spec.task_id, Some(task("FRK-1")));
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
        harness.file_under("FRK-3", "ready", Some("FRK-1"), |_| {});
        harness
            .project
            .moved("FRK-3", "ready", "escalated", &json!({}));
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
        for (id, last) in [("FRK-2", "accepted"), ("FRK-3", "cancelled")] {
            an_epic(&harness, id, "ready", |_| {});
            harness.project.moved(id, "ready", "assigned", &people);
            harness
                .project
                .moved(id, "assigned", "in_progress", &people);
            harness.project.moved(id, "in_progress", last, &people);
        }
        an_epic(&harness, "FRK-4", "ready", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(moves_of(&harness, "FRK-4"), ["ready -> assigned"]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_an_epic_whose_criterion_farik_could_not_run() {
        let harness = Harness::new("epic-unrunnable", |_| {});
        an_epic_verifying(&harness, |_| {});
        integrated(&harness, true);
        let sandboxes = Arc::new(BrokenSandboxFactory::for_base(ExecError::SpawnFailed {
            detail: "no shell".to_string(),
        }));
        let orchestrator = harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        let EventBody::EscalationRaised(body) = &escalation.body else {
            panic!("an escalation");
        };
        assert!(body.detail.contains("C1"), "{}", body.detail);
        assert!(body.detail.contains("no shell"), "{}", body.detail);
        assert!(governor_runs(&harness, "FRK-1").is_empty());
        assert!(!harness.worktree("FRK-1-base").exists());
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
        let adapter =
            harness.recorded(vec![replays_farik_read_board(), replays_farik_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("Farik runs C1");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: task("FRK-1"),
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
}
