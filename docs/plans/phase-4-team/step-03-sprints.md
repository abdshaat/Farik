# Phase 4, step 03: Sprints

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 3, 5.2, 5.3, 5.5, 5.11, 6.2, 8.4, 8.5; F17; D10
Depends on: phase 3 (merged in #11); steps 01 and 02 of this phase (committed through d34972d)
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the two decisions the first found open, and one more it found, folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The human can start a sprint, optionally with a budget, and the team works it: the assigner (the Scrum Master, or the Product Manager without one) plans it from the ready backlog, only its tasks are assigned while it is open, the tasks an epic's breakdown files join their epic's sprint, a spent sprint budget stops new assignments into it, and the sprint ends by itself when every task in it is accepted or cancelled, or when the human ends it. `farik sprint show` and `farik metrics --sprint` say how it went. With no sprint open, the board flows as in phase 3 (the founder, 2026-09-24: sprints are optional). Out of scope: the planning session as a channel ceremony with its digest, standups, review, and retro (step 06), and every budget consequence other than stopping assignments (step 04).

## Decisions

- A sprint is `.farik/sprints/S<n>.yaml`, held to `docs/schemas/sprint.schema.json` (owner `farik-core`, `import_types!`, ADR 0009): `id` (`^S[1-9][0-9]{0,5}$`), `started_at` (date-time), `ended_at` (date-time, absent while open), `budget_usd` (number greater than 0, absent for none: ADR 0015), `task_ids` (unique task ids, at most 1000), `status` (`open` or `ended`), `additionalProperties: false`. `validate_sprint(&Value) -> Result<Sprint, Vec<ValidationError>>` beside `validate_team`. `ProjectFiles` reads and writes it as it does the team file (validated both ways, written atomically), creating `.farik/sprints/` on the first write; `init` is unchanged.
- `n` is one more than the highest of the sprint files' numbers and the ids in the log's `sprint.started` events, as task ids take the higher of the log's counter and the files (8.4): a fresh clone does not hand out S1 again, and a lost file does not reuse an id the log holds.
- Events, none about one contract: `sprint.started { sprint_id, budget_usd, started_by }` (`budget_usd` null for none), `sprint.planned { sprint_id, task_ids, planned_by }`, `sprint.ended { sprint_id, ended_by, left }` (`ended_by` `governor` or `human`, a closed vocabulary like `task.integrated`'s; `left` the task ids that were in it and were neither accepted nor cancelled). Attribution is `started_by` and `planned_by`; `ended_by` needs none, as `integrated_by`.
- Projections: a `sprints` table (emptied by `Projections::rebuild` with the others) (`sprint_id`, `budget_usd`, `open`) from `sprint.started` and `sprint.ended`, read through `Projections::open_sprint() -> Result<Option<SprintProjection>, StoreError>`; `TaskProjection::sprint: Option<String>`, set by `sprint.planned` for each id and cleared by `sprint.ended` for each id in `left`; `cost_records.sprint`, filled when a `cost.recorded` is applied from its task's `sprint` at that moment, so a cost stays with the sprint it was spent in when the task later leaves; `CostScope::Sprint`. One migration, `0008_sprints.sql`, adds the table and the two columns and ends by emptying the projections, as `0007` does, so the log is replayed into them.
- Each change writes the files first, then records the event, the order `Transitions::record_move` uses: a sprint start writes the sprint file then `sprint.started`; a plan writes each task's contract `sprint` field (the governor's field, 5.11, writable at every status) and the sprint file's `task_ids`, then `sprint.planned`; an end writes the sprint file (`status: ended`, `ended_at`) and clears `sprint` in each contract of `left`, then `sprint.ended`. One module, `farik-runtime::sprints`, holds `start_sprint`, `plan_sprint`, and `end_sprint`, used by the commands, the tool, and the orchestrator.
- Commands: `sprint_start` with body `sprintStartBody { budget_usd: number > 0 | null }`, `budget_usd` required so that the body matches no other branch of `commandBodyWire`; `sprint_end` with `emptyBody`. `Command::SprintStart { budget_usd: Option<f64> }`, `Command::SprintEnd`. Handled in `orchestrator::human::handle`. A start while a sprint is open is refused naming it ("S1 is open; end it with farik sprint end"); an end with none open is refused ("no sprint is open"). The human's end is `ended_by: human`.
- Command line: `farik sprint start [--budget <usd>]` and `farik sprint end`, routed as the human's commands are (`start::command`: to the driving process through the daemon, or handled here); `farik sprint show [<id>]`, a reading command, prints the sprint named, else the open one, else the latest, else "no sprint yet": its id, status, start and end, `budget $<n>` or `budget none`, `spent $<n>` (from `CostScope::Sprint`), and each task in it with its status; an open sprint with no task and its planning session spent says `empty: end it with farik sprint end`.
- Planning: a rule the orchestrator runs before rule 1: while a sprint is open, holds no task, has had no planning session, the daily budget is not spent, and at least one candidate exists, the assigner (the active Scrum Master, else the active Product Manager) gets one `plan` session. A sprint has had its planning session once a `session.started` of purpose `plan` with no task was recorded after its `sprint.started`; so a planning session that plans nothing is not asked again, and an empty sprint stays open until the human ends it (`farik sprint show` says so). The session is about no task: `SessionAsk.contract` becomes `Option<&TaskContract>`, `None` here, which leaves the prompt's contract section out (ADR 0011: an empty section is left out) and gives `budget_state` no task; its `only_tool` is `farik_plan_sprint`, and its `This session` text is `SPRINT_PLAN_INSTRUCTION` (plan the sprint from the candidates in the first message with `farik_plan_sprint`, within its budget, then end), chosen by the one tool as `JUDGMENT_INSTRUCTION` is; it has no human message, since it is about no task (the log is not read for one); the daily budget is checked by a day-only check that sets `day_spent` as `spent` does; `cwd` the files' root; first message `sprint_plan_message(sprint, candidates, budget_left)` listing each candidate's id, title, kind, and `max_cost_usd`, `budget_left` being the sprint's `budget_usd` (the sprint is empty) or "no budget". Its cost has no task, so it counts against the day and against no sprint. Candidates are rows with no parent, in no sprint, `ready` (a task or an approved epic). The tick reports the session, and the automatic end below, as `TickReport::Sprint { sprint_id, what }`, printed by `farik run` and `farik plan` as `<sprint_id>: <what>`, and with `--json` as `{"sprint_id": ..., "what": ...}`, after which the printer's per-report step runs as for `Acted`. Both sprint rules run under `TickRules::All` and `TickRules::Planning`, not under `Refining`, and only when the tick has no `task_id` scope. A team with neither an active Scrum Master nor an active Product Manager gets no planning session, and an open sprint then assigns nothing; the human ends it. Step 06 turns this session into the planning ceremony.
- `farik_plan_sprint { task_ids: [String] }`, tier `read`: accepts the assigner (the active Scrum Master, or the Product Manager when the team has none), with a sprint open that holds no task yet (one plan per sprint); refuses, with the reason `sprint_plan_refused: <why>`, an id unknown, one with a parent, one not `ready`, one already in a sprint, and a list whose `max_cost_usd` added to that of the tasks already in the sprint exceeds its `budget_usd`. An epic counts once, its own `max_cost_usd` covering its tasks, and planning an epic also plans every task already under it. It answers `{ sprint_id, task_ids }`. `plan_sprint` takes `PlannedBy::Assigner(agent_id)`, which makes every check above, or `PlannedBy::Governor`, which checks only that a sprint is open and the task is in none (the join below).
- A task filed under an epic that is in a sprint joins it: after the task is created, by the `farik_create_task` handler with a parent and by `farik task create --parent` (`crates/cli/src/task.rs`, which files through `farik_store::requests::file_request` and then calls `plan_sprint` with the `ToolDeps` it builds as the other local commands do), Farik plans it into the epic's sprint with `PlannedBy::Governor`, recorded `planned_by: governor`, the epic's budget already counting it.
- Assignment within a sprint is the governor's: `AssignmentInput` gains `open_sprint: Option<String>` and `task_sprint: Option<String>` and `parent_sprint: Option<Option<String>>` (the task's epic's sprint when it has an epic), and `check_assignment` refuses, while a sprint is open, a task whose sprint is not the open one, unless its epic is in no sprint (work under an epic that began before any sprint goes on). Rule 8 and `ready_epic` pass over the rows the gate would refuse, by the gate's own predicates (membership as above, and `fits_within(max_cost_usd, the open sprint's remainder)`), so the refusal is what an agent's `farik_assign_task` meets and never a passed-over row that Farik asks again. `transitions::assignment()` is given the task's row: `task_sprint` from its `TaskProjection::sprint`, `parent_sprint` from its epic's row, and `open_sprint` read once in `context()` beside `budget_state`. Tasks assigned before the sprint started go on as they were.
- The sprint budget: `budget_state` fills `sprint_max_usd` from the open sprint's `budget_usd` (infinite with none open or none set) and `sprint_spent_usd` from `CostScope::Sprint` for it. The assignment gate's `remaining_sprint_budget_usd` becomes the open sprint's remainder (infinite without one). The readiness context's becomes the remainder of the contract's own sprint, as 5.3 says ("the remaining sprint budget, when the sprint has one"), and is infinite for a contract in no sprint: a contract refined during a sprint is in none (only a breakdown's task joins at filing), so it is never refused, refined again, and escalated for a sprint it is not in; what goes into a sprint is budget-checked by `farik_plan_sprint`. Both replace phase 3's daily stand-in; the daily budget stays enforced where it is, by `spent`. The session runner no longer aborts a session on `SprintUsd` (5.5: in-progress work may finish; `drive` aborts on every other crossed scope but `TaskSessions`, as today). When `SprintUsd` is exhausted, rule 8 and `ready_epic` start no assignment (no `plan` session, no move) for a task of that sprint; sessions of tasks already assigned go on (5.5, "in-progress tasks may finish"). `budget.exhausted` records the crossing, as it does today; the channel line comes with step 05.
- The sprint ends by itself: a rule the orchestrator runs before rule 1 ends the open sprint with `ended_by: governor` when it holds at least one task and every task in it is `accepted` or `cancelled`; `left` is then empty.
- Metrics: `Projections::metrics_for_sprint(files, sprint_id) -> Result<HarnessMetrics, MetricsError>` computes the five metrics over the rows whose `sprint` is the id and the costs whose `sprint` is the id; costs with no task are not a sprint's. `farik metrics --sprint <id>` prints them, `--json` as one object; an unknown id is refused with exit 1. The test `has_no_sprint_flag_until_sprints_exist` is replaced by the flag's tests.

