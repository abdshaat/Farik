//! Farik's own Google Ads connector (`docs/SPEC.md` 6.7, ADR 0038, ADR 0042): the client for
//! Google's API, and what each of the connector's ten tools sends. It speaks to one fixed address,
//! follows no redirect and uses no proxy, sends the agent's grant as a bearer and nothing else (no
//! developer token, no `login-customer-id`), checks every input before anything leaves, and builds
//! its queries from fixed text and checked values alone. What Google answers is data the agent
//! reads under the untrusted-content notice.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use farik_core::marketing::{Amount, BudgetKind, parse_amount};
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::claude::Secret;

/// Google Ads' address, version 25 (released 2026-07-22; a Farik release moves it). Never an
/// argument or an input: the tests pass a fixture's address to [`GoogleAds::new`].
pub const GOOGLE_ADS_API: &str = "https://googleads.googleapis.com/v25";

/// The connector's three reads, in the order its kit lists them.
pub const READ_TOOLS: [&str; 3] = ["list_accounts", "report", "keyword_ideas"];

/// The connector's seven writes, in the order its kit lists them.
pub const WRITE_TOOLS: [&str; 7] = [
    "create_search_campaign",
    "add_ad_group",
    "add_keywords",
    "add_negative_keywords",
    "add_responsive_search_ad",
    "set_campaign_budget",
    "set_campaign_status",
];

/// The ten tools the shim lists: the reads, then the writes.
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    READ_TOOLS.iter().chain(&WRITE_TOOLS).copied().collect()
}

/// Why a call to Google, or a tool's input, was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoogleAdsError {
    /// An input is not valid; nothing was sent.
    Input(String),
    /// Google refused the request, in Farik's words with Google's own cut and quoted.
    Google(String),
    /// Google would not allow it: its `PERMISSION_DENIED`.
    NotAllowed(String),
    /// The call could not be made, or Google answered with a fault, in a sentence.
    Failed(String),
}

impl fmt::Display for GoogleAdsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::Input(words)
        | Self::Google(words)
        | Self::NotAllowed(words)
        | Self::Failed(words)) = self;
        formatter.write_str(words)
    }
}

impl std::error::Error for GoogleAdsError {}

/// How long one call to Google may take.
const TIMEOUT: Duration = Duration::from_secs(25);
/// The most of an answer Farik reads.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// The most characters of Google's own words Farik passes on.
const MAX_WORDS: usize = 300;
/// The most accounts `list_accounts` reads.
const MAX_ACCOUNTS: usize = 20;
/// The most rows a `report` answers with.
const MAX_ROWS: usize = 500;
/// The most ideas `keyword_ideas` answers with.
const MAX_IDEAS: usize = 100;
/// What `list_accounts` asks of each account.
const CUSTOMER_QUERY: &str = "SELECT customer.descriptive_name, customer.currency_code, \
     customer.time_zone, customer.manager FROM customer";

/// The client, speaking to one address.
#[derive(Clone)]
pub struct GoogleAds {
    api: String,
    client: reqwest::Client,
    timeout: Duration,
}

/// Why Farik could not speak to Google at all.
fn failed(words: &str) -> GoogleAdsError {
    GoogleAdsError::Failed(words.to_string())
}

/// `text` cut at the most characters of Google's own words Farik passes on.
fn cut(text: &str) -> String {
    text.chars().take(MAX_WORDS).collect()
}

impl GoogleAds {
    /// A client for `api`: `https`, or `http` on this computer, for the tests.
    ///
    /// # Errors
    ///
    /// The address is not one of those, or the web client could not be made.
    pub fn new(api: &str) -> Result<Self, GoogleAdsError> {
        Self::with_timeout(api, TIMEOUT)
    }

    fn with_timeout(api: &str, timeout: Duration) -> Result<Self, GoogleAdsError> {
        let url = reqwest::Url::parse(api).map_err(|_| failed("Google Ads' address is not one"))?;
        let on_this_computer = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(name)) => name == "localhost",
            None => false,
        };
        let allowed = match url.scheme() {
            "https" => url.host().is_some(),
            "http" => on_this_computer,
            _ => false,
        };
        if !allowed {
            return Err(failed(
                "Google Ads' address is https, or http on this computer",
            ));
        }
        let client = reqwest::Client::builder()
            // Nothing Google answers sends Farik anywhere else, and nothing goes through a proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| failed("the web client could not be made"))?;
        Ok(Self {
            api: url.as_str().trim_end_matches('/').to_string(),
            client,
            timeout,
        })
    }

    /// One call: GET, or POST of `body`, to `path` under the fixed address, answered as JSON.
    async fn call(
        &self,
        token: &Secret,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, GoogleAdsError> {
        let url = format!("{}/{path}", self.api);
        let request = match body {
            Some(body) => self
                .client
                .post(url)
                .header("content-type", "application/json")
                .body(body.to_string()),
            None => self.client.get(url),
        };
        let silent = format!(
            "Google did not answer within {} seconds",
            self.timeout.as_secs()
        );
        let mut response = request
            .bearer_auth(token.expose())
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    GoogleAdsError::Failed(silent.clone())
                } else {
                    failed("Google could not be reached")
                }
            })?;
        let status = response.status();
        if status.is_redirection() {
            return Err(failed(
                "Google answered with a redirect, which Farik does not follow",
            ));
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                GoogleAdsError::Failed(silent.clone())
            } else {
                failed("Google's answer was cut off")
            }
        })? {
            if chunk.len() > MAX_BODY - bytes.len() {
                return Err(failed("Google's answer is too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let answer: Option<Value> = serde_json::from_slice(&bytes).ok();
        if !status.is_success() {
            return Err(refusal(status.as_u16(), answer.as_ref()));
        }
        answer.ok_or_else(|| failed("Google's answer is not JSON"))
    }

    /// The customer ids the sign-in reaches directly.
    ///
    /// # Errors
    ///
    /// As [`GoogleAds::search`].
    pub async fn accessible(&self, token: &Secret) -> Result<Vec<String>, GoogleAdsError> {
        let answer = self
            .call(token, "customers:listAccessibleCustomers", None)
            .await?;
        Ok(answer["resourceNames"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|name| name.as_str()?.strip_prefix("customers/"))
            .filter(|id| is_customer(id))
            .map(str::to_string)
            .collect())
    }

    /// The rows of one `googleAds:search` of `query` in `account`.
    ///
    /// # Errors
    ///
    /// `Input` for an account that is not one, `NotAllowed` for Google's `PERMISSION_DENIED`,
    /// `Google` for any other refusal, `Failed` when Google could not be reached or answered
    /// badly.
    pub async fn search(
        &self,
        token: &Secret,
        account: &str,
        query: &str,
    ) -> Result<Vec<Value>, GoogleAdsError> {
        let customer = customer(account)?;
        let answer = self
            .call(
                token,
                &format!("customers/{customer}/googleAds:search"),
                Some(&json!({ "query": query })),
            )
            .await?;
        Ok(answer["results"].as_array().cloned().unwrap_or_default())
    }

    /// One `googleAds:mutate` of `operations` in `account`: all of them or none, and the resource
    /// name each made or changed, in order.
    ///
    /// # Errors
    ///
    /// As [`GoogleAds::search`].
    pub async fn mutate(
        &self,
        token: &Secret,
        account: &str,
        operations: Vec<Value>,
    ) -> Result<Vec<String>, GoogleAdsError> {
        let customer = customer(account)?;
        if operations.is_empty() {
            return Err(GoogleAdsError::Input(
                "there is nothing to change".to_string(),
            ));
        }
        let asked = operations.len();
        let answer = self
            .call(
                token,
                &format!("customers/{customer}/googleAds:mutate"),
                Some(&json!({ "mutateOperations": operations })),
            )
            .await?;
        let made: Vec<String> = answer["mutateOperationResponses"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|response| {
                response
                    .as_object()?
                    .values()
                    .find_map(|result| result["resourceName"].as_str())
                    .map(str::to_string)
            })
            .collect();
        if made.len() == asked {
            Ok(made)
        } else {
            Err(failed("Google's answer does not say what it changed"))
        }
    }

    /// The ideas of one `generateKeywordIdeas` `request` in `account`.
    ///
    /// # Errors
    ///
    /// As [`GoogleAds::search`], but Google's `PERMISSION_DENIED` is the words of an app Google
    /// has not yet allowed to give keyword ideas.
    pub async fn keyword_ideas(
        &self,
        token: &Secret,
        account: &str,
        request: &Value,
    ) -> Result<Vec<Value>, GoogleAdsError> {
        let customer = customer(account)?;
        let answer = self
            .call(
                token,
                &format!("customers/{customer}:generateKeywordIdeas"),
                Some(request),
            )
            .await
            .map_err(|error| match error {
                GoogleAdsError::NotAllowed(_) => GoogleAdsError::NotAllowed(
                    "Google has not yet allowed Farik's app to give keyword ideas.".to_string(),
                ),
                other => other,
            })?;
        Ok(answer["results"].as_array().cloned().unwrap_or_default())
    }
}

/// What Google's refusal, `status` and its error body, comes to in Farik's words, Google's own
/// message cut and quoted.
fn refusal(status: u16, body: Option<&Value>) -> GoogleAdsError {
    let google = body.map_or("", |body| {
        body["error"]["status"].as_str().unwrap_or_default()
    });
    let message = cut(body.map_or("", |body| {
        body["error"]["message"].as_str().unwrap_or_default()
    }));
    if status == 401 || google == "UNAUTHENTICATED" {
        failed("Google did not accept Farik's sign-in; sign in again")
    } else if status == 403 || google == "PERMISSION_DENIED" {
        GoogleAdsError::NotAllowed(format!("Google would not allow this: “{message}”"))
    } else if status == 429 || google == "RESOURCE_EXHAUSTED" {
        GoogleAdsError::Google(format!(
            "Google says Farik has asked too often: “{message}”"
        ))
    } else if status >= 500 {
        GoogleAdsError::Failed(format!(
            "Google had a fault (status {status}); try again later"
        ))
    } else {
        GoogleAdsError::Google(format!("Google refused it: “{message}”"))
    }
}

/// Whether `text` is a customer id: ten digits.
fn is_customer(text: &str) -> bool {
    text.len() == 10 && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The ten digits of an ad account, given as `NNN-NNN-NNNN`.
///
/// # Errors
///
/// `Input` for anything else.
pub fn customer_of(account: &str) -> Result<String, GoogleAdsError> {
    let bytes = account.as_bytes();
    let shaped = bytes.len() == 12
        && bytes.iter().enumerate().all(|(at, byte)| {
            if at == 3 || at == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        });
    if shaped {
        Ok(account.replace('-', ""))
    } else {
        Err(GoogleAdsError::Input(
            "the account is the ad account's number written 123-456-7890".to_string(),
        ))
    }
}

/// The customer of `account`, written as `NNN-NNN-NNNN` or as ten digits.
fn customer(account: &str) -> Result<String, GoogleAdsError> {
    if is_customer(account) {
        Ok(account.to_string())
    } else {
        customer_of(account)
    }
}

/// A customer's ten digits written `NNN-NNN-NNNN`.
#[must_use]
pub fn dashed(customer: &str) -> String {
    format!("{}-{}-{}", &customer[..3], &customer[3..6], &customer[6..])
}

/// `text`'s field of `input`, 1 to `most` characters with no control character.
fn text(input: &Value, field: &str, most: usize) -> Result<String, GoogleAdsError> {
    let words = input[field]
        .as_str()
        .ok_or_else(|| GoogleAdsError::Input(format!("{field} is needed, as text")))?;
    checked_text(field, words, most)
}

fn checked_text(field: &str, words: &str, most: usize) -> Result<String, GoogleAdsError> {
    let length = words.chars().count();
    if length == 0 || length > most || words.trim().is_empty() {
        return Err(GoogleAdsError::Input(format!(
            "{field} is 1 to {most} characters"
        )));
    }
    if words.chars().any(char::is_control) {
        return Err(GoogleAdsError::Input(format!(
            "{field} holds no control character"
        )));
    }
    Ok(words.to_string())
}

/// The list `field` holds, `least` to `most` long.
fn list<'a>(
    input: &'a Value,
    field: &str,
    least: usize,
    most: usize,
) -> Result<&'a Vec<Value>, GoogleAdsError> {
    input[field]
        .as_array()
        .filter(|items| (least..=most).contains(&items.len()))
        .ok_or_else(|| GoogleAdsError::Input(format!("{field} is a list of {least} to {most}")))
}

/// Each member of the list `field` as a text of 1 to `each` characters.
fn texts(
    input: &Value,
    field: &str,
    (least, most): (usize, usize),
    each: usize,
) -> Result<Vec<String>, GoogleAdsError> {
    list(input, field, least, most)?
        .iter()
        .map(|item| {
            item.as_str()
                .ok_or_else(|| GoogleAdsError::Input(format!("{field} holds text only")))
                .and_then(|words| checked_text(field, words, each))
        })
        .collect()
}

/// Numeric ids of Google's constants: `least` to `most` of them, each a whole number from 1.
fn ids(input: &Value, field: &str, least: usize, most: usize) -> Result<Vec<u64>, GoogleAdsError> {
    list(input, field, least, most)?
        .iter()
        .map(|item| {
            item.as_u64()
                .filter(|id| (1..1_000_000_000_000).contains(id))
                .ok_or_else(|| {
                    GoogleAdsError::Input(format!("{field} holds numeric constant ids, from 1"))
                })
        })
        .collect()
}

/// An amount in the plan's currency, as a decimal text above nothing; none when `field` is absent.
fn money(input: &Value, field: &str) -> Result<Option<Amount>, GoogleAdsError> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(words)) => parse_amount(words)
            .filter(|amount| amount.0 > 0)
            .map(Some)
            .ok_or_else(|| {
                GoogleAdsError::Input(format!(
                    "{field} is an amount above nothing, as text such as 1.50"
                ))
            }),
        Some(_) => Err(GoogleAdsError::Input(format!(
            "{field} is an amount above nothing, as text such as 1.50"
        ))),
    }
}

