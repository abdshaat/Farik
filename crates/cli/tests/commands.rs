//! The command line against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`, as the store's own do and for the same reasons.

use std::path::Path;

use chrono::{DateTime, Utc};
use farik::{CliIo, run_cli};
use farik_protocol::clock::FixedClock;
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;

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

fn files_of(repository: &TempRepo) -> ProjectFiles {
    ProjectFiles::open(repository.path.clone())
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
fn refuses_an_invocation_it_cannot_use() {
    let ran = run_in(Path::new("."), &["nonsense"]);

    assert_eq!(ran.code, 2, "two is what a wrong command line exits with");
    assert!(ran.err.contains("nonsense"), "{}", ran.err);
    assert!(ran.out.is_empty(), "{}", ran.out);
}
