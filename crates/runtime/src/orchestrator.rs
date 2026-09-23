//! The orchestrator (`docs/SPEC.md` sections 5.2 and 5.5, F6): Farik running its team on its own.
//! Each tick reads the board, does the first thing on it that needs doing, running at most one
//! session to its end, and says what it did.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use farik_core::contract::TaskId;
use farik_core::governor::permissions::PermissionTier;
use farik_core::team::Team;
use farik_protocol::clock::IdSource;
use farik_roles::RoleError;
use farik_store::files::FilesError;
use farik_store::{GitError, StoreError};

use crate::cost::CostError;
use crate::daemon::DaemonState;
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::{RuntimeAdapter, RuntimeError};
use crate::tools::ToolDeps;
use crate::transitions::TransitionError;

#[cfg(test)]
pub(crate) mod fixtures;
mod integrate;
mod messages;
mod rules;
mod session;
mod verify;

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
    /// Each task's sandbox, made the first time a session of the task needs one in this process.
    sandboxes: Mutex<BTreeMap<TaskId, Arc<dyn Sandbox>>>,
    /// Read before each tick of `run_until_idle`.
    stopped: AtomicBool,
}

impl Orchestrator {
    /// An orchestrator that has done nothing yet.
    #[must_use]
    pub fn new(deps: OrchestratorDeps) -> Orchestrator {
        Orchestrator {
            deps,
            sandboxes: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
        }
    }

    /// Does the first thing on the board that needs doing, running at most one session to its
    /// end, and says what it did. The team file is read afresh each time.
    ///
    /// # Errors
    ///
    /// When the store, the files, git, a sandbox, a role, the budgets, or the runtime fail; a
    /// session that cannot start is `Runtime`, after its start and its end are recorded.
    pub async fn tick(&self) -> Result<TickReport, OrchestratorError> {
        rules::tick(self).await
    }

    /// Ticks until a tick is idle or `stop` was called.
    ///
    /// # Errors
    ///
    /// The first error a tick returns.
    pub async fn run_until_idle(&self) -> Result<(), OrchestratorError> {
        while !self.stopped.load(Ordering::SeqCst) {
            if let TickReport::Idle { .. } = self.tick().await? {
                break;
            }
        }
        Ok(())
    }

    /// Stops `run_until_idle` before its next tick. A session already running runs to its end.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// The task's sandbox: the one made for it earlier in this process, or a new one rooted at
    /// its worktree, with the network on when its assignee holds `network`.
    fn sandbox_for(
        &self,
        task_id: &TaskId,
        team: &Team,
    ) -> Result<Arc<dyn Sandbox>, OrchestratorError> {
        let mut sandboxes = self
            .sandboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(sandbox) = sandboxes.get(task_id) {
            return Ok(Arc::clone(sandbox));
        }
        let assignee = self
            .deps
            .tools
            .projections
            .task(task_id)?
            .and_then(|row| row.assignee_id);
        let network = team
            .agents
            .iter()
            .find(|agent| Some(agent.id.as_str()) == assignee.as_deref())
            .is_some_and(|agent| agent.tiers().contains(&PermissionTier::Network));
        let sandbox: Arc<dyn Sandbox> = Arc::from(self.deps.sandboxes.create(
            &self.deps.tools.ids.project_id,
            task_id,
            &worktree(&self.deps, task_id),
            network,
        )?);
        sandboxes.insert(task_id.clone(), Arc::clone(&sandbox));
        Ok(sandbox)
    }

    /// Forgets the task's sandbox, so that the next session or criterion of the task gets a new
    /// one from `sandbox_for`. A container sandbox's `create` removes a container of the task's
    /// name first, which is what ends the one forgotten; a host sandbox holds nothing to end.
    fn forget_sandbox(&self, task_id: &TaskId) {
        self.sandboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(task_id);
    }

    /// Whether this process holds a sandbox for the task.
    #[cfg(test)]
    fn holds_sandbox(&self, task_id: &TaskId) -> bool {
        self.sandboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(task_id)
    }
}

/// A task's worktree, `.farik/local/worktrees/<id>` (5.14).
fn worktree(deps: &OrchestratorDeps, task_id: &TaskId) -> PathBuf {
    deps.tools
        .files
        .root()
        .join(".farik/local/worktrees")
        .join(task_id.as_str())
}

