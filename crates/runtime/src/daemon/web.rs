//! The browser's way in (`docs/SPEC.md` section 8.6): a one-time code from the terminal, traded
//! at `POST /connect` for a session cookie, on a daemon that checks every browser request's
//! `Origin` and `Host`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

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
    let named = |name| {
        headers
            .get(name)
            .and_then(|value: &HeaderValue| value.to_str().ok())
    };
    named(header::HOST) == Some(format!("127.0.0.1:{port}").as_str())
        && named(header::ORIGIN) == Some(format!("http://127.0.0.1:{port}").as_str())
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

    use super::{BrowserSessions, ConnectCodes, WebState};
    use crate::daemon::fixtures::TestDaemon;
    use crate::daemon::{DaemonState, router};

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
}
