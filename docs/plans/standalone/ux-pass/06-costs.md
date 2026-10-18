# Standalone UX pass, plan 6: Costs reads as a bill

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` F17 (harness metrics, shown in the app); recorded by plan 8 (revision 0.123)
Depends on: nothing on this branch; `Costs.tsx`, `RoleTag`, `InfoTip` (`@catervas/ui`), `Bullets`, on main
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Costs page (screens 13, 26) answers "who spent what today" at a glance and "how well does the team work" in four numbers. The per-agent table loses its "Right now" column, which repeated the agents' long lines (Ada's ran to nine lines); the agents are sorted by spend; chats are the table's last row; the resting paragraph and every metric's note sit behind ⓘ buttons; the four metrics are tiles; active weeks and message counts fold under "More numbers". Out of scope: the head line and "Set a daily limit" (unchanged), the "Made on other services" section, the `DailyLimit` dialog, the daemon's queries (`costs.summary`, `metrics`; `team.activity` is no longer asked here).

## Decisions

- Table: two columns, `costsAgent` "Agent" and `costsTodayColumn` "Today". `costsNow` "Right now" is deleted with the column, and `Costs.tsx` stops asking `team.activity` (its only use). Rows sorted by `todayUsd`, highest first; equal amounts keep the daemon's order (a stable sort).
- The agent cell: the avatar (32 px) and the name, then the role as `<RoleTag role={agent.role} />` on the same line, in place of the role's words on their own line (`board.small` + `roleName`). The amount cell: `class={own.amount}`: `text-align: right; font-family: var(--catervas-type-code-family); white-space: nowrap;` (the brand sets costs in monospace); the "Today" header cell right-aligned too.
- Chats: a last row in `<tbody>`: a `<th scope="row">` with `costsChatsRow` "Your chats with the team" and `<InfoTip id="chats-info">{t("costsChatsInfo")}</InfoTip>` (`costsChatsInfo` "They count toward the daily limit."), and the amount cell `dollars(summary.conversationsTodayUsd)`. `costsConversations` and `costsConversationsWhy` are deleted.
- Resting: the `<p className={styles.muted}>{t("costsResting")}</p>` goes; the "By agent" `<h2>` sits in a `div.titled` (`pages.module.css`) with `<InfoTip id="resting-info"><Bullets text={t("costsResting")} /></InfoTip>`. `costsResting` becomes three lines: "An agent whose AI account reached its usage limit rests.\nIt picks up again at the time Today shows.\nThe rest of the team keeps working."
- Metrics: the `<dl className={own.metrics}>` becomes a grid of four tiles, `grid-template-columns: repeat(auto-fill, minmax(min(100%, 12rem), 1fr))`, gap `--catervas-space-3`. Each tile (`div` in the `dl`) has `background: var(--catervas-color-surface); border: 1px solid var(--catervas-color-rule); border-radius: var(--catervas-radius-raised); padding: var(--catervas-space-3);` and, in this order: the value `<dd className={own.value}>` in the title type token (`--catervas-type-title-*`), then the label `<dt>` in the small type and `ink-muted`, with the note, when there is one, as `<InfoTip id="metric-<key>-info">` beside the label (inside the `dt`). In the DOM the `dt` still precedes the `dd` (valid `dl`); the value shows first by `order: -1` on the `dd` in a `display: flex; flex-direction: column` tile. The not-yet value (`metricNotYet`) shows in the body type, not the title type, since it is a sentence. The cost tile's breakdown ("Planning $0.62, Building $1.88, …") is its ⓘ note.
- "More numbers": `metricWeeks` and `metricMessages` move into `<details className={own.more}><summary>{t("costsMore")}</summary>…</details>` (`costsMore` "More numbers") under the tiles; their words unchanged. `costsWholeProject` "Across the whole project." stays visible under the heading (a short scope line).
- 360 px: the table has two short columns and fits; tiles stack one per row (`min(100%, 12rem)`).

## File map

```
apps/web/src/pages/Costs.tsx            modifies: the table, the chats row, the ⓘs, the tiles, "More numbers"; no team.activity
apps/web/src/pages/Costs.module.css     modifies: .amount, .metrics as tiles, .more
apps/web/src/strings/en.ts              modifies: costsResting as bullets; costsChatsRow, costsChatsInfo, costsMore added; costsNow, costsConversations, costsConversationsWhy deleted
apps/web/src/pages/Costs.test.tsx       tests:   shows_the_costs and shows_conversations_on_costs rewritten; the helper stops answering team.activity
apps/web/src/pages/allowances.test.tsx  tests:   the two /costs tests stop answering team.activity (lines 532, 605)
```

## Interfaces

Consumes: `RoleTag`, `InfoTip` (`@catervas/ui`), `Bullets` (`components/Bullets.tsx`), `styles.titled` (`pages.module.css`), `dollars` (`pages/Gate.tsx`).

Produces: nothing exported.

## Tasks

### Task 1: the table as a bill

Files: modified `apps/web/src/pages/Costs.tsx`, `apps/web/src/pages/Costs.module.css`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/Costs.test.tsx`, `apps/web/src/pages/allowances.test.tsx`
Consumes: nothing from earlier plans

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `shows_the_costs` (its first part rewritten) — the helper `costs()` no longer answers `team.activity`. Asserts the column headers are `["Agent", "Today"]`; the body rows in order are Theo ($2.94) then Mira ($1.48) then the chats row (sorted by spend; the fixture has Mira first); Theo's row has the role tag titled "Developer" and the cells `["$2.94"]`; the last row's header is "Your chats with the team" and its cell "$0.42"; the text "They count toward the daily limit." is only in a tip (`onlyInTips`); each line of `en.costsResting` is only in a tip; no query `team.activity` was asked (`s.calls("query")`); no axe violations. Fails first because there are three columns and Mira comes first.
- `shows_conversations_on_costs` — deleted: the chats row is asserted in `shows_the_costs` above, and its sentence is gone. (One test per major functionality, ADR 0060.)
- `allowances.test.tsx` lines 532 and 605: delete `await answerQuery(s, "team.activity", { activity: [] });` in the two tests that open `/costs`; nothing else changes. They fail first (waiting for a query never asked) only after the code change, so run them in the same red-green cycle.

- [x] `feat(web): Costs lists each agent's spend, highest first, with chats as the last row`

### Task 2: the four numbers as tiles

Files: modified `apps/web/src/pages/Costs.tsx`, `apps/web/src/pages/Costs.module.css`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/Costs.test.tsx`
Consumes: Task 1's page

Tests:

- `shows_the_costs` (its second and third parts updated) — with `METRICS`: the tile values "6 of 9", "1.3", "$3.07", "71%" each sit in a `dd` with the class `own.value`; `en.metricFirstTryNote`, `en.metricNeededNote`, `en.metricChecksNote` and "Planning $0.62, Building $1.88, Checking $0.41, Meetings and talk $0.16." are each only in a tip (`onlyInTips`); "Active weeks: 2" and "Messages: 4 reactions, 3 replies, 7 from meetings, 5 from you" are inside a `details` whose summary is "More numbers"; with `first_pass_acceptance_rate: 1`, "9 of 9" shows and `metricFirstTryNote` is absent from the page, tips included; with no work accepted, four `metricNotYet` as now. Fails first because the notes are visible lines.

- [x] `feat(web): Costs shows the team's four numbers as tiles, their notes behind ⓘ`

## Verification

```
pnpm --filter @catervas/web test -- Costs allowances
# expected: pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
