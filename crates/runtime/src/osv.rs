//! Farik's own MCP server over the open vulnerability database (`docs/SPEC.md` 6.7, ADR 0038),
//! started by `farik connector osv`. It speaks to one fixed address, follows no redirect and uses
//! no proxy, checks every input before it leaves, and sends OSV only a package's name, ecosystem
//! and version, or an advisory's id. What OSV answers is data the agent reads under the
//! untrusted-content notice.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

/// OSV's address. Never an argument, an environment value or a tool input: the tests pass a
/// fixture's address to [`Osv::new`], not to the command.
pub const OSV_API: &str = "https://api.osv.dev/v1";

/// Why the server could not run.
#[derive(Debug)]
pub enum OsvError {
    /// The web client could not be made.
    Client,
    /// The server could not start, or ended with an error.
    Serving(String),
}

impl fmt::Display for OsvError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => write!(formatter, "the web client could not be made"),
            Self::Serving(why) => write!(formatter, "the OSV server stopped: {why}"),
        }
    }
}

impl std::error::Error for OsvError {}

/// The tools the server lists, in the order it lists them.
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    vec!["query_package", "query_packages", "get_vulnerability"]
}

/// How long one call to OSV may take, which OSV may spend up to twenty seconds of before it pages.
const TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 25 });
/// The most of an answer Farik reads.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// The most advisories `query_package` answers with.
const MAX_ADVISORIES: usize = 50;
/// The most packages `query_packages` takes.
const MAX_PACKAGES: usize = 100;
/// The most characters of an advisory's `details` `get_vulnerability` answers with.
const MAX_DETAILS: usize = 8_000;
/// The most `affected` entries `get_vulnerability` answers with.
const MAX_AFFECTED: usize = 20;

/// The server, speaking to one address.
#[derive(Clone)]
pub struct Osv {
    api: reqwest::Url,
    client: reqwest::Client,
}

/// A package to ask about, every field checked.
struct Package {
    ecosystem: String,
    name: String,
    version: String,
}

impl Package {
    /// The package `input` names, or what is wrong with it.
    fn of(input: &Value) -> Result<Self, String> {
        let field = |key: &str, most: usize| -> Result<String, String> {
            let text = input[key]
                .as_str()
                .ok_or_else(|| format!("{key} is needed, as text"))?;
            let length = text.chars().count();
            if length == 0 || length > most {
                return Err(format!("{key} is 1 to {most} characters"));
            }
            Ok(text.to_string())
        };
        let ecosystem = field("ecosystem", 40)?;
        if !ecosystem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:_ -".contains(c))
        {
            return Err("ecosystem is letters, digits and . : _ - or a space".to_string());
        }
        let name = field("name", 214)?;
        let version = field("version", 128)?;
        if name.chars().any(char::is_control) || version.chars().any(char::is_control) {
            return Err("name and version hold no control character".to_string());
        }
        Ok(Self {
            ecosystem,
            name,
            version,
        })
    }

    /// The query OSV takes for it.
    fn query(&self) -> Value {
        json!({
            "package": { "name": self.name, "ecosystem": self.ecosystem },
            "version": self.version
        })
    }
}

/// Whether `id` is one advisory's id: 1 to 64 characters, the first a letter or a digit, then
/// letters, digits and `-._:`, so it is one path segment.
fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 64
        && chars
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "-._:".contains(c))
}

impl Osv {
    /// A server that asks `api`.
    ///
    /// # Errors
    ///
    /// The address is not a web address, or the web client could not be made.
    pub fn new(api: &str) -> Result<Self, OsvError> {
        let client = reqwest::Client::builder()
            // Nothing OSV answers sends Farik anywhere else, and nothing is sent through a proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| OsvError::Client)?;
        let api = reqwest::Url::parse(api).map_err(|_| OsvError::Client)?;
        Ok(Self { api, client })
    }

    /// Runs `tool` with `input`: the answer as JSON, or why it was refused, in words.
    ///
    /// # Errors
    ///
    /// The input is not valid, OSV could not be reached or answered badly.
    pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String> {
        match tool {
            "query_package" => self.query_package(input).await,
            "query_packages" => self.query_packages(input).await,
            "get_vulnerability" => self.get_vulnerability(input).await,
            _ => Err(format!("there is no tool named {tool}")),
        }
    }

