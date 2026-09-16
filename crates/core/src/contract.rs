//! The task contract: `docs/schemas/task-contract.schema.json` as Rust types, and the validator
//! that turns an untrusted JSON value into one.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

pub use crate::generated::task_contract::{
    ExitCriterion, ExitCriterionVerification as VerificationWire,
    FarikTaskContract as TaskContract, FarikTaskContractBudget as Budget,
    FarikTaskContractId as TaskId, FarikTaskContractNotes as Notes,
    FarikTaskContractRequirementsItem as Requirement, FarikTaskContractRisk as Risk,
    FarikTaskContractStatus as TaskStatus, Role,
};

/// A criterion's verification method with named variants. The generated wire enum names its
/// variants by position; this is the one mapping `farik-core` keeps at its edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    /// Run a command; pass on the expected exit code and output.
    Command {
        /// Run inside the sandbox from the project root.
        command: String,
        /// The exit code that counts as a pass; the schema's default is 0.
        exit_code: i64,
        /// Text the standard output must contain.
        stdout_contains: Option<String>,
        /// Text the standard output must not contain.
        stdout_not_contains: Option<String>,
    },
    /// Run a test command; pass on exit code 0.
    Test {
        /// The test command.
        command: String,
        /// Whether the reviewer must also see a new test that fails on the base branch.
        new_tests_required: bool,
    },
    /// A file must exist after the task.
    Artifact {
        /// The path, relative to the project root.
        path: String,
        /// Strings the file must contain.
        must_contain: Vec<String>,
    },
    /// Yes or no questions the reviewer answers with a cited reason each.
    Review {
        /// The questions.
        rubric: Vec<String>,
    },
    /// Satisfied only by a `human.accepted` event.
    Human {
        /// What the human is asked to confirm.
        question: String,
    },
}

impl From<&VerificationWire> for Verification {
    fn from(wire: &VerificationWire) -> Self {
        match wire {
            VerificationWire::Variant0 {
                command, expect, ..
            } => Self::Command {
                command: command.clone(),
                exit_code: expect.exit_code,
                stdout_contains: expect.stdout_contains.clone(),
                stdout_not_contains: expect.stdout_not_contains.clone(),
            },
            VerificationWire::Variant1 {
                command,
                new_tests_required,
                ..
            } => Self::Test {
                command: command.clone(),
                new_tests_required: *new_tests_required,
            },
            VerificationWire::Variant2 {
                must_contain, path, ..
            } => Self::Artifact {
                path: path.clone(),
                must_contain: must_contain.clone(),
            },
            VerificationWire::Variant3 { rubric, .. } => Self::Review {
                rubric: rubric.clone(),
            },
            VerificationWire::Variant4 { question, .. } => Self::Human {
                question: question.clone(),
            },
        }
    }
}

impl Verification {
    /// The wire name of the method: `command`, `test`, `artifact`, `review`, or `human`.
    #[must_use]
    pub fn method(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Test { .. } => "test",
            Self::Artifact { .. } => "artifact",
            Self::Review { .. } => "review",
            Self::Human { .. } => "human",
        }
    }
}

/// Builders for test contracts, usable by every crate's tests.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("generated/task_contract.schema.json");

/// One way in which a value failed the contract schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSON pointer into the input; `/` for the root.
    pub path: String,
    /// The schema's own message.
    pub message: String,
}

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value =
        serde_json::from_str(SCHEMA_JSON).expect("the embedded contract schema is valid JSON");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the embedded contract schema compiles")
});

/// Checks a value against `docs/schemas/task-contract.schema.json` and, when it conforms, returns
/// the typed contract with the schema's defaults applied. Refuses anything the schema refuses,
/// with one error per violation. Does not check Definition of Ready rules.
///
/// # Errors
///
/// Every schema violation, in document order.
pub fn validate_contract(input: &Value) -> Result<TaskContract, Vec<ValidationError>> {
    let errors: Vec<ValidationError> = VALIDATOR
        .iter_errors(input)
        .map(|error| ValidationError {
            path: pointer(&error.instance_path().to_string()),
            message: error.to_string(),
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    serde_json::from_value::<TaskContract>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: error.to_string(),
        }]
    })
}

