//! Farik's own MCP server over the central banks' exchange rates (`docs/SPEC.md` 6.7, ADR 0038),
//! started by `farik connector fx`. It speaks to one fixed address, follows no redirect and uses
//! no proxy, checks every input before it leaves, and sends Frankfurter only currency codes and a
//! day. What Frankfurter answers is data the agent reads under the untrusted-content notice.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

/// Frankfurter's address. Never an argument, an environment value or a tool input: the tests pass
/// a fixture's address to [`Fx::new`], not to the command.
pub const FX_API: &str = "https://api.frankfurter.dev/v2";

/// Why the server could not run.
#[derive(Debug)]
pub enum FxError {
    /// The web client could not be made.
    Client,
    /// The server could not start, or ended with an error.
    Serving(String),
}

impl fmt::Display for FxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => write!(formatter, "the web client could not be made"),
            Self::Serving(why) => write!(formatter, "the exchange-rate server stopped: {why}"),
        }
    }
}

impl std::error::Error for FxError {}

/// The tools the server lists, in the order it lists them.
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    vec!["latest_rates", "rate_on", "list_currencies"]
}

/// How long one call to Frankfurter may take.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The most of an answer Farik reads.
const MAX_BODY: usize = 1024 * 1024;
/// The most currencies `list_currencies` answers with.
const MAX_CURRENCIES: usize = 400;
/// The most quotes `latest_rates` takes.
const MAX_QUOTES: usize = 30;
/// What a call says whenever Frankfurter does not answer with rates: never its own words.
const REFUSED: &str = "Frankfurter could not answer that; check the codes and the date";
/// The first day Frankfurter has rates for.
const FIRST_DAY: (i32, u32, u32) = (1999, 1, 4);

/// The server, speaking to one address.
#[derive(Clone)]
pub struct Fx {
    api: reqwest::Url,
    client: reqwest::Client,
    timeout: Duration,
    today: Option<NaiveDate>,
}

impl Fx {
    /// A server that asks `api`, and reads the day from the clock.
    ///
    /// # Errors
    ///
    /// The address is not a web address, or the web client could not be made.
    pub fn new(api: &str) -> Result<Self, FxError> {
        Self::build(api, None, TIMEOUT)
    }

    /// A server that asks `api` and believes it is `today`, for a test.
    #[cfg(test)]
    fn with_today(api: &str, today: NaiveDate) -> Result<Self, FxError> {
        Self::build(api, Some(today), TIMEOUT)
    }

    /// [`Fx::with_today`] with a timeout of its own.
    #[cfg(test)]
    fn with_timeout(api: &str, today: NaiveDate, timeout: Duration) -> Result<Self, FxError> {
        Self::build(api, Some(today), timeout)
    }

    fn build(api: &str, today: Option<NaiveDate>, timeout: Duration) -> Result<Self, FxError> {
        let client = reqwest::Client::builder()
            // Nothing Frankfurter answers sends Farik anywhere else, and nothing is sent through a
            // proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| FxError::Client)?;
        let api = reqwest::Url::parse(api).map_err(|_| FxError::Client)?;
        Ok(Self {
            api,
            client,
            timeout,
            today,
        })
    }

    /// Runs `tool` with `input`: the answer as JSON, or why it was refused, in words.
    ///
    /// # Errors
    ///
    /// The input is not valid, Frankfurter could not be reached or answered badly.
    pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String> {
        match tool {
            "latest_rates" => self.latest_rates(input).await,
            "rate_on" => self.rate_on(input).await,
            "list_currencies" => self.list_currencies().await,
            _ => Err(format!("there is no tool named {tool}")),
        }
    }

