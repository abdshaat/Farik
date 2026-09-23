//! Builders for test events, usable by every crate's tests.

use serde_json::{Value, json};

use crate::event::{EventKind, NewEvent, event_from_value};

/// A schema-valid wire event of one kind, with that kind's body, and no optional envelope field
/// beyond the `task_id` that a contract-scoped kind may not be recorded without.
#[must_use]
pub fn an_event_wire(kind: EventKind) -> Value {
    let mut event = json!({
        "seq": 1,
        "recorded_at": "2026-09-17T10:00:00Z",
        "team_id": "farik",
        "project_id": "farik",
        "kind": kind.to_string(),
        "body": a_body_wire(kind)
    });
    if crate::event::is_about_one_contract(kind) {
        event["task_id"] = json!("FRK-1");
    }
    event
}

/// A schema-valid event of one kind, ready to append: what `new_event` would have produced, built
/// from `an_event_wire` so that a test of the store and a test of the protocol cannot drift apart.
///
/// # Panics
///
/// When `an_event_wire` stops being schema-valid, which is the fixture's own bug.
#[must_use]
pub fn a_new_event(kind: EventKind) -> NewEvent {
    let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
    NewEvent {
        recorded_at: event.envelope.recorded_at,
        ids: event.envelope.ids,
        body: event.body,
    }
}

/// The same event with every optional envelope field present.
#[must_use]
pub fn a_full_event_wire(kind: EventKind) -> Value {
    let mut event = an_event_wire(kind);
    event["task_id"] = json!("FRK-1");
    event["agent_id"] = json!("maya-chen");
    event["session_id"] = json!("session-1");
    event
}

/// The body one kind carries, schema-valid and with no optional field.
#[must_use]
pub fn a_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::TaskCreated => {
            json!({ "summary": a_contract_summary_wire(), "created_by": "human" })
        }
        EventKind::RequestTriaged => json!({
            "size": "small",
            "reason": "One deliverable and one reviewer.",
            "triaged_by": "sam-ortiz"
        }),
        EventKind::ContractWritten => {
            json!({ "summary": a_contract_summary_wire(), "written_by": "maya-chen" })
        }
        EventKind::ContractLocked => json!({ "locked_by": "human" }),
        EventKind::ContractUnlocked => json!({ "unlocked_by": "human" }),
        EventKind::DriftDetected => json!({
            "drift": "contract_without_events",
            "detail": "FRK-1 has a contract file and no events."
        }),
        EventKind::ProjectScanned => json!({
            "read_back": "A Rust workspace with one crate and a check command.",
            "detected_criteria": ["cargo xtask check"]
        }),
        EventKind::TeamUpdated => json!({
            "team_name": "Farik",
            "agent_ids": ["maya-chen", "sam-ortiz"],
            "updated_by": "human"
        }),
        EventKind::CriteriaUpdated => json!({
            "criterion_names": ["the check passes"],
            "updated_by": "human"
        }),
        EventKind::CostRecorded => json!({
            "purpose": "implement",
            "model_id": "claude-sonnet-4-5",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 100,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            },
            "cost_usd": 0.5,
            "unpriced": false
        }),
        EventKind::BudgetExhausted => json!({ "scope": "day_usd", "consequence": "pause_team" }),
        EventKind::TaskTransitioned => json!({
            "from": "ready",
            "to": "assigned",
            "actor": "product_manager",
            "requested_by": "maya-chen",
            "gate": "assignment",
            "effects": [],
            "assignee": "dev-a",
            "reviewer": "dev-b",
            "iteration": 0
        }),
        EventKind::TransitionRefused => json!({
            "from": "ready",
            "to": "assigned",
            "actor": "product_manager",
            "requested_by": "maya-chen",
            "refusal": "gate_failed",
            "details": ["the reviewer is the assignee"]
        }),
        EventKind::EscalationRaised => json!({
            "reason": "blocker_age",
            "detail": "blocked_age: no key"
        }),
        EventKind::ContractEvaluated => json!({
            "gate": "definition_of_ready",
            "passed": false,
            "failures": ["the contract has no exit criteria"]
        }),
        EventKind::CriterionRecorded => json!({
            "criterion_id": "C1",
            "passed": true,
            "evidence": "cargo xtask check: ok",
            "run_by": "assignee",
            "recorded_by": "dev-a"
        }),
        EventKind::NoteWritten => json!({
            "kind": "completion",
            "text": "The login page is done and its tests pass.",
            "written_by": "dev-a"
        }),
        EventKind::ReviewRecorded | EventKind::ProductDocWritten => a_record_body_wire(kind),
        EventKind::ToolCalled => a_tool_body_wire("input", "{\"file_path\":\"src/lib.rs\"}"),
        EventKind::ToolDenied => a_tool_body_wire("reason", "tool_not_allowed: Bash has no tier"),
        EventKind::ToolReturned => a_tool_body_wire("output", "{\"type\":\"text\"}"),
        EventKind::SessionStarted | EventKind::SessionEnded => a_session_body_wire(kind),
        EventKind::TaskIntegrated | EventKind::PullRequestOpened => an_integration_body_wire(kind),
        EventKind::QuestionAsked
        | EventKind::QuestionAnswered
        | EventKind::HumanAccepted
        | EventKind::EscalationResolved
        | EventKind::AgentUpdated => a_human_body_wire(kind),
    }
}

/// A review summed up, or a product document written.
fn a_record_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::ReviewRecorded {
        json!({ "reviewer": "dev-b", "criteria_run": 2, "passed": true })
    } else {
        json!({ "path": "prd.md", "written_by": "maya-chen" })
    }
}

/// A body of a question to the human or of the human's own acts: a question, an answer to question
/// 3, an acceptance of a result with its words, a resolution back to `refining`, and a pause of
/// `dev-a`.
fn a_human_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::QuestionAsked => json!({
            "question": "Should a login page remember the user?",
            "asked_by": "maya-chen"
        }),
        EventKind::QuestionAnswered => {
            json!({ "question_id": 3, "answer": "Yes.", "answered_by": "human" })
        }
        EventKind::HumanAccepted => {
            json!({ "subject": "result", "accepted_by": "human", "message": "Both look right." })
        }
        EventKind::EscalationResolved => {
            json!({ "to": "refining", "message": "Split it by page.", "resolved_by": "human" })
        }
        _ => json!({ "agent_id": "dev-a", "status": "paused", "updated_by": "human" }),
    }
}

/// An integration body: FRK-1 merged into `main` by the governor, or its pull request 7 opened.
fn an_integration_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::TaskIntegrated {
        json!({ "sha": "4b825dc642cb6eb9a060e54bf8d69288fbee4904", "into": "main", "integrated_by": "governor" })
    } else {
        json!({ "url": "https://github.com/o/r/pull/7", "number": 7, "branch": "farik/FRK-1" })
    }
}

/// A `session.` body: an implement session on haiku that started, or that completed.
fn a_session_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::SessionStarted {
        json!({ "purpose": "implement", "model": "claude-haiku-4-5-20251001", "effort": "high" })
    } else {
        json!({ "reason": "completed", "detail": "done" })
    }
}

/// A `tool.` body of a `Read`, with its one field of its own.
fn a_tool_body_wire(field: &str, value: &str) -> Value {
    let mut body = json!({ "tool": "Read" });
    body[field] = json!(value);
    body
}

/// A schema-valid contract summary: a task in `draft`, with no parent.
#[must_use]
pub fn a_contract_summary_wire() -> Value {
    json!({ "kind": "task", "title": "Add a login page", "status": "draft", "risk": "low" })
}
