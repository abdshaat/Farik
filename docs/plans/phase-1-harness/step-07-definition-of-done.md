# Phase 1, step 07: Definition of Done

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.4 (the five conditions for acceptance, and the reviewer's fresh session), section 5.16 item 4 (an epic needs the human), section 5.3 (the `human` verification method), section 4 (what a contract is, which task 1 makes exact), F5
Depends on: phase 0 (merged in #4) for `contract::{TaskContract, Verification, VerificationWire, validate_contract}` and the generated types; step 02 of this phase for `governor::readiness::fixtures::a_contract`, which every one of task 2's tests builds on (committed as 21fe00a, 87a3561, 4a1ac90); step 03 for `check_allowed_paths` (220b576, 3355d35); step 06 (33202e7 and its review fix abfb7d8)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code session that did not write this plan, 2026-09-16. Fourth pass at `e0cb984`: READY under all three rules of `docs/standards/workflow.md` stage 2, after three passes that refused it. It executed both tasks in a scratch copy rather than taking the evidence on trust: the baseline, both red states verbatim, `cargo fmt --all --check` clean immediately after pasting each block, clippy pedantic with `-D warnings`, `cargo xtask core-io`, every quoted count to the digit, both commit subjects through `cargo xtask commit-msg`, and byte identity of the blocks. It ran 32 mutations and killed 31. It also attacked the result: through any contract `validate_contract` produced it could not get a task accepted with a criterion nobody ran, with a change outside its allowed paths, or without the human where spec 5.4 item 5 or 5.16 item 4 requires one. The one survivor is taken in this commit: the message a Product Manager reads most often, the diff that reaches outside the allowed paths, was pinned only by its prefix while the bullet above claimed all seven messages were asserted, so it now has an exact assertion and the bullet says which branches it covers. Four smaller items are taken too: the first Decisions bullet no longer says `validate_contract` runs on every read from the wire, which is a convention rather than a type-level guarantee, and task 1 now writes that convention into the two phase 2 entries that could bypass it; the duplicate-id note is attributed to the third review, which moved the rule, rather than the second, which put it in the wrong place; the header and Consumes list name phase 0 for `Verification` and step 02 for the fixture every test builds on; the three project-plan anchors are marked as substrings of longer lines; and a Decisions bullet says which reviewer result decides when two name one criterion.

## Goal

`farik-core` can say whether a task may be accepted: every exit criterion run by the reviewer and passed, every `human` criterion answered by the human rather than by an agent, nothing changed outside the contract's allowed paths, a completion note from the assignee, a review note from the reviewer, and the human's acceptance where the contract's risk or kind requires it. Step 09's transition evaluation calls it behind the `DefinitionOfDone` gate on `verifying -> accepted`; phase 3's orchestrator gathers the evidence it takes.

First, though, the step makes the first of those rules decidable at all. A recorded result names the criterion it belongs to by id, and nothing refused a contract that gave one id to two criteria, so one run was credited to both and a task could be accepted with a criterion nobody ran. Task 1 refuses such a contract in `validate_contract`, where every contract read from the wire passes.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- Exit criterion ids must name one criterion each, and the rule belongs in `validate_contract` rather than in the Definition of Ready. Uniqueness inside an array is a well-formedness property of the document, the same kind as `^C[0-9]+$` and `minItems: 1`, and the project plan's step 02 entry already set the precedent: there is no `RiskSet` readiness rule "because the schema requires `risk` and `validate_contract` refuses a contract without one". JSON Schema 2020-12 cannot express uniqueness by property, but the validator has a stage after the schema where it can. Rejected: a `CriteriaIdsUnique` readiness rule, because the Definition of Ready runs once, on `refining -> ready`, and its verdict is never stored: step 01's committed table has two gate-free human rows (`any -> escalated` and `escalated -> any`), so a human can move a task from `draft` to `verifying` without it, and phase 2's recovery and `farik doctor --adopt` rebuild contracts with no readiness run at all. `validate_contract` is the crate's only path from a wire value to a `TaskContract` today, and `crates/core/src/generated/mod.rs` states the convention that every wire value passes it. That is a convention, not a type-level guarantee: `TaskContract` is a re-export of a generated type that derives `Deserialize`, so a caller can bypass it. Task 1 therefore also writes the convention into the project plan's phase 2 entries, where the two places that could bypass it are declared.
- Every repeated id is named once, in the order the criteria appear, because reporting the first of several hides the rest, as step 05's budgets taught. The message says why the rule exists, so that a Product Manager reading a refusal knows what to change.
- The refusal is one error at `/exit_criteria` rather than one per repeated criterion, because the fault is the set of ids, not any one of them.
- `evaluate_done` returns `Result<(), Vec<DoneFailure>>` and reports every rule the task fails, in the order of `DoneRule`, as step 02's `evaluate_readiness` does. Each check returns `Option<DoneFailure>` and `evaluate_done` collects with `filter_map`, the shape `evaluate_readiness` already has, so the two halves of the governor read the same way. Rejected: stopping at the first failure.
- Task 2's checks read the criterion they hold and never look a verification back up by id. With task 1 in place a contract from the wire cannot repeat an id, so this is defence in depth: `TaskContract`'s fields are public, and a runtime that builds one itself is not held by the validator. It is not a second implementation of task 1's rule, and it does not catch every shape task 1 catches: two criteria with one id and one result between them are indistinguishable here unless one of them is a `human` criterion.
- A criterion whose verification method is `human` is exempt from `CriterionRunByReviewer` and judged by `HumanCriterionAccepted` instead, because spec 5.4 item 1 says such criteria "are satisfied only by an explicit human acceptance event": a reviewer cannot run them at all, so requiring its run would make every contract with a `human` criterion permanently unacceptable. It needs a result that passed with `run_by: Human`.
- A reviewer's result counts only with non-blank evidence, because spec 5.4 item 4 requires the review note to map each criterion to evidence, and a recorded result with nothing in it is the "I ran the tests and they passed" the section warns about. Whitespace is not evidence, with the same floor the notes have: `trim` follows Unicode White_Space, so a non-breaking or ideographic space is refused and a zero-width one is not.
- A `human` result needs no evidence while a reviewer's does, because the acceptance event is the evidence: spec 5.4 item 1 asks for an explicit human acceptance, not for the human to write up a command's output. A test pins it.
- `DoneEvidence.results` holds what this verification round recorded: the reviewer's own runs and the human's acceptances. The assignee's earlier run is step 08's `check_criteria_recorded` gate, which takes it as its own parameter, so a criterion the assignee failed and the reviewer passed is accepted: the independent run is the point of spec 5.4 item 1. The type cannot stop a runtime from putting an assignee's result here, so every check ignores one rather than trusting the field to be clean. Changed 2026-09-16 by this plan from the project plan's `reviewer_results`, whose name said less than the field holds.
- When more than one reviewer result names the same criterion, the first decides, so a blank result followed by a good one refuses and a good one followed by a blank one accepts. Both are the safe direction for a list that should hold one result per criterion, and the runtime is what keeps it to one; neither is asserted.
- A result whose `criterion_id` names no criterion of the contract decides nothing, including when it failed: the contract's criteria are the list, and anything else is noise the runtime recorded. The criteria that are in the contract still have to be run and to pass, which the other rules enforce. A test pins it.
- `CriterionPassed` reads every result that is not the assignee's, rather than only the reviewer's, so that a `human` criterion recorded as failed also refuses; and it ignores the assignee's, so that its earlier failure does not outvote the reviewer's own run. Both directions have a test, because a check restricted to the reviewer alone passed the whole suite when this plan was first reviewed.
- The diff is checked with step 03's `check_allowed_paths`, so that one set of glob semantics decides what a path means everywhere in the harness, and a contract whose `allowed_paths` do not compile refuses acceptance rather than accepting everything. A task that changed nothing passes: spec 5.4 item 2 forbids changes outside the allowed paths and says nothing about changes being required, and step 08's `CriteriaRecorded` gate is where a commit is demanded.
- An empty `allowed_paths` makes every changed path a violation, and its message says so rather than ending in a dangling list. Like the assignee's results, it is unreachable through `validate_contract` (`minItems: 1`) and reachable by hand, so it is checked rather than assumed, with a test.
- `requires_human_acceptance` answers one question and says so in its doc: must the human accept the finished result before the task is accepted? Spec 5.4 item 5 and 5.16 item 4 give the answer, risk `high` or kind `epic`, and no team policy touches it. Its consumers are this step's `HumanAccepted` rule and phase 3's orchestrator; it is public so that they agree. It is deliberately **not** the answer to step 09's `ContractRequiresHuman` gate, which is the human's approval of the contract *before* work starts: project plan D8 widens that one to every task under `human_accepts_contracts: all`, and step 09 takes it as the context field `contract_requires_human_acceptance` rather than calling this function. The schema's `risk` description names the two moments separately, and conflating them would either silence the policy or make high-risk acceptance policy-dependent.
- A note counts as written only when it is not blank, and both notes are `Option<String>` rather than `String`, so that "the runtime has none" and "the agent wrote nothing" are the same refusal with one message. Every one of the seven messages is asserted exactly by a test, and so is each of the three branches `PathsWithinAllowed` can take, because four messages were free to say anything when this plan was reviewed a third time and the most-read of the three branches still was at the fourth.
- A contract with no exit criteria at all passes the three rules that iterate them vacuously. That is unreachable rather than decided: the schema sets `minItems: 1` on `exit_criteria`, `validate_contract` refuses a contract without one, and step 02's `CriteriaPresent` rule refuses it again before the task is ever assigned.
- Revised three times on 2026-09-16, after three readiness reviews, each of which found the previous fix narrower than it read. The first found a `human` criterion's exemption handed to a `test` criterion by an id lookup. The second found results still matched by id, so a `test` and a `command` criterion sharing `C1` were accepted on one run, and put the rule in the Definition of Ready. The third showed that the Definition of Ready is not a boundary a task must cross: two gate-free human rows in step 01's table go around it, and phase 2 rebuilds contracts without it. The rule now lives in `validate_contract`, and the claim in this plan is the one the code can keep.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

Task 1: `contract::validate_contract` refuses a contract whose exit criteria repeat an id, with three tests, a line in spec section 4, and the project plan's entries brought in line.

Task 2: the `governor::done` module with `RunBy`, `CriterionResult`, `DoneEvidence`, `DoneRule`, `DoneFailure`, `requires_human_acceptance`, `evaluate_done` and its seven private checks, and sixteen tests.

Out of scope: the gate that calls it and the assignee's own recorded run (step 08), the transition row it serves (step 09), the events that record an acceptance (phase 2), and who is allowed to be the reviewer, which step 02's `ReviewerAvailable` and step 08's `check_assignment` already decide. Also out of scope, and worth naming because this step trusts them: nothing here ties `RunBy::Human` to a `human.accepted` event, which is phase 2's, or the `Reviewer` label to the fresh session spec 5.4's closing paragraph requires, which is phase 3's. `CriterionResult` is what the runtime recorded, and phase 1 takes it at its word.

## Architecture notes

Touches `crates/core` only: `contract.rs` gains one rule, and `governor` gains one child. Task 2 consumes `contract::{TaskContract, Verification}`, the generated `Kind` and `Risk` (phase 0), and `governor::paths::check_allowed_paths` (step 03). Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/contract.rs                         modifies: validate_contract refuses repeated criterion ids, with three tests (task 1)
docs/SPEC.md                                        modifies: section 4's Contract definition says exit criterion ids name one criterion each (task 1)
docs/plans/project-plan.md                          modifies: phase 0 step 03's validate_contract entry records the refusal; phase 1's duplicate-id note points at the validator; step 07's entry stops claiming requires_human_acceptance answers ContractRequiresHuman (task 1)
crates/core/src/governor.rs                         modifies: declares done (task 2)
crates/core/src/governor/done.rs                    creates: RunBy, CriterionResult, DoneEvidence, DoneRule, DoneFailure, requires_human_acceptance, evaluate_done, sixteen tests (task 2)
docs/plans/phase-1-harness/step-07-definition-of-done.md   modifies: checkboxes ticked
```

The project plan's step 07 interface entry already records `DoneEvidence.results` and `requires_human_acceptance`, added by this plan's own commit.

## Tasks

### Task 1: An exit criterion id names one criterion

Files: modified `crates/core/src/contract.rs`, `docs/SPEC.md`, `docs/plans/project-plan.md`

Consumes: `contract::{TaskContract, ValidationError, validate_contract}` from phase 0
Produces: no new public item; `validate_contract` refuses one more shape

- [x] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 122 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Write the failing tests. In `crates/core/src/contract.rs`, insert before `    #[test]\n    fn accepts_a_schema_valid_contract_and_applies_the_defaults() {`:

  ```rust
      #[test]
      fn refuses_exit_criteria_that_share_an_id() {
          // A recorded result, a note, and an event all name a criterion by its id, so two criteria
          // with one id are indistinguishable downstream: one run would be credited to both, and a
          // task could be accepted with a criterion nobody ran. JSON Schema 2020-12 cannot say a
          // property is unique across an array, so the rule lives here rather than in the schema.
          let mut input = a_contract_wire();
          let twin = input["exit_criteria"][0].clone();
          input["exit_criteria"] = json!([twin.clone(), twin]);
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/exit_criteria");
          assert_eq!(
              errors[0].message,
              "the id C1 names more than one exit criterion; give each criterion its own, because a \
               recorded result, a note, and an event all name a criterion by its id and cannot tell \
               two apart"
          );
      }

      #[test]
      fn names_every_repeated_id_once_however_far_apart_they_are() {
          let mut input = a_contract_wire();
          let first = input["exit_criteria"][0].clone();
          let named = |id: &str| {
              let mut criterion = first.clone();
              criterion["id"] = json!(id);
              criterion
          };
          // C1 and C3 each name two criteria, never adjacent, and C1 names three.
          input["exit_criteria"] = json!([
              named("C1"),
              named("C2"),
              named("C3"),
              named("C1"),
              named("C3"),
              named("C1"),
          ]);
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert!(
              errors[0]
                  .message
                  .starts_with("the ids C1, C3 name more than one"),
              "{}",
              errors[0].message
          );
      }

      #[test]
      fn accepts_exit_criteria_whose_ids_are_all_distinct() {
          let mut input = a_contract_wire();
          let first = input["exit_criteria"][0].clone();
          let mut second = first.clone();
          second["id"] = json!("C2");
          input["exit_criteria"] = json!([first, second]);
          let contract = validate_contract(&input).expect("valid");
          assert_eq!(contract.exit_criteria.len(), 2);
      }

  ```

- [x] Run them and confirm they fail because `validate_contract` accepts a repeated id. Each panics at `expected a refusal` with the accepted contract printed:

  ```
  cargo test --package farik-core contract
  # expected, among the output:
  # failures:
  #     contract::tests::names_every_repeated_id_once_however_far_apart_they_are
  #     contract::tests::refuses_exit_criteria_that_share_an_id
  # test result: FAILED. 17 passed; 2 failed; 0 ignored; 0 measured; 106 filtered out; finished in 0.09s
  ```

- [x] Refuse it. In `crates/core/src/contract.rs`, replace

  ```rust
  use std::sync::LazyLock;
  ```

  with

  ```rust
  use std::collections::BTreeSet;
  use std::sync::LazyLock;
  ```

  replace

  ```rust
  /// Every schema violation, in the schema's order rather than the input's key order; or, when the
  /// schema passes but the typed contract cannot be built, one error at the root.
  ```

  with

  ```rust
  /// Every schema violation, in the schema's order rather than the input's key order; one error at
  /// the root when the schema passes but the typed contract cannot be built; or one error at
  /// `/exit_criteria` when two criteria share an `id`.
  ```

  and replace

  ```rust
      serde_json::from_value::<TaskContract>(with_integers_normalised(input)).map_err(|error| {
          vec![ValidationError {
              path: "/".to_string(),
              message: format!(
                  "the schema passed but the typed contract could not be built: {error}"
              ),
          }]
      })
  }
  ```

  with

  ```rust
      let contract = serde_json::from_value::<TaskContract>(with_integers_normalised(input))
          .map_err(|error| {
              vec![ValidationError {
                  path: "/".to_string(),
                  message: format!(
                      "the schema passed but the typed contract could not be built: {error}"
                  ),
              }]
          })?;
      let repeated = repeated_criterion_ids(&contract);
      if !repeated.is_empty() {
          return Err(vec![ValidationError {
              path: "/exit_criteria".to_string(),
              message: format!(
                  "{} more than one exit criterion; give each criterion its own, because a \
                   recorded result, a note, and an event all name a criterion by its id and cannot \
                   tell two apart",
                  named(&repeated)
              ),
          }]);
      }
      Ok(contract)
  }

  /// Every `id` that names more than one exit criterion, in the schema's order and without
  /// repeats. JSON Schema 2020-12 cannot say that a property is unique across an array, so the
  /// rule lives here, where every contract read from the wire passes.
  fn repeated_criterion_ids(contract: &TaskContract) -> Vec<String> {
      let mut seen: BTreeSet<&str> = BTreeSet::new();
      let mut repeated: Vec<String> = Vec::new();
      for criterion in &contract.exit_criteria {
          let id = criterion.id.as_str();
          if !seen.insert(id) && !repeated.iter().any(|already| already == id) {
              repeated.push(id.to_string());
          }
      }
      repeated
  }

  /// "the id C1 names" or "the ids C1, C2 name", so that a message reads as English either way.
  fn named(ids: &[String]) -> String {
      if ids.len() == 1 {
          format!("the id {} names", ids[0])
      } else {
          format!("the ids {} name", ids.join(", "))
      }
  }
  ```

- [x] Say it in the spec. In `docs/SPEC.md` section 4, replace

  ```
  **Contract.** A structured document attached to an epic or a task: intent, scope, requirements, exit criteria with a verification method for each, constraints, budget, and a named reviewer who is not the assignee. The schema is in `docs/schemas/task-contract.schema.json`; the `kind` field says which of the two it is.
  ```

  with

  ```
  **Contract.** A structured document attached to an epic or a task: intent, scope, requirements, exit criteria with a verification method for each, constraints, budget, and a named reviewer who is not the assignee. The schema is in `docs/schemas/task-contract.schema.json`; the `kind` field says which of the two it is. Every exit criterion's `id` names one criterion: a recorded result, a note, and an event all refer to a criterion by its id, so a contract that gives one id to two criteria is refused when it is read, which JSON Schema cannot express and the validator therefore does (added in 0.3).
  ```

- [x] Bring the project plan in line. Each of these before-texts is a substring of a longer line rather than a whole line, and each occurs exactly once. In `docs/plans/project-plan.md`, replace

  ```
  `contract::validate_contract(input: &serde_json::Value) -> Result<TaskContract, Vec<ValidationError>>`
  ```

  with

  ```
  `contract::validate_contract(input: &serde_json::Value) -> Result<TaskContract, Vec<ValidationError>>` (refuses repeated exit criterion ids as well as schema violations, added 2026-09-16 by the phase 1 step 07 plan)
  ```

  replace

  ```
  was answered by step 07's second readiness review, not by this step: the Definition of Ready refuses it (`CriteriaIdsUnique`, step 07 task 2)
  ```

  with

  ```
  was answered by step 07's third readiness review, not by this step: `validate_contract` refuses it (step 07 task 1), not the Definition of Ready, which a human moving a task out of `escalated` and phase 2's recovery both go around
  ```

  replace

  ```
  `TaskCreate { contract: TaskContract }`
  ```

  with

  ```
  `TaskCreate { contract: TaskContract }` (the daemon builds it with `validate_contract`, never by deserialising a wire value directly, because the repeated-criterion-id rule of phase 1 step 07 lives there)
  ```

  replace

  ```
  read_contract(&self, id)
  ```

  with

  ```
  read_contract(&self, id) (through `validate_contract`, so that a contract read back from a file is held to the same rules as one that arrived on the wire)
  ```

  and replace

  ```
  so that step 09's `ContractRequiresHuman` gate and the Definition of Done ask the same question once
  ```

  with

  ```
  answering spec 5.4 item 5 and 5.16 item 4 only, so that step 07's `HumanAccepted` rule and phase 3's orchestrator agree; step 09's `ContractRequiresHuman` gate is the human's approval of the contract before work starts, which `human_accepts_contracts` widens, and step 09 takes it from its context field instead
  ```

- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core contract
  # expected, among the output:
  # test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 106 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 125 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `fix(core): refuse a contract that gives one id to two criteria`

### Task 2: The Definition of Done

Files: created `crates/core/src/governor/done.rs`; modified `crates/core/src/governor.rs`

Consumes: `contract::{TaskContract, Verification}` and, in the tests, `contract::VerificationWire` and `governor::readiness::fixtures::a_contract`; `generated::task_contract::{FarikTaskContractKind, FarikTaskContractRisk}`; `governor::paths::{GlobError, PathRefusal, check_allowed_paths}`
Produces: `governor::done::{RunBy, CriterionResult, DoneEvidence, DoneRule, DoneFailure, requires_human_acceptance, evaluate_done}`

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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CriterionRunByReviewer),
              "the reviewer's own run recorded no evidence for criterion C1"
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
          // A result that is blank and failed breaks the first two rules at once, which is what
          // pins their order against each other.
          evidence.results[0].passed = false;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer, R::CriterionPassed]
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
          assert_eq!(
              message_of(&contract, &evidence, R::HumanCriterionAccepted),
              "the human has not answered criterion C1, and only the human can"
          );
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
          // The acceptance event is the evidence, so a human answer needs no write-up of its own
          // while a reviewer's run does.
          evidence.results[0].evidence = String::new();
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::PathsWithinAllowed),
              "the diff changes src/billing/invoice.rs outside the contract's allowed paths src/login/**"
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CompletionNotePresent),
              "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first"
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::ReviewNotePresent),
              "the reviewer wrote no review note mapping each criterion to its evidence"
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CriterionRunByReviewer),
              "the reviewer's own run recorded no evidence for criterion C1"
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
          // A result that is blank and failed breaks the first two rules at once, which is what
          // pins their order against each other.
          evidence.results[0].passed = false;
          assert_eq!(
              failed_rules(&a_contract(), &evidence),
              [R::CriterionRunByReviewer, R::CriterionPassed]
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
          assert_eq!(
              message_of(&contract, &evidence, R::HumanCriterionAccepted),
              "the human has not answered criterion C1, and only the human can"
          );
          evidence.results = vec![a_result("C1", RunBy::Human)];
          assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
          // The acceptance event is the evidence, so a human answer needs no write-up of its own
          // while a reviewer's run does.
          evidence.results[0].evidence = String::new();
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::PathsWithinAllowed),
              "the diff changes src/billing/invoice.rs outside the contract's allowed paths src/login/**"
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::CompletionNotePresent),
              "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first"
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
          assert_eq!(
              message_of(&a_contract(), &evidence, R::ReviewNotePresent),
              "the reviewer wrote no review note mapping each criterion to its evidence"
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
  # test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 141 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide whether a task meets the definition of done`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 141 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
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
# feat(core): decide whether a task meets the definition of done
# fix(core): refuse a contract that gives one id to two criteria
```

## Open questions

none
