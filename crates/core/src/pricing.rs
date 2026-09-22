//! The cost of model usage (`docs/SPEC.md` section 5.5): the price table's types and validator,
//! and the cost of one usage report at a model's prices.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

pub use crate::contract::ValidationError;
pub use crate::generated::prices::{FarikPriceTable as PriceTable, ModelPrice};

/// The price table Farik ships.
pub mod prices;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/prices.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded price schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded price schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// The one table format this program reads.
const KNOWN_VERSION: u64 = 1;

/// Checks a value against `docs/schemas/prices.schema.json` and, when it conforms, returns the
/// typed table. Refuses anything the schema refuses, with one error per violation, and refuses a
/// table whose `version` this program does not know, because a later format read as this one
/// would price a session by guesswork.
///
/// # Errors
///
/// Every schema violation, in the schema's order rather than the input's key order; one error at
/// `/version` for a format this program does not know; or, when the schema passes but the typed
/// table cannot be built, one error at the root.
pub fn validate_price_table(input: &Value) -> Result<PriceTable, Vec<ValidationError>> {
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
    let table = serde_json::from_value::<PriceTable>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!(
                "the schema passed but the typed price table could not be built: {error}"
            ),
        }]
    })?;
    if table.version.get() != KNOWN_VERSION {
        return Err(vec![ValidationError {
            path: "/version".to_string(),
            message: format!(
                "the table is written in format version {}, and this program reads version {KNOWN_VERSION}",
                table.version
            ),
        }]);
    }
    Ok(table)
}

fn pointer(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

/// The usage one model call reported, in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    /// Input tokens billed at the base input price.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Input tokens read from the prompt cache.
    pub cache_read_tokens: u64,
    /// Input tokens written to the prompt cache.
    pub cache_write_tokens: u64,
}

/// Why a cost cannot be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PricingError {
    /// The price table has no row for the model.
    UnknownModel {
        /// The model id as the usage reported it.
        model_id: String,
    },
}

