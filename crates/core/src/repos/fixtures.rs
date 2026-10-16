use serde_json::{Value, json};

/// A schema-valid wire list of two member repositories: `app` at `/home/me/app` with no
/// integration branch, and `infra` at `/home/me/infra` with `main`.
#[must_use]
pub fn two_repos_wire() -> Value {
    json!({
        "repos": [
            { "name": "app", "path": "/home/me/app" },
            { "name": "infra", "path": "/home/me/infra", "integration_branch": "main" }
        ]
    })
}
