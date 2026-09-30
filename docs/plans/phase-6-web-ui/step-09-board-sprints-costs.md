# Phase 6, step 09: Board, sprints, task detail, and costs

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.2, 5.14, 5.16, F3, F17
Depends on: steps 01 to 08 of this phase
Readiness confirmed by: fresh-session reviewer, 2026-09-29, round one: not ready on four planner decisions, all settled below; round two found them settled (ready with findings, folded in).

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
  - `laneOf(task: TaskRow): Lane` decides the escalated lane from `awaiting_approval` alone (the task row carries it).
  - A task's mark comes from `waiting.list` (step 07) and `team.activity`, whose `purpose` gives "Writing" (refine), "Building" (implement) or "Reviewing" (verify) while an agent works on the task.
  - The `escalated` placement (Planning or Stuck) is an addition to `web-ui.md`'s table, recorded there and in spec F3 by Task 5.
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
  - **"Stop work on this task".** It sends `session_stop` for the running session that `team.activity` names (its new `session_id`), and appears only while one runs.
  - **"Cancel this task".** It shows unless the task is `accepted` or `cancelled`. A dialog asks for the reason, then sends `task_transition { to: cancelled, reason }`, or, for an `escalated` task, `escalation_resolve { to: cancelled, message: reason }`, as the command schema requires.

  Differences from the mockup: the cost words are the plan's four purpose words, not "Writing the plan / Building it / Reviewing it"; "What this task is for" is the contract's intent at the top of Summary; the "<reviewer> sent it back…" line is the latest review summary. Also: "See the earlier version of the plan" is dropped (the log keeps no earlier contract), and "Add a note for the team" is dropped (step 10's channel is where the user speaks to the team).
- **Costs by purpose.**
  - `Projections::costs_by_purpose(task_id: &TaskId) -> Result<BTreeMap<String, f64>, StoreError>` reads `cost_records.purpose`.
  - Purposes map to plain words: `triage`, `refine` and `plan` are "Planning"; `implement` is "Building"; `verify` is "Checking"; `ceremony` and `conversation` are "Meetings and talk".
  - The query `task.costs { task_id }` answers `{ by_purpose: [{ words, usd }], total_usd, limit_usd }`.
- **Sprints.**
  - `sprints.list {}` answers `[{ sprint_id, status, started_at, started_by, ended_at, budget_usd, spent_usd, planned_by, task_count, done_count }]` from `ProjectFiles::list_sprints` and the events. `started_by` comes from `sprint.started`. `planned_by` is the first `sprint.planned` by an assigner, or null when only the governor planned (a breakdown joining its epic's sprint).
  - `sprint.get { sprint_id }` answers the same fields plus `tasks: [{ task_id, title, status }]` and `meetings: [{ thread, first_seq, at, posts }]`. The meetings come from the ceremony `message.posted` events from the sprint's start until the next sprint starts, so the review and the look back, which run after the end, are included.
  - **Start.** The dialog reads "Start sprint N". It explains that the planner plans it and names how many ready tasks there are (ready, with no parent and no sprint, from `tasks.list`). The budget is a choice: "No limit", or "Stop handing out new work after $[20.00]". "Start sprint N" sends `sprint_start { budget_usd }`. The planner is the Scrum Master, or the Product Manager when there is none, as spec 5.9 says.
  - **End early.** A confirmation says how many tasks are unfinished and that they go back on the board. "End sprint N" sends `sprint_end`.
  - **The sprint page** (`/sprints/:id`) has the mockup's header, the task list, the meetings with "Read it" links (to step 10's channel thread; until step 10 lands, the link is omitted), and the spending line.
- **Costs** (`/costs`).
  - `Projections::costs_for(scope: CostScope, window: CostWindow) -> Result<Vec<CostProjection>, StoreError>`, with `CostWindow { Day(NaiveDate), Sprint(String), All }`, gives agent × day and agent × sprint.
  - `costs.summary {}` answers `{ today_usd, daily_limit_usd, sprint: { sprint_id, spent_usd, budget_usd } | null, agents: [{ agent_id, today_usd, sprint_usd }] }`.
  - `metrics { sprint_id? }` answers this wire shape, built by the daemon from `HarnessMetrics` (no `Serialize` added): `{ accepted_tasks, first_pass_acceptance_rate: number | null, interventions_per_accepted_task: number | null, cost_per_accepted_task: { total_usd, by_purpose: [{ words, usd }] } | null, mechanically_verified_criteria_share: number | null, active_weeks, messages: { reaction, ambient, reply, ceremony, system, human } }`. The page shows the five rate lines, then "Active weeks: N" and one messages line ("Messages: N reactions, N replies, N from meetings, N from you").
  - Differences from the mockup: SprintView's "See costs by agent" is the link to `/costs`; the board's empty lane reads "Nothing here right now" everywhere, not "Nobody is stuck."
  - The page reads "Today the team has spent $X" and either "with no daily limit set" or "of $L a day". Beside it:
    - the sprint line;
    - "Set a daily limit", which saves `budgets.daily_usd` through `team.save` (step 06) in a small dialog;
    - the per-agent table: Today, This sprint, and Right now (from `team.activity`).
  - "How well the team works" gives the five metrics in words, with the cost split into the purpose words and "Show sprint N only" toggling `sprint_id`. Each rate with no accepted task reads "Not yet: no work has been accepted".
- **The rail** gains Board, Team (step 06's page) and Costs. After this step the desktop rail is Today, Board, Team, Costs, Settings (five), and the phone bar Today, Board, Team, Costs (four), with Settings under Team, as `web-ui.md` says. Step 10 adds Channel between Board and Team.
- **Activity.** Step 07's `AgentActivity` gains `session_id: Option<String>` and `purpose: Option<SessionPurpose>`, and its `state` is the enum `working | resting | waiting_on_you | paused | idle` (step 07's list), so the page never parses `line`.
- **Tests.**
  - Vitest and axe on each page. `lanes.test.ts` checks every `TaskStatus`.
  - Playwright `board.spec.ts`, on step 08's team. The recorded adapter replays in order, so every session is listed: `triage_frk_1_small_by_pm`, `refine_writes_task_for_theo_frk_1`, `judge_frk_1_by_architect`, `planning_ceremony_frk_1` (phase 4's, which plans FRK-1 and plays as Mira, the planner without a Scrum Master), `plan_assigns_frk_1_to_theo`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`, then the sprint's review and look-back ceremonies: phase 4's `review` and `retro` transcripts (`transcripts/review.jsonl`, `transcripts/retro.jsonl`), the order phase 4's orchestrator tests already use. It assumes no UTC midnight during the run, so no standup session starts. The steps:
    1. start a sprint with a $20 budget;
    2. file the request on Today (after the sprint opens, so the planning ceremony plans it);
    3. see the task move from Planning to To do to In progress to Review to Done on the board by itself;
    4. once the task is Done, open its five tabs;
    5. see the sprint end by itself once its one task is accepted (spec 5.2), and its meetings listed;
    6. see the Costs page's per-agent row;
    7. take screenshots at 360 and 1280 px.

  "End the sprint early" is covered by its unit test, since the governor ends this sprint itself.
- **Team settings after setup (Task 4b).** The controller ruled (step 06 ledger, 2026-09-29) that Settings gains setup's four questions, reusing the setup components and saving through `team.save`, because a non-technical user must be able to change them without a text file; the carried items from steps 06 and 08 (N1, N2, "Developer", `too_short`, M10, the Gate's history link and the shared plan-approved rule) go with it.

## File map

```
crates/store/src/{projections.rs,activity.rs} (+ tests)                      modifies: costs_by_purpose, costs_for; session_id and purpose (T1)
apps/web/src/pages/Gate.tsx                                                  modifies: "See the whole history" to /tasks/:id (T3)
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
export function laneOf(task: TaskRow): Lane;
```

RPC queries: `task.costs`, `sprints.list`, `sprint.get`, `costs.summary`, `metrics`.

## Tasks

### Task 1: The queries

- `splits_a_tasks_cost_by_purpose`: two implement records and one verify record.
- `costs_each_agent_by_day_and_sprint`.
- `answers_the_sprints`: `started_by`, `planned_by`, the counts, and the meetings including the review after the end.
- `says_which_session_an_agent_is_in`: `team.activity` answers `session_id` and `purpose` while a session runs, and neither when idle.
- `answers_the_cost_summary_and_the_metrics`, with a daily limit set and without one.

- [x] `feat(runtime): answer the board's, the sprints' and the costs' queries`

