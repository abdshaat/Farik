//! The hub (`docs/SPEC.md` 8.1; ADR 0059): the one port `catervas serve` holds answers for every
//! team on the computer. Each team's daemon listens on a port of its own; the hub routes the
//! browser's pages to them, by `/t/<team>/rpc`, and `/rpc` to the current team.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use axum::Extension;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio_util::sync::CancellationToken;

use serde_json::{Value, json};

use super::web::{Failure, INTERNAL_ERROR, REFUSED};
use super::{DaemonState, SetupError, documents, gates, web};
use crate::slots::Slots;

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

/// Why a team is on this computer but not driven by this serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleStatus {
    /// Another process drives its project, and serve tries it again.
    Elsewhere,
    /// Its project has no team any more.
    Missing,
    /// Its drive ended with this error.
    Failed(String),
}

/// A team this serve does not drive, as `teams.list` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleTeam {
    /// Its name in an address.
    pub slug: TeamSlug,
    /// The team's name.
    pub name: String,
    /// Its project's folder.
    pub root: PathBuf,
    /// Why it is not driven.
    pub status: IdleStatus,
}

/// A team that was made beside the running ones, which serve is to drive (ADR 0059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamAdded {
    /// Its project's folder.
    pub root: PathBuf,
    /// Its name in an address.
    pub slug: TeamSlug,
}

/// The teams one serve drives, and the one `/rpc` answers for.
#[derive(Default)]
pub struct Hub {
    teams: RwLock<Vec<(TeamSlug, Arc<DaemonState>)>>,
    current: RwLock<Option<TeamSlug>>,
    /// The teams that are not driven, which serve keeps up to date.
    idle: Mutex<Vec<IdleTeam>>,
    /// The places, the account's sleep and the computer's day the teams share.
    slots: OnceLock<Arc<Slots>>,
    /// Where `teams.remove` says which team to stop and forget.
    removals: OnceLock<tokio::sync::mpsc::UnboundedSender<TeamSlug>>,
    /// Where `proposal.create` says which team to start driving.
    additions: OnceLock<tokio::sync::mpsc::UnboundedSender<TeamAdded>>,
    /// What finding projects keeps (`finding.*`).
    pub(super) finding: Arc<super::finding::Finding>,
    /// The daily reminder every page of the computer asks about (step 07).
    pub reminders: Arc<super::reminders::Reminders>,
}

