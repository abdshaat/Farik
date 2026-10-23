//! Moving a one-repo project's team into a project folder (`docs/SPEC.md` 4.4; phase 9 step 07b).
//!
//! `catervas serve` calls `convert` between the two drives: the old project's driver has finished
//! and let go of its lock, and the project folder's has not started. Every step is undone, in
//! reverse, when a later one before the commit point fails; the commit point is the removal of
//! the marker the project folder carries while it is being made.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas_core::contract::{TaskStatus, validate_contract};
use catervas_core::criteria::{CriteriaLibrary, CriterionSource};
use catervas_core::governor::readiness::is_a_document_path;
use catervas_core::repos::validate_repos;
use catervas_core::team::{Team, changes_code};
use catervas_protocol::event::{
    ContractWrittenBody, CriteriaUpdatedBody, EventBody, ProjectConvertedBody, RepoAddedBody,
};
use catervas_runtime::computer::on_path;
use catervas_runtime::connectors::{ConnectorSecrets, SecretAt, copy_keys};
use catervas_runtime::daemon::convert::{
    CONVERTING, Conversion, MOVED_TO, converting_from, moved_to, parent_in_home, work_in_progress,
};
use catervas_store::convert::convert_blockers;
use catervas_store::files::{FilesError, ProjectFiles};
use catervas_store::git::{CATERVAS_IDENTITY, integration_branch};
use catervas_store::repos::member_root;
use catervas_store::requests::summary_of;
use catervas_store::{CATERVAS_ACTOR, Git, open_event_log, open_projections, rescan_members};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::CliIo;
use crate::contract::event_ids;
use crate::project::{DATABASE, Project, ProjectIds};
use crate::start::{RunLock, driven_elsewhere, try_lock};
use crate::state::{home_of, state_dir};

/// How long the move waits for the old project's run lock. The driver that just finished has let
/// go of it, but a process another thread of this one forked while it was held keeps the lock
/// until that process runs `exec`, a few milliseconds under load.
#[cfg(not(test))]
pub(crate) const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
#[cfg(test)]
pub(crate) const LOCK_WAIT: std::time::Duration = std::time::Duration::from_millis(200);

