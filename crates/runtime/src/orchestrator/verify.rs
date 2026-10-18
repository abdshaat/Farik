//! Rule 5 of the order, a task `verifying` (`docs/SPEC.md` 5.4): Catervas runs the contract's
//! `command`, `test`, and `artifact` criteria itself, as the reviewer, in the task's sandbox; the
//! reviewer's fresh session answers the `review` criteria and writes the review note; a failed
//! review is filed as a rejection in the reviewer's name, with its note; a passed one goes to the
//! Product Manager's session, which accepts it, unless the human has to.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas_core::branch::task_branch;
use catervas_core::contract::{ExitCriterion, TaskContract, TaskId, TaskStatus, wire_method};
use catervas_core::governor::done::{CriterionResult, RunBy, catervas_verifies_alone};
use catervas_core::governor::gates::Rejection;
use catervas_core::governor::team_rules::is_ui_change;
use catervas_core::governor::transition::{TransitionContext, TransitionRequest};
use catervas_core::governor::transition_table::TransitionActor;
use catervas_core::team::{Agent, Team, task_private_folder};
use catervas_protocol::event::{
    CatervasEvent, CriterionRecordedBody, CriterionRecordedBodyRunBy, EventBody, EventIds,
    NoteWrittenBody, NoteWrittenBodyKind, ReviewRecordedBody, new_event,
};
use catervas_store::baseline::{FolderChangeKind, baseline_of, changes_since_baseline, folder_in};
use catervas_store::git::task_base;
use catervas_store::repos::TaskRepositoryError;
use catervas_store::{EventQuery, Git, TaskProjection};

use super::design::DESIGN_REVIEW_TOOLS;
use super::messages::{
    Changes, ReviewBrief, accept_message, design_review_message, review_message,
};
use super::requests;
use super::rules::{Waiting, acted, active, spent, unavailable};
use super::session::{SessionAsk, run_session};
use super::{
    Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, repository_refused, session_dir,
    task_repository_of, worktree,
};
use crate::criteria::{
    CriterionError, CriterionOutcome, NewTestsInput, check_artifact_in, run_criteria,
};
use crate::exec::ExecError;
use crate::preview::designer_browser;
use crate::session::SessionPurpose;
use crate::tools::ToolDeps;
use crate::tools::design::{ReviewState, design_review};
use crate::transitions::{
    TransitionAsk, TransitionError, TransitionOutcome, last_move_into, refusal_details,
};

/// Who records the criteria Catervas runs for the reviewer: Catervas ran them, as `requested_by:
/// "governor"` names the governor's own moves.
pub(super) const GOVERNOR: &str = "governor";

/// Rule 5: a task `verifying`, with no refusal since it last moved there. In order: the criteria
/// Catervas runs that it has not run in this verification, one at a time, each recorded before the
/// next runs; then, judged on the governor's own context for `verifying -> accepted`, the
/// reviewer's session when there is no review note, the rejection when the reviewer's results hold
/// a failure, the reviewer's session again when a criterion is unanswered, and the Product
/// Manager's session when every criterion passed and the human need not accept. An epic, which
/// has no branch or worktree of its own, goes to `verifying_epic` whoever reviews it.
pub(super) async fn verifying(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    // Only a refusal of the governor's own parks the task: an agent's refused request is told to
    // its reviewer's next session instead (ADR 0069).
    if history.iter().any(|event| {
        event.envelope.seq > since
            && matches!(&event.body, EventBody::TransitionRefused(body) if body.requested_by == GOVERNOR)
    }) {
        return Ok(None);
    }
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if requests::is_epic(row) {
        return requests::verifying_epic(
            orchestrator,
            team,
            row,
            &contract,
            &history,
            since,
            waiting,
        )
        .await;
    }
    if catervas_verifies_alone(&contract) {
        return verify_alone(orchestrator, team, row, &contract, waiting).await;
    }
    let Some(reviewer) = active(team, row.reviewer_id.as_deref()) else {
        return Ok(None);
    };
    let ran = match run_what_catervas_runs(orchestrator, team, &contract, &history, since).await? {
        CatervasRan::Criteria(ran) => ran,
        CatervasRan::Unrunnable(why) => return escalate(deps, team, row, &why),
        CatervasRan::MemberMissing => return Ok(None),
    };
    let context = context(deps, team, &row.task_id)?;
    if let Some(handled) = design_review_first(
        orchestrator,
        team,
        row,
        &contract,
        &context.done.changed_paths,
        &history,
        waiting,
        ran,
    )
    .await?
    {
        return Ok(handled);
    }
    let answers = reviewer_results(&context);
    let Some(review_note) = context.done.review_note.clone() else {
        return review(orchestrator, team, row, reviewer, &[], waiting, ran).await;
    };
    let failed = failed(&contract, &answers);
    if !failed.is_empty() {
        return reject(deps, team, row, reviewer, &failed, review_note).map(Some);
    }
    let unanswered = unanswered(&contract, &answers);
    if !unanswered.is_empty() {
        return review(orchestrator, team, row, reviewer, &unanswered, waiting, ran).await;
    }
    // Only the human's answer satisfies a `human` criterion: until it is given, a session would ask
    // for a move the Definition of Done refuses.
    if contract.exit_criteria.iter().any(is_human) && !context.done.human_accepted {
        return Ok(ran_criteria(row, ran));
    }
    accept(
        orchestrator,
        team,
        row,
        &review_note,
        &answers,
        waiting,
        ran,
    )
    .await
}

/// Rule 5 for a task no agent reviews, whose every criterion Catervas runs (ADR 0061): Catervas
/// runs them, writes the review note itself, and accepts or rejects as the governor, with no
/// reviewer's session and no Product Manager's.
pub(super) async fn verify_alone(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    contract: &TaskContract,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    if spent(deps, team, contract, &mut waiting.day_spent)? {
        return Ok(None);
    }
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    let ran = match run_what_catervas_runs(orchestrator, team, contract, &history, since).await? {
        CatervasRan::Criteria(ran) => ran,
        CatervasRan::Unrunnable(why) => return escalate(deps, team, row, &why),
        CatervasRan::MemberMissing => return Ok(None),
    };
    decide_alone(deps, team, row, contract, ran)
}

/// The end of `verify_alone`, and of an epic Catervas verifies alone once its criteria ran: while
/// a criterion has no result of Catervas's, nothing; then the review note Catervas writes from
/// them, once; then the governor's rejection for the criteria that failed, or its acceptance.
pub(super) fn decide_alone(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    contract: &TaskContract,
    ran: usize,
) -> Result<Option<TickReport>, OrchestratorError> {
    let history = history(deps, &row.task_id)?;
    let results = governor_results(&history, since_verifying(&history));
    if contract.exit_criteria.iter().any(|criterion| {
        !results
            .iter()
            .any(|result| result.criterion_id == criterion.id.as_str())
    }) {
        return Ok(ran_criteria(row, ran));
    }
    // The contract's order, whatever the order the runs were recorded in.
    let ordered: Vec<CriterionResult> = contract
        .exit_criteria
        .iter()
        .filter_map(|criterion| {
            results
                .iter()
                .find(|result| result.criterion_id == criterion.id.as_str())
                .cloned()
        })
        .collect();
    let note = catervas_note(&ordered);
    if context(deps, team, &row.task_id)?
        .done
        .review_note
        .is_none()
    {
        append(
            &deps.tools,
            &row.task_id,
            None,
            None,
            EventBody::NoteWritten(NoteWrittenBody {
                kind: NoteWrittenBodyKind::Review,
                text: note.clone(),
                written_by: "catervas".to_string(),
            }),
        )?;
    }
    let failed = failed(contract, &results);
    let rejecting = !failed.is_empty();
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: if rejecting {
                TaskStatus::Rejected
            } else {
                TaskStatus::Accepted
            },
            actor: TransitionActor::Governor,
            agent_id: None,
        },
        &TransitionAsk {
            rejection: rejecting.then(|| Rejection {
                failed_criterion_ids: failed.clone(),
                reasons: note,
            }),
            ..TransitionAsk::default()
        },
        team,
    )?;
    let what = match outcome {
        TransitionOutcome::Moved(_) if rejecting => {
            format!("rejected it for {}, which Catervas ran", failed.join(", "))
        }
        TransitionOutcome::Moved(_) => {
            "accepted it on its criteria, which Catervas ran: only code is reviewed".to_string()
        }
        // Nobody else can move a task Catervas verifies alone, so its refusal goes to the owner
        // rather than parking the task (ADR 0069).
        TransitionOutcome::Refused(refusal) => {
            return escalate(
                deps,
                team,
                row,
                &format!(
                    "Catervas could not {} it: {}",
                    if rejecting { "reject" } else { "accept" },
                    refusal_details(&refusal).join("; ")
                ),
            );
        }
    };
    Ok(Some(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    }))
}

