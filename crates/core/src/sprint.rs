//! The sprint (`docs/SPEC.md` sections 3, 5.2, 5.3, 5.5, 5.11, 8.4 and 8.5):
//! `docs/schemas/sprint.schema.json` as Rust types and the validator that turns an untrusted
//! JSON value into one.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::pointer;

pub use crate::contract::ValidationError;
pub use crate::generated::sprint::{FarikSprint as Sprint, FarikSprintStatus as SprintStatus};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/sprint.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded sprint schema is valid JSON: it is the file in docs/schemas/ that typify \
         generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded sprint schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Checks a value against `docs/schemas/sprint.schema.json` and, when it conforms, returns the
/// typed sprint.
///
/// # Errors
///
/// Every schema violation, each at its own JSON pointer; or, when the schema passes and the typed
/// sprint cannot be built, one error at the root.
pub fn validate_sprint(input: &Value) -> Result<Sprint, Vec<ValidationError>> {
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
    serde_json::from_value::<Sprint>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!("the schema passed but the typed sprint could not be built: {error}"),
        }]
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::an_open_sprint_wire;
    use super::{Sprint, validate_sprint};

    fn sprint(wire: &Value) -> Sprint {
        validate_sprint(wire).expect("the fixture is a sprint")
    }

    fn refusals(wire: &Value) -> Vec<(String, String)> {
        validate_sprint(wire)
            .expect_err("this wire sprint is refused")
            .into_iter()
            .map(|error| (error.path, error.message))
            .collect()
    }

    fn paths(wire: &Value) -> Vec<String> {
        refusals(wire).into_iter().map(|(path, _)| path).collect()
    }

    #[test]
    fn validates_a_sprint() {
        let sprint = sprint(&an_open_sprint_wire());
        assert_eq!(sprint.id.as_str(), "S1");
        assert!(sprint.ended_at.is_none());
        assert!(sprint.budget_usd.is_none());

        let mut zero_budget = an_open_sprint_wire();
        zero_budget["budget_usd"] = json!(0);
        assert_eq!(paths(&zero_budget), ["/budget_usd"]);

        let mut bad_id = an_open_sprint_wire();
        bad_id["id"] = json!("S0");
        assert_eq!(paths(&bad_id), ["/id"]);

        let mut unknown_key = an_open_sprint_wire();
        unknown_key["mascot"] = json!("a penguin");
        assert_eq!(paths(&unknown_key), ["/"]);
    }
}
