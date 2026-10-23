//! Who drives a project, and how a command reaches it (ADR 0014): one process holds the run lock
//! for as long as it drives, and a command typed anywhere else is sent to that process's daemon,
//! or handled here, under the lock, when nothing drives.

use std::collections::BTreeMap;
use std::fs::{File, TryLockError};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use catervas_core::claude_account::AccountKind;
use catervas_protocol::command::{Command, command_to_value, reply_from_value};
use catervas_runtime::account::{AccountPort, ClaudeAccount};
use catervas_runtime::claude::{ClaudeAdapter, ClaudeConfig, credential_variable};
use catervas_runtime::daemon::web::{BrowserSessions, ConnectCodes, WebState};
use catervas_runtime::daemon::{DaemonConfig, DaemonHandle, DaemonState, PortChoice, serve};
use catervas_runtime::forge::Forge;
use catervas_runtime::orchestrator::{
    CommandError, CommandReport, Orchestrator, OrchestratorDeps, OrchestratorError, RecoveryReport,
    command_handler, result_of, sandbox_recreator,
};
use catervas_runtime::sleep::{Sleeper, TokioSleeper};
use catervas_runtime::slots::Slots;
use catervas_runtime::{
    AVAILABLE_FOR, DockerPreviewFactory, DockerSandboxFactory, HostSandboxFactory, NoPreviews,
    PolledPreviews, PreviewFactory, RuntimeAdapter, RuntimeError, SANDBOX_IMAGE, SandboxFactory,
    SessionHandle, SessionSpec, Templates,
};
use catervas_store::files::Sandbox;
use serde_json::Value;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::daemon_client::{ClientError, DaemonAddress, exchange, read_daemon_file};
use crate::doctor::unpriced;
use crate::project::{Project, tool_deps};
use crate::state::{home_of, make_state_dir, state_dir};
use crate::{CliIo, Engine, Interrupts};

/// The lock the process driving a project holds, under the gitignored `.catervas/local/`.
pub(crate) const RUN_LOCK: &str = ".catervas/local/run.lock";
/// Where the driving process's daemon says how to reach it.
pub(crate) const DAEMON_FILE: &str = ".catervas/local/daemon.json";
/// How long a command sent to the driving process may take: `integrate` may push.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// The project's run lock, held until it is dropped, which the process's end also does.
pub(crate) struct RunLock {
    _file: File,
}

/// Takes the run lock of the project at `root`, or answers `None` when another process holds it.
///
/// # Errors
///
/// A sentence saying the lock file cannot be opened or locked.
pub(crate) fn try_lock(root: &Path) -> Result<Option<RunLock>, String> {
    let path = root.join(RUN_LOCK);
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("{} cannot be made: {error}", directory.display()))?;
    }
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| format!("{} cannot be opened: {error}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(RunLock { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(format!("{} cannot be locked: {error}", path.display()))
        }
    }
}

/// Does what the human asked: handled here, holding the run lock while it runs, when nothing
/// drives the project; sent to the process that does, when something does. `name` is the command
/// as the person typed it, for the sentence a session it would need gives.
///
/// # Errors
///
/// The `CommandError` the handling process answered; `Failed` when the lock cannot be read or
/// the driving process cannot be asked.
pub(crate) fn command(
    project: &Project,
    command: Command,
    name: &str,
    io: &CliIo<'_>,
) -> Result<CommandReport, CommandError> {
    match try_lock(&project.root).map_err(|detail| CommandError::Failed { detail })? {
        Some(lock) => {
            let ended = match &command {
                Command::MarketingPlanEnd { plan, .. } => Some(plan.clone()),
                _ => None,
            };
            let handled = handle_here(project, command, name, io).map(|report| match &ended {
                Some(plan) => saying_when_ads_pause(project, io, plan, report),
                None => report,
            });
            drop(lock);
            handled
        }
        None => send(&project.root, &command),
    }
}

/// `report`, the answer to ending `plan` in a process that drives nothing, with the sentence that
/// Catervas pauses the plan's ads when a process does, when it made some that are still to pause.
fn saying_when_ads_pause(
    project: &Project,
    io: &CliIo<'_>,
    plan: &str,
    mut report: CommandReport,
) -> CommandReport {
    let left = tool_deps(project, io)
        .ok()
        .and_then(|tools| catervas_runtime::marketing::ads::ads_left_to_pause(&tools, plan).ok());
    if left == Some(true) {
        report.said = format!(
            "{}. Catervas pauses {plan}'s ads when it next runs.",
            report.said
        );
    }
    report
}

