//! Which services the owner reaches through Claude's connectors: names from settings, tool calls,
//! the catalog. Never a key: from a settings file only a server's name, kind, host and program.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use url::Url;

/// How a configured server is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServerKind {
    /// A program Claude starts.
    Stdio,
    /// A server reached over HTTP.
    Http,
    /// A server reached over server-sent events.
    Sse,
}

impl ServerKind {
    /// The kind's value in `machine.db`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }

    /// The kind a `machine.db` value names.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        match wire {
            "stdio" => Some(Self::Stdio),
            "http" => Some(Self::Http),
            "sse" => Some(Self::Sse),
            _ => None,
        }
    }
}

/// A connector a settings file names: the only four facts kept of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredServer {
    /// The name the settings give it.
    pub name: String,
    /// How it is reached.
    pub kind: ServerKind,
    /// The host of its address, lower case; no user, password, port, path or query.
    pub host: Option<String>,
    /// The file name of the program it starts, when that is plainly a program.
    pub program: Option<String>,
}

fn host_of(address: &str) -> Option<String> {
    Url::parse(address)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

fn program_of(command: &str) -> Option<String> {
    let plain =
        |text: &str| !text.is_empty() && !text.contains(char::is_whitespace) && !text.contains('=');
    let name = command.rsplit(['/', '\\']).next().unwrap_or(command);
    (plain(command) && plain(name)).then(|| name.to_string())
}

fn server_of(name: &str, entry: &Value) -> Option<ConfiguredServer> {
    let command = entry.get("command").and_then(Value::as_str);
    let address = entry.get("url").and_then(Value::as_str);
    let kind = match entry.get("type") {
        None => {
            if command.is_some() {
                ServerKind::Stdio
            } else if address.is_some() {
                ServerKind::Http
            } else {
                return None;
            }
        }
        Some(declared) => ServerKind::from_wire(declared.as_str()?)?,
    };
    Some(ConfiguredServer {
        name: name.to_string(),
        kind,
        host: address.and_then(host_of),
        program: command.and_then(program_of),
    })
}

fn servers_of_map(map: &Value) -> Vec<ConfiguredServer> {
    let Some(entries) = map.as_object() else {
        return Vec::new();
    };
    let mut servers: Vec<ConfiguredServer> = entries
        .iter()
        .filter_map(|(name, entry)| server_of(name, entry))
        .collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    servers
}

/// The servers a JSON object's `mcpServers` names, sorted by name. Only each entry's `type`, `url`'s
/// host and `command`'s program name are read; never `env`, `headers`, `args`, `oauth` or any other
/// field. An entry that is not a server, or text that is not JSON, adds nothing.
#[must_use]
pub fn mcp_servers(json: &str) -> Vec<ConfiguredServer> {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|value| value.get("mcpServers").map(servers_of_map))
        .unwrap_or_default()
}

/// The servers a plugin's `.mcp.json` names: its `mcpServers`, else the file's top-level object
/// taken as the map of servers.
#[must_use]
pub fn plugin_mcp_json_servers(json: &str) -> Vec<ConfiguredServer> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    servers_of_map(value.get("mcpServers").unwrap_or(&value))
}

/// What `~/.claude.json` names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeJson {
    /// The user's servers.
    pub user: Vec<ConfiguredServer>,
    /// Each project's servers, by the project's path; only paths with servers.
    pub projects: BTreeMap<String, Vec<ConfiguredServer>>,
    /// Each project's `.mcp.json` servers the user turned off, by the project's path.
    pub disabled_mcp_json: BTreeMap<String, BTreeSet<String>>,
    /// The claude.ai connectors Claude Code has connected, `claude.ai ` dropped from the front.
    pub claude_ai: Vec<String>,
}

/// Reads `~/.claude.json` for server names only. Text that is not JSON gives nothing.
#[must_use]
pub fn read_claude_json(json: &str) -> ClaudeJson {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return ClaudeJson::default();
    };
    let mut read = ClaudeJson {
        user: value
            .get("mcpServers")
            .map(servers_of_map)
            .unwrap_or_default(),
        ..ClaudeJson::default()
    };
    if let Some(projects) = value.get("projects").and_then(Value::as_object) {
        for (path, project) in projects {
            let servers = project
                .get("mcpServers")
                .map(servers_of_map)
                .unwrap_or_default();
            if !servers.is_empty() {
                read.projects.insert(path.clone(), servers);
            }
            let disabled: BTreeSet<String> = project
                .get("disabledMcpjsonServers")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if !disabled.is_empty() {
                read.disabled_mcp_json.insert(path.clone(), disabled);
            }
        }
    }
    read.claude_ai = value
        .get("claudeAiMcpEverConnected")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| name.strip_prefix("claude.ai ").unwrap_or(name).to_string())
                .collect()
        })
        .unwrap_or_default();
    read
}

