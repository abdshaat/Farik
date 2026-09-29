//! What each agent is doing now, and what moved since the person last looked (`docs/SPEC.md`
//! 5.9 and 5.16), in the words the pages show.

use std::collections::BTreeMap;
use std::str::FromStr as _;

use chrono::{DateTime, Utc};
use farik_core::contract::{TaskId, TaskStatus};
use farik_core::team::{AgentStatus, Team};
use farik_protocol::event::{EventBody, EventKind, FarikEvent, MessageKind};

use crate::files::ProjectFiles;
use crate::waiting::name_of;
use crate::{EventLog, EventQuery, Projections, StoreError};

/// What an agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    /// A session of its runs.
    Working,
    /// Its provider refused it for a usage limit, until a time.
    Resting,
    /// Something of its waits on the human.
    Waiting,
    /// The human paused it, or the whole team.
    Paused,
    /// Nothing.
    Idle,
}

/// One agent's activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivity {
    /// The agent.
    pub agent_id: String,
    /// What it is doing.
    pub state: ActivityState,
    /// What it is doing, in a sentence.
    pub line: String,
    /// The task it is doing it on, when there is one.
    pub task_id: Option<TaskId>,
    /// When a resting agent picks up again.
    pub until: Option<DateTime<Utc>>,
}

/// Each active or paused agent's activity, in the team's order: paused, else working, else
/// resting, else waiting on the human, else idle.
///
/// # Errors
///
/// What the store refused.
pub fn activity(
    log: &EventLog,
    projections: &Projections,
    files: &ProjectFiles,
    team: &Team,
    now: DateTime<Utc>,
) -> Result<Vec<AgentActivity>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::SessionStarted,
            EventKind::SessionEnded,
            EventKind::AgentSlept,
            EventKind::TeamPaused,
            EventKind::TeamResumed,
        ],
        ..EventQuery::default()
    })?;
    let team_paused = events
        .iter()
        .rev()
        .find_map(|event| match event.body.kind() {
            EventKind::TeamPaused => Some(true),
            EventKind::TeamResumed => Some(false),
            _ => None,
        })
        .unwrap_or(false);
    let titles = titles(projections)?;
    let waiting = crate::waiting::waiting(projections, log, files, team)?;
    let mut all = Vec::new();
    for agent in team
        .agents
        .iter()
        .filter(|agent| agent.status != AgentStatus::Retired)
    {
        let id = agent.id.as_str();
        let by_agent = |event: &&FarikEvent| event.envelope.ids.agent_id.as_deref() == Some(id);
        let one = |state, line: String, task_id: Option<TaskId>, until| AgentActivity {
            agent_id: id.to_string(),
            state,
            line,
            task_id,
            until,
        };
        if team_paused || agent.status == AgentStatus::Paused {
            all.push(one(ActivityState::Paused, "Paused".to_string(), None, None));
            continue;
        }
        if let Some((line, task)) = at_work(&events, id, &titles) {
            all.push(one(ActivityState::Working, line, task, None));
            continue;
        }
        let asleep = events
            .iter()
            .filter(by_agent)
            .rev()
            .find_map(|event| match &event.body {
                EventBody::AgentSlept(body) if body.until > now => Some(body.until),
                _ => None,
            });
        if let Some(until) = asleep {
            let line = format!(
                "Resting until {}. It reached its usage limit.",
                until.format("%H:%M")
            );
            all.push(one(ActivityState::Resting, line, None, Some(until)));
            continue;
        }
        if let Some(item) = waiting
            .iter()
            .find(|item| item.agent_id.as_deref() == Some(id))
        {
            let line = format!("Waiting on you: {}", item.line);
            all.push(one(
                ActivityState::Waiting,
                line,
                Some(item.task_id.clone()),
                None,
            ));
            continue;
        }
        all.push(one(
            ActivityState::Idle,
            "Nothing to do right now".to_string(),
            None,
            None,
        ));
    }
    Ok(all)
}

