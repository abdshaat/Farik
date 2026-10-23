//! Finding projects, behind the hub (`docs/SPEC.md` 4.1, 4.4; ADR 0059): the scan of the
//! computer's Claude history, the found list with the owner's answers, and the settings around
//! them. All of it is the computer's, kept in `machine.db`; nothing here is a team's event.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt as _;

use catervas_core::claude_history::{ProjectKey, fold_by_remote, normalise_remote};
use catervas_core::claude_services::catalog::CATALOG;
use catervas_core::finding::{Candidate, Choice, Section, section_of, suggest};
use catervas_core::linking::{ClaudeAiProjectText, Proof};
use catervas_store::claude_export::{ExportError, ExportLimits, import_claude_export};
use catervas_store::claude_history::{HistoryError, add_project_by_hand, place_sessions};
use catervas_store::git::Git;
use catervas_store::linking::{
    Answer, LinkProject, LinkingError, answer_pair, asked_pairs, confirmed_groups, evaluate,
    maybe_pairs,
};
use catervas_store::machine::{
    ClaudeWork, GithubProject, MACHINE_DB, MachineIndex, Ownership, WorkKind, forget_machine_index,
    open_machine_index,
};
use catervas_store::repos::{MemberPathRefusal, member_root};

use crate::claude::Secret;
use crate::connectors::ConnectorSecrets;
use crate::github::{self, GithubClient, GithubError, GithubPlaces};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::DaemonState;
use super::hub::Hub;
use super::reminders::{ReminderChange, ReminderSettings};
use super::web::{Failure, INTERNAL_ERROR, REFUSED, from_own_page, has_session};
use crate::notify::NotSent;

/// How long the daily rescan waits between two scans.
const RESCAN_EVERY: Duration = Duration::from_hours(24);

/// How often a scan's counts are published to the page at most.
const PUBLISH_EVERY: Duration = Duration::from_millis(250);

/// How many files a scan reads before it says how long it has left.
const FILES_BEFORE_ESTIMATE: u64 = 20;

/// How long forgetting waits for a scan it stopped to end.
const STOP_WAIT: Duration = Duration::from_secs(10);

/// What `cleanupPeriodDays` is when Claude Code's settings do not say.
const DEFAULT_RETENTION_DAYS: u64 = 30;

/// How long Claude Code's history can be kept: 90 days, a year or ten years.
const RETENTION_OFFERED: [u64; 3] = [90, 365, 3650];

/// How far a scan has got, told to the hub as it goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindingProgress {
    /// Transcript files to read.
    pub files_total: u64,
    /// Transcript files passed so far.
    pub files_read: u64,
    /// Sessions in the index so far.
    pub sessions: u64,
    /// Projects placed so far.
    pub projects: u64,
    /// Jobs found so far.
    pub jobs: u64,
    /// What is being read, such as "sessions from August".
    pub reading: Option<String>,
    /// Transcripts with lines this version of Claude Code's format could not be read from.
    pub unreadable: u64,
}

/// What the CLI does for finding: the scan, and what only it knows about this computer.
pub trait FindingHost: Send + Sync {
    /// Runs one scan, telling `report` how far it has got and ending between two files once `stop`
    /// is set. What it read stays in the index.
    ///
    /// # Errors
    ///
    /// A sentence saying what the index, git or a file refused.
    fn scan(&self, stop: &AtomicBool, report: &dyn Fn(FindingProgress)) -> Result<(), String>;
    /// Catervas's state folder, which holds `machine.db`.
    fn state_dir(&self) -> PathBuf;
    /// Claude Code's folder.
    fn claude_dir(&self) -> PathBuf;
    /// The user's home folder.
    fn home(&self) -> PathBuf;
    /// The time now.
    fn now(&self) -> DateTime<Utc>;
    /// The folders that are never a project: home and the temporary folders.
    fn not_projects(&self) -> Vec<PathBuf>;
    /// Whether the owner paused finding.
    fn paused(&self) -> bool;
    /// Keeps whether finding is paused.
    ///
    /// # Errors
    ///
    /// A sentence saying what could not be written.
    fn set_paused(&self, paused: bool) -> Result<(), String>;
    /// Whether the owner dismissed the card that offers claude.ai's export.
    fn export_dismissed(&self) -> bool;
    /// Keeps whether the export card is dismissed.
    ///
    /// # Errors
    ///
    /// A sentence saying what could not be written.
    fn set_export_dismissed(&self, dismissed: bool) -> Result<(), String>;
    /// Where this computer's own keys are kept: the GitHub token among them.
    fn secrets(&self) -> Arc<dyn ConnectorSecrets>;
    /// Where GitHub is and how a repository is cloned from it.
    fn github(&self) -> GithubPlaces;
    /// Makes the project of one team of the proposal, paused with its setup marker removed and its
    /// template applied, and writes what the plan found to `.catervas/local/found-services.json`. It
    /// does not drive the team.
    ///
    /// # Errors
    ///
    /// A sentence beginning with a code, saying what could not be made; nothing is left behind.
    fn create_team(&self, plan: &TeamPlan) -> Result<MadeProject, String> {
        let _ = plan;
        Err("not_available: this catervas cannot make teams".to_string())
    }
    /// Deletes what `created` lists once nobody drives `root` any more, but only a folder whose git
    /// log holds Catervas's own commits alone. Answers the sentence to tell the owner when it left
    /// something.
    ///
    /// # Errors
    ///
    /// A sentence saying what could not be deleted.
    fn remove_made(&self, root: &Path, created: &[PathBuf]) -> Result<Option<String>, String> {
        let _ = (root, created);
        Err("not_available: this catervas cannot remove teams".to_string())
    }
    /// Runs the owner's Claude Code once with `prompt`, Haiku and no tools.
    ///
    /// # Errors
    ///
    /// Claude Code's words when it did not answer.
    fn ask_names(&self, prompt: &str) -> Result<NamesRun, String> {
        let _ = prompt;
        Err("Claude Code cannot be asked here".to_string())
    }
    /// When the daily reminder comes and how it is told.
    fn reminder_settings(&self) -> ReminderSettings {
        ReminderSettings::default()
    }
    /// Keeps a change to the daily reminder's settings and answers them as they are now.
    ///
    /// # Errors
    ///
    /// A sentence saying what could not be written.
    fn set_reminder_settings(&self, change: &ReminderChange) -> Result<ReminderSettings, String> {
        let _ = change;
        Err("not_available: this catervas cannot keep the reminder's settings".to_string())
    }
    /// The computer's offset from UTC at `at`, in seconds.
    fn utc_offset_secs(&self, at: DateTime<Utc>) -> i32 {
        use chrono::{Local, Offset as _, TimeZone as _};
        Local
            .offset_from_utc_datetime(&at.naive_utc())
            .fix()
            .local_minus_utc()
    }
    /// Shows a notification through the operating system.
    ///
    /// # Errors
    ///
    /// [`NotSent`] when the program is missing or fails.
    fn notify(&self, title: &str, body: &str) -> Result<(), NotSent> {
        let _ = (title, body);
        Err(NotSent("this catervas has no way to notify".to_string()))
    }
    /// The committed files of the repository at `root` that proofs of linking are read from, as
    /// `(path, text)`; none when they cannot be read.
    fn committed_files(&self, root: &str) -> Vec<(String, String)> {
        catervas_store::linking::read_from_git(root)
    }
}

/// How one team of the proposal is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanKind {
    /// A repository of the owner's, made a team where it is.
    Folder {
        /// The repository's folder.
        folder: PathBuf,
    },
    /// Several repositories, bundled as a project folder made at `folder`.
    Project {
        /// The project folder to make.
        folder: PathBuf,
        /// The repositories, each with its name as a member and its folder.
        members: Vec<(String, PathBuf)>,
    },
    /// Work done through connectors: a new folder at `folder`, a git repository with a README.
    Connector {
        /// The folder to make.
        folder: PathBuf,
    },
}

/// One team to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamPlan {
    /// The team's name.
    pub name: String,
    /// The ready-made or saved team it starts from.
    pub template: String,
    /// How its project is made.
    pub kind: PlanKind,
    /// The services its projects use through Claude, by the catalog's id and name.
    pub services: Vec<(String, String)>,
}

/// A team that was made, paused, ready to be driven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadeProject {
    /// The folder of its project.
    pub root: PathBuf,
    /// Its team id, from which its name in an address comes.
    pub team_id: String,
    /// What was created: the `.catervas` folder in a repository of the owner's, or the whole project
    /// folder or new folder.
    pub created: Vec<PathBuf>,
}

/// What Claude Code answered the names run.
#[derive(Debug, Clone, PartialEq)]
pub struct NamesRun {
    /// What it said.
    pub result: String,
    /// The `total_cost_usd` it reported.
    pub cost_usd: f64,
}

/// Where a scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    Running,
    Stopped,
    Done,
}

impl Phase {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Done => "done",
        }
    }
}

/// The scan the hub holds: its phase, how far it has got, and why it ended when it failed.
#[derive(Default)]
struct Run {
    phase: Phase,
    progress: FindingProgress,
    error: Option<String>,
    started_at: Option<DateTime<Utc>>,
    started: Option<Instant>,
    ended_at: Option<DateTime<Utc>>,
    bumped: Option<Instant>,
}

/// What the hub keeps of finding: its host, and the one scan that runs at a time.
#[derive(Default)]
pub struct Finding {
    host: OnceLock<Arc<dyn FindingHost>>,
    run: Mutex<Run>,
    stop: AtomicBool,
    /// Counts up each time the counts are published, so that a page knows to ask again.
    version: AtomicU64,
    /// The cards `proposal.get` showed last, by id, with the keys of their projects: what
    /// `proposal.create` makes, and checks the proofs of again.
    shown: Mutex<BTreeMap<String, Vec<String>>>,
}

