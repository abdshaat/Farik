//! The orchestrator (`docs/SPEC.md` sections 5.2 and 5.5, F6): Catervas running its team on its own.
//! Each tick reads the board, does the first thing on it that needs doing, running at most one
//! session to its end, and says what it did.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use catervas_core::contract::{TaskContract, TaskId};
use catervas_core::governor::permissions::PermissionTier;
use catervas_core::team::{Team, task_private_folder};
use catervas_protocol::clock::IdSource;
use catervas_protocol::command::{Command, CommandReply, ReplyKind};
use catervas_roles::{KitError, RoleError};
use catervas_store::baseline::{folder_in, make_private_directory};
use catervas_store::files::FilesError;
use catervas_store::repos::{TaskRepository, TaskRepositoryError, task_repository};
use catervas_store::{GitError, StoreError};
use chrono::{DateTime, Utc};

use crate::channel::ChannelError;
use crate::cost::CostError;
use crate::daemon::{CommandHandler, DaemonState};
use crate::forge::{Forge, ForgeError};
use crate::preview::PreviewFactory;
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::{RuntimeAdapter, RuntimeError};
use crate::sleep::Sleeper;
use crate::tools::ToolDeps;
use crate::transitions::TransitionError;

mod design;
#[cfg(test)]
pub(crate) mod fixtures;
pub(crate) mod hand_over;
pub(crate) mod human;
pub(crate) use human::{
    forget_removed_keys, status_effects, update_agent_held, with_status, without_connector,
};
mod integrate;
mod messages;
mod recover;
mod requests;
mod rules;
mod session;
pub(crate) use session::read_roots;
mod spend;
mod verify;
mod work;
pub use work::Work;

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
    /// What prepares and starts a task's preview, for a session given a connector.
    pub previews: Arc<dyn PreviewFactory>,
    /// Where session ids come from.
    pub session_ids: Arc<dyn IdSource + Send + Sync>,
    /// The forge pull requests are opened on, under the `pull_request` policy.
    pub forge: Arc<Forge>,
    /// What a run waits on while every agent with work is asleep.
    pub sleeper: Arc<dyn Sleeper>,
    /// Which agents have a session running in this process, and the turn every tick holds but
    /// while its session runs (ADR 0064).
    pub work: Arc<Work>,
}

/// Why a tick could not finish. Something the governor refused is not one of these: it is an
/// answer, and the tick says what it did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorError {
    /// The log or the board failed.
    Store(StoreError),
    /// A file under `.catervas/` could not be read or written.
    Files(FilesError),
    /// Git failed.
    Git(GitError),
    /// A session could not be started or read.
    Runtime(RuntimeError),
    /// A task's sandbox could not be made.
    Sandbox(SandboxError),
    /// A role could not be loaded.
    Role(RoleError),
    /// A role's kit could not be loaded.
    Kit(KitError),
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
            Self::Kit(error) => write!(formatter, "the kit failed: {error}"),
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

impl From<KitError> for OrchestratorError {
    fn from(error: KitError) -> Self {
        Self::Kit(error)
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

impl From<ChannelError> for OrchestratorError {
    fn from(error: ChannelError) -> Self {
        match error {
            ChannelError::Store(error) => Self::Store(error),
            ChannelError::Files(error) => Self::Files(error),
            ChannelError::Refused { reason } => Self::Refused { reason },
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
    /// Something was done about a delivery.
    Delivery {
        /// The delivery.
        delivery_id: String,
        /// What was done.
        what: String,
    },
    /// An agent answered what it was asked in the channel.
    Conversation {
        /// The agent.
        agent_id: String,
        /// What was done.
        what: String,
    },
    /// An agent answered the user in its one-to-one chat.
    Chat {
        /// The agent.
        agent_id: String,
        /// What was done.
        what: String,
    },
    /// Something was done about a folder change: integrated, put in a pull request, or escalated.
    FolderChange {
        /// The change's number, the n of `docs/folder-<n>`.
        change: u64,
        /// What was done.
        what: String,
    },
}

/// What the ticks driving a project did next (ADR 0064).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// A tick started a session, which runs on while the next ticks go on; its tick reports when
    /// it ends.
    Started {
        /// The agent the session is of.
        agent_id: String,
        /// The task it is about, when there is one.
        task_id: Option<TaskId>,
        /// What it is for, as `session.started` records it.
        purpose: String,
    },
    /// A tick ended, and said this.
    Ticked(TickReport),
}

/// The ticks of a driving process: the running ones, and the sessions they started that `next`
/// has not told of yet.
struct Ticks {
    set: tokio::task::JoinSet<Result<TickReport, OrchestratorError>>,
    started: tokio::sync::mpsc::UnboundedReceiver<work::Started>,
    sender: tokio::sync::mpsc::UnboundedSender<work::Started>,
}

impl Ticks {
    fn new() -> Self {
        let (sender, started) = tokio::sync::mpsc::unbounded_channel();
        Ticks {
            set: tokio::task::JoinSet::new(),
            started,
            sender,
        }
    }

    /// A session started that `next` has not told of yet.
    fn started(&mut self) -> Option<Progress> {
        self.started
            .try_recv()
            .ok()
            .map(work::Started::into_progress)
    }
}

/// What a tick that ended answered; `None` when the set was empty.
fn ended(
    joined: Option<Result<Result<TickReport, OrchestratorError>, tokio::task::JoinError>>,
) -> Option<Result<TickReport, OrchestratorError>> {
    joined.map(|joined| joined.unwrap_or_else(|error| Err(tick_failed(&error))))
}

fn tick_failed(error: &tokio::task::JoinError) -> OrchestratorError {
    OrchestratorError::Refused {
        reason: format!("tick_failed: {error}"),
    }
}

/// How a wait for a sleeping agent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// It is the time waited for, or the minute the wait is capped at (`RECHECK`) passed first,
    /// so that work filed straight into the store, not through a command `handle` sees, is
    /// picked up within a minute (`docs/SPEC.md` 8.2).
    Reached,
    /// `stop` was called first.
    Stopped,
    /// A command the human gave was handled first, or a request was filed in the browser, which
    /// may have made work for an agent awake.
    Woken,
}

/// The longest `wait_until` sleeps before the board is ticked again: work filed straight into the
/// store while a run waits, such as `catervas task create` writing the store directly, does not
/// notify `commands` and would otherwise sit until the sleeping agent wakes.
const RECHECK: chrono::Duration = chrono::Duration::seconds(60);

/// The longest a wait lasts while a finished task waits for its merge: the forge is read once a
/// tick (rule 2), and the owner merging on GitHub or by hand should not wait a minute to be seen.
const MERGE_RECHECK: chrono::Duration = chrono::Duration::seconds(15);

/// Which of the rules a tick runs (`docs/SPEC.md` 8.2): every one, the planning ones, or the
/// refining ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TickRules {
    /// Every rule, as `catervas run` ticks.
    #[default]
    All,
    /// Plan without doing, as `catervas plan` ticks: triage and `draft -> refining` (rule 10),
    /// refining and the governor's judgment (rule 9), the plan sessions that assign and an
    /// approved epic's assignment (rule 8), and an epic's breakdown and close-out (rule 6 for an
    /// epic alone). No cleanup, integration, criterion run, worktree, or start of work.
    Planning,
    /// Triage and refining alone (rules 9 and 10), as `catervas contract new` ticks.
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
        /// What became of the project scan once it landed.
        scan: ScanRefresh,
    },
    /// A pull request was opened for it, at this address.
    PullRequestOpened {
        /// The pull request's address.
        url: String,
    },
    /// Its pull request is open on the forge, waiting for the human.
    AwaitingForge,
    /// Its pull request stopped merging: the integration branch was merged into the task's
    /// branch, the conflicts were left in its worktree and the task went back to its assignee
    /// (ADR 0069).
    SentBack {
        /// The conflicted files.
        paths: Vec<String>,
    },
    /// The project has no `origin` to open a pull request on: the branch waits for the human to
    /// merge it into the integration branch by hand.
    AwaitingHand {
        /// The branch to merge.
        branch: String,
        /// The integration branch it goes into.
        into: String,
    },
    /// It could not land, and the human was told why in these words.
    Escalated {
        /// The escalation's detail.
        detail: String,
        /// What became of the project scan, when the escalation follows a merge (a failed push or
        /// fast-forward).
        scan: Option<ScanRefresh>,
    },
}

