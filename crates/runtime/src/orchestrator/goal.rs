//! The rules that keep the team working towards the owner's goal (`docs/SPEC.md` 5.18, ADR 0075):
//! one plans the next steps whenever the board is quiet, the other reads the goal's number once a
//! day. Both come last in a tick, after every rule that has work to do.

use std::fmt::Write as _;

use catervas_core::contract::Role;
use catervas_core::goal::{Goal, GoalStatus, ReadingSource, board_quiet};
use catervas_core::team::{Agent, Team};
use catervas_protocol::event::{
    EventBody, EventIds, EventKind, GoalPlannedBody, SessionStartedBodyPurpose, new_event,
};
use catervas_store::{EventQuery, TaskProjection};

use super::rules::{Waiting, day_is_spent, ran, unavailable};
use super::session::{SessionAsk, SessionEnd, run_session};
use super::{OrchestratorDeps, OrchestratorError, TickReport, TickRules, TickScope};
use crate::goals::{goal_book, planner, reader};
use crate::session::{EndReason, SessionPurpose};

/// The Catervas tools a planning session is offered: the reading tools, the two goal tools, its
/// one post and the notebook.
pub(super) const GOAL_PLAN_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_post_message",
    "catervas_write_memory",
    "catervas_file_goal_work",
    "catervas_record_goal_progress",
];

/// The one Catervas tool a measuring session is offered.
pub(super) const GOAL_MEASURE_TOOLS: &[&str] = &["catervas_record_goal_progress"];

/// How many readings a planning session is shown.
const READINGS_SHOWN: usize = 5;

/// The goal rules, last in a tick (5.18): the planning rule, then the measuring rule. They are
/// about no one task, so they run only under `All` in a tick scoped to none, and only for a goal
/// that stands, is not paused, and a team with a daily budget.
pub(super) async fn goal_rules(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if scope.rules != TickRules::All || scope.task_id.is_some() || team.budgets.daily_usd.is_none()
    {
        return Ok(None);
    }
    let Some(goal) = goal_book(&deps.tools.log)?
        .standing
        .filter(|goal| !goal.paused)
    else {
        return Ok(None);
    };
    if let Some(report) = plan(deps, team, board, &goal, waiting).await? {
        return Ok(Some(report));
    }
    measure(deps, team, &goal, waiting).await
}

/// The planning rule: the planner gets one `goal` session when the board is quiet and the goal is
/// neither met nor stalled.
async fn plan(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    goal: &Goal,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(planner) = planner(team) else {
        return Ok(None);
    };
    if goal.status() != GoalStatus::Active
        || !board_quiet(board.iter().map(|row| row.status))
        || deps.tools.transitions.awaits_the_product_plan()?
    {
        return Ok(None);
    }
    if day_is_spent(deps, team, Role::from(planner.role), &mut waiting.day_spent)?
        | unavailable(deps, planner, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let message = plan_message(deps, team, board, goal, planner)?;
    let end = goal_session(deps, team, planner, GOAL_PLAN_TOOLS, message).await?;
    // A session that was stopped found nothing and filed nothing: it is no round, and the next
    // tick with room plans again.
    if end.reason != EndReason::Aborted {
        let filed = filed_by(deps, goal, planner, &end.session_id)?;
        record_round(deps, goal, planner, &end.session_id, &filed)?;
    }
    Ok(Some(TickReport::Goal {
        goal_id: goal.id.clone(),
        what: ran(planner, "goal planning", &end),
    }))
}

/// The measuring rule: once per UTC day, the agent that reads the goal's service gets one `goal`
/// session offered the recording tool alone. It does not wait for a quiet board.
async fn measure(
    deps: &OrchestratorDeps,
    team: &Team,
    goal: &Goal,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(server) = goal.server.as_deref() else {
        return Ok(None);
    };
    let Some(reader) = reader(team, server) else {
        return Ok(None);
    };
    let today = deps.tools.clock.now().date_naive();
    if goal.readings.iter().any(|reading| {
        reading.source == ReadingSource::Connector && reading.at.date_naive() == today
    }) || goal_session_today(deps, reader)?
    {
        return Ok(None);
    }
    if day_is_spent(deps, team, Role::from(reader.role), &mut waiting.day_spent)?
        | unavailable(deps, reader, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let message = measure_message(goal, reader, server);
    let end = goal_session(deps, team, reader, GOAL_MEASURE_TOOLS, message).await?;
    Ok(Some(TickReport::Goal {
        goal_id: goal.id.clone(),
        what: ran(reader, "goal reading", &end),
    }))
}

/// Runs one goal session of `agent`: about no task, read-only, offered `tools`.
async fn goal_session(
    deps: &OrchestratorDeps,
    team: &Team,
    agent: &Agent,
    tools: &'static [&'static str],
    initial_prompt: String,
) -> Result<SessionEnd, OrchestratorError> {
    run_session(
        deps,
        team,
        SessionAsk {
            agent,
            contract: None,
            purpose: SessionPurpose::Goal,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: true,
            only_tool: None,
            tools: Some(tools),
            in_reply_to: None,
            thread: None,
            initial_prompt,
        },
    )
    .await
}

/// Whether `agent` started a goal session today (UTC).
// ponytail: the newest 500 sessions of the agent; a day with more than that misses an older one.
fn goal_session_today(deps: &OrchestratorDeps, agent: &Agent) -> Result<bool, OrchestratorError> {
    let today = deps.tools.clock.now().date_naive();
    Ok(deps
        .tools
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::SessionStarted],
            agent_id: Some(agent.id.to_string()),
            newest_first: true,
            limit: Some(500),
            ..EventQuery::default()
        })?
        .iter()
        .take_while(|event| event.envelope.recorded_at.date_naive() >= today)
        .any(|event| {
            matches!(&event.body, EventBody::SessionStarted(body)
                if body.purpose == SessionStartedBodyPurpose::Goal)
        }))
}

