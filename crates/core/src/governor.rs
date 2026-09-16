//! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
//! in. It never reads the world and never mutates; the runtime applies what it decides.

/// Allowed and protected paths (`docs/SPEC.md` sections 5.4, 5.6, 5.12).
pub mod paths;
/// The Definition of Ready of `docs/SPEC.md` section 5.3 as one function over a contract and a
/// context.
pub mod readiness;
/// The lifecycle's statuses and which of them are terminal.
pub mod task_status;
/// Team rules of `docs/SPEC.md` section 5.12 and their defaults.
pub mod team_rules;
/// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
pub mod transition_table;
