//! `farik triage`: record how big a request is, or overrule the triage that did (`docs/SPEC.md`
//! section 5.16).

use chrono::{DateTime, Utc};
use farik_core::contract::{TaskId, TaskKind, TaskStatus};
use farik_core::governor::gates::check_human_triage;
use farik_protocol::command::{Command, RequestSize, command_from_value};
use farik_protocol::event::EventBody;
use farik_protocol::generated::event::{RequestTriagedBody, RequestTriagedBodySize};
use serde_json::json;

use crate::project::Project;
use crate::{HUMAN, Report};

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
    // 5.16 says the triage records its decision with a reason, and the log is where a person reads it
    // back months later. A blank one is not a reason; the command schema says `reason` is a string and
    // nothing more, so this is the place that holds it to being written.
    if reason.trim().is_empty() {
        return Err(
            "a triage is recorded with a reason, and the log is where somebody reads it back: say \
             why this is the size it is"
                .to_string(),
        );
    }
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

    let mut contract = project
        .files
        .read_contract(&task_id)
        .map_err(|error| error.to_string())?;
    check_human_triage(status_of(project, &task_id)?, contract.parent.is_some())
        .map_err(|reasons| reasons.join("; "))?;

    // The triage decides the kind, and its own tool is what changes it (5.11, 5.16 item 1). The
    // file is written as well as the event, because the board takes the kind from the event and the
    // file is what a person reads.
    contract.kind = match size {
        RequestSize::Large => TaskKind::Epic,
        RequestSize::Small => TaskKind::Task,
    };
    contract.updated_at = Some(now);
    project
        .files
        .write_contract(&contract)
        .map_err(|error| error.to_string())?;
    let kind = contract.kind.to_string();

    let event = project.event(
        EventBody::RequestTriaged(RequestTriagedBody {
            reason: reason.clone(),
            size: match size {
                RequestSize::Large => RequestTriagedBodySize::Large,
                RequestSize::Small => RequestTriagedBodySize::Small,
            },
            triaged_by: HUMAN.to_string(),
        }),
        now,
        Some(task_id.clone()),
    )?;
    let seq = project.append(&event)?;

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
    })
}

/// The status the log says the task is at, which is the source of truth for it (`docs/SPEC.md`
/// section 8.4).
///
/// # Errors
///
/// A sentence saying the board has never heard of this task, or what the store refused.
pub(crate) fn status_of(project: &Project, task_id: &TaskId) -> Result<TaskStatus, String> {
    let projections = farik_store::open_projections(std::sync::Arc::clone(&project.log))
        .map_err(|error| error.to_string())?;
    let row = projections
        .task(task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "the log has never heard of {}, so there is nothing of it to change: farik \
                 doctor says where the files and the log disagree",
                task_id.as_str()
            )
        })?;
    Ok(row.status)
}

/// How the size is spelled on the wire.
fn wire_size(size: RequestSize) -> &'static str {
    match size {
        RequestSize::Large => "large",
        RequestSize::Small => "small",
    }
}
