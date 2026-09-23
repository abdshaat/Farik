//! Budgets and limits (`docs/SPEC.md` section 5.5): the session limits per role, the session
//! ledger, and which budget is exhausted with what consequence.

use std::cmp::Ordering;
use std::time::Duration;

use crate::contract::Role;
use crate::pricing::Usage;

/// The limits of one session: tokens, wall clock, and tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLimits {
    /// Input tokens the session may consume.
    pub max_input_tokens: u64,
    /// Output tokens the session may produce.
    pub max_output_tokens: u64,
    /// How long the session may run.
    pub max_wall_clock: Duration,
    /// How many tool calls the session may make.
    pub max_tool_calls: u32,
}

/// The team default (`docs/SPEC.md` section 5.5, project plan D3): 400k input and 40k output
/// tokens, thirty minutes, two hundred tool calls.
pub const DEFAULT_SESSION_LIMITS: SessionLimits = SessionLimits {
    max_input_tokens: 400_000,
    max_output_tokens: 40_000,
    max_wall_clock: Duration::from_mins(30),
    max_tool_calls: 200,
};

/// The session limits a role gets by default: the Scrum Master half the tokens of the team
/// default, every other role the team default.
#[must_use]
pub fn default_session_limits(role: Role) -> SessionLimits {
    match role {
        Role::ScrumMaster => SessionLimits {
            max_input_tokens: 200_000,
            max_output_tokens: 20_000,
            ..DEFAULT_SESSION_LIMITS
        },
        _ => DEFAULT_SESSION_LIMITS,
    }
}

/// What one session has consumed so far.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SessionLedger {
    /// Tokens, summed over the session's model calls.
    pub usage: Usage,
    /// How long the session has run.
    pub wall_clock: Duration,
    /// How many tool calls the session has made.
    pub tool_calls: u32,
    /// What the session has cost, in dollars.
    pub cost_usd: f64,
}

/// The ledger after one more model call: tokens and cost added, wall clock and tool calls
/// unchanged, because the runtime keeps those itself. Token counts saturate rather than wrap,
/// so that a ledger never reads as an empty session.
#[must_use]
pub fn add_usage(ledger: &SessionLedger, usage: &Usage, cost_usd: f64) -> SessionLedger {
    SessionLedger {
        usage: Usage {
            input_tokens: ledger.usage.input_tokens.saturating_add(usage.input_tokens),
            output_tokens: ledger
                .usage
                .output_tokens
                .saturating_add(usage.output_tokens),
            cache_read_tokens: ledger
                .usage
                .cache_read_tokens
                .saturating_add(usage.cache_read_tokens),
            cache_write_tokens: ledger
                .usage
                .cache_write_tokens
                .saturating_add(usage.cache_write_tokens),
        },
        wall_clock: ledger.wall_clock,
        tool_calls: ledger.tool_calls,
        cost_usd: ledger.cost_usd + cost_usd,
    }
}

/// A budget or limit of `docs/SPEC.md` section 5.5, in the order they are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BudgetScope {
    /// The session's input or output tokens.
    SessionTokens,
    /// The session's wall clock.
    SessionWallClock,
    /// The session's tool calls.
    SessionToolCalls,
    /// The task's dollars, `max_cost_usd` in the contract.
    TaskUsd,
    /// The task's sessions, `max_sessions` in the contract.
    TaskSessions,
    /// The sprint's dollars.
    SprintUsd,
    /// The team's dollars for the day.
    DayUsd,
}

/// What happens when a budget is exhausted (`docs/SPEC.md` section 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetConsequence {
    /// The session ends and the task goes to `blocked` with a note; the next session resumes
    /// from the note.
    EndSessionAndBlockTask,
    /// The task goes to `escalated`.
    EscalateTask,
    /// No new assignments; tasks in progress may finish.
    StopNewAssignments,
    /// Everything pauses and the user is notified.
    PauseTeam,
}

/// Everything the budget check needs, gathered by the runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetState {
    /// The current session's ledger.
    pub session: SessionLedger,
    /// The current session's limits.
    pub session_limits: SessionLimits,
    /// What the task has cost so far, in dollars.
    pub task_spent_usd: f64,
    /// The contract's `max_cost_usd`.
    pub task_max_usd: f64,
    /// How many sessions the task has had.
    pub task_sessions: u32,
    /// The contract's `max_sessions`.
    pub task_max_sessions: u32,
    /// What the sprint has cost so far, in dollars.
    pub sprint_spent_usd: f64,
    /// The sprint's budget.
    pub sprint_max_usd: f64,
    /// What the team has spent today, in dollars.
    pub day_spent_usd: f64,
    /// The team's daily budget.
    pub day_max_usd: f64,
}

