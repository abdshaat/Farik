//! `farik_exec`: the agent's shell, run in the task's sandbox.

use std::collections::BTreeMap;
use std::time::Duration;

use farik_core::governor::permissions::evaluate_command;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::exec::ExecError;

/// How long a command may run when the agent does not say, and the most it may ask for.
const DEFAULT_TIMEOUT_SECONDS: u64 = 600;
const MAX_TIMEOUT_SECONDS: u64 = 1800;

/// How much of each stream an agent is shown: enough to read a failure, little enough not to
/// flood its context.
const SHOWN_BYTES: usize = 65_536;

/// `farik_exec`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecInput {
    /// The command, run through `sh -c`.
    command: String,
    /// The directory, relative to the workspace; the workspace itself when absent.
    cwd: Option<String>,
    /// How long it may run, in seconds: 600 when absent, 1800 at most.
    timeout_seconds: Option<u64>,
}

/// Runs a command in the task's sandbox, after `evaluate_command` against the team's rules, which
/// refuses git in any segment or behind any wrapper (ADR 0004) and every forbidden pattern
/// (5.12). The environment is empty, the directory the workspace unless `cwd` says otherwise, and
/// the timeout 600 seconds unless asked, 1800 at most. Each stream is cut at 64 KiB.
pub(super) async fn exec(call: &Call<'_>, input: ExecInput) -> Result<Value, ToolError> {
    evaluate_command(&input.command, &call.team.rules()).map_err(Refusal::Command)?;
    let executor = call
        .context
        .executor
        .clone()
        .ok_or_else(|| ToolError::Failed {
            detail: "this session has no sandbox to run a command in".to_string(),
        })?;
    let timeout = Duration::from_secs(
        input
            .timeout_seconds
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
            .min(MAX_TIMEOUT_SECONDS),
    );
    let cwd = input.cwd.unwrap_or_default();
    let command = input.command;
    let ran = tokio::task::spawn_blocking(move || {
        executor.run(&command, &cwd, timeout, &BTreeMap::new())
    })
    .await
    .map_err(failed)?
    .map_err(|error| match error {
        ExecError::OutsideWorkspace { cwd } => Refusal::OutsideWorkspace { cwd }.into(),
        other => failed(other),
    })?;
    Ok(json!({
        "exit_code": ran.exit_code,
        "stdout": shown(ran.stdout),
        "stderr": shown(ran.stderr),
        "timed_out": ran.timed_out,
    }))
}

/// The first 64 KiB of a stream, cut on the last character boundary at or before it, and a note
/// saying so when it was cut.
fn shown(text: String) -> String {
    if text.len() <= SHOWN_BYTES {
        return text;
    }
    let mut end = SHOWN_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}[output cut at 64 KiB]", &text[..end])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::shown;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::{Value, json};

    use crate::exec::{ExecError, ExecResult, Executor};
    use crate::sandbox::host::HostSandbox;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    /// An executor that runs nothing and remembers what it was asked.
    #[derive(Default)]
    struct Recording(Mutex<Vec<(String, Duration)>>);

    impl Executor for Recording {
        fn run(
            &self,
            command: &str,
            _cwd: &str,
            timeout: Duration,
            _env: &BTreeMap<String, String>,
        ) -> Result<ExecResult, ExecError> {
            self.0
                .lock()
                .expect("no test panics holding it")
                .push((command.to_string(), timeout));
            Ok(ExecResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                truncated: false,
            })
        }
    }

    fn exec(
        project: &TestProject,
        executor: Arc<dyn Executor>,
        input: Value,
    ) -> Result<Value, ToolError> {
        project.call_with(executor, "dev-a", None, "farik_exec", input)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn runs_a_command_and_cuts_its_output_at_64_kib() {
        let project = TestProject::new("tools-exec-cut", &a_team_of_three(|_| {}));
        let sandbox = Arc::new(HostSandbox::new(project.repo.path.clone()));
        let ran = exec(
            &project,
            sandbox,
            json!({ "command": "head -c 100000 /dev/zero | tr '\\0' a; echo oops >&2; exit 3" }),
        )
        .expect("the command runs");
        let stdout = ran["stdout"].as_str().expect("text");
        assert_eq!(
            stdout,
            format!("{}[output cut at 64 KiB]", "a".repeat(65_536))
        );
        assert_eq!(ran["stderr"], "oops\n");
        assert_eq!(ran["exit_code"], 3);
        assert_eq!(ran["timed_out"], false);
    }

    #[test]
    fn cuts_on_a_character_boundary() {
        let text = format!("{}é", "a".repeat(65_535));
        assert_eq!(
            shown(text),
            format!("{}[output cut at 64 KiB]", "a".repeat(65_535))
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_git_as_a_command() {
        let project = TestProject::new("tools-exec-git", &a_team_of_three(|_| {}));
        let recording = Arc::new(Recording::default());
        for command in ["git status", "sudo git push"] {
            match exec(&project, recording.clone(), json!({ "command": command })) {
                Err(ToolError::Refused { reason }) => {
                    assert!(reason.starts_with("git_via_exec: "), "{reason}");
                    assert!(reason.contains("farik_git_status"), "{reason}");
                }
                other => panic!("{command}: expected a refusal, got {other:?}"),
            }
        }
        assert!(
            recording.0.lock().expect("unpoisoned").is_empty(),
            "nothing ran"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_forbidden_command() {
        let project = TestProject::new(
            "tools-exec-forbidden",
            &a_team_of_three(|wire| {
                wire["rules"] = json!({ "forbidden_commands": ["rm -rf \\*"] });
            }),
        );
        let recording = Arc::new(Recording::default());
        match exec(
            &project,
            recording.clone(),
            json!({ "command": "rm -rf *" }),
        ) {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with("command_forbidden: "), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(
            recording.0.lock().expect("unpoisoned").is_empty(),
            "nothing ran"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn caps_the_timeout() {
        let project = TestProject::new("tools-exec-timeout", &a_team_of_three(|_| {}));
        let recording = Arc::new(Recording::default());
        exec(
            &project,
            recording.clone(),
            json!({ "command": "true", "timeout_seconds": 99_999 }),
        )
        .expect("runs");
        exec(&project, recording.clone(), json!({ "command": "true" })).expect("runs");
        let asked = recording.0.lock().expect("unpoisoned").clone();
        assert_eq!(asked[0].1, Duration::from_mins(30));
        assert_eq!(asked[1].1, Duration::from_mins(10));
    }
}
