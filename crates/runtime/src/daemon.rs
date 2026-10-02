//! Farik's local service (`docs/SPEC.md` sections 8.2 and 8.6): the governor in front of every tool
//! call a session makes. Claude Code's `PreToolUse` hook asks it for allow or deny, its
//! `PostToolUse` hook reports what came back, and the sessions it knows are the only ones it
//! answers for.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::io::Read as _;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use farik_core::budget::SessionLimits;
use farik_core::contract::TaskId;
use farik_core::governor::permissions::{PermissionTier, SessionConnector};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use farik_protocol::command::{
    Command, CommandReply, ReplyKind, command_from_value, reply_to_value,
};
use farik_protocol::event::Thread;
use serde_json::Value;

use self::mcp::FarikMcp;

use crate::connectors::{ConnectorSecrets, MemoryConnectorSecrets, SecretAt};
use crate::exec::Executor;
use crate::orchestrator::{CommandError, CommandReport, reply_of};
use crate::preview::RunningPreview;
use crate::session::SessionPurpose;
use crate::tools::{ToolContext, ToolDeps};

mod app;
mod board;
#[cfg(test)]
pub(crate) mod fixtures;
mod gates;
mod hooks;
mod mcp;
mod setup;
mod team;
mod templates;
pub mod web;

#[cfg(test)]
pub(crate) use mcp::listed_names;

pub(crate) use hooks::APPROVAL_NEEDED;
pub use hooks::{
    HookDecision, HookRequest, builtin_tool_tier, decide_pre_tool_use, record_post_tool_use,
};
pub use setup::{SetupError, SetupHost};
pub use team::SETUP_PENDING;
pub use team::{custom_entry, labelled};
pub(crate) use team::{secret_at, with_server};

/// What a daemon with no project answers what needs one.
pub(crate) const NO_PROJECT: &str = "farik has no project yet";

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
    /// Why it runs, which decides what kind of message it posts.
    pub purpose: SessionPurpose,
    /// The seq of the message a conversation session answers, which its reply names.
    pub in_reply_to: Option<u64>,
    /// A ceremony's thread, which its posts are in.
    pub thread: Option<Thread>,
    /// Its working directory: the task's worktree. No tool call reaches outside it.
    pub cwd: PathBuf,
    /// Where the task's commands run, when it has somewhere.
    pub executor: Option<Arc<dyn Executor>>,
    /// Its limits, of which the daemon holds `max_tool_calls`.
    pub limits: SessionLimits,
    /// The Farik tools it was given (`SessionSpec::farik_tools`), by name without the
    /// `mcp__farik__` prefix: the hook denies every other Farik tool (`tool_not_in_session`), and
    /// the MCP server neither lists nor calls one.
    pub farik_tools: Vec<String>,
    /// The agent's tiers when the session started (spec 4.4): a grant or a revoke waits for the
    /// agent's next session, while a pause or a retirement stops this one at once.
    pub tiers: Vec<PermissionTier>,
    /// The connectors it was given (5.6): the hook refuses a connector's call unless it is one of
    /// these and passes `evaluate_connector_call`.
    pub connectors: Vec<SessionConnector>,
    /// The task's preview while the session runs, when it was given a connector.
    pub preview: Option<Arc<dyn RunningPreview>>,
    /// The names of the skills it loads on demand: the hook allows a `Skill` call for
    /// `farik:<name>` of one of these and no other (ADR 0034).
    pub skills: Vec<String>,
    /// The `skills/` folder of its plugin folder, when it has skills: `Read`, `Glob` and `Grep`
    /// there are allowed at tier `read`, whatever its worktree.
    pub skills_root: Option<PathBuf>,
}

/// A registration, the tool calls the hook has allowed it, and why it was told to stop, once it
/// was.
pub(crate) struct Session {
    pub(crate) registration: SessionRegistration,
    pub(crate) tool_calls: u32,
    pub(crate) stop_reason: Option<String>,
}

/// What `POST /command` hands a command the human gave to: the orchestrator driving the project in
/// this process (`crate::orchestrator::command_handler`).
pub type CommandHandler = Arc<
    dyn Fn(Command) -> Pin<Box<dyn Future<Output = Result<CommandReport, CommandError>> + Send>>
        + Send
        + Sync,
>;

/// What the daemon holds: the project's tools, the sessions it answers for, the notice a
/// session's loop waits on for a stop, and who takes the human's commands. In setup mode it has no
/// project's tools, and a host that answers the wizard in their place.
pub struct DaemonState {
    deps: Option<Arc<ToolDeps>>,
    host: Option<Arc<dyn SetupHost>>,
    sessions: Mutex<BTreeMap<String, Session>>,
    stops: tokio::sync::Notify,
    wakes: tokio::sync::Notify,
    commands: OnceLock<CommandHandler>,
    web: OnceLock<web::WebState>,
    /// Held by every write of `team.yaml` from its read to its write, so a change is checked
    /// against the team it replaces.
    team_writes: Mutex<()>,
    /// Where each agent's connector keys are kept (ADR 0030), once it is set.
    connector_secrets: OnceLock<Arc<dyn ConnectorSecrets>>,
    /// What that store held for each account the last time it was read, so that `team.get` does
    /// not ask a keychain each time.
    connectors_kept: Mutex<BTreeMap<String, Kept>>,
    /// The user's state folder, where each stdio connector runs (ADR 0030), once it is set.
    state_dir: OnceLock<std::path::PathBuf>,
}

/// What the connector store held for one agent's server the last time Farik read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kept {
    /// An entry, connected as the definition this is the `spec_sha256` of, with these keys.
    Entry {
        /// The hash of the definition connected.
        spec_sha256: String,
        /// The names of the keys kept.
        keys: std::collections::BTreeSet<String>,
        /// Where they are kept.
        stored_in: crate::connectors::SecretStore,
    },
    /// No entry.
    Nothing,
    /// The store could not be read.
    Unavailable,
}

impl Kept {
    /// Whether `server` runs from what was kept: connected as the team file has it now, with
    /// every key it names. Short of a key, its launcher or headers helper would be refused, and
    /// Claude Code connects an http server without its headers then (finding I2).
    pub(crate) fn runs(&self, server: &farik_core::team::CustomServer) -> bool {
        matches!(self, Kept::Entry { spec_sha256, keys, .. }
            if *spec_sha256 == farik_core::team::spec_sha256(server)
                && server.credential_keys.iter().all(|key| keys.contains(key)))
    }
}

impl DaemonState {
    /// A daemon for one project, answering for no session yet.
    #[must_use]
    pub fn new(deps: Arc<ToolDeps>) -> DaemonState {
        DaemonState {
            deps: Some(deps),
            host: None,
            sessions: Mutex::new(BTreeMap::new()),
            stops: tokio::sync::Notify::new(),
            wakes: tokio::sync::Notify::new(),
            commands: OnceLock::new(),
            web: OnceLock::new(),
            team_writes: Mutex::new(()),
            connector_secrets: OnceLock::new(),
            connectors_kept: Mutex::new(BTreeMap::new()),
            state_dir: OnceLock::new(),
        }
    }

    /// A daemon in setup mode: no project, the browser routes on with `web`, and the wizard's
    /// calls answered through `host`.
    #[must_use]
    pub fn setup(host: Arc<dyn SetupHost>, web: web::WebState) -> DaemonState {
        DaemonState {
            deps: None,
            host: Some(host),
            sessions: Mutex::new(BTreeMap::new()),
            stops: tokio::sync::Notify::new(),
            wakes: tokio::sync::Notify::new(),
            commands: OnceLock::new(),
            web: OnceLock::from(web),
            team_writes: Mutex::new(()),
            connector_secrets: OnceLock::new(),
            connectors_kept: Mutex::new(BTreeMap::new()),
            state_dir: OnceLock::new(),
        }
    }

    /// Keeps the agents' connector keys in `secrets` from now on. Answers `true`, or `false` when
    /// a store was already set, which is kept.
    pub fn set_connector_secrets(&self, secrets: Arc<dyn ConnectorSecrets>) -> bool {
        self.connector_secrets.set(secrets).is_ok()
    }

