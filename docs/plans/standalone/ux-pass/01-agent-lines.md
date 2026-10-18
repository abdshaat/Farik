# Standalone UX pass, plan 1: one short line per agent

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.2 (Today's band), F1's Team page; recorded by plan 8 (revision 0.123)
Depends on: nothing on this branch; `team.activity` (`crates/store/src/activity.rs`, on main), `visibly` (`apps/web/src/pages/dialogs/ToolApproval.tsx`, on main)
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065: every decision made, no ambiguity, no forward dependency)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

On Today's team band and on each Team page card, an agent that waits on the owner says "Waiting on you", not "Waiting on you: " followed by its whole question (screens 16, 22, 24 of the note-chat2 journey: Ada's line ran to 30 lines). Every band line is at most two lines tall; the whole line stays in the element's `title` and in the accessibility tree. The owner reads the question once, in its "Waiting on you" row. Out of scope: the daemon's `line` (unchanged on the wire, so the command line and older pages keep it), the Costs page's "Right now" column (plan 6 removes it), the board.

## Decisions

- The shortening is in the web app, not the store: `activity.rs`'s `line` is also what `catervas status` prints, where the whole sentence is wanted. Rejected: a second wire field (a protocol change for a display rule).
- Rule: an activity whose `state` is `"waiting"` is shown as `t("statusWaiting")` ("Waiting on you", the board's existing mark word). Every other state keeps the daemon's `line` as it is: working ("Building …", "Writing the plan for …"), resting, paused and idle lines are already one sentence about the agent. Rejected: appending the task's title ("Waiting on you: <title>"): it needs `tasks.list` on two more pages and titles are long too; the row below names the task.
- One helper, `agentLine`, in a new `apps/web/src/app/agentLine.ts`, used by `Today.tsx`'s `AgentEntry` and `Team.tsx`'s card. It returns a string; callers keep wrapping it in `visibly(...)` where they already do (Team), and Today's band adds `visibly` too, since a working line holds a task title an agent wrote.
- Clamp: `Today.module.css`'s `.line` gets the existing clamp declarations with two lines (`display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden;`). The span gets `title={shown}`, the same string it shows, so a pointer user sees a clamped working line whole. The Team card's line (`<p className={styles.muted}>`) gets no clamp: it sits in a wide card and, once waiting lines are short, is one or two lines.
- `Today.tsx`'s and `Team.tsx`'s local activity types gain `state: string` (Team's lacks it today); the wire already carries it.
- Empty and loading states are unchanged: the band draws once `team.activity` answers; a Team card shows `agentActive` ("On the team") while there is no activity row.

## File map

```
apps/web/src/app/agentLine.ts           creates: agentLine(one) -> string
apps/web/src/app/agentLine.test.ts      tests:   the one rule
apps/web/src/pages/Today.tsx            modifies: AgentEntry shows agentLine(one), visibly, with title
apps/web/src/pages/Today.module.css     modifies: .line clamps to two lines
apps/web/src/pages/Today.test.tsx       tests:   shows_the_team_band gains a waiting agent
apps/web/src/pages/Team.tsx             modifies: the card's line through agentLine; activity type gains state
apps/web/src/pages/team.test.tsx        tests:   writes_out_what_hides_text_in_an_agents_activity_line keeps passing; one assertion added
```

## Interfaces

Consumes: `t`, `en.statusWaiting` (`apps/web/src/strings`), `visibly` (`pages/dialogs/ToolApproval.tsx`).

Produces:

```ts
// apps/web/src/app/agentLine.ts
export type ActivityLine = { state: string; line: string };
/** What an agent is doing, in the words the band and the Team page show: a waiting agent's
 * question is read in its "Waiting on you" row, so here it is only "Waiting on you". */
export function agentLine(one: ActivityLine): string;
```

## Tasks

### Task 1: the short line on Today and the Team page

Files: created `apps/web/src/app/agentLine.ts`, `apps/web/src/app/agentLine.test.ts`; modified `apps/web/src/pages/Today.tsx`, `apps/web/src/pages/Today.module.css`, `apps/web/src/pages/Team.tsx`, `apps/web/src/pages/Today.test.tsx`, `apps/web/src/pages/team.test.tsx`
Produces: `agentLine`
Consumes: nothing from this branch

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `agentLine.test.ts`, `shortens_a_waiting_line_and_keeps_the_others` — asserts that `agentLine({ state: "waiting", line: "Waiting on you: CTV-24’s last check (C3) needs someone …" })` is `"Waiting on you"`, and that for each of `working` ("Building Hunt for bugs in the code"), `resting` ("Resting until 15:40 UTC"), `paused` ("Paused") and `idle` ("Nothing to do right now") it returns the line unchanged. Fails first because the module does not exist.
- `Today.test.tsx`, `shows_the_team_band` (modified, not a new test) — it passes its own `activity` to the file's `today()` helper (which answers `fields.activity ?? ACTIVITY`; the shared `ACTIVITY` fixture stays as it is): the fixture's rows with Theo's replaced by `{ agent_id: "theo", state: "waiting", line: "Waiting on you: Could you run git checkout in the worktree, then tell me so I can request verifying?", task_id: "CTV-7" }`; the test asserts that Theo's entry shows the text "Waiting on you", that `within(theo).queryByText(/git checkout/)` is null, that the span holding "Waiting on you" has `title` "Waiting on you" and the class `styles.line` (import `styles from "./Today.module.css"`), and the existing assertions (Mira's "Writing the plan for CTV-2", Ada's "Resting until 15:40 UTC", pictures opening their agents, no axe violations) still hold. Fails first because Theo's entry shows the whole question.
- `team.test.tsx`, `writes_out_what_hides_text_in_an_agents_activity_line` keeps its `state: "idle"` row and passes unchanged (an idle line is shown whole, still written out by `visibly`). In `shows_the_designer_card_in_its_colour` (line 241, whose `team.activity` answer is at line 260), add a second row `{ agent_id: "kai", state: "waiting", line: "Waiting on you: Kai asks which branch", task_id: "CTV-3" }` (Kai is in the file's `TEAM`, as `writes_out_what_hides_text_in_an_agents_activity_line` uses him) and assert Kai's card (`getByText("Kai persona").closest("li")`) shows "Waiting on you" and not "which branch". Fails first for the same reason.
- No Playwright journey changes: none asserts a band line, and whether an agent is `waiting` at a given moment of a journey depends on the store's precedence (a task under way ranks first), so the unit tests carry this change.

- [x] `feat(web): a waiting agent's line on Today and the Team page says only "Waiting on you"`

## Verification

```
pnpm --filter @catervas/web test -- agentLine Today team
# expected: all listed files pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
