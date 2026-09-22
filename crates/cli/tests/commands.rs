//! The command line against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`, as the store's own do and for the same reasons.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik::{CliIo, run_cli};
use farik_core::contract::TaskId;
use farik_protocol::clock::FixedClock;
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};

/// The moment every test runs at, so that "last commit today" is an answer rather than a guess.
fn at() -> DateTime<Utc> {
    Utc::now()
}

/// What one run of the command line did.
struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// Runs one command in a repository, as a person standing in `cwd` would.
fn run_in(cwd: &Path, args: &[&str]) -> Ran {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let mut io = CliIo {
            stdout: Box::new(&mut out),
            stderr: Box::new(&mut err),
            cwd: cwd.to_path_buf(),
            clock: Box::new(FixedClock::new(at())),
        };
        let arguments: Vec<String> = std::iter::once("farik")
            .chain(args.iter().copied())
            .map(ToString::to_string)
            .collect();
        run_cli(&arguments, &mut io)
    };
    Ran {
        code,
        out: String::from_utf8(out).expect("the command line writes text"),
        err: String::from_utf8(err).expect("the command line writes text"),
    }
}

/// A repository with a Rust project in it, ready for `farik init`.
fn a_repository(name: &str) -> TempRepo {
    let repository = TempRepo::new(name);
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.commit("a project");
    repository
}

