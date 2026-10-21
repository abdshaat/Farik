//! The machine-level index of the owner's Claude Code history, apart from every project's log.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use catervas_core::claude_history::ToolUse;
use catervas_core::claude_services::{ConfiguredServer, ServiceSource};
use catervas_core::jobs::{JobKind, SESSION_CRON_DAYS};
use catervas_core::pricing::Usage;
use chrono::{DateTime, Duration as Days, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::error::StoreError;
use crate::migrations;

/// The index's file name in Catervas's state folder.
pub const MACHINE_DB: &str = "machine.db";

const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// The open index. It has one writer at a time, so it uses SQLite's default rollback journal.
pub struct MachineIndex {
    pub(crate) connection: Connection,
}

/// Opens the index at `path`, making its folder and the file when they are not there, and brings it
/// to the shape this version expects.
///
/// # Errors
///
/// `Io` when the folder cannot be made; `Sqlite` when the file cannot be opened or migrated.
pub fn open_machine_index(path: &Path, now: DateTime<Utc>) -> Result<MachineIndex, StoreError> {
    if let Some(directory) = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    {
        std::fs::create_dir_all(directory)?;
    }
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrations::apply_machine(&mut connection, now)?;
    Ok(MachineIndex { connection })
}

/// Deletes the index and its rollback journal, opening and reading nothing. `true` when there was an
/// index to delete.
///
/// # Errors
///
/// `Io` when a file is there and cannot be removed.
pub fn forget_machine_index(path: &Path) -> Result<bool, StoreError> {
    let mut journal = path.as_os_str().to_owned();
    journal.push("-journal");
    let mut existed = false;
    for file in [PathBuf::from(path), PathBuf::from(journal)] {
        match std::fs::remove_file(&file) {
            Ok(()) => existed = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(existed)
}

/// How far one transcript file has been read, and what was counted in it.
pub(crate) struct FileState {
    pub read_to: u64,
    pub turns: u64,
    pub unreadable_lines: u64,
}

/// One session as one file reports it: the earliest turn's folder and the span of its turns.
pub(crate) struct SessionBatch {
    pub session_id: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    pub entrypoint: Option<String>,
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
}

/// One reply as one file reports it, its counters already the largest the file shows.
pub(crate) struct ReplyBatch {
    pub key: String,
    pub session_id: String,
    pub model: String,
    pub at: DateTime<Utc>,
    pub usage: Usage,
    /// The tool calls of every line of the reply, once each by tool-use id.
    pub tools: Vec<ToolUse>,
}

/// A non-durable `CronCreate` call, as the line that carries it reports it.
pub(crate) struct SessionCronBatch {
    pub session_id: String,
    pub tool_use_id: String,
    pub cwd: String,
    pub cron: String,
    pub at: DateTime<Utc>,
}

/// A connector a settings file names, and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredRow {
    /// Whose settings.
    pub source: ServiceSource,
    /// `''` for the user level and claude.ai, else a project path, `plugin:<id>`, a Claude Desktop
    /// folder or `extension:<folder name>`.
    pub found_in: String,
    /// What was kept of the server.
    pub server: ConfiguredServer,
}

/// A tool call with a server or on a spreadsheet, as the line that carries it reports it.
pub(crate) struct ToolCallBatch {
    pub tool_use_id: String,
    pub reply_key: String,
    pub tool_name: String,
    pub touches_spreadsheet: bool,
    pub at: DateTime<Utc>,
}

/// Everything one file adds to the index, written in one transaction.
pub(crate) struct FileRead<'a> {
    pub path: &'a str,
    pub state: FileState,
    pub sessions: Vec<SessionBatch>,
    pub titles: Vec<(String, String)>,
    pub replies: Vec<ReplyBatch>,
    pub crons: Vec<SessionCronBatch>,
    pub tool_calls: Vec<ToolCallBatch>,
}

/// One recurring job, as `machine.db` keeps it. Its key is `(origin, file, name)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    /// Where it was found: a repository's root, the scheduler's wire value, `claude_session`,
    /// `claude_tasks:<folder>` or `claude_jobs`.
    pub origin: String,
    /// A repository job's file relative to its root; a session cron's session id; else empty.
    pub file: String,
    /// The unit, the task's id, the tool use's id or the folder's name.
    pub name: String,
    /// Where the job lives.
    pub kind: JobKind,
    /// The name shown.
    pub display_name: String,
    /// Its schedules as written, never interpreted.
    pub schedules: Vec<String>,
    /// The folder it works in, for a session cron, a saved cron and a background job.
    pub folder: Option<String>,
    /// The project it belongs to.
    pub project_root: Option<String>,
    /// When a session cron or saved cron expires.
    pub expires_at: Option<DateTime<Utc>>,
    /// A background job's state.
    pub state: Option<String>,
    /// When the state was set: a call's time, a task's creation, a job's last update.
    pub state_at: Option<DateTime<Utc>>,
    /// When the row was last confirmed.
    pub last_seen: DateTime<Utc>,
}

