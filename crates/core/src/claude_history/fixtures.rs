//! Synthetic Claude Code transcript lines for tests, in this crate and in others. None holds a real
//! conversation.

use serde_json::json;

use crate::pricing::Usage;

/// A hand-written transcript of Claude Code 2.1.296: one session, every kind of line the reader
/// meets, every text field `NEVER-KEPT-TEXT`.
pub const CLAUDE_CODE_2_1_296: &str = include_str!("fixtures/claude_code_2_1_296.jsonl");

/// A `user` line of a session at a time.
#[must_use]
pub fn a_turn_line(session_id: &str, cwd: &str, at: &str) -> String {
    json!({
        "type": "user", "sessionId": session_id, "cwd": cwd, "timestamp": at,
        "gitBranch": "main", "version": "2.1.296", "entrypoint": "cli",
        "message": {"role": "user", "content": "NEVER-KEPT-TEXT"},
    })
    .to_string()
}

/// An `assistant` line carrying one reply's usage.
#[must_use]
pub fn a_reply_line(
    session_id: &str,
    cwd: &str,
    at: &str,
    message_id: &str,
    model: &str,
    usage: Usage,
) -> String {
    json!({
        "type": "assistant", "sessionId": session_id, "cwd": cwd, "timestamp": at,
        "gitBranch": "main", "version": "2.1.296", "entrypoint": "cli",
        "message": {
            "id": message_id, "model": model,
            "content": [{"type": "text", "text": "NEVER-KEPT-TEXT"}],
            "usage": {
                "input_tokens": usage.input_tokens, "output_tokens": usage.output_tokens,
                "cache_read_input_tokens": usage.cache_read_tokens,
                "cache_creation_input_tokens": usage.cache_write_tokens,
            },
        },
    })
    .to_string()
}

/// Three `assistant` lines of one reply, `msg_01C`, as Claude Code writes them: one content block
/// each, a text block, a `Bash` tool use and a `CronCreate` with no `recurring` or `durable`.
pub const CRON_CREATE_LINES: &str = include_str!("fixtures/cron_create_2_1_296.jsonl");

/// What a `CronCreate` line says.
#[derive(Debug, Clone, Copy)]
pub struct CronCreateLine<'a> {
    /// The session it belongs to.
    pub session_id: &'a str,
    /// The folder Claude Code was in.
    pub cwd: &'a str,
    /// When the line was written.
    pub at: &'a str,
    /// The reply's message id.
    pub message_id: &'a str,
    /// The tool use's id.
    pub tool_use_id: &'a str,
    /// The schedule.
    pub cron: &'a str,
    /// `recurring`, when the call names it.
    pub recurring: Option<bool>,
    /// `durable`, when the call names it.
    pub durable: Option<bool>,
}

/// An `assistant` line with one `CronCreate` block, its prompt `NEVER-KEPT-TEXT`.
#[must_use]
pub fn a_cron_create_line(line: &CronCreateLine<'_>) -> String {
    let mut input = json!({"cron": line.cron, "prompt": "NEVER-KEPT-TEXT"});
    if let Some(recurring) = line.recurring {
        input["recurring"] = json!(recurring);
    }
    if let Some(durable) = line.durable {
        input["durable"] = json!(durable);
    }
    json!({
        "type": "assistant", "sessionId": line.session_id, "cwd": line.cwd, "timestamp": line.at,
        "version": "2.1.296", "entrypoint": "cli",
        "message": {
            "id": line.message_id, "model": "claude-opus-5-5",
            "content": [{"type": "tool_use", "id": line.tool_use_id, "name": "CronCreate", "input": input}],
            "usage": {"input_tokens": 1, "output_tokens": 2},
        },
    })
    .to_string()
}

/// An `ai-title` line.
#[must_use]
pub fn a_title_line(session_id: &str, title: &str) -> String {
    json!({"type": "ai-title", "sessionId": session_id, "aiTitle": title}).to_string()
}

/// A hand-written transcript of Claude Code 2.1.296 with connector and spreadsheet tool calls: one
/// session at home, every text and input field `NEVER-KEPT-TEXT`.
pub const CLAUDE_CODE_2_1_296_CONNECTORS: &str =
    include_str!("fixtures/claude_code_2_1_296_connectors.jsonl");

/// One connectors fixture per entry of `CHECKED_VERSIONS`.
pub const CONNECTOR_FIXTURES: [(&str, &str); 1] = [("2.1.296", CLAUDE_CODE_2_1_296_CONNECTORS)];
