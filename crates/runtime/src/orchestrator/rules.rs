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

use super::integrate::{awaiting, cleanup};
use super::messages::{Resume, implement_message, plan_message};
use super::session::{SessionAsk, SessionEnd, run_session};
use super::verify::verifying;
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, worktree};
use crate::cost::budget_state;
use crate::exec::Executor;
use crate::session::{EndReason, SessionPurpose};
use crate::transitions::{TransitionAsk, TransitionOutcome, integration_branch};

/// What a tick says when no rule matched.
const NOTHING_TO_DO: &str = "nothing on the board needs doing";
/// What a tick says when the only rules that matched would have started a session on a spent day.
const DAY_SPENT: &str = "the team's daily budget is spent";

/// Whether a budget stops a session from starting for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Room {
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
    // The store gives the board in the order of the number in each task id, which is how ties
    // are broken.
    let board = deps.tools.projections.board()?;
    let mut day_spent = false;
    for row in board
        .iter()
        .filter(|row| matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
    {
        if let Some(report) = cleanup(orchestrator, row)? {
            return Ok(report);
        }
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Accepted)
    {
        if let Some(report) = awaiting(orchestrator, &team, row).await? {
            return Ok(report);
        }
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Rejected)
    {
        if let Some(report) = rejected(deps, &team, row)? {
            return Ok(report);
        }
    }
    for row in board.iter().filter(|row| row.status == TaskStatus::Blocked) {
        if let Some(report) = blocked(deps, &team, row)? {
            return Ok(report);
        }
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Verifying)
    {
        if let Some(report) = verifying(orchestrator, &team, row, &mut day_spent).await? {
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
/// effects count the iteration and raise the escalation. A task whose escalation was refused since
/// it was rejected is passed over, as is one the governor would move neither way: the refusal is
/// on the board for the human, and asking again would record it again on every tick.
fn rejected(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if refused_since_entering(
        deps,
        &row.task_id,
        TaskStatus::Rejected,
        TaskStatus::Escalated,
    )? {
        return Ok(None);
    }
    let what = match governor_moves(deps, team, row, TaskStatus::InProgress)? {
        TransitionOutcome::Moved(_) => "returned it to its assignee for another iteration",
        TransitionOutcome::Refused(_) => {
            match governor_moves(deps, team, row, TaskStatus::Escalated)? {
                TransitionOutcome::Moved(_) => {
                    "escalated it: it was rejected as often as it may be"
                }
                TransitionOutcome::Refused(_) => return Ok(None),
            }
        }
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what: what.to_string(),
    }))
}

/// Rule 4: a task `blocked` for at least the team's `blocked_limit_hours` is escalated as the
/// governor, whose `BlockedAge` gate decides; a younger block has no rule, nor has one whose
/// escalation was refused since it blocked.
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
    if deps.tools.clock.now() - blocked_at < chrono::Duration::hours(hours)
        || refused_since_entering(
            deps,
            &row.task_id,
            TaskStatus::Blocked,
            TaskStatus::Escalated,
        )?
    {
        return Ok(None);
    }
    match governor_moves(deps, team, row, TaskStatus::Escalated)? {
        TransitionOutcome::Moved(_) => Ok(Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("escalated it: blocked for {hours} hours or more"),
        })),
        TransitionOutcome::Refused(_) => Ok(None),
    }
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

/// Whether the task's move from `from` to `to` was refused since the task last moved into `from`.
/// Such a move is not asked again until the task moves: nothing but a move changes what the
/// governor would answer, and the refusal is on the board for the human.
fn refused_since_entering(
    deps: &OrchestratorDeps,
    task_id: &TaskId,
    from: TaskStatus,
    to: TaskStatus,
) -> Result<bool, OrchestratorError> {
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned, EventKind::TransitionRefused],
        ..EventQuery::default()
    })?;
    let (from, to) = (from.to_string(), to.to_string());
    Ok(history
        .iter()
        .rev()
        .take_while(|event| {
            !matches!(&event.body, EventBody::TaskTransitioned(body) if body.to.to_string() == from)
        })
        .any(|event| {
            matches!(&event.body, EventBody::TransitionRefused(body)
                if body.from.to_string() == from && body.to.to_string() == to)
        }))
}

/// Whether the budgets leave room for a session about `contract`, read with an empty session
/// ledger: the day's dollars, then the task's dollars and sessions. `day_spent` is set when the
/// day is what stops it.
pub(super) fn room(
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
            read_only: false,
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
/// starts: the next tick's rule 6 starts it. A task whose assignee is not active is passed over, as
/// is one whose start the governor refused, now or since it was assigned.
fn assigned(
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
    match outcome {
        TransitionOutcome::Moved(_) => Ok(Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!(
                "started it for {} in its worktree on {branch}",
                assignee.id.as_str()
            ),
        })),
        TransitionOutcome::Refused(_) => Ok(None),
    }
}

