//! A project folder's members (`docs/SPEC.md` 4.1; ADR 0057): which repositories the team works
//! across, listed and changed. `web.rs` answers the frames; this module answers what they ask.

use std::fmt::Display;
use std::path::Path;

use catervas_core::repos::{Repo, Repos, check_member};
use catervas_protocol::event::EventBody;
use catervas_protocol::generated::event::CriteriaUpdatedBody;
use catervas_store::Git;
use catervas_store::files::FilesError;
use catervas_store::repos::{MemberPathRefusal, member_root, repo_present};
use catervas_store::scan::{CATERVAS_ACTOR, names_of, rescan_members, reseeded};
use serde_json::{Value, json};

use super::DaemonState;
use super::gates::off_the_worker;
use super::team::append;
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, NOT_FOUND, REFUSED, UNKNOWN_QUERY};
use crate::tools::ToolDeps;
use crate::transitions::integration_lock;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 1] = ["repos.list"];

/// The methods this module answers.
pub(super) const METHODS: [&str; 3] = ["repo.add", "repo.remove", "repo.locate"];

fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// A refusal the page words by `code`, at the JSON pointer `pointer`.
fn refusal(code: &str, pointer: &str, message: &str) -> Failure {
    let mut failure = Failure::new(REFUSED, message);
    failure.data =
        Some(json!({ "errors": [{ "path": pointer, "message": message, "code": code }] }));
    failure
}

fn not_a_project_folder() -> Failure {
    refusal(
        "not_a_project_folder",
        "/",
        "This project is one repository; adding another comes later.",
    )
}

fn no_such_member(name: &str) -> Failure {
    Failure::new(
        NOT_FOUND,
        format!("this project has no repository named {name}"),
    )
}

