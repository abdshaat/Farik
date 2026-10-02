//! Saved teams for the browser (`docs/SPEC.md` 4.4, ADR 0026 C): the list, a preview of using
//! one on this project, and saving, using, renaming and deleting one.
//! `web.rs` answers the frames; this module answers what they ask.

use std::collections::BTreeSet;
use std::sync::Arc;

use farik_core::contract::TaskStatus;
use farik_core::team::{
    AgentStatus, Team, TemplateApplied, apply_template, describe_change, template_from_team,
};
use serde_json::{Value, json};

use super::DaemonState;
use super::team::{
    Refused, append, errors_wire, internal, off_the_worker, suggested, team_updated, web_of, worked,
};
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, NOT_FOUND, REFUSED};
use crate::sprints::sprint_work;
use crate::templates::{TemplateError, Templates};
use crate::tools::ToolDeps;

/// The methods this module answers.
pub(super) const METHODS: [&str; 4] = [
    "template.save",
    "template.apply",
    "template.rename",
    "template.delete",
];

/// The queries this module answers.
pub(super) const QUERIES: [&str; 2] = ["templates.list", "template.preview"];

const NO_STATE_FOLDER: &str = "Farik has no folder on this computer to keep saved teams in.";
const TEMPLATE_CHANGED: &str = "This saved team was saved again since you looked at it. Go back and look at what changes \
     once more.";

/// A refusal the page words by `code`, at `path`.
fn refusal(code: &str, path: &str, message: &str) -> Failure {
    let mut failure = Failure::new(REFUSED, message);
    failure.data = Some(json!({ "errors": [{ "path": path, "message": message, "code": code }] }));
    failure
}

/// `error` as the wire says it: straight from the variant, not from its words.
fn failure(error: &TemplateError) -> Failure {
    let message = error.to_string();
    match error {
        TemplateError::Exists { .. } => refusal("template_exists", "/name", &message),
        TemplateError::Name => refusal("template_name", "/name", &message),
        TemplateError::Unreadable { .. } => refusal("template_unreadable", "/slug", &message),
        TemplateError::NotFound { .. } => Failure::new(NOT_FOUND, message),
        TemplateError::Io { .. } => Failure::new(INTERNAL_ERROR, message),
    }
}

/// Where saved teams are kept, or the refusal when Farik has no state folder.
fn templates_of(state: &DaemonState) -> Result<&Templates, Failure> {
    web_of(state)?
        .templates
        .as_ref()
        .ok_or_else(|| refusal("no_state_folder", "/", NO_STATE_FOLDER))
}

/// The queries of this module, whose params the schema already passed.
pub(super) fn query(
    state: &DaemonState,
    deps: &ToolDeps,
    name: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let templates = templates_of(state)?;
    if name == "templates.list" {
        let listing = templates.list().map_err(|error| failure(&error))?;
        return Ok(json!({
            "folder": templates.dir(),
            "templates": listing
                .templates
                .iter()
                .map(|(slug, template)| json!({ "slug": slug, "template": template }))
                .collect::<Vec<_>>(),
            "unreadable": listing
                .unreadable
                .iter()
                .map(|slug| json!({ "slug": slug }))
                .collect::<Vec<_>>(),
        }));
    }
    let (before, applied, _, digest) = applying(deps, templates, slug_of(params), None)?;
    answer(deps, &before, &applied, &digest)
}

fn slug_of(params: &Value) -> &str {
    params["slug"].as_str().unwrap_or_default()
}

/// The team as it is, what the template saved as `slug` makes of it, the template's name, and its
/// file's digest. An agent has worked when the log has an event of its. When `digest` is given, a
/// file with another digest is refused: it changed since the preview that answered that one.
fn applying(
    deps: &ToolDeps,
    templates: &Templates,
    slug: &str,
    digest: Option<&str>,
) -> Result<(Team, TemplateApplied, String, String), Failure> {
    let (template, read) = templates
        .read_digested(slug)
        .map_err(|error| failure(&error))?;
    if digest.is_some_and(|digest| digest != read) {
        return Err(refusal("template_changed", "/digest", TEMPLATE_CHANGED));
    }
    let current = deps.files.read_team().map_err(|e| internal(&e))?;
    let mut workers = BTreeSet::new();
    for agent in current
        .agents
        .iter()
        .filter(|agent| agent.status != AgentStatus::Retired)
    {
        if worked(deps, agent.id.as_str())? {
            workers.insert(agent.id.to_string());
        }
    }
    let applied = apply_template(&current, &template, &workers, &suggested()?);
    Ok((current, applied, template.name.to_string(), read))
}

