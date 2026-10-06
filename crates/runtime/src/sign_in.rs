//! Signing an agent in to a connector's service with OAuth (ADR 0033): the grant, the
//! sign-in that makes one, and the refresh and revocation that keep it.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::team::OAuthSettings;
use oauth2::{CsrfToken, PkceCodeChallenge};
use reqwest::Url;
use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationMetadata, AuthorizationMetadataSource,
    AuthorizationRequest, AuthorizationSession, OAuthHttpClient, OAuthHttpClientError,
    OAuthHttpClientFuture, OAuthHttpRedirectPolicy, OAuthHttpRequest,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::claude::Secret;
use crate::registered_apps::{AppFlow, RegisteredApp, app_for};

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
    /// The id of the app of Farik's own this grant was made with (`RegisteredApp::id`), when it
    /// was; none for a client the user gave or the service registered.
    pub app: Option<String>,
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
            .field("app", &self.app)
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

    /// [`Guarded::post_form`] to a device or token endpoint, which is asked for JSON: GitHub
    /// otherwise answers form-encoded.
    pub(crate) async fn post_token_form(
        &self,
        url: &str,
        pairs: &[(&str, &str)],
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        let request = self
            .stop
            .post(url)
            .header(http::header::ACCEPT, "application/json")
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

/// A sign-in under way: a page to send the user to, and what Farik waits for after: a listener
/// for the way back, or, for one of Farik's own apps, the service saying yes to a code.
pub struct SignIn {
    way: Way,
    setup: Setup,
}

/// How Farik learns the user said yes.
enum Way {
    /// Step 03's: the service sends the browser back to a listener on this computer.
    Redirect(Box<Redirect>),
    /// The device flow (RFC 8628): the user types a code on the service's page and Farik polls.
    Device(Device),
    /// Farik's own authorization-code flow with PKCE, for one of its own connectors: the service
    /// sends the browser back to a listener on this computer.
    Loopback(Box<Loopback>),
}

/// What the way back of a redirect sign-in needs.
struct Redirect {
    listeners: Vec<TcpListener>,
    addr: SocketAddr,
    session: AuthorizationSession,
    iss_promised: bool,
    state: String,
    requested_scopes: Vec<String>,
}

/// What the way back of Farik's own loopback sign-in needs: the listener, the `state` it answers
/// to, and the verifier that proves the exchange comes from the program that started the sign-in.
struct Loopback {
    listeners: Vec<TcpListener>,
    addr: SocketAddr,
    app: RegisteredApp,
    state: String,
    verifier: Secret,
    redirect_uri: String,
    scopes: Vec<String>,
}

/// What polling a device code needs. The code never leaves Farik: the user types the other one.
struct Device {
    code: Secret,
    user_code: String,
    interval: Duration,
}

/// What a sign-in makes its grant from, whichever way it goes.
struct Setup {
    guard: Arc<Guarded>,
    authorize_url: String,
    issuer: String,
    resource: String,
    client_id: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    app: Option<RegisteredApp>,
    started: tokio::time::Instant,
    started_at: DateTime<Utc>,
}

impl fmt::Debug for SignIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignIn")
            .field("issuer", &self.setup.issuer)
            .field("addr", &self.callback_addr())
            .finish_non_exhaustive()
    }
}

impl SignIn {
    /// The service's page, which the user opens.
    #[must_use]
    pub fn authorize_url(&self) -> &str {
        &self.setup.authorize_url
    }

    /// The service's issuer.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.setup.issuer
    }

    /// Where the listener is; none for the device flow, which has none.
    #[must_use]
    pub fn callback_addr(&self) -> Option<SocketAddr> {
        match &self.way {
            Way::Redirect(redirect) => Some(redirect.addr),
            Way::Loopback(loopback) => Some(loopback.addr),
            Way::Device(_) => None,
        }
    }

    /// The code the user types on the service's page; only the device flow has one.
    #[must_use]
    pub fn user_code(&self) -> Option<&str> {
        match &self.way {
            Way::Device(device) => Some(&device.user_code),
            Way::Redirect(_) | Way::Loopback(_) => None,
        }
    }

    /// What Farik's own app for the service is called, when one is signing in.
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        self.setup.app.as_ref().map(|app| app.name)
    }

    /// Where the user installs Farik's own app on what an agent is to read, when it must be.
    #[must_use]
    pub fn install_url(&self) -> Option<&str> {
        self.setup.app.as_ref().and_then(|app| app.install_url)
    }

    /// Waits for the user, up to ten minutes from the start, and makes the grant. A redirect
    /// sign-in's listener answers the one callback that carries this attempt's `state`, tells the
    /// user in the tab how it went, and closes; a device sign-in polls the service from here, so
    /// that dropping this future ends it.
    ///
    /// # Errors
    /// Why the sign-in did not happen.
    pub async fn finish(self) -> Result<OAuthGrant, SignInError> {
        let SignIn { way, setup } = self;
        let deadline = setup.started + SIGN_IN_WINDOW;
        match way {
            Way::Redirect(redirect) => setup.finish_redirect(*redirect, deadline).await,
            Way::Loopback(loopback) => setup.finish_loopback(*loopback, deadline).await,
            Way::Device(device) => tokio::time::timeout_at(deadline, setup.poll(&device))
                .await
                .map_err(|_| SignInError::TimedOut)?,
        }
    }
}

