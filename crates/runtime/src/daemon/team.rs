//! The team's setup and settings for the browser (`docs/SPEC.md` 4.1, 4.4, 10): the suggested six,
//! a change checked and described before it is saved, the setup form's start, the checks, the
//! models, the AI account's disconnect and its connecting again, and the project read back with
//! the user's note on it.
//! `web.rs` answers the frames; this module answers what they ask.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::Path;
use std::sync::Arc;

use farik_core::contract::Role;
use farik_core::criteria::validate_criteria;
use farik_core::governor::gates::DesignerBrowser;
use farik_core::governor::paths::{PathRefusal, check_protected_paths};
use farik_core::team::{
    Agent, AgentStatus, CustomServer, CustomTransport, MODEL_FAMILIES, McpServerSource,
    McpServerWire, Team, ValidationError, custom_server, describe_change, spec_sha256,
    validate_team,
};
use farik_protocol::command::{Command, CommandReply};
use farik_protocol::event::{EventBody, new_event};
use farik_protocol::generated::event::{CriteriaUpdatedBody, TeamUpdatedBody};
use farik_roles::{Kit, KitConnector, load_role};
use farik_store::{EventQuery, names_of, scan_project};
use serde_json::{Value, json};

use super::signed_in::Binding;
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, REFUSED};
use super::{DaemonState, Kept};
use crate::allowances::{asked_allowances, checked_allowances};
use crate::claude::{CredentialKind, Secret, credential_variable};
use crate::connectors::{ConnectorEntry, ConnectorError, ListedTool, SecretAt, list_tools};
use crate::credential::{CredentialError, credential_of_kind, load_credential, save_credential};
use crate::pause::{key_refused, paused};
use crate::session::session_model;
use crate::sprints::sprint_work;
use crate::tools::ToolDeps;

/// The methods this module answers.
pub(super) const METHODS: [&str; 13] = [
    "team.save",
    "agent.replace",
    "team.start",
    "criteria.save",
    "account.disconnect",
    "project.note",
    "connector.tools",
    "connector.connect",
    "connector.allowances",
    "connector.disconnect",
    "connector.sign_in",
    "connector.sign_in_status",
    "connector.sign_in_cancel",
];

/// The marker the setup host leaves in a project it just made one, which "Start the team" removes.
pub const SETUP_PENDING: &str = ".farik/local/setup-pending";

/// How deep `project.scan` looks for private files, and how many entries it looks at at most.
const WALK_DEPTH: u32 = 4;
const WALK_CAP: usize = 2000;

/// Why a save does not change an agent's status, and why it does not remove one.
const FROM_THE_CARD: &str = "pause, retire or resume an agent from its card";
const WORKED: &str = " has done work; retire it instead";

/// Who the human is in the log.
const HUMAN: &str = "human";

/// The six the team builder suggests (spec 4.1): name, role, and shipped avatar. The id is the
/// name's slug.
const SIX: [(&str, Role, &str); 6] = [
    ("Mira", Role::ProductManager, "product-manager"),
    ("Sol", Role::ScrumMaster, "scrum-master"),
    ("Ada", Role::Architect, "architect"),
    ("Theo", Role::SoftwareDeveloper, "developer"),
    ("Iris", Role::UiUxDesigner, "extra-1"),
    ("Kai", Role::MarketingSpecialist, "marketing-specialist"),
];

