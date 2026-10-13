//! The owner's recurring jobs: what kinds there are, how a job is tied to a project, and Claude
//! Code's own records of scheduled work. Nothing a job runs, says or is given is ever read.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use crate::claude_history::TITLE_LIMIT;

/// How many days a session-only or recurring saved cron lasts (Claude Code's `recurringMaxAgeMs`).
pub const SESSION_CRON_DAYS: i64 = 7;

/// How many characters of a background job's state are kept.
pub const STATE_LIMIT: usize = 40;

/// Where a job was found, in the order jobs are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobKind {
    /// A workflow's `schedule` in `.github/workflows/`.
    GithubActions,
    /// A `crons` entry of `vercel.json`.
    Vercel,
    /// A Kubernetes `CronJob`.
    Kubernetes,
    /// A cron job of `render.yaml`.
    Render,
    /// A literal `schedule_expression` in a Terraform file.
    Terraform,
    /// A line of the user's crontab.
    Crontab,
    /// A timer of `systemctl --user`.
    SystemdUser,
    /// A timer of the system's `systemctl`.
    SystemdSystem,
    /// An agent in `~/Library/LaunchAgents`.
    Launchd,
    /// A Windows Task Scheduler task, seen through WSL.
    TaskScheduler,
    /// A `CronCreate` call in a session.
    SessionCron,
    /// A task in `.claude/scheduled_tasks.json`.
    SavedSessionCron,
    /// A folder of `~/.claude/jobs`.
    BackgroundJob,
}

impl JobKind {
    /// The kind's value in `machine.db` and in `--json`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::GithubActions => "github_actions",
            Self::Vercel => "vercel",
            Self::Kubernetes => "kubernetes",
            Self::Render => "render",
            Self::Terraform => "terraform",
            Self::Crontab => "crontab",
            Self::SystemdUser => "systemd_user",
            Self::SystemdSystem => "systemd_system",
            Self::Launchd => "launchd",
            Self::TaskScheduler => "task_scheduler",
            Self::SessionCron => "session_cron",
            Self::SavedSessionCron => "saved_session_cron",
            Self::BackgroundJob => "background_job",
        }
    }

    /// The kind for a wire value.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        [
            Self::GithubActions,
            Self::Vercel,
            Self::Kubernetes,
            Self::Render,
            Self::Terraform,
            Self::Crontab,
            Self::SystemdUser,
            Self::SystemdSystem,
            Self::Launchd,
            Self::TaskScheduler,
            Self::SessionCron,
            Self::SavedSessionCron,
            Self::BackgroundJob,
        ]
        .into_iter()
        .find(|kind| kind.as_wire() == wire)
    }

    /// The kind as the owner reads it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::GithubActions => "GitHub Actions",
            Self::Vercel => "Vercel",
            Self::Kubernetes => "Kubernetes",
            Self::Render => "Render",
            Self::Terraform => "Terraform",
            Self::Crontab => "crontab",
            Self::SystemdUser => "systemd (user)",
            Self::SystemdSystem => "systemd",
            Self::Launchd => "launchd",
            Self::TaskScheduler => "Task Scheduler",
            Self::SessionCron => "session cron",
            Self::SavedSessionCron => "saved session cron",
            Self::BackgroundJob => "background job",
        }
    }
}

/// One task of `.claude/scheduled_tasks.json`; its prompt is never read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCron {
    /// The task's id.
    pub id: String,
    /// Its schedule, as written.
    pub cron: String,
    /// When it was created.
    pub created_at: DateTime<Utc>,
    /// Whether it repeats.
    pub is_recurring: bool,
    /// Whether it never expires.
    pub is_permanent: bool,
}

