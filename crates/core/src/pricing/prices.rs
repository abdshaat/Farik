//! The price table Farik ships (`docs/SPEC.md` section 5.5), copied from the provider's published
//! table on the day named in `retrieved_at`. The user overrides it with `.farik/prices.json`.

use std::sync::LazyLock;

use super::{PriceTable, validate_price_table};

/// The shipped table as JSON, in the shape of `docs/schemas/prices.schema.json`.
pub const PRICES_JSON: &str = r#"{
  "version": 1,
  "source_url": "https://platform.claude.com/docs/en/about-claude/pricing",
  "retrieved_at": "2026-09-23",
  "prices": {
    "claude-fable-5-1": {
      "input_usd_per_mtok": 10.0,
      "output_usd_per_mtok": 50.0,
      "cache_write_usd_per_mtok": 12.5,
      "cache_read_usd_per_mtok": 0.25
    },
    "claude-fable-5": {
      "input_usd_per_mtok": 10.0,
      "output_usd_per_mtok": 50.0,
      "cache_write_usd_per_mtok": 12.5,
      "cache_read_usd_per_mtok": 1.0
    },
    "claude-opus-5-5": {
      "input_usd_per_mtok": 4.0,
      "output_usd_per_mtok": 20.0,
      "cache_write_usd_per_mtok": 5.0,
      "cache_read_usd_per_mtok": 0.2
    },
    "claude-opus-5": {
      "input_usd_per_mtok": 5.0,
      "output_usd_per_mtok": 25.0,
      "cache_write_usd_per_mtok": 6.25,
      "cache_read_usd_per_mtok": 0.5
    },
    "claude-opus-4-8": {
      "input_usd_per_mtok": 5.0,
      "output_usd_per_mtok": 25.0,
      "cache_write_usd_per_mtok": 6.25,
      "cache_read_usd_per_mtok": 0.5
    },
    "claude-opus-4-7": {
      "input_usd_per_mtok": 5.0,
      "output_usd_per_mtok": 25.0,
      "cache_write_usd_per_mtok": 6.25,
      "cache_read_usd_per_mtok": 0.5
    },
    "claude-opus-4-6": {
      "input_usd_per_mtok": 5.0,
      "output_usd_per_mtok": 25.0,
      "cache_write_usd_per_mtok": 6.25,
      "cache_read_usd_per_mtok": 0.5
    },
    "claude-opus-4-5": {
      "input_usd_per_mtok": 5.0,
      "output_usd_per_mtok": 25.0,
      "cache_write_usd_per_mtok": 6.25,
      "cache_read_usd_per_mtok": 0.5
    },
    "claude-sonnet-5": {
      "input_usd_per_mtok": 2.0,
      "output_usd_per_mtok": 10.0,
      "cache_write_usd_per_mtok": 2.5,
      "cache_read_usd_per_mtok": 0.2
    },
    "claude-sonnet-4-6": {
      "input_usd_per_mtok": 3.0,
      "output_usd_per_mtok": 15.0,
      "cache_write_usd_per_mtok": 3.75,
      "cache_read_usd_per_mtok": 0.3
    },
    "claude-sonnet-4-5": {
      "input_usd_per_mtok": 3.0,
      "output_usd_per_mtok": 15.0,
      "cache_write_usd_per_mtok": 3.75,
      "cache_read_usd_per_mtok": 0.3
    },
    "claude-haiku-4-5-20251001": {
      "input_usd_per_mtok": 1.0,
      "output_usd_per_mtok": 5.0,
      "cache_write_usd_per_mtok": 1.25,
      "cache_read_usd_per_mtok": 0.1
    },
    "claude-haiku-4-5": {
      "input_usd_per_mtok": 1.0,
      "output_usd_per_mtok": 5.0,
      "cache_write_usd_per_mtok": 1.25,
      "cache_read_usd_per_mtok": 0.1
    }
  }
}"#;

/// The shipped table, validated once.
pub static PRICE_TABLE: LazyLock<PriceTable> = LazyLock::new(|| {
    let value = serde_json::from_str(PRICES_JSON)
        .expect("the shipped price table is valid JSON: pricing::tests parses it");
    validate_price_table(&value)
        .expect("the shipped price table matches its schema: pricing::tests validates it")
});
