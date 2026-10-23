//! Which services the owner reaches through Claude's connectors: names from settings, tool calls,
//! the catalog. Never a key: from a settings file only a server's name, kind, host and program.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde_json::Value;
use url::Url;

pub use catalog::NOT_SERVICES;
use catalog::{CatalogEntry, ServiceKind, is_a_service, match_service, normalise};

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

/// Where a service was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceSource {
    /// Claude Code's settings and plugins.
    ClaudeCode,
    /// The connectors claude.ai has connected, or a call to one.
    ClaudeAi,
    /// Claude Desktop's settings and extensions.
    ClaudeDesktop,
    /// A tool call in a transcript.
    Transcripts,
}

impl ServiceSource {
    /// The source's value in `machine.db` and in `--json`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::ClaudeAi => "claude_ai",
            Self::ClaudeDesktop => "claude_desktop",
            Self::Transcripts => "transcripts",
        }
    }

    /// The source a `machine.db` value names.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        match wire {
            "claude_code" => Some(Self::ClaudeCode),
            "claude_ai" => Some(Self::ClaudeAi),
            "claude_desktop" => Some(Self::ClaudeDesktop),
            "transcripts" => Some(Self::Transcripts),
            _ => None,
        }
    }
}

/// A server a settings file names, and which settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configured {
    /// Where it was configured.
    pub source: ServiceSource,
    /// What was kept of it.
    pub server: ConfiguredServer,
}

/// One service, however many sources name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The catalog id, else `<normalised name>@<host>`.
    pub key: String,
    /// The catalog's name, else the first configured server's, else the earliest call's.
    pub name: String,
    /// The kind of work it is for.
    pub kind: ServiceKind,
    /// The first configured host, in source order.
    pub host: Option<String>,
    /// Where it was seen.
    pub sources: BTreeSet<ServiceSource>,
}

/// One tool call to a connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The tool use's id.
    pub tool_use_id: String,
    /// The raw tool name, `mcp__<server>__<tool>`.
    pub tool_name: String,
    /// When the reply carrying it was written.
    pub at: DateTime<Utc>,
}

/// The tool calls of one session of the owner's, and where it belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionUse {
    /// The session's id.
    pub session_id: String,
    /// The project root it belongs to; `None` when it belongs to no project.
    pub root: Option<String>,
    /// Whether it worked at home or in a temporary folder.
    pub is_home: bool,
    /// Its tool calls to connectors.
    pub calls: Vec<Call>,
}

/// How a service was used: by a project, or by the sessions of a formed project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceUse {
    /// The service's key.
    pub key: String,
    /// The service's name.
    pub name: String,
    /// How many calls.
    pub calls: u64,
    /// When it was last called.
    pub last_used_at: DateTime<Utc>,
}

/// The kind of work a project formed from connector work is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Work {
    /// Books, invoices and spreadsheets.
    Bookkeeping,
    /// Payments.
    Payments,
    /// Selling.
    Selling,
    /// Marketing.
    Marketing,
}

impl Work {
    /// The work's value in `--json`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Bookkeeping => "bookkeeping",
            Self::Payments => "payments",
            Self::Selling => "selling",
            Self::Marketing => "marketing",
        }
    }

    /// The work as a project's name begins.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Bookkeeping => "Bookkeeping",
            Self::Payments => "Payments",
            Self::Selling => "Selling",
            Self::Marketing => "Marketing",
        }
    }

    /// The work a kind of service is: spreadsheets are bookkeeping; code and other are none.
    #[must_use]
    pub fn of(kind: ServiceKind) -> Option<Self> {
        match kind {
            ServiceKind::Bookkeeping | ServiceKind::Spreadsheets => Some(Self::Bookkeeping),
            ServiceKind::Payments => Some(Self::Payments),
            ServiceKind::Selling => Some(Self::Selling),
            ServiceKind::Marketing => Some(Self::Marketing),
            ServiceKind::Code | ServiceKind::Other => None,
        }
    }
}

/// A project formed from connector work done at home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormedProject {
    /// `"<Work> (<service>)"`.
    pub name: String,
    /// The work.
    pub work: Work,
    /// The sessions that form it, in the order given.
    pub session_ids: Vec<String>,
    /// Every service those sessions called, most calls first.
    pub services: Vec<ServiceUse>,
}

