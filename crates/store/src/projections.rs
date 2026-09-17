//! The board, read from the log rather than scanned out of it (`docs/SPEC.md` sections 8.4 and 10).

use std::str::FromStr;
use std::sync::Arc;

use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};
use farik_protocol::event::{
    ContractSummary, ContractSummaryKind, ContractSummaryRisk, ContractSummaryStatus, EventBody,
    FarikEvent,
};
use rusqlite::{Connection, Transaction};

use crate::error::StoreError;
use crate::event_log::{EventLog, TASK_ID_PREFIX};

/// What a board shows about one contract, as the log left it.
///
/// Every field comes from an event a phase 2 command emits. The assignee, the reviewer, the sprint
/// and the iteration arrive with `task.transitioned` in phase 3, and `cost_usd` with
/// `cost.recorded`; a column nothing can write is a column no test can hold to anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProjection {
    /// The contract this is about.
    pub task_id: TaskId,
    /// Whether it is an epic or a task, as the last event to say so said.
    pub kind: TaskKind,
    /// The epic this is under, when it is under one.
    pub parent: Option<TaskId>,
    /// The contract's title.
    pub title: String,
    /// Where the contract is in the lifecycle of `docs/SPEC.md` 5.2.
    pub status: TaskStatus,
    /// The contract's risk.
    pub risk: Risk,
    /// Whether triage has sized the request (5.16).
    pub triaged: bool,
    /// Whether the human holds the contract (5.11).
    pub locked: bool,
    /// The sequence number of the last event that changed this row, which is what says how fresh it
    /// is and which event to blame for what it says.
    pub updated_seq: u64,
}

/// The projections of one log: derived tables that answer a view in one query.
///
/// They share the log's connection and its lock, so a view cannot read a half-written append, and
/// a log opened in memory can be projected at all.
pub struct Projections {
    log: Arc<EventLog>,
}

/// Opens the projections of `log`.
///
/// # Errors
///
/// `Sqlite` when a projection table cannot be read.
pub fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError> {
    Ok(Projections { log })
}

impl Projections {
    /// Applies one event, and moves the cursor to it.
    ///
    /// The row and the cursor move in one transaction, so the cursor never claims work that was not
    /// done.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the write fails.
    pub fn apply(&self, event: &FarikEvent) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        apply_to(&transaction, event)?;
        write_cursor(&transaction, event.envelope.seq)?;
        transaction.commit()?;
        Ok(())
    }

    /// Every contract the log knows about, oldest task id first.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails; `InvalidEvent` when a projected row cannot be read back.
    pub fn board(&self) -> Result<Vec<TaskProjection>, StoreError> {
        let connection = self.connection();
        let mut statement =
            connection.prepare(&format!("{SELECT_PROJECTION} ORDER BY {BY_NUMBER}"))?;
        let rows = statement.query_map((number_offset(),), projected_row)?;
        let mut board = Vec::new();
        for row in rows {
            board.push(projection_of_row(row?)?);
        }
        Ok(board)
    }

    /// One contract, or nothing when the log has no event about it.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails; `InvalidEvent` when the projected row cannot be read back.
    pub fn task(&self, task_id: &TaskId) -> Result<Option<TaskProjection>, StoreError> {
        let connection = self.connection();
        let mut statement =
            connection.prepare(&format!("{SELECT_PROJECTION} WHERE task_id = ?1"))?;
        let mut rows = statement.query_map((task_id.to_string(),), projected_row)?;
        match rows.next() {
            None => Ok(None),
            Some(row) => Ok(Some(projection_of_row(row?)?)),
        }
    }

    /// How far into the log the projections have read: the sequence number of the last event
    /// applied, or zero when none has been.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the cursor cannot be read.
    pub fn cursor(&self) -> Result<u64, StoreError> {
        let connection = self.connection();
        read_cursor(&connection)
    }

    /// The log's own connection: the projections live in the same database, and share its lock so
    /// that a view cannot read a half-written append.
    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.log.connection()
    }
}

const SELECT_PROJECTION: &str = "SELECT task_id, kind, parent, title, status, risk, triaged, \
                                 locked, updated_seq FROM task_projections";

