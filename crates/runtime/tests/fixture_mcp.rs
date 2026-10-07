//! MCP servers to connect to, and listing their tools: a stdio server written in `sh`, and a
//! streamable-HTTP server run in this process.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::http::request::Parts;
use farik_core::governor::permissions::ConnectorTag;
use farik_core::team::{CustomServer, CustomTransport};
use farik_runtime::claude::Secret;
use farik_runtime::connectors::{ConnectorError, ListedTool, call_tool, list_tools};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};

/// A stdio MCP server in `sh` (`fixtures/mcp_server.sh`). Its tools are `search`, `env`, whose
/// description is what the server sees of its environment, `delete_repo`, and `repo.delete`, a
/// name Claude Code would rewrite.
const STDIO_SERVER: &str = include_str!("fixtures/mcp_server.sh");

/// A stdio server that never answers, nor reads, so that only being killed ends it before a
/// minute: it writes its process id to `pid` beside itself first.
const SILENT_SERVER: &str = "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 60\n";

/// A stdio server that answers `initialize`, and answers `tools/list` with an error quoting its
/// key, as a server echoing what it was sent might.
const ERRING_SERVER: &str = r#"while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"erring","version":"1"}}}\n' "$id"
      ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32000,"message":"bad key %s"}}\n' "$id" "$API_KEY"
      ;;
  esac
done
"#;

/// A fresh folder for one test.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("farik-fixture-mcp-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder is made");
    dir
}

/// A folder the server runs in, which no test asserts on.
fn own_folder() -> PathBuf {
    std::env::temp_dir()
}

/// A custom server run as `sh <script>`: read by `sh`, never executed itself, so a test thread
/// forking meanwhile cannot make it "text file busy".
fn stdio_server(test: &str, script: &str, credential_keys: &[&str]) -> CustomServer {
    let path = scratch(test).join("server.sh");
    std::fs::write(&path, script).expect("the script is written");
    CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Stdio {
            command: "sh".to_string(),
            args: vec![path.display().to_string()],
            oauth: None,
        },
        credential_keys: credential_keys.iter().map(ToString::to_string).collect(),
        tools: BTreeMap::new(),
        kit: false,
        allowances: BTreeMap::new(),
    }
}

fn keys(pairs: &[(&str, &str)]) -> BTreeMap<String, Secret> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), Secret::new((*value).to_string())))
        .collect()
}

fn names(tools: &[ListedTool]) -> Vec<&str> {
    tools.iter().map(|tool| tool.name.as_str()).collect()
}

fn tool<'a>(tools: &'a [ListedTool], name: &str) -> &'a ListedTool {
    tools
        .iter()
        .find(|tool| tool.name == name)
        .unwrap_or_else(|| panic!("{name} is listed: {tools:?}"))
}

#[tokio::test]
async fn lists_a_stdio_servers_tools() {
    let server = stdio_server("lists", STDIO_SERVER, &[]);
    let tools = list_tools(
        &server,
        &BTreeMap::new(),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect("the tools are listed");
    assert_eq!(
        names(&tools),
        ["search", "env", "delete_repo", "repo.delete"]
    );
    assert_eq!(tool(&tools, "search").description, "Searches.");
}

#[tokio::test]
async fn the_server_sees_its_keys_and_not_the_model_key() {
    // The model keys must be in this process's own environment, which a test cannot set
    // (`unsafe_code` is forbidden): without them, the test runs itself as a child that has them.
    let model_keys = ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"];
    if model_keys.iter().any(|key| std::env::var_os(key).is_none()) {
        let status = std::process::Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "the_server_sees_its_keys_and_not_the_model_key",
                "--nocapture",
            ])
            .envs(model_keys.map(|key| (key, "model-secret")))
            .status()
            .expect("the test runs itself");
        assert!(status.success(), "the child run failed");
        return;
    }
    let server = stdio_server("env", STDIO_SERVER, &["API_KEY"]);
    let folder = scratch("env-folder");
    let tools = list_tools(
        &server,
        &keys(&[("API_KEY", "k")]),
        None,
        &folder,
        std::path::Path::new("farik"),
    )
    .await
    .expect("the tools are listed");
    assert_eq!(
        tool(&tools, "env").description,
        // `HOME` is one of the variables kept (`KEPT_ENV`); the server runs in the folder it is
        // given, never this process's (finding C1: an agent's worktree).
        format!(
            "PWD={} HOME=set API_KEY=k ANTHROPIC_API_KEY= CLAUDE_CODE_OAUTH_TOKEN=",
            folder.display()
        )
    );
}

