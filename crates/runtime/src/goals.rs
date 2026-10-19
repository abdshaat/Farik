//! The owner's goal in the runtime (`docs/SPEC.md` 5.18, ADR 0075): the book the log makes of it,
//! who plans and who reads it, and the commands the owner gives it. The goal is events alone, so
//! every read folds the log again, as deliveries do.

use std::sync::Mutex;

use catervas_core::contract::{Role, TaskStatus};
use catervas_core::goal::{
    Goal, GoalBook, GoalOutcome, GoalRecord, GoalStatus, ReadingSource, StallReason, fold_goals,
};
use catervas_core::governor::task_status::is_terminal;
use catervas_core::team::{Agent, Team};
use catervas_protocol::event::{
    EventBody, EventKind, GoalEndedBody, GoalEndedBodyOutcome, GoalIdBody, GoalMeasuredBody,
    GoalMeasuredBodySource, GoalSetBody, SessionStartedBodyPurpose, new_event,
};
use catervas_store::requests::goal_of;
use catervas_store::{EventLog, EventQuery, StoreError};
use serde_json::{Value, json};

use crate::orchestrator::{CommandError, CommandReport};
use crate::tools::ToolDeps;

/// Held while a goal command reads the book and records, so that two presses make one goal.
// ponytail: one lock for the process, as drafting a product plan has; the command line sets no goal.
static GOALS: Mutex<()> = Mutex::new(());

/// The kinds of event the book is made of.
const GOAL_KINDS: [EventKind; 6] = [
    EventKind::GoalSet,
    EventKind::GoalPaused,
    EventKind::GoalResumed,
    EventKind::GoalEnded,
    EventKind::GoalMeasured,
    EventKind::GoalPlanned,
];

/// What the log says about goals: the one that stands, the last one ended and the next id.
///
/// # Errors
///
/// The log could not be read.
pub fn goal_book(log: &EventLog) -> Result<GoalBook, StoreError> {
    let events = log.read(&EventQuery {
        kinds: GOAL_KINDS.to_vec(),
        ..EventQuery::default()
    })?;
    let records: Vec<_> = events
        .into_iter()
        .filter_map(|event| {
            let at = event.envelope.recorded_at;
            let agent = event.envelope.ids.agent_id;
            let record = match event.body {
                EventBody::GoalSet(body) => GoalRecord::Set {
                    goal: body.goal.to_string(),
                    words: body.words.to_string(),
                    target: body.target,
                    unit: body.unit.map(|unit| unit.to_string()),
                    server: body.server.map(|server| server.to_string()),
                },
                EventBody::GoalPaused(body) => GoalRecord::Paused {
                    goal: body.goal.to_string(),
                },
                EventBody::GoalResumed(body) => GoalRecord::Resumed {
                    goal: body.goal.to_string(),
                },
                EventBody::GoalEnded(body) => GoalRecord::Ended {
                    goal: body.goal.to_string(),
                    outcome: match body.outcome {
                        GoalEndedBodyOutcome::Reached => GoalOutcome::Reached,
                        GoalEndedBodyOutcome::Dropped => GoalOutcome::Dropped,
                    },
                },
                EventBody::GoalMeasured(body) => GoalRecord::Measured {
                    goal: body.goal.to_string(),
                    value: body.value,
                    source: match body.source {
                        GoalMeasuredBodySource::Connector => ReadingSource::Connector,
                        GoalMeasuredBodySource::Owner => ReadingSource::Owner,
                    },
                    server: body.server.map(|server| server.to_string()),
                    agent,
                },
                EventBody::GoalPlanned(body) => GoalRecord::Planned {
                    goal: body.goal.to_string(),
                    filed: body.filed.len(),
                },
                _ => return None,
            };
            Some((at, record))
        })
        .collect();
    Ok(fold_goals(&records))
}

/// Who plans the team's next steps towards the goal: the active Product Manager, or, for one agent
/// alone, that agent.
pub(crate) fn planner(team: &Team) -> Option<&Agent> {
    if team.is_solo() {
        return team.active_agents().next();
    }
    team.active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
}

