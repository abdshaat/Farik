//! An OAuth authorization server and a protected MCP server, in one axum server on loopback,
//! for the sign-in tests (phase 7 step 03). It records every request and has flags for the ways
//! a real service differs. It is `#[path]`-included by the tests that need it, so it uses only
//! what every including crate has: `axum`, `rmcp`, `reqwest`, `base64`, `sha2` and `tokio`.
#![allow(dead_code, missing_docs, clippy::pedantic)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use rmcp::model::{
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use sha2::{Digest, Sha256};

/// What the PKCE support in the metadata says.
#[derive(Clone, PartialEq, Eq)]
pub enum Methods {
    S256,
    Plain,
    Absent,
}

/// What `iss` the callback carries.
#[derive(Clone, PartialEq, Eq)]
pub enum Iss {
    Correct,
    Other(String),
    Absent,
}

/// What a tool of the protected MCP server answers when it is called.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolAnswer {
    /// A result with this text.
    Text(String),
    /// A result whose structured content is this value.
    Json(serde_json::Value),
    /// A result marked as an error, with this text.
    Error(String),
    /// No MCP result at all: the HTTP answer of a service that turns the call away, with this
    /// status, a `Retry-After` of sixty seconds and this body (a service over its rate limit
    /// answers `429` so).
    Http(u16, String),
}

/// The ways this server can differ from a well-behaved one.
#[derive(Clone)]
pub struct Flags {
    pub dcr: bool,
    pub methods: Methods,
    pub iss: Iss,
    /// `authorization_response_iss_parameter_supported` in the metadata.
    pub iss_promised: bool,
    pub challenge: bool,
    pub prm: bool,
    pub as_metadata: bool,
    pub access_denied: bool,
    pub error_description: Option<String>,
    pub revocation: bool,
    pub revoke_status: u16,
    pub authorization_endpoint: Option<String>,
    pub metadata_redirect: Option<String>,
    /// Where `/register` redirects to, when it does.
    pub register_redirect: Option<String>,
    pub no_issuer: bool,
    /// What `/token` answers a refresh with, when it is not a success.
    pub refresh_error: Option<(u16, String)>,
    /// A refresh answer without a new refresh token.
    pub keep_refresh_token: bool,
    pub expires_in: u64,
    /// A token answer with no `expires_in`.
    pub no_expiry: bool,
    /// Whether the tool's description is the `Authorization` header it was listed with.
    pub echo_authorization: bool,
    /// What each tool answers when called, by name; a tool with no answer here answers
    /// `whoami-ok`.
    pub tool_answers: BTreeMap<String, ToolAnswer>,
    /// How many polls of each device code answer `authorization_pending` before it is approved.
    pub device_pending: u32,
    /// Whether a device code's first poll answers `slow_down`, once.
    pub device_slow_down: bool,
    /// What every poll answers instead of approval, when set (`access_denied`, `expired_token`).
    pub device_error: Option<String>,
    /// The `verification_uri` a device code is answered with, when not the fixture's own.
    pub device_verification_uri: Option<String>,
    /// The `interval` a device code is answered with, in seconds; none leaves it out.
    pub device_interval: Option<u64>,
    /// How each device code's first polls are answered, in order, before its usual answers: true
    /// is a gateway's 502 page that is not JSON, false is the usual answer.
    pub device_bad_gateways: Vec<bool>,
    /// Whether that 502 carries JSON with no `error` in it, as some gateways answer, in place of
    /// a page.
    pub device_gateway_json: bool,
}

impl Default for Flags {
    fn default() -> Flags {
        Flags {
            dcr: true,
            methods: Methods::S256,
            iss: Iss::Correct,
            iss_promised: false,
            challenge: true,
            prm: true,
            as_metadata: true,
            access_denied: false,
            error_description: None,
            revocation: true,
            revoke_status: 200,
            authorization_endpoint: None,
            metadata_redirect: None,
            register_redirect: None,
            no_issuer: false,
            refresh_error: None,
            keep_refresh_token: false,
            expires_in: 3600,
            no_expiry: false,
            echo_authorization: true,
            tool_answers: BTreeMap::new(),
            device_pending: 0,
            device_slow_down: false,
            device_error: None,
            device_verification_uri: None,
            device_interval: Some(1),
            device_bad_gateways: Vec::new(),
            device_gateway_json: false,
        }
    }
}

