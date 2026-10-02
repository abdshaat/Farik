//! The browser's way in (`docs/SPEC.md` section 8.6): a one-time code from the terminal, traded
//! at `POST /connect` for a session cookie, on a daemon that checks every browser request's
//! `Origin` and `Host`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use farik_core::contract::{TaskId, TaskStatus};
use farik_core::governor::gates::in_the_backlog;
use farik_core::team::Team;
use farik_protocol::clock::Clock;
use farik_protocol::command::{Command, command_from_value, reply_to_value};
use farik_protocol::event::{EventBody, event_to_value};
use farik_protocol::rpc::{QueryName, rpc_request_from_value};
use farik_store::EventQuery;
use farik_store::projections::TaskProjection;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use super::setup::list_folders;
use super::{DaemonError, DaemonState, SetupError, SetupHost, hex, random_token, same_token};
use super::{board, gates, team, templates};
use crate::claude::CredentialKind;
use crate::computer::{build_sandbox_image, check_computer, pull_browser_image};
use crate::credential::CredentialStore;
use crate::locked;
use crate::sprints::sprint_hold;

/// What the browser routes need, which only `farik serve` gives the daemon (`DaemonState::set_web`).
pub struct WebState {
    /// The code the terminal printed last.
    pub codes: ConnectCodes,
    /// The browsers that traded a code.
    pub sessions: BrowserSessions,
    /// The project the daemon drives.
    pub project_root: PathBuf,
    /// Which credential the sessions run on, or `None` when they are given one.
    pub credential: Option<CredentialKind>,
    /// The daemon's own port, which every browser request's `Origin` and `Host` must name.
    pub port: u16,
    /// The time sessions are issued and checked at, which in setup mode no project's tools give.
    pub clock: Arc<dyn Clock + Send + Sync>,
    /// Why taking the chosen project on failed, which `serve` sets when it goes back to setup
    /// mode, and `serve.status` tells the page.
    pub take_on_error: Mutex<Option<String>>,
    /// Where the AI account's credential is kept, in the order they are tried.
    pub stores: Vec<Arc<dyn CredentialStore>>,
    /// The environment `farik serve` was given, which a credential may come from.
    pub env: BTreeMap<String, String>,
    /// The credential the sessions start with, which connecting the account again replaces;
    /// `None` in setup mode or when the sessions are given one.
    pub in_use: Option<crate::claude::SharedCredential>,
    /// Where saved teams are kept: the state folder's `templates/`, or `None` when there is no
    /// state folder.
    pub templates: Option<crate::templates::Templates>,
    /// Whether a browser at `http://localhost:<port>` is let in without a code, and given a
    /// session by `GET /`: the end-to-end server's `--preview`, which runs as a project's preview
    /// with a temporary team and credential store (step 12, D1). Absent from the release build.
    #[cfg(feature = "e2e")]
    pub admit_local_preview: bool,
}

/// The one live connect code: `issue` replaces it, and `redeem` spends it. It lives in memory, so
/// a restart makes the link it was printed in useless.
#[derive(Default)]
pub struct ConnectCodes {
    live: Mutex<Option<String>>,
}

impl ConnectCodes {
    /// A new code, thirty-two random bytes in hex, which replaces any earlier one.
    ///
    /// # Errors
    ///
    /// `Io` when no random bytes can be read.
    pub fn issue(&self) -> Result<String, DaemonError> {
        let code = random_token()?;
        *locked(&self.live) = Some(code.clone());
        Ok(code)
    }

    /// Whether `code` is the live code, compared in constant time; a code that is spends it.
    #[must_use]
    pub fn redeem(&self, code: &str) -> bool {
        let mut live = locked(&self.live);
        let opens = live
            .as_deref()
            .is_some_and(|live| same_token(code.as_bytes(), live.as_bytes()));
        if opens {
            *live = None;
        }
        opens
    }

    /// Makes `code`, which `redeem` spent, live again, unless a newer code took its place: the
    /// browser it was traded for got no session, so the link still has to open.
    pub fn give_back(&self, code: &str) {
        locked(&self.live).get_or_insert_with(|| code.to_string());
    }
}

/// How long a browser session lasts: thirty days, which is also the cookie's `Max-Age`.
const SESSION_DAYS: i64 = 30;

/// The browsers that traded a code, kept in `<state_dir>/browser-sessions.json` (mode 0600) as the
/// SHA-256 of each session's secret, never the secret. Without a state folder they are kept in
/// memory for the process's life.
pub struct BrowserSessions {
    file: Option<PathBuf>,
    /// The sessions when there is no file; with one, the lock each read-modify-write holds.
    memory: Mutex<Vec<StoredSession>>,
}

/// One session as the file keeps it.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredSession {
    hash: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct SessionsFile {
    sessions: Vec<StoredSession>,
}

impl BrowserSessions {
    /// The sessions kept in `file`, or in memory when there is none. A file that is not there yet
    /// holds no session.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be read or is not a sessions file.
    pub fn open(file: Option<PathBuf>) -> Result<Self, DaemonError> {
        let sessions = BrowserSessions {
            file,
            memory: Mutex::new(Vec::new()),
        };
        sessions.load(&[])?;
        Ok(sessions)
    }

    /// A new session: thirty-two random bytes in hex, whose hash is kept until `now` plus thirty
    /// days. Sessions already expired are dropped as it is written.
    ///
    /// # Errors
    ///
    /// `Io` when no random bytes can be read, or the file cannot be read or written.
    pub fn issue(&self, now: DateTime<Utc>) -> Result<String, DaemonError> {
        let secret = random_token()?;
        let mut memory = locked(&self.memory);
        // ponytail: two `farik serve` processes (two projects) each read, change, and write the
        // one file, so an issue can drop the other's session issued between the two; a lock file
        // beside it fixes that if it ever bites.
        let mut sessions = self.load(&memory)?;
        sessions.retain(|session| now < session.expires_at);
        sessions.push(StoredSession {
            hash: hash(&secret),
            created_at: now,
            expires_at: now + chrono::Duration::days(SESSION_DAYS),
        });
        match &self.file {
            Some(file) => write(file, &sessions)?,
            None => *memory = sessions,
        }
        Ok(secret)
    }

    /// Whether `secret` is a stored session that has not expired at `now`. A file that cannot be
    /// read opens no session.
    #[must_use]
    pub fn verify(&self, secret: &str, now: DateTime<Utc>) -> bool {
        let wanted = hash(secret);
        let memory = locked(&self.memory);
        self.load(&memory).is_ok_and(|sessions| {
            sessions.iter().any(|session| {
                now < session.expires_at && same_token(wanted.as_bytes(), session.hash.as_bytes())
            })
        })
    }

    /// Ends the session `secret`, if it is one.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be read or written.
    pub fn revoke(&self, secret: &str) -> Result<(), DaemonError> {
        let wanted = hash(secret);
        let mut memory = locked(&self.memory);
        let mut sessions = self.load(&memory)?;
        sessions.retain(|session| !same_token(wanted.as_bytes(), session.hash.as_bytes()));
        if let Some(file) = &self.file {
            return write(file, &sessions);
        }
        *memory = sessions;
        Ok(())
    }

    /// What is stored now: the file, read afresh because another process may have written it, or
    /// `memory`, the sessions the caller holds locked.
    fn load(&self, memory: &[StoredSession]) -> Result<Vec<StoredSession>, DaemonError> {
        let Some(file) = &self.file else {
            return Ok(memory.to_vec());
        };
        let io = |detail: String| DaemonError::Io {
            detail: format!("{} cannot be read: {detail}", file.display()),
        };
        match std::fs::read_to_string(file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(io(error.to_string())),
            Ok(text) => serde_json::from_str::<SessionsFile>(&text)
                .map(|stored| stored.sessions)
                .map_err(|error| io(error.to_string())),
        }
    }
}

/// Writes `sessions` to `file`, readable by its owner alone.
fn write(file: &Path, sessions: &[StoredSession]) -> Result<(), DaemonError> {
    let text = serde_json::to_string_pretty(&SessionsFile {
        sessions: sessions.to_vec(),
    })
    .map_err(|error| DaemonError::Io {
        detail: error.to_string(),
    })?;
    crate::write_private(file, text.as_bytes()).map_err(|error| DaemonError::Io {
        detail: format!("{} cannot be written: {error}", file.display()),
    })
}

/// The lowercase hex SHA-256 of `secret`: what the file keeps of a session.
fn hash(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

/// What `/connect` answers a code that does not open.
const USED_LINK: &str =
    "this link has been used or is out of date; start farik serve again for a new one";

/// Whether a browser request comes from the daemon's own page: `Host` is `127.0.0.1:<port>` and
/// `Origin` is `http://127.0.0.1:<port>`, exactly. `localhost` is refused on purpose, since the
/// link and the cookie are bound to `127.0.0.1`; the port counts, because a browser sends the
/// cookie to every port of a host, and a page on another local port is another origin. The one
/// exception is the end-to-end server's `--preview` ([`local_preview`]).
fn from_own_page(headers: &HeaderMap, web: &WebState) -> bool {
    own_host(headers, web)
        && named(headers, header::ORIGIN).is_some_and(|origin| own_origin(origin, web))
}

/// Whether `Host` is `127.0.0.1:<port>`, exactly: what a navigation, which sends no `Origin`, is
/// checked by.
pub(super) fn own_host(headers: &HeaderMap, web: &WebState) -> bool {
    named(headers, header::HOST)
        .is_some_and(|host| host == format!("127.0.0.1:{}", web.port) || local_preview(web, host))
}

/// Whether `origin` is the daemon's own page's.
fn own_origin(origin: &str, web: &WebState) -> bool {
    origin == format!("http://127.0.0.1:{}", web.port)
        || origin
            .strip_prefix("http://")
            .is_some_and(|host| local_preview(web, host))
}

/// Whether `host` is `localhost:<port>`, exactly, on a daemon that admits a local preview. Any
/// other port or name is still refused, so DNS rebinding and cross-site requests stay out.
#[cfg(feature = "e2e")]
fn local_preview(web: &WebState, host: &str) -> bool {
    web.admit_local_preview && host == format!("localhost:{}", web.port)
}

/// No build but the end-to-end server's admits a local preview.
#[cfg(not(feature = "e2e"))]
fn local_preview(_: &WebState, _: &str) -> bool {
    false
}

/// The session cookie a new session's `secret` is set by.
fn session_cookie(secret: &str) -> String {
    format!(
        "farik_session={secret}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        SESSION_DAYS * 24 * 60 * 60
    )
}

/// The cookie of a new session for a request whose `Host` is the local preview's, which needs no
/// code; `None` for any other request.
///
/// # Errors
///
/// As `BrowserSessions::issue`.
#[cfg(feature = "e2e")]
pub(super) fn local_preview_cookie(
    headers: &HeaderMap,
    web: &WebState,
) -> Result<Option<String>, DaemonError> {
    if !named(headers, header::HOST).is_some_and(|host| local_preview(web, host)) {
        return Ok(None);
    }
    web.sessions
        .issue(web.clock.now())
        .map(|secret| Some(session_cookie(&secret)))
}

/// The header `name`, when it is there and is text.
fn named(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers
        .get(name)
        .and_then(|value: &HeaderValue| value.to_str().ok())
}

/// `POST /connect { "code" }`: trades the code the terminal printed for a session, set as an
/// `HttpOnly` cookie, and answers 204. A code that does not open answers 401; a request that is
/// not from the daemon's own page answers 403 before the code is looked at; a daemon without the
/// browser routes answers 404.
pub(super) async fn connect(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !from_own_page(&headers, web) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let code = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|value| value.get("code")?.as_str().map(str::to_string));
    let Some(code) = code.filter(|code| web.codes.redeem(code)) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": USED_LINK })),
        )
            .into_response();
    };
    match web.sessions.issue(web.clock.now()) {
        Ok(secret) => (
            StatusCode::NO_CONTENT,
            [(header::SET_COOKIE, session_cookie(&secret))],
        )
            .into_response(),
        Err(error) => {
            web.codes.give_back(&code);
            (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
        }
    }
}

