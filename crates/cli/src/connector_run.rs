//! `farik connector run` and `farik connector headers` (ADR 0030). A session's `mcp.json` names
//! them in place of a custom connector's command or headers, so that no key is written to a file:
//! each asks the daemon's `POST /connector/launch` for the session's server.
//!
//! Neither prints a key anywhere but where it goes: the launcher into the server's environment,
//! the helper onto its standard output, which Claude Code reads as the headers.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::time::Duration;

use farik_runtime::connectors::KEPT_ENV;
use serde_json::Value;

use crate::CliIo;
use crate::daemon_client::exchange;

/// How long connecting, sending, and waiting for the daemon may each take: within Claude Code's
/// ten seconds for a headers helper.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(8);

/// What the daemon answers for `server` of `session`: a refusal says why, and holds no key.
fn launch(daemon_file: &Path, session: &str, server: &str) -> Result<Value, String> {
    let body = serde_json::json!({ "session": session, "server": server }).to_string();
    let answer = exchange(daemon_file, "/connector/launch", &body, EXCHANGE_TIMEOUT)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&answer).map_err(|_| "the daemon's answer is not JSON".to_string())
}

/// Each string member of `value`'s object `field`.
fn strings(value: &Value, field: &str) -> Option<BTreeMap<String, String>> {
    value[field]
        .as_object()?
        .iter()
        .map(|(name, item)| Some((name.clone(), item.as_str()?.to_string())))
        .collect()
}

/// Starts the server with only its keys and `KEPT_ENV` from this process's environment, in place
/// of this process, so that Claude Code speaks to it directly. Answers 1, saying why, only when it
/// could not.
pub fn run(daemon_file: &Path, session: &str, server: &str, io: &mut CliIo<'_>) -> i32 {
    let started: Result<(), String> = launch(daemon_file, session, server).and_then(|answer| {
        let command = answer["command"].as_str();
        let args: Option<Vec<&str>> = answer["args"]
            .as_array()
            .and_then(|args| args.iter().map(Value::as_str).collect());
        let (Some(command), Some(args), Some(keys)) = (command, args, strings(&answer, "env"))
        else {
            return Err("the daemon's answer names no command".to_string());
        };
        let mut process = std::process::Command::new(command);
        process.args(args).env_clear();
        for name in KEPT_ENV {
            if let Some(value) = io.env.get(name) {
                process.env(name, value);
            }
        }
        process.envs(keys);
        // Only returns when the server could not be started.
        Err(format!(
            "{command} could not be started: {}",
            process.exec()
        ))
    });
    let Err(why) = started else {
        return 0;
    };
    let _ = writeln!(io.stderr, "farik connector run {server}: {why}");
    1
}

/// Prints the server's headers, filled with its keys, as one JSON object. Prints nothing on
/// standard output, says why on standard error, and answers 1 when it cannot, so that Claude Code
/// fails the connection rather than connecting without them.
pub fn headers(daemon_file: &Path, session: &str, server: &str, io: &mut CliIo<'_>) -> i32 {
    let filled = launch(daemon_file, session, server).and_then(|answer| {
        strings(&answer, "headers").ok_or_else(|| "the daemon's answer has no headers".to_string())
    });
    match filled {
        Ok(headers) => {
            let printed = serde_json::to_string(&headers).unwrap_or_default();
            i32::from(writeln!(io.stdout, "{printed}").is_err())
        }
        Err(why) => {
            let _ = writeln!(io.stderr, "farik connector headers {server}: {why}");
            1
        }
    }
}