impl Setup {
    async fn finish_redirect(
        &self,
        mut redirect: Redirect,
        deadline: tokio::time::Instant,
    ) -> Result<OAuthGrant, SignInError> {
        let listeners = std::mem::take(&mut redirect.listeners);
        let waiting = wait_for_callback(listeners, &redirect.state);
        let (mut stream, params) = tokio::time::timeout_at(deadline, waiting)
            .await
            .map_err(|_| SignInError::TimedOut)??;
        let outcome = self.complete(&redirect, &params).await;
        tell_the_tab(&mut stream, &outcome, &host_of(&self.issuer)).await;
        outcome
    }

    /// The same for Farik's own loopback sign-in, whose tab names the provider.
    async fn finish_loopback(
        &self,
        mut loopback: Loopback,
        deadline: tokio::time::Instant,
    ) -> Result<OAuthGrant, SignInError> {
        let listeners = std::mem::take(&mut loopback.listeners);
        let waiting = wait_for_callback(listeners, &loopback.state);
        let (mut stream, params) = tokio::time::timeout_at(deadline, waiting)
            .await
            .map_err(|_| SignInError::TimedOut)??;
        let outcome = self.complete_loopback(&loopback, &params).await;
        tell_the_tab(&mut stream, &outcome, loopback.app.name).await;
        outcome
    }

    /// What the callback of Farik's own loopback sign-in comes to: `iss` checked, the code
    /// exchanged with the verifier (and the app's client secret, when it has one), and the
    /// answer held to what Farik needs of it.
    async fn complete_loopback(
        &self,
        loopback: &Loopback,
        params: &HashMap<String, String>,
    ) -> Result<OAuthGrant, SignInError> {
        let app = &loopback.app;
        let host = host_of(&self.token_endpoint);
        // RFC 9207, and required here: an answer from another issuer is not acted on, nor shown.
        if params.get("iss").map(String::as_str) != Some(self.issuer.as_str()) {
            return Err(SignInError::Mismatch);
        }
        if let Some(error) = params.get("error") {
            return Err(if error == "access_denied" {
                SignInError::Denied("access_denied".to_string())
            } else {
                SignInError::Failed(format!("{} refused the sign-in", app.name))
            });
        }
        let code = params
            .get("code")
            .ok_or_else(|| SignInError::Failed(format!("{} answered without a code", app.name)))?;
        let mut form = vec![
            ("code", code.as_str()),
            ("client_id", app.client_id),
            ("redirect_uri", loopback.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", loopback.verifier.expose()),
        ];
        if let Some(secret) = app.client_secret {
            form.push(("client_secret", secret));
        }
        self.guard.forget_status();
        let answer = self
            .guard
            .post_token_form(&self.token_endpoint, &form)
            .await
            .map_err(|_| failed_at(&self.guard, &host, "finish signing in"))?;
        let body: serde_json::Value = serde_json::from_slice(answer.body()).unwrap_or_default();
        if body.get("error").is_some() || !answer.status().is_success() {
            return Err(SignInError::Failed(format!(
                "{} refused the token request{}",
                app.name,
                self.guard.status_note()
            )));
        }
        if !body["token_type"]
            .as_str()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("bearer"))
        {
            return Err(SignInError::Failed(format!(
                "{} did not give Farik a bearer token",
                app.name
            )));
        }
        if body["refresh_token"].as_str().is_none() {
            return Err(SignInError::Failed(format!(
                "{} did not give Farik a lasting sign-in",
                app.name
            )));
        }
        let granted: Vec<&str> = body["scope"]
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .collect();
        if loopback
            .scopes
            .iter()
            .any(|asked| !granted.contains(&asked.as_str()))
        {
            return Err(SignInError::Failed(format!(
                "you did not allow Farik all it asks of {}; sign in again and tick every box",
                app.name
            )));
        }
        self.grant_from(&body, &loopback.scopes)
    }

