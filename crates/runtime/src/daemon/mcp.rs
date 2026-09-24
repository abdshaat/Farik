//! Farik's tools over MCP, for the session the request's headers name: an axum middleware
//! resolves `X-Farik-Session` to its registration, and the handler reads it back from the HTTP
//! request parts `rmcp` puts in each request's context.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

use super::DaemonState;
use crate::tools::refusal::Refusal;
use crate::tools::{ToolContext, call_tool, tool_descriptors};

/// The header naming the Farik session a request to `/mcp` comes from.
pub(crate) const SESSION_HEADER: &str = "x-farik-session";
/// The name of the permission-prompt tool, `mcp__farik__permission` to Claude Code.
const PERMISSION_TOOL: &str = "permission";
/// What the permission-prompt tool answers, to everything.
const PERMISSION_MESSAGE: &str =
    "farik decides tool calls in its PreToolUse hook; this one was not allowed there";

/// What the tools of the session a request comes from are called with, as
/// `DaemonState::tool_context` built it, and the Farik tools the session was given, put in the
/// request's extensions by `require_session`.
#[derive(Clone)]
pub(crate) struct CallingSession {
    context: Arc<ToolContext>,
    farik_tools: Arc<Vec<String>>,
}

impl CallingSession {
    /// Whether the session was given the Farik tool `name`.
    fn was_given(&self, name: &str) -> bool {
        self.farik_tools.iter().any(|given| given == name)
    }
}

/// Answers 403 for a request whose `X-Farik-Session` names no registered session, and passes
/// the registration on otherwise.
pub(crate) async fn require_session(
    State(state): State<Arc<DaemonState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let named = request
        .headers()
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let calling = named.and_then(|session_id| {
        Some(CallingSession {
            context: Arc::new(state.tool_context(&session_id)?),
            farik_tools: Arc::new(state.farik_tools(&session_id)?),
        })
    });
    match calling {
        Some(calling) => {
            request.extensions_mut().insert(calling);
            next.run(request).await
        }
        None => (
            StatusCode::FORBIDDEN,
            "the daemon answers for no session of that name",
        )
            .into_response(),
    }
}

/// Farik's MCP server: `list_tools` and `call_tool` over the session's own tools of
/// `tool_descriptors` and `call_tool`, and the permission-prompt tool. The session a call comes
/// from, and the project it works on, are the request's.
#[derive(Clone)]
pub(crate) struct FarikMcp;

impl ServerHandler for FarikMcp {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("farik", env!("CARGO_PKG_VERSION"));
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(calling_session(&context).map(|calling| listed(&calling)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name == PERMISSION_TOOL {
            let answer = json!({ "behavior": "deny", "message": PERMISSION_MESSAGE });
            return Ok(
                CallToolResult::success(vec![ContentBlock::text(answer.to_string())]).into(),
            );
        }
        let calling = calling_session(&context)?;
        if !calling.was_given(&request.name) {
            let refusal = Refusal::ToolNotInSession {
                tool: request.name.to_string(),
            };
            return Ok(CallToolResult::error(vec![ContentBlock::text(refusal.reason())]).into());
        }
        let input = request.arguments.map_or_else(|| json!({}), Value::Object);
        let result = match call_tool(&calling.context, &request.name, input).await {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        };
        Ok(result.into())
    }
}

/// The session a request comes from, which `require_session` put in its extensions.
fn calling_session(context: &RequestContext<RoleServer>) -> Result<CallingSession, ErrorData> {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.extensions.get::<CallingSession>())
        .cloned()
        .ok_or_else(|| {
            ErrorData::invalid_request("the request names no session Farik answers for", None)
        })
}

