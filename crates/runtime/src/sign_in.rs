//! Signing an agent in to a connector's service with OAuth (ADR 0033): the grant, the
//! sign-in that makes one, and the refresh and revocation that keep it.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::team::OAuthSettings;
use reqwest::Url;
use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationMetadata, AuthorizationMetadataSource,
    AuthorizationRequest, AuthorizationSession, OAuthHttpClient, OAuthHttpClientError,
    OAuthHttpClientFuture, OAuthHttpRedirectPolicy, OAuthHttpRequest,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::claude::Secret;

/// How long the user has to say yes on the service's page.
pub const SIGN_IN_WINDOW: Duration = Duration::from_secs(600);
/// How long finding out how to sign in, and registering, may take.
pub const SIGN_IN_START: Duration = Duration::from_secs(15);

/// What an agent's sign-in to a service holds, kept in its connector entry.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthGrant {
    /// Who the service says it is, which `iss` must equal.
    pub issuer: String,
    /// The server the token is for (RFC 8707), sent on every token request.
    pub resource: String,
    /// The client this sign-in registered or was given; a client is bound to its issuer.
    pub client_id: String,
    /// Where tokens are asked for.
    pub token_endpoint: String,
    /// Where a token is asked to be forgotten, when the service has one.
    pub revocation_endpoint: Option<String>,
    /// What the session sends.
    pub access_token: Secret,
    /// What gets a new access token, when the service gave one.
    pub refresh_token: Option<Secret>,
    /// When the grant was made or last refreshed.
    pub issued_at: DateTime<Utc>,
    /// When the access token stops working, when the service said.
    pub expires_at: Option<DateTime<Utc>>,
    /// What the service granted.
    pub scopes: Vec<String>,
    /// Whether the service ended the sign-in.
    pub lapsed: bool,
}

impl fmt::Debug for OAuthGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthGrant")
            .field("issuer", &self.issuer)
            .field("resource", &self.resource)
            .field("client_id", &self.client_id)
            .field("access_token", &"***")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "***"))
            .field("expires_at", &self.expires_at)
            .field("lapsed", &self.lapsed)
            .finish_non_exhaustive()
    }
}

/// Why a sign-in did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInError {
    /// The server does not say it can be signed in to.
    NotOffered,
    /// It can, but Farik has no way to register as a client.
    NotSupported,
    /// The service does not do PKCE with S256.
    PkceNotSupported,
    /// The user said no on the service's page.
    Denied(String),
    /// The way back from the service did not match what Farik sent.
    Mismatch,
    /// The user took more than ten minutes.
    TimedOut,
    /// The service ended the sign-in.
    Lapsed,
    /// Anything else, as a sentence that quotes no response body.
    Failed(String),
}

/// The port a pre-registered client's redirect names when the team file gives none.
const DEFAULT_CALLBACK_PORT: u16 = 33418;
/// How long one request to a service may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// The most a service's answer may weigh.
const MAX_BODY: usize = 1024 * 1024;
/// How long a connection to the callback listener may take to say what it wants.
const CONNECTION_READ: Duration = Duration::from_secs(5);

/// Whether Farik may talk to `url`: `https`, or `http` to this computer, which the tests need.
fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => url.host_str().is_some_and(|host| {
            let host = host.trim_start_matches('[').trim_end_matches(']');
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        }),
        _ => false,
    }
}

/// The one HTTP client every request to a service goes through: https, or loopback, at every hop
/// of every redirect. What it refused and the last error status it saw are kept, so that a reason
/// can name the step and the status and quote nothing the service said.
pub(crate) struct Guarded {
    follow: reqwest::Client,
    stop: reqwest::Client,
    refused: Arc<Mutex<Option<String>>>,
    status: Mutex<Option<u16>>,
}

