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

/// Types generated from `docs/schemas/kit.schema.json`; a kit file is validated against the schema
/// before it is deserialised into these.
#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod kit {
    typify::import_types!(
        schema = "../../docs/schemas/kit.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}

/// Types generated from `docs/schemas/approved-sites.schema.json`; Farik's list of approved sites
/// is validated against the schema before it is deserialised into these.
#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod approved_sites {
    typify::import_types!(
        schema = "../../docs/schemas/approved-sites.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}
