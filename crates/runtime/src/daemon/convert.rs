//! Adding a second repository to a one-repo project (`docs/SPEC.md` 4.4; phase 9 step 07b): what
//! holds the move, the checks that come before it, and the stop of the run that hands the team to
//! `catervas serve`, which makes the project folder between the two drives.

use std::path::{Path, PathBuf};

use catervas_core::repos::{Repo, check_member};
use catervas_protocol::command::{Command, CommandReply};
use catervas_store::Git;
use catervas_store::convert::{Blockers, convert_blockers};
use catervas_store::repos::{MemberPathRefusal, member_root};
use serde_json::{Value, json};

use super::DaemonState;
use super::gates::off_the_worker;
use super::repos::refusal;
use super::setup::MemberRequest;
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, REFUSED, UNKNOWN_QUERY};
use crate::tools::ToolDeps;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 1] = ["project.convert_blockers"];

/// The methods this module answers.
pub(super) const METHODS: [&str; 1] = ["project.convert"];

/// The marker a project folder holds while the team is being moved into it: the old root, as text.
/// The move removes it at its commit point, so a folder that still holds it is half made.
pub const CONVERTING: &str = ".catervas/local/converting";

/// What a repository the team moved out of holds: the project folder it moved to, as text.
pub const MOVED_TO: &str = ".catervas/local/moved-to";

/// What a one-repo project is asked to become: the first repository's name, the project folder's
/// place and name, and the second repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    /// What the project calls the repository it is now.
    pub first_name: String,
    /// The folder the project folder goes in, relative to home. It need not exist yet.
    pub parent: String,
    /// The project folder's name.
    pub name: String,
    /// The repository that joins it.
    pub second: MemberRequest,
}

/// What a project's name is refused with.
pub const NOT_A_NAME: &str =
    "a project's name is lowercase letters, digits, and -, up to 64 characters";

/// What a folder outside home is refused with.
pub const OUTSIDE_HOME: &str = "that folder is outside your home folder";

fn internal(error: &dyn std::fmt::Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The folder `parent`, relative to `home`, which need not exist yet: its nearest folder that does,
/// followed through its links, must be inside home. A `..`, a root or a prefix in `parent` is
/// refused before anything is read.
///
/// # Errors
///
/// A sentence: the folder is outside home, or home cannot be read.
pub fn parent_in_home(home: &Path, parent: &str) -> Result<PathBuf, String> {
    let home = std::fs::canonicalize(home)
        .map_err(|error| format!("your home folder cannot be read: {error}"))?;
    let mut parts = Vec::new();
    for component in Path::new(parent).components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part),
            std::path::Component::CurDir => {}
            _ => return Err(OUTSIDE_HOME.to_string()),
        }
    }
    let mut folder = home.clone();
    for (made, part) in parts.iter().enumerate() {
        let next = folder.join(part);
        if std::fs::symlink_metadata(&next).is_err() {
            return Ok(parts[made..]
                .iter()
                .fold(folder, |path, part| path.join(part)));
        }
        folder = std::fs::canonicalize(&next).map_err(|_| OUTSIDE_HOME.to_string())?;
        if !folder.starts_with(&home) {
            return Err(OUTSIDE_HOME.to_string());
        }
    }
    Ok(folder)
}

/// The old root a half-made project folder says it is being made from, when `folder` says so.
#[must_use]
pub fn converting_from(folder: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(folder.join(CONVERTING)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// The project folder the team of the repository at `root` moved to, when it says so.
#[must_use]
pub fn moved_to(root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(root.join(MOVED_TO)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// What holds the move, in the owner's words: "Your team is still working in <folder>: CTV-1,
/// CTV-2, the Catervafication and 2 changes to your team's notes. Let them finish, or cancel
/// them, before you add a repository."
#[must_use]
pub fn work_in_progress(folder: &str, blockers: &Blockers) -> String {
    let mut items: Vec<String> = blockers
        .tasks
        .iter()
        .map(|task| task.as_str().to_string())
        .collect();
    if blockers.catervafication {
        items.push("the Catervafication".to_string());
    }
    match blockers.folder_changes.len() {
        0 => {}
        1 => items.push("1 change to your team's notes".to_string()),
        many => items.push(format!("{many} changes to your team's notes")),
    }
    let list = match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    };
    format!(
        "Your team is still working in {folder}: {list}. Let them finish, or cancel them, before \
         you add a repository."
    )
}

/// The folder's own name, for the sentence.
fn folder_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |name| name.to_string_lossy().to_string(),
    )
}

/// The blockers, with the board caught up first.
fn blockers_of(deps: &ToolDeps) -> Result<Blockers, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    convert_blockers(&deps.files, &deps.log, &deps.projections, &deps.git).map_err(|e| internal(&e))
}

