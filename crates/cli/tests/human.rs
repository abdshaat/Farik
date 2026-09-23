//! The human's commands from a terminal (`docs/SPEC.md` sections 5.2, 5.7, and 8.2): handled in
//! this process when nothing drives the project, sent to the process driving it when something
//! does.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use farik_protocol::command::{Command, RequestSize};
use farik_protocol::event::{EventBody, EventKind};
use serde_json::{Value, json};

use project::{
    LiveDriver, a_project, a_team, events, filed, hold_the_run_lock, moved, record, record_as, run,
    status_of,
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
    record_as(
        &repository,
        &task,
        Some(("pm", "s-1")),
        "session.started",
        &json!({ "purpose": "refine", "model": "claude-opus-5", "effort": "high" }),
    );
    let driver = LiveDriver::new(&repository);
    for (args, command) in [
        (vec!["stop"], Command::RunStop),
        (
            vec!["stop", task.as_str()],
            Command::SessionStop {
                session_id: "s-1".to_string(),
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
    let ran = run(&repository.path, &["stop", "FRK-2"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("FRK-2 has no session running"),
        "{}",
        ran.err
    );
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
