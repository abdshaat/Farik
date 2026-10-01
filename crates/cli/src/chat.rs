//! `farik chat <agent>`: the human's one-to-one chat with one agent, from the log (`docs/SPEC.md`
//! 4.3).

use farik_protocol::event::EventBody;
use farik_runtime::chat::chat_page;
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// The chat's messages, oldest first, one block each: `<time> <author>`, the text with its line
/// breaks, and a proposed request's title and text; blocks apart by a blank line. With `--json`,
/// one object per line. What an agent wrote is escaped when it is printed.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn chat(project: &Project, agent: &str) -> Result<Report, String> {
    // ponytail: the whole chat in one read; page it when a chat grows past what a terminal shows.
    let events =
        chat_page(&project.log, agent, None, usize::MAX).map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    let mut messages = Vec::new();
    for event in &events {
        let EventBody::ChatMessagePosted(body) = &event.body else {
            continue;
        };
        let time = event
            .envelope
            .recorded_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("{time} {}", body.author));
        lines.push(body.text.clone());
        if let Some(request) = &body.request {
            lines.push(format!("Proposed request: {}", request.title));
            lines.push(request.text.clone());
        }
        let mut message = json!(body);
        message["seq"] = json!(event.envelope.seq);
        message["recorded_at"] = json!(time);
        messages.push(message);
    }
    if lines.is_empty() {
        lines.push(format!(
            "no messages yet: farik chat {agent} <text> sends one"
        ));
    }
    Ok(Report {
        lines,
        json: json!({ "messages": messages }),
        json_lines: Some(messages.into_iter().collect::<Vec<Value>>()),
    })
}
