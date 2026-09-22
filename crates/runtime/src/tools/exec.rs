//! `farik_exec`: the agent's shell, run in the task's sandbox.
#![expect(
    dead_code,
    reason = "the handlers arrive with the later tasks of phase 3 step 05"
)]

use schemars::JsonSchema;
use serde::Deserialize;

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
