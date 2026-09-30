//! The board's, the sprints', the costs' and the channel's queries for the browser (`docs/SPEC.md`
//! 5.5, 5.9, F17): what a task cost by purpose, each sprint with its meetings, today's and this
//! sprint's spending per agent, the harness metrics, and the channel a page at a time. `web.rs`
//! answers the frames; this module answers what they ask.

use std::collections::BTreeMap;
use std::fmt::Display;

use farik_core::contract::{TaskId, TaskStatus};
use farik_core::sprint::Sprint;
use farik_core::team::AgentStatus;
use farik_protocol::event::{EventBody, EventKind, FarikEvent, MessageKind};
use farik_store::{CostScope, CostWindow, EventQuery, HarnessMetrics, TaskProjection};
use serde_json::{Value, json};

use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, UNKNOWN_QUERY};
use crate::tools::ToolDeps;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 6] = [
    "task.costs",
    "sprints.list",
    "sprint.get",
    "costs.summary",
    "metrics",
    "channel.messages",
];

/// The purposes' plain words, in the order the pages show them.
const WORDS: [&str; 4] = ["Planning", "Building", "Checking", "Meetings and talk"];

fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, params: &Value) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    match name {
        "task.costs" => task_costs(deps, params),
        "sprints.list" => {
            let listed = sprints(deps)?;
            Ok(json!({ "sprints": listed.into_iter().map(|(_, row)| row).collect::<Vec<_>>() }))
        }
        "sprint.get" => sprint_get(deps, params["sprint_id"].as_str().unwrap_or_default()),
        "costs.summary" => costs_summary(deps),
        "channel.messages" => channel_messages(deps, params),
        "metrics" => {
            let metrics = match params["sprint_id"].as_str() {
                Some(sprint) => deps.projections.metrics_for_sprint(&deps.files, sprint),
                None => deps.projections.metrics(&deps.files),
            }
            .map_err(|e| internal(&e))?;
            Ok(metrics_wire(&metrics))
        }
        _ => Err(Failure::new(
            UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// `[{ words, usd }]`: `spent`, by purpose as the log writes it, summed into the purposes' plain
/// words, each word that any purpose of `spent` falls under, in `WORDS`' order.
fn in_words(spent: impl IntoIterator<Item = (String, f64)>) -> Vec<Value> {
    let mut summed: BTreeMap<usize, f64> = BTreeMap::new();
    for (purpose, usd) in spent {
        let word = match purpose.as_str() {
            "triage" | "refine" | "plan" | "explore" => 0,
            "implement" => 1,
            "verify" => 2,
            _ => 3,
        };
        *summed.entry(word).or_default() += usd;
    }
    summed
        .into_iter()
        .map(|(word, usd)| json!({ "words": WORDS[word], "usd": usd }))
        .collect()
}

/// `task.costs { task_id }`: what the task spent by purpose, the whole, and its contract's limit.
fn task_costs(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let asked = params["task_id"].as_str().unwrap_or_default();
    let missing = || Failure::new(NOT_FOUND, format!("there is no task {asked}"));
    let task_id: TaskId = asked.parse().map_err(|_| missing())?;
    deps.projections
        .task(&task_id)
        .map_err(|e| internal(&e))?
        .ok_or_else(missing)?;
    let contract = deps
        .files
        .read_contract(&task_id)
        .map_err(|e| internal(&e))?;
    let spent = deps
        .projections
        .costs_by_purpose(&task_id)
        .map_err(|e| internal(&e))?;
    Ok(json!({
        "total_usd": spent.values().sum::<f64>(),
        "by_purpose": in_words(spent),
        "limit_usd": contract.budget.max_cost_usd,
    }))
}

/// Each sprint's file, beside its row: the file's dates, status and budget; who started it; the
/// first assigner that planned it, or none when only Farik did; what it spent; and how many of its
/// tasks are done (accepted or cancelled), as `sprint.current` counts them.
fn sprints(deps: &ToolDeps) -> Result<Vec<(Sprint, Value)>, Failure> {
    let events = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::SprintStarted, EventKind::SprintPlanned],
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    let spent: BTreeMap<String, f64> = deps
        .projections
        .costs(CostScope::Sprint)
        .map_err(|e| internal(&e))?
        .into_iter()
        .map(|row| (row.key, row.usd))
        .collect();
    let board = board(deps)?;
    let files = deps.files.list_sprints().map_err(|e| internal(&e))?;
    Ok(files
        .into_iter()
        .map(|sprint| {
            let id = sprint.id.as_str();
            let started_by = events.iter().find_map(|event| match &event.body {
                EventBody::SprintStarted(body) if body.sprint_id.as_str() == id => {
                    Some(body.started_by.clone())
                }
                _ => None,
            });
            let planned_by = events.iter().find_map(|event| match &event.body {
                EventBody::SprintPlanned(body)
                    if body.sprint_id.as_str() == id && body.planned_by != "governor" =>
                {
                    Some(body.planned_by.clone())
                }
                _ => None,
            });
            let done = sprint
                .task_ids
                .iter()
                .filter(|task| {
                    board.get(task.as_str()).is_some_and(|row| {
                        matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled)
                    })
                })
                .count();
            let row = json!({
                "sprint_id": id,
                "status": sprint.status,
                "started_at": sprint.started_at,
                "started_by": started_by,
                "ended_at": sprint.ended_at,
                "budget_usd": sprint.budget_usd,
                "spent_usd": spent.get(id).copied().unwrap_or(0.0),
                "planned_by": planned_by,
                "task_count": sprint.task_ids.len(),
                "done_count": done,
            });
            (sprint, row)
        })
        .collect())
}

/// The board's rows, by task id.
fn board(deps: &ToolDeps) -> Result<BTreeMap<String, TaskProjection>, Failure> {
    Ok(deps
        .projections
        .board()
        .map_err(|e| internal(&e))?
        .into_iter()
        .map(|row| (row.task_id.to_string(), row))
        .collect())
}

/// `sprint.get { sprint_id }`: the sprint's row, its tasks, and its meetings.
fn sprint_get(deps: &ToolDeps, id: &str) -> Result<Value, Failure> {
    let (sprint, mut row) = sprints(deps)?
        .into_iter()
        .find(|(sprint, _)| sprint.id.as_str() == id)
        .ok_or_else(|| Failure::new(NOT_FOUND, format!("there is no sprint {id}")))?;
    let board = board(deps)?;
    row["tasks"] = json!(
        sprint
            .task_ids
            .iter()
            .filter_map(|task| board.get(task.as_str()))
            .map(|task| json!({ "task_id": task.task_id, "title": task.title, "status": task.status }))
            .collect::<Vec<_>>()
    );
    let events = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::SprintStarted, EventKind::MessagePosted],
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    row["meetings"] = json!(meetings(&events, id));
    Ok(row)
}

