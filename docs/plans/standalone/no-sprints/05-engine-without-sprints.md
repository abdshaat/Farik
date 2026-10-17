# Standalone CTV-9, plan 5: sprints leave the engine, and old projects keep working

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 3, 5.2, 5.3, 5.5, 5.9, 5.12, 6.7, 8.4, 8.5, F3, F17; recorded by plan 8
Depends on: plans 1 to 4 of this folder (committed on this branch; plan 4 took the web app off the sprint wire)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

No rule, gate, query, command or file of Catervas holds work for a sprint, plans or ends one,
holds a standup, spends a sprint budget, or counts by a sprint: a ready task goes to the first free
agent of its role. A project made before this change loads and carries on: its `team.yaml` with
`plan_in_sprints` (true or false) or `ambient_messages_per_sprint`, its saved templates with
`plan_in_sprints`, its log with `sprint.started`, `sprint.planned`, `sprint.ended` (with its Backlog
mark), planning and standup sessions, and its `.catervas/sprints/` files. Nothing rewrites or deletes
a past event or those files. Out of scope: WIP counting (plan 6) and words in roles and skills (plan 7).

## Decisions

Ada's section 4 is the contract for old projects, with these corrections and additions:

- The hold goes from `catervas_core::governor::gates` (`SprintHold`, `waits_for_a_sprint`,
  `in_the_backlog`, `in_the_open_sprint`, `fits_the_open_sprint`, `the_sprint_pays`, and
  `AssignmentInput`'s `remaining_sprint_budget_usd`, `open_sprint`, `task_sprint`,
  `parent_sprint`, `plan_in_sprints`, `skips_sprints`), from `catervas_runtime::sprints`
  (`sprint_hold`, `backlog`) and from `tick` (`goes_on`, the idle "waits for a
  sprint"), all in Task 1. The Backlog's readers then answer that nothing waits until Task 4
  removes them: `daemon/gates.rs`'s `backlog.summary` answers a count of 0, `daemon/web.rs`'s row
  `backlog` is `false`, `cli/src/run.rs`'s Backlog count is 0 (its line is never printed), and
  `sprint_work`'s `in_the_backlog` is empty until Task 2 deletes `sprint_work` with
  `describe_change`'s switch lines. `Team::plans_in_sprints` goes in Task 5 (its last readers,
  `carry.rs`, `init.rs`, `template.rs` and `daemon/team.rs`, change there).
- Sprint budgets go, which Ada's plan did not name: `BudgetScope::SprintUsd` and `BudgetState`'s
  `sprint_spent_usd` and `sprint_max_usd` (`crates/core/src/budget.rs`), the readiness rule
  `BudgetWithinSprint` and `ReadinessContext.remaining_sprint_budget_usd`
  (`governor/readiness.rs`, `plain.rs`, `readiness/fixtures.rs`), and their reading in
  `runtime/src/cost.rs`, `transitions.rs`, `orchestrator/session.rs` (the `SprintUsd` arms) and
  `ceremonies.rs` (`budgets_spent_since_planning` stops reading a sprint scope in Task 1 and is
  deleted in Task 2). A team file has no sprint budget key (R2's "sprint budget" lived on
  `sprint.started` and the sprint files), so nothing in `team.yaml` changes for it.
