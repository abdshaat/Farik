//! What happens to a task's work once it is finished (`docs/SPEC.md` 5.14): an accepted task's
//! branch integrated the way the team's policy says, one task at a time even across processes,
//! and a finished task's worktrees and containers removed, its branch kept.

use std::fs::File;
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

/// Rule 2: an accepted task awaiting integration under `auto_merge` is merged and pushed, and
/// under `pull_request` has its pull request opened, then its state read on each tick until it is
/// merged or closed; an open one is not an action, and the tick goes on. None of it once an
/// integration escalation was raised since the task was accepted: that one is the human's,
/// through `integrate`, and is not tried again on every tick. ponytail: one gh call per open pull
/// request per tick; a slower poll when a board holds many.
pub(super) async fn awaiting(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if row.kind != TaskKind::Task
        || !row.awaiting_integration
        || team.policy.integration == Integration::Manual
        || escalated_since_accepted(&orchestrator.deps.tools, &row.task_id)?
    {
        return Ok(None);
    }
    let outcome = match attempt(orchestrator, team, &row.task_id, false).await {
        Err(OrchestratorError::Refused { .. }) | Ok(IntegrationOutcome::AwaitingForge) => {
            return Ok(None);
        }
        other => other?,
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what: match outcome {
            IntegrationOutcome::Merged { sha } => format!("integrated it at {sha}"),
            IntegrationOutcome::PullRequestOpened { url } => format!("opened {url}"),
            IntegrationOutcome::AwaitingForge => "its pull request is open".to_string(),
            IntegrationOutcome::Escalated { detail } => {
                format!("could not integrate it, and told the human: {detail}")
            }
        },
    }))
}

/// `Orchestrator::integrate`: the human's integration of an accepted task, now.
pub(super) async fn integrate(
    orchestrator: &Orchestrator,
    task_id: &TaskId,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let team = orchestrator.deps.tools.files.read_team()?;
    attempt(orchestrator, &team, task_id, true).await
}

/// One integration, under the lock and off the async threads, because it runs git.
async fn attempt(
    orchestrator: &Orchestrator,
    team: &Team,
    task_id: &TaskId,
    by_human: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let tools = Arc::clone(&orchestrator.deps.tools);
    let forge = Arc::clone(&orchestrator.deps.forge);
    let team = team.clone();
    let task_id = task_id.clone();
    tokio::task::spawn_blocking(move || integrate_locked(&tools, &forge, &team, &task_id, by_human))
        .await
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))
}

/// Takes the lock, brings the board up to what other processes appended, and integrates the task
/// by the policy in force, unless it is no longer awaiting integration.
fn integrate_locked(
    tools: &ToolDeps,
    forge: &Forge,
    team: &Team,
    task_id: &TaskId,
    by_human: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let _held = lock(tools.files.root())?;
    tools.projections.catch_up()?;
    let row = tools
        .projections
        .task(task_id)?
        .ok_or_else(|| OrchestratorError::Refused {
            reason: format!(
                "no_such_task: the log has nothing about {}",
                task_id.as_str()
            ),
        })?;
    if row.status != TaskStatus::Accepted {
        return Err(OrchestratorError::Refused {
            reason: format!(
                "not_accepted: {} is {}, and only accepted work integrates",
                task_id.as_str(),
                row.status
            ),
        });
    }
    if row.kind != TaskKind::Task {
        return Err(OrchestratorError::Refused {
            reason: format!(
                "an_epic: {} is an epic, whose children carry the branches",
                task_id.as_str()
            ),
        });
    }
    if !row.awaiting_integration {
        return Ok(IntegrationOutcome::Merged {
            sha: last_integrated_sha(tools, task_id)?.unwrap_or_default(),
        });
    }
    match team.policy.integration {
        Integration::AutoMerge => merge(tools, team, &row, by_human, true),
        Integration::Manual => merge(tools, team, &row, by_human, false),
        Integration::PullRequest => through_the_forge(tools, forge, team, &row, by_human),
    }
}

