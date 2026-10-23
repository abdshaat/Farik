//! The daily reminder behind the hub (`docs/SPEC.md` 4.4, F12; ADR 0059): the digest of what needs
//! the owner, the actions on its items, and the one notification a digest sends. The rules are
//! `catervas_core::reminders`; what they read comes from `catervas_store::reminders` and from the
//! teams this serve drives. Nothing here is a team's event.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use catervas_core::reminders::{
    AgedFact, GithubPage, Group, Reminder, ReminderInput, Target, TeamFact, WaitingFact, headline,
    reminders,
};
use catervas_protocol::event::EventKind;
use catervas_store::EventQuery;
use catervas_store::machine::{MACHINE_DB, open_machine_index};
use catervas_store::reminders::{ReminderSources, services_needing_sign_in};
use catervas_store::waiting::{name_of, waiting};
use chrono::{DateTime, Duration as Days, FixedOffset, NaiveTime, Offset as _, SecondsFormat, Utc};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::finding::{host_of, open_index};
use super::hub::{Hub, hub_of};
use super::web::{Failure, INTERNAL_ERROR, REFUSED};
use super::{DaemonState, gates};
use crate::github;

/// The methods of the daily reminder: the hub answers five and a team's daemon one
/// (docs/schemas/rpc.schema.json).
pub(super) const METHODS: [&str; 6] = [
    "reminders.get",
    "reminders.set",
    "reminders.snooze",
    "reminders.dismiss",
    "reminders.give",
    "reminders.mine",
];

/// The groups in the order they are shown.
const GROUPS: [Group; 6] = [
    Group::Forgotten,
    Group::Ending,
    Group::Schedules,
    Group::Teams,
    Group::Services,
    Group::Runs,
];

/// When the daily reminder comes and how it is told, as the owner set it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderSettings {
    /// The computer's local time, `HH:MM`.
    pub time: String,
    /// Whether a Catervas tab that is open gets a browser notice.
    pub browser: bool,
    /// Whether the operating system gets one when no tab is open.
    pub system: bool,
}

impl Default for ReminderSettings {
    fn default() -> Self {
        Self {
            time: "09:00".to_string(),
            browser: true,
            system: true,
        }
    }
}

/// What `reminders.set` changes; a field left out is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReminderChange {
    /// The time, `HH:MM`.
    pub time: Option<String>,
    /// The browser switch.
    pub browser: Option<bool>,
    /// The operating system switch.
    pub system: Option<bool>,
}

/// `text` as a time of day when it is `HH:MM`, two digits each.
#[must_use]
pub fn parse_time(text: &str) -> Option<NaiveTime> {
    let (hours, minutes) = text.split_once(':')?;
    let two_digits = |part: &str| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit());
    if !two_digits(hours) || !two_digits(minutes) {
        return None;
    }
    NaiveTime::from_hms_opt(hours.parse().ok()?, minutes.parse().ok()?, 0)
}

/// How long from `now` until the next `time` on a clock `offset_secs` from UTC: today's if it has
/// not come, else tomorrow's; at least a second.
#[must_use]
pub fn until_next(now: DateTime<Utc>, offset_secs: i32, time: NaiveTime) -> Duration {
    let offset = FixedOffset::east_opt(offset_secs).unwrap_or_else(|| Utc.fix());
    let local = now.with_timezone(&offset).naive_local();
    let mut due = local.date().and_time(time);
    if due <= local {
        due += Days::days(1);
    }
    (due - local)
        .to_std()
        .unwrap_or(Duration::from_secs(60))
        .max(Duration::from_secs(1))
}

/// One item of a digest with the team it belongs to, when it belongs to one.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The item.
    pub reminder: Reminder,
    /// The name in an address of the team whose project it is about.
    pub team: Option<String>,
    /// That team's name.
    pub team_name: Option<String>,
}

/// What needs the owner, as computed at one time.
#[derive(Debug, Clone)]
pub struct Digest {
    /// When it was computed.
    pub at: DateTime<Utc>,
    /// Its items, in the order of their groups.
    pub entries: Vec<Entry>,
    /// How the owner was told: `browser`, `system`, or `none` when the program failed.
    pub notified: Option<&'static str>,
    /// The owner's home, for the folders the items name.
    pub home: PathBuf,
}

