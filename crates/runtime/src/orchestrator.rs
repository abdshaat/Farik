//! The orchestrator (`docs/SPEC.md` sections 5.2 and 5.5, F6): Farik running its team on its own.
//! Each tick reads the board, does the first thing on it that needs doing, running at most one
//! session to its end, and says what it did.

use std::fmt;
use std::sync::Arc;

use farik_core::contract::TaskId;
use farik_protocol::clock::IdSource;
use farik_roles::RoleError;
use farik_store::files::FilesError;
use farik_store::{GitError, StoreError};

use crate::cost::CostError;
use crate::daemon::DaemonState;
use crate::sandbox::{SandboxError, SandboxFactory};
use crate::session::{RuntimeAdapter, RuntimeError};
use crate::tools::ToolDeps;
use crate::transitions::TransitionError;

#[cfg(test)]
pub(crate) mod fixtures;
mod messages;
mod rules;
mod session;

/// What the orchestrator works with.
pub struct OrchestratorDeps {
    /// The project's log, board, files, repository, clock, and the governor's door.
    pub tools: Arc<ToolDeps>,
    /// The daemon every session registers with.
    pub daemon: Arc<DaemonState>,
    /// What starts sessions.
    pub adapter: Arc<dyn RuntimeAdapter>,
    /// What makes a task's sandbox.
    pub sandboxes: Arc<dyn SandboxFactory>,
    /// Where session ids come from.
    pub session_ids: Arc<dyn IdSource + Send + Sync>,
}

/// Why a tick could not finish. Something the governor refused is not one of these: it is an
/// answer, and the tick says what it did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorError {
    /// The log or the board failed.
    Store(StoreError),
    /// A file under `.farik/` could not be read or written.
    Files(FilesError),
    /// Git failed.
    Git(GitError),
    /// A session could not be started or read.
    Runtime(RuntimeError),
    /// A task's sandbox could not be made.
    Sandbox(SandboxError),
    /// A role could not be loaded.
    Role(RoleError),
    /// A transition could not be judged or recorded.
    Transition(TransitionError),
    /// A cost could not be recorded, or a budget read.
    Cost(CostError),
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "the store failed: {error}"),
            Self::Files(error) => write!(formatter, "the files failed: {error}"),
            Self::Git(error) => write!(formatter, "git failed: {error}"),
            Self::Runtime(error) => write!(formatter, "the runtime failed: {error}"),
            Self::Sandbox(error) => write!(formatter, "the sandbox failed: {error}"),
            Self::Role(error) => write!(formatter, "the role failed: {error}"),
            Self::Transition(error) => write!(formatter, "the transition failed: {error}"),
            Self::Cost(error) => write!(formatter, "the cost failed: {error}"),
        }
    }
}

impl std::error::Error for OrchestratorError {}

impl From<StoreError> for OrchestratorError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<FilesError> for OrchestratorError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<GitError> for OrchestratorError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<RuntimeError> for OrchestratorError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<SandboxError> for OrchestratorError {
    fn from(error: SandboxError) -> Self {
        Self::Sandbox(error)
    }
}

impl From<RoleError> for OrchestratorError {
    fn from(error: RoleError) -> Self {
        Self::Role(error)
    }
}

impl From<TransitionError> for OrchestratorError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

impl From<CostError> for OrchestratorError {
    fn from(error: CostError) -> Self {
        Self::Cost(error)
    }
}

/// What one tick did, in words a person reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickReport {
    /// Nothing was done, and why.
    Idle {
        /// Why not.
        why: String,
    },
    /// Something was done about a task.
    Acted {
        /// The task.
        task_id: TaskId,
        /// What was done.
        what: String,
    },
}

/// Farik running a project's team. ponytail: one session at a time, a session pool when a team
/// outgrows a WIP limit of one.
pub struct Orchestrator {
    deps: OrchestratorDeps,
}

impl Orchestrator {
    /// An orchestrator that has done nothing yet.
    #[must_use]
    pub fn new(deps: OrchestratorDeps) -> Orchestrator {
        Orchestrator { deps }
    }

    /// Does the first thing on the board that needs doing, running at most one session to its
    /// end, and says what it did. The team file is read afresh each time.
    ///
    /// # Errors
    ///
    /// When the store, the files, git, a sandbox, a role, the budgets, or the runtime fail; a
    /// session that cannot start is `Runtime`, after its start and its end are recorded.
    pub async fn tick(&self) -> Result<TickReport, OrchestratorError> {
        rules::tick(&self.deps).await
    }
}