    async fn query_package(&self, input: &Value) -> Result<Value, String> {
        let package = Package::of(input)?;
        let answer = self.ask(&["query"], Some(&package.query())).await?;
        let found = answer["vulns"].as_array().map_or(&[][..], Vec::as_slice);
        let advisories: Vec<Value> = found
            .iter()
            .take(MAX_ADVISORIES)
            .map(|vuln| {
                json!({
                    "id": vuln["id"],
                    "summary": vuln["summary"],
                    "aliases": vuln["aliases"],
                    "severity": vuln["severity"],
                    "fixed": fixed_versions(vuln, &package),
                })
            })
            .collect();
        let more = found.len() > MAX_ADVISORIES || answer["next_page_token"].is_string();
        Ok(json!({ "advisories": advisories, "more": more }))
    }

    async fn query_packages(&self, input: &Value) -> Result<Value, String> {
        let asked = input["packages"]
            .as_array()
            .filter(|list| (1..=MAX_PACKAGES).contains(&list.len()))
            .ok_or_else(|| format!("packages is a list of 1 to {MAX_PACKAGES} packages"))?;
        let packages = asked
            .iter()
            .map(Package::of)
            .collect::<Result<Vec<_>, _>>()?;
        let queries: Vec<Value> = packages.iter().map(Package::query).collect();
        let answer = self
            .ask(&["querybatch"], Some(&json!({ "queries": queries })))
            .await?;
        let results = answer["results"].as_array().map_or(&[][..], Vec::as_slice);
        if results.len() != packages.len() {
            return Err("OSV answered for a different number of packages".to_string());
        }
        let results: Vec<Value> = packages
            .iter()
            .zip(results)
            .map(|(package, result)| {
                let ids: Vec<&Value> = result["vulns"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|vuln| &vuln["id"])
                    .collect();
                json!({
                    "ecosystem": package.ecosystem,
                    "name": package.name,
                    "version": package.version,
                    "advisories": ids,
                    "more": result["next_page_token"].is_string(),
                })
            })
            .collect();
        Ok(json!({ "results": results }))
    }

    async fn get_vulnerability(&self, input: &Value) -> Result<Value, String> {
        let id = input["id"].as_str().filter(|id| valid_id(id)).ok_or(
            "id is 1 to 64 characters: letters, digits and - . _ :, the first a letter or digit",
        )?;
        let mut record = self.ask(&["vulns", id], None).await?;
        let Some(object) = record.as_object_mut() else {
            return Err("OSV's answer is not a record".to_string());
        };
        if let Some(details) = object.get("details").and_then(Value::as_str)
            && details.chars().count() > MAX_DETAILS
        {
            let cut: String = details.chars().take(MAX_DETAILS).collect();
            object.insert("details".to_string(), Value::from(cut));
            object.insert("details_cut".to_string(), Value::from(true));
        }
        if let Some(affected) = object.get_mut("affected").and_then(Value::as_array_mut)
            && affected.len() > MAX_AFFECTED
        {
            let total = affected.len();
            affected.truncate(MAX_AFFECTED);
            object.insert("affected_cut".to_string(), Value::from(total));
        }
        Ok(record)
    }

    /// One call: POST `body` to the segments under the fixed address, or GET when there is none.
    async fn ask(&self, segments: &[&str], body: Option<&Value>) -> Result<Value, String> {
        let mut url = self.api.clone();
        url.path_segments_mut()
            .map_err(|()| "OSV's address cannot hold a path".to_string())?
            .extend(segments);
        let request = match body {
            Some(body) => self
                .client
                .post(url)
                .header("content-type", "application/json")
                .body(body.to_string()),
            None => self.client.get(url),
        };
        let mut response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                format!("OSV did not answer within {} seconds", TIMEOUT.as_secs())
            } else {
                "OSV could not be reached".to_string()
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("OSV answered with status {}", status.as_u16()));
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                format!("OSV did not answer within {} seconds", TIMEOUT.as_secs())
            } else {
                "OSV's answer was cut off".to_string()
            }
        })? {
            if chunk.len() > MAX_BODY - bytes.len() {
                return Err(
                    "OSV's answer is too large; ask about one version of the package".to_string(),
                );
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "OSV's answer is not JSON".to_string())
    }
}