## File map

```
docs/schemas/sprint.schema.json                  creates
docs/schemas/event.schema.json                   modifies: sprint.started, sprint.planned, sprint.ended
docs/schemas/command.schema.json                 modifies: sprint_start, sprint_end
crates/core/src/sprint.rs, crates/core/src/lib.rs, crates/core/src/generated/mod.rs   creates / modifies: Sprint, validate_sprint
crates/store/src/files.rs                        modifies: read_sprint, write_sprint, list_sprints; tests
crates/store/src/projections.rs, crates/store/src/migrations.rs, crates/store/src/migrations/0008_sprints.sql   modifies / creates
crates/store/src/metrics.rs                      modifies: metrics_for_sprint
crates/protocol/src/event.rs, crates/protocol/src/lib.rs, crates/protocol/src/event/fixtures.rs, crates/protocol/src/command.rs   modifies
crates/runtime/src/sprints.rs, crates/runtime/src/lib.rs   creates / modifies: start_sprint, plan_sprint, end_sprint
crates/runtime/src/cost.rs, crates/runtime/src/transitions.rs   modifies: the sprint budget
crates/runtime/src/tools.rs, crates/runtime/src/tools/contracts.rs, crates/runtime/src/tools/refusal.rs   modifies: farik_plan_sprint; a child joins its epic's sprint
crates/runtime/src/orchestrator/{rules,requests,human,messages,session}.rs   modifies: the sprint rules, assignment within a sprint, the commands, sprint_plan_message, SessionAsk.contract optional, no abort on SprintUsd
crates/runtime/src/orchestrator.rs              modifies: TickReport::Sprint
crates/core/src/governor/gates.rs               modifies: AssignmentInput's sprint fields and their check
crates/runtime/src/daemon/mcp.rs                modifies: the tool count
crates/cli/src/run.rs, crates/cli/src/task.rs    modifies: printing TickReport::Sprint; a child joins its epic's sprint
crates/runtime/src/recorded/transcripts/plan_sprint_frk_1.jsonl   creates
crates/cli/src/lib.rs, crates/cli/src/sprint.rs, crates/cli/src/metrics.rs   modifies / creates
crates/cli/tests/reading.rs, crates/cli/tests/human.rs   modifies: sprint show, metrics --sprint, sprint start and end
docs/SPEC.md, docs/plans/project-plan.md         modifies
```

