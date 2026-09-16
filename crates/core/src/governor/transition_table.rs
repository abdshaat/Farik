use super::task_status::is_terminal;
use crate::contract::TaskStatus;

/// Who may request a transition. `Governor` means the orchestrator acting on observed facts;
/// `Human` means the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionActor {
    /// The Product Manager role.
    ProductManager,
    /// The Scrum Master role.
    ScrumMaster,
    /// The agent assigned to the task.
    Assignee,
    /// The agent reviewing the task.
    Reviewer,
    /// The orchestrator, from observed facts, never from a request.
    Governor,
    /// The user.
    Human,
}

/// The gate a transition must pass; `None` means the transition has no gate. Each gate is a
/// predicate of its own in a later step of this phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateId {
    /// No gate.
    None,
    /// The request has a recorded triage decision (spec 5.16).
    Triaged,
    /// The Definition of Ready (spec 5.3).
    DefinitionOfReady,
    /// The contract failed the Definition of Ready three times.
    ReadinessExhausted,
    /// The contract itself needs the human's acceptance: high risk, policy, or an epic.
    ContractRequiresHuman,
    /// Assignee role, sprint budget, WIP limit, and integrated dependencies (spec 5.2, 5.14).
    Assignment,
    /// Every criterion has the assignee's own result and the work is committed; for an epic,
    /// its tasks are done (spec 5.16).
    CriteriaRecorded,
    /// A written blocker with what is needed.
    BlockerWritten,
    /// The blocker is resolved.
    BlockerResolved,
    /// Blocked longer than the configured limit.
    BlockedAge,
    /// The Definition of Done (spec 5.4).
    DefinitionOfDone,
    /// Written reasons mapped to failed criteria.
    RejectionReasons,
    /// The iteration count is below the limit.
    IterationBelowLimit,
    /// The iteration limit is reached.
    IterationLimitReached,
    /// Budget exhausted or permission denied on a required action.
    GovernorEscalation,
}

/// A status pattern in a row: one status, or any status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Matches every status.
    Any,
    /// Matches one status.
    Is(TaskStatus),
}

impl Status {
    /// Whether the pattern matches a status.
    #[must_use]
    pub fn matches(self, status: TaskStatus) -> bool {
        match self {
            Self::Any => true,
            Self::Is(pattern) => pattern == status,
        }
    }
}

/// One line of the transition table: a move, who may request it, and the gate it passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRow {
    /// The status the task leaves.
    pub from: Status,
    /// The status the task enters.
    pub to: Status,
    /// Who may request the move.
    pub actor: TransitionActor,
    /// The gate the move must pass.
    pub gate: GateId,
}

const fn row(from: Status, to: Status, actor: TransitionActor, gate: GateId) -> TransitionRow {
    TransitionRow {
        from,
        to,
        actor,
        gate,
    }
}

/// The transition table of `docs/SPEC.md` section 5.2: the spec's lines in order, split where
/// a line names two triggers or two actors, with the `any` rows last.
pub static TRANSITION_TABLE: [TransitionRow; 20] = {
    use GateId as G;
    use Status::{Any, Is};
    use TaskStatus as S;
    use TransitionActor as A;
    [
        row(Is(S::Draft), Is(S::Refining), A::ProductManager, G::Triaged),
        row(
            Is(S::Refining),
            Is(S::Ready),
            A::Governor,
            G::DefinitionOfReady,
        ),
        row(
            Is(S::Refining),
            Is(S::Escalated),
            A::Governor,
            G::ReadinessExhausted,
        ),
        row(
            Is(S::Refining),
            Is(S::Escalated),
            A::Governor,
            G::ContractRequiresHuman,
        ),
        row(Is(S::Ready), Is(S::Assigned), A::ScrumMaster, G::Assignment),
        row(
            Is(S::Ready),
            Is(S::Assigned),
            A::ProductManager,
            G::Assignment,
        ),
        row(Is(S::Assigned), Is(S::InProgress), A::Assignee, G::None),
        row(
            Is(S::InProgress),
            Is(S::Verifying),
            A::Assignee,
            G::CriteriaRecorded,
        ),
        row(
            Is(S::InProgress),
            Is(S::Blocked),
            A::Assignee,
            G::BlockerWritten,
        ),
        row(
            Is(S::Blocked),
            Is(S::InProgress),
            A::ScrumMaster,
            G::BlockerResolved,
        ),
        row(
            Is(S::Blocked),
            Is(S::InProgress),
            A::Human,
            G::BlockerResolved,
        ),
        row(Is(S::Blocked), Is(S::Escalated), A::Governor, G::BlockedAge),
        row(
            Is(S::Verifying),
            Is(S::Accepted),
            A::ProductManager,
            G::DefinitionOfDone,
        ),
        row(
            Is(S::Verifying),
            Is(S::Rejected),
            A::Reviewer,
            G::RejectionReasons,
        ),
        row(
            Is(S::Rejected),
            Is(S::InProgress),
            A::Governor,
            G::IterationBelowLimit,
        ),
        row(
            Is(S::Rejected),
            Is(S::Escalated),
            A::Governor,
            G::IterationLimitReached,
        ),
        row(Any, Is(S::Escalated), A::Governor, G::GovernorEscalation),
        row(Any, Is(S::Escalated), A::Human, G::None),
        row(Any, Is(S::Cancelled), A::Human, G::None),
        row(Is(S::Escalated), Any, A::Human, G::None),
    ]
};

