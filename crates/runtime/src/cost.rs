//! What a session consumed, as money in the log, and what each budget of `docs/SPEC.md` 5.5 has
//! left.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::budget::{
    BudgetConsequence, BudgetScope, BudgetState, Exhausted, SessionLedger, check_budgets,
    default_session_limits,
};
use farik_core::contract::{Role, TaskContract};
use farik_core::pricing::{PriceTable, Usage, compute_cost_usd};
use farik_core::team::Team;
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    BudgetExhaustedBody, BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope,
    CostRecordedBody, CostRecordedBodyModelId, CostRecordedBodyPurpose, EventBody, EventIds,
    TokenUsage, new_event,
};
use farik_store::{CostProjection, CostScope, EventLog, Projections, StoreError};

use crate::session::SessionPurpose;

/// Why a cost or an exhausted budget was not recorded, or a budget could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    /// The log or its projections refused.
    Store {
        /// What the store said.
        detail: String,
    },
    /// The usage could not be priced: the table has no row for its model.
    Pricing {
        /// Which model, and why.
        detail: String,
    },
    /// The event could not be stamped: a blank id, or a cost that names no session or no agent.
    Event {
        /// What is missing.
        detail: String,
    },
}

impl fmt::Display for CostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store { detail } => write!(formatter, "the store refused: {detail}"),
            Self::Pricing { detail } => write!(formatter, "the usage cannot be priced: {detail}"),
            Self::Event { detail } => write!(formatter, "the event cannot be recorded: {detail}"),
        }
    }
}

impl std::error::Error for CostError {}

impl From<StoreError> for CostError {
    fn from(error: StoreError) -> Self {
        Self::Store {
            detail: error.to_string(),
        }
    }
}

/// Where a usage report came from: the ids its event is stamped with, which must name the session
/// and the agent and may name the task, why the session ran, and the model that was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostSource<'a> {
    /// The session, agent, and task the cost belongs to, with the team and the project.
    pub ids: EventIds,
    /// Why the session ran.
    pub purpose: SessionPurpose,
    /// The model the usage was reported for, as the price table names it.
    pub model_id: &'a str,
}

/// Prices one usage report, appends it to the log as a `cost.recorded`, and projects it. Returns
/// the dollars charged, which the event carries so that a later change to the prices does not
/// rewrite what was charged.
///
/// # Errors
///
/// `Event` when the source names no session or no agent, since every sum by agent and every count
/// of sessions would lose the cost, or when `new_event` refuses a blank id; `Pricing` when the
/// table has no row for the model, because a cost of zero would under-report spend; `Store` when
/// the append or the projection fails. Nothing is recorded on any of them.
pub fn record_session_cost(
    log: &EventLog,
    projections: &Projections,
    source: &CostSource<'_>,
    usage: &Usage,
    prices: &PriceTable,
    clock: &dyn Clock,
) -> Result<f64, CostError> {
    let cost_usd =
        compute_cost_usd(usage, source.model_id, prices).map_err(|_| CostError::Pricing {
            detail: format!("the price table has no row for {}", source.model_id),
        })?;
    let body = CostRecordedBody {
        purpose: purpose_wire(source.purpose),
        model_id: source
            .model_id
            .parse::<CostRecordedBodyModelId>()
            .map_err(|error| CostError::Event {
                detail: format!("the model id is not one: {error}"),
            })?,
        usage: TokenUsage {
            input_tokens: tokens(usage.input_tokens)?,
            output_tokens: tokens(usage.output_tokens)?,
            cache_read_tokens: tokens(usage.cache_read_tokens)?,
            cache_write_tokens: tokens(usage.cache_write_tokens)?,
        },
        cost_usd,
    };
    let event = stamp(EventBody::CostRecorded(body), clock, &source.ids)?;
    // Checked on what `new_event` settled, so that a blank id counts as none.
    for (id, field) in [
        (&event.ids.session_id, "session_id"),
        (&event.ids.agent_id, "agent_id"),
    ] {
        if id.is_none() {
            return Err(CostError::Event {
                detail: format!("a cost names no {field}, and its sums would lose it"),
            });
        }
    }
    let appended = log.append(&event)?;
    projections.apply(&appended)?;
    Ok(cost_usd)
}

