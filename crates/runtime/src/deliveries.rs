//! Closing a delivery (ADR 0062, `docs/SPEC.md` 5.9): when the governor closes one, the changelog
//! task it files, and what the ceremonies after it read from the log.

use std::fmt;

use catervas_core::contract::{Role, TaskId, TaskStatus};
use catervas_core::delivery::{AcceptedRow, DeliveryTrigger, delivery_trigger};
use catervas_core::team::Team;
use catervas_protocol::event::{
    CatervasEvent, EventBody, EventKind, TaskCreatedBodyFiledAs, Thread, new_event,
};
use catervas_store::requests::{RequestError, changelog_request, file_small_request};
use catervas_store::{EventLog, EventQuery, Projections, StoreError};
use serde_json::json;

use crate::ceremonies::{has_run, in_thread};
use crate::tools::ToolDeps;
use crate::transitions::is_move_into;

/// A delivery that is due to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueDelivery {
    /// Its id, `D<n>`.
    pub delivery_id: String,
    /// The seq after which its events are read.
    pub since_seq: u64,
    /// Why it closes.
    pub trigger: DeliveryTrigger,
    /// The tasks accepted since, each once, in the order they were accepted.
    pub task_ids: Vec<TaskId>,
}

/// The delivery a review and a retro are about: the last one closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedDelivery {
    /// Its id, `D<n>`.
    pub delivery_id: String,
    /// The seq after which its events are read.
    pub since_seq: u64,
    /// The seq of its `delivery.closed`: its ceremonies come after.
    pub closed_seq: u64,
    /// Its tasks.
    pub task_ids: Vec<TaskId>,
    /// Whether its review has run, under the ceremonies' bound (`has_run`).
    pub reviewed: bool,
    /// Each agent with a review session that has run since the close, in the order their first
    /// sessions started.
    pub reviewed_by: Vec<String>,
    /// Whether its retro has run, under the same bound, or has appended.
    pub retro_held: bool,
    /// Whether a `retro.appended` has been recorded since the close.
    pub retro_appended: bool,
}

/// Why a delivery was not closed.
#[derive(Debug)]
pub enum DeliveryError {
    /// The log or the board failed.
    Store(StoreError),
    /// The changelog task could not be filed.
    Request(RequestError),
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "{error}"),
            Self::Request(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for DeliveryError {}

impl From<StoreError> for DeliveryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<RequestError> for DeliveryError {
    fn from(error: RequestError) -> Self {
        Self::Request(error)
    }
}

/// The seq after which the next delivery counts accepted tasks: the later of the last
/// `delivery.closed` and the last `sprint.ended` after which a review session has run (a sprint
/// of an older log whose tasks were reviewed), else 0.
///
/// # Errors
///
/// When the log cannot be read.
pub fn delivery_start(log: &EventLog) -> Result<u64, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::DeliveryClosed,
            EventKind::SprintEnded,
            EventKind::SessionStarted,
            EventKind::SessionEnded,
        ],
        ..EventQuery::default()
    })?;
    let mut start = 0;
    for (at, event) in events.iter().enumerate() {
        match event.body {
            EventBody::DeliveryClosed(_) => start = start.max(event.envelope.seq),
            EventBody::SprintEnded(_)
                if has_run(&events[at + 1..], |event| in_thread(event, Thread::Review)) =>
            {
                start = start.max(event.envelope.seq);
            }
            _ => {}
        }
    }
    Ok(start)
}

/// The changelog tasks Catervas has filed: they are never in a delivery.
///
/// # Errors
///
/// When the log cannot be read.
pub fn changelog_tasks(log: &EventLog) -> Result<Vec<TaskId>, StoreError> {
    Ok(log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            ..EventQuery::default()
        })?
        .into_iter()
        .filter(|event| {
            matches!(&event.body, EventBody::TaskCreated(body)
                if body.filed_as == Some(TaskCreatedBodyFiledAs::Changelog))
        })
        .filter_map(|event| event.envelope.ids.task_id)
        .collect())
}

