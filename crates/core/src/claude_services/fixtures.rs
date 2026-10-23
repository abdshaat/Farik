//! Settings files full of made-up keys, for tests in this crate and in others. Every key is a
//! distinct `KEY-MARKER-<n>`; nothing here is a real credential or a real person's setup.

use serde_json::json;

/// A `~/.claude.json`: user servers, a project's servers and disabled `.mcp.json` servers, a
/// project with none, the claude.ai connectors, and a key in every place a server can hold one.
#[must_use]
pub fn a_claude_json_full_of_keys() -> String {
    json!({
        "oauthAccount": {"accessToken": "KEY-MARKER-20"},
        "mcpServers": {
            "stripe": {
                "type": "http", "url": "https://mcp.stripe.com",
                "headers": {"Authorization": "Bearer KEY-MARKER-11"},
            },
            "local-db": {
                "command": "/usr/bin/db-mcp", "args": ["--token", "KEY-MARKER-12"],
                "env": {"DB_PASSWORD": "KEY-MARKER-13"},
            },
        },
        "projects": {
            "/home/me/shop": {
                "mcpServers": {
                    "acme": {
                        "type": "sse",
                        "url": "https://user:KEY-MARKER-14@acme.example/sse?key=KEY-MARKER-15",
                        "oauth": {"clientSecret": "KEY-MARKER-16"},
                    },
                },
                "disabledMcpjsonServers": ["sentry"],
            },
            "/home/me/empty": {"mcpServers": {}, "allowedTools": []},
        },
        "claudeAiMcpEverConnected": ["claude.ai Higgsfield", "claude.ai Zoho Books", "plain", 7],
    })
    .to_string()
}

/// A `claude_desktop_config.json` with one stdio server holding keys in `args` and `env`.
#[must_use]
pub fn a_desktop_config_full_of_keys() -> String {
    json!({
        "mcpServers": {
            "xero": {
                "command": "/opt/xero-mcp",
                "args": ["--secret", "KEY-MARKER-31"],
                "env": {"XERO_CLIENT_SECRET": "KEY-MARKER-32"},
            },
        },
        "preferences": {"apiKey": "KEY-MARKER-33"},
    })
    .to_string()
}

/// A plugin's `.mcp.json` with `mcpServers`, an HTTP server holding keys in its headers and address.
#[must_use]
pub fn a_plugin_mcp_json_full_of_keys() -> String {
    json!({
        "mcpServers": {
            "slack": {
                "type": "http",
                "url": "https://mcp.slack.com/mcp?token=KEY-MARKER-41",
                "headers": {"X-Api-Key": "KEY-MARKER-42"},
            },
        },
    })
    .to_string()
}
