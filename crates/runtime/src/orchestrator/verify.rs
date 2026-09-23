//! Rule 5 of the order, a task `verifying` (`docs/SPEC.md` 5.4): Farik runs the contract's
//! `command`, `test`, and `artifact` criteria itself, as the reviewer, in the task's sandbox; the
//! reviewer's fresh session answers the `review` criteria and writes the review note; a failed
//! review is filed as a rejection in the reviewer's name, with its note; a passed one goes to the
//! Product Manager's session, which accepts it, unless the human has to.

use std::path::PathBuf;
use std::sync::Arc;

use farik_core::contract::{ExitCriterion, Role, TaskContract, TaskId, TaskStatus, wire_method};
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
use super::rules::{Room, acted, active, room};
use super::session::{SessionAsk, run_session};
use super::{Orchestrator, OrchestratorDeps, OrchestratorError, TickReport, worktree};
use crate::criteria::{CriterionOutcome, NewTestsInput, run_criteria};
use crate::session::SessionPurpose;
use crate::transitions::{
    TransitionAsk, TransitionError, TransitionOutcome, integration_branch, refusal_details,
};

/// Who records the criteria Farik runs for the reviewer: Farik ran them, as `requested_by:
/// "governor"` names the governor's own moves.
const GOVERNOR: &str = "governor";