/// The Farik tools the session was given, and the permission-prompt tool.
///
/// Protocol `2026-07-28` requires a list result to say how long it stays fresh and who may
/// cache it, and Claude Code 2.1.280 drops every tool of a list without them. The list is the
/// session's own, so only its client may cache it, and it is fresh for no time at all, because
/// a session ends and the list with it. `rmcp` sends both fields to older peers too, whose
/// schemas allow a result any other field.
fn listed(calling: &CallingSession) -> ListToolsResult {
    let mut tools: Vec<Tool> = tool_descriptors()
        .into_iter()
        .filter(|tool| calling.was_given(tool.name))
        .map(|tool| Tool::new(tool.name, tool.description, object(tool.input_schema)))
        .collect();
    tools.push(Tool::new(
        PERMISSION_TOOL,
        "Answers Claude Code's permission prompts: every one is denied, because Farik decides \
             tool calls in its PreToolUse hook.",
        object(json!({
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "tool_use_id": { "type": "string" }
            }
        })),
    ));
    ListToolsResult::with_all_items(tools)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}

/// The names `tools/list` answers the session `session_id` with, as its client sees them, or
/// `None` for a session the daemon does not know: `require_session` and `listed` without HTTP.
#[cfg(test)]
pub(crate) fn listed_names(state: &DaemonState, session_id: &str) -> Option<Vec<String>> {
    let calling = CallingSession {
        context: Arc::new(state.tool_context(session_id)?),
        farik_tools: Arc::new(state.farik_tools(session_id)?),
    };
    Some(
        listed(&calling)
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect(),
    )
}

