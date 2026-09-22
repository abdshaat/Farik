# Phase 3, step 03: Cost recording

Status: draft
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` section 5.5, F6
Depends on: step 01 of this phase (`SessionPurpose`, committed); phase 2 (the log, the projections, `ProjectFiles::read_prices`, on main)
Readiness confirmed by: pending

## Goal

What a session consumed becomes money in the log: one `cost.recorded` event per usage report, priced from the shipped table or the project's override, and summed by task, agent, session, and day in the projections. From those sums Farik can say, before and after any session, how much of each budget in 5.5 is left, and records `budget.exhausted` the moment one runs out. Out of scope: acting on an exhausted budget (ending the session, blocking or escalating the task, pausing the team is the orchestrator's, step 11), sprint budgets (phase 4), and showing costs (step 13's `farik task show`).

## Decisions

- One `cost.recorded` per `SessionEvent::UsageReported`, which the runtime reads from the `result` line once per session run (step 01). Its envelope carries the session, agent, and task ids; its body is `{ purpose, model_id, usage: { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens }, cost_usd }`. The event carries the computed dollars, not only the tokens, so that the log says what was charged under the prices of that day, and a later change to `.farik/prices.json` does not rewrite history.
- `budget.exhausted` body is `{ scope, consequence }` with the snake_case names of `farik_core::budget::BudgetScope` and `BudgetConsequence`; the envelope carries whatever ids the scope is about. It is recorded once, when a scope crosses from not exhausted to exhausted, by comparing the budget state before and after the cost: a day that stays exhausted records it once, not after every later session.
- Projections keep one row per `cost.recorded` in a new `cost_records` table (migration `0003_costs.sql`), and every sum is a `GROUP BY` over it. Chose rows over running totals per scope because the sums wanted are several (task, agent, session, day, and by purpose in step 14) and `COUNT(DISTINCT session_id)` for a task's sessions is a query, not another counter to keep right. `rebuild` clears it with the board.
- The day of a cost is the UTC date of its `recorded_at`. 5.5 says "daily"; the user's time zone is not known to the log, and UTC is the one day every machine agrees on. Recorded as a known edge: a team in UTC-8 sees its day end at four in the afternoon.
- `CostScope` is `Task`, `Agent`, `Session`, `Day`. `Sprint` is cut until phase 4 adds sprints (revision 8 listed it); a scope with no key to group by is a scope no test can hold to anything.
- `TaskProjection` gains `cost_usd: f64`, the sum of that task's records, `0.0` when there are none.
- A task's sessions, for `BudgetState::task_sessions`, are the distinct session ids among its cost records. `session.started` arrives in step 08; counting from it then is step 08's to decide, and until then a session that recorded no usage cost nothing and is not counted.
- Prices are the project's `.farik/prices.json` when present, else `farik_core::pricing::prices::PRICE_TABLE`, whole-table rather than merged per model, because 5.5 calls the file an override and a merge would make a model the user deleted still priced. A usage report for a model neither table knows is `CostError::Pricing` and records nothing: a cost of zero for an unknown model would under-report spend, which is the failure 5.5 exists to prevent.
- `record_session_cost` and `budget_state` live in `farik-runtime::cost` and return `CostError` (revision 8); `effective_prices` lives in `farik-store` beside `read_prices` and returns `FilesError`.
- `budget_state` limits: session limits from `team.budgets.session` when set, else `farik_core::budget::default_session_limits(role)`; the task's from its contract's `budget` (`max_cost_usd`, `max_sessions`); a session with no task has `task_max_usd` and `task_max_sessions` at `f64::INFINITY` and `u32::MAX`; the day's is `team.budgets.daily_usd`; the sprint's is `f64::INFINITY` spent `0.0` (the phase decision).
- `SessionPurpose` maps to the generated wire enum through one `From` in `farik-runtime::cost`, the crate's one mapping at its edge (code.md).

## File map

```
docs/schemas/event.schema.json                     modifies: cost.recorded, budget.exhausted
crates/protocol/src/event.rs                       modifies: the two kinds in EventBody, EVERY_KIND, body_def_name; tests
crates/protocol/src/event/fixtures.rs              modifies: a_cost_recorded(), a_budget_exhausted()
crates/store/src/migrations/0003_costs.sql         creates: cost_records
crates/store/src/migrations.rs                     modifies: the third migration
crates/store/src/projections.rs                    modifies: apply cost.recorded, rebuild, cost_usd, costs(); tests
crates/store/src/files.rs                          modifies: effective_prices; tests
crates/store/src/lib.rs                            modifies: re-exports
crates/runtime/Cargo.toml                          modifies: farik-protocol, farik-store, chrono
crates/runtime/src/cost.rs                         creates: CostError, record_session_cost, budget_state, record_exhaustion; tests
crates/runtime/src/lib.rs                          modifies: `pub mod cost;`
docs/SPEC.md                                       modifies: 5.5, a day is a UTC day and a cost is priced when recorded
```

## Interfaces

Consumes: `SessionPurpose` (step 01); `Usage`, `compute_cost_usd`, `PriceTable`, `PRICE_TABLE` (`farik-core::pricing`); `BudgetState`, `BudgetScope`, `BudgetConsequence`, `SessionLedger`, `check_budgets`, `default_session_limits` (`farik-core::budget`); `Team`, `Role`, `TaskContract`; `EventLog`, `Projections`, `ProjectFiles`, `EventIds`, `new_event` (on main); `Clock` (`farik-protocol`).

Produces:

```rust
// farik-protocol
EventKind::{CostRecorded, BudgetExhausted}; EventBody::{CostRecorded(CostRecordedBody), BudgetExhausted(BudgetExhaustedBody)}
// farik-store
pub enum CostScope { Task, Agent, Session, Day }
pub struct CostProjection { pub scope: CostScope, pub key: String, pub usd: f64, pub input_tokens: u64, pub output_tokens: u64, pub sessions: u32 }
impl Projections { pub fn costs(&self, scope: CostScope) -> Result<Vec<CostProjection>, StoreError>; }   // ordered by key
TaskProjection { .., pub cost_usd: f64 }
impl ProjectFiles { pub fn effective_prices(&self) -> Result<PriceTable, FilesError>; }
// farik-runtime::cost
pub enum CostError { Store { detail: String }, Pricing { detail: String }, Event { detail: String } }
pub struct CostSource<'a> { pub ids: EventIds, pub purpose: SessionPurpose, pub model_id: &'a str }   // ids carries session, agent, task
pub fn record_session_cost(log: &EventLog, projections: &Projections, source: &CostSource<'_>, usage: &Usage, prices: &PriceTable, clock: &dyn Clock) -> Result<f64, CostError>;
pub fn budget_state(projections: &Projections, team: &Team, role: Role, task: Option<&TaskContract>, session: &SessionLedger, now: DateTime<Utc>) -> Result<BudgetState, CostError>;
pub fn record_exhaustion(log: &EventLog, projections: &Projections, before: &BudgetState, after: &BudgetState, ids: &EventIds, clock: &dyn Clock) -> Result<Vec<Exhausted>, CostError>;   // returns what it recorded
```

`CostError::Event` is added beside the two revision 8 named: `new_event` can refuse a blank id, and that refusal is neither the store's nor the prices'.

## Tasks

### Task 1: the two events

Files: modified `docs/schemas/event.schema.json`, `crates/protocol/src/event.rs`, `crates/protocol/src/event/fixtures.rs`, tested in `event.rs`
Produces: the two kinds and bodies, and their fixtures
Consumes: nothing new

Tests:

- `reads_back_a_cost_recorded_event_it_wrote` — `event_to_value` then `event_from_value` of `a_cost_recorded()` is equal to it, and its `kind` on the wire is `cost.recorded`.
- `refuses_a_cost_with_a_negative_amount` — a `cost.recorded` value with `cost_usd: -0.01` is refused with a `ValidationError` whose `path` ends in `/cost_usd`.
- `refuses_a_cost_for_an_unknown_purpose` — `purpose: "lunch"` is refused at `/body/purpose`.
- `reads_back_a_budget_exhausted_event_it_wrote` — as the first, for `a_budget_exhausted()` with `scope: day_usd`, `consequence: pause_team`.
- The existing test that walks `EVERY_KIND` covers the two new kinds once they are in it; it is extended, not duplicated.

- [ ] `feat(protocol): add the cost.recorded and budget.exhausted events`

### Task 2: costs in the projections

Files: created `crates/store/src/migrations/0003_costs.sql`; modified `crates/store/src/migrations.rs`, `crates/store/src/projections.rs`, `crates/store/src/lib.rs`, tested in `projections.rs`
Produces: `CostScope`, `CostProjection`, `Projections::costs`, `TaskProjection::cost_usd`
Consumes: the events from Task 1

Tests, each on a log in memory:

- `sums_a_tasks_costs_on_the_board` — two `cost.recorded` of 0.25 and 0.5 for FRK-1 after its `task.created`: `task(FRK-1).cost_usd == 0.75`, and a task with none has `0.0`.
- `groups_costs_by_each_scope` — records for two agents, two sessions, one task, on two UTC days: `costs(Agent)`, `costs(Session)`, `costs(Day)` each return one row per key, ordered by key, with the right `usd` and token sums; the day keys are `2026-09-21` and `2026-09-22`.
- `counts_a_tasks_distinct_sessions` — three records for FRK-1 from sessions `a`, `a`, `b`: its `Task` row has `sessions == 2`.
- `rebuilds_costs_from_the_log` — after `rebuild`, `costs(Task)` equals what it was before, not double.
- `applies_the_third_migration_once` — `applied_migrations()` is `[1, 2, 3]` on a new log and on a reopened one.

- [ ] `feat(store): project costs by task, agent, session, and day`

### Task 3: which prices apply

Files: modified `crates/store/src/files.rs`, tested in `crates/store/tests/project_files.rs`
Produces: `ProjectFiles::effective_prices`
Consumes: `read_prices` (on main), `PRICE_TABLE`

Tests:

- `prices_with_the_shipped_table_when_there_is_no_override` — equals `*PRICE_TABLE`.
- `prices_with_the_override_as_a_whole` — an override naming one model only: the result has that one model, and a model only the shipped table has is absent.
- `refuses_an_override_that_is_not_a_price_table` — `{"version": 1}` is `Err(FilesError::Invalid { .. })` naming `prices.json`.

- [ ] `feat(store): choose the project's prices or the shipped table`

### Task 4: recording costs and exhausted budgets

Files: created `crates/runtime/src/cost.rs`; modified `crates/runtime/Cargo.toml`, `crates/runtime/src/lib.rs`, `docs/SPEC.md` (5.5), tested in `cost.rs`
Produces: `CostError`, `CostSource`, `record_session_cost`, `budget_state`, `record_exhaustion`
Consumes: Tasks 1 to 3; `SessionPurpose`; `check_budgets`

Tests, on a log in memory with `FixedClock` at `2026-09-22T10:00:00Z` and a price table of one model at 1, 2, 0.5, 0.25 dollars per million input, output, cache read, cache write tokens:

- `records_one_cost_priced_from_the_table` — a usage of 1,000,000 input and 500,000 output tokens returns `2.0`, appends one `cost.recorded` with `cost_usd == 2.0`, the purpose `implement`, and the source's session, agent, and task ids, and the task's `cost_usd` on the board is `2.0`.
- `refuses_a_model_the_table_does_not_price` — `Err(CostError::Pricing { detail })` naming the model, and the log is unchanged.
- `reads_budgets_from_the_team_the_contract_and_the_day` — with a team `daily_usd: 20`, a contract `max_cost_usd: 5`, `max_sessions: 3`, and 4.0 already recorded today for the task in two sessions and 1.0 yesterday: `task_spent_usd == 4.0`, `task_max_usd == 5.0`, `task_sessions == 2`, `task_max_sessions == 3`, `day_spent_usd == 4.0`, `day_max_usd == 20.0`, `sprint_max_usd == f64::INFINITY`, `sprint_spent_usd == 0.0`.
- `uses_the_roles_default_session_limits_when_the_team_sets_none` — for a Scrum Master with no `budgets.session`: `session_limits == default_session_limits(Role::ScrumMaster)`.
- `leaves_a_session_without_a_task_unbounded_by_task_budgets` — `task: None`: `task_max_usd == f64::INFINITY`, `task_max_sessions == u32::MAX`.
- `records_an_exhausted_budget_once_when_it_is_crossed` — before at 19.0 of 20 for the day, after at 21.0: one `budget.exhausted` with `scope: day_usd`, `consequence: pause_team` is appended and returned; calling again with before and after both past 20 appends nothing.

- [ ] `feat(runtime): record session costs and exhausted budgets`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```