/// The board is ordered by the number in the task id, not by the id itself: `FRK-10` sorts before
/// `FRK-9` as text, and a board that puts the tenth task before the ninth is a board nobody trusts.
const BY_NUMBER: &str = "CAST(substr(task_id, ?1) AS INTEGER)";

/// Where the number starts in a task id, counting from one as `substr` does: past the prefix and
/// the dash that follows it. Taken from the prefix rather than written down again.
fn number_offset() -> i64 {
    i64::try_from(TASK_ID_PREFIX.len() + 2).unwrap_or(i64::MAX)
}

type ProjectedRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    bool,
    bool,
    i64,
);

fn projected_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectedRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn projection_of_row(row: ProjectedRow) -> Result<TaskProjection, StoreError> {
    let (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq) = row;
    let refuse = |what: &str, value: &str| StoreError::InvalidEvent {
        detail: format!("the projection of {task_id} holds {value:?} as its {what}"),
    };
    Ok(TaskProjection {
        task_id: TaskId::from_str(&task_id).map_err(|_| refuse("id", &task_id))?,
        kind: TaskKind::from_str(&kind).map_err(|_| refuse("kind", &kind))?,
        parent: match parent {
            None => None,
            Some(parent) => Some(TaskId::from_str(&parent).map_err(|_| refuse("parent", &parent))?),
        },
        title,
        status: TaskStatus::from_str(&status).map_err(|_| refuse("status", &status))?,
        risk: Risk::from_str(&risk).map_err(|_| refuse("risk", &risk))?,
        triaged,
        locked,
        updated_seq: u64::try_from(updated_seq)
            .map_err(|_| refuse("sequence number", &updated_seq.to_string()))?,
    })
}

/// Applies one event to the projection tables, leaving the cursor to the caller.
fn apply_to(transaction: &Transaction<'_>, event: &FarikEvent) -> Result<(), StoreError> {
    let Some(task_id) = &event.envelope.task_id else {
        // Only the five kinds that are about one contract touch the board, and the protocol crate
        // refuses one of those without a task id. The rest — a scan, a team, a criterion library,
        // a drift report — are about the project.
        return Ok(());
    };
    let seq = i64::try_from(event.envelope.seq).map_err(|_| StoreError::Sqlite {
        detail: format!(
            "event {} is past what the engine can hold",
            event.envelope.seq
        ),
    })?;
    let id = task_id.to_string();
    match &event.body {
        EventBody::TaskCreated(body) => write_summary(transaction, &id, &body.summary, seq),
        EventBody::ContractWritten(body) => write_summary(transaction, &id, &body.summary, seq),
        EventBody::RequestTriaged(_)
        | EventBody::ContractLocked(_)
        | EventBody::ContractUnlocked(_)
        | EventBody::DriftDetected(_)
        | EventBody::ProjectScanned(_)
        | EventBody::TeamUpdated(_)
        | EventBody::CriteriaUpdated(_) => Ok(()),
    }
}

/// Writes what a summary says, creating the row when this is the first event about the contract.
///
/// An upsert rather than an insert, and an upsert rather than a refusal: a `contract.written` whose
/// `task.created` is missing means a log that cannot be right, and refusing it here would make the
/// whole board unreadable over one row. `farik doctor` is what reports a log and its files
/// disagreeing (5.1), and it needs a board it can read to do that.
fn write_summary(
    transaction: &Transaction<'_>,
    task_id: &str,
    summary: &ContractSummary,
    seq: i64,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO task_projections
             (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7)
         ON CONFLICT (task_id) DO UPDATE SET
             kind = ?2, parent = ?3, title = ?4, status = ?5, risk = ?6, updated_seq = ?7",
        (
            task_id,
            kind_of(summary.kind).to_string(),
            summary.parent.as_ref().map(|parent| parent.to_string()),
            &summary.title,
            status_of(summary.status).to_string(),
            risk_of(summary.risk).to_string(),
            seq,
        ),
    )?;
    Ok(())
}

