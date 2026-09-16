//! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
//! in. It never reads the world and never mutates; the runtime applies what it decides.

/// The lifecycle's statuses and which of them are terminal.
pub mod task_status;
/// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
pub mod transition_table;
