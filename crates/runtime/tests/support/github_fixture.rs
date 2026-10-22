//! A stand-in for GitHub's API, in one axum server on loopback, for the tests of Catervas's GitHub
//! client (phase 10 step 06d): it answers hand-written, GitHub-shaped JSON about invented
//! repositories and records every request. No test calls GitHub.
#![allow(dead_code, missing_docs, clippy::pedantic)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, Response, StatusCode, Uri};
use serde_json::{Value, json};

/// One request the fixture was sent.
#[derive(Clone, Debug)]
pub struct Seen {
    pub path: String,
    pub headers: BTreeMap<String, String>,
}

/// How the fixture answers.
#[derive(Clone, Debug, Default)]
pub struct Script {
    /// The only token it accepts (`Authorization: Bearer <token>`); any other is a 401.
    pub token: String,
    /// Answers every request with a 403 and no rate left, resetting at this epoch second.
    pub rate_limited_until: Option<i64>,
    /// Answers every request with a plain 403 and rate left.
    pub forbidden: bool,
}

pub struct Fixture {
    pub address: String,
    pub script: Arc<Mutex<Script>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

/// Pushed on this date (invented): the three repositories.
pub fn repositories(now: chrono::DateTime<chrono::Utc>) -> Value {
    let days = |n: i64| (now - chrono::Duration::days(n)).to_rfc3339();
    json!([
        {"full_name": "octo/shop", "private": true, "pushed_at": days(2), "default_branch": "main",
         "clone_url": "https://github.com/octo/shop.git", "html_url": "https://github.com/octo/shop"},
        {"full_name": "octo/notes", "private": false, "pushed_at": days(40), "default_branch": "trunk",
         "clone_url": "https://github.com/octo/notes.git", "html_url": "https://github.com/octo/notes"},
        {"full_name": "octo/old-thing", "private": false, "pushed_at": days(400), "default_branch": "main",
         "clone_url": "https://github.com/octo/old-thing.git", "html_url": "https://github.com/octo/old-thing"}
    ])
}

pub fn branches() -> Value {
    json!([
        {"name": "main", "protected": true},
        {"name": "claude/fix-login", "protected": false},
        {"name": "claude/add-search-7Qx", "protected": false},
        {"name": "feature/other", "protected": false}
    ])
}

pub fn pulls() -> Value {
    json!([
        {"number": 12, "title": "Fix the login", "draft": true, "updated_at": "2026-10-20T09:00:00Z",
         "head": {"ref": "claude/fix-login"}},
        {"number": 13, "title": "Someone else's", "draft": false, "updated_at": "2026-10-19T09:00:00Z",
         "head": {"ref": "feature/other"}}
    ])
}

/// The workflows of an invented repository: one that is active, one GitHub turned off for lack of
/// changes. The extra fields are what GitHub also says, which Catervas does not read.
pub fn workflows() -> Value {
    json!({"total_count": 2, "workflows": [
        {"id": 1, "name": "Hourly price check", "path": ".github/workflows/price.yml",
         "state": "active", "badge_url": "https://example.invalid/NEVER-READ-badge"},
        {"id": 2, "name": "Weekly database backup", "path": ".github/workflows/backup.yml",
         "state": "disabled_inactivity", "html_url": "https://example.invalid/NEVER-READ-html"}
    ]})
}

/// The last scheduled run of a workflow: failed. `head_commit` is what Catervas does not read.
pub fn scheduled_run(file: &str) -> Value {
    if file == "backup.yml" {
        return json!({"total_count": 0, "workflow_runs": []});
    }
    json!({"total_count": 1, "workflow_runs": [{
        "id": 901, "status": "completed", "conclusion": "failure", "event": "schedule",
        "run_started_at": "2026-10-22T07:00:00Z",
        "head_commit": {"message": "NEVER-READ-commit-message",
                        "author": {"email": "never-read@example.invalid"}}
    }]})
}

pub fn deployments() -> Value {
    json!([{"id": 77, "environment": "Production", "created_at": "2026-10-21T18:40:00Z",
            "creator": {"login": "never-read"}, "payload": {"secret": "NEVER-READ-payload"}}])
}

pub fn deployment_statuses() -> Value {
    json!([{"id": 5, "state": "failure", "created_at": "2026-10-21T18:41:00Z",
            "description": "NEVER-READ-description", "environment": "Production"}])
}

fn answer(status: u16, headers: &[(&str, String)], body: &Value) -> Response<axum::body::Body> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status).expect("a status"))
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    builder
        .body(axum::body::Body::from(body.to_string()))
        .expect("a response")
}

impl Fixture {
    pub async fn start(token: &str, now: chrono::DateTime<chrono::Utc>) -> Self {
        let script = Arc::new(Mutex::new(Script {
            token: token.to_string(),
            ..Script::default()
        }));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (s, r) = (Arc::clone(&script), Arc::clone(&seen));
        let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap| {
            let (script, seen) = (Arc::clone(&s), Arc::clone(&r));
            async move {
                let path = uri
                    .path_and_query()
                    .map_or_else(|| uri.path().to_string(), ToString::to_string);
                let headers: BTreeMap<String, String> = headers
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().to_string(),
                            v.to_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect();
                let script = script.lock().expect("script").clone();
                seen.lock().expect("seen").push(Seen {
                    path: path.clone(),
                    headers: headers.clone(),
                });
                if let Some(reset) = script.rate_limited_until {
                    return answer(
                        403,
                        &[
                            ("x-ratelimit-remaining", "0".to_string()),
                            ("x-ratelimit-reset", reset.to_string()),
                        ],
                        &json!({"message": "API rate limit exceeded"}),
                    );
                }
                let want = format!("Bearer {}", script.token);
                if headers.get("authorization") != Some(&want) {
                    return answer(401, &[], &json!({"message": "Bad credentials"}));
                }
                if script.forbidden {
                    return answer(
                        403,
                        &[("x-ratelimit-remaining", "4000".to_string())],
                        &json!({"message": "Resource not accessible"}),
                    );
                }
                let route = path.split('?').next().unwrap_or_default();
                let body = match route {
                    "/user" => json!({"login": "octo", "id": 1, "email": "x@example.com"}),
                    "/user/repos" => repositories(now),
                    r if r.ends_with("/branches") => branches(),
                    r if r.ends_with("/pulls") => pulls(),
                    r if r.ends_with("/actions/workflows") => workflows(),
                    r if r.contains("/actions/workflows/") && r.ends_with("/runs") => {
                        let file = r
                            .rsplit_once("/runs")
                            .and_then(|(front, _)| front.rsplit('/').next())
                            .unwrap_or_default();
                        scheduled_run(file)
                    }
                    r if r.ends_with("/deployments") => deployments(),
                    r if r.contains("/deployments/") && r.ends_with("/statuses") => {
                        deployment_statuses()
                    }
                    _ => return answer(404, &[], &json!({"message": "Not Found"})),
                };
                answer(200, &[("x-ratelimit-remaining", "4000".to_string())], &body)
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let address = format!("http://{}", listener.local_addr().expect("an address"));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            address,
            script,
            seen,
        }
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().expect("seen").clone()
    }
}