fn read_cursor(connection: &Connection) -> Result<u64, StoreError> {
    let seq: i64 = connection
        .query_row(
            "SELECT seq FROM projection_cursor WHERE id = 1",
            (),
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(0),
            other => Err(StoreError::from(other)),
        })?;
    u64::try_from(seq).map_err(|_| StoreError::Sqlite {
        detail: "the projection cursor is negative, which no event can produce".to_string(),
    })
}

fn write_cursor(transaction: &Transaction<'_>, seq: u64) -> Result<(), StoreError> {
    let seq = i64::try_from(seq).map_err(|_| StoreError::Sqlite {
        detail: format!("event {seq} is past what the engine can hold"),
    })?;
    transaction.execute(
        "INSERT INTO projection_cursor (id, seq) VALUES (1, ?1)
         ON CONFLICT (id) DO UPDATE SET seq = ?1",
        (seq,),
    )?;
    Ok(())
}

/// The three vocabularies an event repeats from the contract schema, in the contract's own types.
///
/// The two spellings are generated from two schemas, and a test in `farik-protocol` fails when they
/// drift, so these mappings are total and stay total.
fn kind_of(kind: ContractSummaryKind) -> TaskKind {
    match kind {
        ContractSummaryKind::Epic => TaskKind::Epic,
        ContractSummaryKind::Task => TaskKind::Task,
    }
}

fn status_of(status: ContractSummaryStatus) -> TaskStatus {
    match status {
        ContractSummaryStatus::Draft => TaskStatus::Draft,
        ContractSummaryStatus::Refining => TaskStatus::Refining,
        ContractSummaryStatus::Ready => TaskStatus::Ready,
        ContractSummaryStatus::Assigned => TaskStatus::Assigned,
        ContractSummaryStatus::InProgress => TaskStatus::InProgress,
        ContractSummaryStatus::Blocked => TaskStatus::Blocked,
        ContractSummaryStatus::Verifying => TaskStatus::Verifying,
        ContractSummaryStatus::Rejected => TaskStatus::Rejected,
        ContractSummaryStatus::Accepted => TaskStatus::Accepted,
        ContractSummaryStatus::Escalated => TaskStatus::Escalated,
        ContractSummaryStatus::Cancelled => TaskStatus::Cancelled,
    }
}

