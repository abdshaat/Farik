//! Transcripts recorded from `claude` 2.1.280 with `--model haiku` on 2026-09-22, trimmed of
//! hook lines, thinking signatures, and machine paths; and transcripts of Farik tool calls,
//! hand-written in the same shapes, which no recording can hold because the answers depend on the
//! project a test builds. For tests, in this crate and in others.

use std::path::PathBuf;

use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::team::Effort;

use super::Transcript;
use crate::session::{SessionPurpose, SessionSpec};

/// An implement session for `maya-chen` in `/workspace`, with the team's default limits.
#[must_use]
pub fn a_session_spec() -> SessionSpec {
    SessionSpec {
        session_id: "0b7e4f0e-5a3c-4c55-9d6f-2f1a8c3e9b10".to_string(),
        agent_id: "maya-chen".to_string(),
        task_id: None,
        purpose: SessionPurpose::Implement,
        system_prompt: "You are a Software Developer.".to_string(),
        model: "claude-haiku-4-5-20251001".to_string(),
        effort: Effort::High,
        farik_tools: vec!["farik_read_task".to_string()],
        builtin_tools: ["Glob", "Grep", "Read", "ToolSearch"]
            .map(str::to_string)
            .to_vec(),
        mcp_servers: Vec::new(),
        cwd: PathBuf::from("/workspace"),
        limits: DEFAULT_SESSION_LIMITS,
        initial_prompt: "Read note.txt.".to_string(),
    }
}

/// A Read of `/workspace/note.txt`, its result, the text `hello fixture`, and a successful end.
#[must_use]
pub fn reads_a_file() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/reads_a_file.jsonl"))
}

/// A Write refused by a safety check, the model's reply, and a successful end.
#[must_use]
pub fn write_denied() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/write_denied.jsonl"))
}

/// A Read call, then the end a one-turn limit forces.
#[must_use]
pub fn hits_the_turn_limit() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/hits_the_turn_limit.jsonl"))
}

/// A Read, then a Write the `PreToolUse` hook denied with `farik says no`, and a successful end.
#[must_use]
pub fn hook_denies_a_write() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/hook_denies_a_write.jsonl"))
}

/// A call of `mcp__farik__farik_read_board` with no input, its answer, and a successful end.
/// Hand-written in the shapes above; with `RecordedAdapter::with_tools` the answer is the runner's.
#[must_use]
pub fn replays_farik_read_board() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/replays_farik_read_board.jsonl"))
}

/// The Product Manager's plan session: `farik_assign_task` of FRK-1 to `dev-a`, reviewed by
/// `dev-b`, and a successful end. Hand-written.
#[must_use]
pub fn plan_assigns_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_assigns_frk_1.jsonl"))
}

/// `dev-a`'s implement session of FRK-1, done: `touch done.txt` through `farik_exec`, a commit of
/// it, C1 recorded as passed, a completion note, and `verifying` asked for. Hand-written.
#[must_use]
pub fn implement_finishes_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/implement_finishes_frk_1.jsonl"))
}

/// `dev-a`'s implement session of FRK-1, stopped early: `touch done.txt`, a commit of it, and a
/// progress note saying C1 has not run. Hand-written.
#[must_use]
pub fn implement_stops_early() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/implement_stops_early.jsonl"))
}
