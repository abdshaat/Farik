//! Starting and ending a sprint (`docs/SPEC.md` sections 3 and 5.5): the files first, then the
//! event, the order `Transitions::record_move` uses.

use std::fmt;

use farik_core::contract::{TaskId, TaskStatus};
use farik_core::sprint::{Sprint, validate_sprint};
use farik_protocol::event::{EventBody, EventKind, new_event};
use farik_store::files::FilesError;
use farik_store::{EventQuery, StoreError};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::tools::ToolDeps;

/// Who ended a sprint, as `sprint.ended` records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndedBy {
    /// Farik, when every task in it was accepted or cancelled.
    Governor,
    /// The human, with `farik sprint end`.
    Human,
}

/// Why a sprint was not started or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SprintError {
    /// A sprint is already open: one at a time.
    AlreadyOpen {
        /// The open sprint.
        sprint_id: String,
    },
    /// No sprint is open to end.
    NoneOpen,
    /// What was asked breaks a rule, in these words.
    Refused {
        /// Why, starting with a `snake_case` kind and `: `.
        reason: String,
    },
    /// A file under `.farik/` could not be read or written.
    Files(FilesError),
    /// The log or the board failed.
    Store(StoreError),
}

impl fmt::Display for SprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyOpen { sprint_id } => {
                write!(
                    formatter,
                    "{sprint_id} is open; end it with farik sprint end"
                )
            }
            Self::NoneOpen => write!(formatter, "no sprint is open"),
            Self::Refused { reason } => write!(formatter, "{reason}"),
            Self::Files(error) => write!(formatter, "the files failed: {error}"),
            Self::Store(error) => write!(formatter, "the store failed: {error}"),
        }
    }
}

impl std::error::Error for SprintError {}

impl From<FilesError> for SprintError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<StoreError> for SprintError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Starts a sprint, `S<n>` with `n` one more than the highest of the sprint files' numbers and the
/// ids in the log's `sprint.started` events, open with `budget_usd`: its file, then
/// `sprint.started` by `started_by`.
///
/// # Errors
///
/// `AlreadyOpen` while a sprint is open; `Refused` for a budget of zero or less; `Files` when the
/// sprint files cannot be read or the new one written; `Store` when the log fails.
pub fn start_sprint(
    deps: &ToolDeps,
    budget_usd: Option<f64>,
    started_by: &str,
) -> Result<Sprint, SprintError> {
    if let Some(open) = deps.projections.open_sprint()? {
        return Err(SprintError::AlreadyOpen {
            sprint_id: open.sprint_id,
        });
    }
    let sprint_id = format!("S{}", highest_number(deps)? + 1);
    let mut wire = json!({
        "id": sprint_id,
        "started_at": deps.clock.now().to_rfc3339(),
        "task_ids": [],
        "status": "open",
    });
    if let Some(usd) = budget_usd {
        wire["budget_usd"] = json!(usd);
    }
    let sprint = held(&wire)?;
    deps.files.write_sprint(&sprint)?;
    record(
        deps,
        EventBody::SprintStarted(typed(json!({
            "sprint_id": sprint_id,
            "budget_usd": budget_usd,
            "started_by": started_by,
        }))?),
    )?;
    Ok(sprint)
}

/// Ends the open sprint: its file `ended` at now, and each task in it that is neither accepted nor
/// cancelled taken out of it, its contract's `sprint` cleared and its status left as it is; then
/// `sprint.ended` naming those tasks as `left`.
///
/// # Errors
///
/// `NoneOpen` when no sprint is open; `Files` and `Store` as `start_sprint`.
pub fn end_sprint(deps: &ToolDeps, ended_by: EndedBy) -> Result<Sprint, SprintError> {
    let open = deps
        .projections
        .open_sprint()?
        .ok_or(SprintError::NoneOpen)?;
    let mut wire = as_wire(&deps.files.read_sprint(&open.sprint_id)?)?;
    wire["status"] = json!("ended");
    wire["ended_at"] = json!(deps.clock.now().to_rfc3339());
    let sprint = held(&wire)?;
    let mut left: Vec<TaskId> = Vec::new();
    for id in &sprint.task_ids {
        let task_id: TaskId = id.as_str().parse().map_err(|error| SprintError::Refused {
            reason: format!(
                "sprint_refused: {} holds {id:?}: {error}",
                sprint.id.as_str()
            ),
        })?;
        let finished = deps
            .projections
            .task(&task_id)?
            .is_some_and(|row| matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled));
        if !finished {
            left.push(task_id);
        }
    }
    deps.files.write_sprint(&sprint)?;
    for task_id in &left {
        let mut contract = deps.files.read_contract(task_id)?;
        contract.sprint = None;
        deps.files.write_contract(&contract)?;
    }
    record(
        deps,
        EventBody::SprintEnded(typed(json!({
            "sprint_id": sprint.id.as_str(),
            "ended_by": match ended_by {
                EndedBy::Governor => "governor",
                EndedBy::Human => "human",
            },
            "left": left.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        }))?),
    )?;
    Ok(sprint)
}

/// The highest sprint number the files or the log's `sprint.started` events hold, or 0.
fn highest_number(deps: &ToolDeps) -> Result<u64, SprintError> {
    let started = deps.log.read(&EventQuery {
        kinds: vec![EventKind::SprintStarted],
        ..EventQuery::default()
    })?;
    let from_log = started.iter().filter_map(|event| match &event.body {
        EventBody::SprintStarted(body) => Some(body.sprint_id.as_str().to_string()),
        _ => None,
    });
    let from_files = deps
        .files
        .list_sprints()?
        .into_iter()
        .map(|sprint| sprint.id.as_str().to_string());
    Ok(from_log
        .chain(from_files)
        .filter_map(|id| id.trim_start_matches('S').parse::<u64>().ok())
        .max()
        .unwrap_or(0))
}

/// A sprint's wire, held to its schema.
fn held(wire: &Value) -> Result<Sprint, SprintError> {
    validate_sprint(wire).map_err(|errors| SprintError::Refused {
        reason: format!(
            "sprint_refused: {}",
            errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })
}

/// A sprint as its wire, to change a field and hold it to the schema again.
fn as_wire(sprint: &Sprint) -> Result<Value, SprintError> {
    serde_json::to_value(sprint).map_err(|error| SprintError::Refused {
        reason: format!(
            "sprint_refused: {} cannot be written: {error}",
            sprint.id.as_str()
        ),
    })
}

/// An event body from its wire, which the protocol's types hold to the event schema's patterns.
fn typed<Body: DeserializeOwned>(wire: Value) -> Result<Body, SprintError> {
    serde_json::from_value(wire).map_err(|error| SprintError::Refused {
        reason: format!("sprint_refused: the event cannot be written: {error}"),
    })
}

/// Appends one event about no task, stamped with the project's ids, and projects it.
fn record(deps: &ToolDeps, body: EventBody) -> Result<(), SprintError> {
    let event = new_event(body, deps.clock.now(), deps.ids.clone()).map_err(|error| {
        SprintError::Refused {
            reason: format!("sprint_refused: the event cannot be stamped: {error:?}"),
        }
    })?;
    let appended = deps.log.append(&event)?;
    deps.projections.apply(&appended)?;
    Ok(())
}
