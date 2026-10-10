//! The board's, the sprints', the costs', the channel's and the chats' queries for the browser
//! (`docs/SPEC.md` 4.3, 5.5, 5.9, F17): what a task cost by purpose, each sprint with its meetings,
//! today's and this sprint's spending per agent, the harness metrics, and the channel and each
//! chat a page at a time. `web.rs` answers the frames; this module answers what they ask.

use std::collections::BTreeMap;
use std::fmt::Display;

use catervas_core::contract::{Role, TaskId, TaskStatus};
use catervas_core::sprint::Sprint;
use catervas_core::team::{AgentStatus, custom_server};
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind, MessageKind};
use catervas_store::{CostScope, CostWindow, EventQuery, HarnessMetrics, TaskProjection};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, UNKNOWN_QUERY};
use crate::allowances::{AllowancePeriod, allowance_period};
use crate::chat::{ChatWaiting, chat_page, chat_waiting};
use crate::tools::ToolDeps;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 8] = [
    "task.costs",
    "sprints.list",
    "sprint.get",
    "costs.summary",
    "metrics",
    "channel.messages",
    "chats.list",
    "chat.messages",
];

/// The purposes' plain words, in the order the pages show them. "Conversations" is the one-to-one
/// chats alone; the channel's `conversation` stays under "Meetings and talk".
const WORDS: [&str; 5] = [
    "Planning",
    "Building",
    "Checking",
    "Meetings and talk",
    "Conversations",
];

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
        "chats.list" => chats_list(deps),
        "chat.messages" => chat_messages(deps, params),
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
            "chat" => 4,
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
/// first assigner that planned it, or none when only Catervas did; what it spent; and how many of its
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
fn meetings(events: &[CatervasEvent], id: &str) -> Vec<Value> {
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

/// `chat.messages { agent_id, before_seq?, limit }`: the newest page of one agent's chat before
/// `before_seq`, oldest first within it, each reply with the request its proposal was sent as.
fn chat_messages(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let page = chat_page(
        &deps.log,
        params["agent_id"].as_str().unwrap_or_default(),
        params["before_seq"].as_u64(),
        params["limit"]
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or(100),
    )
    .map_err(|e| internal(&e))?;
    // ponytail: every task.created is read; an index on the link if chats grow past thousands.
    let sent: Vec<(u64, String)> = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter_map(|event| match event.body {
            EventBody::TaskCreated(body) => Some((
                body.from_chat_message?.get(),
                event.envelope.ids.task_id?.as_str().to_string(),
            )),
            _ => None,
        })
        .collect();
    let messages: Vec<Value> = page
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::ChatMessagePosted(body) => Some(json!({
                "seq": event.envelope.seq,
                "at": event.envelope.recorded_at,
                "author": body.author,
                "text": body.text,
                "in_reply_to": body.in_reply_to,
                "request": body.request,
                "sent_as": sent
                    .iter()
                    .find(|(seq, _)| *seq == event.envelope.seq)
                    .map(|(_, task_id)| task_id),
            })),
            _ => None,
        })
        .collect();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let waiting = chat_waiting(
        &deps.log,
        &deps.projections,
        &team,
        params["agent_id"].as_str().unwrap_or_default(),
        deps.clock.now(),
    )
    .map_err(|e| internal(&e))?
    .map(|waiting| match waiting {
        ChatWaiting::Asleep { until } => json!({ "because": waiting.because(), "until": until }),
        _ => json!({ "because": waiting.because() }),
    });
    Ok(json!({ "messages": messages, "waiting": waiting }))
}

/// `{ seq, at, author, text }` of a message in the channel or in a chat.
fn last_line(event: &CatervasEvent) -> Value {
    let (author, text) = match &event.body {
        EventBody::MessagePosted(body) => (&body.author, &body.text),
        EventBody::ChatMessagePosted(body) => (&body.author, &body.text),
        _ => return Value::Null,
    };
    json!({ "seq": event.envelope.seq, "at": event.envelope.recorded_at, "author": author, "text": text })
}

