//! What the team's ceremonies are told (`docs/SPEC.md` section 5.9): the facts each is given,
//! read from the log and the board.

use chrono::{DateTime, NaiveTime, Utc};
use farik_core::contract::{TaskId, TaskStatus};
use farik_protocol::event::{
    BudgetExhaustedBodyScope, EscalationRaisedBodyReason, EventBody, EventKind, FarikEvent,
    SessionEndedBodyReason, Thread,
};
use farik_store::{EventLog, EventQuery, Projections, StoreError};

use crate::sprints::is_planning;
use crate::transitions::is_move_into;

/// An escalation that waits on the human: a task in `escalated`, or an accepted task whose
/// integration failed and has not landed since.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenEscalation {
    /// The task it is about.
    pub task_id: TaskId,
    /// The task's title, an agent's words.
    pub title: String,
    /// The reason of its last `escalation.raised`, as the log writes it.
    pub reason: String,
    /// That event's detail.
    pub detail: String,
    /// That event's seq.
    pub raised_seq: u64,
    /// When it was raised, which is what it has waited since.
    pub raised_at: DateTime<Utc>,
}

/// Every open escalation, oldest first: each task in `escalated`, with its last
/// `escalation.raised`; and each accepted task whose last `integration` escalation since its move
/// into `accepted` has no `task.integrated` after it, as the integration rule reads it.
///
/// # Errors
///
/// When the log or the board cannot be read.
pub fn open_escalations(
    log: &EventLog,
    projections: &Projections,
) -> Result<Vec<OpenEscalation>, StoreError> {
    let mut open = Vec::new();
    for row in projections.board()? {
        if !matches!(row.status, TaskStatus::Escalated | TaskStatus::Accepted) {
            continue;
        }
        let history = log.read(&EventQuery {
            task_id: Some(row.task_id.clone()),
            kinds: vec![
                EventKind::TaskTransitioned,
                EventKind::EscalationRaised,
                EventKind::TaskIntegrated,
            ],
            ..EventQuery::default()
        })?;
        let since = if row.status == TaskStatus::Accepted {
            history
                .iter()
                .rposition(|event| is_move_into(event, TaskStatus::Accepted))
                .map_or(0, |at| at + 1)
        } else {
            0
        };
        let last = history[since..]
            .iter()
            .rposition(|event| match &event.body {
                EventBody::EscalationRaised(body) => {
                    row.status == TaskStatus::Escalated
                        || body.reason == EscalationRaisedBodyReason::Integration
                }
                _ => false,
            });
        let Some(at) = last.map(|at| since + at) else {
            continue;
        };
        let landed = history[at..]
            .iter()
            .any(|event| matches!(event.body, EventBody::TaskIntegrated(_)));
        if row.status == TaskStatus::Accepted && landed {
            continue;
        }
        let EventBody::EscalationRaised(body) = &history[at].body else {
            continue;
        };
        open.push(OpenEscalation {
            task_id: row.task_id.clone(),
            title: row.title.clone(),
            reason: body.reason.to_string(),
            detail: body.detail.clone(),
            raised_seq: history[at].envelope.seq,
            raised_at: history[at].envelope.recorded_at,
        });
    }
    open.sort_by_key(|open| open.raised_seq);
    Ok(open)
}

/// Each `budget.exhausted` of the day's or a sprint's dollars since the planning before sprint
/// `sprint_id`'s, with when it was recorded: since the last planning session started before the
/// sprint's `sprint.started`, else since the log's start. A planning of this sprint that is asked
/// again does not hide what the one before it was shown.
///
/// # Errors
///
/// When the log cannot be read.
pub fn budgets_spent_since_planning(
    log: &EventLog,
    sprint_id: &str,
) -> Result<Vec<(BudgetExhaustedBodyScope, DateTime<Utc>)>, StoreError> {
    let mut spent = Vec::new();
    let mut started = false;
    for event in log.read(&EventQuery {
        kinds: vec![
            EventKind::BudgetExhausted,
            EventKind::SessionStarted,
            EventKind::SprintStarted,
        ],
        ..EventQuery::default()
    })? {
        match &event.body {
            EventBody::SprintStarted(body) if body.sprint_id.as_str() == sprint_id => {
                started = true;
            }
            _ if !started && is_planning(&event) => spent.clear(),
            EventBody::BudgetExhausted(body)
                if matches!(
                    body.scope,
                    BudgetExhaustedBodyScope::DayUsd | BudgetExhaustedBodyScope::SprintUsd
                ) =>
            {
                spent.push((body.scope, event.envelope.recorded_at));
            }
            _ => {}
        }
    }
    Ok(spent)
}

/// How many sessions a ceremony is given at most, so that one that keeps stopping at a limit is
/// not asked for ever.
const CEREMONY_SESSIONS: usize = 3;

