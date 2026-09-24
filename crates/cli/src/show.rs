//! `farik task show`: one contract, and what happened to it (F3).

use farik_core::contract::{TaskContract, TaskId, TaskKind};
use farik_protocol::event::{FarikEvent, event_to_value};
use farik_store::requests::board_row_json;
use farik_store::{CostProjection, CostScope, EventQuery, Git, TaskProjection};
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// One task: its contract from the file, its status from the log, every event about it with what
/// each said, what it cost, an epic's tasks, and with `with_diff` its branch's diff.
///
/// The file decides a contract's content and the log decides its status (`docs/SPEC.md` section
/// 8.4), so when the two disagree this says so rather than picking one silently.
///
/// # Errors
///
/// A sentence saying the id is not one, that there is no such contract, what the store refused;
/// with `with_diff`, that an epic has no branch, that the task has none yet, or what git refused.
pub fn show(project: &Project, task_id: &str, with_diff: bool) -> Result<Report, String> {
    let task_id = crate::task(task_id)?;
    let contract = project
        .files
        .read_contract(&task_id)
        .map_err(|error| error.to_string())?;
    let projections = project.projections()?;
    let row = projections
        .task(&task_id)
        .map_err(|error| error.to_string())?;
    let events = project
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;

    let status = row
        .as_ref()
        .map_or_else(|| contract.status.to_string(), |row| row.status.to_string());
    let mut lines = vec![
        format!("{} {}", task_id.as_str(), contract.title.as_str()),
        format!(
            "{} {}, {} risk{}",
            status,
            contract.kind,
            contract.risk,
            if contract.locked { ", yours" } else { "" }
        ),
    ];
    if row.is_none() {
        lines.push(
            "the log has never heard of this task, so the status above is the file's own: farik \
             doctor says where the files and the log disagree"
                .to_string(),
        );
    } else if status != contract.status.to_string() {
        lines.push(format!(
            "the file says {} and the log says {status}: farik doctor reports that",
            contract.status
        ));
    }
    lines.extend(body_lines(&contract));
    lines.push(String::new());
    lines.push("events".to_string());
    lines.extend(events.iter().map(event_line));

    let cost = projections
        .costs(CostScope::Task)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|cost| cost.key == task_id.as_str());
    lines.push(String::new());
    lines.push(cost_line(cost.as_ref(), &contract));
    let children: Vec<TaskProjection> = projections
        .board()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|child| child.parent.as_ref() == Some(&task_id))
        .collect();
    if contract.kind == TaskKind::Epic {
        lines.push("children".to_string());
        lines.extend(children.iter().map(|child| {
            format!(
                "  {} {} {}",
                child.task_id.as_str(),
                child.status,
                child.title
            )
        }));
    }
    let diff = if with_diff {
        let diff = diff_of(project, &task_id, &contract, &events)?;
        lines.push(String::new());
        lines.extend(diff.lines().map(ToString::to_string));
        Some(diff)
    } else {
        None
    };

    Ok(Report {
        lines,
        json: json!({
            "cost": cost.as_ref().map(cost_json),
            "children": children.iter().map(board_row_json).collect::<Vec<_>>(),
            "diff": diff,
            "task_id": task_id.as_str(),
            "title": contract.title.as_str(),
            "status": status,
            "file_status": contract.status.to_string(),
            "kind": contract.kind.to_string(),
            "risk": contract.risk.to_string(),
            "locked": contract.locked,
            "events": events
                .iter()
                .map(farik_protocol::event::event_to_value)
                .collect::<Vec<_>>(),
        }),
        json_lines: None,
    })
}

/// A contract's body as a person reads it: its intent, its requirements, and its exit criteria.
pub(crate) fn body_lines(contract: &TaskContract) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        format!("intent: {}", contract.intent.as_str()),
        String::new(),
        "requirements".to_string(),
    ];
    for requirement in &contract.requirements {
        lines.push(format!(
            "  {} {}",
            requirement.id.as_str(),
            requirement.text.as_str()
        ));
    }
    lines.push("exit criteria".to_string());
    for criterion in &contract.exit_criteria {
        lines.push(format!(
            "  {} {} [{}]",
            criterion.id.as_str(),
            criterion.text.as_str(),
            farik_core::contract::Verification::from(&criterion.verification).method()
        ));
    }
    lines
}

/// One event as a line: its number, its time, its kind, and what it said.
pub(crate) fn event_line(event: &FarikEvent) -> String {
    let mut line = format!(
        "  {:>4} {} {}",
        event.envelope.seq,
        event
            .envelope
            .recorded_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        event.body.kind()
    );
    if let Some(summary) = summary(event) {
        line.push_str(" — ");
        line.push_str(&cut(&summary, SUMMARY_LIMIT));
    }
    line
}

/// What the task has cost against its budget.
fn cost_line(cost: Option<&CostProjection>, contract: &TaskContract) -> String {
    match cost {
        Some(cost) => format!(
            "cost: ${:.2} of ${:.2}; sessions: {}; tokens: {} in, {} out",
            cost.usd,
            contract.budget.max_cost_usd,
            cost.sessions,
            cost.input_tokens,
            cost.output_tokens
        ),
        None => "cost: nothing yet".to_string(),
    }
}