/// A budget that is exhausted, and what follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted {
    /// The exhausted budget.
    pub scope: BudgetScope,
    /// What the runtime does about it.
    pub consequence: BudgetConsequence,
}

/// Every exhausted budget, in the order of `BudgetScope`, and the empty list while every budget
/// has room. A budget is exhausted when what was spent reaches its limit, so that a session ends
/// at its last token rather than one past it and a task with `max_sessions` sessions gets no
/// more, and a spend that is not a number counts as exhausted, because a broken figure is no
/// licence to keep spending. Every one is reported, because the consequences act on different
/// things and none subsumes another: a sprint that stops new assignments does not end the
/// session that is already running, and the shipped sprint budget is smaller than the shipped
/// daily one, so reporting only the first would mean the team never pauses.
#[must_use]
pub fn check_budgets(state: &BudgetState) -> Vec<Exhausted> {
    let session = &state.session;
    let limits = &state.session_limits;
    let checks = [
        (
            BudgetScope::SessionTokens,
            BudgetConsequence::EndSessionAndBlockTask,
            session.usage.input_tokens >= limits.max_input_tokens
                || session.usage.output_tokens >= limits.max_output_tokens,
        ),
        (
            BudgetScope::SessionWallClock,
            BudgetConsequence::EndSessionAndBlockTask,
            session.wall_clock >= limits.max_wall_clock,
        ),
        (
            BudgetScope::SessionToolCalls,
            BudgetConsequence::EndSessionAndBlockTask,
            session.tool_calls >= limits.max_tool_calls,
        ),
        (
            BudgetScope::TaskUsd,
            BudgetConsequence::EscalateTask,
            has_reached(state.task_spent_usd, state.task_max_usd),
        ),
        (
            BudgetScope::TaskSessions,
            BudgetConsequence::EscalateTask,
            state.task_sessions >= state.task_max_sessions,
        ),
        (
            BudgetScope::SprintUsd,
            BudgetConsequence::StopNewAssignments,
            has_reached(state.sprint_spent_usd, state.sprint_max_usd),
        ),
        (
            BudgetScope::DayUsd,
            BudgetConsequence::PauseTeam,
            has_reached(state.day_spent_usd, state.day_max_usd),
        ),
    ];
    checks
        .into_iter()
        .filter(|(_, _, exhausted)| *exhausted)
        .map(|(scope, consequence, _)| Exhausted { scope, consequence })
        .collect()
}

