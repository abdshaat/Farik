//! `catervas run` and `catervas plan` (`docs/SPEC.md` sections 5.7, 8.2, and 8.3): the start, the loop,
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

use catervas::{Engine, Interrupts};
use catervas_core::pricing::Usage;
use catervas_protocol::clock::MovableClock;
use catervas_protocol::event::{EventBody, EventKind, SessionEndedBodyReason};
use catervas_runtime::RuntimeAdapter;
use catervas_runtime::recorded::fixtures::{
    UsageThenWaitAdapter, accept_frk_1, implement_finishes_frk_1, plan_assigns_frk_1,
    planning_ceremony_frk_1, refine_writes_task_frk_1, reply_to_a_mention, review_writes_note,
    standup,
};
use catervas_runtime::sleep::Sleeper;
use catervas_store::git::fixtures::TempRepo;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use catervas_core::team::fixtures::an_agent_wire;
use project::{
    LiveDriver, a_bare_env, a_claude_saying, a_high_risk_task_verifying, a_project, a_team,
    a_team_with, at, events, filed, hold_the_run_lock, joined, no_sandbox, record, record_as,
    recorded, run, run_with, scratch, status_of, the_run_lock_frees, walked,
};

/// An engine whose sessions are `adapter`'s.
fn given(adapter: &Arc<UsageThenWaitAdapter>) -> Engine {
    let adapter = Arc::clone(adapter);
    Engine::Given(Arc::new(move |_| {
        let adapter: Arc<dyn RuntimeAdapter> = adapter.clone();
        adapter
    }))
}

/// A sleeper that moves its clock to the time waited for and returns at once.
struct MovingSleeper(Arc<MovableClock>);

impl Sleeper for MovingSleeper {
    fn sleep_until(
        &self,
        until: DateTime<Utc>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.0.set(until);
        Box::pin(std::future::ready(()))
    }
}

fn daemon_file(repository: &TempRepo) -> PathBuf {
    repository.path.join(".catervas/local/daemon.json")
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
    err.contains("~/.git-credentials")
        && err.contains("a git hidden in a script")
        && err.contains(".catervas/local/daemon.json, whose token lets them act as you through catervas")
        // ADR 0030: the token also gets a connector's keys, as does its process's environment.
        // ADR 0034: a command that reads the token can also send `skill_save`, which counts as the
        // person's confirmation of a skill.
        // Step 10c: it can also send `purchase_order_place` and `purchase_order_receive`, which an
        // order's fold counts as the owner's own marking of it placed and received.
        && err.contains("approve, accept, answer, add skills, mark orders placed and received, and integrate")
        && err.contains("and get the keys you gave a connector")
        && err.contains("/proc/<pid>/environ")
}

