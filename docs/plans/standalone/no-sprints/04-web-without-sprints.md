# Standalone CTV-9, plan 4: sprints leave the web app

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1, 4.2, 4.4, F3, F7, F17, the web UI; recorded by plan 8 (revision 0.116)
Depends on: plans 1 to 3 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The web app shows no sprint, no Backlog lane, no "Start sprint", no sprint budget and no "Plan work
in sprints", and asks the daemon nothing about sprints, so plan 5 can take them off the wire.
Ready work shows under "To do". The Costs page counts by the day; allowances say "each day". The
sprint journey goes and the board journey shows a request flow to "Done" and the team's review and
retro run by themselves. Epic C3's web half holds: no `SprintPage.tsx`, no `sprints.spec.ts`, no
"sprint" in `en.ts`. Out of scope: the daemon, the CLI and the schemas (plan 5).

## Decisions

- Shared pieces move before anything is deleted, unchanged: `useCommand` from
  `pages/dialogs/StartSprint.tsx` to a new `apps/web/src/app/command.ts` (its 22 importers updated);
  `MEETINGS` and `threadAnchor` from `pages/SprintPage.tsx` into `pages/Channel.tsx`, their one user.
  `Board.tsx`'s `Backlog`, `moreWaits` and `plannerOf` are deleted with their callers.
- Deleted whole: `pages/SprintPage.tsx` and its test, `pages/dialogs/StartSprint.tsx`,
  `pages/dialogs/EndSprint.tsx`, `apps/web/e2e/sprints.spec.ts`, the route `/sprints/:id`
  (`App.tsx`), `pages.module.css`'s `.waiting` and `setup.module.css`'s `.sprints`.
- Board: no `sprint.current`, `sprints.list` or `backlog.summary` query; no sprint line, no
  Start/End button, no `?start=sprint`, no sprint filter (the filters left: agent, risk, epic); the
  lane `backlog` leaves `lanes.ts` (`Lane`, `LANES`, `laneOf`'s first rule, `TaskRow.backlog`,
  `TaskRow.sprint`); a ready row is in "To do" as `PLACES` already says.
- Today: no sprint line and no Backlog prompt in the band; no `sprint.current` or `backlog.summary`
  query. Channel: no "Meetings in this sprint" panel and no `sprint.current` or `sprint.get` query;
  ceremony threads still group as today. Task page: the lead is `${id}.`; `sprint.planned` is no
  longer worded in the history (`toldPlanned` deleted), so an old log's planning move is not listed.
- Costs: no sprint head lines, no sprint column, no "Show sprint N only"; `metrics` is asked without
  `sprintId`; the page reads `costs.summary`'s `agents[].todayUsd` and ignores `sprint` and
  `sprintUsd` until plan 5 removes them. Allowances: `period` is `{ kind: "day"; day }` only; the
  words: `allowQuestion` "How many may {name} make each day without asking?", `allowEach` "each
  day", `allowDay` "The count starts again each day, at midnight UTC.", `allowMakeUpTo` "make up to
  {list} each day"; `allowRowSprint`, `allowBoardSprint`, `allowCostsSprint`, `allowApprovalSprint`
  deleted with their branches.
