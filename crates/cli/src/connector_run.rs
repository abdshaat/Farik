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

use farik_runtime::connectors::{KEPT_ENV, own_program_for, program};
use serde_json::Value;

use crate::CliIo;
use crate::daemon_client::{exchange, read_daemon_file};

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

/// Starts the server in the folder the daemon names, with only its keys and `KEPT_ENV` from this
/// process's environment, in place of this process, so that Claude Code speaks to it directly. Answers 1, saying why, only when it
/// could not.
pub fn run(daemon_file: &Path, session: &str, server: &str, io: &mut CliIo<'_>) -> i32 {
    let started: Result<(), String> = launch(daemon_file, session, server).and_then(|answer| {
        let command = answer["command"].as_str();
        let args: Option<Vec<&str>> = answer["args"]
            .as_array()
            .and_then(|args| args.iter().map(Value::as_str).collect());
        let (Some(command), Some(args), Some(keys), Some(folder)) = (
            command,
            args,
            strings(&answer, "env"),
            answer["cwd"].as_str(),
        ) else {
            return Err("the daemon's answer names no command or folder".to_string());
        };
        let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
        let own =
            own_program_for(command, &owned, io.own_program.as_deref()).map_err(str::to_string)?;
        // A Farik connector that signs in calls the daemon this file names, with its ticket.
        let port = read_daemon_file(daemon_file)
            .map_err(|error| error.to_string())?
            .port;
        // Farik's own connector is this executable, never a `farik` the PATH finds (ADR 0038).
        let mut process = std::process::Command::new(program(command, &owned, &own));
        // The folder Farik keeps for the server, never the worktree Claude Code started this in.
        process.args(args).current_dir(folder).env_clear();
        process.envs(environment(&answer, keys, port, &io.env));
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

/// Serves Google Ads' shim (ADR 0038, ADR 0042) on standard input and output until its client
/// leaves: it lists its tools, and forwards a call to the daemon at `FARIK_CONNECTOR_URL` with the
/// ticket in `FARIK_CONNECTOR_TICKET`, both set by the launcher. With neither it still lists them,
/// and a call says Google Ads runs only inside a Farik session. Answers 1, saying why on standard
/// error, when it cannot run.
pub fn google_ads(io: &mut CliIo<'_>) -> i32 {
    let (url, ticket) = (
        io.env.get(CONNECTOR_URL).cloned(),
        io.env.get(CONNECTOR_TICKET).cloned(),
    );
    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| {
            runtime
                .block_on(farik_runtime::google_ads::serve_shim(
                    url.as_deref(),
                    ticket.as_deref(),
                ))
                .map_err(|error| error.to_string())
        });
    match served {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(io.stderr, "farik connector google-ads: {why}");
            1
        }
    }
}

/// Serves the OSV lookup server (ADR 0038) on standard input and output, at OSV's one address,
/// until its client leaves. Answers 1, saying why on standard error, when it cannot.
pub fn osv(io: &mut CliIo<'_>) -> i32 {
    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| {
            runtime
                .block_on(farik_runtime::osv::serve_stdio(farik_runtime::osv::OSV_API))
                .map_err(|error| error.to_string())
        });
    match served {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(io.stderr, "farik connector osv: {why}");
            1
        }
    }
}

/// Serves the exchange-rate server (ADR 0038) on standard input and output, at Frankfurter's one
/// address, until its client leaves. Answers 1, saying why on standard error, when it cannot.
pub fn fx(io: &mut CliIo<'_>) -> i32 {
    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| {
            runtime
                .block_on(farik_runtime::fx::serve_stdio(farik_runtime::fx::FX_API))
                .map_err(|error| error.to_string())
        });
    match served {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(io.stderr, "farik connector fx: {why}");
            1
        }
    }
}