/// The note Catervas writes for a task it verified alone, from its latest results: what it did,
/// then a line for each criterion, in the order given.
pub(super) fn catervas_note(results: &[CriterionResult]) -> String {
    let mut note = "Catervas ran every criterion of this task itself; only a code change is reviewed by an agent (ADR 0061).\n".to_string();
    for result in results {
        let _ = write!(
            note,
            "\n{}: {}: {}",
            result.criterion_id,
            if result.passed { "passed" } else { "failed" },
            result.evidence
        );
    }
    note
}

/// What Catervas's runs for the reviewer came to.
enum CatervasRan {
    /// This many criteria were run and recorded.
    Criteria(usize),
    /// One could not be run, for a reason that is not the work's, in these words.
    Unrunnable(String),
    /// The task's member is not where the project left it: nothing was run, and the task waits
    /// until the member is located.
    MemberMissing,
}

/// Runs, one at a time, each `command`, `test`, or `artifact` criterion with no governor's result
/// since the task last moved into `verifying`, with the base-branch check, and records each result
/// as the reviewer's run before the next one runs, so that a run killed half way is taken up at the
/// first criterion it did not record. The first runs in a sandbox made for them rather than the
/// assignee's, so that nothing the assignee's commands left outside the worktree reaches them (5.4
/// item 1). A criterion whose container went is recorded failed, with the error as its evidence,
/// and the next runs in a new sandbox; any other error stops the runs, to be escalated.
async fn run_what_catervas_runs(
    orchestrator: &Orchestrator,
    team: &Team,
    contract: &TaskContract,
    history: &[CatervasEvent],
    since: u64,
) -> Result<CatervasRan, OrchestratorError> {
    let deps = &orchestrator.deps;
    let pending: Vec<&ExitCriterion> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| is_mechanical(criterion))
        .filter(|criterion| {
            !governor_results(history, since)
                .iter()
                .any(|result| result.criterion_id == criterion.id.as_str())
        })
        .collect();
    if pending.is_empty() {
        return Ok(CatervasRan::Criteria(0));
    }
    // A task in a private folder has no worktree, branch or sandbox to run anything in (6.6).
    if let Some(folder) = task_private_folder(contract) {
        return run_in_the_folder(deps, contract, &pending, folder);
    }
    orchestrator.forget_sandbox(&contract.id);
    // The task's branch and its base are its repository's: the member's, in a project folder.
    let repository = match task_repository_of(deps, team, contract) {
        Ok(repository) => repository,
        Err(TaskRepositoryError::Missing { .. }) => return Ok(CatervasRan::MemberMissing),
        Err(error) => return Err(repository_refused(&contract.id, &error)),
    };
    let base = task_base(
        &repository.git,
        &repository.integration_branch,
        &contract.id,
    );
    for criterion in &pending {
        let mut alone = contract.clone();
        alone.exit_criteria = vec![(*criterion).clone()];
        let sandbox = orchestrator.sandbox_for(&contract.id, team)?;
        let factory = Arc::clone(&deps.sandboxes);
        let root = repository.git.root().to_path_buf();
        let project = deps.tools.git.root().to_path_buf();
        let project_id = deps.tools.ids.project_id.clone();
        let base = base.clone();
        let outcomes = tokio::task::spawn_blocking(move || {
            let git = Git::open(root);
            let base_worktree = project
                .join(".catervas/local/worktrees")
                .join(format!("{}-base", alone.id.as_str()));
            let head = task_branch(&alone);
            let input = NewTestsInput {
                git: &git,
                base_worktree: &base_worktree,
                base: &base,
                head: &head,
                sandboxes: factory.as_ref(),
                project_id: &project_id,
                task_id: &alone.id,
            };
            run_criteria(&alone, sandbox.as_ref(), RunBy::Reviewer, Some(&input))
        })
        .await
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()));
        let results = match outcomes {
            Ok(outcomes) => outcomes
                .into_iter()
                .filter_map(|outcome| match outcome {
                    CriterionOutcome::Result(result) => Some(result),
                    CriterionOutcome::NeedsReview { .. } | CriterionOutcome::NeedsHuman { .. } => {
                        None
                    }
                })
                .collect(),
            Err(error) if fails_the_criterion(&error) => {
                orchestrator.forget_sandbox(&contract.id);
                vec![CriterionResult {
                    criterion_id: criterion.id.to_string(),
                    passed: false,
                    evidence: error.to_string(),
                    run_by: RunBy::Reviewer,
                }]
            }
            Err(error) => {
                return Ok(CatervasRan::Unrunnable(format!(
                    "Catervas could not run {} for the reviewer: {error}",
                    criterion.id.as_str()
                )));
            }
        };
        for result in results {
            record_run(deps, contract, result)?;
        }
    }
    Ok(CatervasRan::Criteria(pending.len()))
}

/// Records `result` as the reviewer's run, which Catervas made.
fn record_run(
    deps: &OrchestratorDeps,
    contract: &TaskContract,
    result: CriterionResult,
) -> Result<(), OrchestratorError> {
    append(
        &deps.tools,
        &contract.id,
        None,
        None,
        EventBody::CriterionRecorded(CriterionRecordedBody {
            criterion_id: result.criterion_id,
            passed: result.passed,
            evidence: result.evidence,
            run_by: CriterionRecordedBodyRunBy::Reviewer,
            recorded_by: GOVERNOR.to_string(),
        }),
    )?;
    Ok(())
}

/// `run_what_catervas_runs` for a task in a private folder: each `artifact` criterion is checked on
/// the host, as a file in the folder (`check_artifact_in`), and recorded before the next. A
/// `command` or `test` criterion cannot be run for it, the folder being no worktree, and is not
/// the work's fault, so it is escalated (readiness refuses one, so it is a contract edited by
/// hand).
fn run_in_the_folder(
    deps: &OrchestratorDeps,
    contract: &TaskContract,
    pending: &[&ExitCriterion],
    folder: &str,
) -> Result<CatervasRan, OrchestratorError> {
    for criterion in pending {
        if wire_method(&criterion.verification) != Some("artifact") {
            return Ok(CatervasRan::Unrunnable(format!(
                "Catervas could not run {} for the reviewer: a task in a private folder has no \
                 worktree to run a command or a test in",
                criterion.id.as_str()
            )));
        }
        if let CriterionOutcome::Result(result) =
            check_artifact_in(deps.tools.files.root(), folder, criterion, RunBy::Reviewer)
        {
            record_run(deps, contract, result)?;
        }
    }
    Ok(CatervasRan::Criteria(pending.len()))
}

/// The contract's criteria, in its order, that one of `answers` failed.
pub(super) fn failed(contract: &TaskContract, answers: &[CriterionResult]) -> Vec<String> {
    contract
        .exit_criteria
        .iter()
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            answers
                .iter()
                .any(|result| result.criterion_id == *id && !result.passed)
        })
        .collect()
}