/// Whether a spend has reached its limit. A spend that is not a number has, because a figure
/// that cannot be compared is no licence to keep spending.
fn has_reached(spent: f64, limit: f64) -> bool {
    !matches!(spent.partial_cmp(&limit), Some(Ordering::Less))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        BudgetConsequence as C, BudgetScope as B, BudgetState, DEFAULT_SESSION_LIMITS, Exhausted,
        SessionLedger, SessionLimits, add_usage, check_budgets, default_session_limits,
    };
    use crate::contract::Role;
    use crate::pricing::Usage;

    fn a_state() -> BudgetState {
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

    fn exhausted(scope: B, consequence: C) -> Exhausted {
        Exhausted { scope, consequence }
    }

    #[test]
    fn gives_the_scrum_master_half_the_tokens_and_every_other_role_the_team_default() {
        assert_eq!(
            default_session_limits(Role::ScrumMaster),
            SessionLimits {
                max_input_tokens: 200_000,
                max_output_tokens: 20_000,
                max_wall_clock: Duration::from_mins(30),
                max_tool_calls: 200,
            }
        );
        for role in [
            Role::ProductManager,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
            Role::Human,
        ] {
            assert_eq!(
                default_session_limits(role),
                DEFAULT_SESSION_LIMITS,
                "{role}"
            );
        }
        assert_eq!(DEFAULT_SESSION_LIMITS.max_input_tokens, 400_000);
        assert_eq!(DEFAULT_SESSION_LIMITS.max_output_tokens, 40_000);
    }

    #[test]
    fn adds_tokens_and_cost_to_the_ledger_and_leaves_the_clock_and_calls_alone() {
        let ledger = SessionLedger {
            usage: Usage {
                input_tokens: 10,
                output_tokens: 1,
                cache_read_tokens: 5,
                cache_write_tokens: 2,
            },
            wall_clock: Duration::from_secs(60),
            tool_calls: 3,
            cost_usd: 0.5,
        };
        let more = Usage {
            input_tokens: 100,
            output_tokens: 10,
            cache_read_tokens: 50,
            cache_write_tokens: 20,
        };
        assert_eq!(
            add_usage(&ledger, &more, 0.25),
            SessionLedger {
                usage: Usage {
                    input_tokens: 110,
                    output_tokens: 11,
                    cache_read_tokens: 55,
                    cache_write_tokens: 22,
                },
                wall_clock: Duration::from_secs(60),
                tool_calls: 3,
                cost_usd: 0.75,
            }
        );
    }

    #[test]
    fn finds_nothing_exhausted_while_every_budget_has_room() {
        assert_eq!(check_budgets(&a_state()), []);
    }

    #[test]
    fn ends_the_session_and_blocks_the_task_when_its_tokens_run_out() {
        let mut state = a_state();
        state.session.usage.input_tokens = 400_000;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::SessionTokens, C::EndSessionAndBlockTask)]
        );
        let mut state = a_state();
        state.session.usage.input_tokens = 399_999;
        assert_eq!(check_budgets(&state), []);
        let mut state = a_state();
        state.session.usage.output_tokens = 40_000;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::SessionTokens, C::EndSessionAndBlockTask)]
        );
        state.session.usage.output_tokens = 39_999;
        assert_eq!(check_budgets(&state), []);
    }

    #[test]
    fn ends_the_session_and_blocks_the_task_when_its_wall_clock_runs_out() {
        let mut state = a_state();
        state.session.wall_clock = Duration::from_mins(30);
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::SessionWallClock, C::EndSessionAndBlockTask)]
        );
    }

    #[test]
    fn ends_the_session_and_blocks_the_task_when_its_tool_calls_run_out() {
        let mut state = a_state();
        state.session.tool_calls = 200;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::SessionToolCalls, C::EndSessionAndBlockTask)]
        );
    }

    #[test]
    fn escalates_the_task_when_its_dollars_run_out() {
        let mut state = a_state();
        state.task_spent_usd = 5.0;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::TaskUsd, C::EscalateTask)]
        );
    }

    #[test]
    fn escalates_the_task_when_its_sessions_run_out() {
        let mut state = a_state();
        state.task_sessions = 5;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::TaskSessions, C::EscalateTask)]
        );
    }

    #[test]
    fn stops_new_assignments_when_the_sprint_runs_out() {
        let mut state = a_state();
        state.sprint_spent_usd = 15.0;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::SprintUsd, C::StopNewAssignments)]
        );
    }

    #[test]
    fn pauses_the_team_when_the_day_runs_out() {
        let mut state = a_state();
        state.day_spent_usd = 20.0;
        assert_eq!(check_budgets(&state), [exhausted(B::DayUsd, C::PauseTeam)]);
    }

    #[test]
    fn treats_a_spend_that_is_not_a_number_as_exhausted() {
        let mut state = a_state();
        state.task_spent_usd = f64::NAN;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::TaskUsd, C::EscalateTask)]
        );
        let mut state = a_state();
        state.day_spent_usd = f64::INFINITY;
        assert_eq!(check_budgets(&state), [exhausted(B::DayUsd, C::PauseTeam)]);
    }

    #[test]
    fn exhausts_a_budget_of_nothing_before_anything_is_spent() {
        let mut state = a_state();
        state.task_spent_usd = 0.0;
        state.task_max_usd = 0.0;
        assert_eq!(
            check_budgets(&state),
            [exhausted(B::TaskUsd, C::EscalateTask)]
        );
    }

    #[test]
    fn reports_every_exhausted_budget_in_scope_order() {
        let mut state = a_state();
        state.session.tool_calls = 200;
        state.task_spent_usd = 5.0;
        state.sprint_spent_usd = 15.0;
        state.day_spent_usd = 20.0;
        assert_eq!(
            check_budgets(&state),
            [
                exhausted(B::SessionToolCalls, C::EndSessionAndBlockTask),
                exhausted(B::TaskUsd, C::EscalateTask),
                exhausted(B::SprintUsd, C::StopNewAssignments),
                exhausted(B::DayUsd, C::PauseTeam),
            ]
        );
    }

    #[test]
    fn pauses_the_team_even_though_the_sprint_ran_out_first() {
        let mut state = a_state();
        state.sprint_spent_usd = 15.0;
        state.day_spent_usd = 100.0;
        assert_eq!(
            check_budgets(&state),
            [
                exhausted(B::SprintUsd, C::StopNewAssignments),
                exhausted(B::DayUsd, C::PauseTeam),
            ]
        );
    }

    #[test]
    fn keeps_a_ledger_that_cannot_wrap_around() {
        let ledger = SessionLedger {
            usage: Usage {
                input_tokens: u64::MAX,
                output_tokens: u64::MAX,
                cache_read_tokens: u64::MAX,
                cache_write_tokens: u64::MAX,
            },
            ..SessionLedger::default()
        };
        let more = Usage {
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: 1,
            cache_write_tokens: 1,
        };
        assert_eq!(add_usage(&ledger, &more, 0.0).usage, ledger.usage);
    }
}
