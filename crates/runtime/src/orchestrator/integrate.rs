//! What happens to a task's work once it is finished (`docs/SPEC.md` 5.14): an accepted task's
//! branch integrated the way the team's policy says, one task at a time even across processes,
//! and a finished task's worktrees and containers removed, its branch kept.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use catervas_core::branch::task_branch;
use catervas_core::contract::{TaskId, TaskKind, TaskStatus};
use catervas_core::team::{Integration, Team, task_private_folder};
use catervas_protocol::event::{
    CatervasEvent, EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventKind,
    NoteWrittenBodyKind, PullRequestOpenedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
};
use catervas_protocol::generated::event::{CriteriaUpdatedBody, ProjectScannedBody};
use catervas_store::files::FilesError;
use catervas_store::folder_docs::{FolderChange, folder_docs};
use catervas_store::{
    EventQuery, Git, GitError, MergeOutcome, TaskProjection, material, names_of, project_document,
    scan_project, seeded_library,
};

use super::verify::{GOVERNOR, append, append_stamped};
use super::{
    IntegrationOutcome, Orchestrator, OrchestratorError, ScanRefresh, TickReport, worktree,
};
use crate::channel::post_system;
use crate::forge::{Forge, PullRequestState};
use crate::tools::ToolDeps;
use crate::transitions::{INTEGRATION_LOCK, integration_branch, integration_lock, is_move_into};

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
    let subject = Subject::Task(row.task_id.clone());
    let Some(outcome) = attempt(orchestrator, team, &subject, Asker::Tick).await? else {
        return Ok(None);
    };
    let what = match outcome {
        IntegrationOutcome::Merged { sha, scan } => format!("integrated at {sha}{scan}"),
        IntegrationOutcome::PullRequestOpened { url } => format!("opened the pull request {url}"),
        IntegrationOutcome::Escalated { detail, scan } => format!(
            "escalated its integration to the human: {detail}{}",
            scan.map(|scan| scan.to_string()).unwrap_or_default()
        ),
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
    // A task in a private folder has no branch: accepting it was its end (6.6).
    if task_private_folder(&orchestrator.deps.tools.files.read_contract(task_id)?).is_some() {
        return Err(refused(format!(
            "nothing_to_integrate: {} works in a private folder and has no branch; its acceptance \
             was its end",
            task_id.as_str()
        )));
    }
    let team = orchestrator.deps.tools.files.read_team()?;
    attempt(
        orchestrator,
        &team,
        &Subject::Task(task_id.clone()),
        Asker::Human,
    )
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

/// What an integration is about: an accepted task's branch, or a folder change's (5.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Subject {
    Task(TaskId),
    FolderChange(u64),
}

impl Subject {
    /// How the human names it: `catervas integrate <this>`.
    fn name(&self) -> String {
        match self {
            Self::Task(task_id) => task_id.as_str().to_string(),
            Self::FolderChange(change) => format!("folder-{change}"),
        }
    }

    /// Its branch.
    fn branch(&self, tools: &ToolDeps) -> Result<String, OrchestratorError> {
        Ok(match self {
            Self::Task(task_id) => task_branch(&tools.files.read_contract(task_id)?),
            Self::FolderChange(change) => format!("docs/folder-{change}"),
        })
    }

