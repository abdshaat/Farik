//! A stand-in for the Google Ads API, in one axum server on loopback, for the tests of Catervas's own
//! Google Ads connector (phase 7 step 08f): it records every request and answers
//! `customers:listAccessibleCustomers`, `googleAds:search`, `googleAds:mutate` and
//! `generateKeywordIdeas` as Google does, with ways to answer an error with a long message, a
//! `PERMISSION_DENIED`, a redirect or an oversized body. No test calls Google. It is
//! `#[path]`-included by the tests that need it, so it uses only what every including crate has.
#![allow(dead_code, missing_docs, clippy::pedantic)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Response, StatusCode, Uri};
use serde_json::{Value, json};

/// One request the fixture was sent.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

impl Seen {
    /// The method of Google's own name for the request, such as `googleAds:mutate`, after the
    /// customer.
    pub fn method_name(&self) -> String {
        let last = self.path.rsplit('/').next().unwrap_or_default();
        last.rsplit_once(':')
            .map_or(last, |(_, name)| name)
            .to_string()
    }

    /// The customer id in the path, when there is one.
    pub fn customer(&self) -> Option<String> {
        let rest = self.path.strip_prefix("/v25/customers/")?;
        let id: String = rest.chars().take_while(char::is_ascii_digit).collect();
        (!id.is_empty()).then_some(id)
    }
}

/// How the fixture answers every request while it is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// An error answer whose message is this many characters long.
    LongError(usize),
    /// `403 PERMISSION_DENIED`.
    PermissionDenied,
    /// A redirect to another address, which Catervas never follows.
    Redirect,
    /// A success whose body is one byte over 4 MiB.
    Oversized,
    /// `401 UNAUTHENTICATED`.
    Unauthenticated,
}

/// What the fixture answers with.
pub struct Script {
    pub mode: Mode,
    /// The accessible customers, each with the fields `SELECT customer.…` answers.
    pub customers: Vec<(String, Value)>,
    /// The rows a `googleAds:search` of anything but a customer answers with.
    pub rows: Vec<Value>,
    /// A `googleAds:search` whose query holds this text answers with this status and an error.
    pub fail_search_containing: Option<(String, u16)>,
    /// What `generateKeywordIdeas` answers with.
    pub ideas: Vec<Value>,
    /// A `googleAds:mutate` answers with this status and an error, when set.
    pub fail_mutate: Option<u16>,
    /// A `googleAds:mutate` waits this long before it answers, so that two calls overlap.
    pub mutate_delay: Option<std::time::Duration>,
    /// A `googleAds:search` waits this long before it answers, so that something else can happen
    /// while a call reads.
    pub search_delay: Option<std::time::Duration>,
    /// The rows a `googleAds:search` of anything but a customer or a status answers with for a
    /// customer (ten digits), in place of `rows`: each ad account has its own campaigns' cost.
    pub customer_rows: BTreeMap<String, Vec<Value>>,
    /// The status each campaign (a resource name) has at Google, as a status read answers it:
    /// `ENABLED` for a campaign not named here, and no row at all for `MISSING`. A mutate that
    /// sets a status changes it.
    pub statuses: BTreeMap<String, String>,
    /// Campaigns whose pause Google refuses, with the words it refuses in (a `400`).
    pub refuse_pause: BTreeMap<String, String>,
    /// Access tokens Google does not allow into any account: a request bearing one is a `403
    /// PERMISSION_DENIED`, as a sign-in that does not reach the account is.
    pub deny_tokens: Vec<String>,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            customers: vec![
                (
                    "1234567890".to_string(),
                    json!({
                        "descriptiveName": "Shop", "currencyCode": "USD",
                        "timeZone": "America/New_York", "manager": false
                    }),
                ),
                (
                    "2345678901".to_string(),
                    json!({
                        "descriptiveName": "Agency", "currencyCode": "EUR",
                        "timeZone": "Europe/Paris", "manager": true
                    }),
                ),
            ],
            rows: Vec::new(),
            fail_search_containing: None,
            ideas: Vec::new(),
            fail_mutate: None,
            mutate_delay: None,
            search_delay: None,
            customer_rows: BTreeMap::new(),
            statuses: BTreeMap::new(),
            refuse_pause: BTreeMap::new(),
            deny_tokens: Vec::new(),
        }
    }
}

