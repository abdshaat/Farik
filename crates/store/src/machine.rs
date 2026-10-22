//! The machine-level index of the owner's Claude Code history, apart from every project's log.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use catervas_core::claude_export::{ClaudeProjectRecord, ConversationRecord};
use catervas_core::claude_history::{
    Copies, CopyLink, ProjectKey, SessionReplies, Source, ToolUse, match_hand_project,
    reply_fingerprint,
};
use catervas_core::claude_services::catalog::CATALOG;
use catervas_core::claude_services::{
    Call, Configured, ConfiguredServer, ServerKind, Service, ServiceSource, ServiceUse, SessionUse,
    Work, find_services, is_bookkeeping_folder,
};
use catervas_core::finding::Choice;
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
    /// `(session id, root)`: the roots a session changed a file under.
    pub roots_written: Vec<(String, String)>,
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

/// How many conversations were started in a month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthCount {
    /// The month, `YYYY-MM`, in UTC.
    pub month: String,
    /// The conversations started in it.
    pub conversations: u64,
}

/// A claude.ai Project, as the imported exports hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeAiProject {
    /// Its id.
    pub uuid: String,
    /// Its name.
    pub name: Option<String>,
    /// Its description.
    pub description: Option<String>,
    /// Its documents' file names.
    pub doc_names: Vec<String>,
    /// When it was last changed.
    pub updated_at: Option<DateTime<Utc>>,
    /// Whether its account's latest export holds it.
    pub in_latest_export: bool,
}

/// What the imported claude.ai exports hold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClaudeAi {
    /// The latest time any account's exports reach, when an export was imported.
    pub latest_export_at: Option<DateTime<Utc>>,
    /// Every conversation known, the ones missing from the latest export included.
    pub conversations: u64,
    /// Those with no readable start time.
    pub undated: u64,
    /// Those started in each of twelve months ending with the newest start, newest first.
    pub by_month: Vec<MonthCount>,
    /// Those started before the twelve months.
    pub earlier: u64,
    /// The Projects, most recently changed first.
    pub projects: Vec<ClaudeAiProject>,
    /// Conversations that an account's latest export no longer holds.
    pub conversations_not_in_latest: u64,
    /// Projects that an account's latest export no longer holds.
    pub projects_not_in_latest: u64,
}

/// What Claude Code on the web made in a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    /// A branch named `claude/…`.
    Branch,
    /// An open pull request from such a branch.
    Pull,
}

impl WorkKind {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Pull => "pull",
        }
    }

    fn from_wire(wire: &str) -> Self {
        if wire == "pull" {
            Self::Pull
        } else {
            Self::Branch
        }
    }
}

/// A branch or an open pull request that Claude Code on the web made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeWork {
    /// A branch or a pull request.
    pub kind: WorkKind,
    /// The branch's name; a pull request's title.
    pub name: String,
    /// The pull request's number.
    pub number: Option<u64>,
    /// Whether the pull request is a draft.
    pub draft: Option<bool>,
    /// When the pull request last changed.
    pub updated_at: Option<String>,
}

/// One repository as a read of GitHub saw it, with its Claude work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSeen {
    /// `owner/name`.
    pub full_name: String,
    /// Whether it is private.
    pub private: bool,
    /// When it was last pushed to.
    pub pushed_at: DateTime<Utc>,
    /// Its default branch.
    pub default_branch: String,
    /// Its `claude/…` branches and open pull requests from them.
    pub work: Vec<ClaudeWork>,
}

/// A GitHub repository that no folder on this computer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubProject {
    /// `owner/name`.
    pub full_name: String,
    /// Its normalised remote.
    pub remote_key: String,
    /// When it was last pushed to.
    pub pushed_at: DateTime<Utc>,
    /// Whether it is private.
    pub private: bool,
    /// Its default branch.
    pub default_branch: String,
    /// Whether the latest read of GitHub did not list it.
    pub not_seen_lately: bool,
    /// Its `claude/…` branches and open pull requests from them.
    pub claude_work: Vec<ClaudeWork>,
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
    /// Its repository's address as `normalise_remote` writes it, when it has a remote.
    pub remote: Option<String>,
    /// The name the owner gave a project added by hand that is this one, which is shown instead of the
    /// folder's.
    pub name_by_hand: Option<String>,
    /// Where it was found: `Local` always, `Teleported` when one of its sessions has a copy, and
    /// `ByHand` when a project added by hand is this one.
    pub sources: BTreeSet<Source>,
    /// The `claude/…` branches and pull requests of its GitHub repository, when GitHub lists it.
    pub claude_work: Vec<ClaudeWork>,
}

/// A project the owner added by hand: where it is, in a folder, a repository, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandProject {
    /// The name the owner gave it.
    pub name: String,
    /// Its root folder, canonical, when a folder was given.
    pub root: Option<String>,
    /// Its repository's address as `normalise_remote` writes it, when one was given.
    pub remote: Option<String>,
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
    /// Sessions that may be copies of others but are not certain enough to drop; none is dropped.
    pub maybe_same_sessions: u64,
    /// Projects added by hand that no found project is, oldest first.
    pub by_hand: Vec<HandProject>,
    /// What the imported claude.ai exports hold.
    pub claude_ai: ClaudeAi,
    /// GitHub repositories no folder on this computer has, most recently pushed first.
    pub github: Vec<GithubProject>,
}

/// A project formed from connector work done at home, with what its sessions did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormedFound {
    /// `"<Work> (<service>)"`.
    pub name: String,
    /// The work.
    pub work: Work,
    /// When any of its sessions last did anything.
    pub last_at: DateTime<Utc>,
    /// Its sessions and the tokens they used.
    pub activity: Activity,
    /// Every service its sessions called, most calls first.
    pub services: Vec<ServiceUse>,
    /// Whether the owner declared the service and no session formed the project.
    pub declared: bool,
}

/// A routine the owner typed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandRoutine {
    /// Its row id.
    pub id: i64,
    /// Its name.
    pub name: String,
    /// The key of the project it belongs to, when it belongs to one.
    pub project_key: Option<String>,
    /// When it runs, as the owner wrote it.
    pub schedule: String,
}

/// The services the owner reaches through Claude's connectors, as the index holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundServices {
    /// Every service, by kind then name.
    pub services: Vec<Service>,
    /// The services the sessions under each project root called, most calls first.
    pub by_root: BTreeMap<String, Vec<ServiceUse>>,
    /// Projects formed from connector work at home.
    pub formed: Vec<FormedFound>,
    /// Present roots without git whose owner sessions called spreadsheet files often enough.
    pub bookkeeping_roots: BTreeSet<String>,
    /// The sessions at home or in a temporary folder that formed no project.
    pub not_a_project: Activity,
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
    /// Its normalised remote; kept as it was when the root is not on this computer any more.
    pub remote: Option<String>,
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