/// `chats.list {}`: the channel's newest line that is not Catervas's own, then every agent's chat in
/// the team's order, with its newest message.
fn chats_list(deps: &ToolDeps) -> Result<Value, Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    // ponytail: reads the whole channel newest first to skip Catervas's lines. Upgrade: a channel
    // projection holding its newest line that is not a system one.
    let channel = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::MessagePosted],
            newest_first: true,
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    let team_last = channel
        .iter()
        .find(|event| {
            matches!(&event.body, EventBody::MessagePosted(body) if body.kind != MessageKind::System)
        })
        .map_or(Value::Null, last_line);
    let mut chats = Vec::new();
    for agent in &team.agents {
        let last = chat_page(&deps.log, agent.id.as_str(), None, 1).map_err(|e| internal(&e))?;
        chats.push(json!({
            "agent_id": agent.id,
            "retired": agent.status == AgentStatus::Retired,
            "last": last.first().map_or(Value::Null, last_line),
        }));
    }
    Ok(json!({ "team_last": team_last, "chats": chats }))
}

/// `costs.summary {}`: today's spending (UTC) and the daily limit, today's one-to-one chats, the
/// last sprint's (the open one while one is), and each agent's that is not retired, in the team's
/// order.
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
    let purposes_today = sum_by(CostScope::Purpose, CostWindow::Day(day))?;
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
        "conversations_today_usd": of(&purposes_today, "chat"),
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