/// The fixture: its address, what it was sent, and what it answers with.
pub struct Fixture {
    pub address: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    script: Arc<Mutex<Script>>,
}

/// An error answer in Google's shape.
fn error_answer(status: u16, google_status: &str, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "error": { "code": status, "message": message, "status": google_status } })
                .to_string(),
        ))
        .expect("a response")
}

fn json_answer(value: &Value) -> Response<Body> {
    Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .expect("a response")
}

/// The collection a mutate operation's resource lives in, and the key of its result.
fn collection_of(operation: &str) -> Option<(&'static str, &'static str)> {
    Some(match operation {
        "campaignBudgetOperation" => ("campaignBudgets", "campaignBudgetResult"),
        "campaignOperation" => ("campaigns", "campaignResult"),
        "campaignCriterionOperation" => ("campaignCriteria", "campaignCriterionResult"),
        "adGroupOperation" => ("adGroups", "adGroupResult"),
        "adGroupCriterionOperation" => ("adGroupCriteria", "adGroupCriterionResult"),
        "adGroupAdOperation" => ("adGroupAds", "adGroupAdResult"),
        _ => return None,
    })
}

/// `googleAds:mutate`'s answer: each operation's result, a created resource named by a real id
/// where its operation used a temporary one.
fn mutate_answer(customer: &str, body: &Value, next: &AtomicU64) -> Response<Body> {
    let mut responses = Vec::new();
    for operation in body["mutateOperations"].as_array().into_iter().flatten() {
        let Some((name, value)) = operation
            .as_object()
            .and_then(|fields| fields.iter().next())
        else {
            return error_answer(400, "INVALID_ARGUMENT", "an empty operation");
        };
        let Some((collection, result)) = collection_of(name) else {
            return error_answer(
                400,
                "INVALID_ARGUMENT",
                "an operation Catervas does not make",
            );
        };
        let named = value["update"]["resourceName"].as_str().or_else(|| {
            value["create"]["resourceName"]
                .as_str()
                .filter(|r| !r.contains("/-"))
        });
        let resource = named.map_or_else(
            || {
                format!(
                    "customers/{customer}/{collection}/{}",
                    next.fetch_add(1, Ordering::SeqCst)
                )
            },
            str::to_string,
        );
        responses.push(json!({ result: { "resourceName": resource } }));
    }
    json_answer(&json!({ "mutateOperationResponses": responses }))
}