- The sprint's life goes: `finished_sprint`, `sprint_planning`, `standup`, `PLANNING_TOOLS`, the
  tool `catervas_plan_sprint` (`tools.rs`, `tools/contracts.rs`), `start_sprint`, `end_sprint`,
  `plan_sprint`, `join_epics_sprint`, `planning_session_spent`, `is_planning` and the rest of
  `crates/runtime/src/sprints.rs` (deleted), `ceremonies::{standup_moves,
  budgets_spent_since_planning}`, `TickReport::Sprint`, the commands `sprint_start` and `sprint_end`
  (`command.schema.json`, `crates/protocol/src/command.rs`, `orchestrator/human.rs`), and
  `catervas sprint` (`crates/cli/src/sprint.rs` deleted, `lib.rs`). A Catervafication files its
  epic and opens no sprint (`catervafication::Started.sprint` goes; the owner's line is "Your team
  starts documenting the project: <epic>."). Prompts: `CEREMONY_INSTRUCTIONS` keeps review and retro
  only. `rules::sprint_rules` becomes `delivery_rules` (`close_a_delivery`, then `review_and_retro`)
  and `sprint_rules_run` becomes `delivery_rules_run`. The transcripts `planning_ceremony_ctv_1`, `_ctv_1_ctv_2`, `_ctv_1_ctv_3` and `standup` and
  their fixtures go, and `catervas-e2e-serve` stops naming them.
- The reads go (Task 4, but for the allowance period): the queries `sprint.current`, `sprints.list`, `sprint.get` and `backlog.summary`;
  the board row's `backlog` and `sprint`; `costs.summary`'s `sprint` and `agents[].sprint_usd`;
  `metrics`'s `sprint_id`; the allowance period's `sprint` branch (in Task 3, before the store's `open_sprint` goes); `catervas metrics --sprint`; the
  end-of-run line "start a sprint: ..."; `catervas_read_costs`'s `by: sprint`; the store's
  `open_sprint`, `SprintProjection`, `CostScope::Sprint`, `CostWindow::Sprint`,
  `metrics_for_sprint`, `read_sprint`, `write_sprint`, `list_sprints`; the reconciliation's
  `Drift::SprintMismatch` check; `crates/core/src/sprint.rs` (and `sprint/fixtures.rs`), its
  generated module, and `docs/schemas/sprint.schema.json`.
- What stays, so an old log reads: the event kinds `sprint.started`, `sprint.planned`,
  `sprint.ended` and their bodies, the threads `planning` and `standup`, `budget.exhausted`'s
  `sprint_usd` and `drift.detected`'s `sprint_mismatch` values, `team.updated`'s
  `plan_in_sprints`, the contract's `sprint` field, the store's `apply_sprint` and its tables and
  columns (migrations 0008 to 0010 and 0015 are never removed), each read and never written. The
  projection's `sprint`, `left_for_the_backlog` and `skips_sprints` stay and nothing reads them.
  An open sprint at the upgrade is never ended: Catervas appends no sprint event, ever again.
- `team.yaml`: `policy.plan_in_sprints` stays in `team.schema.json`, described "Read from teams made
  before ADR 0062 and ignored; Catervas no longer writes it." `policy.ambient_messages_per_day`
  (integer 0 to 20, no schema default) is added and `ambient_messages_per_sprint` loses its default
  and is described "Read from teams made before ADR 0062 as ambient_messages_per_day, when that is
  absent." `Team::ambient_messages_per_day()` answers `per_day`, else `per_sprint`, else 1.
  `Team::without_old_keys()` is new in this plan: it clears `policy.judgment` (so ADR 0061's drop
  of it in `ProjectFiles::write_team` moves into this function), clears `plan_in_sprints`, and
  moves `ambient_messages_per_sprint` into `ambient_messages_per_day` when that is absent (clearing
  `ambient_messages_per_sprint` either way). `ProjectFiles::write_team` writes
  `team.clone().without_old_keys()`, so the daemon's `write_team`, which calls it, drops them too.
  Defaults (`team/defaults.rs`), `catervas init`, `carry.rs`, `team.propose` and `team.updated`
  write neither key. `describe_change(old, new)` loses its `SprintWork` parameter and the switch's
  lines in Task 2, with `sprint_work`.
- Templates: `plan_in_sprints` leaves `team-template.schema.json` (epic C3); `template.rs` drops the
  key from a saved template's value before validating it, so a template saved earlier still loads,
  and no longer writes or applies it.
- Allowances (ADR 0037, amended by ADR 0062) and the channel's ambient allowance count per UTC day:
  `AllowancePeriod` becomes `pub struct AllowancePeriod { pub day: NaiveDate }`, counted from the
  day's start; the refusal says "today".

## File map

```
crates/core/src/governor/{gates,readiness,plain,transition,transition_table}.rs, governor/readiness/fixtures.rs, src/budget.rs   modifies: the hold and the sprint budget (Task 1)
crates/runtime/src/{transitions,cost,ceremonies}.rs, orchestrator/{rules,session}.rs, src/sprints.rs, daemon/{gates,web}.rs, crates/cli/src/run.rs   modifies: the hold out of tick and assignment; the Backlog answers nothing waits (Task 1)
crates/runtime/src/orchestrator/rules.rs              tests: the old project (Task 1)
crates/runtime/src/{sprints,catervafication,ceremonies,prompt,tools,orchestrator,lib}.rs, tools/contracts.rs, orchestrator/{rules,human,requests,session,messages}.rs, recorded/{fixtures.rs,transcripts/*}   modifies or deletes: the sprint's life (Task 2)
crates/core/src/team/describe.rs, crates/runtime/src/daemon/{team,templates}.rs   modifies: describe_change without SprintWork (Task 2)
docs/schemas/command.schema.json, crates/protocol/src/command.rs, crates/cli/src/{lib,sprint,task,run}.rs, crates/cli/src/bin/catervas-e2e-serve.rs, crates/cli/tests/commands.rs   modifies or deletes (Task 2)
crates/runtime/src/allowances.rs, daemon/hooks.rs, daemon/board.rs, tools/channel.rs, docs/schemas/rpc.schema.json (the allowance period)   modifies: by the day (Task 3)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, crates/runtime/src/daemon/{board,gates,web}.rs, tools/costs.rs, tools/fixtures.rs, orchestrator/fixtures.rs, crates/store/src/{projections,files,metrics,reconcile,lib}.rs, crates/store/tests/*.rs, crates/cli/src/{metrics,doctor,run}.rs, crates/core/src/{sprint.rs,sprint/,generated/mod.rs,lib.rs}, docs/schemas/sprint.schema.json   modifies or deletes: the reads (Task 4)
docs/schemas/team.schema.json, team-template.schema.json, crates/core/src/team.rs, team/{template,defaults}.rs, crates/store/src/files.rs, crates/runtime/src/daemon/{team,templates}.rs, tools/channel.rs, crates/cli/src/{init,carry}.rs   modifies: old team files and templates (Task 5)
```

Tests that pin removed behaviour are deleted in the task that removes it (hard rule 9 is about
failing tests, not these): in `rules.rs` every test from `ends_a_finished_sprint_by_itself` to
`plans_a_sprint_at_most_three_times`, `assigns_only_the_open_sprints_tasks`,
`stops_assigning_once_the_sprint_budget_is_spent`, `lets_a_session_finish_past_the_sprint_budget`,
`plans_a_sprint_again_after_its_planner_slept`, `waits_for_a_sleeping_sprint_planner`, the standup
tests, `holds_a_ready_task_until_a_sprint_opens` to `forgets_the_backlog_when_the_policy_turns_off`,
`lists_the_spent_budgets_in_the_digest`; in `core/governor/gates.rs` the sprint and Backlog tests;
`budget.rs`'s sprint tests; `readiness.rs`'s `refuses_a_budget_above_the_remaining_sprint_budget`
and `fits_any_budget_in_a_sprint_with_no_limit`; `sprints.rs`'s and `tools/contracts.rs`'s plan
tests; `requests.rs`'s `plans_both_on_the_first_tick_of_the_sprint`; `human.rs`'s
`starts_a_sprint` and its sprint-planning parts; `daemon/gates.rs`'s `summarises_the_backlog`,
`leaves_the_backlog_out_of_waiting`, `answers_tries_and_the_sprint`'s sprint part;
`daemon/web.rs`'s `marks_backlog_rows`; `daemon/board.rs`'s `answers_the_sprints`; `daemon/team.rs`'s
`describes_the_work_a_switch_touches`, `proposes_sprints_for_a_new_team`; `templates.rs`'s
`previews_the_work_a_switch_touches`; `describe.rs`'s tests of the switch's lines; the store's sprint tests in `projections.rs` but
`replays_the_sprints_after_the_migration` and `frees_a_task_left_in_an_ended_sprint_by_the_migration`
(they read old logs and stay), `tests/project_files.rs`' sprint file tests, `tests/metrics.rs`'
`measures_one_sprint`, `tests/reconciliation.rs`' sprint test; the CLI's
`starts_and_shows_a_sprint_from_the_command_line`,
`says_an_empty_sprint_whose_planning_is_spent_waits_for_its_end`,
`joins_a_task_the_human_files_under_a_sprints_epic`, `files_a_task_whose_join_fails_and_says_so`,
`prints_the_metrics_of_a_sprint`, `says_there_is_no_sprint_yet`,
`prints_a_sprint_line_for_a_planning_session`, `says_the_backlog_waits`. Tests that only set
`plan_in_sprints: false` (`crates/cli/tests/shared/project.rs`, `serving.rs`) drop the key in
Task 5. Since the Backlog answers nothing waits from Task 1, the tests that pin a non-empty
Backlog (`summarises_the_backlog`, `leaves_the_backlog_out_of_waiting`, `marks_backlog_rows`,
`says_the_backlog_waits`, the switch-off half of `describes_the_work_a_switch_touches`) are
deleted in Task 1. Each task's commit body names the tests it deleted.

## Interfaces

Consumes: plan 3's deliveries (the only ceremonies left). Produces:

```rust
// catervas_runtime::transitions (plan 6 changes what it counts)
pub(crate) fn open_tasks(board: &[TaskProjection], agent_id: &str, under: Option<&TaskId>) -> u32;
// catervas_runtime::orchestrator::rules (plan 6 reads it)
pub(super) fn has_room(team: &Team, board: &[TaskProjection], agent: &Agent, under: Option<&TaskId>) -> bool;
// catervas_core::team
#[must_use] pub fn describe_change(old: &Team, new: &Team) -> Vec<String>;   // Task 2
// catervas_core::team
impl Team {
    #[must_use] pub fn ambient_messages_per_day(&self) -> u64;
    #[must_use] pub fn without_old_keys(self) -> Team;
}
// catervas_runtime::allowances
pub struct AllowancePeriod { pub day: NaiveDate }
```

## Tasks

### Task 1: nothing holds ready work, and a project made with sprints assigns its Backlog at once

Files as the map's Task 1. `transitions::open_tasks` loses `team` and `open_sprint` (it still counts
every row neither accepted nor cancelled; plan 6 changes that). `rules::assignable` checks the
private folder and the dependencies (accepted, as ADR 0061 counts them) only. The idle tick says `nothing on the board needs doing`.
What still uses the hold or the sprint budget until Task 2 removes it is cut to compile: the
planning ceremony's candidates are the `ready` rows with no parent and in no sprint, and
`sprints::fits` no longer checks a budget.

- `a_project_made_with_sprints_assigns_its_backlog_on_the_first_tick` (rules.rs, ignored "needs the
  git program", epic C2 (a)) — a `Harness` whose team sets `policy.plan_in_sprints: true`; the log
  holds, recorded with `harness.project.record`, `sprint.started { sprint_id: "S1", budget_usd: 20,
  started_by: "human" }`, `sprint.planned { sprint_id: "S1", task_ids: ["CTV-1"], planned_by: "pm" }`,
  `sprint.ended { sprint_id: "S1", ended_by: "human", left: ["CTV-1"], backlog: true }` and a
  planning `session.started`/`session.ended` pair, with CTV-1 `ready` and marked left for the
  Backlog, and `.catervas/sprints/S1.yaml` written with `std::fs`; with the transcript
  `plan_assigns_ctv_1`, the first tick runs a `plan` session about CTV-1 and CTV-1 is `assigned` to
  `dev-a` after it; every event recorded before the tick is in the log after it with the same seq,
  kind and body, no `sprint.*` event was appended, and `S1.yaml` is byte for byte as written.

- [ ] `feat(runtime): ready work is assigned at once, whatever sprint a project had`

### Task 2: no sprint is planned, stood up or ended

Files as the map's Task 2.

- `an_open_sprint_from_before_is_never_planned_ended_or_stood_up` (rules.rs, ignored as above) — a
  log holding `sprint.started` S1 and `sprint.planned` S1 `[CTV-1]` with CTV-1 moved `ready ->
  cancelled` by the human (which an old tick would end S1 for; cancelled, not accepted, so no
  integration rule acts on it and nothing is accepted for a delivery) and a `ready` CTV-2 that no
  sprint holds: with four `reads_a_file` transcripts, the first tick starts a `plan` session about
  CTV-2, and over four ticks the log gains no `sprint.*` event and no session in the `planning` or
  `standup` thread starts.
- `refuses_the_sprint_commands` (`crates/protocol/src/command.rs` `mod tests`) — the wire commands
  `{ "command": "sprint_start", "body": { "budget_usd": 20 } }` and `{ "command": "sprint_end",
  "body": {} }` are each refused by `command_from_value` (an `Err`); `catervas sprint start` exits 2 (clap: no such
  command), checked in `crates/cli/tests/commands.rs`.

- [ ] `feat(runtime): no sprint is planned, held or ended`

### Task 3: allowances and the channel count by the day

Files as the map's Task 3. This comes before the reads go (Task 4), because `allowances.rs` and
`tools/channel.rs` read the store's `open_sprint`, which Task 4 deletes. `allowance_period` and the
channel's ambient count stop reading `open_sprint` and `sprint.ended`; `rpc.schema.json`'s
`allowances.list` period loses its `sprint` variant, and `board.rs` its `AllowancePeriod::Sprint`
arm. `hooks.rs`'s `resets_with_the_sprint` and
`counts_from_the_sprints_end_on_the_day_it_ended`, and `channel.rs`'s `counts_the_allowance_per_sprint`
are deleted (`counts_a_second_post_as_ambient` only loses its `open_sprint` line); `counts_per_utc_day_with_no_sprint_open` and
`counts_the_allowance_per_day_without_a_sprint` become the two tests below. A test here that needs
a sprint from before records `sprint.started` with `record`/`record_at`, not `open_sprint`.

- `counts_by_the_utc_day_even_with_a_sprint_from_before_open` (`daemon/hooks.rs`, ignored as its
  neighbours are) — with `sprint.started` S1 recorded yesterday and no `sprint.ended`, a spending
  tool with an allowance of 1 called once yesterday and once today: today's second call asks
  (`approval_needed`); the count answered by `allowances.list` is 1 with period
  `{ "kind": "day", "day": <today> }`.
- `counts_ambient_posts_by_the_utc_day_even_with_a_sprint_from_before_open` (`tools/channel.rs`
  `mod tests`) — with `sprint.started` S1 recorded yesterday, an allowance of 1, one ambient post
  of `dev-a` yesterday and one today: a second ambient post today is refused, its reason starting
  `channel_limit: ` and ending "today".

- [ ] `feat(runtime): allowances and unprompted messages count by the day`

### Task 4: nothing reads a sprint

Files as the map's Task 4. The daemon answers every removed query with the schema's refusal of an
unknown name. The fixtures' `open_sprint` (`tools/fixtures.rs`, `orchestrator/fixtures.rs`) go
with `write_sprint`; a test that still needs an old sprint in the log records its events with
`record`.

- `refuses_the_sprint_queries` (`crates/protocol/src/rpc.rs` `mod tests`) — `query` requests named
  `sprint.current`, `sprints.list`, `sprint.get` (with `sprint_id: "S1"`) and `backlog.summary` are
  each refused by `rpc_request_from_value`; a `metrics` query with `sprint_id` is refused; a
  `taskProjection` value and a `costsSummaryResult` value with no `backlog`, `sprint` or
  `sprint_usd` pass `validator_for` of their definitions.

- [ ] `feat(protocol): the sprint queries, fields and files are gone`

### Task 5: old team files and templates

Files as the map's Task 5. `team.rs`'s test that pinned `plans_in_sprints`, `defaults.rs`'s
`defaults_to_planning_in_sprints`, `template.rs`'s `carries_the_policy_in_a_template`, init's
`writes_a_starter_team_that_plans_in_sprints` and `commands.rs`'s `plan_in_sprints` assertion in
`makes_a_project_out_of_a_repository` are deleted or lose that part. `tools/channel.rs`'s
allowance reads `Team::ambient_messages_per_day()`. `team.rs`'s assertions that
`policy.ambient_messages_per_sprint` is 1 (around lines 1960 and 2319) assert `ambient_messages_per_day()` is 1.

- `an_old_team_file_loads_and_forgets_its_sprint_keys_when_written` (`crates/store/tests/project_files.rs`)
  — a `team.yaml` with `policy.plan_in_sprints: true` and `policy.ambient_messages_per_sprint: 3`
  reads with no error, `ambient_messages_per_day()` is 3, and after `write_team` of what was read
  the file holds `ambient_messages_per_day: 3` and neither old key.
- `a_template_saved_with_plan_in_sprints_still_applies` (`crates/core/src/team/template.rs`) — a
  saved template value with `policy.plan_in_sprints: false` validates and applies to a project's
  team that has no `plan_in_sprints`, and the applied team's `policy.plan_in_sprints` is `None`; `team-template.schema.json` has
  no `plan_in_sprints` (C3's schema half, also checked as a command below).

- [ ] `feat(core): old team files and templates load, and lose their sprint keys`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
test ! -e apps/web/src/pages/SprintPage.tsx && test ! -e apps/web/e2e/sprints.spec.ts && ! grep -qi 'sprint' apps/web/src/strings/en.ts && ! grep -q 'plan_in_sprints' docs/schemas/team-template.schema.json && echo NO_SPRINTS_IN_UI
# expected: NO_SPRINTS_IN_UI   (epic C3)
```
