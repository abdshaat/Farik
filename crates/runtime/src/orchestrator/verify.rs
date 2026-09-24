//! Rule 5 of the order, a task `verifying` (`docs/SPEC.md` 5.4): Farik runs the contract's
//! `command`, `test`, and `artifact` criteria itself, as the reviewer, in the task's sandbox; the
//! reviewer's fresh session answers the `review` criteria and writes the review note; a failed
//! review is filed as a rejection in the reviewer's name, with its note; a passed one goes to the
//! Product Manager's session, which accepts it, unless the human has to.

use std::path::PathBuf;
use std::sync::Arc;

use farik_core::branch::task_branch;
use farik_core::contract::{ExitCriterion, TaskContract, TaskId, TaskStatus, wire_method};
use farik_core::governor::done::{CriterionResult, RunBy, requires_human_acceptance};
use farik_core::governor::gates::Rejection;
use farik_core::governor::transition::{TransitionContext, TransitionRequest};
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_protocol::event::{
    CriterionRecordedBody, CriterionRecordedBodyRunBy, EventBody, EventIds, FarikEvent,
    NoteWrittenBodyKind, ReviewRecordedBody, new_event,
};
use farik_store::{EventQuery, Git, TaskProjection};

use super::messages::{ReviewBrief, accept_message, review_message};
use super::requests;
use super::rules::{acted, active, spent};
use super::session::{SessionAsk, run_session};
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, worktree};
use crate::criteria::{CriterionError, CriterionOutcome, NewTestsInput, run_criteria};
use crate::exec::ExecError;
use crate::session::SessionPurpose;
use crate::tools::ToolDeps;
use crate::transitions::{
    TransitionAsk, TransitionError, TransitionOutcome, integration_branch, last_move_into,
    refusal_details,
};

/// Who records the criteria Farik runs for the reviewer: Farik ran them, as `requested_by:
/// "governor"` names the governor's own moves.
pub(super) const GOVERNOR: &str = "governor";

/// Rule 5: a task `verifying`, with no refusal since it last moved there. In order: the criteria
/// Farik runs that it has not run in this verification, one at a time, each recorded before the
/// next runs; then, judged on the governor's own context for `verifying -> accepted`, the
/// reviewer's session when there is no review note, the rejection when the reviewer's results hold
/// a failure, the reviewer's session again when a criterion is unanswered, and the Product
/// Manager's session when every criterion passed and the human need not accept. An epic, which
/// has no branch or worktree of its own, goes to `verifying_epic` whoever reviews it.
pub(super) async fn verifying(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    if history.iter().any(|event| {
        event.envelope.seq > since && matches!(event.body, EventBody::TransitionRefused(_))
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
            day_spent,
        )
        .await;
    }
    let Some(reviewer) = active(team, row.reviewer_id.as_deref()) else {
        return Ok(None);
    };
    let ran = match run_what_farik_runs(orchestrator, team, &contract, &history, since).await? {
        FarikRan::Criteria(ran) => ran,
        FarikRan::Unrunnable(why) => return escalate(deps, team, row, &why),
    };
    let context = context(deps, team, &row.task_id)?;
    let answers = reviewer_results(&context);
    let Some(review_note) = context.done.review_note.clone() else {
        return review(orchestrator, team, row, reviewer, &[], day_spent, ran).await;
    };
    let failed = failed(&contract, &answers);
    if !failed.is_empty() {
        return reject(deps, team, row, reviewer, &failed, review_note).map(Some);
    }
    let unanswered = unanswered(&contract, &answers);
    if !unanswered.is_empty() {
        return review(
            orchestrator,
            team,
            row,
            reviewer,
            &unanswered,
            day_spent,
            ran,
        )
        .await;
    }
    // Only the human's acceptance satisfies a `high` risk task or a `human` criterion: until it is
    // given, a session would ask for a move the Definition of Done refuses.
    if (requires_human_acceptance(&contract) || contract.exit_criteria.iter().any(is_human))
        && !context.done.human_accepted
    {
        return Ok(ran_criteria(row, ran));
    }
    accept(
        orchestrator,
        team,
        row,
        &review_note,
        &answers,
        day_spent,
        ran,
    )
    .await
}

/// What Farik's runs for the reviewer came to.
enum FarikRan {
    /// This many criteria were run and recorded.
    Criteria(usize),
    /// One could not be run, for a reason that is not the work's, in these words.
    Unrunnable(String),
}

