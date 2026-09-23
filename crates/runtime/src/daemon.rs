//! Farik's local service (`docs/SPEC.md` sections 8.2 and 8.6): the governor in front of every tool
//! call a session makes. Claude Code's `PreToolUse` hook asks it for allow or deny, its
//! `PostToolUse` hook reports what came back, and the sessions it knows are the only ones it
//! answers for.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::io::{Read as _, Write as _};
use std::net::Ipv4Addr;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use farik_core::budget::SessionLimits;
use farik_core::contract::TaskId;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

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

/// Where the daemon listens, and where it says so.
pub struct DaemonConfig {
    /// The port to bind on `127.0.0.1`; `None` lets the operating system pick one.
    pub port: Option<u16>,
    /// Where `daemon.json` is written: `.farik/local/daemon.json`.
    pub daemon_file: PathBuf,
}

/// What `daemon.json` holds: how a hook reaches the daemon.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DaemonInfo {
    /// The port on `127.0.0.1`.
    pub port: u16,
    /// The bearer token every request carries.
    pub token: String,
    /// The daemon's process.
    pub pid: u32,
}

/// A daemon that is up.
pub struct DaemonHandle {
    /// How it is reached.
    pub info: DaemonInfo,
    daemon_file: PathBuf,
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
    let app = router(state, &info.token);
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
    // Removed first, because a mode is only given to a file as it is created, and a file a
    // crashed daemon left may be readable by others.
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(error)),
        _ => {}
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io)?;
    let text = serde_json::to_string(info).map_err(|error| DaemonError::Io {
        detail: error.to_string(),
    })?;
    file.write_all(text.as_bytes()).map_err(io)
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

/// The routes, each behind the token.
pub(crate) fn router(state: Arc<DaemonState>, token: &str) -> Router {
    let expected: Arc<str> = Arc::from(format!("Bearer {token}"));
    Router::new()
        .route("/hook/pre-tool-use", post(pre_tool_use))
        .route("/hook/post-tool-use", post(post_tool_use))
        .with_state(state)
        .layer(middleware::from_fn_with_state(expected, require_token))
}

/// Refuses a request without `Authorization: Bearer <token>`.
async fn require_token(State(expected): State<Arc<str>>, request: Request, next: Next) -> Response {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if given == Some(&*expected) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
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

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::fixtures::{PRE_READ, TestDaemon};
    use super::{DaemonConfig, DaemonInfo, router, serve};

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
        for path in ["/hook/pre-tool-use", "/hook/post-tool-use"] {
            for token in [None, Some("another-token")] {
                let answer = router(daemon.state.clone(), TOKEN)
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
        let answer = router(daemon.state.clone(), TOKEN)
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
        handle.shutdown().await.expect("the daemon stops");
        assert!(!daemon_file.exists());
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    }
}
