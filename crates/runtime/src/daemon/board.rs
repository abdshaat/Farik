//! The board's, the costs', the channel's and the chats' queries for the browser
//! (`docs/SPEC.md` 4.3, 5.9, F17): what a task cost by purpose, today's spending per agent, the
//! harness metrics, and the channel and each chat a page at a time. `web.rs` answers the frames; this module answers what they ask.

use std::collections::BTreeMap;
use std::fmt::Display;

use catervas_core::contract::{Role, TaskId};
use catervas_core::team::{AgentStatus, custom_server};
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind, MessageKind};
use catervas_store::{CostScope, CostWindow, EventQuery, HarnessMetrics};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, UNKNOWN_QUERY};
use crate::allowances::allowance_period;
use crate::chat::{ChatWaiting, chat_page, chat_waiting};
use crate::tools::ToolDeps;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 6] = [
    "task.costs",
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
        "costs.summary" => costs_summary(deps),
        "channel.messages" => channel_messages(deps, params),
        "chats.list" => chats_list(deps),
        "chat.messages" => chat_messages(deps, params),
        "metrics" => {
            let metrics = deps
                .projections
                .metrics(&deps.files)
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
    let granted = deps
        .projections
        .log()
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::TaskRoomGranted],
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    Ok(json!({
        "total_usd": spent.values().sum::<f64>(),
        "by_purpose": in_words(spent),
        "limit_usd": contract.budget.max_cost_usd + crate::cost::room_granted(&granted).1,
    }))
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
    let names = attached_names(deps)?;
    let messages: Vec<Value> = page
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::MessagePosted(body) => {
                let mut message = json!({
                    "seq": event.envelope.seq,
                    "at": event.envelope.recorded_at,
                    "author": body.author,
                    "kind": body.kind,
                    "text": body.text,
                    "mentions": body.mentions,
                    "thread": body.thread,
                    "in_reply_to": body.in_reply_to,
                    "task_id": event.envelope.ids.task_id,
                });
                with_attachments(&mut message, body.attachments.as_ref(), &names);
                Some(message)
            }
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
    let names = attached_names(deps)?;
    let messages: Vec<Value> = page
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::ChatMessagePosted(body) => {
                let mut message = json!({
                    "seq": event.envelope.seq,
                    "at": event.envelope.recorded_at,
                    "author": body.author,
                    "text": body.text,
                    "in_reply_to": body.in_reply_to,
                    "request": body.request,
                    "heard": body.heard,
                    "sent_as": sent
                        .iter()
                        .find(|(seq, _)| *seq == event.envelope.seq)
                        .map(|(_, task_id)| task_id),
                });
                with_attachments(&mut message, body.attachments.as_ref(), &names);
                Some(message)
            }
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

/// The name of each file the owner attached, by its path, from each path's newest
/// `document.attached`.
fn attached_names(deps: &ToolDeps) -> Result<BTreeMap<String, String>, Failure> {
    let mut names = BTreeMap::new();
    for event in deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::DocumentAttached],
            newest_first: true,
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?
    {
        if let EventBody::DocumentAttached(body) = event.body {
            names
                .entry(body.path.to_string())
                .or_insert_with(|| body.name.to_string());
        }
    }
    Ok(names)
}

/// Sets `message`'s `attachments` to `[{ path, name }]`, and leaves the key out when it names none.
fn with_attachments(
    message: &mut Value,
    paths: Option<&catervas_protocol::event::AttachmentPaths>,
    names: &BTreeMap<String, String>,
) {
    let chips: Vec<Value> = paths
        .into_iter()
        .flat_map(|paths| paths.iter())
        .map(|path| {
            let path = path.as_str();
            json!({ "path": path, "name": names.get(path).map_or(path, String::as_str) })
        })
        .collect();
    if !chips.is_empty() {
        message["attachments"] = Value::Array(chips);
    }
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

/// `costs.summary {}`: today's spending (UTC) and the daily limit, today's one-to-one chats, and
/// each agent's that is not retired, in the team's order.
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
    let of = |spent: &BTreeMap<String, f64>, agent: &str| spent.get(agent).copied().unwrap_or(0.0);
    Ok(json!({
        "today_usd": today.values().sum::<f64>(),
        "daily_limit_usd": team.budgets.daily_usd,
        "conversations_today_usd": of(&purposes_today, "chat"),
        "agents": team
            .agents
            .iter()
            .filter(|agent| agent.status != AgentStatus::Retired)
            .map(|agent| {
                let id = agent.id.as_str();
                json!({
                    "agent_id": id,
                    "today_usd": of(&agents_today, id),
                })
            })
            .collect::<Vec<_>>(),
    }))
}

