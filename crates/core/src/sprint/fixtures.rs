use serde_json::{Value, json};

/// A schema-valid wire sprint: open, no end, no budget, no tasks yet.
#[must_use]
pub fn an_open_sprint_wire() -> Value {
    json!({
        "id": "S1",
        "started_at": "2026-09-24T00:00:00Z",
        "task_ids": [],
        "status": "open"
    })
}
