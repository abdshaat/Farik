//! Starting, planning, and ending a sprint (`docs/SPEC.md` sections 3 and 5.5): the files first,
//! then the event, the order `Transitions::record_move` uses.

use std::fmt;

use farik_core::contract::{Role, TaskId, TaskStatus};
use farik_core::sprint::{Sprint, validate_sprint};
use farik_protocol::event::{EventBody, EventKind, new_event};
use farik_store::files::FilesError;
use farik_store::{EventLog, EventQuery, SprintProjection, StoreError, TaskProjection};
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

/// Who plans tasks into a sprint, as `sprint.planned` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedBy {
    /// The assigner, with `farik_plan_sprint`: every rule of a plan holds.
    Assigner(String),
    /// Farik, putting a breakdown's task in its epic's sprint.
    Governor,
}

/// Why a sprint was not started, planned, or ended.
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

/// Plans `task_ids` into the open sprint: each contract's `sprint` field written, then the sprint
/// file's `task_ids`, then `sprint.planned` by the planner. The assigner (the active Scrum Master,
/// else the active Product Manager) plans once, into a sprint that holds no task yet, only `ready`
/// tasks and approved epics with no parent and in no sprint, and within the sprint's budget, an
/// epic counting once and bringing every task already under it. The governor plans a breakdown's
/// task into its epic's sprint, which checks only that a sprint is open and the task is in none:
/// the epic's budget already counts it.
///
/// # Errors
///
/// `Refused`, starting `sprint_plan_refused: `, naming what breaks a rule; `Files` and `Store` as
/// `start_sprint`.
pub fn plan_sprint(
    deps: &ToolDeps,
    task_ids: &[TaskId],
    planned_by: &PlannedBy,
) -> Result<Sprint, SprintError> {
    let open = deps
        .projections
        .open_sprint()?
        .ok_or_else(|| plan_refused("no sprint is open"))?;
    let assigner = match planned_by {
        PlannedBy::Assigner(agent_id) => Some(agent_id.as_str()),
        PlannedBy::Governor => None,
    };
    if let Some(agent_id) = assigner {
        may_plan(deps, agent_id)?;
        if task_ids.is_empty() {
            return Err(plan_refused("name at least one task to plan"));
        }
    }
    let board = deps.projections.board()?;
    let mut planned: Vec<TaskId> = Vec::new();
    for task_id in task_ids {
        let id = task_id.as_str();
        let row = board
            .iter()
            .find(|row| &row.task_id == task_id)
            .ok_or_else(|| plan_refused(&format!("the board has no {id}")))?;
        if let Some(sprint_id) = &row.sprint {
            return Err(plan_refused(&format!("{id} is already in {sprint_id}")));
        }
        planned.push(task_id.clone());
        if assigner.is_none() {
            continue;
        }
        if let Some(parent) = &row.parent {
            return Err(plan_refused(&format!(
                "{id} is under {}, and joins a sprint with its epic",
                parent.as_str()
            )));
        }
        if row.status != TaskStatus::Ready {
            return Err(plan_refused(&format!(
                "{id} is {}, and only a ready task or an approved epic is planned",
                row.status
            )));
        }
        planned.extend(
            board
                .iter()
                .filter(|child| child.parent.as_ref() == Some(task_id) && child.sprint.is_none())
                .map(|child| child.task_id.clone()),
        );
    }
    if assigner.is_some() {
        fits(deps, &open, &board, task_ids)?;
    }
    let mut wire = as_wire(&deps.files.read_sprint(&open.sprint_id)?)?;
    let ids: Vec<&str> = planned.iter().map(|id| id.as_str()).collect();
    if let Some(listed) = wire["task_ids"].as_array_mut() {
        listed.extend(ids.iter().map(|id| json!(id)));
    }
    let sprint = held(&wire)?;
    for task_id in &planned {
        let mut contract = deps.files.read_contract(task_id)?;
        contract.sprint = Some(open.sprint_id.clone());
        deps.files.write_contract(&contract)?;
    }
    deps.files.write_sprint(&sprint)?;
    record(
        deps,
        EventBody::SprintPlanned(typed(json!({
            "sprint_id": open.sprint_id,
            "task_ids": ids,
            "planned_by": match planned_by {
                PlannedBy::Assigner(agent_id) => agent_id.as_str(),
                PlannedBy::Governor => "governor",
            },
        }))?),
    )?;
    Ok(sprint)
}

