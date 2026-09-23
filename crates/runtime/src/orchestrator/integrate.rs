//! What happens to a task's work once it is finished (`docs/SPEC.md` 5.14): an accepted task's
//! branch integrated the way the team's policy says, one task at a time even across processes,
//! and a finished task's worktrees and containers removed, its branch kept.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;

use farik_core::contract::{TaskId, TaskKind, TaskStatus};
use farik_core::team::{Integration, Team};
use farik_protocol::event::{
    EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventIds, EventKind, FarikEvent,
    NoteWrittenBodyKind, PullRequestOpenedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
    new_event,
};
use farik_store::files::FilesError;
use farik_store::{EventQuery, Git, GitError, MergeOutcome, TaskProjection};

use super::{IntegrationOutcome, Orchestrator, OrchestratorError, TickReport, worktree};
use crate::forge::{Forge, PullRequestState};
use crate::tools::ToolDeps;
use crate::transitions::{TransitionError, integration_branch};

/// Where the integration lock lives, under the project root.
const LOCK: &str = ".farik/local/integration.lock";

/// Who asked for an integration: the tick, on the team's policy, or the human.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asker {
    Tick,
    Human,
}

/// Rule 2: an accepted task awaiting integration is integrated the way the team's policy says,
/// unless an integration escalation was raised since it was accepted: that one is the human's to
/// settle through `integrate`, and is not tried again on every tick. `manual` has no rule.
pub(super) async fn awaiting(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if row.kind != TaskKind::Task
        || !row.awaiting_integration
        || team.policy.integration == Integration::Manual
    {
        return Ok(None);
    }
    let Some(outcome) = attempt(orchestrator, team, &row.task_id, Asker::Tick).await? else {
        return Ok(None);
    };
    let what = match outcome {
        IntegrationOutcome::Merged { sha } => format!("integrated at {sha}"),
        IntegrationOutcome::PullRequestOpened { url } => format!("opened the pull request {url}"),
        IntegrationOutcome::Escalated { detail } => {
            format!("escalated its integration to the human: {detail}")
        }
        IntegrationOutcome::AwaitingForge => return Ok(None),
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    }))
}

/// The human's integration of an accepted task, whatever escalations it carries.
pub(super) async fn integrate(
    orchestrator: &Orchestrator,
    task_id: &TaskId,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let Some(row) = orchestrator.deps.tools.projections.task(task_id)? else {
        return Err(refused(format!("no_such_task: {}", task_id.as_str())));
    };
    refuse_unless_integrable(&row)?;
    let team = orchestrator.deps.tools.files.read_team()?;
    attempt(orchestrator, &team, task_id, Asker::Human)
        .await?
        .ok_or_else(|| refused(format!("nothing_to_do: {}", task_id.as_str())))
}

/// `Refused` for a row that is not an accepted task.
fn refuse_unless_integrable(row: &TaskProjection) -> Result<(), OrchestratorError> {
    if row.status != TaskStatus::Accepted {
        return Err(refused(format!(
            "not_accepted: {} is {}",
            row.task_id.as_str(),
            row.status
        )));
    }
    if row.kind != TaskKind::Task {
        return Err(refused(format!(
            "an_epic: {} has no branch of its own to integrate",
            row.task_id.as_str()
        )));
    }
    Ok(())
}

fn refused(reason: String) -> OrchestratorError {
    OrchestratorError::Refused { reason }
}

/// One integration, under the lock and off the async threads, because it runs git; `None` when
/// the tick finds nothing to do once it holds the lock.
async fn attempt(
    orchestrator: &Orchestrator,
    team: &Team,
    task_id: &TaskId,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let tools = Arc::clone(&orchestrator.deps.tools);
    let forge = Arc::clone(&orchestrator.deps.forge);
    let team = team.clone();
    let task_id = task_id.clone();
    let finished = tokio::task::spawn_blocking(move || {
        let _lock = lock(tools.files.root())?;
        integrate_locked(&tools, &forge, &team, &task_id, asker)
    })
    .await;
    match finished {
        Ok(outcome) => outcome,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => Err(OrchestratorError::Lock {
            detail: format!("the integration was cancelled: {error}"),
        }),
    }
}

