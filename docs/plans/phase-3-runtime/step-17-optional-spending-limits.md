# Phase 3, step 17: Optional spending limits

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 3, 4.1, 5.2, 5.3, 5.5, 5.12, 10; F5
Depends on: step 16 of this phase, a start gate: Task 1 does not begin until step 16's last commit is on this branch and step 11's landing fixes are on it. Task 1 edits step 15's `crates/cli/src/contract_new.rs`, Task 6 edits step 15's `crates/cli/src/start.rs`, Task 4 reads step 14's `TRIAGE_MODEL`, and Task 3 tests step 16's `Projections::metrics`. Also step 11 (`room`, the day's check before a session, commit 1340e5a); step 04 (`Transitions::context`); step 03 (`budget_state`, `record_session_cost`); phase 1 (`check_budgets`, `TeamRules`, the price table, on main); ADR 0015
Readiness confirmed by: fresh-session reviewer, 2026-09-23 (one round; findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A new Farik project spends without any dollar ceiling unless its user sets one. The team's daily budget and the team's cap on a task's budget become optional, with no default. A limit that is set works exactly as it does today, and one that is left out is no limit of that kind. The loop bounds stay: session tokens, wall clock, tool calls, sessions per task, and rejections. An agent may run on any model: a model no price table prices is recorded at no cost and named by `farik doctor` and by every start of `farik run`, instead of having its usage refused. `claude-opus-5-5` joins the shipped price table. Out of scope: the sprint's budget itself (phase 4 step 02, which ADR 0015 binds to the same rule), the setup screen (phase 5), and a contract without `max_cost_usd`.

## Decisions

- Every dollar limit Farik ships becomes optional, with no default: ADR 0015. `budgets.daily_usd` leaves the `required` list of `$defs/budgets`, and `rules.max_task_budget_usd` loses its five-dollar default. Both keep `exclusiveMinimum: 0`: leaving a limit out is how to have none, and a zero is refused as it is today. Rejected: keeping a default that a user can raise, which still caps spending, and the founder ruled that out.
- `budgets` stays a required object, and a team with no daily budget writes `budgets: {}`, just as `rules: {}` is written today. Rejected: making `budgets` optional too. typify would then generate `Option<TeamBudgets>`, and every reader of `budgets.session` would change for no behaviour. `budgets.session` is untouched.
- An unset limit is `f64::INFINITY` inside `BudgetState`, as the sprint's and a task-less session's are already (step 03). `check_budgets`, `has_reached`, `exceeds`, and `BudgetState` are unchanged. Rejected: `Option<f64>` fields in `BudgetState`, which changes `farik-core`'s budget decision and its tests to say what infinity already says.
- `budget_state` sets `day_max_usd` to `team.budgets.daily_usd.unwrap_or(f64::INFINITY)`. `Transitions::context` keeps computing `remaining_sprint_budget_usd` as the day's remainder. That is infinite with no daily budget, so a contract no longer fails readiness late in a UTC day. With a daily budget set, it behaves as today. Rejected: reading the sprint's (unbounded) remainder for every team, which changes how a set daily limit behaves. ADR 0015 keeps a set limit exactly as it is, and phase 4 step 02 already replaces the stand-in with the sprint's own remainder.
- `TeamRules::default().max_task_budget_usd` is `None`. `Team::rules()` still uses the team's cap when it writes one. `budget_within_team_max` already skips a `None` cap, `farik rules show` already prints `most a contract may cost: no cap`, and the prompt already prints `max_task_budget_usd: none`. The schema's description, which says there is no way to write "no cap", is rewritten to say that leaving the field out is no cap. That was 5.12's "rules only narrow" reasoning: an absent rule narrows nothing, and so it loosens nothing either.
- `DEFAULT_DAY_BUDGET_USD`, `DEFAULT_SPRINT_BUDGET_USD` (`farik-core::budget`), and `DEFAULT_MAX_TASK_BUDGET_USD` (`farik-core::governor::team_rules`) are deleted. A constant named "default" that nothing defaults to invites the next reader to wire it back in. The tests that use them take the same numbers as literals.
- A contract's `budget.max_cost_usd` stays required in `task-contract.schema.json`. The Product Manager writes it as the contract's estimate, the human approves it with the contract, and three readiness rules (`BudgetWithinSprint`, `BudgetWithinTeamMax`, `BudgetWithinParent`), the judgment "small enough to finish within its budget" (5.3), and an epic's child arithmetic (`Transitions::parent_state`) are all measured against it. Making it optional would touch the schema, the generated type, all of those, the Product Manager's skill, and `TaskUsd`. That is the largest change on the table, and the founder asked for none of it. It is not a limit Farik ships.
- A team file that never wrote `max_task_budget_usd` loses the old five-dollar cap without a word: it read as 5 through `TeamRules::default()`, and it now reads as no cap. That is the decision (ADR 0015), and the SPEC's revision note says so, so that nobody learns it from a bill. A team that wants the cap writes `max_task_budget_usd: 5`.
- The placeholder budget of `farik contract new` (step 15, amended here, controller's ruling): step 15 builds the placeholder request with `budget.max_cost_usd` set to the team's `max_task_budget_usd`, which is always a number before this step and may be `None` after it. `contract_new.rs` gains `PLACEHOLDER_MAX_COST_USD: f64 = 20.0` and `placeholder_budget_usd(rules: &TeamRules) -> f64`, the team's cap when it has one and otherwise 20 dollars. The Product Manager rewrites the figure while refining, as it rewrites every placeholder. This change is part of Task 1, because Task 1 is what makes the cap `None`. Step 15's plan and the project plan's step 17 line say so. The step 18 run depends on it.
- An error after a session starts (a store or event error) leaves that session with no `session.ended` and no cost. Step 11's landing review found this, and it belongs to step 11's landing fixes, which land before this step starts (the start gate). This step does not own it. It does remove one source of such errors, `CostError::Pricing`.
- Fixtures: `farik_core::team::fixtures::a_team_wire` keeps `daily_usd: 20`, so every existing test reads the team it reads today. The tests of this step build their team with `budgets: {}`. A test that relied on the shipped five-dollar task cap sets `max_task_budget_usd` to `Some(5.0)` (or `5` in its wire) itself, and its assertions do not change. `reads_a_team_with_only_what_it_must_have` deletes `/budgets/daily_usd` from its wire, because that field is no longer something a team must have.
- A model no price table prices is recorded, not refused (ADR 0015). `record_session_cost` appends `cost.recorded` with the tokens, `cost_usd: 0.0`, and `unpriced: true`, projects it, and returns `Ok(0.0)`. `costRecordedBody` gains `unpriced`, a boolean with `"default": false`, which typify renders as `pub unpriced: bool` with `#[serde(default)]`, as it renders `new_tests_required`. An older log with no `unpriced` reads as priced. typify writes `#[serde(default)]` with no `skip_serializing_if`, so every `cost.recorded` written from this step on carries `"unpriced": false` or `true` explicitly. The protocol's `a_body_wire` fixture for `cost.recorded` therefore gains `"unpriced": false`, so that `writes_back_exactly_the_value_it_read_for_every_kind` still round-trips. `CostError::Pricing` is deleted, because nothing produces it any more. `compute_cost_usd` and `PricingError::UnknownModel` in `farik-core` stay: the runtime's reading of the error changes, not the pricing decision. No migration: `cost_records` keeps the row with `cost_usd` 0, so the session still counts toward `max_sessions` and toward step 16's active weeks. Rejected: a `cost_usd` of `null` or NaN. `null` breaks every sum. NaN counts as exhausted (5.5), which would stop the team over a price it merely lacks.
- Cost per accepted task (step 16) sums `cost_usd` as it does, so an unpriced report adds its session and no dollars. The metric is unchanged, and it now under-reports by exactly what the warnings name. Rejected: a count of unpriced reports in `HarnessMetrics`, which would need a new column and a migration for a case the warnings already cover.
- Which models count as "in use": for each active agent, the model its sessions run on, which is the agent's own `model.id` if it has one and otherwise its role's default model (`load_role`). That rule moves out of `session_spec` into `session_model`, so the check and the sessions cannot disagree. `session_spec` keeps step 14's choice of `TRIAGE_MODEL` at effort `low` for `SessionPurpose::Triage`, and calls `session_model` for every other purpose, so step 14's `triages_a_request_on_the_cheaper_model` passes unchanged. On top of that, `TRIAGE_MODEL` counts for each active Product Manager, whose triage sessions run on it (step 14). An agent with its own `model.id` counts that model and needs no role file. An agent with no `model` whose role Farik does not ship (`load_role` answers `RoleError::NotFound` for the Scrum Master, the Architect, and the Marketing Specialist) is skipped, because it runs no session in this phase: nothing could load its role. `unpriced_models` returns `Err` only for `RoleError::Invalid`, a shipped role file that breaks its schema. The prices are `ProjectFiles::effective_prices()`: the override as a whole when it exists, else the shipped table (5.5).
- The warnings. `farik doctor` adds one finding per unpriced model: `.farik/team.yaml: no price table prices <model> (used by <ids>): its usage is recorded at no cost, and no dollar limit counts it. Add it to .farik/prices.json to price it (5.5)`, with the ids joined by `, ` in team order and the models in `BTreeMap` order. As for every finding, doctor exits 1. When `effective_prices` fails, doctor adds that error's own sentence as a finding instead and checks no model. Every driving start (`farik run`, `farik plan`, `farik contract new`, through `start.rs`) prints one stderr line per unpriced model, `warning: no price table prices <model> (used by <ids>): its usage is recorded at no cost, and no dollar limit counts it. Add it to .farik/prices.json to price it.`, at the place in step 15's ordered start list that comes after `read_settings` and the credential line (when there is one; `Engine::Given` prints none) and before `serve`. An `effective_prices` error refuses the start in the error's own words, with the lock released, because every session would fail on the same read (step 11 reads the prices once per session). Rejected: refusing to run while a dollar limit is set and a model is unpriced. Every contract carries `max_cost_usd`, so that would block task work on any unpriced model (ADR 0015).
- `claude-opus-5-5` is added to the shipped table at 4.00 input, 20.00 output, 5.00 cache write, and 0.20 cache read (USD per million tokens), and `retrieved_at` becomes the day Task 2 runs. Input, output, and cache read are the provider's published figures. The cache write follows the 1.25 × input rule that every other row follows. Task 2 checks all four against `source_url` on the day and stops for the founder if any differs. The table's format, version 1, is unchanged. The starter team stays on `claude-opus-5` (SPEC 8.2), because nothing in the founder's decision moves the default model.
- `farik init`'s starter team writes `budgets: {}` and `rules: {}`, so it has no dollar limit. A rescan keeps a team file that is already there (section 3), so existing projects keep the limits they hold, which stay enforced.
- The Product Manager's skill changes in two places, both of which speak of the sprint. At line 53, the rule becomes "A task's budget is within the team's maximum when the team sets one, what is left of the sprint when it has a budget, and, under an epic, what is left of the epic." At line 82, the checklist line becomes "the budget is set and fits the sprint when it has a budget, the team's maximum when there is one, and, under an epic, the epic;".
- Changed 2026-09-23 in execution (Task 1): one test outside the file map relied on the shipped five-dollar cap, `returns_a_failing_contract_with_its_failures` (`crates/runtime/src/orchestrator/requests.rs`), which reads the cap's refusal; its team sets `max_task_budget_usd: 5` and its assertions are unchanged. `reads_no_days_remainder_without_a_daily_budget` and `starts_sessions_whatever_the_day_cost_without_a_daily_budget` passed once `budget_state` read an unset day as infinite, since they were written after it; each was seen to fail with `unwrap_or(0.0)` put in its place. 5.5's opening line keeps "the others with role-based defaults" after "the three dollar ones optional", since the session limits still have them.
- Changed 2026-09-23 in execution (Task 3): the new required field broke the one place that builds a `CostRecordedBody`, `record_session_cost`, so this task sets `unpriced: false` there and Task 4 gives it its meaning. `reads_a_cost_without_unpriced_as_priced` could not fail at run time before the field existed (the old wire already had no `unpriced`); it failed to compile, and it holds the default from here on.
- Changed 2026-09-23 in execution (Task 4): `TRIAGE_MODEL` moves to `farik_runtime::session`, beside `session_model`, and `orchestrator` re-exports it, so `farik_runtime::orchestrator::TRIAGE_MODEL` still names it: the orchestrator is built on unix alone, and `cost::unpriced_models`, which names the triage model, is built everywhere. Step 11's `ends_a_session_whose_usage_cannot_be_costed` is rewritten, as its note says: the usage it reports is `u64::MAX` input tokens, past what the log's integer holds, so the cost fails with `CostError::Event` (the tick is `Err(Cost(Event))`, `abort` called once, one `session.ended { reason: error }`). Its neighbour `ends_a_session_that_cannot_start_when_its_cost_fails` rested on the same pricing error and no longer met a failing cost; it is renamed `ends_a_session_that_cannot_start_on_a_model_no_table_prices` and also holds that the session's one zero cost is recorded `unpriced`. `runs_a_session_on_a_model_no_table_prices` passed once `record_session_cost` recorded an unpriced model, being written after it; it was seen to fail with the unknown model's arm put back to an error. SPEC's revision 0.9 note also names 5.5's unpriced sentence.

## File map

```
docs/schemas/team.schema.json                        modifies: daily_usd optional; max_task_budget_usd's description
docs/schemas/event.schema.json                       modifies: costRecordedBody gains `unpriced`
crates/core/src/budget.rs                            modifies: two constants deleted; its tests take literals
crates/core/src/governor/team_rules.rs               modifies: no default cap; the constant deleted; tests
crates/core/src/governor/transition.rs               modifies: tests only, literals for the deleted constants
crates/core/src/team.rs                              modifies: tests
crates/core/src/pricing/prices.rs                    modifies: claude-opus-5-5, retrieved_at
crates/core/src/pricing.rs                           modifies: the shipped-table test
crates/protocol/src/event.rs                         modifies: tests
crates/protocol/src/event/fixtures.rs                modifies: a_body_wire's cost.recorded gains "unpriced": false
crates/runtime/src/cost.rs                           modifies: budget_state's day; record_session_cost; CostError; unpriced_models; tests
crates/runtime/src/session.rs                        modifies: session_model and its test
crates/runtime/src/orchestrator/session.rs           modifies: session_spec calls session_model
crates/runtime/src/orchestrator/rules.rs             modifies: tests
crates/runtime/src/transitions.rs                    modifies: tests
crates/store/tests/metrics.rs                        modifies: one test
crates/cli/src/init.rs                               modifies: starter team; test
crates/cli/src/contract_new.rs                       modifies: the placeholder budget (step 15, amended); test
crates/cli/src/doctor.rs                             modifies: the unpriced-model finding
crates/cli/src/start.rs                              modifies: the unpriced-model warning
crates/cli/tests/reading.rs                          modifies: doctor's tests
crates/cli/tests/running.rs                          modifies: the start's test
crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md   modifies: "when the team sets one"
docs/SPEC.md                                         modifies: 3, 4.1, 5.2, 5.3, 5.5, 5.12, 10; a revision note
docs/plans/project-plan.md                           modifies: step 17's interface line, as landed
```

## Interfaces

Consumes: `check_budgets`, `BudgetState`, `TeamRules`, `Team::rules`, `validate_team`, `TeamBudgets`, `compute_cost_usd`, `PricingError`, `PRICES_JSON`, `PriceTable` (`farik-core`, main); `CostRecordedBody` (`farik-protocol`); `budget_state`, `record_session_cost`, `CostSource`, `CostError` (step 03); `Transitions::context` (step 04); `room`, `session_spec` (step 11); `load_role`, `RoleDefinition`, `RoleError` (step 09); `TRIAGE_MODEL` (step 14, `crates/runtime/src/orchestrator.rs`); `start.rs`'s driving start and `crates/cli/tests/running.rs` (step 15); `Projections::metrics`, `crates/store/tests/metrics.rs` (step 16); `ProjectFiles::effective_prices` (step 03).

Produces:

```rust
// farik-core (generated from team.schema.json)
pub struct TeamBudgets { pub daily_usd: Option<f64>, pub session: Option<SessionLimitsWire> }
// farik-protocol (generated from event.schema.json)
pub struct CostRecordedBody { .., pub unpriced: bool }
// farik-runtime::cost
pub enum CostError { Store { detail: String }, Event { detail: String } }
pub fn unpriced_models(team: &Team, prices: &PriceTable) -> Result<BTreeMap<String, Vec<String>>, RoleError>;
// farik-runtime::session
pub fn session_model(agent: &Agent, role: &RoleDefinition) -> (String, Effort);
// farik (binary crate), crates/cli/src/contract_new.rs
pub(crate) const PLACEHOLDER_MAX_COST_USD: f64 = 20.0;
pub(crate) fn placeholder_budget_usd(rules: &TeamRules) -> f64;
```

`unpriced_models` maps each model id that `prices.prices` has no row for to the ids of the active agents that use it, in team order and without repeats.

## Tasks

### Task 1: dollar limits the user may leave out

Files: `team.schema.json`, `budget.rs`, `team_rules.rs`, `transition.rs` (tests), `team.rs` (tests), `cost.rs` (`budget_state`, its doc comment saying an unset day is unbounded, and tests), `contract_new.rs`, `transitions.rs` (tests), `orchestrator/rules.rs` (tests), `SKILL.md`, `docs/SPEC.md`
Produces: `TeamBudgets::daily_usd: Option<f64>`; `TeamRules::default().max_task_budget_usd == None`; the three constants deleted
Consumes: nothing from this plan

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `reads_a_team_with_only_what_it_must_have` (`team.rs`, changed): with `/budgets/daily_usd` removed from `a_team_wire`, the team validates, `budgets.daily_usd` is `None` and `budgets.session` is `None`. Fails first on the schema's `required`.
- `protects_the_secret_paths_and_caps_no_task_by_default` (`team_rules.rs`, renamed from `..._and_caps_a_task_at_five_dollars_by_default`): the five protected paths, and `max_task_budget_usd == None`.
- `fills_in_every_rule_the_team_left_out` (`team.rs`, changed): `max_task_budget_usd` is `None`. `reads_the_rules_a_team_wrote` still reads `Some(12.5)`.
- `refuses_a_number_outside_what_a_rule_allows` (`team.rs`, unchanged): still refuses `daily_usd: 0` and `max_task_budget_usd: 0`. Run to confirm that zero is still refused.
- `leaves_the_day_unbounded_without_a_daily_budget` (`cost.rs`): a team wire with `budgets: {}` and a seeded `cost.recorded` of 1000 dollars today. `budget_state` gives `day_max_usd` infinite and positive and `day_spent_usd` 1000, and `check_budgets` reports no `DayUsd`.
- `reads_budgets_from_the_team_the_contract_and_the_day` (`cost.rs`, unchanged): still `day_max_usd` 20 with the team's 20.
- `reads_no_days_remainder_without_a_daily_budget` (`transitions.rs`, `#[ignore]` as its neighbours, integration): with `budgets: {}`, the readiness context's `remaining_sprint_budget_usd` and the assignment's are both infinite and positive.
- `starts_sessions_whatever_the_day_cost_without_a_daily_budget` (`orchestrator/rules.rs`): the setup of `starts_nothing_when_the_day_is_spent` with `budgets: {}`. Rule 6 comes before rule 8, so the recorded adapter is given the implement transcript first. After the second tick, `adapter.started()[0]` is FRK-2's session with purpose `Implement`, and the tick's report is not `Idle { why: "the team's daily budget is spent" }`.
- `places_a_budget_of_the_team_cap_or_twenty_dollars` (`contract_new.rs`, unit): `placeholder_budget_usd` of rules with `Some(12.5)` is 12.5, and of `TeamRules::default()` is 20.0. `builds_a_request_the_store_files` (step 15) still passes with the value this gives.
- Every test in the workspace that relied on the deleted defaults sets its figure as a literal or in its wire, and its assertions are unchanged.

SPEC, in this commit: 5.5's opening line ("Five budgets, all enforced by the governor, all configurable per team" becomes "Five budgets, each enforced by the governor when it is set, all configurable per team", the three dollar ones optional), and its phase 3 minimum (added in 0.8: "or the day" becomes "or a daily budget that is set", and "nor for anyone once the day is spent" becomes "nor for anyone once a daily budget that is set is spent"); 3 (a team has "optional spending limits"; a sprint's budget is one "when the user sets one"); 4.1 (the daily budget is offered, optional, and empty means no daily dollar limit; "Nothing runs until these are set" now covers the two permissions only); 5.2's `ready → assigned` gate ("budget available in the sprint, when it has one"); 5.3 ("does not exceed the remaining sprint budget, when the sprint has one"); 5.5's table (per sprint: "set at planning, optional"; per day: "set by the user, optional") and the defaults sentence, which becomes "Farik ships no dollar limit: the daily budget, a sprint's budget, and the team's cap on a task's budget (5.12) are each the user's to set, and one left out is no limit of that kind (ADR 0015); the token, wall-clock, tool-call, session, and iteration limits ship as above, because they bound a loop rather than spending"; 5.12's `max_task_budget_usd` row ("when it is set") and its defaults ("`max_task_budget_usd` unset, no cap"); 10's first bullet ("the shipped models are chosen so that a first day of a team of five running a full sprint costs under twenty dollars on the user's key at current list prices; this is a property of the defaults, not a cap"); and a revision note in the header, numbered one past the last note there when this task starts, naming these sections and ADR 0015, and saying that a team file which never wrote `max_task_budget_usd` no longer has the five-dollar cap it used to get by default.

- [x] `feat(core): make the team's dollar limits optional`

### Task 2: the price of Opus 5.5

Files: `prices.rs`, `pricing.rs` (tests)
Produces: the row `claude-opus-5-5`
Consumes: nothing

- `ships_a_table_that_matches_its_schema_and_prices_every_model_a_team_can_call` (changed): 13 rows, including `("claude-opus-5-5", 4.0, 20.0, 5.0, 0.2)`.
- `computes_the_cost_of_opus_5_5` : one million input tokens and one million output tokens on `claude-opus-5-5` cost 24.0 dollars, within 1e-9.

- [x] `feat(core): price claude-opus-5-5`

### Task 3: a cost no price table priced

Files: `event.schema.json`, `event.rs` (tests), `event/fixtures.rs`, `crates/store/tests/metrics.rs`
Produces: `CostRecordedBody::unpriced: bool`
Consumes: nothing from this plan

- `reads_a_cost_without_unpriced_as_priced` (`event.rs`): a copy of the `cost.recorded` fixture with `/body/unpriced` removed parses with `unpriced == false`.
- `writes_back_exactly_the_value_it_read_for_every_kind` (`event.rs`, unchanged): still passes, with `a_body_wire`'s `"unpriced": false`.
- `keeps_an_unpriced_cost` (`event.rs`): the fixture with `"unpriced": true` and `"cost_usd": 0` validates, parses with `unpriced == true`, and serialises back with `"unpriced": true`.
- `counts_an_unpriced_report_as_a_session_at_no_cost` (`metrics.rs`): one accepted task with a priced `cost.recorded` of 2.0 on its implement session and an unpriced one on its verify session. Cost per accepted task is 2.0 in total, with `verify` at 0.0, and `active_weeks` is 1. It fails first because the schema refuses `unpriced` as a property it does not know.

- [x] `feat(protocol): mark a cost that no price table priced`

### Task 4: record usage of a model no table prices

Files: `cost.rs`, `session.rs`, `orchestrator/session.rs`, `orchestrator/rules.rs` (tests), `docs/SPEC.md` (5.5)
Produces: `record_session_cost`'s new behaviour; `CostError` without `Pricing`; `session_model`; `unpriced_models`
Consumes: `CostRecordedBody::unpriced` from Task 3

- `records_a_model_no_table_prices_at_no_cost_and_says_so` (`cost.rs`, replaces `refuses_a_model_the_table_does_not_price`): usage of 1000 input tokens on `claude-unknown-9` returns `Ok(0.0)`. The log holds one `cost.recorded { model_id: claude-unknown-9, cost_usd: 0, unpriced: true, usage.input_tokens: 1000 }`, and the task's cost projection has `sessions` 1 and `usd` 0.
- `records_one_cost_priced_from_the_table` (`cost.rs`, one assertion added, no new test): its event has `unpriced: false`.
- `takes_the_agents_model_and_else_the_roles` (`session.rs`): an agent with `model { id: claude-sonnet-5, effort: low }` gives `("claude-sonnet-5", Low)`; with `model { id: claude-sonnet-5 }` and no effort, the role's effort; with no `model`, the role's model and effort.
- `names_each_unpriced_model_with_the_agents_that_use_it` (`cost.rs`): a team of `pm` (no model; `product_manager`'s default), `dev-a` on `claude-unknown-9`, `dev-b` on `claude-unknown-9`, a paused `dev-c` on `claude-other-1`, an active Architect `arch` with no `model` (skipped: Farik ships no Architect role), and an active Architect `arch-2` on `claude-other-2`, against a table holding only `claude-opus-5` (the Product Manager role's default model), gives `{ "claude-other-2": ["arch-2"], "claude-sonnet-5": ["pm"], "claude-unknown-9": ["dev-a", "dev-b"] }`, and is `Ok` although `load_role(Architect)` is `NotFound`; against `PRICE_TABLE`, `{ "claude-other-2": ["arch-2"], "claude-unknown-9": ["dev-a", "dev-b"] }`.
- `runs_a_session_on_a_model_no_table_prices` (`orchestrator/rules.rs`): a developer on `claude-unknown-9` with FRK-1 `in_progress` and a recorded implement session. The tick is `Ok`. The log holds `session.started`, then `cost.recorded { unpriced: true, cost_usd: 0 }`, then `session.ended`, and FRK-1's cost projection has `sessions == 1`.

SPEC 5.5, in this commit: "a report for a model neither table prices is refused and records nothing, because a cost of zero would under-report spend" becomes "a report for a model the table in use does not price is recorded with its tokens, a cost of zero and `unpriced: true`, and no dollar limit counts it, so that an agent can run on any model its user chooses (ADR 0015)".

- [x] `feat(runtime): record usage of a model no price table prices`

### Task 5: a starter team with no dollar limit

Files: `init.rs`, `docs/SPEC.md` (3)
Consumes: Task 1's optional `daily_usd`

- `starts_a_project_with_no_dollar_limit` (`init.rs`): `starter_team("notes")` has `budgets.daily_usd == None`, `budgets.session == None`, and `rules().max_task_budget_usd == None`.

SPEC 3, in this commit: the starter team is written "with no dollar limit (5.5)".

- [ ] `feat(cli): start a project with no dollar limit`

### Task 6: say which models no price table prices

Files: `doctor.rs`, `start.rs`, `reading.rs`, `running.rs`, `docs/SPEC.md` (3)
Consumes: `unpriced_models` from Task 4

- `reports_a_model_no_price_table_prices` (`reading.rs`, `#[ignore]` as its neighbours): a team with `dev` on `claude-unknown-9`. `farik doctor` exits 1 and prints exactly the finding in this plan's Decisions for `claude-unknown-9` used by `dev`. With a `.farik/prices.json` that is the shipped table plus a `claude-unknown-9` row, that line is absent and doctor exits 0.
- `reports_a_price_table_it_cannot_read` (`reading.rs`, `#[ignore]`): `.farik/prices.json` holding `{"version": 2}` makes doctor exit 1 with a finding that contains `.farik/prices.json`, and no finding that starts `.farik/team.yaml: no price table prices`.
- `warns_on_every_start_of_a_model_no_table_prices` (`running.rs`, recorded engine, empty board): with `dev` on `claude-unknown-9`, two `farik run`s each print the warning line in this plan's Decisions to stderr. With every model priced, neither does.
- `refuses_to_start_on_a_price_table_it_cannot_read` (`running.rs`): with `.farik/prices.json` holding `{"version": 2}`, `farik run` exits 1, stderr contains `.farik/prices.json`, the run lock is free, and nothing is logged.

SPEC, in this commit: 3, `farik doctor` also reports "an active agent's model that no price table prices"; 5.5, after the unpriced sentence of Task 4: "`farik doctor` and every start of `farik run` name such a model".

- [ ] `feat(cli): warn of a model no price table prices`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
git grep -n "DEFAULT_DAY_BUDGET_USD\|DEFAULT_SPRINT_BUDGET_USD\|DEFAULT_MAX_TASK_BUDGET_USD\|CostError::Pricing" -- crates
# expected: no output, exit 1
```