    async fn complete(
        &self,
        redirect: &Redirect,
        params: &HashMap<String, String>,
    ) -> Result<OAuthGrant, SignInError> {
        let host = host_of(&self.issuer);
        let iss = params.get("iss").map(String::as_str);
        if let Some(error) = params.get("error") {
            // RFC 9207: an error carrying another issuer is not acted on, nor shown.
            if iss.is_some_and(|iss| iss != self.issuer) || (iss.is_none() && redirect.iss_promised)
            {
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
        let token = redirect
            .session
            .handle_callback_with_issuer(code, &redirect.state, iss)
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
        self.grant_from(&token, &redirect.requested_scopes)
    }

    /// Polls the service for the device code until it says yes or no; the caller bounds the time.
    async fn poll(&self, device: &Device) -> Result<OAuthGrant, SignInError> {
        let host = host_of(&self.token_endpoint);
        let mut interval = device.interval;
        let form = [
            ("client_id", self.client_id.as_str()),
            ("device_code", device.code.expose()),
            ("grant_type", DEVICE_GRANT),
        ];
        // Answers in a row that were not the service's: a gateway's page, a dropped connection.
        let mut passing = 0_u32;
        loop {
            tokio::time::sleep(interval).await;
            self.guard.forget_status();
            let answer = self
                .guard
                .post_token_form(&self.token_endpoint, &form)
                .await;
            // A refused address is not a passing failure: asking again would be refused again.
            if answer.is_err() && self.guard.refusal().is_some() {
                return Err(failed_at(&self.guard, &host, "finish signing in"));
            }
            // GitHub answers an error with status 200, so the body is read before the status.
            let body = answer.ok().as_ref().and_then(service_answer);
            let Some(body) = body else {
                passing += 1;
                if passing >= PASSING_FAILURES {
                    return Err(failed_at(&self.guard, &host, "finish signing in"));
                }
                continue;
            };
            passing = 0;
            match body["error"].as_str() {
                Some("authorization_pending") => {}
                Some("slow_down") => interval += Duration::from_secs(5),
                Some("access_denied") => {
                    return Err(SignInError::Denied("access_denied".to_string()));
                }
                Some("expired_token") => return Err(SignInError::TimedOut),
                Some(_) => {
                    return Err(SignInError::Failed(format!(
                        "{host} refused the sign-in{}",
                        self.guard.status_note()
                    )));
                }
                None => {
                    let mut grant = self.grant_from(&body, &[])?;
                    // A GitHub App's rights are set on the app, not asked for.
                    grant.scopes.clear();
                    return Ok(grant);
                }
            }
        }
    }

    /// The grant a service's token answer makes.
    fn grant_from(
        &self,
        token: &serde_json::Value,
        requested_scopes: &[String],
    ) -> Result<OAuthGrant, SignInError> {
        let host = host_of(&self.issuer);
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
            || requested_scopes.to_vec(),
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
            app: self.app.map(|app| app.id.to_string()),
        })
    }
}

/// The grant type of a device code's poll (RFC 8628).
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// How many answers in a row that are not the service's end a device sign-in `Failed`.
const PASSING_FAILURES: u32 = 3;

