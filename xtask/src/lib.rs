//! Repository tasks: the check command, hooks, and code generation. Run with `cargo xtask`.

/// Commit message rules from `docs/standards/code.md`.
pub mod commit_message;
/// The rule that `farik-core` performs no I/O, hard rule 5 in `CLAUDE.md`.
pub mod core_io;
/// Rust types generated from the JSON Schemas in `docs/schemas/`.
pub mod generate;
/// The bare `TODO` rule from `docs/standards/code.md`.
pub mod todos;