impl Hub {
    /// The teams that are not driven.
    fn idle(&self) -> Vec<IdleTeam> {
        self.idle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Sets the teams that are not driven, which `teams.list` shows after the driven ones.
    pub fn set_idle(&self, idle: Vec<IdleTeam>) {
        *self
            .idle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = idle;
    }

    /// Gives the hub the limits the teams share, for `computer.get` and `computer.set`.
    pub fn set_slots(&self, slots: Arc<Slots>) {
        let _ = self.slots.set(slots);
    }

    /// Gives the hub where `teams.remove` sends the team to stop.
    pub fn set_removals(&self, removals: tokio::sync::mpsc::UnboundedSender<TeamSlug>) {
        let _ = self.removals.set(removals);
    }

    /// Gives the hub where `proposal.create` sends the team to drive.
    pub fn set_additions(&self, additions: tokio::sync::mpsc::UnboundedSender<TeamAdded>) {
        let _ = self.additions.set(additions);
    }

    /// Asks serve to drive the team at `root` as `slug`; `false` when nothing listens.
    pub(super) fn add_team(&self, added: TeamAdded) -> bool {
        self.additions
            .get()
            .is_some_and(|additions| additions.send(added).is_ok())
    }

    /// Asks serve to stop `slug` and forget its project; `false` when nothing listens.
    pub(super) fn remove_team(&self, slug: &TeamSlug) -> bool {
        self.removals
            .get()
            .is_some_and(|removals| removals.send(slug.clone()).is_ok())
    }

    /// Whether the hub drives the team `slug` or lists it as one it does not.
    pub(super) fn knows(&self, slug: &str) -> bool {
        self.team(slug).is_some() || self.idle().iter().any(|idle| idle.slug.as_str() == slug)
    }

    /// Gives the hub the host that scans the computer's Claude history.
    pub fn set_finder(&self, host: Arc<dyn super::finding::FindingHost>) {
        self.finding.set_host(host);
    }

    /// Stops the scan that runs, as serve ends.
    pub fn stop_finding(&self) {
        self.finding.stop();
    }

    /// Every team with the folder of its project: those this serve drives, then those it does not.
    pub(super) fn team_roots(&self) -> Vec<(String, PathBuf)> {
        let mut roots: Vec<(String, PathBuf)> = self
            .driven()
            .into_iter()
            .filter_map(|(slug, state)| {
                let root = state.web()?.project_root.clone();
                Some((slug.as_str().to_string(), root))
            })
            .collect();
        roots.extend(
            self.idle()
                .into_iter()
                .map(|idle| (idle.slug.as_str().to_string(), idle.root)),
        );
        roots
    }

    /// Drives `state` as the team `slug`; the first team is the current one.
    pub fn add(&self, slug: TeamSlug, state: Arc<DaemonState>) {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.is_none() {
            *current = Some(slug.clone());
        }
        let mut teams = self
            .teams
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        teams.retain(|(name, _)| *name != slug);
        state.set_reminders(slug.as_str(), Arc::clone(&self.reminders));
        teams.push((slug, state));
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
        let mut teams = self
            .teams
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let at = teams.iter().position(|(name, _)| name == slug)?;
        Some(teams.remove(at).1)
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

    /// Makes the team `slug` the one `/rpc` answers for; `false`, changing nothing, when there is
    /// none by that name.
    pub fn set_current(&self, slug: &str) -> bool {
        let found = self
            .teams
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(name, _)| name.as_str() == slug)
            .map(|(name, _)| name.clone());
        let known = found.is_some();
        if known {
            *self
                .current
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = found;
        }
        known
    }

    /// The name of the team `/rpc` answers for, when there is one.
    #[must_use]
    pub fn current_slug(&self) -> Option<TeamSlug> {
        self.current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Every driven team with its name in an address, in the order they were added.
    pub(super) fn driven(&self) -> Vec<(TeamSlug, Arc<DaemonState>)> {
        self.teams
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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

/// The methods the hub answers besides the wizard's (docs/schemas/rpc.schema.json).
pub(super) const METHODS: [&str; 3] = ["teams.current", "teams.remove", "computer.set"];

/// The hub of `state`, or the refusal of a call a team's own daemon cannot answer.
pub(super) fn hub_of(state: &DaemonState) -> Result<&Arc<Hub>, Failure> {
    state.hub().ok_or_else(|| {
        Failure::new(
            REFUSED,
            "catervas answers this on the hub, at /hub/rpc, not on a team's page",
        )
    })
}

/// `teams.current`, `teams.remove` and `computer.set`, whose params the schema already passed.
pub(super) fn call(
    state: &Arc<DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let hub = hub_of(state)?;
    match method {
        "teams.current" => {
            let slug = params["slug"].as_str().unwrap_or_default();
            if hub.set_current(slug) {
                Ok(json!({}))
            } else {
                Err(unknown(slug))
            }
        }
        "teams.remove" => {
            let slug = params["slug"].as_str().unwrap_or_default();
            if params["confirmed"].as_bool() != Some(true) {
                return Err(Failure::new(
                    REFUSED,
                    "confirmation_needed: removing a team stops it and forgets its project; \
                     ask again with confirmed set. No file is deleted.",
                ));
            }
            let known = hub.team(slug).is_some()
                || hub.idle().iter().any(|idle| idle.slug.as_str() == slug);
            if !known {
                return Err(unknown(slug));
            }
            let sent = hub
                .removals
                .get()
                .is_some_and(|removals| removals.send(TeamSlug::new(slug)).is_ok());
            if sent {
                Ok(json!({ "removed": true }))
            } else {
                Err(Failure::new(
                    INTERNAL_ERROR,
                    "this catervas has nothing to stop the team with",
                ))
            }
        }
        _ => set_limits(state, hub, params),
    }
}

/// The refusal of a slug no team has.
fn unknown(slug: &str) -> Failure {
    Failure::new(REFUSED, format!("team_unknown: there is no team {slug}"))
}

/// `computer.set`: checks the limits, keeps them where the host keeps them, and applies them.
fn set_limits(state: &DaemonState, hub: &Hub, params: &Value) -> Result<Value, Failure> {
    let agents = match params.get("agents_at_once") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(wanted) => {
            let limit = wanted
                .as_u64()
                .filter(|limit| (1..=16).contains(limit))
                .and_then(|limit| usize::try_from(limit).ok())
                .ok_or_else(|| {
                    Failure::new(
                        REFUSED,
                        "agents_at_once_out_of_range: agents at once is from 1 to 16",
                    )
                })?;
            Some(Some(limit))
        }
    };
    let cap = match params.get("daily_cap_usd") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(wanted) => {
            let usd = wanted
                .as_f64()
                .filter(|usd| *usd > 0.0 && *usd <= 100_000.0)
                .ok_or_else(|| {
                    Failure::new(
                        REFUSED,
                        "daily_cap_out_of_range: the daily limit is above 0 and at most 100000 dollars",
                    )
                })?;
            Some(Some(usd))
        }
    };
    if let Some(host) = state.host() {
        host.set_limits(agents, cap).map_err(|error| match error {
            SetupError::Refused(sentence) => Failure::new(REFUSED, sentence),
            SetupError::Failed(why) => Failure::new(INTERNAL_ERROR, why),
        })?;
    }
    if let Some(slots) = hub.slots.get() {
        if let Some(limit) = agents {
            slots.set_limit(limit);
        }
        if let Some(cap) = cap {
            slots.set_daily_cap(cap);
        }
    }
    Ok(json!({}))
}

/// `teams.list` and `computer.get`, read off the async workers.
pub(super) fn query(state: &DaemonState, name: &str) -> Result<Value, Failure> {
    let hub = hub_of(state)?;
    if name == "teams.list" {
        return teams_list(hub);
    }
    let slots = hub.slots.get();
    Ok(json!({
        "agents_at_once": slots.and_then(|slots| slots.limit()),
        "daily_cap_usd": slots.and_then(|slots| slots.daily_cap()),
        "spent_today_usd": slots.map_or(0.0, |slots| slots.spent_today_now()),
    }))
}

/// Every team: those this serve drives, then those it does not.
fn teams_list(hub: &Hub) -> Result<Value, Failure> {
    let internal = |error: &dyn std::fmt::Display| Failure::new(INTERNAL_ERROR, error.to_string());
    let mut teams = Vec::new();
    let made = super::finding::made_slugs(hub);
    for (slug, state) in hub.driven() {
        let (Some(deps), Some(web)) = (state.deps(), state.web()) else {
            continue;
        };
        deps.projections.catch_up().map_err(|e| internal(&e))?;
        let now = deps.clock.now();
        let paused = crate::pause::paused(&deps.log).map_err(|e| internal(&e))?;
        let name = deps
            .files
            .read_team()
            .map_or_else(|_| slug.as_str().to_string(), |team| team.name.to_string());
        let waiting = gates::query(&state, deps, "waiting.list", &json!({}))?["waiting"]
            .as_array()
            .map_or(0, Vec::len);
        teams.push(json!({
            "slug": slug.as_str(),
            "name": name,
            "root": web.project_root.display().to_string(),
            "status": if paused { "paused" } else { "running" },
            "working_agents": state.session_ids().len(),
            "waiting": waiting,
            "today_usd": crate::cost::day_usd(&deps.projections, now).map_err(|e| internal(&e))?,
            "month_usd": crate::cost::month_spent(&deps.projections, now).map_err(|e| internal(&e))?,
            "undoable": made.contains(slug.as_str()) && !super::finding::has_started(deps),
        }));
    }
    for idle in hub.idle() {
        let (status, failed) = match &idle.status {
            IdleStatus::Elsewhere => ("elsewhere", None),
            IdleStatus::Missing => ("missing", None),
            IdleStatus::Failed(why) => ("failed", Some(why.clone())),
        };
        let mut row = json!({
            "slug": idle.slug.as_str(),
            "name": idle.name,
            "root": idle.root.display().to_string(),
            "status": status,
            "working_agents": 0,
            "waiting": 0,
            "today_usd": 0.0,
            "month_usd": 0.0,
            "undoable": false,
        });
        if let Some(why) = failed {
            row["failed"] = json!(why);
        }
        teams.push(row);
    }
    Ok(json!({
        "teams": teams,
        "current": hub.current_slug().map(|slug| slug.as_str().to_string()),
        "account_asleep_until": hub
            .slots
            .get()
            .and_then(|slots| slots.asleep_until())
            .map(|until| until.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)),
    }))
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

    use super::{Hub, IdleStatus, IdleTeam, TeamSlug, slugs_for};
    use crate::daemon::web::{self, BrowserSessions, ConnectCodes, WebState};
    use crate::daemon::{DaemonConfig, DaemonHandle, PortChoice, serve};
    use crate::orchestrator::fixtures::Harness;
    use crate::slots::Slots;
    use crate::tools::fixtures::at;

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

    /// Fails unless `value` is what `definition` of the RPC schema says.
    fn conforms(value: &Value, definition: &str) {
        let schema: Value =
            serde_json::from_str(catervas_protocol::rpc::SCHEMA_JSON).expect("the schema is JSON");
        let root = json!({
            "$schema": schema["$schema"],
            "$ref": format!("#/$defs/{definition}"),
            "$defs": schema["$defs"],
        });
        let validator = jsonschema::options()
            .build(&root)
            .expect("the definition compiles");
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|error| error.to_string())
            .collect();
        assert!(errors.is_empty(), "{definition}: {errors:?} in {value}");
    }

