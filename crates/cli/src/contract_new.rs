//! `farik contract new` (`docs/SPEC.md` 5.13): a request filed from a brief or an issue, its
//! contract written with the Product Manager, and its questions asked at the terminal.

use std::io::{BufRead, BufReader, Read, Write};

use farik_core::contract::{TaskId, TaskStatus};
use farik_core::governor::team_rules::TeamRules;
use farik_protocol::command::{Command, RequestSize};
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::forge::{Forge, Issue};
use farik_runtime::orchestrator::{TickRules, TickScope};
use farik_store::EventQuery;
use farik_store::requests::{RequestError, file_request};
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::human::{refusal, said};
use crate::project::Project;
use crate::run::{
    Ended, INTERRUPTED, Printer, finish_quietly, print_waiting, refuse, report_error, ticks,
    waiting_now,
};
use crate::show::{body_lines, event_line};
use crate::start::{Driver, on_path, runtime, send, start_holding, try_lock};
use crate::waiting::Waiting;
use crate::{CliIo, HUMAN};

/// Every text of a request the Product Manager replaces while refining it.
pub(crate) const PLACEHOLDER: &str = "(placeholder, for the Product Manager to write)";
/// The budget a request is filed with when the team caps no task's budget (ADR 0015). The Product
/// Manager rewrites it while refining, as it rewrites every placeholder.
pub(crate) const PLACEHOLDER_MAX_COST_USD: f64 = 20.0;
/// The longest title a contract has (the schema's `maxLength`).
const TITLE_LIMIT: usize = 120;
/// Why the human's size was given, as the log keeps it.
const SIZED_BY: &str = "sized by the human with farik contract new";
/// The kinds of the task's events printed after each tick.
const SHOWN: [EventKind; 6] = [
    EventKind::RequestTriaged,
    EventKind::QuestionAsked,
    EventKind::ContractWritten,
    EventKind::ContractEvaluated,
    EventKind::EscalationRaised,
    EventKind::TaskTransitioned,
];

/// Where the request comes from.
pub(crate) enum Source<'s> {
    /// A brief the person typed.
    Brief(&'s str),
    /// An issue on the forge, at this address.
    Issue(&'s str),
}

/// What `farik contract new` was asked.
pub(crate) struct Asked<'s> {
    pub(crate) source: Source<'s>,
    pub(crate) size: Option<RequestSize>,
    pub(crate) lock: bool,
}

/// The title and the brief of the request an issue makes: its title, and its title, body, and
/// address.
pub(crate) fn brief_from_issue(issue: &Issue) -> (String, String) {
    (
        issue.title.clone(),
        format!("{}\n\n{}\n\nFrom {}", issue.title, issue.body, issue.url),
    )
}

/// A request a person files from a brief: its title and its intent, the brief, are theirs, and
/// every other field is a placeholder the refine session replaces. The placeholder path is a glob
/// no file matches.
///
/// # Errors
///
/// A sentence saying the title is under three characters or the brief under twenty, the schema's
/// minimums.
pub(crate) fn request_from_brief(
    title: &str,
    brief: &str,
    max_cost_usd: f64,
) -> Result<Value, String> {
    if title.trim().chars().count() < 3 {
        return Err(format!(
            "{title:?} is too short a title: a contract's title is three characters or more"
        ));
    }
    if brief.trim().chars().count() < 20 {
        return Err(format!(
            "{brief:?} is too short a brief: it is the contract's intent, which is twenty \
             characters or more"
        ));
    }
    Ok(json!({
        "title": title,
        "intent": brief,
        "scope": { "in_scope": [PLACEHOLDER], "out_of_scope": [PLACEHOLDER] },
        "requirements": [{ "id": "R1", "text": PLACEHOLDER }],
        "exit_criteria": [{
            "id": "C1",
            "text": PLACEHOLDER,
            "satisfies": ["R1"],
            "verification": { "method": "review", "rubric": [PLACEHOLDER] }
        }],
        "assignee_role": "software_developer",
        "reviewer_role": "software_developer",
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": [PLACEHOLDER]
    }))
}