/// Where jobs are looked for, from the projects and sessions the index holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobSearch {
    /// The roots of the owner's git repositories still on this computer.
    pub repositories: Vec<String>,
    /// Every root still on this computer, the owner's and explored.
    pub roots: Vec<String>,
    /// The folders sessions worked in.
    pub folders: Vec<String>,
}

/// Whether the owner made a repository or is only looking at someone else's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// The owner's own work, or a repository with nobody's commits yet.
    Yours,
    /// A repository with commits, none of them by the owner.
    Explored,
}

impl Ownership {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Explored => "explored",
        }
    }

    fn from_wire(wire: &str) -> Self {
        if wire == "explored" {
            Self::Explored
        } else {
            Self::Yours
        }
    }
}

/// What a set of sessions did: how many, and the tokens of their replies by model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Activity {
    /// How many sessions.
    pub sessions: u64,
    /// The tokens their replies used, by model.
    pub tokens: BTreeMap<String, Usage>,
}

/// A project on this computer that sessions were placed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundProject {
    /// The root folder, canonical.
    pub root: String,
    /// Whether the root is a git repository.
    pub is_git: bool,
    /// Whether the repository is the owner's.
    pub ownership: Ownership,
    /// When any session in it last did anything.
    pub last_at: DateTime<Utc>,
    /// The owner's own sessions.
    pub own: Activity,
    /// Catervas's agents' sessions.
    pub catervas: Activity,
}

/// A folder that is no longer on this computer, with everything that worked in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoneFolder {
    /// The folder.
    pub folder: String,
    /// When any session in it last did anything.
    pub last_at: DateTime<Utc>,
    /// The owner's and Catervas's sessions together.
    pub activity: Activity,
}

/// What the index holds, grouped for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Roots still on this computer with sessions placed under them, newest first.
    pub projects: Vec<FoundProject>,
    /// Folders no longer on this computer, newest first.
    pub gone: Vec<GoneFolder>,
    /// Sessions in home or a temporary folder.
    pub not_a_project: Activity,
    /// Transcript files with lines but no turn: a version of Claude Code this one cannot read.
    pub unreadable_sessions: u64,
    /// Lines in all transcript files that could not be read.
    pub unreadable_lines: u64,
}

/// A root as the index remembers it.
pub(crate) struct StoredRoot {
    pub root: String,
    pub is_git: bool,
    pub ownership: Ownership,
}

/// What a scan found out about a root.
pub(crate) struct ProjectRow {
    pub root: String,
    pub is_git: bool,
    pub ownership: Ownership,
    pub present: bool,
}

/// Where one session was placed.
pub(crate) struct Placement {
    pub session_id: String,
    pub place: &'static str,
    pub root: Option<String>,
}

fn add(activity: &mut Activity, tokens: Option<&BTreeMap<String, Usage>>) {
    activity.sessions += 1;
    for (model, usage) in tokens.into_iter().flatten() {
        let total = activity.tokens.entry(model.clone()).or_default();
        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        total.cache_read_tokens += usage.cache_read_tokens;
        total.cache_write_tokens += usage.cache_write_tokens;
    }
}

fn read_time(text: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(text)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| StoreError::Sqlite {
            detail: format!("the index holds a time {text:?} that is not a time: {error}"),
        })
}

