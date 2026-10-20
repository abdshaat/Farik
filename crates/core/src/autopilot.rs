//! What the team may decide for the owner while autopilot is on (`docs/SPEC.md` 5.2 and 5.7, ADR
//! 0082): the choices the Product Manager has for each thing that waits, the limits on them, and
//! who answers a question the team asks itself. Pure: the log's answers are `catervas-store`'s.

use crate::contract::{Role, TaskStatus};
use crate::governor::gates::return_from_block;

/// How many `more_tries` or `more_room` the team may give one task (decision 3).
pub const TEAM_GRANTS_PER_TASK: u32 = 3;

/// How many times the team may reopen one task's failed integration, or retry one failed folder
/// change: a failing check retried forever would spend a decide session per tick.
pub const REOPENS_PER_TASK: u32 = 3;

/// How many decide sessions one wait may cost before it is flagged.
pub const DECIDE_SESSIONS_PER_WAIT: u32 = 3;

/// How long a block may stand before the Product Manager decides it (decision 3).
pub const BLOCK_WAIT_MINUTES: i64 = 60;

/// How many seller messages go out in a day under autopilot (decision 6).
pub const SELLER_MESSAGES_PER_DAY: usize = 50;

/// How many days "Done on its own" lists (decision 10).
pub const DONE_DAYS: i64 = 7;

/// What the team decides in the owner's place, by name. The wire names are the owner's own where
/// the owner has the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecideChoice {
    /// Two more tries for a task that ran out of iterations.
    MoreTries,
    /// More budget or sessions for a task that ran out of them.
    MoreRoom,
    /// Back to the plan.
    BackToPlan,
    /// Check the contract's readiness again.
    TryReadinessAgain,
    /// Integrate the accepted work again.
    ReopenIntegration,
    /// Answer a block.
    AnswerBlock,
    /// Cancel the task.
    Cancel,
    /// Approve an epic or a high-risk contract.
    ApprovePlan,
    /// Accept a result that waits for the owner.
    AcceptResult,
    /// Carry on after a permission, an explicit request or a preview.
    CarryOn,
    /// Try a failed folder change again.
    RetryFolderChange,
    /// Leave it for the owner: only the owner can give what it needs.
    Flag,
}

impl DecideChoice {
    /// Every choice, in the order of the wire list.
    const ALL: [DecideChoice; 12] = [
        DecideChoice::MoreTries,
        DecideChoice::MoreRoom,
        DecideChoice::BackToPlan,
        DecideChoice::TryReadinessAgain,
        DecideChoice::ReopenIntegration,
        DecideChoice::AnswerBlock,
        DecideChoice::Cancel,
        DecideChoice::ApprovePlan,
        DecideChoice::AcceptResult,
        DecideChoice::CarryOn,
        DecideChoice::RetryFolderChange,
        DecideChoice::Flag,
    ];

    /// The choice's name on the wire and in the tool.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            DecideChoice::MoreTries => "more_tries",
            DecideChoice::MoreRoom => "more_room",
            DecideChoice::BackToPlan => "back_to_plan",
            DecideChoice::TryReadinessAgain => "try_readiness_again",
            DecideChoice::ReopenIntegration => "reopen_integration",
            DecideChoice::AnswerBlock => "answer_block",
            DecideChoice::Cancel => "cancel",
            DecideChoice::ApprovePlan => "approve_plan",
            DecideChoice::AcceptResult => "accept_result",
            DecideChoice::CarryOn => "carry_on",
            DecideChoice::RetryFolderChange => "retry_folder_change",
            DecideChoice::Flag => "flag",
        }
    }

    /// The choice a wire name stands for.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<DecideChoice> {
        Self::ALL.into_iter().find(|choice| choice.wire() == wire)
    }

    /// What the choice does, in one sentence, for the decide session's prompt.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            DecideChoice::MoreTries => "give the task two more tries, and send it back to work",
            DecideChoice::MoreRoom => {
                "give the task more budget or sessions, and send it back to where it was"
            }
            DecideChoice::BackToPlan => "send the task back to its plan, to be refined again",
            DecideChoice::TryReadinessAgain => {
                "send the contract back to refining, to be checked for readiness again"
            }
            DecideChoice::ReopenIntegration => {
                "integrate the accepted work again, after the cause of the failure is gone"
            }
            DecideChoice::AnswerBlock => {
                "answer what the task is blocked on, and send it back to where it was blocked"
            }
            DecideChoice::Cancel => "cancel the task: its work is dropped",
            DecideChoice::ApprovePlan => "approve the plan or the contract, as the owner would",
            DecideChoice::AcceptResult => "accept the result, as the owner would",
            DecideChoice::CarryOn => "let the task carry on from where it was",
            DecideChoice::RetryFolderChange => "try the failed folder change again",
            DecideChoice::Flag => {
                "leave it on Today for the owner, with what only the owner can give"
            }
        }
    }
}

