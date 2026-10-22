//! The hub (`docs/SPEC.md` 8.1; ADR 0059): the one port `catervas serve` holds answers for every
//! team on the computer. Each team's daemon listens on a port of its own; the hub routes the
//! browser's pages to them, by `/t/<team>/rpc`, and `/rpc` to the current team.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use axum::Extension;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio_util::sync::CancellationToken;

use super::{DaemonState, documents, web};

/// A team's name in an address: the slug of its team id, with `-2`, `-3` for a repeat.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TeamSlug(String);

impl TeamSlug {
    /// The slug `slug`.
    #[must_use]
    pub fn new(slug: &str) -> Self {
        TeamSlug(slug.to_string())
    }

    /// The slug as it is written in an address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The slugs of `team_ids`, in order: a team id that came before gets `-2` the second time, `-3`
/// the third, and so on.
#[must_use]
pub fn slugs_for(team_ids: &[String]) -> Vec<TeamSlug> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    team_ids
        .iter()
        .map(|id| {
            let times = seen.entry(id.as_str()).or_default();
            *times += 1;
            TeamSlug(if *times == 1 {
                id.clone()
            } else {
                format!("{id}-{times}")
            })
        })
        .collect()
}

/// The teams one serve drives, and the one `/rpc` answers for.
#[derive(Default)]
pub struct Hub {
    teams: RwLock<BTreeMap<TeamSlug, Arc<DaemonState>>>,
    current: RwLock<Option<TeamSlug>>,
}

impl Hub {
    /// Drives `state` as the team `slug`; the first team is the current one.
    pub fn add(&self, slug: TeamSlug, state: Arc<DaemonState>) {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.is_none() {
            *current = Some(slug.clone());
        }
        self.teams
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(slug, state);
    }

    /// Stops answering for `slug`, giving its state back; the current team is none then, when it
    /// was that one.
    pub fn remove(&self, slug: &TeamSlug) -> Option<Arc<DaemonState>> {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.as_ref() == Some(slug) {
            *current = None;
        }
        self.teams
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(slug)
    }

    /// The team `slug` names.
    #[must_use]
    pub fn team(&self, slug: &str) -> Option<Arc<DaemonState>> {
        self.teams
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(name, _)| name.as_str() == slug)
            .map(|(_, state)| Arc::clone(state))
    }

    /// Makes the team `slug` the one `/rpc` answers for; `false` when there is none by that name.
    pub fn set_current(&self, slug: &str) -> bool {
        let found = self
            .teams
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .find(|name| name.as_str() == slug)
            .cloned();
        let known = found.is_some();
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = found;
        known
    }

    /// The team `/rpc` answers for, when there is one.
    #[must_use]
    pub fn current(&self) -> Option<Arc<DaemonState>> {
        let current = self
            .current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        self.team(current.as_str())
    }
}