/// A brief's first line, cut at the longest title a contract has.
fn title_of(brief: &str) -> String {
    brief
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(TITLE_LIMIT)
        .collect()
}

/// `farik contract new`: builds the request and, when no other process drives the project, starts
/// driving it first, so that a start that refuses files nothing; then files the request, sizes it
/// when asked, and drives it with the refining rules until the contract is written or the Product
/// Manager waits on the person, answering its questions at the terminal. When another process
/// drives the project, files the request and hands it over. Answers the exit code: 0 when the
/// loop ends, 1 on a refusal or a failed tick, 130 after an interrupt.
pub(crate) fn contract_new(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    as_json: bool,
) -> i32 {
    let request = title_and_brief(project, &asked.source, io).and_then(|(title, brief)| {
        request_from_brief(
            &title,
            &brief,
            placeholder_budget_usd(&project.team.rules()),
        )
    });
    let request = match request {
        Ok(request) => request,
        Err(error) => return refuse(io, as_json, &error),
    };
    // Held from before the request is filed until the driver ends, so that no other process can
    // start driving in between and leave this one with a request filed and no start.
    let lock = match try_lock(&project.root) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            return match hand_over(project, asked, request, io, as_json) {
                Ok(()) => 0,
                Err(error) => refuse(io, as_json, &error),
            };
        }
        Err(error) => return refuse(io, as_json, &error),
    };
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => return refuse(io, as_json, &error),
    };
    runtime.block_on(async {
        let driver = match start_holding(project, io, lock).await {
            Ok(driver) => driver,
            Err(error) => return refuse(io, as_json, &error),
        };
        let stdin = std::mem::replace(&mut io.stdin, Box::new(std::io::empty()));
        let mut printer = Printer {
            io,
            as_json,
            json_lines: false,
        };
        let filed = match file_here(project, asked, request, &driver, &mut printer).await {
            Ok(task_id) => task_id,
            Err(error) => {
                report_error(&mut printer, &error);
                return finish_quietly(driver, &mut printer, 1).await;
            }
        };
        crate::run::started(&mut printer, &driver);
        with_the_product_manager(project, &filed, asked.lock, driver, stdin, &mut printer).await
    })
}

/// The request's title and brief: the brief's first line and the brief, or the issue's.
fn title_and_brief(
    project: &Project,
    source: &Source<'_>,
    io: &CliIo<'_>,
) -> Result<(String, String), String> {
    Ok(match source {
        Source::Brief(brief) => (title_of(brief), (*brief).to_string()),
        Source::Issue(url) => {
            let program = on_path("gh", io).ok_or(
                "farik contract new --from reads the issue with gh, and there is no gh on PATH",
            )?;
            let forge = Forge {
                program,
                root: project.root.clone(),
            };
            let (title, brief) =
                brief_from_issue(&forge.issue(url).map_err(|error| error.to_string())?);
            (title.chars().take(TITLE_LIMIT).collect(), brief)
        }
    })
}

/// What the human's size makes a request, in words.
fn sized_line(task_id: &TaskId, size: RequestSize) -> String {
    let (size, kind) = match size {
        RequestSize::Large => ("large", "an epic"),
        RequestSize::Small => ("small", "a standalone task"),
    };
    format!("{} sized {size} by you, so it is {kind}", task_id.as_str())
}

/// The budget a request is filed with: the team's cap on a task's budget when it has one, and
/// otherwise `PLACEHOLDER_MAX_COST_USD`.
pub(crate) fn placeholder_budget_usd(rules: &TeamRules) -> f64 {
    rules
        .max_task_budget_usd
        .unwrap_or(PLACEHOLDER_MAX_COST_USD)
}

