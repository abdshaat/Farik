//! Iteration and escalation rules (`docs/SPEC.md` sections 5.2 and 5.7): how many rejections a
//! task may take, how many readiness failures a contract may take, how long a task may stay
//! blocked, and the record an escalation carries to the user.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::contract::TaskId;

/// Why a task is escalated (`docs/SPEC.md` section 5.7). Serialised in `snake_case`, as on the
/// wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationReason {
    /// A dollar budget ran out.
    Budget,
    /// The task's sessions ran out.
    Sessions,
    /// The rejection iteration limit was reached.
    Iterations,
    /// The task stayed blocked for the limit or longer.
    BlockerAge,
    /// A permission was denied on a required action.
    Permission,
    /// The contract's risk requires the human's acceptance.
    RiskGate,
    /// A contract awaits the user's approval: every epic (spec 5.16 item 2), and any contract the
    /// team's policy `human_accepts_contracts` sends to the human. The contract's own `high` risk
    /// is `RiskGate` instead.
    Approval,
    /// The contract failed the Definition of Ready three times (spec 5.2).
    ReadinessFailures,
    /// Integrating the accepted work failed (spec 5.14).
    Integration,
    /// The user asked for it.
    ExplicitRequest,
}

/// An escalation: the task, why, what was tried, and the options proposed to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escalation {
    /// The escalated task.
    pub task_id: TaskId,
    /// Why.
    pub reason: EscalationReason,
    /// What the agent tried before escalating.
    pub tried: String,
    /// The options the agent proposes; the user picks one or decides otherwise.
    pub options: Vec<String>,
}

/// The rejection iteration limit the schema defaults to (`max_iterations`).
pub const DEFAULT_ITERATION_LIMIT: u32 = 3;

/// How many times a contract may fail the Definition of Ready before its task escalates
/// (`docs/SPEC.md` section 5.2, `refining → escalated`).
pub const READINESS_ATTEMPT_LIMIT: u32 = 3;

/// How long a task may stay blocked before it escalates (`docs/SPEC.md` section 5.2, default
/// 24 hours).
pub const DEFAULT_BLOCKED_LIMIT: Duration = Duration::from_hours(24);

/// What follows a rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionOutcome {
    /// The task returns to `in_progress` for another iteration.
    ReturnToInProgress,
    /// The iteration limit is reached; the task escalates.
    Escalate,
}

/// Decides a rejection: `iteration` is how many times the task has already been returned to
/// `in_progress` after a rejection, and the contract's `max_iterations` is how many times that
/// may happen, so the task returns while `iteration` is below the limit and escalates once the
/// limit is reached.
#[must_use]
pub fn evaluate_rejection(iteration: u32, max_iterations: u32) -> RejectionOutcome {
    if iteration < max_iterations {
        RejectionOutcome::ReturnToInProgress
    } else {
        RejectionOutcome::Escalate
    }
}

/// What follows a failed Definition of Ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessOutcome {
    /// The Product Manager refines the contract again.
    Retry,
    /// Three attempts failed; the task escalates.
    Escalate,
}

/// Decides a failed readiness check from the number of failed attempts, this one included:
/// retry below `READINESS_ATTEMPT_LIMIT`, escalate at it.
#[must_use]
pub fn evaluate_readiness_attempts(failed_attempts: u32) -> ReadinessOutcome {
    if failed_attempts < READINESS_ATTEMPT_LIMIT {
        ReadinessOutcome::Retry
    } else {
        ReadinessOutcome::Escalate
    }
}

/// Whether a blocked task has waited too long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedAge {
    /// Blocked for less than the limit.
    WithinLimit,
    /// Blocked for the limit or longer; the task escalates.
    Exceeded,
}

