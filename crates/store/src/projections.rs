//! The board, read from the log rather than scanned out of it (`docs/SPEC.md` sections 8.4 and 10).

use std::str::FromStr;
use std::sync::Arc;

use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};
use farik_protocol::event::{
    ContractSummary, ContractSummaryKind, ContractSummaryRisk, ContractSummaryStatus,
    CostRecordedBody, EventBody, FarikEvent, RequestTriagedBodySize,
};
use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::StoreError;
use crate::event_log::{EventLog, EventQuery, TASK_ID_PREFIX};

/// What a board shows about one contract, as the log left it.
///
/// Every field comes from an event a command or the governor emits. The sprint arrives with phase
/// 4, which has the sprints; a column nothing can write is a column no test can hold to anything.
#[derive(Debug, Clone, PartialEq)]
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
    /// What the task has cost, in dollars: the sum of its `cost.recorded` events, `0.0` when
    /// there are none.
    pub cost_usd: f64,
    /// The agent the last `task.transitioned` left the contract with, if any.
    pub assignee_id: Option<String>,
    /// The agent reviewing it, as the last `task.transitioned` said.
    pub reviewer_id: Option<String>,
    /// How many times the task has been returned to work after a rejection, as the last
    /// `task.transitioned` said; `0` before any.
    pub iteration: u32,
    /// Whether the task is `accepted` and its branch has not reached the integration branch yet:
    /// set by its move into `accepted`, cleared by `task.integrated` (5.14). Never set for an
    /// epic, whose children carry the branches.
    pub awaiting_integration: bool,
}

/// A key that costs are summed by (`docs/SPEC.md` 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostScope {
    /// By task id. A cost with no task is in no task's row.
    Task,
    /// By agent id.
    Agent,
    /// By session id.
    Session,
    /// By the UTC date the cost was recorded on, as `YYYY-MM-DD`.
    Day,
}

/// What one key of a scope has spent.
#[derive(Debug, Clone, PartialEq)]
pub struct CostProjection {
    /// The scope this row sums by.
    pub scope: CostScope,
    /// The task id, agent id, session id, or day.
    pub key: String,
    /// Dollars, as priced when each cost was recorded.
    pub usd: f64,
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// How many distinct sessions the costs came from.
    pub sessions: u32,
}

/// The projections of one log: derived tables that answer a view in one query.
///
/// Dropping them and replaying the log is always correct, which is what `rebuild` does. They share
/// the log's connection and its lock, so a view cannot read a half-written append, and a log opened
/// in memory can be projected at all.
pub struct Projections {
    log: Arc<EventLog>,
}

/// Opens the projections of `log` and brings them up to date with it.
///
/// Catching up on open is what makes the pair self-healing: an `apply` that never ran, because the
/// process stopped between the append and the projection, is applied here instead. The cursor is
/// what remembers how far the projections had read.
///
/// # Errors
///
/// `Sqlite` when a projection table cannot be read or written; `InvalidEvent` when the log holds a
/// row that is not an event.
pub fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError> {
    let projections = Projections { log };
    projections.catch_up()?;
    Ok(projections)
}

impl Projections {
    /// Throws the projections away and builds them again from the whole log.
    ///
    /// This is the repair: nothing in the tables is a source of truth, so a projection that has
    /// drifted for any reason — a bug fixed since, a row changed by hand, a migration that added a
    /// column — is corrected by reading the log again.
    ///
    /// Not atomic to a reader in another thread or process: the tables are emptied and committed
    /// before the log is read back, because the connection's lock may not be held across the reads
    /// that refill them. A board read while this is running is empty or half-built.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the tables cannot be cleared or written; `InvalidEvent` when the log holds a
    /// row that is not an event.
    pub fn rebuild(&self) -> Result<(), StoreError> {
        {
            let mut connection = self.connection();
            // `Immediate` as in `apply`: this one writes before it reads, so it is safe either way
            // today, and taking the lock up front keeps it safe if a read is ever added above.
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute("DELETE FROM task_projections", ())?;
            transaction.execute("DELETE FROM cost_records", ())?;
            write_cursor(&transaction, 0)?;
            transaction.commit()?;
        }
        self.catch_up()?;
        Ok(())
    }

