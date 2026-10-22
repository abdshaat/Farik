//! What the daily reminder reads (`docs/SPEC.md` 4.4, 8.4; ADR 0059): the facts the rules of
//! `catervas_core::reminders` need, from the machine index, from git in the owner's own folders
//! and from Claude Code's folder, and the owner's snoozes and dismissals.

use std::collections::HashMap;
use std::path::Path;

use catervas_core::jobs::JobKind;
use catervas_core::reminders::{
    CronFact, DRAFT_QUIET_DAYS, DeployFact, FORGOTTEN_FROM_DAYS, FORGOTTEN_UNTIL_DAYS, JobFact,
    Mark, MarkKind, ReminderInput, RunFact, SessionFact, cron_interval_secs,
};
use chrono::{DateTime, Duration, FixedOffset, Offset as _, Utc};

use crate::error::StoreError;
use crate::git::{has_uncommitted, unpushed_commits};
use crate::machine::{MachineIndex, read_time, stamp};

/// The most of `mcp-needs-auth-cache.json` that is read.
const AUTH_CACHE_LIMIT: u64 = 1024 * 1024;

/// Where the facts are read from.
pub struct ReminderSources<'a> {
    /// Claude Code's folder.
    pub claude_dir: &'a Path,
    /// The owner's home: git is read in no folder outside it.
    pub home: &'a Path,
    /// Whether GitHub is connected, so that its answers about runs count.
    pub github_connected: bool,
    /// The computer's offset from UTC, in seconds.
    pub utc_offset_secs: i32,
}

/// A scheduled workflow of a repository, as GitHub last said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    /// The workflow's file name.
    pub workflow: String,
    /// The workflow's state, such as `active` or `disabled_inactivity`.
    pub state: Option<String>,
    /// The last scheduled run's conclusion.
    pub last_conclusion: Option<String>,
    /// When the last scheduled run started.
    pub last_run_at: Option<DateTime<Utc>>,
}

/// A repository's latest deployment, as GitHub last said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    /// The environment.
    pub environment: String,
    /// The latest state it reported.
    pub state: String,
    /// When the deployment was made.
    pub created_at: DateTime<Utc>,
}

