//! Finding the owner's jobs: asking the computer's schedulers, reading Claude Code's records and,
//! in a repository, the schedules it commits. A command line, an environment value, a prompt or a
//! key is never kept; every file is opened by its canonical path, inside the folder it belongs to.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use catervas_core::jobs::computer::{
    ComputerJob, read_crontab, read_launchd_agent, read_systemd_timer, read_task_scheduler,
    timer_units,
};
use catervas_core::jobs::repository::{jobs_in_documents, jobs_in_terraform, schedule_file_kind};
use catervas_core::jobs::{
    JobKind, cron_expires_at, read_background_job, read_scheduled_tasks, tie_to_project,
};
use chrono::{DateTime, Utc};

use crate::claude_history::HistoryError;
use crate::files::yaml_documents;
use crate::git::{Git, GitError};
use crate::machine::{JobRow, MachineIndex};

/// The largest file a job is read from.
pub const JOB_FILE_LIMIT: u64 = 1_048_576;

/// What a program printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramAnswer {
    /// Whether it exited with success.
    pub succeeded: bool,
    /// What it wrote to its standard output.
    pub stdout: String,
}

/// Runs a program with arguments. `None` when it could not be run or passed the time limit.
pub type RunProgram<'a> = &'a dyn Fn(&str, &[&str]) -> Option<ProgramAnswer>;

/// Where this computer keeps what jobs are read from.
#[derive(Debug, Clone)]
pub struct JobPlaces {
    /// The owner's home, for `~/Library/LaunchAgents` and for expanding `~` in a job's paths.
    pub home: Option<PathBuf>,
    /// Claude Code's folder, holding `jobs/`.
    pub claude_dir: PathBuf,
}

/// What one scan for jobs did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JobsSummary {
    /// Repositories searched for committed schedules.
    pub repositories: u64,
    /// Jobs found in this scan.
    pub jobs: u64,
    /// Places that could not be read.
    pub unreadable: u64,
}

/// What opening a file inside its folder found.
enum Opened {
    Absent,
    Refused,
    Text(String),
}

/// Reads `path` only when its canonical path is `exact`, or inside `inside`, is a regular file and
/// is at most `JOB_FILE_LIMIT` bytes. The canonical path is the one opened.
fn open_inside(path: &Path, inside: &Path, exact: bool) -> Opened {
    let Ok(real) = path.canonicalize() else {
        return if path.symlink_metadata().is_ok() {
            Opened::Refused
        } else {
            Opened::Absent
        };
    };
    let allowed = if exact {
        real == inside
    } else {
        real.starts_with(inside)
    };
    if !allowed || !real.is_file() {
        return Opened::Refused;
    }
    let mut text = String::new();
    let result = std::fs::File::open(&real)
        .and_then(|file| file.take(JOB_FILE_LIMIT + 1).read_to_string(&mut text));
    match result {
        Ok(size) if (size as u64) <= JOB_FILE_LIMIT => Opened::Text(text),
        _ => Opened::Refused,
    }
}

struct Scan<'a> {
    roots: &'a [String],
    home: Option<&'a str>,
    now: DateTime<Utc>,
    answered: Vec<String>,
    searched: BTreeSet<String>,
    rows: Vec<JobRow>,
    unreadable: u64,
}