/// The requests the session filed towards the goal: each `task.created` with its goal after the
/// session's start.
fn filed_by(
    deps: &OrchestratorDeps,
    goal: &Goal,
    agent: &Agent,
    session_id: &str,
) -> Result<Vec<String>, OrchestratorError> {
    let log = &deps.tools.log;
    let started = log
        .read(&EventQuery {
            kinds: vec![EventKind::SessionStarted],
            agent_id: Some(agent.id.to_string()),
            newest_first: true,
            limit: Some(50),
            ..EventQuery::default()
        })?
        .iter()
        .find(|event| event.envelope.ids.session_id.as_deref() == Some(session_id))
        .map_or(0, |event| event.envelope.seq);
    Ok(log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            after_seq: Some(started),
            ..EventQuery::default()
        })?
        .iter()
        .filter(|event| {
            matches!(&event.body, EventBody::TaskCreated(body)
                if body.goal.as_ref().is_some_and(|id| id.as_str() == goal.id))
        })
        .filter_map(|event| {
            event
                .envelope
                .ids
                .task_id
                .as_ref()
                .map(|task| task.as_str().to_string())
        })
        .collect())
}

/// Records the round: `goal.planned` by the planner, in the session that ran it.
fn record_round(
    deps: &OrchestratorDeps,
    goal: &Goal,
    agent: &Agent,
    session_id: &str,
    filed: &[String],
) -> Result<(), OrchestratorError> {
    let refused = |what: &str| OrchestratorError::Refused {
        reason: format!("goal_round_unrecorded: the round's {what} is not one the log can hold"),
    };
    let body = GoalPlannedBody {
        goal: goal.id.clone().try_into().map_err(|_| refused("goal"))?,
        filed: filed
            .iter()
            .map(|task| task.clone().try_into().map_err(|_| refused("task")))
            .collect::<Result<_, _>>()?,
    };
    let ids = EventIds {
        agent_id: Some(agent.id.to_string()),
        session_id: Some(session_id.to_string()),
        ..deps.tools.ids.clone()
    };
    let event =
        new_event(EventBody::GoalPlanned(body), deps.tools.clock.now(), ids).map_err(|error| {
            OrchestratorError::Refused {
                reason: format!("goal_round_unrecorded: {error:?}"),
            }
        })?;
    let appended = deps.tools.log.append(&event)?;
    deps.tools.projections.apply(&appended)?;
    Ok(())
}