/// Every session secret a request's `Cookie` header carries: a browser can send the name twice,
/// a stale cookie beside the live one, and either may be the one that opens. One cookie needs no
/// library: the header is `name=value` pairs split by `;`.
fn session_cookies(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix("farik_session="))
}

/// `GET /session`: 204 when the request carries a live session, 401 when it does not. The page
/// asks it before opening `/rpc`, whose refused upgrade gives a browser no status. A same-origin
/// fetch sends no `Origin`, so one is refused only when it is there and foreign; `Host` must be
/// the daemon's own. A daemon without the browser routes answers 404.
pub(super) async fn session(State(state): State<Arc<DaemonState>>, headers: HeaderMap) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let origin = named(&headers, header::ORIGIN);
    if !own_host(&headers, web) || origin.is_some_and(|origin| !own_origin(origin, web)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = web.clock.now();
    if session_cookies(&headers).any(|secret| web.sessions.verify(secret, now)) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// `POST /disconnect`: ends every session the request carries and clears the cookie, answering
/// 204; a request not from the daemon's own page answers 403, and a daemon without the browser
/// routes 404.
pub(super) async fn disconnect(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !from_own_page(&headers, web) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(error) = session_cookies(&headers).try_for_each(|secret| web.sessions.revoke(secret))
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
    }
    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            "farik_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
    )
        .into_response()
}

/// `GET /rpc`: a WebSocket that speaks JSON-RPC 2.0 (`docs/schemas/rpc.schema.json`) for the
/// daemon's own page. A request that is not from that page answers 403, one without a live
/// session 401, and a daemon without the browser routes 404, each before the upgrade.
pub(super) async fn rpc(
    State(state): State<Arc<DaemonState>>,
    Extension(cancel): Extension<CancellationToken>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !from_own_page(&headers, web) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = web.clock.now();
    // ponytail: `verify` reads the sessions file on this async worker, one small read per upgrade;
    // move it to `spawn_blocking` if upgrades ever come in bursts.
    if !session_cookies(&headers).any(|secret| web.sessions.verify(secret, now)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match upgrade {
        Ok(upgrade) => upgrade.on_upgrade(move |socket| talk(socket, state, cancel)),
        Err(rejection) => rejection.into_response(),
    }
}

/// How often a subscription reads the log for new events.
const POLL: Duration = Duration::from_millis(500);

/// One browser's socket, until it or the daemon closes (`cancel`): each request answered in turn,
/// and, while it is subscribed, every event past the last one it was sent.
async fn talk(mut socket: WebSocket, state: Arc<DaemonState>, cancel: CancellationToken) {
    // The seq of the last event sent to the subscription, or `None` while there is none.
    let mut sent: Option<u64> = None;
    // How many polls running have failed to read the log.
    let mut failed = 0;
    // ponytail: the log is re-read every 500 ms, because other processes append to it too and the
    // in-process `EventLog::subscribe` channel misses those; a cross-process notify replaces the
    // poll if 500 ms ever shows.
    let mut poll = tokio::time::interval(POLL);
    // ponytail: a request is answered before the next frame is read, so a long command holds up
    // this socket's other requests and its events; spawning each answer and sending it back through
    // a channel lifts that if a page ever waits on it.
    loop {
        tokio::select! {
            message = socket.recv() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => continue,
                };
                let answer = answer(&state, text.as_str(), &mut sent).await;
                if socket.send(Message::Text(answer.to_string().into())).await.is_err() {
                    return;
                }
            }
            _ = poll.tick(), if sent.is_some() => {}
            () = cancel.cancelled() => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        }
        if !push(&mut socket, &state, &mut sent, &mut failed).await {
            return;
        }
    }
}

/// How many polls running may fail to read the log before the socket is closed.
const READS_TRIED: u32 = 3;

/// Sends the subscription every event past `sent`, oldest first. Answers `false` when the socket
/// is gone. A log that cannot be read is read again at the next poll; after `READS_TRIED` polls
/// running have failed (`failed` counts them), the socket is closed with 1011, so that the page
/// shows the failure rather than wait in silence.
async fn push(
    socket: &mut WebSocket,
    state: &Arc<DaemonState>,
    sent: &mut Option<u64>,
    failed: &mut u32,
) -> bool {
    let Some(after) = *sent else {
        return true;
    };
    let query = EventQuery {
        after_seq: Some(after),
        ..EventQuery::default()
    };
    // A daemon with no project has no log, and nothing to send.
    let Some(deps) = state.deps().cloned() else {
        return true;
    };
    let read = tokio::task::spawn_blocking(move || deps.log.read(&query)).await;
    let Ok(Ok(events)) = read else {
        *failed += 1;
        if *failed < READS_TRIED {
            return true;
        }
        let close = CloseFrame {
            code: close_code::ERROR,
            reason: "farik could not read its event log".into(),
        };
        let _ = socket.send(Message::Close(Some(close))).await;
        return false;
    };
    *failed = 0;
    for event in events {
        *sent = Some(event.envelope.seq);
        let note = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": { "event": event_to_value(&event) },
        });
        if socket
            .send(Message::Text(note.to_string().into()))
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}

/// A JSON-RPC error: its code and its sentence, and for invalid params what the schema found.
#[derive(Debug)]
pub(super) struct Failure {
    code: i64,
    message: String,
    pub(super) data: Option<Value>,
}

impl Failure {
    pub(super) fn new(code: i64, message: impl Into<String>) -> Self {
        Failure {
            code,
            message: message.into(),
            data: None,
        }
    }
}

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const UNKNOWN_METHOD: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
pub(super) const INTERNAL_ERROR: i64 = -32603;
pub(super) const UNKNOWN_QUERY: i64 = -32001;
pub(super) const NOT_FOUND: i64 = -32002;
const REFUSED_HERE: i64 = -32003;
pub(super) const NO_PROJECT: i64 = -32004;
pub(super) const REFUSED: i64 = -32005;

/// The methods the first-run wizard calls, which setup mode's host answers.
const SETUP_METHODS: [&str; 5] = [
    "project.open",
    "project.create",
    "account.connect",
    "sandbox.build",
    "browser.pull",
];
/// The methods whose params hold a secret, whose refusal never quotes them: the schema's errors
/// quote the whole frame.
const SECRET_METHODS: [&str; 3] = ["account.connect", "connector.connect", "connector.tools"];

/// The response to one text frame. A `subscribe` sets `sent` to its `from_seq`, and an
/// `unsubscribe` clears it.
pub(super) async fn answer(state: &Arc<DaemonState>, text: &str, sent: &mut Option<u64>) -> Value {
    let Ok(request) = serde_json::from_str::<Value>(text) else {
        return failure(
            &Value::Null,
            Failure::new(PARSE_ERROR, "the frame is not JSON"),
        );
    };
    let id = request
        .get("id")
        .filter(|id| id.is_i64() || id.is_u64())
        .cloned()
        .unwrap_or(Value::Null);
    let (Some("2.0"), false, Some(method)) = (
        request.get("jsonrpc").and_then(Value::as_str),
        id.is_null(),
        request.get("method").and_then(Value::as_str),
    ) else {
        return failure(
            &id,
            Failure::new(
                INVALID_REQUEST,
                "the frame is not a JSON-RPC 2.0 request with an integer id and a method",
            ),
        );
    };
    if !["subscribe", "unsubscribe", "command", "query"].contains(&method)
        && !SETUP_METHODS.contains(&method)
        && !gates::METHODS.contains(&method)
        && !team::METHODS.contains(&method)
        && !templates::METHODS.contains(&method)
    {
        return failure(
            &id,
            Failure::new(UNKNOWN_METHOD, format!("there is no method {method}")),
        );
    }
    let params = &request["params"];
    if let Err(errors) = rpc_request_from_value(&request) {
        let known = serde_json::from_value::<QueryName>(params["name"].clone()).is_ok();
        let failed = if method == "query" && !known {
            Failure::new(
                UNKNOWN_QUERY,
                format!("there is no query {}", params["name"]),
            )
        } else {
            Failure {
                data: (!SECRET_METHODS.contains(&method)).then(|| json!(errors)),
                ..Failure::new(
                    INVALID_PARAMS,
                    format!("the params of {method} are not right"),
                )
            }
        };
        return failure(&id, failed);
    }
    let result = match method {
        "subscribe" => {
            *sent = params["from_seq"].as_u64();
            Ok(json!({}))
        }
        "unsubscribe" => {
            *sent = None;
            Ok(json!({}))
        }
        "command" => command(state, &params["command"]).await,
        method if gates::METHODS.contains(&method) => gates::call(state, method, params).await,
        method if team::METHODS.contains(&method) => team::call(state, method, params).await,
        method if templates::METHODS.contains(&method) => {
            templates::call(state, method, params).await
        }
        "account.connect" if state.host().is_none() => team::connect(state, params).await,
        "query" => {
            // The store and the files are read off the async workers.
            let (state, params) = (Arc::clone(state), params.clone());
            tokio::task::spawn_blocking(move || {
                query(
                    &state,
                    params["name"].as_str().unwrap_or_default(),
                    &params["params"],
                )
            })
            .await
            .unwrap_or_else(|error| Err(Failure::new(INTERNAL_ERROR, error.to_string())))
        }
        _ => setup_call(state, method, params).await,
    };
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(failed) => failure(&id, failed),
    }
}