/// One of phase 2's writes: done here by `here`, holding the run lock while it writes, when nothing
/// drives the project; sent to the process that does as `command`, printing what it said, when
/// something does.
///
/// # Errors
///
/// `here`'s refusal, `command`'s, or the driving process's.
pub(crate) fn here_or_sent(
    project: &Project,
    here: impl FnOnce() -> Result<crate::Report, String>,
    command: impl FnOnce() -> Result<Command, String>,
) -> Result<crate::Report, String> {
    match try_lock(&project.root)? {
        Some(lock) => {
            let done = here();
            drop(lock);
            done
        }
        None => crate::human::said(send(&project.root, &command()?)),
    }
}

/// Sends `command` to the daemon of the process driving the project at `root`, and answers its
/// reply as `handle` would have. Never retried here: the command may have been applied.
///
/// # Errors
///
/// The reply's `CommandError`; `Failed` when there is no daemon yet, or it cannot be reached or
/// does not answer with a reply.
pub(crate) fn send(root: &Path, command: &Command) -> Result<CommandReport, CommandError> {
    let may_still = |detail: String| CommandError::Failed {
        detail: format!(
            "{detail}; the command may still take effect: catervas log shows whether it did"
        ),
    };
    let answer = match exchange(
        &root.join(DAEMON_FILE),
        "/command",
        &command_to_value(command).to_string(),
        COMMAND_TIMEOUT,
    ) {
        Ok(answer) => answer,
        Err(ClientError::NoDaemon { .. }) => {
            return Err(CommandError::Failed {
                detail: "another catervas process holds this project's run lock and serves no \
                         daemon yet: try again in a moment"
                    .to_string(),
            });
        }
        Err(ClientError::Failed { detail }) => return Err(may_still(detail)),
    };
    let value: Value = serde_json::from_str(&answer)
        .map_err(|error| may_still(format!("the daemon's answer is not JSON: {error}")))?;
    let reply = reply_from_value(&value)
        .map_err(|_| may_still(format!("the daemon's answer is not a reply: {value}")))?;
    result_of(reply)
}

/// Handles `command` in this process, on an orchestrator that starts no sessions.
fn handle_here(
    project: &Project,
    command: Command,
    name: &str,
    io: &CliIo<'_>,
) -> Result<CommandReport, CommandError> {
    let failed = |detail: String| CommandError::Failed { detail };
    let orchestrator = command_orchestrator(project, name, io).map_err(failed)?;
    let runtime = runtime().map_err(failed)?;
    runtime.block_on(orchestrator.handle(command))
}

/// A multi-thread runtime for a command that needs the orchestrator; `run_cli` blocks on it, so
/// that the command line stays synchronous.
///
/// # Errors
///
/// A sentence saying the runtime could not be built.
pub(crate) fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("the runtime could not be started: {error}"))
}

/// The orchestrator a command handled in this process runs on: its daemon state is not served,
/// and its adapter starts no session. Its sandboxes are the host's, which `handle` never makes or
/// removes.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub(crate) fn command_orchestrator(
    project: &Project,
    name: &str,
    io: &CliIo<'_>,
) -> Result<Orchestrator, String> {
    Ok(Orchestrator::new(command_deps(project, name, io)?))
}

/// What the orchestrator of a command handled in this process is made of. Its daemon holds the
/// connections kept on this computer, as a driving process's does, so that a command that calls a
/// connector itself (a Stop of a post Buffer has) reaches it.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub(crate) fn command_deps(
    project: &Project,
    name: &str,
    io: &CliIo<'_>,
) -> Result<OrchestratorDeps, String> {
    let tools = tool_deps(project, io)?;
    Ok(OrchestratorDeps {
        daemon: connected_daemon(&tools, io),
        tools,
        adapter: Arc::new(NoSessions {
            command: name.to_string(),
        }),
        sandboxes: Arc::new(HostSandboxFactory),
        previews: Arc::new(NoPreviews),
        session_ids: Arc::clone(&io.session_ids),
        forge: Arc::new(forge(&project.root, io)),
        sleeper: sleeper(io),
        work: Arc::default(),
        slots: None,
    })
}