/// Serves the safety-recalls server (ADR 0038) on standard input and output, at the CPSC's, NHTSA's
/// and vPIC's one address each, until its client leaves. Answers 1, saying why on standard error,
/// when it cannot.
pub fn recalls(io: &mut CliIo<'_>) -> i32 {
    use farik_runtime::recalls::{CPSC_API, NHTSA_API, VPIC_API, serve_stdio};

    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| {
            runtime
                .block_on(serve_stdio(CPSC_API, NHTSA_API, VPIC_API))
                .map_err(|error| error.to_string())
        });
    match served {
        Ok(()) => 0,
        Err(why) => {
            let _ = writeln!(io.stderr, "farik connector recalls: {why}");
            1
        }
    }
}

/// The variables a Farik connector's shim reads: where the daemon is, and the session's ticket.
pub const CONNECTOR_URL: &str = "FARIK_CONNECTOR_URL";
/// See [`CONNECTOR_URL`].
pub const CONNECTOR_TICKET: &str = "FARIK_CONNECTOR_TICKET";

/// What the server's process is given: the variables Farik keeps from its own environment, the
/// server's keys, and, for a Farik connector the daemon gave a ticket, the ticket and the
/// address of the daemon's route on `port`; nothing else.
fn environment(
    answer: &Value,
    keys: BTreeMap<String, String>,
    port: u16,
    env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut given: BTreeMap<String, String> = KEPT_ENV
        .iter()
        .filter_map(|name| Some(((*name).to_string(), env.get(*name)?.clone())))
        .collect();
    given.extend(keys);
    if let Some(ticket) = answer["ticket"].as_str() {
        given.insert(CONNECTOR_TICKET.to_string(), ticket.to_string());
        given.insert(
            CONNECTOR_URL.to_string(),
            format!("http://127.0.0.1:{port}/connector/call"),
        );
    }
    given
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{CONNECTOR_TICKET, CONNECTOR_URL, environment};

    fn session_env() -> BTreeMap<String, String> {
        [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/someone"),
            ("LANG", "C.UTF-8"),
            ("TMPDIR", "/tmp"),
            ("ANTHROPIC_API_KEY", "sk-ant-model-secret"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "oauth-model-secret"),
            ("FARIK_OTHER", "anything"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
    }

    #[test]
    fn the_launcher_gives_the_shim_its_ticket_and_url() {
        let answer = json!({ "command": "farik", "args": ["connector", "google-ads"], "env": {}, "cwd": "/x", "ticket": "ab12" });
        let given = environment(&answer, BTreeMap::new(), 4242, &session_env());
        // The four variables Farik keeps, and the two of the shim, and no others: not the model's
        // credential, not another of Farik's.
        assert_eq!(
            given,
            BTreeMap::from([
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("HOME".to_string(), "/home/someone".to_string()),
                ("LANG".to_string(), "C.UTF-8".to_string()),
                ("TMPDIR".to_string(), "/tmp".to_string()),
                (CONNECTOR_TICKET.to_string(), "ab12".to_string()),
                (
                    CONNECTOR_URL.to_string(),
                    "http://127.0.0.1:4242/connector/call".to_string()
                ),
            ])
        );
        // No ticket, as for a server that is not a Farik connector that signs in: a key and the
        // four, and neither variable.
        let plain = json!({ "command": "env", "args": [], "env": { "API_KEY": "k" }, "cwd": "/x" });
        let keys = BTreeMap::from([("API_KEY".to_string(), "k".to_string())]);
        let given = environment(&plain, keys, 4242, &session_env());
        assert_eq!(given.len(), 5, "{given:?}");
        assert!(!given.contains_key(CONNECTOR_TICKET) && !given.contains_key(CONNECTOR_URL));
        assert_eq!(given["API_KEY"], "k");
        // A variable this process lacks is not passed at all, and not as an empty one.
        let bare = BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string())]);
        let given = environment(&answer, BTreeMap::new(), 4242, &bare);
        assert_eq!(
            given.keys().map(String::as_str).collect::<Vec<_>>(),
            [CONNECTOR_TICKET, CONNECTOR_URL, "PATH"]
        );
        // A key cannot stand in for the ticket.
        let forged = BTreeMap::from([(CONNECTOR_TICKET.to_string(), "forged".to_string())]);
        let given = environment(&answer, forged, 4242, &session_env());
        assert_eq!(given[CONNECTOR_TICKET], "ab12");
    }
}