fn natural(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// A count as SQLite keeps it; no transcript has more tokens than an `i64` holds.
fn int(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// A time as the index keeps it: fixed width, so that text order is time order.
fn stamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

// A session's turn fields are those of its earliest turn: the folder it started in.
const EARLIER: &str = "excluded.first_at IS NOT NULL AND (sessions.first_at IS NULL OR excluded.first_at < sessions.first_at)";

/// Writes a file's session crons and tool calls inside its transaction.
fn write_crons_and_calls(
    transaction: &rusqlite::Transaction<'_>,
    file: &FileRead<'_>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    for cron in &file.crons {
        // A session-only cron is gone with its session, and at the latest `SESSION_CRON_DAYS`
        // after the call. Whether the session still runs cannot be known.
        let schedules =
            serde_json::to_string(&[&cron.cron]).map_err(|error| StoreError::Sqlite {
                detail: error.to_string(),
            })?;
        transaction.execute(
            "INSERT INTO jobs (origin, file, name, kind, display_name, schedules, folder, expires_at,
                               state_at, last_seen)
             VALUES ('claude_session', ?1, ?2, 'session_cron', ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (origin, file, name) DO UPDATE SET
                 display_name = excluded.display_name, schedules = excluded.schedules,
                 folder = excluded.folder, expires_at = excluded.expires_at,
                 state_at = excluded.state_at, last_seen = excluded.last_seen",
            (
                &cron.session_id,
                &cron.tool_use_id,
                format!("Session cron {}", cron.cron),
                schedules,
                &cron.cwd,
                stamp(cron.at + Days::days(SESSION_CRON_DAYS)),
                stamp(cron.at),
                stamp(now),
            ),
        )?;
    }
    for call in &file.tool_calls {
        // The first row of an id stays: a resumed session repeats the line.
        transaction.execute(
            "INSERT OR IGNORE INTO tool_calls (tool_use_id, reply_key, tool_name, touches_spreadsheet, at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &call.tool_use_id,
                &call.reply_key,
                &call.tool_name,
                i64::from(call.touches_spreadsheet),
                stamp(call.at),
            ),
        )?;
    }
    Ok(())
}

impl MachineIndex {
    pub(crate) fn file_state(&self, path: &str) -> Result<Option<FileState>, StoreError> {
        let state = self
            .connection
            .query_row(
                "SELECT read_to, turns, unreadable_lines FROM transcript_files WHERE path = ?1",
                (path,),
                |row| {
                    let count = |column| -> rusqlite::Result<u64> {
                        Ok(u64::try_from(row.get::<_, i64>(column)?).unwrap_or(0))
                    };
                    Ok(FileState {
                        read_to: count(0)?,
                        turns: count(1)?,
                        unreadable_lines: count(2)?,
                    })
                },
            )
            .optional()?;
        Ok(state)
    }

