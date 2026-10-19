# Standalone: the team works towards the owner's goal

Status: ready
Branch: `feat/team-goals` (its own pull request to `main`)
Design: `docs/design/team-goals.md`; ADR 0075
Spec: `docs/SPEC.md` 3, 4.2, 5.5, 5.18 (new), 6.1, 8.2, 8.5, F17, F18 (new) (revision 0.130)
Depends on: `origin/main` at `ca21251b`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-19, against the three readiness rules (ADR 0065); see the end

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008). Longer than the usual 300 lines because the goal is one change across the protocol, core, store, runtime, roles and the web, each task small; split it only by landing tasks 1 to 8 before task 9.

## Goal

The founder's brief of 2026-10-19 (design, "The brief"). When done:
- the owner sets one goal on Today: words, an optional number and unit, and how it is counted (from a connected service or by the owner);
- while the board is quiet, the Product Manager, or the one agent in solo, gets a `goal` session that files the next one to five steps;
- the agent holding the goal's service reads it once a day;
- Today shows the progress, and the owner pauses, edits, ends or marks the goal reached;
- the daily budget is the only spend limit.

Out of scope: a goals page, several goals, deadlines, a goal budget, a `catervas goal` command, Catervas merging.

## Decisions

Every product decision is in ADR 0075 and the design; this list fixes the rest.
- Event body field names: `goal` (the id, pattern `^goal-[1-9][0-9]*$`), `words` (1 to 300), `target` (number, `exclusiveMinimum: 0`), `unit` (1 to 40), `server` (1 to 64), `value` (number, `minimum: 0`), `note` (1 to 300), `filed` (array of task ids, at most 5).
- The outcome enum is `reached | dropped`, and the source enum is `connector | owner`.
- The command body schemas are named `goalSetBody`, `goalEditBody`, `goalIdBody` (pause, resume), `goalEndBody` and `goalReportBody` in `command.schema.json`. The event schema's `goalSetBody` is another file's and another module's (`generated::event`), as `taskCreatedBody` is beside the command's `taskCreateBody`.
- `goal.set` and `goal.measured` from the owner carry no agent on the envelope. `goal.measured` from a tool carries the agent and the session. `goal.planned` carries the planner and the session that ran.
- `task.created.goal` is optional and written only by `file_goal_request`.
- Goal state is folded from the log on every read, as deliveries are (`deliveries.rs`); there is no projection table.
- A reading is "today" by its UTC date, as the daily budget's day is (5.5).
- The planner's session uses the agent's own model and effort, the `_` arm of the model match. A plan is leverage, as refining is.
- `SessionPurpose::Goal` gets the agent's connectors (`gives_connectors`), is `read_only: true`, has no contract, and is a work session for `Work::claim`, not a chat fork.
- `goal_get`'s `planning` is true while the newest `session.started { purpose: goal }` of the planner has no `session.ended`.
- `guessTarget` (web) reads the first number in the words:
  - digits, with `,` thousands and one optional `.` decimal part, and an optional `k` (×1,000) or `m` (×1,000,000) directly after, in either case;
  - its unit is the text after it up to the first `,`, `.`, " and ", " by ", " in ", " within " or the end, trimmed and cut to 40 characters;
  - no number, or an empty unit, is `null`.
- No goal events are shown in a task's history. `What moved` and the channel are unchanged.

## File map