impl Guarded {
    pub(crate) fn new() -> Result<Arc<Guarded>, SignInError> {
        let refused = Arc::new(Mutex::new(None));
        let hop_refused = refused.clone();
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if allowed(attempt.url()) {
                attempt.follow()
            } else {
                let url = attempt.url().to_string();
                *hop_refused
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(format!("{url} is not https"));
                attempt.error("not https")
            }
        });
        let build = |policy| {
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .redirect(policy)
                .build()
                .map_err(|_| SignInError::Failed("the web client could not be made".to_string()))
        };
        Ok(Arc::new(Guarded {
            follow: build(policy)?,
            stop: build(reqwest::redirect::Policy::none())?,
            refused,
            status: Mutex::new(None),
        }))
    }

    /// Sends `request`, refusing a URL that is not https or loopback, and answers its status,
    /// headers and at most 1 MiB of body.
    pub(crate) async fn send(
        &self,
        request: reqwest::Request,
        follow_redirects: bool,
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        if !allowed(request.url()) {
            let reason = format!("{} is not https", request.url());
            *self
                .refused
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(reason.clone());
            return Err(reason.into());
        }
        let client = if follow_redirects {
            &self.follow
        } else {
            &self.stop
        };
        let mut response = client.execute(request).await.map_err(|error| {
            // A refused hop is the reason, not the client's wrapping of it.
            match self.refusal() {
                Some(reason) => OAuthHttpClientError::from(reason),
                None => OAuthHttpClientError::from(error),
            }
        })?;
        let status = response.status();
        if !status.is_success() && !status.is_redirection() {
            *self
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(status.as_u16());
        }
        let mut builder = http::Response::builder()
            .status(status)
            .version(response.version());
        for (name, value) in response.headers() {
            builder = builder.header(name, value);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(OAuthHttpClientError::from)? {
            if chunk.len() > MAX_BODY - body.len() {
                return Err("a response was too large".into());
            }
            body.extend_from_slice(&chunk);
        }
        builder.body(body).map_err(OAuthHttpClientError::from)
    }

    /// Sends a form POST to `url`, which must be https or loopback, and does not follow redirects.
    pub(crate) async fn post_form(
        &self,
        url: &str,
        pairs: &[(&str, &str)],
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        let request = self
            .stop
            .post(url)
            .form(pairs)
            .build()
            .map_err(OAuthHttpClientError::from)?;
        self.send(request, false).await
    }

    /// What was refused as not https, if anything.
    pub(crate) fn refusal(&self) -> Option<String> {
        self.refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The last error status a service answered, as " (HTTP 400)", or nothing.
    pub(crate) fn status_note(&self) -> String {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .map_or_else(String::new, |status| format!(" (HTTP {status})"))
    }

    fn forget_status(&self) {
        *self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

impl OAuthHttpClient for Guarded {
    fn execute(&self, request: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
        Box::pin(async move {
            let OAuthHttpRequest {
                request,
                redirect_policy,
                timeout,
                ..
            } = request;
            let mut request =
                reqwest::Request::try_from(request).map_err(OAuthHttpClientError::from)?;
            if let Some(timeout) = timeout {
                *request.timeout_mut() = Some(timeout);
            }
            self.send(request, redirect_policy == OAuthHttpRedirectPolicy::Follow)
                .await
        })
    }
}

/// The host of `url`, for a sentence.
fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
        .unwrap_or_else(|| "the service".to_string())
}

/// A sign-in under way: a page to send the user to, and a listener for the way back.
pub struct SignIn {
    listeners: Vec<TcpListener>,
    addr: SocketAddr,
    session: AuthorizationSession,
    guard: Arc<Guarded>,
    authorize_url: String,
    issuer: String,
    iss_promised: bool,
    state: String,
    resource: String,
    client_id: String,
    requested_scopes: Vec<String>,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    started: tokio::time::Instant,
    started_at: DateTime<Utc>,
}

impl fmt::Debug for SignIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignIn")
            .field("issuer", &self.issuer)
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl SignIn {
    /// The service's page, which the user opens.
    #[must_use]
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// The service's issuer.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Where the listener is.
    #[must_use]
    pub fn callback_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Waits for the way back, up to ten minutes from the start, and makes the grant. The
    /// listener answers the one callback that carries this attempt's `state`, tells the user in
    /// the tab how it went, and closes.
    ///
    /// # Errors
    /// Why the sign-in did not happen.
    pub async fn finish(mut self) -> Result<OAuthGrant, SignInError> {
        let deadline = self.started + SIGN_IN_WINDOW;
        let listeners = std::mem::take(&mut self.listeners);
        let waiting = wait_for_callback(listeners, &self.state);
        let (mut stream, params) = tokio::time::timeout_at(deadline, waiting)
            .await
            .map_err(|_| SignInError::TimedOut)??;
        let outcome = self.complete(&params).await;
        let host = host_of(&self.issuer);
        let (status, text) = match &outcome {
            Ok(_) => (
                200,
                format!("You're signed in to {host}. You can close this tab and go back to Farik."),
            ),
            Err(error) => (
                400,
                format!(
                    "Farik couldn't finish signing in: {}. Close this tab and try again in Farik.",
                    error.sentence(&host)
                ),
            ),
        };
        let _ = respond(&mut stream, status, &text).await;
        outcome
    }

    async fn complete(&self, params: &HashMap<String, String>) -> Result<OAuthGrant, SignInError> {
        let host = host_of(&self.issuer);
        let iss = params.get("iss").map(String::as_str);
        if let Some(error) = params.get("error") {
            // RFC 9207: an error carrying another issuer is not acted on, nor shown.
            if iss.is_some_and(|iss| iss != self.issuer) || (iss.is_none() && self.iss_promised) {
                return Err(SignInError::Mismatch);
            }
            return Err(if error == "access_denied" {
                SignInError::Denied("access_denied".to_string())
            } else {
                SignInError::Failed(format!("{host} refused the sign-in"))
            });
        }
        let code = params
            .get("code")
            .ok_or_else(|| SignInError::Failed(format!("{host} answered without a code")))?;
        self.guard.forget_status();
        let token = self
            .session
            .handle_callback_with_issuer(code, &self.state, iss)
            .await
            .map_err(|error| match error {
                AuthError::AuthorizationServerMismatch { .. }
                | AuthError::AuthorizationServerMissingIssuer { .. } => SignInError::Mismatch,
                _ => SignInError::Failed(format!(
                    "{host} refused the token request{}",
                    self.guard.status_note()
                )),
            })?;
        let token = serde_json::to_value(&token)
            .map_err(|_| SignInError::Failed(format!("{host} sent a token Farik cannot read")))?;
        let unreadable = || SignInError::Failed(format!("{host} sent a token Farik cannot read"));
        if !token["token_type"]
            .as_str()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("bearer"))
        {
            return Err(unreadable());
        }
        let access = token["access_token"].as_str().ok_or_else(unreadable)?;
        let issued_at = self.started_at
            + chrono::Duration::from_std(self.started.elapsed()).unwrap_or_default();
        let expires_at = token["expires_in"]
            .as_u64()
            .and_then(|seconds| i64::try_from(seconds).ok())
            .map(|seconds| issued_at + chrono::Duration::seconds(seconds));
        let scopes = token["scope"].as_str().map_or_else(
            || self.requested_scopes.clone(),
            |scope| scope.split_whitespace().map(ToString::to_string).collect(),
        );
        Ok(OAuthGrant {
            issuer: self.issuer.clone(),
            resource: self.resource.clone(),
            client_id: self.client_id.clone(),
            token_endpoint: self.token_endpoint.clone(),
            revocation_endpoint: self.revocation_endpoint.clone(),
            access_token: Secret::new(access.to_string()),
            refresh_token: token["refresh_token"]
                .as_str()
                .map(|token| Secret::new(token.to_string())),
            issued_at,
            expires_at,
            scopes,
            lapsed: false,
        })
    }
}

impl SignInError {
    /// The reason as the sentence the tab and the page say, from the code alone.
    fn sentence(&self, host: &str) -> String {
        match self {
            SignInError::Denied(_) => format!("you said no on {host}'s page"),
            SignInError::Mismatch => format!(
                "something didn't match on the way back from {host}, so Farik stopped to keep you safe"
            ),
            SignInError::TimedOut => "it took longer than 10 minutes".to_string(),
            _ => format!("{host} did not accept the sign-in"),
        }
    }
}

/// Minimal HTML for a tab: a sentence, no link, no script, nothing the service sent.
fn page(text: &str) -> String {
    let text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Farik</title></head>\
         <body><h1>Farik</h1><p>{text}</p></body></html>"
    )
}