/// The enabled plugins with a user-scope install: each plugin's id and the `installPath` of its
/// first user-scope install, by id. Project-scoped installs are not read.
#[must_use]
pub fn user_plugin_installs(
    settings_json: &str,
    installed_plugins_json: &str,
) -> Vec<(String, String)> {
    let (Ok(settings), Ok(installed)) = (
        serde_json::from_str::<Value>(settings_json),
        serde_json::from_str::<Value>(installed_plugins_json),
    ) else {
        return Vec::new();
    };
    let Some(enabled) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    enabled
        .iter()
        .filter(|(_, on)| on.as_bool() == Some(true))
        .filter_map(|(id, _)| {
            let path = installed
                .get("plugins")?
                .get(id)?
                .as_array()?
                .iter()
                .find(|install| install.get("scope").and_then(Value::as_str) == Some("user"))?
                .get("installPath")?
                .as_str()?;
            Some((id.clone(), path.to_string()))
        })
        .collect()
}

/// The name a Claude Desktop extension's `manifest.json` gives it.
#[must_use]
pub fn extension_name(manifest_json: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(manifest_json).ok()?;
    let name = value.get("name")?.as_str()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The server a tool name belongs to: `mcp__<server>__<tool>` gives `<server>`.
#[must_use]
pub fn server_segment(tool_name: &str) -> Option<&str> {
    let (server, _tool) = tool_name.strip_prefix("mcp__")?.split_once("__")?;
    (!server.is_empty()).then_some(server)
}

/// Where a called server was configured, as its name shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerOrigin {
    /// A settings file or Claude Desktop.
    Settings,
    /// A claude.ai connector.
    ClaudeAi,
    /// A plugin.
    Plugin,
}

/// The name and origin of the server a tool call's `<server>` segment names: `claude_ai_<Name>` is
/// `<Name>` with `_` as spaces; `plugin_<plugin>_<server>` is `<server>`; anything else is itself.
#[must_use]
pub fn called_server(segment: &str) -> Option<(String, ServerOrigin)> {
    if segment.is_empty() {
        return None;
    }
    if let Some(name) = segment.strip_prefix("claude_ai_") {
        return (!name.is_empty()).then(|| (name.replace('_', " "), ServerOrigin::ClaudeAi));
    }
    if let Some((_plugin, server)) = segment
        .strip_prefix("plugin_")
        .and_then(|rest| rest.split_once('_'))
        && !server.is_empty()
    {
        return Some((server.to_string(), ServerOrigin::Plugin));
    }
    Some((segment.to_string(), ServerOrigin::Settings))
}

/// Settings files full of made-up keys, for tests.
pub mod fixtures;

#[cfg(test)]
mod tests {
    use super::fixtures::{
        a_claude_json_full_of_keys, a_desktop_config_full_of_keys, a_plugin_mcp_json_full_of_keys,
    };
    use super::*;
    use serde_json::json;

    fn server(
        name: &str,
        kind: ServerKind,
        host: Option<&str>,
        program: Option<&str>,
    ) -> ConfiguredServer {
        ConfiguredServer {
            name: name.to_string(),
            kind,
            host: host.map(str::to_string),
            program: program.map(str::to_string),
        }
    }

    #[test]
    fn reads_a_servers_name_kind_host_and_program() {
        let settings = json!({"mcpServers": {
            "a": {"type": "http", "url": "https://u:KEY-MARKER-1@mcp.zoho.example:8443/mcp?key=KEY-MARKER-2"},
            "b": {"command": "/usr/local/bin/stripe-mcp", "args": ["x"], "env": {"K": "v"}},
            "c": {"type": "sse", "url": "not a url"},
            "d": {"command": "API_KEY=KEY-MARKER-3 npx"},
            "e": {},
            "f": {"url": "https://h.example/x"},
            "g": {"type": "websocket", "url": "wss://w.example"},
        }})
        .to_string();
        assert_eq!(
            mcp_servers(&settings),
            vec![
                server("a", ServerKind::Http, Some("mcp.zoho.example"), None),
                server("b", ServerKind::Stdio, None, Some("stripe-mcp")),
                server("c", ServerKind::Sse, None, None),
                server("d", ServerKind::Stdio, None, None),
                server("f", ServerKind::Http, Some("h.example"), None),
            ]
        );
        assert_eq!(mcp_servers("not json"), vec![]);
        let windows =
            json!({"mcpServers": {"w": {"command": "C:\\tools\\Xero-MCP.exe"}}}).to_string();
        assert_eq!(
            mcp_servers(&windows)[0].program.as_deref(),
            Some("Xero-MCP.exe")
        );
        for wire in ["stdio", "http", "sse"] {
            assert_eq!(
                ServerKind::from_wire(wire).map(ServerKind::as_wire),
                Some(wire)
            );
        }
        assert_eq!(ServerKind::from_wire("ws"), None);
    }