/// Runs, one at a time, each `command`, `test`, or `artifact` criterion with no governor's result
/// since the task last moved into `verifying`, with the base-branch check, and records each result
/// as the reviewer's run before the next one runs, so that a run killed half way is taken up at the
/// first criterion it did not record. The first runs in a sandbox made for them rather than the
/// assignee's, so that nothing the assignee's commands left outside the worktree reaches them (5.4
/// item 1). A criterion whose container went is recorded failed, with the error as its evidence,
/// and the next runs in a new sandbox; any other error stops the runs, to be escalated.
async fn run_what_farik_runs(
    orchestrator: &Orchestrator,
    team: &Team,
    contract: &TaskContract,
    history: &[FarikEvent],
    since: u64,
) -> Result<FarikRan, OrchestratorError> {
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
        return Ok(FarikRan::Criteria(0));
    }
    orchestrator.forget_sandbox(&contract.id);
    let base = integration_branch(team, &deps.tools.git)?;
    for criterion in &pending {
        let mut alone = contract.clone();
        alone.exit_criteria = vec![(*criterion).clone()];
        let sandbox = orchestrator.sandbox_for(&contract.id, team)?;
        let factory = Arc::clone(&deps.sandboxes);
        let root = deps.tools.git.root().to_path_buf();
        let project_id = deps.tools.ids.project_id.clone();
        let base = base.clone();
        let outcomes = tokio::task::spawn_blocking(move || {
            let git = Git::open(root);
            let head = task_branch(&alone);
            let input = NewTestsInput {
                git: &git,
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
                return Ok(FarikRan::Unrunnable(format!(
                    "Farik could not run {} for the reviewer: {error}",
                    criterion.id.as_str()
                )));
            }
        };
        for result in results {
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
        }
    }
    Ok(FarikRan::Criteria(pending.len()))
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

/// Whether a criterion Farik could not run is recorded failed rather than escalated: only when the
/// task's container went, which the work's own commands can cause and a new sandbox answers.
/// Git, the base-branch sandbox, a file written into the base worktree, or a command that would not
/// start are Farik's to fix, not the assignee's, so a failure would send the task back to someone
/// who cannot fix it.
pub(super) fn fails_the_criterion(error: &CriterionError) -> bool {
    matches!(error, CriterionError::Exec(ExecError::ContainerGone))
}

/// Escalates the task as the governor, in the words of what Farik could not run; a refusal passes
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

/// The reviewer's `verify` session, in the task's worktree with the read tier's built-ins and no
/// executor, told what Farik found and, when `unanswered` names any, which criteria it still has
/// to answer; then `review.recorded` when the review is complete.
async fn review(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    reviewer: &Agent,
    unanswered: &[String],
    day_spent: &mut bool,
    ran: usize,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if spent(deps, team, &contract, day_spent)? {
        return Ok(ran_criteria(row, ran));
    }
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    let context = context(deps, team, &row.task_id)?;
    let git = &deps.tools.git;
    let diff = git.diff(&integration_branch(team, git)?, &task_branch(&contract))?;
    let initial_prompt = review_message(&ReviewBrief {
        contract: &contract,
        results: &governor_results(&history, since),
        completion_note: context.done.completion_note.as_deref(),
        diff: &diff,
        unanswered,
    });
    let end = run_session(
        deps,
        team,
        read_only(
            &contract,
            reviewer,
            worktree(deps, &row.task_id),
            initial_prompt,
        ),
    )
    .await?;
    record_review(deps, team, &contract, reviewer, &end.session_id, since)?;
    Ok(Some(acted(row, reviewer, "verify", &end)))
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
/// note as the reasons, from the session that wrote the note (5.4: Farik files it).
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
    day_spent: &mut bool,
    ran: usize,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    let Some(product_manager) = requests::product_manager(team) else {
        return Ok(ran_criteria(row, ran));
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if spent(deps, team, &contract, day_spent)? {
        return Ok(ran_criteria(row, ran));
    }
    let initial_prompt = accept_message(&contract, review_note, answers);
    let end = run_session(
        deps,
        team,
        read_only(
            &contract,
            product_manager,
            worktree(deps, &row.task_id),
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

/// The results Farik recorded as the governor since `since`, the latest per criterion. Farik's own
/// carry no agent on their envelope; `recorded_by` alone would trust an agent the team named
/// `governor`.
pub(super) fn governor_results(history: &[FarikEvent], since: u64) -> Vec<CriterionResult> {
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

/// A `command`, `test`, or `artifact` criterion: one Farik runs itself.
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
) -> Result<Vec<FarikEvent>, OrchestratorError> {
    Ok(deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        ..EventQuery::default()
    })?)
}

/// The sequence number of the task's last move into `verifying`, or 0.
pub(super) fn since_verifying(history: &[FarikEvent]) -> u64 {
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
    use farik_store::GitError;

    use super::fails_the_criterion;
    use crate::criteria::CriterionError;
    use crate::exec::ExecError;
    use crate::sandbox::SandboxError;

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
