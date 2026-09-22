//! `farik task create`: file a YAML contract as a draft request (F3, `docs/SPEC.md` section 5.16).

use std::path::Path;

use chrono::{DateTime, Utc};
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
    now: DateTime<Utc>,
) -> Result<Report, String> {
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
    let contract = file_request(&project.files, &project.log, wire, HUMAN, None, now, &ids)
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

    Ok(Report {
        lines: vec![
            format!(
                "{} filed as a draft request: {}",
                contract.id.as_str(),
                contract.title.as_str()
            ),
            "farik triage says whether it is large or small; nothing starts before that \
             (5.16)"
                .to_string(),
        ],
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