## Interfaces

Consumes: `ProjectFiles`, `Projections`, `EventLog`, `Transitions`, `budget_state`, `check_assignment`, `Command`, `human::handle`, `start::command`, the tools' `Call` (on main and steps 01 and 02).

Produces:

```rust
// farik-core::sprint (from the schema): Sprint { id, started_at, ended_at, budget_usd, task_ids, status }, SprintStatus { Open, Ended }
pub fn validate_sprint(input: &Value) -> Result<Sprint, Vec<ValidationError>>;
// farik-store
impl ProjectFiles { pub fn read_sprint(&self, id: &str) -> Result<Sprint, FilesError>; pub fn write_sprint(&self, sprint: &Sprint) -> Result<(), FilesError>; pub fn list_sprints(&self) -> Result<Vec<Sprint>, FilesError>; }
pub struct SprintProjection { pub sprint_id: String, pub budget_usd: Option<f64> }
impl Projections { pub fn open_sprint(&self) -> Result<Option<SprintProjection>, StoreError>; pub fn metrics_for_sprint(&self, files: &ProjectFiles, sprint_id: &str) -> Result<HarnessMetrics, MetricsError>; }
pub enum CostScope { Task, Agent, Session, Day, Sprint }
pub struct TaskProjection { /* as before */ pub sprint: Option<String> }
// farik-protocol: EventBody::{SprintStarted, SprintPlanned, SprintEnded}; Command::{SprintStart { budget_usd: Option<f64> }, SprintEnd}
// farik-runtime::sprints
pub fn start_sprint(deps: &ToolDeps, budget_usd: Option<f64>, started_by: &str) -> Result<Sprint, SprintError>;
pub enum PlannedBy { Assigner(String), Governor }
pub enum EndedBy { Governor, Human }
pub fn plan_sprint(deps: &ToolDeps, task_ids: &[TaskId], planned_by: &PlannedBy) -> Result<Sprint, SprintError>;
pub fn end_sprint(deps: &ToolDeps, ended_by: EndedBy) -> Result<Sprint, SprintError>;
pub enum SprintError { AlreadyOpen { sprint_id: String }, NoneOpen, Refused { reason: String }, Files(FilesError), Store(StoreError) }
```