/// A daemon over `tools` that keeps connector keys where `io` says, and runs each stdio
/// connector in a folder of the user's state folder (ADR 0030).
pub(crate) fn connected_daemon(
    tools: &Arc<catervas_runtime::tools::ToolDeps>,
    io: &CliIo<'_>,
) -> Arc<DaemonState> {
    let daemon = Arc::new(DaemonState::new(Arc::clone(tools)));
    daemon.set_connector_secrets(Arc::clone(&io.connector_secrets));
    if let Some(directory) = state_dir(&io.env) {
        daemon.set_state_dir(directory);
    }
    daemon.set_home(home_of(io));
    if let Some(program) = &io.own_program {
        daemon.set_own_program(program.clone());
    }
    daemon
}

/// What a driving process waits on: the test's sleeper, else the machine's timer over the clock.
fn sleeper(io: &CliIo<'_>) -> Arc<dyn Sleeper> {
    io.sleeper.clone().unwrap_or_else(|| {
        Arc::new(TokioSleeper {
            clock: Arc::clone(&io.clock),
        })
    })
}

/// The forge: the `gh` on the environment's `PATH`, else `gh`.
pub(crate) fn forge(root: &Path, io: &CliIo<'_>) -> Forge {
    Forge {
        program: on_path("gh", io).unwrap_or_else(|| PathBuf::from("gh")),
        root: root.to_path_buf(),
    }
}

/// The first executable called `program` on the environment's `PATH`.
pub(crate) fn on_path(program: &str, io: &CliIo<'_>) -> Option<PathBuf> {
    catervas_runtime::computer::on_path(program, &io.env)
}

/// The adapter of a command handled in this process, which starts no session: answering a
/// question or approving a contract needs neither a credential nor `claude`.
struct NoSessions {
    command: String,
}

impl NoSessions {
    fn refusal(&self) -> RuntimeError {
        RuntimeError::Spawn {
            detail: format!("catervas {} starts no sessions", self.command),
        }
    }
}

impl RuntimeAdapter for NoSessions {
    fn start_session(&self, _spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        Err(self.refusal())
    }

    fn resume(
        &self,
        _session_id: &str,
        _prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        Err(self.refusal())
    }
}

/// Why a second process cannot drive the project at `root`.
pub(crate) fn driven_elsewhere(root: &Path) -> String {
    match read_daemon_file(&root.join(DAEMON_FILE)) {
        Ok(DaemonAddress { pid: Some(pid), .. }) => {
            format!("another catervas process is driving this project (pid {pid} in {DAEMON_FILE})")
        }
        _ => "another catervas process is driving this project, and it serves no daemon yet"
            .to_string(),
    }
}

/// What every start in no-sandbox mode says on standard error (`docs/SPEC.md` 8.3).
pub(crate) const NO_SANDBOX_WARNING: &str = "warning: no-sandbox mode (.catervas/local/settings.json \
    says sandbox: none). Agents' commands run on this machine as you, with your HOME: they can \
    read your credential files (~/.git-credentials, ~/.ssh, ~/.claude/.credentials.json, the gh \
    configuration), push with a git hidden in a script, which catervas_exec's check does not see, and \
    read .catervas/local/daemon.json, whose token lets them act as you through catervas: approve, \
    accept, answer, add skills, mark orders placed and received, and integrate, and get the keys \
    you gave a connector, which they can also read from a running connector's /proc/<pid>/environ. \
    The governor still checks every path and permission it is asked about.";

/// The variables of the environment a Claude Code session is given, each copied only when set:
/// Claude Code finds its own sign-in (`CLAUDE_CONFIG_DIR` says where), and a key the user set
/// passes through unread (ADR 0084).
const SESSION_ENV: [&str; 9] = [
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "TERM",
    "TMPDIR",
    "CLAUDE_CONFIG_DIR",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

/// What a start calls with the daemon's state once it listens.
pub(crate) type Listening<'l> = &'l dyn Fn(&Arc<DaemonState>);

/// What a start is told besides the project: how the process differs from `run`'s.
#[derive(Clone, Copy, Default)]
pub(crate) struct StartOptions<'l> {
    /// What `catervas serve` shares among the teams it drives (ADR 0059). With it the daemon
    /// answers the browser routes, over the hub's sessions and port; without it, as `run`'s,
    /// it answers none.
    pub(crate) shared: Option<&'l Shared>,
    /// Called with the daemon's state as soon as it listens and has its browser routes, so that
    /// the hub routes the team's pages to it from then on.
    pub(crate) listening: Option<Listening<'l>>,
}