/// How old a reading is, in the words a planner reads.
fn days_ago(days: i64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        days => format!("{days} days ago"),
    }
}

/// The first message of a planning session (5.18).
fn plan_message(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    goal: &Goal,
    planner: &Agent,
) -> Result<String, OrchestratorError> {
    let mut message = format!(
        "{}, the owner's goal is: \u{201c}{}\u{201d}.\n",
        planner.display_name.as_str(),
        goal.words
    );
    match (goal.target, goal.unit.as_deref()) {
        (Some(target), Some(unit)) => {
            let _ = writeln!(message, "The number to reach: {target} {unit}.");
        }
        _ => message.push_str(
            "It has no number: the owner marks it reached when it is, so plan what builds it.\n",
        ),
    }
    let now = deps.tools.clock.now();
    let reading_server = goal.server.as_deref();
    match reading_server.and_then(|server| reader(team, server).map(|agent| (agent, server))) {
        Some((agent, server)) => {
            let _ = writeln!(
                message,
                "It is counted from {server}, which {} reads once a day.",
                agent.display_name.as_str()
            );
            if agent.id == planner.id {
                let _ = writeln!(
                    message,
                    "Read {server} first and record it with `catervas_record_goal_progress`."
                );
            }
        }
        None if goal.target.is_some() => {
            message.push_str("The owner tells the team the number.\n");
        }
        None => {}
    }
    if goal.readings.is_empty() {
        if goal.target.is_some() {
            message.push_str("Not counted yet.\n");
        }
    } else {
        message.push_str("The latest readings, newest first:\n");
        for reading in goal.readings.iter().rev().take(READINGS_SHOWN) {
            let from = match reading.source {
                ReadingSource::Owner => "you",
                ReadingSource::Connector => reading.server.as_deref().unwrap_or("a service"),
            };
            let _ = writeln!(
                message,
                "- {} from {from}, {}",
                reading.value,
                days_ago((now - reading.at).num_days())
            );
        }
    }
    message.push_str(&earlier_rounds(deps, board, goal)?);
    let unmerged: Vec<String> = board
        .iter()
        .filter(|row| row.awaiting_integration)
        .map(|row| format!("{} ({})", row.task_id.as_str(), row.title))
        .collect();
    if !unmerged.is_empty() {
        let _ = writeln!(
            message,
            "Accepted and waiting for the owner's merge: {}. A step that builds on one names it as its dependency.",
            unmerged.join("; ")
        );
    }
    if team.is_solo() {
        message.push_str("Each step you file is your own task; file small ones.\n");
    }
    message.push_str(
        "\nHow to plan:\n\
         - Read the progress and what earlier rounds did.\n\
         - Pick the next one to five steps that move the number or build what the goal names, each \
         small enough for one task or one epic, and file each with `catervas_file_goal_work`.\n\
         - Prefer what the team can finish over research it cannot act on.\n\
         - Never repeat work that is done or under way.\n\
         - Name a step that needs a service the team lacks as a request for that connection.\n\
         - When nothing sensible is left, file nothing and say why in one post with \
         `catervas_post_message`.\n",
    );
    Ok(message)
}

/// What each earlier round of the goal filed, and where each task stands.
fn earlier_rounds(
    deps: &OrchestratorDeps,
    board: &[TaskProjection],
    goal: &Goal,
) -> Result<String, OrchestratorError> {
    let mut message = String::new();
    let status_of = |task: &str| {
        board
            .iter()
            .find(|row| row.task_id.as_str() == task)
            .map_or_else(
                || "not on the board".to_string(),
                |row| format!("{}, {}", row.title, row.status),
            )
    };
    let rounds = deps.tools.log.read(&EventQuery {
        kinds: vec![EventKind::GoalPlanned],
        ..EventQuery::default()
    })?;
    let mut round = 0;
    for event in &rounds {
        let EventBody::GoalPlanned(body) = &event.body else {
            continue;
        };
        if body.goal.as_str() != goal.id {
            continue;
        }
        round += 1;
        if body.filed.is_empty() {
            let _ = writeln!(message, "Round {round} filed nothing.");
        } else {
            let tasks: Vec<String> = body
                .filed
                .iter()
                .map(|task| format!("{} ({})", task.as_str(), status_of(task.as_str())))
                .collect();
            let _ = writeln!(message, "Round {round} filed {}.", tasks.join("; "));
        }
    }
    Ok(message)
}