    /// Its merge commit's message and its pull request's title.
    fn titles(&self, tools: &ToolDeps) -> Result<(String, String), OrchestratorError> {
        match self {
            Self::Task(task_id) => {
                let title = tools
                    .projections
                    .task(task_id)?
                    .map(|row| row.title)
                    .unwrap_or_default();
                let id = task_id.as_str();
                Ok((format!("Merge {id}: {title}"), format!("{id}: {title}")))
            }
            Self::FolderChange(change) => {
                let paths = folder_change(tools, *change)?
                    .paths
                    .iter()
                    .map(|path| {
                        path.strip_prefix("docs/catervas/")
                            .unwrap_or(path)
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok((
                    format!("Merge docs/folder-{change}: {paths}"),
                    format!("Folder change {change}: {paths}"),
                ))
            }
        }
    }
}

/// The folder change numbered `change`, or `no_such_folder_change`.
fn folder_change(tools: &ToolDeps, change: u64) -> Result<FolderChange, OrchestratorError> {
    folder_docs(&tools.log)?
        .changes
        .into_iter()
        .find(|known| known.change == change)
        .ok_or_else(|| {
            refused(format!(
                "no_such_folder_change: there is no folder change {change}"
            ))
        })
}

/// Rule 2's second half: each folder change not integrated is integrated the way the team's policy
/// says, as `awaiting` does for each accepted task, in a tick scoped to no task. One that was
/// escalated is the human's to settle through `integrate_folder_change`; `manual` has no rule.
pub(super) async fn awaiting_folder_changes(
    orchestrator: &Orchestrator,
    team: &Team,
) -> Result<Option<TickReport>, OrchestratorError> {
    if team.policy.integration == Integration::Manual {
        return Ok(None);
    }
    let waiting: Vec<u64> = folder_docs(&orchestrator.deps.tools.log)?
        .changes
        .iter()
        .filter(|change| !change.integrated && !change.escalated)
        .map(|change| change.change)
        .collect();
    for change in waiting {
        let subject = Subject::FolderChange(change);
        let Some(outcome) = attempt(orchestrator, team, &subject, Asker::Tick).await? else {
            continue;
        };
        let what = match outcome {
            IntegrationOutcome::Merged { sha, .. } => format!("integrated at {sha}"),
            IntegrationOutcome::PullRequestOpened { url } => {
                format!("opened the pull request {url}")
            }
            IntegrationOutcome::Escalated { detail, .. } => {
                format!("escalated its integration to the human: {detail}")
            }
            IntegrationOutcome::AwaitingForge => continue,
        };
        return Ok(Some(TickReport::FolderChange { change, what }));
    }
    Ok(None)
}

/// The human's integration of a folder change, whatever escalations it carries.
pub(super) async fn integrate_folder_change(
    orchestrator: &Orchestrator,
    change: u64,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let team = orchestrator.deps.tools.files.read_team()?;
    let subject = Subject::FolderChange(change);
    attempt(orchestrator, &team, &subject, Asker::Human)
        .await?
        .ok_or_else(|| refused(format!("nothing_to_do: {}", subject.name())))
}

/// One integration, under the lock and off the async threads, because it runs git; `None` when
/// the tick finds nothing to do once it holds the lock.
async fn attempt(
    orchestrator: &Orchestrator,
    team: &Team,
    subject: &Subject,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let tools = Arc::clone(&orchestrator.deps.tools);
    let forge = Arc::clone(&orchestrator.deps.forge);
    let team = team.clone();
    let subject = subject.clone();
    let finished = tokio::task::spawn_blocking(move || {
        let _lock = lock(tools.files.root())?;
        integrate_locked(&tools, &forge, &team, &subject, asker)
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

/// What the policy does for the subject, with the lock held: the board is brought up to what other
/// processes appended first, so that a task another one integrated is answered, not merged again.
fn integrate_locked(
    tools: &ToolDeps,
    forge: &Forge,
    team: &Team,
    subject: &Subject,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    tools.projections.catch_up()?;
    let awaiting = match subject {
        Subject::Task(task_id) => {
            let Some(row) = tools.projections.task(task_id)? else {
                return Err(refused(format!("no_such_task: {}", task_id.as_str())));
            };
            refuse_unless_integrable(&row)?;
            row.awaiting_integration
        }
        Subject::FolderChange(change) => !folder_change(tools, *change)?.integrated,
    };
    if !awaiting {
        return match last_integrated_sha(tools, subject)? {
            Some(sha) => Ok(Some(IntegrationOutcome::Merged {
                sha,
                scan: ScanRefresh::Unchanged,
            })),
            None if asker == Asker::Tick => Ok(None),
            None => Err(refused(format!(
                "never_integrated: {} is not awaiting integration and was never integrated",
                subject.name()
            ))),
        };
    }
    if asker == Asker::Tick && escalated_since_accepted(tools, subject)? {
        return Ok(None);
    }
    let into = match integration_branch(team, &tools.git) {
        Ok(into) => into,
        Err(error) => return escalate(tools, subject, git_words(&error)).map(Some),
    };
    let integrated_by = match asker {
        Asker::Tick => TaskIntegratedBodyIntegratedBy::Governor,
        Asker::Human => TaskIntegratedBodyIntegratedBy::Human,
    };
    let integrated_before = integrations_of(tools, subject)?;
    let outcome = match team.policy.integration {
        Integration::Manual => Some(merge(tools, subject, &into, integrated_by, false)?),
        Integration::AutoMerge => Some(merge(tools, subject, &into, integrated_by, true)?),
        Integration::PullRequest => through_the_forge(tools, forge, subject, &into, asker)?,
    };
    // A folder change touches no language, toolchain, test or criterion: no scan to refresh.
    let Subject::Task(task_id) = subject else {
        return Ok(outcome);
    };
    if integrations_of(tools, subject)? == integrated_before {
        return Ok(outcome);
    }
    let scan = refresh_the_scan(tools, task_id, &into);
    Ok(outcome.map(|outcome| match outcome {
        IntegrationOutcome::Merged { sha, .. } => IntegrationOutcome::Merged { sha, scan },
        IntegrationOutcome::Escalated { detail, .. } => IntegrationOutcome::Escalated {
            detail,
            scan: Some(scan),
        },
        other => other,
    }))
}

/// How many times the subject was integrated, so that a caller can tell whether it just was.
fn integrations_of(tools: &ToolDeps, subject: &Subject) -> Result<usize, OrchestratorError> {
    Ok(match subject {
        Subject::Task(task_id) => tools
            .log
            .read(&EventQuery {
                task_id: Some(task_id.clone()),
                kinds: vec![EventKind::TaskIntegrated],
                ..EventQuery::default()
            })?
            .len(),
        Subject::FolderChange(change) => tools
            .log
            .read(&EventQuery {
                kinds: vec![EventKind::FolderChangeIntegrated],
                ..EventQuery::default()
            })?
            .iter()
            .filter(|event| {
                matches!(&event.body, EventBody::FolderChangeIntegrated(body)
                if body.change.get() == *change)
            })
            .count(),
    })
}

/// Scans the project again once work landed in it (5.8), and writes `project.md` and the
/// criterion library when what the scan reads changed. It never fails the integration: whatever
/// goes wrong is the answer's words, not an error.
fn refresh_the_scan(tools: &ToolDeps, task_id: &TaskId, into: &str) -> ScanRefresh {
    // `scan_project` reads the working tree at the root, which holds what landed only when it is
    // the integration branch that is checked out there.
    match tools.git.current_branch() {
        Ok(branch) if branch == into => {}
        Ok(branch) => {
            return ScanRefresh::Skipped {
                why: format!("the checkout is on {branch}, not {into}"),
            };
        }
        Err(_) => {
            return ScanRefresh::Skipped {
                why: "the checkout is not on a branch".to_string(),
            };
        }
    }
    rescan(tools, task_id).unwrap_or_else(|error| ScanRefresh::Failed { error })
}

/// The refresh itself, once the checkout is known to hold what landed.
fn rescan(tools: &ToolDeps, task_id: &TaskId) -> Result<ScanRefresh, String> {
    let scan = scan_project(&tools.git, tools.clock.now()).map_err(|error| error.to_string())?;
    let found = names_of(&scan.detected_criteria);
    let last = tools
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::ProjectScanned],
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?
        .into_iter()
        .rev()
        .find_map(|event| match event.body {
            EventBody::ProjectScanned(body) => Some(body),
            _ => None,
        });
    if last.is_some_and(|last| {
        material(&last.read_back, &last.detected_criteria) == material(&scan.read_back, &found)
    }) {
        return Ok(ScanRefresh::Unchanged);
    }
    let kept = match tools.files.read_criteria() {
        Ok(library) => Some(library),
        Err(FilesError::NotFound { .. }) => None,
        Err(error) => return Err(error.to_string()),
    };
    let library = seeded_library(&scan.detected_criteria, kept.as_ref());
    let words = |error: OrchestratorError| error.to_string();
    let previous = tools.files.read_project_scan().ok();
    tools
        .files
        .write_project_scan(&project_document(&scan, &library, previous.as_deref()))
        .map_err(|error| error.to_string())?;
    if kept.as_ref() != Some(&library) {
        tools
            .files
            .write_criteria(&library)
            .map_err(|error| error.to_string())?;
        append(
            tools,
            task_id,
            None,
            None,
            EventBody::CriteriaUpdated(CriteriaUpdatedBody {
                criterion_names: names_of(&library.criteria),
                updated_by: GOVERNOR.to_string(),
            }),
        )
        .map_err(words)?;
    }
    // Last, as the mark that the refresh finished: one that failed before it is compared against
    // the scan before, and so is done again by the next integration.
    append(
        tools,
        task_id,
        None,
        None,
        EventBody::ProjectScanned(ProjectScannedBody {
            detected_criteria: found,
            read_back: scan.read_back.clone(),
        }),
    )
    .map_err(words)?;
    Ok(ScanRefresh::Refreshed)
}

/// Under `pull_request`: with no pull request opened since acceptance, the branch pushed to
/// `origin` and one opened; with one, its state read. A merge on the forge is recorded as the
/// human's and the local integration branch brought up to it; a closed one is escalated, unless
/// the human asks and the branch is in the integration branch all the same.
fn through_the_forge(
    tools: &ToolDeps,
    forge: &Forge,
    subject: &Subject,
    into: &str,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let branch = subject.branch(tools)?;
    let Some(url) = opened_url(tools, subject)? else {
        return open(tools, forge, subject, into, &branch).map(Some);
    };
    let state = match forge.pull_request_state(&url) {
        Ok(state) => state,
        Err(error) => {
            let detail = format!("reading the pull request {url} failed: {error}");
            return escalate(tools, subject, detail).map(Some);
        }
    };
    match state {
        PullRequestState::Open => Ok(Some(IntegrationOutcome::AwaitingForge)),
        PullRequestState::Merged { sha } => {
            record_integrated(
                tools,
                subject,
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
                return escalate(tools, subject, detail).map(Some);
            }
            Ok(Some(IntegrationOutcome::Merged {
                sha,
                scan: ScanRefresh::Unchanged,
            }))
        }
        PullRequestState::Closed if asker == Asker::Tick => {
            let detail = format!("the pull request {url} was closed without merging");
            escalate(tools, subject, detail).map(Some)
        }
        PullRequestState::Closed => {
            closed_for_the_human(tools, subject, &url, into, &branch).map(Some)
        }
    }
}

/// The address of the pull request opened for the subject: the latest since a task was accepted,
/// the latest for a folder change.
fn opened_url(tools: &ToolDeps, subject: &Subject) -> Result<Option<String>, OrchestratorError> {
    Ok(match subject {
        Subject::Task(task_id) => since_accepted(tools, task_id, &[EventKind::PullRequestOpened])?
            .into_iter()
            .rev()
            .find_map(|event| match event.body {
                EventBody::PullRequestOpened(body) => Some(body.url),
                _ => None,
            }),
        Subject::FolderChange(change) => tools
            .log
            .read(&EventQuery {
                kinds: vec![EventKind::FolderChangeOpened],
                ..EventQuery::default()
            })?
            .into_iter()
            .rev()
            .find_map(|event| match event.body {
                EventBody::FolderChangeOpened(body) if body.change.get() == *change => {
                    Some(body.url.to_string())
                }
                _ => None,
            }),
    })
}

/// A closed pull request the human asks about: the subject is integrated when its branch is in the
/// integration branch as the forge has it, merged by hand or through another pull request.
fn closed_for_the_human(
    tools: &ToolDeps,
    subject: &Subject,
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
                subject,
                &head,
                into,
                TaskIntegratedBodyIntegratedBy::Human,
            )?;
            Ok(IntegrationOutcome::Merged {
                sha: head,
                scan: ScanRefresh::Unchanged,
            })
        }
        Ok(_) => {
            let detail = format!(
                "the pull request {url} was closed without merging, and {branch} is not in \
                 {into}: reopen it on the forge or merge the branch, then run catervas integrate"
            );
            escalate(tools, subject, detail)
        }
        Err(error) => {
            let detail = format!(
                "the pull request {url} was closed without merging, and whether {branch} is in \
                 {into} could not be read: {}",
                git_words(&error)
            );
            escalate(tools, subject, detail)
        }
    }
}

/// Pushes the branch to `origin` and opens its pull request, recording it.
fn open(
    tools: &ToolDeps,
    forge: &Forge,
    subject: &Subject,
    into: &str,
    branch: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    match tools.git.has_remote("origin") {
        Ok(true) => {}
        Ok(false) => {
            let detail = format!(
                "the pull_request policy pushes {branch} to origin, and there is no remote named \
                 origin"
            );
            return escalate(tools, subject, detail);
        }
        Err(error) => return escalate(tools, subject, git_words(&error)),
    }
    if let Err(error) = tools.git.push("origin", &format!("refs/heads/{branch}")) {
        let detail = format!("pushing {branch} to origin failed: {}", git_words(&error));
        return escalate(tools, subject, detail);
    }
    let (_, title) = subject.titles(tools)?;
    let body = pull_request_body(tools, subject)?;
    let pull_request = match forge.open_pull_request(into, branch, &title, &body) {
        Ok(pull_request) => pull_request,
        Err(error) => {
            let detail = format!("opening a pull request for {branch} failed: {error}");
            return escalate(tools, subject, detail);
        }
    };
    match subject {
        Subject::Task(task_id) => append(
            tools,
            task_id,
            None,
            None,
            EventBody::PullRequestOpened(PullRequestOpenedBody {
                url: pull_request.url.clone(),
                number: pull_request.number,
                branch: branch.to_string(),
            }),
        )?,
        Subject::FolderChange(change) => {
            let body = serde_json::from_value(serde_json::json!({
                "change": change, "url": pull_request.url, "number": pull_request.number,
            }));
            let Ok(body) = body else {
                let detail = format!(
                    "the forge answered {} for the pull request of {branch}, which is no pull \
                     request number",
                    pull_request.number
                );
                return escalate(tools, subject, detail);
            };
            append_stamped(
                tools,
                tools.ids.clone(),
                EventBody::FolderChangeOpened(body),
            )?;
        }
    }
    Ok(IntegrationOutcome::PullRequestOpened {
        url: pull_request.url,
    })
}

/// The pull request's body: a task's contract intent, then the last completion and review notes; a
/// folder change's paths, one per line, then a line for the owner's approval.
fn pull_request_body(tools: &ToolDeps, subject: &Subject) -> Result<String, OrchestratorError> {
    let task_id = match subject {
        Subject::Task(task_id) => task_id,
        Subject::FolderChange(change) => {
            let held = folder_change(tools, *change)?;
            let mut body = held.paths.join("\n");
            body.push('\n');
            if held.approved {
                body.push_str("\nApproved by the owner on Today.\n");
            }
            return Ok(body);
        }
    };
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

/// Merges the subject's branch into `into`, records it, and, when `push` and there is an `origin`,
/// pushes `into` there. A conflict, a git failure, or a failed push is an escalation; the merge
/// stays when only the push failed, since the local integration branch is what dependents start
/// from.
fn merge(
    tools: &ToolDeps,
    subject: &Subject,
    into: &str,
    integrated_by: TaskIntegratedBodyIntegratedBy,
    push: bool,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let id = subject.name();
    let branch = subject.branch(tools)?;
    let (message, _) = subject.titles(tools)?;
    let sha = match tools.git.merge(into, &branch, &message) {
        Ok(MergeOutcome::Merged { sha }) => sha,
        Ok(MergeOutcome::Conflicts(paths)) => {
            let detail = format!(
                "merging {branch} into {into} conflicts in {}: resolve them on {into}, then run \
                 catervas integrate {id}",
                paths.join(", ")
            );
            return escalate(tools, subject, detail);
        }
        Err(error) => {
            let detail = format!("merging {branch} into {into} failed: {}", git_words(&error));
            return escalate(tools, subject, detail);
        }
    };
    record_integrated(tools, subject, &sha, into, integrated_by)?;
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
            return escalate(tools, subject, detail);
        }
    }
    Ok(IntegrationOutcome::Merged {
        sha,
        scan: ScanRefresh::Unchanged,
    })
}