impl Finding {
    /// Gives finding its host.
    pub fn set_host(&self, host: Arc<dyn FindingHost>) {
        let _ = self.host.set(host);
    }

    /// Asks a running scan to end between two files.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// The host, once it is given.
    pub(super) fn host(&self) -> Option<&Arc<dyn FindingHost>> {
        self.host.get()
    }

    fn run(&self) -> MutexGuard<'_, Run> {
        self.run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn bump(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Keeps `progress`, and tells the page at most every 250 ms.
    fn publish(&self, progress: FindingProgress) {
        let due = {
            let mut run = self.run();
            run.progress = progress;
            let due = run.bumped.is_none_or(|at| at.elapsed() >= PUBLISH_EVERY);
            if due {
                run.bumped = Some(Instant::now());
            }
            due
        };
        if due {
            self.bump();
        }
    }

    /// Ends the scan with `result`.
    fn finish(&self, result: Result<(), String>, now: DateTime<Utc>) {
        {
            let mut run = self.run();
            let stopped = self.stop.load(Ordering::SeqCst);
            run.phase = match &result {
                Ok(()) if !stopped => Phase::Done,
                _ => Phase::Stopped,
            };
            run.error = result.err();
            run.ended_at = Some(now);
        }
        self.bump();
    }

    /// Whether a daily rescan should start now: finding is on, and a first scan made the index.
    fn rescan_wanted(&self) -> bool {
        self.host
            .get()
            .is_some_and(|host| !host.paused() && host.state_dir().join(MACHINE_DB).exists())
    }
}

/// The methods the hub answers for finding (docs/schemas/rpc.schema.json).
pub(super) const METHODS: [&str; 35] = [
    "finding.offer",
    "finding.catalog",
    "finding.start",
    "finding.stop",
    "finding.status",
    "finding.list",
    "finding.ignored",
    "finding.choose",
    "finding.separate",
    "finding.locate",
    "finding.declare_services",
    "finding.add_project",
    "finding.add_routine",
    "finding.retention",
    "finding.keep_longer",
    "finding.pause",
    "finding.export_dismissed",
    "finding.forget",
    "github.connect",
    "github.status",
    "github.disconnect",
    "github.copy",
    "linking.pairs",
    "linking.maybe",
    "linking.groups",
    "linking.answer",
    "linking.reject",
    "proposal.get",
    "proposal.edit",
    "proposal.move_out",
    "proposal.revert",
    "proposal.names_estimate",
    "proposal.names",
    "proposal.create",
    "teams.undo",
];

mod proposal;
pub(super) use proposal::{has_started, made_slugs};

/// A refusal: its code, then its sentence.
fn refusal(code: &str, sentence: &str) -> Failure {
    Failure::new(REFUSED, format!("{code}: {sentence}"))
}

fn internal(error: &dyn std::fmt::Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

fn unknown_project() -> Failure {
    refusal("unknown_project", "Catervas doesn't know that project")
}

fn moment(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// The host, or the refusal of a Catervas that cannot look through the history.
pub(super) fn host_of(hub: &Hub) -> Result<&Arc<dyn FindingHost>, Failure> {
    hub.finding
        .host
        .get()
        .ok_or_else(|| internal(&"this catervas has nothing to look through your history with"))
}

/// The machine index, made if it was not there.
pub(super) fn open_index(host: &dyn FindingHost) -> Result<MachineIndex, Failure> {
    open_machine_index(&host.state_dir().join(MACHINE_DB), host.now()).map_err(|e| internal(&e))
}

/// `method` of finding, whose params the schema already passed. It reads and writes files, so it
/// runs off the async workers.
pub(super) fn call(hub: &Hub, method: &str, params: &Value) -> Result<Value, Failure> {
    let host = host_of(hub)?;
    match method {
        "finding.offer" => Ok(offer(host.as_ref())),
        "finding.catalog" => Ok(catalog()),
        "finding.start" => start(hub),
        "finding.stop" => {
            hub.finding.stop();
            Ok(json!({}))
        }
        "finding.status" => Ok(status(hub)),
        "finding.list" => {
            let listing = listing_of(hub, host.as_ref())?;
            Ok(listing.to_json(false))
        }
        "finding.ignored" => {
            let listing = listing_of(hub, host.as_ref())?;
            Ok(json!({ "ignored": listing.to_json(true)["ignored"] }))
        }
        "finding.choose" => choose(hub, host.as_ref(), params),
        "finding.separate" => separate(hub, host.as_ref(), params),
        "finding.locate" => locate(hub, host.as_ref(), params),
        "finding.declare_services" => declare_services(host.as_ref(), params),
        "finding.add_project" => add_project(host.as_ref(), params),
        "finding.add_routine" => add_routine(hub, host.as_ref(), params),
        "finding.retention" => Ok(retention(host.as_ref())),
        "finding.keep_longer" => keep_longer(host.as_ref(), params),
        "finding.pause" => pause(hub, host.as_ref(), params),
        "finding.export_dismissed" => {
            host.set_export_dismissed(true)
                .map_err(|why| internal(&why))?;
            Ok(json!({}))
        }
        "github.connect" => github_connect(hub, host.as_ref(), params),
        "github.status" => Ok(github_status(host.as_ref())),
        "github.disconnect" => github_disconnect(host.as_ref()),
        "github.copy" => github_copy(hub, host.as_ref(), params),
        "linking.pairs" => linking_pairs(hub, host.as_ref()),
        "linking.maybe" => linking_maybe(hub, host.as_ref()),
        "linking.groups" => linking_groups(hub, host.as_ref()),
        "linking.answer" => linking_answer(hub, host.as_ref(), params),
        method if proposal::METHODS.contains(&method) => {
            proposal::call(hub, host.as_ref(), method, params)
        }
        _ => forget(hub, host.as_ref(), params),
    }
}

/// What a failure to speak to GitHub is told as.
fn github_refusal(error: &GithubError) -> Failure {
    match error {
        GithubError::Refused => refusal(
            "github_token_refused",
            "GitHub did not accept that token: check that it is a fine-grained token that has not expired",
        ),
        GithubError::RateLimited { .. } => refusal("github_rate_limited", &error.to_string()),
        GithubError::Failed(why) => refusal("github_unreachable", why),
    }
}

/// `github.connect`: checks the token with GitHub, keeps it, and reads the repositories. The
/// refusal never holds the token.
fn github_connect(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let token = Secret::new(
        params["token"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string(),
    );
    if token.expose().is_empty() {
        return Err(refusal("github_token_refused", "paste the token first"));
    }
    let places = host.github();
    let client = GithubClient::new_at(&places.api).map_err(|e| github_refusal(&e))?;
    let login = github::block_on(client.login(&token))
        .map_err(|e| internal(&e))?
        .map_err(|e| github_refusal(&e))?;
    github::save_connection(host.secrets().as_ref(), &token, &login)
        .map_err(|why| refusal("no_secret_store", &why))?;
    // The first read. GitHub saying no here (a limit) leaves the connection kept and the list
    // empty until the next scan.
    let mut index = open_index(host)?;
    let _ = github::read_into_index(
        &places.api,
        &token,
        &mut index,
        host.now(),
        &AtomicBool::new(false),
    );
    hub.finding.bump();
    Ok(json!({ "login": login }))
}

/// `github.status`: whether a token is kept, and whose it is.
fn github_status(host: &dyn FindingHost) -> Value {
    match github::load_connection(host.secrets().as_ref()) {
        Ok(Some((_, login))) if !login.is_empty() => json!({ "connected": true, "login": login }),
        Ok(Some(_)) => json!({ "connected": true }),
        _ => json!({ "connected": false }),
    }
}

/// `github.disconnect`: forgets the token.
fn github_disconnect(host: &dyn FindingHost) -> Result<Value, Failure> {
    github::delete_token(host.secrets().as_ref()).map_err(|why| internal(&why))?;
    Ok(json!({}))
}

/// The folder `parent`, relative to home, which need not exist yet: its nearest folder that does,
/// followed through its links, must be inside home. A `..`, a root or a prefix is refused.
fn parent_in_home(home: &Path, parent: &str) -> Result<PathBuf, Failure> {
    let outside = || refusal("outside_home", "choose a folder inside your home folder");
    let home = std::fs::canonicalize(home).map_err(|e| internal(&e))?;
    let mut parts = Vec::new();
    for component in Path::new(parent).components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part),
            std::path::Component::CurDir => {}
            _ => return Err(outside()),
        }
    }
    let mut folder = home.clone();
    for (made, part) in parts.iter().enumerate() {
        let next = folder.join(part);
        if std::fs::symlink_metadata(&next).is_err() {
            return Ok(parts[made..]
                .iter()
                .fold(folder, |path, part| path.join(part)));
        }
        folder = std::fs::canonicalize(&next).map_err(|_| outside())?;
        if !folder.starts_with(&home) {
            return Err(outside());
        }
    }
    Ok(folder)
}

/// The folder name a repository is copied to when the owner gives none: its name, lowercase.
fn folder_name_of(full_name: &str) -> String {
    let name: String = full_name
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    name.trim_matches('-').to_string()
}

/// `github.copy`: clones a repository of the owner's into a new folder, and makes the folder a
/// project like any other. The token reaches git through its environment alone.
fn github_copy(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let full_name = params["full_name"]
        .as_str()
        .and_then(github::valid_full_name)
        .ok_or_else(|| refusal("unknown_repository", "that is not a GitHub repository"))?;
    let Some((token, _)) =
        github::load_connection(host.secrets().as_ref()).map_err(|why| internal(&why))?
    else {
        return Err(refusal("github_not_connected", "connect GitHub first"));
    };
    let name = params["name"]
        .as_str()
        .map_or_else(|| folder_name_of(full_name), str::to_string);
    let named = (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !named {
        return Err(refusal(
            "bad_name",
            "name the folder with lowercase letters, digits and hyphens",
        ));
    }
    let parent = parent_in_home(&host.home(), params["parent"].as_str().unwrap_or_default())?;
    let folder = parent.join(&name);
    if std::fs::symlink_metadata(&folder).is_ok() {
        return Err(refusal(
            "folder_taken",
            "a folder with that name is already there",
        ));
    }
    let places = host.github();
    let call = github::clone_call(&places.clone_base, full_name, &folder, &token);
    (places.spawn)(&call)
        .map_err(|why| refusal("copy_failed", &why.replace(token.expose(), "***")))?;
    // The folder is a project now: added by hand with the repository's address, so that the row for
    // the repository and the row for the folder are one. A scan then reads its jobs.
    let folder = folder.canonicalize().unwrap_or(folder);
    let mut index = open_index(host)?;
    let _ = add_project_by_hand(
        &mut index,
        &name,
        Some(&folder),
        Some(&format!("https://github.com/{full_name}.git")),
        &host.not_projects(),
        host.now(),
    );
    drop(index);
    if !host.paused() {
        let _ = start(hub);
    }
    hub.finding.bump();
    Ok(json!({ "folder": folder.to_string_lossy() }))
}

/// Whether Claude Code's folder holds a transcript, counted without reading any.
fn holds_transcript(projects: &Path) -> bool {
    let is_transcript = |path: &Path| path.extension().is_some_and(|ext| ext == "jsonl");
    let Ok(entries) = std::fs::read_dir(projects) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            std::fs::read_dir(&path)
                .is_ok_and(|files| files.flatten().any(|file| is_transcript(&file.path())))
        } else {
            is_transcript(&path)
        }
    })
}