/// Whether a criterion Catervas could not run is recorded failed rather than escalated: only when the
/// task's container went, which the work's own commands can cause and a new sandbox answers.
/// Git, the base-branch sandbox, a file written into the base worktree, or a command that would not
/// start are Catervas's to fix, not the assignee's, so a failure would send the task back to someone
/// who cannot fix it.
pub(super) fn fails_the_criterion(error: &CriterionError) -> bool {
    matches!(error, CriterionError::Exec(ExecError::ContainerGone))
}

/// Escalates the task as the governor, in the words of what Catervas could not run; a refusal passes
/// the task over, and rule 5 does not ask again until it moves.
pub(super) fn escalate(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    why: &str,
) -> Result<Option<TickReport>, OrchestratorError> {
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Escalated,
            actor: TransitionActor::Governor,
            agent_id: None,
        },
        &TransitionAsk {
            criterion_unrunnable: Some(why.to_string()),
            ..TransitionAsk::default()
        },
        team,
    )?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("escalated it: {why}"),
        }),
        TransitionOutcome::Refused(_) => None,
    })
}

/// Step 12's design review, before the reviewer's session: for a Software Developer's UI change on
/// a team with a UI/UX Designer, the task waits for the team's preview, the Designer's browser, and
/// a Designer that is not paused; the change the Designer failed since the task last entered
/// `verifying` is rejected in its name, with its reasons; and with no review since then, the
/// Designer's read-only session reviews it. Nothing, when the reviewer's turn has come.
#[allow(
    clippy::too_many_arguments,
    reason = "rule 5's own state, handed on as it stands"
)]
async fn design_review_first(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    contract: &TaskContract,
    changed_paths: &[String],
    history: &[CatervasEvent],
    waiting: &mut Waiting,
    ran: usize,
) -> Result<Option<Option<TickReport>>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let ui_change = is_ui_change(
        contract,
        contract.assignee_role,
        changed_paths,
        &team.rules().ui_paths,
    );
    let review = design_review(team, ui_change, history, || {
        designer_browser(team, deps.previews.as_ref())
    });
    let designer = match (review.state, &review.recorded_by) {
        (ReviewState::NotNeeded | ReviewState::Passed, _) => return Ok(None),
        (ReviewState::Failed, Some((designer, session_id))) => {
            let reasons = review.reasons.unwrap_or_default();
            return reject_as_designer(deps, team, row, designer, session_id.clone(), reasons)
                .map(|report| Some(Some(report)));
        }
        (ReviewState::Waiting, _) => team.designer(),
        _ => None,
    };
    let Some(designer) = designer else {
        return Ok(Some(ran_criteria(row, ran)));
    };
    if spent(deps, team, contract, &mut waiting.day_spent)?
        | unavailable(deps, designer, &mut waiting.slept)?
    {
        return Ok(Some(ran_criteria(row, ran)));
    }
    // A member that is not where the project left it passes the task over until it is located.
    let repository = match task_repository_of(deps, team, contract) {
        Ok(repository) => repository,
        Err(TaskRepositoryError::Missing { .. }) => return Ok(Some(ran_criteria(row, ran))),
        Err(error) => return Err(repository_refused(&row.task_id, &error)),
    };
    let diff = repository.git.diff(
        &task_base(
            &repository.git,
            &repository.integration_branch,
            &contract.id,
        ),
        &task_branch(contract),
    )?;
    let end = run_session(
        deps,
        team,
        SessionAsk {
            tools: Some(DESIGN_REVIEW_TOOLS),
            ..read_only(
                contract,
                designer,
                worktree(deps, &row.task_id),
                design_review_message(contract, &diff),
            )
        },
    )
    .await?;
    Ok(Some(Some(acted(row, designer, "design review", &end))))
}

/// Files `verifying -> rejected` in the name of the Designer whose design review failed, with its
/// reasons, from the session that recorded it (F9): a design review fails no exit criterion.
fn reject_as_designer(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    designer: &str,
    session_id: Option<String>,
    reasons: String,
) -> Result<TickReport, OrchestratorError> {
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Rejected,
            actor: TransitionActor::Reviewer,
            agent_id: Some(designer.to_string()),
        },
        &TransitionAsk {
            rejection: Some(Rejection {
                failed_criterion_ids: Vec::new(),
                reasons,
            }),
            session_id,
            filed_by_catervas: true,
            ..TransitionAsk::default()
        },
        team,
    )?;
    let what = match outcome {
        TransitionOutcome::Moved(_) => {
            format!("rejected it, as {designer}'s design review says")
        }
        TransitionOutcome::Refused(refusal) => format!(
            "the governor would not reject it: {}",
            refusal_details(&refusal).join("; ")
        ),
    };
    Ok(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    })
}

/// The reviewer's `verify` session, in the task's worktree (or its private folder) with the read tier's built-ins and no
/// executor, told what Catervas found and, when `unanswered` names any, which criteria it still has
/// to answer; then `review.recorded` when the review is complete.
async fn review(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    reviewer: &Agent,
    unanswered: &[String],
    waiting: &mut Waiting,
    ran: usize,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | unavailable(deps, reviewer, &mut waiting.slept)?
    {
        return Ok(ran_criteria(row, ran));
    }
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    let context = context(deps, team, &row.task_id)?;
    let refused: Vec<String> = history
        .iter()
        .filter(|event| event.envelope.seq > since)
        .filter_map(|event| match &event.body {
            EventBody::TransitionRefused(body) if body.requested_by == reviewer.id.as_str() => {
                Some(body.details.join("; "))
            }
            _ => None,
        })
        .collect();
    // A task in a private folder has no branch to diff: its reviewer is told which files changed
    // in the folder, and reads each beside its copy from the start of the task (6.6).
    let (diff, files);
    let changes = if let Some(folder) = task_private_folder(&contract) {
        files = folder_changes(&folder_in(deps.tools.files.root(), folder)?, &contract.id)?;
        Changes::Folder(&files)
    } else {
        let repository = match task_repository_of(deps, team, &contract) {
            Ok(repository) => repository,
            // As in the design review: the task waits until its member is located.
            Err(TaskRepositoryError::Missing { .. }) => return Ok(ran_criteria(row, ran)),
            Err(error) => return Err(repository_refused(&row.task_id, &error)),
        };
        diff = repository.git.diff(
            &task_base(
                &repository.git,
                &repository.integration_branch,
                &contract.id,
            ),
            &task_branch(&contract),
        )?;
        Changes::Diff(&diff)
    };
    let initial_prompt = review_message(&ReviewBrief {
        contract: &contract,
        results: &governor_results(&history, since),
        completion_note: context.done.completion_note.as_deref(),
        changes,
        unanswered,
        refused: &refused,
    });
    let end = run_session(
        deps,
        team,
        read_only(
            &contract,
            reviewer,
            session_dir(deps, &contract)?,
            initial_prompt,
        ),
    )
    .await?;
    record_review(deps, team, &contract, reviewer, &end.session_id, since)?;
    Ok(Some(acted(row, reviewer, "verify", &end)))
}

/// One line for each file a task changed in its private `folder` since the copy taken for it:
/// its path, whether it is `new`, `changed` or `removed`, and its size, which for a removed file
/// is the copy's. Or one line saying nothing changed.
fn folder_changes(folder: &Path, task: &TaskId) -> Result<Vec<String>, OrchestratorError> {
    let size = |path: PathBuf| std::fs::metadata(path).map_or(0, |metadata| metadata.len());
    let lines: Vec<String> = changes_since_baseline(folder, task)?
        .into_iter()
        .map(|change| match change.kind {
            FolderChangeKind::Removed => format!(
                "{}: removed, it was {} bytes",
                change.path,
                size(baseline_of(folder, task).join(&change.path))
            ),
            kind => format!(
                "{}: {}, {} bytes",
                change.path,
                kind.word(),
                size(folder.join(&change.path))
            ),
        })
        .collect();
    Ok(if lines.is_empty() {
        vec!["no file in the folder differs from the copy".to_string()]
    } else {
        lines
    })
}