/// Appends `task.integrated`, or `folder_change.integrated`.
fn record_integrated(
    tools: &ToolDeps,
    subject: &Subject,
    sha: &str,
    into: &str,
    integrated_by: TaskIntegratedBodyIntegratedBy,
) -> Result<(), OrchestratorError> {
    match subject {
        Subject::Task(task_id) => append(
            tools,
            task_id,
            None,
            None,
            EventBody::TaskIntegrated(TaskIntegratedBody {
                sha: sha.to_string(),
                into: into.to_string(),
                integrated_by,
            }),
        ),
        Subject::FolderChange(change) => {
            let by = match integrated_by {
                TaskIntegratedBodyIntegratedBy::Governor => "governor",
                TaskIntegratedBodyIntegratedBy::Human => "human",
            };
            let body = serde_json::from_value(serde_json::json!({
                "change": change, "sha": sha, "into": into, "integrated_by": by,
            }))
            .map_err(|error| refused(format!("folder_change_unrecordable: {error}")))?;
            append_stamped(
                tools,
                tools.ids.clone(),
                EventBody::FolderChangeIntegrated(body),
            )
        }
    }
}

/// Appends an integration escalation, which moves nothing, and answers it. A folder change's is
/// also said in the channel, since Today lists the escalations of tasks.
fn escalate(
    tools: &ToolDeps,
    subject: &Subject,
    detail: String,
) -> Result<IntegrationOutcome, OrchestratorError> {
    match subject {
        Subject::Task(task_id) => append(
            tools,
            task_id,
            None,
            None,
            EventBody::EscalationRaised(EscalationRaisedBody {
                reason: EscalationRaisedBodyReason::Integration,
                detail: detail.clone(),
            }),
        )?,
        Subject::FolderChange(change) => {
            // The detail is cut to the event's bound; the channel says what the log keeps.
            let kept: String = detail.chars().take(4_000).collect();
            let body = serde_json::from_value(serde_json::json!({
                "change": change, "detail": kept,
            }))
            .map_err(|error| refused(format!("folder_change_unrecordable: {error}")))?;
            append_stamped(
                tools,
                tools.ids.clone(),
                EventBody::FolderChangeEscalated(body),
            )?;
            post_system(
                &tools.log,
                tools.clock.as_ref(),
                &tools.ids,
                None,
                &format!("Folder change {change} could not be added to the project: {detail}"),
            )?;
        }
    }
    Ok(IntegrationOutcome::Escalated { detail, scan: None })
}

