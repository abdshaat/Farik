//! The human's commands (`docs/SPEC.md` sections 5.2, 5.4, 5.7, 5.14, and 5.16): approve, accept,
//! answer, integrate, resolve, cancel, and stop, each one `Orchestrator::handle` command, reaching
//! the process driving the project from any terminal (ADR 0014).

use farik_core::contract::TaskId;
use farik_protocol::command::Command;
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::orchestrator::{CommandError, CommandReport};
use farik_store::EventQuery;
use serde_json::json;

use crate::Report;
use crate::project::Project;
use crate::start::{send, try_lock};

/// `farik stop`: the run, or with `target` one session, in the process driving the project. A
/// `FRK-<n>` names the task's last session started and not ended.
///
/// # Errors
///
/// A sentence saying nothing drives the project, or the task has no session running; or the
/// command's refusal.
pub fn stop(project: &Project, target: Option<&str>) -> Result<Report, String> {
    if let Some(lock) = try_lock(&project.root)? {
        drop(lock);
        return Err(
            "no farik process is driving this project, so there is nothing to stop".to_string(),
        );
    }
    let command = match target {
        None => Command::RunStop,
        Some(target) => match target.parse::<TaskId>() {
            Ok(task_id) => Command::SessionStop {
                session_id: running_session(project, &task_id)?
                    .ok_or_else(|| format!("{} has no session running", task_id.as_str()))?,
            },
            Err(_) => Command::SessionStop {
                session_id: target.to_string(),
            },
        },
    };
    said(send(&project.root, &command))
}

/// The task's last `session.started` that no `session.ended` follows.
fn running_session(project: &Project, task_id: &TaskId) -> Result<Option<String>, String> {
    let sessions = project
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::SessionStarted, EventKind::SessionEnded],
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;
    let ended: Vec<&str> = sessions
        .iter()
        .filter(|event| matches!(event.body, EventBody::SessionEnded(_)))
        .filter_map(|event| event.envelope.ids.session_id.as_deref())
        .collect();
    Ok(sessions
        .iter()
        .rev()
        .find(|event| matches!(event.body, EventBody::SessionStarted(_)))
        .and_then(|event| event.envelope.ids.session_id.clone())
        .filter(|session| !ended.contains(&session.as_str())))
}

/// A command's outcome as the command line prints it: `said`, or its refusal.
///
/// # Errors
///
/// The refusal's text.
pub fn said(outcome: Result<CommandReport, CommandError>) -> Result<Report, String> {
    let report = outcome.map_err(|error| refusal(&error))?;
    Ok(Report {
        lines: vec![report.said.clone()],
        json: json!({ "said": report.said, "events": report.events }),
        json_lines: None,
    })
}

/// A `CommandError` in the words a person reads after `farik: `.
#[must_use]
pub fn refusal(error: &CommandError) -> String {
    match error {
        CommandError::Invalid { detail } | CommandError::Failed { detail } => detail.clone(),
        CommandError::Refused { reason } => reason.clone(),
        CommandError::NotFound { what } => format!("{what} is not in this project"),
    }
}