/// The team's agent of that id, when it is active.
pub(super) fn active<'a>(team: &'a Team, agent_id: Option<&str>) -> Option<&'a Agent> {
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
            read_only: false,
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
pub(super) fn acted(
    row: &TaskProjection,
    agent: &Agent,
    purpose: &str,
    end: &SessionEnd,
) -> TickReport {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;

    use farik_core::contract::{Role, TaskStatus};
    use farik_core::governor::permissions::{PermissionTier, default_tiers};
    use farik_core::pricing::Usage;
    use farik_core::team::Effort;
    use farik_protocol::event::{
        BudgetExhaustedBodyScope, CriterionRecordedBodyRunBy, EscalationRaisedBodyReason,
        EventBody, EventKind, NoteWrittenBodyKind, ReviewRecordedBody, SessionEndedBodyReason,
        SessionStartedBodyPurpose, TransitionActorWire,
    };
    use farik_protocol::event::{NewEvent, event_from_value};
    use farik_roles::RoleError;
    use farik_store::git::fixtures::git_output_in;
    use serde_json::json;

    use crate::claude::allowed_builtins;
    use crate::exec::ExecError;
    use crate::orchestrator::fixtures::{
        BrokenSandboxFactory, CountingSandboxFactory, ExecutorWitness, Harness,
        UsageThenWaitAdapter,
    };
    use crate::orchestrator::{Orchestrator, OrchestratorError, TickReport};
    use crate::recorded::fixtures::{
        accept_frk_1, implement_finishes_frk_1, implement_stops_early, plan_assigns_frk_1,
        reads_a_file, review_answers_nothing, review_writes_note,
    };
    use crate::recorded::{RecordedAdapter, Transcript};
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;
    use crate::tools::tool_descriptors;

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
    async fn waits_for_an_accepted_dependency_until_it_is_integrated() {
        let harness = Harness::new("orch-plan-dependency-integrated", |_| {});
        harness.verifying("FRK-1");
        harness.project.moved(
            "FRK-1",
            "verifying",
            "accepted",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness
            .project
            .deps
            .git
            .remove_worktree(&harness.worktree("FRK-1"))
            .expect("the worktree goes");
        harness.file("FRK-2", "ready", |wire| {
            wire["dependencies"] = json!(["FRK-1"]);
        });
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert!(adapter.started().is_empty());

        harness.project.record(
            "FRK-1",
            "task.integrated",
            &json!({ "sha": "abc", "into": "main", "integrated_by": "human" }),
        );
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_dependency_the_board_does_not_hold() {
        let harness = Harness::new("orch-plan-dependency-missing", |_| {});
        harness.file("FRK-2", "ready", |wire| {
            wire["dependencies"] = json!(["FRK-1"]);
        });
        let adapter = harness.recorded(vec![reads_a_file()]);
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

    /// The paths `git worktree list` names.
    fn worktrees_listed(harness: &Harness) -> String {
        git_output_in(
            &harness.project.repo.path,
            &["worktree", "list", "--porcelain"],
        )
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn cleans_up_a_task_once_it_is_accepted() {
        let harness = Harness::new("orch-cleanup", |_| {});
        harness.accepted_with_worktree("FRK-1");
        let base = harness.worktree("FRK-1-base");
        harness
            .project
            .deps
            .git
            .create_detached_worktree(&base, "main")
            .expect("the base worktree is made");
        let adapter = harness.recorded(Vec::new());
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(adapter, sandboxes.clone());
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        orchestrator
            .sandbox_for(&"FRK-1".parse().expect("a task id"), &team)
            .expect("a sandbox is made");

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(sandboxes.removed("FRK-1"), 1);
        assert!(!harness.worktree("FRK-1").exists());
        assert!(!base.exists());
        let listed = worktrees_listed(&harness);
        assert!(!listed.contains("FRK-1"), "{listed}");
        assert_eq!(
            git_output_in(
                &harness.project.repo.path,
                &["branch", "--list", "farik/FRK-1"]
            )
            .trim(),
            "farik/FRK-1"
        );
        assert!(!orchestrator.holds_sandbox(&"FRK-1".parse().expect("a task id")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn cleans_up_a_worktree_git_no_longer_knows() {
        let harness = Harness::new("orch-cleanup-unknown", |_| {});
        harness.accepted("FRK-1");
        std::fs::create_dir_all(harness.worktree("FRK-1").join("left"))
            .expect("a directory is left behind");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter);

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert!(!harness.worktree("FRK-1").exists());
        assert_eq!(
            orchestrator.tick().await.expect("the tick runs"),
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
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
        harness.rejected("FRK-1", 0, "done.txt is missing");
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
        let rejection = block(prompt, "rejection");
        assert!(rejection.contains("failed criteria: C1"), "{prompt}");
        assert!(rejection.contains("done.txt is missing"), "{prompt}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn forgets_a_rejection_once_the_task_has_moved_on() {
        // Returned after its rejection, then blocked and resumed: the resumed session answers
        // no rejection.
        let harness = Harness::new("orch-rejection-forgotten", |_| {});
        harness.rejected("FRK-1", 0, "done.txt is missing");
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": 1 });
        harness
            .project
            .moved("FRK-1", "rejected", "in_progress", &people);
        let mut blocking = people.clone();
        blocking["blocker"] = json!({ "description": "the API is down", "needed": "the API" });
        harness
            .project
            .moved("FRK-1", "in_progress", "blocked", &blocking);
        let mut resuming = people;
        resuming["actor"] = json!("human");
        resuming["requested_by"] = json!("human");
        harness
            .project
            .moved("FRK-1", "blocked", "in_progress", &resuming);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator
            .tick()
            .await
            .expect("the implement session runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let prompt = &started[0].initial_prompt;
        assert!(!prompt.contains("rejected"), "{prompt}");
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

        // Exactly the limit is old enough.
        let harness = Harness::new("orch-blocked-at-limit", |_| {});
        harness.blocked_hours_ago("FRK-1", "dev-a", "dev-b", 24);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);

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

    /// A `transition.refused` of `task`'s move from `from` to `to`, asked by `actor` as
    /// `requested_by`.
    fn refused(harness: &Harness, task: &str, from: &str, to: &str, actor: &str, by: &str) {
        harness.project.record(
            task,
            "transition.refused",
            &json!({
                "from": from,
                "to": to,
                "actor": actor,
                "requested_by": by,
                "refusal": "gate_failed",
                "details": ["the gate did not open"]
            }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_move_that_was_refused() {
        let harness = Harness::new("orch-refused-start", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        refused(
            &harness,
            "FRK-1",
            "assigned",
            "in_progress",
            "assignee",
            "dev-a",
        );
        harness.rejected("FRK-2", 3, "C1: done.txt missing");
        refused(
            &harness,
            "FRK-2",
            "rejected",
            "escalated",
            "governor",
            "governor",
        );
        harness.blocked_hours_ago("FRK-3", "dev-b", "dev-a", 25);
        refused(
            &harness,
            "FRK-3",
            "blocked",
            "escalated",
            "governor",
            "governor",
        );
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(harness.events(&[EventKind::TransitionRefused]).len(), 3);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Assigned);
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Rejected);
        assert_eq!(harness.row("FRK-3").status, TaskStatus::Blocked);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_again_after_another_refusal() {
        // A human's refused request to resume a blocked task says nothing about its escalation.
        let harness = Harness::new("orch-refused-other", |_| {});
        harness.blocked_hours_ago("FRK-1", "dev-a", "dev-b", 25);
        refused(
            &harness,
            "FRK-1",
            "blocked",
            "in_progress",
            "human",
            "human",
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);

        // Nor does a refusal from before the task last blocked.
        let harness = Harness::new("orch-refused-earlier", |_| {});
        harness.blocked_hours_ago("FRK-1", "dev-a", "dev-b", 25);
        refused(
            &harness,
            "FRK-1",
            "blocked",
            "escalated",
            "governor",
            "governor",
        );
        let people = json!({ "actor": "human", "requested_by": "human" });
        harness
            .project
            .moved("FRK-1", "blocked", "in_progress", &people);
        let mut body = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        body["blocker"] = json!({ "description": "the API is down again", "needed": "the API" });
        harness.project.moved_at(
            at() - chrono::Duration::hours(25),
            "FRK-1",
            "in_progress",
            "blocked",
            &body,
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
    }

    /// `run_until_idle` on a thread of its own, which is left behind if the run does not end
    /// within ten seconds: a run that does not stop at an idle tick ticks for ever without
    /// yielding, so no timer on its own runtime could stop it.
    fn run_until_idle_within_ten_seconds(
        orchestrator: Orchestrator,
    ) -> Result<(), OrchestratorError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime is built");
            let _ = sender.send(runtime.block_on(orchestrator.run_until_idle()));
        });
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the run ends")
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn runs_until_a_tick_is_idle() {
        let harness = Harness::new("orch-run-until-idle", |_| {});
        harness.file("FRK-1", "ready", |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.project.moved(
            "FRK-1",
            "ready",
            "assigned",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        run_until_idle_within_ten_seconds(orchestrator).expect("the run is idle");

        assert_eq!(adapter.started().len(), 1);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
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

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn picks_a_rejected_task_before_a_verifying_one() {
        let harness = Harness::new("orch-order-rejected-verifying", |_| {});
        harness.verifying("FRK-1");
        harness.rejected("FRK-2", 0, "done.txt is missing");
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn picks_a_verifying_task_before_one_in_progress() {
        let harness = Harness::new("orch-order-verifying-in-progress", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.verifying("FRK-2");
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
    }

    /// The `criterion.recorded` events Farik recorded as the governor, with no agent on their
    /// envelope, as (id, passed).
    fn governor_runs(harness: &Harness) -> Vec<(String, bool)> {
        harness
            .events(&[EventKind::CriterionRecorded])
            .iter()
            .filter(|event| event.envelope.ids.agent_id.is_none())
            .filter_map(|event| match &event.body {
                EventBody::CriterionRecorded(body) if body.recorded_by == "governor" => {
                    assert_eq!(body.run_by, CriterionRecordedBodyRunBy::Reviewer);
                    Some((body.criterion_id.clone(), body.passed))
                }
                _ => None,
            })
            .collect()
    }

    /// Every `review.recorded` body.
    fn reviews(harness: &Harness) -> Vec<ReviewRecordedBody> {
        harness
            .events(&[EventKind::ReviewRecorded])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::ReviewRecorded(body) => Some(body.clone()),
                _ => None,
            })
            .collect()
    }

    /// The (agent, purpose) of every session started.
    fn sessions(adapter: &RecordedAdapter) -> Vec<(String, SessionPurpose)> {
        adapter
            .started()
            .iter()
            .map(|spec| (spec.agent_id.clone(), spec.purpose))
            .collect()
    }

    /// The text of `source`'s untrusted block in `text`, or nothing.
    fn block<'a>(text: &'a str, source: &str) -> &'a str {
        let open = format!("<untrusted source=\"{source}\">");
        text.split_once(&open)
            .and_then(|(_, rest)| rest.split_once("</untrusted>"))
            .map_or("", |(inside, _)| inside)
    }

    /// A `review` criterion C2.
    fn with_a_review_criterion(wire: &mut serde_json::Value) {
        push_criterion(
            wire,
            json!({
                "id": "C2",
                "text": "The file is where it belongs.",
                "verification": { "method": "review", "rubric": ["Is done.txt at the root?"] }
            }),
        );
    }

    fn push_criterion(wire: &mut serde_json::Value, criterion: serde_json::Value) {
        wire["exit_criteria"]
            .as_array_mut()
            .expect("a list of criteria")
            .push(criterion);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_the_criteria_as_the_reviewer_before_the_review() {
        let harness = Harness::new("orch-verify-runs", |_| {});
        harness.verifying("FRK-1");
        let recorded = harness.recorded(vec![review_writes_note()]);
        let witness = Arc::new(ExecutorWitness::new(
            recorded.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let events = harness.events(&[EventKind::CriterionRecorded, EventKind::SessionStarted]);
        let kinds: Vec<EventKind> = events.iter().map(|event| event.body.kind()).collect();
        assert_eq!(
            kinds,
            vec![EventKind::CriterionRecorded, EventKind::SessionStarted]
        );
        match &events[0].body {
            EventBody::CriterionRecorded(body) => {
                assert_eq!(body.criterion_id, "C1");
                assert!(body.passed, "{}", body.evidence);
                assert_eq!(body.run_by, CriterionRecordedBodyRunBy::Reviewer);
                assert_eq!(body.recorded_by, "governor");
            }
            other => panic!("expected a criterion, got {other:?}"),
        }
        assert!(matches!(
            &events[1].body,
            EventBody::SessionStarted(body) if body.purpose == SessionStartedBodyPurpose::Verify
        ));
        assert_eq!(events[1].envelope.ids.agent_id.as_deref(), Some("dev-b"));
        let started = recorded.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Verify);
        assert_eq!(started[0].agent_id, "dev-b");
        assert_eq!(started[0].cwd, harness.worktree("FRK-1"));
        assert_eq!(
            started[0].builtin_tools,
            allowed_builtins(&BTreeSet::from([PermissionTier::Read]))
        );
        assert_eq!(witness.had_executor(), vec![false]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn shows_the_reviewer_the_diff_and_the_note_and_not_the_transcript() {
        let harness = Harness::new("orch-verify-shows", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_finishes_frk_1(), review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        for _ in 0..3 {
            orchestrator.tick().await.expect("the tick runs");
        }

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(started[1].purpose, SessionPurpose::Verify);
        let prompt = &started[1].initial_prompt;
        assert!(block(prompt, "diff").contains("b/done.txt"), "{prompt}");
        assert!(
            block(prompt, "completion_note").contains("Added done.txt; nothing left out."),
            "{prompt}"
        );
        assert!(
            block(prompt, "results").contains("$ test -f done.txt"),
            "{prompt}"
        );
        assert!(
            !prompt.contains("FRK-1 is done and waiting for review."),
            "{prompt}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn does_not_run_the_criteria_twice_in_one_verification() {
        let harness = Harness::new("orch-verify-once", |_| {});
        harness.verifying_with("FRK-1", true, true, with_a_review_criterion);
        let adapter = harness.recorded(vec![review_answers_nothing(), review_answers_nothing()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the first review runs");
        assert_eq!(governor_runs(&harness), vec![("C1".to_string(), true)]);
        orchestrator.tick().await.expect("the second review runs");

        assert_eq!(adapter.started().len(), 2);
        assert_eq!(governor_runs(&harness), vec![("C1".to_string(), true)]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_only_the_criteria_farik_has_not_run() {
        let harness = Harness::new("orch-verify-remaining", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            push_criterion(
                wire,
                json!({
                    "id": "C2",
                    "text": "done.txt is still there.",
                    "verification": {
                        "method": "command",
                        "command": "test -f done.txt",
                        "expect": { "exit_code": 0 }
                    }
                }),
            );
            push_criterion(
                wire,
                json!({
                    "id": "C3",
                    "text": "done.txt can be read.",
                    "verification": { "method": "artifact", "path": "done.txt" }
                }),
            );
        });
        harness.project.record(
            "FRK-1",
            "criterion.recorded",
            &json!({
                "criterion_id": "C1",
                "passed": true,
                "evidence": "$ test -f done.txt\nexit 0",
                "run_by": "reviewer",
                "recorded_by": "governor"
            }),
        );
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            governor_runs(&harness),
            vec![
                ("C1".to_string(), true),
                ("C2".to_string(), true),
                ("C3".to_string(), true)
            ]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn checks_that_a_tasks_new_tests_fail_on_the_integration_branch() {
        let harness = Harness::new("orch-verify-new-tests", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            wire["exit_criteria"] = json!([
                {
                    "id": "C1",
                    "text": "A new test needs done.txt.",
                    "verification": {
                        "method": "test",
                        "command": "sh tests/done_test.sh",
                        "new_tests_required": true
                    }
                },
                {
                    "id": "C2",
                    "text": "A new test passes.",
                    "verification": {
                        "method": "test",
                        "command": "sh tests/always_test.sh",
                        "new_tests_required": true
                    }
                }
            ]);
        });
        let worktree = harness.worktree("FRK-1");
        std::fs::create_dir_all(worktree.join("tests")).expect("a directory");
        std::fs::write(worktree.join("tests/done_test.sh"), "test -f done.txt\n").expect("a test");
        std::fs::write(worktree.join("tests/always_test.sh"), "exit 0\n").expect("a test");
        harness
            .project
            .deps
            .git
            .commit(
                &worktree,
                "Add the tests",
                &[
                    "tests/done_test.sh".to_string(),
                    "tests/always_test.sh".to_string(),
                ],
            )
            .expect("the tests are committed");
        let orchestrator = harness.orchestrator(harness.recorded(vec![review_writes_note()]));

        orchestrator.tick().await.expect("the tick runs");

        // C2's test passes on main as well, so it proves nothing about the change.
        assert_eq!(
            governor_runs(&harness),
            vec![("C1".to_string(), true), ("C2".to_string(), false)]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_the_criteria_of_a_task_out_of_sessions() {
        let harness = Harness::new("orch-verify-no-sessions", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Acted {
                task_id: "FRK-1".parse().expect("an id"),
                what: "ran 1 of its criteria as its reviewer".to_string()
            }
        );
        assert_eq!(governor_runs(&harness), vec![("C1".to_string(), true)]);
        assert!(adapter.started().is_empty());
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
    async fn runs_a_criterion_whose_result_an_agent_recorded_as_the_governor() {
        // An agent may be called `governor`; what it records carries its id on the envelope.
        let harness = Harness::new("orch-verify-named-governor", |_| {});
        harness.verifying("FRK-1");
        let wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "task_id": "FRK-1",
            "agent_id": "dev-b",
            "kind": "criterion.recorded",
            "body": {
                "criterion_id": "C1",
                "passed": true,
                "evidence": "$ test -f done.txt\nexit 0",
                "run_by": "reviewer",
                "recorded_by": "governor"
            },
        });
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        let deps = &harness.project.deps;
        let appended = deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        deps.projections.apply(&appended).expect("projects");
        let orchestrator = harness.orchestrator(harness.recorded(vec![review_writes_note()]));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(governor_runs(&harness), vec![("C1".to_string(), true)]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn rejects_a_failed_review_with_the_reviewers_note() {
        let harness = Harness::new("orch-verify-rejects", |_| {});
        harness.verifying_with("FRK-1", false, true, |_| {});
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the review runs");
        assert_eq!(governor_runs(&harness), vec![("C1".to_string(), false)]);
        assert_eq!(
            reviews(&harness),
            vec![ReviewRecordedBody {
                reviewer: "dev-b".to_string(),
                criteria_run: 1,
                passed: false
            }]
        );
        let prompt = &adapter.started()[0].initial_prompt;
        assert!(block(prompt, "results").contains("C1: failed"), "{prompt}");
        assert!(
            block(prompt, "title").contains("Add a login page"),
            "{prompt}"
        );
        orchestrator.tick().await.expect("the rejection is filed");

        let notes = harness.events(&[EventKind::NoteWritten]);
        let review = notes
            .iter()
            .rev()
            .find(|event| {
                matches!(&event.body, EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Review)
            })
            .expect("a review note");
        let EventBody::NoteWritten(note) = &review.body else {
            panic!("a note");
        };
        let transitions = harness.events(&[EventKind::TaskTransitioned]);
        let rejection = transitions.last().expect("a move");
        let EventBody::TaskTransitioned(moved) = &rejection.body else {
            panic!("a move");
        };
        assert_eq!(moved.from.to_string(), "verifying");
        assert_eq!(moved.to.to_string(), "rejected");
        assert_eq!(moved.actor, TransitionActorWire::Reviewer);
        assert_eq!(moved.requested_by, "dev-b");
        let reasons = moved
            .rejection
            .as_ref()
            .expect("the move carries its rejection");
        assert_eq!(reasons.failed_criterion_ids, vec!["C1".to_string()]);
        assert_eq!(reasons.reasons, note.text);
        assert!(review.envelope.ids.session_id.is_some());
        assert_eq!(
            rejection.envelope.ids.session_id,
            review.envelope.ids.session_id
        );
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_the_reviewer_again_for_an_unanswered_criterion() {
        let harness = Harness::new("orch-verify-again", |_| {});
        harness.verifying_with("FRK-1", true, true, with_a_review_criterion);
        let adapter = harness.recorded(vec![review_answers_nothing(), review_answers_nothing()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the first review runs");
        // C2 is unanswered, so the review is not complete.
        assert!(reviews(&harness).is_empty());
        orchestrator.tick().await.expect("the second review runs");

        let started = adapter.started();
        assert!(
            block(&started[0].initial_prompt, "rubric").contains("Is done.txt at the root?"),
            "{}",
            started[0].initial_prompt
        );
        assert_eq!(
            sessions(&adapter),
            vec![
                ("dev-b".to_string(), SessionPurpose::Verify),
                ("dev-b".to_string(), SessionPurpose::Verify),
            ]
        );
        assert!(
            !started[0].initial_prompt.contains("Still unanswered"),
            "{}",
            started[0].initial_prompt
        );
        let prompt = &started[1].initial_prompt;
        assert!(prompt.contains("Still unanswered: C2"), "{prompt}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_a_passed_review_to_the_product_manager() {
        let harness = Harness::new("orch-verify-hands-on", |_| {});
        harness.verifying("FRK-1");
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the review runs");
        assert_eq!(
            reviews(&harness),
            vec![ReviewRecordedBody {
                reviewer: "dev-b".to_string(),
                criteria_run: 1,
                passed: true
            }]
        );
        orchestrator
            .tick()
            .await
            .expect("the Product Manager's session runs");

        let started = adapter.started();
        assert_eq!(
            sessions(&adapter),
            vec![
                ("dev-b".to_string(), SessionPurpose::Verify),
                ("pm".to_string(), SessionPurpose::Verify),
            ]
        );
        let prompt = &started[1].initial_prompt;
        assert!(
            block(prompt, "review_note").contains("the diff adds done.txt and nothing else"),
            "{prompt}"
        );
        assert!(prompt.contains("`accepted`"), "{prompt}");
        assert_eq!(started[1].cwd, harness.worktree("FRK-1"));
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_high_risk_task_for_the_human() {
        let harness = Harness::new("orch-verify-high-risk", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| wire["risk"] = json!("high"));
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the review runs");
        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_task_with_a_human_criterion_for_the_human() {
        let harness = Harness::new("orch-verify-human-criterion", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            push_criterion(
                wire,
                json!({
                    "id": "C2",
                    "text": "The founder has opened done.txt.",
                    "verification": { "method": "human", "question": "Did you open done.txt?" }
                }),
            );
        });
        let adapter = harness.recorded(vec![
            review_writes_note(),
            review_writes_note(),
            accept_frk_1(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the review runs");
        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            reviews(&harness),
            vec![ReviewRecordedBody {
                reviewer: "dev-b".to_string(),
                criteria_run: 1,
                passed: true
            }]
        );
        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_whose_request_was_refused() {
        let harness = Harness::new("orch-verify-refused", |_| {});
        harness.verifying_with("FRK-1", true, false, |_| {});
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the review runs");
        orchestrator
            .tick()
            .await
            .expect("the Product Manager's session runs");
        assert_eq!(harness.events(&[EventKind::TransitionRefused]).len(), 1);
        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert_eq!(adapter.started().len(), 2);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Verifying);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn runs_until_idle_past_a_refused_acceptance() {
        let harness = Harness::new("orch-verify-refused-run", |_| {});
        harness.verifying_with("FRK-1", true, false, |_| {});
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        run_until_idle_within_ten_seconds(orchestrator).expect("the run is idle");

        assert_eq!(
            sessions(&adapter),
            vec![
                ("dev-b".to_string(), SessionPurpose::Verify),
                ("pm".to_string(), SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.events(&[EventKind::TransitionRefused]).len(), 1);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Verifying);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_the_review_once_per_verification() {
        // The first session writes no note, so the reviewer is asked again after every criterion
        // already has its result.
        let harness = Harness::new("orch-verify-review-once", |_| {});
        harness.verifying("FRK-1");
        let adapter = harness.recorded(vec![reads_a_file(), review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the first review runs");
        assert_eq!(reviews(&harness).len(), 1);
        orchestrator.tick().await.expect("the second review runs");

        assert_eq!(
            sessions(&adapter),
            vec![
                ("dev-b".to_string(), SessionPurpose::Verify),
                ("dev-b".to_string(), SessionPurpose::Verify),
            ]
        );
        assert_eq!(reviews(&harness).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_the_criteria_in_a_fresh_sandbox() {
        let harness = Harness::new("orch-verify-fresh-sandbox", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
        ]);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        orchestrator.tick().await.expect("the task starts");
        orchestrator
            .tick()
            .await
            .expect("the implement session runs");
        assert_eq!(sandboxes.created("FRK-1"), 1);
        orchestrator.tick().await.expect("the review runs");
        // Not the assignee's: whatever its commands left outside the worktree is gone.
        assert_eq!(sandboxes.created("FRK-1"), 2);
        orchestrator
            .tick()
            .await
            .expect("the Product Manager's session runs");

        assert_eq!(sandboxes.created("FRK-1"), 2);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_a_criterion_whose_container_went_as_failed() {
        let harness = Harness::new("orch-verify-container-gone", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            push_criterion(
                wire,
                json!({
                    "id": "C2",
                    "text": "done.txt is still there.",
                    "verification": {
                        "method": "command",
                        "command": "test -f done.txt",
                        "expect": { "exit_code": 0 }
                    }
                }),
            );
        });
        let adapter = harness.recorded(vec![review_writes_note()]);
        let sandboxes = Arc::new(BrokenSandboxFactory::new(ExecError::ContainerGone, 1));
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(
            governor_runs(&harness),
            vec![("C1".to_string(), false), ("C2".to_string(), true)]
        );
        let evidence: Vec<String> = harness
            .events(&[EventKind::CriterionRecorded])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::CriterionRecorded(body) if body.criterion_id == "C1" => {
                    Some(body.evidence.clone())
                }
                _ => None,
            })
            .collect();
        assert!(
            evidence[0].contains("the task's container is no longer running"),
            "{evidence:?}"
        );
        // C2 ran in a sandbox made after C1's went.
        assert_eq!(sandboxes.created("FRK-1"), 2);
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
        orchestrator.tick().await.expect("the rejection is filed");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Rejected);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn escalates_a_task_whose_criterion_farik_could_not_run() {
        let harness = Harness::new("orch-verify-unrunnable", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(2);
        });
        harness.file("FRK-1", "ready", |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.verifying("FRK-2");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let sandboxes = Arc::new(BrokenSandboxFactory::new(
            ExecError::SpawnFailed {
                detail: "no shell".to_string(),
            },
            u32::MAX,
        ));
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes);

        run_until_idle_within_ten_seconds(orchestrator).expect("the run is idle");

        assert_eq!(harness.row("FRK-2").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::ExplicitRequest]
        );
        let escalations = harness.events(&[EventKind::EscalationRaised]);
        let EventBody::EscalationRaised(escalation) = &escalations[0].body else {
            panic!("an escalation.raised");
        };
        assert!(escalation.detail.contains("C1"), "{}", escalation.detail);
        assert!(
            escalation.detail.contains("no shell"),
            "{}",
            escalation.detail
        );
        assert!(governor_runs(&harness).is_empty());
        // The run went on to the ready task.
        assert_eq!(
            sessions(&adapter),
            vec![("pm".to_string(), SessionPurpose::Plan)]
        );
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

    /// A `.farik/prices.json` that prices a model no agent of the harness runs, so that costing any
    /// of their sessions fails.
    fn prices_without_the_teams_models(harness: &Harness) {
        let prices = json!({
            "version": 1,
            "source_url": "https://example.com/prices",
            "retrieved_at": "2026-09-22",
            "prices": {
                "test-model": {
                    "input_usd_per_mtok": 1.0,
                    "output_usd_per_mtok": 2.0,
                    "cache_read_usd_per_mtok": 0.5,
                    "cache_write_usd_per_mtok": 0.25
                }
            }
        });
        std::fs::write(
            harness.project.repo.path.join(".farik/prices.json"),
            prices.to_string(),
        )
        .expect("the override is written");
    }

    /// The reason of every `session.ended`, in order.
    fn end_reasons(harness: &Harness) -> Vec<SessionEndedBodyReason> {
        harness
            .events(&[EventKind::SessionEnded])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::SessionEnded(body) => Some(body.reason),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_that_fails_after_it_started() {
        // The usage crosses the session's tokens, and the abort that follows fails: the tick
        // fails with the session running, which is then aborted again and ended.
        let harness = Harness::new("orch-session-fails", |wire| {
            wire["budgets"]["session"] = json!({ "max_input_tokens": 100 });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::failing_to_abort(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        // A tick that waits for the session's end waits for ever.
        let ticked = tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the tick ends");

        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        assert_eq!(adapter.aborts(), 2);
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
        assert_eq!(harness.events(&[EventKind::CostRecorded]).len(), 1);
        let session_id = adapter.started()[0].session_id.clone();
        assert!(harness.daemon.tool_context(&session_id).is_none());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_whose_usage_cannot_be_costed() {
        let harness = Harness::new("orch-session-uncosted", |_| {});
        prices_without_the_teams_models(&harness);
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        // A session left running waits for ever.
        let ticked = tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the tick ends");

        assert!(
            matches!(ticked, Err(OrchestratorError::Cost(_))),
            "{ticked:?}"
        );
        assert_eq!(adapter.aborts(), 1);
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_that_cannot_start_when_its_cost_fails() {
        let harness = Harness::new("orch-session-no-start-no-cost", |_| {});
        prices_without_the_teams_models(&harness);
        harness.ready("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let ticked = orchestrator.tick().await;

        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
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

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn breaks_ties_by_the_number_in_the_task_id() {
        // Filed tenth first, so that neither the order of filing nor the order of the ids as text
        // puts FRK-2 first.
        let harness = Harness::new("orch-order-number", |_| {});
        harness.assigned("FRK-10", "dev-b", "dev-a");
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn makes_a_tasks_sandbox_once_with_its_assignees_network() {
        let harness = Harness::new("orch-sandbox-network", |wire| {
            wire["agents"][1]["grants"] = json!(["network"]);
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_stops_early(), implement_stops_early()]);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        orchestrator.tick().await.expect("the first session runs");
        orchestrator.tick().await.expect("the second session runs");

        assert_eq!(adapter.started().len(), 2);
        assert_eq!(sandboxes.networks("FRK-1"), vec![true]);

        let harness = Harness::new("orch-sandbox-no-network", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(
            harness.recorded(vec![implement_stops_early()]),
            sandboxes.clone(),
        );

        orchestrator.tick().await.expect("the session runs");

        assert_eq!(sandboxes.networks("FRK-1"), vec![false]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reuses_an_assigned_tasks_worktree() {
        let harness = Harness::new("orch-start-reuse", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        harness
            .project
            .deps
            .git
            .create_worktree(&harness.worktree("FRK-1"), "farik/FRK-1", "main")
            .expect("the worktree is made");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_the_work_of_an_assignee_who_is_not_active() {
        let harness = Harness::new("orch-paused-assignee", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert!(adapter.started().is_empty());
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Assigned);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn forgets_a_note_from_before_the_task_started() {
        let harness = Harness::new("orch-resume-old-note", |_| {});
        harness.assigned("FRK-1", "dev-a", "dev-b");
        harness.project.record(
            "FRK-1",
            "note.written",
            &json!({ "kind": "progress", "text": "an old plan", "written_by": "pm" }),
        );
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the task starts");
        orchestrator.tick().await.expect("the session runs");

        let prompt = &adapter.started()[0].initial_prompt;
        assert!(!prompt.contains("an old plan"), "{prompt}");
        assert!(!prompt.contains("Resuming"), "{prompt}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_epics_child_to_its_breakdown() {
        let harness = Harness::new("orch-plan-child", |_| {});
        harness
            .project
            .filed_with("FRK-1", "ready", "task", Some("FRK-9"), |_| {});
        let adapter = harness.recorded(vec![reads_a_file()]);
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
    async fn leaves_cancelled_work_out_of_the_wip_limit() {
        let harness = Harness::new("orch-plan-cancelled", |_| {});
        for (task, assignee, reviewer) in [("FRK-1", "dev-a", "dev-b"), ("FRK-2", "dev-b", "dev-a")]
        {
            harness.assigned(task, assignee, reviewer);
            harness.project.moved(
                task,
                "assigned",
                "cancelled",
                &json!({
                    "actor": "human",
                    "requested_by": "human",
                    "assignee": assignee,
                    "reviewer": reviewer
                }),
            );
            assert_eq!(harness.row(task).assignee_id.as_deref(), Some(assignee));
        }
        harness.ready("FRK-3");
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-3"), "{report:?}");
        assert_eq!(adapter.started()[0].purpose, SessionPurpose::Plan);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_an_agent_on_its_own_model() {
        let harness = Harness::new("orch-session-model", |wire| {
            wire["agents"][1]["model"] = json!({ "id": "claude-sonnet-5", "effort": "low" });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the session runs");

        let spec = &adapter.started()[0];
        assert_eq!(spec.model, "claude-sonnet-5");
        assert_eq!(spec.effort, Effort::Low);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn gives_a_session_the_farik_tools_of_its_tiers() {
        let harness = Harness::new("orch-session-tools", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the session runs");

        let spec = &adapter.started()[0];
        let tiers = default_tiers(Role::ProductManager);
        let expected: Vec<String> = tool_descriptors()
            .iter()
            .filter(|tool| tiers.contains(&tool.tier))
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(spec.farik_tools, expected);
        assert!(spec.farik_tools.contains(&"farik_assign_task".to_string()));
        assert!(!spec.farik_tools.contains(&"farik_exec".to_string()));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn counts_the_daemons_tool_calls_against_the_session() {
        let harness = Harness::new("orch-session-tool-calls", |wire| {
            wire["budgets"]["session"] = json!({ "max_tool_calls": 1 });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let orchestrator = harness.orchestrator(harness.recorded(vec![implement_stops_early()]));

        orchestrator.tick().await.expect("the session runs");

        assert_eq!(
            scopes_exhausted(&harness),
            vec![BudgetExhaustedBodyScope::SessionToolCalls]
        );
    }
}
