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
use catervas_store::claude_export::{ExportError, ExportLimits, import_claude_export};
use catervas_store::claude_history::{HistoryError, add_project_by_hand, place_sessions};
use catervas_store::git::Git;
use catervas_store::machine::{
    MACHINE_DB, MachineIndex, Ownership, forget_machine_index, open_machine_index,
};
use catervas_store::repos::{MemberPathRefusal, member_root};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::DaemonState;
use super::hub::Hub;
use super::web::{Failure, INTERNAL_ERROR, REFUSED, from_own_page, has_session};

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
pub(super) const METHODS: [&str; 17] = [
    "finding.offer",
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
];

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
fn host_of(hub: &Hub) -> Result<&Arc<dyn FindingHost>, Failure> {
    hub.finding
        .host
        .get()
        .ok_or_else(|| internal(&"this catervas has nothing to look through your history with"))
}

/// The machine index, made if it was not there.
fn open_index(host: &dyn FindingHost) -> Result<MachineIndex, Failure> {
    open_machine_index(&host.state_dir().join(MACHINE_DB), host.now()).map_err(|e| internal(&e))
}

/// `method` of finding, whose params the schema already passed. It reads and writes files, so it
/// runs off the async workers.
pub(super) fn call(hub: &Hub, method: &str, params: &Value) -> Result<Value, Failure> {
    let host = host_of(hub)?;
    match method {
        "finding.offer" => Ok(offer(host.as_ref())),
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
        _ => forget(hub, host.as_ref(), params),
    }
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
        rows.push(make(
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
        ));
    }
    for hand in &found.by_hand {
        let key = hand
            .root
            .clone()
            .or_else(|| hand.remote.clone())
            .unwrap_or_else(|| hand.name.clone());
        rows.push(make(
            key,
            hand.name.clone(),
            "hand",
            hand.root.iter().cloned().collect(),
            None,
            Candidate::ByHand,
            Vec::new(),
            false,
        ));
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

/// `finding.forget`: the index goes, and so do the two settings that belong to finding.
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
    host.set_paused(false).map_err(|why| internal(&why))?;
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
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use catervas_store::claude_history::add_project_by_hand;
    use catervas_store::machine::{MACHINE_DB, open_machine_index};
    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};

    use super::{FindingHost, FindingProgress, call, rescan_daily};
    use crate::daemon::hub::{Hub, IdleStatus, IdleTeam, TeamSlug};

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
    }

    fn asked(hub: &Hub, method: &str, params: &Value) -> Value {
        call(hub, method, params).unwrap_or_else(|failed| panic!("{method}: {failed:?}"))
    }

    fn refused(hub: &Hub, method: &str, params: &Value) -> String {
        call(hub, method, params).expect_err("a refusal").message
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