/// The run lock of `root`, tried again every 50 ms for up to `wait`; `None` when something else
/// still holds it.
pub(crate) fn lock_within(
    root: &Path,
    wait: std::time::Duration,
) -> Result<Option<RunLock>, String> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Some(lock) = try_lock(root)? {
            return Ok(Some(lock));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// What the project folder's repository starts with.
const START: &str = "Start the project";

/// Under the project folder's `.catervas/local/`, where the removal branch is made.
const MOVING: &str = ".catervas/local/moving";

/// What `.catervas/` leaves behind in the copy: a worktree or a lock is the old project's alone,
/// and the markers are about this move.
const NOT_COPIED: [&str; 7] = [
    "local/worktrees",
    "local/run.lock",
    "local/daemon.json",
    "local/integration.lock",
    "local/setup-pending",
    "local/moved-to",
    "local/converting",
];

/// The role folders whose documents belong to a repository, under `docs/catervas/`.
const MEMBER_ROLE_FOLDERS: [&str; 3] = ["architecture", "engineering", "design"];

/// What the move made so far, which a failure takes back.
#[derive(Default)]
struct Made {
    folder: Option<PathBuf>,
    keys: Vec<SecretAt>,
    branch: Option<String>,
    moved_to: bool,
}

impl Made {
    /// Takes everything made back, in reverse. A step that cannot be taken back is left, and the
    /// failure that brought us here is the one said.
    fn undo(self, old_root: &Path, old_git: &Git, secrets: &dyn ConnectorSecrets) {
        if self.moved_to {
            let _ = std::fs::remove_file(old_root.join(MOVED_TO));
        }
        if let Some(folder) = &self.folder {
            let _ = old_git.prune_worktrees();
            let _ = std::fs::remove_dir_all(folder);
        }
        let _ = old_git.prune_worktrees();
        if let Some(branch) = &self.branch {
            let _ = old_git.delete_branch(branch);
        }
        for at in &self.keys {
            let _ = secrets.delete(at);
        }
    }
}

/// Moves the team of the one-repo project at `old_root` into the project folder `conversion`
/// names, and answers the folder. Nothing in the old repository's files changes; it gets the branch
/// `catervas/moved-out`, which removes what Catervas kept in it, for the owner to merge.
///
/// # Errors
///
/// A sentence saying what held the move (work in progress, a folder in the way, another process
/// driving the project) or what could not be done; everything made is taken back first.
pub(crate) fn convert(
    old_root: &Path,
    conversion: &Conversion,
    io: &CliIo<'_>,
    now: DateTime<Utc>,
) -> Result<PathBuf, String> {
    let old_root = std::fs::canonicalize(old_root)
        .map_err(|error| format!("{} cannot be read: {error}", old_root.display()))?;
    let _lock = lock_within(&old_root, LOCK_WAIT)?.ok_or_else(|| driven_elsewhere(&old_root))?;
    let old_git = Git::open(old_root.clone());
    let mut made = Made::default();
    let moved = move_team(&old_root, &old_git, conversion, io, now, &mut made);
    if moved.is_err() {
        made.undo(&old_root, &old_git, &*io.connector_secrets);
    }
    moved
}

#[allow(
    clippy::too_many_lines,
    reason = "one function in the order of the steps the plan numbers"
)]
fn move_team(
    old_root: &Path,
    old_git: &Git,
    conversion: &Conversion,
    io: &CliIo<'_>,
    now: DateTime<Utc>,
    made: &mut Made,
) -> Result<PathBuf, String> {
    let home = home_of(io);
    let old_files = ProjectFiles::open(old_root.to_path_buf());
    let team = old_files.read_team().map_err(|error| error.to_string())?;
    let integration = integration_branch(&team, old_git).map_err(|error| error.to_string())?;
    let old_text = old_root
        .to_str()
        .ok_or_else(|| format!("{} is not a path Catervas can write", old_root.display()))?
        .to_string();

    // 0. Counted again, now that nothing drives.
    count_again(old_root, io, now)?;
    let parent = parent_in_home(&home, &conversion.parent)?;
    let folder = parent.join(&conversion.name);
    clear_a_stale_move(old_root, &folder, old_git)?;
    let second = member_root(&home, &conversion.second.path).map_err(|why| why.message())?;
    let second_text = second
        .to_str()
        .ok_or_else(|| format!("{} is not a path Catervas can write", second.display()))?
        .to_string();

    // 1. The project folder, marked as unfinished.
    let empty = std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_none());
    if std::fs::symlink_metadata(&folder).is_ok() && !empty {
        return Err("a folder with that name is already there".to_string());
    }
    std::fs::create_dir_all(&parent)
        .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
    if !empty {
        std::fs::create_dir(&folder)
            .map_err(|error| format!("{} cannot be made: {error}", folder.display()))?;
    }
    made.folder = Some(folder.clone());
    let folder = std::fs::canonicalize(&folder)
        .map_err(|error| format!("{} cannot be read: {error}", folder.display()))?;
    made.folder = Some(folder.clone());
    let folder_text = folder
        .to_str()
        .ok_or_else(|| format!("{} is not a path Catervas can write", folder.display()))?
        .to_string();
    write_file(&folder.join(CONVERTING), old_text.as_bytes())?;
    git(&io.env, &folder, &["init", "-b", "main"])?;
    write_file(
        &folder.join("README.md"),
        format!(
            "# {}\n\nThe Catervas project folder for {}: its team and its documents. Its \
             repositories are listed in .catervas/repos.yaml.\n",
            conversion.name, conversion.name
        )
        .as_bytes(),
    )?;
    git(&io.env, &folder, &["add", "README.md"])?;
    git(&io.env, &folder, &["commit", "-m", START])?;

    // 2. `.catervas/`, whole.
    copy_catervas(
        &old_root.join(".catervas"),
        &folder.join(".catervas"),
        Path::new(""),
    )?;

    // 3. The connector keys, under the project folder's id.
    if let Some(state) = state_dir(&io.env) {
        made.keys = copy_keys(&*io.connector_secrets, &state, old_root, &folder, &team)
            .map_err(|error| format!("the connector keys cannot be copied: {error}"))?;
    }

    // 4. The documents.
    let moving = documents_to_move(old_git, &integration)?;
    move_documents(
        old_git,
        &integration,
        &moving,
        &folder,
        &conversion.first_name,
    )?;
    if moving
        .iter()
        .any(|entry| entry.path.starts_with("docs/catervas/"))
    {
        git(&io.env, &folder, &["add", "-f", "--", "docs"])?;
        git(
            &io.env,
            &folder,
            &[
                "commit",
                "-m",
                &format!("Move the team's documents from {}", conversion.first_name),
            ],
        )?;
    }

    // 5. The members, and the team's branch, which names the old repository's alone.
    let policy_branch: Option<String> = team
        .policy
        .integration_branch
        .as_ref()
        .map(|branch| branch.to_string());
    let new_files = ProjectFiles::open(folder.clone());
    let first = member_json(&conversion.first_name, &old_text, policy_branch.as_deref());
    let next = member_json(
        &conversion.second.name,
        &second_text,
        conversion.second.integration_branch.as_deref(),
    );
    let repos = validate_repos(&json!({ "repos": [first, next] })).map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{} {}", error.path, error.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    new_files
        .write_repos(&repos)
        .map_err(|error| error.to_string())?;
    let mut moved_team = new_files.read_team().map_err(|error| error.to_string())?;
    moved_team.policy.integration_branch = None;
    new_files
        .write_team(&moved_team)
        .map_err(|error| error.to_string())?;

    // The copied log, opened once the files it goes with are all there.
    let project = project_at(&folder, now)?;
    let ids = event_ids(&project);

    // 6. The contracts of work not yet done.
    migrate_contracts(&project, &moved_team, &conversion.first_name, now)?;

    // 7. The checks: the one repository's scan goes, the people's stay, each member's come.
    drop_the_old_scan(&project, now)?;
    rescan_members(
        &project.files,
        &project.log,
        &ids,
        now,
        None,
        CATERVAS_ACTOR,
    )
    .map_err(|error| error.to_string())?;

    // 8. In the old repository, the branch that removes what Catervas kept there: none when it
    // kept nothing in git, and the log then records no branch.
    let removed: Vec<String> = moving
        .iter()
        .filter(|entry| entry.mode != "160000")
        .map(|entry| entry.path.clone())
        .collect();
    let branch = if removed.is_empty() {
        None
    } else {
        let branch = free_branch(old_git)?;
        let scratch = folder.join(MOVING).join("removal");
        old_git
            .prune_worktrees()
            .map_err(|error| error.to_string())?;
        made.branch = Some(branch.clone());
        old_git
            .commit_removing(
                &scratch,
                &branch,
                &integration,
                &removed,
                &removal_message(&folder),
            )
            .map_err(|error| format!("the branch {branch} could not be made: {error}"))?;
        let _ = std::fs::remove_dir_all(folder.join(MOVING));
        Some(branch)
    };

    // 9. The log says so.
    for (name, path, branch_of) in [
        (
            conversion.first_name.as_str(),
            old_text.as_str(),
            policy_branch.clone(),
        ),
        (
            conversion.second.name.as_str(),
            second_text.as_str(),
            conversion.second.integration_branch.clone(),
        ),
    ] {
        let added: RepoAddedBody = serde_json::from_value(json!({
            "name": name, "path": path, "integration_branch": branch_of
        }))
        .map_err(|error| format!("a repository the log cannot hold: {error}"))?;
        let event = project.event(EventBody::RepoAdded(added), now, None)?;
        project.append(&event)?;
    }
    let converted: ProjectConvertedBody = serde_json::from_value(json!({
        "project_folder": folder_text,
        "first_repo": conversion.first_name,
        "branch": branch,
    }))
    .map_err(|error| format!("the move is not one the log can hold: {error}"))?;
    let event = project.event(EventBody::ProjectConverted(converted), now, None)?;
    project.append(&event)?;

    // 10. The old repository says where its team went.
    write_file(&old_root.join(MOVED_TO), folder_text.as_bytes())?;
    made.moved_to = true;

    // 11. The commit point.
    std::fs::remove_file(folder.join(CONVERTING))
        .map_err(|error| format!("{CONVERTING} cannot be removed: {error}"))?;
    drop(project);
    Ok(folder)
}