/// An HTTP MCP server. Its tool `whoami` lists with the `Authorization` and `X-MCP-Readonly`
/// headers it was listed with as its description; the tools below answer when called: `structured` structured content that
/// differs from its text, `json_text` the text `{"b":2}`, `plain_text` the text `hello`,
/// `long_error` an error result of 2,000 characters, `rpc_error` a JSON-RPC error of 2,000
/// characters, `authorization` the `Authorization` header it was called with, and `sleeps` a call
/// that does not answer within the minute, after setting `sleeping`.
#[derive(Clone)]
struct HttpFixture {
    sleeping: Arc<std::sync::atomic::AtomicBool>,
}

const FIXTURE_TOOLS: [&str; 8] = [
    "whoami",
    "structured",
    "json_text",
    "plain_text",
    "long_error",
    "rpc_error",
    "authorization",
    "sleeps",
];

/// The header `name` the request in `context` carried, or `none`.
fn header_of(context: &RequestContext<RoleServer>, name: &str) -> String {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.headers.get(name))
        .and_then(|value| value.to_str().ok())
        .unwrap_or("none")
        .to_string()
}

fn authorization_of(context: &RequestContext<RoleServer>) -> String {
    header_of(context, "authorization")
}

impl ServerHandler for HttpFixture {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("fixture", "1");
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        // `whoami` lists with the two headers a narrowing server like GitHub's reads at listing.
        let seen = format!(
            "Authorization: {}; X-MCP-Readonly: {}",
            authorization_of(&context),
            header_of(&context, "x-mcp-readonly")
        );
        std::future::ready(Ok(ListToolsResult::with_all_items(
            FIXTURE_TOOLS
                .iter()
                .map(|name| {
                    let description = if *name == "whoami" {
                        seen.clone()
                    } else {
                        format!("The fixture's {name}.")
                    };
                    Tool::new(*name, description, Arc::new(JsonObject::new()))
                })
                .collect(),
        )))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + Send + '_ {
        let seen = authorization_of(&context);
        let text = |text: &str| vec![ContentBlock::text(text)];
        async move {
            Ok(match request.name.as_ref() {
                "structured" => {
                    let mut result = CallToolResult::success(text("not this"));
                    result.structured_content = Some(serde_json::json!({ "a": 1 }));
                    result
                }
                "json_text" => CallToolResult::success(text(r#"{"b":2}"#)),
                "plain_text" => CallToolResult::success(text("hello")),
                "long_error" => CallToolResult::error(text(&"e".repeat(2_000))),
                "rpc_error" => {
                    return Err(ErrorData::invalid_params("r".repeat(2_000), None));
                }
                "authorization" => CallToolResult::success(text(&seen)),
                "sleeps" => {
                    self.sleeping
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    CallToolResult::success(text("too late"))
                }
                other => return Err(ErrorData::invalid_params(format!("no tool {other}"), None)),
            }
            .into())
        }
    }
}

/// Serves `HttpFixture` on a free local port, and answers its `/mcp` address.
async fn http_server() -> String {
    http_server_watched(Arc::default()).await
}

/// `http_server`, whose tool `sleeps` sets `sleeping` when a call reaches it.
async fn http_server_watched(sleeping: Arc<std::sync::atomic::AtomicBool>) -> String {
    // No keep-alive pings: on a paused clock each one is due at once, and the stream answers
    // them in a loop that holds the runtime busy, so the clock only moves after 15 real seconds.
    let mut config = StreamableHttpServerConfig::default();
    config.sse_keep_alive = None;
    let service = StreamableHttpService::new(
        move || {
            Ok(HttpFixture {
                sleeping: Arc::clone(&sleeping),
            })
        },
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let router = axum::Router::new().route_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = listener.local_addr().expect("its address");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("http://{address}/mcp")
}

/// An http server that refuses every request with 401, its body quoting the `Authorization` it
/// was sent, as a server echoing what it was sent might. Answers its `/mcp` address.
async fn refusing_server() -> String {
    let router = axum::Router::new().route(
        "/mcp",
        axum::routing::any(|headers: axum::http::HeaderMap| async move {
            let sent = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("none")
                .to_string();
            (
                axum::http::StatusCode::UNAUTHORIZED,
                format!("refused the credential {sent}"),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = listener.local_addr().expect("its address");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("http://{address}/mcp")
}

/// The fixture server at `url`, with no header, key or sign-in of its own.
fn http_fixture_at(url: String) -> CustomServer {
    CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Http {
            url,
            headers: BTreeMap::new(),
            oauth: None,
        },
        credential_keys: Vec::new(),
        tools: BTreeMap::new(),
        kit: false,
        allowances: BTreeMap::new(),
    }
}

#[tokio::test]
async fn fills_http_headers_from_keys() {
    let server = CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Http {
            url: http_server().await,
            headers: BTreeMap::from([
                ("Authorization".to_string(), "Bearer {API_KEY}".to_string()),
                // A header that names no key, which a narrowing server reads at listing.
                ("X-MCP-Readonly".to_string(), "true".to_string()),
            ]),
            oauth: None,
        },
        credential_keys: vec!["API_KEY".to_string()],
        tools: BTreeMap::from([("whoami".to_string(), ConnectorTag::Network)]),
        kit: false,
        allowances: BTreeMap::new(),
    };
    let tools = list_tools(
        &server,
        &keys(&[("API_KEY", "k")]),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect("the tools are listed");
    assert_eq!(
        tool(&tools, "whoami").description,
        "Authorization: Bearer k; X-MCP-Readonly: true"
    );

    // A key the header names, with no value kept for it, is refused before anything is sent.
    assert_eq!(
        list_tools(
            &server,
            &BTreeMap::new(),
            None,
            &own_folder(),
            std::path::Path::new("farik")
        )
        .await,
        Err(ConnectorError::KeyMissing("API_KEY".to_string()))
    );
}

#[tokio::test]
async fn marks_a_tool_name_claude_code_would_rewrite() {
    let server = stdio_server("marks", STDIO_SERVER, &[]);
    let tools = list_tools(
        &server,
        &BTreeMap::new(),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect("the tools are listed");
    assert!(tool(&tools, "search").usable);
    assert!(tool(&tools, "env").usable);
    assert!(!tool(&tools, "repo.delete").usable);
}

#[tokio::test(start_paused = true)]
async fn gives_up_after_thirty_seconds() {
    let server = stdio_server("silent", SILENT_SERVER, &[]);
    let CustomTransport::Stdio { args, .. } = &server.transport else {
        panic!("a stdio server");
    };
    let pid_file = PathBuf::from(&args[0]).with_file_name("pid");
    let started = tokio::time::Instant::now();
    // Paused time does not move while a blocking task runs: this one holds it until the server
    // has started and said who it is.
    let watched = pid_file.clone();
    let (no_keys, folder) = (BTreeMap::new(), own_folder());
    let (listed, pid) = tokio::join!(
        list_tools(
            &server,
            &no_keys,
            None,
            &folder,
            std::path::Path::new("farik")
        ),
        tokio::task::spawn_blocking(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                match std::fs::read_to_string(&watched) {
                    Ok(pid) if pid.ends_with('\n') => return pid,
                    _ if std::time::Instant::now() > deadline => return String::new(),
                    _ => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        })
    );
    assert_eq!(listed, Err(ConnectorError::Timeout));
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(30) && waited < Duration::from_secs(31),
        "{waited:?}"
    );
    // The server given up on is killed, never left running with the keys in its environment
    // (carry R2). Dead is gone, or a zombie not yet reaped.
    let pid = pid.expect("the watch ends");
    assert!(!pid.is_empty(), "the server never wrote its pid");
    let state = || {
        std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(") ")
                    .and_then(|(_, rest)| rest.chars().next())
            })
    };
    // Waited for in real time, on a blocking thread so the runtime, which the kill is a task of,
    // stays free to run it: on the paused clock a `tokio::time::sleep` passes all of its 5 seconds
    // at once, before the kill has had any real time to land.
    for _ in 0..250 {
        if matches!(state(), None | Some('Z')) {
            break;
        }
        tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(20)))
            .await
            .expect("the wait ends");
    }
    assert!(matches!(state(), None | Some('Z')), "{:?}", state());
}

#[tokio::test]
async fn a_servers_own_error_text_is_not_repeated() {
    // A server's error may quote what it was sent; the reply a person reads says only what
    // failed (carry M12).
    let server = stdio_server("erring", ERRING_SERVER, &["API_KEY"]);
    let failed = list_tools(
        &server,
        &keys(&[("API_KEY", "k-secret-value")]),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect_err("the listing fails");
    let ConnectorError::Failed(said) = &failed else {
        panic!("{failed:?}");
    };
    assert!(
        said.starts_with("fixture could not list its tools"),
        "{said}"
    );
    assert!(!said.contains("k-secret-value"), "{said}");
    assert!(!said.contains("bad key"), "{said}");
}

#[tokio::test]
async fn a_servers_refusal_at_connect_is_not_repeated() {
    // The server refuses the handshake and quotes the key it was sent; the reply a person reads
    // says only that it did not answer as an MCP server.
    let server = CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Http {
            url: refusing_server().await,
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer {API_KEY}".to_string(),
            )]),
            oauth: None,
        },
        credential_keys: vec!["API_KEY".to_string()],
        tools: BTreeMap::new(),
        kit: false,
        allowances: BTreeMap::new(),
    };
    let failed = list_tools(
        &server,
        &keys(&[("API_KEY", "k-secret-value")]),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect_err("the connection fails");
    assert_eq!(
        failed,
        ConnectorError::Failed("fixture did not answer as an MCP server".to_string())
    );
    let said = format!("{failed:?}");
    assert!(!said.contains("k-secret-value"), "{said}");
    assert!(!said.contains("refused the credential"), "{said}");
}

