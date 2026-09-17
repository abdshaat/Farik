//! The board, read from the log rather than scanned out of it (`docs/SPEC.md` sections 8.4 and 10).

use std::sync::Arc;

use rusqlite::Connection;

use crate::error::StoreError;
use crate::event_log::EventLog;

/// The projections of one log: derived tables that answer a view in one query.
///
/// They share the log's connection and its lock, so a view cannot read a half-written append, and
/// a log opened in memory can be projected at all.
pub struct Projections {
    log: Arc<EventLog>,
}

/// Opens the projections of `log`.
///
/// # Errors
///
/// `Sqlite` when a projection table cannot be read.
pub fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError> {
    Ok(Projections { log })
}

impl Projections {
    /// How far into the log the projections have read: the sequence number of the last event
    /// applied, or zero when none has been.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the cursor cannot be read.
    pub fn cursor(&self) -> Result<u64, StoreError> {
        let connection = self.connection();
        read_cursor(&connection)
    }

    /// The log's own connection: the projections live in the same database, and share its lock so
    /// that a view cannot read a half-written append.
    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.log.connection()
    }
}

fn read_cursor(connection: &Connection) -> Result<u64, StoreError> {
    let seq: i64 = connection
        .query_row(
            "SELECT seq FROM projection_cursor WHERE id = 1",
            (),
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(0),
            other => Err(StoreError::from(other)),
        })?;
    u64::try_from(seq).map_err(|_| StoreError::Sqlite {
        detail: "the projection cursor is negative, which no event can produce".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};

    use super::{Arc, EventLog, Projections, open_projections};
    use crate::event_log::{IN_MEMORY, open_event_log};
    use crate::migrations;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn a_log() -> Arc<EventLog> {
        Arc::new(
            open_event_log(std::path::Path::new(IN_MEMORY), at(9)).expect("a log in memory opens"),
        )
    }

    /// A log and the projections of it, both empty.
    fn a_board() -> (Arc<EventLog>, Projections) {
        let log = a_log();
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        (log, projections)
    }

    #[test]
    fn opens_the_projections_of_a_log_that_has_read_nothing_yet() {
        let (log, projections) = a_board();
        assert_eq!(projections.cursor().expect("the cursor reads"), 0);
        // The projection tables are part of the log's own database, applied as a migration like
        // everything else in it, so opening the log is what makes them.
        assert_eq!(
            log.applied_migrations().expect("the ledger reads"),
            migrations::known_versions()
        );
        assert_eq!(migrations::known_versions(), vec![1, 2]);
    }
}