/// What a poll's answer says, when it is the service's: a JSON object. A reply that is not one (a
/// gateway's page) is not, nor is a server error that carries no `error` code; the poll asks again.
fn service_answer(answer: &http::Response<Vec<u8>>) -> Option<serde_json::Value> {
    let body: serde_json::Value = serde_json::from_slice(answer.body()).ok()?;
    let coded = body.get("error").is_some();
    (body.is_object() && (coded || !answer.status().is_server_error())).then_some(body)
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

/// Tells the tab the callback came in how the sign-in went, and closes it.
async fn tell_the_tab(
    stream: &mut TcpStream,
    outcome: &Result<OAuthGrant, SignInError>,
    host: &str,
) {
    let (status, headline, rest) = match outcome {
        Ok(_) => (
            200,
            format!("You're signed in to {host}."),
            "You can close this tab and go back to Farik.",
        ),
        Err(error) => (
            400,
            format!(
                "Farik couldn't finish signing in: {}.",
                error.sentence(host)
            ),
            "Close this tab and try again in Farik.",
        ),
    };
    let _ = respond(stream, status, &headline, rest).await;
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Minimal HTML for a tab, as the board draws it: the mark, a headline and a sentence, no link,
/// no script, nothing the service sent. The mark is inline, so the page fetches nothing.
fn page(headline: &str, rest: &str) -> String {
    use base64::Engine as _;

    let mark = base64::engine::general_purpose::STANDARD.encode(include_bytes!(
        "../../../packages/brand/assets/icons/icon-48.png"
    ));
    let rest = if rest.is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", escaped(rest))
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Farik</title></head>\
         <body><img alt=\"Farik\" width=\"48\" height=\"48\" src=\"data:image/png;base64,{mark}\">\
         <h1>{}</h1>{rest}</body></html>",
        escaped(headline)
    )
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    headline: &str,
    rest: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body = page(headline, rest);
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Security-Policy: default-src 'none'; img-src data:\r\nCache-Control: no-store\r\n\
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
        let _ = respond(&mut stream, 404, "Not found.", "").await;
        return;
    };
    if method != "GET" || url.path() != "/callback" {
        let _ = respond(&mut stream, 404, "Not found.", "").await;
        return;
    }
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if params.get("state") != Some(&state) {
        let _ = respond(
            &mut stream,
            400,
            "This is not the page Farik is waiting for.",
            "",
        )
        .await;
        return;
    }
    let _ = found.send((stream, params));
}

/// Accepts connections on the listeners until one is the callback, then closes them. The
/// listeners belong to this future, so dropping it closes them at once.
async fn wait_for_callback(
    mut listeners: Vec<TcpListener>,
    state: &str,
) -> Result<(TcpStream, HashMap<String, String>), SignInError> {
    let (found, mut arrived) = tokio::sync::mpsc::unbounded_channel();
    let second = if listeners.len() > 1 {
        listeners.pop()
    } else {
        None
    };
    let Some(first) = listeners.pop() else {
        return Err(SignInError::Failed("there is no listener".to_string()));
    };
    loop {
        let accepted = tokio::select! {
            accepted = first.accept() => accepted,
            accepted = async {
                match &second {
                    Some(listener) => listener.accept().await,
                    None => std::future::pending().await,
                }
            } => accepted,
            callback = arrived.recv() => {
                // The listeners close here, before the code is exchanged.
                return callback.ok_or_else(|| {
                    SignInError::Failed("the sign-in listener stopped".to_string())
                });
            }
        };
        if let Ok((stream, _)) = accepted {
            tokio::spawn(serve_connection(stream, state.to_string(), found.clone()));
        }
    }
}

/// The callback port a registered client fixes; none when Farik registers its own client.
fn fixed_port(settings: &OAuthSettings) -> Option<u16> {
    (settings.client_id.is_some()).then(|| settings.callback_port.unwrap_or(DEFAULT_CALLBACK_PORT))
}

