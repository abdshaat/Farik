# Phase 1, step 08: Gate predicates

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.2 (the gates of the transition table), section 5.11 (a locked contract and a frozen one), section 5.14 (a dependency is accepted and integrated), section 5.16 (an epic's assignee, its tasks, its product documents), F5
Depends on: phase 0 (merged in #4); step 01 of this phase for `TransitionActor` and the gates the table names (f9f0e67, 0e50df1, 3cc8bc3); step 02 for `readiness::fixtures::a_contract` (21fe00a, 87a3561, 4a1ac90); step 07 for `CriterionResult` and `RunBy` (a0cdecd, 090e015)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`farik-core` can answer every gate of section 5.2's table that is not already a function: may this task be assigned, has the assignee done what it must before declaring done, are an epic's tasks finished, is there a written blocker, is it cleared, are there reasons for a rejection. It can also answer the two questions 5.11 and 5.16 ask outside the table: may this actor write this contract, and may this actor create a task under this epic, and may the Product Manager write a product document now. Step 09 composes them with the table, the actor check, and the four counting gates step 06 already answers.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- Every gate returns `GateResult = Result<(), Vec<String>>`: no rule enum, because the caller already knows which gate it asked and step 09 wraps the answer in `TransitionRefusal::GateFailed { gate, details }`. Each gate reports every reason it refuses, in the order the rules are written, as `evaluate_readiness` and `evaluate_done` do, so that an agent sent back learns everything at once.
- The messages are the agent's instructions, so each says what to change rather than what is wrong. They name the agent, the task, the status, or the field at fault.
- `check_assignment` is the one gate that reads the contract and the world together. Its rules, in order: the Product Manager may ask only when the team has no active Scrum Master (5.2); the assignee's role is the contract's, except for an epic, which goes to the Scrum Master, or to the Product Manager when there is no active one (5.16 item 3); the reviewer's role is the contract's; the reviewer is not the assignee (5.1); the assignee is below the work-in-progress limit; the task's budget fits what is left of the sprint's; and every dependency the contract lists is accepted and integrated (5.14).
- A dependency the runtime reported nothing about refuses the assignment rather than passing, because an unknown dependency is not an accepted one.
- `DependencyState` and `ChildState` name their task with a `String`, not the typed `TaskId`, because step 02's `ReadinessContext::dependency_statuses` already keys by `String` and the contract's own `dependencies` are strings on the wire; two spellings of the same thing in one module would be worse than one loose one. Changed from the project plan's `TaskId` by this plan.
- `check_criteria_recorded` asks for a recorded result from the assignee's own run with evidence, not for a passing one. Spec 5.2 asks for "a recorded result", and the reviewer's independent run is what decides: demanding a pass here would reward an agent that records one it did not get. A `human` criterion is exempt, because the assignee cannot run one; the human answers it and the Definition of Done checks that (step 07).
- Evidence must not be blank, the same floor step 07 holds the reviewer to: a recorded result with nothing in it is not a run.
- `check_children_done` is the `CriteriaRecorded` gate of an epic (5.16 item 4): every task accepted or cancelled, and at least one accepted. An epic with no tasks at all fails the second rule, which is right: there is nothing to verify.
- `check_product_doc_write` refuses an epic in `escalated`, although an epic escalated after approval is past the bar, because an epic awaiting the user's approval sits in `escalated` too (5.16 item 2) and the status alone cannot tell them apart. The safe direction is to wait.
- `check_child_creation` takes a `ParentEpic` of its own rather than step 02's `ParentState`, because that one carries the paths and the budget the Definition of Ready needs and not the assignee this question turns on; a caller would have to invent two fields it does not use. Added by this plan; the project plan's entry named `ParentState` and a separate assignee argument.
- It also checks that the epic is `in_progress`, which the Definition of Ready checks as well. The overlap is deliberate: readiness runs when the task is written, this runs when it is created, and 5.16 item 3 states both.
- `check_contract_write` returns `Result<ContractWriteOutcome, ContractWriteRefusal>` rather than a `GateResult`, because its three answers are not two: allowed, allowed but the task goes back to `refining`, or refused, and 5.11 gives the refusal two named reasons. `FIELDS_AFTER_FREEZE` and `FIELDS_WHEN_LOCKED` are public constants, so that phase 2's command handling and phase 5's editor name the same fields once. Added by this plan.
- A locked contract leaves only its notes open to an agent. Criterion results are events rather than contract fields, so nothing here covers them, and 5.11's "agents may record criterion results" is about the log.
- The human may write anything; a human write of a frozen contract that touches a field outside `FIELDS_AFTER_FREEZE` returns `ReturnsToRefining`, and one that touches only those fields is an ordinary allowed write, because the status and the assignee change on every transition.
- A budget that cannot be compared does not fit, as a spend that is not a number counts as exhausted in `budget` (spec 5.5).
- The `Triaged` gate needs no function: it is one boolean the runtime records (`request.triaged`, 5.16), and step 09 takes it as `TransitionContext::triaged`.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

One task: the `governor::gates` module with `GateResult`, the nine checks, the values they take, the two field constants, and twenty-four tests.

Out of scope: the table, the actor check, and the four counting gates, which are step 09's to compose from step 01's and step 06's work; the Definition of Ready and the Definition of Done, which are gates of their own from steps 02 and 07; and everything the runtime must observe to fill these values, which is phase 3's.

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `contract::{Role, TaskContract, TaskStatus, Verification}`, the generated `Kind`, `governor::done::{CriterionResult, RunBy}` (step 07), and `governor::transition_table::TransitionActor` (step 01). Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock; `cargo xtask core-io` passes.
- Every public item carries a doc comment, and every function returning a `Result` carries an `# Errors` section; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                         modifies: declares gates
crates/core/src/governor/gates.rs                   creates: GateResult, the nine checks and their values, the two field constants, twenty-four tests
docs/plans/project-plan.md                          modifies: phase 1 step 08's interface records the three changes above (in the plan's own commit)
docs/plans/phase-1-harness/step-08-gate-predicates.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: The gate predicates

Files: created `crates/core/src/governor/gates.rs`; modified `crates/core/src/governor.rs`

Consumes: `contract::{Role, TaskContract, TaskStatus, Verification}`, `generated::task_contract::FarikTaskContractKind`, `governor::done::{CriterionResult, RunBy}`, `governor::transition_table::TransitionActor`, and in the tests `contract::VerificationWire` and `governor::readiness::fixtures::a_contract`
Produces: `governor::gates::{GateResult, AssignmentRequester, DependencyState, AssignmentInput, check_assignment, WorkState, check_criteria_recorded, ChildState, check_children_done, check_product_doc_write, ContractWriteActor, ParentEpic, check_child_creation, Blocker, check_blocker_written, check_blocker_resolved, Rejection, check_rejection_reasons, ContractWriteOutcome, ContractWriteRefusal, FIELDS_AFTER_FREEZE, FIELDS_WHEN_LOCKED, check_contract_write}`

- [ ] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
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
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```

- [ ] Write the failing tests. `crates/core/src/governor/gates.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::{
          AssignmentInput, AssignmentRequester, Blocker, ChildState, ContractWriteActor,
          ContractWriteOutcome, ContractWriteRefusal, DependencyState, ParentEpic, Rejection,
          WorkState, check_assignment, check_blocker_resolved, check_blocker_written,
          check_child_creation, check_children_done, check_contract_write, check_criteria_recorded,
          check_product_doc_write, check_rejection_reasons,
      };
      use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::governor::done::{CriterionResult, RunBy};
      use crate::governor::readiness::fixtures::a_contract;
      use crate::governor::transition_table::TransitionActor;

      fn an_assignment() -> AssignmentInput {
          AssignmentInput {
              requested_by: AssignmentRequester::ScrumMaster,
              has_active_scrum_master: true,
              assignee_id: "dev-1".to_string(),
              assignee_role: Role::SoftwareDeveloper,
              reviewer_id: "arch-1".to_string(),
              reviewer_role: Role::Architect,
              assignee_in_progress_count: 0,
              wip_limit: 2,
              remaining_sprint_budget_usd: 15.0,
              dependencies: Vec::new(),
          }
      }

      fn reasons(result: super::GateResult) -> Vec<String> {
          result.expect_err("expected a refusal")
      }

      fn an_assignee_result(criterion_id: &str) -> CriterionResult {
          CriterionResult {
              criterion_id: criterion_id.to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by: RunBy::Assignee,
          }
      }

      fn a_depending_contract() -> TaskContract {
          let mut contract = a_contract();
          contract.dependencies = vec!["FRK-2".parse().expect("a dependency id")];
          contract
      }

      #[test]
      fn assigns_a_task_to_an_agent_of_its_role_with_a_reviewer_of_its_own() {
          assert_eq!(check_assignment(&a_contract(), &an_assignment()), Ok(()));
      }

      #[test]
      fn lets_the_product_manager_assign_only_without_a_scrum_master() {
          let mut input = an_assignment();
          input.requested_by = AssignmentRequester::ProductManager;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the Product Manager assigns only when the team has no active Scrum Master"]
          );
          input.has_active_scrum_master = false;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
      }

      #[test]
      fn refuses_an_assignee_whose_role_is_not_the_contracts() {
          let mut input = an_assignment();
          input.assignee_role = Role::MarketingSpecialist;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              [
                  "the assignee is a marketing_specialist and this contract is assigned to a software_developer"
              ]
          );
      }

      #[test]
      fn assigns_an_epic_to_the_scrum_master_or_the_product_manager_without_one() {
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          let mut input = an_assignment();
          assert_eq!(
              reasons(check_assignment(&epic, &input)),
              [
                  "the assignee is a software_developer and this contract is assigned to a scrum_master"
              ]
          );
          input.assignee_role = Role::ScrumMaster;
          assert_eq!(check_assignment(&epic, &input), Ok(()));
          input.has_active_scrum_master = false;
          assert_eq!(
              reasons(check_assignment(&epic, &input)),
              ["the assignee is a scrum_master and this contract is assigned to a product_manager"]
          );
          input.assignee_role = Role::ProductManager;
          assert_eq!(check_assignment(&epic, &input), Ok(()));
      }

      #[test]
      fn refuses_a_reviewer_of_the_wrong_role_or_the_assignee_itself() {
          let mut input = an_assignment();
          input.reviewer_role = Role::MarketingSpecialist;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the reviewer is a marketing_specialist and this contract is reviewed by a architect"]
          );
          let mut input = an_assignment();
          input.reviewer_id = "dev-1".to_string();
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["dev-1 cannot review its own work; name another agent as reviewer"]
          );
      }

      #[test]
      fn refuses_an_agent_that_is_at_its_limit_of_work_in_progress() {
          let mut input = an_assignment();
          input.assignee_in_progress_count = 1;
          input.wip_limit = 2;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
          input.assignee_in_progress_count = 2;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["dev-1 already has 2 tasks in progress and the limit is 2"]
          );
      }

      #[test]
      fn refuses_a_budget_the_sprint_cannot_pay_and_one_that_is_not_a_number() {
          let mut input = an_assignment();
          input.remaining_sprint_budget_usd = 5.0;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
          input.remaining_sprint_budget_usd = 4.99;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the budget of 5 USD does not fit the 4.99 USD left in the sprint"]
          );
          input.remaining_sprint_budget_usd = f64::NAN;
          assert_eq!(reasons(check_assignment(&a_contract(), &input)).len(), 1);
      }

      #[test]
      fn assigns_a_task_only_once_every_dependency_is_accepted_and_integrated() {
          let contract = a_depending_contract();
          let mut input = an_assignment();
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              ["the runtime reported nothing about FRK-2, which this task depends on"]
          );
          input.dependencies = vec![DependencyState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Verifying,
              integrated: false,
          }];
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              ["FRK-2 is verifying and a dependency is assigned only once it is accepted"]
          );
          input.dependencies[0].status = TaskStatus::Accepted;
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              [
                  "FRK-2 is accepted but not integrated, and this task's branch starts from the integration branch"
              ]
          );
          input.dependencies[0].integrated = true;
          assert_eq!(check_assignment(&contract, &input), Ok(()));
      }

      #[test]
      fn reports_every_assignment_rule_that_fails_at_once() {
          let mut input = an_assignment();
          input.requested_by = AssignmentRequester::ProductManager;
          input.assignee_role = Role::MarketingSpecialist;
          input.reviewer_id = "dev-1".to_string();
          input.assignee_in_progress_count = 9;
          input.remaining_sprint_budget_usd = 0.0;
          assert_eq!(reasons(check_assignment(&a_contract(), &input)).len(), 5);
      }

      #[test]
      fn declares_done_when_the_assignee_ran_every_criterion_and_committed() {
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(
              check_criteria_recorded(&a_contract(), &[an_assignee_result("C1")], &work),
              Ok(())
          );
      }

      #[test]
      fn refuses_a_criterion_the_assignee_did_not_run_itself_or_ran_without_evidence() {
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[], &work)),
              ["the assignee recorded no run with evidence for criterion C1"]
          );
          let mut reviewers = an_assignee_result("C1");
          reviewers.run_by = RunBy::Reviewer;
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[reviewers], &work)).len(),
              1
          );
          let mut blank = an_assignee_result("C1");
          blank.evidence = "  ".to_string();
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[blank], &work)).len(),
              1
          );
      }

      #[test]
      fn takes_a_recorded_failure_as_a_run_because_the_reviewer_decides() {
          // Spec 5.2 asks for a recorded result, not a passing one: an assignee that records its
          // failure is telling the truth, and demanding a pass here would reward one that does not.
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          let mut failed = an_assignee_result("C1");
          failed.passed = false;
          assert_eq!(
              check_criteria_recorded(&a_contract(), &[failed], &work),
              Ok(())
          );
      }

      #[test]
      fn does_not_ask_the_assignee_to_run_a_criterion_only_the_human_can_answer() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(check_criteria_recorded(&contract, &[], &work), Ok(()));
      }

      #[test]
      fn refuses_work_with_no_commit_or_an_unclean_worktree() {
          let results = [an_assignee_result("C1")];
          assert_eq!(
              reasons(check_criteria_recorded(
                  &a_contract(),
                  &results,
                  &WorkState {
                      commits: 0,
                      worktree_clean: true
                  }
              )),
              ["the task branch has no commit on it"]
          );
          assert_eq!(
              reasons(check_criteria_recorded(
                  &a_contract(),
                  &results,
                  &WorkState {
                      commits: 2,
                      worktree_clean: false
                  }
              )),
              ["the worktree has changes that are not committed"]
          );
      }

      #[test]
      fn verifies_an_epic_when_its_tasks_are_done_and_at_least_one_was_accepted() {
          let done = [
              ChildState {
                  task_id: "FRK-2".to_string(),
                  status: TaskStatus::Accepted,
              },
              ChildState {
                  task_id: "FRK-3".to_string(),
                  status: TaskStatus::Cancelled,
              },
          ];
          assert_eq!(check_children_done(&done), Ok(()));
          let mut running = done.clone();
          running[1].status = TaskStatus::InProgress;
          assert_eq!(
              reasons(check_children_done(&running)),
              ["every task under this epic is accepted or cancelled first, and FRK-3 is in_progress"]
          );
          let cancelled = [ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Cancelled,
          }];
          assert_eq!(
              reasons(check_children_done(&cancelled)),
              ["no task under this epic was accepted, so there is nothing to verify"]
          );
          assert_eq!(reasons(check_children_done(&[])).len(), 1);
      }

      #[test]
      fn writes_a_product_document_only_for_the_product_manager_and_an_approved_epic() {
          assert_eq!(
              check_product_doc_write(TaskStatus::Ready, Role::ProductManager),
              Ok(())
          );
          assert_eq!(
              check_product_doc_write(TaskStatus::InProgress, Role::ProductManager),
              Ok(())
          );
          assert_eq!(
              reasons(check_product_doc_write(
                  TaskStatus::Ready,
                  Role::SoftwareDeveloper
              )),
              [
                  "a software_developer may not write a product document; the Product Manager owns them"
              ]
          );
          for status in [
              TaskStatus::Draft,
              TaskStatus::Refining,
              TaskStatus::Escalated,
              TaskStatus::Cancelled,
          ] {
              assert_eq!(
                  reasons(check_product_doc_write(status, Role::ProductManager)).len(),
                  1,
                  "{status}"
              );
          }
      }

      #[test]
      fn writes_a_task_under_an_epic_only_as_its_assignee_or_the_human() {
          let parent = ParentEpic {
              status: TaskStatus::InProgress,
              assignee_id: "sm-1".to_string(),
          };
          let assignee = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("sm-1".to_string()),
          };
          assert_eq!(check_child_creation(&parent, &assignee), Ok(()));
          let human = ContractWriteActor {
              kind: TransitionActor::Human,
              agent_id: None,
          };
          assert_eq!(check_child_creation(&parent, &human), Ok(()));
          let other = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("dev-1".to_string()),
          };
          assert_eq!(
              reasons(check_child_creation(&parent, &other)),
              ["a task under an epic is written by the epic's assignee, sm-1, or by the human"]
          );
          let not_started = ParentEpic {
              status: TaskStatus::Ready,
              assignee_id: "sm-1".to_string(),
          };
          assert_eq!(
              reasons(check_child_creation(&not_started, &assignee)),
              ["the epic is ready and its tasks are written while it is in progress"]
          );
      }

      #[test]
      fn blocks_a_task_only_with_a_written_blocker() {
          let blocker = Blocker {
              description: "The staging database refuses the migration.".to_string(),
              needed: "A password for the staging database.".to_string(),
          };
          assert_eq!(check_blocker_written(Some(&blocker)), Ok(()));
          assert_eq!(reasons(check_blocker_written(None)).len(), 1);
          let blank = Blocker {
              description: " ".to_string(),
              needed: String::new(),
          };
          assert_eq!(
              reasons(check_blocker_written(Some(&blank))),
              [
                  "the blocker does not say what is in the way",
                  "the blocker does not say what is needed to clear it"
              ]
          );
      }

      #[test]
      fn clears_a_blocker_only_with_a_written_resolution() {
          assert_eq!(
              check_blocker_resolved(Some("The password is in the vault.")),
              Ok(())
          );
          assert_eq!(reasons(check_blocker_resolved(None)).len(), 1);
          assert_eq!(reasons(check_blocker_resolved(Some("\n"))).len(), 1);
      }

      #[test]
      fn rejects_work_only_with_reasons_mapped_to_criteria_the_contract_has() {
          let rejection = Rejection {
              failed_criterion_ids: vec!["C1".to_string()],
              reasons: "The form accepts an empty password.".to_string(),
          };
          assert_eq!(
              check_rejection_reasons(&a_contract(), Some(&rejection)),
              Ok(())
          );
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), None)).len(),
              1
          );
          let empty = Rejection {
              failed_criterion_ids: Vec::new(),
              reasons: "Something is wrong.".to_string(),
          };
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), Some(&empty))),
              ["the rejection names no failed criterion"]
          );
          let unknown = Rejection {
              failed_criterion_ids: vec!["C9".to_string()],
              reasons: String::new(),
          };
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), Some(&unknown))),
              [
                  "this contract has no criterion C9",
                  "the rejection says nothing about why the criteria failed"
              ]
          );
      }

      #[test]
      fn lets_an_agent_write_a_contract_that_is_neither_locked_nor_frozen() {
          let agent = ContractWriteActor {
              kind: TransitionActor::ProductManager,
              agent_id: Some("pm-1".to_string()),
          };
          assert_eq!(
              check_contract_write(
                  TaskStatus::Refining,
                  false,
                  &agent,
                  &["intent".to_string(), "exit_criteria".to_string()]
              ),
              Ok(ContractWriteOutcome::Allowed)
          );
      }

      #[test]
      fn keeps_a_locked_contract_for_the_human_but_leaves_the_notes_open() {
          let agent = ContractWriteActor {
              kind: TransitionActor::ProductManager,
              agent_id: Some("pm-1".to_string()),
          };
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &agent, &["notes".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &agent, &["intent".to_string()]),
              Err(ContractWriteRefusal::ContractLocked)
          );
      }

      #[test]
      fn freezes_a_contract_once_its_task_leaves_refining() {
          let agent = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("dev-1".to_string()),
          };
          for field in ["status", "assignee", "reviewer", "iteration", "notes"] {
              assert_eq!(
                  check_contract_write(TaskStatus::Ready, false, &agent, &[field.to_string()]),
                  Ok(ContractWriteOutcome::Allowed),
                  "{field}"
              );
          }
          assert_eq!(
              check_contract_write(
                  TaskStatus::InProgress,
                  false,
                  &agent,
                  &[
                      "notes".to_string(),
                      "scope".to_string(),
                      "budget".to_string()
                  ]
              ),
              Err(ContractWriteRefusal::ContractFrozen {
                  fields: vec!["scope".to_string(), "budget".to_string()]
              })
          );
      }

      #[test]
      fn sends_a_task_back_to_refining_when_the_human_changes_a_frozen_contract() {
          let human = ContractWriteActor {
              kind: TransitionActor::Human,
              agent_id: None,
          };
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &human, &["intent".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Verifying, false, &human, &["notes".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Verifying, true, &human, &["scope".to_string()]),
              Ok(ContractWriteOutcome::ReturnsToRefining)
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::gates
  # expected, among the output:
  # error[E0432]: unresolved imports `super::AssignmentInput`, `super::AssignmentRequester`, `super::Blocker`, `super::ChildState`, `super::ContractWriteActor`, `super::ContractWriteOutcome`, `super::ContractWriteRefusal`, `super::DependencyState`, `super::ParentEpic`, `super::Rejection`, `super::WorkState`, `super::check_assignment`, `super::check_blocker_resolved`, `super::check_blocker_written`, `super::check_child_creation`, `super::check_children_done`, `super::check_contract_write`, `super::check_criteria_recorded`, `super::check_product_doc_write`, `super::check_rejection_reasons`
  # error[E0425]: cannot find type `GateResult` in module `super`
  # error: could not compile `farik-core` (lib test) due to 2 previous errors
  ```

- [ ] Write the implementation above the tests. `crates/core/src/governor/gates.rs` in full:

  ```rust
  //! The gate predicates of `docs/SPEC.md` section 5.2's transition table, with the contract-write
  //! rules of 5.11 and the epic rules of 5.16, as pure functions over values the runtime passes in.
  //! Step 09 composes them into one transition decision; each one here says everything that is
  //! missing rather than only that something is.

  use std::cmp::Ordering;

  use crate::contract::{Role, TaskContract, TaskStatus, Verification};
  use crate::generated::task_contract::FarikTaskContractKind as Kind;
  use crate::governor::done::{CriterionResult, RunBy};
  use crate::governor::transition_table::TransitionActor;

  /// What a gate says: nothing when it passes, or every reason it does not, in the order the rules
  /// are written.
  pub type GateResult = Result<(), Vec<String>>;

  fn verdict(reasons: Vec<String>) -> GateResult {
      if reasons.is_empty() {
          Ok(())
      } else {
          Err(reasons)
      }
  }

  fn is_written(text: &str) -> bool {
      !text.trim().is_empty()
  }

  /// Whether a cost fits what is left. A figure that cannot be compared does not fit, as a spend
  /// that is not a number counts as exhausted in `budget` (`docs/SPEC.md` section 5.5).
  fn fits_within(cost: f64, remaining: f64) -> bool {
      matches!(
          cost.partial_cmp(&remaining),
          Some(Ordering::Less | Ordering::Equal)
      )
  }

  /// Who asked for a task to be assigned (`docs/SPEC.md` section 5.2).
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum AssignmentRequester {
      /// The Scrum Master, which is the ordinary case.
      ScrumMaster,
      /// The Product Manager, which stands in only when the team has no active Scrum Master.
      ProductManager,
  }

  /// A dependency of the task being assigned, as the runtime found it.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct DependencyState {
      /// The dependency's task id, as the contract spells it.
      pub task_id: String,
      /// Its status.
      pub status: TaskStatus,
      /// Whether its branch has been integrated (`docs/SPEC.md` section 5.14).
      pub integrated: bool,
  }

  /// Everything the assignment gate needs from the world.
  #[derive(Debug, Clone, PartialEq)]
  pub struct AssignmentInput {
      /// Who asked.
      pub requested_by: AssignmentRequester,
      /// Whether the team has an active Scrum Master.
      pub has_active_scrum_master: bool,
      /// The proposed assignee's agent id.
      pub assignee_id: String,
      /// The proposed assignee's role.
      pub assignee_role: Role,
      /// The proposed reviewer's agent id.
      pub reviewer_id: String,
      /// The proposed reviewer's role.
      pub reviewer_role: Role,
      /// How many tasks the proposed assignee already has in progress.
      pub assignee_in_progress_count: u32,
      /// The team's limit on work in progress per agent.
      pub wip_limit: u32,
      /// What is left of the sprint's budget, in dollars.
      pub remaining_sprint_budget_usd: f64,
      /// The state of every dependency the contract lists, in any order.
      pub dependencies: Vec<DependencyState>,
  }

  /// The `Assignment` gate of `ready -> assigned` (`docs/SPEC.md` sections 5.2, 5.14, 5.16): who may
  /// ask, whether the pair of agents fits the contract, whether the agent has room, whether the
  /// sprint can pay for it, and whether every dependency is accepted and integrated.
  ///
  /// # Errors
  ///
  /// Every rule the assignment fails, each saying what to change.
  pub fn check_assignment(contract: &TaskContract, input: &AssignmentInput) -> GateResult {
      let mut reasons = Vec::new();
      if input.requested_by == AssignmentRequester::ProductManager && input.has_active_scrum_master {
          reasons.push(
              "the Product Manager assigns only when the team has no active Scrum Master".to_string(),
          );
      }
      let expected_assignee = if contract.kind == Kind::Epic {
          if input.has_active_scrum_master {
              Role::ScrumMaster
          } else {
              Role::ProductManager
          }
      } else {
          contract.assignee_role
      };
      if input.assignee_role != expected_assignee {
          reasons.push(format!(
              "the assignee is a {} and this contract is assigned to a {expected_assignee}",
              input.assignee_role
          ));
      }
      if input.reviewer_role != contract.reviewer_role {
          reasons.push(format!(
              "the reviewer is a {} and this contract is reviewed by a {}",
              input.reviewer_role, contract.reviewer_role
          ));
      }
      if input.reviewer_id == input.assignee_id {
          reasons.push(format!(
              "{} cannot review its own work; name another agent as reviewer",
              input.assignee_id
          ));
      }
      if input.assignee_in_progress_count >= input.wip_limit {
          reasons.push(format!(
              "{} already has {} tasks in progress and the limit is {}",
              input.assignee_id, input.assignee_in_progress_count, input.wip_limit
          ));
      }
      if !fits_within(
          contract.budget.max_cost_usd,
          input.remaining_sprint_budget_usd,
      ) {
          reasons.push(format!(
              "the budget of {} USD does not fit the {} USD left in the sprint",
              contract.budget.max_cost_usd, input.remaining_sprint_budget_usd
          ));
      }
      reasons.extend(unready_dependencies(contract, input));
      verdict(reasons)
  }

  fn unready_dependencies(contract: &TaskContract, input: &AssignmentInput) -> Vec<String> {
      contract
          .dependencies
          .iter()
          .map(|dependency| dependency.to_string())
          .filter_map(|id| {
              match input
                  .dependencies
                  .iter()
                  .find(|state| state.task_id == id)
              {
                  None => Some(format!("the runtime reported nothing about {id}, which this task depends on")),
                  Some(state) if state.status != TaskStatus::Accepted => Some(format!(
                      "{id} is {} and a dependency is assigned only once it is accepted",
                      state.status
                  )),
                  Some(state) if !state.integrated => Some(format!(
                      "{id} is accepted but not integrated, and this task's branch starts from the integration branch"
                  )),
                  Some(_) => None,
              }
          })
          .collect()
  }

  /// The state of the task's branch when the assignee declares it done.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub struct WorkState {
      /// How many commits the task branch carries.
      pub commits: u32,
      /// Whether the worktree has no uncommitted change.
      pub worktree_clean: bool,
  }

  /// The `CriteriaRecorded` gate of `in_progress -> verifying` for a task (`docs/SPEC.md` section
  /// 5.2): every exit criterion has a result from the assignee's own run, and the branch has at
  /// least one commit and a clean worktree. A `human` criterion is exempt, because the assignee
  /// cannot run one; the human answers it and the Definition of Done checks that.
  ///
  /// # Errors
  ///
  /// Every rule the work fails, each saying what is missing.
  pub fn check_criteria_recorded(
      contract: &TaskContract,
      assignee_results: &[CriterionResult],
      work: &WorkState,
  ) -> GateResult {
      let mut reasons = Vec::new();
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
              !assignee_results.iter().any(|result| {
                  &result.criterion_id == id
                      && result.run_by == RunBy::Assignee
                      && is_written(&result.evidence)
              })
          })
          .collect();
      if !missing.is_empty() {
          reasons.push(format!(
              "the assignee recorded no run with evidence for {}",
              listed(&missing)
          ));
      }
      if work.commits == 0 {
          reasons.push("the task branch has no commit on it".to_string());
      }
      if !work.worktree_clean {
          reasons.push("the worktree has changes that are not committed".to_string());
      }
      verdict(reasons)
  }

  /// "criterion C1" or "criteria C1, C2", so that a message reads as English either way.
  fn listed(ids: &[String]) -> String {
      if ids.len() == 1 {
          format!("criterion {}", ids[0])
      } else {
          format!("criteria {}", ids.join(", "))
      }
  }

  /// One task under an epic, as the runtime found it.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ChildState {
      /// The task's id.
      pub task_id: String,
      /// Its status.
      pub status: TaskStatus,
  }

  /// The `CriteriaRecorded` gate of `in_progress -> verifying` for an epic (`docs/SPEC.md` section
  /// 5.16 item 4): every task under it is accepted or cancelled, and at least one is accepted.
  ///
  /// # Errors
  ///
  /// Every rule the epic fails, each naming the tasks that are not done.
  pub fn check_children_done(children: &[ChildState]) -> GateResult {
      let mut reasons = Vec::new();
      let unfinished: Vec<String> = children
          .iter()
          .filter(|child| !matches!(child.status, TaskStatus::Accepted | TaskStatus::Cancelled))
          .map(|child| format!("{} is {}", child.task_id, child.status))
          .collect();
      if !unfinished.is_empty() {
          reasons.push(format!(
              "every task under this epic is accepted or cancelled first, and {}",
              unfinished.join("; ")
          ));
      }
      if !children
          .iter()
          .any(|child| child.status == TaskStatus::Accepted)
      {
          reasons.push(
              "no task under this epic was accepted, so there is nothing to verify".to_string(),
          );
      }
      verdict(reasons)
  }

  /// Whether the Product Manager may write a product document for an epic in this status
  /// (`docs/SPEC.md` section 5.16 items 1 and 5): the writer is the Product Manager, and the epic is
  /// approved. An epic in `escalated` is refused, because one awaiting the user's approval sits
  /// there and the status alone cannot tell it from one escalated afterwards.
  ///
  /// # Errors
  ///
  /// Every rule the write fails.
  pub fn check_product_doc_write(epic_status: TaskStatus, actor_role: Role) -> GateResult {
      let mut reasons = Vec::new();
      if actor_role != Role::ProductManager {
          reasons.push(format!(
              "a {actor_role} may not write a product document; the Product Manager owns them"
          ));
      }
      if matches!(
          epic_status,
          TaskStatus::Draft | TaskStatus::Refining | TaskStatus::Escalated | TaskStatus::Cancelled
      ) {
          reasons.push(format!(
              "the epic is {epic_status} and a product document is written only once the user has approved it"
          ));
      }
      verdict(reasons)
  }

  /// Who is asking to write a contract, and which agent they are.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ContractWriteActor {
      /// Which of the lifecycle's actors this is.
      pub kind: TransitionActor,
      /// The agent's id, when an agent rather than the human or the governor.
      pub agent_id: Option<String>,
  }

  /// The state of the epic a task would be created under.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ParentEpic {
      /// The epic's status.
      pub status: TaskStatus,
      /// The agent id the epic is assigned to.
      pub assignee_id: String,
  }

  /// Whether a task may be created under this epic (`docs/SPEC.md` section 5.16 item 3): only its
  /// epic's assignee or the human, and only while the epic is in progress. The Definition of Ready
  /// checks the task's paths and budget against the epic's; this checks who and when.
  ///
  /// # Errors
  ///
  /// Every rule the creation fails.
  pub fn check_child_creation(parent: &ParentEpic, actor: &ContractWriteActor) -> GateResult {
      let mut reasons = Vec::new();
      let is_the_assignee = actor.kind == TransitionActor::Assignee
          && actor.agent_id.as_deref() == Some(parent.assignee_id.as_str());
      if actor.kind != TransitionActor::Human && !is_the_assignee {
          reasons.push(format!(
              "a task under an epic is written by the epic's assignee, {}, or by the human",
              parent.assignee_id
          ));
      }
      if parent.status != TaskStatus::InProgress {
          reasons.push(format!(
              "the epic is {} and its tasks are written while it is in progress",
              parent.status
          ));
      }
      verdict(reasons)
  }

  /// What the assignee wrote when it blocked the task.
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub struct Blocker {
      /// What is in the way.
      pub description: String,
      /// What would unblock it.
      pub needed: String,
  }

  /// The `BlockerWritten` gate of `in_progress -> blocked` (`docs/SPEC.md` section 5.2): a written
  /// blocker that says what is in the way and what is needed.
  ///
  /// # Errors
  ///
  /// Every part of the blocker that is missing.
  pub fn check_blocker_written(blocker: Option<&Blocker>) -> GateResult {
      let Some(blocker) = blocker else {
          return Err(vec![
              "a task is blocked with a written blocker: what is in the way and what is needed"
                  .to_string(),
          ]);
      };
      let mut reasons = Vec::new();
      if !is_written(&blocker.description) {
          reasons.push("the blocker does not say what is in the way".to_string());
      }
      if !is_written(&blocker.needed) {
          reasons.push("the blocker does not say what is needed to clear it".to_string());
      }
      verdict(reasons)
  }

  /// The `BlockerResolved` gate of `blocked -> in_progress` (`docs/SPEC.md` section 5.2): a written
  /// resolution, so that the next session knows what changed.
  ///
  /// # Errors
  ///
  /// One reason when nothing was written.
  pub fn check_blocker_resolved(resolution: Option<&str>) -> GateResult {
      if resolution.is_some_and(is_written) {
          return Ok(());
      }
      Err(vec![
          "a blocker is cleared with a written resolution, so that the next session knows what changed"
              .to_string(),
      ])
  }

  /// What the reviewer wrote when it rejected the work.
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub struct Rejection {
      /// The ids of the criteria that failed.
      pub failed_criterion_ids: Vec<String>,
      /// Why, in the reviewer's words.
      pub reasons: String,
  }

  /// The `RejectionReasons` gate of `verifying -> rejected` (`docs/SPEC.md` section 5.2): written
  /// reasons mapped to criteria the contract actually has, so that the next iteration knows what to
  /// fix.
  ///
  /// # Errors
  ///
  /// Every part of the rejection that is missing or does not name a criterion of this contract.
  pub fn check_rejection_reasons(
      contract: &TaskContract,
      rejection: Option<&Rejection>,
  ) -> GateResult {
      let Some(rejection) = rejection else {
          return Err(vec![
              "work is rejected with written reasons mapped to the criteria that failed".to_string(),
          ]);
      };
      let mut reasons = Vec::new();
      if rejection.failed_criterion_ids.is_empty() {
          reasons.push("the rejection names no failed criterion".to_string());
      }
      let unknown: Vec<String> = rejection
          .failed_criterion_ids
          .iter()
          .filter(|id| {
              !contract
                  .exit_criteria
                  .iter()
                  .any(|criterion| criterion.id.as_str() == id.as_str())
          })
          .cloned()
          .collect();
      if !unknown.is_empty() {
          reasons.push(format!("this contract has no {}", listed(&unknown)));
      }
      if !is_written(&rejection.reasons) {
          reasons.push("the rejection says nothing about why the criteria failed".to_string());
      }
      verdict(reasons)
  }

  /// What a write to a contract does when it is allowed.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ContractWriteOutcome {
      /// The write is applied and the task keeps its status.
      Allowed,
      /// The write is applied and the task goes back to `refining`, because the human changed a
      /// frozen contract (`docs/SPEC.md` section 5.11).
      ReturnsToRefining,
  }

  /// Why a write to a contract is refused (`docs/SPEC.md` section 5.11).
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ContractWriteRefusal {
      /// The contract is locked, so it belongs to the human.
      ContractLocked,
      /// The contract is frozen and these fields are not among the few that still change.
      ContractFrozen {
          /// The fields the write would have changed that it may not.
          fields: Vec<String>,
      },
  }

  /// The fields an agent may still change once a contract is frozen (`docs/SPEC.md` section 5.11).
  pub const FIELDS_AFTER_FREEZE: [&str; 5] = ["status", "assignee", "reviewer", "iteration", "notes"];

  /// The fields an agent may change on a locked contract: its notes, and nothing else. Criterion
  /// results are events rather than contract fields, so nothing here covers them.
  pub const FIELDS_WHEN_LOCKED: [&str; 1] = ["notes"];

  /// Whether a write to a contract is allowed (`docs/SPEC.md` section 5.11). A locked contract
  /// belongs to the human: an agent may write its notes and nothing else. A contract is frozen once
  /// its task leaves `refining`: from `ready` onward only the fields of `FIELDS_AFTER_FREEZE`
  /// change. The human may change anything, and a human change to a frozen contract sends the task
  /// back to `refining`.
  ///
  /// # Errors
  ///
  /// `ContractLocked` when an agent writes anything but the notes of a locked contract, or
  /// `ContractFrozen` naming the fields an agent may not change once the contract is frozen.
  pub fn check_contract_write(
      status: TaskStatus,
      locked: bool,
      actor: &ContractWriteActor,
      changed_fields: &[String],
  ) -> Result<ContractWriteOutcome, ContractWriteRefusal> {
      let frozen = !matches!(status, TaskStatus::Draft | TaskStatus::Refining);
      let human = actor.kind == TransitionActor::Human;
      let beyond_freeze: Vec<String> = changed_fields
          .iter()
          .filter(|field| !FIELDS_AFTER_FREEZE.contains(&field.as_str()))
          .cloned()
          .collect();
      if human {
          if frozen && !beyond_freeze.is_empty() {
              return Ok(ContractWriteOutcome::ReturnsToRefining);
          }
          return Ok(ContractWriteOutcome::Allowed);
      }
      if locked
          && changed_fields
              .iter()
              .any(|field| !FIELDS_WHEN_LOCKED.contains(&field.as_str()))
      {
          return Err(ContractWriteRefusal::ContractLocked);
      }
      if frozen && !beyond_freeze.is_empty() {
          return Err(ContractWriteRefusal::ContractFrozen {
              fields: beyond_freeze,
          });
      }
      Ok(ContractWriteOutcome::Allowed)
  }

  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::{
          AssignmentInput, AssignmentRequester, Blocker, ChildState, ContractWriteActor,
          ContractWriteOutcome, ContractWriteRefusal, DependencyState, ParentEpic, Rejection,
          WorkState, check_assignment, check_blocker_resolved, check_blocker_written,
          check_child_creation, check_children_done, check_contract_write, check_criteria_recorded,
          check_product_doc_write, check_rejection_reasons,
      };
      use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::governor::done::{CriterionResult, RunBy};
      use crate::governor::readiness::fixtures::a_contract;
      use crate::governor::transition_table::TransitionActor;

      fn an_assignment() -> AssignmentInput {
          AssignmentInput {
              requested_by: AssignmentRequester::ScrumMaster,
              has_active_scrum_master: true,
              assignee_id: "dev-1".to_string(),
              assignee_role: Role::SoftwareDeveloper,
              reviewer_id: "arch-1".to_string(),
              reviewer_role: Role::Architect,
              assignee_in_progress_count: 0,
              wip_limit: 2,
              remaining_sprint_budget_usd: 15.0,
              dependencies: Vec::new(),
          }
      }

      fn reasons(result: super::GateResult) -> Vec<String> {
          result.expect_err("expected a refusal")
      }

      fn an_assignee_result(criterion_id: &str) -> CriterionResult {
          CriterionResult {
              criterion_id: criterion_id.to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by: RunBy::Assignee,
          }
      }

      fn a_depending_contract() -> TaskContract {
          let mut contract = a_contract();
          contract.dependencies = vec!["FRK-2".parse().expect("a dependency id")];
          contract
      }

      #[test]
      fn assigns_a_task_to_an_agent_of_its_role_with_a_reviewer_of_its_own() {
          assert_eq!(check_assignment(&a_contract(), &an_assignment()), Ok(()));
      }

      #[test]
      fn lets_the_product_manager_assign_only_without_a_scrum_master() {
          let mut input = an_assignment();
          input.requested_by = AssignmentRequester::ProductManager;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the Product Manager assigns only when the team has no active Scrum Master"]
          );
          input.has_active_scrum_master = false;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
      }

      #[test]
      fn refuses_an_assignee_whose_role_is_not_the_contracts() {
          let mut input = an_assignment();
          input.assignee_role = Role::MarketingSpecialist;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              [
                  "the assignee is a marketing_specialist and this contract is assigned to a software_developer"
              ]
          );
      }

      #[test]
      fn assigns_an_epic_to_the_scrum_master_or_the_product_manager_without_one() {
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          let mut input = an_assignment();
          assert_eq!(
              reasons(check_assignment(&epic, &input)),
              [
                  "the assignee is a software_developer and this contract is assigned to a scrum_master"
              ]
          );
          input.assignee_role = Role::ScrumMaster;
          assert_eq!(check_assignment(&epic, &input), Ok(()));
          input.has_active_scrum_master = false;
          assert_eq!(
              reasons(check_assignment(&epic, &input)),
              ["the assignee is a scrum_master and this contract is assigned to a product_manager"]
          );
          input.assignee_role = Role::ProductManager;
          assert_eq!(check_assignment(&epic, &input), Ok(()));
      }

      #[test]
      fn refuses_a_reviewer_of_the_wrong_role_or_the_assignee_itself() {
          let mut input = an_assignment();
          input.reviewer_role = Role::MarketingSpecialist;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the reviewer is a marketing_specialist and this contract is reviewed by a architect"]
          );
          let mut input = an_assignment();
          input.reviewer_id = "dev-1".to_string();
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["dev-1 cannot review its own work; name another agent as reviewer"]
          );
      }

      #[test]
      fn refuses_an_agent_that_is_at_its_limit_of_work_in_progress() {
          let mut input = an_assignment();
          input.assignee_in_progress_count = 1;
          input.wip_limit = 2;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
          input.assignee_in_progress_count = 2;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["dev-1 already has 2 tasks in progress and the limit is 2"]
          );
      }

      #[test]
      fn refuses_a_budget_the_sprint_cannot_pay_and_one_that_is_not_a_number() {
          let mut input = an_assignment();
          input.remaining_sprint_budget_usd = 5.0;
          assert_eq!(check_assignment(&a_contract(), &input), Ok(()));
          input.remaining_sprint_budget_usd = 4.99;
          assert_eq!(
              reasons(check_assignment(&a_contract(), &input)),
              ["the budget of 5 USD does not fit the 4.99 USD left in the sprint"]
          );
          input.remaining_sprint_budget_usd = f64::NAN;
          assert_eq!(reasons(check_assignment(&a_contract(), &input)).len(), 1);
      }

      #[test]
      fn assigns_a_task_only_once_every_dependency_is_accepted_and_integrated() {
          let contract = a_depending_contract();
          let mut input = an_assignment();
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              ["the runtime reported nothing about FRK-2, which this task depends on"]
          );
          input.dependencies = vec![DependencyState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Verifying,
              integrated: false,
          }];
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              ["FRK-2 is verifying and a dependency is assigned only once it is accepted"]
          );
          input.dependencies[0].status = TaskStatus::Accepted;
          assert_eq!(
              reasons(check_assignment(&contract, &input)),
              [
                  "FRK-2 is accepted but not integrated, and this task's branch starts from the integration branch"
              ]
          );
          input.dependencies[0].integrated = true;
          assert_eq!(check_assignment(&contract, &input), Ok(()));
      }

      #[test]
      fn reports_every_assignment_rule_that_fails_at_once() {
          let mut input = an_assignment();
          input.requested_by = AssignmentRequester::ProductManager;
          input.assignee_role = Role::MarketingSpecialist;
          input.reviewer_id = "dev-1".to_string();
          input.assignee_in_progress_count = 9;
          input.remaining_sprint_budget_usd = 0.0;
          assert_eq!(reasons(check_assignment(&a_contract(), &input)).len(), 5);
      }

      #[test]
      fn declares_done_when_the_assignee_ran_every_criterion_and_committed() {
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(
              check_criteria_recorded(&a_contract(), &[an_assignee_result("C1")], &work),
              Ok(())
          );
      }

      #[test]
      fn refuses_a_criterion_the_assignee_did_not_run_itself_or_ran_without_evidence() {
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[], &work)),
              ["the assignee recorded no run with evidence for criterion C1"]
          );
          let mut reviewers = an_assignee_result("C1");
          reviewers.run_by = RunBy::Reviewer;
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[reviewers], &work)).len(),
              1
          );
          let mut blank = an_assignee_result("C1");
          blank.evidence = "  ".to_string();
          assert_eq!(
              reasons(check_criteria_recorded(&a_contract(), &[blank], &work)).len(),
              1
          );
      }

      #[test]
      fn takes_a_recorded_failure_as_a_run_because_the_reviewer_decides() {
          // Spec 5.2 asks for a recorded result, not a passing one: an assignee that records its
          // failure is telling the truth, and demanding a pass here would reward one that does not.
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          let mut failed = an_assignee_result("C1");
          failed.passed = false;
          assert_eq!(
              check_criteria_recorded(&a_contract(), &[failed], &work),
              Ok(())
          );
      }

      #[test]
      fn does_not_ask_the_assignee_to_run_a_criterion_only_the_human_can_answer() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("human"),
              question: "Did you sign in successfully?".to_string(),
          };
          let work = WorkState {
              commits: 1,
              worktree_clean: true,
          };
          assert_eq!(check_criteria_recorded(&contract, &[], &work), Ok(()));
      }

      #[test]
      fn refuses_work_with_no_commit_or_an_unclean_worktree() {
          let results = [an_assignee_result("C1")];
          assert_eq!(
              reasons(check_criteria_recorded(
                  &a_contract(),
                  &results,
                  &WorkState {
                      commits: 0,
                      worktree_clean: true
                  }
              )),
              ["the task branch has no commit on it"]
          );
          assert_eq!(
              reasons(check_criteria_recorded(
                  &a_contract(),
                  &results,
                  &WorkState {
                      commits: 2,
                      worktree_clean: false
                  }
              )),
              ["the worktree has changes that are not committed"]
          );
      }

      #[test]
      fn verifies_an_epic_when_its_tasks_are_done_and_at_least_one_was_accepted() {
          let done = [
              ChildState {
                  task_id: "FRK-2".to_string(),
                  status: TaskStatus::Accepted,
              },
              ChildState {
                  task_id: "FRK-3".to_string(),
                  status: TaskStatus::Cancelled,
              },
          ];
          assert_eq!(check_children_done(&done), Ok(()));
          let mut running = done.clone();
          running[1].status = TaskStatus::InProgress;
          assert_eq!(
              reasons(check_children_done(&running)),
              ["every task under this epic is accepted or cancelled first, and FRK-3 is in_progress"]
          );
          let cancelled = [ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Cancelled,
          }];
          assert_eq!(
              reasons(check_children_done(&cancelled)),
              ["no task under this epic was accepted, so there is nothing to verify"]
          );
          assert_eq!(reasons(check_children_done(&[])).len(), 1);
      }

      #[test]
      fn writes_a_product_document_only_for_the_product_manager_and_an_approved_epic() {
          assert_eq!(
              check_product_doc_write(TaskStatus::Ready, Role::ProductManager),
              Ok(())
          );
          assert_eq!(
              check_product_doc_write(TaskStatus::InProgress, Role::ProductManager),
              Ok(())
          );
          assert_eq!(
              reasons(check_product_doc_write(
                  TaskStatus::Ready,
                  Role::SoftwareDeveloper
              )),
              [
                  "a software_developer may not write a product document; the Product Manager owns them"
              ]
          );
          for status in [
              TaskStatus::Draft,
              TaskStatus::Refining,
              TaskStatus::Escalated,
              TaskStatus::Cancelled,
          ] {
              assert_eq!(
                  reasons(check_product_doc_write(status, Role::ProductManager)).len(),
                  1,
                  "{status}"
              );
          }
      }

      #[test]
      fn writes_a_task_under_an_epic_only_as_its_assignee_or_the_human() {
          let parent = ParentEpic {
              status: TaskStatus::InProgress,
              assignee_id: "sm-1".to_string(),
          };
          let assignee = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("sm-1".to_string()),
          };
          assert_eq!(check_child_creation(&parent, &assignee), Ok(()));
          let human = ContractWriteActor {
              kind: TransitionActor::Human,
              agent_id: None,
          };
          assert_eq!(check_child_creation(&parent, &human), Ok(()));
          let other = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("dev-1".to_string()),
          };
          assert_eq!(
              reasons(check_child_creation(&parent, &other)),
              ["a task under an epic is written by the epic's assignee, sm-1, or by the human"]
          );
          let not_started = ParentEpic {
              status: TaskStatus::Ready,
              assignee_id: "sm-1".to_string(),
          };
          assert_eq!(
              reasons(check_child_creation(&not_started, &assignee)),
              ["the epic is ready and its tasks are written while it is in progress"]
          );
      }

      #[test]
      fn blocks_a_task_only_with_a_written_blocker() {
          let blocker = Blocker {
              description: "The staging database refuses the migration.".to_string(),
              needed: "A password for the staging database.".to_string(),
          };
          assert_eq!(check_blocker_written(Some(&blocker)), Ok(()));
          assert_eq!(reasons(check_blocker_written(None)).len(), 1);
          let blank = Blocker {
              description: " ".to_string(),
              needed: String::new(),
          };
          assert_eq!(
              reasons(check_blocker_written(Some(&blank))),
              [
                  "the blocker does not say what is in the way",
                  "the blocker does not say what is needed to clear it"
              ]
          );
      }

      #[test]
      fn clears_a_blocker_only_with_a_written_resolution() {
          assert_eq!(
              check_blocker_resolved(Some("The password is in the vault.")),
              Ok(())
          );
          assert_eq!(reasons(check_blocker_resolved(None)).len(), 1);
          assert_eq!(reasons(check_blocker_resolved(Some("\n"))).len(), 1);
      }

      #[test]
      fn rejects_work_only_with_reasons_mapped_to_criteria_the_contract_has() {
          let rejection = Rejection {
              failed_criterion_ids: vec!["C1".to_string()],
              reasons: "The form accepts an empty password.".to_string(),
          };
          assert_eq!(
              check_rejection_reasons(&a_contract(), Some(&rejection)),
              Ok(())
          );
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), None)).len(),
              1
          );
          let empty = Rejection {
              failed_criterion_ids: Vec::new(),
              reasons: "Something is wrong.".to_string(),
          };
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), Some(&empty))),
              ["the rejection names no failed criterion"]
          );
          let unknown = Rejection {
              failed_criterion_ids: vec!["C9".to_string()],
              reasons: String::new(),
          };
          assert_eq!(
              reasons(check_rejection_reasons(&a_contract(), Some(&unknown))),
              [
                  "this contract has no criterion C9",
                  "the rejection says nothing about why the criteria failed"
              ]
          );
      }

      #[test]
      fn lets_an_agent_write_a_contract_that_is_neither_locked_nor_frozen() {
          let agent = ContractWriteActor {
              kind: TransitionActor::ProductManager,
              agent_id: Some("pm-1".to_string()),
          };
          assert_eq!(
              check_contract_write(
                  TaskStatus::Refining,
                  false,
                  &agent,
                  &["intent".to_string(), "exit_criteria".to_string()]
              ),
              Ok(ContractWriteOutcome::Allowed)
          );
      }

      #[test]
      fn keeps_a_locked_contract_for_the_human_but_leaves_the_notes_open() {
          let agent = ContractWriteActor {
              kind: TransitionActor::ProductManager,
              agent_id: Some("pm-1".to_string()),
          };
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &agent, &["notes".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &agent, &["intent".to_string()]),
              Err(ContractWriteRefusal::ContractLocked)
          );
      }

      #[test]
      fn freezes_a_contract_once_its_task_leaves_refining() {
          let agent = ContractWriteActor {
              kind: TransitionActor::Assignee,
              agent_id: Some("dev-1".to_string()),
          };
          for field in ["status", "assignee", "reviewer", "iteration", "notes"] {
              assert_eq!(
                  check_contract_write(TaskStatus::Ready, false, &agent, &[field.to_string()]),
                  Ok(ContractWriteOutcome::Allowed),
                  "{field}"
              );
          }
          assert_eq!(
              check_contract_write(
                  TaskStatus::InProgress,
                  false,
                  &agent,
                  &[
                      "notes".to_string(),
                      "scope".to_string(),
                      "budget".to_string()
                  ]
              ),
              Err(ContractWriteRefusal::ContractFrozen {
                  fields: vec!["scope".to_string(), "budget".to_string()]
              })
          );
      }

      #[test]
      fn sends_a_task_back_to_refining_when_the_human_changes_a_frozen_contract() {
          let human = ContractWriteActor {
              kind: TransitionActor::Human,
              agent_id: None,
          };
          assert_eq!(
              check_contract_write(TaskStatus::Refining, true, &human, &["intent".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Verifying, false, &human, &["notes".to_string()]),
              Ok(ContractWriteOutcome::Allowed)
          );
          assert_eq!(
              check_contract_write(TaskStatus::Verifying, true, &human, &["scope".to_string()]),
              Ok(ContractWriteOutcome::ReturnsToRefining)
          );
      }
  }
  ```

- [ ] Record the three interface changes in `docs/plans/project-plan.md`, phase 1 step 08: `DependencyState` and `ChildState` name their task with a `String`; `check_child_creation` takes a `ParentEpic { status, assignee_id }` of its own rather than `ParentState` and a separate argument; `FIELDS_AFTER_FREEZE` and `FIELDS_WHEN_LOCKED` are public constants.

- [ ] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::gates
  # expected, among the output:
  # test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 144 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 168 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): check the gates of the transition table`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 168 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): check the gates of the transition table
```

## Open questions

none
