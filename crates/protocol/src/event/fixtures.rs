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
    }
}

/// A schema-valid contract summary: a task in `draft`, with no parent.
#[must_use]
pub fn a_contract_summary_wire() -> Value {
    json!({ "kind": "task", "title": "Add a login page", "status": "draft", "risk": "low" })
}