/// What the settings and the calls say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicesFound {
    /// Every service, by kind then name.
    pub services: Vec<Service>,
    /// The services each project root's sessions called, most calls first.
    pub by_root: BTreeMap<String, Vec<ServiceUse>>,
    /// Projects formed from connector work at home.
    pub formed: Vec<FormedProject>,
}

/// How many calls of one kind of work make a session's work.
pub const MIN_WORK_CALLS: u64 = 3;

/// The kind of one call to a service: the service's kind, except that Microsoft 365 is not only
/// Excel, so a call to it is spreadsheets only when its tool name says excel, workbook, worksheet or
/// sheet, and otherwise other.
#[must_use]
pub fn call_kind(service: &CatalogEntry, tool_name: &str) -> ServiceKind {
    if service.id != "microsoft_365" {
        return service.kind;
    }
    let lower = tool_name.to_ascii_lowercase();
    if ["excel", "workbook", "worksheet", "sheet"]
        .iter()
        .any(|word| lower.contains(word))
    {
        ServiceKind::Spreadsheets
    } else {
        ServiceKind::Other
    }
}

/// Whether a folder with no git is a folder of bookkeeping: its owner sessions called spreadsheet
/// files at least `MIN_WORK_CALLS` times.
#[must_use]
pub fn is_bookkeeping_folder(is_git: bool, spreadsheet_calls: u64) -> bool {
    !is_git && spreadsheet_calls >= MIN_WORK_CALLS
}

struct Accumulator {
    entry: Option<&'static CatalogEntry>,
    sources: BTreeSet<ServiceSource>,
    host: Option<String>,
    configured_name: Option<String>,
    earliest_call: Option<(DateTime<Utc>, String, String)>,
}

struct SeenCall {
    key: String,
    kind: ServiceKind,
    at: DateTime<Utc>,
}

fn key_of(entry: Option<&CatalogEntry>, name: &str, host: Option<&str>) -> String {
    entry.map_or_else(
        || format!("{}@{}", normalise(name), host.unwrap_or_default()),
        |entry| entry.id.to_string(),
    )
}

fn uses_of<'a>(
    calls: impl Iterator<Item = &'a SeenCall>,
    names: &BTreeMap<String, String>,
) -> Vec<ServiceUse> {
    let mut by_key: BTreeMap<&str, (u64, DateTime<Utc>)> = BTreeMap::new();
    for call in calls {
        let seen = by_key.entry(call.key.as_str()).or_insert((0, call.at));
        seen.0 += 1;
        seen.1 = seen.1.max(call.at);
    }
    let mut uses: Vec<ServiceUse> = by_key
        .into_iter()
        .map(|(key, (calls, last_used_at))| ServiceUse {
            key: key.to_string(),
            name: names.get(key).cloned().unwrap_or_default(),
            calls,
            last_used_at,
        })
        .collect();
    uses.sort_by(|a, b| {
        b.calls
            .cmp(&a.calls)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.key.cmp(&b.key))
    });
    uses
}

#[derive(Default)]
struct Registry {
    accumulators: BTreeMap<String, Accumulator>,
    hosts_by_name: BTreeMap<String, BTreeSet<Option<String>>>,
    entry_by_name: BTreeMap<String, &'static CatalogEntry>,
}

impl Registry {
    fn accumulator(
        &mut self,
        key: String,
        entry: Option<&'static CatalogEntry>,
    ) -> &mut Accumulator {
        self.accumulators.entry(key).or_insert_with(|| Accumulator {
            entry,
            sources: BTreeSet::new(),
            host: None,
            configured_name: None,
            earliest_call: None,
        })
    }

    fn add_configured(&mut self, item: &Configured) {
        let server = &item.server;
        let entry = match_service(
            &server.name,
            server.host.as_deref(),
            server.program.as_deref(),
        );
        let normal = normalise(&server.name);
        self.hosts_by_name
            .entry(normal.clone())
            .or_default()
            .insert(server.host.clone());
        if let Some(entry) = entry {
            self.entry_by_name.entry(normal).or_insert(entry);
        }
        let key = key_of(entry, &server.name, server.host.as_deref());
        let acc = self.accumulator(key, entry);
        acc.sources.insert(item.source);
        if acc.host.is_none() {
            acc.host.clone_from(&server.host);
        }
        if acc.configured_name.is_none() {
            acc.configured_name = Some(server.name.clone());
        }
    }