async fn respond(stream: &mut TcpStream, status: u16, text: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body = page(text);
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Security-Policy: default-src 'none'\r\nCache-Control: no-store\r\n\
         Referrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

/// Reads one request's first line and answers it, or hands the first callback with `state` over.
async fn serve_connection(
    mut stream: TcpStream,
    state: String,
    found: tokio::sync::mpsc::UnboundedSender<(TcpStream, HashMap<String, String>)>,
) {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 1024];
    let read = async {
        while !head.windows(4).any(|window| window == b"\r\n\r\n") && head.len() < 8192 {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(count) => head.extend_from_slice(&chunk[..count]),
            }
        }
    };
    if tokio::time::timeout(CONNECTION_READ, read).await.is_err() {
        return;
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let first = text.lines().next().unwrap_or_default();
    let mut words = first.split(' ');
    let (method, target) = (
        words.next().unwrap_or_default(),
        words.next().unwrap_or_default(),
    );
    let Ok(url) = Url::parse(&format!("http://localhost{target}")) else {
        let _ = respond(&mut stream, 404, "Not found.").await;
        return;
    };
    if method != "GET" || url.path() != "/callback" {
        let _ = respond(&mut stream, 404, "Not found.").await;
        return;
    }
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if params.get("state") != Some(&state) {
        let _ = respond(
            &mut stream,
            400,
            "This is not the page Farik is waiting for.",
        )
        .await;
        return;
    }
    let _ = found.send((stream, params));
}

