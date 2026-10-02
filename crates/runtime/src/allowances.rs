//! A kit connector's allowances (`docs/SPEC.md` 6.7, ADR 0037): how many calls each period an
//! agent makes of a spending tool without asking.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use farik_core::governor::permissions::MAX_ALLOWANCE;
use farik_protocol::event::{EventBody, EventKind};
use farik_roles::{Kit, KitConnector};
use farik_store::{EventLog, EventQuery, Projections, StoreError};
use serde_json::Value;

/// The allowances a connector entry is written with: the kit's defaults for the service `name`,
/// overlaid by `asked`.
///
/// # Errors
///
/// `allowance_not_offered` for a tool the kit gives no allowance, and `allowance_out_of_range`
/// above 1,000, each with its sentence. A service the kit lacks gives no defaults and offers
/// nothing, so any `asked` is not offered.
pub fn checked_allowances(
    kit: &Kit,
    name: &str,
    asked: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, u32>, String> {
    let offered = kit.connectors.iter().find_map(|connector| match connector {
        KitConnector::Server {
            entry, allowances, ..
        } if entry.name.as_str() == name => Some(allowances),
        _ => None,
    });
    let mut checked: BTreeMap<String, u32> = offered
        .into_iter()
        .flatten()
        .map(|(tool, offer)| (tool.clone(), offer.calls))
        .collect();
    for (tool, calls) in asked {
        if !checked.contains_key(tool) {
            return Err(format!(
                "allowance_not_offered: the kit gives {tool} no allowance, so it asks every time"
            ));
        }
        if *calls > MAX_ALLOWANCE {
            return Err(format!(
                "allowance_out_of_range: {calls} is more than {MAX_ALLOWANCE}; give {tool} from 0 \
                 to {MAX_ALLOWANCE}"
            ));
        }
        checked.insert(tool.clone(), *calls);
    }
    Ok(checked)
}

/// The allowances a request asked for, each tool with a whole number of calls.
///
/// # Errors
///
/// `allowance_out_of_range` for a value that is not a whole number from 0 up to `u32::MAX`.
pub fn asked_allowances(value: &Value) -> Result<BTreeMap<String, u32>, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(tool, calls)| {
            calls
                .as_u64()
                .and_then(|calls| u32::try_from(calls).ok())
                .map(|calls| (tool.clone(), calls))
                .ok_or_else(|| {
                    format!(
                        "allowance_out_of_range: give {tool} a whole number of calls from 0 to \
                         {MAX_ALLOWANCE}"
                    )
                })
        })
        .collect()
}

/// The span a call is counted in (ADR 0037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowancePeriod {
    /// The open sprint: the calls after its `sprint.started`.
    Sprint {
        /// The sprint.
        sprint_id: String,
        /// The sequence number of its `sprint.started`.
        started_seq: u64,
    },
    /// With no sprint open: the calls since the later of the UTC day's start and the last
    /// `sprint.ended`.
    Day {
        /// The UTC day.
        day: NaiveDate,
        /// The sequence number of the last `sprint.ended`, when it falls on that day.
        from_seq: Option<u64>,
    },
}

/// The period `now` is in: the open sprint's, or the day's.
///
/// # Errors
///
/// The log or the projections could not be read.
pub fn allowance_period(
    log: &EventLog,
    projections: &Projections,
    now: DateTime<Utc>,
) -> Result<AllowancePeriod, StoreError> {
    if let Some(open) = projections.open_sprint()? {
        let started = log
            .read(&EventQuery {
                kinds: vec![EventKind::SprintStarted],
                newest_first: true,
                ..EventQuery::default()
            })?
            .into_iter()
            .find(|event| {
                matches!(&event.body, EventBody::SprintStarted(body)
                    if body.sprint_id.as_str() == open.sprint_id)
            })
            // ponytail: a sprint the log does not start counts from the beginning, so a call asks
            // rather than runs; it cannot happen while the projection is built from the log.
            .map_or(0, |event| event.envelope.seq);
        return Ok(AllowancePeriod::Sprint {
            sprint_id: open.sprint_id,
            started_seq: started,
        });
    }
    let day = now.date_naive();
    let from_seq = log
        .read(&EventQuery {
            kinds: vec![EventKind::SprintEnded],
            newest_first: true,
            limit: Some(1),
            ..EventQuery::default()
        })?
        .first()
        .filter(|ended| ended.envelope.recorded_at >= day_start(day))
        .map(|ended| ended.envelope.seq);
    Ok(AllowancePeriod::Day { day, from_seq })
}

