//! The board, read from the log rather than scanned out of it (`docs/SPEC.md` sections 8.4 and 10).

use std::str::FromStr;
use std::sync::Arc;

use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};
use farik_protocol::event::{
    ContractSummary, ContractSummaryKind, ContractSummaryRisk, ContractSummaryStatus,
    CostRecordedBody, EscalationRaisedBodyReason, EventBody, FarikEvent, RequestTriagedBodySize,
    TaskStatusWire, TaskTransitionedBody, TransitionActorWire,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::StoreError;
use crate::event_log::{EventLog, EventQuery, TASK_ID_PREFIX};

/// What a board shows about one contract, as the log left it.
///
/// Every field comes from an event a command or the governor emits.
// The flags are what a board shows, each a yes or no a row is filtered by; a state machine of them
// would be a second copy of the lifecycle the status already holds.
#[allow(clippy::struct_excessive_bools)]
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
    /// Whether a question asked about the contract is still unanswered (5.7): a `question.asked`
    /// counts one up and a `question.answered` one down.
    pub waiting_on_human: bool,
    /// Whether the contract waits for the human's approval (5.16 item 2): set by an
    /// `escalation.raised` with reason `approval` or `risk_gate`, cleared by its next move.
    pub awaiting_approval: bool,
    /// How many times the contract has moved into `verifying`, whoever moved it (F17).
    pub verifications: u32,
    /// How many times the contract has moved into `rejected` (F17).
    pub rejections: u32,
    /// How many times the human had to act where the process did not ask them to (F17): every
    /// `escalation.raised` but an `approval` or a `risk_gate`, and every move the human asked for
    /// that neither leaves nor enters `escalated`.
    pub interventions: u32,
    /// The sprint the task is in: set by `sprint.planned`, cleared by the `sprint.ended` that
    /// leaves it (5.5).
    pub sprint: Option<String>,
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
    /// By the sprint the cost's task was in when the cost was recorded. A cost with no task, or of
    /// a task in no sprint, is in no sprint's row.
    Sprint,
}

/// A sprint, as its `sprint.started` and `sprint.ended` left it.
#[derive(Debug, Clone, PartialEq)]
pub struct SprintProjection {
    /// The sprint's id, `S<n>`.
    pub sprint_id: String,
    /// What it may spend, or nothing when it has no budget of its own (ADR 0015).
    pub budget_usd: Option<f64>,
    /// Whether it is still open: started and not yet ended.
    pub open: bool,
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
            transaction.execute("DELETE FROM sprints", ())?;
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
            CostScope::Sprint => ("sprint", "sprint"),
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

    /// The sprint that is open, or nothing when none is.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the read fails.
    pub fn open_sprint(&self) -> Result<Option<SprintProjection>, StoreError> {
        let connection = self.connection();
        // One is open at a time; were the log to hold two, the one started last is the open one.
        let sprint = connection
            .query_row(
                "SELECT sprint_id, budget_usd FROM sprints WHERE open = 1
                 ORDER BY rowid DESC LIMIT 1",
                (),
                |row| {
                    Ok(SprintProjection {
                        sprint_id: row.get(0)?,
                        budget_usd: row.get(1)?,
                        open: true,
                    })
                },
            )
            .optional()?;
        Ok(sprint)
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
    pub(crate) fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.log.connection()
    }
}

