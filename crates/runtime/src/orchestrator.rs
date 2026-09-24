//! The orchestrator (`docs/SPEC.md` sections 5.2 and 5.5, F6): Farik running its team on its own.
//! Each tick reads the board, does the first thing on it that needs doing, running at most one
//! session to its end, and says what it did.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_core::governor::permissions::PermissionTier;
use farik_core::team::Team;
use farik_protocol::clock::IdSource;
use farik_protocol::command::{Command, CommandReply, ReplyKind};
use farik_roles::RoleError;
use farik_store::files::FilesError;
use farik_store::{GitError, StoreError};

use crate::cost::CostError;
use crate::daemon::{CommandHandler, DaemonState};
use crate::forge::{Forge, ForgeError};
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::{RuntimeAdapter, RuntimeError};
use crate::sleep::Sleeper;
use crate::sprints::SprintError;
use crate::tools::ToolDeps;
use crate::transitions::TransitionError;

#[cfg(test)]
pub(crate) mod fixtures;
mod human;
mod integrate;
mod messages;
mod recover;
mod requests;
mod rules;
mod session;
mod verify;

pub use crate::session::TRIAGE_MODEL;

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
    /// The forge pull requests are opened on, under the `pull_request` policy.
    pub forge: Arc<Forge>,
    /// What a run waits on while every agent with work is asleep.
    pub sleeper: Arc<dyn Sleeper>,
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
    /// What was asked cannot be done to the task as it stands, such as integrating one that is
    /// not accepted.
    Refused {
        /// Why, starting with a word a program can match.
        reason: String,
    },
    /// The integration lock could not be taken.
    Lock {
        /// What the operating system said.
        detail: String,
    },
    /// The forge could not answer.
    Forge(ForgeError),
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
            Self::Refused { reason } => write!(formatter, "refused: {reason}"),
            Self::Lock { detail } => {
                write!(
                    formatter,
                    "the integration lock could not be taken: {detail}"
                )
            }
            Self::Forge(error) => write!(formatter, "the forge failed: {error}"),
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

impl From<ForgeError> for OrchestratorError {
    fn from(error: ForgeError) -> Self {
        Self::Forge(error)
    }
}

impl From<SprintError> for OrchestratorError {
    fn from(error: SprintError) -> Self {
        match error {
            SprintError::Files(error) => Self::Files(error),
            SprintError::Store(error) => Self::Store(error),
            other => Self::Refused {
                reason: other.to_string(),
            },
        }
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
        /// When the first sleeping agent the rules passed over wakes, when one was (5.5): a run
        /// waits until then and ticks again.
        until: Option<DateTime<Utc>>,
    },
    /// Something was done about a task.
    Acted {
        /// The task.
        task_id: TaskId,
        /// What was done.
        what: String,
    },
    /// Something was done about the open sprint.
    Sprint {
        /// The sprint.
        sprint_id: String,
        /// What was done.
        what: String,
    },
}

/// How a wait for a sleeping agent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// It is the time waited for.
    Reached,
    /// `stop` was called first.
    Stopped,
}

/// Which of the rules a tick runs (`docs/SPEC.md` 8.2): every one, the planning ones, or the
/// refining ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TickRules {
    /// Every rule, as `farik run` ticks.
    #[default]
    All,
    /// Plan without doing, as `farik plan` ticks: triage and `draft -> refining` (rule 10),
    /// refining and the governor's judgment (rule 9), the plan sessions that assign and an
    /// approved epic's assignment (rule 8), and an epic's breakdown and close-out (rule 6 for an
    /// epic alone). No cleanup, integration, criterion run, worktree, or start of work.
    Planning,
    /// Triage and refining alone (rules 9 and 10), as `farik contract new` ticks.
    Refining,
}

/// What a tick may act on: one task, or every task, under a set of rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickScope {
    /// The task every rule is confined to, when one is named.
    pub task_id: Option<TaskId>,
    /// The rules the tick runs.
    pub rules: TickRules,
}

/// What came of integrating an accepted task (5.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationOutcome {
    /// Its branch is in the integration branch, at this commit.
    Merged {
        /// The integration branch's commit holding it.
        sha: String,
    },
    /// A pull request was opened for it, at this address.
    PullRequestOpened {
        /// The pull request's address.
        url: String,
    },
    /// Its pull request is open on the forge, waiting for the human.
    AwaitingForge,
    /// It could not land, and the human was told why in these words.
    Escalated {
        /// The escalation's detail.
        detail: String,
    },
}

