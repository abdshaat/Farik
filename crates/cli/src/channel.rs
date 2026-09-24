//! `farik channel`: the team's channel, from the log (`docs/SPEC.md` sections 3 and 5.9).

use farik_protocol::event::{EventBody, EventKind};
use farik_store::EventQuery;
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// The latest `last` messages, oldest first, one line each: `<time> <author> [<thread>] <text>`.
/// With `--json`, one object per line. What an agent wrote is escaped when it is printed.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn channel(project: &Project, last: usize) -> Result<Report, String> {
    // ponytail: reads every message to keep the last ones; a projection when the channel grows.
    let events = project
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::MessagePosted],
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    let mut messages = Vec::new();
    for event in &events[events.len().saturating_sub(last)..] {
        let EventBody::MessagePosted(body) = &event.body else {
            continue;
        };
        let time = event
            .envelope
            .recorded_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let thread = body
            .thread
            .map_or_else(String::new, |thread| format!(" [{thread}]"));
        lines.push(format!("{time} {}{thread} {}", body.author, body.text));
        let mut message = json!(body);
        message["seq"] = json!(event.envelope.seq);
        message["recorded_at"] = json!(time);
        message["task_id"] = json!(event.envelope.ids.task_id.as_ref().map(|id| id.as_str()));
        messages.push(message);
    }
    if lines.is_empty() {
        lines.push("no messages yet: farik say posts one".to_string());
    }
    Ok(Report {
        lines,
        json: json!({ "messages": messages }),
        json_lines: Some(messages.into_iter().collect::<Vec<Value>>()),
    })
}
