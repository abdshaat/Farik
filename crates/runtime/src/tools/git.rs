//! The git tools, on the task's worktree (ADR 0004).
#![expect(
    dead_code,
    reason = "the handlers arrive with the later tasks of phase 3 step 05"
)]

use schemars::JsonSchema;
use serde::Deserialize;

/// `farik_git_commit`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitInput {
    /// The commit message.
    message: String,
    /// The paths to commit, relative to the worktree.
    paths: Vec<String>,
}