/// The error response to the request `id`.
fn failure(id: &Value, failed: Failure) -> Value {
    let mut error = json!({ "code": failed.code, "message": failed.message });
    if let Some(data) = failed.data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

/// `command { command }`: handled as `POST /command` handles it, and answered with the same
/// reply, except `run_stop`, which a page cannot ask for: stopping `farik serve` from its own page
/// would leave the page with nothing to talk to.
async fn command(state: &DaemonState, wire: &Value) -> Result<Value, Failure> {
    let reply = match command_from_value(wire) {
        Err(errors) => super::invalid(&errors),
        Ok(Command::RunStop) => {
            return Err(Failure::new(
                REFUSED_HERE,
                "stopping Farik is done where it runs; pause the team instead",
            ));
        }
        Ok(command) => super::handled(state, command).await,
    };
    Ok(reply_to_value(&reply))
}

/// `query { name, params }`, whose params the schema already passed. A `match` rather than a
/// registry until there are enough queries to want one.
fn query(state: &DaemonState, name: &str, params: &Value) -> Result<Value, Failure> {
    let internal = |error: &dyn std::fmt::Display| Failure::new(INTERNAL_ERROR, error.to_string());
    match name {
        "serve.status" => return serve_status(state),
        "account.status" if state.host().is_none() => return team::account_status(state),
        "folders.list" | "computer.check" | "account.status" => {
            return setup_query(host_of(state)?, name, params);
        }
        _ => {}
    }
    let Some(deps) = state.deps() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    match name {
        "events.since" => {
            let query = EventQuery {
                after_seq: params["after_seq"].as_u64(),
                limit: params["limit"]
                    .as_u64()
                    .and_then(|limit| usize::try_from(limit).ok()),
                ..EventQuery::default()
            };
            let events = deps.log.read(&query).map_err(|error| internal(&error))?;
            Ok(json!({ "events": events.iter().map(event_to_value).collect::<Vec<_>>() }))
        }
        "tasks.list" => {
            let board = deps.projections.board().map_err(|error| internal(&error))?;
            let team = deps.files.read_team().map_err(|error| internal(&error))?;
            let open = open_sprint(deps)?;
            let mut tasks = Vec::with_capacity(board.len());
            for task in &board {
                let mut wire = task_wire(task, &team, open.as_deref());
                // A UI change in review says where its design review stands, for the board's card.
                // One that cannot be read (its contract or branch) leaves only its card without
                // the state, not the whole board blank; the task's page says what failed.
                if task.status == TaskStatus::Verifying
                    && let Ok((true, review)) = deps.transitions.design_review(&team, &task.task_id)
                {
                    wire["design_review_state"] = json!(review.state);
                }
                tasks.push(wire);
            }
            Ok(json!({ "tasks": tasks }))
        }
        "task.get" => {
            let asked = params["task_id"].as_str().unwrap_or_default();
            let missing = || Failure::new(NOT_FOUND, format!("there is no task {asked}"));
            let task_id: TaskId = asked.parse().map_err(|_| missing())?;
            match deps
                .projections
                .task(&task_id)
                .map_err(|error| internal(&error))?
            {
                Some(task) => {
                    let plan = crate::tools::design::read_design_plan(&deps.log, &task_id)
                        .map_err(|error| internal(&error))?;
                    let team = deps.files.read_team().map_err(|error| internal(&error))?;
                    let (ui_change, review) = deps
                        .transitions
                        .design_review(&team, &task_id)
                        .map_err(|error| internal(&error))?;
                    let history = deps
                        .log
                        .read(&EventQuery {
                            task_id: Some(task_id.clone()),
                            ..EventQuery::default()
                        })
                        .map_err(|error| internal(&error))?;
                    let reviews: Vec<Value> = history
                        .iter()
                        .filter_map(|event| match &event.body {
                            EventBody::DesignReviewRecorded(body) => Some(json!({
                                "agent_id": event.envelope.ids.agent_id,
                                "pass": body.pass,
                                "reasons": body.reasons,
                                "recorded_at": event.envelope.recorded_at,
                            })),
                            _ => None,
                        })
                        .collect();
                    Ok(json!({
                        "task": task_wire(&task, &team, open_sprint(deps)?.as_deref()),
                        "design_plan": plan,
                        "ui_change": ui_change,
                        "design_review": ui_change.then_some(review),
                        "design_reviews": reviews,
                    }))
                }
                None => Err(missing()),
            }
        }
        "task.screenshot" => screenshot(deps, params),
        "team.get" | "team.propose" | "team.validate" | "models.list" | "project.scan"
        | "settings.defaults" | "skills.list" | "skill.get" => {
            team::query(state, deps, name, params)
        }
        name if board::QUERIES.contains(&name) => board::query(deps, name, params),
        name if templates::QUERIES.contains(&name) => templates::query(state, deps, name, params),
        _ => gates::query(deps, name, params),
    }
}

/// `task.screenshot { task_id, file }`: the screenshot `file` of the task, in base64, when one of
/// the task's `page.checked` events names it; any other name, one that climbs out of the task's
/// folder among them, is `not_found` (F4).
fn screenshot(deps: &crate::tools::ToolDeps, params: &Value) -> Result<Value, Failure> {
    use base64::Engine as _;

    let asked = params["task_id"].as_str().unwrap_or_default();
    let file = params["file"].as_str().unwrap_or_default();
    let missing = || Failure::new(NOT_FOUND, format!("{asked} has no screenshot {file}"));
    let task_id: TaskId = asked.parse().map_err(|_| missing())?;
    let checked = deps
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![farik_protocol::event::EventKind::PageChecked],
            ..EventQuery::default()
        })
        .map_err(|error| Failure::new(INTERNAL_ERROR, error.to_string()))?;
    let named = checked.iter().any(|event| {
        matches!(&event.body, farik_protocol::event::EventBody::PageChecked(body)
            if body.screenshot.as_str() == file)
    });
    if !named || file.contains(['/', '\\']) || file.starts_with('.') {
        return Err(missing());
    }
    let path = crate::tools::design::screenshots(deps.files.root(), &task_id).join(file);
    let png = std::fs::read(path).map_err(|_| missing())?;
    Ok(json!({ "png_base64": base64::engine::general_purpose::STANDARD.encode(png) }))
}

/// `serve.status`: the project and whether its team is paused, or in setup mode no project and the
/// credential read afresh; why the last take-on failed, if it did; and whether the project waits
/// for the team's setup.
fn serve_status(state: &DaemonState) -> Result<Value, Failure> {
    let web = state
        .web()
        .ok_or_else(|| Failure::new(INTERNAL_ERROR, "the browser routes are off"))?;
    let setup_pending =
        state.deps().is_some() && web.project_root.join(team::SETUP_PENDING).exists();
    let (project_root, paused, credential) = match state.deps() {
        Some(deps) => (
            json!(web.project_root.display().to_string()),
            crate::pause::paused(&deps.log)
                .map_err(|error| Failure::new(INTERNAL_ERROR, error.to_string()))?,
            // The key in use, which connecting the account again may have replaced.
            web.in_use
                .as_ref()
                .map(|in_use| crate::locked(in_use).kind())
                .or(web.credential),
        ),
        None => (
            Value::Null,
            false,
            state
                .host()
                .and_then(|host| host.account())
                .map(|(kind, _)| kind),
        ),
    };
    Ok(json!({
        "project_root": project_root,
        "paused": paused,
        "credential": credential,
        "port": web.port,
        "take_on_error": *locked(&web.take_on_error),
        "setup_pending": setup_pending,
    }))
}

/// The setup host, or the refusal of a setup call on a daemon that has a project.
fn host_of(state: &DaemonState) -> Result<&Arc<dyn SetupHost>, Failure> {
    state.host().ok_or_else(|| {
        Failure::new(
            REFUSED_HERE,
            "farik answers this only while it is being set up",
        )
    })
}

/// The wizard's queries, answered through the setup host.
fn setup_query(host: &Arc<dyn SetupHost>, name: &str, params: &Value) -> Result<Value, Failure> {
    match name {
        "folders.list" => list_folders(&host.home(), params["path"].as_str().unwrap_or_default())
            .map_err(|sentence| Failure::new(REFUSED, sentence)),
        // Setup proposes the six, the UI/UX Designer among them, so its browser is checked.
        "computer.check" => serde_json::to_value(check_computer(&host.env(), true))
            .map_err(|error| Failure::new(INTERNAL_ERROR, error.to_string())),
        _ => Ok(match host.account() {
            Some((kind, source)) => {
                json!({ "provider": "anthropic", "kind": kind, "source": source })
            }
            None => json!({ "provider": null, "kind": null, "source": null }),
        }),
    }
}

/// The wizard's methods, `SETUP_METHODS`, whose params the schema already passed, answered through
/// the setup host off the async workers. A refusal answers `-32005` with the host's sentence.
async fn setup_call(state: &DaemonState, method: &str, params: &Value) -> Result<Value, Failure> {
    let host = Arc::clone(host_of(state)?);
    if method == "sandbox.build" {
        return build_sandbox_image(&host.env())
            .await
            .map(|image| json!({ "image": image }))
            .map_err(|sentence| Failure::new(REFUSED, sentence));
    }
    let (method, params) = (method.to_string(), params.clone());
    tokio::task::spawn_blocking(move || {
        let text = |name: &str| params[name].as_str().unwrap_or_default();
        let no_sandbox = params["no_sandbox"].as_bool().unwrap_or_default();
        let root = |root: PathBuf| json!({ "project_root": root.display().to_string() });
        let answered = match method.as_str() {
            "browser.pull" => {
                return pull_browser_image(&host.env())
                    .map(|image| json!({ "image": image }))
                    .map_err(|sentence| Failure::new(REFUSED, sentence));
            }
            "project.open" => host.open(text("path"), no_sandbox).map(root),
            "project.create" => host
                .create(
                    text("parent"),
                    text("name"),
                    text("description"),
                    no_sandbox,
                )
                .map(root),
            _ => {
                let kind = serde_json::from_value::<CredentialKind>(params["kind"].clone())
                    .map_err(|_| {
                        Failure::new(
                            INVALID_PARAMS,
                            "the params of account.connect are not right",
                        )
                    })?;
                host.connect(kind, text("secret")).map(
                    |(source, taking_on)| json!({ "stored_in": source, "taking_on": taking_on }),
                )
            }
        };
        answered.map_err(|error| match error {
            SetupError::Refused(sentence) => Failure::new(REFUSED, sentence),
            SetupError::Failed(why) => Failure::new(INTERNAL_ERROR, why),
        })
    })
    .await
    .unwrap_or_else(|error| Err(Failure::new(INTERNAL_ERROR, error.to_string())))
}

/// One board row as the RPC schema's `taskProjection`: an optional field is left out, not null,
/// when the projection has none.
/// The id of the sprint that is open, if one is.
fn open_sprint(deps: &crate::tools::ToolDeps) -> Result<Option<String>, Failure> {
    deps.projections
        .open_sprint()
        .map(|open| open.map(|open| open.sprint_id))
        .map_err(|error| Failure::new(INTERNAL_ERROR, error.to_string()))
}

