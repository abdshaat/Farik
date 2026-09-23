use serde_json::{Value, json};

/// A schema-valid wire contract in `draft`, with every required field and no optional ones.
#[must_use]
pub fn a_contract_wire() -> Value {
    json!({
        "id": "FRK-1",
        "title": "Add a login page",
        "intent": "A user can sign in with an email and password so that their work is private.",
        "scope": { "in_scope": ["login form"], "out_of_scope": ["password reset"] },
        "requirements": [{ "id": "R1", "text": "The login form has email and password fields." }],
        "exit_criteria": [{
            "id": "C1",
            "text": "Unit tests for the login form pass.",
            "satisfies": ["R1"],
            "verification": { "method": "test", "command": "pnpm test login" }
        }],
        "assignee_role": "software_developer",
        "reviewer_role": "architect",
        "risk": "low",
        "budget": { "max_cost_usd": 5 },
        "allowed_paths": ["src/login/**"],
        "status": "draft"
    })
}

/// A wire contract with every optional field present and every default written explicitly.
#[must_use]
pub fn a_full_contract_wire() -> Value {
    let mut contract = a_contract_wire();
    contract["requirements"] = json!([{ "id": "R1", "text": "The login form has email and password fields.", "rationale": "Baseline." }]);
    contract["exit_criteria"] = json!([
        { "id": "C1", "text": "The check command exits zero and prints the summary.", "satisfies": ["R1"],
          "verification": { "method": "command", "command": "pnpm check", "expect": { "exit_code": 0, "stdout_contains": "passed", "stdout_not_contains": "failed" } } },
        { "id": "C2", "text": "A new test exists and fails on the base branch.",
          "verification": { "method": "test", "command": "pnpm test login", "new_tests_required": true } },
        { "id": "C3", "text": "The release notes mention the login page.",
          "verification": { "method": "artifact", "path": "CHANGELOG.md", "must_contain": ["login"] } },
        { "id": "C4", "text": "The form follows the design system.",
          "verification": { "method": "review", "rubric": ["Does the form use the shared Button?"] } },
        { "id": "C5", "text": "The founder has tried the login flow.",
          "verification": { "method": "human", "question": "Did you sign in successfully?" } }
    ]);
    contract["kind"] = json!("task");
    contract["parent"] = json!("FRK-3");
    contract["constraints"] = json!(["Use the existing session store."]);
    contract["dependencies"] = json!(["FRK-2"]);
    contract["references"] = json!(["https://github.com/abdshaat/farik/issues/1"]);
    contract["budget"] = json!({ "max_cost_usd": 5.0, "max_sessions": 12, "max_iterations": 3 });
    contract["locked"] = json!(true);
    contract["sprint"] = json!("S1");
    contract["assignee"] = json!("maya-chen");
    contract["reviewer"] = json!("omar-reyes");
    contract["iteration"] = json!(0);
    contract["notes"] =
        json!({ "completion": "Done.", "review": "C1 passed: see output.", "escalation": "None." });
    contract["created_by"] = json!("maya-chen");
    contract["created_at"] = json!("2026-09-14T10:00:00Z");
    contract["updated_at"] = json!("2026-09-14T11:00:00Z");
    contract
}