/// The queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str) -> Result<Value, Failure> {
    match name {
        "project.convert_blockers" => {
            let blockers = blockers_of(deps)?;
            Ok(json!({
                "tasks": blockers.tasks.iter().map(|task| task.as_str()).collect::<Vec<_>>(),
                "catervafication": blockers.catervafication,
                "folder_changes": blockers.folder_changes,
            }))
        }
        _ => Err(Failure::new(
            UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// `at` with `prefix` put before its pointer.
fn under(prefix: &str, pointer: &str) -> String {
    format!("{prefix}{pointer}")
}

/// Every check that comes before a move, in the order the plan lists, answered as the
/// `Conversion` they passed. Nothing is made or stored.
fn check(deps: &ToolDeps, home: &Path, params: &Value) -> Result<Conversion, Failure> {
    let text = |value: &Value, name: &str| value[name].as_str().unwrap_or_default().to_string();
    let conversion = Conversion {
        first_name: text(params, "first_name"),
        parent: text(params, "parent"),
        name: text(params, "name"),
        second: MemberRequest {
            name: text(&params["second"], "name"),
            path: text(&params["second"], "path"),
            integration_branch: params["second"]["integration_branch"]
                .as_str()
                .map(str::to_string),
        },
    };
    if deps.files.is_project_folder() {
        return Err(refusal(
            "not_one_repo",
            "/",
            "This project has several repositories already.",
        ));
    }
    let old_root = std::fs::canonicalize(deps.files.root()).map_err(|e| internal(&e))?;
    let blockers = blockers_of(deps)?;
    if !blockers.is_empty() {
        return Err(refusal(
            "work_in_progress",
            "/",
            &work_in_progress(&folder_name(&old_root), &blockers),
        ));
    }
    let named = (1..=64).contains(&conversion.name.len())
        && conversion
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !named {
        return Err(refusal("project_name", "/name", NOT_A_NAME));
    }
    let parent = parent_in_home(home, &conversion.parent)
        .map_err(|why| refusal("outside_home", "/parent", &why))?;
    let folder = parent.join(&conversion.name);
    if std::fs::symlink_metadata(&folder).is_ok() {
        let empty = std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_none());
        let ours = converting_from(&folder)
            .and_then(|from| std::fs::canonicalize(from).ok())
            .is_some_and(|from| from == old_root);
        if !empty && !ours {
            return Err(refusal(
                "folder_there",
                "/name",
                "a folder with that name is already there",
            ));
        }
    }
    let (Some(folder_text), Some(old_text)) = (folder.to_str(), old_root.to_str()) else {
        return Err(internal(&"a path that is not text"));
    };
    check_member(folder_text, &[], &conversion.first_name, old_text).map_err(|why| {
        let pointer = if why.pointer() == "/name" {
            "/first_name"
        } else {
            "/parent"
        };
        refusal(why.code(), pointer, &why.message())
    })?;
    check_second(&conversion, home, (folder_text, old_text))?;
    Ok(conversion)
}

/// The checks of the repository that joins: the old root itself, then the file system's, then
/// core's against the project folder and the first member.
fn check_second(
    conversion: &Conversion,
    home: &Path,
    (folder_text, old_text): (&str, &str),
) -> Result<(), Failure> {
    let second = &conversion.second;
    if std::fs::canonicalize(home.join(&second.path))
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .is_some_and(|path| path == old_text)
    {
        return Err(refusal(
            "path_taken",
            "/second/path",
            &format!(
                "that repository is already in the project, as {}",
                conversion.first_name
            ),
        ));
    }
    let root = member_root(home, &second.path)
        .map_err(|why| refusal(why.code(), &under("/second", why.pointer()), &why.message()))?;
    let path_text = root
        .to_str()
        .ok_or_else(|| internal(&"a path that is not text"))?;
    let first: Repo = serde_json::from_value(json!({
        "name": conversion.first_name, "path": old_text
    }))
    .map_err(|e| internal(&e))?;
    check_member(folder_text, &[first], &second.name, path_text)
        .map_err(|why| refusal(why.code(), &under("/second", why.pointer()), &why.message()))?;
    if let Some(branch) = &second.integration_branch
        && !Git::open(root)
            .has_local_branch(branch)
            .map_err(|e| internal(&e))?
    {
        let why = MemberPathRefusal::NoSuchBranch {
            branch: branch.clone(),
        };
        return Err(refusal(
            why.code(),
            &under("/second", why.pointer()),
            &why.message(),
        ));
    }
    Ok(())
}

/// `project.convert`: checks everything, stores the conversion and stops the run, as
/// `project.leave` does. `catervas serve` makes the project folder once the driver has finished.
pub(super) async fn call(
    state: &DaemonState,
    _method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    let web = super::team::web_of(state)?;
    let home = state
        .home()
        .map(Path::to_path_buf)
        .ok_or_else(|| Failure::new(INTERNAL_ERROR, "the home folder is not known"))?;
    if crate::locked(&web.converting).is_some() {
        return Err(Failure::new(
            REFUSED,
            "the team is already being moved into a project folder",
        ));
    }
    let params = params.clone();
    let conversion = off_the_worker(move || check(&deps, &home, &params)).await?;
    *crate::locked(&web.converting) = Some(conversion);
    match super::handled(state, Command::RunStop).await {
        CommandReply::Done { .. } => Ok(json!({})),
        CommandReply::Error { detail, .. } => {
            *crate::locked(&web.converting) = None;
            Err(Failure::new(REFUSED, detail))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use catervas_core::repos::validate_repos;
    use catervas_protocol::clock::FixedClock;
    use catervas_protocol::command::Command;
    use catervas_store::git::fixtures::TempRepo;
    use serde_json::{Value, json};

    use crate::daemon::DaemonState;
    use crate::daemon::gates::tests::{query, rpc};
    use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
    use crate::orchestrator::CommandReport;
    use crate::tools::fixtures::{TestProject, a_team_of_three, at};

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

    /// A one-repo project, its daemon with home set and a web state, and the commands the daemon
    /// was handed.
    struct World {
        project: TestProject,
        state: Arc<DaemonState>,
        commands: Arc<Mutex<Vec<Command>>>,
    }

    fn world(test: &str) -> World {
        let project = TestProject::new(&format!("convert-{test}"), &a_team_of_three(|_| {}));
        let state = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        assert!(state.set_home(home()));
        assert!(state.set_web(WebState {
            codes: ConnectCodes::default(),
            sessions: BrowserSessions::open(None).expect("the sessions open"),
            project_root: project.repo.path.clone(),
            account: Arc::new(crate::account::fixtures::FakeAccount::new()),
            kept_key_removed: false,
            port: 49_731,
            clock: Arc::new(FixedClock::new(at())),
            take_on_error: Mutex::default(),
            leaving: Mutex::default(),
            converting: Mutex::default(),
            env: std::collections::BTreeMap::new(),
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
        let commands: Arc<Mutex<Vec<Command>>> = Arc::default();
        let seen = Arc::clone(&commands);
        assert!(state.set_command_handler(Arc::new(move |command| {
            seen.lock().expect("the list").push(command);
            Box::pin(async {
                Ok(CommandReport {
                    said: "stopped".to_string(),
                    events: Vec::new(),
                })
            })
        })));
        World {
            project,
            state,
            commands,
        }
    }

    /// The place the project folder is asked for: a folder under home that is the test's own.
    fn parent_of(test: &str) -> String {
        format!("catervas-convert-{test}-{}", std::process::id())
    }

    fn ask(first: &str, parent: &str, name: &str, second: &Value) -> Value {
        json!({ "first_name": first, "parent": parent, "name": name, "second": second })
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
    fn answers_what_holds_the_move() {
        let world = world("blockers");
        let nothing = query(
            &world.state,
            "project.convert_blockers",
            &json!({}),
            "projectConvertBlockersResult",
        );
        assert_eq!(
            nothing,
            json!({ "tasks": [], "catervafication": false, "folder_changes": [] })
        );
        world
            .project
            .filed_with("CTV-1", "in_progress", "task", None, |wire| {
                wire["allowed_paths"] = json!(["src/**"]);
                wire["assignee"] = json!("dev-a");
            });
        let held = query(
            &world.state,
            "project.convert_blockers",
            &json!({}),
            "projectConvertBlockersResult",
        );
        assert_eq!(
            held,
            json!({ "tasks": ["CTV-1"], "catervafication": false, "folder_changes": [] })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one block for each refusal the plan lists, in its order"
    )]
    fn refuses_a_move_in_order() {
        let second = TempRepo::new("convert-order-second");
        let second_ask = json!({ "name": "web", "path": named(&second.path) });
        let parent = parent_of("order");
        let refused = |world: &World, params: &Value| {
            let reply = rpc(&world.state, "project.convert", params);
            assert!(
                world.state.converting().is_none(),
                "nothing is stored: {reply}"
            );
            assert!(
                world.commands.lock().expect("the list").is_empty(),
                "the run is not stopped: {reply}"
            );
            refusal(&reply)
        };
        let said = |code: &str, pointer: &str| (-32005, code.to_string(), pointer.to_string());
        let head = |got: (i64, String, String, String)| (got.0, got.1, got.2);

        // A project folder already.
        let folder = world("order-folder");
        folder
            .project
            .deps
            .files
            .write_repos(
                &validate_repos(&json!({ "repos": [
                    { "name": "app", "path": second.path.display().to_string() },
                    { "name": "infra", "path": folder.project.repo.path.display().to_string() },
                ] }))
                .expect("a list"),
            )
            .expect("written");
        assert_eq!(
            head(refused(
                &folder,
                &ask("app", &parent, "bakery", &second_ask)
            )),
            said("not_one_repo", "/")
        );

        // Work in progress comes before a name that is wrong.
        let busy = world("order-busy");
        busy.project
            .filed_with("CTV-1", "in_progress", "task", None, |wire| {
                wire["allowed_paths"] = json!(["src/**"]);
                wire["assignee"] = json!("dev-a");
            });
        let got = refused(&busy, &ask("app", &parent, "Shop!", &second_ask));
        assert_eq!(head(got.clone()), said("work_in_progress", "/"));
        assert!(got.3.contains("CTV-1"), "{}", got.3);
        assert!(got.3.ends_with("before you add a repository."), "{}", got.3);

        let idle = world("order-idle");
        assert_eq!(
            head(refused(&idle, &ask("app", &parent, "Shop!", &second_ask))),
            said("project_name", "/name")
        );
        assert_eq!(
            head(refused(&idle, &ask("app", "../x", "bakery", &second_ask))),
            said("outside_home", "/parent")
        );
        let there = home().join(&parent).join("bakery");
        std::fs::create_dir_all(&there).expect("a folder");
        std::fs::write(there.join("notes.txt"), "mine").expect("a file");
        assert_eq!(
            head(refused(&idle, &ask("app", &parent, "bakery", &second_ask))),
            said("folder_there", "/name")
        );
        let _ = std::fs::remove_dir_all(home().join(&parent));
        assert_eq!(
            head(refused(
                &idle,
                &ask("Corner", &parent, "bakery", &second_ask)
            )),
            said("repo_name", "/first_name")
        );
        let inside = format!("{}/inner", named(&idle.project.repo.path));
        assert_eq!(
            head(refused(&idle, &ask("app", &inside, "bakery", &second_ask))),
            said("holds_project_folder", "/parent")
        );
        let itself = json!({ "name": "web", "path": named(&idle.project.repo.path) });
        assert_eq!(
            head(refused(&idle, &ask("app", &parent, "bakery", &itself))),
            said("path_taken", "/second/path")
        );
        let with_a_team = world("order-team");
        let theirs = json!({ "name": "web", "path": named(&with_a_team.project.repo.path) });
        assert_eq!(
            head(refused(&idle, &ask("app", &parent, "bakery", &theirs))),
            said("has_team", "/second/path")
        );
        let same_name = json!({ "name": "app", "path": named(&second.path) });
        assert_eq!(
            head(refused(&idle, &ask("app", &parent, "bakery", &same_name))),
            said("name_taken", "/second/name")
        );
        assert!(!home().join(&parent).exists(), "nothing was made");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn stops_the_run_for_the_move() {
        let second = TempRepo::new("convert-stops-second");
        let world = world("stops");
        let params = ask(
            "app",
            &parent_of("stops"),
            "bakery",
            &json!({ "name": "web", "path": named(&second.path), "integration_branch": "main" }),
        );
        let reply = rpc(&world.state, "project.convert", &params);
        assert_eq!(reply["result"], json!({}), "{reply}");
        assert_eq!(
            *world.commands.lock().expect("the list"),
            [Command::RunStop]
        );
        let held = world.state.converting().expect("the conversion is stored");
        assert_eq!(held.first_name, "app");
        assert_eq!(held.name, "bakery");
        assert_eq!(held.second.name, "web");
        assert_eq!(held.second.integration_branch.as_deref(), Some("main"));
        assert!(
            !home().join(parent_of("stops")).exists(),
            "nothing is made yet"
        );
    }
}
