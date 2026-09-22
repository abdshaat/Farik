//! A tool's refusals in the words an agent reads: each starts with its kind in `snake_case`, then
//! `: `, then what it says, so that an agent and a test can both tell which rule refused.

use farik_core::contract::{Role, TaskStatus};
use farik_core::criteria::CriteriaError;
use farik_core::governor::gates::ContractWriteRefusal;
use farik_core::governor::permissions::{CommandRefusal, ToolRefusal};
use farik_core::team::AgentStatus;

use super::ToolError;

/// Every way a tool refuses, one variant each.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Refusal {
    /// The caller is not an active agent of the team.
    AgentNotActive {
        agent_id: String,
        status: Option<AgentStatus>,
    },
    /// The board has no such task.
    NoSuchTask { task_id: String },
    /// The tier or path check of 5.6 refused.
    Tool(ToolRefusal),
    /// The tool acts on the session's task and the session has none.
    NoTask,
    /// A triage was asked with no reason.
    BlankReason,
    /// This caller may not triage this request now (5.16).
    TriageNotAllowed {
        role: Role,
        status: TaskStatus,
        has_parent: bool,
        triaged: bool,
    },
    /// The caller is neither a contract-writing role nor the task's assignee or reviewer.
    NotAContractWriter { agent_id: String, task_id: String },
    /// An epic's contract waits for the human's answer.
    QuestionUnanswered { task_id: String },
    /// `check_contract_write` refused.
    ContractWrite(ContractWriteRefusal),
    /// The contract the write would leave breaks the contract's rules.
    ContractInvalid { details: Vec<String> },
    /// A criterion could not be taken from the library.
    Criteria(CriteriaError),
    /// A parent that is not an epic.
    NotAnEpic { task_id: String },
    /// A gate refused, with every reason.
    GateFailed { details: Vec<String> },
    /// The store would not file the request.
    RequestRefused { reason: String },
    /// The caller is none of the actors the move's rows name.
    ActorNotAllowed { detail: String },
    /// The caller is neither the assignee nor the reviewer.
    NotTheRunner { agent_id: String },
    /// The contract has no such criterion.
    UnknownCriterion { criterion_id: String },
    /// This note is another's to write.
    NotTheNotesWriter { agent_id: String, kind: String },
    /// `evaluate_command` refused.
    Command(CommandRefusal),
    /// A command's directory is outside the workspace.
    OutsideWorkspace { cwd: String },
}

impl Refusal {
    /// The kind, then what it says.
    fn reason(&self) -> String {
        let (kind, detail) = match self {
            Self::AgentNotActive { agent_id, status } => (
                "agent_not_active",
                match status {
                    Some(status) => {
                        format!("{agent_id} is {status}, and only an active agent works")
                    }
                    None => format!("{agent_id} is not an agent of this team"),
                },
            ),
            Self::NoSuchTask { task_id } => ("no_such_task", format!("the board has no {task_id}")),
            Self::Tool(refusal) => tool(refusal),
            Self::NoTask => (
                "no_task",
                "this tool acts on the session's task, and this session has none".to_string(),
            ),
            Self::BlankReason => (
                "blank_reason",
                "a triage is recorded with a reason, and the log is where somebody reads it back"
                    .to_string(),
            ),
            Self::TriageNotAllowed {
                role,
                status,
                has_parent,
                triaged,
            } => (
                "triage_not_allowed",
                triage(*role, *status, *has_parent, *triaged),
            ),
            Self::NotAContractWriter { agent_id, task_id } => (
                "not_a_contract_writer",
                format!(
                    "{agent_id} is not the Product Manager, the Scrum Master, or {task_id}'s \
                     assignee or reviewer"
                ),
            ),
            Self::QuestionUnanswered { task_id } => (
                "question_unanswered",
                format!(
                    "a question about {task_id} is waiting for the human, and the epic's contract \
                     waits for the answer (5.16)"
                ),
            ),
            Self::ContractWrite(refusal) => contract_write(refusal),
            Self::ContractInvalid { details } => ("contract_invalid", details.join("; ")),
            Self::Criteria(error) => ("criterion_not_expanded", error.to_string()),
            Self::NotAnEpic { task_id } => (
                "not_an_epic",
                format!("{task_id} is a task, and only an epic has tasks under it"),
            ),
            Self::GateFailed { details } => ("gate_failed", details.join("; ")),
            Self::RequestRefused { reason } => ("request_refused", format!("the request {reason}")),
            Self::ActorNotAllowed { detail } => ("actor_not_allowed", detail.clone()),
            Self::NotTheRunner { agent_id } => (
                "not_the_runner",
                format!(
                    "{agent_id} is neither the assignee nor the reviewer, and a result is recorded \
                     by the agent that ran it"
                ),
            ),
            Self::UnknownCriterion { criterion_id } => (
                "unknown_criterion",
                format!("the contract has no criterion {criterion_id}"),
            ),
            Self::Command(refusal) => command(refusal),
            Self::OutsideWorkspace { cwd } => (
                "outside_workspace",
                format!("{cwd} is not a directory inside the task's workspace"),
            ),
            Self::NotTheNotesWriter { agent_id, kind } => (
                "not_the_notes_writer",
                format!(
                    "a {kind} note is written by {}, and {agent_id} is not",
                    match kind.as_str() {
                        "completion" => "the assignee",
                        "review" => "the reviewer",
                        _ => "the assignee or the reviewer",
                    }
                ),
            ),
        };
        format!("{kind}: {detail}")
    }
}