/// What became of the project scan after an integration (5.8): the scan every prompt carries is
/// kept current as work lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanRefresh {
    /// The project changed, and `project.md` and the criterion library say so now.
    Refreshed,
    /// Nothing the scan reads changed.
    Unchanged,
    /// The scan was not run, for this reason.
    Skipped {
        /// Why, in words.
        why: String,
    },
    /// The scan or its writing failed, and nothing was changed.
    Failed {
        /// What failed, in words.
        error: String,
    },
}

impl fmt::Display for ScanRefresh {
    /// What is said after the integration's own words: nothing when the scan is unchanged.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refreshed => write!(formatter, "; the project scan was refreshed"),
            Self::Unchanged => Ok(()),
            Self::Skipped { why: reason } | Self::Failed { error: reason } => {
                write!(formatter, "; the project scan was not refreshed: {reason}")
            }
        }
    }
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

/// A task's held sandbox, and whether it was made for Catervas's checks, which see the git store
/// (ADR 0069): an agent's session is never given one of those.
struct Held {
    sandbox: Arc<dyn Sandbox>,
    for_checks: bool,
}

/// Whom a held sandbox is for.
#[derive(Clone, Copy)]
enum SandboxFor<'a> {
    /// An agent's session: it never sees the git store.
    Agent,
    /// Catervas's checks, which see this store read-only when there is one.
    Checks(Option<&'a Path>),
}