/// Binds the callback listener: this computer's loopback only, never every address. With a
/// registered client the port is fixed; otherwise any free one, tried again up to five times.
async fn bind(settings: &OAuthSettings) -> Result<(Vec<TcpListener>, SocketAddr), SignInError> {
    let fixed = fixed_port(settings);
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

/// Starts signing a user in to the server at `url`. If `apps` has one of Farik's own apps for the
/// server's address (and the team file gives no client of its own), the user signs in with it;
/// otherwise the server's sign-in is found out, a client registered, the listener bound and the
/// address made, as step 03 does. A client id of one of `apps` is used only for its own servers.
///
/// # Errors
/// Why signing in is not possible, among them an app's client id on another server's address.
pub async fn start_sign_in(
    url: &str,
    settings: &OAuthSettings,
    apps: &[RegisteredApp],
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    tokio::time::timeout(SIGN_IN_START, start(url, settings, apps, now))
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
    apps: &[RegisteredApp],
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    let serving = app_for(apps, url);
    let given = settings.client_id.as_deref();
    // Farik's client ids are for Farik's own apps' servers: another address is refused before any
    // request is made.
    if let Some(app) = apps.iter().find(|app| given == Some(app.client_id))
        && serving.is_none_or(|serving| serving.id != app.id)
    {
        return Err(SignInError::Failed(format!(
            "this sign-in is only for {}'s own servers",
            app.name
        )));
    }
    match serving {
        Some(app) if given.is_none_or(|given| given == app.client_id) => match app.flow {
            AppFlow::Device {
                device_endpoint,
                verification_uri,
            } => start_device(app, device_endpoint, verification_uri, url, now).await,
            // Its sign-in is for a connector Farik starts, not for an address.
            AppFlow::Loopback { .. } => Err(SignInError::NotSupported),
        },
        _ => start_redirect(url, settings, now).await,
    }
}

/// The device flow (RFC 8628) with one of Farik's own apps: asks the service for a code.
async fn start_device(
    app: &RegisteredApp,
    device_endpoint: &str,
    verification_uri: &str,
    url: &str,
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    let guard = Guarded::new()?;
    let host = host_of(device_endpoint);
    // No `scope`: what a GitHub App may do is set on the app.
    let answer = guard
        .post_token_form(device_endpoint, &[("client_id", app.client_id)])
        .await
        .map_err(|_| failed_at(&guard, &host, "ask for a sign-in code"))?;
    let body: serde_json::Value = serde_json::from_slice(answer.body()).unwrap_or_default();
    if body.get("error").is_some() || !answer.status().is_success() {
        return Err(SignInError::Failed(format!(
            "{host} refused to start the sign-in{}",
            guard.status_note()
        )));
    }
    let text = |name: &str| body[name].as_str().map(ToString::to_string);
    let (Some(device_code), Some(user_code), Some(page)) = (
        text("device_code"),
        text("user_code"),
        text("verification_uri"),
    ) else {
        return Err(SignInError::Failed(format!(
            "{host} sent a sign-in code Farik cannot read"
        )));
    };
    // The page the user is sent to, and told to type a code on, is the table's and no other.
    if page != verification_uri {
        return Err(SignInError::Failed(format!(
            "{host} named a sign-in page Farik does not use"
        )));
    }
    // RFC 8628's default is five seconds; never a busy loop.
    let interval = Duration::from_secs(body["interval"].as_u64().unwrap_or(5).max(1));
    Ok(SignIn {
        way: Way::Device(Device {
            code: Secret::new(device_code),
            user_code,
            interval,
        }),
        setup: Setup {
            guard,
            authorize_url: page,
            issuer: app.issuer.to_string(),
            resource: url.to_string(),
            client_id: app.client_id.to_string(),
            token_endpoint: app.token_endpoint.to_string(),
            revocation_endpoint: app.revocation_endpoint.map(ToString::to_string),
            app: Some(*app),
            started: tokio::time::Instant::now(),
            started_at: now,
        },
    })
}

/// Starts signing a user in for one of Farik's own connectors with `app`, whose flow is the
/// loopback one: the listener is bound, and the address the user is sent to carries the app's
/// client id, the redirect, the scopes (the app's own, or the team file's when each is one of
/// them), `state` and an S256 challenge. Nothing is requested yet.
///
/// # Errors
/// `NotSupported` for an app whose flow is not the loopback one, and `Failed` for a scope the app
/// does not ask for, an authorization address that is not https, or no free port.
pub async fn start_app_sign_in(
    app: &RegisteredApp,
    scopes: &[String],
    now: DateTime<Utc>,
) -> Result<SignIn, SignInError> {
    let AppFlow::Loopback {
        authorization_endpoint,
    } = app.flow
    else {
        return Err(SignInError::NotSupported);
    };
    let asked: Vec<String> = if scopes.is_empty() {
        app.scopes.iter().map(ToString::to_string).collect()
    } else {
        scopes.to_vec()
    };
    if asked
        .iter()
        .any(|scope| !app.scopes.contains(&scope.as_str()))
    {
        return Err(SignInError::Failed(format!(
            "Farik's {} sign-in asks only for {}",
            app.name,
            app.scopes.join(", ")
        )));
    }
    let mut authorize = Url::parse(authorization_endpoint)
        .ok()
        .filter(allowed)
        .ok_or_else(|| SignInError::Failed(format!("{authorization_endpoint} is not https")))?;
    let guard = Guarded::new()?;
    let (listeners, addr) = bind(&OAuthSettings {
        client_id: None,
        callback_port: None,
        scopes: Vec::new(),
    })
    .await?;
    let redirect_uri = format!("http://localhost:{}/callback", addr.port());
    let state = CsrfToken::new_random().secret().clone();
    // 48 random bytes make a verifier of 64 characters, within RFC 7636's 43 to 128.
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256_len(48);
    authorize
        .query_pairs_mut()
        .append_pair("client_id", app.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &asked.join(" "))
        .append_pair("state", &state)
        .append_pair("code_challenge", challenge.as_str())
        .append_pair("code_challenge_method", "S256");
    Ok(SignIn {
        way: Way::Loopback(Box::new(Loopback {
            listeners,
            addr,
            app: *app,
            state,
            verifier: Secret::new(verifier.secret().clone()),
            redirect_uri,
            scopes: asked,
        })),
        setup: Setup {
            guard,
            authorize_url: authorize.to_string(),
            issuer: app.issuer.to_string(),
            // Nothing reads it for an app's grant, whose refresh sends none.
            resource: app.issuer.to_string(),
            client_id: app.client_id.to_string(),
            token_endpoint: app.token_endpoint.to_string(),
            revocation_endpoint: app.revocation_endpoint.map(ToString::to_string),
            app: Some(*app),
            started: tokio::time::Instant::now(),
            started_at: now,
        },
    })
}

async fn start_redirect(
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
        way: Way::Redirect(Box::new(Redirect {
            listeners,
            addr,
            session,
            iss_promised,
            state,
            requested_scopes: field("scope")
                .map(|scope| scope.split_whitespace().map(ToString::to_string).collect())
                .unwrap_or_default(),
        })),
        setup: Setup {
            guard,
            authorize_url,
            issuer,
            resource,
            client_id,
            token_endpoint,
            revocation_endpoint,
            app: None,
            started: tokio::time::Instant::now(),
            started_at: now,
        },
    })
}