/// What every team of one `catervas serve` shares (ADR 0059): the hub's port and the browser
/// sessions and connect codes it issues, so that a browser connects once for all the teams.
#[derive(Clone)]
pub(crate) struct Shared {
    /// The browsers that traded a code.
    pub(crate) sessions: BrowserSessions,
    /// The live connect code.
    pub(crate) codes: ConnectCodes,
    /// The hub's port, which every page's `Origin` and `Host` name.
    pub(crate) port: u16,
    /// The places for sessions, the AI account's sleep and the computer's day, which every team's
    /// sessions share.
    pub(crate) slots: Arc<Slots>,
}

/// The AI account a start found Claude Code signed in with.
#[derive(Clone, Copy)]
pub(crate) struct AccountSeen {
    /// The kind of sign-in.
    pub(crate) kind: AccountKind,
    /// The variable it comes from, when it comes from the user's environment.
    pub(crate) variable: Option<&'static str>,
}

/// What a start is refused with while Claude Code is not signed in.
pub(crate) const NOT_SIGNED_IN: &str = "Claude Code is not signed in: sign in on the AI account page of the web app, or run claude auth login, or set ANTHROPIC_API_KEY where you start Catervas";

/// A process driving the project: its lock, its served daemon, its orchestrator, and where it
/// hears Ctrl-C.
pub(crate) struct Driver {
    /// The orchestrator driving the project.
    pub(crate) orchestrator: Arc<Orchestrator>,
    /// Its daemon's state.
    pub(crate) daemon: Arc<DaemonState>,
    /// One `()` per interrupt.
    pub(crate) interrupts: UnboundedReceiver<()>,
    /// The Claude Code account it found signed in, when the engine is Claude Code.
    pub(crate) account: Option<AccountSeen>,
    /// Whether it runs in no-sandbox mode.
    pub(crate) sandbox: Sandbox,
    /// What recovery found and did.
    pub(crate) recovered: RecoveryReport,
    handle: DaemonHandle,
    /// The watch on the marketing plans' Google Ads spend, which runs beside the ticks.
    watch: tokio::task::JoinHandle<Result<(), OrchestratorError>>,
    _lock: RunLock,
}

/// What the end of the spend watch says of the run, when it ended on its own with an error (a
/// store error, which stopped the orchestrator): the sentence to report, so that `catervas serve`
/// ends saying why. `None` when it was still watching, or ended without a fault.
fn watch_failure(
    ended: Option<Result<Result<(), OrchestratorError>, tokio::task::JoinError>>,
) -> Option<String> {
    match ended? {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("the ad spend watch stopped the run: {error}")),
        Err(error) => Some(format!("the ad spend watch failed: {error}")),
    }
}

impl Driver {
    /// Shuts the daemon down, removing `daemon.json`, then gives the lock back.
    ///
    /// # Errors
    ///
    /// A sentence saying the daemon could not be shut down.
    pub(crate) async fn finish(self) -> Result<(), String> {
        let Driver {
            handle,
            watch,
            _lock,
            ..
        } = self;
        let ended = if watch.is_finished() {
            Some(watch.await)
        } else {
            watch.abort();
            None
        };
        let shut = handle.shutdown().await.map_err(|error| error.to_string());
        match watch_failure(ended) {
            Some(failure) => Err(failure),
            None => shut,
        }
    }
}

/// Starts a process driving the project, in order: the lock, the interrupt listener, the
/// settings (warning on standard error in no-sandbox mode), the credential and `claude` for the
/// Claude Code engine, the prices (warning on standard error of each model no price table prices,
/// ADR 0015), the daemon, the adapter, the orchestrator and its command handler, and recovery
/// (5.15). A step that fails refuses with the lock given back and nothing left running.
///
/// # Errors
///
/// The sentence of the step that failed.
pub(crate) async fn start(
    project: &Project,
    io: &mut CliIo<'_>,
    options: StartOptions<'_>,
) -> Result<Driver, String> {
    let Some(lock) = try_lock(&project.root)? else {
        return Err(driven_elsewhere(&project.root));
    };
    start_holding(project, io, lock, options).await
}