/// One request the server saw.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub body: String,
    pub form: BTreeMap<String, String>,
    pub authorization: Option<String>,
    /// Every header, its name in lower case.
    pub headers: BTreeMap<String, String>,
    /// When the server saw it, on the clock the test runs on (paused, when the test paused it).
    pub at: tokio::time::Instant,
}

/// A device code the server issued: how many polls are still to answer `authorization_pending`,
/// and whether `slow_down` was answered yet.
struct Device {
    pending_left: u32,
    slowed: bool,
    /// The polls still to be answered by the script of `Flags::device_bad_gateways`.
    script: std::collections::VecDeque<bool>,
}

struct Code {
    challenge: String,
    client_id: String,
    redirect_uri: String,
}

struct Shared {
    origin: String,
    flags: Mutex<Flags>,
    requests: Mutex<Vec<Recorded>>,
    codes: Mutex<HashMap<String, Code>>,
    access: Mutex<HashSet<String>>,
    refresh: Mutex<HashSet<String>>,
    clients: Mutex<HashMap<String, Vec<String>>>,
    devices: Mutex<HashMap<String, Device>>,
    counter: AtomicU64,
    held: tokio::sync::watch::Sender<HashSet<String>>,
    /// Every `tools/call`: the tool and the arguments it carried, in order.
    calls: Mutex<Vec<(String, JsonObject)>>,
}

pub struct Fixture {
    pub origin: String,
    pub mcp_url: String,
    shared: Arc<Shared>,
}

#[derive(Clone)]
struct Tools(Arc<Shared>);

impl ServerHandler for Tools {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("fixture", "1");
        info
    }

    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::CallToolResponse, ErrorData>> + Send + '_ {
        let shared = self.0.clone();
        async move {
            let name = request.name.to_string();
            shared
                .calls
                .lock()
                .expect("calls")
                .push((name.clone(), request.arguments.clone().unwrap_or_default()));
            // `hold("tool:<name>")` keeps this tool from answering until `release`d.
            shared.wait_if_held(&format!("tool:{name}")).await;
            let answer = shared.flags().tool_answers.get(&name).cloned();
            let result = match answer {
                None => {
                    rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
                        "whoami-ok",
                    )])
                }
                Some(ToolAnswer::Text(text)) => {
                    rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
                        text,
                    )])
                }
                Some(ToolAnswer::Json(value)) => rmcp::model::CallToolResult::structured(value),
                Some(ToolAnswer::Error(text)) => {
                    rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(text)])
                }
                // Answered before the call reached this handler, by `front`.
                Some(ToolAnswer::Http(..)) => {
                    rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
                        "whoami-ok",
                    )])
                }
            };
            Ok(result.into())
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        let seen = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.headers.get("authorization"))
            .and_then(|value| value.to_str().ok())
            .unwrap_or("none")
            .to_string();
        let seen = if self.0.flags().echo_authorization {
            seen
        } else {
            "Says who is asking.".to_string()
        };
        std::future::ready(Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "whoami",
            seen,
            Arc::new(JsonObject::new()),
        )])))
    }
}

fn parse_pairs(text: &str) -> BTreeMap<String, String> {
    reqwest::Url::parse(&format!("http://x/?{text}"))
        .map(|url| url.query_pairs().into_owned().collect())
        .unwrap_or_default()
}

