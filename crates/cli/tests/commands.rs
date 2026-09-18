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
fn refuses_an_invocation_it_cannot_use() {
    let ran = run_in(Path::new("."), &["nonsense"]);

    assert_eq!(ran.code, 2, "two is what a wrong command line exits with");
    assert!(ran.err.contains("nonsense"), "{}", ran.err);
    assert!(ran.out.is_empty(), "{}", ran.out);
}
