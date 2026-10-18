# Standalone UX pass, plan 5: a denser board

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` F3 (Board); `docs/design/web-ui.md`'s board; recorded by plan 8 (revision 0.123)
Depends on: nothing on this branch; `Board.tsx`, `KanbanColumn` (`@catervas/ui`), on main
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The board stops being a column of tall cards (screens 20, 25: six lanes of about 170 px each, every card five to eight lines, Done 20 cards long). A card's title is at most three lines; its id and its epic's name share one muted line; Done shows every card waiting on the owner and the five most recent others, with "Show all n". Out of scope: the lanes themselves and their order, the filters, the phone tabs, the marks' words and colours, the allowances line.

## Decisions

- Card layout (`row` in `Board.tsx`): avatar, then `rowText` holding (1) the title link, clamped to three lines with the class `styles.title3` (`display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 3; line-clamp: 3; overflow: hidden;`), its `title` attribute the full title; (2) one `<span className={styles.meta}>` with the id and, when the task has a parent, ` · ` and the parent's title; `.meta` is one line, `white-space: nowrap; overflow: hidden; text-overflow: ellipsis;` in the small type and `ink-muted`, its `title` attribute the whole line; (3) an epic's parts count (`epicParts`) as now; (4) the mark (`StatusWord`) as now. The parent's title was its own line above the title; it moves into the meta line. Rejected: dropping it (a part names its epic: `shows_each_row_with_its_mark`).
- The meta line's text is `${task.taskId} · ${parent.title}` with a middle dot (U+00B7) between spaces; without a parent, the id alone.
- Done's cap applies to the `done` lane only, wide or on a phone: its rows are split into `waiting` (the task is in `waitsOn`) and `others`. All `waiting` rows show. Of `others`, `DONE_SHOWN = 5` show: the five with the highest task number, since `tasks.list` lists tasks oldest task id first (`Projections::board`, `ORDER BY` number); the shown rows keep the lane's order (oldest first among them). With more, a `Button` under the lane's list: `boardShowAll` "Show all {count}" (`count` = all rows in Done) and once open `boardShowFewer` "Show fewer"; `aria-expanded` and `aria-controls` the lane's `<ul>` (give it `id="lane-done-rows"`). The open state is the page's `useState`, reset when the filters change (a filter rarely leaves more than five). Rejected: capping every lane (the other lanes are work in flight the owner watches).
- The lane's count in its heading (`KanbanColumn`'s `count`) stays the whole count, and on a phone the tab's count too.
- The task number for "highest" is the digits after the last `-` of `taskId` (`CTV-31` → 31), parsed with `Number`; an id with no number sorts first. One helper `taskNumber(id: string): number` in `apps/web/src/app/lanes.ts`.

## File map

```
apps/web/src/app/lanes.ts             modifies: taskNumber(id)
apps/web/src/app/lanes.test.ts        tests:   taskNumber
apps/web/src/pages/Board.tsx          modifies: the card's title clamp and meta line; Done's cap and its button
apps/web/src/pages/Board.module.css   modifies: .title3, .meta
apps/web/src/strings/en.ts            modifies: boardShowAll, boardShowFewer added
apps/web/src/pages/Board.test.tsx     tests:   shows_each_row_with_its_mark updated; caps_done_and_shows_all added
apps/web/e2e/board.spec.ts            tests:   unchanged; re-run (its Done holds one task waiting on the owner)
```

## Interfaces

Consumes: `TaskRow`, `LANES`, `laneOf` (`apps/web/src/app/lanes.ts`, on main).

Produces:

```ts
// apps/web/src/app/lanes.ts
/** The number in a task id ("CTV-31" → 31); 0 when it has none. */
export function taskNumber(id: string): number;
// apps/web/src/pages/Board.tsx (module scope, not exported)
const DONE_SHOWN = 5;
```

## Tasks

### Task 1: the card in fewer lines

Files: modified `apps/web/src/pages/Board.tsx`, `apps/web/src/pages/Board.module.css`, `apps/web/src/pages/Board.test.tsx`
Consumes: nothing from earlier plans

Tests:

- `shows_each_row_with_its_mark` (updated in place) — its assertion that "Gift card page" names its epic becomes: the row's meta line has the text "CTV-<n> · Gift cards" (use the fixture's id for "Gift card page"), with the class `styles.meta`; the review row's `getByText("CTV-9")` becomes `getByText("CTV-9")` on the meta line (it has no parent, so the meta line is the id alone and the existing assertion still holds); the title link of "New checkout page" has the class `styles.title3` and `title` "New checkout page". Everything else in the test is unchanged. Fails first because the epic's name is its own line and there is no meta class.

- [x] `feat(web): a board card shows its id and epic on one line and its title in three`

### Task 2: Done shows what waits and the five most recent

Files: modified `apps/web/src/app/lanes.ts`, `apps/web/src/app/lanes.test.ts`, `apps/web/src/pages/Board.tsx`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/Board.test.tsx`
Produces: `taskNumber`
Consumes: Task 1's card

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `lanes.test.ts`, `reads_the_number_of_a_task_id` — `taskNumber("CTV-31")` is 31, `taskNumber("CTV-9")` is 9, `taskNumber("odd")` is 0. Fails first because the function does not exist.
- `Board.test.tsx`, `caps_done_and_shows_all` (new) — wide; `tasks.list` answers eight accepted tasks `CTV-1` to `CTV-8` (titles "Done 1" to "Done 8"), and `waiting.list` one `integration` row for `CTV-2`. Asserts the Done region's heading count is 8; the Done list shows six rows, in this order: "Done 2" (waits on the owner, kept although old), "Done 4", "Done 5", "Done 6", "Done 7", "Done 8"; a button "Show all 8" with `aria-expanded="false"`; clicking it shows eight rows in board order and the button "Show fewer"; no axe violations. Then on a phone (`media.set(WIDE, false)`), choosing the "Done 8" tab shows the same six rows and the same button. Fails first because Done shows all eight.

- [x] `feat(web): the board's Done lane shows what waits on you and the five most recent`

## Verification

```
pnpm --filter @catervas/web test -- Board lanes
# expected: pass
pnpm --filter @catervas/web e2e -- board.spec.ts
# expected: all pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
