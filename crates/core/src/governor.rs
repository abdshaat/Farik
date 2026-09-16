//! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
//! in. It never reads the world and never mutates; the runtime applies what it decides.

/// The Definition of Done of `docs/SPEC.md` section 5.4 as one function over a contract and
/// the evidence gathered for it.
pub mod done;
/// Iteration and escalation rules (`docs/SPEC.md` sections 5.2 and 5.7).
pub mod escalation;
/// The gate predicates of `docs/SPEC.md` section 5.2's table, with the contract-write rules of
/// 5.11 and the epic rules of 5.16.
pub mod gates;
/// Allowed and protected paths (`docs/SPEC.md` sections 5.4, 5.6, 5.12).
pub mod paths;
/// Permission tiers and the tool-call and command checks (`docs/SPEC.md` section 5.6, ADR 0004).
pub mod permissions;
/// The Definition of Ready of `docs/SPEC.md` section 5.3 as one function over a contract and a
/// context.
pub mod readiness;
/// The lifecycle's statuses and which of them are terminal.
pub mod task_status;
/// Team rules of `docs/SPEC.md` section 5.12 and their defaults.
pub mod team_rules;
/// Transition evaluation: the table, the actors, and every gate as one decision
/// (`docs/SPEC.md` section 5.2).
pub mod transition;
/// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
pub mod transition_table;
