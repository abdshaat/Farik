//! The event log: every action the team takes, in the order it happened, never changed afterwards
//! (`docs/SPEC.md` sections 5.1 and 8.4).

use std::fmt::Write;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, SecondsFormat, Utc};
use farik_core::contract::TaskId;
use farik_protocol::event::{
    EventEnvelope, EventKind, FarikEvent, NewEvent, body_to_value, event_from_value,
};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, params_from_iter};
use serde_json::{Map, Value};

use crate::error::StoreError;
use crate::migrations;

/// The path that opens a database in memory rather than on disk, for tests and for a dry run.
pub const IN_MEMORY: &str = ":memory:";

/// An append-only log of everything that happened, with the projections' counters beside it.
///
/// One connection, behind a lock: SQLite serialises writers anyway, and a lock here means the
/// sequence number a caller is handed is the one its own insert produced rather than another
/// thread's. `append` takes `&self` so that the log can be shared.
pub struct EventLog {
    connection: Mutex<Connection>,
    subscribers: Mutex<Vec<Sender<FarikEvent>>>,
}

/// Which events to read. Every field left empty means "no filter"; `Default` reads the whole log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventQuery {
    /// Only events after this sequence number, exclusive, which is how a reader resumes.
    pub after_seq: Option<u64>,
    /// Only events about this contract.
    pub task_id: Option<TaskId>,
    /// Only events produced by this agent.
    pub agent_id: Option<String>,
    /// Only these kinds; an empty list is every kind.
    pub kinds: Vec<EventKind>,
    /// At most this many, taken from the lowest sequence number up.
    pub limit: Option<usize>,
}

/// Opens the log at `path`, making its directory and bringing its shape up to date, and returns it
/// ready to append to. `IN_MEMORY` opens a database that lives only as long as the value returned.
///
/// Opening an existing log applies whatever migrations it is missing and nothing else, so opening
/// is safe to do on every command.
///
/// # Errors
///
/// `Io` when the directory cannot be made; `Sqlite` when the file cannot be opened or a migration
/// fails.
pub fn open_event_log(path: &Path, now: DateTime<Utc>) -> Result<EventLog, StoreError> {
    let in_memory = path == Path::new(IN_MEMORY);
    if !in_memory
        && let Some(directory) = path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
    {
        std::fs::create_dir_all(directory)?;
    }
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrations::apply(&mut connection, now)?;
    Ok(EventLog {
        connection: Mutex::new(connection),
        subscribers: Mutex::new(Vec::new()),
    })
}

