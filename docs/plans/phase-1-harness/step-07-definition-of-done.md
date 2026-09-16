# Phase 1, step 07: Definition of Done

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.4 (the five conditions for acceptance, and the reviewer's fresh session), section 5.16 item 4 (an epic needs the human), section 5.3 (the `human` verification method, and the structural rule this step adds: exit criteria have distinct ids), F5
Depends on: phase 0 (merged in #4); step 02 of this phase for `contract::Verification` (committed as 21fe00a, 87a3561, 4a1ac90); step 03 for `check_allowed_paths` (220b576, 3355d35); step 06 (33202e7 and its review fix abfb7d8)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`farik-core` can say whether a task may be accepted: every exit criterion run by the reviewer in its own session with evidence and passed, every `human` criterion answered by the human rather than by an agent, nothing changed outside the contract's allowed paths, a completion note from the assignee, a review note from the reviewer, and the human's acceptance where the contract's risk or kind requires it. Step 09's transition evaluation calls it behind the `DefinitionOfDone` gate on `verifying -> accepted`; phase 3's orchestrator gathers the evidence it takes. The step also closes the gap that makes the first rule decidable at all: a contract whose exit criteria share an id is refused by the Definition of Ready, because every result, note, and event names a criterion by its id and cannot tell two apart.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- `evaluate_done` returns `Result<(), Vec<DoneFailure>>` and reports every rule the task fails, in the order of `DoneRule`, as step 02's `evaluate_readiness` does: a reviewer told one thing at a time sends the task back once per round, and step 05's budgets taught that reporting the first of several independent failures hides the rest. Rejected: stopping at the first failure.
- Two exit criteria may share an id and nothing refused it: the schema's `id` is a pattern, JSON Schema 2020-12 cannot express uniqueness by property, `validate_contract` accepts it, and no readiness rule looked. That is not a cosmetic slip. Results, notes, and events all name a criterion by its id, so N criteria sharing one are indistinguishable to every check here and one recorded run is credited to all of them: a task is accepted with a criterion nobody ran. Task 2 refuses such a contract at the Definition of Ready, which is the boundary every task crosses before it can be assigned, so the ambiguity never reaches this function.
- Task 1's checks nevertheless read the criterion they hold and never look a verification back up by id, and a test with two `C1` criteria pins it. That is defence in depth, not the fix: `TaskContract`'s fields are public and a runtime can build one itself, so this module refuses what it can see rather than trusting its caller. What it cannot see is two criteria with one id and one result between them, which is why task 2 exists.
- A criterion whose verification method is `human` is exempt from `CriterionRunByReviewer` and judged by `HumanCriterionAccepted` instead, because spec 5.4 item 1 says such criteria "are satisfied only by an explicit human acceptance event": a reviewer cannot run them at all, so requiring its run would make every contract with a `human` criterion permanently unacceptable. It needs a result that passed with `run_by: Human`.
- A reviewer's result counts only with non-blank evidence, because spec 5.4 item 4 requires the review note to map each criterion to evidence, and a recorded result with nothing in it is the "I ran the tests and they passed" the section warns about. Whitespace is not evidence, with the same floor the notes have: `trim` follows Unicode White_Space, so a non-breaking or ideographic space is refused and a zero-width one is not.
- `DoneEvidence.results` holds what this verification round recorded: the reviewer's own runs and the human's acceptances. The assignee's earlier run is step 08's `check_criteria_recorded` gate, which takes it as its own parameter, so a criterion the assignee failed and the reviewer passed is accepted: the independent run is the point of spec 5.4 item 1. The type cannot stop a runtime from putting an assignee's result here, so every check ignores one rather than trusting the field to be clean. Changed 2026-09-16 by this plan from the project plan's `reviewer_results`, whose name said less than the field holds.
- A result whose `criterion_id` names no criterion of the contract decides nothing, including when it failed: the contract's criteria are the list, and anything else is noise the runtime recorded. The criteria that are in the contract still have to be run and to pass, which the other rules enforce. A test pins it.
- `CriterionPassed` reads every result that is not the assignee's, rather than only the reviewer's, so that a `human` criterion recorded as failed also refuses; and it ignores the assignee's, so that its earlier failure does not outvote the reviewer's own run. Both directions have a test, because a check restricted to the reviewer alone passed the whole suite when the plan was first reviewed.
- The diff is checked with step 03's `check_allowed_paths`, so that one set of glob semantics decides what a path means everywhere in the harness, and a contract whose `allowed_paths` do not compile refuses acceptance rather than accepting everything. A task that changed nothing passes: spec 5.4 item 2 forbids changes outside the allowed paths and says nothing about changes being required, and step 08's `CriteriaRecorded` gate is where a commit is demanded.
- `requires_human_acceptance` answers one question and says so in its doc: must the human accept the finished result before the task is accepted? Spec 5.4 item 5 and 5.16 item 4 give the answer, risk `high` or kind `epic`, and no team policy touches it. Its consumers are this step's `HumanAccepted` rule and phase 3's orchestrator; it is public so that they agree. It is deliberately **not** the answer to step 09's `ContractRequiresHuman` gate, which is the human's approval of the contract *before* work starts: project plan D8 widens that one to every task under `human_accepts_contracts: all`, and step 09 takes it as the context field `contract_requires_human_acceptance` rather than calling this function. The schema's `risk` description names the two moments separately, and conflating them would either silence the policy or make high-risk acceptance policy-dependent.
- A note counts as written only when it is not blank, and both notes are `Option<String>` rather than `String`, so that "the runtime has none" and "the agent wrote nothing" are the same refusal with one message. `trim` is a floor, not a judgment of quality: it follows Unicode White_Space, so it catches a non-breaking or ideographic space but not a zero-width one, and the substance spec 5.4 item 4 asks for is carried by each result's evidence rather than by the note's length.
- An empty `allowed_paths` makes every changed path a violation, and its message says so rather than ending in a dangling list. Like the assignee's results, it is unreachable through `validate_contract` (`minItems: 1`) and reachable by hand, so it is checked rather than assumed, with a test.
- Each check returns `Option<DoneFailure>` and `evaluate_done` collects with `filter_map`, the shape step 02's `evaluate_readiness` already has, so the two halves of the governor read the same way.
- A contract with no exit criteria at all passes the two criterion rules vacuously. That is unreachable rather than decided: the schema sets `minItems: 1` on `exit_criteria`, `validate_contract` refuses a contract without one, and step 02's `CriteriaPresent` rule refuses it again before the task is ever assigned.
- A `human` result needs no evidence while a reviewer's does, because the acceptance event is the evidence: spec 5.4 item 1 asks for an explicit human acceptance, not for the human to write up a command's output.
- Revised twice on 2026-09-16, after two readiness reviews. The second found that the first fix was narrower than it looked: verifications were no longer looked up by id, but results still were, so a contract with a `test` and a `command` criterion both called `C1` was accepted on one recorded run. Task 2 is that finding's answer. The second review also found the project plan still asserting the rationale this plan had replaced, the empty-`allowed_paths` branch untested with a surviving mutation, a message that reports an epic as a high risk task with the suite green, and plural wording for a single criterion. All taken. The first review found the `human`-exemption lookup accepting a criterion nobody ran when two criteria share an id (critical), the stated decision about the assignee's failure contradicted by the code, the invalid-glob refusal and the assignee rule both survivable as mutations, and the `requires_human_acceptance` rationale conflating the contract's approval with the result's acceptance. All taken; the glob refusal now reads in plain English and a contract that allows no path at all no longer ends its message in a dangling list.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

Task 1: the `governor::done` module with `RunBy`, `CriterionResult`, `DoneEvidence`, `DoneRule`, `DoneFailure`, `requires_human_acceptance`, `evaluate_done` and its seven private checks, and sixteen tests.

Task 2: one more rule in `governor::readiness`, `CriteriaIdsUnique`, with its check, its test, its line in spec 5.3's structural list, and its entry in the project plan.

Out of scope: the gate that calls it and the assignee's own recorded run (step 08), the transition row it serves (step 09), the events that record an acceptance (phase 2), and who is allowed to be the reviewer, which step 02's `ReviewerAvailable` and step 08's `check_assignment` already decide. Also out of scope, and worth naming because this step trusts them: nothing here ties `RunBy::Human` to a `human.accepted` event, which is phase 2's, or the `Reviewer` label to the fresh session spec 5.4's closing paragraph requires, which is phase 3's. `CriterionResult` is what the runtime recorded, and phase 1 takes it at its word.

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `contract::{TaskContract, Verification}` and the generated `Kind` and `Risk` (phase 0), and `governor::paths::check_allowed_paths` (step 03). Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                         modifies: declares done (task 1)
crates/core/src/governor/done.rs                    creates: RunBy, CriterionResult, DoneEvidence, DoneRule, DoneFailure, requires_human_acceptance, evaluate_done, sixteen tests (task 1)
crates/core/src/governor/readiness.rs               modifies: the CriteriaIdsUnique rule, its check, and its test (task 2)
docs/SPEC.md                                        modifies: section 5.3's structural list gains the distinct-ids line (task 2)
docs/plans/project-plan.md                          modifies: phase 1 step 07 interface gains requires_human_acceptance and renames the evidence field (in the plan's own commit); step 02's ReadinessRule gains CriteriaIdsUnique and step 09's entry stops claiming requires_human_acceptance answers ContractRequiresHuman (task 2)
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
              "criterion C1 did not pass"
          );
      }

      #[test]
      fn accepts_what_the_reviewer_passed_although_the_assignee_had_failed_it() {
          // The independent run is the point: the assignee's earlier failure is step 08's gate, and
          // a reviewer that ran the criterion itself and watched it pass is what 5.4 item 1 asks.
          let mut evidence = an_evidence();
          let mut failed = a_result("C1", RunBy::Assignee);
          failed.passed = false;
          evidence.results.push(failed);
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_criterion_whose_id_is_shared_with_a_human_one() {
          // Two criteria called C1, the human one first. Reading a verification back up by id would
          // hand the human exemption to the second and accept a criterion nobody ran.
          let mut contract = a_contract();
          let mut human = contract.exit_criteria[0].clone();
          human.verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          contract.exit_criteria.insert(0, human);
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_diff_when_the_contract_allows_no_path_at_all() {
          // `validate_contract` refuses an empty `allowed_paths` (minItems 1), so this is defence in
          // depth against a runtime that builds a contract itself: every field of TaskContract is
          // public, and the same argument keeps the assignee's results from being trusted.
          let mut contract = a_contract();
          contract.allowed_paths = Vec::new();
          assert_eq!(
              failed_rules(&contract, &an_evidence()),
              [R::PathsWithinAllowed]
          );
          assert_eq!(
              message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
              "the diff changes src/login/form.rs, and the contract allows no path at all"
          );
      }

      #[test]
      fn ignores_a_result_that_names_no_criterion_of_this_contract() {
          // The contract's criteria are the list; a result for anything else is noise from the
          // runtime and decides nothing, including when it failed. The criteria that are in the
          // contract still have to be run and pass, which the other tests pin.
          let mut evidence = an_evidence();
          let mut stray = a_result("C9", RunBy::Reviewer);
          stray.passed = false;
          evidence.results.push(stray);
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_allowed_paths_that_do_not_compile() {
          let mut contract = a_contract();
          contract.allowed_paths = vec!["src/[".to_string()];
          assert_eq!(
              failed_rules(&contract, &an_evidence()),
              [R::PathsWithinAllowed]
          );
          assert_eq!(
              message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
              "the contract's allowed path src/[ is not a valid glob: unclosed character class; missing ']'"
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
          // A human answer recorded as a failure refuses too, which is why the passed check reads
          // every result that is not the assignee's rather than only the reviewer's.
          let mut refused = a_result("C1", RunBy::Human);
          refused.passed = false;
          evidence.results = vec![refused];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::CriterionPassed, R::HumanCriterionAccepted]
          );
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
          assert_eq!(
              message_of(&risky, &an_evidence(), R::HumanAccepted),
              "a high risk task is accepted by the human, and the human has not accepted"
          );
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          assert!(requires_human_acceptance(&epic));
          assert_eq!(failed_rules(&epic, &an_evidence()), [R::HumanAccepted]);
          assert_eq!(
              message_of(&epic, &an_evidence(), R::HumanAccepted),
              "an epic is accepted by the human, and the human has not accepted"
          );
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
  use crate::governor::paths::{GlobError, PathRefusal, check_allowed_paths};

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

  /// Whether the human must accept the finished result before the task is accepted: the contract's
  /// risk is `high`, or it is an epic (`docs/SPEC.md` section 5.4 item 5 and section 5.16 item 4).
  /// No team policy touches this.
  ///
  /// This is not the human's approval of the contract before work starts, which spec 5.2's
  /// `refining -> escalated` gate asks and the team policy `human_accepts_contracts` widens to every
  /// task; step 09 takes that answer from its context rather than from here.
  #[must_use]
  pub fn requires_human_acceptance(contract: &TaskContract) -> bool {
      contract.risk == Risk::High || contract.kind == Kind::Epic
  }

  type Check = fn(&TaskContract, &DoneEvidence) -> Option<DoneFailure>;

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
          .filter_map(|check| check(contract, evidence))
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

  /// "criterion C1" or "criteria C1, C2", so that a message reads as English either way.
  fn listed(ids: &[String]) -> String {
      if ids.len() == 1 {
          format!("criterion {}", ids[0])
      } else {
          format!("criteria {}", ids.join(", "))
      }
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

  fn criterion_run_by_reviewer(
      contract: &TaskContract,
      evidence: &DoneEvidence,
  ) -> Option<DoneFailure> {
      let missing: Vec<String> = contract
          .exit_criteria
          .iter()
          .filter(|criterion| {
              !matches!(
                  Verification::from(&criterion.verification),
                  Verification::Human { .. }
              )
          })
          .map(|criterion| criterion.id.to_string())
          .filter(|id| {
              reviewer_result(evidence, id).is_none_or(|result| result.evidence.trim().is_empty())
          })
          .collect();
      if missing.is_empty() {
          return None;
      }
      Some(failure(
          DoneRule::CriterionRunByReviewer,
          format!(
              "the reviewer's own run recorded no evidence for {}",
              listed(&missing)
          ),
      ))
  }

  fn criterion_passed(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
      let failed: Vec<String> = contract
          .exit_criteria
          .iter()
          .map(|criterion| criterion.id.to_string())
          .filter(|id| {
              evidence.results.iter().any(|result| {
                  &result.criterion_id == id && result.run_by != RunBy::Assignee && !result.passed
              })
          })
          .collect();
      if failed.is_empty() {
          return None;
      }
      Some(failure(
          DoneRule::CriterionPassed,
          format!("{} did not pass", listed(&failed)),
      ))
  }

  fn human_criterion_accepted(
      contract: &TaskContract,
      evidence: &DoneEvidence,
  ) -> Option<DoneFailure> {
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
          return None;
      }
      Some(failure(
          DoneRule::HumanCriterionAccepted,
          format!(
              "the human has not answered {}, and only the human can",
              listed(&missing)
          ),
      ))
  }

  fn paths_within_allowed(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
      match check_allowed_paths(&evidence.changed_paths, &contract.allowed_paths) {
          Ok(()) => None,
          Err(PathRefusal::Violations(violations)) => {
              let changed = violations
                  .iter()
                  .map(|violation| violation.path.clone())
                  .collect::<Vec<String>>()
                  .join(", ");
              Some(failure(
                  DoneRule::PathsWithinAllowed,
                  if contract.allowed_paths.is_empty() {
                      format!("the diff changes {changed}, and the contract allows no path at all")
                  } else {
                      format!(
                          "the diff changes {changed} outside the contract's allowed paths {}",
                          contract.allowed_paths.join(", ")
                      )
                  },
              ))
          }
          Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) => Some(failure(
              DoneRule::PathsWithinAllowed,
              format!("the contract's allowed path {pattern} is not a valid glob: {detail}"),
          )),
      }
  }

  fn is_written(note: Option<&String>) -> bool {
      note.is_some_and(|text| !text.trim().is_empty())
  }

  fn completion_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
      if is_written(evidence.completion_note.as_ref()) {
          return None;
      }
      Some(failure(
          DoneRule::CompletionNotePresent,
          "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first".to_string(),
      ))
  }

  fn review_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
      if is_written(evidence.review_note.as_ref()) {
          return None;
      }
      Some(failure(
          DoneRule::ReviewNotePresent,
          "the reviewer wrote no review note mapping each criterion to its evidence".to_string(),
      ))
  }

  fn human_accepted(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
      if !requires_human_acceptance(contract) || evidence.human_accepted {
          return None;
      }
      let why = if contract.kind == Kind::Epic {
          "an epic"
      } else {
          "a high risk task"
      };
      Some(failure(
          DoneRule::HumanAccepted,
          format!("{why} is accepted by the human, and the human has not accepted"),
      ))
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
              "criterion C1 did not pass"
          );
      }

      #[test]
      fn accepts_what_the_reviewer_passed_although_the_assignee_had_failed_it() {
          // The independent run is the point: the assignee's earlier failure is step 08's gate, and
          // a reviewer that ran the criterion itself and watched it pass is what 5.4 item 1 asks.
          let mut evidence = an_evidence();
          let mut failed = a_result("C1", RunBy::Assignee);
          failed.passed = false;
          evidence.results.push(failed);
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_a_criterion_whose_id_is_shared_with_a_human_one() {
          // Two criteria called C1, the human one first. Reading a verification back up by id would
          // hand the human exemption to the second and accept a criterion nobody ran.
          let mut contract = a_contract();
          let mut human = contract.exit_criteria[0].clone();
          human.verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          contract.exit_criteria.insert(0, human);
          let mut evidence = an_evidence();
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::CriterionRunByReviewer]
          );
      }

      #[test]
      fn refuses_a_diff_when_the_contract_allows_no_path_at_all() {
          // `validate_contract` refuses an empty `allowed_paths` (minItems 1), so this is defence in
          // depth against a runtime that builds a contract itself: every field of TaskContract is
          // public, and the same argument keeps the assignee's results from being trusted.
          let mut contract = a_contract();
          contract.allowed_paths = Vec::new();
          assert_eq!(
              failed_rules(&contract, &an_evidence()),
              [R::PathsWithinAllowed]
          );
          assert_eq!(
              message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
              "the diff changes src/login/form.rs, and the contract allows no path at all"
          );
      }

      #[test]
      fn ignores_a_result_that_names_no_criterion_of_this_contract() {
          // The contract's criteria are the list; a result for anything else is noise from the
          // runtime and decides nothing, including when it failed. The criteria that are in the
          // contract still have to be run and pass, which the other tests pin.
          let mut evidence = an_evidence();
          let mut stray = a_result("C9", RunBy::Reviewer);
          stray.passed = false;
          evidence.results.push(stray);
          assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
      }

      #[test]
      fn refuses_allowed_paths_that_do_not_compile() {
          let mut contract = a_contract();
          contract.allowed_paths = vec!["src/[".to_string()];
          assert_eq!(
              failed_rules(&contract, &an_evidence()),
              [R::PathsWithinAllowed]
          );
          assert_eq!(
              message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
              "the contract's allowed path src/[ is not a valid glob: unclosed character class; missing ']'"
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
          // A human answer recorded as a failure refuses too, which is why the passed check reads
          // every result that is not the assignee's rather than only the reviewer's.
          let mut refused = a_result("C1", RunBy::Human);
          refused.passed = false;
          evidence.results = vec![refused];
          assert_eq!(
              failed_rules(&contract, &evidence),
              [R::CriterionPassed, R::HumanCriterionAccepted]
          );
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
          assert_eq!(
              message_of(&risky, &an_evidence(), R::HumanAccepted),
              "a high risk task is accepted by the human, and the human has not accepted"
          );
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          assert!(requires_human_acceptance(&epic));
          assert_eq!(failed_rules(&epic, &an_evidence()), [R::HumanAccepted]);
          assert_eq!(
              message_of(&epic, &an_evidence(), R::HumanAccepted),
              "an epic is accepted by the human, and the human has not accepted"
          );
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
  # test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 122 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 138 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide whether a task meets the definition of done`

### Task 2: Exit criteria have distinct ids

Files: modified `crates/core/src/governor/readiness.rs`, `docs/SPEC.md`, `docs/plans/project-plan.md`

Consumes: `governor::readiness::{ReadinessRule, ReadinessFailure, ReadinessContext}` from step 02
Produces: `governor::readiness::ReadinessRule::CriteriaIdsUnique`

- [ ] Confirm task 1 landed:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 138 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Write the failing test. In `crates/core/src/governor/readiness.rs`, insert before `fn a_task_under(parent: ParentState)` in the test module:

  ```rust
      #[test]
      fn refuses_two_exit_criteria_that_share_an_id() {
          // A result, a note, and an event all name a criterion by its id, so two criteria with one
          // id are indistinguishable downstream: one recorded run would be credited to both, and a
          // task could be accepted with a criterion nobody ran.
          let mut contract = a_contract();
          let twin = contract.exit_criteria[0].clone();
          contract.exit_criteria.push(twin);
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CriteriaIdsUnique]
          );
          assert_eq!(
              message_of(&contract, &a_ready_context(), R::CriteriaIdsUnique),
              "the ids C1 name more than one exit criterion; give each its own, because a result, a note, and an event all name a criterion by its id and cannot tell two apart"
          );
      }
  ```

- [ ] Run it and confirm it fails because the rule does not exist:

  ```
  cargo test --package farik-core governor::readiness
  # expected, among the output:
  # error[E0599]: no variant, associated function, or constant named `CriteriaIdsUnique` found for enum `ReadinessRule` in the current scope
  # error[E0599]: no variant, associated function, or constant named `CriteriaIdsUnique` found for enum `ReadinessRule` in the current scope
  # error: could not compile `farik-core` (lib test) due to 2 previous errors
  ```

- [ ] Add the rule. In `crates/core/src/governor/readiness.rs`, replace

  ```rust
      /// At least one exit criterion exists.
      CriteriaPresent,
  ```

  with

  ```rust
      /// At least one exit criterion exists.
      CriteriaPresent,
      /// No two exit criteria share an `id`.
      CriteriaIdsUnique,
  ```

  replace

  ```rust
  const CHECKS: [Check; 19] = [
      intent_present,
      criteria_present,
      criteria_methods_valid,
  ```

  with

  ```rust
  const CHECKS: [Check; 20] = [
      intent_present,
      criteria_present,
      criteria_ids_unique,
      criteria_methods_valid,
  ```

  and insert before `fn criteria_methods_valid(`:

  ```rust
  fn criteria_ids_unique(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
      let mut seen: BTreeSet<&str> = BTreeSet::new();
      let repeated: BTreeSet<&str> = contract
          .exit_criteria
          .iter()
          .map(|criterion| criterion.id.as_str())
          .filter(|id| !seen.insert(id))
          .collect();
      if repeated.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::CriteriaIdsUnique,
          format!(
              "the ids {} name more than one exit criterion; give each its own, because a result, \
               a note, and an event all name a criterion by its id and cannot tell two apart",
              repeated.into_iter().collect::<Vec<&str>>().join(", ")
          ),
      ))
  }
  ```

- [ ] Put the rule in the spec. In `docs/SPEC.md` section 5.3's structural list, replace

  ```
  - At least one exit criterion exists, and every criterion has a `verification` with a `method` that is one of `command`, `test`, `artifact`, `review`, `human`.
  ```

  with

  ```
  - At least one exit criterion exists, every criterion has a `verification` with a `method` that is one of `command`, `test`, `artifact`, `review`, `human`, and no two criteria share an `id`, because a recorded result, a note, and an event all name a criterion by its id and cannot tell two apart (added in 0.3).
  ```

- [ ] Correct the project plan. In `docs/plans/project-plan.md`, phase 1, replace

  ```
  so that step 09's `ContractRequiresHuman` gate and the Definition of Done ask the same question once
  ```

  with

  ```
  answering spec 5.4 item 5 and 5.16 item 4 only, so that this step's `HumanAccepted` rule and phase 3's orchestrator agree; step 09's `ContractRequiresHuman` gate is the human's approval of the contract before work starts, which `human_accepts_contracts` widens, and step 09 takes it from its context field instead
  ```

  and add `CriteriaIdsUnique` to step 02's `ReadinessRule` list, after `CriteriaPresent`.

- [ ] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::readiness
  # expected, among the output:
  # test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 139 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): refuse exit criteria that share an id`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 139 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): refuse exit criteria that share an id
```

## Open questions

none