/// Accepts connections on every listener until one is the callback, then closes them all.
async fn wait_for_callback(
    listeners: Vec<TcpListener>,
    state: &str,
) -> Result<(TcpStream, HashMap<String, String>), SignInError> {
    let (found, mut arrived) = tokio::sync::mpsc::unbounded_channel();
    let mut accepting = tokio::task::JoinSet::new();
    for listener in listeners {
        let (state, found) = (state.to_string(), found.clone());
        accepting.spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve_connection(stream, state.clone(), found.clone()));
            }
        });
    }
    drop(found);
    let callback = arrived.recv().await;
    // The listeners close here, before the code is exchanged.
    accepting.shutdown().await;
    callback.ok_or_else(|| SignInError::Failed("the sign-in listener stopped".to_string()))
}

/// Binds the callback listener: this computer's loopback only, never every address. With a
/// registered client the port is fixed; otherwise any free one, tried again up to five times.
async fn bind(settings: &OAuthSettings) -> Result<(Vec<TcpListener>, SocketAddr), SignInError> {
    let fixed = settings
        .client_id
        .is_some()
        .then(|| settings.callback_port.unwrap_or(DEFAULT_CALLBACK_PORT));
    let taken = |port: u16| SignInError::Failed(format!("port {port} is in use on this computer"));
    for _ in 0..5 {
        let first = TcpListener::bind(("127.0.0.1", fixed.unwrap_or(0)))
            .await
            .map_err(|_| taken(fixed.unwrap_or(0)))?;
        let addr = first.local_addr().map_err(|_| taken(fixed.unwrap_or(0)))?;
        match TcpListener::bind(("::1", addr.port())).await {
            Ok(second) => return Ok((vec![first, second], addr)),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                if fixed.is_some() {
                    return Err(taken(addr.port()));
                }
            }
            // No IPv6 on this computer.
            Err(_) => return Ok((vec![first], addr)),
        }
    }
    Err(SignInError::Failed(
        "no free port was found on this computer".to_string(),
    ))
}

/// What `discover` found out about a server's sign-in.
struct Discovered {
    manager: AuthorizationManager,
    metadata: AuthorizationMetadata,
}

