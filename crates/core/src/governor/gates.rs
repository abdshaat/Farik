//! The gate predicates of `docs/SPEC.md` section 5.2's transition table, with the contract-write
//! rules of 5.11 and the epic rules of 5.16, as pure functions over values the runtime passes in.
//! Step 09 composes them into one transition decision; each one here says everything that is
//! missing rather than only that something is.

use std::cmp::Ordering;

use crate::contract::{Role, TaskContract, TaskStatus, wire_method};
use crate::generated::task_contract::FarikTaskContractKind as Kind;
use crate::governor::done::{CriterionResult, RunBy};
use crate::governor::task_status::is_terminal;
use crate::governor::transition_table::TransitionActor;
use crate::text::{distinct, listed};

/// What a gate says: nothing when it passes, or every reason it does not, in the order the rules
/// are written.
pub type GateResult = Result<(), Vec<String>>;

fn verdict(reasons: Vec<String>) -> GateResult {
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
    }
}

fn is_written(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Whether a cost fits what is left. A figure that cannot be compared does not fit, as a spend
/// that is not a number counts as exhausted in `budget` (`docs/SPEC.md` section 5.5).
fn fits_within(cost: f64, remaining: f64) -> bool {
    matches!(
        cost.partial_cmp(&remaining),
        Some(Ordering::Less | Ordering::Equal)
    )
}

/// Who asked for a task to be assigned (`docs/SPEC.md` section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentRequester {
    /// The Scrum Master, which is the ordinary case.
    ScrumMaster,
    /// The Product Manager, which stands in only when the team has no active Scrum Master.
    ProductManager,
}

/// A dependency of the task being assigned, as the runtime found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyState {
    /// The dependency's task id, as the contract spells it.
    pub task_id: String,
    /// Its status.
    pub status: TaskStatus,
    /// Whether its branch has been integrated (`docs/SPEC.md` section 5.14).
    pub integrated: bool,
}

/// Everything the assignment gate needs from the world.
#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentInput {
    /// Who asked.
    pub requested_by: AssignmentRequester,
    /// Whether the team has an active Scrum Master.
    pub has_active_scrum_master: bool,
    /// The proposed assignee's agent id.
    pub assignee_id: String,
    /// The proposed assignee's role.
    pub assignee_role: Role,
    /// The proposed reviewer's agent id.
    pub reviewer_id: String,
    /// The proposed reviewer's role.
    pub reviewer_role: Role,
    /// How many tasks the proposed assignee already holds and has not finished: every task of its
    /// that is neither `accepted` nor `cancelled`. Counting only the started ones would let an
    /// agent be assigned any number of tasks and start none, because `assigned -> in_progress` has
    /// no gate; counting only the assigned, in-progress and blocked ones would let the limit be
    /// walked through `verifying` and `rejected`, and `rejected -> in_progress` hands a task
    /// straight back to the same agent.
    pub assignee_open_tasks: u32,
    /// The team's limit on work in progress per agent.
    pub wip_limit: u32,
    /// What is left of the sprint's budget, in dollars.
    pub remaining_sprint_budget_usd: f64,
    /// The state of every dependency the contract lists, in any order.
    pub dependencies: Vec<DependencyState>,
}

/// The `Assignment` gate of `ready -> assigned` (`docs/SPEC.md` sections 5.2, 5.14, 5.16): who may
/// ask, whether the pair of agents fits the contract, whether the agent has room, whether the
/// sprint can pay for it, and whether every dependency is accepted and integrated.
///
/// # Errors
///
/// Every rule the assignment fails, each saying what to change.
pub fn check_assignment(contract: &TaskContract, input: &AssignmentInput) -> GateResult {
    let mut reasons = Vec::new();
    match (input.requested_by, input.has_active_scrum_master) {
        (AssignmentRequester::ProductManager, true) => reasons.push(
            "the Product Manager assigns only when the team has no active Scrum Master".to_string(),
        ),
        (AssignmentRequester::ScrumMaster, false) => reasons.push(
            "this team has no active Scrum Master, so the Product Manager assigns".to_string(),
        ),
        _ => {}
    }
    let expected_assignee = if contract.kind == Kind::Epic {
        if input.has_active_scrum_master {
            Role::ScrumMaster
        } else {
            Role::ProductManager
        }
    } else {
        contract.assignee_role
    };
    if input.assignee_role != expected_assignee {
        reasons.push(format!(
            "the assignee has role {} and this contract is assigned to role {expected_assignee}",
            input.assignee_role
        ));
    }
    let expected_reviewer = if contract.kind == Kind::Epic {
        if input.has_active_scrum_master {
            Role::ProductManager
        } else {
            Role::Human
        }
    } else {
        contract.reviewer_role
    };
    if input.reviewer_role != expected_reviewer {
        reasons.push(format!(
            "the reviewer has role {} and this contract is reviewed by role {expected_reviewer}",
            input.reviewer_role
        ));
    }
    if input.assignee_id.trim().is_empty() {
        reasons.push(
            "the runtime named no agent for the assignee, and a task is assigned to one"
                .to_string(),
        );
    }
    // An epic the Product Manager broke down is reviewed by the human (5.16 item 4), and the human
    // is not an agent: there is no id to name, and nothing for the reviewer to be the assignee of.
    if expected_reviewer != Role::Human {
        if input.reviewer_id.trim().is_empty() {
            reasons.push(
                "the runtime named no agent for the reviewer, and a task is reviewed by one"
                    .to_string(),
            );
        } else if input.reviewer_id.trim() == input.assignee_id.trim() {
            reasons.push(format!(
                "{} cannot review its own work; name another agent as reviewer",
                input.assignee_id.trim()
            ));
        }
    }
    if input.assignee_open_tasks >= input.wip_limit {
        reasons.push(if input.wip_limit == 0 {
            format!(
                "{} takes no work: its limit is zero",
                input.assignee_id.trim()
            )
        } else {
            format!(
                "{} already holds {} unfinished tasks and the limit is {}",
                input.assignee_id.trim(),
                input.assignee_open_tasks,
                input.wip_limit
            )
        });
    }
    if !fits_within(
        contract.budget.max_cost_usd,
        input.remaining_sprint_budget_usd,
    ) {
        reasons.push(format!(
            "the budget of {} USD does not fit the {} USD left in the sprint",
            contract.budget.max_cost_usd, input.remaining_sprint_budget_usd
        ));
    }
    reasons.extend(unready_dependencies(contract, input));
    verdict(reasons)
}

fn unready_dependencies(contract: &TaskContract, input: &AssignmentInput) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for dependency in &contract.dependencies {
        let id = dependency.trim().to_string();
        if !named.contains(&id) {
            named.push(id);
        }
    }
    let mut reported: Vec<&str> = Vec::new();
    let mut twice: Vec<String> = Vec::new();
    for state in &input.dependencies {
        let id = state.task_id.trim();
        // Only the dependencies this contract names are its business: what the runtime says about
        // anything else decides nothing here, contradictory or not.
        if !named.iter().any(|dependency| dependency == id) {
            continue;
        }
        if reported.contains(&id) {
            twice.push(id.to_string());
        } else {
            reported.push(id);
        }
    }
    if !twice.is_empty() {
        return vec![format!(
            "the runtime reported {} twice, and a dependency has one state",
            distinct(&twice)
        )];
    }
    named
        .into_iter()
        .filter_map(|id| {
            if id == contract.id.to_string() {
                return Some(format!(
                    "{id} depends on itself, which nothing can satisfy"
                ));
            }
            match input
                .dependencies
                .iter()
                .find(|state| state.task_id.trim() == id)
            {
                None => Some(format!("the runtime reported nothing about {id}, which this task depends on")),
                Some(state) if state.status != TaskStatus::Accepted => Some(format!(
                    "{id} is {} and a dependency is assigned only once it is accepted",
                    state.status
                )),
                Some(state) if !state.integrated => Some(format!(
                    "{id} is accepted but not integrated, and this task's branch starts from the integration branch"
                )),
                Some(_) => None,
            }
        })
        .collect()
}

/// The state of the task's branch when the assignee declares it done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkState {
    /// How many commits the task branch carries.
    pub commits: u32,
    /// Whether the worktree has no uncommitted change.
    pub worktree_clean: bool,
}

/// The `CriteriaRecorded` gate of `in_progress -> verifying` for a task (`docs/SPEC.md` section
/// 5.2): every exit criterion the assignee can run has a result from its own run, and the branch
/// has at least one commit and a clean worktree. A `human` criterion is exempt, because the human
/// answers it, and so is a `review` one, which 5.3 gives to the reviewer; the Definition of Done
/// checks both.
///
/// # Errors
///
/// Every rule the work fails, each saying what is missing.
pub fn check_criteria_recorded(
    contract: &TaskContract,
    assignee_results: &[CriterionResult],
    work: &WorkState,
) -> GateResult {
    let mut reasons = Vec::new();
    let missing: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| {
            !matches!(
                wire_method(&criterion.verification),
                Some("human" | "review")
            )
        })
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            !assignee_results.iter().any(|result| {
                result.criterion_id.trim() == id
                    && result.run_by == RunBy::Assignee
                    && is_written(&result.evidence)
            })
        })
        .collect();
    if !missing.is_empty() {
        reasons.push(format!(
            "the assignee recorded no run with evidence for {}",
            listed("criterion", "criteria", &missing)
        ));
    }
    if work.commits == 0 {
        reasons.push("the task branch has no commit on it".to_string());
    }
    if !work.worktree_clean {
        reasons.push("the worktree has changes that are not committed".to_string());
    }
    verdict(reasons)
}

/// One task under an epic, as the runtime found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildState {
    /// The task's id.
    pub task_id: String,
    /// Its status.
    pub status: TaskStatus,
}