    /// Applies one event, and moves the cursor to it.
    ///
    /// Takes the event in whatever order it arrives. An event at or before the cursor is ignored
    /// rather than applied twice, so a caller that both subscribes and catches up on open does not
    /// take one append in twice. An event *past* the next one is a gap — two processes each
    /// appending and projecting can hand this one event 3 before event 2 — and the events in
    /// between are read from the log rather than stepped over: a cursor that moved past an event
    /// nothing applied would leave the board short of the log for good, with nothing to notice.
    ///
    /// The event must be one this log returned from `append`: the sequence number is what places
    /// it, and one from another log would be placed by a number that means nothing here.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the write or the cursor read fails; `InvalidEvent` when the log holds a row
    /// that is not an event, which only a gap can reach.
    pub fn apply(&self, event: &FarikEvent) -> Result<(), StoreError> {
        if event.envelope.seq > self.cursor()?.saturating_add(1) {
            self.catch_up()?;
            return Ok(());
        }
        self.apply_in_order(event)
    }

    /// Applies one event that is the next one, or one already applied.
    fn apply_in_order(&self, event: &FarikEvent) -> Result<(), StoreError> {
        let mut connection = self.connection();
        // `Immediate`, for the reason `migrations::apply` gives at length: this transaction reads
        // the cursor before it writes, and a transaction that takes its read lock first cannot wait
        // for the write lock it turns out to need — SQLite refuses it at once rather than after
        // `busy_timeout`. Two `farik` commands projecting what they appended is the ordinary case.
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if event.envelope.seq <= read_cursor(&transaction)? {
            return Ok(());
        }
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

    /// What each key of `scope` has spent: tasks by the number in the id, as `board` is; the other
    /// scopes by key as text.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails; `InvalidEvent` when a sum cannot be read back as a count.
    pub fn costs(&self, scope: CostScope) -> Result<Vec<CostProjection>, StoreError> {
        let (column, order) = match scope {
            CostScope::Task => ("task_id", BY_NUMBER),
            CostScope::Agent => ("agent_id", "agent_id"),
            CostScope::Session => ("session_id", "session_id"),
            CostScope::Day => ("day", "day"),
        };
        let connection = self.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT {column}, SUM(cost_usd), SUM(input_tokens), SUM(output_tokens),
                    COUNT(DISTINCT session_id)
             FROM cost_records WHERE {column} IS NOT NULL
             GROUP BY {column} ORDER BY {order}"
        ))?;
        // Only the task order has a parameter; SQLite refuses one bound to nothing.
        let parameters: Vec<i64> = match scope {
            CostScope::Task => vec![number_offset()],
            _ => Vec::new(),
        };
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut costs = Vec::new();
        for row in rows {
            let (key, usd, input_tokens, output_tokens, sessions) = row?;
            let refuse = |what: &str, value: i64| StoreError::InvalidEvent {
                detail: format!("the costs of {key} hold {value} as their {what}"),
            };
            costs.push(CostProjection {
                scope,
                usd,
                input_tokens: u64::try_from(input_tokens)
                    .map_err(|_| refuse("input tokens", input_tokens))?,
                output_tokens: u64::try_from(output_tokens)
                    .map_err(|_| refuse("output tokens", output_tokens))?,
                sessions: u32::try_from(sessions).map_err(|_| refuse("sessions", sessions))?,
                key,
            });
        }
        Ok(costs)
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

    /// Applies every event the log has that the cursor has not reached, and says how many that
    /// was. Zero is the ordinary answer, and the one `docs/SPEC.md` section 10 asks for: a board
    /// that is already current costs nothing to open. A caller that appended through something
    /// that does not project, such as `requests::file_request`, calls this after it.
    ///
    /// # Errors
    ///
    /// `Sqlite` when a projection cannot be written; `InvalidEvent` when the log holds a row that
    /// is not an event.
    pub fn catch_up(&self) -> Result<usize, StoreError> {
        let after_seq = self.cursor()?;
        let behind = self.log.read(&EventQuery {
            after_seq: Some(after_seq),
            ..EventQuery::default()
        })?;
        for event in &behind {
            // In order, and through the door that does not look for a gap: this is what fills one.
            self.apply_in_order(event)?;
        }
        Ok(behind.len())
    }

    /// The log's own connection: the projections live in the same database, and share its lock so
    /// that a view cannot read a half-written append.
    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.log.connection()
    }
}

const SELECT_PROJECTION: &str = "SELECT task_id, kind, parent, title, status, risk, triaged, \
                                 locked, updated_seq, \
                                 (SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_records \
                                  WHERE cost_records.task_id = task_projections.task_id), \
                                 assignee_id, reviewer_id, iteration, awaiting_integration \
                                 FROM task_projections";

/// The board is ordered by the number in the task id, not by the id itself: `FRK-10` sorts before
/// `FRK-9` as text, and a board that puts the tenth task before the ninth is a board nobody trusts.
///
/// The id itself breaks a tie. The schema's pattern allows a leading zero, so `FRK-007` and `FRK-7`
/// are two spellings of one number; this store never writes one, but a log written by another tool
/// could, and two rows whose order is whatever SQLite happens to return is not an order.
const BY_NUMBER: &str = "CAST(substr(task_id, ?1) AS INTEGER), task_id";

