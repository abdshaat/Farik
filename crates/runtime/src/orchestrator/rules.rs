//! The order of work, and what each state on the board asks of the orchestrator. A tick takes the
//! rules in order, one per state, and each rule the tasks in its state by their number; the first
//! that acts ends the tick.

use std::sync::Arc;

use farik_core::budget::{BudgetScope, SessionLedger, check_budgets};
use farik_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_protocol::event::{EventBody, EventKind, FarikEvent};
use farik_store::{EventQuery, Git, TaskProjection};

use super::messages::{Resume, implement_message, plan_message};
use super::session::{SessionAsk, SessionEnd, run_session};
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, worktree};
use crate::cost::budget_state;
use crate::exec::Executor;
use crate::session::{EndReason, SessionPurpose};
use crate::transitions::{TransitionAsk, TransitionOutcome, integration_branch, refusal_details};

/// What a tick says when no rule matched.
const NOTHING_TO_DO: &str = "nothing on the board needs doing";
/// What a tick says when the only rules that matched would have started a session on a spent day.
const DAY_SPENT: &str = "the team's daily budget is spent";

/// Whether a budget stops a session from starting for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    /// Nothing stops it.
    Free,
    /// The task's dollars or sessions are spent: no session for this task.
    TaskSpent,
    /// The team's day is spent: no session at all.
    DaySpent,
}

/// One tick: the first rule that acts, or `Idle`.
pub(super) async fn tick(orchestrator: &Orchestrator) -> Result<TickReport, OrchestratorError> {
    let deps = &orchestrator.deps;
    let team = deps.tools.files.read_team()?;
    let mut board = deps.tools.projections.board()?;
    board.sort_by_key(|row| task_number(row.task_id.as_str()));
    let mut day_spent = false;
    if let Some(row) = board.iter().find(|row| row.status == TaskStatus::Rejected) {
        return rejected(deps, &team, row);
    }
    for row in board.iter().filter(|row| row.status == TaskStatus::Blocked) {
        if let Some(report) = blocked(deps, &team, row)? {
            return Ok(report);
        }
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::InProgress)
    {
        if let Some(report) = in_progress(orchestrator, &team, row, &mut day_spent).await? {
            return Ok(report);
        }
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Assigned)
    {
        if let Some(report) = assigned(deps, &team, row)? {
            return Ok(report);
        }
    }
    for row in board.iter().filter(|row| row.status == TaskStatus::Ready) {
        if let Some(report) = ready(deps, &team, &board, row, &mut day_spent).await? {
            return Ok(report);
        }
    }
    Ok(TickReport::Idle {
        why: if day_spent { DAY_SPENT } else { NOTHING_TO_DO }.to_string(),
    })
}

/// Rule 3: a task `rejected` goes back to `in_progress` for its next iteration, or, when the
/// governor refuses that at the iteration limit, to `escalated`; both asked as the governor, whose
/// effects count the iteration and raise the escalation.
fn rejected(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<TickReport, OrchestratorError> {
    let what = match governor_moves(deps, team, row, TaskStatus::InProgress)? {
        TransitionOutcome::Moved(_) => "returned it to its assignee for another iteration".into(),
        TransitionOutcome::Refused(_) => {
            match governor_moves(deps, team, row, TaskStatus::Escalated)? {
                TransitionOutcome::Moved(_) => {
                    "escalated it: it was rejected as often as it may be".to_string()
                }
                TransitionOutcome::Refused(refusal) => format!(
                    "the governor would neither return nor escalate it: {}",
                    refusal_details(&refusal).join("; ")
                ),
            }
        }
    };
    Ok(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    })
}

/// Rule 4: a task `blocked` for at least the team's `blocked_limit_hours` is escalated as the
/// governor, whose `BlockedAge` gate decides; a younger block has no rule.
fn blocked(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(blocked_at) = last_move_into(deps, &row.task_id, TaskStatus::Blocked)?
        .map(|event| event.envelope.recorded_at)
    else {
        return Ok(None);
    };
    let hours = i64::try_from(team.policy.blocked_limit_hours.get()).unwrap_or(i64::MAX);
    if deps.tools.clock.now() - blocked_at < chrono::Duration::hours(hours) {
        return Ok(None);
    }
    let what = match governor_moves(deps, team, row, TaskStatus::Escalated)? {
        TransitionOutcome::Moved(_) => format!("escalated it: blocked for {hours} hours or more"),
        TransitionOutcome::Refused(refusal) => format!(
            "the governor would not escalate it: {}",
            refusal_details(&refusal).join("; ")
        ),
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    }))
}