/// `finding.offer`: whether there is Claude Code history to look through.
fn offer(host: &dyn FindingHost) -> Value {
    let projects = host.claude_dir().join("projects");
    json!({
        "history": if holds_transcript(&projects) { "claude_code" } else { "none" },
        "sessions_dir_exists": projects.is_dir(),
    })
}

/// `finding.catalog`: the services the owner can say they use with Claude, in the catalog's order.
fn catalog() -> Value {
    json!({
        "services": CATALOG
            .iter()
            .map(|entry| json!({ "id": entry.id, "name": entry.name, "kind": entry.kind.as_wire() }))
            .collect::<Vec<_>>(),
    })
}

/// `finding.start`: begins a scan on a thread of its own, unless one runs.
fn start(hub: &Hub) -> Result<Value, Failure> {
    let finding = &hub.finding;
    let host = host_of(hub)?;
    if host.paused() {
        return Err(refusal(
            "finding_paused",
            "finding is paused: turn it on in This computer",
        ));
    }
    {
        let mut run = finding.run();
        if run.phase == Phase::Running {
            return Ok(json!({ "started": false }));
        }
        *run = Run {
            phase: Phase::Running,
            started_at: Some(host.now()),
            started: Some(Instant::now()),
            ..Run::default()
        };
        finding.stop.store(false, Ordering::SeqCst);
    }
    finding.bump();
    let (finding, host) = (Arc::clone(&hub.finding), Arc::clone(host));
    let spawned = std::thread::Builder::new()
        .name("finding".to_string())
        .spawn({
            let (finding, host) = (Arc::clone(&finding), Arc::clone(&host));
            move || {
                let result = host.scan(&finding.stop, &|progress| finding.publish(progress));
                finding.finish(result, host.now());
            }
        });
    if let Err(error) = spawned {
        finding.finish(Err(error.to_string()), host.now());
        return Err(internal(&error));
    }
    Ok(json!({ "started": true }))
}

/// `finding.status`: where the scan is, and the settings around it.
fn status(hub: &Hub) -> Value {
    let finding = &hub.finding;
    let (paused, index_exists, dismissed) =
        finding.host.get().map_or((false, false, false), |host| {
            (
                host.paused(),
                host.state_dir().join(MACHINE_DB).exists(),
                host.export_dismissed(),
            )
        });
    let run = finding.run();
    let progress = &run.progress;
    // The time left is the files still to read at the mean time per file so far, once there is a
    // mean to speak of.
    let seconds_left = run
        .started
        .filter(|_| run.phase == Phase::Running && progress.files_read >= FILES_BEFORE_ESTIMATE)
        .map(|started| {
            let left = u128::from(progress.files_total.saturating_sub(progress.files_read))
                * started.elapsed().as_millis()
                / u128::from(progress.files_read);
            u64::try_from(left.div_ceil(1000)).unwrap_or(u64::MAX)
        });
    json!({
        "phase": run.phase.as_wire(),
        "files_total": progress.files_total,
        "files_read": progress.files_read,
        "sessions": progress.sessions,
        "projects": progress.projects,
        "jobs": progress.jobs,
        "reading": progress.reading,
        "unreadable": progress.unreadable,
        "error": run.error,
        "started_at": run.started_at.map(moment),
        "ended_at": run.ended_at.map(moment),
        "seconds_left": seconds_left,
        "paused": paused,
        "index_exists": index_exists,
        "export_dismissed": dismissed,
        "version": finding.version.load(Ordering::SeqCst),
    })
}

/// One row of the found list.
struct Row {
    key: String,
    name: String,
    kind: &'static str,
    folders: Vec<String>,
    last_at: Option<DateTime<Utc>>,
    jobs: Vec<(String, String)>,
    services: Vec<String>,
    suggested: Option<Choice>,
    choice: Option<Choice>,
    already_team: Option<String>,
    gone: bool,
    section: Section,
    claude_work: Vec<ClaudeWork>,
    github: Option<GithubProject>,
    /// The repository's normalised remote, when the project has one.
    remote: Option<String>,
}

/// A piece of Claude's work on GitHub, as the page reads it.
fn work_json(work: &ClaudeWork) -> Value {
    let mut shown = json!({
        "kind": if work.kind == WorkKind::Pull { "pull" } else { "branch" },
        "name": work.name,
    });
    if let Some(number) = work.number {
        shown["number"] = json!(number);
    }
    if let Some(draft) = work.draft {
        shown["draft"] = json!(draft);
    }
    shown
}

impl Row {
    fn to_json(&self) -> Value {
        json!({
            "key": self.key,
            "name": self.name,
            "kind": self.kind,
            "folders": self.folders,
            "last_at": self.last_at.map(moment),
            "jobs": self.jobs.iter().map(|(name, schedule)| json!({ "name": name, "schedule": schedule })).collect::<Vec<_>>(),
            "services": self.services,
            "suggested": self.suggested.map(Choice::as_wire),
            "choice": self.choice.map(Choice::as_wire),
            "already_team": self.already_team,
            "gone": self.gone,
            "claude_work": self.claude_work.iter().map(work_json).collect::<Vec<_>>(),
            "github": self.github.as_ref().map(|repo| json!({
                "full_name": repo.full_name,
                "private": repo.private,
                "default_branch": repo.default_branch,
                "pushed_at": moment(repo.pushed_at),
                "not_seen_lately": repo.not_seen_lately,
            })),
        })
    }

    /// The section the row is shown in, or none while the owner ignores it.
    fn shown_in(&self) -> Option<Section> {
        match (self.choice, self.section) {
            (Some(Choice::Ignore), _) => None,
            (Some(_), Section::Explored) => Some(Section::Main),
            (_, section) => Some(section),
        }
    }
}

/// The found list: its rows, and where the home folder's sessions went.
struct Listing {
    rows: Vec<Row>,
    home_sessions: u64,
    formed_into: Vec<String>,
}