    async fn latest_rates(&self, input: &Value) -> Result<Value, String> {
        let base = code(input, "base")?;
        let asked = input["quotes"]
            .as_array()
            .filter(|list| (1..=MAX_QUOTES).contains(&list.len()))
            .ok_or_else(|| format!("quotes is a list of 1 to {MAX_QUOTES} currency codes"))?;
        let mut quotes: Vec<String> = Vec::new();
        for item in asked {
            let quote = code_of(item, "a quote")?;
            if quote == base || quotes.contains(&quote) {
                return Err("quotes are different currencies, none the base".to_string());
            }
            quotes.push(quote);
        }
        let rows = self
            .ask("rates", &[("base", &base), ("quotes", &quotes.join(","))])
            .await?;
        let mut rates = serde_json::Map::new();
        for quote in &quotes {
            if let Some(row) = row_for(&rows, quote)? {
                rates.insert(
                    quote.clone(),
                    json!({ "rate": row["rate"], "date": row["date"] }),
                );
            }
        }
        let missing: Vec<&str> = quotes
            .iter()
            .map(String::as_str)
            .filter(|quote| !rates.contains_key(*quote))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Frankfurter gave no rate for {}",
                missing.join(", ")
            ));
        }
        Ok(json!({ "base": base, "rates": rates }))
    }

    async fn rate_on(&self, input: &Value) -> Result<Value, String> {
        let base = code(input, "base")?;
        let quote = code(input, "quote")?;
        if quote == base {
            return Err("quote is a different currency from base".to_string());
        }
        let date = self.day(input)?;
        let rows = self
            .ask(
                "rates",
                &[("base", &base), ("quotes", &quote), ("date", &date)],
            )
            .await?;
        let row = row_for(&rows, &quote)?
            .ok_or_else(|| "Frankfurter gave no rate for that day".to_string())?;
        Ok(json!({ "date": row["date"], "base": base, "quote": quote, "rate": row["rate"] }))
    }

    async fn list_currencies(&self) -> Result<Value, String> {
        let rows = self.ask("currencies", &[]).await?;
        let rows = rows.as_array().ok_or_else(unexpected)?;
        let listed: Vec<Value> = rows
            .iter()
            .filter_map(|row| {
                Some(json!({ "code": row["iso_code"].as_str()?, "name": row["name"].as_str()? }))
            })
            .take(MAX_CURRENCIES)
            .collect();
        Ok(Value::Array(listed))
    }

    /// The day `input` names: an ISO date from Frankfurter's first day to today in UTC.
    fn day(&self, input: &Value) -> Result<String, String> {
        let text = input["date"]
            .as_str()
            .ok_or_else(|| "date is needed, as YYYY-MM-DD".to_string())?;
        let shaped = text.len() == 10
            && text.char_indices().all(|(at, c)| {
                if at == 4 || at == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            });
        let day = shaped
            .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
            .flatten()
            .ok_or_else(|| "date is a real day, written YYYY-MM-DD".to_string())?;
        let (year, month, date) = FIRST_DAY;
        let first = NaiveDate::from_ymd_opt(year, month, date).expect("a real day");
        if day < first {
            return Err(format!(
                "date is {first} or later: there are no rates before"
            ));
        }
        let today = self
            .today
            .unwrap_or_else(|| chrono::Utc::now().date_naive());
        if day > today {
            return Err(format!(
                "date is {today} or earlier: there are no rates ahead"
            ));
        }
        Ok(text.to_string())
    }

    /// One call: GET `segment` under the fixed address with `query`, built as pairs and never
    /// formatted into the address.
    async fn ask(&self, segment: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let mut url = self.api.clone();
        url.path_segments_mut()
            .map_err(|()| "Frankfurter's address cannot hold a path".to_string())?
            .push(segment);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let seconds = self.timeout.as_secs();
        let mut response = self.client.get(url).send().await.map_err(|error| {
            if error.is_timeout() {
                format!("Frankfurter did not answer within {seconds} seconds")
            } else {
                "Frankfurter could not be reached".to_string()
            }
        })?;
        // Whatever it says other than rates, a redirect included, is one sentence of Farik's.
        if !response.status().is_success() {
            return Err(REFUSED.to_string());
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                format!("Frankfurter did not answer within {seconds} seconds")
            } else {
                "Frankfurter's answer was cut off".to_string()
            }
        })? {
            if chunk.len() > MAX_BODY - bytes.len() {
                return Err("Frankfurter's answer is too large".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Frankfurter's answer is not JSON".to_string())
    }
}

/// What a call says when Frankfurter's answer is not the list of rows it should be.
fn unexpected() -> String {
    "Frankfurter's answer was not what was expected".to_string()
}

/// The first row of `rows` for `quote`, whose rate is a number and whose date is text; `None` when
/// there is none.
fn row_for<'a>(rows: &'a Value, quote: &str) -> Result<Option<&'a Value>, String> {
    let rows = rows.as_array().ok_or_else(unexpected)?;
    let Some(row) = rows.iter().find(|row| row["quote"].as_str() == Some(quote)) else {
        return Ok(None);
    };
    if row["rate"].is_number() && row["date"].is_string() {
        Ok(Some(row))
    } else {
        Err(unexpected())
    }
}