/// Asks the governor, as itself, to move the task to `to`.
fn governor_moves(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    to: TaskStatus,
) -> Result<TransitionOutcome, OrchestratorError> {
    Ok(deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to,
            actor: TransitionActor::Governor,
            agent_id: None,
        },
        &TransitionAsk::default(),
        team,
    )?)
}

/// The task's last `task.transitioned` into `status`.
fn last_move_into(
    deps: &OrchestratorDeps,
    task_id: &TaskId,
    status: TaskStatus,
) -> Result<Option<FarikEvent>, OrchestratorError> {
    let moves = deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned],
        ..EventQuery::default()
    })?;
    Ok(moves.into_iter().rev().find(|event| {
        matches!(&event.body, EventBody::TaskTransitioned(body) if body.to.to_string() == status.to_string())
    }))
}

/// Whether the budgets leave room for a session about `contract`, read with an empty session
/// ledger: the day's dollars, then the task's dollars and sessions. `day_spent` is set when the
/// day is what stops it.
fn room(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    day_spent: &mut bool,
) -> Result<Room, OrchestratorError> {
    let state = budget_state(
        &deps.tools.projections,
        team,
        contract.assignee_role,
        Some(contract),
        &SessionLedger::default(),
        deps.tools.clock.now(),
    )?;
    let exhausted: Vec<BudgetScope> = check_budgets(&state)
        .into_iter()
        .map(|exhausted| exhausted.scope)
        .collect();
    if exhausted.contains(&BudgetScope::DayUsd) {
        *day_spent = true;
        return Ok(Room::DaySpent);
    }
    if exhausted
        .iter()
        .any(|scope| matches!(scope, BudgetScope::TaskUsd | BudgetScope::TaskSessions))
    {
        return Ok(Room::TaskSpent);
    }
    Ok(Room::Free)
}

/// Rule 6: a task `in_progress` gets its assignee's implement session, in its worktree, with its
/// sandbox, told where an earlier session left the work. A task whose assignee is not active is
/// passed over: every tool call of its session would be refused.
async fn in_progress(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(None);
    }
    let sandbox = orchestrator.sandbox_for(&row.task_id, team)?;
    let resume = resume(deps, team, &row.task_id)?;
    let executor: Arc<dyn Executor> = sandbox;
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assignee,
            contract: &contract,
            purpose: SessionPurpose::Implement,
            cwd: worktree(deps, &row.task_id),
            executor: Some(executor),
            initial_prompt: implement_message(&contract, &resume),
        },
    )
    .await?;
    Ok(Some(acted(row, assignee, "implement", &end)))
}

/// Where the task's work stands: its branch's tip when the branch has a commit past the
/// integration branch, the last note written since it last moved into `in_progress`, and, when that
/// move came from `rejected`, the rejection that sent it back.
fn resume(
    deps: &OrchestratorDeps,
    team: &Team,
    task_id: &TaskId,
) -> Result<Resume, OrchestratorError> {
    let worktree = worktree(deps, task_id);
    let last_commit = if worktree.is_dir() {
        let git = &deps.tools.git;
        let base = integration_branch(team, git)?;
        if git.commit_count(&base, &format!("farik/{}", task_id.as_str()))? > 0 {
            Git::open(worktree).head_summary()?
        } else {
            None
        }
    } else {
        None
    };
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned, EventKind::NoteWritten],
        ..EventQuery::default()
    })?;
    let mut last_note = None;
    let mut rejection = None;
    for event in &history {
        match &event.body {
            EventBody::TaskTransitioned(body) if body.to.to_string() == "rejected" => {
                rejection = body.rejection.as_ref().map(|rejection| {
                    (
                        rejection.failed_criterion_ids.clone(),
                        rejection.reasons.clone(),
                    )
                });
            }
            EventBody::TaskTransitioned(body) if body.to.to_string() == "in_progress" => {
                last_note = None;
                if body.from.to_string() != "rejected" {
                    rejection = None;
                }
            }
            EventBody::NoteWritten(body) => {
                last_note = Some((body.kind.to_string(), body.text.clone()));
            }
            _ => {}
        }
    }
    Ok(Resume {
        last_commit,
        last_note,
        rejection,
    })
}

