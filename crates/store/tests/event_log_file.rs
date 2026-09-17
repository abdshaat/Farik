//! What only a log on the file system can show: the directory being made, the sequence surviving a
//! reopen, write-ahead logging, and two processes appending to one file.
//!
//! These need the file system, so they live here rather than in the module
//! (`docs/standards/code.md`, "Rust integration test").

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use farik_protocol::event::EventKind;
use farik_protocol::event::fixtures::a_new_event as an_event;
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
    // And the ids the store hands out carry on too.
    assert_eq!(reopened.next_task_id().expect("an id").to_string(), "FRK-1");
}

#[test]
fn keeps_two_logs_on_one_file_from_sharing_a_place_or_an_id() {
    // Two commands can run at once, and `farik` is a separate process each time.
    let directory = TempDir::new("two-logs");
    let first = open_event_log(&directory.db(), at(9)).expect("the first log opens");
    let second = open_event_log(&directory.db(), at(9)).expect("the second log opens");
    let mut places = Vec::new();
    for _ in 0..2 {
        places.push(
            first
                .append(&an_event(EventKind::TaskCreated))
                .expect("appends")
                .envelope
                .seq,
        );
        places.push(
            second
                .append(&an_event(EventKind::TeamUpdated))
                .expect("appends")
                .envelope
                .seq,
        );
    }
    assert_eq!(places, [1, 2, 3, 4]);
    let ids = [
        first.next_task_id().expect("an id").to_string(),
        second.next_task_id().expect("an id").to_string(),
        first.next_task_id().expect("an id").to_string(),
    ];
    assert_eq!(ids, ["FRK-1", "FRK-2", "FRK-3"]);
    // Both connections see the whole log, whichever of them wrote each event.
    assert_eq!(second.read(&EventQuery::default()).expect("reads").len(), 4);
}

#[test]
fn writes_ahead_of_the_database_file() {
    // Write-ahead logging is what makes `synchronous = FULL` affordable, and it is a property of
    // the file, so another connection can read it back.
    //
    // `synchronous` is not asserted here, and no test can pin it: it is per connection, and `FULL`
    // is already the compiled-in default of the bundled SQLite, so setting it changes nothing this
    // build can observe. It is set anyway, because a build whose default is `NORMAL` would
    // otherwise lose an append that had returned.
    let directory = TempDir::new("writes-ahead");
    let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
    log.append(&an_event(EventKind::TaskCreated))
        .expect("appends");
    let connection = rusqlite::Connection::open(directory.db()).expect("another connection");
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the journal mode reads");
    assert_eq!(mode.to_lowercase(), "wal");
}

/// How many `farik` commands the two tests below start at once.
const PROCESSES: u32 = 16;

/// Enough ids per process that a counter which read the number it had not yet written hands the
/// same one out twice, and few enough that the test is over in well under a second.
const IDS_PER_PROCESS: u32 = 125;

/// How many fresh files the opening race is run against. One run catches a migration applied twice
/// most of the time; several make it near certain, and each costs a tenth of a second.
const OPENING_ATTEMPTS: u32 = 5;

#[test]
fn opens_one_log_from_several_processes_at_once() {
    // Opening applies whatever migrations the file is missing, and several farik commands can start
    // on a fresh repository at the same moment. Each has to end up with a log it can use, rather
    // than one of them meeting another half way through the migration: the loser of that race sees
    // either "table events already exists" or "database is locked", because a transaction that
    // takes its read lock first cannot wait for the write lock it turns out to need.
    for attempt in 0..OPENING_ATTEMPTS {
        let directory = TempDir::new(&format!("opened-at-once-{attempt}"));
        let ready = std::sync::Barrier::new(PROCESSES as usize);
        std::thread::scope(|scope| {
            for _ in 0..PROCESSES {
                scope.spawn(|| {
                    ready.wait();
                    let log = open_event_log(&directory.db(), at(9))
                        .unwrap_or_else(|refusal| panic!("the log opens: {refusal}"));
                    assert_eq!(
                        log.applied_migrations().expect("the ledger reads"),
                        farik_store::migrations::known_versions()
                    );
                });
            }
        });
    }
}

#[test]
fn hands_two_processes_on_one_file_a_different_id_every_time() {
    // Two farik commands run at once, each its own process with its own connection. The counter's
    // read is its write statement, so neither can be handed a number the other has taken; reading
    // the number before writing it is the classic lost update, and two agents would start on one
    // contract.
    let directory = TempDir::new("two-logs-at-once");
    let logs: Vec<_> = (0..PROCESSES)
        .map(|_| open_event_log(&directory.db(), at(9)).expect("the log opens"))
        .collect();
    let ready = std::sync::Barrier::new(PROCESSES as usize);
    let taken: Vec<String> = std::thread::scope(|scope| {
        let processes: Vec<_> = logs
            .iter()
            .map(|log| {
                scope.spawn(|| {
                    ready.wait();
                    (0..IDS_PER_PROCESS)
                        .map(|_| log.next_task_id().expect("an id").to_string())
                        .collect::<Vec<String>>()
                })
            })
            .collect();
        processes
            .into_iter()
            .flat_map(|process| process.join().expect("the process finishes"))
            .collect()
    });
    let mut numbers: Vec<u64> = taken
        .iter()
        .map(|id| {
            id.strip_prefix("FRK-")
                .expect("every id carries the prefix")
                .parse()
                .expect("and a number")
        })
        .collect();
    numbers.sort_unstable();
    let every_number: Vec<u64> = (1..=u64::from(PROCESSES * IDS_PER_PROCESS)).collect();
    assert_eq!(
        numbers, every_number,
        "each id was handed out exactly once, in one unbroken run"
    );
}