/// What the policy does for the task, with the lock held: the board is brought up to what other
/// processes appended first, so that a task another one integrated is answered, not merged again.
fn integrate_locked(
    tools: &ToolDeps,
    forge: &Forge,
    team: &Team,
    task_id: &TaskId,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    tools.projections.catch_up()?;
    let Some(row) = tools.projections.task(task_id)? else {
        return Err(refused(format!("no_such_task: {}", task_id.as_str())));
    };
    refuse_unless_integrable(&row)?;
    if !row.awaiting_integration {
        return match last_integrated_sha(tools, task_id)? {
            Some(sha) => Ok(Some(IntegrationOutcome::Merged { sha })),
            None if asker == Asker::Tick => Ok(None),
            None => Err(refused(format!(
                "never_integrated: {} is not awaiting integration and was never integrated",
                task_id.as_str()
            ))),
        };
    }
    if asker == Asker::Tick && escalated_since_accepted(tools, task_id)? {
        return Ok(None);
    }
    let into = match integration_branch(team, &tools.git) {
        Ok(into) => into,
        Err(error) => return escalate(tools, task_id, git_words(&error)).map(Some),
    };
    let integrated_by = match asker {
        Asker::Tick => TaskIntegratedBodyIntegratedBy::Governor,
        Asker::Human => TaskIntegratedBodyIntegratedBy::Human,
    };
    let outcome = match team.policy.integration {
        Integration::Manual => Some(merge(tools, &row, &into, integrated_by, false)?),
        Integration::AutoMerge => Some(merge(tools, &row, &into, integrated_by, true)?),
        Integration::PullRequest => through_the_forge(tools, forge, &row, &into, asker)?,
    };
    Ok(outcome)
}

/// Under `pull_request`: with no pull request opened since acceptance, the branch pushed to
/// `origin` and one opened; with one, its state read. A merge on the forge is recorded as the
/// human's and the local integration branch brought up to it; a closed one is escalated, unless
/// the human asks and the branch is in the integration branch all the same.
fn through_the_forge(
    tools: &ToolDeps,
    forge: &Forge,
    row: &TaskProjection,
    into: &str,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let task_id = &row.task_id;
    let branch = format!("farik/{}", task_id.as_str());
    let opened = since_accepted(tools, task_id, &[EventKind::PullRequestOpened])?
        .into_iter()
        .rev()
        .find_map(|event| match event.body {
            EventBody::PullRequestOpened(body) => Some(body),
            _ => None,
        });
    let Some(opened) = opened else {
        return open(tools, forge, row, into, &branch).map(Some);
    };
    let url = opened.url;
    let state = match forge.pull_request_state(&url) {
        Ok(state) => state,
        Err(error) => {
            let detail = format!("reading the pull request {url} failed: {error}");
            return escalate(tools, task_id, detail).map(Some);
        }
    };
    match state {
        PullRequestState::Open => Ok(Some(IntegrationOutcome::AwaitingForge)),
        PullRequestState::Merged { sha } => {
            record_integrated(
                tools,
                task_id,
                &sha,
                into,
                TaskIntegratedBodyIntegratedBy::Human,
            )?;
            if let Err(error) = tools.git.fetch_fast_forward("origin", into) {
                let detail = format!(
                    "the pull request {url} was merged as {sha}, but the local {into} could not be \
                     brought up to it: {}",
                    git_words(&error)
                );
                return escalate(tools, task_id, detail).map(Some);
            }
            Ok(Some(IntegrationOutcome::Merged { sha }))
        }
        PullRequestState::Closed if asker == Asker::Tick => {
            let detail = format!("the pull request {url} was closed without merging");
            escalate(tools, task_id, detail).map(Some)
        }
        PullRequestState::Closed => {
            closed_for_the_human(tools, task_id, &url, into, &branch).map(Some)
        }
    }
}

