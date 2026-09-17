//! The criterion library (`docs/SPEC.md` section 5.13): `docs/schemas/criteria.schema.json` as
//! Rust types, its validator, and the expansion that turns a reference by name into a contract's
//! own exit criterion.

use std::fmt;
use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::{pointer, repeated_ids, with_integers_normalised};
use crate::text::listed;

pub use crate::contract::{ExitCriterion, ValidationError};
pub use crate::generated::criteria::{
    CriterionTemplate, CriterionTemplateSource as CriterionSource,
    CriterionTemplateVerification as TemplateVerification, FarikCriteriaLibrary as CriteriaLibrary,
};
use crate::generated::task_contract::{
    ExitCriterionId, ExitCriterionText, ExitCriterionVerification,
    ExitCriterionVerificationVariant0Expect,
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

/// Why a reference could not be expanded into a criterion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriteriaError {
    /// The library has no criterion of this name.
    UnknownCriterion {
        /// The name that was asked for.
        name: String,
    },
    /// The library's criterion is not one a contract will take. Today only an id reaches this: the
    /// caller names the id the criterion will carry, and a contract's ids are `C1`, `C2`, and so
    /// on. It also stands ready for the day the two schemas disagree about what a criterion is.
    Refused {
        /// The name of the criterion being expanded.
        name: String,
        /// What the contract's own shape refused, in its words.
        detail: String,
    },
}