/// Files `request` as the human's, and answers its id and the line that says so.
fn filed(project: &Project, request: Value, io: &CliIo<'_>) -> Result<(TaskId, String), String> {
    let contract = file_request(
        &project.files,
        &project.log,
        request,
        HUMAN,
        None,
        io.clock.now(),
        &crate::contract::event_ids(project),
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => format!("the request {reason}"),
        other => other.to_string(),
    })?;
    let line = format!(
        "{} filed as a draft request: {}",
        contract.id.as_str(),
        contract.title.as_str()
    );
    Ok((contract.id, line))
}

/// Files the request in the process that drives the project, and sizes it there when asked.
async fn file_here(
    project: &Project,
    asked: &Asked<'_>,
    request: Value,
    driver: &Driver,
    printer: &mut Printer<'_, '_>,
) -> Result<TaskId, String> {
    let (id, line) = filed(project, request, printer.io)?;
    printer.line(&line, &Value::Null);
    if let Some(size) = asked.size {
        said(
            driver
                .orchestrator
                .handle(Command::RequestTriage {
                    task_id: id.clone(),
                    size,
                    reason: SIZED_BY.to_string(),
                })
                .await,
        )?;
        printer.line(&sized_line(&id, size), &Value::Null);
    }
    Ok(id)
}

/// Files the request, sizes it through the process driving the project when asked, and leaves it
/// to that process. `--lock` is refused before anything is filed, because a lock taken now would
/// refuse the Product Manager's writes.
fn hand_over(
    project: &Project,
    asked: &Asked<'_>,
    request: Value,
    io: &mut CliIo<'_>,
    as_json: bool,
) -> Result<(), String> {
    if asked.lock {
        return Err(
            "--lock waits for the Product Manager's contract, which the process driving \
                    this project writes: run farik contract lock FRK-<n> once it has"
                .to_string(),
        );
    }
    let (id, line) = filed(project, request, io)?;
    let mut printer = Printer {
        io,
        as_json,
        json_lines: false,
    };
    printer.line(&line, &Value::Null);
    if let Some(size) = asked.size {
        send(
            &project.root,
            &Command::RequestTriage {
                task_id: id.clone(),
                size,
                reason: SIZED_BY.to_string(),
            },
        )
        .map_err(|error| refusal(&error))?;
        printer.line(&sized_line(&id, size), &Value::Null);
    }
    let pid = crate::daemon_client::read_daemon_file(&project.root.join(crate::start::DAEMON_FILE))
        .ok()
        .and_then(|address| address.pid)
        .map_or_else(|| "unknown".to_string(), |pid| pid.to_string());
    let handed = format!(
        "the farik process driving this project (pid {pid}) takes it from here; answer its \
         questions with farik answer"
    );
    printer.json_lines = true;
    printer.line(
        &handed,
        &json!({ "task_id": id.as_str(), "status": "draft", "handed_to": handed }),
    );
    Ok(())
}