impl Digest {
    /// The items that need the owner: all but today's runs.
    fn counted(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.reminder.group != Group::Runs)
    }

    /// How many things need the owner.
    #[must_use]
    pub fn count(&self) -> usize {
        self.counted().count()
    }

    /// The first two items' short words.
    #[must_use]
    pub fn headline(&self) -> String {
        let items: Vec<Reminder> = self
            .entries
            .iter()
            .map(|entry| entry.reminder.clone())
            .collect();
        headline(&items)
    }
}

/// Makes a digest from the host, the index and the teams.
type Computer = Arc<dyn Fn() -> Result<Digest, String> + Send + Sync>;

/// What every page of the computer asks of the daily reminder.
pub struct Reminders {
    digest: Mutex<Option<Digest>>,
    pushes: broadcast::Sender<Value>,
    pages: AtomicUsize,
    changed: tokio::sync::Notify,
    computer: OnceLock<Computer>,
}

impl Default for Reminders {
    fn default() -> Self {
        Self {
            digest: Mutex::new(None),
            pushes: broadcast::channel(16).0,
            pages: AtomicUsize::new(0),
            changed: tokio::sync::Notify::new(),
            computer: OnceLock::new(),
        }
    }
}

/// Counts a page while it is held: a socket of a Catervas tab is open.
pub struct PageGuard(Arc<Reminders>);

impl Drop for PageGuard {
    fn drop(&mut self) {
        self.0.pages.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Reminders {
    fn digest(&self) -> std::sync::MutexGuard<'_, Option<Digest>> {
        self.digest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Counts a page until the guard is dropped.
    #[must_use]
    pub fn page_opened(self: &Arc<Self>) -> PageGuard {
        self.pages.fetch_add(1, Ordering::SeqCst);
        PageGuard(Arc::clone(self))
    }

    /// How many pages are open.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.pages.load(Ordering::SeqCst)
    }

    /// Keeps `digest` as the one pages are answered from.
    pub fn replace(&self, digest: Digest) {
        *self.digest() = Some(digest);
    }

    /// What the hub's sockets are told: `reminders.due` as a whole frame.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.pushes.subscribe()
    }

    /// The digest kept, when there is one.
    fn kept(&self) -> Option<Digest> {
        self.digest().clone()
    }

    /// Takes the item `key` out of the kept digest.
    fn forget(&self, key: &str) {
        if let Some(digest) = self.digest().as_mut() {
            digest.entries.retain(|entry| entry.reminder.key != key);
        }
    }

    /// The kept digest, or one computed at once by the computer the daily timer gave, off the async
    /// workers.
    pub(super) fn current(&self) -> Result<Digest, String> {
        if let Some(digest) = self.kept() {
            return Ok(digest);
        }
        let computer = self
            .computer
            .get()
            .ok_or_else(|| "this catervas is not looking at what needs you".to_string())?;
        let digest = computer()?;
        self.replace(digest.clone());
        Ok(digest)
    }
}

fn internal(why: &str) -> Failure {
    Failure::new(INTERNAL_ERROR, why)
}

/// A refusal: its code, then its sentence.
fn refusal(code: &str, sentence: &str) -> Failure {
    Failure::new(REFUSED, format!("{code}: {sentence}"))
}