/// The `CriteriaRecorded` gate of `in_progress -> verifying` for an epic (`docs/SPEC.md` section
/// 5.16 item 4): every task under it is accepted or cancelled, and at least one is accepted.
///
/// # Errors
///
/// Every rule the epic fails, each naming the tasks that are not done.
pub fn check_children_done(children: &[ChildState]) -> GateResult {
    let mut reported: Vec<&str> = Vec::new();
    let mut twice: Vec<String> = Vec::new();
    for child in children {
        let named = child.task_id.trim();
        if named.is_empty() {
            continue;
        }
        if reported.contains(&named) {
            twice.push(named.to_string());
        } else {
            reported.push(named);
        }
    }
    if !twice.is_empty() {
        return Err(vec![format!(
            "the runtime reported {} twice, and a task under this epic has one state",
            distinct(&twice)
        )]);
    }
    let mut reasons = Vec::new();
    let unfinished: Vec<String> = children
        .iter()
        .filter(|child| !is_terminal(child.status))
        .map(|child| {
            let named = child.task_id.trim();
            if named.is_empty() {
                format!("a task the runtime did not name is {}", child.status)
            } else {
                format!("{named} is {}", child.status)
            }
        })
        .collect();
    if !unfinished.is_empty() {
        reasons.push(format!(
            "every task under this epic is accepted or cancelled first, and {}",
            unfinished.join("; ")
        ));
    }
    if !children
        .iter()
        .any(|child| child.status == TaskStatus::Accepted)
    {
        reasons.push(
            "no task under this epic was accepted, so there is nothing to verify".to_string(),
        );
    }
    verdict(reasons)
}

/// Whether the Product Manager may write a product document for this contract (`docs/SPEC.md`
/// section 5.16 items 1, 2 and 5): the writer is the Product Manager, the contract is an epic,
/// the user has approved the contract the epic has now, and the epic is not cancelled.
///
/// The approval is passed in rather than read from the status, because the status cannot answer
/// it: an epic waiting for approval sits in `escalated` (5.16 item 2), and so does one that
/// failed the Definition of Ready three times and one whose risk needs the human, neither of
/// which was ever approved. `user_approved` means the user approved **this** contract and the epic
/// has not been returned to `refining` since: a human edit of a frozen contract sends the epic
/// back to `refining` (5.11) and the approval it had does not carry over, because 5.16 item 2 asks
/// for it before the epic leaves `refining` each time. The return is what ends the approval, not
/// the session it starts, or a document could be written in the window between the two. The
/// approval therefore answers 5.16 item 1's "not yet `ready`" on its own, and the status is asked
/// only about `cancelled`: an epic escalated after its approval, for a budget or a permission,
/// keeps the documents it was approved for.
///
/// # Errors
///
/// Every rule the write fails.
pub fn check_product_doc_write(
    epic_kind: Kind,
    epic_status: TaskStatus,
    user_approved: bool,
    actor_role: Role,
) -> GateResult {
    let mut reasons = Vec::new();
    if actor_role != Role::ProductManager {
        reasons.push(format!(
            "role {actor_role} may not write a product document; the Product Manager owns them"
        ));
    }
    if epic_kind != Kind::Epic {
        reasons.push(
            "a product document is written for an epic the user approved, and this contract is a task"
                .to_string(),
        );
    }
    if !user_approved {
        reasons.push(
            "the user has not approved the contract this epic has now, and a product document waits for that"
                .to_string(),
        );
    }
    if epic_status == TaskStatus::Cancelled {
        reasons.push(
            "the epic is cancelled, and a product document would describe a decision the team abandoned"
                .to_string(),
        );
    }
    verdict(reasons)
}

/// Who is asking to write a contract, and which agent they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractWriteActor {
    /// Which of the lifecycle's actors this is.
    pub kind: TransitionActor,
    /// The agent's id, when an agent rather than the human or the governor.
    pub agent_id: Option<String>,
}

/// The state of the epic a task would be created under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentEpic {
    /// The epic's status.
    pub status: TaskStatus,
    /// The agent id the epic is assigned to.
    pub assignee_id: String,
}

/// Whether a task may be created under this epic (`docs/SPEC.md` section 5.16 item 3): only its
/// epic's assignee or the human, and only while the epic is in progress. The Definition of Ready
/// checks the task's paths and budget against the epic's; this checks who and when.
///
/// # Errors
///
/// Every rule the creation fails.
pub fn check_child_creation(parent: &ParentEpic, actor: &ContractWriteActor) -> GateResult {
    let mut reasons = Vec::new();
    let named = parent.assignee_id.trim();
    let is_the_assignee = !named.is_empty()
        && actor.kind != TransitionActor::Governor
        && actor.agent_id.as_deref().map(str::trim) == Some(named);
    if actor.kind != TransitionActor::Human && !is_the_assignee {
        reasons.push(format!(
            "a task under an epic is written by the epic's assignee, {}, or by the human",
            if named.is_empty() {
                "which the runtime did not name"
            } else {
                named
            }
        ));
    }
    if parent.status != TaskStatus::InProgress {
        reasons.push(format!(
            "the epic is {} and its tasks are written while it is in progress",
            parent.status
        ));
    }
    verdict(reasons)
}

/// What the assignee wrote when it blocked the task.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blocker {
    /// What is in the way.
    pub description: String,
    /// What would unblock it.
    pub needed: String,
}

/// The `BlockerWritten` gate of `in_progress -> blocked` (`docs/SPEC.md` section 5.2): a written
/// blocker that says what is in the way and what is needed.
///
/// # Errors
///
/// Every part of the blocker that is missing.
pub fn check_blocker_written(blocker: Option<&Blocker>) -> GateResult {
    let Some(blocker) = blocker else {
        return Err(vec![
            "a task is blocked with a written blocker: what is in the way and what is needed"
                .to_string(),
        ]);
    };
    let mut reasons = Vec::new();
    if !is_written(&blocker.description) {
        reasons.push("the blocker does not say what is in the way".to_string());
    }
    if !is_written(&blocker.needed) {
        reasons.push("the blocker does not say what is needed to clear it".to_string());
    }
    verdict(reasons)
}

/// The `BlockerResolved` gate of `blocked -> in_progress` (`docs/SPEC.md` section 5.2): a written
/// resolution, so that the next session knows what changed.
///
/// # Errors
///
/// One reason when nothing was written.
pub fn check_blocker_resolved(resolution: Option<&str>) -> GateResult {
    if resolution.is_some_and(is_written) {
        return Ok(());
    }
    Err(vec![
        "a blocker is cleared with a written resolution, so that the next session knows what changed"
            .to_string(),
    ])
}

/// What the reviewer wrote when it rejected the work.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rejection {
    /// The ids of the criteria that failed.
    pub failed_criterion_ids: Vec<String>,
    /// Why, in the reviewer's words.
    pub reasons: String,
}

/// The `RejectionReasons` gate of `verifying -> rejected` (`docs/SPEC.md` section 5.2): written
/// reasons mapped to criteria the contract actually has, so that the next iteration knows what to
/// fix.
///
/// # Errors
///
/// Every part of the rejection that is missing or does not name a criterion of this contract.
pub fn check_rejection_reasons(
    contract: &TaskContract,
    rejection: Option<&Rejection>,
) -> GateResult {
    let Some(rejection) = rejection else {
        return Err(vec![
            "work is rejected with written reasons mapped to the criteria that failed".to_string(),
        ]);
    };
    let mut reasons = Vec::new();
    if rejection.failed_criterion_ids.is_empty() {
        reasons.push("the rejection names no failed criterion".to_string());
    }
    if rejection
        .failed_criterion_ids
        .iter()
        .any(|id| id.trim().is_empty())
    {
        reasons.push("the rejection names a criterion with no id".to_string());
    }
    let unknown: Vec<String> = rejection
        .failed_criterion_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .filter(|id| {
            !contract
                .exit_criteria
                .iter()
                .any(|criterion| criterion.id.as_str() == id.as_str())
        })
        .collect();
    if !unknown.is_empty() {
        reasons.push(format!(
            "this contract has no {}",
            listed("criterion", "criteria", &unknown)
        ));
    }
    if !is_written(&rejection.reasons) {
        reasons.push("the rejection says nothing about why the criteria failed".to_string());
    }
    verdict(reasons)
}

/// What a write to a contract does when it is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractWriteOutcome {
    /// The write is applied and the task keeps its status.
    Allowed,
    /// The write is applied and the task goes back to `refining`, because the human changed a
    /// frozen contract (`docs/SPEC.md` section 5.11).
    ReturnsToRefining,
}

/// Why a write to a contract is refused (`docs/SPEC.md` section 5.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractWriteRefusal {
    /// The contract is locked, so it belongs to the human.
    ContractLocked,
    /// The contract is frozen and these fields are not among the few that still change.
    ContractFrozen {
        /// The fields the write would have changed that it may not.
        fields: Vec<String>,
    },
    /// The task is `accepted` or `cancelled`: no transition leaves those, so nothing in its
    /// contract changes again, not even for the human (`docs/SPEC.md` section 5.2).
    TaskTerminal {
        /// Which of the two it is.
        status: TaskStatus,
    },
    /// Someone asked to write fields that change only through a governed transition. They ask
    /// the governor for the transition instead (`docs/SPEC.md` section 5.2).
    LifecycleFields {
        /// The fields they may not write.
        fields: Vec<String>,
    },
    /// An agent asked to lock or unlock a contract, which only the human does
    /// (`docs/SPEC.md` section 5.11).
    HumansFields {
        /// The fields it may not write.
        fields: Vec<String>,
    },
    /// Someone asked to write a field the store owns: an identifier or a timestamp.
    StoresFields {
        /// The fields nobody writes here.
        fields: Vec<String>,
    },
    /// Someone asked to write a field that is fixed when a contract is created: its kind or its
    /// epic (`docs/SPEC.md` section 5.16).
    CreationFields {
        /// The fields nobody writes here.
        fields: Vec<String>,
    },
    /// This actor does not write a contract's content. The Product Manager writes a contract, an
    /// epic's assignee writes the tasks under it, and the human writes anything; the governor
    /// applies transitions, an assignee or a reviewer works to the contract it was given, and the
    /// Scrum Master writes the tasks under an epic but not the epic itself (6.2, 6.4, 5.16).
    ContentFields {
        /// The fields it may not write.
        fields: Vec<String>,
    },
    /// The write names fields this program does not know. A contract's fields are listed one by
    /// one, so that one added to the schema is refused until somebody says who writes it.
    UnknownFields {
        /// The names it did not recognise.
        fields: Vec<String>,
    },
}