    /// Records one call and says which service it was to; `None` for what is not a service.
    fn add_call(&mut self, call: &Call) -> Option<SeenCall> {
        let (name, origin) = server_segment(&call.tool_name).and_then(called_server)?;
        if !is_a_service(&name) {
            return None;
        }
        let normal = normalise(&name);
        let host = self
            .hosts_by_name
            .get(&normal)
            .filter(|hosts| hosts.len() == 1)
            .and_then(|hosts| hosts.iter().next().cloned().flatten());
        let entry = self
            .entry_by_name
            .get(&normal)
            .copied()
            .or_else(|| match_service(&name, host.as_deref(), None));
        let key = key_of(entry, &name, host.as_deref());
        let acc = self.accumulator(key.clone(), entry);
        acc.sources.insert(ServiceSource::Transcripts);
        if origin == ServerOrigin::ClaudeAi {
            acc.sources.insert(ServiceSource::ClaudeAi);
        }
        let is_earlier = acc
            .earliest_call
            .as_ref()
            .is_none_or(|(at, id, _)| (call.at, &call.tool_use_id) < (*at, id));
        if is_earlier {
            acc.earliest_call = Some((call.at, call.tool_use_id.clone(), name));
        }
        Some(SeenCall {
            key,
            kind: entry.map_or(ServiceKind::Other, |entry| {
                call_kind(entry, &call.tool_name)
            }),
            at: call.at,
        })
    }

    fn into_services(self) -> Vec<Service> {
        let mut services: Vec<Service> = self
            .accumulators
            .into_iter()
            .map(|(key, acc)| {
                let name = acc
                    .entry
                    .map(|entry| entry.name.to_string())
                    .or(acc.configured_name)
                    .or_else(|| acc.earliest_call.map(|(_, _, name)| name))
                    .unwrap_or_default();
                Service {
                    key,
                    name,
                    kind: acc.entry.map_or(ServiceKind::Other, |entry| entry.kind),
                    host: acc.host,
                    sources: acc.sources,
                }
            })
            .collect();
        services.sort_by(|a, b| (a.kind, &a.name, &a.key).cmp(&(b.kind, &b.name, &b.key)));
        services
    }
}

fn attach_to_roots(
    sessions: &[SessionUse],
    seen_by_session: &[Vec<SeenCall>],
    names: &BTreeMap<String, String>,
) -> BTreeMap<String, Vec<ServiceUse>> {
    let mut roots: BTreeMap<&str, Vec<&SeenCall>> = BTreeMap::new();
    for (session, seen) in sessions.iter().zip(seen_by_session) {
        if let Some(root) = session.root.as_deref() {
            roots.entry(root).or_default().extend(seen);
        }
    }
    roots
        .into_iter()
        .filter(|(_, seen)| !seen.is_empty())
        .map(|(root, seen)| (root.to_string(), uses_of(seen.into_iter(), names)))
        .collect()
}

fn form_projects(
    sessions: &[SessionUse],
    seen_by_session: &[Vec<SeenCall>],
    names: &BTreeMap<String, String>,
) -> Vec<FormedProject> {
    let mut grouped: BTreeMap<Work, Vec<usize>> = BTreeMap::new();
    for (index, (session, seen)) in sessions.iter().zip(seen_by_session).enumerate() {
        if !session.is_home {
            continue;
        }
        let total = seen.len() as u64;
        for work in [
            Work::Bookkeeping,
            Work::Payments,
            Work::Selling,
            Work::Marketing,
        ] {
            let of_work = seen
                .iter()
                .filter(|call| Work::of(call.kind) == Some(work))
                .count() as u64;
            if of_work >= MIN_WORK_CALLS && of_work * 2 > total {
                grouped.entry(work).or_default().push(index);
            }
        }
    }
    grouped
        .into_iter()
        .filter_map(|(work, members)| {
            let mut per_service: BTreeMap<&str, u64> = BTreeMap::new();
            for call in members.iter().flat_map(|&i| &seen_by_session[i]) {
                if Work::of(call.kind) == Some(work) {
                    *per_service.entry(call.key.as_str()).or_default() += 1;
                }
            }
            let main = per_service
                .into_iter()
                .max_by(|a, b| {
                    let (name_a, name_b) = (names.get(a.0), names.get(b.0));
                    a.1.cmp(&b.1).then_with(|| name_b.cmp(&name_a))
                })?
                .0;
            Some(FormedProject {
                name: format!("{} ({})", work.label(), names.get(main)?),
                work,
                session_ids: members
                    .iter()
                    .map(|&i| sessions[i].session_id.clone())
                    .collect(),
                services: uses_of(members.iter().flat_map(|&i| &seen_by_session[i]), names),
            })
        })
        .collect()
}