    /// `method` of the hub's own state, answered as the socket would.
    async fn asked(hub: &Arc<crate::daemon::DaemonState>, method: &str, params: &Value) -> Value {
        let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        web::answer(hub, &frame.to_string(), &mut None).await
    }

    /// A query of the hub's own state.
    async fn queried(hub: &Arc<crate::daemon::DaemonState>, name: &str) -> Value {
        asked(hub, "query", &json!({ "name": name, "params": {} })).await
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one test of the hub's methods, each step reading the one before"
    )]
    async fn lists_every_team_with_its_state() {
        let (own, bakery, shop) = (
            Harness::new("hub-list-own", |_| {}),
            Harness::new("hub-list-bakery", |_| {}),
            Harness::new("hub-list-shop", |_| {}),
        );
        let hub = Arc::new(Hub::default());
        assert!(own.daemon.set_hub(Arc::clone(&hub)));
        let slots = Slots::with_clock(
            Some(2),
            Some(30.0),
            Arc::new(catervas_protocol::clock::FixedClock::new(at())),
        );
        hub.set_slots(Arc::clone(&slots));
        let (removals, mut removed) = tokio::sync::mpsc::unbounded_channel();
        hub.set_removals(removals);
        let handle = serve(
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
        hub.add(TeamSlug::new("bakery"), Arc::clone(&bakery.daemon));
        hub.add(TeamSlug::new("shop"), Arc::clone(&shop.daemon));
        // What each spent: today, earlier this month, and last month.
        bakery.spent(None, "s-1", 2.0);
        bakery.project.record_at(
            at() - chrono::Duration::days(40),
            "",
            "cost.recorded",
            &json!({
                "purpose": "implement", "model_id": "claude-opus-5",
                "usage": { "input_tokens": 1, "output_tokens": 1, "cache_read_tokens": 0, "cache_write_tokens": 0 },
                "cost_usd": 5.0
            }),
        );
        shop.spent(None, "s-1", 1.0);
        shop.project.record_at(
            at() - chrono::Duration::days(10),
            "",
            "cost.recorded",
            &json!({
                "purpose": "implement", "model_id": "claude-opus-5",
                "usage": { "input_tokens": 1, "output_tokens": 1, "cache_read_tokens": 0, "cache_write_tokens": 0 },
                "cost_usd": 4.0
            }),
        );
        shop.project
            .record("", "team.paused", &json!({ "by": "human" }));
        hub.set_idle(vec![
            IdleTeam {
                slug: TeamSlug::new("away"),
                name: "Away".to_string(),
                root: "/away".into(),
                status: IdleStatus::Elsewhere,
            },
            IdleTeam {
                slug: TeamSlug::new("broken"),
                name: "Broken".to_string(),
                root: "/broken".into(),
                status: IdleStatus::Failed("the log is gone".to_string()),
            },
        ]);
        slots.account_asleep_until(at() + chrono::Duration::hours(2));

        let listed = queried(&own.daemon, "teams.list").await;
        conforms(&listed["result"], "teamsListResult");
        let teams = listed["result"]["teams"].as_array().expect("teams");
        let row = |slug: &str| {
            teams
                .iter()
                .find(|team| team["slug"] == slug)
                .unwrap_or_else(|| panic!("no {slug} in {listed}"))
        };
        assert_eq!(teams.len(), 4, "{listed}");
        assert_eq!(row("bakery")["status"], "running");
        assert_eq!(row("shop")["status"], "paused");
        assert_eq!(row("bakery")["today_usd"], 2.0);
        assert_eq!(row("bakery")["month_usd"], 2.0);
        assert_eq!(row("shop")["today_usd"], 1.0);
        assert_eq!(row("shop")["month_usd"], 5.0);
        assert_eq!(row("bakery")["working_agents"], 0);
        assert_eq!(row("bakery")["waiting"], 0);
        assert_eq!(
            row("bakery")["root"],
            bakery.project.repo.path.display().to_string()
        );
        assert_eq!(row("away")["status"], "elsewhere");
        assert_eq!(row("broken")["status"], "failed");
        assert_eq!(row("broken")["failed"], "the log is gone");
        assert_eq!(listed["result"]["current"], "bakery");
        assert!(
            listed["result"]["account_asleep_until"].is_string(),
            "{listed}"
        );

        // The computer's limits and what the teams spent.
        let computer = queried(&own.daemon, "computer.get").await;
        conforms(&computer["result"], "computerGetResult");
        assert_eq!(computer["result"]["agents_at_once"], 2);
        assert_eq!(computer["result"]["daily_cap_usd"], 30.0);

        // `teams.current` changes what `/rpc` answers.
        let cookie = connected(port, &codes.issue().expect("a code")).await;
        let root = |harness: &Harness| harness.project.repo.path.display().to_string();
        let before = status_at(port, "/rpc", &cookie).await.expect("current");
        assert_eq!(before["result"]["project_root"], root(&bakery), "{before}");
        let current = asked(&own.daemon, "teams.current", &json!({ "slug": "shop" })).await;
        assert_eq!(current["result"], json!({}), "{current}");
        let after = status_at(port, "/rpc", &cookie).await.expect("current");
        assert_eq!(after["result"]["project_root"], root(&shop), "{after}");
        let unknown = asked(&own.daemon, "teams.current", &json!({ "slug": "nope" })).await;
        assert!(
            unknown["error"]["message"]
                .as_str()
                .is_some_and(|said| said.contains("team_unknown")),
            "{unknown}"
        );

        // `teams.remove` needs a confirmation, and then names the team to stop.
        let refused = asked(&own.daemon, "teams.remove", &json!({ "slug": "shop" })).await;
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|said| said.contains("confirmation_needed")),
            "{refused}"
        );
        assert!(removed.try_recv().is_err(), "nothing was removed");
        let done = asked(
            &own.daemon,
            "teams.remove",
            &json!({ "slug": "shop", "confirmed": true }),
        )
        .await;
        conforms(&done["result"], "teamsRemoveResult");
        assert_eq!(
            removed.try_recv().expect("the team to stop").as_str(),
            "shop"
        );
        handle.shutdown().await.expect("down");
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
