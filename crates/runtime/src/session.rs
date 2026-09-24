//! What a session is given, what it reports, and the two traits every runtime implements: the
//! adapter that starts sessions and the handle a caller holds while one runs.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use farik_core::budget::SessionLimits;
use farik_core::contract::TaskId;
use farik_core::pricing::Usage;
use farik_core::team::{Agent, Effort};
use farik_roles::RoleDefinition;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::Receiver;

/// Why a session was started. Stamped on the session's events and on every cost it records, so
/// that what the team spends can be read back by what it was spent on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPurpose {
    /// Sizing a request as an epic or a task.
    Triage,
    /// Writing or improving a contract.
    Refine,
    /// Breaking an epic into tasks and assigning them.
    Plan,
    /// Doing a task's work.
    Implement,
    /// Reviewing a task's work against its contract.
    Verify,
    /// A team ceremony.
    Ceremony,
    /// Talking with the human.
    Conversation,
}

/// How a session reaches an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransport {
    /// Over streamable HTTP at `url`.
    Http {
        /// Where the server listens.
        url: String,
    },
    /// As a child process speaking on its standard streams.
    Stdio {
        /// The program to run.
        command: String,
        /// Its arguments.
        args: Vec<String>,
    },
}

/// One MCP server a session is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    /// The name the session knows the server by; its tools are `mcp__<name>__<tool>`.
    pub name: String,
    /// How to reach it.
    pub transport: McpTransport,
    /// Headers sent with every request, for the HTTP transport.
    pub headers: BTreeMap<String, String>,
}

/// Everything a runtime needs to start one session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSpec {
    /// Farik's id for the session, which the runtime also hands the program.
    pub session_id: String,
    /// The agent the session is for.
    pub agent_id: String,
    /// The contract the session works on, when it works on one.
    pub task_id: Option<TaskId>,
    /// Why it was started.
    pub purpose: SessionPurpose,
    /// The assembled system prompt.
    pub system_prompt: String,
    /// The provider's model id.
    pub model: String,
    /// How hard the model thinks.
    pub effort: Effort,
    /// The Farik tools the session may call, by name. The daemon's hook enforces it; nothing
    /// puts it on the program's command line.
    pub farik_tools: Vec<String>,
    /// The program's own tools the session may call, and no other: the allowlist
    /// (`claude::allowed_builtins` gives the ones an agent's tiers grant).
    pub builtin_tools: Vec<String>,
    /// The MCP servers it is given besides Farik's own, which the runtime adds itself; a server
    /// here named `farik` is refused.
    pub mcp_servers: Vec<McpServerConfig>,
    /// The directory it works in.
    pub cwd: PathBuf,
    /// When it is stopped.
    pub limits: SessionLimits,
    /// The first user message.
    pub initial_prompt: String,
}

/// Why a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// The model finished its turn.
    Completed,
    /// The caller stopped it.
    Aborted,
    /// A limit stopped it: turns, tokens, time.
    Limit,
    /// The program reported an error.
    Error,
    /// The model provider refused it for a usage or rate limit.
    ProviderLimit,
}

/// One thing a session reported.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// The model called a tool.
    ToolCalled {
        /// The tool's name.
        tool: String,
        /// What it was called with.
        input: Value,
    },
    /// A tool answered.
    ToolReturned {
        /// The tool's name.
        tool: String,
        /// Its answer, as text.
        output: String,
    },
    /// A tool call was refused before it ran.
    ToolDenied {
        /// The tool's name.
        tool: String,
        /// Why, in the words of whoever refused it.
        reason: String,
    },
    /// What the whole session consumed, reported once at its end.
    UsageReported(Usage),
    /// The model wrote text.
    TextProduced(String),
    /// The session is over; nothing follows this.
    Ended {
        /// Why.
        reason: EndReason,
        /// What the program said about it.
        detail: String,
        /// When the provider said its limit resets, as it said it; set only for `ProviderLimit`.
        resets_at: Option<DateTime<Utc>>,
    },
}