    /// Guard: checked by mutation (keep `env`, keep the address's key), not watched to fail.
    #[test]
    fn keeps_no_key_from_settings() {
        let claude_json = a_claude_json_full_of_keys();
        let read = read_claude_json(&claude_json);
        let desktop = mcp_servers(&a_desktop_config_full_of_keys());
        let plugin = mcp_servers(&a_plugin_mcp_json_full_of_keys());
        let bare = plugin_mcp_json_servers(&a_plugin_mcp_json_full_of_keys());
        let everything = format!("{read:?} {desktop:?} {plugin:?} {bare:?}");
        assert!(!everything.contains("KEY-MARKER"), "{everything}");
        assert!(
            everything.contains("acme") && everything.contains("xero"),
            "{everything}"
        );
    }

    #[test]
    fn reads_claude_json() {
        let read = read_claude_json(&a_claude_json_full_of_keys());
        let names: Vec<&str> = read.user.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["local-db", "stripe"]);
        assert_eq!(read.user[1].host.as_deref(), Some("mcp.stripe.com"));
        assert_eq!(
            read.projects.keys().map(String::as_str).collect::<Vec<_>>(),
            ["/home/me/shop"]
        );
        assert_eq!(read.projects["/home/me/shop"][0].name, "acme");
        assert_eq!(
            read.disabled_mcp_json,
            BTreeMap::from([(
                "/home/me/shop".to_string(),
                BTreeSet::from(["sentry".to_string()])
            )])
        );
        assert_eq!(read.claude_ai, ["Higgsfield", "Zoho Books", "plain"]);
        assert_eq!(read_claude_json("not json"), ClaudeJson::default());
    }

    #[test]
    fn finds_enabled_user_plugins() {
        let settings =
            json!({"enabledPlugins": {"pm@kw": true, "fin@s": false, "proj@x": true}}).to_string();
        let installed = json!({"version": 2, "plugins": {
            "pm@kw": [{"scope": "user", "installPath": "P1"}],
            "fin@s": [{"scope": "user", "installPath": "P2"}],
            "proj@x": [{"scope": "project", "installPath": "P3", "projectPath": "/p"}],
        }})
        .to_string();
        assert_eq!(
            user_plugin_installs(&settings, &installed),
            vec![("pm@kw".to_string(), "P1".to_string())]
        );
        assert_eq!(user_plugin_installs("not json", &installed), vec![]);
    }

    #[test]
    fn reads_an_extensions_name() {
        let manifest = json!({"name": "zoho-books", "display_name": "Zoho", "user_config": {"key": "KEY-MARKER-4"}}).to_string();
        assert_eq!(extension_name(&manifest).as_deref(), Some("zoho-books"));
        assert_eq!(extension_name("{}"), None);
    }

    #[test]
    fn names_the_server_a_tool_called() {
        assert_eq!(
            server_segment("mcp__claude_ai_Zoho_Books__list"),
            Some("claude_ai_Zoho_Books")
        );
        assert_eq!(server_segment("mcp__github__get_me"), Some("github"));
        for none in ["Read", "mcp__x", "mcp____t"] {
            assert_eq!(server_segment(none), None, "{none}");
        }
        let called = |segment| called_server(segment);
        assert_eq!(
            called("claude_ai_Zoho_Books"),
            Some(("Zoho Books".to_string(), ServerOrigin::ClaudeAi))
        );
        assert_eq!(
            called("plugin_product-management_slack"),
            Some(("slack".to_string(), ServerOrigin::Plugin))
        );
        assert_eq!(
            called("github"),
            Some(("github".to_string(), ServerOrigin::Settings))
        );
        assert_eq!(
            called("plugin_x"),
            Some(("plugin_x".to_string(), ServerOrigin::Settings))
        );
        assert_eq!(called("claude_ai_"), None);
        assert_eq!(called(""), None);
    }
}