/// A sentence that begins with a code as a refusal, any other as a failure.
fn sentence_failure(why: &str) -> Failure {
    let coded = why
        .split_once(": ")
        .is_some_and(|(code, _)| code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
    if coded {
        Failure::new(REFUSED, why)
    } else {
        internal(why)
    }
}

/// The root of the project of each team this serve drives, with the roots of its member
/// repositories, and its name.
pub(super) struct TeamRoots {
    pub(super) slug: String,
    pub(super) name: String,
    pub(super) roots: Vec<PathBuf>,
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The project and member roots of the team `slug` that `state` drives, and its name.
pub(super) fn roots_of(slug: &str, state: &DaemonState) -> Option<TeamRoots> {
    let deps = state.deps()?;
    let root = state.web().map_or_else(
        || deps.files.root().to_path_buf(),
        |web| web.project_root.clone(),
    );
    let mut roots = vec![canonical(&root)];
    if let Ok(Some(repos)) = deps.files.read_repos() {
        roots.extend(
            repos
                .repos
                .iter()
                .map(|repo| canonical(Path::new(repo.path.as_str()))),
        );
    }
    let name = deps
        .files
        .read_team()
        .map_or_else(|_| slug.to_string(), |team| team.name.to_string());
    Some(TeamRoots {
        slug: slug.to_string(),
        name,
        roots,
    })
}

fn team_roots(hub: &Hub) -> Vec<TeamRoots> {
    hub.driven()
        .into_iter()
        .filter_map(|(slug, state)| roots_of(slug.as_str(), &state))
        .collect()
}

/// What a driven team has waited on the owner for.
pub(super) fn team_fact(slug: &str, state: &DaemonState) -> Option<TeamFact> {
    let deps = state.deps()?;
    deps.projections.catch_up().ok()?;
    let team = deps.files.read_team().ok()?;
    let listed = waiting(&deps.projections, &deps.log, &deps.files, &team).ok()?;
    let board = deps.projections.board().ok()?;
    let since_of = |task_id: &catervas_core::contract::TaskId| -> Option<DateTime<Utc>> {
        let seq = board
            .iter()
            .find(|row| &row.task_id == task_id)?
            .updated_seq;
        deps.log
            .read(&EventQuery {
                after_seq: Some(seq.saturating_sub(1)),
                limit: Some(1),
                ..EventQuery::default()
            })
            .ok()?
            .first()
            .map(|event| event.envelope.recorded_at)
    };
    let oldest = listed
        .iter()
        .filter_map(|item| {
            Some(WaitingFact {
                agent: item
                    .agent_id
                    .as_deref()
                    .map_or_else(|| "The team".to_string(), |id| name_of(&team, id)),
                title: item.title.clone(),
                since: since_of(&item.task_id)?,
            })
        })
        .min_by_key(|fact| fact.since);
    let mut aged: Vec<AgedFact> = Vec::new();
    let events = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::EscalationAged],
            ..EventQuery::default()
        })
        .ok()?;
    for event in events {
        let Some(task) = &event.envelope.ids.task_id else {
            continue;
        };
        let Some(item) = listed.iter().find(|item| &item.task_id == task) else {
            continue;
        };
        let fact = AgedFact {
            task_id: task.to_string(),
            title: item.title.clone(),
            since: event.envelope.recorded_at,
        };
        match aged.iter_mut().find(|seen| seen.task_id == fact.task_id) {
            Some(seen) => *seen = fact,
            None => aged.push(fact),
        }
    }
    Some(TeamFact {
        slug: slug.to_string(),
        name: team.name.to_string(),
        oldest_waiting: oldest,
        aged,
    })
}

/// What each driven team waits on the owner for.
fn team_facts(hub: &Hub) -> Vec<TeamFact> {
    hub.driven()
        .into_iter()
        .filter_map(|(slug, state)| team_fact(slug.as_str(), &state))
        .collect()
}

/// The digest of `input` at `now`: the rules' items, each with the team whose project it is about.
pub(super) fn build(hub: &Hub, input: &ReminderInput, now: DateTime<Utc>) -> Digest {
    let teams = team_roots(hub);
    let entries = reminders(input, now)
        .into_iter()
        .map(|reminder| {
            let owner = match &reminder.target {
                Target::Team { slug } => teams.iter().find(|team| team.slug == *slug),
                _ => reminder.project_root.as_deref().and_then(|root| {
                    let root = canonical(Path::new(root));
                    teams.iter().find(|team| team.roots.contains(&root))
                }),
            };
            Entry {
                team: owner.map(|team| team.slug.clone()),
                team_name: owner.map(|team| team.name.clone()),
                reminder,
            }
        })
        .collect();
    Digest {
        at: now,
        entries,
        notified: None,
        home: PathBuf::new(),
    }
}

/// Reads what the rules need and makes the digest: from the index when there is one, with GitHub's
/// runs read first when `refresh_github` and GitHub is connected.
fn compute(hub: &Hub, refresh_github: bool) -> Result<Digest, String> {
    let host = hub
        .finding
        .host()
        .ok_or_else(|| "this catervas has nothing to look through your history with".to_string())?;
    let now = host.now();
    let connection = github::load_connection(host.secrets().as_ref())
        .ok()
        .flatten();
    let (claude, home) = (host.claude_dir(), host.home());
    let sources = ReminderSources {
        claude_dir: &claude,
        home: &home,
        github_connected: connection.is_some(),
        utc_offset_secs: host.utc_offset_secs(now),
    };
    let path = host.state_dir().join(MACHINE_DB);
    let mut input = if path.exists() {
        let mut index = open_machine_index(&path, now).map_err(|e| e.to_string())?;
        if refresh_github && let Some((token, _)) = &connection {
            // GitHub saying no keeps what was read before.
            let _ = github::read_runs_into_index(
                &host.github().api,
                token,
                &mut index,
                &AtomicBool::new(false),
            );
        }
        index
            .reminder_input(&sources, now)
            .map_err(|e| e.to_string())?
    } else {
        ReminderInput {
            services: services_needing_sign_in(&claude),
            github_connected: sources.github_connected,
            utc_offset_secs: sources.utc_offset_secs,
            ..ReminderInput::default()
        }
    };
    input.teams = team_facts(hub);
    let mut digest = build(hub, &input, now);
    digest.home = canonical(&home);
    Ok(digest)
}

