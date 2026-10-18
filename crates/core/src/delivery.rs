//! When a delivery closes (ADR 0062): the first time since the previous one that an epic was
//! accepted, `TASKS_PER_DELIVERY` tasks with no parent were accepted, or the team has no work left.

use crate::contract::TaskKind;

/// How many accepted tasks with no parent close a delivery.
pub const TASKS_PER_DELIVERY: usize = 5;

/// Why a delivery closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryTrigger {
    /// An epic was accepted.
    EpicAccepted,
    /// `TASKS_PER_DELIVERY` tasks with no parent were accepted.
    TasksAccepted,
    /// Something was accepted and nothing is left to do.
    TeamQuiet,
}

/// A task accepted since the delivery's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedRow {
    /// Epic or task.
    pub kind: TaskKind,
    /// Whether the row belongs to an epic.
    pub has_parent: bool,
}

/// What closes a delivery now, given the rows accepted since it started and whether any task is
/// still `ready`, `assigned`, `in_progress`, `verifying` or `rejected`.
#[must_use]
pub fn delivery_trigger(accepted: &[AcceptedRow], work_left: bool) -> Option<DeliveryTrigger> {
    if accepted.iter().any(|row| row.kind == TaskKind::Epic) {
        return Some(DeliveryTrigger::EpicAccepted);
    }
    let standalone = accepted
        .iter()
        .filter(|row| row.kind == TaskKind::Task && !row.has_parent)
        .count();
    if standalone >= TASKS_PER_DELIVERY {
        return Some(DeliveryTrigger::TasksAccepted);
    }
    (!accepted.is_empty() && !work_left).then_some(DeliveryTrigger::TeamQuiet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(has_parent: bool) -> AcceptedRow {
        AcceptedRow {
            kind: TaskKind::Task,
            has_parent,
        }
    }

    #[test]
    fn closes_on_an_epic_five_tasks_or_a_quiet_team() {
        assert_eq!(delivery_trigger(&[], false), None);
        assert_eq!(delivery_trigger(&[task(false)], true), None);
        assert_eq!(
            delivery_trigger(&[task(false)], false),
            Some(DeliveryTrigger::TeamQuiet)
        );
        let mut four = vec![task(false); 4];
        four.push(task(true));
        assert_eq!(delivery_trigger(&four, true), None);
        assert_eq!(
            delivery_trigger(&[task(false); 5], true),
            Some(DeliveryTrigger::TasksAccepted)
        );
        let epic = AcceptedRow {
            kind: TaskKind::Epic,
            has_parent: false,
        };
        assert_eq!(
            delivery_trigger(&[task(true), epic], true),
            Some(DeliveryTrigger::EpicAccepted)
        );
    }
}
