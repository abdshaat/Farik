//! `farik run` and `farik plan` (`docs/SPEC.md` sections 5.7, 8.2, and 8.3): the start, the loop,
//! Ctrl-C, and what waits on the human, driven by the recorded adapter through the harness's
//! engine, never by a Claude Code session.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use farik::{Engine, Interrupts};
use farik_core::pricing::Usage;
use farik_protocol::event::{EventBody, EventKind, SessionEndedBodyReason};
use farik_runtime::recorded::fixtures::{
    UsageThenWaitAdapter, accept_frk_1, implement_finishes_frk_1, plan_assigns_frk_1,
    refine_writes_task_frk_1, review_writes_note, tool_runner,
};
use farik_runtime::{RecordedAdapter, RuntimeAdapter, Transcript};
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};

use farik_core::team::fixtures::an_agent_wire;
use project::{
    a_bare_env, a_claude_saying, a_project, a_team, a_team_with, events, filed, hold_the_run_lock,
    moved, no_sandbox, record, record_as, run, run_with, status_of,
};

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

/// An engine whose sessions are `adapter`'s.
fn given(adapter: &Arc<UsageThenWaitAdapter>) -> Engine {
    let adapter = Arc::clone(adapter);
    Engine::Given(Arc::new(move |_| {
        let adapter: Arc<dyn RuntimeAdapter> = adapter.clone();
        adapter
    }))
}

fn daemon_file(repository: &TempRepo) -> PathBuf {
    repository.path.join(".farik/local/daemon.json")
}

