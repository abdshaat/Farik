//! Who drives a project, and how a command reaches it (ADR 0014): one process holds the run lock
//! for as long as it drives, and a command typed anywhere else is sent to that process's daemon,
//! or handled here, under the lock, when nothing drives.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use farik_protocol::command::{Command, command_to_value, reply_from_value};
use farik_runtime::daemon::DaemonState;
use farik_runtime::forge::Forge;
use farik_runtime::orchestrator::{
    CommandError, CommandReport, Orchestrator, OrchestratorDeps, result_of,
};
use farik_runtime::transitions::Transitions;
use farik_runtime::{
    HostSandboxFactory, RuntimeAdapter, RuntimeError, SessionHandle, SessionSpec, ToolDeps,
};
use farik_store::files::ProjectFiles;
use farik_store::{Git, open_projections};
use serde_json::Value;

use crate::CliIo;
use crate::daemon_client::{ClientError, exchange};
use crate::project::Project;

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
