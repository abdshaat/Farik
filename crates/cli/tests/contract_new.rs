//! `farik contract new` (`docs/SPEC.md` 5.13): a request filed from a brief, its contract written
//! with the Product Manager, and its questions asked at the terminal, driven by the recorded
//! adapter through the harness's engine.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use farik::{Engine, Interrupts};
use farik_protocol::command::{Command, RequestSize};
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::recorded::fixtures::{
    refine_asks_frk_1, refine_writes_epic_frk_1, refine_writes_task_frk_1, tool_runner,
    triage_frk_1_large,
};
use farik_runtime::{RecordedAdapter, RuntimeAdapter, Transcript};
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};

use project::{
    LiveDriver, a_bare_env, a_claude_saying, a_team, events, filed, files_of, hold_the_run_lock,
    joined, record, record_as, run_with, scratch, status_of,
};

const BRIEF: &str = "Add done.txt and a check that it exists.";

/// An engine replaying `transcripts`, whose Farik tool calls the driving process's daemon answers.
fn recorded(transcripts: Vec<Transcript>) -> Engine {
    Engine::Given(Arc::new(move |daemon| {
        let adapter: Arc<dyn RuntimeAdapter> = Arc::new(RecordedAdapter::with_tools(
            transcripts.clone(),
            tool_runner(daemon),
        ));
        adapter
    }))
}

/// `farik contract new <args>` with `transcripts` and `stdin`.
fn contract_new(
    repository: &TempRepo,
    args: &[&str],
    transcripts: Vec<Transcript>,
    stdin: &str,
) -> project::Ran {
    let input = stdin.to_string();
    let arguments: Vec<&str> = ["contract", "new"].iter().chain(args).copied().collect();
    run_with(&repository.path, &arguments, |io| {
        io.engine = recorded(transcripts);
        io.stdin = Box::new(std::io::Cursor::new(input));
    })
}