/// Where the number starts in a task id, counting from one as `substr` does: past the prefix and
/// the dash that follows it. Taken from the prefix rather than written down again.
///
/// A byte length, where `substr` counts characters. The two agree for every prefix the contract
/// schema's pattern can spell, which is ASCII; a prefix outside it would need this to count
/// characters too.
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
    f64,
    Option<String>,
    Option<String>,
    i64,
    bool,
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
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
    ))
}

/// Reads one projected row back, refusing one it cannot.
///
/// A refusal here fails the whole board, which is what the step 02 review refused for the log's own
/// `read`. The difference is the repair: a board is derived, so `rebuild` throws these rows away and
/// reads the log again without parsing any of them, and `farik doctor` reaches a board in this state
/// that way. The log has no such door — a row of it that cannot be read is the only copy.
fn projection_of_row(row: ProjectedRow) -> Result<TaskProjection, StoreError> {
    let (
        task_id,
        kind,
        parent,
        title,
        status,
        risk,
        triaged,
        locked,
        updated_seq,
        cost_usd,
        assignee_id,
        reviewer_id,
        iteration,
        awaiting_integration,
    ) = row;
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
        cost_usd,
        assignee_id,
        reviewer_id,
        iteration: u32::try_from(iteration)
            .map_err(|_| refuse("iteration", &iteration.to_string()))?,
        awaiting_integration,
    })
}

/// Applies one event to the projection tables, leaving the cursor to the caller.
fn apply_to(transaction: &Transaction<'_>, event: &FarikEvent) -> Result<(), StoreError> {
    let seq = i64::try_from(event.envelope.seq).map_err(|_| StoreError::Sqlite {
        detail: format!(
            "event {} is past what the engine can hold",
            event.envelope.seq
        ),
    })?;
    if let EventBody::CostRecorded(body) = &event.body {
        // Before the guard below: a cost with no task still costs its agent, session, and day.
        write_cost(transaction, event, body, seq)?;
    }
    let Some(task_id) = &event.envelope.ids.task_id else {
        // Only the kinds that are about one contract touch the board, and the protocol crate
        // refuses one of those without a task id. The rest — a scan, a team, a criterion library,
        // a drift report — are about the project.
        return Ok(());
    };
    let id = task_id.to_string();
    match &event.body {
        EventBody::TaskCreated(body) => write_summary(transaction, &id, &body.summary, seq),
        EventBody::ContractWritten(body) => write_summary(transaction, &id, &body.summary, seq),
        EventBody::RequestTriaged(body) => {
            // Triage decides the kind as well as recording that it happened (5.16 item 1).
            let kind = match body.size {
                RequestTriagedBodySize::Large => TaskKind::Epic,
                RequestTriagedBodySize::Small => TaskKind::Task,
            };
            update(
                transaction,
                "UPDATE task_projections SET triaged = 1, kind = ?2, updated_seq = ?3
                 WHERE task_id = ?1",
                (&id, kind.to_string(), seq),
            )
        }
        EventBody::ContractLocked(_) => set_locked(transaction, &id, true, seq),
        EventBody::ContractUnlocked(_) => set_locked(transaction, &id, false, seq),
        EventBody::TaskTransitioned(body) => {
            // The wire's status is the contract schema's own list, which a test in
            // `farik-protocol` holds to it, so every value it can carry reads here.
            let to = TaskStatus::from_str(&body.to.to_string()).map_err(|_| {
                StoreError::InvalidEvent {
                    detail: format!("event {seq} moves {id} to {}, which is no status", body.to),
                }
            })?;
            update(
                transaction,
                // Nothing leaves `accepted` (5.2), so only a move into it touches the flag; the
                // row's kind says whether there is a branch to integrate.
                "UPDATE task_projections
                 SET status = ?2, assignee_id = ?3, reviewer_id = ?4, iteration = ?5,
                     updated_seq = ?6,
                     awaiting_integration = CASE WHEN ?2 = 'accepted' THEN kind = 'task'
                                                 ELSE awaiting_integration END
                 WHERE task_id = ?1",
                (
                    &id,
                    to.to_string(),
                    body.assignee.as_ref(),
                    body.reviewer.as_ref(),
                    body.iteration,
                    seq,
                ),
            )
        }
        EventBody::TaskIntegrated(_) => update(
            transaction,
            "UPDATE task_projections SET awaiting_integration = 0, updated_seq = ?2
             WHERE task_id = ?1",
            (&id, seq),
        ),
        EventBody::DriftDetected(_)
        | EventBody::PullRequestOpened(_)
        | EventBody::ProjectScanned(_)
        | EventBody::TeamUpdated(_)
        | EventBody::CriteriaUpdated(_)
        | EventBody::CostRecorded(_)
        | EventBody::BudgetExhausted(_)
        | EventBody::TransitionRefused(_)
        | EventBody::EscalationRaised(_)
        | EventBody::ContractEvaluated(_)
        | EventBody::CriterionRecorded(_)
        | EventBody::NoteWritten(_)
        | EventBody::ReviewRecorded(_)
        | EventBody::QuestionAsked(_)
        | EventBody::ProductDocWritten(_)
        | EventBody::ToolCalled(_)
        | EventBody::ToolDenied(_)
        | EventBody::ToolReturned(_)
        | EventBody::SessionStarted(_)
        | EventBody::SessionEnded(_) => Ok(()),
    }
}

