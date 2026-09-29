# Phase 6, step 09: Board, sprints, task detail, and costs

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.2, 5.14, 5.16, F3, F17
Depends on: steps 01 to 08 of this phase
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The user sees the team's work on a board. Lanes are named for the brand's sprint board, and each task shows who is on it and a plain status word. The board can be filtered by agent, sprint, risk, epic, and "only what is waiting on me".

Opening a task shows six things:
- its summary and checks;
- its history in plain words;
- its plan;
- its code changes;
- its notes;
- its cost by purpose.

From the task page the user can add it to the project, stop the work running on it, or cancel it.

The user can also start a sprint with an optional budget, watch it (its tasks, meetings and spending), and end it early. The Costs page shows today's and this sprint's spending per agent, what each agent is doing right now, and the harness metrics for the whole project or one sprint.

Out of scope: the channel (step 10), one-on-ones (phase 8), notifications (phase 8).

## Decisions

- **Lanes.** `laneOf(task: TaskRow): Lane` lives in `apps/web/src/app/lanes.ts`. It follows `web-ui.md`'s table, checked against spec 5.2's states in its test.
  - **Planning:** `draft`, `refining`, and an `escalated` task awaiting approval (its mark is "Waiting on you").
  - **To do:** `ready`, `assigned`.
  - **In progress:** `in_progress`, and `rejected` ("Being reworked").
  - **Stuck:** `blocked`, and any other `escalated` task ("Needs your help").
  - **Review:** `verifying`. Its mark is "Waiting on you" when step 07's waiting list names it.
  - **Done:** `accepted`. `cancelled` shows only with the "Show cancelled" filter.
  - The lane order is the mockup's: Planning, To do, In progress, Stuck, Review, Done.
  - A task's mark comes from `waiting.list` (step 07) and `team.activity`, the latter giving "Writing" or "Building" while its assignee works on it.
- **The board.**
  - The data is `tasks.list` plus `waiting.list` and `team.activity`. No new query is needed.
  - Rows show the assignee's avatar, the title, the id, and the mark. An epic is a row too, with its tasks' count ("3 parts, 1 done"). A task under an epic shows the epic's title in small text.
  - Filters are chips: Everyone and each agent; "Only what is waiting on me"; each open epic. Under "More filters": sprint (this sprint, no sprint, all), risk (low, medium, high), and "Show cancelled". The mockup has no risk or sprint chips; the phase plan asks for both, and they sit behind "More filters".
  - At the top: the sprint line and "End the sprint early", or "No sprint is running" and "Start a sprint".
  - On a phone the lanes become tabs with counts, one lane shown, with "Nothing here right now" when empty.
