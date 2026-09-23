//! The first user message of each kind of session: what it is about, in words the agent reads
//! before anything else.

use farik_core::contract::TaskContract;

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

fn listed(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
