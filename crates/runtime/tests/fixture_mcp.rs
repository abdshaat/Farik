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
use farik_runtime::connectors::{ConnectorError, ListedTool, list_tools};
use rmcp::model::{
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};

/// A stdio MCP server in `sh`, one JSON-RPC message per line. Its tools are `search`, `env`,
/// whose description is what the server sees of its environment, and `repo.delete`, a name
/// Claude Code would rewrite.
const STDIO_SERVER: &str = r#"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      version=$(printf '%s' "$line" | sed -n 's/.*"protocolVersion":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"%s","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}}\n' "$id" "$version"
      ;;
    *'"method":"tools/list"'*)
      seen="HOME=${HOME:+set} API_KEY=${API_KEY-} ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY-} CLAUDE_CODE_OAUTH_TOKEN=${CLAUDE_CODE_OAUTH_TOKEN-}"
      schema='{"type":"object"}'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","description":"Searches.","inputSchema":%s},{"name":"env","description":"%s","inputSchema":%s},{"name":"repo.delete","description":"Deletes a repository.","inputSchema":%s}]}}\n' "$id" "$schema" "$seen" "$schema" "$schema"
      ;;
  esac
done
"#;

/// A stdio server that reads and never answers.
const SILENT_SERVER: &str = "cat > /dev/null\n";

/// A fresh folder for one test.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("farik-fixture-mcp-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder is made");
    dir
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
        },
        credential_keys: credential_keys.iter().map(ToString::to_string).collect(),
        tools: BTreeMap::new(),
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
    let tools = list_tools(&server, &BTreeMap::new())
        .await
        .expect("the tools are listed");
    assert_eq!(names(&tools), ["search", "env", "repo.delete"]);
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
    let tools = list_tools(&server, &keys(&[("API_KEY", "k")]))
        .await
        .expect("the tools are listed");
    assert_eq!(
        tool(&tools, "env").description,
        // `HOME` is one of the variables kept (`KEPT_ENV`).
        "HOME=set API_KEY=k ANTHROPIC_API_KEY= CLAUDE_CODE_OAUTH_TOKEN="
    );
}

/// An HTTP MCP server with one tool, whose description is the `Authorization` header it was
/// listed with.
#[derive(Clone)]
struct HttpFixture;

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
        let seen = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.headers.get("authorization"))
            .and_then(|value| value.to_str().ok())
            .unwrap_or("none")
            .to_string();
        std::future::ready(Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "whoami",
            seen,
            Arc::new(JsonObject::new()),
        )])))
    }
}

/// Serves `HttpFixture` on a free local port, and answers its `/mcp` address.
async fn http_server() -> String {
    let service = StreamableHttpService::new(
        || Ok(HttpFixture),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().route_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = listener.local_addr().expect("its address");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("http://{address}/mcp")
}

#[tokio::test]
async fn fills_http_headers_from_keys() {
    let server = CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Http {
            url: http_server().await,
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer {API_KEY}".to_string(),
            )]),
        },
        credential_keys: vec!["API_KEY".to_string()],
        tools: BTreeMap::from([("whoami".to_string(), ConnectorTag::Network)]),
    };
    let tools = list_tools(&server, &keys(&[("API_KEY", "k")]))
        .await
        .expect("the tools are listed");
    assert_eq!(tool(&tools, "whoami").description, "Bearer k");

    // A key the header names, with no value kept for it, is refused before anything is sent.
    assert_eq!(
        list_tools(&server, &BTreeMap::new()).await,
        Err(ConnectorError::KeyMissing("API_KEY".to_string()))
    );
}

#[tokio::test]
async fn marks_a_tool_name_claude_code_would_rewrite() {
    let server = stdio_server("marks", STDIO_SERVER, &[]);
    let tools = list_tools(&server, &BTreeMap::new())
        .await
        .expect("the tools are listed");
    assert!(tool(&tools, "search").usable);
    assert!(tool(&tools, "env").usable);
    assert!(!tool(&tools, "repo.delete").usable);
}

#[tokio::test(start_paused = true)]
async fn gives_up_after_thirty_seconds() {
    let server = stdio_server("silent", SILENT_SERVER, &[]);
    let started = tokio::time::Instant::now();
    assert_eq!(
        list_tools(&server, &BTreeMap::new()).await,
        Err(ConnectorError::Timeout)
    );
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(30) && waited < Duration::from_secs(31),
        "{waited:?}"
    );
}