```
docs/schemas/event.schema.json            modifies: six kinds and bodies; taskCreatedBody.goal; purpose "goal" in sessionStarted and costRecorded (T1)
crates/protocol/src/event.rs, event/fixtures.rs, lib.rs   modifies: the kinds (KINDS 122 -> 128) (T1)
crates/store/src/projections.rs           modifies: passes over the six kinds (T1)
crates/runtime/src/session.rs             modifies: SessionPurpose::Goal (T1)
crates/runtime/src/sessions.rs, cost.rs, orchestrator/recover.rs, orchestrator/session.rs (gives_connectors), prompt.rs (its instruction)   modifies (T1)
crates/core/src/goal.rs                   creates: the fold, the status, board_quiet; mod tests (T2)
crates/core/src/lib.rs                    modifies: pub mod goal (T2)
crates/store/src/requests.rs              modifies: Filing.goal, file_goal_request, goal_of; mod tests (T3)
docs/schemas/command.schema.json          modifies: six commands, five bodies (T4)
crates/protocol/src/command.rs            modifies: Command::Goal*; mod tests (T4)
crates/runtime/src/goals.rs               creates: goal_book, planner, reader, the owner's commands (T4); goal_get (T6)
crates/runtime/src/lib.rs                 modifies: mod goals (T4)
crates/runtime/src/orchestrator/human.rs  modifies: six dispatch arms; mod tests (T4)
crates/runtime/src/tools/goals.rs         creates: the two tools; mod tests (T5)
crates/runtime/src/tools.rs               modifies: mod goals, TOOLS entries, dispatch arms (T5)
crates/runtime/src/orchestrator/goal.rs   creates: goal_rules, the two messages; mod tests (T7)
crates/runtime/src/orchestrator.rs        modifies: mod goal, TickReport::Goal (T7)
crates/runtime/src/orchestrator/rules.rs  modifies: calls goal_rules before idle; its TickReport match (T7)
crates/runtime/src/orchestrator/requests.rs   modifies: the given refine message names the goal (T7)
crates/cli/src/run.rs                     modifies: prints TickReport::Goal (T7)
docs/schemas/rpc.schema.json              modifies: goal.get query and goalGetResult (T6)
crates/runtime/src/daemon/gates.rs        modifies: "goal.get" arm; mod tests (T6)
crates/roles/roles/product_manager/skills/working-towards-the-goal/SKILL.md   creates (T8)
crates/roles/roles/product_manager/kit.yaml, system.md; crates/roles/src/kit.rs   modifies (T8)
apps/web/src/pages/GoalCard.tsx, GoalCard.module.css, GoalCard.test.tsx   creates (T9)
apps/web/src/pages/Today.tsx, apps/web/src/strings/en.ts   modifies (T9)
docs/SPEC.md                              modifies: revision 0.130 (T10)
```

## Interfaces

Consumes (on `main`):
- `EventLog`, `EventQuery`, `StoreError`, `TaskProjection`, `request_from_brief`, `placeholder_budget_usd`, `Filing`, `file` (`catervas-store`);
- `Team`, `Agent`, `TaskStatus`, `is_terminal` (`catervas-core`);
- `run_session`, `SessionAsk`, `day_is_spent`, `unavailable`, `Waiting`, `ran`, `TickScope`, `TickRules`, `CommandReport`, `CommandError`, `ToolDeps` (`catervas-runtime`);
- `given_request` (`chat.rs`), `Call`, `ToolError::Refused`, `tool::<I>` (`tools.rs`);
- `Harness`, `an_agent_wire` (test fixtures); `InfoTip`, `Button` (`@catervas/ui`); `useQuery`, `useCommand`.

Produces:

