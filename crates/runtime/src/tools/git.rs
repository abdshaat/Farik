//! The git tools, on the task's worktree (ADR 0004): four tools rather than one, so that each
//! carries one tier and is checked as its own tool.

use std::path::PathBuf;

use farik_core::branch::task_branch;
use farik_core::contract::TaskId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::transitions::integration_branch;

/// The remote a task branch is pushed to.
const REMOTE: &str = "origin";

/// `farik_git_commit`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitInput {
    /// The commit message.
    message: String,
    /// The paths to commit, relative to the worktree.
    paths: Vec<String>,
}

/// `farik_git_status`: what is uncommitted in the task's worktree, and whether nothing is.
pub(super) fn status(call: &Call<'_>) -> Result<Value, ToolError> {
    let status = call
        .deps()
        .git
        .status(&worktree(call, call.task()?))
        .map_err(failed)?;
    Ok(json!({ "clean": status.is_empty(), "status": status }))
}

/// `farik_git_diff`: what the task branch changed since the integration branch, as a patch.
pub(super) fn diff(call: &Call<'_>) -> Result<Value, ToolError> {
    let task = call.task()?;
    let git = &call.deps().git;
    let base = integration_branch(&call.team, git).map_err(failed)?;
    let diff = git.diff(&base, &branch(call, task)?).map_err(failed)?;
    Ok(json!({ "diff": diff }))
}

/// `farik_git_commit`: commits the named paths of the task's worktree on the task branch, for the
/// task's assignee alone. A path that is a directory is refused rather than staged whole: the path
/// checks of 5.6 judged the directory's name, not the files under it, and a protected one among
/// them would be committed.
pub(super) fn commit(call: &Call<'_>, input: &CommitInput) -> Result<Value, ToolError> {
    let task = assignees_task(call)?;
    let worktree = worktree(call, task);
    if let Some(path) = input.paths.iter().find(|path| worktree.join(path).is_dir()) {
        return Err(Refusal::PathIsADirectory { path: path.clone() }.into());
    }
    let sha = call
        .deps()
        .git
        .commit(&worktree, &input.message, &input.paths)
        .map_err(failed)?;
    Ok(json!({ "sha": sha }))
}