/// The first message of a measuring session.
fn measure_message(goal: &Goal, reader: &Agent, server: &str) -> String {
    let counted = match (goal.target, goal.unit.as_deref()) {
        (Some(target), Some(unit)) => format!("how many {unit} there are, towards {target}"),
        _ => "the goal's number".to_string(),
    };
    format!(
        "{}, read {server} and find {counted}, for the owner's goal \u{201c}{}\u{201d}. Record it \
         once with `catervas_record_goal_progress`: the number, and a short note on what you \
         counted. Change nothing. Then end the session.",
        reader.display_name.as_str(),
        goal.words
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use catervas_core::pricing::Usage;
    use catervas_protocol::command::Command;
    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::json;

    use crate::orchestrator::Orchestrator;
    use crate::orchestrator::fixtures::{Harness, UsageThenWaitAdapter};
    use crate::recorded::RecordedAdapter;
    use crate::recorded::fixtures::ends_without_a_decision;
    use crate::session::{SessionPurpose, SessionSpec};

    fn a_stripe() -> serde_json::Value {
        json!({
            "name": "stripe", "source": "custom", "transport": "stdio",
            "command": "stripe-mcp", "args": ["stdio"], "credential_keys": ["API_KEY"],
            "tools": { "read": "network" }
        })
    }

    /// A project whose team has a daily budget, with `change` applied to its wire as well.
    fn a_project(name: &str, change: impl FnOnce(&mut serde_json::Value)) -> Harness {
        Harness::new(name, |wire| {
            wire["budgets"]["daily_usd"] = json!(10);
            change(wire);
        })
    }

    fn sessions_of(adapter: &RecordedAdapter) -> Vec<SessionSpec> {
        adapter
            .started()
            .into_iter()
            .filter(|spec| spec.purpose == SessionPurpose::Goal)
            .collect()
    }

    async fn owner_sets(orchestrator: &Orchestrator, server: Option<&str>) {
        orchestrator
            .handle(Command::GoalSet {
                words: "Reach 10 paying customers".to_string(),
                target: Some(10.0),
                unit: Some("paying customers".to_string()),
                server: server.map(str::to_string),
            })
            .await
            .expect("the goal is set");
    }

    async fn owner_says(orchestrator: &Orchestrator, command: Command) {
        orchestrator
            .handle(command)
            .await
            .expect("the owner's word is taken");
    }

    fn report(value: f64) -> Command {
        Command::GoalReport {
            goal: "goal-1".to_string(),
            value,
        }
    }

    fn transcripts() -> Vec<crate::recorded::Transcript> {
        (0..6).map(|_| ends_without_a_decision()).collect()
    }

    fn rounds(harness: &Harness) -> Vec<usize> {
        harness
            .events(&[EventKind::GoalPlanned])
            .iter()
            .map(|event| match &event.body {
                EventBody::GoalPlanned(body) => body.filed.len(),
                other => panic!("a goal.planned, not {other:?}"),
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn plans_towards_the_goal_when_the_board_is_quiet() {
        let harness = a_project("goal-plan", |_| {});
        let adapter = harness.recorded(transcripts());
        let orchestrator = harness.orchestrator(adapter.clone());
        owner_sets(&orchestrator, None).await;

        orchestrator.tick().await.expect("the tick runs");
        let started = sessions_of(&adapter);
        assert_eq!(started.len(), 1);
        assert_eq!(
            (started[0].agent_id.as_str(), &started[0].task_id),
            ("pm", &None)
        );
        assert!(
            started[0]
                .initial_prompt
                .contains("Reach 10 paying customers"),
            "{}",
            started[0].initial_prompt
        );
        assert_eq!(rounds(&harness), [0], "a round that filed nothing");

        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            sessions_of(&adapter).len(),
            1,
            "nothing new, so no second round"
        );

        owner_says(&orchestrator, report(2.0)).await;
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(sessions_of(&adapter).len(), 2, "the owner's number is news");

        // Accepted work that waits for the owner's merge does not hold a round.
        let merging = a_project("goal-plan-merge", |_| {});
        let adapter = merging.recorded(transcripts());
        let orchestrator = merging.orchestrator(adapter.clone());
        owner_sets(&orchestrator, None).await;
        merging.accepted("CTV-2");
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(sessions_of(&adapter).len(), 1);

        // Open work does: a young blocked task is the board's, and no round starts beside it.
        let busy = a_project("goal-plan-busy", |_| {});
        let adapter = busy.recorded(transcripts());
        let orchestrator = busy.orchestrator(adapter.clone());
        owner_sets(&orchestrator, None).await;
        busy.blocked("CTV-1", "dev-a", "dev-b");
        orchestrator.tick().await.expect("the tick runs");
        assert!(sessions_of(&adapter).is_empty());

        // The board is quiet again, but the owner paused the goal.
        busy.project
            .moved("CTV-1", "blocked", "cancelled", &json!({}));
        owner_says(
            &orchestrator,
            Command::GoalPause {
                goal: "goal-1".to_string(),
            },
        )
        .await;
        orchestrator.tick().await.expect("the tick runs");
        assert!(sessions_of(&adapter).is_empty());

        // A session that was stopped found nothing and filed nothing: it is no round.
        let stopped = a_project("goal-plan-aborted", |wire| {
            wire["budgets"]["session"] = json!({ "max_input_tokens": 100 });
        });
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage {
            input_tokens: 1000,
            output_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }));
        let orchestrator = stopped.orchestrator(adapter.clone());
        owner_sets(&orchestrator, None).await;
        tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the session was stopped")
            .expect("the tick runs");
        assert!(rounds(&stopped).is_empty());
        tokio::time::timeout(Duration::from_secs(10), orchestrator.tick())
            .await
            .expect("the session was stopped")
            .expect("the tick runs");
        let goal_sessions = adapter
            .started()
            .iter()
            .filter(|spec| spec.purpose == SessionPurpose::Goal)
            .count();
        assert_eq!(goal_sessions, 2, "the next tick plans again");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn measures_the_goal_once_a_day() {
        let harness = a_project("goal-measure", |wire| {
            wire["policy"]["blocked_limit_hours"] = json!(700);
            wire["agents"][1]["mcp_servers"] = json!([a_stripe()]);
        });
        let adapter = harness.recorded(transcripts());
        let orchestrator = harness.orchestrator(adapter.clone());
        owner_sets(&orchestrator, Some("stripe")).await;
        // The board is not quiet, so no round starts and nothing else acts.
        harness.blocked("CTV-1", "dev-b", "pm");

        orchestrator.tick().await.expect("the tick runs");
        let started = sessions_of(&adapter);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].agent_id, "dev-a");
        assert_eq!(started[0].catervas_tools, ["catervas_record_goal_progress"]);

        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(sessions_of(&adapter).len(), 1, "once a day");

        let tomorrow = harness.project.deps.clock.now() + chrono::Duration::days(1);
        let later = harness.orchestrator_at(adapter.clone(), tomorrow);
        later.tick().await.expect("the tick runs");
        assert_eq!(sessions_of(&adapter).len(), 2, "and again the next day");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn one_agent_alone_plans_its_own_goal() {
        let harness = a_project("goal-solo", |wire| {
            wire["agents"] = json!([catervas_core::team::fixtures::an_agent_wire(
                "theo",
                "software_developer"
            )]);
            wire["policy"]["solo"] = json!(true);
        });
        let adapter = harness.recorded(transcripts());
        let orchestrator = harness.orchestrator(adapter.clone());
        owner_sets(&orchestrator, None).await;

        orchestrator.tick().await.expect("the tick runs");
        let started = sessions_of(&adapter);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].agent_id, "theo");
        assert!(
            started[0]
                .initial_prompt
                .contains("Each step you file is your own task"),
            "{}",
            started[0].initial_prompt
        );
    }
}
