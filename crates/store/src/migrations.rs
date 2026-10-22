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
/// change to the shape is a new one, so that a database written by an older Catervas reaches the same
/// shape as one made today.
const MIGRATIONS: [Migration; 16] = [
    Migration {
        version: 1,
        sql: include_str!("migrations/0001_event_log.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/0002_projections.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/0003_costs.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("migrations/0004_transitions.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("migrations/0005_integration.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("migrations/0006_human.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("migrations/0007_metrics.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("migrations/0008_sprints.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("migrations/0009_sprint_membership.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("migrations/0010_backlog_mark.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("migrations/0011_events_by_agent_and_kind.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("migrations/0012_tool_approvals.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("migrations/0013_marketing_plans.sql"),
    },
    Migration {
        version: 14,
        sql: include_str!("migrations/0014_site_requests.sql"),
    },
    Migration {
        version: 15,
        sql: include_str!("migrations/0015_skips_sprints.sql"),
    },
    Migration {
        version: 16,
        sql: include_str!("migrations/0016_counted_seq.sql"),
    },
];

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
    let last = MIGRATIONS.last().map_or(0, |migration| migration.version);
    apply_through(connection, last, now)
}

/// `apply`, stopping after the migration numbered `last_version`: the shape an older Catervas left,
/// which is what a test of a later migration starts from.
///
/// # Errors
///
/// As `apply`.
pub(crate) fn apply_through(
    connection: &mut Connection,
    last_version: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    apply_list(connection, &MIGRATIONS, last_version, now)
}

/// The migrations of the machine-level index (`machine.db`), numbered apart from a project log's so
/// the two never meet in one folder.
const MACHINE_MIGRATIONS: [Migration; 11] = [
    Migration {
        version: 1,
        sql: include_str!("migrations/machine/0001_history.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/machine/0002_jobs.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/machine/0003_services.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("migrations/machine/0004_claude_export.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("migrations/machine/0005_copies.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("migrations/machine/0006_by_hand.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("migrations/machine/0007_finding.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("migrations/machine/0008_github.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("migrations/machine/0009_links.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("migrations/machine/0010_proposal.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("migrations/machine/0011_reminders.sql"),
    },
];

/// Brings the machine-level index to the shape this version expects.
///
/// # Errors
///
/// As `apply`.
pub(crate) fn apply_machine(
    connection: &mut Connection,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let last = MACHINE_MIGRATIONS
        .last()
        .map_or(0, |migration| migration.version);
    apply_list(connection, &MACHINE_MIGRATIONS, last, now)
}

fn apply_list(
    connection: &mut Connection,
    list: &[Migration],
    last_version: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        ) STRICT;",
    )?;
    for migration in list
        .iter()
        .take_while(|migration| migration.version <= last_version)
    {
        // `Immediate` takes the write lock before the ledger is read rather than after. A
        // transaction that reads first and then finds it needs to write cannot wait for the lock,
        // because another reader may be waiting to write the same rows, so SQLite refuses it at
        // once instead of after `busy_timeout` — and two processes that both read "not applied"
        // both run the migration, and the loser is told the tables already exist. Several `catervas`
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn adds_the_jobs_table_and_reads_every_transcript_again() {
        let now = Utc
            .with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time");
        let mut connection = Connection::open_in_memory().expect("a database");
        apply_list(&mut connection, &MACHINE_MIGRATIONS, 1, now).expect("version 1");
        connection
            .execute(
                "INSERT INTO transcript_files (path, read_to, turns, unreadable_lines, read_at)
                 VALUES ('a.jsonl', 500, 6, 2, '2026-10-12T09:00:00.000Z')",
                [],
            )
            .expect("a transcript file");
        apply_list(&mut connection, &MACHINE_MIGRATIONS, 2, now).expect("version 2");
        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert_eq!(versions, vec![1, 2]);
        let jobs: i64 = connection
            .query_row("SELECT count(*) FROM jobs", [], |row| row.get(0))
            .expect("the jobs table exists");
        assert_eq!(jobs, 0);
        let file: (i64, i64, i64) = connection
            .query_row(
                "SELECT read_to, turns, unreadable_lines FROM transcript_files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("the row stays");
        assert_eq!(file, (0, 0, 0));
    }

    #[test]
    fn adds_the_services_tables_and_reads_every_transcript_again() {
        let now = Utc
            .with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time");
        let mut connection = Connection::open_in_memory().expect("a database");
        apply_list(&mut connection, &MACHINE_MIGRATIONS, 2, now).expect("version 2");
        connection
            .execute(
                "INSERT INTO transcript_files (path, read_to, turns, unreadable_lines, read_at)
                 VALUES ('a.jsonl', 500, 6, 2, '2026-10-12T09:00:00.000Z')",
                [],
            )
            .expect("a transcript file");
        apply_list(&mut connection, &MACHINE_MIGRATIONS, 3, now).expect("version 3");
        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert_eq!(versions, vec![1, 2, 3]);
        for table in ["tool_calls", "configured_servers"] {
            let rows: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("the table exists");
            assert_eq!(rows, 0);
        }
        let file: (i64, i64, i64) = connection
            .query_row(
                "SELECT read_to, turns, unreadable_lines FROM transcript_files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("the row stays");
        assert_eq!(file, (0, 0, 0));
    }
}
