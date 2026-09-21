//! `farik task show`: one contract, and what happened to it (F3).

use farik_core::contract::TaskId;
use farik_store::EventQuery;
use serde_json::json;

use crate::Report;
use crate::project::Project;

/// One task: its contract from the file, its status from the log, and every event about it.
///
/// The file decides a contract's content and the log decides its status (`docs/SPEC.md` section
/// 8.4), so when the two disagree this says so rather than picking one silently.
///
/// # Errors
///
/// A sentence saying the id is not one, that there is no such contract, or what the store refused.
pub fn show(project: &Project, task_id: &str) -> Result<Report, String> {
    let task_id: TaskId = task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
    let contract = project
        .files
        .read_contract(&task_id)
        .map_err(|error| error.to_string())?;
    let row = project
        .projections()?
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
        String::new(),
        format!("intent: {}", contract.intent.as_str()),
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
    lines.push(String::new());
    lines.push("requirements".to_string());
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
    lines.push(String::new());
    lines.push("events".to_string());
    for event in &events {
        lines.push(format!(
            "  {:>4} {} {}",
            event.envelope.seq,
            event
                .envelope
                .recorded_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            event.body.kind()
        ));
    }

    Ok(Report {
        lines,
        json: json!({
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