/// Everything `check_budgets` needs for one session, read from the projections at `now`.
///
/// The session limits are the role's defaults with each field `team.budgets.session` sets put in
/// its place. The task's come from its contract, and a session with no task is bounded by none. The
/// day is the UTC date of `now`. The sprint's is unbounded until sprints exist.
///
/// # Errors
///
/// `Store` when the costs cannot be read.
pub fn budget_state(
    projections: &Projections,
    team: &Team,
    role: Role,
    task: Option<&TaskContract>,
    session: &SessionLedger,
    now: DateTime<Utc>,
) -> Result<BudgetState, CostError> {
    let mut session_limits = default_session_limits(role);
    if let Some(set) = &team.budgets.session {
        if let Some(max) = set.max_input_tokens {
            session_limits.max_input_tokens = max.get();
        }
        if let Some(max) = set.max_output_tokens {
            session_limits.max_output_tokens = max.get();
        }
        if let Some(max) = set.max_wall_clock_seconds {
            session_limits.max_wall_clock = Duration::from_secs(max.get());
        }
        if let Some(max) = set.max_tool_calls {
            session_limits.max_tool_calls = u32::try_from(max.get()).unwrap_or(u32::MAX);
        }
    }
    let (task_spent_usd, task_sessions, task_max_usd, task_max_sessions) = match task {
        None => (0.0, 0, f64::INFINITY, u32::MAX),
        Some(contract) => {
            let spent = spent_by(projections, CostScope::Task, &contract.id.to_string())?;
            (
                spent.as_ref().map_or(0.0, |row| row.usd),
                spent.as_ref().map_or(0, |row| row.sessions),
                contract.budget.max_cost_usd,
                u32::try_from(contract.budget.max_sessions.get()).unwrap_or(u32::MAX),
            )
        }
    };
    let day = spent_by(projections, CostScope::Day, &now.date_naive().to_string())?;
    Ok(BudgetState {
        session: *session,
        session_limits,
        task_spent_usd,
        task_max_usd,
        task_sessions,
        task_max_sessions,
        sprint_spent_usd: 0.0,
        sprint_max_usd: f64::INFINITY,
        day_spent_usd: day.map_or(0.0, |row| row.usd),
        day_max_usd: team.budgets.daily_usd,
    })
}

/// Records a `budget.exhausted` for every budget exhausted in `after` that was not in `before`,
/// stamped with `ids` as given, and returns what it recorded. A budget that stays exhausted is
/// recorded once, when it crossed.
///
/// # Errors
///
/// `Event` when `new_event` refuses a blank id; `Store` when an append or a projection fails. The
/// ones recorded before a failure stay recorded.
pub fn record_exhaustion(
    log: &EventLog,
    projections: &Projections,
    before: &BudgetState,
    after: &BudgetState,
    ids: &EventIds,
    clock: &dyn Clock,
) -> Result<Vec<Exhausted>, CostError> {
    let was = check_budgets(before);
    let crossed: Vec<Exhausted> = check_budgets(after)
        .into_iter()
        .filter(|now| !was.iter().any(|then| then.scope == now.scope))
        .collect();
    for exhausted in &crossed {
        let body = BudgetExhaustedBody {
            scope: scope_wire(exhausted.scope),
            consequence: consequence_wire(exhausted.consequence),
        };
        let appended = log.append(&stamp(EventBody::BudgetExhausted(body), clock, ids)?)?;
        projections.apply(&appended)?;
    }
    Ok(crossed)
}

fn stamp(
    body: EventBody,
    clock: &dyn Clock,
    ids: &EventIds,
) -> Result<farik_protocol::event::NewEvent, CostError> {
    new_event(body, clock.now(), ids.clone()).map_err(|error| CostError::Event {
        detail: format!("{error:?}"),
    })
}

fn spent_by(
    projections: &Projections,
    scope: CostScope,
    key: &str,
) -> Result<Option<CostProjection>, CostError> {
    // ponytail: reads every key of the scope to find one; a keyed query if a project's keys grow
    // past what a board shows.
    Ok(projections
        .costs(scope)?
        .into_iter()
        .find(|row| row.key == key))
}

/// A token count as the wire's integer. The schema bounds it by 2^53, so a count past `i64` is
/// refused here rather than wrapped.
fn tokens(count: u64) -> Result<i64, CostError> {
    i64::try_from(count).map_err(|_| CostError::Event {
        detail: format!("{count} tokens is past what the log can hold"),
    })
}

fn purpose_wire(purpose: SessionPurpose) -> CostRecordedBodyPurpose {
    match purpose {
        SessionPurpose::Triage => CostRecordedBodyPurpose::Triage,
        SessionPurpose::Refine => CostRecordedBodyPurpose::Refine,
        SessionPurpose::Plan => CostRecordedBodyPurpose::Plan,
        SessionPurpose::Implement => CostRecordedBodyPurpose::Implement,
        SessionPurpose::Verify => CostRecordedBodyPurpose::Verify,
        SessionPurpose::Ceremony => CostRecordedBodyPurpose::Ceremony,
        SessionPurpose::Conversation => CostRecordedBodyPurpose::Conversation,
    }
}

