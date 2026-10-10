//! `catervas site list`: the sites the Procurement Specialist may read, and the requests that wait
//! (`docs/SPEC.md` 6.10, ADR 0039). Deciding, adding and removing are commands, sent as `catervas
//! tool approve` sends its own.

use catervas_runtime::tools::sites::site_list;
use serde_json::Value;

use crate::Report;
use crate::project::Project;

/// Catervas's sites with whether each is on and where it is turned off, the sites you allowed, and
/// the requests that wait with the number that answers each, and the same as `sites.list` answers
/// with `--json`.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn list(project: &Project) -> Result<Report, String> {
    let wire = site_list(&project.log).map_err(|error| error.to_string())?;
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_string();
    let day = |row: &Value| text(row, "at").chars().take(10).collect::<String>();
    let mut lines = vec!["Catervas's approved sites:".to_string()];
    let mut category = String::new();
    for row in wire["catervas"].as_array().into_iter().flatten() {
        if text(row, "category") != category {
            category = text(row, "category");
            lines.push(format!("  {}", category.replace('_', " ")));
        }
        let on = row["on"].as_bool().unwrap_or(false);
        lines.push(format!(
            "    {} {}  {}{}",
            if on { "on " } else { "off" },
            text(row, "host"),
            text(row, "shop"),
            if row.get("at").is_some() {
                format!(" (turned {} {})", if on { "on" } else { "off" }, day(row))
            } else {
                String::new()
            }
        ));
    }
    lines.push("Sites you allowed:".to_string());
    let owner = wire["owner"].as_array().cloned().unwrap_or_default();
    if owner.is_empty() {
        lines.push("  none".to_string());
    }
    for row in &owner {
        lines.push(format!(
            "  {} (allowed {}{})",
            text(row, "host"),
            day(row),
            row["request"]
                .as_u64()
                .map_or(String::new(), |request| format!(
                    ", answering request {request}"
                ))
        ));
    }
    lines.push("Waiting for you:".to_string());
    let waiting = wire["waiting"].as_array().cloned().unwrap_or_default();
    if waiting.is_empty() {
        lines.push("  nothing".to_string());
    }
    for row in &waiting {
        lines.push(format!(
            "  {} {}  {} on {} asks for {}: {}",
            row["request"].as_u64().unwrap_or_default(),
            text(row, "host"),
            text(row, "agent_id"),
            text(row, "task_id"),
            text(row, "url"),
            text(row, "why"),
        ));
    }
    Ok(Report {
        lines,
        json: wire,
        json_lines: None,
    })
}
