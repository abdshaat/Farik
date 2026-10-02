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
    let tools = list_tools(&server, &BTreeMap::new(), &own_folder())
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
    let tools = list_tools(&server, &keys(&[("API_KEY", "k")]), &folder)
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
            oauth: None,
        },
        credential_keys: vec!["API_KEY".to_string()],
        tools: BTreeMap::from([("whoami".to_string(), ConnectorTag::Network)]),
    };
    let tools = list_tools(&server, &keys(&[("API_KEY", "k")]), &own_folder())
        .await
        .expect("the tools are listed");
    assert_eq!(tool(&tools, "whoami").description, "Bearer k");

    // A key the header names, with no value kept for it, is refused before anything is sent.
    assert_eq!(
        list_tools(&server, &BTreeMap::new(), &own_folder()).await,
        Err(ConnectorError::KeyMissing("API_KEY".to_string()))
    );
}

#[tokio::test]
async fn marks_a_tool_name_claude_code_would_rewrite() {
    let server = stdio_server("marks", STDIO_SERVER, &[]);
    let tools = list_tools(&server, &BTreeMap::new(), &own_folder())
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
        list_tools(&server, &no_keys, &folder),
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
    // Polled on the runtime, which the kill may be a task of.
    for _ in 0..250 {
        if matches!(state(), None | Some('Z')) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
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
        &own_folder(),
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
