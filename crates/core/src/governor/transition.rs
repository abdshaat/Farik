//! Transition evaluation (`docs/SPEC.md` section 5.2, F5): the one question the runtime asks
//! before it moves a task. It composes the table of 5.2, the actor rules of 5.1, the gate
//! predicates, the Definition of Ready, the Definition of Done, and the counting rules of 5.5 and
//! 5.7 into one answer: the row the move takes and what the runtime must record with it, or every
//! reason the move is refused.
//!
//! Nothing here reads the world. The runtime gathers the values, this decides, and the runtime
//! applies what it is told.

use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::budget::{BudgetConsequence, BudgetScope, BudgetState, check_budgets};
use crate::contract::{TaskContract, TaskId, TaskStatus};
use crate::generated::task_contract::FarikTaskContractKind as Kind;
use crate::generated::task_contract::FarikTaskContractRisk as Risk;
use crate::governor::done::{
    CriterionResult, DoneEvidence, evaluate_done, requires_human_acceptance,
};
use crate::governor::escalation::{
    BlockedAge, EscalationReason, READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome,
    evaluate_blocked_age, evaluate_readiness_attempts, evaluate_rejection,
};
use crate::governor::gates::{
    AssignmentInput, AssignmentRequester, Blocker, ChildState, GateResult, Rejection, WorkState,
    check_assignment, check_blocker_resolved, check_blocker_written, check_children_done,
    check_criteria_recorded, check_rejection_reasons,
};
use crate::governor::readiness::{ReadinessContext, evaluate_readiness};
use crate::governor::transition_table::{GateId, TransitionActor, TransitionRow, find_transitions};

/// A request to move one task to one status, by one actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRequest {
    /// The task to move.
    pub task_id: TaskId,
    /// The status it would enter.
    pub to: TaskStatus,
    /// Which of the lifecycle's actors is asking.
    pub actor: TransitionActor,
    /// The asking agent's id, when an agent rather than the human or the governor. A row that
    /// names the assignee or the reviewer opens only to the agent the contract names.
    pub agent_id: Option<String>,
}

/// Whether the human must accept the contract the task has now, and whether they have. The two
/// travel together because one question is asked of them: is this contract waiting for the human?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContractAcceptance {
    /// Whether the team's policy requires the human to accept this contract, over and above the
    /// risk and the kind that `done::requires_human_acceptance` answers
    /// (`human_accepts_contracts`, a team policy rather than one of 5.12's rules: `docs/SPEC.md`
    /// section 5.16 and its resolved question 1, with the values `high_risk` and `all`).
    pub required_by_policy: bool,
    /// Whether the human has accepted the contract the task has now. An edit of a frozen contract
    /// sends the task back to `refining` (5.11), and the acceptance it had does not carry over.
    pub given: bool,
}

/// Everything a gate of the table can ask, gathered by the runtime before it asks. Not `PartialEq`:
/// the generated `TaskContract` is not, and a context is a bundle of inputs rather than a value to
/// compare.
#[derive(Debug, Clone)]
pub struct TransitionContext {
    /// Its contract as it stands.
    pub contract: TaskContract,
    /// Whether the request behind this task has a recorded triage decision (`docs/SPEC.md`
    /// section 5.16).
    pub triaged: bool,
    /// The tasks under this epic, for an epic's own `CriteriaRecorded` gate. Empty for a task.
    pub children: Vec<ChildState>,
    /// What the Definition of Ready needs.
    pub readiness: ReadinessContext,
    /// How many times the contract has failed the Definition of Ready, counting the failure that
    /// has just happened, so that the third failure escalates (`docs/SPEC.md` section 5.2). The
    /// runtime may keep it as a running total: the gate that reads it also asks whether the
    /// contract fails the Definition of Ready now, so a counter nobody reset cannot escalate a
    /// contract that has since been fixed.
    pub readiness_failed_attempts: u32,
    /// Whether this contract is waiting for the human, and whether the human has answered.
    pub acceptance: ContractAcceptance,
    /// The pair of agents an assignment would name, when the move is an assignment.
    pub assignment: Option<AssignmentInput>,
    /// The results the assignee recorded from its own runs.
    pub assignee_results: Vec<CriterionResult>,
    /// The state of the task's branch and worktree.
    pub work: WorkState,
    /// What the assignee wrote when it blocked the task.
    pub blocker: Option<Blocker>,
    /// What cleared the blocker.
    pub blocker_resolution: Option<String>,
    /// When the task blocked, if it is blocked.
    pub blocked_at: Option<DateTime<Utc>>,
    /// The runtime's clock, passed in because `farik-core` reads no clock of its own.
    pub now: DateTime<Utc>,
    /// How long a task may stay blocked before it escalates
    /// (`escalation::DEFAULT_BLOCKED_LIMIT` when the team set nothing).
    pub blocked_limit: Duration,
    /// The evidence gathered for the Definition of Done.
    pub done: DoneEvidence,
    /// What the reviewer wrote when it rejected the work.
    pub rejection: Option<Rejection>,
    /// Every budget's spend and limit. What is left of the sprint appears three times in this
    /// context — here, in `readiness`, and in `assignment` — because each of those answers its own
    /// question; the runtime derives all three from one figure, or the Definition of Ready and the
    /// assignment gate could disagree about the same sprint.
    pub budget: BudgetState,
    /// Whether a permission was denied on an action the task requires.
    pub permission_denied: bool,
    /// Whether Farik could not run one of the task's criteria for its reviewer, for a reason that
    /// is not the work's (5.4): the task goes to the human rather than back to its assignee.
    pub criterion_unrunnable: bool,
}

/// What the runtime must record along with the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionEffect {
    /// The contract's `iteration` goes up by one: the task has been returned to `in_progress`
    /// after a rejection once more, which is what `max_iterations` bounds.
    IncrementIteration,
    /// An escalation is raised, for this reason (`docs/SPEC.md` section 5.7).
    RaiseEscalation(EscalationReason),
    /// The blocker and its time are cleared: the task is moving again.
    ResetBlocker,
    /// The time the block is aged from is stamped. Without it the `blocked -> escalated` row can
    /// never open, because a block the runtime recorded no time for cannot be aged, so the stamp is
    /// part of the decision rather than something to be remembered.
    StampBlockedAt,
}

/// The move the governor will apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionDecision {
    /// The status the task leaves.
    pub from: TaskStatus,
    /// The status it enters, the request's own rather than the row's pattern.
    pub to: TaskStatus,
    /// The row of the table the move takes.
    pub row: &'static TransitionRow,
    /// What the runtime records with it, in this order.
    pub effects: Vec<TransitionEffect>,
}

/// One gate that refused, and everything it refused for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFailure {
    /// Which gate.
    pub gate: GateId,
    /// Every reason it gives, in the order the gate writes them.
    pub details: Vec<String>,
}

/// Why a move is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionRefusal {
    /// The request names one task and the contract another, so the runtime has paired them wrongly
    /// and nothing here can say what either task may do. The status a move leaves comes from the
    /// contract, which is the one place it is written (`docs/SPEC.md` section 5.11), so a request
    /// decided from somebody else's contract would record a row that was never open for either.
    WrongTask {
        /// The task the request names.
        asked: TaskId,
        /// The task the contract is for.
        contract: TaskId,
    },
    /// No row of the table moves a task from this status to that one. `transitions_from` says
    /// what this status can reach, for a caller that wants to tell the agent what to ask instead.
    NoSuchTransition {
        /// The status the task is in.
        from: TaskStatus,
        /// The status the request asked for.
        to: TaskStatus,
    },
    /// Rows exist for the move, but none of them is this actor's.
    ActorNotAllowed {
        /// Who asked.
        actor: TransitionActor,
        /// Who may ask, in table order.
        allowed: Vec<TransitionActor>,
    },
    /// The row is the assignee's or the reviewer's, and the agent asking is not the one the
    /// contract names (`docs/SPEC.md` section 5.1).
    NotTheNamedAgent {
        /// Which of the two the row names.
        actor: TransitionActor,
        /// The agent the contract names, if it names one.
        named: Option<String>,
        /// The agent that asked, if the runtime named one.
        asked: Option<String>,
    },
    /// Every row open to this actor has a gate that refuses, and this is what each one said.
    GateFailed {
        /// One entry per row tried, in table order.
        failures: Vec<GateFailure>,
    },
}

