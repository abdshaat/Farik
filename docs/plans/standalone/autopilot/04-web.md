# Autopilot, plan 4: the web app

Status: ready
Branch: `feat/autopilot-web` in its own worktree, from `feat/autopilot` after plan 1's Task 1; merged into `feat/autopilot` before plan 5
Spec and decisions: see `README.md`; ADR 0082 decisions 1, 4, 10

Component and test names, and what each test asserts (ADR 0008). Paths are under `apps/web/src/`. Every new string goes in `strings/en.ts`; TypeScript names are camelCase, read through the daemon client's existing mapping (hard rule 6). The tests answer the daemon's calls with the fixtures the neighbouring tests use; they need no running daemon.

## Decisions

- The switch is its own component, `pages/AutopilotSwitch.tsx` (one component per file; `TeamRules.tsx` is already 695 lines), rendered by `pages/settings/AllowedActions.tsx` after `May`, keyed `key("autopilot")`. It takes the `Part` of `useRuleParts`, which gains `autopilot?: boolean` read from `team.get`'s `autopilot` (`TeamRules.tsx` `Part` and `useRuleParts`, near line 45 and 59). Built as `Finish`'s merging switch is (`TeamRules.tsx` near 258): `Switch`, `Dialog`, `Bullets`, `InfoTip`, `Button`.
- The two presses (decision 1): pressing the switch on opens the dialog; only its **Turn on** sends `autopilot.set { on: true, confirmed: true }`. Turning off sends `{ on: false }` at once. With no daily limit (`saved.budgets.dailyUsd` absent) the switch is disabled and a line says why, linking to `/settings/spending`; a refusal from the daemon (`daily_limit_required`, `autopilot_unconfirmed`) is shown in an `alert` with its words (`saidAll`).
- Words (all new keys in `en.ts`):
  - `autopilotSwitch`: "Autopilot: let the team run the product"
  - `autopilotSwitchNote`: "Off by default. While it is on, nothing waits on you: the team decides, spends within your daily limit, and merges when checks pass.\nIt needs a daily spending limit, which is its stop."
  - `autopilotNeedsLimit`: "Set a daily spending limit first: autopilot stops at it." with the link text `autopilotNeedsLimitLink`: "Spending"
  - `autopilotWarnTitle`: "Let the team run the product?"
  - `autopilotWarn`, one bullet per line, decision by decision: "No question comes to you: agents ask the team, and the Product Manager or the Architect answers.\nThe Product Manager decides what would wait on you: it approves plans, accepts finished work, and gives more tries, more room, a new plan, or cancels.\nPaid tools, marketing plans and paid data sources run without asking, up to your daily limit of ${limit} a day.\nMessages to sellers go out when written, up to 50 a day; posts go out as planned ones do.\nCatervas merges a pull request when its checks pass, never over a failed one.\nAn agent with nothing to do takes the next ready task of its role.\nWhat only you can give, a sign-in, a key, a purchase, is flagged on Today, and the team works on.\nThe team spends what its work costs, up to your daily limit, every day; with nothing ready it stops."
  - `autopilotWarnNote`: "You can turn it off at any time. What the team decided stays decided; anything still waiting then waits on you."
  - `autopilotTurnOn`: "Turn on", `autopilotCancel`: "Cancel"
  - `flaggedTitle`: "Flagged by the team ({count})", `flaggedBy`: "{name} needs you: {needs}", `flaggedGoal`: "Your goal: {needs}"
  - `doneTitle`: "Done on its own", `doneNone`: "Nothing in the last seven days.", `doneWhy`: "Why: {reason}"