/// A day written `YYYY-MM-DD`.
fn day(input: &Value, field: &str) -> Result<NaiveDate, GoogleAdsError> {
    let said = format!("{field} is a day written 2026-10-31");
    let words = input[field]
        .as_str()
        .filter(|words| {
            words.len() == 10
                && words.bytes().enumerate().all(|(at, byte)| {
                    if at == 4 || at == 7 {
                        byte == b'-'
                    } else {
                        byte.is_ascii_digit()
                    }
                })
        })
        .ok_or_else(|| GoogleAdsError::Input(said.clone()))?;
    words.parse().map_err(|_| GoogleAdsError::Input(said))
}

/// A plan campaign's key: lower-case words joined by hyphens, at most 40 characters.
fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// The resource name `field` holds: `customers/<ten digits>/<kind>/<id>`, `kind` `campaigns` or
/// `adGroups`.
fn resource(input: &Value, field: &str, kind: &str) -> Result<String, GoogleAdsError> {
    let said = || {
        GoogleAdsError::Input(format!(
            "{field} is a resource name such as customers/1234567890/{kind}/123"
        ))
    };
    let name = input[field].as_str().ok_or_else(said)?;
    let mut parts = name.split('/');
    let shaped = parts.next() == Some("customers")
        && parts.next().is_some_and(is_customer)
        && parts.next() == Some(kind)
        && parts.next().is_some_and(|id| {
            (1..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
        })
        && parts.next().is_none();
    if shaped {
        Ok(name.to_string())
    } else {
        Err(said())
    }
}

/// Google's micros of `amount`, hundredths times 10,000, as the text of a 64-bit number.
fn micros(amount: Amount) -> String {
    (amount.0 * 10_000).to_string()
}

/// Micros, read from the text or number Google writes a 64-bit number as.
fn micros_of(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|words| words.parse().ok()))
}

/// Micros as a decimal of the currency: at least two decimals, never more than six.
fn decimal(micros: u64) -> String {
    let mut fraction = format!("{:06}", micros % 1_000_000);
    while fraction.len() > 2 && fraction.ends_with('0') {
        fraction.pop();
    }
    format!("{}.{fraction}", micros / 1_000_000)
}

/// Google's `camelCase` name as the wire's `snake_case`.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for letter in name.chars() {
        if letter.is_ascii_uppercase() {
            out.push('_');
            out.push(letter.to_ascii_lowercase());
        } else {
            out.push(letter);
        }
    }
    out
}

/// `value` with every key of every object `snake_case`.
fn snake_keys(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, field)| (snake(key), snake_keys(field)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(snake_keys).collect()),
        other => other.clone(),
    }
}

/// One row of a report as the tool answers it: Google's names as `snake_case`, and the metrics
/// as numbers, the cost as a decimal of the account's currency.
fn report_row(row: &Value) -> Value {
    let Some(fields) = row.as_object() else {
        return snake_keys(row);
    };
    Value::Object(
        fields
            .iter()
            .map(|(key, field)| {
                let shaped = if key == "metrics" {
                    json!({
                        "clicks": micros_of(&field["clicks"]),
                        "impressions": micros_of(&field["impressions"]),
                        "cost": micros_of(&field["costMicros"]).map(decimal),
                        "conversions": field["conversions"],
                    })
                } else {
                    snake_keys(field)
                };
                (snake(key), shaped)
            })
            .collect(),
    )
}

/// A `report`'s query and the account it is for, from the tool's input.
///
/// # Errors
///
/// `Input` for anything but an account, one of the five kinds and two days, the second not before
/// the first and at most 366 days after it.
pub fn report_query(input: &Value) -> Result<(String, String), GoogleAdsError> {
    let account = input["account"].as_str().unwrap_or_default();
    customer_of(account)?;
    let (from, to) = (day(input, "from")?, day(input, "to")?);
    if to < from || (to - from).num_days() > 366 {
        return Err(GoogleAdsError::Input(
            "to is not before from, and at most 366 days after it".to_string(),
        ));
    }
    let metrics = "metrics.clicks, metrics.impressions, metrics.cost_micros, metrics.conversions";
    let when = format!("segments.date BETWEEN '{from}' AND '{to}'");
    let order = "ORDER BY metrics.cost_micros DESC LIMIT 501";
    let query = match input["kind"].as_str() {
        Some("campaigns") => format!(
            "SELECT campaign.resource_name, campaign.name, campaign.status, {metrics} \
             FROM campaign WHERE {when} AND campaign.status != 'REMOVED' {order}"
        ),
        Some("ad_groups") => format!(
            "SELECT ad_group.resource_name, ad_group.name, ad_group.status, campaign.name, \
             {metrics} FROM ad_group WHERE {when} AND ad_group.status != 'REMOVED' {order}"
        ),
        Some("keywords") => format!(
            "SELECT ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, \
             ad_group_criterion.keyword.match_type, ad_group_criterion.status, ad_group.name, \
             campaign.name, {metrics} FROM keyword_view WHERE {when} \
             AND ad_group_criterion.status != 'REMOVED' {order}"
        ),
        Some("search_terms") => format!(
            "SELECT search_term_view.search_term, search_term_view.status, ad_group.name, \
             campaign.name, {metrics} FROM search_term_view WHERE {when} {order}"
        ),
        Some("ads") => format!(
            "SELECT ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, \
             ad_group_ad.status, ad_group.name, campaign.name, {metrics} \
             FROM ad_group_ad WHERE {when} AND ad_group_ad.status != 'REMOVED' {order}"
        ),
        _ => {
            return Err(GoogleAdsError::Input(
                "kind is campaigns, ad_groups, keywords, search_terms or ads".to_string(),
            ));
        }
    };
    Ok((account.to_string(), query))
}

/// The `report` tool: its answer, the rows cut at 500 with `more` when there were more.
///
/// # Errors
///
/// As [`report_query`] and [`GoogleAds::search`].
pub async fn report(
    ads: &GoogleAds,
    token: &Secret,
    input: &Value,
) -> Result<Value, GoogleAdsError> {
    let (account, query) = report_query(input)?;
    let rows = ads.search(token, &account, &query).await?;
    Ok(json!({
        "kind": input["kind"], "from": input["from"], "to": input["to"],
        "rows": rows.iter().take(MAX_ROWS).map(report_row).collect::<Vec<_>>(),
        "more": rows.len() > MAX_ROWS,
    }))
}

/// The `list_accounts` tool: each account the sign-in reaches, at most 20. One that cannot be
/// read says why in its place.
///
/// # Errors
///
/// As [`GoogleAds::accessible`].
pub async fn list_accounts(ads: &GoogleAds, token: &Secret) -> Result<Value, GoogleAdsError> {
    let reached = ads.accessible(token).await?;
    let mut accounts = Vec::new();
    for id in reached.iter().take(MAX_ACCOUNTS) {
        let account = dashed(id);
        accounts.push(match ads.search(token, id, CUSTOMER_QUERY).await {
            Ok(rows) => {
                let found = &rows
                    .first()
                    .map_or(Value::Null, |row| row["customer"].clone());
                json!({
                    "account": account, "name": found["descriptiveName"],
                    "currency": found["currencyCode"], "time_zone": found["timeZone"],
                    "manager": found["manager"],
                })
            }
            Err(error) => json!({ "account": account, "error": error.to_string() }),
        });
    }
    Ok(json!({ "accounts": accounts, "more": reached.len() > MAX_ACCOUNTS }))
}

/// One idea as the tool answers it.
fn idea(result: &Value) -> Value {
    let metrics = &result["keywordIdeaMetrics"];
    let competition = metrics["competition"]
        .as_str()
        .filter(|level| ["LOW", "MEDIUM", "HIGH"].contains(level))
        .map(str::to_lowercase);
    json!({
        "text": result["text"],
        "avg_monthly_searches": micros_of(&metrics["avgMonthlySearches"]),
        "competition": competition,
        "low_bid": micros_of(&metrics["lowTopOfPageBidMicros"]).map(decimal),
        "high_bid": micros_of(&metrics["highTopOfPageBidMicros"]).map(decimal),
    })
}

/// The `keyword_ideas` tool: at most 100 ideas.
///
/// # Errors
///
/// `Input` for an input that is not valid, else as [`GoogleAds::keyword_ideas`].
pub async fn keyword_ideas(
    ads: &GoogleAds,
    token: &Secret,
    input: &Value,
) -> Result<Value, GoogleAdsError> {
    let account = input["account"].as_str().unwrap_or_default();
    customer_of(account)?;
    let words = texts(input, "words", (1, 10), 80)?;
    let language = input["language"]
        .as_u64()
        .filter(|id| (1..1_000_000_000_000).contains(id))
        .ok_or_else(|| {
            GoogleAdsError::Input("language is a numeric constant id, from 1".to_string())
        })?;
    let locations: Vec<String> = ids(input, "locations", 1, 10)?
        .iter()
        .map(|id| format!("geoTargetConstants/{id}"))
        .collect();
    let request = json!({
        "language": format!("languageConstants/{language}"),
        "geoTargetConstants": locations,
        "keywordPlanNetwork": "GOOGLE_SEARCH",
        "keywordSeed": { "keywords": words },
        "pageSize": MAX_IDEAS,
    });
    let ideas = ads.keyword_ideas(token, account, &request).await?;
    Ok(json!({ "ideas": ideas.iter().take(MAX_IDEAS).map(idea).collect::<Vec<_>>() }))
}

/// How a campaign bids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bidding {
    /// As many clicks as the budget buys, within `max_cpc` when one is given.
    MaximizeClicks,
    /// As many conversions as the budget buys.
    MaximizeConversions,
}

/// The input of `create_search_campaign`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignInput {
    /// The ad account, `NNN-NNN-NNNN`.
    pub account: String,
    /// The plan campaign's key.
    pub plan_campaign: String,
    /// The campaign's name after the plan id and key.
    pub name: String,
    /// How it bids.
    pub bidding: Bidding,
    /// The most a click may cost, in the plan's currency.
    pub max_cpc: Option<Amount>,
    /// Numeric ids of Google's location constants.
    pub locations: Vec<u64>,
    /// Numeric ids of Google's language constants.
    pub languages: Vec<u64>,
}

impl CampaignInput {
    /// The checked input of `create_search_campaign`.
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value) -> Result<Self, GoogleAdsError> {
        let account = input["account"].as_str().unwrap_or_default();
        customer_of(account)?;
        let plan_campaign = text(input, "plan_campaign", 40)?;
        if !is_key(&plan_campaign) {
            return Err(GoogleAdsError::Input(
                "plan_campaign is the plan campaign's key, lower-case words joined by hyphens"
                    .to_string(),
            ));
        }
        let max_cpc = money(input, "max_cpc")?;
        let bidding = match input["bidding"].as_str() {
            Some("maximize_clicks") => Bidding::MaximizeClicks,
            Some("maximize_conversions") if max_cpc.is_none() => Bidding::MaximizeConversions,
            Some("maximize_conversions") => {
                return Err(GoogleAdsError::Input(
                    "max_cpc goes with maximize_clicks only".to_string(),
                ));
            }
            _ => {
                return Err(GoogleAdsError::Input(
                    "bidding is maximize_clicks or maximize_conversions; a campaign with Manual \
                     CPC has no campaign-level bid for max_cpc to be"
                        .to_string(),
                ));
            }
        };
        Ok(Self {
            account: account.to_string(),
            plan_campaign,
            name: text(input, "name", 80)?,
            bidding,
            max_cpc,
            locations: ids(input, "locations", 1, 20)?,
            languages: ids(input, "languages", 1, 10)?,
        })
    }
}

/// A campaign to make, with what the plan and today decide.
pub struct NewCampaign<'a> {
    /// What the agent asked for.
    pub input: &'a CampaignInput,
    /// The plan's id, `MP-<n>`.
    pub plan: &'a str,
    /// The first day, a UTC date no earlier than two days ahead.
    pub start: NaiveDate,
    /// The last day.
    pub ends_on: NaiveDate,
    /// The budget Google keeps: its kind and amount, in hundredths.
    pub budget: (BudgetKind, Amount),
}