/// Catervas running a project's team.
pub struct Orchestrator {
    deps: OrchestratorDeps,
    /// Each task's sandbox, made the first time a session of the task needs one in this process.
    sandboxes: Mutex<BTreeMap<TaskId, Held>>,
    /// Read before each tick `next` starts.
    stopped: AtomicBool,
    /// Notified by `stop`, so that a wait for a sleeping agent ends at once.
    stops: tokio::sync::Notify,
    /// Notified by a command that recorded something, so that a wait ends and the next tick sees
    /// it. One permit is kept when nobody waits: a command handled during a tick ends the wait
    /// after it at once, which costs one tick.
    commands: tokio::sync::Notify,
    /// The ticks `next` has started (ADR 0064). Locked for the whole of a call of `next`.
    ticks: tokio::sync::Mutex<Ticks>,
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
            commands: tokio::sync::Notify::new(),
            ticks: tokio::sync::Mutex::new(Ticks::new()),
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
        let log = &self.deps.tools.log;
        // The ends that dates bring to marketing plans come first, before the pause is read: they
        // start no session, so a paused team has them too (ADR 0042).
        if scope.task_id.is_none() {
            rules::end_marketing_plans(&self.deps).await?;
            // Posts go to Buffer between sessions, with no model, so under Catervas's own pause
            // for a refused key too, and the owner's pause alone holds them (ADR 0042).
            rules::hand_over_posts(&self.deps).await?;
            // The orders nobody decided in time, and the renewals coming up, close and flag with
            // no model too (ADR 0039).
            rules::close_orders_and_flag_renewals(&self.deps)?;
            // The procurement mailbox is read every 15 minutes with no model, in a task of its own
            // so that a slow server never holds the tick, and under a pause too (ADR 0039).
            rules::check_mail(&self.deps);
        }
        if crate::pause::paused(log)? {
            // A paused team still answers its chats (ADR 0026), unless the provider refused the
            // key, with which no chat can be answered either.
            if !crate::pause::key_refused(log)?
                && let Some(report) = rules::chat_alone(self, scope).await?
            {
                return Ok(report);
            }
            return Ok(TickReport::Idle {
                why: "the team is paused; catervas resume starts it again".to_string(),
                until: None,
            });
        }
        rules::tick(self, scope).await
    }

    /// The next thing the ticks driving the project did (ADR 0064): a session started, which runs
    /// on while the next ticks go on, or a tick ended and said what it did. `None` once `stop`
    /// was called and no tick runs.
    ///
    /// A tick takes the turn (`Work`), does everything but run its session, hands the turn back
    /// to run the session, and takes it again to record its end; `next` starts a fresh tick
    /// whenever it is called, so that an agent without a session finds work while others work.
    /// Only a fresh tick's idleness with no tick running is ever answered, so a driver never ends
    /// while sessions run.
    ///
    /// # Errors
    ///
    /// The error a tick ended with; a tick that panicked is `Refused` as `tick_failed`.
    pub async fn next(
        self: &Arc<Self>,
        scope: &TickScope,
    ) -> Option<Result<Progress, OrchestratorError>> {
        let mut guard = self.ticks.lock().await;
        let ticks = &mut *guard;
        loop {
            if let Some(started) = ticks.started() {
                return Some(Ok(started));
            }
            if self.is_stopped() {
                if ticks.set.is_empty() {
                    return None;
                }
                let joined = ticks.set.join_next().await;
                match ended(joined) {
                    Some(Ok(TickReport::Idle { .. })) | None => {}
                    Some(report) => return Some(report.map(Progress::Ticked)),
                }
                continue;
            }
            let turn = self.deps.work.turn().lock_owned().await;
            let me = Arc::clone(self);
            let owned = scope.clone();
            let spawned = ticks.set.spawn(work::holding_the_turn(
                turn,
                ticks.sender.clone(),
                async move { me.tick_within(&owned).await },
            ));
            let spawned = spawned.id();
            let report = loop {
                tokio::select! {
                    biased;
                    Some(started) = ticks.started.recv() => {
                        return Some(Ok(started.into_progress()));
                    }
                    joined = ticks.set.join_next_with_id() => {
                        let Some(joined) = joined else { break None };
                        let (id, result) = match joined {
                            Ok((id, report)) => (id, report),
                            Err(error) => (error.id(), Err(tick_failed(&error))),
                        };
                        match result {
                            Ok(TickReport::Idle { .. }) if id != spawned => {}
                            other => break Some((id == spawned, other)),
                        }
                    }
                }
            };
            match report {
                None => {}
                Some((true, Ok(TickReport::Idle { why, until }))) => {
                    if ticks.set.is_empty() {
                        return Some(Ok(Progress::Ticked(TickReport::Idle { why, until })));
                    }
                    if let Some(report) = self.wait_for_a_tick(ticks, until).await {
                        return Some(report);
                    }
                }
                Some((_, other)) => return Some(other.map(Progress::Ticked)),
            }
        }
    }

    /// After a fresh tick was idle while others run: waits for a running tick to end (answered,
    /// unless idle), a session to start, a command, a filed request, a stop, or the time the
    /// first sleeping agent wakes, at most `RECHECK`; answers nothing for the last four.
    async fn wait_for_a_tick(
        &self,
        ticks: &mut Ticks,
        until: Option<DateTime<Utc>>,
    ) -> Option<Result<Progress, OrchestratorError>> {
        // Taken before the stop is read, so that a stop between the two still ends the wait.
        let stopped = self.stops.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        if self.is_stopped() {
            return None;
        }
        let now = self.deps.tools.clock.now();
        let limit = now + self.recheck();
        let capped = until.map_or(limit, |until| until.min(limit));
        tokio::select! {
            Some(started) = ticks.started.recv() => Some(Ok(started.into_progress())),
            joined = ticks.set.join_next() => match ended(joined) {
                Some(Ok(TickReport::Idle { .. })) | None => None,
                Some(report) => Some(report.map(Progress::Ticked)),
            },
            () = self.deps.sleeper.sleep_until(capped) => None,
            () = stopped => None,
            () = self.commands.notified() => None,
            () = self.deps.daemon.wakes().notified() => None,
        }
    }

    /// How long a wait may last: `MERGE_RECHECK` while any task awaits its integration, else
    /// `RECHECK`. A store error reads as `RECHECK`.
    fn recheck(&self) -> chrono::Duration {
        match self.deps.tools.projections.board() {
            Ok(board) if board.iter().any(|row| row.awaiting_integration) => MERGE_RECHECK,
            _ => RECHECK,
        }
    }

    /// Whether `stop` was called.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Drives the ticks (`next`) until one is idle with no session running and no agent to wait
    /// for, or `stop` was called. A tick idle while an agent sleeps is waited out (`wait_until`),
    /// and the ticks go on. On an error, stops and lets every running session end before
    /// returning it.
    ///
    /// # Errors
    ///
    /// The first error a tick returns.
    pub async fn run_until_idle(self: &Arc<Self>) -> Result<(), OrchestratorError> {
        let scope = TickScope::default();
        loop {
            match self.next(&scope).await {
                None | Some(Ok(Progress::Ticked(TickReport::Idle { until: None, .. }))) => {
                    return Ok(());
                }
                Some(Err(error)) => {
                    self.stop();
                    while self.next(&scope).await.is_some() {}
                    return Err(error);
                }
                Some(Ok(Progress::Ticked(TickReport::Idle {
                    until: Some(until), ..
                }))) => {
                    self.wait_until(until).await;
                }
                Some(Ok(_)) => {}
            }
        }
    }

    /// Waits until `until`, `RECHECK` from now, `stop` is called, or a command the human gave
    /// records something, whichever comes first; at once when `stop` was called before. Capped at
    /// `RECHECK` so the caller ticks again at least that often, whatever `until` is.
    pub async fn wait_until(&self, until: DateTime<Utc>) -> Waited {
        // Taken before the stop is read, so that a stop between the two still ends the wait.
        let stopped = self.stops.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        if self.is_stopped() {
            return Waited::Stopped;
        }
        let capped = until.min(self.deps.tools.clock.now() + self.recheck());
        tokio::select! {
            () = self.deps.sleeper.sleep_until(capped) => Waited::Reached,
            () = stopped => Waited::Stopped,
            () = self.commands.notified() => Waited::Woken,
            () = self.deps.daemon.wakes().notified() => Waited::Woken,
        }
    }

    /// Integrates an accepted task now, as the human asks (`catervas integrate`), whatever
    /// escalations it carries: a pull request opened when none was since acceptance, else the
    /// recorded one read (`AwaitingForge` while open; when closed, `Merged` if the branch is in
    /// the integration branch all the same). Catervas never merges: with no `origin`, `Merged`
    /// once the branch is in the integration branch, else `Refused` as `not_merged`. A task already integrated answers the commit it was integrated at and
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

    /// Adds a folder change to the project now, as the human asks (`catervas integrate
    /// folder-<n>`), whatever escalations it carries, as `integrate` does for an accepted task. A
    /// change already integrated answers the commit it was integrated at.
    ///
    /// # Errors
    ///
    /// `Refused` with `no_such_folder_change` for a number that is none; `Lock` when the
    /// integration lock cannot be taken; the store's, the files', and git's own failures. A merge
    /// or push that cannot land is not an error: it is `Escalated`.
    pub async fn integrate_folder_change(
        &self,
        change: u64,
    ) -> Result<IntegrationOutcome, OrchestratorError> {
        integrate::integrate_folder_change(self, change).await
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
        let handled = human::handle(self, command).await;
        if handled
            .as_ref()
            .is_ok_and(|report| !report.events.is_empty())
        {
            self.commands.notify_one();
        }
        handled
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

    /// Stops `next` and `run_until_idle` from ticking again, and ends a wait at once. Sessions
    /// already running run to their end.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.stops.notify_waiters();
    }

    /// The task's sandbox: the one made for it earlier in this process, or a new one rooted at
    /// its worktree, with the network on when its assignee holds `network`. It never sees the git
    /// store: a sandbox made for the checks is dropped for it.
    fn sandbox_for(
        &self,
        task_id: &TaskId,
        team: &Team,
    ) -> Result<Arc<dyn Sandbox>, OrchestratorError> {
        self.held_sandbox(task_id, team, SandboxFor::Agent)
    }

    /// The task's sandbox for Catervas's own checks (ADR 0069): like `sandbox_for`, and, given
    /// `git_store`, it also sees that store read-only. Reused only by the checks; the next
    /// `sandbox_for` drops it and makes the agent's own.
    fn sandbox_for_checks(
        &self,
        task_id: &TaskId,
        team: &Team,
        git_store: Option<&Path>,
    ) -> Result<Arc<dyn Sandbox>, OrchestratorError> {
        self.held_sandbox(task_id, team, SandboxFor::Checks(git_store))
    }

    /// `sandbox_for` and `sandbox_for_checks`: the held sandbox when it is of the kind asked for
    /// (`whom`, with the store to mount for the checks'), else a
    /// new one that replaces it. A container sandbox's `create` removes a container of the task's
    /// name first, which is what ends the one replaced.
    fn held_sandbox(
        &self,
        task_id: &TaskId,
        team: &Team,
        whom: SandboxFor<'_>,
    ) -> Result<Arc<dyn Sandbox>, OrchestratorError> {
        let for_checks = matches!(whom, SandboxFor::Checks(_));
        let mut sandboxes = self
            .sandboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(held) = sandboxes.get(task_id) {
            if held.for_checks == for_checks {
                return Ok(Arc::clone(&held.sandbox));
            }
            sandboxes.remove(task_id);
        }
        let assignee = self
            .deps
            .tools
            .projections
            .task(task_id)?
            .and_then(|row| row.assignee_id);
        // The network follows the tier for every role (ADR 0061).
        let network = team
            .agents
            .iter()
            .find(|agent| Some(agent.id.as_str()) == assignee.as_deref())
            .is_some_and(|agent| {
                agent
                    .tiers(&team.permissions())
                    .contains(&PermissionTier::Network)
            });
        // The project's other repositories, which a command can read at `/repos/<name>` (5.6).
        let contract = self.deps.tools.files.read_contract(task_id)?;
        let folder = worktree(&self.deps, task_id);
        let project = std::fs::canonicalize(self.deps.tools.files.root()).ok();
        let reads: Vec<(String, PathBuf)> =
            read_roots(&self.deps.tools.files, Some(&contract), &folder)
                .into_iter()
                .filter(|root| Some(&root.path) != project.as_ref())
                .map(|root| (root.name, root.path))
                .collect();
        let project_id = &self.deps.tools.ids.project_id;
        let made = match whom {
            SandboxFor::Checks(Some(store)) => self
                .deps
                .sandboxes
                .create_checking(project_id, task_id, &folder, network, &reads, store)?,
            SandboxFor::Checks(None) | SandboxFor::Agent => self
                .deps
                .sandboxes
                .create(project_id, task_id, &folder, network, &reads)?,
        };
        let sandbox: Arc<dyn Sandbox> = Arc::from(made);
        sandboxes.insert(
            task_id.clone(),
            Held {
                sandbox: Arc::clone(&sandbox),
                for_checks,
            },
        );
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

/// A task's worktree, `.catervas/local/worktrees/<id>` (5.14).
fn worktree(deps: &OrchestratorDeps, task_id: &TaskId) -> PathBuf {
    deps.tools
        .files
        .root()
        .join(".catervas/local/worktrees")
        .join(task_id.as_str())
}

/// The repository `contract` works in: the project folder's own, or the member it names.
fn task_repository_of(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
) -> Result<TaskRepository, TaskRepositoryError> {
    task_repository(
        &deps.tools.files,
        team,
        &deps.tools.git,
        contract.repo.as_ref().map(|repo| repo.as_str()),
    )
}

/// A task's repository that cannot be had, as the refusal that carries its sentence.
fn repository_refused(task_id: &TaskId, error: &TaskRepositoryError) -> OrchestratorError {
    OrchestratorError::Refused {
        reason: format!("task_repository: {}: {error}", task_id.as_str()),
    }
}

/// Where a task's sessions work: its private folder, `.catervas/local/finance` for a Finance
/// Specialist's task and `.catervas/local/procurement` for a Procurement Specialist's, made for its
/// owner alone when it is not there (6.6, 6.10); any other task's worktree (5.14). A session in a
/// folder has no worktree, no branch and no sandbox.
///
/// # Errors
///
/// When the folder cannot be made.
fn session_dir(
    deps: &OrchestratorDeps,
    contract: &TaskContract,
) -> Result<PathBuf, OrchestratorError> {
    let Some(folder) = task_private_folder(contract) else {
        return Ok(worktree(deps, &contract.id));
    };
    let folder = folder_in(deps.tools.files.root(), folder)?;
    make_private_directory(&folder)?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use catervas_core::contract::TaskStatus;
    use catervas_protocol::event::{
        CriterionRecordedBodyRunBy, EventBody, EventKind, NoteWrittenBodyKind,
        SessionStartedBodyPurpose,
    };
    use chrono::{DateTime, Utc};

    use catervas_store::git::fixtures::git_output_in;

    use catervas_core::contract::{TaskId, TaskKind};
    use catervas_protocol::command::{Command, RequestSize};
    use catervas_protocol::event::EscalationRaisedBodyReason;

    use crate::orchestrator::fixtures::Harness;
    use crate::recorded::fixtures::{
        accept_ctv_1, implement_finishes_ctv_1, plan_assigns_ctv_1, plan_assigns_ctv_2,
        plan_breaks_down_ctv_1, plan_closes_epic_ctv_1, refine_asks_ctv_1,
        refine_writes_epic_ctv_1, refine_writes_task_ctv_1, retro, review, review_writes_note,
        triage_ctv_1_large,
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

    /// Each move of CTV-1, as `from -> to`, in order: the changelog task a closed delivery files is
    /// another task's.
    fn moves(harness: &Harness) -> Vec<String> {
        harness
            .events(&[EventKind::TaskTransitioned])
            .iter()
            .filter(|event| {
                event
                    .envelope
                    .ids
                    .task_id
                    .as_ref()
                    .is_some_and(|task| task.as_str() == "CTV-1")
            })
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
        harness.ready("CTV-1");
        let until = at() + chrono::Duration::hours(1);
        harness.asleep("dev-a", until);
        let adapter = harness.recorded(vec![
            plan_assigns_ctv_1(),
            implement_finishes_ctv_1(),
            review_writes_note(),
            accept_ctv_1(),
            review(),
            retro(),
        ]);
        let orchestrator = std::sync::Arc::new(harness.orchestrator_at(adapter.clone(), at()));

        harness
            .run_until_idle_past_the_changelog(&orchestrator)
            .await
            .expect("the run ends idle");

        assert_eq!(orchestrator.deps.tools.clock.now(), until);
        assert_eq!(
            sessions(&harness),
            vec![
                (SessionStartedBodyPurpose::Plan, "pm".to_string()),
                (SessionStartedBodyPurpose::Implement, "dev-a".to_string()),
                (SessionStartedBodyPurpose::Verify, "dev-b".to_string()),
                (SessionStartedBodyPurpose::Verify, "pm".to_string()),
                // The delivery the acceptance closed: its review, then its retro.
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
            ]
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
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
    async fn wakes_a_wait_on_a_human_command() {
        let harness = Harness::new("orch-wait-command", |_| {});
        harness.a_request("Add done.txt and its check");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let (ended, ()) = tokio::join!(
            waited(&orchestrator, at() + chrono::Duration::hours(1)),
            async {
                tokio::task::yield_now().await;
                orchestrator
                    .handle(Command::RequestTriage {
                        task_id: "CTV-1".parse().expect("a task id"),
                        size: RequestSize::Small,
                        reason: "Sized by the human.".to_string(),
                    })
                    .await
                    .expect("the human sizes the request");
            }
        );

        assert_eq!(ended, super::Waited::Woken);
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
    async fn caps_a_wait_at_a_minute_so_the_board_is_rechecked() {
        let harness = Harness::new("orch-wait-cap", |_| {});
        let orchestrator = harness.orchestrator_at(harness.recorded(Vec::new()), at());

        let ended = waited(&orchestrator, at() + chrono::Duration::hours(1)).await;

        assert_eq!(ended, super::Waited::Reached);
        assert_eq!(
            orchestrator.deps.tools.clock.now(),
            at() + chrono::Duration::seconds(60),
            "a wait an hour away is capped at a minute, not run to the end"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn caps_a_wait_at_fifteen_seconds_while_a_merge_waits() {
        let harness = Harness::new("orch-wait-merge-cap", |_| {});
        harness.accepted("CTV-1");
        let orchestrator = harness.orchestrator_at(harness.recorded(Vec::new()), at());

        let ended = waited(&orchestrator, at() + chrono::Duration::hours(1)).await;

        assert_eq!(ended, super::Waited::Reached);
        assert_eq!(
            orchestrator.deps.tools.clock.now(),
            at() + chrono::Duration::seconds(15),
            "a merge waiting on the owner is looked for every fifteen seconds"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one journey, every fact asserted in order"
    )]
    async fn takes_one_task_from_ready_to_accepted() {
        let harness = Harness::new("orch-one-task", |wire| {
            wire["policy"]["integration"] = serde_json::json!("auto_merge");
        });
        harness.ready("CTV-1");
        let adapter = harness.recorded(vec![
            plan_assigns_ctv_1(),
            implement_finishes_ctv_1(),
            review_writes_note(),
            accept_ctv_1(),
            review(),
            retro(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let base = git_output_in(&harness.project.repo.path, &["rev-parse", "main"]);

        harness
            .run_until_idle_past_the_changelog(&orchestrator)
            .await
            .expect("the run ends idle");
        // Catervas never merges: the run waits for the owner, who merges the branch by hand.
        assert!(harness.row("CTV-1").awaiting_integration);
        harness.merge_by_hand("CTV-1");
        harness
            .run_until_idle_past_the_changelog(&orchestrator)
            .await
            .expect("the run ends idle");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
        let git = &harness.project.deps.git;
        let branch = harness.branch("CTV-1");
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
                // The delivery the acceptance closed: its review, then its retro.
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
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
        // The task's four sessions are charged to it; the delivery's review and retro are not.
        let (own, ceremonies): (Vec<_>, Vec<_>) = costs
            .iter()
            .partition(|cost| cost.envelope.ids.task_id.is_some());
        assert_eq!((own.len(), ceremonies.len()), (4, 2), "{costs:?}");
        for cost in &own {
            assert_eq!(
                cost.envelope.ids.task_id.as_ref().map(|task| task.as_str()),
                Some("CTV-1")
            );
        }
        assert_eq!(adapter.started().len(), 6);
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
        assert!(!harness.worktree("CTV-1").exists());
        assert_eq!(
            git_output_in(&harness.project.repo.path, &["branch", "--list", &branch]).trim(),
            branch
        );
    }

    /// The Product Manager's triage of CTV-1 as `small`, a task: `triage_ctv_1_large` with its
    /// size changed, since the size is the only thing a replayed triage decides.
    fn triage_ctv_1_small() -> crate::recorded::Transcript {
        let large = triage_ctv_1_large().lines().collect::<Vec<_>>().join("\n");
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
            triage_ctv_1_small(),
            refine_writes_task_ctv_1(),
            plan_assigns_ctv_1(),
            implement_finishes_ctv_1(),
            review_writes_note(),
            accept_ctv_1(),
            review(),
            retro(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        harness
            .run_until_idle_past_the_changelog(&orchestrator)
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
                // The delivery the acceptance closed: its review, then its retro.
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
                (SessionStartedBodyPurpose::Ceremony, "pm".to_string()),
            ]
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Accepted);
        assert!(
            harness.events(&[EventKind::BudgetExhausted]).is_empty(),
            "no budget ran out"
        );
        assert_eq!(adapter.transcripts_left(), 0);
        let contract = harness
            .project
            .deps
            .files
            .read_contract(&"CTV-1".parse().expect("a task id"))
            .expect("the contract reads");
        assert_eq!(
            contract.budget.max_sessions.get(),
            14,
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
                    EventBody::RequestTriaged(_) if task == "CTV-1" => {
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
                    EventBody::TaskTransitioned(body) if task == "CTV-1" => {
                        format!("CTV-1 {} -> {}", body.from, body.to)
                    }
                    EventBody::TaskCreated(_) if task == "CTV-2" => "CTV-2 created".to_string(),
                    EventBody::TaskIntegrated(_) => format!("{task} integrated"),
                    EventBody::CriterionRecorded(body)
                        if task == "CTV-1" && body.recorded_by == "governor" =>
                    {
                        format!("CTV-1 {} run by the governor", body.criterion_id)
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

    /// Runs until idle, answers the one question, runs, and runs again, as the human at a
    /// terminal would: Catervas accepts the epic's result on its criteria (ADR 0061).
    async fn drive_one_request(harness: &Harness, orchestrator: &super::Orchestrator) {
        harness
            .run_until_idle_past_the_changelog(orchestrator)
            .await
            .expect("the run idles");
        let question = harness.events(&[EventKind::QuestionAsked]);
        assert_eq!(question.len(), 1, "one question is asked");
        orchestrator
            .handle(Command::QuestionAnswer {
                question_id: question[0].envelope.seq,
                answer: "No, one line.".to_string(),
            })
            .await
            .expect("the human answers");
        harness
            .run_until_idle_past_the_changelog(orchestrator)
            .await
            .expect("the run idles");
        // No contract waits for the owner (ADR 0061), so the epic is ready and runs on.
        // Catervas never merges: the owner merges the task's branch, and the epic goes on.
        harness.merge_by_hand("CTV-2");
        harness
            .run_until_idle_past_the_changelog(orchestrator)
            .await
            .expect("the run idles");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_one_request_to_an_accepted_epic() {
        let harness = Harness::new("orch-one-epic", |wire| {
            wire["policy"]["integration"] = serde_json::json!("auto_merge");
        });
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![
            triage_ctv_1_large(),
            refine_asks_ctv_1(),
            refine_writes_epic_ctv_1(),
            plan_breaks_down_ctv_1(),
            plan_assigns_ctv_2(),
            implement_finishes_ctv_1(),
            review_writes_note(),
            accept_ctv_1(),
            plan_closes_epic_ctv_1(),
            review(),
            retro(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        let epic: TaskId = "CTV-1".parse().expect("a task id");

        drive_one_request(&harness, &orchestrator).await;

        let ctv_1 = harness.row("CTV-1");
        assert_eq!(
            (ctv_1.kind, ctv_1.status),
            (TaskKind::Epic, TaskStatus::Accepted)
        );
        let ctv_2 = harness.row("CTV-2");
        assert_eq!(ctv_2.parent, Some(epic.clone()));
        assert_eq!(ctv_2.status, TaskStatus::Accepted);
        assert!(!ctv_2.awaiting_integration);
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
            (SessionStartedBodyPurpose::Triage, "pm", "CTV-1"),
            (SessionStartedBodyPurpose::Refine, "pm", "CTV-1"),
            (SessionStartedBodyPurpose::Refine, "pm", "CTV-1"),
            (SessionStartedBodyPurpose::Plan, "pm", "CTV-1"),
            (SessionStartedBodyPurpose::Plan, "pm", "CTV-2"),
            (SessionStartedBodyPurpose::Implement, "dev-a", "CTV-2"),
            (SessionStartedBodyPurpose::Verify, "dev-b", "CTV-2"),
            (SessionStartedBodyPurpose::Verify, "pm", "CTV-2"),
            (SessionStartedBodyPurpose::Plan, "pm", "CTV-1"),
            // The delivery the epic's acceptance closed: its review, then its retro.
            (SessionStartedBodyPurpose::Ceremony, "pm", ""),
            (SessionStartedBodyPurpose::Ceremony, "pm", ""),
        ]
        .into_iter()
        .map(|(purpose, agent, task)| (purpose, agent.to_string(), task.to_string()))
        .collect();
        assert_eq!(started, expected);
        assert_eq!(
            milestones(&harness),
            [
                "request.triaged",
                "CTV-1 draft -> refining",
                "question.asked",
                "question.answered",
                "CTV-1 refining -> ready",
                "CTV-1 ready -> assigned",
                "CTV-1 assigned -> in_progress",
                "CTV-2 created",
                "CTV-1 in_progress -> verifying",
                "CTV-2 integrated",
                "CTV-1 C1 run by the governor",
                "CTV-1 C2 run by the governor",
                "CTV-1 verifying -> accepted",
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

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_work_in_a_folder_reached_through_a_link() {
        use std::os::unix::fs::symlink;

        // A session in a folder that is a link would work where the link points (5.6, 6.6).
        let harness = Harness::with_finance("orch-session-dir-link");
        harness.finance_task("CTV-1", None);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let contract = orchestrator
            .deps
            .tools
            .files
            .read_contract(&"CTV-1".parse().expect("a task id"))
            .expect("the contract");
        let folder = harness.finance_folder();
        let outside = harness
            .project
            .repo
            .path
            .parent()
            .expect("a parent")
            .join(format!("{}-session-dir-outside", std::process::id()));
        std::fs::create_dir_all(&outside).expect("made");
        std::fs::create_dir_all(folder.parent().expect("a parent")).expect("made");
        symlink(&outside, &folder).expect("a link");

        let error = super::session_dir(&orchestrator.deps, &contract).expect_err("refused");

        assert!(error.to_string().contains("is a link"), "{error}");
        // Nothing was made where the link points.
        assert_eq!(std::fs::read_dir(&outside).expect("read").count(), 0);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn makes_a_tasks_sandbox_hold_its_sibling_repositories() {
        use std::sync::Arc;

        use crate::orchestrator::fixtures::CountingSandboxFactory;

        let (harness, members) = Harness::project_folder("sandbox-siblings", &["app", "infra"]);
        harness.assigned_in("CTV-1", "iris", "ada", "app");
        harness.assigned("CTV-2", "iris", "ada");
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator =
            harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes.clone());
        let team = orchestrator.deps.tools.files.read_team().expect("the team");
        let path = |index: usize| members[index].path.canonicalize().expect("a member");

        for task in ["CTV-1", "CTV-2"] {
            orchestrator
                .sandbox_for(&task.parse().expect("a task id"), &team)
                .expect("a sandbox is made");
        }

        // A task in `app` mounts `infra`; a task in the project folder mounts both.
        assert_eq!(
            sandboxes.reads("CTV-1"),
            [vec![("infra".to_string(), path(1))]]
        );
        assert_eq!(
            sandboxes.reads("CTV-2"),
            [vec![
                ("app".to_string(), path(0)),
                ("infra".to_string(), path(1))
            ]]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_agent_session_never_reuses_the_checks_sandbox() {
        use std::sync::Arc;

        use crate::orchestrator::fixtures::CountingSandboxFactory;

        let harness = Harness::new("sandbox-checks", |_| {});
        harness.assigned("CTV-1", "dev-a", "dev-b");
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator =
            harness.orchestrator_with(harness.recorded(Vec::new()), sandboxes.clone());
        let team = orchestrator.deps.tools.files.read_team().expect("the team");
        let task: TaskId = "CTV-1".parse().expect("a task id");
        let store = std::path::Path::new("/tmp/catervas-the-store/.git");

        let checking = orchestrator
            .sandbox_for_checks(&task, &team, Some(store))
            .expect("the checks' sandbox is made");
        let again = orchestrator
            .sandbox_for_checks(&task, &team, Some(store))
            .expect("the checks' sandbox is reused");
        assert!(Arc::ptr_eq(&checking, &again));
        assert_eq!(sandboxes.stores("CTV-1"), [store.to_path_buf()]);

        let agent = orchestrator
            .sandbox_for(&task, &team)
            .expect("the agent's sandbox is made");
        assert!(!Arc::ptr_eq(&checking, &agent), "an agent never gets it");
        let again = orchestrator
            .sandbox_for(&task, &team)
            .expect("the agent's sandbox is reused");
        assert!(Arc::ptr_eq(&agent, &again));
        assert_eq!(sandboxes.created("CTV-1"), 2);
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

    /// The team of `pm`, `dev-a` and `arch`, an Architect, with a WIP limit of one.
    fn a_developer_and_an_architect(wire: &mut serde_json::Value) {
        use catervas_core::team::fixtures::an_agent_wire;
        wire["agents"] = serde_json::json!([
            an_agent_wire("pm", "product_manager"),
            an_agent_wire("dev-a", "software_developer"),
            an_agent_wire("arch", "architect"),
        ]);
    }

    /// A task of a Developer that an Architect reviews, whose change may touch `done.txt` and
    /// `site/style.css`.
    fn reviewed_by_the_architect(wire: &mut serde_json::Value) {
        wire["reviewer_role"] = serde_json::json!("architect");
        wire["allowed_paths"] = serde_json::json!(["done.txt", "site/style.css"]);
    }

    /// The next progress of `orchestrator`, within ten seconds.
    async fn next_progress(orchestrator: &std::sync::Arc<super::Orchestrator>) -> super::Progress {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            orchestrator.next(&super::TickScope::default()),
        )
        .await
        .expect("the next progress came within ten seconds")
        .expect("the orchestrator was not stopped")
        .expect("the tick ran")
    }

    /// Progresses until one matches, at most ten; every one seen, the match last.
    async fn until(
        orchestrator: &std::sync::Arc<super::Orchestrator>,
        matches: impl Fn(&super::Progress) -> bool,
    ) -> Vec<super::Progress> {
        let mut seen = Vec::new();
        for _ in 0..10 {
            let progress = next_progress(orchestrator).await;
            let done = matches(&progress);
            seen.push(progress);
            if done {
                return seen;
            }
        }
        panic!("no progress matched in ten: {seen:?}");
    }

    /// Whether `progress` is a start of `agent`'s `purpose` session about `task`.
    fn started_of(
        progress: &super::Progress,
        agent: &str,
        task: Option<&str>,
        purpose: &str,
    ) -> bool {
        matches!(progress, super::Progress::Started { agent_id, task_id, purpose: what }
            if agent_id == agent && task_id.as_ref().map(|task| task.as_str()) == task && what == purpose)
    }

    /// Whether `progress` is a tick that acted on `task`, and said something starting `said`.
    fn acted_on(progress: &super::Progress, task: &str, said: &str) -> bool {
        matches!(progress, super::Progress::Ticked(super::TickReport::Acted { task_id, what })
            if task_id.as_str() == task && what.starts_with(said))
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reviews_one_task_while_its_developer_builds_the_next() {
        use crate::orchestrator::fixtures::HeldAdapter;
        use crate::recorded::fixtures::implement_css_ctv_2;
        use crate::tools::fixtures::at;
        use catervas_store::activity::{ActivityState, activity};

        let harness = Harness::new("parallel-scenario", a_developer_and_an_architect);
        harness.in_progress_with("CTV-1", "dev-a", "arch", reviewed_by_the_architect);
        harness.assigned_with("CTV-2", "dev-a", "arch", reviewed_by_the_architect);
        let adapter = HeldAdapter::new(harness.recorded(vec![
            implement_css_ctv_2(),
            review_writes_note(),
            implement_finishes_ctv_1(),
        ]));
        let orchestrator = std::sync::Arc::new(harness.orchestrator(adapter.clone()));

        // (a) The Developer builds CTV-1 and sends it to review.
        until(&orchestrator, |p| {
            started_of(p, "dev-a", Some("CTV-1"), "implement")
        })
        .await;
        adapter.release("dev-a");
        until(&orchestrator, |p| acted_on(p, "CTV-1", "")).await;
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Verifying);

        // (b) The Architect reviews CTV-1 while the Developer builds CTV-2.
        until(&orchestrator, |p| {
            started_of(p, "arch", Some("CTV-1"), "verify")
        })
        .await;
        until(&orchestrator, |p| {
            started_of(p, "dev-a", Some("CTV-2"), "implement")
        })
        .await;
        assert_eq!(adapter.running(), ["arch", "dev-a"]);
        assert_eq!(
            adapter.starts().last().expect("a start").alongside,
            ["arch"]
        );
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        let now = activity(&deps.log, &deps.projections, &deps.files, &team, at())
            .expect("the activity reads");
        let of = |agent: &str| {
            now.iter()
                .find(|row| row.agent_id == agent)
                .unwrap_or_else(|| panic!("{agent} has an activity"))
        };
        let (dev, arch) = (of("dev-a"), of("arch"));
        assert_eq!(dev.state, ActivityState::Working);
        assert_eq!(
            dev.task_id.as_ref().map(|task| task.as_str()),
            Some("CTV-2")
        );
        assert_eq!(dev.line, format!("Building {}", harness.row("CTV-2").title));
        assert_eq!(arch.state, ActivityState::Working);
        assert_eq!(
            arch.task_id.as_ref().map(|task| task.as_str()),
            Some("CTV-1")
        );
        assert_eq!(
            arch.line,
            format!("Reviewing {}", harness.row("CTV-1").title)
        );

        // (c) The review ends: CTV-1 is sent back while the Developer still builds CTV-2.
        adapter.release("arch");
        until(&orchestrator, |p| {
            acted_on(p, "CTV-1", "rejected it for C1")
        })
        .await;
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Rejected);
        assert_eq!(adapter.running(), ["dev-a"]);
        assert_eq!(adapter.starts().len(), 3);

        // (d) The Developer finishes CTV-2, and reworks CTV-1 while the Architect reviews CTV-2.
        adapter.release("dev-a");
        let seen = until(&orchestrator, |p| {
            started_of(p, "dev-a", Some("CTV-1"), "implement")
        })
        .await;
        assert!(
            seen.iter()
                .any(|p| started_of(p, "arch", Some("CTV-2"), "verify")),
            "the Architect reviews CTV-2: {seen:?}"
        );
        assert_eq!(adapter.running(), ["arch", "dev-a"]);
        assert_eq!(
            adapter.starts().last().expect("a start").alongside,
            ["arch"]
        );
        assert_eq!(harness.row("CTV-1").iteration, 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_one_session_per_agent_at_a_time() {
        use crate::orchestrator::fixtures::HeldAdapter;
        use crate::recorded::fixtures::reads_a_file;

        let harness = Harness::new("parallel-one-each", |wire| {
            wire["policy"]["wip_limit_per_agent"] = serde_json::json!(2);
        });
        harness.in_progress("CTV-1", "dev-a", "dev-b");
        harness.in_progress("CTV-2", "dev-a", "dev-b");
        harness.in_progress("CTV-3", "dev-b", "dev-a");
        let adapter = HeldAdapter::new(harness.recorded(vec![reads_a_file()]));
        let orchestrator = std::sync::Arc::new(harness.orchestrator(adapter.clone()));

        let first = next_progress(&orchestrator).await;
        assert!(
            started_of(&first, "dev-a", Some("CTV-1"), "implement"),
            "{first:?}"
        );
        let second = next_progress(&orchestrator).await;
        assert!(
            started_of(&second, "dev-b", Some("CTV-3"), "implement"),
            "{second:?}"
        );
        adapter.release("dev-a");
        until(&orchestrator, |p| acted_on(p, "CTV-1", "")).await;
        until(
            &orchestrator,
            |p| matches!(p, super::Progress::Started { agent_id, .. } if agent_id == "dev-a"),
        )
        .await;
        let starts = adapter.starts();
        assert_eq!(starts.len(), 3, "{starts:?}");
        for start in starts
            .iter()
            .filter(|start| start.purpose != crate::session::SessionPurpose::Chat)
        {
            assert!(!start.alongside.contains(&start.agent), "{start:?}");
        }
        assert_eq!(
            harness.events(&[EventKind::SessionStarted]).len(),
            3,
            "three sessions were recorded as started"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_chat_while_its_agent_works() {
        use crate::orchestrator::fixtures::HeldAdapter;
        use crate::recorded::fixtures::chat_answers_with_a_request;
        use crate::session::SessionPurpose;
        use catervas_core::budget::CHAT_SESSION_LIMITS;
        use catervas_core::team::Effort;

        let harness = Harness::new("parallel-chat-fork", |_| {});
        harness.in_progress("CTV-1", "dev-a", "dev-b");
        let inner = harness.recorded(vec![chat_answers_with_a_request()]);
        let adapter = HeldAdapter::new(inner.clone());
        let orchestrator = std::sync::Arc::new(harness.orchestrator(adapter.clone()));
        until(&orchestrator, |p| {
            started_of(p, "dev-a", Some("CTV-1"), "implement")
        })
        .await;
        let deps = &harness.project.deps;
        for text in ["What are you on?", "And after that?"] {
            crate::chat::post_chat(
                &deps.log,
                deps.clock.as_ref(),
                &deps.ids,
                crate::chat::NewChatMessage {
                    heard: None,
                    chat: "dev-a".to_string(),
                    author: "human".to_string(),
                    text: text.to_string(),
                    in_reply_to: None,
                    request: None,
                    session_id: None,
                    as_task: false,
                },
            )
            .expect("the chat message is recorded");
        }
        until(&orchestrator, |p| started_of(p, "dev-a", None, "chat")).await;
        assert_eq!(
            adapter.starts().last().expect("a start").alongside,
            ["dev-a"]
        );
        adapter.release_purpose("dev-a", SessionPurpose::Chat);
        until(&orchestrator, |p| {
            matches!(p, super::Progress::Ticked(super::TickReport::Chat { agent_id, .. })
                if agent_id == "dev-a")
        })
        .await;
        let replies: Vec<_> = harness
            .events(&[EventKind::ChatMessagePosted])
            .into_iter()
            .filter(|event| match &event.body {
                EventBody::ChatMessagePosted(body) => body.author == "dev-a",
                _ => false,
            })
            .collect();
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert!(
            !harness
                .events(&[EventKind::SessionEnded])
                .iter()
                .any(|event| event.envelope.ids.task_id.is_some()),
            "the work session has not ended"
        );
        assert_eq!(adapter.running(), ["dev-a"]);
        assert_eq!(adapter.starts().len(), 2, "no second fork started");
        let spec = &inner.started()[0];
        assert_eq!(spec.purpose, SessionPurpose::Chat);
        assert_eq!(spec.effort, Effort::Low);
        assert_eq!(spec.limits, CHAT_SESSION_LIMITS);
        assert!(
            spec.initial_prompt.contains(&format!(
                "What you are doing now: CTV-1 \"{}\" (In progress).",
                harness.row("CTV-1").title
            )) && spec.initial_prompt.ends_with("And after that?"),
            "{}",
            spec.initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_every_running_session_once_the_day_is_spent() {
        use crate::orchestrator::fixtures::HeldAdapter;
        use crate::recorded::fixtures::UsageThenWaitAdapter;
        use catervas_core::pricing::Usage;
        use catervas_protocol::event::{BudgetExhaustedBodyScope, SessionEndedBodyReason};

        let harness = Harness::new("parallel-day-spent", |_| {});
        harness.spent(None, "s-0", 19.75);
        harness.in_progress("CTV-1", "dev-a", "dev-b");
        harness.in_progress("CTV-2", "dev-b", "dev-a");
        let adapter = HeldAdapter::new(std::sync::Arc::new(UsageThenWaitAdapter::completing(
            Usage {
                input_tokens: 300_000,
                ..Usage::default()
            },
        )));
        let orchestrator = std::sync::Arc::new(harness.orchestrator(adapter.clone()));
        until(&orchestrator, |p| {
            started_of(p, "dev-a", Some("CTV-1"), "implement")
        })
        .await;
        until(&orchestrator, |p| {
            started_of(p, "dev-b", Some("CTV-2"), "implement")
        })
        .await;
        adapter.release("dev-a");
        let acted = |p: &super::Progress| {
            matches!(p, super::Progress::Ticked(super::TickReport::Acted { .. }))
        };
        until(&orchestrator, acted).await;
        until(&orchestrator, acted).await;

        assert!(adapter.running().is_empty(), "{:?}", adapter.running());
        let ended = harness.events(&[EventKind::SessionEnded]);
        let of_dev_b: Vec<_> = ended
            .iter()
            .filter(|event| event.envelope.ids.agent_id.as_deref() == Some("dev-b"))
            .collect();
        assert_eq!(of_dev_b.len(), 1, "{ended:?}");
        assert!(
            matches!(&of_dev_b[0].body, EventBody::SessionEnded(body)
                if body.reason == SessionEndedBodyReason::Aborted),
            "{:?}",
            of_dev_b[0]
        );
        let costs: Vec<_> = harness
            .events(&[EventKind::CostRecorded])
            .into_iter()
            .filter(|event| event.envelope.ids.agent_id.as_deref() == Some("dev-b"))
            .collect();
        assert_eq!(costs.len(), 1, "{costs:?}");
        assert!(
            matches!(&costs[0].body, EventBody::CostRecorded(body) if body.cost_usd == 0.0),
            "{:?}",
            costs[0]
        );
        assert!(harness.events(&[EventKind::EscalationRaised]).is_empty());
        assert_eq!(harness.row("CTV-1").status, TaskStatus::InProgress);
        assert_eq!(harness.row("CTV-2").status, TaskStatus::InProgress);
        let exhausted = harness.events(&[EventKind::BudgetExhausted]);
        assert!(
            matches!(exhausted.as_slice(), [event] if matches!(&event.body,
                EventBody::BudgetExhausted(body) if body.scope == BudgetExhaustedBodyScope::DayUsd)),
            "{exhausted:?}"
        );
        assert!(matches!(
            next_progress(&orchestrator).await,
            super::Progress::Ticked(super::TickReport::Idle { why, until: None })
                if why == "the team's daily budget is spent"
        ));
        assert_eq!(adapter.starts().len(), 2);
    }
}
