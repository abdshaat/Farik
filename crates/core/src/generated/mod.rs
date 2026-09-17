//! Rust types generated from the JSON Schemas in `docs/schemas/`. Regenerate with `cargo xtask generate`.
//!
//! The generated types do not enforce every schema rule (a `const` discriminator such as a
//! verification's `method` is a plain value here), so a value is validated against the schema
//! before it is deserialised: `crate::contract::validate_contract` for the task contract.

pub mod prices;
pub mod task_contract;
