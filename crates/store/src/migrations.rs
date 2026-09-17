//! The database's shape, as SQL applied in order and recorded once applied.

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, TransactionBehavior};

use crate::error::StoreError;

/// One migration: its version and the SQL that applies it.
struct Migration {
    version: i64,
    sql: &'static str,
}

/// Every migration, in the order they apply. A migration is never edited once it has shipped; a
/// change to the shape is a new one, so that a database written by an older Farik reaches the same
/// shape as one made today.
const MIGRATIONS: [Migration; 1] = [Migration {
    version: 1,
    sql: include_str!("migrations/0001_event_log.sql"),
}];

/// Brings the database to the shape this version expects, and records what it applied. Applying to
/// a database that is already current does nothing, so opening a log twice is not an error, and
/// neither is opening it from several processes at the same moment.
///
/// Each migration, the ledger it is read from, and the record of it go in one transaction that
/// takes the write lock before it reads: a migration that fails leaves the database as it was
/// rather than half-migrated, which is the state nothing knows how to repair.
///
/// # Errors
///
/// `Sqlite` when a migration or its record fails.
pub(crate) fn apply(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        ) STRICT;",
    )?;
    for migration in &MIGRATIONS {
        // `Immediate` takes the write lock before the ledger is read rather than after. A
        // transaction that reads first and then finds it needs to write cannot wait for the lock,
        // because another reader may be waiting to write the same rows, so SQLite refuses it at
        // once instead of after `busy_timeout` — and two processes that both read "not applied"
        // both run the migration, and the loser is told the tables already exist. Several `farik`
        // commands starting on a fresh repository at the same moment is the ordinary case.
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if is_applied(&transaction, migration.version)? {
            continue;
        }
        transaction.execute_batch(migration.sql)?;
        // A plain insert, not `INSERT OR REPLACE`: the write lock is held, so the row cannot
        // already be there, and a ledger that disagrees with the tables should fail loudly rather
        // than have its `applied_at` quietly rewritten.
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            (
                migration.version,
                now.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            ),
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn is_applied(connection: &Connection, version: i64) -> Result<bool, StoreError> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM schema_migrations WHERE version = ?1",
        (version,),
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// The versions this build knows about, in order. `open_event_log` has applied every one of them.
#[must_use]
pub fn known_versions() -> Vec<i64> {
    MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .collect()
}