/// Rule 7: a task `assigned` gets its worktree on `farik/<id>` from the integration branch,
/// reused when it is already there, and is moved to `in_progress` as its assignee asks. No session
/// starts: the next tick's rule 6 starts it. A task whose assignee is not active is passed over.
fn assigned(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    let worktree = worktree(deps, &row.task_id);
    let branch = format!("farik/{}", row.task_id.as_str());
    if !worktree.is_dir() {
        let git = &deps.tools.git;
        git.create_worktree(&worktree, &branch, &integration_branch(team, git)?)?;
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
    let what = match outcome {
        TransitionOutcome::Moved(_) => {
            format!(
                "started it for {} in its worktree on {branch}",
                assignee.id.as_str()
            )
        }
        TransitionOutcome::Refused(refusal) => format!(
            "the governor would not start it: {}",
            refusal_details(&refusal).join("; ")
        ),
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    }))
}

/// The team's agent of that id, when it is active.
fn active<'a>(team: &'a Team, agent_id: Option<&str>) -> Option<&'a Agent> {
    let agent_id = agent_id?;
    team.active_agents()
        .find(|agent| agent.id.as_str() == agent_id)
}

/// Rule 8: a standalone task that is `ready` gets a plan session of its assigner (the active Scrum
/// Master, else the Product Manager), when an agent of its assignee role has room for it and every
/// dependency is accepted and integrated.
async fn ready(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    if row.kind != TaskKind::Task || row.parent.is_some() {
        return Ok(None);
    }
    let Some(assigner) = assigner(team) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(None);
    }
    let assignees: Vec<String> = team
        .active_agents()
        .filter(|agent| Role::from(agent.role) == contract.assignee_role)
        .filter(|agent| has_room(team, board, agent))
        .map(|agent| agent.id.to_string())
        .collect();
    let Some(first) = assignees.first() else {
        return Ok(None);
    };
    if !dependencies_integrated(deps, team, &contract, assigner, first)? {
        return Ok(None);
    }
    let reviewers: Vec<String> = team
        .active_agents()
        .filter(|agent| Role::from(agent.role) == contract.reviewer_role)
        .map(|agent| agent.id.to_string())
        .collect();
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assigner,
            contract: &contract,
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            initial_prompt: plan_message(&contract, &assignees, &reviewers),
        },
    )
    .await?;
    Ok(Some(acted(row, assigner, "plan", &end)))
}

/// Who assigns: the active Scrum Master, else the active Product Manager (D6).
fn assigner(team: &Team) -> Option<&Agent> {
    let holding = |role: Role| {
        team.active_agents()
            .find(move |agent| Role::from(agent.role) == role)
    };
    holding(Role::ScrumMaster).or_else(|| holding(Role::ProductManager))
}

/// Whether `agent` holds fewer open tasks, neither accepted nor cancelled, than the WIP limit.
fn has_room(team: &Team, board: &[TaskProjection], agent: &Agent) -> bool {
    let held = board
        .iter()
        .filter(|row| row.assignee_id.as_deref() == Some(agent.id.as_str()))
        .filter(|row| !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
        .count();
    u64::try_from(held).unwrap_or(u64::MAX)
        < u64::try_from(team.policy.wip_limit_per_agent).unwrap_or(0)
}

/// Whether every dependency of the task is accepted and integrated, as the governor's context for
/// the assignment reads them.
fn dependencies_integrated(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    assigner: &Agent,
    candidate: &str,
) -> Result<bool, OrchestratorError> {
    if contract.dependencies.is_empty() {
        return Ok(true);
    }
    let request = TransitionRequest {
        task_id: contract.id.clone(),
        to: TaskStatus::Assigned,
        actor: if Role::from(assigner.role) == Role::ScrumMaster {
            TransitionActor::ScrumMaster
        } else {
            TransitionActor::ProductManager
        },
        agent_id: Some(assigner.id.to_string()),
    };
    let ask = TransitionAsk {
        assignee_id: Some(candidate.to_string()),
        ..TransitionAsk::default()
    };
    let context = deps.tools.transitions.context(&request, &ask, team)?;
    let states = context
        .assignment
        .map(|assignment| assignment.dependencies)
        .unwrap_or_default();
    Ok(states.len() == contract.dependencies.len()
        && states
            .iter()
            .all(|state| state.status == TaskStatus::Accepted && state.integrated))
}

/// What a tick that ran a session says.
fn acted(row: &TaskProjection, agent: &Agent, purpose: &str, end: &SessionEnd) -> TickReport {
    let how = match end.reason {
        EndReason::Completed => "completed",
        EndReason::Aborted => "was aborted",
        EndReason::Limit => "reached a limit",
        EndReason::Error => "failed",
    };
    TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!(
            "ran {}'s {purpose} session, which {how}: {}",
            agent.id.as_str(),
            end.detail
        ),
    }
}

