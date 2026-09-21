//! Repository tasks: the check command and hooks. Run with `cargo xtask`.

/// Which tests `cargo xtask check` runs.
pub mod check;
/// Commit message rules from `docs/standards/code.md`.
pub mod commit_message;
/// The rule that `farik-core` performs no I/O, hard rule 5 in `CLAUDE.md`.
pub mod core_io;
/// The bare `TODO` rule from `docs/standards/code.md`.
pub mod todos;