/// Sorts what the settings name and the sessions called into one record per service, attaches each
/// service to the project roots whose sessions called it, and forms a project from the connector
/// work a session did at home.
///
/// `configured` is in source order and, within a source, by where it was found then by name; the
/// first server of a service names it. Servers and calls to what `is_a_service` refuses count
/// nowhere. A home session is W-work when at least `MIN_WORK_CALLS` of its calls are of W's kinds
/// and those are more than half of all its calls to services; all W-work sessions form one project.
#[must_use]
pub fn find_services(configured: &[Configured], sessions: &[SessionUse]) -> ServicesFound {
    let mut ordered: Vec<&Configured> = configured
        .iter()
        .filter(|c| is_a_service(&c.server.name))
        .collect();
    ordered.sort_by_key(|c| c.source);
    let mut registry = Registry::default();
    for item in ordered {
        registry.add_configured(item);
    }
    let seen_by_session: Vec<Vec<SeenCall>> = sessions
        .iter()
        .map(|session| {
            session
                .calls
                .iter()
                .filter_map(|call| registry.add_call(call))
                .collect()
        })
        .collect();
    let services = registry.into_services();
    let names: BTreeMap<String, String> = services
        .iter()
        .map(|service| (service.key.clone(), service.name.clone()))
        .collect();
    ServicesFound {
        by_root: attach_to_roots(sessions, &seen_by_session, &names),
        formed: form_projects(sessions, &seen_by_session, &names),
        services,
    }
}

