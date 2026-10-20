# Governor flexibility, plan 2: integration and the server

Status: ready
Branch: `fix/governor-flexibility`, after plan 1
Spec and decisions: see `README.md`; ADR 0079 decisions 7, 8, 13, 14, and the server side of the board, Files and Costs findings

## Decisions

- Decision 7: in `integrate.rs` `integrate_locked`, a plan task whose plan file is already on the integration branch is recorded integrated as now; otherwise it goes `through_the_forge` like any task. `carried_plan`'s refusal `carried_by_its_tasks` is removed. `start_point`'s path (b) is unchanged.
- Decision 8: `start_point` appends, after the dependencies and the plan, the branch of every task that is `Accepted`, `awaiting_integration`, in the same repository, not in a private folder, and whose `allowed_paths` all start with `docs/catervas/`, in board order. `start_worktree` merges these as now; a conflict on one of them aborts that merge only (`merge_or_abort` from plan 1) and the start goes on. A conflict on a dependency or the plan still undoes the start, as now.
- Decision 13: `escalate` posts, for a task, `post_system(.., Some(task_id), "<id> could not be added to <into>: <detail>")`.
- Decision 14: route `POST /connect/code`, token-guarded like `/command`, answers `{ "code": "<hex>" }` from `state.web().codes.issue()`, or 404 without a web state. CLI `catervas open` reads `.catervas/local/daemon.json`, calls it with `exchange`, and prints `open http://127.0.0.1:<port>/connect#<code> in your browser`; with no running driver it prints "Catervas is not running here. Start it with catervas serve." and exits 1.
- Board: `tasks.list` rows gain optional `waits_on: [{ "task_id", "status" }]`, the contract's dependencies not yet `accepted`, from `task_wire`.
- Files: `last_change` sets `agent_id` to the pending task's assignee when the subject names none and the document comes from a pending task. The injected `FIT` style gains `table { overflow-wrap: break-word }`.
- Costs: `costs.summary` gains optional `cancelled_today_usd`, today's spend on tasks now `cancelled`, omitted when zero.

## File map

```
runtime/src/orchestrator/integrate.rs     modifies: plan PR (T9), channel line (T11); tests
runtime/src/orchestrator/requests.rs      modifies: start_point documents (T10); tests
runtime/src/orchestrator/rules.rs         modifies: start_worktree keeps going past a documents conflict (T10)
runtime/src/daemon.rs, runtime/src/daemon/web.rs   modifies: /connect/code (T12); tests
cli/src/open.rs                           creates: catervas open (T12)
cli/src/main.rs (or the command list)     modifies: Open subcommand (T12)
docs/schemas/rpc.schema.json              modifies: waits_on, cancelled_today_usd (T13, T14)
runtime/src/daemon/web.rs                 modifies: task_wire waits_on (T13)
apps/web/src/pages/Board.tsx, Board.test.tsx, strings/en.ts   modifies: "Waits on <id>" meta (T13)
runtime/src/daemon/documents.rs           modifies: last_change agent, FIT (T14a); tests
runtime/src/daemon/board.rs               modifies: cancelled_today_usd (T14b); tests
apps/web/src/pages/Costs.tsx, Costs.test.tsx, strings/en.ts   modifies (T14b)
```

## Tasks

### Task 9: a plan task opens its own pull request

Tests (integrate.rs, ignored like its neighbours):
- `opens_a_pull_request_for_an_accepted_plan` — replaces `integrates_a_plan_task_by_its_plan_not_a_pull_request`: an accepted plan task whose plan is not on main, with an origin and the fake `gh`, gets `pull_request.opened` for its branch after one tick; one whose plan is on main is recorded integrated with no `gh` call.

- [ ] `fix(runtime): a plan task opens its own pull request`

### Task 10: a new task starts with the team's accepted documents

Tests (requests.rs):
- `a_task_starts_with_the_teams_accepted_documents` — CTV-5 accepted, awaiting integration, allowed paths `docs/catervas/architecture/overview.html`; a new task CTV-8 with no dependencies starts; its worktree contains that file and its log has `chore: start from <CTV-5's branch>`. A documents branch that conflicts with the task's start is left out and the worktree still starts.

- [ ] `feat(runtime): a new task starts with the team's accepted documents`

### Task 11: a failed integration of a task is said in the channel

Tests (integrate.rs):
- `says_in_the_channel_that_a_task_could_not_be_integrated` — a task whose pull request was closed on the forge: after one tick the channel has a system line tagged with the task that contains "could not be added to main".

- [ ] `fix(runtime): a task that could not be integrated is said in the channel`

### Task 12: `catervas open` prints a new start link

Tests:
- `issues_a_new_start_link_to_the_token_holder` (daemon.rs) — `POST /connect/code` with the token answers 200 and a 64-hex code that `/connect` then redeems once; without the token, 401.
- `says_how_to_start_when_nothing_runs` (cli tests) — `catervas open` in a project with no `daemon.json` exits 1 with "Catervas is not running here".

- [ ] `feat(cli): catervas open prints a new start link from the running driver`

### Task 13: the board says what a waiting task waits on

Tests:
- `lists_what_a_task_waits_on` (web.rs) — `tasks.list` for CTV-3 depending on CTV-1 `cancelled` and CTV-2 `accepted` has `waits_on: [{task_id: "CTV-1", status: "cancelled"}]`.
- `shows_what_a_to_do_card_waits_on` (Board.test.tsx) — the CTV-3 card's meta reads "Waits on CTV-1 (cancelled)".

- [ ] `feat(web): a To do card says what it waits on`

### Task 14: Files names the writer and keeps table words whole; Costs shows cancelled spend

Tests:
- `names_the_agent_of_a_pending_documents_task` (documents.rs) — a pending task's document whose commit subject names no task has `agent_id` = that task's assignee.
- `keeps_table_words_whole` (documents.rs) — the raw document's injected style contains `table { overflow-wrap: break-word }`.
- `sums_today_spend_on_cancelled_tasks` (board.rs) — cost records today of $1.50 on a cancelled task and $2 on an accepted one give `cancelled_today_usd: 1.5`.
- `says_what_cancelled_work_cost` (Costs.test.tsx) — with `cancelledTodayUsd: 1.5` the page shows "$1.50 of it went to cancelled tasks".

- [ ] `fix(runtime): Files names who wrote a pending document and keeps table words whole` (first two tests)
- [ ] `feat(web): Costs says what cancelled work cost` (last two tests)