/// What a team question is about: the Product Manager answers `product`, the Architect `technical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionAbout {
    /// What to build, for whom, in what order.
    Product,
    /// How to build it.
    Technical,
}

/// What the team has already used of its limits on one task, or one folder change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    /// `more_tries` and `more_room` chosen for the task.
    pub grants: u32,
    /// `reopen_integration` chosen for the task, or `retry_folder_change` for the change.
    pub reopens: u32,
}

/// What a choice does to the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecideMove {
    /// A transition of the task to `status`, with more tries or more room granted.
    To {
        /// Where the task goes.
        status: TaskStatus,
        /// Iterations granted to the contract, when the choice grants them.
        extra_tries: Option<u8>,
        /// Whether the task's budget and sessions are raised.
        extra_room: bool,
    },
    /// The task's integration is opened again.
    Reopen,
    /// The folder change is tried again.
    RetryFolderChange,
    /// The contract is accepted, then the task goes to `ready`.
    ApprovePlan,
    /// The result is accepted.
    AcceptResult,
    /// The task stays where it is, flagged.
    Flag,
}

/// The choices the team has for a wait, `reason` being the word the log uses for it (an
/// escalation's reason, or `blocked`, `acceptance` or `folder_change`). `flag` is always last.
#[must_use]
pub fn choices_for(
    reason: &str,
    before: Option<TaskStatus>,
    blocked_from: Option<TaskStatus>,
    spent: Spent,
) -> Vec<DecideChoice> {
    let carry_on =
        before.is_some_and(|b| !matches!(b, TaskStatus::Refining | TaskStatus::Escalated));
    let mut choices = match reason {
        "iterations" | "budget" | "sessions" if spent.grants >= TEAM_GRANTS_PER_TASK => {
            vec![DecideChoice::BackToPlan, DecideChoice::Cancel]
        }
        "iterations" => vec![
            DecideChoice::MoreTries,
            DecideChoice::BackToPlan,
            DecideChoice::Cancel,
        ],
        "budget" | "sessions" => {
            let mut choices = Vec::new();
            if before.is_some() {
                choices.push(DecideChoice::MoreRoom);
            }
            choices.push(DecideChoice::Cancel);
            choices
        }
        "blocker_age" | "blocked" => {
            let mut choices = vec![DecideChoice::AnswerBlock];
            if return_from_block(blocked_from) != TaskStatus::Refining {
                choices.push(DecideChoice::BackToPlan);
            }
            choices.push(DecideChoice::Cancel);
            choices
        }
        "readiness_failures" => vec![DecideChoice::TryReadinessAgain, DecideChoice::Cancel],
        "integration" => {
            if spent.reopens < REOPENS_PER_TASK {
                vec![DecideChoice::ReopenIntegration]
            } else {
                Vec::new()
            }
        }
        "approval" | "risk_gate" => vec![
            DecideChoice::ApprovePlan,
            DecideChoice::BackToPlan,
            DecideChoice::Cancel,
        ],
        "acceptance" => vec![DecideChoice::AcceptResult],
        "permission" | "explicit_request" => {
            let mut choices = Vec::new();
            if carry_on {
                choices.push(DecideChoice::CarryOn);
            }
            choices.push(DecideChoice::BackToPlan);
            choices.push(DecideChoice::Cancel);
            choices
        }
        "preview" => {
            let mut choices = Vec::new();
            if carry_on {
                choices.push(DecideChoice::CarryOn);
            }
            choices.push(DecideChoice::Cancel);
            choices
        }
        "folder_change" => {
            if spent.reopens < REOPENS_PER_TASK {
                vec![DecideChoice::RetryFolderChange]
            } else {
                Vec::new()
            }
        }
        _ => vec![DecideChoice::BackToPlan, DecideChoice::Cancel],
    };
    choices.push(DecideChoice::Flag);
    choices
}