/// A closed pull request the human asks about: the task is integrated when its branch is in the
/// integration branch as the forge has it, merged by hand or through another pull request.
fn closed_for_the_human(
    tools: &ToolDeps,
    task_id: &TaskId,
    url: &str,
    into: &str,
    branch: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let merged = tools
        .git
        .fetch_fast_forward("origin", into)
        .and_then(|()| tools.git.commit_count(into, branch));
    match merged {
        Ok(0) => {
            let head = tools.git.merge_base(into, into)?;
            record_integrated(
                tools,
                task_id,
                &head,
                into,
                TaskIntegratedBodyIntegratedBy::Human,
            )?;
            Ok(IntegrationOutcome::Merged { sha: head })
        }
        Ok(_) => {
            let detail = format!(
                "the pull request {url} was closed without merging, and {branch} is not in \
                 {into}: reopen it on the forge or merge the branch, then run farik integrate"
            );
            escalate(tools, task_id, detail)
        }
        Err(error) => {
            let detail = format!(
                "the pull request {url} was closed without merging, and whether {branch} is in \
                 {into} could not be read: {}",
                git_words(&error)
            );
            escalate(tools, task_id, detail)
        }
    }
}

/// Pushes the branch to `origin` and opens its pull request, recording it.
fn open(
    tools: &ToolDeps,
    forge: &Forge,
    row: &TaskProjection,
    into: &str,
    branch: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let task_id = &row.task_id;
    match tools.git.has_remote("origin") {
        Ok(true) => {}
        Ok(false) => {
            let detail = format!(
                "the pull_request policy pushes {branch} to origin, and there is no remote named \
                 origin"
            );
            return escalate(tools, task_id, detail);
        }
        Err(error) => return escalate(tools, task_id, git_words(&error)),
    }
    if let Err(error) = tools.git.push("origin", &format!("refs/heads/{branch}")) {
        let detail = format!("pushing {branch} to origin failed: {}", git_words(&error));
        return escalate(tools, task_id, detail);
    }
    let title = format!("{}: {}", task_id.as_str(), row.title);
    let body = pull_request_body(tools, task_id)?;
    let pull_request = match forge.open_pull_request(into, branch, &title, &body) {
        Ok(pull_request) => pull_request,
        Err(error) => {
            let detail = format!("opening a pull request for {branch} failed: {error}");
            return escalate(tools, task_id, detail);
        }
    };
    append(
        tools,
        task_id,
        EventBody::PullRequestOpened(PullRequestOpenedBody {
            url: pull_request.url.clone(),
            number: pull_request.number,
            branch: branch.to_string(),
        }),
    )?;
    Ok(IntegrationOutcome::PullRequestOpened {
        url: pull_request.url,
    })
}

/// The pull request's body: the contract's intent, then the last completion and review notes.
fn pull_request_body(tools: &ToolDeps, task_id: &TaskId) -> Result<String, OrchestratorError> {
    let contract = tools.files.read_contract(task_id)?;
    let intent = contract.intent.as_str();
    let notes = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::NoteWritten],
        ..EventQuery::default()
    })?;
    let last_note = |kind: NoteWrittenBodyKind| {
        notes
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                EventBody::NoteWritten(body) if body.kind == kind => Some(body.text.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "(none was written)".to_string())
    };
    Ok(format!(
        "## Intent\n\n{intent}\n\n## Completion note\n\n{}\n\n## Review note\n\n{}\n",
        last_note(NoteWrittenBodyKind::Completion),
        last_note(NoteWrittenBodyKind::Review)
    ))
}

