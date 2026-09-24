//! What a session consumed, as money in the log, and what each budget of `docs/SPEC.md` 5.5 has
//! left.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::budget::{
    BudgetConsequence, BudgetScope, BudgetState, Exhausted, SessionLedger, check_budgets,
    default_session_limits,
};
use farik_core::contract::{Role, TaskContract};
use farik_core::pricing::{PriceTable, PricingError, Usage, compute_cost_usd};
use farik_core::team::Team;
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    BudgetExhaustedBody, BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope,
    CostRecordedBody, CostRecordedBodyModelId, CostRecordedBodyPurpose, EventBody, EventIds,
    TokenUsage, new_event,
};
use farik_roles::{RoleError, load_role};
use farik_store::{CostProjection, CostScope, EventLog, Projections, StoreError};

use crate::channel::{ChannelError, post_system};
use crate::session::{SessionPurpose, TRIAGE_MODEL, session_model};

/// Why a cost or an exhausted budget was not recorded, or a budget could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    /// The log or its projections refused.
    Store {
        /// What the store said.
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
            Self::Event { detail } => write!(formatter, "the event cannot be recorded: {detail}"),
        }
    }
}

impl std::error::Error for CostError {}

impl From<ChannelError> for CostError {
    fn from(error: ChannelError) -> Self {
        match error {
            ChannelError::Store(error) => error.into(),
            ChannelError::Refused { reason } => Self::Event { detail: reason },
        }
    }
}

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
/// rewrite what was charged. A model the table has no row for is recorded with its tokens, a cost
/// of zero, and `unpriced`, and no dollar limit counts it (ADR 0015).
///
/// # Errors
///
/// `Event` when the source names no session or no agent, since every sum by agent and every count
/// of sessions would lose the cost, or when `new_event` refuses a blank id; `Store` when the append
/// or the projection fails. Nothing is recorded on any of them.
pub fn record_session_cost(
    log: &EventLog,
    projections: &Projections,
    source: &CostSource<'_>,
    usage: &Usage,
    prices: &PriceTable,
    clock: &dyn Clock,
) -> Result<f64, CostError> {
    // The one refusal is a model the table has no row for, which is recorded at no cost rather
    // than refused, so that an agent may run on any model (ADR 0015).
    let (cost_usd, unpriced) = match compute_cost_usd(usage, source.model_id, prices) {
        Ok(cost_usd) => (cost_usd, false),
        Err(PricingError::UnknownModel { .. }) => (0.0, true),
    };
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
        unpriced,
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

/// Each model an active agent's sessions run on that `prices` has no row for, with the ids of the
/// active agents that use it, in team order and without repeats.
///
/// An agent's model is its own `model.id` when it has one and otherwise its role's default
/// (`session_model`); an active Product Manager also uses `TRIAGE_MODEL`, which its triage
/// sessions run on (5.16). An agent with no model of its own whose role Farik does not ship is
/// passed over, since nothing could load its role to start a session of it.
///
/// # Errors
///
/// `RoleError::Invalid` when a shipped role's files do not describe a role.
pub fn unpriced_models(
    team: &Team,
    prices: &PriceTable,
) -> Result<BTreeMap<String, Vec<String>>, RoleError> {
    let mut unpriced: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for agent in team.active_agents() {
        let role = Role::from(agent.role);
        let mut models = Vec::new();
        match &agent.model {
            Some(model) => models.push(model.id.to_string()),
            None => match load_role(role) {
                Ok(definition) => models.push(session_model(agent, &definition).0),
                // Every role a team file can give an agent ships now; `NotFound` is `Human`
                // alone, which no agent's role is, so this arm never fires on a real team.
                Err(RoleError::NotFound { .. }) => {}
                Err(error) => return Err(error),
            },
        }
        if role == Role::ProductManager {
            models.push(TRIAGE_MODEL.to_string());
        }
        for model in models {
            if prices.prices.contains_key(model.as_str()) {
                continue;
            }
            let ids = unpriced.entry(model).or_default();
            let id = agent.id.to_string();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(unpriced)
}

/// Everything `check_budgets` needs for one session, read from the projections at `now`.
///
/// The session limits are the role's defaults with each field `team.budgets.session` sets put in
/// its place. The task's come from its contract, and a session with no task is bounded by none. The
/// day is the UTC date of `now`, and a team that sets no daily budget has an unbounded day (ADR
/// 0015). The sprint's is the open sprint's, and unbounded with none open or one with no budget.
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
    let (sprint_spent_usd, sprint_max_usd) = match projections.open_sprint()? {
        None => (0.0, f64::INFINITY),
        Some(open) => (
            spent_by(projections, CostScope::Sprint, &open.sprint_id)?.map_or(0.0, |row| row.usd),
            open.budget_usd.unwrap_or(f64::INFINITY),
        ),
    };
    Ok(BudgetState {
        session: *session,
        session_limits,
        task_spent_usd,
        task_max_usd,
        task_sessions,
        task_max_sessions,
        sprint_spent_usd,
        sprint_max_usd,
        day_spent_usd: day.map_or(0.0, |row| row.usd),
        day_max_usd: team.budgets.daily_usd.unwrap_or(f64::INFINITY),
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
        // The team's budgets stop everyone, so the channel is told (5.9).
        let line = match exhausted.scope {
            BudgetScope::DayUsd => format!(
                "the daily budget is spent (${:.2} of ${:.2}): the team pauses until tomorrow (UTC)",
                after.day_spent_usd, after.day_max_usd
            ),
            BudgetScope::SprintUsd => format!(
                "the sprint's budget is spent (${:.2} of ${:.2}): no new work is assigned",
                after.sprint_spent_usd, after.sprint_max_usd
            ),
            _ => continue,
        };
        post_system(log, clock, ids, ids.task_id.clone(), &line)?;
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
        BudgetConsequence::EndSessionWithNote => BudgetExhaustedBodyConsequence::EndSessionWithNote,
        BudgetConsequence::EscalateTask => BudgetExhaustedBodyConsequence::EscalateTask,
        BudgetConsequence::StopNewAssignments => BudgetExhaustedBodyConsequence::StopNewAssignments,
        BudgetConsequence::PauseTeam => BudgetExhaustedBodyConsequence::PauseTeam,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use farik_core::budget::{
        BudgetConsequence, BudgetScope, BudgetState, DEFAULT_SESSION_LIMITS, Exhausted,
        SessionLedger, check_budgets, default_session_limits,
    };
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{Role, TaskContract, validate_contract};
    use farik_core::pricing::prices::PRICE_TABLE;
    use farik_core::pricing::{PriceTable, Usage, validate_price_table};
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::{Team, validate_team};
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::fixtures::{a_new_event, an_event_wire};
    use farik_protocol::event::{
        BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope, CostRecordedBodyPurpose,
        EventBody, EventIds, EventKind, FarikEvent, MessageKind, NewEvent, event_from_value,
    };
    use farik_store::{
        CostScope, EventLog, EventQuery, IN_MEMORY, Projections, open_event_log, open_projections,
    };
    use serde_json::json;

    use super::{
        CostError, CostSource, budget_state, consequence_wire, purpose_wire, record_exhaustion,
        record_session_cost, scope_wire, unpriced_models,
    };
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
            &Usage {
                cache_read_tokens: 2_000_000,
                cache_write_tokens: 4_000_000,
                ..usage(1_000_000, 500_000)
            },
            &prices(),
            &clock(),
        )
        .expect("recorded");
        assert!(close(charged, 4.0), "{charged}");
        let events = everything(&log);
        assert_eq!(events.len(), 2);
        let recorded = &events[1];
        let EventBody::CostRecorded(body) = &recorded.body else {
            panic!("a cost.recorded, not {:?}", recorded.body.kind());
        };
        assert!(close(body.cost_usd, 4.0));
        assert_eq!(
            (body.usage.input_tokens, body.usage.output_tokens),
            (1_000_000, 500_000)
        );
        assert_eq!(body.usage.cache_read_tokens, 2_000_000);
        assert_eq!(body.usage.cache_write_tokens, 4_000_000);
        assert_eq!(body.purpose, CostRecordedBodyPurpose::Implement);
        assert!(!body.unpriced);
        assert_eq!(recorded.envelope.ids, ids(Some("FRK-1"), "s1"));
        let task = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert!(close(task.cost_usd, 4.0));
    }

    #[test]
    fn writes_every_purpose_scope_and_consequence_by_its_own_wire_name() {
        for purpose in [
            SessionPurpose::Triage,
            SessionPurpose::Refine,
            SessionPurpose::Plan,
            SessionPurpose::Implement,
            SessionPurpose::Verify,
            SessionPurpose::Ceremony,
            SessionPurpose::Conversation,
        ] {
            assert_eq!(
                serde_json::to_value(purpose_wire(purpose)).expect("serializes"),
                serde_json::to_value(purpose).expect("serializes"),
            );
        }
        for (scope, wire) in [
            (BudgetScope::SessionTokens, "session_tokens"),
            (BudgetScope::SessionWallClock, "session_wall_clock"),
            (BudgetScope::SessionToolCalls, "session_tool_calls"),
            (BudgetScope::TaskUsd, "task_usd"),
            (BudgetScope::TaskSessions, "task_sessions"),
            (BudgetScope::SprintUsd, "sprint_usd"),
            (BudgetScope::DayUsd, "day_usd"),
        ] {
            assert_eq!(
                serde_json::to_value(scope_wire(scope)).expect("serializes"),
                wire
            );
        }
        for (consequence, wire) in [
            (
                BudgetConsequence::EndSessionWithNote,
                "end_session_with_note",
            ),
            (BudgetConsequence::EscalateTask, "escalate_task"),
            (
                BudgetConsequence::StopNewAssignments,
                "stop_new_assignments",
            ),
            (BudgetConsequence::PauseTeam, "pause_team"),
        ] {
            assert_eq!(
                serde_json::to_value(consequence_wire(consequence)).expect("serializes"),
                wire
            );
        }
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
    fn records_a_model_no_table_prices_at_no_cost_and_says_so() {
        // An agent may run on any model (ADR 0015): its usage is recorded, at no cost, and marked.
        let (log, projections) = a_board();
        filed(&log, &projections, "FRK-1");
        let charged = record_session_cost(
            &log,
            &projections,
            &CostSource {
                model_id: "claude-unknown-9",
                ..source(ids(Some("FRK-1"), "s1"))
            },
            &usage(1000, 0),
            &prices(),
            &clock(),
        )
        .expect("recorded");
        assert!(close(charged, 0.0), "{charged}");
        let events = everything(&log);
        assert_eq!(events.len(), 2);
        let EventBody::CostRecorded(body) = &events[1].body else {
            panic!("a cost.recorded, not {:?}", events[1].body.kind());
        };
        assert_eq!(body.model_id.as_str(), "claude-unknown-9");
        assert!(close(body.cost_usd, 0.0));
        assert!(body.unpriced);
        assert_eq!(body.usage.input_tokens, 1000);
        let task = projections
            .costs(CostScope::Task)
            .expect("the costs read")
            .into_iter()
            .find(|row| row.key == "FRK-1")
            .expect("the task's cost");
        assert_eq!(task.sessions, 1);
        assert!(close(task.usd, 0.0));
    }

    #[test]
    fn names_each_unpriced_model_with_the_agents_that_use_it() {
        let on = |id: &str, role: &str, model: Option<&str>| {
            let mut agent = an_agent_wire(id, role);
            if let Some(model) = model {
                agent["model"] = json!({ "id": model });
            }
            agent
        };
        let mut paused = on("dev-c", "software_developer", Some("claude-other-1"));
        paused["status"] = json!("paused");
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            // Its own model is the one its triage sessions run on: named once, not twice.
            on("pm", "product_manager", Some("claude-sonnet-5")),
            on("dev-a", "software_developer", Some("claude-unknown-9")),
            on("dev-b", "software_developer", Some("claude-unknown-9")),
            paused,
            on("arch", "architect", None),
            on("arch-2", "architect", Some("claude-other-2")),
        ]);
        let team = validate_team(&wire).expect("a team");
        let only_opus_5 = validate_price_table(&json!({
            "version": 1,
            "source_url": "https://example.com/prices",
            "retrieved_at": "2026-09-23",
            "prices": {
                "claude-opus-5": {
                    "input_usd_per_mtok": 5.0,
                    "output_usd_per_mtok": 25.0,
                    "cache_read_usd_per_mtok": 0.5,
                    "cache_write_usd_per_mtok": 6.25
                }
            }
        }))
        .expect("a price table");
        let named = |pairs: &[(&str, &[&str])]| -> BTreeMap<String, Vec<String>> {
            pairs
                .iter()
                .map(|(model, ids)| {
                    (
                        (*model).to_string(),
                        ids.iter().map(|id| (*id).to_string()).collect(),
                    )
                })
                .collect()
        };
        assert_eq!(
            unpriced_models(&team, &only_opus_5)
                .expect("an Architect with no model uses its role's shipped model"),
            named(&[
                ("claude-other-2", &["arch-2"]),
                ("claude-sonnet-5", &["pm"]),
                ("claude-unknown-9", &["dev-a", "dev-b"]),
            ])
        );
        assert_eq!(
            unpriced_models(&team, &PRICE_TABLE).expect("the shipped table"),
            named(&[
                ("claude-other-2", &["arch-2"]),
                ("claude-unknown-9", &["dev-a", "dev-b"]),
            ])
        );
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
    fn fills_the_sprint_budget_from_the_open_sprint() {
        let (log, projections) = a_board();
        filed(&log, &projections, "FRK-1");
        let team = a_team(None);
        let none = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert!(none.sprint_max_usd.is_infinite() && none.sprint_max_usd > 0.0);
        assert!(close(none.sprint_spent_usd, 0.0));

        // S1 open with ten dollars, holding FRK-1.
        let mut started = an_event_wire(EventKind::SprintStarted);
        started["body"]["budget_usd"] = json!(10.0);
        let started = event_from_value(&started).expect("the fixture is schema-valid");
        let started = NewEvent {
            recorded_at: started.envelope.recorded_at,
            ids: started.envelope.ids,
            body: started.body,
        };
        for event in [started, a_new_event(EventKind::SprintPlanned)] {
            let appended = log.append(&event).expect("appends");
            projections.apply(&appended).expect("projects");
        }
        record_session_cost(
            &log,
            &projections,
            &source(ids(Some("FRK-1"), "a")),
            &usage(4_000_000, 0),
            &prices(),
            &clock(),
        )
        .expect("recorded");
        let read = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert!(close(read.sprint_max_usd, 10.0), "{}", read.sprint_max_usd);
        assert!(
            close(read.sprint_spent_usd, 4.0),
            "{}",
            read.sprint_spent_usd
        );
    }

    #[test]
    fn leaves_the_day_unbounded_without_a_daily_budget() {
        let (log, projections) = a_board();
        record_session_cost(
            &log,
            &projections,
            &source(ids(None, "a")),
            &usage(1_000_000_000, 0),
            &prices(),
            &clock(),
        )
        .expect("recorded");
        let mut wire = a_team_wire();
        wire["budgets"] = json!({});
        let team = validate_team(&wire).expect("a team with no daily budget");
        let read = state(&projections, &team, Role::SoftwareDeveloper, None);
        assert!(read.day_max_usd.is_infinite() && read.day_max_usd > 0.0);
        assert!(close(read.day_spent_usd, 1000.0), "{}", read.day_spent_usd);
        assert!(
            !check_budgets(&read)
                .iter()
                .any(|exhausted| exhausted.scope == BudgetScope::DayUsd),
            "no daily budget is spent"
        );
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
        // The exhaustion, and Farik's line in the channel about it.
        assert_eq!(events.len(), 2);
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
        assert_eq!(everything(&log).len(), 2);
    }

    #[test]
    fn posts_a_line_when_the_day_is_spent() {
        let (log, projections) = a_board();
        let team = a_team(None);
        let before = BudgetState {
            day_spent_usd: 19.0,
            sprint_spent_usd: 9.0,
            sprint_max_usd: 10.0,
            ..state(&projections, &team, Role::SoftwareDeveloper, None)
        };
        let day = BudgetState {
            day_spent_usd: 21.0,
            ..before
        };
        let sprint = BudgetState {
            sprint_spent_usd: 11.0,
            ..day
        };

        record_exhaustion(
            &log,
            &projections,
            &before,
            &day,
            &ids(None, "s1"),
            &clock(),
        )
        .expect("recorded");
        record_exhaustion(
            &log,
            &projections,
            &day,
            &sprint,
            &ids(None, "s1"),
            &clock(),
        )
        .expect("recorded");

        let lines: Vec<FarikEvent> = everything(&log)
            .into_iter()
            .filter(|event| event.body.kind() == EventKind::MessagePosted)
            .collect();
        let texts: Vec<String> = lines
            .iter()
            .map(|event| match &event.body {
                EventBody::MessagePosted(body) => {
                    assert_eq!(body.author, "farik");
                    assert_eq!(body.kind, MessageKind::System);
                    assert_eq!(event.envelope.ids.agent_id, None);
                    body.text.clone()
                }
                other => panic!("a message, not {other:?}"),
            })
            .collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        assert!(texts[0].contains("daily budget"), "{texts:?}");
        assert!(texts[1].contains("sprint's budget"), "{texts:?}");
    }
}