## Tasks

### Task 1: the sprint file

Files: the schema, `crates/core/src/sprint.rs`, `lib.rs`, `generated/mod.rs`, `crates/store/src/files.rs`
- `validates_a_sprint` (core) — an open sprint with no `ended_at` and no budget validates; `budget_usd: 0`, `id: "S0"`, and an unknown key are each refused naming the field.
- `writes_and_reads_a_sprint` (store) — `write_sprint` then `read_sprint("S1")` gives it back equal; `.farik/sprints/` did not exist before.
- `lists_sprints_by_number` (store) — S2, S10, S1 written: `list_sprints` answers S1, S2, S10.
- `refuses_a_sprint_file_that_breaks_its_schema` (store) — a hand-written file with `status: closed`: `read_sprint` fails naming `status`.

- [x] `feat(core): hold a sprint to its schema and keep it in .farik/sprints`

### Task 2: the events and the projections

Files: the event schema, `event.rs`, `lib.rs`, `event/fixtures.rs` (protocol), `projections.rs`, `migrations.rs`, `0008_sprints.sql`
- `projects_the_open_sprint` — `sprint.started` S1 with 20 dollars: `open_sprint()` is S1 with `Some(20.0)`; after `sprint.ended` S1: `None`.
- `puts_a_task_in_a_sprint_and_takes_it_out` — `sprint.planned` S1 [FRK-1, FRK-2], then `sprint.ended` S1 left [FRK-2]: FRK-1's `sprint` is `Some("S1")`, FRK-2's `None`.
- `keeps_a_cost_with_the_sprint_it_was_spent_in` — a cost of FRK-1 while in S1, then FRK-1 left, then a second cost: `costs(CostScope::Sprint)` has S1 with the first cost alone.
- `rebuilds_the_sprints` — `Projections::rebuild` after sprint events: `open_sprint()` and each row's `sprint` are what they were.
- `replays_the_sprints_after_the_migration` — a log holding sprint events, opened at migration 0007 then migrated: the projections equal a fresh replay.
- `names_every_event_kind_as_an_entity_and_a_past_tense_verb` (protocol, existing) — passes with 35 kinds.