/// The last part of a path, or the path itself.
fn name_of(path: &str) -> String {
    Path::new(path).file_name().map_or_else(
        || path.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The names of the connectors Claude Code lists in `mcp-needs-auth-cache.json`, sorted. Only the
/// top-level keys are read; a file that is not a regular file, is too large or is not an object
/// reads as none.
#[must_use]
pub fn services_needing_sign_in(claude_dir: &Path) -> Vec<String> {
    let path = claude_dir.join("mcp-needs-auth-cache.json");
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return Vec::new();
    };
    if !meta.is_file() || meta.len() > AUTH_CACHE_LIMIT {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str(&text) else {
        return Vec::new();
    };
    let mut names: Vec<String> = object.keys().cloned().collect();
    names.sort();
    names
}

impl MachineIndex {
    /// The facts the rules read at `now`: no team's, which the teams hold.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn reminder_input(
        &self,
        sources: &ReminderSources<'_>,
        now: DateTime<Utc>,
    ) -> Result<ReminderInput, StoreError> {
        let jobs = self.found_jobs(now)?;
        let sessions = self.reminder_sessions(sources, now, &jobs)?;
        let crons = jobs
            .iter()
            .filter(|job| matches!(job.kind, JobKind::SessionCron | JobKind::SavedSessionCron))
            .filter_map(|job| {
                let root = job.project_root.clone();
                let project = root
                    .as_deref()
                    .or(job.folder.as_deref())
                    .map(name_of)
                    .unwrap_or_default();
                Some(CronFact {
                    id: format!("{}:{}", job.file, job.name),
                    label: job.display_name.clone(),
                    session_id: if job.kind == JobKind::SessionCron {
                        job.file.clone()
                    } else {
                        String::new()
                    },
                    project,
                    project_root: root,
                    folder: job.folder.clone(),
                    expires_at: job.expires_at?,
                    recurring: true,
                })
            })
            .collect();
        let (job_facts, runs_today) = self.reminder_jobs(sources, now, &jobs)?;
        Ok(ReminderInput {
            sessions,
            crons,
            jobs: job_facts,
            deploys: self.reminder_deploys()?,
            teams: Vec::new(),
            services: services_needing_sign_in(sources.claude_dir),
            runs_today,
            marks: self.reminder_marks()?,
            github_connected: sources.github_connected,
            utc_offset_secs: sources.utc_offset_secs,
        })
    }

    /// The owner's sessions that were quiet for a day to two weeks, with what they left undone.
    fn reminder_sessions(
        &self,
        sources: &ReminderSources<'_>,
        now: DateTime<Utc>,
        jobs: &[crate::machine::JobRow],
    ) -> Result<Vec<SessionFact>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT s.session_id, s.title, s.cwd, s.git_branch, s.last_at, s.project_root,
                    p.is_git, p.remote
             FROM sessions s JOIN projects p ON p.root = s.project_root
             WHERE s.place = 'owner' AND p.present = 1 AND s.last_at IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM session_links l
                               WHERE l.session_id = s.session_id AND l.kind = 'copy_of')
             ORDER BY s.last_at DESC, s.session_id",
        )?;
        let mut rows = statement.query([])?;
        let home = sources.home.canonicalize().ok();
        let mut git_read: HashMap<String, (bool, u32)> = HashMap::new();
        let mut sessions = Vec::new();
        while let Some(row) = rows.next()? {
            let last_at = read_time(&row.get::<_, String>(4)?)?;
            let days = (now - last_at).num_days();
            if !(FORGOTTEN_FROM_DAYS..=FORGOTTEN_UNTIL_DAYS).contains(&days) {
                continue;
            }
            let id: String = row.get(0)?;
            let root: String = row.get(5)?;
            let cwd = row
                .get::<_, Option<String>>(2)?
                .unwrap_or_else(|| root.clone());
            let branch: Option<String> = row.get(3)?;
            let is_git = row.get::<_, i64>(6)? != 0;
            let remote: Option<String> = row.get(7)?;
            let (uncommitted, unpushed) = if is_git {
                *git_read
                    .entry(cwd.clone())
                    .or_insert_with(|| git_state(home.as_deref(), &cwd))
            } else {
                (false, 0)
            };
            let title = row
                .get::<_, Option<String>>(1)?
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Untitled session".to_string());
            sessions.push(SessionFact {
                open_todos: self.last_open_todos(&id)?,
                draft_pull_quiet_days: self.draft_pull_quiet_days(
                    remote.as_deref(),
                    branch.as_deref(),
                    now,
                )?,
                waiting_job: jobs.iter().any(|job| {
                    job.kind == JobKind::BackgroundJob
                        && (job.project_root.as_deref() == Some(root.as_str())
                            || job.folder.as_deref() == Some(cwd.as_str()))
                        && job.state.as_deref().is_some_and(|state| {
                            let state = state.to_lowercase();
                            state.contains("wait") || state.contains("input")
                        })
                }),
                id,
                title,
                project: name_of(&root),
                project_root: root,
                folder: cwd,
                branch,
                last_at,
                uncommitted,
                unpushed,
            });
        }
        Ok(sessions)
    }

    /// The count of not completed items of the session's last `TodoWrite`.
    fn last_open_todos(&self, session_id: &str) -> Result<u32, StoreError> {
        let open: Option<i64> = self
            .connection
            .query_row(
                "SELECT tool_calls.open_todos FROM tool_calls
                 JOIN replies ON replies.reply_key = tool_calls.reply_key
                 WHERE replies.session_id = ?1 AND tool_calls.tool_name = 'TodoWrite'
                   AND tool_calls.open_todos IS NOT NULL
                 ORDER BY tool_calls.at DESC, tool_calls.tool_use_id DESC LIMIT 1",
                (session_id,),
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(open.and_then(|open| u32::try_from(open).ok()).unwrap_or(0))
    }

    /// How many days the newest draft pull request Claude made in the session's repository has
    /// been quiet, when the session's branch is one Claude made there and the pull request has been
    /// quiet long enough to say.
    fn draft_pull_quiet_days(
        &self,
        remote: Option<&str>,
        branch: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<i64>, StoreError> {
        let (Some(remote), Some(branch)) = (remote, branch) else {
            return Ok(None);
        };
        let mut statement = self.connection.prepare(
            "SELECT work.kind, work.name, work.draft, work.updated_at
             FROM github_repos repo JOIN github_claude_work work ON work.full_name = repo.full_name
             WHERE repo.remote_key = ?1",
        )?;
        let mut rows = statement.query((remote,))?;
        let (mut has_branch, mut newest): (bool, Option<DateTime<Utc>>) = (false, None);
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            if kind == "branch" {
                has_branch |= row.get::<_, String>(1)? == branch;
            } else if row.get::<_, Option<i64>>(2)? == Some(1)
                && let Some(updated) = row.get::<_, Option<String>>(3)?
                && let Ok(updated) = DateTime::parse_from_rfc3339(&updated)
            {
                let updated = updated.with_timezone(&Utc);
                newest = Some(newest.map_or(updated, |seen| seen.max(updated)));
            }
        }
        let quiet = newest.map(|updated| (now - updated).num_days());
        Ok(quiet.filter(|days| has_branch && *days >= DRAFT_QUIET_DAYS))
    }

    /// The scheduled GitHub Actions jobs of the owner's projects with what GitHub said of their
    /// runs, and today's runs.
    fn reminder_jobs(
        &self,
        sources: &ReminderSources<'_>,
        now: DateTime<Utc>,
        jobs: &[crate::machine::JobRow],
    ) -> Result<(Vec<JobFact>, Vec<RunFact>), StoreError> {
        let offset = FixedOffset::east_opt(sources.utc_offset_secs).unwrap_or_else(|| Utc.fix());
        let today = now.with_timezone(&offset).date_naive();
        let mut facts = Vec::new();
        let mut today_runs = Vec::new();
        for job in jobs.iter().filter(|job| job.kind == JobKind::GithubActions) {
            let Some(root) = job.project_root.as_deref() else {
                continue;
            };
            let workflow = name_of(&job.file);
            let full_name = self.github_repo_of(root)?;
            if sources.github_connected && full_name.is_none() {
                continue;
            }
            let full_name = full_name.unwrap_or_default();
            let run = self.github_run(&full_name, &workflow)?;
            let interval_secs = job
                .schedules
                .iter()
                .filter_map(|schedule| cron_interval_secs(schedule))
                .min();
            let project = name_of(root);
            if let Some(run) = &run
                && run.last_conclusion.as_deref() == Some("success")
                && let Some(at) = run.last_run_at
                && at.with_timezone(&offset).date_naive() == today
            {
                today_runs.push(RunFact {
                    key: format!("{full_name}:{workflow}"),
                    name: job.display_name.clone(),
                    project: project.clone(),
                    at,
                });
            }
            facts.push(JobFact {
                full_name,
                workflow,
                name: job.display_name.clone(),
                project,
                project_root: Some(root.to_string()),
                interval_secs,
                state: run.as_ref().and_then(|run| run.state.clone()),
                last_conclusion: run.as_ref().and_then(|run| run.last_conclusion.clone()),
                last_run_at: run.and_then(|run| run.last_run_at),
            });
        }
        Ok((facts, today_runs))
    }

    /// The repository on GitHub the project at `root` is a copy of, when the index knows both.
    fn github_repo_of(&self, root: &str) -> Result<Option<String>, StoreError> {
        match self.connection.query_row(
            "SELECT repo.full_name FROM projects JOIN github_repos repo
                 ON repo.remote_key = projects.remote
             WHERE projects.root = ?1",
            (root,),
            |row| row.get::<_, String>(0),
        ) {
            Ok(full_name) => Ok(Some(full_name)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn github_run(
        &self,
        full_name: &str,
        workflow: &str,
    ) -> Result<Option<WorkflowRun>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT state, last_conclusion, last_run_at FROM github_runs
             WHERE full_name = ?1 AND workflow = ?2",
        )?;
        let mut rows = statement.query((full_name, workflow))?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(WorkflowRun {
            workflow: workflow.to_string(),
            state: row.get(0)?,
            last_conclusion: row.get(1)?,
            last_run_at: row
                .get::<_, Option<String>>(2)?
                .map(|text| read_time(&text))
                .transpose()?,
        }))
    }

    /// The latest deployment of each repository, with the project it is a copy of.
    fn reminder_deploys(&self) -> Result<Vec<DeployFact>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT deploy.full_name, deploy.environment, deploy.state, deploy.created_at,
                    (SELECT projects.root FROM projects JOIN github_repos repo
                         ON repo.remote_key = projects.remote
                     WHERE repo.full_name = deploy.full_name AND projects.present = 1 LIMIT 1)
             FROM github_deploys deploy
             WHERE deploy.state IS NOT NULL AND deploy.created_at IS NOT NULL
             ORDER BY deploy.full_name",
        )?;
        let mut rows = statement.query([])?;
        let mut deploys = Vec::new();
        while let Some(row) = rows.next()? {
            let full_name: String = row.get(0)?;
            let root: Option<String> = row.get(4)?;
            deploys.push(DeployFact {
                project: root.as_deref().map_or_else(|| name_of(&full_name), name_of),
                project_root: root,
                environment: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                state: row.get(2)?,
                created_at: read_time(&row.get::<_, String>(3)?)?,
                full_name,
            });
        }
        Ok(deploys)
    }

    /// The repositories on GitHub that have a scheduled workflow, each with the files of its
    /// workflows, at most `most` of them, most recently pushed first. What the daily read of
    /// GitHub asks about.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn scheduled_workflows(
        &self,
        most: usize,
    ) -> Result<Vec<(String, Vec<String>)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT repo.full_name, jobs.file FROM jobs
             JOIN projects ON projects.root = jobs.project_root
             JOIN github_repos repo ON repo.remote_key = projects.remote
             WHERE jobs.kind = 'github_actions'
             ORDER BY repo.pushed_at DESC, repo.full_name, jobs.file",
        )?;
        let mut rows = statement.query([])?;
        let mut repositories: Vec<(String, Vec<String>)> = Vec::new();
        while let Some(row) = rows.next()? {
            let (full_name, file): (String, String) = (row.get(0)?, row.get(1)?);
            let workflow = name_of(&file);
            match repositories.iter_mut().find(|(seen, _)| *seen == full_name) {
                Some((_, workflows)) => {
                    if !workflows.contains(&workflow) {
                        workflows.push(workflow);
                    }
                }
                None => repositories.push((full_name, vec![workflow])),
            }
        }
        repositories.truncate(most);
        Ok(repositories)
    }

    /// Keeps what GitHub said of `full_name`: its workflows' runs and its latest deployment,
    /// replacing what was kept of that repository.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn replace_github_state(
        &mut self,
        full_name: &str,
        runs: &[WorkflowRun],
        deploy: Option<&Deployment>,
    ) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM github_runs WHERE full_name = ?1", (full_name,))?;
        for run in runs {
            transaction.execute(
                "INSERT INTO github_runs (full_name, workflow, state, last_conclusion, last_run_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    full_name,
                    &run.workflow,
                    &run.state,
                    &run.last_conclusion,
                    run.last_run_at.map(stamp),
                ),
            )?;
        }
        transaction.execute(
            "DELETE FROM github_deploys WHERE full_name = ?1",
            (full_name,),
        )?;
        if let Some(deploy) = deploy {
            transaction.execute(
                "INSERT INTO github_deploys (full_name, environment, state, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                (
                    full_name,
                    &deploy.environment,
                    &deploy.state,
                    stamp(deploy.created_at),
                ),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Keeps the owner's answer to the item `key`, replacing an earlier one.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_reminder_mark(
        &mut self,
        key: &str,
        kind: &MarkKind,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let (word, until, fingerprint) = match kind {
            MarkKind::Snooze { until } => ("snooze", Some(stamp(*until)), None),
            MarkKind::Dismiss { fingerprint } => ("dismiss", None, Some(fingerprint.as_str())),
        };
        self.connection.execute(
            "INSERT INTO reminder_marks (key, kind, until, fingerprint, at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (key) DO UPDATE SET kind = excluded.kind, until = excluded.until,
                 fingerprint = excluded.fingerprint, at = excluded.at",
            (key, word, until, fingerprint, stamp(now)),
        )?;
        Ok(())
    }

    /// The owner's answers, by key.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn reminder_marks(&self) -> Result<Vec<Mark>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT key, kind, until, fingerprint FROM reminder_marks ORDER BY key")?;
        let mut rows = statement.query([])?;
        let mut marks = Vec::new();
        while let Some(row) = rows.next()? {
            let kind = match (
                row.get::<_, String>(1)?.as_str(),
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ) {
                ("snooze", Some(until), _) => MarkKind::Snooze {
                    until: read_time(&until)?,
                },
                ("dismiss", _, Some(fingerprint)) => MarkKind::Dismiss { fingerprint },
                _ => continue,
            };
            marks.push(Mark {
                key: row.get(0)?,
                kind,
            });
        }
        Ok(marks)
    }

    /// Forgets the answers that no longer hide anything: snoozes whose time has passed.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn forget_old_reminder_marks(&mut self, now: DateTime<Utc>) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM reminder_marks WHERE kind = 'snooze' AND until <= ?1",
            (stamp(now - Duration::days(1)),),
        )?;
        Ok(())
    }
}

/// Whether the folder `cwd` has uncommitted changes and how many commits it has not pushed. Git
/// is read in a folder inside `home` alone, and a folder git cannot read is clean.
fn git_state(home: Option<&Path>, cwd: &str) -> (bool, u32) {
    let Ok(real) = Path::new(cwd).canonicalize() else {
        return (false, 0);
    };
    if !home.is_some_and(|home| real.starts_with(home)) || !real.is_dir() {
        return (false, 0);
    }
    (
        has_uncommitted(&real).unwrap_or(false),
        unpushed_commits(&real).unwrap_or(0),
    )
}