- **Task detail** (`/tasks/:id`). The header reads "<id> in sprint N. <assignee> is doing it, and <reviewer> reviews it. <Try i of n>". It has five tabs:
  - **Summary and checks.** The latest review or completion summary (step 08's rule); `task.checks` with "Passed", "Failed last time", or "Not run yet".
  - **History.** `task.history`, newest first, each line in plain words from step 07's `moved.since` phrasing, with the raw kind in small grey text.
  - **The plan.** `contract.get`: approval and lock state, scope, out of scope, risk in words, and the spending limit.
  - **Code changes.** `task.diff`: file and line counts, the branch (`task_branch`), and the `DiffView`.
  - **Notes.** The contract's `notes`: completion, review and escalation, each signed.

  Beside the tabs:
  - **"Cost so far".** From `task.costs`, with the limit.
  - **"Adding it to your project".** "Add to the project" (`task_integrate`), shown when the task is `accepted` and awaiting integration. Otherwise it reads "Not yet: the task has not been accepted" or "Added on <date>".
  - **"Stop work on this task".** It sends `session_stop` for the running session that `team.activity` names, and appears only while one runs.
  - **"Cancel this task".** A dialog asks for the reason, then sends `task_transition { to: cancelled, reason }`.

  Two differences from the mockup: "See the earlier version of the plan" is dropped (the log keeps no earlier contract), and "Add a note for the team" is dropped (step 10's channel is where the user speaks to the team).
- **Costs by purpose.**
  - `Projections::costs_by_purpose(task_id: &TaskId) -> Result<BTreeMap<String, f64>, StoreError>` reads `cost_records.purpose`.
  - Purposes map to plain words: `triage`, `refine` and `plan` are "Planning"; `implement` is "Building"; `verify` is "Checking"; `ceremony` and `conversation` are "Meetings and talk".
  - The query `task.costs { task_id }` answers `{ by_purpose: [{ words, usd }], total_usd, limit_usd }`.
- **Sprints.**
  - `sprints.list {}` answers `[{ sprint_id, status, started_at, ended_at, budget_usd, spent_usd, planned_by, task_count, done_count }]` from `ProjectFiles::list_sprints` and the events.
  - `sprint.get { sprint_id }` answers the same fields plus `tasks: [{ task_id, title, status }]` and `meetings: [{ thread, first_seq, at, posts }]`. The meetings come from the ceremony `message.posted` events inside the sprint's window.
  - **Start.** The dialog reads "Start sprint N". It explains that the planner plans it and names how many ready tasks there are (ready, with no parent and no sprint, from `tasks.list`). The budget is a choice: "No limit", or "Stop handing out new work after $[20.00]". "Start sprint N" sends `sprint_start { budget_usd }`. The planner is the Scrum Master, or the Product Manager when there is none, as spec 5.9 says.
  - **End early.** A confirmation says how many tasks are unfinished and that they go back on the board. "End sprint N" sends `sprint_end`.
  - **The sprint page** (`/sprints/:id`) has the mockup's header, the task list, the meetings with "Read it" links (to step 10's channel thread; until step 10 lands, the link is omitted), and the spending line.
- **Costs** (`/costs`).
  - `Projections::costs_for(scope: CostScope, window: CostWindow) -> Result<Vec<CostProjection>, StoreError>`, with `CostWindow { Day(NaiveDate), Sprint(String), All }`, gives agent × day and agent × sprint.
  - `costs.summary {}` answers `{ today_usd, daily_limit_usd, sprint: { sprint_id, spent_usd, budget_usd } | null, agents: [{ agent_id, today_usd, sprint_usd }] }`.
  - `metrics { sprint_id? }` answers `HarnessMetrics` as JSON.
  - The page reads "Today the team has spent $X" and either "with no daily limit set" or "of $L a day". Beside it:
    - the sprint line;
    - "Set a daily limit", which saves `budgets.daily_usd` through `team.save` (step 06) in a small dialog;
    - the per-agent table: Today, This sprint, and Right now (from `team.activity`).
  - "How well the team works" gives the five metrics in words, with the cost split into the purpose words and "Show sprint N only" toggling `sprint_id`. Each rate with no accepted task reads "Not yet: no work has been accepted".
- **The rail** gains Board, Team (step 06's page) and Costs, in the mockup's order: Today, Board, Channel (step 10), Team, Costs, Settings. On a phone, the bottom bar holds Today, Board, Channel, Team and Costs, with Settings under Team, as `web-ui.md` says.
- **Tests.**
  - Vitest and axe on each page. `lanes.test.ts` checks every `TaskStatus`.
  - Playwright `board.spec.ts`, with the step 08 team and transcripts `triage_frk_1_small_by_pm`, `refine_writes_task_for_theo_frk_1`, `judge_frk_1_by_architect`, `plan_sprint_frk_1`, `plan_assigns_frk_1_to_theo`, and `implement_finishes_frk_1`. It:
    1. starts a sprint with a $20 budget;
    2. sees the task move from Planning to To do to In progress to Review on the board by itself;
    3. opens the task's tabs;
    4. sees the Costs page's per-agent row;
    5. ends the sprint early and sees the task leave the sprint;
    6. takes screenshots at 360 and 1280 px.

## File map

```
crates/store/src/projections.rs (+ tests)                                   modifies: costs_by_purpose, costs_for (T1)
crates/runtime/src/daemon/web.rs, docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/client.ts   modifies: queries (T1)
apps/web/src/app/{lanes.ts,lanes.test.ts,App.tsx}, shell/{Shell.tsx,Shell.test.tsx}, strings/en.ts   creates / modifies (T2)
apps/web/src/pages/{Board,TaskDetail,SprintPage,Costs}.tsx, dialogs/{StartSprint,EndSprint,CancelTask,DailyLimit}.tsx (+ css, tests)   creates (T2, T3, T4)
crates/cli/src/bin/farik-e2e-serve.rs, apps/web/e2e/board.spec.ts                                 modifies / creates (T5)
docs/SPEC.md (F3, F17), docs/plans/project-plan.md                                                 modifies (T5)
```

## Interfaces

Consumes: `tasks.list`, `team.get`, `serve.status`; step 07's `waiting.list`, `team.activity`, `task.history`, `task.checks`, `task.diff`, `task.tries`, `contract.get`, `sprint.current`; commands `sprint_start`, `sprint_end`, `task_integrate`, `session_stop`, `task_transition`; step 06's `team.save`; step 08's `statusWord`.

Produces:

```rust
Projections::costs_by_purpose(&self, task_id: &TaskId) -> Result<BTreeMap<String, f64>, StoreError>;
Projections::costs_for(&self, scope: CostScope, window: CostWindow) -> Result<Vec<CostProjection>, StoreError>;
pub enum CostWindow { Day(NaiveDate), Sprint(String), All }
```

```ts
export type Lane = 'planning' | 'todo' | 'in_progress' | 'stuck' | 'review' | 'done';
export function laneOf(task: TaskRow, waiting: WaitingRow[]): Lane;
```

RPC queries: `task.costs`, `sprints.list`, `sprint.get`, `costs.summary`, `metrics`.

## Tasks

### Task 1: The queries

- `splits_a_tasks_cost_by_purpose`: two implement records and one verify record.
- `costs_each_agent_by_day_and_sprint`.
- `answers_the_sprints`: `planned_by`, the counts, and the meetings from ceremony posts inside the window.
- `answers_the_cost_summary_and_the_metrics`, with a daily limit set and without one.

- [ ] `feat(runtime): answer the board's, the sprints' and the costs' queries`

### Task 2: Lanes, the board, and the rail

- `puts_every_status_in_its_lane`: every `TaskStatus`, and both `escalated` cases.
- `filters_the_board`: agent, waiting-on-me, epic, sprint, risk, and cancelled.
- `shows_each_row_with_its_mark`: the avatar alt, the title, the id, and the mark.
- `starts_and_ends_a_sprint_from_the_board`: the dialogs send their commands; the budget is null for "No limit".
- `orders_the_rail`: the six places, and the phone's five.

- [ ] `feat(web): add the board, with lanes, filters and sprint controls`

### Task 3: Task detail

- `shows_the_five_tabs`, each with its data.
- `shows_cost_by_purpose`: the words and the limit.
- `offers_add_stop_and_cancel_when_they_apply`: each button appears only in its state, and sends its command; cancel requires a reason.

- [ ] `feat(web): add the task page`

### Task 4: Sprints and costs

- `shows_a_sprint`: the header, the tasks, the meetings, and the spending.
- `shows_the_costs`: today's line with and without a limit, the per-agent table, and the metrics' "Not yet".
- `sets_a_daily_limit`: `team.save` is called with `daily_usd`.
- `shows_one_sprints_metrics`: the toggle passes `sprint_id`.

- [ ] `feat(web): add the sprint and costs pages`

### Task 5: The journey, the spec and the plan

- `board.spec.ts`, as in the Tests decision.
- Spec F3 and F17: the board's lanes and filters, and the costs page.
- The project plan's step 09 line.

- [ ] `test(web): run a sprint on the board through the real server and browser`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T1 4 new); @farik/web: step 08's landed count plus 12 (T2 5, T3 3, T4 4);
#   playwright: step 08's 7 plus 1 = 8 passed; last line: xtask check: ok
```
