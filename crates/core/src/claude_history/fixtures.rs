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

/// An `ai-title` line.
#[must_use]
pub fn a_title_line(session_id: &str, title: &str) -> String {
    json!({"type": "ai-title", "sessionId": session_id, "aiTitle": title}).to_string()
}