/// `start`, with the run lock already taken by the caller, which it gives back on a refusal. The
/// interrupts are taken from `io` and put back there on a refusal, so that `serve`, which goes
/// back to setup mode then, still hears Ctrl-C.
///
/// # Errors
///
/// The sentence of the step that failed.
pub(crate) async fn start_holding(
    project: &Project,
    io: &mut CliIo<'_>,
    lock: RunLock,
    options: StartOptions<'_>,
) -> Result<Driver, String> {
    let interrupts = listen(std::mem::replace(&mut io.interrupts, never()))?;
    match start_listening(project, io, lock, options).await {
        Ok(driver) => Ok(Driver {
            interrupts,
            ..driver
        }),
        Err(error) => {
            io.interrupts = Interrupts::Channel(interrupts);
            Err(error)
        }
    }
}

/// Interrupts that never come.
fn never() -> Interrupts {
    Interrupts::Channel(tokio::sync::mpsc::unbounded_channel().1)
}

/// `start_holding` once the interrupts are listened for: the driver it answers hears none yet.
#[allow(
    clippy::too_many_lines,
    reason = "one setup in order, each step failing by shutting the daemon down"
)]
async fn start_listening(
    project: &Project,
    io: &mut CliIo<'_>,
    lock: RunLock,
    options: StartOptions<'_>,
) -> Result<Driver, String> {
    let settings = project
        .files
        .read_settings()
        .map_err(|error| error.to_string())?;
    if settings.sandbox == Sandbox::None {
        let _ = writeln!(io.stderr, "{NO_SANDBOX_WARNING}");
    }
    let (claude, account) = match &io.engine {
        Engine::Claude => {
            let path = on_path("claude", io)
                .ok_or("Claude Code is not installed: there is no claude on PATH")?;
            let status = crate::start::account(io).status().await;
            if !status.signed_in {
                return Err(NOT_SIGNED_IN.to_string());
            }
            let seen = status.kind.map(|kind| AccountSeen {
                kind,
                variable: credential_variable(&session_env(io)),
            });
            (Some(path), seen)
        }
        Engine::Given(_) => (None, None),
    };
    // Every session reads the prices, so a table that cannot be read is refused here, once.
    for sentence in unpriced(project)? {
        let _ = writeln!(
            io.stderr,
            "warning: {}.",
            crate::printable::printable(&sentence)
        );
    }
    let tools = tool_deps(project, io)?;
    // Before the daemon listens: a tab that reconnects asks `team.get` the moment it does, and a
    // task in `verifying` or `team.propose` asks whether the Designer can have its browser.
    tools.transitions.set_sandbox(settings.sandbox);
    let (sandboxes, previews) = told_factories(&tools, settings.sandbox, sandbox_image(io)).await?;
    let daemon = connected_daemon(&tools, io);
    let web = options
        .shared
        .map(|shared| web_sharing(&project.root, io, shared));
    let config = DaemonConfig {
        port: PortChoice::Any,
        daemon_file: Some(project.root.join(DAEMON_FILE)),
    };
    let handle = serve(config, Arc::clone(&daemon))
        .await
        .map_err(|error| error.to_string())?;
    // The routes read the state on each request, and the hub's port is what a page names.
    if let Some(web) = web {
        daemon.set_web(web);
    }
    if let Some(listening) = options.listening {
        listening(&daemon);
    }
    let adapter = match adapter(project, io, claude, &daemon, &handle) {
        Ok(adapter) => adapter,
        Err(error) => {
            let _ = handle.shutdown().await;
            return Err(error);
        }
    };
    let orchestrator = Arc::new(Orchestrator::new(OrchestratorDeps {
        tools,
        daemon: Arc::clone(&daemon),
        adapter,
        sandboxes,
        previews,
        session_ids: Arc::clone(&io.session_ids),
        forge: Arc::new(forge(&project.root, io)),
        sleeper: sleeper(io),
        work: Arc::default(),
        slots: options.shared.map(|shared| Arc::clone(&shared.slots)),
    }));
    daemon.set_command_handler(command_handler(Arc::clone(&orchestrator)));
    daemon.set_sandbox_recreator(sandbox_recreator(Arc::clone(&orchestrator)));
    let recovering = Arc::clone(&orchestrator);
    let recovered = match tokio::task::spawn_blocking(move || recovering.recover()).await {
        Ok(Ok(recovered)) => recovered,
        Ok(Err(error)) => {
            let _ = handle.shutdown().await;
            return Err(error.to_string());
        }
        Err(error) => {
            let _ = handle.shutdown().await;
            return Err(format!("recovery failed: {error}"));
        }
    };
    Ok(Driver {
        watch: watching(&orchestrator),
        orchestrator,
        daemon,
        interrupts: tokio::sync::mpsc::unbounded_channel().1,
        account,
        sandbox: settings.sandbox,
        recovered,
        handle,
        _lock: lock,
    })
}

