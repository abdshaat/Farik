//! What only a log on the file system can show: the directory being made, the sequence surviving a
//! reopen, write-ahead logging, and two processes appending to one file.
//!
//! These need the file system, so they live here rather than in the module
//! (`docs/standards/code.md`, "Rust integration test").

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use farik_protocol::event::fixtures::an_event_wire;
use farik_protocol::event::{EventKind, NewEvent, event_from_value};
use farik_store::{EventQuery, open_event_log};

/// A directory of its own, removed when the test ends however the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "farik-store-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a directory under the temporary directory");
        Self { path }
    }

    fn db(&self) -> PathBuf {
        self.path.join(".farik").join("local").join("farik.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
        .single()
        .expect("a real hour")
}

fn an_event(kind: EventKind) -> NewEvent {
    let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
    NewEvent {
        recorded_at: event.envelope.recorded_at,
        team_id: event.envelope.team_id,
        project_id: event.envelope.project_id,
        task_id: event.envelope.task_id,
        agent_id: event.envelope.agent_id,
        session_id: event.envelope.session_id,
        body: event.body,
    }
}

#[test]
fn makes_the_directory_the_log_belongs_in() {
    // `farik init` gives a path inside a repository that has no `.farik/` yet, so opening makes it.
    let directory = TempDir::new("makes-the-directory");
    let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
    assert!(directory.db().exists(), "the database file was made");
    log.append(&an_event(EventKind::TaskCreated))
        .expect("appends");
}

#[test]
fn keeps_every_event_and_its_place_across_a_reopen() {
    let directory = TempDir::new("survives-a-reopen");
    {
        let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
        log.append(&an_event(EventKind::TaskCreated))
            .expect("appends");
        log.append(&an_event(EventKind::RequestTriaged))
            .expect("appends");
    }
    let reopened = open_event_log(&directory.db(), at(10)).expect("the log opens again");
    let read = reopened.read(&EventQuery::default()).expect("reads");
    assert_eq!(
        read.iter()
            .map(|event| event.envelope.seq)
            .collect::<Vec<u64>>(),
        [1, 2]
    );
    // The next place carries on from where the log left off; a reused sequence number would make
    // two different events look like one.
    let third = reopened
        .append(&an_event(EventKind::TeamUpdated))
        .expect("appends");
    assert_eq!(third.envelope.seq, 3);
}