### Task 2: Lanes, the board, and the rail

- `puts_every_status_in_its_lane`: every `TaskStatus`, and both `escalated` cases.
- `filters_the_board`: agent, waiting-on-me, epic, sprint, risk, and cancelled.
- `shows_each_row_with_its_mark`: the avatar alt, the title, the id, and the mark.
- `starts_and_ends_a_sprint_from_the_board`: the dialogs send their commands; the budget is null for "No limit".
- `orders_the_rail`: the five places, and the phone's four.

- [x] `feat(web): add the board, with lanes, filters and sprint controls`

### Task 3: Task detail

- `shows_the_five_tabs`, each with its data.
- `shows_cost_by_purpose`: the words and the limit.
- `offers_add_stop_and_cancel_when_they_apply`: each button appears only in its state, and sends its command; cancel requires a reason; an `escalated` task cancels through `escalation_resolve`.

- [x] `feat(web): add the task page`

### Task 4: Sprints and costs

- `shows_a_sprint`: the header, the tasks, the meetings, and the spending.
- `shows_the_costs`: today's line with and without a limit, the per-agent table, and the metrics' "Not yet".
- `sets_a_daily_limit`: `team.save` is called with `daily_usd`.
- `shows_one_sprints_metrics`: the toggle passes `sprint_id`.