/// A schema as the object MCP carries it in; a schema that is not an object is carried empty.
fn object(schema: Value) -> Arc<JsonObject> {
    Arc::new(match schema {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use farik_protocol::event::EventKind;
    use serde_json::{Value, json};
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use farik_core::budget::DEFAULT_SESSION_LIMITS;

    use crate::daemon::fixtures::{DEV_SESSION, TestDaemon};
    use crate::daemon::{decide_pre_tool_use, router};
    use crate::tools::tool_descriptors;

    const TOKEN: &str = "a-token";
    const VERSION: &str = "2025-06-18";
    /// The version Claude Code 2.1.280 speaks, measured on 2026-09-23: no `initialize` and no
    /// MCP session, `server/discover` first, and every request naming the version in its
    /// `_meta` and its headers.
    const MODERN_VERSION: &str = "2026-07-28";

    /// One MCP client of the router: a legacy one does `initialize` first, then sends every
    /// request with the session id it answered; a modern one, as Claude Code 2.1.280 is, names
    /// the protocol version on every request instead.
    struct Client {
        app: Router,
        farik_session: String,
        mcp_session: Option<String>,
        next_id: u64,
        modern: bool,
    }

    impl Client {
        fn new(daemon: &TestDaemon, farik_session: &str) -> Self {
            Self {
                app: router(daemon.state.clone(), TOKEN, CancellationToken::new()),
                farik_session: farik_session.to_string(),
                mcp_session: None,
                next_id: 1,
                modern: false,
            }
        }

        /// A client of protocol `2026-07-28`.
        fn modern(daemon: &TestDaemon, farik_session: &str) -> Self {
            Self {
                modern: true,
                ..Self::new(daemon, farik_session)
            }
        }

        async fn send(&mut self, message: &Value) -> (StatusCode, Option<Value>) {
            let mut request = Request::post("/mcp")
                .header("Host", "127.0.0.1")
                .header("Accept", "application/json, text/event-stream")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .header("X-Farik-Session", &self.farik_session);
            if let Some(session) = &self.mcp_session {
                request = request
                    .header("Mcp-Session-Id", session)
                    .header("MCP-Protocol-Version", VERSION);
            }
            if self.modern {
                request = request.header("MCP-Protocol-Version", MODERN_VERSION);
                if let Some(method) = message["method"].as_str() {
                    request = request.header("Mcp-Method", method);
                }
                if let Some(name) = message["params"]["name"].as_str() {
                    request = request.header("Mcp-Name", name);
                }
            }
            let answer = tokio::time::timeout(
                Duration::from_secs(10),
                self.app.clone().oneshot(
                    request
                        .body(Body::from(message.to_string()))
                        .expect("a request is built"),
                ),
            )
            .await
            .expect("the router answers in time")
            .expect("the router answers");
            let status = answer.status();
            if let Some(session) = answer.headers().get("Mcp-Session-Id") {
                self.mcp_session = Some(session.to_str().expect("text").to_string());
            }
            let bytes = tokio::time::timeout(
                Duration::from_secs(10),
                to_bytes(answer.into_body(), usize::MAX),
            )
            .await
            .expect("the body ends in time")
            .expect("a body");
            let text = String::from_utf8(bytes.to_vec()).expect("text");
            (status, message_in(&text))
        }

        async fn request(&mut self, method: &str, mut params: Value) -> Value {
            let id = self.next_id;
            self.next_id += 1;
            if self.modern {
                params["_meta"] = json!({
                    "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "claude-code",
                        "version": "2.1.280"
                    }
                });
            }
            let (status, answer) = self
                .send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
                .await;
            assert_eq!(status, StatusCode::OK, "{method}: {answer:?}");
            let answer = answer.expect("an answer");
            assert_eq!(answer["id"], json!(id), "{answer}");
            answer
        }

        async fn initialize(&mut self) {
            let answer = self
                .request(
                    "initialize",
                    json!({
                        "protocolVersion": VERSION,
                        "capabilities": {},
                        "clientInfo": { "name": "claude-code", "version": "2.1.280" }
                    }),
                )
                .await;
            assert!(
                answer["result"]["capabilities"]["tools"].is_object(),
                "{answer}"
            );
            let (status, _) = self
                .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await;
            assert!(status.is_success(), "{status}");
        }

        async fn call(&mut self, tool: &str, arguments: Value) -> Value {
            self.request(
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await
        }
    }

    /// The JSON-RPC message in a body: the body itself, or the last `data:` line of an event
    /// stream that holds one.
    fn message_in(text: &str) -> Option<Value> {
        serde_json::from_str(text).ok().or_else(|| {
            text.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
                .rfind(|message| message.get("jsonrpc").is_some())
        })
    }

    /// The text a tool answered with.
    fn text_of(answer: &Value) -> String {
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("a text answer: {answer}"))
            .to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reads_the_calling_session_from_the_request_headers() {
        let daemon = TestDaemon::new("mcp-session", |_| {});
        let mut client = Client::new(&daemon, DEV_SESSION);
        client.initialize().await;
        let answer = client
            .call(
                "farik_ask_human",
                json!({ "question": "Should the login page remember the user?" }),
            )
            .await;
        assert_ne!(answer["result"]["isError"], json!(true), "{answer}");
        let asked = daemon.events(EventKind::QuestionAsked);
        assert_eq!(asked.len(), 1);
        assert_eq!(
            asked[0].envelope.ids.session_id.as_deref(),
            Some(DEV_SESSION)
        );
        assert_eq!(asked[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));
        assert_eq!(
            asked[0]
                .envelope
                .ids
                .task_id
                .as_ref()
                .map(|id| id.to_string())
                .as_deref(),
            Some("FRK-1")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_tool_s_failure_as_an_error_result() {
        let daemon = TestDaemon::new("mcp-error", |_| {});
        let mut client = Client::new(&daemon, DEV_SESSION);
        client.initialize().await;
        let answer = client.call("farik_no_such_tool", json!({})).await;
        assert_eq!(answer["result"]["isError"], json!(true), "{answer}");
        assert!(text_of(&answer).contains("farik_no_such_tool"), "{answer}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_answering_for_a_session_that_ended() {
        let daemon = TestDaemon::new("mcp-ended", |_| {});
        daemon.state.end_session(DEV_SESSION);
        assert_eq!(daemon.state.tool_calls(DEV_SESSION), None);
        let decision = decide_pre_tool_use(
            &daemon.dev_call("Read", &json!({ "file_path": daemon.inside("src/a.rs") })),
            &daemon.state,
        );
        assert!(!decision.allow, "{decision:?}");
        assert!(
            decision.reason.starts_with("unknown_session: "),
            "{decision:?}"
        );
        let mut client = Client::new(&daemon, DEV_SESSION);
        let (status, _) = client
            .send(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "claude-code", "version": "2.1.280" }
                }
            }))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn lists_every_farik_tool_and_the_permission_tool() {
        let daemon = TestDaemon::new("mcp-list", |_| {});
        let mut client = Client::new(&daemon, DEV_SESSION);
        client.initialize().await;
        let answer = client.request("tools/list", json!({})).await;
        let names: Vec<&str> = answer["result"]["tools"]
            .as_array()
            .expect("a list of tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        let mut expected: Vec<&str> = tool_descriptors().iter().map(|tool| tool.name).collect();
        assert_eq!(expected.len(), 26);
        expected.push("permission");
        assert_eq!(names, expected);
        for tool in answer["result"]["tools"].as_array().expect("a list") {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_modern_client_as_protocol_2026_07_28_requires() {
        let daemon = TestDaemon::new("mcp-modern", |_| {});
        let mut client = Client::modern(&daemon, DEV_SESSION);
        let discovered = client.request("server/discover", json!({})).await;
        assert!(
            discovered["result"]["supportedVersions"]
                .as_array()
                .is_some_and(|versions| versions.contains(&json!(MODERN_VERSION))),
            "{discovered}"
        );
        let listed = client.request("tools/list", json!({})).await;
        assert_eq!(
            listed["result"]["resultType"],
            json!("complete"),
            "{listed}"
        );
        assert_eq!(listed["result"]["ttlMs"], json!(0), "{listed}");
        assert_eq!(listed["result"]["cacheScope"], json!("private"), "{listed}");
        let called = client.call("farik_read_board", json!({})).await;
        assert_eq!(
            called["result"]["resultType"],
            json!("complete"),
            "{called}"
        );
        assert_ne!(called["result"]["isError"], json!(true), "{called}");
        assert!(text_of(&called).contains("FRK-1"), "{called}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_a_session_only_the_tools_it_was_given() {
        let daemon = TestDaemon::new("mcp-session-tools", |_| {});
        daemon.register_with_tools(
            "session-triage",
            "pm",
            Some("FRK-1"),
            DEFAULT_SESSION_LIMITS,
            &["farik_triage_request"],
        );
        let mut client = Client::new(&daemon, "session-triage");
        client.initialize().await;
        let answer = client.request("tools/list", json!({})).await;
        let names: Vec<&str> = answer["result"]["tools"]
            .as_array()
            .expect("a list of tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(names, ["farik_triage_request", "permission"]);
        let refused = client
            .call(
                "farik_write_contract",
                json!({ "fields": { "intent": "More." } }),
            )
            .await;
        assert_eq!(refused["result"]["isError"], json!(true), "{refused}");
        assert!(
            text_of(&refused).starts_with("tool_not_in_session: farik_write_contract"),
            "{refused}"
        );
        assert!(daemon.events(EventKind::ContractWritten).is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn calls_a_tool_as_the_registered_session() {
        let daemon = TestDaemon::new("mcp-call", |_| {});
        let mut client = Client::new(&daemon, DEV_SESSION);
        client.initialize().await;
        let answer = client.call("farik_read_board", json!({})).await;
        assert_ne!(answer["result"]["isError"], json!(true), "{answer}");
        let board: Value = serde_json::from_str(&text_of(&answer)).expect("the board is JSON");
        assert!(board.to_string().contains("FRK-1"), "{board}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_mcp_for_an_unregistered_session() {
        let daemon = TestDaemon::new("mcp-unknown", |_| {});
        let mut client = Client::new(&daemon, "nobody");
        let (status, _) = client
            .send(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "claude-code", "version": "2.1.280" }
                }
            }))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn denies_every_permission_prompt() {
        let daemon = TestDaemon::new("mcp-permission", |_| {});
        let mut client = Client::new(&daemon, DEV_SESSION);
        client.initialize().await;
        let answer = client
            .call(
                "permission",
                json!({ "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "toolu_1" }),
            )
            .await;
        let said: Value = serde_json::from_str(&text_of(&answer)).expect("the answer is JSON");
        assert_eq!(
            said,
            json!({
                "behavior": "deny",
                "message": "farik decides tool calls in its PreToolUse hook; this one was not allowed there"
            })
        );
    }
}
