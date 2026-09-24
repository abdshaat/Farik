//! `farik log`: the event log, filtered, and the export F11 asks for.

use std::str::FromStr;

use farik_protocol::event::{EVERY_KIND, EventKind, event_to_value};
use farik_store::EventQuery;
use serde_json::json;

use crate::Report;
use crate::project::Project;

/// The events this project has recorded, oldest first.
///
/// With `--json` this is one JSON object per line rather than one object for the run: F11 calls it
/// an export, and a log read a line at a time is what survives being large.
///
/// # Errors
///
/// A sentence saying the task id or the kind is not one, or what the store refused.
pub fn log(
    project: &Project,
    task_id: Option<&String>,
    kind: Option<&String>,
    limit: Option<usize>,
) -> Result<Report, String> {
    let task_id = match task_id {
        Some(text) => Some(crate::task(text)?),
        None => None,
    };
    let kinds = match kind {
        Some(text) => vec![EventKind::from_str(text).map_err(|_| {
            format!(
                "{text} is not an event kind. They are: {}",
                EVERY_KIND
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?],
        None => Vec::new(),
    };
    let events = project
        .log
        .read(&EventQuery {
            task_id,
            kinds,
            limit,
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;

    let lines = if events.is_empty() {
        vec!["no events: nothing has happened in this project yet".to_string()]
    } else {
        events
            .iter()
            .map(|event| {
                format!(
                    "{:>4} {} {:<18} {}",
                    event.envelope.seq,
                    event
                        .envelope
                        .recorded_at
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    event.body.kind().to_string(),
                    event
                        .envelope
                        .ids
                        .task_id
                        .as_ref()
                        .map_or("-", |id| id.as_str())
                )
            })
            .collect()
    };
    let exported: Vec<_> = events.iter().map(event_to_value).collect();
    Ok(Report {
        lines,
        json: json!({ "events": exported }),
        json_lines: Some(exported),
    })
}