/// `allowances.list {}`: each allowance of a connected kit connector of an active agent with the
/// calls made of its tool this period, counted as the hook counts them (ADR 0037): through the
/// daemon's counts, so the page and the hook never disagree. The period is the open sprint's, or
/// else the UTC day's.
pub(super) fn allowances(state: &DaemonState, deps: &ToolDeps) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let period = allowance_period(&deps.log, &deps.projections, deps.clock.now())
        .map_err(|e| internal(&e))?;
    let connected: Vec<(String, String)> = super::team::connector_states(state, deps, &team)
        .into_iter()
        .filter(|row| row["state"] == "connected" && row["source"] == "kit")
        .map(|row| {
            (
                row["agent"].as_str().unwrap_or_default().to_string(),
                row["server"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let mut rows = Vec::new();
    for agent in team
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::Active)
    {
        let kit = (deps.kits)(Role::from(agent.role)).ok();
        for server in agent.mcp_servers.iter().flatten().filter_map(custom_server) {
            let id = agent.id.as_str();
            if !connected.contains(&(id.to_string(), server.name.clone())) {
                continue;
            }
            for (tool, of) in &server.allowances {
                let what = kit
                    .as_ref()
                    .and_then(|kit| kit_what(kit, &server.name, tool))
                    .unwrap_or_default();
                let used = state
                    .allowance_counts()
                    .used(&deps.log, &period, id, &server.name, tool)
                    .map_err(|e| internal(&e))?;
                rows.push(json!({
                    "agent": id, "server": server.name, "tool": tool,
                    "what": what, "used": used, "of": of,
                }));
            }
        }
    }
    let period = match &period {
        AllowancePeriod::Sprint { sprint_id, .. } => {
            json!({ "kind": "sprint", "sprint_id": sprint_id })
        }
        AllowancePeriod::Day { day, .. } => json!({ "kind": "day", "day": day.to_string() }),
    };
    Ok(json!({ "period": period, "rows": rows }))
}

/// The kit's plural noun for what a call of `tool` of its service `server` makes.
fn kit_what(kit: &catervas_roles::Kit, server: &str, tool: &str) -> Option<String> {
    kit.connectors.iter().find_map(|connector| match connector {
        catervas_roles::KitConnector::Server {
            entry, allowances, ..
        } if entry.name.as_str() == server => allowances.get(tool).map(|offer| offer.what.clone()),
        _ => None,
    })
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
    use catervas_core::team::AgentStatus;
    use catervas_protocol::event::{EventKind, NewEvent, event_from_value};
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

    #[test]
    fn counts_a_chat_as_conversations_not_meetings() {
        assert_eq!(
            super::in_words([("chat".to_string(), 3.0), ("ceremony".to_string(), 1.0)]),
            [
                json!({ "words": "Meetings and talk", "usd": 1.0 }),
                json!({ "words": "Conversations", "usd": 3.0 }),
            ]
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
                "team_id": "catervas",
                "project_id": "catervas",
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
            "team_id": "catervas",
            "project_id": "catervas",
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
        // S2 is planned by Catervas alone: a breakdown joining its epic's sprint.
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
                "team_id": "catervas",
                "project_id": "catervas",
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
    fn answers_the_conversations() {
        let harness = Harness::new("board-conversations", |_| {});
        let conversations = |harness: &Harness| {
            query(
                &harness.daemon,
                "costs.summary",
                &json!({}),
                "costsSummaryResult",
            )["conversations_today_usd"]
                .clone()
        };
        assert_eq!(conversations(&harness), json!(0.0));

        let today = at().to_rfc3339();
        spent(&harness, (None, "pm", "c1"), "chat", 0.25, &today);
        spent(&harness, (None, "dev-a", "c2"), "chat", 0.125, &today);
        // Channel talk, yesterday's chat and today's work are not today's conversations.
        spent(&harness, (None, "pm", "t1"), "conversation", 1.0, &today);
        spent(
            &harness,
            (None, "pm", "c0"),
            "chat",
            2.0,
            "2026-09-21T12:00:00Z",
        );
        harness.ready("FRK-1");
        spent(
            &harness,
            (Some("FRK-1"), "dev-a", "s1"),
            "plan",
            4.0,
            &today,
        );
        assert_eq!(conversations(&harness), json!(0.375));
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
                "conversations_today_usd": 0.0,
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
            "team_id": "catervas",
            "project_id": "catervas",
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
        let third = said(&harness, "catervas", "system", "Three.", &json!({}));

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

    /// A chat message in `agent`'s chat by `author`, answering `in_reply_to` when it is a reply.
    fn chatted(
        harness: &Harness,
        agent: &str,
        author: &str,
        text: &str,
        in_reply_to: Option<u64>,
    ) -> u64 {
        let deps = &harness.project.deps;
        crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: agent.to_string(),
                author: author.to_string(),
                text: text.to_string(),
                in_reply_to,
                request: None,
                session_id: None,
            },
        )
        .expect("the chat message is recorded")
    }

    fn chat_page(harness: &Harness, params: &Value) -> Vec<u64> {
        query(
            &harness.daemon,
            "chat.messages",
            params,
            "chatMessagesResult",
        )["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| message["seq"].as_u64().expect("a seq"))
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_chats_out_of_the_channel() {
        let harness = Harness::new("board-chats-apart", |_| {});
        let deps = &harness.project.deps;
        let morning = said(&harness, "human", "human", "Morning.", &json!({}));
        let asked = chatted(
            &harness,
            "dev-a",
            "human",
            "@dev-b could you look?\nThanks.",
            None,
        );
        let answered = chatted(&harness, "dev-a", "dev-a", "On it, privately.", Some(asked));

        let channel = page(&harness, &json!({}));
        assert!(channel.contains(&morning), "{channel:?}");
        assert!(
            !channel.contains(&asked) && !channel.contains(&answered),
            "{channel:?}"
        );
        let summary = crate::channel::channel_summary(&deps.log, &deps.files).expect("summarised");
        assert!(summary.contains("Morning."), "{summary}");
        assert!(
            !summary.contains("could you look") && !summary.contains("privately"),
            "{summary}"
        );
        for agent in ["dev-a", "dev-b"] {
            assert!(
                crate::channel::pending_mentions(&deps.log, agent)
                    .expect("the log reads")
                    .is_empty(),
                "{agent}"
            );
        }
        // `@dev-b` in a chat to dev-a starts no conversation: dev-a's chat session answers it.
        let orchestrator = harness.orchestrator(harness.recorded(vec![
            crate::recorded::fixtures::chat_answers_with_a_request(),
        ]));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime is made")
            .block_on(orchestrator.tick())
            .expect("the tick runs");
        let started: Vec<_> = harness
            .events(&[EventKind::SessionStarted])
            .into_iter()
            .map(|event| match event.body {
                catervas_protocol::event::EventBody::SessionStarted(body) => {
                    (event.envelope.ids.agent_id, body.purpose.to_string())
                }
                other => panic!("a session's start, not {other:?}"),
            })
            .collect();
        assert_eq!(started, [(Some("dev-a".to_string()), "chat".to_string())]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn pages_one_chat() {
        let harness = Harness::new("board-chat-pages", |_| {});
        let one = chatted(&harness, "dev-a", "human", "One.", None);
        chatted(&harness, "dev-b", "human", "Another chat.", None);
        said(&harness, "human", "human", "The channel.", &json!({}));
        let deps = &harness.project.deps;
        let two = crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "dev-a".to_string(),
                author: "dev-a".to_string(),
                text: "Two.\nLines kept.".to_string(),
                in_reply_to: Some(one),
                request: Some(crate::chat::ProposedRequest {
                    title: "Let customers pay with Apple Pay".to_string(),
                    text: "Add Apple Pay at checkout, beside the card form.".to_string(),
                }),
                session_id: Some("session-1".to_string()),
            },
        )
        .expect("the reply is recorded");
        let three = chatted(&harness, "dev-a", "human", "Three.", None);

        assert_eq!(
            chat_page(&harness, &json!({ "agent_id": "dev-a" })),
            [one, two, three]
        );
        assert_eq!(
            chat_page(
                &harness,
                &json!({ "agent_id": "dev-a", "before_seq": three, "limit": 1 })
            ),
            [two]
        );
        assert_eq!(
            chat_page(&harness, &json!({ "agent_id": "dev-a", "before_seq": two })),
            [one]
        );
        assert_eq!(
            chat_page(&harness, &json!({ "agent_id": "pm" })),
            Vec::<u64>::new()
        );
        let messages = query(
            &harness.daemon,
            "chat.messages",
            &json!({ "agent_id": "dev-a", "limit": 2 }),
            "chatMessagesResult",
        )["messages"]
            .clone();
        let at_now = at().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(
            messages,
            json!([
                {
                    "seq": two, "at": at_now, "author": "dev-a", "text": "Two.\nLines kept.",
                    "in_reply_to": one,
                    "request": {
                        "title": "Let customers pay with Apple Pay",
                        "text": "Add Apple Pay at checkout, beside the card form."
                    },
                    "sent_as": null,
                },
                {
                    "seq": three, "at": at_now, "author": "human", "text": "Three.",
                    "in_reply_to": null, "request": null, "sent_as": null,
                },
            ])
        );
        for params in [
            json!({ "agent_id": "dev-a", "limit": 0 }),
            json!({ "agent_id": "dev-a", "limit": 201 }),
            json!({}),
        ] {
            let refused = super::super::gates::tests::rpc(
                &harness.daemon,
                "query",
                &json!({ "name": "chat.messages", "params": params }),
            );
            assert_eq!(refused["error"]["code"], -32602, "{refused}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn shows_what_was_sent() {
        let harness = super::super::gates::tests::driven("board-chat-sent");
        let deps = &harness.project.deps;
        let proposal = |text: &str| {
            crate::chat::post_chat(
                &deps.log,
                deps.clock.as_ref(),
                &deps.ids,
                crate::chat::NewChatMessage {
                    chat: "dev-a".to_string(),
                    author: "dev-a".to_string(),
                    text: text.to_string(),
                    in_reply_to: None,
                    request: Some(crate::chat::ProposedRequest {
                        title: "Let customers pay with Apple Pay".to_string(),
                        text: "Add Apple Pay at checkout, beside the card form.".to_string(),
                    }),
                    session_id: None,
                },
            )
            .expect("the reply is recorded")
        };
        let asked = chatted(&harness, "dev-a", "human", "Apple Pay?", None);
        let sent = proposal("Here is one.");
        let unsent = proposal("And another.");
        super::super::gates::tests::call(
            &harness.daemon,
            "request.file",
            &json!({
                "text": "Let customers pay with Apple Pay at the checkout",
                "from_chat_message": sent,
            }),
            "requestFileResult",
        );

        let messages = query(
            &harness.daemon,
            "chat.messages",
            &json!({ "agent_id": "dev-a" }),
            "chatMessagesResult",
        )["messages"]
            .clone();
        let sent_as: Vec<(u64, Value)> = messages
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| {
                (
                    message["seq"].as_u64().expect("a seq"),
                    message["sent_as"].clone(),
                )
            })
            .collect();
        assert_eq!(
            sent_as,
            [
                (asked, Value::Null),
                (sent, json!("FRK-1")),
                (unsent, Value::Null)
            ]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_the_chats() {
        let harness = Harness::new("board-chats-list", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("agents")
                .push(json!({ "id": "old", "display_name": "Old", "role": "software_developer", "status": "retired" }));
        });
        let empty = query(&harness.daemon, "chats.list", &json!({}), "chatsListResult");
        assert_eq!(empty["team_last"], Value::Null, "{empty}");
        let asked = chatted(&harness, "dev-a", "human", "One.", None);
        let answered = chatted(&harness, "dev-a", "dev-a", "Two.", Some(asked));
        let past = chatted(&harness, "old", "human", "Still there?", None);
        let hi = said(&harness, "human", "human", "Hi team.", &json!({}));
        said(&harness, "catervas", "system", "FRK-1 moved.", &json!({}));

        let listed = query(&harness.daemon, "chats.list", &json!({}), "chatsListResult");

        let at_now = at().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let last = |seq: u64, author: &str, text: &str| json!({ "seq": seq, "at": at_now, "author": author, "text": text });
        assert_eq!(
            listed,
            json!({
                "team_last": last(hi, "human", "Hi team."),
                "chats": [
                    { "agent_id": "pm", "retired": false, "last": null },
                    { "agent_id": "dev-a", "retired": false, "last": last(answered, "dev-a", "Two.") },
                    { "agent_id": "dev-b", "retired": false, "last": null },
                    { "agent_id": "old", "retired": true, "last": last(past, "human", "Still there?") },
                ],
            })
        );
    }

    /// What `chat.messages` says `agent`'s chat waits on.
    fn waiting(harness: &Harness, agent: &str) -> Value {
        query(
            &harness.daemon,
            "chat.messages",
            &json!({ "agent_id": agent }),
            "chatMessagesResult",
        )["waiting"]
            .clone()
    }

    /// `agent`'s chat session `session` answering `in_reply_to`: its start, and its end when it
    /// ended.
    fn chat_session(harness: &Harness, agent: &str, session: &str, in_reply_to: u64, ended: bool) {
        let envelope = json!({
            "seq": 1, "recorded_at": at().to_rfc3339(), "team_id": "catervas", "project_id": "catervas",
            "agent_id": agent, "session_id": session,
        });
        let mut started = envelope.clone();
        started["kind"] = json!("session.started");
        started["body"] = json!({
            "purpose": "chat", "model": "claude-opus-5", "effort": "low",
            "in_reply_to": in_reply_to, "chat": agent,
        });
        put(harness, &started);
        if ended {
            let mut end = envelope;
            end["kind"] = json!("session.ended");
            end["body"] = json!({ "reason": "completed", "detail": "" });
            put(harness, &end);
        }
    }

    /// Runs one tick with `transcripts` and answers how many sessions it started.
    fn tick_with(harness: &Harness, transcripts: Vec<crate::recorded::Transcript>) -> usize {
        let adapter = harness.recorded(transcripts);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime is made")
            .block_on(harness.orchestrator(adapter.clone()).tick())
            .expect("the tick runs");
        adapter.started().len()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_why_a_chat_waits() {
        let harness = Harness::new("board-chat-waits", |_| {});
        assert_eq!(waiting(&harness, "dev-a"), Value::Null);
        let asked = chatted(&harness, "dev-a", "human", "Status?", None);
        let answering = json!({ "because": "answering" });
        assert_eq!(waiting(&harness, "dev-a"), answering);
        // While its session runs, and until the reply.
        chat_session(&harness, "dev-a", "session-1", asked, false);
        assert_eq!(waiting(&harness, "dev-a"), answering);
        chatted(&harness, "dev-a", "dev-a", "On it.", Some(asked));
        assert_eq!(waiting(&harness, "dev-a"), Value::Null);
        chat_session(&harness, "dev-a", "session-1", asked, true);
        assert_eq!(waiting(&harness, "dev-a"), Value::Null);

        // Each reason in turn, the later ones first in precedence.
        chatted(&harness, "dev-a", "human", "And now?", None);
        assert_eq!(waiting(&harness, "dev-a"), answering);
        harness.asleep("dev-a", at() + chrono::Duration::hours(1));
        assert_eq!(
            waiting(&harness, "dev-a"),
            json!({ "because": "asleep", "until": "2026-09-22T13:00:00Z" })
        );
        // Asleep, its chat starts no session.
        assert_eq!(
            tick_with(
                &harness,
                vec![crate::recorded::fixtures::chat_answers_with_a_request()]
            ),
            0
        );
        harness.spent(None, "s-0", 20.0);
        assert_eq!(
            waiting(&harness, "dev-a"),
            json!({ "because": "day_spent" })
        );
        // dev-b is awake: the spent day alone keeps its chat from starting a session.
        chatted(&harness, "dev-b", "human", "Status?", None);
        assert_eq!(
            waiting(&harness, "dev-b"),
            json!({ "because": "day_spent" })
        );
        assert_eq!(
            tick_with(
                &harness,
                vec![crate::recorded::fixtures::chat_answers_with_a_request()]
            ),
            0
        );
        harness.project.record(
            "",
            "team.paused",
            &json!({ "by": "catervas", "reason": "credential_refused", "detail": "401" }),
        );
        assert_eq!(
            waiting(&harness, "dev-a"),
            json!({ "because": "key_refused" })
        );
        let files = &harness.project.deps.files;
        let mut team = files.read_team().expect("the team");
        for agent in &mut team.agents {
            if agent.id.as_str() == "dev-a" {
                agent.status = AgentStatus::Retired;
            }
        }
        files.write_team(&team).expect("the team is written");
        assert_eq!(waiting(&harness, "dev-a"), json!({ "because": "retired" }));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_no_answer_after_a_failed_session() {
        let harness = Harness::new("board-chat-no-answer", |_| {});
        chatted(&harness, "dev-a", "human", "Status?", None);
        // A session that ends without catervas_chat_reply: its note is not one of a chat's tools.
        let answer_nothing = crate::recorded::fixtures::review_answers_nothing;
        assert_eq!(tick_with(&harness, vec![answer_nothing()]), 1);
        assert_eq!(
            waiting(&harness, "dev-a"),
            json!({ "because": "no_answer" })
        );
        // No second session until the user writes again.
        assert_eq!(tick_with(&harness, vec![answer_nothing()]), 0);
        chatted(&harness, "dev-a", "human", "Status, please?", None);
        assert_eq!(
            waiting(&harness, "dev-a"),
            json!({ "because": "answering" })
        );
        assert_eq!(tick_with(&harness, vec![answer_nothing()]), 1);
    }
}