/// What `agent` is doing in its session that has started and not ended, and on which task.
fn at_work(
    events: &[FarikEvent],
    agent: &str,
    titles: &BTreeMap<TaskId, String>,
) -> Option<(String, Option<TaskId>)> {
    let (event, body) = events
        .iter()
        .rev()
        .filter(|event| event.envelope.ids.agent_id.as_deref() == Some(agent))
        .find_map(|event| {
            let EventBody::SessionStarted(body) = &event.body else {
                return None;
            };
            let session = event.envelope.ids.session_id.as_ref()?;
            let ended = events.iter().any(|later| {
                later.body.kind() == EventKind::SessionEnded
                    && later.envelope.ids.session_id.as_ref() == Some(session)
            });
            (!ended).then_some((event, body))
        })?;
    let task = event.envelope.ids.task_id.clone();
    let title = task
        .as_ref()
        .and_then(|task| titles.get(task))
        .cloned()
        .unwrap_or_default();
    let line = match body.purpose.to_string().as_str() {
        "triage" => "Sizing a request".to_string(),
        "refine" => format!("Writing the plan for {title}"),
        "plan" => format!("Planning {title}"),
        "implement" => format!("Building {title}"),
        "verify" => format!("Reviewing {title}"),
        "ceremony" => format!(
            "Running the {}",
            body.thread
                .map(|thread| thread.to_string())
                .unwrap_or_default()
        ),
        _ => "Answering in the channel".to_string(),
    };
    Some((line, task))
}

/// One thing that moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// When.
    pub at: DateTime<Utc>,
    /// What, in a sentence.
    pub line: String,
}

/// What moved after `since`, oldest first: the moves, the integrations, the human's acceptances,
/// the agents' naps, and the ceremonies' posts.
///
/// # Errors
///
/// What the store refused.
pub fn moved_since(
    log: &EventLog,
    projections: &Projections,
    team: &Team,
    since: DateTime<Utc>,
) -> Result<Vec<Moved>, StoreError> {
    let titles = titles(projections)?;
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::TaskTransitioned,
            EventKind::TaskIntegrated,
            EventKind::HumanAccepted,
            EventKind::AgentSlept,
            EventKind::MessagePosted,
        ],
        ..EventQuery::default()
    })?;
    let who = |id: &str| match id {
        "human" => "You".to_string(),
        "governor" | "farik" => "Farik".to_string(),
        id => name_of(team, id),
    };
    Ok(events
        .iter()
        .filter(|event| event.envelope.recorded_at > since)
        .filter_map(|event| {
            let title = || {
                event
                    .envelope
                    .ids
                    .task_id
                    .as_ref()
                    .and_then(|task| titles.get(task))
                    .cloned()
                    .unwrap_or_default()
            };
            let line = match &event.body {
                EventBody::TaskTransitioned(body) => {
                    let to = TaskStatus::from_str(&body.to.to_string()).ok()?;
                    format!(
                        "{} moved {} to {}",
                        who(&body.requested_by),
                        title(),
                        status_in_words(to)
                    )
                }
                EventBody::TaskIntegrated(_) => format!("{} was added to the project", title()),
                EventBody::HumanAccepted(_) => format!("You accepted {}", title()),
                EventBody::AgentSlept(body) => format!(
                    "{} reached its usage limit and will pick up again at {}",
                    who(event.envelope.ids.agent_id.as_deref().unwrap_or_default()),
                    body.until.format("%H:%M")
                ),
                EventBody::MessagePosted(body) if body.kind == MessageKind::Ceremony => format!(
                    "{} posted the {}",
                    who(&body.author),
                    body.thread
                        .map(|thread| thread.to_string())
                        .unwrap_or_default()
                ),
                _ => return None,
            };
            Some(Moved {
                at: event.envelope.recorded_at,
                line,
            })
        })
        .collect())
}