fn failed_at(guard: &Guarded, host: &str, step: &str) -> SignInError {
    SignInError::Failed(guard.refusal().unwrap_or_else(|| {
        format!(
            "{host} could not be reached to {step}{}",
            guard.status_note()
        )
    }))
}

/// Asks `url` to initialize without a token and reads where its sign-in is, as the specification
/// says, and nowhere else: a server that does not point to its metadata is not guessed at.
async fn discover(url: &str, guard: &Arc<Guarded>) -> Result<Discovered, SignInError> {
    let parsed =
        Url::parse(url).map_err(|_| SignInError::Failed(format!("{url} is not a web address")))?;
    if !allowed(&parsed) {
        return Err(SignInError::Failed(format!("{url} is not https")));
    }
    let host = host_of(url);
    let manager = AuthorizationManager::new_with_oauth_http_client(url, guard.clone())
        .await
        .map_err(|_| SignInError::Failed(format!("{url} is not a web address")))?;
    let initialize = reqwest::Client::new()
        .post(parsed)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "Farik", "version": "0" },
                },
            })
            .to_string(),
        )
        .build()
        .map_err(|_| SignInError::Failed(format!("{url} is not a web address")))?;
    let answer = guard
        .send(initialize, false)
        .await
        .map_err(|_| failed_at(guard, &host, "ask how to sign in"))?;
    let challenge = (answer.status() == http::StatusCode::UNAUTHORIZED)
        .then(|| answer.headers().get(http::header::WWW_AUTHENTICATE))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    guard.forget_status();
    let resolved = manager
        .resolve_metadata_from_challenge(challenge.as_deref())
        .await
        .map_err(|_| failed_at(guard, &host, "find its sign-in"))?;
    if resolved.source != AuthorizationMetadataSource::ProtectedResourceMetadata {
        return Err(SignInError::NotOffered);
    }
    Ok(Discovered {
        manager,
        metadata: resolved.metadata,
    })
}

/// Farik's checks on what a service published, before anything is registered.
fn check_metadata(
    metadata: &AuthorizationMetadata,
    settings: &OAuthSettings,
) -> Result<(), SignInError> {
    let revocation = metadata
        .additional_fields
        .get("revocation_endpoint")
        .and_then(serde_json::Value::as_str);
    let endpoints = [
        Some(metadata.authorization_endpoint.as_str()),
        Some(metadata.token_endpoint.as_str()),
        metadata.registration_endpoint.as_deref(),
        revocation,
    ];
    for endpoint in endpoints.into_iter().flatten() {
        if !Url::parse(endpoint).is_ok_and(|url| allowed(&url)) {
            return Err(SignInError::Failed(format!("{endpoint} is not https")));
        }
    }
    if !metadata
        .code_challenge_methods_supported
        .as_ref()
        .is_some_and(|methods| methods.iter().any(|method| method == "S256"))
    {
        return Err(SignInError::PkceNotSupported);
    }
    if settings.client_id.is_none() && metadata.registration_endpoint.is_none() {
        return Err(SignInError::NotSupported);
    }
    if metadata.issuer.is_none() {
        return Err(SignInError::Failed(
            "the service did not say who it is".to_string(),
        ));
    }
    Ok(())
}

/// Finds out how `url` signs a user in, registers, binds the listener and makes the address.
///
/// # Errors
/// Why signing in is not possible.
pub async fn start_sign_in(
    url: &str,
    settings: &OAuthSettings,
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    tokio::time::timeout(SIGN_IN_START, start(url, settings, now))
        .await
        .map_err(|_| {
            SignInError::Failed(format!(
                "{} did not answer in {} seconds",
                host_of(url),
                SIGN_IN_START.as_secs()
            ))
        })?
}