/// The queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, _params: &Value) -> Result<Value, Failure> {
    match name {
        "repos.list" => list(deps),
        _ => Err(Failure::new(
            UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// `repos.list`: the members and which are missing, or `null` for a project of one repository.
fn list(deps: &ToolDeps) -> Result<Value, Failure> {
    let Some(repos) = deps.files.read_repos().map_err(|e| internal(&e))? else {
        return Ok(json!({ "repos": null }));
    };
    let listed: Vec<Value> = repos
        .repos
        .iter()
        .map(|repo| {
            json!({
                "name": repo.name.as_str(),
                "path": repo.path.as_str(),
                "integration_branch": repo.integration_branch.as_ref().map(|branch| branch.as_str()),
                "missing": !repo_present(Path::new(repo.path.as_str())),
            })
        })
        .collect();
    Ok(json!({ "repos": listed }))
}

/// The methods, whose params the schema already passed. One writer at a time: each holds the
/// integration lock from reading `repos.yaml` to recording its event.
pub(super) async fn call(
    state: &DaemonState,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    let home = state.home().map(Path::to_path_buf);
    let (method, params) = (method.to_string(), params.clone());
    off_the_worker(move || {
        let _lock = integration_lock(deps.files.root()).map_err(|e| internal(&e))?;
        match method.as_str() {
            "repo.add" => add(&deps, home.as_deref(), &params),
            "repo.remove" => remove(&deps, &params),
            _ => locate(&deps, home.as_deref(), &params),
        }
    })
    .await
}

fn the_home(home: Option<&Path>) -> Result<&Path, Failure> {
    home.ok_or_else(|| Failure::new(INTERNAL_ERROR, "the home folder is not known"))
}

/// `repos` with `entries` as its members, held to the rules a file is.
fn with_entries(entries: &[Value]) -> Result<Repos, Failure> {
    serde_json::from_value(json!({ "repos": entries })).map_err(|e| internal(&e))
}

fn entry_of(repo: &Repo) -> Value {
    serde_json::to_value(repo).unwrap_or(Value::Null)
}

/// The checks of a member's folder and name against the others, in the order the plan lists:
/// the file system's, then core's.
fn check(
    deps: &ToolDeps,
    home: &Path,
    others: &[Repo],
    name: &str,
    path: &str,
) -> Result<String, Failure> {
    let said = |code: &str, pointer: &str, message: String| refusal(code, pointer, &message);
    let root =
        member_root(home, path).map_err(|why| said(why.code(), why.pointer(), why.message()))?;
    let folder = std::fs::canonicalize(deps.files.root()).map_err(|e| internal(&e))?;
    let (Some(folder), Some(text)) = (folder.to_str(), root.to_str()) else {
        return Err(internal(&"a path that is not text"));
    };
    check_member(folder, others, name, text)
        .map_err(|why| said(why.code(), why.pointer(), why.message()))?;
    Ok(text.to_string())
}

/// Scans the members again, `only` one of them when it is named, as Catervas's own doing. A
/// member's scan error is in the read-back; anything else answers `INTERNAL_ERROR`, and the member
/// stays added, moved or removed.
fn rescan(deps: &ToolDeps, only: Option<&str>) -> Result<(), Failure> {
    rescan_members(
        &deps.files,
        &deps.log,
        &deps.ids,
        deps.clock.now(),
        only,
        CATERVAS_ACTOR,
    )
    .map_err(|e| internal(&e))?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    Ok(())
}

/// Drops the checks the scan found in the member `name`, which no longer is one, and records the
/// library when anything went. A person's entry that names it stays.
fn drop_the_checks_of(deps: &ToolDeps, name: &str) -> Result<(), Failure> {
    let existing = match deps.files.read_criteria() {
        Ok(library) => library,
        Err(FilesError::NotFound { .. }) => return Ok(()),
        Err(other) => return Err(internal(&other)),
    };
    let (library, _) = reseeded(&[], Some(&existing), &[name]);
    if library == existing {
        return Ok(());
    }
    deps.files
        .write_criteria(&library)
        .map_err(|e| internal(&e))?;
    append(
        deps,
        EventBody::CriteriaUpdated(CriteriaUpdatedBody {
            criterion_names: names_of(&library.criteria),
            updated_by: CATERVAS_ACTOR.to_string(),
        }),
    )
}

/// `repo.add`.
fn add(deps: &ToolDeps, home: Option<&Path>, params: &Value) -> Result<Value, Failure> {
    let Some(repos) = deps.files.read_repos().map_err(|e| internal(&e))? else {
        return Err(not_a_project_folder());
    };
    let home = the_home(home)?;
    let name = params["name"].as_str().unwrap_or_default();
    let path = check(
        deps,
        home,
        &repos.repos,
        name,
        params["path"].as_str().unwrap_or_default(),
    )?;
    let branch = params["integration_branch"].as_str();
    if let Some(branch) = branch
        && !Git::open(path.clone().into())
            .has_local_branch(branch)
            .map_err(|e| internal(&e))?
    {
        let why = MemberPathRefusal::NoSuchBranch {
            branch: branch.to_string(),
        };
        return Err(refusal(why.code(), why.pointer(), &why.message()));
    }
    let mut member = json!({ "name": name, "path": path });
    if let Some(branch) = branch {
        member["integration_branch"] = json!(branch);
    }
    let mut entries: Vec<Value> = repos.repos.iter().map(entry_of).collect();
    entries.push(member);
    deps.files
        .write_repos(&with_entries(&entries)?)
        .map_err(|e| internal(&e))?;
    let body = serde_json::from_value(json!({
        "name": name, "path": path, "integration_branch": branch,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::RepoAdded(body))?;
    rescan(deps, Some(name))?;
    Ok(json!({}))
}

/// `repo.remove`.
fn remove(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let name = params["name"].as_str().unwrap_or_default();
    let repos = deps.files.read_repos().map_err(|e| internal(&e))?;
    let Some(repos) = repos.filter(|repos| repos.repos.iter().any(|r| r.name.as_str() == name))
    else {
        return Err(no_such_member(name));
    };
    if repos.repos.len() == 1 {
        return Err(refusal(
            "last_repo",
            "/name",
            "A project keeps at least one repository.",
        ));
    }
    let entries: Vec<Value> = repos
        .repos
        .iter()
        .filter(|repo| repo.name.as_str() != name)
        .map(entry_of)
        .collect();
    deps.files
        .write_repos(&with_entries(&entries)?)
        .map_err(|e| internal(&e))?;
    let body = serde_json::from_value(json!({ "name": name })).map_err(|e| internal(&e))?;
    append(deps, EventBody::RepoRemoved(body))?;
    drop_the_checks_of(deps, name)?;
    rescan(deps, None)?;
    Ok(json!({}))
}

/// `repo.locate`.
fn locate(deps: &ToolDeps, home: Option<&Path>, params: &Value) -> Result<Value, Failure> {
    let name = params["name"].as_str().unwrap_or_default();
    let repos = deps.files.read_repos().map_err(|e| internal(&e))?;
    let Some(repos) = repos.filter(|repos| repos.repos.iter().any(|r| r.name.as_str() == name))
    else {
        return Err(no_such_member(name));
    };
    let home = the_home(home)?;
    let others: Vec<Repo> = serde_json::from_value::<Vec<Repo>>(json!(
        repos
            .repos
            .iter()
            .filter(|repo| repo.name.as_str() != name)
            .map(entry_of)
            .collect::<Vec<_>>()
    ))
    .map_err(|e| internal(&e))?;
    let path = check(
        deps,
        home,
        &others,
        name,
        params["path"].as_str().unwrap_or_default(),
    )?;
    let entries: Vec<Value> = repos
        .repos
        .iter()
        .map(|repo| {
            let mut entry = entry_of(repo);
            if repo.name.as_str() == name {
                entry["path"] = json!(path);
            }
            entry
        })
        .collect();
    deps.files
        .write_repos(&with_entries(&entries)?)
        .map_err(|e| internal(&e))?;
    let body =
        serde_json::from_value(json!({ "name": name, "path": path })).map_err(|e| internal(&e))?;
    append(deps, EventBody::RepoMoved(body))?;
    rescan(deps, Some(name))?;
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use catervas_core::criteria::validate_criteria;
    use catervas_core::repos::validate_repos;
    use catervas_core::team::Team;
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_store::git::fixtures::TempRepo;
    use serde_json::{Value, json};

    use crate::daemon::DaemonState;
    use crate::daemon::gates::tests::{call, query, rpc};
    use crate::tools::fixtures::{TestProject, a_team_of_three, waits_for_the_lock_file};

    /// The folder every `TempRepo` is made in, which is home here.
    fn home() -> PathBuf {
        std::fs::canonicalize(std::env::temp_dir()).expect("the temporary directory")
    }

    /// A repository's folder as a request names it: relative to home.
    fn named(path: &Path) -> String {
        path.strip_prefix(home())
            .expect("under home")
            .display()
            .to_string()
    }

    /// A project folder (the `TestProject`'s repository), its daemon with home set, and the two
    /// repositories it lists.
    struct Folder {
        project: TestProject,
        state: Arc<DaemonState>,
        app: TempRepo,
        infra: TempRepo,
    }

    fn team() -> Team {
        a_team_of_three(|_| {})
    }

    fn daemon_of(project: &TestProject) -> Arc<DaemonState> {
        let state = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        assert!(state.set_home(home()));
        state
    }

    fn write_repos(project: &TestProject, repos: &Value) {
        project
            .deps
            .files
            .write_repos(&validate_repos(repos).expect("a list of repositories"))
            .expect("the file is written");
    }

    fn folder(test: &str) -> Folder {
        let project = TestProject::new(&format!("repos-{test}"), &team());
        let app = TempRepo::new(&format!("repos-{test}-app"));
        let infra = TempRepo::new(&format!("repos-{test}-infra"));
        write_repos(
            &project,
            &json!({ "repos": [
                { "name": "app", "path": app.path.display().to_string() },
                {
                    "name": "infra",
                    "path": infra.path.display().to_string(),
                    "integration_branch": "main"
                },
            ] }),
        );
        let state = daemon_of(&project);
        Folder {
            project,
            state,
            app,
            infra,
        }
    }

    fn listed(folder: &Folder) -> Value {
        query(&folder.state, "repos.list", &json!({}), "reposListResult")
    }

    fn repos_yaml(folder: &Folder) -> String {
        std::fs::read_to_string(folder.project.repo.path.join(".catervas/repos.yaml"))
            .expect("the file reads")
    }

    fn repo_events(folder: &Folder) -> Vec<EventBody> {
        folder
            .project
            .events(&[
                EventKind::RepoAdded,
                EventKind::RepoRemoved,
                EventKind::RepoMoved,
            ])
            .into_iter()
            .map(|event| event.body)
            .collect()
    }

    /// The one refusal a reply carries: its code, its pointer and its sentence.
    fn refusal(reply: &Value) -> (i64, String, String, String) {
        let error = &reply["error"];
        let first = &error["data"]["errors"][0];
        (
            error["code"].as_i64().expect("a code"),
            first["code"].as_str().unwrap_or_default().to_string(),
            first["path"].as_str().unwrap_or_default().to_string(),
            first["message"].as_str().unwrap_or_default().to_string(),
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_the_members_and_which_are_missing() {
        let folder = folder("lists");
        let wanted = |app_missing: bool| {
            json!({ "repos": [
                {
                    "name": "app",
                    "path": folder.app.path.display().to_string(),
                    "integration_branch": null,
                    "missing": app_missing
                },
                {
                    "name": "infra",
                    "path": folder.infra.path.display().to_string(),
                    "integration_branch": "main",
                    "missing": false
                },
            ] })
        };
        assert_eq!(listed(&folder), wanted(false));

        let away = folder.app.path.with_extension("away");
        // A run that failed before moving it back leaves this behind.
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&folder.app.path, &away).expect("the repository is moved away");
        assert_eq!(listed(&folder), wanted(true));
        std::fs::rename(&away, &folder.app.path).expect("and back");

        let one = TestProject::new("repos-lists-one", &team());
        let state = daemon_of(&one);
        assert_eq!(
            query(&state, "repos.list", &json!({}), "reposListResult"),
            json!({ "repos": null })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one block for each refusal the plan lists"
    )]
    fn adds_a_member() {
        let folder = folder("adds");
        let web = TempRepo::new("repos-adds-web");
        let before = repos_yaml(&folder);

        let nowhere = call(
            &folder.state,
            "repo.add",
            &json!({ "name": "web", "path": named(&web.path) }),
            "emptyResult",
        );
        assert_eq!(nowhere, json!({}));
        let listed = listed(&folder);
        assert_eq!(listed["repos"][2]["name"], "web");
        assert_eq!(listed["repos"][2]["path"], web.path.display().to_string());
        assert_eq!(listed["repos"][2]["integration_branch"], Value::Null);
        assert_eq!(
            repo_events(&folder),
            [serde_json::from_value::<EventBody>(json!({
                "kind": "repo.added",
                "body": {
                    "name": "web",
                    "path": web.path.display().to_string(),
                    "integration_branch": null
                }
            }))
            .expect("an event body")]
        );

        // Each refusal writes and records nothing more.
        let events_after = repo_events(&folder);
        let written = repos_yaml(&folder);
        assert_ne!(written, before);
        let reply = |params: Value| rpc(&folder.state, "repo.add", &params);
        let nothing_more = |reply: &Value, code: &str, pointer: &str, message: &str| {
            assert_eq!(
                refusal(reply),
                (
                    -32005,
                    code.to_string(),
                    pointer.to_string(),
                    message.to_string()
                ),
                "{reply}"
            );
            assert_eq!(repos_yaml(&folder), written);
            assert_eq!(repo_events(&folder), events_after);
        };
        let other = TempRepo::new("repos-adds-other");
        nothing_more(
            &reply(json!({ "name": "app", "path": named(&other.path) })),
            "name_taken",
            "/name",
            "another of the project's repositories is named app",
        );
        nothing_more(
            &reply(json!({ "name": "other", "path": "../" })),
            "outside_home",
            "/path",
            "that folder is outside your home folder",
        );
        nothing_more(
            &reply(json!({
                "name": "other", "path": named(&other.path), "integration_branch": "nope"
            })),
            "no_such_branch",
            "/integration_branch",
            "that repository has no branch named nope",
        );
        nothing_more(
            &reply(json!({
                "name": "other", "path": named(&other.path), "integration_branch": "main~0"
            })),
            "no_such_branch",
            "/integration_branch",
            "that repository has no branch named main~0",
        );
        let outer = home().join(format!("catervas-repos-outer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outer);
        std::fs::create_dir_all(outer.join("inner")).expect("folders");
        for repository in [&outer, &outer.join("inner")] {
            catervas_store::git::fixtures::git_in(repository, &["init", "-b", "main"]);
        }
        write_repos(
            &folder.project,
            &json!({ "repos": [
                { "name": "app", "path": folder.app.path.display().to_string() },
                { "name": "inner", "path": outer.join("inner").display().to_string() },
            ] }),
        );
        let written = repos_yaml(&folder);
        let nested = reply(json!({ "name": "outer", "path": named(&outer) }));
        assert_eq!(
            refusal(&nested),
            (
                -32005,
                "nested".to_string(),
                "/path".to_string(),
                "that folder and inner, already in the project, are one inside the other"
                    .to_string()
            ),
            "{nested}"
        );
        assert_eq!(repos_yaml(&folder), written);
        let _ = std::fs::remove_dir_all(&outer);

        let sixteen: Vec<Value> = (0..16)
            .map(|n| json!({ "name": format!("r{n}"), "path": format!("/nowhere/r{n}") }))
            .collect();
        write_repos(&folder.project, &json!({ "repos": sixteen }));
        let full = reply(json!({ "name": "web", "path": named(&web.path) }));
        assert_eq!(
            refusal(&full),
            (
                -32005,
                "too_many_repos".to_string(),
                "/name".to_string(),
                "a project holds at most 16 repositories".to_string()
            ),
            "{full}"
        );
        assert_eq!(repo_events(&folder), events_after);

        let one = TestProject::new("repos-adds-one", &team());
        let state = daemon_of(&one);
        let reply = rpc(
            &state,
            "repo.add",
            &json!({ "name": "web", "path": named(&web.path) }),
        );
        assert_eq!(
            refusal(&reply),
            (
                -32005,
                "not_a_project_folder".to_string(),
                "/".to_string(),
                "This project is one repository; adding another comes later.".to_string()
            ),
            "{reply}"
        );
        assert!(!one.repo.path.join(".catervas/repos.yaml").exists());
    }

    /// `repository` made a vitest project, as the scan reads one.
    fn made_a_vitest_project(repository: &TempRepo) {
        repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
        repository.write(
            "package.json",
            "{\"name\":\"member\",\"scripts\":{\"test\":\"vitest\"}}\n",
        );
        repository.write("src/index.ts", "export const one = 1;\n");
        repository.commit("a vitest project");
    }

    /// The library `folder` holds, as `(name, text)` pairs in file order.
    fn library_of(folder: &Folder) -> Vec<(String, String)> {
        folder
            .project
            .deps
            .files
            .read_criteria()
            .expect("the library reads")
            .criteria
            .iter()
            .map(|one| (one.name.to_string(), one.text.to_string()))
            .collect()
    }

    fn texts(library: &[(String, String)], name: &str) -> Option<String> {
        library
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, text)| text.clone())
    }

    /// The names a `criteria.updated` event lists, newest last, and who recorded it.
    fn criteria_updates(folder: &Folder) -> Vec<(Vec<String>, String)> {
        folder
            .project
            .events(&[EventKind::CriteriaUpdated])
            .into_iter()
            .filter_map(|event| match event.body {
                EventBody::CriteriaUpdated(body) => Some((body.criterion_names, body.updated_by)),
                _ => None,
            })
            .collect()
    }

    fn scans(folder: &Folder) -> usize {
        folder.project.events(&[EventKind::ProjectScanned]).len()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one block for each of add, locate and remove"
    )]
    fn scans_a_member_as_it_joins_and_leaves() {
        let folder = folder("scans");
        made_a_vitest_project(&folder.app);
        // What an earlier scan left: a check of each member, and one a person wrote about web.
        let library = validate_criteria(&json!({ "criteria": [
            { "name": "app-the-tests-pass", "text": "Stale app text, to be rewritten.",
              "source": "project_scan", "repo": "app",
              "verification": { "method": "test", "command": "npm run test" } },
            { "name": "infra-the-tests-pass", "text": "Stale infra text, to be kept.",
              "source": "project_scan", "repo": "infra",
              "verification": { "method": "test", "command": "cargo test" } },
            { "name": "shop-check", "text": "A person wrote this one about web.",
              "source": "human", "repo": "web",
              "verification": { "method": "human", "question": "Does the shop work?" } },
        ]}))
        .expect("a library");
        folder
            .project
            .deps
            .files
            .write_criteria(&library)
            .expect("the library is written");

        // Add: the new member's checks join; the others are untouched.
        let web = TempRepo::new("repos-scans-web");
        made_a_vitest_project(&web);
        call(
            &folder.state,
            "repo.add",
            &json!({ "name": "web", "path": named(&web.path) }),
            "emptyResult",
        );
        let held = library_of(&folder);
        assert_eq!(
            texts(&held, "web-the-tests-pass").as_deref(),
            Some("Every test passes: the project's own test script.")
        );
        assert_eq!(
            texts(&held, "app-the-tests-pass").as_deref(),
            Some("Stale app text, to be rewritten."),
            "app was not scanned"
        );
        assert_eq!(scans(&folder), 1);
        let updates = criteria_updates(&folder);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].1, "catervas");
        assert!(updates[0].0.contains(&"web-the-tests-pass".to_string()));

        // Locate: the member at its new path is scanned alone.
        let moved = folder.app.path.with_extension("moved");
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&folder.app.path, &moved).expect("app is moved");
        call(
            &folder.state,
            "repo.locate",
            &json!({ "name": "app", "path": named(&moved) }),
            "emptyResult",
        );
        let held = library_of(&folder);
        assert_eq!(
            texts(&held, "app-the-tests-pass").as_deref(),
            Some("Every test passes: the project's own test script.")
        );
        assert_eq!(
            texts(&held, "infra-the-tests-pass").as_deref(),
            Some("Stale infra text, to be kept."),
            "infra was not scanned"
        );
        assert_eq!(scans(&folder), 2);

        // Remove: the member's checks go, a person's stay, and project.md forgets it.
        call(
            &folder.state,
            "repo.remove",
            &json!({ "name": "web" }),
            "emptyResult",
        );
        let held = library_of(&folder);
        assert_eq!(texts(&held, "web-the-tests-pass"), None);
        assert!(
            texts(&held, "shop-check").is_some(),
            "a person's entry stays"
        );
        assert!(texts(&held, "app-the-tests-pass").is_some());
        let updates = criteria_updates(&folder);
        assert!(updates.len() >= 3, "{updates:?}");
        let last = updates.last().expect("an update");
        assert_eq!(last.1, "catervas");
        assert!(!last.0.contains(&"web-the-tests-pass".to_string()));
        assert_eq!(scans(&folder), 3);
        let document = folder
            .project
            .deps
            .files
            .read_project_scan()
            .expect("project.md reads");
        assert!(!document.contains("web:"), "{document}");
        assert!(!document.contains("web-the-tests-pass"), "{document}");
        assert!(document.contains("app: "), "{document}");
        let _ = std::fs::remove_dir_all(&moved);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn needs_the_home_folder_to_add_or_locate() {
        let folder = folder("no-home");
        // The same project with a daemon that was never told where home is.
        let state = Arc::new(DaemonState::new(Arc::clone(&folder.project.deps)));
        let web = TempRepo::new("repos-no-home-web");
        let before = repos_yaml(&folder);
        let events_before = repo_events(&folder);
        for (method, params) in [
            (
                "repo.add",
                json!({ "name": "web", "path": named(&web.path) }),
            ),
            (
                "repo.locate",
                json!({ "name": "app", "path": named(&web.path) }),
            ),
        ] {
            let reply = rpc(&state, method, &params);
            assert_eq!(reply["error"]["code"], -32603, "{method}: {reply}");
            assert_eq!(
                reply["error"]["message"], "the home folder is not known",
                "{method}: {reply}"
            );
            assert_eq!(repos_yaml(&folder), before, "{method}");
            assert_eq!(repo_events(&folder), events_before, "{method}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn removes_a_member_but_not_the_last() {
        let folder = folder("removes");
        let removed = call(
            &folder.state,
            "repo.remove",
            &json!({ "name": "app" }),
            "emptyResult",
        );
        assert_eq!(removed, json!({}));
        let names: Vec<Value> = listed(&folder)["repos"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|repo| repo["name"].clone())
            .collect();
        assert_eq!(names, [json!("infra")]);
        assert_eq!(
            repo_events(&folder),
            [serde_json::from_value::<EventBody>(
                json!({ "kind": "repo.removed", "body": { "name": "app" } })
            )
            .expect("an event body")]
        );
        assert!(
            folder.app.path.join(".git").is_dir(),
            "the folder is untouched"
        );

        let last = rpc(&folder.state, "repo.remove", &json!({ "name": "infra" }));
        assert_eq!(
            refusal(&last),
            (
                -32005,
                "last_repo".to_string(),
                "/name".to_string(),
                "A project keeps at least one repository.".to_string()
            ),
            "{last}"
        );
        let unknown = rpc(&folder.state, "repo.remove", &json!({ "name": "nope" }));
        assert_eq!(unknown["error"]["code"], -32002, "{unknown}");
        assert_eq!(repo_events(&folder).len(), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn locates_a_moved_member() {
        let folder = folder("locates");
        let apps = home().join(format!("catervas-repos-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&apps);
        std::fs::create_dir_all(&apps).expect("a folder");
        let moved = apps.join("app");
        std::fs::rename(&folder.app.path, &moved).expect("the repository is moved");
        assert_eq!(listed(&folder)["repos"][0]["missing"], true);

        let located = call(
            &folder.state,
            "repo.locate",
            &json!({ "name": "app", "path": named(&moved) }),
            "emptyResult",
        );
        assert_eq!(located, json!({}));
        assert_eq!(
            listed(&folder)["repos"][0]["path"],
            moved.display().to_string()
        );
        assert_eq!(listed(&folder)["repos"][0]["missing"], false);
        assert_eq!(
            repo_events(&folder),
            [serde_json::from_value::<EventBody>(json!({
                "kind": "repo.moved",
                "body": { "name": "app", "path": moved.display().to_string() }
            }))
            .expect("an event body")]
        );

        // Another member's folder is taken; its own, present, is accepted.
        let taken = rpc(
            &folder.state,
            "repo.locate",
            &json!({ "name": "app", "path": named(&folder.infra.path) }),
        );
        assert_eq!(
            refusal(&taken),
            (
                -32005,
                "path_taken".to_string(),
                "/path".to_string(),
                "that repository is already in the project, as infra".to_string()
            ),
            "{taken}"
        );
        assert_eq!(repo_events(&folder).len(), 1);
        let again = call(
            &folder.state,
            "repo.locate",
            &json!({ "name": "app", "path": named(&moved) }),
            "emptyResult",
        );
        assert_eq!(again, json!({}));
        let unknown = rpc(
            &folder.state,
            "repo.locate",
            &json!({ "name": "nope", "path": named(&moved) }),
        );
        assert_eq!(unknown["error"]["code"], -32002, "{unknown}");
        let _ = std::fs::remove_dir_all(&apps);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn changes_the_members_one_at_a_time() {
        let folder = folder("one-at-a-time");
        let web = TempRepo::new("repos-one-at-a-time-web");
        let before = repos_yaml(&folder);
        let params = json!({ "name": "web", "path": named(&web.path) });

        let answered = waits_for_the_lock_file(&folder.project.repo.path, &folder.project, || {
            rpc(&folder.state, "repo.add", &params)
        });

        assert_eq!(answered["result"], json!({}), "{answered}");
        assert_ne!(repos_yaml(&folder), before);
        assert_eq!(repo_events(&folder).len(), 1);
    }
}