impl Scan<'_> {
    fn tie(&self, paths: &[String]) -> Option<String> {
        let present: Vec<String> = paths
            .iter()
            .filter_map(|path| Path::new(path).canonicalize().ok())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        tie_to_project(&present, self.roots)
    }

    fn origin(&mut self, origin: &str, answered: bool) {
        self.searched.insert(origin.to_string());
        if answered {
            self.answered.push(origin.to_string());
        }
    }

    fn row(&self, origin: &str, file: &str, name: &str, kind: JobKind, shown: &str) -> JobRow {
        JobRow {
            origin: origin.to_string(),
            file: file.to_string(),
            name: name.to_string(),
            kind,
            display_name: shown.to_string(),
            schedules: Vec::new(),
            folder: None,
            project_root: None,
            expires_at: None,
            state: None,
            state_at: None,
            last_seen: self.now,
        }
    }

    fn computer_rows(&self, kind: JobKind, jobs: Vec<ComputerJob>, tied_only: bool) -> Vec<JobRow> {
        jobs.into_iter()
            .filter_map(|job| {
                let root = self.tie(&job.paths);
                if tied_only && root.is_none() {
                    return None;
                }
                let mut row = self.row(kind.as_wire(), "", &job.unit, kind, &job.name);
                row.schedules = job.schedules;
                row.project_root = root;
                Some(row)
            })
            .collect()
    }

    fn crontab(&mut self, run: RunProgram<'_>) {
        let origin = JobKind::Crontab.as_wire();
        let answer = run("crontab", &["-l"]);
        self.origin(origin, answer.is_some());
        if let Some(answer) = answer.filter(|answer| answer.succeeded) {
            let jobs = read_crontab(&answer.stdout, self.home);
            self.rows
                .extend(self.computer_rows(JobKind::Crontab, jobs, false));
        }
    }

    fn systemd(&mut self, run: RunProgram<'_>, kind: JobKind) {
        let user: &[&str] = if kind == JobKind::SystemdUser {
            &["--user"]
        } else {
            &[]
        };
        let with = |rest: &[&str]| -> Vec<String> {
            user.iter().chain(rest).map(ToString::to_string).collect()
        };
        let call = |rest: &[&str]| {
            let args = with(rest);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            run("systemctl", &args)
        };
        let mut jobs = Vec::new();
        let mut complete = true;
        match call(&["list-timers", "--all", "--no-legend", "--no-pager"]) {
            None => complete = false,
            Some(list) if !list.succeeded => {}
            Some(list) => {
                for (timer, unit) in timer_units(&list.stdout) {
                    match call(&["cat", &timer, &unit]) {
                        None => complete = false,
                        Some(cat) if !cat.succeeded => {}
                        Some(cat) => {
                            jobs.extend(read_systemd_timer(kind, &timer, &cat.stdout, self.home));
                        }
                    }
                }
            }
        }
        self.origin(kind.as_wire(), complete);
        if complete {
            self.rows.extend(self.computer_rows(kind, jobs, false));
        }
    }

    fn launchd(&mut self, run: RunProgram<'_>, places: &JobPlaces) {
        let origin = JobKind::Launchd.as_wire();
        let folder = places
            .home
            .as_ref()
            .map(|home| home.join("Library/LaunchAgents"));
        let mut files: Vec<PathBuf> = Vec::new();
        let mut complete = true;
        if let Some(folder) = folder {
            match std::fs::read_dir(&folder) {
                Ok(entries) => {
                    files = entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.extension()
                                .is_some_and(|extension| extension == "plist")
                                && path.is_file()
                        })
                        .collect();
                    files.sort();
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    complete = false;
                    self.unreadable += 1;
                }
            }
        }
        let mut jobs = Vec::new();
        for file in files {
            let path = file.to_string_lossy().into_owned();
            match run("plutil", &["-convert", "json", "-o", "-", &path]) {
                None => complete = false,
                Some(answer) if !answer.succeeded => self.unreadable += 1,
                Some(answer) => {
                    let stem = file
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    jobs.extend(read_launchd_agent(&stem, &answer.stdout));
                }
            }
        }
        self.origin(origin, complete);
        if complete {
            self.rows
                .extend(self.computer_rows(JobKind::Launchd, jobs, false));
        }
    }

    fn task_scheduler(&mut self, run: RunProgram<'_>) {
        let origin = JobKind::TaskScheduler.as_wire();
        let answer = run("schtasks.exe", &["/query", "/fo", "CSV", "/v"]);
        let Some(answer) = answer else {
            self.origin(origin, false);
            return;
        };
        if !answer.succeeded {
            self.origin(origin, true);
            return;
        }
        if let Some(jobs) = read_task_scheduler(&answer.stdout) {
            self.origin(origin, true);
            self.rows
                .extend(self.computer_rows(JobKind::TaskScheduler, jobs, true));
        } else {
            self.origin(origin, false);
            self.unreadable += 1;
        }
    }

    /// Reads the schedules a repository commits. Files come from `ls-files`, so an ignored or
    /// untracked file is not seen.
    fn repository(&mut self, root: &str) -> Result<(), HistoryError> {
        let tracked = match Git::open(PathBuf::from(root)).tracked_paths() {
            Ok(tracked) => tracked,
            Err(error @ GitError::NotInstalled { .. }) => return Err(error.into()),
            Err(_) => {
                self.origin(root, false);
                self.unreadable += 1;
                return Ok(());
            }
        };
        self.origin(root, true);
        let Ok(real) = Path::new(root).canonicalize() else {
            return Ok(());
        };
        for path in tracked {
            let Some(kind) = schedule_file_kind(&path) else {
                continue;
            };
            let text = match open_inside(&real.join(&path), &real, false) {
                Opened::Absent => continue,
                Opened::Refused => {
                    self.unreadable += 1;
                    continue;
                }
                Opened::Text(text) => text,
            };
            if kind == JobKind::Kubernetes && !text.contains("CronJob") {
                continue;
            }
            let found = if kind == JobKind::Terraform {
                Some(jobs_in_terraform(&text))
            } else {
                let documents = if kind == JobKind::Vercel {
                    serde_json::from_str(&text)
                        .map(|document| vec![document])
                        .ok()
                } else {
                    yaml_documents(&text, &path).ok()
                };
                documents.map(|documents| jobs_in_documents(kind, &path, &documents))
            };
            let Some(found) = found else {
                self.unreadable += 1;
                continue;
            };
            for job in found {
                let mut row = self.row(root, &path, &job.name, job.kind, &job.name);
                row.schedules = job.schedules;
                row.project_root = Some(root.to_string());
                self.rows.push(row);
            }
        }
        Ok(())
    }

    fn saved_crons(&mut self, folders: &BTreeSet<String>) {
        for folder in folders {
            let origin = format!("claude_tasks:{folder}");
            let expected = Path::new(folder).join(".claude/scheduled_tasks.json");
            let file = ".claude/scheduled_tasks.json";
            let tasks = match open_inside(&expected, &expected, true) {
                Opened::Absent => {
                    self.origin(&origin, true);
                    continue;
                }
                Opened::Refused => None,
                Opened::Text(text) => read_scheduled_tasks(&text),
            };
            let Some(tasks) = tasks else {
                self.origin(&origin, false);
                self.unreadable += 1;
                continue;
            };
            self.origin(&origin, true);
            let root = tie_to_project(std::slice::from_ref(folder), self.roots);
            for task in tasks {
                let mut row = self.row(
                    &origin,
                    file,
                    &task.id,
                    JobKind::SavedSessionCron,
                    &format!("Session cron {}", task.cron),
                );
                row.schedules = vec![task.cron];
                row.folder = Some(folder.clone());
                row.project_root.clone_from(&root);
                row.expires_at =
                    cron_expires_at(task.created_at, task.is_recurring, task.is_permanent);
                row.state_at = Some(task.created_at);
                self.rows.push(row);
            }
        }
    }

    fn background_jobs(&mut self, claude_dir: &Path) {
        self.origin("claude_jobs", true);
        let Ok(jobs_dir) = claude_dir.join("jobs").canonicalize() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&jobs_dir) else {
            return;
        };
        let mut folders: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        for folder in folders {
            let name = folder
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let job = match open_inside(&folder.join("state.json"), &jobs_dir, false) {
                Opened::Absent => continue,
                Opened::Refused => None,
                Opened::Text(text) => read_background_job(&text),
            };
            let Some(job) = job else {
                self.unreadable += 1;
                continue;
            };
            let mut row = self.row("claude_jobs", "", &name, JobKind::BackgroundJob, &job.name);
            row.project_root = job
                .cwd
                .as_ref()
                .and_then(|cwd| self.tie(std::slice::from_ref(cwd)));
            row.folder = job.cwd;
            row.state = Some(job.state);
            row.state_at = job.updated_at;
            self.rows.push(row);
        }
    }
}