fn risk_of(risk: ContractSummaryRisk) -> Risk {
    match risk {
        ContractSummaryRisk::Low => Risk::Low,
        ContractSummaryRisk::Medium => Risk::Medium,
        ContractSummaryRisk::High => Risk::High,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};
    use farik_protocol::event::fixtures::{a_contract_summary_wire, a_new_event, an_event_wire};
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};
    use serde_json::json;

    use super::{Arc, EventLog, FarikEvent, Projections, TaskProjection, open_projections};
    use crate::error::StoreError;
    use crate::event_log::{IN_MEMORY, open_event_log};
    use farik_core::contract::{Risk, TaskKind, TaskStatus};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn a_log() -> Arc<EventLog> {
        Arc::new(
            open_event_log(std::path::Path::new(IN_MEMORY), at(9)).expect("a log in memory opens"),
        )
    }

    /// A log and the projections of it, both empty.
    fn a_board() -> (Arc<EventLog>, Projections) {
        let log = a_log();
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        (log, projections)
    }

    /// Appends one event and hands it to the projections, the way a command does.
    fn record(log: &EventLog, projections: &Projections, event: &NewEvent) -> FarikEvent {
        let appended = log.append(event).expect("appends");
        projections.apply(&appended).expect("projects");
        appended
    }

    /// The fixture event of one kind, about the contract `task_id`.
    fn about(kind: EventKind, task_id: &str) -> NewEvent {
        let mut event = a_new_event(kind);
        event.task_id = Some(task_id.parse().expect("a task id"));
        event
    }

    /// A `contract.written` whose summary says what the arguments say.
    fn written(
        task_id: &str,
        title: &str,
        status: &str,
        risk: &str,
        parent: Option<&str>,
    ) -> NewEvent {
        let mut summary = a_contract_summary_wire();
        summary["title"] = json!(title);
        summary["status"] = json!(status);
        summary["risk"] = json!(risk);
        if let Some(parent) = parent {
            summary["parent"] = json!(parent);
        }
        let mut wire = an_event_wire(EventKind::ContractWritten);
        wire["task_id"] = json!(task_id);
        wire["body"]["summary"] = summary;
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
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

    fn ids_of(board: &[TaskProjection]) -> Vec<String> {
        board.iter().map(|task| task.task_id.to_string()).collect()
    }

    #[test]
    fn shows_a_request_on_the_board_as_soon_as_it_is_filed() {
        let (log, projections) = a_board();
        let filed = record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        let board = projections.board().expect("the board reads");
        assert_eq!(
            board,
            vec![TaskProjection {
                task_id: "FRK-1".parse().expect("a task id"),
                kind: TaskKind::Task,
                parent: None,
                title: "Add a login page".to_string(),
                status: TaskStatus::Draft,
                risk: Risk::Low,
                triaged: false,
                locked: false,
                updated_seq: filed.envelope.seq,
            }]
        );
        assert_eq!(projections.cursor().expect("the cursor reads"), 1);
    }

    #[test]
    fn takes_every_field_of_the_latest_contract_written() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
        let rewritten = record(
            &log,
            &projections,
            &written(
                "FRK-2",
                "Add a logout page",
                "refining",
                "high",
                Some("FRK-1"),
            ),
        );
        assert_eq!(
            projections
                .task(&"FRK-2".parse().expect("a task id"))
                .expect("the read works")
                .expect("on the board"),
            TaskProjection {
                task_id: "FRK-2".parse().expect("a task id"),
                kind: TaskKind::Task,
                parent: Some("FRK-1".parse().expect("a task id")),
                title: "Add a logout page".to_string(),
                status: TaskStatus::Refining,
                risk: Risk::High,
                triaged: false,
                locked: false,
                updated_seq: rewritten.envelope.seq,
            }
        );
    }

    #[test]
    fn moves_the_cursor_past_an_event_that_is_about_no_contract() {
        // A scan, a team, a criterion library and a drift report change no row. The cursor still has
        // to pass them, or catching up would read them again on every open, for ever.
        let (log, projections) = a_board();
        for kind in [
            EventKind::ProjectScanned,
            EventKind::TeamUpdated,
            EventKind::CriteriaUpdated,
            EventKind::DriftDetected,
        ] {
            record(&log, &projections, &a_new_event(kind));
        }
        assert_eq!(projections.board().expect("the board reads"), Vec::new());
        assert_eq!(projections.cursor().expect("the cursor reads"), 4);
    }

    #[test]
    fn orders_the_board_by_the_number_in_the_id_rather_than_by_its_text() {
        // `FRK-10` sorts before `FRK-9` as text, and a board that puts the tenth task before the
        // ninth is a board nobody trusts.
        let (log, projections) = a_board();
        for id in ["FRK-2", "FRK-10", "FRK-1"] {
            record(&log, &projections, &about(EventKind::TaskCreated, id));
        }
        assert_eq!(
            ids_of(&projections.board().expect("the board reads")),
            ["FRK-1", "FRK-2", "FRK-10"]
        );
    }

    #[test]
    fn says_nothing_about_a_contract_it_never_saw_an_event_for() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        assert_eq!(
            projections
                .task(&"FRK-9".parse().expect("a task id"))
                .expect("the read works"),
            None
        );
    }

    #[test]
    fn refuses_a_projected_row_it_cannot_read_back() {
        // Nothing this crate writes can produce such a row, so this is about the file having been
        // changed by something else, or written by a Farik this one does not understand.
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        log.connection()
            .execute(
                "UPDATE task_projections SET status = 'nearly done' WHERE task_id = 'FRK-1'",
                (),
            )
            .expect("a row is changed by hand");
        let refusal = projections.board().expect_err("a status nothing spells");
        assert!(
            matches!(&refusal, StoreError::InvalidEvent { detail }
                if detail.contains("FRK-1") && detail.contains("nearly done")),
            "{refusal:?}"
        );
    }
}
