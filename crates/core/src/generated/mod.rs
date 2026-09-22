//! Rust types generated at compile time by `typify` from the JSON Schemas in `docs/schemas/`.
//! Every schema is also embedded with `include_str!` elsewhere in the crate, so cargo rebuilds
//! (and regenerates) when one changes. Read the generated code with `cargo expand`.
//!
//! The generated types do not enforce every schema rule (a `const` discriminator such as a
//! verification's `method` is a plain value here), so a value is validated against the schema
//! before it is deserialised: `crate::contract::validate_contract` for the task contract.

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod criteria {
    typify::import_types!(
        schema = "../../docs/schemas/criteria.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod prices {
    typify::import_types!(
        schema = "../../docs/schemas/prices.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod task_contract {
    typify::import_types!(
        schema = "../../docs/schemas/task-contract.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod team {
    typify::import_types!(
        schema = "../../docs/schemas/team.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}