    /// Runs each stdio connector in a folder of `directory`, the user's state folder, from now on.
    /// Answers `true`, or `false` when one was already set, which is kept.
    pub fn set_state_dir(&self, directory: std::path::PathBuf) -> bool {
        self.state_dir.set(directory).is_ok()
    }

    /// The user's state folder, or why there is none.
    fn state_dir(&self) -> std::io::Result<&std::path::Path> {
        self.state_dir
            .get()
            .map(std::path::PathBuf::as_path)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "there is no Farik state folder on this computer",
                )
            })
    }

    /// The folder this project's sessions' plugin folders are written in ([`skills_dir`]).
    ///
    /// # Errors
    ///
    /// No state folder was set, or no project is open, so no session is given a skill; or the
    /// project's id could not be read or made.
    ///
    /// [`skills_dir`]: crate::skills::skills_dir
    pub(crate) fn skills_dir(&self) -> std::io::Result<std::path::PathBuf> {
        let deps = self.deps().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no project is open")
        })?;
        let state = self.state_dir()?;
        let id = crate::connectors::local_project_id(state, deps.files.root())?;
        Ok(crate::skills::skills_dir(state, &id))
    }

    /// Removes every plugin folder a session of this project left, which only a daemon that was
    /// killed leaves: no session of an earlier daemon is running now. A project with no state
    /// folder has none.
    fn wipe_skill_folders(&self) {
        if let Ok(folder) = self.skills_dir() {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// Where `agent`'s keys for `server` are kept in the project at `root` ([`SecretAt::of`]).
    ///
    /// # Errors
    ///
    /// No state folder was set, so no connector is confirmed, or the project's id could not be
    /// read or made.
    pub(crate) fn secret_at(
        &self,
        root: &std::path::Path,
        agent: &str,
        server: &str,
    ) -> std::io::Result<SecretAt> {
        SecretAt::of(self.state_dir()?, root, agent, server)
    }

    /// The folder the stdio connector `at` runs in, made again empty ([`working_folder`]).
    ///
    /// # Errors
    ///
    /// No state folder was set, or no project is open, so no connector runs; the state folder is
    /// inside the project; or the folder could not be made.
    ///
    /// [`working_folder`]: crate::connectors::working_folder
    pub(crate) fn connector_folder(&self, at: &SecretAt) -> std::io::Result<std::path::PathBuf> {
        let deps = self.deps().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no project is open")
        })?;
        crate::connectors::working_folder(self.state_dir()?, deps.files.root(), at)
    }

    /// Where the agents' connector keys are kept: until a store is set, an empty one, so that no
    /// custom connector is confirmed and none runs.
    pub(crate) fn connector_secrets(&self) -> Arc<dyn ConnectorSecrets> {
        self.connector_secrets.get().map_or_else(
            || Arc::new(MemoryConnectorSecrets::default()) as _,
            Arc::clone,
        )
    }

    /// What the store holds for `at`, read now and remembered for `team.get`: when a server is
    /// connected, and when a session is set up, so a store that fails then shows on the agent's
    /// page rather than leaving the server out unsaid.
    pub(crate) fn read_kept(&self, at: &SecretAt) -> Kept {
        let kept = match self.connector_secrets().locate(at) {
            Ok(Some((entry, stored_in))) => Kept::Entry {
                keys: entry.keys.into_keys().collect(),
                spec_sha256: entry.spec_sha256,
                stored_in,
            },
            Ok(None) => Kept::Nothing,
            Err(_) => Kept::Unavailable,
        };
        crate::locked(&self.connectors_kept).insert(at.account(), kept.clone());
        kept
    }

    /// What the store held for `at` the last time it was read, read now the first time.
    pub(crate) fn kept(&self, at: &SecretAt) -> Kept {
        let remembered = crate::locked(&self.connectors_kept)
            .get(&at.account())
            .cloned();
        remembered.unwrap_or_else(|| self.read_kept(at))
    }

    /// Forgets what the store held for `at`, whose server was taken away.
    pub(crate) fn forget_kept(&self, at: &SecretAt) {
        crate::locked(&self.connectors_kept).remove(&at.account());
    }

    /// The host answering the wizard, in setup mode.
    pub(crate) fn host(&self) -> Option<&Arc<dyn SetupHost>> {
        self.host.as_ref()
    }

    /// Hands the human's commands to `handler` from now on. It is set once the orchestrator
    /// exists, which is after the daemon is served, since the adapter the orchestrator holds needs
    /// the served port and token. Answers `true`, or `false` when a handler was already set, which
    /// is kept.
    pub fn set_command_handler(&self, handler: CommandHandler) -> bool {
        self.commands.set(handler).is_ok()
    }

    /// Turns the browser routes on with `web`: until then, and under every driver but
    /// `farik serve`, they answer 404. Answers `true`, or `false` when they were already on, and
    /// the state they have is kept.
    pub fn set_web(&self, web: web::WebState) -> bool {
        self.web.set(web).is_ok()
    }

    /// What the browser routes have, once they are on.
    pub(crate) fn web(&self) -> Option<&web::WebState> {
        self.web.get()
    }

    /// The ids of every registered session, in order.
    #[must_use]
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions().keys().cloned().collect()
    }

    /// Answers for `registration`'s session from now on, with no tool calls made. A session
    /// registered again starts its count again.
    pub fn register_session(&self, registration: SessionRegistration) {
        self.sessions().insert(
            registration.session_id.clone(),
            Session {
                registration,
                tool_calls: 0,
                stop_reason: None,
            },
        );
    }

    /// Tells a registered session to stop, for `reason`: every later hook of it is denied
    /// `session_stopped: <reason>`, and the loop reading it, woken now, aborts it (5.2, F1). The
    /// first reason given is the one kept. Answers whether the daemon answers for the session.
    pub fn request_stop(&self, session_id: &str, reason: &str) -> bool {
        let known = match self.sessions().get_mut(session_id) {
            Some(session) => {
                session
                    .stop_reason
                    .get_or_insert_with(|| reason.to_string());
                true
            }
            None => false,
        };
        if known {
            self.stops.notify_waiters();
        }
        known
    }

    /// Why a session was told to stop, or `None` when it was not, or is not registered.
    #[must_use]
    pub fn stop_reason(&self, session_id: &str) -> Option<String> {
        self.sessions()
            .get(session_id)
            .and_then(|session| session.stop_reason.clone())
    }

    /// The ids of the registered sessions of `agent_id`, in order, each with its purpose.
    #[must_use]
    pub fn sessions_of(&self, agent_id: &str) -> Vec<(String, SessionPurpose)> {
        self.sessions()
            .values()
            .filter(|session| session.registration.agent_id == agent_id)
            .map(|session| {
                (
                    session.registration.session_id.clone(),
                    session.registration.purpose,
                )
            })
            .collect()
    }

    /// The notice every `request_stop` wakes: a loop takes `notified()` from it before it reads
    /// `stop_reason`, so that no stop falls between the two.
    pub(crate) fn stops(&self) -> &tokio::sync::Notify {
        &self.stops
    }

    /// The notice a request filed in the browser wakes, so that the team's wait ends at once.
    pub(crate) fn wakes(&self) -> &tokio::sync::Notify {
        &self.wakes
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

    /// What a Farik tool called from a session is called with: the registration's agent, task,
    /// session, and executor as they stand now, and the project's tools; `None` for a session the
    /// daemon does not answer for. The MCP server and a replayed session both take this path.
    #[must_use]
    pub fn tool_context(&self, session_id: &str) -> Option<ToolContext> {
        let deps = self.deps.as_ref()?;
        self.sessions().get(session_id).map(|session| ToolContext {
            agent_id: session.registration.agent_id.clone(),
            task_id: session.registration.task_id.clone(),
            session_id: session.registration.session_id.clone(),
            purpose: session.registration.purpose,
            in_reply_to: session.registration.in_reply_to,
            thread: session.registration.thread,
            executor: session.registration.executor.clone(),
            tiers: session.registration.tiers.clone(),
            connectors: session.registration.connectors.clone(),
            preview: session.registration.preview.clone(),
            deps: Arc::clone(deps),
        })
    }

    /// The Farik tools a registered session was given, or `None` for one the daemon does not
    /// answer for.
    pub(crate) fn farik_tools(&self, session_id: &str) -> Option<Vec<String>> {
        self.sessions()
            .get(session_id)
            .map(|session| session.registration.farik_tools.clone())
    }

    /// The project's tools, or `None` in setup mode.
    pub(crate) fn deps(&self) -> Option<&Arc<ToolDeps>> {
        self.deps.as_ref()
    }

    /// The lock every write of the team file holds from its read to its write.
    pub(crate) fn team_writes(&self) -> MutexGuard<'_, ()> {
        crate::locked(&self.team_writes)
    }

    /// The sessions, locked. A panic while they were held leaves them as they were, which is
    /// still a map of registrations and counts.
    pub(crate) fn sessions(&self) -> MutexGuard<'_, BTreeMap<String, Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Which port the daemon asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortChoice {
    /// One the operating system picks.
    #[default]
    Any,
    /// This one, else the next nine in order, else `Any`.
    Preferred(u16),
    /// This one, or none.
    Exact(u16),
}

