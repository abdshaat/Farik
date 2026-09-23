//! Farik's local service (`docs/SPEC.md` sections 8.2 and 8.6): the governor in front of every tool
//! call a session makes. Claude Code's `PreToolUse` hook asks it for allow or deny, its
//! `PostToolUse` hook reports what came back, and the sessions it knows are the only ones it
//! answers for.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use farik_core::budget::SessionLimits;
use farik_core::contract::TaskId;

use crate::exec::Executor;
use crate::tools::ToolDeps;

#[cfg(test)]
pub(crate) mod fixtures;
mod hooks;

pub use hooks::{
    HookDecision, HookRequest, builtin_tool_tier, decide_pre_tool_use, record_post_tool_use,
};

/// Why the daemon could not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonError {
    /// The port could not be bound.
    Bind {
        /// What the operating system said.
        detail: String,
    },
    /// A file, the log, or the connection failed.
    Io {
        /// What failed.
        detail: String,
    },
}

impl fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind { detail } => write!(formatter, "the daemon could not listen: {detail}"),
            Self::Io { detail } => write!(formatter, "the daemon failed: {detail}"),
        }
    }
}

impl std::error::Error for DaemonError {}

/// One session the daemon answers for: who it is, what it works on, and where.
pub struct SessionRegistration {
    /// The id Farik gave Claude Code with `--session-id`, which every hook carries back.
    pub session_id: String,
    /// The agent the session is.
    pub agent_id: String,
    /// The task it works on, when it works on one.
    pub task_id: Option<TaskId>,
    /// Its working directory: the task's worktree. No tool call reaches outside it.
    pub cwd: PathBuf,
    /// Where the task's commands run, when it has somewhere.
    pub executor: Option<Arc<dyn Executor>>,
    /// Its limits, of which the daemon holds `max_tool_calls`.
    pub limits: SessionLimits,
}

/// A registration, and the tool calls the hook has allowed it.
pub(crate) struct Session {
    pub(crate) registration: SessionRegistration,
    pub(crate) tool_calls: u32,
}

/// What the daemon holds: the project's tools, and the sessions it answers for.
pub struct DaemonState {
    deps: Arc<ToolDeps>,
    sessions: Mutex<BTreeMap<String, Session>>,
}

impl DaemonState {
    /// A daemon for one project, answering for no session yet.
    #[must_use]
    pub fn new(deps: Arc<ToolDeps>) -> DaemonState {
        DaemonState {
            deps,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    /// Answers for `registration`'s session from now on, with no tool calls made. A session
    /// registered again starts its count again.
    pub fn register_session(&self, registration: SessionRegistration) {
        self.sessions().insert(
            registration.session_id.clone(),
            Session {
                registration,
                tool_calls: 0,
            },
        );
    }

    /// Stops answering for a session: every later hook of it is `unknown_session`.
    pub fn end_session(&self, session_id: &str) {
        self.sessions().remove(session_id);
    }

    /// How many tool calls the hook has allowed a session, or `None` for one it does not know.
    /// This count is the source of truth for a session's tool calls.
    #[must_use]
    pub fn tool_calls(&self, session_id: &str) -> Option<u32> {
        self.sessions()
            .get(session_id)
            .map(|session| session.tool_calls)
    }

    pub(crate) fn deps(&self) -> &Arc<ToolDeps> {
        &self.deps
    }

    /// The sessions, locked. A panic while they were held leaves them as they were, which is
    /// still a map of registrations and counts.
    pub(crate) fn sessions(&self) -> MutexGuard<'_, BTreeMap<String, Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
