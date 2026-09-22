//! The work tools: transitions, assignment, blocks, criterion results, notes, questions, and
//! product documents.
#![expect(
    dead_code,
    reason = "the handlers arrive with the later tasks of phase 3 step 05"
)]

use std::str::FromStr;

use farik_core::contract::{Role, TaskContract, TaskId, TaskStatus};
use farik_core::governor::gates::{Blocker, Rejection, check_product_doc_write};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::{TransitionActor, find_transitions};
use farik_protocol::event::{
    CriterionRecordedBody, CriterionRecordedBodyRunBy, EventBody, NoteWrittenBody,
    NoteWrittenBodyKind, ProductDocWrittenBody, QuestionAskedBody,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::transitions::{
    TransitionAsk, TransitionOutcome, actor_wire, refusal_details, refusal_wire,
};

/// What is in the way, for a block.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlockerInput {
    /// What is in the way.
    description: String,
    /// What is needed to clear it.
    needed: String,
}

/// Why the reviewer rejected the work.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RejectionInput {
    /// The criteria that failed.
    failed_criterion_ids: Vec<String>,
    /// Why they failed.
    reasons: String,
}

/// `farik_request_transition`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestTransitionInput {
    /// The status to move to.
    to: String,
    /// What is in the way, for a move into `blocked`.
    blocker: Option<BlockerInput>,
    /// What cleared the blocker, for a move out of `blocked`.
    resolution: Option<String>,
    /// Why, for a move into `rejected`.
    rejection: Option<RejectionInput>,
}

/// `farik_assign_task`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "the wire names the task and the two agents by id"
)]
pub(crate) struct AssignTaskInput {
    /// The ready task to assign.
    task_id: String,
    /// The agent that does it.
    assignee_id: String,
    /// The agent that reviews it; none for an epic the human reviews.
    reviewer_id: Option<String>,
}

/// `farik_declare_blocked`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeclareBlockedInput {
    /// What is in the way.
    description: String,
    /// What is needed to clear it.
    needed: String,
}

/// `farik_record_criterion_result`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordCriterionInput {
    /// The criterion, `C<n>`.
    criterion_id: String,
    /// Whether it passed.
    passed: bool,
    /// What shows it: the command's output, the file's contents, what was seen.
    evidence: String,
}

/// Which note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NoteKind {
    /// The assignee's, when the work is done.
    Completion,
    /// The reviewer's.
    Review,
    /// Either's, along the way.
    Progress,
}

/// `farik_write_note`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteNoteInput {
    /// `completion`, `review`, or `progress`.
    kind: NoteKind,
    /// The note.
    text: String,
}

/// `farik_ask_human`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AskHumanInput {
    /// The question.
    question: String,
}

/// `farik_write_product_doc`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteProductDocInput {
    /// Where under `.farik/product/`.
    path: String,
    /// The document.
    content: String,
}

/// Asks the governor to move the session's task, as the first actor of the matching rows the
/// caller is: the assignee or the reviewer the board names, or the Product Manager or Scrum Master
/// by role. When it is none of them nothing is asked, and nothing is recorded.
pub(super) fn request_transition(
    call: &Call<'_>,
    input: RequestTransitionInput,
) -> Result<Value, ToolError> {
    let task = call.task()?.clone();
    let to = TaskStatus::from_str(&input.to).map_err(|_| ToolError::InvalidInput {
        detail: format!("{} is not a status", input.to),
    })?;
    let (contract, _) = call.contract(&task)?;
    let actor = actor_for(call, &contract, to)?;
    let ask = TransitionAsk {
        blocker: input.blocker.map(|blocker| Blocker {
            description: blocker.description,
            needed: blocker.needed,
        }),
        blocker_resolution: input.resolution,
        rejection: input.rejection.map(|rejection| Rejection {
            failed_criterion_ids: rejection.failed_criterion_ids,
            reasons: rejection.reasons,
        }),
        ..TransitionAsk::default()
    };
    ask_governor(call, task, to, actor, ask)
}