/// The board's row as the wire carries it, with whether it waits in `team`'s Backlog while
/// `open` is the open sprint.
fn task_wire(task: &TaskProjection, team: &Team, open: Option<&str>) -> Value {
    let mut wire = json!({
        "backlog": in_the_backlog(&sprint_hold(team, open, task)),
        "task_id": task.task_id,
        "kind": task.kind,
        "title": task.title,
        "status": task.status,
        "risk": task.risk,
        "triaged": task.triaged,
        "locked": task.locked,
        "updated_seq": task.updated_seq,
        "cost_usd": task.cost_usd,
        "iteration": task.iteration,
        "awaiting_integration": task.awaiting_integration,
        "waiting_on_human": task.waiting_on_human,
        "awaiting_approval": task.awaiting_approval,
        "verifications": task.verifications,
        "rejections": task.rejections,
        "interventions": task.interventions,
    });
    let optional = [
        ("parent", task.parent.as_ref().map(|parent| json!(parent))),
        ("assignee_id", task.assignee_id.as_ref().map(|id| json!(id))),
        ("reviewer_id", task.reviewer_id.as_ref().map(|id| json!(id))),
        ("sprint", task.sprint.as_ref().map(|sprint| json!(sprint))),
    ];
    for (name, value) in optional {
        if let Some(value) = value {
            wire[name] = value;
        }
    }
    wire
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use farik_protocol::command::Command;
    use farik_protocol::event::{EventKind, NewEvent, event_from_value, event_to_value};
    use farik_store::open_event_log;
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
    use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};

    use rust_embed::RustEmbed;

    use super::{BrowserSessions, ConnectCodes, WebState};
    use crate::claude::CredentialKind;
    use crate::credential::{Source, credential_of_kind};
    use crate::daemon::app::fixtures::{Fixture, Unbuilt};
    use crate::daemon::fixtures::TestDaemon;
    use crate::daemon::{
        DaemonConfig, DaemonHandle, DaemonState, PortChoice, router, router_serving, serve,
    };
    use crate::daemon::{SetupError, SetupHost};
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{CommandReport, command_handler};
    use crate::tools::ToolDeps;
    use crate::tools::fixtures::at;
    use farik_protocol::clock::FixedClock;

    /// The port the daemon under test says it is on; nothing binds it, since the requests go
    /// straight to the router.
    const PORT: u16 = 49_731;
    const TOKEN: &str = "a-token";
    const ORIGIN: &str = "http://127.0.0.1:49731";
    const HOST: &str = "127.0.0.1:49731";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("farik-web-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .expect("the mode is set");
        dir
    }

    fn is_hex_64(text: &str) -> bool {
        text.len() == 64
            && text
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    }

    #[test]
    fn a_code_opens_once() {
        let codes = ConnectCodes::default();
        let code = codes.issue().expect("a code");
        assert!(is_hex_64(&code), "{code}");
        // Neither a prefix of the live code nor nothing opens it, and neither spends it.
        assert!(!codes.redeem(&code[..32]));
        assert!(!codes.redeem(""));
        assert!(codes.redeem(&code));
        assert!(!codes.redeem(&code));

        let earlier = codes.issue().expect("a code");
        let later = codes.issue().expect("a code");
        assert_ne!(earlier, later);
        assert!(!codes.redeem(&earlier));
        assert!(codes.redeem(&later));
        assert!(!codes.redeem(""));
    }

    #[test]
    fn sessions_keep_only_hashes() {
        let file = scratch("hashes").join("browser-sessions.json");
        let sessions = BrowserSessions::open(Some(file.clone())).expect("the sessions open");
        let first = sessions.issue(now()).expect("a session");
        let second = sessions.issue(now()).expect("a session");
        assert!(is_hex_64(&first), "{first}");

        let text = std::fs::read_to_string(&file).expect("the file is there");
        assert!(!text.contains(&first) && !text.contains(&second), "{text}");
        let written: Value = serde_json::from_str(&text).expect("JSON");
        let hashes: Vec<&str> = written["sessions"]
            .as_array()
            .expect("a list of sessions")
            .iter()
            .map(|session| session["hash"].as_str().expect("a hash"))
            .collect();
        assert_eq!(hashes.len(), 2, "{text}");
        assert!(hashes.iter().all(|hash| is_hex_64(hash)), "{text}");
        let digest =
            Sha256::digest(first.as_bytes())
                .iter()
                .fold(String::new(), |mut hex, byte| {
                    let _ = write!(hex, "{byte:02x}");
                    hex
                });
        assert_eq!(hashes[0], digest);
        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        // The file is what is believed: another process's sessions open the same browser.
        let reopened = BrowserSessions::open(Some(file)).expect("the sessions open");
        assert!(reopened.verify(&second, now()));
    }

    #[test]
    fn a_session_lasts_thirty_days() {
        let sessions = BrowserSessions::open(None).expect("the sessions open");
        let secret = sessions.issue(now()).expect("a session");
        assert!(sessions.verify(&secret, now()));
        assert!(sessions.verify(&secret, now() + Duration::days(29)));
        assert!(!sessions.verify(&secret, now() + Duration::days(30) + Duration::seconds(1)));
        assert!(!sessions.verify(&"0".repeat(64), now()));
    }

    /// A daemon whose browser routes answer, and a code to connect with.
    fn served(name: &str) -> (TestDaemon, String) {
        served_with(name, None)
    }

    /// `served`, its sessions kept in `file` when one is given.
    fn served_with(name: &str, file: Option<PathBuf>) -> (TestDaemon, String) {
        let daemon = TestDaemon::new(name, |_| {});
        let codes = ConnectCodes::default();
        let code = codes.issue().expect("a code");
        assert!(daemon.state.set_web(WebState {
            codes,
            sessions: BrowserSessions::open(file).expect("the sessions open"),
            project_root: daemon.project.repo.path.clone(),
            credential: None,
            port: PORT,
            clock: Arc::new(FixedClock::new(now())),
            take_on_error: std::sync::Mutex::default(),
            stores: Vec::new(),
            env: std::collections::BTreeMap::new(),
            in_use: None,
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
        (daemon, code)
    }

    fn connect(origin: Option<&str>, host: &str, code: &str) -> Request<Body> {
        let mut request = Request::post("/connect")
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        request
            .body(Body::from(json!({ "code": code }).to_string()))
            .expect("a request is built")
    }

    async fn send(state: &Arc<DaemonState>, request: Request<Body>) -> axum::response::Response {
        router(Arc::clone(state), TOKEN, CancellationToken::new())
            .oneshot(request)
            .await
            .expect("the router answers")
    }

    async fn body_text(answer: axum::response::Response) -> String {
        String::from_utf8(
            to_bytes(answer.into_body(), usize::MAX)
                .await
                .expect("a body")
                .to_vec(),
        )
        .expect("text")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn connect_sets_the_session_cookie() {
        let (daemon, code) = served("web-cookie");
        let answer = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(answer.status(), StatusCode::NO_CONTENT);
        let cookie = answer
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .expect("a cookie is set")
            .to_string();
        let parts: Vec<&str> = cookie.split(';').map(str::trim).collect();
        let secret = parts[0]
            .strip_prefix("farik_session=")
            .expect("the cookie is the session");
        for part in ["HttpOnly", "SameSite=Strict", "Path=/", "Max-Age=2592000"] {
            assert!(parts.contains(&part), "{cookie}");
        }
        assert!(!parts.contains(&"Secure"), "{cookie}");
        let now = daemon.state.deps().expect("a project").clock.now();
        let web = daemon.state.web().expect("the browser routes are on");
        assert!(web.sessions.verify(secret, now), "{cookie}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn connect_refuses_a_used_code() {
        let (daemon, code) = served("web-used");
        let first = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(first.status(), StatusCode::NO_CONTENT);
        let unknown = "0".repeat(64);
        for code in [code.as_str(), unknown.as_str()] {
            let answer = send(&daemon.state, connect(Some(ORIGIN), HOST, code)).await;
            assert_eq!(answer.status(), StatusCode::UNAUTHORIZED);
            assert!(answer.headers().get(header::SET_COOKIE).is_none());
            let body: Value = serde_json::from_str(&body_text(answer).await).expect("JSON");
            assert_eq!(
                body,
                json!({ "error": "this link has been used or is out of date; start farik serve again for a new one" })
            );
        }
    }

    #[test]
    fn gives_a_spent_code_back_only_while_no_newer_one_is_live() {
        let codes = ConnectCodes::default();
        let old = codes.issue().expect("a code");
        assert!(codes.redeem(&old));
        let new = codes.issue().expect("a code");
        codes.give_back(&old);
        assert!(!codes.redeem(&old));
        assert!(codes.redeem(&new));
        codes.give_back(&new);
        assert!(codes.redeem(&new));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_the_code_when_the_session_cannot_be_written() {
        let folder =
            std::env::temp_dir().join(format!("farik-web-unwritten-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("made");
        let (daemon, code) =
            served_with("web-unwritten", Some(folder.join("browser-sessions.json")));
        // The state folder is a file now, so the session cannot be kept.
        std::fs::remove_dir(&folder).expect("removed");
        std::fs::write(&folder, "").expect("written");
        let failed = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(failed.headers().get(header::SET_COOKIE).is_none());

        // The failure spent nothing: the same link opens once the folder is back.
        std::fs::remove_file(&folder).expect("removed");
        std::fs::create_dir_all(&folder).expect("made");
        let opened = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(opened.status(), StatusCode::NO_CONTENT);
        let again = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(again.status(), StatusCode::UNAUTHORIZED);
        std::fs::remove_dir_all(&folder).expect("removed");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn connect_refuses_a_foreign_origin_or_host() {
        let (daemon, code) = served("web-origin");
        let another_port = format!("http://127.0.0.1:{}", PORT + 1);
        let localhost = format!("http://localhost:{PORT}");
        let refused = [
            (Some("http://evil.example"), HOST),
            (Some(localhost.as_str()), HOST),
            (Some(another_port.as_str()), HOST),
            (None, HOST),
            (Some(ORIGIN), "attacker.example"),
        ];
        for (origin, host) in refused {
            let answer = send(&daemon.state, connect(origin, host, &code)).await;
            assert_eq!(answer.status(), StatusCode::FORBIDDEN, "{origin:?} {host}");
            assert!(answer.headers().get(header::SET_COOKIE).is_none());
            assert_eq!(body_text(answer).await, "", "{origin:?} {host}");
        }
        // A refused request never reached the code, which still opens.
        let answer = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(answer.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn browser_routes_are_absent_without_web_state() {
        let daemon = TestDaemon::new("web-absent", |_| {});
        let answer = send(&daemon.state, connect(Some(ORIGIN), HOST, &"0".repeat(64))).await;
        assert_eq!(answer.status(), StatusCode::NOT_FOUND);
        for request in [
            page("/", HOST),
            page("/settings", HOST),
            page("/session", HOST),
            disconnect("farik_session=x"),
        ] {
            let path = request.uri().to_string();
            let answer = fetch::<Fixture>(&daemon.state, request).await;
            assert_eq!(answer.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    // The app's own routes, served from a test embed in place of the built web app.

    /// `request` answered by the router, serving the embed `E` as the web app.
    async fn fetch<E: RustEmbed + 'static>(
        state: &Arc<DaemonState>,
        request: Request<Body>,
    ) -> axum::response::Response {
        router_serving::<E>(Arc::clone(state), TOKEN, CancellationToken::new())
            .oneshot(request)
            .await
            .expect("the router answers")
    }

    /// A navigation to `path`: a browser sends it with a `Host` and no `Origin`.
    fn page(path: &str, host: &str) -> Request<Body> {
        Request::get(path)
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("a request is built")
    }

    fn header_of<'a>(answer: &'a axum::response::Response, name: &header::HeaderName) -> &'a str {
        answer
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_else(|| panic!("no {name} header"))
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_the_app_and_its_routes() {
        let (daemon, _) = served("app-serves");
        for path in ["/", "/settings"] {
            let answer = fetch::<Fixture>(&daemon.state, page(path, HOST)).await;
            assert_eq!(answer.status(), StatusCode::OK, "{path}");
            assert_eq!(
                header_of(&answer, &header::CONTENT_TYPE),
                "text/html; charset=utf-8",
                "{path}"
            );
            assert_eq!(
                header_of(&answer, &header::CACHE_CONTROL),
                "no-store",
                "{path}"
            );
            assert_eq!(
                body_text(answer).await,
                include_str!("app-fixture/index.html"),
                "{path}"
            );
        }
        let asset = fetch::<Fixture>(&daemon.state, page("/assets/app-3f2a.js", HOST)).await;
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(
            header_of(&asset, &header::CONTENT_TYPE),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            header_of(&asset, &header::CACHE_CONTROL),
            "max-age=31536000, immutable"
        );
        assert_eq!(
            body_text(asset).await,
            include_str!("app-fixture/assets/app-3f2a.js")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn says_when_the_app_was_not_built() {
        let (daemon, _) = served("app-unbuilt");
        let answer = fetch::<Unbuilt>(&daemon.state, page("/", HOST)).await;
        assert_eq!(answer.status(), StatusCode::OK);
        assert_eq!(
            body_text(answer).await,
            "The web app was not built into this farik. Run pnpm --filter @farik/web build, then build farik again."
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_the_security_headers() {
        let (daemon, _) = served("app-headers");
        let policy = format!(
            "default-src 'self'; connect-src 'self' ws://127.0.0.1:{PORT}; img-src 'self' data:; font-src 'self' data:; style-src 'self'; frame-ancestors 'none'"
        );
        let answers = [
            fetch::<Fixture>(&daemon.state, page("/", HOST)).await,
            fetch::<Fixture>(&daemon.state, page("/assets/app-3f2a.js", HOST)).await,
            fetch::<Unbuilt>(&daemon.state, page("/", HOST)).await,
        ];
        for answer in &answers {
            assert_eq!(header_of(answer, &header::CONTENT_SECURITY_POLICY), policy);
            assert_eq!(
                header_of(answer, &header::X_CONTENT_TYPE_OPTIONS),
                "nosniff"
            );
            assert_eq!(header_of(answer, &header::REFERRER_POLICY), "no-referrer");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_the_app_only_to_its_own_host() {
        let (daemon, _) = served("app-host");
        let localhost = format!("localhost:{PORT}");
        for host in ["evil.example", localhost.as_str()] {
            for path in ["/", "/assets/app-3f2a.js"] {
                let answer = fetch::<Fixture>(&daemon.state, page(path, host)).await;
                assert_eq!(answer.status(), StatusCode::FORBIDDEN, "{host} {path}");
                assert_eq!(body_text(answer).await, "", "{host} {path}");
            }
        }
        let answer = fetch::<Fixture>(&daemon.state, page("/", HOST)).await;
        assert_eq!(answer.status(), StatusCode::OK);
    }

    /// `GET /session` with `cookie`, from `origin` when there is one.
    fn session(origin: Option<&str>, host: &str, cookie: Option<&str>) -> Request<Body> {
        let mut request = Request::get("/session").header(header::HOST, host);
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.body(Body::empty()).expect("a request is built")
    }

    /// `POST /disconnect` from the daemon's own page, with `cookie`.
    fn disconnect(cookie: &str) -> Request<Body> {
        Request::post("/disconnect")
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .expect("a request is built")
    }

    /// A new session on `daemon`'s browser routes.
    fn a_session(daemon: &TestDaemon) -> String {
        let web = daemon.state.web().expect("the browser routes are on");
        web.sessions
            .issue(daemon.state.deps().expect("a project").clock.now())
            .expect("a session")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn tells_the_page_whether_it_has_a_session() {
        let (daemon, _) = served("app-session");
        let secret = a_session(&daemon);
        let live = format!("theme=dark; farik_session={secret}");
        let unknown = format!("farik_session={}", "0".repeat(64));
        let asked = [
            (None, HOST, Some(live.as_str()), StatusCode::NO_CONTENT),
            (
                Some(ORIGIN),
                HOST,
                Some(live.as_str()),
                StatusCode::NO_CONTENT,
            ),
            (None, HOST, None, StatusCode::UNAUTHORIZED),
            (None, HOST, Some(unknown.as_str()), StatusCode::UNAUTHORIZED),
            (
                Some("http://evil.example"),
                HOST,
                Some(live.as_str()),
                StatusCode::FORBIDDEN,
            ),
            (
                None,
                "evil.example",
                Some(live.as_str()),
                StatusCode::FORBIDDEN,
            ),
        ];
        for (origin, host, cookie, status) in asked {
            let answer = send(&daemon.state, session(origin, host, cookie)).await;
            assert_eq!(answer.status(), status, "{origin:?} {host} {cookie:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn disconnect_revokes_the_session() {
        let (daemon, _) = served("app-disconnect");
        let secret = a_session(&daemon);
        let other = a_session(&daemon);
        let carried = format!("farik_session=junk; farik_session={secret}");
        let now = daemon.state.deps().expect("a project").clock.now();
        let web = daemon.state.web().expect("the browser routes are on");

        // Not from the daemon's own page: refused, and the session lives on.
        let foreign = Request::post("/disconnect")
            .header(header::HOST, HOST)
            .header(header::COOKIE, carried.as_str())
            .body(Body::empty())
            .expect("a request is built");
        assert_eq!(
            send(&daemon.state, foreign).await.status(),
            StatusCode::FORBIDDEN
        );
        assert!(web.sessions.verify(&secret, now));

        let answer = send(&daemon.state, disconnect(&carried)).await;
        assert_eq!(answer.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            header_of(&answer, &header::SET_COOKIE),
            "farik_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
        );
        assert!(!web.sessions.verify(&secret, now));
        // Another browser's session is not this one's to end.
        assert!(web.sessions.verify(&other, now));
        let asked = send(&daemon.state, session(None, HOST, Some(&carried))).await;
        assert_eq!(asked.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn browser_routes_need_no_bearer_and_others_still_do() {
        let (daemon, code) = served("web-bearer");
        let answer = send(&daemon.state, connect(Some(ORIGIN), HOST, &code)).await;
        assert_eq!(answer.status(), StatusCode::NO_CONTENT);
        for path in [
            "/command",
            "/hook/pre-tool-use",
            "/hook/post-tool-use",
            "/mcp",
        ] {
            for token in [None, Some("another-token")] {
                let mut request = Request::post(path)
                    .header(header::HOST, HOST)
                    .header(header::ORIGIN, ORIGIN)
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(token) = token {
                    request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
                }
                let request = request
                    .body(Body::from(
                        json!({ "command": "run_stop", "body": {} }).to_string(),
                    ))
                    .expect("a request is built");
                let answer = send(&daemon.state, request).await;
                assert_eq!(
                    answer.status(),
                    StatusCode::UNAUTHORIZED,
                    "{path} {token:?}"
                );
            }
        }
    }

    // The socket tests: a daemon served on a port the system gave, its browser routes on, and a
    // WebSocket client talking to `/rpc` as a browser would.

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// How long a test waits for a frame before it fails rather than hang.
    const BOUND: std::time::Duration = std::time::Duration::from_secs(30);

    /// Serves `state` on a port the system gave, turns its browser routes on, and answers the
    /// handle and a session secret.
    async fn on_a_socket(
        state: &Arc<DaemonState>,
        root: &std::path::Path,
    ) -> (DaemonHandle, String) {
        let handle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: Some(root.join(".farik/local/daemon.json")),
            },
            Arc::clone(state),
        )
        .await
        .expect("the daemon is up");
        let sessions = BrowserSessions::open(None).expect("the sessions open");
        let secret = sessions
            .issue(state.deps().expect("a project").clock.now())
            .expect("a session");
        assert!(state.set_web(WebState {
            codes: ConnectCodes::default(),
            sessions,
            project_root: root.to_path_buf(),
            credential: None,
            port: handle.info.port,
            clock: Arc::clone(&state.deps().expect("a project").clock),
            take_on_error: std::sync::Mutex::default(),
            stores: Vec::new(),
            env: std::collections::BTreeMap::new(),
            in_use: None,
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
        (handle, secret)
    }

    /// `harness`'s daemon served with its orchestrator taking commands, and a socket open to it.
    async fn driven(harness: &Harness) -> (DaemonHandle, Socket) {
        let orchestrator = Arc::new(harness.orchestrator(harness.recorded(Vec::new())));
        assert!(
            harness
                .daemon
                .set_command_handler(command_handler(orchestrator))
        );
        let (handle, secret) = on_a_socket(&harness.daemon, &harness.project.repo.path).await;
        let socket = open(handle.info.port, &secret).await;
        (handle, socket)
    }

    /// The upgrade a browser on `origin` sends, with `cookie` as its `Cookie` header.
    fn upgrade(port: u16, origin: Option<&str>, cookie: Option<&str>) -> WsRequest {
        let mut request = format!("ws://127.0.0.1:{port}/rpc")
            .into_client_request()
            .expect("a request is built");
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert(header::ORIGIN, origin.parse().expect("a header"));
        }
        if let Some(cookie) = cookie {
            request
                .headers_mut()
                .insert(header::COOKIE, cookie.parse().expect("a header"));
        }
        request
    }

    /// A socket to `/rpc` from the daemon's own page, with the session `secret`.
    async fn open(port: u16, secret: &str) -> Socket {
        let origin = format!("http://127.0.0.1:{port}");
        let cookie = format!("theme=dark; farik_session={secret}");
        let (socket, _) = connect_async(upgrade(port, Some(&origin), Some(&cookie)))
            .await
            .expect("the socket opens");
        socket
    }

    /// The status an upgrade was refused with.
    async fn refused(request: WsRequest) -> u16 {
        match connect_async(request).await {
            Err(WsError::Http(answer)) => answer.status().as_u16(),
            Err(error) => panic!("the upgrade failed otherwise: {error}"),
            Ok(_) => panic!("the upgrade was accepted"),
        }
    }

    /// The next text frame, as JSON.
    async fn next(socket: &mut Socket) -> Value {
        loop {
            let frame = tokio::time::timeout(BOUND, socket.next())
                .await
                .expect("a frame within the bound")
                .expect("the socket is open")
                .expect("a frame");
            if let WsMessage::Text(text) = frame {
                return serde_json::from_str(text.as_str()).expect("the frame is JSON");
            }
        }
    }

    async fn send_text(socket: &mut Socket, text: String) {
        socket
            .send(WsMessage::Text(text.into()))
            .await
            .expect("the frame is sent");
    }

    /// Sends a request and answers the frame that follows it.
    async fn call(socket: &mut Socket, id: u64, method: &str, params: &Value) -> Value {
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        send_text(socket, request.to_string()).await;
        next(socket).await
    }

    /// The result of `query { name, params }`, checked against its result definition in the RPC
    /// schema.
    async fn query(
        socket: &mut Socket,
        id: u64,
        name: &str,
        params: &Value,
        definition: &str,
    ) -> Value {
        let answer = call(
            socket,
            id,
            "query",
            &json!({ "name": name, "params": params }),
        )
        .await;
        assert_eq!(answer["id"], id, "{answer}");
        conforms(&answer["result"], definition);
        answer["result"].clone()
    }

    /// Fails unless `value` is what `definition` of the RPC schema says.
    fn conforms(value: &Value, definition: &str) {
        let schema: Value =
            serde_json::from_str(farik_protocol::rpc::SCHEMA_JSON).expect("the schema is JSON");
        let root = json!({
            "$schema": schema["$schema"],
            "$ref": format!("#/$defs/{definition}"),
            "$defs": schema["$defs"],
        });
        let validator = jsonschema::options()
            .build(&root)
            .expect("the definition compiles");
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{definition}: {errors:?} in {value}");
    }

    /// Appends a `team.paused` through `log`.
    fn paused_through(log: &farik_store::event_log::EventLog) -> u64 {
        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": "2026-09-28T10:00:00Z",
            "team_id": "farik", "project_id": "farik",
            "kind": "team.paused", "body": { "by": "human" }
        }))
        .expect("the fixture is an event");
        log.append(&NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        })
        .expect("appends")
        .envelope
        .seq
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_an_upgrade_without_a_session() {
        let daemon = TestDaemon::new("rpc-refuses", |_| {});
        let (handle, secret) = on_a_socket(&daemon.state, &daemon.project.repo.path).await;
        let port = handle.info.port;
        let own = format!("http://127.0.0.1:{port}");
        let session = format!("farik_session={secret}");

        let expired = daemon
            .state
            .web()
            .expect("the browser routes are on")
            .sessions
            .issue(daemon.state.deps().expect("a project").clock.now() - Duration::days(31))
            .expect("a session");
        let unknown = format!("farik_session={}", "0".repeat(64));
        let expired = format!("farik_session={expired}");
        for cookie in [None, Some(unknown.as_str()), Some(expired.as_str())] {
            assert_eq!(
                refused(upgrade(port, Some(&own), cookie)).await,
                401,
                "{cookie:?}"
            );
        }

        let another_port = format!("http://127.0.0.1:{}", port.wrapping_add(1));
        for origin in [
            Some("http://evil.example"),
            Some(another_port.as_str()),
            None,
        ] {
            assert_eq!(
                refused(upgrade(port, origin, Some(&session))).await,
                403,
                "{origin:?}"
            );
        }

        // A stale `farik_session` ahead of the live one does not hide it.
        let shadowed = format!("farik_session=junk; farik_session={secret}");
        connect_async(upgrade(port, Some(&own), Some(&shadowed)))
            .await
            .expect("the live session behind a stale one opens");

        // The same session from the daemon's own page opens.
        let mut socket = open(port, &secret).await;
        let answer = call(&mut socket, 1, "unsubscribe", &json!({})).await;
        assert_eq!(answer, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_browser_sockets_on_shutdown() {
        let daemon = TestDaemon::new("rpc-shutdown", |_| {});
        let (handle, secret) = on_a_socket(&daemon.state, &daemon.project.repo.path).await;
        let mut socket = open(handle.info.port, &secret).await;
        let answer = call(&mut socket, 1, "unsubscribe", &json!({})).await;
        assert_eq!(answer["result"], json!({}), "{answer}");

        handle.shutdown().await.expect("the daemon stops");
        // The socket ends with the daemon: a close frame or the end of the stream, in time.
        let ended = tokio::time::timeout(BOUND, socket.next())
            .await
            .expect("the socket ends within the bound");
        assert!(
            matches!(ended, Some(Ok(WsMessage::Close(_)) | Err(_)) | None),
            "the socket still talks: {ended:?}"
        );
        // A request after it gets no answer.
        let request = json!({ "jsonrpc": "2.0", "id": 2, "method": "unsubscribe", "params": {} });
        if socket
            .send(WsMessage::Text(request.to_string().into()))
            .await
            .is_ok()
        {
            let later =
                tokio::time::timeout(std::time::Duration::from_secs(2), socket.next()).await;
            assert!(
                !matches!(later, Ok(Some(Ok(WsMessage::Text(_))))),
                "{later:?}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn streams_events_after_the_sequence_asked() {
        let daemon = TestDaemon::new("rpc-streams", |_| {});
        // A log in a file, so that another handle on it is another writer, as another process is.
        let path = daemon.project.repo.path.join(".farik/local/stream.db");
        let log = Arc::new(open_event_log(&path, at()).expect("the log opens"));
        let other = open_event_log(&path, at()).expect("the log opens again");
        let deps = &daemon.project.deps;
        let state = Arc::new(DaemonState::new(Arc::new(ToolDeps {
            log: Arc::clone(&log),
            projections: Arc::clone(&deps.projections),
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: daemon.project.repo.adapter(),
            clock: Arc::clone(&deps.clock),
            ids: deps.ids.clone(),
            kits: Arc::clone(&deps.kits),
        })));
        let seqs: Vec<u64> = (0..3).map(|_| paused_through(&log)).collect();
        assert_eq!(seqs, [1, 2, 3]);
        let (handle, secret) = on_a_socket(&state, &daemon.project.repo.path).await;
        let mut socket = open(handle.info.port, &secret).await;

        let answer = call(&mut socket, 1, "subscribe", &json!({ "from_seq": 1 })).await;
        assert_eq!(answer, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
        for seq in [2, 3] {
            let note = next(&mut socket).await;
            assert_eq!(note["method"], "event", "{note}");
            assert_eq!(note["params"]["event"]["seq"], seq, "{note}");
            farik_protocol::rpc::rpc_notification_from_value(&note)
                .unwrap_or_else(|errors| panic!("{note}: {errors:?}"));
        }

        let appended = paused_through(&other);
        let note = next(&mut socket).await;
        assert_eq!(note["params"]["event"]["seq"], appended, "{note}");
        assert_eq!(note["params"]["event"]["kind"], "team.paused", "{note}");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn closes_the_socket_when_the_log_cannot_be_read() {
        let daemon = TestDaemon::new("rpc-unreadable", |_| {});
        let path = daemon.project.repo.path.join(".farik/local/unreadable.db");
        let log = Arc::new(open_event_log(&path, at()).expect("the log opens"));
        let deps = &daemon.project.deps;
        let state = Arc::new(DaemonState::new(Arc::new(ToolDeps {
            log,
            projections: Arc::clone(&deps.projections),
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: daemon.project.repo.adapter(),
            clock: Arc::clone(&deps.clock),
            ids: deps.ids.clone(),
            kits: Arc::clone(&deps.kits),
        })));
        let (handle, secret) = on_a_socket(&state, &daemon.project.repo.path).await;
        let mut socket = open(handle.info.port, &secret).await;
        let answer = call(&mut socket, 1, "subscribe", &json!({ "from_seq": 0 })).await;
        assert_eq!(answer["result"], json!({}), "{answer}");

        rusqlite::Connection::open(&path)
            .expect("a second connection opens")
            .execute_batch("DROP TABLE events")
            .expect("the table is dropped");
        let closed = loop {
            let frame = tokio::time::timeout(BOUND, socket.next())
                .await
                .expect("the socket closes within the bound");
            match frame {
                Some(Ok(WsMessage::Close(frame))) => break frame,
                Some(Ok(WsMessage::Text(text))) => panic!("the socket still talks: {text}"),
                Some(Ok(_)) => {}
                other => panic!("the socket ended without a close frame: {other:?}"),
            }
        };
        let closed = closed.expect("the close frame has a code");
        assert_eq!(u16::from(closed.code), 1011);
        assert_eq!(closed.reason.as_str(), "farik could not read its event log");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_a_command_like_post_command() {
        let harness = Harness::new("rpc-command", |_| {});
        let (handle, mut socket) = driven(&harness).await;
        let pause = json!({ "command": "team_pause", "body": {} });
        let posted = |command: Value| {
            let router = router(Arc::clone(&harness.daemon), TOKEN, CancellationToken::new());
            async move {
                let answer = router
                    .oneshot(
                        Request::post("/command")
                            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(command.to_string()))
                            .expect("a request is built"),
                    )
                    .await
                    .expect("the router answers");
                serde_json::from_str::<Value>(&body_text(answer).await).expect("JSON")
            }
        };

        let answer = call(&mut socket, 1, "command", &json!({ "command": pause })).await;
        assert_eq!(answer["id"], 1, "{answer}");
        let reply = answer["result"].clone();
        conforms(&reply, "commandReply");
        let paused = harness.events(&[EventKind::TeamPaused]);
        assert_eq!(paused.len(), 1);
        assert_eq!(reply["events"], json!([paused[0].envelope.seq]), "{reply}");

        // The same command twice is refused, and both routes refuse it in the same words.
        let again = call(&mut socket, 2, "command", &json!({ "command": pause })).await;
        let posted_again = posted(pause.clone()).await;
        assert_eq!(again["result"], posted_again, "{again}");
        assert_eq!(posted_again["error"]["kind"], "refused", "{posted_again}");

        // A done command answers alike on both routes, but for the event it appended.
        let resumed = posted(json!({ "command": "team_resume", "body": {} })).await;
        let posted_pause = posted(pause.clone()).await;
        assert_eq!(reply["said"], posted_pause["said"], "{posted_pause}");
        assert_eq!(
            resumed["events"].as_array().map(Vec::len),
            Some(1),
            "{resumed}"
        );

        // A command that is not one is `invalid` on both.
        let wrong = json!({ "command": "team_pause", "body": { "why": 1 } });
        let invalid = call(&mut socket, 3, "command", &json!({ "command": wrong })).await;
        assert_eq!(invalid["result"], posted(wrong).await, "{invalid}");
        assert_eq!(invalid["result"]["error"]["kind"], "invalid", "{invalid}");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_the_task_with_its_plan() {
        let harness = Harness::new("rpc-design-plan", |_| {});
        harness.file("FRK-1", "in_progress", |_| {});
        let (handle, mut socket) = driven(&harness).await;
        let mut id = 0;
        let mut plan_of = async |socket: &mut Socket| {
            id += 1;
            let got = query(
                socket,
                id,
                "task.get",
                &json!({ "task_id": "FRK-1" }),
                "taskGetResult",
            )
            .await;
            got["design_plan"].clone()
        };
        let record = |kind: &str, body: Value| {
            harness.project.record("FRK-1", kind, &body);
        };

        assert_eq!(plan_of(&mut socket).await, Value::Null);
        record("design_plan.proposed", json!({ "plan": "The first plan." }));
        assert_eq!(
            plan_of(&mut socket).await,
            json!({ "plan": "The first plan.", "state": "proposed" })
        );
        record("design_plan.returned", json!({ "reason": "Say more." }));
        assert_eq!(
            plan_of(&mut socket).await,
            json!({ "plan": "The first plan.", "state": "returned", "reason": "Say more." })
        );
        record(
            "design_plan.proposed",
            json!({ "plan": "The second plan." }),
        );
        assert_eq!(
            plan_of(&mut socket).await,
            json!({ "plan": "The second plan.", "state": "proposed" })
        );
        record("design_plan.approved", json!({ "reason": "Go ahead." }));
        assert_eq!(
            plan_of(&mut socket).await,
            json!({ "plan": "The second plan.", "state": "approved", "reason": "Go ahead." })
        );
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    /// FRK-2, `dev-a`'s change to `site/style.css` on a team with the Designer, in `verifying`;
    /// and FRK-1, a task with no change.
    fn a_ui_change(name: &str) -> Harness {
        let mut harness = Harness::new(name, crate::tools::fixtures::browsing);
        harness.previews = Arc::new(crate::preview::fixtures::FakePreviews::ready());
        harness.file("FRK-1", "in_progress", |_| {});
        harness.verifying_a_ui_change("FRK-2");
        harness
    }

    /// A `page.checked` of `task`'s page at `width` and `theme`, its screenshot `file`.
    fn checked(harness: &Harness, task: &str, width: &str, theme: &str, file: &str) {
        harness.project.record(
            task,
            "page.checked",
            &json!({
                "width": width, "theme": theme, "path": "/", "screenshot": file,
                "violations": [{
                    "rule": "color-contrast", "impact": "serious", "target": "h1",
                    "help": "Elements must meet minimum color contrast ratio thresholds"
                }]
            }),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn lists_the_design_review_state_with_the_tasks() {
        let harness = a_ui_change("rpc-board-design-review");
        let (handle, mut socket) = driven(&harness).await;
        let listed = query(&mut socket, 1, "tasks.list", &json!({}), "tasksListResult").await;
        let row = |id: &str| {
            listed["tasks"]
                .as_array()
                .and_then(|tasks| tasks.iter().find(|task| task["task_id"] == id))
                .cloned()
                .unwrap_or_else(|| panic!("{id} is listed: {listed}"))
        };
        // A UI change in review says where its design review stands, so the board need not ask.
        assert_eq!(row("FRK-2")["design_review_state"], "waiting", "{listed}");
        assert_eq!(row("FRK-1").get("design_review_state"), None, "{listed}");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn lists_the_tasks_when_a_design_review_cannot_be_read() {
        let harness = a_ui_change("rpc-board-design-review-unread");
        let contract = harness
            .project
            .deps
            .files
            .root()
            .join(".farik/contracts/FRK-2.yaml");
        std::fs::write(&contract, "not: [a contract\n").expect("written");
        let (handle, mut socket) = driven(&harness).await;
        let listed = query(&mut socket, 1, "tasks.list", &json!({}), "tasksListResult").await;
        // One unreadable review leaves its card without a state; the board still shows.
        let ids: Vec<&Value> = listed["tasks"]
            .as_array()
            .map(|tasks| tasks.iter().map(|task| &task["task_id"]).collect())
            .unwrap_or_default();
        assert_eq!(ids, vec!["FRK-1", "FRK-2"], "{listed}");
        assert_eq!(
            listed["tasks"][1].get("design_review_state"),
            None,
            "{listed}"
        );
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    /// Whether `tasks.list` puts each of `tasks` in the Backlog, in order.
    fn backlog_of(harness: &Harness, tasks: &[&str]) -> Vec<Value> {
        let listed = crate::daemon::gates::tests::query(
            &harness.daemon,
            "tasks.list",
            &json!({}),
            "tasksListResult",
        );
        tasks
            .iter()
            .map(|id| {
                listed["tasks"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["task_id"] == *id))
                    .unwrap_or_else(|| panic!("{id} is listed: {listed}"))["backlog"]
                    .clone()
            })
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn marks_backlog_rows() {
        let on = Harness::new("rpc-backlog-on", |wire| {
            wire["policy"]["plan_in_sprints"] = json!(true);
        });
        on.ready("FRK-1");
        // Under way since before the switch: no mark, so it keeps its lane.
        on.file("FRK-2", "in_progress", |_| {});
        assert_eq!(
            backlog_of(&on, &["FRK-1", "FRK-2"]),
            [json!(true), json!(false)]
        );
        on.open_sprint("S1", &["FRK-1"]);
        assert_eq!(
            backlog_of(&on, &["FRK-1", "FRK-2"]),
            [json!(false), json!(false)]
        );

        let off = Harness::new("rpc-backlog-off", |_| {});
        off.ready("FRK-1");
        off.file("FRK-2", "in_progress", |_| {});
        assert_eq!(
            backlog_of(&off, &["FRK-1", "FRK-2"]),
            [json!(false), json!(false)]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one walk through the design review's queries"
    )]
    async fn answers_the_task_with_its_review() {
        use base64::Engine as _;

        let harness = a_ui_change("rpc-design-review");
        let (handle, mut socket) = driven(&harness).await;
        let get = async |socket: &mut Socket, id: u64, task: &str| {
            query(
                socket,
                id,
                "task.get",
                &json!({ "task_id": task }),
                "taskGetResult",
            )
            .await
        };

        let plain = get(&mut socket, 1, "FRK-1").await;
        assert_eq!(plain["ui_change"], false, "{plain}");
        assert_eq!(plain["design_review"], Value::Null, "{plain}");
        let waiting = get(&mut socket, 2, "FRK-2").await;
        assert_eq!(waiting["ui_change"], true, "{waiting}");
        assert_eq!(
            waiting["design_review"],
            json!({ "state": "waiting", "checks": [] })
        );

        checked(&harness, "FRK-2", "phone", "dark", "s-1-phone-dark.png");
        let violation = json!({
            "rule": "color-contrast", "impact": "serious", "target": "h1",
            "help": "Elements must meet minimum color contrast ratio thresholds"
        });
        let at = crate::tools::fixtures::at();
        harness.project.record_by(
            Some("iris"),
            at,
            "FRK-2",
            "design_review.recorded",
            &json!({
                "pass": false,
                "reasons": "The heading is too faint.",
                "checks": [{ "width": "phone", "theme": "dark", "violations": [violation] }]
            }),
        );
        let failed = get(&mut socket, 3, "FRK-2").await;
        assert_eq!(
            failed["design_review"],
            json!({
                "state": "failed",
                "reasons": "The heading is too faint.",
                "checks": [{ "width": "phone", "theme": "dark", "violations": [violation] }]
            })
        );
        // Every design review of the task, oldest first, whoever recorded it.
        assert_eq!(plain["design_reviews"], json!([]), "{plain}");
        let first = json!({
            "agent_id": "iris", "pass": false, "reasons": "The heading is too faint.",
            "recorded_at": "2026-09-22T12:00:00Z"
        });
        assert_eq!(failed["design_reviews"], json!([first]), "{failed}");
        harness.project.record_by(
            Some("iris"),
            at + chrono::Duration::hours(1),
            "FRK-2",
            "design_review.recorded",
            &json!({ "pass": true, "reasons": "Darker now.", "checks": [] }),
        );
        let both = get(&mut socket, 6, "FRK-2").await;
        assert_eq!(
            both["design_reviews"],
            json!([first, {
                "agent_id": "iris", "pass": true, "reasons": "Darker now.",
                "recorded_at": "2026-09-22T13:00:00Z"
            }]),
            "{both}"
        );

        let png = b"\x89PNG\r\n\x1a\nthe phone in the dark";
        let folder = harness
            .project
            .repo
            .path
            .join(".farik/local/screenshots/FRK-2");
        std::fs::create_dir_all(&folder).expect("made");
        std::fs::write(folder.join("s-1-phone-dark.png"), png).expect("written");
        let shot = query(
            &mut socket,
            4,
            "task.screenshot",
            &json!({ "task_id": "FRK-2", "file": "s-1-phone-dark.png" }),
            "taskScreenshotResult",
        )
        .await;
        assert_eq!(
            shot["png_base64"],
            base64::engine::general_purpose::STANDARD.encode(png)
        );

        let defaults = query(
            &mut socket,
            5,
            "settings.defaults",
            &json!({}),
            "settingsDefaultsResult",
        )
        .await;
        assert_eq!(
            defaults["ui_paths"],
            json!(farik_core::governor::team_rules::DEFAULT_UI_PATHS)
        );
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_screenshot_the_task_did_not_take() {
        let harness = a_ui_change("rpc-screenshot-refused");
        checked(&harness, "FRK-1", "phone", "light", "s-1-phone-light.png");
        let root = &harness.project.repo.path;
        for task in ["FRK-1", "FRK-2"] {
            let folder = root.join(".farik/local/screenshots").join(task);
            std::fs::create_dir_all(&folder).expect("made");
            std::fs::write(folder.join("s-1-phone-light.png"), b"png").expect("written");
        }
        std::fs::write(root.join(".farik/local/screenshots/x.png"), b"png").expect("written");
        let (handle, mut socket) = driven(&harness).await;

        for (id, file) in [(1, "../x.png"), (2, "s-1-phone-light.png")] {
            let answer = call(
                &mut socket,
                id,
                "query",
                &json!({ "name": "task.screenshot", "params": { "task_id": "FRK-2", "file": file } }),
            )
            .await;
            assert_eq!(
                answer["error"]["code"],
                super::NOT_FOUND,
                "{file}: {answer}"
            );
            conforms(&answer, "rpcFailure");
        }
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one walk through every query the browser makes"
    )]
    async fn answers_the_queries() {
        let harness = Harness::new("rpc-queries", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        harness.file("FRK-2", "draft", |_| {});
        let (handle, mut socket) = driven(&harness).await;

        let listed = query(&mut socket, 1, "tasks.list", &json!({}), "tasksListResult").await;
        assert_eq!(listed["tasks"][0]["task_id"], "FRK-1", "{listed}");
        assert_eq!(listed["tasks"][0]["status"], "refining", "{listed}");

        let got = query(
            &mut socket,
            2,
            "task.get",
            &json!({ "task_id": "FRK-1" }),
            "taskGetResult",
        )
        .await;
        assert_eq!(got["task"], listed["tasks"][0], "{got}");
        let missing = call(
            &mut socket,
            3,
            "query",
            &json!({ "name": "task.get", "params": { "task_id": "FRK-99" } }),
        )
        .await;
        assert_eq!(missing["id"], 3, "{missing}");
        assert_eq!(missing["error"]["code"], -32002, "{missing}");
        conforms(&missing, "rpcFailure");

        let team = query(&mut socket, 4, "team.get", &json!({}), "teamGetResult").await;
        let file = serde_json::to_value(harness.project.deps.files.read_team().expect("the team"))
            .expect("the team is JSON");
        assert_eq!(team["team"], file, "{team}");

        let all = harness.project.events(&[]);
        let since = query(
            &mut socket,
            5,
            "events.since",
            &json!({ "after_seq": 0, "limit": 500 }),
            "eventsSinceResult",
        )
        .await;
        let seqs: Vec<&Value> = since["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|e| &e["seq"])
            .collect();
        assert_eq!(seqs.len(), all.len(), "{since}");
        assert_eq!(since["events"][0], event_to_value(&all[0]), "{since}");
        let one = query(
            &mut socket,
            6,
            "events.since",
            &json!({ "after_seq": 0, "limit": 1 }),
            "eventsSinceResult",
        )
        .await;
        assert_eq!(one["events"], json!([event_to_value(&all[0])]), "{one}");
        let rest = query(
            &mut socket,
            7,
            "events.since",
            &json!({ "after_seq": 1, "limit": 500 }),
            "eventsSinceResult",
        )
        .await;
        assert_eq!(rest["events"][0]["seq"], 2, "{rest}");
        assert_eq!(
            rest["events"].as_array().map(Vec::len),
            Some(all.len() - 1),
            "{rest}"
        );

        let status = query(
            &mut socket,
            8,
            "serve.status",
            &json!({}),
            "serveStatusResult",
        )
        .await;
        assert_eq!(
            status,
            json!({
                "project_root": harness.project.repo.path.display().to_string(),
                "paused": false,
                "credential": null,
                "port": handle.info.port,
                "take_on_error": null,
                "setup_pending": false,
            })
        );
        let pause = json!({ "command": { "command": "team_pause", "body": {} } });
        call(&mut socket, 9, "command", &pause).await;
        let status = query(
            &mut socket,
            10,
            "serve.status",
            &json!({}),
            "serveStatusResult",
        )
        .await;
        assert_eq!(status["paused"], true, "{status}");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_json_rpc_errors() {
        let daemon = TestDaemon::new("rpc-errors", |_| {});
        let (handle, secret) = on_a_socket(&daemon.state, &daemon.project.repo.path).await;
        let mut socket = open(handle.info.port, &secret).await;

        send_text(&mut socket, "not json".to_string()).await;
        let parse = next(&mut socket).await;
        assert_eq!(parse["error"]["code"], -32700, "{parse}");
        assert_eq!(parse["id"], Value::Null, "{parse}");
        conforms(&parse, "rpcFailure");

        let nope = call(&mut socket, 7, "nope", &json!({})).await;
        assert_eq!(nope["error"]["code"], -32601, "{nope}");
        assert_eq!(nope["id"], 7, "{nope}");
        conforms(&nope, "rpcFailure");

        let bare = call(&mut socket, 8, "subscribe", &json!({})).await;
        assert_eq!(bare["error"]["code"], -32602, "{bare}");
        assert_eq!(bare["id"], 8, "{bare}");
        conforms(&bare, "rpcFailure");

        let unknown = call(
            &mut socket,
            9,
            "query",
            &json!({ "name": "secrets.get", "params": {} }),
        )
        .await;
        assert_eq!(unknown["error"]["code"], -32001, "{unknown}");
        assert_eq!(unknown["id"], 9, "{unknown}");

        send_text(
            &mut socket,
            json!({ "jsonrpc": "1.0", "id": 10, "method": "unsubscribe" }).to_string(),
        )
        .await;
        let invalid = next(&mut socket).await;
        assert_eq!(invalid["error"]["code"], -32600, "{invalid}");
        assert_eq!(invalid["id"], 10, "{invalid}");

        // The socket still answers after every error.
        let answer = call(&mut socket, 11, "unsubscribe", &json!({})).await;
        assert_eq!(answer["result"], json!({}), "{answer}");
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_to_stop_farik_from_the_browser() {
        let daemon = TestDaemon::new("rpc-stop", |_| {});
        let handled: Arc<std::sync::Mutex<Vec<Command>>> = Arc::default();
        let seen = Arc::clone(&handled);
        daemon.state.set_command_handler(Arc::new(move |command| {
            seen.lock().expect("the list").push(command);
            Box::pin(async {
                Ok(CommandReport {
                    said: "handled".to_string(),
                    events: Vec::new(),
                })
            })
        }));
        let (handle, secret) = on_a_socket(&daemon.state, &daemon.project.repo.path).await;
        let mut socket = open(handle.info.port, &secret).await;

        let stop = json!({ "command": { "command": "run_stop", "body": {} } });
        let answer = call(&mut socket, 1, "command", &stop).await;
        assert_eq!(
            answer,
            json!({
                "jsonrpc": "2.0", "id": 1,
                "error": {
                    "code": -32003,
                    "message": "stopping Farik is done where it runs; pause the team instead"
                }
            })
        );
        assert!(handled.lock().expect("the list").is_empty());

        // Any other command reaches the handler.
        let pause = json!({ "command": { "command": "team_pause", "body": {} } });
        let answer = call(&mut socket, 2, "command", &pause).await;
        assert_eq!(answer["result"]["said"], "handled", "{answer}");
        assert_eq!(*handled.lock().expect("the list"), [Command::TeamPause]);
        drop(socket);
        handle.shutdown().await.expect("the daemon stops");
    }

    // Setup mode: a daemon with no project, answering the wizard through a fake host.

    /// A setup host that records what it was asked and hands out paths under `home`.
    struct FakeHost {
        home: PathBuf,
        path: String,
        calls: std::sync::Mutex<Vec<Value>>,
        account: std::sync::Mutex<Option<(CredentialKind, Source)>>,
    }

    impl SetupHost for FakeHost {
        fn open(&self, path: &str, no_sandbox: bool) -> Result<PathBuf, SetupError> {
            self.calls
                .lock()
                .expect("the calls")
                .push(json!({ "open": path, "no_sandbox": no_sandbox }));
            if path == "busy" {
                return Err(SetupError::Refused(
                    "another farik is already running this project".to_string(),
                ));
            }
            Ok(self.home.join(path))
        }

        fn create(
            &self,
            parent: &str,
            name: &str,
            description: &str,
            no_sandbox: bool,
        ) -> Result<PathBuf, SetupError> {
            self.calls.lock().expect("the calls").push(json!({
                "create": [parent, name, description], "no_sandbox": no_sandbox
            }));
            Ok(self.home.join(parent).join(name))
        }

        fn connect(
            &self,
            kind: CredentialKind,
            secret: &str,
        ) -> Result<(Source, bool), SetupError> {
            self.calls
                .lock()
                .expect("the calls")
                .push(json!({ "connect": kind }));
            credential_of_kind(kind, secret)
                .map(|_| (Source::File, false))
                .map_err(SetupError::Refused)
        }

        fn home(&self) -> PathBuf {
            self.home.clone()
        }

        fn env(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::from([("PATH".to_string(), self.path.clone())])
        }

        fn account(&self) -> Option<(CredentialKind, Source)> {
            *self.account.lock().expect("the account")
        }
    }

    /// A daemon in setup mode over `home`, whose programs are looked for on `path`.
    fn in_setup(home: &std::path::Path, path: &str) -> (Arc<DaemonState>, Arc<FakeHost>) {
        let host = Arc::new(FakeHost {
            home: home.to_path_buf(),
            path: path.to_string(),
            calls: std::sync::Mutex::default(),
            account: std::sync::Mutex::default(),
        });
        let web = WebState {
            codes: ConnectCodes::default(),
            sessions: BrowserSessions::open(None).expect("the sessions open"),
            project_root: PathBuf::new(),
            credential: None,
            port: PORT,
            clock: Arc::new(FixedClock::new(now())),
            take_on_error: std::sync::Mutex::default(),
            stores: Vec::new(),
            env: std::collections::BTreeMap::new(),
            in_use: None,
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        };
        let state = Arc::new(DaemonState::setup(
            Arc::clone(&host) as Arc<dyn SetupHost>,
            web,
        ));
        (state, host)
    }

    /// The reply frame to `method` with `params`, as text, and as JSON.
    async fn asked(state: &Arc<DaemonState>, method: &str, params: &Value) -> (String, Value) {
        let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let reply = super::answer(state, &frame.to_string(), &mut None).await;
        (reply.to_string(), reply)
    }

    async fn setup_query(state: &Arc<DaemonState>, name: &str, params: &Value) -> Value {
        asked(state, "query", &json!({ "name": name, "params": params }))
            .await
            .1
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn lists_only_folders_inside_home() {
        let home = scratch("setup-home");
        let outside = scratch("setup-outside");
        for folder in ["a/.git", "b", ".hidden"] {
            std::fs::create_dir_all(home.join(folder)).expect("the folder is made");
        }
        std::fs::write(home.join("f.txt"), "a file").expect("written");
        std::os::unix::fs::symlink(&outside, home.join("out")).expect("the link is made");
        let (state, _) = in_setup(&home, "");

        let listed = setup_query(&state, "folders.list", &json!({})).await;
        conforms(&listed["result"], "foldersListResult");
        assert_eq!(
            listed["result"],
            json!({
                "path": "", "parent": null,
                "entries": [{ "name": "a", "git": true }, { "name": "b", "git": false }]
            })
        );
        let inside = setup_query(&state, "folders.list", &json!({ "path": "a" })).await;
        assert_eq!(
            inside["result"],
            json!({ "path": "a", "parent": "", "entries": [] })
        );
        for path in ["../", "out"] {
            let refused = setup_query(&state, "folders.list", &json!({ "path": path })).await;
            assert_eq!(
                refused["error"],
                json!({ "code": -32005, "message": "that folder is outside your home folder" }),
                "{path}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn passes_open_and_create_to_the_host_and_refuses_in_words() {
        let home = scratch("setup-open");
        let (state, host) = in_setup(&home, "");
        let description = "A bakery's web shop, with orders taken online.";

        let (_, opened) = asked(
            &state,
            "project.open",
            &json!({ "path": "code/a", "no_sandbox": true }),
        )
        .await;
        conforms(&opened["result"], "projectOpenResult");
        assert_eq!(
            opened["result"]["project_root"],
            home.join("code/a").display().to_string()
        );
        let (_, created) = asked(
            &state,
            "project.create",
            &json!({
                "parent": "code", "name": "bakery",
                "description": description, "no_sandbox": false
            }),
        )
        .await;
        conforms(&created["result"], "projectCreateResult");
        assert_eq!(
            created["result"]["project_root"],
            home.join("code/bakery").display().to_string()
        );
        let (_, busy) = asked(
            &state,
            "project.open",
            &json!({ "path": "busy", "no_sandbox": false }),
        )
        .await;
        assert_eq!(
            busy["error"],
            json!({ "code": -32005, "message": "another farik is already running this project" })
        );
        conforms(&busy, "rpcFailure");
        assert_eq!(
            *host.calls.lock().expect("the calls"),
            [
                json!({ "open": "code/a", "no_sandbox": true }),
                json!({ "create": ["code", "bakery", description], "no_sandbox": false }),
                json!({ "open": "busy", "no_sandbox": false }),
            ]
        );
    }

    /// Writes an executable `sh` script called `name` into `bin`. A child process writes it, so
    /// that the write descriptor never lives in this process: another test thread's fork would
    /// inherit it until its exec, and running the script meanwhile fails with "text file busy".
    fn script(bin: &std::path::Path, name: &str, body: &str) {
        let staged = bin.join(format!(".{name}.new"));
        let written = std::process::Command::new("sh")
            .args([
                "-c",
                "printf '%s\\n' \"$2\" > \"$1\" && chmod 755 \"$1\"",
                "sh",
            ])
            .arg(&staged)
            .arg(format!("#!/bin/sh\n{body}"))
            .status()
            .expect("sh runs");
        assert!(written.success());
        std::fs::rename(&staged, bin.join(name)).expect("renamed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answers_the_computer_check() {
        let bin = scratch("setup-computer");
        script(&bin, "claude", "echo '2.1.300 (Claude Code)'");
        script(&bin, "git", "echo 'git version 2.43.0'");
        let recorded = bin.join("recorded");
        script(
            &bin,
            "docker",
            &format!(
                "case \"$1\" in\n  version) exit 0 ;;\n  image) exit 1 ;;\n  build) echo \"$@\" > '{0}/args'; cat > '{0}/stdin' ;;\nesac",
                recorded.display()
            ),
        );
        std::fs::create_dir_all(&recorded).expect("the folder is made");
        let (state, _) = in_setup(&bin, &format!("{}:/usr/bin:/bin", bin.display()));

        let checked = setup_query(&state, "computer.check", &json!({})).await;
        conforms(&checked["result"], "computerCheckResult");
        assert_eq!(
            checked["result"]["claude"],
            json!({ "state": "ready", "version": "2.1.300" })
        );
        assert_eq!(checked["result"]["git"]["state"], "ready", "{checked}");
        assert_eq!(checked["result"]["docker"]["state"], "ready", "{checked}");
        assert_eq!(
            checked["result"]["sandbox_image"]["state"], "missing",
            "{checked}"
        );

        script(&bin, "claude", "echo '2.1.200 (Claude Code)'");
        let old = setup_query(&state, "computer.check", &json!({})).await;
        assert_eq!(
            old["result"]["claude"],
            json!({ "state": "too_old", "version": "2.1.200" })
        );

        let (_, built) = asked(&state, "sandbox.build", &json!({})).await;
        assert_eq!(
            built["result"],
            json!({ "image": crate::SANDBOX_IMAGE }),
            "{built}"
        );
        assert_eq!(
            std::fs::read_to_string(recorded.join("args")).expect("docker was run"),
            format!("build -t {} -\n", crate::SANDBOX_IMAGE)
        );
        assert_eq!(
            std::fs::read_to_string(recorded.join("stdin")).expect("docker was fed"),
            include_str!("../../sandbox/Dockerfile")
        );

        // Docker is there, but its daemon does not answer.
        script(&bin, "docker", "exit 1");
        let stopped = setup_query(&state, "computer.check", &json!({})).await;
        assert_eq!(
            stopped["result"]["docker"],
            json!({ "state": "not_running" }),
            "{stopped}"
        );
        assert_eq!(
            stopped["result"]["sandbox_image"]["state"], "missing",
            "{stopped}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pulls_the_designer_browser_on_request() {
        let bin = scratch("setup-browser-pull");
        let recorded = bin.join("recorded");
        std::fs::create_dir_all(&recorded).expect("the folder is made");
        script(
            &bin,
            "docker",
            &format!("echo \"$@\" > '{}/args'", recorded.display()),
        );
        let (state, _) = in_setup(&bin, &format!("{}:/usr/bin:/bin", bin.display()));
        let image = farik_roles::builtin_connector("playwright")
            .expect("shipped")
            .image;

        let (_, pulled) = asked(&state, "browser.pull", &json!({})).await;
        conforms(&pulled["result"], "browserPullResult");
        assert_eq!(pulled["result"], json!({ "image": image }), "{pulled}");
        assert_eq!(
            std::fs::read_to_string(recorded.join("args")).expect("docker was run"),
            format!("pull --quiet {image}\n")
        );

        script(&bin, "docker", "echo 'pull access denied' >&2; exit 1");
        let (_, refused) = asked(&state, "browser.pull", &json!({})).await;
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("pull access denied")),
            "{refused}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answers_503_and_no_project_in_setup_mode() {
        let home = scratch("setup-503");
        let (state, host) = in_setup(&home, "");
        for path in [
            "/command",
            "/hook/pre-tool-use",
            "/hook/post-tool-use",
            "/mcp",
        ] {
            let request = Request::post(path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "command": "run_stop", "body": {} }).to_string(),
                ))
                .expect("a request is built");
            let answer = send(&state, request).await;
            assert_eq!(answer.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
            assert_eq!(
                body_text(answer).await,
                "farik has no project yet",
                "{path}"
            );
        }

        let tasks = setup_query(&state, "tasks.list", &json!({})).await;
        assert_eq!(tasks["error"]["code"], -32004, "{tasks}");
        conforms(&tasks, "rpcFailure");

        let status = setup_query(&state, "serve.status", &json!({})).await;
        conforms(&status["result"], "serveStatusResult");
        assert_eq!(
            status["result"],
            json!({
                "project_root": null, "paused": false, "credential": null,
                "port": PORT, "take_on_error": null, "setup_pending": false,
            })
        );
        let account = setup_query(&state, "account.status", &json!({})).await;
        assert_eq!(
            account["result"],
            json!({ "provider": null, "kind": null, "source": null })
        );

        // The credential is read afresh, and a failed take-on is told.
        *host.account.lock().expect("the account") =
            Some((CredentialKind::SubscriptionToken, Source::Keychain));
        *state
            .web()
            .expect("the browser routes are on")
            .take_on_error
            .lock()
            .expect("the error") = Some("the prices file cannot be read".to_string());
        let status = setup_query(&state, "serve.status", &json!({})).await;
        assert_eq!(
            status["result"]["credential"], "subscription_token",
            "{status}"
        );
        assert_eq!(
            status["result"]["take_on_error"], "the prices file cannot be read",
            "{status}"
        );
        let account = setup_query(&state, "account.status", &json!({})).await;
        conforms(&account["result"], "accountStatusResult");
        assert_eq!(
            account["result"],
            json!({ "provider": "anthropic", "kind": "subscription_token", "source": "keychain" })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn never_echoes_the_secret() {
        let home = scratch("setup-secret");
        let (state, _) = in_setup(&home, "");
        let secret = "sk-ant-oat01-never-to-be-echoed";

        let (text, stored) = asked(
            &state,
            "account.connect",
            &json!({ "kind": "subscription_token", "secret": secret }),
        )
        .await;
        conforms(&stored["result"], "accountConnectResult");
        assert_eq!(
            stored["result"],
            json!({ "stored_in": "file", "taking_on": false }),
            "{text}"
        );
        let (text, malformed) =
            asked(&state, "account.connect", &json!({ "secret": secret })).await;
        assert_eq!(malformed["error"]["code"], -32602, "{text}");
        assert!(!text.contains(secret), "{text}");
        let (text, refused) = asked(
            &state,
            "account.connect",
            &json!({ "kind": "api_key", "secret": secret }),
        )
        .await;
        assert_eq!(refused["error"]["code"], -32005, "{text}");
        assert!(!text.contains(secret), "{text}");
    }
}