/// Decides whether a task blocked at `blocked_at` has, at `now`, been blocked for `limit` or
/// longer. A `now` before `blocked_at` counts as within the limit: the pair cannot be aged, and
/// escalating on it would send every blocked task to the user the moment a clock steps backwards.
///
/// The cost is that a `blocked_at` stamped in the future keeps a task from ever escalating, because
/// unlike a `now` that is behind, a persisted `blocked_at` tells the same lie on every tick. Not
/// persisting one is the runtime's job (phase 3): it stamps `blocked_at` when the task blocks and
/// re-stamps it if its clock is corrected backwards afterwards.
#[must_use]
pub fn evaluate_blocked_age(
    blocked_at: DateTime<Utc>,
    now: DateTime<Utc>,
    limit: Duration,
) -> BlockedAge {
    let Ok(age) = (now - blocked_at).to_std() else {
        return BlockedAge::WithinLimit;
    };
    if age >= limit {
        BlockedAge::Exceeded
    } else {
        BlockedAge::WithinLimit
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use serde_json::json;

    use super::{
        BlockedAge, DEFAULT_BLOCKED_LIMIT, DEFAULT_ITERATION_LIMIT, Escalation, EscalationReason,
        READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome, evaluate_blocked_age,
        evaluate_readiness_attempts, evaluate_rejection,
    };

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().expect("an ISO 8601 UTC timestamp")
    }

    #[test]
    fn returns_a_rejected_task_while_its_iterations_are_below_the_limit() {
        // The schema's default for max_iterations and spec 5.2's "default 3": pinned as a literal,
        // because every other use here is relative to the constant and would follow it if it drifted.
        assert_eq!(DEFAULT_ITERATION_LIMIT, 3);
        assert_eq!(
            evaluate_rejection(2, 3),
            RejectionOutcome::ReturnToInProgress
        );
        assert_eq!(evaluate_rejection(3, 3), RejectionOutcome::Escalate);
        for iteration in 0..DEFAULT_ITERATION_LIMIT {
            assert_eq!(
                evaluate_rejection(iteration, DEFAULT_ITERATION_LIMIT),
                RejectionOutcome::ReturnToInProgress,
                "{iteration}"
            );
        }
    }

    #[test]
    fn escalates_a_rejected_task_once_its_iterations_reach_the_limit() {
        assert_eq!(
            evaluate_rejection(DEFAULT_ITERATION_LIMIT, DEFAULT_ITERATION_LIMIT),
            RejectionOutcome::Escalate
        );
        assert_eq!(evaluate_rejection(1, 1), RejectionOutcome::Escalate);
        assert_eq!(
            evaluate_rejection(0, 1),
            RejectionOutcome::ReturnToInProgress
        );
    }

    #[test]
    fn retries_readiness_twice_and_escalates_on_the_third_failure() {
        // The count includes the failure that has just happened, so the runtime never passes
        // zero; zero retries all the same rather than escalating on a count nothing produces.
        assert_eq!(evaluate_readiness_attempts(0), ReadinessOutcome::Retry);
        assert_eq!(evaluate_readiness_attempts(1), ReadinessOutcome::Retry);
        assert_eq!(evaluate_readiness_attempts(2), ReadinessOutcome::Retry);
        assert_eq!(
            evaluate_readiness_attempts(READINESS_ATTEMPT_LIMIT),
            ReadinessOutcome::Escalate
        );
        assert_eq!(evaluate_readiness_attempts(4), ReadinessOutcome::Escalate);
    }

    #[test]
    fn escalates_a_task_blocked_for_the_limit_or_longer() {
        let blocked_at = at("2026-09-16T10:00:00Z");
        assert_eq!(
            evaluate_blocked_age(
                blocked_at,
                at("2026-09-17T09:59:59Z"),
                DEFAULT_BLOCKED_LIMIT
            ),
            BlockedAge::WithinLimit
        );
        assert_eq!(
            evaluate_blocked_age(
                blocked_at,
                at("2026-09-17T10:00:00Z"),
                DEFAULT_BLOCKED_LIMIT
            ),
            BlockedAge::Exceeded
        );
        assert_eq!(
            evaluate_blocked_age(
                blocked_at,
                at("2026-09-16T10:30:00Z"),
                Duration::from_mins(30)
            ),
            BlockedAge::Exceeded
        );
    }

    #[test]
    fn treats_a_clock_that_went_backwards_as_within_the_limit() {
        assert_eq!(
            evaluate_blocked_age(
                at("2026-09-16T10:00:00Z"),
                at("2026-09-15T10:00:00Z"),
                Duration::ZERO
            ),
            BlockedAge::WithinLimit
        );
    }

    #[test]
    fn cannot_age_a_task_whose_block_time_has_not_arrived() {
        // A blocked_at a year ahead of now: really blocked for a month, but the clock cannot say
        // so, and the rule keeps waiting rather than escalating every blocked task at once. The
        // runtime is what must not persist this; the doc comment on the function says so.
        assert_eq!(
            evaluate_blocked_age(
                at("2027-08-16T10:00:00Z"),
                at("2026-09-16T10:00:00Z"),
                DEFAULT_BLOCKED_LIMIT
            ),
            BlockedAge::WithinLimit
        );
    }

    #[test]
    fn names_the_reasons_of_the_spec_on_the_wire() {
        // Exhaustive on purpose: an eleventh reason cannot be added to the enum without failing
        // to compile here, so the governance vocabulary cannot grow without the spec growing too.
        fn wire_name(reason: EscalationReason) -> &'static str {
            match reason {
                EscalationReason::Budget => "budget",
                EscalationReason::Sessions => "sessions",
                EscalationReason::Iterations => "iterations",
                EscalationReason::BlockerAge => "blocker_age",
                EscalationReason::Permission => "permission",
                EscalationReason::RiskGate => "risk_gate",
                EscalationReason::Approval => "approval",
                EscalationReason::ReadinessFailures => "readiness_failures",
                EscalationReason::Integration => "integration",
                EscalationReason::ExplicitRequest => "explicit_request",
            }
        }
        let reasons = [
            EscalationReason::Budget,
            EscalationReason::Sessions,
            EscalationReason::Iterations,
            EscalationReason::BlockerAge,
            EscalationReason::Permission,
            EscalationReason::RiskGate,
            EscalationReason::Approval,
            EscalationReason::ReadinessFailures,
            EscalationReason::Integration,
            EscalationReason::ExplicitRequest,
        ];
        assert_eq!(reasons.len(), 10, "spec 5.7 names ten reasons");
        for reason in reasons {
            let wire = wire_name(reason);
            assert_eq!(serde_json::to_value(reason).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<EscalationReason>(json!(wire)).unwrap(),
                reason
            );
        }
        // A reason this program does not know is refused rather than absorbed, so that the
        // governance vocabulary cannot widen behind a catch-all variant.
        for unknown in ["Budget", "blockerAge", "readiness_failure", ""] {
            assert!(
                serde_json::from_value::<EscalationReason>(json!(unknown)).is_err(),
                "{unknown}"
            );
        }
    }

    #[test]
    fn carries_the_task_the_reason_what_was_tried_and_the_options() {
        let escalation = Escalation {
            task_id: "FRK-7".parse().expect("a task id"),
            reason: EscalationReason::Budget,
            tried: "Two sessions; the second ran out of tokens.".to_string(),
            options: vec![
                "Raise the budget.".to_string(),
                "Split the task.".to_string(),
            ],
        };
        assert_eq!(escalation.task_id.to_string(), "FRK-7");
        assert_eq!(escalation.options.len(), 2);
        assert_eq!(escalation.reason, EscalationReason::Budget);
    }
}
