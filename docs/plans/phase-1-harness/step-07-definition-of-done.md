# Phase 1, step 07: Definition of Done

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.4 (the five conditions for acceptance, and the reviewer's fresh session), section 5.16 item 4 (an epic needs the human), section 5.3 (the `human` verification method), F5
Depends on: phase 0 (merged in #4); step 02 of this phase for `contract::Verification` (committed as 21fe00a, 87a3561, 4a1ac90); step 03 for `check_allowed_paths` (220b576, 3355d35); step 06 (33202e7 and its review fix)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`farik-core` can say whether a task may be accepted: every exit criterion run by the reviewer in its own session with evidence and passed, every `human` criterion answered by the human rather than by an agent, nothing changed outside the contract's allowed paths, a completion note from the assignee, a review note from the reviewer, and the human's acceptance where the contract's risk or kind requires it. Step 09's transition evaluation calls it behind the `DefinitionOfDone` gate on `verifying -> accepted`; phase 3's orchestrator gathers the evidence it takes.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- `evaluate_done` returns `Result<(), Vec<DoneFailure>>` and reports every rule the task fails, in the order of `DoneRule`, as step 02's `evaluate_readiness` does: a reviewer told one thing at a time sends the task back once per round, and step 05's budgets taught that reporting the first of several independent failures hides the rest. Rejected: stopping at the first failure.
- A criterion whose verification method is `human` is exempt from `CriterionRunByReviewer` and judged by `HumanCriterionAccepted` instead, because spec 5.4 item 1 says such criteria "are satisfied only by an explicit human acceptance event": a reviewer cannot run them at all, so requiring its run would make every contract with a `human` criterion permanently unacceptable. It needs a result that passed with `run_by: Human`.
- A reviewer's result counts only with non-blank evidence, because spec 5.4 item 4 requires the review note to map each criterion to evidence, and a recorded result with nothing in it is the "I ran the tests and they passed" the section warns about. Whitespace is not evidence.
- `DoneEvidence.results` holds what this verification round recorded: the reviewer's own runs and the human's acceptances. The assignee's earlier run is step 08's `check_criteria_recorded` gate, not this one's evidence, so a criterion that the assignee failed and the reviewer passed is accepted, which is the point of an independent run. Changed 2026-09-16 by this plan from the project plan's `reviewer_results`, whose name said less than the field holds.
- `CriterionPassed` reads every result in the round rather than only the reviewer's, so that a `human` criterion recorded as failed also refuses.
- The diff is checked with step 03's `check_allowed_paths`, so that one set of glob semantics decides what a path means everywhere in the harness, and a contract whose `allowed_paths` do not compile refuses acceptance rather than accepting everything. A task that changed nothing passes: spec 5.4 item 2 forbids changes outside the allowed paths and says nothing about changes being required, and step 08's `CriteriaRecorded` gate is where a commit is demanded.
- `requires_human_acceptance` is public, because step 09's `ContractRequiresHuman` gate and phase 3's orchestrator ask the same question, and spec 5.4 item 5 and 5.16 item 4 give one answer: risk `high`, or kind `epic`. The team policy `human_accepts_contracts: all` of resolved question 1 is about the contract at `refining`, not about acceptance, and is phase 3's.
- A note counts as written only when it is not blank, and both notes are `Option<String>` rather than `String`, so that "the runtime has none" and "the agent wrote nothing" are the same refusal with one message.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

One task: the `governor::done` module with `RunBy`, `CriterionResult`, `DoneEvidence`, `DoneRule`, `DoneFailure`, `requires_human_acceptance`, `evaluate_done` and its seven private checks, and eleven tests.

Out of scope: the gate that calls it and the assignee's own recorded run (step 08), the transition row it serves (step 09), the events that record an acceptance (phase 2), and who is allowed to be the reviewer, which step 02's `ReviewerAvailable` and step 08's `check_assignment` already decide.

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `contract::{TaskContract, Verification}` and the generated `Kind` and `Risk` (phase 0), and `governor::paths::check_allowed_paths` (step 03). Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                         modifies: declares done
crates/core/src/governor/done.rs                    creates: RunBy, CriterionResult, DoneEvidence, DoneRule, DoneFailure, requires_human_acceptance, evaluate_done, eleven tests
docs/plans/project-plan.md                          modifies: phase 1 step 07 interface gains requires_human_acceptance and renames the evidence field (in the plan's own commit)
docs/plans/phase-1-harness/step-07-definition-of-done.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: The Definition of Done

Files: created `crates/core/src/governor/done.rs`; modified `crates/core/src/governor.rs`

Consumes: `contract::{TaskContract, Verification}`, `generated::task_contract::{FarikTaskContractKind, FarikTaskContractRisk}`, `governor::paths::{PathRefusal, check_allowed_paths}`
Produces: `governor::done::{RunBy, CriterionResult, DoneEvidence, DoneRule, DoneFailure, requires_human_acceptance, evaluate_done}`

- [ ] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 122 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// The Definition of Done of `docs/SPEC.md` section 5.4 as one function over a contract and
  /// the evidence gathered for it.
  pub mod done;
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

- [ ] Write the failing tests. `crates/core/src/governor/done.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::{
          CriterionResult, DoneEvidence, DoneRule as R, RunBy, evaluate_done,
          requires_human_acceptance,
      };
      use crate::contract::{TaskContract, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::generated::task_contract::FarikTaskContractRisk as Risk;
      use crate::governor::readiness::fixtures::a_contract;

      fn a_result(criterion_id: &str, run_by: RunBy) -> CriterionResult {
          CriterionResult {
              criterion_id: criterion_id.to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by,
          }
      }

      fn an_evidence() -> DoneEvidence {
          DoneEvidence {
              results: vec![a_result("C1", RunBy::Reviewer)],
              changed_paths: vec!["src/login/form.rs".to_string()],
              completion_note: Some("The form takes an email and a password.".to_string()),
              review_note: Some("C1: cargo test, 11 passed.".to_string()),
              human_accepted: false,
          }
      }

      fn failed_rules(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<R> {
          evaluate_done(contract, evidence)
              .expect_err("expected a refusal")
              .iter()
              .map(|failure| failure.rule)
              .collect()
      }

      fn message_of(contract: &TaskContract, evidence: &DoneEvidence, rule: R) -> String {
          evaluate_done(contract, evidence)
              .expect_err("expected a refusal")
              .into_iter()
              .find(|failure| failure.rule == rule)
              .map(|failure| failure.message)
              .expect("the rule failed")
      }

      #[test]
      fn accepts_a_task_the_reviewer_ran_and_wrote_up() {
          assert_eq!(evaluate_done(&a_contract(), &an_evidence()), Ok(()));
      }

      #[test]
      fn refuses_a_criterion_the_reviewer_did_not_run_itself() {
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Assignee)];
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
          evidence.results = Vec::new();
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_reviewer_result_without_evidence() {
          let mut evidence = an_evidence();
          evidence.results[0].evidence = "  ".to_string();
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_criterion_that_did_not_pass() {
          let mut evidence = an_evidence();
          evidence.results[0].passed = false;
          assert_eq!(failed_rules(&a_contract(), &evidence), [R::CriterionPassed]);
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CriterionPassed),
              "criteria C1 did not pass"
          );
      }

      #[test]
      fn lets_only_the_human_satisfy_a_human_criterion() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Reviewer)];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::HumanCriterionAccepted]
          );
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_diff_that_reaches_outside_the_allowed_paths() {
          let mut evidence = an_evidence();
          evidence
              .changed_paths
              .push("src/billing/invoice.rs".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::PathsWithinAllowed]
          );
          assert!(
              message_of(&a_contract(), &evidence, R::PathsWithinAllowed)
                  .starts_with("the diff changes src/billing/invoice.rs outside")
          );
      }

      #[test]
      fn accepts_a_task_that_changed_nothing_outside_its_paths_including_nothing_at_all() {
          let mut evidence = an_evidence();
          evidence.changed_paths = Vec::new();
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_missing_or_blank_completion_note() {
          let mut evidence = an_evidence();
          evidence.completion_note = None;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CompletionNotePresent]
          );
          evidence.completion_note = Some(" ".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CompletionNotePresent]
          );
      }

      #[test]
      fn refuses_a_missing_or_blank_review_note() {
          let mut evidence = an_evidence();
          evidence.review_note = None;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::ReviewNotePresent]
          );
          evidence.review_note = Some("\n".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::ReviewNotePresent]
          );
      }

      #[test]
      fn needs_the_human_for_a_high_risk_task_and_for_every_epic() {
          let contract = a_contract();
          assert!(!requires_human_acceptance(&contract));
          let mut risky = a_contract();
          risky.risk = Risk::High;
          assert!(requires_human_acceptance(&risky));
          assert_eq!(failed_rules(&risky, &an_evidence()), [R::HumanAccepted]);
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          assert!(requires_human_acceptance(&epic));
          assert_eq!(failed_rules(&epic, &an_evidence()), [R::HumanAccepted]);
          let mut evidence = an_evidence();
          evidence.human_accepted = true;
          assert_eq!(evaluate_done(&risky, &evidence), Ok(()));
          assert_eq!(evaluate_done(&epic, &evidence), Ok(()));
      }

      #[test]
      fn reports_every_failure_in_rule_order() {
          let mut contract = a_contract();
          contract.risk = Risk::High;
          let mut evidence = an_evidence();
          evidence.results = Vec::new();
          evidence.changed_paths = vec!["README.md".to_string()];
          evidence.completion_note = None;
          evidence.review_note = None;
          assert_eq!(
              failed_rules(&contract, &evidence),
              [
                  R::CriterionRunByReviewer,
                  R::PathsWithinAllowed,
                  R::CompletionNotePresent,
                  R::ReviewNotePresent,
                  R::HumanAccepted
              ]
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::done
  # expected, among the output:
  # error[E0432]: unresolved imports `super::CriterionResult`, `super::DoneEvidence`, `super::DoneRule`, `super::RunBy`, `super::evaluate_done`, `super::requires_human_acceptance`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [ ] Write the implementation above the tests. `crates/core/src/governor/done.rs` in full:

  ```rust
  //! The Definition of Done (`docs/SPEC.md` section 5.4): what a reviewer must have run, what the
  //! diff may touch, what was written down, and when the human must accept, as one function over a
  //! contract and the evidence gathered for it.

  use crate::contract::{TaskContract, Verification};
  use crate::generated::task_contract::FarikTaskContractKind as Kind;
  use crate::generated::task_contract::FarikTaskContractRisk as Risk;
  use crate::governor::paths::{PathRefusal, check_allowed_paths};

  /// Who ran a criterion.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum RunBy {
      /// The agent assigned to the task.
      Assignee,
      /// The agent reviewing the task, in its own session.
      Reviewer,
      /// The user.
      Human,
  }

  /// One criterion's result, as the runtime recorded it.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct CriterionResult {
      /// The criterion's id within the contract.
      pub criterion_id: String,
      /// Whether it passed.
      pub passed: bool,
      /// What shows it: a command's output, a file path, a test name.
      pub evidence: String,
      /// Who ran it.
      pub run_by: RunBy,
  }

  /// Everything the Definition of Done is judged on, gathered by the runtime.
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub struct DoneEvidence {
      /// The results recorded in this verification round: the reviewer's own runs and the
      /// human's acceptances. The assignee's earlier run is step 08's gate, not this one's evidence.
      pub results: Vec<CriterionResult>,
      /// The paths the task's diff touches.
      pub changed_paths: Vec<String>,
      /// The assignee's completion note.
      pub completion_note: Option<String>,
      /// The reviewer's note mapping each criterion to its evidence.
      pub review_note: Option<String>,
      /// Whether the human has accepted the task.
      pub human_accepted: bool,
  }

  /// One rule of the Definition of Done. Failures are reported in this order.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum DoneRule {
      /// Every criterion has a result from the reviewer's own run, with evidence.
      CriterionRunByReviewer,
      /// Every criterion passed.
      CriterionPassed,
      /// Every `human` criterion was accepted by the human, not by an agent.
      HumanCriterionAccepted,
      /// The diff touches nothing outside the contract's allowed paths.
      PathsWithinAllowed,
      /// The assignee wrote a completion note.
      CompletionNotePresent,
      /// The reviewer wrote a review note.
      ReviewNotePresent,
      /// The human accepted, which a `high` risk task and every epic require.
      HumanAccepted,
  }

  /// One rule the task fails, with a message that says what is missing.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct DoneFailure {
      /// The rule that failed.
      pub rule: DoneRule,
      /// What is wrong and what would fix it.
      pub message: String,
  }

  /// Whether the contract needs the human's acceptance: `high` risk, or an epic (`docs/SPEC.md`
  /// sections 5.4 item 5 and 5.16).
  #[must_use]
  pub fn requires_human_acceptance(contract: &TaskContract) -> bool {
      contract.risk == Risk::High || contract.kind == Kind::Epic
  }

  type Check = fn(&TaskContract, &DoneEvidence) -> Vec<DoneFailure>;

  const CHECKS: [Check; 7] = [
      criterion_run_by_reviewer,
      criterion_passed,
      human_criterion_accepted,
      paths_within_allowed,
      completion_note_present,
      review_note_present,
      human_accepted,
  ];

  /// Checks a task against the Definition of Done (`docs/SPEC.md` section 5.4): every exit
  /// criterion run by the reviewer independently and passed, every `human` criterion accepted by
  /// the human, no change outside the contract's allowed paths, a completion note, a review note,
  /// and the human's acceptance where the risk or the kind requires it. Refuses with every rule the
  /// task fails.
  ///
  /// # Errors
  ///
  /// Every failed rule, in the order of `DoneRule`, each with a message that says what is missing.
  pub fn evaluate_done(
      contract: &TaskContract,
      evidence: &DoneEvidence,
  ) -> Result<(), Vec<DoneFailure>> {
      let failures: Vec<DoneFailure> = CHECKS
          .iter()
          .flat_map(|check| check(contract, evidence))
          .collect();
      if failures.is_empty() {
          Ok(())
      } else {
          Err(failures)
      }
  }

  fn failure(rule: DoneRule, message: String) -> DoneFailure {
      DoneFailure { rule, message }
  }

  fn one(rule: DoneRule, message: String) -> Vec<DoneFailure> {
      vec![failure(rule, message)]
  }

  /// The result the reviewer recorded for a criterion, if any.
  fn reviewer_result<'a>(
      evidence: &'a DoneEvidence,
      criterion_id: &str,
  ) -> Option<&'a CriterionResult> {
      evidence
          .results
          .iter()
          .find(|result| result.criterion_id == criterion_id && result.run_by == RunBy::Reviewer)
  }

  fn criterion_run_by_reviewer(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      let missing: Vec<String> = contract
          .exit_criteria
          .iter()
          .map(|criterion| criterion.id.to_string())
          .filter(|id| {
              !matches!(
                  verification_of(contract, id),
                  Some(Verification::Human { .. })
              ) && reviewer_result(evidence, id)
                  .is_none_or(|result| result.evidence.trim().is_empty())
          })
          .collect();
      if missing.is_empty() {
          return Vec::new();
      }
      one(
          DoneRule::CriterionRunByReviewer,
          format!(
              "criteria {} have no result with evidence from the reviewer's own run",
              missing.join(", ")
          ),
      )
  }

  fn verification_of(contract: &TaskContract, criterion_id: &str) -> Option<Verification> {
      contract
          .exit_criteria
          .iter()
          .find(|criterion| criterion.id.as_str() == criterion_id)
          .map(|criterion| Verification::from(&criterion.verification))
  }

  fn criterion_passed(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      let failed: Vec<String> = contract
          .exit_criteria
          .iter()
          .map(|criterion| criterion.id.to_string())
          .filter(|id| {
              evidence
                  .results
                  .iter()
                  .any(|result| &result.criterion_id == id && !result.passed)
          })
          .collect();
      if failed.is_empty() {
          return Vec::new();
      }
      one(
          DoneRule::CriterionPassed,
          format!("criteria {} did not pass", failed.join(", ")),
      )
  }

  fn human_criterion_accepted(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      let missing: Vec<String> = contract
          .exit_criteria
          .iter()
          .filter(|criterion| {
              matches!(
                  Verification::from(&criterion.verification),
                  Verification::Human { .. }
              )
          })
          .map(|criterion| criterion.id.to_string())
          .filter(|id| {
              !evidence.results.iter().any(|result| {
                  &result.criterion_id == id && result.passed && result.run_by == RunBy::Human
              })
          })
          .collect();
      if missing.is_empty() {
          return Vec::new();
      }
      one(
          DoneRule::HumanCriterionAccepted,
          format!(
              "criteria {} ask the human a question and only the human can answer it",
              missing.join(", ")
          ),
      )
  }

  fn paths_within_allowed(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      match check_allowed_paths(&evidence.changed_paths, &contract.allowed_paths) {
          Ok(()) => Vec::new(),
          Err(PathRefusal::Violations(violations)) => one(
              DoneRule::PathsWithinAllowed,
              format!(
                  "the diff changes {} outside the contract's allowed paths {}",
                  violations
                      .iter()
                      .map(|violation| violation.path.clone())
                      .collect::<Vec<String>>()
                      .join(", "),
                  contract.allowed_paths.join(", ")
              ),
          ),
          Err(PathRefusal::Glob(error)) => one(
              DoneRule::PathsWithinAllowed,
              format!("the contract's allowed paths cannot be read: {error:?}"),
          ),
      }
  }

  fn is_written(note: Option<&String>) -> bool {
      note.is_some_and(|text| !text.trim().is_empty())
  }

  fn completion_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      if is_written(evidence.completion_note.as_ref()) {
          return Vec::new();
      }
      one(
          DoneRule::CompletionNotePresent,
          "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first".to_string(),
      )
  }

  fn review_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      if is_written(evidence.review_note.as_ref()) {
          return Vec::new();
      }
      one(
          DoneRule::ReviewNotePresent,
          "the reviewer wrote no review note mapping each criterion to its evidence".to_string(),
      )
  }

  fn human_accepted(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<DoneFailure> {
      if !requires_human_acceptance(contract) || evidence.human_accepted {
          return Vec::new();
      }
      let why = if contract.kind == Kind::Epic {
          "an epic"
      } else {
          "a high risk task"
      };
      one(
          DoneRule::HumanAccepted,
          format!("{why} is accepted by the human, and the human has not accepted"),
      )
  }

  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::{
          CriterionResult, DoneEvidence, DoneRule as R, RunBy, evaluate_done,
          requires_human_acceptance,
      };
      use crate::contract::{TaskContract, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::generated::task_contract::FarikTaskContractRisk as Risk;
      use crate::governor::readiness::fixtures::a_contract;

      fn a_result(criterion_id: &str, run_by: RunBy) -> CriterionResult {
          CriterionResult {
              criterion_id: criterion_id.to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by,
          }
      }

      fn an_evidence() -> DoneEvidence {
          DoneEvidence {
              results: vec![a_result("C1", RunBy::Reviewer)],
              changed_paths: vec!["src/login/form.rs".to_string()],
              completion_note: Some("The form takes an email and a password.".to_string()),
              review_note: Some("C1: cargo test, 11 passed.".to_string()),
              human_accepted: false,
          }
      }

      fn failed_rules(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<R> {
          evaluate_done(contract, evidence)
              .expect_err("expected a refusal")
              .iter()
              .map(|failure| failure.rule)
              .collect()
      }

      fn message_of(contract: &TaskContract, evidence: &DoneEvidence, rule: R) -> String {
          evaluate_done(contract, evidence)
              .expect_err("expected a refusal")
              .into_iter()
              .find(|failure| failure.rule == rule)
              .map(|failure| failure.message)
              .expect("the rule failed")
      }

      #[test]
      fn accepts_a_task_the_reviewer_ran_and_wrote_up() {
          assert_eq!(evaluate_done(&a_contract(), &an_evidence()), Ok(()));
      }

      #[test]
      fn refuses_a_criterion_the_reviewer_did_not_run_itself() {
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Assignee)];
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
          evidence.results = Vec::new();
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_reviewer_result_without_evidence() {
          let mut evidence = an_evidence();
          evidence.results[0].evidence = "  ".to_string();
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_criterion_that_did_not_pass() {
          let mut evidence = an_evidence();
          evidence.results[0].passed = false;
          assert_eq!(failed_rules(&a_contract(), &evidence), [R::CriterionPassed]);
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CriterionPassed),
              "criteria C1 did not pass"
          );
      }

      #[test]
      fn lets_only_the_human_satisfy_a_human_criterion() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Reviewer)];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::HumanCriterionAccepted]
          );
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_diff_that_reaches_outside_the_allowed_paths() {
          let mut evidence = an_evidence();
          evidence
              .changed_paths
              .push("src/billing/invoice.rs".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::PathsWithinAllowed]
          );
          assert!(
              message_of(&a_contract(), &evidence, R::PathsWithinAllowed)
                  .starts_with("the diff changes src/billing/invoice.rs outside")
          );
      }

      #[test]
      fn accepts_a_task_that_changed_nothing_outside_its_paths_including_nothing_at_all() {
          let mut evidence = an_evidence();
          evidence.changed_paths = Vec::new();
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_missing_or_blank_completion_note() {
          let mut evidence = an_evidence();
          evidence.completion_note = None;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CompletionNotePresent]
          );
          evidence.completion_note = Some(" ".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CompletionNotePresent]
          );
      }

      #[test]
      fn refuses_a_missing_or_blank_review_note() {
          let mut evidence = an_evidence();
          evidence.review_note = None;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::ReviewNotePresent]
          );
          evidence.review_note = Some("\n".to_string());
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::ReviewNotePresent]
          );
      }

      #[test]
      fn needs_the_human_for_a_high_risk_task_and_for_every_epic() {
          let contract = a_contract();
          assert!(!requires_human_acceptance(&contract));
          let mut risky = a_contract();
          risky.risk = Risk::High;
          assert!(requires_human_acceptance(&risky));
          assert_eq!(failed_rules(&risky, &an_evidence()), [R::HumanAccepted]);
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          assert!(requires_human_acceptance(&epic));
          assert_eq!(failed_rules(&epic, &an_evidence()), [R::HumanAccepted]);
          let mut evidence = an_evidence();
          evidence.human_accepted = true;
          assert_eq!(evaluate_done(&risky, &evidence), Ok(()));
          assert_eq!(evaluate_done(&epic, &evidence), Ok(()));
      }

      #[test]
      fn reports_every_failure_in_rule_order() {
          let mut contract = a_contract();
          contract.risk = Risk::High;
          let mut evidence = an_evidence();
          evidence.results = Vec::new();
          evidence.changed_paths = vec!["README.md".to_string()];
          evidence.completion_note = None;
          evidence.review_note = None;
          assert_eq!(
              failed_rules(&contract, &evidence),
              [
                  R::CriterionRunByReviewer,
                  R::PathsWithinAllowed,
                  R::CompletionNotePresent,
                  R::ReviewNotePresent,
                  R::HumanAccepted
              ]
          );
      }
  }
  ```

- [ ] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::done
  # expected, among the output:
  # test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 122 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 133 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide whether a task meets the definition of done`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 133 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): decide whether a task meets the definition of done
```

## Open questions

none
