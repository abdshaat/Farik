use crate::contract::TaskStatus;

/// Every status of the task lifecycle, in the order of the schema's `status` enum.
pub const TASK_STATUSES: [TaskStatus; 11] = [
    TaskStatus::Draft,
    TaskStatus::Refining,
    TaskStatus::Ready,
    TaskStatus::Assigned,
    TaskStatus::InProgress,
    TaskStatus::Blocked,
    TaskStatus::Verifying,
    TaskStatus::Rejected,
    TaskStatus::Accepted,
    TaskStatus::Escalated,
    TaskStatus::Cancelled,
];

/// Whether a status is one that no transition leaves: `accepted` or `cancelled`.
#[must_use]
pub fn is_terminal(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Accepted | TaskStatus::Cancelled)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::{TASK_STATUSES, is_terminal};
    use crate::contract::TaskStatus;

    #[test]
    fn lists_every_status_of_the_schema_exactly_once() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../docs/schemas/task-contract.schema.json"
        ))
        .expect("the embedded schema is valid JSON");
        let wire: Vec<String> = schema["properties"]["status"]["enum"]
            .as_array()
            .expect("status is an enum")
            .iter()
            .map(|value| value.as_str().expect("a status name").to_string())
            .collect();
        let listed: Vec<String> = TASK_STATUSES.iter().map(ToString::to_string).collect();
        let distinct: BTreeSet<&String> = listed.iter().collect();
        assert_eq!(distinct.len(), TASK_STATUSES.len());
        assert_eq!(listed, wire);
    }

    #[test]
    fn treats_only_accepted_and_cancelled_as_terminal() {
        let terminal: Vec<TaskStatus> = TASK_STATUSES
            .into_iter()
            .filter(|status| is_terminal(*status))
            .collect();
        assert_eq!(terminal, [TaskStatus::Accepted, TaskStatus::Cancelled]);
    }
}