/// Decides one transition (`docs/SPEC.md` section 5.2): whether the table has a row for it,
/// whether it is this actor's to ask, whether the agent asking is the one the contract names, and
/// whether a gate opens. Several rows can carry one move, each with its own gate — three reasons
/// send a `refining` contract to `escalated` — and the move is allowed when any one of them opens,
/// taken in table order, so that the more specific reason is the one recorded.
///
/// # Errors
///
/// `NoSuchTransition` when the table has no row, `ActorNotAllowed` when no row is this actor's,
/// `NotTheNamedAgent` when the row belongs to the assignee or the reviewer and another agent
/// asked, and `GateFailed` with what every row's gate said when none of them opens.
pub fn evaluate_transition(
    request: &TransitionRequest,
    context: &TransitionContext,
) -> Result<TransitionDecision, TransitionRefusal> {
    if request.task_id != context.contract.id {
        return Err(TransitionRefusal::WrongTask {
            asked: request.task_id.clone(),
            contract: context.contract.id.clone(),
        });
    }
    let rows = find_transitions(context.contract.status, request.to);
    if rows.is_empty() {
        return Err(TransitionRefusal::NoSuchTransition {
            from: context.contract.status,
            to: request.to,
        });
    }
    let mine: Vec<&'static TransitionRow> = rows
        .iter()
        .copied()
        .filter(|row| row.actor == request.actor)
        .collect();
    if mine.is_empty() {
        return Err(TransitionRefusal::ActorNotAllowed {
            actor: request.actor,
            allowed: allowed_actors(&rows),
        });
    }
    if is_named_by_the_contract(request.actor) {
        let named = named_agent(request.actor, &context.contract);
        let named_id = trimmed(named);
        if named_id.is_none() || named_id != trimmed(request.agent_id.as_deref()) {
            return Err(TransitionRefusal::NotTheNamedAgent {
                actor: request.actor,
                named: named.map(str::to_string),
                asked: request.agent_id.clone(),
            });
        }
    }
    let mut failures = Vec::new();
    for row in mine {
        match check_gate(row.gate, request.actor, context) {
            Ok(()) => {
                return Ok(TransitionDecision {
                    from: context.contract.status,
                    to: request.to,
                    row,
                    effects: effects(row.gate, request.to, context),
                });
            }
            Err(details) => failures.push(GateFailure {
                gate: row.gate,
                details,
            }),
        }
    }
    Err(TransitionRefusal::GateFailed { failures })
}

/// An id once trimmed, and nobody when it is blank: a stray space must not make an agent somebody
/// else, and a blank id names no agent at all.
fn trimmed(id: Option<&str>) -> Option<&str> {
    id.map(str::trim).filter(|id| !id.is_empty())
}