/// Under `pull_request`: opens the task's pull request when none was opened since it was
/// accepted, pushing its branch first; otherwise reads the one opened. Merged on the forge, the
/// local integration branch is brought up to it and the task recorded as the human's
/// integration. Closed, the human's own `integrate` asks whether the branch is in the integration
/// branch all the same (merged by hand, or through another pull request); the tick only says so.
fn through_the_forge(
    tools: &ToolDeps,
    forge: &Forge,
    team: &Team,
    row: &TaskProjection,
    by_human: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let git = &tools.git;
    let id = row.task_id.as_str();
    let branch = format!("farik/{id}");
    let into = match integration_branch(team, git) {
        Ok(into) => into,
        Err(error) => return escalate(tools, &row.task_id, format!("{error}")),
    };
    let Some(url) = opened_since_accepted(tools, &row.task_id)? else {
        return open(tools, forge, row, &branch, &into);
    };
    let state = match forge.pull_request_state(&url) {
        Ok(state) => state,
        Err(error) => {
            return escalate(
                tools,
                &row.task_id,
                format!("the state of {url} could not be read: {error}"),
            );
        }
    };
    match state {
        PullRequestState::Open => Ok(IntegrationOutcome::AwaitingForge),
        PullRequestState::Merged { sha } => {
            let fetched = git.fetch_fast_forward("origin", &into);
            record_integrated(tools, &row.task_id, &sha, &into, true)?;
            if let Err(error) = fetched {
                return escalate(
                    tools,
                    &row.task_id,
                    format!(
                        "{url} was merged as {sha}, but the local {into} could not be brought up \
                         to it: {}; run git pull origin {into} in the repository",
                        git_words(&error)
                    ),
                );
            }
            Ok(IntegrationOutcome::Merged { sha })
        }
        PullRequestState::Closed if !by_human => escalate(
            tools,
            &row.task_id,
            format!(
                "the pull request {url} was closed without merging; reopen it on the forge or \
                 merge {branch}, then run farik integrate {id}"
            ),
        ),
        PullRequestState::Closed => {
            let merged = git
                .fetch_fast_forward("origin", &into)
                .and_then(|()| git.commit_count(&into, &branch));
            match merged {
                Ok(0) => {
                    // A branch's merge base with itself is its head: the commit of `into` that
                    // holds the task's branch.
                    let sha = git.merge_base(&into, &into)?;
                    record_integrated(tools, &row.task_id, &sha, &into, true)?;
                    Ok(IntegrationOutcome::Merged { sha })
                }
                Ok(_) => escalate(
                    tools,
                    &row.task_id,
                    format!(
                        "the pull request {url} was closed without merging, and {branch} is not \
                         in {into}: reopen it on the forge or merge the branch, then run farik \
                         integrate {id}"
                    ),
                ),
                Err(error) => escalate(
                    tools,
                    &row.task_id,
                    format!(
                        "the pull request {url} was closed without merging, and whether {branch} \
                         is in {into} could not be read: {}",
                        git_words(&error)
                    ),
                ),
            }
        }
    }
}

/// Pushes `branch` to `origin` and opens its pull request into `into`, recording it; each failure,
/// no `origin` among them, is an integration escalation with its words.
fn open(
    tools: &ToolDeps,
    forge: &Forge,
    row: &TaskProjection,
    branch: &str,
    into: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let git = &tools.git;
    match git.has_remote("origin") {
        Ok(true) => {}
        Ok(false) => {
            return escalate(
                tools,
                &row.task_id,
                format!(
                    "there is no origin to push {branch} to and open its pull request on; add \
                     one, or choose another policy.integration"
                ),
            );
        }
        Err(error) => return escalate(tools, &row.task_id, git_words(&error)),
    }
    if let Err(error) = git.push("origin", &format!("refs/heads/{branch}")) {
        return escalate(
            tools,
            &row.task_id,
            format!("pushing {branch} to origin failed: {}", git_words(&error)),
        );
    }
    let body = pull_request_body(tools, &row.task_id)?;
    let title = format!("{}: {}", row.task_id.as_str(), row.title);
    let opened = match forge.open_pull_request(into, branch, &title, &body) {
        Ok(opened) => opened,
        Err(error) => {
            return escalate(
                tools,
                &row.task_id,
                format!("the pull request for {branch} could not be opened: {error}"),
            );
        }
    };
    append(
        tools,
        &row.task_id,
        EventBody::PullRequestOpened(PullRequestOpenedBody {
            url: opened.url.clone(),
            number: opened.number,
            branch: branch.to_string(),
        }),
    )?;
    Ok(IntegrationOutcome::PullRequestOpened { url: opened.url })
}

/// The pull request's body: the contract's intent, the completion note, and the review note, each
/// under its own heading; a note never written is said to be missing.
fn pull_request_body(tools: &ToolDeps, task_id: &TaskId) -> Result<String, OrchestratorError> {
    let contract = tools.files.read_contract(task_id)?;
    let notes = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::NoteWritten],
        ..EventQuery::default()
    })?;
    let last = |kind: NoteWrittenBodyKind| {
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
        "## Intent\n\n{}\n\n## Completion note\n\n{}\n\n## Review note\n\n{}\n",
        contract.intent.as_str(),
        last(NoteWrittenBodyKind::Completion),
        last(NoteWrittenBodyKind::Review)
    ))
}

/// The url of the task's `pull_request.opened` since it last moved into `accepted`.
fn opened_since_accepted(
    tools: &ToolDeps,
    task_id: &TaskId,
) -> Result<Option<String>, OrchestratorError> {
    let history = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned, EventKind::PullRequestOpened],
        ..EventQuery::default()
    })?;
    Ok(history
        .iter()
        .rev()
        .take_while(|event| !is_move_into_accepted(event))
        .find_map(|event| match &event.body {
            EventBody::PullRequestOpened(body) => Some(body.url.clone()),
            _ => None,
        }))
}