fn scope_wire(scope: BudgetScope) -> BudgetExhaustedBodyScope {
    match scope {
        BudgetScope::SessionTokens => BudgetExhaustedBodyScope::SessionTokens,
        BudgetScope::SessionWallClock => BudgetExhaustedBodyScope::SessionWallClock,
        BudgetScope::SessionToolCalls => BudgetExhaustedBodyScope::SessionToolCalls,
        BudgetScope::TaskUsd => BudgetExhaustedBodyScope::TaskUsd,
        BudgetScope::TaskSessions => BudgetExhaustedBodyScope::TaskSessions,
        BudgetScope::SprintUsd => BudgetExhaustedBodyScope::SprintUsd,
        BudgetScope::DayUsd => BudgetExhaustedBodyScope::DayUsd,
    }
}

fn consequence_wire(consequence: BudgetConsequence) -> BudgetExhaustedBodyConsequence {
    match consequence {
        BudgetConsequence::EndSessionAndBlockTask => {
            BudgetExhaustedBodyConsequence::EndSessionAndBlockTask
        }
        BudgetConsequence::EscalateTask => BudgetExhaustedBodyConsequence::EscalateTask,
        BudgetConsequence::StopNewAssignments => BudgetExhaustedBodyConsequence::StopNewAssignments,
        BudgetConsequence::PauseTeam => BudgetExhaustedBodyConsequence::PauseTeam,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use farik_core::budget::{
        BudgetConsequence, BudgetScope, BudgetState, DEFAULT_SESSION_LIMITS, Exhausted,
        SessionLedger, default_session_limits,
    };
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{Role, TaskContract, validate_contract};
    use farik_core::pricing::{PriceTable, Usage, validate_price_table};
    use farik_core::team::fixtures::a_team_wire;
    use farik_core::team::{Team, validate_team};
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::fixtures::a_new_event;
    use farik_protocol::event::{
        BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope, CostRecordedBodyPurpose,
        EventBody, EventIds, EventKind, FarikEvent,
    };
    use farik_store::{
        EventLog, EventQuery, IN_MEMORY, Projections, open_event_log, open_projections,
    };
    use serde_json::json;

    use super::{CostError, CostSource, budget_state, record_exhaustion, record_session_cost};
    use crate::session::SessionPurpose;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().expect("a fixed timestamp")
    }

    fn clock() -> FixedClock {
        FixedClock::new(at("2026-09-22T10:00:00Z"))
    }

    fn a_board() -> (Arc<EventLog>, Projections) {
        let log = Arc::new(
            open_event_log(std::path::Path::new(IN_MEMORY), at("2026-09-22T09:00:00Z"))
                .expect("a log in memory opens"),
        );
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        (log, projections)
    }

    /// One model at 1, 2, 0.5, and 0.25 dollars per million input, output, cache read, and cache
    /// write tokens.
    fn prices() -> PriceTable {
        validate_price_table(&json!({
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
        }))
        .expect("a price table")
    }

    fn ids(task: Option<&str>, session: &str) -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            task_id: task.map(|id| id.parse().expect("a task id")),
            agent_id: Some("linus".to_string()),
            session_id: Some(session.to_string()),
        }
    }

    fn source(ids: EventIds) -> CostSource<'static> {
        CostSource {
            ids,
            purpose: SessionPurpose::Implement,
            model_id: "test-model",
        }
    }

    fn usage(input_tokens: u64, output_tokens: u64) -> Usage {
        Usage {
            input_tokens,
            output_tokens,
            ..Usage::default()
        }
    }

    fn filed(log: &EventLog, projections: &Projections, task_id: &str) {
        let mut event = a_new_event(EventKind::TaskCreated);
        event.ids.task_id = Some(task_id.parse().expect("a task id"));
        let appended = log.append(&event).expect("appends");
        projections.apply(&appended).expect("projects");
    }

    fn everything(log: &EventLog) -> Vec<FarikEvent> {
        log.read(&EventQuery::default()).expect("the log reads")
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn records_one_cost_priced_from_the_table() {
        let (log, projections) = a_board();
        filed(&log, &projections, "FRK-1");
        let charged = record_session_cost(
            &log,
            &projections,
            &source(ids(Some("FRK-1"), "s1")),
            &usage(1_000_000, 500_000),
            &prices(),
            &clock(),
        )
        .expect("recorded");
        assert!(close(charged, 2.0), "{charged}");
        let events = everything(&log);
        assert_eq!(events.len(), 2);
        let recorded = &events[1];
        let EventBody::CostRecorded(body) = &recorded.body else {
            panic!("a cost.recorded, not {:?}", recorded.body.kind());
        };
        assert!(close(body.cost_usd, 2.0));
        assert_eq!(body.purpose, CostRecordedBodyPurpose::Implement);
        assert_eq!(recorded.envelope.ids, ids(Some("FRK-1"), "s1"));
        let task = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert!(close(task.cost_usd, 2.0));
    }

    #[test]
    fn refuses_a_cost_without_a_session_or_an_agent() {
        let (log, projections) = a_board();
        for ids in [
            EventIds {
                session_id: None,
                ..ids(None, "s1")
            },
            EventIds {
                agent_id: None,
                ..ids(None, "s1")
            },
        ] {
            let refused = record_session_cost(
                &log,
                &projections,
                &source(ids),
                &usage(1, 1),
                &prices(),
                &clock(),
            );
            assert!(
                matches!(refused, Err(CostError::Event { .. })),
                "{refused:?}"
            );
        }
        assert_eq!(everything(&log).len(), 0);
    }

    #[test]
    fn refuses_a_model_the_table_does_not_price() {
        // A cost of zero for an unknown model would under-report spend, which is what 5.5 exists
        // to prevent.
        let (log, projections) = a_board();
        let refused = record_session_cost(
            &log,
            &projections,
            &CostSource {
                model_id: "claude-unknown-9",
                ..source(ids(None, "s1"))
            },
            &usage(1, 1),
            &prices(),
            &clock(),
        );
        let Err(CostError::Pricing { detail }) = refused else {
            panic!("expected a pricing refusal, got {refused:?}");
        };
        assert!(detail.contains("claude-unknown-9"), "{detail}");
        assert_eq!(everything(&log).len(), 0);
    }

    fn a_team(session: Option<serde_json::Value>) -> Team {
        let mut wire = a_team_wire();
        if let Some(session) = session {
            wire["budgets"]["session"] = session;
        }
        validate_team(&wire).expect("a team")
    }

    fn a_contract(id: &str, max_cost_usd: f64, max_sessions: u64) -> TaskContract {
        let mut wire = a_contract_wire();
        wire["id"] = json!(id);
        wire["budget"] = json!({ "max_cost_usd": max_cost_usd, "max_sessions": max_sessions });
        validate_contract(&wire).expect("a contract")
    }

    fn state(
        projections: &Projections,
        team: &Team,
        role: Role,
        task: Option<&TaskContract>,
    ) -> BudgetState {
        budget_state(
            projections,
            team,
            role,
            task,
            &SessionLedger::default(),
            clock().at,
        )
        .expect("the state reads")
    }

    #[test]
    fn reads_budgets_from_the_team_the_contract_and_the_day() {
        let (log, projections) = a_board();
        filed(&log, &projections, "FRK-1");
        filed(&log, &projections, "FRK-2");
        let yesterday = FixedClock::new(at("2026-09-21T10:00:00Z"));
        // A million input tokens is a dollar at these prices.
        for (task, session, millions, when) in [
            ("FRK-1", "a", 1, clock()),
            ("FRK-1", "b", 3, clock()),
            ("FRK-2", "c", 1, yesterday),
        ] {
            record_session_cost(
                &log,
                &projections,
                &source(ids(Some(task), session)),
                &usage(millions * 1_000_000, 0),
                &prices(),
                &when,
            )
            .expect("recorded");
        }
        let contract = a_contract("FRK-1", 5.0, 3);
        let read = state(
            &projections,
            &a_team(None),
            Role::SoftwareDeveloper,
            Some(&contract),
        );
        assert!(close(read.task_spent_usd, 4.0), "{}", read.task_spent_usd);
        assert!(close(read.task_max_usd, 5.0));
        assert_eq!(read.task_sessions, 2);
        assert_eq!(read.task_max_sessions, 3);
        assert!(close(read.day_spent_usd, 4.0), "{}", read.day_spent_usd);
        assert!(close(read.day_max_usd, 20.0));
        assert!(read.sprint_max_usd.is_infinite() && read.sprint_max_usd > 0.0);
        assert!(close(read.sprint_spent_usd, 0.0));
    }

    #[test]
    fn uses_the_roles_default_session_limits_when_the_team_sets_none() {
        let (_log, projections) = a_board();
        let read = state(&projections, &a_team(None), Role::ScrumMaster, None);
        assert_eq!(
            read.session_limits,
            default_session_limits(Role::ScrumMaster)
        );
    }

    #[test]
    fn overrides_only_the_session_limits_the_team_sets() {
        let (_log, projections) = a_board();
        let team = a_team(Some(json!({ "max_input_tokens": 100_000 })));
        let read = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert_eq!(
            read.session_limits,
            farik_core::budget::SessionLimits {
                max_input_tokens: 100_000,
                ..DEFAULT_SESSION_LIMITS
            }
        );
        // The Scrum Master's own default stays where the team sets nothing.
        let read = state(&projections, &team, Role::ScrumMaster, None);
        assert_eq!(read.session_limits.max_input_tokens, 100_000);
        assert_eq!(read.session_limits.max_output_tokens, 20_000);
        let team = a_team(Some(json!({ "max_output_tokens": 9_000 })));
        let read = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert_eq!(read.session_limits.max_output_tokens, 9_000);
        let team = a_team(Some(
            json!({ "max_wall_clock_seconds": 60, "max_tool_calls": 7 }),
        ));
        let read = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert_eq!(read.session_limits.max_wall_clock, Duration::from_mins(1));
        assert_eq!(read.session_limits.max_tool_calls, 7);
    }

    #[test]
    fn saturates_a_limit_past_what_a_count_of_sessions_or_tool_calls_holds() {
        let (_log, projections) = a_board();
        let past_u32 = u64::from(u32::MAX) + 1;
        let team = a_team(Some(json!({ "max_tool_calls": past_u32 })));
        let contract = a_contract("FRK-1", 5.0, past_u32);
        let read = state(
            &projections,
            &team,
            Role::SoftwareDeveloper,
            Some(&contract),
        );
        assert_eq!(read.session_limits.max_tool_calls, u32::MAX);
        assert_eq!(read.task_max_sessions, u32::MAX);
    }

    #[test]
    fn counts_nothing_spent_today_when_the_only_costs_are_yesterdays() {
        let (log, projections) = a_board();
        let yesterday = FixedClock::new(at("2026-09-21T10:00:00Z"));
        record_session_cost(
            &log,
            &projections,
            &source(ids(None, "a")),
            &usage(1_000_000, 0),
            &prices(),
            &yesterday,
        )
        .expect("recorded");
        let read = state(&projections, &a_team(None), Role::SoftwareDeveloper, None);
        assert!(close(read.day_spent_usd, 0.0), "{}", read.day_spent_usd);
    }

    #[test]
    fn leaves_a_session_without_a_task_unbounded_by_task_budgets() {
        let (_log, projections) = a_board();
        let read = state(&projections, &a_team(None), Role::SoftwareDeveloper, None);
        assert!(read.task_max_usd.is_infinite() && read.task_max_usd > 0.0);
        assert_eq!(read.task_max_sessions, u32::MAX);
    }

    #[test]
    fn records_an_exhausted_budget_once_when_it_is_crossed() {
        let (log, projections) = a_board();
        let team = a_team(None);
        let before = BudgetState {
            day_spent_usd: 19.0,
            ..state(&projections, &team, Role::SoftwareDeveloper, None)
        };
        let after = BudgetState {
            day_spent_usd: 21.0,
            ..before
        };
        let crossed = ids(None, "s1");
        let recorded = record_exhaustion(&log, &projections, &before, &after, &crossed, &clock())
            .expect("recorded");
        assert_eq!(
            recorded,
            vec![Exhausted {
                scope: BudgetScope::DayUsd,
                consequence: BudgetConsequence::PauseTeam,
            }]
        );
        let events = everything(&log);
        assert_eq!(events.len(), 1);
        let EventBody::BudgetExhausted(body) = &events[0].body else {
            panic!("a budget.exhausted, not {:?}", events[0].body.kind());
        };
        assert_eq!(body.scope, BudgetExhaustedBodyScope::DayUsd);
        assert_eq!(body.consequence, BudgetExhaustedBodyConsequence::PauseTeam);
        assert_eq!(events[0].envelope.ids, crossed);

        let still = BudgetState {
            day_spent_usd: 22.0,
            ..after
        };
        let again = record_exhaustion(&log, &projections, &after, &still, &crossed, &clock())
            .expect("nothing to record");
        assert_eq!(again, Vec::new());
        assert_eq!(everything(&log).len(), 1);
    }
}
