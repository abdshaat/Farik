# Phase 1, step 02: Definition of Ready and team rules

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.3 (Definition of Ready), 5.12 (team rules), 5.16 (an epic's tasks: parent, paths, budget), 5.6 (the default protected paths), F5
Depends on: phase 0 (merged in #4); step 01 of this phase (committed as f9f0e67, 0e50df1, 3cc8bc3)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-16, before execution (first pass NOT READY on two undecided cases, the human as reviewer and the budget cap on epics, decided in 96e70da; second pass READY under all three rules at 96e70da; recorded on pull request #5)

## Goal

`farik-core` can say whether a contract is ready to work on, and if not, exactly which rules of `docs/SPEC.md` section 5.3 it fails and what would fix each: the structural rules, the team rules of 5.12, the three rules that tie an epic's task to its epic (5.16), and the Scrum Master's recorded judgment when the team has one. Team rules exist as a value with the spec's defaults. Every crate's tests can build a typed contract and a context in which it is ready. Nothing in this step evaluates a transition; step 09 composes this function behind the `DefinitionOfReady` gate.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- The rules are the project plan's `ReadinessRule` list minus `RiskSet`: the schema requires `risk`, so `validate_contract` refuses a contract without one before readiness is ever evaluated, and a rule that cannot fail has no failing test. The project plan's interface entry is changed in the same commit as this plan, per its "Changing this plan" section. Rejected: keeping the variant for spec traceability, because hard rule 1 asks for a test that fails first.
- Rules the schema guarantees on the wire are still checked on the typed value where it can differ: the intent and `out_of_scope` items must have words (the schema counts whitespace), `exit_criteria` can be empty on a contract built in Rust, a `command` or `test` criterion's command must not be blank, and a criterion's `method` value must name the shape its fields have (the generated types carry `method` as a plain value, so an inconsistent one can be built in Rust).
- One check function per rule, all with the signature `fn(&TaskContract, &ReadinessContext) -> Option<ReadinessFailure>`, listed in a `const` array in enum order; `evaluate_readiness` runs them all and reports every failure, so that the Product Manager fixes a contract in one pass. Rejected: stopping at the first failure.
- `DependenciesReady`: a dependency counts as at least `ready` when its status is `ready`, `assigned`, `in_progress`, `blocked`, `verifying`, `rejected`, or `accepted`; one missing from the context does not exist. Rejected: counting `escalated` and `cancelled`, because neither is on the way to `accepted`.
- `ReviewerAvailable` passes when the reviewer role is `human`, because the human is the reviewer of an epic the Product Manager broke down (spec 5.1, 5.16 item 4) and is always available; otherwise it counts agents of the reviewer role: one when it differs from the assignee role, two when it is the same (spec 5.3, decided 2026-09-15), and the message ends with `add an agent with role <role>`. Rejected: having the runtime report the human as an active agent, because the human is not an agent.
- Paths within a ceiling or a parent are judged by literal prefix. A glob is split at its first `*`, `?`, `[`, or `{`; an entry whose rest is empty or `**` is a directory ceiling (`src`, `src/`, `src/**`, `**`) and admits every path whose own literal prefix is that directory or below it: `src/login/**` is within `src/**`, `src2/**` is not, `**` admits everything. Any other entry (`docs/**/*.md`, `src/*.rs`) admits only a path written exactly like it, so that a file filter is never widened into its directory. Paths are compared as written: `./src/**` is not `src/**`. Rejected: glob containment through `globset`, because deciding whether one glob contains another is not what the crate offers, and step 03 matches concrete paths, not patterns.
- Team rules defaults: `protected_paths` as in spec 5.6, `max_task_budget_usd` 5 dollars (project plan D3), everything else empty or off. `docs/SPEC.md` 5.12 says "everything else empty or off"; it is updated in this step to name the budget cap, because behavior and spec change together (hard rule 8). `DEFAULT_TEAM_RULES` is a `LazyLock` because the value owns strings.
- The team cap is a cap on a task (D3 says "a task's `max_cost_usd`"): `BudgetWithinTeamMax` skips an epic, whose budget is bounded by the sprint budget through `BudgetWithinSprint` and whose tasks each fit under the cap and within the epic's remaining budget. Rejected: capping epics too, because under the default rules no epic could then exceed five dollars in total. The 5.12 table row and the defaults sentence say so.
- `TeamRules` is a plain value with `Default`; the file format that fills it is `team.schema.json` in phase 2 step 05, which adds its own conversion.
- The judgment rules apply only when the context says the team has an active Scrum Master (`requires_judgment_review`); without one the structural checks alone gate (project plan D2), and a review recorded anyway is ignored.
- Fixtures: `governor::readiness::fixtures::a_contract()` returns the phase 0 wire fixture typed, built once in a `LazyLock` whose `expect` says why it cannot fail (the wire fixture is pinned schema-valid by `contract::tests`); `a_ready_context()` is a context in which it is ready. Both are `pub` so later steps' tests use them.
- Tests use qualified enum paths through short aliases (`R`, `Kind`) rather than glob imports; check functions return `Option` through a `failure` constructor wrapped at the call site, because `clippy::unnecessary_wraps` refuses a helper that always returns `Some`.
- Every code block below is the file after `cargo fmt --all`, so that the committed file and the plan are the same bytes.

## Design

Two tasks. The first adds `governor::team_rules` (the value, its defaults, the two constants) and the 5.12 wording in the spec. The second adds `governor::readiness` (the rule enum, the context types, the failure type, `evaluate_readiness`, and one check per rule) with its fixtures, and one test per rule plus one for the fixture pair and one for the reporting order.

Out of scope: matching concrete changed paths against globs (step 03), permission tiers and forbidden commands (step 04), budgets and cost (step 05), the assignment gate's reviewer-is-not-assignee check (step 08), composing this behind the gate (step 09).

## Architecture notes

Touches `crates/core` only: two new children of `governor` and a fixtures module. Consumes `contract::{Role, TaskContract, TaskStatus, Verification, VerificationWire, validate_contract}` and `contract::fixtures::a_contract_wire` (phase 0 step 03), the generated `FarikTaskContractKind` (phase 0 step 02), and nothing from step 01. Adds no dependency. Edits `docs/SPEC.md` 5.12 (one row and one sentence) and `docs/plans/project-plan.md` (the `RiskSet` decision and the two default constants).

## Global constraints

- `farik-core` does no I/O; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` outside tests and the fixture's `LazyLock`.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                          modifies: declares team_rules (task 1) and readiness (task 2)
crates/core/src/governor/team_rules.rs               creates: TeamRules, Default, DEFAULT_PROTECTED_PATHS, DEFAULT_MAX_TASK_BUDGET_USD, DEFAULT_TEAM_RULES, two tests
crates/core/src/governor/readiness.rs                creates: ReadinessRule, JudgmentReview, ParentState, ReadinessContext, ReadinessFailure, evaluate_readiness, the nineteen checks, twenty-five tests
crates/core/src/governor/readiness/fixtures.rs       creates: a_contract, a_ready_context
docs/SPEC.md                                         modifies: 5.12 says the budget cap is on a task and names its default
docs/plans/project-plan.md                           modifies: phase 1 step 02 interface drops RiskSet, with the reason, and lists the two default constants (in the plan's own commits)
docs/plans/phase-1-harness/step-02-definition-of-ready-and-team-rules.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: Team rules and their defaults

Files: created `crates/core/src/governor/team_rules.rs`; modified `crates/core/src/governor.rs`, `docs/SPEC.md`

Consumes: nothing
Produces: `governor::team_rules::{TeamRules, DEFAULT_PROTECTED_PATHS, DEFAULT_MAX_TASK_BUDGET_USD, DEFAULT_TEAM_RULES}`

- [x] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// The lifecycle's statuses and which of them are terminal.
  pub mod task_status;
  /// Team rules of `docs/SPEC.md` section 5.12 and their defaults.
  pub mod team_rules;
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```
- [x] Write the failing tests. `crates/core/src/governor/team_rules.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::{DEFAULT_TEAM_RULES, TeamRules};

      #[test]
      fn protects_the_secret_paths_and_caps_a_task_at_five_dollars_by_default() {
          let rules = TeamRules::default();
          assert_eq!(
              rules.protected_paths,
              [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"]
          );
          assert_eq!(rules.max_task_budget_usd, Some(5.0));
      }

      #[test]
      fn leaves_every_other_rule_empty_or_off_by_default() {
          let rules = TeamRules::default();
          assert!(rules.allowed_paths_ceiling.is_empty());
          assert!(rules.required_criteria.is_empty());
          assert!(!rules.require_new_tests);
          assert!(rules.forbidden_commands.is_empty());
          assert_eq!(*DEFAULT_TEAM_RULES, rules);
      }
  }
  ```
- [x] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::team_rules
  # expected, among the output:
  # error[E0432]: unresolved imports `super::DEFAULT_TEAM_RULES`, `super::TeamRules`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [x] Write the implementation above the tests. `crates/core/src/governor/team_rules.rs` in full:

  ```rust
  use std::sync::LazyLock;

  /// The protected paths every team starts with (`docs/SPEC.md` section 5.6): secrets that no
  /// tool may read or write.
  pub const DEFAULT_PROTECTED_PATHS: [&str; 5] =
      [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"];

  /// The cap on one task's budget in dollars unless the human raises it (project plan D3).
  pub const DEFAULT_MAX_TASK_BUDGET_USD: f64 = 5.0;

  /// Constraints the human writes once in `.farik/team.yaml` under `rules`, applied by the governor
  /// to every contract and every tool call (`docs/SPEC.md` section 5.12). Rules never loosen a
  /// permission tier; they only narrow what a granted tier allows.
  #[derive(Debug, Clone, PartialEq)]
  pub struct TeamRules {
      /// Globs no tool may read or write, whatever its tier.
      pub protected_paths: Vec<String>,
      /// Globs a contract's `allowed_paths` must fall within; empty means no ceiling.
      pub allowed_paths_ceiling: Vec<String>,
      /// Verification methods every contract must have at least one criterion of.
      pub required_criteria: Vec<String>,
      /// Whether every `test` criterion must set `new_tests_required`.
      pub require_new_tests: bool,
      /// The most a contract's `max_cost_usd` may be; `None` means no cap.
      pub max_task_budget_usd: Option<f64>,
      /// Regular expressions a command must not match.
      pub forbidden_commands: Vec<String>,
  }

  impl Default for TeamRules {
      fn default() -> Self {
          Self {
              protected_paths: DEFAULT_PROTECTED_PATHS
                  .iter()
                  .map(|path| (*path).to_string())
                  .collect(),
              allowed_paths_ceiling: Vec::new(),
              required_criteria: Vec::new(),
              require_new_tests: false,
              max_task_budget_usd: Some(DEFAULT_MAX_TASK_BUDGET_USD),
              forbidden_commands: Vec::new(),
          }
      }
  }

  /// The default team rules, built once.
  pub static DEFAULT_TEAM_RULES: LazyLock<TeamRules> = LazyLock::new(TeamRules::default);

  #[cfg(test)]
  mod tests {
      use super::{DEFAULT_TEAM_RULES, TeamRules};

      #[test]
      fn protects_the_secret_paths_and_caps_a_task_at_five_dollars_by_default() {
          let rules = TeamRules::default();
          assert_eq!(
              rules.protected_paths,
              [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"]
          );
          assert_eq!(rules.max_task_budget_usd, Some(5.0));
      }

      #[test]
      fn leaves_every_other_rule_empty_or_off_by_default() {
          let rules = TeamRules::default();
          assert!(rules.allowed_paths_ceiling.is_empty());
          assert!(rules.required_criteria.is_empty());
          assert!(!rules.require_new_tests);
          assert!(rules.forbidden_commands.is_empty());
          assert_eq!(*DEFAULT_TEAM_RULES, rules);
      }
  }
  ```
- [x] In `docs/SPEC.md` section 5.12, replace the table row

  ```
  | `max_task_budget_usd` | number | Definition of Ready refuses a contract whose budget exceeds it |
  ```

  with

  ```
  | `max_task_budget_usd` | number | Definition of Ready refuses a task whose budget exceeds it; an epic is bounded by the sprint budget instead |
  ```

  and the sentence

  ```
  Defaults: `protected_paths` as in 5.6, everything else empty or off.
  ```

  with

  ```
  Defaults: `protected_paths` as in 5.6, `max_task_budget_usd` 5 dollars (the human raises it in `team.yaml`), everything else empty or off.
  ```

- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::team_rules
  # expected, among the output:
  # test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `feat(core): add team rules with the spec's defaults`

### Task 2: The Definition of Ready

Files: created `crates/core/src/governor/readiness.rs`, `crates/core/src/governor/readiness/fixtures.rs`; modified `crates/core/src/governor.rs`

Consumes: `governor::team_rules::TeamRules` from Task 1; `contract::{Role, TaskContract, TaskStatus, Verification, VerificationWire, validate_contract}`, `contract::fixtures::a_contract_wire`, and `generated::task_contract::FarikTaskContractKind` from `main`
Produces: `governor::readiness::{ReadinessRule, JudgmentReview, ParentState, ReadinessContext, ReadinessFailure, evaluate_readiness}`; `governor::readiness::fixtures::{a_contract, a_ready_context}`

- [x] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

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
- [x] Write the fixtures. `crates/core/src/governor/readiness/fixtures.rs` in full:

  ```rust
  use std::collections::BTreeMap;
  use std::sync::LazyLock;

  use super::ReadinessContext;
  use crate::contract::fixtures::a_contract_wire;
  use crate::contract::{Role, TaskContract, validate_contract};
  use crate::governor::team_rules::TeamRules;

  static A_CONTRACT: LazyLock<TaskContract> = LazyLock::new(|| {
      validate_contract(&a_contract_wire())
          .expect("the wire fixture is schema-valid: contract::tests pins it")
  });

  /// The minimal valid contract of `contract::fixtures::a_contract_wire`, typed: a Software
  /// Developer's task reviewed by the Architect, one `test` criterion, a five-dollar budget.
  #[must_use]
  pub fn a_contract() -> TaskContract {
      A_CONTRACT.clone()
  }

  /// A context in which `a_contract()` is ready: a sprint with a hundred dollars left, one active
  /// agent of every launch role, the default team rules, no parent, and no judgment review
  /// required.
  #[must_use]
  pub fn a_ready_context() -> ReadinessContext {
      ReadinessContext {
          remaining_sprint_budget_usd: 100.0,
          dependency_statuses: BTreeMap::new(),
          active_agents_by_role: [
              (Role::ProductManager, 1),
              (Role::ScrumMaster, 1),
              (Role::Architect, 1),
              (Role::SoftwareDeveloper, 1),
              (Role::MarketingSpecialist, 1),
          ]
          .into_iter()
          .collect(),
          parent: None,
          rules: TeamRules::default(),
          requires_judgment_review: false,
          judgment_review: None,
      }
  }
  ```
- [x] Write the failing tests. `crates/core/src/governor/readiness.rs` holds the module doc, the imports, the fixtures declaration, the rule enum, the context and failure types, and the tests, but not `evaluate_readiness` or the checks:

  ```rust
  //! The Definition of Ready (`docs/SPEC.md` section 5.3), the team rules it applies (5.12), and
  //! the parent rules of an epic's tasks (5.16), as one function over a contract and a context.

  use std::collections::{BTreeMap, BTreeSet};

  use super::team_rules::TeamRules;
  use crate::contract::{Role, TaskContract, TaskStatus, Verification, VerificationWire};
  use crate::generated::task_contract::FarikTaskContractKind as Kind;

  /// Builders for readiness contexts and typed contracts, usable by every crate's tests.
  pub mod fixtures;

  /// One rule of the Definition of Ready. Failures are reported in this order.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum ReadinessRule {
      /// The intent has words in it, not only whitespace.
      IntentPresent,
      /// At least one exit criterion exists.
      CriteriaPresent,
      /// Every criterion's `method` value names the shape its fields have.
      CriteriaMethodsValid,
      /// Every `command` and `test` criterion has a command that is not blank.
      CommandCriteriaComplete,
      /// The budget does not exceed the remaining sprint budget.
      BudgetWithinSprint,
      /// Someone other than the assignee can review: the human always can; otherwise one active
      /// agent of the reviewer role, or two when it is the assignee's role.
      ReviewerAvailable,
      /// Scope names at least one `out_of_scope` item that is not blank.
      OutOfScopePresent,
      /// Every listed dependency exists and is at least `ready`.
      DependenciesReady,
      /// The contract has a criterion of every method the team rules require.
      RequiredCriteriaPresent,
      /// Every `test` criterion sets `new_tests_required` when the team rules require new tests.
      NewTestsRequiredByRule,
      /// Every allowed path falls within the team's ceiling.
      AllowedPathsWithinCeiling,
      /// A task's budget does not exceed the team's cap on a task; an epic is bounded by the
      /// sprint budget instead.
      BudgetWithinTeamMax,
      /// An epic has no parent.
      NoParentForEpic,
      /// A task's parent is known and `in_progress`.
      ParentInProgress,
      /// A task's allowed paths fall within its parent's.
      PathsWithinParent,
      /// A task's budget fits in its parent's remaining budget.
      BudgetWithinParent,
      /// The Scrum Master's judgment review is recorded when the team has one.
      JudgmentRecorded,
      /// The Scrum Master judged that the task fits its budget.
      JudgmentFitsBudget,
      /// The Scrum Master judged that the criteria would detect the failure the intent worries
      /// about.
      JudgmentCriteriaDetectFailure,
  }

  /// The Scrum Master's judgment (`docs/SPEC.md` section 5.3), recorded by the runtime as a
  /// `review.recorded` event and passed in.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct JudgmentReview {
      /// The task is small enough to finish within its budget.
      pub fits_budget: bool,
      /// The criteria would detect the failure the intent worries about, not just that something
      /// ran.
      pub criteria_detect_failure: bool,
      /// The written reason for both answers.
      pub reason: String,
  }

  /// What the readiness check needs to know about a task's epic (`docs/SPEC.md` section 5.16).
  #[derive(Debug, Clone, PartialEq)]
  pub struct ParentState {
      /// The epic's status; a task is ready only while its epic is `in_progress`.
      pub status: TaskStatus,
      /// The epic's allowed paths, which the task's must fall within.
      pub allowed_paths: Vec<String>,
      /// What is left of the epic's budget for its tasks, in dollars.
      pub remaining_budget_usd: f64,
  }

  /// Everything the readiness check needs from the world, passed in by the runtime.
  #[derive(Debug, Clone, PartialEq)]
  pub struct ReadinessContext {
      /// What is left of the sprint's budget, in dollars.
      pub remaining_sprint_budget_usd: f64,
      /// The status of every task the contract may list as a dependency, by task id.
      pub dependency_statuses: BTreeMap<String, TaskStatus>,
      /// How many active agents the team has of each role.
      pub active_agents_by_role: BTreeMap<Role, u32>,
      /// The contract's epic, when it lists a parent the runtime knows.
      pub parent: Option<ParentState>,
      /// The team rules in force.
      pub rules: TeamRules,
      /// Whether the team has an active Scrum Master, whose judgment review is then required.
      pub requires_judgment_review: bool,
      /// The recorded judgment review, when there is one.
      pub judgment_review: Option<JudgmentReview>,
  }

  /// One rule the contract fails, with a message that says what to change.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ReadinessFailure {
      /// The rule that failed.
      pub rule: ReadinessRule,
      /// What is wrong and what would fix it.
      pub message: String,
  }

  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::fixtures::{a_contract, a_ready_context};
      use super::{
          JudgmentReview, ParentState, ReadinessContext, ReadinessRule as R, evaluate_readiness,
      };
      use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;

      fn failed_rules(contract: &TaskContract, context: &ReadinessContext) -> Vec<R> {
          evaluate_readiness(contract, context)
              .expect_err("expected a refusal")
              .iter()
              .map(|failure| failure.rule)
              .collect()
      }

      fn message_of(contract: &TaskContract, context: &ReadinessContext, rule: R) -> String {
          evaluate_readiness(contract, context)
              .expect_err("expected a refusal")
              .into_iter()
              .find(|failure| failure.rule == rule)
              .map(|failure| failure.message)
              .expect("the rule failed")
      }

      fn a_review(fits_budget: bool, criteria_detect_failure: bool) -> JudgmentReview {
          JudgmentReview {
              fits_budget,
              criteria_detect_failure,
              reason: "Two files, one form.".to_string(),
          }
      }

      fn a_task_under(parent: ParentState) -> (TaskContract, ReadinessContext) {
          let mut contract = a_contract();
          contract.parent = Some("FRK-3".parse().expect("a task id"));
          let mut context = a_ready_context();
          context.parent = Some(parent);
          (contract, context)
      }

      fn an_in_progress_parent() -> ParentState {
          ParentState {
              status: TaskStatus::InProgress,
              allowed_paths: vec!["src/**".to_string()],
              remaining_budget_usd: 50.0,
          }
      }

      #[test]
      fn accepts_the_fixture_contract_in_the_fixture_context() {
          assert_eq!(
              evaluate_readiness(&a_contract(), &a_ready_context()),
              Ok(())
          );
      }

      #[test]
      fn refuses_a_blank_intent() {
          let mut contract = a_contract();
          contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::IntentPresent]
          );
      }

      #[test]
      fn refuses_a_contract_without_exit_criteria() {
          let mut contract = a_contract();
          contract.exit_criteria.clear();
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CriteriaPresent]
          );
      }

      #[test]
      fn refuses_a_criterion_whose_method_does_not_match_its_fields() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("review"),
              question: "Did you sign in?".to_string(),
          };
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CriteriaMethodsValid]
          );
      }

      #[test]
      fn refuses_a_test_criterion_with_a_blank_command() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant1 {
              command: "   ".to_string(),
              method: json!("test"),
              new_tests_required: false,
          };
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CommandCriteriaComplete]
          );
      }

      #[test]
      fn refuses_a_budget_above_the_remaining_sprint_budget() {
          let mut context = a_ready_context();
          context.remaining_sprint_budget_usd = 4.0;
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::BudgetWithinSprint]
          );
      }

      #[test]
      fn refuses_a_contract_nobody_can_review_and_names_the_role_to_add() {
          let mut context = a_ready_context();
          context.active_agents_by_role.remove(&Role::Architect);
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::ReviewerAvailable]
          );
          assert!(
              message_of(&a_contract(), &context, R::ReviewerAvailable)
                  .ends_with("add an agent with role architect")
          );
      }

      #[test]
      fn accepts_the_human_as_reviewer_without_counting_agents() {
          let mut contract = a_contract();
          contract.reviewer_role = Role::Human;
          let mut context = a_ready_context();
          context.active_agents_by_role.clear();
          context
              .active_agents_by_role
              .insert(Role::SoftwareDeveloper, 1);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn needs_two_agents_when_the_reviewer_role_is_the_assignee_role() {
          let mut contract = a_contract();
          contract.reviewer_role = Role::SoftwareDeveloper;
          let mut context = a_ready_context();
          assert_eq!(failed_rules(&contract, &context), [R::ReviewerAvailable]);
          context
              .active_agents_by_role
              .insert(Role::SoftwareDeveloper, 2);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_scope_whose_out_of_scope_items_are_blank() {
          let mut contract = a_contract();
          contract.scope.out_of_scope = vec![" ".to_string()];
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::OutOfScopePresent]
          );
      }

      #[test]
      fn refuses_a_dependency_that_is_unknown_or_not_yet_ready() {
          let mut contract = a_contract();
          contract.dependencies = vec!["FRK-2".parse().expect("a task id")];
          let mut context = a_ready_context();
          assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
          context
              .dependency_statuses
              .insert("FRK-2".to_string(), TaskStatus::Refining);
          assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
          context
              .dependency_statuses
              .insert("FRK-2".to_string(), TaskStatus::Ready);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_contract_missing_a_criterion_method_the_team_requires() {
          let mut context = a_ready_context();
          context.rules.required_criteria = vec!["review".to_string()];
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::RequiredCriteriaPresent]
          );
      }

      #[test]
      fn refuses_a_test_criterion_without_new_tests_when_the_team_requires_them() {
          let mut context = a_ready_context();
          context.rules.require_new_tests = true;
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::NewTestsRequiredByRule]
          );
      }

      #[test]
      fn refuses_allowed_paths_outside_the_team_ceiling() {
          let mut context = a_ready_context();
          context.rules.allowed_paths_ceiling = vec!["docs/**".to_string()];
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::AllowedPathsWithinCeiling]
          );
          context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
          context.rules.allowed_paths_ceiling = vec!["src2/**".to_string(), "**".to_string()];
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn keeps_a_ceiling_with_a_file_filter_exact() {
          let mut contract = a_contract();
          contract.allowed_paths = vec!["docs/**".to_string()];
          let mut context = a_ready_context();
          context.rules.allowed_paths_ceiling = vec!["docs/**/*.md".to_string()];
          assert_eq!(
              failed_rules(&contract, &context),
              [R::AllowedPathsWithinCeiling]
          );
          contract.allowed_paths = vec!["docs/**/*.md".to_string()];
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_budget_above_the_team_cap_and_accepts_any_budget_without_one() {
          let mut context = a_ready_context();
          context.rules.max_task_budget_usd = Some(4.0);
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::BudgetWithinTeamMax]
          );
          context.rules.max_task_budget_usd = None;
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn does_not_cap_an_epic_at_the_team_maximum_for_a_task() {
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          epic.reviewer_role = Role::Human;
          epic.budget.max_cost_usd = 40.0;
          assert_eq!(evaluate_readiness(&epic, &a_ready_context()), Ok(()));
      }

      #[test]
      fn refuses_an_epic_with_a_parent() {
          let (mut contract, context) = a_task_under(an_in_progress_parent());
          contract.kind = Kind::Epic;
          assert_eq!(failed_rules(&contract, &context), [R::NoParentForEpic]);
      }

      #[test]
      fn refuses_a_task_whose_parent_is_unknown_or_not_in_progress() {
          let (contract, mut context) = a_task_under(an_in_progress_parent());
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
          context.parent = None;
          assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
          context.parent = Some(ParentState {
              status: TaskStatus::Ready,
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
      }

      #[test]
      fn refuses_a_task_whose_paths_reach_outside_its_parent() {
          let (contract, context) = a_task_under(ParentState {
              allowed_paths: vec!["docs/**".to_string()],
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::PathsWithinParent]);
      }

      #[test]
      fn refuses_a_task_whose_budget_exceeds_what_its_parent_has_left() {
          let (contract, context) = a_task_under(ParentState {
              remaining_budget_usd: 4.0,
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::BudgetWithinParent]);
      }

      #[test]
      fn requires_the_judgment_review_only_when_the_team_has_a_scrum_master() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          assert_eq!(failed_rules(&a_contract(), &context), [R::JudgmentRecorded]);
          context.judgment_review = Some(a_review(true, true));
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
          context.requires_judgment_review = false;
          context.judgment_review = None;
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn refuses_a_judgment_that_the_task_does_not_fit_its_budget() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.judgment_review = Some(a_review(false, true));
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::JudgmentFitsBudget]
          );
      }

      #[test]
      fn refuses_a_judgment_that_the_criteria_would_not_detect_the_failure() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.judgment_review = Some(a_review(true, false));
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::JudgmentCriteriaDetectFailure]
          );
      }

      #[test]
      fn reports_every_failure_in_rule_order() {
          let mut contract = a_contract();
          contract.exit_criteria.clear();
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.remaining_sprint_budget_usd = 1.0;
          context.rules.required_criteria = vec!["human".to_string()];
          assert_eq!(
              failed_rules(&contract, &context),
              [
                  R::CriteriaPresent,
                  R::BudgetWithinSprint,
                  R::RequiredCriteriaPresent,
                  R::JudgmentRecorded
              ]
          );
      }
  }
  ```
- [x] Run them and confirm they fail because the function is missing (the three unused-import warnings are the imports the checks will use):

  ```
  cargo test --package farik-core governor::readiness
  # expected, among the output:
  # error[E0432]: unresolved import `super::evaluate_readiness`
  # warning: unused import: `BTreeSet`
  # warning: unused imports: `TaskContract`, `VerificationWire`, and `Verification`
  # warning: unused import: `crate::generated::task_contract::FarikTaskContractKind as Kind`
  # error: could not compile `farik-core` (lib test) due to 1 previous error; 3 warnings emitted
  ```

- [x] Write the checks and `evaluate_readiness` between the types and the tests. `crates/core/src/governor/readiness.rs` in full:

  ```rust
  //! The Definition of Ready (`docs/SPEC.md` section 5.3), the team rules it applies (5.12), and
  //! the parent rules of an epic's tasks (5.16), as one function over a contract and a context.

  use std::collections::{BTreeMap, BTreeSet};

  use super::team_rules::TeamRules;
  use crate::contract::{Role, TaskContract, TaskStatus, Verification, VerificationWire};
  use crate::generated::task_contract::FarikTaskContractKind as Kind;

  /// Builders for readiness contexts and typed contracts, usable by every crate's tests.
  pub mod fixtures;

  /// One rule of the Definition of Ready. Failures are reported in this order.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum ReadinessRule {
      /// The intent has words in it, not only whitespace.
      IntentPresent,
      /// At least one exit criterion exists.
      CriteriaPresent,
      /// Every criterion's `method` value names the shape its fields have.
      CriteriaMethodsValid,
      /// Every `command` and `test` criterion has a command that is not blank.
      CommandCriteriaComplete,
      /// The budget does not exceed the remaining sprint budget.
      BudgetWithinSprint,
      /// Someone other than the assignee can review: the human always can; otherwise one active
      /// agent of the reviewer role, or two when it is the assignee's role.
      ReviewerAvailable,
      /// Scope names at least one `out_of_scope` item that is not blank.
      OutOfScopePresent,
      /// Every listed dependency exists and is at least `ready`.
      DependenciesReady,
      /// The contract has a criterion of every method the team rules require.
      RequiredCriteriaPresent,
      /// Every `test` criterion sets `new_tests_required` when the team rules require new tests.
      NewTestsRequiredByRule,
      /// Every allowed path falls within the team's ceiling.
      AllowedPathsWithinCeiling,
      /// A task's budget does not exceed the team's cap on a task; an epic is bounded by the
      /// sprint budget instead.
      BudgetWithinTeamMax,
      /// An epic has no parent.
      NoParentForEpic,
      /// A task's parent is known and `in_progress`.
      ParentInProgress,
      /// A task's allowed paths fall within its parent's.
      PathsWithinParent,
      /// A task's budget fits in its parent's remaining budget.
      BudgetWithinParent,
      /// The Scrum Master's judgment review is recorded when the team has one.
      JudgmentRecorded,
      /// The Scrum Master judged that the task fits its budget.
      JudgmentFitsBudget,
      /// The Scrum Master judged that the criteria would detect the failure the intent worries
      /// about.
      JudgmentCriteriaDetectFailure,
  }

  /// The Scrum Master's judgment (`docs/SPEC.md` section 5.3), recorded by the runtime as a
  /// `review.recorded` event and passed in.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct JudgmentReview {
      /// The task is small enough to finish within its budget.
      pub fits_budget: bool,
      /// The criteria would detect the failure the intent worries about, not just that something
      /// ran.
      pub criteria_detect_failure: bool,
      /// The written reason for both answers.
      pub reason: String,
  }

  /// What the readiness check needs to know about a task's epic (`docs/SPEC.md` section 5.16).
  #[derive(Debug, Clone, PartialEq)]
  pub struct ParentState {
      /// The epic's status; a task is ready only while its epic is `in_progress`.
      pub status: TaskStatus,
      /// The epic's allowed paths, which the task's must fall within.
      pub allowed_paths: Vec<String>,
      /// What is left of the epic's budget for its tasks, in dollars.
      pub remaining_budget_usd: f64,
  }

  /// Everything the readiness check needs from the world, passed in by the runtime.
  #[derive(Debug, Clone, PartialEq)]
  pub struct ReadinessContext {
      /// What is left of the sprint's budget, in dollars.
      pub remaining_sprint_budget_usd: f64,
      /// The status of every task the contract may list as a dependency, by task id.
      pub dependency_statuses: BTreeMap<String, TaskStatus>,
      /// How many active agents the team has of each role.
      pub active_agents_by_role: BTreeMap<Role, u32>,
      /// The contract's epic, when it lists a parent the runtime knows.
      pub parent: Option<ParentState>,
      /// The team rules in force.
      pub rules: TeamRules,
      /// Whether the team has an active Scrum Master, whose judgment review is then required.
      pub requires_judgment_review: bool,
      /// The recorded judgment review, when there is one.
      pub judgment_review: Option<JudgmentReview>,
  }

  /// One rule the contract fails, with a message that says what to change.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ReadinessFailure {
      /// The rule that failed.
      pub rule: ReadinessRule,
      /// What is wrong and what would fix it.
      pub message: String,
  }

  type Check = fn(&TaskContract, &ReadinessContext) -> Option<ReadinessFailure>;

  const CHECKS: [Check; 19] = [
      intent_present,
      criteria_present,
      criteria_methods_valid,
      command_criteria_complete,
      budget_within_sprint,
      reviewer_available,
      out_of_scope_present,
      dependencies_ready,
      required_criteria_present,
      new_tests_required_by_rule,
      allowed_paths_within_ceiling,
      budget_within_team_max,
      no_parent_for_epic,
      parent_in_progress,
      paths_within_parent,
      budget_within_parent,
      judgment_recorded,
      judgment_fits_budget,
      judgment_criteria_detect_failure,
  ];

  /// Checks a contract against the Definition of Ready: the structural rules of `docs/SPEC.md`
  /// section 5.3, the team rules of 5.12, the parent rules of 5.16, and the Scrum Master's
  /// recorded judgment when the team has one. Refuses with every rule the contract fails.
  ///
  /// # Errors
  ///
  /// Every failed rule, in the order of `ReadinessRule`, each with a message that says what to
  /// change.
  pub fn evaluate_readiness(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Result<(), Vec<ReadinessFailure>> {
      let failures: Vec<ReadinessFailure> = CHECKS
          .iter()
          .filter_map(|check| check(contract, context))
          .collect();
      if failures.is_empty() {
          Ok(())
      } else {
          Err(failures)
      }
  }

  fn failure(rule: ReadinessRule, message: String) -> ReadinessFailure {
      ReadinessFailure { rule, message }
  }

  fn criterion_ids<'a>(
      contract: &'a TaskContract,
      mut keep: impl FnMut(&Verification, &VerificationWire) -> bool + 'a,
  ) -> Vec<String> {
      contract
          .exit_criteria
          .iter()
          .filter(|criterion| {
              keep(
                  &Verification::from(&criterion.verification),
                  &criterion.verification,
              )
          })
          .map(|criterion| criterion.id.to_string())
          .collect()
  }

  fn wire_method(wire: &VerificationWire) -> Option<&str> {
      match wire {
          VerificationWire::Variant0 { method, .. }
          | VerificationWire::Variant1 { method, .. }
          | VerificationWire::Variant2 { method, .. }
          | VerificationWire::Variant3 { method, .. }
          | VerificationWire::Variant4 { method, .. } => method.as_str(),
      }
  }

  fn intent_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
      if contract.intent.trim().is_empty() {
          return Some(failure(
              ReadinessRule::IntentPresent,
              "the intent is blank; state the user-facing reason for the task".to_string(),
          ));
      }
      None
  }

  fn criteria_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
      if contract.exit_criteria.is_empty() {
          return Some(failure(
              ReadinessRule::CriteriaPresent,
              "there is no exit criterion; add at least one".to_string(),
          ));
      }
      None
  }

  fn criteria_methods_valid(
      contract: &TaskContract,
      _: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      let bad = criterion_ids(contract, |verification, wire| {
          wire_method(wire) != Some(verification.method())
      });
      if bad.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::CriteriaMethodsValid,
          format!(
              "criteria {} name a verification method that does not match their fields",
              bad.join(", ")
          ),
      ))
  }

  fn command_criteria_complete(
      contract: &TaskContract,
      _: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      let bad = criterion_ids(contract, |verification, _| match verification {
          Verification::Command { command, .. } | Verification::Test { command, .. } => {
              command.trim().is_empty()
          }
          _ => false,
      });
      if bad.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::CommandCriteriaComplete,
          format!("criteria {} have a blank command", bad.join(", ")),
      ))
  }

  fn budget_within_sprint(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if contract.budget.max_cost_usd > context.remaining_sprint_budget_usd {
          return Some(failure(
              ReadinessRule::BudgetWithinSprint,
              format!(
                  "the budget of {} USD exceeds the {} USD left in the sprint",
                  contract.budget.max_cost_usd, context.remaining_sprint_budget_usd
              ),
          ));
      }
      None
  }

  fn reviewer_available(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if contract.reviewer_role == Role::Human {
          return None;
      }
      let needed = if contract.reviewer_role == contract.assignee_role {
          2
      } else {
          1
      };
      let active = context
          .active_agents_by_role
          .get(&contract.reviewer_role)
          .copied()
          .unwrap_or(0);
      if active < needed {
          return Some(failure(
              ReadinessRule::ReviewerAvailable,
              format!(
                  "no agent can review: the team needs {needed} active agent(s) with role {} and has {active}; add an agent with role {}",
                  contract.reviewer_role, contract.reviewer_role
              ),
          ));
      }
      None
  }

  fn out_of_scope_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
      if !contract
          .scope
          .out_of_scope
          .iter()
          .any(|item| !item.trim().is_empty())
      {
          return Some(failure(
              ReadinessRule::OutOfScopePresent,
              "scope names no out_of_scope item; say where the task stops".to_string(),
          ));
      }
      None
  }

  fn is_at_least_ready(status: TaskStatus) -> bool {
      matches!(
          status,
          TaskStatus::Ready
              | TaskStatus::Assigned
              | TaskStatus::InProgress
              | TaskStatus::Blocked
              | TaskStatus::Verifying
              | TaskStatus::Rejected
              | TaskStatus::Accepted
      )
  }

  fn dependencies_ready(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      let not_ready: Vec<&str> = contract
          .dependencies
          .iter()
          .map(|dependency| dependency.as_str())
          .filter(|dependency| {
              !context
                  .dependency_statuses
                  .get(*dependency)
                  .is_some_and(|status| is_at_least_ready(*status))
          })
          .collect();
      if not_ready.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::DependenciesReady,
          format!(
              "dependencies {} do not exist or are not yet ready",
              not_ready.join(", ")
          ),
      ))
  }

  fn required_criteria_present(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      let methods: BTreeSet<&str> = contract
          .exit_criteria
          .iter()
          .map(|criterion| Verification::from(&criterion.verification).method())
          .collect();
      let missing: Vec<&str> = context
          .rules
          .required_criteria
          .iter()
          .map(String::as_str)
          .filter(|method| !methods.contains(method))
          .collect();
      if missing.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::RequiredCriteriaPresent,
          format!(
              "the team rules require a criterion with method {} and the contract has none",
              missing.join(", ")
          ),
      ))
  }

  fn new_tests_required_by_rule(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if !context.rules.require_new_tests {
          return None;
      }
      let bad = criterion_ids(contract, |verification, _| {
          matches!(
              verification,
              Verification::Test {
                  new_tests_required: false,
                  ..
              }
          )
      });
      if bad.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::NewTestsRequiredByRule,
          format!(
              "the team rules require new tests; test criteria {} do not set new_tests_required",
              bad.join(", ")
          ),
      ))
  }

  /// Splits a glob at its first `*`, `?`, `[`, or `{` into its literal prefix and the rest.
  fn split_glob(pattern: &str) -> (&str, &str) {
      let end = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
      pattern.split_at(end)
  }

  /// Whether a path glob stays under one of the ceiling globs. A ceiling that is a directory
  /// (`src`, `src/`, `src/**`, or `**`) admits every path whose literal prefix is that directory
  /// or below it: `src/login/**` is within `src/**`, `src2/**` is not, and `**` admits everything.
  /// Any other ceiling (`docs/**/*.md`, `src/*.rs`) admits only a path written exactly like it, so
  /// that a file filter is never widened. Paths are compared as written: `./src/**` is not
  /// `src/**`.
  fn is_within_any(path: &str, ceilings: &[String]) -> bool {
      ceilings.iter().any(|ceiling| {
          let (prefix, rest) = split_glob(ceiling);
          if !matches!(rest, "" | "**") {
              return path == ceiling;
          }
          let directory = prefix.trim_end_matches('/');
          let path = split_glob(path).0.trim_end_matches('/');
          directory.is_empty() || path == directory || path.starts_with(&format!("{directory}/"))
      })
  }

  fn paths_outside<'a>(paths: &'a [String], ceilings: &[String]) -> Vec<&'a str> {
      paths
          .iter()
          .map(String::as_str)
          .filter(|path| !is_within_any(path, ceilings))
          .collect()
  }

  fn allowed_paths_within_ceiling(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if context.rules.allowed_paths_ceiling.is_empty() {
          return None;
      }
      let outside = paths_outside(
          &contract.allowed_paths,
          &context.rules.allowed_paths_ceiling,
      );
      if outside.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::AllowedPathsWithinCeiling,
          format!(
              "allowed paths {} reach outside the team's ceiling",
              outside.join(", ")
          ),
      ))
  }

  fn budget_within_team_max(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if contract.kind == Kind::Task
          && let Some(max) = context.rules.max_task_budget_usd
          && contract.budget.max_cost_usd > max
      {
          return Some(failure(
              ReadinessRule::BudgetWithinTeamMax,
              format!(
                  "the budget of {} USD exceeds the team's cap of {max} USD on a task",
                  contract.budget.max_cost_usd
              ),
          ));
      }
      None
  }

  fn no_parent_for_epic(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
      if contract.kind == Kind::Epic && contract.parent.is_some() {
          return Some(failure(
              ReadinessRule::NoParentForEpic,
              "an epic cannot have a parent".to_string(),
          ));
      }
      None
  }

  fn parent_in_progress(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      let Some(parent_id) = &contract.parent else {
          return None;
      };
      match &context.parent {
          None => Some(failure(
              ReadinessRule::ParentInProgress,
              format!("the parent {} is not a known task", parent_id.as_str()),
          )),
          Some(parent) if parent.status != TaskStatus::InProgress => Some(failure(
              ReadinessRule::ParentInProgress,
              format!(
                  "the parent {} is {}, not in_progress",
                  parent_id.as_str(),
                  parent.status
              ),
          )),
          Some(_) => None,
      }
  }

  fn paths_within_parent(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      contract.parent.as_ref()?;
      let Some(parent) = &context.parent else {
          return None;
      };
      let outside = paths_outside(&contract.allowed_paths, &parent.allowed_paths);
      if outside.is_empty() {
          return None;
      }
      Some(failure(
          ReadinessRule::PathsWithinParent,
          format!(
              "allowed paths {} reach outside the parent's allowed paths",
              outside.join(", ")
          ),
      ))
  }

  fn budget_within_parent(
      contract: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      contract.parent.as_ref()?;
      let Some(parent) = &context.parent else {
          return None;
      };
      if contract.budget.max_cost_usd > parent.remaining_budget_usd {
          return Some(failure(
              ReadinessRule::BudgetWithinParent,
              format!(
                  "the budget of {} USD exceeds the {} USD left in the parent's budget",
                  contract.budget.max_cost_usd, parent.remaining_budget_usd
              ),
          ));
      }
      None
  }

  fn judgment_recorded(_: &TaskContract, context: &ReadinessContext) -> Option<ReadinessFailure> {
      if context.requires_judgment_review && context.judgment_review.is_none() {
          return Some(failure(
              ReadinessRule::JudgmentRecorded,
              "the Scrum Master's judgment review is not recorded".to_string(),
          ));
      }
      None
  }

  fn judgment_fits_budget(_: &TaskContract, context: &ReadinessContext) -> Option<ReadinessFailure> {
      if context.requires_judgment_review
          && let Some(review) = &context.judgment_review
          && !review.fits_budget
      {
          return Some(failure(
              ReadinessRule::JudgmentFitsBudget,
              format!(
                  "the Scrum Master judged the task too large for its budget: {}",
                  review.reason
              ),
          ));
      }
      None
  }

  fn judgment_criteria_detect_failure(
      _: &TaskContract,
      context: &ReadinessContext,
  ) -> Option<ReadinessFailure> {
      if context.requires_judgment_review
          && let Some(review) = &context.judgment_review
          && !review.criteria_detect_failure
      {
          return Some(failure(
              ReadinessRule::JudgmentCriteriaDetectFailure,
              format!(
                  "the Scrum Master judged that the criteria would not detect the failure the intent worries about: {}",
                  review.reason
              ),
          ));
      }
      None
  }

  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::fixtures::{a_contract, a_ready_context};
      use super::{
          JudgmentReview, ParentState, ReadinessContext, ReadinessRule as R, evaluate_readiness,
      };
      use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
      use crate::generated::task_contract::FarikTaskContractKind as Kind;

      fn failed_rules(contract: &TaskContract, context: &ReadinessContext) -> Vec<R> {
          evaluate_readiness(contract, context)
              .expect_err("expected a refusal")
              .iter()
              .map(|failure| failure.rule)
              .collect()
      }

      fn message_of(contract: &TaskContract, context: &ReadinessContext, rule: R) -> String {
          evaluate_readiness(contract, context)
              .expect_err("expected a refusal")
              .into_iter()
              .find(|failure| failure.rule == rule)
              .map(|failure| failure.message)
              .expect("the rule failed")
      }

      fn a_review(fits_budget: bool, criteria_detect_failure: bool) -> JudgmentReview {
          JudgmentReview {
              fits_budget,
              criteria_detect_failure,
              reason: "Two files, one form.".to_string(),
          }
      }

      fn a_task_under(parent: ParentState) -> (TaskContract, ReadinessContext) {
          let mut contract = a_contract();
          contract.parent = Some("FRK-3".parse().expect("a task id"));
          let mut context = a_ready_context();
          context.parent = Some(parent);
          (contract, context)
      }

      fn an_in_progress_parent() -> ParentState {
          ParentState {
              status: TaskStatus::InProgress,
              allowed_paths: vec!["src/**".to_string()],
              remaining_budget_usd: 50.0,
          }
      }

      #[test]
      fn accepts_the_fixture_contract_in_the_fixture_context() {
          assert_eq!(
              evaluate_readiness(&a_contract(), &a_ready_context()),
              Ok(())
          );
      }

      #[test]
      fn refuses_a_blank_intent() {
          let mut contract = a_contract();
          contract.intent = " "
              .repeat(24)
              .parse()
              .expect("twenty-four spaces pass the schema");
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::IntentPresent]
          );
      }

      #[test]
      fn refuses_a_contract_without_exit_criteria() {
          let mut contract = a_contract();
          contract.exit_criteria.clear();
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CriteriaPresent]
          );
      }

      #[test]
      fn refuses_a_criterion_whose_method_does_not_match_its_fields() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant4 {
              method: json!("review"),
              question: "Did you sign in?".to_string(),
          };
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CriteriaMethodsValid]
          );
      }

      #[test]
      fn refuses_a_test_criterion_with_a_blank_command() {
          let mut contract = a_contract();
          contract.exit_criteria[0].verification = VerificationWire::Variant1 {
              command: "   ".to_string(),
              method: json!("test"),
              new_tests_required: false,
          };
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::CommandCriteriaComplete]
          );
      }

      #[test]
      fn refuses_a_budget_above_the_remaining_sprint_budget() {
          let mut context = a_ready_context();
          context.remaining_sprint_budget_usd = 4.0;
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::BudgetWithinSprint]
          );
      }

      #[test]
      fn refuses_a_contract_nobody_can_review_and_names_the_role_to_add() {
          let mut context = a_ready_context();
          context.active_agents_by_role.remove(&Role::Architect);
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::ReviewerAvailable]
          );
          assert!(
              message_of(&a_contract(), &context, R::ReviewerAvailable)
                  .ends_with("add an agent with role architect")
          );
      }

      #[test]
      fn accepts_the_human_as_reviewer_without_counting_agents() {
          let mut contract = a_contract();
          contract.reviewer_role = Role::Human;
          let mut context = a_ready_context();
          context.active_agents_by_role.clear();
          context
              .active_agents_by_role
              .insert(Role::SoftwareDeveloper, 1);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn needs_two_agents_when_the_reviewer_role_is_the_assignee_role() {
          let mut contract = a_contract();
          contract.reviewer_role = Role::SoftwareDeveloper;
          let mut context = a_ready_context();
          assert_eq!(failed_rules(&contract, &context), [R::ReviewerAvailable]);
          context
              .active_agents_by_role
              .insert(Role::SoftwareDeveloper, 2);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_scope_whose_out_of_scope_items_are_blank() {
          let mut contract = a_contract();
          contract.scope.out_of_scope = vec![" ".to_string()];
          assert_eq!(
              failed_rules(&contract, &a_ready_context()),
              [R::OutOfScopePresent]
          );
      }

      #[test]
      fn refuses_a_dependency_that_is_unknown_or_not_yet_ready() {
          let mut contract = a_contract();
          contract.dependencies = vec!["FRK-2".parse().expect("a task id")];
          let mut context = a_ready_context();
          assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
          context
              .dependency_statuses
              .insert("FRK-2".to_string(), TaskStatus::Refining);
          assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
          context
              .dependency_statuses
              .insert("FRK-2".to_string(), TaskStatus::Ready);
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_contract_missing_a_criterion_method_the_team_requires() {
          let mut context = a_ready_context();
          context.rules.required_criteria = vec!["review".to_string()];
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::RequiredCriteriaPresent]
          );
      }

      #[test]
      fn refuses_a_test_criterion_without_new_tests_when_the_team_requires_them() {
          let mut context = a_ready_context();
          context.rules.require_new_tests = true;
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::NewTestsRequiredByRule]
          );
      }

      #[test]
      fn refuses_allowed_paths_outside_the_team_ceiling() {
          let mut context = a_ready_context();
          context.rules.allowed_paths_ceiling = vec!["docs/**".to_string()];
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::AllowedPathsWithinCeiling]
          );
          context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
          context.rules.allowed_paths_ceiling = vec!["src2/**".to_string(), "**".to_string()];
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn keeps_a_ceiling_with_a_file_filter_exact() {
          let mut contract = a_contract();
          contract.allowed_paths = vec!["docs/**".to_string()];
          let mut context = a_ready_context();
          context.rules.allowed_paths_ceiling = vec!["docs/**/*.md".to_string()];
          assert_eq!(
              failed_rules(&contract, &context),
              [R::AllowedPathsWithinCeiling]
          );
          contract.allowed_paths = vec!["docs/**/*.md".to_string()];
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
      }

      #[test]
      fn refuses_a_budget_above_the_team_cap_and_accepts_any_budget_without_one() {
          let mut context = a_ready_context();
          context.rules.max_task_budget_usd = Some(4.0);
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::BudgetWithinTeamMax]
          );
          context.rules.max_task_budget_usd = None;
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn does_not_cap_an_epic_at_the_team_maximum_for_a_task() {
          let mut epic = a_contract();
          epic.kind = Kind::Epic;
          epic.reviewer_role = Role::Human;
          epic.budget.max_cost_usd = 40.0;
          assert_eq!(evaluate_readiness(&epic, &a_ready_context()), Ok(()));
      }

      #[test]
      fn refuses_an_epic_with_a_parent() {
          let (mut contract, context) = a_task_under(an_in_progress_parent());
          contract.kind = Kind::Epic;
          assert_eq!(failed_rules(&contract, &context), [R::NoParentForEpic]);
      }

      #[test]
      fn refuses_a_task_whose_parent_is_unknown_or_not_in_progress() {
          let (contract, mut context) = a_task_under(an_in_progress_parent());
          assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
          context.parent = None;
          assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
          context.parent = Some(ParentState {
              status: TaskStatus::Ready,
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
      }

      #[test]
      fn refuses_a_task_whose_paths_reach_outside_its_parent() {
          let (contract, context) = a_task_under(ParentState {
              allowed_paths: vec!["docs/**".to_string()],
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::PathsWithinParent]);
      }

      #[test]
      fn refuses_a_task_whose_budget_exceeds_what_its_parent_has_left() {
          let (contract, context) = a_task_under(ParentState {
              remaining_budget_usd: 4.0,
              ..an_in_progress_parent()
          });
          assert_eq!(failed_rules(&contract, &context), [R::BudgetWithinParent]);
      }

      #[test]
      fn requires_the_judgment_review_only_when_the_team_has_a_scrum_master() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          assert_eq!(failed_rules(&a_contract(), &context), [R::JudgmentRecorded]);
          context.judgment_review = Some(a_review(true, true));
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
          context.requires_judgment_review = false;
          context.judgment_review = None;
          assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
      }

      #[test]
      fn refuses_a_judgment_that_the_task_does_not_fit_its_budget() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.judgment_review = Some(a_review(false, true));
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::JudgmentFitsBudget]
          );
      }

      #[test]
      fn refuses_a_judgment_that_the_criteria_would_not_detect_the_failure() {
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.judgment_review = Some(a_review(true, false));
          assert_eq!(
              failed_rules(&a_contract(), &context),
              [R::JudgmentCriteriaDetectFailure]
          );
      }

      #[test]
      fn reports_every_failure_in_rule_order() {
          let mut contract = a_contract();
          contract.exit_criteria.clear();
          let mut context = a_ready_context();
          context.requires_judgment_review = true;
          context.remaining_sprint_budget_usd = 1.0;
          context.rules.required_criteria = vec!["human".to_string()];
          assert_eq!(
              failed_rules(&contract, &context),
              [
                  R::CriteriaPresent,
                  R::BudgetWithinSprint,
                  R::RequiredCriteriaPresent,
                  R::JudgmentRecorded
              ]
          );
      }
  }
  ```
- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::readiness
  # expected, among the output:
  # test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `feat(core): evaluate the definition of ready`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
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
# feat(core): evaluate the definition of ready
# feat(core): add team rules with the spec's defaults
```

## Open questions

none
