//! The order of work, and what each state on the board asks of the orchestrator. A tick takes the
//! rules in order, one per state, and each rule the tasks in its state by their number; the first
//! that acts ends the tick.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_core::branch::task_branch;
use farik_core::budget::{BudgetScope, SessionLedger, check_budgets};
use farik_core::contract::{Role, TaskContract, TaskId, TaskStatus};
use farik_core::governor::gates::fits_the_open_sprint;
use farik_core::governor::task_status::is_terminal;
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_protocol::event::{EventBody, EventKind, FarikEvent};
use farik_store::{EventQuery, Git, TaskProjection};

use super::integrate::{awaiting, cleanup};
use super::messages::{
    Resume, implement_message, mention_message, plan_message, sprint_plan_message,
};
use super::requests;
use super::session::{SPRINT_PLAN_TOOL, SessionAsk, SessionEnd, run_session};
use super::verify::verifying;
use super::{
    Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, TickRules, TickScope, worktree,
};
use crate::channel::{channel_summary, pending_mentions};
use crate::cost::budget_state;
use crate::exec::Executor;
use crate::session::{EndReason, SessionPurpose};
use crate::sleep::asleep_until;
use crate::sprints::{EndedBy, end_sprint, planning_session_spent};
use crate::transitions::{self, TransitionAsk, TransitionOutcome, integration_branch};

/// What a tick says when no rule matched.
const NOTHING_TO_DO: &str = "nothing on the board needs doing";
/// What a tick says when the only rules that matched would have started a session on a spent day.
const DAY_SPENT: &str = "the team's daily budget is spent";

/// What kept the rules of one tick from starting a session, gathered as they pass work over.
#[derive(Debug, Default)]
pub(super) struct Waiting {
    /// Whether the team's daily budget stopped one (`spent`).
    pub(super) day_spent: bool,
    /// The earliest time a sleeping agent whose session was not started wakes, and that agent
    /// (`asleep`).
    pub(super) slept: Option<(DateTime<Utc>, String)>,
}

/// One tick within `scope`: the first rule of the scope's set that acts on a task in scope, or
/// `Idle`. A rule outside the set passes its tasks over, as a spent budget does.
pub(super) async fn tick(
    orchestrator: &Orchestrator,
    scope: &TickScope,
) -> Result<TickReport, OrchestratorError> {
    let deps = &orchestrator.deps;
    // A command another process handled is on this process's board before anything is read.
    deps.tools.projections.catch_up()?;
    let team = deps.tools.files.read_team()?;
    // The store gives the board in the order of the number in each task id, which is how ties
    // are broken.
    let board = deps.tools.projections.board()?;
    let in_scope = in_scope(scope);
    let runs = |rule: u8| rule_runs(scope.rules, rule);
    let mut waiting = Waiting::default();
    if let Some(report) = sprint_rules(deps, scope, &team, &board, &mut waiting).await? {
        return Ok(report);
    }
    if runs(1) {
        for row in board
            .iter()
            .filter(|row| matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
            .filter(in_scope)
        {
            if let Some(report) = cleanup(orchestrator, row)? {
                return Ok(report);
            }
        }
    }
    if runs(2) {
        for row in board
            .iter()
            .filter(|row| row.status == TaskStatus::Accepted)
            .filter(in_scope)
        {
            if let Some(report) = awaiting(orchestrator, &team, row).await? {
                return Ok(report);
            }
        }
    }
    if let Some(report) = budget_and_channel(deps, scope, &team, &board, &mut waiting).await? {
        return Ok(report);
    }
    if runs(3) {
        for row in waiting_on_nobody(&board, TaskStatus::Rejected).filter(in_scope) {
            if let Some(report) = rejected(deps, &team, row)? {
                return Ok(report);
            }
        }
    }
    if runs(4) {
        for row in waiting_on_nobody(&board, TaskStatus::Blocked).filter(in_scope) {
            if let Some(report) = blocked(deps, &team, row)? {
                return Ok(report);
            }
        }
    }
    if runs(5) {
        for row in waiting_on_nobody(&board, TaskStatus::Verifying).filter(in_scope) {
            if let Some(report) = verifying(orchestrator, &team, row, &mut waiting).await? {
                return Ok(report);
            }
        }
    }
    // Planning runs an epic's breakdown and close-out, which are plan sessions, and no
    // implement session.
    let epics_only = scope.rules == TickRules::Planning;
    if runs(6) || epics_only {
        for row in waiting_on_nobody(&board, TaskStatus::InProgress)
            .filter(in_scope)
            .filter(|row| !epics_only || requests::is_epic(row))
        {
            if let Some(report) = in_progress(orchestrator, &team, row, &mut waiting).await? {
                return Ok(report);
            }
        }
    }
    if runs(7) {
        for row in waiting_on_nobody(&board, TaskStatus::Assigned).filter(in_scope) {
            if let Some(report) = assigned(deps, &team, row)? {
                return Ok(report);
            }
        }
    }
    if runs(8) {
        for row in waiting_on_nobody(&board, TaskStatus::Ready).filter(in_scope) {
            if let Some(report) = ready(deps, &team, &board, row, &mut waiting).await? {
                return Ok(report);
            }
        }
    }
    if runs(9) {
        for row in waiting_on_nobody(&board, TaskStatus::Refining).filter(in_scope) {
            if let Some(report) = requests::refining(deps, &team, row, &mut waiting).await? {
                return Ok(report);
            }
        }
    }
    if runs(10) {
        for row in waiting_on_nobody(&board, TaskStatus::Draft).filter(in_scope) {
            if let Some(report) = requests::draft(deps, &team, row, &mut waiting).await? {
                return Ok(report);
            }
        }
    }
    Ok(idle(&waiting))
}

/// What an idle tick says, from what kept its rules from starting a session: a spent day before a
/// sleeping agent, and the time the first sleeping agent wakes whichever it names.
fn idle(waiting: &Waiting) -> TickReport {
    let why = match &waiting.slept {
        _ if waiting.day_spent => DAY_SPENT.to_string(),
        Some((until, agent)) => format!(
            "waiting for {agent}, asleep until {} (its model's usage limit)",
            until.format("%Y-%m-%d %H:%M:%S UTC")
        ),
        None => NOTHING_TO_DO.to_string(),
    };
    TickReport::Idle {
        why,
        until: waiting.slept.as_ref().map(|(until, _)| *until),
    }
}

/// Whether `scope` takes in a row: every row, or the one task it names.
fn in_scope(scope: &TickScope) -> impl Fn(&&TaskProjection) -> bool + Copy + '_ {
    move |row| {
        scope
            .task_id
            .as_ref()
            .is_none_or(|task_id| &row.task_id == task_id)
    }
}

/// The sprint rules, which come before the numbered ones: the sprint that ends by itself, then the
/// open sprint's planning.
async fn sprint_rules(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if let Some(report) = finished_sprint(deps, scope, board)? {
        return Ok(Some(report));
    }
    sprint_planning(deps, scope, team, board, waiting).await
}

/// The sprint that ends by itself (5.5): the open sprint, once it holds a task and every task in it
/// is accepted or cancelled, ended by the governor. A sprint rule is about no one task, so it runs
/// only in a tick scoped to none, under `All` or `Planning`.
fn finished_sprint(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    board: &[TaskProjection],
) -> Result<Option<TickReport>, OrchestratorError> {
    if !sprint_rules_run(scope) {
        return Ok(None);
    }
    let Some(open) = deps.tools.projections.open_sprint()? else {
        return Ok(None);
    };
    let mut held = board
        .iter()
        .filter(|row| row.sprint.as_deref() == Some(open.sprint_id.as_str()))
        .peekable();
    if held.peek().is_none()
        || !held.all(|row| matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
    {
        return Ok(None);
    }
    let sprint = end_sprint(&deps.tools, EndedBy::Governor)?;
    Ok(Some(TickReport::Sprint {
        sprint_id: sprint.id.as_str().to_string(),
        what: "ended it: every task in it is accepted or cancelled".to_string(),
    }))
}

/// Whether the sprint rules run in `scope`: they are about no one task, so only in a tick scoped
/// to none, under `All` or `Planning`.
fn sprint_rules_run(scope: &TickScope) -> bool {
    scope.task_id.is_none() && scope.rules != TickRules::Refining
}

/// The open sprint's planning (5.5): while it holds no task and has had no planning session, the
/// assigner gets one `plan` session about no task, given `farik_plan_sprint` alone and offered the
/// candidates, the rows `ready` with no parent and in no sprint. Passed over with no candidate, no
/// assigner, or on a spent day; a planning session that plans nothing is not asked again, and the
/// empty sprint waits for the human to end it.
async fn sprint_planning(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if !sprint_rules_run(scope) {
        return Ok(None);
    }
    let Some(open) = deps.tools.projections.open_sprint()? else {
        return Ok(None);
    };
    let candidates: Vec<&TaskProjection> = board
        .iter()
        .filter(|row| {
            row.status == TaskStatus::Ready && row.parent.is_none() && row.sprint.is_none()
        })
        .collect();
    let Some(assigner) = assigner(team) else {
        return Ok(None);
    };
    if candidates.is_empty()
        || board
            .iter()
            .any(|row| row.sprint.as_deref() == Some(open.sprint_id.as_str()))
        || planning_session_spent(&deps.tools.log, &open.sprint_id)?
    {
        return Ok(None);
    }
    if day_is_spent(
        deps,
        team,
        Role::from(assigner.role),
        &mut waiting.day_spent,
    )? | asleep(deps, assigner, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let contracts = candidates
        .iter()
        .map(|row| deps.tools.files.read_contract(&row.task_id))
        .collect::<Result<Vec<_>, _>>()?;
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assigner,
            contract: None,
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            only_tool: Some(SPRINT_PLAN_TOOL),
            tools: None,
            in_reply_to: None,
            initial_prompt: sprint_plan_message(&open.sprint_id, &contracts, open.budget_usd),
        },
    )
    .await?;
    Ok(Some(TickReport::Sprint {
        sprint_id: open.sprint_id,
        what: ran(assigner, "plan", &end),
    }))
}