/// The ports to try, in order, for `choice`; `0` is the operating system's pick.
#[must_use]
pub fn candidates(choice: PortChoice) -> Vec<u16> {
    match choice {
        PortChoice::Any => vec![0],
        PortChoice::Preferred(port) => (port..=port.saturating_add(9)).chain([0]).collect(),
        PortChoice::Exact(port) => vec![port],
    }
}

/// Where the daemon listens, and where it says so.
pub struct DaemonConfig {
    /// The port to bind on `127.0.0.1`.
    pub port: PortChoice,
    /// Where `daemon.json` is written: `.farik/local/daemon.json`; `None` writes none, as the
    /// setup daemon, which no hook reaches, does not.
    pub daemon_file: Option<PathBuf>,
}

/// What `daemon.json` holds: how a hook reaches the daemon. Its `Debug` prints the token as
/// `[redacted]`, so a logged value does not hand it out.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DaemonInfo {
    /// The port on `127.0.0.1`.
    pub port: u16,
    /// The bearer token every request carries.
    pub token: String,
    /// The daemon's process.
    pub pid: u32,
}

impl fmt::Debug for DaemonInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DaemonInfo")
            .field("port", &self.port)
            .field("token", &"[redacted]")
            .field("pid", &self.pid)
            .finish()
    }
}

/// A daemon that is up.
pub struct DaemonHandle {
    /// How it is reached.
    pub info: DaemonInfo,
    daemon_file: Option<PathBuf>,
    cancel: CancellationToken,
    stop: oneshot::Sender<()>,
    server: JoinHandle<std::io::Result<()>>,
}

impl DaemonHandle {
    /// Stops the daemon, waits for the requests in flight, and removes `daemon.json`.
    ///
    /// # Errors
    ///
    /// `Io` when the server failed or the file cannot be removed.
    pub async fn shutdown(self) -> Result<(), DaemonError> {
        // The MCP sessions first: Claude Code holds an event stream open with keep-alives, and a
        // graceful shutdown would wait on it forever.
        self.cancel.cancel();
        // The server may have stopped already, in which case there is nobody to tell.
        let _ = self.stop.send(());
        let served = self.server.await.map_err(|error| DaemonError::Io {
            detail: format!("the server's task failed: {error}"),
        })?;
        let removed = match self
            .daemon_file
            .as_deref()
            .map(|file| (file, std::fs::remove_file(file)))
        {
            Some((file, Err(error))) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(DaemonError::Io {
                    detail: format!("{} cannot be removed: {error}", file.display()),
                })
            }
            _ => Ok(()),
        };
        served.map_err(|error| DaemonError::Io {
            detail: format!("the server failed: {error}"),
        })?;
        removed
    }
}

/// Starts the daemon on `127.0.0.1`, on the configured port (`PortChoice`) or one the operating system picks,
/// with a fresh token, and writes `daemon.json` (mode 0600) once it is listening. A file left by
/// a daemon that crashed is overwritten: only the one `farik run` that owns the project serves.
///
/// # Errors
///
/// `Bind` when the port cannot be bound; `Io` when the token or `daemon.json` cannot be made.
pub async fn serve(
    config: DaemonConfig,
    state: Arc<DaemonState>,
) -> Result<DaemonHandle, DaemonError> {
    serve_on(bind(config.port).await?, config.daemon_file, state).await
}

/// `serve` on a socket its caller already holds and keeps when the daemon stops. `farik serve`
/// binds its port once and serves each of its daemons on it in turn, so between two of them the
/// port is never free for another process to take; a browser connecting meanwhile waits in the
/// socket's queue for the next one.
///
/// # Errors
///
/// As `serve`'s; `Bind` when the socket cannot be shared.
pub async fn serve_held(
    listener: &std::net::TcpListener,
    daemon_file: Option<PathBuf>,
    state: Arc<DaemonState>,
) -> Result<DaemonHandle, DaemonError> {
    let failed = |error: std::io::Error| DaemonError::Bind {
        detail: error.to_string(),
    };
    let shared = listener.try_clone().map_err(failed)?;
    shared.set_nonblocking(true).map_err(failed)?;
    serve_on(
        TcpListener::from_std(shared).map_err(failed)?,
        daemon_file,
        state,
    )
    .await
}

/// `serve` on `listener`.
async fn serve_on(
    listener: TcpListener,
    daemon_file: Option<PathBuf>,
    state: Arc<DaemonState>,
) -> Result<DaemonHandle, DaemonError> {
    state.wipe_skill_folders();
    let token = random_token()?;
    let port = listener
        .local_addr()
        .map_err(|error| DaemonError::Bind {
            detail: error.to_string(),
        })?
        .port();
    let info = DaemonInfo {
        port,
        token,
        pid: std::process::id(),
    };
    let cancel = CancellationToken::new();
    let app = router(state, &info.token, cancel.clone());
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await
    });
    let handle = DaemonHandle {
        info,
        daemon_file,
        cancel,
        stop,
        server,
    };
    let written = handle
        .daemon_file
        .as_deref()
        .map_or(Ok(()), |file| write_daemon_file(file, &handle.info));
    if let Err(error) = written {
        let _ = handle.shutdown().await;
        return Err(error);
    }
    Ok(handle)
}

/// Binds the first of `candidates(choice)` that is free on `127.0.0.1`. A port in use is skipped;
/// any other error fails.
async fn bind(choice: PortChoice) -> Result<TcpListener, DaemonError> {
    let mut last = None;
    for port in candidates(choice) {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => last = Some(error),
            Err(error) => {
                return Err(DaemonError::Bind {
                    detail: error.to_string(),
                });
            }
        }
    }
    // `Any` (port 0) is last, and the system does not answer "in use" for it, so this is a
    // defensive answer for a list that ended without a listener.
    Err(DaemonError::Bind {
        detail: last.map_or_else(|| "no port to try".to_string(), |error| error.to_string()),
    })
}

/// Writes `info` to `path` readable by its owner alone, replacing whatever was there.
fn write_daemon_file(path: &Path, info: &DaemonInfo) -> Result<(), DaemonError> {
    let io = |error: std::io::Error| DaemonError::Io {
        detail: format!("{} cannot be written: {error}", path.display()),
    };
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(io)?;
    }
    let text = serde_json::to_string(info).map_err(|error| DaemonError::Io {
        detail: error.to_string(),
    })?;
    crate::write_private(path, text.as_bytes()).map_err(io)
}

/// Thirty-two bytes from the kernel's random source, hex-encoded.
pub(crate) fn random_token() -> Result<String, DaemonError> {
    let mut bytes = [0_u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| DaemonError::Io {
            detail: format!("no token could be made: {error}"),
        })?;
    Ok(hex(&bytes))
}