fn lock_is_free(repository: &TempRepo) {
    the_run_lock_frees(repository);
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
    .catervas/prices.json to price it.";

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
        repository.path.join(".catervas/prices.json"),
        "{\"version\": 2}",
    )
    .expect("the override is written");
    let before = events(&repository, &[]).len();

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains(".catervas/prices.json"), "{}", ran.err);
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
                .contains("another catervas process is driving this project"),
            "{}",
            ran.err
        );
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn names_the_driving_process_a_second_driver_found() {
    let repository = a_project("run-second-named");
    let driver = LiveDriver::new(&repository);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains(&format!(
            "another catervas process is driving this project (pid {} in .catervas/local/daemon.json)",
            driver.pid()
        )),
        "{}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn passes_over_a_claude_that_is_not_executable() {
    use std::os::unix::fs::PermissionsExt;

    let repository = a_project("run-claude-not-executable");
    let directory = scratch("claude-not-executable");
    let claude = directory.join("claude");
    std::fs::write(&claude, "#!/bin/sh\necho '2.1.280 (Claude Code)'\n").expect("written");
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o644)).expect("set");

    let ran = run_with(&repository.path, &["run"], |io| {
        io.env
            .insert("PATH".to_string(), directory.display().to_string());
        io.env
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());
    });

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("there is no claude on PATH"),
        "{}",
        ran.err
    );
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
            &format!("{task} waits for you to integrate it: catervas integrate {task}").as_str()
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
    let ran = joined(running, "the run");

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
fn exits_130_after_one_ctrl_c_once_the_session_ends() {
    let repository = a_team("run-one-ctrl-c");
    a_small_request(&repository);
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
    adapter.complete();
    let ran = joined(running, "the run");

    assert_eq!(ran.code, 130, "{}\n{}", ran.out, ran.err);
    assert!(ran.out.lines().any(|line| line == "stopped"), "{}", ran.out);
    assert_eq!(adapter.aborts(), 0);
    assert_eq!(adapter.started().len(), 1);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn exits_1_when_a_tick_fails() {
    let repository = a_team("run-tick-fails");
    a_small_request(&repository);

    // The refine session has no transcript to play, so it cannot start, which fails the tick.
    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.err.lines().any(|line| line.starts_with("catervas: ")),
        "{}",
        ran.err
    );
    assert!(!daemon_file(&repository).exists());
    lock_is_free(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_a_plan_through_catervas_stop() {
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
    let ran = joined(planning, "the plan");

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(ran.out.lines().any(|line| line == "stopped"), "{}", ran.out);
    assert_eq!(adapter.started().len(), 1);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_wait() {
    let repository = a_team("run-wait");
    let task = a_small_request(&repository);
    walked(&repository, &task, &["refining", "ready"]);
    let until = at() + chrono::Duration::hours(1);
    record_as(
        &repository,
        "",
        Some(("dev-a", "s-0")),
        "agent.slept",
        &json!({ "until": until.to_rfc3339(), "detail": "Claude AI usage limit reached" }),
    );
    let clock = Arc::new(MovableClock::new(at()));
    let path = repository.path.clone();
    // On a thread of its own, left behind if the run does not end in time: a run whose sleeper
    // is lost waits a real hour.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run_with(&path, &["run"], |io| {
            io.clock = clock.clone();
            io.sleeper = Some(Arc::new(MovingSleeper(Arc::clone(&clock))));
            io.engine = recorded(vec![
                plan_assigns_frk_1(),
                implement_finishes_frk_1(),
                review_writes_note(),
                accept_frk_1(),
            ]);
        }));
    });

    let ran = receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("the run ends within a minute");

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    let waiting = lines
        .iter()
        .position(|line| line.starts_with("waiting for dev-a, asleep until "))
        .unwrap_or_else(|| panic!("no wait in {}", ran.out));
    assert!(
        lines[waiting..]
            .iter()
            .any(|line| line.starts_with(&format!("{task}: "))),
        "{}",
        ran.out
    );
    // The wait for dev-a is capped at a minute so the board is rechecked (docs/SPEC.md 8.2),
    // which ticks `until - at()` (an hour) worth of minutes before dev-a wakes; the waiting line
    // still prints once, not once per recheck.
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("waiting for dev-a, asleep until "))
            .count(),
        1,
        "{}",
        ran.out
    );
    assert_eq!(
        purposes(&repository),
        ["plan", "implement", "verify", "verify"]
    );
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
            .join(format!(".catervas/local/worktrees/{task}"))
            .exists()
    );
    assert_eq!(purposes(&repository), ["plan"]);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_a_sprint_line_for_a_planning_session() {
    let repository = a_team("plan-sprint");
    let task = a_small_request(&repository);
    walked(&repository, &task, &["refining", "ready"]);
    let started = run(&repository.path, &["sprint", "start"]);
    assert_eq!(started.code, 0, "{}", started.err);

    let ran = run_with(&repository.path, &["plan"], |io| {
        io.engine = recorded(vec![planning_ceremony_frk_1(), plan_assigns_frk_1()]);
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.out
            .lines()
            .any(|line| line.starts_with("S1: ") && line.contains("S1 holds FRK-1")),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_a_conversation_line() {
    let repository = a_team("run-conversation");
    let said = run(&repository.path, &["say", "@dev-a status?"]);
    assert_eq!(said.code, 0, "{}", said.err);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(vec![reply_to_a_mention()]);
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.out
            .lines()
            .any(|line| line.starts_with("dev-a: ") && line.contains("conversation")),
        "{}",
        ran.out
    );
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
        format!("  catervas answer {n} <your answer>"),
        format!("{second} awaits your approval: catervas approve {second}"),
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

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_a_connector_calls_whole_input_escaped() {
    let repository = a_team("run-waiting-input");
    let task = a_small_request(&repository);
    // A call's input holding a raw escape sequence and a right-to-left override, which a
    // terminal would obey or reorder, ahead of text the human must still be able to read.
    let input = "{\"body\":\"\u{1b}[2Jpay \u{202e}100\u{200b}0\",\"to\":\"a@example.com\"}";
    let n = record_as(
        &repository,
        &task,
        Some(("theo", "session-1")),
        "tool_approval.requested",
        &json!({
            "server": "mail", "tool": "send", "input": input, "input_sha256": "0".repeat(64)
        }),
    )
    .envelope
    .seq;

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        !ran.out.contains('\u{1b}') && !ran.out.contains('\u{202e}'),
        "{:?}",
        ran.out
    );
    let expected =
        "  input: {\"body\":\"\\u001b[2Jpay \\u202e100\\u200b0\",\"to\":\"a@example.com\"}";
    assert!(ran.out.lines().any(|line| line == expected), "{}", ran.out);
    assert!(
        ran.out.contains(&format!(
            "catervas tool approve {n}, or catervas tool refuse {n}"
        )),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_a_marketing_plan_that_waits_with_its_commands() {
    let repository = a_team("run-waiting-plan");
    let task = a_small_request(&repository);
    let mut body = catervas_protocol::event::fixtures::a_body_wire(
        catervas_protocol::event::EventKind::MarketingPlanProposed,
    );
    body["title"] = json!("Spring launch");
    record_as(
        &repository,
        &task,
        Some(("kai", "session-1")),
        "marketing_plan.proposed",
        &body,
    );

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let line = format!(
        "{task} waits: kai proposes a marketing plan: Spring launch: catervas marketing plan approve \
         MP-1, or catervas marketing plan return MP-1 --reason <text>"
    );
    assert!(ran.out.lines().any(|found| found == line), "{}", ran.out);

    let ran = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    let last: Value = serde_json::from_str(ran.out.lines().last().expect("a line")).expect("JSON");
    assert_eq!(last["waiting_on_you"][0]["plan"], "MP-1", "{last}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_the_backlog_waits() {
    let repository = a_team_with("run-backlog", |wire| {
        wire["policy"]["plan_in_sprints"] = json!(true);
    });
    let task = a_small_request(&repository);
    walked(&repository, &task, &["refining", "ready"]);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.out
            .lines()
            .any(|line| line == "idle: the ready work waits for a sprint"),
        "{}",
        ran.out
    );
    assert_eq!(
        ran.out.lines().last(),
        Some("start a sprint: 1 waits in the Backlog (`catervas sprint start`)"),
        "{}",
        ran.out
    );
    assert_eq!(status_of(&repository, &task), "ready");

    let ran = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    let last: Value = serde_json::from_str(ran.out.lines().last().expect("a line")).expect("JSON");
    assert_eq!(last, json!({ "backlog": { "count": 1 } }));

    // With a sprint open, its Backlog waits for the next sprint, not for the human to start one.
    // Its planning session posts and plans nothing, so the request stays in the Backlog.
    let started = run_with(&repository.path, &["sprint", "start"], |_| {});
    assert_eq!(started.code, 0, "{}", started.err);
    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(vec![standup()]);
    });
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.out
            .lines()
            .any(|line| line == "idle: nothing on the board needs doing"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("start a sprint"), "{}", ran.out);
    let ran = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}", ran.err);
    let last: Value = serde_json::from_str(ran.out.lines().last().expect("a line")).expect("JSON");
    assert_eq!(last, json!({ "waiting_on_you": [] }));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_a_high_risk_result_and_no_answered_question() {
    let repository = a_team("plan-waiting");
    let task = a_high_risk_task_verifying(&repository, "Add done.txt");
    // A result waits on the human once its reviewer has passed it.
    record(
        &repository,
        &task,
        "review.recorded",
        &json!({ "reviewer": "dev-b", "criteria_run": 1, "passed": true }),
    );
    let n = record(
        &repository,
        &task,
        "question.asked",
        &json!({ "question": "Should done.txt be empty?", "asked_by": "dev-a" }),
    )
    .envelope
    .seq;
    record(
        &repository,
        &task,
        "question.answered",
        &json!({ "question_id": n, "answer": "Yes.", "answered_by": "human" }),
    );

    let ran = run_with(&repository.path, &["plan"], |io| {
        io.engine = recorded(Vec::new());
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let after: Vec<&str> = ran
        .out
        .lines()
        .skip_while(|line| !line.starts_with("idle:"))
        .skip(1)
        .collect();
    assert_eq!(
        after,
        [format!(
            "{task} may need your acceptance: catervas accept {task} --message <your review>"
        )],
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn gives_a_session_the_connectors_kept_where_this_computer_keeps_them() {
    use catervas_core::team::{custom_server, spec_sha256, validate_team};
    use catervas_runtime::claude::Secret;
    use catervas_runtime::connectors::{
        ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
    };
    use catervas_runtime::recorded::fixtures::tool_runner;
    use catervas_runtime::{RecordedAdapter, SessionSpec};

    let mut team = Value::Null;
    let repository = a_team_with("run-custom-connector", |wire| {
        wire["agents"][0]["mcp_servers"] = json!([{
            "name": "github", "source": "custom", "transport": "stdio",
            "command": "github-mcp", "args": [], "credential_keys": [],
            "tools": { "search_issues": "network" }
        }]);
        team = wire.clone();
    });
    let task = a_small_request(&repository);
    let server = validate_team(&team).expect("a team").agents[0]
        .mcp_servers
        .iter()
        .flatten()
        .find_map(custom_server)
        .expect("a custom server");
    // `XDG_CONFIG_HOME`, outside the repository: its `catervas` folder keeps the project's id.
    let config = PathBuf::from(format!("{}-config", repository.path.display()));
    let store = Arc::new(MemoryConnectorSecrets::default());
    store
        .save(
            &SecretAt::of(&config.join("catervas"), &repository.path, "pm", "github")
                .expect("an address"),
            &ConnectorEntry {
                spec_sha256: spec_sha256(&server),
                keys: std::collections::BTreeMap::<String, Secret>::new(),
                oauth: None,
            },
        )
        .expect("kept");
    let started: Arc<std::sync::Mutex<Option<Arc<RecordedAdapter>>>> = Arc::default();
    let kept = Arc::clone(&started);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.connector_secrets = store;
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        io.engine = Engine::Given(Arc::new(move |daemon| {
            let adapter = Arc::new(RecordedAdapter::with_tools(
                vec![refine_writes_task_frk_1()],
                tool_runner(daemon),
            ));
            *kept.lock().expect("not poisoned") = Some(Arc::clone(&adapter));
            let adapter: Arc<dyn RuntimeAdapter> = adapter;
            adapter
        }));
    });

    let adapter = started
        .lock()
        .expect("not poisoned")
        .clone()
        .expect("an engine");
    let specs: Vec<SessionSpec> = adapter.started();
    assert_eq!(
        specs[0].purpose,
        catervas_runtime::SessionPurpose::Refine,
        "{}\n{}",
        ran.out,
        ran.err
    );
    let servers: Vec<&str> = specs[0]
        .mcp_servers
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(servers, ["github"], "{task}");
}