impl Listing {
    fn find(&self, key: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn section(&self, section: Section) -> Vec<Value> {
        self.rows
            .iter()
            .filter(|row| row.shown_in() == Some(section))
            .map(Row::to_json)
            .collect()
    }

    /// The answer of `finding.list`; `finding.ignored` takes its `ignored` alone.
    fn to_json(&self, _ignored_only: bool) -> Value {
        json!({
            "main": self.section(Section::Main),
            "explored": self.section(Section::Explored),
            "gone": self.section(Section::Gone),
            "home": { "sessions": self.home_sessions, "formed_into": self.formed_into },
            "computer_jobs": self.section(Section::ComputerJobs),
            "ignored": self
                .rows
                .iter()
                .filter(|row| row.choice == Some(Choice::Ignore))
                .map(Row::to_json)
                .collect::<Vec<_>>(),
        })
    }
}

fn name_of(path: &str) -> String {
    Path::new(path).file_name().map_or_else(
        || path.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The list the index and the hub's teams make, with the owner's choices.
fn listing_of(hub: &Hub, host: &dyn FindingHost) -> Result<Listing, Failure> {
    if !host.state_dir().join(MACHINE_DB).exists() {
        return Ok(Listing {
            rows: Vec::new(),
            home_sessions: 0,
            formed_into: Vec::new(),
        });
    }
    let index = open_index(host)?;
    build_listing(hub, host, &index).map_err(|e| internal(&e))
}

#[allow(
    clippy::too_many_lines,
    reason = "one pass that turns each kind of found thing into a row, in the order they are shown"
)]
fn build_listing(
    hub: &Hub,
    host: &dyn FindingHost,
    index: &MachineIndex,
) -> Result<Listing, catervas_store::StoreError> {
    let now = host.now();
    let found = index.found()?;
    let services = index.found_services()?;
    let jobs = index.found_jobs(now)?;
    let choices = index.choices()?;
    let separated = index.separations()?;
    let routines = index.hand_routines()?;
    let teams: Vec<(String, PathBuf)> = hub
        .team_roots()
        .into_iter()
        .map(|(slug, root)| (slug, root.canonicalize().unwrap_or(root)))
        .collect();
    let team_of = |folders: &[String]| {
        teams
            .iter()
            .find(|(_, root)| {
                folders.iter().any(|folder| {
                    let folder = Path::new(folder);
                    folder
                        .canonicalize()
                        .unwrap_or_else(|_| folder.to_path_buf())
                        == *root
                })
            })
            .map(|(slug, _)| slug.clone())
    };
    let jobs_of = |folders: &[String], key: &str| -> Vec<(String, String)> {
        let mut listed: Vec<(String, String)> = jobs
            .iter()
            .filter(|job| {
                job.project_root
                    .as_ref()
                    .is_some_and(|root| folders.contains(root))
            })
            .map(|job| (job.display_name.clone(), job.schedules.join("; ")))
            .collect();
        listed.extend(
            routines
                .iter()
                .filter(|routine| routine.project_key.as_deref() == Some(key))
                .map(|routine| (routine.name.clone(), routine.schedule.clone())),
        );
        listed
    };
    let mut rows: Vec<Row> = Vec::new();
    let make = |key: String,
                name: String,
                kind: &'static str,
                folders: Vec<String>,
                last_at: Option<DateTime<Utc>>,
                candidate: Candidate,
                services: Vec<String>,
                gone: bool|
     -> Row {
        Row {
            jobs: jobs_of(&folders, &key),
            suggested: suggest(candidate, now),
            choice: choices.get(&key).copied(),
            already_team: team_of(&folders),
            section: section_of(candidate),
            key,
            name,
            kind,
            folders,
            last_at,
            services,
            gone,
            claude_work: Vec::new(),
            github: None,
            remote: None,
        }
    };

    // The folders on this computer, the ones that share a repository folded into one project.
    let keys: Vec<ProjectKey<'_>> = found
        .projects
        .iter()
        .map(|project| ProjectKey {
            root: &project.root,
            remote: project.remote.as_deref(),
        })
        .collect();
    let groups = fold_by_remote(&keys, &separated);
    let mut sharing: BTreeMap<&str, usize> = BTreeMap::new();
    for group in &groups {
        if let Some(remote) = found.projects[group[0]].remote.as_deref() {
            *sharing.entry(remote).or_default() += 1;
        }
    }
    for group in &groups {
        let members: Vec<_> = group.iter().map(|&at| &found.projects[at]).collect();
        let first = members[0];
        let key = match first.remote.as_deref() {
            Some(remote) if sharing.get(remote) == Some(&1) => remote.to_string(),
            _ => first.root.clone(),
        };
        let last_at = members.iter().map(|project| project.last_at).max();
        let yours = members
            .iter()
            .any(|project| project.ownership == Ownership::Yours);
        let folders: Vec<String> = members.iter().map(|project| project.root.clone()).collect();
        let mut used: Vec<String> = Vec::new();
        for project in &members {
            for service in services
                .by_root
                .get(&project.root)
                .map_or(&[][..], Vec::as_slice)
            {
                if !used.contains(&service.name) {
                    used.push(service.name.clone());
                }
            }
        }
        let name = members
            .iter()
            .find_map(|project| project.name_by_hand.clone())
            .unwrap_or_else(|| name_of(&first.root));
        let at = last_at.unwrap_or(first.last_at);
        let mut row = make(
            key,
            name,
            "folder",
            folders,
            last_at,
            if yours {
                Candidate::Yours { last_at: at }
            } else {
                Candidate::Explored
            },
            used,
            false,
        );
        row.claude_work = members
            .iter()
            .find(|project| !project.claude_work.is_empty())
            .map(|project| project.claude_work.clone())
            .unwrap_or_default();
        row.remote.clone_from(&first.remote);
        rows.push(row);
    }
    for repo in &found.github {
        let mut row = make(
            repo.remote_key.clone(),
            repo.full_name.clone(),
            "github",
            Vec::new(),
            Some(repo.pushed_at),
            Candidate::Github,
            Vec::new(),
            false,
        );
        row.claude_work.clone_from(&repo.claude_work);
        row.github = Some(repo.clone());
        row.remote = Some(repo.remote_key.clone());
        rows.push(row);
    }
    for hand in &found.by_hand {
        let key = hand
            .root
            .clone()
            .or_else(|| hand.remote.clone())
            .unwrap_or_else(|| hand.name.clone());
        let mut row = make(
            key,
            hand.name.clone(),
            "hand",
            hand.root.iter().cloned().collect(),
            None,
            Candidate::ByHand,
            Vec::new(),
            false,
        );
        row.remote.clone_from(&hand.remote);
        rows.push(row);
    }
    for formed in &services.formed {
        rows.push(make(
            formed.name.clone(),
            formed.name.clone(),
            "connector",
            Vec::new(),
            Some(formed.last_at),
            Candidate::Connector {
                last_at: formed.last_at,
            },
            formed
                .services
                .iter()
                .map(|service| service.name.clone())
                .collect(),
            false,
        ));
    }
    for project in &found.claude_ai.projects {
        rows.push(make(
            project.uuid.clone(),
            project
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Untitled project".to_string()),
            "claude_ai",
            Vec::new(),
            project.updated_at,
            Candidate::ClaudeAi,
            Vec::new(),
            false,
        ));
    }
    for gone in &found.gone {
        rows.push(make(
            gone.folder.clone(),
            name_of(&gone.folder),
            "folder",
            vec![gone.folder.clone()],
            Some(gone.last_at),
            Candidate::Gone,
            Vec::new(),
            true,
        ));
    }
    for job in jobs.iter().filter(|job| job.project_root.is_none()) {
        let key = format!("job:{}:{}:{}", job.origin, job.file, job.name);
        rows.push(make(
            key.clone(),
            job.display_name.clone(),
            "job",
            Vec::new(),
            Some(job.last_seen),
            Candidate::ComputerJob,
            Vec::new(),
            false,
        ));
        if let Some(row) = rows.last_mut() {
            row.jobs = vec![(job.display_name.clone(), job.schedules.join("; "))];
        }
    }
    for routine in routines
        .iter()
        .filter(|routine| routine.project_key.is_none())
    {
        let key = format!("routine:{}", routine.id);
        rows.push(make(
            key,
            routine.name.clone(),
            "job",
            Vec::new(),
            None,
            Candidate::ComputerJob,
            Vec::new(),
            false,
        ));
        if let Some(row) = rows.last_mut() {
            row.jobs = vec![(routine.name.clone(), routine.schedule.clone())];
        }
    }
    rows.sort_by(|a, b| {
        b.last_at
            .cmp(&a.last_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(Listing {
        rows,
        home_sessions: found.not_a_project.sessions,
        formed_into: services
            .formed
            .iter()
            .map(|formed| formed.name.clone())
            .collect(),
    })
}

/// What the linking methods work on: the projects the owner means to make teams of, with the
/// claude.ai Projects among them, as `catervas_store::linking` reads them.
struct LinkScope {
    projects: Vec<LinkProject>,
    claude_ai: Vec<ClaudeAiProjectText>,
}

/// The projects chosen as teams (the choice made, else the suggestion the page showed), and the
/// teams already driven, apart from a folder that is gone.
fn link_scope(
    hub: &Hub,
    host: &dyn FindingHost,
    index: &MachineIndex,
) -> Result<LinkScope, Failure> {
    let listing = build_listing(hub, host, index).map_err(|e| internal(&e))?;
    let activity = index.link_activity().map_err(|e| internal(&e))?;
    let found = index.found().map_err(|e| internal(&e))?;
    let mut scope = LinkScope {
        projects: Vec::new(),
        claude_ai: Vec::new(),
    };
    for row in &listing.rows {
        let wanted =
            row.choice.or(row.suggested) == Some(Choice::Team) || row.already_team.is_some();
        if !wanted || row.gone || row.kind == "job" {
            continue;
        }
        let mut active_days = std::collections::BTreeSet::new();
        let mut written_in = std::collections::BTreeSet::new();
        for folder in &row.folders {
            active_days.extend(
                activity
                    .days_by_root
                    .get(folder)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
            written_in.extend(
                activity
                    .written_by_root
                    .get(folder)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        let folder = row.folders.first().cloned();
        scope.projects.push(LinkProject {
            key: row.key.clone(),
            name: row.name.clone(),
            last_at: row.last_at,
            remote_key: row.remote.clone(),
            root: matches!(row.kind, "folder" | "hand")
                .then(|| folder.clone())
                .flatten(),
            folder,
            active_days,
            written_in,
        });
        if let Some(project) = found
            .claude_ai
            .projects
            .iter()
            .find(|project| project.uuid == row.key)
        {
            scope.claude_ai.push(ClaudeAiProjectText {
                uuid: project.uuid.clone(),
                name: row.name.clone(),
                description: project.description.clone(),
                file_names: project.doc_names.clone(),
            });
        }
    }
    Ok(scope)
}

/// A project of the scope as the page shows it: its name, when it was last worked on, and its
/// folder with the home folder written `~`.
fn link_project_json(host: &dyn FindingHost, project: &LinkProject) -> Value {
    let home = host.home();
    let folder = project.folder.as_deref().map(|folder| {
        Path::new(folder).strip_prefix(&home).map_or_else(
            |_| folder.to_string(),
            |rest| format!("~/{}", rest.to_string_lossy()),
        )
    });
    json!({
        "key": project.key,
        "name": project.name,
        "last_at": project.last_at.map(moment),
        "folder": folder,
    })
}

fn link_project<'a>(scope: &'a LinkScope, key: &str) -> Option<&'a LinkProject> {
    scope.projects.iter().find(|project| project.key == key)
}

/// A proof as the page reads it: each end names its project.
fn proof_json(scope: &LinkScope, proof: &Proof) -> Value {
    let mut shown = proof.to_json();
    if let Some(ends) = shown["ends"].as_array_mut() {
        for end in ends {
            let name = end["project"]
                .as_str()
                .and_then(|key| link_project(scope, key))
                .map(|project| project.name.clone())
                .unwrap_or_default();
            end["name"] = json!(name);
        }
    }
    shown
}

fn link_failure(error: &LinkingError) -> Failure {
    match error {
        LinkingError::ProofNoLongerHolds => refusal(
            "proof_no_longer_holds",
            "the files that proved they work together no longer say so",
        ),
        LinkingError::UnknownProject => unknown_project(),
        LinkingError::Store(_) => internal(error),
    }
}

/// Opens the index and the scope the linking methods read, or the empty scope when there is no
/// index yet.
fn with_scope<T>(
    hub: &Hub,
    host: &dyn FindingHost,
    then: impl FnOnce(&mut MachineIndex, &LinkScope) -> Result<T, Failure>,
    empty: T,
) -> Result<T, Failure> {
    if !host.state_dir().join(MACHINE_DB).exists() {
        return Ok(empty);
    }
    let mut index = open_index(host)?;
    let scope = link_scope(hub, host, &index)?;
    then(&mut index, &scope)
}

fn read_of(host: &dyn FindingHost) -> impl Fn(&str) -> Vec<(String, String)> + '_ {
    move |root| host.committed_files(root)
}

/// `linking.pairs`: the pairs with a proof the owner is to be asked about, read from the files
/// as they are now.
fn linking_pairs(hub: &Hub, host: &dyn FindingHost) -> Result<Value, Failure> {
    with_scope(
        hub,
        host,
        |index, scope| {
            let evaluation = evaluate(&scope.projects, &scope.claude_ai, &read_of(host));
            let pairs =
                asked_pairs(index, &scope.projects, &evaluation).map_err(|e| internal(&e))?;
            let shown = pairs
                .iter()
                .filter_map(|pair| {
                    let (a, b) = (
                        link_project(scope, &pair.proof.a)?,
                        link_project(scope, &pair.proof.b)?,
                    );
                    Some(json!({
                        "a": link_project_json(host, a),
                        "b": link_project_json(host, b),
                        "proof": proof_json(scope, &pair.proof),
                        "also": pair.also,
                        "hints": pair.hints.iter().map(|hint| hint.as_wire()).collect::<Vec<_>>(),
                    }))
                })
                .collect::<Vec<_>>();
            Ok(json!({ "pairs": shown }))
        },
        json!({ "pairs": [] }),
    )
}

/// `linking.maybe`: the pairs with only hints and no answer.
fn linking_maybe(hub: &Hub, host: &dyn FindingHost) -> Result<Value, Failure> {
    with_scope(
        hub,
        host,
        |index, scope| {
            let evaluation = evaluate(&scope.projects, &scope.claude_ai, &read_of(host));
            let maybe = maybe_pairs(index, &evaluation).map_err(|e| internal(&e))?;
            let shown = maybe
                .iter()
                .filter_map(|hint| {
                    Some(json!({
                        "a": link_project_json(host, link_project(scope, &hint.a)?),
                        "b": link_project_json(host, link_project(scope, &hint.b)?),
                        "hints": hint.kinds.iter().map(|kind| kind.as_wire()).collect::<Vec<_>>(),
                    }))
                })
                .collect::<Vec<_>>();
            Ok(json!({ "maybe": shown }))
        },
        json!({ "maybe": [] }),
    )
}

/// `linking.groups`: the groups the owner's Yes answers form.
fn linking_groups(hub: &Hub, host: &dyn FindingHost) -> Result<Value, Failure> {
    with_scope(
        hub,
        host,
        |index, scope| {
            let evaluation = evaluate(&scope.projects, &scope.claude_ai, &read_of(host));
            let groups =
                confirmed_groups(index, &scope.projects, &evaluation).map_err(|e| internal(&e))?;
            let shown = groups
                .iter()
                .map(|group| {
                    json!({
                        "projects": group.projects,
                        "names": group
                            .projects
                            .iter()
                            .map(|key| link_project(scope, key).map_or_else(|| key.clone(), |project| project.name.clone()))
                            .collect::<Vec<_>>(),
                        "why": group.why,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({ "groups": shown }))
        },
        json!({ "groups": [] }),
    )
}

/// `linking.answer`: the owner's answer about a pair.
fn linking_answer(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let wanted = (
        params["a"].as_str().unwrap_or_default(),
        params["b"].as_str().unwrap_or_default(),
    );
    let answer = Answer::from_wire(params["answer"].as_str().unwrap_or_default())
        .ok_or_else(|| refusal("bad_answer", "the answer is yes or no"))?;
    let from_maybe = params["from_maybe"].as_bool().unwrap_or(false);
    if !host.state_dir().join(MACHINE_DB).exists() {
        return Err(unknown_project());
    }
    with_scope(
        hub,
        host,
        |index, scope| {
            answer_pair(
                index,
                &scope.projects,
                &scope.claude_ai,
                &read_of(host),
                wanted,
                answer,
                from_maybe,
                host.now(),
            )
            .map_err(|error| link_failure(&error))?;
            Ok(json!({}))
        },
        json!({}),
    )
}

/// `finding.choose`.
fn choose(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let key = params["key"].as_str().unwrap_or_default();
    let choice = Choice::from_wire(params["choice"].as_str().unwrap_or_default())
        .ok_or_else(unknown_project)?;
    let listing = listing_of(hub, host)?;
    if listing.find(key).is_none() {
        return Err(unknown_project());
    }
    open_index(host)?
        .put_choice(key, choice, host.now())
        .map_err(|e| internal(&e))?;
    Ok(json!({}))
}

/// `finding.separate`: the folder is its own project, apart from those it was folded with.
fn separate(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let key = params["key"].as_str().unwrap_or_default();
    let folder = params["folder"].as_str().unwrap_or_default();
    let listing = listing_of(hub, host)?;
    let row = listing
        .find(key)
        .filter(|row| row.folders.iter().any(|each| each == folder) && row.folders.len() > 1)
        .ok_or_else(unknown_project)?;
    let mut index = open_index(host)?;
    for other in row.folders.iter().filter(|each| *each != folder) {
        index.separate(folder, other).map_err(|e| internal(&e))?;
    }
    Ok(json!({}))
}

/// `finding.locate`: a gone folder is now at `path`, and its sessions follow.
fn locate(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let key = params["key"].as_str().unwrap_or_default();
    let path = params["path"].as_str().unwrap_or_default();
    let listing = listing_of(hub, host)?;
    let row = listing.find(key).ok_or_else(unknown_project)?;
    if !row.gone {
        return Err(refusal("not_gone", "that project's folder is still there"));
    }
    let home = host.home();
    let new = match member_root(&home, path) {
        Ok(folder) => folder,
        // A folder with a team is where a project can have moved to.
        Err(MemberPathRefusal::HasTeam) => {
            std::fs::canonicalize(home.join(path)).map_err(|e| internal(&e))?
        }
        Err(why) => return Err(refusal(why.code(), &why.message())),
    };
    let mut index = open_index(host)?;
    if let Some(wanted) = index.remote_of(key).map_err(|e| internal(&e))? {
        let found = Git::open(new.clone())
            .remote_url()
            .ok()
            .flatten()
            .and_then(|address| normalise_remote(&address));
        if found.as_deref() != Some(wanted.as_str()) {
            return Err(refusal(
                "not_the_same_repository",
                "that folder holds another repository",
            ));
        }
    }
    index
        .put_move(key, &new.to_string_lossy())
        .map_err(|e| internal(&e))?;
    place_sessions(&mut index, &host.not_projects(), host.now()).map_err(|e| internal(&e))?;
    Ok(json!({}))
}

/// `finding.declare_services`: the services the owner says they use replace those declared.
fn declare_services(host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let mut rows: Vec<(String, String)> = Vec::new();
    for id in params["ids"].as_array().into_iter().flatten() {
        let id = id.as_str().unwrap_or_default();
        let entry = CATALOG
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| refusal("unknown_service", "Catervas doesn't know that service"))?;
        rows.push((entry.id.to_string(), entry.name.to_string()));
    }
    if let Some(other) = params["other"]
        .as_str()
        .map(str::trim)
        .filter(|other| !other.is_empty())
    {
        rows.push((format!("other:{other}"), other.to_string()));
    }
    open_index(host)?
        .replace_declared_services(&rows, host.now())
        .map_err(|e| internal(&e))?;
    Ok(json!({}))
}

/// `finding.add_project`: a project by hand, in a folder under home, a repository, or both.
fn add_project(host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let folder = params["folder"].as_str().map(|each| host.home().join(each));
    let mut index = open_index(host)?;
    let added = add_project_by_hand(
        &mut index,
        params["name"].as_str().unwrap_or_default(),
        folder.as_deref(),
        params["repository"].as_str(),
        &host.not_projects(),
        host.now(),
    )
    .map_err(|error| match error {
        HistoryError::HandProject(_) | HistoryError::NoSuchFolder(_) => {
            refusal("cannot_add", &error.to_string())
        }
        other => internal(&other),
    })?;
    Ok(json!({
        "name": added.project.name,
        "root": added.project.root,
        "repository": added.project.remote,
        "same_as": added.same_as,
    }))
}

/// `finding.add_routine`: a routine typed in, under a project or the computer's.
fn add_routine(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let project_key = params["project_key"].as_str();
    if let Some(key) = project_key
        && listing_of(hub, host)?.find(key).is_none()
    {
        return Err(unknown_project());
    }
    let id = open_index(host)?
        .add_hand_routine(
            params["name"].as_str().unwrap_or_default(),
            project_key,
            params["schedule"].as_str().unwrap_or_default(),
            host.now(),
        )
        .map_err(|e| internal(&e))?;
    Ok(json!({ "id": id }))
}

/// Claude Code's settings file.
fn settings_file(host: &dyn FindingHost) -> PathBuf {
    host.claude_dir().join("settings.json")
}

/// `finding.retention`: how long Claude Code keeps its history.
fn retention(host: &dyn FindingHost) -> Value {
    let set = std::fs::read_to_string(settings_file(host))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|settings| settings["cleanupPeriodDays"].as_u64());
    json!({
        "cleanup_period_days": set.unwrap_or(DEFAULT_RETENTION_DAYS),
        "default": set.is_none(),
    })
}

/// `finding.keep_longer`: the one write Catervas makes into Claude Code's settings, on the owner's
/// press. Every other key stays, and so does the file's mode.
fn keep_longer(host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let days = params["days"]
        .as_u64()
        .filter(|days| RETENTION_OFFERED.contains(days))
        .ok_or_else(|| {
            refusal(
                "retention_not_offered",
                "Catervas keeps Claude Code's history for 90 days, a year or ten years",
            )
        })?;
    let file = settings_file(host);
    let mut settings = match std::fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str::<Value>(&text)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| {
                internal(&"Claude Code's settings.json is not JSON Catervas can keep; it was left as it is")
            })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(internal(&error)),
    };
    settings["cleanupPeriodDays"] = json!(days);
    let text = serde_json::to_string_pretty(&settings).map_err(|e| internal(&e))?;
    let written = (|| {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mode = std::fs::metadata(&file).map(|meta| meta.permissions()).ok();
        let temporary = file.with_extension("json.catervas-tmp");
        std::fs::write(&temporary, format!("{text}\n"))?;
        if let Some(mode) = mode {
            std::fs::set_permissions(&temporary, mode)?;
        }
        std::fs::rename(&temporary, &file)
    })();
    written.map_err(|e| internal(&e))?;
    Ok(json!({ "cleanup_period_days": days }))
}

/// `finding.pause`: while paused no scan starts, and one that runs ends.
fn pause(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let paused = params["paused"].as_bool().unwrap_or(false);
    host.set_paused(paused).map_err(|why| internal(&why))?;
    if paused {
        hub.finding.stop();
    }
    hub.finding.bump();
    Ok(json!({ "paused": paused }))
}

/// `finding.forget`: the index goes, the export card comes back, and finding stays paused.
fn forget(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    if params["confirmed"].as_bool() != Some(true) {
        return Err(refusal(
            "forget_needs_confirming",
            "say yes to forget everything Catervas found",
        ));
    }
    let finding = &hub.finding;
    finding.stop();
    let began = Instant::now();
    while finding.run().phase == Phase::Running && began.elapsed() < STOP_WAIT {
        std::thread::sleep(Duration::from_millis(10));
    }
    forget_machine_index(&host.state_dir().join(MACHINE_DB)).map_err(|e| internal(&e))?;
    host.set_paused(true).map_err(|why| internal(&why))?;
    host.set_export_dismissed(false)
        .map_err(|why| internal(&why))?;
    *finding.run() = Run::default();
    finding.bump();
    Ok(json!({ "forgotten": true }))
}

/// `method` of finding, answered on a blocking thread: it reads and writes files.
pub(super) async fn call_blocking(
    state: &Arc<super::DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let hub = Arc::clone(super::hub::hub_of(state)?);
    let (method, params) = (method.to_string(), params.clone());
    tokio::task::spawn_blocking(move || call(&hub, &method, &params))
        .await
        .unwrap_or_else(|error| Err(internal(&error)))
}

/// A temporary file that is deleted when it goes out of scope, whatever happened to it.
struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The answer to a refused or failed export.
fn export_error(status: StatusCode, word: &str, message: &str) -> Response {
    (status, Json(json!({ "error": word, "message": message }))).into_response()
}

/// Writes the request's body to `file`, a chunk at a time off the async workers, and answers
/// whether it was all written within `limit` bytes.
async fn write_body(body: Body, file: std::fs::File, limit: u64) -> Result<bool, std::io::Error> {
    use std::io::Write as _;

    let (chunks, mut written) = tokio::sync::mpsc::channel::<Bytes>(8);
    let writer = tokio::task::spawn_blocking(move || {
        let mut file = file;
        while let Some(chunk) = written.blocking_recv() {
            file.write_all(&chunk)?;
        }
        file.sync_all()
    });
    let (mut total, mut within) = (0_u64, true);
    let mut stream = body.into_data_stream();
    while let Some(Ok(chunk)) = stream.next().await {
        total += chunk.len() as u64;
        if total > limit {
            within = false;
            break;
        }
        if chunks.send(chunk).await.is_err() {
            break;
        }
    }
    drop(chunks);
    writer
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error)))?;
    Ok(within)
}