fn day_start(day: NaiveDate) -> DateTime<Utc> {
    day.and_time(NaiveTime::MIN).and_utc()
}

/// How many `tool.called` of `agent` for `tool` of `server` the log holds in `period`. The tool
/// is matched by its full name, `mcp__<server>__<tool>`, and the call's server, never by the bare
/// tool name, which another service may share.
///
/// # Errors
///
/// The log could not be read.
pub fn calls_in_period(
    log: &EventLog,
    agent: &str,
    server: &str,
    tool: &str,
    period: &AllowancePeriod,
) -> Result<u32, StoreError> {
    const PAGE: usize = 500;
    let (after_seq, floor) = match period {
        AllowancePeriod::Sprint { started_seq, .. } => (Some(*started_seq), None),
        AllowancePeriod::Day { day, from_seq } => (*from_seq, Some(day_start(*day))),
    };
    let full = format!("mcp__{server}__{tool}");
    let (mut count, mut before_seq) = (0_u32, None);
    loop {
        let page = log.read(&EventQuery {
            agent_id: Some(agent.to_string()),
            kinds: vec![EventKind::ToolCalled],
            after_seq,
            before_seq,
            limit: Some(PAGE),
            newest_first: true,
            ..EventQuery::default()
        })?;
        for event in &page {
            // ponytail: a day's start is found by walking back from the newest call, which
            // assumes the log's times rise with its sequence; the sprint's span needs no time.
            if floor.is_some_and(|start| event.envelope.recorded_at < start) {
                return Ok(count);
            }
            if matches!(&event.body, EventBody::ToolCalled(body)
                if body.tool == full && body.server.as_deref().map(String::as_str) == Some(server))
            {
                count = count.saturating_add(1);
            }
        }
        match page.last() {
            Some(last) if page.len() == PAGE => before_seq = Some(last.envelope.seq),
            _ => return Ok(count),
        }
    }
}

/// What the daemon holds of the calls made in the period it last read, so a spending call does
/// not decode the agent's whole log each time (ADR 0037). Held in `DaemonState`.
#[derive(Debug, Default)]
pub struct AllowanceCounts {
    /// The period the counts were read for.
    pub period: Option<AllowancePeriod>,
    /// The calls made, by agent, server and bare tool name, for each one asked about.
    pub used: HashMap<(String, String, String), u32>,
}

impl AllowanceCounts {
    /// The calls of `agent` for `tool` of `server` in `period`: held, or read from the log the
    /// first time they are asked for. A period other than the one held drops every count.
    ///
    /// # Errors
    ///
    /// The log could not be read.
    pub fn used(
        &mut self,
        log: &EventLog,
        period: &AllowancePeriod,
        agent: &str,
        server: &str,
        tool: &str,
    ) -> Result<u32, StoreError> {
        if self.period.as_ref() != Some(period) {
            self.period = Some(period.clone());
            self.used.clear();
        }
        let key = (agent.to_string(), server.to_string(), tool.to_string());
        if let Some(held) = self.used.get(&key) {
            return Ok(*held);
        }
        let read = calls_in_period(log, agent, server, tool, period)?;
        self.used.insert(key, read);
        Ok(read)
    }

    /// One more call recorded for the key, when it is held; one not held is read later, the call
    /// included.
    pub fn raise(&mut self, agent: &str, server: &str, tool: &str) {
        let key = (agent.to_string(), server.to_string(), tool.to_string());
        if let Some(held) = self.used.get_mut(&key) {
            *held = held.saturating_add(1);
        }
    }
}