/// Whether a ceremony has run among `events` (5.9): a `session.started` that `is_ceremony` matches
/// whose `session.ended` says it completed, was aborted, or failed, or three such starts whatever
/// their ends. One that stopped at a limit or at its model provider's limit is asked again, but not
/// for ever.
pub(crate) fn has_run(events: &[FarikEvent], is_ceremony: impl Fn(&FarikEvent) -> bool) -> bool {
    let mut started: Vec<Option<&str>> = Vec::new();
    for event in events {
        let session_id = event.envelope.ids.session_id.as_deref();
        match &event.body {
            EventBody::SessionStarted(_) if is_ceremony(event) => {
                started.push(session_id);
                if started.len() >= CEREMONY_SESSIONS {
                    return true;
                }
            }
            EventBody::SessionEnded(body)
                if started.contains(&session_id)
                    && matches!(
                        body.reason,
                        SessionEndedBodyReason::Completed
                            | SessionEndedBodyReason::Aborted
                            | SessionEndedBodyReason::Error
                    ) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// The moves the standup of sprint `sprint_id` reports on `now`'s UTC day, oldest first: each
/// `task.transitioned` of a task in `in_sprint` recorded after the sprint's `sprint.started`, from
/// the start of the UTC day of the last standup that has run (else the sprint's start) up to the
/// start of today. Empty when today's standup has run.
///
/// # Errors
///
/// When the log cannot be read.
pub fn standup_moves(
    log: &EventLog,
    sprint_id: &str,
    in_sprint: &[TaskId],
    now: DateTime<Utc>,
) -> Result<Vec<FarikEvent>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::SprintStarted,
            EventKind::SessionStarted,
            EventKind::SessionEnded,
            EventKind::TaskTransitioned,
        ],
        ..EventQuery::default()
    })?;
    let Some(started) = events.iter().position(|event| {
        matches!(&event.body, EventBody::SprintStarted(body) if body.sprint_id.as_str() == sprint_id)
    }) else {
        return Ok(Vec::new());
    };
    let events = &events[started + 1..];
    let today = day_of(now);
    let ran_on = |day: DateTime<Utc>| {
        has_run(events, |event| {
            is_standup(event) && day_of(event.envelope.recorded_at) == day
        })
    };
    if ran_on(today) {
        return Ok(Vec::new());
    }
    let from = events
        .iter()
        .filter(|event| is_standup(event))
        .map(|event| day_of(event.envelope.recorded_at))
        .filter(|day| *day < today && ran_on(*day))
        .max();
    Ok(events
        .iter()
        .filter(|event| {
            let at = event.envelope.recorded_at;
            matches!(event.body, EventBody::TaskTransitioned(_))
                && at < today
                && from.is_none_or(|from| at >= from)
                && event
                    .envelope
                    .ids
                    .task_id
                    .as_ref()
                    .is_some_and(|task| in_sprint.contains(task))
        })
        .cloned()
        .collect())
}

/// Whether `event` is the start of a standup: a `session.started` in the `standup` thread.
fn is_standup(event: &FarikEvent) -> bool {
    matches!(&event.body, EventBody::SessionStarted(body) if body.thread == Some(Thread::Standup))
}

/// The start of `at`'s UTC day.
fn day_of(at: DateTime<Utc>) -> DateTime<Utc> {
    at.date_naive().and_time(NaiveTime::MIN).and_utc()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;

    use super::open_escalations;
    use crate::tools::fixtures::{TestProject, a_team_of_three, at};

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_the_open_escalations() {
        let project = TestProject::new("ceremonies-open", &a_team_of_three(|_| {}));
        let raised = |task: &str, reason: &str| json!({ "reason": reason, "detail": format!("{task} waits") });
        // FRK-1 escalated at its iterations, thirty hours ago.
        let long_ago = at() - Duration::hours(30);
        project.filed("FRK-1", "rejected", "task", None);
        project.moved_at(long_ago, "FRK-1", "rejected", "escalated", &json!({}));
        project.record_at(
            long_ago,
            "FRK-1",
            "escalation.raised",
            &raised("FRK-1", "iterations"),
        );
        // FRK-2 accepted, its integration failed and not landed since.
        project.filed("FRK-2", "verifying", "task", None);
        project.moved("FRK-2", "verifying", "accepted", &json!({}));
        project.record(
            "FRK-2",
            "escalation.raised",
            &raised("FRK-2", "integration"),
        );
        // FRK-3 escalated, then resolved by the human.
        project.filed("FRK-3", "in_progress", "task", None);
        project.moved("FRK-3", "in_progress", "escalated", &json!({}));
        project.record("FRK-3", "escalation.raised", &raised("FRK-3", "budget"));
        project.record(
            "FRK-3",
            "escalation.resolved",
            &json!({ "to": "in_progress", "message": "go on", "resolved_by": "human" }),
        );
        project.moved("FRK-3", "escalated", "in_progress", &json!({}));
        // FRK-4 accepted, its integration failed, then landed.
        project.filed("FRK-4", "verifying", "task", None);
        project.moved("FRK-4", "verifying", "accepted", &json!({}));
        project.record(
            "FRK-4",
            "escalation.raised",
            &raised("FRK-4", "integration"),
        );
        project.record(
            "FRK-4",
            "task.integrated",
            &json!({ "sha": "abc123", "into": "main", "integrated_by": "human" }),
        );

        let open =
            open_escalations(&project.deps.log, &project.deps.projections).expect("the log reads");

        let listed: Vec<(&str, &str, &str, _)> = open
            .iter()
            .map(|open| {
                (
                    open.task_id.as_str(),
                    open.reason.as_str(),
                    open.detail.as_str(),
                    open.raised_at,
                )
            })
            .collect();
        assert_eq!(
            listed,
            vec![
                ("FRK-1", "iterations", "FRK-1 waits", long_ago),
                ("FRK-2", "integration", "FRK-2 waits", at()),
            ]
        );
        assert_eq!(open[0].title, "Add a login page");
    }
}
