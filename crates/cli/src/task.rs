//! `farik task create`: file a YAML contract as a draft request (F3, `docs/SPEC.md` section 5.16).

use std::path::Path;

use chrono::{DateTime, Utc};
use farik_core::contract::{TaskId, TaskKind};
use farik_core::governor::gates::{ContractWriteActor, ParentEpic, check_child_creation};
use farik_core::governor::transition_table::TransitionActor;
use farik_protocol::event::EventIds;
use farik_store::EventQuery;
use farik_store::requests::{RequestError, fields_not_the_authors, file_request};
use serde_json::json;

use crate::project::Project;
use crate::{HUMAN, Report};

/// Reads the contract in `file` and files it as a `draft` request through `file_request`, which
/// gives it the next id.
///
/// The contract goes through `command_from_value` rather than through `validate_contract` alone, so
/// that a contract filed from a terminal is held to exactly the rules one arriving from an agent is.
///
/// `file` is taken as the person typed it: absolute as it is, relative to `cwd`, which is where the
/// command was run.
///
/// # Errors
///
/// A sentence saying the file could not be read, that it is not YAML, that it carries a field it is
/// not the author's to write, every rule the contract breaks, or what could not be written.
pub fn create(
    project: &Project,
    cwd: &Path,
    file: &Path,
    parent: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Report, String> {
    let parent = parent
        .map(|parent| epic_in_progress(project, parent))
        .transpose()?;
    // A path a person typed is relative to where they typed it, and `cwd` is where that was: the
    // project is the whole repository, so the directory the command was run in is not where the
    // project is, and reading the file through the process's own current directory would make the
    // library answer differently from the binary.
    let file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };
    let text = std::fs::read_to_string(&file)
        .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
    let wire = farik_store::files::yaml_value(&text, &file.display().to_string())
        .map_err(|error| error.to_string())?;
    // The store's words are said to whoever files, an agent included; a person at a terminal is
    // also told which commands do what the request tried to.
    let reminder = if fields_not_the_authors(&wire).is_empty() {
        ""
    } else {
        "; from the command line, farik contract lock takes the lock and farik triage gives the size"
    };
    let ids = EventIds {
        team_id: project.ids.team_id.clone(),
        project_id: project.ids.project_id.clone(),
        ..EventIds::default()
    };
    let contract = file_request(
        &project.files,
        &project.log,
        wire,
        HUMAN,
        parent.as_ref(),
        now,
        &ids,
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => {
            format!("{} {reason}{reminder}", file.display())
        }
        other => other.to_string(),
    })?;
    let seqs: Vec<u64> = project
        .log
        .read(&EventQuery {
            task_id: Some(contract.id.clone()),
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?
        .iter()
        .map(|event| event.envelope.seq)
        .collect();

    let lines = match &parent {
        Some(parent) => vec![
            format!(
                "{} filed as a task of {}: {}",
                contract.id.as_str(),
                parent.as_str(),
                contract.title.as_str()
            ),
            format!(
                "farik run judges it against the Definition of Ready, and {}'s assignee assigns \
                 it",
                parent.as_str()
            ),
        ],
        None => vec![
            format!(
                "{} filed as a draft request: {}",
                contract.id.as_str(),
                contract.title.as_str()
            ),
            "farik triage says whether it is large or small; nothing starts before that \
             (5.16)"
                .to_string(),
        ],
    };
    Ok(Report {
        lines,
        json: json!({
            "task_id": contract.id.to_string(),
            "title": contract.title,
            "status": "draft",
            "path": format!(".farik/contracts/{}.yaml", contract.id.as_str()),
            "events": seqs,
        }),
        json_lines: None,
    })
}

/// The epic `parent` names, when the board holds it as an epic under which the human may file a
/// task now (`check_child_creation`, 5.16 item 3).
fn epic_in_progress(project: &Project, parent: &str) -> Result<TaskId, String> {
    let task_id = crate::task(parent)?;
    let row = project
        .projections()?
        .task(&task_id)
        .map_err(|error| error.to_string())?
        .filter(|row| row.kind == TaskKind::Epic)
        .ok_or_else(|| format!("{parent} is not an epic: a task is filed under an epic"))?;
    check_child_creation(
        &ParentEpic {
            status: row.status,
            assignee_id: row.assignee_id.unwrap_or_default(),
        },
        &ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        },
    )
    .map_err(|reasons| reasons.join("; "))?;
    Ok(task_id)
}