/// Rule 5: a task `verifying`, with no refusal since it last moved there. In order: the criteria
/// Farik runs that it has not run in this verification, one at a time, each recorded before the
/// next runs; then, judged on the governor's own context for `verifying -> accepted`, the
/// reviewer's session when there is no review note, the rejection when the reviewer's results hold
/// a failure, the reviewer's session again when a criterion is unanswered, and the Product
/// Manager's session when every criterion passed and the human need not accept.
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
    let Some(reviewer) = active(team, row.reviewer_id.as_deref()) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let ran = run_what_farik_runs(orchestrator, team, &contract, &history, since).await?;
    let context = context(deps, team, &row.task_id)?;
    let answers = reviewer_results(&context);
    let Some(review_note) = context.done.review_note.clone() else {
        return review(orchestrator, team, row, reviewer, &[], day_spent, ran).await;
    };
    let failed: Vec<String> = contract
        .exit_criteria
        .iter()
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            answers
                .iter()
                .any(|result| result.criterion_id == *id && !result.passed)
        })
        .collect();
    if !failed.is_empty() {
        return reject(deps, team, row, reviewer, &failed, review_note).map(Some);
    }
    let unanswered: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| !is_human(criterion))
        .map(|criterion| criterion.id.to_string())
        .filter(|id| !answers.iter().any(|result| result.criterion_id == *id))
        .collect();
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
    // Only the human's acceptance satisfies a `high` risk task or a `human` criterion, and it
    // arrives with step 14: a session now would ask for a move the Definition of Done refuses.
    if requires_human_acceptance(&contract) || contract.exit_criteria.iter().any(is_human) {
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

/// Runs, one at a time, each `command`, `test`, or `artifact` criterion with no governor's result
/// since the task last moved into `verifying`, in the task's sandbox with the base-branch check,
/// and records each result as the reviewer's run before the next one runs, so that a run killed
/// half way is taken up at the first criterion it did not record. Says how many it ran.
async fn run_what_farik_runs(
    orchestrator: &Orchestrator,
    team: &Team,
    contract: &TaskContract,
    history: &[FarikEvent],
    since: u64,
) -> Result<usize, OrchestratorError> {
    let deps = &orchestrator.deps;
    let pending: Vec<&ExitCriterion> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| {
            matches!(
                wire_method(&criterion.verification),
                Some("command" | "test" | "artifact")
            )
        })
        .filter(|criterion| {
            !governor_results(history, since)
                .iter()
                .any(|result| result.criterion_id == criterion.id.as_str())
        })
        .collect();
    if pending.is_empty() {
        return Ok(0);
    }
    let sandbox = orchestrator.sandbox_for(&contract.id, team)?;
    let base = integration_branch(team, &deps.tools.git)?;
    for criterion in &pending {
        let mut alone = contract.clone();
        alone.exit_criteria = vec![(*criterion).clone()];
        let sandbox = Arc::clone(&sandbox);
        let factory = Arc::clone(&deps.sandboxes);
        let root = deps.tools.git.root().to_path_buf();
        let project_id = deps.tools.ids.project_id.clone();
        let base = base.clone();
        let outcomes = tokio::task::spawn_blocking(move || {
            let git = Git::open(root);
            let head = format!("farik/{}", alone.id.as_str());
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
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))?;
        for outcome in outcomes {
            if let CriterionOutcome::Result(result) = outcome {
                append(
                    deps,
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
    }
    Ok(pending.len())
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
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(ran_criteria(row, ran));
    }
    let history = history(deps, &row.task_id)?;
    let since = since_verifying(&history);
    let context = context(deps, team, &row.task_id)?;
    let git = &deps.tools.git;
    let diff = git.diff(
        &integration_branch(team, git)?,
        &format!("farik/{}", row.task_id.as_str()),
    )?;
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
fn record_review(
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
    let complete = contract
        .exit_criteria
        .iter()
        .filter(|criterion| !is_human(criterion))
        .all(|criterion| {
            answers
                .iter()
                .any(|result| result.criterion_id == criterion.id.as_str())
        });
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
        deps,
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
fn reject(
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
    let Some(product_manager) = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
    else {
        return Ok(ran_criteria(row, ran));
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    if room(deps, team, &contract, day_spent)? != Room::Free {
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
fn read_only<'a>(
    contract: &'a TaskContract,
    agent: &'a Agent,
    cwd: PathBuf,
    initial_prompt: String,
) -> SessionAsk<'a> {
    SessionAsk {
        agent,
        contract,
        purpose: SessionPurpose::Verify,
        cwd,
        executor: None,
        read_only: true,
        initial_prompt,
    }
}

/// What a tick that started no session says: that it ran criteria, when it did.
fn ran_criteria(row: &TaskProjection, ran: usize) -> Option<TickReport> {
    (ran > 0).then(|| TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!("ran {ran} of its criteria as its reviewer"),
    })
}

/// The governor's context for `verifying -> accepted`, so that every step judges on exactly what
/// the governor will.
fn context(
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
fn reviewer_results(context: &TransitionContext) -> Vec<CriterionResult> {
    context
        .done
        .results
        .iter()
        .filter(|result| result.run_by == RunBy::Reviewer)
        .cloned()
        .collect()
}

/// The results Farik recorded as the governor since `since`, the latest per criterion.
fn governor_results(history: &[FarikEvent], since: u64) -> Vec<CriterionResult> {
    let mut results: Vec<CriterionResult> = Vec::new();
    for event in history.iter().filter(|event| event.envelope.seq > since) {
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

fn is_human(criterion: &ExitCriterion) -> bool {
    wire_method(&criterion.verification) == Some("human")
}

/// Every event about the task, oldest first.
fn history(
    deps: &OrchestratorDeps,
    task_id: &TaskId,
) -> Result<Vec<FarikEvent>, OrchestratorError> {
    Ok(deps.tools.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        ..EventQuery::default()
    })?)
}

/// The sequence number of the task's last move into `verifying`, or 0.
fn since_verifying(history: &[FarikEvent]) -> u64 {
    history
        .iter()
        .rev()
        .find(|event| {
            matches!(&event.body, EventBody::TaskTransitioned(body) if body.to.to_string() == "verifying")
        })
        .map_or(0, |event| event.envelope.seq)
}

/// Appends one event about the task, stamped with the agent and session when there are any, and
/// projects it.
fn append(
    deps: &OrchestratorDeps,
    task_id: &TaskId,
    agent_id: Option<String>,
    session_id: Option<String>,
    body: EventBody,
) -> Result<(), OrchestratorError> {
    let tools = &deps.tools;
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
