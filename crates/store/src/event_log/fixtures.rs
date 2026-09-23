//! A log that refuses one kind of event, for tests in this crate and in others that need an
//! append to fail where the code under test cannot otherwise be made to fail it.

use farik_protocol::event::EventKind;

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
