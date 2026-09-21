//! The files against the log, in a real project.
//!
//! These need somewhere to write and nothing else, so they run in the default `cargo xtask check`.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use farik_core::contract::{TaskContract, TaskId, TaskStatus, validate_contract};
use farik_protocol::event::fixtures::{a_contract_summary_wire, an_event_wire};
use farik_protocol::event::{EventKind, NewEvent, event_from_value};
use farik_store::files::ProjectFiles;
use farik_store::files::fixtures::TempProject;
use farik_store::{
    Drift, EventLog, IN_MEMORY, Projections, ReconcileError, open_event_log, open_projections,
};
use serde_json::json;

/// A log and the projections of it, both empty.
fn a_board() -> (Arc<EventLog>, Projections) {
    let at = Utc
        .with_ymd_and_hms(2026, 9, 17, 9, 0, 0)
        .single()
        .expect("a real hour");
    let log = Arc::new(
        open_event_log(std::path::Path::new(IN_MEMORY), at).expect("a log in memory opens"),
    );
    let projections = open_projections(Arc::clone(&log)).expect("the projections open");
    (log, projections)
}

/// Puts a task on the board at a status, the way a governed transition does.
fn on_the_board(
    log: &EventLog,
    projections: &Projections,
    task_id: &str,
    status: &str,
    locked: bool,
) {
    let mut summary = a_contract_summary_wire();
    summary["status"] = json!(status);
    let mut wire = an_event_wire(EventKind::ContractWritten);
    wire["task_id"] = json!(task_id);
    wire["body"]["summary"] = summary;
    let event = event_from_value(&wire).expect("the fixture is schema-valid");
    let written = NewEvent {
        recorded_at: event.envelope.recorded_at,
        ids: event.envelope.ids,
        body: event.body,
    };
    projections
        .apply(&log.append(&written).expect("appends"))
        .expect("projects");
    if locked {
        let mut wire = an_event_wire(EventKind::ContractLocked);
        wire["task_id"] = json!(task_id);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        let held = NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        };
        projections
            .apply(&log.append(&held).expect("appends"))
            .expect("projects");
    }
}

/// A contract of that id, at that status, held or not.
fn a_contract(id: &str, status: TaskStatus, locked: bool) -> TaskContract {
    let mut wire = farik_core::contract::fixtures::a_contract_wire();
    wire["id"] = json!(id);
    wire["status"] = json!(status.to_string());
    wire["locked"] = json!(locked);
    validate_contract(&wire).expect("the fixture is a contract")
}

fn an_id(id: &str) -> TaskId {
    id.parse().expect("a task id")
}

fn drifts(files: &ProjectFiles, projections: &Projections) -> Vec<Drift> {
    farik_store::reconcile(files, projections).expect("it reconciles")
}

#[test]
fn a_project_whose_files_and_log_agree_has_nothing_to_report() {
    let project = TempProject::new("reconcile-agree");
    let files = project.files();
    let (log, projections) = a_board();
    for id in ["FRK-1", "FRK-2"] {
        on_the_board(&log, &projections, id, "ready", false);
        files
            .write_contract(&a_contract(id, TaskStatus::Ready, false))
            .expect("written");
    }
    assert_eq!(drifts(&files, &projections), []);
}

#[test]
fn says_when_there_is_a_contract_the_log_has_never_heard_of() {
    let project = TempProject::new("reconcile-orphan-file");
    let files = project.files();
    let (_log, projections) = a_board();
    files
        .write_contract(&a_contract("FRK-7", TaskStatus::Draft, false))
        .expect("written");

    let found = drifts(&files, &projections);
    assert_eq!(found.len(), 1, "{found:?}");
    let Drift::ContractWithoutEvents { task_id, detail } = &found[0] else {
        panic!("a file nothing created: {found:?}");
    };
    assert_eq!(task_id, &an_id("FRK-7"));
    assert!(detail.contains("never heard of this task"), "{detail}");
    // What a person is shown, which is `Drift`'s own two accessors and its `Display`.
    assert_eq!(found[0].task_id(), &an_id("FRK-7"));
    assert_eq!(found[0].detail(), detail);
    assert_eq!(
        found[0].to_string(),
        "FRK-7: there is a contract file and the log has never heard of this task, so no transition \
         of it was ever governed"
    );
}