```rust
// catervas-core::goal
pub const GOAL_ROUNDS_BEFORE_CHECK_IN: usize = 5;
pub const GOAL_WORK_PER_SESSION: usize = 5;
pub enum GoalOutcome { Reached, Dropped }
pub enum ReadingSource { Connector, Owner }
pub enum GoalRecord {
    Set { goal: String, words: String, target: Option<f64>, unit: Option<String>, server: Option<String> },
    Paused { goal: String }, Resumed { goal: String },
    Ended { goal: String, outcome: GoalOutcome },
    Measured { goal: String, value: f64, source: ReadingSource, server: Option<String>, agent: Option<String> },
    Planned { goal: String, filed: usize },
}
pub struct Reading { pub value: f64, pub source: ReadingSource, pub server: Option<String>, pub agent: Option<String>, pub at: DateTime<Utc> }
pub struct Goal { pub id: String, pub words: String, pub target: Option<f64>, pub unit: Option<String>,
    pub server: Option<String>, pub paused: bool, pub readings: Vec<Reading> /* oldest first */,
    pub rounds: usize, pub rounds_since_progress: usize, pub nothing_planned: bool }
pub enum StallReason { NothingPlanned, NoProgress }
pub enum GoalStatus { Active, Paused, Met, Stalled(StallReason) }
pub struct EndedGoal { pub id: String, pub words: String, pub outcome: GoalOutcome, pub at: DateTime<Utc> }
pub struct GoalBook { pub standing: Option<Goal>, pub last_ended: Option<EndedGoal>, pub next_id: String }
pub fn fold_goals(records: &[(DateTime<Utc>, GoalRecord)]) -> GoalBook;
impl Goal { pub fn status(&self) -> GoalStatus; pub fn latest(&self) -> Option<&Reading>; }
pub fn board_quiet(statuses: impl IntoIterator<Item = TaskStatus>) -> bool;

// catervas-store::requests
pub fn file_goal_request(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str, goal: &str,
    size: RequestTriagedBodySize, given_to: Option<&str>, now: DateTime<Utc>, ids: &EventIds)
    -> Result<TaskContract, RequestError>;
pub fn goal_of(log: &EventLog, task: &TaskId) -> Result<Option<String>, StoreError>;

// catervas-protocol::command::Command
GoalSet { words: String, target: Option<f64>, unit: Option<String>, server: Option<String> },
GoalEdit { goal: String, words: String, target: Option<f64>, unit: Option<String>, server: Option<String> },
GoalPause { goal: String }, GoalResume { goal: String },
GoalEnd { goal: String, outcome: GoalEndBodyOutcome }, GoalReport { goal: String, value: f64 },

// catervas-runtime::goals
pub fn goal_book(log: &EventLog) -> Result<GoalBook, StoreError>;
pub(crate) fn planner(team: &Team) -> Option<&Agent>;   // the active Product Manager; in solo the one active agent
pub(crate) fn reader<'a>(team: &'a Team, server: &str) -> Option<&'a Agent>;  // first active agent with it in mcp_servers
pub(crate) fn set_goal(tools: &ToolDeps, team: &Team, edit: Option<&str>, words: &str, target: Option<f64>,
    unit: Option<&str>, server: Option<&str>) -> Result<CommandReport, CommandError>;
pub(crate) fn pause_goal(tools: &ToolDeps, goal: &str) -> Result<CommandReport, CommandError>;
pub(crate) fn resume_goal(tools: &ToolDeps, goal: &str) -> Result<CommandReport, CommandError>;
pub(crate) fn end_goal(tools: &ToolDeps, goal: &str, outcome: GoalOutcome) -> Result<CommandReport, CommandError>;
pub(crate) fn report_goal(tools: &ToolDeps, goal: &str, value: f64) -> Result<CommandReport, CommandError>;
pub(crate) fn goal_get(tools: &ToolDeps, team: &Team) -> Result<Value, StoreError>;   // T6

// catervas-runtime::tools::goals
pub(crate) struct FileGoalWorkInput { title: String, request: String, size: GoalWorkSize }  // deny_unknown_fields
pub(crate) enum GoalWorkSize { Large, Small }                                              // snake_case
pub(crate) struct RecordGoalProgressInput { value: f64, note: Option<String> }            // deny_unknown_fields
pub(super) fn file_goal_work(call: &Call<'_>, input: FileGoalWorkInput) -> Result<Value, ToolError>;
pub(super) fn record_goal_progress(call: &Call<'_>, input: RecordGoalProgressInput) -> Result<Value, ToolError>;

// catervas-runtime::orchestrator
TickReport::Goal { goal_id: String, what: String },
pub(super) const GOAL_PLAN_TOOLS: &[&str];    // read_task, read_board, read_rules, read_criteria, read_decisions,
                                              // post_message, write_memory, file_goal_work, record_goal_progress
pub(super) const GOAL_MEASURE_TOOLS: &[&str]; // record_goal_progress
pub(super) async fn goal_rules(deps: &OrchestratorDeps, scope: &TickScope, team: &Team, board: &[TaskProjection],
    waiting: &mut Waiting) -> Result<Option<TickReport>, OrchestratorError>;
```

