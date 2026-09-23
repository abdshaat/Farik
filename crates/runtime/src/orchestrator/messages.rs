//! The first user message of each kind of session: what it is about, in words the agent reads
//! before anything else.

use farik_core::contract::TaskContract;
use farik_store::git::HeadSummary;

use crate::prompt::untrusted_block;

/// How much of a note a first message carries.
const NOTE_CAP_BYTES: usize = 16 * 1024;

/// Where an implement session picks up: the branch's last commit past its base, and the last note
/// written since the task last moved into `in_progress`, as its kind and text.
pub(super) struct Resume {
    /// The branch's tip, when it has a commit past its base.
    pub(super) last_commit: Option<HeadSummary>,
    /// The last note, when there is one.
    pub(super) last_note: Option<(String, String)>,
}

/// The plan session's message for a ready task: assign it, with the agents that could do it and
/// review it.
pub(super) fn plan_message(
    contract: &TaskContract,
    assignees: &[String],
    reviewers: &[String],
) -> String {
    format!(
        "Assign {task} with `farik_assign_task`, naming its assignee and its reviewer. The agents \
         of its assignee role, {assignee_role}, with room for it: {assignees}. The agents of its \
         reviewer role, {reviewer_role}: {reviewers}. The reviewer is never the assignee.",
        task = contract.id.as_str(),
        assignee_role = contract.assignee_role,
        reviewer_role = contract.reviewer_role,
        assignees = listed(assignees),
        reviewers = listed(reviewers),
    )
}

/// The implement session's message: the task, and where the work stands when an earlier session
/// left something: `Resuming: last commit <sha> <subject>; last note (<kind>): <text>`, the note as
/// untrusted text.
pub(super) fn implement_message(contract: &TaskContract, resume: &Resume) -> String {
    let task = contract.id.as_str();
    let message = format!(
        "Do the work of {task} under its contract, in this worktree, on the branch farik/{task}."
    );
    if resume.last_commit.is_none() && resume.last_note.is_none() {
        return message;
    }
    let commit = resume.last_commit.as_ref().map_or_else(
        || "no commit yet".to_string(),
        |head| format!("last commit {} {}", head.sha, head.subject),
    );
    let note = resume.last_note.as_ref().map_or_else(
        || "no note yet".to_string(),
        |(kind, text)| {
            format!(
                "last note ({kind}): {}",
                untrusted_block("note", text, NOTE_CAP_BYTES)
            )
        },
    );
    format!("{message}\n\nResuming: {commit}; {note}")
}

fn listed(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
