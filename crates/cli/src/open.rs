//! `catervas open`: a new start link for a browser, from the process driving the project
//! (ADR 0079).

use std::time::Duration;

use serde_json::{Value, json};

use crate::Report;
use crate::daemon_client::{ClientError, exchange, read_daemon_file};
use crate::project::Project;
use crate::start::DAEMON_FILE;

/// How long the driver has to answer.
const TIMEOUT: Duration = Duration::from_secs(5);

/// What is said when nothing drives the project.
const NOT_RUNNING: &str = "Catervas is not running here. Start it with catervas serve.";

/// Asks the driver of `project` for a one-time code and answers the link a browser opens it with.
///
/// # Errors
///
/// A sentence: that nothing drives the project, or why the driver could not be asked.
pub fn open(project: &Project) -> Result<Report, String> {
    let daemon_file = project.root.join(DAEMON_FILE);
    let address = read_daemon_file(&daemon_file).map_err(|error| match error {
        ClientError::NoDaemon { .. } => NOT_RUNNING.to_string(),
        other @ ClientError::Failed { .. } => other.to_string(),
    })?;
    let answer = exchange(&daemon_file, "/connect/code", "{}", TIMEOUT)
        // A file a stopped process left behind names nothing that listens.
        .map_err(|error| format!("{NOT_RUNNING} ({error})"))?;
    let code = serde_json::from_str::<Value>(&answer)
        .ok()
        .and_then(|value| value["code"].as_str().map(str::to_string))
        .ok_or_else(|| format!("the driver answered no code: {answer}"))?;
    let link = format!("http://127.0.0.1:{}/connect#{code}", address.port);
    Ok(Report {
        lines: vec![format!("open {link} in your browser")],
        json: json!({ "link": link }),
        json_lines: None,
    })
}
