//! The machine-level index of the owner's Claude Code history, apart from every project's log.

use std::path::{Path, PathBuf};
use std::time::Duration;

use catervas_core::pricing::Usage;
use chrono::{DateTime, SecondsFormat, Utc};
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
}

/// Everything one file adds to the index, written in one transaction.
pub(crate) struct FileRead<'a> {
    pub path: &'a str,
    pub state: FileState,
    pub sessions: Vec<SessionBatch>,
    pub titles: Vec<(String, String)>,
    pub replies: Vec<ReplyBatch>,
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

    /// Writes one file's sessions, titles, replies and its new offset, all or none.
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
        assert_eq!(index.applied_migrations().expect("versions"), vec![1]);
        drop(index);
        let again = open_machine_index(&path, now()).expect("it opens again");
        assert_eq!(again.applied_migrations().expect("versions"), vec![1]);
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
            vec!["projects", "replies", "sessions", "transcript_files"]
        );
        assert_eq!(
            crate::migrations::known_versions(),
            (1..=15).collect::<Vec<i64>>()
        );
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
