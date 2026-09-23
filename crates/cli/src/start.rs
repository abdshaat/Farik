//! Who drives a project, and how a command reaches it (ADR 0014): one process holds the run lock
//! for as long as it drives, and a command typed anywhere else is sent to that process's daemon,
//! or handled here, under the lock, when nothing drives.

use std::fs::{File, TryLockError};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use farik_protocol::command::{Command, command_to_value, reply_from_value};
use farik_runtime::claude::{ClaudeAdapter, ClaudeConfig, ClaudeCredential, credential_from_env};
use farik_runtime::daemon::{DaemonConfig, DaemonHandle, DaemonState, serve};
use farik_runtime::forge::Forge;
use farik_runtime::orchestrator::{
    CommandError, CommandReport, Orchestrator, OrchestratorDeps, RecoveryReport, command_handler,
    result_of,
};
use farik_runtime::transitions::Transitions;
use farik_runtime::{
    DockerSandboxFactory, HostSandboxFactory, RuntimeAdapter, RuntimeError, SANDBOX_IMAGE,
    SandboxFactory, SessionHandle, SessionSpec, ToolDeps,
};
use farik_store::files::{ProjectFiles, Sandbox};
use farik_store::{Git, open_projections};
use serde_json::Value;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::daemon_client::{ClientError, DaemonAddress, exchange, read_daemon_file};
use crate::doctor::unpriced;
use crate::project::Project;
use crate::{CliIo, Engine, Interrupts};

/// The lock the process driving a project holds, under the gitignored `.farik/local/`.
pub(crate) const RUN_LOCK: &str = ".farik/local/run.lock";
/// Where the driving process's daemon says how to reach it.
pub(crate) const DAEMON_FILE: &str = ".farik/local/daemon.json";
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
            let handled = handle_here(project, command, name, io);
            drop(lock);
            handled
        }
        None => send(&project.root, &command),
    }
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
            "{detail}; the command may still take effect: farik log shows whether it did"
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
                detail: "another farik process holds this project's run lock and serves no \
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
    let tools = tool_deps(project, io)?;
    Ok(Orchestrator::new(OrchestratorDeps {
        daemon: Arc::new(DaemonState::new(Arc::clone(&tools))),
        tools,
        adapter: Arc::new(NoSessions {
            command: name.to_string(),
        }),
        sandboxes: Arc::new(HostSandboxFactory),
        session_ids: Arc::clone(&io.session_ids),
        forge: Arc::new(forge(&project.root, io)),
    }))
}

