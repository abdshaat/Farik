//! Catervas's harness: schemas, the task state machine, the governor, and the cost model.
//! This crate performs no I/O.

/// What the team may decide for the owner under autopilot (ADR 0082).
pub mod autopilot;
/// The one place a task's branch name is made.
pub mod branch;
/// Budgets and limits: session limits, the session ledger, and the budget check.
pub mod budget;
/// What Catervas reads of Claude Code's account, from strings (ADR 0084).
pub mod claude_account;
/// What claude.ai's data export says about conversations and Projects, metadata only.
pub mod claude_export;
/// What Claude Code's history says: one transcript line, and where each session belongs.
pub mod claude_history;
/// Which services the owner reaches through Claude's connectors: names from settings, tool calls, the catalog.
pub mod claude_services;
/// The task contract and its validator.
pub mod contract;
/// The criterion library and its validator.
pub mod criteria;
/// When a delivery closes (ADR 0062).
pub mod delivery;
/// What Catervas suggests for each project it found, and where its row sits.
pub mod finding;
/// Each role's folder under `docs/catervas/`, its human documents and their agent twins.
pub mod folders;
/// What leaves for the forge, with no task id in it (ADR 0066).
pub mod forge_text;
/// Types generated from `docs/schemas/`.
pub mod generated;
/// The owner's goal and its status (ADR 0075).
pub mod goal;
/// The governor: every rule of `docs/SPEC.md` section 5 as pure functions.
pub mod governor;
/// The owner's recurring jobs, read from files, schedulers' answers and Claude Code's records.
pub mod jobs;
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
/// A project folder's member repositories and the rules for adding one.
pub mod repos;
/// A skill's hash.
pub mod skill;
/// The team, its rules, and its validator.
pub mod team;
/// Small shared pieces of English used in refusal messages, and how a text is counted in tokens.
pub mod text;