const SELECT_PROJECTION: &str = "SELECT task_id, kind, parent, title, status, risk, triaged, \
                                 locked, updated_seq, \
                                 (SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_records \
                                  WHERE cost_records.task_id = task_projections.task_id), \
                                 assignee_id, reviewer_id, iteration, awaiting_integration, \
                                 open_questions > 0, awaiting_approval, verifications, \
                                 rejections, interventions, sprint \
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
    bool,
    bool,
    i64,
    i64,
    i64,
    Option<String>,
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
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
        row.get(17)?,
        row.get(18)?,
        row.get(19)?,
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
        waiting_on_human,
        awaiting_approval,
        verifications,
        rejections,
        interventions,
        sprint,
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
        waiting_on_human,
        awaiting_approval,
        verifications: u32::try_from(verifications)
            .map_err(|_| refuse("verifications", &verifications.to_string()))?,
        rejections: u32::try_from(rejections)
            .map_err(|_| refuse("rejections", &rejections.to_string()))?,
        interventions: u32::try_from(interventions)
            .map_err(|_| refuse("interventions", &interventions.to_string()))?,
        sprint,
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
    match &event.body {
        // Before the guard below: a cost with no task still costs its agent, session, and day.
        EventBody::CostRecorded(body) => write_cost(transaction, event, body, seq)?,
        // About no one contract, so they name their tasks in the body rather than the envelope.
        EventBody::SprintStarted(_) | EventBody::SprintPlanned(_) | EventBody::SprintEnded(_) => {
            return apply_sprint(transaction, &event.body, seq);
        }
        _ => {}
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
        EventBody::TaskTransitioned(body) => apply_move(transaction, &id, body, seq),
        EventBody::TaskIntegrated(_) => update(
            transaction,
            "UPDATE task_projections SET awaiting_integration = 0, updated_seq = ?2
             WHERE task_id = ?1",
            (&id, seq),
        ),
        EventBody::QuestionAsked(_)
        | EventBody::QuestionAnswered(_)
        | EventBody::EscalationRaised(_) => apply_waiting(transaction, &id, &event.body, seq),
        EventBody::DriftDetected(_)
        | EventBody::PullRequestOpened(_)
        | EventBody::ProjectScanned(_)
        | EventBody::TeamUpdated(_)
        | EventBody::CriteriaUpdated(_)
        | EventBody::CostRecorded(_)
        | EventBody::BudgetExhausted(_)
        | EventBody::TransitionRefused(_)
        | EventBody::ContractEvaluated(_)
        | EventBody::ContractJudged(_)
        | EventBody::CriterionRecorded(_)
        | EventBody::NoteWritten(_)
        | EventBody::ReviewRecorded(_)
        | EventBody::ProductDocWritten(_)
        | EventBody::ToolCalled(_)
        | EventBody::ToolDenied(_)
        | EventBody::ToolReturned(_)
        | EventBody::SessionStarted(_)
        | EventBody::SessionEnded(_)
        | EventBody::HumanAccepted(_)
        | EventBody::EscalationResolved(_)
        | EventBody::AgentUpdated(_)
        | EventBody::SprintStarted(_)
        | EventBody::SprintPlanned(_)
        | EventBody::SprintEnded(_)
        | EventBody::AgentSlept(_)
        | EventBody::MessagePosted(_)
        | EventBody::RetroAppended(_)
        | EventBody::EscalationAged(_)
        | EventBody::MemoryWritten(_) => Ok(()),
    }
}

/// A move of the task's status, and what it changes on the board: its assignee, reviewer, and
/// iteration, the flags a move clears or sets, and the counts of verifications, rejections, and
/// the human's interventions.
fn apply_move(
    transaction: &Transaction<'_>,
    id: &str,
    body: &TaskTransitionedBody,
    seq: i64,
) -> Result<(), StoreError> {
    // The wire's status is the contract schema's own list, which a test in
    // `farik-protocol` holds to it, so every value it can carry reads here.
    let to = TaskStatus::from_str(&body.to.to_string()).map_err(|_| StoreError::InvalidEvent {
        detail: format!("event {seq} moves {id} to {}, which is no status", body.to),
    })?;
    // A move the human asked for is an intervention (F17) unless it answers an
    // escalation, which was counted when it was raised, or makes one, which its
    // `explicit_request` escalation counts.
    let is_intervention = body.actor == TransitionActorWire::Human
        && body.from != TaskStatusWire::Escalated
        && body.to != TaskStatusWire::Escalated;
    update(
        transaction,
        // Nothing leaves `accepted` (5.2), so only a move into it touches the flag; the
        // row's kind says whether there is a branch to integrate.
        "UPDATE task_projections
         SET status = ?2, assignee_id = ?3, reviewer_id = ?4, iteration = ?5,
             updated_seq = ?6, awaiting_approval = 0,
             awaiting_integration = CASE WHEN ?2 = 'accepted' THEN kind = 'task'
                                         ELSE awaiting_integration END,
             verifications = verifications + (?2 = 'verifying'),
             rejections = rejections + (?2 = 'rejected'),
             interventions = interventions + ?7
         WHERE task_id = ?1",
        (
            id,
            to.to_string(),
            body.assignee.as_ref(),
            body.reviewer.as_ref(),
            body.iteration,
            seq,
            i64::from(is_intervention),
        ),
    )
}

/// The two columns that say what the board waits on the human for: an open question (5.7) and an
/// approval (5.16 item 2).
fn apply_waiting(
    transaction: &Transaction<'_>,
    id: &str,
    body: &EventBody,
    seq: i64,
) -> Result<(), StoreError> {
    match body {
        EventBody::QuestionAsked(_) => update(
            transaction,
            "UPDATE task_projections SET open_questions = open_questions + 1, updated_seq = ?2
             WHERE task_id = ?1",
            (id, seq),
        ),
        EventBody::QuestionAnswered(_) => update(
            transaction,
            "UPDATE task_projections SET open_questions = max(0, open_questions - 1),
                 updated_seq = ?2
             WHERE task_id = ?1",
            (id, seq),
        ),
        EventBody::EscalationRaised(body) => {
            // The two reasons of the `ContractRequiresHuman` gate: the contract waits on an
            // approval the process asks for by design, which is no intervention (F17). Any other
            // escalation is one, and waits on no approval.
            let is_approval = matches!(
                body.reason,
                EscalationRaisedBodyReason::Approval | EscalationRaisedBodyReason::RiskGate
            );
            update(
                transaction,
                "UPDATE task_projections
                 SET awaiting_approval = ?2, interventions = interventions + (1 - ?2),
                     updated_seq = ?3
                 WHERE task_id = ?1",
                (id, is_approval, seq),
            )
        }
        _ => Ok(()),
    }
}