/// `bytes` in lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The routes, each behind the token: the two hooks, and Farik's MCP server for the session
/// `X-Farik-Session` names. `cancel` ends every MCP session, whose event streams a graceful
/// shutdown would otherwise wait on forever, and every browser socket, which it does not track.
pub(crate) fn router(state: Arc<DaemonState>, token: &str, cancel: CancellationToken) -> Router {
    router_serving::<app::WebApp>(state, token, cancel)
}

/// `router`, serving the embed `E` as the web app.
pub(crate) fn router_serving<E: rust_embed::RustEmbed + 'static>(
    state: Arc<DaemonState>,
    token: &str,
    cancel: CancellationToken,
) -> Router {
    let expected: Arc<str> = Arc::from(format!("Bearer {token}"));
    let server = StreamableHttpService::new(
        || Ok(FarikMcp),
        Arc::new(LocalSessionManager::default()),
        // The stateful sessions Claude Code opens with `initialize` are the default.
        StreamableHttpServerConfig::default()
            .with_json_response(true)
            .with_cancellation_token(cancel.clone()),
    );
    let mcp = Router::new()
        .route_service("/mcp", server)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            mcp::require_session,
        ));
    // Merged after the bearer layer, which a layer only puts on the routes it already has: a
    // browser has no bearer token, and proves itself with its `Origin` and a session instead.
    let browser = Router::new()
        .route("/", get(app::app_from::<E>))
        .route("/connect", get(app::app_from::<E>).post(web::connect))
        .route("/rpc", get(web::rpc))
        .route("/session", get(web::session))
        .route("/disconnect", post(web::disconnect))
        .fallback(get(app::app_from::<E>))
        .layer(Extension(cancel))
        .with_state(Arc::clone(&state));
    Router::new()
        .route("/hook/pre-tool-use", post(pre_tool_use))
        .route("/hook/post-tool-use", post(post_tool_use))
        .route("/command", post(command))
        .route("/connector/launch", post(connector_launch))
        .with_state(Arc::clone(&state))
        .merge(mcp)
        .layer(middleware::from_fn_with_state(state, require_project))
        .layer(middleware::from_fn_with_state(expected, require_token))
        .merge(browser)
}

/// Answers 503 for a route that needs the project, on a daemon in setup mode.
async fn require_project(
    State(state): State<Arc<DaemonState>>,
    request: Request,
    next: Next,
) -> Response {
    if state.deps().is_some() {
        next.run(request).await
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, NO_PROJECT).into_response()
    }
}

/// Refuses a request without `Authorization: Bearer <token>`.
async fn require_token(State(expected): State<Arc<str>>, request: Request, next: Next) -> Response {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if given.is_some_and(|given| same_token(given.as_bytes(), expected.as_bytes())) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Whether `given` is `expected`, in a time that depends on their lengths alone, so that how long
/// a refusal takes says nothing about how much of a guess was right. The length is no secret: every
/// token is sixty-four hex digits.
fn same_token(given: &[u8], expected: &[u8]) -> bool {
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0_u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

async fn pre_tool_use(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<HookRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || decide_pre_tool_use(&request, &state)).await {
        Ok(decision) => Json(decision).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

/// A command the human gave from another terminal, handled by this process's orchestrator and
/// answered with the reply wire.
async fn command(State(state): State<Arc<DaemonState>>, Json(value): Json<Value>) -> Response {
    let reply = match command_from_value(&value) {
        Err(errors) => invalid(&errors),
        Ok(command) => handled(&state, command).await,
    };
    Json(reply_to_value(&reply)).into_response()
}

/// What a command that is not one is answered: `invalid`, with every error the schema found.
fn invalid(errors: &[farik_protocol::command::ValidationError]) -> CommandReply {
    CommandReply::Error {
        kind: ReplyKind::Invalid,
        detail: errors
            .iter()
            .map(|error| format!("{} {}", error.path, error.message))
            .collect::<Vec<_>>()
            .join("; "),
    }
}

/// `command` handled by the orchestrator, or `failed` on a daemon with no handler. The command
/// runs on a task of its own, so that a client that goes away does not cut it off half done.
async fn handled(state: &DaemonState, command: Command) -> CommandReply {
    match state.commands.get() {
        None => CommandReply::Error {
            kind: ReplyKind::Failed,
            detail: "this daemon takes no commands".to_string(),
        },
        Some(handler) => match tokio::spawn(handler(command)).await {
            Ok(result) => reply_of(result),
            Err(error) => CommandReply::Error {
                kind: ReplyKind::Failed,
                detail: format!("the command's task failed: {error}"),
            },
        },
    }
}

/// What `farik connector run` and `farik connector headers` ask: a server of a session.
#[derive(serde::Deserialize)]
struct LaunchAsk {
    session: String,
    server: String,
}

/// `POST /connector/launch` (ADR 0030): a custom connector's command and keys, or its filled
/// headers, for a session the daemon registered and was given it, when it runs as it was connected
/// on this machine. The answer holds keys, so it is never logged.
async fn connector_launch(
    State(state): State<Arc<DaemonState>>,
    Json(asked): Json<LaunchAsk>,
) -> Response {
    let (session, server) = (asked.session.clone(), asked.server.clone());
    let held = Arc::clone(&state);
    let launched = tokio::task::spawn_blocking(move || launch(&held, &asked));
    match tokio::time::timeout(KEY_STORE_DEADLINE, launched).await {
        Ok(Ok(Ok(answer))) => Json(answer).into_response(),
        Ok(Ok(Err((status, reason)))) => (status, reason).into_response(),
        Ok(Err(error)) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
        // A store answering later would find the helper gone, and Claude Code would connect an
        // http server without its headers (re-review N1): refused, and taken away, first.
        Err(_) => {
            take_from_session(&state, &session, &server);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "secret_store_unavailable: the key store did not answer for {server} within \
                     {} seconds",
                    KEY_STORE_DEADLINE.as_secs()
                ),
            )
                .into_response()
        }
    }
}

/// How long the launch route waits for the key store: less than the helper's eight seconds
/// (`EXCHANGE_TIMEOUT`), so the route refuses before the helper gives up.
const KEY_STORE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// Takes `server` from `session`'s registration, so the hook denies its calls
/// `connector_not_in_session`.
fn take_from_session(state: &DaemonState, session: &str, server: &str) {
    if let Some(session) = state.sessions().get_mut(session) {
        session
            .registration
            .connectors
            .retain(|connector| connector.server != server);
    }
}

/// A refusal of the launch route: its status, and the reason's kind, `: `, and what it says.
type Refusal = (StatusCode, String);

/// The launch route's answer, or its refusal. A server refused is taken from the session, so the
/// hook denies its calls `connector_not_in_session`: Claude Code connects an http server without
/// its headers when the helper fails, and would offer its tools (finding I2).
fn launch(state: &DaemonState, asked: &LaunchAsk) -> Result<Value, Refusal> {
    let answer = launch_answer(state, asked);
    if answer.is_err() {
        take_from_session(state, &asked.session, &asked.server);
    }
    answer
}

/// What the launch route answers for `asked`. No refusal names a key's value.
fn launch_answer(state: &DaemonState, asked: &LaunchAsk) -> Result<Value, Refusal> {
    use crate::connectors::{ConnectorError, confirmed_entry, launch_headers, launch_spec};
    use farik_core::team::CustomTransport;

    let not_confirmed = || {
        (
            StatusCode::FORBIDDEN,
            format!(
                "connector_not_confirmed: {} is not as it was connected on this computer; \
                 connect it again",
                asked.server
            ),
        )
    };
    let failed = |detail: String| (StatusCode::INTERNAL_SERVER_ERROR, detail);
    let (server, at) = launched_server(state, asked)?;
    let entry = confirmed_entry(state.connector_secrets().as_ref(), &at, &server)
        .map_err(|error| {
            let detail = match error {
                crate::credential::CredentialError::NoKeychain => {
                    "this computer has no keychain".to_string()
                }
                crate::credential::CredentialError::Failed(detail) => detail,
            };
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("secret_store_unavailable: {detail}"),
            )
        })?
        .ok_or_else(not_confirmed)?;
    let refused = |error: ConnectorError| match error {
        ConnectorError::KeyMissing(_) => not_confirmed(),
        other => failed(format!("{other:?}")),
    };
    let exposed =
        |secrets: &BTreeMap<String, crate::claude::Secret>| -> serde_json::Map<String, Value> {
            secrets
                .iter()
                .map(|(name, value)| (name.clone(), value.expose().into()))
                .collect()
        };
    Ok(match &server.transport {
        CustomTransport::Stdio { .. } => {
            let spec = launch_spec(&server, &entry).map_err(refused)?;
            let folder = state
                .connector_folder(&at)
                .map_err(|error| failed(crate::connectors::folder_refusal(&error)))?;
            serde_json::json!({
                "command": spec.command, "args": spec.args, "env": exposed(&spec.env),
                "cwd": folder.display().to_string()
            })
        }
        CustomTransport::Http { .. } => {
            let headers = launch_headers(&server, &entry).map_err(refused)?;
            serde_json::json!({ "headers": exposed(&headers) })
        }
    })
}

