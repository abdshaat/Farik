//! `farik contract lock` and `farik contract unlock`: contract ownership (`docs/SPEC.md` section
//! 5.11).

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_protocol::event::EventIds;
use farik_store::requests::hold_contract;
use serde_json::json;

use crate::Report;
use crate::project::Project;

/// Takes a contract, or gives it back, through `farik_store::requests::hold_contract`, the one
/// way every caller does it: the governor's `check_contract_write` decides, the file and the log
/// record it.
///
/// # Errors
///
/// A sentence saying there is no such task, that the contract is already held or already the
/// team's, the governor's own refusal, or what could not be written.
pub fn hold(
    project: &Project,
    task_id: &str,
    held: bool,
    now: DateTime<Utc>,
) -> Result<Report, String> {
    let task_id: TaskId = task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
    let projections = project.projections()?;
    let event = hold_contract(
        &project.files,
        &project.log,
        &projections,
        &task_id,
        held,
        now,
        &event_ids(project),
    )
    .map_err(|error| error.to_string())?;

    Ok(Report {
        lines: vec![if held {
            format!(
                "{} is yours: agents may record criterion results and write notes, and nothing \
                 else",
                task_id.as_str()
            )
        } else {
            format!("{} is the team's again", task_id.as_str())
        }],
        json: json!({
            "task_id": task_id.to_string(),
            "locked": held,
            "events": [event.envelope.seq],
        }),
        json_lines: None,
    })
}

/// The ids every event this project records carries, with no task, agent, or session named.
pub(crate) fn event_ids(project: &Project) -> EventIds {
    EventIds {
        team_id: project.ids.team_id.clone(),
        project_id: project.ids.project_id.clone(),
        ..EventIds::default()
    }
}