async fn start(
    url: &str,
    settings: &OAuthSettings,
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    let guard = Guarded::new()?;
    let Discovered {
        mut manager,
        metadata,
    } = discover(url, &guard).await?;
    check_metadata(&metadata, settings)?;
    let host = host_of(url);
    let (listeners, addr) = bind(settings).await?;
    let redirect = format!("http://localhost:{}/callback", addr.port());
    let issuer = metadata.issuer.clone().unwrap_or_default();
    let iss_promised = metadata
        .additional_fields
        .get("authorization_response_iss_parameter_supported")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let token_endpoint = metadata.token_endpoint.clone();
    let revocation_endpoint = metadata
        .additional_fields
        .get("revocation_endpoint")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    manager.set_metadata(metadata);
    let mut request = AuthorizationRequest::new(redirect).with_client_name("Farik");
    if let Some(client_id) = &settings.client_id {
        request = request.with_preregistered_client(client_id);
    }
    if !settings.scopes.is_empty() {
        request = request.with_scopes(settings.scopes.clone());
    }
    guard.forget_status();
    let session = AuthorizationSession::new(manager, request)
        .await
        .map_err(|(_, error)| match error {
            AuthError::RegistrationFailed(_) => {
                SignInError::Failed(guard.refusal().unwrap_or_else(|| {
                    format!("{host} refused the registration{}", guard.status_note())
                }))
            }
            _ => SignInError::Failed(format!("{host} could not start the sign-in")),
        })?;
    let authorize_url = session.get_authorization_url().to_string();
    let query: HashMap<String, String> = Url::parse(&authorize_url)
        .map(|url| url.query_pairs().into_owned().collect())
        .unwrap_or_default();
    let field = |name: &str| query.get(name).cloned();
    let (Some(state), Some(resource), Some(client_id)) =
        (field("state"), field("resource"), field("client_id"))
    else {
        return Err(SignInError::Failed(format!(
            "{host} could not start the sign-in"
        )));
    };
    Ok(SignIn {
        listeners,
        addr,
        session,
        guard,
        authorize_url,
        issuer,
        iss_promised,
        state,
        resource,
        client_id,
        requested_scopes: field("scope")
            .map(|scope| scope.split_whitespace().map(ToString::to_string).collect())
            .unwrap_or_default(),
        token_endpoint,
        revocation_endpoint,
        started: tokio::time::Instant::now(),
        started_at: now,
    })
}

/// How long a grant with no known expiry is trusted for before it is refreshed.
const UNKNOWN_EXPIRY_TRUST: chrono::Duration = chrono::Duration::minutes(50);
/// How long asking a service to forget a grant may take.
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);

/// The `error` codes that mean the service ended the sign-in.
const ENDED: [&str; 3] = ["invalid_grant", "invalid_client", "unauthorized_client"];

/// The grant refreshed, when it will not last `valid_for` more from `now`: `Ok(None)` when it
/// will. A grant with no refresh token is `Ok(None)` while its access token holds, and lapses once
/// it has expired. The rotated refresh token is the answer's; one left out keeps the old.
///
/// # Errors
/// `Lapsed` when the service ended the sign-in (`invalid_grant`, `invalid_client` or
/// `unauthorized_client`); `Failed` for anything else, including `timeout` passing.
pub async fn refreshed(
    grant: &OAuthGrant,
    now: DateTime<Utc>,
    valid_for: Duration,
    timeout: Duration,
) -> Result<Option<OAuthGrant>, SignInError> {
    if grant.lapsed {
        return Err(SignInError::Lapsed);
    }
    let due = match grant.expires_at {
        Some(expires_at) => {
            expires_at < now + chrono::Duration::from_std(valid_for).unwrap_or_default()
        }
        None => now - grant.issued_at > UNKNOWN_EXPIRY_TRUST,
    };
    if !due {
        return Ok(None);
    }
    let Some(refresh) = &grant.refresh_token else {
        return if grant.expires_at.is_some_and(|expires_at| expires_at <= now) {
            Err(SignInError::Lapsed)
        } else {
            Ok(None)
        };
    };
    let host = host_of(&grant.token_endpoint);
    let guard = Guarded::new()?;
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.expose()),
        ("client_id", grant.client_id.as_str()),
        ("resource", grant.resource.as_str()),
    ];
    let asking = guard.post_form(&grant.token_endpoint, &form);
    let answer = match tokio::time::timeout(timeout, asking).await {
        Err(_) => {
            return Err(SignInError::Failed(format!(
                "{host} did not answer in {} seconds",
                timeout.as_secs()
            )));
        }
        Ok(Err(_)) => {
            return Err(SignInError::Failed(guard.refusal().unwrap_or_else(|| {
                format!("{host} could not be reached to refresh the sign-in")
            })));
        }
        Ok(Ok(answer)) => answer,
    };
    let body: serde_json::Value = serde_json::from_slice(answer.body()).unwrap_or_default();
    if let Some(error) = body["error"].as_str() {
        return Err(if ENDED.contains(&error) {
            SignInError::Lapsed
        } else {
            SignInError::Failed(format!(
                "{host} refused to refresh the sign-in (HTTP {})",
                answer.status().as_u16()
            ))
        });
    }
    let Some(access) = body["access_token"]
        .as_str()
        .filter(|_| answer.status().is_success())
    else {
        return Err(SignInError::Failed(format!(
            "{host} refused to refresh the sign-in (HTTP {})",
            answer.status().as_u16()
        )));
    };
    let mut fresh = grant.clone();
    fresh.access_token = Secret::new(access.to_string());
    if let Some(rotated) = body["refresh_token"].as_str() {
        fresh.refresh_token = Some(Secret::new(rotated.to_string()));
    }
    fresh.issued_at = now;
    fresh.expires_at = body["expires_in"]
        .as_u64()
        .and_then(|seconds| i64::try_from(seconds).ok())
        .map(|seconds| now + chrono::Duration::seconds(seconds));
    if let Some(scope) = body["scope"].as_str() {
        fresh.scopes = scope.split_whitespace().map(ToString::to_string).collect();
    }
    Ok(Some(fresh))
}

