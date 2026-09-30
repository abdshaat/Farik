//! The team's setup and settings for the browser (`docs/SPEC.md` 4.1, 4.4, 10): the suggested five,
//! a change checked and described before it is saved, the setup form's start, the checks, the
//! models, the AI account's disconnect, and the project read back with the user's note on it.
//! `web.rs` answers the frames; this module answers what they ask.

use std::fmt::Display;
use std::path::Path;
use std::sync::Arc;

use farik_core::contract::Role;
use farik_core::criteria::validate_criteria;
use farik_core::governor::paths::{PathRefusal, check_protected_paths};
use farik_core::team::{Team, ValidationError, describe_change, validate_team};
use farik_protocol::command::{Command, CommandReply};
use farik_protocol::event::{EventBody, new_event};
use farik_protocol::generated::event::{CriteriaUpdatedBody, TeamUpdatedBody};
use farik_roles::load_role;
use farik_store::{EventQuery, names_of, scan_project};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, REFUSED};
use crate::claude::credential_variable;
use crate::credential::{CredentialError, load_credential};
use crate::pause::paused;
use crate::session::session_model;
use crate::tools::ToolDeps;

/// The methods this module answers.
pub(super) const METHODS: [&str; 6] = [
    "team.save",
    "agent.replace",
    "team.start",
    "criteria.save",
    "account.disconnect",
    "project.note",
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

/// The five the team builder suggests (spec 4.1): name, role, and shipped avatar. The id is the
/// name's slug.
const FIVE: [(&str, Role, &str); 5] = [
    ("Mira", Role::ProductManager, "product-manager"),
    ("Sol", Role::ScrumMaster, "scrum-master"),
    ("Ada", Role::Architect, "architect"),
    ("Theo", Role::SoftwareDeveloper, "developer"),
    ("Kai", Role::MarketingSpecialist, "marketing-specialist"),
];

/// Each model family a person may choose, as its ids start, and the words they read for it.
const FAMILIES: [(&str, &str); 4] = [
    ("claude-fable-", "Most capable model"),
    ("claude-opus-", "Strongest model, thinks hard"),
    ("claude-sonnet-", "Everyday model"),
    ("claude-haiku-", "Quick model"),
];

fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The queries of this module, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, params: &Value) -> Result<Value, Failure> {
    match name {
        "team.get" => {
            let team = deps.files.read_team().map_err(|e| internal(&e))?;
            let mut answer = effective(deps, &team)?;
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
                    answer["effects"] = json!(describe_change(&before, &after));
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
    Ok(match load_credential(&web.env, &web.stores) {
        Some((credential, source)) => {
            let mut status =
                json!({ "provider": "anthropic", "kind": credential.kind(), "source": source });
            if let Some(variable) = credential_variable(&web.env) {
                status["environment_variable"] = json!(variable);
            }
            status
        }
        None => json!({ "provider": null, "kind": null, "source": null }),
    })
}

fn web_of(state: &DaemonState) -> Result<&super::web::WebState, Failure> {
    state
        .web()
        .ok_or_else(|| Failure::new(INTERNAL_ERROR, "the browser routes are off"))
}

/// `team.propose`: the team as it is, with the five in place of its agents, and the criteria.
fn propose(deps: &ToolDeps) -> Result<Value, Failure> {
    let mut team = serde_json::to_value(deps.files.read_team().map_err(|e| internal(&e))?)
        .map_err(|e| internal(&e))?;
    let agents = FIVE
        .iter()
        .map(|(name, role, avatar)| {
            let shipped = load_role(*role).map_err(|e| internal(&e))?;
            Ok(json!({
                "id": name.to_lowercase(),
                "display_name": name,
                "role": role,
                "avatar": avatar,
                "persona": shipped.persona,
                "status": "active",
                "model": { "id": shipped.model, "effort": shipped.effort },
            }))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    team["agents"] = json!(agents);
    let criteria = serde_json::to_value(deps.files.read_criteria().map_err(|e| internal(&e))?)
        .map_err(|e| internal(&e))?;
    Ok(json!({ "team": team, "criteria": criteria }))
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
    Ok(FAMILIES
        .iter()
        .filter_map(|(prefix, label)| {
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
    match FAMILIES.iter().find(|(prefix, _)| id.starts_with(prefix)) {
        Some((_, label)) if newest.iter().any(|(new, _)| new == id) => (*label).to_string(),
        Some((_, label)) => format!("{label} (older)"),
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
enum Refused {
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

fn errors_wire(errors: &[ValidationError]) -> Vec<Value> {
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
            let seen = deps
                .log
                .read(&EventQuery {
                    agent_id: Some(gone.id.to_string()),
                    limit: Some(1),
                    ..EventQuery::default()
                })
                .map_err(|e| Refused::Failed(internal(&e)))?;
            if !seen.is_empty() {
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

/// `agent.replace`: the agent retired and the newcomer added in one write, checked as a save is,
/// then `agent.updated` and `team.updated`.
fn replace(deps: &ToolDeps, state: &DaemonState, params: &Value) -> Result<(), Failure> {
    let agent_id = params["agent_id"].as_str().unwrap_or_default();
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
    let report = crate::orchestrator::update_agent_with(
        deps,
        state,
        agent_id,
        farik_core::team::AgentStatus::Retired,
        newcomer,
    );
    match report {
        Ok(_) => {}
        Err(crate::orchestrator::CommandError::Refused { reason }) => {
            return Err(Failure::new(REFUSED, reason));
        }
        Err(error) => return Err(internal(&format!("{error:?}"))),
    }
    let written = deps.files.read_team().map_err(|e| internal(&e))?;
    append(deps, team_updated(&written))
}

/// Appends `body` as the human's and projects it.
fn append(deps: &ToolDeps, body: EventBody) -> Result<(), Failure> {
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
    append(deps, team_updated(team))
}

/// `team.updated` for `team`, as the human's.
fn team_updated(team: &Team) -> EventBody {
    EventBody::TeamUpdated(TeamUpdatedBody {
        team_name: team.name.to_string(),
        agent_ids: team
            .agents
            .iter()
            .map(|agent| agent.id.to_string())
            .collect(),
        updated_by: HUMAN.to_string(),
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
        "team.save" => off_the_worker(move || {
            let (_, team) = checked(&deps, &params["team"], false)?;
            write_team(&deps, &team)
        })
        .await
        .map(|()| json!({})),
        "agent.replace" => {
            let state = Arc::clone(state);
            off_the_worker(move || replace(&deps, &state, &params))
                .await
                .map(|()| json!({}))
        }
        "criteria.save" => {
            off_the_worker(move || write_criteria(&deps, &library(&params["criteria"])?))
                .await
                .map(|()| json!({}))
        }
        _ => {
            let held = Arc::clone(&deps);
            // Only setup's start resumes the team: without the marker, a team paused by a
            // budget's stop or by a person stays paused.
            let setup = off_the_worker(move || {
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
async fn off_the_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| Err(internal(&error)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};
    use serde_json::{Value, json};

    use crate::claude::{ClaudeCredential, Secret};
    use crate::credential::{CredentialStore, MemoryStore};
    use crate::daemon::gates::tests::{call, driven, query, rpc};
    use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
    use crate::orchestrator::fixtures::Harness;
    use crate::tools::fixtures::at;

    const MARKER: &str = ".farik/local/setup-pending";

    /// `harness`'s daemon with its browser routes on, its credential kept in `store`, and `env`.
    fn served(harness: &Harness, store: &Arc<MemoryStore>, env: &[(&str, &str)]) {
        let stores: Vec<Arc<dyn CredentialStore>> = vec![Arc::clone(store) as _];
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
            })
        );
    }

    /// An event `agent` produced, which is work the log has seen.
    fn worked(harness: &Harness, agent: &str) {
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

    fn team_file(harness: &Harness) -> Value {
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
    fn refused(harness: &Harness, method: &str, params: &Value) -> (i64, String) {
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
    fn proposes_the_suggested_five() {
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
                    "claude-opus-5",
                    "high",
                    "Asks the questions that decide what to build",
                    "active"
                ]),
                json!([
                    "sol",
                    "Sol",
                    "scrum_master",
                    "scrum-master",
                    "claude-sonnet-5",
                    "medium",
                    "Keeps the work moving and nobody stuck",
                    "active"
                ]),
                json!([
                    "ada",
                    "Ada",
                    "architect",
                    "architect",
                    "claude-opus-5",
                    "high",
                    "Thinks about how it all fits together",
                    "active"
                ]),
                json!([
                    "theo",
                    "Theo",
                    "software_developer",
                    "developer",
                    "claude-opus-5",
                    "high",
                    "Builds it and tests it",
                    "active"
                ]),
                json!([
                    "kai",
                    "Kai",
                    "marketing_specialist",
                    "marketing-specialist",
                    "claude-sonnet-5",
                    "medium",
                    "Tells people about what you made",
                    "active"
                ]),
            ]
        );
        // The rest is the team as it is, and the checks the form edits beside it.
        let team = team_file(&harness);
        assert_eq!(proposed["team"]["name"], team["name"]);
        assert_eq!(proposed["team"]["policy"], team["policy"]);
        farik_core::team::validate_team(&proposed["team"]).expect("a team");
        assert_eq!(
            proposed["criteria"],
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON")
        );
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
                "effects": ["dev-a now uses claude-opus-5.", "dev-a now thinks with low effort."],
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
            json!({ "team_name": "Farik", "agent_ids": ["pm", "dev-a", "dev-b"], "updated_by": "human" })
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
                    "model": { "id": "claude-opus-5", "label": "Strongest model, thinks hard (older)", "effort": "high" },
                    "tiers": ["read", "network"],
                    "base_tiers": ["read", "network"],
                },
                {
                    "id": "dev-a",
                    "model": { "id": "claude-opus-5", "label": "Strongest model, thinks hard (older)", "effort": "high" },
                    "tiers": ["read", "write_workspace", "git_remote"],
                    "base_tiers": ["read", "write_workspace", "git_local", "git_remote"],
                },
                {
                    "id": "dev-b",
                    "model": { "id": "claude-sonnet-5", "label": "Everyday model", "effort": "low" },
                    "tiers": ["read", "write_workspace", "git_local", "git_remote"],
                    "base_tiers": ["read", "write_workspace", "git_local", "git_remote"],
                },
                {
                    "id": "ada",
                    "model": { "id": "claude-opus-5", "label": "Strongest model, thinks hard (older)", "effort": "high" },
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
                "permissions": { "run_commands": true, "push": false }
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
                { "id": "claude-sonnet-5", "label": "Everyday model" },
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
}