/// A status in the words the pages show (`docs/design/web-ui.md`'s lifecycle table).
#[must_use]
pub const fn status_in_words(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft | TaskStatus::Refining => "Planning",
        TaskStatus::Ready | TaskStatus::Assigned => "To do",
        TaskStatus::InProgress => "In progress",
        TaskStatus::Rejected => "Being reworked",
        TaskStatus::Verifying => "Review",
        TaskStatus::Accepted => "Done",
        TaskStatus::Cancelled => "Cancelled",
        TaskStatus::Blocked => "Stuck",
        TaskStatus::Escalated => "Needs your help",
    }
}

/// Every task's title, by id.
fn titles(projections: &Projections) -> Result<BTreeMap<TaskId, String>, StoreError> {
    Ok(projections
        .board()?
        .into_iter()
        .map(|row| (row.task_id, row.title))
        .collect())
}

#[cfg(test)]
mod tests {
    use farik_core::team::validate_team;
    use serde_json::json;

    use super::{ActivityState, activity, moved_since};
    use crate::waiting::fixtures::{Board, at};

    /// Ada, Linus, and Grace, with Mira and Theo beside them, Theo paused.
    fn five() -> farik_core::team::Team {
        let mut wire = farik_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "grace", "display_name": "Grace", "role": "architect", "status": "active" },
            { "id": "mira", "display_name": "Mira", "role": "scrum_master", "status": "active" },
            { "id": "theo", "display_name": "Theo", "role": "software_developer", "status": "paused" },
        ]);
        validate_team(&wire).expect("the fixture is a team")
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "each state, a session's end, a nap's end, and the team's pause"
    )]
    fn derives_each_agents_activity() {
        let board = Board::new("activity-states");
        let team = five();
        board.file("FRK-1", "Login form", |_| {});
        board.moved(
            at(9, 1),
            "FRK-1",
            ("draft", "in_progress"),
            "linus",
            (Some("linus"), Some("grace")),
        );
        board.session(
            at(9, 2),
            Some("FRK-1"),
            "linus",
            "session-1",
            "session.started",
            json!({ "purpose": "implement", "model": "claude-opus-5", "effort": "high" }),
        );
        board.put(
            at(9, 3),
            None,
            Some("grace"),
            "agent.slept",
            json!({ "until": at(14, 30).to_rfc3339(), "detail": "usage limit" }),
        );
        board.file("FRK-2", "A plan", |_| {});
        board.moved(
            at(9, 4),
            "FRK-2",
            ("draft", "escalated"),
            "ada",
            (None, None),
        );
        board.put(
            at(9, 4),
            Some("FRK-2"),
            None,
            "escalation.raised",
            json!({ "reason": "approval", "detail": "waits" }),
        );

        let states = |board: &Board| {
            activity(
                &board.log,
                &board.projections,
                &board.files,
                &team,
                at(12, 0),
            )
            .expect("the store reads")
            .into_iter()
            .map(|one| {
                (
                    one.agent_id,
                    one.state,
                    one.line,
                    one.task_id.map(|id| id.to_string()),
                    one.until,
                )
            })
            .collect::<Vec<_>>()
        };
        assert_eq!(
            states(&board),
            vec![
                (
                    "ada".to_string(),
                    ActivityState::Waiting,
                    "Waiting on you: Ada wrote a plan for you to approve".to_string(),
                    Some("FRK-2".to_string()),
                    None
                ),
                (
                    "linus".to_string(),
                    ActivityState::Working,
                    "Building Login form".to_string(),
                    Some("FRK-1".to_string()),
                    None
                ),
                (
                    "grace".to_string(),
                    ActivityState::Resting,
                    "Resting until 14:30. It reached its usage limit.".to_string(),
                    None,
                    Some(at(14, 30))
                ),
                (
                    "mira".to_string(),
                    ActivityState::Idle,
                    "Nothing to do right now".to_string(),
                    None,
                    None
                ),
                (
                    "theo".to_string(),
                    ActivityState::Paused,
                    "Paused".to_string(),
                    None,
                    None
                ),
            ]
        );

        // Linus's session ends and Mira runs the standup; a nap past its time is over.
        board.session(
            at(9, 5),
            Some("FRK-1"),
            "linus",
            "session-1",
            "session.ended",
            json!({ "reason": "completed", "detail": "done" }),
        );
        board.session(
            at(9, 6),
            None,
            "mira",
            "session-2",
            "session.started",
            json!({ "purpose": "ceremony", "model": "claude-opus-5", "effort": "low", "thread": "standup" }),
        );
        let now = states(&board);
        assert_eq!(now[1].2, "Nothing to do right now");
        assert_eq!(now[3].2, "Running the standup");
        let later = activity(
            &board.log,
            &board.projections,
            &board.files,
            &team,
            at(15, 0),
        )
        .expect("the store reads");
        assert_eq!(later[2].state, ActivityState::Idle);

        // The whole team paused pauses everyone.
        board.put(
            at(9, 7),
            None,
            None,
            "team.paused",
            json!({ "by": "human" }),
        );
        let paused = states(&board);
        assert!(
            paused
                .iter()
                .all(|one| one.1 == ActivityState::Paused && one.2 == "Paused"),
            "{paused:?}"
        );
    }

    #[test]
    fn says_what_moved_since() {
        let board = Board::new("moved-since");
        let team = five();
        board.file("FRK-1", "Login form", |_| {});
        // Before `since`, and not shown.
        board.moved(
            at(9, 0),
            "FRK-1",
            ("draft", "refining"),
            "ada",
            (None, None),
        );
        board.moved(
            at(10, 1),
            "FRK-1",
            ("refining", "in_progress"),
            "linus",
            (Some("linus"), Some("grace")),
        );
        board.moved(
            at(10, 2),
            "FRK-1",
            ("in_progress", "rejected"),
            "human",
            (Some("linus"), Some("grace")),
        );
        board.put(
            at(10, 3),
            Some("FRK-1"),
            None,
            "human.accepted",
            json!({ "subject": "result", "accepted_by": "human" }),
        );
        board.put(
            at(10, 4),
            Some("FRK-1"),
            None,
            "task.integrated",
            json!({ "sha": "abc", "into": "main", "integrated_by": "governor" }),
        );
        board.put(
            at(10, 5),
            None,
            Some("grace"),
            "agent.slept",
            json!({ "until": at(14, 30).to_rfc3339(), "detail": "usage limit" }),
        );
        board.put(
            at(10, 6),
            None,
            Some("mira"),
            "message.posted",
            json!({ "author": "mira", "kind": "ceremony", "text": "Yesterday, today.", "mentions": [], "thread": "standup" }),
        );
        // A plain message is not a move.
        board.put(
            at(10, 7),
            None,
            Some("linus"),
            "message.posted",
            json!({ "author": "linus", "kind": "ambient", "text": "Hello.", "mentions": [] }),
        );

        let moved =
            moved_since(&board.log, &board.projections, &team, at(10, 0)).expect("the store reads");
        let lines: Vec<(chrono::DateTime<chrono::Utc>, &str)> = moved
            .iter()
            .map(|one| (one.at, one.line.as_str()))
            .collect();
        assert_eq!(
            lines,
            vec![
                (at(10, 1), "Linus moved Login form to In progress"),
                (at(10, 2), "You moved Login form to Being reworked"),
                (at(10, 3), "You accepted Login form"),
                (at(10, 4), "Login form was added to the project"),
                (
                    at(10, 5),
                    "Grace reached its usage limit and will pick up again at 14:30"
                ),
                (at(10, 6), "Mira posted the standup"),
            ]
        );
    }
}