fn json(status: u16, value: serde_json::Value) -> Response {
    (
        StatusCode::from_u16(status).expect("a status"),
        [(header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

impl Shared {
    fn flags(&self) -> Flags {
        self.flags.lock().expect("flags").clone()
    }

    fn next(&self, prefix: &str) -> String {
        format!(
            "{prefix}-{}",
            self.counter.fetch_add(1, Ordering::SeqCst) + 1
        )
    }

    async fn wait_if_held(&self, route: &str) {
        let mut rx = self.held.subscribe();
        let _ = rx.wait_for(|held| !held.contains(route)).await;
    }

    fn mint(&self) -> (String, String) {
        let access = self.next("at");
        let refresh = self.next("rt");
        self.access.lock().expect("access").insert(access.clone());
        self.refresh
            .lock()
            .expect("refresh")
            .insert(refresh.clone());
        (access, refresh)
    }
}

async fn front(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, 1 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let query = parts.uri.query().map(parse_pairs).unwrap_or_default();
    let is_form = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"));
    let recorded = Recorded {
        method: parts.method.to_string(),
        path: parts.uri.path().to_string(),
        query: query.clone(),
        form: if is_form {
            parse_pairs(&text)
        } else {
            BTreeMap::new()
        },
        body: text,
        authorization: parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string),
        headers: parts
            .headers
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_string(), value.to_str().ok()?.to_string()))
            })
            .collect(),
        at: tokio::time::Instant::now(),
    };
    shared
        .requests
        .lock()
        .expect("requests")
        .push(recorded.clone());
    let flags = shared.flags();
    let path = recorded.path.clone();
    if path == "/mcp" {
        let bearer = recorded
            .authorization
            .as_deref()
            .and_then(|value| value.strip_prefix("Bearer "));
        let good =
            bearer.is_some_and(|token| shared.access.lock().expect("access").contains(token));
        if !good {
            let mut response = StatusCode::UNAUTHORIZED.into_response();
            if flags.challenge {
                let value = format!(
                    "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"",
                    shared.origin
                );
                response.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_str(&value).expect("a header"),
                );
            }
            return response;
        }
        if let Some(turned_away) = turns_away(&shared, &flags, &recorded.body) {
            return turned_away;
        }
        return next
            .run(Request::from_parts(parts, Body::from(bytes)))
            .await;
    }
    route(&shared, &flags, &recorded).await
}

/// The HTTP answer of a `tools/call` whose tool is set to `ToolAnswer::Http`, which is recorded as
/// a call of that tool all the same.
fn turns_away(shared: &Shared, flags: &Flags, body: &str) -> Option<Response> {
    let call: serde_json::Value = serde_json::from_str(body).ok()?;
    if call["method"] != "tools/call" {
        return None;
    }
    let name = call["params"]["name"].as_str()?;
    let Some(ToolAnswer::Http(status, words)) = flags.tool_answers.get(name) else {
        return None;
    };
    let arguments = call["params"]["arguments"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    shared
        .calls
        .lock()
        .expect("calls")
        .push((name.to_string(), arguments));
    Some(
        (
            StatusCode::from_u16(*status).expect("a status"),
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::RETRY_AFTER, "60"),
            ],
            words.clone(),
        )
            .into_response(),
    )
}

