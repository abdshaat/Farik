//! `farik triage`: record how big a request is, or overrule the triage that did (`docs/SPEC.md`
//! section 5.16).

use chrono::{DateTime, Utc};
use farik_core::contract::{TaskId, TaskKind};
use farik_protocol::command::{Command, RequestSize, command_from_value};
use farik_store::requests::triage_by_human;
use serde_json::json;

use crate::Report;
use crate::contract::event_ids;
use crate::project::Project;

/// Records the size of a request, and the kind that follows from it.
///
/// Large makes the request an epic and small makes it one standalone task, which is what the kind on
/// its contract says; the human may record either while the request is still a draft, and this is
/// both the first triage and the overrule of one, because there is one rule for who may say how big
/// a request is and when.
///
/// # Errors
///
/// A sentence saying there is no such task, that refining has already started, or what could not be
/// written.
pub fn triage(
    project: &Project,
    task_id: &str,
    size: RequestSize,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Report, String> {
    // The id is parsed here rather than left to the schema: a person who typed it wrongly should read
    // what is wrong with what they typed, not the `oneOf` sentence a schema refuses a whole body
    // with. The command is still built and validated, so a triage from a terminal is held to exactly
    // the rules one arriving from an agent is.
    let named: TaskId = task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
    let command = command_from_value(&json!({
        "command": "request_triage",
        "body": { "task_id": named.as_str(), "size": wire_size(size), "reason": reason }
    }))
    .map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{} {}", error.path, error.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let Command::RequestTriage {
        task_id,
        size,
        reason,
    } = command
    else {
        return Err("the command line built a command the reader did not read back".to_string());
    };

    let event = triage_by_human(
        &project.files,
        &project.log,
        &project.projections()?,
        &task_id,
        size,
        &reason,
        now,
        &event_ids(project),
    )
    .map_err(|error| error.to_string())?;
    let seq = event.envelope.seq;
    let kind = match size {
        RequestSize::Large => TaskKind::Epic,
        RequestSize::Small => TaskKind::Task,
    }
    .to_string();

    Ok(Report {
        lines: vec![format!(
            "{} is {}: {kind}. {reason}",
            task_id.as_str(),
            match size {
                RequestSize::Large => "large",
                RequestSize::Small => "small",
            }
        )],
        json: json!({
            "task_id": task_id.to_string(),
            "size": wire_size(size),
            "kind": kind,
            "reason": reason,
            "events": [seq],
        }),
        json_lines: None,
    })
}

/// How the size is spelled on the wire.
fn wire_size(size: RequestSize) -> &'static str {
    match size {
        RequestSize::Large => "large",
        RequestSize::Small => "small",
    }
}