/// Whether `text` holds each of `parts`, in that order.
fn in_order(text: &str, parts: &[&str]) -> bool {
    let mut rest = text;
    for part in parts {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

fn question_seq(repository: &TempRepo) -> u64 {
    events(repository, &[EventKind::QuestionAsked])[0]
        .envelope
        .seq
}

/// `transcript` with every `FRK-1` in it naming `task` instead.
fn about(transcript: &Transcript, task: &str) -> Transcript {
    Transcript::from_jsonl(
        &transcript
            .lines()
            .collect::<Vec<_>>()
            .join("\n")
            .replace("FRK-1", task),
    )
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn writes_a_small_request_with_the_product_manager() {
    let repository = a_team("new-small");
    // Two drafts besides the request: FRK-1, which triage would start a session for, and FRK-2,
    // whose question waits on the person. Neither is this command's to act on or to ask.
    filed(&repository, "Add done.txt first");
    let other = filed(&repository, "Add done.txt second");
    record(
        &repository,
        &other,
        "question.asked",
        &json!({ "question": "Which done.txt?", "asked_by": "pm" }),
    );

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF, "--size", "small"],
        vec![about(&refine_writes_task_frk_1(), "FRK-3")],
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        in_order(
            &ran.out,
            &[
                &format!("FRK-3 filed as a draft request: {BRIEF}"),
                "FRK-3 sized small by you, so it is a standalone task",
                "contract.written",
                "C1",
                "readiness: passed",
            ]
        ),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("answer> "), "{}", ran.out);
    assert_eq!(status_of(&repository, "FRK-3"), "ready");
    let started = events(&repository, &[EventKind::SessionStarted]);
    assert_eq!(started.len(), 1);
    assert_eq!(
        started[0]
            .envelope
            .ids
            .task_id
            .as_ref()
            .map(|task| task.as_str()),
        Some("FRK-3")
    );
    assert!(matches!(
        &started[0].body,
        EventBody::SessionStarted(body) if body.purpose.to_string() == "refine"
    ));
    assert_eq!(status_of(&repository, "FRK-1"), "draft");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn asks_its_questions_at_the_terminal() {
    let transcripts = || {
        vec![
            triage_frk_1_large(),
            refine_asks_frk_1(),
            refine_writes_epic_frk_1(),
        ]
    };
    let repository = a_team("new-asks");

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF],
        transcripts(),
        "\nNo, one line.\n",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let n = question_seq(&repository);
    for expected in [
        "large by pm: A file and the check that it exists.",
        &format!("question {n} from pm: Should done.txt be empty?"),
        "readiness: passed the structural checks",
        "FRK-1 awaits your approval",
    ] {
        assert!(ran.out.contains(expected), "{expected:?} in {}", ran.out);
    }
    assert_eq!(ran.out.matches("answer> ").count(), 2, "{}", ran.out);
    // The blank line is asked again, not sent as an answer for handle to refuse.
    assert!(
        !ran.err.lines().any(|line| line.starts_with("farik: ")),
        "{}",
        ran.err
    );
    let answers = events(&repository, &[EventKind::QuestionAnswered]);
    assert_eq!(answers.len(), 1);
    assert!(matches!(
        &answers[0].body,
        EventBody::QuestionAnswered(body) if body.answer == "No, one line."
    ));
    assert_eq!(status_of(&repository, "FRK-1"), "escalated");

    let quiet = a_team("new-asks-json");
    let ran = run_with(
        &quiet.path,
        &["--json", "contract", "new", "--brief", BRIEF],
        |io| {
            io.engine = recorded(transcripts());
            io.stdin = Box::new(std::io::Cursor::new("No, one line.\n".to_string()));
        },
    );
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert_eq!(ran.out.trim().lines().count(), 1, "{}", ran.out);
    let object: Value = serde_json::from_str(ran.out.trim()).expect("one JSON object");
    assert_eq!(object["readiness"]["state"], "structural", "{object}");
    assert_eq!(object["question_open"], Value::Null, "{object}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn leaves_the_question_open_when_input_ends() {
    let repository = a_team("new-input-ends");

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF],
        vec![triage_frk_1_large(), refine_asks_frk_1()],
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let n = question_seq(&repository);
    assert!(
        ran.out
            .contains(&format!("the question stays open: farik answer {n}")),
        "{}",
        ran.out
    );
    assert!(events(&repository, &[EventKind::QuestionAnswered]).is_empty());
    assert_eq!(status_of(&repository, "FRK-1"), "refining");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn ends_at_the_prompt_on_an_interrupt() {
    let repository = a_team("new-interrupt");
    let (reader, writer) = std::io::pipe().expect("a pipe");
    let (interrupt, interrupts) = tokio::sync::mpsc::unbounded_channel();
    let root = repository.path.clone();
    let running = std::thread::spawn(move || {
        run_with(&root, &["contract", "new", "--brief", BRIEF], |io| {
            io.engine = recorded(vec![triage_frk_1_large(), refine_asks_frk_1()]);
            io.stdin = Box::new(reader);
            io.interrupts = Interrupts::Channel(interrupts);
        })
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    while events(&repository, &[EventKind::QuestionAsked]).is_empty() {
        assert!(Instant::now() < deadline, "no question in time");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    interrupt.send(()).expect("the command listens");
    let ran = joined(running, "the command");

    assert_eq!(ran.code, 130, "{}\n{}", ran.out, ran.err);
    let n = question_seq(&repository);
    assert!(
        ran.out
            .contains(&format!("the question stays open: farik answer {n}")),
        "{}",
        ran.out
    );
    assert!(events(&repository, &[EventKind::QuestionAnswered]).is_empty());
    drop(hold_the_run_lock(&repository));
    let mut writer = writer;
    let _ = writer.write_all(b"too late\n");
}

/// Spends the team's day with one seeded `cost.recorded`, so that no session starts.
fn a_spent_day(repository: &TempRepo) {
    record_as(
        repository,
        "",
        Some(("dev-a", "s-0")),
        "cost.recorded",
        &json!({
            "purpose": "implement",
            "model_id": "claude-opus-5",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 100,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            },
            "cost_usd": 25.0
        }),
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_why_it_stopped_before_a_contract() {
    let repository = a_team("new-day-spent");
    a_spent_day(&repository);
    // A judgement from before FRK-1 last moved into refining, which says nothing of it now.
    record(
        &repository,
        "FRK-1",
        "contract.evaluated",
        &json!({ "gate": "definition_of_ready", "passed": false, "failures": ["stale"] }),
    );

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF, "--size", "small"],
        Vec::new(),
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert_eq!(status_of(&repository, "FRK-1"), "refining");
    assert!(
        ran.out
            .contains("readiness: not judged yet (the team's daily budget is spent)"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("stale"), "{}", ran.out);
    assert!(events(&repository, &[EventKind::SessionStarted]).is_empty());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn locks_only_a_contract_the_product_manager_wrote() {
    let repository = a_team("new-lock-unwritten");
    a_spent_day(&repository);

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF, "--size", "small", "--lock"],
        Vec::new(),
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(events(&repository, &[EventKind::ContractLocked]).is_empty());
    let contract = files_of(&repository)
        .read_contract(&"FRK-1".parse().expect("a task id"))
        .expect("the contract reads");
    assert!(!contract.locked);
    assert!(
        ran.out.contains(
            "not locked: the Product Manager has written no contract for FRK-1 yet: run farik \
             contract lock FRK-1 once it has"
        ),
        "{}",
        ran.out
    );
}

/// Nothing was filed and nothing is left running: the log as it was, no FRK-1, no daemon, and
/// the run lock free.
fn nothing_filed(repository: &TempRepo, before: usize) {
    assert_eq!(events(repository, &[]).len(), before);
    assert!(!repository.path.join(".farik/contracts/FRK-1.yaml").exists());
    assert!(!repository.path.join(".farik/local/daemon.json").exists());
    drop(hold_the_run_lock(repository));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn files_nothing_when_the_start_refuses() {
    let repository = a_team("new-start-refused");
    let before = events(&repository, &[]).len();
    let (_old, old_claude) = a_claude_saying("new-old-claude", "2.1.200 (Claude Code)");
    let nowhere = scratch("new-no-claude");
    let with_key = |path: String| {
        BTreeMap::from([
            ("PATH".to_string(), path),
            ("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string()),
        ])
    };
    for (said, env) in [
        ("CLAUDE_CODE_OAUTH_TOKEN", a_bare_env()),
        (
            "there is no claude on PATH",
            with_key(nowhere.display().to_string()),
        ),
        ("2.1.272", with_key(old_claude)),
    ] {
        let ran = run_with(
            &repository.path,
            &["contract", "new", "--brief", BRIEF, "--size", "small"],
            |io| io.env = env,
        );
        assert_eq!(ran.code, 1, "{}", ran.out);
        assert!(ran.err.contains(said), "{said:?} in {}", ran.err);
        nothing_filed(&repository, before);
    }

    std::fs::write(
        repository.path.join(".farik/prices.json"),
        "{\"version\": 2}",
    )
    .expect("the override is written");
    let ran = contract_new(&repository, &["--brief", BRIEF], Vec::new(), "");
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains(".farik/prices.json"), "{}", ran.err);
    nothing_filed(&repository, before);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn locks_the_contract_it_wrote_when_asked() {
    let repository = a_team("new-lock");

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF, "--size", "small", "--lock"],
        vec![refine_writes_task_frk_1()],
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let last = events(&repository, &[]).pop().expect("an event");
    assert!(
        matches!(&last.body, EventBody::ContractLocked(body) if body.locked_by == "human"),
        "{last:?}"
    );
    let contract = files_of(&repository)
        .read_contract(&"FRK-1".parse().expect("a task id"))
        .expect("the contract reads");
    assert!(contract.locked);
    assert!(
        ran.out.contains("locked: the contract is yours (5.11)"),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_brief_too_short_to_be_an_intent() {
    let repository = a_team("new-short");
    let before = events(&repository, &[]).len();

    let ran = contract_new(&repository, &["--brief", "Fix it"], Vec::new(), "");

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert_eq!(events(&repository, &[]).len(), before);
    assert!(!repository.path.join(".farik/contracts/FRK-1.yaml").exists());
    assert!(!repository.path.join(".farik/local/daemon.json").exists());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn hands_the_request_to_the_driving_process() {
    let repository = a_team("new-handed");
    let driver = LiveDriver::new(&repository);

    let ran = contract_new(
        &repository,
        &["--brief", BRIEF, "--size", "large"],
        Vec::new(),
        "",
    );

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(repository.path.join(".farik/contracts/FRK-1.yaml").exists());
    assert!(matches!(
        driver.commands().as_slice(),
        [Command::RequestTriage { task_id, size: RequestSize::Large, .. }]
            if task_id.as_str() == "FRK-1"
    ));
    assert!(ran.out.contains("(pid "), "{}", ran.out);
    assert!(ran.out.contains("takes it from here"), "{}", ran.out);
    assert!(events(&repository, &[EventKind::SessionStarted]).is_empty());

    let before = events(&repository, &[]).len();
    let ran = contract_new(&repository, &["--brief", BRIEF, "--lock"], Vec::new(), "");
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert_eq!(events(&repository, &[]).len(), before);
    assert!(!repository.path.join(".farik/contracts/FRK-2.yaml").exists());
}
