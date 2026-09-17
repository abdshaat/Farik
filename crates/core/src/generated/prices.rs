// Generated from docs/schemas/prices.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///The model price table Farik ships and the user may override in .farik/prices.json: what one million tokens cost, per model id, in US dollars. See docs/SPEC.md section 5.5.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FarikPriceTable {
    ///Prices keyed by the provider's model id, exactly as the runtime reports it in usage.
    pub prices: ::std::collections::HashMap<::std::string::String, ModelPrice>,
    ///The day the numbers were copied, ISO 8601.
    pub retrieved_at: ::chrono::naive::NaiveDate,
    ///The provider's published price page the numbers were copied from.
    pub source_url: ::std::string::String,
    ///The table's format version; 1 for this shape.
    pub version: ::std::num::NonZeroU64,
}
///`ModelPrice`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ModelPrice {
    ///Prompt cache hits and refreshes.
    pub cache_read_usd_per_mtok: f64,
    ///Prompt cache writes with the five-minute duration, the one the agent engine uses.
    pub cache_write_usd_per_mtok: f64,
    ///Base input tokens.
    pub input_usd_per_mtok: f64,
    ///Output tokens.
    pub output_usd_per_mtok: f64,
}
/// Error types.
pub mod error {
    /// Error from a `TryFrom` or `FromStr` implementation.
    pub struct ConversionError(::std::borrow::Cow<'static, str>);
    impl ::std::error::Error for ConversionError {}
    impl ::std::fmt::Display for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Display::fmt(&self.0, f)
        }
    }
    impl ::std::fmt::Debug for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Debug::fmt(&self.0, f)
        }
    }
    impl From<&'static str> for ConversionError {
        fn from(value: &'static str) -> Self {
            Self(value.into())
        }
    }
    impl From<String> for ConversionError {
        fn from(value: String) -> Self {
            Self(value.into())
        }
    }
}