/// Git's own words for a failure.
fn git_words(error: &GitError) -> String {
    match error {
        GitError::CommandFailed { stderr, .. } => stderr.clone(),
        other => other.to_string(),
    }
}

/// The task's events of `kinds` since its last move into `accepted`.
fn since_accepted(
    tools: &ToolDeps,
    task_id: &TaskId,
    kinds: &[EventKind],
) -> Result<Vec<CatervasEvent>, OrchestratorError> {
    let mut asked = vec![EventKind::TaskTransitioned];
    asked.extend_from_slice(kinds);
    let history = tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: asked,
        ..EventQuery::default()
    })?;
    let start = history
        .iter()
        .rposition(|event| is_move_into(event, TaskStatus::Accepted))
        .map_or(0, |at| at + 1);
    Ok(history
        .into_iter()
        .skip(start)
        .filter(|event| kinds.contains(&event.body.kind()))
        .collect())
}

/// Whether an integration escalation was raised since the task was accepted, or for the folder
/// change since it was recorded.
fn escalated_since_accepted(
    tools: &ToolDeps,
    subject: &Subject,
) -> Result<bool, OrchestratorError> {
    match subject {
        Subject::Task(task_id) => {
            Ok(
                since_accepted(tools, task_id, &[EventKind::EscalationRaised])?
                    .iter()
                    .any(|event| {
                        matches!(&event.body, EventBody::EscalationRaised(body)
                    if body.reason == EscalationRaisedBodyReason::Integration)
                    }),
            )
        }
        Subject::FolderChange(change) => Ok(folder_change(tools, *change)?.escalated),
    }
}

/// The commit the subject was last integrated at.
fn last_integrated_sha(
    tools: &ToolDeps,
    subject: &Subject,
) -> Result<Option<String>, OrchestratorError> {
    let (task_id, kind) = match subject {
        Subject::Task(task_id) => (Some(task_id.clone()), EventKind::TaskIntegrated),
        Subject::FolderChange(_) => (None, EventKind::FolderChangeIntegrated),
    };
    let integrated = tools.log.read(&EventQuery {
        task_id,
        kinds: vec![kind],
        ..EventQuery::default()
    })?;
    Ok(integrated.into_iter().rev().find_map(|event| match event.body {
        EventBody::TaskIntegrated(body) => Some(body.sha),
        EventBody::FolderChangeIntegrated(body)
            if matches!(subject, Subject::FolderChange(change) if body.change.get() == *change) =>
        {
            Some(body.sha.to_string())
        }
        _ => None,
    }))
}

/// Takes the integration lock, mapping its error.
fn lock(root: &Path) -> Result<File, OrchestratorError> {
    integration_lock(root).map_err(|error| OrchestratorError::Lock {
        detail: format!("{}: {error}", root.join(INTEGRATION_LOCK).display()),
    })
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
    // The removal already happened, so a contract that cannot be read only loses the name.
    let branch = orchestrator
        .deps
        .tools
        .files
        .read_contract(&row.task_id)
        .map_or_else(
            |_| "its branch".to_string(),
            |contract| task_branch(&contract),
        );
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!("removed its worktree and its sandbox, and kept {branch}"),
    }))
}