/// `GET /t/<team>/rpc`: the socket of that team's page, answered by its daemon's state. 404 for a
/// team the hub does not drive.
pub(super) async fn team_rpc(
    State(hub): State<Arc<DaemonState>>,
    Path(team): Path<String>,
    Extension(hub_cancel): Extension<CancellationToken>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    match hub.hub().and_then(|hub| hub.team(&team)) {
        Some(state) => {
            // The page of a team ends with that team's daemon, not with the hub, which is served
            // again each time a team is added.
            let cancel = state.closing().cloned().unwrap_or(hub_cancel);
            web::rpc(State(state), Extension(cancel), headers, upgrade).await
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /rpc` on the hub: the current team's socket while there is a current team, else the hub's
/// own, which answers the wizard.
pub(super) async fn current_rpc(
    State(hub): State<Arc<DaemonState>>,
    cancel: Extension<CancellationToken>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let state = hub.hub().and_then(|teams| teams.current()).unwrap_or(hub);
    web::rpc(State(state), cancel, headers, upgrade).await
}

/// `GET /t/<team>/documents/raw`: that team's document, as `GET /documents/raw` serves it.
pub(super) async fn team_documents(
    State(hub): State<Arc<DaemonState>>,
    Path(team): Path<String>,
    headers: HeaderMap,
    query: RawQuery,
) -> Response {
    match hub.hub().and_then(|hub| hub.team(&team)) {
        Some(state) => documents::raw(State(state), headers, query).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /documents/raw` on the hub: the current team's while there is one.
pub(super) async fn current_documents(
    State(hub): State<Arc<DaemonState>>,
    headers: HeaderMap,
    query: RawQuery,
) -> Response {
    let state = hub.hub().and_then(|teams| teams.current()).unwrap_or(hub);
    documents::raw(State(state), headers, query).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::{SinkExt as _, StreamExt as _};
    use serde_json::{Value, json};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    use super::{Hub, TeamSlug, slugs_for};
    use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
    use crate::daemon::{DaemonConfig, DaemonHandle, PortChoice, serve};
    use crate::orchestrator::fixtures::Harness;

    fn slug(name: &str) -> TeamSlug {
        slugs_for(&[name.to_string()]).remove(0)
    }

    #[test]
    fn names_teams_once() {
        let ids = ["bakery", "shop", "bakery"].map(String::from);
        let names: Vec<String> = slugs_for(&ids)
            .iter()
            .map(|slug| slug.as_str().to_string())
            .collect();
        assert_eq!(names, ["bakery", "shop", "bakery-2"]);
    }

    /// `harness`'s daemon with the browser routes on, over `port`, sharing `sessions` and `codes`.
    fn paged(harness: &Harness, port: u16, sessions: &BrowserSessions, codes: &ConnectCodes) {
        let deps = harness.daemon.deps().expect("a project");
        assert!(harness.daemon.set_web(WebState {
            codes: codes.clone(),
            sessions: sessions.clone(),
            project_root: harness.project.repo.path.clone(),
            account: Arc::new(crate::account::fixtures::FakeAccount::new()),
            kept_key_removed: false,
            port,
            clock: Arc::clone(&deps.clock),
            take_on_error: std::sync::Mutex::default(),
            leaving: std::sync::Mutex::default(),
            env: std::collections::BTreeMap::new(),
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
    }

    /// The `serve.status` a socket at `path` of the hub on `port` answers, with `cookie`; `None`
    /// when the socket is refused.
    async fn status_at(port: u16, path: &str, cookie: &str) -> Option<Value> {
        let mut request = format!("ws://127.0.0.1:{port}{path}")
            .into_client_request()
            .expect("a request");
        let headers = request.headers_mut();
        headers.insert(
            "origin",
            format!("http://127.0.0.1:{port}").parse().expect("header"),
        );
        headers.insert("cookie", cookie.parse().expect("header"));
        let (mut socket, _) = connect_async(request).await.ok()?;
        let frame = json!({
            "jsonrpc": "2.0", "id": 1, "method": "query",
            "params": { "name": "serve.status", "params": {} },
        });
        socket
            .send(Message::Text(frame.to_string().into()))
            .await
            .expect("sent");
        loop {
            if let Message::Text(text) = socket.next().await?.ok()? {
                return serde_json::from_str(text.as_str()).ok();
            }
        }
    }

    /// Trades `code` at the hub's `/connect` for the session cookie.
    async fn connected(port: u16, code: &str) -> String {
        let answer = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/connect"))
            .header("origin", format!("http://127.0.0.1:{port}"))
            .json(&json!({ "code": code }))
            .send()
            .await
            .expect("connect answers");
        assert_eq!(answer.status().as_u16(), 204);
        let set = answer
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("a cookie");
        set.split(';').next().expect("a pair").to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn routes_a_page_to_its_team() {
        let (own, bakery, shop) = (
            Harness::new("hub-own", |_| {}),
            Harness::new("hub-bakery", |_| {}),
            Harness::new("hub-shop", |_| {}),
        );
        let hub = Arc::new(Hub::default());
        assert!(own.daemon.set_hub(Arc::clone(&hub)));
        let handle: DaemonHandle = serve(
            DaemonConfig {
                port: PortChoice::Any,
                daemon_file: None,
            },
            Arc::clone(&own.daemon),
        )
        .await
        .expect("the hub is up");
        let port = handle.info.port;
        let (sessions, codes) = (
            BrowserSessions::open(None).expect("sessions"),
            ConnectCodes::default(),
        );
        for harness in [&own, &bakery, &shop] {
            paged(harness, port, &sessions, &codes);
        }
        hub.add(slug("bakery"), Arc::clone(&bakery.daemon));
        hub.add(slug("shop"), Arc::clone(&shop.daemon));
        assert!(hub.set_current("shop"));

        let cookie = connected(port, &codes.issue().expect("a code")).await;
        let root = |harness: &Harness| harness.project.repo.path.display().to_string();
        let asked = status_at(port, "/t/bakery/rpc", &cookie)
            .await
            .expect("bakery");
        assert_eq!(asked["result"]["project_root"], root(&bakery), "{asked}");
        let asked = status_at(port, "/t/shop/rpc", &cookie).await.expect("shop");
        assert_eq!(asked["result"]["project_root"], root(&shop), "{asked}");
        let asked = status_at(port, "/rpc", &cookie).await.expect("current");
        assert_eq!(asked["result"]["project_root"], root(&shop), "{asked}");
        assert!(status_at(port, "/t/nope/rpc", &cookie).await.is_none());
        handle.shutdown().await.expect("down");
    }
}
