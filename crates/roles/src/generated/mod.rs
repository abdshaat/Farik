//! Rust types generated at compile time by `typify` from `docs/schemas/role.schema.json` (ADR
//! 0009). The schema is also embedded with `include_str!` in `lib.rs`, so cargo rebuilds (and
//! regenerates) when it changes. A role file is validated against the schema before it is
//! deserialised into these types.

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod role {
    typify::import_types!(
        schema = "../../docs/schemas/role.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}