/// The versions that fix `vuln` for `package`: only the `affected` entries naming that ecosystem
/// and name, each version once, in OSV's order.
fn fixed_versions(vuln: &Value, package: &Package) -> Vec<String> {
    let mut fixed: Vec<String> = Vec::new();
    for entry in vuln["affected"].as_array().into_iter().flatten() {
        if entry["package"]["ecosystem"].as_str() != Some(package.ecosystem.as_str())
            || entry["package"]["name"].as_str() != Some(package.name.as_str())
        {
            continue;
        }
        for range in entry["ranges"].as_array().into_iter().flatten() {
            for event in range["events"].as_array().into_iter().flatten() {
                if let Some(version) = event["fixed"].as_str()
                    && !fixed.iter().any(|seen| seen == version)
                {
                    fixed.push(version.to_string());
                }
            }
        }
    }
    fixed
}

/// The name the server gives itself.
const SERVER_NAME: &str = "farik-osv";

/// Every tool, with what it takes.
fn descriptors() -> Vec<Tool> {
    let package = json!({
        "type": "object",
        "properties": {
            "ecosystem": { "type": "string", "description": "The package's ecosystem, such as npm, PyPI, crates.io or Go." },
            "name": { "type": "string", "description": "The package's name." },
            "version": { "type": "string", "description": "One exact version." }
        },
        "required": ["ecosystem", "name", "version"]
    });
    let schemas = [
        (
            "query_package",
            "List the known flaws of one version of a package, with the versions that fix each.",
            package.clone(),
        ),
        (
            "query_packages",
            "List the ids of the known flaws of up to 100 package versions at once.",
            json!({
                "type": "object",
                "properties": { "packages": { "type": "array", "items": package, "minItems": 1, "maxItems": MAX_PACKAGES } },
                "required": ["packages"]
            }),
        ),
        (
            "get_vulnerability",
            "Read one known flaw by its id, such as GHSA-xxxx-xxxx-xxxx or CVE-2021-23337.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        ),
    ];
    schemas
        .into_iter()
        .map(|(name, about, schema)| {
            let Value::Object(schema): Value = schema else {
                unreachable!("each schema above is an object")
            };
            Tool::new(name, about, Arc::new(schema as JsonObject))
                .with_annotations(ToolAnnotations::new().read_only(true))
        })
        .collect()
}

impl ServerHandler for Osv {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"));
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        // As Farik's own server answers: protocol 2026-07-28 wants a list's freshness said.
        std::future::ready(Ok(ListToolsResult::with_all_items(descriptors())
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let input = request.arguments.map_or_else(|| json!({}), Value::Object);
        let result = match self.call(&request.name, &input).await {
            Ok(answer) => CallToolResult::success(vec![ContentBlock::text(answer.to_string())]),
            Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
        };
        Ok(result.into())
    }
}

/// Serves on standard input and output until the client leaves.
///
/// # Errors
///
/// The client could not be made, or the server stopped with an error.
pub async fn serve_stdio(api: &str) -> Result<(), OsvError> {
    use rmcp::ServiceExt as _;

    let server = Osv::new(api)?;
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|error| OsvError::Serving(error.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|error| OsvError::Serving(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, Method, Response, StatusCode, Uri};
    use serde_json::{Value, json};

    use super::{Osv, descriptors, tool_names};

    /// What the fixture was asked.
    #[derive(Clone, Debug)]
    struct Seen {
        method: Method,
        uri: Uri,
        header_names: Vec<String>,
        body: Vec<u8>,
    }

    /// What the fixture answers.
    #[derive(Clone)]
    enum Reply {
        Json(Value),
        Bytes(Vec<u8>),
        Redirect,
        Hang,
    }

    type Answer = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

    /// A stand-in for `api.osv.dev` on this computer.
    struct Fixture {
        address: String,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl Fixture {
        async fn start(answer: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Self {
            let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
            let answer: Answer = Arc::new(answer);
            let record = seen.clone();
            let app = Router::new().fallback(
                move |method: Method, uri: Uri, headers: HeaderMap, body: Body| {
                    let record = record.clone();
                    let answer = answer.clone();
                    async move {
                        let bytes = to_bytes(body, usize::MAX).await.unwrap_or_default();
                        let one = Seen {
                            method,
                            uri,
                            header_names: headers
                                .keys()
                                .map(|name| name.as_str().to_string())
                                .collect(),
                            body: bytes.to_vec(),
                        };
                        record.lock().expect("the record").push(one.clone());
                        match answer(&one) {
                            Reply::Json(value) => Response::new(Body::from(value.to_string())),
                            Reply::Bytes(bytes) => Response::new(Body::from(bytes)),
                            Reply::Redirect => Response::builder()
                                .status(StatusCode::FOUND)
                                .header("location", "http://127.0.0.1:9/elsewhere")
                                .body(Body::empty())
                                .expect("a response"),
                            Reply::Hang => {
                                tokio::time::sleep(Duration::from_secs(3600)).await;
                                Response::new(Body::empty())
                            }
                        }
                    }
                },
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a port");
            let address = format!("http://{}/v1", listener.local_addr().expect("an address"));
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self { address, seen }
        }

        fn requests(&self) -> Vec<Seen> {
            self.seen.lock().expect("the record").clone()
        }
    }

    fn body_of(seen: &Seen) -> Value {
        serde_json::from_slice(&seen.body).expect("a JSON body")
    }

    fn advisory(id: &str) -> Value {
        json!({
            "id": id,
            "summary": format!("{id} summary"),
            "aliases": [format!("CVE-{id}")],
            "severity": [{ "type": "CVSS_V3", "score": "CVSS:3.1/AV:N" }],
            "details": "a long text that is never sent on",
            "affected": [
                { "package": { "name": "lodash", "ecosystem": "npm" },
                  "ranges": [{ "type": "SEMVER", "events": [{ "introduced": "0" }, { "fixed": "4.17.21" }] }] },
                { "package": { "name": "lodash-es", "ecosystem": "npm" },
                  "ranges": [{ "type": "SEMVER", "events": [{ "introduced": "0" }, { "fixed": "4.17.22" }] }] }
            ]
        })
    }

    #[test]
    fn lists_the_tools_it_names() {
        let listed: Vec<String> = descriptors()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(listed, tool_names());
    }

    fn asking(fixture: &Fixture) -> Osv {
        Osv::new(&fixture.address).expect("a server")
    }

    fn lodash() -> Value {
        json!({ "ecosystem": "npm", "name": "lodash", "version": "4.17.15" })
    }

    /// Over MCP, as a client sees it: the name it gives, the tools it lists, an answer, and a
    /// refusal that is a tool error.
    #[tokio::test]
    async fn answers_over_mcp_as_farik_osv() {
        use rmcp::ServiceExt as _;
        use rmcp::model::CallToolRequestParams;

        let fixture =
            Fixture::start(|_| Reply::Json(json!({ "vulns": [advisory("GHSA-1")] }))).await;
        let (server_io, client_io) = tokio::io::duplex(1 << 16);
        let server = asking(&fixture);
        tokio::spawn(async move {
            if let Ok(running) = server.serve(server_io).await {
                let _ = running.waiting().await;
            }
        });
        let client = ().serve(client_io).await.expect("a client");
        let info = client
            .peer_info()
            .expect("the server's answer to initialize");
        assert_eq!(info.server_info.as_ref().expect("a name").name, "farik-osv");
        let tools = client.list_all_tools().await.expect("a list");
        let names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
        assert_eq!(names, tool_names());
        let arguments = |value: Value| value.as_object().cloned().expect("an object");
        let answer = client
            .call_tool(
                CallToolRequestParams::new("query_package").with_arguments(arguments(lodash())),
            )
            .await
            .expect("a call");
        assert_ne!(answer.is_error, Some(true));
        let text = answer.content[0].as_text().expect("text");
        let said: Value = serde_json::from_str(&text.text).expect("JSON");
        assert_eq!(said["advisories"][0]["id"], "GHSA-1");
        let refused = client
            .call_tool(
                CallToolRequestParams::new("get_vulnerability")
                    .with_arguments(arguments(json!({ "id": "../x" }))),
            )
            .await
            .expect("a call");
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(fixture.requests().len(), 1, "the refused id was not sent");
        let _ = client.cancel().await;
    }

    #[tokio::test]
    async fn query_package_asks_for_one_version() {
        let fixture =
            Fixture::start(|_| Reply::Json(json!({ "vulns": [advisory("GHSA-1")] }))).await;
        let answer = asking(&fixture)
            .call("query_package", &lodash())
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, Method::POST);
        assert_eq!(seen[0].uri.path(), "/v1/query");
        assert_eq!(
            body_of(&seen[0]),
            json!({ "package": { "name": "lodash", "ecosystem": "npm" }, "version": "4.17.15" })
        );
        for name in &seen[0].header_names {
            assert!(
                ["host", "content-type", "content-length", "accept"].contains(&name.as_str()),
                "an unexpected header {name}"
            );
        }
        let advisories = answer["advisories"].as_array().expect("a list");
        assert_eq!(advisories.len(), 1);
        let first = &advisories[0];
        assert_eq!(first["id"], "GHSA-1");
        assert_eq!(first["summary"], "GHSA-1 summary");
        assert_eq!(first["aliases"], json!(["CVE-GHSA-1"]));
        assert_eq!(first["severity"][0]["type"], "CVSS_V3");
        assert_eq!(first["fixed"], json!(["4.17.21"]));
        assert!(first.get("details").is_none());
        assert_eq!(answer["more"], false);
    }

    #[tokio::test]
    async fn query_package_cuts_a_long_list() {
        let many: Vec<Value> = (0..60).map(|n| advisory(&format!("GHSA-{n}"))).collect();
        let fixture = Fixture::start(move |_| {
            Reply::Json(json!({ "vulns": many, "next_page_token": "next" }))
        })
        .await;
        let answer = asking(&fixture)
            .call("query_package", &lodash())
            .await
            .expect("an answer");
        assert_eq!(answer["advisories"].as_array().expect("a list").len(), 50);
        assert_eq!(answer["advisories"][0]["id"], "GHSA-0");
        assert_eq!(answer["more"], true);
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert!(body_of(&seen[0]).get("page_token").is_none());

        let only_token = Fixture::start(|_| Reply::Json(json!({ "next_page_token": "t" }))).await;
        let answer = asking(&only_token)
            .call("query_package", &lodash())
            .await
            .expect("an answer");
        assert_eq!(answer["advisories"], json!([]));
        assert_eq!(answer["more"], true);

        let huge = Fixture::start(|_| Reply::Bytes(vec![b' '; 5 << 20])).await;
        let error = asking(&huge)
            .call("query_package", &lodash())
            .await
            .expect_err("too large");
        assert!(error.contains("too large"), "{error}");
    }

    #[tokio::test]
    async fn query_packages_sends_one_batch() {
        let fixture = Fixture::start(|_| {
            Reply::Json(json!({ "results": [
                { "vulns": [{ "id": "GHSA-1", "modified": "x" }, { "id": "GHSA-2", "modified": "x" }] },
                {},
                { "vulns": [{ "id": "GHSA-3", "modified": "x" }], "next_page_token": "t" }
            ] }))
        })
        .await;
        let packages = json!({ "packages": [
            { "ecosystem": "npm", "name": "a", "version": "1.0.0" },
            { "ecosystem": "PyPI", "name": "b", "version": "2.0.0" },
            { "ecosystem": "npm", "name": "c", "version": "3.0.0" }
        ] });
        let answer = asking(&fixture)
            .call("query_packages", &packages)
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].uri.path(), "/v1/querybatch");
        assert_eq!(
            body_of(&seen[0]),
            json!({ "queries": [
                { "package": { "name": "a", "ecosystem": "npm" }, "version": "1.0.0" },
                { "package": { "name": "b", "ecosystem": "PyPI" }, "version": "2.0.0" },
                { "package": { "name": "c", "ecosystem": "npm" }, "version": "3.0.0" }
            ] })
        );
        let results = answer["results"].as_array().expect("a list");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["name"], "a");
        assert_eq!(results[0]["advisories"], json!(["GHSA-1", "GHSA-2"]));
        assert_eq!(results[0]["more"], false);
        assert_eq!(results[1]["advisories"], json!([]));
        assert_eq!(results[2]["advisories"], json!(["GHSA-3"]));
        assert_eq!(results[2]["more"], true);

        let fresh = Fixture::start(|_| Reply::Json(json!({ "results": [] }))).await;
        let osv = asking(&fresh);
        let too_many = json!({ "packages": vec![json!({ "ecosystem": "npm", "name": "a", "version": "1" }); 101] });
        assert!(osv.call("query_packages", &too_many).await.is_err());
        assert!(
            osv.call("query_packages", &json!({ "packages": [] }))
                .await
                .is_err()
        );
        assert!(fresh.requests().is_empty());
    }

    #[tokio::test]
    async fn get_vulnerability_cuts_what_is_long() {
        let affected: Vec<Value> = (0..30)
            .map(|n| json!({ "package": { "name": format!("p{n}"), "ecosystem": "npm" } }))
            .collect();
        let record = json!({
            "id": "GHSA-9",
            "details": "é".repeat(20_000),
            "affected": affected,
            "references": [{ "type": "WEB", "url": "https://example.com" }]
        });
        let fixture = Fixture::start(move |_| Reply::Json(record.clone())).await;
        let answer = asking(&fixture)
            .call("get_vulnerability", &json!({ "id": "GHSA-9" }))
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen[0].method, Method::GET);
        assert_eq!(seen[0].uri.path(), "/v1/vulns/GHSA-9");
        assert_eq!(
            answer["details"].as_str().expect("text").chars().count(),
            8_000
        );
        assert_eq!(answer["affected"].as_array().expect("a list").len(), 20);
        assert_eq!(answer["details_cut"], true);
        assert_eq!(answer["affected_cut"], 30);
        assert_eq!(answer["id"], "GHSA-9");
        assert_eq!(answer["references"][0]["url"], "https://example.com");
    }

    #[tokio::test]
    async fn refuses_a_malformed_input_without_asking() {
        let fixture = Fixture::start(|_| Reply::Json(json!({}))).await;
        let osv = asking(&fixture);
        let with = |ecosystem: &str, name: &str, version: &str| json!({ "ecosystem": ecosystem, "name": name, "version": version });
        for bad in [
            with("np/m", "lodash", "1"),
            with(&"e".repeat(41), "lodash", "1"),
            with("npm", "lo\ndash", "1"),
            with("npm", "", "1"),
            with("npm", &"n".repeat(215), "1"),
            with("npm", "lodash", "1\u{7}"),
            with("npm", "lodash", ""),
            with("npm", "lodash", &"v".repeat(129)),
            json!({ "ecosystem": "npm", "name": "lodash" }),
            json!({ "ecosystem": 4, "name": "lodash", "version": "1" }),
        ] {
            assert!(osv.call("query_package", &bad).await.is_err(), "{bad}");
        }
        for bad in [
            "../x",
            "..",
            ".",
            "",
            "-x",
            "a/b",
            "a b",
            "a?b",
            "a#b",
            "a%2fb",
            &"a".repeat(65),
        ] {
            assert!(
                osv.call("get_vulnerability", &json!({ "id": bad }))
                    .await
                    .is_err(),
                "{bad}"
            );
        }
        assert!(
            fixture.requests().is_empty(),
            "nothing an agent wrote left the computer"
        );
        osv.call("get_vulnerability", &json!({ "id": "ALSA-2022:1234" }))
            .await
            .expect("a colon is allowed in an id");
        assert_eq!(fixture.requests()[0].uri.path(), "/v1/vulns/ALSA-2022:1234");
        assert!(osv.call("nothing", &json!({})).await.is_err());
    }

    #[tokio::test]
    async fn follows_no_redirect_and_gives_up() {
        let fixture = Fixture::start(|_| Reply::Redirect).await;
        let error = asking(&fixture)
            .call("query_package", &lodash())
            .await
            .expect_err("a redirect is refused");
        assert!(error.contains("302"), "{error}");
        assert_eq!(fixture.requests().len(), 1);

        let silent = Fixture::start(|_| Reply::Hang).await;
        let started = Instant::now();
        let error = asking(&silent)
            .call("query_package", &lodash())
            .await
            .expect_err("no answer");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert!(error.contains("did not answer"), "{error}");
    }
}