/// The agent that runs the ceremonies: the first active Scrum Master, else the first active
/// Product Manager.
fn has_assigner(team: &Team) -> bool {
    [Role::ScrumMaster, Role::ProductManager]
        .iter()
        .any(|role| {
            team.active_agents()
                .any(|agent| Role::from(agent.role) == *role)
        })
}

/// The delivery that is due now, if any (ADR 0062): the first of an epic accepted, five tasks with
/// no parent accepted, or a quiet team, among the tasks accepted since `delivery_start`. None while
/// the last delivery's retro is yet to be held, unless the team has no one to run it.
///
/// # Errors
///
/// When the log or the board cannot be read.
pub fn due_delivery(
    log: &EventLog,
    projections: &Projections,
    team: &Team,
) -> Result<Option<DueDelivery>, StoreError> {
    if let Some(last) = closed_delivery(log)?
        && !last.retro_held
        && has_assigner(team)
    {
        return Ok(None);
    }
    let since_seq = delivery_start(log)?;
    let changelog = changelog_tasks(log)?;
    let board = projections.board()?;
    let mut accepted: Vec<TaskId> = Vec::new();
    for event in log.read(&EventQuery {
        kinds: vec![EventKind::TaskTransitioned],
        after_seq: Some(since_seq),
        ..EventQuery::default()
    })? {
        if let Some(task_id) = event.envelope.ids.task_id.clone()
            && is_move_into(&event, TaskStatus::Accepted)
            && !changelog.contains(&task_id)
            && !accepted.contains(&task_id)
        {
            accepted.push(task_id);
        }
    }
    let rows: Vec<AcceptedRow> = accepted
        .iter()
        .filter_map(|id| board.iter().find(|row| &row.task_id == id))
        .map(|row| AcceptedRow {
            kind: row.kind,
            has_parent: row.parent.is_some(),
        })
        .collect();
    let work_left = board.iter().any(|row| {
        matches!(
            row.status,
            TaskStatus::Ready
                | TaskStatus::Assigned
                | TaskStatus::InProgress
                | TaskStatus::Verifying
                | TaskStatus::Rejected
        )
    });
    let Some(trigger) = delivery_trigger(&rows, work_left) else {
        return Ok(None);
    };
    let closed = log
        .read(&EventQuery {
            kinds: vec![EventKind::DeliveryClosed],
            ..EventQuery::default()
        })?
        .len();
    Ok(Some(DueDelivery {
        delivery_id: format!("D{}", closed + 1),
        since_seq,
        trigger,
        task_ids: accepted,
    }))
}

/// Appends `delivery.closed`, then files the changelog task when the team can take it: an active
/// Product Manager, and a role to name as its reviewer. Answers the task's id.
///
/// # Errors
///
/// When the log refuses, or the changelog task cannot be filed.
pub fn close_delivery(
    deps: &ToolDeps,
    team: &Team,
    due: &DueDelivery,
) -> Result<Option<TaskId>, DeliveryError> {
    let trigger = match due.trigger {
        DeliveryTrigger::EpicAccepted => "epic_accepted",
        DeliveryTrigger::TasksAccepted => "tasks_accepted",
        DeliveryTrigger::TeamQuiet => "team_quiet",
    };
    let body = serde_json::from_value(json!({
        "delivery_id": due.delivery_id,
        "trigger": trigger,
        "since_seq": due.since_seq,
        "task_ids": due.task_ids,
    }))
    .map_err(|error| {
        DeliveryError::Store(StoreError::InvalidEvent {
            detail: format!("delivery.closed cannot be written: {error}"),
        })
    })?;
    let event = new_event(
        EventBody::DeliveryClosed(body),
        deps.clock.now(),
        deps.ids.clone(),
    )
    .map_err(|error| {
        DeliveryError::Store(StoreError::InvalidEvent {
            detail: format!("delivery.closed cannot be stamped: {error:?}"),
        })
    })?;
    let appended = deps.log.append(&event)?;
    deps.projections.apply(&appended)?;
    if !team
        .active_agents()
        .any(|agent| Role::from(agent.role) == Role::ProductManager)
    {
        return Ok(None);
    }
    let Some(reviewer) = catervas_roles::reviewer_for(team, Role::ProductManager) else {
        return Ok(None);
    };
    let board = deps.projections.board()?;
    let tasks: Vec<(TaskId, String)> = due
        .task_ids
        .iter()
        .filter_map(|id| board.iter().find(|row| &row.task_id == id))
        .map(|row| (row.task_id.clone(), row.title.clone()))
        .collect();
    let wire = changelog_request(
        &due.delivery_id,
        &tasks,
        reviewer,
        catervas_store::requests::placeholder_budget_usd(&team.rules()),
    );
    let filed = file_small_request(
        &deps.files,
        &deps.log,
        wire,
        "catervas",
        TaskCreatedBodyFiledAs::Changelog,
        &format!("the changelog entry of {}", due.delivery_id),
        (deps.clock.now(), &deps.ids),
    )?;
    Ok(Some(filed.id))
}

