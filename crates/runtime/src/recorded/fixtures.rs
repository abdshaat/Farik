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

/// `dev-b`'s verify session of FRK-1: a review note, and nothing else. Hand-written.
#[must_use]
pub fn review_writes_note() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_writes_note.jsonl"))
}

/// `dev-b`'s verify session of FRK-1 that answers no `review` criterion: a review note, and
/// nothing else. Hand-written.
#[must_use]
pub fn review_answers_nothing() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_answers_nothing.jsonl"))
}

/// The Product Manager's verify session of FRK-1: `accepted` asked for. Hand-written.
#[must_use]
pub fn accept_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/accept_frk_1.jsonl"))
}

/// The Product Manager's triage of FRK-1: `farik_triage_request` of size `large`, "A file and the
/// check that it exists.". Hand-written.
#[must_use]
pub fn triage_frk_1_large() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/triage_frk_1_large.jsonl"))
}

/// The Product Manager's refine session of FRK-1 that asks the human "Should done.txt be empty?"
/// with `farik_ask_human`, and nothing else. Hand-written.
#[must_use]
pub fn refine_asks_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_asks_frk_1.jsonl"))
}

/// The Product Manager's refine session of FRK-1 as an epic: `farik_write_contract` with its intent,
/// `product_manager` as assignee role and the human as reviewer, `done.txt` its one allowed path,
/// C1 (`command`, `test -f done.txt`) and C2 (`review`), one item out of scope, risk `low`, and a
/// budget of 5 dollars and 10 sessions. Hand-written.
#[must_use]
pub fn refine_writes_epic_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_writes_epic_frk_1.jsonl"))
}

/// The Product Manager's refine session of FRK-1 as a task: `farik_write_contract` restating the
/// request's fields. Hand-written.
#[must_use]
pub fn refine_writes_task_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_writes_task_frk_1.jsonl"))
}

/// The Product Manager's plan session breaking epic FRK-1 down: `farik_create_task` of one task
/// under it, "Add done.txt", with the request's fields and a budget of 2 dollars. Hand-written.
#[must_use]
pub fn plan_breaks_down_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_breaks_down_frk_1.jsonl"))
}

/// The Product Manager's plan session assigning FRK-2 to `dev-a`, reviewed by `dev-b`, with
/// `farik_assign_task`. Hand-written.
#[must_use]
pub fn plan_assigns_frk_2() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_assigns_frk_2.jsonl"))
}

/// The Product Manager's plan session closing epic FRK-1 out: a completion note, "FRK-2 added
/// done.txt; nothing left out.", and `verifying` asked for. Hand-written.
#[must_use]
pub fn plan_closes_epic_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_closes_epic_frk_1.jsonl"))
}