/// The word an export that could not be read is refused with.
fn export_word(error: &ExportError) -> &'static str {
    match error {
        ExportError::NotAZip
        | ExportError::NotAnExport(_)
        | ExportError::NotAList { .. }
        | ExportError::Unreadable { .. } => "not_an_export",
        ExportError::Damaged { .. } => "damaged",
        ExportError::Unsupported { .. } => "unsupported",
        ExportError::Encrypted { .. } => "encrypted",
        ExportError::TooLarge { .. } => "export_too_large",
        ExportError::NothingDated => "nothing_dated",
        ExportError::Io(_) | ExportError::Store(_) => "failed",
    }
}

/// `POST /hub/export`: claude.ai's export, dropped onto the page, read into the index. Behind the
/// page's own origin and its session, as the socket is. The body is written to a temporary file
/// in the state folder (mode 0600), read, and deleted, whatever the result.
pub(super) async fn export(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !from_own_page(&headers, web) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !has_session(&headers, web) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(hub) = state.hub() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(host) = hub.finding.host.get().cloned() else {
        return export_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            "this catervas has nothing to read an export with",
        );
    };
    let limits = ExportLimits::DEFAULT;
    if headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok())
        .and_then(|length| length.parse::<u64>().ok())
        .is_some_and(|length| length > limits.file_bytes)
    {
        return too_large();
    }
    let kept = host.state_dir();
    let (temporary, file) = match create_temporary(&kept) {
        Ok(made) => made,
        Err(error) => {
            return export_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed",
                &error.to_string(),
            );
        }
    };
    match write_body(body, file, limits.file_bytes).await {
        Ok(true) => {}
        Ok(false) => return too_large(),
        Err(error) => {
            return export_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed",
                &error.to_string(),
            );
        }
    }
    let imported = tokio::task::spawn_blocking(move || {
        let imported = open_index(host.as_ref())
            .map_err(|_| ExportError::Io(std::io::Error::other("the index could not be opened")))
            .and_then(|mut index| {
                import_claude_export(&mut index, &temporary.0, limits, host.now())
            });
        drop(temporary);
        if imported.is_ok() {
            let _ = host.set_export_dismissed(false);
        }
        imported
    })
    .await;
    match imported {
        Ok(Ok(summary)) => Json(json!({
            "conversations": summary.conversations,
            "new_conversations": summary.new_conversations,
            "projects": summary.projects,
            "new_projects": summary.new_projects,
            "skipped": summary.skipped,
            "skipped_projects": summary.skipped_projects,
            "content_at": moment(summary.content_at),
        }))
        .into_response(),
        Ok(Err(ExportError::TooLarge { .. })) => too_large(),
        Ok(Err(error)) => {
            let status = if matches!(error, ExportError::Io(_) | ExportError::Store(_)) {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            export_error(status, export_word(&error), &error.to_string())
        }
        Err(error) => export_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            &error.to_string(),
        ),
    }
}