/// Who reads the goal's number from `server`: the first active agent, in team order, that has it
/// connected.
pub(crate) fn reader<'a>(team: &'a Team, server: &str) -> Option<&'a Agent> {
    team.active_agents().find(|agent| {
        agent
            .mcp_servers
            .iter()
            .flatten()
            .any(|entry| entry.name.as_str() == server)
    })
}

fn refused(code: &str, detail: impl std::fmt::Display) -> CommandError {
    CommandError::Refused {
        reason: format!("{code}: {detail}"),
    }
}

fn failed(error: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        detail: error.to_string(),
    }
}

/// Records `body` as the owner's: no task, no agent and no session.
fn append(tools: &ToolDeps, body: EventBody) -> Result<u64, CommandError> {
    let event = new_event(body, tools.clock.now(), tools.ids.clone()).map_err(|error| {
        CommandError::Failed {
            detail: format!("the event cannot be recorded: {error:?}"),
        }
    })?;
    let appended = tools.log.append(&event).map_err(failed)?;
    tools.projections.apply(&appended).map_err(failed)?;
    Ok(appended.envelope.seq)
}

/// The goal that stands, when it is `goal`.
fn standing_goal(tools: &ToolDeps, goal: &str) -> Result<Goal, CommandError> {
    match goal_book(&tools.log).map_err(failed)?.standing {
        Some(standing) if standing.id == goal => Ok(standing),
        _ => Err(refused(
            "goal_unknown",
            format!("{goal} is not the goal that stands"),
        )),
    }
}

/// Sets the goal, or, with `edit`, changes the standing one. Each refusal is `<code>: <words>`, in
/// this order: `goal_unknown` (an edit of a goal that does not stand), `goal_active` (a new goal
/// while one stands), `goal_needs_daily_budget`, `goal_target_unit`, `goal_server_without_target`
/// and `goal_server_not_connected`.
pub(crate) fn set_goal(
    tools: &ToolDeps,
    team: &Team,
    edit: Option<&str>,
    words: &str,
    target: Option<f64>,
    unit: Option<&str>,
    server: Option<&str>,
) -> Result<CommandReport, CommandError> {
    let _setting = crate::locked(&GOALS);
    let book = goal_book(&tools.log).map_err(failed)?;
    let id = match (edit, &book.standing) {
        (Some(goal), Some(standing)) if standing.id == goal => goal.to_string(),
        (Some(goal), _) => {
            return Err(refused(
                "goal_unknown",
                format!("{goal} is not the goal that stands"),
            ));
        }
        (None, Some(standing)) => {
            return Err(refused(
                "goal_active",
                format!(
                    "the goal \u{201c}{}\u{201d} stands; end it before setting another",
                    standing.words
                ),
            ));
        }
        (None, None) => book.next_id.clone(),
    };
    if team.budgets.daily_usd.is_none() {
        return Err(refused(
            "goal_needs_daily_budget",
            "set a daily budget first: a goal keeps the team working every day",
        ));
    }
    if target.is_some() != unit.is_some() {
        return Err(refused(
            "goal_target_unit",
            "a number to reach comes with what it counts, and the other way round",
        ));
    }
    if let Some(server) = server {
        if target.is_none() {
            return Err(refused(
                "goal_server_without_target",
                "only a goal with a number to reach is counted from a service",
            ));
        }
        if reader(team, server).is_none() {
            return Err(refused(
                "goal_server_not_connected",
                format!("no active agent has {server} connected"),
            ));
        }
    }
    let words = words.trim();
    if words.is_empty() || words.chars().count() > 300 {
        return Err(CommandError::Invalid {
            detail: "a goal is 1 to 300 characters".to_string(),
        });
    }
    let bad = |field: &str| CommandError::Invalid {
        detail: format!("the goal's {field} is not one the log can hold"),
    };
    let body = GoalSetBody {
        goal: id.clone().try_into().map_err(|_| bad("id"))?,
        words: words.to_string().try_into().map_err(|_| bad("words"))?,
        target,
        unit: unit
            .map(|unit| unit.trim().to_string().try_into().map_err(|_| bad("unit")))
            .transpose()?,
        server: server
            .map(|server| server.to_string().try_into().map_err(|_| bad("service")))
            .transpose()?,
    };
    let seq = append(tools, EventBody::GoalSet(body))?;
    Ok(CommandReport {
        said: if edit.is_some() {
            format!("Changed the goal {id}.")
        } else {
            format!("Set the goal {id}.")
        },
        events: vec![seq],
    })
}