#[cfg(test)]
mod tests {
    use farik_core::contract::TaskStatus;
    use farik_protocol::event::{
        CriterionRecordedBodyRunBy, EventBody, EventKind, NoteWrittenBodyKind,
        SessionStartedBodyPurpose,
    };

    use crate::orchestrator::fixtures::Harness;
    use crate::recorded::fixtures::{
        accept_frk_1, implement_finishes_frk_1, plan_assigns_frk_1, review_writes_note,
    };

    /// Each session started, as its purpose and its agent, in order.
    fn sessions(harness: &Harness) -> Vec<(SessionStartedBodyPurpose, String)> {
        harness
            .events(&[EventKind::SessionStarted])
            .iter()
            .map(|event| match &event.body {
                EventBody::SessionStarted(body) => (
                    body.purpose,
                    event.envelope.ids.agent_id.clone().unwrap_or_default(),
                ),
                other => panic!("a session.started, got {other:?}"),
            })
            .collect()
    }

    /// Each move, as `from -> to`, in order.
    fn moves(harness: &Harness) -> Vec<String> {
        harness
            .events(&[EventKind::TaskTransitioned])
            .iter()
            .map(|event| match &event.body {
                EventBody::TaskTransitioned(body) => format!("{} -> {}", body.from, body.to),
                other => panic!("a task.transitioned, got {other:?}"),
            })
            .collect()
    }

    /// Each criterion result, as its id, its runner, and who recorded it, in order.
    fn runs(harness: &Harness) -> Vec<(String, CriterionRecordedBodyRunBy, String)> {
        harness
            .events(&[EventKind::CriterionRecorded])
            .iter()
            .map(|event| match &event.body {
                EventBody::CriterionRecorded(body) => (
                    body.criterion_id.clone(),
                    body.run_by,
                    body.recorded_by.clone(),
                ),
                other => panic!("a criterion.recorded, got {other:?}"),
            })
            .collect()
    }

    /// Each note's kind, in order.
    fn notes(harness: &Harness) -> Vec<NoteWrittenBodyKind> {
        harness
            .events(&[EventKind::NoteWritten])
            .iter()
            .map(|event| match &event.body {
                EventBody::NoteWritten(body) => body.kind,
                other => panic!("a note.written, got {other:?}"),
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_one_task_from_ready_to_accepted() {
        let harness = Harness::new("orch-one-task", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![
            plan_assigns_frk_1(),
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator
            .run_until_idle()
            .await
            .expect("the run ends idle");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
        let git = &harness.project.deps.git;
        assert_eq!(
            git.commit_count("main", "farik/FRK-1").expect("git counts"),
            1
        );
        assert_eq!(
            git.changed_paths("main", "farik/FRK-1").expect("git lists"),
            vec!["done.txt".to_string()]
        );
        assert_eq!(
            sessions(&harness),
            vec![
                (SessionStartedBodyPurpose::Plan, "pm".to_string()),
                (SessionStartedBodyPurpose::Implement, "dev-a".to_string()),
                (SessionStartedBodyPurpose::Verify, "dev-b".to_string()),
                (SessionStartedBodyPurpose::Verify, "pm".to_string()),
            ]
        );
        assert_eq!(
            moves(&harness),
            [
                "ready -> assigned",
                "assigned -> in_progress",
                "in_progress -> verifying",
                "verifying -> accepted",
            ]
        );
        assert_eq!(
            runs(&harness),
            vec![
                (
                    "C1".to_string(),
                    CriterionRecordedBodyRunBy::Assignee,
                    "dev-a".to_string()
                ),
                (
                    "C1".to_string(),
                    CriterionRecordedBodyRunBy::Reviewer,
                    "governor".to_string()
                ),
            ]
        );
        assert_eq!(
            notes(&harness),
            vec![NoteWrittenBodyKind::Completion, NoteWrittenBodyKind::Review]
        );
        let reviews = harness.events(&[EventKind::ReviewRecorded]);
        assert!(
            matches!(
                &reviews[..],
                [review] if matches!(&review.body, EventBody::ReviewRecorded(body) if body.passed)
            ),
            "{reviews:?}"
        );
        let costs = harness.events(&[EventKind::CostRecorded]);
        assert_eq!(costs.len(), 4, "{costs:?}");
        for cost in &costs {
            assert_eq!(
                cost.envelope.ids.task_id.as_ref().map(|task| task.as_str()),
                Some("FRK-1")
            );
        }
        assert_eq!(adapter.started().len(), 4);
        assert_eq!(adapter.transcripts_left(), 0);
    }
}