/// Waits until the log holds `count` events of `kind`.
fn wait_for(repository: &TempRepo, kind: EventKind, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while events(repository, &[kind]).len() < count {
        assert!(Instant::now() < deadline, "no {count} {kind} in time");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The purpose of each session started, in order.
fn purposes(repository: &TempRepo) -> Vec<String> {
    events(repository, &[EventKind::SessionStarted])
        .iter()
        .map(|event| match &event.body {
            EventBody::SessionStarted(body) => body.purpose.to_string(),
            other => panic!("a session.started, got {other:?}"),
        })
        .collect()
}

/// `a_request` filed and sized small by the human.
fn a_small_request(repository: &TempRepo) -> String {
    let task = filed(repository, "Add done.txt");
    let ran = run(
        &repository.path,
        &["triage", &task, "small", "--reason", "One file."],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    task
}

/// The warning every start in no-sandbox mode prints.
fn warned(err: &str) -> bool {
    err.contains("~/.git-credentials") && err.contains("a git hidden in a script")
}

fn lock_is_free(repository: &TempRepo) {
    drop(hold_the_run_lock(repository));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_run_without_a_credential() {
    let repository = a_project("run-no-credential");
    let before = events(&repository, &[]).len();

    let ran = run_with(&repository.path, &["run"], |io| io.env = a_bare_env());

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("ANTHROPIC_API_KEY"), "{}", ran.err);
    assert!(ran.err.contains("CLAUDE_CODE_OAUTH_TOKEN"), "{}", ran.err);
    assert!(!daemon_file(&repository).exists());
    lock_is_free(&repository);
    assert_eq!(events(&repository, &[]).len(), before);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_claude_code_older_than_the_minimum() {
    let repository = a_project("run-old-claude");
    let (_directory, path) = a_claude_saying("old-claude", "2.1.200 (Claude Code)");

    let ran = run_with(&repository.path, &["run"], |io| {
        io.env.insert("PATH".to_string(), path);
        io.env
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());
    });

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("2.1.272"), "{}", ran.err);
    assert!(!daemon_file(&repository).exists());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn names_the_credential_it_chose_and_never_prints_it() {
    let repository = a_project("run-credential");
    let (_directory, path) = a_claude_saying("new-claude", "2.1.280 (Claude Code)");

    let both = run_with(&repository.path, &["run"], |io| {
        io.env.insert("PATH".to_string(), path.clone());
        io.env
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-key-secret".to_string());
        io.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "oauth-token-secret".to_string(),
        );
    });
    assert_eq!(both.code, 0, "{}", both.err);
    assert!(
        both.out
            .lines()
            .any(|line| line == "credential: ANTHROPIC_API_KEY (an API key)"),
        "{}",
        both.out
    );
    for secret in ["sk-key-secret", "oauth-token-secret"] {
        assert!(!both.out.contains(secret) && !both.err.contains(secret));
    }

    let token = run_with(&repository.path, &["run"], |io| {
        io.env.insert("PATH".to_string(), path.clone());
        io.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "oauth-token-secret".to_string(),
        );
    });
    assert_eq!(token.code, 0, "{}", token.err);
    assert!(
        token
            .out
            .lines()
            .any(|line| line == "credential: CLAUDE_CODE_OAUTH_TOKEN (a subscription token)"),
        "{}",
        token.out
    );
    assert!(!token.out.contains("oauth-token-secret") && !token.err.contains("oauth-token-secret"));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn warns_on_every_start_in_no_sandbox_mode() {
    let repository = a_project("run-warns");
    no_sandbox(&repository);
    for _ in 0..2 {
        let ran = run_with(&repository.path, &["run"], |io| {
            io.engine = recorded(Vec::new());
        });
        assert_eq!(ran.code, 0, "{}", ran.err);
        assert!(warned(&ran.err), "{}", ran.err);
    }

    let quiet = a_project("run-warns-not");
    let ran = run_with(&quiet.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!warned(&ran.err), "{}", ran.err);

    let task = filed(&repository, "Add done.txt");
    let n = record(
        &repository,
        &task,
        "question.asked",
        &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
    )
    .envelope
    .seq;
    let ran = run(&repository.path, &["answer", &n.to_string(), "Yes."]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!warned(&ran.err), "{}", ran.err);
}

/// What every start says of `claude-unknown-9`, used by `dev`.
const UNPRICED_WARNING: &str = "warning: no price table prices claude-unknown-9 (used by dev): \
    its usage is recorded at no cost, and no dollar limit counts it. Add it to \
    .farik/prices.json to price it.";

/// A team of `pm` and `dev`, with `dev` on a model the shipped table does not price.
fn a_team_on_an_unpriced_model(name: &str) -> TempRepo {
    a_team_with(name, |wire| {
        let mut dev = an_agent_wire("dev", "software_developer");
        dev["model"] = json!({ "id": "claude-unknown-9" });
        wire["agents"] = json!([an_agent_wire("pm", "product_manager"), dev]);
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn warns_on_every_start_of_a_model_no_table_prices() {
    let repository = a_team_on_an_unpriced_model("run-warns-unpriced");
    for _ in 0..2 {
        let ran = run_with(&repository.path, &["run"], |io| {
            io.engine = recorded(Vec::new());
        });
        assert_eq!(ran.code, 0, "{}", ran.err);
        assert!(
            ran.err.lines().any(|line| line == UNPRICED_WARNING),
            "{}",
            ran.err
        );
    }

    let priced = a_team("run-warns-priced");
    let ran = run_with(&priced.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.err.contains("no price table prices"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_start_on_a_price_table_it_cannot_read() {
    let repository = a_team("run-prices-unreadable");
    std::fs::write(
        repository.path.join(".farik/prices.json"),
        "{\"version\": 2}",
    )
    .expect("the override is written");
    let before = events(&repository, &[]).len();

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains(".farik/prices.json"), "{}", ran.err);
    assert!(!daemon_file(&repository).exists());
    lock_is_free(&repository);
    assert_eq!(events(&repository, &[]).len(), before);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_second_driver() {
    let repository = a_project("run-second");
    let _lock = hold_the_run_lock(&repository);
    for command in ["run", "plan"] {
        let ran = run_with(&repository.path, &[command], |io| {
            io.engine = recorded(Vec::new());
        });
        assert_eq!(ran.code, 1, "{}", ran.out);
        assert!(
            ran.err
                .contains("another farik process is driving this project"),
            "{}",
            ran.err
        );
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn runs_a_task_to_acceptance_and_says_what_waits() {
    let repository = a_team("run-to-acceptance");
    let task = a_small_request(&repository);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(vec![
            refine_writes_task_frk_1(),
            plan_assigns_frk_1(),
            implement_finishes_frk_1(),
            review_writes_note(),
            accept_frk_1(),
        ]);
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(&format!("{task}: "))),
        "{}",
        ran.out
    );
    assert!(
        lines.contains(&"idle: nothing on the board needs doing"),
        "{}",
        ran.out
    );
    assert!(
        lines.contains(
            &format!("{task} waits for you to integrate it: farik integrate {task}").as_str()
        ),
        "{}",
        ran.out
    );
    assert_eq!(
        purposes(&repository),
        ["refine", "plan", "implement", "verify", "verify"]
    );
    assert!(!daemon_file(&repository).exists());

    let ran = run(&repository.path, &["integrate", &task]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("merged"), "{}", ran.out);
    assert!(
        std::process::Command::new("git")
            .args(["cat-file", "-e", "main:done.txt"])
            .current_dir(&repository.path)
            .status()
            .expect("git runs")
            .success()
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn recovers_before_the_first_tick() {
    let repository = a_project("run-recovers");
    let task = filed(&repository, "Add done.txt");
    let ran = run(&repository.path, &["cancel", &task, "Not", "needed."]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    record_as(
        &repository,
        &task,
        Some(("pm", "s-1")),
        "session.started",
        &json!({ "purpose": "refine", "model": "claude-opus-5", "effort": "high" }),
    );

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.contains("recovered: sessions interrupted 1,"),
        "{}",
        ran.out
    );
    let ended = events(&repository, &[EventKind::SessionEnded]);
    assert_eq!(ended.len(), 1);
    assert!(matches!(
        &ended[0].body,
        EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Aborted
    ));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_after_the_session_then_aborts_it_on_ctrl_c() {
    let repository = a_team("run-ctrl-c");
    let task = a_small_request(&repository);
    let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage::default()));
    let (interrupt, interrupts) = tokio::sync::mpsc::unbounded_channel();
    let root = repository.path.clone();
    let engine = given(&adapter);

    let running = std::thread::spawn(move || {
        run_with(&root, &["run"], |io| {
            io.engine = engine;
            io.interrupts = Interrupts::Channel(interrupts);
        })
    });
    wait_for(&repository, EventKind::SessionStarted, 1);
    interrupt.send(()).expect("the run listens");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(adapter.aborts(), 0);
    interrupt.send(()).expect("the run listens");
    let ran = running.join().expect("the run ends");

    assert_eq!(ran.code, 130, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.err.contains("stopping after the current session"),
        "{}",
        ran.err
    );
    assert_eq!(adapter.aborts(), 1);
    let ended = events(&repository, &[EventKind::SessionEnded]);
    assert!(matches!(
        &ended[0].body,
        EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Aborted
    ));
    assert!(events(&repository, &[EventKind::EscalationRaised]).is_empty());
    assert_eq!(status_of(&repository, &task), "refining");
    assert!(!daemon_file(&repository).exists());
    lock_is_free(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_a_plan_through_farik_stop() {
    let repository = a_team("plan-stop");
    a_small_request(&repository);
    let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage::default()));
    let root = repository.path.clone();
    let engine = given(&adapter);

    let planning = std::thread::spawn(move || {
        run_with(&root, &["plan"], |io| {
            io.engine = engine;
        })
    });
    wait_for(&repository, EventKind::SessionStarted, 1);
    let stop = run(&repository.path, &["stop"]);
    assert_eq!(stop.code, 0, "{}", stop.err);
    adapter.complete();
    let ran = planning.join().expect("the plan ends");

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(ran.out.lines().any(|line| line == "stopped"), "{}", ran.out);
    assert_eq!(adapter.started().len(), 1);
}

/// Records `task`'s moves from `draft` through `path`.
fn walked(repository: &TempRepo, task: &str, path: &[&str]) {
    let mut from = "draft";
    for to in path {
        moved(repository, task, from, to, &json!({}));
        from = to;
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn plans_without_starting_work() {
    let repository = a_team("plan-no-work");
    let task = a_small_request(&repository);
    walked(&repository, &task, &["refining", "ready"]);

    let ran = run_with(&repository.path, &["plan"], |io| {
        io.engine = recorded(vec![plan_assigns_frk_1()]);
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert_eq!(status_of(&repository, &task), "assigned");
    assert!(
        !repository
            .path
            .join(format!(".farik/local/worktrees/{task}"))
            .exists()
    );
    assert_eq!(purposes(&repository), ["plan"]);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_what_waits_on_the_human() {
    let repository = a_team("run-waiting");
    let first = a_small_request(&repository);
    walked(&repository, &first, &["refining"]);
    let n = record(
        &repository,
        &first,
        "question.asked",
        &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
    )
    .envelope
    .seq;
    let second = filed(&repository, "Add done.txt and its check");
    let ran = run(
        &repository.path,
        &["triage", &second, "large", "--reason", "Two parts."],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    walked(&repository, &second, &["refining", "escalated"]);
    record(
        &repository,
        &second,
        "escalation.raised",
        &json!({ "reason": "approval", "detail": "contract_requires_human" }),
    );
    let third = a_small_request(&repository);
    walked(&repository, &third, &["refining", "escalated"]);
    record(
        &repository,
        &third,
        "escalation.raised",
        &json!({ "reason": "iterations", "detail": "rejected three times" }),
    );

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let after: Vec<&str> = ran
        .out
        .lines()
        .skip_while(|line| !line.starts_with("idle:"))
        .skip(1)
        .collect();
    let expected = [
        format!("question {n} on {first} from pm: "),
        format!("  farik answer {n} <your answer>"),
        format!("{second} awaits your approval: farik approve {second}"),
        format!("{third} is escalated (iterations)"),
    ];
    assert_eq!(after.len(), expected.len(), "{}", ran.out);
    for (line, start) in after.iter().zip(&expected) {
        assert!(
            line.starts_with(start.as_str()),
            "{line:?} does not start {start:?}"
        );
    }

    let ran = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    let last: Value = serde_json::from_str(ran.out.lines().last().expect("a line")).expect("JSON");
    assert_eq!(
        last["waiting_on_you"].as_array().map(Vec::len),
        Some(3),
        "{last}"
    );
}
