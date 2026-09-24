//! `farik sprint show`: one sprint and how it went (`docs/SPEC.md` sections 3 and 5.5).

use farik_core::sprint::{Sprint, SprintStatus};
use farik_runtime::sprints::planning_session_spent;
use farik_store::CostScope;
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// The sprint named, else the open one, else the latest, else a line saying there is none yet:
/// its id, status, start and end, budget, what it spent, and each task in it with its status.
///
/// # Errors
///
/// A sentence naming a sprint that is not there, or saying what the store or a file refused.
pub fn show(project: &Project, sprint_id: Option<&str>) -> Result<Report, String> {
    let projections = project.projections()?;
    let named = match sprint_id {
        Some(id) => Some(id.to_string()),
        None => projections
            .open_sprint()
            .map_err(|error| error.to_string())?
            .map(|open| open.sprint_id),
    };
    let sprint = match named {
        Some(id) => project
            .files
            .read_sprint(&id)
            .map_err(|error| match error {
                farik_store::files::FilesError::NotFound { .. } => {
                    format!("{id} is not in this project")
                }
                other => other.to_string(),
            })?,
        None => match project
            .files
            .list_sprints()
            .map_err(|error| error.to_string())?
            .pop()
        {
            Some(latest) => latest,
            None => {
                return Ok(Report {
                    lines: vec!["no sprint yet: farik sprint start starts one".to_string()],
                    json: json!({ "sprint": null }),
                    json_lines: None,
                });
            }
        },
    };
    let id = sprint.id.as_str();
    let spent = projections
        .costs(CostScope::Sprint)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|row| row.key == id)
        .map_or(0.0, |row| row.usd);
    let mut tasks = Vec::new();
    for task_id in &sprint.task_ids {
        let task_id = crate::task(task_id.as_str())?;
        let status = projections
            .task(&task_id)
            .map_err(|error| error.to_string())?
            .map_or_else(
                || "not on the board".to_string(),
                |row| row.status.to_string(),
            );
        tasks.push((task_id.as_str().to_string(), status));
    }
    // An empty sprint whose planning session planned nothing stays open until the human ends it.
    let waits_for_its_end = sprint.status == SprintStatus::Open
        && tasks.is_empty()
        && planning_session_spent(&project.log, id).map_err(|error| error.to_string())?;
    let mut lines = lines(&sprint, spent, &tasks);
    if waits_for_its_end {
        lines.push("empty: end it with farik sprint end".to_string());
    }
    Ok(Report {
        lines,
        json: sprint_json(&sprint, spent, &tasks),
        json_lines: None,
    })
}

fn status(sprint: &Sprint) -> &'static str {
    match sprint.status {
        SprintStatus::Open => "open",
        SprintStatus::Ended => "ended",
    }
}

fn lines(sprint: &Sprint, spent: f64, tasks: &[(String, String)]) -> Vec<String> {
    let mut lines = vec![
        format!("{} {}", sprint.id.as_str(), status(sprint)),
        format!("started {}", sprint.started_at),
    ];
    if let Some(ended_at) = &sprint.ended_at {
        lines.push(format!("ended {ended_at}"));
    }
    lines.push(sprint.budget_usd.map_or_else(
        || "budget none".to_string(),
        |usd| format!("budget ${usd:.2}"),
    ));
    lines.push(format!("spent ${spent:.2}"));
    lines.extend(
        tasks
            .iter()
            .map(|(task_id, status)| format!("  {task_id:<9} {status}")),
    );
    lines
}

/// The sprint as the wire says it: `snake_case` keys, `null` for no end and no budget.
fn sprint_json(sprint: &Sprint, spent: f64, tasks: &[(String, String)]) -> Value {
    json!({
        "sprint_id": sprint.id.as_str(),
        "status": status(sprint),
        "started_at": sprint.started_at.to_string(),
        "ended_at": sprint.ended_at.as_ref().map(ToString::to_string),
        "budget_usd": sprint.budget_usd,
        "spent_usd": spent,
        "tasks": tasks
            .iter()
            .map(|(task_id, status)| json!({ "task_id": task_id, "status": status }))
            .collect::<Vec<_>>(),
    })
}
