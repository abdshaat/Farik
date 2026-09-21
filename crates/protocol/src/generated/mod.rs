//! Rust types generated at compile time by `typify` from the JSON Schemas in `docs/schemas/`.
//! Every schema is also embedded with `include_str!` elsewhere in the crate, so cargo rebuilds
//! (and regenerates) when one changes. Read the generated code with `cargo expand`.
//!
//! The generated types do not enforce every schema rule: `body` here is an untagged enum that
//! answers "which body is this?" by shape, and the event's `kind` is what decides it. A value is
//! validated against the schema and then read by kind: `crate::event::event_from_value`.

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod command {
    typify::import_types!(
        schema = "../../docs/schemas/command.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}

#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod event {
    typify::import_types!(
        schema = "../../docs/schemas/event.schema.json",
        struct_builder = false,
        derives = [PartialEq],
    );
}