pub(super) fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The queries of this module, whose params the schema already passed.
pub(super) fn query(
    state: &DaemonState,
    deps: &ToolDeps,
    name: &str,
    params: &Value,
) -> Result<Value, Failure> {
    match name {
        "team.get" => {
            let team = deps.files.read_team().map_err(|e| internal(&e))?;
            let mut answer = effective(deps, &team)?;
            answer["connectors"] = json!(connector_states(state, deps, &team));
            answer["kits"] = json!(kits_of(deps, &team, state.registered_apps())?);
            answer["team"] = serde_json::to_value(team).map_err(|e| internal(&e))?;
            answer["max_agents"] = json!(farik_core::team::MAX_AGENTS);
            answer["sandboxed"] = json!(deps.transitions.sandboxed());
            Ok(answer)
        }
        "team.propose" => propose(deps),
        "team.validate" => {
            let setup = deps.files.root().join(SETUP_PENDING).exists();
            match checked(deps, &params["team"], setup) {
                Ok((before, after)) => {
                    let mut answer = effective(deps, &after)?;
                    answer["errors"] = json!([]);
                    let board = deps.projections.board().map_err(|e| internal(&e))?;
                    let open = deps.projections.open_sprint().map_err(|e| internal(&e))?;
                    let open = open.as_ref().map(|open| open.sprint_id.as_str());
                    let work = sprint_work(&before, open, &board);
                    answer["effects"] = json!(describe_change(&before, &after, &work));
                    Ok(answer)
                }
                Err(Refused::Errors(errors)) => {
                    Ok(json!({ "errors": errors_wire(&errors), "effects": [] }))
                }
                Err(Refused::Failed(failed)) => Err(failed),
            }
        }
        "models.list" => models(deps),
        "settings.defaults" => {
            let defaults = farik_core::team::defaults();
            Ok(json!({
                "budgets": serde_json::to_value(defaults.budgets).map_err(|e| internal(&e))?,
                "policy": serde_json::to_value(defaults.policy).map_err(|e| internal(&e))?,
                "rules": {},
                "ui_paths": farik_core::governor::team_rules::DEFAULT_UI_PATHS,
            }))
        }
        "project.scan" => scanned(deps),
        "skills.list" => skills_list(deps, params),
        "skill.get" => skill_get(deps, params),
        _ => Err(Failure::new(
            super::web::UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// `skills.list { agent }`: the rows of `skill_rows` for one agent (ADR 0034).
fn skills_list(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let agent = params["agent"].as_str().unwrap_or_default();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let Some(held) = team.agents.iter().find(|held| held.id.as_str() == agent) else {
        return Err(Failure::new(
            super::web::NOT_FOUND,
            format!("there is no agent {agent}"),
        ));
    };
    let kit = (deps.kits)(Role::from(held.role)).map_err(|e| internal(&e))?;
    let events = deps
        .log
        .read(&farik_store::EventQuery {
            kinds: vec![
                farik_protocol::event::EventKind::SkillAdded,
                farik_protocol::event::EventKind::SkillChanged,
                farik_protocol::event::EventKind::SkillRemoved,
                farik_protocol::event::EventKind::SkillConfirmed,
            ],
            ..farik_store::EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    let rows = crate::skills::skill_rows(
        deps.files.root(),
        &team,
        Some(agent),
        &crate::skills::confirmed_skills(&events),
        &kit.skills,
    );
    Ok(json!({ "skills": rows }))
}

/// `skill.get { level, agent?, name }`: every file of the skill as its folder holds it, its hash,
/// and the frontmatter keys Farik ignores; the refusal's code when the folder is one the checks
/// refuse.
fn skill_get(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let name = params["name"].as_str().unwrap_or_default();
    if params["level"] == "role" {
        return role_skill_get(deps, params, name);
    }
    let level = match (params["level"].as_str(), params["agent"].as_str()) {
        (Some("team"), None) => crate::skills::SkillLevel::Team,
        (Some("agent"), Some(agent)) => crate::skills::SkillLevel::Agent(agent.to_string()),
        _ => {
            return Err(Failure::new(
                REFUSED,
                "an agent's skill names its agent, and the team's names none",
            ));
        }
    };
    if !farik_roles::skill_name_ok(name) {
        return Err(Failure::new(
            REFUSED,
            "skill_name_invalid: a skill's name is lower-case words joined by hyphens, up to 64 characters",
        ));
    }
    if let crate::skills::SkillLevel::Agent(id) = &level {
        let team = deps.files.read_team().map_err(|e| internal(&e))?;
        if !team.agents.iter().any(|agent| agent.id.as_str() == id) {
            return Err(Failure::new(
                super::web::NOT_FOUND,
                format!("there is no agent {id}"),
            ));
        }
    }
    let folder = crate::skills::skill_folder_unlinked(deps.files.root(), &level, name)
        .map_err(|refusal| Failure::new(REFUSED, refusal.to_string()))?;
    if std::fs::symlink_metadata(&folder).is_err() {
        return Err(Failure::new(
            super::web::NOT_FOUND,
            format!("there is no skill {name} there"),
        ));
    }
    let refused = |refusal: farik_roles::SkillRefusal| Failure::new(REFUSED, refusal.to_string());
    let files = crate::skills::read_skill_folder(&folder).map_err(refused)?;
    let checked = farik_roles::check_skill(name, &files).map_err(refused)?;
    // `check_skill` refused any file that is not UTF-8, so what is shown is what is hashed.
    let mut texts = BTreeMap::new();
    for (path, bytes) in &files {
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| refused(farik_roles::SkillRefusal::FileNotText(path.clone())))?;
        texts.insert(path.clone(), text);
    }
    Ok(json!({
        "files": texts,
        "sha256": farik_core::skill::skill_sha256(&files),
        "ignored_fields": checked.ignored_fields,
    }))
}

/// `skill.get { level: "role", role, name }`: the `SKILL.md` Farik ships for `role`, with no hash,
/// since it is trusted and pinned in the binary. `role` and `name` are checked before any lookup,
/// and nothing here builds a path.
fn role_skill_get(deps: &ToolDeps, params: &Value, name: &str) -> Result<Value, Failure> {
    if !farik_roles::skill_name_ok(name) {
        return Err(Failure::new(
            REFUSED,
            "skill_name_invalid: a skill's name is lower-case words joined by hyphens, up to 64 characters",
        ));
    }
    let asked = serde_json::from_value::<farik_core::contract::Role>(params["role"].clone()).ok();
    let role = asked.and_then(|role| farik_roles::load_role(role).ok());
    let (Some(role), Some(asked)) = (role, asked) else {
        return Err(Failure::new(
            REFUSED,
            "a role's skill names a role Farik ships",
        ));
    };
    if let Some(skill) = role.skills.iter().find(|skill| skill.name == name) {
        return Ok(json!({ "files": { "SKILL.md": skill.text }, "ignored_fields": [] }));
    }
    // Then the role's kit's, as the session's copy holds it: its frontmatter is name and
    // description alone.
    let kit = (deps.kits)(asked).map_err(|error| internal(&error))?;
    if let Some(text) = kit
        .skills
        .iter()
        .find(|skill| skill.name == name)
        .and_then(|skill| skill.session_files.get("SKILL.md"))
    {
        return Ok(json!({ "files": { "SKILL.md": text }, "ignored_fields": [] }));
    }
    Err(Failure::new(
        super::web::NOT_FOUND,
        format!("the role ships no skill {name}"),
    ))
}

/// `account.status` on a daemon with a project: the credential read afresh from the environment
/// and the stores `farik serve` was given.
pub(super) fn account_status(state: &DaemonState) -> Result<Value, Failure> {
    let web = web_of(state)?;
    let mut status = match load_credential(&web.env, &web.stores) {
        Some((credential, source)) => {
            let mut status =
                json!({ "provider": "anthropic", "kind": credential.kind(), "source": source });
            if let Some(variable) = credential_variable(&web.env) {
                status["environment_variable"] = json!(variable);
            }
            status
        }
        None => json!({ "provider": null, "kind": null, "source": null }),
    };
    if let Some(deps) = state.deps()
        && key_refused(&deps.log).map_err(|e| internal(&e))?
    {
        status["key_refused"] = json!(true);
    }
    Ok(status)
}

/// `account.connect` on a daemon with a project: the credential kept, as setup keeps it, and put
/// in place for the next session; a team paused because the provider refused the old key is
/// resumed (5.5), and one the human paused stays paused. A credential from the environment comes
/// before any kept one, so connecting over it is refused, naming its variable.
pub(super) async fn connect(state: &DaemonState, params: &Value) -> Result<Value, Failure> {
    let web = web_of(state)?;
    if let Some(variable) = credential_variable(&web.env) {
        return Err(Failure::new(
            REFUSED,
            format!(
                "your AI account's key comes from {variable}, which Farik cannot change: change \
                 it where Farik runs, then start Farik again"
            ),
        ));
    }
    let kind = serde_json::from_value::<CredentialKind>(params["kind"].clone())
        .map_err(|e| internal(&e))?;
    let credential = credential_of_kind(kind, params["secret"].as_str().unwrap_or_default())
        .map_err(|why| Failure::new(REFUSED, why))?;
    let (stores, kept) = (web.stores.clone(), credential.clone());
    let source = off_the_worker(move || {
        save_credential(&kept, &stores).map_err(|error| Failure::new(REFUSED, words(&error)))
    })
    .await?;
    if let Some(in_use) = &web.in_use {
        *crate::locked(in_use) = credential;
    }
    if let Some(deps) = state.deps()
        && key_refused(&deps.log).map_err(|e| internal(&e))?
        && let Err(refused) = handled(state, Command::TeamResume).await
        // The human's own Resume can land first: the key is kept and the team runs, so the
        // connect stands.
        && paused(&deps.log).map_err(|e| internal(&e))?
    {
        return Err(refused);
    }
    Ok(json!({ "stored_in": source, "taking_on": false }))
}

/// Where `agent`'s keys for `server` are kept in this project (ADR 0030).
///
/// # Errors
///
/// The project's id on this machine could not be read or made.
pub(crate) fn secret_at(
    state: &DaemonState,
    deps: &ToolDeps,
    agent: &str,
    server: &str,
) -> std::io::Result<SecretAt> {
    state.secret_at(deps.files.root(), agent, server)
}

/// Each agent's custom servers in `team` and whether each runs: `connected` when the definition
/// kept beside its keys is the team file's, `store_unavailable` when the store could not be read
/// the last time it was, and `connect_again` otherwise.
pub(super) fn connector_states(state: &DaemonState, deps: &ToolDeps, team: &Team) -> Vec<Value> {
    team.agents
        .iter()
        .flat_map(|agent| {
            agent
                .mcp_servers
                .iter()
                .flatten()
                .filter_map(custom_server)
                .map(move |server| (agent.id.as_str(), server))
        })
        .map(|(agent, server)| {
            let kept = secret_at(state, deps, agent, &server.name)
                .map_or(Kept::Unavailable, |at| state.kept(&at));
            let signs_in = server.oauth().is_some();
            let ended = matches!(&kept, Kept::Entry { spec_sha256, signed_in: Some(grant), .. }
                if grant.lapsed && *spec_sha256 == farik_core::team::spec_sha256(&server));
            // A kit's service the kit no longer says is as it is stays unconnected until the user
            // connects it again, and one the kit no longer has cannot be (ADR 0036).
            let kit_says = if server.kit {
                let role = team
                    .agents
                    .iter()
                    .find(|held| held.id.as_str() == agent)
                    .map(|held| Role::from(held.role));
                role.and_then(|role| (deps.kits)(role).ok()).map(|kit| {
                    if matches_kit(&kit, &server) {
                        "same"
                    } else if kit.connectors.iter().any(|connector| {
                        matches!(connector, KitConnector::Server { .. })
                            && connector.name() == server.name
                    }) {
                        "changed"
                    } else {
                        "gone"
                    }
                })
            } else {
                None
            };
            let shown = match kept {
                _ if kit_says == Some("gone") => "not_in_kit",
                _ if kit_says == Some("changed") => "connect_again",
                ref kept if kept.runs(&server) => "connected",
                Kept::Unavailable => "store_unavailable",
                _ if ended => "sign_in_again",
                _ => "connect_again",
            };
            let mut row = json!({
                "agent": agent, "server": server.name, "state": shown,
                "auth": if signs_in { "oauth" } else { "keys" },
                "source": if server.kit { "kit" } else { "custom" },
            });
            if let Kept::Entry {
                stored_in,
                signed_in,
                ..
            } = kept
            {
                row["stored_in"] = json!(stored_in);
                if let (true, Some(grant)) = (signs_in, signed_in) {
                    row["revokes"] = json!(grant.revokes);
                    if let Some(provider) = grant.provider {
                        row["provider"] = json!(provider);
                    }
                    if let Some(settings_url) = grant.settings_url {
                        row["settings_url"] = json!(settings_url);
                    }
                }
            }
            row
        })
        .collect()
}

/// `team` with `agent`'s entry named `name` replaced by `entry`, or added, or with `entry` `None`
/// removed, held to the team's rules. An agent the team lacks is an error at `/agents`.
///
/// # Errors
///
/// The team's errors, each at its field.
pub(crate) fn with_server(
    team: &Team,
    agent: &str,
    name: &str,
    entry: Option<&Value>,
) -> Result<Team, Vec<ValidationError>> {
    let at = team
        .agents
        .iter()
        .position(|held| held.id.as_str() == agent)
        .ok_or_else(|| {
            vec![ValidationError {
                path: "/agents".to_string(),
                message: format!("there is no agent {agent}"),
            }]
        })?;
    let mut wire = serde_json::to_value(team).map_err(|error| {
        vec![ValidationError {
            path: String::new(),
            message: error.to_string(),
        }]
    })?;
    let held = &mut wire["agents"][at];
    let mut servers: Vec<Value> = held["mcp_servers"].as_array().cloned().unwrap_or_default();
    match (
        servers.iter().position(|server| server["name"] == name),
        entry,
    ) {
        (Some(place), Some(entry)) => servers[place] = entry.clone(),
        (None, Some(entry)) => servers.push(entry.clone()),
        (Some(place), None) => {
            servers.remove(place);
        }
        (None, None) => {}
    }
    if let Some(fields) = held.as_object_mut() {
        if servers.is_empty() {
            fields.remove("mcp_servers");
        } else {
            fields.insert("mcp_servers".to_string(), Value::Array(servers));
        }
    }
    validate_team(&wire)
}

/// `wire`, a custom server as the user describes it, as `agent`'s entry with `tools`, held to the
/// team's rules: the entry and the server it describes.
fn described(
    deps: &ToolDeps,
    agent: &str,
    wire: &Value,
    tools: Value,
) -> Result<(Value, CustomServer), Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    if wire["source"] == "kit" {
        // A kit's service is named, and the kit says the rest, tags included.
        let role = team
            .agents
            .iter()
            .find(|held| held.id.as_str() == agent)
            .map(|held| Role::from(held.role))
            .ok_or_else(|| {
                Failure::from(Refused::Errors(vec![ValidationError {
                    path: "/agents".to_string(),
                    message: format!("there is no agent {agent}"),
                }]))
            })?;
        let kit = (deps.kits)(role).map_err(|error| internal(&error))?;
        let name = wire["name"].as_str().unwrap_or_default();
        let asked =
            asked_allowances(&wire["allowances"]).map_err(|why| Failure::new(REFUSED, why))?;
        return kit_entry(&kit, &team, agent, name, &asked)
            .map_err(|errors| Failure::from(Refused::Errors(errors)));
    }
    custom_entry(&team, agent, wire, tools).map_err(|errors| Failure::from(Refused::Errors(errors)))
}

/// `wire`, a custom server as the user describes it, as `agent`'s entry in `team` with `tools`,
/// held to the team's rules: the entry and the server it describes. What `connector.connect` and
/// `farik connect` both build (ADR 0030).
///
/// # Errors
///
/// The team's errors, each at its field.
pub fn custom_entry(
    team: &Team,
    agent: &str,
    wire: &Value,
    tools: Value,
) -> Result<(Value, CustomServer), Vec<ValidationError>> {
    let mut entry = wire.clone();
    entry["source"] = json!("custom");
    entry["tools"] = tools;
    entry_in(team, agent, entry)
}

/// `entry` given to `agent` in `team`, held to the team's rules: the entry and the server it
/// describes.
fn entry_in(
    team: &Team,
    agent: &str,
    entry: Value,
) -> Result<(Value, CustomServer), Vec<ValidationError>> {
    let name = entry["name"].as_str().unwrap_or_default().to_string();
    let after = with_server(team, agent, &name, Some(&entry))?;
    let server = after
        .agents
        .iter()
        .filter(|held| held.id.as_str() == agent)
        .flat_map(|held| held.mcp_servers.iter().flatten())
        .find(|server| server.name.as_str() == name)
        .and_then(custom_server)
        .ok_or_else(|| {
            vec![ValidationError {
                path: "/agents".to_string(),
                message: format!("{name} is not a custom server once validated"),
            }]
        })?;
    Ok((entry, server))
}

/// The service `name` of `kit` as `agent`'s entry in `team`: `source: kit`, every field from the
/// kit and the kit's tags as its tools, held to the team's rules (ADR 0036). What
/// `connector.connect` and `farik connect` both build for a kit's service.
///
/// # Errors
///
/// `connector_not_in_kit` at `/agents/<i>/mcp_servers` when `kit` is not the agent's role's, lacks
/// `name`, or holds a `container` connector of that name, which Farik runs itself; an agent the
/// team lacks at `/agents`; `allowance_not_offered` or `allowance_out_of_range` at
/// `/agents/<i>/mcp_servers` for an `asked` allowance (ADR 0037); else the team's errors, each at
/// its field. The entry's allowances are the kit's defaults overlaid by `asked`.
pub fn kit_entry(
    kit: &Kit,
    team: &Team,
    agent: &str,
    name: &str,
    asked: &BTreeMap<String, u32>,
) -> Result<(Value, CustomServer), Vec<ValidationError>> {
    let at = team
        .agents
        .iter()
        .position(|held| held.id.as_str() == agent)
        .ok_or_else(|| {
            vec![ValidationError {
                path: "/agents".to_string(),
                message: format!("there is no agent {agent}"),
            }]
        })?;
    let not_in_kit = |why: String| {
        vec![ValidationError {
            path: format!("/agents/{at}/mcp_servers"),
            message: format!("connector_not_in_kit: {why}"),
        }]
    };
    if Role::from(team.agents[at].role) != kit.role {
        return Err(not_in_kit(format!(
            "{name} belongs to the kit of the {}, and {agent} is another role",
            kit.role
        )));
    }
    let Some(KitConnector::Server { entry, .. }) = kit
        .connectors
        .iter()
        .find(|connector| connector.name() == name)
    else {
        return Err(not_in_kit(format!(
            "the {}'s kit has no service {name} to connect",
            kit.role
        )));
    };
    let allowances = checked_allowances(kit, name, asked).map_err(|why| {
        vec![ValidationError {
            path: format!("/agents/{at}/mcp_servers"),
            message: why,
        }]
    })?;
    let mut wire = entry.clone();
    wire.source = McpServerSource::Kit;
    let mut value = serde_json::to_value(&wire).map_err(|error| {
        vec![ValidationError {
            path: String::new(),
            message: error.to_string(),
        }]
    })?;
    if !allowances.is_empty() {
        value["allowances"] = json!(allowances);
    }
    entry_in(team, agent, value)
}

/// The kit's own server of `entry`, as the team file holds it once connected.
fn kit_server(entry: &McpServerWire) -> Option<CustomServer> {
    let mut wire = entry.clone();
    wire.source = McpServerSource::Kit;
    custom_server(&wire)
}

/// Whether `server` is exactly what `kit` says its service of that name is: every field and every
/// tag, its allowances apart: those are the user's, and each must be for a tool the kit offers
/// one for. A kit entry that is not (a team file or a clone widened a tag, or the kit changed with
/// a release) is not the kit's, and runs nothing (ADR 0036, ADR 0037).
#[must_use]
pub fn matches_kit(kit: &Kit, server: &CustomServer) -> bool {
    let mut bare = server.clone();
    bare.allowances.clear();
    server.kit
        && kit.connectors.iter().any(|connector| {
            matches!(connector, KitConnector::Server { entry, allowances, .. }
                if entry.name.as_str() == server.name
                    && kit_server(entry).as_ref() == Some(&bare)
                    && server.allowances.keys().all(|tool| allowances.contains_key(tool)))
        })
}

/// The tools the owner's marketing plan approves for `server` (ADR 0042): the kit's `plan_approved`
/// of the entry of its name, and only when `server` is exactly what the kit says (`matches_kit`),
/// so a custom entry naming the same command, or a kit entry that was widened, gets none.
#[must_use]
pub fn plan_tools_of(kit: &Kit, server: &CustomServer) -> std::collections::BTreeSet<String> {
    if !matches_kit(kit, server) {
        return std::collections::BTreeSet::new();
    }
    kit.connectors
        .iter()
        .find_map(|connector| match connector {
            KitConnector::Server {
                entry,
                plan_approved,
                ..
            } if entry.name.as_str() == server.name => Some(plan_approved.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether `entry` is a service that signs in through one of Farik's own connectors while the
/// daemon's table has no app of Farik's for it (ADR 0044): until Farik Cloud signs customers in
/// at the web launch, nothing here can connect it, and the page says so in place of Connect.
fn at_launch(entry: &McpServerWire, apps: &[crate::registered_apps::RegisteredApp]) -> bool {
    let Some(server) = kit_server(entry) else {
        return false;
    };
    server.oauth().is_some()
        && matches!(
            &server.transport,
            CustomTransport::Stdio { command, args, .. }
                if farik_roles::is_farik_connector(command, args)
                    && crate::registered_apps::app_for_farik_connector(apps, command, args)
                        .is_none()
        )
}

/// What `team.get` says of each role on the team whose kit has a service to connect by name: the
/// copy the page shows and how the service is reached. A `container` connector is no service to
/// connect, and a role with none is not listed.
fn kits_of(
    deps: &ToolDeps,
    team: &Team,
    apps: &[crate::registered_apps::RegisteredApp],
) -> Result<Vec<Value>, Failure> {
    let mut roles: Vec<Role> = Vec::new();
    for agent in team
        .agents
        .iter()
        .filter(|agent| agent.status != AgentStatus::Retired)
    {
        let role = Role::from(agent.role);
        if !roles.contains(&role) {
            roles.push(role);
        }
    }
    let mut kits = Vec::new();
    for role in roles {
        let kit = (deps.kits)(role).map_err(|error| internal(&error))?;
        let connectors: Vec<Value> = kit
            .connectors
            .iter()
            .filter_map(|connector| match connector {
                KitConnector::Server {
                    entry,
                    copy,
                    allowances,
                    ..
                } => {
                    let mut row = json!({
                        "name": entry.name.as_str(), "title": copy.title, "about": copy.about,
                        "why": copy.why, "setup": copy.setup, "labels": copy.labels,
                        "auth": if entry.oauth.is_some() { "oauth" } else { "keys" },
                        "credential_keys": entry
                            .credential_keys
                            .iter()
                            .flatten()
                            .map(|key| key.as_str())
                            .collect::<Vec<_>>(),
                    });
                    if let Some(page) = &copy.key_page {
                        row["key_page"] = json!(page);
                    }
                    if at_launch(entry, apps) {
                        row["at_launch"] = json!(true);
                    }
                    if !allowances.is_empty() {
                        row["allowances"] = allowances
                            .iter()
                            .map(|(tool, offer)| {
                                json!({ "tool": tool, "calls": offer.calls, "what": offer.what })
                            })
                            .collect();
                    }
                    Some(row)
                }
                KitConnector::Container(_) => None,
            })
            .collect();
        if !connectors.is_empty() {
            kits.push(json!({ "role": role.to_string(), "connectors": connectors }));
        }
    }
    Ok(kits)
}

/// The usable tools of `listed`, each labelled by `tags` (an object of tool name to tag) or else
/// `external_effect` (SPEC 5.6).
///
/// # Errors
///
/// `tag_unknown_tool`, naming the usable tools, when `tags` labels a tool that is not one of them:
/// a misspelled `delete_rep=denied` would leave `delete_repo` unlabelled while its user believes
/// it denied.
pub fn labelled(
    listed: &[ListedTool],
    tags: &Value,
) -> Result<serde_json::Map<String, Value>, String> {
    let usable: Vec<&str> = listed
        .iter()
        .filter(|tool| tool.usable)
        .map(|tool| tool.name.as_str())
        .collect();
    if let Some(unknown) = tags
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, _)| name)
        .find(|name| !usable.contains(&name.as_str()))
    {
        return Err(format!(
            "tag_unknown_tool: {unknown} is not a tool this server lists that Farik can use; its \
             tools are {}",
            usable.join(", ")
        ));
    }
    Ok(usable
        .into_iter()
        .map(|name| {
            let tag = tags
                .get(name)
                .cloned()
                .unwrap_or_else(|| json!("external_effect"));
            (name.to_string(), tag)
        })
        .collect())
}

/// The keys `connector.tools` and `connector.connect` carry.
fn keys_of(params: &Value) -> BTreeMap<String, Secret> {
    params["keys"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, value)| {
            (
                name.clone(),
                Secret::new(value.as_str().unwrap_or_default().to_string()),
            )
        })
        .collect()
}

/// Why a server's tools could not be listed, in a sentence that quotes no key.
fn not_listed(error: ConnectorError) -> Failure {
    Failure::new(
        REFUSED,
        match error {
            ConnectorError::Timeout => {
                "the server did not answer within thirty seconds".to_string()
            }
            ConnectorError::KeyMissing(name) => format!("the key {name} has no value"),
            ConnectorError::Failed(why) => format!("its tools could not be listed: {why}"),
            // A listing calls no tool, so this is not one it can meet; it says no more than the
            // service did not list.
            ConnectorError::ToolError { .. } => "its tools could not be listed".to_string(),
        },
    )
}

/// What lists, and then keeps, a server for an agent.
enum Authority {
    /// The keys the user gave.
    Keys(BTreeMap<String, Secret>),
    /// A sign-in the user finished, and who it is with.
    SignedIn(Box<crate::sign_in::OAuthGrant>),
}

/// Whether `server` is one the user signs in to rather than gives keys.
fn signs_in(server: &CustomServer) -> bool {
    server.oauth().is_some()
}

/// The attempt `params` carries, for a server that signs in; its keys, for one that does not.
/// A server that signs in with no attempt is refused `sign_in_needed`, and an attempt for a server
/// that does not is `sign_in_unknown`.
fn authority(
    state: &DaemonState,
    params: &Value,
    server: &CustomServer,
) -> Result<Authority, Failure> {
    let agent = params["agent"].as_str().unwrap_or_default();
    match (signs_in(server), params["attempt"].as_str()) {
        (true, None) => Err(Failure::new(
            REFUSED,
            format!(
                "sign_in_needed: {} signs in to its service; sign in first",
                server.name
            ),
        )),
        (true, Some(attempt)) => state
            .peek_sign_in(attempt, &Binding { agent, server })
            .map(|grant| Authority::SignedIn(Box::new(grant)))
            .map_err(|why| Failure::new(REFUSED, why)),
        (false, Some(_)) => Err(Failure::new(
            REFUSED,
            "sign_in_unknown: this server takes keys, not a sign-in".to_string(),
        )),
        (false, None) => Ok(Authority::Keys(keys_of(params))),
    }
}

/// The tools of the server `params` describes for its agent, held to the team's rules first, as
/// it lists them with the keys `params` carries, or the sign-in its attempt made.
async fn listed(
    state: &DaemonState,
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Vec<ListedTool>, Failure> {
    let (held, asked) = (Arc::clone(deps), params.clone());
    let (_, server) = off_the_worker(move || {
        described(
            &held,
            asked["agent"].as_str().unwrap_or_default(),
            &asked["server"],
            json!({}),
        )
    })
    .await?;
    let authority = authority(state, params, &server)?;
    list_with(state, deps, params, &server, &authority).await
}

/// `server`'s tools, listed with `authority`.
async fn list_with(
    state: &DaemonState,
    deps: &Arc<ToolDeps>,
    params: &Value,
    server: &CustomServer,
    authority: &Authority,
) -> Result<Vec<ListedTool>, Failure> {
    let folder = secret_at(
        state,
        deps,
        params["agent"].as_str().unwrap_or_default(),
        &server.name,
    )
    .and_then(|at| state.connector_folder(&at))
    .map_err(|error| Failure::new(REFUSED, crate::connectors::folder_refusal(&error)))?;
    let (keys, bearer) = match authority {
        Authority::Keys(keys) => (keys.clone(), None),
        Authority::SignedIn(grant) => (BTreeMap::new(), Some(grant.access_token.clone())),
    };
    let farik = crate::connectors::own_program(server, state.own_program())
        .map_err(|why| Failure::new(REFUSED, why.to_string()))?;
    list_tools(server, &keys, bearer.as_ref(), &folder, &farik)
        .await
        .map_err(not_listed)
}

/// `connector.sign_in`: signs `agent` in to the service of the web-address server `server`.
async fn connector_sign_in(
    state: &Arc<DaemonState>,
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Value, Failure> {
    let (held, asked) = (Arc::clone(deps), params.clone());
    let (_, server) = off_the_worker(move || {
        described(
            &held,
            asked["agent"].as_str().unwrap_or_default(),
            &asked["server"],
            json!({}),
        )
    })
    .await?;
    let agent = params["agent"].as_str().unwrap_or_default();
    let started = state
        .begin_sign_in(agent, &server)
        .await
        .map_err(|why| Failure::new(REFUSED, why))?;
    let mut answer = json!({
        "attempt": started.attempt, "authorize_url": started.authorize_url, "issuer": started.issuer
    });
    for (name, value) in [
        ("provider", started.provider),
        ("user_code", started.user_code),
        ("install_url", started.install_url),
    ] {
        if let Some(value) = value {
            answer[name] = json!(value);
        }
    }
    Ok(answer)
}

/// `connector.connect`: the server's tools listed with its keys, or its sign-in, each usable one
/// labelled by `tags` or else `external_effect` (SPEC 5.6), the keys or the grant kept beside the
/// definition's hash, then `connector_connect` handled, which writes the team file and records
/// `connector.connected`.
async fn connector_connect(
    state: &Arc<DaemonState>,
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Value, Failure> {
    // The allowances are part of the entry (ADR 0037): where the entry is described, they are
    // beside its name. A custom server given any is refused `allowance_not_kit`.
    let mut with_allowances = params.clone();
    if let Some(allowances) = params.get("allowances") {
        with_allowances["server"]["allowances"] = allowances.clone();
    }
    let params = &with_allowances;
    let agent = params["agent"].as_str().unwrap_or_default().to_string();
    if params["server"]["source"] == "kit"
        && params["tags"]
            .as_object()
            .is_some_and(|tags| !tags.is_empty())
    {
        return Err(Failure::new(
            REFUSED,
            "kit_names_these: the kit says what each tool may do; there is nothing to label"
                .to_string(),
        ));
    }
    let (held, asked) = (Arc::clone(deps), params.clone());
    let (_, described_as) = off_the_worker(move || {
        described(
            &held,
            asked["agent"].as_str().unwrap_or_default(),
            &asked["server"],
            json!({}),
        )
    })
    .await?;
    // A sign-in's attempt is used up here, and put back if the connect fails before the grant is
    // kept, so a refused label does not cost the user their sign-in.
    let mut taken = None;
    let authority = match (signs_in(&described_as), params["attempt"].as_str()) {
        (true, Some(attempt)) => {
            let (grant, _, kept) = state
                .take_sign_in(
                    attempt,
                    &Binding {
                        agent: &agent,
                        server: &described_as,
                    },
                )
                .map_err(|why| Failure::new(REFUSED, why))?;
            taken = Some((attempt.to_string(), kept));
            Authority::SignedIn(Box::new(grant))
        }
        _ => authority(state, params, &described_as)?,
    };
    let mut saved = false;
    let result = Box::pin(connect_with(
        state, deps, params, &agent, &authority, &mut saved,
    ))
    .await;
    // Put back only while the grant is not kept: a retry then has nothing of its own to undo.
    if let (Err(_), Some((attempt, kept)), false) = (&result, taken, saved) {
        state.restore_sign_in(&attempt, kept);
    }
    result
}

/// What `connector_connect` does once it knows what lists and keeps the server. `saved` is set
/// once the entry is kept.
async fn connect_with(
    state: &Arc<DaemonState>,
    deps: &Arc<ToolDeps>,
    params: &Value,
    agent: &str,
    authority: &Authority,
    saved: &mut bool,
) -> Result<Value, Failure> {
    let (held, asked) = (Arc::clone(deps), params.clone());
    let (first, server) = off_the_worker(move || {
        described(
            &held,
            asked["agent"].as_str().unwrap_or_default(),
            &asked["server"],
            json!({}),
        )
    })
    .await?;
    // Listing proves the keys or the sign-in work. A kit's service is written with the kit's tags,
    // whatever the service lists: a tool it added is not offered, one it dropped costs nothing.
    let listed = list_with(state, deps, params, &server, authority).await?;
    let tools = if server.kit {
        first["tools"].as_object().cloned().unwrap_or_default()
    } else {
        labelled(&listed, &params["tags"]).map_err(|why| Failure::new(REFUSED, why))?
    };
    let (held, asked, labels) = (Arc::clone(deps), params.clone(), tools.clone());
    let (entry, server) = off_the_worker(move || {
        described(
            &held,
            asked["agent"].as_str().unwrap_or_default(),
            &asked["server"],
            Value::Object(labels),
        )
    })
    .await?;
    let spec = spec_sha256(&server);
    let (keys, oauth, issuer) = match authority {
        Authority::Keys(keys) => {
            let mut keys = keys.clone();
            keys.retain(|name, _| server.credential_keys.contains(name));
            (keys, None, None)
        }
        Authority::SignedIn(grant) => (
            BTreeMap::new(),
            Some((**grant).clone()),
            Some(grant.issuer.clone()),
        ),
    };
    let kept = ConnectorEntry {
        spec_sha256: spec.clone(),
        keys,
        oauth,
    };
    let at = secret_at(state, deps, agent, &server.name).map_err(|error| internal(&error))?;
    let (secrets, kept_at) = (state.connector_secrets(), at.clone());
    // Held across the read of what is replaced and the save, with refresh and remove.
    let lock = state.entry_lock(&at);
    let held = lock.lock().await;
    let (stored_in, replaced) = off_the_worker(move || {
        let replaced = secrets
            .load(&kept_at)
            .ok()
            .flatten()
            .and_then(|old| old.oauth);
        let stored_in = secrets
            .save(&kept_at, &kept)
            .map_err(|error| Failure::new(REFUSED, words(&error)))?;
        Ok((stored_in, replaced))
    })
    .await?;
    drop(held);
    *saved = true;
    // A sign-in this one replaces is asked to be forgotten, so it does not linger at the service;
    // not when it is the one just kept, or its client's (`revocable_after`).
    if let (Some(old), Authority::SignedIn(new)) = (replaced, authority)
        && old.revocable_after(new)
    {
        tokio::spawn(async move { crate::sign_in::revoke(&old).await });
    }
    handled(
        state,
        Command::ConnectorConnect {
            agent: agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer,
        },
    )
    .await?;
    Ok(json!({ "stored_in": stored_in, "tools": tools }))
}

/// What `connector_allowances` decides before it keeps anything: the kept entry, the team file's
/// entry with its new allowances, and the server it describes.
fn allowed_entry(
    deps: &ToolDeps,
    secrets: &dyn crate::connectors::ConnectorSecrets,
    at: &SecretAt,
    (agent, name): (&str, &str),
    requested: &Value,
) -> Result<(ConnectorEntry, Value, CustomServer), Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let held = team
        .agents
        .iter()
        .find(|held| held.id.as_str() == agent)
        .ok_or_else(|| Failure::new(REFUSED, format!("there is no agent {agent}")))?;
    let current = held
        .mcp_servers
        .iter()
        .flatten()
        .find(|server| server.name.as_str() == name)
        .ok_or_else(|| {
            Failure::new(
                REFUSED,
                format!("connector_not_found: {agent} has no connector {name}"),
            )
        })?;
    let current_server = custom_server(current)
        .filter(|server| server.kit)
        .ok_or_else(|| {
            Failure::new(
                REFUSED,
                format!(
                    "connector_not_in_kit: {name} is not a service of the kit, so it has no \
                 allowances"
                ),
            )
        })?;
    let old = crate::connectors::confirmed_entry(secrets, at, &current_server)
        .map_err(|error| Failure::new(REFUSED, words(&error)))?
        .ok_or_else(|| {
            Failure::new(
                REFUSED,
                format!(
                    "connector_not_confirmed: {name} is not as it was connected on this \
                     computer; connect it again"
                ),
            )
        })?;
    // Not the kit's now (a release changed it): connecting again is the way.
    let kit = (deps.kits)(Role::from(held.role)).map_err(|error| internal(&error))?;
    if !matches_kit(&kit, &current_server) {
        return Err(Failure::new(
            REFUSED,
            format!(
                "connector_not_in_kit: {name} is not what the kit says it is now; connect \
                 it again"
            ),
        ));
    }
    // The ones it holds, overlaid by the request: a tool left out keeps its number.
    let mut asked = current_server.allowances.clone();
    asked.extend(asked_allowances(requested).map_err(|why| Failure::new(REFUSED, why))?);
    let allowances =
        checked_allowances(&kit, name, &asked).map_err(|why| Failure::new(REFUSED, why))?;
    let mut entry = serde_json::to_value(current).map_err(|e| internal(&e))?;
    entry["allowances"] = json!(allowances);
    let (entry, server) =
        entry_in(&team, agent, entry).map_err(|errors| Failure::from(Refused::Errors(errors)))?;
    Ok((old, entry, server))
}

/// `connector.allowances`: a kit connector's allowances changed without typing its key again
/// (ADR 0037). With the entry's lock held, the kept entry must be the team file's entry now
/// (`connector_not_confirmed`); the allowances are checked as connect checks them, over the ones
/// the entry holds; the same keys or grant are kept beside the new entry's hash; then
/// `connector_connect` is handled with the new entry, which writes the team file and records
/// `connector.connected`. An entry the kit has changed is `connector_not_in_kit`, and nothing is
/// kept.
async fn connector_allowances(
    state: &Arc<DaemonState>,
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Value, Failure> {
    let (agent, name) = (
        params["agent"].as_str().unwrap_or_default().to_string(),
        params["server"].as_str().unwrap_or_default().to_string(),
    );
    let at = secret_at(state, deps, &agent, &name).map_err(|error| internal(&error))?;
    let lock = state.entry_lock(&at);
    let held = lock.lock().await;
    let (secrets, held_deps, requested) = (
        state.connector_secrets(),
        Arc::clone(deps),
        params["allowances"].clone(),
    );
    let (agent_of, name_of, at_of) = (agent.clone(), name.clone(), at.clone());
    let (old, entry, server) = off_the_worker(move || {
        allowed_entry(
            &held_deps,
            secrets.as_ref(),
            &at_of,
            (&agent_of, &name_of),
            &requested,
        )
    })
    .await?;
    let spec = spec_sha256(&server);
    let issuer = old.oauth.as_ref().map(|grant| grant.issuer.clone());
    let kept = ConnectorEntry {
        spec_sha256: spec.clone(),
        keys: old.keys.clone(),
        oauth: old.oauth.clone(),
    };
    let (secrets, save_at) = (state.connector_secrets(), at.clone());
    off_the_worker(move || {
        secrets
            .save(&save_at, &kept)
            .map(|_| ())
            .map_err(|error| Failure::new(REFUSED, words(&error)))
    })
    .await?;
    let handled_as = handled(
        state,
        Command::ConnectorConnect {
            agent: agent.clone(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer,
        },
    )
    .await;
    if let Err(failed) = handled_as {
        // The team file is as it was, so the kept entry goes back to match it.
        let (secrets, save_at) = (state.connector_secrets(), at.clone());
        let _ = off_the_worker(move || {
            secrets
                .save(&save_at, &old)
                .map(|_| ())
                .map_err(|error| Failure::new(REFUSED, words(&error)))
        })
        .await;
        drop(held);
        return Err(failed);
    }
    drop(held);
    Ok(json!({}))
}

/// `connector.disconnect`: `connector_disconnect` handled, which removes the entry from the team
/// file and records `connector.disconnected`, then the agent's entry deleted and, when it held a
/// sign-in, the service asked to forget it.
async fn connector_disconnect(
    state: &DaemonState,
    deps: &ToolDeps,
    params: &Value,
) -> Result<Value, Failure> {
    let (agent, server) = (
        params["agent"].as_str().unwrap_or_default(),
        params["server"].as_str().unwrap_or_default(),
    );
    handled(
        state,
        Command::ConnectorDisconnect {
            agent: agent.to_string(),
            server: server.to_string(),
        },
    )
    .await?;
    let at = secret_at(state, deps, agent, server).map_err(|error| internal(&error))?;
    let (secrets, deleted_at) = (state.connector_secrets(), at.clone());
    // Held across the delete: a refresh in flight saves before it, never after (ADR 0033).
    let lock = state.entry_lock(&at);
    let _held = lock.lock().await;
    let grant = off_the_worker(move || {
        let at = deleted_at;
        let grant = secrets
            .load(&at)
            .ok()
            .flatten()
            .and_then(|entry| entry.oauth);
        secrets
            .delete(&at)
            .map_err(|error| Failure::new(REFUSED, words(&error)))?;
        Ok(grant)
    })
    .await?;
    // A refresh that held the entry read it back as it saved: it is gone now.
    state.forget_kept(&at);
    if let Some(grant) = grant {
        crate::sign_in::revoke(&grant).await;
    }
    Ok(json!({}))
}

pub(super) fn web_of(state: &DaemonState) -> Result<&super::web::WebState, Failure> {
    state
        .web()
        .ok_or_else(|| Failure::new(INTERNAL_ERROR, "the browser routes are off"))
}

/// `team.propose`: the team as it is, with the six in place of its agents and planning in sprints,
/// and the criteria. The Designer comes with its Playwright connector on, and is listed
/// `unavailable` where it cannot have its browser for want of Docker's sandbox, which the page
/// shows unticked (D3).
fn propose(deps: &ToolDeps) -> Result<Value, Failure> {
    let current = deps.files.read_team().map_err(|e| internal(&e))?;
    let no_sandbox = deps.transitions.designer_browser(&current) == DesignerBrowser::NoSandbox;
    let mut team = serde_json::to_value(current).map_err(|e| internal(&e))?;
    let agents = suggested()?;
    let unavailable: Vec<Value> = agents
        .iter()
        .filter(|agent| no_sandbox && Role::from(agent.role) == Role::UiUxDesigner)
        .map(|agent| json!({ "agent_id": agent.id, "reason": "designer_needs_sandbox" }))
        .collect();
    team["agents"] = serde_json::to_value(agents).map_err(|e| internal(&e))?;
    // Setup only ever makes a new team, and a new team plans in sprints (ADR 0028).
    team["policy"]["plan_in_sprints"] = json!(true);
    let criteria = serde_json::to_value(deps.files.read_criteria().map_err(|e| internal(&e))?)
        .map_err(|e| internal(&e))?;
    Ok(json!({ "team": team, "criteria": criteria, "unavailable": unavailable }))
}

/// The six setup suggests, one of each role, active, each with its role's persona, model and
/// effort and its shipped picture; the Designer with its Playwright connector on (step 12). A
/// template's added agent takes from these whatever the template leaves out.
pub(super) fn suggested() -> Result<Vec<Agent>, Failure> {
    SIX.iter()
        .map(|(name, role, avatar)| {
            let shipped = load_role(*role).map_err(|e| internal(&e))?;
            let mut agent = json!({
                "id": name.to_lowercase(),
                "display_name": name,
                "role": role,
                "avatar": avatar,
                "persona": shipped.persona,
                "status": "active",
                "model": { "id": shipped.model, "effort": shipped.effort },
            });
            if *role == Role::UiUxDesigner {
                agent["mcp_servers"] = json!([{ "name": "playwright", "source": "builtin" }]);
            }
            serde_json::from_value(agent).map_err(|e| internal(&e))
        })
        .collect()
}

/// `models.list`: the newest model of each family the prices name.
fn models(deps: &ToolDeps) -> Result<Value, Failure> {
    let models: Vec<Value> = newest(deps)?
        .into_iter()
        .map(|(id, label)| json!({ "id": id, "label": label }))
        .collect();
    Ok(json!({ "models": models }))
}

/// The newest model of each family the prices name, with its words. A dated snapshot is the same
/// model as its alias, so it is passed over.
fn newest(deps: &ToolDeps) -> Result<Vec<(String, &'static str)>, Failure> {
    let prices = deps.files.effective_prices().map_err(|e| internal(&e))?;
    Ok(MODEL_FAMILIES
        .iter()
        .filter_map(|(prefix, label, _)| {
            prices
                .prices
                .keys()
                .filter_map(|id| {
                    let version = id
                        .strip_prefix(prefix)?
                        .split('-')
                        .map(|part| part.parse::<u32>().ok().filter(|n| *n < 1000))
                        .collect::<Option<Vec<u32>>>()?;
                    Some((version, id))
                })
                .max()
                .map(|(_, id)| (id.clone(), *label))
        })
        .collect())
}

/// The words for `id`: its family's, marked older when a newer one of the family is priced, or
/// the id itself when no family Farik names it.
fn model_label(id: &str, newest: &[(String, &str)]) -> String {
    match MODEL_FAMILIES
        .iter()
        .find(|(prefix, _, _)| id.starts_with(prefix))
    {
        Some((_, label, _)) if newest.iter().any(|(new, _)| new == id) => (*label).to_string(),
        Some((_, label, _)) => format!("{label} (older)"),
        None => id.to_string(),
    }
}

/// What the browser shows of `team` and does not work out itself: each agent's model and effort
/// as its sessions run them, the tiers it holds and those its role and the team's answers give it
/// before its own grants and revokes; and who checks plans under each choice.
fn effective(deps: &ToolDeps, team: &Team) -> Result<Value, Failure> {
    let newest = newest(deps)?;
    let permissions = team.permissions();
    let agents = team
        .agents
        .iter()
        .map(|agent| {
            let role = load_role(Role::from(agent.role)).map_err(|e| internal(&e))?;
            let (model, effort) = session_model(agent, &role);
            let mut bare = agent.clone();
            bare.grants = None;
            bare.revokes = None;
            Ok(json!({
                "id": agent.id,
                "model": { "id": model, "label": model_label(&model, &newest), "effort": effort },
                "tiers": agent.tiers(&permissions),
                "base_tiers": bare.tiers(&permissions),
            }))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let holder = |role: Role| {
        team.active_agents()
            .find(|agent| Role::from(agent.role) == role)
            .map(|agent| {
                json!({ "agent_id": agent.id, "display_name": agent.display_name, "role": agent.role })
            })
    };
    let mut auto = team.clone();
    if let Some(judgment) = auto.policy.judgment.as_mut() {
        judgment.judge = farik_core::team::JudgeChoice::Auto;
    }
    Ok(json!({
        "agents": agents,
        "judges": {
            "auto": holder(auto.judge()),
            "architect": holder(Role::Architect),
            "scrum_master": holder(Role::ScrumMaster),
        },
    }))
}

/// `project.scan`: the project scanned again, its facts for the rows, the checks it found, and the
/// protected globs that match something on disk.
fn scanned(deps: &ToolDeps) -> Result<Value, Failure> {
    let scan = scan_project(&deps.git, deps.clock.now()).map_err(|e| internal(&e))?;
    let globs = deps
        .files
        .read_team()
        .map_err(|e| internal(&e))?
        .rules()
        .protected_paths;
    let mut on_disk = Vec::new();
    walk(deps.files.root(), "", 1, &mut on_disk);
    let kept_private: Vec<&String> = globs
        .iter()
        // Farik's own local folder is not the user's private file.
        .filter(|glob| !glob.starts_with(".farik/local"))
        .filter(|glob| {
            matches!(
                check_protected_paths(&on_disk, std::slice::from_ref(glob)),
                Err(PathRefusal::Violations(_))
            )
        })
        .collect();
    let facts = &scan.facts;
    Ok(json!({
        "facts": {
            "language": facts.language,
            "toolchain": facts.toolchain,
            "workspace": facts.workspace,
            "packages": facts.packages,
            "tests_in": facts.tests_in,
            "tracked_files": facts.tracked_files,
            "last_commit": facts.last_commit,
        },
        "checks": scan.detected_criteria.iter().map(|one| one.text.to_string()).collect::<Vec<_>>(),
        "kept_private": kept_private,
    }))
}

/// Adds the paths under `dir`, as `prefix` and each name, to `found`, in name order and folders
/// first-deep, `WALK_DEPTH` folders down, without `.git` and `node_modules` or following a link,
/// until `found` holds `WALK_CAP`.
fn walk(dir: &Path, prefix: &str, depth: u32, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in names {
        if found.len() >= WALK_CAP {
            return;
        }
        if name == ".git" || name == "node_modules" {
            continue;
        }
        let path = format!("{prefix}{name}");
        found.push(path.clone());
        let full = dir.join(&name);
        if depth < WALK_DEPTH && std::fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_dir()) {
            walk(&full, &format!("{path}/"), depth + 1, found);
        }
    }
}

/// Why a team change is not made: the errors to show at their rows, or a failure.
pub(super) enum Refused {
    Errors(Vec<ValidationError>),
    Failed(Failure),
}

impl From<Refused> for Failure {
    fn from(refused: Refused) -> Failure {
        match refused {
            Refused::Failed(failed) => failed,
            Refused::Errors(errors) => {
                let mut failure = Failure::new(
                    REFUSED,
                    errors
                        .iter()
                        .map(|error| error.message.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                );
                failure.data = Some(json!({ "errors": errors_wire(&errors) }));
                failure
            }
        }
    }
}

pub(super) fn errors_wire(errors: &[ValidationError]) -> Vec<Value> {
    errors
        .iter()
        .map(
            |error| json!({ "path": error.path, "message": error.message, "code": code_of(error) }),
        )
        .collect()
}

/// The refusal `error` is, as a code the page words for a person (spec 10, ADR 0016): the page
/// never shows a schema's own text. `invalid` is any other.
fn code_of(error: &ValidationError) -> &'static str {
    let (path, message) = (error.path.as_str(), error.message.as_str());
    let segments: Vec<&str> = path.split('/').skip(1).collect();
    match segments.as_slice() {
        ["agents"] if message.starts_with("A team has seven") => "too_many",
        ["agents"] if message.starts_with("A team needs an active Product Manager") => {
            "needs_product_manager"
        }
        ["agents"] if message.starts_with("A team needs an active Software Developer") => {
            "needs_developer"
        }
        ["agents"] if message.starts_with("an agent id names") => "repeated_id",
        ["agents"] if message.ends_with(WORKED) => "worked",
        ["agents", _, "display_name"] => "name",
        ["agents", _, "revokes"] => "keeps_read",
        ["agents", _, "status"] if message == FROM_THE_CARD => "status_from_card",
        ["policy", "judgment", "judge"] => "judge_not_held",
        ["policy", "judgment", "questions"] => "no_questions",
        ["policy", "judgment", "questions", _] => "question_length",
        _ => "invalid",
    }
}

/// The team as it is and `wire` as the team it would be, held to the schema's and the team's rules
/// and to two of a save's: a status changes only from the agent's card, and an agent the log has
/// seen work from is retired rather than removed, except in `setup`, whose starter team is
/// replaced.
fn checked(deps: &ToolDeps, wire: &Value, setup: bool) -> Result<(Team, Team), Refused> {
    let mut after = validate_team(wire).map_err(Refused::Errors)?;
    let before = deps
        .files
        .read_team()
        .map_err(|e| Refused::Failed(internal(&e)))?;
    keep_pins(&before, &mut after);
    let mut errors = Vec::new();
    for (index, agent) in after.agents.iter().enumerate() {
        if before
            .agents
            .iter()
            .any(|was| was.id == agent.id && was.status != agent.status)
        {
            errors.push(ValidationError {
                path: format!("/agents/{index}/status"),
                message: FROM_THE_CARD.to_string(),
            });
        }
    }
    if !setup {
        for gone in before
            .agents
            .iter()
            .filter(|was| !after.agents.iter().any(|agent| agent.id == was.id))
        {
            if worked(deps, gone.id.as_str()).map_err(Refused::Failed)? {
                errors.push(ValidationError {
                    path: "/agents".to_string(),
                    message: format!("{}{WORKED}", gone.display_name.as_str()),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok((before, after))
    } else {
        Err(Refused::Errors(errors))
    }
}

/// `after`'s skills as `before` holds them, whatever the browser sent: only `skill_save`,
/// `skill_remove` and `skill_confirm` change a pin (ADR 0034). An agent `before` does not have
/// has none.
pub(super) fn keep_pins(before: &Team, after: &mut Team) {
    after.skills.clone_from(&before.skills);
    for agent in &mut after.agents {
        agent.skills = before
            .agents
            .iter()
            .find(|was| was.id == agent.id)
            .map(|was| was.skills.clone())
            .unwrap_or_default();
    }
}

/// Whether the log has an event of `agent_id`'s, or a task is left with it as its assignee or
/// reviewer: work, which retires an agent rather than removing it (step 06), so no task is left
/// with nobody.
pub(super) fn worked(deps: &ToolDeps, agent_id: &str) -> Result<bool, Failure> {
    let holds = deps
        .projections
        .board()
        .map_err(|e| internal(&e))?
        .iter()
        .any(|task| {
            [&task.assignee_id, &task.reviewer_id]
                .iter()
                .any(|held| held.as_deref() == Some(agent_id))
        });
    if holds {
        return Ok(true);
    }
    let seen = deps
        .log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            limit: Some(1),
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    Ok(!seen.is_empty())
}

/// `agent.replace`: the agent retired and the newcomer added in one write, checked as a save is
/// against the team it replaces, under the lock from the read to the write; then `agent.updated`
/// and `team.updated`.
fn replace(deps: &ToolDeps, state: &DaemonState, params: &Value) -> Result<(), Failure> {
    let agent_id = params["agent_id"].as_str().unwrap_or_default();
    let writing = state.team_writes();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let mut wire = serde_json::to_value(&team).map_err(|e| internal(&e))?;
    let Some(at) = team
        .agents
        .iter()
        .position(|agent| agent.id.as_str() == agent_id)
    else {
        return Err(Failure::new(
            super::web::NOT_FOUND,
            format!("there is no agent {agent_id}"),
        ));
    };
    wire["agents"][at]["status"] = json!("retired");
    if let Some(agents) = wire["agents"].as_array_mut() {
        agents.push(params["newcomer"].clone());
    }
    let after = validate_team(&wire).map_err(|errors| Failure::from(Refused::Errors(errors)))?;
    // A newcomer starts with no skills: only the skill commands pin one.
    let newcomer = after.agents.last().cloned().map(|mut agent| {
        agent.skills.clear();
        agent
    });
    let report = crate::orchestrator::update_agent_held(
        deps,
        state,
        agent_id,
        farik_core::team::AgentStatus::Retired,
        newcomer,
    );
    drop(writing);
    match report {
        Ok(_) => {}
        Err(crate::orchestrator::CommandError::Refused { reason }) => {
            return Err(Failure::new(REFUSED, reason));
        }
        Err(error) => return Err(internal(&format!("{error:?}"))),
    }
    let written = deps.files.read_team().map_err(|e| internal(&e))?;
    append(deps, team_updated(&written, None))
}

/// Appends `body` as the human's and projects it.
pub(super) fn append(deps: &ToolDeps, body: EventBody) -> Result<(), Failure> {
    let event = new_event(body, deps.clock.now(), deps.ids.clone())
        .map_err(|error| internal(&format!("{error:?}")))?;
    let recorded = deps.log.append(&event).map_err(|e| internal(&e))?;
    deps.projections
        .apply(&recorded)
        .map_err(|e| internal(&e))?;
    Ok(())
}

/// Writes `team` and records `team.updated`.
/// Writes `team` and records `team.updated`; an agent it no longer has loses its connector keys.
fn write_team(deps: &ToolDeps, state: &DaemonState, team: &Team) -> Result<(), Failure> {
    let before = deps.files.read_team().ok();
    deps.files.write_team(team).map_err(|e| internal(&e))?;
    if let Some(before) = before {
        crate::orchestrator::forget_removed_keys(deps, state, &before, team);
    }
    append(deps, team_updated(team, None))
}

/// `team.updated` for `team`, as the human's, naming the saved team it was made from.
pub(super) fn team_updated(team: &Team, template: Option<&str>) -> EventBody {
    EventBody::TeamUpdated(TeamUpdatedBody {
        team_name: team.name.to_string(),
        agent_ids: team
            .agents
            .iter()
            .map(|agent| agent.id.to_string())
            .collect(),
        updated_by: HUMAN.to_string(),
        template: template.map(str::to_string),
        plan_in_sprints: Some(team.plans_in_sprints()),
    })
}

/// The criterion library `wire` is, or the refusal with the schema's errors.
fn library(wire: &Value) -> Result<farik_core::criteria::CriteriaLibrary, Failure> {
    validate_criteria(wire).map_err(|errors| Refused::Errors(errors).into())
}

/// Writes the library and records `criteria.updated`.
fn write_criteria(
    deps: &ToolDeps,
    library: &farik_core::criteria::CriteriaLibrary,
) -> Result<(), Failure> {
    deps.files
        .write_criteria(library)
        .map_err(|e| internal(&e))?;
    append(
        deps,
        EventBody::CriteriaUpdated(CriteriaUpdatedBody {
            criterion_names: names_of(&library.criteria),
            updated_by: HUMAN.to_string(),
        }),
    )
}

/// The methods of this module, whose params the schema already passed.
pub(super) async fn call(
    state: &Arc<DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    if method == "account.disconnect" {
        return disconnect(state).await;
    }
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    let params = params.clone();
    match method {
        "connector.tools" => {
            let tools = Box::pin(listed(state, &deps, &params)).await?;
            Ok(json!({ "tools": tools
                .iter()
                .map(|tool| json!({
                    "name": tool.name, "description": tool.description, "usable": tool.usable,
                }))
                .collect::<Vec<_>>() }))
        }
        // Boxed: listing a server's tools makes a large future of every method's.
        "connector.connect" => Box::pin(connector_connect(state, &deps, &params)).await,
        "connector.allowances" => connector_allowances(state, &deps, &params).await,
        "connector.disconnect" => connector_disconnect(state, &deps, &params).await,
        "connector.sign_in" => Box::pin(connector_sign_in(state, &deps, &params)).await,
        "connector.sign_in_status" => state
            .sign_in_status(params["attempt"].as_str().unwrap_or_default())
            .map_err(|why| Failure::new(REFUSED, why)),
        "connector.sign_in_cancel" => {
            state
                .cancel_sign_in(params["attempt"].as_str().unwrap_or_default())
                .await;
            Ok(json!({}))
        }
        "project.note" => off_the_worker(move || {
            deps.files
                .append_project_note(
                    params["text"].as_str().unwrap_or_default(),
                    deps.clock.now().date_naive(),
                )
                .map_err(|e| internal(&e))
        })
        .await
        .map(|()| json!({})),
        "team.save" => {
            let holder = Arc::clone(state);
            off_the_worker(move || {
                let _writing = holder.team_writes();
                let (_, team) = checked(&deps, &params["team"], false)?;
                write_team(&deps, &holder, &team)
            })
            .await?;
            // A saved rule can free work at once: the policy switched off frees the Backlog.
            state.wakes().notify_one();
            Ok(json!({}))
        }
        "agent.replace" => {
            let holder = Arc::clone(state);
            off_the_worker(move || replace(&deps, &holder, &params)).await?;
            // A newcomer can take ready work at once.
            state.wakes().notify_one();
            Ok(json!({}))
        }
        "criteria.save" => {
            off_the_worker(move || write_criteria(&deps, &library(&params["criteria"])?))
                .await
                .map(|()| json!({}))
        }
        _ => {
            let (held, holder) = (Arc::clone(&deps), Arc::clone(state));
            // Only setup's start resumes the team: without the marker, a team paused by a
            // budget's stop or by a person stays paused.
            let setup = off_the_worker(move || {
                let _writing = holder.team_writes();
                let marker = held.files.root().join(SETUP_PENDING);
                let setup = marker.exists();
                let (_, team) = checked(&held, &params["team"], setup)?;
                let library = library(&params["criteria"])?;
                write_team(&held, &holder, &team)?;
                write_criteria(&held, &library)?;
                match std::fs::remove_file(&marker) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        Err(internal(&error))
                    }
                    _ => Ok(setup),
                }
            })
            .await?;
            if setup && paused(&deps.log).map_err(|e| internal(&e))? {
                handled(state, Command::TeamResume).await?;
            }
            Ok(json!({}))
        }
    }
}

/// `command`, handled by the orchestrator, or its refusal.
async fn handled(state: &DaemonState, command: Command) -> Result<(), Failure> {
    match super::handled(state, command).await {
        CommandReply::Error { detail, .. } => Err(Failure::new(REFUSED, detail)),
        CommandReply::Done { .. } => Ok(()),
    }
}

/// `account.disconnect`: the credential deleted from every store that holds it and, when one did,
/// the team paused if it was running, since the driver keeps the key it already loaded. A credential from the
/// environment cannot be removed, so nothing is, and the answer names its variable.
async fn disconnect(state: &DaemonState) -> Result<Value, Failure> {
    let web = web_of(state)?;
    if let Some(variable) = credential_variable(&web.env) {
        return Ok(
            json!({ "removed_from": [], "paused": false, "environment_variable": variable }),
        );
    }
    let stores = web.stores.clone();
    let removed = off_the_worker(move || {
        let mut removed = Vec::new();
        for store in &stores {
            match store.load() {
                Ok(Some(_)) => {
                    store.delete().map_err(|error| internal(&words(&error)))?;
                    removed.push(store.source());
                }
                Ok(None) | Err(CredentialError::NoKeychain) => {}
                Err(error) => return Err(internal(&words(&error))),
            }
        }
        Ok(removed)
    })
    .await?;
    // `paused` says whether this call paused the team, not whether it is paused.
    let paused_now = match state.deps() {
        Some(deps) if !removed.is_empty() && !paused(&deps.log).map_err(|e| internal(&e))? => {
            handled(state, Command::TeamPause).await?;
            true
        }
        _ => false,
    };
    Ok(json!({ "removed_from": removed, "paused": paused_now }))
}

fn words(error: &CredentialError) -> String {
    match error {
        CredentialError::NoKeychain => "this computer has no keychain".to_string(),
        CredentialError::Failed(why) => why.clone(),
    }
}

/// `work`, which reads and writes the store, run where it cannot hold up the daemon's worker.
pub(super) async fn off_the_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| Err(internal(&error)))
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};
    use serde_json::{Value, json};

    use farik_protocol::command::{command_from_value, command_to_value};
    use farik_protocol::event::event_to_value;

    use crate::claude::{ClaudeCredential, Secret, SharedCredential};
    use crate::connectors::{
        ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
    };
    use crate::credential::{CredentialStore, MemoryStore};
    use crate::daemon::gates::tests::{call, driven, query, rpc};
    use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
    use crate::orchestrator::fixtures::Harness;
    use crate::pause::paused;
    use crate::tools::fixtures::at;

    const MARKER: &str = ".farik/local/setup-pending";

    /// `harness`'s daemon with its browser routes on, its credential kept in `store`, and `env`;
    /// answered with the credential its sessions start with.
    fn served(
        harness: &Harness,
        store: &Arc<MemoryStore>,
        env: &[(&str, &str)],
    ) -> SharedCredential {
        let stores: Vec<Arc<dyn CredentialStore>> = vec![Arc::clone(store) as _];
        let in_use: SharedCredential = Arc::new(std::sync::Mutex::new(ClaudeCredential::ApiKey(
            Secret::new("sk-ant-api-old".to_string()),
        )));
        assert!(
            harness.daemon.set_web(WebState {
                codes: ConnectCodes::default(),
                sessions: BrowserSessions::open(None).expect("the sessions open"),
                project_root: harness.project.repo.path.clone(),
                credential: None,
                port: 49_731,
                clock: Arc::new(FixedClock::new(at())),
                take_on_error: std::sync::Mutex::default(),
                stores,
                env: env
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                    .collect::<BTreeMap<_, _>>(),
                in_use: Some(Arc::clone(&in_use)),
                templates: None,
                #[cfg(feature = "e2e")]
                admit_local_preview: false,
            })
        );
        in_use
    }

    /// An event `agent` produced, which is work the log has seen.
    pub(crate) fn worked(harness: &Harness, agent: &str) {
        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": at().to_rfc3339(), "team_id": "farik", "project_id": "farik",
            "agent_id": agent, "kind": "tool.called", "body": { "tool": "Read", "input": "{}" },
        }))
        .expect("the fixture is schema-valid");
        harness
            .project
            .deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
    }

    pub(crate) fn team_file(harness: &Harness) -> Value {
        serde_json::to_value(
            harness
                .project
                .deps
                .files
                .read_team()
                .expect("the team reads"),
        )
        .expect("the team is JSON")
    }

    fn kinds(harness: &Harness) -> Vec<EventKind> {
        harness
            .project
            .events(&[])
            .iter()
            .map(|event| event.body.kind())
            .collect()
    }

    /// The error of the reply to `method` with `params`: its code and its message.
    pub(crate) fn refused(harness: &Harness, method: &str, params: &Value) -> (i64, String) {
        let reply = rpc(&harness.daemon, method, params);
        (
            reply["error"]["code"].as_i64().unwrap_or_default(),
            reply["error"]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("an error: {reply}"))
                .to_string(),
        )
    }

    /// The team's skill, or `agent`'s, `name`, saved through the function the commands use, so
    /// that it is pinned and confirmed (the save is the confirmation).
    pub(crate) fn save_a_skill(
        harness: &Harness,
        agent: Option<&str>,
        name: &str,
        extra_front: &str,
    ) {
        let text = format!(
            "---\nname: {name}\ndescription: Use when {name}.\n{extra_front}---\nbody of {name}"
        );
        let files = BTreeMap::from([
            ("SKILL.md".to_string(), text.into_bytes()),
            ("references/a.md".to_string(), b"details".to_vec()),
        ]);
        let level = agent.map_or(crate::skills::SkillLevel::Team, |agent| {
            crate::skills::SkillLevel::Agent(agent.to_string())
        });
        crate::skills::save_skill(&harness.project.deps, &level, &files, false).expect("saved");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn skills_list_answers_the_rows_of_one_agent() {
        let harness = Harness::new("rpc-skills-list", |_| {});
        save_a_skill(&harness, None, "api-style", "");
        save_a_skill(&harness, Some("dev-a"), "notes", "");
        let got = query(
            &harness.daemon,
            "skills.list",
            &json!({ "agent": "dev-a" }),
            "skillsListResult",
        );
        let rows: Vec<(&str, &str, &str)> = got["skills"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| {
                (
                    row["level"].as_str().unwrap_or_default(),
                    row["name"].as_str().unwrap_or_default(),
                    row["state"].as_str().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("role", "implementing-a-contract", "in_use"),
                ("role", "test-driven-development", "in_use"),
                ("role", "debugging", "in_use"),
                ("role", "safe-migrations", "in_use"),
                ("role", "testing-per-stack", "in_use"),
                ("role", "answering-a-review", "in_use"),
                ("role", "using-docs-and-the-browser", "in_use"),
                ("team", "api-style", "in_use"),
                ("agent", "notes", "in_use"),
            ]
        );
        assert!(
            got["skills"][8]["bytes"].as_u64().unwrap_or_default() > 0,
            "{got}"
        );
        assert_eq!(got["skills"][8]["description"], "Use when notes.");
        let ghost = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skills.list", "params": { "agent": "ghost" } }),
        );
        assert_eq!(ghost["error"]["code"], -32002, "{ghost}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn skill_get_answers_the_files_and_ignored_fields() {
        let harness = Harness::new("rpc-skill-get", |_| {});
        save_a_skill(
            &harness,
            Some("dev-a"),
            "api-style",
            "allowed-tools: Bash\n",
        );
        let asked = json!({ "level": "agent", "agent": "dev-a", "name": "api-style" });
        let got = query(&harness.daemon, "skill.get", &asked, "skillGetResult");
        let folder = harness
            .project
            .repo
            .path
            .join(".farik/agents/dev-a/skills/api-style");
        let held = crate::skills::read_skill_folder(&folder).expect("readable");
        assert_eq!(got["sha256"], farik_core::skill::skill_sha256(&held));
        assert_eq!(got["ignored_fields"], json!(["allowed-tools"]));
        let files: BTreeMap<String, String> = held
            .iter()
            .map(|(path, bytes)| (path.clone(), String::from_utf8_lossy(bytes).into_owned()))
            .collect();
        assert_eq!(
            got["files"],
            json!(files),
            "the files as the folder holds them"
        );
        assert_eq!(got["files"]["references/a.md"], "details");

        // A folder holding a command is answered with the refusal's code, not its files.
        std::fs::write(
            folder.join("SKILL.md"),
            "---\nname: api-style\ndescription: d\n---\nrun !`ls`",
        )
        .expect("an edit");
        let refused = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params": asked }),
        );
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("skill_runs_commands: ")),
            "{refused}"
        );
        // A level names its agent or none.
        for params in [
            json!({ "level": "team", "agent": "dev-a", "name": "api-style" }),
            json!({ "level": "agent", "name": "api-style" }),
        ] {
            let reply = rpc(
                &harness.daemon,
                "query",
                &json!({ "name": "skill.get", "params": params }),
            );
            assert_eq!(reply["error"]["code"], -32005, "{reply}");
        }
        // No folder, no skill.
        let missing = json!({ "level": "team", "name": "nothing" });
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params": missing }),
        );
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
        // No such agent, and a name that is not a skill name, are refused before any path is built.
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params":
                { "level": "agent", "agent": "../../x", "name": "api-style" } }),
        );
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
        assert!(
            reply["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("no agent")),
            "{reply}"
        );
        // The protocol schema refuses such a name first; the function refuses it too.
        let refused = super::skill_get(
            &harness.project.deps,
            &json!({ "level": "team", "name": "../x" }),
        )
        .expect_err("refused");
        let shown = format!("{refused:?}");
        assert!(
            shown.contains("-32005") && shown.contains("skill_name_invalid: "),
            "{shown}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_a_kit_skill_as_the_roles() {
        let harness = Harness::new("rpc-kit-skill-rows", |_| {});
        harness
            .project
            .set_kit(crate::tools::fixtures::a_developer_kit(
                &[("launch-plans", "KIT-BODY")],
                None,
            ));
        let rows = || {
            let got = query(
                &harness.daemon,
                "skills.list",
                &json!({ "agent": "dev-a" }),
                "skillsListResult",
            );
            got["skills"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| {
                    (
                        row["level"].as_str().unwrap_or_default().to_string(),
                        row["name"].as_str().unwrap_or_default().to_string(),
                        row["state"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let row = |level: &str, name: &str, state: &str| {
            (level.to_string(), name.to_string(), state.to_string())
        };
        assert_eq!(
            rows(),
            [
                row("role", "implementing-a-contract", "in_use"),
                row("role", "launch-plans", "in_use"),
            ]
        );
        // A team skill of that name in use replaces it.
        save_a_skill_replacing(&harness, "launch-plans");
        assert_eq!(
            rows(),
            [
                row("role", "implementing-a-contract", "in_use"),
                row("role", "launch-plans", "replaced"),
                row("team", "launch-plans", "in_use"),
            ]
        );
        let got = query(
            &harness.daemon,
            "skill.get",
            &json!({ "level": "role", "role": "software_developer", "name": "launch-plans" }),
            "skillGetResult",
        );
        let text = got["files"]["SKILL.md"].as_str().expect("the text");
        assert!(text.starts_with("---\nname: launch-plans\n"), "{text:?}");
        assert!(text.ends_with("KIT-BODY"), "{text}");
        assert!(got.get("sha256").is_none(), "{got}");
        // Another role's kit skill is not this role's.
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params":
                { "level": "role", "role": "architect", "name": "launch-plans" } }),
        );
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
    }

    /// The team's skill `name` saved over a shipped name, as the person said to.
    fn save_a_skill_replacing(harness: &Harness, name: &str) {
        let text = format!("---\nname: {name}\ndescription: Use when {name}.\n---\nTEAM-BODY");
        let files = BTreeMap::from([("SKILL.md".to_string(), text.into_bytes())]);
        crate::skills::save_skill(
            &harness.project.deps,
            &crate::skills::SkillLevel::Team,
            &files,
            true,
        )
        .expect("saved");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn skill_get_reads_a_shipped_role_skill_without_a_hash() {
        let harness = Harness::new("rpc-skill-get-role", |_| {});
        let asked = json!({
            "level": "role", "role": "software_developer", "name": "implementing-a-contract"
        });
        let got = query(&harness.daemon, "skill.get", &asked, "skillGetResult");
        let shipped = farik_roles::load_role(farik_core::contract::Role::SoftwareDeveloper)
            .expect("ships")
            .skills
            .remove(0);
        assert_eq!(got["files"], json!({ "SKILL.md": shipped.text }));
        assert!(got.get("sha256").is_none(), "{got}");
        assert_eq!(got["ignored_fields"], json!([]));
        // The role and the name are checked before anything is looked up.
        for params in [
            json!({ "level": "role", "role": "../../x", "name": "implementing-a-contract" }),
            json!({ "level": "role", "role": "human", "name": "implementing-a-contract" }),
            json!({ "level": "role", "name": "implementing-a-contract" }),
        ] {
            let reply = rpc(
                &harness.daemon,
                "query",
                &json!({ "name": "skill.get", "params": params }),
            );
            assert_eq!(reply["error"]["code"], -32005, "{params}: {reply}");
        }
        let refused = super::skill_get(
            &harness.project.deps,
            &json!({ "level": "role", "role": "software_developer", "name": "../x" }),
        )
        .expect_err("refused");
        assert!(format!("{refused:?}").contains("skill_name_invalid: "));
        // A skill another role ships is not this role's.
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params":
                { "level": "role", "role": "software_developer", "name": "api-style" } }),
        );
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_skill_with_a_file_that_is_not_text_is_refused_everywhere() {
        let harness = Harness::new("rpc-skill-binary", |_| {});
        let text = "---\nname: api-style\ndescription: Use when.\n---\nbody".as_bytes();
        let files = BTreeMap::from([
            ("SKILL.md".to_string(), text.to_vec()),
            ("references/a.md".to_string(), vec![0xff, 0xfe, b'x']),
        ]);
        let level = crate::skills::SkillLevel::Team;
        let saved = crate::skills::save_skill(&harness.project.deps, &level, &files, false)
            .expect_err("refused at save");
        assert_eq!(saved.code(), "skill_file_not_text");
        // A folder edited outside Farik is refused at read and at confirm, naming the file.
        save_a_skill(&harness, None, "api-style", "");
        let folder = harness.project.repo.path.join(".farik/skills/api-style");
        std::fs::write(folder.join("references/a.md"), [0xff, 0xfe, b'x']).expect("edit");
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "skill.get", "params": { "level": "team", "name": "api-style" } }),
        );
        let message = reply["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.starts_with("skill_file_not_text: ") && message.contains("references/a.md"),
            "{reply}"
        );
        let held = crate::skills::read_skill_folder(&folder).expect("readable");
        let hash = farik_core::skill::skill_sha256(&held);
        let confirmed =
            crate::skills::confirm_skill(&harness.project.deps, &level, "api-style", &hash, false)
                .expect_err("refused at confirm");
        assert_eq!(confirmed.code(), "skill_file_not_text");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn whole_team_saves_keep_pins() {
        let harness = Harness::new("rpc-skills-pins", |_| {});
        save_a_skill(&harness, None, "api-style", "");
        save_a_skill(&harness, Some("dev-a"), "notes", "");
        let pins = |harness: &Harness| {
            let team = team_file(harness);
            (team["skills"].clone(), team["agents"][1]["skills"].clone())
        };
        let (team_pins, agent_pins) = pins(&harness);
        assert_eq!(team_pins[0]["name"], "api-style");
        assert_eq!(agent_pins[0]["name"], "notes");

        // A team the browser sends with no pins, or other ones, leaves both levels as they were.
        let mut sent = team_file(&harness);
        sent.as_object_mut().expect("a team").remove("skills");
        sent["agents"][1]
            .as_object_mut()
            .expect("an agent")
            .remove("skills");
        sent["name"] = json!("Renamed");
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": sent }),
            "emptyResult",
        );
        assert_eq!(team_file(&harness)["name"], "Renamed");
        assert_eq!(pins(&harness), (team_pins.clone(), agent_pins.clone()));
        let mut other = team_file(&harness);
        other["skills"] = json!([{ "name": "forged", "sha256": "0".repeat(64) }]);
        other["agents"][1]["skills"] = json!([]);
        other["agents"][2]["skills"] = json!([{ "name": "forged", "sha256": "0".repeat(64) }]);
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": other }),
            "emptyResult",
        );
        assert_eq!(pins(&harness), (team_pins.clone(), agent_pins.clone()));
        assert_eq!(
            team_file(&harness)["agents"][2].get("skills"),
            None,
            "dev-b got none"
        );

        // An agent a save adds has no pins either, whatever it carries.
        let mut added = team_file(&harness);
        let mut zed = farik_core::team::fixtures::an_agent_wire("zed", "architect");
        zed["skills"] = json!([{ "name": "forged", "sha256": "0".repeat(64) }]);
        added["agents"].as_array_mut().expect("agents").push(zed);
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": added }),
            "emptyResult",
        );
        assert_eq!(team_file(&harness)["agents"][3].get("skills"), None);
        assert_eq!(pins(&harness), (team_pins.clone(), agent_pins.clone()));

        // A replacement keeps every pin and gives the newcomer none.
        let mut newcomer = farik_core::team::fixtures::an_agent_wire("noor", "software_developer");
        newcomer["skills"] = json!([{ "name": "forged", "sha256": "0".repeat(64) }]);
        call(
            &harness.daemon,
            "agent.replace",
            &json!({ "agent_id": "dev-b", "newcomer": newcomer }),
            "emptyResult",
        );
        assert_eq!(pins(&harness), (team_pins, agent_pins));
        let team = team_file(&harness);
        assert_eq!(team["agents"][4]["id"], "noor");
        assert_eq!(team["agents"][4].get("skills"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_the_designer_with_its_connector() {
        use crate::preview::NoPreviews;
        use crate::preview::fixtures::FakePreviews;

        let proposed_with = |name: &str, previews: Arc<dyn crate::preview::PreviewFactory>| {
            let mut harness = Harness::new(name, |_| {});
            harness.previews = previews;
            // The orchestrator tells the governor's door what runs previews, as `farik serve` does.
            let _ = harness.orchestrator(harness.recorded(Vec::new()));
            query(
                &harness.daemon,
                "team.propose",
                &json!({}),
                "teamProposeResult",
            )
        };
        let connectors = |proposed: &Value| -> Vec<(String, Value)> {
            proposed["team"]["agents"]
                .as_array()
                .expect("agents")
                .iter()
                .filter_map(|agent| {
                    agent.get("mcp_servers").map(|servers| {
                        (
                            agent["id"].as_str().unwrap_or_default().to_string(),
                            servers.clone(),
                        )
                    })
                })
                .collect()
        };
        let playwright = json!([{ "name": "playwright", "source": "builtin" }]);

        let ticked = proposed_with("team-propose-browser", Arc::new(FakePreviews::ready()));
        assert_eq!(
            connectors(&ticked),
            [("iris".to_string(), playwright.clone())]
        );
        assert_eq!(ticked["unavailable"], json!([]));

        let unticked = proposed_with("team-propose-no-sandbox", Arc::new(NoPreviews));
        assert_eq!(connectors(&unticked), [("iris".to_string(), playwright)]);
        assert_eq!(
            unticked["unavailable"],
            json!([{ "agent_id": "iris", "reason": "designer_needs_sandbox" }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_the_team_while_docker_has_not_answered() {
        use crate::preview::fixtures::{HeldPreviews, at_once, settled};
        use crate::preview::{AVAILABLE_FOR, PolledPreviews};

        // Setup is not held up behind `docker info`, which the driver asks off the request path.
        let docker = Arc::new(HeldPreviews::held(true));
        let mut harness = Harness::new("team-propose-docker-asked", |_| {});
        harness.previews = Arc::new(PolledPreviews::new(Arc::clone(&docker) as _, AVAILABLE_FOR));
        let _ = harness.orchestrator(harness.recorded(Vec::new()));
        let proposed = at_once(&docker, || {
            query(
                &harness.daemon,
                "team.propose",
                &json!({}),
                "teamProposeResult",
            )
        });
        // Until Docker answers, the Designer is shown as it is without Docker (D3).
        assert_eq!(
            proposed["unavailable"],
            json!([{ "agent_id": "iris", "reason": "designer_needs_sandbox" }])
        );

        docker.release();
        settled(&harness.previews);
        let proposed = query(
            &harness.daemon,
            "team.propose",
            &json!({}),
            "teamProposeResult",
        );
        assert_eq!(proposed["unavailable"], json!([]));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_whether_sessions_run_in_the_sandbox() {
        // Without the sandbox an agent's command can reach a connector's keys, and the add page
        // must not promise otherwise (re-review N5). The project's sandbox setting says it: asking
        // Docker per request held up every other request of the page behind `docker info`, which
        // `UnaskedPreviews` fails.
        use crate::preview::fixtures::UnaskedPreviews;
        use farik_store::files::Sandbox;

        for (name, setting, sandboxed) in [
            ("team-get-sandboxed", Some(Sandbox::Docker), true),
            ("team-get-no-sandbox", Some(Sandbox::None), false),
            ("team-get-not-told", None, false),
        ] {
            let mut harness = Harness::new(name, |_| {});
            harness.previews = Arc::new(UnaskedPreviews);
            let _ = harness.orchestrator(harness.recorded(Vec::new()));
            if let Some(setting) = setting {
                harness.project.deps.transitions.set_sandbox(setting);
            }
            let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
            assert_eq!(got["sandboxed"], json!(sandboxed), "{name}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_the_suggested_six() {
        let harness = driven("team-propose");
        let proposed = query(
            &harness.daemon,
            "team.propose",
            &json!({}),
            "teamProposeResult",
        );
        let agents: Vec<Value> = proposed["team"]["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|agent| {
                json!([
                    agent["id"],
                    agent["display_name"],
                    agent["role"],
                    agent["avatar"],
                    agent["model"]["id"],
                    agent["model"]["effort"],
                    agent["persona"],
                    agent["status"]
                ])
            })
            .collect();
        assert_eq!(
            agents,
            [
                json!([
                    "mira",
                    "Mira",
                    "product_manager",
                    "product-manager",
                    "claude-opus-5-5",
                    "high",
                    "Asks the questions that decide what to build",
                    "active"
                ]),
                json!([
                    "sol",
                    "Sol",
                    "scrum_master",
                    "scrum-master",
                    "claude-sonnet-5-5",
                    "medium",
                    "Keeps the work moving and nobody stuck",
                    "active"
                ]),
                json!([
                    "ada",
                    "Ada",
                    "architect",
                    "architect",
                    "claude-opus-5-5",
                    "high",
                    "Thinks about how it all fits together",
                    "active"
                ]),
                json!([
                    "theo",
                    "Theo",
                    "software_developer",
                    "developer",
                    "claude-opus-5-5",
                    "high",
                    "Builds it and tests it",
                    "active"
                ]),
                json!([
                    "iris",
                    "Iris",
                    "ui_ux_designer",
                    "extra-1",
                    "claude-opus-5-5",
                    "high",
                    "Makes it clear, calm and easy to use",
                    "active"
                ]),
                json!([
                    "kai",
                    "Kai",
                    "marketing_specialist",
                    "marketing-specialist",
                    "claude-sonnet-5-5",
                    "medium",
                    "Owns your brand and how you reach people",
                    "active"
                ]),
            ]
        );
        // The rest is the team as it is, and the checks the form edits beside it.
        let mut team = team_file(&harness);
        assert_eq!(proposed["team"]["name"], team["name"]);
        // Planning in sprints, as every new team does (ADR 0028).
        team["policy"]["plan_in_sprints"] = json!(true);
        assert_eq!(proposed["team"]["policy"], team["policy"]);
        farik_core::team::validate_team(&proposed["team"]).expect("a team");
        assert_eq!(
            proposed["criteria"],
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON")
        );
    }

    /// Whether `act` wakes an orchestrator of `harness` waiting for an hour, within 5 s, and
    /// what `act` answered.
    pub(crate) async fn wakes<T>(
        harness: &Harness,
        act: impl std::future::Future<Output = T>,
    ) -> (bool, T) {
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let (ended, answered) = tokio::join!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                orchestrator.wait_until(at() + chrono::Duration::hours(1)),
            ),
            async {
                tokio::task::yield_now().await;
                act.await
            }
        );
        (ended == Ok(crate::orchestrator::Waited::Woken), answered)
    }

    /// Switching "Plan work in sprints" off frees the Backlog's work at once, not at the next
    /// minute's look (the step 15 journey).
    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn wakes_the_team_when_the_team_is_saved() {
        let harness = Harness::new("team-save-wakes", |_| {});
        let team = team_file(&harness);

        let (woken, saved) = wakes(
            &harness,
            super::call(&harness.daemon, "team.save", &json!({ "team": team })),
        )
        .await;

        assert!(saved.is_ok(), "the team is saved");
        assert!(woken, "the wait ends");
    }

    /// A new Developer in a retired one's place can take ready work at once.
    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn wakes_the_team_when_an_agent_is_replaced() {
        let harness = Harness::new("team-replace-wakes", |_| {});
        let newcomer = json!({
            "id": "noor", "display_name": "Noor", "role": "software_developer",
            "avatar": "extra-1", "status": "active",
        });

        let (woken, replaced) = wakes(
            &harness,
            super::call(
                &harness.daemon,
                "agent.replace",
                &json!({ "agent_id": "dev-a", "newcomer": newcomer }),
            ),
        )
        .await;

        assert!(replaced.is_ok(), "{replaced:?}");
        assert!(woken, "the wait ends");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn validates_with_effects_and_saves() {
        let harness = driven("team-validate");
        let before = team_file(&harness);
        let mut changed = before.clone();
        changed["agents"][1]["model"] = json!({ "id": "claude-opus-5", "effort": "low" });
        let checked = query(
            &harness.daemon,
            "team.validate",
            &json!({ "team": changed }),
            "teamValidateResult",
        );
        assert_eq!(
            json!({ "errors": checked["errors"], "effects": checked["effects"] }),
            json!({
                "errors": [],
                "effects": [
                    "dev-a's model changes from the role's model to the strongest model.",
                    "dev-a now works quickly.",
                ],
            })
        );
        let mut without_pm = before.clone();
        without_pm["agents"][0]["role"] = json!("architect");
        let wrong = query(
            &harness.daemon,
            "team.validate",
            &json!({ "team": without_pm }),
            "teamValidateResult",
        );
        assert_eq!(wrong["effects"], json!([]));
        assert_eq!(wrong["errors"][0]["path"], "/agents");
        assert!(
            wrong["errors"][0]["message"].as_str().is_some_and(
                |message| message.starts_with("A team needs an active Product Manager")
            ),
            "{wrong}"
        );
        assert_eq!(team_file(&harness), before, "validating writes nothing");
        assert!(harness.project.events(&[]).is_empty());

        // Saving refuses what validating refused, with the errors to show at their rows.
        let reply = rpc(&harness.daemon, "team.save", &json!({ "team": without_pm }));
        assert_eq!(reply["error"]["code"], -32005, "{reply}");
        assert_eq!(reply["error"]["data"]["errors"], wrong["errors"]);
        assert_eq!(team_file(&harness), before);

        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": changed }),
            "emptyResult",
        );
        assert_eq!(team_file(&harness)["agents"][1]["model"]["effort"], "low");
        let updated = harness.project.events(&[EventKind::TeamUpdated]);
        assert_eq!(updated.len(), 1);
        assert_eq!(
            farik_protocol::event::event_to_value(&updated[0])["body"],
            json!({
                "team_name": "Farik",
                "agent_ids": ["pm", "dev-a", "dev-b"],
                "updated_by": "human",
                "plan_in_sprints": false
            })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_what_each_agent_may_do_and_who_checks_plans() {
        let harness = driven("team-effective");
        let mut team = team_file(&harness);
        team["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        team["agents"][1]["revokes"] = json!(["git_local"]);
        team["agents"][2]["model"] = json!({ "id": "claude-sonnet-5", "effort": "low" });
        let mut paused =
            json!({ "id": "ada", "display_name": "Ada", "role": "architect", "status": "paused" });
        team["agents"]
            .as_array_mut()
            .expect("agents")
            .push(paused.clone());
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": team }),
            "emptyResult",
        );

        let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
        // The most agents a team may have is core's, so the page never keeps its own copy.
        assert_eq!(got["max_agents"], json!(7), "{got}");
        assert_eq!(
            got["agents"],
            json!([
                {
                    "id": "pm",
                    "model": { "id": "claude-opus-5-5", "label": "Strongest model, thinks hard", "effort": "high" },
                    "tiers": ["read", "network"],
                    "base_tiers": ["read", "network"],
                },
                {
                    "id": "dev-a",
                    "model": { "id": "claude-opus-5-5", "label": "Strongest model, thinks hard", "effort": "high" },
                    "tiers": ["read", "write_workspace", "git_remote"],
                    "base_tiers": ["read", "write_workspace", "git_local", "git_remote"],
                },
                {
                    "id": "dev-b",
                    "model": { "id": "claude-sonnet-5", "label": "Everyday model (older)", "effort": "low" },
                    "tiers": ["read", "write_workspace", "git_local", "git_remote"],
                    "base_tiers": ["read", "write_workspace", "git_local", "git_remote"],
                },
                {
                    "id": "ada",
                    "model": { "id": "claude-opus-5-5", "label": "Strongest model, thinks hard", "effort": "high" },
                    "tiers": ["read", "write_workspace", "network", "git_local"],
                    "base_tiers": ["read", "write_workspace", "network", "git_local"],
                },
            ])
        );
        // A paused Architect checks nothing: Farik's choice is the Product Manager.
        let pm = json!({ "agent_id": "pm", "display_name": "pm", "role": "product_manager" });
        assert_eq!(
            got["judges"],
            json!({ "auto": pm, "architect": null, "scrum_master": null })
        );

        // A draft is answered the same way, before it is saved.
        paused["id"] = json!("ivo");
        paused["display_name"] = json!("Ivo");
        paused["status"] = json!("active");
        team["agents"].as_array_mut().expect("agents").push(paused);
        let checked = query(
            &harness.daemon,
            "team.validate",
            &json!({ "team": team }),
            "teamValidateResult",
        );
        let ivo = json!({ "agent_id": "ivo", "display_name": "Ivo", "role": "architect" });
        assert_eq!(
            checked["judges"],
            json!({ "auto": ivo, "architect": ivo, "scrum_master": null })
        );
        assert_eq!(
            checked["agents"][4]["tiers"],
            json!(["read", "write_workspace", "network", "git_local"])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn names_each_refusal_by_a_code_the_page_words() {
        let harness = driven("team-codes");
        worked(&harness, "dev-b");
        let before = team_file(&harness);
        let codes = |change: &dyn Fn(&mut Value)| {
            let mut team = before.clone();
            change(&mut team);
            let checked = query(
                &harness.daemon,
                "team.validate",
                &json!({ "team": team }),
                "teamValidateResult",
            );
            checked["errors"]
                .as_array()
                .expect("errors")
                .iter()
                .map(|error| {
                    format!(
                        "{} {}",
                        error["path"].as_str().unwrap_or_default(),
                        error["code"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
        };
        let agent = |id: &str, role: &str| json!({ "id": id, "display_name": id, "role": role, "status": "active" });
        assert_eq!(
            codes(&|team| team["agents"][0]["role"] = json!("architect")),
            ["/agents needs_product_manager"]
        );
        assert_eq!(
            codes(&|team| {
                team["agents"][1]["role"] = json!("architect");
                team["agents"][2]["role"] = json!("architect");
            }),
            ["/agents needs_developer"]
        );
        assert_eq!(
            codes(&|team| {
                let agents = team["agents"].as_array_mut().expect("agents");
                agents.extend((0..5).map(|n| agent(&format!("x-{n}"), "architect")));
            }),
            ["/agents too_many"]
        );
        assert_eq!(
            codes(&|team| team["agents"][1]["id"] = json!("pm")),
            ["/agents repeated_id"]
        );
        assert_eq!(
            codes(&|team| team["agents"][1]["display_name"] = json!("")),
            ["/agents/1/display_name name"]
        );
        assert_eq!(
            codes(&|team| team["agents"][1]["revokes"] = json!(["read"])),
            ["/agents/1/revokes keeps_read"]
        );
        assert_eq!(
            codes(&|team| team["agents"][1]["status"] = json!("paused")),
            ["/agents/1/status status_from_card"]
        );
        assert_eq!(
            codes(&|team| {
                team["agents"].as_array_mut().expect("agents").remove(2);
            }),
            ["/agents worked"]
        );
        assert_eq!(
            codes(&|team| team["policy"]["judgment"] = json!({ "judge": "architect" })),
            ["/policy/judgment/judge judge_not_held"]
        );
        assert_eq!(
            codes(&|team| team["policy"]["judgment"] =
                json!({ "required": "always", "questions": [] })),
            ["/policy/judgment/questions no_questions"]
        );
        assert_eq!(
            codes(&|team| team["policy"]["judgment"] = json!({ "questions": ["short"] })),
            ["/policy/judgment/questions/0 question_length"]
        );
        assert_eq!(
            codes(&|team| team["policy"]["integration"] = json!("mail")),
            ["/policy/integration invalid"]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_status_changes_and_removing_a_worked_agent() {
        let harness = driven("team-refuse");
        let before = team_file(&harness);
        let mut paused = before.clone();
        paused["agents"][1]["status"] = json!("paused");
        assert_eq!(
            refused(&harness, "team.save", &json!({ "team": paused })),
            (
                -32005,
                "pause, retire or resume an agent from its card".to_string()
            )
        );
        worked(&harness, "dev-b");
        let mut without = before.clone();
        without["agents"].as_array_mut().expect("agents").remove(2);
        assert_eq!(
            refused(&harness, "team.save", &json!({ "team": without })),
            (-32005, "dev-b has done work; retire it instead".to_string())
        );
        // Validating says so first, so the page never offers a save that is then refused.
        let checked = query(
            &harness.daemon,
            "team.validate",
            &json!({ "team": without }),
            "teamValidateResult",
        );
        assert_eq!(
            checked["errors"][0]["message"],
            "dev-b has done work; retire it instead"
        );
        assert_eq!(team_file(&harness), before);
        // An agent that did nothing yet may go.
        let mut without_a = before.clone();
        without_a["agents"]
            .as_array_mut()
            .expect("agents")
            .remove(1);
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": without_a }),
            "emptyResult",
        );
        assert_eq!(
            team_file(&harness)["agents"].as_array().map(Vec::len),
            Some(2)
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_removing_an_agent_that_holds_a_task() {
        // Assigned work but no event of its own yet: removed, it would leave FRK-1 with nobody.
        let harness = driven("team-refuse-assigned");
        harness.ready("FRK-1");
        harness.project.moved(
            "FRK-1",
            "ready",
            "assigned",
            &json!({ "assignee": "dev-b", "reviewer": "dev-a" }),
        );
        let before = team_file(&harness);
        for id in ["dev-b", "dev-a"] {
            let mut without = before.clone();
            without["agents"]
                .as_array_mut()
                .expect("agents")
                .retain(|agent| agent["id"] != id);
            assert_eq!(
                refused(&harness, "team.save", &json!({ "team": without })),
                (-32005, format!("{id} has done work; retire it instead"))
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn replaces_the_only_developer_in_one_change() {
        let harness = driven("team-replace");
        let retired = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": { "command": "agent_update", "body": { "agent_id": "dev-b", "status": "retired" } } }),
        );
        assert!(retired["result"]["said"].is_string(), "{retired}");
        let before = team_file(&harness);
        let newcomer = json!({
            "id": "noor", "display_name": "Noor", "role": "software_developer",
            "avatar": "extra-1", "status": "active",
        });

        // Another role in the only Developer's place leaves the team with none: nothing changes.
        let mut wrong = newcomer.clone();
        wrong["role"] = json!("architect");
        let reply = rpc(
            &harness.daemon,
            "agent.replace",
            &json!({ "agent_id": "dev-a", "newcomer": wrong }),
        );
        assert_eq!(reply["error"]["code"], -32005, "{reply}");
        assert_eq!(reply["error"]["data"]["errors"][0]["path"], "/agents");
        assert_eq!(team_file(&harness), before);

        let seen = harness.project.events(&[]).len();
        call(
            &harness.daemon,
            "agent.replace",
            &json!({ "agent_id": "dev-a", "newcomer": newcomer }),
            "emptyResult",
        );
        let agents: Vec<Value> = team_file(&harness)["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|agent| json!([agent["id"], agent["status"]]))
            .collect();
        assert_eq!(
            agents,
            [
                json!(["pm", "active"]),
                json!(["dev-a", "retired"]),
                json!(["dev-b", "retired"]),
                json!(["noor", "active"]),
            ]
        );
        assert_eq!(
            kinds(&harness)[seen..],
            [EventKind::AgentUpdated, EventKind::TeamUpdated]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn checks_a_replacement_against_the_team_it_replaces() {
        let harness = driven("team-replace-locked");
        let writing = harness.daemon.team_writes();
        let daemon = Arc::clone(&harness.daemon);
        let asking = std::thread::spawn(move || {
            rpc(
                &daemon,
                "agent.replace",
                &json!({ "agent_id": "dev-b", "newcomer": farik_core::team::fixtures::an_agent_wire("lin", "software_developer") }),
            )
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        // While another write holds the team, it adds a Lin of its own: the newcomer's id is taken.
        let mut wire = team_file(&harness);
        wire["agents"].as_array_mut().expect("agents").push(
            farik_core::team::fixtures::an_agent_wire("lin", "architect"),
        );
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&wire).expect("a team"))
            .expect("written");
        drop(writing);
        let reply = asking.join().expect("the replace ends");
        assert_eq!(reply["error"]["code"], -32005, "{reply}");
        assert_eq!(reply["error"]["data"]["errors"][0]["path"], "/agents");
        assert_eq!(team_file(&harness), wire, "nothing is written");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn starts_the_team_from_setup() {
        let harness = driven("team-start");
        let marker = harness.project.repo.path.join(MARKER);
        std::fs::write(&marker, "").expect("the marker is written");
        let paused = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": { "command": "team_pause", "body": {} } }),
        );
        assert!(paused["result"]["said"].is_string(), "{paused}");
        // The starter team's ids have done work, which setup replaces all the same.
        worked(&harness, "pm");
        let proposed = query(
            &harness.daemon,
            "team.propose",
            &json!({}),
            "teamProposeResult",
        );
        let mut criteria = proposed["criteria"].clone();
        criteria["criteria"]
            .as_array_mut()
            .expect("criteria")
            .truncate(1);
        let start = json!({ "team": proposed["team"], "criteria": criteria });

        // Without the marker, replacing an id that worked is refused, as a save refuses it.
        std::fs::remove_file(&marker).expect("removed");
        assert_eq!(
            refused(&harness, "team.start", &start),
            (-32005, "pm has done work; retire it instead".to_string())
        );
        std::fs::write(&marker, "").expect("the marker is written");

        let seen = harness.project.events(&[]).len();
        call(&harness.daemon, "team.start", &start, "emptyResult");
        let ids: Vec<Value> = team_file(&harness)["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|agent| agent["id"].clone())
            .collect();
        assert_eq!(
            ids,
            [
                json!("mira"),
                json!("sol"),
                json!("ada"),
                json!("theo"),
                json!("iris"),
                json!("kai")
            ]
        );
        assert_eq!(
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON"),
            criteria
        );
        assert!(!marker.exists(), "the marker is removed");
        assert_eq!(
            kinds(&harness)[seen..],
            [
                EventKind::TeamUpdated,
                EventKind::CriteriaUpdated,
                EventKind::TeamResumed
            ]
        );
        assert!(!crate::pause::paused(&harness.project.deps.log).expect("reads"));

        // Without the marker it is no setup: it saves, and leaves a team paused for any other
        // reason, a budget's stop or a person's pause, paused.
        let paused = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": { "command": "team_pause", "body": {} } }),
        );
        assert!(paused["result"]["said"].is_string(), "{paused}");
        let again = json!({ "team": team_file(&harness), "criteria": criteria });
        call(&harness.daemon, "team.start", &again, "emptyResult");
        assert!(crate::pause::paused(&harness.project.deps.log).expect("reads"));
    }

    /// `team.validate`'s effects of switching the sprint policy to `on`, on a board with the ready
    /// FRK-1, FRK-2 under way since before the switch, FRK-3 under way in the open S2, and FRK-4,
    /// which S1 left for the Backlog.
    fn switching(name: &str, on: bool) -> Value {
        let harness = crate::orchestrator::fixtures::Harness::new(name, |wire| {
            wire["policy"]["plan_in_sprints"] = json!(!on);
        });
        harness.ready("FRK-1");
        harness.file("FRK-2", "in_progress", |_| {});
        harness.file("FRK-3", "in_progress", |_| {});
        harness.file("FRK-4", "assigned", |_| {});
        harness.open_sprint("S1", &["FRK-4"]);
        harness.project.record(
            "",
            "sprint.ended",
            &json!({ "sprint_id": "S1", "ended_by": "human", "left": ["FRK-4"], "backlog": true }),
        );
        harness.open_sprint("S2", &["FRK-3"]);
        let mut team = team_file(&harness);
        team["policy"]["plan_in_sprints"] = json!(on);
        query(
            &harness.daemon,
            "team.validate",
            &json!({ "team": team }),
            "teamValidateResult",
        )["effects"]
            .clone()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn describes_the_work_a_switch_touches() {
        assert_eq!(
            switching("team-switch-on", true),
            json!([
                "Ready work now waits in the Backlog until you start a sprint.",
                "The task already under way finishes first.",
            ])
        );
        assert_eq!(
            switching("team-switch-off", false),
            json!([
                "Ready work starts as soon as someone is free, without waiting for a sprint.",
                "The 2 pieces of work in the Backlog, Add a login page and Add a login page, can \
                 start now.",
                "You can still start sprints from the Board.",
            ])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_sprints_for_a_new_team() {
        let harness = driven("team-propose-sprints");
        assert_eq!(team_file(&harness)["policy"].get("plan_in_sprints"), None);
        let proposed = query(
            &harness.daemon,
            "team.propose",
            &json!({}),
            "teamProposeResult",
        );
        assert_eq!(
            proposed["team"]["policy"]["plan_in_sprints"],
            json!(true),
            "{proposed}"
        );
        let defaults = query(
            &harness.daemon,
            "settings.defaults",
            &json!({}),
            "settingsDefaultsResult",
        );
        assert_eq!(defaults["policy"]["plan_in_sprints"], json!(true));
        let start = json!({ "team": proposed["team"], "criteria": proposed["criteria"] });
        call(&harness.daemon, "team.start", &start, "emptyResult");
        assert_eq!(
            team_file(&harness)["policy"]["plan_in_sprints"],
            json!(true)
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn saves_the_checks() {
        let harness = driven("team-criteria");
        let mut library =
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON");
        library["criteria"]
            .as_array_mut()
            .expect("criteria")
            .push(json!({
                "name": "the-page-loads-fast",
                "text": "The page loads fast.",
                "source": "human",
                "verification": { "method": "review", "rubric": ["The page loads fast."] },
            }));
        call(
            &harness.daemon,
            "criteria.save",
            &json!({ "criteria": library }),
            "emptyResult",
        );
        assert_eq!(
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON"),
            library
        );
        let updated = harness.project.events(&[EventKind::CriteriaUpdated]);
        assert_eq!(updated.len(), 1);
        assert_eq!(
            farik_protocol::event::event_to_value(&updated[0])["body"]["updated_by"],
            "human"
        );
        let (code, _) = refused(
            &harness,
            "criteria.save",
            &json!({ "criteria": { "criteria": [{}] } }),
        );
        assert_eq!(code, -32005);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn disconnects_the_account_and_pauses() {
        let harness = driven("team-disconnect");
        let store = Arc::new(MemoryStore::default());
        store
            .save(&ClaudeCredential::ApiKey(Secret::new(
                "sk-ant-api-x".to_string(),
            )))
            .expect("kept");
        served(&harness, &store, &[]);
        assert_eq!(
            query(
                &harness.daemon,
                "account.status",
                &json!({}),
                "accountStatusResult"
            ),
            json!({ "provider": "anthropic", "kind": "api_key", "source": "keychain" })
        );
        assert_eq!(
            call(
                &harness.daemon,
                "account.disconnect",
                &json!({}),
                "accountDisconnectResult"
            ),
            json!({ "removed_from": ["keychain"], "paused": true })
        );
        assert!(store.load().expect("reads").is_none());
        assert_eq!(harness.project.events(&[EventKind::TeamPaused]).len(), 1);
        assert_eq!(
            query(
                &harness.daemon,
                "account.status",
                &json!({}),
                "accountStatusResult"
            ),
            json!({ "provider": null, "kind": null, "source": null })
        );

        // On a team already paused, disconnecting pauses nothing, and says so.
        store
            .save(&ClaudeCredential::ApiKey(Secret::new(
                "sk-ant-api-x".to_string(),
            )))
            .expect("kept");
        assert_eq!(
            call(
                &harness.daemon,
                "account.disconnect",
                &json!({}),
                "accountDisconnectResult"
            ),
            json!({ "removed_from": ["keychain"], "paused": false })
        );
        assert_eq!(harness.project.events(&[EventKind::TeamPaused]).len(), 1);

        // A key from the environment cannot be removed; the answer names where it is, and so
        // does the account's status, before anyone tries.
        let harness = driven("team-disconnect-env");
        let store = Arc::new(MemoryStore::default());
        served(&harness, &store, &[("ANTHROPIC_API_KEY", "sk-ant-api-y")]);
        assert_eq!(
            query(
                &harness.daemon,
                "account.status",
                &json!({}),
                "accountStatusResult"
            ),
            json!({
                "provider": "anthropic", "kind": "api_key", "source": "environment",
                "environment_variable": "ANTHROPIC_API_KEY",
            })
        );
        assert_eq!(
            call(
                &harness.daemon,
                "account.disconnect",
                &json!({}),
                "accountDisconnectResult"
            ),
            json!({ "removed_from": [], "paused": false, "environment_variable": "ANTHROPIC_API_KEY" })
        );
        assert!(harness.project.events(&[EventKind::TeamPaused]).is_empty());
    }

    /// Appends a `team.paused` with `body`, as Farik or the human would.
    fn paused_with(harness: &Harness, body: &Value) {
        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": at().to_rfc3339(), "team_id": "farik", "project_id": "farik",
            "kind": "team.paused", "body": body.clone(),
        }))
        .expect("the fixture is schema-valid");
        harness
            .project
            .deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connects_the_account_again_and_resumes_a_team_paused_for_its_key() {
        let harness = driven("team-reconnect");
        let store = Arc::new(MemoryStore::default());
        let in_use = served(&harness, &store, &[]);
        let status = || {
            query(
                &harness.daemon,
                "account.status",
                &json!({}),
                "accountStatusResult",
            )
        };
        assert_eq!(status().get("key_refused"), None);
        paused_with(
            &harness,
            &json!({ "by": "farik", "reason": "credential_refused", "detail": "401" }),
        );
        assert_eq!(status()["key_refused"], json!(true));

        let answer = call(
            &harness.daemon,
            "account.connect",
            &json!({ "kind": "subscription_token", "secret": "sk-ant-oat01-new" }),
            "accountConnectResult",
        );

        assert_eq!(
            answer,
            json!({ "stored_in": "keychain", "taking_on": false })
        );
        let new = ClaudeCredential::OauthToken(Secret::new("sk-ant-oat01-new".to_string()));
        assert_eq!(store.load().expect("reads"), Some(new.clone()));
        assert_eq!(*in_use.lock().expect("not poisoned"), new);
        // The page is told the kind the sessions now run on.
        assert_eq!(
            query(
                &harness.daemon,
                "serve.status",
                &json!({}),
                "serveStatusResult"
            )["credential"],
            json!("subscription_token")
        );
        assert!(!paused(&harness.project.deps.log).expect("reads"));
        assert_eq!(status().get("key_refused"), None);

        // A pause the human made is theirs to end: connecting again keeps it.
        paused_with(&harness, &json!({ "by": "human" }));
        call(
            &harness.daemon,
            "account.connect",
            &json!({ "kind": "subscription_token", "secret": "sk-ant-oat01-newer" }),
            "accountConnectResult",
        );
        assert!(paused(&harness.project.deps.log).expect("reads"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_a_connect_whose_resume_the_human_beat() {
        // The human's Resume lands between the connect's look at the pause and its own resume.
        let harness = Harness::new("team-reconnect-race", |_| {});
        let orchestrator = Arc::new(harness.orchestrator(harness.recorded(Vec::new())));
        let handler: crate::daemon::CommandHandler = Arc::new(move |command| {
            let orchestrator = Arc::clone(&orchestrator);
            Box::pin(async move {
                if matches!(command, farik_protocol::command::Command::TeamResume) {
                    orchestrator
                        .handle(farik_protocol::command::Command::TeamResume)
                        .await
                        .expect("the human's resume");
                }
                orchestrator.handle(command).await
            })
        });
        assert!(harness.daemon.set_command_handler(handler));
        let store = Arc::new(MemoryStore::default());
        served(&harness, &store, &[]);
        paused_with(
            &harness,
            &json!({ "by": "farik", "reason": "credential_refused", "detail": "401" }),
        );

        let reply = rpc(
            &harness.daemon,
            "account.connect",
            &json!({ "kind": "api_key", "secret": "sk-ant-api-new" }),
        );

        assert_eq!(
            reply["result"],
            json!({ "stored_in": "keychain", "taking_on": false }),
            "{reply}"
        );
        assert!(!paused(&harness.project.deps.log).expect("reads"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_connect_over_a_key_from_the_environment() {
        let harness = driven("team-reconnect-env");
        let store = Arc::new(MemoryStore::default());
        served(&harness, &store, &[("ANTHROPIC_API_KEY", "sk-ant-api-y")]);
        let reply = rpc(
            &harness.daemon,
            "account.connect",
            &json!({ "kind": "api_key", "secret": "sk-ant-api-z" }),
        );
        assert_eq!(reply["error"]["code"], json!(-32005), "{reply}");
        assert!(
            reply["error"]["message"]
                .as_str()
                .is_some_and(|said| said.contains("ANTHROPIC_API_KEY")),
            "{reply}"
        );
        assert!(store.load().expect("reads").is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_the_defaults_a_setting_is_put_back_to() {
        let harness = driven("team-defaults");
        let defaults = query(
            &harness.daemon,
            "settings.defaults",
            &json!({}),
            "settingsDefaultsResult",
        );
        assert_eq!(defaults["budgets"], json!({}), "no daily limit (ADR 0015)");
        assert_eq!(defaults["rules"], json!({}), "the rules Farik ships");
        assert_eq!(
            defaults["policy"],
            json!({
                "human_accepts_contracts": "high_risk",
                "wip_limit_per_agent": 1,
                "blocked_limit_hours": 24,
                "max_iterations": 3,
                "integration": "auto_merge",
                "ambient_messages_per_sprint": 1,
                "escalation_age_hours": 24,
                "memory_cap_tokens": 8000,
                "judgment": {
                    "required": "always",
                    "questions": [
                        "Does the task fit its budget?",
                        "Would its checks notice if the work went wrong the way its intent worries about?"
                    ],
                    "judge": "auto"
                },
                "permissions": { "run_commands": true, "push": false },
                "plan_in_sprints": true
            })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_the_models_with_labels() {
        let harness = driven("team-models");
        assert_eq!(
            query(
                &harness.daemon,
                "models.list",
                &json!({}),
                "modelsListResult"
            ),
            json!({ "models": [
                { "id": "claude-fable-5-1", "label": "Most capable model" },
                { "id": "claude-opus-5-5", "label": "Strongest model, thinks hard" },
                { "id": "claude-sonnet-5-5", "label": "Everyday model" },
                { "id": "claude-haiku-4-5", "label": "Quick model" },
            ] })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_scan_back_for_the_browser() {
        let harness = driven("project-scan");
        let root = &harness.project.repo.path;
        let write = |path: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("made");
            std::fs::write(path, "x").expect("written");
        };
        write(".env");
        // Four deep is looked at; five deep, node_modules and .git are not.
        write("a/b/c/near.pem");
        write("a/b/c/d/far.key");
        write("node_modules/x/dep.key");
        write(".git/secret.key");
        let scanned = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        let found = farik_store::scan_project(&harness.project.deps.git, at()).expect("scans");
        assert_eq!(
            scanned["facts"],
            json!({
                "language": found.facts.language,
                "toolchain": found.facts.toolchain,
                "workspace": found.facts.workspace,
                "packages": found.facts.packages,
                "tests_in": found.facts.tests_in,
                "tracked_files": found.facts.tracked_files,
                "last_commit": found.facts.last_commit,
            })
        );
        assert_eq!(
            scanned["checks"],
            json!(
                found
                    .detected_criteria
                    .iter()
                    .map(|one| one.text.to_string())
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(scanned["kept_private"], json!([".env", "**/*.pem"]));

        // The walk stops after 2000 entries: a key file met after them is not seen.
        for n in 0..2000 {
            write(&format!("many/f{n:04}"));
        }
        write("zz.key");
        let capped = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        assert_eq!(capped["kept_private"], json!([".env", "**/*.pem"]));
        std::fs::remove_dir_all(root.join("many")).expect("removed");
        let uncapped = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        assert_eq!(
            uncapped["kept_private"],
            json!([".env", "**/*.pem", "**/*.key"])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaves_farik_own_folder_out_of_kept_private() {
        let harness = driven("project-scan-own");
        let local = harness.project.repo.path.join(".farik/local/x");
        std::fs::create_dir_all(local.parent().expect("a parent")).expect("made");
        std::fs::write(local, "x").expect("written");
        let scanned = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        assert!(
            !scanned["kept_private"]
                .as_array()
                .expect("a list")
                .contains(&json!(".farik/local/**")),
            "{}",
            scanned["kept_private"]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn does_not_follow_a_link_out_of_the_project() {
        let harness = driven("project-scan-link");
        let root = &harness.project.repo.path;
        let outside = root.with_extension("outside");
        std::fs::create_dir_all(&outside).expect("made");
        std::fs::write(outside.join("x.key"), "x").expect("written");
        std::os::unix::fs::symlink(&outside, root.join("elsewhere")).expect("linked");
        let scanned = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        std::fs::remove_dir_all(&outside).expect("removed");
        assert!(
            !scanned["kept_private"]
                .as_array()
                .expect("a list")
                .contains(&json!("**/*.key")),
            "{}",
            scanned["kept_private"]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_the_user_note_from_the_browser() {
        let harness = driven("project-note");
        call(
            &harness.daemon,
            "project.note",
            &json!({ "text": "It is a shop, not a game." }),
            "emptyResult",
        );
        let document = harness
            .project
            .deps
            .files
            .read_project_scan()
            .expect("project.md reads");
        assert!(
            document.contains("## The user says\n\n2026-09-22: It is a shop, not a game.\n"),
            "{document}"
        );
        // The words are kept without the space around them.
        call(
            &harness.daemon,
            "project.note",
            &json!({ "text": "  It sells bread.\n " }),
            "emptyResult",
        );
        let document = harness
            .project
            .deps
            .files
            .read_project_scan()
            .expect("project.md reads");
        assert!(
            document.ends_with("\n2026-09-22: It sells bread.\n"),
            "{document}"
        );
        for text in [String::new(), "   ".to_string(), "x".repeat(2001)] {
            let (code, _) = refused(&harness, "project.note", &json!({ "text": text }));
            assert_eq!(code, -32602, "{text:?}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reports_setup_pending() {
        let harness = driven("team-pending");
        served(&harness, &Arc::new(MemoryStore::default()), &[]);
        let pending = || {
            query(
                &harness.daemon,
                "serve.status",
                &json!({}),
                "serveStatusResult",
            )["setup_pending"]
                .clone()
        };
        assert_eq!(pending(), json!(false));
        std::fs::write(harness.project.repo.path.join(MARKER), "").expect("the marker is written");
        assert_eq!(pending(), json!(true));
    }

    /// The value of the fixture server's key: no reply, event or file may hold it.
    const KEY: &str = "fixture-key-never-echoed";

    /// The stdio MCP server of `tests/fixtures/mcp_server.sh`, written for `test`, as
    /// `connector.connect` takes it: its tools are `search`, `env`, `delete_repo`, and
    /// `repo.delete`, a name Farik can't use.
    fn fixture_server(test: &str) -> Value {
        let dir = std::env::temp_dir().join(format!(
            "farik-team-connector-{}-{test}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        let path = dir.join("server.sh");
        std::fs::write(&path, include_str!("../../tests/fixtures/mcp_server.sh"))
            .expect("the script is written");
        json!({
            "name": "fixture", "transport": "stdio", "command": "sh",
            "args": [path.display().to_string()], "credential_keys": ["API_KEY"],
        })
    }

    /// A driven daemon keeping its agents' connector keys in memory.
    fn keeping(name: &str) -> (Harness, Arc<MemoryConnectorSecrets>) {
        let harness = driven(name);
        let store = Arc::new(MemoryConnectorSecrets::default());
        assert!(
            harness
                .daemon
                .set_connector_secrets(Arc::clone(&store) as _)
        );
        (harness, store)
    }

    fn kept_at(harness: &Harness, agent: &str, server: &str) -> SecretAt {
        harness
            .daemon
            .secret_at(harness.project.deps.files.root(), agent, server)
            .expect("an address")
    }

    fn connect_params(agent: &str, server: &Value, tags: &Value) -> Value {
        json!({ "agent": agent, "server": server, "keys": { "API_KEY": KEY }, "tags": tags })
    }

    /// `server` connected to `agent` through `connector.connect`, labelled by `tags`.
    fn connected(harness: &Harness, agent: &str, server: &Value, tags: &Value) -> Value {
        call(
            &harness.daemon,
            "connector.connect",
            &connect_params(agent, server, tags),
            "connectorConnectResult",
        )
    }

    /// `agent`'s entry named `name` in the team file.
    fn entry(harness: &Harness, agent: usize, name: &str) -> Option<Value> {
        team_file(harness)["agents"][agent]["mcp_servers"]
            .as_array()
            .and_then(|servers| servers.iter().find(|server| server["name"] == name))
            .cloned()
    }

    /// The custom server `entry` describes.
    fn custom(entry: &Value) -> farik_core::team::CustomServer {
        farik_core::team::custom_server(
            &serde_json::from_value(entry.clone()).expect("an mcp_servers entry"),
        )
        .expect("a custom server")
    }

    /// The whole log as text.
    fn log_text(harness: &Harness) -> String {
        harness
            .project
            .events(&[])
            .iter()
            .map(|event| event_to_value(event).to_string())
            .collect()
    }

    /// Each body of `kind` in the log, as JSON.
    fn bodies(harness: &Harness, kind: EventKind) -> Vec<Value> {
        harness
            .project
            .events(&[kind])
            .iter()
            .map(|event| event_to_value(event)["body"].clone())
            .collect()
    }

    fn states(harness: &Harness) -> Value {
        query(&harness.daemon, "team.get", &json!({}), "teamGetResult")["connectors"].clone()
    }

    /// The Developer's kit with one service, `fixture`: the fixture server, with its copy and its
    /// tags (`search` network, `env` external effect, `delete_repo` denied).
    pub(crate) fn fixture_kit(test: &str) -> farik_roles::Kit {
        fixture_kit_with(test, &json!({}), &json!({}))
    }

    /// [`fixture_kit`] with the tools `more` tagged too, and the kit's `allowances`.
    pub(crate) fn fixture_kit_with(
        test: &str,
        more: &Value,
        allowances: &Value,
    ) -> farik_roles::Kit {
        let mut connector = fixture_server(test);
        connector["title"] = json!("Fixture");
        connector["about"] = json!("A server that stands in for a service.");
        connector["why"] = json!("Lets the Developer search the fixture.");
        connector["setup"] = json!("Make a key on the fixture's page and paste it.");
        connector["key_page"] = json!("https://fixture.example/keys");
        connector["labels"] = json!({ "search": "search the fixture" });
        connector["tools"] =
            json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" });
        for (tool, tag) in more.as_object().into_iter().flatten() {
            connector["tools"][tool] = tag.clone();
        }
        if allowances
            .as_object()
            .is_some_and(|given| !given.is_empty())
        {
            connector["allowances"] = allowances.clone();
        }
        let kit = json!({ "role": "software_developer", "skills": [], "connectors": [connector] });
        farik_roles::parse_fixture_kit(
            farik_core::contract::Role::SoftwareDeveloper,
            &kit.to_string(),
            &[],
            &[],
        )
        .expect("the fixture kit loads")
    }

    fn kit_server() -> Value {
        json!({ "name": "fixture", "source": "kit" })
    }

    fn kit_params(agent: &str, tags: &Value) -> Value {
        connect_params(agent, &kit_server(), tags)
    }

    const KIT_TAGS: fn() -> Value =
        || json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" });

    /// The fixture kit with a spending tool `make`, which may run 20 times a sprint unasked, and
    /// `post`, which publishes and so always asks.
    fn allowance_kit(test: &str) -> farik_roles::Kit {
        fixture_kit_with(
            test,
            &json!({ "make": "external_effect", "post": "external_effect" }),
            &json!({ "make": { "calls": 20, "what": "pictures" } }),
        )
    }

    fn keeping_an_allowance_kit(name: &str) -> (Harness, Arc<MemoryConnectorSecrets>) {
        let (harness, store) = keeping(name);
        harness.project.set_kit(allowance_kit(name));
        (harness, store)
    }

    fn allowance_params(agent: &str, allowances: &Value) -> Value {
        let mut params = kit_params(agent, &json!({}));
        params["allowances"] = allowances.clone();
        params
    }

    /// A driven daemon keeping connector keys in memory, whose Developer kit is the fixture's.
    fn keeping_a_kit(name: &str) -> (Harness, Arc<MemoryConnectorSecrets>) {
        let (harness, store) = keeping(name);
        harness.project.set_kit(fixture_kit(name));
        (harness, store)
    }

    #[test]
    fn kit_entry_writes_the_kits_tags() {
        let team = crate::tools::fixtures::a_team_of_three(|_| {});
        let (entry, server) = super::kit_entry(
            &fixture_kit("entry"),
            &team,
            "dev-a",
            "fixture",
            &BTreeMap::new(),
        )
        .expect("an entry");
        assert_eq!(entry["source"], "kit");
        assert_eq!(entry["command"], "sh");
        assert_eq!(entry["credential_keys"], json!(["API_KEY"]));
        assert_eq!(entry["tools"], KIT_TAGS());
        assert!(server.kit);
        assert_eq!(server.tools.len(), 3);
    }

    #[test]
    fn kit_entry_refuses_another_roles_connector() {
        let team = crate::tools::fixtures::a_team_of_three(|_| {});
        let kit = fixture_kit("another-role");
        let refusal = |agent: &str, name: &str| {
            super::kit_entry(&kit, &team, agent, name, &BTreeMap::new())
                .expect_err("refused")
                .into_iter()
                .map(|error| (error.path, error.message))
                .collect::<Vec<_>>()
        };
        // The Product Manager's role is not the kit's.
        let pm = refusal("pm", "fixture");
        assert_eq!(pm[0].0, "/agents/0/mcp_servers");
        assert!(pm[0].1.starts_with("connector_not_in_kit: "), "{pm:?}");
        // A name the kit lacks.
        let lacks = refusal("dev-a", "other");
        assert_eq!(lacks[0].0, "/agents/1/mcp_servers");
        assert!(
            lacks[0].1.starts_with("connector_not_in_kit: "),
            "{lacks:?}"
        );
        // An agent the team lacks.
        assert_eq!(refusal("nobody", "fixture")[0].0, "/agents");
        // The Designer's container connector is no service to connect by name.
        let team = crate::tools::fixtures::a_team_of_three(crate::tools::fixtures::browsing);
        let designer = farik_roles::load_kit(farik_core::contract::Role::UiUxDesigner)
            .expect("the Designer's kit");
        let container = super::kit_entry(&designer, &team, "iris", "playwright", &BTreeMap::new())
            .expect_err("a container is not connected by name");
        assert_eq!(container[0].path, "/agents/3/mcp_servers");
        assert!(container[0].message.starts_with("connector_not_in_kit: "));
    }

    /// A guard: each shipped service of the Product Manager, the Architect and the Developer connects
    /// by name, GitHub being each of the first two's own entry (step 07c).
    #[test]
    fn connects_every_shipped_kit_connector_by_name() {
        use farik_core::contract::Role;
        let team = crate::tools::fixtures::a_team_of_three(|wire| {
            let agents = wire["agents"].as_array_mut().expect("agents");
            agents.push(farik_core::team::fixtures::an_agent_wire(
                "sam",
                "scrum_master",
            ));
            agents.push(farik_core::team::fixtures::an_agent_wire(
                "archie",
                "architect",
            ));
        });
        let pm = farik_roles::load_kit(Role::ProductManager).expect("the Product Manager's kit");
        for name in ["amplitude", "linear", "notion", "github"] {
            let (_, server) = super::kit_entry(&pm, &team, "pm", name, &BTreeMap::new())
                .unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
            assert!(super::matches_kit(&pm, &server), "{name}");
        }
        let scrum = farik_roles::load_kit(Role::ScrumMaster).expect("the Scrum Master's kit");
        let refused = super::kit_entry(&scrum, &team, "sam", "notion", &BTreeMap::new())
            .expect_err("the Scrum Master has no Notion");
        assert!(refused[0].message.starts_with("connector_not_in_kit: "));
        let architect = farik_roles::load_kit(Role::Architect).expect("the Architect's kit");
        for name in ["context7", "grep", "osv", "github"] {
            let (_, server) = super::kit_entry(&architect, &team, "archie", name, &BTreeMap::new())
                .unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
            assert!(super::matches_kit(&architect, &server), "{name}");
        }
        // The two roles' GitHub entries share a name and a key and are not each other's: a team
        // file that gives one role the other's entry runs nothing.
        let (_, pm_github) = super::kit_entry(&pm, &team, "pm", "github", &BTreeMap::new())
            .expect("the Product Manager's GitHub");
        let (_, architect_github) =
            super::kit_entry(&architect, &team, "archie", "github", &BTreeMap::new())
                .expect("the Architect's GitHub");
        assert!(!super::matches_kit(&architect, &pm_github));
        assert!(!super::matches_kit(&pm, &architect_github));
        let refused = super::kit_entry(&pm, &team, "pm", "osv", &BTreeMap::new())
            .expect_err("the Product Manager has no OSV");
        assert!(refused[0].message.starts_with("connector_not_in_kit: "));
        let developer =
            farik_roles::load_kit(Role::SoftwareDeveloper).expect("the Developer's kit");
        let (_, server) =
            super::kit_entry(&developer, &team, "dev-a", "context7", &BTreeMap::new())
                .unwrap_or_else(|refused| panic!("context7: {refused:?}"));
        assert!(super::matches_kit(&developer, &server), "context7");
        for name in ["osv", "github"] {
            let refused = super::kit_entry(&developer, &team, "dev-a", name, &BTreeMap::new())
                .expect_err("the Developer has neither OSV nor GitHub");
            assert!(
                refused[0].message.starts_with("connector_not_in_kit: "),
                "{name}: {refused:?}"
            );
        }
    }

    /// The kit's entry named `name`, as the daemon reads it.
    fn kit_entry_of(kit: &farik_roles::Kit, name: &str) -> farik_core::team::McpServerWire {
        kit.connectors
            .iter()
            .find_map(|connector| match connector {
                farik_roles::KitConnector::Server { entry, .. } if entry.name.as_str() == name => {
                    Some(entry.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("the kit has no {name}"))
    }

    /// An entry that signs in for one of Farik's own connectors, as a literal: nothing here is an
    /// app of Farik's.
    fn app_for(connector: &'static str) -> crate::registered_apps::RegisteredApp {
        use crate::registered_apps::{AppFlow, RegisteredApp};
        RegisteredApp {
            id: "a-test-app",
            name: "A test app",
            host: None,
            farik_connector: Some(connector),
            flow: AppFlow::Loopback {
                authorization_endpoint: "https://accounts.example/auth",
            },
            client_id: "a-test-client",
            client_secret: None,
            scopes: &["a-scope"],
            issuer: "https://accounts.example",
            token_endpoint: "https://accounts.example/token",
            revocation_endpoint: None,
            install_url: None,
            settings_url: "https://accounts.example/settings",
        }
    }

    /// Until Farik Cloud signs customers in (ADR 0044), a kit's service that signs in through one
    /// of Farik's own connectors comes at the launch when the table has no app for that connector,
    /// and no other service does.
    #[test]
    fn a_kit_row_without_an_app_comes_at_launch() {
        use farik_core::contract::Role;
        let marketing = farik_roles::load_kit(Role::MarketingSpecialist)
            .expect("the Marketing Specialist's kit");
        let ads = kit_entry_of(&marketing, "google-ads");
        assert!(
            super::at_launch(&ads, &[]),
            "no app: it comes at the launch"
        );
        assert!(
            !super::at_launch(&ads, &[app_for("google-ads")]),
            "an app for it: Connect works"
        );
        assert!(
            super::at_launch(&ads, &[app_for("osv")]),
            "an app for another of Farik's connectors is no app for this one"
        );
        // A service signed in to by route 1 has nothing to wait for, and neither has a Farik
        // connector that signs in to nothing.
        for name in ["higgsfield", "recraft", "buffer", "kit"] {
            assert!(
                !super::at_launch(&kit_entry_of(&marketing, name), &[]),
                "{name}"
            );
        }
        let architect = farik_roles::load_kit(Role::Architect).expect("the Architect's kit");
        assert!(!super::at_launch(&kit_entry_of(&architect, "osv"), &[]));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_which_kit_row_comes_at_launch() {
        let (harness, _) = keeping_a_kit("kit-at-launch");
        let mut team = team_file(&harness);
        team["agents"].as_array_mut().expect("agents").push(
            farik_core::team::fixtures::an_agent_wire("kai", "marketing_specialist"),
        );
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&team).expect("a team"))
            .expect("the team is written");
        let rows = |harness: &Harness| -> Vec<Value> {
            let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
            got["kits"]
                .as_array()
                .and_then(|kits| {
                    kits.iter()
                        .find(|kit| kit["role"] == "marketing_specialist")
                })
                .and_then(|kit| kit["connectors"].as_array().cloned())
                .expect("the Marketing Specialist's kit")
        };
        let row = |rows: &[Value], name: &str| -> Value {
            rows.iter()
                .find(|row| row["name"] == name)
                .cloned()
                .unwrap_or_else(|| panic!("no {name}"))
        };
        // No build carries an app of Farik's: Google Ads waits for the launch, the others do not.
        let before = rows(&harness);
        assert_eq!(row(&before, "google-ads")["at_launch"], json!(true));
        assert_eq!(row(&before, "google-ads")["auth"], "oauth");
        for name in ["higgsfield", "recraft", "buffer", "kit"] {
            assert!(row(&before, name).get("at_launch").is_none(), "{name}");
        }
        // With an app for it, Connect works.
        let table: &'static [crate::registered_apps::RegisteredApp] =
            Box::leak(Box::new([app_for("google-ads")]));
        assert!(harness.daemon.set_registered_apps(table));
        assert!(
            row(&rows(&harness), "google-ads")
                .get("at_launch")
                .is_none()
        );
    }

    /// A guard: each of the Marketing Specialist's five services connects by name, carrying the
    /// kit's allowances (Buffer and Google Ads none, Kit its two broadcast tools), and is no other
    /// role's.
    #[test]
    fn connects_each_marketing_service_by_name() {
        use farik_core::contract::Role;
        let team = crate::tools::fixtures::a_team_of_three(|wire| {
            wire["agents"].as_array_mut().expect("agents").push(
                farik_core::team::fixtures::an_agent_wire("kai", "marketing_specialist"),
            );
        });
        let marketing = farik_roles::load_kit(Role::MarketingSpecialist)
            .expect("the Marketing Specialist's kit");
        for (name, allowed) in [
            (
                "higgsfield",
                BTreeMap::from([
                    ("generate_image".to_string(), 20),
                    ("generate_video".to_string(), 6),
                    ("upscale_image".to_string(), 10),
                    ("remove_background".to_string(), 10),
                    ("outpaint_image".to_string(), 10),
                ]),
            ),
            (
                "recraft",
                BTreeMap::from([
                    ("generate_image".to_string(), 20),
                    ("image_to_image".to_string(), 10),
                    ("vectorize_image".to_string(), 10),
                    ("remove_background".to_string(), 10),
                    ("replace_background".to_string(), 10),
                    ("crisp_upscale".to_string(), 10),
                ]),
            ),
            // Every post asks: `kit_entry` adds no `allowances` when the map is empty.
            ("buffer", BTreeMap::new()),
            (
                "kit",
                BTreeMap::from([
                    ("create_broadcast".to_string(), 10),
                    ("update_broadcast".to_string(), 10),
                ]),
            ),
            // Its writes run inside the marketing plan, so none has an allowance.
            ("google-ads", BTreeMap::new()),
        ] {
            let (_, server) = super::kit_entry(&marketing, &team, "kai", name, &BTreeMap::new())
                .unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
            assert!(super::matches_kit(&marketing, &server), "{name}");
            assert_eq!(server.allowances, allowed, "{name}");
        }
        // None is the Developer's: not by its role, and not by its kit.
        let developer =
            farik_roles::load_kit(Role::SoftwareDeveloper).expect("the Developer's kit");
        for name in ["higgsfield", "recraft", "buffer", "kit", "google-ads"] {
            let refused = super::kit_entry(&marketing, &team, "dev-a", name, &BTreeMap::new())
                .expect_err("the Developer is not the Marketing Specialist");
            assert!(
                refused[0].message.starts_with("connector_not_in_kit: "),
                "{name}: {refused:?}"
            );
            let refused = super::kit_entry(&developer, &team, "dev-a", name, &BTreeMap::new())
                .expect_err("the Developer's kit has no marketing service");
            assert!(
                refused[0].message.starts_with("connector_not_in_kit: "),
                "{name}: {refused:?}"
            );
        }
    }

    /// A guard: each of the Finance Specialist's three services connects by name, signed in to,
    /// with the scope the kit pins and no allowance, and is no other role's (step 10).
    #[test]
    fn connects_each_finance_service_by_name() {
        use farik_core::contract::Role;
        use farik_core::team::CustomTransport;
        let team = crate::tools::fixtures::a_team_of_three(|wire| {
            wire["agents"].as_array_mut().expect("agents").push(
                farik_core::team::fixtures::an_agent_wire("fin", "finance_specialist"),
            );
        });
        let finance =
            farik_roles::load_kit(Role::FinanceSpecialist).expect("the Finance Specialist's kit");
        for (name, scopes) in [
            ("stripe", vec!["mcp".to_string()]),
            ("digits", Vec::new()),
            ("kick", vec!["mcp:read".to_string()]),
        ] {
            let (_, server) = super::kit_entry(&finance, &team, "fin", name, &BTreeMap::new())
                .unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
            assert!(super::matches_kit(&finance, &server), "{name}");
            let CustomTransport::Http { oauth, .. } = &server.transport else {
                panic!("{name} is http");
            };
            assert_eq!(
                oauth.as_ref().map(|settings| settings.scopes.clone()),
                Some(scopes),
                "{name} signs in"
            );
            assert!(server.allowances.is_empty(), "{name}");
        }
        // None is the Developer's: not by its role, and not by its kit.
        let developer =
            farik_roles::load_kit(Role::SoftwareDeveloper).expect("the Developer's kit");
        for name in ["stripe", "digits", "kick"] {
            let refused = super::kit_entry(&finance, &team, "dev-a", name, &BTreeMap::new())
                .expect_err("the Developer is not the Finance Specialist");
            assert!(
                refused[0].message.starts_with("connector_not_in_kit: "),
                "{name}: {refused:?}"
            );
            let refused = super::kit_entry(&developer, &team, "fin", name, &BTreeMap::new())
                .expect_err("the Developer's kit has no finance service");
            assert!(
                refused[0].message.starts_with("connector_not_in_kit: "),
                "{name}: {refused:?}"
            );
        }
    }

    /// A guard: every `farik_*` tool a role's skill or a kit's skill names is one Farik lists.
    #[test]
    fn kit_skills_name_only_tools_farik_lists() {
        let listed: Vec<&str> = crate::tools::tool_descriptors()
            .iter()
            .map(|tool| tool.name)
            .collect();
        let mut named = 0;
        for role in farik_roles::SHIPPED_ROLES {
            let mut texts: Vec<(String, String)> = farik_roles::load_role(role)
                .expect("a role")
                .skills
                .into_iter()
                .map(|skill| (skill.name, skill.text))
                .collect();
            for skill in farik_roles::load_kit(role).expect("a kit").skills {
                for text in skill.session_files.values() {
                    texts.push((skill.name.clone(), text.clone()));
                }
            }
            for (name, text) in &texts {
                for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                    if word.starts_with("farik_") {
                        assert!(listed.contains(&word), "{role}/{name}: {word}");
                        named += 1;
                    }
                }
            }
        }
        assert!(named > 0, "the skills name tools, and the check saw them");
    }

    #[test]
    fn matches_kit_compares_the_whole_entry() {
        let team = crate::tools::fixtures::a_team_of_three(|_| {});
        let kit = fixture_kit("matches");
        let (_, server) =
            super::kit_entry(&kit, &team, "dev-a", "fixture", &BTreeMap::new()).expect("an entry");
        assert!(super::matches_kit(&kit, &server));
        let mut wider = server.clone();
        wider.tools.insert(
            "search".to_string(),
            farik_core::governor::permissions::ConnectorTag::ExternalEffect,
        );
        wider.tools.insert(
            "delete_repo".to_string(),
            farik_core::governor::permissions::ConnectorTag::Network,
        );
        assert!(!super::matches_kit(&kit, &wider));
        let mut renamed = server.clone();
        renamed.name = "other".to_string();
        assert!(!super::matches_kit(&kit, &renamed));
        let mut custom = server;
        custom.kit = false;
        assert!(
            !super::matches_kit(&kit, &custom),
            "a custom entry is not the kit's"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connects_a_kit_connector_by_name() {
        let (harness, store) = keeping_a_kit("kit-connect");
        // The request's tags are empty: the kit decides every tool.
        let answer = call(
            &harness.daemon,
            "connector.connect",
            &kit_params("dev-a", &json!({})),
            "connectorConnectResult",
        );
        assert_eq!(
            answer,
            json!({ "stored_in": "keychain", "tools": KIT_TAGS() })
        );
        let written = entry(&harness, 1, "fixture").expect("the entry is written");
        assert_eq!(written["source"], "kit");
        assert_eq!(written["tools"], KIT_TAGS());
        let spec = farik_core::team::spec_sha256(&custom(&written));
        let kept = store
            .load(&kept_at(&harness, "dev-a", "fixture"))
            .expect("the store reads")
            .expect("the entry is kept");
        assert_eq!(kept.spec_sha256, spec);
        assert_eq!(
            kept.keys
                .iter()
                .map(|(name, value)| (name.as_str(), value.expose()))
                .collect::<Vec<_>>(),
            [("API_KEY", KEY)]
        );
        assert_eq!(
            bodies(&harness, EventKind::ConnectorConnected),
            [json!({
                "agent": "dev-a", "server": "fixture", "transport": "stdio",
                "credential_keys": ["API_KEY"], "tools": KIT_TAGS(), "spec_sha256": spec,
            })]
        );
        assert!(!log_text(&harness).contains(KEY));
    }

    const ALLOW_TAGS: fn() -> Value = || {
        json!({ "search": "network", "env": "external_effect", "delete_repo": "denied",
                "make": "external_effect", "post": "external_effect" })
    };

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connects_with_the_kits_default_allowances() {
        let (harness, _) = keeping_an_allowance_kit("allow-default");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        let written = entry(&harness, 1, "fixture").expect("the entry is written");
        assert_eq!(written["allowances"], json!({ "make": 20 }));
        let spec = farik_core::team::spec_sha256(&custom(&written));
        assert_eq!(
            bodies(&harness, EventKind::ConnectorConnected),
            [json!({
                "agent": "dev-a", "server": "fixture", "transport": "stdio",
                "credential_keys": ["API_KEY"], "tools": ALLOW_TAGS(), "spec_sha256": spec,
                "allowances": { "make": 20 },
            })]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connects_with_the_users_allowances() {
        let (harness, _) = keeping_an_allowance_kit("allow-users");
        call(
            &harness.daemon,
            "connector.connect",
            &allowance_params("dev-a", &json!({ "make": 5 })),
            "connectorConnectResult",
        );
        assert_eq!(
            entry(&harness, 1, "fixture").expect("written")["allowances"],
            json!({ "make": 5 })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_allowance_the_kit_does_not_offer() {
        let (harness, store) = keeping_an_allowance_kit("allow-not-offered");
        let (code, message) = refused(
            &harness,
            "connector.connect",
            &allowance_params("dev-a", &json!({ "post": 3 })),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("allowance_not_offered: "), "{message}");
        assert_eq!(entry(&harness, 1, "fixture"), None);
        assert!(
            store
                .load(&kept_at(&harness, "dev-a", "fixture"))
                .expect("reads")
                .is_none(),
            "nothing is kept"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_allowance_out_of_range() {
        let (harness, _) = keeping_an_allowance_kit("allow-range");
        for calls in [json!(1001), json!(-1), json!(4_294_967_297_u64)] {
            let (code, message) = refused(
                &harness,
                "connector.connect",
                &allowance_params("dev-a", &json!({ "make": calls })),
            );
            assert_eq!(code, -32005, "{calls}");
            assert!(
                message.starts_with("allowance_out_of_range: "),
                "{calls}: {message}"
            );
        }
        assert_eq!(entry(&harness, 1, "fixture"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn changes_allowances_without_the_key() {
        let (harness, store) = keeping_an_allowance_kit("allow-change");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        let before = store
            .load(&kept_at(&harness, "dev-a", "fixture"))
            .expect("reads")
            .expect("kept");
        call(
            &harness.daemon,
            "connector.allowances",
            &json!({ "agent": "dev-a", "server": "fixture", "allowances": { "make": 30 } }),
            "emptyResult",
        );
        let written = entry(&harness, 1, "fixture").expect("the entry stays");
        assert_eq!(written["allowances"], json!({ "make": 30 }));
        let spec = farik_core::team::spec_sha256(&custom(&written));
        assert_ne!(spec, before.spec_sha256, "an allowance is in the hash");
        let after = store
            .load(&kept_at(&harness, "dev-a", "fixture"))
            .expect("reads")
            .expect("kept");
        assert_eq!(after.spec_sha256, spec);
        assert_eq!(
            after.keys.keys().collect::<Vec<_>>(),
            before.keys.keys().collect::<Vec<_>>(),
            "the same keys, none typed again"
        );
        let connected_bodies = bodies(&harness, EventKind::ConnectorConnected);
        assert_eq!(connected_bodies.len(), 2);
        assert_eq!(connected_bodies[1]["allowances"], json!({ "make": 30 }));
        assert_eq!(connected_bodies[1]["spec_sha256"], json!(spec));
        assert_eq!(states(&harness)[0]["state"], "connected");
        assert!(!log_text(&harness).contains(KEY));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn changing_allowances_keeps_a_tool_left_out() {
        let (harness, _) = keeping("allow-change-left-out");
        harness.project.set_kit(fixture_kit_with(
            "allow-change-left-out",
            &json!({ "make": "external_effect", "post": "external_effect" }),
            &json!({
                "make": { "calls": 20, "what": "pictures" },
                "post": { "calls": 4, "what": "posts" }
            }),
        ));
        call(
            &harness.daemon,
            "connector.connect",
            &allowance_params("dev-a", &json!({ "make": 5, "post": 1 })),
            "connectorConnectResult",
        );
        call(
            &harness.daemon,
            "connector.allowances",
            &json!({ "agent": "dev-a", "server": "fixture", "allowances": { "make": 30 } }),
            "emptyResult",
        );
        assert_eq!(
            entry(&harness, 1, "fixture").expect("kept")["allowances"],
            json!({ "make": 30, "post": 1 })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn changing_allowances_checks_them_as_connect_does() {
        let (harness, _) = keeping_an_allowance_kit("allow-change-checked");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        for (allowances, code) in [
            (json!({ "post": 1 }), "allowance_not_offered: "),
            (json!({ "make": 1001 }), "allowance_out_of_range: "),
        ] {
            let (_, message) = refused(
                &harness,
                "connector.allowances",
                &json!({ "agent": "dev-a", "server": "fixture", "allowances": allowances }),
            );
            assert!(message.starts_with(code), "{message}");
        }
        assert_eq!(
            entry(&harness, 1, "fixture").expect("kept")["allowances"],
            json!({ "make": 20 })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_change_an_unconfirmed_entry() {
        let (harness, _) = keeping_an_allowance_kit("allow-unconfirmed");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        let mut team = team_file(&harness);
        team["agents"][1]["mcp_servers"][0]["args"]
            .as_array_mut()
            .expect("args")
            .push(json!("--elsewhere"));
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&team).expect("a team"))
            .expect("the team is written");
        let (_, message) = refused(
            &harness,
            "connector.allowances",
            &json!({ "agent": "dev-a", "server": "fixture", "allowances": { "make": 30 } }),
        );
        assert!(
            message.starts_with("connector_not_confirmed: "),
            "{message}"
        );
        assert_eq!(
            entry(&harness, 1, "fixture").expect("kept")["allowances"],
            json!({ "make": 20 })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn changing_allowances_after_the_kit_changed_says_connect_again() {
        let (harness, _) = keeping_an_allowance_kit("allow-kit-changed");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        // A release tags `make` denied: the entry is no longer the kit's.
        harness.project.set_kit(fixture_kit_with(
            "allow-kit-changed-2",
            &json!({ "make": "denied", "post": "external_effect" }),
            &json!({}),
        ));
        let (_, message) = refused(
            &harness,
            "connector.allowances",
            &json!({ "agent": "dev-a", "server": "fixture", "allowances": { "make": 30 } }),
        );
        assert!(message.starts_with("connector_not_in_kit: "), "{message}");
    }

    #[test]
    fn matches_a_kit_entry_whose_allowance_differs_from_the_kits_default() {
        let team = crate::tools::fixtures::a_team_of_three(|_| {});
        let kit = allowance_kit("allow-matches");
        let (_, server) = super::kit_entry(
            &kit,
            &team,
            "dev-a",
            "fixture",
            &BTreeMap::from([("make".to_string(), 5)]),
        )
        .expect("an entry");
        assert_eq!(server.allowances, BTreeMap::from([("make".to_string(), 5)]));
        assert!(super::matches_kit(&kit, &server));
        let mut other = server.clone();
        other.allowances = BTreeMap::from([("post".to_string(), 3)]);
        assert!(
            !super::matches_kit(&kit, &other),
            "the kit offers none for post"
        );
        let mut none = server;
        none.allowances.clear();
        assert!(super::matches_kit(&kit, &none), "an entry may hold none");
    }

    #[test]
    fn kit_entry_gives_the_defaults_and_refuses_what_is_not_offered() {
        let team = crate::tools::fixtures::a_team_of_three(|_| {});
        let kit = allowance_kit("allow-entry");
        let (entry, _) =
            super::kit_entry(&kit, &team, "dev-a", "fixture", &BTreeMap::new()).expect("an entry");
        assert_eq!(entry["allowances"], json!({ "make": 20 }));
        let refused = |asked: &[(&str, u32)]| {
            let asked = asked
                .iter()
                .map(|(tool, n)| ((*tool).to_string(), *n))
                .collect();
            super::kit_entry(&kit, &team, "dev-a", "fixture", &asked)
                .expect_err("refused")
                .remove(0)
                .message
        };
        assert!(refused(&[("post", 3)]).starts_with("allowance_not_offered: "));
        assert!(refused(&[("make", 1001)]).starts_with("allowance_out_of_range: "));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_what_a_kit_offers_to_allow() {
        let (harness, _) = keeping_an_allowance_kit("allow-team-get");
        let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
        let developer = got["kits"]
            .as_array()
            .and_then(|kits| kits.iter().find(|kit| kit["role"] == "software_developer"))
            .expect("the Developer's kit");
        let row = &developer["connectors"][0];
        assert_eq!(
            row["allowances"],
            json!([{ "tool": "make", "calls": 20, "what": "pictures" }])
        );
    }

    /// `dev-a`'s `make` of the fixture kit, called `n` times by the hook, the last of them under
    /// the human's grant when `granted`.
    fn made(harness: &Harness, n: u32, granted: bool) {
        for call in 0..n {
            let mut body = json!({
                "tool": "mcp__fixture__make", "input": "{}", "server": "fixture",
                "tag": "external_effect",
            });
            if granted && call + 1 == n {
                body["approval"] = json!(5);
            }
            harness
                .project
                .record_by(Some("dev-a"), at(), "", "tool.called", &body);
        }
    }

    fn allowance_rows(harness: &Harness) -> Value {
        query(
            &harness.daemon,
            "allowances.list",
            &json!({}),
            "allowancesListResult",
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_each_allowance_with_its_use_and_period() {
        let (harness, _) = keeping_an_allowance_kit("allow-list");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        harness.open_sprint("S1", &[]);
        made(&harness, 3, false);
        assert_eq!(
            allowance_rows(&harness),
            json!({
                "period": { "kind": "sprint", "sprint_id": "S1" },
                "rows": [{
                    "agent": "dev-a", "server": "fixture", "tool": "make",
                    "what": "pictures", "used": 3, "of": 20
                }]
            })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn matches_the_tool_called_events() {
        let (harness, _) = keeping_an_allowance_kit("allow-list-events");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        made(&harness, 4, true);
        // Another agent's call, and another tool's, are not dev-a's make.
        harness.project.record_by(
            Some("dev-b"),
            at(),
            "",
            "tool.called",
            &json!({ "tool": "mcp__fixture__make", "input": "{}", "server": "fixture" }),
        );
        harness.project.record_by(
            Some("dev-a"),
            at(),
            "",
            "tool.called",
            &json!({ "tool": "mcp__fixture__post", "input": "{}", "server": "fixture" }),
        );
        let listed = allowance_rows(&harness);
        assert_eq!(listed["rows"][0]["used"], 4, "{listed}");
        let counted = harness
            .project
            .events(&[EventKind::ToolCalled])
            .iter()
            .filter(|event| {
                event.envelope.ids.agent_id.as_deref() == Some("dev-a")
                    && matches!(&event.body, farik_protocol::event::EventBody::ToolCalled(body)
                        if body.tool == "mcp__fixture__make")
            })
            .count();
        assert_eq!(counted, 4, "a granted call is among them");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_the_day_with_no_sprint_open() {
        let (harness, _) = keeping_an_allowance_kit("allow-list-day");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        let listed = allowance_rows(&harness);
        assert_eq!(
            listed["period"],
            json!({ "kind": "day", "day": "2026-09-22" })
        );
        assert_eq!(listed["rows"][0]["used"], 0);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaves_out_an_unconfirmed_entry_and_a_paused_agent() {
        let (harness, _) = keeping_an_allowance_kit("allow-list-left-out");
        connected(&harness, "dev-a", &kit_server(), &json!({}));
        connected(&harness, "dev-b", &kit_server(), &json!({}));
        assert_eq!(
            allowance_rows(&harness)["rows"].as_array().map(Vec::len),
            Some(2)
        );
        let mut team = team_file(&harness);
        // dev-a's entry edited by hand, and dev-b paused.
        team["agents"][1]["mcp_servers"][0]["args"]
            .as_array_mut()
            .expect("args")
            .push(json!("--elsewhere"));
        team["agents"][2]["status"] = json!("paused");
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&team).expect("a team"))
            .expect("the team is written");
        assert_eq!(allowance_rows(&harness)["rows"], json!([]));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_labels_for_a_kit_connector() {
        let (harness, store) = keeping_a_kit("kit-labels");
        let (code, message) = refused(
            &harness,
            "connector.connect",
            &kit_params("dev-a", &json!({ "search": "denied" })),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("kit_names_these: "), "{message}");
        assert_eq!(entry(&harness, 1, "fixture"), None);
        assert!(
            store
                .load(&kept_at(&harness, "dev-a", "fixture"))
                .expect("reads")
                .is_none(),
            "nothing is kept"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_extra_fields_on_a_kit_server() {
        let (harness, _) = keeping_a_kit("kit-extra");
        let mut params = kit_params("dev-a", &json!({}));
        params["server"]["url"] = json!("https://elsewhere.example/mcp");
        let reply = rpc(&harness.daemon, "connector.connect", &params);
        assert!(reply["error"].is_object(), "{reply}");
        assert_eq!(entry(&harness, 1, "fixture"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_entry_that_is_not_the_kits() {
        let (harness, _) = keeping_a_kit("kit-not-the-kits");
        let team = harness.project.deps.files.read_team().expect("the team");
        let (mut written, _) = super::kit_entry(
            &fixture_kit("kit-not-the-kits"),
            &team,
            "dev-a",
            "fixture",
            &BTreeMap::new(),
        )
        .expect("an entry");
        written["tools"]["env"] = json!("network");
        let spec = farik_core::team::spec_sha256(&custom(&written));
        let command = farik_protocol::command::Command::ConnectorConnect {
            agent: "dev-a".to_string(),
            server: written.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer: None,
        };
        let reply = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": command_to_value(&command) }),
        );
        let text = reply.to_string();
        assert!(text.contains("connector_not_in_kit"), "{text}");
        assert_eq!(entry(&harness, 1, "fixture"), None);
        // The same entry for an agent of another role is refused too.
        let (mut other, _) = super::kit_entry(
            &fixture_kit("kit-not-the-kits"),
            &team,
            "dev-b",
            "fixture",
            &BTreeMap::new(),
        )
        .expect("an entry");
        other["tools"] = KIT_TAGS();
        let command = farik_protocol::command::Command::ConnectorConnect {
            agent: "pm".to_string(),
            server: other.as_object().cloned().unwrap_or_default(),
            spec_sha256: farik_core::team::spec_sha256(&custom(&other)),
            issuer: None,
        };
        let reply = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": command_to_value(&command) }),
        );
        assert!(
            reply.to_string().contains("connector_not_in_kit"),
            "{reply}"
        );
        assert_eq!(entry(&harness, 0, "fixture"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_a_kit_connectors_tools_before_connecting() {
        let (harness, _) = keeping_a_kit("kit-tools");
        let listed = call(
            &harness.daemon,
            "connector.tools",
            &json!({ "agent": "dev-a", "server": kit_server(), "keys": { "API_KEY": "k" } }),
            "connectorToolsResult",
        );
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(names, ["search", "env", "delete_repo", "repo.delete"]);
        assert_eq!(entry(&harness, 1, "fixture"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_lists_each_roles_kit_and_each_rows_source() {
        let (harness, _) = keeping_a_kit("kit-team-get");
        let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
        // The team's Product Manager has a kit of services too, since step 06, and it lists first.
        let kits = got["kits"].as_array().expect("kits");
        assert_eq!(
            kits.iter()
                .map(|kit| kit["role"].clone())
                .collect::<Vec<_>>(),
            [json!("product_manager"), json!("software_developer")]
        );
        assert_eq!(
            json!([kits[1]]),
            json!([{
                "role": "software_developer",
                "connectors": [{
                    "name": "fixture", "title": "Fixture",
                    "about": "A server that stands in for a service.",
                    "why": "Lets the Developer search the fixture.",
                    "setup": "Make a key on the fixture's page and paste it.",
                    "key_page": "https://fixture.example/keys",
                    "labels": { "search": "search the fixture" },
                    "auth": "keys", "credential_keys": ["API_KEY"],
                }],
            }]),
            "no container connector in the Developer's kit"
        );
        call(
            &harness.daemon,
            "connector.connect",
            &kit_params("dev-a", &json!({})),
            "connectorConnectResult",
        );
        connected(
            &harness,
            "dev-b",
            &fixture_server("kit-team-get-custom"),
            &json!({}),
        );
        let rows = states(&harness);
        let source = |agent: &str| {
            rows.as_array()
                .and_then(|rows| rows.iter().find(|row| row["agent"] == agent))
                .map(|row| row["source"].clone())
        };
        assert_eq!(source("dev-a"), Some(json!("kit")));
        assert_eq!(source("dev-b"), Some(json!("custom")));
        // A role with no one on it left is not listed: every Developer retires.
        // A role whose agents have all retired is not listed: an Architect, who may retire.
        let mut architect_kit = fixture_kit("kit-team-get-architect");
        architect_kit.role = farik_core::contract::Role::Architect;
        harness.project.set_kit(architect_kit);
        let mut team = team_file(&harness);
        team["agents"].as_array_mut().expect("agents").push(
            farik_core::team::fixtures::an_agent_wire("ada", "architect"),
        );
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&team).expect("a team"))
            .expect("the team is written");
        let roles = |harness: &Harness| {
            let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
            got["kits"]
                .as_array()
                .expect("kits")
                .iter()
                .map(|kit| kit["role"].clone())
                .collect::<Vec<_>>()
        };
        assert!(roles(&harness).contains(&json!("architect")));
        let retired = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": { "command": "agent_update", "body": { "agent_id": "ada", "status": "retired" } } }),
        );
        assert!(retired["result"]["said"].is_string(), "{retired}");
        assert_eq!(
            roles(&harness),
            [json!("product_manager"), json!("software_developer")],
            "a retired agent's role lists no kit"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn disconnects_a_kit_connector_and_deletes_its_keys() {
        let (harness, store) = keeping_a_kit("kit-disconnect");
        call(
            &harness.daemon,
            "connector.connect",
            &kit_params("dev-a", &json!({})),
            "connectorConnectResult",
        );
        call(
            &harness.daemon,
            "connector.connect",
            &kit_params("dev-b", &json!({})),
            "connectorConnectResult",
        );
        call(
            &harness.daemon,
            "connector.disconnect",
            &json!({ "agent": "dev-a", "server": "fixture" }),
            "emptyResult",
        );
        assert_eq!(entry(&harness, 1, "fixture"), None);
        assert!(entry(&harness, 2, "fixture").is_some());
        let load = |agent: &str| {
            store
                .load(&kept_at(&harness, agent, "fixture"))
                .expect("reads")
        };
        assert!(load("dev-a").is_none());
        assert!(load("dev-b").is_some());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connector_tools_lists_with_the_keys() {
        let (harness, _) = keeping("connector-tools");
        let listed = call(
            &harness.daemon,
            "connector.tools",
            &json!({ "agent": "dev-a", "server": fixture_server("tools"), "keys": { "API_KEY": "k" } }),
            "connectorToolsResult",
        );
        let tools = listed["tools"].as_array().expect("a list");
        let usable: Vec<(&str, bool)> = tools
            .iter()
            .map(|tool| {
                (
                    tool["name"].as_str().unwrap_or_default(),
                    tool["usable"] == true,
                )
            })
            .collect();
        assert_eq!(
            usable,
            [
                ("search", true),
                ("env", true),
                ("delete_repo", true),
                ("repo.delete", false)
            ]
        );
        assert!(
            tools[1]["description"]
                .as_str()
                .is_some_and(|seen| seen.contains("API_KEY=k ")),
            "{listed}"
        );
        assert_eq!(team_file(&harness)["agents"][1].get("mcp_servers"), None);
        assert!(bodies(&harness, EventKind::ConnectorConnected).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connect_lists_tags_saves_and_records() {
        let (harness, store) = keeping("connector-connect");
        let mut params = connect_params(
            "dev-a",
            &fixture_server("connect"),
            &json!({ "search": "network", "delete_repo": "denied" }),
        );
        // A key the server does not name is never kept (carry T2).
        params["keys"]["OTHER_KEY"] = json!("another-value");
        let answer = call(
            &harness.daemon,
            "connector.connect",
            &params,
            "connectorConnectResult",
        );
        let tools =
            json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" });
        assert_eq!(answer, json!({ "stored_in": "keychain", "tools": tools }));
        let written = entry(&harness, 1, "fixture").expect("the entry is written");
        assert_eq!(written["source"], "custom");
        assert_eq!(written["tools"], tools);
        let spec = farik_core::team::spec_sha256(&custom(&written));
        let kept = store
            .load(&kept_at(&harness, "dev-a", "fixture"))
            .expect("the store reads")
            .expect("the entry is kept");
        assert_eq!(kept.spec_sha256, spec);
        assert_eq!(
            kept.keys
                .iter()
                .map(|(name, value)| (name.as_str(), value.expose()))
                .collect::<Vec<_>>(),
            [("API_KEY", KEY)]
        );
        assert_eq!(
            bodies(&harness, EventKind::ConnectorConnected),
            [json!({
                "agent": "dev-a", "server": "fixture", "transport": "stdio",
                "credential_keys": ["API_KEY"], "tools": tools, "spec_sha256": spec,
            })]
        );
        assert!(!log_text(&harness).contains(KEY));
        assert!(
            !std::fs::read_to_string(harness.project.repo.path.join(".farik/team.yaml"))
                .expect("the team file reads")
                .contains(KEY)
        );
        assert_eq!(
            states(&harness),
            json!([{ "source": "custom", "agent": "dev-a", "server": "fixture", "auth": "keys", "state": "connected", "stored_in": "keychain" }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_label_for_a_tool_not_listed_is_refused() {
        // `delete_rep=denied` must not leave `delete_repo` unlabelled while its user believes it
        // denied (carry: a misspelled --tag), nor may a tool Farik can't use be labelled.
        let (harness, store) = keeping("connector-misspelled");
        for tags in [
            json!({ "delete_rep": "denied" }),
            json!({ "repo.delete": "network" }),
        ] {
            let (code, message) = refused(
                &harness,
                "connector.connect",
                &connect_params("dev-a", &fixture_server("misspelled"), &tags),
            );
            assert_eq!(code, -32005, "{tags}");
            assert!(message.starts_with("tag_unknown_tool: "), "{message}");
            assert!(
                message.contains("search, env, delete_repo"),
                "names the tools listed: {message}"
            );
        }
        assert!(
            store
                .load(&kept_at(&harness, "dev-a", "fixture"))
                .expect("the store reads")
                .is_none()
        );
        assert_eq!(entry(&harness, 1, "fixture"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_unlabelled_tool_is_written_external_effect() {
        let (harness, _) = keeping("connector-unlabelled");
        let server = fixture_server("unlabelled");
        connected(&harness, "dev-a", &server, &json!({}));
        let written = entry(&harness, 1, "fixture").expect("the entry is written");
        assert_eq!(
            written["tools"],
            json!({ "search": "external_effect", "env": "external_effect", "delete_repo": "external_effect" })
        );
        let shown = json!([{ "source": "custom", "agent": "dev-a", "server": "fixture", "auth": "keys", "state": "connected", "stored_in": "keychain" }]);
        assert_eq!(states(&harness), shown);

        // Connected again with a label: the one entry is replaced, and the new hash is read.
        connected(&harness, "dev-a", &server, &json!({ "search": "network" }));
        let servers = team_file(&harness)["agents"][1]["mcp_servers"].clone();
        assert_eq!(servers.as_array().map(Vec::len), Some(1), "{servers}");
        assert_eq!(servers[0]["tools"]["search"], "network");
        assert_eq!(states(&harness), shown);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_refused_connect_echoes_no_secret() {
        let (harness, store) = keeping("connector-refused");
        let mut broken = fixture_server("refused");
        broken["command"] = json!("false");
        broken["args"] = json!([]);
        for server in [broken, {
            let mut reserved = fixture_server("refused-name");
            reserved["name"] = json!("farik");
            reserved
        }] {
            let reply = rpc(
                &harness.daemon,
                "connector.connect",
                &connect_params("dev-a", &server, &json!({})),
            );
            assert_eq!(reply["error"]["code"], -32005, "{reply}");
            assert!(!reply.to_string().contains(KEY), "{reply}");
        }
        assert!(
            store
                .load(&kept_at(&harness, "dev-a", "fixture"))
                .expect("the store reads")
                .is_none()
        );
        assert!(bodies(&harness, EventKind::ConnectorConnected).is_empty());
        for method in ["connector.connect", "connector.tools"] {
            let mut params = connect_params("dev-a", &fixture_server("refused-frame"), &json!({}));
            params["keys"]["API_KEY"] = json!(KEY);
            params["unexpected"] = json!(true);
            let reply = rpc(&harness.daemon, method, &params);
            assert_eq!(reply["error"]["code"], -32602, "{method}: {reply}");
            assert_eq!(reply["error"].get("data"), None, "{method}: {reply}");
            assert!(!reply.to_string().contains(KEY), "{method}: {reply}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_connect_command_body_holds_no_secret() {
        let (harness, _) = keeping("connector-command");
        let mut server = fixture_server("command");
        server["source"] = json!("custom");
        server["tools"] = json!({ "search": "network" });
        let spec = farik_core::team::spec_sha256(&custom(&server));
        let wire = |server: &Value, spec: &str| {
            json!({ "command": "connector_connect",
                    "body": { "agent": "dev-a", "server": server, "spec_sha256": spec } })
        };
        let command =
            command_from_value(&wire(&server, &spec)).expect("names, labels and a hash are enough");
        assert_eq!(command_to_value(&command), wire(&server, &spec));
        let mut with_value = wire(&server, &spec);
        with_value["body"]["keys"] = json!({ "API_KEY": KEY });
        assert!(command_from_value(&with_value).is_err());

        let sent = |wire: Value| rpc(&harness.daemon, "command", &json!({ "command": wire }));
        let mut holding = server.clone();
        holding["env"] = json!({ "API_KEY": KEY });
        let refused = sent(wire(&holding, &spec));
        assert_eq!(refused["result"]["error"]["kind"], "refused", "{refused}");
        assert!(!refused.to_string().contains(KEY), "{refused}");
        let refused = sent(wire(&server, &"0".repeat(64)));
        assert_eq!(refused["result"]["error"]["kind"], "refused", "{refused}");
        assert!(bodies(&harness, EventKind::ConnectorConnected).is_empty());

        let done = sent(wire(&server, &spec));
        assert!(done["result"].get("said").is_some(), "{done}");
        assert_eq!(entry(&harness, 1, "fixture"), Some(server.clone()));
        assert_eq!(
            bodies(&harness, EventKind::ConnectorConnected),
            [json!({
                "agent": "dev-a", "server": "fixture", "transport": "stdio",
                "credential_keys": ["API_KEY"], "tools": { "search": "network" },
                "spec_sha256": spec,
            })]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn disconnect_removes_entry_and_keys_and_records() {
        let (harness, store) = keeping("connector-disconnect");
        let server = fixture_server("disconnect");
        connected(&harness, "dev-a", &server, &json!({}));
        connected(&harness, "dev-b", &server, &json!({}));
        call(
            &harness.daemon,
            "connector.disconnect",
            &json!({ "agent": "dev-a", "server": "fixture" }),
            "emptyResult",
        );
        assert_eq!(entry(&harness, 1, "fixture"), None);
        assert!(entry(&harness, 2, "fixture").is_some());
        let load = |agent: &str| {
            store
                .load(&kept_at(&harness, agent, "fixture"))
                .expect("the store reads")
        };
        assert!(load("dev-a").is_none());
        assert!(load("dev-b").is_some());
        assert_eq!(
            bodies(&harness, EventKind::ConnectorDisconnected),
            [json!({ "agent": "dev-a", "server": "fixture" })]
        );
        assert_eq!(
            states(&harness),
            json!([{ "source": "custom", "agent": "dev-b", "server": "fixture", "auth": "keys", "state": "connected", "stored_in": "keychain" }])
        );
        let (code, _) = refused(
            &harness,
            "connector.disconnect",
            &json!({ "agent": "dev-a", "server": "fixture" }),
        );
        assert_eq!(code, -32005);
        assert!(load("dev-b").is_some());

        // A connector Farik ships is no custom one to disconnect (carry H3).
        let mut wire = team_file(&harness);
        wire["agents"][1]["mcp_servers"] = json!([{ "name": "playwright", "source": "builtin" }]);
        let team = farik_core::team::validate_team(&wire).expect("a team");
        harness
            .project
            .deps
            .files
            .write_team(&team)
            .expect("written");
        let (code, _) = refused(
            &harness,
            "connector.disconnect",
            &json!({ "agent": "dev-a", "server": "playwright" }),
        );
        assert_eq!(code, -32005);
        assert!(entry(&harness, 1, "playwright").is_some());
        assert_eq!(
            bodies(&harness, EventKind::ConnectorDisconnected).len(),
            1,
            "only the custom one"
        );

        // The entry put back by hand, as a revert would: its keys are gone, so it runs nothing.
        let mut wire = team_file(&harness);
        wire["agents"][1]["mcp_servers"] = wire["agents"][2]["mcp_servers"].clone();
        let team = farik_core::team::validate_team(&wire).expect("a team");
        harness
            .project
            .deps
            .files
            .write_team(&team)
            .expect("written");
        assert_eq!(
            states(&harness),
            json!([
                { "source": "custom", "agent": "dev-a", "server": "fixture", "auth": "keys", "state": "connect_again" },
                { "source": "custom", "agent": "dev-b", "server": "fixture", "auth": "keys", "state": "connected", "stored_in": "keychain" },
            ])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn remove_during_a_refresh_keeps_nothing() {
        use crate::connectors::{ConnectorEntry, ConnectorSecrets as _};

        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let fixture = runtime.block_on(crate::oauth_fixture::Fixture::start());
        let (harness, store) = keeping("connector-remove-refreshing");
        let mut wire = team_file(&harness);
        wire["agents"][1]["mcp_servers"] = json!([{
            "name": "notion", "source": "custom", "transport": "http",
            "url": fixture.mcp_url, "oauth": {}, "tools": { "whoami": "network" }
        }]);
        let team = farik_core::team::validate_team(&wire).expect("a team");
        harness
            .project
            .deps
            .files
            .write_team(&team)
            .expect("written");
        let server = custom(&entry(&harness, 1, "notion").expect("notion"));
        let now = chrono::Utc::now();
        let (access, refresh) = fixture.mint();
        let at = kept_at(&harness, "dev-a", "notion");
        store
            .save(
                &at,
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(&server),
                    keys: std::collections::BTreeMap::new(),
                    oauth: Some(crate::sign_in::OAuthGrant {
                        issuer: fixture.origin.clone(),
                        resource: fixture.mcp_url.clone(),
                        client_id: "client-kept".to_string(),
                        token_endpoint: format!("{}/token", fixture.origin),
                        revocation_endpoint: None,
                        access_token: Secret::new(access),
                        refresh_token: Some(Secret::new(refresh)),
                        issued_at: now,
                        expires_at: Some(now - chrono::Duration::minutes(1)),
                        scopes: Vec::new(),
                        lapsed: false,
                        app: None,
                    }),
                },
            )
            .expect("kept");

        // A session's setup is refreshing, the service not yet answering.
        fixture.hold("token");
        let refreshing = {
            let (state, at, server) = (Arc::clone(&harness.daemon), at.clone(), server.clone());
            runtime.spawn(async move {
                crate::daemon::refreshed_entry(
                    &state,
                    &at,
                    &server,
                    std::time::Duration::from_mins(35),
                    std::time::Duration::from_secs(30),
                    false,
                )
                .await
            })
        };
        while fixture.count("/token") == 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // The user presses Remove; the service answers a moment after.
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                fixture.release("token");
            });
            call(
                &harness.daemon,
                "connector.disconnect",
                &json!({ "agent": "dev-a", "server": "notion" }),
                "emptyResult",
            );
        });
        runtime
            .block_on(refreshing)
            .expect("the refresh task")
            .expect("the refresh finished");
        assert!(
            store.load(&at).expect("the store reads").is_none(),
            "the entry removed during the refresh is not put back"
        );
        assert!(
            matches!(harness.daemon.kept(&at), crate::daemon::Kept::Nothing),
            "and the agent's page is not told it is kept"
        );
    }

    /// A daemon whose agent `dev-a` signs in to an OAuth fixture, on one runtime the fixture,
    /// the daemon's tasks and every call share, so that a sign-in left waiting between two calls
    /// is still there for the second.
    struct Signing {
        runtime: tokio::runtime::Runtime,
        fixture: crate::oauth_fixture::Fixture,
        harness: Harness,
        store: Arc<MemoryConnectorSecrets>,
        /// Every reply the daemon sent, as text.
        replies: std::sync::Mutex<Vec<String>>,
    }

    impl Signing {
        fn new(name: &str) -> Signing {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            let fixture = runtime.block_on(crate::oauth_fixture::Fixture::start());
            let (harness, store) = keeping(name);
            Signing {
                runtime,
                fixture,
                harness,
                store,
                replies: std::sync::Mutex::default(),
            }
        }

        /// `notion`, as `connector.sign_in` takes it, at the fixture's address.
        fn server(&self) -> Value {
            json!({
                "name": "notion", "transport": "http", "url": self.fixture.mcp_url, "oauth": {}
            })
        }

        /// `notion` as the daemon holds it.
        fn described(&self) -> farik_core::team::CustomServer {
            let wire = json!({
                "name": "notion", "source": "custom", "transport": "http",
                "url": self.fixture.mcp_url, "oauth": {}, "tools": { "whoami": "network" }
            });
            custom(&wire)
        }

        fn reply(&self, method: &str, params: &Value) -> Value {
            let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
            // A sign-in that waits for a task to end a window long (ten minutes) is a defect, not
            // a slow test: the answer is due at once.
            let reply = self
                .runtime
                .block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        crate::daemon::web::answer(
                            &self.harness.daemon,
                            &frame.to_string(),
                            &mut None,
                        ),
                    )
                    .await
                })
                .expect("the daemon answered within thirty seconds");
            crate::locked(&self.replies).push(reply.to_string());
            reply
        }

        /// The result of `method`, checked against `definition`.
        fn call(&self, method: &str, params: &Value, definition: &str) -> Value {
            let reply = self.reply(method, params);
            crate::daemon::gates::tests::conforms(&reply["result"], definition, &reply);
            reply["result"].clone()
        }

        /// The code and message `method` is refused with.
        fn refused(&self, method: &str, params: &Value) -> (i64, String) {
            let reply = self.reply(method, params);
            (
                reply["error"]["code"].as_i64().unwrap_or_default(),
                reply["error"]["message"]
                    .as_str()
                    .unwrap_or_else(|| panic!("an error: {reply}"))
                    .to_string(),
            )
        }

        fn sign_in(&self, server: &Value) -> Value {
            self.call(
                "connector.sign_in",
                &json!({ "agent": "dev-a", "server": server }),
                "connectorSignInResult",
            )
        }

        fn status(&self, attempt: &Value) -> Value {
            self.call(
                "connector.sign_in_status",
                &json!({ "attempt": attempt }),
                "connectorSignInStatusResult",
            )
        }

        /// The person pressed Cancel: the attempt ends, and whatever it gets is dropped.
        fn cancel(&self, attempt: &Value) -> Value {
            self.call(
                "connector.sign_in_cancel",
                &json!({ "attempt": attempt }),
                "emptyResult",
            )
        }

        /// The user says yes on the service's page, and the status stops waiting.
        fn approve(&self, started: &Value) -> Value {
            let url = started["authorize_url"].as_str().expect("an address");
            self.runtime.block_on(crate::oauth_fixture::follow(url));
            for _ in 0..200 {
                let status = self.status(&started["attempt"]);
                if status["state"] != "waiting" {
                    return status;
                }
                self.runtime.block_on(async {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                });
            }
            panic!("the sign-in never finished");
        }

        /// A finished sign-in's attempt.
        fn signed_in(&self, server: &Value) -> Value {
            let started = self.sign_in(server);
            assert_eq!(self.approve(&started)["state"], "signed_in");
            started["attempt"].clone()
        }

        fn connect(&self, attempt: &Value) -> Value {
            self.call(
                "connector.connect",
                &json!({
                    "agent": "dev-a", "server": self.server(), "attempt": attempt,
                    "tags": { "whoami": "network" }
                }),
                "connectorConnectResult",
            )
        }

        fn grant(&self) -> Option<crate::sign_in::OAuthGrant> {
            self.store
                .load(&kept_at(&self.harness, "dev-a", "notion"))
                .expect("the store reads")
                .and_then(|entry| entry.oauth)
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn sign_in_then_connect_keeps_the_grant() {
        let signing = Signing::new("connector-sign-in");
        let started = signing.sign_in(&signing.server());
        assert_eq!(started["issuer"], signing.fixture.origin);
        assert_eq!(signing.status(&started["attempt"])["state"], "waiting");
        assert_eq!(signing.approve(&started)["state"], "signed_in");
        let listed = signing.call(
            "connector.tools",
            &json!({ "agent": "dev-a", "server": signing.server(), "attempt": started["attempt"] }),
            "connectorToolsResult",
        );
        assert_eq!(listed["tools"][0]["name"], "whoami");
        let connected = signing.connect(&started["attempt"]);
        assert_eq!(connected["tools"], json!({ "whoami": "network" }));
        let entry = entry(&signing.harness, 1, "notion").expect("the team file has notion");
        assert_eq!(entry["oauth"], json!({}));
        let grant = signing.grant().expect("a grant is kept");
        assert_eq!(grant.issuer, signing.fixture.origin);
        assert_eq!(
            bodies(&signing.harness, EventKind::ConnectorConnected)[0]["issuer"],
            signing.fixture.origin
        );
        assert_eq!(
            states(&signing.harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "notion", "state": "connected", "auth": "oauth",
                "revokes": true, "stored_in": "keychain"
            }])
        );
    }

    /// A table of one device app, `Dev`, whose endpoints are `fixture`'s, for the servers at its
    /// address. Leaked: a table is `'static`, and a test's leak is small.
    fn dev_apps(
        fixture: &crate::oauth_fixture::Fixture,
    ) -> &'static [crate::registered_apps::RegisteredApp] {
        use crate::registered_apps::{AppFlow, RegisteredApp};
        fn leaked(text: String) -> &'static str {
            Box::leak(text.into_boxed_str())
        }
        let origin = &fixture.origin;
        Box::leak(Box::new([RegisteredApp {
            id: "dev",
            name: "Dev",
            host: Some("127.0.0.1"),
            farik_connector: None,
            flow: AppFlow::Device {
                device_endpoint: leaked(format!("{origin}/device/code")),
                verification_uri: leaked(format!("{origin}/login/device")),
            },
            client_id: "dev-client",
            client_secret: None,
            scopes: &[],
            issuer: leaked(format!("{origin}/login/oauth")),
            token_endpoint: leaked(format!("{origin}/token")),
            revocation_endpoint: None,
            install_url: Some("https://github.com/apps/dev/installations/new"),
            settings_url: "https://github.com/settings/apps/authorizations",
        }]))
    }

    impl Signing {
        /// The daemon serves `Dev`, whose endpoints are the fixture's.
        fn serving_dev(&self) {
            assert!(
                self.harness
                    .daemon
                    .set_registered_apps(dev_apps(&self.fixture))
            );
        }

        /// The daemon signs in for Farik's connector `osv` with `Google test`, whose endpoints are
        /// the fixture's, and starts that connector as the fixture's stdio server.
        fn serving_farik(&self, test: &str) {
            assert!(
                self.harness.daemon.set_registered_apps(
                    crate::registered_apps::fixtures::google_apps(&self.fixture)
                )
            );
            assert!(self.harness.daemon.set_own_program(
                crate::daemon::fixtures::own_program_serving_the_fixture(test)
            ));
        }

        /// Farik's own connector `osv`, as `connector.sign_in` takes it.
        fn farik_server() -> Value {
            json!({
                "name": "osv", "transport": "stdio", "command": "farik",
                "args": ["connector", "osv"], "oauth": {}
            })
        }

        /// The grant kept for `dev-a`'s `server`.
        fn grant_of(&self, server: &str) -> Option<crate::sign_in::OAuthGrant> {
            self.store
                .load(&kept_at(&self.harness, "dev-a", server))
                .expect("the store reads")
                .and_then(|entry| entry.oauth)
        }

        /// How many times the fixture saw `device_code` polled.
        fn polls_of(&self, device_code: &str) -> usize {
            self.fixture
                .requests("/token")
                .iter()
                .filter(|poll| {
                    poll.form
                        .get("device_code")
                        .is_some_and(|code| code == device_code)
                })
                .count()
        }

        /// Lets the daemon's tasks run for `seconds` of real time.
        fn run_for(&self, seconds: f64) {
            self.runtime.block_on(async {
                tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
            });
        }

        /// The status of a device sign-in, asked until it stops waiting: the service says yes by
        /// itself, after its interval.
        fn waited_for(&self, started: &Value) -> Value {
            for _ in 0..600 {
                let status = self.status(&started["attempt"]);
                if status["state"] != "waiting" {
                    return status;
                }
                self.run_for(0.01);
            }
            panic!("the sign-in never finished");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn sign_in_answers_the_provider_and_code() {
        let signing = Signing::new("connector-sign-in-device");
        signing.serving_dev();
        let started = signing.sign_in(&signing.server());
        let origin = &signing.fixture.origin;
        assert_eq!(started["provider"], "Dev");
        assert_eq!(started["user_code"], "WDJB-0001");
        assert_eq!(
            started["install_url"],
            "https://github.com/apps/dev/installations/new"
        );
        assert_eq!(started["authorize_url"], format!("{origin}/login/device"));
        assert_eq!(started["issuer"], format!("{origin}/login/oauth"));
        assert_eq!(signing.status(&started["attempt"])["state"], "waiting");
        assert_eq!(signing.waited_for(&started)["state"], "signed_in");
        // The code the user types is for the browser; the one Farik polls with never leaves it.
        for reply in crate::locked(&signing.replies).iter() {
            assert!(!reply.contains("dc-1"), "{reply}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_new_device_attempt_ends_the_old() {
        let signing = Signing::new("connector-sign-in-device-twice");
        signing.serving_dev();
        // Never approved, so that only the second attempt's start can end the first.
        signing.fixture.set(|flags| flags.device_pending = u32::MAX);
        let first = signing.sign_in(&signing.server());
        signing.run_for(1.5);
        assert!(signing.polls_of("dc-1") >= 1, "the first attempt polls");
        let second = signing.sign_in(&signing.server());
        assert_ne!(first["attempt"], second["attempt"]);
        // What was in flight when the first attempt ended has arrived.
        signing.run_for(0.3);
        let before = signing.polls_of("dc-1");
        signing.run_for(2.5);
        assert_eq!(
            signing.polls_of("dc-1"),
            before,
            "the first device code is no longer polled"
        );
        assert!(signing.polls_of("dc-2") >= 2, "the second one is");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_names_the_provider() {
        let signing = Signing::new("connector-sign-in-device-provider");
        signing.serving_dev();
        let started = signing.sign_in(&signing.server());
        assert_eq!(signing.waited_for(&started)["state"], "signed_in");
        signing.connect(&started["attempt"]);
        let grant = signing.grant().expect("a grant is kept");
        assert_eq!(grant.app.as_deref(), Some("dev"));
        assert_eq!(
            states(&signing.harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "notion", "state": "connected",
                "auth": "oauth", "revokes": false, "stored_in": "keychain",
                "provider": "Dev", "settings_url": "https://github.com/settings/apps/authorizations"
            }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn signs_in_and_connects_a_farik_connector() {
        let signing = Signing::new("connector-sign-in-farik");
        signing.serving_farik("connector-sign-in-farik");
        let server = Signing::farik_server();
        let started = signing.sign_in(&server);
        let origin = &signing.fixture.origin;
        assert_eq!(started["provider"], "Google test");
        assert_eq!(started["issuer"], *origin);
        assert!(
            started["authorize_url"]
                .as_str()
                .is_some_and(|url| url.starts_with(&format!("{origin}/o/oauth2/v2/auth?"))),
            "{started}"
        );
        assert!(started.get("user_code").is_none(), "no code to type");
        assert_eq!(signing.status(&started["attempt"])["state"], "waiting");
        assert_eq!(signing.approve(&started)["state"], "signed_in");
        let listed = signing.call(
            "connector.tools",
            &json!({ "agent": "dev-a", "server": server, "attempt": started["attempt"] }),
            "connectorToolsResult",
        );
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(names, ["search", "env", "delete_repo", "repo.delete"]);
        let connected = signing.call(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": server, "attempt": started["attempt"],
                "tags": { "search": "network", "delete_repo": "denied" }
            }),
            "connectorConnectResult",
        );
        let tools =
            json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" });
        assert_eq!(
            connected,
            json!({ "stored_in": "keychain", "tools": tools })
        );
        let written = entry(&signing.harness, 1, "osv").expect("the team file has osv");
        assert_eq!(written["oauth"], json!({}));
        assert_eq!(written["command"], "farik");
        let grant = signing.grant_of("osv").expect("a grant is kept");
        assert_eq!(grant.app.as_deref(), Some("google-test"));
        assert_eq!(
            states(&signing.harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "osv", "state": "connected",
                "auth": "oauth", "revokes": false, "stored_in": "keychain",
                "provider": "Google test",
                "settings_url": "https://myaccount.google.com/connections"
            }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_attempt_holds_only_for_its_whole_transport() {
        let signing = Signing::new("connector-sign-in-whole-transport");
        signing.serving_farik("connector-sign-in-whole-transport");
        let pair = Signing::farik_server();
        let mut scoped = pair.clone();
        scoped["oauth"] = json!({ "scopes": ["https://example.test/auth/other"] });
        let web = json!({
            "name": "osv", "transport": "http", "url": signing.fixture.mcp_url, "oauth": {}
        });
        // An attempt made for the pair serves neither an `http` server of the same name nor the
        // pair with other settings; and an `http` server's serves not the pair.
        for (made_for, used_for, what) in [
            (&pair, &scoped, "the pair with other scopes"),
            (&pair, &web, "an http server of the same name"),
            (&web, &pair, "the pair, from an http server's"),
        ] {
            let attempt = signing.signed_in(made_for);
            let (code, message) = signing.refused(
                "connector.connect",
                &json!({
                    "agent": "dev-a", "server": used_for, "attempt": attempt, "tags": {}
                }),
            );
            assert_eq!(code, -32005, "{what}");
            assert_eq!(
                message, "sign_in_unknown: that sign-in was for another server",
                "{what}"
            );
            assert!(
                signing.grant_of("osv").is_none(),
                "{what}: no entry is kept"
            );
            // The mismatch ended the attempt: even its own server is refused now.
            let (_, message) = signing.refused(
                "connector.connect",
                &json!({
                    "agent": "dev-a", "server": made_for, "attempt": attempt, "tags": {}
                }),
            );
            assert!(message.starts_with("sign_in_unknown:"), "{what}: {message}");
        }
        // The same transport, used for what it was made for, connects.
        let attempt = signing.signed_in(&pair);
        signing.call(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": pair, "attempt": attempt,
                "tags": { "search": "network" }
            }),
            "connectorConnectResult",
        );
        assert!(signing.grant_of("osv").is_some());
    }

    /// A guard: a connector no app signs in for, as in a build without Google's client secret,
    /// cannot be signed in to; and one that does asks for the app's scopes alone.
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_farik_connector_signs_in_only_with_an_app_and_its_scopes() {
        let signing = Signing::new("connector-sign-in-farik-no-app");
        let (code, message) = signing.refused(
            "connector.sign_in",
            &json!({ "agent": "dev-a", "server": Signing::farik_server() }),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("sign_in_not_supported: "), "{message}");
        assert!(signing.fixture.seen().is_empty());

        signing.serving_farik("connector-sign-in-farik-scopes");
        let mut asks_more = Signing::farik_server();
        asks_more["oauth"] = json!({ "scopes": ["https://example.test/auth/other"] });
        let (code, message) = signing.refused(
            "connector.sign_in",
            &json!({ "agent": "dev-a", "server": asks_more }),
        );
        assert_eq!(code, -32005);
        assert_eq!(
            message,
            format!(
                "sign_in_failed: Farik's Google test sign-in asks only for {}",
                crate::registered_apps::fixtures::SCOPE
            )
        );
        let mut asks_its_own = Signing::farik_server();
        asks_its_own["oauth"] = json!({ "scopes": [crate::registered_apps::fixtures::SCOPE] });
        let started = signing.sign_in(&asks_its_own);
        let address = reqwest::Url::parse(started["authorize_url"].as_str().expect("an address"))
            .expect("an address");
        let scope = address
            .query_pairs()
            .find(|(name, _)| name == "scope")
            .map(|(_, value)| value.into_owned());
        assert_eq!(
            scope.as_deref(),
            Some(crate::registered_apps::fixtures::SCOPE)
        );
    }

    /// A guard: the app that signs a connector in is the one whose connector it is, and not any
    /// app that serves one of Farik's connectors. With a table whose one entry is for another
    /// connector, `osv` has no way to sign in, and no page is made for it.
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_farik_connector_is_signed_in_only_by_the_app_for_its_name() {
        let signing = Signing::new("connector-sign-in-farik-other-app");
        let for_another: &'static [crate::registered_apps::RegisteredApp] =
            Box::leak(Box::new([crate::registered_apps::RegisteredApp {
                farik_connector: Some("other"),
                ..crate::registered_apps::fixtures::google_apps(&signing.fixture)[0]
            }]));
        assert!(signing.harness.daemon.set_registered_apps(for_another));
        let (code, message) = signing.refused(
            "connector.sign_in",
            &json!({ "agent": "dev-a", "server": Signing::farik_server() }),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("sign_in_not_supported: "), "{message}");
        assert!(signing.fixture.seen().is_empty(), "no request was made");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn sign_in_status_names_the_provider() {
        let signing = Signing::new("connector-sign-in-status-provider");
        signing.serving_farik("connector-sign-in-status-provider");
        signing.fixture.set(|flags| flags.access_denied = true);
        let status = signing.approve(&signing.sign_in(&Signing::farik_server()));
        assert_eq!(status["state"], "failed");
        assert_eq!(status["reason"]["code"], "access_denied");
        assert_eq!(
            status["reason"]["message"],
            "You said no on Google test's page."
        );
        // A sign-in that did not match on the way back names the provider too.
        signing.fixture.set(|flags| {
            flags.access_denied = false;
            flags.iss = crate::oauth_fixture::Iss::Absent;
        });
        let status = signing.approve(&signing.sign_in(&Signing::farik_server()));
        assert_eq!(status["reason"]["code"], "sign_in_mismatch");
        assert_eq!(
            status["reason"]["message"],
            "Something did not match on the way back from Google test."
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn no_reply_event_or_log_holds_a_token() {
        let signing = Signing::new("connector-sign-in-secrets");
        // A server that echoes its `Authorization` header into a tool's description would put the
        // token in a reply itself: this one does not.
        signing
            .fixture
            .set(|flags| flags.echo_authorization = false);
        let attempt = signing.signed_in(&signing.server());
        signing.call(
            "connector.tools",
            &json!({ "agent": "dev-a", "server": signing.server(), "attempt": attempt }),
            "connectorToolsResult",
        );
        signing.connect(&attempt);
        let grant = signing.grant().expect("a grant");
        let tokens = [
            grant.access_token.expose().to_string(),
            grant
                .refresh_token
                .as_ref()
                .expect("a refresh token")
                .expose()
                .to_string(),
        ];
        let mut seen = crate::locked(&signing.replies).concat();
        seen.push_str(&log_text(&signing.harness));
        for token in &tokens {
            assert!(!seen.contains(token), "a reply or an event holds a token");
        }
        // No file Farik wrote holds one either: the repository, `.farik/local/events.db` and
        // `team.yaml` among them, and the state folder.
        let root = signing.harness.project.repo.path.clone();
        let state = std::path::PathBuf::from(format!("{}-state", root.display()));
        let mut stack = vec![root, state];
        let mut files = 0;
        while let Some(path) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&path) else {
                continue;
            };
            for item in read.flatten() {
                let path = item.path();
                if path.is_dir() {
                    if path.file_name().is_none_or(|name| name != ".git") {
                        stack.push(path);
                    }
                } else if let Ok(bytes) = std::fs::read(&path) {
                    files += 1;
                    let text = String::from_utf8_lossy(&bytes);
                    for token in &tokens {
                        assert!(!text.contains(token), "{} holds a token", path.display());
                    }
                }
            }
        }
        assert!(files > 3, "the files were searched: {files}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn sign_in_status_says_why_it_failed() {
        let signing = Signing::new("connector-sign-in-denied");
        signing.fixture.set(|flags| flags.access_denied = true);
        let started = signing.sign_in(&signing.server());
        let status = signing.approve(&started);
        assert_eq!(status["state"], "failed");
        assert_eq!(status["reason"]["code"], "access_denied");
        assert!(
            status["reason"]["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_attempt_is_used_once_and_expires() {
        let signing = Signing::new("connector-sign-in-once");
        let attempt = signing.signed_in(&signing.server());
        signing.connect(&attempt);
        let params = json!({
            "agent": "dev-a", "server": signing.server(), "attempt": attempt, "tags": {}
        });
        let (code, message) = signing.refused("connector.connect", &params);
        assert_eq!(code, -32005);
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
        // Another, finished and left standing past ten minutes.
        let late = signing.signed_in(&signing.server());
        signing.runtime.block_on(async {
            tokio::time::pause();
            tokio::time::advance(
                crate::sign_in::SIGN_IN_WINDOW + std::time::Duration::from_secs(1),
            )
            .await;
        });
        let (code, message) = signing.refused(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": signing.server(), "attempt": late, "tags": {}
            }),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_attempt_signs_in_one_server_only() {
        let signing = Signing::new("connector-sign-in-bound");
        let variants = [
            ("url", json!("http://127.0.0.1:9/elsewhere")),
            ("name", json!("other")),
            ("oauth", json!({ "scopes": ["write"] })),
        ];
        for (field, value) in variants {
            for method in ["connector.connect", "connector.tools"] {
                let attempt = signing.signed_in(&signing.server());
                let mut server = signing.server();
                server[field] = value.clone();
                let mut params = json!({ "agent": "dev-a", "server": server, "attempt": attempt });
                if method == "connector.connect" {
                    params["tags"] = json!({});
                }
                let (code, message) = signing.refused(method, &params);
                assert_eq!(code, -32005, "{field} {method}");
                assert!(
                    message.starts_with("sign_in_unknown:"),
                    "{field}: {message}"
                );
                assert!(signing.grant().is_none(), "{field}: no entry is kept");
                // The mismatch ended the attempt: even the right server is refused now.
                let (_, message) = signing.refused(
                    "connector.connect",
                    &json!({
                        "agent": "dev-a", "server": signing.server(), "attempt": attempt,
                        "tags": {}
                    }),
                );
                assert!(
                    message.starts_with("sign_in_unknown:"),
                    "{field}: {message}"
                );
            }
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_attempt_signs_in_one_agent_only() {
        let signing = Signing::new("connector-sign-in-agent");
        for method in ["connector.connect", "connector.tools"] {
            let attempt = signing.signed_in(&signing.server());
            let mut params =
                json!({ "agent": "dev-b", "server": signing.server(), "attempt": attempt });
            if method == "connector.connect" {
                params["tags"] = json!({});
            }
            let (code, message) = signing.refused(method, &params);
            assert_eq!(code, -32005, "{method}");
            assert!(
                message.starts_with("sign_in_unknown:"),
                "{method}: {message}"
            );
            assert!(signing.grant().is_none(), "{method}: nothing for dev-a");
            assert!(
                signing
                    .store
                    .load(&kept_at(&signing.harness, "dev-b", "notion"))
                    .expect("the store reads")
                    .is_none(),
                "{method}: nothing for dev-b"
            );
            // The mismatch ended the attempt: even its own agent is refused now.
            let (_, message) = signing.refused(
                "connector.connect",
                &json!({
                    "agent": "dev-a", "server": signing.server(), "attempt": attempt, "tags": {}
                }),
            );
            assert!(
                message.starts_with("sign_in_unknown:"),
                "{method}: {message}"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_refused_label_keeps_the_sign_in() {
        let signing = Signing::new("connector-sign-in-label");
        let attempt = signing.signed_in(&signing.server());
        let (code, message) = signing.refused(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": signing.server(), "attempt": attempt,
                "tags": { "nope": "network" }
            }),
        );
        assert_eq!(code, -32005);
        assert!(message.starts_with("tag_unknown_tool"), "{message}");
        assert!(signing.grant().is_none());
        // The sign-in was not spent: the same attempt connects once the label is right.
        signing.connect(&attempt);
        assert!(signing.grant().is_some());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_connect_over_its_own_grant_does_not_revoke_it() {
        // A connect that failed after its grant was kept is pressed again: the grant it replaces
        // is itself, and the service is not asked to forget it.
        let signing = Signing::new("connector-sign-in-own");
        let attempt = signing.signed_in(&signing.server());
        let server = signing.described();
        let grant = signing
            .harness
            .daemon
            .peek_sign_in(
                attempt.as_str().expect("an attempt"),
                &crate::daemon::signed_in::Binding {
                    agent: "dev-a",
                    server: &server,
                },
            )
            .expect("the grant");
        signing
            .store
            .save(
                &kept_at(&signing.harness, "dev-a", "notion"),
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(&server),
                    keys: BTreeMap::new(),
                    oauth: Some(grant.clone()),
                },
            )
            .expect("kept");
        signing.connect(&attempt);
        signing.runtime.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        });
        assert_eq!(signing.fixture.count("/revoke"), 0);
        assert_eq!(
            signing
                .grant()
                .and_then(|kept| kept.refresh_token)
                .map(|token| token.expose().to_string()),
            grant.refresh_token.map(|token| token.expose().to_string())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connect_again_with_the_same_client_keeps_both_alive() {
        // Some services end every grant of a client when one is revoked, so a grant is not
        // revoked when the one replacing it is the same client's.
        let signing = Signing::new("connector-sign-in-same-client");
        let port = crate::ports::free_port();
        let server = json!({
            "name": "notion", "transport": "http", "url": signing.fixture.mcp_url,
            "oauth": { "client_id": "fixed", "callback_port": port }
        });
        for _ in 0..2 {
            let attempt = signing.signed_in(&server);
            signing.call(
                "connector.connect",
                &json!({
                    "agent": "dev-a", "server": server, "attempt": attempt,
                    "tags": { "whoami": "network" }
                }),
                "connectorConnectResult",
            );
        }
        signing.runtime.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        });
        assert_eq!(signing.fixture.count("/revoke"), 0);
        assert_eq!(
            signing.grant().map(|grant| grant.client_id),
            Some("fixed".to_string())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_dropped_signed_in_server_is_asked_to_forget() {
        // A save that takes the server away, and a retirement, each ask the service to forget the
        // sign-in the entry held.
        for retire in [false, true] {
            let signing = Signing::new(&format!("connector-sign-in-dropped-{retire}"));
            let attempt = signing.signed_in(&signing.server());
            signing.connect(&attempt);
            let refresh = signing
                .grant()
                .and_then(|grant| grant.refresh_token)
                .expect("a refresh token");
            if retire {
                let reply = signing.reply(
                    "command",
                    &json!({ "command": { "command": "agent_update", "body": {
                        "agent_id": "dev-a", "status": "retired"
                    } } }),
                );
                assert!(reply["result"]["said"].is_string(), "{reply}");
            } else {
                let mut team = team_file(&signing.harness);
                team["agents"][1]
                    .as_object_mut()
                    .expect("an agent")
                    .remove("mcp_servers");
                signing.call("team.save", &json!({ "team": team }), "emptyResult");
            }
            for _ in 0..500 {
                if signing.fixture.count("/revoke") > 0 {
                    break;
                }
                signing.runtime.block_on(async {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                });
            }
            let revoked = signing.fixture.requests("/revoke");
            assert_eq!(revoked.len(), 1, "retire: {retire}");
            assert_eq!(
                revoked[0].form["token"],
                refresh.expose(),
                "retire: {retire}"
            );
            assert!(
                signing.grant().is_none(),
                "retire: {retire}: the entry is gone"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connect_needs_an_attempt_to_sign_in() {
        let signing = Signing::new("connector-sign-in-needed");
        for method in ["connector.connect", "connector.tools"] {
            let mut params = json!({
                "agent": "dev-a", "server": signing.server(), "keys": { "API_KEY": "k" }
            });
            if method == "connector.connect" {
                params["tags"] = json!({});
            }
            let (code, message) = signing.refused(method, &params);
            assert_eq!(code, -32005, "{method}");
            assert!(
                message.starts_with("sign_in_needed:"),
                "{method}: {message}"
            );
        }
        assert!(signing.grant().is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_new_attempt_ends_the_old() {
        let signing = Signing::new("connector-sign-in-replaced");
        let first = signing.sign_in(&signing.server());
        let port = |started: &Value| {
            reqwest::Url::parse(started["authorize_url"].as_str().expect("an address"))
                .expect("a url")
                .query_pairs()
                .find(|(name, _)| name == "redirect_uri")
                .and_then(|(_, uri)| reqwest::Url::parse(&uri).ok())
                .and_then(|uri| uri.port())
                .expect("the redirect's port")
        };
        let old = port(&first);
        let listening = |port: u16| {
            signing
                .runtime
                .block_on(tokio::net::TcpStream::connect(("127.0.0.1", port)))
                .is_ok()
        };
        assert!(listening(old), "the first attempt listens");
        let second = signing.sign_in(&signing.server());
        assert!(
            !listening(old),
            "the old attempt's callback address is closed"
        );
        assert!(listening(port(&second)));
        let (_, message) = signing.refused(
            "connector.sign_in_status",
            &json!({ "attempt": first["attempt"] }),
        );
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn cancelling_a_device_attempt_stops_polling() {
        let signing = Signing::new("connector-sign-in-device-cancel");
        signing.serving_dev();
        signing.fixture.set(|flags| flags.device_pending = u32::MAX);
        let started = signing.sign_in(&signing.server());
        signing.run_for(1.5);
        assert!(signing.polls_of("dc-1") >= 1, "the attempt polls");
        assert_eq!(signing.cancel(&started["attempt"]), json!({}));
        // What was in flight when it ended has arrived; the service says yes from now on.
        signing.run_for(0.3);
        signing.fixture.set(|flags| flags.device_pending = 0);
        let before = signing.polls_of("dc-1");
        signing.run_for(2.5);
        assert_eq!(
            signing.polls_of("dc-1"),
            before,
            "a cancelled attempt is no longer polled"
        );
        let (_, message) = signing.refused(
            "connector.sign_in_status",
            &json!({ "attempt": started["attempt"] }),
        );
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
        // The yes the service gave after Cancel is never taken: nothing can connect it.
        let (_, message) = signing.refused(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": signing.server(), "attempt": started["attempt"],
                "tags": { "whoami": "network" }
            }),
        );
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
        assert!(signing.grant().is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn cancelling_a_redirect_attempt_closes_its_callback_and_drops_its_grant() {
        let signing = Signing::new("connector-sign-in-cancel");
        let port = |started: &Value| {
            reqwest::Url::parse(started["authorize_url"].as_str().expect("an address"))
                .expect("a url")
                .query_pairs()
                .find(|(name, _)| name == "redirect_uri")
                .and_then(|(_, uri)| reqwest::Url::parse(&uri).ok())
                .and_then(|uri| uri.port())
                .expect("the redirect's port")
        };
        let listening = |port: u16| {
            signing
                .runtime
                .block_on(tokio::net::TcpStream::connect(("127.0.0.1", port)))
                .is_ok()
        };
        // Cancelled while waiting: the way back is closed, so a yes given later reaches nothing.
        let waiting = signing.sign_in(&signing.server());
        assert!(listening(port(&waiting)), "the attempt listens");
        assert_eq!(signing.cancel(&waiting["attempt"]), json!({}));
        assert!(
            !listening(port(&waiting)),
            "a cancelled attempt's callback address is closed"
        );
        assert_eq!(signing.fixture.count("/token"), 0);
        // Cancelled after the yes, before Next: the grant is dropped with the attempt.
        let attempt = signing.signed_in(&signing.server());
        assert_eq!(signing.cancel(&attempt), json!({}));
        let (_, message) = signing.refused(
            "connector.connect",
            &json!({
                "agent": "dev-a", "server": signing.server(), "attempt": attempt,
                "tags": { "whoami": "network" }
            }),
        );
        assert!(message.starts_with("sign_in_unknown:"), "{message}");
        assert!(signing.grant().is_none());
        // Cancelling what is already gone is done, not a refusal.
        assert_eq!(signing.cancel(&attempt), json!({}));
        assert_eq!(signing.cancel(&json!("0".repeat(32))), json!({}));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn disconnect_deletes_then_revokes() {
        for status in [200, 500] {
            let signing = Signing::new(&format!("connector-sign-in-revoke-{status}"));
            signing.fixture.set(|flags| flags.revoke_status = status);
            let attempt = signing.signed_in(&signing.server());
            signing.connect(&attempt);
            let refresh = signing
                .grant()
                .and_then(|grant| grant.refresh_token)
                .expect("a refresh token");
            let reply = signing.call(
                "connector.disconnect",
                &json!({ "agent": "dev-a", "server": "notion" }),
                "emptyResult",
            );
            assert_eq!(reply, json!({}), "{status}");
            assert!(signing.grant().is_none(), "{status}: the entry is gone");
            let revoked = signing.fixture.requests("/revoke");
            assert_eq!(revoked.len(), 1, "{status}");
            assert_eq!(revoked[0].form["token"], refresh.expose(), "{status}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn connect_again_revokes_the_replaced_grant() {
        let signing = Signing::new("connector-sign-in-replace");
        let first = signing.signed_in(&signing.server());
        signing.connect(&first);
        let old = signing
            .grant()
            .and_then(|grant| grant.refresh_token)
            .expect("a refresh token");
        let second = signing.signed_in(&signing.server());
        signing.connect(&second);
        for _ in 0..500 {
            if signing.fixture.count("/revoke") > 0 {
                break;
            }
            signing.runtime.block_on(async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            });
        }
        let revoked = signing.fixture.requests("/revoke");
        assert_eq!(revoked.len(), 1);
        assert_eq!(revoked[0].form["token"], old.expose());
        let kept = signing.grant().and_then(|grant| grant.refresh_token);
        assert_ne!(
            kept.map(|token| token.expose().to_string()),
            Some(old.expose().to_string())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_auth_and_sign_in_again() {
        let signing = Signing::new("connector-sign-in-states");
        // One that cannot be revoked, signed in and connected.
        signing.fixture.set(|flags| flags.revocation = false);
        let attempt = signing.signed_in(&signing.server());
        signing.connect(&attempt);
        assert_eq!(
            states(&signing.harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "notion", "state": "connected", "auth": "oauth",
                "revokes": false, "stored_in": "keychain"
            }])
        );
        // The service ends it.
        let at = kept_at(&signing.harness, "dev-a", "notion");
        let mut kept = signing.store.load(&at).expect("reads").expect("kept");
        if let Some(grant) = &mut kept.oauth {
            grant.lapsed = true;
            grant.revocation_endpoint = Some("https://auth.example/revoke".to_string());
        }
        signing.store.save(&at, &kept).expect("kept");
        signing.harness.daemon.read_kept(&at);
        assert_eq!(
            states(&signing.harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "notion", "state": "sign_in_again", "auth": "oauth",
                "revokes": true, "stored_in": "keychain"
            }])
        );
        // A server that takes keys.
        let (harness, _) = keeping("connector-sign-in-keys");
        connected(
            &harness,
            "dev-a",
            &fixture_server("sign-in-keys"),
            &json!({}),
        );
        assert_eq!(
            states(&harness),
            json!([{
                "source": "custom", "agent": "dev-a", "server": "fixture", "state": "connected", "auth": "keys",
                "stored_in": "keychain"
            }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_synchronous_delete_waits_for_whoever_holds_the_entry() {
        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let (harness, store) = keeping("connector-forget-waits");
        connected(
            &harness,
            "dev-a",
            &fixture_server("forget-waits"),
            &json!({}),
        );
        let at = kept_at(&harness, "dev-a", "fixture");
        // A refresh or a connect holds the entry.
        let held = runtime.block_on(harness.daemon.entry_lock(&at).lock_owned());
        runtime.block_on(async { harness.daemon.forget_entry(&at) });
        // The holder saves, and reads what it saved, as a refresh does.
        harness.daemon.read_kept(&at);
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            store.load(&at).expect("reads").is_some(),
            "the delete waits its turn"
        );
        drop(held);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while store.load(&at).expect("reads").is_some() {
            assert!(std::time::Instant::now() < deadline, "the delete ran");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            matches!(harness.daemon.kept(&at), crate::daemon::Kept::Nothing),
            "what the holder remembered is forgotten once the entry is gone"
        );
        // And with no one holding it, the delete is done when it returns.
        connected(
            &harness,
            "dev-a",
            &fixture_server("forget-waits"),
            &json!({}),
        );
        harness.daemon.forget_entry(&at);
        assert!(store.load(&at).expect("reads").is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn removing_an_agent_deletes_its_connector_keys() {
        // Removed, not retired: nothing is kept for an agent the team no longer has.
        let (harness, store) = keeping("connector-remove-agent");
        let server = fixture_server("remove-agent");
        connected(&harness, "dev-a", &server, &json!({}));
        connected(&harness, "dev-b", &server, &json!({}));
        let mut team = team_file(&harness);
        team["agents"].as_array_mut().expect("agents").remove(2);
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": team }),
            "emptyResult",
        );
        let load = |agent: &str| {
            store
                .load(&kept_at(&harness, agent, "fixture"))
                .expect("the store reads")
        };
        assert!(load("dev-b").is_none());
        assert!(load("dev-a").is_some());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn removing_a_server_by_a_save_deletes_its_keys() {
        // A save that takes one custom server from an agent, rather than `connector.disconnect`,
        // leaves nothing kept for it either (re-review N8).
        let (harness, store) = keeping("connector-remove-server");
        let server = fixture_server("remove-server");
        connected(&harness, "dev-a", &server, &json!({}));
        connected(&harness, "dev-b", &server, &json!({}));
        let mut team = team_file(&harness);
        assert_eq!(team["agents"][2]["id"], "dev-b");
        team["agents"][2]
            .as_object_mut()
            .expect("an agent")
            .remove("mcp_servers");
        call(
            &harness.daemon,
            "team.save",
            &json!({ "team": team }),
            "emptyResult",
        );
        let load = |agent: &str| {
            store
                .load(&kept_at(&harness, agent, "fixture"))
                .expect("the store reads")
        };
        assert!(load("dev-b").is_none());
        assert!(load("dev-a").is_some());
    }

    /// A keychain that is not there.
    struct NoKeychain;

    impl crate::connectors::ConnectorSecrets for NoKeychain {
        fn load(
            &self,
            _: &SecretAt,
        ) -> Result<Option<ConnectorEntry>, crate::credential::CredentialError> {
            Err(crate::credential::CredentialError::NoKeychain)
        }
        fn save(
            &self,
            _: &SecretAt,
            _: &ConnectorEntry,
        ) -> Result<crate::connectors::SecretStore, crate::credential::CredentialError> {
            Err(crate::credential::CredentialError::NoKeychain)
        }
        fn delete(&self, _: &SecretAt) -> Result<(), crate::credential::CredentialError> {
            Err(crate::credential::CredentialError::NoKeychain)
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_where_the_keys_are_kept() {
        // The agent's page says "in your keychain" or "in a private file" (carry M5).
        let harness = driven("connector-stored-in");
        let file = harness
            .project
            .repo
            .path
            .join(".farik/local/state/connectors.json");
        assert!(harness.daemon.set_connector_secrets(Arc::new(
            crate::connectors::ConnectorSecretStores::new(Arc::new(NoKeychain), Some(file))
        )));
        connected(&harness, "dev-a", &fixture_server("stored-in"), &json!({}));
        assert_eq!(
            states(&harness),
            json!([{ "source": "custom", "agent": "dev-a", "server": "fixture", "auth": "keys", "state": "connected", "stored_in": "file" }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn team_get_says_connect_again_for_an_unconfirmed_server() {
        let (harness, store) = keeping("connector-connect-again");
        let deps = &harness.project.deps;
        let mut wire = team_file(&harness);
        wire["agents"][1]["mcp_servers"] = json!([{
            "name": "linear", "source": "custom", "transport": "http",
            "url": "https://mcp.linear.example/mcp",
            "headers": { "Authorization": "Bearer {API_KEY}" },
            "credential_keys": ["API_KEY"], "tools": { "search": "network" },
        }]);
        let team = farik_core::team::validate_team(&wire).expect("a team");
        deps.files.write_team(&team).expect("written");
        let linear = custom(&wire["agents"][1]["mcp_servers"][0]);
        store
            .save(
                &kept_at(&harness, "dev-a", "linear"),
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(&linear),
                    keys: [("API_KEY".to_string(), Secret::new(KEY.to_string()))].into(),
                    oauth: None,
                },
            )
            .expect("kept");
        assert_eq!(
            states(&harness),
            json!([{ "source": "custom", "agent": "dev-a", "server": "linear", "auth": "keys", "state": "connected", "stored_in": "keychain" }])
        );
        // What is kept is read once, not on each query: a keychain may ask the user each time.
        store
            .delete(&kept_at(&harness, "dev-a", "linear"))
            .expect("deleted");
        assert_eq!(
            states(&harness),
            json!([{ "source": "custom", "agent": "dev-a", "server": "linear", "auth": "keys", "state": "connected", "stored_in": "keychain" }])
        );

        wire["agents"][1]["mcp_servers"][0]["url"] = json!("https://elsewhere.example/mcp");
        let team = farik_core::team::validate_team(&wire).expect("a team");
        deps.files.write_team(&team).expect("written");
        let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
        // The keys read last are still kept somewhere, which Remove says.
        assert_eq!(
            got["connectors"],
            json!([{ "source": "custom", "agent": "dev-a", "server": "linear", "auth": "keys", "state": "connect_again", "stored_in": "keychain" }])
        );
        farik_core::team::validate_team(&got["team"]).expect("team is still the team file");
    }
}