/// What `choice` does to the task: `before` is the status an escalation came from, `blocked_from`
/// the one a block came from. `None` when the choice needs a `before` the task does not have.
#[must_use]
pub fn move_of(
    choice: DecideChoice,
    before: Option<TaskStatus>,
    blocked_from: Option<TaskStatus>,
) -> Option<DecideMove> {
    let to = |status, extra_tries, extra_room| {
        Some(DecideMove::To {
            status,
            extra_tries,
            extra_room,
        })
    };
    match choice {
        DecideChoice::MoreTries => to(TaskStatus::InProgress, Some(2), false),
        DecideChoice::MoreRoom => before.and_then(|status| to(status, None, true)),
        DecideChoice::BackToPlan | DecideChoice::TryReadinessAgain => {
            to(TaskStatus::Refining, None, false)
        }
        DecideChoice::AnswerBlock => to(return_from_block(blocked_from), None, false),
        DecideChoice::CarryOn => before.and_then(|status| to(status, None, false)),
        DecideChoice::Cancel => to(TaskStatus::Cancelled, None, false),
        DecideChoice::ReopenIntegration => Some(DecideMove::Reopen),
        DecideChoice::RetryFolderChange => Some(DecideMove::RetryFolderChange),
        DecideChoice::ApprovePlan => Some(DecideMove::ApprovePlan),
        DecideChoice::AcceptResult => Some(DecideMove::AcceptResult),
        DecideChoice::Flag => Some(DecideMove::Flag),
    }
}

