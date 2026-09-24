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
use axum::routing::post;
use axum::{Json, Router};
use farik_core::budget::SessionLimits;
use farik_core::contract::TaskId;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use farik_protocol::command::{
    Command, CommandReply, ReplyKind, command_from_value, reply_to_value,
};
use serde_json::Value;

use self::mcp::FarikMcp;

use crate::exec::Executor;
use crate::orchestrator::{CommandError, CommandReport, reply_of};
use crate::tools::{ToolContext, ToolDeps};

#[cfg(test)]
pub(crate) mod fixtures;
mod hooks;
mod mcp;

#[cfg(test)]
pub(crate) use mcp::listed_names;

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
    /// The Farik tools it was given (`SessionSpec::farik_tools`), by name without the
    /// `mcp__farik__` prefix: the hook denies every other Farik tool (`tool_not_in_session`), and
    /// the MCP server neither lists nor calls one.
    pub farik_tools: Vec<String>,
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
/// session's loop waits on for a stop, and who takes the human's commands.
pub struct DaemonState {
    deps: Arc<ToolDeps>,
    sessions: Mutex<BTreeMap<String, Session>>,
    stops: tokio::sync::Notify,
    commands: OnceLock<CommandHandler>,
}

impl DaemonState {
    /// A daemon for one project, answering for no session yet.
    #[must_use]
    pub fn new(deps: Arc<ToolDeps>) -> DaemonState {
        DaemonState {
            deps,
            sessions: Mutex::new(BTreeMap::new()),
            stops: tokio::sync::Notify::new(),
            commands: OnceLock::new(),
        }
    }

    /// Hands the human's commands to `handler` from now on. It is set once the orchestrator
    /// exists, which is after the daemon is served, since the adapter the orchestrator holds needs
    /// the served port and token. Answers `true`, or `false` when a handler was already set, which
    /// is kept.
    pub fn set_command_handler(&self, handler: CommandHandler) -> bool {
        self.commands.set(handler).is_ok()
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

    /// The ids of the registered sessions of `agent_id`, in order.
    #[must_use]
    pub fn sessions_of(&self, agent_id: &str) -> Vec<String> {
        self.sessions()
            .values()
            .filter(|session| session.registration.agent_id == agent_id)
            .map(|session| session.registration.session_id.clone())
            .collect()
    }

    /// The notice every `request_stop` wakes: a loop takes `notified()` from it before it reads
    /// `stop_reason`, so that no stop falls between the two.
    pub(crate) fn stops(&self) -> &tokio::sync::Notify {
        &self.stops
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
        self.sessions().get(session_id).map(|session| ToolContext {
            agent_id: session.registration.agent_id.clone(),
            task_id: session.registration.task_id.clone(),
            session_id: session.registration.session_id.clone(),
            executor: session.registration.executor.clone(),
            deps: Arc::clone(&self.deps),
        })
    }

    /// The Farik tools a registered session was given, or `None` for one the daemon does not
    /// answer for.
    pub(crate) fn farik_tools(&self, session_id: &str) -> Option<Vec<String>> {
        self.sessions()
            .get(session_id)
            .map(|session| session.registration.farik_tools.clone())
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

/// Where the daemon listens, and where it says so.
pub struct DaemonConfig {
    /// The port to bind on `127.0.0.1`; `None` lets the operating system pick one.
    pub port: Option<u16>,
    /// Where `daemon.json` is written: `.farik/local/daemon.json`.
    pub daemon_file: PathBuf,
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
    daemon_file: PathBuf,
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
        let removed = match std::fs::remove_file(&self.daemon_file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(DaemonError::Io {
                detail: format!("{} cannot be removed: {error}", self.daemon_file.display()),
            }),
            _ => Ok(()),
        };
        served.map_err(|error| DaemonError::Io {
            detail: format!("the server failed: {error}"),
        })?;
        removed
    }
}

/// Starts the daemon on `127.0.0.1`, on the configured port or one the operating system picks,
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
    let token = random_token()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port.unwrap_or(0)))
        .await
        .map_err(|error| DaemonError::Bind {
            detail: error.to_string(),
        })?;
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
        daemon_file: config.daemon_file,
        cancel,
        stop,
        server,
    };
    if let Err(error) = write_daemon_file(&handle.daemon_file, &handle.info) {
        let _ = handle.shutdown().await;
        return Err(error);
    }
    Ok(handle)
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
fn random_token() -> Result<String, DaemonError> {
    let mut bytes = [0_u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| DaemonError::Io {
            detail: format!("no token could be made: {error}"),
        })?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

/// The routes, each behind the token: the two hooks, and Farik's MCP server for the session
/// `X-Farik-Session` names. `cancel` ends every MCP session, whose event streams a graceful
/// shutdown would otherwise wait on forever.
pub(crate) fn router(state: Arc<DaemonState>, token: &str, cancel: CancellationToken) -> Router {
    let expected: Arc<str> = Arc::from(format!("Bearer {token}"));
    let server = StreamableHttpService::new(
        || Ok(FarikMcp),
        Arc::new(LocalSessionManager::default()),
        // The stateful sessions Claude Code opens with `initialize` are the default.
        StreamableHttpServerConfig::default()
            .with_json_response(true)
            .with_cancellation_token(cancel),
    );
    let mcp = Router::new()
        .route_service("/mcp", server)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            mcp::require_session,
        ));
    Router::new()
        .route("/hook/pre-tool-use", post(pre_tool_use))
        .route("/hook/post-tool-use", post(post_tool_use))
        .route("/command", post(command))
        .with_state(state)
        .merge(mcp)
        .layer(middleware::from_fn_with_state(expected, require_token))
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
/// answered with the reply wire. A command that is not one is `invalid`, with every error the
/// schema found; a daemon with no handler answers `failed`. The command runs on a task of its own,
/// so that a client that goes away does not cut it off half done.
async fn command(State(state): State<Arc<DaemonState>>, Json(value): Json<Value>) -> Response {
    let reply = match command_from_value(&value) {
        Err(errors) => CommandReply::Error {
            kind: ReplyKind::Invalid,
            detail: errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; "),
        },
        Ok(command) => match state.commands.get() {
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
        },
    };
    Json(reply_to_value(&reply)).into_response()
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
    use super::{DaemonConfig, DaemonInfo, DaemonState, SessionRegistration, router, serve};
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
        for path in ["/hook/pre-tool-use", "/hook/post-tool-use", "/mcp"] {
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

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn writes_the_daemon_file_and_removes_it_on_shutdown() {
        let daemon = TestDaemon::new("daemon-file", |_| {});
        let daemon_file = daemon.project.repo.path.join(".farik/local/daemon.json");
        let handle = serve(
            DaemonConfig {
                port: None,
                daemon_file: daemon_file.clone(),
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
                port: None,
                daemon_file: daemon_file.clone(),
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
                port: None,
                daemon_file: daemon.project.repo.path.join(".farik/local/daemon.json"),
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
                port: None,
                daemon_file: daemon.project.repo.path.join(".farik/local/daemon.json"),
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
}
