# Phase 1, step 09: Transition evaluation

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.2 (the transition table, its gates, and who triggers each move), section 5.1 (a row that belongs to the assignee or the reviewer is that agent's), section 5.3 (the Definition of Ready), section 5.4 (the Definition of Done), section 5.5 (the budgets the governor escalates on), section 5.7 (the ten escalation reasons), section 5.11 (a contract the human must accept), section 5.12 (`human_accepts_contracts`), section 5.16 (triage, an epic's tasks, an epic's approval), F5 (the governor as a library with a test for every transition)
Depends on: phase 0 (merged in #4); step 01 of this phase for the table, `GateId`, `TransitionActor` and the two lookups (f9f0e67, 0e50df1, 3cc8bc3); step 02 for `evaluate_readiness`, `ReadinessContext` and the fixtures (21fe00a, 87a3561, 4a1ac90); step 05 for `check_budgets` and `BudgetState` (5ad2c21, c4cf30b, f3ecea4, 04ba838); step 06 for the three counting rules and `EscalationReason` (33202e7, abfb7d8); step 07 for `evaluate_done`, `DoneEvidence` and `requires_human_acceptance` (a0cdecd, 090e015, cc27028); step 08 for the nine gate predicates and their values (fa02450, 2594121)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`farik-core` can answer the whole of `docs/SPEC.md` section 5.2 in one call. Given a request to move one task to one status and the state the runtime gathered, `evaluate_transition` says which row of the table the move takes and what the runtime must record with it — the rejection counted, the escalation raised with its reason, the blocker cleared — or refuses with the row that is missing, the actor that may not ask, the agent that is not the one the contract names, or everything each gate it tried was waiting for. Phase 1 ends here: every rule of section 5 that is a decision rather than an effect is a pure function in `farik-core`, and phase 3's runtime has one function to call before it moves anything.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- The module is `governor::transition`, a child of `governor` beside the table it reads, rather than a top-level `transition`: it is the governor's decision, and `governor::transition_table` already sits there.
- `evaluate_transition` answers in four steps, and each step's refusal is its own variant: the table has no row for this move (`NoSuchTransition`), no row for this actor (`ActorNotAllowed`), a row that names an agent the contract does not (`NotTheNamedAgent`), or a gate that refuses (`GateFailed`). The steps are ordered from the most structural, as `check_contract_write` orders its nine: a caller told the actor is wrong when the move does not exist at all would fix the wrong thing.
- More than one row can carry one move, so `GateFailed` reports a list rather than one gate. Three rows send a `refining` contract to `escalated` (the readiness failures, the contract's acceptance, the governor's own reasons), and a blocked or rejected task has its own row and the governor's. The move is allowed when any row's gate opens, taken in table order, so the more specific reason is the one recorded; when none opens, the refusal carries one `GateFailure` per row tried, each with everything that gate said. Changed from the project plan's `GateFailed { gate, details }`, which could name only one and would have reported the first row's gate for a move refused by three.
- A row whose actor is the assignee or the reviewer is open only to the agent the contract names in that field, and `NotTheNamedAgent` says which agent it wanted. The Scrum Master and the Product Manager hold team roles rather than a place on the contract, and the governor and the human are not agents, so their rows ask nothing about an agent id and the runtime's own authentication is what says who asked. Added by this plan; the project plan named `agent_id` in the request without saying what reads it.
- A task the contract names no agent for has no assignee row and no reviewer row: an unassigned task cannot declare itself done, however the request is spelled, and a blank id names nobody. Ids are compared once trimmed, as everywhere else in the governor.
- `TransitionEffect::RaiseEscalation` carries the `EscalationReason`, because the reason is a fact of the row that opened rather than something the runtime could work out afterwards: the same move to `escalated` means the readiness failures, an approval, a risk gate, a blocker's age, an iteration limit, a budget, a permission, or the user asking. Changed from the project plan's payload-free variant.
- The reason comes from the gate that opened the row: `ReadinessExhausted` is `readiness_failures`, `ContractRequiresHuman` is `approval` for an epic and `risk_gate` for a task (5.16 item 2 against 5.4 item 5), `BlockedAge` is `blocker_age`, `IterationLimitReached` is `iterations`, and the governor's own row is `sessions` when the task's sessions ran out and `budget` for every other exhausted budget, or `permission` when nothing is exhausted and a permission was denied. A gate-free row into `escalated` is the human's, so it is `explicit_request`. The match is exhaustive over `GateId` and a test walks the table to prove no other gate reaches `escalated`.
- A `stop` from the user reaches the table as the human's own `any -> escalated` row, which needs no gate, not through the governor's. Spec 5.2's Governor row listed it, and this step takes it out: the governor cannot know a user has asked for a stop, and the context would need a field the runtime sets from an event the human's row already carries.
- The effects of a move, in this order: `IncrementIteration` when the task enters `rejected`, because that is the rejection the limit on `rejected -> in_progress` counts; `RaiseEscalation` when it enters `escalated`; `ResetBlocker` when it leaves `blocked` for `in_progress`, because the blocker it carried is answered and a stale one would age again. A task that escalates out of `blocked` keeps its blocker: that is what the user is being shown.
- The iteration counts at the rejection rather than at the return, so that the count the gate reads is the number of rejections so far. Rejected: counting at `rejected -> in_progress`, which would let a task be rejected without limit as long as nobody sent it back.
- `TransitionContext` carries the values rather than a trait the runtime implements, because `farik-core` does no I/O and a trait would let the world in through the back door. It is not `PartialEq`: the generated `TaskContract` is not, and a context is a bundle of inputs rather than a value to compare.
- The two halves of "is this contract waiting for the human?" travel together as `ContractAcceptance { required_by_policy, given }`, and the gate passes when the team's policy requires it **or** the contract's own risk or kind does (`done::requires_human_acceptance`), and the human has not answered. Either half is enough, so the two cannot disagree in the direction that would carry a task past the human. `TeamRules` does not yet carry `human_accepts_contracts` (5.12), which is why the policy half is passed in. Changed from the project plan's two bools, which were also one bool too many for clippy's pedantic limit on a struct.
- `iteration` and `max_iterations` are converted with `u32::try_from` and saturate at `u32::MAX`. A count the schema's `u64` holds and `u32` cannot is a corrupt figure, and saturating sends the task to the human at the next rejection rather than letting it be worked for ever; a limit above `u32::MAX` is what the contract asked for, which is effectively none. Rejected: refusing the move, because a corrupt count would then freeze the task instead of escalating it.
- A gate that needs a value the runtime did not gather refuses and says so: no pair of agents for an assignment, no time for a block. Rejected: passing, which would move a task on a value nobody supplied.
- `CriteriaRecorded` asks `check_children_done` for an epic and `check_criteria_recorded` for a task, from the contract's own `kind`. The table has one row and 5.16 item 4 gives an epic the other question.
- The details of a gate that wraps a longer answer are that answer's own messages, in its own order: the Definition of Ready's failures, the Definition of Done's failures, and each gate predicate's reasons. Nothing is reworded here, so an agent reads the same sentence wherever it meets the rule.
- The blocked limit is a field of the context rather than a constant read here, because it is a team setting; `escalation::DEFAULT_BLOCKED_LIMIT` is what a runtime with no setting passes. The message says the limit in seconds, which is what the value carries.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

One task: the `governor::transition` module with `evaluate_transition`, the request, the context, the decision, the effects, the refusals, and twenty-one tests. Twenty of the tests are one gate or one refusal each; the twenty-first walks `TRANSITION_TABLE` and opens every one of its twenty rows with the state that belongs to it, which is what spec F5 asks for and what makes a row added to 5.2 fail until somebody says what opens it.

Out of scope: applying the decision, which is phase 3's runtime; the escalation record itself (`Escalation` from step 06), which the runtime fills from the reason this returns; the integration escalation of 5.14, which has no row in the table; and anything that reads a clock, a file, or the log.

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `budget::{BudgetScope, BudgetState, check_budgets}` and `contract::{TaskContract, TaskId, TaskStatus}` from phase 0 and step 05, the generated `Kind`, `governor::done::{CriterionResult, DoneEvidence, evaluate_done, requires_human_acceptance}` from step 07, `governor::escalation`'s three counting rules and `EscalationReason` from step 06, the nine gate predicates and their values from step 08, `governor::readiness::{ReadinessContext, evaluate_readiness}` from step 02, and `governor::transition_table::{GateId, TransitionActor, TransitionRow, find_transitions}` from step 01. Adds no dependency.

## Global constraints

- `farik-core` does no I/O and reads no clock: the runtime's `now` is a field of the context. `cargo xtask core-io` passes.
- Every public item carries a doc comment, and every function returning a `Result` carries an `# Errors` section; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                          modifies: declares transition
crates/core/src/governor/transition.rs               creates: evaluate_transition, the request, the context, the decision, the effects, the refusals, twenty-one tests
docs/SPEC.md                                         modifies: section 5.2's governor row stops claiming the user's stop, and one paragraph says how several rows carrying one move are ordered, whose an assignee's or a reviewer's row is, what each escalation's reason is, and what a rejection and a return from blocked record
docs/plans/project-plan.md                           modifies: phase 1 step 09's interface records the changes this plan makes to it
docs/plans/phase-1-harness/step-09-transition-evaluation.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: Transition evaluation

Files: created `crates/core/src/governor/transition.rs`; modified `crates/core/src/governor.rs`

Consumes: `std::time::Duration`; `chrono::{DateTime, Utc}`; `budget::{BudgetScope, BudgetState, check_budgets}`; `contract::{TaskContract, TaskId, TaskStatus}`; `generated::task_contract::FarikTaskContractKind`; `governor::done::{CriterionResult, DoneEvidence, evaluate_done, requires_human_acceptance}`; `governor::escalation::{BlockedAge, EscalationReason, READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome, evaluate_blocked_age, evaluate_readiness_attempts, evaluate_rejection}`; `governor::gates::{AssignmentInput, Blocker, ChildState, GateResult, Rejection, WorkState, check_assignment, check_blocker_resolved, check_blocker_written, check_children_done, check_criteria_recorded, check_rejection_reasons}`; `governor::readiness::{ReadinessContext, evaluate_readiness}`; `governor::transition_table::{GateId, TransitionActor, TransitionRow, find_transitions}`; and in the tests `chrono::TimeZone`, `budget::{DEFAULT_DAY_BUDGET_USD, DEFAULT_SESSION_LIMITS, DEFAULT_SPRINT_BUDGET_USD, SessionLedger}`, `contract::Role`, the generated `Risk`, `governor::done::RunBy`, `governor::escalation::{DEFAULT_BLOCKED_LIMIT, evaluate_rejection}`, `governor::gates::AssignmentRequester`, `governor::readiness::fixtures::{a_contract, a_ready_context}` and `governor::transition_table::{Status, TRANSITION_TABLE}`
Produces: `governor::transition::{TransitionRequest, ContractAcceptance, TransitionContext, TransitionEffect, TransitionDecision, GateFailure, TransitionRefusal, evaluate_transition}`

- [ ] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
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
  /// Transition evaluation: the table, the actors, and every gate as one decision
  /// (`docs/SPEC.md` section 5.2).
  pub mod transition;
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```

- [ ] Write the failing tests. `crates/core/src/governor/transition.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::time::Duration;

      use chrono::{DateTime, TimeZone, Utc};

      use super::{
          ContractAcceptance, GateFailure, TransitionContext, TransitionDecision, TransitionEffect,
          TransitionRefusal, TransitionRequest, evaluate_transition,
      };
      use crate::budget::{
          BudgetState, DEFAULT_DAY_BUDGET_USD, DEFAULT_SESSION_LIMITS, DEFAULT_SPRINT_BUDGET_USD,
          SessionLedger,
      };
      use crate::contract::{Role, TaskStatus};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::generated::task_contract::FarikTaskContractRisk as Risk;
      use crate::governor::done::{CriterionResult, DoneEvidence, RunBy};
      use crate::governor::escalation::{
          DEFAULT_BLOCKED_LIMIT, EscalationReason as Why, RejectionOutcome,
      };
      use crate::governor::gates::{
          AssignmentInput, AssignmentRequester, Blocker, ChildState, Rejection, WorkState,
      };
      use crate::governor::readiness::fixtures::{a_contract, a_ready_context};
      use crate::governor::transition_table::{
          GateId, Status, TRANSITION_TABLE, TransitionActor as A,
      };

      fn at(hour: u32) -> DateTime<Utc> {
          Utc.with_ymd_and_hms(2026, 9, 16, hour, 0, 0)
              .single()
              .expect("a real hour")
      }

      fn an_assignment() -> AssignmentInput {
          AssignmentInput {
              requested_by: AssignmentRequester::ScrumMaster,
              has_active_scrum_master: true,
              assignee_id: "dev-1".to_string(),
              assignee_role: Role::SoftwareDeveloper,
              reviewer_id: "arch-1".to_string(),
              reviewer_role: Role::Architect,
              assignee_open_tasks: 0,
              wip_limit: 2,
              remaining_sprint_budget_usd: 15.0,
              dependencies: Vec::new(),
          }
      }

      fn a_result(run_by: RunBy) -> CriterionResult {
          CriterionResult {
              criterion_id: "C1".to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by,
          }
      }

      fn a_budget() -> BudgetState {
          BudgetState {
              session: SessionLedger::default(),
              session_limits: DEFAULT_SESSION_LIMITS,
              task_spent_usd: 1.0,
              task_max_usd: 5.0,
              task_sessions: 1,
              task_max_sessions: 5,
              sprint_spent_usd: 3.0,
              sprint_max_usd: DEFAULT_SPRINT_BUDGET_USD,
              day_spent_usd: 4.0,
              day_max_usd: DEFAULT_DAY_BUDGET_USD,
          }
      }

      /// A task in `in_progress`, assigned to `dev-1` and reviewed by `arch-1`, with everything a
      /// gate could ask for in the state that lets it pass, so that a test sets only what it is
      /// about.
      fn a_context() -> TransitionContext {
          let mut contract = a_contract();
          contract.assignee = Some("dev-1".to_string());
          contract.reviewer = Some("arch-1".to_string());
          TransitionContext {
              status: TaskStatus::InProgress,
              contract,
              triaged: true,
              children: Vec::new(),
              readiness: a_ready_context(),
              readiness_failed_attempts: 1,
              acceptance: ContractAcceptance::default(),
              assignment: Some(an_assignment()),
              assignee_results: vec![a_result(RunBy::Assignee)],
              work: WorkState {
                  commits: 1,
                  worktree_clean: true,
              },
              blocker: Some(Blocker {
                  description: "The staging database refuses the migration.".to_string(),
                  needed: "A password for the staging database.".to_string(),
              }),
              blocker_resolution: Some("The password is in the vault.".to_string()),
              blocked_at: Some(at(1)),
              now: at(2),
              blocked_limit: DEFAULT_BLOCKED_LIMIT,
              done: DoneEvidence {
                  results: vec![a_result(RunBy::Reviewer)],
                  changed_paths: vec!["src/login/form.rs".to_string()],
                  completion_note: Some("The form takes an email and a password.".to_string()),
                  review_note: Some("C1: cargo test, 11 passed.".to_string()),
                  human_accepted: false,
              },
              rejection: Some(Rejection {
                  failed_criterion_ids: vec!["C1".to_string()],
                  reasons: "The form accepts an empty password.".to_string(),
              }),
              budget: a_budget(),
              permission_denied: false,
          }
      }

      fn ask(to: TaskStatus, actor: A, agent_id: Option<&str>) -> TransitionRequest {
          TransitionRequest {
              task_id: "FRK-1".parse().expect("a task id"),
              to,
              actor,
              agent_id: agent_id.map(str::to_string),
          }
      }

      fn decide(
          request: &TransitionRequest,
          context: &TransitionContext,
      ) -> Result<TransitionDecision, TransitionRefusal> {
          evaluate_transition(request, context)
      }

      fn effects(request: &TransitionRequest, context: &TransitionContext) -> Vec<TransitionEffect> {
          decide(request, context)
              .expect("expected the move to be allowed")
              .effects
      }

      fn gates(request: &TransitionRequest, context: &TransitionContext) -> Vec<GateFailure> {
          match decide(request, context) {
              Err(TransitionRefusal::GateFailed { failures }) => failures,
              other => panic!("expected a gate refusal, got {other:?}"),
          }
      }

      fn one_gate(request: &TransitionRequest, context: &TransitionContext) -> (GateId, Vec<String>) {
          let mut failures = gates(request, context);
          assert_eq!(failures.len(), 1, "expected one gate to be tried");
          let failure = failures.remove(0);
          (failure.gate, failure.details)
      }

      #[test]
      fn refuses_a_move_the_table_has_no_row_for() {
          let mut context = a_context();
          assert_eq!(
              decide(
                  &ask(TaskStatus::Accepted, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::InProgress,
                  to: TaskStatus::Accepted
              })
          );
          // Nothing leaves a terminal status, and no row means staying put.
          context.status = TaskStatus::Accepted;
          assert_eq!(
              decide(&ask(TaskStatus::InProgress, A::Human, None), &context),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::Accepted,
                  to: TaskStatus::InProgress
              })
          );
          context.status = TaskStatus::InProgress;
          assert_eq!(
              decide(
                  &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::InProgress,
                  to: TaskStatus::InProgress
              })
          );
      }

      #[test]
      fn refuses_an_actor_no_row_names_and_says_who_may_ask() {
          let mut context = a_context();
          context.status = TaskStatus::Ready;
          assert_eq!(
              decide(
                  &ask(TaskStatus::Assigned, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::ActorNotAllowed {
                  actor: A::Assignee,
                  allowed: vec![A::ScrumMaster, A::ProductManager]
              })
          );
      }

      #[test]
      fn opens_the_assignees_row_only_to_the_agent_the_contract_names() {
          let mut context = a_context();
          assert_eq!(
              decide(
                  &ask(TaskStatus::Verifying, A::Assignee, Some("dev-2")),
                  &context
              ),
              Err(TransitionRefusal::NotTheNamedAgent {
                  actor: A::Assignee,
                  named: Some("dev-1".to_string()),
                  asked: Some("dev-2".to_string())
              })
          );
          // A stray space is the same agent.
          assert!(
              decide(
                  &ask(TaskStatus::Verifying, A::Assignee, Some(" dev-1 ")),
                  &context
              )
              .is_ok()
          );
          // An unassigned task has no assignee to ask for it, however the request is spelled.
          context.contract.assignee = None;
          for asked in [None, Some("dev-1"), Some("  ")] {
              assert_eq!(
                  decide(&ask(TaskStatus::Verifying, A::Assignee, asked), &context),
                  Err(TransitionRefusal::NotTheNamedAgent {
                      actor: A::Assignee,
                      named: None,
                      asked: asked.map(str::to_string)
                  }),
                  "{asked:?}"
              );
          }
      }

      #[test]
      fn opens_the_reviewers_row_only_to_the_reviewer_the_contract_names() {
          let context = a_context();
          let mut verifying = a_context();
          verifying.status = TaskStatus::Verifying;
          assert_eq!(
              decide(
                  &ask(TaskStatus::Rejected, A::Reviewer, Some("dev-1")),
                  &verifying
              ),
              Err(TransitionRefusal::NotTheNamedAgent {
                  actor: A::Reviewer,
                  named: Some("arch-1".to_string()),
                  asked: Some("dev-1".to_string())
              })
          );
          assert!(
              decide(
                  &ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1")),
                  &verifying
              )
              .is_ok()
          );
          // The Scrum Master, the Product Manager, the governor and the human hold no place on the
          // contract, so their rows ask nothing about an agent id.
          let mut blocked = context;
          blocked.status = TaskStatus::Blocked;
          assert!(decide(&ask(TaskStatus::InProgress, A::ScrumMaster, None), &blocked).is_ok());
      }

      #[test]
      fn refines_a_draft_request_only_once_it_is_triaged() {
          let mut context = a_context();
          context.status = TaskStatus::Draft;
          let request = ask(TaskStatus::Refining, A::ProductManager, Some("pm-1"));
          assert_eq!(effects(&request, &context), []);
          context.triaged = false;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Triaged,
                  vec![
                      "the request has no recorded triage decision, and refining starts from one (5.16)"
                          .to_string()
                  ]
              )
          );
      }

      #[test]
      fn readies_a_contract_that_passes_the_definition_of_ready() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          let request = ask(TaskStatus::Ready, A::Governor, None);
          assert_eq!(effects(&request, &context), []);
          // The gate's details are the Definition of Ready's own messages, in its own order.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::DefinitionOfReady);
          assert_eq!(
              details,
              vec!["the intent is blank; state the user-facing reason for the task".to_string()]
          );
      }

      #[test]
      fn escalates_a_contract_that_has_failed_the_definition_of_ready_three_times() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
          );
          // Below the limit, all three of the governor's rows into `escalated` are tried, in table
          // order, and each says what it is waiting for.
          context.readiness_failed_attempts = 2;
          let failures = gates(&request, &context);
          assert_eq!(
              failures
                  .iter()
                  .map(|failure| failure.gate)
                  .collect::<Vec<GateId>>(),
              [
                  GateId::ReadinessExhausted,
                  GateId::ContractRequiresHuman,
                  GateId::GovernorEscalation
              ]
          );
          assert_eq!(
              failures[0].details,
              vec![
                  "the contract has failed the Definition of Ready 2 times, and it is refined again until 3"
                      .to_string()
              ]
          );
      }

      #[test]
      fn escalates_a_contract_that_waits_for_the_human_and_names_why() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 0;
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          // An epic waits for the user's approval (5.16 item 2); a high-risk task waits on the risk
          // gate (5.4 item 5). The reason the escalation carries says which.
          context.contract.kind = Kind::Epic;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
          context.contract.kind = Kind::Task;
          context.contract.risk = Risk::High;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::RiskGate)]
          );
          // The team's policy is the other half of the question, and either half is enough.
          context.contract.risk = Risk::Medium;
          context.acceptance.required_by_policy = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::RiskGate)]
          );
          // A contract the human has already accepted goes to `ready`, not to the human again.
          context.acceptance.given = true;
          let details = gates(&request, &context)[1].details.clone();
          assert_eq!(
              details,
              vec![
                  "the human has already accepted this contract, so it goes to ready rather than escalating"
                      .to_string()
              ]
          );
          // And one that needs nobody's acceptance says that instead.
          context.acceptance = ContractAcceptance::default();
          let details = gates(&request, &context)[1].details.clone();
          assert_eq!(
              details,
              vec![
                  "this contract does not need the human's acceptance: the risk is not high, it is not an epic, and the team's policy does not ask for it"
                      .to_string()
              ]
          );
      }

      #[test]
      fn assigns_a_task_through_the_assignment_gate() {
          let mut context = a_context();
          context.status = TaskStatus::Ready;
          let request = ask(TaskStatus::Assigned, A::ScrumMaster, Some("sm-1"));
          assert_eq!(effects(&request, &context), []);
          // The gate's details are the assignment gate's own, and a runtime that named no pair of
          // agents is told so rather than refused for a rule it could not have met.
          context.assignment = None;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Assignment,
                  vec!["the runtime named no pair of agents for this assignment".to_string()]
              )
          );
      }

      #[test]
      fn verifies_a_task_on_its_own_runs_and_an_epic_on_its_tasks() {
          let mut context = a_context();
          let request = ask(TaskStatus::Verifying, A::Assignee, Some("dev-1"));
          assert_eq!(effects(&request, &context), []);
          context.assignee_results = Vec::new();
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::CriteriaRecorded,
                  vec!["the assignee recorded no run with evidence for criterion C1".to_string()]
              )
          );
          // An epic is verified on its tasks instead, and nothing asks its assignee for a run.
          context.contract.kind = Kind::Epic;
          context.children = vec![ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Accepted,
          }];
          assert_eq!(effects(&request, &context), []);
          context.children = vec![ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::InProgress,
          }];
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::CriteriaRecorded);
          assert_eq!(details.len(), 2);
      }

      #[test]
      fn blocks_a_task_with_a_written_blocker_and_clears_it_with_a_resolution() {
          let mut context = a_context();
          let block = ask(TaskStatus::Blocked, A::Assignee, Some("dev-1"));
          assert_eq!(effects(&block, &context), []);
          context.blocker = None;
          assert_eq!(one_gate(&block, &context).0, GateId::BlockerWritten);
          // Leaving `blocked` for work clears the blocker, whoever asked.
          let mut blocked = a_context();
          blocked.status = TaskStatus::Blocked;
          for actor in [A::ScrumMaster, A::Human] {
              assert_eq!(
                  effects(&ask(TaskStatus::InProgress, actor, None), &blocked),
                  [TransitionEffect::ResetBlocker],
                  "{actor:?}"
              );
          }
          blocked.blocker_resolution = None;
          assert_eq!(
              one_gate(&ask(TaskStatus::InProgress, A::Human, None), &blocked).0,
              GateId::BlockerResolved
          );
      }

      #[test]
      fn escalates_a_task_blocked_for_the_limit_or_longer() {
          let mut context = a_context();
          context.status = TaskStatus::Blocked;
          context.blocked_limit = Duration::from_secs(3600);
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::BlockerAge)]
          );
          // Within the limit, the age row says so; the governor's own row is tried after it.
          context.blocked_limit = Duration::from_secs(7200);
          context.now = at(2);
          let failures = gates(&request, &context);
          assert_eq!(
              failures
                  .iter()
                  .map(|failure| failure.gate)
                  .collect::<Vec<GateId>>(),
              [GateId::BlockedAge, GateId::GovernorEscalation]
          );
          assert_eq!(
              failures[0].details,
              vec!["the task has not been blocked for 7200 seconds yet".to_string()]
          );
          // A block the runtime stamped no time for cannot be aged.
          context.blocked_at = None;
          assert_eq!(
              gates(&request, &context)[0].details,
              vec![
                  "the runtime recorded no time for this block, and a blocked task escalates on its age"
                      .to_string()
              ]
          );
          // With the age row shut and a budget exhausted, the governor's own row carries the move,
          // and the reason follows the row that opened rather than the one that did not.
          context.budget.day_spent_usd = context.budget.day_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
      }

      #[test]
      fn accepts_a_task_on_the_definition_of_done() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Accepted, A::ProductManager, Some("pm-1"));
          assert_eq!(effects(&request, &context), []);
          context.done.review_note = None;
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::DefinitionOfDone);
          assert_eq!(
              details,
              vec![
                  "the reviewer wrote no review note mapping each criterion to its evidence"
                      .to_string()
              ]
          );
      }

      #[test]
      fn rejects_work_with_written_reasons_and_counts_the_iteration() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1"));
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::IncrementIteration]
          );
          context.rejection = None;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::RejectionReasons,
                  vec![
                      "work is rejected with written reasons mapped to the criteria that failed"
                          .to_string()
                  ]
              )
          );
      }

      #[test]
      fn works_a_rejected_task_again_until_the_iteration_limit() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          let again = ask(TaskStatus::InProgress, A::Governor, None);
          let escalate = ask(TaskStatus::Escalated, A::Governor, None);
          // The contract's default limit is three; the iteration counts the rejections so far.
          context.contract.iteration = 2;
          assert_eq!(effects(&again, &context), []);
          assert_eq!(
              gates(&escalate, &context)[0].details,
              vec![
                  "the task has been rejected 2 times and the limit is 3, so it is worked again rather than escalated"
                      .to_string()
              ]
          );
          context.contract.iteration = 3;
          assert_eq!(
              effects(&escalate, &context),
              [TransitionEffect::RaiseEscalation(Why::Iterations)]
          );
          assert_eq!(
              one_gate(&again, &context),
              (
                  GateId::IterationBelowLimit,
                  vec![
                      "the task has been rejected 3 times and the limit is 3, so it escalates rather than being worked again"
                          .to_string()
                  ]
              )
          );
          // A count the schema's `u64` can hold but `u32` cannot is a corrupt figure, and the safe
          // reading sends the task to the human rather than working it for ever.
          context.contract.iteration = u64::from(u32::MAX) + 1;
          assert_eq!(
              effects(&escalate, &context),
              [TransitionEffect::RaiseEscalation(Why::Iterations)]
          );
      }

      #[test]
      fn escalates_on_an_exhausted_budget_or_a_denied_permission() {
          let mut context = a_context();
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::GovernorEscalation,
                  vec![
                      "no budget is exhausted and no permission was denied, so the governor has nothing to escalate"
                          .to_string()
                  ]
              )
          );
          // The task's sessions are their own reason (5.7); every other budget is `budget`.
          context.budget.task_sessions = context.budget.task_max_sessions;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Sessions)]
          );
          context.budget = a_budget();
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          context.budget = a_budget();
          context.permission_denied = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Permission)]
          );
      }

      #[test]
      fn lets_the_human_escalate_or_cancel_anything_and_move_an_escalated_task() {
          let mut context = a_context();
          assert_eq!(
              effects(&ask(TaskStatus::Escalated, A::Human, None), &context),
              [TransitionEffect::RaiseEscalation(Why::ExplicitRequest)]
          );
          assert_eq!(
              effects(&ask(TaskStatus::Cancelled, A::Human, None), &context),
              []
          );
          // From `escalated` the human may put the task anywhere the lifecycle has room for.
          context.status = TaskStatus::Escalated;
          for to in [
              TaskStatus::Refining,
              TaskStatus::Ready,
              TaskStatus::InProgress,
              TaskStatus::Accepted,
          ] {
              assert!(decide(&ask(to, A::Human, None), &context).is_ok(), "{to}");
          }
          // An agent cannot: the row is the human's.
          assert_eq!(
              decide(
                  &ask(TaskStatus::Ready, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::ActorNotAllowed {
                  actor: A::Assignee,
                  allowed: vec![A::Human]
              })
          );
      }

      /// One row of the table, the move that takes it, and what about the state opens that row
      /// rather than another.
      struct Case {
          row: usize,
          from: TaskStatus,
          to: TaskStatus,
          actor: A,
          prepare: fn(&mut TransitionContext),
      }

      const fn case(
          row: usize,
          from: TaskStatus,
          to: TaskStatus,
          actor: A,
          prepare: fn(&mut TransitionContext),
      ) -> Case {
          Case {
              row,
              from,
              to,
              actor,
              prepare,
          }
      }

      /// One case per row of `TRANSITION_TABLE`, in its order.
      fn every_row() -> [Case; 20] {
          use TaskStatus as S;
          let no_change: fn(&mut TransitionContext) = |_| {};
          [
              case(0, S::Draft, S::Refining, A::ProductManager, no_change),
              case(1, S::Refining, S::Ready, A::Governor, no_change),
              case(2, S::Refining, S::Escalated, A::Governor, |context| {
                  context.readiness_failed_attempts = 3;
              }),
              case(3, S::Refining, S::Escalated, A::Governor, |context| {
                  context.readiness_failed_attempts = 0;
                  context.contract.risk = Risk::High;
              }),
              case(4, S::Ready, S::Assigned, A::ScrumMaster, no_change),
              case(5, S::Ready, S::Assigned, A::ProductManager, |context| {
                  let assignment = context.assignment.as_mut().expect("the fixture assigns");
                  assignment.requested_by = AssignmentRequester::ProductManager;
                  assignment.has_active_scrum_master = false;
              }),
              case(6, S::Assigned, S::InProgress, A::Assignee, no_change),
              case(7, S::InProgress, S::Verifying, A::Assignee, no_change),
              case(8, S::InProgress, S::Blocked, A::Assignee, no_change),
              case(9, S::Blocked, S::InProgress, A::ScrumMaster, no_change),
              case(10, S::Blocked, S::InProgress, A::Human, no_change),
              case(11, S::Blocked, S::Escalated, A::Governor, |context| {
                  context.blocked_limit = Duration::from_secs(3600);
              }),
              case(12, S::Verifying, S::Accepted, A::ProductManager, no_change),
              case(13, S::Verifying, S::Rejected, A::Reviewer, no_change),
              case(14, S::Rejected, S::InProgress, A::Governor, no_change),
              case(15, S::Rejected, S::Escalated, A::Governor, |context| {
                  context.contract.iteration = 3;
              }),
              case(16, S::InProgress, S::Escalated, A::Governor, |context| {
                  context.permission_denied = true;
              }),
              case(17, S::InProgress, S::Escalated, A::Human, no_change),
              case(18, S::InProgress, S::Cancelled, A::Human, no_change),
              case(19, S::Escalated, S::Ready, A::Human, no_change),
          ]
      }

      #[test]
      fn opens_every_row_of_the_table_with_the_state_that_belongs_to_it() {
          // Spec F5 asks for a test for every transition. `every_row` names, for each row, the move
          // that takes it and the one thing about the state that opens that row rather than another;
          // the last assertion is that the cases cover the table, so a row added to 5.2 fails here
          // until somebody says what opens it.
          let mut covered: Vec<usize> = Vec::new();
          for case in every_row() {
              let mut context = a_context();
              context.status = case.from;
              (case.prepare)(&mut context);
              let agent_id = match case.actor {
                  A::Assignee => Some("dev-1"),
                  A::Reviewer => Some("arch-1"),
                  A::ScrumMaster | A::ProductManager | A::Governor | A::Human => None,
              };
              let decision = decide(&ask(case.to, case.actor, agent_id), &context)
                  .unwrap_or_else(|refusal| panic!("row {} was shut: {refusal:?}", case.row));
              assert_eq!(
                  *decision.row, TRANSITION_TABLE[case.row],
                  "row {}: took {:?}",
                  case.row, decision.row
              );
              covered.push(case.row);
          }
          covered.sort_unstable();
          assert_eq!(
              covered,
              (0..TRANSITION_TABLE.len()).collect::<Vec<usize>>(),
              "every row of the table has a case"
          );
      }

      #[test]
      fn names_a_reason_for_every_row_of_the_table_that_escalates() {
          // The reason an escalation carries comes from the gate that opened the row, and these are
          // the only gates the table pairs with `escalated`: a row added with another gate would
          // record the user as the reason, so this test is what says none does.
          let escalating: Vec<GateId> = TRANSITION_TABLE
              .iter()
              .filter(|row| row.to == Status::Is(TaskStatus::Escalated))
              .map(|row| row.gate)
              .collect();
          assert_eq!(
              escalating,
              [
                  GateId::ReadinessExhausted,
                  GateId::ContractRequiresHuman,
                  GateId::BlockedAge,
                  GateId::IterationLimitReached,
                  GateId::GovernorEscalation,
                  GateId::None
              ]
          );
      }

      #[test]
      fn takes_the_first_row_whose_gate_opens() {
          // A `refining` contract that has failed the Definition of Ready three times and has a
          // budget exhausted could escalate for either reason. The table's order decides, so that
          // the more specific reason is the one the user reads.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          context.budget.day_spent_usd = context.budget.day_max_usd;
          let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
              .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::ReadinessExhausted);
          assert_eq!(
              decision.effects,
              [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
          );
          assert_eq!(decision.from, TaskStatus::Refining);
          assert_eq!(decision.to, TaskStatus::Escalated);
      }

      #[test]
      fn starts_a_session_on_an_assigned_task_with_no_gate_at_all() {
          let mut context = a_context();
          context.status = TaskStatus::Assigned;
          let decision = decide(
              &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
              &context,
          )
          .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::None);
          assert_eq!(decision.effects, []);
          // And the rejection outcome of step 06 is what the iteration gates read, not a count of
          // their own: at the default limit a fresh task is worked rather than escalated.
          assert_eq!(
              crate::governor::escalation::evaluate_rejection(0, 3),
              RejectionOutcome::ReturnToInProgress
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::transition
  # expected, among the output:
  # error[E0432]: unresolved imports `super::ContractAcceptance`, `super::GateFailure`, `super::TransitionContext`, `super::TransitionDecision`, `super::TransitionEffect`, `super::TransitionRefusal`, `super::TransitionRequest`, `super::evaluate_transition`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [ ] Write the implementation above the tests. `crates/core/src/governor/transition.rs` in full:

  ```rust
  //! Transition evaluation (`docs/SPEC.md` section 5.2, F5): the one question the runtime asks
  //! before it moves a task. It composes the table of 5.2, the actor rules of 5.1, the gate
  //! predicates, the Definition of Ready, the Definition of Done, and the counting rules of 5.5 and
  //! 5.7 into one answer: the row the move takes and what the runtime must record with it, or every
  //! reason the move is refused.
  //!
  //! Nothing here reads the world. The runtime gathers the values, this decides, and the runtime
  //! applies what it is told.

  use std::time::Duration;

  use chrono::{DateTime, Utc};

  use crate::budget::{BudgetScope, BudgetState, check_budgets};
  use crate::contract::{TaskContract, TaskId, TaskStatus};
  use crate::generated::task_contract::FarikTaskContractKind as Kind;
  use crate::governor::done::{
      CriterionResult, DoneEvidence, evaluate_done, requires_human_acceptance,
  };
  use crate::governor::escalation::{
      BlockedAge, EscalationReason, READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome,
      evaluate_blocked_age, evaluate_readiness_attempts, evaluate_rejection,
  };
  use crate::governor::gates::{
      AssignmentInput, Blocker, ChildState, GateResult, Rejection, WorkState, check_assignment,
      check_blocker_resolved, check_blocker_written, check_children_done, check_criteria_recorded,
      check_rejection_reasons,
  };
  use crate::governor::readiness::{ReadinessContext, evaluate_readiness};
  use crate::governor::transition_table::{GateId, TransitionActor, TransitionRow, find_transitions};

  /// A request to move one task to one status, by one actor.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct TransitionRequest {
      /// The task to move.
      pub task_id: TaskId,
      /// The status it would enter.
      pub to: TaskStatus,
      /// Which of the lifecycle's actors is asking.
      pub actor: TransitionActor,
      /// The asking agent's id, when an agent rather than the human or the governor. A row that
      /// names the assignee or the reviewer opens only to the agent the contract names.
      pub agent_id: Option<String>,
  }

  /// Whether the human must accept the contract the task has now, and whether they have. The two
  /// travel together because one question is asked of them: is this contract waiting for the human?
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub struct ContractAcceptance {
      /// Whether the team's policy requires the human to accept this contract, over and above the
      /// risk and the kind that `done::requires_human_acceptance` answers
      /// (`human_accepts_contracts`, `docs/SPEC.md` section 5.12).
      pub required_by_policy: bool,
      /// Whether the human has accepted the contract the task has now. An edit of a frozen contract
      /// sends the task back to `refining` (5.11), and the acceptance it had does not carry over.
      pub given: bool,
  }

  /// Everything a gate of the table can ask, gathered by the runtime before it asks. Not `PartialEq`:
  /// the generated `TaskContract` is not, and a context is a bundle of inputs rather than a value to
  /// compare.
  #[derive(Debug, Clone)]
  pub struct TransitionContext {
      /// The task's status now, before the move.
      pub status: TaskStatus,
      /// Its contract as it stands.
      pub contract: TaskContract,
      /// Whether the request behind this task has a recorded triage decision (`docs/SPEC.md`
      /// section 5.16).
      pub triaged: bool,
      /// The tasks under this epic, for an epic's own `CriteriaRecorded` gate. Empty for a task.
      pub children: Vec<ChildState>,
      /// What the Definition of Ready needs.
      pub readiness: ReadinessContext,
      /// How many times the contract has failed the Definition of Ready, counting the failure that
      /// has just happened, so that the third failure escalates (`docs/SPEC.md` section 5.2).
      pub readiness_failed_attempts: u32,
      /// Whether this contract is waiting for the human, and whether the human has answered.
      pub acceptance: ContractAcceptance,
      /// The pair of agents an assignment would name, when the move is an assignment.
      pub assignment: Option<AssignmentInput>,
      /// The results the assignee recorded from its own runs.
      pub assignee_results: Vec<CriterionResult>,
      /// The state of the task's branch and worktree.
      pub work: WorkState,
      /// What the assignee wrote when it blocked the task.
      pub blocker: Option<Blocker>,
      /// What cleared the blocker.
      pub blocker_resolution: Option<String>,
      /// When the task blocked, if it is blocked.
      pub blocked_at: Option<DateTime<Utc>>,
      /// The runtime's clock, passed in because `farik-core` reads no clock of its own.
      pub now: DateTime<Utc>,
      /// How long a task may stay blocked before it escalates
      /// (`escalation::DEFAULT_BLOCKED_LIMIT` when the team set nothing).
      pub blocked_limit: Duration,
      /// The evidence gathered for the Definition of Done.
      pub done: DoneEvidence,
      /// What the reviewer wrote when it rejected the work.
      pub rejection: Option<Rejection>,
      /// Every budget's spend and limit.
      pub budget: BudgetState,
      /// Whether a permission was denied on an action the task requires.
      pub permission_denied: bool,
  }

  /// What the runtime must record along with the move.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TransitionEffect {
      /// The contract's `iteration` goes up by one: the task has been rejected once more.
      IncrementIteration,
      /// An escalation is raised, for this reason (`docs/SPEC.md` section 5.7).
      RaiseEscalation(EscalationReason),
      /// The blocker and its time are cleared: the task is moving again.
      ResetBlocker,
  }

  /// The move the governor will apply.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct TransitionDecision {
      /// The status the task leaves.
      pub from: TaskStatus,
      /// The status it enters, the request's own rather than the row's pattern.
      pub to: TaskStatus,
      /// The row of the table the move takes.
      pub row: &'static TransitionRow,
      /// What the runtime records with it, in this order.
      pub effects: Vec<TransitionEffect>,
  }

  /// One gate that refused, and everything it refused for.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct GateFailure {
      /// Which gate.
      pub gate: GateId,
      /// Every reason it gives, in the order the gate writes them.
      pub details: Vec<String>,
  }

  /// Why a move is refused.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum TransitionRefusal {
      /// No row of the table moves a task from this status to that one. `transitions_from` says
      /// what this status can reach, for a caller that wants to tell the agent what to ask instead.
      NoSuchTransition {
          /// The status the task is in.
          from: TaskStatus,
          /// The status the request asked for.
          to: TaskStatus,
      },
      /// Rows exist for the move, but none of them is this actor's.
      ActorNotAllowed {
          /// Who asked.
          actor: TransitionActor,
          /// Who may ask, in table order.
          allowed: Vec<TransitionActor>,
      },
      /// The row is the assignee's or the reviewer's, and the agent asking is not the one the
      /// contract names (`docs/SPEC.md` section 5.1).
      NotTheNamedAgent {
          /// Which of the two the row names.
          actor: TransitionActor,
          /// The agent the contract names, if it names one.
          named: Option<String>,
          /// The agent that asked, if the runtime named one.
          asked: Option<String>,
      },
      /// Every row open to this actor has a gate that refuses, and this is what each one said.
      GateFailed {
          /// One entry per row tried, in table order.
          failures: Vec<GateFailure>,
      },
  }

  /// Decides one transition (`docs/SPEC.md` section 5.2): whether the table has a row for it,
  /// whether it is this actor's to ask, whether the agent asking is the one the contract names, and
  /// whether a gate opens. Several rows can carry one move, each with its own gate — three reasons
  /// send a `refining` contract to `escalated` — and the move is allowed when any one of them opens,
  /// taken in table order, so that the more specific reason is the one recorded.
  ///
  /// # Errors
  ///
  /// `NoSuchTransition` when the table has no row, `ActorNotAllowed` when no row is this actor's,
  /// `NotTheNamedAgent` when the row belongs to the assignee or the reviewer and another agent
  /// asked, and `GateFailed` with what every row's gate said when none of them opens.
  pub fn evaluate_transition(
      request: &TransitionRequest,
      context: &TransitionContext,
  ) -> Result<TransitionDecision, TransitionRefusal> {
      let rows = find_transitions(context.status, request.to);
      if rows.is_empty() {
          return Err(TransitionRefusal::NoSuchTransition {
              from: context.status,
              to: request.to,
          });
      }
      let mine: Vec<&'static TransitionRow> = rows
          .iter()
          .copied()
          .filter(|row| row.actor == request.actor)
          .collect();
      if mine.is_empty() {
          return Err(TransitionRefusal::ActorNotAllowed {
              actor: request.actor,
              allowed: allowed_actors(&rows),
          });
      }
      if is_named_by_the_contract(request.actor) {
          let named = named_agent(request.actor, &context.contract);
          let named_id = trimmed(named);
          if named_id.is_none() || named_id != trimmed(request.agent_id.as_deref()) {
              return Err(TransitionRefusal::NotTheNamedAgent {
                  actor: request.actor,
                  named: named.map(str::to_string),
                  asked: request.agent_id.clone(),
              });
          }
      }
      let mut failures = Vec::new();
      for row in mine {
          match check_gate(row.gate, context) {
              Ok(()) => {
                  return Ok(TransitionDecision {
                      from: context.status,
                      to: request.to,
                      row,
                      effects: effects(row.gate, request.to, context),
                  });
              }
              Err(details) => failures.push(GateFailure {
                  gate: row.gate,
                  details,
              }),
          }
      }
      Err(TransitionRefusal::GateFailed { failures })
  }

  /// An id once trimmed, and nobody when it is blank: a stray space must not make an agent somebody
  /// else, and a blank id names no agent at all.
  fn trimmed(id: Option<&str>) -> Option<&str> {
      id.map(str::trim).filter(|id| !id.is_empty())
  }

  /// Who may ask for this move, in table order and without repeats.
  fn allowed_actors(rows: &[&'static TransitionRow]) -> Vec<TransitionActor> {
      let mut allowed: Vec<TransitionActor> = Vec::new();
      for row in rows {
          if !allowed.contains(&row.actor) {
              allowed.push(row.actor);
          }
      }
      allowed
  }

  /// Whether the contract names the agent this row belongs to. The Scrum Master and the Product
  /// Manager hold team roles rather than a place on the contract, and the governor and the human are
  /// not agents, so a row of theirs names nobody here and the runtime's own authentication is what
  /// says who asked.
  fn is_named_by_the_contract(actor: TransitionActor) -> bool {
      match actor {
          TransitionActor::Assignee | TransitionActor::Reviewer => true,
          TransitionActor::ScrumMaster
          | TransitionActor::ProductManager
          | TransitionActor::Governor
          | TransitionActor::Human => false,
      }
  }

  /// The agent the contract names for this actor, exhaustive so that an actor added to the lifecycle
  /// cannot quietly read the assignee's field.
  fn named_agent(actor: TransitionActor, contract: &TaskContract) -> Option<&str> {
      match actor {
          TransitionActor::Assignee => contract.assignee.as_deref(),
          TransitionActor::Reviewer => contract.reviewer.as_deref(),
          TransitionActor::ScrumMaster
          | TransitionActor::ProductManager
          | TransitionActor::Governor
          | TransitionActor::Human => None,
      }
  }

  /// Whether one gate opens, and everything it says when it does not.
  fn check_gate(gate: GateId, context: &TransitionContext) -> GateResult {
      match gate {
          GateId::None => Ok(()),
          GateId::Triaged => open_or(context.triaged, || {
              "the request has no recorded triage decision, and refining starts from one (5.16)"
                  .to_string()
          }),
          GateId::DefinitionOfReady => evaluate_readiness(&context.contract, &context.readiness)
              .map_err(|failures| {
                  failures
                      .iter()
                      .map(|failure| failure.message.clone())
                      .collect()
              }),
          GateId::ReadinessExhausted => open_or(
              evaluate_readiness_attempts(context.readiness_failed_attempts)
                  == ReadinessOutcome::Escalate,
              || {
                  format!(
                      "the contract has failed the Definition of Ready {} times, and it is refined again until {READINESS_ATTEMPT_LIMIT}",
                      context.readiness_failed_attempts
                  )
              },
          ),
          GateId::ContractRequiresHuman => human_must_accept_the_contract(context),
          GateId::Assignment => match &context.assignment {
              Some(assignment) => check_assignment(&context.contract, assignment),
              None => Err(vec![
                  "the runtime named no pair of agents for this assignment".to_string(),
              ]),
          },
          GateId::CriteriaRecorded => {
              if context.contract.kind == Kind::Epic {
                  check_children_done(&context.children)
              } else {
                  check_criteria_recorded(&context.contract, &context.assignee_results, &context.work)
              }
          }
          GateId::BlockerWritten => check_blocker_written(context.blocker.as_ref()),
          GateId::BlockerResolved => check_blocker_resolved(context.blocker_resolution.as_deref()),
          GateId::BlockedAge => blocked_long_enough(context),
          GateId::DefinitionOfDone => {
              evaluate_done(&context.contract, &context.done).map_err(|failures| {
                  failures
                      .iter()
                      .map(|failure| failure.message.clone())
                      .collect()
              })
          }
          GateId::RejectionReasons => {
              check_rejection_reasons(&context.contract, context.rejection.as_ref())
          }
          GateId::IterationBelowLimit => open_or(
              rejection_outcome(&context.contract) == RejectionOutcome::ReturnToInProgress,
              || {
                  format!(
                      "the task has been rejected {} times and the limit is {}, so it escalates rather than being worked again",
                      iteration(&context.contract),
                      max_iterations(&context.contract)
                  )
              },
          ),
          GateId::IterationLimitReached => open_or(
              rejection_outcome(&context.contract) == RejectionOutcome::Escalate,
              || {
                  format!(
                      "the task has been rejected {} times and the limit is {}, so it is worked again rather than escalated",
                      iteration(&context.contract),
                      max_iterations(&context.contract)
                  )
              },
          ),
          GateId::GovernorEscalation => {
              open_or(governor_escalation_reason(context).is_some(), || {
                  "no budget is exhausted and no permission was denied, so the governor has nothing to escalate"
                      .to_string()
              })
          }
      }
  }

  /// `Ok` when the gate opens, and the one reason it gives when it does not, built only then.
  fn open_or(open: bool, shut: impl FnOnce() -> String) -> GateResult {
      if open { Ok(()) } else { Err(vec![shut()]) }
  }

  /// The `ContractRequiresHuman` gate of `refining -> escalated`: the contract needs the human's
  /// acceptance and has not had it. The risk and the kind answer half of it
  /// (`done::requires_human_acceptance`) and the team's policy the other half, which the runtime
  /// passes in because `TeamRules` does not yet carry `human_accepts_contracts`; either is enough,
  /// so the two cannot disagree in the direction that would carry a task past the human.
  fn human_must_accept_the_contract(context: &TransitionContext) -> GateResult {
      if !(context.acceptance.required_by_policy || requires_human_acceptance(&context.contract)) {
          return Err(vec![
              "this contract does not need the human's acceptance: the risk is not high, it is not an epic, and the team's policy does not ask for it"
                  .to_string(),
          ]);
      }
      if context.acceptance.given {
          return Err(vec![
              "the human has already accepted this contract, so it goes to ready rather than escalating"
                  .to_string(),
          ]);
      }
      Ok(())
  }

  /// The `BlockedAge` gate of `blocked -> escalated`. A task the runtime stamped no block time for
  /// cannot be aged, and saying so is better than escalating on a value that is not there.
  fn blocked_long_enough(context: &TransitionContext) -> GateResult {
      let Some(blocked_at) = context.blocked_at else {
          return Err(vec![
              "the runtime recorded no time for this block, and a blocked task escalates on its age"
                  .to_string(),
          ]);
      };
      open_or(
          evaluate_blocked_age(blocked_at, context.now, context.blocked_limit)
              == BlockedAge::Exceeded,
          || {
              format!(
                  "the task has not been blocked for {} seconds yet",
                  context.blocked_limit.as_secs()
              )
          },
      )
  }

  /// The contract's `iteration`, saturated into the width step 06 counts in. A count above
  /// `u32::MAX` is a corrupt figure, and saturating sends the task to the human at the next
  /// rejection rather than letting it be worked for ever.
  fn iteration(contract: &TaskContract) -> u32 {
      u32::try_from(contract.iteration).unwrap_or(u32::MAX)
  }

  /// The contract's `max_iterations`, saturated the same way. A limit above `u32::MAX` is what the
  /// contract asked for: effectively none.
  fn max_iterations(contract: &TaskContract) -> u32 {
      u32::try_from(contract.budget.max_iterations.get()).unwrap_or(u32::MAX)
  }

  fn rejection_outcome(contract: &TaskContract) -> RejectionOutcome {
      evaluate_rejection(iteration(contract), max_iterations(contract))
  }

  /// Why the governor's own `any -> escalated` row is open, or `None` when it is not. The task's
  /// sessions are their own reason (5.7); every other exhausted budget is `budget`. A user's `stop`
  /// reaches the table as the human's own row instead, which needs no gate.
  fn governor_escalation_reason(context: &TransitionContext) -> Option<EscalationReason> {
      if let Some(first) = check_budgets(&context.budget).first() {
          return Some(if first.scope == BudgetScope::TaskSessions {
              EscalationReason::Sessions
          } else {
              EscalationReason::Budget
          });
      }
      if context.permission_denied {
          return Some(EscalationReason::Permission);
      }
      None
  }

  /// What the runtime records with the move: the rejection that has just happened counts, a move to
  /// `escalated` carries the reason the gate that opened it names, and a task leaving `blocked` for
  /// work leaves its blocker behind.
  fn effects(gate: GateId, to: TaskStatus, context: &TransitionContext) -> Vec<TransitionEffect> {
      let mut effects = Vec::new();
      if to == TaskStatus::Rejected {
          effects.push(TransitionEffect::IncrementIteration);
      }
      if to == TaskStatus::Escalated {
          effects.push(TransitionEffect::RaiseEscalation(escalation_reason(
              gate, context,
          )));
      }
      if context.status == TaskStatus::Blocked && to == TaskStatus::InProgress {
          effects.push(TransitionEffect::ResetBlocker);
      }
      effects
  }

  /// The reason an escalation carries, from the gate that opened the row. Exhaustive on purpose: a
  /// gate added to the table cannot reach `escalated` without a reason being chosen for it here. The
  /// five gates that lead there each name their own; a gate-free row into `escalated` is the human's
  /// (5.2), so it is the user asking. No row pairs any other gate with `escalated`, which a test
  /// over the table pins.
  fn escalation_reason(gate: GateId, context: &TransitionContext) -> EscalationReason {
      match gate {
          GateId::ReadinessExhausted => EscalationReason::ReadinessFailures,
          GateId::ContractRequiresHuman => {
              if context.contract.kind == Kind::Epic {
                  EscalationReason::Approval
              } else {
                  EscalationReason::RiskGate
              }
          }
          GateId::BlockedAge => EscalationReason::BlockerAge,
          GateId::IterationLimitReached => EscalationReason::Iterations,
          GateId::GovernorEscalation => {
              governor_escalation_reason(context).unwrap_or(EscalationReason::Budget)
          }
          GateId::None
          | GateId::Triaged
          | GateId::DefinitionOfReady
          | GateId::Assignment
          | GateId::CriteriaRecorded
          | GateId::BlockerWritten
          | GateId::BlockerResolved
          | GateId::DefinitionOfDone
          | GateId::RejectionReasons
          | GateId::IterationBelowLimit => EscalationReason::ExplicitRequest,
      }
  }

  #[cfg(test)]
  mod tests {
      use std::time::Duration;

      use chrono::{DateTime, TimeZone, Utc};

      use super::{
          ContractAcceptance, GateFailure, TransitionContext, TransitionDecision, TransitionEffect,
          TransitionRefusal, TransitionRequest, evaluate_transition,
      };
      use crate::budget::{
          BudgetState, DEFAULT_DAY_BUDGET_USD, DEFAULT_SESSION_LIMITS, DEFAULT_SPRINT_BUDGET_USD,
          SessionLedger,
      };
      use crate::contract::{Role, TaskStatus};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;
      use crate::generated::task_contract::FarikTaskContractRisk as Risk;
      use crate::governor::done::{CriterionResult, DoneEvidence, RunBy};
      use crate::governor::escalation::{
          DEFAULT_BLOCKED_LIMIT, EscalationReason as Why, RejectionOutcome,
      };
      use crate::governor::gates::{
          AssignmentInput, AssignmentRequester, Blocker, ChildState, Rejection, WorkState,
      };
      use crate::governor::readiness::fixtures::{a_contract, a_ready_context};
      use crate::governor::transition_table::{
          GateId, Status, TRANSITION_TABLE, TransitionActor as A,
      };

      fn at(hour: u32) -> DateTime<Utc> {
          Utc.with_ymd_and_hms(2026, 9, 16, hour, 0, 0)
              .single()
              .expect("a real hour")
      }

      fn an_assignment() -> AssignmentInput {
          AssignmentInput {
              requested_by: AssignmentRequester::ScrumMaster,
              has_active_scrum_master: true,
              assignee_id: "dev-1".to_string(),
              assignee_role: Role::SoftwareDeveloper,
              reviewer_id: "arch-1".to_string(),
              reviewer_role: Role::Architect,
              assignee_open_tasks: 0,
              wip_limit: 2,
              remaining_sprint_budget_usd: 15.0,
              dependencies: Vec::new(),
          }
      }

      fn a_result(run_by: RunBy) -> CriterionResult {
          CriterionResult {
              criterion_id: "C1".to_string(),
              passed: true,
              evidence: "cargo test: 11 passed".to_string(),
              run_by,
          }
      }

      fn a_budget() -> BudgetState {
          BudgetState {
              session: SessionLedger::default(),
              session_limits: DEFAULT_SESSION_LIMITS,
              task_spent_usd: 1.0,
              task_max_usd: 5.0,
              task_sessions: 1,
              task_max_sessions: 5,
              sprint_spent_usd: 3.0,
              sprint_max_usd: DEFAULT_SPRINT_BUDGET_USD,
              day_spent_usd: 4.0,
              day_max_usd: DEFAULT_DAY_BUDGET_USD,
          }
      }

      /// A task in `in_progress`, assigned to `dev-1` and reviewed by `arch-1`, with everything a
      /// gate could ask for in the state that lets it pass, so that a test sets only what it is
      /// about.
      fn a_context() -> TransitionContext {
          let mut contract = a_contract();
          contract.assignee = Some("dev-1".to_string());
          contract.reviewer = Some("arch-1".to_string());
          TransitionContext {
              status: TaskStatus::InProgress,
              contract,
              triaged: true,
              children: Vec::new(),
              readiness: a_ready_context(),
              readiness_failed_attempts: 1,
              acceptance: ContractAcceptance::default(),
              assignment: Some(an_assignment()),
              assignee_results: vec![a_result(RunBy::Assignee)],
              work: WorkState {
                  commits: 1,
                  worktree_clean: true,
              },
              blocker: Some(Blocker {
                  description: "The staging database refuses the migration.".to_string(),
                  needed: "A password for the staging database.".to_string(),
              }),
              blocker_resolution: Some("The password is in the vault.".to_string()),
              blocked_at: Some(at(1)),
              now: at(2),
              blocked_limit: DEFAULT_BLOCKED_LIMIT,
              done: DoneEvidence {
                  results: vec![a_result(RunBy::Reviewer)],
                  changed_paths: vec!["src/login/form.rs".to_string()],
                  completion_note: Some("The form takes an email and a password.".to_string()),
                  review_note: Some("C1: cargo test, 11 passed.".to_string()),
                  human_accepted: false,
              },
              rejection: Some(Rejection {
                  failed_criterion_ids: vec!["C1".to_string()],
                  reasons: "The form accepts an empty password.".to_string(),
              }),
              budget: a_budget(),
              permission_denied: false,
          }
      }

      fn ask(to: TaskStatus, actor: A, agent_id: Option<&str>) -> TransitionRequest {
          TransitionRequest {
              task_id: "FRK-1".parse().expect("a task id"),
              to,
              actor,
              agent_id: agent_id.map(str::to_string),
          }
      }

      fn decide(
          request: &TransitionRequest,
          context: &TransitionContext,
      ) -> Result<TransitionDecision, TransitionRefusal> {
          evaluate_transition(request, context)
      }

      fn effects(request: &TransitionRequest, context: &TransitionContext) -> Vec<TransitionEffect> {
          decide(request, context)
              .expect("expected the move to be allowed")
              .effects
      }

      fn gates(request: &TransitionRequest, context: &TransitionContext) -> Vec<GateFailure> {
          match decide(request, context) {
              Err(TransitionRefusal::GateFailed { failures }) => failures,
              other => panic!("expected a gate refusal, got {other:?}"),
          }
      }

      fn one_gate(request: &TransitionRequest, context: &TransitionContext) -> (GateId, Vec<String>) {
          let mut failures = gates(request, context);
          assert_eq!(failures.len(), 1, "expected one gate to be tried");
          let failure = failures.remove(0);
          (failure.gate, failure.details)
      }

      #[test]
      fn refuses_a_move_the_table_has_no_row_for() {
          let mut context = a_context();
          assert_eq!(
              decide(
                  &ask(TaskStatus::Accepted, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::InProgress,
                  to: TaskStatus::Accepted
              })
          );
          // Nothing leaves a terminal status, and no row means staying put.
          context.status = TaskStatus::Accepted;
          assert_eq!(
              decide(&ask(TaskStatus::InProgress, A::Human, None), &context),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::Accepted,
                  to: TaskStatus::InProgress
              })
          );
          context.status = TaskStatus::InProgress;
          assert_eq!(
              decide(
                  &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::NoSuchTransition {
                  from: TaskStatus::InProgress,
                  to: TaskStatus::InProgress
              })
          );
      }

      #[test]
      fn refuses_an_actor_no_row_names_and_says_who_may_ask() {
          let mut context = a_context();
          context.status = TaskStatus::Ready;
          assert_eq!(
              decide(
                  &ask(TaskStatus::Assigned, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::ActorNotAllowed {
                  actor: A::Assignee,
                  allowed: vec![A::ScrumMaster, A::ProductManager]
              })
          );
      }

      #[test]
      fn opens_the_assignees_row_only_to_the_agent_the_contract_names() {
          let mut context = a_context();
          assert_eq!(
              decide(
                  &ask(TaskStatus::Verifying, A::Assignee, Some("dev-2")),
                  &context
              ),
              Err(TransitionRefusal::NotTheNamedAgent {
                  actor: A::Assignee,
                  named: Some("dev-1".to_string()),
                  asked: Some("dev-2".to_string())
              })
          );
          // A stray space is the same agent.
          assert!(
              decide(
                  &ask(TaskStatus::Verifying, A::Assignee, Some(" dev-1 ")),
                  &context
              )
              .is_ok()
          );
          // An unassigned task has no assignee to ask for it, however the request is spelled.
          context.contract.assignee = None;
          for asked in [None, Some("dev-1"), Some("  ")] {
              assert_eq!(
                  decide(&ask(TaskStatus::Verifying, A::Assignee, asked), &context),
                  Err(TransitionRefusal::NotTheNamedAgent {
                      actor: A::Assignee,
                      named: None,
                      asked: asked.map(str::to_string)
                  }),
                  "{asked:?}"
              );
          }
      }

      #[test]
      fn opens_the_reviewers_row_only_to_the_reviewer_the_contract_names() {
          let context = a_context();
          let mut verifying = a_context();
          verifying.status = TaskStatus::Verifying;
          assert_eq!(
              decide(
                  &ask(TaskStatus::Rejected, A::Reviewer, Some("dev-1")),
                  &verifying
              ),
              Err(TransitionRefusal::NotTheNamedAgent {
                  actor: A::Reviewer,
                  named: Some("arch-1".to_string()),
                  asked: Some("dev-1".to_string())
              })
          );
          assert!(
              decide(
                  &ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1")),
                  &verifying
              )
              .is_ok()
          );
          // The Scrum Master, the Product Manager, the governor and the human hold no place on the
          // contract, so their rows ask nothing about an agent id.
          let mut blocked = context;
          blocked.status = TaskStatus::Blocked;
          assert!(decide(&ask(TaskStatus::InProgress, A::ScrumMaster, None), &blocked).is_ok());
      }

      #[test]
      fn refines_a_draft_request_only_once_it_is_triaged() {
          let mut context = a_context();
          context.status = TaskStatus::Draft;
          let request = ask(TaskStatus::Refining, A::ProductManager, Some("pm-1"));
          assert_eq!(effects(&request, &context), []);
          context.triaged = false;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Triaged,
                  vec![
                      "the request has no recorded triage decision, and refining starts from one (5.16)"
                          .to_string()
                  ]
              )
          );
      }

      #[test]
      fn readies_a_contract_that_passes_the_definition_of_ready() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          let request = ask(TaskStatus::Ready, A::Governor, None);
          assert_eq!(effects(&request, &context), []);
          // The gate's details are the Definition of Ready's own messages, in its own order.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::DefinitionOfReady);
          assert_eq!(
              details,
              vec!["the intent is blank; state the user-facing reason for the task".to_string()]
          );
      }

      #[test]
      fn escalates_a_contract_that_has_failed_the_definition_of_ready_three_times() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
          );
          // Below the limit, all three of the governor's rows into `escalated` are tried, in table
          // order, and each says what it is waiting for.
          context.readiness_failed_attempts = 2;
          let failures = gates(&request, &context);
          assert_eq!(
              failures
                  .iter()
                  .map(|failure| failure.gate)
                  .collect::<Vec<GateId>>(),
              [
                  GateId::ReadinessExhausted,
                  GateId::ContractRequiresHuman,
                  GateId::GovernorEscalation
              ]
          );
          assert_eq!(
              failures[0].details,
              vec![
                  "the contract has failed the Definition of Ready 2 times, and it is refined again until 3"
                      .to_string()
              ]
          );
      }

      #[test]
      fn escalates_a_contract_that_waits_for_the_human_and_names_why() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 0;
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          // An epic waits for the user's approval (5.16 item 2); a high-risk task waits on the risk
          // gate (5.4 item 5). The reason the escalation carries says which.
          context.contract.kind = Kind::Epic;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
          context.contract.kind = Kind::Task;
          context.contract.risk = Risk::High;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::RiskGate)]
          );
          // The team's policy is the other half of the question, and either half is enough.
          context.contract.risk = Risk::Medium;
          context.acceptance.required_by_policy = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::RiskGate)]
          );
          // A contract the human has already accepted goes to `ready`, not to the human again.
          context.acceptance.given = true;
          let details = gates(&request, &context)[1].details.clone();
          assert_eq!(
              details,
              vec![
                  "the human has already accepted this contract, so it goes to ready rather than escalating"
                      .to_string()
              ]
          );
          // And one that needs nobody's acceptance says that instead.
          context.acceptance = ContractAcceptance::default();
          let details = gates(&request, &context)[1].details.clone();
          assert_eq!(
              details,
              vec![
                  "this contract does not need the human's acceptance: the risk is not high, it is not an epic, and the team's policy does not ask for it"
                      .to_string()
              ]
          );
      }

      #[test]
      fn assigns_a_task_through_the_assignment_gate() {
          let mut context = a_context();
          context.status = TaskStatus::Ready;
          let request = ask(TaskStatus::Assigned, A::ScrumMaster, Some("sm-1"));
          assert_eq!(effects(&request, &context), []);
          // The gate's details are the assignment gate's own, and a runtime that named no pair of
          // agents is told so rather than refused for a rule it could not have met.
          context.assignment = None;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Assignment,
                  vec!["the runtime named no pair of agents for this assignment".to_string()]
              )
          );
      }

      #[test]
      fn verifies_a_task_on_its_own_runs_and_an_epic_on_its_tasks() {
          let mut context = a_context();
          let request = ask(TaskStatus::Verifying, A::Assignee, Some("dev-1"));
          assert_eq!(effects(&request, &context), []);
          context.assignee_results = Vec::new();
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::CriteriaRecorded,
                  vec!["the assignee recorded no run with evidence for criterion C1".to_string()]
              )
          );
          // An epic is verified on its tasks instead, and nothing asks its assignee for a run.
          context.contract.kind = Kind::Epic;
          context.children = vec![ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::Accepted,
          }];
          assert_eq!(effects(&request, &context), []);
          context.children = vec![ChildState {
              task_id: "FRK-2".to_string(),
              status: TaskStatus::InProgress,
          }];
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::CriteriaRecorded);
          assert_eq!(details.len(), 2);
      }

      #[test]
      fn blocks_a_task_with_a_written_blocker_and_clears_it_with_a_resolution() {
          let mut context = a_context();
          let block = ask(TaskStatus::Blocked, A::Assignee, Some("dev-1"));
          assert_eq!(effects(&block, &context), []);
          context.blocker = None;
          assert_eq!(one_gate(&block, &context).0, GateId::BlockerWritten);
          // Leaving `blocked` for work clears the blocker, whoever asked.
          let mut blocked = a_context();
          blocked.status = TaskStatus::Blocked;
          for actor in [A::ScrumMaster, A::Human] {
              assert_eq!(
                  effects(&ask(TaskStatus::InProgress, actor, None), &blocked),
                  [TransitionEffect::ResetBlocker],
                  "{actor:?}"
              );
          }
          blocked.blocker_resolution = None;
          assert_eq!(
              one_gate(&ask(TaskStatus::InProgress, A::Human, None), &blocked).0,
              GateId::BlockerResolved
          );
      }

      #[test]
      fn escalates_a_task_blocked_for_the_limit_or_longer() {
          let mut context = a_context();
          context.status = TaskStatus::Blocked;
          context.blocked_limit = Duration::from_secs(3600);
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::BlockerAge)]
          );
          // Within the limit, the age row says so; the governor's own row is tried after it.
          context.blocked_limit = Duration::from_secs(7200);
          context.now = at(2);
          let failures = gates(&request, &context);
          assert_eq!(
              failures
                  .iter()
                  .map(|failure| failure.gate)
                  .collect::<Vec<GateId>>(),
              [GateId::BlockedAge, GateId::GovernorEscalation]
          );
          assert_eq!(
              failures[0].details,
              vec!["the task has not been blocked for 7200 seconds yet".to_string()]
          );
          // A block the runtime stamped no time for cannot be aged.
          context.blocked_at = None;
          assert_eq!(
              gates(&request, &context)[0].details,
              vec![
                  "the runtime recorded no time for this block, and a blocked task escalates on its age"
                      .to_string()
              ]
          );
          // With the age row shut and a budget exhausted, the governor's own row carries the move,
          // and the reason follows the row that opened rather than the one that did not.
          context.budget.day_spent_usd = context.budget.day_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
      }

      #[test]
      fn accepts_a_task_on_the_definition_of_done() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Accepted, A::ProductManager, Some("pm-1"));
          assert_eq!(effects(&request, &context), []);
          context.done.review_note = None;
          let (gate, details) = one_gate(&request, &context);
          assert_eq!(gate, GateId::DefinitionOfDone);
          assert_eq!(
              details,
              vec![
                  "the reviewer wrote no review note mapping each criterion to its evidence"
                      .to_string()
              ]
          );
      }

      #[test]
      fn rejects_work_with_written_reasons_and_counts_the_iteration() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1"));
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::IncrementIteration]
          );
          context.rejection = None;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::RejectionReasons,
                  vec![
                      "work is rejected with written reasons mapped to the criteria that failed"
                          .to_string()
                  ]
              )
          );
      }

      #[test]
      fn works_a_rejected_task_again_until_the_iteration_limit() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          let again = ask(TaskStatus::InProgress, A::Governor, None);
          let escalate = ask(TaskStatus::Escalated, A::Governor, None);
          // The contract's default limit is three; the iteration counts the rejections so far.
          context.contract.iteration = 2;
          assert_eq!(effects(&again, &context), []);
          assert_eq!(
              gates(&escalate, &context)[0].details,
              vec![
                  "the task has been rejected 2 times and the limit is 3, so it is worked again rather than escalated"
                      .to_string()
              ]
          );
          context.contract.iteration = 3;
          assert_eq!(
              effects(&escalate, &context),
              [TransitionEffect::RaiseEscalation(Why::Iterations)]
          );
          assert_eq!(
              one_gate(&again, &context),
              (
                  GateId::IterationBelowLimit,
                  vec![
                      "the task has been rejected 3 times and the limit is 3, so it escalates rather than being worked again"
                          .to_string()
                  ]
              )
          );
          // A count the schema's `u64` can hold but `u32` cannot is a corrupt figure, and the safe
          // reading sends the task to the human rather than working it for ever.
          context.contract.iteration = u64::from(u32::MAX) + 1;
          assert_eq!(
              effects(&escalate, &context),
              [TransitionEffect::RaiseEscalation(Why::Iterations)]
          );
      }

      #[test]
      fn escalates_on_an_exhausted_budget_or_a_denied_permission() {
          let mut context = a_context();
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::GovernorEscalation,
                  vec![
                      "no budget is exhausted and no permission was denied, so the governor has nothing to escalate"
                          .to_string()
                  ]
              )
          );
          // The task's sessions are their own reason (5.7); every other budget is `budget`.
          context.budget.task_sessions = context.budget.task_max_sessions;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Sessions)]
          );
          context.budget = a_budget();
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          context.budget = a_budget();
          context.permission_denied = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Permission)]
          );
      }

      #[test]
      fn lets_the_human_escalate_or_cancel_anything_and_move_an_escalated_task() {
          let mut context = a_context();
          assert_eq!(
              effects(&ask(TaskStatus::Escalated, A::Human, None), &context),
              [TransitionEffect::RaiseEscalation(Why::ExplicitRequest)]
          );
          assert_eq!(
              effects(&ask(TaskStatus::Cancelled, A::Human, None), &context),
              []
          );
          // From `escalated` the human may put the task anywhere the lifecycle has room for.
          context.status = TaskStatus::Escalated;
          for to in [
              TaskStatus::Refining,
              TaskStatus::Ready,
              TaskStatus::InProgress,
              TaskStatus::Accepted,
          ] {
              assert!(decide(&ask(to, A::Human, None), &context).is_ok(), "{to}");
          }
          // An agent cannot: the row is the human's.
          assert_eq!(
              decide(
                  &ask(TaskStatus::Ready, A::Assignee, Some("dev-1")),
                  &context
              ),
              Err(TransitionRefusal::ActorNotAllowed {
                  actor: A::Assignee,
                  allowed: vec![A::Human]
              })
          );
      }

      /// One row of the table, the move that takes it, and what about the state opens that row
      /// rather than another.
      struct Case {
          row: usize,
          from: TaskStatus,
          to: TaskStatus,
          actor: A,
          prepare: fn(&mut TransitionContext),
      }

      const fn case(
          row: usize,
          from: TaskStatus,
          to: TaskStatus,
          actor: A,
          prepare: fn(&mut TransitionContext),
      ) -> Case {
          Case {
              row,
              from,
              to,
              actor,
              prepare,
          }
      }

      /// One case per row of `TRANSITION_TABLE`, in its order.
      fn every_row() -> [Case; 20] {
          use TaskStatus as S;
          let no_change: fn(&mut TransitionContext) = |_| {};
          [
              case(0, S::Draft, S::Refining, A::ProductManager, no_change),
              case(1, S::Refining, S::Ready, A::Governor, no_change),
              case(2, S::Refining, S::Escalated, A::Governor, |context| {
                  context.readiness_failed_attempts = 3;
              }),
              case(3, S::Refining, S::Escalated, A::Governor, |context| {
                  context.readiness_failed_attempts = 0;
                  context.contract.risk = Risk::High;
              }),
              case(4, S::Ready, S::Assigned, A::ScrumMaster, no_change),
              case(5, S::Ready, S::Assigned, A::ProductManager, |context| {
                  let assignment = context.assignment.as_mut().expect("the fixture assigns");
                  assignment.requested_by = AssignmentRequester::ProductManager;
                  assignment.has_active_scrum_master = false;
              }),
              case(6, S::Assigned, S::InProgress, A::Assignee, no_change),
              case(7, S::InProgress, S::Verifying, A::Assignee, no_change),
              case(8, S::InProgress, S::Blocked, A::Assignee, no_change),
              case(9, S::Blocked, S::InProgress, A::ScrumMaster, no_change),
              case(10, S::Blocked, S::InProgress, A::Human, no_change),
              case(11, S::Blocked, S::Escalated, A::Governor, |context| {
                  context.blocked_limit = Duration::from_secs(3600);
              }),
              case(12, S::Verifying, S::Accepted, A::ProductManager, no_change),
              case(13, S::Verifying, S::Rejected, A::Reviewer, no_change),
              case(14, S::Rejected, S::InProgress, A::Governor, no_change),
              case(15, S::Rejected, S::Escalated, A::Governor, |context| {
                  context.contract.iteration = 3;
              }),
              case(16, S::InProgress, S::Escalated, A::Governor, |context| {
                  context.permission_denied = true;
              }),
              case(17, S::InProgress, S::Escalated, A::Human, no_change),
              case(18, S::InProgress, S::Cancelled, A::Human, no_change),
              case(19, S::Escalated, S::Ready, A::Human, no_change),
          ]
      }

      #[test]
      fn opens_every_row_of_the_table_with_the_state_that_belongs_to_it() {
          // Spec F5 asks for a test for every transition. `every_row` names, for each row, the move
          // that takes it and the one thing about the state that opens that row rather than another;
          // the last assertion is that the cases cover the table, so a row added to 5.2 fails here
          // until somebody says what opens it.
          let mut covered: Vec<usize> = Vec::new();
          for case in every_row() {
              let mut context = a_context();
              context.status = case.from;
              (case.prepare)(&mut context);
              let agent_id = match case.actor {
                  A::Assignee => Some("dev-1"),
                  A::Reviewer => Some("arch-1"),
                  A::ScrumMaster | A::ProductManager | A::Governor | A::Human => None,
              };
              let decision = decide(&ask(case.to, case.actor, agent_id), &context)
                  .unwrap_or_else(|refusal| panic!("row {} was shut: {refusal:?}", case.row));
              assert_eq!(
                  *decision.row, TRANSITION_TABLE[case.row],
                  "row {}: took {:?}",
                  case.row, decision.row
              );
              covered.push(case.row);
          }
          covered.sort_unstable();
          assert_eq!(
              covered,
              (0..TRANSITION_TABLE.len()).collect::<Vec<usize>>(),
              "every row of the table has a case"
          );
      }

      #[test]
      fn names_a_reason_for_every_row_of_the_table_that_escalates() {
          // The reason an escalation carries comes from the gate that opened the row, and these are
          // the only gates the table pairs with `escalated`: a row added with another gate would
          // record the user as the reason, so this test is what says none does.
          let escalating: Vec<GateId> = TRANSITION_TABLE
              .iter()
              .filter(|row| row.to == Status::Is(TaskStatus::Escalated))
              .map(|row| row.gate)
              .collect();
          assert_eq!(
              escalating,
              [
                  GateId::ReadinessExhausted,
                  GateId::ContractRequiresHuman,
                  GateId::BlockedAge,
                  GateId::IterationLimitReached,
                  GateId::GovernorEscalation,
                  GateId::None
              ]
          );
      }

      #[test]
      fn takes_the_first_row_whose_gate_opens() {
          // A `refining` contract that has failed the Definition of Ready three times and has a
          // budget exhausted could escalate for either reason. The table's order decides, so that
          // the more specific reason is the one the user reads.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          context.budget.day_spent_usd = context.budget.day_max_usd;
          let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
              .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::ReadinessExhausted);
          assert_eq!(
              decision.effects,
              [TransitionEffect::RaiseEscalation(Why::ReadinessFailures)]
          );
          assert_eq!(decision.from, TaskStatus::Refining);
          assert_eq!(decision.to, TaskStatus::Escalated);
      }

      #[test]
      fn starts_a_session_on_an_assigned_task_with_no_gate_at_all() {
          let mut context = a_context();
          context.status = TaskStatus::Assigned;
          let decision = decide(
              &ask(TaskStatus::InProgress, A::Assignee, Some("dev-1")),
              &context,
          )
          .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::None);
          assert_eq!(decision.effects, []);
          // And the rejection outcome of step 06 is what the iteration gates read, not a count of
          // their own: at the default limit a fresh task is worked rather than escalated.
          assert_eq!(
              crate::governor::escalation::evaluate_rejection(0, 3),
              RejectionOutcome::ReturnToInProgress
          );
      }
  }
  ```

- [ ] Stop the spec claiming the user's stop for the governor. In `docs/SPEC.md` section 5.2's transition table, replace

  ```
  | any | escalated | Governor | budget exhausted, permission denied on a required action, or a `stop` from the user |
  ```

  with

  ```
  | any | escalated | Governor | budget exhausted, or permission denied on a required action; a `stop` from the user takes the human's row below, which needs no gate, because the governor is not what hears the user (added in 0.3) |
  ```

- [ ] Say in the spec how the table is read when more than one row carries a move. In `docs/SPEC.md` section 5.2, replace

  ```
  The governor judges an assignment on what the runtime tells it,
  ```

  with

  ```
  More than one row can carry one move: three send a `refining` contract to `escalated`, and a blocked or rejected task has its own row and the governor's. The rows are taken in the order they appear here and the first whose gate opens is the one recorded, so the more specific reason is the one the user reads; when none opens, the refusal says what every gate it tried was waiting for. A row whose actor is the assignee or the reviewer is open only to the agent the contract names in that field, so one Developer cannot declare another's task done (5.1). A move to `escalated` raises an escalation whose reason is the row's: the readiness failures, an epic's approval or a task's risk gate, the blocker's age, the iteration limit, the exhausted budget (the task's sessions being their own reason), the denied permission, or the user's own request on the human's row (5.7). A rejection counts: `iteration` goes up by one as the task enters `rejected`, which is what the limit on `rejected -> in_progress` is measured against, and a task that leaves `blocked` for `in_progress` leaves its blocker and its block time behind, while one that escalates out of `blocked` keeps both, because that is what the user is shown (added in 0.3).

  The governor judges an assignment on what the runtime tells it,
  ```

- [ ] Record the interface changes in `docs/plans/project-plan.md`, phase 1 step 09. Each before-text is a substring of a longer line and occurs exactly once. Replace

  ```
  contract_requires_human_acceptance: bool, contract_human_accepted: bool
  ```

  with

  ```
  acceptance: ContractAcceptance
  ```

  replace

  ```
  `enum TransitionEffect { IncrementIteration, RaiseEscalation, ResetBlocker }`
  ```

  with

  ```
  `struct ContractAcceptance { required_by_policy: bool, given: bool }` (changed 2026-09-16 by the step 09 plan: the team's policy and the human's answer are two halves of one question, either half of the first is enough, and four bools in the context is one more than clippy's pedantic limit); `enum TransitionEffect { IncrementIteration, RaiseEscalation(EscalationReason), ResetBlocker }` (the reason added by the step 09 plan: it is a fact of the row that opened, and the runtime cannot work it out afterwards)
  ```

  replace

  ```
  its plan decides what a value above `u32::MAX` does
  ```

  with

  ```
  a value above `u32::MAX` saturates, decided 2026-09-16 by the step 09 plan: a corrupt count escalates the task at the next rejection rather than freezing it, and a limit that large is effectively none
  ```

  and replace

  ```
  `enum TransitionRefusal { NoSuchTransition, ActorNotAllowed, GateFailed { gate: GateId, details: Vec<String> } }`
  ```

  with

  ```
  `enum TransitionRefusal { NoSuchTransition { from: TaskStatus, to: TaskStatus }, ActorNotAllowed { actor: TransitionActor, allowed: Vec<TransitionActor> }, NotTheNamedAgent { actor: TransitionActor, named: Option<String>, asked: Option<String> }, GateFailed { failures: Vec<GateFailure> } }` with `struct GateFailure { gate: GateId, details: Vec<String> }` (changed 2026-09-16 by the step 09 plan: a move can be carried by more than one row, so a refusal that named one gate would report the first row's for a move three gates refused; and a row that belongs to the assignee or the reviewer is open only to the agent the contract names, which is the refusal `agent_id` exists for)
  ```

- [ ] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::transition::tests
  # expected, among the output:
  # test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 198 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 219 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide every transition of the lifecycle`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 219 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): decide every transition of the lifecycle
```

## Open questions

none