/// The cost of a usage report in US dollars at a model's prices: each token count times its
/// price per million tokens.
///
/// # Errors
///
/// `UnknownModel` when the table has no row for `model_id`.
pub fn compute_cost_usd(
    usage: &Usage,
    model_id: &str,
    prices: &PriceTable,
) -> Result<f64, PricingError> {
    let price = prices
        .prices
        .get(model_id)
        .ok_or_else(|| PricingError::UnknownModel {
            model_id: model_id.to_string(),
        })?;
    Ok(per_million(usage.input_tokens, price.input_usd_per_mtok)
        + per_million(usage.output_tokens, price.output_usd_per_mtok)
        + per_million(usage.cache_read_tokens, price.cache_read_usd_per_mtok)
        + per_million(usage.cache_write_tokens, price.cache_write_usd_per_mtok))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a token count is far below 2^53, where an f64 stops being exact"
)]
fn per_million(tokens: u64, usd_per_mtok: f64) -> f64 {
    tokens as f64 * usd_per_mtok / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::prices::{PRICE_TABLE, PRICES_JSON};
    use super::{PricingError, Usage, compute_cost_usd, validate_price_table};

    fn shipped() -> Value {
        serde_json::from_str(PRICES_JSON).expect("the shipped table is valid JSON")
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn ships_a_table_that_matches_its_schema_and_prices_every_model_a_team_can_call() {
        let table = validate_price_table(&shipped()).expect("valid");
        assert_eq!(table.version.get(), 1);
        assert_eq!(
            table.source_url,
            "https://platform.claude.com/docs/en/about-claude/pricing"
        );
        assert_eq!(table.prices.len(), 12);
        let spot = [
            ("claude-fable-5-1", 10.0, 50.0, 12.5, 0.25),
            ("claude-fable-5", 10.0, 50.0, 12.5, 1.0),
            ("claude-opus-5", 5.0, 25.0, 6.25, 0.5),
            ("claude-opus-4-8", 5.0, 25.0, 6.25, 0.5),
            ("claude-opus-4-7", 5.0, 25.0, 6.25, 0.5),
            ("claude-opus-4-6", 5.0, 25.0, 6.25, 0.5),
            ("claude-opus-4-5", 5.0, 25.0, 6.25, 0.5),
            ("claude-sonnet-5", 2.0, 10.0, 2.5, 0.2),
            ("claude-sonnet-4-6", 3.0, 15.0, 3.75, 0.3),
            ("claude-sonnet-4-5", 3.0, 15.0, 3.75, 0.3),
            ("claude-haiku-4-5", 1.0, 5.0, 1.25, 0.1),
            ("claude-haiku-4-5-20251001", 1.0, 5.0, 1.25, 0.1),
        ];
        for (model, input, output, cache_write, cache_read) in spot {
            let price = PRICE_TABLE.prices.get(model).expect(model);
            assert!(close(price.input_usd_per_mtok, input), "{model}");
            assert!(close(price.output_usd_per_mtok, output), "{model}");
            assert!(
                close(price.cache_write_usd_per_mtok, cache_write),
                "{model}"
            );
            assert!(close(price.cache_read_usd_per_mtok, cache_read), "{model}");
        }
    }

    #[test]
    fn refuses_a_table_written_in_a_format_this_program_does_not_know() {
        let mut input = shipped();
        input["version"] = json!(2);
        let errors = validate_price_table(&input).expect_err("refused");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/version");
        assert!(
            errors[0].message.contains("version 1"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_a_typed_failure_after_a_schema_pass_at_the_root() {
        let mut input = shipped();
        input["version"] = json!(1.0);
        let errors = validate_price_table(&input).expect_err("refused");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
        assert!(
            errors[0]
                .message
                .starts_with("the schema passed but the typed price table could not be built"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn refuses_a_negative_price_and_an_unknown_field_at_their_paths() {
        let mut input = shipped();
        input["prices"]["claude-opus-5"]["input_usd_per_mtok"] = json!(-1);
        let errors = validate_price_table(&input).expect_err("refused");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/prices/claude-opus-5/input_usd_per_mtok");
        let mut input = shipped();
        input["currency"] = json!("USD");
        let errors = validate_price_table(&input).expect_err("refused");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }

    #[test]
    fn refuses_a_table_without_prices() {
        let input = json!({"version": 1, "source_url": "https://example.com/p", "retrieved_at": "2026-09-16", "prices": {}});
        let errors = validate_price_table(&input).expect_err("refused");
        assert_eq!(errors[0].path, "/prices");
    }

    #[test]
    fn computes_the_provider_worked_example_for_opus_5() {
        let usage = Usage {
            input_tokens: 50_000,
            output_tokens: 15_000,
            ..Usage::default()
        };
        let cost = compute_cost_usd(&usage, "claude-opus-5", &PRICE_TABLE).expect("priced");
        assert!(close(cost, 0.625), "{cost}");
    }

    #[test]
    fn prices_cache_reads_and_writes_at_their_own_rates() {
        let usage = Usage {
            input_tokens: 10_000,
            output_tokens: 0,
            cache_read_tokens: 40_000,
            cache_write_tokens: 1_000_000,
        };
        let cost = compute_cost_usd(&usage, "claude-opus-5", &PRICE_TABLE).expect("priced");
        assert!(close(cost, 0.05 + 0.02 + 6.25), "{cost}");
    }

    #[test]
    fn costs_nothing_when_nothing_was_used() {
        assert!(close(
            compute_cost_usd(&Usage::default(), "claude-sonnet-5", &PRICE_TABLE).expect("priced"),
            0.0
        ));
    }

    #[test]
    fn refuses_a_model_the_table_does_not_price() {
        assert_eq!(
            compute_cost_usd(&Usage::default(), "claude-2", &PRICE_TABLE),
            Err(PricingError::UnknownModel {
                model_id: "claude-2".to_string()
            })
        );
    }
}
