# Standalone UX pass, plan 2: "Waiting on you" grouped by what the owner does

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.2 (Today), 6.7's "Suggested connections on Today"; recorded by plan 8 (revision 0.123)
Depends on: plan 1 of this folder (committed on this branch; it touches `Today.tsx`'s `AgentEntry`, which this plan leaves alone)
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

"Waiting on you" stops being one list of up to 13 rows in no order (screens 16, 22, 24). Its heading keeps the total; under it the rows fall into three groups, by what the owner does: set something up, answer, merge. A question's or a stuck task's row names the task it is about and shows at most three lines of the agent's words; the whole text is on the page its Answer opens. The merge group shows three rows, then "Show all n". After it, "What moved" comes before "Suggested connections", which shows two rows instead of four. Out of scope: each row's own content and buttons (orders, posts, pipelines, ads, tool approvals, document changes keep their components unchanged), the request box's place (it stays above "Waiting on you", `docs/design/web-ui.md`), the help and question pages (plan 3, and `fix/open-issues`).

## Decisions

- Groups, in this order, each a `<section aria-labelledby>` with an `<h3>` and its own `<ul>`; a group with no rows is not drawn:
  1. `set_up`, named `waitingGroupSetUp` "Needs setting up": the key-refused row, `preview_missing`, `designer_needs_sandbox`, `designer_needs_browser`. First, because each stops work.
  2. `answer`, named `waitingGroupAnswer` "Needs your answer": `question`, `help`, `approval`, `acceptance`, `tool_approval`, `marketing_plan`, `social_post`, `purchase_order`, `data_pipeline`, the three ads kinds, and then the document-change cards (`folder_doc_proposals.list`), in that order of the daemon's list then cards.
  3. `merge`, named `waitingGroupMerge` "Ready for you to merge": `integration` rows (a folder change's `integration` row included) (finished work whose pull request waits; ADR 0061 left no human acceptance, so `acceptance`, rare in an older log, stays with answers).
  Within a group rows keep the daemon's order. Rejected: sorting by age (`waiting.list` rows carry no time).
- The grouping is one pure function in a new file, `apps/web/src/pages/waitingGroups.ts`, so the rule is tested without rendering. `Today.tsx` keeps its row components and moves the existing `waiting.waiting.map(...)` body into a local `row(item: Waiting): ReactNode` it calls per group.
- The h2 stays `waitingTitle` "Waiting on you ({count})" with the same total (rows + cards + key row). Each group's `<h3>` reads `waitingGroupCount` "{group} ({count})" (so "Needs your answer (4)"), and its `<ul>` carries `aria-label={t(<group key>)}`, the group's name without the count, so a test or a screen reader finds a group by a name that does not change with its size. `waitingList` ("Waiting on you") labels no list any more and is deleted from `en.ts`.
- The empty state is unchanged: "Nothing waits on you." under the h2.
- A `question` or `help` row (`WaitingRow`) shows, under its bold title, a muted line `waitingAbout` "About “{title}”" with the task's title, then the agent's `item.line` in a span with the new class `.clamp3` (three lines, the same declarations as `.clamp` with 3). Other kinds' lines are short and are not clamped.
- The merge group shows its first `MERGE_SHOWN = 3` rows; with more, a `Button` under them reads `suggestionsShowAll` "Show all {count}" (the existing key, the same words as the connections' button), and once open, `suggestionsShowFewer` "Show fewer". `aria-expanded` on the button, `aria-controls` the group's `<ul>`. The set-up and answer groups are never folded: each row there is something the owner must act on.
- Order of Today's sections after this plan: title, band, request box, Catervafication card, "Waiting on you", seller mail, renewals, posts going out, "What moved", "Suggested connections", channel preview. Only "Suggested connections" moves (from after "Waiting on you" to after "What moved"): it is optional advice, and four full cards pushed the day's news off the screen (screen 16).
- `SUGGESTIONS_SHOWN` in `SuggestedConnections.tsx` goes from 4 to 2.
- Headings: the groups are `h3` under the `h2`; styled by a new `.groupTitle` in `Today.module.css` (the `heading` type token at `small` size is not a token; use `--catervas-type-body-*` with `font-weight: 600`, margin `var(--catervas-space-4) 0 var(--catervas-space-2)`). No colour: ink.
- 360 px: groups are block sections, rows keep `.row`'s existing phone rules; the fold button is full width like `.movedMore > button` (reuse that class on a wrapping `div`).

## File map

```
apps/web/src/pages/waitingGroups.ts         creates: WaitingGroup, GROUP_ORDER, groupOf(kind)
apps/web/src/pages/Today.tsx                modifies: groups, the about line, the clamp, the fold, section order
apps/web/src/pages/Today.module.css         modifies: .clamp3, .groupTitle
apps/web/src/pages/SuggestedConnections.tsx modifies: SUGGESTIONS_SHOWN = 2
apps/web/src/strings/en.ts                  modifies: waitingGroupSetUp, waitingGroupAnswer, waitingGroupMerge, waitingGroupCount, waitingAbout added; waitingList deleted
apps/web/src/pages/Today.test.tsx           tests:   lists_what_waits_on_you rewritten; folds_the_merge_group_after_three, puts_what_moved_before_the_suggested_connections added; list lookups renamed
apps/web/src/pages/SellerMail.test.tsx      tests:   list lookups renamed (lines 406, 454, 463, 477)
apps/web/src/pages/dialogs/DataPipeline.test.tsx, RaiseBudget.test.tsx, PurchaseOrder.test.tsx   tests: list lookup renamed (line 13, 14, 17)
apps/web/e2e/accept.spec.ts                 tests:   unchanged words; re-run (its "Add" link sits in the merge group)
```

## Interfaces

Consumes: the `Waiting` type and its row components in `Today.tsx` (on main).

Produces:

```ts
// apps/web/src/pages/waitingGroups.ts
export type WaitingGroup = "set_up" | "answer" | "merge";
export const GROUP_ORDER: readonly WaitingGroup[]; // ["set_up", "answer", "merge"]
/** Which group a `waiting.list` kind falls in; any kind Today draws and this does not name is "answer". */
export function groupOf(kind: string): WaitingGroup;
export const MERGE_SHOWN = 3;
```

## Tasks

### Task 1: three groups, the about line and the fold

Files: created `apps/web/src/pages/waitingGroups.ts`; modified `apps/web/src/pages/Today.tsx`, `apps/web/src/pages/Today.module.css`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/Today.test.tsx`, `apps/web/src/pages/SellerMail.test.tsx`, `apps/web/src/pages/dialogs/DataPipeline.test.tsx`, `apps/web/src/pages/dialogs/RaiseBudget.test.tsx`, `apps/web/src/pages/dialogs/PurchaseOrder.test.tsx`
Produces: `groupOf`, `GROUP_ORDER`, `MERGE_SHOWN`
Consumes: nothing from earlier plans

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `lists_what_waits_on_you` (rewritten in place; it is the test of this functionality) — the same five rows as today (`CTV-1` approval, `CTV-2` acceptance, `CTV-3` question with title "the launch post", `CTV-4` help with title "the menu page", `CTV-5` integration), plus one `preview_missing` row (`task_id` "CTV-6", agent "mira"). Asserts: the h2 "Waiting on you (6)"; three h3 headings in document order "Needs setting up (1)", "Needs your answer (4)", "Ready for you to merge (1)"; `getByRole("list", { name: en.waitingGroupAnswer })` holds, in order, the approval, acceptance, question and help rows with the same titles, words ("Review", "Answer") and routes the current test asserts; the set-up list holds "Tell Catervas how to open your app" with "Open Settings" to `/settings/your-app`; the merge list holds "Add the photos to your project" with "Add" to `/tasks/CTV-5/accept`; the question row shows "About “the launch post”" and its `question line` span has the class `styles.clamp3`; the help row shows "About “the menu page”"; the checks-passed part of the old test (acceptance row, `task.checks`, the event, the failing check) is kept as it is, now within the answer list; `picturesOpenTheirAgents` runs on each of the three lists; no axe violations. Fails first because there is one list named "Waiting on you".
- `folds_the_merge_group_after_three` (new) — five `integration` rows (CTV-11 to CTV-15, titles "one" to "five"). Asserts the heading "Ready for you to merge (5)" and that the merge list shows three list items and a button "Show all 5" with `aria-expanded="false"`; clicking it shows five items and the button reads "Show fewer" with `aria-expanded="true"`; clicking again shows three. Fails first because all five show.
- Every other lookup of `{ name: en.waitingList }` (`Today.test.tsx` lines 313, 436, 621, 652, 706, 756, 785, 999, 1022, 1155, 1235, 1361, 1519, 2232; `SellerMail.test.tsx` 406, 454, 463, 477; `DataPipeline.test.tsx` 13; `RaiseBudget.test.tsx` 14; `PurchaseOrder.test.tsx` 17) names instead the group that holds the row the test looks at: `en.waitingGroupSetUp` for the key-refused, `preview_missing`, `designer_needs_sandbox` and `designer_needs_browser` rows; `en.waitingGroupMerge` for `integration` rows, a folder change's included (line 313); `en.waitingGroupAnswer` for every other row and for document-change cards. Line 386-387 (the Catervafication card is not in the list) asserts the card's title is in none of the three group lists (`queryAllByRole("list")` filtered by those names). No assertion about a row's content changes and no test is deleted.

- [x] `feat(web): Today groups what waits on the owner by what to do`

### Task 2: what moved before the suggested connections, two of them

Files: modified `apps/web/src/pages/Today.tsx`, `apps/web/src/pages/SuggestedConnections.tsx`, `apps/web/src/pages/Today.test.tsx`
Consumes: Task 1's `Today.tsx`

Tests:

- `puts_what_moved_before_the_suggested_connections` (new) — with one move and three suggested services (use `KIT_LINEAR` / `KIT_NOTION` fixtures as the existing connections test at `Today.test.tsx:2541` does), asserts the h2 "What moved since yesterday" precedes the h2 "Suggested connections" in document order (`compareDocumentPosition`), and the suggestions list shows two rows and a "Show all 3" button. Fails first because the connections come first and show three.
- The existing test that clicks "Show all 6" (`Today.test.tsx:2541`) keeps its words; any count of rows shown before the click changes from 4 to 2.

- [x] `feat(web): Today shows what moved before two suggested connections`

## Verification

```
pnpm --filter @catervas/web test -- Today SuggestedConnections
# expected: pass
pnpm --filter @catervas/web e2e -- accept.spec.ts request.spec.ts escalation.spec.ts chats.spec.ts
# expected: all pass; their row titles ("Mira has a question", "Theo needs your help") and links ("Answer", "Add") are unchanged
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