/// `farik_git_push`: pushes the task branch to `origin`, for the task's assignee alone. It spells
/// `refs/heads/<branch>`, as integration does, so that a tag of the same name cannot make the push
/// ambiguous.
pub(super) fn push(call: &Call<'_>) -> Result<Value, ToolError> {
    let branch = branch(call, assignees_task(call)?)?;
    call.deps()
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

/// The task's worktree, `.farik/local/worktrees/<id>` (5.14).
fn worktree(call: &Call<'_>, task: &TaskId) -> PathBuf {
    call.deps()
        .files
        .root()
        .join(".farik/local/worktrees")
        .join(task.as_str())
}

/// The task's branch, the one its contract names (5.14).
fn branch(call: &Call<'_>, task: &TaskId) -> Result<String, ToolError> {
    let (contract, _) = call.contract(task)?;
    Ok(task_branch(&contract))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn commits_through_the_tool_on_the_tasks_worktree() {
        let project = TestProject::new("tools-git-commit", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "assigned", "task", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("FRK-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");

        let status = project
            .call("dev-a", Some("FRK-1"), "farik_git_status", json!({}))
            .expect("the status reads");
        assert_eq!(status["clean"], false);
        let committed = project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_git_commit",
                json!({ "message": "add the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");
        let head = farik_store::git::fixtures::git_output_in(&worktree, &["rev-parse", "HEAD"]);
        assert_eq!(committed["sha"], head);

        let status = project
            .call("dev-a", Some("FRK-1"), "farik_git_status", json!({}))
            .expect("the status reads");
        assert_eq!(status["clean"], true, "{status}");
        let diff = project
            .call("dev-a", Some("FRK-1"), "farik_git_diff", json!({}))
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
    fn diffs_the_tasks_branch_through_the_tool() {
        // A fix works on `fix/<id>` (5.14), so the diff is of that branch.
        let project = TestProject::new("tools-git-diff-fix", &a_team_of_three(|_| {}));
        project.filed_with("FRK-1", "assigned", "task", None, |wire| {
            wire["change"] = json!("fix");
        });
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .deps
            .git
            .create_worktree(&worktree, "fix/FRK-1", "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_git_commit",
                json!({ "message": "fix the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");

        let diff = project
            .call("dev-a", Some("FRK-1"), "farik_git_diff", json!({}))
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
        use farik_store::git::fixtures::{git_in, git_output_in};
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
        project.filed_with("FRK-1", "assigned", "task", None, |wire| {
            wire["change"] = json!("fix");
        });
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = root.join(".farik/local/worktrees/FRK-1");
        project
            .deps
            .git
            .create_worktree(&worktree, "fix/FRK-1", "main")
            .expect("the task's worktree is made");
        git_in(root, &["tag", "fix/FRK-1"]);
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_git_commit",
                json!({ "message": "fix the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the assignee commits");

        let pushed = project
            .call("dev-a", Some("FRK-1"), "farik_git_push", json!({}))
            .expect("the assignee pushes");

        assert_eq!(pushed, json!({ "remote": "origin", "branch": "fix/FRK-1" }));
        let head = git_output_in(&worktree, &["rev-parse", "HEAD"]);
        let heads = git_output_in(&origin, &["for-each-ref", "refs/heads"]);
        assert!(
            heads.contains(&format!("{head} commit\trefs/heads/fix/FRK-1")),
            "{heads}"
        );
        assert!(!heads.contains("farik/FRK-1"), "{heads}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_commit_a_directory_that_holds_a_protected_file() {
        // `src/login/**` is allowed and `**/*.pem` protected (5.12's default), so the directory
        // passes both path checks while git would stage the key under it.
        let project = TestProject::new("tools-git-directory", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "assigned", "task", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("FRK-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        std::fs::write(worktree.join("src/login/k.pem"), "a key\n").expect("a key");

        for path in ["src/login", "src/login/"] {
            match project.call(
                "dev-a",
                Some("FRK-1"),
                "farik_git_commit",
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
            git.commit_count("main", &project.branch("FRK-1"))
                .expect("git counts"),
            0
        );
        project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_git_commit",
                json!({ "message": "add the login form", "paths": ["src/login/form.ts"] }),
            )
            .expect("the file itself is committed");
        assert_eq!(
            git.changed_paths("main", &project.branch("FRK-1"))
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
        project.filed("FRK-1", "assigned", "task", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch("FRK-1"), "main")
            .expect("the task's worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.ts"), "export {};\n").expect("a file");
        let commit = json!({ "message": "add the login form", "paths": ["src/login/form.ts"] });

        for agent in ["dev-b", "pm"] {
            for (name, input) in [
                ("farik_git_commit", commit.clone()),
                ("farik_git_push", json!({})),
            ] {
                match project.call(agent, Some("FRK-1"), name, input) {
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
            git.commit_count("main", &project.branch("FRK-1"))
                .expect("git counts"),
            0
        );
        project
            .call("dev-a", Some("FRK-1"), "farik_git_commit", commit)
            .expect("the assignee commits");
        assert_eq!(
            git.commit_count("main", &project.branch("FRK-1"))
                .expect("git counts"),
            1
        );
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
            ("farik_git_status", json!({})),
            ("farik_git_diff", json!({})),
            (
                "farik_git_commit",
                json!({ "message": "m", "paths": ["a"] }),
            ),
            ("farik_git_push", json!({})),
        ] {
            match project.call("dev-a", None, name, input) {
                Err(ToolError::Refused { reason }) => {
                    assert!(reason.starts_with("no_task: "), "{name}: {reason}");
                }
                other => panic!("{name}: expected a refusal, got {other:?}"),
            }
        }
    }
}