```ts
// apps/web/src/pages/GoalCard.tsx
export type GoalReading = { value: number; source: "connector" | "owner"; server: string | null; agent: string | null; at: string };
export type Goal = { goal: string; words: string; target: number | null; unit: string | null; server: string | null;
  reader: string | null; planner: string | null; status: "active" | "paused" | "met" | "stalled";
  stall: "nothing_planned" | "no_progress" | null; readings: GoalReading[]; open: number; done: number;
  waitingOnOwner: number; planning: boolean };
export type GoalState = { goal: Goal | null; lastEnded: { goal: string; words: string; outcome: "reached" | "dropped"; at: string } | null };
export function guessTarget(words: string): { target: number; unit: string } | null;
export function GoalCard(props: { state: GoalState; team: Team; again: () => void }): JSX.Element;
```

Wire of `goal.get`: `{ goal: null | { goal, words, target, unit, server, reader, planner, status, stall, readings (newest first, at most 10), open, done, waiting_on_owner, planning }, last_ended: null | { goal, words, outcome, at } }`.
- `open` counts the board's tasks that are not accepted or cancelled.
- `done` counts the accepted tasks whose `task.created.goal` is the goal.
- `waiting_on_owner` counts the open tasks that are `escalated`, `blocked` or waiting on the human.

## Tasks

### Task 1: the events and the `goal` purpose

Files as the map's T1 lines. `SessionPurpose::Goal`'s wire is `goal`. Its "This session" line in `prompt.rs` reads: "This session works towards the owner's goal; the first message says what to do. Change nothing in the project. Then end the session." Every exhaustive match the compiler names on the new kinds, or on the purpose, passes them over or maps them one to one.

- `reads_the_goal_events` (`event.rs` `mod tests`): `event_from_value` reads one fixture of each of the six kinds, with the bodies of the design's table and every optional field present. It refuses `goal.set` with an extra body field `"x": 1`. It refuses `goal.measured` with `"source": "guess"`. It reads `task.created` with `"goal": "goal-1"`, and `session.started` with `"purpose": "goal"`.

- [x] `feat(protocol): record the owner's goal, its readings and its rounds`

### Task 2: the fold

Files: `crates/core/src/goal.rs`, `lib.rs`. Rules, all in `fold_goals`:
- The standing goal is the newest `Set` id with no `Ended`. Records of other ids are ignored.
- An edit (a second `Set` of the id) replaces the words, target, unit and server, and keeps the readings and rounds.
- `rounds` counts `Planned` with `filed > 0`.
- `rounds_since_progress` resets to 0 on:
  - a `Set` or a `Resumed`;
  - a `Measured` from `Owner`;
  - a `Measured` above every earlier reading (the first reading counts).
- `nothing_planned` is true after a `Planned { filed: 0 }`. It is cleared by any later `Set`, `Resumed` or `Measured`.
- `next_id` is `goal-<1 + the highest n seen>`.

`status` is the first that holds:
1. `Paused`;
2. `Met` (the latest reading is at or above the target);
3. `Stalled(NothingPlanned)`;
4. `Stalled(NoProgress)` (`rounds_since_progress >= GOAL_ROUNDS_BEFORE_CHECK_IN`);
5. `Active`.

`board_quiet` is true when every status is terminal (`is_terminal`).