impl EventLog {
    /// Appends one event and returns it with the sequence number the log gave it.
    ///
    /// The event is checked against `docs/schemas/event.schema.json` on the way in, by the same
    /// reader that guards the wire, because `NewEvent`'s fields are public and a caller that built
    /// one by hand rather than through `new_event` would otherwise put a blank id or a body that
    /// does not match its kind into a log nothing can correct.
    ///
    /// # Errors
    ///
    /// `InvalidEvent` when the event does not pass that check; `Sqlite` when the insert fails.
    pub fn append(&self, event: &NewEvent) -> Result<FarikEvent, StoreError> {
        // Sequence zero is a placeholder that never reaches a row: the insert assigns the real one,
        // and the schema's `seq` allows zero only so that this check can be made before it exists.
        let checked = read_wire(&wire_of(event, 0)).map_err(|detail| StoreError::InvalidEvent {
            detail: format!("an event was refused before it was appended: {detail}"),
        })?;
        let body = body_to_value(&checked.body).to_string();
        let seq = {
            let connection = self.connection();
            connection.execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    stamp(checked.envelope.recorded_at),
                    &checked.envelope.team_id,
                    &checked.envelope.project_id,
                    checked.envelope.task_id.as_ref().map(|id| id.to_string()),
                    checked.envelope.agent_id.as_ref(),
                    checked.envelope.session_id.as_ref(),
                    checked.body.kind().to_string(),
                    body,
                ),
            )?;
            u64::try_from(connection.last_insert_rowid()).map_err(|_| StoreError::Sqlite {
                detail: "the log's sequence number is negative, which no append can produce"
                    .to_string(),
            })?
        };
        let appended = FarikEvent {
            envelope: EventEnvelope {
                seq,
                ..checked.envelope
            },
            body: checked.body,
        };
        self.announce(&appended);
        Ok(appended)
    }

    /// Reads the events the query asks for, in the order they happened.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails; `InvalidEvent` when a row cannot be read back as an event.
    pub fn read(&self, query: &EventQuery) -> Result<Vec<FarikEvent>, StoreError> {
        let Some((sql, parameters)) = statement_of(query) else {
            // A sequence number no row can hold asks for nothing, which is an empty answer rather
            // than an error: a reader resuming from the end of the log is the ordinary case.
            return Ok(Vec::new());
        };
        let connection = self.connection();
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(parameters), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(event_of_row(row?)?);
        }
        Ok(events)
    }

    /// A channel that receives every event appended after this call. Each subscriber gets its own,
    /// and one that stops listening is dropped at the next append rather than holding events for a
    /// receiver nobody owns.
    ///
    /// An event reaches a subscriber only once it is committed: a subscriber that acted on an
    /// append that then failed would have seen something that did not happen.
    pub fn subscribe(&self) -> Receiver<FarikEvent> {
        let (sender, receiver) = channel();
        self.subscribers_lock().push(sender);
        receiver
    }

    /// The migration versions this log has applied, in order.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the ledger cannot be read.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        let mut versions = Vec::new();
        for row in rows {
            versions.push(row?);
        }
        Ok(versions)
    }

    /// The connection, recovering from a lock another thread poisoned by panicking. A panic
    /// somewhere else says nothing about this database, and refusing every later append because of
    /// it would turn one bug into a stopped team.
    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn subscribers_lock(&self) -> MutexGuard<'_, Vec<Sender<FarikEvent>>> {
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn announce(&self, event: &FarikEvent) {
        self.subscribers_lock()
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
}

/// The wire value of an event that has not been appended, with `seq` as given.
fn wire_of(event: &NewEvent, seq: u64) -> Value {
    let mut wire = Map::new();
    wire.insert("seq".to_string(), Value::from(seq));
    wire.insert(
        "recorded_at".to_string(),
        Value::String(stamp(event.recorded_at)),
    );
    wire.insert("team_id".to_string(), Value::String(event.team_id.clone()));
    wire.insert(
        "project_id".to_string(),
        Value::String(event.project_id.clone()),
    );
    if let Some(task_id) = &event.task_id {
        wire.insert("task_id".to_string(), Value::String(task_id.to_string()));
    }
    if let Some(agent_id) = &event.agent_id {
        wire.insert("agent_id".to_string(), Value::String(agent_id.clone()));
    }
    if let Some(session_id) = &event.session_id {
        wire.insert("session_id".to_string(), Value::String(session_id.clone()));
    }
    wire.insert(
        "kind".to_string(),
        Value::String(event.body.kind().to_string()),
    );
    wire.insert("body".to_string(), body_to_value(&event.body));
    Value::Object(wire)
}

fn read_wire(wire: &Value) -> Result<FarikEvent, String> {
    event_from_value(wire).map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{}: {}", error.path, error.message))
            .collect::<Vec<String>>()
            .join("; ")
    })
}

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

type Row = (
    i64,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
);

fn event_of_row(row: Row) -> Result<FarikEvent, StoreError> {
    let (seq, recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body) = row;
    let body: Value = serde_json::from_str(&body).map_err(|error| StoreError::InvalidEvent {
        detail: format!("the body of event {seq} is not JSON: {error}"),
    })?;
    let mut wire = Map::new();
    let seq_number = u64::try_from(seq).map_err(|_| StoreError::InvalidEvent {
        detail: format!("event {seq} has a negative sequence number"),
    })?;
    wire.insert("seq".to_string(), Value::from(seq_number));
    wire.insert("recorded_at".to_string(), Value::String(recorded_at));
    wire.insert("team_id".to_string(), Value::String(team_id));
    wire.insert("project_id".to_string(), Value::String(project_id));
    for (field, value) in [
        ("task_id", task_id),
        ("agent_id", agent_id),
        ("session_id", session_id),
    ] {
        if let Some(value) = value {
            wire.insert(field.to_string(), Value::String(value));
        }
    }
    wire.insert("kind".to_string(), Value::String(kind));
    wire.insert("body".to_string(), body);
    read_wire(&Value::Object(wire)).map_err(|detail| StoreError::InvalidEvent {
        detail: format!("event {seq} cannot be read back: {detail}"),
    })
}

