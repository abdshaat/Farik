//! The git tools, on the task's worktree (ADR 0004): four tools rather than one, so that each
//! carries one tier and is checked as its own tool.

use std::path::PathBuf;

use catervas_core::branch::task_branch;
use catervas_core::contract::TaskId;
use catervas_core::forge_text::task_id_in;
use catervas_core::governor::paths::{PathRefusal, check_allowed_paths, normalise};
use catervas_core::governor::permissions::ToolRefusal;
use catervas_store::git::task_base;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::channel::post_system;

/// The remote a task branch is pushed to.
const REMOTE: &str = "origin";

/// `catervas_git_commit`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitInput {
    /// The commit message.
    message: String,
    /// The paths to commit, relative to the worktree.
    paths: Vec<String>,
    /// Paths, relative to the worktree, to delete and commit as removed.
    #[serde(default)]
    removed: Vec<String>,
}

/// `catervas_git_status`: what is uncommitted in the task's worktree, and whether nothing is.
pub(super) fn status(call: &Call<'_>) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, _) = call.contract(task)?;
    let status = call
        .task_repository(&contract)?
        .git
        .status(&worktree(call, task))
        .map_err(failed)?;
    Ok(json!({ "clean": status.is_empty(), "status": status }))
}

/// `catervas_git_diff`: what the task branch changed since the integration branch, as a patch.
pub(super) fn diff(call: &Call<'_>) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, _) = call.contract(task)?;
    let repository = call.task_repository(&contract)?;
    let deps = call.deps();
    let diff = repository
        .git
        .diff(
            &task_base(&repository.git, &repository.integration_branch, task),
            &crate::orchestrator::rev_of(deps, &call.team, &contract),
        )
        .map_err(failed)?;
    Ok(json!({ "diff": diff }))
}

/// `catervas_git_commit`: commits the named paths of the task's worktree on the task branch, for the
/// task's assignee alone. A path that is a directory is refused rather than staged whole: the path
/// checks of 5.6 judged the directory's name, not the files under it, and a protected one among
/// them would be committed.
pub(super) fn commit(call: &Call<'_>, input: &CommitInput) -> Result<Value, ToolError> {
    let task = assignees_task(call)?;
    // A task id stays inside Catervas (ADR 0066); checked first, so a refusal changes nothing.
    if let Some(id) = task_id_in(&input.message) {
        return Err(Refusal::CommitMessageNamesATask { id }.into());
    }
    let worktree = worktree(call, task);
    if let Some(path) = input.paths.iter().find(|path| worktree.join(path).is_dir()) {
        return Err(Refusal::PathIsADirectory { path: path.clone() }.into());
    }
    // A merge Catervas left for the agent to resolve (ADR 0069) must not be committed with its
    // markers still in the files.
    if let Some(path) = input
        .paths
        .iter()
        .find(|path| has_conflict_markers(&worktree.join(path)))
    {
        return Err(Refusal::ConflictMarkers { path: path.clone() }.into());
    }
    let removed = removals(&worktree, &input.removed)?;
    // The team's documents kept on this computer are never committed (ADR 0074).
    if catervas_store::document_source::documents_on_disk(&call.team, &call.deps().log)
        .map_err(failed)?
        && let Some(path) = input
            .paths
            .iter()
            .chain(&removed)
            .filter_map(|path| normalise(path))
            .find(|path| path.starts_with("docs/catervas/"))
    {
        return Err(Refusal::DocumentsStayLocal { path }.into());
    }
    // A deletion is not undone by a refused verification, so the contract's paths are checked now.
    let (contract, _) = call.contract(task)?;
    let allowed: Vec<String> = contract
        .allowed_paths
        .iter()
        .map(ToString::to_string)
        .collect();
    if let Err(PathRefusal::Violations(violations)) = check_allowed_paths(&removed, &allowed) {
        let path = violations
            .into_iter()
            .next()
            .map(|v| v.path)
            .unwrap_or_default();
        return Err(Refusal::Tool(ToolRefusal::PathOutsideAllowed { path }).into());
    }
    for path in &removed {
        std::fs::remove_file(worktree.join(path)).map_err(failed)?;
    }
    let deps = call.deps();
    let repository = call.task_repository(&contract)?;
    let staged: Vec<String> = input.paths.iter().chain(&removed).cloned().collect();
    match repository.git.commit(&worktree, &input.message, &staged) {
        Ok(sha) => Ok(json!({ "sha": sha })),
        Err(error) => {
            // The agent is told in the tool's answer, which it may ignore; a task whose work is
            // not committed never reaches verification, so the user is told too (5.14).
            post_system(
                &deps.log,
                deps.clock.as_ref(),
                &call.ids(Some(task)),
                Some(task.clone()),
                &format!(
                    "{} could not be committed: {}",
                    task.as_str(),
                    why_in_words(&error)
                ),
            )
            .map_err(failed)?;
            Err(failed(error))
        }
    }
}