/// The project's tools, over this process's own board of the project's log.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub(crate) fn tool_deps(project: &Project, io: &CliIo<'_>) -> Result<Arc<ToolDeps>, String> {
    let projections =
        Arc::new(open_projections(Arc::clone(&project.log)).map_err(|error| error.to_string())?);
    let files = Arc::new(ProjectFiles::open(project.root.clone()));
    let ids = crate::contract::event_ids(project);
    let transitions = Arc::new(Transitions::new(
        Arc::clone(&project.log),
        Arc::clone(&projections),
        Arc::clone(&files),
        Git::open(project.root.clone()),
        Arc::clone(&io.clock),
        ids.clone(),
    ));
    Ok(Arc::new(ToolDeps {
        log: Arc::clone(&project.log),
        projections,
        files,
        transitions,
        git: Git::open(project.root.clone()),
        clock: Arc::clone(&io.clock),
        ids,
    }))
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
    use std::os::unix::fs::PermissionsExt;

    io.env.get("PATH").and_then(|path| {
        std::env::split_paths(path)
            .map(|directory| directory.join(program))
            .find(|candidate| {
                std::fs::metadata(candidate)
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
    })
}

/// The adapter of a command handled in this process, which starts no session: answering a
/// question or approving a contract needs neither a credential nor `claude`.
struct NoSessions {
    command: String,
}

impl NoSessions {
    fn refusal(&self) -> RuntimeError {
        RuntimeError::Spawn {
            detail: format!("farik {} starts no sessions", self.command),
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
            format!("another farik process is driving this project (pid {pid} in {DAEMON_FILE})")
        }
        _ => {
            "another farik process is driving this project, and it serves no daemon yet".to_string()
        }
    }
}

/// What every start in no-sandbox mode says on standard error (`docs/SPEC.md` 8.3).
pub(crate) const NO_SANDBOX_WARNING: &str = "warning: no-sandbox mode (.farik/local/settings.json \
    says sandbox: none). Agents' commands run on this machine as you, with your HOME: they can \
    read your credential files (~/.git-credentials, ~/.ssh, ~/.claude/.credentials.json, the gh \
    configuration) and push with a git hidden in a script, which farik_exec's check does not see. \
    The governor still checks every path and permission it is asked about.";

/// The variables of the environment a Claude Code session is given besides its credential.
const SESSION_ENV: [&str; 6] = ["PATH", "HOME", "USER", "LANG", "TERM", "TMPDIR"];

/// A process driving the project: its lock, its served daemon, its orchestrator, and where it
/// hears Ctrl-C.
pub(crate) struct Driver {
    /// The orchestrator driving the project.
    pub(crate) orchestrator: Arc<Orchestrator>,
    /// Its daemon's state.
    pub(crate) daemon: Arc<DaemonState>,
    /// One `()` per interrupt.
    pub(crate) interrupts: UnboundedReceiver<()>,
    /// The credential variable it chose, when the engine is Claude Code.
    pub(crate) credential: Option<&'static str>,
    /// Whether it runs in no-sandbox mode.
    pub(crate) sandbox: Sandbox,
    /// What recovery found and did.
    pub(crate) recovered: RecoveryReport,
    handle: DaemonHandle,
    _lock: RunLock,
}

impl Driver {
    /// Shuts the daemon down, removing `daemon.json`, then gives the lock back.
    ///
    /// # Errors
    ///
    /// A sentence saying the daemon could not be shut down.
    pub(crate) async fn finish(self) -> Result<(), String> {
        let Driver { handle, _lock, .. } = self;
        handle.shutdown().await.map_err(|error| error.to_string())
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
pub(crate) async fn start(project: &Project, io: &mut CliIo<'_>) -> Result<Driver, String> {
    let Some(lock) = try_lock(&project.root)? else {
        return Err(driven_elsewhere(&project.root));
    };
    let interrupts = listen(std::mem::replace(
        &mut io.interrupts,
        Interrupts::Channel(tokio::sync::mpsc::unbounded_channel().1),
    ))?;
    let settings = project
        .files
        .read_settings()
        .map_err(|error| error.to_string())?;
    if settings.sandbox == Sandbox::None {
        let _ = writeln!(io.stderr, "{NO_SANDBOX_WARNING}");
    }
    let claude = match &io.engine {
        Engine::Claude => {
            let credential = credential_from_env(&io.env).ok_or(
                "no credential for Claude Code: set ANTHROPIC_API_KEY to an API key, or \
                 CLAUDE_CODE_OAUTH_TOKEN to the token claude setup-token prints",
            )?;
            let path = on_path("claude", io)
                .ok_or("Claude Code is not installed: there is no claude on PATH")?;
            Some((credential, path))
        }
        Engine::Given(_) => None,
    };
    let credential = claude.as_ref().map(|(credential, _)| match credential {
        ClaudeCredential::ApiKey(_) => "ANTHROPIC_API_KEY",
        ClaudeCredential::OauthToken(_) => "CLAUDE_CODE_OAUTH_TOKEN",
    });
    // Every session reads the prices, so a table that cannot be read is refused here, once.
    for sentence in unpriced(project)? {
        let _ = writeln!(io.stderr, "warning: {sentence}.");
    }
    let tools = tool_deps(project, io)?;
    let daemon = Arc::new(DaemonState::new(Arc::clone(&tools)));
    let handle = serve(
        DaemonConfig {
            port: None,
            daemon_file: project.root.join(DAEMON_FILE),
        },
        Arc::clone(&daemon),
    )
    .await
    .map_err(|error| error.to_string())?;
    let adapter = match adapter(project, io, claude, &daemon, &handle) {
        Ok(adapter) => adapter,
        Err(error) => {
            let _ = handle.shutdown().await;
            return Err(error);
        }
    };
    let sandboxes: Arc<dyn SandboxFactory> = match settings.sandbox {
        Sandbox::Docker => Arc::new(DockerSandboxFactory {
            image: SANDBOX_IMAGE.to_string(),
        }),
        Sandbox::None => Arc::new(HostSandboxFactory),
    };
    let orchestrator = Arc::new(Orchestrator::new(OrchestratorDeps {
        tools,
        daemon: Arc::clone(&daemon),
        adapter,
        sandboxes,
        session_ids: Arc::clone(&io.session_ids),
        forge: Arc::new(forge(&project.root, io)),
    }));
    daemon.set_command_handler(command_handler(Arc::clone(&orchestrator)));
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
        orchestrator,
        daemon,
        interrupts,
        credential,
        sandbox: settings.sandbox,
        recovered,
        handle,
        _lock: lock,
    })
}

/// The adapter sessions start through: the engine's factory's, or Claude Code's once its version
/// passes.
fn adapter(
    project: &Project,
    io: &CliIo<'_>,
    claude: Option<(ClaudeCredential, PathBuf)>,
    daemon: &Arc<DaemonState>,
    handle: &DaemonHandle,
) -> Result<Arc<dyn RuntimeAdapter>, String> {
    match (&io.engine, claude) {
        (Engine::Given(factory), _) => Ok(factory(Arc::clone(daemon))),
        (Engine::Claude, Some((credential, claude_path))) => {
            let config = ClaudeConfig {
                claude_path,
                hook_command: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("farik")),
                daemon_file: project.root.join(DAEMON_FILE),
                daemon: handle.info.clone(),
                sessions_dir: project.root.join(".farik/local/sessions"),
                team_file: project.root.join(".farik/team.yaml"),
                env: SESSION_ENV
                    .iter()
                    .filter_map(|name| {
                        io.env
                            .get(*name)
                            .map(|value| ((*name).to_string(), value.clone()))
                    })
                    .collect(),
            };
            let adapter =
                ClaudeAdapter::new(credential, config).map_err(|error| error.to_string())?;
            Ok(Arc::new(adapter))
        }
        (Engine::Claude, None) => {
            Err("the Claude Code engine was chosen with no credential".to_string())
        }
    }
}

/// Where the process hears the person's interrupts: Ctrl-C, listened for from now on, or the
/// test's channel.
fn listen(interrupts: Interrupts) -> Result<UnboundedReceiver<()>, String> {
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