/// Finds the owner's recurring jobs in this computer's schedulers and in Claude Code's records,
/// and keeps them in the index. A source's rows are replaced only when it answered; one that did
/// not keeps its last-known rows.
///
/// # Errors
///
/// `Store` when the index refuses.
pub fn find_jobs(
    index: &mut MachineIndex,
    places: &JobPlaces,
    run: RunProgram<'_>,
    now: DateTime<Utc>,
) -> Result<JobsSummary, HistoryError> {
    let search = index.job_search()?;
    let home = places
        .home
        .as_ref()
        .map(|home| home.to_string_lossy().into_owned());
    let mut scan = Scan {
        roots: &search.roots,
        home: home.as_deref(),
        now,
        answered: Vec::new(),
        searched: BTreeSet::new(),
        rows: Vec::new(),
        unreadable: 0,
    };
    for repository in &search.repositories {
        scan.repository(repository)?;
    }
    scan.crontab(run);
    scan.systemd(run, JobKind::SystemdUser);
    scan.systemd(run, JobKind::SystemdSystem);
    scan.launchd(run, places);
    scan.task_scheduler(run);

    let mut folders = BTreeSet::new();
    for folder in search.roots.iter().chain(&search.folders) {
        if let Ok(real) = Path::new(folder).canonicalize()
            && real.is_dir()
        {
            folders.insert(real.to_string_lossy().into_owned());
        }
    }
    scan.saved_crons(&folders);
    scan.background_jobs(&places.claude_dir);

    for stored in index.stored_job_origins()? {
        if !scan.searched.contains(&stored) {
            scan.answered.push(stored);
        }
    }
    index.replace_found_jobs(&scan.answered, &scan.rows, now)?;
    Ok(JobsSummary {
        repositories: search.repositories.len() as u64,
        jobs: scan.rows.len() as u64,
        unreadable: scan.unreadable,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use catervas_core::jobs::fixtures::{
        BACKGROUND_JOB, CRONTAB, SCHEDULED_TASKS, SCHTASKS_CSV, SYSTEMCTL_CAT_DB_BACKUP,
        SYSTEMCTL_CAT_LOGROTATE, SYSTEMCTL_CAT_SHOP_SYNC, SYSTEMCTL_TIMERS_SYSTEM,
        SYSTEMCTL_TIMERS_USER,
    };
    use chrono::{DateTime, TimeZone, Utc};

    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::machine::{JobRow, MACHINE_DB, MachineIndex, open_machine_index};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    /// A temporary home with `shop` as a root holding `bin/sync.sh` and `bin/sync`.
    struct Home {
        _temp: TempProject,
        path: PathBuf,
        db: PathBuf,
        index: MachineIndex,
    }

    impl Home {
        fn new(name: &str) -> Self {
            let temp = TempProject::new(name);
            let home = temp.root.canonicalize().expect("a home").join("home");
            std::fs::create_dir_all(home.join("shop/bin")).expect("shop");
            for file in ["sync.sh", "sync"] {
                std::fs::write(home.join("shop/bin").join(file), "x").expect("a script");
            }
            let db = temp.root.join("state").join(MACHINE_DB);
            let index = open_machine_index(&db, now()).expect("the index opens");
            index
                .connection
                .execute(
                    "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at)
                     VALUES (?1, 0, 'yours', 1, '2026-10-12T09:00:00.000Z', '2026-10-12T09:00:00.000Z')",
                    (home.join("shop").to_string_lossy(),),
                )
                .expect("a root");
            Self {
                _temp: temp,
                path: home,
                db,
                index,
            }
        }

        fn mine(&self, text: &str) -> String {
            let home = self.path.to_string_lossy().into_owned();
            text.replace(r"\home\me", &home.replace('/', "\\"))
                .replace("/home/me", &home)
        }

        fn scan(&mut self, answers: &HashMap<String, String>) -> JobsSummary {
            let places = JobPlaces {
                home: Some(self.path.clone()),
                claude_dir: self.path.join(".claude"),
            };
            let run = |program: &str, args: &[&str]| {
                answers
                    .get(&format!("{program} {}", args.join(" ")))
                    .map(|stdout| ProgramAnswer {
                        succeeded: true,
                        stdout: stdout.clone(),
                    })
            };
            find_jobs(&mut self.index, &places, &run, now()).expect("the scan runs")
        }

        fn answers(&self) -> HashMap<String, String> {
            let mut answers = HashMap::new();
            let mut put = |command: &str, fixture: &str| {
                answers.insert(command.to_string(), self.mine(fixture));
            };
            put("crontab -l", CRONTAB);
            put(
                "systemctl list-timers --all --no-legend --no-pager",
                SYSTEMCTL_TIMERS_SYSTEM,
            );
            put(
                "systemctl --user list-timers --all --no-legend --no-pager",
                SYSTEMCTL_TIMERS_USER,
            );
            put(
                "systemctl cat logrotate.timer logrotate.service",
                SYSTEMCTL_CAT_LOGROTATE,
            );
            put(
                "systemctl cat db-backup.timer db-backup.service",
                SYSTEMCTL_CAT_DB_BACKUP,
            );
            put(
                "systemctl --user cat shop-sync.timer shop-sync.service",
                SYSTEMCTL_CAT_SHOP_SYNC,
            );
            put("schtasks.exe /query /fo CSV /v", SCHTASKS_CSV);
            answers
        }

        fn jobs(&self) -> Vec<JobRow> {
            self.index.found_jobs(now()).expect("jobs")
        }
    }

    fn shown(jobs: &[JobRow], home: &Path) -> Vec<(String, String, bool)> {
        let shop = home.join("shop").to_string_lossy().into_owned();
        jobs.iter()
            .map(|job| {
                (
                    job.kind.as_wire().to_string(),
                    job.display_name.clone(),
                    job.project_root.as_deref() == Some(shop.as_str()),
                )
            })
            .collect()
    }

    fn row(kind: &str, name: &str, tied: bool) -> (String, String, bool) {
        (kind.to_string(), name.to_string(), tied)
    }

    #[test]
    fn finds_the_computers_jobs() {
        let mut home = Home::new("finds-computers-jobs");
        let answers = home.answers();
        let summary = home.scan(&answers);
        assert_eq!((summary.jobs, summary.unreadable), (7, 0));
        assert_eq!(
            shown(&home.jobs(), &home.path),
            vec![
                row("crontab", "backup-all", false),
                row("crontab", "backup.sh", false),
                row("crontab", "publish.sh", false),
                row("crontab", "sync.sh", true),
                row("systemd_user", "shop-sync", true),
                row("systemd_system", "db-backup", false),
                row("task_scheduler", "\\ShopSync", true),
            ]
        );
    }

    /// Guard: checked by mutation (keep a command line, an argument or an environment value).
    #[test]
    fn never_keeps_a_token_from_a_scheduler() {
        let mut home = Home::new("never-keeps-scheduler-token");
        let answers = home.answers();
        home.scan(&answers);
        let bytes = std::fs::read(&home.db).expect("the index file");
        assert!(!bytes.windows(6).any(|window| window == b"SECRET"));
    }

    #[test]
    fn finds_claude_codes_saved_crons_and_background_jobs() {
        let mut home = Home::new("finds-claude-code-jobs");
        let claude = home.path.join(".claude");
        std::fs::create_dir_all(home.path.join("shop/.claude")).expect("a folder");
        std::fs::write(
            home.path.join("shop/.claude/scheduled_tasks.json"),
            SCHEDULED_TASKS,
        )
        .expect("tasks");
        for (folder, text) in [
            ("abc12345", home.mine(BACKGROUND_JOB)),
            ("def", "not json".to_string()),
        ] {
            std::fs::create_dir_all(claude.join("jobs").join(folder)).expect("a job folder");
            std::fs::write(claude.join("jobs").join(folder).join("state.json"), text)
                .expect("a state");
        }
        let summary = home.scan(&HashMap::new());
        assert_eq!(summary.unreadable, 1);
        let jobs = home.jobs();
        assert_eq!(
            shown(&jobs, &home.path),
            vec![
                row("saved_session_cron", "Session cron 0 8 * * *", true),
                row("saved_session_cron", "Session cron 0 9 * * 1-5", true),
                row("saved_session_cron", "Session cron 30 14 28 2 *", true),
                row("background_job", "shop-cleanup", true),
            ]
        );
        let origin = format!("claude_tasks:{}", home.path.join("shop").to_string_lossy());
        assert!(jobs.iter().take(3).all(|job| job.origin == origin));
        let expiring: Vec<Option<String>> = jobs
            .iter()
            .take(3)
            .map(|job| job.expires_at.map(|at| at.to_rfc3339()))
            .collect();
        // Ordered by name: "Session cron 0 8", "0 9", "30 14".
        assert_eq!(
            expiring,
            vec![None, Some("2026-10-19T09:00:00+00:00".to_string()), None]
        );
        let background = &jobs[3];
        assert_eq!(background.name, "abc12345");
        assert_eq!(background.state.as_deref(), Some("blocked"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_whatever_is_not_a_job_file() {
        use std::os::unix::fs::symlink;

        let mut home = Home::new("refuses-not-job-files");
        let claude = home.path.join(".claude");
        std::fs::create_dir_all(claude.join("jobs/evil")).expect("folders");
        std::fs::create_dir_all(claude.join("jobs/x")).expect("folders");
        std::fs::write(
            claude.join(".credentials.json"),
            r#"{"name":"SECRET-MARKER","state":"s"}"#,
        )
        .expect("credentials");
        symlink(
            claude.join(".credentials.json"),
            claude.join("jobs/evil/state.json"),
        )
        .expect("a link");
        std::fs::write(claude.join("jobs/x/timeline.jsonl"), "SECRET-MARKER").expect("a timeline");
        let outside = home.path.join("outside.json");
        std::fs::write(
            &outside,
            r#"{"tasks":[{"id":"SECRET-MARKER","cron":"* * * * *","createdAt":1791795600000}]}"#,
        )
        .expect("an outside file");
        std::fs::create_dir_all(home.path.join("shop/.claude")).expect("a folder");
        symlink(
            &outside,
            home.path.join("shop/.claude/scheduled_tasks.json"),
        )
        .expect("a link");
        home.scan(&HashMap::new());
        assert!(home.jobs().is_empty());
        let bytes = std::fs::read(&home.db).expect("the index file");
        assert!(!bytes.windows(13).any(|window| window == b"SECRET-MARKER"));
    }
}