/// The ceremonies' posts from sprint `id`'s start until the next sprint starts, so that the review
/// and the look back, which run after its end, are its: one meeting per ceremony session, oldest
/// first, with its first post's seq and time and how many posts it has.
fn meetings(events: &[FarikEvent], id: &str) -> Vec<Value> {
    let Some(start) = events.iter().rposition(|event| {
        matches!(&event.body, EventBody::SprintStarted(body) if body.sprint_id.as_str() == id)
    }) else {
        return Vec::new();
    };
    let mut held: Vec<(Option<&str>, Value)> = Vec::new();
    for event in &events[start + 1..] {
        let body = match &event.body {
            EventBody::SprintStarted(_) => break,
            EventBody::MessagePosted(body) if body.kind == MessageKind::Ceremony => body,
            _ => continue,
        };
        let session = event.envelope.ids.session_id.as_deref();
        let thread = json!(body.thread);
        match held
            .iter_mut()
            .find(|(one, meeting)| *one == session && meeting["thread"] == thread)
        {
            Some((_, meeting)) => {
                meeting["posts"] = json!(meeting["posts"].as_u64().unwrap_or(0) + 1);
            }
            None => held.push((
                session,
                json!({
                    "thread": thread,
                    "first_seq": event.envelope.seq,
                    "at": event.envelope.recorded_at,
                    "posts": 1,
                }),
            )),
        }
    }
    held.into_iter().map(|(_, meeting)| meeting).collect()
}