/// Asks the service to forget `grant` (RFC 7009): its refresh token, else its access token. Best
/// effort: five seconds, and no failure is reported.
pub async fn revoke(grant: &OAuthGrant) {
    let Some(endpoint) = &grant.revocation_endpoint else {
        return;
    };
    let (token, hint) = match &grant.refresh_token {
        Some(refresh) => (refresh, "refresh_token"),
        None => (&grant.access_token, "access_token"),
    };
    let Ok(guard) = Guarded::new() else {
        return;
    };
    let form = [
        ("token", token.expose()),
        ("token_type_hint", hint),
        ("client_id", grant.client_id.as_str()),
    ];
    let _ = tokio::time::timeout(REVOKE_TIMEOUT, guard.post_form(endpoint, &form)).await;
}

impl OAuthGrant {
    /// The stored form, which holds both tokens.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "issuer": self.issuer,
            "resource": self.resource,
            "client_id": self.client_id,
            "token_endpoint": self.token_endpoint,
            "revocation_endpoint": self.revocation_endpoint,
            "access_token": self.access_token.expose(),
            "refresh_token": self.refresh_token.as_ref().map(Secret::expose),
            "issued_at": self.issued_at.to_rfc3339(),
            "expires_at": self.expires_at.map(|at| at.to_rfc3339()),
            "scopes": self.scopes,
            "lapsed": self.lapsed,
        })
    }

    /// The grant a stored form holds; `None` when it is not one. Never quotes the form.
    pub(crate) fn from_json(value: &serde_json::Value) -> Option<OAuthGrant> {
        let text = |name: &str| value[name].as_str().map(ToString::to_string);
        let time = |name: &str| {
            value[name]
                .as_str()
                .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.with_timezone(&Utc))
        };
        Some(OAuthGrant {
            issuer: text("issuer")?,
            resource: text("resource")?,
            client_id: text("client_id")?,
            token_endpoint: text("token_endpoint")?,
            revocation_endpoint: text("revocation_endpoint"),
            access_token: Secret::new(text("access_token")?),
            refresh_token: text("refresh_token").map(Secret::new),
            issued_at: time("issued_at")?,
            expires_at: time("expires_at"),
            scopes: value["scopes"]
                .as_array()?
                .iter()
                .filter_map(|scope| scope.as_str().map(ToString::to_string))
                .collect(),
            lapsed: value["lapsed"].as_bool().unwrap_or(false),
        })
    }
}