fn id_body(goal: &str) -> Result<GoalIdBody, CommandError> {
    Ok(GoalIdBody {
        goal: goal
            .to_string()
            .try_into()
            .map_err(|_| refused("goal_unknown", format!("{goal} is not a goal's id")))?,
    })
}

/// Pauses the goal that stands. Refused `goal_unknown`, and `goal_already_paused`.
pub(crate) fn pause_goal(tools: &ToolDeps, goal: &str) -> Result<CommandReport, CommandError> {
    let _pausing = crate::locked(&GOALS);
    if standing_goal(tools, goal)?.paused {
        return Err(refused(
            "goal_already_paused",
            format!("{goal} is paused already"),
        ));
    }
    let seq = append(tools, EventBody::GoalPaused(id_body(goal)?))?;
    Ok(CommandReport {
        said: format!("Paused the goal {goal}."),
        events: vec![seq],
    })
}

/// Resumes the goal, or, on one that is not paused, tells it to keep going: the owner's word,
/// which clears a stall. Refused `goal_unknown`.
pub(crate) fn resume_goal(tools: &ToolDeps, goal: &str) -> Result<CommandReport, CommandError> {
    let _resuming = crate::locked(&GOALS);
    standing_goal(tools, goal)?;
    let seq = append(tools, EventBody::GoalResumed(id_body(goal)?))?;
    Ok(CommandReport {
        said: format!("The goal {goal} goes on."),
        events: vec![seq],
    })
}

/// Ends the goal, reached or dropped. Refused `goal_unknown`.
pub(crate) fn end_goal(
    tools: &ToolDeps,
    goal: &str,
    outcome: GoalOutcome,
) -> Result<CommandReport, CommandError> {
    let _ending = crate::locked(&GOALS);
    standing_goal(tools, goal)?;
    let body = GoalEndedBody {
        goal: id_body(goal)?.goal,
        outcome: match outcome {
            GoalOutcome::Reached => GoalEndedBodyOutcome::Reached,
            GoalOutcome::Dropped => GoalEndedBodyOutcome::Dropped,
        },
    };
    let seq = append(tools, EventBody::GoalEnded(body))?;
    Ok(CommandReport {
        said: match outcome {
            GoalOutcome::Reached => format!("The goal {goal} is reached."),
            GoalOutcome::Dropped => format!("Ended the goal {goal}."),
        },
        events: vec![seq],
    })
}

/// Records the owner's number for the goal. Refused `goal_unknown`, `goal_not_measured` for a goal
/// with no target and `goal_value` for a number below 0 or not finite.
pub(crate) fn report_goal(
    tools: &ToolDeps,
    goal: &str,
    value: f64,
) -> Result<CommandReport, CommandError> {
    let _reporting = crate::locked(&GOALS);
    let standing = standing_goal(tools, goal)?;
    if standing.target.is_none() {
        return Err(refused(
            "goal_not_measured",
            "this goal has no number to report",
        ));
    }
    if !value.is_finite() || value < 0.0 {
        return Err(refused(
            "goal_value",
            "a number is 0 or more, and a real number",
        ));
    }
    let body = GoalMeasuredBody {
        goal: id_body(goal)?.goal,
        value,
        source: GoalMeasuredBodySource::Owner,
        server: None,
        note: None,
    };
    let seq = append(tools, EventBody::GoalMeasured(body))?;
    Ok(CommandReport {
        said: format!("Noted {value} for the goal {goal}."),
        events: vec![seq],
    })
}

/// How many readings `goal.get` answers.
const READINGS_SHOWN: usize = 10;