/// `review.recorded`, once per verification: when none was recorded since the task last moved into
/// `verifying` and every criterion but a `human` one has a reviewer's result.
pub(super) fn record_review(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    reviewer: &Agent,
    session_id: &str,
    since: u64,
) -> Result<(), OrchestratorError> {
    let recorded = history(deps, &contract.id)?.iter().any(|event| {
        event.envelope.seq > since && matches!(event.body, EventBody::ReviewRecorded(_))
    });
    let answers = reviewer_results(&context(deps, team, &contract.id)?);
    let complete = unanswered(contract, &answers).is_empty();
    if recorded || !complete {
        return Ok(());
    }
    let run: Vec<&CriterionResult> = answers
        .iter()
        .filter(|result| {
            contract
                .exit_criteria
                .iter()
                .any(|criterion| criterion.id.as_str() == result.criterion_id)
        })
        .collect();
    append(
        &deps.tools,
        &contract.id,
        Some(reviewer.id.to_string()),
        Some(session_id.to_string()),
        EventBody::ReviewRecorded(ReviewRecordedBody {
            reviewer: reviewer.id.to_string(),
            criteria_run: u32::try_from(run.len()).unwrap_or(u32::MAX),
            passed: run.iter().all(|result| result.passed),
        }),
    )
}

/// Files `verifying -> rejected` in the reviewer's name, with the failed criteria and the review
/// note as the reasons, from the session that wrote the note (5.4: Catervas files it).
pub(super) fn reject(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    reviewer: &Agent,
    failed: &[String],
    review_note: String,
) -> Result<TickReport, OrchestratorError> {
    let session_id = history(deps, &row.task_id)?
        .iter()
        .rev()
        .find(|event| {
            matches!(&event.body, EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Review)
        })
        .and_then(|event| event.envelope.ids.session_id.clone());
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Rejected,
            actor: TransitionActor::Reviewer,
            agent_id: Some(reviewer.id.to_string()),
        },
        &TransitionAsk {
            rejection: Some(Rejection {
                failed_criterion_ids: failed.to_vec(),
                reasons: review_note,
            }),
            session_id,
            filed_by_catervas: true,
            ..TransitionAsk::default()
        },
        team,
    )?;
    let what = match outcome {
        TransitionOutcome::Moved(_) => format!(
            "rejected it for {}, as its reviewer's note says",
            failed.join(", ")
        ),
        TransitionOutcome::Refused(refusal) => format!(
            "the governor would not reject it: {}",
            refusal_details(&refusal).join("; ")
        ),
    };
    Ok(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    })
}

/// The Product Manager's `verify` session, told the review passed and shown its note and results.
async fn accept(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    review_note: &str,
    answers: &[CriterionResult],
    waiting: &mut Waiting,
    ran: usize,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let Some(product_manager) = requests::product_manager(team) else {
        return Ok(ran_criteria(row, ran));
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if spent(deps, team, &contract, &mut waiting.day_spent)?
        | unavailable(deps, product_manager, &mut waiting.slept)?
    {
        return Ok(ran_criteria(row, ran));
    }
    let initial_prompt = accept_message(&contract, review_note, answers);
    let end = run_session(
        deps,
        team,
        read_only(
            &contract,
            product_manager,
            session_dir(deps, &contract)?,
            initial_prompt,
        ),
    )
    .await?;
    Ok(Some(acted(row, product_manager, "verify", &end)))
}

/// A `verify` session of `agent` in `cwd`, with the read tier's built-ins and no executor.
pub(super) fn read_only<'a>(
    contract: &'a TaskContract,
    agent: &'a Agent,
    cwd: PathBuf,
    initial_prompt: String,
) -> SessionAsk<'a> {
    SessionAsk {
        agent,
        contract: Some(contract),
        purpose: SessionPurpose::Verify,
        cwd,
        executor: None,
        read_only: true,
        only_tool: None,
        tools: None,
        in_reply_to: None,
        thread: None,
        initial_prompt,
    }
}

/// What a tick that started no session says: that it ran criteria, when it did.
pub(super) fn ran_criteria(row: &TaskProjection, ran: usize) -> Option<TickReport> {
    (ran > 0).then(|| TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!("ran {ran} of its criteria as its reviewer"),
    })
}

/// The governor's context for `verifying -> accepted`, so that every step judges on exactly what
/// the governor will.
pub(super) fn context(
    deps: &OrchestratorDeps,
    team: &Team,
    task_id: &TaskId,
) -> Result<TransitionContext, OrchestratorError> {
    Ok(deps.tools.transitions.context(
        &TransitionRequest {
            task_id: task_id.clone(),
            to: TaskStatus::Accepted,
            actor: TransitionActor::ProductManager,
            agent_id: None,
        },
        &TransitionAsk::default(),
        team,
    )?)
}

/// The reviewer's latest result per criterion in this iteration.
pub(super) fn reviewer_results(context: &TransitionContext) -> Vec<CriterionResult> {
    context
        .done
        .results
        .iter()
        .filter(|result| result.run_by == RunBy::Reviewer)
        .cloned()
        .collect()
}

/// The results Catervas recorded as the governor since `since`, the latest per criterion. Catervas's own
/// carry no agent on their envelope; `recorded_by` alone would trust an agent the team named
/// `governor`.
pub(super) fn governor_results(history: &[CatervasEvent], since: u64) -> Vec<CriterionResult> {
    let mut results: Vec<CriterionResult> = Vec::new();
    for event in history
        .iter()
        .filter(|event| event.envelope.seq > since && event.envelope.ids.agent_id.is_none())
    {
        if let EventBody::CriterionRecorded(body) = &event.body
            && body.recorded_by == GOVERNOR
        {
            results.retain(|result| result.criterion_id != body.criterion_id);
            results.push(CriterionResult {
                criterion_id: body.criterion_id.clone(),
                passed: body.passed,
                evidence: body.evidence.clone(),
                run_by: RunBy::Reviewer,
            });
        }
    }
    results
}

/// The ids of the criteria, but `human` ones, that `answers` holds no reviewer's result for.
pub(super) fn unanswered(contract: &TaskContract, answers: &[CriterionResult]) -> Vec<String> {
    contract
        .exit_criteria
        .iter()
        .filter(|criterion| !is_human(criterion))
        .map(|criterion| criterion.id.to_string())
        .filter(|id| !answers.iter().any(|result| result.criterion_id == *id))
        .collect()
}

pub(super) fn is_human(criterion: &ExitCriterion) -> bool {
    wire_method(&criterion.verification) == Some("human")
}

/// A `command`, `test`, or `artifact` criterion: one Catervas runs itself.
pub(super) fn is_mechanical(criterion: &ExitCriterion) -> bool {
    matches!(
        wire_method(&criterion.verification),
        Some("command" | "test" | "artifact")
    )
}

/// Every event about the task, oldest first.
pub(super) fn history(
    deps: &OrchestratorDeps,
    task_id: &TaskId,
) -> Result<Vec<CatervasEvent>, OrchestratorError> {
    Ok(deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        ..EventQuery::default()
    })?)
}

/// The sequence number of the task's last move into `verifying`, or 0.
pub(super) fn since_verifying(history: &[CatervasEvent]) -> u64 {
    last_move_into(history, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq)
}

/// Appends one event about the task, stamped with the agent and session when there are any, and
/// projects it.
pub(super) fn append(
    tools: &ToolDeps,
    task_id: &TaskId,
    agent_id: Option<String>,
    session_id: Option<String>,
    body: EventBody,
) -> Result<(), OrchestratorError> {
    let ids = EventIds {
        task_id: Some(task_id.clone()),
        agent_id,
        session_id,
        ..tools.ids.clone()
    };
    append_stamped(tools, ids, body)
}

