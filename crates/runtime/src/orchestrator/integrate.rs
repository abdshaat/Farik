//! What happens to a task's work once it is finished (`docs/SPEC.md` 5.14): an accepted task's
//! branch integrated the way the team's policy says, one task at a time even across processes,
//! and a finished task's worktrees and containers removed, its branch kept.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use catervas_core::branch::{change_type, task_branch};
use catervas_core::contract::{TaskId, TaskKind, TaskStatus};
use catervas_core::folders::is_plan_task;
use catervas_core::forge_text::{
    folder_change_title, pull_request_title, storage_change_body, storage_change_title,
    without_task_ids,
};
use catervas_core::governor::transition::TransitionRequest;
use catervas_core::governor::transition_table::TransitionActor;
use catervas_core::team::{Team, task_private_folder};
use catervas_protocol::event::{
    CatervasEvent, EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventKind,
    NoteWrittenBodyKind, PullRequestConflictedBody, PullRequestOpenedBody,
    PullRequestRefreshedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
};
use catervas_protocol::generated::event::{CriteriaUpdatedBody, ProjectScannedBody};
use catervas_store::files::FilesError;
use catervas_store::folder_docs::{FolderChange, folder_docs};
use catervas_store::merging::merging_allowed;
use catervas_store::repos::{TaskRepositoryError, task_repository};
use catervas_store::waiting::name_of;
use catervas_store::{
    EventQuery, Git, GitError, MergeOutcome, TaskProjection, material, names_of, project_document,
    rescan_members, scan_project, seeded_library,
};

use super::verify::{GOVERNOR, append, append_stamped};
use super::{
    IntegrationOutcome, Orchestrator, OrchestratorError, ScanRefresh, TickReport,
    task_repository_of, worktree,
};
use crate::channel::post_system;
use crate::forge::{Checks, Forge, MergeState, Mergeable, PullRequestState};
use crate::tools::ToolDeps;
use crate::tools::folders::plain_names;
use crate::transitions::{
    INTEGRATION_LOCK, TransitionAsk, TransitionOutcome, integration_lock, is_move_into,
    refusal_details,
};

/// Who asked for an integration: the tick, on the team's policy, or the human. Only the tick
/// merges, and only as ADR 0073 allows; the human's `catervas integrate` never does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asker {
    Tick,
    Human,
}

/// Rule 2: an accepted task awaiting integration gets its pull request, and is recorded once it
/// is merged, unless an integration escalation was raised since it was accepted: that one is the
/// human's to settle through `integrate`, and is not tried again on every tick. Catervas never
/// merges unless the owner allows it (ADR 0073).
pub(super) async fn awaiting(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if row.kind != TaskKind::Task || !row.awaiting_integration {
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
        IntegrationOutcome::SentBack { paths } => format!(
            "sent it back to its assignee: its pull request no longer merges ({})",
            paths.join(", ")
        ),
        IntegrationOutcome::AwaitingForge | IntegrationOutcome::AwaitingHand { .. } => {
            return Ok(None);
        }
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
            Self::FolderChange(change) => folder_change(tools, *change)?.branch,
        })
    }

    /// Its pull request's title: a task's is its type and title, with no task id (ADR 0066).
    fn title(&self, tools: &ToolDeps) -> Result<String, OrchestratorError> {
        match self {
            Self::Task(task_id) => {
                let contract = tools.files.read_contract(task_id)?;
                Ok(pull_request_title(
                    change_type(&contract),
                    contract.title.as_str(),
                    &title_of(tools),
                ))
            }
            Self::FolderChange(change) => {
                let held = folder_change(tools, *change)?;
                if let Some(local) = held.storage {
                    return Ok(storage_change_title(local).to_string());
                }
                let paths: Vec<&str> = held.paths.iter().map(String::as_str).collect();
                Ok(folder_change_title(&paths, &plain_names(&paths)))
            }
        }
    }
}