/// The last delivery closed, with whether its review and its retro have run since the close, and
/// whether its retro has appended.
///
/// # Errors
///
/// When the log cannot be read.
pub fn closed_delivery(log: &EventLog) -> Result<Option<ClosedDelivery>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::DeliveryClosed,
            EventKind::SessionStarted,
            EventKind::SessionEnded,
            EventKind::RetroAppended,
        ],
        ..EventQuery::default()
    })?;
    let Some(last) = events
        .iter()
        .rposition(|event| matches!(event.body, EventBody::DeliveryClosed(_)))
    else {
        return Ok(None);
    };
    let EventBody::DeliveryClosed(closed) = &events[last].body else {
        return Ok(None);
    };
    let after = &events[last + 1..];
    let retro_appended = after
        .iter()
        .any(|event| matches!(event.body, EventBody::RetroAppended(_)));
    Ok(Some(ClosedDelivery {
        delivery_id: closed.delivery_id.as_str().to_string(),
        since_seq: closed.since_seq,
        closed_seq: events[last].envelope.seq,
        task_ids: closed
            .task_ids
            .iter()
            .filter_map(|id| id.as_str().parse().ok())
            .collect(),
        reviewed: has_run(after, |event| in_thread(event, Thread::Review)),
        reviewed_by: crate::ceremonies::reviewed_by(after),
        retro_held: retro_appended || has_run(after, |event| in_thread(event, Thread::Retro)),
        retro_appended,
    }))
}

/// Each event about `task_id` recorded since the delivery started and before it closed, oldest
/// first.
///
/// # Errors
///
/// When the log cannot be read.
pub fn delivery_events(
    log: &EventLog,
    delivery: &ClosedDelivery,
    task_id: &TaskId,
) -> Result<Vec<CatervasEvent>, StoreError> {
    Ok(log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            after_seq: Some(delivery.since_seq),
            ..EventQuery::default()
        })?
        .into_iter()
        .take_while(|event| event.envelope.seq < delivery.closed_seq)
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::closed_delivery;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn knows_who_has_reviewed_a_closed_delivery() {
        let project = TestProject::new("deliveries-reviewed-by", &a_team_of_three(|_| {}));
        project.record(
            "",
            "delivery.closed",
            &json!({
                "delivery_id": "D1", "trigger": "team_quiet", "since_seq": 0, "task_ids": ["CTV-1"]
            }),
        );
        let start = |agent: &str, session: &str| {
            project.record_in(
                Some(agent),
                Some(session),
                "",
                "session.started",
                &json!({
                    "purpose": "ceremony", "model": "claude-opus-5", "effort": "high",
                    "thread": "review"
                }),
            );
        };
        let end = |agent: &str, session: &str| {
            project.record_in(
                Some(agent),
                Some(session),
                "",
                "session.ended",
                &json!({ "reason": "completed", "detail": "" }),
            );
        };
        start("sm", "s-1");
        end("sm", "s-1");
        start("pm", "s-2");

        let delivery = closed_delivery(&project.deps.log)
            .expect("the log reads")
            .expect("D1 closed");
        assert_eq!(delivery.reviewed_by, ["sm"]);

        end("pm", "s-2");

        let delivery = closed_delivery(&project.deps.log)
            .expect("the log reads")
            .expect("D1 closed");
        assert_eq!(delivery.reviewed_by, ["sm", "pm"]);
    }
}