/// The role that answers a team question: the Product Manager answers `product`; `technical` goes
/// to the Architect when the team has one and the asker is not it, else to the Product Manager.
#[must_use]
pub fn answerer(about: QuestionAbout, asker: Role, has_architect: bool) -> Role {
    match about {
        QuestionAbout::Technical if has_architect && asker != Role::Architect => Role::Architect,
        _ => Role::ProductManager,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecideChoice::{
            AcceptResult, AnswerBlock, ApprovePlan, BackToPlan, Cancel, CarryOn, Flag, MoreRoom,
            MoreTries, ReopenIntegration, RetryFolderChange, TryReadinessAgain,
        },
        DecideMove, QuestionAbout, Spent, answerer, choices_for, move_of,
    };
    use crate::contract::{Role, TaskStatus};

    fn none() -> Spent {
        Spent::default()
    }

    #[test]
    #[allow(clippy::too_many_lines, reason = "every reason's list, side by side")]
    fn offers_the_owners_choices_for_each_reason_within_the_teams_limits() {
        let ip = Some(TaskStatus::InProgress);
        let all = [
            (
                "iterations",
                None,
                None,
                none(),
                vec![MoreTries, BackToPlan, Cancel, Flag],
            ),
            ("budget", ip, None, none(), vec![MoreRoom, Cancel, Flag]),
            ("budget", None, None, none(), vec![Cancel, Flag]),
            (
                "blocked",
                None,
                ip,
                none(),
                vec![AnswerBlock, BackToPlan, Cancel, Flag],
            ),
            (
                "blocked",
                None,
                Some(TaskStatus::Refining),
                none(),
                vec![AnswerBlock, Cancel, Flag],
            ),
            (
                "readiness_failures",
                None,
                None,
                none(),
                vec![TryReadinessAgain, Cancel, Flag],
            ),
            (
                "integration",
                None,
                None,
                Spent {
                    grants: 0,
                    reopens: 2,
                },
                vec![ReopenIntegration, Flag],
            ),
            (
                "integration",
                None,
                None,
                Spent {
                    grants: 0,
                    reopens: 3,
                },
                vec![Flag],
            ),
            (
                "approval",
                None,
                None,
                none(),
                vec![ApprovePlan, BackToPlan, Cancel, Flag],
            ),
            ("acceptance", None, None, none(), vec![AcceptResult, Flag]),
            (
                "permission",
                ip,
                None,
                none(),
                vec![CarryOn, BackToPlan, Cancel, Flag],
            ),
            (
                "permission",
                Some(TaskStatus::Refining),
                None,
                none(),
                vec![BackToPlan, Cancel, Flag],
            ),
            ("preview", ip, None, none(), vec![CarryOn, Cancel, Flag]),
            (
                "folder_change",
                None,
                None,
                Spent {
                    grants: 0,
                    reopens: 0,
                },
                vec![RetryFolderChange, Flag],
            ),
            (
                "folder_change",
                None,
                None,
                Spent {
                    grants: 0,
                    reopens: 3,
                },
                vec![Flag],
            ),
            (
                "something_new",
                None,
                None,
                none(),
                vec![BackToPlan, Cancel, Flag],
            ),
            (
                "iterations",
                None,
                None,
                Spent {
                    grants: 3,
                    reopens: 0,
                },
                vec![BackToPlan, Cancel, Flag],
            ),
            (
                "sessions",
                ip,
                None,
                Spent {
                    grants: 3,
                    reopens: 0,
                },
                vec![BackToPlan, Cancel, Flag],
            ),
        ];
        for (reason, before, blocked_from, spent, expected) in all {
            let offered = choices_for(reason, before, blocked_from, spent);
            assert_eq!(
                offered, expected,
                "{reason} {before:?} {blocked_from:?} {spent:?}"
            );
            assert_eq!(offered.last(), Some(&Flag));
        }

        let to = |status, extra_tries, extra_room| {
            Some(DecideMove::To {
                status,
                extra_tries,
                extra_room,
            })
        };
        assert_eq!(
            move_of(MoreTries, None, None),
            to(TaskStatus::InProgress, Some(2), false)
        );
        assert_eq!(
            move_of(AnswerBlock, None, Some(TaskStatus::Verifying)),
            to(TaskStatus::Verifying, None, false)
        );
        assert_eq!(move_of(MoreRoom, None, None), None);
        assert_eq!(
            move_of(ReopenIntegration, None, None),
            Some(DecideMove::Reopen)
        );
        assert_eq!(
            move_of(ApprovePlan, None, None),
            Some(DecideMove::ApprovePlan)
        );
        assert_eq!(
            move_of(CarryOn, ip, None),
            to(TaskStatus::InProgress, None, false)
        );
    }

    #[test]
    fn asks_the_architect_technical_questions_unless_it_asks_or_is_missing() {
        let product = QuestionAbout::Product;
        let technical = QuestionAbout::Technical;
        assert_eq!(
            answerer(product, Role::SoftwareDeveloper, true),
            Role::ProductManager
        );
        assert_eq!(
            answerer(technical, Role::SoftwareDeveloper, true),
            Role::Architect
        );
        assert_eq!(
            answerer(technical, Role::SoftwareDeveloper, false),
            Role::ProductManager
        );
        assert_eq!(
            answerer(technical, Role::Architect, true),
            Role::ProductManager
        );
    }
}