/// A task's title as the contract's file holds it, for the ids a pull request's words name.
fn title_of(tools: &ToolDeps) -> impl Fn(&TaskId) -> Option<String> + '_ {
    |id| {
        tools
            .files
            .read_contract(id)
            .ok()
            .map(|contract| contract.title.as_str().to_string())
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

/// Rule 2's second half: each folder change not integrated gets its pull request, as `awaiting`
/// does for each accepted task, in a tick scoped to no task. One that was escalated is the
/// human's to settle through `integrate_folder_change`.
pub(super) async fn awaiting_folder_changes(
    orchestrator: &Orchestrator,
    team: &Team,
) -> Result<Option<TickReport>, OrchestratorError> {
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
            IntegrationOutcome::AwaitingForge
            | IntegrationOutcome::SentBack { .. }
            | IntegrationOutcome::AwaitingHand { .. } => {
                continue;
            }
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

/// Where an integration happens: the repository the subject's branch is in, with its adapter, its
/// forge, and the branch accepted work goes into (5.14).
struct IntegrationTarget {
    /// The member's name, or `None` for the project's own folder.
    repo: Option<String>,
    git: Git,
    forge: Forge,
    into: String,
}

/// Why a subject's target cannot be had: its sentence, and whether it is a member that is not
/// where `repos.yaml` says, which waits on a tick.
enum Unresolved {
    Missing(String),
    Other(String),
}

/// The repository, forge and integration branch the subject integrates in. A task names its
/// member in its contract; a folder change, and a task with no member, integrate in the project
/// folder's own repository.
fn target(
    tools: &ToolDeps,
    forge: &Forge,
    team: &Team,
    subject: &Subject,
) -> Result<IntegrationTarget, Unresolved> {
    let repo = match subject {
        Subject::Task(task_id) => {
            let contract = tools
                .files
                .read_contract(task_id)
                .map_err(|error| Unresolved::Other(error.to_string()))?;
            contract.repo.as_ref().map(|repo| repo.as_str().to_string())
        }
        Subject::FolderChange(_) => None,
    };
    match task_repository(&tools.files, team, &tools.git, repo.as_deref()) {
        Ok(found) => {
            let forge = match found.repo {
                Some(_) => Forge {
                    program: forge.program.clone(),
                    root: found.git.root().to_path_buf(),
                },
                None => forge.clone(),
            };
            Ok(IntegrationTarget {
                repo: found.repo,
                git: found.git,
                forge,
                into: found.integration_branch,
            })
        }
        Err(error @ TaskRepositoryError::Missing { .. }) => {
            Err(Unresolved::Missing(error.to_string()))
        }
        Err(TaskRepositoryError::Git(error)) => Err(Unresolved::Other(git_words(&error))),
        Err(error) => Err(Unresolved::Other(error.to_string())),
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
    let target = match target(tools, forge, team, subject) {
        Ok(target) => target,
        // A member that is not there waits, as its tasks do: the human is told why on asking.
        Err(Unresolved::Missing(_)) if asker == Asker::Tick => return Ok(None),
        Err(Unresolved::Missing(words) | Unresolved::Other(words)) => {
            return escalate(tools, subject, words).map(Some);
        }
    };
    // A task that writes an epic's plan whose plan is already on the integration branch is
    // recorded integrated; otherwise it goes through the forge like any task (ADR 0079).
    if let Subject::Task(task_id) = subject {
        let contract = tools.files.read_contract(task_id)?;
        if is_plan_task(&contract)
            && let Some(plan) = &contract.plan
            && let Some(outcome) = carried_plan(tools, subject, &plan.to_string(), &target, asker)?
        {
            return Ok(Some(outcome));
        }
    }
    let integrated_before = integrations_of(tools, subject)?;
    let outcome = through_the_forge(tools, team, subject, &target, asker)?;
    // A folder change touches no language, toolchain, test or criterion: no scan to refresh.
    let Subject::Task(task_id) = subject else {
        return Ok(outcome);
    };
    if integrations_of(tools, subject)? == integrated_before {
        return Ok(outcome);
    }
    let scan = refresh_the_scan(tools, task_id, &target);
    Ok(outcome.map(|outcome| match outcome {
        IntegrationOutcome::Merged { sha, .. } => IntegrationOutcome::Merged { sha, scan },
        IntegrationOutcome::Escalated { detail, .. } => IntegrationOutcome::Escalated {
            detail,
            scan: Some(scan),
        },
        other => other,
    }))
}

/// A plan task whose plan file is already on the integration branch is integrated by the file, so
/// that a squash merge counts. `None` while it is not: the task then opens its own pull request.
fn carried_plan(
    tools: &ToolDeps,
    subject: &Subject,
    plan: &str,
    target: &IntegrationTarget,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let into = target.into.as_str();
    let carried = match target.git.has_file(into, plan) {
        Ok(carried) => carried,
        Err(GitError::CommandFailed { .. }) => false,
        Err(error) => return Err(error.into()),
    };
    if !carried {
        return Ok(None);
    }
    let head = target.git.merge_base(into, into)?;
    let integrated_by = match asker {
        Asker::Tick => TaskIntegratedBodyIntegratedBy::Governor,
        Asker::Human => TaskIntegratedBodyIntegratedBy::Human,
    };
    record_integrated(tools, subject, &head, into, integrated_by)?;
    Ok(Some(IntegrationOutcome::Merged {
        sha: head,
        scan: ScanRefresh::Unchanged,
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
fn refresh_the_scan(tools: &ToolDeps, task_id: &TaskId, target: &IntegrationTarget) -> ScanRefresh {
    let into = target.into.as_str();
    // A project folder's own scan never runs: its criteria are its members', and a scan of the
    // folder would wipe them. What landed in a member is read from that member, and the project
    // folder's own work reports nothing, so no integration says the scan was skipped.
    if tools.files.is_project_folder() {
        return match target.repo.as_deref() {
            Some(repo) => refresh_the_member(tools, repo, target),
            None => ScanRefresh::Unchanged,
        };
    }
    // `scan_project` reads the working tree at the root, which holds what landed only when it is
    // the integration branch that is checked out there.
    match target.git.current_branch() {
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
    rescan(tools, task_id, &target.git).unwrap_or_else(|error| ScanRefresh::Failed { error })
}

/// The scan of the member `repo` a task landed in, when its checkout is on the integration branch,
/// which is where its working tree holds what landed.
fn refresh_the_member(tools: &ToolDeps, repo: &str, target: &IntegrationTarget) -> ScanRefresh {
    match target.git.current_branch() {
        Ok(branch) if branch == target.into => {}
        Ok(branch) => {
            return ScanRefresh::Skipped {
                why: format!("{repo}'s checkout is on {branch}, not {}", target.into),
            };
        }
        Err(_) => {
            return ScanRefresh::Skipped {
                why: format!("{repo}'s checkout is not on a branch"),
            };
        }
    }
    rescan_member(tools, repo).unwrap_or_else(|error| ScanRefresh::Failed { error })
}

/// The member's checks read again and the library reseeded from them, the other members' kept.
fn rescan_member(tools: &ToolDeps, repo: &str) -> Result<ScanRefresh, String> {
    let scanned = rescan_members(
        &tools.files,
        &tools.log,
        &tools.ids,
        tools.clock.now(),
        Some(repo),
        GOVERNOR,
    )
    .map_err(|error| error.to_string())?;
    tools
        .projections
        .catch_up()
        .map_err(|error| error.to_string())?;
    Ok(if scanned.library_changed {
        ScanRefresh::Refreshed
    } else {
        ScanRefresh::Unchanged
    })
}

/// The refresh itself, once the checkout is known to hold what landed.
fn rescan(tools: &ToolDeps, task_id: &TaskId, git: &Git) -> Result<ScanRefresh, String> {
    let scan = scan_project(git, tools.clock.now()).map_err(|error| error.to_string())?;
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
        .write_project_scan(&project_document(
            &scan.read_back,
            &library,
            &[],
            previous.as_deref(),
        ))
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

/// With no pull request opened since acceptance, the branch pushed to
/// `origin` and one opened; with one, its state read. A merge on the forge is recorded as the
/// human's and the local integration branch brought up to it; a closed one is escalated, unless
/// the human asks and the branch is in the integration branch all the same.
fn through_the_forge(
    tools: &ToolDeps,
    team: &Team,
    subject: &Subject,
    target: &IntegrationTarget,
    asker: Asker,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let branch = subject.branch(tools)?;
    let Some(url) = opened_url(tools, subject)? else {
        return open(tools, subject, target, &branch, asker).map(Some);
    };
    let state = match target.forge.pull_request_state(&url) {
        Ok(state) => state,
        Err(error) => {
            let detail = format!("reading the pull request {url} failed: {error}");
            return escalate(tools, subject, detail).map(Some);
        }
    };
    match state {
        PullRequestState::Open {
            mergeable: Mergeable::Conflicting,
            ..
        } => stopped_merging(tools, team, subject, target, &branch, &url).map(Some),
        PullRequestState::Open {
            merge_state,
            head,
            checks,
            ..
        } => {
            // Only Catervas's own tick merges, and only as ADR 0073 allows.
            if asker == Asker::Tick && merging_allowed(&tools.log, team)? {
                let open = OpenPullRequest {
                    merge_state,
                    head,
                    checks,
                };
                return merge_when_checks_pass(tools, team, subject, target, &branch, &url, &open)
                    .map(Some);
            }
            Ok(Some(IntegrationOutcome::AwaitingForge))
        }
        PullRequestState::Merged { sha } => merged_on_the_forge(tools, subject, target, &url, sha),
        PullRequestState::Closed if asker == Asker::Tick => {
            let detail = format!("the pull request {url} was closed without merging");
            let escalated = escalate(tools, subject, detail)?;
            revert_the_switch(tools, subject)?;
            Ok(Some(escalated))
        }
        PullRequestState::Closed => {
            closed_for_the_human(tools, subject, &url, target, &branch).map(Some)
        }
    }
}

/// An open pull request as the forge reported it, copied out of `PullRequestState::Open`; its
/// `mergeable` was read before (a conflict is `stopped_merging`'s).
struct OpenPullRequest {
    merge_state: MergeState,
    head: String,
    checks: Checks,
}

/// A pull request merged on the forge, by the human or, since ADR 0073, by Catervas itself:
/// recorded as integrated, `governor` when the log holds Catervas's own `pull_request.merged` for
/// its address and `human` otherwise, and the local integration branch brought up to it.
fn merged_on_the_forge(
    tools: &ToolDeps,
    subject: &Subject,
    target: &IntegrationTarget,
    url: &str,
    sha: String,
) -> Result<Option<IntegrationOutcome>, OrchestratorError> {
    let into = target.into.as_str();
    let merged_by_catervas = tools
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::PullRequestMerged],
            ..EventQuery::default()
        })?
        .iter()
        .any(|event| {
            matches!(&event.body, EventBody::PullRequestMerged(body) if body.url.to_string() == url)
        });
    let integrated_by = if merged_by_catervas {
        TaskIntegratedBodyIntegratedBy::Governor
    } else {
        TaskIntegratedBodyIntegratedBy::Human
    };
    // A switch of where the documents are kept (ADR 0074) finishes against the checkout the
    // merge reached: the local branch is brought up to it before the integration is recorded, so
    // that git cannot remove the documents after Catervas puts them back.
    let switches_storage = matches!(subject, Subject::FolderChange(change)
        if folder_change(tools, *change)?.storage.is_some());
    if !switches_storage {
        record_integrated(tools, subject, &sha, into, integrated_by)?;
    }
    if let Err(error) = target.git.fetch_fast_forward("origin", into) {
        let detail = format!(
            "the pull request {url} was merged as {sha}, but the local {into} could not be \
             brought up to it: {}",
            git_words(&error)
        );
        return escalate(tools, subject, detail).map(Some);
    }
    if switches_storage {
        record_integrated(tools, subject, &sha, into, integrated_by)?;
    }
    Ok(Some(IntegrationOutcome::Merged {
        sha,
        scan: ScanRefresh::Unchanged,
    }))
}

/// Catervas's own merge of an open pull request (ADR 0073), the cases of the design in order: a
/// conflict is brought up to date or escalated as before; a failed check is escalated; pending
/// checks wait; a branch behind its base is refreshed; a rule GitHub enforces is escalated; a
/// clean one is merged with the first method the repository allows, pinned to the commit whose
/// checks were read. A repository with no checks at all merges like one whose checks passed: the
/// task is accepted, so Catervas's own verification passed.
fn merge_when_checks_pass(
    tools: &ToolDeps,
    team: &Team,
    subject: &Subject,
    target: &IntegrationTarget,
    branch: &str,
    url: &str,
    open: &OpenPullRequest,
) -> Result<IntegrationOutcome, OrchestratorError> {
    if open.merge_state == MergeState::Dirty {
        return stopped_merging(tools, team, subject, target, branch, url);
    }
    match &open.checks {
        Checks::Failed { names } => {
            let detail = format!(
                "A check failed on GitHub: {}. Catervas did not merge {url}; fix it or merge it \
                 yourself.",
                names.join(", ")
            );
            return escalate(tools, subject, detail);
        }
        Checks::Pending => return Ok(IntegrationOutcome::AwaitingForge),
        Checks::None | Checks::Passed => {}
    }
    match open.merge_state {
        MergeState::Behind => {
            return stopped_merging(tools, team, subject, target, branch, url);
        }
        MergeState::Blocked => {
            let detail = format!(
                "GitHub will not merge {url} yet: it needs a review or another of the branch's \
                 rules. Merge it yourself once it can."
            );
            return escalate(tools, subject, detail);
        }
        MergeState::Clean | MergeState::HasHooks | MergeState::Unstable => {}
        MergeState::Dirty | MergeState::Draft | MergeState::Unknown => {
            return Ok(IntegrationOutcome::AwaitingForge);
        }
    }
    let methods = match target.forge.merge_methods() {
        Ok(methods) => methods,
        Err(error) => {
            let detail = format!("reading how {url} may be merged failed: {error}");
            return escalate(tools, subject, detail);
        }
    };
    let Some(method) = methods.first().copied() else {
        let detail = format!("{} allows no merge method.", repository_of(url));
        return escalate(tools, subject, detail);
    };
    if let Err(error) = target.forge.merge_pull_request(url, method, &open.head) {
        let detail = format!("GitHub would not merge {url}: {error}");
        return escalate(tools, subject, detail);
    }
    let body = serde_json::from_value(serde_json::json!({ "url": url, "method": method.wire() }))
        .map_err(|error| refused(format!("pull_request_merged_unrecordable: {error}")))?;
    append_stamped(tools, tools.ids.clone(), EventBody::PullRequestMerged(body))?;
    // A merge queue may not have merged it yet: the next tick finds it merged.
    match target.forge.pull_request_state(url) {
        Ok(PullRequestState::Merged { sha }) => {
            match merged_on_the_forge(tools, subject, target, url, sha)? {
                Some(outcome) => Ok(outcome),
                None => Ok(IntegrationOutcome::AwaitingForge),
            }
        }
        _ => Ok(IntegrationOutcome::AwaitingForge),
    }
}

/// `owner/repo` of the pull request at `url`, else the address itself.
fn repository_of(url: &str) -> String {
    let path = url.split_once("://").map_or(url, |(_, rest)| rest);
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [_, owner, repo, "pull", ..] => format!("{owner}/{repo}"),
        _ => url.to_string(),
    }
}

/// An open pull request that no longer merges (ADR 0069). A folder change's is escalated. A
/// task's branch is brought up to date by merging the integration branch into it in its own
/// worktree, made again for the purpose: a clean merge is pushed to the same pull request; a
/// conflicted one is left there with its markers, and the task goes back to its assignee.
fn stopped_merging(
    tools: &ToolDeps,
    team: &Team,
    subject: &Subject,
    target: &IntegrationTarget,
    branch: &str,
    url: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let into = target.into.as_str();
    let Subject::Task(task_id) = subject else {
        let detail = format!("the pull request {url} no longer merges into {into}");
        return escalate(tools, subject, detail);
    };
    if let Err(error) = target.git.fetch_fast_forward("origin", into) {
        let detail = format!(
            "the pull request {url} no longer merges, and the local {into} could not be brought \
             up to the remote's: {}",
            git_words(&error)
        );
        return escalate(tools, subject, detail);
    }
    // The branch already holds all of the integration branch: the forge's answer is stale, as it
    // is for a while after a push, and there is nothing to bring up to date.
    if target.git.commit_count(branch, into)? == 0 {
        return Ok(IntegrationOutcome::AwaitingForge);
    }
    let worktree = tools
        .files
        .root()
        .join(".catervas/local/worktrees")
        .join(task_id.as_str());
    if !worktree.is_dir() {
        target.git.add_worktree(&worktree, branch)?;
    }
    let message = format!("chore: bring up to date with {into}");
    match target
        .git
        .merge_keeping_conflicts(&worktree, into, &message)?
    {
        MergeOutcome::Merged { sha } => {
            let pushed = target.git.push("origin", &format!("refs/heads/{branch}"));
            target.git.remove_worktree(&worktree)?;
            if let Err(error) = pushed {
                let detail = format!(
                    "pushing {branch} after bringing it up to date failed: {}",
                    git_words(&error)
                );
                return escalate(tools, subject, detail);
            }
            append(
                tools,
                task_id,
                None,
                None,
                EventBody::PullRequestRefreshed(PullRequestRefreshedBody {
                    url: url.to_string(),
                    sha,
                }),
            )?;
            Ok(IntegrationOutcome::AwaitingForge)
        }
        MergeOutcome::Conflicts(paths) => {
            append(
                tools,
                task_id,
                None,
                None,
                EventBody::PullRequestConflicted(PullRequestConflictedBody {
                    url: url.to_string(),
                    into: into.to_string(),
                    paths: paths.iter().take(200).cloned().collect(),
                }),
            )?;
            let outcome = tools.transitions.request(
                &TransitionRequest {
                    task_id: task_id.clone(),
                    to: TaskStatus::Assigned,
                    actor: TransitionActor::Governor,
                    agent_id: None,
                },
                &TransitionAsk {
                    pull_request_conflicts: Some(paths.clone()),
                    ..TransitionAsk::default()
                },
                team,
            )?;
            match outcome {
                TransitionOutcome::Moved(_) => {
                    // The task's documents are a copy of the owner's folder (ADR 0074); the
                    // worktree made again for the merge gets one for the agent to work in.
                    if catervas_store::document_source::documents_on_disk(team, &tools.log)? {
                        catervas_store::local_documents::copy_in(
                            &target.git,
                            tools.files.root(),
                            &worktree,
                            None,
                        )?;
                    }
                    Ok(IntegrationOutcome::SentBack { paths })
                }
                TransitionOutcome::Refused(refusal) => {
                    let _ = target.git.remove_worktree(&worktree);
                    let detail = format!(
                        "the pull request {url} no longer merges into {into}, and the task could \
                         not be sent back: {}",
                        refusal_details(&refusal).join("; ")
                    );
                    escalate(tools, subject, detail)
                }
            }
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
    target: &IntegrationTarget,
    branch: &str,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let into = target.into.as_str();
    let merged = target
        .git
        .fetch_fast_forward("origin", into)
        .and_then(|()| target.git.commit_count(into, branch));
    match merged {
        Ok(0) => {
            let head = target.git.merge_base(into, into)?;
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
            let escalated = escalate(tools, subject, detail)?;
            revert_the_switch(tools, subject)?;
            Ok(escalated)
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

/// Pushes the branch to `origin` and opens its pull request, recording it. With no `origin` there
/// is nothing to open: the branch is integrated once it is in the integration branch, merged by
/// hand, and waits for that until then (the human asking is told so).
fn open(
    tools: &ToolDeps,
    subject: &Subject,
    target: &IntegrationTarget,
    branch: &str,
    asker: Asker,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let into = target.into.as_str();
    match target.git.has_remote("origin") {
        Ok(true) => {}
        Ok(false) => return merged_by_hand(tools, subject, target, branch, asker),
        Err(error) => return escalate(tools, subject, git_words(&error)),
    }
    if let Err(error) = target.git.push("origin", &format!("refs/heads/{branch}")) {
        let detail = format!("pushing {branch} to origin failed: {}", git_words(&error));
        return escalate(tools, subject, detail);
    }
    let title = subject.title(tools)?;
    let body = pull_request_body(tools, subject)?;
    let pull_request = match target.forge.open_pull_request(into, branch, &title, &body) {
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

/// A subject with no `origin` to open a pull request on: integrated, as the human's, once its
/// branch is in the integration branch; else the tick waits and the human asking is refused.
fn merged_by_hand(
    tools: &ToolDeps,
    subject: &Subject,
    target: &IntegrationTarget,
    branch: &str,
    asker: Asker,
) -> Result<IntegrationOutcome, OrchestratorError> {
    let into = target.into.as_str();
    match target.git.commit_count(into, branch) {
        Ok(0) => {
            let head = target.git.merge_base(into, into)?;
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
        Ok(_) if asker == Asker::Tick => Ok(IntegrationOutcome::AwaitingHand {
            branch: branch.to_string(),
            into: into.to_string(),
        }),
        Ok(_) => Err(refused(format!(
            "not_merged: {branch} is not in {into} yet; merge it, then ask again"
        ))),
        Err(error) => escalate(tools, subject, git_words(&error)),
    }
}

/// The pull request's body (ADR 0069), short and composed by Catervas: for a task, its summary and
/// its criteria's texts, and the first sentence of the last review an agent wrote; for a folder
/// change, its documents and how they were checked.
fn pull_request_body(tools: &ToolDeps, subject: &Subject) -> Result<String, OrchestratorError> {
    let team = tools.files.read_team().ok();
    let display_name = |id: &str| {
        team.as_ref()
            .and_then(|team| team.agents.iter().find(|agent| agent.id.as_str() == id))
            .map_or_else(|| id.to_string(), |agent| agent.display_name.to_string())
    };
    let task_id = match subject {
        Subject::Task(task_id) => task_id,
        Subject::FolderChange(change) => {
            let held = folder_change(tools, *change)?;
            if let Some(local) = held.storage {
                return Ok(storage_change_body(local));
            }
            let paths: Vec<&str> = held.paths.iter().map(String::as_str).collect();
            let documents: Vec<String> = plain_names(&paths)
                .split(" and ")
                .map(ToString::to_string)
                .collect();
            let how = if held.approved {
                "Approved by the owner on Today.".to_string()
            } else if held.edited {
                "Saved by the owner on the Files page.".to_string()
            } else {
                let author = held
                    .written_by
                    .as_deref()
                    .map_or_else(|| "the team".to_string(), display_name);
                format!(
                    "Written by {author} at the delivery's review; the team's documents are not \
                     reviewed (ADR 0061)."
                )
            };
            return Ok(catervas_core::forge_text::pull_request_body(
                &documents
                    .iter()
                    .map(|name| format!("- {name}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                &[how],
                None,
            ));
        }
    };
    let contract = tools.files.read_contract(task_id)?;
    let plain = |text: &str| without_task_ids(text, &title_of(tools));
    let changed = plain(
        contract
            .summary
            .as_ref()
            .map_or(contract.title.as_str(), |summary| summary.as_str()),
    );
    let checks: Vec<String> = contract
        .exit_criteria
        .iter()
        .map(|criterion| plain(criterion.text.as_str()))
        .collect();
    let review = tools
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::NoteWritten],
            ..EventQuery::default()
        })?
        .into_iter()
        .rev()
        .find_map(|event| match event.body {
            EventBody::NoteWritten(body)
                if body.kind == NoteWrittenBodyKind::Review && body.written_by != "catervas" =>
            {
                Some((display_name(&body.written_by), plain(&body.text)))
            }
            _ => None,
        });
    Ok(catervas_core::forge_text::pull_request_body(
        &changed,
        &checks,
        review
            .as_ref()
            .map(|(name, note)| (name.as_str(), note.as_str())),
    ))
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
            // A change that moves the documents (ADR 0074) finishes the move before it is
            // recorded, so that a restore that failed is tried again, and files no re-derive.
            if let Some(local) = folder_change(tools, *change)?.storage {
                finish_the_switch(tools, *change, local)?;
                append_stamped(
                    tools,
                    tools.ids.clone(),
                    EventBody::FolderChangeIntegrated(body),
                )?;
                return Ok(());
            }
            append_stamped(
                tools,
                tools.ids.clone(),
                EventBody::FolderChangeIntegrated(body),
            )?;
            file_the_rederive(tools, *change)
        }
    }
}

/// What a switch of where the documents are kept leaves to do once its change is in the integration
/// branch (ADR 0074): taken out, git has deleted the files it removed from the checkout, so they
/// are written back from the change's parent commit; put back, the exclude lines go.
fn finish_the_switch(tools: &ToolDeps, change: u64, local: bool) -> Result<(), OrchestratorError> {
    if !local {
        return catervas_store::local_documents::include_documents(&tools.git)
            .map_err(OrchestratorError::from);
    }
    let sha = tools
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::DocumentStorageChanged],
            ..EventQuery::default()
        })?
        .into_iter()
        .rev()
        .find_map(|event| match event.body {
            EventBody::DocumentStorageChanged(body)
                if body
                    .change
                    .as_ref()
                    .is_some_and(|held| held.get() == change) =>
            {
                body.sha.as_ref().map(|sha| sha.as_str().to_string())
            }
            _ => None,
        })
        .ok_or_else(|| {
            refused(format!(
                "folder_change_unrecordable: folder change {change} has no commit to restore from"
            ))
        })?;
    catervas_store::local_documents::restore_taken_out(&tools.git, &sha)?;
    Ok(())
}

/// A switch whose pull request was closed without merging puts the owner's choice back (ADR 0074):
/// the policy as it was, the exclude lines removed when they were added for a take-out, and the
/// channel told. Nothing else that goes wrong with a switch comes here.
fn revert_the_switch(tools: &ToolDeps, subject: &Subject) -> Result<(), OrchestratorError> {
    let Subject::FolderChange(change) = subject else {
        return Ok(());
    };
    let Some(local) = folder_change(tools, *change)?.storage else {
        return Ok(());
    };
    let mut team = tools.files.read_team()?;
    team.policy.documents_local = (!local).then_some(true);
    tools.files.write_team(&team)?;
    append_stamped(
        tools,
        tools.ids.clone(),
        EventBody::TeamUpdated(catervas_protocol::event::TeamUpdatedBody {
            team_name: team.name.to_string(),
            agent_ids: team
                .agents
                .iter()
                .map(|agent| agent.id.to_string())
                .collect(),
            updated_by: "catervas".to_string(),
            template: None,
            plan_in_sprints: None,
        }),
    )?;
    if local {
        catervas_store::local_documents::include_documents(&tools.git)?;
    }
    post_system(
        &tools.log,
        tools.clock.as_ref(),
        &tools.ids,
        None,
        "Your documents stay as they were: the pull request that moved them was closed.",
    )?;
    Ok(())
}

/// Files the task that brings an owner's change to a document up to date in its twin. A filing
/// refused is said in the channel with its reason, and the integration stands.
fn file_the_rederive(tools: &ToolDeps, change: u64) -> Result<(), OrchestratorError> {
    let filed = tools
        .files
        .read_team()
        .map_err(catervas_store::requests::RequestError::from)
        .and_then(|team| crate::tools::folders::file_rederive(tools, &team, change));
    if let Err(reason) = filed {
        post_system(
            &tools.log,
            tools.clock.as_ref(),
            &tools.ids,
            None,
            &format!(
                "Folder change {change} was added to the project, but the task that brings the team's version of it up to date could not be filed: {reason}"
            ),
        )?;
    }
    Ok(())
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
    // The documents a task worked on land in the owner's folder before its worktree goes; a
    // landing that failed keeps the worktree, and the next tick tries again (ADR 0074).
    land_the_documents(orchestrator, row)?;
    if !remove_workspace(orchestrator, &row.task_id)? {
        return Ok(None);
    }
    // The removal already happened, so a contract that cannot be read only loses the name.
    let deps = &orchestrator.deps.tools;
    let kept = deps.files.read_contract(&row.task_id).map_or_else(
        |_| "its branch".to_string(),
        |contract| {
            let branchless = deps
                .files
                .read_team()
                .ok()
                .and_then(|team| {
                    catervas_store::document_source::is_branchless(&team, &deps.log, &contract).ok()
                })
                .unwrap_or(false);
            if branchless {
                "its documents are in your folder".to_string()
            } else {
                format!("kept {}", task_branch(&contract))
            }
        },
    );
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!("removed its worktree and its sandbox, and {kept}"),
    }))
}

/// Lands an accepted task's changed documents in the owner's folder, or forgets the copy a
/// cancelled one started from, while its worktree is still there. What landed is recorded, and a
/// document whose version was set aside is said in the channel.
fn land_the_documents(
    orchestrator: &Orchestrator,
    row: &TaskProjection,
) -> Result<(), OrchestratorError> {
    let deps = &orchestrator.deps;
    let tools = &deps.tools;
    let root = tools.files.root();
    let tree = worktree(deps, &row.task_id);
    if !tree.is_dir() {
        return Ok(());
    }
    if row.status != TaskStatus::Accepted {
        catervas_store::local_documents::forget_start(root, &row.task_id)?;
        return Ok(());
    }
    let landing = catervas_store::local_documents::land(root, &tree, &row.task_id)?;
    if landing.written.is_empty() && landing.removed.is_empty() && landing.set_aside.is_empty() {
        return Ok(());
    }
    let body = serde_json::from_value(serde_json::json!({
        "paths": landing.written, "removed": landing.removed, "by": "task",
        "written_by": null, "task_id": row.task_id.as_str(), "set_aside": landing.set_aside,
    }))
    .map_err(|error| refused(format!("local_documents_unrecordable: {error}")))?;
    append_stamped(
        tools,
        tools.ids.clone(),
        EventBody::LocalDocumentsWritten(body),
    )?;
    let team = tools.files.read_team()?;
    let who = row
        .assignee_id
        .as_deref()
        .map_or_else(|| "the agent".to_string(), |id| name_of(&team, id));
    for path in &landing.set_aside {
        post_system(
            &tools.log,
            tools.clock.as_ref(),
            &tools.ids,
            None,
            &format!(
                "{} changed on this computer while {who} worked on it, so {who}'s version was \
                 not put in. It is in .catervas/local/documents/set-aside/{}/.",
                plain_names(&[path.as_str()]),
                row.task_id.as_str()
            ),
        )?;
    }
    Ok(())
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
    // The member's own git removes a member task's worktrees; the project folder's removes any
    // other's, and also what a repository that cannot be found leaves as plain folders.
    if let Some(member) = member_of(orchestrator, task_id) {
        for path in [&base, &own] {
            if remove_worktree(&member, path).is_err() {
                // A member that moved answers that the worktree is not its own.
                remove_folder(path)?;
            }
        }
        // The folders are gone either way, so a failure here is not tried again; the stale
        // registration is pruned by the next finished task in the member (5.15).
        let _ = member.prune_worktrees();
    } else {
        remove_worktree(&deps.tools.git, &base)?;
        remove_worktree(&deps.tools.git, &own)?;
    }
    Ok(true)
}

/// The member the task works in, when its contract, the team and `repos.yaml` say one that is
/// there. Whatever else it is, the task's worktrees are cleaned up as the project folder's.
fn member_of(orchestrator: &Orchestrator, task_id: &TaskId) -> Option<Git> {
    let deps = &orchestrator.deps;
    let contract = deps.tools.files.read_contract(task_id).ok()?;
    let team = deps.tools.files.read_team().ok()?;
    let found = task_repository_of(deps, &team, &contract).ok()?;
    found.repo.map(|_| found.git)
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
    remove_folder(path)
}

/// Removes the folder at `path` and what is in it; one that is not there is removed already.
fn remove_folder(path: &Path) -> Result<(), OrchestratorError> {
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
    use catervas_protocol::event::{
        EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventKind, NewEvent,
        PullRequestOpenedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
        event_from_value,
    };
    use catervas_store::git::fixtures::{TempRepo, git_in, git_output_in};
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

    /// The words of Today's integration rows for tasks.
    fn integration_lines(harness: &Harness) -> Vec<String> {
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        catervas_store::waiting::waiting(&deps.projections, &deps.log, &deps.files, &team)
            .expect("the store reads")
            .into_iter()
            .filter(|item| item.kind == catervas_store::waiting::WaitingKind::Integration)
            .map(|item| item.line)
            .collect()
    }

    /// The folder changes Today lists as waiting for a merge.
    fn folder_rows(harness: &Harness) -> Vec<serde_json::Value> {
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        crate::tools::folders::integration_rows(deps, &team).expect("the log reads")
    }

    /// A harness whose team keeps its documents on this computer, with `notes.html` in the
    /// Software Developer's folder at the root, ignored.
    fn keeping_documents_here(name: &str) -> Harness {
        let harness = Harness::new(name, |wire| {
            wire["policy"]["integration"] = json!("pull_request");
            wire["policy"]["documents_local"] = json!(true);
        });
        catervas_store::local_documents::exclude_documents(&harness.project.deps.git)
            .expect("the folder is ignored");
        let root = harness.project.repo.path.clone();
        std::fs::create_dir_all(root.join("docs/catervas/engineering")).expect("a folder");
        std::fs::write(
            root.join("docs/catervas/engineering/notes.html"),
            "notes v1",
        )
        .expect("v1");
        harness
    }

    /// Asks the governor for `to` for `task`, as `actor` (and `agent`) would.
    fn asks(
        harness: &Harness,
        task: &str,
        to: catervas_core::contract::TaskStatus,
        actor: catervas_core::governor::transition_table::TransitionActor,
        agent: &str,
    ) -> crate::transitions::TransitionOutcome {
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        harness
            .project
            .deps
            .transitions
            .request(
                &catervas_core::governor::transition::TransitionRequest {
                    task_id: task.parse().expect("a task id"),
                    to,
                    actor,
                    agent_id: Some(agent.to_string()),
                },
                &crate::transitions::TransitionAsk::default(),
                &team,
            )
            .expect("the request is judged")
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one task from its start to its landing"
    )]
    fn a_documents_task_lands_on_acceptance_with_no_pull_request() {
        use catervas_core::contract::TaskStatus;
        use catervas_core::governor::transition_table::TransitionActor;

        let harness = keeping_documents_here("docs-task-lands");
        let root = harness.project.repo.path.clone();
        harness.assigned_with("CTV-5", "dev-a", "dev-b", |wire| {
            wire["allowed_paths"] = json!(["docs/catervas/engineering/notes.html"]);
            wire["exit_criteria"] = json!([{
                "id": "C1", "text": "The notes are there.", "satisfies": ["R1"],
                "verification": { "method": "command",
                    "command": "test -f docs/catervas/engineering/notes.html",
                    "expect": { "exit_code": 0 } }
            }]);
        });
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let what = block_on(acted(&orchestrator));
        assert!(what.contains("started it"), "{what}");

        // The task has no branch: a detached worktree holding a copy of the folder.
        let worktree = harness.worktree("CTV-5");
        assert_eq!(
            std::fs::read_to_string(worktree.join("docs/catervas/engineering/notes.html"))
                .expect("the copy is there"),
            "notes v1"
        );
        assert!(
            git_output_in(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]) == "HEAD",
            "detached"
        );
        assert!(
            harness
                .project
                .deps
                .files
                .read_contract(&"CTV-5".parse().expect("an id"))
                .expect("the contract")
                .branch
                .is_none()
        );
        // Its documents are never committed.
        let refused = harness.project.call(
            "dev-a",
            Some("CTV-5"),
            "catervas_git_commit",
            json!({ "message": "add the notes", "paths": ["docs/catervas/engineering/notes.html"] }),
        );
        assert!(
            matches!(&refused, Err(crate::tools::ToolError::Refused { reason })
                if reason.starts_with("documents_stay_local: ")),
            "{refused:?}"
        );

        // It writes its version and asks for verifying with no commit.
        std::fs::write(
            worktree.join("docs/catervas/engineering/notes.html"),
            "notes v2",
        )
        .expect("v2");
        harness.project.record_by(
            Some("dev-a"),
            crate::tools::fixtures::at(),
            "CTV-5",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "the notes are there",
                     "run_by": "assignee", "recorded_by": "dev-a" }),
        );
        harness.project.record_by(
            Some("dev-a"),
            crate::tools::fixtures::at(),
            "CTV-5",
            "note.written",
            &json!({ "kind": "completion", "text": "Wrote the notes.", "written_by": "dev-a" }),
        );
        let verifying = asks(
            &harness,
            "CTV-5",
            TaskStatus::Verifying,
            TransitionActor::Assignee,
            "dev-a",
        );
        assert!(
            matches!(verifying, crate::transitions::TransitionOutcome::Moved(_)),
            "{verifying:?}"
        );

        // Accepted, it integrates nothing, and the documents land where the owner reads them.
        harness.project.record(
            "CTV-5",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "at abc: exit 0",
                     "run_by": "reviewer", "recorded_by": "governor" }),
        );
        harness.project.record_by(
            Some("dev-b"),
            crate::tools::fixtures::at(),
            "CTV-5",
            "note.written",
            &json!({ "kind": "review", "text": "C1: the notes are there.", "written_by": "dev-b" }),
        );
        let accepted = asks(
            &harness,
            "CTV-5",
            TaskStatus::Accepted,
            TransitionActor::ProductManager,
            "pm",
        );
        let crate::transitions::TransitionOutcome::Moved(decision) = accepted else {
            panic!("the notes are in the copy: {accepted:?}");
        };
        assert_eq!(
            decision.effects,
            [catervas_core::governor::transition::TransitionEffect::NothingToIntegrate]
        );
        assert!(!harness.row("CTV-5").awaiting_integration);
        harness.settle_delivery();
        let what = block_on(acted(&orchestrator));
        assert!(what.contains("removed its worktree"), "{what}");
        assert_eq!(
            std::fs::read_to_string(root.join("docs/catervas/engineering/notes.html"))
                .expect("the owner's copy"),
            "notes v2"
        );
        let written = harness.events(&[EventKind::LocalDocumentsWritten]);
        assert_eq!(written.len(), 1);
        assert_eq!(
            catervas_protocol::event::event_to_value(&written[0])["body"],
            json!({ "paths": ["docs/catervas/engineering/notes.html"], "removed": [],
                    "by": "task", "written_by": null, "task_id": "CTV-5", "set_aside": [] })
        );
        assert!(!harness.worktree("CTV-5").exists());
        assert!(!root.join(".catervas/local/documents/start/CTV-5").exists());
        assert!(harness.events(&[EventKind::PullRequestOpened]).is_empty());
        assert_eq!(creates(&harness), 0);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one task from its start to its pull request"
    )]
    fn a_code_task_lands_its_documents_and_opens_its_pull_request() {
        use catervas_core::contract::TaskStatus;
        use catervas_core::governor::transition_table::TransitionActor;

        let harness = keeping_documents_here("docs-with-code");
        harness.with_origin();
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", "https://github.com/o/r/pull/7\n", "", 0);
        let root = harness.project.repo.path.clone();
        std::fs::create_dir_all(root.join("docs/catervas/product")).expect("a folder");
        std::fs::write(root.join("docs/catervas/product/spec.html"), "spec s1").expect("s1");
        harness.assigned_with("CTV-5", "dev-a", "dev-b", |wire| {
            wire["allowed_paths"] = json!(["src/**", "docs/catervas/engineering/**"]);
            wire["exit_criteria"] = json!([{
                "id": "C1", "text": "The code is there.", "satisfies": ["R1"],
                "verification": { "method": "command", "command": "test -f src/a.rs",
                    "expect": { "exit_code": 0 } }
            }]);
        });
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let what = block_on(acted(&orchestrator));
        assert!(
            what.contains("started it") && what.contains("on feat/"),
            "{what}"
        );
        let worktree = harness.worktree("CTV-5");

        // It commits its code, writes a document of its folder, and strays into the product's.
        std::fs::create_dir_all(worktree.join("src")).expect("src");
        std::fs::write(worktree.join("src/a.rs"), "fn a() {}\n").expect("code");
        harness
            .project
            .deps
            .git
            .commit(&worktree, "add a", &["src/a.rs".to_string()])
            .expect("committed");
        std::fs::write(
            worktree.join("docs/catervas/engineering/conventions.html"),
            "conventions c1",
        )
        .expect("a document");
        std::fs::write(worktree.join("docs/catervas/product/spec.html"), "spec s2")
            .expect("strays");
        std::fs::write(worktree.join("docs/catervas/product/other.html"), "other").expect("strays");
        let record = |kind: &str, body: serde_json::Value, by: &str| {
            harness
                .project
                .record_by(Some(by), crate::tools::fixtures::at(), "CTV-5", kind, &body);
        };
        record(
            "criterion.recorded",
            json!({ "criterion_id": "C1", "passed": true, "evidence": "src/a.rs is there",
                    "run_by": "assignee", "recorded_by": "dev-a" }),
            "dev-a",
        );
        record(
            "note.written",
            json!({ "kind": "completion", "text": "Added a.", "written_by": "dev-a" }),
            "dev-a",
        );
        let verifying = asks(
            &harness,
            "CTV-5",
            TaskStatus::Verifying,
            TransitionActor::Assignee,
            "dev-a",
        );
        assert!(
            matches!(verifying, crate::transitions::TransitionOutcome::Moved(_)),
            "{verifying:?}"
        );
        // The tidy put the strays back from the task's copy before judging.
        assert_eq!(
            std::fs::read_to_string(worktree.join("docs/catervas/product/spec.html"))
                .expect("back"),
            "spec s1"
        );
        assert!(!worktree.join("docs/catervas/product/other.html").exists());
        let tidied = harness.events(&[EventKind::WorktreeTidied]);
        assert_eq!(
            catervas_protocol::event::event_to_value(&tidied[0])["body"],
            json!({ "put_back": ["docs/catervas/product/spec.html"],
                    "removed": ["docs/catervas/product/other.html"] })
        );

        harness.project.record(
            "CTV-5",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "at abc: exit 0",
                     "run_by": "reviewer", "recorded_by": "governor" }),
        );
        record(
            "note.written",
            json!({ "kind": "review", "text": "C1: src/a.rs is there.", "written_by": "dev-b" }),
            "dev-b",
        );
        let accepted = asks(
            &harness,
            "CTV-5",
            TaskStatus::Accepted,
            TransitionActor::ProductManager,
            "pm",
        );
        let crate::transitions::TransitionOutcome::Moved(decision) = accepted else {
            panic!("the work is done: {accepted:?}");
        };
        assert_eq!(decision.effects, []);
        assert!(harness.row("CTV-5").awaiting_integration);

        // The documents land at acceptance; the pull request carries the code alone.
        harness.settle_delivery();
        block_on(acted(&orchestrator));
        assert_eq!(
            std::fs::read_to_string(root.join("docs/catervas/engineering/conventions.html"))
                .expect("landed"),
            "conventions c1"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("docs/catervas/product/spec.html")).expect("kept"),
            "spec s1"
        );
        block_on(acted(&orchestrator));
        assert_eq!(creates(&harness), 1);
        assert_eq!(
            git_output_in(
                &root,
                &[
                    "diff",
                    "--name-only",
                    &format!("main...{}", harness.branch("CTV-5"))
                ]
            ),
            "src/a.rs"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_only_through_a_pull_request_or_a_hand_merge() {
        // With an origin: a pull request, never a merge of Catervas's, whatever the file says.
        let harness = under("int-only-pr", "auto_merge");
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        let main_before = at_root(&harness, &["rev-parse", "refs/heads/main"]);
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(opened(&harness).len(), 1);
        assert_eq!(
            at_root(&harness, &["rev-parse", "refs/heads/main"]),
            main_before
        );
        assert!(integrations(&harness).is_empty());
        assert_eq!(
            integration_lines(&harness),
            vec!["Accepted: its pull request waits for you to merge it".to_string()]
        );
        let sha = harness.merge_on_the_forge(&origin, "CTV-1");
        harness.gh.answers(
            "view",
            &format!(r#"{{"mergeCommit":{{"oid":"{sha}"}},"state":"MERGED"}}"#),
            "",
            0,
        );
        orchestrator.tick().await.expect("the tick runs");
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
        let _ = std::fs::remove_dir_all(&origin);

        // With no origin: it waits for a merge by hand, and the human's ask is refused until then.
        let harness = under("int-only-hand", "auto_merge");
        harness.accepted("CTV-1");
        assert_eq!(harness.folder_change(CADENCE, "a\n"), 1);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert!(integrations(&harness).is_empty());
        assert!(
            escalations(&harness).is_empty(),
            "{:?}",
            escalations(&harness)
        );
        assert_eq!(
            integration_lines(&harness),
            vec!["Accepted: waiting for you to merge feature/CTV-1".to_string()]
        );
        assert_eq!(folder_rows(&harness).len(), 1);
        let refusal = orchestrator.integrate(&task("CTV-1")).await;
        assert!(
            matches!(&refusal, Err(OrchestratorError::Refused { reason }) if reason.starts_with("not_merged: ")),
            "{refusal:?}"
        );
        let refusal = orchestrator.integrate_folder_change(1).await;
        assert!(
            matches!(&refusal, Err(OrchestratorError::Refused { reason }) if reason.starts_with("not_merged: ")),
            "{refusal:?}"
        );
        harness.merge_by_hand("CTV-1");
        harness.merge_branch_by_hand(&harness.folder_branch(1));
        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
        assert_eq!(
            folder_events(&harness, EventKind::FolderChangeIntegrated).len(),
            1
        );
        assert!(integration_lines(&harness).is_empty());
        assert!(folder_rows(&harness).is_empty());
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
        harness.settle_delivery();
        harness.merge_by_hand("CTV-1");
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
    async fn never_rescans_a_project_folder_itself() {
        // The Architect's documents land in the project folder, whose own scan never runs: its
        // members' checks stay in the library, and the integration reports nothing about a scan.
        let (harness, _members) = Harness::project_folder_with(
            "int-folder-docs",
            &["app"],
            crate::tools::fixtures::with_the_suggested_six,
        );
        let library = catervas_core::criteria::validate_criteria(&json!({ "criteria": [
            { "name": "app-the-tests-pass", "text": "Every test passes in app.",
              "source": "project_scan", "repo": "app",
              "verification": { "method": "test", "command": "npm run test" } },
        ]}))
        .expect("a library");
        harness
            .project
            .deps
            .files
            .write_criteria(&library)
            .expect("the library is written");
        harness.verifying_with("CTV-1", true, true, |wire| {
            wire["assignee_role"] = json!("architect");
            wire["reviewer_role"] = json!("product_manager");
            wire["allowed_paths"] = json!(["docs/**", "done.txt"]);
        });
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "ada", "reviewer": "pm" }),
        );
        harness
            .project
            .deps
            .git
            .remove_worktree(&harness.worktree("CTV-1"))
            .expect("the task's worktree is removed");
        harness.merge_by_hand("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let merged = orchestrator.integrate(&task("CTV-1")).await;

        let head = at_root(&harness, &["rev-parse", "main"]);
        assert_eq!(
            merged,
            Ok(IntegrationOutcome::Merged {
                sha: head,
                scan: ScanRefresh::Unchanged
            })
        );
        let kept = harness
            .project
            .deps
            .files
            .read_criteria()
            .expect("the library reads");
        assert_eq!(kept, library, "the members' checks are not wiped");
        assert!(
            harness.events(&[EventKind::ProjectScanned]).is_empty(),
            "the project folder is not scanned"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_an_integrated_task_without_merging_again() {
        let harness = under("int-twice", "auto_merge");
        harness.accepted("CTV-1");
        harness.merge_by_hand("CTV-1");
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
        harness.settle_delivery();
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
    async fn escalates_a_push_origin_refuses() {
        let harness = under("int-push-fails", "pull_request");
        harness.with_origin_nowhere();
        harness.accepted("CTV-1");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let raised = escalations(&harness);
        assert_eq!(raised.len(), 1, "{raised:?}");
        assert_eq!(raised[0].reason, EscalationRaisedBodyReason::Integration);
        assert!(
            raised[0].detail.starts_with(&format!(
                "pushing {} to origin failed",
                harness.branch("CTV-1")
            )),
            "{}",
            raised[0].detail
        );
        assert!(opened(&harness).is_empty());
        assert!(integrations(&harness).is_empty());
        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
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
        harness.merge_by_hand("CTV-1");
        harness.project.record(
            "CTV-1",
            "escalation.raised",
            &json!({ "reason": "budget", "detail": "the task's budget is spent" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(integrations(&harness).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn opens_a_pull_request_for_an_accepted_plan() {
        const PLAN: &str = "docs/catervas/architecture/plans/CTV-9.md";
        let harness = under("int-plan-task", "pull_request");
        let origin = harness.with_origin();
        harness.verifying_with("CTV-1", true, true, |wire| {
            wire["plan"] = json!(PLAN);
            wire["allowed_paths"] = json!([PLAN]);
            wire["assignee_role"] = json!("architect");
            wire["reviewer_role"] = json!("product_manager");
        });
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness.settle_delivery();
        harness
            .project
            .deps
            .git
            .remove_worktree(&harness.worktree("CTV-1"))
            .expect("the worktree goes");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        // Its plan is on its own branch alone: it opens a pull request like any task.
        orchestrator.tick().await.expect("the tick runs");
        let recorded = opened(&harness);
        assert_eq!(recorded.len(), 1, "{recorded:?}");
        assert_eq!(recorded[0].branch, harness.branch("CTV-1"));
        assert!(harness.row("CTV-1").awaiting_integration);

        // Once the plan is on the integration branch, it is integrated with no call to the forge.
        let calls = harness.gh.calls().len();
        std::fs::create_dir_all(
            harness
                .project
                .repo
                .path
                .join("docs/catervas/architecture/plans"),
        )
        .expect("the plans folder is made");
        harness.commit_at_root(PLAN, "# Plan for CTV-9\n", "docs: plan CTV-9");
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.gh.calls().len(), calls);
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].into, "main");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Governor
        );
        assert!(!harness.row("CTV-1").awaiting_integration);
        let _ = std::fs::remove_dir_all(&origin);
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
            &json!({ "kind": "review", "text": "C1 still passes, on a second look. The second sentence stays out.", "written_by": "dev-b" }),
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
        let headings: Vec<&str> = body
            .lines()
            .filter(|line| line.starts_with("## "))
            .collect();
        assert_eq!(
            headings,
            ["## What changed", "## How it was verified"],
            "{body}"
        );
        for criterion in &contract.exit_criteria {
            assert!(body.contains(criterion.text.as_str()), "{body}");
        }
        assert!(
            body.contains("- Reviewed by dev-b: C1 still passes, on a second look."),
            "{body}"
        );
        assert!(!body.contains("The second sentence stays out."), "{body}");
        assert!(
            !body.contains("Added done.txt; nothing left out."),
            "{body}"
        );
        assert!(!body.contains(contract.intent.as_str()), "{body}");
        assert!(body.chars().count() <= 1500, "{body}");
        harness.gh.answers("view", OPEN, "", 0);
        assert_eq!(
            orchestrator.integrate(&task("CTV-1")).await,
            Ok(IntegrationOutcome::AwaitingForge)
        );
        assert_eq!(opened(&harness).len(), 1);
        assert_eq!(creates(&harness), 1);
        let _ = std::fs::remove_dir_all(&origin);
    }

    const CONFLICTING: &str = r#"{"mergeCommit":null,"state":"OPEN","mergeable":"CONFLICTING"}"#;

    /// `main` gains a commit writing `path`, on the project and on `origin`: other work landed.
    fn main_moves_on(harness: &Harness, path: &str, text: &str) {
        harness.commit_at_root(path, text, "docs: other work landed first");
        git_in(&harness.project.repo.path, &["push", "origin", "main"]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_back_a_pull_request_that_stopped_merging() {
        let (harness, origin) = a_pull_request("int-pr-conflicts", true);
        let git = &harness.project.deps.git;
        let side = harness.worktree("CTV-1");
        git.add_worktree(&side, &harness.branch("CTV-1"))
            .expect("the branch is checked out");
        std::fs::write(side.join("README.md"), "the task's line\n").expect("written");
        git.commit(&side, "Edit README", &["README.md".to_string()])
            .expect("committed");
        git.remove_worktree(&side).expect("it is taken away");
        main_moves_on(&harness, "README.md", "main wrote this first\n");
        harness.gh.answers("view", CONFLICTING, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let conflicted = harness.events(&[EventKind::PullRequestConflicted]);
        let [conflicted] = conflicted.as_slice() else {
            panic!("expected one: {conflicted:?}");
        };
        let EventBody::PullRequestConflicted(body) = &conflicted.body else {
            panic!("{conflicted:?}");
        };
        assert_eq!(body.url, PULL_URL);
        assert_eq!(body.into, "main");
        assert_eq!(body.paths, vec!["README.md".to_string()]);
        let row = harness.row("CTV-1");
        assert_eq!(row.status, catervas_core::contract::TaskStatus::Assigned);
        assert!(
            !row.awaiting_integration,
            "Today lists no merge to wait for"
        );
        let moved = harness
            .events(&[EventKind::TaskTransitioned])
            .pop()
            .expect("a move");
        let EventBody::TaskTransitioned(moved) = &moved.body else {
            panic!("{moved:?}");
        };
        assert_eq!(moved.requested_by, "governor");
        assert_eq!(
            serde_json::to_value(moved.gate).expect("a gate"),
            json!("pull_request_conflicted")
        );
        let worktree = harness.worktree("CTV-1");
        let text = std::fs::read_to_string(worktree.join("README.md")).expect("read");
        assert!(text.contains("<<<<<<< "), "{text}");
        assert!(
            harness
                .project
                .deps
                .git
                .is_merging(&worktree)
                .expect("the read works")
        );

        orchestrator.tick().await.expect("the next tick runs");
        assert_eq!(
            harness.row("CTV-1").status,
            catervas_core::contract::TaskStatus::InProgress
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refreshes_a_pull_request_that_merges_again() {
        let (harness, origin) = a_pull_request("int-pr-refreshes", true);
        main_moves_on(&harness, "other.txt", "other work\n");
        let old_tip = at_root(&harness, &["rev-parse", &harness.branch("CTV-1")]);
        harness.gh.answers("view", CONFLICTING, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        let refreshed = harness.events(&[EventKind::PullRequestRefreshed]);
        let [refreshed] = refreshed.as_slice() else {
            panic!("expected one: {refreshed:?}");
        };
        let EventBody::PullRequestRefreshed(body) = &refreshed.body else {
            panic!("{refreshed:?}");
        };
        assert_eq!(body.url, PULL_URL);
        let branch = harness.branch("CTV-1");
        assert_eq!(git_output_in(&origin, &["rev-parse", &branch]), body.sha);
        assert_eq!(
            git_output_in(&origin, &["rev-parse", &format!("{}^1", body.sha)]),
            old_tip
        );
        assert_eq!(
            git_output_in(&origin, &["rev-parse", &format!("{}^2", body.sha)]),
            git_output_in(&origin, &["rev-parse", "main"])
        );
        assert_eq!(
            harness.row("CTV-1").status,
            catervas_core::contract::TaskStatus::Accepted
        );
        assert!(!harness.worktree("CTV-1").exists());

        // The forge still says it conflicts, as it does for a while after a push: nothing repeats.
        orchestrator.tick().await.expect("the next tick runs");
        assert_eq!(harness.events(&[EventKind::PullRequestRefreshed]).len(), 1);
        assert!(!harness.worktree("CTV-1").exists());
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_tasks_pull_request_carries_no_task_id() {
        let harness = under("int-pr-no-id", "pull_request");
        let origin = harness.with_origin();
        harness.file("CTV-2", "cancelled", |wire| {
            wire["title"] = json!("Pie pre-orders");
        });
        harness.assigned_with("CTV-1", "dev-a", "dev-b", |wire| {
            wire["title"] = json!("Character counter on the note input");
        });
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        // Rule 7 starts it: its branch is named from its title and recorded.
        orchestrator.tick().await.expect("the tick runs");
        let branch = harness.branch("CTV-1");
        assert_eq!(branch, "feat/character-counter-on-the-note-input");
        let worktree = harness.worktree("CTV-1");
        std::fs::write(worktree.join("counter.txt"), "1\n").expect("written");
        harness
            .project
            .deps
            .git
            .commit(
                &worktree,
                "feat: count the note's characters",
                &["counter.txt".to_string()],
            )
            .expect("committed");
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "completion", "text": "Added the counter; CTV-2 still owes the pies.", "written_by": "dev-a" }),
        );
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "C1 passes; CTV-2 still owes the pies.", "written_by": "dev-b" }),
        );
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        let mut by_assignee = people.clone();
        by_assignee["actor"] = json!("assignee");
        by_assignee["requested_by"] = json!("dev-a");
        harness
            .project
            .moved("CTV-1", "in_progress", "verifying", &by_assignee);
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        harness.settle_delivery();
        harness
            .project
            .deps
            .git
            .remove_worktree(&worktree)
            .expect("the worktree is removed");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            git_output_in(&origin, &["rev-parse", &branch]),
            at_root(&harness, &["rev-parse", &branch])
        );
        let create = harness
            .gh
            .calls()
            .into_iter()
            .find(|call| call.get(1).map(String::as_str) == Some("create"))
            .expect("a pull request is created");
        let after = |flag: &str| {
            let at = create.iter().position(|argument| argument == flag);
            at.and_then(|at| create.get(at + 1)).cloned()
        };
        assert_eq!(after("--head"), Some(branch.clone()));
        assert_eq!(
            after("--title").as_deref(),
            Some("Feat: Character counter on the note input")
        );
        let body = harness.gh.stdin_of("create");
        assert!(
            body.contains("\"Pie pre-orders\" still owes the pies"),
            "{body}"
        );
        assert!(!body.contains("CTV-"), "{body}");
        let log = at_root(
            &harness,
            &["log", "--format=%B", &format!("main..{branch}")],
        );
        assert!(!log.contains("CTV-"), "{log}");
        let recorded = opened(&harness);
        assert_eq!(recorded.len(), 1, "{recorded:?}");
        assert_eq!(recorded[0].branch, branch);
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

    /// CTV-1 accepted with its pull request 7 opened, under a team whose file says `key` and whose
    /// log holds the owner's grant when `grant`, `policy` being how it integrates.
    fn merging(name: &str, policy: &str, key: bool, grant: bool) -> (Harness, std::path::PathBuf) {
        let policy = policy.to_string();
        let harness = Harness::new(name, move |wire| {
            wire["policy"]["integration"] = json!(policy);
            if key {
                wire["policy"]["merge_when_checks_pass"] = json!(true);
            }
        });
        let origin = harness.with_origin();
        harness.accepted("CTV-1");
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "review", "text": "C1 passes; done.txt is there.", "written_by": "dev-b" }),
        );
        harness.project.record(
            "CTV-1",
            "pull_request.opened",
            &json!({ "url": PULL_URL, "number": 7, "branch": harness.branch("CTV-1") }),
        );
        if grant {
            harness.project.record(
                "",
                "merge_permission.granted",
                &json!({ "by": "human", "into": "main" }),
            );
        }
        harness.gh.answers("repos/{owner}/{repo}", "{}", "", 0);
        harness.gh.answers("merge", "", "", 0);
        (harness, origin)
    }

    /// What `gh pr view` answers of an open pull request at `abc123` that GitHub calls `state`,
    /// whose checks are the `rollup`.
    fn viewed_open(state: &str, rollup: &str) -> String {
        format!(
            r#"{{"state":"OPEN","mergeCommit":null,"mergeable":"MERGEABLE","mergeStateStatus":"{state}","headRefOid":"abc123","statusCheckRollup":{rollup}}}"#
        )
    }

    const PASSED: &str =
        r#"[{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"}]"#;

    fn merges(harness: &Harness) -> Vec<Vec<String>> {
        harness
            .gh
            .calls()
            .into_iter()
            .filter(|call| call.get(1).map(String::as_str) == Some("merge"))
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn merges_a_pull_request_once_its_checks_pass() {
        for (name, rollup) in [("int-merges", PASSED), ("int-merges-no-checks", "[]")] {
            let (harness, origin) = merging(name, "pull_request", true, true);
            let sha = harness.merge_on_the_forge(&origin, "CTV-1");
            harness
                .gh
                .answers_on_call("view", 1, &viewed_open("CLEAN", rollup))
                .answers_on_call(
                    "view",
                    2,
                    &format!(r#"{{"state":"MERGED","mergeCommit":{{"oid":"{sha}"}}}}"#),
                );
            let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

            orchestrator.tick().await.expect("the tick runs");

            let url = PULL_URL.to_string();
            let args = [
                "pr",
                "merge",
                &url,
                "--merge",
                "--match-head-commit",
                "abc123",
            ]
            .map(str::to_string)
            .to_vec();
            assert_eq!(merges(&harness), vec![args], "{name}");
            let kinds: Vec<EventKind> = harness
                .events(&[EventKind::PullRequestMerged, EventKind::TaskIntegrated])
                .iter()
                .map(|event| event.body.kind())
                .collect();
            assert_eq!(
                kinds,
                [EventKind::PullRequestMerged, EventKind::TaskIntegrated],
                "{name}"
            );
            let merged = harness.events(&[EventKind::PullRequestMerged]);
            assert!(
                matches!(&merged[0].body, EventBody::PullRequestMerged(body)
                    if body.url.to_string() == PULL_URL && body.method.to_string() == "merge"),
                "{name}: {merged:?}"
            );
            let integrated = integrations(&harness);
            assert_eq!(integrated[0].sha, sha, "{name}");
            assert_eq!(
                integrated[0].integrated_by,
                TaskIntegratedBodyIntegratedBy::Governor,
                "{name}"
            );
            assert_eq!(at_root(&harness, &["rev-parse", "main"]), sha, "{name}");
            let _ = std::fs::remove_dir_all(&origin);
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn never_merges_without_permission_or_passing_checks() {
        let failed = r#"[{"__typename":"CheckRun","name":"lint","status":"COMPLETED","conclusion":"FAILURE"}]"#;
        let blocked = "GitHub will not merge https://github.com/o/r/pull/7 yet: it needs a review or another of the branch's rules. Merge it yourself once it can.";
        let failing = "A check failed on GitHub: lint. Catervas did not merge https://github.com/o/r/pull/7; fix it or merge it yourself.";
        for (name, policy, key, grant, state, rollup, escalation) in [
            (
                "int-never-no-grant",
                "pull_request",
                true,
                false,
                "CLEAN",
                PASSED,
                None,
            ),
            (
                "int-never-no-key",
                "auto_merge",
                false,
                true,
                "CLEAN",
                PASSED,
                None,
            ),
            (
                "int-never-failed",
                "pull_request",
                true,
                true,
                "UNSTABLE",
                failed,
                Some(failing),
            ),
            (
                "int-never-blocked",
                "pull_request",
                true,
                true,
                "BLOCKED",
                PASSED,
                Some(blocked),
            ),
        ] {
            let (harness, origin) = merging(name, policy, key, grant);
            harness
                .gh
                .answers("view", &viewed_open(state, rollup), "", 0);
            let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

            orchestrator.tick().await.expect("the tick runs");

            assert!(
                merges(&harness).is_empty(),
                "{name}: {:?}",
                harness.gh.calls()
            );
            assert!(integrations(&harness).is_empty(), "{name}");
            let raised = escalations(&harness);
            assert_eq!(
                raised
                    .iter()
                    .map(|each| each.detail.as_str())
                    .collect::<Vec<_>>(),
                escalation.into_iter().collect::<Vec<_>>(),
                "{name}"
            );
            let _ = std::fs::remove_dir_all(&origin);
        }
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
    /// branch, and the branch merged by hand into `main`.
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
        harness.settle_delivery();
        harness
            .project
            .deps
            .git
            .remove_worktree(&worktree)
            .expect("the worktree is removed");
        harness.merge_by_hand(task);
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
            wire["policy"]["integration"] = json!("pull_request");
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

    const ROADMAP: &str = "docs/catervas/product/roadmap.md";
    const ROADMAP_TWIN: &str = "docs/catervas/product/roadmap.agent.md";

    /// A harness with the Architect, under `policy`, whose main holds the roadmap with its twin
    /// and the architecture overview.
    fn with_documents(name: &str, policy: &str) -> Harness {
        let policy = policy.to_string();
        let harness = Harness::new(name, move |wire| {
            crate::tools::fixtures::with_the_designer(wire);
            wire["policy"]["integration"] = json!(policy);
        });
        let repo = &harness.project.repo;
        repo.write(ROADMAP, "the roadmap\n");
        repo.write(ROADMAP_TWIN, "the roadmap for agents\n");
        repo.write("docs/catervas/architecture/overview.md", "the overview\n");
        repo.commit("the documents");
        harness
    }

    /// The owner's save of `path` as `text`, through the daemon, and the change it made.
    fn owner_saves(harness: &Harness, path: &str, text: &str) -> u64 {
        let base = at_root(harness, &["rev-parse", &format!("main:{path}")]);
        let reply = crate::daemon::fixtures::rpc(
            &harness.daemon,
            "files.save",
            &json!({ "path": path, "text": text, "base": base }),
        );
        reply["result"]["change"]
            .as_u64()
            .unwrap_or_else(|| panic!("a change was made: {reply}"))
    }

    fn block_on<T>(work: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(work)
    }

    /// The owner merges folder change `change` by hand and asks for it to be added.
    fn adds(harness: &Harness, orchestrator: &crate::orchestrator::Orchestrator, change: u64) {
        harness.merge_branch_by_hand(&harness.folder_branch(change));
        block_on(
            orchestrator
                .handle(catervas_protocol::command::Command::FolderChangeIntegrate { change }),
        )
        .expect("the owner adds it");
    }

    /// The tasks filed to bring a twin up to date, with the twin each brings up to date.
    fn rederives(harness: &Harness) -> Vec<(String, String)> {
        catervas_store::requests::rederive_tasks(&harness.project.deps.log)
            .expect("the log reads")
            .into_iter()
            .map(|(id, twin)| (id.to_string(), twin))
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_the_rederive_when_the_owners_change_is_integrated() {
        let harness = with_documents("int-rederive", "pull_request");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        // An agent's change to the human document files none, before any owner's edit is held.
        let agents = harness.folder_change(ROADMAP, "an agent's roadmap\n");
        adds(&harness, &orchestrator, agents);
        assert!(
            rederives(&harness).is_empty(),
            "an agent's change files none"
        );

        let change = owner_saves(&harness, ROADMAP, "the owner's roadmap\n");
        assert!(
            rederives(&harness).is_empty(),
            "nothing is filed at the save"
        );
        adds(&harness, &orchestrator, change);
        assert_eq!(
            rederives(&harness),
            [("CTV-1".to_string(), ROADMAP_TWIN.to_string())]
        );
        let contract = harness
            .project
            .deps
            .files
            .read_contract(&task("CTV-1"))
            .expect("the contract");
        assert_eq!(
            contract.assignee_role,
            catervas_core::contract::Role::ProductManager
        );
        assert_eq!(
            contract.reviewer_role,
            catervas_core::contract::Role::Architect
        );
        assert_eq!(contract.allowed_paths, [ROADMAP_TWIN.to_string()]);
        assert_eq!(
            contract.title.as_str(),
            "Bring roadmap.agent.md up to date with the owner's change"
        );
        assert!(
            contract.intent.as_str().contains(ROADMAP)
                && contract.intent.as_str().contains(ROADMAP_TWIN),
            "{}",
            contract.intent.as_str()
        );
        assert_eq!(contract.scope.in_scope, [ROADMAP_TWIN.to_string()]);
        let created = harness.events(&[EventKind::TaskCreated]);
        let EventBody::TaskCreated(created) = &created[0].body else {
            panic!("a task.created");
        };
        assert_eq!(created.created_by, "catervas");
        let triaged = harness.events(&[EventKind::RequestTriaged]);
        let EventBody::RequestTriaged(triaged) = &triaged[0].body else {
            panic!("a request.triaged");
        };
        assert_eq!(
            (triaged.triaged_by.as_str(), triaged.reason.as_str()),
            (
                "catervas",
                "a re-derive of docs/catervas/product/roadmap.agent.md"
            )
        );

        // A second integrated save while the first re-derive is a draft files none.
        let change = owner_saves(&harness, ROADMAP, "a second roadmap\n");
        adds(&harness, &orchestrator, change);
        assert_eq!(rederives(&harness).len(), 1);

        // Once that task is under way, the next one files another.
        harness
            .project
            .moved("CTV-1", "draft", "in_progress", &json!({}));
        let change = owner_saves(&harness, ROADMAP, "a third roadmap\n");
        adds(&harness, &orchestrator, change);
        assert_eq!(rederives(&harness).len(), 2);

        // Another document of the owner's files none.
        let change = owner_saves(&harness, "docs/catervas/architecture/overview.md", "x\n");
        adds(&harness, &orchestrator, change);
        assert_eq!(rederives(&harness).len(), 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_the_rederive_of_an_html_twin() {
        let harness = with_documents("int-rederive-html", "pull_request");
        let spec = "docs/catervas/product/spec.html";
        let twin = "docs/catervas/product/spec.agent.html";
        harness.project.repo.write(spec, "<h1>spec</h1>\n");
        harness.project.repo.write(twin, "<h1>agents</h1>\n");
        harness.project.repo.commit("the html spec");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let change = owner_saves(&harness, spec, "<h1>new spec</h1>\n");
        adds(&harness, &orchestrator, change);

        assert_eq!(
            rederives(&harness),
            [("CTV-1".to_string(), twin.to_string())]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_the_rederive_when_the_tick_integrates_the_owners_change() {
        let harness = with_documents("int-rederive-auto", "pull_request");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let change = owner_saves(&harness, ROADMAP, "the owner's roadmap\n");
        harness.merge_branch_by_hand(&harness.folder_branch(change));
        for _ in 0..3 {
            block_on(orchestrator.tick()).expect("the tick runs");
            if !rederives(&harness).is_empty() {
                break;
            }
        }
        assert_eq!(
            rederives(&harness),
            [("CTV-1".to_string(), ROADMAP_TWIN.to_string())]
        );
    }

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
    async fn opens_a_pull_request_for_a_folder_change() {
        let harness = under("int-folder-pr", "pull_request");
        let origin = harness.with_origin();
        harness.folder_change(CADENCE, "a\n");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        // Named for the document, no number (ADR 0066).
        let name = catervas_core::folders::document_name(CADENCE, &|_| None);
        let branch = format!("docs/{}", catervas_core::branch::branch_slug(&name));

        let (change, what) = folder_report(orchestrator.tick().await.expect("the tick runs"));

        assert_eq!(
            (change, what),
            (1, format!("opened the pull request {PULL_URL}"))
        );
        assert_eq!(
            git_output_in(&origin, &["rev-parse", &branch]),
            at_root(&harness, &["rev-parse", &branch])
        );
        assert_eq!(harness.folder_branch(1), branch);
        let written = folder_events(&harness, EventKind::FolderDocWritten);
        assert_eq!(written[0]["body"]["branch"], branch.as_str());
        let create = harness
            .gh
            .calls()
            .into_iter()
            .find(|call| call.get(1).map(String::as_str) == Some("create"))
            .expect("a pull request is created");
        assert!(
            create.contains(&format!("Docs: Update {name}")),
            "{create:?}"
        );
        assert!(create.contains(&branch), "{create:?}");
        assert_eq!(
            harness.gh.stdin_of("create"),
            format!(
                "## What changed\n\n- {name}\n\n## How it was verified\n\n- Written by sm at the \
                 delivery's review; the team's documents are not reviewed (ADR 0061)."
            )
        );
        let opened = folder_events(&harness, EventKind::FolderChangeOpened);
        assert_eq!(
            opened[0]["body"],
            json!({ "change": 1, "url": PULL_URL, "number": 7 })
        );

        // Once the forge says merged, the next tick records it as the human's and brings main up.
        let sha = harness.merge_branch_on_the_forge(&origin, &branch);
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

    /// A project folder over `app` and `infra`, integrating under `policy`.
    fn in_members(name: &str, policy: &str) -> (Harness, Vec<TempRepo>) {
        let policy = policy.to_string();
        Harness::project_folder_with(name, &["app", "infra"], move |wire| {
            wire["policy"]["integration"] = json!(policy);
        })
    }

    /// `id` accepted in `app` with its worktree gone.
    fn accepted_in_app(harness: &Harness, app: &TempRepo, id: &str) {
        harness.accepted_in(id, &app.path, Some("app"));
        app.adapter()
            .remove_worktree(&harness.worktree(id))
            .expect("the task's worktree is removed");
    }

    fn app_canonical(app: &TempRepo) -> std::path::PathBuf {
        std::fs::canonicalize(&app.path).expect("app is there")
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn integrates_in_its_member_once_merged_by_hand() {
        let (harness, members) = in_members("int-member-hand", "pull_request");
        let (app, infra) = (&members[0], &members[1]);
        accepted_in_app(&harness, app, "CTV-1");
        let root_before = at_root(&harness, &["rev-parse", "main"]);
        let infra_before = infra.git_output(&["rev-parse", "main"]);
        let app_before = app.git_output(&["rev-parse", "main"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        assert_eq!(app.git_output(&["rev-parse", "main"]), app_before);
        assert!(escalations(&harness).is_empty());

        app.git(&["merge", "--no-ff", "-m", "By hand", "feature/CTV-1"]);
        let merged = orchestrator.integrate(&task("CTV-1")).await;

        let head = app.git_output(&["rev-parse", "main"]);
        assert!(
            matches!(&merged, Ok(IntegrationOutcome::Merged { sha, .. }) if *sha == head),
            "{merged:?}"
        );
        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(
            integrated[0].integrated_by,
            TaskIntegratedBodyIntegratedBy::Human
        );
        assert_eq!(integrated[0].into, "main");
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), root_before);
        assert_eq!(infra.git_output(&["rev-parse", "main"]), infra_before);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn opens_and_follows_the_pull_request_in_the_member() {
        let (harness, members) = in_members("int-member-pr", "pull_request");
        let app = &members[0];
        let origin = Harness::with_origin_in(&app.path);
        accepted_in_app(&harness, app, "CTV-1");
        let root_before = at_root(&harness, &["rev-parse", "main"]);
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            git_output_in(&origin, &["rev-parse", &harness.branch("CTV-1")]),
            app.git_output(&["rev-parse", &harness.branch("CTV-1")])
        );
        assert_eq!(opened(&harness).len(), 1);
        assert_eq!(creates(&harness), 1);

        let sha =
            Harness::merge_branch_on_the_forge_in(&app.path, &origin, &harness.branch("CTV-1"));
        harness.gh.answers(
            "view",
            &format!(r#"{{"state":"MERGED","mergeCommit":{{"oid":"{sha}"}}}}"#),
            "",
            0,
        );
        orchestrator.tick().await.expect("the tick runs");

        let integrated = integrations(&harness);
        assert_eq!(integrated.len(), 1, "{integrated:?}");
        assert_eq!(integrated[0].sha, sha);
        assert_eq!(integrated[0].into, "main");
        assert_eq!(app.git_output(&["rev-parse", "main"]), sha);
        assert_eq!(at_root(&harness, &["rev-parse", "main"]), root_before);
        assert!(escalations(&harness).is_empty());
        let cwds = harness.gh.cwds();
        assert!(!cwds.is_empty());
        assert!(
            cwds.iter().all(|cwd| *cwd == app_canonical(app)),
            "{cwds:?}"
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_while_its_member_is_missing() {
        let (harness, members) = in_members("int-member-missing", "auto_merge");
        let app = &members[0];
        accepted_in_app(&harness, app, "CTV-1");
        accepted_in_app(&harness, app, "CTV-2");
        for branch in ["feature/CTV-1", "feature/CTV-2"] {
            app.git(&["merge", "--no-ff", "-m", "By hand", branch]);
        }
        let there = app_canonical(app);
        let moved = std::path::PathBuf::from(format!("{}-moved", app.path.display()));
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&app.path, &moved).expect("the member is moved away");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let events = harness.project.event_count();

        for _ in 0..2 {
            assert_eq!(orchestrator.tick().await.expect("the tick runs"), idle());
        }

        assert_eq!(harness.project.event_count(), events);
        assert!(harness.row("CTV-1").awaiting_integration);

        std::fs::rename(&moved, &app.path).expect("the member is moved back");
        let report = orchestrator.tick().await.expect("the tick runs");
        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        assert!(!harness.row("CTV-1").awaiting_integration);
        assert!(
            escalations(&harness).is_empty(),
            "{:?}",
            escalations(&harness)
        );

        // The human is told why nothing happens.
        std::fs::rename(&app.path, &moved).expect("the member is moved away again");
        let answered = orchestrator.integrate(&task("CTV-2")).await;
        assert!(
            matches!(&answered, Ok(IntegrationOutcome::Escalated { detail, .. })
                if detail.contains(&format!("app is not at {}", there.display()))),
            "{answered:?}"
        );
        assert_eq!(escalations(&harness).len(), 1);
        assert!(harness.row("CTV-2").awaiting_integration);

        std::fs::rename(&moved, &app.path).expect("the member is moved back");
        let merged = orchestrator.integrate(&task("CTV-2")).await;
        assert!(
            matches!(merged, Ok(IntegrationOutcome::Merged { .. })),
            "{merged:?}"
        );
        assert!(!harness.row("CTV-2").awaiting_integration);
    }

    /// An Architect's `CTV-1` accepted in the project folder, its worktree gone.
    fn a_docs_task_accepted(harness: &Harness) {
        harness.verifying_with("CTV-1", true, true, |wire| {
            wire["assignee_role"] = json!("architect");
            wire["reviewer_role"] = json!("product_manager");
            wire["allowed_paths"] = json!(["docs/**", "done.txt"]);
        });
        harness.project.moved(
            "CTV-1",
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "ada", "reviewer": "pm" }),
        );
        harness.settle_delivery();
        harness
            .project
            .deps
            .git
            .remove_worktree(&harness.worktree("CTV-1"))
            .expect("the task's worktree is removed");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn still_integrates_a_docs_task_and_a_folder_change_in_the_project_folder() {
        let policy = |name: &str, policy: &'static str| {
            Harness::project_folder_with(name, &["app", "infra"], move |wire| {
                crate::tools::fixtures::with_the_suggested_six(wire);
                wire["policy"]["integration"] = json!(policy);
            })
        };

        let (harness, members) = policy("int-member-docs-merge", "auto_merge");
        a_docs_task_accepted(&harness);
        harness.folder_change(CADENCE, "a\n");
        harness.merge_by_hand("CTV-1");
        harness.merge_branch_by_hand(&harness.folder_branch(1));
        let before: Vec<String> = members
            .iter()
            .map(|member| member.git_output(&["rev-parse", "main"]))
            .collect();
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator
            .tick()
            .await
            .expect("the tick integrates the task");
        orchestrator
            .tick()
            .await
            .expect("the tick integrates the change");
        assert_eq!(integrations(&harness).len(), 1);
        assert_eq!(
            harness.events(&[EventKind::FolderChangeIntegrated]).len(),
            1
        );
        let parents = at_root(
            &harness,
            &["rev-list", "--parents", "-n", "1", "refs/heads/main"],
        );
        assert_eq!(parents.split(' ').count(), 3, "{parents}");
        assert!(
            at_root(&harness, &["branch", "--merged", "main"]).contains(&harness.folder_branch(1)),
            "the change landed in the project folder"
        );
        assert!(
            at_root(&harness, &["branch", "--merged", "main"]).contains(&harness.branch("CTV-1")),
            "the task landed in the project folder"
        );
        let after: Vec<String> = members
            .iter()
            .map(|member| member.git_output(&["rev-parse", "main"]))
            .collect();
        assert_eq!(before, after, "the members are untouched");

        let (harness, _members) = policy("int-member-docs-pr", "pull_request");
        let origin = Harness::with_origin_in(&harness.project.repo.path);
        a_docs_task_accepted(&harness);
        harness.folder_change(CADENCE, "a\n");
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", &format!("{PULL_URL}\n"), "", 0);
        harness.gh.answers("view", OPEN, "", 0);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator
            .tick()
            .await
            .expect("the tick opens the task's");
        orchestrator
            .tick()
            .await
            .expect("the tick opens the change's");
        assert_eq!(creates(&harness), 2);
        let root = std::fs::canonicalize(&harness.project.repo.path).expect("the root");
        let cwds = harness.gh.cwds();
        assert!(
            !cwds.is_empty() && cwds.iter().all(|cwd| *cwd == root),
            "{cwds:?}"
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    /// `app`: an npm project that runs its tests, committed.
    fn an_npm_app(app: &TempRepo) {
        app.write("package-lock.json", "{\"lockfileVersion\":3}\n");
        app.write(
            "package.json",
            "{\"name\":\"app\",\"scripts\":{\"test\":\"vitest\"}}\n",
        );
        app.write("src/index.ts", "export const one = 1;\n");
        app.commit("an app");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn rescans_the_member_it_integrated_into() {
        let (harness, members) = in_members("int-member-rescan", "auto_merge");
        let (app, infra) = (&members[0], &members[1]);
        an_npm_app(app);
        infra.write("Cargo.lock", "version = 4\n");
        infra.write("Cargo.toml", "[package]\nname = \"infra\"\n");
        infra.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
        infra.commit("an infra");
        let deps = &harness.project.deps;
        catervas_store::rescan_members(
            &deps.files,
            &deps.log,
            &deps.ids,
            deps.clock.now(),
            None,
            "human",
        )
        .expect("the members are scanned");
        deps.projections
            .catch_up()
            .expect("the board is brought up");
        let before = deps.files.read_criteria().expect("the library reads");
        let names_before: Vec<&str> = before.criteria.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names_before.contains(&"app-the-tests-pass"),
            "{names_before:?}"
        );
        assert!(!names_before.contains(&"app-the-linter-is-clean"));

        // The task adds a lint script to app's package.json.
        harness.accepted_in("CTV-1", &app.path, Some("app"));
        let worktree = harness.worktree("CTV-1");
        std::fs::write(
            worktree.join("package.json"),
            "{\"name\":\"app\",\"scripts\":{\"test\":\"vitest\",\"lint\":\"eslint .\"}}\n",
        )
        .expect("package.json is written");
        app.adapter()
            .commit(
                &worktree,
                "Add a lint script",
                &["package.json".to_string()],
            )
            .expect("the change is committed");
        app.adapter()
            .remove_worktree(&worktree)
            .expect("the worktree is removed");
        app.git(&["merge", "--no-ff", "-m", "By hand", "feature/CTV-1"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let merged = orchestrator.integrate(&task("CTV-1")).await;

        assert!(
            matches!(
                &merged,
                Ok(IntegrationOutcome::Merged {
                    scan: ScanRefresh::Refreshed,
                    ..
                })
            ),
            "{merged:?}"
        );
        let after = deps.files.read_criteria().expect("the library reads");
        let linter = after
            .criteria
            .iter()
            .find(|one| one.name.as_str() == "app-the-linter-is-clean")
            .expect("the new check is in the library");
        assert_eq!(linter.repo.as_ref().map(|repo| repo.as_str()), Some("app"));
        let of_infra = |library: &catervas_core::criteria::CriteriaLibrary| -> Vec<_> {
            library
                .criteria
                .iter()
                .filter(|one| one.name.as_str().starts_with("infra-"))
                .cloned()
                .collect()
        };
        assert!(!of_infra(&before).is_empty());
        assert_eq!(of_infra(&after), of_infra(&before));
        let updated = harness.events(&[EventKind::CriteriaUpdated]);
        assert!(
            matches!(&updated.last().expect("an update").body,
                EventBody::CriteriaUpdated(body) if body.updated_by == "governor"),
            "{updated:?}"
        );
        assert_eq!(harness.events(&[EventKind::ProjectScanned]).len(), 2);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn skips_the_rescan_when_the_members_checkout_is_elsewhere() {
        let (harness, members) = in_members("int-member-rescan-elsewhere", "auto_merge");
        let app = &members[0];
        // The branch to integrate into is named, so the merge does not follow the checkout.
        harness
            .project
            .deps
            .files
            .write_repos(
                &catervas_core::repos::validate_repos(&json!({ "repos": [
                    { "name": "app", "path": app_canonical(app).display().to_string(),
                      "integration_branch": "main" },
                    { "name": "infra", "path": members[1].path.display().to_string() },
                ] }))
                .expect("repos"),
            )
            .expect("repos.yaml is written");
        accepted_in_app(&harness, app, "CTV-1");
        app.git(&["merge", "--no-ff", "-m", "By hand", "feature/CTV-1"]);
        app.git(&["checkout", "-b", "topic"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let merged = orchestrator.integrate(&task("CTV-1")).await;

        assert_eq!(
            merged,
            Ok(IntegrationOutcome::Merged {
                sha: app.git_output(&["rev-parse", "main"]),
                scan: ScanRefresh::Skipped {
                    why: "app's checkout is on topic, not main".to_string()
                }
            })
        );
        assert!(harness.events(&[EventKind::ProjectScanned]).is_empty());
    }

    /// `id` filed in `app`, `in_progress` with its worktree made by `app`, then cancelled.
    fn cancelled_in_app(harness: &Harness, app: &TempRepo, id: &str) {
        harness.assigned_in(id, "dev-a", "dev-b", "app");
        harness.project.moved(
            id,
            "assigned",
            "in_progress",
            &json!({ "actor": "assignee", "requested_by": "dev-a", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        app.adapter()
            .create_worktree(&harness.worktree(id), &harness.branch(id), "main")
            .expect("app makes the worktree");
        harness.project.moved(
            id,
            "in_progress",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
    }

    /// What `git worktree list` says in `app`.
    fn worktrees_of(app: &TempRepo) -> String {
        app.git_output(&["worktree", "list", "--porcelain"])
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn removes_a_member_tasks_worktrees_with_its_git() {
        let (harness, members) = in_members("int-member-cleanup", "pull_request");
        let app = &members[0];
        harness.accepted_in("CTV-1", &app.path, Some("app"));
        let base = harness.worktree("CTV-1-base");
        app.adapter()
            .create_detached_worktree(&base, "main")
            .expect("the base worktree is made");
        app.git(&["merge", "--no-ff", "-m", "By hand", "feature/CTV-1"]);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator
            .integrate(&task("CTV-1"))
            .await
            .expect("the human integrates it");

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        assert!(!harness.worktree("CTV-1").exists());
        assert!(!base.exists());
        let listed = worktrees_of(app);
        assert!(!listed.contains("CTV-1"), "{listed}");
        // The branch is kept, and nothing registered holds it.
        app.git(&["checkout", "feature/CTV-1"]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn removes_the_folders_of_a_task_whose_repository_is_gone() {
        let (harness, members) = in_members("int-member-cleanup-gone", "manual");
        let app = &members[0];
        cancelled_in_app(&harness, app, "CTV-1");
        cancelled_in_app(&harness, app, "CTV-2");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        // The member is not where repos.yaml says: its folders go as plain folders.
        let moved = std::path::PathBuf::from(format!("{}-moved", app.path.display()));
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&app.path, &moved).expect("the member is moved away");
        let report = orchestrator
            .tick_within(&crate::orchestrator::TickScope {
                task_id: Some(task("CTV-2")),
                rules: crate::orchestrator::TickRules::All,
            })
            .await
            .expect("the tick runs");
        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-2"),
            "{report:?}"
        );
        assert!(!harness.worktree("CTV-2").exists());
        std::fs::rename(&moved, &app.path).expect("the member is moved back");

        // The member is not one any more.
        let removed = crate::daemon::fixtures::answered(
            &harness.daemon,
            "repo.remove",
            &json!({ "name": "app" }),
        )
        .await;
        assert!(removed.get("error").is_none(), "{removed}");
        let report = orchestrator.tick().await.expect("the tick runs");
        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        assert!(!harness.worktree("CTV-1").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn removes_the_folders_of_a_moved_and_located_member() {
        let (harness, members) = in_members("int-member-cleanup-moved", "manual");
        let app = &members[0];
        cancelled_in_app(&harness, app, "CTV-1");
        let moved = std::path::PathBuf::from(format!("{}-moved", app.path.display()));
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&app.path, &moved).expect("the member is moved away");
        // Located again, as the file says, without the worktree being told where its repository is.
        harness
            .project
            .deps
            .files
            .write_repos(
                &catervas_core::repos::validate_repos(&json!({ "repos": [
                    { "name": "app", "path": moved.display().to_string() },
                    { "name": "infra", "path": members[1].path.display().to_string() },
                ] }))
                .expect("repos"),
            )
            .expect("repos.yaml is written");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.tick().await.expect("the tick runs");

        assert!(
            matches!(&report, TickReport::Acted { task_id, .. } if task_id.as_str() == "CTV-1"),
            "{report:?}"
        );
        assert!(!harness.worktree("CTV-1").exists());
        let listed = git_output_in(&moved, &["worktree", "list", "--porcelain"]);
        assert!(!listed.contains("CTV-1"), "{listed}");
        let _ = std::fs::remove_dir_all(&moved);
    }
}