- [x] `feat(store): project sprints, a task's sprint, and what each sprint spent`

### Task 3: starting and ending a sprint

Files: `command.schema.json`, `command.rs`, `crates/runtime/src/sprints.rs`, `runtime/src/lib.rs`, `orchestrator/human.rs`, `orchestrator/rules.rs`, `crates/cli/src/lib.rs`, `crates/cli/src/sprint.rs`, `crates/cli/tests/human.rs`, `crates/cli/tests/reading.rs`
- `starts_a_sprint` — `SprintStart { budget_usd: Some(20.0) }`: S1's file is open with 20 dollars, `sprint.started` recorded by `human`.
- `numbers_a_sprint_after_the_files_and_the_log` — with `S3.yaml` written (ended): the next start is S4; with no file but a `sprint.started` S5 in the log: S6.
- `refuses_a_second_open_sprint` — a start while S1 is open: `Refused` naming S1; nothing written.
- `ends_a_sprint_leaving_its_unfinished_tasks` — S1 holds FRK-1 `accepted` and FRK-2 `in_progress`; `SprintEnd`: file `ended`, `sprint.ended { ended_by: human, left: [FRK-2] }`, FRK-2's contract has no `sprint` and its status is unchanged.
- `ends_a_finished_sprint_by_itself` — S1 holds FRK-1 `accepted` and FRK-2 `cancelled`: the tick ends it with `ended_by: governor` and `left: []`, and reports it.
- `leaves_an_empty_sprint_open` (guard) — S1 open with no task: the tick does not end it.
- `starts_and_shows_a_sprint_from_the_command_line` (cli) — `farik sprint start --budget 20` then `farik sprint show`: prints `S1`, `open`, `budget $20`, `spent $0`; `farik sprint end` then `show`: `ended`.
- `says_there_is_no_sprint_yet` (cli) — `farik sprint show` in a new project prints `no sprint yet`, exit 0.

- [x] `feat(runtime): start and end a sprint`

### Task 4: planning a sprint

Files: `tools.rs`, `tools/contracts.rs`, `tools/refusal.rs`, `daemon/mcp.rs`, `sprints.rs`, `orchestrator/rules.rs`, `orchestrator/session.rs`, `orchestrator/messages.rs`, `orchestrator.rs`, `cli/src/run.rs`, `cli/src/task.rs`, the transcript
- `closes_a_sprint_planning_session_with_its_own_instruction` — the planning session's prompt ends with `SPRINT_PLAN_INSTRUCTION` and names no `farik_assign_task`.
- `plans_ready_tasks_into_the_sprint` — the Scrum Master's `farik_plan_sprint [FRK-1, FRK-2]` in open S1 (no budget): both contracts' `sprint` is S1, the file lists both, `sprint.planned` by the Scrum Master.
- `refuses_a_plan_past_the_sprint_budget` — S1 with 10 dollars, FRK-1 at 6 already in it, FRK-2 at 5: `sprint_plan_refused` naming the budget; nothing written.
- `refuses_a_task_that_is_not_ready` / `refuses_a_task_with_a_parent` / `refuses_a_task_already_in_a_sprint` — each `sprint_plan_refused` naming the task and why.
- `refuses_a_plan_by_anyone_but_the_assigner` — a Developer, and the Product Manager when a Scrum Master is active: refused.
- `asks_the_assigner_to_plan_an_empty_sprint` — S1 open and empty, FRK-1 `ready`: the tick starts the Scrum Master's `plan` session whose first message lists FRK-1 and its `max_cost_usd`; after the replay, FRK-1 is in S1.
- `asks_no_plan_of_a_sprint_that_holds_a_task` (guard) — S1 holding FRK-1, FRK-2 `ready` and in no sprint: no planning session.
- `plans_a_sprint_once` — S1 open and empty, the planning session's replay plans nothing: the next tick starts no second planning session and reports nothing for S1.
- `refuses_a_second_plan` — S1 holding FRK-1: the assigner's `farik_plan_sprint [FRK-2]` is `sprint_plan_refused` naming S1.
- `plans_an_epics_tasks_with_it` — epic FRK-1 `ready` with no task yet, then planned; and epic FRK-3 `in_progress` with FRK-4 under it, both in no sprint: planning FRK-1 puts only FRK-1 in S1; FRK-4 stays out and is still assigned while S1 is open (its epic is in no sprint).
- `joins_a_task_the_human_files_under_a_sprints_epic` (cli) — epic FRK-1 in S1, `farik task create <file> --parent FRK-1`: the new task's `sprint` is S1.
- `prints_a_sprint_line_for_a_planning_session` (cli, `farik plan` with the recorded adapter) — the output holds `S1: ` and the session's words.
- `puts_an_epics_new_task_in_its_sprint` — epic FRK-1 in S1, its assignee files FRK-2 under it: FRK-2's `sprint` is S1, `sprint.planned` by `governor`.