/// Refuses a plan by anyone but the assigner: the active Scrum Master, or the active Product
/// Manager on a team without one.
fn may_plan(deps: &ToolDeps, agent_id: &str) -> Result<(), SprintError> {
    let team = deps.files.read_team()?;
    let role = team
        .active_agents()
        .find(|agent| agent.id.as_str() == agent_id)
        .map(|agent| Role::from(agent.role));
    let plans = match role {
        Some(Role::ScrumMaster) => true,
        Some(Role::ProductManager) => !team.has_active(Role::ScrumMaster),
        _ => false,
    };
    if plans {
        Ok(())
    } else {
        Err(plan_refused(&format!(
            "{agent_id} is not the assigner: the Scrum Master plans the sprint, or the Product \
             Manager on a team without one"
        )))
    }
}

/// Refuses the assigner's plan of `task_ids` past the open sprint's budget, counting the tasks
/// already in it, and a second plan of a sprint that already holds a task. An epic's budget covers
/// its tasks, so a task under one adds nothing.
fn fits(
    deps: &ToolDeps,
    open: &SprintProjection,
    board: &[TaskProjection],
    task_ids: &[TaskId],
) -> Result<(), SprintError> {
    let in_sprint: Vec<&TaskId> = board
        .iter()
        .filter(|row| row.sprint.as_deref() == Some(open.sprint_id.as_str()))
        .map(|row| &row.task_id)
        .collect();
    if let Some(budget_usd) = open.budget_usd {
        let mut total = 0.0;
        for task_id in task_ids.iter().chain(in_sprint.iter().copied()) {
            if board
                .iter()
                .any(|row| &row.task_id == task_id && row.parent.is_none())
            {
                total += deps.files.read_contract(task_id)?.budget.max_cost_usd;
            }
        }
        if total > budget_usd {
            return Err(plan_refused(&format!(
                "the tasks would cost up to ${total:.2}, past {}'s budget of ${budget_usd:.2}",
                open.sprint_id
            )));
        }
    }
    if in_sprint.is_empty() {
        Ok(())
    } else {
        Err(plan_refused(&format!(
            "{} is planned already, and a sprint is planned once",
            open.sprint_id
        )))
    }
}

/// Puts `task`, filed under an epic, in its epic's sprint when that is the open one, planned by the
/// governor; a task under no epic, or under one in no open sprint, is left as it is.
///
/// # Errors
///
/// As `plan_sprint`.
pub fn join_epics_sprint(deps: &ToolDeps, task: &TaskId) -> Result<Option<Sprint>, SprintError> {
    let Some(parent) = deps.projections.task(task)?.and_then(|row| row.parent) else {
        return Ok(None);
    };
    let epics = deps.projections.task(&parent)?.and_then(|row| row.sprint);
    let open = deps.projections.open_sprint()?.map(|open| open.sprint_id);
    if epics.is_none() || epics != open {
        return Ok(None);
    }
    plan_sprint(deps, std::slice::from_ref(task), &PlannedBy::Governor).map(Some)
}

/// Whether sprint `sprint_id` has had its planning session: a `session.started` of purpose `plan`
/// about no task, recorded after the sprint's `sprint.started`.
///
/// # Errors
///
/// When the log cannot be read.
pub fn planning_session_spent(log: &EventLog, sprint_id: &str) -> Result<bool, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::SprintStarted, EventKind::SessionStarted],
        ..EventQuery::default()
    })?;
    let mut started = false;
    for event in &events {
        match &event.body {
            EventBody::SprintStarted(body) => started = body.sprint_id.as_str() == sprint_id,
            EventBody::SessionStarted(body)
                if started
                    && event.envelope.ids.task_id.is_none()
                    && body.purpose.to_string() == "plan" =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

/// A refusal of a plan, in the words `farik_plan_sprint` answers.
fn plan_refused(why: &str) -> SprintError {
    SprintError::Refused {
        reason: format!("sprint_plan_refused: {why}"),
    }
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