- [x] `feat(web): add the sprint and costs pages`

### Task 4b: Team settings after setup

- `changes_what_agents_may_do_and_the_limit_from_settings`: the team's own answers, the effect before Save, a failed save in plain words, "Put back the default", `team.save` with the permissions, then with `daily_usd` from the Costs page's control.
- `changes_how_work_is_added_and_how_plans_are_checked_from_settings`: integration and its put back, the questions and the judge with their effects, a plan check with no questions refused at the questions, and the saved judgment.
- `setup-team.spec.ts` gains Settings: "How finished work is added" changed, its effect shown, saved to `team.yaml`, and Settings screenshotted at 360 and 1280 px.
- Carried: `says_developer_as_the_mockups_do`; `said("too_short")`; a deep link asks nothing before `serve.status` (M10); the Gate's history links to `/tasks/:id`; `team.get` answers `max_agents` and Add someone reads it (N2); the five `saidAll` catch sites each say plain words (N1).

- [x] `feat(web): change the team's rules from Settings after setup`

### Task 5: The journey, the spec and the plan

- `board.spec.ts`, as in the Tests decision.
- Spec F3 and F17: the board's lanes and filters, and the costs page.
- The project plan's step 09 line.

- [x] `test(web): run a sprint on the board through the real server and browser`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T1 5 new); @farik/web: step 08's landed count plus 12 (T2 5, T3 3, T4 4)
#   plus Task 4b's 5 (two Settings tests, three N1 tests); @farik/ui plus 1 (Developer);
#   playwright: step 08's 7 plus 1 = 8 passed; last line: xtask check: ok
```

Built (2026-09-30): @farik/web 101 passed (Task 4b's run; Task 5 adds no test file, it extends three:
the board's sprint line is a link, the task page's notes come from the log, and one post reads "1 post");
@farik/ui 41; playwright 8 passed, `board.spec.ts` the eighth, which also screenshots the board, the task
page, a sprint page, the Costs page and Settings' team rules at 360 and 1280 px, each with no sideways
scroll at 360; its `farik-e2e-serve --pace 600` holds each recorded session so the board redraws in every
lane the task passes.
