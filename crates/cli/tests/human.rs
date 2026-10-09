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
use farik_protocol::command::{AcceptSubject, Command, RequestSize};
use farik_protocol::event::{EventBody, EventKind, HumanAcceptedBodySubject};
use farik_runtime::orchestrator::CommandError;
use farik_runtime::recorded::fixtures::tool_runner;
use farik_runtime::sprints::{PlannedBy, plan_sprint};
use farik_runtime::{RecordedAdapter, RuntimeAdapter};
use serde_json::{Value, json};

use project::{
    LiveDriver, a_high_risk_task_verifying, a_project, a_team, events, filed, files_of,
    hold_the_run_lock, joined, moved, record, record_as, recorded, run, run_with, status_of,
    tool_deps,
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
fn sends_back_from_the_command_line() {
    let repository = a_project("human-send-back");
    let task = filed(&repository, "Add done.txt");
    let driver = LiveDriver::new(&repository);

    let ran = run(
        &repository.path,
        &[
            "send-back",
            &task,
            "The button is too small to tap.",
            "--criterion",
            "C1",
            "--criterion",
            "C2",
        ],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("handled by the run"), "{}", ran.out);
    assert_eq!(
        driver.commands(),
        vec![Command::HumanSendBack {
            task_id: task.parse().expect("a task id"),
            subject: AcceptSubject::Result,
            message: "The button is too small to tap.".to_string(),
            failed_criteria: vec!["C1".to_string(), "C2".to_string()],
        }]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_the_refusal_the_driving_process_answered() {
    // A project per answer: dropping a driver does not free its run lock at once, because a child
    // another test thread is forking meanwhile holds a copy of the lock's descriptor until it
    // execs, so a second driver on the same project could find the lock still held.
    for (name, answer, said) in [
        (
            "human-routed-not-found",
            CommandError::NotFound {
                what: "FRK-9".to_string(),
            },
            "farik: FRK-9 is not in this project",
        ),
        (
            "human-routed-refused",
            CommandError::Refused {
                reason: "not_awaiting_approval: FRK-1 is a draft".to_string(),
            },
            "farik: not_awaiting_approval: FRK-1 is a draft",
        ),
    ] {
        let repository = a_project(name);
        let task = filed(&repository, "Add done.txt");
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

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn pauses_and_resumes_from_the_command_line() {
    let repository = a_team("human-pause");

    let paused = run(&repository.path, &["pause"]);
    assert_eq!(paused.code, 0, "{}", paused.err);
    assert!(paused.out.contains("paused the team"), "{}", paused.out);
    let resumed = run(&repository.path, &["resume"]);
    assert_eq!(resumed.code, 0, "{}", resumed.err);
    assert!(resumed.out.contains("resumed the team"), "{}", resumed.out);

    let again = run(&repository.path, &["resume"]);
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.contains("not_paused: the team is not paused"),
        "{}",
        again.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn run_on_a_paused_team_says_so_and_exits() {
    let repository = a_team("human-pause-run");
    let paused = run(&repository.path, &["pause"]);
    assert_eq!(paused.code, 0, "{}", paused.err);

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = Engine::Given(Arc::new(|daemon| {
            let adapter: Arc<dyn RuntimeAdapter> =
                Arc::new(RecordedAdapter::with_tools(Vec::new(), tool_runner(daemon)));
            adapter
        }));
    });

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.out
            .contains("idle: the team is paused; farik resume starts it again"),
        "{}",
        ran.out
    );
    assert!(events(&repository, &[EventKind::SessionStarted]).is_empty());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_an_empty_sprint_whose_planning_is_spent_waits_for_its_end() {
    let repository = a_team("human-sprint-empty");
    let started = run(&repository.path, &["sprint", "start"]);
    assert_eq!(started.code, 0, "{}", started.err);
    let empty = "empty: end it with farik sprint end";
    let shown = run(&repository.path, &["sprint", "show"]);
    assert!(!shown.out.contains(empty), "{}", shown.out);

    record_as(
        &repository,
        "",
        Some(("pm", "session-1")),
        "session.started",
        &json!({ "purpose": "plan", "model": "claude-opus-5", "effort": "high" }),
    );
    // A planning session is spent once it ends other than at a limit.
    record_as(
        &repository,
        "",
        Some(("pm", "session-1")),
        "session.ended",
        &json!({ "reason": "completed", "detail": "done" }),
    );
    let shown = run(&repository.path, &["sprint", "show"]);

    assert_eq!(shown.code, 0, "{}", shown.err);
    assert!(shown.out.contains(empty), "{}", shown.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn joins_a_task_the_human_files_under_a_sprints_epic() {
    let repository = a_team("human-sprint-child");
    let epic = an_epic_in_the_open_sprint(&repository);

    let ran = file_under(&repository, &epic);

    assert_eq!(ran.code, 0, "{}", ran.err);
    let contract = files_of(&repository)
        .read_contract(&"FRK-2".parse().expect("a task id"))
        .expect("the child is written");
    assert_eq!(contract.sprint.as_deref(), Some("S1"));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn files_a_task_whose_join_fails_and_says_so() {
    let repository = a_team("human-sprint-child-ending");
    let epic = an_epic_in_the_open_sprint(&repository);
    // The human's end has written S1's file and not yet recorded `sprint.ended`.
    let path = repository.path.join(".farik/sprints/S1.yaml");
    let text = std::fs::read_to_string(&path).expect("S1 reads");
    let ending = text.replace(
        "status: open",
        "status: ended\nended_at: 2026-09-24T01:00:00Z",
    );
    assert_ne!(ending, text, "{text}");
    std::fs::write(&path, ending).expect("S1 is written");

    let ran = file_under(&repository, &epic);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("FRK-2 filed"), "{}", ran.out);
    assert!(ran.out.contains("S1 has ended"), "{}", ran.out);
    let contract = files_of(&repository)
        .read_contract(&"FRK-2".parse().expect("a task id"))
        .expect("the child is written");
    assert_eq!(contract.sprint, None);
}

/// An epic, in progress with the Product Manager, planned into S1, which is open.
fn an_epic_in_the_open_sprint(repository: &farik_store::git::fixtures::TempRepo) -> String {
    let epic = filed(repository, "A whole board");
    let ran = run(
        &repository.path,
        &["triage", &epic, "large", "--reason", "Two parts."],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    moved(repository, &epic, "draft", "refining", &json!({}));
    moved(repository, &epic, "refining", "ready", &json!({}));
    moved(
        repository,
        &epic,
        "ready",
        "assigned",
        &json!({ "assignee": "pm" }),
    );
    moved(
        repository,
        &epic,
        "assigned",
        "in_progress",
        &json!({ "assignee": "pm" }),
    );
    let started = run(&repository.path, &["sprint", "start"]);
    assert_eq!(started.code, 0, "{}", started.err);
    plan_sprint(
        &tool_deps(repository),
        &[epic.parse().expect("a task id")],
        &PlannedBy::Governor,
    )
    .expect("the epic is planned");
    epic
}

/// `farik task create` of a request under `epic`.
fn file_under(repository: &farik_store::git::fixtures::TempRepo, epic: &str) -> project::Ran {
    let child = repository.path.join("child.yaml");
    std::fs::write(&child, project::a_request("One row of the board")).expect("written");
    run(
        &repository.path,
        &[
            "task",
            "create",
            child.to_str().expect("a path"),
            "--parent",
            epic,
        ],
    )
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_and_shows_the_channel() {
    let repository = a_team("human-say");

    let said = run(&repository.path, &["say", "hello @dev-a"]);
    assert_eq!(said.code, 0, "{}", said.err);
    assert!(said.out.contains("posted in the channel"), "{}", said.out);

    let shown = run(&repository.path, &["channel"]);
    assert_eq!(shown.code, 0, "{}", shown.err);
    assert!(
        shown
            .out
            .lines()
            .any(|line| line.contains("human") && line.contains("hello @dev-a")),
        "{}",
        shown.out
    );
    let listed = run(&repository.path, &["--json", "channel"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    let first: Value = serde_json::from_str(listed.out.lines().next().expect("a line"))
        .expect("one JSON object per line");
    assert_eq!(first["kind"], "human");
    assert_eq!(first["author"], "human");
    assert_eq!(first["mentions"], json!(["dev-a"]));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn shows_the_last_messages_of_the_channel() {
    let repository = a_team("human-channel-last");
    for text in ["one", "two", "three"] {
        let said = run(&repository.path, &["say", text]);
        assert_eq!(said.code, 0, "{}", said.err);
    }

    let shown = run(&repository.path, &["channel", "--last", "2"]);

    assert_eq!(shown.code, 0, "{}", shown.err);
    let texts: Vec<&str> = shown
        .out
        .lines()
        .filter_map(|line| line.rsplit(' ').next())
        .collect();
    assert_eq!(texts, ["two", "three"], "{}", shown.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn chats_from_the_command_line() {
    let repository = a_team("human-chat");

    let sent = run(
        &repository.path,
        &[
            "chat",
            "dev-a",
            "Could",
            "customers",
            "pay",
            "with",
            "Apple",
            "Pay?",
        ],
    );
    assert_eq!(sent.code, 0, "{}", sent.err);
    let chats = events(&repository, &[EventKind::ChatMessagePosted]);
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));
    let EventBody::ChatMessagePosted(body) = &chats[0].body else {
        panic!("a chat message");
    };
    assert_eq!(
        (body.chat.as_str(), body.author.as_str(), body.text.as_str()),
        ("dev-a", "human", "Could customers pay with Apple Pay?")
    );
    record_as(
        &repository,
        "",
        Some(("dev-a", "session-1")),
        "chat_message.posted",
        &json!({
            "chat": "dev-a",
            "author": "dev-a",
            "text": "Not yet.\nShall I \u{1b}[31mpropose it?",
            "in_reply_to": chats[0].envelope.seq,
            "request": { "title": "Let customers pay with Apple Pay", "text": "Add Apple Pay at checkout, beside the card form." }
        }),
    );

    let shown = run(&repository.path, &["chat", "dev-a"]);

    assert_eq!(shown.code, 0, "{}", shown.err);
    let asked = shown
        .out
        .find("Could customers pay with Apple Pay?")
        .expect("the user's message is shown");
    let answered = shown
        .out
        .find("Not yet.\n")
        .expect("the reply is shown, lines kept");
    assert!(asked < answered, "oldest first: {}", shown.out);
    assert!(
        shown.out.contains("Let customers pay with Apple Pay"),
        "{}",
        shown.out
    );
    assert!(!shown.out.contains('\u{1b}'), "{:?}", shown.out);
    // The chat is not the channel's.
    let channel = run(&repository.path, &["channel"]);
    assert_eq!(channel.code, 0, "{}", channel.err);
    assert!(!channel.out.contains("Apple Pay"), "{}", channel.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_tool_approve_sends_the_command() {
    let repository = a_project("human-tool-approve-sent");
    let driver = LiveDriver::answering(
        &repository,
        Ok(farik_runtime::orchestrator::CommandReport {
            said: "Allowed create_issue once for theo (approval 12).".to_string(),
            events: Vec::new(),
        }),
    );

    let ran = run(&repository.path, &["tool", "approve", "12"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.trim(),
        "Allowed create_issue once for theo (approval 12)."
    );
    assert_eq!(
        driver.commands(),
        vec![Command::ToolApprove {
            approval: 12,
            note: None
        }]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_tool_refuse_carries_the_note() {
    let repository = a_project("human-tool-refuse-sent");
    let driver = LiveDriver::answering(
        &repository,
        Ok(farik_runtime::orchestrator::CommandReport {
            said: "Not allowed: create_issue for theo (approval 12).".to_string(),
            events: Vec::new(),
        }),
    );

    let ran = run(
        &repository.path,
        &["tool", "refuse", "12", "--note", "not this repo"],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.trim(),
        "Not allowed: create_issue for theo (approval 12)."
    );
    assert_eq!(
        driver.commands(),
        vec![Command::ToolRefuse {
            approval: 12,
            note: Some("not this repo".to_string())
        }]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_tool_approve_writes_here_when_nothing_drives() {
    let repository = a_project("human-tool-approve-here");
    let task = filed(&repository, "Add done.txt");
    let n = record_as(
        &repository,
        &task,
        Some(("theo", "session-1")),
        "tool_approval.requested",
        &json!({
            "server": "github",
            "tool": "create_issue",
            "input": "{\"title\":\"Broken link\"}",
            "input_sha256": "0".repeat(64)
        }),
    )
    .envelope
    .seq;

    let ran = run(&repository.path, &["tool", "approve", &n.to_string()]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.trim(),
        format!(
            "Allowed create_issue once for theo (approval {n}).\nInput: {{\"title\":\"Broken link\"}}"
        )
    );
    let granted = events(&repository, &[EventKind::ToolApprovalGranted]);
    assert_eq!(granted.len(), 1);
    let EventBody::ToolApprovalGranted(body) = &granted[0].body else {
        panic!("a grant");
    };
    assert_eq!((body.approval.get(), body.note.as_deref()), (n, None));
    assert_eq!(
        granted[0].envelope.ids.task_id.as_ref().map(|t| t.as_str()),
        Some(task.as_str())
    );

    let again = run(&repository.path, &["tool", "refuse", &n.to_string()]);
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.starts_with("farik: approval_decided"),
        "{}",
        again.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_tool_refuse_shows_the_input_escaped() {
    let repository = a_project("human-tool-refuse-input");
    let task = filed(&repository, "Add done.txt");
    let n = record_as(
        &repository,
        &task,
        Some(("theo", "session-1")),
        "tool_approval.requested",
        &json!({
            "server": "github",
            "tool": "create_issue",
            "input": "{\"title\":\"\u{1b}[2J\u{202e}gnp\"}",
            "input_sha256": "0".repeat(64)
        }),
    )
    .envelope
    .seq;

    let ran = run(&repository.path, &["tool", "refuse", &n.to_string()]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        !ran.out.contains('\u{1b}') && !ran.out.contains('\u{202e}'),
        "{:?}",
        ran.out
    );
    assert_eq!(
        ran.out.trim(),
        format!(
            "Not allowed: create_issue for theo (approval {n}).\nInput: {{\"title\":\"\\u001b[2J\\u202egnp\"}}"
        )
    );
}

/// What the driver answers to a marketing plan command.
fn a_plan_answer(said: &str) -> farik_runtime::orchestrator::CommandReport {
    farik_runtime::orchestrator::CommandReport {
        said: said.to_string(),
        events: Vec::new(),
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marketing_plan_approve_sends_the_decision() {
    let repository = a_project("human-plan-approve-sent");
    let driver = LiveDriver::answering(
        &repository,
        Ok(a_plan_answer("approved marketing plan MP-1")),
    );

    let noted = run(
        &repository.path,
        &[
            "marketing",
            "plan",
            "approve",
            "MP-1",
            "--note",
            "Start small",
        ],
    );
    let bare = run(&repository.path, &["marketing", "plan", "approve", "MP-2"]);

    assert_eq!(noted.code, 0, "{}", noted.err);
    assert_eq!(noted.out.trim(), "approved marketing plan MP-1");
    assert_eq!(bare.code, 0, "{}", bare.err);
    assert_eq!(
        driver.commands(),
        vec![
            Command::MarketingPlanDecide {
                plan: "MP-1".to_string(),
                approve: true,
                note: Some("Start small".to_string())
            },
            Command::MarketingPlanDecide {
                plan: "MP-2".to_string(),
                approve: true,
                note: None
            },
        ]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marketing_plan_end_sends_the_end() {
    let repository = a_project("human-plan-end-sent");
    let driver = LiveDriver::answering(&repository, Ok(a_plan_answer("ended marketing plan MP-1")));

    let ran = run(
        &repository.path,
        &[
            "marketing",
            "plan",
            "end",
            "MP-1",
            "--note",
            "Changed course.",
        ],
    );
    let bare = run(&repository.path, &["marketing", "plan", "end", "MP-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), "ended marketing plan MP-1");
    assert_eq!(bare.code, 0, "{}", bare.err);
    assert_eq!(
        driver.commands(),
        vec![
            Command::MarketingPlanEnd {
                plan: "MP-1".to_string(),
                note: Some("Changed course.".to_string())
            },
            Command::MarketingPlanEnd {
                plan: "MP-1".to_string(),
                note: None
            },
        ]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn ending_with_no_process_says_when_ads_pause() {
    // A plan whose ads Farik made: with no process driving the project nothing pauses them until
    // one does, and the command says so.
    let repository = a_project("human-plan-end-ads");
    let task = filed(&repository, "Add done.txt");
    a_plan_proposed(&repository, &task, "MP-1", "Spring launch", (-1, 10));
    record(
        &repository,
        &task,
        "marketing_plan.approved",
        &json!({ "plan": "MP-1", "note": "" }),
    );
    record(
        &repository,
        "",
        "marketing_campaign.created",
        &json!({
            "plan": "MP-1", "key": "search-launch", "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "total", "amount": "500.00"
        }),
    );

    let ran = run(&repository.path, &["marketing", "plan", "end", "MP-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.trim(),
        "ended marketing plan MP-1. Farik pauses MP-1's ads when it next runs."
    );

    // A plan with no ads says nothing of them.
    let repository = a_project("human-plan-end-no-ads");
    let task = filed(&repository, "Add done.txt");
    a_plan_proposed(&repository, &task, "MP-1", "Spring launch", (-1, 10));
    record(
        &repository,
        &task,
        "marketing_plan.approved",
        &json!({ "plan": "MP-1", "note": "" }),
    );
    let ran = run(&repository.path, &["marketing", "plan", "end", "MP-1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), "ended marketing plan MP-1");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn ending_with_no_process_counts_a_pause_made_for_a_removal() {
    // The owner removed Google Ads: Farik paused the plan's ads first and recorded it. Ending the
    // plan leaves nothing to pause, and the command says nothing of it, until Google Ads is
    // connected again.
    let repository = a_plan_whose_campaign_was_paused_for_a_removal("human-plan-end-removed");

    let ran = run(&repository.path, &["marketing", "plan", "end", "MP-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), "ended marketing plan MP-1");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn ending_with_no_process_does_not_count_a_removal_s_pause_while_an_agent_has_google_ads() {
    // Google Ads was removed from one agent and Farik paused the campaign for it, but another
    // agent still has it, and its sign-in could have enabled the campaign since, with no
    // connection recorded: ending the plan still leaves ads to pause.
    let repository = a_plan_whose_campaign_was_paused_for_a_removal("human-plan-end-held");
    let files = files_of(&repository);
    let mut wire =
        serde_json::to_value(files.read_team().expect("the team reads")).expect("the team is JSON");
    wire["agents"][1]["mcp_servers"] = json!([{
        "name": "google-ads", "source": "custom", "transport": "stdio", "command": "sh",
        "args": ["server"], "tools": { "search": "network" }
    }]);
    files
        .write_team(&farik_core::team::validate_team(&wire).expect("a team"))
        .expect("the team is written");

    let ran = run(&repository.path, &["marketing", "plan", "end", "MP-1"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.trim(),
        "ended marketing plan MP-1. Farik pauses MP-1's ads when it next runs."
    );
}

/// A project with MP-1 approved and one campaign made for it, which Farik paused because Google
/// Ads was removed.
fn a_plan_whose_campaign_was_paused_for_a_removal(
    name: &str,
) -> farik_store::git::fixtures::TempRepo {
    let repository = a_project(name);
    let task = filed(&repository, "Add done.txt");
    a_plan_proposed(&repository, &task, "MP-1", "Spring launch", (-1, 10));
    record(
        &repository,
        &task,
        "marketing_plan.approved",
        &json!({ "plan": "MP-1", "note": "" }),
    );
    record(
        &repository,
        "",
        "marketing_campaign.created",
        &json!({
            "plan": "MP-1", "key": "search-launch", "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "total", "amount": "500.00"
        }),
    );
    record(
        &repository,
        "",
        "marketing_campaign.paused",
        &json!({
            "plan": "MP-1", "key": "search-launch",
            "campaign": "customers/1234567890/campaigns/11", "why": "connection_removed"
        }),
    );
    repository
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marketing_plan_return_needs_a_reason() {
    let repository = a_project("human-plan-return-sent");
    let driver = LiveDriver::answering(
        &repository,
        Ok(a_plan_answer("sent back marketing plan MP-1")),
    );

    let without = run(&repository.path, &["marketing", "plan", "return", "MP-1"]);
    assert_eq!(without.code, 2, "{}", without.out);
    assert!(without.err.contains("--reason"), "{}", without.err);
    assert!(driver.commands().is_empty(), "nothing was sent");

    let with = run(
        &repository.path,
        &[
            "marketing",
            "plan",
            "return",
            "MP-1",
            "--reason",
            "Halve the budget.",
        ],
    );
    assert_eq!(with.code, 0, "{}", with.err);
    assert_eq!(with.out.trim(), "sent back marketing plan MP-1");
    assert_eq!(
        driver.commands(),
        vec![Command::MarketingPlanDecide {
            plan: "MP-1".to_string(),
            approve: false,
            note: Some("Halve the budget.".to_string())
        }]
    );
}

/// Kai's plan `plan` on `task`, proposed with `title`, between `from` and `to` days from today.
fn a_plan_proposed(
    repository: &farik_store::git::fixtures::TempRepo,
    task: &str,
    plan: &str,
    title: &str,
    (from, to): (i64, i64),
) {
    let today = project::at().date_naive();
    let day = |days: i64| (today + chrono::Duration::days(days)).to_string();
    let mut body = farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
    body["plan"] = json!(plan);
    body["title"] = json!(title);
    body["starts_on"] = json!(day(from));
    body["ends_on"] = json!(day(to));
    body["campaigns"] = json!([]);
    body["posts"] = json!([]);
    body["budget"] = json!({ "total": "2000.50", "google_ads": "0" });
    record_as(
        repository,
        task,
        Some(("kai", "session-1")),
        "marketing_plan.proposed",
        &body,
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marketing_plan_show_prints_the_list_and_one_plan() {
    let repository = a_project("human-plan-show");
    let task = filed(&repository, "Add done.txt");
    a_plan_proposed(&repository, &task, "MP-1", "Spring launch", (-1, 10));
    record(
        &repository,
        &task,
        "marketing_plan.approved",
        &json!({ "plan": "MP-1", "note": "Start small" }),
    );
    a_plan_proposed(
        &repository,
        &task,
        "MP-2",
        "Autumn push \u{1b}[2J",
        (20, 30),
    );
    a_plan_proposed(&repository, &task, "MP-3", "Winter sale", (40, 50));
    record(
        &repository,
        &task,
        "marketing_plan.returned",
        &json!({ "plan": "MP-3", "reason": "Too early." }),
    );

    let list = run(&repository.path, &["marketing", "plan", "show"]);

    assert_eq!(list.code, 0, "{}", list.err);
    let lines: Vec<&str> = list.out.lines().collect();
    assert_eq!(lines.len(), 3, "{}", list.out);
    for (line, plan, state) in [
        (lines[0], "MP-3", "returned"),
        (lines[1], "MP-2", "proposed"),
        (lines[2], "MP-1", "active"),
    ] {
        assert!(line.starts_with(plan), "newest first: {line}");
        assert!(line.contains(state), "{line}");
        assert!(line.contains("USD 2000.50"), "{line}");
    }
    assert!(lines[2].contains("Spring launch"), "{}", lines[2]);
    assert!(!list.out.contains('\u{1b}'), "{:?}", list.out);

    let json = run(&repository.path, &["--json", "marketing", "plan", "show"]);
    assert_eq!(json.code, 0, "{}", json.err);
    assert!(json.err.is_empty(), "stdout alone: {}", json.err);
    let all: Value = serde_json::from_str(json.out.trim()).expect("one JSON document");
    let plans = all["plans"].as_array().expect("a list of plans");
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan["plan"].as_str().expect("an id"))
            .collect::<Vec<_>>(),
        ["MP-3", "MP-2", "MP-1"]
    );
    assert_eq!(plans[2]["state"], "active");
    assert_eq!(plans[2]["total"], "2000.50");
    assert_eq!(plans[2]["currency"], "USD");
    assert_eq!(plans[2]["agent_id"], "kai");
    assert_eq!(plans[2]["task_id"], task);

    let one = run(&repository.path, &["marketing", "plan", "show", "MP-1"]);
    assert_eq!(one.code, 0, "{}", one.err);
    for needle in [
        "MP-1 active",
        "Spring launch",
        "kai",
        "USD 2000.50",
        "approved ",
        ": Start small",
        "Two weeks of posts and one small search campaign.",
    ] {
        assert!(one.out.contains(needle), "{needle}: {}", one.out);
    }
    let one_json = run(
        &repository.path,
        &["--json", "marketing", "plan", "show", "MP-3"],
    );
    let plan: Value = serde_json::from_str(one_json.out.trim()).expect("one JSON document");
    assert_eq!(plan["plan"], "MP-3");
    assert_eq!(plan["state"], "returned");
    assert_eq!(plan["decided"]["decision"], "returned");
    assert_eq!(plan["decided"]["reason"], "Too early.");
    assert_eq!(plan["budget"]["total"], "2000.50");

    let unknown = run(&repository.path, &["marketing", "plan", "show", "MP-9"]);
    assert_eq!(unknown.code, 1, "{}", unknown.out);
    assert!(unknown.err.contains("MP-9"), "{}", unknown.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marketing_plan_show_prices_a_campaign_already_made_by_its_budget() {
    let repository = a_project("human-plan-show-price");
    let task = filed(&repository, "Add done.txt");
    let today = project::at().date_naive();
    let day = |days: i64| (today + chrono::Duration::days(days)).to_string();
    let mut body = farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
    body["plan"] = json!("MP-1");
    body["starts_on"] = json!(day(-1));
    body["ends_on"] = json!(day(60));
    body["posts"] = json!([]);
    body["budget"] = json!({ "total": "2000.50", "google_ads": "1000.00" });
    body["google_ads_account"] = json!("123-456-7890");
    body["campaigns"] = json!([{
        "key": "search-launch", "channel": "google_ads", "name": "Launch", "goal": "Sales",
        "advertises": "Handmade candles", "budget": "300.00",
        "starts_on": day(3), "ends_on": day(30)
    }]);
    record_as(
        &repository,
        &task,
        Some(("kai", "session-1")),
        "marketing_plan.proposed",
        &body,
    );
    record(
        &repository,
        &task,
        "marketing_plan.approved",
        &json!({ "plan": "MP-1", "note": "" }),
    );
    let price = |repository: &farik_store::git::fixtures::TempRepo| {
        let shown = run(
            &repository.path,
            &["--json", "marketing", "plan", "show", "MP-1"],
        );
        assert_eq!(shown.code, 0, "{}", shown.err);
        let plan: Value = serde_json::from_str(shown.out.trim()).expect("one JSON document");
        plan["campaigns"][0]["price"]
            .as_str()
            .unwrap_or("?")
            .to_string()
    };

    // A run of 28 days is a total budget when the campaign is made.
    assert_eq!(price(&repository), "fixed");

    // Made as a daily one, it keeps that budget, and the plan says so.
    record(
        &repository,
        "",
        "marketing_campaign.created",
        &json!({
            "plan": "MP-1", "key": "search-launch", "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "daily", "amount": "10.00"
        }),
    );
    assert_eq!(price(&repository), "not_fixed");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn post_stop_sends_the_number() {
    let repository = a_project("human-post-stop-sent");
    let driver = LiveDriver::answering(&repository, Ok(a_plan_answer("stopped post 42")));

    let ran = run(&repository.path, &["marketing", "post", "stop", "42"]);
    let bad = run(&repository.path, &["marketing", "post", "stop", "soon"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), "stopped post 42");
    assert_eq!(bad.code, 2, "{}", bad.out);
    assert_eq!(
        driver.commands(),
        vec![Command::SocialPostStop { post: 42 }],
        "only the number that was one"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn post_send_and_decline_send_the_decision() {
    let repository = a_project("human-post-decide-sent");
    let driver = LiveDriver::answering(&repository, Ok(a_plan_answer("decided post 42")));

    let sent = run(&repository.path, &["marketing", "post", "send", "42"]);
    let bare = run(&repository.path, &["marketing", "post", "decline", "43"]);
    let noted = run(
        &repository.path,
        &[
            "marketing",
            "post",
            "decline",
            "44",
            "--note",
            "Not this week",
        ],
    );

    for ran in [&sent, &bare, &noted] {
        assert_eq!(ran.code, 0, "{}", ran.err);
        assert_eq!(ran.out.trim(), "decided post 42");
    }
    assert_eq!(
        driver.commands(),
        vec![
            Command::SocialPostDecide {
                post: 42,
                post_it: true,
                note: None
            },
            Command::SocialPostDecide {
                post: 43,
                post_it: false,
                note: None
            },
            Command::SocialPostDecide {
                post: 44,
                post_it: false,
                note: Some("Not this week".to_string())
            },
        ]
    );
}

/// Kai's post of `text` on `channel` in plan MP-1, going out `hours` from the tests' now.
fn a_post_scheduled(
    repository: &farik_store::git::fixtures::TempRepo,
    task: &str,
    (channel, text): (&str, &str),
    hours: i64,
) -> (u64, String) {
    let going_out = (project::at() + chrono::Duration::hours(hours))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let event = record_as(
        repository,
        task,
        Some(("kai", "session-1")),
        "social_post.scheduled",
        &json!({
            "channel": channel, "buffer_channel": "chan-1", "text": text, "media": [],
            "at": going_out, "approved_by": "plan", "plan": "MP-1", "slot": "post-1",
        }),
    );
    (event.envelope.seq, going_out)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn post_list_prints_what_goes_out() {
    let repository = a_project("human-post-list");
    let task = filed(&repository, "Add done.txt");
    let long = format!("{} and a tail that is cut", "Open on Wednesday ".repeat(3));
    let (second, second_at) =
        a_post_scheduled(&repository, &task, ("x", "Second.\nNot this line."), 6);
    let (first, first_at) =
        a_post_scheduled(&repository, &task, ("instagram", "Sale \u{1b}[2J now"), 3);
    let (cut, cut_at) = a_post_scheduled(&repository, &task, ("facebook", &long), 5);
    // Stopped, and so not going out.
    let (gone, _) = a_post_scheduled(&repository, &task, ("linkedin", "Stopped."), 4);
    record(
        &repository,
        &task,
        "social_post.stopped",
        &json!({ "post": gone, "by": "owner" }),
    );

    let list = run(&repository.path, &["marketing", "post", "list"]);

    assert_eq!(list.code, 0, "{}", list.err);
    let lines: Vec<&str> = list.out.lines().collect();
    assert_eq!(
        lines,
        [
            format!("{first} Instagram {first_at} scheduled: Sale \\u001b[2J now"),
            format!(
                "{cut} Facebook {cut_at} scheduled: {}",
                long.chars().take(60).collect::<String>()
            ),
            format!("{second} X {second_at} scheduled: Second."),
        ],
        "soonest first, the text's first line cut at 60 characters, nothing stopped"
    );
    assert!(!list.out.contains('\u{1b}'), "{:?}", list.out);

    let json = run(&repository.path, &["--json", "marketing", "post", "list"]);
    assert_eq!(json.code, 0, "{}", json.err);
    assert!(json.err.is_empty(), "stdout alone: {}", json.err);
    let all: Value = serde_json::from_str(json.out.trim()).expect("one JSON document");
    let posts = all["posts"].as_array().expect("a list of posts");
    assert_eq!(
        posts
            .iter()
            .map(|post| post["post"].as_u64().expect("a number"))
            .collect::<Vec<_>>(),
        [first, cut, second]
    );
    assert_eq!(posts[2]["channel"], "x");
    assert_eq!(posts[2]["at"], second_at);
    assert_eq!(posts[2]["state"], "scheduled");
    assert_eq!(posts[2]["approved_by"], "plan");

    let none = run(
        &a_project("human-post-list-none").path,
        &["marketing", "post", "list"],
    );
    assert_eq!(none.code, 0, "{}", none.err);
    assert_eq!(none.out.trim(), "no post is going out");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn post_stop_with_no_driver_records_it() {
    let repository = a_project("human-post-stop-here");
    let task = filed(&repository, "Add done.txt");
    let (post, _) = a_post_scheduled(&repository, &task, ("instagram", "Hello."), 5);

    let ran = run(
        &repository.path,
        &["marketing", "post", "stop", &post.to_string()],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    let stopped = events(&repository, &[EventKind::SocialPostStopped]);
    assert_eq!(stopped.len(), 1);
    let EventBody::SocialPostStopped(body) = &stopped[0].body else {
        panic!("a stop");
    };
    assert_eq!(body.post.get(), post);
    assert_eq!(
        body.by,
        farik_protocol::event::SocialPostStoppedBodyBy::Owner
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one project, from the waiting line to the list after every command"
)]
fn farik_site_lists_and_decides() {
    let repository = a_project("human-site");
    let task = filed(&repository, "Add done.txt");
    let ask = |host: &str| {
        record_as(
            &repository,
            &task,
            Some(("theo", "session-1")),
            "site.requested",
            &json!({ "host": host, "url": format!("https://{host}/boxes"), "why": "A maker." }),
        )
        .envelope
        .seq
    };
    let (shop, other, third) = (
        ask("shop.example"),
        ask("other.example"),
        ask("third.example"),
    );
    let farik = farik_roles::sites::farik_sites()[0].host.clone();

    // A process driving the project says what waits, with both commands.
    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let line = ran
        .out
        .lines()
        .find(|line| line.contains("asks to read shop.example"))
        .unwrap_or_else(|| panic!("a waiting line: {}", ran.out));
    assert!(line.starts_with(&format!("{task} waits: ")), "{line}");
    assert!(
        line.ends_with(&format!(
            "asks to read shop.example: farik site approve {shop}, or farik site decline {shop}"
        )),
        "{line}"
    );
    let json = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    let last: Value = serde_json::from_str(json.out.lines().last().expect("a line")).expect("JSON");
    assert_eq!(last["waiting_on_you"][0]["request"], shop, "{last}");

    // The list shows Farik's sites on, the owner's, and what waits.
    let listed = run(&repository.path, &["site", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    assert!(
        listed
            .out
            .lines()
            .any(|line| line.contains(&farik) && line.contains(" on")),
        "{}",
        listed.out
    );
    for waiting in [
        format!("{shop} shop.example"),
        format!("{other} other.example"),
    ] {
        assert!(listed.out.contains(&waiting), "{waiting}: {}", listed.out);
    }

    // Deciding, with and without a note, and adding and removing.
    let approved = run(
        &repository.path,
        &["site", "approve", &shop.to_string(), "--note", "ok"],
    );
    assert_eq!(approved.code, 0, "{}", approved.err);
    let declined = run(&repository.path, &["site", "decline", &other.to_string()]);
    assert_eq!(declined.code, 0, "{}", declined.err);
    let allowed = events(&repository, &[EventKind::SiteApproved]);
    let EventBody::SiteApproved(body) = &allowed[0].body else {
        panic!("an approval");
    };
    assert_eq!(
        (
            body.host.as_str(),
            body.request.map(std::num::NonZeroU64::get),
            body.note.as_ref().map(|note| note.as_str())
        ),
        ("shop.example", Some(shop), Some("ok"))
    );
    assert_eq!(events(&repository, &[EventKind::SiteDeclined]).len(), 1);
    let again = run(&repository.path, &["site", "approve", &shop.to_string()]);
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.starts_with("farik: site_request_decided"),
        "{}",
        again.err
    );
    let removed = run(&repository.path, &["site", "remove", &farik]);
    assert_eq!(removed.code, 0, "{}", removed.err);
    let off = events(&repository, &[EventKind::SiteRemoved]);
    let EventBody::SiteRemoved(body) = &off[0].body else {
        panic!("a removal");
    };
    assert_eq!(body.host.as_str(), farik);
    let added = run(
        &repository.path,
        &["site", "add", "https://www.shop2.example/x"],
    );
    assert_eq!(added.code, 0, "{}", added.err);
    let after = run(&repository.path, &["site", "list"]);
    assert!(
        after
            .out
            .lines()
            .any(|line| line.contains(&farik) && line.contains(" off")),
        "{}",
        after.out
    );
    assert!(after.out.contains("shop2.example"), "{}", after.out);
    assert!(
        after.out.contains(&format!("{third} third.example")),
        "{}",
        after.out
    );
    let machine = run(&repository.path, &["--json", "site", "list"]);
    assert_eq!(machine.code, 0, "{}", machine.err);
    let wire: Value = serde_json::from_str(machine.out.trim()).expect("JSON");
    assert_eq!(wire["waiting"][0]["request"], third, "{wire}");
    assert_eq!(wire["farik"][0]["on"], false, "{wire}");
}

/// Order `number` that `theo` drafted in his session on `task`: 59.98 USD from `seller`.
fn order_drafted(
    repository: &farik_store::git::fixtures::TempRepo,
    task: &str,
    number: u64,
    seller: &str,
) {
    record_as(
        repository,
        task,
        Some(("theo", "session-1")),
        "purchase_order.drafted",
        &json!({
            "order": number, "seller": seller, "seller_contact": "sales@acme.example",
            "lines": [
                { "item": "Baby car mirror", "quantity": 3, "unit": "piece",
                  "unit_price": "19.99", "line_total": "59.97" },
                { "item": "Mounting kit", "quantity": 1, "unit": "",
                  "unit_price": "0.01", "line_total": "0.01" }
            ],
            "currency": "USD", "period": "once", "total": "59.98",
            "delivery": "3 days", "terms": "Net 30", "url": "",
            "evaluation": "evaluations/mirrors.md",
            "why": "It is the cheapest seller that ships to us."
        }),
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one project, from the waiting line to the list after every command"
)]
fn farik_pipeline_lists_and_decides() {
    let repository = a_project("human-pipeline");
    let task = filed(&repository, "Add done.txt");
    let ask = |name: &str, cost: &str, sends: bool| {
        record_as(
            &repository,
            &task,
            Some(("theo", "session-1")),
            "data_pipeline.requested",
            &json!({
                "name": name,
                "what": "Reads a seller's page as text, even where prices need a browser.",
                "source_url": "https://www.firecrawl.dev/pricing",
                "why": "Two of the five sellers show their prices only in a full browser.",
                "cost": cost, "needs_account": true, "sends_project_data": sends
            }),
        )
        .envelope
        .seq
    };
    let first = ask("Firecrawl\u{1b}[31m", "paid", false);
    let second = ask("Shippo", "free", true);
    // The manager passes the first on in its decision session, with a reason; Farik the second.
    record_as(
        &repository,
        "",
        Some(("pm", "session-pm")),
        "session.started",
        &json!({ "purpose": "verify", "model": "claude-opus-5-5", "effort": "high",
                 "pipeline": first }),
    );
    record_as(
        &repository,
        "",
        Some(("pm", "session-pm")),
        "data_pipeline.escalated",
        &json!({ "pipeline": first, "reason": "A paid plan.\u{1b}[2J Your call." }),
    );
    record(
        &repository,
        "",
        "data_pipeline.escalated",
        &json!({ "pipeline": second, "reason": "The Product Manager did not decide" }),
    );
    // A request holds no task: the task ends, and the requests still wait for the owner.
    let ended = run(&repository.path, &["cancel", &task, "Not", "needed"]);
    assert_eq!(ended.code, 0, "{}", ended.err);

    // A process driving the project says what waits, with both commands.
    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let line = ran
        .out
        .lines()
        .find(|line| line.contains("asks for a data source: Shippo"))
        .unwrap_or_else(|| panic!("a waiting line: {}", ran.out));
    assert!(line.starts_with(&format!("{task} waits: ")), "{line}");
    assert!(
        line.ends_with(&format!(
            "asks for a data source: Shippo: farik pipeline approve {second}, or farik pipeline decline {second}"
        )),
        "{line}"
    );
    let json = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    let last: Value = serde_json::from_str(json.out.lines().last().expect("a line")).expect("JSON");
    let waiting: Vec<&Value> = last["waiting_on_you"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|item| item.get("pipeline").is_some())
        .collect();
    assert_eq!(
        waiting
            .iter()
            .map(|item| item["pipeline"].as_u64())
            .collect::<Vec<_>>(),
        [Some(first), Some(second)],
        "{last}"
    );

    // A request nobody decided waits on the manager and not on the owner. It is asked for after
    // the runs above, which would start the manager's session for it.
    let open = ask("Tavily", "free", false);

    // The list shows each request, the agent's and the manager's words with their control
    // characters escaped.
    let listed = run(&repository.path, &["pipeline", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    let row = listed
        .out
        .lines()
        .find(|line| line.contains("Firecrawl"))
        .unwrap_or_else(|| panic!("a line for Firecrawl: {}", listed.out));
    for part in [
        first.to_string().as_str(),
        "Firecrawl\\u001b[31m",
        "  firecrawl.dev  ",
        "  costs money  ",
        "needs an account",
        "sends no data",
        "A paid plan.\\u001b[2J Your call.",
    ] {
        assert!(row.contains(part), "{part}: {row}");
    }
    assert!(
        !row.contains("https://"),
        "the site, not the address: {row}"
    );
    let shippo = listed
        .out
        .lines()
        .find(|line| line.contains("Shippo"))
        .unwrap_or_else(|| panic!("a line for Shippo: {}", listed.out));
    assert!(shippo.contains("sends your data"), "{shippo}");
    assert!(shippo.contains("  free  "), "{shippo}");
    assert!(
        !shippo.contains("The Product Manager did not decide"),
        "Farik's passing on is no reason of the manager's: {shippo}"
    );
    assert!(
        !listed.out.contains('\u{1b}'),
        "no escape reaches the terminal"
    );
    assert!(!listed.out.contains("Tavily"), "{}", listed.out);
    let machine = run(&repository.path, &["--json", "pipeline", "list"]);
    assert_eq!(machine.code, 0, "{}", machine.err);
    let rows: Value = serde_json::from_str(machine.out.trim()).expect("one JSON array alone");
    let rows = rows.as_array().expect("an array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["kind"], "data_pipeline");
    assert_eq!(rows[0]["pipeline"], first);
    assert_eq!(rows[0]["cost"], "paid");
    assert_eq!(rows[0]["host"], "firecrawl.dev");
    assert_eq!(rows[1]["pipeline"], second);
    assert_eq!(rows[1]["sends_project_data"], true);
    assert!(rows[1].get("reason").is_none());
    assert!(
        rows[0]["request_text"]
            .as_str()
            .is_some_and(|text| text.starts_with("Set up Firecrawl")),
        "{}",
        rows[0]
    );

    // Deciding: with a note, and without, and once.
    let approved = run(
        &repository.path,
        &["pipeline", "approve", &first.to_string(), "--note", "Go"],
    );
    assert_eq!(approved.code, 0, "{}", approved.err);
    let declined = run(
        &repository.path,
        &["pipeline", "decline", &second.to_string()],
    );
    assert_eq!(declined.code, 0, "{}", declined.err);
    let approvals = events(&repository, &[EventKind::DataPipelineApproved]);
    let EventBody::DataPipelineApproved(body) = &approvals[0].body else {
        panic!("an approval");
    };
    assert_eq!(
        (
            body.pipeline.get(),
            body.by.to_string(),
            body.reason.to_string()
        ),
        (first, "human".to_string(), "Go".to_string())
    );
    assert_eq!(
        (
            &approvals[0].envelope.ids.agent_id,
            &approvals[0].envelope.ids.session_id
        ),
        (&None, &None),
        "the owner's"
    );
    let refusals = events(&repository, &[EventKind::DataPipelineDeclined]);
    let EventBody::DataPipelineDeclined(body) = &refusals[0].body else {
        panic!("a decline");
    };
    assert_eq!(
        (body.pipeline.get(), body.reason.to_string()),
        (second, String::new())
    );
    let created = events(&repository, &[EventKind::TaskCreated]);
    let EventBody::TaskCreated(filed) = &created.last().expect("the request").body else {
        panic!("a request");
    };
    assert_eq!(filed.created_by, "human", "filed in the owner's name");

    // The open one is the manager's, a decided one is decided, and a word is no number.
    let not_yet = run(
        &repository.path,
        &["pipeline", "approve", &open.to_string()],
    );
    assert_eq!(not_yet.code, 1, "{}", not_yet.out);
    assert!(
        not_yet.err.starts_with("farik: pipeline_not_escalated"),
        "{}",
        not_yet.err
    );
    let again = run(
        &repository.path,
        &["pipeline", "decline", &first.to_string()],
    );
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.starts_with("farik: pipeline_decided"),
        "{}",
        again.err
    );
    let junk = run(&repository.path, &["pipeline", "approve", "firecrawl"]);
    assert_eq!(junk.code, 2, "{}", junk.out);
    let after = run(&repository.path, &["pipeline", "list"]);
    assert_eq!(after.code, 0, "{}", after.err);
    assert_eq!(
        after.out.trim(),
        "no data pipeline request waits for you",
        "{}",
        after.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one project, from the waiting line to the list after every command"
)]
fn farik_order_lists_and_decides() {
    let repository = a_project("human-order");
    let task = filed(&repository, "Add done.txt");
    order_drafted(&repository, &task, 1, "Acme\u{1b}[31m");
    order_drafted(&repository, &task, 2, "Bolt");
    order_drafted(&repository, &task, 3, "Cog");
    // An order holds no task: the task ends, and the orders still wait for the owner.
    let ended = run(&repository.path, &["cancel", &task, "Not", "needed"]);
    assert_eq!(ended.code, 0, "{}", ended.err);

    // A process driving the project says what waits, with both commands.
    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let line = ran
        .out
        .lines()
        .find(|line| line.contains("set up an order from Bolt"))
        .unwrap_or_else(|| panic!("a waiting line: {}", ran.out));
    assert!(line.starts_with(&format!("{task} waits: ")), "{line}");
    assert!(
        line.ends_with(
            "set up an order from Bolt: 59.98 USD: farik order approve 2, or farik order reject 2"
        ),
        "{line}"
    );
    let json = run_with(&repository.path, &["--json", "run"], |io| {
        io.engine = recorded(Vec::new());
    });
    let last: Value = serde_json::from_str(json.out.lines().last().expect("a line")).expect("JSON");
    let orders: Vec<&Value> = last["waiting_on_you"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|item| item.get("order").is_some())
        .collect();
    assert_eq!(
        orders
            .iter()
            .map(|item| item["order"].as_u64())
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3)],
        "{last}"
    );

    // The list shows each order, the agent's words with their control characters escaped.
    let listed = run(&repository.path, &["order", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    let first = listed
        .out
        .lines()
        .find(|line| line.contains("PO-1"))
        .unwrap_or_else(|| panic!("a line for PO-1: {}", listed.out));
    for part in [
        "drafted",
        "Acme\\u001b[31m",
        "59.98 USD once",
        task.as_str(),
        "theo",
    ] {
        assert!(first.contains(part), "{part}: {first}");
    }
    assert!(
        !listed.out.contains('\u{1b}'),
        "no escape reaches the terminal"
    );
    let machine = run(&repository.path, &["--json", "order", "list"]);
    assert_eq!(machine.code, 0, "{}", machine.err);
    let all: Value = serde_json::from_str(machine.out.trim()).expect("one JSON object alone");
    assert_eq!(all["orders"].as_array().map(Vec::len), Some(3));
    assert_eq!(all["orders"][0]["order"], 1);
    assert_eq!(all["orders"][0]["state"], "drafted");
    assert_eq!(all["orders"][0]["total"], "59.98");

    // Deciding: with a note, by `PO-2`, and once.
    let approved = run(&repository.path, &["order", "approve", "1", "--note", "Go"]);
    assert_eq!(approved.code, 0, "{}", approved.err);
    let rejected = run(&repository.path, &["order", "reject", "PO-2"]);
    assert_eq!(rejected.code, 0, "{}", rejected.err);
    let approvals = events(&repository, &[EventKind::PurchaseOrderApproved]);
    let EventBody::PurchaseOrderApproved(body) = &approvals[0].body else {
        panic!("an approval");
    };
    assert_eq!(
        (body.order.get(), body.note.to_string()),
        (1, "Go".to_string())
    );
    assert_eq!(
        (
            &approvals[0].envelope.ids.agent_id,
            &approvals[0].envelope.ids.session_id
        ),
        (&None, &None),
        "the owner's"
    );
    let rejections = events(&repository, &[EventKind::PurchaseOrderRejected]);
    let EventBody::PurchaseOrderRejected(body) = &rejections[0].body else {
        panic!("a rejection");
    };
    assert_eq!(
        (body.order.get(), body.note.to_string()),
        (2, String::new())
    );
    let with_a_note = run(
        &repository.path,
        &["order", "reject", "3", "--note", "Too dear"],
    );
    assert_eq!(with_a_note.code, 0, "{}", with_a_note.err);
    let EventBody::PurchaseOrderRejected(body) =
        &events(&repository, &[EventKind::PurchaseOrderRejected])[1].body
    else {
        panic!("a second rejection");
    };
    assert_eq!(
        (body.order.get(), body.note.to_string()),
        (3, "Too dear".to_string())
    );
    let again = run(&repository.path, &["order", "approve", "1"]);
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.starts_with("farik: purchase_order_decided"),
        "{}",
        again.err
    );
    let junk = run(&repository.path, &["order", "approve", "PO-x"]);
    assert_eq!(junk.code, 1, "{}", junk.out);
    let after = run(&repository.path, &["order", "list"]);
    assert!(
        after
            .out
            .lines()
            .any(|line| line.contains("PO-1") && line.contains("approved")),
        "{}",
        after.out
    );
    assert!(
        after
            .out
            .lines()
            .any(|line| line.contains("PO-2") && line.contains("rejected")),
        "{}",
        after.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one order's life, from placing to closing, and what each command sent"
)]
fn farik_order_placed_and_received_send_what_was_paid() {
    let repository = a_project("human-order-steps");
    let task = filed(&repository, "Add done.txt");
    for number in [1, 2] {
        order_drafted(&repository, &task, number, &format!("Seller {number}"));
        record(
            &repository,
            &task,
            "purchase_order.approved",
            &json!({ "order": number, "note": "" }),
        );
    }
    let day = |days: i64| (project::at().date_naive() + chrono::Duration::days(days)).to_string();

    let placed = run(
        &repository.path,
        &[
            "order",
            "placed",
            "1",
            "--paid",
            "1450",
            "--currency",
            "EUR",
            "--on",
            "2026-10-08",
        ],
    );
    assert_eq!(placed.code, 0, "{}", placed.err);
    let events_of = |kind: EventKind| events(&repository, &[kind]);
    let EventBody::PurchaseOrderPlaced(body) = &events_of(EventKind::PurchaseOrderPlaced)[0].body
    else {
        panic!("a placing");
    };
    assert_eq!(body.placed_on.to_string(), "2026-10-08");
    assert_eq!(
        body.paid.as_ref().map(|paid| paid.as_str()),
        Some("1450.00")
    );
    assert_eq!(
        body.currency.as_ref().map(|code| code.as_str()),
        Some("EUR")
    );

    let received = run(
        &repository.path,
        &["order", "received", "1", "--renews-on", "2027-10-08"],
    );
    assert_eq!(received.code, 0, "{}", received.err);
    let EventBody::PurchaseOrderReceived(body) =
        &events_of(EventKind::PurchaseOrderReceived)[0].body
    else {
        panic!("a receipt");
    };
    assert!(body.paid.is_none(), "no amount was sent");
    assert_eq!(
        body.renews_on.map(|renews| renews.to_string()),
        Some("2027-10-08".to_string())
    );
    assert_eq!(
        body.received_on,
        project::at().date_naive(),
        "today when no day is given"
    );

    // An order received on a day the owner names, with what they paid and in which currency.
    order_drafted(&repository, &task, 4, "Seller 4");
    record(
        &repository,
        &task,
        "purchase_order.approved",
        &json!({ "order": 4, "note": "" }),
    );
    let placed = run(&repository.path, &["order", "placed", "4"]);
    assert_eq!(placed.code, 0, "{}", placed.err);
    let received = run(
        &repository.path,
        &[
            "order",
            "received",
            "PO-4",
            "--on",
            "2026-10-10",
            "--paid",
            "12.5",
            "--currency",
            "GBP",
        ],
    );
    assert_eq!(received.code, 0, "{}", received.err);
    let EventBody::PurchaseOrderReceived(body) =
        &events_of(EventKind::PurchaseOrderReceived)[1].body
    else {
        panic!("a second receipt");
    };
    assert_eq!(
        (
            body.order.get(),
            body.received_on.to_string(),
            body.paid.as_ref().map(|paid| paid.as_str()),
            body.currency.as_ref().map(|code| code.as_str()),
            body.renews_on
        ),
        (
            4,
            "2026-10-10".to_string(),
            Some("12.50"),
            Some("GBP"),
            None
        )
    );

    // A second order: placed, its status corrected, and closed.
    let placed = run(&repository.path, &["order", "placed", "2"]);
    assert_eq!(placed.code, 0, "{}", placed.err);
    let status = run(
        &repository.path,
        &[
            "order",
            "status",
            "2",
            "delayed",
            "--note",
            "Short of flour",
            "--expected-on",
            &day(12),
        ],
    );
    assert_eq!(status.code, 0, "{}", status.err);
    let EventBody::PurchaseOrderUpdated(body) = &events_of(EventKind::PurchaseOrderUpdated)[0].body
    else {
        panic!("a status");
    };
    assert_eq!(
        (body.order.get(), body.status.to_string()),
        (2, "delayed".to_string())
    );
    assert_eq!(body.note.to_string(), "Short of flour");
    assert_eq!(body.expected_on.map(|day| day.to_string()), Some(day(12)));
    let listed = run(&repository.path, &["order", "list"]);
    assert!(
        listed.out.lines().any(|line| line.contains("PO-2")
            && line.contains("delayed")
            && line.contains("Short of flour")),
        "{}",
        listed.out
    );
    let refused = run(&repository.path, &["order", "status", "2", "placed"]);
    assert_eq!(refused.code, 1, "{}", refused.out);
    assert!(
        refused
            .err
            .starts_with("farik: purchase_order_status_invalid"),
        "{}",
        refused.err
    );
    let closed = run(
        &repository.path,
        &["order", "close", "2", "--note", "It was lost."],
    );
    assert_eq!(closed.code, 0, "{}", closed.err);
    let EventBody::PurchaseOrderClosed(body) = &events_of(EventKind::PurchaseOrderClosed)[0].body
    else {
        panic!("a closing");
    };
    assert_eq!(
        (body.order.get(), body.note.to_string()),
        (2, "It was lost.".to_string())
    );
    // An order whose seller's day has passed is said to be overdue, as of today.
    order_drafted(&repository, &task, 3, "Seller 3");
    record(
        &repository,
        &task,
        "purchase_order.approved",
        &json!({ "order": 3, "note": "" }),
    );
    record(
        &repository,
        &task,
        "purchase_order.placed",
        &json!({ "order": 3, "placed_on": "2020-01-01" }),
    );
    record_as(
        &repository,
        &task,
        Some(("theo", "session-1")),
        "purchase_order.updated",
        &json!({ "order": 3, "status": "shipped", "note": "", "expected_on": "2020-02-01" }),
    );
    let late = run(&repository.path, &["order", "list"]);
    let late_line = |out: &str, order: &str| {
        out.lines()
            .find(|line| line.starts_with(order))
            .map_or_else(|| panic!("a line for {order}: {out}"), str::to_string)
    };
    assert!(
        late_line(&late.out, "PO-3").contains("  overdue"),
        "{}",
        late.out
    );
    assert!(
        !late_line(&late.out, "PO-1").contains("overdue"),
        "{}",
        late.out
    );
    let bad_day = run(
        &repository.path,
        &["order", "placed", "1", "--on", "tomorrow"],
    );
    assert_eq!(bad_day.code, 1, "{}", bad_day.out);
    assert!(bad_day.err.contains("--on"), "{}", bad_day.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_renewal_lists_and_dismisses() {
    let repository = a_project("human-renewal");
    let none = run(&repository.path, &["renewal", "list"]);
    assert_eq!(none.code, 0, "{}", none.err);
    assert!(none.out.contains("no renewal is coming up"), "{}", none.out);
    let flagged = |vendor: &str| {
        record(
            &repository,
            "",
            "renewal.flagged",
            &json!({ "vendor": vendor, "renews_on": "2026-11-30", "decide_by": "2026-10-31" }),
        )
        .envelope
        .seq
    };
    let (first, second) = (flagged("Vercel\u{1b}[2J"), flagged("Notion"));
    record(
        &repository,
        "",
        "renewal.checked",
        &json!({ "due": 2, "unreadable": 3 }),
    );

    let listed = run(&repository.path, &["renewal", "list"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    let line = listed
        .out
        .lines()
        .find(|line| line.contains(&first.to_string()) && line.contains("Vercel"))
        .unwrap_or_else(|| panic!("a line for the first: {}", listed.out));
    assert!(
        line.contains("2026-11-30") && line.contains("2026-10-31"),
        "{line}"
    );
    assert!(line.contains("\\u001b[2J"), "escaped: {line}");
    assert!(
        !listed.out.contains('\u{1b}'),
        "no escape reaches the terminal"
    );
    assert!(
        listed
            .out
            .contains("3 rows in the register have a renewal date Farik can't read"),
        "{}",
        listed.out
    );
    let machine = run(&repository.path, &["--json", "renewal", "list"]);
    let all: Value = serde_json::from_str(machine.out.trim()).expect("one JSON object alone");
    assert_eq!(all["unreadable"], 3);
    assert_eq!(all["open"].as_array().map(Vec::len), Some(2));

    let dismissed = run(
        &repository.path,
        &["renewal", "dismiss", &first.to_string()],
    );
    assert_eq!(dismissed.code, 0, "{}", dismissed.err);
    let events_now = events(&repository, &[EventKind::RenewalDismissed]);
    let EventBody::RenewalDismissed(body) = &events_now[0].body else {
        panic!("a dismissal");
    };
    assert_eq!(body.renewal.get(), first);
    let after = run(&repository.path, &["--json", "renewal", "list"]);
    let all: Value = serde_json::from_str(after.out.trim()).expect("JSON");
    assert_eq!(all["open"].as_array().map(Vec::len), Some(1));
    assert_eq!(all["open"][0]["renewal"], second);
    let again = run(
        &repository.path,
        &["renewal", "dismiss", &first.to_string()],
    );
    assert_eq!(again.code, 1, "{}", again.out);
    assert!(
        again.err.starts_with("farik: renewal_dismissed"),
        "{}",
        again.err
    );
}

/// Message `number` that `theo` drafted on `task` for `seller`, its draft kept as the tool keeps it.
fn message_drafted(
    repository: &farik_store::git::fixtures::TempRepo,
    task: &str,
    number: u64,
    body: &str,
    order: Option<u64>,
) {
    let mut drafted = json!({
        "message": number, "seller": "Pie Box Pros", "to": "sales@pieboxpros.test",
        "subject": "Quote for 500 printed pie boxes", "purpose": "quote_request",
        "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c"
    });
    if let Some(order) = order {
        drafted["purpose"] = json!("purchase_order");
        drafted["purchase_order"] = json!(order);
    }
    record_as(
        repository,
        task,
        Some(("theo", "session-1")),
        "seller_message.drafted",
        &drafted,
    );
    let out = repository.path.join(".farik/local/procurement/mail/out");
    std::fs::create_dir_all(&out).expect("the folder");
    std::fs::write(out.join(format!("{number}.txt")), body).expect("the draft");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one project, from the waiting message to the commands the driver was sent"
)]
fn farik_procurement_lists_sends_and_discards() {
    let repository = a_project("human-procurement");
    let task = filed(&repository, "Source pie boxes");
    message_drafted(
        &repository,
        &task,
        1,
        "Hello,\nWhat would 500 boxes cost?\u{1b}[31m",
        None,
    );
    message_drafted(&repository, &task, 2, "About your order.", Some(7));
    message_drafted(&repository, &task, 3, "Never mind.", None);
    record(
        &repository,
        &task,
        "seller_message.discarded",
        &json!({ "message": 3 }),
    );

    // The list shows each waiting message whole, the agent's words with their control characters
    // escaped.
    let listed = run(&repository.path, &["procurement", "messages"]);
    assert_eq!(listed.code, 0, "{}", listed.err);
    for part in [
        "Message 1 to Pie Box Pros <sales@pieboxpros.test>",
        "nothing was sent to this domain before",
        "Subject: Quote for 500 printed pie boxes",
        "What would 500 boxes cost?\\u001b[31m",
    ] {
        assert!(listed.out.contains(part), "{part}: {}", listed.out);
    }
    assert!(
        !listed.out.contains('\u{1b}'),
        "no escape reaches the terminal"
    );
    let machine = run(&repository.path, &["--json", "procurement", "messages"]);
    assert_eq!(machine.code, 0, "{}", machine.err);
    let wire: Value = serde_json::from_str(machine.out.trim()).expect("one JSON object alone");
    assert_eq!(wire["messages"].as_array().map(Vec::len), Some(3), "{wire}");
    assert!(
        !listed.out.contains("Never mind."),
        "a discarded message does not wait"
    );
    assert_eq!(wire["cap"], 50);

    // The mailbox is not connected, and the page says so.
    let shown = run(
        &repository.path,
        &["--json", "procurement", "mailbox", "show"],
    );
    assert_eq!(shown.code, 0, "{}", shown.err);
    let state: Value = serde_json::from_str(shown.out.trim()).expect("JSON");
    assert_eq!(state["connected"], false, "{state}");
    let said = run(&repository.path, &["procurement", "mailbox", "show"]);
    assert!(
        said.out.contains("No mailbox is connected."),
        "{}",
        said.out
    );

    // Sending prints the message whole, then sends it as drafted to the process driving.
    let driver = LiveDriver::new(&repository);
    let sent = run(&repository.path, &["procurement", "send", "1"]);
    assert_eq!(sent.code, 0, "{}", sent.err);
    assert!(
        sent.out.contains("What would 500 boxes cost?\\u001b[31m"),
        "{}",
        sent.out
    );
    assert!(sent.out.contains("handled by the run"), "{}", sent.out);
    assert_eq!(
        driver.commands(),
        [Command::SellerMessageSend {
            message: 1,
            subject: "Quote for 500 printed pie boxes".to_string(),
            body: "Hello,\nWhat would 500 boxes cost?\u{1b}[31m".to_string(),
        }]
    );
    let discarded = run(&repository.path, &["procurement", "discard", "1"]);
    assert_eq!(discarded.code, 0, "{}", discarded.err);
    assert_eq!(
        driver.commands()[1],
        Command::SellerMessageDiscard { message: 1 }
    );

    // An order's message is sent from the web app beside its order; an unknown one is not there.
    let order = run(&repository.path, &["procurement", "send", "2"]);
    assert_ne!(order.code, 0);
    assert!(order.err.contains("carries an order"), "{}", order.err);
    let unknown = run(&repository.path, &["procurement", "send", "9"]);
    assert_ne!(unknown.code, 0);
    assert!(
        unknown.err.contains("message 9 is not in this project"),
        "{}",
        unknown.err
    );
    let gone = run(&repository.path, &["procurement", "send", "3"]);
    assert_ne!(gone.code, 0);
    assert!(
        gone.err.contains("does not wait to be sent"),
        "{}",
        gone.err
    );
    assert_eq!(driver.commands().len(), 2, "nothing else was sent");

    // Reading the mailbox, like connecting it, is the web app's while another process drives.
    let checked = run(&repository.path, &["procurement", "check"]);
    assert_ne!(checked.code, 0);
    assert!(
        checked.err.contains("another farik process"),
        "{}",
        checked.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn mailbox_connect_sends_the_password_it_read() {
    let repository = a_project("human-mailbox-connect");
    let config = format!("{}-config", repository.path.display());
    let with_password = |args: &[&str]| with_password(&repository, args);

    // Microsoft is refused in the daemon's words, before the password is read.
    let microsoft = with_password(&[
        "procurement",
        "mailbox",
        "connect",
        "--address",
        "ivo@outlook.com",
        "--name",
        "Ivo",
    ]);
    assert_ne!(microsoft.code, 0);
    assert!(
        microsoft.err.contains("mailbox_provider_unsupported"),
        "{}",
        microsoft.err
    );
    let chosen = with_password(&[
        "procurement",
        "mailbox",
        "connect",
        "--address",
        "ivo@example.test",
        "--name",
        "Ivo",
        "--provider",
        "microsoft",
    ]);
    assert!(
        chosen.err.contains("mailbox_provider_unsupported"),
        "{}",
        chosen.err
    );

    // No password is no mailbox.
    let silent = run_with(
        &repository.path,
        &[
            "procurement",
            "mailbox",
            "connect",
            "--address",
            "ivo@example.test",
            "--name",
            "Ivo",
            "--imap",
            "127.0.0.1:1",
            "--smtp",
            "127.0.0.1:1",
        ],
        |io| {
            io.stdin = Box::new(std::io::Cursor::new(Vec::new()));
            io.env.insert("XDG_CONFIG_HOME".to_string(), config.clone());
        },
    );
    assert_ne!(silent.code, 0);
    assert!(silent.err.contains("no password"), "{}", silent.err);

    // Another provider needs its servers.
    let bare = with_password(&[
        "procurement",
        "mailbox",
        "connect",
        "--address",
        "ivo@example.test",
        "--name",
        "Ivo",
    ]);
    assert_ne!(bare.code, 0);
    assert!(bare.err.contains("--imap"), "{}", bare.err);

    // Servers that cannot be reached refuse in Farik's words, which never quote the password.
    let down = with_password(&[
        "procurement",
        "mailbox",
        "connect",
        "--address",
        "ivo@example.test",
        "--name",
        "Ivo",
        "--imap",
        "127.0.0.1:1",
        "--smtp",
        "127.0.0.1:1",
    ]);
    assert_ne!(down.code, 0);
    assert!(down.err.contains("mailbox_"), "{}", down.err);
    for text in [&down.out, &down.err, &microsoft.err] {
        assert!(!text.contains("swordfish"), "{text}");
    }
    let state = run(
        &repository.path,
        &["--json", "procurement", "mailbox", "show"],
    );
    let state: Value = serde_json::from_str(state.out.trim()).expect("JSON");
    assert_eq!(
        state["connected"], false,
        "a refused login connects nothing: {state}"
    );
}

/// `farik <args>` with the mailbox's password on standard input, in a state folder of its own and
/// with no keychain.
fn with_password(repository: &farik_store::git::fixtures::TempRepo, args: &[&str]) -> project::Ran {
    run_with(&repository.path, args, |io| {
        io.stdin = Box::new(std::io::Cursor::new(b"swordfish\n".to_vec()));
        io.connector_secrets =
            Arc::new(farik_runtime::connectors::MemoryConnectorSecrets::default());
        io.env.insert(
            "XDG_CONFIG_HOME".to_string(),
            format!("{}-config", repository.path.display()),
        );
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn mailbox_connect_is_the_web_app_s_while_another_process_drives() {
    let repository = a_project("human-mailbox-driven");
    // Another process driving the project changes the mailbox in the web app.
    let _driver = LiveDriver::new(&repository);
    let driven = with_password(
        &repository,
        &[
            "procurement",
            "mailbox",
            "connect",
            "--address",
            "ivo@example.test",
            "--name",
            "Ivo",
            "--imap",
            "127.0.0.1:1",
            "--smtp",
            "127.0.0.1:1",
        ],
    );
    assert_ne!(driven.code, 0);
    assert!(
        driven.err.contains("another farik process"),
        "{}",
        driven.err
    );
    assert!(!driven.err.contains("swordfish"), "{}", driven.err);
}