/// Assigns a ready task as the caller's role, the Product Manager or the Scrum Master.
pub(super) fn assign_task(call: &Call<'_>, input: AssignTaskInput) -> Result<Value, ToolError> {
    let task: TaskId = input
        .task_id
        .parse()
        .map_err(|error| ToolError::InvalidInput {
            detail: format!("{} is not a task id: {error}", input.task_id),
        })?;
    let actor = match call.role() {
        Role::ProductManager => TransitionActor::ProductManager,
        Role::ScrumMaster => TransitionActor::ScrumMaster,
        role => {
            return Err(Refusal::ActorNotAllowed {
                detail: format!(
                    "role {role} does not assign: the Scrum Master does, or the Product Manager"
                ),
            }
            .into());
        }
    };
    let ask = TransitionAsk {
        assignee_id: Some(input.assignee_id),
        reviewer_id: input.reviewer_id,
        ..TransitionAsk::default()
    };
    ask_governor(call, task, TaskStatus::Assigned, actor, ask)
}

/// Blocks the session's task as its assignee, with what is in the way and what is needed.
pub(super) fn declare_blocked(
    call: &Call<'_>,
    input: DeclareBlockedInput,
) -> Result<Value, ToolError> {
    let task = call.task()?.clone();
    let ask = TransitionAsk {
        blocker: Some(Blocker {
            description: input.description,
            needed: input.needed,
        }),
        ..TransitionAsk::default()
    };
    ask_governor(
        call,
        task,
        TaskStatus::Blocked,
        TransitionActor::Assignee,
        ask,
    )
}