fn tool(refusal: &ToolRefusal) -> (&'static str, String) {
    match refusal {
        ToolRefusal::TierNotGranted { tier } => (
            "tier_not_granted",
            serde_json::to_value(tier)
                .ok()
                .and_then(|tier| tier.as_str().map(str::to_string))
                .unwrap_or_default(),
        ),
        ToolRefusal::PathOutsideAllowed { path } => (
            "path_outside_allowed",
            format!("{path} is outside the contract's allowed paths"),
        ),
        ToolRefusal::PathProtected { path } => (
            "path_protected",
            format!("{path} is protected, and no tool reads or writes it (5.12)"),
        ),
        ToolRefusal::PathsMissing => (
            "paths_missing",
            "the call names no path, so none could be checked".to_string(),
        ),
        ToolRefusal::InvalidGlob { pattern, detail } => (
            "invalid_glob",
            format!("{pattern} does not compile: {detail}"),
        ),
        ToolRefusal::RequiresHumanApproval { tool } => (
            "requires_human_approval",
            format!(
                "{tool} changes the world outside the sandbox and the human has not approved it"
            ),
        ),
    }
}

fn triage(role: Role, status: TaskStatus, has_parent: bool, triaged: bool) -> String {
    if has_parent {
        return "an epic's tasks are not triaged: the epic was (5.16)".to_string();
    }
    match (role, status) {
        (Role::ProductManager | Role::ScrumMaster, TaskStatus::Draft) if triaged => {
            "the request is already triaged, and a triage the human made first is not overruled"
                .to_string()
        }
        (Role::ProductManager, TaskStatus::Draft) => {
            "the team has an active Scrum Master, and the request is its to triage".to_string()
        }
        (Role::ProductManager, TaskStatus::Refining) => {
            "a refining task is re-sized only as large, into an epic".to_string()
        }
        (Role::ProductManager | Role::ScrumMaster, status) => {
            format!("the request is {status}, and a triage comes before refining")
        }
        (role, _) => format!(
            "role {role} does not triage: the Scrum Master does, or the Product Manager on a team \
             without one (5.16)"
        ),
    }
}

fn command(refusal: &CommandRefusal) -> (&'static str, String) {
    match refusal {
        CommandRefusal::GitViaExec => (
            "git_via_exec",
            "use farik_git_status, farik_git_diff, farik_git_commit, or farik_git_push".to_string(),
        ),
        CommandRefusal::ForbiddenCommand { pattern } => (
            "command_forbidden",
            format!("the command matches the team's forbidden pattern {pattern} (5.12)"),
        ),
        CommandRefusal::InvalidPattern { pattern, detail } => (
            "invalid_pattern",
            format!(
                "the team's forbidden pattern {pattern} does not compile, so nothing runs: {detail}"
            ),
        ),
    }
}

fn contract_write(refusal: &ContractWriteRefusal) -> (&'static str, String) {
    let listed = |fields: &[String]| fields.join(", ");
    match refusal {
        ContractWriteRefusal::ContractLocked => (
            "contract_locked",
            "the contract is held by the human, and its content is the holder's alone (5.11)"
                .to_string(),
        ),
        ContractWriteRefusal::ContractFrozen { fields } => (
            "contract_frozen",
            format!(
                "the task has left refining, so its content is frozen (5.11), and this would \
                 change {}",
                listed(fields)
            ),
        ),
        ContractWriteRefusal::TaskTerminal { status } => (
            "task_terminal",
            format!("the task is {status}, and nothing leaves that status (5.2)"),
        ),
        ContractWriteRefusal::LifecycleFields { fields } => (
            "lifecycle_fields",
            format!(
                "{} change only through a transition (5.2): ask for the transition instead",
                listed(fields)
            ),
        ),
        ContractWriteRefusal::HumansFields { fields } => (
            "humans_fields",
            format!("{} are the human's alone (5.11)", listed(fields)),
        ),
        ContractWriteRefusal::StoresFields { fields } => (
            "stores_fields",
            format!(
                "{} are Farik's: it assigns the id and the stamps",
                listed(fields)
            ),
        ),
        ContractWriteRefusal::CreationFields { fields } => (
            "creation_fields",
            format!(
                "{} are fixed when a contract is created: the triage decides the kind, and a \
                 task's epic is the one that broke it down (5.16)",
                listed(fields)
            ),
        ),
        ContractWriteRefusal::ContentFields { fields } => (
            "content_fields",
            format!(
                "a contract's content is the Product Manager's, and an epic's tasks are its \
                 assignee's (5.16, 6.2): this caller does not write {}",
                listed(fields)
            ),
        ),
        ContractWriteRefusal::UnknownFields { fields } => (
            "unknown_fields",
            format!("{} are not fields of a contract", listed(fields)),
        ),
    }
}

impl From<Refusal> for ToolError {
    fn from(refusal: Refusal) -> Self {
        ToolError::Refused {
            reason: refusal.reason(),
        }
    }
}
