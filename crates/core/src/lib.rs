//! Catervas's harness: schemas, the task state machine, the governor, and the cost model.
//! This crate performs no I/O.

/// The one place a task's branch name is made.
pub mod branch;
/// Budgets and limits: session limits, the session ledger, and the budget check.
pub mod budget;
/// The task contract and its validator.
pub mod contract;
/// The criterion library and its validator.
pub mod criteria;
/// Each role's folder under `docs/catervas/`, its human documents and their agent twins.
pub mod folders;
/// Types generated from `docs/schemas/`.
pub mod generated;
/// The governor: every rule of `docs/SPEC.md` section 5 as pure functions.
pub mod governor;
/// The marketing plan the owner approves: its checks and which approved plan is active.
pub mod marketing;
/// A purchase order's lines and total.
pub mod order;
/// The rule that sends a data pipeline request to the owner.
pub mod pipeline;
/// The price table and the cost of model usage.
pub mod pricing;
/// The renewals a register of vendors says are coming up.
pub mod renewals;
/// A skill's hash.
pub mod skill;
/// The sprint and its validator.
pub mod sprint;
/// The team, its rules, and its validator.
pub mod team;
/// Small shared pieces of English used in refusal messages, and how a text is counted in tokens.
pub mod text;