impl fmt::Display for CriteriaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCriterion { name } => {
                write!(formatter, "the criterion library has no {name}")
            }
            Self::Refused { name, detail } => {
                write!(
                    formatter,
                    "the criterion {name} cannot go in a contract: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for CriteriaError {}

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

/// Expands references into a contract's exit criteria, in the order given.
///
/// Each reference is the id the criterion will carry in the contract and the name it has in the
/// library, in that order — `("C1", "cargo-check")` reads as the criterion it produces. A criterion
/// arrives with no `satisfies`: which requirements it provides evidence for is a fact about one
/// contract, not about the library.
///
/// # Errors
///
/// `UnknownCriterion` for the first name the library does not hold, or `Refused` when the id is not
/// one a contract accepts.
pub fn expand_criteria(
    refs: &[(String, String)],
    library: &CriteriaLibrary,
) -> Result<Vec<ExitCriterion>, CriteriaError> {
    refs.iter()
        .map(|(id, name)| {
            let template = library
                .criteria
                .iter()
                .find(|criterion| criterion.name.as_str() == name)
                .ok_or_else(|| CriteriaError::UnknownCriterion { name: name.clone() })?;
            Ok(ExitCriterion {
                id: ExitCriterionId::try_from(id.as_str()).map_err(|error| {
                    CriteriaError::Refused {
                        name: name.clone(),
                        detail: format!("{id} is not a criterion id a contract takes: {error}"),
                    }
                })?,
                satisfies: Vec::new(),
                text: ExitCriterionText::try_from(template.text.as_str()).map_err(|error| {
                    CriteriaError::Refused {
                        name: name.clone(),
                        detail: format!("its text is not one a contract takes: {error}"),
                    }
                })?,
                verification: verification_of(&template.verification),
            })
        })
        .collect()
}

/// A template's verification as a contract's.
///
/// The two are the same shape because they are the same JSON: `criteria.schema.json` carries a copy
/// of the contract schema's `verification`, and a test holds the two copies to being the same
/// value. This is the mapping between the two Rust types that copy produces.
fn verification_of(template: &TemplateVerification) -> ExitCriterionVerification {
    match template {
        TemplateVerification::Variant0 {
            command,
            expect,
            method,
        } => ExitCriterionVerification::Variant0 {
            command: command.clone(),
            expect: ExitCriterionVerificationVariant0Expect {
                exit_code: expect.exit_code,
                stdout_contains: expect.stdout_contains.clone(),
                stdout_not_contains: expect.stdout_not_contains.clone(),
            },
            method: method.clone(),
        },
        TemplateVerification::Variant1 {
            command,
            method,
            new_tests_required,
        } => ExitCriterionVerification::Variant1 {
            command: command.clone(),
            method: method.clone(),
            new_tests_required: *new_tests_required,
        },
        TemplateVerification::Variant2 {
            method,
            must_contain,
            path,
        } => ExitCriterionVerification::Variant2 {
            method: method.clone(),
            must_contain: must_contain.clone(),
            path: path.clone(),
        },
        TemplateVerification::Variant3 { method, rubric } => ExitCriterionVerification::Variant3 {
            method: method.clone(),
            rubric: rubric.clone(),
        },
        TemplateVerification::Variant4 { method, question } => {
            ExitCriterionVerification::Variant4 {
                method: method.clone(),
                question: question.clone(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::{a_criteria_library_wire, an_empty_criteria_library_wire};
    use super::{
        CriteriaError, CriteriaLibrary, CriterionSource, SCHEMA_JSON, expand_criteria,
        validate_criteria,
    };
    use crate::contract::{Verification, fixtures::a_contract_wire, validate_contract};

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

    /// Every criterion of the fixture, referenced in order as C1 to C5.
    fn every_reference() -> Vec<(String, String)> {
        [
            "cargo-check",
            "unit-tests",
            "decision-recorded",
            "reviewed-for-clarity",
            "human-accepted",
            "tests-pass",
        ]
        .iter()
        .enumerate()
        .map(|(index, name)| (format!("C{}", index + 1), (*name).to_string()))
        .collect()
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

    #[test]
    fn expands_a_reference_into_the_contract_s_own_criterion() {
        let library = library(&a_criteria_library_wire());
        let expanded = expand_criteria(&every_reference(), &library).expect("every name is known");
        assert_eq!(
            expanded
                .iter()
                .map(|criterion| criterion.id.to_string())
                .collect::<Vec<_>>(),
            ["C1", "C2", "C3", "C4", "C5", "C6"],
            "in the order they were asked for, with the ids the caller named"
        );
        assert_eq!(
            expanded[0].text.as_str(),
            "The repository's own check command passes.",
            "the library's words, carried in"
        );
        assert_eq!(
            expanded
                .iter()
                .map(|criterion| Verification::from(&criterion.verification))
                .collect::<Vec<_>>(),
            [
                Verification::Command {
                    command: "cargo xtask check".to_string(),
                    exit_code: 2,
                    stdout_contains: Some("xtask check: ok".to_string()),
                    stdout_not_contains: Some("warning".to_string()),
                },
                Verification::Test {
                    command: "cargo test --workspace".to_string(),
                    new_tests_required: true,
                },
                Verification::Artifact {
                    path: "docs/decisions".to_string(),
                    must_contain: vec!["Status: accepted".to_string()],
                },
                Verification::Review {
                    rubric: vec!["Does every public item say what it is for?".to_string()],
                },
                Verification::Human {
                    question: "Does this do what you asked for?".to_string(),
                },
                Verification::Test {
                    command: "cargo test --workspace".to_string(),
                    new_tests_required: false,
                },
            ],
            "every method, and both sides of every field the mapping carries"
        );
    }

    #[test]
    fn leaves_what_a_criterion_satisfies_to_the_contract() {
        // Which requirements a criterion provides evidence for is a fact about one contract, and
        // the library has never heard of that contract's requirements.
        let library = library(&a_criteria_library_wire());
        let expanded = expand_criteria(&[("C1".to_string(), "cargo-check".to_string())], &library)
            .expect("the name is known");
        assert!(expanded[0].satisfies.is_empty());
    }

    #[test]
    fn an_expanded_criterion_is_one_a_contract_takes() {
        // The point of the library: what comes out of it goes into a contract without further
        // work, and the contract's own validator is what says so.
        let library = library(&a_criteria_library_wire());
        let expanded = expand_criteria(&every_reference(), &library).expect("every name is known");
        let mut wire = a_contract_wire();
        wire["exit_criteria"] = Value::Array(
            expanded
                .iter()
                .map(|criterion| serde_json::to_value(criterion).expect("a criterion serialises"))
                .collect(),
        );
        let contract = validate_contract(&wire).expect("the contract holds");
        assert_eq!(contract.exit_criteria.len(), 6);
    }

    #[test]
    fn refuses_a_name_the_library_does_not_hold() {
        let library = library(&a_criteria_library_wire());
        assert_eq!(
            expand_criteria(
                &[
                    ("C1".to_string(), "cargo-check".to_string()),
                    ("C2".to_string(), "pnpm-check".to_string()),
                ],
                &library,
            ),
            Err(CriteriaError::UnknownCriterion {
                name: "pnpm-check".to_string(),
            })
        );
    }

    #[test]
    fn refuses_an_id_a_contract_would_not_take() {
        let library = library(&a_criteria_library_wire());
        let refused = expand_criteria(&[("C0".to_string(), "cargo-check".to_string())], &library);
        let Err(CriteriaError::Refused { name, detail }) = refused else {
            panic!("a contract's ids start at C1: {refused:?}");
        };
        assert_eq!(name, "cargo-check");
        assert!(
            detail.starts_with("C0 is not a criterion id a contract takes"),
            "{detail}"
        );
    }

    #[test]
    fn says_what_it_refused_and_why_in_plain_words() {
        let said: Vec<String> = [
            CriteriaError::UnknownCriterion {
                name: "pnpm-check".to_string(),
            },
            CriteriaError::Refused {
                name: "cargo-check".to_string(),
                detail: "C0 is not a criterion id a contract takes".to_string(),
            },
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
        assert_eq!(
            said,
            [
                "the criterion library has no pnpm-check",
                "the criterion cargo-check cannot go in a contract: C0 is not a criterion id a \
                 contract takes",
            ]
        );
    }

    #[test]
    fn the_two_schemas_say_the_same_thing_about_a_verification() {
        // `criteria.schema.json` carries a copy of the contract schema's `verification`, and
        // `verification_of` is the mapping between the two Rust types that copy produces. If the
        // two ever drift, that mapping quietly starts lying, so the copy is checked here.
        let contract: Value =
            serde_json::from_str(include_str!("generated/task_contract.schema.json"))
                .expect("the embedded contract schema is valid JSON");
        let library: Value =
            serde_json::from_str(SCHEMA_JSON).expect("the embedded criteria schema is valid JSON");
        let copy = &library["$defs"]["criterionTemplate"]["properties"]["verification"];
        assert!(
            copy.is_object(),
            "the criteria schema still keeps its verification where this test looks; two pointers \
             that both went stale would compare null to null and hold nothing"
        );
        assert_eq!(
            copy,
            &contract["$defs"]["exitCriterion"]["properties"]["verification"],
        );
    }
}
