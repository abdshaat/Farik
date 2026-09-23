//! The first user message of each kind of session: what it is about, in words the agent reads
//! before anything else.

use farik_core::contract::{TaskContract, Verification};
use farik_core::governor::done::CriterionResult;
use farik_store::git::HeadSummary;

use crate::prompt::untrusted_block;

/// How much of a note a first message carries.
const NOTE_CAP_BYTES: usize = 16 * 1024;
/// How much of a list of results a first message carries.
const RESULTS_CAP_BYTES: usize = 32 * 1024;
/// How much of a diff a reviewer's first message carries.
const DIFF_CAP_BYTES: usize = 64 * 1024;

/// Where an implement session picks up: the branch's last commit past its base, and the last note
/// written since the task last moved into `in_progress`, as its kind and text.
pub(super) struct Resume {
    /// The branch's tip, when it has a commit past its base.
    pub(super) last_commit: Option<HeadSummary>,
    /// The last note, when there is one.
    pub(super) last_note: Option<(String, String)>,
    /// The failed criterion ids and the reasons of the rejection this iteration answers, when it
    /// answers one.
    pub(super) rejection: Option<(Vec<String>, String)>,
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

/// The implement session's message: the task, the rejection this iteration answers as untrusted
/// text when there is one, and where the work stands when an earlier session left something: `Resuming: last commit <sha> <subject>; last note (<kind>): <text>`, the note as
/// untrusted text.
pub(super) fn implement_message(contract: &TaskContract, resume: &Resume) -> String {
    let task = contract.id.as_str();
    let mut message = format!(
        "Do the work of {task} under its contract, in this worktree, on the branch farik/{task}."
    );
    if let Some((failed, reasons)) = &resume.rejection {
        let words = format!("failed criteria: {}\nreasons: {reasons}", listed(failed));
        message = format!(
            "{message}\n\nThe reviewer rejected the last iteration. Fix what failed: {}",
            untrusted_block("rejection", &words, NOTE_CAP_BYTES)
        );
    }
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

/// What the reviewer's first message is made of.
pub(super) struct ReviewBrief<'a> {
    /// The task.
    pub(super) contract: &'a TaskContract,
    /// What Farik found when it ran the `command`, `test`, and `artifact` criteria.
    pub(super) results: &'a [CriterionResult],
    /// The assignee's completion note.
    pub(super) completion_note: Option<&'a str>,
    /// The diff from the integration branch to the task's branch.
    pub(super) diff: &'a str,
    /// The criteria a review note was written without answering, on a second asking.
    pub(super) unanswered: &'a [String],
}

/// The reviewer's `verify` session's message: the task's id and title, what is still unanswered
/// when anything is, Farik's results, the rubric of each `review` criterion, the completion note,
/// and the diff, each an agent's or the repository's words and so untrusted; nothing from any
/// implement session (5.4).
pub(super) fn review_message(brief: &ReviewBrief<'_>) -> String {
    let contract = brief.contract;
    let task = contract.id.as_str();
    let mut message = format!(
        "Verify {task} as its reviewer. Its title: {}",
        untrusted_block("title", &contract.title.to_string(), NOTE_CAP_BYTES)
    );
    if !brief.unanswered.is_empty() {
        message = format!(
            "{message}\n\nStill unanswered: {}. Record a result for each with \
             `farik_record_criterion_result`, citing your evidence.",
            brief.unanswered.join(", ")
        );
    }
    let rubrics: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter_map(
            |criterion| match Verification::from(&criterion.verification) {
                Verification::Review { rubric } => Some(format!(
                    "{}: {}\n{}",
                    criterion.id.as_str(),
                    criterion.text.as_str(),
                    rubric
                        .iter()
                        .map(|question| format!("- {question}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )),
                _ => None,
            },
        )
        .collect();
    let rubrics = if rubrics.is_empty() {
        "It has no `review` criteria.".to_string()
    } else {
        format!(
            "Answer each `review` criterion with `farik_record_criterion_result`, citing your \
             evidence: {}",
            untrusted_block("rubric", &rubrics.join("\n\n"), RESULTS_CAP_BYTES)
        )
    };
    format!(
        "{message}\n\nFarik ran its `command`, `test`, and `artifact` criteria in the task's \
         sandbox, as its reviewer: {results}\n\n{rubrics}\n\nThe assignee's completion note: \
         {note}\n\nThe diff from the integration branch to farik/{task}: {diff}\n\nWrite the \
         review note with `farik_write_note` of kind `review`, mapping each criterion to its \
         evidence.",
        results = untrusted_block("results", &results_text(brief.results), RESULTS_CAP_BYTES),
        note = untrusted_block(
            "completion_note",
            brief.completion_note.unwrap_or("none written"),
            NOTE_CAP_BYTES
        ),
        diff = untrusted_block("diff", brief.diff, DIFF_CAP_BYTES),
    )
}

/// The Product Manager's `verify` session's message: the review passed every criterion, its note
/// and the reviewer's results as untrusted text, and `accepted` to ask for.
pub(super) fn accept_message(
    contract: &TaskContract,
    review_note: &str,
    results: &[CriterionResult],
) -> String {
    format!(
        "The review of {task} passed every criterion. Request `accepted` for it with \
         `farik_request_transition`. The review note: {note}\n\nThe reviewer's results: \
         {results}",
        task = contract.id.as_str(),
        note = untrusted_block("review_note", review_note, NOTE_CAP_BYTES),
        results = untrusted_block("results", &results_text(results), RESULTS_CAP_BYTES),
    )
}

/// Each result as its id, whether it passed, and its evidence.
fn results_text(results: &[CriterionResult]) -> String {
    if results.is_empty() {
        return "none".to_string();
    }
    results
        .iter()
        .map(|result| {
            format!(
                "{}: {}\n{}",
                result.criterion_id,
                if result.passed { "passed" } else { "failed" },
                result.evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn listed(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