impl Fixture {
    pub async fn start() -> Self {
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let script: Arc<Mutex<Script>> = Arc::default();
        let next = Arc::new(AtomicU64::new(1000));
        let (record, rules) = (seen.clone(), script.clone());
        let app = Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Body| {
                let (record, rules, next) = (record.clone(), rules.clone(), next.clone());
                async move {
                    let bytes = to_bytes(body, usize::MAX).await.unwrap_or_default();
                    let one = Seen {
                        method: method.to_string(),
                        path: uri.path().to_string(),
                        headers: headers
                            .iter()
                            .map(|(name, value)| {
                                (
                                    name.as_str().to_string(),
                                    value.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect(),
                        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
                    };
                    record.lock().expect("the record").push(one.clone());
                    let delay = {
                        let script = rules.lock().expect("the script");
                        match one.method_name().as_str() {
                            "mutate" => script.mutate_delay,
                            "search" => script.search_delay,
                            _ => None,
                        }
                    };
                    if let Some(delay) = delay {
                        tokio::time::sleep(delay).await;
                    }
                    let mut script = rules.lock().expect("the script");
                    answer(&mut script, &one, &next)
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let address = format!("http://{}/v25", listener.local_addr().expect("an address"));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            address,
            seen,
            script,
        }
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().expect("the record").clone()
    }

    /// The requests of one Google method, such as `googleAds:mutate`'s `mutate`.
    pub fn requests_of(&self, method: &str) -> Vec<Seen> {
        self.requests()
            .into_iter()
            .filter(|seen| seen.method_name() == method)
            .collect()
    }

    /// Changes what the fixture answers with.
    pub fn script(&self, change: impl FnOnce(&mut Script)) {
        change(&mut self.script.lock().expect("the script"));
    }
}

/// The campaigns a query's `IN ('…', '…')` names.
fn named_in(query: &str) -> Vec<String> {
    let Some((_, rest)) = query.split_once(" IN (") else {
        return Vec::new();
    };
    let list = rest.split_once(')').map_or(rest, |(list, _)| list);
    list.split(',')
        .map(|name| name.trim().trim_matches('\'').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The campaign status changes a mutate asks for: a campaign and the status it is to have.
fn status_changes(body: &Value) -> Vec<(String, String)> {
    body["mutateOperations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|operation| {
            let update = &operation["campaignOperation"]["update"];
            Some((
                update["resourceName"].as_str()?.to_string(),
                update["status"].as_str()?.to_string(),
            ))
        })
        .collect()
}

fn answer(script: &mut Script, seen: &Seen, next: &AtomicU64) -> Response<Body> {
    let bearer = seen
        .headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    if bearer.is_some_and(|token| script.deny_tokens.iter().any(|denied| denied == token)) {
        return error_answer(
            403,
            "PERMISSION_DENIED",
            "The caller does not have permission",
        );
    }
    match &script.mode {
        Mode::Normal => {}
        Mode::LongError(length) => {
            return error_answer(400, "INVALID_ARGUMENT", &"x".repeat(*length));
        }
        Mode::PermissionDenied => {
            return error_answer(
                403,
                "PERMISSION_DENIED",
                "The caller does not have permission",
            );
        }
        Mode::Unauthenticated => {
            return error_answer(401, "UNAUTHENTICATED", "Request had invalid credentials");
        }
        Mode::Redirect => {
            return Response::builder()
                .status(StatusCode::FOUND)
                .header("location", "http://127.0.0.1:9/elsewhere")
                .body(Body::empty())
                .expect("a response");
        }
        Mode::Oversized => {
            return Response::new(Body::from(vec![b' '; 4 * 1024 * 1024 + 1]));
        }
    }
    let name = seen.method_name();
    match (seen.method.as_str(), name.as_str()) {
        ("GET", "listAccessibleCustomers") => json_answer(&json!({
            "resourceNames": script
                .customers
                .iter()
                .map(|(id, _)| format!("customers/{id}"))
                .collect::<Vec<_>>()
        })),
        ("POST", "search") => {
            let query = seen.body["query"].as_str().unwrap_or_default();
            if let Some((text, status)) = &script.fail_search_containing
                && query.contains(text.as_str())
            {
                return error_answer(*status, "INTERNAL", "the search failed");
            }
            if query.starts_with("SELECT customer.") {
                let customer = seen.customer().unwrap_or_default();
                let rows: Vec<Value> = script
                    .customers
                    .iter()
                    .filter(|(id, _)| *id == customer)
                    .map(|(_, fields)| json!({ "customer": fields }))
                    .collect();
                return json_answer(&json!({ "results": rows }));
            }
            if query.starts_with("SELECT campaign.resource_name, campaign.status FROM campaign")
                && !query.contains("metrics.")
            {
                let rows: Vec<Value> = named_in(query)
                    .into_iter()
                    .filter_map(|name| {
                        let status = script
                            .statuses
                            .get(&name)
                            .map_or("ENABLED", String::as_str)
                            .to_string();
                        (status != "MISSING").then(
                            || json!({ "campaign": { "resourceName": name, "status": status } }),
                        )
                    })
                    .collect();
                return json_answer(&json!({ "results": rows }));
            }
            let customer = seen.customer().unwrap_or_default();
            let rows = script.customer_rows.get(&customer).unwrap_or(&script.rows);
            json_answer(&json!({ "results": rows }))
        }
        ("POST", "mutate") => {
            if let Some(status) = script.fail_mutate {
                return error_answer(status, "INVALID_ARGUMENT", "the mutate failed");
            }
            let changes = status_changes(&seen.body);
            for (campaign, status) in &changes {
                if status == "PAUSED"
                    && let Some(words) = script.refuse_pause.get(campaign)
                {
                    return error_answer(400, "INVALID_ARGUMENT", words);
                }
            }
            for (campaign, status) in changes {
                script.statuses.insert(campaign, status);
            }
            mutate_answer(&seen.customer().unwrap_or_default(), &seen.body, next)
        }
        ("POST", "generateKeywordIdeas") => json_answer(&json!({ "results": script.ideas })),
        _ => error_answer(404, "NOT_FOUND", "no such method"),
    }
}
