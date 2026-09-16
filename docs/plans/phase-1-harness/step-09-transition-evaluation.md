# Phase 1, step 09: Transition evaluation

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.2 (the transition table, its gates, and who triggers each move), section 5.1 (a row that belongs to the assignee or the reviewer is that agent's), section 5.3 (the Definition of Ready), section 5.4 (the Definition of Done), section 5.5 (the budgets the governor escalates on), section 5.7 (the ten escalation reasons), section 5.11 (a contract the human must accept), section 5.12 (`human_accepts_contracts`), section 5.16 (triage, an epic's tasks, an epic's approval), F5 (the governor as a library with a test for every transition)
Depends on: phase 0 (merged in #4); step 01 of this phase for the table, `GateId`, `TransitionActor` and the two lookups (f9f0e67, 0e50df1, 3cc8bc3); step 02 for `evaluate_readiness`, `ReadinessContext` and the fixtures (21fe00a, 87a3561, 4a1ac90); step 05 for `check_budgets` and `BudgetState` (5ad2c21, c4cf30b, f3ecea4, 04ba838); step 06 for the three counting rules and `EscalationReason` (33202e7, abfb7d8); step 07 for `evaluate_done`, `DoneEvidence` and `requires_human_acceptance` (a0cdecd, 090e015, cc27028); step 08 for the nine gate predicates and their values (fa02450, 2594121)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code session that did not write this plan, the third readiness review, 2026-09-16, at `9656279`: READY under all three rules of `docs/standards/workflow.md` stage 2, with two should-fix findings and two optional ones, all taken in this commit. Two earlier passes refused the plan; what each found is in Decisions.

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
- The effects of a move, in this order: `IncrementIteration` when the task returns from `rejected` to `in_progress`, `RaiseEscalation` when it enters `escalated`, and `ResetBlocker` whenever it enters `in_progress`.
- The iteration counts the **return**, not the rejection, because that is what `max_iterations` bounds: the schema's own words are "how many times the task may be rejected and returned to in_progress before it escalates", step 06 decided it and named this step's effect as what counts each return, and `evaluate_rejection`'s doc comment says the same. Counting the rejection instead would give two returns where the default of three says three. Rejected, and rejected for a reason that was wrong: that a task could be rejected without limit while nobody sent it back, which the lifecycle does not allow, because the only way back to `verifying` is through `rejected -> in_progress`.
- `ResetBlocker` fires on every move into `in_progress`, not only out of `blocked`. A task at work carries no blocker, so clearing one costs the runtime nothing; and a task that escalated out of `blocked` kept its blocker — which is what the user is shown — so the human moving it from `escalated` to `in_progress` must clear it, or a stale blocker would age again and escalate a task nobody is blocked on.
- `TransitionContext` carries the values rather than a trait the runtime implements, because `farik-core` does no I/O and a trait would let the world in through the back door. It is not `PartialEq`: the generated `TaskContract` is not, and a context is a bundle of inputs rather than a value to compare.
- A contract waiting for the human does not leave `refining` for `ready`. Spec 5.16 item 2 gives every epic the human's acceptance "before it leaves `refining`, whatever its risk", and 5.2 says the same for a high risk and for the team's policy; this is the only function the runtime asks, so a rule it does not hold is not held, and an epic would otherwise reach `ready` and then `assigned` with no approval ever asked for. The `DefinitionOfReady` gate therefore refuses first while the contract waits, and the Definition of Ready is the whole gate once the human has answered.
- The `ReadinessExhausted` gate asks whether the contract fails the Definition of Ready **now**, as well as how often it has failed. Without that the two `refining -> escalated` rows overlap, and the counter's row comes first: an epic that failed three times, was sent back by the human with a message, and has since been fixed would escalate with reason `readiness_failures` where 5.16 item 2 says `approval`. The board would show it as having failed readiness, the human would resolve it by moving it rather than approving it, and the approval `check_product_doc_write` reads would never be recorded. Asking the contract makes the two rows exclusive, so the answer no longer depends on table order or on when the runtime resets its counter — which is also why the counter may be a running total, and its doc comment says so. A contract nobody has to accept, that now passes, is not escalated at all: it is ready.
- The `ContractRequiresHuman` gate also asks whether the contract passes the Definition of Ready, because 5.16 item 2 asks the human "once the contract passes the structural checks". It asks that **before** it asks whether the human has already answered, or a contract the human accepted that does not pass would be refused with "so it goes to ready" while the readying gate refuses that very move — and the flow 5.16 names, a user writing an epic themselves and locking and approving it (5.11), reaches exactly that state. Without it an epic goes to the user on its first readiness failure, losing the three refining attempts 5.2 gives it, and the human's gate-free `escalated -> any` row then carries a contract that never passed the Definition of Ready into `ready`, which 5.1's "contracts before work" forbids. The whole Definition of Ready is the reading, because that is the function step 02 provides; the details name the rule and then the readiness failures themselves. Together the fork is: below three failures nothing opens and the Product Manager refines again; at three, the readiness row; ready and waiting, the approval row; ready and accepted, `ready`.
- The governor's own `any -> escalated` row opens only on a budget whose consequence is escalation. Spec 5.5 gives every budget its own consequence and only the task's dollars and the task's sessions say "task to `escalated`": the session's budgets end the session and block the task, the sprint's stops new assignments, and the day's pauses the team. Escalating one task because the team's day is spent would be the wrong answer to the wrong question, and step 05 already attaches the consequence to every exhausted scope. A task out of both dollars and sessions reads as the dollars, which come first in `BudgetScope` and are the harder limit.
- Who asked for an assignment comes from the request's actor, and the `requested_by` of the input is overwritten with it. The two are the same fact spelled twice, and a runtime that filled the input with `scrum_master` while asking as the Product Manager would open the Product Manager's row in a team that has an active Scrum Master. Only those two rows carry an assignment (5.2), so no other actor reaches the conversion. Added by this plan after its readiness review.
- The two halves of "is this contract waiting for the human?" travel together as `ContractAcceptance { required_by_policy, given }`, and the gate passes when the team's policy requires it **or** the contract's own risk or kind does (`done::requires_human_acceptance`), and the human has not answered. Either half is enough, so the two cannot disagree in the direction that would carry a task past the human. `TeamRules` does not yet carry `human_accepts_contracts` (5.12), which is why the policy half is passed in. Changed from the project plan's two bools, which were also one bool too many for clippy's pedantic limit on a struct.
- `iteration` and `max_iterations` are converted with `u32::try_from` and saturate at `u32::MAX`. A count the schema's `u64` holds and `u32` cannot is a corrupt figure, and saturating sends the task to the human at the next rejection rather than letting it be worked for ever; a limit above `u32::MAX` is what the contract asked for, which is effectively none. Rejected: refusing the move, because a corrupt count would then freeze the task instead of escalating it.
- A gate that needs a value the runtime did not gather refuses and says so: no pair of agents for an assignment, no time for a block. Rejected: passing, which would move a task on a value nobody supplied.
- `CriteriaRecorded` asks `check_children_done` for an epic and `check_criteria_recorded` for a task, from the contract's own `kind`. The table has one row and 5.16 item 4 gives an epic the other question.
- The details of a gate that wraps a longer answer are that answer's own messages, in its own order: the Definition of Ready's failures, the Definition of Done's failures, and each gate predicate's reasons. Nothing is reworded here, so an agent reads the same sentence wherever it meets the rule.
- The blocked limit is a field of the context rather than a constant read here, because it is a team setting; `escalation::DEFAULT_BLOCKED_LIMIT` is what a runtime with no setting passes. The message says the limit in seconds, which is what the value carries.
- What is left of the sprint appears three times in the context — in `budget`, in `readiness`, and in `assignment` — because each of those answers its own question in its own shape. The doc comment says the runtime derives all three from one figure, or the Definition of Ready at `refining -> ready` and the assignment gate at `ready -> assigned` could disagree about the same sprint.
- `escalation_reason`'s `GovernorEscalation` arm ends in `unwrap_or(EscalationReason::Budget)`, which is unreachable by construction: the row is taken only when the gate opened, and the gate opens only when that same function answered `Some`. It stays because the function must be total, and the alternative — carrying the reason out of the gate and into the decision — would make every other gate return a value it does not have. A mutant of that fallback survives the suite, which is what unreachable means.
- `EscalationReason::Integration` (5.14) is unreachable after this step, because no row of the table carries it: integration happens after `accepted`, which is terminal. Recorded rather than solved here, so that phase 3 meets it in the plan rather than in the code.
- Revised three times on 2026-09-16. The third readiness review confirmed the plan **ready** under all three rules: it re-executed every block with no drift, walked all eighty states of the refining fork and found exactly one row open in each, replayed the seven replacements, compiled a phase-3 caller from the project-plan entry alone, and killed fifty-nine of sixty-three mutants. Its findings, taken here, are two orders and a sentence: the acceptance gate answered "the human has already accepted this contract, so it goes to ready" for a contract that does not pass the Definition of Ready, which the readying gate refuses, so the structural checks are asked first; the spec's new paragraph stated the approval row's present-tense condition and not the readiness row's mirror of it, which is the asymmetry that produced the second pass's blocking bug; and three orders the suite left free — the two inside these gates and the readying gate's own — are now pinned by assertions, each confirmed by re-introducing the mutant. Its two remaining survivors are the unreachable fallback and one semantically equivalent rewrite.
- Revised twice on 2026-09-16. The second readiness review confirmed the first rework, reproduced every number, and found one blocking hole in it: the `ReadinessExhausted` row asked only its counter while its sibling asked the contract, so the two rows overlapped and the counter's row, being first, took a fixed contract to the user as a readiness failure instead of as an approval. Its bullet is above. The pass also found that nothing pinned the budget being read before the permission, and that the `Consumes` list had `BudgetConsequence` missing and `AssignmentRequester` in the wrong half; both taken. Sixty-eight of the seventy mutants it aimed at the reworked code were already killed.
- Revised once on 2026-09-16, after a readiness review refused the plan with four blocking findings. Three were rules the spec states and this function did not hold: a contract waiting for the human could be readied, the approval row opened before the contract was ready, and the governor escalated a task for a budget whose consequence 5.5 gives to somebody else. The fourth was the iteration count, which reversed step 06's landed decision and the schema's own wording without an ADR. The review's should-fix list closed six mutants: three gate arms were pinned only through their missing-value branch, the far end of the saturating conversion was untested, a blank assignee on the contract was untested, and the two budget tests exhausted the day's dollars, which now proves the opposite of what they were written for. Executing the review's own test of the assignment gate then found one more hole: the request's actor and the input's `requested_by` could disagree, which the bullet above closes.
- Tests import the items by name rather than a glob; every code block below is the file after `cargo fmt --all`.

## Design

One task: the `governor::transition` module with `evaluate_transition`, the request, the context, the decision, the effects, the refusals, and twenty-five tests. Most of them are one gate, one refusal, or one effect each; three read the table itself — one opens every one of its twenty rows with the state that belongs to it, which is what spec F5 asks for, one checks that only the five gates with a reason of their own reach `escalated`, and one pins which row is taken when two could open. Between them a row added to 5.2 fails the suite until somebody says what opens it and what it records.

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
crates/core/src/governor/transition.rs               creates: evaluate_transition, the request, the context, the decision, the effects, the refusals, twenty-five tests
docs/SPEC.md                                         modifies: section 5.2's `refining -> ready` row names the human's acceptance, its governor row stops claiming the user's stop and says which budgets escalate a task, and one paragraph says how several rows carrying one move are ordered, whose an assignee's or a reviewer's row is, what each escalation's reason is, what a return to work records, and what a block with no recorded time does
docs/plans/project-plan.md                           modifies: phase 1 step 09's interface records the changes this plan makes to it
docs/plans/phase-1-harness/step-09-transition-evaluation.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: Transition evaluation

Files: created `crates/core/src/governor/transition.rs`; modified `crates/core/src/governor.rs`

Consumes: `std::time::Duration`; `chrono::{DateTime, Utc}`; `budget::{BudgetConsequence, BudgetScope, BudgetState, check_budgets}`; `contract::{TaskContract, TaskId, TaskStatus}`; `generated::task_contract::FarikTaskContractKind`; `governor::done::{CriterionResult, DoneEvidence, evaluate_done, requires_human_acceptance}`; `governor::escalation::{BlockedAge, EscalationReason, READINESS_ATTEMPT_LIMIT, ReadinessOutcome, RejectionOutcome, evaluate_blocked_age, evaluate_readiness_attempts, evaluate_rejection}`; `governor::gates::{AssignmentInput, AssignmentRequester, Blocker, ChildState, GateResult, Rejection, WorkState, check_assignment, check_blocker_resolved, check_blocker_written, check_children_done, check_criteria_recorded, check_rejection_reasons}`; `governor::readiness::{ReadinessContext, evaluate_readiness}`; `governor::transition_table::{GateId, TransitionActor, TransitionRow, find_transitions}`; and in the tests `chrono::TimeZone`, `budget::{DEFAULT_DAY_BUDGET_USD, DEFAULT_SESSION_LIMITS, DEFAULT_SPRINT_BUDGET_USD, SessionLedger}`, `contract::Role`, the generated `Risk`, `governor::done::RunBy`, `governor::escalation::{DEFAULT_BLOCKED_LIMIT, evaluate_rejection}`, `governor::readiness::fixtures::{a_contract, a_ready_context}` and `governor::transition_table::{Status, TRANSITION_TABLE}`
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
          // An unassigned task has no assignee to ask for it, however the request is spelled, and a
          // contract whose assignee is a blank names nobody either.
          for named in [None, Some("  ")] {
              context.contract.assignee = named.map(str::to_string);
              for asked in [None, Some("dev-1"), Some("  ")] {
                  assert_eq!(
                      decide(&ask(TaskStatus::Verifying, A::Assignee, asked), &context),
                      Err(TransitionRefusal::NotTheNamedAgent {
                          actor: A::Assignee,
                          named: named.map(str::to_string),
                          asked: asked.map(str::to_string)
                      }),
                      "{named:?} {asked:?}"
                  );
              }
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
      fn keeps_a_contract_the_human_has_not_accepted_out_of_ready() {
          // Spec 5.16 item 2: every epic requires the human's acceptance of its contract before it
          // leaves `refining`, whatever its risk, and 5.2 says the same for a high risk and for the
          // team's policy. This is the only function the runtime asks, so a rule it does not hold is
          // not held: the epic would reach `ready`, then `assigned`, with no approval ever asked for.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          let ready = ask(TaskStatus::Ready, A::Governor, None);
          let waiting =
              "this contract needs the human's acceptance before it leaves refining (5.16 item 2), and the human has not given it"
                  .to_string();
          context.contract.kind = Kind::Epic;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          context.contract.kind = Kind::Task;
          context.contract.risk = Risk::High;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          context.contract.risk = Risk::Medium;
          context.acceptance.required_by_policy = true;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          // The human is answered before the Definition of Ready, and only that is reported: it is
          // the one thing that has to happen next, and the escalation path reports the failures.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting])
          );
          context.contract = a_context().contract;
          // Once the human has accepted, the Definition of Ready is the whole gate again.
          context.acceptance.given = true;
          assert_eq!(effects(&ready, &context), []);
      }

      #[test]
      fn asks_the_human_only_once_the_contract_passes_the_structural_checks() {
          // Spec 5.16 item 2: "once the contract passes the structural checks, the governor moves the
          // epic to `escalated` with reason `approval`". An epic sent to the user on its first
          // readiness failure would lose the three refining attempts 5.2 gives it, and the human's
          // gate-free `escalated -> any` row would then carry a contract that never passed the
          // Definition of Ready into `ready`.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.contract.kind = Kind::Epic;
          context.readiness_failed_attempts = 1;
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          let failures = gates(&request, &context);
          assert_eq!(failures[1].gate, GateId::ContractRequiresHuman);
          assert_eq!(
              failures[1].details,
              vec![
                  "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                      .to_string(),
                  "the intent is blank; state the user-facing reason for the task".to_string()
              ]
          );
          // The same epic, written properly, goes to the user.
          context.contract = a_context().contract;
          context.contract.kind = Kind::Epic;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
      }

      #[test]
      fn escalates_a_contract_that_has_failed_the_definition_of_ready_three_times() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          // The row is about a contract that fails the Definition of Ready, so this one does.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
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
      fn asks_the_human_about_a_contract_that_now_passes_however_often_it_failed_before() {
          // The two rows into `escalated` from `refining` must not overlap: a contract that failed
          // the Definition of Ready three times, was sent back by the human, and now passes is
          // waiting for the approval 5.16 item 2 asks for, not for its readiness failures again. The
          // board would otherwise show it as having failed readiness, the human would resolve it by
          // moving it rather than approving it, and the approval `check_product_doc_write` needs
          // would never be recorded.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.contract.kind = Kind::Epic;
          context.readiness_failed_attempts = 3;
          let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
              .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::ContractRequiresHuman);
          assert_eq!(
              decision.effects,
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
          // And a contract nobody has to accept, that now passes, is not escalated at all: it is
          // ready, whatever the counter says.
          context.contract.kind = Kind::Task;
          let failures = gates(&ask(TaskStatus::Escalated, A::Governor, None), &context);
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
                  "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                      .to_string()
              ]
          );
          // The contract is answered before the counter, so a fixed contract reads the same whatever
          // the runtime counted.
          context.readiness_failed_attempts = 1;
          assert_eq!(
              gates(&ask(TaskStatus::Escalated, A::Governor, None), &context)[0].details,
              vec![
                  "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                      .to_string()
              ]
          );
          assert_eq!(
              effects(&ask(TaskStatus::Ready, A::Governor, None), &context),
              []
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
          // A contract the human has accepted that does not pass the Definition of Ready must not be
          // told it goes to `ready`, because the readying gate refuses that very move: the flow 5.16
          // names, a user writing an epic themselves and locking and approving it, reaches this state.
          context.acceptance.given = true;
          let mut unready = a_context();
          unready.status = TaskStatus::Refining;
          unready.contract.kind = Kind::Epic;
          unready.acceptance.given = true;
          unready.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              gates(&request, &unready)[1].details,
              vec![
                  "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                      .to_string(),
                  "the intent is blank; state the user-facing reason for the task".to_string()
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
          // The gate's details are the assignment gate's own: a limit of zero is that gate's answer
          // and this module does not reword it.
          context
              .assignment
              .as_mut()
              .expect("the fixture assigns")
              .wip_limit = 0;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Assignment,
                  vec!["dev-1 takes no work: its limit is zero".to_string()]
              )
          );
          // And the Product Manager asks only when the team has no active Scrum Master, which is the
          // same gate refusing on its own row rather than a row missing. Who asked comes from the
          // request's actor, not from the input's `requested_by`, or the fixture's Scrum Master would
          // open the Product Manager's row.
          let mut by_the_pm = a_context();
          by_the_pm.status = TaskStatus::Ready;
          assert_eq!(
              one_gate(
                  &ask(TaskStatus::Assigned, A::ProductManager, Some("pm-1")),
                  &by_the_pm
              ),
              (
                  GateId::Assignment,
                  vec![
                      "the Product Manager assigns only when the team has no active Scrum Master"
                          .to_string()
                  ]
              )
          );
          // A runtime that named no pair of agents is told so rather than refused for a rule it could
          // not have met.
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
          // A blocker that says nothing about what is needed is the gate's answer, not a missing
          // value, and the details are the gate's own.
          context.blocker.as_mut().expect("the fixture blocks").needed = " ".to_string();
          assert_eq!(
              one_gate(&block, &context),
              (
                  GateId::BlockerWritten,
                  vec!["the blocker does not say what is needed to clear it".to_string()]
              )
          );
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
          // With the age row shut and the task's own budget exhausted, the governor's own row carries
          // the move, and the reason follows the row that opened rather than the one that did not.
          context.budget.task_spent_usd = context.budget.task_max_usd;
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
      fn rejects_work_only_with_written_reasons_and_leaves_the_count_to_the_return() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1"));
          // The contract's `iteration` is how many times the task has already been returned to
          // `in_progress` after a rejection, which is what `max_iterations` bounds (the schema's own
          // words, and step 06's decision), so the rejection itself records nothing.
          assert_eq!(effects(&request, &context), []);
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
          // A rejection the contract cannot make sense of is the gate's own answer, unreworded.
          context.rejection = Some(Rejection {
              failed_criterion_ids: vec!["C9".to_string()],
              reasons: "The form accepts an empty password.".to_string(),
          });
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::RejectionReasons,
                  vec!["this contract has no criterion C9".to_string()]
              )
          );
      }

      #[test]
      fn counts_the_iteration_when_the_task_returns_to_work_and_clears_its_blocker() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          // The return is what the count is of, and a task going back to work carries no blocker.
          assert_eq!(
              effects(&ask(TaskStatus::InProgress, A::Governor, None), &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
          // A task the human takes out of `escalated` into work leaves its blocker behind too: one
          // that escalated out of `blocked` kept it, and a stale blocker would age again.
          context.status = TaskStatus::Escalated;
          assert_eq!(
              effects(&ask(TaskStatus::InProgress, A::Human, None), &context),
              [TransitionEffect::ResetBlocker]
          );
      }

      #[test]
      fn works_a_rejected_task_again_until_the_iteration_limit() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          let again = ask(TaskStatus::InProgress, A::Governor, None);
          let escalate = ask(TaskStatus::Escalated, A::Governor, None);
          // The contract's default limit is three; the iteration counts the returns so far, and the
          // return this decides is the one it counts.
          context.contract.iteration = 2;
          assert_eq!(
              effects(&again, &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
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
          // A limit that large is what the contract asked for: effectively none, so the task is
          // worked again however many times it has been returned.
          context.contract.budget.max_iterations = (u64::from(u32::MAX) + 1)
              .try_into()
              .expect("a limit above u32::MAX is still non-zero");
          context.contract.iteration = 5;
          assert_eq!(
              effects(&again, &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
      }

      #[test]
      fn escalates_only_on_a_budget_whose_consequence_is_escalation() {
          let mut context = a_context();
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          let nothing_to_escalate =
              "no budget whose consequence is escalation is exhausted and no permission was denied, so the governor has nothing to escalate"
                  .to_string();
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::GovernorEscalation,
                  vec![nothing_to_escalate.clone()]
              )
          );
          // Spec 5.5 gives every budget its own consequence, and only the task's carry escalation:
          // the session's end the session and block the task, the sprint's stops new assignments,
          // and the day's pauses the team. Escalating one task for any of those would be the wrong
          // answer to the wrong question.
          for elsewhere in [
              |budget: &mut BudgetState| {
                  budget.session.usage.input_tokens = budget.session_limits.max_input_tokens;
              },
              |budget: &mut BudgetState| {
                  budget.session.wall_clock = budget.session_limits.max_wall_clock;
              },
              |budget: &mut BudgetState| {
                  budget.session.tool_calls = budget.session_limits.max_tool_calls;
              },
              |budget: &mut BudgetState| budget.sprint_spent_usd = budget.sprint_max_usd,
              |budget: &mut BudgetState| budget.day_spent_usd = budget.day_max_usd,
          ] {
              context.budget = a_budget();
              elsewhere(&mut context.budget);
              assert_eq!(
                  one_gate(&request, &context),
                  (
                      GateId::GovernorEscalation,
                      vec![nothing_to_escalate.clone()]
                  )
              );
          }
          // The task's dollars are `budget` and its sessions are `sessions` (5.7).
          context.budget = a_budget();
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          context.budget = a_budget();
          context.budget.task_sessions = context.budget.task_max_sessions;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Sessions)]
          );
          // Both at once: the dollars are the harder limit and the reason the user reads.
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          // A budget is read before a permission, so a task that has run out of money and been
          // denied something reads as the money.
          context.permission_denied = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          // And a permission denied on a required action, with every budget in hand.
          context.budget = a_budget();
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
          // An agent cannot: the row is the human's, and the actor is answered before the agent id,
          // so a reviewer asking with the wrong id is told the row is not its actor's at all.
          for (actor, agent_id) in [(A::Assignee, "dev-1"), (A::Reviewer, "dev-2")] {
              assert_eq!(
                  decide(&ask(TaskStatus::Ready, actor, Some(agent_id)), &context),
                  Err(TransitionRefusal::ActorNotAllowed {
                      actor,
                      allowed: vec![A::Human]
                  }),
                  "{actor:?}"
              );
          }
          // Cancelling is the human's too, and the two rows that carry `escalated -> cancelled` name
          // the same actor, which is named once.
          assert_eq!(
              decide(
                  &ask(TaskStatus::Cancelled, A::Assignee, Some("dev-1")),
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
                  context.contract.intent = " "
                      .repeat(24)
                      .parse()
                      .expect("twenty-four spaces pass the schema");
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
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          context.budget.task_spent_usd = context.budget.task_max_usd;
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
          // Every move into `in_progress` clears the blocker: a task at work has none, and the
          // runtime clearing nothing costs nothing.
          assert_eq!(decision.effects, [TransitionEffect::ResetBlocker]);
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

  use crate::budget::{BudgetConsequence, BudgetScope, BudgetState, check_budgets};
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
      AssignmentInput, AssignmentRequester, Blocker, ChildState, GateResult, Rejection, WorkState,
      check_assignment, check_blocker_resolved, check_blocker_written, check_children_done,
      check_criteria_recorded, check_rejection_reasons,
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
      /// has just happened, so that the third failure escalates (`docs/SPEC.md` section 5.2). The
      /// runtime may keep it as a running total: the gate that reads it also asks whether the
      /// contract fails the Definition of Ready now, so a counter nobody reset cannot escalate a
      /// contract that has since been fixed.
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
      /// Every budget's spend and limit. What is left of the sprint appears three times in this
      /// context — here, in `readiness`, and in `assignment` — because each of those answers its own
      /// question; the runtime derives all three from one figure, or the Definition of Ready and the
      /// assignment gate could disagree about the same sprint.
      pub budget: BudgetState,
      /// Whether a permission was denied on an action the task requires.
      pub permission_denied: bool,
  }

  /// What the runtime must record along with the move.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TransitionEffect {
      /// The contract's `iteration` goes up by one: the task has been returned to `in_progress`
      /// after a rejection once more, which is what `max_iterations` bounds.
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
          match check_gate(row.gate, request.actor, context) {
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
  fn check_gate(gate: GateId, actor: TransitionActor, context: &TransitionContext) -> GateResult {
      match gate {
          GateId::None => Ok(()),
          GateId::Triaged => open_or(context.triaged, || {
              "the request has no recorded triage decision, and refining starts from one (5.16)"
                  .to_string()
          }),
          GateId::DefinitionOfReady => {
              if waits_for_the_human(context) {
                  return Err(vec![
                      "this contract needs the human's acceptance before it leaves refining (5.16 item 2), and the human has not given it"
                          .to_string(),
                  ]);
              }
              readiness_failures(context).map_or(Ok(()), Err)
          }
          GateId::ReadinessExhausted => readiness_exhausted(context),
          GateId::ContractRequiresHuman => human_must_accept_the_contract(context),
          GateId::Assignment => match &context.assignment {
              Some(assignment) => {
                  let mut asked = assignment.clone();
                  asked.requested_by = requester(actor);
                  check_assignment(&context.contract, &asked)
              }
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
                  "no budget whose consequence is escalation is exhausted and no permission was denied, so the governor has nothing to escalate"
                      .to_string()
              })
          }
      }
  }

  /// Who asked for an assignment, from the actor of the row rather than from the input: the request
  /// says who is asking and the two must not be able to disagree, or a team with an active Scrum
  /// Master could have the Product Manager's row opened by an input that says the Scrum Master asked.
  /// Only those two rows carry an assignment (5.2), so no other actor reaches this.
  fn requester(actor: TransitionActor) -> AssignmentRequester {
      match actor {
          TransitionActor::ProductManager => AssignmentRequester::ProductManager,
          TransitionActor::ScrumMaster
          | TransitionActor::Assignee
          | TransitionActor::Reviewer
          | TransitionActor::Governor
          | TransitionActor::Human => AssignmentRequester::ScrumMaster,
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
      // 5.16 item 2 asks the human once the contract passes the structural checks. A contract sent to
      // the user on its first readiness failure would lose the three refining attempts 5.2 gives it,
      // and the human's gate-free `escalated -> any` row would then carry a contract that never
      // passed the Definition of Ready into `ready`. This is asked before the acceptance, because a
      // contract the human has accepted that does not pass would otherwise be told it goes to `ready`,
      // which the readying gate refuses: the user writing an epic themselves and locking and
      // approving it (5.11, 5.16) reaches exactly that state.
      if let Some(failures) = readiness_failures(context) {
          let mut details = vec![
              "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                  .to_string(),
          ];
          details.extend(failures);
          return Err(details);
      }
      if context.acceptance.given {
          return Err(vec![
              "the human has already accepted this contract, so it goes to ready rather than escalating"
                  .to_string(),
          ]);
      }
      Ok(())
  }

  /// The `ReadinessExhausted` gate of `refining -> escalated`: the contract fails the Definition of
  /// Ready now, and it has failed it the limit's worth of times. Asking the contract as well as the
  /// counter is what keeps this row and the approval row apart, whatever the runtime's counter does:
  /// a contract that failed three times, was sent back by the human and now passes is waiting for the
  /// approval 5.16 item 2 asks for, not for its failures again, and one nobody has to accept is
  /// simply ready.
  fn readiness_exhausted(context: &TransitionContext) -> GateResult {
      if readiness_failures(context).is_none() {
          return Err(vec![
              "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                  .to_string(),
          ]);
      }
      open_or(
          evaluate_readiness_attempts(context.readiness_failed_attempts)
              == ReadinessOutcome::Escalate,
          || {
              format!(
                  "the contract has failed the Definition of Ready {} times, and it is refined again until {READINESS_ATTEMPT_LIMIT}",
                  context.readiness_failed_attempts
              )
          },
      )
  }

  /// Whether this contract is waiting for the human's acceptance: the team's policy asks for it, or
  /// the contract's own risk or kind does (`done::requires_human_acceptance`), and the human has not
  /// answered. Either half is enough, so the two cannot disagree in the direction that would carry a
  /// task past the human.
  fn waits_for_the_human(context: &TransitionContext) -> bool {
      (context.acceptance.required_by_policy || requires_human_acceptance(&context.contract))
          && !context.acceptance.given
  }

  /// The Definition of Ready's own messages when the contract fails it, in its own order, and `None`
  /// when it passes. A `refining -> escalated` request evaluates this twice when the readiness row
  /// shuts and the acceptance row is tried next; it is a pure function of the same values both times,
  /// so the two answers cannot differ, and the cost is nineteen re-run checks and no I/O.
  fn readiness_failures(context: &TransitionContext) -> Option<Vec<String>> {
      evaluate_readiness(&context.contract, &context.readiness)
          .err()
          .map(|failures| {
              failures
                  .iter()
                  .map(|failure| failure.message.clone())
                  .collect()
          })
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
      for exhausted in check_budgets(&context.budget) {
          if exhausted.consequence != BudgetConsequence::EscalateTask {
              continue;
          }
          return Some(match exhausted.scope {
              BudgetScope::TaskSessions => EscalationReason::Sessions,
              // The task's dollars are the only other scope 5.5 gives `EscalateTask`, and they come
              // first in `BudgetScope`, so a task out of both reads as the dollars, the harder limit.
              // A scope that grew that consequence would escalate on dollars until somebody chose its
              // reason here.
              BudgetScope::TaskUsd
              | BudgetScope::SessionTokens
              | BudgetScope::SessionWallClock
              | BudgetScope::SessionToolCalls
              | BudgetScope::SprintUsd
              | BudgetScope::DayUsd => EscalationReason::Budget,
          });
      }
      if context.permission_denied {
          return Some(EscalationReason::Permission);
      }
      None
  }

  /// What the runtime records with the move: a task returning to work after a rejection counts that
  /// return, which is what `max_iterations` bounds (step 06, and the schema's own words); a move to
  /// `escalated` carries the reason the gate that opened it names; and a task entering `in_progress`
  /// leaves its blocker behind, whatever status it came from, because a task at work has none and a
  /// stale one would age again — a task that escalates out of `blocked` keeps its blocker, which is
  /// what the user is shown.
  fn effects(gate: GateId, to: TaskStatus, context: &TransitionContext) -> Vec<TransitionEffect> {
      let mut effects = Vec::new();
      if context.status == TaskStatus::Rejected && to == TaskStatus::InProgress {
          effects.push(TransitionEffect::IncrementIteration);
      }
      if to == TaskStatus::Escalated {
          effects.push(TransitionEffect::RaiseEscalation(escalation_reason(
              gate, context,
          )));
      }
      if to == TaskStatus::InProgress {
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
          // An unassigned task has no assignee to ask for it, however the request is spelled, and a
          // contract whose assignee is a blank names nobody either.
          for named in [None, Some("  ")] {
              context.contract.assignee = named.map(str::to_string);
              for asked in [None, Some("dev-1"), Some("  ")] {
                  assert_eq!(
                      decide(&ask(TaskStatus::Verifying, A::Assignee, asked), &context),
                      Err(TransitionRefusal::NotTheNamedAgent {
                          actor: A::Assignee,
                          named: named.map(str::to_string),
                          asked: asked.map(str::to_string)
                      }),
                      "{named:?} {asked:?}"
                  );
              }
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
      fn keeps_a_contract_the_human_has_not_accepted_out_of_ready() {
          // Spec 5.16 item 2: every epic requires the human's acceptance of its contract before it
          // leaves `refining`, whatever its risk, and 5.2 says the same for a high risk and for the
          // team's policy. This is the only function the runtime asks, so a rule it does not hold is
          // not held: the epic would reach `ready`, then `assigned`, with no approval ever asked for.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          let ready = ask(TaskStatus::Ready, A::Governor, None);
          let waiting =
              "this contract needs the human's acceptance before it leaves refining (5.16 item 2), and the human has not given it"
                  .to_string();
          context.contract.kind = Kind::Epic;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          context.contract.kind = Kind::Task;
          context.contract.risk = Risk::High;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          context.contract.risk = Risk::Medium;
          context.acceptance.required_by_policy = true;
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting.clone()])
          );
          // The human is answered before the Definition of Ready, and only that is reported: it is
          // the one thing that has to happen next, and the escalation path reports the failures.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              one_gate(&ready, &context),
              (GateId::DefinitionOfReady, vec![waiting])
          );
          context.contract = a_context().contract;
          // Once the human has accepted, the Definition of Ready is the whole gate again.
          context.acceptance.given = true;
          assert_eq!(effects(&ready, &context), []);
      }

      #[test]
      fn asks_the_human_only_once_the_contract_passes_the_structural_checks() {
          // Spec 5.16 item 2: "once the contract passes the structural checks, the governor moves the
          // epic to `escalated` with reason `approval`". An epic sent to the user on its first
          // readiness failure would lose the three refining attempts 5.2 gives it, and the human's
          // gate-free `escalated -> any` row would then carry a contract that never passed the
          // Definition of Ready into `ready`.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.contract.kind = Kind::Epic;
          context.readiness_failed_attempts = 1;
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          let failures = gates(&request, &context);
          assert_eq!(failures[1].gate, GateId::ContractRequiresHuman);
          assert_eq!(
              failures[1].details,
              vec![
                  "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                      .to_string(),
                  "the intent is blank; state the user-facing reason for the task".to_string()
              ]
          );
          // The same epic, written properly, goes to the user.
          context.contract = a_context().contract;
          context.contract.kind = Kind::Epic;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
      }

      #[test]
      fn escalates_a_contract_that_has_failed_the_definition_of_ready_three_times() {
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.readiness_failed_attempts = 3;
          // The row is about a contract that fails the Definition of Ready, so this one does.
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
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
      fn asks_the_human_about_a_contract_that_now_passes_however_often_it_failed_before() {
          // The two rows into `escalated` from `refining` must not overlap: a contract that failed
          // the Definition of Ready three times, was sent back by the human, and now passes is
          // waiting for the approval 5.16 item 2 asks for, not for its readiness failures again. The
          // board would otherwise show it as having failed readiness, the human would resolve it by
          // moving it rather than approving it, and the approval `check_product_doc_write` needs
          // would never be recorded.
          let mut context = a_context();
          context.status = TaskStatus::Refining;
          context.contract.kind = Kind::Epic;
          context.readiness_failed_attempts = 3;
          let decision = decide(&ask(TaskStatus::Escalated, A::Governor, None), &context)
              .expect("the move is allowed");
          assert_eq!(decision.row.gate, GateId::ContractRequiresHuman);
          assert_eq!(
              decision.effects,
              [TransitionEffect::RaiseEscalation(Why::Approval)]
          );
          // And a contract nobody has to accept, that now passes, is not escalated at all: it is
          // ready, whatever the counter says.
          context.contract.kind = Kind::Task;
          let failures = gates(&ask(TaskStatus::Escalated, A::Governor, None), &context);
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
                  "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                      .to_string()
              ]
          );
          // The contract is answered before the counter, so a fixed contract reads the same whatever
          // the runtime counted.
          context.readiness_failed_attempts = 1;
          assert_eq!(
              gates(&ask(TaskStatus::Escalated, A::Governor, None), &context)[0].details,
              vec![
                  "the contract passes the Definition of Ready, so it goes to ready rather than escalating on its failures"
                      .to_string()
              ]
          );
          assert_eq!(
              effects(&ask(TaskStatus::Ready, A::Governor, None), &context),
              []
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
          // A contract the human has accepted that does not pass the Definition of Ready must not be
          // told it goes to `ready`, because the readying gate refuses that very move: the flow 5.16
          // names, a user writing an epic themselves and locking and approving it, reaches this state.
          context.acceptance.given = true;
          let mut unready = a_context();
          unready.status = TaskStatus::Refining;
          unready.contract.kind = Kind::Epic;
          unready.acceptance.given = true;
          unready.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              gates(&request, &unready)[1].details,
              vec![
                  "the human is asked once the contract passes the structural checks (5.16 item 2), and this one does not yet"
                      .to_string(),
                  "the intent is blank; state the user-facing reason for the task".to_string()
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
          // The gate's details are the assignment gate's own: a limit of zero is that gate's answer
          // and this module does not reword it.
          context
              .assignment
              .as_mut()
              .expect("the fixture assigns")
              .wip_limit = 0;
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::Assignment,
                  vec!["dev-1 takes no work: its limit is zero".to_string()]
              )
          );
          // And the Product Manager asks only when the team has no active Scrum Master, which is the
          // same gate refusing on its own row rather than a row missing. Who asked comes from the
          // request's actor, not from the input's `requested_by`, or the fixture's Scrum Master would
          // open the Product Manager's row.
          let mut by_the_pm = a_context();
          by_the_pm.status = TaskStatus::Ready;
          assert_eq!(
              one_gate(
                  &ask(TaskStatus::Assigned, A::ProductManager, Some("pm-1")),
                  &by_the_pm
              ),
              (
                  GateId::Assignment,
                  vec![
                      "the Product Manager assigns only when the team has no active Scrum Master"
                          .to_string()
                  ]
              )
          );
          // A runtime that named no pair of agents is told so rather than refused for a rule it could
          // not have met.
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
          // A blocker that says nothing about what is needed is the gate's answer, not a missing
          // value, and the details are the gate's own.
          context.blocker.as_mut().expect("the fixture blocks").needed = " ".to_string();
          assert_eq!(
              one_gate(&block, &context),
              (
                  GateId::BlockerWritten,
                  vec!["the blocker does not say what is needed to clear it".to_string()]
              )
          );
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
          // With the age row shut and the task's own budget exhausted, the governor's own row carries
          // the move, and the reason follows the row that opened rather than the one that did not.
          context.budget.task_spent_usd = context.budget.task_max_usd;
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
      fn rejects_work_only_with_written_reasons_and_leaves_the_count_to_the_return() {
          let mut context = a_context();
          context.status = TaskStatus::Verifying;
          let request = ask(TaskStatus::Rejected, A::Reviewer, Some("arch-1"));
          // The contract's `iteration` is how many times the task has already been returned to
          // `in_progress` after a rejection, which is what `max_iterations` bounds (the schema's own
          // words, and step 06's decision), so the rejection itself records nothing.
          assert_eq!(effects(&request, &context), []);
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
          // A rejection the contract cannot make sense of is the gate's own answer, unreworded.
          context.rejection = Some(Rejection {
              failed_criterion_ids: vec!["C9".to_string()],
              reasons: "The form accepts an empty password.".to_string(),
          });
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::RejectionReasons,
                  vec!["this contract has no criterion C9".to_string()]
              )
          );
      }

      #[test]
      fn counts_the_iteration_when_the_task_returns_to_work_and_clears_its_blocker() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          // The return is what the count is of, and a task going back to work carries no blocker.
          assert_eq!(
              effects(&ask(TaskStatus::InProgress, A::Governor, None), &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
          // A task the human takes out of `escalated` into work leaves its blocker behind too: one
          // that escalated out of `blocked` kept it, and a stale blocker would age again.
          context.status = TaskStatus::Escalated;
          assert_eq!(
              effects(&ask(TaskStatus::InProgress, A::Human, None), &context),
              [TransitionEffect::ResetBlocker]
          );
      }

      #[test]
      fn works_a_rejected_task_again_until_the_iteration_limit() {
          let mut context = a_context();
          context.status = TaskStatus::Rejected;
          let again = ask(TaskStatus::InProgress, A::Governor, None);
          let escalate = ask(TaskStatus::Escalated, A::Governor, None);
          // The contract's default limit is three; the iteration counts the returns so far, and the
          // return this decides is the one it counts.
          context.contract.iteration = 2;
          assert_eq!(
              effects(&again, &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
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
          // A limit that large is what the contract asked for: effectively none, so the task is
          // worked again however many times it has been returned.
          context.contract.budget.max_iterations = (u64::from(u32::MAX) + 1)
              .try_into()
              .expect("a limit above u32::MAX is still non-zero");
          context.contract.iteration = 5;
          assert_eq!(
              effects(&again, &context),
              [
                  TransitionEffect::IncrementIteration,
                  TransitionEffect::ResetBlocker
              ]
          );
      }

      #[test]
      fn escalates_only_on_a_budget_whose_consequence_is_escalation() {
          let mut context = a_context();
          let request = ask(TaskStatus::Escalated, A::Governor, None);
          let nothing_to_escalate =
              "no budget whose consequence is escalation is exhausted and no permission was denied, so the governor has nothing to escalate"
                  .to_string();
          assert_eq!(
              one_gate(&request, &context),
              (
                  GateId::GovernorEscalation,
                  vec![nothing_to_escalate.clone()]
              )
          );
          // Spec 5.5 gives every budget its own consequence, and only the task's carry escalation:
          // the session's end the session and block the task, the sprint's stops new assignments,
          // and the day's pauses the team. Escalating one task for any of those would be the wrong
          // answer to the wrong question.
          for elsewhere in [
              |budget: &mut BudgetState| {
                  budget.session.usage.input_tokens = budget.session_limits.max_input_tokens;
              },
              |budget: &mut BudgetState| {
                  budget.session.wall_clock = budget.session_limits.max_wall_clock;
              },
              |budget: &mut BudgetState| {
                  budget.session.tool_calls = budget.session_limits.max_tool_calls;
              },
              |budget: &mut BudgetState| budget.sprint_spent_usd = budget.sprint_max_usd,
              |budget: &mut BudgetState| budget.day_spent_usd = budget.day_max_usd,
          ] {
              context.budget = a_budget();
              elsewhere(&mut context.budget);
              assert_eq!(
                  one_gate(&request, &context),
                  (
                      GateId::GovernorEscalation,
                      vec![nothing_to_escalate.clone()]
                  )
              );
          }
          // The task's dollars are `budget` and its sessions are `sessions` (5.7).
          context.budget = a_budget();
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          context.budget = a_budget();
          context.budget.task_sessions = context.budget.task_max_sessions;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Sessions)]
          );
          // Both at once: the dollars are the harder limit and the reason the user reads.
          context.budget.task_spent_usd = context.budget.task_max_usd;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          // A budget is read before a permission, so a task that has run out of money and been
          // denied something reads as the money.
          context.permission_denied = true;
          assert_eq!(
              effects(&request, &context),
              [TransitionEffect::RaiseEscalation(Why::Budget)]
          );
          // And a permission denied on a required action, with every budget in hand.
          context.budget = a_budget();
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
          // An agent cannot: the row is the human's, and the actor is answered before the agent id,
          // so a reviewer asking with the wrong id is told the row is not its actor's at all.
          for (actor, agent_id) in [(A::Assignee, "dev-1"), (A::Reviewer, "dev-2")] {
              assert_eq!(
                  decide(&ask(TaskStatus::Ready, actor, Some(agent_id)), &context),
                  Err(TransitionRefusal::ActorNotAllowed {
                      actor,
                      allowed: vec![A::Human]
                  }),
                  "{actor:?}"
              );
          }
          // Cancelling is the human's too, and the two rows that carry `escalated -> cancelled` name
          // the same actor, which is named once.
          assert_eq!(
              decide(
                  &ask(TaskStatus::Cancelled, A::Assignee, Some("dev-1")),
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
                  context.contract.intent = " "
                      .repeat(24)
                      .parse()
                      .expect("twenty-four spaces pass the schema");
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
          context.contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          context.budget.task_spent_usd = context.budget.task_max_usd;
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
          // Every move into `in_progress` clears the blocker: a task at work has none, and the
          // runtime clearing nothing costs nothing.
          assert_eq!(decision.effects, [TransitionEffect::ResetBlocker]);
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
  | any | escalated | Governor | a budget whose consequence is escalation is exhausted — the task's dollars or its sessions (5.5) — or a permission was denied on a required action; a `stop` from the user takes the human's row below, which needs no gate, because the governor is not what hears the user (added in 0.3) |
  ```

- [ ] Say in the spec that readying a contract waits for the human. In `docs/SPEC.md` section 5.2's transition table, replace

  ```
  | refining | ready | Governor, after PM submits contract | Definition of Ready (5.3) |
  ```

  with

  ```
  | refining | ready | Governor, after PM submits contract | Definition of Ready (5.3), and the human's acceptance of the contract where it is required: an epic always, a `high` risk, or the team's policy (5.16 item 2, 5.12; added in 0.3) |
  ```

- [ ] Say in the spec how the table is read when more than one row carries a move. In `docs/SPEC.md` section 5.2, where the before-text is a substring of a longer line and occurs exactly once, replace

  ```
  The governor judges an assignment on what the runtime tells it,
  ```

  with

  ```
  More than one row can carry one move: three send a `refining` contract to `escalated`, and a blocked or rejected task has its own row and the governor's. The rows are taken in the order they appear here and the first whose gate opens is the one recorded, so the more specific reason is the one the user reads; when none opens, the refusal says what every gate it tried was waiting for. A row whose actor is the assignee or the reviewer is open only to the agent the contract names in that field, so one Developer cannot declare another's task done (5.1), and who asked for an assignment is the actor of the request rather than anything the runtime repeats back. A move to `escalated` raises an escalation whose reason is the row's: the readiness failures, an epic's approval or a task's risk gate, the blocker's age, the iteration limit, the exhausted budget (the task's sessions being their own reason), the denied permission, or the user's own request on the human's row (5.7). The approval row opens only once the contract passes the structural checks (5.16 item 2), so a contract that fails them is refined again rather than sent to the user, and the readiness row is the mirror of it: a contract that now passes is not escalated on its earlier failures, however many the runtime counted, so the two rows never both open and a contract that has been fixed reaches the user as an approval rather than as a failure. `iteration` counts returns: it goes up by one as the task goes from `rejected` back to `in_progress`, which is what `max_iterations` bounds, and every move into `in_progress` clears the blocker and its time, including the one the human makes from `escalated`, because a task that escalated out of `blocked` kept its blocker and a stale one would age again. A block the runtime recorded no time for cannot be aged, so the governor refuses to escalate it and says so, as it does for any value it was not given (added in 0.3).

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
  # test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 198 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 223 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [ ] Commit: `feat(core): decide every transition of the lifecycle`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 223 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
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