/// The fields that still change once a contract is frozen (`docs/SPEC.md` section 5.11): the
/// five the governor writes when it applies a transition, and the notes. Published so that phase
/// 2's commands and phase 5's editor read the same list; the gate below reaches the same answer
/// from the sets each field belongs to.
pub const FIELDS_AFTER_FREEZE: [&str; 6] = [
    "status",
    "assignee",
    "reviewer",
    "iteration",
    "sprint",
    "notes",
];

/// The fields only the governor writes, at every status and whoever asks (`docs/SPEC.md` sections
/// 5.2 and 5.11). An actor asks for a transition and the governor applies it; an actor that could
/// write `status` itself would skip every gate, the Definition of Ready and the Definition of
/// Done included, and leave no `task.transitioned` event behind.
pub const FIELDS_THE_GOVERNOR_WRITES: [&str; 5] =
    ["status", "assignee", "reviewer", "iteration", "sprint"];

/// The fields only the human writes: the lock that makes a contract the human's
/// (`docs/SPEC.md` section 5.11, and the schema: "Set and cleared only by a human"). An agent
/// that could set it would take the contract from its own team until a human intervened.
pub const FIELDS_ONLY_THE_HUMAN_WRITES: [&str; 1] = ["locked"];

/// The fields the store owns and nobody writes through this path: the identifier it assigns and
/// the times it stamps.
pub const FIELDS_THE_STORE_OWNS: [&str; 4] = ["id", "created_by", "created_at", "updated_at"];

/// The fields fixed when a contract is created and never written afterwards: what kind of thing
/// it is, which the triage decided before refining started, and which epic it belongs to, which
/// the epic's assignee set when it wrote the task. Re-triage is its own tool and its own event
/// (`docs/SPEC.md` section 5.16); clearing `parent` here would take a task out of its epic and
/// past the three checks the Definition of Ready makes against it.
pub const FIELDS_FIXED_AT_CREATION: [&str; 2] = ["kind", "parent"];

/// The contract's content: what the task is, written by the Product Manager, by an epic's
/// assignee writing the tasks under it, and by the human. Written out rather than left as
/// whatever is not in the other sets, so that a field added to the schema is refused until
/// somebody says who writes it.
pub const FIELDS_OF_THE_CONTENT: [&str; 13] = [
    "title",
    "intent",
    "scope",
    "requirements",
    "exit_criteria",
    "constraints",
    "dependencies",
    "references",
    "assignee_role",
    "reviewer_role",
    "risk",
    "budget",
    "allowed_paths",
];

/// The fields the note tools write, which stay open whatever the contract's status: its notes,
/// and nothing else. Criterion results are events rather than contract fields, so nothing here
/// covers them.
pub const FIELDS_ALWAYS_WRITABLE: [&str; 1] = ["notes"];

/// Whether a write to a contract is allowed (`docs/SPEC.md` sections 5.2, 5.11 and 5.16). `kind`
/// is what the contract is, an epic or a task, and `status` is the task's status now, before
/// whatever the write is part of.
///
/// This gate is about changing a contract that exists. Creating one is `check_child_creation`'s
/// question: a create names `kind` and `parent`, which are fixed at creation and which this gate
/// refuses, so a caller that routed a create through here would make every creation impossible.
///
/// Every one of the schema's fields is written into one of six sets, and a name in none of them is
/// refused rather than guessed at, so that a field added to the schema tomorrow waits until
/// somebody says who writes it:
///
/// - `FIELDS_ALWAYS_WRITABLE`, the notes, are the note tools' and stay open at every status to
///   everyone;
/// - `FIELDS_THE_GOVERNOR_WRITES` are the governor's, at every status and whoever asks, because an
///   actor that could write them would move a task past its own gates, and `sprint` is among them
///   because planning works from the ready backlog;
/// - `FIELDS_ONLY_THE_HUMAN_WRITES`, the lock, is the human's, because an agent that could set it
///   would take the contract from its own team;
/// - `FIELDS_THE_STORE_OWNS`, the identifier and the stamps, are nobody's here;
/// - `FIELDS_FIXED_AT_CREATION`, the kind and the parent, are nobody's here either: the triage
///   decided the kind and its own tool changes it, and clearing the parent would take a task out
///   of its epic (5.16);
/// - `FIELDS_OF_THE_CONTENT` are written by the Product Manager and the human on either kind, and
///   by the Scrum Master on a task but never on an epic, which 6.2 forbids and 5.16 item 3 asks of
///   it for the tasks underneath. An assignee or a reviewer works to the contract it was given
///   (6.4), and the governor applies transitions rather than deciding what a task is.
///
/// A task that is `accepted` or `cancelled` is finished: only the note tools still write to it,
/// the human included, because a human write of a frozen contract is defined by sending the task
/// back to `refining` and nothing leaves those two statuses. A locked contract's content is the
/// human's alone. A contract is frozen once its task leaves `refining`, so from `ready` onward its
/// content changes only through the human, and that sends the task back to `refining`; locking is
/// not a content write and does not.
///
/// Refusals are reported most structural first: a name nobody knows, then a finished task, then
/// the store's fields, then the ones fixed at creation, then the governor's, then the human's,
/// then who may write content, then the lock, then the freeze. Each earlier one holds whatever the
/// later ones say, so a caller that fixes what it is told makes progress rather than meeting the
/// same wall under another name.
///
/// # Errors
///
/// `UnknownFields`, `TaskTerminal`, `StoresFields`, `CreationFields`, `LifecycleFields`,
/// `HumansFields`, `ContentFields`, `ContractLocked`, or `ContractFrozen`, in that order of
/// precedence, each naming the fields at fault where there are fields to name.
pub fn check_contract_write(
    kind: Kind,
    status: TaskStatus,
    locked: bool,
    actor: &ContractWriteActor,
    changed_fields: &[String],
) -> Result<ContractWriteOutcome, ContractWriteRefusal> {
    let unknown = beyond(
        changed_fields,
        &[
            FIELDS_ALWAYS_WRITABLE.as_slice(),
            FIELDS_THE_GOVERNOR_WRITES.as_slice(),
            FIELDS_ONLY_THE_HUMAN_WRITES.as_slice(),
            FIELDS_THE_STORE_OWNS.as_slice(),
            FIELDS_FIXED_AT_CREATION.as_slice(),
            FIELDS_OF_THE_CONTENT.as_slice(),
        ]
        .concat(),
    );
    if !unknown.is_empty() {
        return Err(ContractWriteRefusal::UnknownFields { fields: unknown });
    }
    let beyond_notes = beyond(changed_fields, &FIELDS_ALWAYS_WRITABLE);
    if is_terminal(status) {
        if beyond_notes.is_empty() {
            return Ok(ContractWriteOutcome::Allowed);
        }
        return Err(ContractWriteRefusal::TaskTerminal { status });
    }
    let stores = within(changed_fields, &FIELDS_THE_STORE_OWNS);
    if !stores.is_empty() {
        return Err(ContractWriteRefusal::StoresFields { fields: stores });
    }
    let creation = within(changed_fields, &FIELDS_FIXED_AT_CREATION);
    if !creation.is_empty() {
        return Err(ContractWriteRefusal::CreationFields { fields: creation });
    }
    let lifecycle = within(changed_fields, &FIELDS_THE_GOVERNOR_WRITES);
    if actor.kind != TransitionActor::Governor && !lifecycle.is_empty() {
        return Err(ContractWriteRefusal::LifecycleFields { fields: lifecycle });
    }
    let humans = within(changed_fields, &FIELDS_ONLY_THE_HUMAN_WRITES);
    if actor.kind != TransitionActor::Human && !humans.is_empty() {
        return Err(ContractWriteRefusal::HumansFields { fields: humans });
    }
    let content = within(changed_fields, &FIELDS_OF_THE_CONTENT);
    if content.is_empty() {
        return Ok(ContractWriteOutcome::Allowed);
    }
    let writes_content = match actor.kind {
        TransitionActor::Human | TransitionActor::ProductManager => true,
        TransitionActor::ScrumMaster => kind != Kind::Epic,
        _ => false,
    };
    if !writes_content {
        return Err(ContractWriteRefusal::ContentFields { fields: content });
    }
    let frozen = !matches!(status, TaskStatus::Draft | TaskStatus::Refining);
    if actor.kind == TransitionActor::Human {
        if frozen {
            return Ok(ContractWriteOutcome::ReturnsToRefining);
        }
        return Ok(ContractWriteOutcome::Allowed);
    }
    if locked {
        return Err(ContractWriteRefusal::ContractLocked);
    }
    if frozen {
        return Err(ContractWriteRefusal::ContractFrozen { fields: content });
    }
    Ok(ContractWriteOutcome::Allowed)
}

/// The changed fields that are not in the given set, in the order they were given and without
/// repeats: a refusal that names one field twice reads as two problems where there is one.
fn beyond(changed_fields: &[String], open: &[&str]) -> Vec<String> {
    once_each(
        changed_fields
            .iter()
            .filter(|field| !open.contains(&field.as_str())),
    )
}

/// The changed fields that are in the given set, in the order they were given and without repeats.
fn within(changed_fields: &[String], set: &[&str]) -> Vec<String> {
    once_each(
        changed_fields
            .iter()
            .filter(|field| set.contains(&field.as_str())),
    )
}