/// The campaign's name at Google: the plan, the plan campaign's key and the agent's name.
#[must_use]
pub fn campaign_name(plan: &str, input: &CampaignInput) -> String {
    format!("{plan} {}: {}", input.plan_campaign, input.name)
}

/// The one `googleAds:mutate` that makes a campaign: its budget, the campaign paused, and a
/// criterion for each location and language. Temporary ids tie them together.
#[must_use]
pub fn campaign_operations(new: &NewCampaign<'_>) -> Vec<Value> {
    let customer = customer_of(&new.input.account).unwrap_or_default();
    let budget = format!("customers/{customer}/campaignBudgets/-1");
    let campaign = format!("customers/{customer}/campaigns/-2");
    let (kind, amount) = new.budget;
    let mut made = json!({
        "resourceName": budget, "explicitlyShared": false, "deliveryMethod": "STANDARD"
    });
    match kind {
        BudgetKind::Total => {
            made["period"] = json!("CUSTOM_PERIOD");
            made["totalAmountMicros"] = json!(micros(amount));
        }
        BudgetKind::Daily => made["amountMicros"] = json!(micros(amount)),
    }
    let mut running = json!({
        "resourceName": campaign,
        "name": campaign_name(new.plan, new.input),
        "advertisingChannelType": "SEARCH",
        "status": "PAUSED",
        "campaignBudget": budget,
        "networkSettings": {
            "targetGoogleSearch": true, "targetSearchNetwork": false, "targetContentNetwork": false
        },
        "startDateTime": format!("{} 00:00:00", new.start),
        "endDateTime": format!("{} 23:59:59", new.ends_on),
        "containsEuPoliticalAdvertising": "DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING",
    });
    match new.input.bidding {
        Bidding::MaximizeClicks => {
            running["targetSpend"] = new.input.max_cpc.map_or_else(
                || json!({}),
                |ceiling| json!({ "cpcBidCeilingMicros": micros(ceiling) }),
            );
        }
        Bidding::MaximizeConversions => running["maximizeConversions"] = json!({}),
    }
    let mut operations = vec![
        json!({ "campaignBudgetOperation": { "create": made } }),
        json!({ "campaignOperation": { "create": running } }),
    ];
    for id in &new.input.locations {
        operations.push(json!({ "campaignCriterionOperation": { "create": {
            "campaign": campaign,
            "location": { "geoTargetConstant": format!("geoTargetConstants/{id}") }
        } } }));
    }
    for id in &new.input.languages {
        operations.push(json!({ "campaignCriterionOperation": { "create": {
            "campaign": campaign,
            "language": { "languageConstant": format!("languageConstants/{id}") }
        } } }));
    }
    operations
}

/// One search word and how it matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword {
    /// The word or phrase.
    pub text: String,
    /// `EXACT`, `PHRASE` or `BROAD`.
    pub match_type: &'static str,
}

/// The input of `add_ad_group`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdGroupInput {
    /// The campaign's resource name.
    pub campaign: String,
    /// The ad group's name.
    pub name: String,
    /// What a click may cost, in the plan's currency.
    pub cpc_bid: Option<Amount>,
}

impl AdGroupInput {
    /// The checked input of `add_ad_group`.
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value) -> Result<Self, GoogleAdsError> {
        Ok(Self {
            campaign: resource(input, "campaign", "campaigns")?,
            name: text(input, "name", 80)?,
            cpc_bid: money(input, "cpc_bid")?,
        })
    }
}

/// The input of `add_keywords` or `add_negative_keywords`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordsInput {
    /// The ad group's or the campaign's resource name.
    pub parent: String,
    /// One to fifty words.
    pub keywords: Vec<Keyword>,
}

impl KeywordsInput {
    /// The checked input of `add_keywords` (`parent_field` `ad_group`) or
    /// `add_negative_keywords` (`campaign`).
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value, parent_field: &str) -> Result<Self, GoogleAdsError> {
        let kind = if parent_field == "ad_group" {
            "adGroups"
        } else {
            "campaigns"
        };
        let parent = resource(input, parent_field, kind)?;
        let keywords = list(input, "keywords", 1, 50)?
            .iter()
            .map(|word| {
                let match_type = match word["match"].as_str() {
                    Some("exact") => "EXACT",
                    Some("phrase") => "PHRASE",
                    Some("broad") => "BROAD",
                    _ => {
                        return Err(GoogleAdsError::Input(
                            "match is exact, phrase or broad".to_string(),
                        ));
                    }
                };
                Ok(Keyword {
                    text: text(word, "text", 80)?,
                    match_type,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { parent, keywords })
    }
}

/// The input of `add_responsive_search_ad`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdInput {
    /// The ad group's resource name.
    pub ad_group: String,
    /// Three to fifteen headlines.
    pub headlines: Vec<String>,
    /// Two to four descriptions.
    pub descriptions: Vec<String>,
    /// Where the ad leads, `https`.
    pub final_url: String,
    /// The first path part shown.
    pub path1: Option<String>,
    /// The second path part shown.
    pub path2: Option<String>,
}

/// A `final_url`: `https`, a host, no user information and no space.
fn final_url(input: &Value) -> Result<String, GoogleAdsError> {
    let said = || {
        GoogleAdsError::Input(
            "final_url is an https address with a host and no user name or password".to_string(),
        )
    };
    let address = input["final_url"].as_str().ok_or_else(said)?;
    let rest = address.strip_prefix("https://").ok_or_else(said)?;
    let parsed = reqwest::Url::parse(address).map_err(|_| said())?;
    let plain = !address
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\');
    if address.len() > 2000
        || !plain
        || rest.starts_with('/')
        || parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(said());
    }
    Ok(address.to_string())
}

/// An optional path part of an ad's address, at most 15 characters.
fn path(input: &Value, field: &str) -> Result<Option<String>, GoogleAdsError> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => text(input, field, 15).map(Some),
    }
}

impl AdInput {
    /// The checked input of `add_responsive_search_ad`.
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value) -> Result<Self, GoogleAdsError> {
        Ok(Self {
            ad_group: resource(input, "ad_group", "adGroups")?,
            headlines: texts(input, "headlines", (3, 15), 30)?,
            descriptions: texts(input, "descriptions", (2, 4), 90)?,
            final_url: final_url(input)?,
            path1: path(input, "path1")?,
            path2: path(input, "path2")?,
        })
    }
}

/// What `set_campaign_status` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Paused.
    Paused,
    /// Running.
    Enabled,
}

/// The operations of `add_ad_group`: the ad group, running.
#[must_use]
pub fn ad_group_operations(input: &AdGroupInput) -> Vec<Value> {
    let mut group = json!({
        "campaign": input.campaign, "name": input.name,
        "status": "ENABLED", "type": "SEARCH_STANDARD",
    });
    if let Some(bid) = input.cpc_bid {
        group["cpcBidMicros"] = json!(micros(bid));
    }
    vec![json!({ "adGroupOperation": { "create": group } })]
}

/// The operations of `add_keywords`: one criterion in the ad group for each word, running.
#[must_use]
pub fn keyword_operations(input: &KeywordsInput) -> Vec<Value> {
    input
        .keywords
        .iter()
        .map(|word| {
            json!({ "adGroupCriterionOperation": { "create": {
                "adGroup": input.parent, "status": "ENABLED",
                "keyword": { "text": word.text, "matchType": word.match_type }
            } } })
        })
        .collect()
}

/// The operations of `add_negative_keywords`: one negative criterion in the campaign for each
/// word.
#[must_use]
pub fn negative_keyword_operations(input: &KeywordsInput) -> Vec<Value> {
    input
        .keywords
        .iter()
        .map(|word| {
            json!({ "campaignCriterionOperation": { "create": {
                "campaign": input.parent, "negative": true,
                "keyword": { "text": word.text, "matchType": word.match_type }
            } } })
        })
        .collect()
}

/// The operations of `add_responsive_search_ad`: the ad, running.
#[must_use]
pub fn ad_operations(input: &AdInput) -> Vec<Value> {
    let assets = |words: &[String]| -> Vec<Value> {
        words.iter().map(|text| json!({ "text": text })).collect()
    };
    let mut search = json!({
        "headlines": assets(&input.headlines), "descriptions": assets(&input.descriptions)
    });
    for (field, part) in [("path1", &input.path1), ("path2", &input.path2)] {
        if let Some(part) = part {
            search[field] = json!(part);
        }
    }
    vec![json!({ "adGroupAdOperation": { "create": {
        "adGroup": input.ad_group, "status": "ENABLED",
        "ad": { "finalUrls": [input.final_url], "responsiveSearchAd": search }
    } } })]
}

/// The operation that sets a campaign budget's amount, by the budget's kind.
#[must_use]
pub fn budget_operations(budget: &str, kind: BudgetKind, amount: Amount) -> Vec<Value> {
    let field = match kind {
        BudgetKind::Total => "totalAmountMicros",
        BudgetKind::Daily => "amountMicros",
    };
    vec![json!({ "campaignBudgetOperation": {
        "update": { "resourceName": budget, field: micros(amount) },
        "updateMask": field
    } })]
}

/// The operation that sets a campaign's status.
#[must_use]
pub fn status_operations(campaign: &str, status: Status) -> Vec<Value> {
    let word = match status {
        Status::Paused => "PAUSED",
        Status::Enabled => "ENABLED",
    };
    vec![json!({ "campaignOperation": {
        "update": { "resourceName": campaign, "status": word },
        "updateMask": "status"
    } })]
}

/// What `list_accounts` asks of an account's currency, which the route checks a create against.
pub const CUSTOMER_CURRENCY_QUERY: &str = "SELECT customer.currency_code FROM customer";

/// The customer, ten digits, of a campaign's or an ad group's resource name, `kind` being
/// `campaigns` or `adGroups`; `None` for anything that is not exactly one.
#[must_use]
pub fn resource_customer(name: &str, kind: &str) -> Option<String> {
    let mut parts = name.split('/');
    let customer = parts
        .next()
        .filter(|first| *first == "customers")
        .and(parts.next())?;
    let shaped = is_customer(customer)
        && parts.next() == Some(kind)
        && parts.next().is_some_and(|id| {
            (1..=20).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
        })
        && parts.next().is_none();
    shaped.then(|| customer.to_string())
}

/// The query that reads which campaign an ad group belongs to.
///
/// # Errors
///
/// `Input` for a name that is not an ad group's resource name.
pub fn ad_group_campaign_query(ad_group: &str) -> Result<String, GoogleAdsError> {
    resource_customer(ad_group, "adGroups").ok_or_else(|| {
        GoogleAdsError::Input(
            "ad_group is a resource name such as customers/1234567890/adGroups/123".to_string(),
        )
    })?;
    Ok(format!(
        "SELECT ad_group.campaign FROM ad_group WHERE ad_group.resource_name = '{ad_group}'"
    ))
}

/// The query that reads what `campaigns` cost between two days, and what budget and end Google
/// holds for each, which enabling one is checked against.
///
/// # Errors
///
/// `Input` for no campaign, or a name that is not a campaign's resource name.
pub fn spend_query(
    campaigns: &[String],
    from: NaiveDate,
    to: NaiveDate,
) -> Result<String, GoogleAdsError> {
    Ok(format!(
        "SELECT campaign.resource_name, campaign.start_date_time, campaign.end_date_time, \
         campaign_budget.amount_micros, campaign_budget.total_amount_micros, metrics.cost_micros \
         FROM campaign WHERE campaign.resource_name IN ({}) AND segments.date BETWEEN '{from}' \
         AND '{to}'",
        quoted_campaigns(campaigns, "a spend read")?
    ))
}

/// The query that reads the status of `campaigns`, with no metrics, so that a campaign with no
/// cost still has its row (step 08g): Farik's pause reads it first, and sends nothing for a
/// campaign that is paused or removed already.
///
/// # Errors
///
/// `Input` for no campaign, or a name that is not a campaign's resource name.
pub fn status_query(campaigns: &[String]) -> Result<String, GoogleAdsError> {
    Ok(format!(
        "SELECT campaign.resource_name, campaign.status FROM campaign WHERE \
         campaign.resource_name IN ({})",
        quoted_campaigns(campaigns, "a status read")?
    ))
}

/// `campaigns` as the quoted, comma-separated list a query's `IN` takes.
///
/// # Errors
///
/// `Input` for no campaign, or a name that is not a campaign's resource name: `what` is the read
/// that needs them.
fn quoted_campaigns(campaigns: &[String], what: &str) -> Result<String, GoogleAdsError> {
    if campaigns.is_empty()
        || campaigns
            .iter()
            .any(|name| resource_customer(name, "campaigns").is_none())
    {
        return Err(GoogleAdsError::Input(format!(
            "{what} names one or more campaigns, each a resource name such as \
             customers/1234567890/campaigns/123"
        )));
    }
    let names: Vec<String> = campaigns.iter().map(|name| format!("'{name}'")).collect();
    Ok(names.join(", "))
}