/// The catalog that sorts a connector into a kind of service.
pub mod catalog;
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

    use chrono::{DateTime, TimeZone, Utc};

    fn d(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 9, 0, 0).unwrap()
    }

    fn configured(source: ServiceSource, name: &str, host: Option<&str>) -> Configured {
        Configured {
            source,
            server: server(name, ServerKind::Http, host, None),
        }
    }

    /// `n` calls to server `x` at `at`, as `Call`s of tool `mcp__x__do`.
    fn calls(x: &str, n: usize, at: DateTime<Utc>) -> Vec<Call> {
        (0..n)
            .map(|i| Call {
                tool_use_id: format!("{x}-{}-{i}", at.timestamp()),
                tool_name: format!("mcp__{x}__do"),
                at,
            })
            .collect()
    }

    fn session(id: &str, root: Option<&str>, is_home: bool, calls: Vec<Call>) -> SessionUse {
        SessionUse {
            session_id: id.to_string(),
            root: root.map(str::to_string),
            is_home,
            calls,
        }
    }

    fn home(id: &str, calls: Vec<Call>) -> SessionUse {
        session(id, None, true, calls)
    }

    fn keys(found: &ServicesFound) -> Vec<&str> {
        found.services.iter().map(|s| s.key.as_str()).collect()
    }

    #[test]
    fn keeps_one_service_across_sources() {
        let configured = [
            configured(
                ServiceSource::ClaudeCode,
                "zoho-books",
                Some("books.zoho.com"),
            ),
            configured(ServiceSource::ClaudeAi, "Zoho Books", None),
            configured(ServiceSource::ClaudeDesktop, "zoho_books", None),
        ];
        let sessions = [home("h", calls("claude_ai_Zoho_Books", 1, d(1)))];
        let found = find_services(&configured, &sessions);
        assert_eq!(found.services.len(), 1);
        let zoho = &found.services[0];
        assert_eq!(zoho.key, "zoho_books");
        assert_eq!(zoho.name, "Zoho Books");
        assert_eq!(zoho.kind, catalog::ServiceKind::Bookkeeping);
        assert_eq!(zoho.host.as_deref(), Some("books.zoho.com"));
        assert_eq!(
            zoho.sources.iter().copied().collect::<Vec<_>>(),
            [
                ServiceSource::ClaudeCode,
                ServiceSource::ClaudeAi,
                ServiceSource::ClaudeDesktop,
                ServiceSource::Transcripts
            ]
        );
        for wire in ["claude_code", "claude_ai", "claude_desktop", "transcripts"] {
            assert_eq!(
                ServiceSource::from_wire(wire).map(ServiceSource::as_wire),
                Some(wire)
            );
        }
        assert_eq!(ServiceSource::from_wire("x"), None);
    }

    #[test]
    fn keys_an_unknown_service_by_name_and_host() {
        let widget = [configured(
            ServiceSource::ClaudeCode,
            "widget",
            Some("w.example"),
        )];
        let found = find_services(&widget, &[home("h", calls("widget", 1, d(1)))]);
        assert_eq!(keys(&found), ["widget@w.example"]);
        assert_eq!(
            found.services[0]
                .sources
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [ServiceSource::ClaudeCode, ServiceSource::Transcripts]
        );
        let mut both = calls("claude_ai_Higgsfield", 1, d(2));
        both.extend(calls("higgsfield", 1, d(1)));
        let found = find_services(&[], &[home("h", both)]);
        assert_eq!(keys(&found), ["higgsfield@"]);
        assert_eq!(found.services[0].name, "higgsfield");
        assert_eq!(found.services[0].kind, catalog::ServiceKind::Other);
        let names = [
            configured(ServiceSource::ClaudeCode, "widget", Some("w.example")),
            configured(ServiceSource::ClaudeCode, "Widget", Some("w.example")),
        ];
        assert_eq!(find_services(&names, &[]).services[0].name, "widget");
        let acme = [
            configured(ServiceSource::ClaudeCode, "acme", Some("a.example")),
            configured(ServiceSource::ClaudeCode, "acme", Some("b.example")),
        ];
        let found = find_services(&acme, &[home("h", calls("acme", 1, d(1)))]);
        assert_eq!(keys(&found), ["acme@", "acme@a.example", "acme@b.example"]);
    }

    #[test]
    fn counts_no_call_to_what_is_not_a_service() {
        let mut own = Vec::new();
        for x in [
            "catervas",
            "farik",
            "claude-code-remote",
            "claude_ai_Claude_Docs",
        ] {
            own.extend(calls(x, 3, d(1)));
        }
        let found = find_services(
            &[configured(ServiceSource::ClaudeCode, "catervas", None)],
            &[session("s", Some("/r"), false, own.clone()), home("h", own)],
        );
        assert!(found.services.is_empty());
        assert!(found.by_root.is_empty());
        assert!(found.formed.is_empty());
    }

    #[test]
    fn attaches_services_to_projects_by_their_calls() {
        let mut first = calls("github", 1, d(1));
        first.extend(calls("github", 1, d(2)));
        first.extend(calls("stripe", 1, d(1)));
        let sessions = [
            session("s1", Some("/home/me/shop"), false, first),
            session("s2", Some("/home/me/shop"), false, calls("github", 1, d(1))),
            session("s3", None, false, calls("xero", 1, d(1))),
        ];
        let configured = [configured(ServiceSource::ClaudeCode, "sentry", None)];
        let found = find_services(&configured, &sessions);
        let uses: Vec<(&str, u64, DateTime<Utc>)> = found.by_root["/home/me/shop"]
            .iter()
            .map(|u| (u.name.as_str(), u.calls, u.last_used_at))
            .collect();
        assert_eq!(uses, [("GitHub", 3, d(2)), ("Stripe", 1, d(1))]);
        assert_eq!(
            found.by_root.len(),
            1,
            "a configured server and a gone session attach nowhere"
        );
        assert_eq!(keys(&found), ["xero", "stripe", "github", "sentry"]);
    }

    fn formed(found: &ServicesFound) -> Vec<(&str, Vec<&str>)> {
        found
            .formed
            .iter()
            .map(|f| {
                (
                    f.name.as_str(),
                    f.session_ids.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn forms_a_project_from_connector_work_at_home() {
        let mut h2 = calls("claude_ai_Zoho_Books", 2, d(2));
        h2.extend(calls("xero", 2, d(2)));
        let mut h3 = calls("stripe", 3, d(3));
        h3.extend(calls("claude_ai_Zoho_Books", 1, d(3)));
        let sessions = [
            home("h1", calls("claude_ai_Zoho_Books", 3, d(1))),
            home("h2", h2),
            home("h3", h3),
            home("h4", calls("claude_ai_Zoho_Books", 2, d(4))),
            home("h5", calls("github", 5, d(5))),
        ];
        let found = find_services(&[], &sessions);
        assert_eq!(
            formed(&found),
            [
                ("Bookkeeping (Zoho Books)", vec!["h1", "h2"]),
                ("Payments (Stripe)", vec!["h3"])
            ]
        );
        assert_eq!(found.formed[0].work, Work::Bookkeeping);
        let uses: Vec<(&str, u64)> = found.formed[0]
            .services
            .iter()
            .map(|u| (u.name.as_str(), u.calls))
            .collect();
        assert_eq!(uses, [("Zoho Books", 5), ("Xero", 2)]);
        assert_eq!(Work::Payments.as_wire(), "payments");
        assert_eq!(Work::Selling.label(), "Selling");
    }

    #[test]
    fn needs_more_than_half_of_a_sessions_calls() {
        let mixed = |github| {
            let mut calls = calls("claude_ai_Zoho_Books", 3, d(1));
            calls.extend(super::tests::calls("github", github, d(1)));
            find_services(&[], &[home("h", calls)])
        };
        assert!(mixed(3).formed.is_empty());
        assert_eq!(formed(&mixed(2)).len(), 1);
        let under_a_root = session(
            "s",
            Some("/r"),
            false,
            calls("claude_ai_Zoho_Books", 5, d(1)),
        );
        assert!(find_services(&[], &[under_a_root]).formed.is_empty());
    }

    #[test]
    fn counts_microsoft_365_as_spreadsheets_only_for_excel_tools() {
        use catalog::{ServiceKind, match_service};
        let m365 = match_service("microsoft365", None, None).expect("in the catalog");
        for tool in [
            "excel_read_range",
            "get_Workbook",
            "list_worksheets",
            "update_sheet",
        ] {
            let name = format!("mcp__microsoft365__{tool}");
            assert_eq!(call_kind(m365, &name), ServiceKind::Spreadsheets, "{name}");
        }
        for tool in ["send_mail", "list_teams"] {
            let name = format!("mcp__microsoft365__{tool}");
            assert_eq!(call_kind(m365, &name), ServiceKind::Other, "{name}");
        }
        let xero = match_service("xero", None, None).expect("in the catalog");
        assert_eq!(call_kind(xero, "mcp__xero__x"), ServiceKind::Bookkeeping);
        let call = |tool: &str, at: u32| Call {
            tool_use_id: format!("{tool}-{at}"),
            tool_name: format!("mcp__microsoft365__{tool}"),
            at: d(1),
        };
        let excel: Vec<Call> = (0..3).map(|i| call("excel_read_range", i)).collect();
        let found = find_services(&[], &[home("h", excel)]);
        assert_eq!(
            formed(&found),
            [("Bookkeeping (Microsoft 365 and Excel)", vec!["h"])]
        );
        let mail: Vec<Call> = (0..3).map(|i| call("send_mail", i)).collect();
        let found = find_services(&[], &[home("h", mail)]);
        assert!(found.formed.is_empty());
        assert_eq!(found.services[0].key, "microsoft_365");
        assert_eq!(found.services[0].kind, catalog::ServiceKind::Spreadsheets);
    }

    #[test]
    fn tells_a_bookkeeping_folder() {
        assert!(is_bookkeeping_folder(false, 3));
        assert!(!is_bookkeeping_folder(false, 2));
        assert!(!is_bookkeeping_folder(true, 10));
    }
}