fn too_large() -> Response {
    export_error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "export_too_large",
        &ExportError::TooLarge {
            limit_bytes: ExportLimits::DEFAULT.file_bytes,
        }
        .to_string(),
    )
}

/// A new temporary file for an export in `folder`, readable by its owner alone.
fn create_temporary(folder: &Path) -> std::io::Result<(Temporary, std::fs::File)> {
    use std::os::unix::fs::OpenOptionsExt as _;
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    std::fs::create_dir_all(folder)?;
    let path = folder.join(format!(
        "export-{}-{}.zip.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    Ok((Temporary(path), file))
}

/// Scans once at start and then every 24 hours, for as long as it is polled: while finding is not
/// paused and a first scan, which the owner asked for, made the index. A scan that still runs when
/// the time comes is left to finish.
pub async fn rescan_daily(hub: Arc<Hub>) {
    loop {
        if hub.finding.rescan_wanted() {
            let _ = start(&hub);
        }
        tokio::time::sleep(RESCAN_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use catervas_store::claude_history::add_project_by_hand;
    use catervas_store::machine::{MACHINE_DB, open_machine_index};
    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};

    use catervas_core::claude_services::catalog::CATALOG;

    use super::{FindingHost, FindingProgress, call, rescan_daily};
    use crate::connectors::{ConnectorSecrets, MemoryConnectorSecrets, SecretAt};
    use crate::daemon::hub::{Hub, IdleStatus, IdleTeam, TeamSlug};
    use crate::github::GithubPlaces;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
            .expect("a moment")
            .with_timezone(&Utc)
    }

    /// A host whose scan waits for `release`, then adds one project by hand and reports 2 files.
    struct Fake {
        dir: PathBuf,
        release: AtomicBool,
        paused: AtomicBool,
        dismissed: AtomicBool,
        scans: AtomicU64,
        secrets: Arc<MemoryConnectorSecrets>,
        api: Mutex<String>,
        /// The committed files of each folder, by its path.
        files: Mutex<std::collections::BTreeMap<String, Vec<(String, String)>>>,
    }

    impl Fake {
        fn new(name: &str) -> Arc<Self> {
            let dir = std::env::temp_dir()
                .join(format!("catervas-finding-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("home/shop")).expect("a project folder");
            Arc::new(Self {
                dir,
                release: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                dismissed: AtomicBool::new(false),
                scans: AtomicU64::new(0),
                secrets: Arc::new(MemoryConnectorSecrets::default()),
                api: Mutex::new(String::new()),
                files: Mutex::new(std::collections::BTreeMap::new()),
            })
        }

        fn shop(&self) -> PathBuf {
            self.dir.join("home/shop").canonicalize().expect("shop")
        }
    }

    impl FindingHost for Fake {
        fn scan(&self, stop: &AtomicBool, report: &dyn Fn(FindingProgress)) -> Result<(), String> {
            self.scans.fetch_add(1, Ordering::SeqCst);
            let counts = |files_read| FindingProgress {
                files_total: 2,
                files_read,
                sessions: 7,
                projects: 1,
                jobs: 2,
                ..FindingProgress::default()
            };
            report(counts(0));
            while !self.release.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut index = open_machine_index(&self.state_dir().join(MACHINE_DB), self.now())
                .map_err(|e| e.to_string())?;
            add_project_by_hand(
                &mut index,
                "Shop",
                Some(&self.dir.join("home/shop")),
                None,
                &[],
                self.now(),
            )
            .map_err(|e| e.to_string())?;
            report(counts(2));
            Ok(())
        }
        fn state_dir(&self) -> PathBuf {
            self.dir.join("state")
        }
        fn claude_dir(&self) -> PathBuf {
            self.dir.join("home/.claude")
        }
        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }
        fn now(&self) -> DateTime<Utc> {
            now()
        }
        fn not_projects(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn paused(&self) -> bool {
            self.paused.load(Ordering::SeqCst)
        }
        fn set_paused(&self, paused: bool) -> Result<(), String> {
            self.paused.store(paused, Ordering::SeqCst);
            Ok(())
        }
        fn export_dismissed(&self) -> bool {
            self.dismissed.load(Ordering::SeqCst)
        }
        fn set_export_dismissed(&self, dismissed: bool) -> Result<(), String> {
            self.dismissed.store(dismissed, Ordering::SeqCst);
            Ok(())
        }
        fn secrets(&self) -> Arc<dyn ConnectorSecrets> {
            self.secrets.clone()
        }
        fn github(&self) -> GithubPlaces {
            GithubPlaces {
                api: self.api.lock().expect("api").clone(),
                ..GithubPlaces::default()
            }
        }
        fn committed_files(&self, root: &str) -> Vec<(String, String)> {
            self.files
                .lock()
                .expect("files")
                .get(root)
                .cloned()
                .unwrap_or_default()
        }
    }

    fn asked(hub: &Hub, method: &str, params: &Value) -> Value {
        call(hub, method, params).unwrap_or_else(|failed| panic!("{method}: {failed:?}"))
    }

    fn refused(hub: &Hub, method: &str, params: &Value) -> String {
        call(hub, method, params).expect_err("a refusal").message
    }

    #[test]
    fn lists_the_services_the_owner_can_tap() {
        let fake = Fake::new("catalog");
        let hub = Hub::default();
        hub.set_finder(fake);

        let answer = asked(&hub, "finding.catalog", &json!({}));
        let services = answer["services"].as_array().expect("a list of services");
        assert_eq!(
            services.len(),
            CATALOG.len(),
            "every service of the catalog"
        );
        assert_eq!(
            services[0],
            json!({
                "id": CATALOG[0].id,
                "name": CATALOG[0].name,
                "kind": CATALOG[0].kind.as_wire(),
            }),
            "in the catalog's order"
        );
        assert!(
            services.contains(
                &json!({ "id": "zoho_books", "name": "Zoho Books", "kind": "bookkeeping" })
            ),
            "{services:?}"
        );
        // What the page taps is what finding.declare_services takes.
        let ids: Vec<&str> = services
            .iter()
            .map(|s| s["id"].as_str().expect("an id"))
            .collect();
        asked(&hub, "finding.declare_services", &json!({ "ids": ids }));
    }

    #[test]
    fn connects_github_and_lists_its_repositories() {
        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let fixture =
            runtime.block_on(crate::github_fixture::Fixture::start("ghp_invented", now()));
        let fake = Fake::new("github");
        *fake.api.lock().expect("api") = fixture.address.clone();
        let hub = Hub::default();
        hub.set_finder(fake.clone());
        let at = SecretAt::machine("github");

        // Before connecting there is nothing, and a token GitHub refuses keeps nothing and is not
        // quoted back.
        assert_eq!(
            asked(&hub, "github.status", &json!({})),
            json!({ "connected": false })
        );
        let wrong = "ghp_wrong_invented_token";
        let message = refused(&hub, "github.connect", &json!({ "token": wrong }));
        assert!(message.starts_with("github_token_refused: "), "{message}");
        assert!(!message.contains(wrong), "{message}");
        assert!(fake.secrets.load(&at).expect("a store").is_none());

        // A token it accepts is kept under machine:github, and answers the login.
        let connected = asked(&hub, "github.connect", &json!({ "token": "ghp_invented" }));
        assert_eq!(connected, json!({ "login": "octo" }));
        let kept = fake.secrets.load(&at).expect("a store").expect("an entry");
        assert!(kept.keys.values().any(|key| key.expose() == "ghp_invented"));
        assert_eq!(at.account(), "machine:github");
        assert_eq!(
            asked(&hub, "github.status", &json!({})),
            json!({ "connected": true, "login": "octo" })
        );

        // The list has the repositories that are not on this computer, with Claude's work.
        let listed = asked(&hub, "finding.list", &json!({}));
        let rows = listed["main"].as_array().expect("rows");
        let shop = rows
            .iter()
            .find(|row| row["name"] == "octo/shop")
            .unwrap_or_else(|| panic!("{listed}"));
        assert_eq!(shop["kind"], "github", "{shop}");
        assert_eq!(shop["suggested"], "watch", "{shop}");
        assert_eq!(shop["key"], "github.com/octo/shop", "{shop}");
        let work = shop["claude_work"].as_array().expect("work");
        assert!(
            work.contains(&json!({ "kind": "branch", "name": "claude/fix-login" })),
            "{work:?}"
        );
        assert!(
            work.contains(
                &json!({ "kind": "pull", "name": "Fix the login", "number": 12, "draft": true })
            ),
            "{work:?}"
        );
        assert!(
            rows.iter().any(|row| row["name"] == "octo/notes"),
            "{listed}"
        );
        assert!(
            !rows.iter().any(|row| row["name"] == "octo/old-thing"),
            "{listed}"
        );

        // Disconnecting forgets the key and lists no more of it.
        asked(&hub, "github.disconnect", &json!({}));
        assert!(fake.secrets.load(&at).expect("a store").is_none());
        assert_eq!(
            asked(&hub, "github.status", &json!({})),
            json!({ "connected": false })
        );
        drop(runtime);
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
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{definition}: {errors:?} in {value}");
    }

    /// Asks `method` as a frame the schema must accept, and checks the answer against `result`.
    fn linked(hub: &Hub, method: &str, params: &Value, result: &str) -> Value {
        let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        catervas_protocol::rpc::rpc_request_from_value(&frame)
            .unwrap_or_else(|errors| panic!("{method}: {errors:?}"));
        let answer = asked(hub, method, params);
        conforms(&answer, result);
        answer
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one scenario of the four methods, told in the order the owner meets them"
    )]
    fn links_through_the_hub() {
        let fake = Fake::new("links");
        let hub = Hub::default();
        hub.set_finder(fake.clone());
        // Three projects the owner added; "Shop" uses "Api" through a local path, and "Shop Admin"
        // only has a word in its name in common with "Shop".
        for folder in ["shop", "api", "shop-admin"] {
            std::fs::create_dir_all(fake.dir.join("home").join(folder)).expect("a folder");
        }
        let mut index =
            open_machine_index(&fake.state_dir().join(MACHINE_DB), now()).expect("an index");
        for (name, folder) in [
            ("Shop", "shop"),
            ("Api", "api"),
            ("Shop Admin", "shop-admin"),
        ] {
            add_project_by_hand(
                &mut index,
                name,
                Some(&fake.dir.join("home").join(folder)),
                None,
                &[],
                now(),
            )
            .expect("a project");
        }
        drop(index);
        let root = |folder: &str| {
            fake.dir
                .join("home")
                .join(folder)
                .canonicalize()
                .expect("a root")
                .to_string_lossy()
                .into_owned()
        };
        fake.files.lock().expect("files").insert(
            root("shop"),
            vec![(
                "package.json".to_string(),
                "{\n  \"dependencies\": {\n    \"api\": \"file:../api\"\n  }\n}\n".to_string(),
            )],
        );

        // The pair with a proof is asked, and only it.
        let asked_pairs = linked(&hub, "linking.pairs", &json!({}), "linkingPairsResult");
        let pairs = asked_pairs["pairs"].as_array().expect("pairs");
        assert_eq!(pairs.len(), 1, "{asked_pairs}");
        let pair = &pairs[0];
        assert_eq!(pair["a"]["name"], "Shop", "{pair}");
        assert_eq!(pair["b"]["name"], "Api", "{pair}");
        assert_eq!(pair["proof"]["kind"], "path_dependency", "{pair}");
        assert_eq!(pair["proof"]["sentence"], "Shop uses Api's code.", "{pair}");
        assert_eq!(pair["proof"]["ends"][0]["file"], "package.json", "{pair}");
        assert_eq!(pair["proof"]["ends"][0]["line"], 3, "{pair}");
        assert_eq!(pair["proof"]["ends"][0]["name"], "Shop", "{pair}");
        let (a, b) = (
            pair["a"]["key"].as_str().expect("a key").to_string(),
            pair["b"]["key"].as_str().expect("a key").to_string(),
        );

        // The pair with only a hint is listed apart.
        let maybe = linked(&hub, "linking.maybe", &json!({}), "linkingMaybeResult");
        assert_eq!(
            maybe["maybe"].as_array().expect("maybe").len(),
            1,
            "{maybe}"
        );
        assert_eq!(
            maybe["maybe"][0]["hints"],
            json!(["similar_names"]),
            "{maybe}"
        );
        let (c, d) = (
            maybe["maybe"][0]["a"]["key"]
                .as_str()
                .expect("a key")
                .to_string(),
            maybe["maybe"][0]["b"]["key"]
                .as_str()
                .expect("a key")
                .to_string(),
        );

        // Nothing is a group until the owner says yes, and a hint joins nothing.
        let none = linked(&hub, "linking.groups", &json!({}), "linkingGroupsResult");
        assert_eq!(none["groups"], json!([]), "{none}");
        linked(
            &hub,
            "linking.answer",
            &json!({ "a": a, "b": b, "answer": "yes" }),
            "emptyResult",
        );
        let groups = linked(&hub, "linking.groups", &json!({}), "linkingGroupsResult");
        assert_eq!(
            groups["groups"].as_array().expect("groups").len(),
            1,
            "{groups}"
        );
        let mut members: Vec<String> = groups["groups"][0]["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .map(|key| key.as_str().expect("a key").to_string())
            .collect();
        members.sort();
        let mut expected = vec![a.clone(), b.clone()];
        expected.sort();
        assert_eq!(members, expected, "{groups}");
        assert_eq!(
            groups["groups"][0]["why"],
            json!(["Shop uses Api's code."]),
            "{groups}"
        );
        let rest = linked(&hub, "linking.pairs", &json!({}), "linkingPairsResult");
        assert_eq!(rest["pairs"], json!([]), "{rest}");

        // A hint pair answered Separate is not listed again, and a Yes to one nobody proved needs
        // the owner's own choice from the list.
        let message = refused(
            &hub,
            "linking.answer",
            &json!({ "a": c, "b": d, "answer": "yes" }),
        );
        assert!(message.starts_with("proof_no_longer_holds: "), "{message}");
        linked(
            &hub,
            "linking.answer",
            &json!({ "a": c, "b": d, "answer": "no" }),
            "emptyResult",
        );
        let after = linked(&hub, "linking.maybe", &json!({}), "linkingMaybeResult");
        assert_eq!(after["maybe"], json!([]), "{after}");
        assert!(
            refused(
                &hub,
                "linking.answer",
                &json!({ "a": a, "b": "nobody", "answer": "no" })
            )
            .starts_with("unknown_project: ")
        );
    }

    #[test]
    fn finds_projects_from_the_hub() {
        let fake = Fake::new("hub");
        let hub = Hub::default();
        hub.set_finder(fake.clone());

        // A scan runs, then is done with the host's counts.
        assert_eq!(asked(&hub, "finding.start", &json!({}))["started"], true);
        assert_eq!(
            asked(&hub, "finding.start", &json!({}))["started"],
            false,
            "a second start while running does nothing"
        );
        assert_eq!(
            asked(&hub, "finding.status", &json!({}))["phase"],
            "running"
        );
        fake.release.store(true, Ordering::SeqCst);
        let began = Instant::now();
        let status = loop {
            let status = asked(&hub, "finding.status", &json!({}));
            if status["phase"] == "done" {
                break status;
            }
            assert!(began.elapsed() < Duration::from_secs(10), "{status}");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status["sessions"], 7, "{status}");
        assert_eq!(status["projects"], 1, "{status}");
        assert_eq!(status["jobs"], 2, "{status}");
        assert_eq!(status["files_read"], 2, "{status}");

        // The list carries the suggestion, and the choice once the owner made it.
        let listed = asked(&hub, "finding.list", &json!({}));
        let row = listed["main"][0].clone();
        assert_eq!(row["name"], "Shop", "{listed}");
        assert_eq!(row["suggested"], "team", "{listed}");
        assert_eq!(row["choice"], Value::Null, "{listed}");
        let key = row["key"].as_str().expect("a key").to_string();
        asked(
            &hub,
            "finding.choose",
            &json!({ "key": key, "choice": "ignore" }),
        );
        let after = asked(&hub, "finding.list", &json!({}));
        assert_eq!(
            after["main"],
            json!([]),
            "an ignored project leaves the list"
        );
        assert_eq!(after["ignored"][0]["choice"], "ignore", "{after}");
        assert_eq!(
            asked(&hub, "finding.ignored", &json!({}))["ignored"][0]["key"],
            key
        );
        asked(
            &hub,
            "finding.choose",
            &json!({ "key": key, "choice": "watch" }),
        );
        let back = asked(&hub, "finding.list", &json!({}));
        assert_eq!(back["main"][0]["choice"], "watch", "{back}");
        assert!(
            refused(
                &hub,
                "finding.choose",
                &json!({ "key": "nope", "choice": "team" })
            )
            .contains("unknown_project")
        );

        // A team the hub holds for the project's folder is named.
        assert_eq!(back["main"][0]["already_team"], Value::Null);
        hub.set_idle(vec![IdleTeam {
            slug: TeamSlug::new("shop-team"),
            name: "Shop".to_string(),
            root: fake.shop(),
            status: IdleStatus::Elsewhere,
        }]);
        let named = asked(&hub, "finding.list", &json!({}));
        assert_eq!(named["main"][0]["already_team"], "shop-team", "{named}");
        assert!(
            refused(
                &hub,
                "finding.locate",
                &json!({ "key": key, "path": "shop" })
            )
            .contains("not_gone")
        );

        // Paused, nothing starts.
        asked(&hub, "finding.pause", &json!({ "paused": true }));
        assert!(
            refused(&hub, "finding.start", &json!({})).contains("finding_paused"),
            "paused finding starts nothing"
        );
        assert_eq!(fake.scans.load(Ordering::SeqCst), 1);
        asked(&hub, "finding.pause", &json!({ "paused": false }));

        // Forgetting needs a yes, and then deletes the index.
        assert!(refused(&hub, "finding.forget", &json!({})).contains("forget_needs_confirming"));
        assert!(fake.state_dir().join(MACHINE_DB).exists());
        let forgotten = asked(&hub, "finding.forget", &json!({ "confirmed": true }));
        assert_eq!(forgotten, json!({ "forgotten": true }));
        assert!(!fake.state_dir().join(MACHINE_DB).exists());
        assert_eq!(asked(&hub, "finding.status", &json!({}))["phase"], "idle");
        // Forgetting everything leaves finding paused, as its screen says.
        assert_eq!(asked(&hub, "finding.status", &json!({}))["paused"], true);
        assert!(refused(&hub, "finding.start", &json!({})).contains("finding_paused"));
    }

    /// Gives the scan thread and the rescan task time to run, without letting paused time move.
    async fn settle() {
        for _ in 0..40 {
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rescans_each_day_while_finding_is_on() {
        let day = Duration::from_hours(24);
        let scans = |fake: &Fake| fake.scans.load(Ordering::SeqCst);

        // With an index present a scan starts at once, and again after 24 hours.
        let fake = Fake::new("rescan-index");
        fake.release.store(true, Ordering::SeqCst);
        drop(open_machine_index(&fake.state_dir().join(MACHINE_DB), now()).expect("an index"));
        let hub = Arc::new(Hub::default());
        hub.set_finder(fake.clone());
        let task = tokio::spawn(rescan_daily(Arc::clone(&hub)));
        settle().await;
        assert_eq!(scans(&fake), 1, "a scan at start");
        tokio::time::advance(day).await;
        settle().await;
        assert_eq!(scans(&fake), 2, "and one a day later");

        // Paused, none starts; resumed, the next day's does.
        asked(&hub, "finding.pause", &json!({ "paused": true }));
        tokio::time::advance(day).await;
        settle().await;
        assert_eq!(scans(&fake), 2, "paused finding scans nothing");
        asked(&hub, "finding.pause", &json!({ "paused": false }));
        tokio::time::advance(day).await;
        settle().await;
        assert_eq!(scans(&fake), 3);
        task.abort();

        // With no index, none starts until the owner starts one.
        let fake = Fake::new("rescan-none");
        fake.release.store(true, Ordering::SeqCst);
        let hub = Arc::new(Hub::default());
        hub.set_finder(fake.clone());
        let task = tokio::spawn(rescan_daily(Arc::clone(&hub)));
        settle().await;
        tokio::time::advance(day).await;
        settle().await;
        assert_eq!(scans(&fake), 0, "no first scan was asked for");
        asked(&hub, "finding.start", &json!({}));
        settle().await;
        assert_eq!(scans(&fake), 1);
        tokio::time::advance(day).await;
        settle().await;
        assert_eq!(
            scans(&fake),
            2,
            "the index the owner asked for is looked at again"
        );
        task.abort();
    }
}