/// Reads `.claude/scheduled_tasks.json`: only each task's `id`, `cron`, `createdAt` (ms),
/// `recurring` and `permanent`. A task missing one of the first three is left out. `None` when the
/// text is not a JSON object.
#[must_use]
pub fn read_scheduled_tasks(text: &str) -> Option<Vec<SavedCron>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let tasks = object.get("tasks").and_then(Value::as_array);
    Some(
        tasks
            .into_iter()
            .flatten()
            .filter_map(|task| {
                Some(SavedCron {
                    id: task.get("id")?.as_str()?.to_string(),
                    cron: task.get("cron")?.as_str()?.to_string(),
                    created_at: DateTime::from_timestamp_millis(task.get("createdAt")?.as_i64()?)?,
                    is_recurring: task
                        .get("recurring")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    is_permanent: task
                        .get("permanent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
            })
            .collect(),
    )
}

/// When a cron expires: `SESSION_CRON_DAYS` after it was created when it recurs and is not
/// permanent, and never otherwise.
#[must_use]
pub fn cron_expires_at(
    created_at: DateTime<Utc>,
    is_recurring: bool,
    is_permanent: bool,
) -> Option<DateTime<Utc>> {
    (is_recurring && !is_permanent).then(|| created_at + Duration::days(SESSION_CRON_DAYS))
}

/// What a background job's `state.json` says; nothing else in it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJob {
    /// Its name, cut to `TITLE_LIMIT` characters.
    pub name: String,
    /// Its state, cut to `STATE_LIMIT` characters.
    pub state: String,
    /// The folder it works in.
    pub cwd: Option<String>,
    /// When its state last changed.
    pub updated_at: Option<DateTime<Utc>>,
}

/// Reads a background job's `state.json`, which Claude Code does not document, so it is read
/// fail-soft: `None` unless `name` and `state` are strings.
#[must_use]
pub fn read_background_job(text: &str) -> Option<BackgroundJob> {
    let value: Value = serde_json::from_str(text).ok()?;
    let cut = |key: &str, limit: usize| -> Option<String> {
        Some(
            value
                .get(key)?
                .as_str()?
                .trim()
                .chars()
                .take(limit)
                .collect(),
        )
    };
    Some(BackgroundJob {
        name: cut("name", TITLE_LIMIT)?,
        state: cut("state", STATE_LIMIT)?,
        cwd: value.get("cwd").and_then(Value::as_str).map(str::to_string),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.with_timezone(&Utc)),
    })
}

/// The deepest of `roots` that contains any of `paths`, compared component by component.
#[must_use]
pub fn tie_to_project(paths: &[String], roots: &[String]) -> Option<String> {
    roots
        .iter()
        .filter(|root| paths.iter().any(|path| Path::new(path).starts_with(root)))
        .max_by_key(|root| Path::new(root).components().count())
        .cloned()
}

/// The computer's schedulers: crontab, systemd, launchd and Windows Task Scheduler.
pub mod computer;
/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;
/// The schedules committed in a repository's files.
pub mod repository;

#[cfg(test)]
mod tests {
    use super::fixtures::{BACKGROUND_JOB, SCHEDULED_TASKS};
    use super::*;
    use chrono::{DateTime, Duration, Utc};