/// `allowances.list {}`: each allowance of a connected kit connector of an active agent with the
/// calls made of its tool this period, counted as the hook counts them (ADR 0037): through the
/// daemon's counts, so the page and the hook never disagree. The period is the UTC day.
pub(super) fn allowances(state: &DaemonState, deps: &ToolDeps) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let period = allowance_period(deps.clock.now());
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
    let period = json!({ "kind": "day", "day": period.day.to_string() });
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

/// `metrics {}`'s wire, built here from `HarnessMetrics`, which has no wire of its own.
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
    fn says_which_session_an_agent_is_in() {
        let harness = Harness::new("board-activity", |_| {});
        harness.ready("CTV-1");
        harness.started_session("CTV-1", "dev-a", "s-7", "implement");
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
                "task_id": "CTV-1",
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
        harness.ready("CTV-1");
        spent(
            &harness,
            (Some("CTV-1"), "dev-a", "s1"),
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
        harness.accepted("CTV-1");
        let today = at().to_rfc3339();
        for (agent, session, purpose, usd) in [
            ("dev-a", "s1", "implement", 1.0),
            ("dev-a", "s2", "implement", 2.0),
            ("dev-b", "s3", "verify", 4.0),
        ] {
            spent(
                &harness,
                (Some("CTV-1"), agent, session),
                purpose,
                usd,
                &today,
            );
        }
        // Yesterday's talk: not today's.
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
            &json!({ "task_id": "CTV-1" }),
            "taskCostsResult",
        );
        let limit = harness.project.file("CTV-1")["budget"]["max_cost_usd"].clone();
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

        // Today's plan of another task: today's and dev-a's.
        harness.ready("CTV-2");
        spent(
            &harness,
            (Some("CTV-2"), "dev-a", "s5"),
            "plan",
            0.5,
            &today,
        );
        // An earlier day's checks: dev-b's and dev-c's, not today's.
        spent(
            &harness,
            (Some("CTV-1"), "dev-b", "s6"),
            "verify",
            16.0,
            "2026-09-21T12:00:00Z",
        );
        spent(
            &harness,
            (Some("CTV-1"), "dev-c", "s7"),
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
                "agents": [
                    { "agent_id": "pm", "today_usd": 0.0 },
                    { "agent_id": "dev-a", "today_usd": 3.5 },
                    { "agent_id": "dev-b", "today_usd": 4.0 },
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
        harness.ready("CTV-1");
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
            "On CTV-1 now.",
            &json!({
                "agent_id": "dev-a",
                "task_id": "CTV-1",
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
                    "text": "On CTV-1 now.", "mentions": ["pm"], "thread": "standup",
                    "in_reply_to": asked, "task_id": "CTV-1",
                },
            ])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_each_messages_attachments() {
        let harness = Harness::new("board-attachments", |_| {});
        let spec = "docs/catervas/product/attached/spec.md";
        let design = "docs/catervas/architecture/attached/design.md";
        for (path, name) in [(spec, "Spec.docx"), (design, "Plan.pdf")] {
            put(
                &harness,
                &json!({
                    "seq": 1, "recorded_at": at().to_rfc3339(), "team_id": "catervas",
                    "project_id": "catervas", "kind": "document.attached",
                    "body": { "path": path, "name": name, "media_type": "text/markdown" },
                }),
            );
        }
        let plain = said(&harness, "human", "human", "Plain.", &json!({}));
        let with = said(
            &harness,
            "human",
            "human",
            "Both.",
            &json!({ "body": { "attachments": [spec, design] } }),
        );
        let channel = query(
            &harness.daemon,
            "channel.messages",
            &json!({ "limit": 2 }),
            "channelMessagesResult",
        )["messages"]
            .clone();
        assert_eq!(channel[0]["seq"], plain);
        assert!(channel[0].get("attachments").is_none(), "{channel}");
        assert_eq!(channel[1]["seq"], with);
        assert_eq!(
            channel[1]["attachments"],
            json!([
                { "path": spec, "name": "Spec.docx" },
                { "path": design, "name": "Plan.pdf" },
            ])
        );

        let deps = &harness.project.deps;
        let post = |text: &str, attachments: Vec<String>| {
            crate::chat::post_chat(
                &deps.log,
                deps.clock.as_ref(),
                &deps.ids,
                crate::chat::NewChatMessage {
                    heard: None,
                    chat: "dev-a".to_string(),
                    author: "human".to_string(),
                    text: text.to_string(),
                    in_reply_to: None,
                    request: None,
                    session_id: None,
                    as_task: false,
                    attachments,
                },
            )
            .expect("the chat message is recorded")
        };
        post("No file.", Vec::new());
        post("A file.", vec![design.to_string()]);
        let chat = query(
            &harness.daemon,
            "chat.messages",
            &json!({ "agent_id": "dev-a" }),
            "chatMessagesResult",
        )["messages"]
            .clone();
        assert!(chat[0].get("attachments").is_none(), "{chat}");
        assert_eq!(
            chat[1]["attachments"],
            json!([{ "path": design, "name": "Plan.pdf" }])
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
                heard: None,
                chat: agent.to_string(),
                author: author.to_string(),
                text: text.to_string(),
                in_reply_to,
                request: None,
                session_id: None,
                as_task: false,
                attachments: Vec::new(),
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
    fn lists_what_a_reply_heard() {
        let harness = Harness::new("board-chat-heard", |_| {});
        let asked = chatted(&harness, "pm", "human", "That is everything.", None);
        let deps = &harness.project.deps;
        let reply = crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "pm".to_string(),
                author: "pm".to_string(),
                text: "Here is what I heard.".to_string(),
                in_reply_to: Some(asked),
                request: None,
                heard: Some(crate::chat::Heard {
                    customers: "Birthday bakers.".to_string(),
                    problem: "No free days shown.".to_string(),
                    must: "Show free days.\nBy month.".to_string(),
                    not_now: "Delivery.".to_string(),
                    works_when: "Ten orders a month.".to_string(),
                }),
                session_id: Some("session-1".to_string()),
                as_task: false,
                attachments: Vec::new(),
            },
        )
        .expect("the reply is recorded");
        let messages = query(
            &harness.daemon,
            "chat.messages",
            &json!({ "agent_id": "pm" }),
            "chatMessagesResult",
        )["messages"]
            .clone();
        assert_eq!(messages[0]["seq"], asked);
        assert_eq!(messages[0]["heard"], Value::Null);
        assert_eq!(messages[1]["seq"], reply);
        assert_eq!(
            messages[1]["heard"],
            json!({
                "customers": "Birthday bakers.",
                "problem": "No free days shown.",
                "must": "Show free days.\nBy month.",
                "not_now": "Delivery.",
                "works_when": "Ten orders a month."
            })
        );
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
                heard: None,
                chat: "dev-a".to_string(),
                author: "dev-a".to_string(),
                text: "Two.\nLines kept.".to_string(),
                in_reply_to: Some(one),
                request: Some(crate::chat::ProposedRequest {
                    title: "Let customers pay with Apple Pay".to_string(),
                    text: "Add Apple Pay at checkout, beside the card form.".to_string(),
                }),
                session_id: Some("session-1".to_string()),
                as_task: false,
                attachments: Vec::new(),
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
                    "heard": null,
                    "sent_as": null,
                },
                {
                    "seq": three, "at": at_now, "author": "human", "text": "Three.",
                    "in_reply_to": null, "request": null, "heard": null, "sent_as": null,
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
                    heard: None,
                    chat: "dev-a".to_string(),
                    author: "dev-a".to_string(),
                    text: text.to_string(),
                    in_reply_to: None,
                    request: Some(crate::chat::ProposedRequest {
                        title: "Let customers pay with Apple Pay".to_string(),
                        text: "Add Apple Pay at checkout, beside the card form.".to_string(),
                    }),
                    session_id: None,
                    as_task: false,
                    attachments: Vec::new(),
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
                (sent, json!("CTV-1")),
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
        said(&harness, "catervas", "system", "CTV-1 moved.", &json!({}));

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