- Settings and setup: the "Planning work" part (`TeamRules.tsx`'s `Planning`, `Work.tsx`'s use of
  it), `SetupAdvanced.tsx`'s `SprintsSwitch` and its card, `SetupFinish.tsx`'s sprints box, and
  `planInSprints` in `TeamSetup.tsx`, `SavedTeams.tsx` and `draftOf` go. After ADR 0061's plans 2
  and 7 (the integration choice in `SetupFinish.tsx` and `TeamRules.tsx`'s `Finish`, the plan check
  in `SetupAdvanced.tsx` and `TeamRules.tsx`'s `Plans`, already gone when this runs), "How work
  runs" holds `Documents` and, under Advanced, `UiPaths`: `sectionWorkSummary` = "How documents are
  written"; `rulesPlanning`, `planInSprints`, `planInSprintsNote`,
  `finishSprints*` deleted. The pages stop sending `plan_in_sprints`.
- Copy that named sprints: `cardLead` "A first piece of work where your team reads your project and
  writes down what it learns, before building anything."; `planGate` "Nothing else is planned until
  you approve. Then your team documents the plan first."; `readyOfferTitle` "Start with a
  Catervafication"; `refuseCatervaficationNewProject` "A new project's documenting work starts by
  itself once you approve its product plan."; `raiseNextWrites` "{agent} writes the new version at
  once, from today to {day}."; `sprintNotNow` is renamed `notNow` ("Not now") for `DailyLimit.tsx`.
  `refusals.ts` loses `sprint_open` and `no_sprint_open` and their keys. Every other key whose name
  or value holds "sprint" or the Backlog-as-a-hold is deleted with its last user. `packages/ui`:
  the gallery's "Mira plans a sprint." becomes "Mira plans the work."; `ChatList.test.tsx`'s thread
  "Sprint 2" becomes "Launch".
- The e2e fixture loses `sprints?` and its rewrite of `plan_in_sprints` (`serve.ts`); the board
  journey is rewritten, its sprint steps gone and its lane walk kept (Task 5).

## File map

```
apps/web/src/app/command.ts                         creates: useCommand, moved (Task 1)
apps/web/src/pages/** (22 importers), Channel.tsx   modifies: imports (Task 1)
apps/web/src/pages/Board.tsx, Today.tsx, Channel.tsx, TaskDetail.tsx, app/lanes.ts, app/App.tsx   modifies (Task 2)
apps/web/src/pages/SprintPage.tsx(.test), dialogs/StartSprint.tsx, dialogs/EndSprint.tsx        deletes (Task 2)
apps/web/src/pages/Board.test.tsx, Today.test.tsx, Channel.test.tsx, TaskDetail.test.tsx, app/lanes.test.ts, Chats.test.tsx, test/{productPlan,orders,folderDocs,posts,sellerMail}.ts   modifies: sprint fixtures and tests (Task 2)
apps/web/src/pages/Costs.tsx, allowances.tsx, AgentEdit.tsx, KitConnect.tsx, dialogs/ToolApproval.tsx, dialogs/ConnectorAllowance.tsx   modifies (Task 3)
apps/web/src/pages/Costs.test.tsx, allowances.test.tsx   modifies (Task 3)
apps/web/src/pages/TeamRules.tsx, settings/Work.tsx, settings/SettingsLayout.tsx, setup/{SetupAdvanced,TeamSetup,SetupFinish}.tsx, SavedTeams.tsx   modifies (Task 4)
apps/web/src/pages/team.test.tsx, setup/team.test.tsx, settings/settings.test.tsx   modifies (Task 4)
apps/web/src/strings/en.ts, app/refusals.ts, app/words.test.ts, dialogs/DailyLimit.tsx, CatervaficationCard.tsx, ProductPlanCard.tsx, dialogs/RaiseBudget.tsx, setup/SetupReady.tsx, packages/ui/gallery/Gallery.tsx, packages/ui/src/ChatList.test.tsx   modifies (Task 5)
apps/web/e2e/sprints.spec.ts                        deletes (Task 5)
apps/web/e2e/board.spec.ts, e2e/fixtures/serve.ts   modifies (Task 5)
packages/protocol-client/src/client.test.ts         modifies: maps_the_backlog_and_the_sprint_policy deleted (Task 2)
```

## Interfaces

Consumes: plan 3's `delivery.closed` (the journey reads it); `answerQuery`, `renderApp`,
`answerStatus`, `expectNoAxeViolations` (`apps/web/src/test/`). Produces:

```ts
// apps/web/src/app/command.ts — moved as it is from StartSprint.tsx
export function useCommand(): /* the same return type it has today */;
// apps/web/src/app/lanes.ts
export type Lane = "planning" | "todo" | "in_progress" | "stuck" | "review" | "done";
```

## Tasks

### Task 1: move what other pages share

Files as the map's Task 1. A move only: no behaviour, no new test; the suite is the check (stage 3
REFACTOR). `cargo xtask check` passes before and after.

- [ ] `refactor(web): move useCommand and the meeting words out of the sprint pages`

### Task 2: the board, Today, the channel and the task page