/// Who may ask for this move, in table order and without repeats.
fn allowed_actors(rows: &[&'static TransitionRow]) -> Vec<TransitionActor> {
    let mut allowed: Vec<TransitionActor> = Vec::new();
    for row in rows {
        if !allowed.contains(&row.actor) {
            allowed.push(row.actor);
        }
    }
    allowed
}

/// Whether the contract names the agent this row belongs to. The Scrum Master and the Product
/// Manager hold team roles rather than a place on the contract, and the governor and the human are
/// not agents, so a row of theirs names nobody here and the runtime's own authentication is what
/// says who asked.
fn is_named_by_the_contract(actor: TransitionActor) -> bool {
    match actor {
        TransitionActor::Assignee | TransitionActor::Reviewer => true,
        TransitionActor::ScrumMaster
        | TransitionActor::ProductManager
        | TransitionActor::Governor
        | TransitionActor::Human => false,
    }
}

/// The agent the contract names for this actor, exhaustive so that an actor added to the lifecycle
/// cannot quietly read the assignee's field.
fn named_agent(actor: TransitionActor, contract: &TaskContract) -> Option<&str> {
    match actor {
        TransitionActor::Assignee => contract.assignee.as_deref(),
        TransitionActor::Reviewer => contract.reviewer.as_deref(),
        TransitionActor::ScrumMaster
        | TransitionActor::ProductManager
        | TransitionActor::Governor
        | TransitionActor::Human => None,
    }
}

/// Whether one gate opens, and everything it says when it does not.
fn check_gate(gate: GateId, actor: TransitionActor, context: &TransitionContext) -> GateResult {
    match gate {
        GateId::None => Ok(()),
        GateId::Triaged => open_or(context.triaged, || {
            "the request has no recorded triage decision, and refining starts from one (5.16)"
                .to_string()
        }),
        GateId::DefinitionOfReady => {
            let mut details = Vec::new();
            if waits_for_the_human(context) {
                details.push(
                    "this contract needs the human's acceptance before it leaves refining (5.16 item 2), and the human has not given it"
                        .to_string(),
                );
            }
            // The acceptance first, because it is the last thing to happen, and the failures with
            // it: a contract that does not pass the structural checks cannot be put to the human
            // yet (5.16 item 2), so reporting only the acceptance would tell an epic to wait for
            // something nobody can ask for, and the escalation row would say the opposite about the
            // same contract.
            details.extend(readiness_failures(context).unwrap_or_default());
            if details.is_empty() {
                Ok(())
            } else {
                Err(details)
            }
        }
        GateId::ReadinessExhausted => readiness_exhausted(context),
        GateId::ContractRequiresHuman => human_must_accept_the_contract(context),
        GateId::Assignment => match &context.assignment {
            Some(assignment) => {
                let mut asked = assignment.clone();
                asked.requested_by = requester(actor);
                check_assignment(&context.contract, &asked)
            }
            None => Err(vec![
                "the runtime named no pair of agents for this assignment".to_string(),
            ]),
        },
        GateId::CriteriaRecorded => {
            if context.contract.kind == Kind::Epic {
                check_children_done(&context.children)
            } else {
                check_criteria_recorded(&context.contract, &context.assignee_results, &context.work)
            }
        }
        GateId::BlockerWritten => check_blocker_written(context.blocker.as_ref()),
        GateId::BlockerResolved => check_blocker_resolved(context.blocker_resolution.as_deref()),
        GateId::BlockedAge => blocked_long_enough(context),
        GateId::DefinitionOfDone => {
            evaluate_done(&context.contract, &context.done).map_err(|failures| {
                failures
                    .iter()
                    .map(|failure| failure.message.clone())
                    .collect()
            })
        }
        GateId::RejectionReasons => {
            check_rejection_reasons(&context.contract, context.rejection.as_ref())
        }
        GateId::IterationBelowLimit => open_or(
            rejection_outcome(&context.contract) == RejectionOutcome::ReturnToInProgress,
            || {
                format!(
                    "the task has been rejected {} times and the limit is {}, so it escalates rather than being worked again",
                    iteration(&context.contract),
                    max_iterations(&context.contract)
                )
            },
        ),
        GateId::IterationLimitReached => open_or(
            rejection_outcome(&context.contract) == RejectionOutcome::Escalate,
            || {
                format!(
                    "the task has been rejected {} times and the limit is {}, so it is worked again rather than escalated",
                    iteration(&context.contract),
                    max_iterations(&context.contract)
                )
            },
        ),
        GateId::GovernorEscalation => {
            open_or(governor_escalation_reason(context).is_some(), || {
                "no budget whose consequence is escalation is exhausted, no permission was denied, and Farik ran every criterion it tried, so the governor has nothing to escalate"
                    .to_string()
            })
        }
    }
}

/// Who asked for an assignment, from the actor of the row rather than from the input: the request
/// says who is asking and the two must not be able to disagree, or a team with an active Scrum
/// Master could have the Product Manager's row opened by an input that says the Scrum Master asked.
/// Only those two rows carry an assignment (5.2), so no other actor reaches this.
fn requester(actor: TransitionActor) -> AssignmentRequester {
    match actor {
        TransitionActor::ProductManager => AssignmentRequester::ProductManager,
        TransitionActor::ScrumMaster
        | TransitionActor::Assignee
        | TransitionActor::Reviewer
        | TransitionActor::Governor
        | TransitionActor::Human => AssignmentRequester::ScrumMaster,
    }
}

/// `Ok` when the gate opens, and the one reason it gives when it does not, built only then.
fn open_or(open: bool, shut: impl FnOnce() -> String) -> GateResult {
    if open { Ok(()) } else { Err(vec![shut()]) }
}

/// The `ContractRequiresHuman` gate of `refining -> escalated`: the contract needs the human's
/// acceptance and has not had it. The risk and the kind answer half of it
/// (`done::requires_human_acceptance`) and the team's policy the other half, which the runtime
/// passes in because `TeamRules` does not yet carry `human_accepts_contracts`; either is enough,
/// so the two cannot disagree in the direction that would carry a task past the human.
fn human_must_accept_the_contract(context: &TransitionContext) -> GateResult {
    if !(context.acceptance.required_by_policy || requires_human_acceptance(&context.contract)) {
        return Err(vec![
            "this contract does not need the human's acceptance: the risk is not high, it is not an epic, and the team's policy does not ask for it"
                .to_string(),
        ]);
    }
    // 5.16 item 2 asks the human once the contract passes the structural checks. A contract sent to
    // the user on its first readiness failure would lose the three refining attempts 5.2 gives it,
    // and the human's gate-free `escalated -> any` row would then carry a contract that never
    // passed the Definition of Ready into `ready`. This is asked before the acceptance, because a
    // contract the human has accepted that does not pass would otherwise be told it goes to `ready`,
    // which the readying gate refuses: the user writing an epic themselves and locking and
    // approving it (5.11, 5.16) reaches exactly that state.
    if let Some(failures) = readiness_failures(context) {
        let mut details = vec![
            "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                .to_string(),
        ];
        details.extend(failures);
        return Err(details);
    }
    if context.acceptance.given {
        return Err(vec![
            "the human has already accepted this contract, so it goes to ready rather than escalating"
                .to_string(),
        ]);
    }
    Ok(())
}

/// The `ReadinessExhausted` gate of `refining -> escalated`: the contract fails the Definition of
/// Ready now, and it has failed it the limit's worth of times. Asking the contract as well as the
/// counter is what keeps this row and the approval row apart, whatever the runtime's counter does:
/// a contract that failed three times, was sent back by the human and now passes is waiting for the
/// approval 5.16 item 2 asks for, not for its failures again, and one nobody has to accept is
/// simply ready.
fn readiness_exhausted(context: &TransitionContext) -> GateResult {
    if readiness_failures(context).is_none() {
        return Err(vec![
            "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                .to_string(),
        ]);
    }
    open_or(
        evaluate_readiness_attempts(context.readiness_failed_attempts)
            == ReadinessOutcome::Escalate,
        || {
            format!(
                "the contract has failed the Definition of Ready {} times, and it is refined again until {READINESS_ATTEMPT_LIMIT}",
                context.readiness_failed_attempts
            )
        },
    )
}

/// Whether this contract is waiting for the human's acceptance: the team's policy asks for it, or
/// the contract's own risk or kind does (`done::requires_human_acceptance`), and the human has not
/// answered. Either half is enough, so the two cannot disagree in the direction that would carry a
/// task past the human.
fn waits_for_the_human(context: &TransitionContext) -> bool {
    (context.acceptance.required_by_policy || requires_human_acceptance(&context.contract))
        && !context.acceptance.given
}

/// The Definition of Ready's own messages when the contract fails it, in its own order, and `None`
/// when it passes. A `refining -> escalated` request evaluates this twice when the readiness row
/// shuts and the acceptance row is tried next; it is a pure function of the same values both times,
/// so the two answers cannot differ, and the cost is nineteen re-run checks and no I/O.
fn readiness_failures(context: &TransitionContext) -> Option<Vec<String>> {
    evaluate_readiness(&context.contract, &context.readiness)
        .err()
        .map(|failures| {
            failures
                .iter()
                .map(|failure| failure.message.clone())
                .collect()
        })
}

/// The `BlockedAge` gate of `blocked -> escalated`. A task the runtime stamped no block time for
/// cannot be aged, and saying so is better than escalating on a value that is not there.
fn blocked_long_enough(context: &TransitionContext) -> GateResult {
    let Some(blocked_at) = context.blocked_at else {
        return Err(vec![
            "the runtime recorded no time for this block, and a blocked task escalates on its age"
                .to_string(),
        ]);
    };
    open_or(
        evaluate_blocked_age(blocked_at, context.now, context.blocked_limit)
            == BlockedAge::Exceeded,
        || {
            format!(
                "the task has not been blocked for {} seconds yet",
                context.blocked_limit.as_secs()
            )
        },
    )
}

/// The contract's `iteration`, saturated into the width step 06 counts in. A count above
/// `u32::MAX` is a corrupt figure, and saturating sends the task to the human at the next
/// rejection rather than letting it be worked for ever.
fn iteration(contract: &TaskContract) -> u32 {
    u32::try_from(contract.iteration).unwrap_or(u32::MAX)
}

/// The contract's `max_iterations`, saturated the same way. A limit above `u32::MAX` is what the
/// contract asked for: effectively none.
fn max_iterations(contract: &TaskContract) -> u32 {
    u32::try_from(contract.budget.max_iterations.get()).unwrap_or(u32::MAX)
}

fn rejection_outcome(contract: &TaskContract) -> RejectionOutcome {
    evaluate_rejection(iteration(contract), max_iterations(contract))
}

/// Why the governor's own `any -> escalated` row is open, or `None` when it is not. The task's
/// sessions are their own reason (5.7); every other exhausted budget is `budget`; then a denied
/// permission; then a criterion Farik could not run for the reviewer, which asks the human and so
/// is `explicit_request`. A user's `stop` reaches the table as the human's own row instead, which
/// needs no gate.
fn governor_escalation_reason(context: &TransitionContext) -> Option<EscalationReason> {
    for exhausted in check_budgets(&context.budget) {
        if exhausted.consequence != BudgetConsequence::EscalateTask {
            continue;
        }
        return Some(match exhausted.scope {
            BudgetScope::TaskSessions => EscalationReason::Sessions,
            // The task's dollars are the only other scope 5.5 gives `EscalateTask`, and they come
            // first in `BudgetScope`, so a task out of both reads as the dollars, the harder limit.
            // A scope that grew that consequence would escalate on dollars until somebody chose its
            // reason here.
            BudgetScope::TaskUsd
            | BudgetScope::SessionTokens
            | BudgetScope::SessionWallClock
            | BudgetScope::SessionToolCalls
            | BudgetScope::SprintUsd
            | BudgetScope::DayUsd => EscalationReason::Budget,
        });
    }
    if context.permission_denied {
        return Some(EscalationReason::Permission);
    }
    if context.criterion_unrunnable {
        return Some(EscalationReason::ExplicitRequest);
    }
    None
}

/// What the runtime records with the move: a task returning to work after a rejection counts that
/// return, which is what `max_iterations` bounds (step 06, and the schema's own words); a move to
/// `escalated` carries the reason the gate that opened it names; and a task entering `in_progress`
/// leaves its blocker behind, whatever status it came from, because a task at work has none and a
/// stale one would age again — a task that escalates out of `blocked` keeps its blocker, which is
/// what the user is shown.
fn effects(gate: GateId, to: TaskStatus, context: &TransitionContext) -> Vec<TransitionEffect> {
    let mut effects = Vec::new();
    if context.contract.status == TaskStatus::Rejected && to == TaskStatus::InProgress {
        effects.push(TransitionEffect::IncrementIteration);
    }
    if to == TaskStatus::Escalated {
        effects.push(TransitionEffect::RaiseEscalation(escalation_reason(
            gate, context,
        )));
    }
    if to == TaskStatus::InProgress {
        effects.push(TransitionEffect::ResetBlocker);
    }
    if to == TaskStatus::Blocked {
        effects.push(TransitionEffect::StampBlockedAt);
    }
    effects
}

/// The reason an escalation carries, from the gate that opened the row. Exhaustive on purpose: a
/// gate added to the table cannot reach `escalated` without a reason being chosen for it here. The
/// five gates that lead there each name their own; a gate-free row into `escalated` is the human's
/// (5.2), so it is the user asking. No row pairs any other gate with `escalated`, which a test
/// over the table pins.
fn escalation_reason(gate: GateId, context: &TransitionContext) -> EscalationReason {
    match gate {
        GateId::ReadinessExhausted => EscalationReason::ReadinessFailures,
        GateId::ContractRequiresHuman => {
            // The contract's own `high` risk is the risk gate (5.4 item 5); an epic (5.16 item 2)
            // and a contract the team's policy sends to the human are the user's approval. Calling
            // the policy a risk gate would tell the user a low-risk task has one.
            if context.contract.risk == Risk::High {
                EscalationReason::RiskGate
            } else {
                EscalationReason::Approval
            }
        }
        GateId::BlockedAge => EscalationReason::BlockerAge,
        GateId::IterationLimitReached => EscalationReason::Iterations,
        GateId::GovernorEscalation => {
            governor_escalation_reason(context).unwrap_or(EscalationReason::Budget)
        }
        GateId::None
        | GateId::Triaged
        | GateId::DefinitionOfReady
        | GateId::Assignment
        | GateId::CriteriaRecorded
        | GateId::BlockerWritten
        | GateId::BlockerResolved
        | GateId::DefinitionOfDone
        | GateId::RejectionReasons
        | GateId::IterationBelowLimit => EscalationReason::ExplicitRequest,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{DateTime, TimeZone, Utc};

    use super::{
        ContractAcceptance, GateFailure, TransitionContext, TransitionDecision, TransitionEffect,
        TransitionRefusal, TransitionRequest, evaluate_transition,
    };
    use crate::budget::{BudgetState, DEFAULT_SESSION_LIMITS, SessionLedger};
    use crate::contract::{Role, TaskStatus};
    use crate::generated::task_contract::FarikTaskContractKind as Kind;
    use crate::generated::task_contract::FarikTaskContractRisk as Risk;
    use crate::governor::done::{CriterionResult, DoneEvidence, RunBy};
    use crate::governor::escalation::{
        DEFAULT_BLOCKED_LIMIT, EscalationReason as Why, RejectionOutcome,
    };
    use crate::governor::gates::{
        AssignmentInput, AssignmentRequester, Blocker, ChildState, Rejection, WorkState,
    };
    use crate::governor::readiness::fixtures::{a_contract, a_ready_context};
    use crate::governor::transition_table::{
        GateId, Status, TRANSITION_TABLE, TransitionActor as A,
    };

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 16, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn an_assignment() -> AssignmentInput {
        AssignmentInput {
            requested_by: AssignmentRequester::ScrumMaster,
            has_active_scrum_master: true,
            assignee_id: "dev-1".to_string(),
            assignee_role: Role::SoftwareDeveloper,
            reviewer_id: "arch-1".to_string(),
            reviewer_role: Role::Architect,
            assignee_open_tasks: 0,
            wip_limit: 2,
            remaining_sprint_budget_usd: 15.0,
            dependencies: Vec::new(),
        }
    }

    fn a_result(run_by: RunBy) -> CriterionResult {
        CriterionResult {
            criterion_id: "C1".to_string(),
            passed: true,
            evidence: "cargo test: 11 passed".to_string(),
            run_by,
        }
    }

    fn a_budget() -> BudgetState {
        BudgetState {
            session: SessionLedger::default(),
            session_limits: DEFAULT_SESSION_LIMITS,
            task_spent_usd: 1.0,
            task_max_usd: 5.0,
            task_sessions: 1,
            task_max_sessions: 5,
            sprint_spent_usd: 3.0,
            sprint_max_usd: 15.0,
            day_spent_usd: 4.0,
            day_max_usd: 20.0,
        }
    }

    /// A task in `in_progress`, assigned to `dev-1` and reviewed by `arch-1`, with everything a
    /// gate could ask for in the state that lets it pass, so that a test sets only what it is
    /// about.
    fn a_context() -> TransitionContext {
        let mut contract = a_contract();
        contract.status = TaskStatus::InProgress;
        contract.assignee = Some("dev-1".to_string());
        contract.reviewer = Some("arch-1".to_string());
        TransitionContext {
            contract,
            triaged: true,
            children: Vec::new(),
            readiness: a_ready_context(),
            readiness_failed_attempts: 1,
            acceptance: ContractAcceptance::default(),
            assignment: Some(an_assignment()),
            assignee_results: vec![a_result(RunBy::Assignee)],
            work: WorkState {
                commits: 1,
                worktree_clean: true,
            },
            blocker: Some(Blocker {
                description: "The staging database refuses the migration.".to_string(),
                needed: "A password for the staging database.".to_string(),
            }),
            blocker_resolution: Some("The password is in the vault.".to_string()),
            blocked_at: Some(at(1)),
            now: at(2),
            blocked_limit: DEFAULT_BLOCKED_LIMIT,
            done: DoneEvidence {
                results: vec![a_result(RunBy::Reviewer)],
                changed_paths: vec!["src/login/form.rs".to_string()],
                completion_note: Some("The form takes an email and a password.".to_string()),
                review_note: Some("C1: cargo test, 11 passed.".to_string()),
                human_accepted: false,
            },
            rejection: Some(Rejection {
                failed_criterion_ids: vec!["C1".to_string()],
                reasons: "The form accepts an empty password.".to_string(),
            }),
            budget: a_budget(),
            permission_denied: false,
            criterion_unrunnable: false,
        }
    }

    fn ask(to: TaskStatus, actor: A, agent_id: Option<&str>) -> TransitionRequest {
        TransitionRequest {
            task_id: "FRK-1".parse().expect("a task id"),
            to,
            actor,
            agent_id: agent_id.map(str::to_string),
        }
    }

    fn decide(
        request: &TransitionRequest,
        context: &TransitionContext,
    ) -> Result<TransitionDecision, TransitionRefusal> {
        evaluate_transition(request, context)
    }

    fn effects(request: &TransitionRequest, context: &TransitionContext) -> Vec<TransitionEffect> {
        decide(request, context)
            .expect("expected the move to be allowed")
            .effects
    }

    fn gates(request: &TransitionRequest, context: &TransitionContext) -> Vec<GateFailure> {
        match decide(request, context) {
            Err(TransitionRefusal::GateFailed { failures }) => failures,
            other => panic!("expected a gate refusal, got {other:?}"),
        }
    }

    fn one_gate(request: &TransitionRequest, context: &TransitionContext) -> (GateId, Vec<String>) {
        let mut failures = gates(request, context);
        assert_eq!(failures.len(), 1, "expected one gate to be tried");
        let failure = failures.remove(0);
        (failure.gate, failure.details)
    }

    #[test]
    fn refuses_a_request_paired_with_another_tasks_contract() {
        // The status the move leaves comes from the contract, which is the one place it is written
        // (5.11), so a request decided from somebody else's contract would record a row that was
        // never open for either task. The governor refuses what cannot be true rather than guessing
        // which of the two the runtime meant.
        let context = a_context();
        let mut elsewhere = ask(TaskStatus::Verifying, A::Assignee, Some("dev-1"));
        elsewhere.task_id = "FRK-999".parse().expect("a task id");
        assert_eq!(
            decide(&elsewhere, &context),
            Err(TransitionRefusal::WrongTask {
                asked: "FRK-999".parse().expect("a task id"),
                contract: context.contract.id.clone()
            })
        );
    }

    #[test]
    fn refuses_a_move_the_table_has_no_row_for() {
        let mut context = a_context();
        assert_eq!(
            decide(
                &ask(TaskStatus::Accepted, A::Assignee, Some("dev-1")),
                &context
            ),
            Err(TransitionRefusal::NoSuchTransition {
                from: TaskStatus::InProgress,
                to: TaskStatus::Accepted
            })
        );
        // Nothing leaves a terminal status, and no row means staying put.
        context.contract.status = TaskStatus::Accepted;
        assert_eq!(
            decide(&ask(TaskStatus::InProgress, A::Human, None), &context),
            Err(TransitionRefusal::NoSuchTransition {
                from: TaskStatus::Accepted,
                to: TaskStatus::InProgress
            })
        );
        context.contract.status = TaskStatus::InProgress;
        assert_eq!(
            decide(
                &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
                &context
            ),
            Err(TransitionRefusal::NoSuchTransition {
                from: TaskStatus::InProgress,
                to: TaskStatus::InProgress
            })
        );
    }

    #[test]
    fn refuses_an_actor_no_row_names_and_says_who_may_ask() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Ready;
        assert_eq!(
            decide(
                &ask(TaskStatus::Assigned, A::Assignee, Some("dev-1")),
                &context
            ),
            Err(TransitionRefusal::ActorNotAllowed {
                actor: A::Assignee,
                allowed: vec![A::ScrumMaster, A::ProductManager]
            })
        );
    }

    #[test]
    fn opens_the_assignees_row_only_to_the_agent_the_contract_names() {
        let mut context = a_context();
        assert_eq!(
            decide(
                &ask(TaskStatus::Verifying, A::Assignee, Some("dev-2")),
                &context
            ),
            Err(TransitionRefusal::NotTheNamedAgent {
                actor: A::Assignee,
                named: Some("dev-1".to_string()),
                asked: Some("dev-2".to_string())
            })
        );
        // A stray space is the same agent.
        assert!(
            decide(
                &ask(TaskStatus::Verifying, A::Assignee, Some(" dev-1 ")),
                &context
            )
            .is_ok()
        );
        // An unassigned task has no assignee to ask for it, however the request is spelled, and a
        // contract whose assignee is a blank names nobody either.
        for named in [None, Some("  ")] {
            context.contract.assignee = named.map(str::to_string);
            for asked in [None, Some("dev-1"), Some("  ")] {
                assert_eq!(
                    decide(&ask(TaskStatus::Verifying, A::Assignee, asked), &context),
                    Err(TransitionRefusal::NotTheNamedAgent {
                        actor: A::Assignee,
                        named: named.map(str::to_string),
                        asked: asked.map(str::to_string)
                    }),
                    "{named:?} {asked:?}"
                );
            }
        }
    }

    #[test]
    fn opens_the_reviewers_row_only_to_the_reviewer_the_contract_names() {
        let context = a_context();
        let mut verifying = a_context();
        verifying.contract.status = TaskStatus::Verifying;
        assert_eq!(
            decide(
                &ask(TaskStatus::Rejected, A::Reviewer, Some("dev-1")),
                &verifying
            ),
            Err(TransitionRefusal::NotTheNamedAgent {
                actor: A::Reviewer,
                named: Some("arch-1".to_string()),
                asked: Some("dev-1".to_string())
            })
        );
        assert!(
            decide(
                &ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1")),
                &verifying
            )
            .is_ok()
        );
        // The Scrum Master, the Product Manager, the governor and the human hold no place on the
        // contract, so their rows ask nothing about an agent id.
        let mut blocked = context;
        blocked.contract.status = TaskStatus::Blocked;
        assert!(decide(&ask(TaskStatus::InProgress, A::ScrumMaster, None), &blocked).is_ok());
    }

    #[test]
    fn refines_a_draft_request_only_once_it_is_triaged() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Draft;
        let request = ask(TaskStatus::Refining, A::ProductManager, Some("pm-1"));
        assert_eq!(effects(&request, &context), []);
        context.triaged = false;
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::Triaged,
                vec![
                    "the request has no recorded triage decision, and refining starts from one (5.16)"
                        .to_string()
                ]
            )
        );
    }

    #[test]
    fn readies_a_contract_that_passes_the_definition_of_ready() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        let request = ask(TaskStatus::Ready, A::Governor, None);
        assert_eq!(effects(&request, &context), []);
        // The gate's details are the Definition of Ready's own messages, in its own order.
        context.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        let (gate, details) = one_gate(&request, &context);
        assert_eq!(gate, GateId::DefinitionOfReady);
        assert_eq!(
            details,
            vec!["the intent is blank; state the user-facing reason for the task".to_string()]
        );
    }

    #[test]
    fn keeps_a_contract_the_human_has_not_accepted_out_of_ready() {
        // Spec 5.16 item 2: every epic requires the human's acceptance of its contract before it
        // leaves `refining`, whatever its risk, and 5.2 says the same for a high risk and for the
        // team's policy. This is the only function the runtime asks, so a rule it does not hold is
        // not held: the epic would reach `ready`, then `assigned`, with no approval ever asked for.
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        let ready = ask(TaskStatus::Ready, A::Governor, None);
        let waiting =
            "this contract needs the human's acceptance before it leaves refining (5.16 item 2), and the human has not given it"
                .to_string();
        context.contract.kind = Kind::Epic;
        assert_eq!(
            one_gate(&ready, &context),
            (GateId::DefinitionOfReady, vec![waiting.clone()])
        );
        context.contract.kind = Kind::Task;
        context.contract.risk = Risk::High;
        assert_eq!(
            one_gate(&ready, &context),
            (GateId::DefinitionOfReady, vec![waiting.clone()])
        );
        context.contract.risk = Risk::Medium;
        context.acceptance.required_by_policy = true;
        assert_eq!(
            one_gate(&ready, &context),
            (GateId::DefinitionOfReady, vec![waiting.clone()])
        );
        // The acceptance comes first because it is the last thing to happen, but the failures are
        // reported with it: a contract that does not pass the structural checks cannot be put to
        // the human yet (5.16 item 2), so reporting only the acceptance would tell an epic to wait
        // for something nobody can ask for, and the escalation row would say the opposite about the
        // same contract.
        context.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        assert_eq!(
            one_gate(&ready, &context),
            (
                GateId::DefinitionOfReady,
                vec![
                    waiting,
                    "the intent is blank; state the user-facing reason for the task".to_string()
                ]
            )
        );
        // The fixture's contract is `in_progress`, and this test is about a `refining` one.
        context.contract = a_context().contract;
        context.contract.status = TaskStatus::Refining;
        // Once the human has accepted, the Definition of Ready is the whole gate again.
        context.acceptance.given = true;
        assert_eq!(effects(&ready, &context), []);
    }

    #[test]
    fn asks_the_human_only_once_the_contract_passes_the_structural_checks() {
        // Spec 5.16 item 2: "once the contract passes the structural checks, the governor moves the
        // epic to `escalated` with reason `approval`". An epic sent to the user on its first
        // readiness failure would lose the three refining attempts 5.2 gives it, and the human's
        // gate-free `escalated -> any` row would then carry a contract that never passed the
        // Definition of Ready into `ready`.
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        context.contract.kind = Kind::Epic;
        context.readiness_failed_attempts = 1;
        context.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        let request = ask(TaskStatus::Escalated, A::Governor, None);
        let failures = gates(&request, &context);
        assert_eq!(failures[1].gate, GateId::ContractRequiresHuman);
        assert_eq!(
            failures[1].details,
            vec![
                "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                    .to_string(),
                "the intent is blank; state the user-facing reason for the task".to_string()
            ]
        );
        // The same epic, written properly, goes to the user.
        // The fixture's contract is `in_progress`, and this test is about a `refining` one.
        context.contract = a_context().contract;
        context.contract.status = TaskStatus::Refining;
        context.contract.kind = Kind::Epic;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Approval)]
        );
    }

    #[test]
    fn escalates_a_contract_that_has_failed_the_definition_of_ready_three_times() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        context.readiness_failed_attempts = 3;
        // The row is about a contract that fails the Definition of Ready, so this one does.
        context.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        let request = ask(TaskStatus::Escalated, A::Governor, None);
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
        );
        // Below the limit, all three of the governor's rows into `escalated` are tried, in table
        // order, and each says what it is waiting for.
        context.readiness_failed_attempts = 2;
        let failures = gates(&request, &context);
        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.gate)
                .collect::<Vec<GateId>>(),
            [
                GateId::ReadinessExhausted,
                GateId::ContractRequiresHuman,
                GateId::GovernorEscalation
            ]
        );
        assert_eq!(
            failures[0].details,
            vec![
                "the contract has failed the Definition of Ready 2 times, and it is refined again until 3"
                    .to_string()
            ]
        );
    }

    #[test]
    fn asks_the_human_about_a_contract_that_now_passes_however_often_it_failed_before() {
        // The two rows into `escalated` from `refining` must not overlap: a contract that failed
        // the Definition of Ready three times, was sent back by the human, and now passes is
        // waiting for the approval 5.16 item 2 asks for, not for its readiness failures again. The
        // board would otherwise show it as having failed readiness, the human would resolve it by
        // moving it rather than approving it, and the approval `check_product_doc_write` needs
        // would never be recorded.
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        context.contract.kind = Kind::Epic;
        context.readiness_failed_attempts = 3;
        let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
            .expect("the move is allowed");
        assert_eq!(decision.row.gate, GateId::ContractRequiresHuman);
        assert_eq!(
            decision.effects,
            [TransitionEffect::RaiseEscalation(Why::Approval)]
        );
        // And a contract nobody has to accept, that now passes, is not escalated at all: it is
        // ready, whatever the counter says.
        context.contract.kind = Kind::Task;
        let failures = gates(&ask(TaskStatus::Escalated, A::Governor, None), &context);
        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.gate)
                .collect::<Vec<GateId>>(),
            [
                GateId::ReadinessExhausted,
                GateId::ContractRequiresHuman,
                GateId::GovernorEscalation
            ]
        );
        assert_eq!(
            failures[0].details,
            vec![
                "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                    .to_string()
            ]
        );
        // The contract is answered before the counter, so a fixed contract reads the same whatever
        // the runtime counted.
        context.readiness_failed_attempts = 1;
        assert_eq!(
            gates(&ask(TaskStatus::Escalated, A::Governor, None), &context)[0].details,
            vec![
                "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                    .to_string()
            ]
        );
        assert_eq!(
            effects(&ask(TaskStatus::Ready, A::Governor, None), &context),
            []
        );
    }

    #[test]
    fn escalates_a_contract_that_waits_for_the_human_and_names_why() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        context.readiness_failed_attempts = 0;
        let request = ask(TaskStatus::Escalated, A::Governor, None);
        // An epic waits for the user's approval (5.16 item 2); a high-risk task waits on the risk
        // gate (5.4 item 5). The reason the escalation carries says which.
        context.contract.kind = Kind::Epic;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Approval)]
        );
        context.contract.kind = Kind::Task;
        context.contract.risk = Risk::High;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::RiskGate)]
        );
        // The team's policy is the other half of the question, and either half is enough. A
        // contract the policy sends to the human has no risk gate, so it is the user's approval:
        // 5.7's `risk_gate` is the contract's own `high` risk and nothing else.
        context.contract.risk = Risk::Medium;
        context.acceptance.required_by_policy = true;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Approval)]
        );
        // A contract the human has already accepted goes to `ready`, not to the human again.
        context.acceptance.given = true;
        let details = gates(&request, &context)[1].details.clone();
        assert_eq!(
            details,
            vec![
                "the human has already accepted this contract, so it goes to ready rather than escalating"
                    .to_string()
            ]
        );
        // A contract the human has accepted that does not pass the Definition of Ready must not be
        // told it goes to `ready`, because the readying gate refuses that very move: the flow 5.16
        // names, a user writing an epic themselves and locking and approving it, reaches this state.
        context.acceptance.given = true;
        let mut unready = a_context();
        unready.contract.status = TaskStatus::Refining;
        unready.contract.kind = Kind::Epic;
        unready.acceptance.given = true;
        unready.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        assert_eq!(
            gates(&request, &unready)[1].details,
            vec![
                "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                    .to_string(),
                "the intent is blank; state the user-facing reason for the task".to_string()
            ]
        );
        // And one that needs nobody's acceptance says that instead.
        context.acceptance = ContractAcceptance::default();
        let details = gates(&request, &context)[1].details.clone();
        assert_eq!(
            details,
            vec![
                "this contract does not need the human's acceptance: the risk is not high, it is not an epic, and the team's policy does not ask for it"
                    .to_string()
            ]
        );
    }

    #[test]
    fn assigns_a_task_through_the_assignment_gate() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Ready;
        let request = ask(TaskStatus::Assigned, A::ScrumMaster, Some("sm-1"));
        assert_eq!(effects(&request, &context), []);
        // The gate's details are the assignment gate's own: a limit of zero is that gate's answer
        // and this module does not reword it.
        context
            .assignment
            .as_mut()
            .expect("the fixture assigns")
            .wip_limit = 0;
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::Assignment,
                vec!["dev-1 takes no work: its limit is zero".to_string()]
            )
        );
        // And the Product Manager asks only when the team has no active Scrum Master, which is the
        // same gate refusing on its own row rather than a row missing. Who asked comes from the
        // request's actor, not from the input's `requested_by`, or the fixture's Scrum Master would
        // open the Product Manager's row.
        let mut by_the_pm = a_context();
        by_the_pm.contract.status = TaskStatus::Ready;
        assert_eq!(
            one_gate(
                &ask(TaskStatus::Assigned, A::ProductManager, Some("pm-1")),
                &by_the_pm
            ),
            (
                GateId::Assignment,
                vec![
                    "the Product Manager assigns only when the team has no active Scrum Master"
                        .to_string()
                ]
            )
        );
        // A runtime that named no pair of agents is told so rather than refused for a rule it could
        // not have met.
        context.assignment = None;
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::Assignment,
                vec!["the runtime named no pair of agents for this assignment".to_string()]
            )
        );
    }

    #[test]
    fn verifies_a_task_on_its_own_runs_and_an_epic_on_its_tasks() {
        let mut context = a_context();
        let request = ask(TaskStatus::Verifying, A::Assignee, Some("dev-1"));
        assert_eq!(effects(&request, &context), []);
        context.assignee_results = Vec::new();
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::CriteriaRecorded,
                vec!["the assignee recorded no run with evidence for criterion C1".to_string()]
            )
        );
        // An epic is verified on its tasks instead, and nothing asks its assignee for a run.
        context.contract.kind = Kind::Epic;
        context.children = vec![ChildState {
            task_id: "FRK-2".to_string(),
            status: TaskStatus::Accepted,
        }];
        assert_eq!(effects(&request, &context), []);
        context.children = vec![ChildState {
            task_id: "FRK-2".to_string(),
            status: TaskStatus::InProgress,
        }];
        let (gate, details) = one_gate(&request, &context);
        assert_eq!(gate, GateId::CriteriaRecorded);
        assert_eq!(details.len(), 2);
    }

    #[test]
    fn blocks_a_task_with_a_written_blocker_and_clears_it_with_a_resolution() {
        let mut context = a_context();
        let block = ask(TaskStatus::Blocked, A::Assignee, Some("dev-1"));
        // Something has to record the time the block is aged from, or `blocked -> escalated` can
        // never open and a forgotten stamp would quietly retire the rule.
        assert_eq!(
            effects(&block, &context),
            [TransitionEffect::StampBlockedAt]
        );
        // A blocker that says nothing about what is needed is the gate's answer, not a missing
        // value, and the details are the gate's own.
        context.blocker.as_mut().expect("the fixture blocks").needed = " ".to_string();
        assert_eq!(
            one_gate(&block, &context),
            (
                GateId::BlockerWritten,
                vec!["the blocker does not say what is needed to clear it".to_string()]
            )
        );
        context.blocker = None;
        assert_eq!(one_gate(&block, &context).0, GateId::BlockerWritten);
        // Leaving `blocked` for work clears the blocker, whoever asked.
        let mut blocked = a_context();
        blocked.contract.status = TaskStatus::Blocked;
        for actor in [A::ScrumMaster, A::Human] {
            assert_eq!(
                effects(&ask(TaskStatus::InProgress, actor, None), &blocked),
                [TransitionEffect::ResetBlocker],
                "{actor:?}"
            );
        }
        blocked.blocker_resolution = None;
        assert_eq!(
            one_gate(&ask(TaskStatus::InProgress, A::Human, None), &blocked).0,
            GateId::BlockerResolved
        );
    }

    #[test]
    fn escalates_a_task_blocked_for_the_limit_or_longer() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Blocked;
        context.blocked_limit = Duration::from_secs(3600);
        let request = ask(TaskStatus::Escalated, A::Governor, None);
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::BlockerAge)]
        );
        // Within the limit, the age row says so; the governor's own row is tried after it.
        context.blocked_limit = Duration::from_secs(7200);
        context.now = at(2);
        let failures = gates(&request, &context);
        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.gate)
                .collect::<Vec<GateId>>(),
            [GateId::BlockedAge, GateId::GovernorEscalation]
        );
        assert_eq!(
            failures[0].details,
            vec!["the task has not been blocked for 7200 seconds yet".to_string()]
        );
        // A block the runtime stamped no time for cannot be aged.
        context.blocked_at = None;
        assert_eq!(
            gates(&request, &context)[0].details,
            vec![
                "the runtime recorded no time for this block, and a blocked task escalates on its age"
                    .to_string()
            ]
        );
        // With the age row shut and the task's own budget exhausted, the governor's own row carries
        // the move, and the reason follows the row that opened rather than the one that did not.
        context.budget.task_spent_usd = context.budget.task_max_usd;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Budget)]
        );
    }

    #[test]
    fn accepts_a_task_on_the_definition_of_done() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Verifying;
        let request = ask(TaskStatus::Accepted, A::ProductManager, Some("pm-1"));
        assert_eq!(effects(&request, &context), []);
        context.done.review_note = None;
        let (gate, details) = one_gate(&request, &context);
        assert_eq!(gate, GateId::DefinitionOfDone);
        assert_eq!(
            details,
            vec![
                "the reviewer wrote no review note mapping each criterion to its evidence"
                    .to_string()
            ]
        );
    }

    #[test]
    fn rejects_work_only_with_written_reasons_and_leaves_the_count_to_the_return() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Verifying;
        let request = ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1"));
        // The contract's `iteration` is how many times the task has already been returned to
        // `in_progress` after a rejection, which is what `max_iterations` bounds (the schema's own
        // words, and step 06's decision), so the rejection itself records nothing.
        assert_eq!(effects(&request, &context), []);
        context.rejection = None;
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::RejectionReasons,
                vec![
                    "work is rejected with written reasons mapped to the criteria that failed"
                        .to_string()
                ]
            )
        );
        // A rejection the contract cannot make sense of is the gate's own answer, unreworded.
        context.rejection = Some(Rejection {
            failed_criterion_ids: vec!["C9".to_string()],
            reasons: "The form accepts an empty password.".to_string(),
        });
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::RejectionReasons,
                vec!["this contract has no criterion C9".to_string()]
            )
        );
    }

    #[test]
    fn counts_the_iteration_when_the_task_returns_to_work_and_clears_its_blocker() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Rejected;
        // The return is what the count is of, and a task going back to work carries no blocker.
        assert_eq!(
            effects(&ask(TaskStatus::InProgress, A::Governor, None), &context),
            [
                TransitionEffect::IncrementIteration,
                TransitionEffect::ResetBlocker
            ]
        );
        // A task the human takes out of `escalated` into work leaves its blocker behind too: one
        // that escalated out of `blocked` kept it, and a stale blocker would age again.
        context.contract.status = TaskStatus::Escalated;
        assert_eq!(
            effects(&ask(TaskStatus::InProgress, A::Human, None), &context),
            [TransitionEffect::ResetBlocker]
        );
    }

    #[test]
    fn works_a_rejected_task_again_until_the_iteration_limit() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Rejected;
        let again = ask(TaskStatus::InProgress, A::Governor, None);
        let escalate = ask(TaskStatus::Escalated, A::Governor, None);
        // The contract's default limit is three; the iteration counts the returns so far, and the
        // return this decides is the one it counts.
        context.contract.iteration = 2;
        assert_eq!(
            effects(&again, &context),
            [
                TransitionEffect::IncrementIteration,
                TransitionEffect::ResetBlocker
            ]
        );
        assert_eq!(
            gates(&escalate, &context)[0].details,
            vec![
                "the task has been rejected 2 times and the limit is 3, so it is worked again rather than escalated"
                    .to_string()
            ]
        );
        context.contract.iteration = 3;
        assert_eq!(
            effects(&escalate, &context),
            [TransitionEffect::RaiseEscalation(Why::Iterations)]
        );
        assert_eq!(
            one_gate(&again, &context),
            (
                GateId::IterationBelowLimit,
                vec![
                    "the task has been rejected 3 times and the limit is 3, so it escalates rather than being worked again"
                        .to_string()
                ]
            )
        );
        // A count the schema's `u64` can hold but `u32` cannot is a corrupt figure, and the safe
        // reading sends the task to the human rather than working it for ever.
        context.contract.iteration = u64::from(u32::MAX) + 1;
        assert_eq!(
            effects(&escalate, &context),
            [TransitionEffect::RaiseEscalation(Why::Iterations)]
        );
        // A limit that large is what the contract asked for: effectively none, so the task is
        // worked again however many times it has been returned.
        context.contract.budget.max_iterations = (u64::from(u32::MAX) + 1)
            .try_into()
            .expect("a limit above u32::MAX is still non-zero");
        context.contract.iteration = 5;
        assert_eq!(
            effects(&again, &context),
            [
                TransitionEffect::IncrementIteration,
                TransitionEffect::ResetBlocker
            ]
        );
    }

    #[test]
    fn escalates_only_on_a_budget_whose_consequence_is_escalation() {
        let mut context = a_context();
        let request = ask(TaskStatus::Escalated, A::Governor, None);
        let nothing_to_escalate =
            "no budget whose consequence is escalation is exhausted, no permission was denied, and Farik ran every criterion it tried, so the governor has nothing to escalate"
                .to_string();
        assert_eq!(
            one_gate(&request, &context),
            (
                GateId::GovernorEscalation,
                vec![nothing_to_escalate.clone()]
            )
        );
        // Spec 5.5 gives every budget its own consequence, and only the task's carry escalation:
        // the session's end the session and block the task, the sprint's stops new assignments,
        // and the day's pauses the team. Escalating one task for any of those would be the wrong
        // answer to the wrong question.
        for elsewhere in [
            |budget: &mut BudgetState| {
                budget.session.usage.input_tokens = budget.session_limits.max_input_tokens;
            },
            |budget: &mut BudgetState| {
                budget.session.wall_clock = budget.session_limits.max_wall_clock;
            },
            |budget: &mut BudgetState| {
                budget.session.tool_calls = budget.session_limits.max_tool_calls;
            },
            |budget: &mut BudgetState| budget.sprint_spent_usd = budget.sprint_max_usd,
            |budget: &mut BudgetState| budget.day_spent_usd = budget.day_max_usd,
        ] {
            context.budget = a_budget();
            elsewhere(&mut context.budget);
            assert_eq!(
                one_gate(&request, &context),
                (
                    GateId::GovernorEscalation,
                    vec![nothing_to_escalate.clone()]
                )
            );
        }
        // The task's dollars are `budget` and its sessions are `sessions` (5.7).
        context.budget = a_budget();
        context.budget.task_spent_usd = context.budget.task_max_usd;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Budget)]
        );
        context.budget = a_budget();
        context.budget.task_sessions = context.budget.task_max_sessions;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Sessions)]
        );
        // Both at once: the dollars are the harder limit and the reason the user reads.
        context.budget.task_spent_usd = context.budget.task_max_usd;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Budget)]
        );
        // A budget is read before a permission, so a task that has run out of money and been
        // denied something reads as the money.
        context.permission_denied = true;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Budget)]
        );
        // And a permission denied on a required action, with every budget in hand.
        context.budget = a_budget();
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Permission)]
        );
        // A permission is read before a criterion Farik could not run.
        context.criterion_unrunnable = true;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::Permission)]
        );
        // And a criterion Farik could not run for the reviewer, for a reason that is not the
        // work's (5.4): Farik asks the human, which is an explicit request.
        context.permission_denied = false;
        assert_eq!(
            effects(&request, &context),
            [TransitionEffect::RaiseEscalation(Why::ExplicitRequest)]
        );
    }

    #[test]
    fn lets_the_human_escalate_or_cancel_anything_and_move_an_escalated_task() {
        let mut context = a_context();
        assert_eq!(
            effects(&ask(TaskStatus::Escalated, A::Human, None), &context),
            [TransitionEffect::RaiseEscalation(Why::ExplicitRequest)]
        );
        assert_eq!(
            effects(&ask(TaskStatus::Cancelled, A::Human, None), &context),
            []
        );
        // From `escalated` the human may put the task anywhere the lifecycle has room for.
        context.contract.status = TaskStatus::Escalated;
        for to in [
            TaskStatus::Refining,
            TaskStatus::Ready,
            TaskStatus::InProgress,
            TaskStatus::Accepted,
        ] {
            assert!(decide(&ask(to, A::Human, None), &context).is_ok(), "{to}");
        }
        // An agent cannot: the row is the human's, and the actor is answered before the agent id,
        // so a reviewer asking with the wrong id is told the row is not its actor's at all.
        for (actor, agent_id) in [(A::Assignee, "dev-1"), (A::Reviewer, "dev-2")] {
            assert_eq!(
                decide(&ask(TaskStatus::Ready, actor, Some(agent_id)), &context),
                Err(TransitionRefusal::ActorNotAllowed {
                    actor,
                    allowed: vec![A::Human]
                }),
                "{actor:?}"
            );
        }
        // Cancelling is the human's too, and the two rows that carry `escalated -> cancelled` name
        // the same actor, which is named once.
        assert_eq!(
            decide(
                &ask(TaskStatus::Cancelled, A::Assignee, Some("dev-1")),
                &context
            ),
            Err(TransitionRefusal::ActorNotAllowed {
                actor: A::Assignee,
                allowed: vec![A::Human]
            })
        );
    }

    /// One row of the table, the move that takes it, and what about the state opens that row
    /// rather than another.
    struct Case {
        row: usize,
        from: TaskStatus,
        to: TaskStatus,
        actor: A,
        prepare: fn(&mut TransitionContext),
    }

    const fn case(
        row: usize,
        from: TaskStatus,
        to: TaskStatus,
        actor: A,
        prepare: fn(&mut TransitionContext),
    ) -> Case {
        Case {
            row,
            from,
            to,
            actor,
            prepare,
        }
    }

    /// One case per row of `TRANSITION_TABLE`, in its order.
    fn every_row() -> [Case; 20] {
        use TaskStatus as S;
        let no_change: fn(&mut TransitionContext) = |_| {};
        [
            case(0, S::Draft, S::Refining, A::ProductManager, no_change),
            case(1, S::Refining, S::Ready, A::Governor, no_change),
            case(2, S::Refining, S::Escalated, A::Governor, |context| {
                context.readiness_failed_attempts = 3;
                context.contract.intent = " "
                    .repeat(24)
                    .parse()
                    .expect("twenty-four spaces pass the schema");
            }),
            case(3, S::Refining, S::Escalated, A::Governor, |context| {
                context.readiness_failed_attempts = 0;
                context.contract.risk = Risk::High;
            }),
            case(4, S::Ready, S::Assigned, A::ScrumMaster, no_change),
            case(5, S::Ready, S::Assigned, A::ProductManager, |context| {
                let assignment = context.assignment.as_mut().expect("the fixture assigns");
                assignment.requested_by = AssignmentRequester::ProductManager;
                assignment.has_active_scrum_master = false;
            }),
            case(6, S::Assigned, S::InProgress, A::Assignee, no_change),
            case(7, S::InProgress, S::Verifying, A::Assignee, no_change),
            case(8, S::InProgress, S::Blocked, A::Assignee, no_change),
            case(9, S::Blocked, S::InProgress, A::ScrumMaster, no_change),
            case(10, S::Blocked, S::InProgress, A::Human, no_change),
            case(11, S::Blocked, S::Escalated, A::Governor, |context| {
                context.blocked_limit = Duration::from_secs(3600);
            }),
            case(12, S::Verifying, S::Accepted, A::ProductManager, no_change),
            case(13, S::Verifying, S::Rejected, A::Reviewer, no_change),
            case(14, S::Rejected, S::InProgress, A::Governor, no_change),
            case(15, S::Rejected, S::Escalated, A::Governor, |context| {
                context.contract.iteration = 3;
            }),
            case(16, S::InProgress, S::Escalated, A::Governor, |context| {
                context.permission_denied = true;
            }),
            case(17, S::InProgress, S::Escalated, A::Human, no_change),
            case(18, S::InProgress, S::Cancelled, A::Human, no_change),
            case(19, S::Escalated, S::Ready, A::Human, no_change),
        ]
    }

    #[test]
    fn opens_every_row_of_the_table_with_the_state_that_belongs_to_it() {
        // Spec F5 asks for a test for every transition. `every_row` names, for each row, the move
        // that takes it and the one thing about the state that opens that row rather than another;
        // the last assertion is that the cases cover the table, so a row added to 5.2 fails here
        // until somebody says what opens it.
        let mut covered: Vec<usize> = Vec::new();
        for case in every_row() {
            let mut context = a_context();
            context.contract.status = case.from;
            (case.prepare)(&mut context);
            let agent_id = match case.actor {
                A::Assignee => Some("dev-1"),
                A::Reviewer => Some("arch-1"),
                A::ScrumMaster | A::ProductManager | A::Governor | A::Human => None,
            };
            let decision = decide(&ask(case.to, case.actor, agent_id), &context)
                .unwrap_or_else(|refusal| panic!("row {} was shut: {refusal:?}", case.row));
            assert_eq!(
                *decision.row, TRANSITION_TABLE[case.row],
                "row {}: took {:?}",
                case.row, decision.row
            );
            covered.push(case.row);
        }
        covered.sort_unstable();
        assert_eq!(
            covered,
            (0..TRANSITION_TABLE.len()).collect::<Vec<usize>>(),
            "every row of the table has a case"
        );
    }

    #[test]
    fn refuses_every_row_of_the_table_to_an_actor_it_does_not_name() {
        // `docs/standards/code.md`: every transition has a test that exercises it and one that
        // exercises its refusal. Each row is asked by an actor no row of that move names, and the
        // two rows the contract names an agent for are also asked by the wrong agent.
        for case in every_row() {
            let mut context = a_context();
            context.contract.status = case.from;
            (case.prepare)(&mut context);
            let stranger = match case.actor {
                A::Assignee | A::Reviewer => A::ScrumMaster,
                A::ScrumMaster | A::ProductManager | A::Governor | A::Human => A::Reviewer,
            };
            match decide(&ask(case.to, stranger, Some("arch-1")), &context) {
                Err(TransitionRefusal::ActorNotAllowed { actor, allowed }) => {
                    assert_eq!(actor, stranger, "row {}", case.row);
                    assert!(
                        !allowed.contains(&stranger) && allowed.contains(&case.actor),
                        "row {}: allowed {allowed:?}",
                        case.row
                    );
                }
                other => panic!(
                    "row {}: expected the actor to be refused, got {other:?}",
                    case.row
                ),
            }
            // The assignee's and the reviewer's rows are also shut to another agent.
            if matches!(case.actor, A::Assignee | A::Reviewer) {
                assert!(
                    matches!(
                        decide(&ask(case.to, case.actor, Some("dev-9")), &context),
                        Err(TransitionRefusal::NotTheNamedAgent { .. })
                    ),
                    "row {}",
                    case.row
                );
            }
        }
    }

    #[test]
    fn names_a_reason_for_every_row_of_the_table_that_escalates() {
        // The reason an escalation carries comes from the gate that opened the row, and these are
        // the only gates the table pairs with `escalated`: a row added with another gate would
        // record the user as the reason, so this test is what says none does.
        let escalating: Vec<GateId> = TRANSITION_TABLE
            .iter()
            .filter(|row| row.to == Status::Is(TaskStatus::Escalated))
            .map(|row| row.gate)
            .collect();
        assert_eq!(
            escalating,
            [
                GateId::ReadinessExhausted,
                GateId::ContractRequiresHuman,
                GateId::BlockedAge,
                GateId::IterationLimitReached,
                GateId::GovernorEscalation,
                GateId::None
            ]
        );
    }

    #[test]
    fn takes_the_first_row_whose_gate_opens() {
        // A `refining` contract that has failed the Definition of Ready three times and has a
        // budget exhausted could escalate for either reason. The table's order decides, so that
        // the more specific reason is the one the user reads.
        let mut context = a_context();
        context.contract.status = TaskStatus::Refining;
        context.readiness_failed_attempts = 3;
        context.contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        context.budget.task_spent_usd = context.budget.task_max_usd;
        let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
            .expect("the move is allowed");
        assert_eq!(decision.row.gate, GateId::ReadinessExhausted);
        assert_eq!(
            decision.effects,
            [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
        );
        assert_eq!(decision.from, TaskStatus::Refining);
        assert_eq!(decision.to, TaskStatus::Escalated);
    }

    #[test]
    fn starts_a_session_on_an_assigned_task_with_no_gate_at_all() {
        let mut context = a_context();
        context.contract.status = TaskStatus::Assigned;
        let decision = decide(
            &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
            &context,
        )
        .expect("the move is allowed");
        assert_eq!(decision.row.gate, GateId::None);
        // Every move into `in_progress` clears the blocker: a task at work has none, and the
        // runtime clearing nothing costs nothing.
        assert_eq!(decision.effects, [TransitionEffect::ResetBlocker]);
        // And the rejection outcome of step 06 is what the iteration gates read, not a count of
        // their own: at the default limit a fresh task is worked rather than escalated.
        assert_eq!(
            crate::governor::escalation::evaluate_rejection(0, 3),
            RejectionOutcome::ReturnToInProgress
        );
    }
}