/// Merges the task's branch into `into`, records it, and, when `push` and there is an `origin`,
/// pushes `into` there. A conflict, a git failure, or a failed push is an escalation; the merge
/// stays when only the push failed, since the local integration branch is what dependents start
/// from.
fn merge(
    tools: &ToolDeps,
    row: &TaskProjection,
    into: &str,
    integrated_by: TaskIntegratedBodyIntegratedBy,
    push: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let id = row.task_id.as_str();
    let branch = format!("farik/{id}");
    let message = format!("Merge {id}: {}", row.title);
    let sha = match tools.git.merge(into, &branch, &message) {
        Ok(MergeOutcome::Merged { sha }) => sha,
        Ok(MergeOutcome::Conflicts(paths)) => {
            let detail = format!(
                "merging {branch} into {into} conflicts in {}: resolve them on {into}, then run \
                 farik integrate {id}",
                paths.join(", ")
            );
            return escalate(tools, &row.task_id, detail);
        }
        Err(error) => {
            let detail = format!("merging {branch} into {into} failed: {}", git_words(&error));
            return escalate(tools, &row.task_id, detail);
        }
    };
    record_integrated(tools, &row.task_id, &sha, into, integrated_by)?;
    if push {
        let pushed = tools.git.has_remote("origin").and_then(|has| {
            if has {
                tools.git.push("origin", &format!("refs/heads/{into}"))
            } else {
                Ok(())
            }
        });
        if let Err(error) = pushed {
            let detail = format!(
                "merged locally as {sha}; pushing {into} to origin failed: {}; run git push origin \
                 {into} once it can be pushed",
                git_words(&error)
            );
            return escalate(tools, &row.task_id, detail);
        }
    }
    Ok(IntegrationOutcome::Merged { sha })
}

/// Appends `task.integrated`.
fn record_integrated(
    tools: &ToolDeps,
    task_id: &TaskId,
    sha: &str,
    into: &str,
    integrated_by: TaskIntegratedBodyIntegratedBy,
) -> Result<(), OrchestratorError> {
    append(
        tools,
        task_id,
        EventBody::TaskIntegrated(TaskIntegratedBody {
            sha: sha.to_string(),
            into: into.to_string(),
            integrated_by,
        }),
    )
}

/// Appends an integration escalation, which moves nothing, and answers it.
fn escalate(
    tools: &ToolDeps,
    task_id: &TaskId,
    detail: String,
) -> Result<IntegrationOutcome, OrchestratorError> {
    append(
        tools,
        task_id,
        EventBody::EscalationRaised(EscalationRaisedBody {
            reason: EscalationRaisedBodyReason::Integration,
            detail: detail.clone(),
        }),
    )?;
    Ok(IntegrationOutcome::Escalated { detail })
}

/// Git's own words for a failure.
fn git_words(error: &GitError) -> String {
    match error {
        GitError::CommandFailed { stderr, .. } => stderr.clone(),
        other => other.to_string(),
    }
}

/// Appends one event about the task, Farik's own, and projects it.
fn append(tools: &ToolDeps, task_id: &TaskId, body: EventBody) -> Result<(), OrchestratorError> {
    let ids = EventIds {
        task_id: Some(task_id.clone()),
        ..tools.ids.clone()
    };
    let event = new_event(body, tools.clock.now(), ids).map_err(|error| {
        OrchestratorError::Transition(TransitionError::Event {
            detail: format!("{error:?}"),
        })
    })?;
    let appended = tools.log.append(&event)?;
    tools.projections.apply(&appended)?;
    Ok(())
}

/// The task's events of `kinds` since its last move into `accepted`.
fn since_accepted(
    tools: &ToolDeps,
    task_id: &TaskId,
    kinds: &[EventKind],
) -> Result<Vec<FarikEvent>, OrchestratorError> {
    let mut asked = vec![EventKind::TaskTransitioned];
    asked.extend_from_slice(kinds);
    let history = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: asked,
        ..EventQuery::default()
    })?;
    let start = history
        .iter()
        .rposition(is_move_into_accepted)
        .map_or(0, |at| at + 1);
    Ok(history
        .into_iter()
        .skip(start)
        .filter(|event| kinds.contains(&event.body.kind()))
        .collect())
}