/// No arguments for a tool that takes none.
fn no_arguments() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::new()
}

#[tokio::test]
async fn calls_a_tool_and_reads_its_answer() {
    let server = http_fixture_at(http_server().await);
    let called = |tool: &'static str| {
        let server = server.clone();
        async move {
            call_tool(
                &server,
                &BTreeMap::new(),
                None,
                &own_folder(),
                std::path::Path::new("farik"),
                tool,
                no_arguments(),
            )
            .await
        }
    };
    // Structured content is the answer, whatever the text says.
    assert_eq!(
        called("structured").await,
        Ok(serde_json::json!({ "a": 1 }))
    );
    // Else the first text block, read as JSON when it is.
    assert_eq!(called("json_text").await, Ok(serde_json::json!({ "b": 2 })));
    assert_eq!(
        called("plain_text").await,
        Ok(serde_json::json!({ "text": "hello" }))
    );
}

#[tokio::test]
async fn a_tool_error_keeps_its_words_cut() {
    let server = http_fixture_at(http_server().await);
    for (tool, kept) in [("long_error", "e"), ("rpc_error", "r")] {
        let called = call_tool(
            &server,
            &BTreeMap::new(),
            None,
            &own_folder(),
            std::path::Path::new("farik"),
            tool,
            no_arguments(),
        )
        .await;
        assert_eq!(
            called,
            Err(ConnectorError::ToolError {
                text: kept.repeat(500)
            }),
            "{tool}"
        );
    }
}

