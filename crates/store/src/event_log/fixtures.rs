//! A log that refuses one kind of event, for tests in this crate and in others that need an
//! append to fail where the code under test cannot otherwise be made to fail it.

use catervas_protocol::event::EventKind;

use super::EventLog;

/// Makes `log` refuse every later append of an event of `kind`, as a store that has stopped
/// taking writes would, while it goes on taking every other kind.
///
/// # Panics
///
/// When the database will not take the trigger, which is the fixture failing rather than the code.
pub fn refuse_appends_of(log: &EventLog, kind: EventKind) {
    log.connection()
        .execute_batch(&format!(
            "CREATE TRIGGER refuse_{name} BEFORE INSERT ON events WHEN NEW.kind = '{kind}'
             BEGIN SELECT RAISE(ABORT, 'this log refuses {kind}'); END;",
            name = kind.to_string().replace('.', "_"),
        ))
        .expect("the trigger is made");
}

/// Makes every later projection of an event fail, as a database whose projection tables can no
/// longer be written would, while the log goes on taking every append: how a test shows what a
/// caller does with an event that was recorded and not projected.
///
/// # Panics
///
/// When the database will not take the triggers, which is the fixture failing rather than the code.
pub fn refuse_projecting(log: &EventLog) {
    log.connection()
        .execute_batch(
            "CREATE TRIGGER refuse_projecting_insert BEFORE INSERT ON projection_cursor
             BEGIN SELECT RAISE(ABORT, 'this log refuses to project'); END;
             CREATE TRIGGER refuse_projecting_update BEFORE UPDATE ON projection_cursor
             BEGIN SELECT RAISE(ABORT, 'this log refuses to project'); END;",
        )
        .expect("the triggers are made");
}

/// Appends a row of `kind` for `agent_id` whose body no reader accepts, so that a read that
/// reaches it fails: how a test shows that a query never reads that far back.
///
/// # Panics
///
/// When the database will not take the row, which is the fixture failing rather than the code.
pub fn append_unreadable(log: &EventLog, agent_id: &str, kind: EventKind) {
    log.connection()
        .execute(
            "INSERT INTO events (recorded_at, team_id, project_id, agent_id, kind, body)
             VALUES ('2026-09-17T12:00:00Z', 'catervas', 'catervas', ?1, ?2, '{}')",
            (agent_id, kind.to_string()),
        )
        .expect("the row is written");
}