fn pointer(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{a_contract_wire, a_full_contract_wire};
    use super::{ValidationError, Verification, validate_contract};

    fn refusal(input: &serde_json::Value) -> Vec<ValidationError> {
        validate_contract(input).expect_err("expected a refusal")
    }

    #[test]
    fn accepts_a_schema_valid_contract_and_applies_the_defaults() {
        let contract = validate_contract(&a_contract_wire()).expect("valid");
        assert_eq!(contract.id.to_string(), "FRK-1");
        assert_eq!(contract.scope.out_of_scope, vec!["password reset"]);
        assert_eq!(contract.budget.max_sessions.get(), 5);
        assert_eq!(contract.budget.max_iterations.get(), 3);
        assert_eq!(contract.iteration, 0);
        assert!(!contract.locked);
        assert_eq!(contract.kind.to_string(), "task");
        assert!(contract.parent.is_none());
        assert_eq!(
            Verification::from(&contract.exit_criteria[0].verification),
            Verification::Test {
                command: "pnpm test login".to_string(),
                new_tests_required: false
            }
        );
    }

    #[test]
    fn refuses_a_value_that_is_not_an_object() {
        let errors = refusal(&json!("not a contract"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }

    #[test]
    fn refuses_a_task_id_that_does_not_match_the_pattern() {
        let mut input = a_contract_wire();
        input["id"] = json!("TASK-1");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/id");
    }

    #[test]
    fn refuses_an_empty_out_of_scope_list() {
        let mut input = a_contract_wire();
        input["scope"]["out_of_scope"] = json!([]);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/scope/out_of_scope");
    }

    #[test]
    fn refuses_a_command_criterion_without_an_expect_block() {
        let mut input = a_contract_wire();
        input["exit_criteria"][0]["verification"] =
            json!({"method": "command", "command": "pnpm check"});
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/exit_criteria/0/verification");
    }

    #[test]
    fn refuses_an_unknown_top_level_property() {
        let mut input = a_contract_wire();
        input["owner"] = json!("someone");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }

    #[test]
    fn refuses_a_reference_that_is_not_a_uri() {
        let mut input = a_contract_wire();
        input["references"] = json!(["not a uri"]);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/references/0");
    }

    #[test]
    fn accepts_every_verification_method_and_every_optional_field() {
        let contract = validate_contract(&a_full_contract_wire()).expect("valid");
        let methods: Vec<&str> = contract
            .exit_criteria
            .iter()
            .map(|criterion| Verification::from(&criterion.verification).method())
            .collect();
        assert_eq!(methods, ["command", "test", "artifact", "review", "human"]);
        assert_eq!(
            Verification::from(&contract.exit_criteria[0].verification),
            Verification::Command {
                command: "pnpm check".to_string(),
                exit_code: 0,
                stdout_contains: Some("passed".to_string()),
                stdout_not_contains: Some("failed".to_string())
            }
        );
        assert_eq!(contract.dependencies.len(), 1);
        assert_eq!(
            contract.references,
            vec!["https://github.com/abdshaat/farik/issues/1"]
        );
        assert!(contract.locked);
        assert_eq!(
            serde_json::to_value(&contract.parent).unwrap(),
            json!("FRK-3")
        );
        assert_eq!(
            contract
                .notes
                .as_ref()
                .and_then(|notes| notes.review.clone())
                .as_deref(),
            Some("C1 passed: see output.")
        );
    }

    #[test]
    fn serializes_back_to_the_wire_shape_with_defaults_written_explicitly() {
        let contract = validate_contract(&a_contract_wire()).expect("valid");
        let wire = serde_json::to_value(&contract).expect("serializes");
        assert_eq!(
            wire["budget"],
            json!({"max_cost_usd": 5.0, "max_sessions": 5, "max_iterations": 3})
        );
        assert_eq!(wire["iteration"], json!(0));
        assert_eq!(wire["locked"], json!(false));
        assert!(validate_contract(&wire).is_ok());
    }

    #[test]
    fn round_trips_a_contract_that_has_every_field() {
        let input = a_full_contract_wire();
        let contract = validate_contract(&input).expect("valid");
        let wire = serde_json::to_value(&contract).expect("serializes");
        assert_eq!(wire, input);
    }
}