fn is_move_into_accepted(event: &FarikEvent) -> bool {
    matches!(&event.body, EventBody::TaskTransitioned(body) if body.to.to_string() == TaskStatus::Accepted.to_string())
}

/// Whether an integration escalation was raised since the task was accepted.
fn escalated_since_accepted(tools: &ToolDeps, task_id: &TaskId) -> Result<bool, OrchestratorError> {
    Ok(
        since_accepted(tools, task_id, &[EventKind::EscalationRaised])?
            .iter()
            .any(|event| {
                matches!(&event.body, EventBody::EscalationRaised(body)
                if body.reason == EscalationRaisedBodyReason::Integration)
            }),
    )
}

/// The commit the task was last integrated at.
fn last_integrated_sha(
    tools: &ToolDeps,
    task_id: &TaskId,
) -> Result<Option<String>, OrchestratorError> {
    let integrated = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskIntegrated],
        ..EventQuery::default()
    })?;
    Ok(integrated
        .into_iter()
        .rev()
        .find_map(|event| match event.body {
            EventBody::TaskIntegrated(body) => Some(body.sha),
            _ => None,
        }))
}

/// Takes the integration lock, waiting for whoever holds it, process or thread; it is let go when
/// the file is dropped.
fn lock(root: &Path) -> Result<File, OrchestratorError> {
    let path = root.join(LOCK);
    let failed = |error: std::io::Error| OrchestratorError::Lock {
        detail: format!("{}: {error}", path.display()),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(failed)?;
    file.lock().map_err(failed)?;
    Ok(file)
}

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

#[cfg(test)]
mod tests {
    use farik_core::contract::TaskStatus;
    use farik_protocol::event::{
        EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventKind,
        PullRequestOpenedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
    };
    use farik_store::git::fixtures::{git_in, git_output_in};
    use serde_json::json;

    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{IntegrationOutcome, OrchestratorError, TickReport};

    const NOTHING_TO_DO: &str = "nothing on the board needs doing";

    fn idle() -> TickReport {
        TickReport::Idle {
            why: NOTHING_TO_DO.to_string(),
        }
    }

    /// A harness whose team integrates under `policy`.
    fn under(name: &str, policy: &str) -> Harness {
        let policy = policy.to_string();
        Harness::new(name, move |wire| {
            wire["policy"]["integration"] = json!(policy);
        })
    }

    fn integrations(harness: &Harness) -> Vec<TaskIntegratedBody> {
        harness
            .events(&[EventKind::TaskIntegrated])
            .into_iter()
            .map(|event| match event.body {
                EventBody::TaskIntegrated(body) => body,
                other => panic!("a task.integrated, got {other:?}"),
            })
            .collect()
    }

    fn escalations(harness: &Harness) -> Vec<EscalationRaisedBody> {
        harness
            .events(&[EventKind::EscalationRaised])
            .into_iter()
            .map(|event| match event.body {
                EventBody::EscalationRaised(body) => body,
                other => panic!("an escalation.raised, got {other:?}"),
            })
            .collect()
    }

    fn at_root(harness: &Harness, arguments: &[&str]) -> String {
        git_output_in(&harness.project.repo.path, arguments)
    }

    fn task(id: &str) -> farik_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_and_pushes_under_auto_merge() {
        let harness = under("int-auto-merge", "auto_merge");
        let origin = harness.with_origin();
        harness.accepted("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "FRK-1"),
            "{report:?}"
        );
        let head = at_root(&harness, &["rev-parse", "main"]);
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].sha, head);
        assert_eq!(integrated[0].into, "main");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Governor
        );
        let parents = at_root(&harness, &["rev-list", "--parents", "-n", "1", "main"]);
        let branch = at_root(&harness, &["rev-parse", "farik/FRK-1"]);
        assert!(
            parents.split(' ').skip(1).any(|parent| parent == branch),
            "{parents}"
        );
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), head);
        assert_eq!(
            at_root(&harness, &["symbolic-ref", "--short", "HEAD"]),
            "main"
        );
        assert!(!harness.row("FRK-1").awaiting_integration);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_without_a_push_when_there_is_no_origin() {
        let harness = under("int-no-origin", "auto_merge");
        harness.accepted("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(integrations(&harness).len(), 1);
        assert!(escalations(&harness).is_empty());
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_failed_push_and_keeps_the_merge() {
        let harness = under("int-push-fails", "auto_merge");
        harness.with_origin_nowhere();
        harness.accepted("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1);
        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert_eq!(raised[0].reason, EscalationRaisedBodyReason::Integration);
        assert!(
            raised[0].detail.starts_with("merged locally as"),
            "{}",
            raised[0].detail
        );
        assert!(
            raised[0].detail.contains("git push origin main"),
            "{}",
            raised[0].detail
        );
        let kinds = harness.events(&[EventKind::TaskIntegrated, EventKind::EscalationRaised]);
        assert_eq!(kinds[0].body.kind(), EventKind::TaskIntegrated);
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        let before = harness.events(&[]).len();
        assert_eq!(
            orchestrator.integrate(&task("FRK-1")).await,
            Ok(IntegrationOutcome::Merged {
                sha: integrated[0].sha.clone()
            })
        );
        assert_eq!(harness.events(&[]).len(), before);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_conflict_and_leaves_the_task_accepted() {
        let harness = under("int-conflict", "auto_merge");
        harness.accepted_in_conflict("FRK-1");
        let moves_before = harness.events(&[EventKind::TaskTransitioned]).len();
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert_eq!(raised[0].reason, EscalationRaisedBodyReason::Integration);
        assert!(raised[0].detail.contains("a.txt"), "{}", raised[0].detail);
        assert_eq!(
            harness.events(&[EventKind::TaskTransitioned]).len(),
            moves_before
        );
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::Accepted);
        assert!(row.awaiting_integration);
        assert!(integrations(&harness).is_empty());
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_for_the_human_after_an_escalation() {
        let harness = under("int-human", "auto_merge");
        harness.accepted_in_conflict("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");

        let again = orchestrator.integrate(&task("FRK-1")).await;
        assert!(
            matches!(&again, Ok(IntegrationOutcome::Escalated { detail }) if detail.contains("a.txt")),
            "{again:?}"
        );
        assert_eq!(escalations(&harness).len(), 2);

        harness.resolve_the_conflict();
        let merged = orchestrator.integrate(&task("FRK-1")).await;
        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_eq!(merged, Ok(IntegrationOutcome::Merged { sha: head.clone() }));
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1);
        assert_eq!(integrated[0].sha, head);
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_an_integrated_task_without_merging_again() {
        let harness = under("int-twice", "auto_merge");
        harness.accepted("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");
        let sha = integrations(&harness)[0].sha.clone();

        assert_eq!(
            orchestrator.integrate(&task("FRK-1")).await,
            Ok(IntegrationOutcome::Merged { sha: sha.clone() })
        );
        assert_eq!(integrations(&harness).len(), 1);
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_accepted_task_to_the_human_under_manual() {
        let harness = under("int-manual", "manual");
        let origin = harness.with_origin();
        harness.accepted("FRK-1");
        let pushed = git_output_in(&origin, &["rev-parse", "main"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert!(integrations(&harness).is_empty());
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), pushed);

        let merged = orchestrator.integrate(&task("FRK-1")).await;
        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_ne!(head, pushed);
        assert_eq!(merged, Ok(IntegrationOutcome::Merged { sha: head }));
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), pushed);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_integrate_what_is_not_accepted() {
        let harness = under("int-not-accepted", "auto_merge");
        harness.verifying("FRK-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let refused = orchestrator.integrate(&task("FRK-1")).await;

        assert!(
            matches!(&refused, Err(OrchestratorError::Refused { .. })),
            "{refused:?}"
        );
        assert!(integrations(&harness).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn merges_one_task_at_a_time_across_orchestrators() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        for round in 0..20 {
            let harness = Harness::new(&format!("int-lock-{round}"), |wire| {
                wire["policy"]["integration"] = json!("manual");
                wire["policy"]["integration_branch"] = json!("main");
                wire["policy"]["wip_limit_per_agent"] = json!(2);
            });
            // Each branch gets a commit of its own: the fixtures' two `done.txt` commits, made in
            // the same second on the same parent, are one commit, so either branch would hold the
            // other's tip, and whichever merged second could find itself already merged.
            for (task, file) in [("FRK-1", "one.txt"), ("FRK-2", "two.txt")] {
                harness.accepted_with_worktree(task);
                let worktree = harness.worktree(task);
                std::fs::write(worktree.join(file), format!("{task}\n")).expect("written");
                let git = &harness.project.deps.git;
                git.commit(&worktree, &format!("Add {file}"), &[file.to_string()])
                    .expect("committed");
                git.remove_worktree(&worktree).expect("removed");
            }
            git_in(&harness.project.repo.path, &["checkout", "-b", "work"]);
            let first = harness.orchestrator(harness.recorded(Vec::new()));
            let second = harness.orchestrator(harness.recorded(Vec::new()));

            let (frk_1, frk_2) = (task("FRK-1"), task("FRK-2"));
            let (one, two) = runtime.block_on(async {
                tokio::join!(first.integrate(&frk_1), second.integrate(&frk_2))
            });

            assert!(
                matches!(one, Ok(IntegrationOutcome::Merged { .. })),
                "{round}: {one:?}"
            );
            assert!(
                matches!(two, Ok(IntegrationOutcome::Merged { .. })),
                "{round}: {two:?}"
            );
            assert_eq!(
                at_root(&harness, &["symbolic-ref", "--short", "HEAD"]),
                "work",
                "{round}"
            );
            for branch in ["farik/FRK-1", "farik/FRK-2"] {
                git_in(
                    &harness.project.repo.path,
                    &["merge-base", "--is-ancestor", branch, "main"],
                );
            }
            assert_eq!(
                at_root(&harness, &["rev-list", "--merges", "--count", "main"]),
                "2",
                "{round}"
            );
        }
    }

    const PULL_URL: &str = "https://github.com/o/r/pull/7";
    const OPEN: &str = r#"{"mergeCommit":null,"state":"OPEN"}"#;

    /// FRK-1 accepted under `pull_request` with `origin`, its review note written, and, when
    /// `opened`, its pull request 7 already recorded.
    fn a_pull_request(name: &str, opened: bool) -> (Harness, std::path::PathBuf) {
        let harness = under(name, "pull_request");
        let origin = harness.with_origin();
        harness.accepted("FRK-1");
        harness.project.record(
            "FRK-1",
            "note.written",
            &json!({ "kind": "review", "text": "C1 passes; done.txt is there.", "written_by": "dev-b" }),
        );
        if opened {
            harness.project.record(
                "FRK-1",
                "pull_request.opened",
                &json!({ "url": PULL_URL, "number": 7, "branch": "farik/FRK-1" }),
            );
        }
        (harness, origin)
    }

    fn opened(harness: &Harness) -> Vec<PullRequestOpenedBody> {
        harness
            .events(&[EventKind::PullRequestOpened])
            .into_iter()
            .map(|event| match event.body {
                EventBody::PullRequestOpened(body) => body,
                other => panic!("a pull_request.opened, got {other:?}"),
            })
            .collect()
    }

    fn creates(harness: &Harness) -> usize {
        harness
            .gh
            .calls()
            .iter()
            .filter(|call| call.get(1).map(String::as_str) == Some("create"))
            .count()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn opens_a_pull_request_for_an_accepted_task() {
        let (harness, origin) = a_pull_request("int-pr-opens", false);
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            git_output_in(&origin, &["rev-parse", "farik/FRK-1"]),
            at_root(&harness, &["rev-parse", "farik/FRK-1"])
        );
        let recorded = opened(&harness);
        assert_eq!(recorded.len(), 1, "{recorded:?}");
        assert_eq!(recorded[0].number, 7);
        assert_eq!(recorded[0].branch, "farik/FRK-1");
        assert_eq!(recorded[0].url, PULL_URL);
        let body = harness.gh.stdin_of("create");
        let contract = harness
            .project
            .deps
            .files
            .read_contract(&task("FRK-1"))
            .expect("the contract reads");
        let places: Vec<usize> = [
            "## Intent",
            contract.intent.as_str(),
            "## Completion note",
            "Added done.txt; nothing left out.",
            "## Review note",
            "C1 passes; done.txt is there.",
        ]
        .iter()
        .map(|text| {
            body.find(text)
                .unwrap_or_else(|| panic!("{text:?} in {body}"))
        })
        .collect();
        assert!(places.is_sorted(), "{body}");

        harness.gh.answers("view", OPEN, "", 0);
        assert_eq!(
            orchestrator.integrate(&task("FRK-1")).await,
            Ok(IntegrationOutcome::AwaitingForge)
        );
        assert_eq!(opened(&harness).len(), 1);
        assert_eq!(creates(&harness), 1);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_while_the_pull_request_is_open() {
        let (harness, origin) = a_pull_request("int-pr-waits", true);
        harness.gh.answers("view", OPEN, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let before = harness.events(&[]).len();

        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());

        assert_eq!(harness.events(&[]).len(), before);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_a_pull_request_merged_on_the_forge() {
        let (harness, origin) = a_pull_request("int-pr-merged", true);
        let sha = harness.merge_on_the_forge(&origin, "FRK-1");
        harness.gh.answers(
            "view",
            &format!(r#"{{"state":"MERGED","mergeCommit":{{"oid":"{sha}"}}}}"#),
            "",
            0,
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].sha, sha);
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha);
        assert!(escalations(&harness).is_empty());
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_closed_pull_request_once() {
        let (harness, origin) = a_pull_request("int-pr-closed", true);
        harness
            .gh
            .answers("view", r#"{"mergeCommit":null,"state":"CLOSED"}"#, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.contains("was closed without merging"),
            "{}",
            raised[0].detail
        );
        let calls = harness.gh.calls().len();
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert_eq!(harness.gh.calls().len(), calls);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_a_closed_pull_request_the_human_merged() {
        let (harness, origin) = a_pull_request("int-pr-closed-merged", true);
        harness
            .gh
            .answers("view", r#"{"mergeCommit":null,"state":"CLOSED"}"#, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");

        let again = orchestrator.integrate(&task("FRK-1")).await;
        assert!(
            matches!(&again, Ok(IntegrationOutcome::Escalated { detail }) if detail.contains("reopen it")),
            "{again:?}"
        );
        assert_eq!(escalations(&harness).len(), 2);

        let sha = harness.merge_on_the_forge(&origin, "FRK-1");
        assert_eq!(
            orchestrator.integrate(&task("FRK-1")).await,
            Ok(IntegrationOutcome::Merged { sha: sha.clone() })
        );
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1);
        assert_eq!(integrated[0].sha, sha);
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_when_gh_is_missing() {
        let (harness, origin) = a_pull_request("int-pr-no-gh", false);
        let orchestrator = harness.orchestrator_with_forge(
            harness.recorded(Vec::new()),
            std::sync::Arc::new(crate::sandbox::host::HostSandboxFactory),
            crate::forge::Forge {
                program: "/nonexistent/farik/gh".into(),
                root: harness.project.repo.path.clone(),
            },
        );

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.contains("/nonexistent/farik/gh"),
            "{}",
            raised[0].detail
        );
        assert!(opened(&harness).is_empty());
        let _ = std::fs::remove_dir_all(&origin);
    }
}