/// The watch on the marketing plans' Google Ads spend, started once recovery is done and running
/// beside the ticks: a tick waits for a running session to end, and the spend is read every
/// fifteen minutes whatever runs (spec 6.7).
fn watching(
    orchestrator: &Arc<Orchestrator>,
) -> tokio::task::JoinHandle<Result<(), OrchestratorError>> {
    let (stopping, watching) = (Arc::clone(orchestrator), Arc::clone(orchestrator));
    supervised(move || stopping.stop(), async move {
        watching.watch_marketing_spend().await
    })
}

/// A task that runs `watch` and, if `watch` panics, calls `stop` and panics the same way, so that
/// the run ends and `Driver::finish` says why, as it does for the store error that stops the
/// watch itself (spec 6.5). Aborting the task aborts `watch` with it.
fn supervised(
    stop: impl FnOnce() + Send + 'static,
    watch: impl std::future::Future<Output = Result<(), OrchestratorError>> + Send + 'static,
) -> tokio::task::JoinHandle<Result<(), OrchestratorError>> {
    /// A task that is aborted when its handle is dropped.
    struct Aborting(tokio::task::JoinHandle<Result<(), OrchestratorError>>);
    impl Drop for Aborting {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    tokio::spawn(async move {
        let mut inner = Aborting(tokio::spawn(watch));
        match (&mut inner.0).await {
            Ok(ended) => ended,
            Err(error) if error.is_panic() => {
                stop();
                std::panic::resume_unwind(error.into_panic())
            }
            // Cancelled: nothing else holds the handle, so it was aborted from outside.
            Err(_) => Ok(()),
        }
    })
}

/// `factories`, the preview's told to the governor's door once its first answer of whether a
/// preview can run is in, which for Docker takes at most `docker info`'s 10 seconds. It is waited
/// for here, before the daemon listens, and nowhere else: from then on the answer is what Docker
/// said, and no request waits for it.
async fn told_factories(
    tools: &catervas_runtime::tools::ToolDeps,
    sandbox: Sandbox,
    image: &str,
) -> Result<(Arc<dyn SandboxFactory>, Arc<dyn PreviewFactory>), String> {
    let (sandboxes, previews) = factories(sandbox, image);
    tools.transitions.set_previews(Arc::clone(&previews));
    let settling = Arc::clone(&previews);
    tokio::task::spawn_blocking(move || settling.settle())
        .await
        .map_err(|error| format!("Docker could not be asked: {error}"))?;
    Ok((sandboxes, previews))
}

/// The image Docker's sandbox and the preview run in: `SANDBOX_IMAGE`, or the end-to-end
/// server's `--sandbox-image`.
fn sandbox_image<'a>(io: &'a CliIo<'_>) -> &'a str {
    #[cfg(feature = "e2e")]
    if let Some(image) = io.sandbox_image.as_deref() {
        return image;
    }
    let _ = io;
    SANDBOX_IMAGE
}

/// What makes a task's sandbox and its preview, by the project's sandbox setting, in `image`:
/// the Designer has no browser without Docker's sandbox (D3). Whether Docker is there is asked
/// off every request's path, from the moment the factory is made.
fn factories(sandbox: Sandbox, image: &str) -> (Arc<dyn SandboxFactory>, Arc<dyn PreviewFactory>) {
    match sandbox {
        Sandbox::Docker => (
            Arc::new(DockerSandboxFactory {
                image: image.to_string(),
            }),
            Arc::new(PolledPreviews::new(
                Arc::new(DockerPreviewFactory::new(
                    image.to_string(),
                    catervas_runtime::computer::browser_image(),
                )),
                AVAILABLE_FOR,
            )),
        ),
        Sandbox::None => (Arc::new(HostSandboxFactory), Arc::new(NoPreviews)),
    }
}

/// The sessions and codes a serve shares among its teams, behind the hub's `port`. The sessions
/// are kept in the state folder, made here when it is not there yet, or in memory when there is
/// none.
pub(crate) fn shared_web(io: &CliIo<'_>, port: u16, slots: Arc<Slots>) -> Result<Shared, String> {
    let file = match state_dir(&io.env) {
        Some(directory) => {
            make_state_dir(&directory)?;
            Some(directory.join("browser-sessions.json"))
        }
        None => None,
    };
    Ok(Shared {
        sessions: BrowserSessions::open(file).map_err(|error| error.to_string())?,
        codes: ConnectCodes::default(),
        port,
        slots,
    })
}

/// `web`'s state for the project at `root`, over `shared`'s sessions, codes and port.
pub(crate) fn web_sharing(root: &Path, io: &CliIo<'_>, shared: &Shared) -> WebState {
    let state = state_dir(&io.env);
    WebState {
        codes: shared.codes.clone(),
        sessions: shared.sessions.clone(),
        project_root: root.to_path_buf(),
        account: account(io),
        kept_key_removed: io.kept_key_removed,
        port: shared.port,
        clock: Arc::clone(&io.clock),
        take_on_error: std::sync::Mutex::default(),
        leaving: std::sync::Mutex::default(),
        env: io.env.clone(),
        templates: state.map(|directory| Templates::new(directory.join("templates"))),
        #[cfg(feature = "e2e")]
        admit_local_preview: io.admit_local_preview,
    }
}

/// What the browser routes ask of Claude Code's account (ADR 0084): the `claude` found on `PATH`,
/// run with the environment a session gets, and the check run in an empty folder of its own in
/// the state folder.
pub(crate) fn account(io: &CliIo<'_>) -> Arc<dyn AccountPort> {
    let check_dir = state_dir(&io.env).map_or_else(
        || std::env::temp_dir().join("catervas-account-check"),
        |directory| directory.join("account-check"),
    );
    Arc::new(ClaudeAccount::new(
        on_path("claude", io).unwrap_or_else(|| PathBuf::from("claude")),
        session_env(io),
        check_dir,
    ))
}

/// The environment a `claude` program is run in: `SESSION_ENV`'s variables, each only when set.
fn session_env(io: &CliIo<'_>) -> BTreeMap<String, String> {
    session_env_of(&io.env)
}

/// `env` cut down to `SESSION_ENV`'s variables.
pub(crate) fn session_env_of(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    SESSION_ENV
        .iter()
        .filter_map(|name| {
            env.get(*name)
                .map(|value| ((*name).to_string(), value.clone()))
        })
        .collect()
}

/// The adapter sessions start through: the engine's factory's, or Claude Code's once its version
/// passes.
fn adapter(
    project: &Project,
    io: &CliIo<'_>,
    claude: Option<PathBuf>,
    daemon: &Arc<DaemonState>,
    handle: &DaemonHandle,
) -> Result<Arc<dyn RuntimeAdapter>, String> {
    match (&io.engine, claude) {
        (Engine::Given(factory), _) => Ok(factory(Arc::clone(daemon))),
        (Engine::Claude, Some(claude_path)) => {
            let config = ClaudeConfig {
                claude_path,
                hook_command: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("catervas")),
                daemon_file: project.root.join(DAEMON_FILE),
                daemon: handle.info.clone(),
                sessions_dir: project.root.join(".catervas/local/sessions"),
                // No state folder, or no id for the project: the orchestrator offers no skills
                // either, so nothing is written here.
                skills_dir: state_dir(&io.env)
                    .and_then(|state| {
                        let id =
                            catervas_runtime::connectors::local_project_id(&state, &project.root)
                                .ok()?;
                        Some(catervas_runtime::skills::skills_dir(&state, &id))
                    })
                    .unwrap_or_default(),
                team_file: project.root.join(".catervas/team.yaml"),
                env: session_env(io),
            };
            let adapter = ClaudeAdapter::new(config).map_err(|error| error.to_string())?;
            Ok(Arc::new(adapter))
        }
        (Engine::Claude, None) => {
            Err("the Claude Code engine was chosen with no credential".to_string())
        }
    }
}