#[tokio::test]
async fn a_stdio_server_gets_only_its_keys() {
    // The model keys must be in this process's own environment, which a test cannot set
    // (`unsafe_code` is forbidden): without them, the test runs itself as a child that has them.
    let model_keys = ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"];
    if model_keys.iter().any(|key| std::env::var_os(key).is_none()) {
        let status = std::process::Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "a_stdio_server_gets_only_its_keys",
                "--nocapture",
            ])
            .envs(model_keys.map(|key| (key, "model-secret")))
            .status()
            .expect("the test runs itself");
        assert!(status.success(), "the child run failed");
        return;
    }
    let server = stdio_server("call-env", STDIO_SERVER, &["API_KEY"]);
    let folder = scratch("call-env-folder");
    let answer = call_tool(
        &server,
        &keys(&[("API_KEY", "k")]),
        None,
        &folder,
        std::path::Path::new("farik"),
        "env",
        no_arguments(),
    )
    .await
    .expect("the tool answers");
    assert_eq!(
        answer["text"],
        format!(
            "PWD={} HOME=set API_KEY=k ANTHROPIC_API_KEY= CLAUDE_CODE_OAUTH_TOKEN=",
            folder.display()
        )
    );
}

#[tokio::test]
async fn an_http_server_gets_the_bearer() {
    let server = http_fixture_at(http_server().await);
    let answer = call_tool(
        &server,
        &BTreeMap::new(),
        Some(&Secret::new("tok-given".to_string())),
        &own_folder(),
        std::path::Path::new("farik"),
        "authorization",
        no_arguments(),
    )
    .await;
    assert_eq!(
        answer,
        Ok(serde_json::json!({ "text": "Bearer tok-given" }))
    );
}

