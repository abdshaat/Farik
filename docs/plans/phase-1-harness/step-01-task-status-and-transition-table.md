# Phase 1, step 01: Task status and transition table

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.2 (the task lifecycle and its transition table), F5 (the governor as a library with a test for every transition)
Depends on: phase 0 (merged in #4)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-16, before execution (first pass NOT READY on two clippy lints in the planned tests, fixed in 535ddfe; second pass READY under all three rules; recorded on pull request #5)

## Goal

The transition table of `docs/SPEC.md` section 5.2 exists in `farik-core` as data: twenty rows, each naming the status a task leaves, the status it enters, who may request the move, and the gate it must pass. Anyone can ask which rows apply to a move from one status to another, or which rows leave a status, and get the same answer the spec gives. Later steps of this phase build every gate and then compose them over this table; the test suite of step 09 iterates it; documentation can be generated from it. Nothing in this step evaluates a transition.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- The table is data, twenty rows of `{ from, to, actor, gate }`, in the order of the spec's table with the three `any` rows and the `escalated → any` row last. Rejected: code branches, because the test suite must be able to iterate the rows.
- The twenty rows are the spec's sixteen lines with `refining → escalated` split by trigger (`ReadinessExhausted`, `ContractRequiresHuman`), `ready → assigned` split by actor (`ScrumMaster`, `ProductManager`), `blocked → in_progress` split by actor (`ScrumMaster`, `Human`), `any → escalated` split by actor (`Governor` with gate `GovernorEscalation`, `Human` with gate `None`), plus `any → cancelled` for `Human` and `escalated → any` for `Human`, both with gate `None`. The sixteen specific rows and the four `any` rows are counted by the tests.
- `Status::Any` matches every status. A lookup for a move refuses two cases before consulting the table: a move out of a terminal status (`accepted`, `cancelled`), because nothing leaves them, and a move to the same status, because `any → escalated` must not mean `escalated → escalated`. Rejected: encoding those exclusions as extra rows, because the spec has none.
- The table is a `static`, not a `const`, so that lookups return `&'static TransitionRow` without relying on constant promotion. `TASK_STATUSES` is a `const` because callers copy the values.
- `TASK_STATUSES` lists the statuses in the order of the schema's `status` enum, and a test checks that the two agree, so that a schema change is caught here.
- Test names state behavior (`docs/standards/code.md`); tests use qualified enum paths through short aliases (`S`, `A`, `G`) rather than glob imports, because `clippy::enum_glob_use` is on; a `Line` type alias keeps the spec-line table under `clippy::type_complexity`, and `is_specific` takes its `Copy` row by value for `clippy::trivially_copy_pass_by_ref`.
- Every code block below is the file after `cargo fmt --all`, so that the committed file and the plan are the same bytes.

## Design

Two tasks. The first adds the `governor` module with `task_status`: the list of statuses and the terminal predicate. The second adds `transition_table`: the actor and gate enums, the `Status` pattern, the row type, the twenty rows, and the two lookups, with tests that pin every line of the spec's table, the `any` rows, the two refusals, and reachability from `draft`.

Out of scope: evaluating a transition (step 09), any gate (steps 02 to 08), the actor check (step 09).

## Architecture notes

Touches `crates/core` only: a new `governor` module with two children. Consumes `contract::TaskStatus` (phase 0 step 03, the generated status enum, which is `Copy`, `Eq`, `Ord`, `Hash`, and prints its wire name) and the embedded schema copy `crates/core/src/generated/task_contract.schema.json` (phase 0 step 02) in one test. Adds no dependency.

## Global constraints

- `farik-core` does no I/O; `include_str!` in a test is compile-time; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/lib.rs                                 modifies: declares the governor module
crates/core/src/governor.rs                            creates: the governor module root, declares task_status and transition_table
crates/core/src/governor/task_status.rs                creates: TASK_STATUSES, is_terminal, two tests
crates/core/src/governor/transition_table.rs           creates: TransitionActor, GateId, Status, TransitionRow, TRANSITION_TABLE, find_transitions, transitions_from, ten tests
docs/plans/phase-1-harness/step-01-task-status-and-transition-table.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: The statuses and the terminal ones

Files: created `crates/core/src/governor.rs`, `crates/core/src/governor/task_status.rs`; modified `crates/core/src/lib.rs`

Consumes: `farik_core::contract::TaskStatus` from `main`
Produces: `governor::task_status::TASK_STATUSES: [TaskStatus; 11]`; `governor::task_status::is_terminal(status: TaskStatus) -> bool`

- [x] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Declare the module. `crates/core/src/lib.rs` in full:

  ```rust
  //! Farik's harness: schemas, the task state machine, the governor, and the cost model.
  //! This crate performs no I/O.

  /// The task contract and its validator.
  pub mod contract;
  /// Types generated from `docs/schemas/`.
  pub mod generated;
  /// The governor: every rule of `docs/SPEC.md` section 5 as pure functions.
  pub mod governor;

  /// The crate's package name, as published.
  pub const CORE_CRATE_NAME: &str = "farik-core";

  #[cfg(test)]
  mod tests {
      use super::CORE_CRATE_NAME;

      #[test]
      fn exposes_its_crate_name() {
          assert_eq!(CORE_CRATE_NAME, "farik-core");
      }
  }
  ```

  `crates/core/src/governor.rs`, holding only the first child for now:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// The lifecycle's statuses and which of them are terminal.
  pub mod task_status;
  ```

- [x] Write the failing tests. `crates/core/src/governor/task_status.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::collections::BTreeSet;

      use serde_json::Value;

      use super::{TASK_STATUSES, is_terminal};
      use crate::contract::TaskStatus;

      #[test]
      fn lists_every_status_of_the_schema_exactly_once() {
          let schema: Value =
              serde_json::from_str(include_str!("../generated/task_contract.schema.json"))
                  .expect("the embedded schema is valid JSON");
          let wire: Vec<String> = schema["properties"]["status"]["enum"]
              .as_array()
              .expect("status is an enum")
              .iter()
              .map(|value| value.as_str().expect("a status name").to_string())
              .collect();
          let listed: Vec<String> = TASK_STATUSES.iter().map(ToString::to_string).collect();
          let distinct: BTreeSet<&String> = listed.iter().collect();
          assert_eq!(distinct.len(), TASK_STATUSES.len());
          assert_eq!(listed, wire);
      }

      #[test]
      fn treats_only_accepted_and_cancelled_as_terminal() {
          let terminal: Vec<TaskStatus> = TASK_STATUSES
              .into_iter()
              .filter(|status| is_terminal(*status))
              .collect();
          assert_eq!(terminal, [TaskStatus::Accepted, TaskStatus::Cancelled]);
      }
  }
  ```

- [x] Run it and confirm it fails because the items are missing:

  ```
  cargo test --package farik-core governor::task_status
  # expected, among the output:
  # error[E0432]: unresolved imports `super::TASK_STATUSES`, `super::is_terminal`
  ```

- [x] Write the implementation above the tests. `crates/core/src/governor/task_status.rs` in full:

  ```rust
  use crate::contract::TaskStatus;

  /// Every status of the task lifecycle, in the order of the schema's `status` enum.
  pub const TASK_STATUSES: [TaskStatus; 11] = [
      TaskStatus::Draft,
      TaskStatus::Refining,
      TaskStatus::Ready,
      TaskStatus::Assigned,
      TaskStatus::InProgress,
      TaskStatus::Blocked,
      TaskStatus::Verifying,
      TaskStatus::Rejected,
      TaskStatus::Accepted,
      TaskStatus::Escalated,
      TaskStatus::Cancelled,
  ];

  /// Whether a status is one that no transition leaves: `accepted` or `cancelled`.
  #[must_use]
  pub fn is_terminal(status: TaskStatus) -> bool {
      matches!(status, TaskStatus::Accepted | TaskStatus::Cancelled)
  }

  #[cfg(test)]
  mod tests {
      use std::collections::BTreeSet;

      use serde_json::Value;

      use super::{TASK_STATUSES, is_terminal};
      use crate::contract::TaskStatus;

      #[test]
      fn lists_every_status_of_the_schema_exactly_once() {
          let schema: Value =
              serde_json::from_str(include_str!("../generated/task_contract.schema.json"))
                  .expect("the embedded schema is valid JSON");
          let wire: Vec<String> = schema["properties"]["status"]["enum"]
              .as_array()
              .expect("status is an enum")
              .iter()
              .map(|value| value.as_str().expect("a status name").to_string())
              .collect();
          let listed: Vec<String> = TASK_STATUSES.iter().map(ToString::to_string).collect();
          let distinct: BTreeSet<&String> = listed.iter().collect();
          assert_eq!(distinct.len(), TASK_STATUSES.len());
          assert_eq!(listed, wire);
      }

      #[test]
      fn treats_only_accepted_and_cancelled_as_terminal() {
          let terminal: Vec<TaskStatus> = TASK_STATUSES
              .into_iter()
              .filter(|status| is_terminal(*status))
              .collect();
          assert_eq!(terminal, [TaskStatus::Accepted, TaskStatus::Cancelled]);
      }
  }
  ```

- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::task_status
  # expected, among the output:
  # test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `feat(core): list the task statuses and the terminal ones`

### Task 2: The transition table and its lookups

Files: created `crates/core/src/governor/transition_table.rs`; modified `crates/core/src/governor.rs`

Consumes: `governor::task_status::{TASK_STATUSES, is_terminal}` from Task 1; `contract::TaskStatus` from `main`
Produces: `governor::transition_table::{TransitionActor, GateId, Status, TransitionRow, TRANSITION_TABLE, find_transitions, transitions_from}`

- [x] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// The lifecycle's statuses and which of them are terminal.
  pub mod task_status;
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```