/// Keeps one `cost.recorded` as a row, dated by the UTC day it was recorded on.
fn write_cost(
    transaction: &Transaction<'_>,
    event: &FarikEvent,
    body: &CostRecordedBody,
    seq: i64,
) -> Result<(), StoreError> {
    let ids = &event.envelope.ids;
    transaction.execute(
        "INSERT INTO cost_records (seq, task_id, agent_id, session_id, day, purpose, model_id,
                                   input_tokens, output_tokens, cost_usd)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        (
            seq,
            ids.task_id.as_ref().map(|id| id.to_string()),
            ids.agent_id.as_ref(),
            ids.session_id.as_ref(),
            event.envelope.recorded_at.date_naive().to_string(),
            body.purpose.to_string(),
            body.model_id.as_str(),
            body.usage.input_tokens,
            body.usage.output_tokens,
            body.cost_usd,
        ),
    )?;
    Ok(())
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

fn set_locked(
    transaction: &Transaction<'_>,
    task_id: &str,
    locked: bool,
    seq: i64,
) -> Result<(), StoreError> {
    update(
        transaction,
        "UPDATE task_projections SET locked = ?2, updated_seq = ?3 WHERE task_id = ?1",
        (task_id, locked, seq),
    )
}

/// Runs an update that has nothing to say about a contract the board has never heard of.
///
/// A lock, an unlock or a triage names a contract some earlier event created. When no row matches,
/// the log is missing that earlier event, and there is nothing this table can invent: a row needs a
/// title, a status and a risk, and only a summary carries them. Reconciliation is what reports it.
fn update(
    transaction: &Transaction<'_>,
    sql: &str,
    parameters: impl rusqlite::Params,
) -> Result<(), StoreError> {
    transaction.execute(sql, parameters)?;
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

    use super::{
        Arc, CostProjection, CostScope, EventLog, FarikEvent, Projections, TaskProjection,
        open_projections,
    };
    use crate::error::StoreError;
    use crate::event_log::{IN_MEMORY, open_event_log};
    use crate::migrations;
    use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};

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
        event.ids.task_id = Some(task_id.parse().expect("a task id"));
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
            ids: event.envelope.ids,
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
                cost_usd: 0.0,
                assignee_id: None,
                reviewer_id: None,
                iteration: 0,
                awaiting_integration: false,
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
                cost_usd: 0.0,
                assignee_id: None,
                reviewer_id: None,
                iteration: 0,
                awaiting_integration: false,
            }
        );
    }

    #[test]
    fn moves_a_task_on_the_board_when_it_transitions() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        // The fixture is `ready -> assigned` to dev-a, reviewed by dev-b, at iteration 0.
        let moved = record(
            &log,
            &projections,
            &about(EventKind::TaskTransitioned, "FRK-1"),
        );
        let row = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
        assert_eq!(row.iteration, 0);
        assert_eq!(row.updated_seq, moved.envelope.seq);
    }

    #[test]
    fn leaves_the_board_alone_on_a_refusal() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        let before = projections.board().expect("the board reads");
        let refused = record(
            &log,
            &projections,
            &about(EventKind::TransitionRefused, "FRK-1"),
        );
        assert_eq!(projections.board().expect("the board reads"), before);
        assert_eq!(
            projections.cursor().expect("the cursor reads"),
            refused.envelope.seq
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

    #[test]
    fn takes_the_kind_and_the_flag_from_the_triage() {
        // Triage decides whether a request is an epic or a task, and the board is where a user sees
        // that it has happened at all (`docs/SPEC.md` 5.16 item 1).
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        let triaged = record(
            &log,
            &projections,
            &about(EventKind::RequestTriaged, "FRK-1"),
        );
        let task = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("the contract is on the board");
        assert!(task.triaged);
        assert_eq!(task.kind, TaskKind::Task, "small is a task");
        assert_eq!(task.updated_seq, triaged.envelope.seq);

        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
        let mut large = about(EventKind::RequestTriaged, "FRK-2");
        large.body = event_from_value(&{
            let mut wire = an_event_wire(EventKind::RequestTriaged);
            wire["task_id"] = json!("FRK-2");
            wire["body"]["size"] = json!("large");
            wire
        })
        .expect("the fixture is schema-valid")
        .body;
        record(&log, &projections, &large);
        assert_eq!(
            projections
                .task(&"FRK-2".parse().expect("a task id"))
                .expect("the read works")
                .expect("on the board")
                .kind,
            TaskKind::Epic,
            "large is an epic"
        );
    }

    #[test]
    fn says_who_holds_a_contract_the_human_locked_and_gave_back() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        let id: TaskId = "FRK-1".parse().expect("a task id");
        let held = |projections: &Projections| {
            projections
                .task(&id)
                .expect("the read works")
                .expect("on the board")
                .locked
        };
        record(
            &log,
            &projections,
            &about(EventKind::ContractLocked, "FRK-1"),
        );
        assert!(held(&projections), "locked");
        record(
            &log,
            &projections,
            &about(EventKind::ContractUnlocked, "FRK-1"),
        );
        assert!(!held(&projections), "given back");
    }

    #[test]
    fn has_nothing_to_say_about_a_contract_whose_first_event_is_missing() {
        // A lock names a contract some earlier event created, and a row needs a title, a status and
        // a risk that only a summary carries. Refusing here would make one row unread the whole
        // board; reconciliation is what reports a log that cannot be right.
        let (log, projections) = a_board();
        record(
            &log,
            &projections,
            &about(EventKind::ContractLocked, "FRK-4"),
        );
        assert_eq!(projections.board().expect("the board reads"), Vec::new());
        assert_eq!(projections.cursor().expect("the cursor reads"), 1);
    }

    #[test]
    fn opens_the_projections_of_a_log_that_has_read_nothing_yet() {
        // The projection tables are part of the log's own database, applied as a migration like
        // everything else in it, so opening the log is what makes them.
        let (log, projections) = a_board();
        assert_eq!(projections.cursor().expect("the cursor reads"), 0);
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
        assert_eq!(migrations::known_versions(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn catches_up_with_everything_the_log_holds_when_it_is_opened() {
        // The projections are derived, so a process that appended and stopped before projecting has
        // left work behind rather than damage. Opening is where it is done.
        let log = a_log();
        for id in ["FRK-1", "FRK-2"] {
            log.append(&about(EventKind::TaskCreated, id))
                .expect("appends");
        }
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        assert_eq!(
            ids_of(&projections.board().expect("the board reads")),
            ["FRK-1", "FRK-2"]
        );
        assert_eq!(projections.cursor().expect("the cursor reads"), 2);
        // And opening again reads nothing twice.
        let again = open_projections(Arc::clone(&log)).expect("the projections open again");
        assert_eq!(again.cursor().expect("the cursor reads"), 2);
        assert_eq!(again.board().expect("the board reads").len(), 2);
    }

    #[test]
    fn applies_one_event_once_however_often_it_is_handed_over() {
        // A caller that both subscribes and catches up on open hands the same append over twice.
        let (log, projections) = a_board();
        let filed = record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        let rewritten = record(
            &log,
            &projections,
            &written("FRK-1", "Add a logout page", "refining", "low", None),
        );
        projections.apply(&filed).expect("the first one again");
        let task = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert_eq!(
            task.title, "Add a logout page",
            "the later event still stands"
        );
        assert_eq!(task.updated_seq, rewritten.envelope.seq);
        assert_eq!(projections.cursor().expect("the cursor reads"), 2);
    }

    #[test]
    fn keeps_the_triage_and_the_lock_when_the_contract_is_written_afterwards() {
        // A summary carries a kind, a parent, a title, a status and a risk, and nothing else: the
        // triage and the lock have their own events, so a summary must not speak for them. Triage
        // always comes first (5.16 item 1, and the `draft -> refining` gate of 5.2), so a summary
        // that reset the flag would un-triage every contract on the board.
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        record(
            &log,
            &projections,
            &about(EventKind::RequestTriaged, "FRK-1"),
        );
        record(
            &log,
            &projections,
            &about(EventKind::ContractLocked, "FRK-1"),
        );
        record(
            &log,
            &projections,
            &written("FRK-1", "Add a logout page", "refining", "low", None),
        );
        let task = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert!(task.triaged, "the triage still stands");
        assert!(task.locked, "and so does the lock");
        assert_eq!(task.title, "Add a logout page", "and the rewrite landed");
    }

    #[test]
    fn keeps_the_cursor_to_one_row_and_every_column_to_its_type() {
        // What STRICT and the CHECKs are for: a second cursor would make "how far have the
        // projections read" a question with two answers, and a flag that is neither 0 nor 1 is not
        // a flag. SQLite has no boolean, so the CHECK is the type.
        let (log, _projections) = a_board();
        let connection = log.connection();
        let refuses = |sql: &str, what: &str, says: &str| {
            let refusal = connection.execute(sql, ()).expect_err(what);
            assert!(refusal.to_string().contains(says), "{what}: {refusal}");
        };
        refuses(
            "INSERT INTO projection_cursor (id, seq) VALUES (2, 7)",
            "a second cursor",
            "CHECK",
        );
        refuses(
            "INSERT INTO projection_cursor (id, seq) VALUES (1, 'soon')",
            "a word where a sequence number belongs",
            "cannot store TEXT value",
        );
        refuses(
            "INSERT INTO task_projections
                 (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
             VALUES ('FRK-1', 'task', NULL, x'00', 'draft', 'low', 0, 0, 1)",
            "a blob where a title belongs",
            "cannot store BLOB value",
        );
        refuses(
            "INSERT INTO task_projections
                 (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
             VALUES ('FRK-2', 'task', NULL, 'a title', 'draft', 'low', 2, 0, 1)",
            "a flag that is neither 0 nor 1",
            "CHECK",
        );
    }

    #[test]
    fn reads_what_it_was_handed_out_of_order_rather_than_stepping_over_it() {
        // Two processes each append and project what they appended, so the projections can be
        // handed event 3 before event 2. A cursor that moved to 3 would leave the board short of
        // the log for good, with nothing to notice — so the events in between are read from the
        // log, which is the one place they certainly are.
        let (log, projections) = a_board();
        let events: Vec<FarikEvent> = ["FRK-1", "FRK-2", "FRK-3"]
            .iter()
            .map(|id| {
                log.append(&about(EventKind::TaskCreated, id))
                    .expect("appends")
            })
            .collect();
        projections.apply(&events[0]).expect("projects the first");
        projections.apply(&events[2]).expect("projects the third");
        assert_eq!(
            ids_of(&projections.board().expect("the board reads")),
            ["FRK-1", "FRK-2", "FRK-3"],
            "the second was read from the log rather than stepped over"
        );
        assert_eq!(projections.cursor().expect("the cursor reads"), 3);
        // And the one that arrives late changes nothing, because it is already in.
        projections.apply(&events[1]).expect("projects the second");
        assert_eq!(projections.board().expect("the board reads").len(), 3);
        assert_eq!(projections.cursor().expect("the cursor reads"), 3);
    }

    #[test]
    fn reads_nothing_when_the_board_is_already_current() {
        // What `docs/SPEC.md` section 10 asks of this step: opening a project whose board is up to
        // date costs nothing, rather than replaying every event to arrive at the board it has.
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        assert_eq!(
            projections.catch_up().expect("catching up reads the log"),
            0,
            "nothing to catch up on"
        );
    }

    #[test]
    fn builds_the_board_again_from_the_log_when_it_is_told_to() {
        // Nothing in the tables is a source of truth, so this is the repair for a row that drifted
        // for any reason at all.
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        log.connection()
            .execute(
                "UPDATE task_projections SET title = 'something else', status = 'accepted'
                 WHERE task_id = 'FRK-1'",
                (),
            )
            .expect("a row is changed by hand");
        log.connection()
            .execute(
                "INSERT INTO task_projections
                     (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                 VALUES ('FRK-7', 'task', NULL, 'never happened', 'draft', 'low', 0, 0, 1)",
                (),
            )
            .expect("and a row is invented");
        projections.rebuild().expect("the board is built again");
        let board = projections.board().expect("the board reads");
        assert_eq!(ids_of(&board), ["FRK-1"], "the invented row is gone");
        assert_eq!(board[0].title, "Add a login page");
        assert_eq!(board[0].status, TaskStatus::Draft);
        assert_eq!(projections.cursor().expect("the cursor reads"), 1);
    }

    /// A `task.transitioned` of `task_id` from `from` into `to`, otherwise the fixture's move.
    fn moved(task_id: &str, from: &str, to: &str) -> NewEvent {
        let mut wire = an_event_wire(EventKind::TaskTransitioned);
        wire["task_id"] = json!(task_id);
        wire["body"]["from"] = json!(from);
        wire["body"]["to"] = json!(to);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        }
    }

    fn awaiting(projections: &Projections, task_id: &str) -> bool {
        projections
            .task(&task_id.parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board")
            .awaiting_integration
    }

    #[test]
    fn awaits_integration_from_acceptance_until_integrated() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        record(&log, &projections, &moved("FRK-1", "ready", "verifying"));
        assert!(!awaiting(&projections, "FRK-1"));
        record(&log, &projections, &moved("FRK-1", "verifying", "accepted"));
        assert!(awaiting(&projections, "FRK-1"));
        let integrated = record(
            &log,
            &projections,
            &about(EventKind::TaskIntegrated, "FRK-1"),
        );
        assert!(!awaiting(&projections, "FRK-1"));
        let row = projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board");
        assert_eq!(row.status, TaskStatus::Accepted);
        assert_eq!(row.updated_seq, integrated.envelope.seq);

        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
        let mut large = an_event_wire(EventKind::RequestTriaged);
        large["task_id"] = json!("FRK-2");
        large["body"]["size"] = json!("large");
        let large = event_from_value(&large).expect("the fixture is schema-valid");
        record(
            &log,
            &projections,
            &NewEvent {
                recorded_at: large.envelope.recorded_at,
                ids: large.envelope.ids,
                body: large.body,
            },
        );
        record(&log, &projections, &moved("FRK-2", "verifying", "accepted"));
        assert!(
            !awaiting(&projections, "FRK-2"),
            "an epic has no branch of its own"
        );
    }

    #[test]
    fn reads_an_older_accepted_task_as_awaiting() {
        let directory = std::env::temp_dir().join(format!(
            "farik-older-accepted-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory under the temporary directory");
        let path = directory.join("farik.db");
        {
            let mut connection = rusqlite::Connection::open(&path).expect("the database opens");
            migrations::apply_through(&mut connection, 4, at(9)).expect("version 4 applies");
            connection
                .execute_batch(
                    "INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                     VALUES ('FRK-1', 'task', NULL, 'a task', 'accepted', 'low', 1, 0, 1),
                            ('FRK-2', 'epic', NULL, 'an epic', 'accepted', 'low', 1, 0, 2),
                            ('FRK-3', 'task', NULL, 'a task', 'verifying', 'low', 1, 0, 3);",
                )
                .expect("the older rows are written");
        }
        let log = Arc::new(open_event_log(&path, at(10)).expect("the log opens"));
        let projections = Projections { log };

        assert!(awaiting(&projections, "FRK-1"));
        assert!(!awaiting(&projections, "FRK-2"));
        assert!(!awaiting(&projections, "FRK-3"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A `cost.recorded` for `task_id` (or no task), by `agent` in `session`, recorded at ten on
    /// `day`, of `usd` dollars and `input` and `output` tokens.
    fn cost(
        task_id: Option<&str>,
        agent: &str,
        session: &str,
        day: &str,
        (usd, input, output): (f64, u64, u64),
    ) -> NewEvent {
        let mut wire = an_event_wire(EventKind::CostRecorded);
        if let Some(task_id) = task_id {
            wire["task_id"] = json!(task_id);
        }
        wire["agent_id"] = json!(agent);
        wire["session_id"] = json!(session);
        wire["recorded_at"] = json!(format!("{day}T10:00:00Z"));
        wire["body"]["cost_usd"] = json!(usd);
        wire["body"]["usage"]["input_tokens"] = json!(input);
        wire["body"]["usage"]["output_tokens"] = json!(output);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        }
    }

    fn row(
        scope: CostScope,
        key: &str,
        (usd, input_tokens, output_tokens, sessions): (f64, u64, u64, u32),
    ) -> CostProjection {
        CostProjection {
            scope,
            key: key.to_string(),
            usd,
            input_tokens,
            output_tokens,
            sessions,
        }
    }

    fn cost_on_board(projections: &Projections, task_id: &str) -> f64 {
        projections
            .task(&task_id.parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board")
            .cost_usd
    }

    #[test]
    fn sums_a_tasks_costs_on_the_board() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
        for usd in [0.25, 0.5] {
            let spent = cost(Some("FRK-1"), "a", "s1", "2026-09-22", (usd, 1, 1));
            record(&log, &projections, &spent);
        }
        assert!((cost_on_board(&projections, "FRK-1") - 0.75).abs() < f64::EPSILON);
        assert!(cost_on_board(&projections, "FRK-2").abs() < f64::EPSILON);
    }

    /// The three records `groups_costs_by_each_scope` describes.
    fn three_costs_for_one_task(log: &EventLog, projections: &Projections) {
        record(log, projections, &about(EventKind::TaskCreated, "FRK-1"));
        for spent in [
            cost(Some("FRK-1"), "a", "s1", "2026-09-21", (1.0, 100, 10)),
            cost(Some("FRK-1"), "a", "s2", "2026-09-22", (2.0, 200, 20)),
            cost(Some("FRK-1"), "b", "s3", "2026-09-22", (4.0, 400, 40)),
        ] {
            record(log, projections, &spent);
        }
    }

    #[test]
    fn groups_costs_by_each_scope() {
        let (log, projections) = a_board();
        three_costs_for_one_task(&log, &projections);
        let costs = |scope| projections.costs(scope).expect("the costs read");
        assert_eq!(
            costs(CostScope::Agent),
            vec![
                row(CostScope::Agent, "a", (3.0, 300, 30, 2)),
                row(CostScope::Agent, "b", (4.0, 400, 40, 1)),
            ]
        );
        assert_eq!(
            costs(CostScope::Session),
            vec![
                row(CostScope::Session, "s1", (1.0, 100, 10, 1)),
                row(CostScope::Session, "s2", (2.0, 200, 20, 1)),
                row(CostScope::Session, "s3", (4.0, 400, 40, 1)),
            ]
        );
        assert_eq!(
            costs(CostScope::Day),
            vec![
                row(CostScope::Day, "2026-09-21", (1.0, 100, 10, 1)),
                row(CostScope::Day, "2026-09-22", (6.0, 600, 60, 2)),
            ]
        );
    }

    #[test]
    fn counts_a_cost_with_no_task_in_every_other_scope() {
        let (log, projections) = a_board();
        record(
            &log,
            &projections,
            &cost(None, "a", "s1", "2026-09-22", (1.0, 100, 10)),
        );
        let one = (1.0, 100, 10, 1);
        assert_eq!(
            projections.costs(CostScope::Day).expect("the costs read"),
            vec![row(CostScope::Day, "2026-09-22", one)]
        );
        assert_eq!(
            projections.costs(CostScope::Agent).expect("the costs read"),
            vec![row(CostScope::Agent, "a", one)]
        );
        assert_eq!(
            projections
                .costs(CostScope::Session)
                .expect("the costs read"),
            vec![row(CostScope::Session, "s1", one)]
        );
        assert_eq!(
            projections.costs(CostScope::Task).expect("the costs read"),
            Vec::new()
        );
    }

    #[test]
    fn counts_a_tasks_distinct_sessions() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        for session in ["a", "a", "b"] {
            let spent = cost(Some("FRK-1"), "x", session, "2026-09-22", (1.0, 1, 1));
            record(&log, &projections, &spent);
        }
        let tasks = projections.costs(CostScope::Task).expect("the costs read");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].key, "FRK-1");
        assert_eq!(tasks[0].sessions, 2);
    }

    #[test]
    fn dates_a_cost_by_the_utc_day_it_was_recorded_on() {
        let (log, projections) = a_board();
        for at in ["2026-09-21T23:30:00Z", "2026-09-22T00:30:00Z"] {
            let mut spent = cost(None, "a", "s1", "2026-09-21", (1.0, 1, 1));
            spent.recorded_at = at.parse().expect("a timestamp");
            record(&log, &projections, &spent);
        }
        let one = (1.0, 1, 1, 1);
        assert_eq!(
            projections.costs(CostScope::Day).expect("the costs read"),
            vec![
                row(CostScope::Day, "2026-09-21", one),
                row(CostScope::Day, "2026-09-22", one),
            ]
        );
    }

    #[test]
    fn orders_task_costs_by_the_number_in_the_id() {
        let (log, projections) = a_board();
        for task_id in ["FRK-10", "FRK-9"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task_id));
            let spent = cost(Some(task_id), "a", "s1", "2026-09-22", (1.0, 1, 1));
            record(&log, &projections, &spent);
        }
        let keys: Vec<String> = projections
            .costs(CostScope::Task)
            .expect("the costs read")
            .into_iter()
            .map(|row| row.key)
            .collect();
        assert_eq!(keys, ["FRK-9", "FRK-10"]);
    }

    #[test]
    fn rebuilds_costs_from_the_log() {
        let (log, projections) = a_board();
        three_costs_for_one_task(&log, &projections);
        let before = projections.costs(CostScope::Task).expect("the costs read");
        projections.rebuild().expect("the board is built again");
        assert_eq!(
            projections.costs(CostScope::Task).expect("the costs read"),
            before
        );
        assert_eq!(
            before,
            vec![row(CostScope::Task, "FRK-1", (7.0, 700, 70, 3))]
        );
    }
}
