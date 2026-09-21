//! `farik board`: the lifecycle, one line per task (F3).

use serde_json::json;

use crate::Report;
use crate::project::Project;

/// Every task the log knows about, in the order the board keeps them.
///
/// The board is read from the projections rather than from the contract files, because the log is
/// what decides where a task is (`docs/SPEC.md` section 8.4); `farik doctor` is what says when the
/// two disagree.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn board(project: &Project) -> Result<Report, String> {
    let rows = project
        .projections()?
        .board()
        .map_err(|error| error.to_string())?;
    if rows.is_empty() {
        return Ok(Report {
            lines: vec![
                "no tasks yet: farik task create files one from a YAML contract".to_string(),
            ],
            json: json!({ "tasks": [] }),
            json_lines: None,
        });
    }
    let lines = rows
        .iter()
        .map(|row| {
            let mut flags = Vec::new();
            if !row.triaged {
                flags.push("not triaged");
            }
            if row.locked {
                flags.push("yours");
            }
            let flags = if flags.is_empty() {
                String::new()
            } else {
                format!("  ({})", flags.join(", "))
            };
            format!(
                "{:<9} {:<5} {:<11} {:<6} {}{flags}",
                row.task_id.as_str(),
                row.kind.to_string(),
                row.status.to_string(),
                row.risk.to_string(),
                row.title
            )
        })
        .collect();
    let tasks: Vec<_> = rows
        .iter()
        .map(|row| {
            json!({
                "task_id": row.task_id.as_str(),
                "kind": row.kind.to_string(),
                "parent": row.parent.as_ref().map(|parent| parent.as_str().to_string()),
                "title": row.title,
                "status": row.status.to_string(),
                "risk": row.risk.to_string(),
                "triaged": row.triaged,
                "locked": row.locked,
            })
        })
        .collect();
    Ok(Report {
        lines,
        json: json!({ "tasks": tasks }),
        json_lines: None,
    })
}