/// Appends one event stamped with `ids`, and projects it.
pub(super) fn append_stamped(
    tools: &ToolDeps,
    ids: EventIds,
    body: EventBody,
) -> Result<(), OrchestratorError> {
    let event = new_event(body, tools.clock.now(), ids).map_err(|error| {
        OrchestratorError::Transition(TransitionError::Event {
            detail: format!("{error:?}"),
        })
    })?;
    let appended = tools.log.append(&event)?;
    tools.projections.apply(&appended)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use catervas_core::contract::TaskStatus;
    use catervas_core::governor::permissions::PermissionTier;
    use catervas_protocol::event::{
        EscalationRaisedBodyReason, EventBody, EventKind, NoteWrittenBodyKind,
    };
    use catervas_store::GitError;
    use catervas_store::git::fixtures::TempRepo;
    use serde_json::{Value, json};

    use super::fails_the_criterion;
    use crate::claude::allowed_builtins;
    use crate::criteria::CriterionError;
    use crate::exec::ExecError;
    use crate::orchestrator::fixtures::{ExecutorWitness, Harness};
    use crate::preview::fixtures::FakePreviews;
    use crate::recorded::Transcript;
    use crate::recorded::fixtures::{
        design_review_fails_ctv_2, design_review_passes_ctv_2, implement_css_ctv_2,
        replays_catervas_read_board, review_writes_note,
    };
    use crate::sandbox::SandboxError;
    use crate::session::{SessionPurpose, SessionSpec};
    use crate::tools::design::ReviewState;
    use crate::tools::fixtures::browsing;

    /// What the failing design review says.
    const TOO_FAINT: &str =
        "The heading is too faint to read in the dark theme at 360 px. Make it lighter there.";

    /// A harness on `team`, whose previews all start.
    fn a_harness(name: &str, team: impl FnOnce(&mut Value)) -> Harness {
        let mut harness = Harness::new(name, team);
        harness.previews = Arc::new(FakePreviews::ready());
        // Read before a tick too, so the governor's door is told before any orchestrator is made.
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::clone(&harness.previews));
        harness
    }

    /// CTV-2, the Software Developer `dev-a`'s task, reviewed by the Architect `ada`, in progress
    /// with its worktree made: its one allowed path and its criterion C1 are `path`.
    fn a_developers_task(harness: &Harness, path: &str) {
        harness.file("CTV-2", "ready", |wire| {
            wire["reviewer_role"] = json!("architect");
            wire["allowed_paths"] = json!([path]);
            wire["exit_criteria"][0]["text"] = json!(format!("{path} exists."));
            wire["exit_criteria"][0]["verification"]["command"] = json!(format!("test -f {path}"));
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "ada" });
        harness.project.moved("CTV-2", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-2", "assigned", "in_progress", &people);
        harness
            .project
            .deps
            .git
            .create_worktree(&harness.worktree("CTV-2"), &harness.branch("CTV-2"), "main")
            .expect("the task's worktree is made");
    }

    /// `a_developers_task`, with `path` committed on its branch, its completion note written, and
    /// moved to `verifying`.
    fn a_developers_change(harness: &Harness, path: &str) {
        a_developers_task(harness, path);
        let worktree = harness.worktree("CTV-2");
        let file = worktree.join(path);
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
        std::fs::write(&file, "h1 { color: #3b4a5c; }\n").expect("written");
        harness
            .project
            .deps
            .git
            .commit(&worktree, "The change", &[path.to_string()])
            .expect("committed");
        harness.project.record(
            "CTV-2",
            "note.written",
            &json!({ "kind": "completion", "text": "Changed it.\n\nChanged it.", "written_by": "dev-a" }),
        );
        let people = json!({ "assignee": "dev-a", "reviewer": "ada", "actor": "assignee", "requested_by": "dev-a" });
        harness
            .project
            .moved("CTV-2", "in_progress", "verifying", &people);
    }

    /// Ticks `count` times with `transcripts`; the sessions started, and the tiers each was held to.
    async fn ticked(
        harness: &Harness,
        transcripts: Vec<Transcript>,
        count: usize,
    ) -> (Vec<SessionSpec>, Vec<Vec<PermissionTier>>) {
        let adapter = harness.recorded(transcripts);
        let witness = Arc::new(ExecutorWitness::new(
            adapter.clone(),
            Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());
        for _ in 0..count {
            orchestrator.tick().await.expect("the tick runs");
        }
        (adapter.started(), witness.given_tiers())
    }

    fn who(started: &[SessionSpec]) -> Vec<(&str, SessionPurpose)> {
        started
            .iter()
            .map(|spec| (spec.agent_id.as_str(), spec.purpose))
            .collect()
    }

    /// The design review of CTV-2 as `task.get` reads it.
    fn state(harness: &Harness) -> ReviewState {
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        deps.transitions
            .design_review(&team, &"CTV-2".parse().expect("an id"))
            .expect("read")
            .1
            .state
    }

    /// A `transition.refused` of `verifying -> blocked` on CTV-2 that `by` asked for.
    fn refused_blocking(harness: &Harness, by: &str) {
        harness.project.record(
            "CTV-2",
            "transition.refused",
            &json!({
                "from": "verifying",
                "to": "blocked",
                "actor": "assignee",
                "requested_by": by,
                "refusal": "no_such_transition",
                "details": ["no such transition: from verifying to blocked"]
            }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reviews_again_after_the_reviewers_request_was_refused() {
        let harness = a_harness("review-after-refusal", browsing);
        a_developers_change(&harness, "done.txt");
        refused_blocking(&harness, "ada");
        let (started, _) = ticked(&harness, vec![review_writes_note()], 1).await;
        assert_eq!(who(&started), [("ada", SessionPurpose::Verify)]);
        let prompt = &started[0].initial_prompt;
        assert!(
            prompt.contains("Catervas refused your last request"),
            "{prompt}"
        );
        assert!(prompt.contains("no such transition"), "{prompt}");

        // The governor's own refusal still passes the task over.
        let harness = a_harness("review-after-governor-refusal", browsing);
        a_developers_change(&harness, "done.txt");
        refused_blocking(&harness, "governor");
        let (started, _) = ticked(&harness, vec![review_writes_note()], 2).await;
        assert!(started.is_empty(), "{:?}", who(&started));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn checks_a_ui_change_before_the_architect() {
        let harness = a_harness("design-review-first", browsing);
        a_developers_task(&harness, "site/style.css");
        let (started, tiers) = ticked(
            &harness,
            vec![
                implement_css_ctv_2(),
                design_review_passes_ctv_2(),
                review_writes_note(),
            ],
            3,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("dev-a", SessionPurpose::Implement),
                ("iris", SessionPurpose::Verify),
                ("ada", SessionPurpose::Verify),
            ]
        );
        let review = &started[1];
        assert_eq!(
            review.catervas_tools,
            [
                "catervas_read_task",
                "catervas_read_board",
                "catervas_read_rules",
                "catervas_read_criteria",
                "catervas_read_decisions",
                "catervas_check_page",
                "catervas_record_design_review",
            ]
        );
        assert_eq!(
            review.builtin_tools,
            allowed_builtins(&BTreeSet::from([
                PermissionTier::Read,
                PermissionTier::Network
            ]))
        );
        let servers: Vec<&str> = review.mcp_servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(servers, ["playwright"]);
        assert!(tiers[1].contains(&PermissionTier::Network), "{tiers:?}");
        assert!(
            review.initial_prompt.contains("site/style.css"),
            "the Designer is shown the change: {}",
            review.initial_prompt
        );
        assert!(
            review
                .initial_prompt
                .ends_with("The app's preview is at http://localhost:4401/."),
            "the design review is told where the preview is: {}",
            review.initial_prompt
        );
        // The Architect's review is not the Designer's: no browser, no page check.
        assert!(started[2].mcp_servers.is_empty());
        assert!(
            !started[2]
                .catervas_tools
                .iter()
                .any(|tool| tool.starts_with("catervas_check_page")
                    || tool == "catervas_record_design_review")
        );
        assert_eq!(harness.events(&[EventKind::PageChecked]).len(), 4);
        let recorded = harness.events(&[EventKind::DesignReviewRecorded]);
        assert_eq!(recorded.len(), 1);
        let EventBody::DesignReviewRecorded(body) = &recorded[0].body else {
            panic!("a design review");
        };
        assert!(body.pass);
        assert_eq!(body.checks.len(), 4);
        assert_eq!(harness.events(&[EventKind::ReviewRecorded]).len(), 1);
        assert_eq!(state(&harness), ReviewState::Passed);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_reviewer_sees_the_members_diff() {
        let (mut harness, members) =
            Harness::project_folder_with("design-review-member", &["app", "infra"], browsing);
        harness.previews = Arc::new(FakePreviews::ready());
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::clone(&harness.previews));
        harness.file("CTV-2", "ready", |wire| {
            wire["reviewer_role"] = json!("architect");
            wire["repo"] = json!("app");
            wire["allowed_paths"] = json!(["site/style.css"]);
            wire["exit_criteria"][0]["text"] = json!("site/style.css exists.");
            wire["exit_criteria"][0]["verification"]["command"] = json!("test -f site/style.css");
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "ada" });
        harness.project.moved("CTV-2", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-2", "assigned", "in_progress", &people);
        let worktree = harness.worktree("CTV-2");
        members[0]
            .adapter()
            .create_worktree(&worktree, &harness.branch("CTV-2"), "main")
            .expect("the member makes the task's worktree");
        std::fs::create_dir_all(worktree.join("site")).expect("made");
        std::fs::write(worktree.join("site/style.css"), "h1 { color: #3b4a5c; }\n")
            .expect("written");
        members[0]
            .adapter()
            .commit(&worktree, "The change", &["site/style.css".to_string()])
            .expect("committed");
        harness.project.record(
            "CTV-2",
            "note.written",
            &json!({ "kind": "completion", "text": "Changed it.\n\nChanged it.", "written_by": "dev-a" }),
        );
        harness.project.moved(
            "CTV-2",
            "in_progress",
            "verifying",
            &json!({ "assignee": "dev-a", "reviewer": "ada", "actor": "assignee", "requested_by": "dev-a" }),
        );

        let (started, _) = ticked(
            &harness,
            vec![design_review_passes_ctv_2(), review_writes_note()],
            2,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("iris", SessionPurpose::Verify),
                ("ada", SessionPurpose::Verify)
            ]
        );
        for spec in &started {
            assert!(
                spec.initial_prompt.contains("h1 { color: #3b4a5c; }"),
                "{}: {}",
                spec.agent_id,
                spec.initial_prompt
            );
        }
    }

    /// A project folder over the member `app`, whose `trunk` (its own integration branch, not the
    /// team's `main`) holds `flag.txt` as `broken` (and the test `tests/new.sh` when
    /// `test_on_trunk`), with CTV-1, a task of `app`, in `verifying`: its
    /// branch fixes the flag and adds the test, which a task of the member passes only if the
    /// flag is fixed. Its one criterion is the test command, as the member's `app-the-tests-pass`
    /// expands, requiring a new test.
    fn a_member_task_to_verify(name: &str, test_on_trunk: bool) -> (Harness, Vec<TempRepo>) {
        let (harness, members) = Harness::project_folder(name, &["app"]);
        let app = &members[0];
        app.write("flag.txt", "broken\n");
        app.write("tests/other.sh", "exit 0\n");
        if test_on_trunk {
            app.write("tests/new.sh", "[ \"$(cat flag.txt)\" = fixed ]\n");
        }
        app.commit("the flag and its tests");
        // The member's integration branch is its own, not the team's `main`.
        app.git(&["branch", "-m", "main", "trunk"]);
        harness.file("CTV-1", "ready", |wire| {
            wire["repo"] = json!("app");
            wire["allowed_paths"] = json!(["flag.txt", "tests/**"]);
            wire["exit_criteria"] = json!([{
                "id": "C1",
                "text": "The tests pass in app, and a new one shows the change.",
                "satisfies": ["R1"],
                "verification": {
                    "method": "test", "command": "sh tests/new.sh", "new_tests_required": true
                }
            }]);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        harness.project.moved("CTV-1", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-1", "assigned", "in_progress", &people);
        let worktree = harness.worktree("CTV-1");
        app.adapter()
            .create_worktree(&worktree, &harness.branch("CTV-1"), "trunk")
            .expect("the member makes the task's worktree");
        std::fs::write(worktree.join("flag.txt"), "fixed\n").expect("written");
        std::fs::create_dir_all(worktree.join("tests")).expect("made");
        std::fs::write(
            worktree.join("tests/new.sh"),
            "[ \"$(cat flag.txt)\" = fixed ]\n",
        )
        .expect("written");
        let paths = if test_on_trunk {
            vec!["flag.txt".to_string()]
        } else {
            vec!["flag.txt".to_string(), "tests/new.sh".to_string()]
        };
        app.adapter()
            .commit(&worktree, "Fix the flag", &paths)
            .expect("committed on the member's branch");
        harness.project.record(
            "CTV-1",
            "note.written",
            &json!({ "kind": "completion", "text": "Fixed it.\n\nFixed it.", "written_by": "dev-a" }),
        );
        harness.project.moved(
            "CTV-1",
            "in_progress",
            "verifying",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b", "actor": "assignee", "requested_by": "dev-a" }),
        );
        (harness, members)
    }

    fn recorded_run(harness: &Harness) -> (bool, String) {
        let events = harness.events(&[EventKind::CriterionRecorded]);
        let [event] = events.as_slice() else {
            panic!(
                "one criterion recorded, got {events:?}; escalations {:?}",
                harness.events(&[EventKind::EscalationRaised])
            );
        };
        let EventBody::CriterionRecorded(body) = &event.body else {
            panic!("a criterion.recorded");
        };
        assert_eq!(body.criterion_id, "C1");
        (body.passed, body.evidence.clone())
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_a_members_check_on_its_branch() {
        // The check runs at the member's root in the sandbox made for the task's worktree, and
        // `new_tests_required` is judged against the member's `trunk`, not the project folder's.
        let (harness, members) = a_member_task_to_verify("verify-member-check", false);
        let (started, _) = ticked(&harness, vec![review_writes_note()], 1).await;
        let (passed, evidence) = recorded_run(&harness);
        assert!(passed, "{evidence}");
        for line in [
            "adds tests: yes",
            "fails on base: yes",
            "test files: tests/new.sh",
        ] {
            assert!(
                evidence.lines().any(|one| one == line),
                "{line} in {evidence}"
            );
        }
        assert_eq!(who(&started), [("dev-b", SessionPurpose::Verify)]);
        assert!(
            !members[0].path.join(".catervas").exists(),
            "nothing of Catervas's goes into the member"
        );
        assert!(
            !harness
                .project
                .repo
                .path
                .join(".catervas/local/worktrees/CTV-1-base")
                .exists(),
            "and the base worktree is removed again"
        );

        // The same test already on the member's trunk is not a new one.
        let (already, _members) = a_member_task_to_verify("verify-member-check-old", true);
        ticked(&already, vec![review_writes_note()], 1).await;
        let (passed, evidence) = recorded_run(&already);
        assert!(!passed, "{evidence}");
        assert!(
            evidence.lines().any(|one| one == "adds tests: no"),
            "{evidence}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_missing_member_before_running_its_checks() {
        // A member that is not where the project left it passes the task over, as every other rule
        // that resolves its repository does: no run, no escalation, and the tick goes on.
        let (harness, members) = a_member_task_to_verify("verify-member-missing", false);
        let away = members[0].path.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&members[0].path, &away).expect("the member is moved away");

        let (started, _) = ticked(&harness, vec![review_writes_note()], 2).await;

        assert!(started.is_empty(), "{}", started.len());
        assert!(harness.events(&[EventKind::CriterionRecorded]).is_empty());
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        std::fs::rename(&away, &members[0].path).expect("the member is moved back");
        ticked(&harness, vec![review_writes_note()], 1).await;
        let (passed, evidence) = recorded_run(&harness);
        assert!(passed, "located again, its checks run: {evidence}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_a_failed_design_review_back_to_the_developer() {
        let harness = a_harness("design-review-fails", browsing);
        a_developers_task(&harness, "site/style.css");
        let (started, _) = ticked(
            &harness,
            vec![implement_css_ctv_2(), design_review_fails_ctv_2()],
            3,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("dev-a", SessionPurpose::Implement),
                ("iris", SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Rejected);
        let moves = harness.events(&[EventKind::TaskTransitioned]);
        let rejected = moves.last().expect("a move");
        let EventBody::TaskTransitioned(body) = &rejected.body else {
            panic!("a move");
        };
        assert_eq!(body.requested_by, "iris");
        let rejection = body.rejection.as_ref().expect("its reasons");
        assert_eq!(rejection.reasons, TOO_FAINT);
        assert_eq!(rejected.envelope.ids.agent_id.as_deref(), Some("iris"));
        assert!(harness.events(&[EventKind::ReviewRecorded]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn leaves_a_non_ui_change_to_the_architect() {
        let harness = a_harness("design-review-not-ui", browsing);
        a_developers_change(&harness, "done.txt");
        let (started, _) = ticked(&harness, vec![review_writes_note()], 1).await;

        assert_eq!(who(&started), [("ada", SessionPurpose::Verify)]);
        assert_eq!(state(&harness), ReviewState::NotNeeded);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reviews_alone_without_a_designer() {
        let harness = a_harness("design-review-no-designer", |wire| {
            browsing(wire);
            wire["agents"][3]["status"] = json!("retired");
        });
        a_developers_change(&harness, "site/style.css");
        let (started, _) = ticked(&harness, vec![review_writes_note()], 1).await;

        assert_eq!(who(&started), [("ada", SessionPurpose::Verify)]);
        assert_eq!(state(&harness), ReviewState::NotNeeded);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_on_a_paused_designer() {
        let harness = a_harness("design-review-paused", |wire| {
            browsing(wire);
            wire["agents"][3]["status"] = json!("paused");
        });
        a_developers_change(&harness, "site/style.css");
        let (started, _) = ticked(&harness, Vec::new(), 2).await;

        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Verifying);
        assert_eq!(state(&harness), ReviewState::WaitingOnDesigner);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_missing_preview() {
        let harness = a_harness("design-review-no-preview", |wire| {
            browsing(wire);
            wire.as_object_mut().expect("a team").remove("preview");
        });
        a_developers_change(&harness, "site/style.css");
        let (started, _) = ticked(&harness, Vec::new(), 2).await;

        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Verifying);
        assert_eq!(state(&harness), ReviewState::PreviewMissing);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_the_designers_sandbox() {
        // `Harness::new` runs no previews, as in no-sandbox mode.
        let harness = Harness::new("design-review-no-sandbox", browsing);
        a_developers_change(&harness, "site/style.css");
        let (started, _) = ticked(&harness, Vec::new(), 2).await;

        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Verifying);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn acts_on_a_recorded_review_before_any_wait() {
        // A recorded pass needs no live preview: the Architect reviews though it has gone since.
        let harness = a_harness("design-review-recorded-first", browsing);
        a_developers_change(&harness, "site/style.css");
        harness.project.record(
            "CTV-2",
            "design_review.recorded",
            &json!({ "pass": true, "reasons": "Fine.", "checks": [] }),
        );
        let files = &harness.project.deps.files;
        let mut team = files.read_team().expect("the team");
        team.preview = None;
        files.write_team(&team).expect("written");
        let (started, _) = ticked(&harness, vec![review_writes_note()], 1).await;

        assert_eq!(who(&started), [("ada", SessionPurpose::Verify)]);
        assert_eq!(state(&harness), ReviewState::Passed);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn counts_the_latest_of_two_recorded_reviews() {
        let harness = a_harness("design-review-latest", browsing);
        a_developers_change(&harness, "site/style.css");
        for pass in [false, true] {
            harness.project.record(
                "CTV-2",
                "design_review.recorded",
                &json!({ "pass": pass, "reasons": "Looked again.", "checks": [] }),
            );
        }
        assert_eq!(state(&harness), ReviewState::Passed);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_on_a_designer_without_its_browser() {
        // Playwright switched off for Iris: Catervas gives her no work, so no design review starts.
        let harness = a_harness("design-review-no-connector", |wire| {
            browsing(wire);
            wire["agents"][3]
                .as_object_mut()
                .expect("an agent")
                .remove("mcp_servers");
        });
        a_developers_change(&harness, "site/style.css");
        let (started, _) = ticked(&harness, Vec::new(), 2).await;

        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Verifying);
        assert_eq!(state(&harness), ReviewState::DesignerNeedsBrowser);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_the_design_review_again_without_an_answer() {
        let harness = a_harness("design-review-unanswered", |wire| {
            browsing(wire);
        });
        a_developers_change(&harness, "site/style.css");
        let mut contract = harness
            .project
            .deps
            .files
            .read_contract(&"CTV-2".parse().expect("an id"))
            .expect("the contract");
        contract.budget.max_sessions = std::num::NonZeroU64::new(2).expect("two");
        harness
            .project
            .deps
            .files
            .write_contract(&contract)
            .expect("written");
        let (started, _) = ticked(
            &harness,
            vec![replays_catervas_read_board(), replays_catervas_read_board()],
            3,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("iris", SessionPurpose::Verify),
                ("iris", SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Escalated);
        let reasons: Vec<EscalationRaisedBodyReason> = harness
            .events(&[EventKind::EscalationRaised])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason),
                _ => None,
            })
            .collect();
        assert_eq!(reasons, [EscalationRaisedBodyReason::Sessions]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_both_again_after_a_send_back() {
        let harness = a_harness("design-review-again", browsing);
        a_developers_change(&harness, "site/style.css");
        // The last round: the Designer passed it, and the Architect sent it back.
        harness.project.record(
            "CTV-2",
            "design_review.recorded",
            &json!({ "pass": true, "reasons": "Fine.", "checks": [] }),
        );
        let back = json!({ "actor": "reviewer", "requested_by": "ada", "assignee": "dev-a", "reviewer": "ada" });
        harness
            .project
            .moved("CTV-2", "verifying", "rejected", &back);
        harness
            .project
            .moved("CTV-2", "rejected", "in_progress", &back);
        let people = json!({ "actor": "assignee", "requested_by": "dev-a", "assignee": "dev-a", "reviewer": "ada" });
        harness
            .project
            .moved("CTV-2", "in_progress", "verifying", &people);
        assert_eq!(state(&harness), ReviewState::Waiting);

        let (started, _) = ticked(
            &harness,
            vec![design_review_passes_ctv_2(), review_writes_note()],
            2,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("iris", SessionPurpose::Verify),
                ("ada", SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.events(&[EventKind::DesignReviewRecorded]).len(), 2);
    }

    /// CTV-2, the Architect `ada`'s task, which the Product Manager `pm` is named to review, in
    /// `verifying`: its one criterion C1 is an `artifact` on `docs/catervas/architecture/overview.md`,
    /// committed on its branch when `committed`, with its completion note written.
    fn an_architects_task(harness: &Harness, committed: bool) {
        let path = "docs/catervas/architecture/overview.md";
        harness.file("CTV-2", "ready", |wire| {
            wire["assignee_role"] = json!("architect");
            wire["reviewer_role"] = json!("product_manager");
            wire["allowed_paths"] = json!([path]);
            wire["exit_criteria"] = json!([{
                "id": "C1",
                "text": format!("{path} exists."),
                "satisfies": ["R1"],
                "verification": { "method": "artifact", "path": path }
            }]);
        });
        let people = json!({ "assignee": "ada", "reviewer": "pm" });
        harness.project.moved("CTV-2", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-2", "assigned", "in_progress", &people);
        let git = &harness.project.deps.git;
        let worktree = harness.worktree("CTV-2");
        git.create_worktree(&worktree, &harness.branch("CTV-2"), "main")
            .expect("the task's worktree is made");
        if committed {
            let file = worktree.join(path);
            std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
            std::fs::write(&file, "# Overview\n").expect("written");
            git.commit(&worktree, "The overview", &[path.to_string()])
                .expect("committed");
        }
        harness.project.record(
            "CTV-2",
            "note.written",
            &json!({ "kind": "completion", "text": "Wrote the overview.\n\nWrote the overview.", "written_by": "ada" }),
        );
        let moved = json!({ "assignee": "ada", "reviewer": "pm", "actor": "assignee", "requested_by": "ada" });
        harness
            .project
            .moved("CTV-2", "in_progress", "verifying", &moved);
    }

    /// The moves of `task` out of `verifying` that the governor made, as `(from, to)`.
    fn governor_moves(harness: &Harness, task: &str) -> Vec<(String, String)> {
        harness
            .events(&[EventKind::TaskTransitioned])
            .iter()
            .filter(|event| event.envelope.ids.task_id.as_ref().map(|id| id.as_str()) == Some(task))
            .filter_map(|event| match &event.body {
                EventBody::TaskTransitioned(body)
                    if body.actor.to_string() == "governor"
                        && body.from.to_string() == "verifying"
                        && event.envelope.ids.agent_id.is_none() =>
                {
                    Some((body.from.to_string(), body.to.to_string()))
                }
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_what_the_governor_would_not_accept_alone() {
        let harness = a_harness("verify-alone-escalates", browsing);
        // A Developer's task that no agent reviews, flagged as a change to the interface on a team
        // with a Designer: the governor's acceptance waits for a design review it never gets.
        harness.file("CTV-3", "ready", |wire| {
            wire["assignee_role"] = json!("software_developer");
            wire["reviewer_role"] = json!("product_manager");
            wire["ui_change"] = json!(true);
            wire["allowed_paths"] = json!(["docs/catervas/engineering/bug-hunt.html"]);
            wire["exit_criteria"] = json!([{
                "id": "C1",
                "text": "The report exists.",
                "satisfies": ["R1"],
                "verification": { "method": "artifact", "path": "docs/catervas/engineering/bug-hunt.html" }
            }]);
        });
        let people = json!({ "assignee": "dev-a", "reviewer": "pm" });
        harness.project.moved("CTV-3", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-3", "assigned", "in_progress", &people);
        let git = &harness.project.deps.git;
        let worktree = harness.worktree("CTV-3");
        git.create_worktree(&worktree, &harness.branch("CTV-3"), "main")
            .expect("the task's worktree is made");
        let path = "docs/catervas/engineering/bug-hunt.html";
        std::fs::create_dir_all(worktree.join(path).parent().expect("a folder")).expect("made");
        std::fs::write(worktree.join(path), "# Report\n").expect("written");
        git.commit(&worktree, "The report", &[path.to_string()])
            .expect("committed");
        harness.project.record(
            "CTV-3",
            "note.written",
            &json!({ "kind": "completion", "text": "Wrote it.\n\nWrote it.", "written_by": "dev-a" }),
        );
        harness
            .project
            .moved("CTV-3", "in_progress", "verifying", &people);
        let (started, _) = ticked(&harness, Vec::new(), 3).await;
        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-3").status, TaskStatus::Escalated);
        let raised = harness.events(&[EventKind::EscalationRaised]);
        assert!(
            raised.iter().any(|event| matches!(&event.body,
                EventBody::EscalationRaised(body) if body.detail.contains("Catervas could not accept it"))),
            "{raised:?}"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catervas_verifies_a_task_no_agent_reviews() {
        let harness = a_harness("verify-alone-accepts", |_| {});
        an_architects_task(&harness, true);
        let (started, _) = ticked(&harness, Vec::new(), 2).await;

        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Accepted);
        let runs = harness.events(&[EventKind::CriterionRecorded]);
        assert!(matches!(&runs[0].body,
            EventBody::CriterionRecorded(body) if body.recorded_by == "governor" && body.passed));
        let notes = harness.events(&[EventKind::NoteWritten]);
        let review = notes
            .iter()
            .find_map(|event| match &event.body {
                EventBody::NoteWritten(body) if body.kind == NoteWrittenBodyKind::Review => {
                    Some((event, body))
                }
                _ => None,
            })
            .expect("Catervas wrote the review note");
        assert_eq!(review.1.written_by, "catervas");
        assert_eq!(review.0.envelope.ids.agent_id, None);
        assert_eq!(review.0.envelope.ids.session_id, None);
        assert!(review.1.text.contains("C1: passed"), "{}", review.1.text);
        assert_eq!(
            governor_moves(&harness, "CTV-2"),
            [("verifying".to_string(), "accepted".to_string())]
        );
        assert!(harness.events(&[EventKind::ReviewRecorded]).is_empty());

        // Nothing committed: the criterion fails, and Catervas sends the task back.
        let harness = a_harness("verify-alone-rejects", |_| {});
        an_architects_task(&harness, false);
        let (started, _) = ticked(&harness, Vec::new(), 1).await;
        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-2").status, TaskStatus::Rejected);
        assert_eq!(
            governor_moves(&harness, "CTV-2"),
            [("verifying".to_string(), "rejected".to_string())]
        );
        let runs = harness.events(&[EventKind::CriterionRecorded]);
        assert!(matches!(&runs[0].body,
            EventBody::CriterionRecorded(body) if body.criterion_id == "C1" && !body.passed));

        // A Finance Specialist's task, in its private folder, is accepted the same way.
        let harness = a_harness(
            "verify-alone-folder",
            crate::tools::fixtures::with_the_finance_specialist,
        );
        harness.finance_task("CTV-3", Some("verifying"));
        std::fs::create_dir_all(harness.finance_folder()).expect("the folder");
        std::fs::write(harness.finance_folder().join("books.xlsx"), "books").expect("written");
        harness.project.record(
            "CTV-3",
            "note.written",
            &json!({ "kind": "completion", "text": "Kept the books.\n\nKept the books.", "written_by": "fin" }),
        );
        let (started, _) = ticked(&harness, Vec::new(), 2).await;
        assert!(started.is_empty(), "{:?}", who(&started));
        assert_eq!(harness.row("CTV-3").status, TaskStatus::Accepted);
    }

    #[test]
    fn fails_a_criterion_only_when_its_container_went() {
        assert!(fails_the_criterion(&CriterionError::Exec(
            ExecError::ContainerGone
        )));
        for escalated in [
            CriterionError::Exec(ExecError::SpawnFailed {
                detail: "no shell".to_string(),
            }),
            CriterionError::Exec(ExecError::OutsideWorkspace {
                cwd: "/".to_string(),
            }),
            CriterionError::Git(GitError::NotARepository),
            CriterionError::Sandbox(SandboxError::DockerUnavailable),
            CriterionError::Io {
                path: "tests/a_test.sh".to_string(),
                detail: "denied".to_string(),
            },
        ] {
            assert!(!fails_the_criterion(&escalated), "{escalated:?}");
        }
    }
}
