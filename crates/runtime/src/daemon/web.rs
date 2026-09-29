//! The browser's way in (`docs/SPEC.md` section 8.6): a one-time code from the terminal, traded
//! at `POST /connect` for a session cookie, on a daemon that checks every browser request's
//! `Origin` and `Host`.

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
use farik_core::contract::TaskId;
use farik_protocol::command::{Command, command_from_value, reply_to_value};
use farik_protocol::event::event_to_value;
use farik_protocol::rpc::{QueryName, rpc_request_from_value};
use farik_store::EventQuery;
use farik_store::projections::TaskProjection;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use super::{DaemonError, DaemonState, hex, random_token, same_token};
use crate::claude::CredentialKind;
use crate::locked;

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
/// cookie to every port of a host, and a page on another local port is another origin.
fn from_own_page(headers: &HeaderMap, port: u16) -> bool {
    own_host(headers, port) && named(headers, header::ORIGIN) == Some(own_origin(port).as_str())
}

/// Whether `Host` is `127.0.0.1:<port>`, exactly: what a navigation, which sends no `Origin`, is
/// checked by.
pub(super) fn own_host(headers: &HeaderMap, port: u16) -> bool {
    named(headers, header::HOST) == Some(format!("127.0.0.1:{port}").as_str())
}

fn own_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
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
    if !from_own_page(&headers, web.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let code = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|value| value.get("code")?.as_str().map(str::to_string));
    if !code.is_some_and(|code| web.codes.redeem(&code)) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": USED_LINK })),
        )
            .into_response();
    }
    match web.sessions.issue(state.deps().clock.now()) {
        Ok(secret) => (
            StatusCode::NO_CONTENT,
            [(
                header::SET_COOKIE,
                format!(
                    "farik_session={secret}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
                    SESSION_DAYS * 24 * 60 * 60
                ),
            )],
        )
            .into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
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
    if !own_host(&headers, web.port) || origin.is_some_and(|origin| origin != own_origin(web.port))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = state.deps().clock.now();
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
    if !from_own_page(&headers, web.port) {
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
    if !from_own_page(&headers, web.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = state.deps().clock.now();
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
    let reader = Arc::clone(state);
    let read = tokio::task::spawn_blocking(move || reader.deps().log.read(&query)).await;
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
struct Failure {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl Failure {
    fn new(code: i64, message: impl Into<String>) -> Self {
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
const INTERNAL_ERROR: i64 = -32603;
const UNKNOWN_QUERY: i64 = -32001;
const NOT_FOUND: i64 = -32002;
const REFUSED_HERE: i64 = -32003;

/// The response to one text frame. A `subscribe` sets `sent` to its `from_seq`, and an
/// `unsubscribe` clears it.
async fn answer(state: &Arc<DaemonState>, text: &str, sent: &mut Option<u64>) -> Value {
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
    if !["subscribe", "unsubscribe", "command", "query"].contains(&method) {
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
                data: Some(json!(errors)),
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
        _ => {
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
    let deps = state.deps();
    let internal = |error: &dyn std::fmt::Display| Failure::new(INTERNAL_ERROR, error.to_string());
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
            Ok(json!({ "tasks": board.iter().map(task_wire).collect::<Vec<_>>() }))
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
                Some(task) => Ok(json!({ "task": task_wire(&task) })),
                None => Err(missing()),
            }
        }
        "team.get" => {
            let team = deps.files.read_team().map_err(|error| internal(&error))?;
            let team = serde_json::to_value(team).map_err(|error| internal(&error))?;
            Ok(json!({ "team": team }))
        }
        "serve.status" => {
            let web = state
                .web()
                .ok_or_else(|| Failure::new(INTERNAL_ERROR, "the browser routes are off"))?;
            let paused = crate::pause::paused(&deps.log).map_err(|error| internal(&error))?;
            Ok(json!({
                "project_root": web.project_root.display().to_string(),
                "paused": paused,
                "credential": web.credential,
                "port": web.port,
            }))
        }
        _ => Err(Failure::new(
            UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// One board row as the RPC schema's `taskProjection`: an optional field is left out, not null,
/// when the projection has none.
fn task_wire(task: &TaskProjection) -> Value {
    let mut wire = json!({
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
    use crate::daemon::app::fixtures::{Fixture, Unbuilt};
    use crate::daemon::fixtures::TestDaemon;
    use crate::daemon::{
        DaemonConfig, DaemonHandle, DaemonState, PortChoice, router, router_serving, serve,
    };
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{CommandReport, command_handler};
    use crate::tools::ToolDeps;
    use crate::tools::fixtures::at;

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
        let daemon = TestDaemon::new(name, |_| {});
        let codes = ConnectCodes::default();
        let code = codes.issue().expect("a code");
        assert!(daemon.state.set_web(WebState {
            codes,
            sessions: BrowserSessions::open(None).expect("the sessions open"),
            project_root: daemon.project.repo.path.clone(),
            credential: None,
            port: PORT,
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
        let now = daemon.state.deps().clock.now();
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
            assert!(
                header_of(&answer, &header::CONTENT_TYPE).starts_with("text/html"),
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
        assert_eq!(header_of(&asset, &header::CONTENT_TYPE), "text/javascript");
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
            .issue(daemon.state.deps().clock.now())
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
        let now = daemon.state.deps().clock.now();
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
                daemon_file: root.join(".farik/local/daemon.json"),
            },
            Arc::clone(state),
        )
        .await
        .expect("the daemon is up");
        let sessions = BrowserSessions::open(None).expect("the sessions open");
        let secret = sessions.issue(state.deps().clock.now()).expect("a session");
        assert!(state.set_web(WebState {
            codes: ConnectCodes::default(),
            sessions,
            project_root: root.to_path_buf(),
            credential: None,
            port: handle.info.port,
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
            .issue(daemon.state.deps().clock.now() - Duration::days(31))
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
}