    const KINDS: [(JobKind, &str, &str); 13] = [
        (JobKind::GithubActions, "github_actions", "GitHub Actions"),
        (JobKind::Vercel, "vercel", "Vercel"),
        (JobKind::Kubernetes, "kubernetes", "Kubernetes"),
        (JobKind::Render, "render", "Render"),
        (JobKind::Terraform, "terraform", "Terraform"),
        (JobKind::Crontab, "crontab", "crontab"),
        (JobKind::SystemdUser, "systemd_user", "systemd (user)"),
        (JobKind::SystemdSystem, "systemd_system", "systemd"),
        (JobKind::Launchd, "launchd", "launchd"),
        (JobKind::TaskScheduler, "task_scheduler", "Task Scheduler"),
        (JobKind::SessionCron, "session_cron", "session cron"),
        (
            JobKind::SavedSessionCron,
            "saved_session_cron",
            "saved session cron",
        ),
        (JobKind::BackgroundJob, "background_job", "background job"),
    ];

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a time")
            .with_timezone(&Utc)
    }

    #[test]
    fn names_each_kind() {
        for (kind, wire, label) in KINDS {
            assert_eq!(kind.as_wire(), wire);
            assert_eq!(kind.label(), label);
            assert_eq!(JobKind::from_wire(wire), Some(kind));
        }
        assert_eq!(JobKind::from_wire("cron"), None);
        let mut sorted = KINDS.map(|(kind, _, _)| kind);
        sorted.sort();
        assert_eq!(sorted, KINDS.map(|(kind, _, _)| kind));
    }

    #[test]
    fn reads_saved_session_crons() {
        let tasks = read_scheduled_tasks(SCHEDULED_TASKS).expect("a JSON object");
        let created = at("2026-10-12T09:00:00Z");
        let ids: Vec<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, ["a1b2c3d4", "e5f6", "p1"]);
        assert_eq!(tasks[0].cron, "0 9 * * 1-5");
        assert_eq!(tasks[0].created_at, created);
        assert!(tasks[0].is_recurring && !tasks[0].is_permanent);
        assert!(!tasks[1].is_recurring);
        assert!(tasks[2].is_permanent);
        assert_eq!(read_scheduled_tasks(r#"{"tasks":3}"#), Some(vec![]));
        assert_eq!(read_scheduled_tasks("not json"), None);
        assert_eq!(
            cron_expires_at(created, true, false),
            Some(created + Duration::days(SESSION_CRON_DAYS))
        );
        assert_eq!(cron_expires_at(created, false, false), None);
        assert_eq!(cron_expires_at(created, true, true), None);
    }

    #[test]
    fn reads_a_background_job() {
        let job = read_background_job(BACKGROUND_JOB).expect("a job");
        assert_eq!(job.name, "shop-cleanup");
        assert_eq!(job.state, "blocked");
        assert_eq!(job.cwd.as_deref(), Some("/home/me/shop"));
        assert_eq!(job.updated_at, Some(at("2026-10-11T08:00:00Z")));
        let bare = read_background_job(r#"{"name":"n","state":"s"}"#).expect("a job");
        assert_eq!((bare.cwd, bare.updated_at), (None, None));
        for text in [r#"{"state":"s"}"#, r#"{"name":"n"}"#, "not json"] {
            assert_eq!(read_background_job(text), None, "{text}");
        }
        let long = format!(
            r#"{{"name":"{}","state":"{}"}}"#,
            "n".repeat(300),
            "s".repeat(100)
        );
        let cut = read_background_job(&long).expect("a job");
        assert_eq!(cut.name.chars().count(), 200);
        assert_eq!(cut.state.chars().count(), STATE_LIMIT);
    }

    /// Guard: checked by mutation (keep a prompt or a key), not watched to fail.
    #[test]
    fn reads_no_prompt_or_key() {
        let everything = format!(
            "{:?}{:?}",
            read_scheduled_tasks(SCHEDULED_TASKS),
            read_background_job(BACKGROUND_JOB)
        );
        assert!(!everything.contains("NEVER-KEPT-TEXT"), "{everything}");
        assert!(!everything.contains("SECRET"), "{everything}");
    }

    #[test]
    fn ties_a_job_to_the_deepest_project() {
        let roots = ["/home/me/a".to_string(), "/home/me/a/b".to_string()];
        let tie = |paths: &[&str]| {
            let paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
            tie_to_project(&paths, &roots)
        };
        assert_eq!(
            tie(&["/x", "/home/me/a/b/c.sh"]).as_deref(),
            Some("/home/me/a/b")
        );
        assert_eq!(tie(&["/home/me/a"]).as_deref(), Some("/home/me/a"));
        assert_eq!(tie(&["/home/me/ab/x"]), None);
        assert_eq!(tie(&[]), None);
    }
}