#[test]
fn says_when_the_log_knows_a_task_with_no_contract_to_work_from() {
    let project = TempProject::new("reconcile-orphan-log");
    let files = project.files();
    let (log, projections) = a_board();
    on_the_board(&log, &projections, "FRK-3", "assigned", false);

    let found = drifts(&files, &projections);
    assert_eq!(found.len(), 1, "{found:?}");
    let Drift::EventsWithoutContract { task_id, detail } = &found[0] else {
        panic!("a task with no contract: {found:?}");
    };
    assert_eq!(task_id, &an_id("FRK-3"));
    assert!(detail.contains("assigned"), "{detail}");
    assert!(detail.contains("no contract file"), "{detail}");
}

#[test]
fn says_when_the_file_and_the_log_disagree_about_where_a_task_is() {
    let project = TempProject::new("reconcile-status");
    let files = project.files();
    let (log, projections) = a_board();
    on_the_board(&log, &projections, "FRK-1", "accepted", false);
    files
        .write_contract(&a_contract("FRK-1", TaskStatus::InProgress, false))
        .expect("written");

    let found = drifts(&files, &projections);
    assert_eq!(found.len(), 1, "{found:?}");
    let Drift::StatusMismatch { task_id, detail } = &found[0] else {
        panic!("two answers about one task: {found:?}");
    };
    assert_eq!(task_id, &an_id("FRK-1"));
    assert_eq!(
        detail, "the log has it at accepted and the file says in_progress",
        "the log's answer first, because transitions are what the log records"
    );
}

#[test]
fn says_when_the_file_and_the_log_disagree_about_who_holds_a_contract() {
    // 5.11: the human may hold a contract, and `contract.locked` is the event that says so. A file
    // that says otherwise would let work start on a contract nobody may touch.
    let project = TempProject::new("reconcile-lock");
    let files = project.files();
    let (log, projections) = a_board();
    on_the_board(&log, &projections, "FRK-1", "ready", true);
    files
        .write_contract(&a_contract("FRK-1", TaskStatus::Ready, false))
        .expect("written");

    let found = drifts(&files, &projections);
    assert_eq!(found.len(), 1, "{found:?}");
    let Drift::LockMismatch { task_id, detail } = &found[0] else {
        panic!("two answers about who holds it: {found:?}");
    };
    assert_eq!(task_id, &an_id("FRK-1"));
    assert_eq!(
        detail,
        "the log has it held by the human and the file says not held"
    );
}

#[test]
fn reports_a_contract_it_cannot_read_rather_than_stopping_at_it() {
    // One file a person broke must not hide every other disagreement, which is the whole use of
    // this: a person runs it to find out what is wrong, not to be told one thing at a time.
    let project = TempProject::new("reconcile-broken");
    let files = project.files();
    let (log, projections) = a_board();
    on_the_board(&log, &projections, "FRK-1", "ready", false);
    on_the_board(&log, &projections, "FRK-2", "accepted", false);
    files
        .write_contract(&a_contract("FRK-1", TaskStatus::Ready, false))
        .expect("written");
    files
        .write_contract(&a_contract("FRK-2", TaskStatus::Ready, false))
        .expect("written");
    std::fs::write(
        project.root.join(".farik/contracts/FRK-1.yaml"),
        "id: FRK-1\ntitle: half a contract\n",
    )
    .expect("a person edits one");

    let found = drifts(&files, &projections);
    assert_eq!(found.len(), 2, "{found:?}");
    let Drift::ContractUnreadable { task_id, detail } = &found[0] else {
        panic!("the broken one, and then the other: {found:?}");
    };
    assert_eq!(task_id, &an_id("FRK-1"));
    assert!(detail.contains(".farik/contracts/FRK-1.yaml"), "{detail}");
    assert!(
        matches!(&found[1], Drift::StatusMismatch { task_id, .. } if task_id == &an_id("FRK-2")),
        "{found:?}"
    );
}