/// The custom server a launch asks for, as the team file has it now, and where its keys are
/// kept: refused for a session the daemon did not register (404 `unknown_session`), and for a
/// server that session was not given or that is no longer a custom server of its agent (403
/// `connector_not_in_session`).
fn launched_server(
    state: &DaemonState,
    asked: &LaunchAsk,
) -> Result<(farik_core::team::CustomServer, crate::connectors::SecretAt), Refusal> {
    let not_in_session = || {
        (
            StatusCode::FORBIDDEN,
            format!(
                "connector_not_in_session: the session {} was not given {}",
                asked.session, asked.server
            ),
        )
    };
    let deps = state
        .deps()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, NO_PROJECT.to_string()))?;
    let agent_id = {
        let sessions = state.sessions();
        let session = sessions.get(&asked.session).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!(
                    "unknown_session: the daemon answers for no session {}",
                    asked.session
                ),
            )
        })?;
        let registration = &session.registration;
        if !registration
            .connectors
            .iter()
            .any(|connector| connector.server == asked.server)
        {
            return Err(not_in_session());
        }
        registration.agent_id.clone()
    };
    // The team file as it is now, which the kept hash is checked against.
    let team = deps
        .files
        .read_team()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let server = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == agent_id)
        .into_iter()
        .flat_map(|agent| agent.mcp_servers.iter().flatten())
        .filter_map(farik_core::team::custom_server)
        .find(|server| server.name == asked.server)
        .ok_or_else(not_in_session)?;
    let at = state
        .secret_at(deps.files.root(), &agent_id, &server.name)
        .map_err(|error| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("secret_store_unavailable: this project's id could not be read: {error}"),
            )
        })?;
    Ok((server, at))
}

