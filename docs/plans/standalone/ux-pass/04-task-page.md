# Standalone UX pass, plan 4: the task page says what matters first

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` F3 (open a task), 4.2; recorded by plan 8 (revision 0.123)
Depends on: nothing on this branch; `TaskDetail.tsx` on main (with ADR 0068's blocked section)
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A task's page (screen 21) opens on what the owner wants to know: who is doing it, what the agent last said, and whether the checks pass, in one line. The try count shows only once there has been a second try. The checks' text, written for agents, sits behind "Show each check". "Adding it to your project" shows only when there is something to say. While the page reads its ten queries it says "Loading…" instead of a blank page. Out of scope: the History, The plan, Code changes and Notes tabs' content, the Designer's "How Iris works" panel, the cost table (the board journey reads its rows), the blocked section (ADR 0068) and its "Answer and carry on" link.

## Decisions

- The lead (`lead` in `TaskDetail`) keeps `${id}.` and who does and reviews it; the try part (`taskTry` "Try {try} of {of}.") is added only when `tries.try > 1`. The Designer's plan count (`designPlanCount…`) is unchanged. So a first try reads "CTV-1. Theo is doing it, and Ada reviews it." Rejected: removing the id (the owner talks about tasks by id in the chats; ADR 0066 keeps ids off GitHub only).
- Tabs: `tabSummary` becomes "Summary"; `tabChanges` stays "Code changes" (it says what the tab holds to a non-technical owner better than "Changes"). The other three are unchanged.
- The Summary panel's order: (1) the latest completion or review note, signed, as now (`own.letter`); (2) the Designer's review (`DesignReview`), as now; (3) `<h2>{t("taskFor")}</h2>` and the intent; (4) the checks. Rejected: dropping the intent (it is often the owner's own request).
- Checks: when the contract has no exit criteria the checks part is not drawn. Otherwise `<h2 id="checks">{t("taskChecks")}</h2>` with `taskChecks` "Checks" (was "What Catervas checks"), then `<p>` `checksSummary` "{passed} of {count} checks passed.", followed, when any failed last time, by a space and `checksFailedLast` "{n} failed last time."; then `<details className={page.checks}><summary>{t("checksShowEach")}</summary>` holding the existing `<ul className={own.checks} aria-labelledby="checks">` unchanged ("Show each check"). Not run yet is `count − passed − failed` and is not said in the line (the list says it per check).
- "Adding it to your project" is drawn only when `diff.privateFolder || integrated || awaitsIntegration || contract.status === "accepted"`. Its hint (`taskAddHint`) moves from a muted `<p>` into `<InfoTip id="adding-info">` beside the `<h2>`, both in a `div.titled` (`pages.module.css`); the hint's words are unchanged. `taskAddNotYet` is never shown and is deleted from `en.ts`.
- Loading: while any query has not answered, the page draws `page.layout` > `page.main` with the back link (`taskBack`) and `<p role="status" className={styles.muted}>{t("pageLoading")}</p>`, new key `pageLoading` "Loading…". Rejected: a skeleton (a new visual pattern for one page).
- `page.checks` in `TaskDetail.module.css`: `summary { cursor: pointer; min-height: 24px; }`, `summary:focus-visible { outline: 2px solid var(--catervas-color-focus); outline-offset: 2px; }`, the list `margin-top: var(--catervas-space-2)`. No colour beyond the focus token.

## File map

```
apps/web/src/pages/TaskDetail.tsx         modifies: lead, summary order, checks line and details, adding section condition and ⓘ, loading
apps/web/src/pages/TaskDetail.module.css  modifies: .checks
apps/web/src/strings/en.ts                modifies: tabSummary, taskChecks; checksSummary, checksFailedLast, checksShowEach, pageLoading added; taskAddNotYet deleted
apps/web/src/pages/TaskDetail.test.tsx    tests:   shows_the_five_tabs rewritten; lead strings without "Try 1 of 4."; offers_add_stop_and_cancel_when_they_apply updated; says_it_is_loading added
apps/web/e2e/board.spec.ts                tests:   the lead without "Try 1 of 4."
```

## Interfaces

Consumes: `InfoTip` (`@catervas/ui`), `styles.titled` (`pages.module.css`).

Produces: nothing exported; `TaskDetail`'s route and queries are unchanged.

## Tasks

### Task 1: the summary first, the checks in a line

Files: modified `apps/web/src/pages/TaskDetail.tsx`, `apps/web/src/pages/TaskDetail.module.css`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/TaskDetail.test.tsx`, `apps/web/e2e/board.spec.ts`
Consumes: nothing from earlier plans

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `shows_the_five_tabs` (rewritten in place) — asserts the lead "CTV-1. Theo is doing it, and Ada reviews it." (no "Try"); tab names `["Summary", "History", "The plan", "Code changes", "Notes"]`; the keyboard part unchanged (End → "Notes", Home → "Summary"); in the Summary panel, the signed note "Ada, your Architect, reviewed it" precedes the heading "What this task is for" in document order, which precedes the heading "Checks"; the panel shows "1 of 3 checks passed. 1 failed last time." (the file's three criteria: C1 passed, C2 failed, C3 not run); a `details` element whose summary is "Show each check" holds the three list items with the same text the old test asserts ("A test purchase of each amount goes through.Passed", "…Failed last time", "…Not run yet"); the History part and the rest of the test unchanged; no axe violations. Fails first because the tab is "Summary and checks" and the checks are a bare list.
- The four leads at `TaskDetail.test.tsx:947, 961, 971, 980` drop " Try 1 of 4."; add, in the test that holds line 961, one case where `task.tries` answers `{ try: 2, of: 4 }` and the lead reads "CTV-1. Theo is doing it, and Ada reviews it. Try 2 of 4." (if that test's helper does not take a `task.tries` override, pass it through `opened`'s `overrides`, which `openedGate` spreads over its answers).
- `offers_add_stop_and_cancel_when_they_apply` (updated) — in its first part (in review, not accepted), `queryByRole("region", { name: "Adding it to your project" })` is null instead of showing "Not yet: …"; its later parts, where the task is accepted and waits to be added, still find the region and "Add to the project", and the hint text is only inside a tip (`onlyInTips(en.taskAddHint)`).
- `says_it_is_loading` (new) — renders `/tasks/CTV-1` with the connection up but `contract.get` not yet answered (use the same `renderApp` and `answerStatus` the file's `openedGate` uses, answering `team.get` only); asserts `getByRole("status")` has text "Loading…" and the link "Back to the board" is present (`taskBack`'s words; read them from `en.ts`). Fails first because the page renders nothing.
- `e2e/board.spec.ts` line 98: expect "CTV-1. Theo is doing it, and Ada reviews it." (exact text; the e2e task is on its first try). Before line 107's `panel.getByText("Passed")`, click `panel.getByText("Show each check")`: in Chromium the checks are hidden inside the closed `details` (jsdom does not hide them, so the unit test alone would not catch it).

- [x] `feat(web): the task page shows the agent's note first and its checks in one line`

## Verification

```
pnpm --filter @catervas/web test -- TaskDetail
# expected: pass
pnpm --filter @catervas/web e2e -- board.spec.ts
# expected: all pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