/// Ticks the task with the refining rules until nothing is left to do, asking each question the
/// Product Manager asks at the terminal; then says where the contract stands, what waits on the
/// person, and takes the contract when asked. Shuts the driver down and answers the exit code.
async fn with_the_product_manager(
    project: &Project,
    task_id: &TaskId,
    lock: bool,
    mut driver: Driver,
    stdin: Box<dyn Read + Send>,
    printer: &mut Printer<'_, '_>,
) -> i32 {
    let mut presses = 0;
    let (ended, open) = converse(project, task_id, &mut driver, stdin, printer, &mut presses).await;
    let mut code = 0;
    let readiness = match (&ended, &open) {
        (Ended::Failed(error), _) => {
            report_error(printer, error);
            code = 1;
            None
        }
        (_, Some(question)) => {
            printer.line(
                &format!(
                    "the question stays open: farik answer {} <your answer>, then farik run",
                    question.seq
                ),
                &Value::Null,
            );
            None
        }
        (Ended::Idle(why), None) => Some(judged(project, task_id, why, printer)),
        (Ended::Stopped, None) => Some(judged(project, task_id, "the run was stopped", printer)),
    };
    let waiting = waiting_now(project).unwrap_or_default();
    if !printer.as_json {
        print_waiting(printer, &waiting);
    }
    let mut locked = false;
    if lock && readiness.is_some() && !contract_written(project, task_id) {
        printer.line(
            &format!(
                "not locked: the Product Manager has written no contract for {id} yet: run farik \
                 contract lock {id} once it has",
                id = task_id.as_str()
            ),
            &Value::Null,
        );
    } else if lock && readiness.is_some() {
        match said(
            driver
                .orchestrator
                .handle(Command::ContractLock {
                    task_id: task_id.clone(),
                })
                .await,
        ) {
            Ok(_) => {
                locked = true;
                printer.line("locked: the contract is yours (5.11)", &Value::Null);
            }
            Err(error) => {
                report_error(printer, &error);
                code = 1;
            }
        }
    }
    if printer.as_json {
        let object = json!({
            "task_id": task_id.as_str(),
            "status": status_of(project, task_id),
            "readiness": readiness.unwrap_or_else(|| json!({
                "state": "not_judged",
                "why": "a question is open",
            })),
            "question_open": open.as_ref().map(|question| question.seq),
            "locked": locked,
            "waiting_on_you": waiting.iter().map(Waiting::json).collect::<Vec<_>>(),
        });
        crate::say(&mut printer.io.stdout, &object.to_string());
    }
    let code = finish_quietly(driver, printer, code).await;
    if presses > 0 { INTERRUPTED } else { code }
}

/// The loop of ticks and questions: ticks until nothing is left to do, and at each open question
/// asks the person and records the answer. Answers how the ticks ended, and the question left
/// open at the end of input or on an interrupt.
async fn converse(
    project: &Project,
    task_id: &TaskId,
    driver: &mut Driver,
    stdin: Box<dyn Read + Send>,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
) -> (Ended, Option<OpenQuestion>) {
    let scope = TickScope {
        task_id: Some(task_id.clone()),
        rules: TickRules::Refining,
    };
    let mut seen = last_seq(project);
    let mut stdin = Some(stdin);
    let mut answers: Option<UnboundedReceiver<String>> = None;
    loop {
        let ended = ticks(driver, &scope, printer, presses, |printer| {
            print_new_events(project, task_id, &mut seen, printer);
        })
        .await;
        let open = match &ended {
            Ended::Failed(_) => return (ended, None),
            Ended::Idle(_) | Ended::Stopped => open_question(project, task_id),
        };
        let (Ended::Idle(_), Some(question)) = (&ended, &open) else {
            return (ended, open);
        };
        let answers = answers.get_or_insert_with(|| {
            read_lines(stdin.take().unwrap_or_else(|| Box::new(std::io::empty())))
        });
        let Some(answer) = ask(driver, printer, question, answers, presses).await else {
            return (Ended::Stopped, open);
        };
        let answered = driver
            .orchestrator
            .handle(Command::QuestionAnswer {
                question_id: question.seq,
                answer,
            })
            .await;
        match said(answered) {
            Ok(report) => {
                for line in &report.lines {
                    printer.line(line, &Value::Null);
                }
            }
            Err(error) => report_error(printer, &error),
        }
    }
}

/// Whether the Product Manager has written the task's contract: only then is there one for
/// `--lock` to take.
fn contract_written(project: &Project, task_id: &TaskId) -> bool {
    project
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::ContractWritten],
            ..EventQuery::default()
        })
        .is_ok_and(|written| !written.is_empty())
}

/// The task's status on the board.
fn status_of(project: &Project, task_id: &TaskId) -> Option<String> {
    project
        .projections()
        .ok()
        .and_then(|projections| projections.task(task_id).ok().flatten())
        .map(|row| row.status.to_string())
}

/// A question about the task that no answer names.
struct OpenQuestion {
    seq: u64,
    asked_by: String,
    question: String,
}