/// What the task has cost, as JSON.
fn cost_json(cost: &CostProjection) -> Value {
    json!({
        "usd": cost.usd,
        "sessions": cost.sessions,
        "input_tokens": cost.input_tokens,
        "output_tokens": cost.output_tokens,
    })
}

/// How much of an event's summary a line keeps.
const SUMMARY_LIMIT: usize = 200;

/// What an event said, in a few words, for the kinds a person reads a task's story by.
fn summary(event: &FarikEvent) -> Option<String> {
    let value = event_to_value(event);
    let body = &value["body"];
    let text = |field: &str| body[field].as_str().unwrap_or_default().to_string();
    let with = |head: String, tail: &str| {
        if tail.is_empty() {
            head
        } else {
            format!("{head}: {tail}")
        }
    };
    Some(match value["kind"].as_str()? {
        "task.transitioned" => with(
            format!(
                "{} -> {} by {}",
                text("from"),
                text("to"),
                text("requested_by")
            ),
            &text("reason"),
        ),
        "request.triaged" => format!(
            "{} by {}: {}",
            text("size"),
            text("triaged_by"),
            text("reason")
        ),
        "question.asked" => format!(
            "question {} from {}: {}",
            event.envelope.seq,
            text("asked_by"),
            text("question")
        ),
        "question.answered" => format!(
            "answer to {} by {}: {}",
            body["question_id"],
            text("answered_by"),
            text("answer")
        ),
        "escalation.raised" => format!("{}: {}", text("reason"), text("detail")),
        "escalation.resolved" => format!(
            "to {} by {}: {}",
            text("to"),
            text("resolved_by"),
            text("message")
        ),
        "human.accepted" => with(
            format!("{} by {}", text("subject"), text("accepted_by")),
            &text("message"),
        ),
        "criterion.recorded" => format!(
            "{} {}, run by {}",
            text("criterion_id"),
            if body["passed"].as_bool() == Some(true) {
                "passed"
            } else {
                "failed"
            },
            text("run_by")
        ),
        "note.written" => format!(
            "{}: {}",
            text("kind"),
            text("text").lines().next().unwrap_or_default()
        ),
        "contract.evaluated" => {
            if body["passed"].as_bool() == Some(true) {
                format!("{} passed", text("gate"))
            } else {
                let failures: Vec<&str> = body["failures"]
                    .as_array()
                    .map(|failures| failures.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                format!("{} failed: {}", text("gate"), failures.join("; "))
            }
        }
        "session.started" => format!(
            "{} session {} for {}",
            text("purpose"),
            value["session_id"].as_str().unwrap_or_default(),
            value["agent_id"].as_str().unwrap_or_default()
        ),
        "session.ended" => text("reason"),
        "cost.recorded" => format!("${:.2}", body["cost_usd"].as_f64().unwrap_or_default()),
        "task.integrated" => format!(
            "{} into {} by {}",
            text("sha").chars().take(12).collect::<String>(),
            text("into"),
            text("integrated_by")
        ),
        "pull_request.opened" => text("url"),
        _ => return None,
    })
}

/// `text` cut at `limit` characters, with `…` when it was cut.
fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let kept: String = text.chars().take(limit).collect();
        format!("{kept}…")
    }
}

/// The task's branch's diff against the integration branch (5.14): before integration, what the
/// branch adds; after, against the first parent of the merge commit it was integrated by, or, when
/// its integration was not a merge commit, against the integration branch, which then holds it all.
fn diff_of(
    project: &Project,
    task_id: &TaskId,
    contract: &TaskContract,
    events: &[FarikEvent],
) -> Result<String, String> {
    let id = task_id.as_str();
    if contract.kind == TaskKind::Epic {
        return Err(format!(
            "{id} is an epic and has no branch: farik task show <task> --diff shows each of its \
             tasks'"
        ));
    }
    let git = Git::open(project.root.clone());
    let branch = format!("farik/{id}");
    // `merge-base x x` answers `x`'s commit, and refuses a name that names none.
    if git.merge_base(&branch, &branch).is_err() {
        return Err(format!(
            "{id} has no branch yet: its work starts at assignment"
        ));
    }
    let integrated = events.iter().rev().find_map(|event| {
        let value = event_to_value(event);
        (value["kind"] == "task.integrated").then(|| {
            (
                value["body"]["sha"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                value["body"]["into"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
    });
    let words = |error: farik_store::GitError| error.to_string();
    match integrated {
        None => {
            let base = farik_runtime::transitions::integration_branch(&project.team, &git)
                .map_err(words)?;
            git.diff(&base, &branch).map_err(words)
        }
        Some((sha, into)) => {
            let second_parent = format!("{sha}^2");
            if git.merge_base(&second_parent, &second_parent).is_ok() {
                git.diff(&format!("{sha}^1"), &branch).map_err(words)
            } else {
                let diff = git.diff(&into, &branch).map_err(words)?;
                if diff.trim().is_empty() {
                    Ok(format!(
                        "{branch} is wholly in {into}; its merge is not a commit farik can diff \
                         against"
                    ))
                } else {
                    Ok(diff)
                }
            }
        }
    }
}