/// The preview's answer, and the apply's. Its effects say first what each retirement puts on hold:
/// the `in_progress` and `assigned` tasks the retired agent holds, which its retirement blocks.
fn answer(
    deps: &ToolDeps,
    before: &Team,
    applied: &TemplateApplied,
    digest: &str,
) -> Result<Value, Failure> {
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let mut effects = Vec::new();
    for agent in before
        .agents
        .iter()
        .filter(|agent| applied.retired.iter().any(|id| id == agent.id.as_str()))
    {
        let held: Vec<String> = board
            .iter()
            .filter(|row| {
                row.assignee_id.as_deref() == Some(agent.id.as_str())
                    && matches!(row.status, TaskStatus::InProgress | TaskStatus::Assigned)
            })
            .map(|row| format!("{} \u{201c}{}\u{201d}", row.task_id.as_str(), row.title))
            .collect();
        let name = agent.display_name.as_str();
        match held.as_slice() {
            [] => {}
            [task] => effects.push(format!(
                "{name} is retired. {name}'s unfinished task, {task}, is put on hold until you \
                 give it to someone."
            )),
            tasks => effects.push(format!(
                "{name} is retired. {name}'s unfinished tasks, {}, are put on hold until you give \
                 them to someone.",
                tasks.join(", ")
            )),
        }
    }
    let open = deps.projections.open_sprint().map_err(|e| internal(&e))?;
    let open = open.as_ref().map(|open| open.sprint_id.as_str());
    effects.extend(describe_change(
        before,
        &applied.team,
        &sprint_work(before, open, &board),
    ));
    Ok(json!({
        "team": applied.team,
        "kept": applied.kept,
        "retired": applied.retired,
        "removed": applied.removed,
        "added": applied.added,
        "effects": effects,
        "errors": errors_wire(&applied.errors),
        "digest": digest,
    }))
}

/// `template.apply`: worked out again under the lock that writes the team, so what is written is
/// checked against the team it replaces; then the team written once, each retirement's effects,
/// and `team.updated` naming the template.
fn apply(
    deps: &ToolDeps,
    state: &DaemonState,
    templates: &Templates,
    slug: &str,
    digest: &str,
) -> Result<Value, Failure> {
    let _writing = state.team_writes();
    let (before, applied, name, read) = applying(deps, templates, slug, Some(digest))?;
    let answered = answer(deps, &before, &applied, &read)?;
    if !applied.errors.is_empty() {
        return Err(Refused::Errors(applied.errors).into());
    }
    deps.files
        .write_team(&applied.team)
        .map_err(|e| internal(&e))?;
    crate::orchestrator::forget_removed_agents_keys(deps, state, &before, &applied.team);
    for agent_id in &applied.retired {
        crate::orchestrator::status_effects(
            deps,
            state,
            &applied.team,
            agent_id,
            AgentStatus::Retired,
        )
        .map_err(|error| internal(&format!("{error:?}")))?;
    }
    append(deps, team_updated(&applied.team, Some(&name)))?;
    Ok(answered)
}