/// The SQL and the parameters one query needs, or `None` when the query can match no row at all.
///
/// Every value is a bound parameter; the only thing the query's contents change about the SQL is
/// how many placeholders the `kind` list has.
fn statement_of(query: &EventQuery) -> Option<(String, Vec<SqlValue>)> {
    let mut sql = "SELECT seq, recorded_at, team_id, project_id, task_id, agent_id, session_id, \
                   kind, body FROM events"
        .to_string();
    let mut conditions: Vec<String> = Vec::new();
    let mut parameters: Vec<SqlValue> = Vec::new();
    if let Some(after_seq) = query.after_seq {
        // A sequence number no row can hold matches nothing, because `seq` is a signed integer in
        // the engine and the log cannot reach past it.
        let after_seq = i64::try_from(after_seq).ok()?;
        conditions.push(format!("seq > ?{}", parameters.len() + 1));
        parameters.push(SqlValue::Integer(after_seq));
    }
    if let Some(task_id) = &query.task_id {
        conditions.push(format!("task_id = ?{}", parameters.len() + 1));
        parameters.push(SqlValue::Text(task_id.to_string()));
    }
    if let Some(agent_id) = &query.agent_id {
        conditions.push(format!("agent_id = ?{}", parameters.len() + 1));
        parameters.push(SqlValue::Text(agent_id.clone()));
    }
    if !query.kinds.is_empty() {
        let placeholders: Vec<String> = query
            .kinds
            .iter()
            .enumerate()
            .map(|(offset, _)| format!("?{}", parameters.len() + offset + 1))
            .collect();
        conditions.push(format!("kind IN ({})", placeholders.join(", ")));
        for kind in &query.kinds {
            parameters.push(SqlValue::Text(kind.to_string()));
        }
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY seq");
    if let Some(limit) = query.limit {
        let _ = write!(sql, " LIMIT ?{}", parameters.len() + 1);
        parameters.push(SqlValue::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    }
    Some((sql, parameters))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::TryRecvError;

    use chrono::TimeZone;
    use farik_protocol::event::fixtures::an_event_wire;
    use farik_protocol::event::{EVERY_KIND, EventKind};

    use super::{
        DateTime, EventLog, EventQuery, FarikEvent, IN_MEMORY, NewEvent, Path, StoreError, Utc,
        body_to_value, event_from_value, open_event_log,
    };
    use crate::migrations;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn a_log() -> EventLog {
        open_event_log(Path::new(IN_MEMORY), at(9)).expect("a log in memory opens")
    }

    /// The fixture event of one kind, ready to append: what `new_event` would have produced, built
    /// from the protocol crate's own wire fixture so that the two cannot drift.
    fn an_event(kind: EventKind) -> NewEvent {
        let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            team_id: event.envelope.team_id,
            project_id: event.envelope.project_id,
            task_id: event.envelope.task_id,
            agent_id: event.envelope.agent_id,
            session_id: event.envelope.session_id,
            body: event.body,
        }
    }

    fn kinds_of(events: &[FarikEvent]) -> Vec<EventKind> {
        events.iter().map(|event| event.body.kind()).collect()
    }

    #[test]
    fn opens_a_log_and_records_every_migration_it_applied() {
        let log = a_log();
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
    }

    #[test]
    fn applies_a_migration_once_however_often_the_log_is_opened() {
        // Opening is done on every command, so it has to be free of consequences the second time.
        let log = a_log();
        let mut connection = log.connection.lock().expect("a fresh lock");
        migrations::apply(&mut connection, at(10)).expect("applying again is a no-op");
        drop(connection);
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
    }

    #[test]
    fn refuses_to_change_or_delete_an_event() {
        // Append-only is the engine's rule, not only this module's: a repair script and a person
        // with the sqlite3 shell meet the same wall. The row goes in by hand, so the rule holds
        // from the migration that makes the trigger, before anything can append.
        let log = a_log();
        let connection = log.connection.lock().expect("a fresh lock");
        connection
            .execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                 VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', NULL, NULL, NULL,
                         'team.updated', '{}')",
                (),
            )
            .expect("a row goes in");
        for sql in [
            "UPDATE events SET team_id = 'other' WHERE seq = 1",
            "DELETE FROM events WHERE seq = 1",
        ] {
            let refusal = connection.execute(sql, ()).expect_err("append-only");
            assert!(
                refusal.to_string().contains("append-only"),
                "{sql}: {refusal}"
            );
        }
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM events", (), |row| row.get(0))
            .expect("the count reads");
        assert_eq!(rows, 1);
    }
    #[test]
    fn appends_events_in_order_and_hands_back_the_place_each_was_given() {
        let log = a_log();
        let first = log
            .append(&an_event(EventKind::TaskCreated))
            .expect("appends");
        let second = log
            .append(&an_event(EventKind::RequestTriaged))
            .expect("appends");
        assert_eq!((first.envelope.seq, second.envelope.seq), (1, 2));
        let read = log.read(&EventQuery::default()).expect("reads");
        assert_eq!(read, vec![first, second]);
    }

    #[test]
    fn reads_back_every_kind_exactly_as_it_was_appended() {
        // The row keeps the envelope in columns and the body as JSON, so this is what says the two
        // halves go back together for every kind the phase emits.
        let log = a_log();
        let appended: Vec<FarikEvent> = EVERY_KIND
            .into_iter()
            .map(|kind| log.append(&an_event(kind)).expect("appends"))
            .collect();
        assert_eq!(log.read(&EventQuery::default()).expect("reads"), appended);
        assert_eq!(kinds_of(&appended), EVERY_KIND.to_vec());
    }

    #[test]
    fn refuses_an_event_the_wire_rules_refuse_and_appends_nothing() {
        // `NewEvent`'s fields are public, so an event can reach the log without passing through
        // `new_event`. The log is the source of truth and cannot be corrected afterwards.
        let log = a_log();
        let mut blank = an_event(EventKind::TeamUpdated);
        blank.team_id = "  ".to_string();
        let refusal = log.append(&blank).expect_err("a blank team id is refused");
        assert!(
            matches!(&refusal, StoreError::InvalidEvent { detail } if detail.contains("team_id")),
            "{refusal:?}"
        );
        let mut unnamed = an_event(EventKind::ContractWritten);
        unnamed.task_id = None;
        assert!(matches!(
            log.append(&unnamed),
            Err(StoreError::InvalidEvent { .. })
        ));
        assert_eq!(log.read(&EventQuery::default()).expect("reads"), Vec::new());
    }

    #[test]
    fn reads_the_events_a_query_asks_for_and_no_others() {
        let log = a_log();
        for kind in [
            EventKind::TaskCreated,
            EventKind::RequestTriaged,
            EventKind::TeamUpdated,
            EventKind::ContractWritten,
        ] {
            log.append(&an_event(kind)).expect("appends");
        }
        let after = EventQuery {
            after_seq: Some(2),
            ..EventQuery::default()
        };
        assert_eq!(
            kinds_of(&log.read(&after).expect("reads")),
            [EventKind::TeamUpdated, EventKind::ContractWritten]
        );
        let by_kind = EventQuery {
            kinds: vec![EventKind::TaskCreated, EventKind::ContractWritten],
            ..EventQuery::default()
        };
        assert_eq!(
            kinds_of(&log.read(&by_kind).expect("reads")),
            [EventKind::TaskCreated, EventKind::ContractWritten]
        );
        let limited = EventQuery {
            limit: Some(1),
            ..EventQuery::default()
        };
        assert_eq!(
            kinds_of(&log.read(&limited).expect("reads")),
            [EventKind::TaskCreated]
        );
        // The fixture names the contract on every kind that is about one, and `team.updated` is
        // not about one, so it is the one the filter leaves out.
        let by_task = EventQuery {
            task_id: Some("FRK-1".parse().expect("a task id")),
            ..EventQuery::default()
        };
        assert_eq!(
            kinds_of(&log.read(&by_task).expect("reads")),
            [
                EventKind::TaskCreated,
                EventKind::RequestTriaged,
                EventKind::ContractWritten
            ]
        );
        let other_task = EventQuery {
            task_id: Some("FRK-2".parse().expect("a task id")),
            ..EventQuery::default()
        };
        assert_eq!(log.read(&other_task).expect("reads"), Vec::new());
        // Every filter at once, and then a sequence number the log cannot reach: a reader resuming
        // from the end asks for nothing and is told nothing, rather than refused.
        let everything = EventQuery {
            after_seq: Some(1),
            task_id: Some("FRK-1".parse().expect("a task id")),
            agent_id: None,
            kinds: vec![EventKind::ContractWritten],
            limit: Some(10),
        };
        assert_eq!(
            kinds_of(&log.read(&everything).expect("reads")),
            [EventKind::ContractWritten]
        );
        let past_the_end = EventQuery {
            after_seq: Some(u64::MAX),
            ..EventQuery::default()
        };
        assert_eq!(log.read(&past_the_end).expect("reads"), Vec::new());
    }

    #[test]
    fn reads_the_events_one_agent_produced() {
        let log = a_log();
        let mut by_maya = an_event(EventKind::TeamUpdated);
        by_maya.agent_id = Some("maya-chen".to_string());
        log.append(&by_maya).expect("appends");
        log.append(&an_event(EventKind::TaskCreated))
            .expect("appends");
        let query = EventQuery {
            agent_id: Some("maya-chen".to_string()),
            ..EventQuery::default()
        };
        assert_eq!(
            kinds_of(&log.read(&query).expect("reads")),
            [EventKind::TeamUpdated]
        );
        let nobody = EventQuery {
            agent_id: Some("nobody".to_string()),
            ..EventQuery::default()
        };
        assert_eq!(log.read(&nobody).expect("reads"), Vec::new());
    }

    #[test]
    fn refuses_a_row_that_is_not_an_event_rather_than_half_reading_it() {
        // Nothing this crate writes can produce such a row, so this is about the file having been
        // changed by something else, or written by a Farik this one does not understand.
        let log = a_log();
        let body = body_to_value(
            &event_from_value(&an_event_wire(EventKind::TaskCreated))
                .expect("the fixture")
                .body,
        )
        .to_string();
        log.connection
            .lock()
            .expect("a fresh lock")
            .execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                 VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-1', NULL, NULL,
                         'contract.locked', ?1)",
                (body,),
            )
            .expect("the row is written by hand");
        let refusal = log
            .read(&EventQuery::default())
            .expect_err("a body that does not fit its kind");
        assert!(
            matches!(&refusal, StoreError::InvalidEvent { detail } if detail.contains("event 1")),
            "{refusal:?}"
        );
    }
    #[test]
    fn announces_every_append_to_every_subscriber_and_forgets_the_ones_that_left() {
        let log = a_log();
        let first = log.subscribe();
        let second = log.subscribe();
        let appended = log
            .append(&an_event(EventKind::TaskCreated))
            .expect("appends");
        assert_eq!(first.recv().expect("the first hears"), appended);
        assert_eq!(second.recv().expect("the second hears"), appended);
        drop(second);
        let next = log
            .append(&an_event(EventKind::RequestTriaged))
            .expect("appends");
        assert_eq!(first.recv().expect("the first still hears"), next);
        assert_eq!(log.subscribers_lock().len(), 1);
        // A subscriber hears what happened after it subscribed, not before.
        let late = log.subscribe();
        assert_eq!(late.try_recv(), Err(TryRecvError::Empty));
    }
}