/// What a spend read's row says Google holds for a campaign: its budget's amounts in micros, a
/// total budget's and a daily one's, and the days it starts and ends. `None` is what the row did
/// not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeldRow {
    /// `campaignBudget.totalAmountMicros`.
    pub total_micros: Option<u64>,
    /// `campaignBudget.amountMicros`.
    pub daily_micros: Option<u64>,
    /// The date part of `campaign.startDateTime`.
    pub starts_on: Option<NaiveDate>,
    /// The date part of `campaign.endDateTime`.
    pub ends_on: Option<NaiveDate>,
}

/// What each campaign a spend query's rows name holds at Google.
#[must_use]
pub fn held_by_campaign(rows: &[Value]) -> std::collections::BTreeMap<String, HeldRow> {
    let mut held = std::collections::BTreeMap::new();
    for row in rows {
        let Some(name) = row["campaign"]["resourceName"].as_str() else {
            continue;
        };
        // The day is the first ten characters of "yyyy-MM-dd HH:mm:ss".
        let day_of = |at: &Value| {
            at.as_str()
                .and_then(|at| at.get(..10))
                .and_then(|day| day.parse().ok())
        };
        held.entry(name.to_string()).or_insert(HeldRow {
            total_micros: micros_of(&row["campaignBudget"]["totalAmountMicros"]),
            daily_micros: micros_of(&row["campaignBudget"]["amountMicros"]),
            starts_on: day_of(&row["campaign"]["startDateTime"]),
            ends_on: day_of(&row["campaign"]["endDateTime"]),
        });
    }
    held
}

/// The cost in micros of each campaign a spend query's rows name.
#[must_use]
pub fn spend_by_campaign(rows: &[Value]) -> std::collections::BTreeMap<String, u64> {
    let mut cost = std::collections::BTreeMap::new();
    for row in rows {
        if let Some(name) = row["campaign"]["resourceName"].as_str() {
            *cost.entry(name.to_string()).or_insert(0) +=
                micros_of(&row["metrics"]["costMicros"]).unwrap_or(0);
        }
    }
    cost
}

/// The input of `set_campaign_budget`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetInput {
    /// The campaign's resource name.
    pub campaign: String,
    /// The new amount, in the plan's currency.
    pub amount: Amount,
}

impl BudgetInput {
    /// The checked input of `set_campaign_budget`.
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value) -> Result<Self, GoogleAdsError> {
        Ok(Self {
            campaign: resource(input, "campaign", "campaigns")?,
            amount: money(input, "amount")?.ok_or_else(|| {
                GoogleAdsError::Input(
                    "amount is an amount above nothing, as text such as 250.00".to_string(),
                )
            })?,
        })
    }
}

/// The input of `set_campaign_status`, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusInput {
    /// The campaign's resource name.
    pub campaign: String,
    /// What to set.
    pub status: Status,
}

impl StatusInput {
    /// The checked input of `set_campaign_status`.
    ///
    /// # Errors
    ///
    /// `Input`, saying what is wrong.
    pub fn parse(input: &Value) -> Result<Self, GoogleAdsError> {
        let status = match input["status"].as_str() {
            Some("paused") => Status::Paused,
            Some("enabled") => Status::Enabled,
            _ => {
                return Err(GoogleAdsError::Input(
                    "status is paused or enabled".to_string(),
                ));
            }
        };
        Ok(Self {
            campaign: resource(input, "campaign", "campaigns")?,
            status,
        })
    }
}

/// What the shim answers with when it was started in no session: a call needs the daemon's address
/// and the ticket the launch route gave it.
const NO_SESSION: &str = "Google Ads runs only inside a Farik session";

/// How long the shim waits for the daemon: the route makes up to four calls to Google of 25
/// seconds each.
const SHIM_TIMEOUT: Duration = Duration::from_secs(120);

/// The shim `farik connector google-ads` runs (ADR 0038, ADR 0042): it lists the ten tools itself,
/// so its list is pinned offline, and forwards each call to the daemon's `POST /connector/call`
/// with the session's ticket. The daemon holds the grant and makes the call, so no token is ever
/// in this process's environment, and a plan the owner ends takes effect at once.
#[derive(Clone)]
pub struct Shim {
    url: Option<String>,
    ticket: Option<String>,
    client: reqwest::Client,
    timeout: Duration,
}

/// Whether `url` is exactly `http://127.0.0.1:<port>/connector/call`: the daemon on this computer
/// and the one route, nothing else a variable could name.
fn is_the_daemon(url: &str) -> bool {
    url.strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/connector/call"))
        .and_then(|port| {
            port.parse::<u16>()
                .ok()
                .filter(|number| *number > 0 && number.to_string() == port)
        })
        .is_some()
}

impl Shim {
    /// A shim that forwards to `url` with `ticket`; with either missing, every call says that
    /// Google Ads runs only inside a Farik session, and the list still answers.
    ///
    /// # Errors
    ///
    /// The web client could not be made.
    pub fn new(url: Option<&str>, ticket: Option<&str>) -> Result<Self, GoogleAdsError> {
        Self::with_timeout(url, ticket, SHIM_TIMEOUT)
    }

    fn with_timeout(
        url: Option<&str>,
        ticket: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, GoogleAdsError> {
        let client = reqwest::Client::builder()
            // The daemon answers where it is; nothing it says sends Farik anywhere else, and
            // nothing goes through a proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| failed("the web client could not be made"))?;
        Ok(Self {
            url: url.map(str::to_string),
            ticket: ticket.map(str::to_string),
            client,
            timeout,
        })
    }

    /// Runs `tool` with `input` through the daemon: its answer as text, or the words it was
    /// refused with.
    ///
    /// # Errors
    ///
    /// The words of the refusal: the daemon's own (`<code>: <words>`), or Farik's when the call
    /// could not be made.
    pub async fn call(&self, tool: &str, input: &Value) -> Result<String, String> {
        let (Some(url), Some(ticket)) = (&self.url, &self.ticket) else {
            return Err(NO_SESSION.to_string());
        };
        // Checked at each call: a variable is whatever the environment held.
        if !is_the_daemon(url) {
            return Err("Farik's daemon is not where this connector was told it is".to_string());
        }
        let mut response = self
            .client
            .post(url)
            .bearer_auth(ticket)
            .json(&json!({ "tool": tool, "arguments": input }))
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    format!(
                        "Farik did not answer within {} seconds",
                        self.timeout.as_secs()
                    )
                } else {
                    "Farik's daemon could not be reached".to_string()
                }
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(
                "Farik did not accept this session's ticket; the session may have ended"
                    .to_string(),
            );
        }
        if !status.is_success() {
            return Err(format!("Farik answered with status {}", status.as_u16()));
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Farik's answer was cut off".to_string())?
        {
            if chunk.len() > MAX_BODY - bytes.len() {
                return Err("Farik's answer is too large".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        let answer: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Farik's answer is not JSON".to_string())?;
        match (answer.get("ok"), answer["error"].as_str()) {
            (Some(ok), _) => Ok(ok.to_string()),
            (None, Some(error)) => Err(error.to_string()),
            (None, None) => {
                Err("Farik's answer says neither what was done nor why not".to_string())
            }
        }
    }
}

/// The name the shim gives itself.
const SERVER_NAME: &str = "farik-google-ads";

/// An ad account, as every tool writes it.
fn account_schema() -> Value {
    json!({
        "type": "string", "pattern": "^[0-9]{3}-[0-9]{3}-[0-9]{4}$",
        "description": "The ad account's number, written 123-456-7890."
    })
}

/// A campaign's or an ad group's resource name, as a tool returns it.
fn resource_schema(kind: &str, what: &str) -> Value {
    json!({
        "type": "string",
        "pattern": format!("^customers/[0-9]{{10}}/{kind}/[0-9]{{1,20}}$"),
        "description": format!("{what}, as a tool of this connector gave it: customers/1234567890/{kind}/123.")
    })
}

/// The search words of `add_keywords` and `add_negative_keywords`.
fn keywords_schema() -> Value {
    json!({
        "type": "array", "minItems": 1, "maxItems": 50,
        "items": {
            "type": "object",
            "properties": {
                "text": { "type": "string", "minLength": 1, "maxLength": 80 },
                "match": { "type": "string", "enum": ["exact", "phrase", "broad"] }
            },
            "required": ["text", "match"]
        }
    })
}

/// A list of at most `most` texts of at most `each` characters.
fn texts_schema(least: usize, most: usize, each: usize) -> Value {
    json!({
        "type": "array", "minItems": least, "maxItems": most,
        "items": { "type": "string", "minLength": 1, "maxLength": each }
    })
}

/// Every tool, with what it takes and, for the three reads, that it changes nothing.
#[allow(
    clippy::too_many_lines,
    reason = "one entry for each of the ten tools, side by side, so a missing one is plain to see"
)]
fn descriptors() -> Vec<Tool> {
    let amount = |what: &str| {
        json!({
            "type": "string", "pattern": "^(0|[1-9][0-9]{0,7})(\\.[0-9]{1,2})?$",
            "description": format!("{what}, in the plan's currency, as text such as 1.50.")
        })
    };
    let schemas = [
        (
            "list_accounts",
            "List the Google Ads accounts the owner's sign-in reaches, each with its currency and time zone.",
            json!({ "type": "object", "properties": {} }),
        ),
        (
            "report",
            "Read one report of an ad account's results: campaigns, ad_groups, keywords, search_terms or ads, with clicks, impressions, cost (in the account's currency) and conversions over two days at most 366 days apart. At most 500 rows.",
            json!({
                "type": "object",
                "properties": {
                    "account": account_schema(),
                    "kind": { "type": "string", "enum": ["campaigns", "ad_groups", "keywords", "search_terms", "ads"] },
                    "from": { "type": "string", "description": "The first day, written 2026-10-01." },
                    "to": { "type": "string", "description": "The last day, written 2026-10-31." }
                },
                "required": ["account", "kind", "from", "to"]
            }),
        ),
        (
            "keyword_ideas",
            "Find search words related to the words given, each with how often it is searched a month, how competitive it is and what the bid for a click on top of the page costs.",
            json!({
                "type": "object",
                "properties": {
                    "account": account_schema(),
                    "words": texts_schema(1, 10, 80),
                    "language": { "type": "integer", "minimum": 1, "description": "Google's numeric id of a language, such as 1000 for English." },
                    "locations": { "type": "array", "minItems": 1, "maxItems": 10, "items": { "type": "integer", "minimum": 1 }, "description": "Google's numeric ids of the places to find words for, such as 2840 for the United States." }
                },
                "required": ["account", "words", "language", "locations"]
            }),
        ),
        (
            "create_search_campaign",
            "Make a Google Search campaign for one campaign of the active marketing plan. It is made paused, ends on the plan campaign's last day, and has a budget within the plan campaign's. Only inside the plan the owner approved.",
            json!({
                "type": "object",
                "properties": {
                    "account": account_schema(),
                    "plan_campaign": { "type": "string", "description": "The key of the plan's campaign this is for." },
                    "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                    "bidding": { "type": "string", "enum": ["maximize_clicks", "maximize_conversions"], "description": "maximize_conversions only for an ad account that already tracks conversions." },
                    "max_cpc": amount("The most a click may cost, with maximize_clicks only"),
                    "locations": { "type": "array", "minItems": 1, "maxItems": 20, "items": { "type": "integer", "minimum": 1 } },
                    "languages": { "type": "array", "minItems": 1, "maxItems": 10, "items": { "type": "integer", "minimum": 1 } }
                },
                "required": ["account", "plan_campaign", "name", "bidding", "locations", "languages"]
            }),
        ),
        (
            "add_ad_group",
            "Add an ad group to a campaign this connector made for the active plan.",
            json!({
                "type": "object",
                "properties": {
                    "campaign": resource_schema("campaigns", "The campaign"),
                    "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                    "cpc_bid": amount("What a click may cost in this ad group")
                },
                "required": ["campaign", "name"]
            }),
        ),
        (
            "add_keywords",
            "Add search words to an ad group, each matching exactly, as a phrase or broadly.",
            json!({
                "type": "object",
                "properties": {
                    "ad_group": resource_schema("adGroups", "The ad group"),
                    "keywords": keywords_schema()
                },
                "required": ["ad_group", "keywords"]
            }),
        ),
        (
            "add_negative_keywords",
            "Rule out search words for a whole campaign: its ads do not show for them.",
            json!({
                "type": "object",
                "properties": {
                    "campaign": resource_schema("campaigns", "The campaign"),
                    "keywords": keywords_schema()
                },
                "required": ["campaign", "keywords"]
            }),
        ),
        (
            "add_responsive_search_ad",
            "Write an ad for an ad group: 3 to 15 headlines of at most 30 characters, 2 to 4 descriptions of at most 90, and an https address it leads to.",
            json!({
                "type": "object",
                "properties": {
                    "ad_group": resource_schema("adGroups", "The ad group"),
                    "headlines": texts_schema(3, 15, 30),
                    "descriptions": texts_schema(2, 4, 90),
                    "final_url": { "type": "string", "description": "An https address with no user name or password." },
                    "path1": { "type": "string", "maxLength": 15 },
                    "path2": { "type": "string", "maxLength": 15 }
                },
                "required": ["ad_group", "headlines", "descriptions", "final_url"]
            }),
        ),
        (
            "set_campaign_budget",
            "Change a campaign's budget to a new amount within the plan's: a total budget between what the campaign has spent and the plan campaign's budget, a daily one no larger than what is left divided by the days left.",
            json!({
                "type": "object",
                "properties": {
                    "campaign": resource_schema("campaigns", "The campaign"),
                    "amount": amount("The new amount")
                },
                "required": ["campaign", "amount"]
            }),
        ),
        (
            "set_campaign_status",
            "Pause a campaign, or run it: only within the plan campaign's dates, with its budget not yet spent.",
            json!({
                "type": "object",
                "properties": {
                    "campaign": resource_schema("campaigns", "The campaign"),
                    "status": { "type": "string", "enum": ["paused", "enabled"] }
                },
                "required": ["campaign", "status"]
            }),
        ),
    ];
    schemas
        .into_iter()
        .map(|(name, about, schema)| {
            let Value::Object(schema): Value = schema else {
                unreachable!("each schema above is an object")
            };
            let tool = Tool::new(name, about, Arc::new(schema as JsonObject));
            if READ_TOOLS.contains(&name) {
                tool.with_annotations(ToolAnnotations::new().read_only(true))
            } else {
                tool
            }
        })
        .collect()
}

impl ServerHandler for Shim {
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
            Ok(answer) => CallToolResult::success(vec![ContentBlock::text(answer)]),
            Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
        };
        Ok(result.into())
    }
}