async fn route(shared: &Arc<Shared>, flags: &Flags, request: &Recorded) -> Response {
    let origin = &shared.origin;
    match (request.method.as_str(), request.path.as_str()) {
        (
            "GET",
            "/.well-known/oauth-protected-resource" | "/.well-known/oauth-protected-resource/mcp",
        ) => {
            shared.wait_if_held("prm").await;
            if !flags.prm {
                return StatusCode::NOT_FOUND.into_response();
            }
            json(
                200,
                serde_json::json!({
                    "resource": format!("{origin}/mcp"),
                    "authorization_servers": [origin],
                    "scopes_supported": ["read", "write"],
                }),
            )
        }
        ("GET", "/.well-known/oauth-authorization-server") => {
            if let Some(to) = &flags.metadata_redirect {
                return (StatusCode::FOUND, [(header::LOCATION, to.clone())]).into_response();
            }
            if !flags.as_metadata {
                return StatusCode::NOT_FOUND.into_response();
            }
            let mut metadata = serde_json::json!({
                "authorization_endpoint": flags
                    .authorization_endpoint
                    .clone()
                    .unwrap_or_else(|| format!("{origin}/authorize")),
                "token_endpoint": format!("{origin}/token"),
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "token_endpoint_auth_methods_supported": ["none"],
                "scopes_supported": ["read", "write"],
            });
            if !flags.no_issuer {
                metadata["issuer"] = origin.clone().into();
            }
            if flags.dcr {
                metadata["registration_endpoint"] = format!("{origin}/register").into();
            }
            if flags.revocation {
                metadata["revocation_endpoint"] = format!("{origin}/revoke").into();
            }
            match flags.methods {
                Methods::S256 => {
                    metadata["code_challenge_methods_supported"] = serde_json::json!(["S256"])
                }
                Methods::Plain => {
                    metadata["code_challenge_methods_supported"] = serde_json::json!(["plain"])
                }
                Methods::Absent => {}
            }
            if flags.iss_promised {
                metadata["authorization_response_iss_parameter_supported"] = true.into();
            }
            json(200, metadata)
        }
        ("POST", "/register") => {
            shared.wait_if_held("register").await;
            if let Some(to) = &flags.register_redirect {
                return (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, to.clone())],
                )
                    .into_response();
            }
            let Ok(body) = serde_json::from_str::<serde_json::Value>(&request.body) else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            let id = shared.next("client");
            let redirects: Vec<String> = body["redirect_uris"]
                .as_array()
                .map(|uris| {
                    uris.iter()
                        .filter_map(|uri| uri.as_str().map(ToString::to_string))
                        .collect()
                })
                .unwrap_or_default();
            shared
                .clients
                .lock()
                .expect("clients")
                .insert(id.clone(), redirects.clone());
            json(
                201,
                serde_json::json!({
                    "client_id": id,
                    "redirect_uris": redirects,
                    "token_endpoint_auth_method": "none",
                    "grant_types": ["authorization_code", "refresh_token"],
                    "response_types": ["code"],
                }),
            )
        }
        ("GET", "/authorize") => authorize(shared, flags, request),
        ("POST", "/device/code") => device_code(shared, flags, request),
        ("POST", "/token") => {
            shared.wait_if_held("token").await;
            token(shared, flags, request)
        }
        ("POST", "/revoke") => {
            shared.wait_if_held("revoke").await;
            StatusCode::from_u16(flags.revoke_status)
                .expect("a status")
                .into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn authorize(shared: &Arc<Shared>, flags: &Flags, request: &Recorded) -> Response {
    let query = &request.query;
    let (Some(redirect), Some(state), Some(client)) = (
        query.get("redirect_uri"),
        query.get("state"),
        query.get("client_id"),
    ) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Some(registered) = shared.clients.lock().expect("clients").get(client)
        && !registered.contains(redirect)
    {
        return (StatusCode::BAD_REQUEST, "redirect_uri is not registered").into_response();
    }
    let mut target = reqwest::Url::parse(redirect).expect("a redirect uri");
    {
        let mut pairs = target.query_pairs_mut();
        if flags.access_denied {
            pairs.append_pair("error", "access_denied");
            if let Some(description) = &flags.error_description {
                pairs.append_pair("error_description", description);
            }
        } else {
            let code = shared.next("code");
            shared.codes.lock().expect("codes").insert(
                code.clone(),
                Code {
                    challenge: query.get("code_challenge").cloned().unwrap_or_default(),
                    client_id: client.clone(),
                    redirect_uri: redirect.clone(),
                },
            );
            pairs.append_pair("code", &code);
        }
        pairs.append_pair("state", state);
        match &flags.iss {
            Iss::Correct => {
                pairs.append_pair("iss", &shared.origin);
            }
            Iss::Other(other) => {
                pairs.append_pair("iss", other);
            }
            Iss::Absent => {}
        }
    }
    (StatusCode::FOUND, [(header::LOCATION, target.to_string())]).into_response()
}

/// The grant type of a device code's poll (RFC 8628).
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// An answer as a device-flow service gives it: always status 200, as GitHub does for an error
/// too, JSON when the request asked for it with `Accept: application/json` and form-encoded when
/// it did not.
fn device_answer(request: &Recorded, value: serde_json::Value) -> Response {
    let asked_for_json = request
        .headers
        .get("accept")
        .is_some_and(|accept| accept.contains("application/json"));
    if asked_for_json {
        return json(200, value);
    }
    let mut encoded = reqwest::Url::parse("http://x/").expect("a base address");
    if let Some(fields) = value.as_object() {
        let mut pairs = encoded.query_pairs_mut();
        for (name, field) in fields {
            match field {
                serde_json::Value::String(text) => pairs.append_pair(name, text),
                other => pairs.append_pair(name, &other.to_string()),
            };
        }
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-www-form-urlencoded")],
        encoded.query().unwrap_or_default().to_string(),
    )
        .into_response()
}

/// `POST /device/code`: a new device code, `dc-<n>`, to be polled every second.
fn device_code(shared: &Arc<Shared>, flags: &Flags, request: &Recorded) -> Response {
    if !request.form.contains_key("client_id") {
        return device_answer(
            request,
            serde_json::json!({ "error": "incorrect_client_credentials" }),
        );
    }
    let code = shared.next("dc");
    shared.devices.lock().expect("devices").insert(
        code.clone(),
        Device {
            pending_left: flags.device_pending,
            slowed: !flags.device_slow_down,
            script: flags.device_bad_gateways.iter().copied().collect(),
        },
    );
    let number = code.trim_start_matches("dc-");
    let mut answer = serde_json::json!({
        "device_code": code,
        "user_code": format!("WDJB-{number:0>4}"),
        "verification_uri": flags
            .device_verification_uri
            .clone()
            .unwrap_or_else(|| format!("{}/login/device", shared.origin)),
        "expires_in": 900,
    });
    if let Some(interval) = flags.device_interval {
        answer["interval"] = interval.into();
    }
    device_answer(request, answer)
}

/// A poll of a device code at `/token`: pending, slowed down once, refused, or approved.
fn device_poll(shared: &Arc<Shared>, flags: &Flags, request: &Recorded) -> Response {
    let answer = |value| device_answer(request, value);
    let Some(code) = request.form.get("device_code") else {
        return answer(serde_json::json!({ "error": "incorrect_device_code" }));
    };
    let mut devices = shared.devices.lock().expect("devices");
    let Some(device) = devices.get_mut(code) else {
        return answer(serde_json::json!({ "error": "incorrect_device_code" }));
    };
    if device.script.pop_front() == Some(true) {
        if flags.device_gateway_json {
            return json(502, serde_json::json!({ "message": "Server Error" }));
        }
        return (
            StatusCode::BAD_GATEWAY,
            [(header::CONTENT_TYPE, "text/html")],
            "<html><body><h1>502 Bad Gateway</h1></body></html>",
        )
            .into_response();
    }
    if !device.slowed {
        device.slowed = true;
        return answer(serde_json::json!({ "error": "slow_down", "interval": 10 }));
    }
    if let Some(error) = &flags.device_error {
        return answer(serde_json::json!({ "error": error }));
    }
    if device.pending_left > 0 {
        device.pending_left -= 1;
        return answer(serde_json::json!({ "error": "authorization_pending" }));
    }
    devices.remove(code);
    drop(devices);
    let (access, refresh) = shared.mint();
    let mut approved = serde_json::json!({
        "access_token": access,
        "token_type": "bearer",
        // A GitHub App's answer has none; a service that names one is not believed.
        "scope": "read",
        "refresh_token": refresh,
        "refresh_token_expires_in": 15_811_200,
    });
    if !flags.no_expiry {
        approved["expires_in"] = flags.expires_in.into();
    }
    answer(approved)
}

fn token(shared: &Arc<Shared>, flags: &Flags, request: &Recorded) -> Response {
    let form = &request.form;
    let grant = form.get("grant_type").map(String::as_str);
    let bad = |error: &str| json(400, serde_json::json!({ "error": error }));
    let scope = "read write offline_access";
    let (access, refresh) = match grant {
        Some("authorization_code") => {
            let Some(code) = form
                .get("code")
                .and_then(|code| shared.codes.lock().expect("codes").remove(code))
            else {
                return bad("invalid_grant");
            };
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let hashed = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_bytes()));
            if hashed != code.challenge
                || form.get("client_id") != Some(&code.client_id)
                || form.get("redirect_uri") != Some(&code.redirect_uri)
            {
                return bad("invalid_grant");
            }
            shared.mint()
        }
        Some(DEVICE_GRANT) => return device_poll(shared, flags, request),
        Some("refresh_token") => {
            if let Some((status, error)) = &flags.refresh_error {
                return json(*status, serde_json::json!({ "error": error }));
            }
            let Some(presented) = form.get("refresh_token") else {
                return bad("invalid_request");
            };
            if !shared.refresh.lock().expect("refresh").remove(presented) {
                return bad("invalid_grant");
            }
            let (access, refresh) = shared.mint();
            if flags.keep_refresh_token {
                shared
                    .refresh
                    .lock()
                    .expect("refresh")
                    .insert(presented.clone());
                (access, String::new())
            } else {
                (access, refresh)
            }
        }
        _ => return bad("unsupported_grant_type"),
    };
    let mut answer = serde_json::json!({
        "access_token": access,
        "token_type": "Bearer",
        "scope": scope,
    });
    if !flags.no_expiry {
        answer["expires_in"] = flags.expires_in.into();
    }
    if !refresh.is_empty() {
        answer["refresh_token"] = refresh.into();
    }
    json(200, answer)
}