/// `channel.messages { before_seq?, limit }`: the newest page of the channel before `before_seq`,
/// oldest first within it.
fn channel_messages(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let mut page = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::MessagePosted],
            before_seq: params["before_seq"].as_u64(),
            newest_first: true,
            limit: Some(
                params["limit"]
                    .as_u64()
                    .and_then(|limit| usize::try_from(limit).ok())
                    .unwrap_or(100),
            ),
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    page.reverse();
    let messages: Vec<Value> = page
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::MessagePosted(body) => Some(json!({
                "seq": event.envelope.seq,
                "at": event.envelope.recorded_at,
                "author": body.author,
                "kind": body.kind,
                "text": body.text,
                "mentions": body.mentions,
                "thread": body.thread,
                "in_reply_to": body.in_reply_to,
                "task_id": event.envelope.ids.task_id,
            })),
            _ => None,
        })
        .collect();
    Ok(json!({ "messages": messages }))
}

/// `costs.summary {}`: today's spending (UTC) and the daily limit, the last sprint's (the open one
/// while one is), and each agent's that is not retired, in the team's order.
fn costs_summary(deps: &ToolDeps) -> Result<Value, Failure> {
    let day = deps.clock.now().date_naive();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let sum_by = |scope, window| -> Result<BTreeMap<String, f64>, Failure> {
        Ok(deps
            .projections
            .costs_for(scope, window)
            .map_err(|e| internal(&e))?
            .into_iter()
            .map(|row| (row.key, row.usd))
            .collect())
    };
    let today = sum_by(CostScope::Day, CostWindow::Day(day))?;
    let agents_today = sum_by(CostScope::Agent, CostWindow::Day(day))?;
    // The open sprint, else the last one, so that its figures do not read $0.00 once it ends.
    let last = deps.files.list_sprints().map_err(|e| internal(&e))?.pop();
    let (sprint, agents_sprint) = match &last {
        None => (Value::Null, BTreeMap::new()),
        Some(last) => {
            let window = || CostWindow::Sprint(last.id.as_str().to_string());
            let spent = sum_by(CostScope::Sprint, window())?;
            (
                json!({
                    "sprint_id": last.id,
                    "status": last.status,
                    "spent_usd": spent.values().sum::<f64>(),
                    "budget_usd": last.budget_usd,
                }),
                sum_by(CostScope::Agent, window())?,
            )
        }
    };
    let of = |spent: &BTreeMap<String, f64>, agent: &str| spent.get(agent).copied().unwrap_or(0.0);
    Ok(json!({
        "today_usd": today.values().sum::<f64>(),
        "daily_limit_usd": team.budgets.daily_usd,
        "sprint": sprint,
        "agents": team
            .agents
            .iter()
            .filter(|agent| agent.status != AgentStatus::Retired)
            .map(|agent| {
                let id = agent.id.as_str();
                json!({
                    "agent_id": id,
                    "today_usd": of(&agents_today, id),
                    "sprint_usd": of(&agents_sprint, id),
                })
            })
            .collect::<Vec<_>>(),
    }))
}

