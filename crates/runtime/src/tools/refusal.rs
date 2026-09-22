//! A tool's refusals in the words an agent reads: each starts with its kind in `snake_case`, then
//! `: `, then what it says, so that an agent and a test can both tell which rule refused.

use farik_core::governor::permissions::ToolRefusal;
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

impl From<Refusal> for ToolError {
    fn from(refusal: Refusal) -> Self {
        ToolError::Refused {
            reason: refusal.reason(),
        }
    }
}