/// The rows that apply to a move from `from` to `to`, in table order. Empty when `from` is
/// terminal, because nothing leaves `accepted` or `cancelled`, and when `from` and `to` are the
/// same status, because an `any` row never means staying put.
#[must_use]
pub fn find_transitions(from: TaskStatus, to: TaskStatus) -> Vec<&'static TransitionRow> {
    if from == to || is_terminal(from) {
        return Vec::new();
    }
    TRANSITION_TABLE
        .iter()
        .filter(|row| row.from.matches(from) && row.to.matches(to))
        .collect()
}

/// The rows that leave `from`, in table order. Empty when `from` is terminal.
#[must_use]
pub fn transitions_from(from: TaskStatus) -> Vec<&'static TransitionRow> {
    if is_terminal(from) {
        return Vec::new();
    }
    TRANSITION_TABLE
        .iter()
        .filter(|row| row.from.matches(from))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::GateId as G;
    use super::TransitionActor as A;
    use super::{Status, TRANSITION_TABLE, TransitionRow, find_transitions, transitions_from};
    use crate::contract::TaskStatus as S;
    use crate::governor::task_status::{TASK_STATUSES, is_terminal};

    fn actors_and_gates(rows: &[&TransitionRow]) -> Vec<(A, G)> {
        rows.iter().map(|row| (row.actor, row.gate)).collect()
    }

    type Line = (S, S, &'static [(A, G)]);

    fn is_specific(row: TransitionRow) -> bool {
        row.from != Status::Any && row.to != Status::Any
    }

    #[test]
    fn has_exactly_twenty_distinct_rows() {
        let distinct: BTreeSet<String> = TRANSITION_TABLE
            .iter()
            .map(|row| format!("{row:?}"))
            .collect();
        assert_eq!(TRANSITION_TABLE.len(), 20);
        assert_eq!(distinct.len(), 20);
    }

    #[test]
    fn has_sixteen_specific_rows_and_four_any_rows() {
        let specific = TRANSITION_TABLE
            .iter()
            .filter(|row| is_specific(**row))
            .count();
        assert_eq!(specific, 16);
        assert_eq!(TRANSITION_TABLE.len() - specific, 4);
    }

    #[test]
    fn matches_every_line_of_the_spec_table() {
        let lines: [Line; 13] = [
            (S::Draft, S::Refining, &[(A::ProductManager, G::Triaged)]),
            (
                S::Refining,
                S::Ready,
                &[(A::Governor, G::DefinitionOfReady)],
            ),
            (
                S::Refining,
                S::Escalated,
                &[
                    (A::Governor, G::ReadinessExhausted),
                    (A::Governor, G::ContractRequiresHuman),
                ],
            ),
            (
                S::Ready,
                S::Assigned,
                &[
                    (A::ScrumMaster, G::Assignment),
                    (A::ProductManager, G::Assignment),
                ],
            ),
            (S::Assigned, S::InProgress, &[(A::Assignee, G::None)]),
            (
                S::InProgress,
                S::Verifying,
                &[(A::Assignee, G::CriteriaRecorded)],
            ),
            (
                S::InProgress,
                S::Blocked,
                &[(A::Assignee, G::BlockerWritten)],
            ),
            (
                S::Blocked,
                S::InProgress,
                &[
                    (A::ScrumMaster, G::BlockerResolved),
                    (A::Human, G::BlockerResolved),
                ],
            ),
            (S::Blocked, S::Escalated, &[(A::Governor, G::BlockedAge)]),
            (
                S::Verifying,
                S::Accepted,
                &[(A::ProductManager, G::DefinitionOfDone)],
            ),
            (
                S::Verifying,
                S::Rejected,
                &[(A::Reviewer, G::RejectionReasons)],
            ),
            (
                S::Rejected,
                S::InProgress,
                &[(A::Governor, G::IterationBelowLimit)],
            ),
            (
                S::Rejected,
                S::Escalated,
                &[(A::Governor, G::IterationLimitReached)],
            ),
        ];
        for (from, to, expected) in lines {
            let rows: Vec<&TransitionRow> = find_transitions(from, to)
                .into_iter()
                .filter(|row| is_specific(**row))
                .collect();
            assert_eq!(actors_and_gates(&rows), expected, "{from} -> {to}");
        }
    }

    #[test]
    fn escalates_any_non_terminal_status_for_the_governor_and_the_human() {
        for status in TASK_STATUSES {
            if is_terminal(status) || status == S::Escalated {
                continue;
            }
            let rows: Vec<&TransitionRow> = find_transitions(status, S::Escalated)
                .into_iter()
                .filter(|row| row.from == Status::Any)
                .collect();
            assert_eq!(
                actors_and_gates(&rows),
                [(A::Governor, G::GovernorEscalation), (A::Human, G::None)],
                "{status}"
            );
        }
    }

    #[test]
    fn cancels_any_non_terminal_status_for_the_human_only() {
        for status in TASK_STATUSES {
            if is_terminal(status) {
                continue;
            }
            let rows: Vec<&TransitionRow> = find_transitions(status, S::Cancelled)
                .into_iter()
                .filter(|row| row.from == Status::Any)
                .collect();
            assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
        }
    }

    #[test]
    fn lets_the_human_move_an_escalated_task_to_any_other_status() {
        for status in TASK_STATUSES {
            if status == S::Escalated {
                continue;
            }
            let rows: Vec<&TransitionRow> = find_transitions(S::Escalated, status)
                .into_iter()
                .filter(|row| row.to == Status::Any)
                .collect();
            assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
        }
    }

    #[test]
    fn refuses_to_leave_a_terminal_status() {
        for status in [S::Accepted, S::Cancelled] {
            assert!(transitions_from(status).is_empty(), "{status}");
            assert!(
                find_transitions(status, S::Escalated).is_empty(),
                "{status}"
            );
            assert!(
                find_transitions(status, S::Cancelled).is_empty(),
                "{status}"
            );
        }
    }

    #[test]
    fn refuses_a_transition_to_the_same_status() {
        for status in TASK_STATUSES {
            assert!(find_transitions(status, status).is_empty(), "{status}");
        }
    }

    #[test]
    fn lists_the_rows_that_leave_a_status_in_table_order() {
        assert_eq!(
            actors_and_gates(&transitions_from(S::Blocked)),
            [
                (A::ScrumMaster, G::BlockerResolved),
                (A::Human, G::BlockerResolved),
                (A::Governor, G::BlockedAge),
                (A::Governor, G::GovernorEscalation),
                (A::Human, G::None),
                (A::Human, G::None),
            ]
        );
    }

    #[test]
    fn reaches_every_status_from_draft() {
        let mut seen = BTreeSet::from([S::Draft]);
        let mut queue = vec![S::Draft];
        while let Some(status) = queue.pop() {
            for row in transitions_from(status) {
                let targets: Vec<S> = match row.to {
                    Status::Any => TASK_STATUSES.to_vec(),
                    Status::Is(target) => vec![target],
                };
                for target in targets {
                    if seen.insert(target) {
                        queue.push(target);
                    }
                }
            }
        }
        assert_eq!(seen.len(), TASK_STATUSES.len());
    }
}
