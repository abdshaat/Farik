//! The work tools: transitions, assignment, blocks, criterion results, notes, questions, and
//! product documents.
#![expect(
    dead_code,
    reason = "the handlers arrive with the later tasks of phase 3 step 05"
)]

use schemars::JsonSchema;
use serde::Deserialize;

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