/// How long a grant with no known expiry is trusted for before it is refreshed.
const UNKNOWN_EXPIRY_TRUST: chrono::Duration = chrono::Duration::minutes(50);
/// How long asking a service to forget a grant may take.
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);

/// The `error` codes that mean the service ended the sign-in; the last two are GitHub's.
const ENDED: [&str; 5] = [
    "invalid_grant",
    "invalid_client",
    "unauthorized_client",
    "bad_refresh_token",
    "incorrect_client_credentials",
];

/// The grant refreshed, when it will not last `valid_for` more from `now`: `Ok(None)` when it
/// will. A grant with no refresh token is `Ok(None)` while its access token holds, and lapses once
/// it has expired. The rotated refresh token is the answer's; one left out keeps the old.
///
/// A grant of one of Farik's own apps (`app` set) is refreshed with no `resource`, and with the
/// client secret its entry of `apps` has, if it has one, at that entry's token endpoint and never
/// at the one the kept grant names, since the secret goes only where Farik's table says; a grant
/// whose app `apps` lacks has lapsed.
/// Every grant is asked for JSON, and an `error` in the answer is read whatever its status, since
/// GitHub answers one with status 200.
///
/// # Errors
/// `Lapsed` when the grant's app is not in `apps`, or the service ended the sign-in
/// (`invalid_grant`, `invalid_client`, `unauthorized_client`, `bad_refresh_token` or
/// `incorrect_client_credentials`); `Failed` for anything else, including `timeout` passing.
pub async fn refreshed(
    grant: &OAuthGrant,
    apps: &[RegisteredApp],
    now: DateTime<Utc>,
    valid_for: Duration,
    timeout: Duration,
) -> Result<Option<OAuthGrant>, SignInError> {
    if grant.lapsed {
        return Err(SignInError::Lapsed);
    }
    // A grant of an app the table lacks, as a build without Google's secret does, is over at once.
    let app = match grant.app.as_deref() {
        Some(id) => Some(
            apps.iter()
                .find(|app| app.id == id)
                .ok_or(SignInError::Lapsed)?,
        ),
        None => None,
    };
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
    let endpoint = app.map_or(grant.token_endpoint.as_str(), |app| app.token_endpoint);
    let host = host_of(endpoint);
    let guard = Guarded::new()?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.expose()),
        ("client_id", grant.client_id.as_str()),
    ];
    match app {
        None => form.push(("resource", grant.resource.as_str())),
        // The secret a service insists on for a client it calls public: looked up by the grant's
        // app on each refresh, and never kept in the grant.
        Some(app) => {
            if let Some(secret) = app.client_secret {
                form.push(("client_secret", secret));
            }
        }
    }
    let asking = guard.post_token_form(endpoint, &form);
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
    /// Whether the service should be asked to forget `self`, which `new` replaces. Not when `new`
    /// is the same grant, or the same client's: some services end every grant of a client when one
    /// is revoked, and that would end `new` with it.
    #[must_use]
    pub fn revocable_after(&self, new: &OAuthGrant) -> bool {
        self.client_id != new.client_id && self.access_token.expose() != new.access_token.expose()
    }

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
            "app": self.app,
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
            app: text("app"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default port is checked here, not by binding it: 33418 lies in the kernel's
    /// ephemeral range, so a test that binds it races every other test's port-0 listener.
    #[test]
    fn a_registered_client_without_a_port_gets_33418() {
        let mut settings = OAuthSettings {
            client_id: Some("abc".to_string()),
            callback_port: None,
            scopes: Vec::new(),
        };
        assert_eq!(fixed_port(&settings), Some(33418));
        settings.callback_port = Some(4000);
        assert_eq!(fixed_port(&settings), Some(4000));
        settings.client_id = None;
        assert_eq!(fixed_port(&settings), None);
    }

    /// A grant made with one of Farik's own apps is told so after a restart: its refresh depends on it.
    #[test]
    fn a_grant_keeps_its_app_in_the_stored_form() {
        let grant = OAuthGrant {
            issuer: "https://auth.example/oauth".to_string(),
            resource: "https://mcp.example/mcp/".to_string(),
            client_id: "the-apps-client-id".to_string(),
            token_endpoint: "https://auth.example/oauth/access_token".to_string(),
            revocation_endpoint: None,
            access_token: Secret::new("access".to_string()),
            refresh_token: Some(Secret::new("refresh".to_string())),
            issued_at: "2026-10-06T10:00:00Z".parse().expect("a time"),
            expires_at: Some("2026-10-06T18:00:00Z".parse().expect("a time")),
            scopes: Vec::new(),
            lapsed: false,
            app: Some("github".to_string()),
        };
        let read = OAuthGrant::from_json(&grant.to_json()).expect("a grant");
        assert_eq!(read, grant);
        assert_eq!(read.app.as_deref(), Some("github"));
    }

    /// What step 03 kept has no `app`: it reads as a grant of a client the service registered or the
    /// user gave, and its refresh still sends `resource` (`refresh_sends_the_kept_resource`, whose
    /// grant has `app: None`, is the same through `refreshed`).
    #[test]
    fn a_stored_grant_reads_without_app() {
        let mut stored = OAuthGrant {
            issuer: "https://auth.example".to_string(),
            resource: "https://mcp.example/mcp".to_string(),
            client_id: "client-1".to_string(),
            token_endpoint: "https://auth.example/token".to_string(),
            revocation_endpoint: Some("https://auth.example/revoke".to_string()),
            access_token: Secret::new("access".to_string()),
            refresh_token: Some(Secret::new("refresh".to_string())),
            issued_at: "2026-10-02T10:00:00Z".parse().expect("a time"),
            expires_at: None,
            scopes: vec!["read".to_string()],
            lapsed: false,
            app: None,
        }
        .to_json();
        // Step 03's form has no `app` key at all.
        stored.as_object_mut().expect("an object").remove("app");
        let grant = OAuthGrant::from_json(&stored).expect("a grant");
        assert_eq!(grant.app, None);
        assert_eq!(grant.client_id, "client-1");
    }

    /// rmcp refuses metadata without an issuer before Farik's own check runs, so the check is
    /// held up directly: the plan lists it as Farik's.
    #[test]
    fn refuses_metadata_that_does_not_name_its_issuer() {
        let mut metadata: AuthorizationMetadata = serde_json::from_value(serde_json::json!({
            "authorization_endpoint": "https://auth.example/authorize",
            "token_endpoint": "https://auth.example/token",
            "registration_endpoint": "https://auth.example/register",
            "code_challenge_methods_supported": ["S256"],
        }))
        .expect("metadata");
        let settings = OAuthSettings {
            client_id: None,
            callback_port: None,
            scopes: Vec::new(),
        };
        assert_eq!(
            check_metadata(&metadata, &settings),
            Err(SignInError::Failed(
                "the service did not say who it is".to_string()
            ))
        );
        metadata.issuer = Some("https://auth.example".to_string());
        assert_eq!(check_metadata(&metadata, &settings), Ok(()));
    }
}
