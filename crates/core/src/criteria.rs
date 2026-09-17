//! The criterion library (`docs/SPEC.md` section 5.13): `docs/schemas/criteria.schema.json` as
//! Rust types, its validator, and the expansion that turns a reference by name into a contract's
//! own exit criterion.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::{pointer, repeated_ids, with_integers_normalised};
use crate::text::listed;

pub use crate::contract::ValidationError;
pub use crate::generated::criteria::{
    CriterionTemplate, CriterionTemplateSource as CriterionSource,
    FarikCriteriaLibrary as CriteriaLibrary,
};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("generated/criteria.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded criteria schema is valid JSON: it is a copy of docs/schemas/ written by \
         cargo xtask generate and checked for freshness by cargo xtask check",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded criteria schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Checks a value against `docs/schemas/criteria.schema.json` and, when it conforms, returns the
/// typed library.
///
/// One rule is this function's rather than the schema's: a name names one criterion. JSON Schema
/// cannot say that of an array's items, and a library with one name twice would expand to whichever
/// of the two came first, which is not something a person should have to know.
///
/// # Errors
///
/// Every schema violation, each at its own JSON pointer; or one error naming every repeated name;
/// or, when the schema passes and the typed library cannot be built, one error at the root.
pub fn validate_criteria(input: &Value) -> Result<CriteriaLibrary, Vec<ValidationError>> {
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
    let library = serde_json::from_value::<CriteriaLibrary>(with_integers_normalised(input))
        .map_err(|error| {
            vec![ValidationError {
                path: "/".to_string(),
                message: format!(
                    "the schema passed but the typed criterion library could not be built: \
                         {error}"
                ),
            }]
        })?;
    let repeated = repeated_ids(
        library
            .criteria
            .iter()
            .map(|criterion| criterion.name.as_str()),
    );
    if repeated.is_empty() {
        Ok(library)
    } else {
        Err(vec![ValidationError {
            path: "/criteria".to_string(),
            message: format!(
                "a name names one criterion, and {} used more than once",
                listed("the name", "the names", &repeated)
            ),
        }])
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::{a_criteria_library_wire, an_empty_criteria_library_wire};
    use super::{CriteriaLibrary, CriterionSource, validate_criteria};

    fn library(wire: &Value) -> CriteriaLibrary {
        validate_criteria(wire).expect("the fixture is a library")
    }

    fn refusals(wire: &Value) -> Vec<(String, String)> {
        validate_criteria(wire)
            .expect_err("this wire library is refused")
            .into_iter()
            .map(|error| (error.path, error.message))
            .collect()
    }

    fn paths(wire: &Value) -> Vec<String> {
        refusals(wire).into_iter().map(|(path, _)| path).collect()
    }

    #[test]
    fn reads_a_library_with_a_criterion_of_every_method() {
        let library = library(&a_criteria_library_wire());
        assert_eq!(library.criteria.len(), 6);
        assert_eq!(library.criteria[0].name.as_str(), "cargo-check");
        assert_eq!(
            library.criteria[0].source,
            Some(CriterionSource::ProjectScan)
        );
        assert_eq!(
            library.criteria[2].source, None,
            "left out, and a refresh of the scan will not touch it"
        );
    }

    #[test]
    fn reads_a_library_with_nothing_in_it() {
        // What a project whose scan found no check command starts with. A file that must hold at
        // least one criterion would mean `farik init` could not write one.
        assert!(
            library(&an_empty_criteria_library_wire())
                .criteria
                .is_empty()
        );
    }

    #[test]
    fn refuses_a_name_that_is_not_a_slug() {
        for name in ["Cargo Check", "cargo_check", "-check", ""] {
            let mut wire = a_criteria_library_wire();
            wire["criteria"][0]["name"] = json!(name);
            assert_eq!(paths(&wire), ["/criteria/0/name"], "{name:?}");
        }
    }

    #[test]
    fn refuses_a_criterion_too_short_to_say_anything() {
        let mut wire = a_criteria_library_wire();
        wire["criteria"][0]["text"] = json!("passes");
        assert_eq!(paths(&wire), ["/criteria/0/text"]);
    }

    #[test]
    fn refuses_a_verification_method_that_is_not_one() {
        let mut wire = a_criteria_library_wire();
        wire["criteria"][0]["verification"] = json!({ "method": "vibes" });
        assert_eq!(paths(&wire), ["/criteria/0/verification"]);
    }

    #[test]
    fn names_every_name_used_twice() {
        let mut wire = a_criteria_library_wire();
        wire["criteria"][1]["name"] = json!("cargo-check");
        wire["criteria"][3]["name"] = json!("decision-recorded");
        let refusals = refusals(&wire);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert_eq!(refusals[0].0, "/criteria");
        assert_eq!(
            refusals[0].1,
            "a name names one criterion, and the names cargo-check, decision-recorded used more \
             than once"
        );
    }
}
