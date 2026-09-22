//! Triage, contract writing, and filing tasks: the tools that decide what a task is.
#![expect(
    dead_code,
    reason = "the handlers arrive with the later tasks of phase 3 step 05"
)]

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};

/// How big a request is (5.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Size {
    /// An epic, broken down into tasks.
    Large,
    /// One task.
    Small,
}

/// `farik_triage_request`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriageInput {
    /// `large` for an epic, `small` for a task.
    size: Size,
    /// Why, in a sentence the log keeps.
    reason: String,
}

/// A criterion from the library, and the id it takes in the contract.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CriterionRef {
    /// The id in the contract, `C<n>`.
    id: String,
    /// The criterion's name in the library.
    name: String,
}

/// `farik_write_contract`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteContractInput {
    /// Top-level fields of the contract, each replacing the current value.
    #[serde(default)]
    fields: Map<String, Value>,
    /// Criteria from the library, appended to the exit criteria.
    #[serde(default)]
    criteria: Vec<CriterionRef>,
}

/// `farik_create_task`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTaskInput {
    /// The contract as its author writes it: no id, status, stamps, kind, or parent.
    contract: Map<String, Value>,
    /// The epic this is a task of, when it is one.
    parent: Option<String>,
}