/// The kept digest, or one computed now.
pub(super) fn digest_of(hub: &Hub) -> Result<Digest, Failure> {
    if let Some(digest) = hub.reminders.kept() {
        return Ok(digest);
    }
    let digest = compute(hub, false).map_err(|why| internal(&why))?;
    hub.reminders.replace(digest.clone());
    Ok(digest)
}

/// `text` as one word for a POSIX shell: as it is when it holds only safe characters, else in
/// single quotes.
fn quoted(text: &str) -> String {
    let safe = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./@%+=:,-".contains(c));
    if safe {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// The command that resumes the session `id` in `folder`, the folder as `~/…` under `home`.
fn resume_command(home: &Path, folder: &str, id: &str) -> String {
    let folder = match Path::new(folder).strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", quoted(&rest.to_string_lossy())),
        Err(_) => quoted(folder),
    };
    format!("cd {folder} && claude --resume {}", quoted(id))
}

/// One item as the page reads it.
fn item_wire(entry: &Entry, home: &Path) -> Value {
    let reminder = &entry.reminder;
    let kind = match &reminder.target {
        Target::Session { .. } => "session",
        Target::Cron { .. } => "cron",
        Target::Github {
            page: GithubPage::Deployments,
            ..
        } => "deploy",
        Target::Github { .. } => "schedule",
        Target::ConnectGithub => "connect_github",
        Target::Team { .. } => "team",
        Target::Service => "service",
        Target::Run => "run",
    };
    let mut wire = json!({
        "key": reminder.key,
        "kind": kind,
        "title": reminder.title,
        "detail": reminder.detail,
    });
    if let Some(project) = &reminder.project {
        wire["project"] = json!(project);
    }
    if reminder.key.starts_with("ending:") {
        wire["explain"] = json!("cron");
    } else if reminder.key.starts_with("schedules:off:") {
        wire["explain"] = json!("off");
    }
    match &reminder.target {
        Target::Session {
            session_id, folder, ..
        } => {
            wire["resume"] = json!({ "command": resume_command(home, folder, session_id) });
        }
        Target::Cron {
            session_id,
            folder: Some(folder),
        } if !session_id.is_empty() => {
            wire["resume"] = json!({ "command": resume_command(home, folder, session_id) });
        }
        Target::Github { full_name, page } if !full_name.is_empty() => {
            wire["github_url"] = json!(format!("https://github.com/{full_name}/{}", page.path()));
        }
        Target::Team { slug } => wire["answer"] = json!({ "team": slug }),
        Target::ConnectGithub => wire["connect_github"] = json!(true),
        _ => {}
    }
    if let Some(text) = reminder.request_text() {
        wire["give"] = json!({ "text": text });
        if let (Some(team), Some(name)) = (&entry.team, &entry.team_name) {
            wire["give"]["team"] = json!(team);
            wire["give"]["team_name"] = json!(name);
        }
    }
    wire
}

fn moment(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// `reminders.get`: the digest with the owner's settings.
pub(super) fn get(hub: &Hub) -> Result<Value, Failure> {
    let host = host_of(hub)?;
    let digest = digest_of(hub)?;
    let settings = host.reminder_settings();
    let groups: Vec<Value> = GROUPS
        .into_iter()
        .filter_map(|group| {
            let items: Vec<Value> = digest
                .entries
                .iter()
                .filter(|entry| entry.reminder.group == group)
                .map(|entry| item_wire(entry, &digest.home))
                .collect();
            (!items.is_empty()).then(|| json!({ "group": group.as_wire(), "items": items }))
        })
        .collect();
    Ok(json!({
        "at": moment(digest.at),
        "count": digest.count(),
        "headline": digest.headline(),
        "time": settings.time,
        "browser": settings.browser,
        "system": settings.system,
        "notified": digest.notified,
        "groups": groups,
    }))
}

/// `reminders.mine`: the items of the digest that are this team's.
async fn mine(state: &Arc<DaemonState>) -> Result<Value, Failure> {
    let (Some(slug), Some(reminders)) = (state.team_slug().map(str::to_string), state.reminders())
    else {
        return Err(Failure::new(
            REFUSED,
            "catervas answers this on a team's page, once the hub has added the team",
        ));
    };
    let digest = tokio::task::spawn_blocking(move || reminders.current())
        .await
        .map_err(|e| internal(&e.to_string()))?
        .map_err(|why| internal(&why))?;
    let items: Vec<Value> = digest
        .counted()
        .filter(|entry| entry.team.as_deref() == Some(slug.as_str()))
        .map(|entry| item_wire(entry, &digest.home))
        .collect();
    Ok(json!({ "at": moment(digest.at), "count": items.len(), "items": items }))
}

/// `reminders.set`.
fn set(hub: &Hub, params: &Value) -> Result<Value, Failure> {
    let host = host_of(hub)?;
    let time = params["time"].as_str();
    if time.is_some_and(|time| parse_time(time).is_none()) {
        return Err(refusal("bad_time", "choose a time like 09:00"));
    }
    let change = ReminderChange {
        time: time.map(str::to_string),
        browser: params["browser"].as_bool(),
        system: params["system"].as_bool(),
    };
    let settings = host
        .set_reminder_settings(&change)
        .map_err(|why| sentence_failure(&why))?;
    // The timer looks at the time again.
    hub.reminders.changed.notify_one();
    Ok(json!({
        "time": settings.time,
        "browser": settings.browser,
        "system": settings.system,
    }))
}

/// `reminders.snooze` (`snooze` is `day` or `week`) and `reminders.dismiss` (`snooze` is none).
pub(super) fn mark(hub: &Hub, key: &str, snooze: Option<&str>) -> Result<Value, Failure> {
    use catervas_core::reminders::MarkKind;

    let host = host_of(hub)?;
    let digest = digest_of(hub)?;
    let Some(entry) = digest
        .entries
        .iter()
        .find(|entry| entry.reminder.key == key)
    else {
        return Err(refusal("unknown_item", "that one is gone"));
    };
    let now = host.now();
    let kind = match snooze {
        Some("week") => MarkKind::Snooze {
            until: now + Days::days(7),
        },
        Some(_) => MarkKind::Snooze {
            until: now + Days::days(1),
        },
        None => MarkKind::Dismiss {
            fingerprint: entry.reminder.fingerprint.clone(),
        },
    };
    let mut index = open_index(host.as_ref())?;
    index
        .put_reminder_mark(key, &kind, now)
        .map_err(|e| internal(&e.to_string()))?;
    hub.reminders.forget(key);
    Ok(json!({}))
}

/// `reminders.give`: files the item as a request in the team of its project.
pub(super) async fn give(hub: &Arc<Hub>, key: &str) -> Result<Value, Failure> {
    let for_digest = Arc::clone(hub);
    let digest = tokio::task::spawn_blocking(move || digest_of(&for_digest))
        .await
        .map_err(|e| internal(&e.to_string()))??;
    let Some(entry) = digest
        .entries
        .iter()
        .find(|entry| entry.reminder.key == key)
    else {
        return Err(refusal("unknown_item", "that one is gone"));
    };
    let Some(text) = entry.reminder.request_text() else {
        return Err(refusal(
            "cannot_give",
            "a team cannot finish this one for you",
        ));
    };
    let project = entry.reminder.project.as_deref().unwrap_or("this project");
    let no_team = || {
        refusal(
            "no_team_for_this",
            &format!("make a team for {project} first"),
        )
    };
    let slug = entry.team.clone().ok_or_else(no_team)?;
    let state = hub.team(&slug).ok_or_else(no_team)?;
    let filed = gates::call(&state, "request.file", &json!({ "text": text })).await?;
    Ok(json!({ "task_id": filed["task_id"], "team": slug }))
}

/// `call` of the daily reminder, whose params the schema already passed.
pub(super) async fn call(
    state: &Arc<DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    if method == "reminders.mine" {
        return mine(state).await;
    }
    let hub = Arc::clone(hub_of(state)?);
    let key = params["key"].as_str().unwrap_or_default().to_string();
    if method == "reminders.give" {
        return give(&hub, &key).await;
    }
    let (method, params) = (method.to_string(), params.clone());
    tokio::task::spawn_blocking(move || match method.as_str() {
        "reminders.get" => get(&hub),
        "reminders.set" => set(&hub, &params),
        "reminders.snooze" => mark(&hub, &key, Some(params["for"].as_str().unwrap_or("day"))),
        _ => mark(&hub, &key, None),
    })
    .await
    .unwrap_or_else(|error| Err(internal(&error.to_string())))
}

/// The title and the body of the notice of a digest of `count` things.
fn notice_of(time: &str, count: usize, headline: &str) -> (String, String) {
    let things = if count == 1 {
        "1 thing needs you".to_string()
    } else {
        format!("{count} things need you")
    };
    (
        format!("Catervas · {time}"),
        format!("{things}\n{headline}"),
    )
}

/// Computes the digest and tells the owner once: a page that is open, else the computer.
async fn fire(hub: &Arc<Hub>) {
    let Some(host) = hub.finding.host().cloned() else {
        return;
    };
    let for_digest = Arc::clone(hub);
    let Ok(Ok(mut digest)) = tokio::task::spawn_blocking(move || compute(&for_digest, true)).await
    else {
        return;
    };
    let settings = host.reminder_settings();
    let count = digest.count();
    if count > 0 {
        let headline = digest.headline();
        if hub.reminders.pages() > 0 {
            if settings.browser {
                let _ = hub.reminders.pushes.send(json!({
                    "jsonrpc": "2.0",
                    "method": "reminders.due",
                    "params": {
                        "at": moment(digest.at),
                        "count": count,
                        "headline": headline,
                        "time": settings.time,
                    },
                }));
                digest.notified = Some("browser");
            }
        } else if settings.system {
            let (title, body) = notice_of(&settings.time, count, &headline);
            let sent = tokio::task::spawn_blocking(move || host.notify(&title, &body)).await;
            digest.notified = Some(if matches!(sent, Ok(Ok(()))) {
                "system"
            } else {
                "none"
            });
        }
    }
    hub.reminders.replace(digest);
}

/// The daily reminder: at the owner's time each day, computes the digest and tells the owner once.
/// It also gives the pages of every team a way to ask for the digest.
pub async fn run_daily(hub: Arc<Hub>) {
    let weak = Arc::downgrade(&hub);
    let _ = hub.reminders.computer.set(Arc::new(move || {
        let hub = weak
            .upgrade()
            .ok_or_else(|| "this catervas has stopped".to_string())?;
        match hub.reminders.kept() {
            Some(digest) => Ok(digest),
            None => compute(&hub, false),
        }
    }));
    loop {
        let Some(host) = hub.finding.host().cloned() else {
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        };
        let now = host.now();
        let time = parse_time(&host.reminder_settings().time).unwrap_or(NaiveTime::MIN);
        let wait = until_next(now, host.utc_offset_secs(now), time);
        tokio::select! {
            () = tokio::time::sleep(wait) => fire(&hub).await,
            // The owner changed the time: look at it again.
            () = hub.reminders.changed.notified() => {}
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use catervas_core::reminders::{
        FORGOTTEN_FROM_DAYS, Group, ReminderInput, SessionFact, Target,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{Value, json};

    use super::{ReminderChange, ReminderSettings, build, get, give, mark, run_daily};
    use crate::connectors::{ConnectorSecrets, MemoryConnectorSecrets};
    use crate::daemon::finding::{FindingHost, FindingProgress};
    use crate::daemon::hub::{Hub, TeamSlug};
    use crate::github::GithubPlaces;
    use crate::notify::NotSent;
    use crate::orchestrator::fixtures::Harness;

    /// The time the fake host starts at: a minute before the reminder.
    pub(in crate::daemon) fn start() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 21, 8, 59, 0)
            .single()
            .expect("a time")
    }

    /// A host whose clock follows tokio's, and which records the notifications it was asked for.
    pub(in crate::daemon) struct Fake {
        dir: PathBuf,
        began: tokio::time::Instant,
        settings: Mutex<ReminderSettings>,
        sent: Mutex<Vec<(String, String)>>,
        secrets: Arc<MemoryConnectorSecrets>,
    }

    impl Fake {
        pub(in crate::daemon) fn new(name: &str) -> Arc<Self> {
            let dir = std::env::temp_dir()
                .join(format!("catervas-reminders-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("claude")).expect("a Claude folder");
            // One connector that needs signing in again: one item for the digest.
            std::fs::write(
                dir.join("claude/mcp-needs-auth-cache.json"),
                json!({"zoho-books": {"timestamp": 1}}).to_string(),
            )
            .expect("a cache file");
            Arc::new(Self {
                dir,
                began: tokio::time::Instant::now(),
                settings: Mutex::new(ReminderSettings::default()),
                sent: Mutex::new(Vec::new()),
                secrets: Arc::new(MemoryConnectorSecrets::default()),
            })
        }

        fn sent(&self) -> Vec<(String, String)> {
            self.sent.lock().expect("sent").clone()
        }
    }

    impl FindingHost for Fake {
        fn scan(
            &self,
            _stop: &AtomicBool,
            _report: &dyn Fn(FindingProgress),
        ) -> Result<(), String> {
            Ok(())
        }
        fn state_dir(&self) -> PathBuf {
            self.dir.join("state")
        }
        fn claude_dir(&self) -> PathBuf {
            self.dir.join("claude")
        }
        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }
        fn now(&self) -> DateTime<Utc> {
            start() + chrono::Duration::from_std(self.began.elapsed()).expect("a duration")
        }
        fn not_projects(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn paused(&self) -> bool {
            false
        }
        fn set_paused(&self, _paused: bool) -> Result<(), String> {
            Ok(())
        }
        fn export_dismissed(&self) -> bool {
            false
        }
        fn set_export_dismissed(&self, _dismissed: bool) -> Result<(), String> {
            Ok(())
        }
        fn secrets(&self) -> Arc<dyn ConnectorSecrets> {
            self.secrets.clone()
        }
        fn github(&self) -> GithubPlaces {
            GithubPlaces::default()
        }
        fn reminder_settings(&self) -> ReminderSettings {
            self.settings.lock().expect("settings").clone()
        }
        fn set_reminder_settings(
            &self,
            change: &ReminderChange,
        ) -> Result<ReminderSettings, String> {
            let mut settings = self.settings.lock().expect("settings");
            if let Some(time) = &change.time {
                settings.time.clone_from(time);
            }
            if let Some(browser) = change.browser {
                settings.browser = browser;
            }
            if let Some(system) = change.system {
                settings.system = system;
            }
            Ok(settings.clone())
        }
        fn utc_offset_secs(&self, _at: DateTime<Utc>) -> i32 {
            0
        }
        fn notify(&self, title: &str, body: &str) -> Result<(), NotSent> {
            self.sent
                .lock()
                .expect("sent")
                .push((title.to_string(), body.to_string()));
            Ok(())
        }
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

    /// Lets the spawned timer and the blocking threads it waits on run until `done`.
    async fn until(done: impl Fn() -> bool) {
        for _ in 0..2000 {
            if done() {
                return;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("it did not happen");
    }

    /// Lets everything that is ready run, so that what should not happen has had its chance.
    async fn settle() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn sends_one_notification_per_digest() {
        let fake = Fake::new("one-per-digest");
        let hub = Arc::new(Hub::default());
        hub.set_finder(fake.clone());
        let mut pushed = hub.reminders.subscribe();
        let page = hub.reminders.page_opened();
        let daily = tokio::spawn(run_daily(Arc::clone(&hub)));
        // The timer is waiting for 09:00 before the clock moves.
        settle().await;

        // At 09:00 with a page open, the page is told and the computer is not.
        tokio::time::advance(Duration::from_secs(61)).await;
        until(|| !pushed.is_empty()).await;
        let due = pushed.try_recv().expect("a push");
        conforms(&due, "remindersDueNotification");
        assert_eq!(due["method"], "reminders.due", "{due}");
        assert_eq!(due["params"]["count"], 1, "{due}");
        assert!(
            due["params"]["headline"]
                .as_str()
                .is_some_and(|headline| headline.contains("Zoho Books")),
            "{due}"
        );
        settle().await;
        assert!(
            fake.sent().is_empty(),
            "no system notification: a tab is open"
        );

        // The next day with no page open, one system notification, and nothing pushed.
        drop(page);
        tokio::time::advance(Duration::from_hours(24)).await;
        until(|| !fake.sent().is_empty()).await;
        settle().await;
        let sent = fake.sent();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].0, "Catervas · 09:00");
        assert!(sent[0].1.starts_with("1 thing needs you"), "{sent:?}");
        assert!(pushed.try_recv().is_err(), "nothing was pushed");

        // With the operating system switched off, none is sent.
        fake.set_reminder_settings(&ReminderChange {
            system: Some(false),
            ..ReminderChange::default()
        })
        .expect("settings");
        tokio::time::advance(Duration::from_hours(24)).await;
        settle().await;
        assert_eq!(fake.sent().len(), 1, "system: false sends none");

        // `reminders.get` answers the digest's groups and its headline.
        let answer = get(&hub).expect("a digest");
        conforms(&answer, "remindersGetResult");
        for frame in [
            json!({"jsonrpc": "2.0", "id": 1, "method": "reminders.get", "params": {}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "reminders.set", "params": {"time": "07:30", "browser": false}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "reminders.snooze", "params": {"key": "k", "for": "week"}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "reminders.dismiss", "params": {"key": "k"}}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "reminders.give", "params": {"key": "k"}}),
            json!({"jsonrpc": "2.0", "id": 6, "method": "reminders.mine", "params": {}}),
        ] {
            catervas_protocol::rpc::rpc_request_from_value(&frame)
                .unwrap_or_else(|errors| panic!("{frame} refused: {errors:?}"));
        }
        assert_eq!(answer["count"], 1, "{answer}");
        assert_eq!(answer["groups"][0]["group"], "services", "{answer}");
        assert_eq!(
            answer["groups"][0]["items"][0]["title"],
            "Zoho Books needs you to sign in again in Claude"
        );
        assert!(
            answer["headline"]
                .as_str()
                .is_some_and(|h| h.ends_with('.'))
        );
        daily.abort();
    }

    /// A session of `project` that sat for two days with changes not pushed.
    fn forgotten(project: &std::path::Path) -> ReminderInput {
        ReminderInput {
            sessions: vec![SessionFact {
                id: "s-1".to_string(),
                title: "Fix the login redirect".to_string(),
                project: "tidepool-web".to_string(),
                project_root: project.display().to_string(),
                folder: project.display().to_string(),
                branch: Some("fix/login-redirect".to_string()),
                last_at: start() - chrono::Duration::days(FORGOTTEN_FROM_DAYS + 1),
                open_todos: 0,
                uncommitted: false,
                unpushed: 2,
                draft_pull_quiet_days: None,
                waiting_job: false,
            }],
            github_connected: true,
            ..ReminderInput::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn gives_a_forgotten_session_to_its_team() {
        let fake = Fake::new("gives-a-session");
        let team = Harness::new("reminders-give", |_| {});
        let hub = Arc::new(Hub::default());
        hub.set_finder(fake.clone());
        hub.add(TeamSlug::new("tidepool"), Arc::clone(&team.daemon));
        let root = team.project.repo.path.clone();

        // The item is in a driven team's project: giving it files a request on that team's log.
        let digest = build(&hub, &forgotten(&root), fake.now());
        let key = digest.entries[0].reminder.key.clone();
        assert_eq!(digest.entries[0].team.as_deref(), Some("tidepool"));
        hub.reminders.replace(digest);
        let given = give(&hub, &key).await.expect("it is given");
        assert_eq!(given["team"], "tidepool", "{given}");
        let task = given["task_id"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .expect("the request's task");
        let contract = team
            .project
            .deps
            .files
            .read_contract(&task)
            .expect("the request's contract");
        let said = format!("{contract:?}");
        for part in [
            "Finish “Fix the login redirect” in tidepool-web.",
            "left 2 days ago with changes not pushed",
            "on the branch fix/login-redirect",
        ] {
            assert!(said.contains(part), "{part} in {said}");
        }

        // A project with no team has nowhere to give it.
        let elsewhere = std::env::temp_dir().join("catervas-reminders-no-team");
        let digest = build(&hub, &forgotten(&elsewhere), fake.now());
        assert_eq!(digest.entries[0].team, None);
        hub.reminders.replace(digest);
        let refused = give(&hub, &key).await.expect_err("no team");
        assert!(
            refused.message.starts_with("no_team_for_this: "),
            "{}",
            refused.message
        );

        // A snooze hides the item from the next get.
        assert_eq!(get(&hub).expect("a digest")["count"], 1);
        mark(&hub, &key, Some("day")).expect("snoozed");
        let after = get(&hub).expect("a digest");
        assert_eq!(after["count"], 0, "{after}");
        let _ = (Group::Forgotten, Target::Service, Value::Null);
    }
}