/// The sprints table and each task's sprint (5.5): a start opens a sprint, a plan puts each of its
/// tasks in it, and an end closes it and takes out each task it left.
///
/// The log's order settles a plan racing an end, whichever process wrote each: a plan into a sprint
/// that has ended, or never started, puts nothing in it, and an end takes out every task the board
/// holds in it that is neither accepted nor cancelled, whether or not its `left` names the task.
fn apply_sprint(
    transaction: &Transaction<'_>,
    body: &EventBody,
    seq: i64,
) -> Result<(), StoreError> {
    match body {
        EventBody::SprintStarted(body) => update(
            transaction,
            "INSERT INTO sprints (sprint_id, budget_usd, open) VALUES (?1, ?2, 1)
             ON CONFLICT (sprint_id) DO UPDATE SET budget_usd = ?2, open = 1",
            (body.sprint_id.as_str(), body.budget_usd),
        ),
        EventBody::SprintPlanned(body) => {
            for task_id in &body.task_ids {
                update(
                    transaction,
                    "UPDATE task_projections SET sprint = ?2, updated_seq = ?3
                     WHERE task_id = ?1
                       AND EXISTS (SELECT 1 FROM sprints WHERE sprint_id = ?2 AND open = 1)",
                    (task_id.as_str(), body.sprint_id.as_str(), seq),
                )?;
            }
            Ok(())
        }
        EventBody::SprintEnded(body) => {
            update(
                transaction,
                "UPDATE sprints SET open = 0 WHERE sprint_id = ?1",
                (body.sprint_id.as_str(),),
            )?;
            update(
                transaction,
                "UPDATE task_projections SET sprint = NULL, updated_seq = ?2
                 WHERE sprint = ?1 AND status NOT IN ('accepted', 'cancelled')",
                (body.sprint_id.as_str(), seq),
            )
        }
        _ => Ok(()),
    }
}