impl Fixture {
    pub async fn start() -> Fixture {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let origin = format!("http://{}", listener.local_addr().expect("its address"));
        let (held, _) = tokio::sync::watch::channel(HashSet::new());
        let shared = Arc::new(Shared {
            origin: origin.clone(),
            flags: Mutex::new(Flags::default()),
            requests: Mutex::new(Vec::new()),
            codes: Mutex::new(HashMap::new()),
            access: Mutex::new(HashSet::new()),
            refresh: Mutex::new(HashSet::new()),
            clients: Mutex::new(HashMap::new()),
            devices: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
            held,
            calls: Mutex::new(Vec::new()),
        });
        let for_tools = shared.clone();
        // No keep-alive pings: on a paused clock each one is due at once, and the stream answers
        // them in a loop that holds the runtime busy, so the clock only moves after 15 real
        // seconds.
        let mut config = StreamableHttpServerConfig::default();
        config.sse_keep_alive = None;
        let service = StreamableHttpService::new(
            move || Ok(Tools(for_tools.clone())),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let router = axum::Router::new()
            .route_service("/mcp", service)
            .layer(middleware::from_fn_with_state(shared.clone(), front));
        tokio::spawn(async move { axum::serve(listener, router).await });
        Fixture {
            mcp_url: format!("{origin}/mcp"),
            origin,
            shared,
        }
    }

    /// Changes the flags.
    pub fn set(&self, change: impl FnOnce(&mut Flags)) {
        change(&mut self.shared.flags.lock().expect("flags"));
    }

    /// Makes `route` (`prm`, `register`, `token`, `revoke`, or `tool:<name>` for a tool's answer)
    /// wait until `release`d.
    pub fn hold(&self, route: &str) {
        self.shared.held.send_modify(|held| {
            held.insert(route.to_string());
        });
    }

    pub fn release(&self, route: &str) {
        self.shared.held.send_modify(|held| {
            held.remove(route);
        });
    }

    /// An access token and a refresh token the server will honour.
    pub fn mint(&self) -> (String, String) {
        self.shared.mint()
    }

    pub fn requests(&self, path: &str) -> Vec<Recorded> {
        self.shared
            .requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|request| request.path == path)
            .cloned()
            .collect()
    }