/// The normalised remote of `owner/name` on GitHub, as a found folder's remote reads.
fn github_remote_key(full_name: &str) -> String {
    format!("github.com/{}", full_name.to_lowercase())
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

// An export's item replaces the stored one when it is at least as recent, or the stored one is undated.
const LATER: &str = "(updated_at IS NULL OR excluded.updated_at >= updated_at)";

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
    for (session_id, root) in &file.roots_written {
        transaction.execute(
            "INSERT OR IGNORE INTO session_roots_written (session_id, root) VALUES (?1, ?2)",
            (session_id, root),
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

    /// Writes one export's conversations and Projects, all or none: each item by uuid, its fields
    /// replaced when the incoming `updated_at` is at least the stored one (or the stored one is none),
    /// and the latest export time that held it kept. Returns how many conversations and Projects
    /// were not there before.
    pub(crate) fn record_claude_export(
        &mut self,
        account: &str,
        content_at: DateTime<Utc>,
        conversations: &[ConversationRecord],
        projects: &[ClaudeProjectRecord],
        now: DateTime<Utc>,
    ) -> Result<(u64, u64), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (seen, imported) = (stamp(content_at), stamp(now));
        transaction.execute(
            "INSERT OR IGNORE INTO claude_exports (account, content_at, imported_at)
             VALUES (?1, ?2, ?3)",
            (account, &seen, &imported),
        )?;
        let optional = |time: Option<DateTime<Utc>>| time.map(stamp);
        let mut new_conversations = 0;
        for item in conversations {
            let before: i64 = transaction.query_row(
                "SELECT count(*) FROM claude_conversations WHERE uuid = ?1",
                (&item.uuid,),
                |row| row.get(0),
            )?;
            new_conversations += u64::from(before == 0);
            transaction.execute(
                &"INSERT INTO claude_conversations
                     (uuid, account, title, created_at, updated_at, messages,
                      seen_in_export_at, first_imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (uuid) DO UPDATE SET
                     account = CASE WHEN LATER THEN excluded.account ELSE account END,
                     title = CASE WHEN LATER THEN excluded.title ELSE title END,
                     created_at = CASE WHEN LATER THEN excluded.created_at ELSE created_at END,
                     updated_at = CASE WHEN LATER THEN excluded.updated_at ELSE updated_at END,
                     messages = CASE WHEN LATER THEN excluded.messages ELSE messages END,
                     seen_in_export_at = max(seen_in_export_at, excluded.seen_in_export_at)"
                    .replace("LATER", LATER),
                (
                    &item.uuid,
                    account,
                    &item.title,
                    optional(item.created_at),
                    optional(item.updated_at),
                    int(item.messages),
                    &seen,
                    &imported,
                ),
            )?;
        }
        let mut new_projects = 0;
        for item in projects {
            let before: i64 = transaction.query_row(
                "SELECT count(*) FROM claude_projects WHERE uuid = ?1",
                (&item.uuid,),
                |row| row.get(0),
            )?;
            new_projects += u64::from(before == 0);
            let doc_names =
                serde_json::to_string(&item.doc_names).unwrap_or_else(|_| "[]".to_string());
            transaction.execute(
                &"INSERT INTO claude_projects
                     (uuid, account, name, description, doc_names, created_at, updated_at,
                      seen_in_export_at, first_imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (uuid) DO UPDATE SET
                     account = CASE WHEN LATER THEN excluded.account ELSE account END,
                     name = CASE WHEN LATER THEN excluded.name ELSE name END,
                     description = CASE WHEN LATER THEN excluded.description ELSE description END,
                     doc_names = CASE WHEN LATER THEN excluded.doc_names ELSE doc_names END,
                     created_at = CASE WHEN LATER THEN excluded.created_at ELSE created_at END,
                     updated_at = CASE WHEN LATER THEN excluded.updated_at ELSE updated_at END,
                     seen_in_export_at = max(seen_in_export_at, excluded.seen_in_export_at)"
                    .replace("LATER", LATER),
                (
                    &item.uuid,
                    account,
                    &item.name,
                    &item.description,
                    doc_names,
                    optional(item.created_at),
                    optional(item.updated_at),
                    &seen,
                    &imported,
                ),
            )?;
        }
        transaction.commit()?;
        Ok((new_conversations, new_projects))
    }

    /// Every session with a folder, as its id and that folder, the folder as it is now when the owner
    /// said it moved.
    pub(crate) fn session_folders(&self) -> Result<Vec<(String, String)>, StoreError> {
        let moves = self.moves()?;
        let mut statement = self.connection.prepare(
            "SELECT session_id, cwd FROM sessions WHERE cwd IS NOT NULL ORDER BY session_id",
        )?;
        let rows: Vec<(String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, cwd)| {
                let now = moves.iter().find_map(|(old, new)| {
                    Path::new(&cwd)
                        .strip_prefix(old)
                        .ok()
                        .map(|rest| Path::new(new).join(rest).to_string_lossy().into_owned())
                });
                (id, now.unwrap_or(cwd))
            })
            .collect())
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
                "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at, remote)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
                 ON CONFLICT (root) DO UPDATE SET
                     is_git = excluded.is_git, ownership = excluded.ownership,
                     present = excluded.present, checked_at = excluded.checked_at,
                     remote = CASE WHEN excluded.present = 1 THEN excluded.remote
                                   ELSE projects.remote END",
                (
                    &project.root,
                    i64::from(project.is_git),
                    project.ownership.as_wire(),
                    i64::from(project.present),
                    stamp(now),
                    &project.remote,
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

    /// Adds a project by hand, or renames the one with the same key.
    pub(crate) fn put_hand_project(
        &mut self,
        key: &str,
        project: &HandProject,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO hand_projects (key, name, root, remote, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (key) DO UPDATE SET
                 name = excluded.name, root = excluded.root, remote = excluded.remote",
            (
                key,
                &project.name,
                &project.root,
                &project.remote,
                stamp(now),
            ),
        )?;
        Ok(())
    }

    /// The projects added by hand, oldest first.
    fn hand_projects(&self) -> Result<Vec<HandProject>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT name, root, remote FROM hand_projects ORDER BY added_at, key")?;
        let rows = statement
            .query_map([], |row| {
                Ok(HandProject {
                    name: row.get(0)?,
                    root: row.get(1)?,
                    remote: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Each root's remote.
    fn root_remotes(&self) -> Result<HashMap<String, String>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT root, remote FROM projects WHERE remote IS NOT NULL")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Every link between sessions as `(session_id, other_id, kind)`.
    fn session_links(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, other_id, kind FROM session_links ORDER BY session_id, other_id",
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Every reply with the start of its session and the session's row id, sessions in row order and
    /// replies in time order, for finding copies. A reply of a session no turn started reads as
    /// starting its session.
    pub(crate) fn session_replies(&self) -> Result<Vec<SessionReplies>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT sessions.session_id, sessions.rowid, coalesce(sessions.first_at, replies.at),
                    replies.reply_key, replies.model, replies.at, replies.input_tokens,
                    replies.output_tokens, replies.cache_read_tokens, replies.cache_write_tokens
             FROM replies JOIN sessions ON sessions.session_id = replies.session_id
             ORDER BY sessions.rowid, replies.at, replies.reply_key",
        )?;
        let mut sessions: Vec<SessionReplies> = Vec::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let usage = Usage {
                input_tokens: natural(row.get(6)?),
                output_tokens: natural(row.get(7)?),
                cache_read_tokens: natural(row.get(8)?),
                cache_write_tokens: natural(row.get(9)?),
            };
            let print = reply_fingerprint(
                read_time(&row.get::<_, String>(5)?)?,
                &row.get::<_, String>(4)?,
                usage,
            );
            if sessions.last().is_none_or(|last| last.session_id != id) {
                sessions.push(SessionReplies {
                    session_id: id,
                    first_at: read_time(&row.get::<_, String>(2)?)?,
                    indexed: row.get(1)?,
                    replies: Vec::new(),
                });
            }
            if let Some(last) = sessions.last_mut() {
                last.replies.push((row.get(3)?, print));
            }
        }
        Ok(sessions)
    }

    /// Replaces the links between sessions and the replies counted once, in one transaction.
    pub(crate) fn replace_copies(&mut self, copies: &Copies) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM session_links", [])?;
        transaction.execute("DELETE FROM reply_copies", [])?;
        for link in &copies.links {
            let (first, second, kind) = match link {
                CopyLink::CopyOf { copy, of } => (copy, of, "copy_of"),
                CopyLink::MaybeSame { first, second } => (first, second, "maybe_same"),
            };
            transaction.execute(
                "INSERT INTO session_links (session_id, other_id, kind) VALUES (?1, ?2, ?3)",
                (first, second, kind),
            )?;
        }
        for (copied, kept) in &copies.reply_copies {
            transaction.execute(
                "INSERT INTO reply_copies (reply_key, same_as) VALUES (?1, ?2)",
                (copied, kept),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// How many transcript files had lines but no turn, and how many lines could not be read.
    fn unreadable_counts(&self) -> Result<(u64, u64), StoreError> {
        let (sessions, lines): (i64, i64) = self.connection.query_row(
            "SELECT count(*) FILTER (WHERE turns = 0 AND unreadable_lines > 0),
                    coalesce(sum(unreadable_lines), 0)
             FROM transcript_files",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((natural(sessions), natural(lines)))
    }

    /// Joins each project added by hand to the found project it names, giving that one the owner's
    /// name and the `ByHand` source, and returns the ones no found project is. `projects` is newest
    /// first, which is the order a match prefers.
    fn join_hand_projects(
        &self,
        projects: &mut [FoundProject],
    ) -> Result<Vec<HandProject>, StoreError> {
        let mut by_hand = Vec::new();
        for hand in self.hand_projects()? {
            let matched = {
                let keys: Vec<ProjectKey<'_>> = projects
                    .iter()
                    .map(|project| ProjectKey {
                        root: &project.root,
                        remote: project.remote.as_deref(),
                    })
                    .collect();
                match_hand_project(hand.root.as_deref(), hand.remote.as_deref(), &keys)
            };
            match matched {
                Some(index) => {
                    let project = &mut projects[index];
                    project.name_by_hand.get_or_insert(hand.name);
                    project.sources.insert(Source::ByHand);
                }
                None => by_hand.push(hand),
            }
        }
        Ok(by_hand)
    }

    /// Each session's reply tokens, by model.
    fn reply_tokens(&self) -> Result<HashMap<String, BTreeMap<String, Usage>>, StoreError> {
        let mut tokens: HashMap<String, BTreeMap<String, Usage>> = HashMap::new();
        let mut statement = self.connection.prepare(
            "SELECT session_id, model, SUM(input_tokens), SUM(output_tokens),
                    SUM(cache_read_tokens), SUM(cache_write_tokens)
             FROM replies
             WHERE reply_key NOT IN (SELECT reply_key FROM reply_copies)
             GROUP BY session_id, model",
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
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the sessions, and the joins of what the other tables hold"
    )]
    pub fn found(&self) -> Result<Found, StoreError> {
        let tokens = self.reply_tokens()?;
        let roots = self.root_flags()?;
        let remotes = self.root_remotes()?;
        let links = self.session_links()?;
        let copies: BTreeSet<&String> = links
            .iter()
            .filter(|link| link.2 == "copy_of")
            .map(|link| &link.0)
            .collect();
        let kept_copies: BTreeSet<&String> = links
            .iter()
            .filter(|link| link.2 == "copy_of")
            .map(|link| &link.1)
            .collect();
        let maybe_same: BTreeSet<&String> = links
            .iter()
            .filter(|link| link.2 == "maybe_same")
            .flat_map(|link| [&link.0, &link.1])
            .collect();

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
            if copies.contains(&id) {
                continue;
            }
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
                    remote: remotes.get(&root).cloned(),
                    name_by_hand: None,
                    root,
                    is_git,
                    ownership,
                    last_at,
                    own: Activity::default(),
                    catervas: Activity::default(),
                    sources: BTreeSet::from([Source::Local]),
                    claude_work: Vec::new(),
                });
            if kept_copies.contains(&id) {
                project.sources.insert(Source::Teleported);
            }
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
        let by_hand = self.join_hand_projects(&mut projects)?;
        let github = self.join_github(&mut projects, &by_hand)?;
        let mut gone: Vec<GoneFolder> = gone.into_values().collect();
        gone.sort_by(|a, b| {
            b.last_at
                .cmp(&a.last_at)
                .then_with(|| a.folder.cmp(&b.folder))
        });
        let (unreadable_sessions, unreadable_lines) = self.unreadable_counts()?;
        Ok(Found {
            projects,
            gone,
            not_a_project,
            unreadable_sessions,
            unreadable_lines,
            maybe_same_sessions: maybe_same.len() as u64,
            by_hand,
            claude_ai: self.claude_ai()?,
            github,
        })
    }

    /// Keeps what a read of GitHub saw: each repository seen replaces its row and its Claude work.
    /// A repository this read did not list stays as it was, and shows as not seen lately.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be written.
    pub fn replace_github(
        &mut self,
        seen: &[GithubSeen],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for repo in seen {
            let remote_key = github_remote_key(&repo.full_name);
            transaction.execute(
                "INSERT INTO github_repos
                     (full_name, remote_key, pushed_at, private, default_branch, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(full_name) DO UPDATE SET
                     remote_key = excluded.remote_key, pushed_at = excluded.pushed_at,
                     private = excluded.private, default_branch = excluded.default_branch,
                     seen_at = excluded.seen_at",
                (
                    &repo.full_name,
                    &remote_key,
                    repo.pushed_at.to_rfc3339_opts(SecondsFormat::Millis, true),
                    i64::from(repo.private),
                    &repo.default_branch,
                    &at,
                ),
            )?;
            transaction.execute(
                "DELETE FROM github_claude_work WHERE full_name = ?1",
                (&repo.full_name,),
            )?;
            for work in &repo.work {
                transaction.execute(
                    "INSERT OR REPLACE INTO github_claude_work
                         (full_name, kind, name, number, draft, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    (
                        &repo.full_name,
                        work.kind.as_wire(),
                        &work.name,
                        work.number.map(int),
                        work.draft.map(i64::from),
                        &work.updated_at,
                    ),
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Gives each found project its GitHub repository's Claude work and the `Github` source, and
    /// returns the repositories no found project, and no folder added by hand, has.
    fn join_github(
        &self,
        projects: &mut [FoundProject],
        by_hand: &[HandProject],
    ) -> Result<Vec<GithubProject>, StoreError> {
        let mut work: HashMap<String, Vec<ClaudeWork>> = HashMap::new();
        let mut statement = self.connection.prepare(
            "SELECT full_name, kind, name, number, draft, updated_at FROM github_claude_work
             ORDER BY kind, name",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            work.entry(row.get(0)?).or_default().push(ClaudeWork {
                kind: WorkKind::from_wire(&row.get::<_, String>(1)?),
                name: row.get(2)?,
                number: row.get::<_, Option<i64>>(3)?.map(natural),
                draft: row.get::<_, Option<i64>>(4)?.map(|draft| draft != 0),
                updated_at: row.get(5)?,
            });
        }
        let latest: Option<String> =
            self.connection
                .query_row("SELECT max(seen_at) FROM github_repos", [], |row| {
                    row.get(0)
                })?;
        let mut statement = self.connection.prepare(
            "SELECT full_name, remote_key, pushed_at, private, default_branch, seen_at
             FROM github_repos ORDER BY pushed_at DESC, full_name",
        )?;
        let mut rows = statement.query([])?;
        let mut elsewhere = Vec::new();
        while let Some(row) = rows.next()? {
            let full_name: String = row.get(0)?;
            let remote_key: String = row.get(1)?;
            let claude_work = work.remove(&full_name).unwrap_or_default();
            // A project added by hand with this repository's address is on this computer too.
            let mut on_this_computer = by_hand.iter().any(|hand| {
                hand.root.is_some() && hand.remote.as_deref() == Some(remote_key.as_str())
            });
            for project in projects
                .iter_mut()
                .filter(|project| project.remote.as_deref() == Some(remote_key.as_str()))
            {
                on_this_computer = true;
                project.sources.insert(Source::Github);
                project.claude_work.clone_from(&claude_work);
            }
            if !on_this_computer {
                elsewhere.push(GithubProject {
                    full_name,
                    remote_key,
                    pushed_at: read_time(&row.get::<_, String>(2)?)?,
                    private: row.get::<_, i64>(3)? != 0,
                    default_branch: row.get(4)?,
                    not_seen_lately: Some(row.get::<_, String>(5)?) < latest,
                    claude_work,
                });
            }
        }
        Ok(elsewhere)
    }

    /// The imported claude.ai exports, grouped for the report. Whether an item is in its account's
    /// latest export is worked out here from the times, so the order exports were imported in never
    /// matters.
    fn claude_ai(&self) -> Result<ClaudeAi, StoreError> {
        let mut latest: HashMap<String, String> = HashMap::new();
        let mut statement = self
            .connection
            .prepare("SELECT account, max(content_at) FROM claude_exports GROUP BY account")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            latest.insert(row.get(0)?, row.get(1)?);
        }
        let missing = |account: &str, seen: &str| {
            latest
                .get(account)
                .is_some_and(|latest| seen < latest.as_str())
        };
        let mut claude = ClaudeAi {
            latest_export_at: latest
                .values()
                .max()
                .map(|time| read_time(time))
                .transpose()?,
            ..ClaudeAi::default()
        };
        let mut created: Vec<String> = Vec::new();
        let mut statement = self
            .connection
            .prepare("SELECT account, created_at, seen_in_export_at FROM claude_conversations")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            claude.conversations += 1;
            let (account, start, seen): (String, Option<String>, String) =
                (row.get(0)?, row.get(1)?, row.get(2)?);
            claude.conversations_not_in_latest += u64::from(missing(&account, &seen));
            match start {
                Some(start) => created.push(start),
                None => claude.undated += 1,
            }
        }
        if let Some(newest) = created.iter().max().and_then(|start| start.get(..7)) {
            let (mut year, mut month) = (
                newest[..4].parse::<i32>().unwrap_or(1970),
                newest[5..7].parse::<u32>().unwrap_or(1),
            );
            for _ in 0..12 {
                claude.by_month.push(MonthCount {
                    month: format!("{year:04}-{month:02}"),
                    conversations: 0,
                });
                if month == 1 {
                    (year, month) = (year - 1, 12);
                } else {
                    month -= 1;
                }
            }
            for start in &created {
                let month = start.get(..7).unwrap_or_default();
                match claude
                    .by_month
                    .iter_mut()
                    .find(|entry| entry.month == month)
                {
                    Some(entry) => entry.conversations += 1,
                    None => claude.earlier += 1,
                }
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT uuid, account, name, description, doc_names, updated_at, seen_in_export_at
             FROM claude_projects",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let account: String = row.get(1)?;
            let seen: String = row.get(6)?;
            let doc_names: String = row.get(4)?;
            let in_latest_export = !missing(&account, &seen);
            claude.projects_not_in_latest += u64::from(!in_latest_export);
            claude.projects.push(ClaudeAiProject {
                uuid: row.get(0)?,
                name: row.get(2)?,
                description: row.get(3)?,
                doc_names: serde_json::from_str(&doc_names).unwrap_or_default(),
                updated_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|time| read_time(&time))
                    .transpose()?,
                in_latest_export,
            });
        }
        claude.projects.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.uuid.cmp(&b.uuid))
        });
        Ok(claude)
    }

    fn configured(&self) -> Result<Vec<Configured>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT source, name, kind, host, program FROM configured_servers
             ORDER BY source, found_in, name",
        )?;
        let mut rows = statement.query([])?;
        let mut configured = Vec::new();
        while let Some(row) = rows.next()? {
            let odd = |what: &str| StoreError::Sqlite {
                detail: format!("the index holds a configured server with a bad {what}"),
            };
            configured.push(Configured {
                source: ServiceSource::from_wire(&row.get::<_, String>(0)?)
                    .ok_or_else(|| odd("source"))?,
                server: ConfiguredServer {
                    name: row.get(1)?,
                    kind: ServerKind::from_wire(&row.get::<_, String>(2)?)
                        .ok_or_else(|| odd("kind"))?,
                    host: row.get(3)?,
                    program: row.get(4)?,
                },
            });
        }
        Ok(configured)
    }

    /// The owner's sessions with their tool calls: the owner's, the gone and the home sessions.
    fn session_uses(&self) -> Result<Vec<(SessionUse, DateTime<Utc>)>, StoreError> {
        let mut calls: HashMap<String, Vec<Call>> = HashMap::new();
        let mut statement = self.connection.prepare(
            "SELECT replies.session_id, tool_calls.tool_use_id, tool_calls.tool_name, tool_calls.at
             FROM tool_calls JOIN replies ON replies.reply_key = tool_calls.reply_key
             ORDER BY tool_calls.at, tool_calls.tool_use_id",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            calls.entry(row.get(0)?).or_default().push(Call {
                tool_use_id: row.get(1)?,
                tool_name: row.get(2)?,
                at: read_time(&row.get::<_, String>(3)?)?,
            });
        }
        let mut statement = self.connection.prepare(
            "SELECT session_id, place, project_root, last_at FROM sessions
             WHERE place IN ('owner', 'gone', 'not_a_project') AND last_at IS NOT NULL
             ORDER BY session_id",
        )?;
        let mut rows = statement.query([])?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let place: String = row.get(1)?;
            let root: Option<String> = row.get(2)?;
            sessions.push((
                SessionUse {
                    calls: calls.remove(&id).unwrap_or_default(),
                    session_id: id,
                    root: if place == "owner" { root } else { None },
                    is_home: place == "not_a_project",
                },
                read_time(&row.get::<_, String>(3)?)?,
            ));
        }
        Ok(sessions)
    }

    /// The roots of present folders without git with the spreadsheet calls their owner sessions made.
    fn spreadsheet_roots(&self) -> Result<BTreeSet<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT projects.root, projects.is_git, count(*) FROM projects
             JOIN sessions ON sessions.project_root = projects.root AND sessions.place = 'owner'
             JOIN replies ON replies.session_id = sessions.session_id
             JOIN tool_calls ON tool_calls.reply_key = replies.reply_key
                            AND tool_calls.touches_spreadsheet = 1
             WHERE projects.present = 1
             GROUP BY projects.root, projects.is_git",
        )?;
        let mut roots = BTreeSet::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if is_bookkeeping_folder(row.get::<_, i64>(1)? != 0, natural(row.get(2)?)) {
                roots.insert(row.get(0)?);
            }
        }
        Ok(roots)
    }

    /// The services named by Claude's settings or called in the owner's sessions, where each is used,
    /// the projects connector work at home forms, and the folders of spreadsheets. Catervas's agents'
    /// sessions do not count.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn found_services(&self) -> Result<FoundServices, StoreError> {
        let sessions = self.session_uses()?;
        let uses: Vec<SessionUse> = sessions
            .iter()
            .map(|(session, _)| session.clone())
            .collect();
        let found = find_services(&self.configured()?, &uses);
        let tokens = self.reply_tokens()?;
        let last_at: HashMap<&str, DateTime<Utc>> = sessions
            .iter()
            .map(|(session, at)| (session.session_id.as_str(), *at))
            .collect();
        let formed: Vec<FormedFound> = found
            .formed
            .iter()
            .map(|project| {
                let mut activity = Activity::default();
                let mut latest: Option<DateTime<Utc>> = None;
                for id in &project.session_ids {
                    add(&mut activity, tokens.get(id));
                    latest = latest.max(last_at.get(id.as_str()).copied());
                }
                FormedFound {
                    name: project.name.clone(),
                    work: project.work,
                    last_at: latest.unwrap_or(DateTime::UNIX_EPOCH),
                    activity,
                    services: project.services.clone(),
                    declared: false,
                }
            })
            .collect();
        let mut formed = formed;
        if !formed
            .iter()
            .any(|project| project.work == Work::Bookkeeping)
        {
            for (id, name, at) in self.declared_service_rows()? {
                let bookkeeping = CATALOG
                    .iter()
                    .find(|entry| entry.id == id)
                    .is_some_and(|entry| Work::of(entry.kind) == Some(Work::Bookkeeping));
                if bookkeeping {
                    formed.push(FormedFound {
                        name: format!("{} ({name})", Work::Bookkeeping.label()),
                        work: Work::Bookkeeping,
                        last_at: at,
                        activity: Activity::default(),
                        services: Vec::new(),
                        declared: true,
                    });
                }
            }
        }
        let taken: BTreeSet<&str> = found
            .formed
            .iter()
            .flat_map(|project| project.session_ids.iter().map(String::as_str))
            .collect();
        let mut not_a_project = Activity::default();
        for (session, _) in &sessions {
            if session.is_home && !taken.contains(session.session_id.as_str()) {
                add(&mut not_a_project, tokens.get(&session.session_id));
            }
        }
        Ok(FoundServices {
            services: found.services,
            by_root: found.by_root,
            formed,
            bookkeeping_roots: self.spreadsheet_roots()?,
            not_a_project,
        })
    }

    /// Keeps the owner's choice for the project `key`, replacing an earlier one.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_choice(
        &mut self,
        key: &str,
        choice: Choice,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO choices (key, choice, at) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET choice = excluded.choice, at = excluded.at",
            (key, choice.as_wire(), stamp(now)),
        )?;
        Ok(())
    }

    /// The owner's choices, by project key.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn choices(&self) -> Result<BTreeMap<String, Choice>, StoreError> {
        let mut statement = self.connection.prepare("SELECT key, choice FROM choices")?;
        let mut rows = statement.query([])?;
        let mut choices = BTreeMap::new();
        while let Some(row) = rows.next()? {
            let wire: String = row.get(1)?;
            let choice = Choice::from_wire(&wire).ok_or_else(|| StoreError::Sqlite {
                detail: "the index holds a choice that is not one".to_string(),
            })?;
            choices.insert(row.get(0)?, choice);
        }
        Ok(choices)
    }

    /// The normalised remote of the project at `root`, kept as it was when the folder went.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn remote_of(&self, root: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT remote FROM projects WHERE root = ?1",
                (root,),
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Records that the folders `a` and `b` are different projects, whichever way round they are given.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn separate(&mut self, a: &str, b: &str) -> Result<(), StoreError> {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        self.connection.execute(
            "INSERT OR IGNORE INTO separations (a, b) VALUES (?1, ?2)",
            (low, high),
        )?;
        Ok(())
    }

    /// The pairs of folders the owner said are different, each with the smaller path first.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn separations(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT a, b FROM separations ORDER BY a, b")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Records that the folder `old` is now `new`; the sessions that worked under `old` are placed
    /// under `new` the next time sessions are placed.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_move(&mut self, old: &str, new: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO moved (old, new) VALUES (?1, ?2)
             ON CONFLICT (old) DO UPDATE SET new = excluded.new",
            (old, new),
        )?;
        Ok(())
    }

    /// Every folder that moved, as `(old, new)`.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn moves(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT old, new FROM moved ORDER BY old")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Replaces the services the owner says they use with `rows`, each a catalog id (or
    /// `other:<name>`) and a name.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn replace_declared_services(
        &mut self,
        rows: &[(String, String)],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM declared_services", [])?;
        for (id, name) in rows {
            transaction.execute(
                "INSERT OR REPLACE INTO declared_services (service_id, name, at) VALUES (?1, ?2, ?3)",
                (id, name, stamp(now)),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// The services the owner declared, as `(id, name)`, by name.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn declared_services(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT service_id, name FROM declared_services ORDER BY name, service_id")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    fn declared_service_rows(&self) -> Result<Vec<(String, String, DateTime<Utc>)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT service_id, name, at FROM declared_services ORDER BY name, service_id",
        )?;
        let mut rows = statement.query([])?;
        let mut declared = Vec::new();
        while let Some(row) = rows.next()? {
            declared.push((
                row.get(0)?,
                row.get(1)?,
                read_time(&row.get::<_, String>(2)?)?,
            ));
        }
        Ok(declared)
    }

    /// Adds a routine the owner typed in and answers its id.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn add_hand_routine(
        &mut self,
        name: &str,
        project_key: Option<&str>,
        schedule: &str,
        now: DateTime<Utc>,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO hand_routines (name, project_key, schedule, at) VALUES (?1, ?2, ?3, ?4)",
            (name, project_key, schedule, stamp(now)),
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// The routines the owner typed in, oldest first.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn hand_routines(&self) -> Result<Vec<HandRoutine>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name, project_key, schedule FROM hand_routines ORDER BY id")?;
        let rows = statement
            .query_map([], |row| {
                Ok(HandRoutine {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    project_key: row.get(2)?,
                    schedule: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
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

/// What the owner changed on a card of "Your teams".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProposalEdit {
    /// The team's name.
    pub name: Option<String>,
    /// The built-in or saved team it starts from.
    pub template: Option<String>,
    /// What the team is for, in a sentence.
    pub purpose: Option<String>,
    /// Whether Claude suggested what is edited, and the owner has not changed it.
    pub from_claude: bool,
}

/// A team Catervas made from a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadeTeam {
    /// Its slug in the hub.
    pub slug: String,
    /// Its project folder.
    pub root: String,
    /// The JSON of what was created.
    pub made: String,
}

impl MachineIndex {
    /// Keeps `edit` for the card `card_id`: a field that is `None` keeps what was there, and
    /// `from_claude` is whatever `edit` says, so that the owner's own edit takes the label away.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_edit(&mut self, card_id: &str, edit: &ProposalEdit) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO proposal_edits (card_id, name, template, purpose, from_claude)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (card_id) DO UPDATE SET
                 name = COALESCE(excluded.name, name),
                 template = COALESCE(excluded.template, template),
                 purpose = COALESCE(excluded.purpose, purpose),
                 from_claude = excluded.from_claude",
            (
                card_id,
                &edit.name,
                &edit.template,
                &edit.purpose,
                i64::from(edit.from_claude),
            ),
        )?;
        Ok(())
    }

    /// Every card's edits, by the card's id.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn edits(&self) -> Result<BTreeMap<String, ProposalEdit>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT card_id, name, template, purpose, from_claude FROM proposal_edits")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ProposalEdit {
                        name: row.get(1)?,
                        template: row.get(2)?,
                        purpose: row.get(3)?,
                        from_claude: row.get::<_, i64>(4)? == 1,
                    },
                ))
            })?
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(rows)
    }

    /// Forgets the edits of a card.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn revert_edit(&mut self, card_id: &str) -> Result<(), StoreError> {
        self.connection
            .execute("DELETE FROM proposal_edits WHERE card_id = ?1", (card_id,))?;
        Ok(())
    }

    /// Moves the project `key` out of the card `card_id`, and says No between it and each of
    /// `others`, the projects that stay.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn move_out(
        &mut self,
        card_id: &str,
        key: &str,
        others: &[String],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO moved_out (card_id, project_key) VALUES (?1, ?2)",
            (card_id, key),
        )?;
        for other in others.iter().filter(|other| other.as_str() != key) {
            self.put_link(key, other, crate::linking::Answer::No, &[], now)?;
        }
        Ok(())
    }

    /// The `(card id, project key)` pairs the owner moved out.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn moved_out(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT card_id, project_key FROM moved_out ORDER BY card_id, project_key")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Forgets that the projects `keys` were moved out of a card: they are teams, or undone.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn clear_moved_out_of(&mut self, keys: &[String]) -> Result<(), StoreError> {
        for key in keys {
            self.connection
                .execute("DELETE FROM moved_out WHERE project_key = ?1", (key,))?;
        }
        Ok(())
    }

    /// Keeps that Catervas made the team `slug` at `root`, and what it created for it.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_made_team(
        &mut self,
        slug: &str,
        root: &str,
        made: &str,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO made_teams (slug, root, made, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (slug) DO UPDATE SET root = excluded.root, made = excluded.made,
                                              at = excluded.at",
            (slug, root, made, stamp(now)),
        )?;
        Ok(())
    }

    /// The teams Catervas made.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn made_teams(&self) -> Result<Vec<MadeTeam>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT slug, root, made FROM made_teams ORDER BY at, slug")?;
        let rows = statement
            .query_map([], |row| {
                Ok(MadeTeam {
                    slug: row.get(0)?,
                    root: row.get(1)?,
                    made: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Forgets a made team.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn forget_made_team(&mut self, slug: &str) -> Result<(), StoreError> {
        self.connection
            .execute("DELETE FROM made_teams WHERE slug = ?1", (slug,))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use catervas_core::claude_history::fold_by_remote;
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
        assert_eq!(
            index.applied_migrations().expect("versions"),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
        drop(index);
        let again = open_machine_index(&path, now()).expect("it opens again");
        assert_eq!(
            again.applied_migrations().expect("versions"),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
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
                "choices",
                "claude_conversations",
                "claude_exports",
                "claude_projects",
                "configured_servers",
                "declared_services",
                "github_claude_work",
                "github_repos",
                "hand_projects",
                "hand_routines",
                "jobs",
                "links",
                "made_teams",
                "moved",
                "moved_out",
                "projects",
                "proposal_edits",
                "replies",
                "reply_copies",
                "separations",
                "session_links",
                "session_roots_written",
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

    #[test]
    fn keeps_the_owner_s_edits_and_what_was_made() {
        let temp = TempProject::new("keeps-proposal");
        let path = temp.root.join(MACHINE_DB);
        let mut index = open_machine_index(&path, now()).expect("the index opens");
        assert_eq!(
            index.applied_migrations().expect("versions").last(),
            Some(&10)
        );
        index
            .put_edit(
                "card1",
                &ProposalEdit {
                    name: Some("Shop".into()),
                    template: Some("online-store".into()),
                    ..ProposalEdit::default()
                },
            )
            .expect("an edit");
        index
            .put_edit(
                "card1",
                &ProposalEdit {
                    purpose: Some("Sells things".into()),
                    ..ProposalEdit::default()
                },
            )
            .expect("a second edit keeps the first");
        index
            .put_edit(
                "card2",
                &ProposalEdit {
                    name: Some("Books".into()),
                    from_claude: true,
                    ..ProposalEdit::default()
                },
            )
            .expect("claude's edit");
        index
            .move_out("card1", "b", &["a".to_string(), "c".to_string()], now())
            .expect("moved out");
        index
            .put_made_team(
                "shop",
                "/home/me/shop",
                r#"{"created":[".catervas"]}"#,
                now(),
            )
            .expect("made");
        drop(index);

        let mut index = open_machine_index(&path, now()).expect("it opens again");
        let edits = index.edits().expect("edits");
        assert_eq!(
            edits["card1"],
            ProposalEdit {
                name: Some("Shop".into()),
                template: Some("online-store".into()),
                purpose: Some("Sells things".into()),
                from_claude: false
            }
        );
        assert!(edits["card2"].from_claude);
        index.revert_edit("card2").expect("reverted");
        assert!(!index.edits().expect("edits").contains_key("card2"));
        assert_eq!(
            index.moved_out().expect("moved"),
            vec![("card1".to_string(), "b".to_string())]
        );
        let links = index.link_answers().expect("links");
        let no: Vec<_> = links
            .iter()
            .map(|(pair, link)| (pair.clone(), link.answer))
            .collect();
        assert_eq!(
            no,
            vec![
                (
                    ("a".to_string(), "b".to_string()),
                    crate::linking::Answer::No
                ),
                (
                    ("b".to_string(), "c".to_string()),
                    crate::linking::Answer::No
                ),
            ]
        );
        let made = index.made_teams().expect("made");
        assert_eq!(made.len(), 1);
        assert_eq!(
            (made[0].slug.as_str(), made[0].root.as_str()),
            ("shop", "/home/me/shop")
        );
        assert_eq!(made[0].made, r#"{"created":[".catervas"]}"#);
        index.forget_made_team("shop").expect("forgotten");
        assert!(index.made_teams().expect("made").is_empty());
        index
            .clear_moved_out_of(&["b".to_string()])
            .expect("cleared");
        assert!(index.moved_out().expect("moved").is_empty());
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

    fn seed_project(connection: &Connection, root: &str, is_git: bool) {
        connection
            .execute(
                "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at)
                 VALUES (?1, ?2, 'yours', 1, '2026-10-12T09:00:00.000Z', '2026-10-12T09:00:00.000Z')",
                (root, i64::from(is_git)),
            )
            .expect("a project");
    }

    fn seed_session_at(
        connection: &Connection,
        id: &str,
        place: &str,
        root: Option<&str>,
        last_at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO sessions (session_id, place, project_root, last_at) VALUES (?1, ?2, ?3, ?4)",
                (id, place, root, last_at),
            )
            .expect("a session");
    }

    fn seed_reply(connection: &Connection, key: &str, session: &str, tokens: i64) {
        connection
            .execute(
                "INSERT INTO replies (reply_key, session_id, model, at, input_tokens, output_tokens,
                                      cache_read_tokens, cache_write_tokens)
                 VALUES (?1, ?2, 'm', '2026-10-12T09:00:00.000Z', ?3, ?3, 0, 0)",
                (key, session, tokens),
            )
            .expect("a reply");
    }

    /// A call to server `x`: a `tool_calls` row of tool `mcp__x__do` on reply `reply`.
    fn seed_call(
        connection: &Connection,
        id: &str,
        reply: &str,
        tool: &str,
        sheet: bool,
        at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO tool_calls (tool_use_id, reply_key, tool_name, touches_spreadsheet, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                (id, reply, tool, i64::from(sheet), at),
            )
            .expect("a tool call");
    }

    fn seed_server(connection: &Connection, source: &str, name: &str) {
        connection
            .execute(
                "INSERT INTO configured_servers (source, found_in, name, kind) VALUES (?1, '', ?2, 'http')",
                (source, name),
            )
            .expect("a configured server");
    }

    fn open_seeded(name: &str) -> (TempProject, MachineIndex) {
        let temp = TempProject::new(name);
        let index =
            open_machine_index(&temp.root.join(MACHINE_DB), now()).expect("the index opens");
        (temp, index)
    }

    #[test]
    fn reports_services_per_project() {
        let (_temp, index) = open_seeded("reports-services-per-project");
        let c = &index.connection;
        seed_project(c, "/home/me/shop", true);
        seed_session_at(
            c,
            "s1",
            "owner",
            Some("/home/me/shop"),
            "2026-10-12T09:00:00.000Z",
        );
        seed_reply(c, "r1", "s1", 1);
        seed_reply(c, "r2", "s1", 1);
        seed_call(
            c,
            "t1",
            "r1",
            "mcp__github__do",
            false,
            "2026-10-12T09:00:00.000Z",
        );
        seed_call(
            c,
            "t2",
            "r2",
            "mcp__claude_ai_Zoho_Books__do",
            false,
            "2026-10-12T09:01:00.000Z",
        );
        seed_server(c, "claude_code", "stripe");
        let found = index.found_services().expect("services");
        let uses: Vec<(&str, u64)> = found.by_root["/home/me/shop"]
            .iter()
            .map(|u| (u.name.as_str(), u.calls))
            .collect();
        assert_eq!(uses, [("GitHub", 1), ("Zoho Books", 1)]);
        let names: Vec<&str> = found.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Zoho Books", "Stripe", "GitHub"]);
    }

    #[test]
    fn forms_a_project_and_leaves_the_rest_not_in_a_project() {
        let (_temp, index) = open_seeded("forms-a-project");
        let c = &index.connection;
        seed_session_at(c, "h1", "not_a_project", None, "2026-10-12T09:30:00.000Z");
        seed_session_at(c, "h2", "not_a_project", None, "2026-10-12T10:00:00.000Z");
        for (i, at) in ["09:10", "09:20", "09:30"].iter().enumerate() {
            seed_reply(c, &format!("r{i}"), "h1", 1);
            seed_call(
                c,
                &format!("t{i}"),
                &format!("r{i}"),
                "mcp__claude_ai_Zoho_Books__do",
                false,
                &format!("2026-10-12T{at}:00.000Z"),
            );
        }
        seed_reply(c, "r9", "h2", 5);
        let found = index.found_services().expect("services");
        assert_eq!(found.formed.len(), 1);
        let formed = &found.formed[0];
        assert_eq!(formed.name, "Bookkeeping (Zoho Books)");
        assert_eq!(
            formed.last_at,
            Utc.with_ymd_and_hms(2026, 10, 12, 9, 30, 0)
                .single()
                .expect("a time")
        );
        assert_eq!(formed.activity.sessions, 1);
        assert_eq!(formed.activity.tokens["m"].input_tokens, 3);
        assert_eq!(formed.activity.tokens["m"].output_tokens, 3);
        assert_eq!(
            (formed.services[0].name.as_str(), formed.services[0].calls),
            ("Zoho Books", 3)
        );
        assert_eq!(found.not_a_project.sessions, 1);
        assert_eq!(found.not_a_project.tokens["m"].input_tokens, 5);
    }

    #[test]
    fn leaves_catervas_sessions_out_of_services() {
        let (_temp, index) = open_seeded("leaves-catervas-out");
        let c = &index.connection;
        seed_project(c, "/home/me/shop", true);
        seed_session_at(
            c,
            "s1",
            "catervas",
            Some("/home/me/shop"),
            "2026-10-12T09:00:00.000Z",
        );
        seed_reply(c, "r1", "s1", 1);
        seed_call(
            c,
            "t1",
            "r1",
            "mcp__claude_ai_Zoho_Books__do",
            false,
            "2026-10-12T09:00:00.000Z",
        );
        let found = index.found_services().expect("services");
        assert!(found.services.is_empty());
        assert!(found.by_root.is_empty());
    }

    #[test]
    fn tells_a_folder_of_spreadsheets() {
        let (_temp, index) = open_seeded("tells-a-folder-of-spreadsheets");
        let c = &index.connection;
        seed_project(c, "/home/me/books", false);
        seed_project(c, "/home/me/app", true);
        seed_session_at(
            c,
            "b",
            "owner",
            Some("/home/me/books"),
            "2026-10-12T09:00:00.000Z",
        );
        seed_session_at(
            c,
            "a",
            "owner",
            Some("/home/me/app"),
            "2026-10-12T09:00:00.000Z",
        );
        seed_reply(c, "rb", "b", 1);
        seed_reply(c, "ra", "a", 1);
        let sheets = |count: usize, reply: &str, prefix: &str| {
            for i in 0..count {
                seed_call(
                    c,
                    &format!("{prefix}{i}"),
                    reply,
                    "Read",
                    true,
                    "2026-10-12T09:00:00.000Z",
                );
            }
        };
        sheets(3, "rb", "b");
        sheets(5, "ra", "a");
        let found = index.found_services().expect("services");
        assert_eq!(
            found.bookkeeping_roots,
            BTreeSet::from(["/home/me/books".to_string()])
        );
        c.execute("DELETE FROM tool_calls WHERE tool_use_id = 'b0'", [])
            .expect("one call fewer");
        assert!(
            index
                .found_services()
                .expect("services")
                .bookkeeping_roots
                .is_empty()
        );
    }

    fn github_seen(full_name: &str, pushed: &str, work: Vec<ClaudeWork>) -> GithubSeen {
        GithubSeen {
            full_name: full_name.to_string(),
            private: true,
            pushed_at: DateTime::parse_from_rfc3339(pushed)
                .expect("a time")
                .with_timezone(&Utc),
            default_branch: "main".to_string(),
            work,
        }
    }

    #[test]
    fn keeps_github_repositories_once_beside_folders() {
        let temp = TempProject::new("keeps-github");
        let path = temp.root.join(MACHINE_DB);
        let mut index = open_machine_index(&path, now()).expect("the index opens");
        assert_eq!(
            index.applied_migrations().expect("versions").last(),
            Some(&10)
        );
        let c = &index.connection;
        seed_project(c, "/home/me/shop", true);
        c.execute(
            "UPDATE projects SET remote = 'github.com/octo/shop' WHERE root = '/home/me/shop'",
            [],
        )
        .expect("a remote");
        seed_session_at(
            c,
            "s1",
            "owner",
            Some("/home/me/shop"),
            "2026-10-12T09:00:00.000Z",
        );
        let branch = ClaudeWork {
            kind: WorkKind::Branch,
            name: "claude/fix-login".to_string(),
            number: None,
            draft: None,
            updated_at: None,
        };
        let pull = ClaudeWork {
            kind: WorkKind::Pull,
            name: "Fix the login".to_string(),
            number: Some(12),
            draft: Some(true),
            updated_at: Some("2026-10-20T09:00:00Z".to_string()),
        };
        index
            .replace_github(
                &[
                    github_seen("octo/Shop", "2026-10-11T09:00:00Z", vec![branch.clone()]),
                    github_seen(
                        "octo/notes",
                        "2026-10-10T09:00:00Z",
                        vec![branch.clone(), pull.clone()],
                    ),
                ],
                later(1),
            )
            .expect("a read");

        // The repository a folder has is that project, sourced github too; no second project.
        let found = index.found().expect("found");
        assert_eq!(found.projects.len(), 1);
        assert!(found.projects[0].sources.contains(&Source::Github));
        assert!(found.projects[0].sources.contains(&Source::Local));
        assert_eq!(found.projects[0].claude_work, vec![branch.clone()]);
        // The one with no folder is a github project with its Claude work.
        assert_eq!(found.github.len(), 1);
        assert_eq!(found.github[0].full_name, "octo/notes");
        assert_eq!(found.github[0].remote_key, "github.com/octo/notes");
        assert!(!found.github[0].not_seen_lately);
        assert_eq!(found.github[0].claude_work.len(), 2);
        assert!(found.github[0].claude_work.contains(&pull));

        // A later read without it keeps it, marked not seen lately; the work of a repository the
        // read did list is replaced.
        index
            .replace_github(
                &[github_seen("octo/Shop", "2026-10-12T09:00:00Z", Vec::new())],
                later(2),
            )
            .expect("a later read");
        let found = index.found().expect("found");
        assert!(found.projects[0].claude_work.is_empty());
        assert_eq!(found.github.len(), 1);
        assert!(found.github[0].not_seen_lately);
        assert_eq!(found.github[0].claude_work.len(), 2, "kept as it was");
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

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one test of the owner's answers, each step reading the one before"
    )]
    fn keeps_the_owner_s_answers() {
        let temp = TempProject::new("keeps-owner-answers");
        let path = temp.root.join(MACHINE_DB);
        let mut index = open_machine_index(&path, now()).expect("the index opens");
        // A choice survives a reopen, and the last one wins.
        index
            .put_choice("github.com/me/shop", Choice::Watch, now())
            .expect("a choice");
        index
            .put_choice("/home/me/blog", Choice::Ignore, now())
            .expect("a choice");
        index
            .put_choice("github.com/me/shop", Choice::Team, later(1))
            .expect("a choice again");
        drop(index);
        let mut index = open_machine_index(&path, now()).expect("it opens again");
        let choices = index.choices().expect("choices");
        assert_eq!(choices["github.com/me/shop"], Choice::Team);
        assert_eq!(choices["/home/me/blog"], Choice::Ignore);

        // Two folders with one remote are two projects once the owner separates them.
        let c = &index.connection;
        for root in ["/home/me/shop", "/home/me/shop-copy"] {
            seed_project(c, root, true);
            c.execute(
                "UPDATE projects SET remote = 'github.com/me/shop' WHERE root = ?1",
                (root,),
            )
            .expect("a remote");
            seed_session_at(c, root, "owner", Some(root), "2026-10-12T09:00:00.000Z");
        }
        let found = index.found().expect("found");
        let keys = |found: &Found| -> Vec<(String, Option<String>)> {
            found
                .projects
                .iter()
                .map(|project| (project.root.clone(), project.remote.clone()))
                .collect()
        };
        let folded = |index: &MachineIndex| {
            let found = index.found().expect("found");
            let owned = keys(&found);
            let projects: Vec<ProjectKey<'_>> = owned
                .iter()
                .map(|(root, remote)| ProjectKey {
                    root,
                    remote: remote.as_deref(),
                })
                .collect();
            let separated = index.separations().expect("separations");
            fold_by_remote(&projects, &separated).len()
        };
        assert_eq!(found.projects.len(), 2);
        assert_eq!(folded(&index), 1);
        index
            .separate("/home/me/shop-copy", "/home/me/shop")
            .expect("a separation");
        assert_eq!(
            index.separations().expect("separations"),
            vec![(
                "/home/me/shop".to_string(),
                "/home/me/shop-copy".to_string()
            )]
        );
        assert_eq!(index.found().expect("found").projects.len(), 2);
        assert_eq!(folded(&index), 2);

        // A move places the old path's sessions under the new folder.
        let c = &index.connection;
        for (id, cwd) in [("m1", "/gone/shop/api"), ("m2", "/gone/shopping")] {
            c.execute(
                "INSERT INTO sessions (session_id, cwd) VALUES (?1, ?2)",
                (id, cwd),
            )
            .expect("a session");
        }
        index
            .put_move("/gone/shop", "/home/me/shop")
            .expect("a move");
        let folders: BTreeMap<String, String> = index
            .session_folders()
            .expect("folders")
            .into_iter()
            .filter(|(id, _)| id.starts_with('m'))
            .collect();
        assert_eq!(folders["m1"], "/home/me/shop/api");
        assert_eq!(folders["m2"], "/gone/shopping");

        // Declaring Zoho Books forms a bookkeeping project; a name with no catalog id forms none.
        index
            .replace_declared_services(
                &[
                    ("zoho_books".to_string(), "Zoho Books".to_string()),
                    ("other:My ledger".to_string(), "My ledger".to_string()),
                ],
                now(),
            )
            .expect("declared");
        let formed = index.found_services().expect("services").formed;
        assert_eq!(formed.len(), 1);
        assert_eq!(formed[0].name, "Bookkeeping (Zoho Books)");
        assert!(formed[0].declared);

        // A routine by hand shows under its project.
        let id = index
            .add_hand_routine(
                "Month end",
                Some("/home/me/shop"),
                "every Monday at 9",
                now(),
            )
            .expect("a routine");
        let routines = index.hand_routines().expect("routines");
        assert_eq!(routines.len(), 1);
        assert_eq!(routines[0].id, id);
        assert_eq!(routines[0].project_key.as_deref(), Some("/home/me/shop"));
        assert_eq!(routines[0].schedule, "every Monday at 9");
    }
}