/// Keeps one `cost.recorded` as a row, dated by the UTC day it was recorded on, with the sprint its
/// task is in now, so that the cost stays with that sprint after the task leaves it.
fn write_cost(
    transaction: &Transaction<'_>,
    event: &FarikEvent,
    body: &CostRecordedBody,
    seq: i64,
) -> Result<(), StoreError> {
    let ids = &event.envelope.ids;
    transaction.execute(
        "INSERT INTO cost_records (seq, task_id, agent_id, session_id, day, purpose, model_id,
                                   input_tokens, output_tokens, cost_usd, sprint)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 (SELECT sprint FROM task_projections WHERE task_id = ?2))",
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
        Arc, CostProjection, CostScope, EventLog, FarikEvent, Projections, SprintProjection,
        TaskProjection, open_projections,
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
                waiting_on_human: false,
                awaiting_approval: false,
                verifications: 0,
                rejections: 0,
                interventions: 0,
                sprint: None,
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
                waiting_on_human: false,
                awaiting_approval: false,
                verifications: 0,
                rejections: 0,
                interventions: 0,
                sprint: None,
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
        assert_eq!(
            migrations::known_versions(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
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
            // Version 5 alone: a full open goes on to 0007, which empties the projections for a
            // replay, and this test is of the backfill an older database still runs on its way.
            migrations::apply_through(&mut connection, 5, at(10)).expect("version 5 applies");
            let awaiting = |task: &str| -> bool {
                connection
                    .query_row(
                        "SELECT awaiting_integration FROM task_projections WHERE task_id = ?1",
                        (task,),
                        |row| row.get(0),
                    )
                    .expect("the row reads")
            };
            assert!(awaiting("FRK-1"));
            assert!(!awaiting("FRK-2"));
            assert!(!awaiting("FRK-3"));
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The fixture event of `kind` about `task_id` with `body` in place of the fixture's.
    fn with_body(kind: EventKind, task_id: &str, body: serde_json::Value) -> NewEvent {
        let mut wire = an_event_wire(kind);
        wire["task_id"] = json!(task_id);
        wire["body"] = body;
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        }
    }

    fn row_of(projections: &Projections, task_id: &str) -> TaskProjection {
        projections
            .task(&task_id.parse().expect("a task id"))
            .expect("the read works")
            .expect("on the board")
    }

    #[test]
    fn waits_on_the_human_while_a_question_is_open() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        assert!(!row_of(&projections, "FRK-1").waiting_on_human);
        let first = record(
            &log,
            &projections,
            &about(EventKind::QuestionAsked, "FRK-1"),
        );
        let second = record(
            &log,
            &projections,
            &about(EventKind::QuestionAsked, "FRK-1"),
        );
        assert!(row_of(&projections, "FRK-1").waiting_on_human);

        let answer = |question: &FarikEvent| {
            with_body(
                EventKind::QuestionAnswered,
                "FRK-1",
                json!({
                    "question_id": question.envelope.seq,
                    "answer": "Yes.",
                    "answered_by": "human"
                }),
            )
        };
        record(&log, &projections, &answer(&first));
        assert!(
            row_of(&projections, "FRK-1").waiting_on_human,
            "one question is still open"
        );
        record(&log, &projections, &answer(&second));
        assert!(!row_of(&projections, "FRK-1").waiting_on_human);
    }

    #[test]
    fn awaits_approval_from_the_escalation_until_the_next_move() {
        let (log, projections) = a_board();
        for task in ["FRK-1", "FRK-2", "FRK-3"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task));
        }
        let escalation = |task: &str, reason: &str| {
            with_body(
                EventKind::EscalationRaised,
                task,
                json!({ "reason": reason, "detail": "contract_requires_human" }),
            )
        };
        record(&log, &projections, &escalation("FRK-1", "approval"));
        record(&log, &projections, &escalation("FRK-2", "risk_gate"));
        record(&log, &projections, &escalation("FRK-3", "iterations"));
        assert!(row_of(&projections, "FRK-1").awaiting_approval);
        assert!(row_of(&projections, "FRK-2").awaiting_approval);
        assert!(!row_of(&projections, "FRK-3").awaiting_approval);

        record(&log, &projections, &moved("FRK-1", "escalated", "ready"));
        assert!(!row_of(&projections, "FRK-1").awaiting_approval);
        assert!(row_of(&projections, "FRK-2").awaiting_approval);
    }

    #[test]
    fn reads_an_older_log_into_the_new_columns() {
        let directory = std::env::temp_dir().join(format!(
            "farik-older-human-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory under the temporary directory");
        let path = directory.join("farik.db");
        {
            let mut connection = rusqlite::Connection::open(&path).expect("the database opens");
            migrations::apply_through(&mut connection, 5, at(9)).expect("version 5 applies");
            connection
                .execute_batch(
                    "INSERT INTO events (seq, recorded_at, team_id, project_id, task_id, kind, body)
                     VALUES
                       (1, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-1', 'task.transitioned',
                        '{\"from\":\"refining\",\"to\":\"escalated\",\"actor\":\"governor\",\"requested_by\":\"governor\",\"gate\":\"contract_requires_human\",\"effects\":[\"raise_escalation\"],\"iteration\":0}'),
                       (2, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-1', 'escalation.raised',
                        '{\"reason\":\"approval\",\"detail\":\"contract_requires_human\"}'),
                       (3, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-2', 'question.asked',
                        '{\"question\":\"Should done.txt be empty?\",\"asked_by\":\"pm\"}'),
                       (4, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-3', 'escalation.raised',
                        '{\"reason\":\"approval\",\"detail\":\"contract_requires_human\"}'),
                       (5, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-3', 'task.transitioned',
                        '{\"from\":\"escalated\",\"to\":\"ready\",\"actor\":\"human\",\"requested_by\":\"human\",\"effects\":[],\"iteration\":0}'),
                       (6, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-4', 'escalation.raised',
                        '{\"reason\":\"approval\",\"detail\":\"contract_requires_human\"}'),
                       (7, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-4', 'escalation.raised',
                        '{\"reason\":\"iterations\",\"detail\":\"too many\"}');
                     INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                     VALUES ('FRK-1', 'epic', NULL, 'an epic', 'escalated', 'low', 1, 0, 2),
                            ('FRK-2', 'task', NULL, 'a task', 'refining', 'low', 1, 0, 3),
                            ('FRK-3', 'epic', NULL, 'an approved epic', 'ready', 'low', 1, 0, 5),
                            ('FRK-4', 'task', NULL, 'escalated again', 'escalated', 'low', 1, 0, 7);
                     INSERT INTO projection_cursor (id, seq) VALUES (1, 7);",
                )
                .expect("the older rows are written");
            // Version 6 alone, for the reason `reads_an_older_accepted_task_as_awaiting` gives.
            migrations::apply_through(&mut connection, 6, at(10)).expect("version 6 applies");
            let flags = |task: &str| -> (bool, bool) {
                connection
                    .query_row(
                        "SELECT awaiting_approval, open_questions > 0 FROM task_projections
                         WHERE task_id = ?1",
                        (task,),
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .expect("the row reads")
            };
            // (awaiting approval, waiting on the human)
            assert_eq!(flags("FRK-1"), (true, false));
            assert_eq!(flags("FRK-2"), (false, true));
            // A move after the approval's escalation ends it, and so does a later escalation.
            assert!(!flags("FRK-3").0);
            assert!(!flags("FRK-4").0);
        }
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

    /// A `task.transitioned` of `task_id` from `from` into `to`, asked for by `actor`.
    fn moved_by(task_id: &str, from: &str, to: &str, actor: &str) -> NewEvent {
        let mut wire = an_event_wire(EventKind::TaskTransitioned);
        wire["task_id"] = json!(task_id);
        wire["body"]["from"] = json!(from);
        wire["body"]["to"] = json!(to);
        wire["body"]["actor"] = json!(actor);
        wire["body"]["requested_by"] = json!(actor);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        }
    }

    fn escalated_for(task_id: &str, reason: &str) -> NewEvent {
        with_body(
            EventKind::EscalationRaised,
            task_id,
            json!({ "reason": reason, "detail": "a gate" }),
        )
    }

    /// The three counts of a row: verifications, rejections, interventions.
    fn counts_of(projections: &Projections, task_id: &str) -> (u32, u32, u32) {
        let row = row_of(projections, task_id);
        (row.verifications, row.rejections, row.interventions)
    }

    /// FRK-1 verified twice and rejected once between, by the agents alone.
    fn verified_twice(log: &EventLog, projections: &Projections) {
        record(log, projections, &about(EventKind::TaskCreated, "FRK-1"));
        for (from, to, actor) in [
            ("in_progress", "verifying", "assignee"),
            ("verifying", "rejected", "reviewer"),
            ("rejected", "in_progress", "governor"),
            ("in_progress", "verifying", "assignee"),
            ("verifying", "accepted", "product_manager"),
        ] {
            record(log, projections, &moved_by("FRK-1", from, to, actor));
        }
    }

    #[test]
    fn counts_each_move_into_verifying_and_into_rejected() {
        let (log, projections) = a_board();
        verified_twice(&log, &projections);
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
        assert_eq!(counts_of(&projections, "FRK-1"), (2, 1, 0));
        assert_eq!(counts_of(&projections, "FRK-2"), (0, 0, 0));
    }

    #[test]
    fn counts_every_escalation_but_the_two_the_process_asks_for() {
        let (log, projections) = a_board();
        for task in ["FRK-1", "FRK-2"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task));
        }
        for reason in [
            "budget",
            "sessions",
            "iterations",
            "blocker_age",
            "permission",
            "risk_gate",
            "approval",
            "readiness_failures",
            "integration",
            "explicit_request",
        ] {
            record(&log, &projections, &escalated_for("FRK-1", reason));
        }
        for reason in ["approval", "risk_gate"] {
            record(&log, &projections, &escalated_for("FRK-2", reason));
        }
        assert_eq!(row_of(&projections, "FRK-1").interventions, 8);
        assert_eq!(row_of(&projections, "FRK-2").interventions, 0);
    }

    #[test]
    fn counts_the_humans_own_moves_and_not_their_answers() {
        let (log, projections) = a_board();
        record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "in_progress", "blocked", "assignee"),
        );
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "blocked", "in_progress", "human"),
        );
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "in_progress", "escalated", "human"),
        );
        record(
            &log,
            &projections,
            &escalated_for("FRK-1", "explicit_request"),
        );
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "escalated", "in_progress", "human"),
        );
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "in_progress", "escalated", "governor"),
        );
        record(
            &log,
            &projections,
            &moved_by("FRK-1", "escalated", "cancelled", "human"),
        );
        record(
            &log,
            &projections,
            &about(EventKind::QuestionAsked, "FRK-1"),
        );
        record(
            &log,
            &projections,
            &about(EventKind::QuestionAnswered, "FRK-1"),
        );
        record(
            &log,
            &projections,
            &with_body(
                EventKind::HumanAccepted,
                "FRK-1",
                json!({ "subject": "contract", "accepted_by": "human" }),
            ),
        );
        record(
            &log,
            &projections,
            &with_body(
                EventKind::RequestTriaged,
                "FRK-1",
                json!({ "size": "small", "reason": "One page.", "triaged_by": "human" }),
            ),
        );
        record(
            &log,
            &projections,
            &about(EventKind::ContractLocked, "FRK-1"),
        );
        assert_eq!(row_of(&projections, "FRK-1").interventions, 2);
    }

    #[test]
    fn rebuilds_the_counts_from_the_log() {
        let (log, projections) = a_board();
        verified_twice(&log, &projections);
        projections.rebuild().expect("the board is built again");
        assert_eq!(counts_of(&projections, "FRK-1"), (2, 1, 0));
    }

    #[test]
    fn replays_an_older_project_into_the_new_counts() {
        use farik_protocol::event::fixtures::a_body_wire;

        let directory = std::env::temp_dir().join(format!(
            "farik-older-metrics-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory under the temporary directory");
        let path = directory.join("farik.db");
        {
            let mut connection = rusqlite::Connection::open(&path).expect("the database opens");
            migrations::apply_through(&mut connection, 6, at(9)).expect("version 6 applies");
            let into_verifying = |from: &str| {
                let mut body = a_body_wire(EventKind::TaskTransitioned);
                body["from"] = json!(from);
                body["to"] = json!("verifying");
                body["actor"] = json!("assignee");
                body
            };
            let mut escalation = a_body_wire(EventKind::EscalationRaised);
            escalation["reason"] = json!("blocker_age");
            let events = [
                (EventKind::TaskCreated, a_body_wire(EventKind::TaskCreated)),
                (EventKind::TaskTransitioned, into_verifying("in_progress")),
                (EventKind::TaskTransitioned, into_verifying("in_progress")),
                (EventKind::EscalationRaised, escalation),
                (
                    EventKind::CostRecorded,
                    a_body_wire(EventKind::CostRecorded),
                ),
            ];
            for (seq, (kind, body)) in (1_i64..).zip(events) {
                connection
                    .execute(
                        "INSERT INTO events
                             (seq, recorded_at, team_id, project_id, task_id, agent_id,
                              session_id, kind, body)
                         VALUES (?1, '2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-1', 'dev-a',
                                 's1', ?2, ?3)",
                        (seq, kind.to_string(), body.to_string()),
                    )
                    .expect("an older event is written");
            }
            connection
                .execute_batch(
                    "INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                     VALUES ('FRK-1', 'task', NULL, 'Add a login page', 'verifying', 'low', 0, 0,
                             4);
                     INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                     VALUES ('FRK-9', 'task', NULL, 'Not in the log', 'ready', 'low', 0, 0, 5);
                     INSERT INTO cost_records
                         (seq, task_id, agent_id, session_id, day, purpose, model_id,
                          input_tokens, output_tokens, cost_usd)
                     VALUES (5, 'FRK-1', 'dev-a', 's1', '2026-09-17', 'implement',
                             'claude-sonnet-4-5', 1000, 100, 0.5);
                     INSERT INTO projection_cursor (id, seq) VALUES (1, 5);",
                )
                .expect("the older rows are written");
        }
        let log = Arc::new(open_event_log(&path, at(10)).expect("the log opens"));
        let projections = open_projections(log).expect("the projections open");

        let row = row_of(&projections, "FRK-1");
        assert_eq!(row.verifications, 2);
        assert_eq!(row.interventions, 1);
        assert!(
            (row.cost_usd - 0.5).abs() < f64::EPSILON,
            "{}",
            row.cost_usd
        );
        assert_eq!(
            projections
                .task(&"FRK-9".parse().expect("a task id"))
                .expect("the board reads"),
            None,
            "a row the log never created is not kept"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The fixture `sprint.` event of `kind` with `body` in place of the fixture's.
    fn sprint_event(kind: EventKind, body: serde_json::Value) -> NewEvent {
        let mut wire = an_event_wire(kind);
        wire["body"] = body;
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        }
    }

    fn started(sprint_id: &str, budget_usd: Option<f64>) -> NewEvent {
        sprint_event(
            EventKind::SprintStarted,
            json!({ "sprint_id": sprint_id, "budget_usd": budget_usd, "started_by": "human" }),
        )
    }

    fn planned(sprint_id: &str, task_ids: &[&str]) -> NewEvent {
        sprint_event(
            EventKind::SprintPlanned,
            json!({ "sprint_id": sprint_id, "task_ids": task_ids, "planned_by": "sam-ortiz" }),
        )
    }

    fn ended(sprint_id: &str, left: &[&str]) -> NewEvent {
        sprint_event(
            EventKind::SprintEnded,
            json!({ "sprint_id": sprint_id, "ended_by": "human", "left": left }),
        )
    }

    fn sprint_of(projections: &Projections, task_id: &str) -> Option<String> {
        row_of(projections, task_id).sprint
    }

    #[test]
    fn projects_the_open_sprint() {
        let (log, projections) = a_board();
        record(&log, &projections, &started("S1", Some(20.0)));
        assert_eq!(
            projections.open_sprint().expect("the sprint reads"),
            Some(SprintProjection {
                sprint_id: "S1".to_string(),
                budget_usd: Some(20.0),
                open: true,
            })
        );
        record(&log, &projections, &ended("S1", &[]));
        assert_eq!(projections.open_sprint().expect("the sprint reads"), None);
    }

    #[test]
    fn puts_a_task_in_a_sprint_and_takes_it_out() {
        let (log, projections) = a_board();
        for task_id in ["FRK-1", "FRK-2"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task_id));
        }
        record(&log, &projections, &started("S1", None));
        record(&log, &projections, &planned("S1", &["FRK-1", "FRK-2"]));
        assert_eq!(sprint_of(&projections, "FRK-2").as_deref(), Some("S1"));
        record(&log, &projections, &moved("FRK-1", "verifying", "accepted"));
        record(&log, &projections, &ended("S1", &["FRK-2"]));
        assert_eq!(sprint_of(&projections, "FRK-1").as_deref(), Some("S1"));
        assert_eq!(sprint_of(&projections, "FRK-2"), None);
    }

    #[test]
    fn takes_every_unfinished_task_out_of_an_ended_sprint() {
        let (log, projections) = a_board();
        for task_id in ["FRK-1", "FRK-2", "FRK-3"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task_id));
        }
        record(&log, &projections, &started("S1", None));
        record(
            &log,
            &projections,
            &planned("S1", &["FRK-1", "FRK-2", "FRK-3"]),
        );
        record(&log, &projections, &moved("FRK-2", "verifying", "accepted"));
        record(&log, &projections, &moved("FRK-3", "ready", "cancelled"));
        // An end whose `left` misses FRK-1: its sprint file lost it, or it joined after the end
        // read the board.
        record(&log, &projections, &ended("S1", &[]));
        assert_eq!(sprint_of(&projections, "FRK-1"), None);
        assert_eq!(sprint_of(&projections, "FRK-2").as_deref(), Some("S1"));
        assert_eq!(sprint_of(&projections, "FRK-3").as_deref(), Some("S1"));
    }

    #[test]
    fn plans_nothing_into_a_sprint_that_is_not_open() {
        let (log, projections) = a_board();
        for task_id in ["FRK-1", "FRK-2"] {
            record(&log, &projections, &about(EventKind::TaskCreated, task_id));
        }
        record(&log, &projections, &started("S1", None));
        record(&log, &projections, &ended("S1", &[]));
        // A plan that raced the end and lost, and one into a sprint the log never started.
        record(&log, &projections, &planned("S1", &["FRK-1"]));
        record(&log, &projections, &planned("S9", &["FRK-2"]));
        assert_eq!(sprint_of(&projections, "FRK-1"), None);
        assert_eq!(sprint_of(&projections, "FRK-2"), None);
    }

    /// FRK-1 spends a dollar in S1, leaves it when S1 ends, then spends two more; S2 opens.
    fn a_sprint_that_ended_with_a_task_left(log: &EventLog, projections: &Projections) {
        record(log, projections, &about(EventKind::TaskCreated, "FRK-1"));
        record(log, projections, &started("S1", Some(20.0)));
        record(log, projections, &planned("S1", &["FRK-1"]));
        let first = cost(Some("FRK-1"), "a", "s1", "2026-09-22", (1.0, 100, 10));
        record(log, projections, &first);
        record(log, projections, &ended("S1", &["FRK-1"]));
        let second = cost(Some("FRK-1"), "a", "s2", "2026-09-22", (2.0, 200, 20));
        record(log, projections, &second);
        record(log, projections, &started("S2", None));
    }

    #[test]
    fn keeps_a_cost_with_the_sprint_it_was_spent_in() {
        let (log, projections) = a_board();
        a_sprint_that_ended_with_a_task_left(&log, &projections);
        assert_eq!(
            projections
                .costs(CostScope::Sprint)
                .expect("the costs read"),
            vec![row(CostScope::Sprint, "S1", (1.0, 100, 10, 1))]
        );
    }

    /// What the sprint projections say: the open sprint, each row's sprint, and each sprint's
    /// costs.
    type SprintView = (
        Option<SprintProjection>,
        Vec<(String, Option<String>)>,
        Vec<CostProjection>,
    );

    fn sprint_view(projections: &Projections) -> SprintView {
        (
            projections.open_sprint().expect("the sprint reads"),
            projections
                .board()
                .expect("the board reads")
                .into_iter()
                .map(|task| (task.task_id.to_string(), task.sprint))
                .collect(),
            projections
                .costs(CostScope::Sprint)
                .expect("the costs read"),
        )
    }

    #[test]
    fn rebuilds_the_sprints() {
        let (log, projections) = a_board();
        a_sprint_that_ended_with_a_task_left(&log, &projections);
        record(&log, &projections, &planned("S2", &["FRK-1"]));
        let before = sprint_view(&projections);
        assert_eq!(
            before.0.as_ref().map(|sprint| sprint.sprint_id.as_str()),
            Some("S2")
        );
        assert_eq!(
            before.1,
            vec![("FRK-1".to_string(), Some("S2".to_string()))]
        );
        // A sprint the log never started, which only emptying the table takes away.
        log.connection()
            .execute(
                "INSERT INTO sprints (sprint_id, budget_usd, open) VALUES ('S9', NULL, 1)",
                (),
            )
            .expect("a stray sprint is written");
        projections.rebuild().expect("the board is built again");
        assert_eq!(sprint_view(&projections), before);
    }

    #[test]
    fn replays_the_sprints_after_the_migration() {
        use farik_protocol::event::fixtures::a_body_wire;

        let directory = std::env::temp_dir().join(format!(
            "farik-older-sprints-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory under the temporary directory");
        let path = directory.join("farik.db");
        {
            let mut connection = rusqlite::Connection::open(&path).expect("the database opens");
            migrations::apply_through(&mut connection, 7, at(9)).expect("version 7 applies");
            let events = [
                (Some("FRK-1"), EventKind::TaskCreated),
                (None, EventKind::SprintStarted),
                (None, EventKind::SprintPlanned),
                (Some("FRK-1"), EventKind::CostRecorded),
            ];
            for (seq, (task_id, kind)) in (1_i64..).zip(events) {
                connection
                    .execute(
                        "INSERT INTO events
                             (seq, recorded_at, team_id, project_id, task_id, agent_id,
                              session_id, kind, body)
                         VALUES (?1, '2026-09-17T10:00:00Z', 'farik', 'farik', ?2, 'dev-a',
                                 's1', ?3, ?4)",
                        (
                            seq,
                            task_id,
                            kind.to_string(),
                            a_body_wire(kind).to_string(),
                        ),
                    )
                    .expect("an older event is written");
            }
            // What a Farik that knew no sprints projected from those events.
            connection
                .execute_batch(
                    "INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                     VALUES ('FRK-1', 'task', NULL, 'Add a login page', 'draft', 'low', 0, 0, 1);
                     INSERT INTO cost_records
                         (seq, task_id, agent_id, session_id, day, purpose, model_id,
                          input_tokens, output_tokens, cost_usd)
                     VALUES (4, 'FRK-1', 'dev-a', 's1', '2026-09-17', 'implement',
                             'claude-sonnet-4-5', 1000, 100, 0.5);
                     INSERT INTO projection_cursor (id, seq) VALUES (1, 4);",
                )
                .expect("the older rows are written");
        }
        let log = Arc::new(open_event_log(&path, at(10)).expect("the log opens"));
        let projections = open_projections(log).expect("the projections open");
        let migrated = sprint_view(&projections);
        assert_eq!(
            migrated.1,
            vec![("FRK-1".to_string(), Some("S1".to_string()))]
        );
        projections.rebuild().expect("the board is built again");
        assert_eq!(sprint_view(&projections), migrated);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn frees_a_task_left_in_an_ended_sprint_by_the_migration() {
        let directory = std::env::temp_dir().join(format!(
            "farik-stranded-sprint-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory under the temporary directory");
        let path = directory.join("farik.db");
        {
            let mut connection = rusqlite::Connection::open(&path).expect("the database opens");
            migrations::apply_through(&mut connection, 8, at(9)).expect("version 8 applies");
        }
        {
            // A plan and an end whose `left` missed FRK-1, as version 8 projected them: FRK-1
            // stuck in S1 after S1 ended. A move into `verifying` and a cost are also already
            // projected, in `task_projections` and `cost_records`, so a migration that failed to
            // empty either table (M4c/N2, N4) would replay them a second time.
            let log = Arc::new(open_event_log(&path, at(9)).expect("the log opens"));
            for event in [
                about(EventKind::TaskCreated, "FRK-1"),
                started("S1", None),
                planned("S1", &["FRK-1"]),
                ended("S1", &[]),
                moved("FRK-1", "draft", "verifying"),
                cost(Some("FRK-1"), "dev-a", "s1", "2026-09-17", (0.5, 1000, 100)),
            ] {
                log.append(&event).expect("appends");
            }
            log.connection()
                .execute_batch(
                    "DELETE FROM schema_migrations WHERE version > 8;
                     INSERT INTO task_projections
                         (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq,
                          sprint, verifications)
                     VALUES ('FRK-1', 'task', NULL, 'Add a login page', 'verifying', 'low', 0, 0, 5,
                             'S1', 1);
                     INSERT INTO sprints (sprint_id, budget_usd, open) VALUES ('S1', NULL, 0);
                     INSERT INTO cost_records
                         (seq, task_id, agent_id, session_id, day, purpose, model_id,
                          input_tokens, output_tokens, cost_usd)
                     VALUES (6, 'FRK-1', 'dev-a', 's1', '2026-09-17', 'implement',
                             'claude-sonnet-4-5', 1000, 100, 0.5);
                     INSERT INTO projection_cursor (id, seq) VALUES (1, 6)
                         ON CONFLICT (id) DO UPDATE SET seq = 6;",
                )
                .expect("the stranded rows are written");
        }
        let log = Arc::new(open_event_log(&path, at(10)).expect("the log opens"));
        let projections = open_projections(log).expect("the projections open");
        assert_eq!(sprint_of(&projections, "FRK-1"), None);
        assert_eq!(
            row_of(&projections, "FRK-1").verifications,
            1,
            "a stale row left in task_projections would double it"
        );
        assert_eq!(
            projections.costs(CostScope::Task).expect("the costs read"),
            vec![row(CostScope::Task, "FRK-1", (0.5, 1000, 100, 1))],
            "a stale row left in cost_records would collide with the replayed one"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}
