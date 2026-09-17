//! The event log: every action the team takes, in the order it happened, never changed afterwards
//! (`docs/SPEC.md` sections 5.1 and 8.4).

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::error::StoreError;
use crate::migrations;

/// The path that opens a database in memory rather than on disk, for tests and for a dry run.
pub const IN_MEMORY: &str = ":memory:";

/// An append-only log of everything that happened, with the projections' counters beside it.
///
/// One connection, behind a lock: SQLite serialises writers anyway, and a lock here means the
/// sequence number a caller is handed is the one its own insert produced rather than another
/// thread's. `append` takes `&self` so that the log can be shared.
pub struct EventLog {
    connection: Mutex<Connection>,
}

/// Opens the log at `path`, making its directory and bringing its shape up to date, and returns it
/// ready to append to. `IN_MEMORY` opens a database that lives only as long as the value returned.
///
/// Opening an existing log applies whatever migrations it is missing and nothing else, so opening
/// is safe to do on every command.
///
/// # Errors
///
/// `Io` when the directory cannot be made; `Sqlite` when the file cannot be opened or a migration
/// fails.
pub fn open_event_log(path: &Path, now: DateTime<Utc>) -> Result<EventLog, StoreError> {
    let in_memory = path == Path::new(IN_MEMORY);
    if !in_memory
        && let Some(directory) = path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
    {
        std::fs::create_dir_all(directory)?;
    }
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrations::apply(&mut connection, now)?;
    Ok(EventLog {
        connection: Mutex::new(connection),
    })
}

impl EventLog {
    /// The migration versions this log has applied, in order.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the ledger cannot be read.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        let mut versions = Vec::new();
        for row in rows {
            versions.push(row?);
        }
        Ok(versions)
    }

    /// The connection, recovering from a lock another thread poisoned by panicking. A panic
    /// somewhere else says nothing about this database, and refusing every later append because of
    /// it would turn one bug into a stopped team.
    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::{DateTime, EventLog, IN_MEMORY, Path, Utc, open_event_log};
    use crate::migrations;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn a_log() -> EventLog {
        open_event_log(Path::new(IN_MEMORY), at(9)).expect("a log in memory opens")
    }

    #[test]
    fn opens_a_log_and_records_every_migration_it_applied() {
        let log = a_log();
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
    }

    #[test]
    fn applies_a_migration_once_however_often_the_log_is_opened() {
        // Opening is done on every command, so it has to be free of consequences the second time.
        let log = a_log();
        let mut connection = log.connection.lock().expect("a fresh lock");
        migrations::apply(&mut connection, at(10)).expect("applying again is a no-op");
        drop(connection);
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
    }

    #[test]
    fn refuses_to_change_or_delete_an_event() {
        // Append-only is the engine's rule, not only this module's: a repair script and a person
        // with the sqlite3 shell meet the same wall. The row goes in by hand, so the rule holds
        // from the migration that makes the trigger, before anything can append.
        let log = a_log();
        let connection = log.connection.lock().expect("a fresh lock");
        connection
            .execute(
                "INSERT INTO events
                     (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                 VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', NULL, NULL, NULL,
                         'team.updated', '{}')",
                (),
            )
            .expect("a row goes in");
        for sql in [
            "UPDATE events SET team_id = 'other' WHERE seq = 1",
            "DELETE FROM events WHERE seq = 1",
        ] {
            let refusal = connection.execute(sql, ()).expect_err("append-only");
            assert!(
                refusal.to_string().contains("append-only"),
                "{sql}: {refusal}"
            );
        }
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM events", (), |row| row.get(0))
            .expect("the count reads");
        assert_eq!(rows, 1);
    }
}
