use serde_json::{Value, json};

/// A schema-valid wire team: the two agents a team cannot work without, and no optional field
/// but one. It checks no plans (`judgment.required: never`), so that a test not about checking
/// plans keeps testing what it names; a test about them sets `always`.
#[must_use]
pub fn a_team_wire() -> Value {
    json!({
        "name": "Farik",
        "agents": [an_agent_wire("ada", "product_manager"), an_agent_wire("linus", "software_developer")],
        "budgets": { "daily_usd": 20 },
        "policy": {
            "human_accepts_contracts": "high_risk",
            "wip_limit_per_agent": 2,
            "blocked_limit_hours": 24,
            "max_iterations": 3,
            "integration": "manual",
            "judgment": { "required": "never" }
        },
        "rules": {}
    })
}

/// One active agent, with every required field and no optional one.
#[must_use]
pub fn an_agent_wire(id: &str, role: &str) -> Value {
    json!({
        "id": id,
        "display_name": id,
        "role": role,
        "status": "active"
    })
}

/// A wire team with every optional field present, so that a reader of a test can see the whole
/// shape in one place and a change to the schema has one fixture to update.
#[must_use]
pub fn a_full_team_wire() -> Value {
    json!({
        "name": "Farik",
        "agents": [
            {
                "id": "ada",
                "display_name": "Ada",
                "role": "product_manager",
                "persona": "Asks the question nobody asked.",
                "avatar": "ada.png",
                "status": "active",
                "model": { "id": "claude-opus-5-5", "effort": "high" },
                "grants": ["execute"],
                "revokes": ["network"],
                "preauthorized_external_tools": ["mcp__linear__create_issue"],
                "mcp_servers": [{ "name": "playwright", "source": "builtin" }]
            },
            {
                "id": "linus",
                "display_name": "Linus",
                "role": "software_developer",
                "status": "active",
                "model": { "id": "claude-opus-5-5" }
            }
        ],
        "budgets": {
            "daily_usd": 20.5,
            "session": {
                "max_input_tokens": 200_000,
                "max_output_tokens": 64000,
                "max_wall_clock_seconds": 1800,
                "max_tool_calls": 200
            }
        },
        "policy": {
            "human_accepts_contracts": "all",
            "wip_limit_per_agent": 2,
            "blocked_limit_hours": 24,
            "max_iterations": 3,
            "escalation_age_hours": 24,
            "memory_cap_tokens": 8000,
            "integration": "auto_merge",
            "integration_branch": "trunk",
            "judgment": {
                "required": "always",
                "questions": ["Does the task fit its budget?", "Is it small enough to finish in one go?"],
                "judge": "auto"
            },
            "permissions": { "run_commands": true, "push": false }
        },
        "rules": {
            "protected_paths": ["infra/**"],
            "allowed_paths_ceiling": ["src/**"],
            "required_criteria": ["test", "review"],
            "require_new_tests": true,
            "max_task_budget_usd": 12.5,
            "forbidden_commands": ["^rm -rf /"],
            "document_paths": ["docs/**"],
            "ui_paths": ["app/**"]
        },
        "preview": {
            "prepare": "pnpm install --frozen-lockfile",
            "start": "pnpm dev --port 4400",
            "port": 4400,
            "path": "/app"
        }
    })
}
