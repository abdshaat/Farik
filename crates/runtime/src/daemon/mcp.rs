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
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

use super::DaemonState;
use crate::exec::Executor;
use crate::tools::{ToolContext, ToolDeps, call_tool, tool_descriptors};

/// The header naming the Farik session a request to `/mcp` comes from.
pub(crate) const SESSION_HEADER: &str = "x-farik-session";
/// The name of the permission-prompt tool, `mcp__farik__permission` to Claude Code.
const PERMISSION_TOOL: &str = "permission";
/// What the permission-prompt tool answers, to everything.
const PERMISSION_MESSAGE: &str =
    "farik decides tool calls in its PreToolUse hook; this one was not allowed there";

/// The session a request comes from, as its registration says, put in the request's extensions
/// by `require_session`.
#[derive(Clone)]
pub(crate) struct CallingSession {
    session_id: String,
    agent_id: String,
    task_id: Option<farik_core::contract::TaskId>,
    executor: Option<Arc<dyn Executor>>,
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
        state
            .sessions()
            .get(&session_id)
            .map(|session| CallingSession {
                session_id,
                agent_id: session.registration.agent_id.clone(),
                task_id: session.registration.task_id.clone(),
                executor: session.registration.executor.clone(),
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

/// Farik's MCP server: `list_tools` and `call_tool` over `tool_descriptors` and `call_tool`,
/// and the permission-prompt tool.
#[derive(Clone)]
pub(crate) struct FarikMcp {
    deps: Arc<ToolDeps>,
}

impl FarikMcp {
    pub(crate) fn new(deps: Arc<ToolDeps>) -> Self {
        Self { deps }
    }
}

impl ServerHandler for FarikMcp {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("farik", env!("CARGO_PKG_VERSION"));
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(listed()))
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
        let calling = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.extensions.get::<CallingSession>())
            .cloned()
            .ok_or_else(|| {
                ErrorData::invalid_request("the request names no session Farik answers for", None)
            })?;
        let tool_context = ToolContext {
            agent_id: calling.agent_id,
            task_id: calling.task_id,
            session_id: calling.session_id,
            executor: calling.executor,
            deps: Arc::clone(&self.deps),
        };
        let input = request.arguments.map_or_else(|| json!({}), Value::Object);
        let result = match call_tool(&tool_context, &request.name, input).await {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        };
        Ok(result.into())
    }
}

/// Every Farik tool, and the permission-prompt tool.
fn listed() -> ListToolsResult {
    let mut tools: Vec<Tool> = tool_descriptors()
        .into_iter()
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

    use crate::daemon::fixtures::{DEV_SESSION, TestDaemon};
    use crate::daemon::{decide_pre_tool_use, router};
    use crate::tools::tool_descriptors;

    const TOKEN: &str = "a-token";
    const VERSION: &str = "2025-06-18";

    /// One MCP client of the router, as Claude Code is one: `initialize` first, then every
    /// request with the session id it answered.
    struct Client {
        app: Router,
        farik_session: String,
        mcp_session: Option<String>,
        next_id: u64,
    }

    impl Client {
        fn new(daemon: &TestDaemon, farik_session: &str) -> Self {
            Self {
                app: router(daemon.state.clone(), TOKEN, CancellationToken::new()),
                farik_session: farik_session.to_string(),
                mcp_session: None,
                next_id: 1,
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

        async fn request(&mut self, method: &str, params: Value) -> Value {
            let id = self.next_id;
            self.next_id += 1;
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
        assert_eq!(expected.len(), 19);
        expected.push("permission");
        assert_eq!(names, expected);
        for tool in answer["result"]["tools"].as_array().expect("a list") {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
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
