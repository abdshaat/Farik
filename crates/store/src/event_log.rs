//! The event log: every action the team takes, in the order it happened, never changed afterwards
//! (`docs/SPEC.md` sections 5.1 and 8.4).

use std::fmt::Write;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use farik_core::contract::TaskId;
use farik_protocol::event::{
    EventEnvelope, EventKind, FarikEvent, NewEvent, body_to_value, event_from_value, event_to_value,
};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, params_from_iter};
use serde_json::{Map, Value};

use crate::error::StoreError;
use crate::migrations;

/// The prefix every task id this store hands out carries, from the contract schema's pattern.
pub(crate) const TASK_ID_PREFIX: &str = "FRK";

/// How many times a connection tries to put the file in write-ahead logging mode before it gives
/// up, and how long it waits between tries. `busy_timeout` does not cover this one lock, so this is
/// the same waiting done by hand: a second at the outside, and nothing at all once the file is in
/// the mode.
const JOURNAL_MODE_TRIES: u32 = 50;
const JOURNAL_MODE_WAIT: Duration = Duration::from_millis(20);

/// How long a statement waits for another connection's write lock before it gives up.
///
/// Thirty seconds rather than five, raised 2026-09-21 after CI refused ten processes appending to
/// one log with `database is locked`: `synchronous = FULL` makes every commit an fsync, and on a
/// loaded runner ten of those queued behind one another take longer than five seconds. A command
/// that waits is doing what a person would want; one that refuses because another `farik` was
/// mid-append is not, and 8.5 says two processes on one project is the ordinary case.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Only events produced by this agent, matched exactly. The log stores the id the wire rules
    /// settled on, so an id with space around it matches nothing; `TaskId` cannot hold one.
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
    connection.busy_timeout(BUSY_TIMEOUT)?;
    if !in_memory {
        // The log is the source of truth for what happened, so an append that returned must
        // survive the machine losing power: `FULL` is that promise, and write-ahead logging is what
        // makes it affordable. A database in memory has no journal to set.
        write_ahead(&connection)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
    }
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
        let connection = self.connection();
        connection.execute(
            "INSERT INTO events
                     (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                stamp(checked.envelope.recorded_at),
                &checked.envelope.ids.team_id,
                &checked.envelope.ids.project_id,
                checked
                    .envelope
                    .ids
                    .task_id
                    .as_ref()
                    .map(|id| id.to_string()),
                checked.envelope.ids.agent_id.as_ref(),
                checked.envelope.ids.session_id.as_ref(),
                checked.body.kind().to_string(),
                body,
            ),
        )?;
        let seq =
            u64::try_from(connection.last_insert_rowid()).map_err(|_| StoreError::Sqlite {
                detail: "the log's sequence number is negative, which no append can produce"
                    .to_string(),
            })?;
        let appended = FarikEvent {
            envelope: EventEnvelope {
                seq,
                ..checked.envelope
            },
            body: checked.body,
        };
        // Announced while this lock is still held, so that two threads appending at once hand their
        // subscribers the order the log gave them rather than the order they left the insert in.
        // Nothing takes the subscribers' lock before this one, so holding both cannot deadlock.
        //
        // A subscriber's channel is unbounded, so this send cannot block, and the subscriber's own
        // work happens on its own thread after this returns. A bounded channel here would deadlock
        // against any subscriber that writes to this database — the projections do.
        self.announce(&appended);
        drop(connection);
        Ok(appended)
    }

    /// Reads the events the query asks for, in the order they happened.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails; `InvalidEvent` when a row cannot be read back as an event.
    pub fn read(&self, query: &EventQuery) -> Result<Vec<FarikEvent>, StoreError> {
        let (sql, parameters) = statement_of(query);
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

    /// The next task id, with the counter moved on so that no two callers get the same one.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the counter cannot be read or written; `TaskIdsExhausted` when the next number
    /// no longer fits the contract schema's pattern; `InvalidEvent` never.
    pub fn next_task_id(&self) -> Result<TaskId, StoreError> {
        self.next_task_id_above(0)
    }

    /// The next task id past both the counter and `taken`, the highest number an id already in use
    /// somewhere the log cannot see holds, with the counter moved past both.
    ///
    /// The log is machine-local and the contracts are committed (8.4), so a fresh clone has the
    /// contracts and a counter at zero. The counter stays the authority for uniqueness across
    /// processes: the id is still one increment of it, done in one statement.
    ///
    /// # Errors
    ///
    /// As `next_task_id`.
    pub fn next_task_id_above(&self, taken: u64) -> Result<TaskId, StoreError> {
        let taken =
            i64::try_from(taken).map_err(|_| StoreError::TaskIdsExhausted { next: taken })?;
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let next: i64 = transaction.query_row(
            "INSERT INTO task_counters (prefix, next) VALUES (?1, ?2 + 1)
             ON CONFLICT (prefix) DO UPDATE SET next = max(next, ?2) + 1
             RETURNING next",
            (TASK_ID_PREFIX, taken),
            |row| row.get(0),
        )?;
        transaction.commit()?;
        let number = u64::try_from(next).map_err(|_| StoreError::Sqlite {
            detail: "the task id counter is negative, which no increment can produce".to_string(),
        })?;
        // Where the counter ends is where the contract schema's pattern ends, and `TaskId` is
        // generated from that pattern, so asking it is the only way to know the bound without
        // writing it down a second time — and two spellings of one fact can disagree.
        format!("{TASK_ID_PREFIX}-{number}")
            .parse()
            .map_err(|_| StoreError::TaskIdsExhausted { next: number })
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

    /// The one connection this database has, recovering from a lock another thread poisoned by
    /// panicking. A panic somewhere else says nothing about this database, and refusing every later
    /// append because of it would turn one bug into a stopped team.
    ///
    /// The projections live in this database beside the log and take this same lock, so a view
    /// cannot read a half-written append, and a log opened in memory can be projected at all — a
    /// second connection could not reach it.
    pub(crate) fn connection(&self) -> MutexGuard<'_, Connection> {
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
///
/// Written through the protocol crate's own writer rather than field by field, so that the envelope
/// has one writer. A second one would drop a field added to the envelope later without anything
/// noticing, because both sides of the round-trip test come from the same value.
fn wire_of(event: &NewEvent, seq: u64) -> Value {
    event_to_value(&FarikEvent {
        envelope: EventEnvelope {
            seq,
            recorded_at: event.recorded_at,
            ids: event.ids.clone(),
        },
        body: event.body.clone(),
    })
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

/// Puts the file in write-ahead logging mode, waiting for whichever connection is doing it.
///
/// Changing the journal mode takes an exclusive lock on the file, and SQLite refuses that at once
/// rather than waiting for `busy_timeout`, which every other statement here honours. So several
/// `farik` commands opening one fresh log at the same moment cannot all perform the switch, and the
/// ones that lose have to wait by hand. What the log needs is the mode the file is in, not which
/// connection put it there: a connection that finds the file already in it is done.
///
/// A file that is already in the mode needs no lock at all, so this waits only on the first open of
/// a new log, and only when something else is opening it at the same moment.
fn write_ahead(connection: &Connection) -> Result<(), StoreError> {
    let mut refused = None;
    for attempt in 0..JOURNAL_MODE_TRIES {
        if journal_is_write_ahead(connection) {
            return Ok(());
        }
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(refusal) => refused = Some(refusal),
        }
        if attempt + 1 < JOURNAL_MODE_TRIES {
            std::thread::sleep(JOURNAL_MODE_WAIT);
        }
    }
    Err(refused.map_or_else(
        || StoreError::Sqlite {
            detail: "the journal mode could not be read or set".to_string(),
        },
        StoreError::from,
    ))
}

/// Whether the file is in write-ahead logging mode. A read that is itself refused answers `false`,
/// because the answer is then not yet known and the caller is about to wait and ask again.
fn journal_is_write_ahead(connection: &Connection) -> bool {
    connection
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .is_ok_and(|mode| mode.eq_ignore_ascii_case("wal"))
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

/// The SQL and the parameters one query needs.
///
/// Every value is a bound parameter; the only thing the query's contents change about the SQL is
/// how many placeholders the `kind` list has.
fn statement_of(query: &EventQuery) -> (String, Vec<SqlValue>) {
    let mut sql = "SELECT seq, recorded_at, team_id, project_id, task_id, agent_id, session_id, \
                   kind, body FROM events"
        .to_string();
    let mut conditions: Vec<String> = Vec::new();
    let mut parameters: Vec<SqlValue> = Vec::new();
    if let Some(after_seq) = query.after_seq {
        // A sequence number no row can hold matches nothing, because `seq` is a signed integer in
        // the engine and the log cannot reach past it. A reader resuming from the end of the log is
        // the ordinary case, so that is an empty answer rather than a refusal.
        let after_seq = i64::try_from(after_seq).unwrap_or(i64::MAX);
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
    (sql, parameters)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc::TryRecvError;
    use std::time::Duration;

    use chrono::TimeZone;
    use farik_protocol::event::fixtures::{a_new_event as an_event, an_event_wire};
    use farik_protocol::event::{EVERY_KIND, EventKind};

    use super::{
        DateTime, EventLog, EventQuery, FarikEvent, IN_MEMORY, Path, Receiver, StoreError, Utc,
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

    /// Enough threads and appends that an announcement made outside the log's own lock is handed
    /// out of order many times over, and few enough that the test is over in well under a second.
    const THREADS: u32 = 16;
    const APPENDS_PER_THREAD: u32 = 200;

    /// Sets the task id counter, so that a test can stand at the end of what the schema can spell
    /// without taking a million ids to get there.
    fn set_counter(log: &EventLog, next: i64) {
        log.connection
            .lock()
            .expect("a fresh lock")
            .execute(
                "INSERT INTO task_counters (prefix, next) VALUES ('FRK', ?1)
                 ON CONFLICT (prefix) DO UPDATE SET next = ?1",
                (next,),
            )
            .expect("the counter is set");
    }

    /// The next event a subscriber is handed, waiting only as long as an announcement could take.
    fn heard(stream: &Receiver<FarikEvent>, who: &str) -> FarikEvent {
        stream
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|refusal| panic!("{who} hears the append: {refusal}"))
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
        blank.ids.team_id = "  ".to_string();
        let refusal = log.append(&blank).expect_err("a blank team id is refused");
        assert!(
            matches!(&refusal, StoreError::InvalidEvent { detail } if detail.contains("team_id")),
            "{refusal:?}"
        );
        let mut unnamed = an_event(EventKind::ContractWritten);
        unnamed.ids.task_id = None;
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
        // A limit no `i64` can hold asks for more rows than the log will ever hold, which is all of
        // them rather than none of them.
        let more_than_there_are = EventQuery {
            limit: Some(usize::MAX),
            ..EventQuery::default()
        };
        assert_eq!(log.read(&more_than_there_are).expect("reads").len(), 4);
    }

    #[test]
    fn reads_the_events_one_agent_produced() {
        let log = a_log();
        let mut by_maya = an_event(EventKind::TeamUpdated);
        by_maya.ids.agent_id = Some("maya-chen".to_string());
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
    fn stores_the_ids_the_wire_rules_settled_on_rather_than_the_ones_it_was_given() {
        // The reader trims an id before it accepts one, and the columns are what a query filters on.
        // A log that kept the padding would answer nothing to a query for the id it had just told
        // its caller it stored.
        let log = a_log();
        let mut padded = an_event(EventKind::TeamUpdated);
        padded.ids.team_id = " farik ".to_string();
        padded.ids.agent_id = Some(" maya-chen ".to_string());
        let appended = log.append(&padded).expect("appends");
        assert_eq!(appended.envelope.ids.team_id, "farik");
        assert_eq!(appended.envelope.ids.agent_id.as_deref(), Some("maya-chen"));
        let by_agent = EventQuery {
            agent_id: Some("maya-chen".to_string()),
            ..EventQuery::default()
        };
        assert_eq!(
            log.read(&by_agent).expect("reads"),
            vec![appended],
            "the row holds the trimmed id, so the query finds it"
        );
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
    fn hands_a_subscriber_the_events_in_the_order_the_log_gave_them() {
        // `append` takes `&self` so that one log can be shared, and `docs/SPEC.md` 8.5 pairs the
        // log's monotonic sequence with this channel. A reader that is handed 34 and then 28 either
        // skips events or replays them, and the log it would rebuild from is itself fine, so the
        // announcement has to carry the order the log assigned rather than the order the threads
        // happened to leave the insert in.
        let log = Arc::new(a_log());
        let stream = log.subscribe();
        let threads: Vec<_> = (0..THREADS)
            .map(|_| {
                let log = Arc::clone(&log);
                std::thread::spawn(move || {
                    for _ in 0..APPENDS_PER_THREAD {
                        log.append(&an_event(EventKind::TeamUpdated))
                            .expect("appends");
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("the thread finishes");
        }
        let announced: Vec<u64> = stream.try_iter().map(|event| event.envelope.seq).collect();
        let appended: Vec<u64> = (1..=u64::from(THREADS * APPENDS_PER_THREAD)).collect();
        assert_eq!(announced.len(), appended.len(), "every append is announced");
        assert_eq!(announced, appended, "in the order the log gave them");
    }

    #[test]
    fn announces_every_append_to_every_subscriber_and_forgets_the_ones_that_left() {
        let log = a_log();
        let first = log.subscribe();
        let second = log.subscribe();
        let appended = log
            .append(&an_event(EventKind::TaskCreated))
            .expect("appends");
        // Bounded, because an announcement that never comes should fail the test rather than hang
        // the job: `cargo test` has no timeout of its own.
        assert_eq!(heard(&first, "the first"), appended);
        assert_eq!(heard(&second, "the second"), appended);
        drop(second);
        let next = log
            .append(&an_event(EventKind::RequestTriaged))
            .expect("appends");
        assert_eq!(heard(&first, "the first still"), next);
        assert_eq!(log.subscribers_lock().len(), 1);
        // A subscriber hears what happened after it subscribed, not before.
        let late = log.subscribe();
        assert_eq!(late.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn hands_out_one_task_id_per_call_and_never_the_same_one_twice() {
        let log = a_log();
        let ids: Vec<String> = (0..3)
            .map(|_| log.next_task_id().expect("an id").to_string())
            .collect();
        assert_eq!(ids, ["FRK-1", "FRK-2", "FRK-3"]);
    }

    #[test]
    fn hands_out_an_id_past_one_already_taken_and_moves_the_counter_past_it() {
        // A fresh clone has contracts and a counter at zero: the id has to be past both, and the
        // counter has to remember that, so that the next caller in another process is past it too.
        let log = a_log();
        assert_eq!(
            log.next_task_id_above(4).expect("an id").to_string(),
            "FRK-5"
        );
        assert_eq!(log.next_task_id().expect("an id").to_string(), "FRK-6");
        assert_eq!(
            log.next_task_id_above(2).expect("an id").to_string(),
            "FRK-7",
            "and a counter already past what is taken keeps counting"
        );
    }

    #[test]
    fn moves_a_counter_that_already_counts_past_an_id_taken_since() {
        // A pull brings in contracts the counter has not seen: the counter's row already exists,
        // so this is the conflict branch, and it has to jump rather than count one on.
        let log = a_log();
        assert_eq!(log.next_task_id().expect("an id").to_string(), "FRK-1");
        assert_eq!(
            log.next_task_id_above(10).expect("an id").to_string(),
            "FRK-11"
        );
    }

    #[test]
    fn hands_out_the_last_id_the_contract_schema_can_spell_and_then_refuses() {
        // The pattern is `^FRK-[0-9]{1,6}$`, so the counter has an end, and both sides of it matter:
        // refusing a number early costs a project an id it was entitled to, and refusing none at
        // all hands back something that is not a task id.
        let log = a_log();
        set_counter(&log, 999_998);
        assert_eq!(
            log.next_task_id()
                .expect("the last id there is")
                .to_string(),
            "FRK-999999"
        );
        assert_eq!(
            log.next_task_id().expect_err("and none after it"),
            StoreError::TaskIdsExhausted { next: 1_000_000 }
        );
    }

    #[test]
    fn refuses_a_row_whose_value_is_not_of_the_kind_its_column_promises() {
        // Every table is STRICT, which is what makes a column's declared type a promise about what
        // is in it rather than a hint. Without it SQLite stores whatever it is given and every
        // reader afterwards has to defend against a team id that is not text at all.
        //
        // What STRICT refuses is a value it cannot convert: a blob is not text, and a word is not a
        // number. It does convert a number to text, so a TEXT column is a promise about the kind of
        // value that comes back out, not about what a caller may write.
        let log = a_log();
        let connection = log.connection.lock().expect("a fresh lock");
        let not_text = connection
            .execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, kind, body)
                 VALUES ('2026-09-17T10:00:00Z', X'0001', 'farik', 'team.updated', '{}')",
                (),
            )
            .expect_err("a blob is not a team id");
        assert!(
            not_text.to_string().contains("cannot store BLOB value"),
            "{not_text}"
        );
        let not_a_number = connection
            .execute(
                "INSERT INTO task_counters (prefix, next) VALUES ('FRK', 'the next one')",
                (),
            )
            .expect_err("a word is not a counter");
        assert!(
            not_a_number.to_string().contains("cannot store TEXT value"),
            "{not_a_number}"
        );
    }

    #[test]
    fn refuses_a_row_whose_body_is_not_json_at_all() {
        // A body that is not JSON makes every later read of the log fail, whichever events the
        // query asks for, because a read hands each row to the protocol crate's reader. Nothing
        // this crate writes can produce such a row, so the engine is where it has to be stopped.
        let log = a_log();
        let refusal = log
            .connection
            .lock()
            .expect("a fresh lock")
            .execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, kind, body)
                 VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', 'team.updated',
                         'not json at all')",
                (),
            )
            .expect_err("a body is JSON or it is not a body");
        assert!(
            refusal.to_string().contains("CHECK constraint failed"),
            "{refusal}"
        );
    }
}
