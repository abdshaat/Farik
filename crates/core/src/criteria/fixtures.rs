use serde_json::{Value, json};

/// A schema-valid wire criterion library holding one criterion of every verification method, in
/// the order the schema lists them.
#[must_use]
pub fn a_criteria_library_wire() -> Value {
    json!({
        "criteria": [
            {
                "name": "cargo-check",
                "text": "The repository's own check command passes.",
                "source": "project_scan",
                "verification": {
                    "method": "command",
                    "command": "cargo xtask check",
                    "expect": {
                        "exit_code": 2,
                        "stdout_contains": "xtask check: ok",
                        "stdout_not_contains": "warning"
                    }
                }
            },
            {
                "name": "unit-tests",
                "text": "The unit tests pass, and this change adds one.",
                "source": "project_scan",
                "verification": {
                    "method": "test",
                    "command": "cargo test --workspace",
                    "new_tests_required": true
                }
            },
            {
                "name": "decision-recorded",
                "text": "An architecture decision record says why.",
                "verification": {
                    "method": "artifact",
                    "path": "docs/decisions",
                    "must_contain": ["Status: accepted"]
                }
            },
            {
                "name": "reviewed-for-clarity",
                "text": "A reviewer read it and found it clear.",
                "verification": {
                    "method": "review",
                    "rubric": ["Does every public item say what it is for?"]
                }
            },
            {
                "name": "human-accepted",
                "text": "The human accepted the result.",
                "verification": {
                    "method": "human",
                    "question": "Does this do what you asked for?"
                }
            },
            {
                "name": "tests-pass",
                "text": "The tests pass, and this change need not add one.",
                "verification": {
                    "method": "test",
                    "command": "cargo test --workspace"
                }
            }
        ]
    })
}

/// A library with nothing in it: what a project whose scan found no check command starts with.
#[must_use]
pub fn an_empty_criteria_library_wire() -> Value {
    json!({ "criteria": [] })
}