/// What a command the human gave did, for the command line to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandReport {
    /// One sentence saying what happened.
    pub said: String,
    /// The sequence numbers of the events the command appended, in order.
    pub events: Vec<u64>,
}

/// Why a command the human gave did nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The command is not one that can be handled as given: a blank answer, reason, or message,
    /// or a command this door does not take.
    Invalid {
        /// What is wrong with it.
        detail: String,
    },
    /// The command cannot be done to what it names as it stands.
    Refused {
        /// Why, starting with a `snake_case` kind and `: `, as the tools' refusals do.
        reason: String,
    },
    /// What the command names does not exist.
    NotFound {
        /// The task, question, agent, or session named.
        what: String,
    },
    /// The store, a file, git, or the orchestrator failed.
    Failed {
        /// What failed, in its words.
        detail: String,
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { detail } => write!(formatter, "invalid: {detail}"),
            Self::Refused { reason } => write!(formatter, "refused: {reason}"),
            Self::NotFound { what } => write!(formatter, "not found: {what}"),
            Self::Failed { detail } => write!(formatter, "failed: {detail}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// What `recover` found and did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// Sessions the log shows started and never ended, now recorded as ended `aborted`.
    pub sessions_interrupted: u32,
    /// Finished tasks whose worktrees and sandboxes were removed.
    pub worktrees_removed: u32,
    /// Tasks `in_progress`, which the next ticks resume.
    pub tasks_resumed: u32,
}

/// Farik running a project's team. ponytail: one session at a time, a session pool when a team
/// outgrows a WIP limit of one.
pub struct Orchestrator {
    deps: OrchestratorDeps,
    /// Each task's sandbox, made the first time a session of the task needs one in this process.
    sandboxes: Mutex<BTreeMap<TaskId, Arc<dyn Sandbox>>>,
    /// Read before each tick of `run_until_idle`.
    stopped: AtomicBool,
    /// Notified by `stop`, so that a wait for a sleeping agent ends at once.
    stops: tokio::sync::Notify,
}

impl Orchestrator {
    /// An orchestrator that has done nothing yet.
    #[must_use]
    pub fn new(deps: OrchestratorDeps) -> Orchestrator {
        Orchestrator {
            deps,
            sandboxes: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
            stops: tokio::sync::Notify::new(),
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
        self.tick_within(&TickScope::default()).await
    }

    /// `tick`, confined to `scope`: its task alone when it names one, and its rules alone.
    ///
    /// # Errors
    ///
    /// As `tick`.
    pub async fn tick_within(&self, scope: &TickScope) -> Result<TickReport, OrchestratorError> {
        rules::tick(self, scope).await
    }

    /// Whether `stop` was called.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Ticks until a tick is idle with no agent to wait for, or `stop` was called. A tick idle
    /// while an agent sleeps is waited out (`wait_until`), and the ticks go on.
    ///
    /// # Errors
    ///
    /// The first error a tick returns.
    pub async fn run_until_idle(&self) -> Result<(), OrchestratorError> {
        while !self.is_stopped() {
            match self.tick().await? {
                TickReport::Idle {
                    until: Some(until), ..
                } => {
                    self.wait_until(until).await;
                }
                TickReport::Idle { until: None, .. } => break,
                TickReport::Acted { .. } | TickReport::Sprint { .. } => {}
            }
        }
        Ok(())
    }

    /// Waits until `until`, or until `stop` is called, whichever comes first; at once when
    /// `stop` was called before.
    pub async fn wait_until(&self, until: DateTime<Utc>) -> Waited {
        // Taken before the stop is read, so that a stop between the two still ends the wait.
        let stopped = self.stops.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        if self.is_stopped() {
            return Waited::Stopped;
        }
        tokio::select! {
            () = self.deps.sleeper.sleep_until(until) => Waited::Reached,
            () = stopped => Waited::Stopped,
        }
    }

    /// Integrates an accepted task now, as the human asks (`farik integrate`), whatever
    /// escalations it carries: under `manual` a merge into the integration branch with no push,
    /// under `auto_merge` the merge and the push to `origin` when there is one, under
    /// `pull_request` a pull request opened when none was since acceptance, else the recorded one
    /// read (`AwaitingForge` while open; when closed, `Merged` if the branch is in the integration
    /// branch all the same). A task already integrated answers the commit it was integrated at and
    /// does nothing. One task integrates at a time, across processes too.
    ///
    /// # Errors
    ///
    /// `Refused` for a task that is not an accepted task;
    /// `Lock` when the integration lock cannot be taken; the store's, the files', and git's own
    /// failures. A merge or push that cannot land is not an error: it is `Escalated`.
    pub async fn integrate(
        &self,
        task_id: &TaskId,
    ) -> Result<IntegrationOutcome, OrchestratorError> {
        integrate::integrate(self, task_id).await
    }

    /// Does what the human asks (`docs/SPEC.md` sections 5.2, 5.7, 5.11, 5.14, 5.16, F1): answer
    /// a question, approve a contract or accept a result, resolve an escalation, move, lock,
    /// triage, or integrate a task, pause an agent, stop a session or the run. Every command takes
    /// effect through the store and the files, so any process may handle it, except `SessionStop`,
    /// which reaches only a session registered in this process.
    ///
    /// # Errors
    ///
    /// `Invalid` for a blank answer, reason, or message, and for `TaskCreate`; `Refused` with a
    /// `snake_case` kind first when the command cannot be done as things stand, the governor's
    /// own refusals as `transition_refused`; `NotFound` naming what is not there; `Failed` when
    /// the store, a file, git, or the orchestrator fails.
    pub async fn handle(&self, command: Command) -> Result<CommandReport, CommandError> {
        human::handle(self, command).await
    }

    /// Picks up a run that was killed (5.15), before the first tick: every session the log shows
    /// started and not ended is recorded as ended `aborted`, "interrupted", and, when it has no
    /// cost, as costing nothing, so that it counts against its task's sessions; every accepted or
    /// cancelled task's worktrees and sandboxes left behind are removed. Tasks in progress are
    /// left to the tick, which resumes each in a sandbox made afresh. Synchronous, because it runs
    /// git and Docker: a caller on an async runtime runs it under `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// When the log, the files, git, a sandbox, or a cost cannot be read or recorded.
    pub fn recover(&self) -> Result<RecoveryReport, OrchestratorError> {
        recover::recover(self)
    }

    /// Stops `run_until_idle` before its next tick, and ends a wait at once. A session already
    /// running runs to its end.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.stops.notify_waiters();
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

/// The daemon's handler of the human's commands (`POST /command`): each command handled by
/// `orchestrator`, the one driving the project in this process.
#[must_use]
pub fn command_handler(orchestrator: Arc<Orchestrator>) -> CommandHandler {
    Arc::new(move |command| {
        let orchestrator = Arc::clone(&orchestrator);
        Box::pin(async move { orchestrator.handle(command).await })
    })
}

/// A command's outcome as the reply the daemon sends back.
#[must_use]
pub fn reply_of(result: Result<CommandReport, CommandError>) -> CommandReply {
    match result {
        Ok(report) => CommandReply::Done {
            said: report.said,
            events: report.events,
        },
        Err(CommandError::Invalid { detail }) => CommandReply::Error {
            kind: ReplyKind::Invalid,
            detail,
        },
        Err(CommandError::Refused { reason }) => CommandReply::Error {
            kind: ReplyKind::Refused,
            detail: reason,
        },
        Err(CommandError::NotFound { what }) => CommandReply::Error {
            kind: ReplyKind::NotFound,
            detail: what,
        },
        Err(CommandError::Failed { detail }) => CommandReply::Error {
            kind: ReplyKind::Failed,
            detail,
        },
    }
}

/// A reply from another process's daemon as the outcome `handle` would have given here:
/// `reply_of`'s inverse.
///
/// # Errors
///
/// The `CommandError` of the reply's kind, carrying its detail.
pub fn result_of(reply: CommandReply) -> Result<CommandReport, CommandError> {
    match reply {
        CommandReply::Done { said, events } => Ok(CommandReport { said, events }),
        CommandReply::Error { kind, detail } => Err(match kind {
            ReplyKind::Invalid => CommandError::Invalid { detail },
            ReplyKind::Refused => CommandError::Refused { reason: detail },
            ReplyKind::NotFound => CommandError::NotFound { what: detail },
            ReplyKind::Failed => CommandError::Failed { detail },
        }),
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
    use chrono::{DateTime, Utc};
    use farik_core::contract::TaskStatus;
    use farik_protocol::event::{
        CriterionRecordedBodyRunBy, EventBody, EventKind, NoteWrittenBodyKind,
        SessionStartedBodyPurpose,
    };

    use farik_store::git::fixtures::git_output_in;

    use farik_core::contract::{TaskId, TaskKind};
    use farik_protocol::command::{AcceptSubject, Command};
    use farik_protocol::event::EscalationRaisedBodyReason;

    use crate::orchestrator::fixtures::{Harness, run_until_idle_within_ten_seconds};
    use crate::recorded::fixtures::{
        accept_frk_1, implement_finishes_frk_1, plan_assigns_frk_1, plan_assigns_frk_2,
        plan_breaks_down_frk_1, plan_closes_epic_frk_1, refine_asks_frk_1,
        refine_writes_epic_frk_1, refine_writes_task_frk_1, review_writes_note, triage_frk_1_large,
    };
    use crate::tools::fixtures::at;

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
    async fn says_whether_it_was_stopped() {
        let harness = Harness::new("orch-is-stopped", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        assert!(!orchestrator.is_stopped());
        orchestrator.stop();
        assert!(orchestrator.is_stopped());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_sleeping_agent_then_goes_on() {
        let harness = Harness::new("orch-wait", |_| {});
        harness.ready("FRK-1");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-a", until);
        let adapter = harness.recorded(vec![
            plan_assigns_frk_1(),
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
        ]);
        let orchestrator = std::sync::Arc::new(harness.orchestrator_at(adapter.clone(), at()));

        run_until_idle_within_ten_seconds(std::sync::Arc::clone(&orchestrator))
            .expect("the run ends idle");

        assert_eq!(orchestrator.deps.tools.clock.now(), until);
        assert_eq!(
            sessions(&harness),
            vec![
                (SessionStartedBodyPurpose::Plan, "pm".to_string()),
                (SessionStartedBodyPurpose::Implement, "dev-a".to_string()),
                (SessionStartedBodyPurpose::Verify, "dev-b".to_string()),
                (SessionStartedBodyPurpose::Verify, "pm".to_string()),
            ]
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
    }

    /// Waits until `until` on `orchestrator`, failing the test rather than hanging when the wait
    /// does not end within five seconds.
    async fn waited(orchestrator: &super::Orchestrator, until: DateTime<Utc>) -> super::Waited {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            orchestrator.wait_until(until),
        )
        .await
        .expect("the wait ends")
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_a_wait() {
        let harness = Harness::new("orch-wait-stop", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let (ended, ()) = tokio::join!(
            waited(&orchestrator, at() + chrono::Duration::hours(1)),
            async {
                tokio::task::yield_now().await;
                orchestrator.stop();
            }
        );

        assert_eq!(ended, super::Waited::Stopped);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sees_a_stop_before_the_wait() {
        let harness = Harness::new("orch-wait-stopped", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.stop();

        let ended = waited(&orchestrator, at() + chrono::Duration::hours(1)).await;

        assert_eq!(ended, super::Waited::Stopped);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_one_task_from_ready_to_accepted() {
        let harness = Harness::new("orch-one-task", |wire| {
            wire["policy"]["integration"] = serde_json::json!("auto_merge");
        });
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![
            plan_assigns_frk_1(),
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let base = git_output_in(&harness.project.repo.path, &["rev-parse", "main"]);

        orchestrator
            .run_until_idle()
            .await
            .expect("the run ends idle");

        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
        let git = &harness.project.deps.git;
        let branch = harness.branch("FRK-1");
        assert_eq!(git.commit_count(&base, &branch).expect("git counts"), 1);
        assert_eq!(
            git.changed_paths(&base, &branch).expect("git lists"),
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
        let integrated = harness.events(&[EventKind::TaskIntegrated]);
        assert!(
            matches!(
                &integrated[..],
                [one] if matches!(&one.body, EventBody::TaskIntegrated(body)
                    if body.sha == git_output_in(&harness.project.repo.path, &["rev-parse", "main"]))
            ),
            "{integrated:?}"
        );
        assert!(!harness.worktree("FRK-1").exists());
        assert_eq!(
            git_output_in(&harness.project.repo.path, &["branch", "--list", &branch]).trim(),
            branch
        );
    }

    /// The Product Manager's triage of FRK-1 as `small`, a task: `triage_frk_1_large` with its
    /// size changed, since the size is the only thing a replayed triage decides.
    fn triage_frk_1_small() -> crate::recorded::Transcript {
        let large = triage_frk_1_large().lines().collect::<Vec<_>>().join("\n");
        assert!(
            large.contains(r#""size":"large""#),
            "the triage names a size"
        );
        crate::recorded::Transcript::from_jsonl(
            &large.replace(r#""size":"large""#, r#""size":"small""#),
        )
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_one_request_to_an_accepted_task_on_the_default_budget() {
        let harness = Harness::new("orch-one-request-task", |_| {});
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![
            triage_frk_1_small(),
            refine_writes_task_frk_1(),
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

        assert_eq!(
            sessions(&harness),
            vec![
                (SessionStartedBodyPurpose::Triage, "pm".to_string()),
                (SessionStartedBodyPurpose::Refine, "pm".to_string()),
                (SessionStartedBodyPurpose::Plan, "pm".to_string()),
                (SessionStartedBodyPurpose::Implement, "dev-a".to_string()),
                (SessionStartedBodyPurpose::Verify, "dev-b".to_string()),
                (SessionStartedBodyPurpose::Verify, "pm".to_string()),
            ]
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Accepted);
        assert!(
            harness.events(&[EventKind::BudgetExhausted]).is_empty(),
            "no budget ran out"
        );
        assert_eq!(adapter.transcripts_left(), 0);
        let contract = harness
            .project
            .deps
            .files
            .read_contract(&"FRK-1".parse().expect("a task id"))
            .expect("the contract reads");
        assert_eq!(
            contract.budget.max_sessions.get(),
            12,
            "the contract takes the schema's default"
        );
    }

    /// The events that mark a request's way to an accepted epic, each in a few words, in order.
    fn milestones(harness: &Harness) -> Vec<String> {
        harness
            .events(&[])
            .iter()
            .filter_map(|event| {
                let task = event
                    .envelope
                    .ids
                    .task_id
                    .as_ref()
                    .map(|task| task.as_str().to_string())
                    .unwrap_or_default();
                Some(match &event.body {
                    EventBody::RequestTriaged(_) if task == "FRK-1" => {
                        "request.triaged".to_string()
                    }
                    EventBody::QuestionAsked(_) => "question.asked".to_string(),
                    EventBody::QuestionAnswered(_) => "question.answered".to_string(),
                    EventBody::EscalationRaised(body)
                        if body.reason == EscalationRaisedBodyReason::Approval =>
                    {
                        "escalation.raised approval".to_string()
                    }
                    EventBody::HumanAccepted(body) => format!("human.accepted {}", body.subject),
                    EventBody::TaskTransitioned(body) if task == "FRK-1" => {
                        format!("FRK-1 {} -> {}", body.from, body.to)
                    }
                    EventBody::TaskCreated(_) if task == "FRK-2" => "FRK-2 created".to_string(),
                    EventBody::TaskIntegrated(_) => format!("{task} integrated"),
                    EventBody::CriterionRecorded(body)
                        if task == "FRK-1" && body.recorded_by == "governor" =>
                    {
                        format!("FRK-1 {} run by the governor", body.criterion_id)
                    }
                    _ => return None,
                })
            })
            .collect()
    }

    /// Each session started, as its purpose, its agent, and its task, in order.
    fn sessions_with_tasks(harness: &Harness) -> Vec<(SessionStartedBodyPurpose, String, String)> {
        harness
            .events(&[EventKind::SessionStarted])
            .iter()
            .map(|event| match &event.body {
                EventBody::SessionStarted(body) => (
                    body.purpose,
                    event.envelope.ids.agent_id.clone().unwrap_or_default(),
                    event
                        .envelope
                        .ids
                        .task_id
                        .as_ref()
                        .map(|task| task.as_str().to_string())
                        .unwrap_or_default(),
                ),
                other => panic!("a session.started, got {other:?}"),
            })
            .collect()
    }

    /// Runs until idle, answers the one question, runs, approves the epic, runs, accepts its
    /// result with the human's words, and runs again, as the human at a terminal would.
    async fn drive_one_request(
        harness: &Harness,
        orchestrator: &super::Orchestrator,
        epic: &TaskId,
    ) {
        orchestrator.run_until_idle().await.expect("the run idles");
        let question = harness.events(&[EventKind::QuestionAsked]);
        assert_eq!(question.len(), 1, "one question is asked");
        orchestrator
            .handle(Command::QuestionAnswer {
                question_id: question[0].envelope.seq,
                answer: "No, one line.".to_string(),
            })
            .await
            .expect("the human answers");
        orchestrator.run_until_idle().await.expect("the run idles");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: epic.clone(),
                subject: AcceptSubject::Contract,
                message: None,
            })
            .await
            .expect("the human approves the epic");
        orchestrator.run_until_idle().await.expect("the run idles");
        orchestrator
            .handle(Command::HumanAccept {
                task_id: epic.clone(),
                subject: AcceptSubject::Result,
                message: Some("done.txt is on main.".to_string()),
            })
            .await
            .expect("the human accepts the epic");
        orchestrator.run_until_idle().await.expect("the run idles");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_one_request_to_an_accepted_epic() {
        let harness = Harness::new("orch-one-epic", |wire| {
            wire["policy"]["integration"] = serde_json::json!("auto_merge");
        });
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![
            triage_frk_1_large(),
            refine_asks_frk_1(),
            refine_writes_epic_frk_1(),
            plan_breaks_down_frk_1(),
            plan_assigns_frk_2(),
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
            plan_closes_epic_frk_1(),
            accept_frk_1(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let epic: TaskId = "FRK-1".parse().expect("a task id");

        drive_one_request(&harness, &orchestrator, &epic).await;

        let frk_1 = harness.row("FRK-1");
        assert_eq!(
            (frk_1.kind, frk_1.status),
            (TaskKind::Epic, TaskStatus::Accepted)
        );
        let frk_2 = harness.row("FRK-2");
        assert_eq!(frk_2.parent, Some(epic.clone()));
        assert_eq!(frk_2.status, TaskStatus::Accepted);
        assert!(!frk_2.awaiting_integration);
        assert!(
            git_output_in(
                &harness.project.repo.path,
                &["ls-tree", "--name-only", "main"]
            )
            .lines()
            .any(|path| path == "done.txt"),
            "main holds done.txt"
        );
        let started = sessions_with_tasks(&harness);
        let expected: Vec<(SessionStartedBodyPurpose, String, String)> = [
            (SessionStartedBodyPurpose::Triage, "pm", "FRK-1"),
            (SessionStartedBodyPurpose::Refine, "pm", "FRK-1"),
            (SessionStartedBodyPurpose::Refine, "pm", "FRK-1"),
            (SessionStartedBodyPurpose::Plan, "pm", "FRK-1"),
            (SessionStartedBodyPurpose::Plan, "pm", "FRK-2"),
            (SessionStartedBodyPurpose::Implement, "dev-a", "FRK-2"),
            (SessionStartedBodyPurpose::Verify, "dev-b", "FRK-2"),
            (SessionStartedBodyPurpose::Verify, "pm", "FRK-2"),
            (SessionStartedBodyPurpose::Plan, "pm", "FRK-1"),
            (SessionStartedBodyPurpose::Verify, "pm", "FRK-1"),
        ]
        .into_iter()
        .map(|(purpose, agent, task)| (purpose, agent.to_string(), task.to_string()))
        .collect();
        assert_eq!(started, expected);
        assert_eq!(
            milestones(&harness),
            [
                "request.triaged",
                "FRK-1 draft -> refining",
                "question.asked",
                "question.answered",
                "FRK-1 refining -> escalated",
                "escalation.raised approval",
                "human.accepted contract",
                "FRK-1 escalated -> ready",
                "FRK-1 ready -> assigned",
                "FRK-1 assigned -> in_progress",
                "FRK-2 created",
                "FRK-2 integrated",
                "FRK-1 in_progress -> verifying",
                "FRK-1 C1 run by the governor",
                "human.accepted result",
                "FRK-1 verifying -> accepted",
            ]
        );
        assert!(
            harness.events(&[EventKind::ContractEvaluated]).iter().all(
                |event| matches!(&event.body, EventBody::ContractEvaluated(body) if body.passed)
            ),
            "no contract failed the Definition of Ready"
        );
        assert_eq!(adapter.transcripts_left(), 0);
    }

    #[test]
    fn gives_back_every_outcome_the_reply_carried() {
        use super::{CommandError, CommandReport, reply_of, result_of};

        let outcomes = [
            Ok(CommandReport {
                said: "done".to_string(),
                events: vec![7],
            }),
            Err(CommandError::Invalid {
                detail: "a blank answer".to_string(),
            }),
            Err(CommandError::Refused {
                reason: "already_answered: question 7".to_string(),
            }),
            Err(CommandError::NotFound {
                what: "question 9".to_string(),
            }),
            Err(CommandError::Failed {
                detail: "the store failed".to_string(),
            }),
        ];
        for outcome in outcomes {
            assert_eq!(result_of(reply_of(outcome.clone())), outcome);
        }
    }
}
