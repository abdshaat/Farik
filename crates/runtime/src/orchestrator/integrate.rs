//! What happens to a task's work once it is finished (`docs/SPEC.md` 5.14): its worktrees and
//! containers removed, its branch kept.

use std::path::Path;

use farik_core::contract::TaskId;
use farik_store::files::FilesError;
use farik_store::{Git, GitError, TaskProjection};

use super::{Orchestrator, OrchestratorError, TickReport, worktree};

/// Rule 1: a task `accepted` or `cancelled` whose worktree, or whose base worktree, is still
/// there has its sandbox and worktrees removed.
pub(super) fn cleanup(
    orchestrator: &Orchestrator,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if !remove_workspace(orchestrator, &row.task_id)? {
        return Ok(None);
    }
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!(
            "removed its worktree and its sandbox, and kept farik/{}",
            row.task_id.as_str()
        ),
    }))
}

/// Removes a finished task's sandbox and worktrees when either worktree is still there, and says
/// whether there was anything to remove. The containers go by name, since after a restart no
/// handle reaches them; then the base worktree; then the task's own worktree last, so that a run
/// stopped in between leaves it, which is what brings this back.
pub(super) fn remove_workspace(
    orchestrator: &Orchestrator,
    task_id: &TaskId,
) -> Result<bool, OrchestratorError> {
    let deps = &orchestrator.deps;
    let own = worktree(deps, task_id);
    let base = own.with_file_name(format!("{}-base", task_id.as_str()));
    if !own.exists() && !base.exists() {
        return Ok(false);
    }
    orchestrator.forget_sandbox(task_id);
    deps.sandboxes.remove(&deps.tools.ids.project_id, task_id)?;
    remove_worktree(&deps.tools.git, &base)?;
    remove_worktree(&deps.tools.git, &own)?;
    Ok(true)
}

/// Removes the worktree at `path`, whether git has it registered or it is a directory git no
/// longer knows, as step 06 removes its base worktree.
fn remove_worktree(git: &Git, path: &Path) -> Result<(), OrchestratorError> {
    if !path.exists() {
        return Ok(());
    }
    match git.remove_worktree(path) {
        // ponytail: git's English words for a path it has no worktree at, as in `criteria`.
        Err(GitError::CommandFailed { stderr, .. }) if stderr.contains("is not a working tree") => {
        }
        other => other?,
    }
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(OrchestratorError::Files(FilesError::Io {
                path: path.display().to_string(),
                detail: error.to_string(),
            }))
        }
        _ => Ok(()),
    }
}