/// Why a runtime could not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// The session could not be started.
    Spawn {
        /// Why.
        detail: String,
    },
    /// The program said something the runtime cannot read.
    Protocol {
        /// What, and which field.
        detail: String,
    },
    /// The program is older than the oldest version Farik was tested against.
    VersionTooOld {
        /// The version found.
        found: String,
        /// The oldest version accepted.
        required: String,
    },
    /// The session was stopped by its caller.
    Aborted,
    /// The session was stopped by a limit.
    Limit,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { detail } => write!(formatter, "the session could not start: {detail}"),
            Self::Protocol { detail } => {
                write!(formatter, "the session said something unreadable: {detail}")
            }
            Self::VersionTooOld { found, required } => write!(
                formatter,
                "Claude Code {found} is older than {required}, the oldest version Farik runs on"
            ),
            Self::Aborted => write!(formatter, "the session was stopped"),
            Self::Limit => write!(formatter, "the session was stopped by a limit"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// A running session, held by whoever started it.
pub trait SessionHandle: Send {
    /// Farik's id for the session.
    fn session_id(&self) -> &str;
    /// What the session reports, in order; closed after `Ended`.
    fn events(&mut self) -> &mut Receiver<SessionEvent>;
    /// Sends the session a user message.
    ///
    /// # Errors
    ///
    /// When the session can no longer take one.
    fn send(&self, text: &str) -> Result<(), RuntimeError>;
    /// Stops the session; it ends with `EndReason::Aborted`.
    ///
    /// # Errors
    ///
    /// When the session cannot be reached to be stopped.
    fn abort(&self) -> Result<(), RuntimeError>;
}

/// Something that starts sessions.
pub trait RuntimeAdapter: Send + Sync {
    /// Starts a session.
    ///
    /// # Errors
    ///
    /// When it cannot be started.
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError>;
    /// Continues an earlier session with a new user message.
    ///
    /// # Errors
    ///
    /// When it cannot be continued.
    fn resume(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError>;
}

/// The model a triage session runs on, whatever the agent's own: 5.16 runs triage on the cheaper
/// model, and 8.2 names it. It lives here rather than in the orchestrator, which re-exports it, so
/// that `cost::unpriced_models` can name it where the orchestrator is not built.
pub const TRIAGE_MODEL: &str = "claude-sonnet-5";

/// The model and effort an agent's sessions run on: its own `model.id` when it has one, with its
/// own effort or else its role's, and otherwise its role's model and effort. A triage session
/// runs on the orchestrator's `TRIAGE_MODEL` instead (5.16), which is the orchestrator's choice
/// rather than this one.
#[must_use]
pub fn session_model(agent: &Agent, role: &RoleDefinition) -> (String, Effort) {
    match &agent.model {
        Some(model) => (model.id.to_string(), model.effort.unwrap_or(role.effort)),
        None => (role.model.clone(), role.effort),
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::Role;
    use farik_core::team::fixtures::an_agent_wire;
    use farik_core::team::{Agent, Effort};
    use farik_roles::load_role;
    use serde_json::json;

    use super::{RuntimeError, SessionPurpose, session_model};

    fn an_agent(model: Option<serde_json::Value>) -> Agent {
        let mut wire = an_agent_wire("dev-a", "software_developer");
        if let Some(model) = model {
            wire["model"] = model;
        }
        serde_json::from_value(wire).expect("the fixture is an agent")
    }

    #[test]
    fn takes_the_agents_model_and_else_the_roles() {
        let role = load_role(Role::SoftwareDeveloper).expect("Farik ships the role");
        assert_ne!(
            role.effort,
            Effort::Low,
            "the test needs the role's effort to differ"
        );
        assert_eq!(
            session_model(
                &an_agent(Some(json!({ "id": "claude-sonnet-5", "effort": "low" }))),
                &role
            ),
            ("claude-sonnet-5".to_string(), Effort::Low)
        );
        assert_eq!(
            session_model(&an_agent(Some(json!({ "id": "claude-sonnet-5" }))), &role),
            ("claude-sonnet-5".to_string(), role.effort)
        );
        assert_eq!(
            session_model(&an_agent(None), &role),
            (role.model.clone(), role.effort)
        );
    }

    #[test]
    fn takes_the_roles_effort_whatever_it_is_when_the_agent_names_none() {
        // Every role Farik ships runs at high effort, so the fallback is proved on one that does
        // not.
        let mut role = load_role(Role::SoftwareDeveloper).expect("Farik ships the role");
        role.effort = Effort::Medium;
        assert_eq!(
            session_model(&an_agent(Some(json!({ "id": "claude-sonnet-5" }))), &role),
            ("claude-sonnet-5".to_string(), Effort::Medium)
        );
        assert_eq!(
            session_model(&an_agent(None), &role),
            (role.model.clone(), Effort::Medium)
        );
    }

    #[test]
    fn displays_a_version_too_old_error_with_both_versions() {
        let error = RuntimeError::VersionTooOld {
            found: "2.1.200".to_string(),
            required: "2.1.272".to_string(),
        };
        let text = error.to_string();
        assert!(text.contains("2.1.200"), "{text}");
        assert!(text.contains("2.1.272"), "{text}");
    }

    #[test]
    fn serialises_session_purposes_in_snake_case() {
        assert_eq!(
            serde_json::to_value(SessionPurpose::Implement).expect("a unit variant serialises"),
            serde_json::json!("implement")
        );
        for purpose in [
            SessionPurpose::Triage,
            SessionPurpose::Refine,
            SessionPurpose::Plan,
            SessionPurpose::Implement,
            SessionPurpose::Verify,
            SessionPurpose::Ceremony,
            SessionPurpose::Conversation,
        ] {
            let wire = serde_json::to_value(purpose).expect("a unit variant serialises");
            let back: SessionPurpose =
                serde_json::from_value(wire).expect("what was written reads back");
            assert_eq!(back, purpose);
        }
    }
}