/// Merges `farik/<id>` into the integration branch and records it; then, when `pushes` and the
/// repository has an `origin`, pushes the integration branch there. A conflict, git refusing the
/// merge, or a failed push is an integration escalation; a failed push leaves the merge, because
/// the local integration branch is what dependents start from.
fn merge(
    tools: &ToolDeps,
    team: &Team,
    row: &TaskProjection,
    by_human: bool,
    pushes: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let git = &tools.git;
    let id = row.task_id.as_str();
    let branch = format!("farik/{id}");
    let into = match integration_branch(team, git) {
        Ok(into) => into,
        Err(error) => return escalate(tools, &row.task_id, format!("{error}")),
    };
    let sha = match git.merge(&into, &branch, &format!("Merge {id}: {}", row.title)) {
        Ok(MergeOutcome::Merged { sha }) => sha,
        Ok(MergeOutcome::Conflicts(paths)) => {
            return escalate(
                tools,
                &row.task_id,
                format!(
                    "merging {branch} into {into} conflicts in {}; resolve it on {into} or on \
                     {branch}, then run farik integrate {id}",
                    paths.join(", ")
                ),
            );
        }
        Err(error) => {
            return escalate(
                tools,
                &row.task_id,
                format!("merging {branch} into {into} failed: {error}"),
            );
        }
    };
    record_integrated(tools, &row.task_id, &sha, &into, by_human)?;
    if pushes {
        let push = git.has_remote("origin").and_then(|has| {
            if has {
                git.push("origin", &format!("refs/heads/{into}"))
            } else {
                Ok(())
            }
        });
        if let Err(error) = push {
            return escalate(
                tools,
                &row.task_id,
                format!(
                    "merged locally as {sha}; pushing {into} to origin failed: {}; run git push \
                     origin {into} once it can be pushed",
                    git_words(&error)
                ),
            );
        }
    }
    Ok(IntegrationOutcome::Merged { sha })
}

/// Records the task's branch as in `into` at `sha`, by the human or by Farik.
fn record_integrated(
    tools: &ToolDeps,
    task_id: &TaskId,
    sha: &str,
    into: &str,
    by_human: bool,
) -> Result<(), OrchestratorError> {
    append(
        tools,
        task_id,
        EventBody::TaskIntegrated(TaskIntegratedBody {
            sha: sha.to_string(),
            into: into.to_string(),
            integrated_by: if by_human {
                TaskIntegratedBodyIntegratedBy::Human
            } else {
                TaskIntegratedBodyIntegratedBy::Governor
            },
        }),
    )
}

/// Whether the event is a move into `accepted`.
fn is_move_into_accepted(event: &FarikEvent) -> bool {
    matches!(&event.body, EventBody::TaskTransitioned(body) if body.to.to_string() == "accepted")
}

/// What git said, without the adapter's framing.
fn git_words(error: &GitError) -> String {
    match error {
        GitError::CommandFailed { stderr, .. } => stderr.clone(),
        other => other.to_string(),
    }
}

/// Raises an integration escalation with `detail`, which moves nothing: the task stays accepted
/// and awaiting (5.2, 5.14), and answers `Escalated` with the same words.
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

/// Appends one event about the task, stamped with no agent and no session, and projects it.
fn append(tools: &ToolDeps, task_id: &TaskId, body: EventBody) -> Result<(), OrchestratorError> {
    let ids = EventIds {
        task_id: Some(task_id.clone()),
        agent_id: None,
        session_id: None,
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

/// The sha of the task's last `task.integrated`.
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

/// Whether an integration escalation was raised about the task since it last moved into
/// `accepted`.
fn escalated_since_accepted(tools: &ToolDeps, task_id: &TaskId) -> Result<bool, OrchestratorError> {
    let history = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned, EventKind::EscalationRaised],
        ..EventQuery::default()
    })?;
    Ok(history
        .iter()
        .rev()
        .take_while(|event| !is_move_into_accepted(event))
        .any(|event| {
            matches!(&event.body, EventBody::EscalationRaised(body)
                if body.reason == EscalationRaisedBodyReason::Integration)
        }))
}

/// The integration lock: an exclusive lock on `.farik/local/integration.lock`, held until the
/// file is dropped. A file lock rather than an in-process one, because `farik integrate` runs in
/// its own process beside `farik run`; two opens in one process conflict too, so one lock serves
/// both (5.14).
fn lock(root: &Path) -> Result<File, OrchestratorError> {
    let path = root.join(LOCK);
    let failed = |error: std::io::Error| OrchestratorError::Lock {
        detail: format!("{}: {error}", path.display()),
    };
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(failed)?;
    }
    let file = File::options()
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
            harness.accepted("FRK-1");
            // FRK-2's branch gets a commit of its own: the fixtures' two `done.txt` commits, made
            // in the same second on the same parent, would be one commit.
            harness.accepted_with_worktree("FRK-2");
            let worktree = harness.worktree("FRK-2");
            std::fs::write(worktree.join("two.txt"), "two\n").expect("written");
            let git = &harness.project.deps.git;
            git.commit(&worktree, "Add two.txt", &["two.txt".to_string()])
                .expect("committed");
            git.remove_worktree(&worktree).expect("removed");
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