/// Whether the day's dollars stop a session about no task from starting; `day_spent` is set when
/// they do, as `spent` sets it.
fn day_is_spent(
    deps: &OrchestratorDeps,
    team: &Team,
    role: Role,
    day_spent: &mut bool,
) -> Result<bool, OrchestratorError> {
    let state = budget_state(
        &deps.tools.projections,
        team,
        role,
        None,
        &SessionLedger::default(),
        deps.tools.clock.now(),
    )?;
    let spent = check_budgets(&state)
        .iter()
        .any(|exhausted| exhausted.scope == BudgetScope::DayUsd);
    *day_spent |= spent;
    Ok(spent)
}

/// The Farik tools a conversation session is offered (5.9): the reading tools, its one post, and
/// a request filed without a parent.
const CONVERSATION_TOOLS: &[&str] = &[
    "farik_read_task",
    "farik_read_board",
    "farik_read_rules",
    "farik_read_criteria",
    "farik_post_message",
    "farik_create_task",
];

/// The channel rule, between the budget rule and rule 3 (5.9): the first active agent in team
/// order with a pending mention, awake, on a day whose budget is not spent, gets one
/// `conversation` session about no task, on the read tier's built-ins and `CONVERSATION_TOOLS`,
/// to answer it. It is about no one task, so it runs only under `All` in a tick scoped to none.
async fn conversation(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if scope.rules != TickRules::All || scope.task_id.is_some() {
        return Ok(None);
    }
    for agent in team.active_agents() {
        let pending = pending_mentions(&deps.tools.log, agent.id.as_str())?;
        let Some(latest) = pending.last() else {
            continue;
        };
        if day_is_spent(deps, team, Role::from(agent.role), &mut waiting.day_spent)?
            | asleep(deps, agent, &mut waiting.slept)?
        {
            continue;
        }
        let summary = channel_summary(&deps.tools.log, &deps.tools.files)?;
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent,
                contract: None,
                purpose: SessionPurpose::Conversation,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: true,
                only_tool: None,
                tools: Some(CONVERSATION_TOOLS),
                in_reply_to: Some(latest.envelope.seq),
                initial_prompt: mention_message(agent, &pending, &summary),
            },
        )
        .await?;
        return Ok(Some(TickReport::Conversation {
            agent_id: agent.id.to_string(),
            what: ran(agent, "conversation", &end),
        }));
    }
    Ok(None)
}

/// Whether `rules` runs rule `rule` (1 to 10, in the order of work) for every task. `Planning`
/// runs rule 6 for epics alone, which the tick decides.
fn rule_runs(rules: TickRules, rule: u8) -> bool {
    match rules {
        TickRules::All => true,
        TickRules::Planning => matches!(rule, 8..=10),
        TickRules::Refining => matches!(rule, 9 | 10),
    }
}