#[tokio::test(start_paused = true)]
async fn a_call_gives_up_after_thirty_seconds() {
    let sleeping = Arc::<std::sync::atomic::AtomicBool>::default();
    let server = http_fixture_at(http_server_watched(Arc::clone(&sleeping)).await);
    let started = tokio::time::Instant::now();
    // Paused time does not move while a blocking task runs: this one holds it until the call has
    // reached the tool, so the thirty seconds are the call's and not the connecting's.
    let watched = Arc::clone(&sleeping);
    let (no_keys, folder) = (BTreeMap::new(), own_folder());
    let (called, reached) = tokio::join!(
        call_tool(
            &server,
            &no_keys,
            None,
            &folder,
            std::path::Path::new("farik"),
            "sleeps",
            no_arguments(),
        ),
        tokio::task::spawn_blocking(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !watched.load(std::sync::atomic::Ordering::SeqCst) {
                if std::time::Instant::now() > deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            true
        })
    );
    assert!(reached.expect("the watch ends"), "the call never arrived");
    assert_eq!(called, Err(ConnectorError::Timeout));
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(30) && waited < Duration::from_secs(31),
        "{waited:?}"
    );
}

/// The Developer's kit with the stdio fixture server as its one service, `fixture`, tagging
/// `search`, `env` and `delete_repo`: what a kit's pins look like against a server that lists
/// them (and `repo.delete`, a name no kit can tag, which Farik can't use).
fn fixture_kit(test: &str) -> farik_roles::Kit {
    let script = scratch(test).join("server.sh");
    std::fs::write(&script, STDIO_SERVER).expect("the script is written");
    let file = serde_json::json!({
        "role": "software_developer", "skills": [],
        "connectors": [{
            "name": "fixture", "transport": "stdio", "command": "sh",
            "args": [script.display().to_string()],
            "title": "Fixture", "about": "A stand-in.", "why": "To pin.", "setup": "Nothing to do.",
            "tools": { "search": "network", "env": "external_effect", "delete_repo": "denied" },
        }],
    });
    farik_roles::parse_fixture_kit(
        farik_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &[],
    )
    .expect("the fixture kit loads")
}

#[tokio::test]
async fn pin_drift_is_empty_for_the_fixture_server() {
    let kit = fixture_kit("pins");
    let Some(farik_roles::KitConnector::Server { entry, .. }) = kit.connectors.first() else {
        panic!("the fixture kit has a service");
    };
    let mut entry = entry.clone();
    entry.source = farik_core::team::McpServerSource::Kit;
    let server = farik_core::team::custom_server(&entry).expect("a kit entry");
    let listed = list_tools(
        &server,
        &BTreeMap::new(),
        None,
        &own_folder(),
        std::path::Path::new("farik"),
    )
    .await
    .expect("the tools are listed");
    // A tool whose name Claude Code would rewrite is not one Farik offers, so no pin names it.
    let usable: Vec<String> = listed
        .iter()
        .filter(|tool| tool.usable)
        .map(|tool| tool.name.clone())
        .collect();
    let drift = farik_roles::pin_drift(&server.tools, &usable);
    assert_eq!((drift.added, drift.removed), (Vec::new(), Vec::new()));
    // A pin the service does not list is dropped, and a tool it lists with no pin is added.
    let mut pins = server.tools.clone();
    pins.insert("sync_everything".to_string(), ConnectorTag::Denied);
    pins.remove("env");
    let drift = farik_roles::pin_drift(&pins, &usable);
    assert_eq!(drift.removed, ["sync_everything"]);
    assert_eq!(drift.added, ["env"]);
}