/// The task's last question nobody has answered, if any.
fn open_question(project: &Project, task_id: &TaskId) -> Option<OpenQuestion> {
    let history = project
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::QuestionAsked, EventKind::QuestionAnswered],
            ..EventQuery::default()
        })
        .ok()?;
    history.iter().rev().find_map(|event| match &event.body {
        EventBody::QuestionAsked(body)
            if event.envelope.ids.task_id.as_ref() == Some(task_id)
                && !history.iter().any(|answer| {
                    matches!(&answer.body, EventBody::QuestionAnswered(answered)
                        if answered.question_id.get() == event.envelope.seq)
                }) =>
        {
            Some(OpenQuestion {
                seq: event.envelope.seq,
                asked_by: body.asked_by.clone(),
                question: body.question.clone(),
            })
        }
        _ => None,
    })
}

/// Prints the question and `answer> ` until a line that is not blank comes; answers `None` at the
/// end of input or on an interrupt.
async fn ask(
    driver: &mut Driver,
    printer: &mut Printer<'_, '_>,
    question: &OpenQuestion,
    answers: &mut UnboundedReceiver<String>,
    presses: &mut u32,
) -> Option<String> {
    prompt(
        printer,
        &format!(
            "question {} from {}: {}\n",
            question.seq, question.asked_by, question.question
        ),
    );
    loop {
        prompt(printer, "answer> ");
        tokio::select! {
            line = answers.recv() => match line {
                Some(line) if line.trim().is_empty() => {}
                Some(line) => return Some(line.trim().to_string()),
                None => return None,
            },
            Some(()) = driver.interrupts.recv() => {
                *presses += 1;
                if printer.as_json {
                    printer.note("");
                } else {
                    crate::say(&mut printer.io.stdout, "");
                }
                return None;
            }
        }
    }
}

/// Writes `text` with no line after it where the person reads the prompt: standard output, or
/// standard error under `--json`.
fn prompt(printer: &mut Printer<'_, '_>, text: &str) {
    let stream = if printer.as_json {
        &mut printer.io.stderr
    } else {
        &mut printer.io.stdout
    };
    let _ = write!(stream, "{}", crate::printable::printable(text));
    let _ = stream.flush();
}

/// One thread that owns standard input for the whole loop and sends each line on; it ends at the
/// end of input, or, still blocked, with the process.
fn read_lines(stdin: Box<dyn Read + Send>) -> UnboundedReceiver<String> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdin).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The last sequence number in the log.
fn last_seq(project: &Project) -> u64 {
    project
        .log
        .read(&EventQuery::default())
        .ok()
        .and_then(|events| events.last().map(|event| event.envelope.seq))
        .unwrap_or_default()
}

/// Prints the task's events of the shown kinds after `seen`, as `task show` prints them.
fn print_new_events(
    project: &Project,
    task_id: &TaskId,
    seen: &mut u64,
    printer: &mut Printer<'_, '_>,
) {
    let Ok(events) = project.log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: SHOWN.to_vec(),
        ..EventQuery::default()
    }) else {
        return;
    };
    let after = *seen;
    for event in events.iter().filter(|event| event.envelope.seq > after) {
        printer.line(&event_line(event), &Value::Null);
        *seen = event.envelope.seq;
    }
}

/// Prints the contract as `task show` prints its body and where it stands against the Definition
/// of Ready, and answers the latter as JSON.
fn judged(project: &Project, task_id: &TaskId, why: &str, printer: &mut Printer<'_, '_>) -> Value {
    if let Ok(contract) = project.files.read_contract(task_id) {
        for line in body_lines(&contract) {
            printer.line(&line, &Value::Null);
        }
    }
    let (state, failures) = readiness(project, task_id);
    let (text, value) = match state {
        "passed" => (
            "readiness: passed".to_string(),
            json!({ "state": "passed" }),
        ),
        "failed" => (
            "readiness: failed".to_string(),
            json!({ "state": "failed", "failures": failures }),
        ),
        "structural" => (
            "readiness: passed the structural checks".to_string(),
            json!({ "state": "structural" }),
        ),
        _ => (
            format!("readiness: not judged yet ({why})"),
            json!({ "state": "not_judged", "why": why }),
        ),
    };
    printer.line(&text, &Value::Null);
    for failure in &failures {
        printer.line(&format!("  {failure}"), &Value::Null);
    }
    value
}