/// The methods of this module, whose params the schema already passed.
pub(super) async fn call(
    state: &Arc<DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    templates_of(state)?;
    let applies = method == "template.apply";
    let waker = Arc::clone(state);
    let (state, method, params) = (Arc::clone(state), method.to_string(), params.clone());
    let answered = off_the_worker(move || {
        let templates = templates_of(&state)?;
        let slug = slug_of(&params);
        match method.as_str() {
            "template.save" => {
                let team = deps.files.read_team().map_err(|e| internal(&e))?;
                let name = params["name"].as_str().unwrap_or_default();
                let template = template_from_team(&team, name, deps.clock.now())
                    .map_err(|errors| failure(&errors.into()))?;
                let replace = params["replace"].as_bool().unwrap_or(false);
                let slug = templates
                    .save(&template, replace)
                    .map_err(|error| failure(&error))?;
                Ok(json!({ "slug": slug }))
            }
            "template.rename" => {
                let name = params["name"].as_str().unwrap_or_default();
                let slug = templates
                    .rename(slug, name)
                    .map_err(|error| failure(&error))?;
                Ok(json!({ "slug": slug }))
            }
            "template.delete" => {
                templates.delete(slug).map_err(|error| failure(&error))?;
                Ok(json!({}))
            }
            _ => apply(
                &deps,
                &state,
                templates,
                slug,
                params["digest"].as_str().unwrap_or_default(),
            ),
        }
    })
    .await?;
    // An applied template can free work at once: its policy may switch the Backlog off.
    if applies {
        waker.wakes().notify_one();
    }
    Ok(answered)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use farik_core::team::fixtures::an_agent_wire;
    use farik_core::team::validate_template;
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::event_to_value;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};

    use crate::daemon::gates::tests::{call, query, rpc};
    use crate::daemon::team::tests::{refused, team_file, worked};
    use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
    use crate::orchestrator::fixtures::Harness;
    use crate::templates::Templates;
    use crate::tools::fixtures::at;

    /// `harness`'s daemon with its browser routes on, keeping saved teams in `folder`.
    fn served(harness: &Harness, folder: Option<PathBuf>) {
        assert!(harness.daemon.set_web(WebState {
            codes: ConnectCodes::default(),
            sessions: BrowserSessions::open(None).expect("the sessions open"),
            project_root: harness.project.repo.path.clone(),
            credential: None,
            port: 49_731,
            clock: Arc::new(FixedClock::new(at())),
            take_on_error: std::sync::Mutex::default(),
            stores: Vec::new(),
            env: BTreeMap::new(),
            in_use: None,
            templates: folder.map(Templates::new),
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
    }

    /// The harness `name`, its team `pm`, `dev-a`, `dev-b` and the Marketing Specialist `kai`
    /// after `change`, served with an empty templates folder, which it answers.
    fn templated(name: &str, change: impl FnOnce(&mut Value)) -> (Harness, PathBuf) {
        let harness = Harness::new(name, |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("agents")
                .push(an_agent_wire("kai", "marketing_specialist"));
            change(wire);
        });
        let folder = std::env::temp_dir()
            .join(format!(
                "farik-daemon-templates-{}-{name}",
                std::process::id()
            ))
            .join("templates");
        let _ = std::fs::remove_dir_all(&folder);
        served(&harness, Some(folder.clone()));
        (harness, folder)
    }

    /// A digest no template file has.
    const NO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    /// The digest `template.preview` answers for `slug`, which `template.apply` is given.
    fn digest(harness: &Harness, slug: &str) -> Value {
        preview(harness, slug)["digest"].clone()
    }

    /// "Pair": Mira, the Product Manager `pm`; Ada, the Developer `dev-a`, on a cheaper model;
    /// Noor, a new Developer; commands off and pushing on, no plan check, pull requests, and no
    /// daily limit.
    fn pair() -> Value {
        json!({
            "version": 1,
            "name": "Pair",
            "saved_at": "2026-09-30T12:00:00Z",
            "agents": [
                { "id": "pm", "display_name": "Mira", "role": "product_manager", "persona": "Mira." },
                {
                    "id": "dev-a", "display_name": "Ada", "role": "software_developer",
                    "model": { "id": "claude-sonnet-5", "effort": "low" }
                },
                { "id": "noor", "display_name": "Noor", "role": "software_developer", "persona": "Noor." }
            ],
            "policy": {
                "permissions": { "run_commands": false, "push": true },
                "judgment": { "required": "never" },
                "integration": "pull_request"
            },
            "budgets": {}
        })
    }

    /// Saves `wire` in `folder` as the daemon would.
    fn saved(folder: &Path, wire: &Value) {
        Templates::new(folder.to_path_buf())
            .save(&validate_template(wire).expect("a template"), false)
            .expect("saved");
    }

    fn seq_count(harness: &Harness) -> usize {
        harness.project.event_count()
    }

    /// The refusal of `method` with `params`: its code, and its first error's code and path.
    fn refusal(harness: &Harness, method: &str, params: &Value) -> (i64, Value, Value) {
        let reply = rpc(&harness.daemon, method, params);
        let error = &reply["error"];
        (
            error["code"]
                .as_i64()
                .unwrap_or_else(|| panic!("an error: {reply}")),
            error["data"]["errors"][0]["code"].clone(),
            error["data"]["errors"][0]["path"].clone(),
        )
    }

    fn preview(harness: &Harness, slug: &str) -> Value {
        query(
            &harness.daemon,
            "template.preview",
            &json!({ "slug": slug }),
            "templateAppliedResult",
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_saves_renames_and_deletes() {
        let (harness, folder) = templated("templates-crud", |_| {});
        let list = || {
            query(
                &harness.daemon,
                "templates.list",
                &json!({}),
                "templatesListResult",
            )
        };
        assert_eq!(
            list(),
            json!({ "folder": folder.to_str(), "templates": [], "unreadable": [] })
        );
        let saved = call(
            &harness.daemon,
            "template.save",
            &json!({ "name": " My usual team " }),
            "templateSlugResult",
        );
        assert_eq!(saved, json!({ "slug": "my-usual-team" }));
        let listed = list();
        assert_eq!(listed["templates"][0]["slug"], "my-usual-team");
        assert_eq!(listed["templates"][0]["template"]["name"], "My usual team");
        assert_eq!(
            listed["templates"][0]["template"]["saved_at"],
            json!(at()),
            "saved at the daemon's now"
        );
        let ids: Vec<&Value> = listed["templates"][0]["template"]["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|agent| &agent["id"])
            .collect();
        assert_eq!(ids, ["pm", "dev-a", "dev-b", "kai"], "this project's team");
        assert!(folder.join("my-usual-team.yaml").is_file());

        let renamed = call(
            &harness.daemon,
            "template.rename",
            &json!({ "slug": "my-usual-team", "name": "Two of us" }),
            "templateSlugResult",
        );
        assert_eq!(renamed, json!({ "slug": "two-of-us" }));
        let listed = list();
        assert_eq!(listed["templates"].as_array().map(Vec::len), Some(1));
        assert_eq!(listed["templates"][0]["slug"], "two-of-us");
        assert_eq!(listed["templates"][0]["template"]["name"], "Two of us");

        let deleted = call(
            &harness.daemon,
            "template.delete",
            &json!({ "slug": "two-of-us" }),
            "emptyResult",
        );
        assert_eq!(deleted, json!({}));
        assert_eq!(list()["templates"], json!([]));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn saving_records_no_event() {
        let (harness, _) = templated("templates-no-event", |_| {});
        let before = seq_count(&harness);
        call(
            &harness.daemon,
            "template.save",
            &json!({ "name": "Pair" }),
            "templateSlugResult",
        );
        call(
            &harness.daemon,
            "template.save",
            &json!({ "name": "Pair", "replace": true }),
            "templateSlugResult",
        );
        call(
            &harness.daemon,
            "template.rename",
            &json!({ "slug": "pair", "name": "Two" }),
            "templateSlugResult",
        );
        call(
            &harness.daemon,
            "template.delete",
            &json!({ "slug": "two" }),
            "emptyResult",
        );
        assert_eq!(seq_count(&harness), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn previews_without_writing() {
        let (harness, folder) = templated("templates-preview", |_| {});
        saved(&folder, &pair());
        worked(&harness, "dev-b");
        let (team, events) = (team_file(&harness), seq_count(&harness));
        let shown = preview(&harness, "pair");
        assert_eq!(
            json!([
                shown["kept"],
                shown["retired"],
                shown["removed"],
                shown["added"],
                shown["errors"]
            ]),
            json!([["pm", "dev-a"], ["dev-b"], ["kai"], ["noor"], []])
        );
        assert_eq!(
            shown["effects"],
            json!([
                "Ada's model changes from the role's model to the everyday model.",
                "Ada now works quickly.",
                "The team has no daily spending limit.",
                "Finished work opens a pull request for you.",
                "Developers and Architects may no longer run commands.",
                "Farik still runs every check itself; the agents cannot run commands.",
                "Developers may now push their work and open pull requests."
            ])
        );
        assert_eq!(
            shown["team"]["policy"]["permissions"],
            json!({ "run_commands": false, "push": true })
        );
        let bytes = std::fs::read(folder.join("pair.yaml")).expect("the file reads");
        assert_eq!(
            shown["digest"],
            crate::daemon::hex(&Sha256::digest(&bytes)),
            "the digest is the file's bytes' sha256"
        );
        assert_eq!(team_file(&harness), team, "previewing writes nothing");
        assert_eq!(seq_count(&harness), events, "and records nothing");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn previews_the_work_a_switch_touches() {
        let (harness, folder) = templated("templates-switch", |wire| {
            wire["policy"]["plan_in_sprints"] = json!(true);
        });
        harness.ready("FRK-1");
        let mut off = pair();
        off["policy"]["plan_in_sprints"] = json!(false);
        saved(&folder, &off);
        let effects = preview(&harness, "pair")["effects"].clone();
        assert!(
            effects.as_array().expect("effects").contains(&json!(
                "Add a login page, waiting in the Backlog, can start now."
            )),
            "{effects}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_whole_templates() {
        let (harness, folder) = templated("templates-whole", |_| {});
        saved(&folder, &pair());
        std::fs::write(folder.join("broken.yaml"), "{").expect("written");
        let listed = query(
            &harness.daemon,
            "templates.list",
            &json!({}),
            "templatesListResult",
        );
        assert_eq!(
            listed["templates"],
            json!([{
                "slug": "pair",
                "template": serde_json::to_value(validate_template(&pair()).expect("a template"))
                    .expect("JSON"),
            }])
        );
        assert_eq!(
            listed["templates"][0]["template"]["agents"][0]["persona"],
            "Mira."
        );
        assert_eq!(
            listed["templates"][0]["template"]["agents"][1]["model"],
            json!({ "id": "claude-sonnet-5", "effort": "low" })
        );
        let policy = &listed["templates"][0]["template"]["policy"];
        assert_eq!(
            [
                &policy["permissions"],
                &policy["judgment"]["required"],
                &policy["integration"]
            ],
            [
                &pair()["policy"]["permissions"],
                &json!("never"),
                &json!("pull_request")
            ]
        );
        assert_eq!(listed["templates"][0]["template"]["budgets"], json!({}));
        assert_eq!(listed["unreadable"], json!([{ "slug": "broken" }]));
    }

    /// A template that switches "Plan work in sprints" off frees the Backlog at once.
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn wakes_the_team_when_a_template_is_applied() {
        let (harness, folder) = templated("templates-apply-wakes", |_| {});
        saved(&folder, &pair());
        // The preview's query runs a runtime of its own, so it is read before this one starts.
        let digest = digest(&harness, "pair");

        let (woken, applied) = tokio::runtime::Runtime::new().expect("a runtime").block_on(
            crate::daemon::team::tests::wakes(
                &harness,
                super::call(
                    &harness.daemon,
                    "template.apply",
                    &json!({ "slug": "pair", "digest": digest }),
                ),
            ),
        );

        assert!(applied.is_ok(), "{applied:?}");
        assert!(woken, "the wait ends");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn applying_deletes_a_removed_agents_connector_keys() {
        use crate::connectors::{ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets};

        let (harness, folder) = templated("templates-remove-keys", |wire| {
            wire["agents"][3]["mcp_servers"] = json!([{
                "name": "github", "source": "custom", "transport": "stdio",
                "command": "github-mcp", "credential_keys": ["API_KEY"],
                "tools": { "search": "network" }
            }]);
        });
        saved(&folder, &pair());
        let store = Arc::new(MemoryConnectorSecrets::default());
        assert!(harness.daemon.set_connector_secrets(store.clone()));
        let at = harness
            .daemon
            .secret_at(harness.project.deps.files.root(), "kai", "github")
            .expect("an address");
        let entry = ConnectorEntry {
            spec_sha256: "h".to_string(),
            keys: [(
                "API_KEY".to_string(),
                crate::claude::Secret::new("k".to_string()),
            )]
            .into(),
        };
        store.save(&at, &entry).expect("kept");
        call(
            &harness.daemon,
            "template.apply",
            &json!({ "slug": "pair", "digest": digest(&harness, "pair") }),
            "templateAppliedResult",
        );
        // Kai never worked, so the template removes Kai, and Kai's keys with Kai.
        assert_eq!(store.load(&at), Ok(None));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn applies_with_the_retirements_effects() {
        let (harness, folder) = templated("templates-apply", |wire| {
            wire["agents"][2]["display_name"] = json!("Sol");
        });
        saved(&folder, &pair());
        harness.in_progress("FRK-1", "dev-b", "dev-a");
        // dev-b also holds a task not started, put on hold too, and one handed in, which is not.
        harness.assigned("FRK-2", "dev-b", "dev-a");
        harness.in_progress("FRK-3", "dev-b", "dev-a");
        harness.project.moved(
            "FRK-3",
            "in_progress",
            "verifying",
            &json!({ "actor": "assignee", "requested_by": "dev-b", "assignee": "dev-b", "reviewer": "dev-a" }),
        );
        worked(&harness, "dev-b");
        let shown = preview(&harness, "pair");
        assert_eq!(
            shown["effects"][0],
            "Sol is retired. Sol's unfinished tasks, FRK-1 \u{201c}Add a login page\u{201d}, FRK-2 \
             \u{201c}Add a login page\u{201d}, are put on hold until you give them to someone.",
            "the preview names what a retirement puts on hold, by name, and only what is not handed in"
        );
        let before = seq_count(&harness);

        let applied = call(
            &harness.daemon,
            "template.apply",
            &json!({ "slug": "pair", "digest": shown["digest"] }),
            "templateAppliedResult",
        );
        assert_eq!(applied, shown, "applying makes what the preview showed");
        assert_eq!(team_file(&harness), shown["team"], "and writes it");
        let team = team_file(&harness);
        let status = |id: &str| {
            team["agents"]
                .as_array()
                .expect("agents")
                .iter()
                .find(|agent| agent["id"] == id)
                .map(|agent| agent["status"].clone())
        };
        assert_eq!(status("dev-b"), Some(json!("retired")));
        assert_eq!(status("noor"), Some(json!("active")));
        assert_eq!(status("kai"), None, "a never-worked agent is gone");
        let noor = team["agents"][3].clone();
        assert_eq!(
            [&noor["id"], &noor["avatar"], &noor["model"]],
            [
                &json!("noor"),
                &json!("developer"),
                &json!({ "id": "claude-opus-5-5", "effort": "high" })
            ],
            "a joining agent takes its role's suggested picture and model"
        );
        assert_eq!(
            team["policy"]["permissions"],
            pair()["policy"]["permissions"]
        );

        let after: Vec<Value> = harness.project.events(&[])[before..]
            .iter()
            .map(event_to_value)
            .collect();
        let kinds: Vec<&Value> = after.iter().map(|event| &event["kind"]).collect();
        assert_eq!(
            kinds,
            [
                "agent.updated",
                "task.transitioned",
                "task.transitioned",
                "team.updated"
            ]
        );
        assert_eq!(
            after[0]["body"],
            json!({ "agent_id": "dev-b", "status": "retired", "updated_by": "human" })
        );
        for (blocked, task) in after[1..3].iter().zip(["FRK-1", "FRK-2"]) {
            assert_eq!(blocked["task_id"], task);
            assert_eq!(blocked["body"]["to"], "blocked");
            assert_eq!(
                blocked["body"]["blocker"]["description"],
                "agent retired by the user"
            );
        }
        assert_eq!(
            after[3]["body"],
            json!({
                "team_name": "Farik",
                "agent_ids": ["pm", "dev-a", "dev-b", "noor"],
                "updated_by": "human",
                "template": "Pair",
                "plan_in_sprints": false,
            })
        );
        assert!(
            harness
                .project
                .events(&[])
                .iter()
                .all(|event| event.envelope.ids.agent_id.as_deref() != Some("kai")),
            "no event for the removed agent"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_in_plain_words() {
        let (harness, folder) = templated("templates-refused", |wire| {
            wire["agents"][2]["status"] = json!("paused");
        });
        saved(&folder, &pair());
        std::fs::write(folder.join("broken.yaml"), "{").expect("written");
        assert_eq!(
            refusal(&harness, "template.save", &json!({ "name": "pair" })),
            (-32005, json!("template_exists"), json!("/name"))
        );
        assert_eq!(
            refusal(&harness, "template.save", &json!({ "name": "!!!" })),
            (-32005, json!("template_name"), json!("/name"))
        );
        assert_eq!(
            refusal(
                &harness,
                "template.rename",
                &json!({ "slug": "broken", "name": "Pair" })
            )
            .0,
            -32005,
            "an unreadable file cannot be renamed"
        );
        for method in ["template.apply", "query"] {
            let params = if method == "query" {
                json!({ "name": "template.preview", "params": { "slug": "broken" } })
            } else {
                json!({ "slug": "broken", "digest": NO_DIGEST })
            };
            assert_eq!(
                refusal(&harness, method, &params),
                (-32005, json!("template_unreadable"), json!("/slug")),
                "{method}"
            );
        }
        let (code, message) = refused(&harness, "template.delete", &json!({ "slug": "nope" }));
        assert_eq!(
            (code, message.as_str()),
            (-32002, "There is no saved team nope.")
        );
        assert_eq!(
            rpc(
                &harness.daemon,
                "template.apply",
                &json!({ "slug": "nope", "digest": NO_DIGEST })
            )["error"]
                .get("data"),
            None
        );

        // The paused match: dev-b, paused, is kept as the only Developer while dev-a, who
        // worked, is retired.
        worked(&harness, "dev-a");
        let mut paused = pair();
        paused["name"] = json!("Paused");
        paused["agents"] = json!([
            { "id": "pm", "display_name": "Mira", "role": "product_manager" },
            { "id": "dev-b", "display_name": "Bo", "role": "software_developer" }
        ]);
        saved(&folder, &paused);
        let (team, events) = (team_file(&harness), seq_count(&harness));
        let shown = preview(&harness, "paused");
        assert_eq!(shown["errors"][0]["code"], "needs_developer", "{shown}");
        assert_eq!(shown["errors"][0]["path"], "/agents");
        assert_eq!(
            refusal(
                &harness,
                "template.apply",
                &json!({ "slug": "paused", "digest": digest(&harness, "paused") })
            ),
            (-32005, json!("needs_developer"), json!("/agents"))
        );
        assert_eq!(team_file(&harness), team, "nothing is written");
        assert_eq!(seq_count(&harness), events, "nothing is recorded");

        let bare = Harness::new("templates-no-folder", |_| {});
        served(&bare, None);
        for (method, params) in [
            ("query", json!({ "name": "templates.list", "params": {} })),
            (
                "query",
                json!({ "name": "template.preview", "params": { "slug": "pair" } }),
            ),
            ("template.save", json!({ "name": "Pair" })),
            (
                "template.apply",
                json!({ "slug": "pair", "digest": NO_DIGEST }),
            ),
            ("template.rename", json!({ "slug": "pair", "name": "Two" })),
            ("template.delete", json!({ "slug": "pair" })),
        ] {
            assert_eq!(
                refusal(&bare, method, &params),
                (-32005, json!("no_state_folder"), json!("/")),
                "{method} {params}"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn checks_again_when_applying_a_preview_gone_stale() {
        let (harness, folder) = templated("templates-stale", |_| {});
        let mut duo = pair();
        duo["agents"].as_array_mut().expect("agents").pop();
        saved(&folder, &duo);
        worked(&harness, "dev-b");
        let shown = preview(&harness, "pair");
        assert_eq!(shown["errors"], json!([]));
        // Since the preview, Ada (dev-a), whom Pair now keeps as its only Developer, was paused:
        // applying now would retire dev-b and leave no active Developer.
        let mut wire = team_file(&harness);
        wire["agents"][1]["status"] = json!("paused");
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&wire).expect("a team"))
            .expect("written");
        let events = seq_count(&harness);
        assert_eq!(
            refusal(
                &harness,
                "template.apply",
                &json!({ "slug": "pair", "digest": shown["digest"] })
            ),
            (-32005, json!("needs_developer"), json!("/agents"))
        );
        assert_eq!(team_file(&harness), wire, "nothing is written");
        assert_eq!(seq_count(&harness), events);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_template_saved_again_since_the_preview() {
        let (harness, folder) = templated("templates-changed", |_| {});
        saved(&folder, &pair());
        worked(&harness, "dev-b");
        let shown = preview(&harness, "pair");
        assert_eq!(shown["errors"], json!([]));
        // Since the preview, Pair was saved again, from another tab or by hand, with another
        // permission and the same saved_at.
        let mut again = pair();
        again["policy"]["permissions"]["push"] = json!(false);
        Templates::new(folder.clone())
            .save(&validate_template(&again).expect("a template"), true)
            .expect("saved");
        let (team, events) = (team_file(&harness), seq_count(&harness));
        assert_eq!(
            refusal(
                &harness,
                "template.apply",
                &json!({ "slug": "pair", "digest": shown["digest"] })
            ),
            (-32005, json!("template_changed"), json!("/digest"))
        );
        assert_eq!(team_file(&harness), team, "nothing is written");
        assert_eq!(seq_count(&harness), events);
        let (_, message) = refused(
            &harness,
            "template.apply",
            &json!({ "slug": "pair", "digest": shown["digest"] }),
        );
        assert_eq!(
            message,
            "This saved team was saved again since you looked at it. Go back and look at what \
             changes once more.",
            "the words the page says"
        );
        // Looking again gives the new file's digest, which applies.
        let again = preview(&harness, "pair");
        assert_ne!(again["digest"], shown["digest"]);
        call(
            &harness.daemon,
            "template.apply",
            &json!({ "slug": "pair", "digest": again["digest"] }),
            "templateAppliedResult",
        );
        assert_eq!(team_file(&harness)["policy"]["permissions"]["push"], false);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn works_the_result_out_again_under_the_lock() {
        let (harness, folder) = templated("templates-under-lock", |_| {});
        let mut duo = pair();
        duo["agents"].as_array_mut().expect("agents").pop();
        saved(&folder, &duo);
        worked(&harness, "dev-b");
        let digest = digest(&harness, "pair");
        let writing = harness.daemon.team_writes();
        let daemon = Arc::clone(&harness.daemon);
        let asking = std::thread::spawn(move || {
            rpc(
                &daemon,
                "template.apply",
                &json!({ "slug": "pair", "digest": digest }),
            )
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        // While another write holds the team, it pauses Ada (dev-a), whom Pair keeps as its only
        // Developer: worked out against this team, applying retires dev-b and leaves none active.
        let mut wire = team_file(&harness);
        wire["agents"][1]["status"] = json!("paused");
        harness
            .project
            .deps
            .files
            .write_team(&farik_core::team::validate_team(&wire).expect("a team"))
            .expect("written");
        let events = seq_count(&harness);
        drop(writing);
        let reply = asking.join().expect("the apply ends");
        assert_eq!(reply["error"]["code"], -32005, "{reply}");
        assert_eq!(
            reply["error"]["data"]["errors"][0]["code"],
            "needs_developer"
        );
        assert_eq!(team_file(&harness), wire, "nothing is written");
        assert_eq!(seq_count(&harness), events);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn retires_an_agent_the_user_only_chatted_with() {
        let (harness, folder) = templated("templates-chatted", |_| {});
        saved(&folder, &pair());
        let deps = &harness.project.deps;
        crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "dev-b".to_string(),
                author: "human".to_string(),
                text: "How is it going?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
            },
        )
        .expect("posted");
        let shown = preview(&harness, "pair");
        assert_eq!(
            json!([shown["retired"], shown["removed"]]),
            json!([["dev-b"], ["kai"]]),
            "a chat counts as work"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn applies_only_under_the_lock_that_writes_the_team() {
        let (harness, folder) = templated("templates-locked", |_| {});
        saved(&folder, &pair());
        let team = team_file(&harness);
        let criteria =
            serde_json::to_value(harness.project.deps.files.read_criteria().expect("reads"))
                .expect("JSON");
        // Each write of the team waits while another holds the lock: saving, starting, replacing,
        // applying.
        for (method, params) in [
            ("team.save", json!({ "team": team })),
            ("team.start", json!({ "team": team, "criteria": criteria })),
            (
                "agent.replace",
                json!({ "agent_id": "dev-b", "newcomer": an_agent_wire("lin", "software_developer") }),
            ),
            (
                "template.apply",
                json!({ "slug": "pair", "digest": digest(&harness, "pair") }),
            ),
        ] {
            let before = team_file(&harness);
            let writing = harness.daemon.team_writes();
            let daemon = Arc::clone(&harness.daemon);
            let asked = params.clone();
            let asking = std::thread::spawn(move || rpc(&daemon, method, &asked));
            std::thread::sleep(std::time::Duration::from_millis(300));
            assert!(
                !asking.is_finished(),
                "{method} waits while another write holds the team"
            );
            assert_eq!(team_file(&harness), before);
            drop(writing);
            let reply = asking.join().expect("the write ends");
            assert!(reply.get("result").is_some(), "{method}: {reply}");
        }
    }
}