    pub fn count(&self, path: &str) -> usize {
        self.requests(path).len()
    }

    /// Every request the server saw, in order.
    pub fn seen(&self) -> Vec<Recorded> {
        self.shared.requests.lock().expect("requests").clone()
    }

    /// The arguments each `tools/call` of `tool` carried, in order.
    pub fn calls(&self, tool: &str) -> Vec<JsonObject> {
        self.shared
            .calls
            .lock()
            .expect("calls")
            .iter()
            .filter(|(name, _)| name == tool)
            .map(|(_, arguments)| arguments.clone())
            .collect()
    }

    /// Every request body and query the server saw, as one text, for a search for a token.
    pub fn everything_seen(&self) -> String {
        format!("{:?}", self.shared.requests.lock().expect("requests"))
    }
}

/// What following an authorization address gave.
pub struct Followed {
    /// The callback's status, headers and page.
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub page: String,
    pub location: String,
}

/// Follows `authorize_url` as a browser would: the authorization server's redirect is read, not
/// followed, and then requested.
pub async fn follow(authorize_url: &str) -> Followed {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("a client");
    let redirect = client
        .get(authorize_url)
        .send()
        .await
        .expect("the authorization page");
    let location = redirect
        .headers()
        .get(header::LOCATION)
        .unwrap_or_else(|| panic!("a redirect, got {}", redirect.status()))
        .to_str()
        .expect("text")
        .to_string();
    callback(&location).await
}

/// Requests a callback address.
pub async fn callback(location: &str) -> Followed {
    let response = reqwest::get(location).await.expect("the callback answers");
    Followed {
        status: response.status().as_u16(),
        headers: response.headers().clone(),
        page: String::new(),
        location: location.to_string(),
    }
    .with_page(response)
    .await
}

impl Followed {
    async fn with_page(mut self, response: reqwest::Response) -> Followed {
        self.page = response.text().await.unwrap_or_default();
        self
    }
}