- [ ] `feat(runtime): plan a sprint from the ready backlog`

### Task 5: assignment within a sprint and its budget

Files: `cost.rs`, `transitions.rs`, `gates.rs`, `orchestrator/rules.rs`, `orchestrator/requests.rs`, `orchestrator/session.rs`
- `assigns_only_the_open_sprints_tasks` — S1 open holding FRK-2; FRK-1 and FRK-2 `ready`: the tick's `plan` session's first message offers FRK-2 and not FRK-1.
- `refuses_an_assignment_outside_the_open_sprint` (core, gates.rs) — `open_sprint: Some("S1")`, `task_sprint: None`, `parent_sprint: None`: refused naming S1; with `parent_sprint: Some(None)`: not refused on this rule.
- `assigns_the_backlog_without_a_sprint` (guard) — no sprint: FRK-1 `ready` is offered as in phase 3.
- `fills_the_sprint_budget_from_the_open_sprint` — S1 with 10 dollars, 4 spent in it: `budget_state`'s `sprint_max_usd` 10 and `sprint_spent_usd` 4; with no sprint, infinite and 0.
- `checks_readiness_against_the_contracts_own_sprint` — S1 with 10 dollars, 8 spent, a breakdown's task of 3 in S1: readiness refused on `BudgetWithinSprint`; a standalone contract of 3 in no sprint, S1 open: not refused on that rule; a daily budget of 1 dollar left no longer refuses either on that rule.
- `refines_a_contract_larger_than_the_open_sprints_remainder_once` (guard) — S1 open with 1 dollar left, a written standalone contract of 5 `refining`: the governor moves it to `ready`; no readiness failure and no second refine session.
- `stops_assigning_once_the_sprint_budget_is_spent` — S1 with 5 dollars, 5 spent, FRK-2 `ready` in S1: no session, no move; an `in_progress` FRK-1 of S1 still gets its implement session.
- `lets_a_session_finish_past_the_sprint_budget` — a session whose reported usage crosses S1's budget: it is not aborted (`session.ended` reason `completed`).
- The transitions test that pinned the daily stand-in (`remaining_sprint_budget_usd` 17.5, and 0 on a spent day, transitions.rs ~1748) is changed to the sprint's remainder; `lists_every_tool_with_its_tier` (tools.rs) and the MCP server's tool count (21) change with the new tool.

- [ ] `feat(runtime): assign within the open sprint and within its budget`

### Task 6: metrics per sprint

Files: `crates/store/src/metrics.rs`, `crates/cli/src/metrics.rs`, `crates/cli/src/lib.rs`, `crates/cli/tests/reading.rs`
- `measures_one_sprint` (store) — S1 holding FRK-1 (accepted first pass) and S2 holding FRK-2 (accepted after a rejection): S1's first-pass rate 1.0 and S2's 0.0; each sprint's cost is its own.
- `prints_the_metrics_of_a_sprint` (cli) — `farik metrics --sprint S1` prints the five metrics for S1; `--json` one object; `--sprint S9` exits 1 naming S9. Replaces `has_no_sprint_flag_until_sprints_exist`.

- [ ] `feat(cli): print the harness metrics of one sprint`

### Task 7: the spec

Revision 0.12 in the header naming each change: section 3 (a sprint is started by the human, planned by the assigner, ends by itself or by the human; `farik sprint` and `farik metrics --sprint` among the commands); 5.3 (the remaining sprint budget is the open sprint's); 5.5 (the sprint budget from the sprint file, what it stops, and a cost staying with its sprint); 5.11 (the governor writes `sprint` when a task is planned or leaves); 6.2 (the Scrum Master plans the sprint); 8.4 (`.farik/sprints/`); 8.5 (the three kinds); F17 (per sprint). The step's interface line in the project plan is written as landed.

- [ ] `docs(docs): record sprints in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