/// Why git refused, for the channel: the lines of what it said that name a failure, else its
/// first, without colour codes. A hook's whole log (its banner, an install's progress, timings)
/// would bury the reason; the agent still gets git's full answer.
/// ponytail: a failure is found by its words; a hook whose reason uses none of them shows its first line.
fn why_in_words(error: &catervas_store::git::GitError) -> String {
    let catervas_store::git::GitError::CommandFailed { stderr, .. } = error else {
        return error.to_string();
    };
    let plain = without_colour(stderr);
    let lines: Vec<&str> = plain
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().any(char::is_alphanumeric))
        .collect();
    let failing: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_lowercase();
            [
                "error", "fatal", "fail", "refus", "reject", "denied", "invalid", " is not ",
            ]
            .iter()
            .any(|word| lower.contains(word))
        })
        .collect();
    match (failing.is_empty(), lines.first()) {
        (false, _) => failing.join(" "),
        (true, Some(first)) => (*first).to_string(),
        (true, None) => error.to_string(),
    }
}

/// `text` without its terminal escape sequences (`ESC [` up to a letter).
fn without_colour(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

/// Whether the file at `path` has a line that opens or closes a conflict (`<<<<<<< ` or
/// `>>>>>>> `). A file that is missing or is not text has none.
fn has_conflict_markers(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| {
        text.lines()
            .any(|line| line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> "))
    })
}

/// The removed paths, normalised, once each is a regular file inside the worktree with no link on
/// the way; any other is refused before anything is deleted.
fn removals(worktree: &std::path::Path, removed: &[String]) -> Result<Vec<String>, ToolError> {
    let mut checked = Vec::new();
    for path in removed {
        let refuse = |why: &str| failed(format!("{path} cannot be removed: {why}"));
        let relative = normalise(path).ok_or_else(|| refuse("it is not inside the worktree"))?;
        let mut at = worktree.to_path_buf();
        for segment in relative.split('/') {
            at.push(segment);
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(refuse("a link is on the way"));
                }
                Ok(_) => {}
                Err(_) => return Err(refuse("there is no such file")),
            }
        }
        if at.is_dir() {
            return Err(Refusal::PathIsADirectory { path: path.clone() }.into());
        }
        checked.push(relative);
    }
    Ok(checked)
}

/// `catervas_git_push`: pushes the task branch to `origin`, for the task's assignee alone. It spells
/// `refs/heads/<branch>`, as integration does, so that a tag of the same name cannot make the push
/// ambiguous.
pub(super) fn push(call: &Call<'_>) -> Result<Value, ToolError> {
    let task = assignees_task(call)?;
    let (contract, _) = call.contract(task)?;
    if catervas_store::document_source::is_branchless(&call.team, &call.deps().log, &contract)
        .map_err(failed)?
    {
        return Err(failed(
            "this task has no branch: its documents are not committed",
        ));
    }
    let branch = task_branch(&contract);
    call.task_repository(&contract)?
        .git
        .push(REMOTE, &format!("refs/heads/{branch}"))
        .map_err(failed)?;
    Ok(json!({ "remote": REMOTE, "branch": branch }))
}

/// The session's task when the caller is its assignee, or `not_the_named_agent`: nobody rewrites
/// work they grade (5.1), so the reviewer, who works in the same worktree, cannot change it.
fn assignees_task<'a>(call: &'a Call<'_>) -> Result<&'a TaskId, ToolError> {
    let task = call.task()?;
    let (contract, _) = call.contract(task)?;
    if contract.assignee.as_deref() == Some(call.agent_id()) {
        return Ok(task);
    }
    Err(Refusal::NotTheNamedAgent {
        agent_id: call.agent_id().to_string(),
        task_id: task.to_string(),
        assignee: contract.assignee,
    }
    .into())
}