- [x] Write the failing tests. `crates/core/src/governor/transition_table.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::collections::BTreeSet;

      use super::GateId as G;
      use super::TransitionActor as A;
      use super::{Status, TRANSITION_TABLE, TransitionRow, find_transitions, transitions_from};
      use crate::contract::TaskStatus as S;
      use crate::governor::task_status::{TASK_STATUSES, is_terminal};

      fn actors_and_gates(rows: &[&TransitionRow]) -> Vec<(A, G)> {
          rows.iter().map(|row| (row.actor, row.gate)).collect()
      }

      type Line = (S, S, &'static [(A, G)]);

      fn is_specific(row: TransitionRow) -> bool {
          row.from != Status::Any && row.to != Status::Any
      }

      #[test]
      fn has_exactly_twenty_distinct_rows() {
          let distinct: BTreeSet<String> = TRANSITION_TABLE
              .iter()
              .map(|row| format!("{row:?}"))
              .collect();
          assert_eq!(TRANSITION_TABLE.len(), 20);
          assert_eq!(distinct.len(), 20);
      }

      #[test]
      fn has_sixteen_specific_rows_and_four_any_rows() {
          let specific = TRANSITION_TABLE
              .iter()
              .filter(|row| is_specific(**row))
              .count();
          assert_eq!(specific, 16);
          assert_eq!(TRANSITION_TABLE.len() - specific, 4);
      }

      #[test]
      fn matches_every_line_of_the_spec_table() {
          let lines: [Line; 13] = [
              (S::Draft, S::Refining, &[(A::ProductManager, G::Triaged)]),
              (
                  S::Refining,
                  S::Ready,
                  &[(A::Governor, G::DefinitionOfReady)],
              ),
              (
                  S::Refining,
                  S::Escalated,
                  &[
                      (A::Governor, G::ReadinessExhausted),
                      (A::Governor, G::ContractRequiresHuman),
                  ],
              ),
              (
                  S::Ready,
                  S::Assigned,
                  &[
                      (A::ScrumMaster, G::Assignment),
                      (A::ProductManager, G::Assignment),
                  ],
              ),
              (S::Assigned, S::InProgress, &[(A::Assignee, G::None)]),
              (
                  S::InProgress,
                  S::Verifying,
                  &[(A::Assignee, G::CriteriaRecorded)],
              ),
              (
                  S::InProgress,
                  S::Blocked,
                  &[(A::Assignee, G::BlockerWritten)],
              ),
              (
                  S::Blocked,
                  S::InProgress,
                  &[
                      (A::ScrumMaster, G::BlockerResolved),
                      (A::Human, G::BlockerResolved),
                  ],
              ),
              (S::Blocked, S::Escalated, &[(A::Governor, G::BlockedAge)]),
              (
                  S::Verifying,
                  S::Accepted,
                  &[(A::ProductManager, G::DefinitionOfDone)],
              ),
              (
                  S::Verifying,
                  S::Rejected,
                  &[(A::Reviewer, G::RejectionReasons)],
              ),
              (
                  S::Rejected,
                  S::InProgress,
                  &[(A::Governor, G::IterationBelowLimit)],
              ),
              (
                  S::Rejected,
                  S::Escalated,
                  &[(A::Governor, G::IterationLimitReached)],
              ),
          ];
          for (from, to, expected) in lines {
              let rows: Vec<&TransitionRow> = find_transitions(from, to)
                  .into_iter()
                  .filter(|row| is_specific(**row))
                  .collect();
              assert_eq!(actors_and_gates(&rows), expected, "{from} -> {to}");
          }
      }

      #[test]
      fn escalates_any_non_terminal_status_for_the_governor_and_the_human() {
          for status in TASK_STATUSES {
              if is_terminal(status) || status == S::Escalated {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(status, S::Escalated)
                  .into_iter()
                  .filter(|row| row.from == Status::Any)
                  .collect();
              assert_eq!(
                  actors_and_gates(&rows),
                  [(A::Governor, G::GovernorEscalation), (A::Human, G::None)],
                  "{status}"
              );
          }
      }

      #[test]
      fn cancels_any_non_terminal_status_for_the_human_only() {
          for status in TASK_STATUSES {
              if is_terminal(status) {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(status, S::Cancelled)
                  .into_iter()
                  .filter(|row| row.from == Status::Any)
                  .collect();
              assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
          }
      }

      #[test]
      fn lets_the_human_move_an_escalated_task_to_any_other_status() {
          for status in TASK_STATUSES {
              if status == S::Escalated {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(S::Escalated, status)
                  .into_iter()
                  .filter(|row| row.to == Status::Any)
                  .collect();
              assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
          }
      }

      #[test]
      fn refuses_to_leave_a_terminal_status() {
          for status in [S::Accepted, S::Cancelled] {
              assert!(transitions_from(status).is_empty(), "{status}");
              assert!(
                  find_transitions(status, S::Escalated).is_empty(),
                  "{status}"
              );
              assert!(
                  find_transitions(status, S::Cancelled).is_empty(),
                  "{status}"
              );
          }
      }

      #[test]
      fn refuses_a_transition_to_the_same_status() {
          for status in TASK_STATUSES {
              assert!(find_transitions(status, status).is_empty(), "{status}");
          }
      }

      #[test]
      fn lists_the_rows_that_leave_a_status_in_table_order() {
          assert_eq!(
              actors_and_gates(&transitions_from(S::Blocked)),
              [
                  (A::ScrumMaster, G::BlockerResolved),
                  (A::Human, G::BlockerResolved),
                  (A::Governor, G::BlockedAge),
                  (A::Governor, G::GovernorEscalation),
                  (A::Human, G::None),
                  (A::Human, G::None),
              ]
          );
      }

      #[test]
      fn reaches_every_status_from_draft() {
          let mut seen = BTreeSet::from([S::Draft]);
          let mut queue = vec![S::Draft];
          while let Some(status) = queue.pop() {
              for row in transitions_from(status) {
                  let targets: Vec<S> = match row.to {
                      Status::Any => TASK_STATUSES.to_vec(),
                      Status::Is(target) => vec![target],
                  };
                  for target in targets {
                      if seen.insert(target) {
                          queue.push(target);
                      }
                  }
              }
          }
          assert_eq!(seen.len(), TASK_STATUSES.len());
      }
  }
  ```