/// Records a criterion's result as the assignee's or the reviewer's run, by the caller's relation
/// to the contract.
pub(super) fn record_criterion(
    call: &Call<'_>,
    input: RecordCriterionInput,
) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, _) = call.contract(task)?;
    let me = Some(call.agent_id());
    let run_by = if contract.assignee.as_deref() == me {
        CriterionRecordedBodyRunBy::Assignee
    } else if contract.reviewer.as_deref() == me {
        CriterionRecordedBodyRunBy::Reviewer
    } else {
        return Err(Refusal::NotTheRunner {
            agent_id: call.agent_id().to_string(),
        }
        .into());
    };
    let criterion_id = input.criterion_id.trim().to_string();
    if !contract
        .exit_criteria
        .iter()
        .any(|criterion| criterion.id.as_str() == criterion_id)
    {
        return Err(Refusal::UnknownCriterion { criterion_id }.into());
    }
    let event = call.append(
        Some(task),
        EventBody::CriterionRecorded(CriterionRecordedBody {
            criterion_id,
            passed: input.passed,
            evidence: input.evidence,
            run_by,
            recorded_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

/// Writes a note about the session's task: a completion note from the assignee, a review note
/// from the reviewer, a progress note from either. The contract's own `notes` are the author's
/// and are not touched.
pub(super) fn write_note(call: &Call<'_>, input: WriteNoteInput) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, _) = call.contract(task)?;
    let me = Some(call.agent_id());
    let is_assignee = contract.assignee.as_deref() == me;
    let is_reviewer = contract.reviewer.as_deref() == me;
    let (kind, allowed) = match input.kind {
        NoteKind::Completion => (NoteWrittenBodyKind::Completion, is_assignee),
        NoteKind::Review => (NoteWrittenBodyKind::Review, is_reviewer),
        NoteKind::Progress => (NoteWrittenBodyKind::Progress, is_assignee || is_reviewer),
    };
    if !allowed {
        return Err(Refusal::NotTheNotesWriter {
            agent_id: call.agent_id().to_string(),
            kind: kind.to_string(),
        }
        .into());
    }
    let event = call.append(
        Some(task),
        EventBody::NoteWritten(NoteWrittenBody {
            kind,
            text: input.text,
            written_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

/// Asks the human, and answers with the question's id: the sequence number of its event, which
/// is unique and is what `farik answer` takes.
pub(super) fn ask_human(call: &Call<'_>, input: AskHumanInput) -> Result<Value, ToolError> {
    let event = call.append(
        call.context.task_id.as_ref(),
        EventBody::QuestionAsked(QuestionAskedBody {
            question: input.question,
            asked_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(json!({
        "question_id": event.envelope.seq,
        "next": "end your turn: the human's answer starts the next session",
    }))
}

/// Writes a product document for the session's epic when `check_product_doc_write` allows it.
/// The user's approval is recorded from phase 3 step 14, so until then none is given.
pub(super) fn write_product_doc(
    call: &Call<'_>,
    input: WriteProductDocInput,
) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, row) = call.contract(task)?;
    check_product_doc_write(contract.kind, row.status, false, call.role())
        .map_err(|details| Refusal::GateFailed { details })?;
    call.deps()
        .files
        .write_product_doc(&input.path, &input.content)
        .map_err(failed)?;
    let event = call.append(
        Some(task),
        EventBody::ProductDocWritten(ProductDocWrittenBody {
            path: input.path,
            written_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

/// The first actor, among the rows from the contract's status to `to`, that the caller is.
fn actor_for(
    call: &Call<'_>,
    contract: &TaskContract,
    to: TaskStatus,
) -> Result<TransitionActor, ToolError> {
    let me = Some(call.agent_id());
    let rows = find_transitions(contract.status, to);
    rows.iter()
        .map(|row| row.actor)
        .find(|actor| match actor {
            TransitionActor::Assignee => contract.assignee.as_deref() == me,
            TransitionActor::Reviewer => contract.reviewer.as_deref() == me,
            TransitionActor::ProductManager => call.role() == Role::ProductManager,
            TransitionActor::ScrumMaster => call.role() == Role::ScrumMaster,
            TransitionActor::Governor | TransitionActor::Human => false,
        })
        .ok_or_else(|| {
            let actors: Vec<String> = rows
                .iter()
                .map(|row| actor_wire(row.actor).to_string())
                .collect();
            Refusal::ActorNotAllowed {
                detail: if actors.is_empty() {
                    format!("nothing moves a {} task to {to}", contract.status)
                } else {
                    format!(
                        "a {} task moves to {to} at the request of {}, and {} is none of them",
                        contract.status,
                        actors.join(", "),
                        call.agent_id()
                    )
                },
            }
            .into()
        })
}

/// Asks the door, with the session's id on every event it records.
fn ask_governor(
    call: &Call<'_>,
    task_id: TaskId,
    to: TaskStatus,
    actor: TransitionActor,
    ask: TransitionAsk,
) -> Result<Value, ToolError> {
    let request = TransitionRequest {
        task_id,
        to,
        actor,
        agent_id: Some(call.agent_id().to_string()),
    };
    let ask = TransitionAsk {
        session_id: Some(call.context.session_id.clone()),
        ..ask
    };
    match call
        .deps()
        .transitions
        .request(&request, &ask, &call.team)
        .map_err(failed)?
    {
        TransitionOutcome::Moved(decision) => Ok(json!({ "status": decision.to.to_string() })),
        TransitionOutcome::Refused(refusal) => Err(ToolError::Refused {
            reason: format!(
                "{}: {}",
                refusal_wire(&refusal),
                refusal_details(&refusal).join("; ")
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::TaskStatus;
    use farik_core::governor::done::RunBy;
    use farik_core::governor::transition::{TransitionContext, TransitionRequest};
    use farik_core::governor::transition_table::TransitionActor;
    use farik_protocol::event::{EventBody, EventKind, TransitionActorWire};
    use serde_json::{Value, json};

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};
    use crate::transitions::TransitionAsk;

    fn a_project(name: &str) -> TestProject {
        TestProject::new(name, &a_team_of_three(|_| {}))
    }

    fn refused_with(result: Result<Value, ToolError>, kind: &str) -> String {
        match result {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with(&format!("{kind}: ")), "{reason}");
                reason
            }
            other => panic!("expected a {kind} refusal, got {other:?}"),
        }
    }

    /// FRK-1 in progress, held by `dev-a` and reviewed by `dev-b`.
    fn in_progress(project: &TestProject) {
        project.filed("FRK-1", "assigned", "task", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
    }

    fn context_of(project: &TestProject) -> TransitionContext {
        let team = project.deps.files.read_team().expect("a team");
        project
            .deps
            .transitions
            .context(
                &TransitionRequest {
                    task_id: "FRK-1".parse().expect("an id"),
                    to: TaskStatus::Accepted,
                    actor: TransitionActor::ProductManager,
                    agent_id: Some("pm".to_string()),
                },
                &TransitionAsk::default(),
                &team,
            )
            .expect("the context reads")
    }

    fn record(project: &TestProject, agent: &str, passed: bool) -> Result<Value, ToolError> {
        project.call(
            agent,
            Some("FRK-1"),
            "farik_record_criterion_result",
            json!({ "criterion_id": "C1", "passed": passed, "evidence": "pnpm test login: 4 passed" }),
        )
    }

    fn note(
        project: &TestProject,
        agent: &str,
        kind: &str,
        text: &str,
    ) -> Result<Value, ToolError> {
        project.call(
            agent,
            Some("FRK-1"),
            "farik_write_note",
            json!({ "kind": kind, "text": text }),
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn assigns_a_ready_task_as_the_product_manager() {
        let project = a_project("tools-assign");
        project.filed("FRK-1", "ready", "task", None);
        let moved = project
            .call(
                "pm",
                None,
                "farik_assign_task",
                json!({ "task_id": "FRK-1", "assignee_id": "dev-a", "reviewer_id": "dev-b" }),
            )
            .expect("the Product Manager assigns a ready task");
        assert_eq!(moved["status"], "assigned");
        let row = project
            .deps
            .projections
            .task(&"FRK-1".parse().expect("an id"))
            .expect("the board reads")
            .expect("a row");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn derives_the_assignee_actor_for_a_move_to_verifying() {
        let project = a_project("tools-verifying");
        in_progress(&project);
        let reason = refused_with(
            project.call(
                "dev-a",
                Some("FRK-1"),
                "farik_request_transition",
                json!({ "to": "verifying" }),
            ),
            "gate_failed",
        );
        assert!(reason.contains("no run with evidence"), "{reason}");
        let refused = project.events(&[EventKind::TransitionRefused]);
        assert_eq!(refused.len(), 1);
        let EventBody::TransitionRefused(body) = &refused[0].body else {
            panic!("a refusal");
        };
        assert_eq!(body.actor, TransitionActorWire::Assignee);
        assert_eq!(
            refused[0].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_criterion_result_as_its_runner() {
        let project = a_project("tools-criterion");
        in_progress(&project);
        record(&project, "dev-b", false).expect("the reviewer records its own run");
        record(&project, "dev-b", true).expect("and runs it again");
        record(&project, "dev-a", true).expect("the assignee records its own");
        let recorded = project.events(&[EventKind::CriterionRecorded]);
        let EventBody::CriterionRecorded(body) = &recorded[0].body else {
            panic!("a criterion.recorded");
        };
        assert_eq!(body.run_by.to_string(), "reviewer");
        assert_eq!(body.recorded_by, "dev-b");
        let context = context_of(&project);
        let reviewers: Vec<_> = context
            .done
            .results
            .iter()
            .filter(|result| result.run_by == RunBy::Reviewer)
            .collect();
        assert_eq!(
            reviewers.len(),
            1,
            "the latest run per criterion and runner"
        );
        assert!(reviewers[0].passed);
        assert_eq!(reviewers[0].criterion_id, "C1");
        assert!(
            context
                .assignee_results
                .iter()
                .any(|result| result.run_by == RunBy::Assignee && result.criterion_id == "C1"),
            "{:?}",
            context.assignee_results
        );

        refused_with(record(&project, "pm", true), "not_the_runner");
        let unknown = project.call(
            "dev-a",
            Some("FRK-1"),
            "farik_record_criterion_result",
            json!({ "criterion_id": "C9", "passed": true, "evidence": "it ran" }),
        );
        refused_with(unknown, "unknown_criterion");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_latest_notes_into_the_done_evidence() {
        let project = a_project("tools-notes");
        in_progress(&project);
        note(&project, "dev-a", "completion", "First pass.").expect("the assignee's note");
        note(&project, "dev-a", "completion", "Done, with the tests.").expect("and a later one");
        note(&project, "dev-b", "review", "C1 ran and passed.").expect("the reviewer's note");
        let context = context_of(&project);
        assert_eq!(
            context.done.completion_note.as_deref(),
            Some("Done, with the tests.")
        );
        assert_eq!(
            context.done.review_note.as_deref(),
            Some("C1 ran and passed.")
        );
        refused_with(
            note(&project, "dev-b", "completion", "Not mine to say."),
            "not_the_notes_writer",
        );
        refused_with(
            note(&project, "dev-a", "review", "Not mine either."),
            "not_the_notes_writer",
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn forgets_an_iterations_evidence_after_a_rejection() {
        let project = a_project("tools-forget");
        in_progress(&project);
        record(&project, "dev-a", true).expect("the assignee's run");
        record(&project, "dev-b", false).expect("the reviewer's run");
        note(&project, "dev-a", "completion", "Done.").expect("a note");
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        project.moved("FRK-1", "in_progress", "verifying", &people);
        project.moved("FRK-1", "verifying", "rejected", &people);
        project.moved("FRK-1", "rejected", "in_progress", &people);
        let context = context_of(&project);
        assert!(context.assignee_results.is_empty());
        assert!(context.done.results.is_empty());
        assert_eq!(context.done.completion_note, None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn asks_the_human_and_answers_with_the_question_id() {
        let project = a_project("tools-ask");
        let asked = project
            .call(
                "pm",
                None,
                "farik_ask_human",
                json!({ "question": "Should a login page remember the user?" }),
            )
            .expect("anyone asks");
        let events = project.events(&[EventKind::QuestionAsked]);
        assert_eq!(asked["question_id"], json!(events[0].envelope.seq));
        let EventBody::QuestionAsked(body) = &events[0].body else {
            panic!("a question");
        };
        assert_eq!(body.asked_by, "pm");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_product_document_before_the_user_approves_the_epic() {
        let project = a_project("tools-product-doc");
        project.filed("FRK-1", "in_progress", "epic", None);
        let reason = refused_with(
            project.call(
                "pm",
                Some("FRK-1"),
                "farik_write_product_doc",
                json!({ "path": "prd.md", "content": "# Sign-in" }),
            ),
            "gate_failed",
        );
        assert!(reason.contains("has not approved"), "{reason}");
        assert!(
            !project.repo.path.join(".farik/product/prd.md").exists(),
            "nothing is written"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn declares_a_block_with_its_blocker() {
        let project = a_project("tools-blocked");
        in_progress(&project);
        let moved = project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_declare_blocked",
                json!({ "description": "The sign-in API is down.", "needed": "A working API key." }),
            )
            .expect("the assignee blocks its task");
        assert_eq!(moved["status"], "blocked");
        let transitioned = project.events(&[EventKind::TaskTransitioned]);
        let EventBody::TaskTransitioned(body) = &transitioned[transitioned.len() - 1].body else {
            panic!("a move");
        };
        let blocker = body.blocker.as_ref().expect("the move carries its blocker");
        assert_eq!(blocker.description, "The sign-in API is down.");
        assert_eq!(blocker.needed, "A working API key.");
    }
}