- Today (`pages/Today.tsx`): one query, `autopilot.today`. "Flagged by the team" is a section placed before "Waiting on you" (`waiting-heading`), shown only when `flagged` is not empty: one `<li>` per row, the agent's name (`by`, through the page's `agent()`, Catervas when `by` is `catervas`), the team's own words (`needs`) as text, and a link to `/tasks/<taskId>` or, for a goal row, to nothing (the goal card is on the page). "Done on its own" is a section after "What moved since yesterday" (`moved-heading`), shown when `on` is true or `done` is not empty, laid out as the approved mockup `docs/design/mockups/DoneOnItsOwn.dc.html` (phase 11 step 01's, reused per ADR 0082's Consequences): each row the time (`HH:MM`, and the day when it is not today), the agent's picture and name when `agentId` is set, the line, and `doneWhy` when `reason` is set; at most 20 rows, the rest behind a "Show all {count}" button as `WaitingGroupSection` folds its rows, with a new key `doneShowAll`: "Show all {count}". Both sections are new components in their own files, `pages/FlaggedByTheTeam.tsx` and `pages/DoneOnItsOwn.tsx`.
- Nothing else on Today changes: rows the daemon no longer lists while the team decides simply are not there.

## File map

```
pages/AutopilotSwitch.tsx               creates: the switch and its dialog (T19)
pages/settings/AllowedActions.tsx       modifies: renders it after May (T19)
pages/TeamRules.tsx                     modifies: Part.autopilot, useRuleParts reads it (T19)
pages/FlaggedByTheTeam.tsx              creates: the flagged rows (T20)
pages/DoneOnItsOwn.tsx                  creates: the acts of the last seven days (T20)
pages/Today.tsx                         modifies: the autopilot.today query, both sections (T20)
strings/en.ts                           modifies: every new string (T19, T20)
pages/settings/settings.test.tsx        tests (T19)
pages/Today.test.tsx                    tests (T20)
```

## Interfaces

Consumes: `autopilot.set`, `team.get`'s `autopilot`, `autopilot.today` (README's wire); `Switch`, `Dialog`, `Bullets`, `InfoTip`, `Button`, `useConnection`, `useQuery`, `saidAll`, `Part`, `useRuleParts` (on main).

Produces:

```ts
// pages/AutopilotSwitch.tsx
export function AutopilotSwitch(part: Part): JSX.Element;
// pages/FlaggedByTheTeam.tsx
export type FlaggedRow = { title: string; needs: string; at: string; taskId?: string; goal?: string; by?: string };
export function FlaggedByTheTeam(props: { rows: FlaggedRow[]; agents: Agent[] }): JSX.Element | null;
// pages/DoneOnItsOwn.tsx
export type DoneRow = { at: string; line: string; agentId?: string; taskId?: string; reason?: string };
export function DoneOnItsOwn(props: { rows: DoneRow[]; agents: Agent[]; today: Date }): JSX.Element;
```

## Tasks

### Task 19: the autopilot switch with its confirmation

Tests (`settings.test.tsx`):
- `turns_autopilot_on_only_from_the_dialog_and_with_a_daily_limit` — on `/settings/allowed-actions` with `dailyUsd` absent the switch is disabled and the Spending link shows; with `dailyUsd: 5`, pressing the switch sends nothing and opens the dialog listing the eight bullets of `autopilotWarn` with "$5"; Cancel sends nothing; Turn on sends `autopilot.set` with `{ on: true, confirmed: true }`; a refusal `daily_limit_required` shows its words in an alert; with `autopilot: true` the switch is on and pressing it sends `{ on: false }` with no dialog.

- [ ] `feat(web): the autopilot switch with its confirmation`

### Task 20: Today's flagged by the team and done on its own

Tests (`Today.test.tsx`):
- `shows_what_the_team_flagged_and_did_on_its_own` — with `autopilot.today` answering a flagged task (`by: "pm"`, `needs: "a Stripe key"`), a goal row and two done rows (one with a reason): "Flagged by the team (2)" is before "Waiting on you", the task row names Mira and links to `/tasks/CTV-3`, the goal row shows its words; "Done on its own" lists both lines, the reason as "Why: …"; with `on: false` and nothing flagged or done neither section is shown.

- [ ] `feat(web): today's flagged by the team and done on its own`

## Verification

```
cd apps/web && pnpm check
# expected: exit 0, every test passing
```