/// Serves on standard input and output until the client leaves. With neither `url` nor `ticket`
/// the list still answers, so connecting and the offline pin run it bare; a call then says that
/// Google Ads runs only inside a Farik session.
///
/// # Errors
///
/// The client could not be made, or the server stopped with an error.
pub async fn serve_shim(url: Option<&str>, ticket: Option<&str>) -> Result<(), GoogleAdsError> {
    use rmcp::ServiceExt as _;

    let server = Shim::new(url, ticket)?;
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|error| {
            GoogleAdsError::Failed(format!("the Google Ads server stopped: {error}"))
        })?;
    running.waiting().await.map_err(|error| {
        GoogleAdsError::Failed(format!("the Google Ads server stopped: {error}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::NaiveDate;
    use farik_core::marketing::{Amount, BudgetKind};
    use serde_json::{Value, json};

    use super::{
        AdGroupInput, AdInput, BudgetInput, CUSTOMER_CURRENCY_QUERY, CampaignInput, GoogleAds,
        GoogleAdsError, HeldRow, KeywordsInput, NewCampaign, Status, StatusInput,
        ad_group_campaign_query, ad_group_operations, ad_operations, budget_operations,
        campaign_operations, held_by_campaign, keyword_ideas, keyword_operations, list_accounts,
        negative_keyword_operations, report, resource_customer, spend_by_campaign, spend_query,
        status_operations, status_query, tool_names,
    };
    use crate::claude::Secret;
    use crate::google_ads_fixture::{Fixture, Mode};

    fn token() -> Secret {
        Secret::new("an-access-token".to_string())
    }

    fn day(text: &str) -> NaiveDate {
        text.parse().expect("a date")
    }

    const ACCOUNT: &str = "123-456-7890";

    async fn ads() -> (Fixture, GoogleAds) {
        let fixture = Fixture::start().await;
        let ads = GoogleAds::new(&fixture.address).expect("a client");
        (fixture, ads)
    }

    #[test]
    fn names_ten_tools() {
        assert_eq!(
            tool_names(),
            [
                "list_accounts",
                "report",
                "keyword_ideas",
                "create_search_campaign",
                "add_ad_group",
                "add_keywords",
                "add_negative_keywords",
                "add_responsive_search_ad",
                "set_campaign_budget",
                "set_campaign_status"
            ]
        );
    }

    #[tokio::test]
    async fn sends_the_bearer_and_no_developer_token() {
        let (fixture, ads) = ads().await;
        let token = token();
        ads.accessible(&token).await.expect("customers");
        ads.search(&token, ACCOUNT, "SELECT campaign.name FROM campaign")
            .await
            .expect("rows");
        ads.mutate(
            &token,
            ACCOUNT,
            status_operations("customers/1234567890/campaigns/5", Status::Paused),
        )
        .await
        .expect("a mutate");
        ads.keyword_ideas(
            &token,
            ACCOUNT,
            &json!({ "keywordSeed": { "keywords": ["boots"] } }),
        )
        .await
        .expect("ideas");
        let sent = fixture.requests();
        assert_eq!(
            sent.iter()
                .map(|seen| format!("{} {}", seen.method, seen.path))
                .collect::<Vec<_>>(),
            [
                "GET /v25/customers:listAccessibleCustomers",
                "POST /v25/customers/1234567890/googleAds:search",
                "POST /v25/customers/1234567890/googleAds:mutate",
                "POST /v25/customers/1234567890:generateKeywordIdeas",
            ]
        );
        for seen in &sent {
            assert_eq!(
                seen.headers.get("authorization").map(String::as_str),
                Some("Bearer an-access-token"),
                "{}",
                seen.path
            );
            for header in ["developer-token", "login-customer-id"] {
                assert!(!seen.headers.contains_key(header), "{header}");
            }
        }
    }

    /// The fixed text of each report, as it is sent.
    fn report_text(kind: &str, from: &str, to: &str) -> String {
        let metrics = "metrics.clicks, metrics.impressions, metrics.cost_micros, \
                       metrics.conversions";
        let when = format!("segments.date BETWEEN '{from}' AND '{to}'");
        match kind {
            "campaigns" => format!(
                "SELECT campaign.resource_name, campaign.name, campaign.status, {metrics} \
                 FROM campaign WHERE {when} AND campaign.status != 'REMOVED' \
                 ORDER BY metrics.cost_micros DESC LIMIT 501"
            ),
            "ad_groups" => format!(
                "SELECT ad_group.resource_name, ad_group.name, ad_group.status, campaign.name, \
                 {metrics} FROM ad_group WHERE {when} AND ad_group.status != 'REMOVED' \
                 ORDER BY metrics.cost_micros DESC LIMIT 501"
            ),
            "keywords" => format!(
                "SELECT ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, \
                 ad_group_criterion.keyword.match_type, ad_group_criterion.status, \
                 ad_group.name, campaign.name, {metrics} FROM keyword_view WHERE {when} \
                 AND ad_group_criterion.status != 'REMOVED' \
                 ORDER BY metrics.cost_micros DESC LIMIT 501"
            ),
            "search_terms" => format!(
                "SELECT search_term_view.search_term, search_term_view.status, ad_group.name, \
                 campaign.name, {metrics} FROM search_term_view WHERE {when} \
                 ORDER BY metrics.cost_micros DESC LIMIT 501"
            ),
            "ads" => format!(
                "SELECT ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, \
                 ad_group_ad.status, ad_group.name, campaign.name, {metrics} \
                 FROM ad_group_ad WHERE {when} AND ad_group_ad.status != 'REMOVED' \
                 ORDER BY metrics.cost_micros DESC LIMIT 501"
            ),
            other => panic!("no such report {other}"),
        }
    }

    #[tokio::test]
    async fn builds_each_report_from_fixed_text() {
        let (fixture, ads) = ads().await;
        for kind in ["campaigns", "ad_groups", "keywords", "search_terms", "ads"] {
            report(
                &ads,
                &token(),
                &json!({ "account": ACCOUNT, "kind": kind, "from": "2026-10-01", "to": "2026-10-31" }),
            )
            .await
            .expect("a report");
            let sent = fixture.requests_of("search");
            assert_eq!(
                sent.last().expect("a search").body,
                json!({ "query": report_text(kind, "2026-10-01", "2026-10-31") }),
                "{kind}"
            );
            assert_eq!(
                sent.last()
                    .and_then(crate::google_ads_fixture::Seen::customer),
                Some("1234567890".to_string())
            );
        }
        assert_eq!(fixture.requests_of("search").len(), 5);

        // Two years apart are not allowed, one day short of 367 are.
        for (from, to) in [("2025-01-01", "2026-01-02")] {
            report(
                &ads,
                &token(),
                &json!({ "account": ACCOUNT, "kind": "ads", "from": from, "to": to }),
            )
            .await
            .expect("366 days apart");
        }
    }

    #[tokio::test]
    async fn refuses_a_report_that_is_not_one_before_sending() {
        let (fixture, ads) = ads().await;
        let ask = |changes: Value| {
            let mut input = json!({ "account": ACCOUNT, "kind": "ads", "from": "2026-10-01", "to": "2026-10-31" });
            for (key, value) in changes.as_object().expect("an object") {
                input[key] = value.clone();
            }
            input
        };
        for changes in [
            json!({ "from": "2026-01-01' OR 1=1" }),
            json!({ "to": "2026-10-31' OR '1'='1" }),
            json!({ "from": "2026-02-30" }),
            json!({ "from": "2026-1-5" }),
            json!({ "from": "2026-10-31", "to": "2026-10-01" }),
            json!({ "from": "2025-01-01", "to": "2026-01-03" }),
            json!({ "kind": "ads; DROP" }),
            json!({ "kind": "free_query" }),
            json!({ "account": "123456789" }),
            json!({ "account": "123-456-789" }),
            json!({ "account": "123-456-7890 OR 1" }),
            json!({ "account": "customers/1234567890" }),
            json!({ "from": 20_261_001 }),
            json!({ "kind": null }),
        ] {
            let error = report(&ads, &token(), &ask(changes.clone()))
                .await
                .expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{changes}: {error:?}"
            );
        }
        assert!(fixture.requests().is_empty(), "nothing was sent");
    }

    #[tokio::test]
    async fn shapes_a_report_s_rows_and_cuts_them_at_500() {
        let (fixture, ads) = ads().await;
        fixture.script(|script| {
            script.rows = vec![json!({
                "campaign": { "resourceName": "customers/1234567890/campaigns/5", "name": "Boots", "status": "PAUSED" },
                "metrics": { "clicks": "12", "impressions": "340", "costMicros": "1500000", "conversions": 2.5 }
            })];
        });
        let input = json!({ "account": ACCOUNT, "kind": "campaigns", "from": "2026-10-01", "to": "2026-10-31" });
        let answer = report(&ads, &token(), &input).await.expect("a report");
        assert_eq!(
            answer,
            json!({
                "kind": "campaigns", "from": "2026-10-01", "to": "2026-10-31", "more": false,
                "rows": [{
                    "campaign": { "resource_name": "customers/1234567890/campaigns/5", "name": "Boots", "status": "PAUSED" },
                    "metrics": { "clicks": 12, "impressions": 340, "cost": "1.50", "conversions": 2.5 }
                }]
            })
        );
        fixture.script(|script| {
            script.rows = (0..501)
                .map(|n| json!({ "campaign": { "name": format!("c{n}") }, "metrics": { "costMicros": "1234567" } }))
                .collect();
        });
        let answer = report(&ads, &token(), &input).await.expect("a report");
        assert_eq!(answer["rows"].as_array().map(Vec::len), Some(500));
        assert_eq!(answer["more"], json!(true));
        assert_eq!(answer["rows"][0]["metrics"]["cost"], json!("1.234567"));
    }

    #[tokio::test]
    async fn lists_the_accounts_the_sign_in_reaches() {
        let (fixture, ads) = ads().await;
        let answer = list_accounts(&ads, &token()).await.expect("accounts");
        assert_eq!(
            answer,
            json!({
                "accounts": [
                    { "account": "123-456-7890", "name": "Shop", "currency": "USD", "time_zone": "America/New_York", "manager": false },
                    { "account": "234-567-8901", "name": "Agency", "currency": "EUR", "time_zone": "Europe/Paris", "manager": true }
                ],
                "more": false
            })
        );
        let query = "SELECT customer.descriptive_name, customer.currency_code, customer.time_zone, customer.manager FROM customer";
        assert_eq!(
            fixture
                .requests_of("search")
                .iter()
                .map(|seen| seen.body["query"].clone())
                .collect::<Vec<_>>(),
            [json!(query), json!(query)]
        );
        // One that cannot be read says so, and the others still answer; past 20 there is more.
        fixture.script(|script| {
            script.fail_search_containing = Some(("customer".to_string(), 500));
        });
        let answer = list_accounts(&ads, &token()).await.expect("accounts");
        assert_eq!(answer["accounts"][0]["account"], json!("123-456-7890"));
        assert!(answer["accounts"][0]["error"].is_string(), "{answer}");
        let (fixture, ads) = self::ads().await;
        fixture.script(|script| {
            script.customers = (0..21)
                .map(|n| {
                    (
                        format!("{:010}", 1_000_000_000 + n),
                        json!({ "descriptiveName": "x" }),
                    )
                })
                .collect();
        });
        let answer = list_accounts(&ads, &token()).await.expect("accounts");
        assert_eq!(answer["accounts"].as_array().map(Vec::len), Some(20));
        assert_eq!(answer["more"], json!(true));
    }

    #[tokio::test]
    async fn asks_for_keyword_ideas_and_shapes_them() {
        let (fixture, ads) = ads().await;
        fixture.script(|script| {
            script.ideas = vec![
                json!({
                    "text": "red boots",
                    "keywordIdeaMetrics": {
                        "avgMonthlySearches": "1900", "competition": "LOW",
                        "lowTopOfPageBidMicros": "500000", "highTopOfPageBidMicros": "2250000"
                    }
                }),
                json!({ "text": "boots" }),
            ];
        });
        let input = json!({
            "account": ACCOUNT, "words": ["boots", "red boots"], "language": 1000, "locations": [2840, 2826]
        });
        let answer = keyword_ideas(&ads, &token(), &input).await.expect("ideas");
        assert_eq!(
            answer,
            json!({ "ideas": [
                { "text": "red boots", "avg_monthly_searches": 1900, "competition": "low", "low_bid": "0.50", "high_bid": "2.25" },
                { "text": "boots", "avg_monthly_searches": null, "competition": null, "low_bid": null, "high_bid": null }
            ] })
        );
        assert_eq!(
            fixture.requests_of("generateKeywordIdeas")[0].body,
            json!({
                "language": "languageConstants/1000",
                "geoTargetConstants": ["geoTargetConstants/2840", "geoTargetConstants/2826"],
                "keywordPlanNetwork": "GOOGLE_SEARCH",
                "keywordSeed": { "keywords": ["boots", "red boots"] },
                "pageSize": 100
            })
        );
        // At most 100 ideas.
        fixture.script(|script| {
            script.ideas = (0..150)
                .map(|n| json!({ "text": format!("idea {n}") }))
                .collect();
        });
        let answer = keyword_ideas(&ads, &token(), &input).await.expect("ideas");
        assert_eq!(answer["ideas"].as_array().map(Vec::len), Some(100));
        // Inputs are checked before anything is sent.
        let before = fixture.requests().len();
        for change in [
            json!({ "words": [] }),
            json!({ "words": (0..11).map(|n| format!("w{n}")).collect::<Vec<_>>() }),
            json!({ "words": [""] }),
            json!({ "words": ["x".repeat(81)] }),
            json!({ "language": "en" }),
            json!({ "language": -1 }),
            json!({ "locations": [] }),
            json!({ "locations": (1..12).collect::<Vec<u64>>() }),
            json!({ "locations": ["2840"] }),
            json!({ "account": "1234567890" }),
        ] {
            let mut bad = input.clone();
            for (key, value) in change.as_object().expect("an object") {
                bad[key] = value.clone();
            }
            let error = keyword_ideas(&ads, &token(), &bad)
                .await
                .expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{change}: {error:?}"
            );
        }
        assert_eq!(fixture.requests().len(), before);
    }

    /// The input of a campaign that bids for clicks, within a ceiling of 1.50.
    fn campaign_input() -> Value {
        json!({
            "account": ACCOUNT, "plan_campaign": "search-launch", "name": "Launch search",
            "bidding": "maximize_clicks", "max_cpc": "1.50", "locations": [2840], "languages": [1000]
        })
    }

    fn new_campaign(input: &CampaignInput, budget: (BudgetKind, Amount)) -> NewCampaign<'_> {
        NewCampaign {
            input,
            plan: "MP-3",
            start: day("2026-11-03"),
            ends_on: day("2026-12-02"),
            budget,
        }
    }

    #[tokio::test]
    async fn creates_a_paused_search_campaign_in_one_request() {
        let (fixture, ads) = ads().await;
        let input = CampaignInput::parse(&campaign_input()).expect("a campaign");
        let total = campaign_operations(&new_campaign(&input, (BudgetKind::Total, Amount(30_000))));
        let made = ads.mutate(&token(), ACCOUNT, total).await.expect("made");
        assert_eq!(made.len(), 4);
        assert!(
            made[1].starts_with("customers/1234567890/campaigns/"),
            "{made:?}"
        );
        assert!(!made[1].contains("/-"), "a real id: {made:?}");

        let sent = fixture.requests_of("mutate");
        assert_eq!(sent.len(), 1, "one request");
        let budget = "customers/1234567890/campaignBudgets/-1";
        let campaign = "customers/1234567890/campaigns/-2";
        assert_eq!(
            sent[0].body,
            json!({ "mutateOperations": [
                { "campaignBudgetOperation": { "create": {
                    "resourceName": budget, "explicitlyShared": false, "deliveryMethod": "STANDARD",
                    "period": "CUSTOM_PERIOD", "totalAmountMicros": "300000000"
                } } },
                { "campaignOperation": { "create": {
                    "resourceName": campaign, "name": "MP-3 search-launch: Launch search",
                    "advertisingChannelType": "SEARCH", "status": "PAUSED", "campaignBudget": budget,
                    "networkSettings": { "targetGoogleSearch": true, "targetSearchNetwork": false, "targetContentNetwork": false },
                    "startDateTime": "2026-11-03 00:00:00", "endDateTime": "2026-12-02 23:59:59",
                    "containsEuPoliticalAdvertising": "DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING",
                    "targetSpend": { "cpcBidCeilingMicros": "1500000" }
                } } },
                { "campaignCriterionOperation": { "create": {
                    "campaign": campaign, "location": { "geoTargetConstant": "geoTargetConstants/2840" }
                } } },
                { "campaignCriterionOperation": { "create": {
                    "campaign": campaign, "language": { "languageConstant": "languageConstants/1000" }
                } } }
            ] })
        );

        // A daily budget is `amountMicros`, with no period and no total.
        let daily = campaign_operations(&new_campaign(&input, (BudgetKind::Daily, Amount(2_500))));
        assert_eq!(
            daily[0],
            json!({ "campaignBudgetOperation": { "create": {
                "resourceName": budget, "explicitlyShared": false, "deliveryMethod": "STANDARD",
                "amountMicros": "25000000"
            } } })
        );
        // Conversions have no ceiling and no click strategy; clicks without a ceiling have none.
        let mut conversions = campaign_input();
        conversions["bidding"] = json!("maximize_conversions");
        conversions
            .as_object_mut()
            .expect("an object")
            .remove("max_cpc");
        let conversions = CampaignInput::parse(&conversions).expect("conversions");
        let ops = campaign_operations(&new_campaign(
            &conversions,
            (BudgetKind::Total, Amount(100)),
        ));
        let created = &ops[1]["campaignOperation"]["create"];
        assert_eq!(created["maximizeConversions"], json!({}));
        assert!(created.get("targetSpend").is_none());
        let mut clicks = campaign_input();
        clicks.as_object_mut().expect("an object").remove("max_cpc");
        let clicks = CampaignInput::parse(&clicks).expect("clicks");
        let ops = campaign_operations(&new_campaign(&clicks, (BudgetKind::Total, Amount(100))));
        assert_eq!(
            ops[1]["campaignOperation"]["create"]["targetSpend"],
            json!({})
        );
        assert_eq!(
            ops[1]["campaignOperation"]["create"]["status"],
            json!("PAUSED")
        );
    }

    #[test]
    fn refuses_a_campaign_that_is_not_one() {
        for change in [
            json!({ "bidding": "manual_cpc" }),
            json!({ "bidding": "target_cpa" }),
            json!({ "bidding": "maximize_conversions" }),
            json!({ "max_cpc": "0" }),
            json!({ "max_cpc": "1.234" }),
            json!({ "max_cpc": "-1" }),
            json!({ "max_cpc": 1.5 }),
            json!({ "name": "" }),
            json!({ "name": "x".repeat(81) }),
            json!({ "plan_campaign": "Search Launch" }),
            json!({ "plan_campaign": "-x" }),
            json!({ "locations": [] }),
            json!({ "locations": (1..22).collect::<Vec<u64>>() }),
            json!({ "locations": ["2840"] }),
            json!({ "languages": [] }),
            json!({ "languages": (1..12).collect::<Vec<u64>>() }),
            json!({ "languages": [-1] }),
            json!({ "account": "1234567890" }),
        ] {
            let mut bad = campaign_input();
            for (key, value) in change.as_object().expect("an object") {
                bad[key] = value.clone();
            }
            let error = CampaignInput::parse(&bad).expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{change}: {error:?}"
            );
        }
        // The edges that load: 20 locations, 10 languages, an 80-character name.
        let mut edges = campaign_input();
        edges["locations"] = json!((1..=20).collect::<Vec<u64>>());
        edges["languages"] = json!((1..=10).collect::<Vec<u64>>());
        edges["name"] = json!("x".repeat(80));
        CampaignInput::parse(&edges).expect("the edges load");
        let mut conversions = campaign_input();
        conversions["bidding"] = json!("maximize_conversions");
        conversions
            .as_object_mut()
            .expect("an object")
            .remove("max_cpc");
        CampaignInput::parse(&conversions).expect("conversions load without a ceiling");
    }

    #[tokio::test]
    async fn creates_what_goes_under_a_campaign_enabled() {
        let (fixture, ads) = ads().await;
        let campaign = "customers/1234567890/campaigns/5";
        let ad_group = "customers/1234567890/adGroups/7";

        let group = AdGroupInput::parse(
            &json!({ "campaign": campaign, "name": "Boots", "cpc_bid": "0.75" }),
        )
        .expect("an ad group");
        assert_eq!(
            ad_group_operations(&group),
            [json!({ "adGroupOperation": { "create": {
                "campaign": campaign, "name": "Boots", "status": "ENABLED",
                "type": "SEARCH_STANDARD", "cpcBidMicros": "750000"
            } } })]
        );
        let bare = AdGroupInput::parse(&json!({ "campaign": campaign, "name": "Boots" }))
            .expect("an ad group without a bid");
        assert!(
            ad_group_operations(&bare)[0]["adGroupOperation"]["create"]
                .get("cpcBidMicros")
                .is_none()
        );

        let words = KeywordsInput::parse(
            &json!({ "ad_group": ad_group, "keywords": [
                { "text": "red boots", "match": "phrase" }, { "text": "boots", "match": "exact" },
                { "text": "shoes", "match": "broad" }
            ] }),
            "ad_group",
        )
        .expect("keywords");
        assert_eq!(
            keyword_operations(&words),
            [
                json!({ "adGroupCriterionOperation": { "create": {
                    "adGroup": ad_group, "status": "ENABLED", "keyword": { "text": "red boots", "matchType": "PHRASE" } } } }),
                json!({ "adGroupCriterionOperation": { "create": {
                    "adGroup": ad_group, "status": "ENABLED", "keyword": { "text": "boots", "matchType": "EXACT" } } } }),
                json!({ "adGroupCriterionOperation": { "create": {
                    "adGroup": ad_group, "status": "ENABLED", "keyword": { "text": "shoes", "matchType": "BROAD" } } } }),
            ]
        );

        let negatives = KeywordsInput::parse(
            &json!({ "campaign": campaign, "keywords": [{ "text": "free", "match": "broad" }] }),
            "campaign",
        )
        .expect("negative keywords");
        assert_eq!(
            negative_keyword_operations(&negatives),
            [json!({ "campaignCriterionOperation": { "create": {
                "campaign": campaign, "negative": true, "keyword": { "text": "free", "matchType": "BROAD" } } } })]
        );

        let ad = AdInput::parse(&json!({
            "ad_group": ad_group,
            "headlines": ["Boots", "Red boots", "Free shipping"],
            "descriptions": ["Boots made to last.", "Order today."],
            "final_url": "https://shop.example/boots", "path1": "boots", "path2": "sale"
        }))
        .expect("an ad");
        assert_eq!(
            ad_operations(&ad),
            [json!({ "adGroupAdOperation": { "create": {
                "adGroup": ad_group, "status": "ENABLED",
                "ad": {
                    "finalUrls": ["https://shop.example/boots"],
                    "responsiveSearchAd": {
                        "headlines": [{ "text": "Boots" }, { "text": "Red boots" }, { "text": "Free shipping" }],
                        "descriptions": [{ "text": "Boots made to last." }, { "text": "Order today." }],
                        "path1": "boots", "path2": "sale"
                    }
                }
            } } })]
        );

        // Each goes to Google as one request, and only what the operations say is sent.
        for ops in [ad_group_operations(&group), keyword_operations(&words)] {
            ads.mutate(&token(), ACCOUNT, ops).await.expect("made");
        }
        assert_eq!(fixture.requests_of("mutate").len(), 2);
    }

    #[test]
    fn refuses_what_goes_under_a_campaign_when_it_is_not_valid() {
        let campaign = "customers/1234567890/campaigns/5";
        let ad_group = "customers/1234567890/adGroups/7";
        let words = |text: &str, how: &str| json!({ "text": text, "match": how });
        let many = |n: usize| {
            (0..n)
                .map(|k| words(&format!("w{k}"), "exact"))
                .collect::<Vec<_>>()
        };
        for bad in [
            json!({ "ad_group": ad_group, "keywords": [] }),
            json!({ "ad_group": ad_group, "keywords": many(51) }),
            json!({ "ad_group": ad_group, "keywords": [words("", "exact")] }),
            json!({ "ad_group": ad_group, "keywords": [words(&"x".repeat(81), "exact")] }),
            json!({ "ad_group": ad_group, "keywords": [words("boots", "exact_match")] }),
            json!({ "ad_group": ad_group, "keywords": [words("boots", "EXACT")] }),
            json!({ "ad_group": campaign, "keywords": many(1) }),
            json!({ "ad_group": "customers/123/adGroups/7", "keywords": many(1) }),
            json!({ "ad_group": "customers/1234567890/adGroups/7/x", "keywords": many(1) }),
            json!({ "ad_group": "customers/1234567890/adGroups/", "keywords": many(1) }),
            json!({ "ad_group": "customers/1234567890/adGroups/123456789012345678901", "keywords": many(1) }),
            json!({ "keywords": many(1) }),
        ] {
            let error = KeywordsInput::parse(&bad, "ad_group").expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{bad}: {error:?}"
            );
        }
        KeywordsInput::parse(
            &json!({ "ad_group": ad_group, "keywords": many(50) }),
            "ad_group",
        )
        .expect("fifty load");
        KeywordsInput::parse(
            &json!({ "ad_group": ad_group, "keywords": [words(&"x".repeat(80), "phrase")] }),
            "ad_group",
        )
        .expect("eighty characters load");
        assert!(
            KeywordsInput::parse(
                &json!({ "campaign": ad_group, "keywords": many(1) }),
                "campaign"
            )
            .is_err()
        );
    }

    #[test]
    fn refuses_an_ad_or_an_ad_group_that_is_not_valid() {
        let campaign = "customers/1234567890/campaigns/5";
        let ad_group = "customers/1234567890/adGroups/7";
        let headlines = |n: usize| (0..n).map(|k| format!("Headline {k}")).collect::<Vec<_>>();
        let descriptions = |n: usize| {
            (0..n)
                .map(|k| format!("Description {k}"))
                .collect::<Vec<_>>()
        };
        let ad = |changes: Value| {
            let mut input = json!({
                "ad_group": ad_group, "headlines": headlines(3), "descriptions": descriptions(2),
                "final_url": "https://shop.example/boots"
            });
            for (key, value) in changes.as_object().expect("an object") {
                input[key] = value.clone();
            }
            input
        };
        AdInput::parse(&ad(json!({}))).expect("a plain ad");
        AdInput::parse(&ad(json!({
            "headlines": headlines(15), "descriptions": descriptions(4),
            "path1": "x".repeat(15), "path2": "y".repeat(15)
        })))
        .expect("the edges load");
        for changes in [
            json!({ "headlines": headlines(2) }),
            json!({ "headlines": headlines(16) }),
            json!({ "headlines": ["x".repeat(31), "b", "c"] }),
            json!({ "headlines": ["", "b", "c"] }),
            json!({ "descriptions": descriptions(1) }),
            json!({ "descriptions": descriptions(5) }),
            json!({ "descriptions": ["x".repeat(91), "b"] }),
            json!({ "final_url": "http://shop.example/boots" }),
            json!({ "final_url": "https://user:secret@shop.example/boots" }),
            json!({ "final_url": "https://user@shop.example" }),
            json!({ "final_url": "https:///boots" }),
            json!({ "final_url": "https://" }),
            json!({ "final_url": "javascript:alert(1)" }),
            json!({ "final_url": "https://shop.example/a b" }),
            json!({ "path1": "x".repeat(16) }),
            json!({ "path2": "x".repeat(16) }),
            json!({ "ad_group": campaign }),
        ] {
            let error = AdInput::parse(&ad(changes.clone())).expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{changes}: {error:?}"
            );
        }
        for bad in [
            json!({ "campaign": ad_group, "name": "x" }),
            json!({ "campaign": campaign, "name": "" }),
            json!({ "campaign": campaign, "name": "x".repeat(81) }),
            json!({ "campaign": campaign, "name": "x", "cpc_bid": "0" }),
            json!({ "campaign": campaign, "name": "x", "cpc_bid": "1.234" }),
            json!({ "name": "x" }),
        ] {
            let error = AdGroupInput::parse(&bad).expect_err("refused");
            assert!(
                matches!(error, GoogleAdsError::Input(_)),
                "{bad}: {error:?}"
            );
        }
    }

    #[test]
    fn changes_a_budget_and_a_status_by_update_mask() {
        let budget = "customers/1234567890/campaignBudgets/9";
        assert_eq!(
            budget_operations(budget, BudgetKind::Total, Amount(25_050)),
            [json!({ "campaignBudgetOperation": {
                "update": { "resourceName": budget, "totalAmountMicros": "250500000" },
                "updateMask": "totalAmountMicros"
            } })]
        );
        assert_eq!(
            budget_operations(budget, BudgetKind::Daily, Amount(1_339)),
            [json!({ "campaignBudgetOperation": {
                "update": { "resourceName": budget, "amountMicros": "13390000" },
                "updateMask": "amountMicros"
            } })]
        );
        let campaign = "customers/1234567890/campaigns/5";
        for (status, word) in [(Status::Paused, "PAUSED"), (Status::Enabled, "ENABLED")] {
            assert_eq!(
                status_operations(campaign, status),
                [json!({ "campaignOperation": {
                    "update": { "resourceName": campaign, "status": word },
                    "updateMask": "status"
                } })]
            );
        }
    }

    #[tokio::test]
    async fn says_google_s_refusal_in_farik_s_words() {
        let (fixture, ads) = ads().await;
        let input = json!({ "account": ACCOUNT, "words": ["boots"], "language": 1000, "locations": [2840] });

        // A refusal's message is cut at 300 characters, and quoted.
        fixture.script(|script| script.mode = Mode::LongError(1000));
        let GoogleAdsError::Google(words) = ads
            .search(&token(), ACCOUNT, "SELECT campaign.name FROM campaign")
            .await
            .expect_err("refused")
        else {
            panic!("Google refused it");
        };
        assert_eq!(words.matches('x').count(), 300, "{words}");
        assert!(words.contains('“') && words.contains('”'), "{words}");

        // PERMISSION_DENIED on keyword ideas is the sentence for an app Google has not allowed yet.
        fixture.script(|script| script.mode = Mode::PermissionDenied);
        assert_eq!(
            keyword_ideas(&ads, &token(), &input).await,
            Err(GoogleAdsError::NotAllowed(
                "Google has not yet allowed Farik's app to give keyword ideas.".to_string()
            ))
        );
        let GoogleAdsError::NotAllowed(words) = ads
            .search(&token(), ACCOUNT, "SELECT campaign.name FROM campaign")
            .await
            .expect_err("denied")
        else {
            panic!("Google would not allow it");
        };
        assert!(!words.contains("keyword ideas"), "{words}");

        // A sign-in Google does not accept says to sign in again.
        fixture.script(|script| script.mode = Mode::Unauthenticated);
        let GoogleAdsError::Failed(words) = ads.accessible(&token()).await.expect_err("401") else {
            panic!("a failure");
        };
        assert!(words.contains("sign in again"), "{words}");

        // No redirect is followed, and a body one byte over 4 MiB is refused.
        fixture.script(|script| script.mode = Mode::Redirect);
        let before = fixture.requests().len();
        let GoogleAdsError::Failed(words) = ads.accessible(&token()).await.expect_err("302") else {
            panic!("a failure");
        };
        assert!(words.contains("redirect"), "{words}");
        assert_eq!(fixture.requests().len(), before + 1);
        fixture.script(|script| script.mode = Mode::Oversized);
        let GoogleAdsError::Failed(words) = ads.accessible(&token()).await.expect_err("big") else {
            panic!("a failure");
        };
        assert!(words.contains("too large"), "{words}");
    }

    #[tokio::test]
    async fn takes_only_https_or_a_loopback_address() {
        for api in [
            "http://googleads.googleapis.com/v25",
            "ftp://127.0.0.1/v25",
            "http://10.0.0.1/v25",
            "not an address",
            "https://",
        ] {
            assert!(GoogleAds::new(api).is_err(), "{api}");
        }
        for api in [
            "https://googleads.googleapis.com/v25",
            "http://127.0.0.1:8080/v25",
            "http://localhost/v25",
            "http://[::1]:8080/v25",
        ] {
            assert!(GoogleAds::new(api).is_ok(), "{api}");
        }
    }

    #[tokio::test]
    async fn gives_up_on_a_silent_server() {
        use std::time::Instant;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let address = format!("http://{}/v25", listener.local_addr().expect("an address"));
        tokio::spawn(async move {
            // Accepts and never answers.
            let mut held = Vec::new();
            while let Ok(connection) = listener.accept().await {
                held.push(connection);
            }
        });
        let ads = GoogleAds::with_timeout(&address, Duration::from_secs(1)).expect("a client");
        let started = Instant::now();
        let GoogleAdsError::Failed(words) = ads.accessible(&token()).await.expect_err("no answer")
        else {
            panic!("a failure");
        };
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(words.contains("did not answer"), "{words}");
    }

    /// The environment's proxy is never used: a child copy of this test, with `HTTP_PROXY` set to a
    /// fixture that records, asks a second fixture directly.
    #[tokio::test]
    async fn uses_no_proxy_from_the_environment() {
        if let Ok(target) = std::env::var("FARIK_ADS_PROXY_CHILD") {
            GoogleAds::new(&target)
                .expect("a client")
                .accessible(&token())
                .await
                .expect("answered");
            return;
        }
        let proxy = Fixture::start().await;
        let target = Fixture::start().await;
        let through = proxy.address.trim_end_matches("/v25").to_string();
        let child = tokio::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "google_ads::tests::uses_no_proxy_from_the_environment",
            ])
            .env("FARIK_ADS_PROXY_CHILD", &target.address)
            .env("HTTP_PROXY", &through)
            .env("http_proxy", &through)
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

    #[test]
    fn asks_the_status_of_campaigns_with_no_metrics() {
        let campaigns = vec![
            "customers/1234567890/campaigns/11".to_string(),
            "customers/1234567890/campaigns/12".to_string(),
        ];
        // No metrics and no date: a campaign that cost nothing has its row all the same.
        assert_eq!(
            status_query(&campaigns),
            Ok(
                "SELECT campaign.resource_name, campaign.status FROM campaign WHERE \
                campaign.resource_name IN ('customers/1234567890/campaigns/11', \
                'customers/1234567890/campaigns/12')"
                    .to_string()
            )
        );
        for bad in [
            vec![],
            vec!["customers/1234567890/campaigns/11' OR '1'='1".to_string()],
            vec!["customers/1234567890/adGroups/1".to_string()],
            vec!["x".to_string()],
        ] {
            assert!(
                matches!(status_query(&bad), Err(GoogleAdsError::Input(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn builds_the_route_s_reads_from_fixed_text() {
        let campaigns = vec![
            "customers/1234567890/campaigns/11".to_string(),
            "customers/1234567890/campaigns/12".to_string(),
        ];
        assert_eq!(
            spend_query(&campaigns, day("2026-10-31"), day("2026-11-04")),
            Ok("SELECT campaign.resource_name, campaign.start_date_time, \
                campaign.end_date_time, campaign_budget.amount_micros, \
                campaign_budget.total_amount_micros, metrics.cost_micros FROM campaign WHERE \
                campaign.resource_name IN ('customers/1234567890/campaigns/11', \
                'customers/1234567890/campaigns/12') AND segments.date BETWEEN '2026-10-31' AND \
                '2026-11-04'"
                .to_string())
        );
        for bad in [
            vec![],
            vec!["customers/1234567890/campaigns/11' OR '1'='1".to_string()],
            vec!["customers/123/campaigns/1".to_string()],
            vec!["customers/1234567890/adGroups/1".to_string()],
            vec!["x".to_string()],
        ] {
            assert!(
                matches!(
                    spend_query(&bad, day("2026-10-31"), day("2026-11-04")),
                    Err(GoogleAdsError::Input(_))
                ),
                "{bad:?}"
            );
        }
        assert_eq!(
            ad_group_campaign_query("customers/1234567890/adGroups/7"),
            Ok(
                "SELECT ad_group.campaign FROM ad_group WHERE ad_group.resource_name = \
                'customers/1234567890/adGroups/7'"
                    .to_string()
            )
        );
        for bad in [
            "customers/1234567890/adGroups/7' OR 1=1",
            "customers/1234567890/campaigns/7",
            "",
        ] {
            assert!(
                matches!(ad_group_campaign_query(bad), Err(GoogleAdsError::Input(_))),
                "{bad}"
            );
        }
        assert_eq!(
            CUSTOMER_CURRENCY_QUERY,
            "SELECT customer.currency_code FROM customer"
        );
        assert_eq!(
            resource_customer("customers/1234567890/campaigns/11", "campaigns"),
            Some("1234567890".to_string())
        );
        assert_eq!(
            resource_customer("customers/1234567890/campaigns/11", "adGroups"),
            None
        );
        assert_eq!(
            resource_customer("customers/1234567890/campaigns/", "campaigns"),
            None
        );

        // The cost of a campaign, in micros, as Google writes a 64-bit number.
        let rows = vec![
            json!({ "campaign": { "resourceName": "customers/1234567890/campaigns/11" }, "metrics": { "costMicros": "1500000" } }),
            json!({ "campaign": { "resourceName": "customers/1234567890/campaigns/12" }, "metrics": { "costMicros": 250 } }),
            json!({ "campaign": { "resourceName": "customers/1234567890/campaigns/13" }, "metrics": {} }),
        ];
        assert_eq!(
            spend_by_campaign(&rows),
            std::collections::BTreeMap::from([
                ("customers/1234567890/campaigns/11".to_string(), 1_500_000),
                ("customers/1234567890/campaigns/12".to_string(), 250),
                ("customers/1234567890/campaigns/13".to_string(), 0),
            ])
        );
    }

    #[test]
    fn reads_what_google_holds_for_a_campaign() {
        // Google writes a 64-bit number as text, drops what is zero, and gives the end as a day
        // and a time of day in the account's time zone.
        let rows = vec![
            json!({
                "campaign": {
                    "resourceName": "customers/1234567890/campaigns/11",
                    "startDateTime": "2026-11-04 00:00:00",
                    "endDateTime": "2026-12-02 23:59:59"
                },
                "campaignBudget": { "totalAmountMicros": "500000000" },
                "metrics": { "costMicros": "1500000" }
            }),
            json!({
                "campaign": {
                    "resourceName": "customers/1234567890/campaigns/12",
                    "endDateTime": "2037-12-30 23:59:59"
                },
                "campaignBudget": { "amountMicros": 17_390_000 }
            }),
            json!({ "campaign": { "resourceName": "customers/1234567890/campaigns/13" } }),
            json!({
                "campaign": {
                    "resourceName": "customers/1234567890/campaigns/14",
                    "endDateTime": "not a date"
                }
            }),
        ];
        let held = held_by_campaign(&rows);
        assert_eq!(
            held.get("customers/1234567890/campaigns/11"),
            Some(&HeldRow {
                total_micros: Some(500_000_000),
                daily_micros: None,
                starts_on: Some(day("2026-11-04")),
                ends_on: Some(day("2026-12-02")),
            })
        );
        assert_eq!(
            held.get("customers/1234567890/campaigns/12"),
            Some(&HeldRow {
                total_micros: None,
                daily_micros: Some(17_390_000),
                starts_on: None,
                ends_on: Some(day("2037-12-30")),
            })
        );
        assert_eq!(
            held.get("customers/1234567890/campaigns/13"),
            Some(&HeldRow::default())
        );
        assert_eq!(
            held.get("customers/1234567890/campaigns/14"),
            Some(&HeldRow::default()),
            "a date that is not one is not given"
        );
    }

    #[test]
    fn reads_the_input_of_a_budget_and_a_status_change() {
        let campaign = "customers/1234567890/campaigns/5";
        assert_eq!(
            BudgetInput::parse(&json!({ "campaign": campaign, "amount": "250.5" })),
            Ok(BudgetInput {
                campaign: campaign.to_string(),
                amount: Amount(25_050)
            })
        );
        for bad in [
            json!({ "campaign": campaign }),
            json!({ "campaign": campaign, "amount": "0" }),
            json!({ "campaign": campaign, "amount": "1.234" }),
            json!({ "campaign": campaign, "amount": 5 }),
            json!({ "campaign": "customers/1234567890/adGroups/5", "amount": "5" }),
            json!({ "amount": "5" }),
        ] {
            assert!(
                matches!(BudgetInput::parse(&bad), Err(GoogleAdsError::Input(_))),
                "{bad}"
            );
        }
        for (word, status) in [("paused", Status::Paused), ("enabled", Status::Enabled)] {
            assert_eq!(
                StatusInput::parse(&json!({ "campaign": campaign, "status": word })),
                Ok(StatusInput {
                    campaign: campaign.to_string(),
                    status
                })
            );
        }
        for bad in [
            json!({ "campaign": campaign, "status": "removed" }),
            json!({ "campaign": campaign, "status": "ENABLED" }),
            json!({ "campaign": campaign }),
            json!({ "campaign": "x", "status": "paused" }),
        ] {
            assert!(
                matches!(StatusInput::parse(&bad), Err(GoogleAdsError::Input(_))),
                "{bad}"
            );
        }
    }

    /// A stand-in for the daemon's `POST /connector/call` on this computer: what it was sent, and
    /// what it answers with.
    /// What the stand-in was sent: the bearer, and the body.
    type Sent = Vec<(Option<String>, Value)>;

    struct Daemon {
        url: String,
        seen: std::sync::Arc<std::sync::Mutex<Sent>>,
    }

    impl Daemon {
        async fn start(status: u16, answer: Vec<u8>, redirect: bool) -> Self {
            use axum::Router;
            use axum::body::{Body, to_bytes};
            use axum::http::{HeaderMap, Response};

            let seen: std::sync::Arc<std::sync::Mutex<Sent>> = std::sync::Arc::default();
            let record = seen.clone();
            let app = Router::new().fallback(move |headers: HeaderMap, body: Body| {
                let (record, answer) = (record.clone(), answer.clone());
                async move {
                    let bytes = to_bytes(body, usize::MAX).await.unwrap_or_default();
                    record.lock().expect("the record").push((
                        headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
                    ));
                    if redirect {
                        return Response::builder()
                            .status(302)
                            .header("location", "http://127.0.0.1:9/elsewhere")
                            .body(Body::empty())
                            .expect("a response");
                    }
                    Response::builder()
                        .status(status)
                        .body(Body::from(answer))
                        .expect("a response")
                }
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a port");
            let url = format!(
                "http://{}/connector/call",
                listener.local_addr().expect("an address")
            );
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self { url, seen }
        }

        fn requests(&self) -> Sent {
            self.seen.lock().expect("the record").clone()
        }
    }

    #[test]
    fn the_shim_lists_ten_tools_offline() {
        let listed = super::descriptors();
        assert_eq!(
            listed
                .iter()
                .map(|tool| tool.name.to_string())
                .collect::<Vec<_>>(),
            tool_names()
        );
        for tool in &listed {
            assert_eq!(tool.input_schema["type"], json!("object"), "{}", tool.name);
            assert!(
                tool.description
                    .as_deref()
                    .is_some_and(|about| !about.is_empty())
            );
            let read = super::READ_TOOLS.contains(&&*tool.name);
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|notes| notes.read_only_hint),
                read.then_some(true),
                "{}",
                tool.name
            );
        }
        // Neither variable set: the list is the same, and building the shim asks nothing.
        super::Shim::new(None, None).expect("a shim");
    }

    #[tokio::test]
    async fn forwards_a_call_with_its_ticket() {
        let daemon = Daemon::start(
            200,
            json!({ "ok": { "accounts": [] } }).to_string().into_bytes(),
            false,
        )
        .await;
        let shim = super::Shim::new(Some(&daemon.url), Some("ticket-1")).expect("a shim");
        let answer = shim
            .call("list_accounts", &json!({}))
            .await
            .expect("answered");
        assert_eq!(
            serde_json::from_str::<Value>(&answer).expect("JSON"),
            json!({ "accounts": [] })
        );
        let sent = daemon.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.as_deref(), Some("Bearer ticket-1"));
        assert_eq!(
            sent[0].1,
            json!({ "tool": "list_accounts", "arguments": {} })
        );

        // The daemon's refusal is the tool's error, in its words.
        let daemon = Daemon::start(
            200,
            json!({ "error": "not_in_marketing_plan: the active plan has no campaign x" })
                .to_string()
                .into_bytes(),
            false,
        )
        .await;
        let shim = super::Shim::new(Some(&daemon.url), Some("ticket-1")).expect("a shim");
        assert_eq!(
            shim.call("create_search_campaign", &json!({ "a": 1 }))
                .await,
            Err("not_in_marketing_plan: the active plan has no campaign x".to_string())
        );
        assert_eq!(daemon.requests()[0].1["arguments"], json!({ "a": 1 }));
    }

    #[tokio::test]
    async fn says_what_the_daemon_could_not() {
        // A ticket the daemon does not accept, another status, an answer that is not JSON and
        // one that says neither what was done nor why not.
        for (status, body, says) in [
            (401, "", "ticket"),
            (500, "", "status 500"),
            (200, "no", "not JSON"),
            (200, "{}", "neither"),
        ] {
            let daemon = Daemon::start(status, body.as_bytes().to_vec(), false).await;
            let shim = super::Shim::new(Some(&daemon.url), Some("t")).expect("a shim");
            let error = shim.call("report", &json!({})).await.expect_err("refused");
            assert!(error.contains(says), "{status} {body}: {error}");
        }
        // No redirect is followed, and an answer over 4 MiB is refused.
        let daemon = Daemon::start(200, Vec::new(), true).await;
        let shim = super::Shim::new(Some(&daemon.url), Some("t")).expect("a shim");
        let error = shim.call("report", &json!({})).await.expect_err("302");
        assert!(error.contains("status 302"), "{error}");
        assert_eq!(daemon.requests().len(), 1);
        let daemon = Daemon::start(200, vec![b' '; 4 * 1024 * 1024 + 1], false).await;
        let shim = super::Shim::new(Some(&daemon.url), Some("t")).expect("a shim");
        let error = shim
            .call("report", &json!({}))
            .await
            .expect_err("too large");
        assert!(error.contains("too large"), "{error}");
    }

    #[tokio::test]
    async fn a_call_without_a_session_says_so() {
        let said = "Google Ads runs only inside a Farik session".to_string();
        for (url, ticket) in [
            (None, None),
            (Some("http://127.0.0.1:1/connector/call"), None),
            (None, Some("t")),
        ] {
            let shim = super::Shim::new(url, ticket).expect("a shim");
            assert_eq!(
                shim.call("list_accounts", &json!({})).await,
                Err(said.clone())
            );
        }
    }

    #[tokio::test]
    async fn refuses_a_url_that_is_not_the_daemon_s() {
        let daemon = Daemon::start(200, json!({ "ok": 1 }).to_string().into_bytes(), false).await;
        let port = daemon
            .url
            .split(':')
            .nth(2)
            .and_then(|rest| rest.split('/').next())
            .expect("a port")
            .to_string();
        for url in [
            "http://10.0.0.1:1/connector/call".to_string(),
            "https://127.0.0.1:1/connector/call".to_string(),
            "http://localhost:1/connector/call".to_string(),
            "http://127.0.0.1:1/other".to_string(),
            "http://127.0.0.1:1/connector/call/".to_string(),
            "http://127.0.0.1:1/connector/call?x=1".to_string(),
            "http://user@127.0.0.1:1/connector/call".to_string(),
            "http://127.0.0.1:/connector/call".to_string(),
            "http://127.0.0.1:0/connector/call".to_string(),
            "http://127.0.0.1:01/connector/call".to_string(),
            "http://127.0.0.1:65536/connector/call".to_string(),
            "http://127.0.0.1.evil.test:1/connector/call".to_string(),
            format!("http://127.0.0.1:{port}@10.0.0.1/connector/call"),
            String::new(),
        ] {
            let shim = super::Shim::new(Some(&url), Some("t")).expect("a shim");
            let error = shim
                .call("list_accounts", &json!({}))
                .await
                .expect_err("refused");
            assert!(error.contains("not where"), "{url}: {error}");
        }
        assert!(daemon.requests().is_empty(), "nothing was sent");
    }

    /// The environment's proxy is never used: a child copy of this test, with `HTTP_PROXY` set to
    /// a stand-in that records, calls a second one directly.
    #[tokio::test]
    async fn the_shim_uses_no_proxy_from_the_environment() {
        if let Ok(target) = std::env::var("FARIK_SHIM_PROXY_CHILD") {
            super::Shim::new(Some(&target), Some("t"))
                .expect("a shim")
                .call("list_accounts", &json!({}))
                .await
                .expect("answered");
            return;
        }
        let proxy = Daemon::start(200, json!({ "ok": 1 }).to_string().into_bytes(), false).await;
        let target = Daemon::start(200, json!({ "ok": 1 }).to_string().into_bytes(), false).await;
        let through = proxy.url.trim_end_matches("/connector/call").to_string();
        let child = tokio::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "google_ads::tests::the_shim_uses_no_proxy_from_the_environment",
            ])
            .env("FARIK_SHIM_PROXY_CHILD", &target.url)
            .env("HTTP_PROXY", &through)
            .env("http_proxy", &through)
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
}