- [x] Run it and confirm it fails because the items are missing:

  ```
  cargo test --package farik-core governor::transition_table
  # expected, among the output:
  # error[E0432]: unresolved import `super::GateId`
  # error[E0432]: unresolved import `super::TransitionActor`
  # error[E0432]: unresolved imports `super::Status`, `super::TRANSITION_TABLE`, `super::TransitionRow`, `super::find_transitions`, `super::transitions_from`
  ```

- [x] Write the implementation above the tests. `crates/core/src/governor/transition_table.rs` in full:

  ```rust
  use super::task_status::is_terminal;
  use crate::contract::TaskStatus;

  /// Who may request a transition. `Governor` means the orchestrator acting on observed facts;
  /// `Human` means the user.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub enum TransitionActor {
      /// The Product Manager role.
      ProductManager,
      /// The Scrum Master role.
      ScrumMaster,
      /// The agent assigned to the task.
      Assignee,
      /// The agent reviewing the task.
      Reviewer,
      /// The orchestrator, from observed facts, never from a request.
      Governor,
      /// The user.
      Human,
  }

  /// The gate a transition must pass; `None` means the transition has no gate. Each gate is a
  /// predicate of its own in a later step of this phase.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub enum GateId {
      /// No gate.
      None,
      /// The request has a recorded triage decision (spec 5.16).
      Triaged,
      /// The Definition of Ready (spec 5.3).
      DefinitionOfReady,
      /// The contract failed the Definition of Ready three times.
      ReadinessExhausted,
      /// The contract itself needs the human's acceptance: high risk, policy, or an epic.
      ContractRequiresHuman,
      /// Assignee role, sprint budget, WIP limit, and integrated dependencies (spec 5.2, 5.14).
      Assignment,
      /// Every criterion has the assignee's own result and the work is committed; for an epic,
      /// its tasks are done (spec 5.16).
      CriteriaRecorded,
      /// A written blocker with what is needed.
      BlockerWritten,
      /// The blocker is resolved.
      BlockerResolved,
      /// Blocked longer than the configured limit.
      BlockedAge,
      /// The Definition of Done (spec 5.4).
      DefinitionOfDone,
      /// Written reasons mapped to failed criteria.
      RejectionReasons,
      /// The iteration count is below the limit.
      IterationBelowLimit,
      /// The iteration limit is reached.
      IterationLimitReached,
      /// Budget exhausted or permission denied on a required action.
      GovernorEscalation,
  }

  /// A status pattern in a row: one status, or any status.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Status {
      /// Matches every status.
      Any,
      /// Matches one status.
      Is(TaskStatus),
  }

  impl Status {
      /// Whether the pattern matches a status.
      #[must_use]
      pub fn matches(self, status: TaskStatus) -> bool {
          match self {
              Self::Any => true,
              Self::Is(pattern) => pattern == status,
          }
      }
  }

  /// One line of the transition table: a move, who may request it, and the gate it passes.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct TransitionRow {
      /// The status the task leaves.
      pub from: Status,
      /// The status the task enters.
      pub to: Status,
      /// Who may request the move.
      pub actor: TransitionActor,
      /// The gate the move must pass.
      pub gate: GateId,
  }

  const fn row(from: Status, to: Status, actor: TransitionActor, gate: GateId) -> TransitionRow {
      TransitionRow {
          from,
          to,
          actor,
          gate,
      }
  }

  /// The transition table of `docs/SPEC.md` section 5.2: the spec's lines in order, split where
  /// a line names two triggers or two actors, with the `any` rows last.
  pub static TRANSITION_TABLE: [TransitionRow; 20] = {
      use GateId as G;
      use Status::{Any, Is};
      use TaskStatus as S;
      use TransitionActor as A;
      [
          row(Is(S::Draft), Is(S::Refining), A::ProductManager, G::Triaged),
          row(
              Is(S::Refining),
              Is(S::Ready),
              A::Governor,
              G::DefinitionOfReady,
          ),
          row(
              Is(S::Refining),
              Is(S::Escalated),
              A::Governor,
              G::ReadinessExhausted,
          ),
          row(
              Is(S::Refining),
              Is(S::Escalated),
              A::Governor,
              G::ContractRequiresHuman,
          ),
          row(Is(S::Ready), Is(S::Assigned), A::ScrumMaster, G::Assignment),
          row(
              Is(S::Ready),
              Is(S::Assigned),
              A::ProductManager,
              G::Assignment,
          ),
          row(Is(S::Assigned), Is(S::InProgress), A::Assignee, G::None),
          row(
              Is(S::InProgress),
              Is(S::Verifying),
              A::Assignee,
              G::CriteriaRecorded,
          ),
          row(
              Is(S::InProgress),
              Is(S::Blocked),
              A::Assignee,
              G::BlockerWritten,
          ),
          row(
              Is(S::Blocked),
              Is(S::InProgress),
              A::ScrumMaster,
              G::BlockerResolved,
          ),
          row(
              Is(S::Blocked),
              Is(S::InProgress),
              A::Human,
              G::BlockerResolved,
          ),
          row(Is(S::Blocked), Is(S::Escalated), A::Governor, G::BlockedAge),
          row(
              Is(S::Verifying),
              Is(S::Accepted),
              A::ProductManager,
              G::DefinitionOfDone,
          ),
          row(
              Is(S::Verifying),
              Is(S::Rejected),
              A::Reviewer,
              G::RejectionReasons,
          ),
          row(
              Is(S::Rejected),
              Is(S::InProgress),
              A::Governor,
              G::IterationBelowLimit,
          ),
          row(
              Is(S::Rejected),
              Is(S::Escalated),
              A::Governor,
              G::IterationLimitReached,
          ),
          row(Any, Is(S::Escalated), A::Governor, G::GovernorEscalation),
          row(Any, Is(S::Escalated), A::Human, G::None),
          row(Any, Is(S::Cancelled), A::Human, G::None),
          row(Is(S::Escalated), Any, A::Human, G::None),
      ]
  };

  /// The rows that apply to a move from `from` to `to`, in table order. Empty when `from` is
  /// terminal, because nothing leaves `accepted` or `cancelled`, and when `from` and `to` are the
  /// same status, because an `any` row never means staying put.
  #[must_use]
  pub fn find_transitions(from: TaskStatus, to: TaskStatus) -> Vec<&'static TransitionRow> {
      if from == to || is_terminal(from) {
          return Vec::new();
      }
      TRANSITION_TABLE
          .iter()
          .filter(|row| row.from.matches(from) && row.to.matches(to))
          .collect()
  }

  /// The rows that leave `from`, in table order. Empty when `from` is terminal.
  #[must_use]
  pub fn transitions_from(from: TaskStatus) -> Vec<&'static TransitionRow> {
      if is_terminal(from) {
          return Vec::new();
      }
      TRANSITION_TABLE
          .iter()
          .filter(|row| row.from.matches(from))
          .collect()
  }

  #[cfg(test)]
  mod tests {
      use std::collections::BTreeSet;

      use super::GateId as G;
      use super::TransitionActor as A;
      use super::{Status, TRANSITION_TABLE, TransitionRow, find_transitions, transitions_from};
      use crate::contract::TaskStatus as S;
      use crate::governor::task_status::{TASK_STATUSES, is_terminal};

      fn actors_and_gates(rows: &[&TransitionRow]) -> Vec<(A, G)> {
          rows.iter().map(|row| (row.actor, row.gate)).collect()
      }

      type Line = (S, S, &'static [(A, G)]);

      fn is_specific(row: TransitionRow) -> bool {
          row.from != Status::Any && row.to != Status::Any
      }

      #[test]
      fn has_exactly_twenty_distinct_rows() {
          let distinct: BTreeSet<String> = TRANSITION_TABLE
              .iter()
              .map(|row| format!("{row:?}"))
              .collect();
          assert_eq!(TRANSITION_TABLE.len(), 20);
          assert_eq!(distinct.len(), 20);
      }

      #[test]
      fn has_sixteen_specific_rows_and_four_any_rows() {
          let specific = TRANSITION_TABLE
              .iter()
              .filter(|row| is_specific(**row))
              .count();
          assert_eq!(specific, 16);
          assert_eq!(TRANSITION_TABLE.len() - specific, 4);
      }

      #[test]
      fn matches_every_line_of_the_spec_table() {
          let lines: [Line; 13] = [
              (S::Draft, S::Refining, &[(A::ProductManager, G::Triaged)]),
              (
                  S::Refining,
                  S::Ready,
                  &[(A::Governor, G::DefinitionOfReady)],
              ),
              (
                  S::Refining,
                  S::Escalated,
                  &[
                      (A::Governor, G::ReadinessExhausted),
                      (A::Governor, G::ContractRequiresHuman),
                  ],
              ),
              (
                  S::Ready,
                  S::Assigned,
                  &[
                      (A::ScrumMaster, G::Assignment),
                      (A::ProductManager, G::Assignment),
                  ],
              ),
              (S::Assigned, S::InProgress, &[(A::Assignee, G::None)]),
              (
                  S::InProgress,
                  S::Verifying,
                  &[(A::Assignee, G::CriteriaRecorded)],
              ),
              (
                  S::InProgress,
                  S::Blocked,
                  &[(A::Assignee, G::BlockerWritten)],
              ),
              (
                  S::Blocked,
                  S::InProgress,
                  &[
                      (A::ScrumMaster, G::BlockerResolved),
                      (A::Human, G::BlockerResolved),
                  ],
              ),
              (S::Blocked, S::Escalated, &[(A::Governor, G::BlockedAge)]),
              (
                  S::Verifying,
                  S::Accepted,
                  &[(A::ProductManager, G::DefinitionOfDone)],
              ),
              (
                  S::Verifying,
                  S::Rejected,
                  &[(A::Reviewer, G::RejectionReasons)],
              ),
              (
                  S::Rejected,
                  S::InProgress,
                  &[(A::Governor, G::IterationBelowLimit)],
              ),
              (
                  S::Rejected,
                  S::Escalated,
                  &[(A::Governor, G::IterationLimitReached)],
              ),
          ];
          for (from, to, expected) in lines {
              let rows: Vec<&TransitionRow> = find_transitions(from, to)
                  .into_iter()
                  .filter(|row| is_specific(**row))
                  .collect();
              assert_eq!(actors_and_gates(&rows), expected, "{from} -> {to}");
          }
      }

      #[test]
      fn escalates_any_non_terminal_status_for_the_governor_and_the_human() {
          for status in TASK_STATUSES {
              if is_terminal(status) || status == S::Escalated {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(status, S::Escalated)
                  .into_iter()
                  .filter(|row| row.from == Status::Any)
                  .collect();
              assert_eq!(
                  actors_and_gates(&rows),
                  [(A::Governor, G::GovernorEscalation), (A::Human, G::None)],
                  "{status}"
              );
          }
      }

      #[test]
      fn cancels_any_non_terminal_status_for_the_human_only() {
          for status in TASK_STATUSES {
              if is_terminal(status) {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(status, S::Cancelled)
                  .into_iter()
                  .filter(|row| row.from == Status::Any)
                  .collect();
              assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
          }
      }

      #[test]
      fn lets_the_human_move_an_escalated_task_to_any_other_status() {
          for status in TASK_STATUSES {
              if status == S::Escalated {
                  continue;
              }
              let rows: Vec<&TransitionRow> = find_transitions(S::Escalated, status)
                  .into_iter()
                  .filter(|row| row.to == Status::Any)
                  .collect();
              assert_eq!(actors_and_gates(&rows), [(A::Human, G::None)], "{status}");
          }
      }

      #[test]
      fn refuses_to_leave_a_terminal_status() {
          for status in [S::Accepted, S::Cancelled] {
              assert!(transitions_from(status).is_empty(), "{status}");
              assert!(
                  find_transitions(status, S::Escalated).is_empty(),
                  "{status}"
              );
              assert!(
                  find_transitions(status, S::Cancelled).is_empty(),
                  "{status}"
              );
          }
      }

      #[test]
      fn refuses_a_transition_to_the_same_status() {
          for status in TASK_STATUSES {
              assert!(find_transitions(status, status).is_empty(), "{status}");
          }
      }

      #[test]
      fn lists_the_rows_that_leave_a_status_in_table_order() {
          assert_eq!(
              actors_and_gates(&transitions_from(S::Blocked)),
              [
                  (A::ScrumMaster, G::BlockerResolved),
                  (A::Human, G::BlockerResolved),
                  (A::Governor, G::BlockedAge),
                  (A::Governor, G::GovernorEscalation),
                  (A::Human, G::None),
                  (A::Human, G::None),
              ]
          );
      }

      #[test]
      fn reaches_every_status_from_draft() {
          let mut seen = BTreeSet::from([S::Draft]);
          let mut queue = vec![S::Draft];
          while let Some(status) = queue.pop() {
              for row in transitions_from(status) {
                  let targets: Vec<S> = match row.to {
                      Status::Any => TASK_STATUSES.to_vec(),
                      Status::Is(target) => vec![target],
                  };
                  for target in targets {
                      if seen.insert(target) {
                          queue.push(target);
                      }
                  }
              }
          }
          assert_eq!(seen.len(), TASK_STATUSES.len());
      }
  }
  ```


- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::transition_table
  # expected, among the output:
  # test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `feat(core): add the transition table of spec 5.2 as data`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -2
# expected, newest first:
# feat(core): add the transition table of spec 5.2 as data
# feat(core): list the task statuses and the terminal ones
```

## Open questions

none
