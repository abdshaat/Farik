//! Rust types generated from the JSON Schemas in `docs/schemas/`. Regenerate with
//! `cargo xtask generate`.
//!
//! The generated types do not enforce every schema rule: `body` here is an untagged enum that
//! answers "which body is this?" by shape, and the event's `kind` is what decides it. A value is
//! validated against the schema and then read by kind: `crate::event::event_from_value`.

pub mod event;
