# Phase 1, step 06: Iteration and escalation rules

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.2 (`rejected → in_progress` below the iteration limit and `rejected → escalated` at it; `refining → escalated` after three failed readiness checks; `blocked → escalated` at or past the blocked limit, default 24 hours), section 5.7 (what an escalation carries and its ten reasons; this step writes the tenth into its list), section 5.16 (what the `approval` reason waits for), F5
Depends on: phase 0 (merged in #4); step 05 of this phase (committed as 5ad2c21, c4cf30b, and its review fix f3ecea4)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code session that did not write this plan, 2026-09-16. Second pass at `cdb388c`: READY under all three rules of `docs/standards/workflow.md` stage 2. It executed the whole plan in a scratch copy rather than taking the evidence on trust: the baseline, the red output byte-identical to rustc's two lines, `cargo fmt --all --check` clean on the blocks as written, 7 tests green with 114 filtered out, 121 and 23 in the workspace, clippy pedantic with `-D warnings` clean, `cargo xtask core-io` clean, `git status` showing only the file map's files, and the spec replacement's before-text occurring exactly once. Two important items it raised with the verdict are taken in this commit: the blocked-age boundary is written into spec 5.2 rather than left contradicting it, and the event kind is `escalation.raised`, which spec 8.5 lists, rather than an invented `task.escalated`. Its two minors are taken too: `Depends on` names step 05's review fix, and the zero-attempt assertion says in a comment why a count nothing produces is still pinned.

## Goal

`farik-core` can decide the three counting rules of the lifecycle that are not budgets: whether a rejected task goes back to `in_progress` or escalates, whether a contract that failed the Definition of Ready is refined again or its task escalates, and whether a blocked task has waited too long; and it has the record an escalation carries to the user, with the spec's ten reasons as wire names. Step 09's transition evaluation calls these three functions behind the gates `IterationBelowLimit`, `IterationLimitReached`, `ReadinessExhausted`, and `BlockedAge`, which are gates of step 01's table rather than step 08's predicates; phase 2's events carry the reason.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- `iteration` is how many times the task has already been returned to `in_progress` after a rejection, the contract's `iteration` field; `max_iterations` is how many times that may happen (the schema's own words: "how many times the task may be rejected and returned to in_progress before it escalates"). A rejection returns the task while `iteration < max_iterations` and escalates once the limit is reached, so with the default of 3 the first three rejections return and the fourth escalates; step 09's `IncrementIteration` effect counts each return. Rejected: counting the rejection itself before comparing, because then the third rejection of a task allowed three returns would escalate.
- A failed readiness check is counted with the failure that just happened included: the first two failures retry, the third escalates (spec 5.2, "fails DoR three times"); the limit is the constant `READINESS_ATTEMPT_LIMIT` = 3, because the spec gives one number and no contract field carries it.
- A blocked task escalates when its age reaches the limit (`>=`), consistent with step 05's budgets, where spec 5.5 already says a budget is exhausted when its spend reaches its limit. Spec 5.2's row said "blocked longer than the configured limit", which excludes the instant the limit is reached, and step 01's `GateId::BlockedAge` doc repeated it; both are rewritten to "for the configured limit or longer" in this step's commit, so that the spec and the test pin the same instant (hard rule 8). Rejected: `>` to match the old wording, because a limit that fires at exactly 24 hours is the one a user can predict, and because the harness would then have two boundary conventions. `DEFAULT_BLOCKED_LIMIT` is 24 hours; the limit is a parameter because the spec says it is configurable. Time is injected: the function takes `blocked_at` and `now` (project plan, every-phase decisions), and a `now` before `blocked_at`, a clock that went backwards, counts as within the limit rather than as an error, because the next tick will tell. `std::time::Duration` is the limit's type, as in step 05; `chrono::DateTime<Utc>` is the timestamp's, as in the generated contract.
- `EscalationReason` has spec 5.7's list, which already names the approval of an epic (5.16 says what that approval waits for), plus `readiness_failures`, which 5.7 does not name: spec 5.2 escalates a contract that failed the Definition of Ready three times, and 5.7's prose listed nine reasons without that one. This step writes the tenth into 5.7 in the wire spelling, so that the spec's list and this enum are the same ten (hard rule 8). Serialised in `snake_case` (`blocker_age`, `risk_gate`, `readiness_failures`, `explicit_request`), so that phase 2's `escalation.raised` event, the kind spec 8.5 lists and the project plan's phase 2 step 10 adds, carries the same words the spec uses. Rejected: a generated type, because the event schema that will carry it is phase 2 step 01's.
- `Escalation` is a plain record (`task_id`, `reason`, `tried`, `options`); the runtime fills `tried` and `options` from the agent's escalation call, and this step only defines the shape.
- `DEFAULT_ITERATION_LIMIT` = 3 is a public constant, the schema's default for `max_iterations`, so that tests and later steps name the number once; the project plan's step 06 entry gains the three constants in this plan's commit.
- The counts are `u32`, as step 05's `task_max_sessions` is, while the generated contract carries `iteration: u64` and `max_iterations: NonZeroU64`; step 09 converts with `u32::try_from` rather than `as`, which clippy pedantic refuses, and its plan budgets for the conversion.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

One task: the `governor::escalation` module with `EscalationReason`, `Escalation`, the three constants, `RejectionOutcome` and `evaluate_rejection`, `ReadinessOutcome` and `evaluate_readiness_attempts`, `BlockedAge` and `evaluate_blocked_age`, and seven tests.

Out of scope: the gates that call these (step 08), the transition rows they serve (step 09), the events that carry an escalation (phase 2), the Scrum Master's digest and the notification age (phase 3 and 4).

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `contract::TaskId` (phase 0), and `chrono` and `serde`, already dependencies. Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock; `now` is passed in; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                         modifies: declares escalation
crates/core/src/governor/transition_table.rs        modifies: the BlockedAge gate's doc says "for the configured limit or longer"
crates/core/src/governor/escalation.rs              creates: EscalationReason, Escalation, the three constants, the three outcome enums and functions, seven tests
docs/SPEC.md                                        modifies: section 5.7 names the ten reasons in their wire spelling; section 5.2's blocked row says "for the configured limit or longer"
docs/plans/project-plan.md                          modifies: phase 1 step 06 interface gains the three constants (in the plan's own commit)
docs/plans/phase-1-harness/step-06-iteration-and-escalation-rules.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: The three counting rules and the escalation record

Files: created `crates/core/src/governor/escalation.rs`; modified `crates/core/src/governor.rs`

Consumes: `contract::TaskId` from `main`
Produces: `governor::escalation::{EscalationReason, Escalation, DEFAULT_ITERATION_LIMIT, READINESS_ATTEMPT_LIMIT, DEFAULT_BLOCKED_LIMIT, RejectionOutcome, evaluate_rejection, ReadinessOutcome, evaluate_readiness_attempts, BlockedAge, evaluate_blocked_age}`

- [ ] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 114 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// Iteration and escalation rules (`docs/SPEC.md` sections 5.2 and 5.7).
  pub mod escalation;
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
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```
- [ ] Write the failing tests. `crates/core/src/governor/escalation.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::time::Duration;

      use chrono::{DateTime, Utc};
      use serde_json::json;

      use super::{
          BlockedAge, DEFAULT_BLOCKED_LIMIT, DEFAULT_ITERATION_LIMIT, Escalation, EscalationReason,
          READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome, evaluate_blocked_age,
          evaluate_readiness_attempts, evaluate_rejection,
      };

      fn at(text: &str) -> DateTime<Utc> {
          text.parse().expect("an ISO 8601 UTC timestamp")
      }

      #[test]
      fn returns_a_rejected_task_while_its_iterations_are_below_the_limit() {
          for iteration in 0..DEFAULT_ITERATION_LIMIT {
              assert_eq!(
                  evaluate_rejection(iteration, DEFAULT_ITERATION_LIMIT),
                  RejectionOutcome::ReturnToInProgress,
                  "{iteration}"
              );
          }
      }

      #[test]
      fn escalates_a_rejected_task_once_its_iterations_reach_the_limit() {
          assert_eq!(
              evaluate_rejection(DEFAULT_ITERATION_LIMIT, DEFAULT_ITERATION_LIMIT),
              RejectionOutcome::Escalate
          );
          assert_eq!(evaluate_rejection(1, 1), RejectionOutcome::Escalate);
          assert_eq!(
              evaluate_rejection(0, 1),
              RejectionOutcome::ReturnToInProgress
          );
      }

      #[test]
      fn retries_readiness_twice_and_escalates_on_the_third_failure() {
          // The count includes the failure that has just happened, so the runtime never passes
          // zero; zero retries all the same rather than escalating on a count nothing produces.
          assert_eq!(evaluate_readiness_attempts(0), ReadinessOutcome::Retry);
          assert_eq!(evaluate_readiness_attempts(1), ReadinessOutcome::Retry);
          assert_eq!(evaluate_readiness_attempts(2), ReadinessOutcome::Retry);
          assert_eq!(
              evaluate_readiness_attempts(READINESS_ATTEMPT_LIMIT),
              ReadinessOutcome::Escalate
          );
          assert_eq!(evaluate_readiness_attempts(4), ReadinessOutcome::Escalate);
      }

      #[test]
      fn escalates_a_task_blocked_for_the_limit_or_longer() {
          let blocked_at = at("2026-09-16T10:00:00Z");
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-17T09:59:59Z"),
                  DEFAULT_BLOCKED_LIMIT
              ),
              BlockedAge::WithinLimit
          );
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-17T10:00:00Z"),
                  DEFAULT_BLOCKED_LIMIT
              ),
              BlockedAge::Exceeded
          );
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-16T10:30:00Z"),
                  Duration::from_mins(30)
              ),
              BlockedAge::Exceeded
          );
      }

      #[test]
      fn treats_a_clock_that_went_backwards_as_within_the_limit() {
          assert_eq!(
              evaluate_blocked_age(
                  at("2026-09-16T10:00:00Z"),
                  at("2026-09-15T10:00:00Z"),
                  Duration::ZERO
              ),
              BlockedAge::WithinLimit
          );
      }

      #[test]
      fn names_the_reasons_of_the_spec_on_the_wire() {
          let reasons = [
              (EscalationReason::Budget, "budget"),
              (EscalationReason::Sessions, "sessions"),
              (EscalationReason::Iterations, "iterations"),
              (EscalationReason::BlockerAge, "blocker_age"),
              (EscalationReason::Permission, "permission"),
              (EscalationReason::RiskGate, "risk_gate"),
              (EscalationReason::Approval, "approval"),
              (EscalationReason::ReadinessFailures, "readiness_failures"),
              (EscalationReason::Integration, "integration"),
              (EscalationReason::ExplicitRequest, "explicit_request"),
          ];
          for (reason, wire) in reasons {
              assert_eq!(serde_json::to_value(reason).unwrap(), json!(wire));
              assert_eq!(
                  serde_json::from_value::<EscalationReason>(json!(wire)).unwrap(),
                  reason
              );
          }
      }

      #[test]
      fn carries_the_task_the_reason_what_was_tried_and_the_options() {
          let escalation = Escalation {
              task_id: "FRK-7".parse().expect("a task id"),
              reason: EscalationReason::Budget,
              tried: "Two sessions; the second ran out of tokens.".to_string(),
              options: vec![
                  "Raise the budget.".to_string(),
                  "Split the task.".to_string(),
              ],
          };
          assert_eq!(escalation.task_id.to_string(), "FRK-7");
          assert_eq!(escalation.options.len(), 2);
          assert_eq!(escalation.reason, EscalationReason::Budget);
      }
  }
  ```
- [ ] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::escalation
  # expected, among the output:
  # error[E0432]: unresolved imports `super::BlockedAge`, `super::DEFAULT_BLOCKED_LIMIT`, `super::DEFAULT_ITERATION_LIMIT`, `super::Escalation`, `super::EscalationReason`, `super::READINESS_ATTEMPT_LIMIT`, `super::ReadinessOutcome`, `super::RejectionOutcome`, `super::evaluate_blocked_age`, `super::evaluate_readiness_attempts`, `super::evaluate_rejection`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [ ] Write the implementation above the tests. `crates/core/src/governor/escalation.rs` in full:

  ```rust
  //! Iteration and escalation rules (`docs/SPEC.md` sections 5.2 and 5.7): how many rejections a
  //! task may take, how many readiness failures a contract may take, how long a task may stay
  //! blocked, and the record an escalation carries to the user.

  use std::time::Duration;

  use chrono::{DateTime, Utc};
  use serde::{Deserialize, Serialize};

  use crate::contract::TaskId;

  /// Why a task is escalated (`docs/SPEC.md` section 5.7). Serialised in `snake_case`, as on the
  /// wire.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum EscalationReason {
      /// A dollar budget ran out.
      Budget,
      /// The task's sessions ran out.
      Sessions,
      /// The rejection iteration limit was reached.
      Iterations,
      /// The task stayed blocked longer than the limit.
      BlockerAge,
      /// A permission was denied on a required action.
      Permission,
      /// The contract's risk requires the human's acceptance.
      RiskGate,
      /// An epic awaits the user's approval of its contract (spec 5.16).
      Approval,
      /// The contract failed the Definition of Ready three times (spec 5.2).
      ReadinessFailures,
      /// Integrating the accepted work failed (spec 5.14).
      Integration,
      /// The user asked for it.
      ExplicitRequest,
  }

  /// An escalation: the task, why, what was tried, and the options proposed to the user.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Escalation {
      /// The escalated task.
      pub task_id: TaskId,
      /// Why.
      pub reason: EscalationReason,
      /// What the agent tried before escalating.
      pub tried: String,
      /// The options the agent proposes; the user picks one or decides otherwise.
      pub options: Vec<String>,
  }

  /// The rejection iteration limit the schema defaults to (`max_iterations`).
  pub const DEFAULT_ITERATION_LIMIT: u32 = 3;

  /// How many times a contract may fail the Definition of Ready before its task escalates
  /// (`docs/SPEC.md` section 5.2, `refining → escalated`).
  pub const READINESS_ATTEMPT_LIMIT: u32 = 3;

  /// How long a task may stay blocked before it escalates (`docs/SPEC.md` section 5.2, default
  /// 24 hours).
  pub const DEFAULT_BLOCKED_LIMIT: Duration = Duration::from_hours(24);

  /// What follows a rejection.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum RejectionOutcome {
      /// The task returns to `in_progress` for another iteration.
      ReturnToInProgress,
      /// The iteration limit is reached; the task escalates.
      Escalate,
  }

  /// Decides a rejection: `iteration` is how many times the task has already been returned to
  /// `in_progress` after a rejection, and the contract's `max_iterations` is how many times that
  /// may happen, so the task returns while `iteration` is below the limit and escalates once the
  /// limit is reached.
  #[must_use]
  pub fn evaluate_rejection(iteration: u32, max_iterations: u32) -> RejectionOutcome {
      if iteration < max_iterations {
          RejectionOutcome::ReturnToInProgress
      } else {
          RejectionOutcome::Escalate
      }
  }

  /// What follows a failed Definition of Ready.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ReadinessOutcome {
      /// The Product Manager refines the contract again.
      Retry,
      /// Three attempts failed; the task escalates.
      Escalate,
  }

  /// Decides a failed readiness check from the number of failed attempts, this one included:
  /// retry below `READINESS_ATTEMPT_LIMIT`, escalate at it.
  #[must_use]
  pub fn evaluate_readiness_attempts(failed_attempts: u32) -> ReadinessOutcome {
      if failed_attempts < READINESS_ATTEMPT_LIMIT {
          ReadinessOutcome::Retry
      } else {
          ReadinessOutcome::Escalate
      }
  }

  /// Whether a blocked task has waited too long.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum BlockedAge {
      /// Blocked for less than the limit.
      WithinLimit,
      /// Blocked for the limit or longer; the task escalates.
      Exceeded,
  }

  /// Decides whether a task blocked at `blocked_at` has, at `now`, been blocked for `limit` or
  /// longer. A `now` before `blocked_at` (a clock that went backwards) counts as within the limit.
  #[must_use]
  pub fn evaluate_blocked_age(
      blocked_at: DateTime<Utc>,
      now: DateTime<Utc>,
      limit: Duration,
  ) -> BlockedAge {
      let Ok(age) = (now - blocked_at).to_std() else {
          return BlockedAge::WithinLimit;
      };
      if age >= limit {
          BlockedAge::Exceeded
      } else {
          BlockedAge::WithinLimit
      }
  }

  #[cfg(test)]
  mod tests {
      use std::time::Duration;

      use chrono::{DateTime, Utc};
      use serde_json::json;

      use super::{
          BlockedAge, DEFAULT_BLOCKED_LIMIT, DEFAULT_ITERATION_LIMIT, Escalation, EscalationReason,
          READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome, evaluate_blocked_age,
          evaluate_readiness_attempts, evaluate_rejection,
      };

      fn at(text: &str) -> DateTime<Utc> {
          text.parse().expect("an ISO 8601 UTC timestamp")
      }

      #[test]
      fn returns_a_rejected_task_while_its_iterations_are_below_the_limit() {
          for iteration in 0..DEFAULT_ITERATION_LIMIT {
              assert_eq!(
                  evaluate_rejection(iteration, DEFAULT_ITERATION_LIMIT),
                  RejectionOutcome::ReturnToInProgress,
                  "{iteration}"
              );
          }
      }

      #[test]
      fn escalates_a_rejected_task_once_its_iterations_reach_the_limit() {
          assert_eq!(
              evaluate_rejection(DEFAULT_ITERATION_LIMIT, DEFAULT_ITERATION_LIMIT),
              RejectionOutcome::Escalate
          );
          assert_eq!(evaluate_rejection(1, 1), RejectionOutcome::Escalate);
          assert_eq!(
              evaluate_rejection(0, 1),
              RejectionOutcome::ReturnToInProgress
          );
      }

      #[test]
      fn retries_readiness_twice_and_escalates_on_the_third_failure() {
          // The count includes the failure that has just happened, so the runtime never passes
          // zero; zero retries all the same rather than escalating on a count nothing produces.
          assert_eq!(evaluate_readiness_attempts(0), ReadinessOutcome::Retry);
          assert_eq!(evaluate_readiness_attempts(1), ReadinessOutcome::Retry);
          assert_eq!(evaluate_readiness_attempts(2), ReadinessOutcome::Retry);
          assert_eq!(
              evaluate_readiness_attempts(READINESS_ATTEMPT_LIMIT),
              ReadinessOutcome::Escalate
          );
          assert_eq!(evaluate_readiness_attempts(4), ReadinessOutcome::Escalate);
      }

      #[test]
      fn escalates_a_task_blocked_for_the_limit_or_longer() {
          let blocked_at = at("2026-09-16T10:00:00Z");
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-17T09:59:59Z"),
                  DEFAULT_BLOCKED_LIMIT
              ),
              BlockedAge::WithinLimit
          );
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-17T10:00:00Z"),
                  DEFAULT_BLOCKED_LIMIT
              ),
              BlockedAge::Exceeded
          );
          assert_eq!(
              evaluate_blocked_age(
                  blocked_at,
                  at("2026-09-16T10:30:00Z"),
                  Duration::from_mins(30)
              ),
              BlockedAge::Exceeded
          );
      }

      #[test]
      fn treats_a_clock_that_went_backwards_as_within_the_limit() {
          assert_eq!(
              evaluate_blocked_age(
                  at("2026-09-16T10:00:00Z"),
                  at("2026-09-15T10:00:00Z"),
                  Duration::ZERO
              ),
              BlockedAge::WithinLimit
          );
      }

      #[test]
      fn names_the_reasons_of_the_spec_on_the_wire() {
          let reasons = [
              (EscalationReason::Budget, "budget"),
              (EscalationReason::Sessions, "sessions"),
              (EscalationReason::Iterations, "iterations"),
              (EscalationReason::BlockerAge, "blocker_age"),
              (EscalationReason::Permission, "permission"),
              (EscalationReason::RiskGate, "risk_gate"),
              (EscalationReason::Approval, "approval"),
              (EscalationReason::ReadinessFailures, "readiness_failures"),
              (EscalationReason::Integration, "integration"),
              (EscalationReason::ExplicitRequest, "explicit_request"),
          ];
          for (reason, wire) in reasons {
              assert_eq!(serde_json::to_value(reason).unwrap(), json!(wire));
              assert_eq!(
                  serde_json::from_value::<EscalationReason>(json!(wire)).unwrap(),
                  reason
              );
          }
      }

      #[test]
      fn carries_the_task_the_reason_what_was_tried_and_the_options() {
          let escalation = Escalation {
              task_id: "FRK-7".parse().expect("a task id"),
              reason: EscalationReason::Budget,
              tried: "Two sessions; the second ran out of tokens.".to_string(),
              options: vec![
                  "Raise the budget.".to_string(),
                  "Split the task.".to_string(),
              ],
          };
          assert_eq!(escalation.task_id.to_string(), "FRK-7");
          assert_eq!(escalation.options.len(), 2);
          assert_eq!(escalation.reason, EscalationReason::Budget);
      }
  }
  ```
- [ ] Name the ten reasons in the spec. In `docs/SPEC.md` section 5.7, replace

  ```
  An escalation is a task state and a message to the user. It carries: the task, the reason (budget, sessions, iterations, blocker age, permission, risk gate, approval of an epic, integration, explicit request), what the agent tried, and the options the agent proposes.
  ```

  with

  ```
  An escalation is a task state and a message to the user. It carries: the task, the reason, what the agent tried, and the options the agent proposes. There are ten reasons, written on the wire as `budget`, `sessions`, `iterations`, `blocker_age`, `permission`, `risk_gate`, `approval` (an epic waiting for the user's approval of its contract, 5.16), `readiness_failures` (a contract that failed the Definition of Ready three times, 5.2), `integration` (5.14), and `explicit_request`.
  ```

  The rest of the paragraph is unchanged.

- [ ] Put the blocked-age boundary in the spec and the gate's doc. In `docs/SPEC.md` section 5.2, replace

  ```
  | blocked | escalated | Governor | blocked longer than the configured limit (default: 24 hours) |
  ```

  with

  ```
  | blocked | escalated | Governor | blocked for the configured limit or longer (default: 24 hours) |
  ```

  and in `crates/core/src/governor/transition_table.rs`, replace

  ```
      /// Blocked longer than the configured limit.
  ```

  with

  ```
      /// Blocked for the configured limit or longer.
  ```

  Both before-texts occur exactly once. Neither changes behavior: `evaluate_blocked_age` is written to `>=` and the test above pins the instant the limit is reached.

- [ ] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::escalation
  # expected, among the output:
  # test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 121 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide rejections, readiness attempts, and blocked age`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 121 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): decide rejections, readiness attempts, and blocked age
```

## Open questions

none