/// The words of the commit that removes the files, which also say which move it belongs to.
fn removal_message(folder: &Path) -> String {
    format!(
        "Remove Catervas's files: the team moved to {}",
        folder.display()
    )
}

/// `{ name, path, integration_branch? }` as `repos.yaml` writes a member.
fn member_json(name: &str, path: &str, branch: Option<&str>) -> Value {
    let mut member = json!({ "name": name, "path": path });
    if let Some(branch) = branch {
        member["integration_branch"] = json!(branch);
    }
    member
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("{} cannot be made: {error}", directory.display()))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))
}

/// Runs `git <args>` in `directory`, as catervas, with `git` from the environment's `PATH`.
fn git(env: &BTreeMap<String, String>, directory: &Path, args: &[&str]) -> Result<(), String> {
    let program = on_path("git", env).ok_or_else(|| "git is not installed".to_string())?;
    let output = std::process::Command::new(program)
        .args(CATERVAS_IDENTITY)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(directory)
        .env_clear()
        .envs(env)
        .output()
        .map_err(|error| format!("git could not be run: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Opens the project at `folder` as it is, whatever marker it carries.
fn project_at(folder: &Path, now: DateTime<Utc>) -> Result<Project, String> {
    let files = ProjectFiles::open(folder.to_path_buf());
    let team = files.read_team().map_err(|error| error.to_string())?;
    let log =
        Arc::new(open_event_log(&folder.join(DATABASE), now).map_err(|error| error.to_string())?);
    let ids = ProjectIds::of(&log, &team, folder)?;
    Ok(Project {
        root: folder.to_path_buf(),
        files,
        log,
        team,
        ids,
    })
}

/// Step 0: what holds the move, counted from the old project's own log, now that nothing drives.
fn count_again(old_root: &Path, io: &CliIo<'_>, now: DateTime<Utc>) -> Result<(), String> {
    let project = project_at(old_root, now)?;
    let projections = open_projections(Arc::clone(&project.log)).map_err(|e| e.to_string())?;
    let blockers = convert_blockers(
        &project.files,
        &project.log,
        &projections,
        &Git::open(old_root.to_path_buf()),
    )
    .map_err(|error| error.to_string())?;
    let _ = io;
    if blockers.is_empty() {
        return Ok(());
    }
    let name = old_root.file_name().map_or_else(
        || old_root.display().to_string(),
        |name| name.to_string_lossy().to_string(),
    );
    Err(work_in_progress(&name, &blockers))
}

/// What an earlier move that stopped before its commit point left: the project folder, its
/// branch in the old repository, and the note on where the team went. Worktrees whose folders are
/// gone are forgotten whatever the cause.
fn clear_a_stale_move(old_root: &Path, folder: &Path, old_git: &Git) -> Result<(), String> {
    let ours = converting_from(folder)
        .and_then(|from| std::fs::canonicalize(from).ok())
        .is_some_and(|from| from == old_root);
    if ours {
        std::fs::remove_dir_all(folder)
            .map_err(|error| format!("{} cannot be removed: {error}", folder.display()))?;
    }
    old_git
        .prune_worktrees()
        .map_err(|error| error.to_string())?;
    if ours {
        let note = removal_message(folder);
        for name in old_git.branch_names().map_err(|error| error.to_string())? {
            let ours = name == "catervas/moved-out"
                || name
                    .strip_prefix("catervas/moved-out-")
                    .is_some_and(|n| n.parse::<u64>().is_ok());
            if ours
                && old_git.has_local_branch(&name).unwrap_or(false)
                && old_git
                    .commit_subject(&name)
                    .is_ok_and(|subject| subject == note)
            {
                old_git
                    .delete_branch(&name)
                    .map_err(|error| error.to_string())?;
            }
        }
        if moved_to(old_root).is_some_and(|there| there == folder) {
            let _ = std::fs::remove_file(old_root.join(MOVED_TO));
        }
    }
    Ok(())
}

/// The first of `catervas/moved-out`, `catervas/moved-out-2`, ... that is not a local branch.
fn free_branch(old_git: &Git) -> Result<String, String> {
    let mut count = 1_u64;
    loop {
        let name = if count == 1 {
            "catervas/moved-out".to_string()
        } else {
            format!("catervas/moved-out-{count}")
        };
        if !old_git
            .has_local_branch(&name)
            .map_err(|error| error.to_string())?
        {
            return Ok(name);
        }
        count += 1;
    }
}

/// Copies `source` to `target`, files and folders alone: a link anywhere in it is left behind,
/// never followed. `relative` is where `source` is under `.catervas/`, which `NOT_COPIED` is read
/// against.
fn copy_catervas(source: &Path, target: &Path, relative: &Path) -> Result<(), String> {
    let unmade = |path: &Path, error: std::io::Error| {
        format!("{} cannot be copied: {error}", path.display())
    };
    let Ok(meta) = std::fs::symlink_metadata(source) else {
        return Ok(());
    };
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(target).map_err(|error| unmade(source, error))?;
    for entry in std::fs::read_dir(source).map_err(|error| unmade(source, error))? {
        let entry = entry.map_err(|error| unmade(source, error))?;
        let (path, kind) = (
            entry.path(),
            entry.file_type().map_err(|error| unmade(source, error))?,
        );
        let below = relative.join(entry.file_name());
        if NOT_COPIED.iter().any(|skipped| below == Path::new(skipped)) {
            continue;
        }
        let there = target.join(entry.file_name());
        if kind.is_dir() {
            copy_catervas(&path, &there, &below)?;
        } else if kind.is_file() {
            std::fs::copy(&path, &there).map_err(|error| unmade(&path, error))?;
        }
    }
    Ok(())
}

/// The tracked paths under `.catervas/` and `docs/catervas/` on `integration`: what moves, and
/// what the removal branch removes.
fn documents_to_move(
    old_git: &Git,
    integration: &str,
) -> Result<Vec<catervas_store::git::TreeEntry>, String> {
    let mut entries = old_git
        .tree_entries(integration, ".catervas")
        .map_err(|error| error.to_string())?;
    entries.extend(
        old_git
            .tree_entries(integration, "docs/catervas")
            .map_err(|error| error.to_string())?,
    );
    Ok(entries)
}

/// Where a document goes in the project folder: a repository's own role folders move under
/// `docs/catervas/repos/<first>/`, everything else stays where it is.
fn moved_path(path: &str, first: &str) -> String {
    let Some(rest) = path.strip_prefix("docs/catervas/") else {
        return path.to_string();
    };
    let (top, below) = rest.split_once('/').unwrap_or((rest, ""));
    let plans = top == "architecture" && below.starts_with("plans/");
    if MEMBER_ROLE_FOLDERS.contains(&top) && !plans {
        format!("docs/catervas/repos/{first}/{rest}")
    } else {
        path.to_string()
    }
}

/// Writes the documents of `entries` byte for byte, a link as a link, and leaves a submodule where
/// it is. `.catervas/` is not written here: the copy has all of it.
fn move_documents(
    old_git: &Git,
    integration: &str,
    entries: &[catervas_store::git::TreeEntry],
    folder: &Path,
    first: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    let _ = integration;
    for entry in entries
        .iter()
        .filter(|entry| entry.path.starts_with("docs/catervas/"))
    {
        let to = folder.join(moved_path(&entry.path, first));
        let bytes = old_git
            .blob_bytes(&entry.blob)
            .map_err(|error| error.to_string())?;
        match entry.mode.as_str() {
            "100644" | "100755" => {
                write_file(&to, &bytes)?;
                if entry.mode == "100755" {
                    std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))
                        .map_err(|error| format!("{} cannot be changed: {error}", to.display()))?;
                }
            }
            "120000" => {
                if let Some(directory) = to.parent() {
                    std::fs::create_dir_all(directory).map_err(|error| {
                        format!("{} cannot be made: {error}", directory.display())
                    })?;
                }
                let target = String::from_utf8_lossy(&bytes).to_string();
                std::os::unix::fs::symlink(&target, &to)
                    .map_err(|error| format!("{} cannot be linked: {error}", to.display()))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Step 6: the contracts of work not yet done name the folders where the documents now are, and a
/// builder's task that changes code names the repository it changes.
fn migrate_contracts(
    project: &Project,
    team: &Team,
    first: &str,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let projections = open_projections(Arc::clone(&project.log)).map_err(|e| e.to_string())?;
    let documents = team.rules().document_paths;
    for id in project
        .files
        .list_contracts()
        .map_err(|error| error.to_string())?
    {
        let Some(row) = projections.task(&id).map_err(|error| error.to_string())? else {
            continue;
        };
        let finished = row.status == TaskStatus::Cancelled
            || (row.status == TaskStatus::Accepted && !row.awaiting_integration);
        if finished {
            continue;
        }
        let contract = project
            .files
            .read_contract(&id)
            .map_err(|error| error.to_string())?;
        let mut wire = serde_json::to_value(&contract).map_err(|error| error.to_string())?;
        let mut changed = false;
        if let Some(paths) = wire["allowed_paths"].as_array_mut() {
            for path in paths {
                if let Some(text) = path.as_str() {
                    let there = moved_path(text, first);
                    if there != text {
                        *path = json!(there);
                        changed = true;
                    }
                }
            }
        }
        if let Some(criteria) = wire["exit_criteria"].as_array_mut() {
            for criterion in criteria {
                let verification = &mut criterion["verification"];
                if verification["method"] == "artifact"
                    && let Some(text) = verification["path"].as_str()
                {
                    let there = moved_path(text, first);
                    if there != text {
                        verification["path"] = json!(there);
                        changed = true;
                    }
                }
            }
        }
        let outside = wire["allowed_paths"].as_array().is_some_and(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .any(|path| !is_a_document_path(&path.to_string(), &documents))
        });
        if contract.kind == catervas_core::contract::TaskKind::Task
            && contract.repo.is_none()
            && changes_code(contract.assignee_role)
            && outside
        {
            wire["repo"] = json!(first);
            changed = true;
        }
        if !changed {
            continue;
        }
        let mut written = validate_contract(&wire).map_err(|errors| {
            format!(
                "{} cannot be moved: {}",
                id.as_str(),
                errors
                    .iter()
                    .map(|error| format!("{} {}", error.path, error.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
        written.status = row.status;
        written.updated_at = Some(now);
        project
            .files
            .write_contract(&written)
            .map_err(|error| error.to_string())?;
        let event = project.event(
            EventBody::ContractWritten(ContractWrittenBody {
                summary: summary_of(&written),
                written_by: CATERVAS_ACTOR.to_string(),
            }),
            now,
            Some(id),
        )?;
        project.append(&event)?;
    }
    Ok(())
}

/// Step 7's first half: every check the scan of the one repository found, which no member's name
/// is on, is dropped; the people's stay.
fn drop_the_old_scan(project: &Project, now: DateTime<Utc>) -> Result<(), String> {
    let existing = match project.files.read_criteria() {
        Ok(library) => library,
        Err(FilesError::NotFound { .. }) => return Ok(()),
        Err(other) => return Err(other.to_string()),
    };
    let kept: Vec<_> = existing
        .criteria
        .iter()
        .filter(|one| !(one.source == Some(CriterionSource::ProjectScan) && one.repo.is_none()))
        .cloned()
        .collect();
    if kept.len() == existing.criteria.len() {
        return Ok(());
    }
    let library = CriteriaLibrary { criteria: kept };
    project
        .files
        .write_criteria(&library)
        .map_err(|error| error.to_string())?;
    let event = project.event(
        EventBody::CriteriaUpdated(CriteriaUpdatedBody {
            criterion_names: catervas_store::names_of(&library.criteria),
            updated_by: CATERVAS_ACTOR.to_string(),
        }),
        now,
        None,
    )?;
    project.append(&event)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use catervas_core::team::fixtures::an_agent_wire;
    use catervas_core::team::validate_team;
    use catervas_protocol::clock::FixedClock;
    use catervas_protocol::event::{EventKind, NewEvent, event_from_value};
    use catervas_runtime::claude::Secret;
    use catervas_runtime::connectors::{
        ConnectorEntry, ConnectorSecrets, MemoryConnectorSecrets, SecretAt, local_project_id,
    };
    use catervas_runtime::daemon::MemberRequest;
    use catervas_runtime::daemon::convert::Conversion;
    use catervas_store::EventQuery;
    use catervas_store::files::ProjectFiles;
    use catervas_store::git::fixtures::TempRepo;
    use chrono::Utc;
    use serde_json::{Value, json};

    use super::convert;
    use crate::project::open_project;
    use crate::start::try_lock;
    use crate::{CliIo, init};

    /// Home is the temporary folder, where `TempRepo` makes its repositories.
    fn home() -> PathBuf {
        std::fs::canonicalize(std::env::temp_dir()).expect("the temporary directory")
    }

    fn named(repository: &TempRepo) -> String {
        repository
            .path
            .strip_prefix(home())
            .expect("under home")
            .display()
            .to_string()
    }

    /// The computer the move runs on: a state folder, the keys it keeps and an environment.
    struct World {
        state: PathBuf,
        secrets: Arc<MemoryConnectorSecrets>,
        env: BTreeMap<String, String>,
    }

    impl World {
        fn new(test: &str) -> Self {
            let state = std::env::temp_dir()
                .join(format!("catervas-convert-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&state);
            std::fs::create_dir_all(&state).expect("a state folder");
            let env = BTreeMap::from([
                (
                    "PATH".to_string(),
                    std::env::var("PATH").unwrap_or_default(),
                ),
                ("HOME".to_string(), home().display().to_string()),
                ("XDG_CONFIG_HOME".to_string(), state.display().to_string()),
            ]);
            Self {
                state: state.join("catervas"),
                secrets: Arc::new(MemoryConnectorSecrets::default()),
                env,
            }
        }

        fn io(&self) -> CliIo<'static> {
            let mut io = CliIo::new(
                home(),
                Box::new(std::io::sink()),
                Box::new(std::io::sink()),
                Arc::new(FixedClock::new(Utc::now())),
            );
            io.env = self.env.clone();
            io.connector_secrets = self.secrets.clone();
            io
        }

        fn at(&self, root: &Path, agent: &str, server: &str) -> SecretAt {
            std::fs::create_dir_all(&self.state).expect("a state folder");
            SecretAt {
                project_id: local_project_id(&self.state, root).expect("an id"),
                agent_id: agent.to_string(),
                server: server.to_string(),
            }
        }

        fn keep(&self, root: &Path, agent: &str, server: &str) {
            let entry = ConnectorEntry {
                spec_sha256: "a".repeat(64),
                keys: BTreeMap::from([(
                    "API_KEY".to_string(),
                    Secret::new("brave-secret".to_string()),
                )]),
                oauth: None,
            };
            self.secrets
                .save(&self.at(root, agent, server), &entry)
                .expect("kept");
        }

        fn kept(&self, root: &Path, agent: &str, server: &str) -> bool {
            self.secrets
                .load(&self.at(root, agent, server))
                .expect("read")
                .is_some()
        }
    }

    /// A parent for a test's project folder that is not there yet, named by the test.
    fn new_parent(test: &str) -> String {
        let parent = format!("catervas-convert-parent-{test}-{}/x", std::process::id());
        forget(&parent);
        parent
    }

    fn forget(parent: &str) {
        let top = parent.split('/').next().unwrap_or(parent);
        let _ = std::fs::remove_dir_all(home().join(top));
    }

    fn write(root: &Path, path: &str, bytes: &[u8]) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
        std::fs::write(file, bytes).expect("written");
    }

    /// What git prints for `args` in `folder`.
    fn git_says(folder: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(folder)
            .output()
            .expect("git runs");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Appends one event of the project at `root`, about `task` when named.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the tests build each body in the call"
    )]
    fn put(root: &Path, task: Option<&str>, kind: &str, body: Value) {
        let project = open_project(root, Utc::now()).expect("a project");
        let mut wire = json!({
            "seq": 1, "recorded_at": Utc::now().to_rfc3339(),
            "team_id": project.ids.team_id, "project_id": project.ids.project_id,
            "kind": kind, "body": body
        });
        if let Some(task) = task {
            wire["task_id"] = json!(task);
        }
        let event = event_from_value(&wire).unwrap_or_else(|e| panic!("{wire}: {e:?}"));
        project
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
    }

    /// Files `task` in `status` for `role`, allowed `paths`, with `change` applied to its wire.
    fn file_task(
        root: &Path,
        task: &str,
        (role, reviewer): (&str, &str),
        (status, paths): (&str, &[&str]),
        change: impl FnOnce(&mut Value),
    ) {
        let mut wire = catervas_core::contract::fixtures::a_contract_wire();
        wire["id"] = json!(task);
        wire["assignee_role"] = json!(role);
        wire["reviewer_role"] = json!(reviewer);
        wire["status"] = json!(status);
        wire["allowed_paths"] = json!(paths);
        change(&mut wire);
        let contract = catervas_core::contract::validate_contract(&wire).expect("a contract");
        ProjectFiles::open(root.to_path_buf())
            .create_contract(&contract)
            .expect("filed");
        put(
            root,
            Some(task),
            "task.created",
            json!({ "created_by": "human", "summary": {
                "kind": "task", "title": "A task", "status": "draft", "risk": "low"
            }}),
        );
        put(
            root,
            Some(task),
            "task.transitioned",
            json!({
                "from": "draft", "to": status, "actor": "human", "requested_by": "human",
                "gate": "none", "effects": [], "iteration": 0
            }),
        );
    }

    /// A one-repo project made by `catervas init`, integrating into `trunk`, with a Developer who
    /// holds a key, three tasks and a few documents committed on `trunk`.
    fn a_bakery(world: &World, test: &str) -> TempRepo {
        let repository = TempRepo::new(&format!("convert-{test}"));
        write(&repository.path, "Cargo.lock", b"# lock\n");
        repository.commit("a lock file");
        init::init(&repository.path, Utc::now()).expect("init");
        let files = ProjectFiles::open(repository.path.clone());
        let mut wire = serde_json::to_value(files.read_team().expect("a team")).expect("wire");
        let mut theo = an_agent_wire("theo", "software_developer");
        theo["mcp_servers"] = json!([{
            "name": "brave", "source": "custom", "transport": "stdio",
            "command": "server", "tools": { "search": "network" }
        }]);
        wire["agents"] = json!([an_agent_wire("pm", "product_manager"), theo]);
        wire["policy"]["integration_branch"] = json!("trunk");
        files
            .write_team(&validate_team(&wire).expect("a team"))
            .expect("written");
        // A scan entry from before there were members, and one a person wrote.
        let library = json!({ "criteria": [
            { "name": "old-scan", "text": "A check the one repository's scan found.",
              "source": "project_scan",
              "verification": { "method": "test", "command": "make test" } },
            { "name": "mine", "text": "A check I wrote myself, in my own words.",
              "verification": { "method": "human", "question": "Is it right?" } },
        ] });
        files
            .write_criteria(
                &catervas_core::criteria::validate_criteria(&library).expect("a library"),
            )
            .expect("written");
        let root = &repository.path;
        file_task(
            root,
            "CTV-3",
            ("software_developer", "architect"),
            ("ready", &["src/**"]),
            |_| {},
        );
        file_task(
            root,
            "CTV-4",
            ("architect", "product_manager"),
            ("refining", &["docs/catervas/architecture/overview.md"]),
            |wire| {
                wire["exit_criteria"] = json!([{
                    "id": "C1", "text": "The overview exists after the task.",
                    "verification": {
                        "method": "artifact", "path": "docs/catervas/architecture/overview.md"
                    }
                }]);
            },
        );
        file_task(
            root,
            "CTV-5",
            ("software_developer", "architect"),
            ("ready", &["docs/catervas/engineering/**"]),
            |_| {},
        );
        write(root, "docs/catervas/product/spec.html", b"<p>spec</p>");
        write(
            root,
            "docs/catervas/product/spec.agent.html",
            b"<p>agent spec</p>",
        );
        write(
            root,
            "docs/catervas/product/blob.bin",
            &[0xff, 0xfe, 0x00, 0x80],
        );
        write(
            root,
            "docs/catervas/architecture/overview.md",
            b"# Overview\n",
        );
        write(
            root,
            "docs/catervas/architecture/features.html",
            b"<p>features</p>",
        );
        write(
            root,
            "docs/catervas/architecture/plans/CTV-3.html",
            b"<p>plan</p>",
        );
        write(
            root,
            "docs/catervas/engineering/conventions.md",
            b"# Conventions\n",
        );
        write(root, "docs/catervas/notes.md", b"notes\n");
        std::os::unix::fs::symlink(
            "conventions.md",
            root.join("docs/catervas/engineering/latest.md"),
        )
        .expect("a link");
        repository.git(&["branch", "-m", "main", "trunk"]);
        repository.commit("The team and its documents");
        world.keep(root, "theo", "brave");
        repository
    }

    /// The conversion of `old` into `parent/bakery` with `second` joining as `web`.
    fn conversion(parent: &str, second: &TempRepo) -> Conversion {
        Conversion {
            first_name: "bakery".to_string(),
            parent: parent.to_string(),
            name: "bakery".to_string(),
            second: MemberRequest {
                name: "web".to_string(),
                path: named(second),
                integration_branch: None,
            },
        }
    }

    fn kinds_of(root: &Path) -> Vec<EventKind> {
        open_project(root, Utc::now())
            .expect("a project")
            .log
            .read(&EventQuery::default())
            .expect("events")
            .iter()
            .map(|event| event.body.kind())
            .collect()
    }

    fn tracked(folder: &Path, branch: &str) -> Vec<String> {
        git_says(folder, &["ls-tree", "-r", "--name-only", branch])
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one block for each thing the move carries, in the order the plan lists"
    )]
    fn moves_the_team_and_its_documents() {
        let world = World::new("moves");
        let old = a_bakery(&world, "moves");
        let web = TempRepo::new("convert-moves-web");
        write(&web.path, "Cargo.lock", b"# lock\n");
        web.commit("a lock file");
        let parent = new_parent("moves");
        // Events still in the write-ahead log when the copy is made are copied with it: another
        // connection to the log is open, so that the last one to close does not fold them in.
        let held = open_project(&old.path, Utc::now()).expect("a project");
        put(
            &old.path,
            None,
            "project.scanned",
            json!({ "detected_criteria": [], "read_back": "nothing tracked yet" }),
        );
        assert!(
            old.path.join(".catervas/local/catervas.db-wal").is_file(),
            "the write-ahead log is there"
        );
        let before = kinds_of(&old.path);

        let folder = convert(
            &old.path,
            &conversion(&parent, &web),
            &world.io(),
            Utc::now(),
        )
        .expect("moved");

        drop(held);
        assert_eq!(folder, home().join(&parent).join("bakery"));
        let catervas = folder.join(".catervas");
        assert!(catervas.join("team.yaml").is_file());
        for task in ["CTV-3", "CTV-4", "CTV-5"] {
            assert!(catervas.join(format!("contracts/{task}.yaml")).is_file());
        }
        assert!(!catervas.join("local/worktrees").exists());
        assert!(!catervas.join("local/converting").exists());
        assert!(!catervas.join("local/run.lock").exists());

        // The log has every old event, and ends with the two members and the move.
        let after = kinds_of(&folder);
        assert!(after.len() > before.len());
        assert_eq!(&after[..before.len()], &before[..]);
        assert_eq!(
            after[after.len() - 3..],
            [
                EventKind::RepoAdded,
                EventKind::RepoAdded,
                EventKind::ProjectConverted
            ]
        );

        // The documents: byte for byte, each member's under repos/<name>/.
        let held = tracked(&folder, "main");
        for path in [
            "docs/catervas/product/spec.html",
            "docs/catervas/product/spec.agent.html",
            "docs/catervas/architecture/plans/CTV-3.html",
            "docs/catervas/repos/bakery/architecture/overview.md",
            "docs/catervas/repos/bakery/architecture/features.html",
            "docs/catervas/repos/bakery/engineering/conventions.md",
            "docs/catervas/repos/bakery/engineering/latest.md",
            "docs/catervas/notes.md",
        ] {
            assert!(held.contains(&path.to_string()), "{path} in {held:?}");
        }
        assert!(
            !held
                .iter()
                .any(|path| path == "docs/catervas/architecture/overview.md"),
            "{held:?}"
        );
        assert_eq!(
            std::fs::read(folder.join("docs/catervas/product/blob.bin")).expect("moved"),
            [0xff, 0xfe, 0x00, 0x80]
        );
        let link =
            std::fs::read_link(folder.join("docs/catervas/repos/bakery/engineering/latest.md"))
                .expect("a link");
        assert_eq!(link, Path::new("conventions.md"));
        assert_eq!(
            git_says(&folder, &["log", "-1", "--format=%s", "main"]),
            "Move the team's documents from bakery"
        );

        // The members, and the team's own branch left behind.
        let project = open_project(&folder, Utc::now()).expect("a project");
        let repos = project.files.read_repos().expect("read").expect("a list");
        let members: Vec<(String, String, Option<String>)> = repos
            .repos
            .iter()
            .map(|repo| {
                (
                    repo.name.to_string(),
                    repo.path.to_string(),
                    repo.integration_branch
                        .as_ref()
                        .map(|branch| branch.as_str().to_string()),
                )
            })
            .collect();
        assert_eq!(
            members,
            [
                (
                    "bakery".to_string(),
                    std::fs::canonicalize(&old.path)
                        .expect("path")
                        .display()
                        .to_string(),
                    Some("trunk".to_string())
                ),
                (
                    "web".to_string(),
                    std::fs::canonicalize(&web.path)
                        .expect("path")
                        .display()
                        .to_string(),
                    None
                ),
            ]
        );
        assert!(project.team.policy.integration_branch.is_none());

        // The contracts: where they read, and who wrote them.
        let read = |task: &str| {
            project
                .files
                .read_contract(&task.parse().expect("id"))
                .expect("read")
        };
        assert_eq!(
            read("CTV-3")
                .repo
                .as_ref()
                .map(|repo| repo.as_str().to_string())
                .as_deref(),
            Some("bakery")
        );
        assert!(read("CTV-4").repo.is_none());
        assert_eq!(
            read("CTV-4").allowed_paths,
            ["docs/catervas/repos/bakery/architecture/overview.md"]
        );
        let artifact = serde_json::to_value(&read("CTV-4").exit_criteria).expect("wire");
        assert_eq!(
            artifact[0]["verification"]["path"],
            "docs/catervas/repos/bakery/architecture/overview.md"
        );
        assert!(read("CTV-5").repo.is_none());
        assert_eq!(
            read("CTV-5").allowed_paths,
            ["docs/catervas/repos/bakery/engineering/**"]
        );
        let written: Vec<String> = project
            .log
            .read(&EventQuery {
                kinds: vec![EventKind::ContractWritten],
                ..EventQuery::default()
            })
            .expect("events")
            .iter()
            .filter_map(|event| {
                let wire = catervas_protocol::event::event_to_value(event);
                (wire["body"]["written_by"] == "catervas")
                    .then(|| wire["task_id"].as_str().unwrap_or_default().to_string())
            })
            .collect();
        assert!(written.contains(&"CTV-3".to_string()), "{written:?}");

        // The key reads under the project folder's id.
        assert!(world.kept(&folder, "theo", "brave"));

        // The library: no scan entry of the one repository, the person's own, each member's.
        let names: Vec<String> = project
            .files
            .read_criteria()
            .expect("a library")
            .criteria
            .iter()
            .map(|one| one.name.to_string())
            .collect();
        assert!(!names.contains(&"old-scan".to_string()), "{names:?}");
        assert!(names.contains(&"mine".to_string()), "{names:?}");
        assert!(
            names.iter().any(|name| name.starts_with("bakery-")),
            "{names:?}"
        );
        assert!(
            names.iter().any(|name| name.starts_with("web-")),
            "{names:?}"
        );
        forget(&parent);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaves_the_removal_for_the_owner() {
        let world = World::new("removal");
        let old = a_bakery(&world, "removal");
        let web = TempRepo::new("convert-removal-web");
        let parent = new_parent("removal");
        let checkout = git_says(&old.path, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let tip = git_says(&old.path, &["rev-parse", "trunk"]);

        let folder = convert(
            &old.path,
            &conversion(&parent, &web),
            &world.io(),
            Utc::now(),
        )
        .expect("moved");

        // One commit past trunk, removing every tracked path under the two folders.
        assert_eq!(
            git_says(
                &old.path,
                &["rev-list", "--count", &format!("{tip}..catervas/moved-out")]
            ),
            "1"
        );
        let left = tracked(&old.path, "catervas/moved-out");
        assert!(
            !left
                .iter()
                .any(|path| path.starts_with(".catervas/") || path.starts_with("docs/catervas/")),
            "{left:?}"
        );
        assert!(left.contains(&"README.md".to_string()));
        assert!(left.contains(&"Cargo.lock".to_string()));
        let subject = git_says(
            &old.path,
            &["log", "-1", "--format=%s", "catervas/moved-out"],
        );
        assert_eq!(
            subject,
            format!(
                "Remove Catervas's files: the team moved to {}",
                folder.display()
            )
        );
        // The checkout is where it was, with its files, and nothing is left registered.
        assert_eq!(
            git_says(&old.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
            checkout
        );
        assert_eq!(git_says(&old.path, &["rev-parse", "trunk"]), tip);
        assert!(old.path.join(".catervas/team.yaml").is_file());
        assert!(old.path.join("docs/catervas/notes.md").is_file());
        assert!(!git_says(&old.path, &["worktree", "list"]).contains("moving"));
        assert_eq!(
            std::fs::read_to_string(old.path.join(".catervas/local/moved-to")).expect("written"),
            folder.display().to_string()
        );

        // Asked again for another place, the first free name is taken.
        let second_parent = new_parent("removal-again");
        let again = convert(
            &old.path,
            &Conversion {
                name: "bakery-again".to_string(),
                ..conversion(&second_parent, &web)
            },
            &world.io(),
            Utc::now(),
        );
        // The first move left the team where it is, so the second sees the same repository.
        assert!(again.is_ok(), "{again:?}");
        assert!(
            git_says(&old.path, &["branch", "--list", "catervas/moved-out-2"])
                .contains("moved-out-2")
        );
        forget(&parent);
        forget(&second_parent);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn undoes_a_failed_move() {
        let world = World::new("undo");
        let old = a_bakery(&world, "undo");
        let web = TempRepo::new("convert-undo-web");
        let parent = new_parent("undo");
        // A branch named `catervas` keeps git from making `catervas/moved-out`: step 8 fails.
        old.git(&["branch", "catervas", "trunk"]);
        let log_before = kinds_of(&old.path);
        let branches_before = git_says(&old.path, &["branch", "--list"]);
        // The project folder's id is made while the folder is there, so that it can be asked for
        // after the move has taken the folder away again.
        let folder = home().join(&parent).join("bakery");
        std::fs::create_dir_all(&folder).expect("a folder");
        let at = world.at(&folder, "theo", "brave");
        std::fs::remove_dir_all(&folder).expect("removed");

        let failed = convert(
            &old.path,
            &conversion(&parent, &web),
            &world.io(),
            Utc::now(),
        );

        assert!(failed.is_err(), "{failed:?}");
        assert!(!folder.exists());
        assert!(
            world.secrets.load(&at).expect("read").is_none(),
            "the key copied under the project folder's id is deleted"
        );
        assert_eq!(git_says(&old.path, &["branch", "--list"]), branches_before);
        assert!(!old.path.join(".catervas/local/moved-to").exists());
        assert!(!git_says(&old.path, &["worktree", "list"]).contains("moving"));
        assert_eq!(kinds_of(&old.path), log_before);
        forget(&parent);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_again_before_moving() {
        let world = World::new("again");
        let old = a_bakery(&world, "again");
        let web = TempRepo::new("convert-again-web");
        let parent = new_parent("again");
        // The run stopped, and a session that was finishing moved the task on.
        put(
            &old.path,
            Some("CTV-3"),
            "task.transitioned",
            json!({
                "from": "ready", "to": "assigned", "actor": "scrum_master",
                "requested_by": "sol", "gate": "none", "effects": [], "iteration": 0,
                "assignee": "theo"
            }),
        );

        let failed = convert(
            &old.path,
            &conversion(&parent, &web),
            &world.io(),
            Utc::now(),
        );

        let why = failed.expect_err("held");
        assert!(why.contains("CTV-3"), "{why}");
        assert!(why.starts_with("Your team is still working in"), "{why}");
        assert!(!home().join(&parent).exists(), "nothing is made");
        forget(&parent);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn finishes_a_half_made_project_folder() {
        let world = World::new("half");
        let old = a_bakery(&world, "half");
        let web = TempRepo::new("convert-half-web");
        let parent = new_parent("half");
        let asked = conversion(&parent, &web);
        let folder = convert(&old.path, &asked, &world.io(), Utc::now()).expect("moved");
        // The move stopped after step 9: the marker is back, and the repository still says it moved.
        std::fs::write(
            folder.join(".catervas/local/converting"),
            old.path.display().to_string(),
        )
        .expect("a marker");

        let again = convert(&old.path, &asked, &world.io(), Utc::now()).expect("moved again");

        assert_eq!(again, folder);
        assert!(!folder.join(".catervas/local/converting").exists());
        assert_eq!(
            git_says(&old.path, &["branch", "--list", "catervas/moved-out*"])
                .lines()
                .count(),
            1,
            "the stale branch was deleted first"
        );
        assert!(
            git_says(&old.path, &["branch", "--list", "catervas/moved-out"]).contains("moved-out")
        );
        forget(&parent);

        // Stopped after step 8's worktree was added: the folder is gone, the worktree is still
        // registered. The move prunes it and goes through.
        let parent = new_parent("half-worktree");
        let asked = conversion(&parent, &web);
        let scratch = home()
            .join(&parent)
            .join("bakery/.catervas/local/moving/scratch");
        std::fs::create_dir_all(scratch.parent().expect("a folder")).expect("made");
        old.git(&[
            "worktree",
            "add",
            "-b",
            "stranded",
            &scratch.display().to_string(),
            "trunk",
        ]);
        std::fs::remove_dir_all(home().join(&parent)).expect("the folder is gone");
        assert!(convert(&old.path, &asked, &world.io(), Utc::now()).is_ok());
        forget(&parent);

        // An empty folder at the place is used.
        let parent = new_parent("half-empty");
        std::fs::create_dir_all(home().join(&parent).join("bakery")).expect("an empty folder");
        let used = convert(
            &old.path,
            &conversion(&parent, &web),
            &world.io(),
            Utc::now(),
        );
        assert!(used.is_ok(), "{used:?}");
        forget(&parent);
    }

    #[test]
    fn refuses_a_project_another_process_drives() {
        let world = World::new("busy");
        let old =
            std::env::temp_dir().join(format!("catervas-convert-busy-{}", std::process::id()));
        std::fs::create_dir_all(&old).expect("a folder");
        let _held = try_lock(&old).expect("lock").expect("held");
        let web = TempRepo::new("convert-busy-web");

        let refused = convert(
            &old,
            &conversion("catervas-convert-busy-parent", &web),
            &world.io(),
            Utc::now(),
        );

        assert!(refused.is_err());
        let _ = std::fs::remove_dir_all(&old);
    }
}
