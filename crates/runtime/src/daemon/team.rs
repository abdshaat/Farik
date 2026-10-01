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
    Agent, CustomServer, MODEL_FAMILIES, Team, ValidationError, custom_server, describe_change,
    spec_sha256, validate_team,
};
use farik_protocol::command::{Command, CommandReply};
use farik_protocol::event::{EventBody, new_event};
use farik_protocol::generated::event::{CriteriaUpdatedBody, TeamUpdatedBody};
use farik_roles::load_role;
use farik_store::{EventQuery, names_of, scan_project};
use serde_json::{Value, json};

use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, REFUSED};
use super::{DaemonState, Kept};
use crate::claude::{CredentialKind, Secret, credential_variable};
use crate::connectors::{ConnectorEntry, ConnectorError, SecretAt, list_tools};
use crate::credential::{CredentialError, credential_of_kind, load_credential, save_credential};
use crate::pause::{key_refused, paused};
use crate::session::session_model;
use crate::sprints::sprint_work;
use crate::tools::ToolDeps;

/// The methods this module answers.
pub(super) const METHODS: [&str; 9] = [
    "team.save",
    "agent.replace",
    "team.start",
    "criteria.save",
    "account.disconnect",
    "project.note",
    "connector.tools",
    "connector.connect",
    "connector.disconnect",
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
            answer["team"] = serde_json::to_value(team).map_err(|e| internal(&e))?;
            answer["max_agents"] = json!(farik_core::team::MAX_AGENTS);
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
        _ => Err(Failure::new(
            super::web::UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
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
pub(crate) fn secret_at(deps: &ToolDeps, agent: &str, server: &str) -> SecretAt {
    SecretAt {
        project_id: deps.ids.project_id.clone(),
        agent_id: agent.to_string(),
        server: server.to_string(),
    }
}

/// Each agent's custom servers in `team` and whether each runs: `connected` when the definition
/// kept beside its keys is the team file's, `store_unavailable` when the store could not be read
/// the last time it was, and `connect_again` otherwise.
fn connector_states(state: &DaemonState, deps: &ToolDeps, team: &Team) -> Vec<Value> {
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
            let shown = match state.kept(&secret_at(deps, agent, &server.name)) {
                Kept::Hash(kept) if kept == spec_sha256(&server) => "connected",
                Kept::Unavailable => "store_unavailable",
                _ => "connect_again",
            };
            json!({ "agent": agent, "server": server.name, "state": shown })
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
    let mut entry = wire.clone();
    entry["source"] = json!("custom");
    entry["tools"] = tools;
    let name = entry["name"].as_str().unwrap_or_default().to_string();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let after = with_server(&team, agent, &name, Some(&entry))
        .map_err(|errors| Failure::from(Refused::Errors(errors)))?;
    let server = after
        .agents
        .iter()
        .filter(|held| held.id.as_str() == agent)
        .flat_map(|held| held.mcp_servers.iter().flatten())
        .find(|server| server.name.as_str() == name)
        .and_then(custom_server)
        .ok_or_else(|| internal(&format!("{name} is not a custom server once validated")))?;
    Ok((entry, server))
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
        },
    )
}

/// The tools of the server `params` describes for its agent, held to the team's rules first, as
/// it lists them with the keys `params` carries.
async fn listed(
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Vec<crate::connectors::ListedTool>, Failure> {
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
    list_tools(&server, &keys_of(params))
        .await
        .map_err(not_listed)
}

/// `connector.connect`: the server's tools listed with its keys, each usable one labelled by
/// `tags` or else `external_effect` (SPEC 5.6), the keys kept beside the definition's hash, then
/// `connector_connect` handled, which writes the team file and records `connector.connected`.
async fn connector_connect(
    state: &DaemonState,
    deps: &Arc<ToolDeps>,
    params: &Value,
) -> Result<Value, Failure> {
    let agent = params["agent"].as_str().unwrap_or_default().to_string();
    let listed = listed(deps, params).await?;
    let tools: serde_json::Map<String, Value> = listed
        .iter()
        .filter(|tool| tool.usable)
        .map(|tool| {
            let tag = params["tags"]
                .get(&tool.name)
                .cloned()
                .unwrap_or_else(|| json!("external_effect"));
            (tool.name.clone(), tag)
        })
        .collect();
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
    let mut keys = keys_of(params);
    keys.retain(|name, _| server.credential_keys.contains(name));
    let kept = ConnectorEntry {
        spec_sha256: spec.clone(),
        keys,
    };
    let (secrets, at) = (
        state.connector_secrets(),
        secret_at(deps, &agent, &server.name),
    );
    let stored_in = off_the_worker(move || {
        secrets
            .save(&at, &kept)
            .map_err(|error| Failure::new(REFUSED, words(&error)))
    })
    .await?;
    handled(
        state,
        Command::ConnectorConnect {
            agent,
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
        },
    )
    .await?;
    Ok(json!({ "stored_in": stored_in, "tools": tools }))
}

/// `connector.disconnect`: `connector_disconnect` handled, which removes the entry from the team
/// file and records `connector.disconnected`, then the agent's keys for it deleted.
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
    let (secrets, at) = (state.connector_secrets(), secret_at(deps, agent, server));
    off_the_worker(move || {
        secrets
            .delete(&at)
            .map_err(|error| Failure::new(REFUSED, words(&error)))
    })
    .await?;
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
    let after = validate_team(wire).map_err(Refused::Errors)?;
    let before = deps
        .files
        .read_team()
        .map_err(|e| Refused::Failed(internal(&e)))?;
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
    let newcomer = after.agents.last().cloned();
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
fn write_team(deps: &ToolDeps, team: &Team) -> Result<(), Failure> {
    deps.files.write_team(team).map_err(|e| internal(&e))?;
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
            let tools = Box::pin(listed(&deps, &params)).await?;
            Ok(json!({ "tools": tools
                .iter()
                .map(|tool| json!({
                    "name": tool.name, "description": tool.description, "usable": tool.usable,
                }))
                .collect::<Vec<_>>() }))
        }
        // Boxed: listing a server's tools makes a large future of every method's.
        "connector.connect" => Box::pin(connector_connect(state, &deps, &params)).await,
        "connector.disconnect" => connector_disconnect(state, &deps, &params).await,
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
                write_team(&deps, &team)
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
                write_team(&held, &team)?;
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
                    "Tells people about what you made",
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
        SecretAt {
            project_id: harness.project.deps.ids.project_id.clone(),
            agent_id: agent.to_string(),
            server: server.to_string(),
        }
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
        let answer = connected(
            &harness,
            "dev-a",
            &fixture_server("connect"),
            &json!({ "search": "network", "delete_repo": "denied" }),
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
            json!([{ "agent": "dev-a", "server": "fixture", "state": "connected" }])
        );
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
        let shown = json!([{ "agent": "dev-a", "server": "fixture", "state": "connected" }]);
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
            json!([{ "agent": "dev-b", "server": "fixture", "state": "connected" }])
        );
        let (code, _) = refused(
            &harness,
            "connector.disconnect",
            &json!({ "agent": "dev-a", "server": "fixture" }),
        );
        assert_eq!(code, -32005);
        assert!(load("dev-b").is_some());

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
                { "agent": "dev-a", "server": "fixture", "state": "connect_again" },
                { "agent": "dev-b", "server": "fixture", "state": "connected" },
            ])
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
                },
            )
            .expect("kept");
        assert_eq!(
            states(&harness),
            json!([{ "agent": "dev-a", "server": "linear", "state": "connected" }])
        );
        // What is kept is read once, not on each query: a keychain may ask the user each time.
        store
            .delete(&kept_at(&harness, "dev-a", "linear"))
            .expect("deleted");
        assert_eq!(
            states(&harness),
            json!([{ "agent": "dev-a", "server": "linear", "state": "connected" }])
        );

        wire["agents"][1]["mcp_servers"][0]["url"] = json!("https://elsewhere.example/mcp");
        let team = farik_core::team::validate_team(&wire).expect("a team");
        deps.files.write_team(&team).expect("written");
        let got = query(&harness.daemon, "team.get", &json!({}), "teamGetResult");
        assert_eq!(
            got["connectors"],
            json!([{ "agent": "dev-a", "server": "linear", "state": "connect_again" }])
        );
        farik_core::team::validate_team(&got["team"]).expect("team is still the team file");
    }
}