/// A repository that is already a Farik project.
fn a_project(name: &str) -> TempRepo {
    let repository = a_repository(name);
    let ran = run_in(&repository.path, &["init"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    repository
}

/// A contract a person would write, as YAML, with nothing in it that is not theirs to write.
fn a_request(title: &str) -> String {
    format!(
        r"title: {title}
intent: A person can read the board without opening a database.
scope:
  in_scope:
    - the board command
  out_of_scope:
    - the web app
requirements:
  - id: R1
    text: The board prints one line per task.
exit_criteria:
  - id: C1
    text: Every test in the workspace passes.
    satisfies:
      - R1
    verification:
      method: test
      command: cargo test --workspace
      new_tests_required: true
assignee_role: software_developer
reviewer_role: architect
risk: low
budget:
  max_cost_usd: 5
allowed_paths:
  - crates/cli/**
"
    )
}

fn files_of(repository: &TempRepo) -> ProjectFiles {
    ProjectFiles::open(repository.path.clone())
}

/// Writes a request to a file beside the repository and answers with its path.
fn a_request_file(repository: &TempRepo, name: &str, title: &str) -> std::path::PathBuf {
    let path = repository.path.join(name);
    std::fs::write(&path, a_request(title)).expect("the request is written");
    path
}

/// Every event kind the log holds, in order, so a test says what a command recorded.
fn kinds_in(repository: &TempRepo) -> Vec<String> {
    let log = farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
        .expect("the log opens");
    log.read(&farik_store::EventQuery::default())
        .expect("the log reads")
        .iter()
        .map(|event| event.body.kind().to_string())
        .collect()
}

/// The board, so a test says what the log makes of what a command recorded.
fn board_of(repository: &TempRepo) -> Vec<(String, String, String, bool, bool)> {
    let log = Arc::new(
        farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
            .expect("the log opens"),
    );
    farik_store::open_projections(log)
        .expect("the projections open")
        .board()
        .expect("the board reads")
        .iter()
        .map(|row| {
            (
                row.task_id.as_str().to_string(),
                row.kind.to_string(),
                row.status.to_string(),
                row.triaged,
                row.locked,
            )
        })
        .collect()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn makes_a_project_out_of_a_repository() {
    let repository = a_repository("cli-init");
    let ran = run_in(&repository.path, &["init"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().next(),
        Some("Rust, cargo, last commit today"),
        "the first line is the scan read back, which is what section 4 shows a person"
    );
    assert!(
        ran.out
            .contains("wrote .farik/team.yaml: product-manager, developer"),
        "{}",
        ran.out
    );
    assert!(ran.out.contains("the-tests-pass"), "{}", ran.out);

    let files = files_of(&repository);
    let team = files.read_team().expect("a team was written");
    assert_eq!(team.agents.len(), 2);
    assert!(
        files
            .read_project_scan()
            .expect("a scan was written")
            .contains("Rust, cargo, last commit today")
    );
    assert_eq!(
        files
            .read_criteria()
            .expect("a library was written")
            .criteria
            .iter()
            .map(|one| one.name.as_str().to_string())
            .collect::<Vec<_>>(),
        [
            "the-tests-pass",
            "the-build-succeeds",
            "clippy-is-clean",
            "formatting-is-clean"
        ]
    );
    assert_eq!(
        kinds_in(&repository),
        ["team.updated", "project.scanned", "criteria.updated"],
        "one event per thing it wrote, in the order it wrote them"
    );
    assert!(
        repository.path.join(".farik/local/.gitignore").is_file(),
        "the log is local and not committed (D5)"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_the_team_id_the_first_init_gave_after_the_team_is_renamed() {
    // The ids are fixed by the log's first event, so renaming the team does not split one
    // project's log in two (section 3).
    let repository = a_project("cli-ids-fixed");
    let mut team = files_of(&repository).read_team().expect("a team");
    team.name = "Renamed".parse().expect("a team name");
    files_of(&repository)
        .write_team(&team)
        .expect("the team is written");

    let file = a_request_file(&repository, "request.yaml", "A board command");
    let ran = run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);

    let log = farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
        .expect("the log opens");
    let events = log
        .read(&farik_store::EventQuery::default())
        .expect("the log reads");
    let first = &events.first().expect("init recorded events").envelope.ids;
    let last = &events.last().expect("and the create one").envelope.ids;
    assert_ne!(first.team_id, "renamed", "the first id is the old name's");
    assert_eq!(last.team_id, first.team_id);
    assert_eq!(last.project_id, first.project_id);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn a_second_init_rescans_and_keeps_the_team() {
    let repository = a_project("cli-init-again");
    let mut team = files_of(&repository).read_team().expect("a team");
    team.name = "Renamed".parse().expect("a team name");
    files_of(&repository)
        .write_team(&team)
        .expect("the team is written");

    let ran = run_in(&repository.path, &["init"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out
            .contains("kept the team already in .farik/team.yaml"),
        "{}",
        ran.out
    );
    assert_eq!(
        files_of(&repository)
            .read_team()
            .expect("a team")
            .name
            .as_str(),
        "Renamed",
        "a second init is a rescan, not a command that throws the team away"
    );
    assert_eq!(
        kinds_in(&repository),
        [
            "team.updated",
            "project.scanned",
            "criteria.updated",
            "project.scanned",
            "criteria.updated"
        ],
        "and it records the scan it did, not a team it did not write"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_rescan_a_project_whose_team_file_cannot_be_read() {
    // A file that is there and broken is not a file that is absent: `ProjectFiles::init` leaves a
    // team file where it is, so a run that treated the two the same would report a team it had not
    // written, and the log would keep that report for good.
    let repository = a_project("cli-init-broken-team");
    repository.write(".farik/team.yaml", "name: one\nagents: []\n");

    let ran = run_in(&repository.path, &["init"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains(".farik/team.yaml"),
        "the refusal names the file: {}",
        ran.err
    );
    assert!(ran.out.is_empty(), "{}", ran.out);
    assert_eq!(
        kinds_in(&repository),
        ["team.updated", "project.scanned", "criteria.updated"],
        "and it recorded nothing this run: the log is append-only and a team.updated naming agents \
         that are not in the file could never be taken back"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_rescan_a_project_whose_criterion_library_cannot_be_read() {
    // `.farik/team/criteria.yaml` is a file 5.13 expects people to hand-edit, and `seeded_library`
    // keeps only the criteria it is handed: reading it as absent would delete every criterion a
    // person had written because one line of it has a typo in it.
    let repository = a_project("cli-init-broken-criteria");
    let broken = "criteria:\n  - name: the-docs-are-updated\n    text: The documents say what changed.\n    source: human\n    verification:\n      method: comand\n";
    repository.write(".farik/team/criteria.yaml", broken);

    let ran = run_in(&repository.path, &["init"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains(".farik/team/criteria.yaml"),
        "the refusal names the file: {}",
        ran.err
    );
    assert_eq!(
        std::fs::read_to_string(repository.path.join(".farik/team/criteria.yaml"))
            .expect("the file is still there"),
        broken,
        "and what the person wrote is still there to fix"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn writes_nothing_at_all_when_it_has_to_refuse() {
    // Both hand-edited files are read before anything is written. The other order leaves a team file
    // behind that no `team.updated` will ever record: the next run finds the file, says it kept the
    // team already there, and the log is append-only, so the event that says where that team came
    // from can never be added.
    let repository = a_repository("cli-init-refuse-first");
    repository.write(
        ".farik/team/criteria.yaml",
        "criteria:\n  - name: the-docs-are-updated\n    text: The documents say what changed.\n    source: human\n    verification:\n      method: comand\n",
    );

    let ran = run_in(&repository.path, &["init"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains(".farik/team/criteria.yaml"),
        "the refusal names the file: {}",
        ran.err
    );
    assert!(
        !repository.path.join(".farik/team.yaml").exists(),
        "a run that refuses has written no team, so the run that follows it writes one and records \
         that it did"
    );
    assert!(
        !repository.path.join(".farik/local/farik.db").exists(),
        "and no log, because nothing happened to record"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn works_from_any_directory_under_the_repository_root() {
    let repository = a_repository("cli-subdirectory");
    let inside = repository.path.join("src");
    let ran = run_in(&inside, &["init"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        repository.path.join(".farik/team.yaml").is_file(),
        "a project is a whole repository, so the project is at the root whatever directory the \
         person stood in"
    );
    assert!(
        !inside.join(".farik").exists(),
        "and not a second project in the subdirectory"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_directory_that_is_not_a_repository() {
    let plain = std::env::temp_dir().join(format!("farik-cli-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).expect("a plain directory");
    let ran = run_in(&plain, &["init"]);
    let _ = std::fs::remove_dir_all(&plain);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("is not a git repository") && ran.err.contains("git init"),
        "{}",
        ran.err
    );
    assert!(ran.out.is_empty(), "{}", ran.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_every_other_command_until_the_project_exists() {
    let repository = a_repository("cli-no-project");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    let ran = run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("there is no Farik project at") && ran.err.contains("farik init"),
        "{}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn files_a_contract_as_a_draft_request() {
    let repository = a_project("cli-create");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    let ran = run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out
            .contains("FRK-1 filed as a draft request: A board command"),
        "{}",
        ran.out
    );
    assert!(ran.out.contains("farik triage"), "{}", ran.out);

    let contract = files_of(&repository)
        .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
        .expect("a contract was written");
    assert_eq!(contract.status.to_string(), "draft");
    assert_eq!(contract.created_by.as_deref(), Some("human"));
    assert_eq!(
        contract.kind.to_string(),
        "task",
        "a request is a task until the triage says otherwise (5.16)"
    );
    assert_eq!(
        board_of(&repository),
        [(
            "FRK-1".to_string(),
            "task".to_string(),
            "draft".to_string(),
            false,
            false
        )],
        "and the board knows it from the event alone"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn never_hands_out_an_id_a_committed_contract_already_has() {
    // The log is machine-local and the contracts travel with the repository (8.4), so a fresh clone
    // has the files and a counter at zero. Taking the id from the counter alone would file the next
    // request as FRK-1 and write it over the contract a teammate committed.
    let repository = a_project("cli-create-fresh-clone");
    let first = a_request_file(&repository, "first.yaml", "The committed one");
    let filed = run_in(
        &repository.path,
        &["task", "create", first.to_str().expect("a path")],
    );
    assert_eq!(filed.code, 0, "{}", filed.err);
    std::fs::remove_dir_all(repository.path.join(".farik/local")).expect("a fresh clone");

    let second = a_request_file(&repository, "second.yaml", "The new one");
    let ran = run_in(
        &repository.path,
        &["task", "create", second.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("FRK-2 filed"), "{}", ran.out);
    assert_eq!(
        files_of(&repository)
            .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
            .expect("the committed contract is still there")
            .title
            .as_str(),
        "The committed one"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_the_contract_from_where_the_command_was_run() {
    // The path is the person's, so it is relative to the directory they typed it in — not to the
    // repository root, which is where the project is, and not to whatever directory this process
    // happens to be in.
    let repository = a_project("cli-create-relative");
    std::fs::create_dir_all(repository.path.join("notes")).expect("a directory");
    std::fs::write(
        repository.path.join("notes/request.yaml"),
        a_request("A board command"),
    )
    .expect("the request is written");

    let ran = run_in(
        &repository.path.join("notes"),
        &["task", "create", "request.yaml"],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("FRK-1 filed as a draft request"),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_request_that_sets_what_is_not_the_authors_to_set() {
    let repository = a_project("cli-create-fields");
    let path = repository.path.join("request.yaml");
    std::fs::write(
        &path,
        format!(
            "{}id: FRK-9\nstatus: ready\nlocked: true\nparent: FRK-2\n",
            a_request("A board command")
        ),
    )
    .expect("the request is written");
    let ran = run_in(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("id") && ran.err.contains("status") && ran.err.contains("locked"),
        "{}",
        ran.err
    );
    assert!(
        ran.err.contains("farik contract lock") && ran.err.contains("farik triage"),
        "a refusal says what to do instead: {}",
        ran.err
    );
    assert!(
        ran.err.contains("parent") && ran.err.contains("5.16"),
        "and `parent` is the one a person may legitimately write, so its refusal names the rule \
         that will open it: {}",
        ran.err
    );
    assert!(
        files_of(&repository)
            .list_contracts()
            .expect("a list")
            .is_empty(),
        "and nothing was filed"
    );
}

/// Files a request with one line added to it, and says that it was refused for that field and that
/// nothing was filed.
fn refused_for_one_field(name: &str, line: &str, field: &str) {
    let repository = a_project(name);
    let path = repository.path.join("request.yaml");
    std::fs::write(&path, format!("{}{line}\n", a_request("A board command")))
        .expect("the request is written");
    let ran = run_in(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err
            .contains(&format!("sets {field}, which a request does not")),
        "{}",
        ran.err
    );
    assert!(
        files_of(&repository)
            .list_contracts()
            .expect("a list")
            .is_empty(),
        "and nothing was filed"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_request_that_sets_a_field_the_store_owns() {
    refused_for_one_field(
        "cli-create-store-field",
        "created_at: 2026-01-01T00:00:00Z",
        "created_at",
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_request_that_sets_a_field_fixed_at_creation() {
    refused_for_one_field("cli-create-fixed-field", "kind: epic", "kind");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_request_the_contract_rules_refuse() {
    let repository = a_project("cli-create-invalid");
    let path = repository.path.join("request.yaml");
    std::fs::write(
        &path,
        a_request("A board command").replace("  - id: C1", "  - id: R1"),
    )
    .expect("the request is written");
    let ran = run_in(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("is not a contract Farik can file"),
        "{}",
        ran.err
    );
    assert!(
        files_of(&repository)
            .list_contracts()
            .expect("a list")
            .is_empty(),
        "and nothing was filed"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn a_refused_request_does_not_use_up_an_id() {
    let repository = a_project("cli-create-refused-id");
    let path = repository.path.join("broken.yaml");
    std::fs::write(
        &path,
        a_request("A board command").replace("  - id: C1", "  - id: R1"),
    )
    .expect("the request is written");
    let refused = run_in(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );
    assert_eq!(refused.code, 1, "{}", refused.out);

    let file = a_request_file(&repository, "request.yaml", "A board command");
    let ran = run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("FRK-1 filed"),
        "the next id is the next one, not one past a refusal: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_file_that_is_not_yaml() {
    let repository = a_project("cli-create-not-yaml");
    let path = repository.path.join("request.yaml");
    std::fs::write(&path, "title: one\ntitle: two\n").expect("the request is written");
    let ran = run_in(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("request.yaml"),
        "the refusal names the file it is about: {}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn sizes_a_request_as_large_and_makes_it_an_epic() {
    let repository = a_project("cli-triage-large");
    let file = a_request_file(&repository, "request.yaml", "A whole board");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    let ran = run_in(
        &repository.path,
        &[
            "triage",
            "FRK-1",
            "large",
            "--reason",
            "it is three screens and a migration",
        ],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out
            .contains("FRK-1 is large: epic. it is three screens and a migration"),
        "{}",
        ran.out
    );
    assert_eq!(
        files_of(&repository)
            .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
            .expect("a contract")
            .kind
            .to_string(),
        "epic",
        "the triage decides the kind and its own tool changes it (5.11)"
    );
    assert_eq!(
        board_of(&repository),
        [(
            "FRK-1".to_string(),
            "epic".to_string(),
            "draft".to_string(),
            true,
            false
        )]
    );
    assert_eq!(
        kinds_in(&repository).last().map(String::as_str),
        Some("request.triaged")
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn overrules_a_triage_while_the_request_is_still_a_draft() {
    let repository = a_project("cli-triage-overrule");
    let file = a_request_file(&repository, "request.yaml", "A whole board");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    run_in(
        &repository.path,
        &["triage", "FRK-1", "large", "--reason", "it looked large"],
    );

    let ran = run_in(
        &repository.path,
        &[
            "triage",
            "FRK-1",
            "small",
            "--reason",
            "one screen after all",
        ],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        board_of(&repository)[0].1,
        "task",
        "the user may overrule a triage until refining starts (5.16)"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_triage_of_an_id_that_is_not_one() {
    // What a person typed, not the `oneOf` sentence a schema refuses a whole body with.
    let repository = a_project("cli-triage-bad-id");
    let ran = run_in(
        &repository.path,
        &["triage", "nine", "large", "--reason", "it has no number"],
    );

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("nine is not a task id"), "{}", ran.err);
    assert!(!ran.err.contains("oneOf"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_triage_with_no_reason_written() {
    // 5.16 records the decision with a reason, and the log is where somebody reads it back.
    let repository = a_project("cli-triage-no-reason");
    let file = a_request_file(&repository, "request.yaml", "A whole board");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    let ran = run_in(
        &repository.path,
        &["triage", "FRK-1", "large", "--reason", "   "],
    );

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("recorded with a reason"), "{}", ran.err);
    assert_eq!(
        kinds_in(&repository).last().map(String::as_str),
        Some("task.created"),
        "and nothing was recorded"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_triage_once_refining_has_started() {
    let repository = a_project("cli-triage-late");
    let file = a_request_file(&repository, "request.yaml", "A whole board");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    moved_to(&repository, "FRK-1", "refining");

    let ran = run_in(
        &repository.path,
        &["triage", "FRK-1", "large", "--reason", "too late"],
    );

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("refining") && ran.err.contains("5.16"),
        "{}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_triage_of_a_task_that_belongs_to_an_epic() {
    let repository = a_project("cli-triage-child");
    let file = a_request_file(&repository, "request.yaml", "A whole board");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    let files = files_of(&repository);
    let id = TaskId::try_from("FRK-1").expect("a task id");
    let mut contract = files.read_contract(&id).expect("a contract");
    contract.parent = Some("FRK-2".parse().expect("a parent id"));
    files
        .write_contract(&contract)
        .expect("the contract is written");

    let ran = run_in(
        &repository.path,
        &["triage", "FRK-1", "large", "--reason", "not mine to size"],
    );

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("belongs to an epic"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn takes_a_contract_and_gives_it_back() {
    let repository = a_project("cli-lock");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );

    let taken = run_in(&repository.path, &["contract", "lock", "FRK-1"]);
    assert_eq!(taken.code, 0, "{}", taken.err);
    assert!(taken.out.contains("FRK-1 is yours"), "{}", taken.out);
    assert!(
        files_of(&repository)
            .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
            .expect("a contract")
            .locked
    );
    assert!(
        board_of(&repository)[0].4,
        "the board says the human holds it"
    );

    let given = run_in(&repository.path, &["contract", "unlock", "FRK-1"]);
    assert_eq!(given.code, 0, "{}", given.err);
    assert!(
        given.out.contains("FRK-1 is the team's again"),
        "{}",
        given.out
    );
    assert!(!board_of(&repository)[0].4, "and that it gave it back");
    assert_eq!(
        kinds_in(&repository)
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>(),
        ["contract.unlocked", "contract.locked"]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_take_a_contract_that_is_already_yours() {
    let repository = a_project("cli-lock-twice");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    run_in(&repository.path, &["contract", "lock", "FRK-1"]);

    let ran = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("already yours"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_take_a_contract_whose_task_is_finished() {
    let repository = a_project("cli-lock-accepted");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    moved_to(&repository, "FRK-1", "accepted");

    let ran = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("accepted") && ran.err.contains("nothing leaves that status"),
        "{}",
        ran.err
    );

    // And the governor is asked before the contract is found to be held already, so a task nothing
    // can be written to says that rather than answering about the lock.
    let files = files_of(&repository);
    let id = TaskId::try_from("FRK-1").expect("a task id");
    let mut contract = files.read_contract(&id).expect("a contract");
    contract.locked = true;
    files
        .write_contract(&contract)
        .expect("the contract is written");
    let again = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

    assert_eq!(again.code, 1);
    assert!(
        again.err.contains("nothing leaves that status"),
        "not `already yours`: {}",
        again.err
    );
}

/// A project with FRK-1 filed and a second contract, FRK-7, written straight to its file, so that
/// the log has never heard of it.
fn a_project_with_a_contract_only_the_files_know(name: &str) -> TempRepo {
    let repository = a_project(name);
    let file = a_request_file(&repository, "request.yaml", "A board command");
    let filed = run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    assert_eq!(filed.code, 0, "{}", filed.err);
    let files = files_of(&repository);
    let mut contract = files
        .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
        .expect("a contract");
    contract.id = TaskId::try_from("FRK-7").expect("a task id");
    files
        .write_contract(&contract)
        .expect("the contract is written");
    repository
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_the_status_the_log_says_and_that_the_file_disagrees() {
    let repository = a_project("cli-show-status");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    run_in(
        &repository.path,
        &["task", "create", file.to_str().expect("a path")],
    );
    moved_to(&repository, "FRK-1", "refining");

    let ran = run_in(&repository.path, &["task", "show", "FRK-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().nth(1),
        Some("refining task, low risk"),
        "the log decides a task's status (8.4): {}",
        ran.out
    );
    assert!(
        ran.out
            .contains("the file says draft and the log says refining"),
        "and a disagreement is said, not settled silently: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_a_task_the_log_has_never_heard_of_and_says_so() {
    let repository = a_project_with_a_contract_only_the_files_know("cli-show-unheard");

    let ran = run_in(&repository.path, &["task", "show", "FRK-7"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("the log has never heard of this task"),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_task_id_that_is_not_one() {
    let repository = a_project("cli-bad-id");
    let ran = run_in(&repository.path, &["contract", "lock", "nine"]);

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("nine is not a task id"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_what_it_did_as_json_when_asked() {
    let repository = a_project("cli-json");
    let file = a_request_file(&repository, "request.yaml", "A board command");
    let ran = run_in(
        &repository.path,
        &["--json", "task", "create", file.to_str().expect("a path")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    let printed: Value = serde_json::from_str(ran.out.trim()).expect("one JSON object per run");
    assert_eq!(printed["task_id"], json!("FRK-1"));
    assert_eq!(printed["status"], json!("draft"));
    assert_eq!(printed["path"], json!(".farik/contracts/FRK-1.yaml"));
    assert!(printed["events"].is_array(), "{printed}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_why_it_refused_as_json_when_asked() {
    let repository = a_project("cli-json-refusal");
    let ran = run_in(
        &repository.path,
        &["--json", "task", "create", "nowhere.yaml"],
    );

    assert_eq!(ran.code, 1);
    let printed: Value = serde_json::from_str(ran.err.trim()).expect("one JSON object per refusal");
    assert!(
        printed["error"]
            .as_str()
            .is_some_and(|text| text.contains("nowhere.yaml")),
        "a script that asked for JSON gets JSON for the refusal too: {printed}"
    );
    assert!(ran.out.is_empty(), "and nothing on stdout: {}", ran.out);
}

#[test]
fn says_what_it_can_do_when_asked() {
    let ran = run_in(Path::new("."), &["--help"]);

    assert_eq!(
        ran.code, 0,
        "help is what a person asked for, not a mistake"
    );
    for command in ["init", "task", "triage", "contract"] {
        assert!(ran.out.contains(command), "{}", ran.out);
    }
    assert!(ran.err.is_empty(), "{}", ran.err);
}

#[test]
fn refuses_an_invocation_it_cannot_use() {
    let ran = run_in(Path::new("."), &["nonsense"]);

    assert_eq!(ran.code, 2, "two is what a wrong command line exits with");
    assert!(ran.err.contains("nonsense"), "{}", ran.err);
    assert!(ran.out.is_empty(), "{}", ran.out);
}

/// Puts a task's board row at a status, so that a test can ask what a command does about one.
///
/// `contract.written` is the only event of this phase that moves a row, and its body carries a whole
/// summary, so this rewrites the row's title, kind, risk and parent from the fixture as well. A
/// governed transition is a `task.transitioned` event and rewrites none of that; it arrives with the
/// runtime in phase 3, and so does the command that asks for one.
fn moved_to(repository: &TempRepo, task_id: &str, status: &str) {
    use farik_protocol::event::fixtures::{a_contract_summary_wire, an_event_wire};
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};

    let log = farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
        .expect("the log opens");
    let log = Arc::new(log);
    let projections =
        farik_store::open_projections(Arc::clone(&log)).expect("the projections open");
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
}