/// `metrics { sprint_id? }`'s wire, built here from `HarnessMetrics`, which has no wire of its own.
fn metrics_wire(metrics: &HarnessMetrics) -> Value {
    let messages = &metrics.messages;
    json!({
        "accepted_tasks": metrics.accepted_tasks,
        "first_pass_acceptance_rate": metrics.first_pass_acceptance_rate,
        "interventions_per_accepted_task": metrics.interventions_per_accepted_task,
        "cost_per_accepted_task": metrics.cost_per_accepted_task_usd.as_ref().map(|split| json!({
            "total_usd": split.total,
            "by_purpose": in_words(
                split.by_purpose.iter().map(|(purpose, usd)| (purpose.to_string(), *usd)),
            ),
        })),
        "mechanically_verified_criteria_share": metrics.mechanically_verified_criteria_share,
        "active_weeks": metrics.active_weeks,
        "messages": {
            "reaction": messages.reaction,
            "ambient": messages.ambient,
            "reply": messages.reply,
            "ceremony": messages.ceremony,
            "system": messages.system,
            "human": messages.human,
        },
    })
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{NewEvent, event_from_value};
    use serde_json::{Value, json};

    use super::super::gates::tests::query;
    use crate::orchestrator::fixtures::Harness;
    use crate::sprints::{EndedBy, PlannedBy, end_sprint, plan_sprint};
    use crate::tools::fixtures::at;

    #[test]
    fn counts_exploring_as_planning() {
        assert_eq!(
            super::in_words([("explore".to_string(), 1.5), ("plan".to_string(), 0.5)]),
            [json!({ "words": "Planning", "usd": 2.0 })]
        );
    }

    /// Appends one event of `wire`'s shape and projects it, and answers its seq.
    fn put(harness: &Harness, wire: &Value) -> u64 {
        let event = event_from_value(wire).expect("the fixture is schema-valid");
        let deps = &harness.project.deps;
        let appended = deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        deps.projections.apply(&appended).expect("projects");
        appended.envelope.seq
    }

    /// A ceremony line by `agent` in its session `session`, in `thread`.
    fn posted(harness: &Harness, agent: &str, session: &str, thread: &str) -> u64 {
        put(
            harness,
            &json!({
                "seq": 1,
                "recorded_at": at().to_rfc3339(),
                "team_id": "farik",
                "project_id": "farik",
                "agent_id": agent,
                "session_id": session,
                "kind": "message.posted",
                "body": { "author": agent, "kind": "ceremony", "text": "Notes.", "mentions": [], "thread": thread },
            }),
        )
    }

    /// A cost of `usd` by `agent` in `session` for `purpose`, on `task` when one is named, at
    /// `recorded_at`.
    fn spent(
        harness: &Harness,
        (task, agent, session): (Option<&str>, &str, &str),
        purpose: &str,
        usd: f64,
        recorded_at: &str,
    ) {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": recorded_at,
            "team_id": "farik",
            "project_id": "farik",
            "agent_id": agent,
            "session_id": session,
            "kind": "cost.recorded",
            "body": {
                "purpose": purpose,
                "model_id": "claude-opus-5",
                "usage": { "input_tokens": 1, "output_tokens": 1, "cache_read_tokens": 0, "cache_write_tokens": 0 },
                "cost_usd": usd
            },
        });
        if let Some(task) = task {
            wire["task_id"] = json!(task);
        }
        put(harness, &wire);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "two sprints, their list, one's tasks and meetings, and a missing one"
    )]
    fn answers_the_sprints() {
        let harness = Harness::new("board-sprints", |_| {});
        let deps = &harness.project.deps;
        harness.accepted("FRK-1");
        harness.ready("FRK-2");
        harness.file("FRK-4", "cancelled", |_| {});
        harness
            .project
            .open_sprint("S1", Some(20.0), &["FRK-1", "FRK-2", "FRK-4"]);
        let planning = posted(&harness, "pm", "c1", "planning");
        posted(&harness, "pm", "c1", "planning");
        // A reply in the planning thread is talk, not the meeting.
        said(
            &harness,
            "dev-a",
            "reply",
            "Sounds right.",
            &json!({ "session_id": "c1", "body": { "thread": "planning" } }),
        );
        // Each day's standup is a meeting of its own, in the same thread.
        let monday = posted(&harness, "pm", "c5", "standup");
        let tuesday = posted(&harness, "pm", "c6", "standup");
        spent(
            &harness,
            (Some("FRK-1"), "dev-a", "s1"),
            "implement",
            1.5,
            &at().to_rfc3339(),
        );
        end_sprint(deps, EndedBy::Human).expect("the sprint ends");
        // The review and the look back run after the end, and are the sprint's.
        let review = posted(&harness, "pm", "c2", "review");
        let retro = posted(&harness, "pm", "c3", "retro");
        // S2 is planned by Farik alone: a breakdown joining its epic's sprint.
        harness.ready("FRK-3");
        harness.project.open_sprint("S2", None, &[]);
        plan_sprint(
            deps,
            &["FRK-3".parse().expect("an id")],
            &PlannedBy::Governor,
        )
        .expect("the governor plans");
        posted(&harness, "pm", "c4", "standup");

        let listed = query(
            &harness.daemon,
            "sprints.list",
            &json!({}),
            "sprintsListResult",
        );
        let at_now = at().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(
            listed["sprints"],
            json!([
                {
                    "sprint_id": "S1", "status": "ended", "started_at": "2026-09-24T00:00:00Z",
                    "started_by": "human", "ended_at": at_now, "budget_usd": 20.0,
                    "spent_usd": 1.5, "planned_by": "pm", "task_count": 3, "done_count": 2,
                },
                {
                    "sprint_id": "S2", "status": "open", "started_at": "2026-09-24T00:00:00Z",
                    "started_by": "human", "ended_at": null, "budget_usd": null,
                    "spent_usd": 0.0, "planned_by": null, "task_count": 1, "done_count": 0,
                },
            ]),
            "{listed}"
        );

        let one = query(
            &harness.daemon,
            "sprint.get",
            &json!({ "sprint_id": "S1" }),
            "sprintGetResult",
        );
        assert_eq!(one["planned_by"], "pm", "{one}");
        assert_eq!(
            one["tasks"]
                .as_array()
                .expect("tasks")
                .iter()
                .map(|task| (task["task_id"].clone(), task["status"].clone()))
                .collect::<Vec<_>>(),
            vec![
                (json!("FRK-1"), json!("accepted")),
                (json!("FRK-2"), json!("ready")),
                (json!("FRK-4"), json!("cancelled")),
            ],
            "{one}"
        );
        assert_eq!(
            one["meetings"],
            json!([
                { "thread": "planning", "first_seq": planning, "at": at_now, "posts": 2 },
                { "thread": "standup", "first_seq": monday, "at": at_now, "posts": 1 },
                { "thread": "standup", "first_seq": tuesday, "at": at_now, "posts": 1 },
                { "thread": "review", "first_seq": review, "at": at_now, "posts": 1 },
                { "thread": "retro", "first_seq": retro, "at": at_now, "posts": 1 },
            ]),
            "{one}"
        );
        let later = query(
            &harness.daemon,
            "sprint.get",
            &json!({ "sprint_id": "S2" }),
            "sprintGetResult",
        );
        assert_eq!(
            later["meetings"].as_array().map(Vec::len),
            Some(1),
            "{later}"
        );
        let missing = super::super::gates::tests::rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "sprint.get", "params": { "sprint_id": "S9" } }),
        );
        assert_eq!(
            missing["error"]["code"],
            super::super::web::NOT_FOUND,
            "{missing}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_which_session_an_agent_is_in() {
        let harness = Harness::new("board-activity", |_| {});
        harness.ready("FRK-1");
        harness.started_session("FRK-1", "dev-a", "s-7", "implement");
        let of = |agent: &str| {
            let all = query(
                &harness.daemon,
                "team.activity",
                &json!({}),
                "teamActivityResult",
            );
            all["activity"]
                .as_array()
                .expect("activity")
                .iter()
                .find(|one| one["agent_id"] == agent)
                .cloned()
                .expect("the agent is listed")
        };
        let working = of("dev-a");
        assert_eq!(working["state"], "working", "{working}");
        assert_eq!(working["session_id"], "s-7", "{working}");
        assert_eq!(working["purpose"], "implement", "{working}");
        let idle = of("pm");
        assert!(
            idle.get("session_id").is_none() && idle.get("purpose").is_none(),
            "{idle}"
        );

        put(
            &harness,
            &json!({
                "seq": 1,
                "recorded_at": at().to_rfc3339(),
                "team_id": "farik",
                "project_id": "farik",
                "task_id": "FRK-1",
                "agent_id": "dev-a",
                "session_id": "s-7",
                "kind": "session.ended",
                "body": { "reason": "completed", "detail": "done" },
            }),
        );
        let ended = of("dev-a");
        assert!(
            ended.get("session_id").is_none() && ended.get("purpose").is_none(),
            "{ended}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "a task's costs, the summary with and without a limit, and three metrics scopes"
    )]
    fn answers_the_cost_summary_and_the_metrics() {
        let harness = Harness::new("board-costs", |wire| {
            wire["budgets"]["daily_usd"] = json!(10.0);
            // A retired agent is left out of the summary.
            wire["agents"].as_array_mut().expect("agents").push(json!({
                "id": "dev-c", "display_name": "dev-c", "role": "software_developer", "status": "retired",
            }));
        });
        let deps = &harness.project.deps;
        harness.accepted("FRK-1");
        harness.project.open_sprint("S1", Some(20.0), &["FRK-1"]);
        let today = at().to_rfc3339();
        for (agent, session, purpose, usd) in [
            ("dev-a", "s1", "implement", 1.0),
            ("dev-a", "s2", "implement", 2.0),
            ("dev-b", "s3", "verify", 4.0),
        ] {
            spent(
                &harness,
                (Some("FRK-1"), agent, session),
                purpose,
                usd,
                &today,
            );
        }
        // Yesterday's talk: not today's, and in no sprint.
        spent(
            &harness,
            (None, "dev-b", "s4"),
            "conversation",
            8.0,
            "2026-09-21T12:00:00Z",
        );

        let task = query(
            &harness.daemon,
            "task.costs",
            &json!({ "task_id": "FRK-1" }),
            "taskCostsResult",
        );
        let limit = harness.project.file("FRK-1")["budget"]["max_cost_usd"].clone();
        assert_eq!(
            task,
            json!({
                "by_purpose": [
                    { "words": "Building", "usd": 3.0 },
                    { "words": "Checking", "usd": 4.0 },
                ],
                "total_usd": 7.0,
                "limit_usd": limit,
            })
        );

        // Today's plan of a task in no sprint: today's and dev-a's, not the sprint's.
        harness.ready("FRK-2");
        spent(
            &harness,
            (Some("FRK-2"), "dev-a", "s5"),
            "plan",
            0.5,
            &today,
        );
        // An earlier day's check of the sprint's task: the sprint's and dev-b's, not today's.
        spent(
            &harness,
            (Some("FRK-1"), "dev-b", "s6"),
            "verify",
            16.0,
            "2026-09-21T12:00:00Z",
        );
        spent(
            &harness,
            (Some("FRK-1"), "dev-c", "s7"),
            "verify",
            0.25,
            "2026-09-21T12:00:00Z",
        );

        let summary = |harness: &Harness| {
            query(
                &harness.daemon,
                "costs.summary",
                &json!({}),
                "costsSummaryResult",
            )
        };
        assert_eq!(
            summary(&harness),
            json!({
                "today_usd": 7.5,
                "daily_limit_usd": 10.0,
                "sprint": { "sprint_id": "S1", "status": "open", "spent_usd": 23.25, "budget_usd": 20.0 },
                "agents": [
                    { "agent_id": "pm", "today_usd": 0.0, "sprint_usd": 0.0 },
                    { "agent_id": "dev-a", "today_usd": 3.5, "sprint_usd": 3.0 },
                    { "agent_id": "dev-b", "today_usd": 4.0, "sprint_usd": 20.0 },
                ],
            })
        );

        let whole = query(&harness.daemon, "metrics", &json!({}), "metricsResult");
        assert_eq!(whole["accepted_tasks"], 1, "{whole}");
        assert!(whole["first_pass_acceptance_rate"].is_number(), "{whole}");
        assert_eq!(
            whole["cost_per_accepted_task"],
            json!({
                "total_usd": 31.75,
                "by_purpose": [
                    { "words": "Planning", "usd": 0.5 },
                    { "words": "Building", "usd": 3.0 },
                    { "words": "Checking", "usd": 20.25 },
                    { "words": "Meetings and talk", "usd": 8.0 },
                ],
            }),
            "{whole}"
        );
        assert_eq!(whole["active_weeks"], 1, "{whole}");
        assert_eq!(
            whole["messages"],
            json!({ "reaction": 0, "ambient": 0, "reply": 0, "ceremony": 0, "system": 0, "human": 0 }),
            "{whole}"
        );
        let sprint = query(
            &harness.daemon,
            "metrics",
            &json!({ "sprint_id": "S1" }),
            "metricsResult",
        );
        assert_eq!(
            sprint["cost_per_accepted_task"]["total_usd"], 23.25,
            "{sprint}"
        );
        let none = query(
            &harness.daemon,
            "metrics",
            &json!({ "sprint_id": "S9" }),
            "metricsResult",
        );
        assert_eq!(none["accepted_tasks"], 0, "{none}");
        for rate in [
            "first_pass_acceptance_rate",
            "interventions_per_accepted_task",
            "cost_per_accepted_task",
            "mechanically_verified_criteria_share",
        ] {
            assert_eq!(none[rate], Value::Null, "{rate}: {none}");
        }

        // Once the sprint ends, the summary keeps its figures and says it ended, until the next.
        end_sprint(deps, EndedBy::Human).expect("the sprint ends");
        let ended = summary(&harness);
        assert_eq!(
            ended["sprint"],
            json!({ "sprint_id": "S1", "status": "ended", "spent_usd": 23.25, "budget_usd": 20.0 }),
            "{ended}"
        );
        assert_eq!(ended["agents"][2]["sprint_usd"], 20.0, "{ended}");

        // With no daily limit set, the summary says so.
        let mut team = deps.files.read_team().expect("the team reads");
        team.budgets.daily_usd = None;
        deps.files.write_team(&team).expect("the team is written");
        assert_eq!(summary(&harness)["daily_limit_usd"], Value::Null);
    }

    /// A message by `author` of `kind` saying `text`, with `extra` merged over the event's wire.
    fn said(harness: &Harness, author: &str, kind: &str, text: &str, extra: &Value) -> u64 {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "kind": "message.posted",
            "body": { "author": author, "kind": kind, "text": text, "mentions": [] },
        });
        for (key, value) in extra.as_object().expect("an object") {
            if key == "body" {
                for (field, one) in value.as_object().expect("an object") {
                    wire["body"][field] = one.clone();
                }
            } else {
                wire[key] = value.clone();
            }
        }
        put(harness, &wire)
    }

    fn page(harness: &Harness, params: &Value) -> Vec<u64> {
        query(
            &harness.daemon,
            "channel.messages",
            params,
            "channelMessagesResult",
        )["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| message["seq"].as_u64().expect("a seq"))
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn pages_the_channel() {
        let harness = Harness::new("board-channel-pages", |_| {});
        let zero = said(&harness, "human", "human", "Zero.", &json!({}));
        let first = said(&harness, "human", "human", "One.", &json!({}));
        // Events that are not messages are not the channel's.
        harness.ready("FRK-1");
        let second = said(&harness, "human", "human", "Two.", &json!({}));
        let third = said(&harness, "farik", "system", "Three.", &json!({}));

        assert_eq!(page(&harness, &json!({ "limit": 2 })), [second, third]);
        // The page counts messages only, however many other events stand between them.
        let before = page(&harness, &json!({ "before_seq": second, "limit": 2 }));
        assert_eq!(before, [zero, first]);
        let all = page(&harness, &json!({}));
        assert_eq!(
            all[all.len() - 4..],
            [zero, first, second, third],
            "{all:?}"
        );
        for limit in [0, 201] {
            let refused = super::super::gates::tests::rpc(
                &harness.daemon,
                "query",
                &json!({ "name": "channel.messages", "params": { "limit": limit } }),
            );
            assert_eq!(refused["error"]["code"], -32602, "{refused}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn carries_each_messages_links() {
        let harness = Harness::new("board-channel-links", |_| {});
        let asked = said(&harness, "human", "human", "@dev-a look?", &json!({}));
        let reply = said(
            &harness,
            "dev-a",
            "reply",
            "On FRK-1 now.",
            &json!({
                "agent_id": "dev-a",
                "task_id": "FRK-1",
                "body": { "mentions": ["pm"], "thread": "standup", "in_reply_to": asked },
            }),
        );
        let messages = query(
            &harness.daemon,
            "channel.messages",
            &json!({ "limit": 2 }),
            "channelMessagesResult",
        )["messages"]
            .clone();
        let at_now = at().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(
            messages,
            json!([
                {
                    "seq": asked, "at": at_now, "author": "human", "kind": "human",
                    "text": "@dev-a look?", "mentions": [], "thread": null,
                    "in_reply_to": null, "task_id": null,
                },
                {
                    "seq": reply, "at": at_now, "author": "dev-a", "kind": "reply",
                    "text": "On FRK-1 now.", "mentions": ["pm"], "thread": "standup",
                    "in_reply_to": asked, "task_id": "FRK-1",
                },
            ])
        );
    }
}
