//! The commands that read, against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`, as the rest of this crate's do.

#[cfg(unix)]
#[path = "shared/project.rs"]
mod project;

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik::{CliIo, run_cli};
use farik_protocol::clock::FixedClock;
use farik_runtime::sprints::{PlannedBy, plan_sprint};
use farik_store::git::fixtures::TempRepo;
use serde_json::Value;

/// The moment every test runs at, fixed rather than read from the wall clock (code.md).
const NOW: &str = "2026-09-22T12:00:00Z";

/// When the project's own commit was made: earlier the same day as `NOW`.
const COMMITTED: &str = "2026-09-22T09:00:00Z";

/// `NOW`, as the clock the command line is handed.
fn at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(NOW)
        .expect("a moment")
        .with_timezone(&Utc)
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

fn run_in(cwd: &Path, args: &[&str]) -> Ran {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let mut io = CliIo::new(
            cwd.to_path_buf(),
            Box::new(&mut out),
            Box::new(&mut err),
            Arc::new(FixedClock::new(at())),
        );
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

/// A repository that is a Farik project with one filed, triaged request.
fn a_project_with_a_task(name: &str) -> TempRepo {
    let repository = TempRepo::new(name);
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.commit_at("a project", COMMITTED);
    let init = run_in(&repository.path, &["init"]);
    assert_eq!(init.code, 0, "{}", init.err);
    repository.write("request.yaml", &a_request("Show the board"));
    let filed = run_in(&repository.path, &["task", "create", "request.yaml"]);
    assert_eq!(filed.code, 0, "{}", filed.err);
    repository
}

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

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_the_board_one_line_per_task() {
    let repository = a_project_with_a_task("read-board");
    let ran = run_in(&repository.path, &["board"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("FRK-1"), "{}", ran.out);
    assert!(ran.out.contains("draft"), "{}", ran.out);
    assert!(ran.out.contains("Show the board"), "{}", ran.out);
    assert!(
        ran.out.contains("not triaged"),
        "a request nothing has sized says so, because nothing starts before that (5.16): {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_when_the_board_is_empty() {
    let repository = TempRepo::new("read-board-empty");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.commit_at("a project", COMMITTED);
    run_in(&repository.path, &["init"]);

    let ran = run_in(&repository.path, &["board"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("no tasks yet") && ran.out.contains("farik task create"),
        "an empty board says what to do next: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_one_contract_and_what_happened_to_it() {
    let repository = a_project_with_a_task("read-show");
    run_in(
        &repository.path,
        &["triage", "FRK-1", "small", "--reason", "one screen"],
    );

    let ran = run_in(&repository.path, &["task", "show", "FRK-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("Show the board"), "{}", ran.out);
    assert!(ran.out.contains("draft"), "{}", ran.out);
    assert!(
        ran.out.contains("R1") && ran.out.contains("C1"),
        "a contract is its requirements and its exit criteria: {}",
        ran.out
    );
    assert!(
        ran.out.contains("task.created") && ran.out.contains("request.triaged"),
        "and what happened to it, in the order it happened: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_show_a_task_that_is_not_there() {
    let repository = a_project_with_a_task("read-show-missing");
    let ran = run_in(&repository.path, &["task", "show", "FRK-9"]);

    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("FRK-9"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_the_log_and_filters_it() {
    let repository = a_project_with_a_task("read-log");
    let all = run_in(&repository.path, &["log"]);

    assert_eq!(all.code, 0, "{}", all.err);
    for kind in [
        "team.updated",
        "project.scanned",
        "criteria.updated",
        "task.created",
    ] {
        assert!(all.out.contains(kind), "{}", all.out);
    }

    let one = run_in(&repository.path, &["log", "--task", "FRK-1"]);
    assert_eq!(one.code, 0, "{}", one.err);
    assert!(one.out.contains("task.created"), "{}", one.out);
    assert!(
        !one.out.contains("project.scanned"),
        "a filter that does not filter is not one: {}",
        one.out
    );

    let kind = run_in(&repository.path, &["log", "--kind", "project.scanned"]);
    assert_eq!(kind.code, 0, "{}", kind.err);
    assert!(kind.out.contains("project.scanned"), "{}", kind.out);
    assert!(!kind.out.contains("task.created"), "{}", kind.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn exports_the_log_as_one_json_object_per_line() {
    let repository = a_project_with_a_task("read-log-json");
    let ran = run_in(&repository.path, &["--json", "log"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.trim().lines().collect();
    assert_eq!(lines.len(), 4, "one line per event: {}", ran.out);
    for line in &lines {
        let event: Value = serde_json::from_str(line).expect("every line is one event");
        assert!(event["kind"].is_string(), "{event}");
        assert!(event["seq"].is_number(), "{event}");
    }
    assert_eq!(
        serde_json::from_str::<Value>(lines[3]).expect("an event")["kind"],
        Value::String("task.created".to_string())
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_log_filter_that_is_not_a_kind() {
    let repository = a_project_with_a_task("read-log-bad-kind");
    let ran = run_in(&repository.path, &["log", "--kind", "task.exploded"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("task.exploded") && ran.err.contains("task.created"),
        "the refusal says what the kinds are: {}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_the_team_rules_and_the_criterion_library() {
    let repository = a_project_with_a_task("read-team");

    let rules = run_in(&repository.path, &["rules", "show"]);
    assert_eq!(rules.code, 0, "{}", rules.err);
    assert!(
        rules.out.contains(".env"),
        "the protected paths farik-core ships are rules whatever the team wrote (5.12): {}",
        rules.out
    );
    assert!(rules.out.contains("new tests"), "{}", rules.out);
    assert!(
        rules
            .out
            .contains("document paths: docs/**, **/*.md, CHANGELOG.md"),
        "the document paths the schema defaults to: {}",
        rules.out
    );
    let json = run_in(&repository.path, &["--json", "rules", "show"]);
    assert_eq!(json.code, 0, "{}", json.err);
    let shown: Value = serde_json::from_str(json.out.trim()).expect("JSON");
    assert_eq!(
        shown["document_paths"],
        serde_json::json!(["docs/**", "**/*.md", "CHANGELOG.md"]),
        "{shown}"
    );

    let criteria = run_in(&repository.path, &["criteria", "list"]);
    assert_eq!(criteria.code, 0, "{}", criteria.err);
    assert!(criteria.out.contains("the-tests-pass"), "{}", criteria.out);
    assert!(
        criteria.out.contains("cargo test --workspace"),
        "a criterion is its command: {}",
        criteria.out
    );
    assert!(
        criteria.out.contains("project_scan"),
        "and where it came from, because a refresh replaces what the scan found: {}",
        criteria.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_when_a_project_agrees_with_itself() {
    let repository = a_project_with_a_task("read-doctor-clean");
    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("nothing to report"), "{}", ran.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_contract_the_log_has_never_heard_of() {
    let repository = a_project_with_a_task("read-doctor-drift");
    std::fs::copy(
        repository.path.join(".farik/contracts/FRK-1.yaml"),
        repository.path.join(".farik/contracts/FRK-7.yaml"),
    )
    .expect("a second contract file");
    let written = std::fs::read_to_string(repository.path.join(".farik/contracts/FRK-7.yaml"))
        .expect("read")
        .replace("id: FRK-1", "id: FRK-7");
    std::fs::write(repository.path.join(".farik/contracts/FRK-7.yaml"), written).expect("write");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1, "doctor exits 1 when it found something");
    assert!(ran.out.contains("FRK-7"), "{}", ran.out);
    assert!(ran.out.contains("never heard of"), "{}", ran.out);

    let log = run_in(&repository.path, &["log", "--kind", "drift.detected"]);
    assert!(
        log.out.contains("drift.detected"),
        "somebody looked, and the log says so (D5): {}",
        log.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_team_rule_that_does_not_compile() {
    let repository = a_project_with_a_task("read-doctor-rules");
    replace_the_rules(
        &repository.path,
        "rules:\n  forbidden_commands:\n    - \"rm -rf (\"\n",
    );

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.out.contains("forbidden_commands") && ran.out.contains("rm -rf ("),
        "the report names the rule and the pattern: {}",
        ran.out
    );
    assert!(
        ran.out.contains("refuses every command"),
        "and what it costs, which is every command the team runs: {}",
        ran.out
    );
}

/// Replaces the whole top-level `rules:` block of the project's team.yaml with `rules`, which is
/// that block written out. `farik init` writes the rules a team starts with (the default document
/// paths among them), so the block is whatever lines follow `rules:` up to the next top-level key.
fn replace_the_rules(project: &Path, rules: &str) {
    let path = project.join(".farik/team.yaml");
    let team = std::fs::read_to_string(&path).expect("read");
    let start = team
        .find("\nrules:")
        .map(|at| at + 1)
        .or_else(|| team.starts_with("rules:").then_some(0))
        .expect("the team has rules");
    let block = &team[start..];
    let end = block
        .match_indices('\n')
        .map(|(at, _)| at + 1)
        .find(|&at| block[at..].starts_with(|c: char| !c.is_whitespace()))
        .map_or(team.len(), |at| start + at);
    std::fs::write(&path, format!("{}{rules}{}", &team[..start], &team[end..])).expect("write");
}

/// Runs doctor on a project whose team rules are `rules`, and says it found `rule` and `pattern`.
fn reports_a_glob_that_does_not_compile(name: &str, rules: &str, rule: &str, pattern: &str) {
    let repository = a_project_with_a_task(name);
    replace_the_rules(&repository.path, rules);

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(
        ran.out.contains(&format!(
            "{rule} has a glob that does not compile, \"{pattern}\""
        )),
        "the report names the rule and the pattern: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_protected_path_that_does_not_compile() {
    reports_a_glob_that_does_not_compile(
        "read-doctor-protected",
        "rules:\n  protected_paths:\n    - \"a/[\"\n",
        "protected_paths",
        "a/[",
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_an_allowed_paths_ceiling_that_does_not_compile() {
    reports_a_glob_that_does_not_compile(
        "read-doctor-ceiling",
        "rules:\n  allowed_paths_ceiling:\n    - \"b/[\"\n",
        "allowed_paths_ceiling",
        "b/[",
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_document_path_that_does_not_compile() {
    reports_a_glob_that_does_not_compile(
        "read-doctor-documents",
        "rules:\n  document_paths:\n    - \"c/[\"\n",
        "document_paths",
        "c/[",
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_setting_farik_does_not_know() {
    let repository = a_project_with_a_task("read-doctor-settings");
    std::fs::write(
        repository.path.join(".farik/local/settings.json"),
        "{\"sandbox\": \"docker\", \"sandbox_mode\": \"none\"}\n",
    )
    .expect("write");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.out.contains("sandbox_mode"),
        "the one structured file with no schema behind it is where a typo goes unread: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_criterion_whose_verification_matches_no_branch() {
    let repository = a_project_with_a_task("read-doctor-criteria");
    std::fs::write(
        repository.path.join(".farik/team/criteria.yaml"),
        "criteria:\n  - name: the-docs-are-updated\n    text: The documents say what changed.\n    source: human\n    verification:\n      method: test\n      new_tests_required: true\n",
    )
    .expect("write");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.out.contains("the-docs-are-updated") || ran.out.contains("criteria.yaml"),
        "{}",
        ran.out
    );
    assert!(
        ran.out.contains("command"),
        "a `test` criterion wants a command, which is what the oneOf would not say: {}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_team_file_that_cannot_be_read() {
    let repository = a_project_with_a_task("read-doctor-team");
    std::fs::write(
        repository.path.join(".farik/team.yaml"),
        "name: one\nagents: []\n",
    )
    .expect("write");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("team.yaml") || ran.out.contains("team.yaml"),
        "{}",
        ran.err
    );
}

/// A team of `pm`, `dev`, and `dev-2`, with both developers on a model the shipped table does not
/// price.
#[cfg(unix)]
fn a_team_on_an_unpriced_model(name: &str) -> TempRepo {
    project::a_team_with(name, |wire| {
        let on_unknown_9 = |id: &str| {
            let mut dev = farik_core::team::fixtures::an_agent_wire(id, "software_developer");
            dev["model"] = serde_json::json!({ "id": "claude-unknown-9" });
            dev
        };
        wire["agents"] = serde_json::json!([
            farik_core::team::fixtures::an_agent_wire("pm", "product_manager"),
            on_unknown_9("dev"),
            on_unknown_9("dev-2"),
        ]);
    })
}

/// What doctor says of `claude-unknown-9`, used by `dev` and `dev-2`.
const UNPRICED_FINDING: &str = ".farik/team.yaml: no price table prices claude-unknown-9 (used by \
    dev, dev-2): its usage is recorded at no cost, and no dollar limit counts it. Add it to \
    .farik/prices.json to price it (5.5)";

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_model_no_price_table_prices() {
    let repository = a_team_on_an_unpriced_model("read-doctor-unpriced");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.out.lines().any(|line| line == UNPRICED_FINDING),
        "{}",
        ran.out
    );

    let mut prices: Value =
        serde_json::from_str(farik_core::pricing::prices::PRICES_JSON).expect("the shipped table");
    prices["prices"]["claude-unknown-9"] = prices["prices"]["claude-opus-5"].clone();
    std::fs::write(
        repository.path.join(".farik/prices.json"),
        prices.to_string(),
    )
    .expect("the override is written");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 0, "{}", ran.out);
    assert!(!ran.out.contains("claude-unknown-9"), "{}", ran.out);
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reports_a_price_table_it_cannot_read() {
    let repository = a_team_on_an_unpriced_model("read-doctor-prices");
    std::fs::write(
        repository.path.join(".farik/prices.json"),
        "{\"version\": 2}",
    )
    .expect("the override is written");

    let ran = run_in(&repository.path, &["doctor"]);

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.out.contains(".farik/prices.json"), "{}", ran.out);
    assert!(
        !ran.out
            .lines()
            .any(|line| line.starts_with(".farik/team.yaml: no price table prices")),
        "{}",
        ran.out
    );
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_a_tasks_events_cost_and_children() {
    use farik_protocol::event::EventIds;
    use farik_store::requests::file_request;
    use serde_json::json;

    let repository = a_project_with_a_task("read-show-story");
    let sized = run_in(
        &repository.path,
        &["triage", "FRK-1", "large", "--reason", "Three screens."],
    );
    assert_eq!(sized.code, 0, "{}", sized.err);
    project::walked(
        &repository,
        "FRK-1",
        &["refining", "ready", "assigned", "in_progress"],
    );
    let log = project::log_of(&repository);
    let first = project::events(&repository, &[]);
    let ids = EventIds {
        task_id: None,
        agent_id: None,
        session_id: None,
        ..first[0].envelope.ids.clone()
    };
    file_request(
        &project::files_of(&repository),
        &log,
        farik_store::files::yaml_value(&a_request("Show one row"), "child.yaml")
            .expect("the child is YAML"),
        "human",
        Some(&"FRK-1".parse().expect("a task id")),
        at(),
        &ids,
    )
    .expect("the child is filed");
    project::record_as(
        &repository,
        "FRK-1",
        Some(("pm", "s-1")),
        "cost.recorded",
        &json!({
            "purpose": "plan",
            "model_id": "claude-opus-5",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 100,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            },
            "cost_usd": 0.5
        }),
    );

    let ran = run_in(&repository.path, &["task", "show", "FRK-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    for expected in [
        "request.triaged — large by human: ",
        "cost: $0.50 of $5.00; sessions: 1;",
        "children",
        "  FRK-2 draft ",
    ] {
        assert!(ran.out.contains(expected), "{expected:?} in {}", ran.out);
    }
    let ran = run_in(&repository.path, &["--json", "task", "show", "FRK-1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let shown: Value = serde_json::from_str(ran.out.trim()).expect("JSON");
    assert_eq!(shown["cost"]["usd"], 0.5, "{shown}");
    assert_eq!(shown["children"][0]["task_id"], "FRK-2", "{shown}");
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_a_tasks_diff_before_and_after_integration() {
    use serde_json::json;

    let repository = a_project_with_a_task("read-show-diff");
    let ran = run_in(&repository.path, &["task", "show", "FRK-1", "--diff"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("has no branch yet"), "{}", ran.err);

    // A Software Developer's task with no `change` works on `feature/<id>` (5.14).
    let branch_with = |task: &str, file: &str| {
        repository.git(&["checkout", "-q", "-b", &format!("feature/{task}")]);
        repository.write(file, "done\n");
        repository.git(&["add", "--", file]);
        repository.git(&["commit", "-q", "-m", &format!("Add {file}")]);
        repository.git(&["checkout", "-q", "main"]);
    };
    branch_with("FRK-1", "done.txt");
    let ran = run_in(&repository.path, &["task", "show", "FRK-1", "--diff"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("+++ b/done.txt"), "{}", ran.out);

    repository.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge FRK-1",
        "feature/FRK-1",
    ]);
    let sha = repository.git_output(&["rev-parse", "HEAD"]);
    project::record(
        &repository,
        "FRK-1",
        "task.integrated",
        &json!({ "sha": sha, "into": "main", "integrated_by": "human" }),
    );
    let ran = run_in(&repository.path, &["task", "show", "FRK-1", "--diff"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("+++ b/done.txt"), "{}", ran.out);

    repository.write("second.yaml", &a_request("Show a second board"));
    let filed = run_in(&repository.path, &["task", "create", "second.yaml"]);
    assert_eq!(filed.code, 0, "{}", filed.err);
    branch_with("FRK-2", "b.txt");
    repository.git(&["merge", "-q", "--ff-only", "feature/FRK-2"]);
    let head = repository.git_output(&["rev-parse", "HEAD"]);
    project::record(
        &repository,
        "FRK-2",
        "task.integrated",
        &json!({ "sha": head, "into": "main", "integrated_by": "human" }),
    );
    let ran = run_in(&repository.path, &["task", "show", "FRK-2", "--diff"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("feature/FRK-2 is wholly in main"),
        "{}",
        ran.out
    );

    repository.write("third.yaml", &a_request("Show a third board"));
    let filed = run_in(&repository.path, &["task", "create", "third.yaml"]);
    assert_eq!(filed.code, 0, "{}", filed.err);
    let sized = run_in(
        &repository.path,
        &["triage", "FRK-3", "large", "--reason", "Many boards."],
    );
    assert_eq!(sized.code, 0, "{}", sized.err);
    let ran = run_in(&repository.path, &["task", "show", "FRK-3", "--diff"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("is an epic and has no branch"),
        "{}",
        ran.err
    );
}

/// `a_project_with_a_task` and a second request, both accepted, with every metric a different
/// number so that no line or field can be printed from another's value:
///
/// - FRK-1 (one `test` criterion) is verified twice, the human unblocking it once and one
///   escalation raised; FRK-2 (a `command` and a `review` criterion) is verified once and accepted
///   first time, and escalated once after. So two accepted tasks, 50% first pass, 1.50
///   interventions each, and two criteria of three run by Farik.
/// - Three sessions cost $0.50 of implement, $1.00 of verify, and $0.50 of triage, each in an ISO
///   week of its own: $1.00 per accepted task, of which the largest purpose is $0.50, over three
///   active weeks.
#[cfg(unix)]
fn accepted_project(name: &str) -> TempRepo {
    use chrono::TimeZone;
    use serde_json::json;

    let repository = a_project_with_a_task(name);
    repository.write(
        "second.yaml",
        &a_request("Show one task").replace(
            r"  - id: C1
    text: Every test in the workspace passes.
    satisfies:
      - R1
    verification:
      method: test
      command: cargo test --workspace
      new_tests_required: true
",
            r"  - id: C1
    text: The board command exits cleanly.
    satisfies:
      - R1
    verification:
      method: command
      command: farik board
      expect:
        exit_code: 0
  - id: C2
    text: The board reads well.
    satisfies:
      - R1
    verification:
      method: review
      rubric:
        - Is each line one task?
",
        ),
    );
    let filed = run_in(&repository.path, &["task", "create", "second.yaml"]);
    assert_eq!(filed.code, 0, "{}", filed.err);
    assert!(filed.out.starts_with("FRK-2 "), "{}", filed.out);

    let by = |actor: &str| json!({ "actor": actor, "requested_by": actor });
    for (from, to, actor) in [
        ("in_progress", "blocked", "assignee"),
        ("blocked", "in_progress", "human"),
        ("in_progress", "verifying", "assignee"),
        ("verifying", "escalated", "governor"),
    ] {
        project::moved(&repository, "FRK-1", from, to, &by(actor));
    }
    project::record(
        &repository,
        "FRK-1",
        "escalation.raised",
        &json!({ "reason": "iterations", "detail": "Two tries." }),
    );
    for (from, to, actor) in [
        ("escalated", "in_progress", "human"),
        ("in_progress", "verifying", "assignee"),
        ("verifying", "accepted", "product_manager"),
    ] {
        project::moved(&repository, "FRK-1", from, to, &by(actor));
    }
    for (from, to, actor) in [
        ("in_progress", "verifying", "assignee"),
        ("verifying", "accepted", "product_manager"),
    ] {
        project::moved(&repository, "FRK-2", from, to, &by(actor));
    }
    project::record(
        &repository,
        "FRK-2",
        "escalation.raised",
        &json!({ "reason": "integration", "detail": "A conflict." }),
    );

    for (task, session, purpose, usd, day) in [
        ("FRK-1", "s1", "implement", 0.5, 7),
        ("FRK-1", "s2", "verify", 1.0, 14),
        ("FRK-2", "s3", "triage", 0.5, 21),
    ] {
        project::record_on(
            &repository,
            task,
            Some(("dev-a", session)),
            "cost.recorded",
            &json!({
                "purpose": purpose,
                "model_id": "claude-sonnet-4-5",
                "usage": {
                    "input_tokens": 1000,
                    "output_tokens": 100,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0
                },
                "cost_usd": usd
            }),
            Utc.with_ymd_and_hms(2026, 9, day, 10, 0, 0)
                .single()
                .expect("a real hour"),
        );
    }
    repository
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_harness_metrics() {
    let repository = accepted_project("read-metrics");
    let ran = run_in(&repository.path, &["metrics"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        [
            "accepted tasks: 2",
            "first-pass acceptance: 50.0%",
            "human interventions per accepted task: 1.50",
            "cost per accepted task: $1.00",
            "  triage: $0.25",
            "  refine: $0.00",
            "  plan: $0.00",
            "  implement: $0.25",
            "  verify: $0.50",
            "  ceremony: $0.00",
            "  conversation: $0.00",
            "criteria verified by command, test, or artifact: 66.7%",
            "active weeks: 3",
        ]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_none_before_a_task_is_accepted() {
    let repository = a_project_with_a_task("read-metrics-none");
    let ran = run_in(&repository.path, &["metrics"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    let none = "none yet, no task has been accepted";
    assert_eq!(
        ran.out.lines().map(str::to_string).collect::<Vec<_>>(),
        [
            "accepted tasks: 0".to_string(),
            format!("first-pass acceptance: {none}"),
            format!("human interventions per accepted task: {none}"),
            format!("cost per accepted task: {none}"),
            format!("criteria verified by command, test, or artifact: {none}"),
            "active weeks: 0".to_string(),
        ]
    );
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_harness_metrics_as_json() {
    let repository = accepted_project("read-metrics-json");
    let ran = run_in(&repository.path, &["metrics", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let metrics: Value = serde_json::from_str(ran.out.trim()).expect("one JSON object");
    assert_eq!(
        metrics,
        serde_json::json!({
            "accepted_tasks": 2,
            "first_pass_acceptance_rate": 0.5,
            "interventions_per_accepted_task": 1.5,
            "cost_per_accepted_task_usd": {
                "total": 1.0,
                "by_purpose": {
                    "triage": 0.25,
                    "refine": 0.0,
                    "plan": 0.0,
                    "implement": 0.25,
                    "verify": 0.5,
                    "ceremony": 0.0,
                    "conversation": 0.0
                }
            },
            "mechanically_verified_criteria_share": 2.0 / 3.0,
            "active_weeks": 3
        })
    );

    let repository = a_project_with_a_task("read-metrics-json-none");
    let ran = run_in(&repository.path, &["metrics", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let metrics: Value = serde_json::from_str(ran.out.trim()).expect("one JSON object");
    for field in [
        "first_pass_acceptance_rate",
        "interventions_per_accepted_task",
        "cost_per_accepted_task_usd",
        "mechanically_verified_criteria_share",
    ] {
        assert_eq!(metrics[field], Value::Null, "{field}: {metrics}");
    }
    assert_eq!(metrics["active_weeks"], 0);
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_metrics_of_a_sprint() {
    use serde_json::json;

    let repository = a_project_with_a_task("read-metrics-sprint");
    let started = run_in(&repository.path, &["sprint", "start"]);
    assert_eq!(started.code, 0, "{}", started.err);
    plan_sprint(
        &project::tool_deps(&repository),
        &["FRK-1".parse().expect("a task id")],
        &PlannedBy::Governor,
    )
    .expect("FRK-1 is planned into S1");

    let by = |actor: &str| json!({ "actor": actor, "requested_by": actor });
    project::moved(
        &repository,
        "FRK-1",
        "in_progress",
        "verifying",
        &by("assignee"),
    );
    project::moved(
        &repository,
        "FRK-1",
        "verifying",
        "accepted",
        &by("product_manager"),
    );
    project::record_on(
        &repository,
        "FRK-1",
        Some(("dev-a", "s1")),
        "cost.recorded",
        &json!({
            "purpose": "implement",
            "model_id": "claude-sonnet-4-5",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 100,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            },
            "cost_usd": 1.0
        }),
        at(),
    );

    let ran = run_in(&repository.path, &["metrics", "--sprint", "S1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        [
            "accepted tasks: 1",
            "first-pass acceptance: 100.0%",
            "human interventions per accepted task: 0.00",
            "cost per accepted task: $1.00",
            "  triage: $0.00",
            "  refine: $0.00",
            "  plan: $0.00",
            "  implement: $1.00",
            "  verify: $0.00",
            "  ceremony: $0.00",
            "  conversation: $0.00",
            "criteria verified by command, test, or artifact: 100.0%",
            "active weeks: 1",
        ]
    );

    let as_json = run_in(&repository.path, &["metrics", "--sprint", "S1", "--json"]);
    assert_eq!(as_json.code, 0, "{}", as_json.err);
    let metrics: Value = serde_json::from_str(as_json.out.trim()).expect("one JSON object");
    assert_eq!(
        metrics,
        json!({
            "accepted_tasks": 1,
            "first_pass_acceptance_rate": 1.0,
            "interventions_per_accepted_task": 0.0,
            "cost_per_accepted_task_usd": {
                "total": 1.0,
                "by_purpose": {
                    "triage": 0.0,
                    "refine": 0.0,
                    "plan": 0.0,
                    "implement": 1.0,
                    "verify": 0.0,
                    "ceremony": 0.0,
                    "conversation": 0.0
                }
            },
            "mechanically_verified_criteria_share": 1.0,
            "active_weeks": 1
        })
    );

    let unknown = run_in(&repository.path, &["metrics", "--sprint", "S9"]);
    assert_eq!(unknown.code, 1, "{}{}", unknown.out, unknown.err);
    assert!(unknown.err.contains("S9"), "{}", unknown.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_control_characters_an_agent_wrote_escaped() {
    use serde_json::json;

    let repository = a_project_with_a_task("read-show-escaped");
    project::record(
        &repository,
        "FRK-1",
        "question.asked",
        &json!({ "question": "Clear\u{1b}[2Jthe\u{7}screen?\tNo.", "asked_by": "pm" }),
    );

    let shown = run_in(&repository.path, &["task", "show", "FRK-1"]);

    assert_eq!(shown.code, 0, "{}", shown.err);
    assert!(!shown.out.contains(['\u{1b}', '\u{7}']), "{:?}", shown.out);
    assert!(
        shown
            .out
            .contains("from pm: Clear\\u001b[2Jthe\\u0007screen?\tNo."),
        "{:?}",
        shown.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_there_is_no_sprint_yet() {
    let repository = TempRepo::new("read-sprint-none");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.commit_at("a project", COMMITTED);
    run_in(&repository.path, &["init"]);

    let ran = run_in(&repository.path, &["sprint", "show"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("no sprint yet"), "{}", ran.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn escapes_what_an_agent_wrote_in_the_channel() {
    use serde_json::json;

    let repository = a_project_with_a_task("read-channel-escaped");
    project::record_as(
        &repository,
        "FRK-1",
        Some(("pm", "session-1")),
        "message.posted",
        &json!({
            "author": "pm",
            "kind": "reaction",
            "text": "FRK-1 is \u{1b}[31mready",
            "mentions": []
        }),
    );

    let shown = run_in(&repository.path, &["channel"]);

    assert_eq!(shown.code, 0, "{}", shown.err);
    assert!(!shown.out.contains('\u{1b}'), "{:?}", shown.out);
    assert!(
        shown.out.contains("pm FRK-1 is \\u001b[31mready"),
        "{:?}",
        shown.out
    );
}