Files as the map's Task 2. `answerQuery` waits for a query the app sends, so every test answer of
`sprint.current`, `sprints.list`, `sprint.get` and `backlog.summary` is removed with the call. The
tests that pin sprint behaviour are deleted: in `Board.test.tsx` `starts_and_ends_a_sprint_from_the_board`
and the whole `describe("the board's backlog")`; `filters_the_board` loses its sprint part;
`Today.test.tsx`'s `counts_the_backlog_on_today`; `lanes.test.ts`'s `places_backlog_rows_in_their_lane`;
`Channel.test.tsx`'s side-panel meeting assertions; `SprintPage.test.tsx` with its page.

- `shows_ready_work_to_do_with_no_sprint_anywhere` (`Board.test.tsx`) — a board of a `ready` task
  CTV-1, an `in_progress` CTV-2 and an `escalated` CTV-3: CTV-1 is in the column "To do"; there is
  no column "Backlog", no button "Start a sprint" or "End the sprint early", no filter "Sprint";
  `document.body.textContent` matches no `/sprint/i`; the socket received no query named
  `sprint.current`, `sprints.list`, `sprint.get` or `backlog.summary`.

- [ ] `feat(web): the board and Today work with no sprint and no Backlog`

### Task 3: costs and allowances by the day

Files as the map's Task 3. `allowances.test.tsx`'s `sprintPeriod` fixture becomes a day period;
`board_shows_each_allowance_beside_the_sprint` becomes `board_shows_each_allowance_for_the_day`;
`costs_says_today_with_no_sprint_open` is deleted (covered below); `Costs.test.tsx`'s
`shows_one_sprints_metrics` is deleted and `shows_the_costs` loses its sprint lines.

- `costs_and_allowances_count_by_the_day` (`allowances.test.tsx`) — with `allowances.list`'s period
  `{ kind: "day", day: "2026-10-16" }`: the connect dialog asks "How many may Theo make each day
  without asking?"; the Costs page's allowance column is headed as the day's (`allowCostsDay`); the
  Costs page has no text matching `/sprint/i` and sends `metrics` with params `{}`.

- [ ] `feat(web): costs and allowances count by the day`

### Task 4: setup and Settings never ask about sprints

Files as the map's Task 4. Deleted tests: `team.test.tsx`'s `reads_an_old_team_as_not_planning_in_sprints`
and `switches_planning_in_sprints`; `setup/team.test.tsx`'s `starts_a_team_that_plans_in_sprints`;
`keeps_each_rules_draft_until_it_is_saved_or_cancelled` uses the `Documents` part's choice in
place of the sprints switch (as ADR 0061 left that test); `splits_the_rules_across_three_sections`
expects no "Planning work" heading.

- `setup_and_settings_never_ask_about_sprints` (`setup/team.test.tsx`) — through setup with
  Advanced open, then Settings' "How work runs": no switch or text matching `/sprint/i`, and the
  `team.save` sent at the end of setup has no `plan_in_sprints` under `policy`.

- [ ] `feat(web): setup and Settings no longer ask about sprints`

### Task 5: the words, and the journeys

Files as the map's Task 5. `board.spec.ts`'s one test becomes `work runs on the board by itself,
from its plan to the team's look back`: transcripts `triage_ctv_1_small_by_pm`,
`refine_writes_task_for_theo_ctv_1`, `plan_assigns_ctv_1_to_theo`,
`implement_finishes_ctv_1`, `review_writes_note`, `accept_ctv_1`, `review`, `retro`; it keeps the
lane walk (`LANES`, the cell checks) and drops every sprint step, the `/sprints/S1` page and the
"Sprint 1" cost column.

- `work runs on the board by itself, from its plan to the team's look back` (Playwright) — the
  request sent from Today reaches "Done" with no click on the Board (accepted, its branch waiting
  for a merge by hand, as ADR 0061 has it with no `origin`); the log then holds one
  `delivery.closed` with `trigger: "team_quiet"` and `task_ids: ["CTV-1"]`; the Team chat shows a
  "Review" and a "Looking back" thread; no page visited shows text matching `/sprint/i`.
- C3's web half, as a command (Verification).

- [ ] `feat(web): no sprint words left, and the board journey without sprints`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
test ! -e apps/web/src/pages/SprintPage.tsx && test ! -e apps/web/e2e/sprints.spec.ts && ! grep -qi 'sprint' apps/web/src/strings/en.ts && echo WEB_HALF_OK
# expected: WEB_HALF_OK
```