fn status_word(status: GoalStatus) -> (&'static str, Option<&'static str>) {
    match status {
        GoalStatus::Active => ("active", None),
        GoalStatus::Paused => ("paused", None),
        GoalStatus::Met => ("met", None),
        GoalStatus::Stalled(StallReason::NothingPlanned) => ("stalled", Some("nothing_planned")),
        GoalStatus::Stalled(StallReason::NoProgress) => ("stalled", Some("no_progress")),
    }
}

/// Whether `agent`'s newest goal session has not ended.
fn planning(log: &EventLog, agent: &str) -> Result<bool, StoreError> {
    let started = log.read(&EventQuery {
        kinds: vec![EventKind::SessionStarted],
        agent_id: Some(agent.to_string()),
        newest_first: true,
        ..EventQuery::default()
    })?;
    let Some(newest) = started.iter().find(|event| {
        matches!(&event.body, EventBody::SessionStarted(body)
            if body.purpose == SessionStartedBodyPurpose::Goal)
    }) else {
        return Ok(false);
    };
    let Some(session) = newest.envelope.ids.session_id.as_deref() else {
        return Ok(false);
    };
    let ended = log.read(&EventQuery {
        kinds: vec![EventKind::SessionEnded],
        agent_id: Some(agent.to_string()),
        after_seq: Some(newest.envelope.seq),
        ..EventQuery::default()
    })?;
    Ok(!ended
        .iter()
        .any(|event| event.envelope.ids.session_id.as_deref() == Some(session)))
}

/// The answer to `goal.get`: the goal that stands with its progress, and the last one ended.
///
/// # Errors
///
/// The log or the board could not be read.
pub(crate) fn goal_get(tools: &ToolDeps, team: &Team) -> Result<Value, StoreError> {
    let book = goal_book(&tools.log)?;
    let last_ended = book.last_ended.as_ref().map(|ended| {
        json!({
            "goal": ended.id,
            "words": ended.words,
            "outcome": match ended.outcome {
                GoalOutcome::Reached => "reached",
                GoalOutcome::Dropped => "dropped",
            },
            "at": ended.at,
        })
    });
    let Some(goal) = book.standing else {
        return Ok(json!({ "goal": null, "last_ended": last_ended }));
    };
    let board = tools.projections.board()?;
    let open: Vec<_> = board
        .iter()
        .filter(|row| !is_terminal(row.status))
        .collect();
    let mut done = 0;
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Accepted)
    {
        if goal_of(&tools.log, &row.task_id)?.as_deref() == Some(goal.id.as_str()) {
            done += 1;
        }
    }
    let waiting = open
        .iter()
        .filter(|row| {
            matches!(row.status, TaskStatus::Escalated | TaskStatus::Blocked)
                || row.waiting_on_human
                || row.awaiting_approval
        })
        .count();
    let planner = planner(team);
    let running = match planner {
        Some(agent) => planning(&tools.log, agent.id.as_str())?,
        None => false,
    };
    let (status, stall) = status_word(goal.status());
    let readings: Vec<_> = goal
        .readings
        .iter()
        .rev()
        .take(READINGS_SHOWN)
        .map(|reading| {
            json!({
                "value": reading.value,
                "source": match reading.source {
                    ReadingSource::Connector => "connector",
                    ReadingSource::Owner => "owner",
                },
                "server": reading.server,
                "agent": reading.agent,
                "at": reading.at,
            })
        })
        .collect();
    Ok(json!({
        "goal": {
            "goal": goal.id,
            "words": goal.words,
            "target": goal.target,
            "unit": goal.unit,
            "server": goal.server,
            "reader": goal.server.as_deref()
                .and_then(|server| reader(team, server))
                .map(|agent| agent.id.as_str()),
            "planner": planner.map(|agent| agent.id.as_str()),
            "status": status,
            "stall": stall,
            "readings": readings,
            "open": open.len(),
            "done": done,
            "waiting_on_owner": waiting,
            "planning": running,
        },
        "last_ended": last_ended,
    }))
}