- `folds_a_goal_through_its_life`: a `Set` of goal-1 with target 10, then a `Measured` of 4 from the connector, an edit with target 12, `Paused`, `Resumed`, a `Measured` of 12 and `Ended { Reached }`. Before the end, `status` is `Met` and `readings.len()` is 2 with the target 12. After it, `standing` is `None`, `last_ended` is goal-1 `Reached`, and `next_id` is `goal-2`.
- `stalls_without_progress_and_after_an_empty_round`:
  - five `Planned { filed: 2 }` after a `Set` give `Stalled(NoProgress)`;
  - a `Measured` above the earlier ones makes it `Active`;
  - a `Planned { filed: 0 }` gives `Stalled(NothingPlanned)`, and a later `Measured` of the same value makes it `Active` again;
  - `board_quiet` is false with one `Escalated` among `Accepted` rows, and true with `Accepted` and `Cancelled` alone.

- [x] `feat(core): fold a goal's life, readings and rounds into its status`

### Task 3: filing goal work

Files: `crates/store/src/requests.rs`. `Filing` gains `goal: Option<&str>`, `None` in every existing filing. With `goal`:
- `creation_bodies` writes `task.created.goal`;
- it records `request.triaged { size, reason: "planned towards <goal>", triaged_by: "catervas" }` in the same filing;
- with `given_to`, `task.created.given_to` is set as `file_given_task` sets it, with no chat message.

`goal_of` reads `task.created.goal`.

- `files_goal_work_triaged_by_its_planner`:
  - `file_goal_request` with goal-1, `Large` and no `given_to` files CTV-1 as a draft whose `task.created` names `goal: "goal-1"` and whose `request.triaged` is `large`, `triaged_by` catervas;
  - with `Small` and `given_to: Some("theo")` it files CTV-2, whose `given_to` is `theo`;
  - `goal_of` answers goal-1 for both, and `None` for a request filed with `file_request`.

- [x] `feat(store): file a request towards the goal, triaged as its planner sized it`

### Task 4: the owner's commands

Files: `command.schema.json`, `command.rs`, `crates/runtime/src/goals.rs`, `lib.rs`, `human.rs`. Refusals are `CommandError::Refused` with the code first.
- `goal_set` checks, in order:
  - `goal_active`;
  - `goal_needs_daily_budget` (`team.budgets.daily_usd` is `None`);
  - `goal_target_unit` (one of the two without the other);
  - `goal_server_without_target`;
  - `goal_server_not_connected` (`reader` is `None`).
- `goal_edit` makes the same checks but `goal_active`, after `goal_unknown` (the goal is not the standing one).
- `goal_pause` refuses `goal_already_paused`. `goal_report` refuses `goal_not_measured` (no target) and `goal_value`.
- Each command appends its one event and reports its seq.