/// Removes a finished task's sandbox and worktrees when either worktree is still there, and says
/// whether it removed them. The containers go by name, since after a restart no handle reaches
/// them; then the browser's folder and the base worktree; then the task's own worktree last, so that a run stopped in
/// between leaves it, which is what brings this back.
///
/// Containers that cannot be removed (docker is down) leave the worktrees as they are, so that
/// the task is taken up again on a later tick, and answer `false`, so that a docker outage holds
/// up no other rule.
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
    if deps
        .sandboxes
        .remove(&deps.tools.ids.project_id, task_id)
        .is_err()
    {
        // ponytail: nothing reports the failure; the board shows the task's worktree kept.
        return Ok(false);
    }
    // What the task's browser sessions saved (step 12), before the worktree that brings this back.
    let browser = deps
        .tools
        .files
        .root()
        .join(".catervas/local/browser")
        .join(task_id.as_str());
    match std::fs::remove_dir_all(&browser) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(OrchestratorError::Files(FilesError::Io {
                path: browser.display().to_string(),
                detail: error.to_string(),
            }));
        }
        _ => {}
    }
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
    use catervas_core::contract::TaskStatus;
    use catervas_protocol::event::{
        EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventKind, NewEvent,
        PullRequestOpenedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
        event_from_value,
    };
    use catervas_store::git::fixtures::{git_in, git_output_in};
    use serde_json::json;

    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{IntegrationOutcome, OrchestratorError, ScanRefresh, TickReport};

    const NOTHING_TO_DO: &str = "nothing on the board needs doing";

    fn idle() -> TickReport {
        TickReport::Idle {
            why: NOTHING_TO_DO.to_string(),
            until: None,
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

    fn task(id: &str) -> catervas_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_and_pushes_under_auto_merge() {
        let harness = under("int-auto-merge", "auto_merge");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        // A tag named as the integration branch: a push that did not spell `refs/heads/main` would
        // be refused as matching both.
        git_in(&harness.project.repo.path, &["tag", "main"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        assert!(
            escalations(&harness).is_empty(),
            "{:?}",
            escalations(&harness)
        );
        let head = at_root(&harness, &["rev-parse", "refs/heads/main"]);
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].sha, head);
        assert_eq!(integrated[0].into, "main");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Governor
        );
        let parents = at_root(
            &harness,
            &["rev-list", "--parents", "-n", "1", "refs/heads/main"],
        );
        let branch = at_root(&harness, &["rev-parse", &harness.branch("CTV-1")]);
        assert!(
            parents.split(' ').skip(1).any(|parent| parent == branch),
            "{parents}"
        );
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), head);
        assert_eq!(
            at_root(&harness, &["symbolic-ref", "HEAD"]),
            "refs/heads/main"
        );
        assert!(!harness.row("CTV-1").awaiting_integration);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_a_fix_branch() {
        let harness = under("int-fix", "auto_merge");
        harness.verifying_with("CTV-1", true, true, |wire| wire["change"] = json!("fix"));
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let mut said = Vec::new();
        for _ in 0..2 {
            if let TickReport::Acted { what, .. } =
                orchestrator.tick().await.expect("the tick runs")
            {
                said.push(what);
            }
        }

        assert!(
            escalations(&harness).is_empty(),
            "{:?}",
            escalations(&harness)
        );
        assert_eq!(integrations(&harness).len(), 1, "{said:?}");
        let parents = at_root(
            &harness,
            &["rev-list", "--parents", "-n", "1", "refs/heads/main"],
        );
        let branch = at_root(&harness, &["rev-parse", "fix/CTV-1"]);
        assert_eq!(
            parents.split(' ').nth(2),
            Some(branch.as_str()),
            "{parents}"
        );
        assert!(
            said.iter().any(|what| what.ends_with("and kept fix/CTV-1")),
            "{said:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_without_a_push_when_there_is_no_origin() {
        let harness = under("int-no-origin", "auto_merge");
        harness.accepted("CTV-1");
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
        harness.accepted("CTV-1");
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
            orchestrator.integrate(&task("CTV-1")).await,
            Ok(IntegrationOutcome::Merged {
                sha: integrated[0].sha.clone(),
                scan: ScanRefresh::Unchanged,
            })
        );
        assert_eq!(harness.events(&[]).len(), before);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_conflict_and_leaves_the_task_accepted() {
        let harness = under("int-conflict", "auto_merge");
        harness.accepted_in_conflict("CTV-1");
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
        let row = harness.row("CTV-1");
        assert_eq!(row.status, TaskStatus::Accepted);
        assert!(row.awaiting_integration);
        assert!(integrations(&harness).is_empty());
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_for_the_human_after_an_escalation() {
        let harness = under("int-human", "auto_merge");
        harness.accepted_in_conflict("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");

        let again = orchestrator.integrate(&task("CTV-1")).await;
        assert!(
            matches!(&again, Ok(IntegrationOutcome::Escalated { detail, scan: None }) if detail.contains("a.txt")),
            "{again:?}"
        );
        assert_eq!(escalations(&harness).len(), 2);

        harness.resolve_the_conflict();
        let merged = orchestrator.integrate(&task("CTV-1")).await;
        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_eq!(
            merged,
            Ok(IntegrationOutcome::Merged {
                sha: head.clone(),
                scan: ScanRefresh::Refreshed
            })
        );
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
        harness.accepted("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");
        let sha = integrations(&harness)[0].sha.clone();

        assert_eq!(
            orchestrator.integrate(&task("CTV-1")).await,
            Ok(IntegrationOutcome::Merged {
                sha: sha.clone(),
                scan: ScanRefresh::Unchanged
            })
        );
        assert_eq!(integrations(&harness).len(), 1);
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_an_accepted_task_to_the_human_under_manual() {
        let harness = under("int-manual", "manual");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        let pushed = git_output_in(&origin, &["rev-parse", "main"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert!(integrations(&harness).is_empty());
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), pushed);

        let merged = orchestrator.integrate(&task("CTV-1")).await;
        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_ne!(head, pushed);
        assert_eq!(
            merged,
            Ok(IntegrationOutcome::Merged {
                sha: head,
                scan: ScanRefresh::Refreshed
            })
        );
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), pushed);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_integrate_what_is_not_accepted() {
        let harness = under("int-not-accepted", "auto_merge");
        harness.verifying("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let refused = orchestrator.integrate(&task("CTV-1")).await;

        assert!(
            matches!(&refused, Err(OrchestratorError::Refused { reason }) if reason.starts_with("not_accepted: CTV-1")),
            "{refused:?}"
        );
        assert!(integrations(&harness).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_integrate_a_task_in_a_private_folder() {
        // It was accepted with nothing to integrate: it has no branch, and acceptance is its end.
        let harness = Harness::new("int-private-folder", |wire| {
            wire["policy"]["integration"] = json!("auto_merge");
            crate::tools::fixtures::with_the_finance_specialist(wire);
        });
        harness.finance_task("CTV-1", Some("verifying"));
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "fin",
                     "reviewer": "pm", "effects": ["nothing_to_integrate"] }),
        );
        assert!(!harness.row("CTV-1").awaiting_integration);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let refused = orchestrator.integrate(&task("CTV-1")).await;

        assert!(
            matches!(&refused, Err(OrchestratorError::Refused { reason }) if reason.starts_with("nothing_to_integrate: CTV-1")),
            "{refused:?}"
        );
        assert!(integrations(&harness).is_empty());
        // Nor does a tick try: nothing awaits.
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        // An earlier refusal still comes first: a task that is not accepted is not accepted.
        harness.finance_task("CTV-2", Some("verifying"));
        let refused = orchestrator.integrate(&task("CTV-2")).await;
        assert!(
            matches!(&refused, Err(OrchestratorError::Refused { reason }) if reason.starts_with("not_accepted: CTV-2")),
            "{refused:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_integrate_an_epic_or_a_task_it_does_not_know() {
        let harness = under("int-refusals", "auto_merge");
        harness
            .project
            .filed_with("CTV-1", "accepted", "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
            });
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        for (task_id, prefix) in [
            ("CTV-1", "an_epic: CTV-1"),
            ("CTV-9", "no_such_task: CTV-9"),
        ] {
            let refused = orchestrator.integrate(&task(task_id)).await;
            assert!(
                matches!(&refused, Err(OrchestratorError::Refused { reason }) if reason.starts_with(prefix)),
                "{task_id}: {refused:?}"
            );
        }
        assert!(integrations(&harness).is_empty());
    }

    /// Appends an event to the log without projecting it, as another process that stopped between
    /// the two would leave it.
    fn appended_elsewhere(harness: &Harness, task: &str, kind: &str, body: &serde_json::Value) {
        let wire = json!({
            "seq": 1,
            "recorded_at": crate::tools::fixtures::at().to_rfc3339(),
            "team_id": "catervas",
            "project_id": "catervas",
            "task_id": task,
            "kind": kind,
            "body": body,
        });
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
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

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_task_another_process_integrated() {
        let harness = under("int-elsewhere", "auto_merge");
        harness.accepted("CTV-1");
        let head = at_root(&harness, &["rev-parse", "main"]);
        appended_elsewhere(
            &harness,
            "CTV-1",
            "task.integrated",
            &json!({ "sha": head, "into": "main", "integrated_by": "human" }),
        );
        assert!(harness.row("CTV-1").awaiting_integration, "not projected");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let answered = orchestrator.integrate(&task("CTV-1")).await;

        assert_eq!(
            answered,
            Ok(IntegrationOutcome::Merged {
                sha: head.clone(),
                scan: ScanRefresh::Unchanged
            })
        );
        assert_eq!(integrations(&harness).len(), 1);
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), head);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_push_that_would_have_to_be_forced() {
        let harness = under("int-push-behind", "auto_merge");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        // `origin`'s `main` gains a commit the local `main` lacks.
        let theirs = harness.merge_on_the_forge(&origin, "CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(integrations(&harness).len(), 1);
        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.starts_with("merged locally as"),
            "{}",
            raised[0].detail
        );
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), theirs);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_merge_git_refuses() {
        let harness = under("int-merge-refused", "auto_merge");
        harness.accepted("CTV-1");
        // An untracked `done.txt` in the root checkout, which the merge would overwrite.
        std::fs::write(harness.project.repo.path.join("done.txt"), "mine\n").expect("written");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert_eq!(raised[0].reason, EscalationRaisedBodyReason::Integration);
        assert!(
            raised[0].detail.starts_with(&format!(
                "merging {} into main failed",
                harness.branch("CTV-1")
            )),
            "{}",
            raised[0].detail
        );
        assert!(integrations(&harness).is_empty());
        assert!(harness.row("CTV-1").awaiting_integration);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_an_integration_branch_git_would_not_name() {
        let harness = Harness::new("int-bad-branch", |wire| {
            wire["policy"]["integration"] = json!("auto_merge");
            wire["policy"]["integration_branch"] = json!("main:other");
        });
        harness.accepted("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.contains("main:other"),
            "{}",
            raised[0].detail
        );
        assert!(integrations(&harness).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_whatever_escalations_of_another_reason_it_carries() {
        let harness = under("int-other-escalation", "auto_merge");
        harness.accepted("CTV-1");
        harness.project.record(
            "CTV-1",
            "escalation.raised",
            &json!({ "reason": "budget", "detail": "the task's budget is spent" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(integrations(&harness).len(), 1);
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
            for (task, file) in [("CTV-1", "one.txt"), ("CTV-2", "two.txt")] {
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

            let (ctv_1, ctv_2) = (task("CTV-1"), task("CTV-2"));
            let (one, two) = runtime.block_on(async {
                tokio::join!(first.integrate(&ctv_1), second.integrate(&ctv_2))
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
            for branch in [harness.branch("CTV-1"), harness.branch("CTV-2")] {
                git_in(
                    &harness.project.repo.path,
                    &["merge-base", "--is-ancestor", &branch, "main"],
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

    /// CTV-1 accepted under `pull_request` with `origin`, its review note written, and, when
    /// `opened`, its pull request 7 already recorded.
    fn a_pull_request(name: &str, opened: bool) -> (Harness, std::path::PathBuf) {
        let harness = under(name, "pull_request");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "C1 passes; done.txt is there.", "written_by": "dev-b" }),
        );
        if opened {
            harness.project.record(
                "CTV-1",
                "pull_request.opened",
                &json!({ "url": PULL_URL, "number": 7, "branch": harness.branch("CTV-1") }),
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
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "C1 still passes, on a second look.", "written_by": "dev-b" }),
        );
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            git_output_in(&origin, &["rev-parse", &harness.branch("CTV-1")]),
            at_root(&harness, &["rev-parse", &harness.branch("CTV-1")])
        );
        let recorded = opened(&harness);
        assert_eq!(recorded.len(), 1, "{recorded:?}");
        assert_eq!(recorded[0].number, 7);
        assert_eq!(recorded[0].branch, harness.branch("CTV-1"));
        assert_eq!(recorded[0].url, PULL_URL);
        let body = harness.gh.stdin_of("create");
        let contract = harness
            .project
            .deps
            .files
            .read_contract(&task("CTV-1"))
            .expect("the contract reads");
        let places: Vec<usize> = [
            "## Intent",
            contract.intent.as_str(),
            "## Completion note",
            "Added done.txt; nothing left out.",
            "## Review note",
            "C1 still passes, on a second look.",
        ]
        .iter()
        .map(|text| {
            body.find(text)
                .unwrap_or_else(|| panic!("{text:?} in {body}"))
        })
        .collect();
        assert!(places.is_sorted(), "{body}");
        assert!(!body.contains("C1 passes; done.txt is there."), "{body}");

        harness.gh.answers("view", OPEN, "", 0);
        assert_eq!(
            orchestrator.integrate(&task("CTV-1")).await,
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
        let sha = harness.merge_on_the_forge(&origin, "CTV-1");
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
        // The tick does not ask whether the branch is in `main` all the same: that is the human's
        // `integrate`.
        assert!(
            !raised[0].detail.contains("reopen it"),
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

        let again = orchestrator.integrate(&task("CTV-1")).await;
        assert!(
            matches!(&again, Ok(IntegrationOutcome::Escalated { detail, scan: None }) if detail.contains("reopen it")),
            "{again:?}"
        );
        assert_eq!(escalations(&harness).len(), 2);

        let sha = harness.merge_on_the_forge(&origin, "CTV-1");
        assert_eq!(
            orchestrator.integrate(&task("CTV-1")).await,
            Ok(IntegrationOutcome::Merged {
                sha: sha.clone(),
                scan: ScanRefresh::Refreshed
            })
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
                program: "/nonexistent/catervas/gh".into(),
                root: harness.project.repo.path.clone(),
            },
        );

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.contains("/nonexistent/catervas/gh"),
            "{}",
            raised[0].detail
        );
        assert!(opened(&harness).is_empty());
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_pull_request_with_no_origin() {
        let harness = under("int-pr-no-origin", "pull_request");
        harness.accepted("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0].detail.contains("there is no remote named origin"),
            "{}",
            raised[0].detail
        );
        assert!(harness.gh.calls().is_empty());
        assert!(opened(&harness).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_forge_merge_the_local_branch_cannot_follow() {
        let (harness, origin) = a_pull_request("int-pr-diverged", true);
        let sha = harness.merge_on_the_forge(&origin, "CTV-1");
        harness.gh.answers(
            "view",
            &format!(r#"{{"state":"MERGED","mergeCommit":{{"oid":"{sha}"}}}}"#),
            "",
            0,
        );
        // The local `main` gains a commit `origin`'s lacks, so it cannot be fast-forwarded.
        harness.commit_at_root("local.txt", "local\n", "A local commit");
        let local = at_root(&harness, &["rev-parse", "main"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].sha, sha);
        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert!(
            raised[0]
                .detail
                .contains("the local main could not be brought up to it"),
            "{}",
            raised[0].detail
        );
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), local);
        let _ = std::fs::remove_dir_all(&origin);
    }

    /// `task` accepted, its worktree gone, with each of `files` written and committed on its
    /// branch.
    fn accepted_adding(harness: &Harness, task: &str, files: &[(&str, &str)]) {
        harness.verifying_with(task, false, true, |_| {});
        let worktree = harness.worktree(task);
        for (path, text) in files {
            std::fs::write(worktree.join(path), text).expect("written");
        }
        harness
            .project
            .deps
            .git
            .commit(
                &worktree,
                "The task's change",
                &files
                    .iter()
                    .map(|(path, _)| (*path).to_string())
                    .collect::<Vec<_>>(),
            )
            .expect("committed");
        harness.project.moved(
            task,
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness
            .project
            .deps
            .git
            .remove_worktree(&worktree)
            .expect("the worktree is removed");
    }

    /// What the tick said it did, the first time it acted.
    async fn acted(orchestrator: &crate::orchestrator::Orchestrator) -> String {
        match orchestrator.tick().await.expect("the tick runs") {
            TickReport::Acted { what, .. } => what,
            other => panic!("the tick acted, got {other:?}"),
        }
    }

    const PACKAGE: &[(&str, &str)] = &[
        ("package.json", r#"{ "scripts": { "test": "vitest" } }"#),
        ("package-lock.json", "{}"),
    ];

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn rescans_after_an_integration_that_changed_the_project() {
        let harness = under("int-rescan", "auto_merge");
        accepted_adding(&harness, "CTV-1", PACKAGE);
        let files = &harness.project.deps.files;
        files
            .append_project_note(
                "It is a shop, not a game.",
                chrono::NaiveDate::from_ymd_opt(2026, 9, 22).expect("a date"),
            )
            .expect("the note is kept");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let what = acted(&orchestrator).await;

        assert!(what.ends_with("; the project scan was refreshed"), "{what}");
        let document = files.read_project_scan().expect("project.md");
        assert!(document.contains("npm"), "{document}");
        assert!(
            document.contains("2026-09-22: It is a shop, not a game."),
            "the user's words outlive the rescan: {document}"
        );
        assert!(document.contains("the-tests-pass"), "{document}");
        let kinds: Vec<EventKind> = harness
            .events(&[
                EventKind::TaskIntegrated,
                EventKind::ProjectScanned,
                EventKind::CriteriaUpdated,
            ])
            .iter()
            .map(|event| event.body.kind())
            .collect();
        assert_eq!(
            kinds,
            [
                EventKind::TaskIntegrated,
                EventKind::CriteriaUpdated,
                EventKind::ProjectScanned
            ],
            "the scan is recorded last, once the refresh finished"
        );
        let scanned = harness.events(&[EventKind::ProjectScanned]);
        assert_eq!(
            scanned[0]
                .envelope
                .ids
                .task_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("CTV-1")
        );
        match &harness.events(&[EventKind::CriteriaUpdated])[0].body {
            EventBody::CriteriaUpdated(body) => {
                assert_eq!(body.updated_by, "governor");
                assert!(
                    body.criterion_names.contains(&"the-tests-pass".to_string()),
                    "{:?}",
                    body.criterion_names
                );
            }
            other => panic!("a criteria.updated, got {other:?}"),
        }
        let library = files.read_criteria().expect("the library");
        assert!(
            library
                .criteria
                .iter()
                .any(|one| one.name.as_str() == "the-tests-pass")
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn writes_nothing_when_the_scan_is_the_same() {
        let harness = under("int-rescan-same", "auto_merge");
        accepted_adding(&harness, "CTV-1", PACKAGE);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        acted(&orchestrator).await;
        accepted_adding(&harness, "CTV-2", &[("README.md", "# Notes\n")]);
        let scanned = harness.events(&[EventKind::ProjectScanned]).len();

        let what = acted(&orchestrator).await;

        assert!(what.starts_with("integrated at"), "{what}");
        assert!(!what.contains("project scan"), "{what}");
        assert_eq!(harness.events(&[EventKind::ProjectScanned]).len(), scanned);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn retries_a_refresh_whose_library_was_not_written() {
        use std::os::unix::fs::PermissionsExt;

        let harness = under("int-rescan-retry", "auto_merge");
        accepted_adding(&harness, "CTV-1", PACKAGE);
        let team = harness.project.repo.path.join(".catervas/team");
        let mode = |bits| {
            std::fs::set_permissions(&team, std::fs::Permissions::from_mode(bits))
                .expect("the team directory's mode");
        };
        // `project.md` is written, and then the library cannot be.
        mode(0o555);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let what = acted(&orchestrator).await;
        mode(0o755);
        assert!(
            what.contains("; the project scan was not refreshed: "),
            "{what}"
        );

        // The next integration changes nothing the scan reads, and finishes the refresh.
        accepted_adding(&harness, "CTV-2", &[("README.md", "# Notes\n")]);
        let what = acted(&orchestrator).await;

        assert!(what.ends_with("; the project scan was refreshed"), "{what}");
        assert!(
            harness
                .project
                .deps
                .files
                .read_criteria()
                .expect("the library")
                .criteria
                .iter()
                .any(|one| one.name.as_str() == "the-tests-pass")
        );
        assert_eq!(harness.events(&[EventKind::ProjectScanned]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn skips_the_scan_off_the_integration_branch() {
        let harness = Harness::new("int-rescan-off", |wire| {
            wire["policy"]["integration"] = json!("manual");
            // Named, since with none the integration branch is whatever is checked out.
            wire["policy"]["integration_branch"] = json!("main");
        });
        accepted_adding(&harness, "CTV-1", PACKAGE);
        git_in(&harness.project.repo.path, &["checkout", "-b", "elsewhere"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let reply = orchestrator
            .handle(catervas_protocol::command::Command::TaskIntegrate {
                task_id: task("CTV-1"),
            })
            .await
            .expect("integrated");

        assert!(
            reply.said.ends_with(
                "; the project scan was not refreshed: the checkout is on elsewhere, not main"
            ),
            "{}",
            reply.said
        );
        assert_eq!(integrations(&harness).len(), 1);
        assert!(harness.events(&[EventKind::ProjectScanned]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_the_integration_when_the_scan_fails() {
        let harness = under("int-rescan-fails", "auto_merge");
        accepted_adding(&harness, "CTV-1", PACKAGE);
        let document = harness.project.repo.path.join(".catervas/project.md");
        let _ = std::fs::remove_file(&document);
        std::fs::create_dir_all(&document).expect("project.md is a directory");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let what = acted(&orchestrator).await;

        assert_eq!(integrations(&harness).len(), 1);
        assert!(!harness.row("CTV-1").awaiting_integration);
        assert!(
            what.contains("; the project scan was not refreshed: ") && what.contains("project.md"),
            "{what}"
        );
        assert!(harness.events(&[EventKind::ProjectScanned]).is_empty());
        assert!(harness.events(&[EventKind::CriteriaUpdated]).is_empty());
    }

    const CADENCE: &str = "docs/catervas/delivery/cadence.md";

    fn folder_events(harness: &Harness, kind: EventKind) -> Vec<serde_json::Value> {
        harness
            .events(&[kind])
            .iter()
            .map(|event| {
                assert!(
                    event.envelope.ids.task_id.is_none(),
                    "{kind}: about no task"
                );
                catervas_protocol::event::event_to_value(event)
            })
            .collect()
    }

    fn folder_report(report: TickReport) -> (u64, String) {
        match report {
            TickReport::FolderChange { change, what } => (change, what),
            other => panic!("a folder change's report, not {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_a_folder_change_on_the_next_tick() {
        let harness = under("int-folder-merge", "auto_merge");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        assert_eq!(harness.folder_change(CADENCE, "a\n"), 1);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        // The accepted task goes first, the folder change on the tick after it.
        let first = orchestrator.tick().await.expect("the tick runs");
        assert!(
            matches!(&first, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{first:?}"
        );
        assert!(folder_events(&harness, EventKind::FolderChangeIntegrated).is_empty());
        let (change, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));

        let head = at_root(&harness, &["rev-parse", "refs/heads/main"]);
        assert_eq!((change, what), (1, format!("integrated at {head}")));
        assert_eq!(
            at_root(&harness, &["log", "-1", "--format=%s", "main"]),
            "Merge docs/folder-1: delivery/cadence.md"
        );
        assert_eq!(git_output_in(&origin, &["rev-parse", "main"]), head);
        let integrated = folder_events(&harness, EventKind::FolderChangeIntegrated);
        assert_eq!(integrated.len(), 1);
        assert_eq!(
            integrated[0]["body"],
            json!({ "change": 1, "sha": head, "into": "main", "integrated_by": "governor" })
        );
        assert_eq!(
            at_root(
                &harness,
                &["show", "main:docs/catervas/delivery/cadence.md"]
            ),
            "a"
        );
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn opens_a_pull_request_for_a_folder_change() {
        let harness = under("int-folder-pr", "pull_request");
        let origin = harness.with_origin();
        harness.folder_change(CADENCE, "a\n");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let (change, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));

        assert_eq!(
            (change, what),
            (1, format!("opened the pull request {PULL_URL}"))
        );
        assert_eq!(
            git_output_in(&origin, &["rev-parse", "docs/folder-1"]),
            at_root(&harness, &["rev-parse", "docs/folder-1"])
        );
        let create = harness
            .gh
            .calls()
            .into_iter()
            .find(|call| call.get(1).map(String::as_str) == Some("create"))
            .expect("a pull request is created");
        assert!(
            create.contains(&"Folder change 1: delivery/cadence.md".to_string()),
            "{create:?}"
        );
        assert_eq!(harness.gh.stdin_of("create"), format!("{CADENCE}\n"));
        let opened = folder_events(&harness, EventKind::FolderChangeOpened);
        assert_eq!(
            opened[0]["body"],
            json!({ "change": 1, "url": PULL_URL, "number": 7 })
        );

        // Once the forge says merged, the next tick records it as the human's and brings main up.
        let sha = harness.merge_branch_on_the_forge(&origin, "docs/folder-1");
        harness.gh.answers(
            "view",
            &format!(r#"{{"state":"MERGED","mergeCommit":{{"oid":"{sha}"}}}}"#),
            "",
            0,
        );
        let (_, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));
        assert_eq!(what, format!("integrated at {sha}"));
        let integrated = folder_events(&harness, EventKind::FolderChangeIntegrated);
        assert_eq!(integrated[0]["body"]["integrated_by"], "human");
        assert_eq!(integrated[0]["body"]["sha"], sha);
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_folder_change_whose_pull_request_was_closed() {
        let harness = under("int-folder-pr-closed", "pull_request");
        let origin = harness.with_origin();
        harness.folder_change(CADENCE, "a\n");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the pull request opens");
        harness
            .gh
            .answers("view", r#"{"mergeCommit":null,"state":"CLOSED"}"#, "", 0);

        let (_, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));

        assert!(
            what.starts_with("escalated its integration to the human: ")
                && what.contains("was closed without merging"),
            "{what}"
        );
        let escalated = folder_events(&harness, EventKind::FolderChangeEscalated);
        assert_eq!(escalated.len(), 1);
        let detail = escalated[0]["body"]["detail"].as_str().expect("a detail");
        assert!(detail.contains("was closed without merging"), "{detail}");
        let said = harness.events(&[EventKind::MessagePosted]);
        assert!(
            said.iter()
                .any(|event| matches!(&event.body, EventBody::MessagePosted(body)
                if body.text.contains(detail))),
            "the detail is said in the channel"
        );
        let calls = harness.gh.calls().len();
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert_eq!(harness.gh.calls().len(), calls, "not tried again");
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_folder_change_for_the_human_under_manual() {
        use catervas_protocol::command::Command;

        let harness = under("int-folder-manual", "manual");
        let origin = harness.with_origin();
        harness.folder_change(CADENCE, "a\n");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let main_before = at_root(&harness, &["rev-parse", "main"]);

        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), main_before);

        let report = orchestrator
            .handle(Command::FolderChangeIntegrate { change: 1 })
            .await
            .expect("the human integrates it");
        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_ne!(head, main_before);
        assert!(report.said.contains(&head), "{}", report.said);
        let integrated = folder_events(&harness, EventKind::FolderChangeIntegrated);
        assert_eq!(
            integrated[0]["body"],
            json!({ "change": 1, "sha": head, "into": "main", "integrated_by": "human" })
        );
        assert_eq!(
            git_output_in(&origin, &["rev-parse", "main"]),
            main_before,
            "manual pushes nothing"
        );
        assert_eq!(
            orchestrator.integrate_folder_change(1).await,
            Ok(IntegrationOutcome::Merged {
                sha: head,
                scan: ScanRefresh::Unchanged
            })
        );
        assert_eq!(
            folder_events(&harness, EventKind::FolderChangeIntegrated).len(),
            1
        );
        let refusal = orchestrator.integrate_folder_change(9).await;
        assert!(
            matches!(&refusal, Err(OrchestratorError::Refused { reason })
                if reason.starts_with("no_such_folder_change: ")),
            "{refusal:?}"
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_folder_change_that_cannot_land() {
        use catervas_protocol::command::Command;

        let harness = under("int-folder-escalates", "auto_merge");
        harness.folder_change(CADENCE, "a\n");
        // The owner's own file at the path, made after the write: the merge would overwrite it.
        harness.project.repo.write(CADENCE, "the owner's\n");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let (change, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));

        assert_eq!(change, 1);
        assert!(
            what.starts_with("escalated its integration to the human: "),
            "{what}"
        );
        let escalated = folder_events(&harness, EventKind::FolderChangeEscalated);
        assert_eq!(escalated.len(), 1);
        let detail = escalated[0]["body"]["detail"].as_str().expect("a detail");
        assert!(
            detail.contains("merging docs/folder-1 into main failed"),
            "{detail}"
        );
        // The channel keeps a message on one line.
        let one_line = detail.replace('\n', " ");
        assert!(
            harness
                .events(&[EventKind::MessagePosted])
                .iter()
                .any(|event| matches!(&event.body, EventBody::MessagePosted(body)
                    if body.text.contains(&one_line)))
        );
        assert!(folder_events(&harness, EventKind::FolderChangeIntegrated).is_empty());
        // Not tried again by a tick.
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert_eq!(
            folder_events(&harness, EventKind::FolderChangeEscalated).len(),
            1
        );

        // The human's word tries again, once the owner's file is out of the way.
        std::fs::remove_file(harness.project.repo.path.join(CADENCE)).expect("removed");
        // An escalated change no longer holds its path: the next write of it is change 2.
        assert_eq!(harness.folder_change(CADENCE, "b\n"), 2);
        orchestrator
            .handle(Command::FolderChangeIntegrate { change: 1 })
            .await
            .expect("the human integrates it");
        let integrated = folder_events(&harness, EventKind::FolderChangeIntegrated);
        assert_eq!(integrated[0]["body"]["integrated_by"], "human");
    }
}