    pub(crate) fn session_count(&self) -> Result<u64, StoreError> {
        let count: i64 = self
            .connection
            .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Writes one file's sessions, titles, replies, tool calls and its new offset, all or none.
    pub(crate) fn record_file(
        &mut self,
        file: &FileRead<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for session in &file.sessions {
            transaction.execute(
                &format!(
                    "INSERT INTO sessions (session_id, cwd, git_branch, version, entrypoint, first_at, last_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (session_id) DO UPDATE SET
                         cwd = CASE WHEN {EARLIER} THEN excluded.cwd ELSE sessions.cwd END,
                         git_branch = CASE WHEN {EARLIER} THEN excluded.git_branch ELSE sessions.git_branch END,
                         version = CASE WHEN {EARLIER} THEN excluded.version ELSE sessions.version END,
                         entrypoint = CASE WHEN {EARLIER} THEN excluded.entrypoint ELSE sessions.entrypoint END,
                         last_at = CASE WHEN sessions.last_at IS NULL OR excluded.last_at > sessions.last_at
                                        THEN excluded.last_at ELSE sessions.last_at END,
                         first_at = CASE WHEN {EARLIER} THEN excluded.first_at ELSE sessions.first_at END"
                ),
                (
                    &session.session_id,
                    &session.cwd,
                    &session.git_branch,
                    &session.version,
                    &session.entrypoint,
                    stamp(session.first_at),
                    stamp(session.last_at),
                ),
            )?;
        }
        for (session_id, title) in &file.titles {
            transaction.execute(
                "UPDATE sessions SET title = ?2 WHERE session_id = ?1",
                (session_id, title),
            )?;
        }
        for reply in &file.replies {
            // The session that recorded a reply is the one that started first, ties to the smaller
            // id, whichever file is read first.
            transaction.execute(
                "INSERT INTO replies (reply_key, session_id, model, at, input_tokens, output_tokens,
                                      cache_read_tokens, cache_write_tokens)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (reply_key) DO UPDATE SET
                     session_id = CASE WHEN
                         (SELECT first_at FROM sessions WHERE session_id = excluded.session_id) || ' ' || excluded.session_id
                         < (SELECT first_at FROM sessions WHERE session_id = replies.session_id) || ' ' || replies.session_id
                         THEN excluded.session_id ELSE replies.session_id END,
                     at = min(replies.at, excluded.at),
                     input_tokens = max(replies.input_tokens, excluded.input_tokens),
                     output_tokens = max(replies.output_tokens, excluded.output_tokens),
                     cache_read_tokens = max(replies.cache_read_tokens, excluded.cache_read_tokens),
                     cache_write_tokens = max(replies.cache_write_tokens, excluded.cache_write_tokens)",
                (
                    &reply.key,
                    &reply.session_id,
                    &reply.model,
                    stamp(reply.at),
                    int(reply.usage.input_tokens),
                    int(reply.usage.output_tokens),
                    int(reply.usage.cache_read_tokens),
                    int(reply.usage.cache_write_tokens),
                ),
            )?;
        }
        write_crons_and_calls(&transaction, file, now)?;
        transaction.execute(
            "INSERT OR REPLACE INTO transcript_files (path, read_to, turns, unreadable_lines, read_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                file.path,
                int(file.state.read_to),
                int(file.state.turns),
                int(file.state.unreadable_lines),
                stamp(now),
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every session with a folder, as its id and that folder.
    pub(crate) fn session_folders(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, cwd FROM sessions WHERE cwd IS NOT NULL ORDER BY session_id",
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Every root any scan has placed a session under.
    pub(crate) fn stored_roots(&self) -> Result<Vec<StoredRoot>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT root, is_git, ownership FROM projects ORDER BY root")?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredRoot {
                    root: row.get(0)?,
                    is_git: row.get::<_, i64>(1)? != 0,
                    ownership: Ownership::from_wire(&row.get::<_, String>(2)?),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Writes where every session was placed and what each root is, all or none.
    pub(crate) fn write_placements(
        &mut self,
        placements: &[Placement],
        projects: &[ProjectRow],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for placement in placements {
            transaction.execute(
                "UPDATE sessions SET place = ?2, project_root = ?3 WHERE session_id = ?1",
                (&placement.session_id, placement.place, &placement.root),
            )?;
        }
        for project in projects {
            transaction.execute(
                "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT (root) DO UPDATE SET
                     is_git = excluded.is_git, ownership = excluded.ownership,
                     present = excluded.present, checked_at = excluded.checked_at",
                (
                    &project.root,
                    i64::from(project.is_git),
                    project.ownership.as_wire(),
                    i64::from(project.present),
                    stamp(now),
                ),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Where jobs are looked for.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn job_search(&self) -> Result<JobSearch, StoreError> {
        let strings = |sql: &str| -> Result<Vec<String>, StoreError> {
            let mut statement = self.connection.prepare(sql)?;
            let rows = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        };
        Ok(JobSearch {
            repositories: strings(
                "SELECT root FROM projects WHERE present = 1 AND is_git = 1 AND ownership = 'yours' ORDER BY root",
            )?,
            roots: strings("SELECT root FROM projects WHERE present = 1 ORDER BY root")?,
            folders: strings(
                "SELECT DISTINCT cwd FROM sessions WHERE cwd IS NOT NULL AND place IN ('owner', 'catervas') ORDER BY cwd",
            )?,
        })
    }

    /// Replaces every configured server with `rows`, all or none. The first row of a key stays.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn replace_configured_servers(&mut self, rows: &[ConfiguredRow]) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM configured_servers", [])?;
        for row in rows {
            transaction.execute(
                "INSERT OR IGNORE INTO configured_servers (source, found_in, name, kind, host, program)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    row.source.as_wire(),
                    &row.found_in,
                    &row.server.name,
                    row.server.kind.as_wire(),
                    &row.server.host,
                    &row.server.program,
                ),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// The origins of every stored job except session crons.
    pub(crate) fn stored_job_origins(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT origin FROM jobs WHERE origin != 'claude_session' ORDER BY origin",
        )?;
        let rows = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Replaces the rows of every origin in `answered` with `jobs`, all or none. A source that did
    /// not answer keeps its rows and their `last_seen`. Session crons are never replaced; their
    /// `project_root` is set again from `sessions`. A key given twice is one row, its schedules
    /// joined in order with duplicates dropped.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn replace_found_jobs(
        &mut self,
        answered: &[String],
        jobs: &[JobRow],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let mut merged: Vec<JobRow> = Vec::new();
        for job in jobs {
            let key = (&job.origin, &job.file, &job.name);
            if let Some(seen) = merged
                .iter_mut()
                .find(|seen| (&seen.origin, &seen.file, &seen.name) == key)
            {
                for schedule in &job.schedules {
                    if !seen.schedules.contains(schedule) {
                        seen.schedules.push(schedule.clone());
                    }
                }
            } else {
                merged.push(job.clone());
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for origin in answered.iter().filter(|origin| *origin != "claude_session") {
            transaction.execute("DELETE FROM jobs WHERE origin = ?1", (origin,))?;
        }
        for job in &merged {
            let schedules =
                serde_json::to_string(&job.schedules).map_err(|error| StoreError::Sqlite {
                    detail: error.to_string(),
                })?;
            transaction.execute(
                "INSERT OR REPLACE INTO jobs (origin, file, name, kind, display_name, schedules, folder,
                                              project_root, expires_at, state, state_at, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                (
                    &job.origin,
                    &job.file,
                    &job.name,
                    job.kind.as_wire(),
                    &job.display_name,
                    schedules,
                    &job.folder,
                    &job.project_root,
                    job.expires_at.map(stamp),
                    &job.state,
                    job.state_at.map(stamp),
                    stamp(now),
                ),
            )?;
        }
        transaction.execute(
            "UPDATE jobs SET project_root = (
                 SELECT sessions.project_root FROM sessions
                 WHERE sessions.session_id = jobs.file AND sessions.place IN ('owner', 'catervas'))
             WHERE origin = 'claude_session'",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every job, by kind, then name, then file, except a session cron that expired more than
    /// `SESSION_CRON_DAYS` before `now`.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read or holds a value that is not one.
    pub fn found_jobs(&self, now: DateTime<Utc>) -> Result<Vec<JobRow>, StoreError> {
        let odd = |what: &str| StoreError::Sqlite {
            detail: format!("the index holds a job with a bad {what}"),
        };
        let mut statement = self.connection.prepare(
            "SELECT origin, file, name, kind, display_name, schedules, folder, project_root,
                    expires_at, state, state_at, last_seen FROM jobs",
        )?;
        let mut rows = statement.query([])?;
        let mut jobs = Vec::new();
        while let Some(row) = rows.next()? {
            let time = |column: usize| -> Result<Option<DateTime<Utc>>, StoreError> {
                row.get::<_, Option<String>>(column)?
                    .map(|text| read_time(&text))
                    .transpose()
            };
            let job = JobRow {
                origin: row.get(0)?,
                file: row.get(1)?,
                name: row.get(2)?,
                kind: JobKind::from_wire(&row.get::<_, String>(3)?).ok_or_else(|| odd("kind"))?,
                display_name: row.get(4)?,
                schedules: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(|_| odd("schedule list"))?,
                folder: row.get(6)?,
                project_root: row.get(7)?,
                expires_at: time(8)?,
                state: row.get(9)?,
                state_at: time(10)?,
                last_seen: read_time(&row.get::<_, String>(11)?)?,
            };
            let stale = job.kind == JobKind::SessionCron
                && job
                    .expires_at
                    .is_some_and(|at| at < now - Days::days(SESSION_CRON_DAYS));
            if !stale {
                jobs.push(job);
            }
        }
        jobs.sort_by(|a, b| {
            (a.kind, &a.display_name, &a.file, &a.origin, &a.name).cmp(&(
                b.kind,
                &b.display_name,
                &b.file,
                &b.origin,
                &b.name,
            ))
        });
        Ok(jobs)
    }

    /// Each session's reply tokens, by model.
    fn reply_tokens(&self) -> Result<HashMap<String, BTreeMap<String, Usage>>, StoreError> {
        let mut tokens: HashMap<String, BTreeMap<String, Usage>> = HashMap::new();
        let mut statement = self.connection.prepare(
            "SELECT session_id, model, SUM(input_tokens), SUM(output_tokens),
                    SUM(cache_read_tokens), SUM(cache_write_tokens)
             FROM replies GROUP BY session_id, model",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let usage = Usage {
                input_tokens: natural(row.get(2)?),
                output_tokens: natural(row.get(3)?),
                cache_read_tokens: natural(row.get(4)?),
                cache_write_tokens: natural(row.get(5)?),
            };
            tokens
                .entry(row.get(0)?)
                .or_default()
                .insert(row.get(1)?, usage);
        }
        Ok(tokens)
    }

    /// Each root's `(is_git, ownership, present)`.
    fn root_flags(&self) -> Result<HashMap<String, (bool, Ownership, bool)>, StoreError> {
        let mut roots: HashMap<String, (bool, Ownership, bool)> = HashMap::new();
        let mut statement = self
            .connection
            .prepare("SELECT root, is_git, ownership, present FROM projects")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            roots.insert(
                row.get(0)?,
                (
                    row.get::<_, i64>(1)? != 0,
                    Ownership::from_wire(&row.get::<_, String>(2)?),
                    row.get::<_, i64>(3)? != 0,
                ),
            );
        }
        Ok(roots)
    }

    /// What the index holds, grouped by project, gone folder and no project.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn found(&self) -> Result<Found, StoreError> {
        let tokens = self.reply_tokens()?;
        let roots = self.root_flags()?;

        let mut projects: HashMap<String, FoundProject> = HashMap::new();
        let mut gone: HashMap<String, GoneFolder> = HashMap::new();
        let mut not_a_project = Activity::default();
        let mut statement = self.connection.prepare(
            "SELECT session_id, place, project_root, last_at FROM sessions
             WHERE place IS NOT NULL AND last_at IS NOT NULL ORDER BY session_id",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let place: String = row.get(1)?;
            let root: Option<String> = row.get(2)?;
            let last_at = read_time(&row.get::<_, String>(3)?)?;
            let used = tokens.get(&id);
            let Some(root) = root else {
                add(&mut not_a_project, used);
                continue;
            };
            let (is_git, ownership, present) =
                roots
                    .get(&root)
                    .copied()
                    .unwrap_or((false, Ownership::Yours, place != "gone"));
            if place == "gone" || !present {
                let folder = gone.entry(root.clone()).or_insert_with(|| GoneFolder {
                    folder: root.clone(),
                    last_at,
                    activity: Activity::default(),
                });
                folder.last_at = folder.last_at.max(last_at);
                add(&mut folder.activity, used);
                continue;
            }
            let project = projects
                .entry(root.clone())
                .or_insert_with(|| FoundProject {
                    root,
                    is_git,
                    ownership,
                    last_at,
                    own: Activity::default(),
                    catervas: Activity::default(),
                });
            project.last_at = project.last_at.max(last_at);
            let side = if place == "catervas" {
                &mut project.catervas
            } else {
                &mut project.own
            };
            add(side, used);
        }

        let mut projects: Vec<FoundProject> = projects.into_values().collect();
        projects.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| a.root.cmp(&b.root)));
        let mut gone: Vec<GoneFolder> = gone.into_values().collect();
        gone.sort_by(|a, b| {
            b.last_at
                .cmp(&a.last_at)
                .then_with(|| a.folder.cmp(&b.folder))
        });
        let (unreadable_sessions, unreadable_lines): (i64, i64) = self.connection.query_row(
            "SELECT count(*) FILTER (WHERE turns = 0 AND unreadable_lines > 0),
                    coalesce(sum(unreadable_lines), 0)
             FROM transcript_files",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(Found {
            projects,
            gone,
            not_a_project,
            unreadable_sessions: natural(unreadable_sessions),
            unreadable_lines: natural(unreadable_lines),
        })
    }

    /// The migration versions applied to this index, in order.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the ledger cannot be read.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let versions = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;

    use super::*;
    use crate::files::fixtures::TempProject;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    #[test]
    fn opens_the_machine_index_and_records_its_migration() {
        let temp = TempProject::new("opens-machine-index");
        let path = temp.root.join("state").join(MACHINE_DB);
        let index = open_machine_index(&path, now()).expect("the index opens");
        assert_eq!(index.applied_migrations().expect("versions"), vec![1, 2, 3]);
        drop(index);
        let again = open_machine_index(&path, now()).expect("it opens again");
        assert_eq!(again.applied_migrations().expect("versions"), vec![1, 2, 3]);
        drop(again);
        let connection = Connection::open(&path).expect("the file opens");
        let mut tables: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations'")
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows");
        tables.sort();
        assert_eq!(
            tables,
            vec![
                "configured_servers",
                "jobs",
                "projects",
                "replies",
                "sessions",
                "tool_calls",
                "transcript_files"
            ]
        );
        assert_eq!(
            crate::migrations::known_versions(),
            (1..=16).collect::<Vec<i64>>()
        );
    }

    fn later(hours: i64) -> chrono::DateTime<Utc> {
        now() + chrono::Duration::hours(hours)
    }

    fn job(origin: &str, file: &str, name: &str, kind: JobKind, schedules: &[&str]) -> JobRow {
        JobRow {
            origin: origin.to_string(),
            file: file.to_string(),
            name: name.to_string(),
            kind,
            display_name: name.to_string(),
            schedules: schedules.iter().map(ToString::to_string).collect(),
            folder: None,
            project_root: None,
            expires_at: None,
            state: None,
            state_at: None,
            last_seen: now(),
        }
    }

    fn seed_session(connection: &Connection, id: &str, place: &str, root: Option<&str>) {
        connection
            .execute(
                "INSERT INTO sessions (session_id, place, project_root) VALUES (?1, ?2, ?3)",
                (id, place, root),
            )
            .expect("a session");
    }

    fn seed_session_cron(connection: &Connection, session: &str, tool: &str, expires: &str) {
        connection
            .execute(
                "INSERT INTO jobs (origin, file, name, kind, display_name, schedules, expires_at, last_seen)
                 VALUES ('claude_session', ?1, ?2, 'session_cron', 'Session cron * * * * *', '[\"* * * * *\"]', ?3,
                         '2026-10-12T09:00:00.000Z')",
                (session, tool, expires),
            )
            .expect("a session cron");
    }

    fn seen(index: &MachineIndex, origin: &str) -> Vec<(String, String)> {
        let mut statement = index
            .connection
            .prepare("SELECT name, last_seen FROM jobs WHERE origin = ?1 ORDER BY name")
            .expect("a statement");
        statement
            .query_map((origin,), |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    fn answered(origins: &[&str]) -> Vec<String> {
        origins.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn replaces_only_the_sources_that_answered() {
        let temp = TempProject::new("replaces-answered-sources");
        let mut index =
            open_machine_index(&temp.root.join(MACHINE_DB), now()).expect("the index opens");
        seed_session(&index.connection, "s1", "owner", Some("/home/me/shop"));
        seed_session(&index.connection, "s2", "not_a_project", None);
        seed_session_cron(&index.connection, "s1", "t1", "2026-10-19T09:00:00.000Z");
        seed_session_cron(&index.connection, "s2", "t2", "2026-10-19T09:00:00.000Z");

        let a = job("crontab", "", "aaaa", JobKind::Crontab, &["0 2 * * *"]);
        let b = job("claude_jobs", "", "abc", JobKind::BackgroundJob, &[]);
        index
            .replace_found_jobs(&answered(&["crontab", "claude_jobs"]), &[a, b], now())
            .expect("t1");
        assert_eq!(seen(&index, "claude_jobs").len(), 1);

        let a2 = job("crontab", "", "aaaa", JobKind::Crontab, &["0 3 * * *"]);
        index
            .replace_found_jobs(&answered(&["crontab", "claude_jobs"]), &[a2], later(1))
            .expect("t2");
        assert!(seen(&index, "claude_jobs").is_empty());
        let found = index.found_jobs(later(1)).expect("jobs");
        let crontab: Vec<&JobRow> = found.iter().filter(|j| j.origin == "crontab").collect();
        assert_eq!(crontab.len(), 1);
        assert_eq!(crontab[0].schedules, vec!["0 3 * * *".to_string()]);
        assert_eq!(crontab[0].last_seen, later(1));

        index
            .replace_found_jobs(&answered(&["claude_jobs"]), &[], later(2))
            .expect("t3");
        let kept = index.found_jobs(later(2)).expect("jobs");
        let kept_crontab: Vec<&JobRow> = kept.iter().filter(|j| j.origin == "crontab").collect();
        assert_eq!(kept_crontab.len(), 1);
        assert_eq!(kept_crontab[0].last_seen, later(1));

        assert_eq!(seen(&index, "claude_session").len(), 2);
        let roots: Vec<Option<String>> = ["s1", "s2"]
            .iter()
            .map(|id| {
                index
                    .connection
                    .query_row(
                        "SELECT project_root FROM jobs WHERE file = ?1",
                        (id,),
                        |row| row.get(0),
                    )
                    .expect("a root")
            })
            .collect();
        assert_eq!(roots, vec![Some("/home/me/shop".to_string()), None]);

        let k1 = job("crontab", "", "kkkk", JobKind::Crontab, &["x"]);
        let k2 = job("crontab", "", "kkkk", JobKind::Crontab, &["y", "x"]);
        index
            .replace_found_jobs(&answered(&["crontab"]), &[k1, k2], later(3))
            .expect("t4");
        let found = index.found_jobs(later(3)).expect("jobs");
        let k: Vec<&JobRow> = found.iter().filter(|j| j.name == "kkkk").collect();
        assert_eq!(k.len(), 1);
        assert_eq!(k[0].schedules, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn lists_jobs_by_kind_and_name() {
        let temp = TempProject::new("lists-jobs-by-kind");
        let mut index =
            open_machine_index(&temp.root.join(MACHINE_DB), now()).expect("the index opens");
        let mut deploy = job(
            "/r",
            "f2.yml",
            "z",
            JobKind::GithubActions,
            &["0 3 * * *", "1 * * * *"],
        );
        deploy.project_root = Some("/r".to_string());
        let rows = [
            job("crontab", "", "b", JobKind::Crontab, &["@daily"]),
            job("crontab", "", "a", JobKind::Crontab, &["@hourly"]),
            deploy,
            job("/r", "f1.yml", "z", JobKind::GithubActions, &[]),
            job(
                "/r",
                "vercel.json",
                "/api/x",
                JobKind::Vercel,
                &["* * * * *"],
            ),
        ];
        index
            .replace_found_jobs(&answered(&["crontab", "/r"]), &rows, now())
            .expect("replaced");
        seed_session_cron(&index.connection, "s1", "gone", "2026-10-01T09:00:00.000Z");
        seed_session_cron(
            &index.connection,
            "s1",
            "recent",
            "2026-10-08T09:00:00.000Z",
        );
        let found = index.found_jobs(now()).expect("jobs");
        let order: Vec<(String, String, String)> = found
            .iter()
            .map(|j| (j.kind.as_wire().to_string(), j.name.clone(), j.file.clone()))
            .collect();
        assert_eq!(
            order,
            [
                ("github_actions", "z", "f1.yml"),
                ("github_actions", "z", "f2.yml"),
                ("vercel", "/api/x", "vercel.json"),
                ("crontab", "a", ""),
                ("crontab", "b", ""),
                ("session_cron", "recent", "s1"),
            ]
            .map(|(kind, name, file)| (
                kind.to_string(),
                name.to_string(),
                file.to_string()
            ))
        );
        assert_eq!(
            found[1].schedules,
            vec!["0 3 * * *".to_string(), "1 * * * *".to_string()]
        );
        assert_eq!(found[1].project_root.as_deref(), Some("/r"));
        assert_eq!(found[1].last_seen, now());
    }

    #[test]
    fn forgets_the_machine_index() {
        let temp = TempProject::new("forgets-machine-index");
        let path = temp.root.join(MACHINE_DB);
        drop(open_machine_index(&path, now()).expect("the index opens"));
        let journal = temp.root.join("machine.db-journal");
        std::fs::write(&journal, b"x").expect("a journal");
        let state = temp.root.join("state.json");
        std::fs::write(&state, b"{}").expect("a state file");
        assert!(forget_machine_index(&path).expect("forgets"));
        assert!(!path.exists());
        assert!(!journal.exists());
        assert!(state.exists());
        assert!(!forget_machine_index(&path).expect("forgets again"));
    }
}