async fn post_tool_use(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<HookRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || record_post_tool_use(&request, &state)).await {
        Ok(Ok(())) => Json(serde_json::json!({})).into_response(),
        Ok(Err(error)) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use farik_core::budget::DEFAULT_SESSION_LIMITS;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use farik_protocol::event::EventKind;
    use serde_json::json;

    use super::fixtures::{PRE_READ, TestDaemon};
    use super::{
        DaemonConfig, DaemonInfo, DaemonState, PortChoice, SessionPurpose, SessionRegistration,
        candidates, router, serve,
    };
    use crate::exec::Executor;
    use crate::orchestrator::command_handler;
    use crate::orchestrator::fixtures::Harness;
    use crate::sandbox::host::HostSandbox;

    const TOKEN: &str = "a-token";

    fn post(path: &str, token: Option<&str>, body: &Value) -> Request<Body> {
        let mut request = Request::post(path).header("Content-Type", "application/json");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        request
            .body(Body::from(body.to_string()))
            .expect("a request is built")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_request_without_the_token() {
        let daemon = TestDaemon::new("daemon-token", |_| {});
        let body = daemon.recorded(PRE_READ);
        for path in [
            "/hook/pre-tool-use",
            "/hook/post-tool-use",
            "/mcp",
            "/connector/launch",
        ] {
            for token in [None, Some("another-token")] {
                let answer = router(daemon.state.clone(), TOKEN, CancellationToken::new())
                    .oneshot(post(path, token, &body))
                    .await
                    .expect("the router answers");
                assert_eq!(
                    answer.status(),
                    StatusCode::UNAUTHORIZED,
                    "{path} {token:?}"
                );
            }
        }
        assert!(
            daemon.project.events(&[]).iter().all(|event| !event
                .body
                .kind()
                .to_string()
                .starts_with("tool."))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_the_pre_tool_use_hook() {
        let daemon = TestDaemon::new("daemon-pre", |_| {});
        let answer = router(daemon.state.clone(), TOKEN, CancellationToken::new())
            .oneshot(post(
                "/hook/pre-tool-use",
                Some(TOKEN),
                &daemon.recorded(PRE_READ),
            ))
            .await
            .expect("the router answers");
        assert_eq!(answer.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &to_bytes(answer.into_body(), usize::MAX)
                .await
                .expect("a body"),
        )
        .expect("JSON");
        assert_eq!(
            body["hookSpecificOutput"]["permissionDecision"], "allow",
            "{body}"
        );
        assert_eq!(body["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    }

    #[test]
    fn candidates_are_the_port_the_next_nine_then_any() {
        assert_eq!(
            candidates(PortChoice::Preferred(7420)),
            [
                7420, 7421, 7422, 7423, 7424, 7425, 7426, 7427, 7428, 7429, 0
            ]
        );
        assert_eq!(candidates(PortChoice::Any), [0]);
        assert_eq!(candidates(PortChoice::Exact(7420)), [7420]);
        assert_eq!(
            candidates(PortChoice::Preferred(65530)),
            [65530, 65531, 65532, 65533, 65534, 65535, 0]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn skips_a_port_in_use() {
        let daemon = TestDaemon::new("daemon-port-in-use", |_| {});
        let holder = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port is held");
        let held = holder.local_addr().expect("an address").port();
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Preferred(held),
                daemon_file: Some(daemon.project.repo.path.join(".farik/local/daemon.json")),
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up on another port");
        assert_ne!(handle.info.port, held);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_daemon_start_wipes_old_plugin_folders() {
        let daemon = TestDaemon::new("daemon-wipe-skills", |_| {});
        let state =
            std::path::PathBuf::from(format!("{}-state", daemon.project.repo.path.display()));
        let id = crate::connectors::local_project_id(&state, &daemon.project.repo.path)
            .expect("the project's id");
        let ours = crate::skills::skills_dir(&state, &id);
        std::fs::create_dir_all(ours.join("old-session/skills/api-style")).expect("a leftover");
        std::fs::write(ours.join("old-session/skills/api-style/SKILL.md"), "x").expect("a file");
        // Another project's folders are not this daemon's to remove.
        let others = crate::skills::skills_dir(&state, "another-project-id");
        std::fs::create_dir_all(others.join("session")).expect("another project's folder");
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: None,
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up");
        assert!(!ours.exists(), "the leftover folder is gone");
        assert!(others.join("session").exists());
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn writes_the_daemon_file_and_removes_it_on_shutdown() {
        let daemon = TestDaemon::new("daemon-file", |_| {});
        let daemon_file = daemon.project.repo.path.join(".farik/local/daemon.json");
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: Some(daemon_file.clone()),
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up");
        let written: DaemonInfo = serde_json::from_str(
            &std::fs::read_to_string(&daemon_file).expect("the file is there"),
        )
        .expect("the file is the daemon's info");
        assert_eq!(written, handle.info);
        assert_eq!(written.token.len(), 64);
        assert!(written.token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(written.pid, std::process::id());
        let mode = std::fs::metadata(&daemon_file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let port = written.port;
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
        // `shutdown` answers only once the server's task has finished, so the port is closed by
        // then. Connecting to it afterwards to prove that raced: another test could bind the
        // freed port in between.
        handle.shutdown().await.expect("the daemon stops");
        assert!(!daemon_file.exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn replaces_a_daemon_file_a_crashed_daemon_left() {
        let daemon = TestDaemon::new("daemon-stale", |_| {});
        let daemon_file = daemon.project.repo.path.join(".farik/local/daemon.json");
        std::fs::write(&daemon_file, r#"{"port":1,"token":"old","pid":1}"#).expect("written");
        std::fs::set_permissions(&daemon_file, std::fs::Permissions::from_mode(0o644))
            .expect("the mode is set");
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: Some(daemon_file.clone()),
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up over a stale file");
        let written: DaemonInfo = serde_json::from_str(
            &std::fs::read_to_string(&daemon_file).expect("the file is there"),
        )
        .expect("the file is the daemon's info");
        assert_eq!(written, handle.info);
        let mode = std::fs::metadata(&daemon_file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn listens_on_the_loopback_address_only() {
        let daemon = TestDaemon::new("daemon-loopback", |_| {});
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: Some(daemon.project.repo.path.join(".farik/local/daemon.json")),
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up");
        // The kernel's table of IPv4 sockets: the local address is the address in hex, in the
        // host's byte order, then the port; state 0A is a listening socket.
        let table = std::fs::read_to_string("/proc/net/tcp").expect("the socket table reads");
        let suffix = format!(":{:04X}", handle.info.port);
        let listening: Vec<&str> = table
            .lines()
            .skip(1)
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                (fields.get(3) == Some(&"0A") && fields[1].ends_with(&suffix)).then_some(fields[1])
            })
            .collect();
        assert_eq!(
            listening,
            vec![format!("0100007F{suffix}").as_str()],
            "{table}"
        );
        handle.shutdown().await.expect("the daemon stops");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn builds_the_tool_context_of_a_registered_session() {
        let daemon = TestDaemon::new("daemon-tool-context", |_| {});
        let executor: Arc<dyn Executor> = Arc::new(HostSandbox::new(daemon.worktree.clone()));
        daemon.state.register_session(SessionRegistration {
            session_id: "s-exec".to_string(),
            agent_id: "dev-a".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            cwd: daemon.worktree.clone(),
            executor: Some(Arc::clone(&executor)),
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: Vec::new(),
            connectors: Vec::new(),
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
        let context = daemon
            .state
            .tool_context("s-exec")
            .expect("a registered session has a context");
        assert_eq!(context.agent_id, "dev-a");
        assert_eq!(
            context.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(context.session_id, "s-exec");
        assert!(
            context
                .executor
                .as_ref()
                .is_some_and(|given| Arc::ptr_eq(given, &executor))
        );
        assert!(Arc::ptr_eq(&context.deps, &daemon.project.deps));
        daemon.state.end_session("s-exec");
        assert!(daemon.state.tool_context("s-exec").is_none());
    }

    async fn body_of(answer: axum::response::Response) -> Value {
        serde_json::from_slice(
            &to_bytes(answer.into_body(), usize::MAX)
                .await
                .expect("a body"),
        )
        .expect("JSON")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_command_on_the_daemon() {
        let harness = Harness::new("daemon-command", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        let question = harness.project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
        );
        let n = question.envelope.seq;
        let orchestrator = Arc::new(harness.orchestrator(harness.recorded(Vec::new())));
        assert!(
            harness
                .daemon
                .set_command_handler(command_handler(Arc::clone(&orchestrator)))
        );
        let send = |token: Option<&str>, body: Value| {
            router(harness.daemon.clone(), TOKEN, CancellationToken::new())
                .oneshot(post("/command", token, &body))
        };

        let answer = send(
            Some(TOKEN),
            json!({ "command": "question_answer", "body": { "question_id": n, "answer": "Yes." } }),
        )
        .await
        .expect("the router answers");
        assert_eq!(answer.status(), StatusCode::OK);
        let body = body_of(answer).await;
        let answered = harness.events(&[EventKind::QuestionAnswered]);
        assert_eq!(answered.len(), 1);
        assert_eq!(body["events"], json!([answered[0].envelope.seq]), "{body}");
        assert!(body["said"].is_string(), "{body}");

        let answer = send(
            Some(TOKEN),
            json!({ "command": "question_answer", "body": { "question_id": n, "answer": 7 } }),
        )
        .await
        .expect("the router answers");
        assert_eq!(answer.status(), StatusCode::OK);
        let body = body_of(answer).await;
        assert_eq!(body["error"]["kind"], "invalid", "{body}");

        let answer = send(None, json!({ "command": "run_stop", "body": {} }))
            .await
            .expect("the router answers");
        assert_eq!(answer.status(), StatusCode::UNAUTHORIZED);

        let bare = Arc::new(DaemonState::new(Arc::clone(&harness.project.deps)));
        let answer = router(bare, TOKEN, CancellationToken::new())
            .oneshot(post(
                "/command",
                Some(TOKEN),
                &json!({ "command": "run_stop", "body": {} }),
            ))
            .await
            .expect("the router answers");
        assert_eq!(answer.status(), StatusCode::OK);
        let body = body_of(answer).await;
        assert_eq!(body["error"]["kind"], "failed", "{body}");
        assert_eq!(body["error"]["detail"], "this daemon takes no commands");

        assert!(
            !harness
                .daemon
                .set_command_handler(command_handler(orchestrator))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn wakes_serve_on_a_chat() {
        use crate::orchestrator::{TickReport, Waited};
        use crate::tools::fixtures::at;

        let harness = Harness::new("daemon-chat-wakes", |_| {});
        // Paused by the human: `farik serve` waits, and a chat is answered all the same.
        harness
            .project
            .record("", "team.paused", &json!({ "by": "human" }));
        let orchestrator = Arc::new(harness.orchestrator(harness.recorded(vec![
            crate::recorded::fixtures::chat_answers_with_a_request(),
        ])));
        assert!(
            harness
                .daemon
                .set_command_handler(command_handler(Arc::clone(&orchestrator)))
        );
        let idle = orchestrator.tick().await.expect("the tick runs");
        assert!(matches!(idle, TickReport::Idle { .. }), "{idle:?}");

        let (ended, answer) = tokio::join!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                orchestrator.wait_until(at() + chrono::Duration::hours(1)),
            ),
            async {
                tokio::task::yield_now().await;
                router(harness.daemon.clone(), TOKEN, CancellationToken::new())
                    .oneshot(post(
                        "/command",
                        Some(TOKEN),
                        &json!({ "command": "chat_message_post",
                                 "body": { "agent_id": "dev-a", "text": "Status?" } }),
                    ))
                    .await
                    .expect("the router answers")
            }
        );

        let body = body_of(answer).await;
        assert!(body["said"].is_string(), "{body}");
        assert_eq!(ended.expect("the wait ends"), Waited::Woken);
        let next = orchestrator.tick().await.expect("the tick runs");
        assert!(
            matches!(&next, TickReport::Chat { agent_id, .. } if agent_id == "dev-a"),
            "{next:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn finishes_a_command_whose_client_went_away() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};

        use crate::orchestrator::CommandReport;

        let daemon = TestDaemon::new("daemon-command-gone", |_| {});
        let state = Arc::new(DaemonState::new(Arc::clone(&daemon.project.deps)));
        let done = Arc::new(AtomicBool::new(false));
        let begun = Arc::new(tokio::sync::Notify::new());
        let (finished, beginning) = (Arc::clone(&done), Arc::clone(&begun));
        state.set_command_handler(Arc::new(move |_| {
            let (finished, beginning) = (Arc::clone(&finished), Arc::clone(&beginning));
            Box::pin(async move {
                beginning.notify_one();
                tokio::time::sleep(Duration::from_millis(300)).await;
                finished.store(true, Ordering::SeqCst);
                Ok(CommandReport {
                    said: "handled".to_string(),
                    events: Vec::new(),
                })
            })
        }));
        let request = router(state, TOKEN, CancellationToken::new()).oneshot(post(
            "/command",
            Some(TOKEN),
            &json!({ "command": "run_stop", "body": {} }),
        ));

        // The request is dropped once its command has begun, as a client that hangs up drops it.
        tokio::select! {
            _ = request => panic!("the command answered before its client went away"),
            () = begun.notified() => {}
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        while !done.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < deadline,
                "the command was cut off with its client"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_every_registered_session() {
        let daemon = TestDaemon::new("daemon-session-ids", |_| {});
        // A daemon of its own: the fixture's already answers for a session.
        let state = DaemonState::new(Arc::clone(&daemon.project.deps));
        for id in ["s-2", "s-1"] {
            state.register_session(SessionRegistration {
                session_id: id.to_string(),
                agent_id: "dev-a".to_string(),
                task_id: None,
                cwd: daemon.worktree.clone(),
                executor: None,
                limits: DEFAULT_SESSION_LIMITS,
                farik_tools: Vec::new(),
                tiers: Vec::new(),
                connectors: Vec::new(),
                preview: None,
                purpose: SessionPurpose::Implement,
                in_reply_to: None,
                thread: None,
                skills: Vec::new(),
                skills_root: None,
            });
        }
        assert_eq!(state.session_ids(), ["s-1", "s-2"]);
        state.end_session("s-1");
        assert_eq!(state.session_ids(), ["s-2"]);
    }

    #[test]
    fn compares_a_token_whole() {
        let expected = b"Bearer 0123456789abcdef";
        assert!(super::same_token(b"Bearer 0123456789abcdef", expected));
        assert!(!super::same_token(b"Bearer 0123456789abcdee", expected));
        assert!(!super::same_token(b"Bearer 0123456789abcde", expected));
        assert!(!super::same_token(b"Bearer 0123456789abcdef0", expected));
        assert!(!super::same_token(b"", expected));
    }
    /// Sends one raw HTTP/1.1 request and reads until `until` appears in what came back.
    async fn exchange(stream: &mut tokio::net::TcpStream, request: &str, until: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream.write_all(request.as_bytes()).await.expect("sent");
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !String::from_utf8_lossy(&seen).contains(until) {
            let read = stream.read(&mut buffer).await.expect("read");
            assert!(
                read > 0,
                "closed before {until}: {}",
                String::from_utf8_lossy(&seen)
            );
            seen.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8_lossy(&seen).to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn shuts_down_while_a_session_holds_an_event_stream_open() {
        let daemon = TestDaemon::new("daemon-stream", |_| {});
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: Some(daemon.project.repo.path.join(".farik/local/daemon.json")),
            },
            daemon.state.clone(),
        )
        .await
        .expect("the daemon is up");
        let headers = format!(
            "Host: 127.0.0.1\r\nAuthorization: Bearer {}\r\nX-Farik-Session: {}\r\n\
             Accept: application/json, text/event-stream\r\n",
            handle.info.token,
            super::fixtures::DEV_SESSION
        );
        let initialize = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "2.1.280" }
            }
        })
        .to_string();
        let mut first = tokio::net::TcpStream::connect(("127.0.0.1", handle.info.port))
            .await
            .expect("connects");
        let answered = exchange(
            &mut first,
            &format!(
                "POST /mcp HTTP/1.1\r\n{headers}Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\r\n{initialize}",
                initialize.len()
            ),
            "protocolVersion",
        )
        .await;
        let session = answered
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("mcp-session-id")
                    .then(|| value.trim().to_string())
            })
            .expect("a session id");
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", handle.info.port))
            .await
            .expect("connects");
        exchange(
            &mut stream,
            &format!(
                "GET /mcp HTTP/1.1\r\n{headers}Mcp-Session-Id: {session}\r\n\
                 MCP-Protocol-Version: 2025-06-18\r\n\r\n"
            ),
            "200 OK",
        )
        .await;
        tokio::time::timeout(std::time::Duration::from_secs(10), handle.shutdown())
            .await
            .expect("the daemon stops with a stream open")
            .expect("the daemon stops");
    }

    /// `dev-a`'s custom servers: `github`, started on the host, and `linear`, at a web address.
    fn custom_servers() -> Value {
        json!([
            {
                "name": "github", "source": "custom", "transport": "stdio",
                "command": "github-mcp", "args": ["stdio"],
                "credential_keys": ["API_KEY"],
                "tools": { "search_issues": "network", "delete_repo": "denied" }
            },
            {
                "name": "linear", "source": "custom", "transport": "http",
                "url": "https://mcp.linear.example/mcp",
                "headers": { "Authorization": "Bearer {API_KEY}", "X-Team": "farik" },
                "credential_keys": ["API_KEY"],
                "tools": { "search": "network" }
            }
        ])
    }

    /// A store that fails every read and write.
    struct FailingStore;

    impl crate::connectors::ConnectorSecrets for FailingStore {
        fn load(
            &self,
            _: &crate::connectors::SecretAt,
        ) -> Result<Option<crate::connectors::ConnectorEntry>, crate::credential::CredentialError>
        {
            Err(crate::credential::CredentialError::Failed(
                "the keychain is locked".to_string(),
            ))
        }

        fn save(
            &self,
            _: &crate::connectors::SecretAt,
            _: &crate::connectors::ConnectorEntry,
        ) -> Result<crate::connectors::SecretStore, crate::credential::CredentialError> {
            Err(crate::credential::CredentialError::Failed(
                "the keychain is locked".to_string(),
            ))
        }

        fn delete(
            &self,
            _: &crate::connectors::SecretAt,
        ) -> Result<(), crate::credential::CredentialError> {
            Err(crate::credential::CredentialError::Failed(
                "the keychain is locked".to_string(),
            ))
        }
    }

    const KEY_VALUE: &str = "ghp-a-secret-value";

    /// A daemon whose team gives `dev-a` the custom servers, with `session-custom` registered
    /// with both, and their entries kept as they are now in `store` (`None`: a failing store).
    fn launching(name: &str, connected: bool, failing: bool) -> TestDaemon {
        launching_through(name, connected, |store| {
            if failing {
                Arc::new(FailingStore)
            } else {
                store
            }
        })
    }

    /// [`launching`], its entries kept in a store in memory that `through` wraps.
    fn launching_through(
        name: &str,
        connected: bool,
        through: impl FnOnce(
            Arc<dyn crate::connectors::ConnectorSecrets>,
        ) -> Arc<dyn crate::connectors::ConnectorSecrets>,
    ) -> TestDaemon {
        use crate::connectors::{ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets};
        use farik_core::governor::permissions::SessionConnector;

        let daemon = TestDaemon::new(name, |_| {});
        let team = crate::tools::fixtures::a_team_of_three(|wire| {
            wire["agents"][1]["mcp_servers"] = custom_servers();
        });
        daemon
            .project
            .deps
            .files
            .write_team(&team)
            .expect("the team is written");
        let servers: Vec<farik_core::team::CustomServer> = team.agents[1]
            .mcp_servers
            .iter()
            .flatten()
            .filter_map(farik_core::team::custom_server)
            .collect();
        let store = Arc::new(MemoryConnectorSecrets::default());
        if connected {
            for server in &servers {
                let at = daemon
                    .state
                    .secret_at(daemon.project.deps.files.root(), "dev-a", &server.name)
                    .expect("an address");
                let entry = ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(server),
                    keys: [(
                        "API_KEY".to_string(),
                        crate::claude::Secret::new(KEY_VALUE.to_string()),
                    )]
                    .into(),
                    oauth: None,
                };
                store.save(&at, &entry).expect("kept");
            }
        }
        daemon.state.set_connector_secrets(through(store));
        daemon.state.register_session(SessionRegistration {
            session_id: "session-custom".to_string(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: daemon.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: Vec::new(),
            connectors: servers
                .into_iter()
                .map(|server| SessionConnector {
                    server: server.name,
                    origin: None,
                    tools: server.tools,
                })
                .collect(),
            preview: None,
        });
        daemon
    }

    /// The launch route's status and body for `session` and `server`.
    async fn launch(daemon: &TestDaemon, session: &str, server: &str) -> (StatusCode, String) {
        let answer = router(daemon.state.clone(), TOKEN, CancellationToken::new())
            .oneshot(post(
                "/connector/launch",
                Some(TOKEN),
                &json!({ "session": session, "server": server }),
            ))
            .await
            .expect("the router answers");
        let status = answer.status();
        let body = to_bytes(answer.into_body(), usize::MAX)
            .await
            .expect("a body");
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn launch_answers_a_confirmed_servers_command_keys_and_headers() {
        let daemon = launching("launch-confirmed", true, false);
        let (status, body) = launch(&daemon, "session-custom", "github").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        // It runs in a folder Farik keeps for it in the user's state folder, never the session's
        // worktree, nor anywhere in the repository (finding C1).
        let root = daemon.project.deps.files.root();
        let at = daemon
            .state
            .secret_at(root, "dev-a", "github")
            .expect("an address");
        let folder =
            std::path::PathBuf::from(format!("{}-state", daemon.project.repo.path.display()))
                .join("connectors")
                .join(&at.project_id)
                .join("dev-a/github");
        assert_eq!(
            answer,
            json!({
                "command": "github-mcp", "args": ["stdio"], "env": { "API_KEY": KEY_VALUE },
                "cwd": folder.display().to_string()
            })
        );
        assert!(folder.is_dir(), "{}", folder.display());
        let (status, body) = launch(&daemon, "session-custom", "linear").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(
            answer,
            json!({ "headers": { "Authorization": format!("Bearer {KEY_VALUE}"), "X-Team": "farik" } })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn launch_refuses_an_unregistered_session_or_server() {
        let daemon = launching("launch-unregistered", true, false);
        let (status, body) = launch(&daemon, "session-nobody", "github").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert!(body.starts_with("unknown_session:"), "{body}");
        // dev-a's own session, which was given no connector, and a server no one has.
        for (session, server) in [
            (super::fixtures::DEV_SESSION, "github"),
            ("session-custom", "jira"),
        ] {
            let (status, body) = launch(&daemon, session, server).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{session} {server}: {body}");
            assert!(body.starts_with("connector_not_in_session:"), "{body}");
            assert!(!body.contains(KEY_VALUE), "{body}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn launch_refuses_a_server_changed_since_connect() {
        for (field, value) in [
            ("url", json!("https://attacker.example/mcp")),
            ("command", json!("sh")),
        ] {
            let daemon = launching(&format!("launch-changed-{field}"), true, false);
            let index = usize::from(field == "url");
            let server = if field == "url" { "linear" } else { "github" };
            let team = crate::tools::fixtures::a_team_of_three(|wire| {
                wire["agents"][1]["mcp_servers"] = custom_servers();
                wire["agents"][1]["mcp_servers"][index][field] = value.clone();
            });
            daemon
                .project
                .deps
                .files
                .write_team(&team)
                .expect("the team is changed");
            let (status, body) = launch(&daemon, "session-custom", server).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{field}: {body}");
            assert!(body.starts_with("connector_not_confirmed:"), "{body}");
            assert!(!body.contains(KEY_VALUE), "{body}");
        }
        // Never connected on this machine is the same refusal.
        let daemon = launching("launch-never-connected", false, false);
        let (status, body) = launch(&daemon, "session-custom", "github").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body.starts_with("connector_not_confirmed:"), "{body}");
        // So is an entry as connected but without a key the server names: nothing is launched
        // short of its keys.
        let daemon = launching("launch-key-missing", false, false);
        let team = daemon.project.deps.files.read_team().expect("the team");
        for server in team.agents[1]
            .mcp_servers
            .iter()
            .flatten()
            .filter_map(farik_core::team::custom_server)
        {
            let at = daemon
                .state
                .secret_at(daemon.project.deps.files.root(), "dev-a", &server.name)
                .expect("an address");
            let entry = crate::connectors::ConnectorEntry {
                spec_sha256: farik_core::team::spec_sha256(&server),
                keys: std::collections::BTreeMap::new(),
                oauth: None,
            };
            daemon
                .state
                .connector_secrets()
                .save(&at, &entry)
                .expect("kept");
        }
        for server in ["github", "linear"] {
            let (status, body) = launch(&daemon, "session-custom", server).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{server}: {body}");
            assert!(body.starts_with("connector_not_confirmed:"), "{body}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn launch_answers_503_when_the_store_fails() {
        let daemon = launching("launch-store-fails", true, true);
        let (status, body) = launch(&daemon, "session-custom", "linear").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert!(body.starts_with("secret_store_unavailable:"), "{body}");
        // Claude Code connects an http server without its headers when the helper fails (finding
        // I2): the server refused is taken from the session, so the hook denies its calls.
        assert_eq!(given(&daemon, "session-custom"), ["github"]);
        let decision = super::hooks::decide_pre_tool_use(
            &daemon.call("session-custom", "mcp__linear__search", &json!({})),
            &daemon.state,
        );
        assert!(
            decision.reason.starts_with("connector_not_in_session:"),
            "{decision:?}"
        );
    }

    /// A store whose reads wait until `release` is dropped: a keychain asking to be unlocked.
    struct SlowStore {
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
        inner: Arc<dyn crate::connectors::ConnectorSecrets>,
    }

    impl crate::connectors::ConnectorSecrets for SlowStore {
        fn load(
            &self,
            at: &crate::connectors::SecretAt,
        ) -> Result<Option<crate::connectors::ConnectorEntry>, crate::credential::CredentialError>
        {
            let _ = crate::locked(&self.release).recv();
            self.inner.load(at)
        }

        fn save(
            &self,
            at: &crate::connectors::SecretAt,
            entry: &crate::connectors::ConnectorEntry,
        ) -> Result<crate::connectors::SecretStore, crate::credential::CredentialError> {
            self.inner.save(at, entry)
        }

        fn delete(
            &self,
            at: &crate::connectors::SecretAt,
        ) -> Result<(), crate::credential::CredentialError> {
            self.inner.delete(at)
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_store_slower_than_the_helper_takes_the_server_from_the_session() {
        // The helper gives up after eight seconds and Claude Code connects the http server
        // without its headers; a store that answered after that left it in the session
        // (re-review N1). The route gives up first, and takes it away.
        let (release, wait) = std::sync::mpsc::channel();
        let daemon = launching_through("launch-slow-store", true, |inner| {
            Arc::new(SlowStore {
                release: std::sync::Mutex::new(wait),
                inner,
            })
        });
        // `farik connector headers` gives up after eight seconds (`EXCHANGE_TIMEOUT`).
        let (status, body) = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            launch(&daemon, "session-custom", "linear"),
        )
        .await
        .expect("the route answers before the helper gives up");
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert!(body.starts_with("secret_store_unavailable:"), "{body}");
        assert_eq!(given(&daemon, "session-custom"), ["github"]);
        drop(release);
    }

    /// The servers `session` is given now.
    fn given(daemon: &TestDaemon, session: &str) -> Vec<String> {
        daemon.state.sessions()[session]
            .registration
            .connectors
            .iter()
            .map(|connector| connector.server.clone())
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_refused_launch_takes_the_server_from_the_session() {
        // Changed since connect: refused, and taken away.
        let daemon = launching("launch-takes-away", true, false);
        let team = crate::tools::fixtures::a_team_of_three(|wire| {
            wire["agents"][1]["mcp_servers"] = custom_servers();
            wire["agents"][1]["mcp_servers"][1]["url"] = json!("https://attacker.example/mcp");
        });
        daemon
            .project
            .deps
            .files
            .write_team(&team)
            .expect("the team is changed");
        let (status, _) = launch(&daemon, "session-custom", "linear").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(given(&daemon, "session-custom"), ["github"]);
        // A launch that works leaves the session as it was.
        let (status, body) = launch(&daemon, "session-custom", "github").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(given(&daemon, "session-custom"), ["github"]);
    }
}