/// The currency code `input[key]` holds: three capital letters.
fn code(input: &Value, key: &str) -> Result<String, String> {
    code_of(&input[key], key)
}

/// `value` as a currency code, named `what` in a refusal.
fn code_of(value: &Value, what: &str) -> Result<String, String> {
    value
        .as_str()
        .filter(|text| text.len() == 3 && text.bytes().all(|byte| byte.is_ascii_uppercase()))
        .map(str::to_string)
        .ok_or_else(|| format!("{what} is a currency code of three capital letters, such as USD"))
}

/// The name the server gives itself.
const SERVER_NAME: &str = "farik-fx";

/// Every tool, with what it takes.
fn descriptors() -> Vec<Tool> {
    let code = |about: &str| json!({ "type": "string", "description": about });
    let schemas = [
        (
            "latest_rates",
            "Read the latest reference exchange rates from one currency to up to 30 others, each with the day Frankfurter gave it for.",
            json!({
                "type": "object",
                "properties": {
                    "base": code("The currency to convert from, as three capital letters such as USD."),
                    "quotes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": 30,
                        "description": "The currencies to convert to, as three capital letters each."
                    }
                },
                "required": ["base", "quotes"]
            }),
        ),
        (
            "rate_on",
            "Read the reference exchange rate between two currencies on a day. The answer says which day it is for, which may be an earlier one.",
            json!({
                "type": "object",
                "properties": {
                    "base": code("The currency to convert from, as three capital letters such as USD."),
                    "quote": code("The currency to convert to, as three capital letters such as EUR."),
                    "date": code("The day, as YYYY-MM-DD, from 1999-01-04 to today.")
                },
                "required": ["base", "quote", "date"]
            }),
        ),
        (
            "list_currencies",
            "List the currencies Frankfurter has rates for, each with its code and name.",
            json!({ "type": "object", "properties": {} }),
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

impl ServerHandler for Fx {
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
pub async fn serve_stdio(api: &str) -> Result<(), FxError> {
    use rmcp::ServiceExt as _;

    let server = Fx::new(api)?;
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|error| FxError::Serving(error.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|error| FxError::Serving(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, Method, Response, StatusCode, Uri};
    use chrono::NaiveDate;
    use serde_json::{Value, json};

    use super::{FX_API, Fx, descriptors, tool_names};

    /// What the fixture was asked.
    #[derive(Clone, Debug)]
    struct Seen {
        method: Method,
        uri: Uri,
        header_names: Vec<String>,
    }

    /// What the fixture answers.
    #[derive(Clone)]
    enum Reply {
        Json(Value),
        Bytes(Vec<u8>),
        Redirect(String),
        Status(u16),
        Hang,
    }

    type Answer = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

    /// A stand-in for `api.frankfurter.dev` on this computer.
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
                        let _ = to_bytes(body, usize::MAX).await;
                        let one = Seen {
                            method,
                            uri,
                            header_names: headers
                                .keys()
                                .map(|name| name.as_str().to_string())
                                .collect(),
                        };
                        record.lock().expect("the record").push(one.clone());
                        match answer(&one) {
                            Reply::Json(value) => Response::new(Body::from(value.to_string())),
                            Reply::Bytes(bytes) => Response::new(Body::from(bytes)),
                            Reply::Redirect(to) => Response::builder()
                                .status(StatusCode::FOUND)
                                .header("location", to)
                                .body(Body::empty())
                                .expect("a response"),
                            Reply::Status(code) => Response::builder()
                                .status(code)
                                .body(Body::from("{\"message\":\"unknown quote XYZ\"}"))
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
            let address = format!("http://{}/v2", listener.local_addr().expect("an address"));
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self { address, seen }
        }

        fn requests(&self) -> Vec<Seen> {
            self.seen.lock().expect("the record").clone()
        }
    }

    /// The day the server believes it is, so no test depends on the clock.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).expect("a day")
    }

    fn asking(fixture: &Fixture) -> Fx {
        Fx::with_today(&fixture.address, today()).expect("a server")
    }

    fn row(date: &str, base: &str, quote: &str, rate: f64) -> Value {
        json!({ "date": date, "base": base, "quote": quote, "rate": rate })
    }

    /// `count` distinct codes of three capital letters, none of them `USD`.
    fn codes(count: u8) -> Vec<String> {
        (0..count)
            .map(|n| {
                format!(
                    "A{}{}",
                    char::from(b'A' + n / 26),
                    char::from(b'A' + n % 26)
                )
            })
            .collect()
    }

    const REFUSAL: &str = "Frankfurter could not answer that; check the codes and the date";

    #[test]
    fn fx_api_is_frankfurters_v2() {
        assert_eq!(FX_API, "https://api.frankfurter.dev/v2");
    }

    /// Over MCP, as a client sees it: the name it gives, exactly three tools each with an input
    /// schema, an answer, and a refusal that is a tool error.
    #[tokio::test]
    async fn lists_exactly_three_tools() {
        use rmcp::ServiceExt as _;
        use rmcp::model::CallToolRequestParams;

        assert_eq!(tool_names(), ["latest_rates", "rate_on", "list_currencies"]);
        let listed: Vec<String> = descriptors()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(listed, tool_names());

        let fixture =
            Fixture::start(|_| Reply::Json(json!([row("2026-10-07", "USD", "EUR", 0.86)]))).await;
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
        assert_eq!(info.server_info.as_ref().expect("a name").name, "farik-fx");
        let tools = client.list_all_tools().await.expect("a list");
        let names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
        assert_eq!(names, tool_names());
        for tool in &tools {
            assert_eq!(
                tool.input_schema.get("type"),
                Some(&json!("object")),
                "{}",
                tool.name
            );
        }
        let arguments = |value: Value| value.as_object().cloned().expect("an object");
        let answer = client
            .call_tool(
                CallToolRequestParams::new("latest_rates")
                    .with_arguments(arguments(json!({ "base": "USD", "quotes": ["EUR"] }))),
            )
            .await
            .expect("a call");
        assert_ne!(answer.is_error, Some(true));
        let text = answer.content[0].as_text().expect("text");
        let said: Value = serde_json::from_str(&text.text).expect("JSON");
        assert_eq!(said["rates"]["EUR"]["rate"], 0.86);
        let refused = client
            .call_tool(
                CallToolRequestParams::new("latest_rates")
                    .with_arguments(arguments(json!({ "base": "usd", "quotes": ["EUR"] }))),
            )
            .await
            .expect("a call");
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(fixture.requests().len(), 1, "the refused call was not sent");
        let _ = client.cancel().await;
    }

    #[tokio::test]
    async fn latest_rates_asks_once_and_shapes_the_answer() {
        // The central banks Frankfurter reads publish on different days: each rate keeps its own.
        let fixture = Fixture::start(|_| {
            Reply::Json(json!([
                row("2026-10-07", "USD", "EUR", 0.86),
                row("2026-10-06", "USD", "GBP", 0.75),
            ]))
        })
        .await;
        let answer = asking(&fixture)
            .call(
                "latest_rates",
                &json!({ "base": "USD", "quotes": ["EUR", "GBP"] }),
            )
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, Method::GET);
        assert_eq!(seen[0].uri.path(), "/v2/rates");
        assert_eq!(seen[0].uri.query(), Some("base=USD&quotes=EUR%2CGBP"));
        for name in &seen[0].header_names {
            assert!(
                ["host", "accept"].contains(&name.as_str()),
                "an unexpected header {name}"
            );
        }
        assert_eq!(
            answer,
            json!({
                "base": "USD",
                "rates": {
                    "EUR": { "rate": 0.86, "date": "2026-10-07" },
                    "GBP": { "rate": 0.75, "date": "2026-10-06" }
                }
            })
        );
    }

    #[tokio::test]
    async fn rate_on_says_the_day_answered() {
        let fixture =
            Fixture::start(|_| Reply::Json(json!([row("2026-10-02", "USD", "EUR", 0.9)]))).await;
        let answer = asking(&fixture)
            .call(
                "rate_on",
                &json!({ "base": "USD", "quote": "EUR", "date": "2026-10-03" }),
            )
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].uri.path(), "/v2/rates");
        assert_eq!(
            seen[0].uri.query(),
            Some("base=USD&quotes=EUR&date=2026-10-03")
        );
        assert_eq!(
            answer,
            json!({ "date": "2026-10-02", "base": "USD", "quote": "EUR", "rate": 0.9 })
        );
    }

    #[tokio::test]
    async fn list_currencies_keeps_code_and_name() {
        let rows: Vec<Value> = (0..450)
            .map(|n| {
                json!({
                    "iso_code": format!("C{n:02}"), "iso_numeric": "978", "name": format!("Currency {n}"),
                    "symbol": "x", "start_date": "1999-01-04", "end_date": null
                })
            })
            .collect();
        let fixture = Fixture::start(move |_| Reply::Json(Value::Array(rows.clone()))).await;
        let answer = asking(&fixture)
            .call("list_currencies", &json!({}))
            .await
            .expect("an answer");
        let seen = fixture.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].uri.path(), "/v2/currencies");
        assert_eq!(seen[0].uri.query(), None);
        let listed = answer.as_array().expect("a list");
        assert_eq!(listed.len(), 400, "at most 400");
        assert_eq!(listed[0], json!({ "code": "C00", "name": "Currency 0" }));
        for entry in listed {
            let mut keys: Vec<&str> = entry
                .as_object()
                .expect("an entry")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(keys, ["code", "name"]);
        }
    }

    #[tokio::test]
    async fn refuses_bad_input_before_sending() {
        let fixture = Fixture::start(|_| Reply::Json(json!([]))).await;
        let fx = asking(&fixture);
        let many = codes(31);
        assert_eq!(many.len(), 31);
        for (tool, input) in [
            ("latest_rates", json!({ "base": "usd", "quotes": ["EUR"] })),
            ("latest_rates", json!({ "base": "US", "quotes": ["EUR"] })),
            ("latest_rates", json!({ "base": "USDD", "quotes": ["EUR"] })),
            ("latest_rates", json!({ "base": "USD", "quotes": [] })),
            ("latest_rates", json!({ "base": "USD", "quotes": many })),
            (
                "latest_rates",
                json!({ "base": "USD", "quotes": ["EUR", "EUR"] }),
            ),
            ("latest_rates", json!({ "base": "USD", "quotes": ["USD"] })),
            ("latest_rates", json!({ "base": "USD", "quotes": ["eur"] })),
            ("latest_rates", json!({ "base": "USD", "quotes": "EUR" })),
            ("latest_rates", json!({ "quotes": ["EUR"] })),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "USD", "date": "2026-10-03" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EU", "date": "2026-10-03" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "1998-12-31" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "2026-10-09" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "2026-02-30" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "2026-2-3" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "20261003" }),
            ),
            (
                "rate_on",
                json!({ "base": "USD", "quote": "EUR", "date": "tomorrow" }),
            ),
            ("rate_on", json!({ "base": "USD", "quote": "EUR" })),
            ("nothing", json!({})),
        ] {
            assert!(fx.call(tool, &input).await.is_err(), "{tool} {input}");
        }
        assert!(fixture.requests().is_empty(), "something was sent");
    }

    #[tokio::test]
    async fn takes_the_first_day_and_today_and_thirty_quotes() {
        let thirty = codes(30);
        assert_eq!(thirty.len(), 30);
        let fixture =
            Fixture::start(|_| Reply::Json(json!([row("2026-10-08", "USD", "EUR", 0.9)]))).await;
        let fx = asking(&fixture);
        for date in ["1999-01-04", "2026-10-08"] {
            fx.call(
                "rate_on",
                &json!({ "base": "USD", "quote": "EUR", "date": date }),
            )
            .await
            .unwrap_or_else(|error| panic!("{date}: {error}"));
        }
        // Frankfurter answers one row for a quote it knows; the other 29 are not in this fixture's
        // answer, which the tool says rather than hides.
        let error = fx
            .call("latest_rates", &json!({ "base": "USD", "quotes": thirty }))
            .await
            .expect_err("29 quotes the fixture did not answer");
        assert!(error.contains("gave no rate for"), "{error}");
        assert_eq!(fixture.requests().len(), 3, "all three were sent");
    }

    #[tokio::test]
    async fn a_refusal_hides_frankfurters_words() {
        for status in [422, 404, 500] {
            let fixture = Fixture::start(move |_| Reply::Status(status)).await;
            let fx = asking(&fixture);
            for (tool, input) in [
                ("latest_rates", json!({ "base": "USD", "quotes": ["XYZ"] })),
                (
                    "rate_on",
                    json!({ "base": "USD", "quote": "XYZ", "date": "2026-10-03" }),
                ),
                ("list_currencies", json!({})),
            ] {
                let error = fx.call(tool, &input).await.expect_err("a refusal");
                assert_eq!(error, REFUSAL, "{status} {tool}");
            }
        }
    }

    #[tokio::test]
    async fn follows_no_redirect() {
        let elsewhere = Fixture::start(|_| Reply::Json(json!([]))).await;
        let to = format!("{}/rates", elsewhere.address);
        let fixture = Fixture::start(move |_| Reply::Redirect(to.clone())).await;
        let error = asking(&fixture)
            .call("latest_rates", &json!({ "base": "USD", "quotes": ["EUR"] }))
            .await
            .expect_err("a redirect is refused");
        assert_eq!(error, REFUSAL);
        assert_eq!(fixture.requests().len(), 1);
        assert!(elsewhere.requests().is_empty(), "the redirect was followed");
    }

    #[tokio::test]
    async fn gives_up_after_the_timeout() {
        let silent = Fixture::start(|_| Reply::Hang).await;
        let started = Instant::now();
        let error = Fx::with_timeout(&silent.address, today(), Duration::from_secs(1))
            .expect("a server")
            .call("list_currencies", &json!({}))
            .await
            .expect_err("no answer");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert!(error.contains("did not answer"), "{error}");
        assert_eq!(
            asking(&silent).timeout,
            Duration::from_secs(15),
            "the server waits fifteen seconds"
        );
    }

    /// The environment's proxy is never used: a child copy of this test, with the proxy variables
    /// set to a fixture that records, asks a second fixture directly.
    #[tokio::test]
    async fn uses_no_proxy_from_the_environment() {
        if let Ok(target) = std::env::var("FARIK_FX_PROXY_CHILD") {
            Fx::with_today(&target, today())
                .expect("a server")
                .call("list_currencies", &json!({}))
                .await
                .expect("answered");
            return;
        }
        let proxy = Fixture::start(|_| Reply::Json(json!([]))).await;
        let target = Fixture::start(|_| Reply::Json(json!([]))).await;
        let through = proxy.address.trim_end_matches("/v2").to_string();
        let child = tokio::process::Command::new(std::env::current_exe().expect("this test"))
            .args(["--exact", "fx::tests::uses_no_proxy_from_the_environment"])
            .env("FARIK_FX_PROXY_CHILD", &target.address)
            .env("HTTP_PROXY", &through)
            .env("http_proxy", &through)
            .env("HTTPS_PROXY", &through)
            .env("https_proxy", &through)
            .env("ALL_PROXY", &through)
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .output()
            .await
            .expect("the child runs");
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        assert!(proxy.requests().is_empty(), "the proxy was used");
        assert_eq!(target.requests().len(), 1);
    }

    #[tokio::test]
    async fn cuts_an_oversized_answer() {
        let padded = |total: usize| {
            let mut body = b"[]".to_vec();
            body.resize(total, b' ');
            body
        };
        let edge = padded(1024 * 1024);
        let at = Fixture::start(move |_| Reply::Bytes(edge.clone())).await;
        asking(&at)
            .call("list_currencies", &json!({}))
            .await
            .expect("exactly 1 MiB is read");
        let over = padded(1024 * 1024 + 1);
        let past = Fixture::start(move |_| Reply::Bytes(over.clone())).await;
        let error = asking(&past)
            .call("list_currencies", &json!({}))
            .await
            .expect_err("one byte more");
        assert_eq!(error, "Frankfurter's answer is too large");
    }
}