/// The task's worktree, `.catervas/local/worktrees/<id>` (5.14).
pub(super) fn worktree(call: &Call<'_>, task: &TaskId) -> PathBuf {
    call.deps()
        .files
        .root()
        .join(".catervas/local/worktrees")
        .join(task.as_str())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    #[test]
    fn tells_the_channel_why_a_commit_was_refused_without_the_hooks_log() {
        // What lefthook printed when a commit's subject had no scope, as git passed it on.
        let stderr = "\u{1b}[38;2;56;56;56m╭──────────────╮\u{1b}[m\n\
            \u{1b}[38;2;56;56;56m│\u{1b}[m 🥊 lefthook  v2.2.1   hook:  \u{1b}[1mpre-commit\u{1b}[m \u{1b}[38;2;56;56;56m│\u{1b}[m\n\
            \u{1b}[38;2;56;56;56m╰──────────────╯\u{1b}[m\n\
            ┃  pre-commit ❯\n\
            Packages: +543\n\
            ++++++++++++++++++++++++++++++++\n\
            Progress: resolved 543, reused 543, downloaded 0, added 543, done\n\
            Checked 1411 files in 204ms. No fixes applied.\n\
            \u{1b}[38;2;56;56;56m  ────────────────────\u{1b}[m\n\
            summary: (done in 0.99 seconds)\n\
            ✔️ pre-commit (0.99 seconds)\n\
            ┃  commit-msg ❯\n\
            $ node tools/check/src/tools.ts commit-msg /home/x/.git/worktrees/CTV-14/COMMIT_EDITMSG\n\
            commit-msg: subject \"docs: add delivery 1\" is not <type>(<scope>): <subject>\n\
            [ELIFECYCLE] Command failed with exit code 1.\n\
            \n\
            exit status 1\n\
            summary: (done in 0.14 seconds)\n\
            🥊 commit-msg (0.14 seconds)";
        let error = catervas_store::git::GitError::CommandFailed {
            command: "commit -m docs: add delivery 1".to_string(),
            stderr: stderr.to_string(),
        };
        assert_eq!(
            super::why_in_words(&error),
            "commit-msg: subject \"docs: add delivery 1\" is not <type>(<scope>): <subject> \
             [ELIFECYCLE] Command failed with exit code 1."
        );
        // git's own refusal is its first line; nothing it says is lost when no line names a failure.
        let error = catervas_store::git::GitError::CommandFailed {
            command: "commit".to_string(),
            stderr: "fatal: pathspec 'src/login/form.ts' did not match any files".to_string(),
        };
        assert_eq!(
            super::why_in_words(&error),
            "fatal: pathspec 'src/login/form.ts' did not match any files"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_commit_message_naming_a_task() {
        let project = TestProject::new("tools-git-commit-id", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        let branch = project.branch("CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &branch, "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        let commit = |message: &str| {
            project.call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": message, "paths": ["src/login/form.ts"] }),
            )
        };

        match commit("feat: add the login form (CTV-1)") {
            Err(ToolError::Refused { reason }) => assert!(
                reason.starts_with("commit_message_names_a_task: CTV-1 is a task id"),
                "{reason}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
        let git = &project.deps.git;
        assert_eq!(git.commit_count("main", &branch), Ok(0));

        let committed = commit("feat: add the login form").expect("a message with no id commits");
        assert!(committed["sha"].is_string(), "{committed}");
        assert_eq!(git.commit_count("main", &branch), Ok(1));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn commits_through_the_tool_on_the_tasks_worktree() {
        let project = TestProject::new("tools-git-commit", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");

        let status = project
            .call("dev-a", Some("CTV-1"), "catervas_git_status", json!({}))
            .expect("the status reads");
        assert_eq!(status["clean"], false);
        let committed = project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "add the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");
        let head = catervas_store::git::fixtures::git_output_in(&worktree, &["rev-parse", "HEAD"]);
        assert_eq!(committed["sha"], head);

        let status = project
            .call("dev-a", Some("CTV-1"), "catervas_git_status", json!({}))
            .expect("the status reads");
        assert_eq!(status["clean"], true, "{status}");
        let diff = project
            .call("dev-a", Some("CTV-1"), "catervas_git_diff", json!({}))
            .expect("the diff reads");
        assert!(
            diff["diff"]
                .as_str()
                .expect("a patch")
                .contains("b/src/login/form.ts"),
            "{diff}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_commit_a_file_with_conflict_markers() {
        let project = TestProject::new("tools-git-markers", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        let form = worktree.join("src/login/form.ts");
        std::fs::write(
            &form,
            "<<<<<<< HEAD\nexport const a = 1;\n=======\nexport const a = 2;\n>>>>>>> main\n",
        )
        .expect("a file");
        let head = catervas_store::git::fixtures::git_output_in(&worktree, &["rev-parse", "HEAD"]);
        let commit = json!({ "message": "resolve it", "paths": ["src/login/form.ts"] });

        let refused = project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                commit.clone(),
            )
            .expect_err("markers are refused");

        assert!(
            format!("{refused:?}").contains("conflict_markers"),
            "{refused:?}"
        );
        assert_eq!(
            catervas_store::git::fixtures::git_output_in(&worktree, &["rev-parse", "HEAD"]),
            head,
            "no commit was made"
        );

        std::fs::write(&form, "export const a = 2;\n").expect("resolved");
        project
            .call("dev-a", Some("CTV-1"), "catervas_git_commit", commit)
            .expect("the resolved file commits");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_git_tools_act_on_the_member() {
        use catervas_store::git::fixtures::{git_in, git_output_in};
        let (project, members) = TestProject::project_folder(
            "tools-git-member",
            &a_team_of_three(|wire| wire["agents"][1]["grants"] = json!(["git_remote"])),
            &["app", "infra"],
        );
        let app = &members[0];
        let origin = app.path.with_extension("origin.git");
        let _ = std::fs::remove_dir_all(&origin);
        std::fs::create_dir_all(&origin).expect("a directory for the remote");
        git_in(&origin, &["init", "--bare", "-b", "main"]);
        app.git(&["remote", "add", "origin", origin.to_str().expect("a path")]);
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["repo"] = json!("app");
            wire["allowed_paths"] = json!(["src/**"]);
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        app.adapter()
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the member makes the task's worktree");
        std::fs::create_dir_all(worktree.join("src")).expect("a directory");
        std::fs::write(worktree.join("src/a.rs"), "fn a() {}\n").expect("a file");

        let status = project
            .call("dev-a", Some("CTV-1"), "catervas_git_status", json!({}))
            .expect("the status reads");
        assert_eq!(status["clean"], false, "{status}");
        let committed = project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "add a", "paths": ["src/a.rs"] }),
            )
            .expect("the assignee commits");
        let head = git_output_in(&worktree, &["rev-parse", "HEAD"]);
        assert_eq!(committed["sha"], head);
        assert_eq!(
            git_output_in(&app.path, &["rev-parse", "feature/CTV-1"]),
            head
        );
        let diff = project
            .call("dev-a", Some("CTV-1"), "catervas_git_diff", json!({}))
            .expect("the diff reads");
        assert!(
            diff["diff"]
                .as_str()
                .expect("a patch")
                .contains("b/src/a.rs"),
            "{diff}"
        );
        project
            .call("dev-a", Some("CTV-1"), "catervas_git_push", json!({}))
            .expect("the assignee pushes to the member's origin");
        assert_eq!(
            git_output_in(&origin, &["rev-parse", "feature/CTV-1"]),
            head
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_in_the_channel_why_a_commit_failed() {
        // A commit that fails leaves the task in `in_progress` with nothing for verification to
        // check, so the user is told why as well as the agent.
        use catervas_protocol::event::{EventBody, EventKind, MessageKind};
        let project = TestProject::new("tools-git-commit-fails", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");

        let refused = project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "add the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect_err("there is nothing at that path to commit");
        let ToolError::Failed { detail } = &refused else {
            panic!("a failure: {refused:?}");
        };
        assert!(detail.contains("src/login/form.ts"), "{detail}");

        let posted = project.events(&[EventKind::MessagePosted]);
        let [line] = posted.as_slice() else {
            panic!("one line: {posted:?}");
        };
        let EventBody::MessagePosted(body) = &line.body else {
            panic!("a message: {line:?}");
        };
        assert_eq!(body.author, "catervas");
        assert_eq!(body.kind, MessageKind::System);
        assert_eq!(
            line.envelope.ids.task_id.as_ref().map(|id| id.as_str()),
            Some("CTV-1")
        );
        assert!(
            body.text.starts_with("CTV-1 could not be committed: ")
                && body.text.contains("src/login/form.ts"),
            "{}",
            body.text
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn diffs_the_tasks_branch_through_the_tool() {
        // A fix works on `fix/<id>` (5.14), so the diff is of that branch.
        let project = TestProject::new("tools-git-diff-fix", &a_team_of_three(|_| {}));
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["change"] = json!("fix");
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, "fix/CTV-1", "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "fix the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");

        let diff = project
            .call("dev-a", Some("CTV-1"), "catervas_git_diff", json!({}))
            .expect("the diff reads");
        assert!(
            diff["diff"]
                .as_str()
                .expect("a patch")
                .contains("b/src/login/form.ts"),
            "{diff}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn pushes_the_tasks_branch_through_the_tool() {
        // A fix pushes `fix/<id>` (5.14). A tag of the same name makes a push that does not spell
        // `refs/heads/` ambiguous, as integration found (integrate.rs).
        use catervas_store::git::fixtures::{git_in, git_output_in};
        let project = TestProject::new(
            "tools-git-push",
            &a_team_of_three(|wire| wire["agents"][1]["grants"] = json!(["git_remote"])),
        );
        let root = &project.repo.path;
        let origin = root.with_extension("origin.git");
        let _ = std::fs::remove_dir_all(&origin);
        std::fs::create_dir_all(&origin).expect("a directory for the remote");
        git_in(&origin, &["init", "--bare", "-b", "main"]);
        git_in(
            root,
            &["remote", "add", "origin", origin.to_str().expect("a path")],
        );
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["change"] = json!("fix");
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = root.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, "fix/CTV-1", "main")
            .expect("the task's worktree is made");
        git_in(root, &["tag", "fix/CTV-1"]);
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "fix the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");

        let pushed = project
            .call("dev-a", Some("CTV-1"), "catervas_git_push", json!({}))
            .expect("the assignee pushes");

        assert_eq!(pushed, json!({ "remote": "origin", "branch": "fix/CTV-1" }));
        let head = git_output_in(&worktree, &["rev-parse", "HEAD"]);
        let heads = git_output_in(&origin, &["for-each-ref", "refs/heads"]);
        assert!(
            heads.contains(&format!("{head} commit\trefs/heads/fix/CTV-1")),
            "{heads}"
        );
        assert!(!heads.contains("catervas/CTV-1"), "{heads}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_commit_a_directory_that_holds_a_protected_file() {
        // `src/login/**` is allowed and `**/*.pem` protected (5.12's default), so the directory
        // passes both path checks while git would stage the key under it.
        let project = TestProject::new("tools-git-directory", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        std::fs::write(worktree.join("src/login/k.pem"), "a key\n").expect("a key");

        for path in ["src/login", "src/login/"] {
            match project.call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "add the login form", "paths": [path] }),
            ) {
                Err(ToolError::Refused { reason }) => assert_eq!(
                    reason,
                    format!("path_is_a_directory: {path} is a directory; name the files to commit")
                ),
                other => panic!("{path}: expected a refusal, got {other:?}"),
            }
        }
        let git = &project.deps.git;
        assert_eq!(
            git.commit_count("main", &project.branch("CTV-1"))
                .expect("git counts"),
            0
        );
        project
            .call(
                "dev-a",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "add the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the file itself is committed");
        assert_eq!(
            git.changed_paths("main", &project.branch("CTV-1"))
                .expect("git lists the paths"),
            ["src/login/form.ts"]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_commit_from_anyone_but_the_assignee() {
        // `git_remote` for the reviewer too, and both git tiers for the Product Manager, who
        // neither holds nor reviews the task, so that each is refused for who it is, not its
        // tiers.
        let project = TestProject::new(
            "tools-git-reviewer",
            &a_team_of_three(|wire| {
                wire["agents"][0]["grants"] = json!(["git_local", "git_remote"]);
                wire["agents"][2]["grants"] = json!(["git_remote"]);
            }),
        );
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        let commit = json!({ "message": "add the login form", "paths": ["src/login/form.ts"] });

        for agent in ["dev-b", "pm"] {
            for (name, input) in [
                ("catervas_git_commit", commit.clone()),
                ("catervas_git_push", json!({})),
            ] {
                match project.call(agent, Some("CTV-1"), name, input) {
                    Err(ToolError::Refused { reason }) => {
                        assert!(
                            reason.starts_with("not_the_named_agent: "),
                            "{agent} {name}: {reason}"
                        );
                    }
                    other => panic!("{agent} {name}: expected a refusal, got {other:?}"),
                }
            }
        }
        let git = &project.deps.git;
        assert_eq!(
            git.commit_count("main", &project.branch("CTV-1"))
                .expect("git counts"),
            0
        );
        project
            .call("dev-a", Some("CTV-1"), "catervas_git_commit", commit)
            .expect("the assignee commits");
        assert_eq!(
            git.commit_count("main", &project.branch("CTV-1"))
                .expect("git counts"),
            1
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_commit_outside_the_tasks_implement_session() {
        let project = TestProject::new("tools-git-refine", &a_team_of_three(|_| {}));
        project.filed("CTV-1", "assigned", "task", None);
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        let commit = json!({ "message": "add the login form", "paths": ["src/login/form.ts"] });
        let git = &project.deps.git;

        let mut refine = project.context("dev-a", Some("CTV-1"));
        refine.purpose = crate::session::SessionPurpose::Refine;
        match crate::tools::fixtures::run(&refine, "catervas_git_commit", commit.clone()) {
            Err(ToolError::Refused { reason }) => assert!(
                reason.starts_with("no_task_no_write: catervas_git_commit "),
                "{reason}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(
            git.commit_count("main", &project.branch("CTV-1"))
                .expect("git counts"),
            0
        );
        project
            .call("dev-a", Some("CTV-1"), "catervas_git_commit", commit)
            .expect("an implement session commits");
        assert_eq!(
            git.commit_count("main", &project.branch("CTV-1"))
                .expect("git counts"),
            1
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_product_manager_commits_in_its_docs_task() {
        let project = TestProject::new("tools-git-pm-docs", &a_team_of_three(|_| {}));
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["allowed_paths"] = json!(["docs/catervas/product/**"]);
            wire["assignee_role"] = json!("product_manager");
            wire["reviewer_role"] = json!("architect");
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm", "reviewer": "dev-a" }),
        );
        let branch = project.branch("CTV-1");
        assert_eq!(branch, "docs/CTV-1");
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &branch, "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("docs/catervas/product")).expect("a directory");
        for file in ["spec.md", "spec.agent.md"] {
            std::fs::write(
                worktree.join("docs/catervas/product").join(file),
                "# Spec\n",
            )
            .expect("a file");
        }

        project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({
                    "message": "docs(product): write the spec",
                    "paths": ["docs/catervas/product/spec.md", "docs/catervas/product/spec.agent.md"]
                }),
            )
            .expect("the Product Manager commits its docs task");
        let git = &project.deps.git;
        assert_eq!(git.commit_count("main", &branch).expect("git counts"), 1);
        let mut changed = git
            .changed_paths("main", &branch)
            .expect("git lists the paths");
        changed.sort();
        assert_eq!(
            changed,
            [
                "docs/catervas/product/spec.agent.md",
                "docs/catervas/product/spec.md"
            ]
        );
    }

    /// The Product Manager's docs task `CTV-1`, with `spec.md` committed on its branch.
    fn a_docs_task_holding_a_spec(
        name: &str,
        allowed: &[&str],
    ) -> (TestProject, std::path::PathBuf) {
        let project = TestProject::new(name, &a_team_of_three(|_| {}));
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["allowed_paths"] = json!(allowed);
            wire["assignee_role"] = json!("product_manager");
            wire["reviewer_role"] = json!("architect");
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm", "reviewer": "dev-a" }),
        );
        let worktree = project.repo.path.join(".catervas/local/worktrees/CTV-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("docs/catervas/product")).expect("a directory");
        std::fs::write(worktree.join("docs/catervas/product/spec.md"), "# Spec\n").expect("a file");
        project
            .deps
            .git
            .commit(
                &worktree,
                "docs(product): the spec",
                &["docs/catervas/product/spec.md".to_string()],
            )
            .expect("the spec is committed");
        (project, worktree)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn commits_a_removed_path() {
        let (project, worktree) =
            a_docs_task_holding_a_spec("tools-git-removed", &["docs/catervas/product/**"]);
        std::fs::write(
            worktree.join("docs/catervas/product/spec.html"),
            "<h1>Spec</h1>\n",
        )
        .expect("a file");
        project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({
                    "message": "docs(product): the spec as HTML",
                    "paths": ["docs/catervas/product/spec.html"],
                    "removed": ["docs/catervas/product/spec.md"]
                }),
            )
            .expect("the assignee commits a file and the removal of another");
        assert!(!worktree.join("docs/catervas/product/spec.md").exists());
        let tree = catervas_store::git::fixtures::git_output_in(
            &worktree,
            &[
                "ls-tree",
                "-r",
                "--name-only",
                "HEAD",
                "docs/catervas/product",
            ],
        );
        assert_eq!(tree.trim(), "docs/catervas/product/spec.html");

        // Nothing there to remove is a refusal, not a quiet pass.
        let missing = project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "m", "paths": [], "removed": ["docs/catervas/product/gone.md"] }),
            )
            .expect_err("a path that is not there");
        assert!(format!("{missing:?}").contains("gone.md"), "{missing:?}");
        // A directory is refused, as for `paths`.
        let directory = project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "m", "paths": [], "removed": ["docs/catervas/product"] }),
            )
            .expect_err("a directory");
        assert!(
            format!("{directory:?}").contains("path_is_a_directory"),
            "{directory:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_remove_through_a_link() {
        let (project, worktree) =
            a_docs_task_holding_a_spec("tools-git-removed-link", &["docs/catervas/**"]);
        let outside = project.repo.path.join("outside");
        std::fs::create_dir_all(&outside).expect("a directory");
        std::fs::write(outside.join("keep.md"), "keep\n").expect("a file");
        std::os::unix::fs::symlink(&outside, worktree.join("docs/catervas/linked"))
            .expect("a link");
        let refused = project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "m", "paths": [], "removed": ["docs/catervas/linked/keep.md"] }),
            )
            .expect_err("a removal through a link");
        assert!(format!("{refused:?}").contains("linked"), "{refused:?}");
        assert!(
            outside.join("keep.md").exists(),
            "nothing outside is deleted"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_removal_outside_the_allowed_paths() {
        let (project, worktree) =
            a_docs_task_holding_a_spec("tools-git-removed-scope", &["docs/catervas/product/**"]);
        std::fs::create_dir_all(worktree.join("src")).expect("a directory");
        std::fs::write(worktree.join("src/lib.rs"), "\n").expect("a file");
        let refused = project
            .call(
                "pm",
                Some("CTV-1"),
                "catervas_git_commit",
                json!({ "message": "m", "paths": [], "removed": ["src/lib.rs"] }),
            )
            .expect_err("outside the allowed paths");
        assert!(matches!(refused, ToolError::Refused { .. }), "{refused:?}");
        assert!(worktree.join("src/lib.rs").exists());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_git_tools_without_a_task() {
        // `git_remote` too, so that the push is refused for the task it lacks, not the tier.
        let project = TestProject::new(
            "tools-git-no-task",
            &a_team_of_three(|wire| wire["agents"][1]["grants"] = json!(["git_remote"])),
        );
        for (name, input) in [
            ("catervas_git_status", json!({})),
            ("catervas_git_diff", json!({})),
            (
                "catervas_git_commit",
                json!({ "message": "m", "paths": ["a"] }),
            ),
            ("catervas_git_push", json!({})),
        ] {
            // A commit and a push change the project, which only a task's session does.
            let expected = if matches!(name, "catervas_git_commit" | "catervas_git_push") {
                "no_task_no_write: "
            } else {
                "no_task: "
            };
            match project.call("dev-a", None, name, input) {
                Err(ToolError::Refused { reason }) => {
                    assert!(reason.starts_with(expected), "{name}: {reason}");
                }
                other => panic!("{name}: expected a refusal, got {other:?}"),
            }
        }
    }
}