/// Where the process hears the person's interrupts: Ctrl-C, listened for from now on, or the
/// test's channel.
pub(crate) fn listen(interrupts: Interrupts) -> Result<UnboundedReceiver<()>, String> {
    match interrupts {
        Interrupts::Channel(receiver) => Ok(receiver),
        Interrupts::CtrlC => {
            let mut signal = signal(SignalKind::interrupt())
                .map_err(|error| format!("Ctrl-C cannot be listened for: {error}"))?;
            let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                while signal.recv().await.is_some() {
                    if sender.send(()).is_err() {
                        break;
                    }
                }
            });
            Ok(receiver)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use catervas_protocol::clock::FixedClock;
    use catervas_runtime::connectors::{ConnectorSecrets, MemoryConnectorSecrets};
    use catervas_store::git::fixtures::TempRepo;

    use catervas_runtime::orchestrator::OrchestratorError;

    use super::{command_deps, supervised, watch_failure};
    use crate::{CliIo, open_project, run_cli};

    #[tokio::test]
    async fn says_why_the_ad_spend_watch_ended_the_run() {
        // Still watching, or ended without a fault: nothing to say.
        assert_eq!(watch_failure(None), None);
        assert_eq!(watch_failure(Some(Ok(Ok(())))), None);
        // A fault it ended on, which stopped the orchestrator, is the run's end.
        let refused = OrchestratorError::Refused {
            reason: "marketing_event_not_recorded: no".to_string(),
        };
        assert_eq!(
            watch_failure(Some(Ok(Err(refused)))),
            Some(
                "the ad spend watch stopped the run: refused: marketing_event_not_recorded: no"
                    .to_string()
            )
        );
        // And so is a panic in it.
        let panicked = tokio::spawn(async { panic!("the watch broke") })
            .await
            .map(|()| Ok(()));
        let said = watch_failure(Some(panicked)).expect("a panic is a fault");
        assert!(said.starts_with("the ad spend watch failed: "), "{said}");
    }

    /// A flag and the `stop` that raises it.
    fn stopping() -> (Arc<AtomicBool>, impl FnOnce() + Send + 'static) {
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopped);
        (stopped, move || flag.store(true, Ordering::SeqCst))
    }

    #[tokio::test]
    async fn a_panic_in_the_ad_spend_watch_stops_the_run_and_is_said() {
        let (stopped, stop) = stopping();

        let handle = supervised(stop, async { panic!("the watch broke") });
        let ended = handle.await;

        // The run is told to stop, and the end of the task still says why when it is finished.
        assert!(stopped.load(Ordering::SeqCst), "the run was not stopped");
        assert!(ended.as_ref().is_err_and(tokio::task::JoinError::is_panic));
        let said = watch_failure(Some(ended)).expect("a panic is a fault");
        assert!(said.starts_with("the ad spend watch failed: "), "{said}");
    }

    #[tokio::test]
    async fn an_ad_spend_watch_that_ends_on_its_own_stops_nothing_more() {
        // Its own store error stops the orchestrator already (`watch_marketing_spend`), and an end
        // without a fault is a stop that was asked for: neither calls `stop` again.
        for ended in [
            Ok(()),
            Err(OrchestratorError::Refused {
                reason: "marketing_event_not_recorded: no".to_string(),
            }),
        ] {
            let (stopped, stop) = stopping();
            let expected = ended.clone();
            let handle = supervised(stop, async move { ended });
            assert_eq!(handle.await.expect("joined"), expected);
            assert!(!stopped.load(Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn aborting_the_supervisor_aborts_the_watch() {
        struct Flag(Arc<AtomicBool>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let held = Flag(Arc::clone(&dropped));
        let (stopped, stop) = stopping();
        let handle = supervised(stop, async move {
            let _held = held;
            std::future::pending::<Result<(), OrchestratorError>>().await
        });
        tokio::task::yield_now().await;

        handle.abort();
        let _ = handle.await;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the watch was dropped with its supervisor");
        assert!(!stopped.load(Ordering::SeqCst), "an abort is not a fault");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_command_handled_here_has_the_kept_connections() {
        let repository = TempRepo::new("start-command-deps");
        let at = chrono::Utc::now();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut io = CliIo::new(
            repository.path.clone(),
            Box::new(&mut out),
            Box::new(&mut err),
            Arc::new(FixedClock::new(at)),
        );
        let init = ["catervas", "init"].map(String::from);
        assert_eq!(run_cli(&init, &mut io), 0);
        let project = open_project(&repository.path, at).expect("the project opens");

        let deps = command_deps(&project, "marketing post stop", &io).expect("the deps are made");

        // The daemon's store of keys is the one `io` names, as a driving process's is, so that a
        // Stop of a post reaches Buffer with the connection kept on this computer.
        let other: Arc<dyn ConnectorSecrets> = Arc::new(MemoryConnectorSecrets::default());
        assert!(
            !deps.daemon.set_connector_secrets(other),
            "a store was set already"
        );
    }
}