fn once_each<'a>(fields: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for field in fields {
        if !kept.contains(field) {
            kept.push(field.clone());
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AssignmentInput, AssignmentRequester, Blocker, ChildState, ContractWriteActor,
        ContractWriteOutcome, ContractWriteRefusal, DependencyState, FIELDS_AFTER_FREEZE,
        FIELDS_ALWAYS_WRITABLE, FIELDS_FIXED_AT_CREATION, FIELDS_OF_THE_CONTENT,
        FIELDS_ONLY_THE_HUMAN_WRITES, FIELDS_THE_GOVERNOR_WRITES, FIELDS_THE_STORE_OWNS,
        ParentEpic, Rejection, WorkState, check_assignment, check_blocker_resolved,
        check_blocker_written, check_child_creation, check_children_done, check_contract_write,
        check_criteria_recorded, check_product_doc_write, check_rejection_reasons,
    };
    use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
    use crate::generated::task_contract::ExitCriterionVerificationVariant0Expect;
    use crate::generated::task_contract::FarikTaskContractKind as Kind;
    use crate::governor::done::{CriterionResult, RunBy};
    use crate::governor::readiness::fixtures::a_contract;
    use crate::governor::task_status::TASK_STATUSES;
    use crate::governor::transition_table::TransitionActor;

    fn an_assignment() -> AssignmentInput {
        AssignmentInput {
            requested_by: AssignmentRequester::ScrumMaster,
            has_active_scrum_master: true,
            assignee_id: "dev-1".to_string(),
            assignee_role: Role::SoftwareDeveloper,
            reviewer_id: "arch-1".to_string(),
            reviewer_role: Role::Architect,
            assignee_open_tasks: 0,
            wip_limit: 2,
            remaining_sprint_budget_usd: 15.0,
            dependencies: Vec::new(),
        }
    }

    fn reasons(result: super::GateResult) -> Vec<String> {
        result.expect_err("expected a refusal")
    }

    fn an_assignee_result(criterion_id: &str) -> CriterionResult {
        CriterionResult {
            criterion_id: criterion_id.to_string(),
            passed: true,
            evidence: "cargo test: 11 passed".to_string(),
            run_by: RunBy::Assignee,
        }
    }

    fn a_depending_contract() -> TaskContract {
        let mut contract = a_contract();
        contract.dependencies = vec!["FRK-2".parse().expect("a dependency id")];
        contract
    }

    #[test]
    fn assigns_a_task_to_an_agent_of_its_role_with_a_reviewer_of_its_own() {
        assert_eq!(check_assignment(&a_contract(), &an_assignment()), Ok(()));
    }

    #[test]
    fn lets_the_product_manager_assign_only_without_a_scrum_master() {
        let mut input = an_assignment();
        input.requested_by = AssignmentRequester::ProductManager;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["the Product Manager assigns only when the team has no active Scrum Master"]
        );
        input.has_active_scrum_master = false;
        assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
    }

    #[test]
    fn refuses_an_assignee_whose_role_is_not_the_contracts() {
        let mut input = an_assignment();
        input.assignee_role = Role::MarketingSpecialist;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            [
                "the assignee has role marketing_specialist and this contract is assigned to role software_developer"
            ]
        );
    }

    #[test]
    fn assigns_an_epic_to_the_scrum_master_or_the_product_manager_without_one() {
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        let mut input = an_assignment();
        input.reviewer_role = Role::ProductManager;
        assert_eq!(
            reasons(check_assignment(&epic, &input)),
            [
                "the assignee has role software_developer and this contract is assigned to role scrum_master"
            ]
        );
        input.assignee_role = Role::ScrumMaster;
        assert_eq!(check_assignment(&epic, &input), Ok(()));
        input.has_active_scrum_master = false;
        input.requested_by = AssignmentRequester::ProductManager;
        assert_eq!(
            reasons(check_assignment(&epic, &input)),
            [
                "the assignee has role scrum_master and this contract is assigned to role product_manager",
                "the reviewer has role product_manager and this contract is reviewed by role human"
            ]
        );
        input.assignee_role = Role::ProductManager;
        input.reviewer_role = Role::Human;
        assert_eq!(check_assignment(&epic, &input), Ok(()));
    }

    #[test]
    fn reviews_an_epic_with_the_product_manager_or_the_human_and_never_the_contracts_role() {
        // Project plan D7 and spec 5.16 item 4: the Scrum Master breaks an epic down and the
        // Product Manager reviews it; without a Scrum Master the Product Manager breaks it down
        // and the human reviews it. The contract's own reviewer_role does not decide an epic.
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        let mut input = an_assignment();
        input.assignee_role = Role::ScrumMaster;
        assert_eq!(
            reasons(check_assignment(&epic, &input)),
            [
                "the reviewer has role architect and this contract is reviewed by role product_manager"
            ]
        );
        input.has_active_scrum_master = false;
        input.requested_by = AssignmentRequester::ProductManager;
        input.assignee_role = Role::ProductManager;
        assert_eq!(
            reasons(check_assignment(&epic, &input)),
            ["the reviewer has role architect and this contract is reviewed by role human"]
        );
    }

    #[test]
    fn asks_for_no_reviewer_agent_when_the_human_reviews_the_epic() {
        // An epic broken down by the Product Manager is reviewed by the human (5.16 item 4), and
        // the human is not an agent: there is no id for the runtime to pass and nothing for the
        // reviewer to be the assignee of.
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        let mut input = an_assignment();
        input.requested_by = AssignmentRequester::ProductManager;
        input.has_active_scrum_master = false;
        input.assignee_id = "pm-1".to_string();
        input.assignee_role = Role::ProductManager;
        input.reviewer_id = String::new();
        input.reviewer_role = Role::Human;
        assert_eq!(check_assignment(&epic, &input), Ok(()));
        // The assignee is still named, and a task whose reviewer is an agent still needs its id.
        input.assignee_id = "  ".to_string();
        assert_eq!(
            reasons(check_assignment(&epic, &input)),
            ["the runtime named no agent for the assignee, and a task is assigned to one"]
        );
    }

    #[test]
    fn lets_the_scrum_master_assign_only_when_the_team_has_one() {
        // Spec 5.2's row gives the trigger to the Scrum Master, or to the Product Manager when
        // the team has no active Scrum Master. There is no row for a Scrum Master without one.
        let mut input = an_assignment();
        input.has_active_scrum_master = false;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["this team has no active Scrum Master, so the Product Manager assigns"]
        );
    }

    #[test]
    fn refuses_an_assignment_the_runtime_did_not_name_two_agents_for() {
        // Each blank is its own reason, so that a caller with one of the two missing is not told
        // to look at both.
        let missing_assignee =
            "the runtime named no agent for the assignee, and a task is assigned to one"
                .to_string();
        let missing_reviewer =
            "the runtime named no agent for the reviewer, and a task is reviewed by one"
                .to_string();
        for (assignee, reviewer, expected) in [
            ("", "arch-1", vec![missing_assignee.clone()]),
            ("dev-1", " ", vec![missing_reviewer.clone()]),
            (
                "",
                "",
                vec![missing_assignee.clone(), missing_reviewer.clone()],
            ),
        ] {
            let mut input = an_assignment();
            input.assignee_id = assignee.to_string();
            input.reviewer_id = reviewer.to_string();
            assert_eq!(
                reasons(check_assignment(&a_contract(), &input)),
                expected,
                "{assignee} {reviewer}"
            );
        }
    }

    #[test]
    fn refuses_a_reviewer_of_the_wrong_role_or_the_assignee_itself() {
        let mut input = an_assignment();
        input.reviewer_role = Role::MarketingSpecialist;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            [
                "the reviewer has role marketing_specialist and this contract is reviewed by role architect"
            ]
        );
        let mut input = an_assignment();
        input.reviewer_id = "dev-1".to_string();
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["dev-1 cannot review its own work; name another agent as reviewer"]
        );
    }

    #[test]
    fn reads_an_agent_id_the_same_however_it_is_padded() {
        let mut input = an_assignment();
        input.reviewer_id = " dev-1 ".to_string();
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["dev-1 cannot review its own work; name another agent as reviewer"]
        );
        let parent = ParentEpic {
            status: TaskStatus::InProgress,
            assignee_id: " sm-1 ".to_string(),
        };
        let padded = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("sm-1".to_string()),
        };
        assert_eq!(check_child_creation(&parent, &padded), Ok(()));
        // And the other way round: the actor the runtime named with a stray space is the same
        // agent, or an epic's own assignee could be refused by it.
        let named = ParentEpic {
            status: TaskStatus::InProgress,
            assignee_id: "sm-1".to_string(),
        };
        let padded_actor = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some(" sm-1 ".to_string()),
        };
        assert_eq!(check_child_creation(&named, &padded_actor), Ok(()));
    }

    #[test]
    fn refuses_an_agent_that_already_holds_its_limit_of_unfinished_tasks() {
        let mut input = an_assignment();
        input.assignee_open_tasks = 1;
        input.wip_limit = 2;
        assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
        input.assignee_open_tasks = 2;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["dev-1 already holds 2 unfinished tasks and the limit is 2"]
        );
        // A limit of zero is how a team pauses an agent, and the refusal says that rather than
        // counting to zero.
        input.assignee_open_tasks = 0;
        input.wip_limit = 0;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["dev-1 takes no work: its limit is zero"]
        );
    }

    #[test]
    fn refuses_a_budget_the_sprint_cannot_pay_and_one_that_is_not_a_number() {
        let mut input = an_assignment();
        input.remaining_sprint_budget_usd = 5.0;
        assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
        input.remaining_sprint_budget_usd = 4.99;
        assert_eq!(
            reasons(check_assignment(&a_contract(), &input)),
            ["the budget of 5 USD does not fit the 4.99 USD left in the sprint"]
        );
        input.remaining_sprint_budget_usd = f64::NAN;
        assert_eq!(reasons(check_assignment(&a_contract(), &input)).len(), 1);
    }

    #[test]
    fn assigns_a_task_only_once_every_dependency_is_accepted_and_integrated() {
        let contract = a_depending_contract();
        let mut input = an_assignment();
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            ["the runtime reported nothing about FRK-2, which this task depends on"]
        );
        for status in [
            TaskStatus::Ready,
            TaskStatus::Verifying,
            TaskStatus::Rejected,
        ] {
            input.dependencies = vec![DependencyState {
                task_id: "FRK-2".to_string(),
                status,
                integrated: true,
            }];
            assert_eq!(
                reasons(check_assignment(&contract, &input)),
                [format!(
                    "FRK-2 is {status} and a dependency is assigned only once it is accepted"
                )],
                "{status}"
            );
        }
        input.dependencies = vec![DependencyState {
            task_id: "FRK-2".to_string(),
            status: TaskStatus::Verifying,
            integrated: false,
        }];
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            ["FRK-2 is verifying and a dependency is assigned only once it is accepted"]
        );
        input.dependencies[0].status = TaskStatus::Accepted;
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            [
                "FRK-2 is accepted but not integrated, and this task's branch starts from the integration branch"
            ]
        );
        input.dependencies[0].integrated = true;
        assert_eq!(check_assignment(&contract, &input), Ok(()));
        // Two dependencies in two different states are both reported, in the order the contract
        // lists them: an agent sent back learns everything it has to wait for at once.
        let mut two = a_depending_contract();
        two.dependencies
            .push("FRK-3".parse().expect("a dependency id"));
        input.dependencies = vec![
            DependencyState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::Verifying,
                integrated: false,
            },
            DependencyState {
                task_id: "FRK-3".to_string(),
                status: TaskStatus::Accepted,
                integrated: false,
            },
        ];
        assert_eq!(
            reasons(check_assignment(&two, &input)),
            [
                "FRK-2 is verifying and a dependency is assigned only once it is accepted",
                "FRK-3 is accepted but not integrated, and this task's branch starts from the integration branch"
            ]
        );
        // A padded dependency id is the same dependency.
        input.dependencies = vec![DependencyState {
            task_id: " FRK-2 ".to_string(),
            status: TaskStatus::Accepted,
            integrated: true,
        }];
        assert_eq!(check_assignment(&contract, &input), Ok(()));
    }

    #[test]
    fn names_a_dependency_once_however_often_the_contract_lists_it() {
        let mut contract = a_depending_contract();
        contract.dependencies = vec![
            "FRK-2".parse().expect("a dependency id"),
            "FRK-2".parse().expect("a dependency id"),
        ];
        assert_eq!(
            reasons(check_assignment(&contract, &an_assignment())),
            ["the runtime reported nothing about FRK-2, which this task depends on"]
        );
    }

    #[test]
    fn refuses_more_than_one_report_about_one_dependency() {
        // A dependency has one state; two reports mean the runtime is confused, and taking the
        // first would make the answer depend on the order they arrived in. However many reports
        // arrive, the dependency is named once: three would otherwise read as three problems.
        let contract = a_depending_contract();
        let mut input = an_assignment();
        input.dependencies = vec![
            DependencyState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::Accepted,
                integrated: true,
            },
            DependencyState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::InProgress,
                integrated: false,
            },
        ];
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            ["the runtime reported FRK-2 twice, and a dependency has one state"]
        );
        input.dependencies.push(DependencyState {
            task_id: " FRK-2 ".to_string(),
            status: TaskStatus::Blocked,
            integrated: false,
        });
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            ["the runtime reported FRK-2 twice, and a dependency has one state"]
        );
        // Only the dependencies this contract names are its business: two reports about a task it
        // does not depend on say nothing about whether it may be assigned.
        let mut unrelated = an_assignment();
        unrelated.dependencies = vec![
            DependencyState {
                task_id: "FRK-99".to_string(),
                status: TaskStatus::Accepted,
                integrated: true,
            },
            DependencyState {
                task_id: "FRK-99".to_string(),
                status: TaskStatus::Draft,
                integrated: false,
            },
        ];
        assert_eq!(check_assignment(&a_contract(), &unrelated), Ok(()));
    }

    #[test]
    fn refuses_a_task_that_depends_on_itself() {
        let mut contract = a_contract();
        contract.dependencies = vec![contract.id.to_string().parse().expect("a dependency id")];
        let mut input = an_assignment();
        input.dependencies = vec![DependencyState {
            task_id: contract.id.to_string(),
            status: TaskStatus::Accepted,
            integrated: true,
        }];
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            ["FRK-1 depends on itself, which nothing can satisfy"]
        );
    }

    #[test]
    fn reports_every_assignment_rule_that_fails_at_once() {
        let mut input = an_assignment();
        input.requested_by = AssignmentRequester::ProductManager;
        input.assignee_role = Role::MarketingSpecialist;
        input.reviewer_id = "dev-1".to_string();
        input.assignee_open_tasks = 9;
        input.remaining_sprint_budget_usd = 0.0;
        let contract = a_depending_contract();
        // The whole list, in the order the rules are written: counting the reasons would leave
        // the order free, and an agent reads them in the order it is given them.
        assert_eq!(
            reasons(check_assignment(&contract, &input)),
            [
                "the Product Manager assigns only when the team has no active Scrum Master",
                "the assignee has role marketing_specialist and this contract is assigned to role software_developer",
                "dev-1 cannot review its own work; name another agent as reviewer",
                "dev-1 already holds 9 unfinished tasks and the limit is 2",
                "the budget of 5 USD does not fit the 0 USD left in the sprint",
                "the runtime reported nothing about FRK-2, which this task depends on"
            ]
        );
    }

    #[test]
    fn declares_done_when_the_assignee_ran_every_criterion_and_committed() {
        let work = WorkState {
            commits: 1,
            worktree_clean: true,
        };
        assert_eq!(
            check_criteria_recorded(&a_contract(), &[an_assignee_result("C1")], &work),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_criterion_the_assignee_did_not_run_itself_or_ran_without_evidence() {
        let work = WorkState {
            commits: 1,
            worktree_clean: true,
        };
        assert_eq!(
            reasons(check_criteria_recorded(&a_contract(), &[], &work)),
            ["the assignee recorded no run with evidence for criterion C1"]
        );
        let mut reviewers = an_assignee_result("C1");
        reviewers.run_by = RunBy::Reviewer;
        assert_eq!(
            reasons(check_criteria_recorded(&a_contract(), &[reviewers], &work)).len(),
            1
        );
        let mut blank = an_assignee_result("C1");
        blank.evidence = "  ".to_string();
        assert_eq!(
            reasons(check_criteria_recorded(&a_contract(), &[blank], &work)).len(),
            1
        );
        // Two criteria nobody ran are named together and in the plural: a gate that reported only
        // the first would send the assignee back twice for one mistake.
        let mut two = a_contract();
        let one = two.exit_criteria[0].clone();
        two.exit_criteria.push(one);
        two.exit_criteria[1].id = "C2".parse().expect("a criterion id");
        assert_eq!(
            reasons(check_criteria_recorded(&two, &[], &work)),
            ["the assignee recorded no run with evidence for criteria C1, C2"]
        );
        // A padded criterion id is the same criterion, as a padded agent id is the same agent.
        assert_eq!(
            check_criteria_recorded(&a_contract(), &[an_assignee_result(" C1 ")], &work),
            Ok(())
        );
    }

    #[test]
    fn takes_a_recorded_failure_as_a_run_because_the_reviewer_decides() {
        // Spec 5.2 asks for a recorded result, not a passing one: an assignee that records its
        // failure is telling the truth, and demanding a pass here would reward one that does not.
        let work = WorkState {
            commits: 1,
            worktree_clean: true,
        };
        let mut failed = an_assignee_result("C1");
        failed.passed = false;
        assert_eq!(
            check_criteria_recorded(&a_contract(), &[failed], &work),
            Ok(())
        );
    }

    #[test]
    fn does_not_ask_the_assignee_to_run_a_criterion_only_the_human_can_answer() {
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant4 {
            method: json!("human"),
            question: "Did you sign in successfully?".to_string(),
        };
        let work = WorkState {
            commits: 1,
            worktree_clean: true,
        };
        assert_eq!(check_criteria_recorded(&contract, &[], &work), Ok(()));
    }

    #[test]
    fn does_not_ask_the_assignee_to_run_the_reviewers_rubric() {
        // Spec 5.3 gives a `review` criterion to the reviewer, as a `human` one goes to the human.
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant3 {
            method: json!("review"),
            rubric: vec!["Does the form say why a password was refused?".to_string()],
        };
        let work = WorkState {
            commits: 1,
            worktree_clean: true,
        };
        assert_eq!(check_criteria_recorded(&contract, &[], &work), Ok(()));
        // Only those two are exempt: a command is the assignee's to run, as a test is.
        let mut runnable = a_contract();
        runnable.exit_criteria[0].verification = VerificationWire::Variant0 {
            method: json!("command"),
            command: "pnpm check".to_string(),
            expect: ExitCriterionVerificationVariant0Expect::default(),
        };
        assert_eq!(
            reasons(check_criteria_recorded(&runnable, &[], &work)),
            ["the assignee recorded no run with evidence for criterion C1"]
        );
        // A verification whose `method` is not a string names no method, so it is not one of the
        // two the assignee is excused: an unreadable criterion stays its to run.
        let mut unreadable = a_contract();
        unreadable.exit_criteria[0].verification = VerificationWire::Variant3 {
            method: json!(7),
            rubric: vec!["Does the form say why a password was refused?".to_string()],
        };
        assert_eq!(
            reasons(check_criteria_recorded(&unreadable, &[], &work)),
            ["the assignee recorded no run with evidence for criterion C1"]
        );
        let mut produced = a_contract();
        produced.exit_criteria[0].verification = VerificationWire::Variant2 {
            method: json!("artifact"),
            must_contain: Vec::new(),
            path: "docs/marketing/release-notes.md".to_string(),
        };
        assert_eq!(
            reasons(check_criteria_recorded(&produced, &[], &work)),
            ["the assignee recorded no run with evidence for criterion C1"]
        );
    }

    #[test]
    fn refuses_work_with_no_commit_or_an_unclean_worktree() {
        let results = [an_assignee_result("C1")];
        assert_eq!(
            reasons(check_criteria_recorded(
                &a_contract(),
                &results,
                &WorkState {
                    commits: 0,
                    worktree_clean: true
                }
            )),
            ["the task branch has no commit on it"]
        );
        assert_eq!(
            reasons(check_criteria_recorded(
                &a_contract(),
                &results,
                &WorkState {
                    commits: 2,
                    worktree_clean: false
                }
            )),
            ["the worktree has changes that are not committed"]
        );
        // All three rules failing at once report all three, in the order they are written.
        assert_eq!(
            reasons(check_criteria_recorded(
                &a_contract(),
                &[],
                &WorkState {
                    commits: 0,
                    worktree_clean: false
                }
            )),
            [
                "the assignee recorded no run with evidence for criterion C1",
                "the task branch has no commit on it",
                "the worktree has changes that are not committed"
            ]
        );
    }

    #[test]
    fn verifies_an_epic_when_its_tasks_are_done_and_at_least_one_was_accepted() {
        let done = [
            ChildState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::Accepted,
            },
            ChildState {
                task_id: "FRK-3".to_string(),
                status: TaskStatus::Cancelled,
            },
        ];
        assert_eq!(check_children_done(&done), Ok(()));
        for status in [
            TaskStatus::Verifying,
            TaskStatus::Escalated,
            TaskStatus::Rejected,
        ] {
            let mut unfinished = done.clone();
            unfinished[1].status = status;
            assert_eq!(
                reasons(check_children_done(&unfinished)),
                [format!(
                    "every task under this epic is accepted or cancelled first, and FRK-3 is {status}"
                )],
                "{status}"
            );
        }
        let mut running = done.clone();
        running[1].status = TaskStatus::InProgress;
        assert_eq!(
            reasons(check_children_done(&running)),
            ["every task under this epic is accepted or cancelled first, and FRK-3 is in_progress"]
        );
        // Every unfinished task is named, not the first: the epic's assignee is told what is left.
        let mut two_left = done.clone();
        two_left[0].status = TaskStatus::InProgress;
        two_left[1].status = TaskStatus::Blocked;
        assert_eq!(
            reasons(check_children_done(&two_left)),
            [
                "every task under this epic is accepted or cancelled first, and FRK-2 is in_progress; FRK-3 is blocked",
                "no task under this epic was accepted, so there is nothing to verify"
            ]
        );
        let cancelled = [ChildState {
            task_id: "FRK-2".to_string(),
            status: TaskStatus::Cancelled,
        }];
        assert_eq!(
            reasons(check_children_done(&cancelled)),
            ["no task under this epic was accepted, so there is nothing to verify"]
        );
        assert_eq!(reasons(check_children_done(&[])).len(), 1);
        // Two reports about one task mean the runtime is confused, as they do for a dependency:
        // an epic must not verify on a report that contradicts itself.
        let twice = [
            ChildState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::Accepted,
            },
            ChildState {
                task_id: " FRK-2 ".to_string(),
                status: TaskStatus::Cancelled,
            },
        ];
        assert_eq!(
            reasons(check_children_done(&twice)),
            ["the runtime reported FRK-2 twice, and a task under this epic has one state"]
        );
        let unnamed = [ChildState {
            task_id: "  ".to_string(),
            status: TaskStatus::Draft,
        }];
        assert_eq!(
            reasons(check_children_done(&unnamed)),
            [
                "every task under this epic is accepted or cancelled first, and a task the runtime did not name is draft",
                "no task under this epic was accepted, so there is nothing to verify"
            ]
        );
    }

    #[test]
    fn writes_a_product_document_only_for_the_product_manager_and_an_approved_epic() {
        assert_eq!(
            check_product_doc_write(Kind::Epic, TaskStatus::Ready, true, Role::ProductManager),
            Ok(())
        );
        assert_eq!(
            check_product_doc_write(
                Kind::Epic,
                TaskStatus::InProgress,
                true,
                Role::ProductManager
            ),
            Ok(())
        );
        assert_eq!(
            reasons(check_product_doc_write(
                Kind::Epic,
                TaskStatus::Ready,
                true,
                Role::SoftwareDeveloper
            )),
            [
                "role software_developer may not write a product document; the Product Manager owns them"
            ]
        );
        for status in [
            TaskStatus::Assigned,
            TaskStatus::Verifying,
            TaskStatus::Accepted,
        ] {
            assert_eq!(
                check_product_doc_write(Kind::Epic, status, true, Role::ProductManager),
                Ok(()),
                "{status}"
            );
        }
        // All four rules failing at once report all four, in the order they are written.
        assert_eq!(
            reasons(check_product_doc_write(
                Kind::Task,
                TaskStatus::Cancelled,
                false,
                Role::SoftwareDeveloper
            )),
            [
                "role software_developer may not write a product document; the Product Manager owns them",
                "a product document is written for an epic the user approved, and this contract is a task",
                "the user has not approved the contract this epic has now, and a product document waits for that",
                "the epic is cancelled, and a product document would describe a decision the team abandoned"
            ]
        );
    }

    #[test]
    fn waits_for_the_approval_itself_and_for_a_status_that_cannot_be_stale() {
        // An epic waiting for approval sits in `escalated`, and so does one that failed the
        // Definition of Ready three times and one whose risk needs the human: none of the three
        // was approved, and the status cannot tell them apart. The approval is its own fact, and
        // the status is checked as well, because either one alone can be out of date.
        for status in TASK_STATUSES {
            assert!(
                reasons(check_product_doc_write(
                    Kind::Epic,
                    status,
                    false,
                    Role::ProductManager
                ))
                .contains(
                    &"the user has not approved the contract this epic has now, and a product document waits for that"
                        .to_string()
                ),
                "{status}"
            );
        }
        // An epic escalated after its approval, for a budget or a permission, keeps the documents
        // it was approved for; only a cancelled one is refused on its status.
        assert_eq!(
            check_product_doc_write(
                Kind::Epic,
                TaskStatus::Escalated,
                true,
                Role::ProductManager
            ),
            Ok(())
        );
        assert_eq!(
            reasons(check_product_doc_write(
                Kind::Epic,
                TaskStatus::Cancelled,
                true,
                Role::ProductManager
            )),
            [
                "the epic is cancelled, and a product document would describe a decision the team abandoned"
            ]
        );
    }

    #[test]
    fn writes_a_product_document_for_an_epic_and_never_for_a_standalone_task() {
        // Under the team policy `human_accepts_contracts: all` every contract is accepted by the
        // human, so the approval alone does not say this one is an epic.
        assert_eq!(
            reasons(check_product_doc_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                Role::ProductManager
            )),
            [
                "a product document is written for an epic the user approved, and this contract is a task"
            ]
        );
    }

    #[test]
    fn writes_a_task_under_an_epic_only_as_its_assignee_or_the_human() {
        let parent = ParentEpic {
            status: TaskStatus::InProgress,
            assignee_id: "sm-1".to_string(),
        };
        let assignee = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("sm-1".to_string()),
        };
        assert_eq!(check_child_creation(&parent, &assignee), Ok(()));
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        assert_eq!(check_child_creation(&parent, &human), Ok(()));
        // The governor carries no agent id of its own, and a caller that filled the epic
        // assignee's in would be describing something else.
        let governor = ContractWriteActor {
            kind: TransitionActor::Governor,
            agent_id: Some("sm-1".to_string()),
        };
        assert_eq!(
            reasons(check_child_creation(&parent, &governor)),
            ["a task under an epic is written by the epic's assignee, sm-1, or by the human"]
        );
        let other = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        assert_eq!(
            reasons(check_child_creation(&parent, &other)),
            ["a task under an epic is written by the epic's assignee, sm-1, or by the human"]
        );
        let unnamed = ParentEpic {
            status: TaskStatus::InProgress,
            assignee_id: String::new(),
        };
        let nobody = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some(String::new()),
        };
        assert_eq!(
            reasons(check_child_creation(&unnamed, &nobody)),
            [
                "a task under an epic is written by the epic's assignee, which the runtime did not name, or by the human"
            ]
        );
        let not_started = ParentEpic {
            status: TaskStatus::Ready,
            assignee_id: "sm-1".to_string(),
        };
        assert_eq!(
            reasons(check_child_creation(&not_started, &assignee)),
            ["the epic is ready and its tasks are written while it is in progress"]
        );
        // Both rules failing at once report both, in the order they are written.
        assert_eq!(
            reasons(check_child_creation(&not_started, &other)),
            [
                "a task under an epic is written by the epic's assignee, sm-1, or by the human",
                "the epic is ready and its tasks are written while it is in progress"
            ]
        );
    }

    #[test]
    fn blocks_a_task_only_with_a_written_blocker() {
        let blocker = Blocker {
            description: "The staging database refuses the migration.".to_string(),
            needed: "A password for the staging database.".to_string(),
        };
        assert_eq!(check_blocker_written(Some(&blocker)), Ok(()));
        assert_eq!(
            reasons(check_blocker_written(None)),
            ["a task is blocked with a written blocker: what is in the way and what is needed"]
        );
        let blank = Blocker {
            description: " ".to_string(),
            needed: "\n".to_string(),
        };
        assert_eq!(
            reasons(check_blocker_written(Some(&blank))),
            [
                "the blocker does not say what is in the way",
                "the blocker does not say what is needed to clear it"
            ]
        );
    }

    #[test]
    fn clears_a_blocker_only_with_a_written_resolution() {
        assert_eq!(
            check_blocker_resolved(Some("The password is in the vault.")),
            Ok(())
        );
        for nothing in [None, Some(""), Some("\n")] {
            assert_eq!(
                reasons(check_blocker_resolved(nothing)),
                [
                    "a blocker is cleared with a written resolution, so that the next session knows what changed"
                ]
            );
        }
    }

    #[test]
    fn rejects_work_only_with_reasons_mapped_to_criteria_the_contract_has() {
        let rejection = Rejection {
            failed_criterion_ids: vec!["C1".to_string()],
            reasons: "The form accepts an empty password.".to_string(),
        };
        assert_eq!(
            check_rejection_reasons(&a_contract(), Some(&rejection)),
            Ok(())
        );
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), None)),
            ["work is rejected with written reasons mapped to the criteria that failed"]
        );
        let empty = Rejection {
            failed_criterion_ids: Vec::new(),
            reasons: "Something is wrong.".to_string(),
        };
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), Some(&empty))),
            ["the rejection names no failed criterion"]
        );
        let unnamed = Rejection {
            failed_criterion_ids: vec![" ".to_string()],
            reasons: "The form accepts an empty password.".to_string(),
        };
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), Some(&unnamed))),
            ["the rejection names a criterion with no id"]
        );
        let unknown = Rejection {
            failed_criterion_ids: vec!["C9".to_string()],
            reasons: " ".to_string(),
        };
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), Some(&unknown))),
            [
                "this contract has no criterion C9",
                "the rejection says nothing about why the criteria failed"
            ]
        );
        let two_unknown = Rejection {
            failed_criterion_ids: vec!["C8".to_string(), "C9".to_string()],
            reasons: "The form accepts an empty password.".to_string(),
        };
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), Some(&two_unknown))),
            ["this contract has no criteria C8, C9"]
        );
        // One id that happens to contain a comma is still one id: the plural is decided by how
        // many values there are, not by what is inside them.
        let comma = Rejection {
            failed_criterion_ids: vec!["C8, C9".to_string()],
            reasons: "The form accepts an empty password.".to_string(),
        };
        assert_eq!(
            reasons(check_rejection_reasons(&a_contract(), Some(&comma))),
            ["this contract has no criterion C8, C9"]
        );
    }

    #[test]
    fn lets_an_agent_write_a_contract_that_is_neither_locked_nor_frozen() {
        let agent = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        for status in [TaskStatus::Draft, TaskStatus::Refining] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    status,
                    false,
                    &agent,
                    &["intent".to_string(), "exit_criteria".to_string()]
                ),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
        }
    }

    #[test]
    fn gives_every_field_of_the_schema_to_exactly_one_owner() {
        // The gate's answer is the six sets, so a field in two of them, or in none, is a hole. The
        // schema is the list: it forbids properties it does not name.
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../generated/task_contract.schema.json"))
                .expect("the generated schema copy");
        let mut declared: Vec<&str> = schema["properties"]
            .as_object()
            .expect("an object of properties")
            .keys()
            .map(String::as_str)
            .collect();
        declared.sort_unstable();
        let mut owned: Vec<&str> = [
            FIELDS_ALWAYS_WRITABLE.as_slice(),
            FIELDS_THE_GOVERNOR_WRITES.as_slice(),
            FIELDS_ONLY_THE_HUMAN_WRITES.as_slice(),
            FIELDS_THE_STORE_OWNS.as_slice(),
            FIELDS_FIXED_AT_CREATION.as_slice(),
            FIELDS_OF_THE_CONTENT.as_slice(),
        ]
        .concat();
        let before = owned.len();
        owned.sort_unstable();
        owned.dedup();
        assert_eq!(owned.len(), before, "a field is in two sets at once");
        assert_eq!(owned, declared);
        // And the published set is the governor's plus the notes, nothing else.
        let mut after_freeze: Vec<&str> = FIELDS_AFTER_FREEZE.to_vec();
        after_freeze.sort_unstable();
        let mut expected: Vec<&str> = [
            FIELDS_THE_GOVERNOR_WRITES.as_slice(),
            FIELDS_ALWAYS_WRITABLE.as_slice(),
        ]
        .concat();
        expected.sort_unstable();
        assert_eq!(after_freeze, expected);
    }

    #[test]
    fn keeps_an_epics_contract_out_of_its_scrum_masters_hands() {
        // Spec 6.2: the Scrum Master cannot change an epic's contract. Spec 5.16 item 3: it
        // writes the tasks under one. `draft` and `refining` are exactly when an epic's contract
        // is written, so the freeze does not draw this line and the contract's kind has to.
        let scrum_master = ContractWriteActor {
            kind: TransitionActor::ScrumMaster,
            agent_id: Some("sm-1".to_string()),
        };
        for status in [TaskStatus::Draft, TaskStatus::Refining] {
            assert_eq!(
                check_contract_write(
                    Kind::Epic,
                    status,
                    false,
                    &scrum_master,
                    &["requirements".to_string()]
                ),
                Err(ContractWriteRefusal::ContentFields {
                    fields: vec!["requirements".to_string()]
                }),
                "{status}"
            );
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    status,
                    false,
                    &scrum_master,
                    &["requirements".to_string()]
                ),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
            // The Product Manager writes an epic: 5.16 item 1 gives it that work, so the rule
            // is about the Scrum Master and not about epics being unwritable.
            let author = ContractWriteActor {
                kind: TransitionActor::ProductManager,
                agent_id: Some("pm-1".to_string()),
            };
            assert_eq!(
                check_contract_write(
                    Kind::Epic,
                    status,
                    false,
                    &author,
                    &["requirements".to_string()]
                ),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
        }
    }

    #[test]
    fn refuses_a_field_this_program_has_given_to_nobody() {
        // The classes are written out rather than left as a residue, so a field added to the
        // schema tomorrow is refused until somebody says who writes it.
        let author = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Draft,
                false,
                &author,
                &["intent".to_string(), "human_acceptance".to_string()]
            ),
            Err(ContractWriteRefusal::UnknownFields {
                fields: vec!["human_acceptance".to_string()]
            })
        );
        // The names are the schema's own, exactly: a path or a padded name is not one of them.
        for name in [
            " status",
            "notes ",
            "notes.completion",
            "budget.max_cost_usd",
        ] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::Draft,
                    false,
                    &author,
                    &[name.to_string()]
                ),
                Err(ContractWriteRefusal::UnknownFields {
                    fields: vec![name.to_string()]
                }),
                "{name}"
            );
        }
    }

    #[test]
    fn fixes_what_a_contract_is_and_whose_it_is_when_it_is_created() {
        // The triage decides `kind` before refining starts and its own tool changes it; the
        // epic's assignee sets `parent` when it writes the task. Clearing `parent` here would
        // take a task out of its epic and past the three checks readiness makes against it.
        for kind in [
            TransitionActor::ProductManager,
            TransitionActor::ScrumMaster,
            TransitionActor::Governor,
            TransitionActor::Human,
        ] {
            let actor = ContractWriteActor {
                kind,
                agent_id: Some("agent-1".to_string()),
            };
            for field in ["kind", "parent"] {
                assert_eq!(
                    check_contract_write(
                        Kind::Task,
                        TaskStatus::Draft,
                        false,
                        &actor,
                        &[field.to_string()]
                    ),
                    Err(ContractWriteRefusal::CreationFields {
                        fields: vec![field.to_string()]
                    }),
                    "{kind:?} {field}"
                );
            }
        }
    }

    #[test]
    fn lets_the_human_lock_a_contract_without_unreadying_it() {
        // A user who writes an epic themselves locks it and approves it (5.16). Locking a `ready`
        // epic must not send it back to `refining` and lose the approval it just got.
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        for status in [TaskStatus::Ready, TaskStatus::InProgress] {
            assert_eq!(
                check_contract_write(Kind::Epic, status, false, &human, &["locked".to_string()]),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
        }
    }

    #[test]
    fn names_the_fields_that_still_change_after_the_freeze() {
        // Spec 5.11 lists them, and the governor writes each one on a frozen contract: `status`
        // and `iteration` on a rejection, `assignee` and `reviewer` at assignment, `sprint` at
        // planning from the ready backlog.
        assert_eq!(
            FIELDS_AFTER_FREEZE,
            [
                "status",
                "assignee",
                "reviewer",
                "iteration",
                "sprint",
                "notes"
            ]
        );
        let governor = ContractWriteActor {
            kind: TransitionActor::Governor,
            agent_id: None,
        };
        for field in FIELDS_AFTER_FREEZE {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::InProgress,
                    true,
                    &governor,
                    &[field.to_string()]
                ),
                Ok(ContractWriteOutcome::Allowed),
                "{field}"
            );
        }
    }

    #[test]
    fn names_two_refused_content_fields_and_names_each_once() {
        let assignee = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Draft,
                false,
                &assignee,
                &[
                    "scope".to_string(),
                    "budget".to_string(),
                    "scope".to_string()
                ]
            ),
            Err(ContractWriteRefusal::ContentFields {
                fields: vec!["scope".to_string(), "budget".to_string()]
            })
        );
    }

    #[test]
    fn keeps_the_lock_the_humans_and_the_identifiers_the_stores() {
        // A deny-list over a wire format grows a hole every time the schema grows a field, so
        // every field belongs to someone and the rest is refused.
        for kind in [
            TransitionActor::ProductManager,
            TransitionActor::ScrumMaster,
            TransitionActor::Assignee,
            TransitionActor::Reviewer,
            TransitionActor::Governor,
        ] {
            let actor = ContractWriteActor {
                kind,
                agent_id: Some("agent-1".to_string()),
            };
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::Refining,
                    false,
                    &actor,
                    &["locked".to_string()]
                ),
                Err(ContractWriteRefusal::HumansFields {
                    fields: vec!["locked".to_string()]
                }),
                "{kind:?}"
            );
        }
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Refining,
                false,
                &human,
                &["locked".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        for field in ["id", "created_by", "created_at", "updated_at"] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::Refining,
                    false,
                    &human,
                    &[field.to_string()]
                ),
                Err(ContractWriteRefusal::StoresFields {
                    fields: vec![field.to_string()]
                }),
                "{field}"
            );
        }
    }

    #[test]
    fn leaves_a_contracts_content_to_the_product_manager_an_epics_assignee_and_the_human() {
        // Spec 6.4: a Developer cannot modify contracts. Spec 6.2: the Scrum Master cannot change
        // an epic's contract, which the freeze enforces, while 5.16 item 3 has it write the tasks
        // under an epic, which is a contract in `draft`.
        for field in ["intent", "exit_criteria", "allowed_paths", "risk", "budget"] {
            for kind in [TransitionActor::Assignee, TransitionActor::Reviewer] {
                let actor = ContractWriteActor {
                    kind,
                    agent_id: Some("dev-1".to_string()),
                };
                assert_eq!(
                    check_contract_write(
                        Kind::Task,
                        TaskStatus::Draft,
                        false,
                        &actor,
                        &[field.to_string()]
                    ),
                    Err(ContractWriteRefusal::ContentFields {
                        fields: vec![field.to_string()]
                    }),
                    "{field} {kind:?}"
                );
            }
            for kind in [
                TransitionActor::ProductManager,
                TransitionActor::ScrumMaster,
                TransitionActor::Human,
            ] {
                let actor = ContractWriteActor {
                    kind,
                    agent_id: Some("agent-1".to_string()),
                };
                assert_eq!(
                    check_contract_write(
                        Kind::Task,
                        TaskStatus::Draft,
                        false,
                        &actor,
                        &[field.to_string()]
                    ),
                    Ok(ContractWriteOutcome::Allowed),
                    "{field} {kind:?}"
                );
            }
        }
    }

    #[test]
    fn freezes_a_contracts_content_from_ready_onward() {
        // Spec 5.11 draws the line at "from `ready` onward", which is the status the freeze turns
        // on at, so that is the status to check it at.
        let author = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        for status in [TaskStatus::Draft, TaskStatus::Refining] {
            assert_eq!(
                check_contract_write(Kind::Task, status, false, &author, &["scope".to_string()]),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
            assert_eq!(
                check_contract_write(Kind::Task, status, false, &human, &["scope".to_string()]),
                Ok(ContractWriteOutcome::Allowed),
                "{status}"
            );
        }
        for status in [TaskStatus::Ready, TaskStatus::InProgress] {
            assert_eq!(
                check_contract_write(Kind::Task, status, false, &author, &["scope".to_string()]),
                Err(ContractWriteRefusal::ContractFrozen {
                    fields: vec!["scope".to_string()]
                }),
                "{status}"
            );
            assert_eq!(
                check_contract_write(Kind::Task, status, false, &human, &["scope".to_string()]),
                Ok(ContractWriteOutcome::ReturnsToRefining),
                "{status}"
            );
        }
    }

    #[test]
    fn never_lets_anyone_but_the_governor_move_a_task_by_writing_its_status() {
        // An agent asks for a transition and the governor applies it. An agent that could write
        // `status` itself would set `ready` without the Definition of Ready, or `accepted`
        // without anything at all, and the contract is not frozen while it is being written.
        for kind in [
            TransitionActor::ProductManager,
            TransitionActor::ScrumMaster,
            TransitionActor::Assignee,
            TransitionActor::Reviewer,
            TransitionActor::Human,
        ] {
            let actor = ContractWriteActor {
                kind,
                agent_id: Some("agent-1".to_string()),
            };
            for status in [TaskStatus::Draft, TaskStatus::Refining, TaskStatus::Ready] {
                assert_eq!(
                    check_contract_write(
                        Kind::Task,
                        status,
                        false,
                        &actor,
                        &["intent".to_string(), "status".to_string()]
                    ),
                    Err(ContractWriteRefusal::LifecycleFields {
                        fields: vec!["status".to_string()]
                    }),
                    "{kind:?} at {status}"
                );
            }
        }
    }

    #[test]
    fn keeps_a_contracts_content_out_of_the_governors_hands() {
        // The governor applies transitions; it does not decide what a task is. Saying so with a
        // reason of its own keeps the log honest: a draft contract is not frozen.
        let governor = ContractWriteActor {
            kind: TransitionActor::Governor,
            agent_id: None,
        };
        for status in [TaskStatus::Draft, TaskStatus::Refining, TaskStatus::Ready] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    status,
                    false,
                    &governor,
                    &["intent".to_string()]
                ),
                Err(ContractWriteRefusal::ContentFields {
                    fields: vec!["intent".to_string()]
                }),
                "{status}"
            );
        }
    }

    #[test]
    fn reports_a_structural_refusal_before_one_about_who_owns_the_field() {
        // Each earlier reason holds whatever the later ones say, so the order is decided rather
        // than left to the order of the branches. This half of it: a name the schema does not
        // have, a finished task, the store's fields, then the ones fixed at creation.
        let agent = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        // finished beats everything
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Accepted,
                true,
                &agent,
                &["status".to_string(), "scope".to_string()]
            ),
            Err(ContractWriteRefusal::TaskTerminal {
                status: TaskStatus::Accepted
            })
        );
        // a finished task beats the store's fields
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Accepted,
                false,
                &agent,
                &["id".to_string()]
            ),
            Err(ContractWriteRefusal::TaskTerminal {
                status: TaskStatus::Accepted
            })
        );
        // a name nobody knows beats a finished task
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Accepted,
                false,
                &agent,
                &["human_acceptance".to_string()]
            ),
            Err(ContractWriteRefusal::UnknownFields {
                fields: vec!["human_acceptance".to_string()]
            })
        );
        // the store's fields beat the ones fixed at creation
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Draft,
                false,
                &agent,
                &["id".to_string(), "kind".to_string()]
            ),
            Err(ContractWriteRefusal::StoresFields {
                fields: vec!["id".to_string()]
            })
        );
        // the ones fixed at creation beat the governor's
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Draft,
                false,
                &agent,
                &["kind".to_string(), "status".to_string()]
            ),
            Err(ContractWriteRefusal::CreationFields {
                fields: vec!["kind".to_string()]
            })
        );
        // the store's fields beat the governor's
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                &agent,
                &["id".to_string(), "status".to_string()]
            ),
            Err(ContractWriteRefusal::StoresFields {
                fields: vec!["id".to_string()]
            })
        );
    }

    #[test]
    fn reports_the_owner_of_a_field_before_the_lock_and_the_freeze() {
        // The rest of the same order, on a field the schema names, a task that is not finished,
        // and no structural set: whose field it is decides, and only then the lock and the
        // freeze.
        let agent = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        // the governor's fields beat the human's, the content rule, the lock and the freeze
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                &agent,
                &[
                    "status".to_string(),
                    "locked".to_string(),
                    "scope".to_string()
                ]
            ),
            Err(ContractWriteRefusal::LifecycleFields {
                fields: vec!["status".to_string()]
            })
        );
        // the human's field beats who may write content
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Draft,
                false,
                &agent,
                &["locked".to_string(), "scope".to_string()]
            ),
            Err(ContractWriteRefusal::HumansFields {
                fields: vec!["locked".to_string()]
            })
        );
        // who may write content beats the lock and the freeze
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                &agent,
                &["scope".to_string()]
            ),
            Err(ContractWriteRefusal::ContentFields {
                fields: vec!["scope".to_string()]
            })
        );
        // and for an actor that may write content, the lock beats the freeze
        let author = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                &author,
                &["scope".to_string()]
            ),
            Err(ContractWriteRefusal::ContractLocked)
        );
    }

    #[test]
    fn keeps_a_locked_contract_for_the_human_but_leaves_the_notes_open() {
        let agent = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Refining,
                true,
                &agent,
                &["notes".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Refining,
                true,
                &agent,
                &["intent".to_string()]
            ),
            Err(ContractWriteRefusal::ContractLocked)
        );
        // For an actor that never writes content the lock is not what stops it, so the reason
        // says which rule it met rather than which it would have met next.
        let assignee = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Refining,
                true,
                &assignee,
                &["intent".to_string()]
            ),
            Err(ContractWriteRefusal::ContentFields {
                fields: vec!["intent".to_string()]
            })
        );
    }

    #[test]
    fn freezes_a_contract_once_its_task_leaves_refining() {
        // The fields that still change from `ready` onward change through a governed transition,
        // so the governor writes them and an agent writes only the notes (spec 5.11, 5.2).
        let agent = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        let governor = ContractWriteActor {
            kind: TransitionActor::Governor,
            agent_id: None,
        };
        for field in ["status", "assignee", "reviewer", "iteration", "notes"] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::Ready,
                    false,
                    &governor,
                    &[field.to_string()]
                ),
                Ok(ContractWriteOutcome::Allowed),
                "{field}"
            );
        }
        for field in ["status", "assignee", "reviewer", "iteration"] {
            for status in [
                TaskStatus::Draft,
                TaskStatus::Refining,
                TaskStatus::Ready,
                TaskStatus::InProgress,
            ] {
                assert_eq!(
                    check_contract_write(Kind::Task, status, false, &agent, &[field.to_string()]),
                    Err(ContractWriteRefusal::LifecycleFields {
                        fields: vec![field.to_string()]
                    }),
                    "{field} at {status}"
                );
            }
        }
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                false,
                &agent,
                &["notes".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        let author = ContractWriteActor {
            kind: TransitionActor::ProductManager,
            agent_id: Some("pm-1".to_string()),
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::InProgress,
                false,
                &author,
                &[
                    "notes".to_string(),
                    "scope".to_string(),
                    "budget".to_string()
                ]
            ),
            Err(ContractWriteRefusal::ContractFrozen {
                fields: vec!["scope".to_string(), "budget".to_string()]
            })
        );
    }

    #[test]
    fn lets_the_governor_move_a_locked_contract() {
        // A lock keeps a contract's content for the human; it does not stop the lifecycle, or a
        // locked task could never leave refining.
        let governor = ContractWriteActor {
            kind: TransitionActor::Governor,
            agent_id: None,
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Ready,
                true,
                &governor,
                &["status".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        // A lock is not what stops the governor writing a contract's content; nothing it does
        // ever writes content, so the reason is the same whether or not the contract is locked.
        for locked in [true, false] {
            assert_eq!(
                check_contract_write(
                    Kind::Task,
                    TaskStatus::Refining,
                    locked,
                    &governor,
                    &["intent".to_string()]
                ),
                Err(ContractWriteRefusal::ContentFields {
                    fields: vec!["intent".to_string()]
                })
            );
        }
    }

    #[test]
    fn writes_nothing_but_notes_to_a_task_that_is_finished() {
        // `accepted` and `cancelled` are terminal (spec 5.2): a human write that would otherwise
        // send the task back to `refining` has nowhere to send it.
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        let agent = ContractWriteActor {
            kind: TransitionActor::Assignee,
            agent_id: Some("dev-1".to_string()),
        };
        for status in [TaskStatus::Accepted, TaskStatus::Cancelled] {
            for actor in [&human, &agent] {
                assert_eq!(
                    check_contract_write(Kind::Task, status, false, actor, &["notes".to_string()]),
                    Ok(ContractWriteOutcome::Allowed),
                    "{status}"
                );
                assert_eq!(
                    check_contract_write(Kind::Task, status, false, actor, &["scope".to_string()]),
                    Err(ContractWriteRefusal::TaskTerminal { status }),
                    "{status}"
                );
            }
        }
    }

    #[test]
    fn allows_a_write_that_changes_no_field_at_all() {
        // The runtime asks before it writes; a call that names no field is not a change and is
        // not a refusal either.
        for locked in [true, false] {
            for kind in [
                TransitionActor::Assignee,
                TransitionActor::Governor,
                TransitionActor::Human,
            ] {
                let actor = ContractWriteActor {
                    kind,
                    agent_id: None,
                };
                assert_eq!(
                    check_contract_write(Kind::Task, TaskStatus::InProgress, locked, &actor, &[]),
                    Ok(ContractWriteOutcome::Allowed)
                );
            }
        }
    }

    #[test]
    fn sends_a_task_back_to_refining_when_the_human_changes_a_frozen_contract() {
        let human = ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        };
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Refining,
                true,
                &human,
                &["intent".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Verifying,
                false,
                &human,
                &["notes".to_string()]
            ),
            Ok(ContractWriteOutcome::Allowed)
        );
        assert_eq!(
            check_contract_write(
                Kind::Task,
                TaskStatus::Verifying,
                true,
                &human,
                &["scope".to_string()]
            ),
            Ok(ContractWriteOutcome::ReturnsToRefining)
        );
    }
}
