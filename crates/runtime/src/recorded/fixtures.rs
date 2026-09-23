//! Transcripts recorded from `claude` 2.1.280 with `--model haiku` on 2026-09-22, trimmed of
//! hook lines, thinking signatures, and machine paths. For tests, in this crate and in others.

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
        disallowed_builtin_tools: vec!["Bash".to_string()],
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
