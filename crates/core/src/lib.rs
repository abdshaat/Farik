//! Farik's harness: schemas, the task state machine, the governor, and the cost model.
//! This crate performs no I/O.

/// The one place a task's branch name is made.
pub mod branch;
/// Budgets and limits: session limits, the session ledger, and the budget check.
pub mod budget;
/// The task contract and its validator.
pub mod contract;
/// The criterion library and its validator.
pub mod criteria;
/// Types generated from `docs/schemas/`.
pub mod generated;
/// The governor: every rule of `docs/SPEC.md` section 5 as pure functions.
pub mod governor;
/// The price table and the cost of model usage.
pub mod pricing;
/// The sprint and its validator.
pub mod sprint;
/// The team, its rules, and its validator.
pub mod team;
/// Small shared pieces of English used in refusal messages.
mod text;
