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
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded contract schema is valid JSON: it is a copy of docs/schemas/ written by \
         cargo xtask generate and checked for freshness by cargo xtask check",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded contract schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Checks a value against `docs/schemas/task-contract.schema.json` and, when it conforms, returns
/// the typed contract with the schema's defaults applied. Refuses anything the schema refuses,
/// with one error per violation. Does not check Definition of Ready rules.
///
/// An integer written with a zero fraction (`0.0`) counts as an integer, as it does for the
/// schema. Timestamps are normalised to UTC with at most nine fractional digits, so a contract
/// written back is not always byte-identical to the one read.
///
/// # Errors
///
/// Every schema violation, in the schema's order rather than the input's key order; or, when the
/// schema passes but the typed contract cannot be built, one error at the root.
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
    serde_json::from_value::<TaskContract>(with_integers_normalised(input)).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!(
                "the schema passed but the typed contract could not be built: {error}"
            ),
        }]
    })
}

/// JSON Schema counts a number with a zero fraction as an integer and serde does not; such
/// numbers are rewritten as integers, where they fit in an `i64`, so that the two agree.
fn with_integers_normalised(value: &Value) -> Value {
    match value {
        Value::Number(number) => {
            Value::Number(as_integer(number).unwrap_or_else(|| number.clone()))
        }
        Value::Array(items) => Value::Array(items.iter().map(with_integers_normalised).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, field)| (key.clone(), with_integers_normalised(field)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn as_integer(number: &serde_json::Number) -> Option<serde_json::Number> {
    let float = number.as_f64().filter(|_| number.is_f64())?;
    if !float.is_finite() || float.fract() != 0.0 {
        return None;
    }
    format!("{float:.0}")
        .parse::<i64>()
        .ok()
        .map(serde_json::Number::from)
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
    fn accepts_an_integer_written_with_a_zero_fraction() {
        let mut input = a_full_contract_wire();
        input["iteration"] = json!(0.0);
        input["exit_criteria"][0]["verification"]["expect"]["exit_code"] = json!(0.0);
        let contract = validate_contract(&input).expect("valid");
        assert_eq!(contract.iteration, 0);
        assert_eq!(
            Verification::from(&contract.exit_criteria[0].verification).method(),
            "command"
        );
    }

    #[test]
    fn reports_a_typed_failure_after_a_schema_pass_at_the_root() {
        let mut input = a_contract_wire();
        input["iteration"] = json!(2e19);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
        assert!(
            errors[0]
                .message
                .starts_with("the schema passed but the typed contract could not be built"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn round_trips_a_contract_that_has_every_field() {
        let input = a_full_contract_wire();
        let contract = validate_contract(&input).expect("valid");
        let wire = serde_json::to_value(&contract).expect("serializes");
        assert_eq!(wire, input);
    }
}
