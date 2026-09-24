//! The human's commands from a terminal (`docs/SPEC.md` sections 5.2, 5.7, and 8.2): handled in
//! this process when nothing drives the project, sent to the process driving it when something
//! does.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use farik::Engine;
use farik_protocol::command::{Command, RequestSize};
use farik_protocol::event::{EventBody, EventKind, HumanAcceptedBodySubject};
use farik_runtime::orchestrator::CommandError;
use farik_runtime::recorded::fixtures::tool_runner;
use farik_runtime::{RecordedAdapter, RuntimeAdapter};
use serde_json::{Value, json};

use project::{
    LiveDriver, a_high_risk_task_verifying, a_project, a_team, events, filed, hold_the_run_lock,
    joined, moved, record, record_as, run, run_with, status_of,
};

/// Records a question from `pm` on `task`, and answers its sequence number.
fn asked(repository: &farik_store::git::fixtures::TempRepo, task: &str) -> u64 {
    record(
        repository,
        task,
        "question.asked",
        &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
    )
    .envelope
    .seq
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn answers_a_question_with_no_driver() {
    let repository = a_project("human-answer");
    let task = filed(&repository, "Add done.txt");
    let n = asked(&repository, &task).to_string();

    let ran = run(&repository.path, &["answer", &n, "No,", "one", "line."]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.trim().is_empty());
    let answers = events(&repository, &[EventKind::QuestionAnswered]);
    assert_eq!(answers.len(), 1);
    let EventBody::QuestionAnswered(body) = &answers[0].body else {
        panic!("an answer");
    };
    assert_eq!(body.answer, "No, one line.");
    assert_eq!(body.answered_by, "human");

    let again = run(&repository.path, &["--json", "answer", &n, "Again"]);
    assert_eq!(again.code, 1, "{}", again.out);
    let error: Value = serde_json::from_str(again.err.trim()).expect("JSON on stderr");
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("already_answered")),
        "{error}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_with_the_words_of_handle() {
    let repository = a_project("human-refuses");
    let task = filed(&repository, "Add done.txt");

    let ran = run(&repository.path, &["approve", &task]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.starts_with("farik: not_awaiting_approval"),
        "{}",
        ran.err
    );

    let ran = run(&repository.path, &["answer", "999", "Yes"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("999"), "{}", ran.err);
    assert!(ran.err.contains("is not in this project"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn cancels_and_resolves_for_the_human() {
    let repository = a_team("human-cancel");
    let first = filed(&repository, "Add done.txt");
    let ran = run(
        &repository.path,
        &["cancel", &first, "Not", "needed", "any", "more"],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(status_of(&repository, &first), "cancelled");
    let last = events(&repository, &[EventKind::TaskTransitioned])
        .pop()
        .expect("the move");
    let EventBody::TaskTransitioned(body) = &last.body else {
        panic!("a move");
    };
    assert_eq!(body.reason.as_deref(), Some("Not needed any more"));

    let second = filed(&repository, "Add done.txt again");
    record(
        &repository,
        &second,
        "request.triaged",
        &json!({ "size": "small", "reason": "One file.", "triaged_by": "human" }),
    );
    moved(&repository, &second, "draft", "refining", &json!({}));
    moved(&repository, &second, "refining", "escalated", &json!({}));
    record(
        &repository,
        &second,
        "escalation.raised",
        &json!({ "reason": "explicit_request", "detail": "It is two tasks." }),
    );
    let ran = run(
        &repository.path,
        &["resolve", &second, "refining", "Split", "it", "by", "page."],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let resolved = events(&repository, &[EventKind::EscalationResolved]);
    assert_eq!(resolved.len(), 1);
    let EventBody::EscalationResolved(body) = &resolved[0].body else {
        panic!("a resolution");
    };
    assert_eq!(body.to.to_string(), "refining");
    assert_eq!(body.message, "Split it by page.");

    let ran = run(&repository.path, &["resolve", &second, "finished", "x"]);
    assert_eq!(ran.code, 2, "{}", ran.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn sends_a_command_to_the_driving_process() {
    let repository = a_project("human-sends");
    let task = filed(&repository, "Add done.txt");
    let n = asked(&repository, &task);
    let contract = repository
        .path
        .join(format!(".farik/contracts/{task}.yaml"));
    let before = std::fs::read_to_string(&contract).expect("the contract reads");
    let driver = LiveDriver::new(&repository);

    let ran = run(&repository.path, &["answer", &n.to_string(), "Yes."]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("handled by the run"), "{}", ran.out);
    let ran = run(
        &repository.path,
        &["triage", &task, "large", "--reason", "Big."],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("handled by the run"), "{}", ran.out);
    let ran = run(&repository.path, &["contract", "lock", &task]);
    assert_eq!(ran.code, 0, "{}", ran.err);

    let id: farik_core::contract::TaskId = task.parse().expect("a task id");
    assert_eq!(
        driver.commands(),
        vec![
            Command::QuestionAnswer {
                question_id: n,
                answer: "Yes.".to_string()
            },
            Command::RequestTriage {
                task_id: id.clone(),
                size: RequestSize::Large,
                reason: "Big.".to_string()
            },
            Command::ContractLock { task_id: id },
        ]
    );
    assert!(events(&repository, &[EventKind::QuestionAnswered]).is_empty());
    assert_eq!(
        std::fs::read_to_string(&contract).expect("the contract reads"),
        before
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_only_a_driving_process() {
    let repository = a_project("human-stops");
    let ran = run(&repository.path, &["stop"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("no farik process is driving this project"),
        "{}",
        ran.err
    );

    let task = filed(&repository, "Add done.txt");
    let started = json!({ "purpose": "refine", "model": "claude-opus-5", "effort": "high" });
    let ended = json!({ "reason": "completed", "detail": "done" });
    // FRK-1's first session ended and its second runs; FRK-2's one session ended.
    record_as(
        &repository,
        &task,
        Some(("pm", "s-1")),
        "session.started",
        &started,
    );
    record_as(
        &repository,
        &task,
        Some(("pm", "s-1")),
        "session.ended",
        &ended,
    );
    record_as(
        &repository,
        &task,
        Some(("pm", "s-2")),
        "session.started",
        &started,
    );
    let other = filed(&repository, "Add done.txt again");
    record_as(
        &repository,
        &other,
        Some(("pm", "s-3")),
        "session.started",
        &started,
    );
    record_as(
        &repository,
        &other,
        Some(("pm", "s-3")),
        "session.ended",
        &ended,
    );
    let driver = LiveDriver::new(&repository);
    for (args, command) in [
        (vec!["stop"], Command::RunStop),
        (
            vec!["stop", task.as_str()],
            Command::SessionStop {
                session_id: "s-2".to_string(),
            },
        ),
        (
            vec!["stop", "s-9"],
            Command::SessionStop {
                session_id: "s-9".to_string(),
            },
        ),
    ] {
        let ran = run(&repository.path, &args);
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
        assert_eq!(driver.commands().last(), Some(&command));
    }
    for (target, said) in [
        (other.as_str(), format!("{other} has no session running")),
        ("FRK-9", "FRK-9 has no session running".to_string()),
    ] {
        let ran = run(&repository.path, &["stop", target]);
        assert_eq!(ran.code, 1, "{}", ran.out);
        assert!(ran.err.contains(&said), "{}", ran.err);
    }
    assert_eq!(driver.commands().len(), 3);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_while_the_lock_holder_serves_no_daemon() {
    let repository = a_project("human-no-daemon");
    let task = filed(&repository, "Add done.txt");
    let _lock = hold_the_run_lock(&repository);

    let ran = run(&repository.path, &["approve", &task]);

    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("serves no daemon yet"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn accepts_a_result_with_the_humans_review() {
    let repository = a_team("human-accept");
    let task = a_high_risk_task_verifying(&repository, "Add done.txt");

    let ran = run(
        &repository.path,
        &["accept", &task, "--message", "I read it."],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    let accepted = events(&repository, &[EventKind::HumanAccepted]);
    assert_eq!(accepted.len(), 1);
    let EventBody::HumanAccepted(body) = &accepted[0].body else {
        panic!("an acceptance");
    };
    assert_eq!(body.subject, HumanAcceptedBodySubject::Result);
    assert_eq!(body.message.as_deref(), Some("I read it."));
    assert_eq!(body.accepted_by, "human");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_refusal_the_driving_process_answered() {
    let repository = a_project("human-routed-refusals");
    let task = filed(&repository, "Add done.txt");
    for (answer, said) in [
        (
            CommandError::NotFound {
                what: "FRK-9".to_string(),
            },
            "farik: FRK-9 is not in this project",
        ),
        (
            CommandError::Refused {
                reason: "not_awaiting_approval: FRK-1 is a draft".to_string(),
            },
            "farik: not_awaiting_approval: FRK-1 is a draft",
        ),
    ] {
        let driver = LiveDriver::answering(&repository, Err(answer));

        let ran = run(&repository.path, &["approve", &task]);

        assert_eq!(ran.code, 1, "{}", ran.out);
        assert_eq!(ran.err.trim(), said);
        assert_eq!(driver.commands().len(), 1);
    }
}

/// Replaces `task`'s contract with a named pipe, so that whatever reads it next waits until the
/// test writes it; answers the pipe's path and the contract's text.
fn a_contract_that_waits(
    repository: &farik_store::git::fixtures::TempRepo,
    task: &str,
) -> (PathBuf, String) {
    let path = repository
        .path
        .join(format!(".farik/contracts/{task}.yaml"));
    let text = std::fs::read_to_string(&path).expect("the contract reads");
    std::fs::remove_file(&path).expect("the contract is removed");
    let made = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    (path, text)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn holds_the_run_lock_while_it_handles_a_command_here() {
    for (name, args) in [
        (
            "human-lock-cancel",
            &["cancel", "FRK-1", "Not", "needed."][..],
        ),
        (
            "human-lock-triage",
            &["triage", "FRK-1", "small", "--reason", "One."][..],
        ),
    ] {
        let repository = a_project(name);
        let task = filed(&repository, "Add done.txt");
        // An open question keeps every rule off the task, so a run that did start would not
        // read its contract.
        asked(&repository, &task);
        let (pipe, text) = a_contract_that_waits(&repository, &task);
        let root = repository.path.clone();
        let command = std::thread::spawn(move || run(&root, args));
        // Opening the pipe to write waits until the command opens it to read its contract.
        let opening = std::thread::spawn(move || {
            std::fs::File::options()
                .write(true)
                .open(&pipe)
                .expect("the pipe opens")
        });
        let mut writer = joined(opening, "the command reading its contract");

        let root = repository.path.clone();
        let second = joined(
            std::thread::spawn(move || {
                run_with(&root, &["run"], |io| {
                    io.engine = Engine::Given(Arc::new(|daemon| {
                        let adapter: Arc<dyn RuntimeAdapter> =
                            Arc::new(RecordedAdapter::with_tools(Vec::new(), tool_runner(daemon)));
                        adapter
                    }));
                })
            }),
            "farik run",
        );
        writer
            .write_all(text.as_bytes())
            .expect("the contract is written");
        drop(writer);
        let ran = joined(command, "the command");

        assert_eq!(second.code, 1, "{args:?}: {}\n{}", second.out, second.err);
        assert!(
            second
                .err
                .contains("another farik process is driving this project"),
            "{args:?}: {}",
            second.err
        );
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn starts_and_shows_a_sprint_from_the_command_line() {
    let repository = a_team("human-sprint");

    let started = run(&repository.path, &["sprint", "start", "--budget", "20"]);
    assert_eq!(started.code, 0, "{}", started.err);
    let shown = run(&repository.path, &["sprint", "show"]);
    assert_eq!(shown.code, 0, "{}", shown.err);
    for said in ["S1", "open", "budget $20", "spent $0"] {
        assert!(shown.out.contains(said), "{said} in {}", shown.out);
    }

    let ended = run(&repository.path, &["sprint", "end"]);
    assert_eq!(ended.code, 0, "{}", ended.err);
    let shown = run(&repository.path, &["sprint", "show"]);
    assert_eq!(shown.code, 0, "{}", shown.err);
    assert!(
        shown.out.contains("S1") && shown.out.contains("ended"),
        "{}",
        shown.out
    );
}