- `reads_the_goal_commands` (`command.rs`): reads `goal_set` with every field and with words alone, and `goal_end` with `outcome: "reached"`. It refuses `goal_report` without `value`, and `goal_set` with an extra `"goal"`.
- `the_owner_sets_pauses_reports_and_ends_a_goal` (`human.rs`, `Harness::new` with `budgets.daily_usd` 10 and `dev-a`'s `mcp_servers` holding a server named `stripe`):
  - `goal_set { words: "Reach 10 paying customers", target: 10, unit: "paying customers", server: "stripe" }` records `goal.set` goal-1;
  - a second `goal_set` is refused `goal_active: `;
  - `goal_pause`, then `goal_pause` again, is refused `goal_already_paused: `;
  - `goal_report { value: 3 }` records `goal.measured { source: owner }`;
  - `goal_end { outcome: reached }` records `goal.ended`, and `goal_book`'s `standing` is `None`;
  - on a team without `daily_usd`, `goal_set` is refused `goal_needs_daily_budget: `, and with `server: "linear"` it is refused `goal_server_not_connected: `.

- [x] `feat(runtime): the owner sets, edits, pauses, ends and reports on a goal`

### Task 5: the two tools

Files: `tools/goals.rs`, `tools.rs`. Both are tier `Read`. The `TOOLS` entries go after `catervas_request_connection`:
- `catervas_file_goal_work`: "File the next step towards the owner's goal: a title, the request in plain words, and large (an epic) or small (a task). Up to five per session.";
- `catervas_record_goal_progress`: "Record the goal's number as you read it from its service, with a short note on what you counted."

`lists_every_tool_with_its_tier` changes here: two more `Read` tools after `catervas_request_connection`.

`file_goal_work`:
- checks `goal_session_only`, then `goal_none` (no standing goal, or a paused one), then `goal_work_limit` (five `task.created` with this goal and this session's `session_id` already), then `goal_large_in_solo`;
- in a team it files `request_from_brief(title, request, placeholder_budget_usd(rules))` with the planner's size;
- in solo it files `given_request(team, agent, request, rules)` with `title` set, `given_to` the caller, small;
- it answers `{ task_id, status: "draft", left }`.

`record_goal_progress`:
- checks `goal_session_only`, then `goal_none`, then `goal_not_measured`, then `goal_not_your_service` (the caller's `mcp_servers` lack the goal's `server`), then `goal_value`;
- it records `goal.measured { source: connector, server, note }`;
- it answers `{ goal, value }`.

- `files_work_and_records_progress_in_a_goal_session` (`TestProject` of `a_team_of_three`, `budgets.daily_usd` 10, `dev-a` holding `stripe`, and goal-1 recorded as a `goal.set` event, target 10, unit "paying customers", server `stripe`):
  - `pm` in a `Goal` session files twice, and the board holds two drafts naming goal-1;
  - a sixth call in one session is refused `goal_work_limit: `;
  - in an `Implement` session the call is refused `goal_session_only: `;
  - `pm` recording is refused `goal_not_your_service: `;
  - `dev-a` recording 4 adds one `goal.measured` of 4 from the connector, its envelope's agent `dev-a`;
  - `-1` is refused `goal_value: `;
  - on a solo team of `theo` (a Developer), `large` is refused `goal_large_in_solo: `, and `small` files a task given to `theo`.

- [x] `feat(runtime): a goal session files the next steps and records the goal's number`

### Task 6: `goal.get`

Files: `rpc.schema.json` (`goal.get` in the query-name enum, `goalGetQuery`, `goalGetResult`, as the wire above, `additionalProperties: false`), `gates.rs`, `goals.rs` (`goal_get`). The test is `#[ignore = "needs the git program: cargo xtask check --integration"]`, as the other `gates.rs` tests are.

- `goal_get_answers_the_standing_goal_and_the_last_ended`:
  - with no goal it answers `{ goal: null, last_ended: null }`;
  - after `goal_set` (target 10, `stripe`), a reading of 4 by `dev-a` and one open draft, it answers `status: "active"`, `reader: "dev-a"`, `planner: "pm"`, `readings[0].value` 4, `open` 1, `planning` false;
  - after `goal_end { reached }` it answers `goal: null` and `last_ended.outcome: "reached"`.

- [x] `feat(runtime): answer the standing goal and its progress`

### Task 7: the loop

Files: `orchestrator/goal.rs`, `orchestrator.rs`, `rules.rs` (`goal_rules` called just before `Ok(idle(&waiting))`, and its match gains `TickReport::Goal { .. } => None`), `requests.rs` (the refine message of a given task with `goal_of` says "<Name>, you filed this task working towards the owner's goal “<words>”, and you write its contract yourself" in place of the chat's words), `run.rs` (prints `goal-1: <what>`).

`goal_rules` runs only under `TickRules::All` in a tick scoped to no task. It takes the planning rule first, then the measuring rule, each exactly as the design's "The loop" says.

Planning:
- the session is `SessionAsk { agent: planner, contract: None, purpose: Goal, cwd: root, read_only: true, tools: Some(GOAL_PLAN_TOOLS), .. }`, with `goal_plan_message`;
- after `run_session`, unless the session ended `EndReason::Aborted`, the rule appends `goal.planned { goal, filed }`. `filed` is the `task.created` with that goal after the session's `session.started` seq;
- it answers `TickReport::Goal { what: ran(planner, "goal planning", &end) }`.

Measuring:
- the same ask for the reader, with `GOAL_MEASURE_TOOLS` and `goal_measure_message`;
- it answers `ran(reader, "goal reading", &end)`.

A spent day sets `waiting.day_spent`, and a sleeping agent sets `waiting.slept`, as the other rules do.

`goal_plan_message` holds:
- the words, the target and unit;
- the last five readings, each `"<value> from <server|you>, <n> days ago"`;
- the reader's name and server, and, when the reader is the planner, "Read <server> first and record it with `catervas_record_goal_progress`.";
- each earlier round's tasks with their status;
- the accepted tasks awaiting a merge;
- in solo, "Each step you file is your own task; file small ones.";
- the method, the design's "What the planner is told to do" as six bullets.

- `plans_towards_the_goal_when_the_board_is_quiet` (`Harness::new` with `budgets.daily_usd` 10, goal-1 set by `goal_set` with target 10, unit "paying customers" and no server, an empty board):
  - a tick starts one session, `purpose` `Goal`, agent `pm`, no task, whose prompt contains "Reach 10 paying customers";
  - `goal.planned { goal: "goal-1", filed: [] }` is recorded;
  - the next tick starts no session (nothing planned and nothing new);
  - after `goal_report { value: 2 }` a tick starts the second;
  - with an `accepted` task waiting for its hand merge (`harness.accepted("CTV-2")`, the project having no `origin`, 5.14) and nothing else, a `Goal` session still starts (the merge does not hold a round);
  - with one young `blocked` task (`harness.blocked("CTV-1", "dev-a", "dev-b")`, which rule 4 passes over), no `Goal` session starts;
  - with that task cancelled and after `goal_pause`, no `Goal` session starts;
  - a goal session whose recorded transcript ends aborted leaves no `goal.planned`, and the next tick starts one again.
- `measures_the_goal_once_a_day` (`dev-a` holding `stripe`, goal-1 with `server: "stripe"`, one young `blocked` task, `harness.blocked("CTV-1", "dev-b", "pm")`, so the board is not quiet and no other rule acts):
  - the tick starts one `Goal` session of `dev-a`, offered `catervas_record_goal_progress` alone;
  - a second tick the same day starts no `Goal` session of `dev-a`;
  - an orchestrator from `harness.orchestrator_at` one day later starts one again.
- `one_agent_alone_plans_its_own_goal` (solo `theo`, `budgets.daily_usd` 10, goal-1): the tick starts a `Goal` session of `theo` whose prompt contains "Each step you file is your own task".

- [x] `feat(runtime): the planner plans towards the goal whenever the board is quiet, and its reader reads it daily`

### Task 8: the Product Manager's skill

Files: `skills/working-towards-the-goal/SKILL.md` (a `name`; a `description` starting "Use when a goal session asks you for the next steps towards the owner's goal" and naming `catervas_file_goal_work`; numbered sections for reading the progress, picking one to five steps, sizing, what never to file again, asking for a connection, and filing nothing; under 6 KB), `kit.yaml` (listed before `catervafying-the-repository`), `kit.rs` (`embedded!`), and `system.md` (one line under "Your mandate": "When the owner has set a goal, you plan the team's next steps towards it whenever the board is quiet: follow working-towards-the-goal.").

- `product_manager_kit_carries_its_skills` (extended): the names are the six of today with `working-towards-the-goal` inserted before `catervafying-the-repository`. That skill's description starts "Use when a goal session" and contains `catervas_file_goal_work`.

- [x] `feat(roles): the Product Manager plans towards the owner's goal`

### Task 9: the goal card on Today

Files: `GoalCard.tsx`, `GoalCard.module.css`, `GoalCard.test.tsx`, `Today.tsx` (asks `goal.get`; renders `GoalCard` after `RequestBox` and before the Catervafication card), `en.ts` (the design's words, each its own key). It follows the design's "What the owner sees" exactly, with brand tokens only, and works at 1280 and 390 px.

- `sets_a_goal_with_the_number_guessed_from_its_words`:
  - `guessTarget("Create a mobile application and reach 10k downloads")` is `{ target: 10000, unit: "downloads" }`, `guessTarget("Reach $5,000 MRR by March")` is `{ target: 5000, unit: "MRR" }`, and `guessTarget("Make the app")` is `null`;
  - in the dialog, typing "Reach 10 paying customers" fills 10 and "paying customers";
  - choosing "From Stripe (Finn reads it)" and Set goal sends `goal_set { words, target: 10, unit: "paying customers", server: "stripe" }`.
- `shows_progress_and_asks_for_the_owners_word`:
  - a `met` goal, 10 of 10, shows "Your team counts 10 of 10 paying customers. Is the goal reached?";
  - Mark reached, confirmed, sends `goal_end { goal: "goal-1", outcome: "reached" }`;
  - a `stalled` goal with `no_progress` shows "Five rounds of work and the count hasn't moved." and Keep going sends `goal_resume`;
  - `goal: null` with a `last_ended` shows "Set a goal" and "Reached on".

- [x] `feat(web): set the team's goal and follow it on Today`

### Task 10: the spec

`docs/SPEC.md` revision 0.130, one paragraph at the head as the others are. Each changed paragraph carries "(added in 0.130; ADR 0075)" or "(changed in 0.130; ADR 0075)":
- 3: the concept of a goal;
- 4.2: the goal card;
- 5.5: the goal adds no budget, and its loop's two bounds are not budgets;
- 5.18 "Goals" (new): the whole of the design's mechanism, and 5.16's note that a goal request is triaged at filing, with no draft window to overrule it;
- 6.1: the planner and the skill, eight kit skills;
- 8.2: the goal rules come last in a tick;
- 8.5: six kinds, 128 in all (122 on `origin/main` at `ca21251b`; main's count plus six if it has moved), `task.created.goal`, the purpose `goal`;
- F17: `goal` in the split by purpose;
- F18: Goals.

- [ ] `docs(spec): record the team's goal (0.130)`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Task 6's and Task 7's ignored tests among them)
```

The founder's own test, in the web app, on a team with a daily budget and the Finance Specialist's Stripe connected:
1. Set "Reach 10 paying customers" counted from Stripe.
2. Watch the Product Manager plan a round once the board is quiet, and Finn read Stripe that day.
3. Report a number, pause, resume, and mark the goal reached.

## Readiness (ADR 0065), checked by the planner

1. **Every decision is made.** The product decisions are in ADR 0075 (one goal, events only, the owner's commands, the two rules, filing, measuring, the bounds, outward acts, Today). The wire names, the refusal codes and their order, the answers, the constants, the prompt's contents and `guessTarget`'s rule are above. Nothing is left as "as needed".
2. **No ambiguity.** Every path is exact and every produced signature is written. Each test names its fixture, its inputs and the exact values it asserts. Compile-level detail, such as which matches the new purpose reaches, is left to the compiler (stage 2).
3. **No forward dependency.** T1 needs only `main`. T2 needs nothing new. T3 needs T1 (`task.created.goal`). T4 needs T1 and T2. T5 needs T1 to T4 (`goal_book`, `planner`, `reader`; its fixture records `goal.set` directly). T6 needs T4. T7 needs T1 to T5. T8 needs T5's tool name. T9 needs T6's wire and T4's commands. T10 comes last. No task stubs a later one.