/// The tasks in `status`, in the board's order, but those waiting on the human's answer to a
/// question, which rules 3 to 10 pass over.
fn waiting_on_nobody(
    board: &[TaskProjection],
    status: TaskStatus,
) -> impl Iterator<Item = &TaskProjection> {
    board
        .iter()
        .filter(move |row| row.status == status && !row.waiting_on_human)
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

/// The rules between rules 2 and 3: the budget rule, then the channel rule.
async fn budget_and_channel(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if let Some(report) = budget(deps, scope, team, board)? {
        return Ok(Some(report));
    }
    conversation(deps, scope, team, waiting).await
}

/// The budget rule, between rules 2 and 3: a task whose dollars or sessions are spent is escalated
/// as the governor, whose `GovernorEscalation` gate names the reason (5.7). One whose escalation
/// was refused since it entered its status is passed over; a task the human resumed without more
/// room entered a new status, so it is escalated again, which is how Farik says it is still spent.
/// It governs work in progress, so it runs only in a tick of every rule scoped to no task; it
/// passes over a terminal or escalated task and one waiting on the human.
fn budget(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
) -> Result<Option<TickReport>, OrchestratorError> {
    if scope.rules != TickRules::All || scope.task_id.is_some() {
        return Ok(None);
    }
    for row in board.iter().filter(|row| {
        !is_terminal(row.status) && row.status != TaskStatus::Escalated && !row.waiting_on_human
    }) {
        if let Some(report) = escalate_if_spent(deps, team, row)? {
            return Ok(Some(report));
        }
    }
    Ok(None)
}

/// The budget rule for one task: escalated when its dollars or sessions are spent and that move
/// was not refused since it entered its status.
fn escalate_if_spent(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let state = budget_state(
        &deps.tools.projections,
        team,
        contract.assignee_role,
        Some(&contract),
        &SessionLedger::default(),
        deps.tools.clock.now(),
    )?;
    let task_spent = check_budgets(&state).iter().any(|exhausted| {
        matches!(
            exhausted.scope,
            BudgetScope::TaskUsd | BudgetScope::TaskSessions
        )
    });
    if !task_spent || refused_since_entering(deps, &row.task_id, row.status, TaskStatus::Escalated)?
    {
        return Ok(None);
    }
    match governor_moves(deps, team, row, TaskStatus::Escalated)? {
        TransitionOutcome::Moved(_) => Ok(Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: "escalated it: its dollars or sessions are spent".to_string(),
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
    Ok(transitions::last_move_into(&moves, status).cloned())
}

/// Whether the task's move from `from` to `to` was refused since the task last moved into `from`.
/// Such a move is not asked again until the task moves: nothing but a move changes what the
/// governor would answer, and the refusal is on the board for the human.
pub(super) fn refused_since_entering(
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

/// Whether a budget stops a session about `contract` from starting, read with an empty session
/// ledger: the day's dollars, then the task's dollars and sessions. `day_spent` is set when the
/// day is what stops it.
pub(super) fn spent(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    day_spent: &mut bool,
) -> Result<bool, OrchestratorError> {
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
        return Ok(true);
    }
    Ok(exhausted
        .iter()
        .any(|scope| matches!(scope, BudgetScope::TaskUsd | BudgetScope::TaskSessions)))
}

/// Whether `agent` is asleep until its model provider's limit resets (5.5): no session of its
/// starts, and no sandbox is made for it. The earliest `until` of the tick and its agent are kept
/// in `slept`, as `spent` keeps a spent day in `day_spent`. A rule asks it with `|` beside
/// `spent`, not `||`, so that both are kept whichever stops the session.
pub(super) fn asleep(
    deps: &OrchestratorDeps,
    agent: &Agent,
    slept: &mut Option<(DateTime<Utc>, String)>,
) -> Result<bool, OrchestratorError> {
    let Some(until) = asleep_until(&deps.tools.log, agent.id.as_str(), deps.tools.clock.now())?
    else {
        return Ok(false);
    };
    if slept.as_ref().is_none_or(|(earlier, _)| until < *earlier) {
        *slept = Some((until, agent.id.to_string()));
    }
    Ok(true)
}

/// Rule 6: a task `in_progress` gets its assignee's implement session, in its worktree, with its
/// sandbox, told where an earlier session left the work. A task whose assignee is not active is
/// passed over: every tool call of its session would be refused.
async fn in_progress(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    if requests::is_epic(row) {
        let board = deps.tools.projections.board()?;
        return requests::in_progress_epic(deps, team, &board, row, waiting).await;
    }
    let Some(assignee) = active(team, row.assignee_id.as_deref()) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | asleep(deps, assignee, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let sandbox = orchestrator.sandbox_for(&row.task_id, team)?;
    let resume = resume(deps, team, &contract)?;
    let executor: Arc<dyn Executor> = sandbox;
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assignee,
            contract: Some(&contract),
            purpose: SessionPurpose::Implement,
            cwd: worktree(deps, &row.task_id),
            executor: Some(executor),
            read_only: false,
            only_tool: None,
            tools: None,
            in_reply_to: None,
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
    contract: &TaskContract,
) -> Result<Resume, OrchestratorError> {
    let task_id = &contract.id;
    let worktree = worktree(deps, task_id);
    let last_commit = if worktree.is_dir() {
        let git = &deps.tools.git;
        let base = integration_branch(team, git)?;
        if git.commit_count(&base, &task_branch(contract))? > 0 {
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

/// Rule 7: a task `assigned` gets its worktree on its branch (5.14) from the integration branch,
/// reused when it is already there, and is moved to `in_progress` as its assignee asks. No session
/// starts: the next tick's rule 6 starts it. A task whose assignee is not active is passed over, as
/// is one whose start the governor refused, now or since it was assigned.
fn assigned(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if requests::is_epic(row) {
        return requests::assigned_epic(deps, team, row);
    }
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
    let branch = task_branch(&deps.tools.files.read_contract(&row.task_id)?);
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
/// Master, else the Product Manager), when an agent of its assignee role has room for it, the open
/// sprint, if any, holds it and can pay for it, and every dependency is accepted and integrated.
async fn ready(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if requests::is_epic(row) {
        return requests::ready_epic(deps, team, board, row);
    }
    // A task under an epic is assigned by the epic's assignee (5.16 item 3).
    let assigner = match &row.parent {
        Some(_) => requests::epic_assignee(team, board, row),
        None => assigner(team),
    };
    let Some(assigner) = assigner else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    // A sleeping agent is still offered as an assignee below; only its own sessions wait.
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | asleep(deps, assigner, &mut waiting.slept)?
    {
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
    if !assignable(deps, team, &contract, assigner, first)? {
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
            contract: Some(&contract),
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            only_tool: None,
            tools: None,
            in_reply_to: None,
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
pub(super) fn has_room(team: &Team, board: &[TaskProjection], agent: &Agent) -> bool {
    u64::from(transitions::open_tasks(board, agent.id.as_str()))
        < u64::try_from(team.policy.wip_limit_per_agent).unwrap_or(0)
}

/// Whether the governor would let `candidate` be assigned the task on the rules the orchestrator
/// checks before asking, read from the governor's own context for the assignment: the open sprint's
/// membership and budget, by the gate's own predicates, and every dependency accepted and
/// integrated. A task that fails them is passed over, so that it is not asked about on every tick.
fn assignable(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    assigner: &Agent,
    candidate: &str,
) -> Result<bool, OrchestratorError> {
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
    let Some(assignment) = deps
        .tools
        .transitions
        .context(&request, &ask, team)?
        .assignment
    else {
        return Ok(false);
    };
    let states = &assignment.dependencies;
    Ok(fits_the_open_sprint(contract, &assignment)
        && states.len() == contract.dependencies.len()
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
    TickReport::Acted {
        task_id: row.task_id.clone(),
        what: ran(agent, purpose, end),
    }
}

/// What a tick says of the session it ran: whose, for what, how it ended, and its last words.
fn ran(agent: &Agent, purpose: &str, end: &SessionEnd) -> String {
    let how = match end.reason {
        EndReason::Completed => "completed",
        EndReason::Aborted => "was aborted",
        EndReason::Limit => "reached a limit",
        EndReason::Error => "failed",
        EndReason::ProviderLimit => "stopped at its model provider's limit",
    };
    format!(
        "ran {}'s {purpose} session, which {how}: {}",
        agent.id.as_str(),
        end.detail
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::NonZeroU64;
    use std::sync::Arc;
    use std::time::Duration;

    use farik_core::contract::{Role, TaskStatus};
    use farik_core::governor::permissions::{PermissionTier, default_tiers};
    use farik_core::pricing::Usage;
    use farik_core::sprint::SprintStatus;
    use farik_core::team::Effort;
    use farik_protocol::event::SprintEndedBodyEndedBy;
    use farik_protocol::event::{
        BudgetExhaustedBodyScope, CriterionRecordedBodyRunBy, EscalationRaisedBodyReason,
        EventBody, EventKind, NoteWrittenBodyKind, ReviewRecordedBody, SessionEndedBodyReason,
        SessionStartedBodyPurpose, TransitionActorWire,
    };
    use farik_protocol::event::{MessageKind, NewEvent, event_from_value};
    use farik_store::event_log::fixtures::refuse_appends_of;
    use farik_store::git::fixtures::git_output_in;
    use serde_json::json;

    use crate::claude::allowed_builtins;
    use crate::cost::CostError;
    use crate::exec::ExecError;
    use crate::orchestrator::fixtures::{
        BrokenSandboxFactory, CountingSandboxFactory, ExecutorWitness, Harness,
        UnremovableSandboxFactory, UsageThenWaitAdapter, run_until_idle_within_ten_seconds,
        waits_for,
    };
    use crate::orchestrator::{OrchestratorError, TickReport, TickRules, TickScope};
    use crate::recorded::fixtures::{
        accept_frk_1, hits_the_turn_limit, implement_finishes_frk_1, implement_stops_early,
        plan_assigns_frk_1, plan_sprint_frk_1, provider_limit_429, provider_limit_rejected,
        reads_a_file, replays_farik_read_board, reply_to_a_mention, review_answers_nothing,
        review_writes_note,
    };
    use crate::recorded::{RecordedAdapter, Transcript};
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;
    use crate::tools::tool_descriptors;

    const NOTHING_TO_DO: &str = "nothing on the board needs doing";

    fn acted_on(report: &TickReport) -> Option<&str> {
        match report {
            TickReport::Acted { task_id, .. } => Some(task_id.as_str()),
            TickReport::Idle { .. }
            | TickReport::Sprint { .. }
            | TickReport::Conversation { .. } => None,
        }
    }

    /// A transcript with every `from` in its text replaced by `to`.
    fn rewritten(transcript: &Transcript, from: &str, to: &str) -> Transcript {
        Transcript::from_jsonl(
            &transcript
                .lines()
                .collect::<Vec<_>>()
                .join("\n")
                .replace(from, to),
        )
    }

    /// Records `task`'s triage as the human's, `small`.
    fn triaged_small(harness: &Harness, task: &str) {
        harness.project.record(
            task,
            "request.triaged",
            &json!({ "size": "small", "reason": "One file.", "triaged_by": "human" }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn acts_only_on_the_task_in_scope() {
        let harness = Harness::new("orch-scope-task", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(2);
        });
        harness.ready("FRK-1");
        harness.assigned("FRK-2", "dev-b", "dev-a");
        // Accepted with its worktree left, which the cleanup rule, first of all, would remove.
        harness.accepted_with_worktree("FRK-3");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator
            .tick_within(&TickScope {
                task_id: Some("FRK-1".parse().expect("a task id")),
                rules: TickRules::All,
            })
            .await
            .expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Plan);
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Assigned);
        assert!(harness.worktree("FRK-3").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refines_and_nothing_else_under_the_refining_rules() {
        let harness = Harness::new("orch-scope-refining", |_| {});
        harness.ready("FRK-1");
        harness.file("FRK-2", "draft", |_| {});
        triaged_small(&harness, "FRK-2");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let refining = TickScope {
            task_id: None,
            rules: TickRules::Refining,
        };

        let report = orchestrator
            .tick_within(&refining)
            .await
            .expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Refining);
        harness.project.record(
            "FRK-2",
            "question.asked",
            &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
        );
        let report = orchestrator
            .tick_within(&refining)
            .await
            .expect("the tick runs");
        assert!(matches!(report, TickReport::Idle { .. }), "{report:?}");
        assert!(adapter.started().is_empty());
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Ready);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn plans_without_running_criteria_or_merging() {
        let harness = Harness::new("orch-scope-planning", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(3);
            wire["policy"]["integration"] = json!("auto_merge");
        });
        harness.verifying("FRK-1");
        harness.accepted("FRK-2");
        harness.assigned("FRK-3", "dev-b", "dev-a");
        harness.ready("FRK-4");
        // A task in progress, whose implement session is work, and an epic in progress whose one
        // task was cancelled, whose breakdown is planning. One session is all the epic has room
        // for, so that its breakdown, which files nothing here, is not asked for again.
        harness.in_progress("FRK-5", "dev-b", "dev-a");
        harness
            .project
            .filed_with("FRK-6", "ready", "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["budget"]["max_sessions"] = json!(1);
            });
        let pm = json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "pm" });
        harness.project.moved("FRK-6", "ready", "assigned", &pm);
        harness
            .project
            .moved("FRK-6", "assigned", "in_progress", &pm);
        harness.file_under("FRK-7", "ready", Some("FRK-6"), |_| {});
        harness
            .project
            .moved("FRK-7", "ready", "cancelled", &json!({}));
        let adapter = harness.recorded(vec![
            replays_farik_read_board(),
            rewritten(&plan_assigns_frk_1(), "FRK-1", "FRK-4"),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let planning = TickScope {
            task_id: None,
            rules: TickRules::Planning,
        };

        for _ in 0..10 {
            if let TickReport::Idle { .. } = orchestrator
                .tick_within(&planning)
                .await
                .expect("the tick runs")
            {
                break;
            }
        }

        let started: Vec<(String, SessionPurpose)> = adapter
            .started()
            .iter()
            .map(|spec| {
                (
                    spec.task_id
                        .clone()
                        .map(|task| task.to_string())
                        .unwrap_or_default(),
                    spec.purpose,
                )
            })
            .collect();
        assert_eq!(
            started,
            [
                ("FRK-6".to_string(), SessionPurpose::Plan),
                ("FRK-4".to_string(), SessionPurpose::Plan)
            ]
        );
        assert_eq!(harness.row("FRK-4").status, TaskStatus::Assigned);
        assert_eq!(harness.row("FRK-5").status, TaskStatus::InProgress);
        assert!(harness.events(&[EventKind::CriterionRecorded]).is_empty());
        assert!(harness.events(&[EventKind::TaskIntegrated]).is_empty());
        assert_eq!(harness.row("FRK-3").status, TaskStatus::Assigned);
        assert!(!harness.worktree("FRK-3").exists());
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                &["branch", "--list", &harness.branch("FRK-1")]
            )
            .trim(),
            harness.branch("FRK-1")
        );
        assert!(!orchestrator.holds_sandbox(&"FRK-1".parse().expect("a task id")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn cleans_up_a_task_once_it_is_cancelled() {
        let harness = Harness::new("orch-cleanup-cancelled", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.project.moved(
            "FRK-1",
            "in_progress",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator =
            harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(sandboxes.removed("FRK-1"), 1);
        assert!(!harness.worktree("FRK-1").exists());
        assert_eq!(
            git_output_in(
                &harness.project.repo.path,
                &["branch", "--list", &harness.branch("FRK-1")]
            )
            .trim(),
            harness.branch("FRK-1")
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn cleans_up_a_base_worktree_left_on_its_own() {
        let harness = Harness::new("orch-cleanup-base", |_| {});
        harness.accepted("FRK-1");
        let base = harness.worktree("FRK-1-base");
        harness
            .project
            .deps
            .git
            .create_detached_worktree(&base, "main")
            .expect("the base worktree is made");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert!(!base.exists());
        let listed = worktrees_listed(&harness);
        assert!(!listed.contains("FRK-1-base"), "{listed}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn removes_the_tasks_own_worktree_last() {
        let harness = Harness::new("orch-cleanup-order", |_| {});
        harness.accepted_with_worktree("FRK-1");
        // A file where the base worktree would be, which cannot be removed as a directory.
        std::fs::write(harness.worktree("FRK-1-base"), "").expect("written");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let ticked = orchestrator.tick().await;

        assert!(ticked.is_err(), "{ticked:?}");
        // What brings the rule back next time is still there.
        assert!(harness.worktree("FRK-1").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn goes_on_with_the_board_while_a_container_cannot_be_removed() {
        let harness = Harness::new("orch-cleanup-docker-down", |_| {});
        harness.accepted_with_worktree("FRK-1");
        harness.ready("FRK-2");
        let adapter = harness.recorded(vec![reads_a_file()]);
        let sandboxes = Arc::new(UnremovableSandboxFactory::new(1));
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-2"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Plan);
        assert_eq!(sandboxes.counting.removed("FRK-1"), 1);
        // Left for a later tick, which removes it once docker answers.
        assert!(harness.worktree("FRK-1").exists());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(sandboxes.counting.removed("FRK-1"), 2);
        assert!(!harness.worktree("FRK-1").exists());
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
        // A ready standalone task on a team with an active Scrum Master starts the Scrum Master's
        // plan session, not the Product Manager's, and no other session runs.
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

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Plan);
        assert_eq!(started[0].agent_id, "sam");
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
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
            "feature/FRK-1"
        );
        assert_eq!(
            git_output_in(&harness.project.repo.path, &["branch", "--list", "farik/*"]),
            ""
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
    async fn starts_a_document_task_on_a_docs_branch() {
        let harness = Harness::new("orch-start-docs", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(json!({
                    "id": "arch",
                    "display_name": "arch",
                    "role": "architect",
                    "status": "active"
                }));
        });
        harness.file("FRK-1", "ready", |wire| {
            wire["assignee_role"] = json!("architect");
            wire["allowed_paths"] = json!(["docs/adr/**"]);
        });
        harness.project.moved(
            "FRK-1",
            "ready",
            "assigned",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "arch", "reviewer": "dev-b" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(
            git_output_in(
                &harness.worktree("FRK-1"),
                &["rev-parse", "--abbrev-ref", "HEAD"]
            ),
            "docs/FRK-1"
        );
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
            git.commit_count("main", &harness.branch("FRK-1"))
                .expect("git counts"),
            1
        );
        assert_eq!(
            git.changed_paths("main", &harness.branch("FRK-1"))
                .expect("git lists"),
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
        let head = git_output_in(
            &harness.project.repo.path,
            &["rev-parse", &harness.branch("FRK-1")],
        );
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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

        run_until_idle_within_ten_seconds(Arc::new(orchestrator)).expect("the run is idle");

        assert_eq!(adapter.started().len(), 1);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
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
    async fn escalates_a_task_out_of_sessions_before_running_its_criteria() {
        let harness = Harness::new("orch-verify-no-sessions", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Sessions]
        );
        assert!(governor_runs(&harness).is_empty());
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
    async fn posts_a_line_for_a_rejection_farik_filed() {
        let harness = Harness::new("orch-verify-rejects-line", |_| {});
        harness.verifying_with("FRK-1", false, true, |_| {});
        let adapter = harness.recorded(vec![review_writes_note()]);
        let orchestrator = harness.orchestrator(adapter);
        orchestrator.tick().await.expect("the review runs");
        let before = harness.events(&[EventKind::MessagePosted]).len();

        orchestrator.tick().await.expect("the rejection is filed");

        let lines = harness.events(&[EventKind::MessagePosted]);
        assert_eq!(lines.len(), before + 1, "{lines:?}");
        let EventBody::MessagePosted(line) = &lines[before].body else {
            panic!("a message");
        };
        assert_eq!(line.kind, MessageKind::System);
        assert!(
            line.text
                .starts_with("FRK-1 verifying → rejected (by dev-b): C1"),
            "{}",
            line.text
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
            }
        );
        assert_eq!(
            sessions(&adapter),
            vec![("dev-b".to_string(), SessionPurpose::Verify)]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn accepts_a_high_risk_task_after_the_human() {
        let harness = Harness::new("orch-verify-high-risk-accepted", |_| {});
        harness.verifying_with("FRK-1", true, true, |wire| wire["risk"] = json!("high"));
        let adapter = harness.recorded(vec![review_writes_note(), accept_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the review runs");
        orchestrator
            .handle(farik_protocol::command::Command::HumanAccept {
                task_id: "FRK-1".parse().expect("a task id"),
                subject: farik_protocol::command::AcceptSubject::Result,
                message: None,
            })
            .await
            .expect("the human accepts the result");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            sessions(&adapter),
            vec![
                ("dev-b".to_string(), SessionPurpose::Verify),
                ("pm".to_string(), SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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

        run_until_idle_within_ten_seconds(Arc::new(orchestrator)).expect("the run is idle");

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

        run_until_idle_within_ten_seconds(Arc::new(orchestrator)).expect("the run is idle");

        assert_eq!(harness.row("FRK-2").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![
                EscalationRaisedBodyReason::ExplicitRequest,
                EscalationRaisedBodyReason::Sessions
            ]
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

    /// Files FRK-1 `in_progress` with dev-a and dev-b, allowed `max_sessions` sessions, `used` of
    /// them spent.
    fn in_progress_with_sessions(harness: &Harness, max_sessions: u64, used: u64) {
        harness.file("FRK-1", "ready", |wire| {
            wire["budget"]["max_sessions"] = json!(max_sessions);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        harness.project.moved("FRK-1", "ready", "assigned", &people);
        harness
            .project
            .moved("FRK-1", "assigned", "in_progress", &people);
        for session in 0..used {
            harness.spent(Some("FRK-1"), &format!("s-{session}"), 0.01);
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_task_out_of_sessions() {
        let harness = Harness::new("orch-budget-sessions", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        let moved = last_move(&harness);
        assert_eq!(moved.from.to_string(), "in_progress");
        assert_eq!(moved.requested_by, "governor");
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Sessions]
        );
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_task_out_of_dollars() {
        let harness = Harness::new("orch-budget-dollars", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.spent(Some("FRK-1"), "s-0", 3.0);
        harness.spent(Some("FRK-1"), "s-0", 2.0);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(last_move(&harness).requested_by, "governor");
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Budget]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_refining_contract_out_of_sessions() {
        let harness = Harness::new("orch-budget-refining", |_| {});
        harness.file("FRK-1", "refining", |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Sessions]
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_the_escalation_once() {
        let harness = Harness::new("orch-budget-once", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        refused(
            &harness,
            "FRK-1",
            "in_progress",
            "escalated",
            "governor",
            "governor",
        );
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string(),
                until: None,
            }
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        assert_eq!(harness.events(&[EventKind::TransitionRefused]).len(), 1);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_again_a_task_resumed_without_room() {
        let harness = Harness::new("orch-budget-resumed", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick escalates it");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        orchestrator
            .handle(farik_protocol::command::Command::EscalationResolve {
                task_id: "FRK-1".parse().expect("a task id"),
                to: TaskStatus::InProgress,
                message: "Carry on.".to_string(),
            })
            .await
            .expect("the human resolves the escalation");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![
                EscalationRaisedBodyReason::Sessions,
                EscalationRaisedBodyReason::Sessions
            ]
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_the_budget_to_farik_plan() {
        let harness = Harness::new("orch-budget-planning", |_| {});
        harness.file("FRK-1", "refining", |wire| {
            wire["budget"]["max_sessions"] = json!(1);
        });
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator
            .tick_within(&TickScope {
                task_id: None,
                rules: TickRules::Planning,
            })
            .await
            .expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Refining);
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_task_with_room_alone() {
        let harness = Harness::new("orch-budget-room", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.spent(Some("FRK-1"), "s-0", 0.01);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        assert_eq!(adapter.started()[0].purpose, SessionPurpose::Implement);
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
        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("input or output tokens"), "{}", notes[0]);
    }

    /// What a session that ran past its wall clock is told when it ends.
    const WALL_CLOCK: &str = "the session ran past its wall clock of 1800 s";

    /// The progress notes Farik wrote, in order.
    fn farik_notes(harness: &Harness) -> Vec<String> {
        harness
            .events(&[EventKind::NoteWritten])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::NoteWritten(body) if body.written_by == "farik" => {
                    assert_eq!(body.kind, NoteWrittenBodyKind::Progress);
                    Some(body.text.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// A session that reads a file and ends at a limit, told `WALL_CLOCK`.
    fn hits_its_wall_clock() -> Transcript {
        rewritten(
            &hits_the_turn_limit(),
            "Reached maximum number of turns (1)",
            WALL_CLOCK,
        )
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_note_when_a_session_hits_its_wall_clock() {
        let harness = Harness::new("orch-note-wall-clock", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![hits_its_wall_clock(), reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("a limit"), "{}", notes[0]);
        assert!(notes[0].contains(WALL_CLOCK), "{}", notes[0]);
        assert!(!notes[0].contains("Your last note"), "{}", notes[0]);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);

        orchestrator.tick().await.expect("the next session runs");
        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(started[1].purpose, SessionPurpose::Implement);
        assert!(
            started[1].initial_prompt.contains(&notes[0]),
            "{}",
            started[1].initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_note_when_a_session_crosses_its_tokens() {
        let harness = Harness::new("orch-note-tokens", |wire| {
            wire["budgets"]["session"] = json!({ "max_input_tokens": 100 });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::completing(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let ends = harness.events(&[EventKind::SessionEnded]);
        assert!(
            matches!(
                &ends[..],
                [end] if matches!(&end.body, EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Completed)
            ),
            "{ends:?}"
        );
        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("input or output tokens"), "{}", notes[0]);
        assert!(!notes[0].contains("a limit"), "{}", notes[0]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_the_agents_own_note_in_farks_note() {
        let harness = Harness::new("orch-note-quotes", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let limited = rewritten(
            &implement_stops_early(),
            r#""subtype":"success","is_error":false"#,
            &format!(r#""subtype":"error_max_turns","is_error":true,"errors":["{WALL_CLOCK}"]"#),
        );
        let adapter = harness.recorded(vec![limited]);
        let orchestrator = harness.orchestrator(adapter);

        orchestrator.tick().await.expect("the tick runs");

        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains(WALL_CLOCK), "{}", notes[0]);
        assert!(
            notes[0].contains("Your last note in it: done.txt committed; C1 not run yet."),
            "{}",
            notes[0]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_no_note_for_a_verify_session() {
        let harness = Harness::new("orch-note-verify", |_| {});
        harness.verifying("FRK-1");
        let adapter = harness.recorded(vec![hits_its_wall_clock()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started()[0].purpose, SessionPurpose::Verify);
        let ends = harness.events(&[EventKind::SessionEnded]);
        assert!(
            ends.iter().any(|end| matches!(
                &end.body,
                EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Limit
            )),
            "{ends:?}"
        );
        assert!(farik_notes(&harness).is_empty());
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
        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Sessions]
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

    /// A `.farik/prices.json` that prices a model no agent of the harness runs, so that every one
    /// of their sessions is recorded unpriced.
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
        // A model no table prices is recorded, not refused (ADR 0015), so the cost that fails is
        // one the log cannot hold: more tokens than its integer counts.
        let harness = Harness::new("orch-session-uncosted", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage {
            input_tokens: u64::MAX,
            ..a_thousand_tokens()
        }));
        let orchestrator = harness.orchestrator(adapter.clone());

        // A session left running waits for ever.
        let ticked = tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the tick ends");

        assert!(
            matches!(
                ticked,
                Err(OrchestratorError::Cost(CostError::Event { .. }))
            ),
            "{ticked:?}"
        );
        assert_eq!(adapter.aborts(), 1);
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_that_cannot_start_on_a_model_no_table_prices() {
        let harness = Harness::new("orch-session-no-start-unpriced", |_| {});
        prices_without_the_teams_models(&harness);
        harness.ready("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let ticked = orchestrator.tick().await;

        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
        let costs = harness.events(&[EventKind::CostRecorded]);
        assert!(
            matches!(&costs[..], [cost] if matches!(&cost.body, EventBody::CostRecorded(body) if body.unpriced)),
            "{costs:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_that_cannot_start_when_its_zero_cost_is_refused() {
        let harness = Harness::new("orch-session-no-start-no-cost", |_| {});
        harness.ready("FRK-1");
        refuse_appends_of(&harness.project.deps.log, EventKind::CostRecorded);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let ticked = orchestrator.tick().await;

        // The start failed first, and that is what the tick says; the refused cost hides neither
        // it nor the session's end.
        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        assert!(harness.events(&[EventKind::CostRecorded]).is_empty());
        assert_eq!(end_reasons(&harness), vec![SessionEndedBodyReason::Error]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_session_that_reported_no_usage_when_its_zero_cost_is_refused() {
        let harness = Harness::new("orch-session-no-usage-no-cost", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        refuse_appends_of(&harness.project.deps.log, EventKind::CostRecorded);
        let init = implement_stops_early()
            .lines()
            .next()
            .expect("a transcript starts with its init")
            .to_string();
        let orchestrator =
            harness.orchestrator(harness.recorded(vec![Transcript::from_jsonl(&init)]));

        let ticked = orchestrator.tick().await;

        assert!(
            matches!(
                ticked,
                Err(OrchestratorError::Cost(CostError::Store { ref detail }))
                    if detail.contains("this log refuses cost.recorded")
            ),
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
                why: "the team's daily budget is spent".to_string(),
                until: None,
            }
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_sessions_whatever_the_day_cost_without_a_daily_budget() {
        let harness = Harness::new("orch-budget-no-day", |wire| wire["budgets"] = json!({}));
        harness.spent(None, "s-0", 20.0);
        harness.ready("FRK-1");
        harness.assigned("FRK-2", "dev-a", "dev-b");
        // Rule 6 comes before rule 8, so FRK-2's implement session is the first one started.
        let adapter = harness.recorded(vec![implement_stops_early(), plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let first = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&first), Some("FRK-2"), "{first:?}");
        let second = orchestrator.tick().await.expect("the tick runs");
        assert_ne!(
            second,
            TickReport::Idle {
                why: "the team's daily budget is spent".to_string(),
                until: None,
            }
        );
        let started = adapter.started();
        assert_eq!(
            started[0].task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-2")
        );
        assert_eq!(started[0].purpose, SessionPurpose::Implement);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_a_ready_task_whatever_the_day_cost_without_a_daily_budget() {
        // No daily budget leaves an unbounded day, however much of it went, and the assignment
        // gate reads that as room for any task's budget.
        let harness = Harness::new("orch-budget-no-day-assign", |wire| {
            wire["budgets"] = json!({});
        });
        harness.spent(None, "s-0", 25.0);
        harness.ready("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(vec![plan_assigns_frk_1()]));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Assigned);
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
            .create_worktree(&harness.worktree("FRK-1"), &harness.branch("FRK-1"), "main")
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
                why: NOTHING_TO_DO.to_string(),
                until: None,
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
    async fn runs_a_session_on_a_model_no_table_prices() {
        let harness = Harness::new("orch-session-unpriced", |wire| {
            wire["agents"][1]["model"] = json!({ "id": "claude-unknown-9" });
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the session runs");

        let events = harness.events(&[
            EventKind::SessionStarted,
            EventKind::CostRecorded,
            EventKind::SessionEnded,
        ]);
        let kinds: Vec<EventKind> = events.iter().map(|event| event.body.kind()).collect();
        assert_eq!(
            kinds,
            [
                EventKind::SessionStarted,
                EventKind::CostRecorded,
                EventKind::SessionEnded
            ]
        );
        assert!(matches!(
            &events[1].body,
            EventBody::CostRecorded(body) if body.unpriced && body.cost_usd.abs() < f64::EPSILON
        ));
        let costs = harness
            .project
            .deps
            .projections
            .costs(farik_store::CostScope::Task)
            .expect("the costs read");
        let frk_1 = costs
            .iter()
            .find(|row| row.key == "FRK-1")
            .expect("FRK-1's cost");
        assert_eq!(frk_1.sessions, 1);
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
    async fn offers_a_verify_session_no_tool_that_runs_or_writes() {
        const WITHHELD: [&str; 3] = ["farik_exec", "farik_git_commit", "farik_git_push"];
        let harness = Harness::new("orch-verify-tools", |_| {});
        harness.verifying("FRK-1");
        let recorded = harness.recorded(vec![review_writes_note(), accept_frk_1()]);
        let witness = Arc::new(ExecutorWitness::new(
            recorded.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());

        orchestrator
            .run_until_idle()
            .await
            .expect("the run ends idle");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
        let started = recorded.started();
        let agents: Vec<&str> = started.iter().map(|spec| spec.agent_id.as_str()).collect();
        assert_eq!(agents, ["dev-b", "pm"]);
        let listed = witness.listed_tools();
        let given = witness.given_tools();
        for (index, spec) in started.iter().enumerate() {
            assert_eq!(spec.purpose, SessionPurpose::Verify);
            assert_eq!(
                given[index], spec.farik_tools,
                "the daemon was given the spec's"
            );
            for name in WITHHELD {
                assert!(
                    !spec.farik_tools.iter().any(|tool| tool == name),
                    "{}'s spec offers {name}: {:?}",
                    spec.agent_id,
                    spec.farik_tools
                );
                assert!(
                    !listed[index].iter().any(|tool| tool == name),
                    "{}'s tools/list shows {name}: {:?}",
                    spec.agent_id,
                    listed[index]
                );
                assert!(
                    !spec.system_prompt.contains(&format!("- {name} (")),
                    "{}'s prompt lists {name}",
                    spec.agent_id
                );
            }
            assert!(
                !spec.system_prompt.contains("The shell is `farik_exec`"),
                "{}'s prompt names a shell it does not have",
                spec.agent_id
            );
        }
        // The reviewer, a developer holding `git_local`, still reads the work through git.
        for name in ["farik_git_status", "farik_git_diff"] {
            assert!(
                started[0].farik_tools.iter().any(|tool| tool == name),
                "{:?}",
                started[0].farik_tools
            );
            assert!(listed[0].iter().any(|tool| tool == name), "{:?}", listed[0]);
        }
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
    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_finished_sprint_by_itself() {
        let harness = Harness::new("orch-sprint-finished", |_| {});
        harness.accepted("FRK-1");
        harness.ready("FRK-2");
        harness.project.moved(
            "FRK-2",
            "ready",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        harness.open_sprint("S1", &["FRK-1", "FRK-2"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Sprint { sprint_id, .. } if sprint_id == "S1"),
            "{report:?}"
        );
        let ended = harness.events(&[EventKind::SprintEnded]);
        assert_eq!(ended.len(), 1);
        let EventBody::SprintEnded(body) = &ended[0].body else {
            panic!("an end");
        };
        assert_eq!(body.ended_by, SprintEndedBodyEndedBy::Governor);
        assert!(body.left.is_empty());
        assert_eq!(
            harness
                .project
                .deps
                .files
                .read_sprint("S1")
                .expect("S1 reads")
                .status,
            SprintStatus::Ended
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_empty_sprint_open() {
        let harness = Harness::new("orch-sprint-empty", |_| {});
        harness.open_sprint("S1", &[]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(matches!(report, TickReport::Idle { .. }), "{report:?}");
        assert!(harness.events(&[EventKind::SprintEnded]).is_empty());
    }

    /// The harness's team with an active Scrum Master `sm` besides.
    fn with_a_scrum_master(wire: &mut serde_json::Value) {
        wire["agents"]
            .as_array_mut()
            .expect("a list of agents")
            .push(farik_core::team::fixtures::an_agent_wire(
                "sm",
                "scrum_master",
            ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_the_assigner_to_plan_an_empty_sprint() {
        let harness = Harness::new("orch-sprint-plan", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let adapter = harness.recorded(vec![plan_sprint_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Sprint { sprint_id, what }
                if sprint_id == "S1" && what.contains("S1 holds FRK-1")),
            "{report:?}"
        );
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(
            (spec.agent_id.as_str(), spec.purpose, spec.task_id.as_ref()),
            ("sm", SessionPurpose::Plan, None)
        );
        assert_eq!(spec.farik_tools, vec!["farik_plan_sprint".to_string()]);
        assert!(
            spec.initial_prompt.contains("FRK-1") && spec.initial_prompt.contains("$5.00"),
            "{}",
            spec.initial_prompt
        );
        assert_eq!(harness.row("FRK-1").sprint.as_deref(), Some("S1"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_no_sprint_rule_in_a_scoped_or_refining_tick() {
        let harness = Harness::new("orch-sprint-scoped", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let adapter = harness.recorded(vec![plan_sprint_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        for scope in [
            TickScope {
                task_id: Some("FRK-1".parse().expect("a task id")),
                ..TickScope::default()
            },
            TickScope {
                rules: TickRules::Refining,
                ..TickScope::default()
            },
        ] {
            let report = orchestrator
                .tick_within(&scope)
                .await
                .expect("the tick runs");
            assert!(!matches!(&report, TickReport::Sprint { .. }), "{report:?}");
        }

        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        assert_eq!(harness.row("FRK-1").sprint, None);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_no_plan_on_a_spent_day() {
        let harness = Harness::new("orch-sprint-day", with_a_scrum_master);
        harness.spent(None, "s-0", 20.0);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let adapter = harness.recorded(vec![plan_sprint_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: "the team's daily budget is spent".to_string(),
                until: None,
            }
        );
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_no_plan_of_a_sprint_that_holds_a_task() {
        let harness = Harness::new("orch-sprint-held", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.ready("FRK-2");
        harness.open_sprint("S1", &["FRK-1"]);
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(
            adapter.started().iter().all(|spec| spec.task_id.is_some()),
            "{:?}",
            adapter.started()
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn plans_a_sprint_once() {
        let harness = Harness::new("orch-sprint-once", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let adapter = harness.recorded(vec![reads_a_file(), plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let first = orchestrator.tick().await.expect("the tick runs");
        assert!(matches!(&first, TickReport::Sprint { .. }), "{first:?}");
        assert_eq!(
            harness.row("FRK-1").sprint,
            None,
            "the session planned nothing"
        );

        let second = orchestrator.tick().await.expect("the tick runs");

        assert!(!matches!(&second, TickReport::Sprint { .. }), "{second:?}");
        let planning = adapter
            .started()
            .iter()
            .filter(|spec| spec.task_id.is_none())
            .count();
        assert_eq!(planning, 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn plans_a_sprint_at_most_three_times() {
        let harness = Harness::new("orch-sprint-thrice", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let adapter = harness.recorded(vec![
            hits_its_wall_clock(),
            hits_its_wall_clock(),
            hits_its_wall_clock(),
            hits_its_wall_clock(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        for _ in 0..4 {
            orchestrator.tick().await.expect("the tick runs");
        }

        let planning = adapter
            .started()
            .iter()
            .filter(|spec| spec.task_id.is_none())
            .count();
        assert_eq!(planning, 3);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_only_the_open_sprints_tasks() {
        let harness = Harness::new("orch-sprint-assign", |_| {});
        harness.ready("FRK-1");
        harness.ready("FRK-2");
        harness.open_sprint("S1", &["FRK-2"]);
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            (
                started[0].purpose,
                started[0].task_id.as_ref().map(|task| task.as_str())
            ),
            (SessionPurpose::Plan, Some("FRK-2"))
        );
        assert!(
            started[0].initial_prompt.contains("FRK-2")
                && !started[0].initial_prompt.contains("FRK-1"),
            "{}",
            started[0].initial_prompt
        );
        assert!(harness.events(&[EventKind::TransitionRefused]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn assigns_the_backlog_without_a_sprint() {
        let harness = Harness::new("orch-sprint-none", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            (
                started[0].purpose,
                started[0].task_id.as_ref().map(|task| task.as_str())
            ),
            (SessionPurpose::Plan, Some("FRK-1"))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_assigning_once_the_sprint_budget_is_spent() {
        let harness = Harness::new("orch-sprint-spent", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.ready("FRK-2");
        harness.accepted("FRK-3");
        harness
            .project
            .open_sprint("S1", Some(5.0), &["FRK-1", "FRK-2", "FRK-3"]);
        // Spent in S1 by a task already accepted, so that neither FRK-1's nor FRK-2's own budget
        // is what stops anything.
        harness.spent(Some("FRK-3"), "s-0", 5.0);
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let scoped = orchestrator
            .tick_within(&TickScope {
                task_id: Some("FRK-2".parse().expect("a task id")),
                ..TickScope::default()
            })
            .await
            .expect("the tick runs");
        assert!(matches!(scoped, TickReport::Idle { .. }), "{scoped:?}");
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Ready);
        assert!(harness.events(&[EventKind::TransitionRefused]).is_empty());

        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(started[0].purpose, SessionPurpose::Implement);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn lets_a_session_finish_past_the_sprint_budget() {
        let harness = Harness::new("orch-sprint-finish", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.project.open_sprint("S1", Some(0.001), &["FRK-1"]);
        let adapter = Arc::new(UsageThenWaitAdapter::completing(a_thousand_tokens()));
        let orchestrator = harness.orchestrator(adapter.clone());

        tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the session ends")
            .expect("the tick runs");

        assert_eq!(adapter.aborts(), 0);
        assert_eq!(
            scopes_exhausted(&harness),
            vec![BudgetExhaustedBodyScope::SprintUsd]
        );
        let ended = harness.events(&[EventKind::SessionEnded]);
        assert!(matches!(
            &ended.last().expect("an end").body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Completed
        ));
    }

    /// A session its model provider refused at a limit it said resets at `until`.
    fn refused_until(until: chrono::DateTime<chrono::Utc>) -> Transcript {
        rewritten(
            &provider_limit_rejected(),
            "1790119200",
            &until.timestamp().to_string(),
        )
    }

    /// Each `agent.slept`: the agent on its envelope and its `until`, in order.
    fn sleeps(harness: &Harness) -> Vec<(Option<String>, chrono::DateTime<chrono::Utc>)> {
        harness
            .events(&[EventKind::AgentSlept])
            .iter()
            .map(|event| match &event.body {
                EventBody::AgentSlept(body) => (event.envelope.ids.agent_id.clone(), body.until),
                other => panic!("not a sleep: {other:?}"),
            })
            .collect()
    }

    /// The team file as it is on disk.
    fn team_file(harness: &Harness) -> String {
        std::fs::read_to_string(harness.project.repo.path.join(".farik/team.yaml"))
            .expect("the team file reads")
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn puts_an_agent_to_sleep_at_its_providers_limit() {
        let harness = Harness::new("orch-sleep", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let team = team_file(&harness);
        let until = at() + chrono::Duration::hours(2);
        let orchestrator = harness.orchestrator(harness.recorded(vec![refused_until(until)]));

        orchestrator.tick().await.expect("the tick runs");

        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes[0].contains("its model provider's usage limit"),
            "{}",
            notes[0]
        );
        assert_eq!(sleeps(&harness), vec![(Some("dev-a".to_string()), until)]);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        assert_eq!(team_file(&harness), team);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sleeps_an_hour_without_a_reset_time() {
        let harness = Harness::new("orch-sleep-hour", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let orchestrator = harness.orchestrator(harness.recorded(vec![provider_limit_429()]));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            sleeps(&harness),
            vec![(Some("dev-a".to_string()), at() + chrono::Duration::hours(1))]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ignores_a_reset_time_in_the_past() {
        let harness = Harness::new("orch-sleep-past", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let refused = refused_until(at() - chrono::Duration::hours(1));
        let orchestrator = harness.orchestrator(harness.recorded(vec![refused]));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            sleeps(&harness),
            vec![(Some("dev-a".to_string()), at() + chrono::Duration::hours(1))]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn makes_no_sandbox_for_a_sleeping_agent() {
        let harness = Harness::new("orch-sleep-sandbox", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.asleep("dev-a", at() + chrono::Duration::hours(1));
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(sandboxes.created("FRK-1"), 0);
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn plans_a_sprint_again_after_its_planner_slept() {
        let harness = Harness::new("orch-sleep-sprint", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let until = at() + chrono::Duration::hours(2);
        let adapter = harness.recorded(vec![refused_until(until), plan_sprint_frk_1()]);
        let first = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(matches!(&first, TickReport::Sprint { .. }), "{first:?}");

        let report = harness
            .orchestrator_at(adapter.clone(), until + chrono::Duration::minutes(1))
            .tick()
            .await
            .expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Sprint { sprint_id, what }
                if sprint_id == "S1" && what.contains("S1 holds FRK-1")),
            "{report:?}"
        );
        let planning = adapter
            .started()
            .iter()
            .filter(|spec| spec.task_id.is_none() && spec.agent_id == "sm")
            .count();
        assert_eq!(planning, 2);
        assert_eq!(harness.row("FRK-1").sprint.as_deref(), Some("S1"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_no_session_for_a_sleeping_agent() {
        let harness = Harness::new("orch-sleep-other", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.in_progress("FRK-2", "dev-b", "dev-a");
        harness.asleep("dev-a", at() + chrono::Duration::hours(1));
        let adapter = harness.recorded(vec![reads_a_file(), reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let first = orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&first), Some("FRK-2"), "{first:?}");
        let agents: Vec<String> = adapter
            .started()
            .iter()
            .map(|spec| spec.agent_id.clone())
            .collect();
        assert_eq!(agents, vec!["dev-b".to_string(), "dev-b".to_string()]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn idles_until_the_first_agent_wakes() {
        let harness = Harness::new("orch-sleep-idle", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-a", until);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Idle { why, until: Some(woken) }
                if *woken == until && why.starts_with("waiting for dev-a, asleep until ")),
            "{report:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn idles_without_a_time_when_nobody_sleeps() {
        let harness = Harness::new("orch-sleep-nobody", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string(),
                until: None,
            }
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn wakes_an_agent_when_its_sleep_ends() {
        let harness = Harness::new("orch-sleep-wake", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let until = at() + chrono::Duration::hours(2);
        let adapter = harness.recorded(vec![refused_until(until), reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 1, "no session while dev-a sleeps");

        harness
            .orchestrator_at(adapter.clone(), until + chrono::Duration::minutes(1))
            .tick()
            .await
            .expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(
            (started[1].agent_id.as_str(), started[1].purpose),
            ("dev-a", SessionPurpose::Implement)
        );
        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes[0].contains("its model provider's usage limit"),
            "{}",
            notes[0]
        );
        assert!(
            started[1].initial_prompt.contains(&notes[0]),
            "{}",
            started[1].initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn still_assigns_work_to_a_sleeping_agent() {
        let harness = Harness::new("orch-sleep-assign", with_a_scrum_master);
        harness.blocked("FRK-2", "dev-b", "dev-a");
        harness.ready("FRK-3");
        harness.asleep("dev-a", at() + chrono::Duration::hours(1));
        let adapter = harness.recorded(vec![reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-3"), "{report:?}");
        let started = adapter.started();
        assert_eq!(
            (started[0].agent_id.as_str(), started[0].purpose),
            ("sm", SessionPurpose::Plan)
        );
        assert!(
            started[0]
                .initial_prompt
                .contains("with room for it: dev-a."),
            "{}",
            started[0].initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_reviewer() {
        let harness = Harness::new("orch-sleep-reviewer", |_| {});
        harness.verifying("FRK-1");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-b", until);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "dev-b", until).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_product_manager_to_accept() {
        let harness = Harness::new("orch-sleep-accept", |_| {});
        harness.verifying("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(vec![review_writes_note()]));
        orchestrator.tick().await.expect("the review runs");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("pm", until);

        waits_for(&orchestrator, "pm", until).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_assigner() {
        let harness = Harness::new("orch-sleep-assigner", |_| {});
        harness.ready("FRK-1");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("pm", until);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "pm", until).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_sprint_planner() {
        let harness = Harness::new("orch-sleep-planner", with_a_scrum_master);
        harness.ready("FRK-1");
        harness.open_sprint("S1", &[]);
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("sm", until);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "sm", until).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_the_agent_that_wakes_first() {
        let harness = Harness::new("orch-sleep-earliest", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.in_progress("FRK-2", "dev-b", "dev-a");
        harness.asleep("dev-a", at() + chrono::Duration::hours(2));
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-b", until);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        waits_for(&orchestrator, "dev-b", until).await;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_spent_task_waiting_on_the_human_alone() {
        let harness = Harness::new("orch-budget-question", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        harness.project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Should done.txt be empty?", "asked_by": "dev-a" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::InProgress);
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        assert!(harness.events(&[EventKind::TransitionRefused]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_spent_task_already_escalated_alone() {
        let harness = Harness::new("orch-budget-escalated", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick escalates it");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.events(&[EventKind::TaskTransitioned]).len(), 3);
        assert_eq!(
            escalation_reasons(&harness),
            vec![EscalationRaisedBodyReason::Sessions]
        );
        assert!(harness.events(&[EventKind::TransitionRefused]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn quotes_no_note_from_an_earlier_session() {
        let harness = Harness::new("orch-note-earlier", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.project.record(
            "FRK-1",
            "note.written",
            &json!({ "kind": "progress", "text": "An earlier session's note.", "written_by": "dev-a" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(vec![hits_its_wall_clock()]));

        orchestrator.tick().await.expect("the tick runs");

        let notes = farik_notes(&harness);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(!notes[0].contains("Your last note"), "{}", notes[0]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn puts_an_agent_to_sleep_when_its_note_cannot_be_written() {
        let harness = Harness::new("orch-sleep-no-note", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        refuse_appends_of(&harness.project.deps.log, EventKind::NoteWritten);
        let orchestrator = harness.orchestrator(harness.recorded(vec![provider_limit_429()]));

        let ticked = orchestrator.tick().await;

        assert!(ticked.is_err(), "{ticked:?}");
        assert_eq!(
            sleeps(&harness),
            vec![(Some("dev-a".to_string()), at() + chrono::Duration::hours(1))]
        );
    }

    /// Posts `text` in the channel as the human, its mentions parsed, and answers its seq.
    fn said(harness: &Harness, author: &str, kind: MessageKind, text: &str) -> u64 {
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        crate::channel::post(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::channel::NewMessage {
                author: author.to_string(),
                agent_id: (author != "human").then(|| author.to_string()),
                kind,
                text: text.to_string(),
                mentions: crate::channel::mentions_in(text, &team, author),
                task_id: None,
                thread: None,
                in_reply_to: None,
                session_id: None,
            },
        )
        .expect("posted")
    }

    /// Every message in the channel, oldest first.
    fn messages(harness: &Harness) -> Vec<farik_protocol::event::MessagePostedBody> {
        harness
            .events(&[EventKind::MessagePosted])
            .into_iter()
            .filter_map(|event| match event.body {
                EventBody::MessagePosted(body) => Some(body),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_mention_in_a_conversation() {
        let harness = Harness::new("orch-mention", |_| {});
        let seq = said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Conversation { agent_id, .. } if agent_id == "dev-a"),
            "{report:?}"
        );
        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        let spec = &started[0];
        assert_eq!(spec.agent_id, "dev-a");
        assert_eq!(spec.purpose, SessionPurpose::Conversation);
        assert_eq!(spec.model, "claude-sonnet-5");
        assert_eq!(spec.effort, Effort::Low);
        assert_eq!(spec.task_id, None);
        assert_eq!(
            spec.farik_tools,
            [
                "farik_read_task",
                "farik_read_board",
                "farik_read_rules",
                "farik_read_criteria",
                "farik_create_task",
                "farik_post_message",
            ]
        );
        assert_eq!(
            spec.builtin_tools,
            allowed_builtins(&BTreeSet::from([PermissionTier::Read]))
        );
        let mentions = block(&spec.initial_prompt, "mentions");
        assert!(
            mentions.contains("@dev-a status?"),
            "{}",
            spec.initial_prompt
        );
        assert!(
            block(&spec.initial_prompt, "channel").contains("human: @dev-a status?"),
            "{}",
            spec.initial_prompt
        );
        let said = messages(&harness);
        assert_eq!(said.len(), 2, "{said:?}");
        assert_eq!(said[1].author, "dev-a");
        assert_eq!(said[1].kind, MessageKind::Reply);
        assert_eq!(said[1].in_reply_to.map(NonZeroU64::get), Some(seq));
        // Its start records what it was shown, which answers the mentions up to it.
        let starts = harness.events(&[EventKind::SessionStarted]);
        let EventBody::SessionStarted(start) = &starts[0].body else {
            panic!("a session's start");
        };
        assert_eq!(start.in_reply_to.map(NonZeroU64::get), Some(seq));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_each_mention_once() {
        let harness = Harness::new("orch-mention-once", |_| {});
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention(), reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        let second = orchestrator.tick().await.expect("the tick runs");

        assert!(matches!(&second, TickReport::Idle { .. }), "{second:?}");
        assert_eq!(adapter.started().len(), 1, "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_no_mention_on_a_spent_day() {
        let harness = Harness::new("orch-mention-day", |_| {});
        harness.spent(None, "s-0", 20.0);
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: "the team's daily budget is spent".to_string(),
                until: None,
            }
        );
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn does_not_answer_a_reply() {
        let harness = Harness::new("orch-mention-reply", |_| {});
        said(
            &harness,
            "dev-a",
            MessageKind::Reply,
            "@dev-b can you look?",
        );
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(matches!(&report, TickReport::Idle { .. }), "{report:?}");
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_agent_to_answer() {
        let harness = Harness::new("orch-mention-asleep", |_| {});
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-a", until);
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);

        let asleep = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            matches!(&asleep, TickReport::Idle { until: Some(woken), .. } if *woken == until),
            "{asleep:?}"
        );
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());

        let awake = harness
            .orchestrator_at(adapter.clone(), until + chrono::Duration::minutes(1))
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            matches!(&awake, TickReport::Conversation { agent_id, .. } if agent_id == "dev-a"),
            "{awake:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_paused_agent_to_answer() {
        for status in ["paused", "retired"] {
            let harness = Harness::new(&format!("orch-mention-{status}"), |wire| {
                wire["agents"][1]["status"] = json!(status);
            });
            said(&harness, "human", MessageKind::Human, "@dev-a status?");
            let adapter = harness.recorded(vec![reply_to_a_mention()]);
            let orchestrator = harness.orchestrator(adapter.clone());

            let away = orchestrator.tick().await.expect("the tick runs");
            assert!(
                matches!(&away, TickReport::Idle { .. }),
                "{status}: {away:?}"
            );
            assert!(
                adapter.started().is_empty(),
                "{status}: {:?}",
                adapter.started()
            );

            harness
                .project
                .deps
                .files
                .write_team(&crate::tools::fixtures::a_team_of_three(|wire| {
                    wire["policy"]["wip_limit_per_agent"] = json!(1);
                }))
                .expect("the team is written");
            let back = orchestrator.tick().await.expect("the tick runs");
            assert!(
                matches!(&back, TickReport::Conversation { agent_id, .. } if agent_id == "dev-a"),
                "{status}: {back:?}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_no_mention_outside_a_full_tick() {
        let harness = Harness::new("orch-mention-scope", |_| {});
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        for scope in [
            TickScope {
                rules: TickRules::Planning,
                ..TickScope::default()
            },
            TickScope {
                rules: TickRules::Refining,
                ..TickScope::default()
            },
            TickScope {
                task_id: Some("FRK-1".parse().expect("a task id")),
                ..TickScope::default()
            },
        ] {
            let report = orchestrator
                .tick_within(&scope)
                .await
                .expect("the tick runs");
            assert!(
                !matches!(&report, TickReport::Conversation { .. }),
                "{scope:?}: {report:?}"
            );
        }
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());

        let full = orchestrator.tick().await.expect("the tick runs");
        assert!(matches!(&full, TickReport::Conversation { .. }), "{full:?}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_mention_after_the_budget_rule_and_before_rule_3() {
        // A task out of sessions is escalated before the mention is answered.
        let harness = Harness::new("orch-mention-after-budget", |_| {});
        in_progress_with_sessions(&harness, 1, 1);
        said(&harness, "human", MessageKind::Human, "@dev-b status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);

        let report = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());

        // The mention is answered before a rejected task goes back to its assignee.
        let harness = Harness::new("orch-mention-before-rejected", |_| {});
        harness.rejected("FRK-1", 0, "done.txt is missing");
        said(&harness, "human", MessageKind::Human, "@dev-b status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);

        let report = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Conversation { agent_id, .. } if agent_id == "dev-b"),
            "{report:?}"
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Rejected);
    }

    /// Session ids `session-1` and so on, the human posting `@dev-a and the tests?` each time
    /// one is asked for: after the channel rule has read the pending mentions, before the
    /// session's start is recorded.
    struct PostsWhenAsked {
        tools: Arc<crate::tools::ToolDeps>,
        ids: farik_protocol::clock::SequentialIds,
    }

    impl farik_protocol::clock::IdSource for PostsWhenAsked {
        fn session_id(&self) -> String {
            crate::channel::post(
                &self.tools.log,
                self.tools.clock.as_ref(),
                &self.tools.ids,
                crate::channel::NewMessage {
                    author: "human".to_string(),
                    agent_id: None,
                    kind: MessageKind::Human,
                    text: "@dev-a and the tests?".to_string(),
                    mentions: vec!["dev-a".to_string()],
                    task_id: None,
                    thread: None,
                    in_reply_to: None,
                    session_id: None,
                },
            )
            .expect("posted");
            self.ids.session_id()
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_a_mention_posted_while_its_conversation_starts_pending() {
        let harness = Harness::new("orch-mention-race", |_| {});
        let shown = said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator_with_ids(
            adapter.clone(),
            Arc::new(crate::sandbox::host::HostSandboxFactory),
            harness.gh.forge(&harness.project.repo.path),
            Arc::new(PostsWhenAsked {
                tools: Arc::clone(&harness.project.deps),
                ids: farik_protocol::clock::SequentialIds::new(),
            }),
        );

        orchestrator.tick().await.expect("the tick runs");

        let starts = harness.events(&[EventKind::SessionStarted]);
        let EventBody::SessionStarted(start) = &starts[0].body else {
            panic!("a session's start");
        };
        assert_eq!(start.in_reply_to.map(NonZeroU64::get), Some(shown));
        let pending: Vec<String> =
            crate::channel::pending_mentions(&harness.project.deps.log, "dev-a")
                .expect("the log reads")
                .into_iter()
                .filter_map(|event| match event.body {
                    EventBody::MessagePosted(body) => Some(body.text),
                    _ => None,
                })
                .collect();
        assert_eq!(pending, ["@dev-a and the tests?"]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_mention_again_after_its_conversation_hits_the_provider_limit() {
        let harness = Harness::new("orch-mention-limit", |_| {});
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![provider_limit_429(), reply_to_a_mention()]);

        let limited = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            matches!(&limited, TickReport::Conversation { agent_id, .. } if agent_id == "dev-a"),
            "{limited:?}"
        );
        let until = at() + chrono::Duration::hours(1);
        assert_eq!(sleeps(&harness), vec![(Some("dev-a".to_string()), until)]);

        let awake = harness
            .orchestrator_at(adapter.clone(), until + chrono::Duration::minutes(1))
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            matches!(&awake, TickReport::Conversation { agent_id, .. } if agent_id == "dev-a"),
            "{awake:?}"
        );
        assert_eq!(adapter.started().len(), 2, "{:?}", adapter.started());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn writes_the_summary_it_shows() {
        let harness = Harness::new("orch-mention-summary", |_| {});
        for number in 0..10 {
            said(
                &harness,
                "human",
                MessageKind::Human,
                &format!("{number} {}", "word ".repeat(300)),
            );
        }
        said(&harness, "human", MessageKind::Human, "@dev-a status?");
        let adapter = harness.recorded(vec![reply_to_a_mention()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let written = std::fs::read_to_string(
            harness
                .project
                .repo
                .path
                .join(".farik/local/channel-summary.md"),
        )
        .expect("the summary is written");
        let started = adapter.started();
        let shown = block(&started[0].initial_prompt, "channel").trim_matches('\n');
        assert_eq!(written, shown);
        assert!(written.chars().count() <= 8_000, "{}", written.len());
        assert!(written.ends_with("human: @dev-a status?"), "{written}");
        assert!(!written.contains("human: 0 "), "the oldest is left out");
    }
}