/// Where the task stands against the Definition of Ready: the last `contract.evaluated` since it
/// last moved into `refining`, passed or failed with its failures; none, and escalated for the
/// person's approval, `structural`; otherwise not judged.
fn readiness(project: &Project, task_id: &TaskId) -> (&'static str, Vec<String>) {
    let history = project
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::TaskTransitioned, EventKind::ContractEvaluated],
            ..EventQuery::default()
        })
        .unwrap_or_default();
    let began = history
        .iter()
        .rev()
        .find(|event| {
            matches!(&event.body, EventBody::TaskTransitioned(body)
                if body.to.to_string() == TaskStatus::Refining.to_string())
        })
        .map_or(0, |event| event.envelope.seq);
    let evaluated = history.iter().rev().find_map(|event| match &event.body {
        EventBody::ContractEvaluated(body) if event.envelope.seq > began => Some(body),
        _ => None,
    });
    if let Some(body) = evaluated {
        return if body.passed {
            ("passed", Vec::new())
        } else {
            ("failed", body.failures.clone())
        };
    }
    let awaiting = project
        .projections()
        .ok()
        .and_then(|projections| projections.task(task_id).ok().flatten())
        .is_some_and(|row| row.status == TaskStatus::Escalated && row.awaiting_approval);
    if awaiting {
        ("structural", Vec::new())
    } else {
        ("not_judged", Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::validate_contract;
    use farik_runtime::forge::Issue;
    use serde_json::{Value, json};

    use farik_core::governor::team_rules::TeamRules;

    use super::{PLACEHOLDER, brief_from_issue, placeholder_budget_usd, request_from_brief};

    /// Every string in `value` that is one of its texts, as against its ids, roles, and risk.
    fn texts(value: &Value) -> Vec<String> {
        [
            value["scope"]["in_scope"][0].clone(),
            value["scope"]["out_of_scope"][0].clone(),
            value["requirements"][0]["text"].clone(),
            value["exit_criteria"][0]["text"].clone(),
            value["exit_criteria"][0]["verification"]["rubric"][0].clone(),
            value["allowed_paths"][0].clone(),
        ]
        .iter()
        .map(|text| text.as_str().unwrap_or_default().to_string())
        .collect()
    }

    #[test]
    fn builds_a_request_the_store_files() {
        let brief = "Add done.txt and a check that it exists";
        assert_eq!(brief.len(), 39);
        let brief = format!("{brief}.");
        let budget = placeholder_budget_usd(&TeamRules::default());
        let mut request =
            request_from_brief("Add done.txt", &brief, budget).expect("a request is built");
        assert!(texts(&request).iter().all(|text| text == PLACEHOLDER));
        request["id"] = json!("FRK-1");
        request["status"] = json!("draft");
        validate_contract(&request).expect("the store would file it");

        assert!(request_from_brief("Add done.txt", "A brief too short.!", 5.0).is_err());
        assert!(request_from_brief("Ad", &brief, 5.0).is_err());
    }

    #[test]
    fn places_a_budget_of_the_team_cap_or_twenty_dollars() {
        let capped = TeamRules {
            max_task_budget_usd: Some(12.5),
            ..TeamRules::default()
        };
        assert!((placeholder_budget_usd(&capped) - 12.5).abs() < 1e-9);
        assert!((placeholder_budget_usd(&TeamRules::default()) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn reads_a_request_from_an_issue() {
        assert_eq!(
            brief_from_issue(&Issue {
                title: "Add done.txt".to_string(),
                body: "It should exist.".to_string(),
                url: "https://github.com/o/r/issues/3".to_string(),
            }),
            (
                "Add done.txt".to_string(),
                "Add done.txt\n\nIt should exist.\n\nFrom https://github.com/o/r/issues/3"
                    .to_string()
            )
        );
    }
}
