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
use crate::tools::ToolDeps;

/// The methods this module answers.
pub(super) const METHODS: [&str; 5] = [
    "team.save",
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
        "team.propose" => propose(deps),
        "team.validate" => {
            let setup = deps.files.root().join(SETUP_PENDING).exists();
            match checked(deps, &params["team"], setup) {
                Ok((before, after)) => {
                    Ok(json!({ "errors": [], "effects": describe_change(&before, &after) }))
                }
                Err(Refused::Errors(errors)) => {
                    Ok(json!({ "errors": errors_wire(&errors), "effects": [] }))
                }
                Err(Refused::Failed(failed)) => Err(failed),
            }
        }
        "models.list" => models(deps),
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
            json!({ "provider": "anthropic", "kind": credential.kind(), "source": source })
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

/// `models.list`: the newest model of each family the prices name. A dated snapshot is the same
/// model as its alias, so it is passed over.
fn models(deps: &ToolDeps) -> Result<Value, Failure> {
    let prices = deps.files.effective_prices().map_err(|e| internal(&e))?;
    let models: Vec<Value> = FAMILIES
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
                .map(|(_, id)| json!({ "id": id, "label": label }))
        })
        .collect();
    Ok(json!({ "models": models }))
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
        .map(|error| json!({ "path": error.path, "message": error.message }))
        .collect()
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
                message: "pause, retire or resume an agent from its card".to_string(),
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
                    message: format!(
                        "{} has done work; retire it instead",
                        gone.display_name.as_str()
                    ),
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
    append(
        deps,
        EventBody::TeamUpdated(TeamUpdatedBody {
            team_name: team.name.to_string(),
            agent_ids: team
                .agents
                .iter()
                .map(|agent| agent.id.to_string())
                .collect(),
            updated_by: HUMAN.to_string(),
        }),
    )
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
    state: &DaemonState,
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
        "criteria.save" => {
            off_the_worker(move || write_criteria(&deps, &library(&params["criteria"])?))
                .await
                .map(|()| json!({}))
        }
        _ => {
            let held = Arc::clone(&deps);
            off_the_worker(move || {
                let marker = held.files.root().join(SETUP_PENDING);
                let (_, team) = checked(&held, &params["team"], marker.exists())?;
                let library = library(&params["criteria"])?;
                write_team(&held, &team)?;
                write_criteria(&held, &library)?;
                match std::fs::remove_file(&marker) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        Err(internal(&error))
                    }
                    _ => Ok(()),
                }
            })
            .await?;
            if paused(&deps.log).map_err(|e| internal(&e))? {
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
/// the team paused, since the driver keeps the key it already loaded. A credential from the
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
    let paused_now = match state.deps() {
        Some(deps) if !removed.is_empty() => {
            if !paused(&deps.log).map_err(|e| internal(&e))? {
                handled(state, Command::TeamPause).await?;
            }
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
            checked,
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
                |message| message.starts_with("a team needs an active product_manager")
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

        // A key from the environment cannot be removed; the answer names where it is.
        let harness = driven("team-disconnect-env");
        let store = Arc::new(MemoryStore::default());
        served(&harness, &store, &[("ANTHROPIC_API_KEY", "sk-ant-api-y")]);
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
        assert_eq!(
            scanned["kept_private"],
            json!([".env", "**/*.pem", ".farik/local/**"])
        );

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
        assert_eq!(
            capped["kept_private"],
            json!([".env", "**/*.pem", ".farik/local/**"])
        );
        std::fs::remove_dir_all(root.join("many")).expect("removed");
        let uncapped = query(
            &harness.daemon,
            "project.scan",
            &json!({}),
            "projectScanResult",
        );
        assert_eq!(
            uncapped["kept_private"],
            json!([".env", "**/*.pem", "**/*.key", ".farik/local/**"])
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
