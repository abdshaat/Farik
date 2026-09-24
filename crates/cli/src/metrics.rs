//! `farik metrics`: the harness metrics of F17, over the whole project or one sprint.

use farik_store::files::FilesError;
use farik_store::metrics::{CostSplit, HarnessMetrics};
use serde_json::{Map, Value, json};

use crate::Report;
use crate::project::Project;

/// What a rate says while no task is accepted, since a rate with no denominator is not zero.
const NONE_YET: &str = "none yet, no task has been accepted";

/// The five harness metrics, as lines and as one JSON object; over `sprint_id`'s rows and costs
/// when there is one, the whole project otherwise.
///
/// # Errors
///
/// A sentence naming a sprint that is not there, or saying what the store refused, or which
/// contract could not be read.
pub fn metrics(project: &Project, sprint_id: Option<&str>) -> Result<Report, String> {
    let projections = project.projections()?;
    let metrics = match sprint_id {
        Some(id) => {
            project.files.read_sprint(id).map_err(|error| match error {
                FilesError::NotFound { .. } => format!("{id} is not in this project"),
                other => other.to_string(),
            })?;
            projections.metrics_for_sprint(&project.files, id)
        }
        None => projections.metrics(&project.files),
    }
    .map_err(|error| error.to_string())?;
    Ok(Report {
        lines: lines(&metrics),
        json: metrics_json(&metrics),
        json_lines: None,
    })
}

fn lines(metrics: &HarnessMetrics) -> Vec<String> {
    let percent = |rate: Option<f64>| rate.map(|rate| format!("{:.1}%", rate * 100.0));
    let mut lines = vec![
        format!("accepted tasks: {}", metrics.accepted_tasks),
        format!(
            "first-pass acceptance: {}",
            or_none(percent(metrics.first_pass_acceptance_rate))
        ),
        format!(
            "human interventions per accepted task: {}",
            or_none(
                metrics
                    .interventions_per_accepted_task
                    .map(|count| format!("{count:.2}"))
            )
        ),
        format!(
            "cost per accepted task: {}",
            or_none(
                metrics
                    .cost_per_accepted_task_usd
                    .as_ref()
                    .map(|split| dollars(split.total))
            )
        ),
    ];
    if let Some(split) = &metrics.cost_per_accepted_task_usd {
        lines.extend(
            split
                .by_purpose
                .iter()
                .map(|(purpose, usd)| format!("  {purpose}: {}", dollars(*usd))),
        );
    }
    lines.push(format!(
        "criteria verified by command, test, or artifact: {}",
        or_none(percent(metrics.mechanically_verified_criteria_share))
    ));
    lines.push(format!("active weeks: {}", metrics.active_weeks));
    lines
}

fn or_none(value: Option<String>) -> String {
    value.unwrap_or_else(|| NONE_YET.to_string())
}

fn dollars(usd: f64) -> String {
    format!("${usd:.2}")
}

/// The metrics as the wire says them: `snake_case` keys, and `null` for a rate with no accepted
/// task. The command line's one mapping of `HarnessMetrics`, which derives no serde.
fn metrics_json(metrics: &HarnessMetrics) -> Value {
    json!({
        "accepted_tasks": metrics.accepted_tasks,
        "first_pass_acceptance_rate": metrics.first_pass_acceptance_rate,
        "interventions_per_accepted_task": metrics.interventions_per_accepted_task,
        "cost_per_accepted_task_usd": metrics.cost_per_accepted_task_usd.as_ref().map(split_json),
        "mechanically_verified_criteria_share": metrics.mechanically_verified_criteria_share,
        "active_weeks": metrics.active_weeks,
    })
}

fn split_json(split: &CostSplit) -> Value {
    let by_purpose: Map<String, Value> = split
        .by_purpose
        .iter()
        .map(|(purpose, usd)| (purpose.to_string(), json!(usd)))
        .collect();
    json!({ "total": split.total, "by_purpose": by_purpose })
}