#[test]
fn says_what_it_could_not_compare_and_why() {
    assert_eq!(
        [
            ReconcileError::Files {
                detail: ".farik/contracts could not be used: Permission denied (os error 13)"
                    .to_string()
            }
            .to_string(),
            ReconcileError::Store {
                detail: "sqlite refused: database is locked".to_string()
            }
            .to_string(),
        ],
        [
            "the files could not be read: .farik/contracts could not be used: Permission denied \
             (os error 13)",
            "the board could not be read: sqlite refused: database is locked",
        ]
    );
}

#[test]
fn reports_everything_it_found_in_an_order_two_runs_agree_on() {
    let project = TempProject::new("reconcile-order");
    let files = project.files();
    let (log, projections) = a_board();
    for (id, status) in [
        ("FRK-10", "ready"),
        ("FRK-2", "accepted"),
        ("FRK-9", "ready"),
    ] {
        on_the_board(&log, &projections, id, status, false);
    }
    files
        .write_contract(&a_contract("FRK-2", TaskStatus::Draft, true))
        .expect("written");
    files
        .write_contract(&a_contract("FRK-10", TaskStatus::Ready, false))
        .expect("written");
    files
        .write_contract(&a_contract("FRK-11", TaskStatus::Draft, false))
        .expect("written");

    let found = drifts(&files, &projections);
    assert_eq!(
        found
            .iter()
            .map(|drift| (drift.task_id().as_str().to_string(), name_of(drift)))
            .collect::<Vec<_>>(),
        [
            ("FRK-2".to_string(), "StatusMismatch"),
            ("FRK-2".to_string(), "LockMismatch"),
            ("FRK-9".to_string(), "EventsWithoutContract"),
            ("FRK-11".to_string(), "ContractWithoutEvents"),
        ],
        "by the number in the id, so the tenth task does not come before the ninth, and then in one \
         fixed order per task"
    );
    assert_eq!(
        drifts(&files, &projections),
        found,
        "and a second run says the same thing"
    );
}

#[test]
fn orders_two_spellings_of_one_number_by_the_id_itself() {
    let project = TempProject::new("reconcile-two-spellings");
    let files = project.files();
    let (log, projections) = a_board();
    // `FRK-01` and `FRK-1` are two spellings of one number, and each is found by a different loop:
    // the board knows the first and the files hold the second. Nothing but a tie-break decides
    // which comes out first, and "whichever loop got there" is not an order a person can diff.
    on_the_board(&log, &projections, "FRK-01", "ready", false);
    files
        .write_contract(&a_contract("FRK-1", TaskStatus::Draft, false))
        .expect("written");

    assert_eq!(
        drifts(&files, &projections)
            .iter()
            .map(|drift| (drift.task_id().as_str().to_string(), name_of(drift)))
            .collect::<Vec<_>>(),
        [
            ("FRK-01".to_string(), "EventsWithoutContract"),
            ("FRK-1".to_string(), "ContractWithoutEvents"),
        ],
        "the id breaks the tie, the way the board's own order does"
    );
}

#[test]
fn a_project_with_no_farik_directory_at_all_has_nothing_to_report() {
    let project = TempProject::new("reconcile-nothing");
    let (_log, projections) = a_board();
    assert_eq!(drifts(&project.files(), &projections), []);
}

fn name_of(drift: &Drift) -> &'static str {
    match drift {
        Drift::ContractWithoutEvents { .. } => "ContractWithoutEvents",
        Drift::EventsWithoutContract { .. } => "EventsWithoutContract",
        Drift::StatusMismatch { .. } => "StatusMismatch",
        Drift::LockMismatch { .. } => "LockMismatch",
        Drift::ContractUnreadable { .. } => "ContractUnreadable",
    }
}