/// The number in a task id, `FRK-<n>`, by which ties are broken; an id without one goes last.
fn task_number(task: &str) -> u64 {
    task.rsplit('-')
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;

    use farik_core::contract::{Role, TaskStatus};
    use farik_core::pricing::Usage;
    use farik_protocol::event::{
        BudgetExhaustedBodyScope, CriterionRecordedBodyRunBy, EscalationRaisedBodyReason,
        EventBody, EventKind, NoteWrittenBodyKind, SessionEndedBodyReason,
        SessionStartedBodyPurpose, TransitionActorWire,
    };
    use farik_roles::RoleError;
    use farik_store::git::fixtures::git_output_in;
    use serde_json::json;

    use crate::claude::allowed_builtins;
    use crate::orchestrator::fixtures::{CountingSandboxFactory, Harness, UsageThenWaitAdapter};
    use crate::orchestrator::{OrchestratorError, TickReport};
    use crate::recorded::Transcript;
    use crate::recorded::fixtures::{
        implement_finishes_frk_1, implement_stops_early, plan_assigns_frk_1, reads_a_file,
    };
    use crate::session::SessionPurpose;

    const NOTHING_TO_DO: &str = "nothing on the board needs doing";

    fn acted_on(report: &TickReport) -> Option<&str> {
        match report {
            TickReport::Acted { task_id, .. } => Some(task_id.as_str()),
            TickReport::Idle { .. } => None,
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_the_assigners_plan_session_for_a_ready_task() {
        let harness = Harness::new("orch-plan-starts", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(
            spec.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert!(spec.mcp_servers.is_empty());
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        let pm = team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "pm")
            .expect("pm is on the team");
        assert_eq!(
            spec.builtin_tools,
            allowed_builtins(&pm.tiers().into_iter().collect::<BTreeSet<_>>())
        );
        assert!(
            spec.initial_prompt.contains("FRK-1"),
            "{}",
            spec.initial_prompt
        );
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_the_session_its_cost_and_its_end() {
        let harness = Harness::new("orch-plan-records", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let session_id = adapter.started()[0].session_id.clone();
        let events = harness.events(&[
            EventKind::SessionStarted,
            EventKind::ToolCalled,
            EventKind::TaskTransitioned,
            EventKind::CostRecorded,
            EventKind::SessionEnded,
        ]);
        let kinds: Vec<EventKind> = events.iter().map(|event| event.body.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::SessionStarted,
                EventKind::ToolCalled,
                EventKind::TaskTransitioned,
                EventKind::CostRecorded,
                EventKind::SessionEnded,
            ]
        );
        assert!(matches!(
            &events[0].body,
            EventBody::SessionStarted(body) if body.purpose == SessionStartedBodyPurpose::Plan
        ));
        assert!(matches!(
            &events[1].body,
            EventBody::ToolCalled(body) if body.tool == "mcp__farik__farik_assign_task"
        ));
        assert!(matches!(
            &events[2].body,
            EventBody::TaskTransitioned(body) if body.to.to_string() == "assigned"
        ));
        let cost = &events[3].envelope.ids;
        assert_eq!(cost.session_id.as_deref(), Some(session_id.as_str()));
        assert_eq!(cost.agent_id.as_deref(), Some("pm"));
        assert_eq!(
            cost.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert!(matches!(
            &events[4].body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Completed
        ));
        assert!(harness.daemon.tool_context(&session_id).is_none());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_free_assignee() {
        let harness = Harness::new("orch-plan-no-room", |_| {});
        harness.ready("FRK-1");
        harness.blocked("FRK-2", "dev-a", "dev-b");
        harness.blocked("FRK-3", "dev-b", "dev-a");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_dependency_to_be_integrated() {
        // FRK-1's assignee is paused, so that no rule works FRK-1 and the tick reaches FRK-2.
        let harness = Harness::new("orch-plan-dependency", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.file("FRK-2", "ready", |wire| {
            wire["dependencies"] = json!(["FRK-1"]);
        });
        let adapter = harness.recorded(vec![reads_a_file(), plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(
            !adapter
                .started()
                .iter()
                .any(|spec| spec.task_id.as_ref().map(|task| task.as_str()) == Some("FRK-2")),
            "{:?}",
            adapter.started()
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn fails_the_tick_when_a_session_cannot_start() {
        let harness = Harness::new("orch-plan-no-start", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter);

        let ticked = orchestrator.tick().await;

        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        let events = harness.events(&[
            EventKind::SessionStarted,
            EventKind::CostRecorded,
            EventKind::SessionEnded,
        ]);
        assert_eq!(events.len(), 3, "{events:?}");
        assert_eq!(events[0].body.kind(), EventKind::SessionStarted);
        match &events[1].body {
            EventBody::CostRecorded(body) => {
                assert_eq!(body.usage.input_tokens, 0);
                assert_eq!(body.usage.output_tokens, 0);
                assert_eq!(
                    events[1]
                        .envelope
                        .ids
                        .task_id
                        .as_ref()
                        .map(|task| task.as_str()),
                    Some("FRK-1")
                );
            }
            other => panic!("expected a cost, got {other:?}"),
        }
        assert!(matches!(
            &events[2].body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Error
        ));
        let session_id = events[0]
            .envelope
            .ids
            .session_id
            .clone()
            .expect("a session's event names it");
        assert!(harness.daemon.tool_context(&session_id).is_none());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_the_scrum_master_as_assigner_when_there_is_one() {
        // Farik ships no Scrum Master until phase 4, so its role does not load: the tick fails
        // naming the role it chose, before any session starts.
        let harness = Harness::new("orch-plan-scrum-master", |wire| {
            let agents = wire["agents"].as_array_mut().expect("a list of agents");
            agents.push(json!({
                "id": "sam",
                "display_name": "sam",
                "role": "scrum_master",
                "status": "active"
            }));
        });
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let ticked = orchestrator.tick().await;

        assert_eq!(
            ticked,
            Err(OrchestratorError::Role(RoleError::NotFound {
                role_id: Role::ScrumMaster.to_string()
            }))
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_an_assigned_task_in_its_worktree() {
        let harness = Harness::new("orch-start", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let worktree = harness.worktree("FRK-1");
        assert!(worktree.is_dir());
        assert_eq!(
            git_output_in(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]),
            "farik/FRK-1"
        );
        let moves = harness.events(&[EventKind::TaskTransitioned]);
        match &moves.last().expect("a move").body {
            EventBody::TaskTransitioned(body) => {
                assert_eq!(body.from.to_string(), "assigned");
                assert_eq!(body.to.to_string(), "in_progress");
                assert_eq!(body.actor, TransitionActorWire::Assignee);
                assert_eq!(body.requested_by, "dev-a");
            }
            other => panic!("expected a move, got {other:?}"),
        }
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_the_implement_session_in_the_worktree_with_the_sandbox() {
        let harness = Harness::new("orch-implement", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_finishes_frk_1()]);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        orchestrator.tick().await.expect("the task starts");
        let report = orchestrator.tick().await.expect("the session runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Implement);
        assert_eq!(started[0].agent_id, "dev-a");
        assert_eq!(started[0].cwd, harness.worktree("FRK-1"));
        assert_eq!(sandboxes.created("FRK-1"), 1);
        let git = &harness.project.deps.git;
        assert_eq!(
            git.commit_count("main", "farik/FRK-1").expect("git counts"),
            1
        );
        assert_eq!(
            git.changed_paths("main", "farik/FRK-1").expect("git lists"),
            vec!["done.txt".to_string()]
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Verifying);
        assert!(harness.events(&[EventKind::CriterionRecorded]).iter().any(|event| matches!(
            &event.body,
            EventBody::CriterionRecorded(body) if body.run_by == CriterionRecordedBodyRunBy::Assignee
        )));
        assert!(
            harness
                .events(&[EventKind::NoteWritten])
                .iter()
                .any(|event| matches!(
                    &event.body,
                    EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Completion
                ))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn resumes_an_implement_session_that_stopped_early() {
        let harness = Harness::new("orch-resume", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_stops_early(), implement_finishes_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the task starts");
        orchestrator.tick().await.expect("the first session runs");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        orchestrator.tick().await.expect("the second session runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert!(
            !started[0].initial_prompt.contains("Resuming"),
            "{}",
            started[0].initial_prompt
        );
        let head = git_output_in(&harness.project.repo.path, &["rev-parse", "farik/FRK-1"]);
        let prompt = &started[1].initial_prompt;
        assert!(
            prompt.contains(&format!("Resuming: last commit {head}")),
            "{prompt}"
        );
        assert!(
            prompt.contains("done.txt committed; C1 not run yet."),
            "{prompt}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn works_nearest_to_done_first() {
        let harness = Harness::new("orch-order-assigned", |_| {});
        harness.ready("FRK-1");
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert!(adapter.started().is_empty());

        let harness = Harness::new("orch-order-in-progress", |_| {});
        harness.ready("FRK-1");
        harness.in_progress("FRK-2", "dev-a", "dev-b");
        harness.assigned("FRK-3", "dev-b", "dev-a");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert_eq!(adapter.started()[0].purpose, SessionPurpose::Implement);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_between_ticks() {
        let harness = Harness::new("orch-stop", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.stop();
        orchestrator.run_until_idle().await.expect("nothing runs");

        assert!(adapter.started().is_empty());
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Ready);
    }

    fn last_move(harness: &Harness) -> farik_protocol::event::TaskTransitionedBody {
        match &harness
            .events(&[EventKind::TaskTransitioned])
            .last()
            .expect("a move")
            .body
        {
            EventBody::TaskTransitioned(body) => body.clone(),
            other => panic!("expected a move, got {other:?}"),
        }
    }

    fn escalation_reasons(harness: &Harness) -> Vec<EscalationRaisedBodyReason> {
        harness
            .events(&[EventKind::EscalationRaised])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn returns_a_rejected_task_to_its_assignee() {
        let harness = Harness::new("orch-rejected", |_| {});
        harness.rejected("FRK-1", 0, "C1: done.txt missing");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert!(adapter.started().is_empty());
        let moved = last_move(&harness);
        assert_eq!(moved.from.to_string(), "rejected");
        assert_eq!(moved.to.to_string(), "in_progress");
        assert_eq!(moved.requested_by, "governor");
        assert_eq!(moved.iteration, 1);
        assert_eq!(harness.row("FRK-1").iteration, 1);

        orchestrator
            .tick()
            .await
            .expect("the implement session runs");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Implement);
        let prompt = &started[0].initial_prompt;
        assert!(prompt.contains("C1: done.txt missing"), "{prompt}");
        assert!(
            prompt.contains("<untrusted source=\"rejection\">"),
            "{prompt}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_task_rejected_too_often() {
        let harness = Harness::new("orch-rejected-too-often", |_| {});
        harness.rejected("FRK-1", 3, "C1: done.txt missing");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Iterations]
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_block_past_its_limit() {
        let harness = Harness::new("orch-blocked-old", |_| {});
        harness.blocked_hours_ago("FRK-1", "dev-a", "dev-b", 25);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::BlockerAge]
        );

        let harness = Harness::new("orch-blocked-young", |_| {});
        harness.blocked_hours_ago("FRK-1", "dev-a", "dev-b", 1);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Blocked);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn picks_a_rejected_task_before_a_ready_one() {
        let harness = Harness::new("orch-order-rejected", |_| {});
        harness.ready("FRK-1");
        harness.rejected("FRK-2", 0, "C1: done.txt missing");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert!(adapter.started().is_empty());
    }

    /// Usage past a session's 100 input tokens and under every dollar budget.
    fn a_thousand_tokens() -> Usage {
        Usage {
            input_tokens: 1000,
            output_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }

    fn scopes_exhausted(harness: &Harness) -> Vec<BudgetExhaustedBodyScope> {
        harness
            .events(&[EventKind::BudgetExhausted])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::BudgetExhausted(body) => Some(body.scope),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_out_of_sessions() {
        let harness = Harness::new("orch-budget-sessions", |_| {});
        harness.file("FRK-1", "ready", |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        harness.ready("FRK-2");
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Plan);
        assert_eq!(
            started[0].task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-2")
        );
        harness.project.moved(
            "FRK-2",
            "ready",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Ready);
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_out_of_dollars() {
        let harness = Harness::new("orch-budget-dollars", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.spent(Some("FRK-1"), "s-0", 3.0);
        harness.spent(Some("FRK-1"), "s-0", 2.0);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn aborts_a_session_whose_usage_crosses_a_budget() {
        let harness = Harness::new("orch-budget-abort", |wire| {
            wire["budgets"]["session"] = json!({ "max_input_tokens": 100 });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        // A session nobody aborts waits for ever.
        tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the session was aborted")
            .expect("the tick runs");

        assert_eq!(adapter.aborts(), 1);
        assert_eq!(
            scopes_exhausted(&harness),
            vec![BudgetExhaustedBodyScope::SessionTokens]
        );
        let ends = harness.events(&[EventKind::BudgetExhausted, EventKind::SessionEnded]);
        assert_eq!(ends.len(), 2, "{ends:?}");
        assert!(matches!(
            &ends[1].body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Aborted
        ));
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        assert!(harness.events(&[EventKind::NoteWritten]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn finishes_the_last_session_a_task_is_allowed() {
        let harness = Harness::new("orch-budget-last-session", |_| {});
        harness.file("FRK-1", "ready", |wire| {
            wire["budget"]["max_sessions"] = json!(2);
        });
        harness.project.moved(
            "FRK-1",
            "ready",
            "assigned",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness.project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = Arc::new(UsageThenWaitAdapter::completing(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the last session runs");

        assert_eq!(adapter.aborts(), 0);
        assert_eq!(
            scopes_exhausted(&harness),
            vec![BudgetExhaustedBodyScope::TaskSessions]
        );
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn counts_a_session_that_reported_no_usage() {
        let harness = Harness::new("orch-budget-no-usage", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let recorded = reads_a_file();
        let without_result: Vec<&str> = recorded
            .lines()
            .filter(|line| !line.contains("\"type\":\"result\""))
            .collect::<Vec<_>>();
        let transcript = Transcript::from_jsonl(&without_result.join("\n"));
        let adapter = harness.recorded(vec![transcript]);
        let orchestrator = harness.orchestrator(adapter);

        orchestrator.tick().await.expect("the tick runs");

        let costs = harness.events(&[EventKind::CostRecorded]);
        assert_eq!(costs.len(), 1, "{costs:?}");
        assert!(matches!(
            &costs[0].body,
            EventBody::CostRecorded(body) if body.usage.input_tokens == 0 && body.usage.output_tokens == 0
        ));
        let ends = harness.events(&[EventKind::SessionEnded]);
        assert!(matches!(
            &ends[..],
            [end] if matches!(&end.body, EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Error)
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_the_task_on_every_sessions_cost() {
        let harness = Harness::new("orch-budget-task-costs", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1(), implement_finishes_frk_1()]);
        let orchestrator = harness.orchestrator(adapter);

        for _ in 0..3 {
            orchestrator.tick().await.expect("the tick runs");
        }

        let costs = harness.events(&[EventKind::CostRecorded]);
        assert_eq!(costs.len(), 2, "{costs:?}");
        for cost in &costs {
            assert_eq!(
                cost.envelope.ids.task_id.as_ref().map(|task| task.as_str()),
                Some("FRK-1")
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_nothing_when_the_day_is_spent() {
        let harness = Harness::new("orch-budget-day", |_| {});
        harness.spent(None, "s-0", 20.0);
        harness.ready("FRK-1");
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![plan_assigns_frk_1(), implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let first = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&first), Some("FRK-2"), "{first:?}");
        assert_eq!(harness.row("FRK-2").status, TaskStatus::InProgress);
        let second = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            second,
            TickReport::Idle {
                why: "the team's daily budget is spent".to_string()
            }
        );
        assert!(adapter.started().is_empty());
    }
}
