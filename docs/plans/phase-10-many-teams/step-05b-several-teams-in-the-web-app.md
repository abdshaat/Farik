# Phase 10, step 05b: Several teams in the web app

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 4.1, 4.4; ADR 0059
Depends on: step 05 (committed): the hub's `/t/<team>/rpc` and `/hub/rpc`, `teams.list`, `teams.current`, `teams.remove`, `computer.get` and `computer.set`
Mockups: `docs/design/mockups/ManyAllTeams.dc.html` (frames 1 and 2; frame 1's Start, Document it first and Undo, and its Watched section, are step 06b's), approved by the founder with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

The owner sees and switches between every team on this computer:

- A team switcher sits at the top of the rail.
- Each team's pages live under `/t/<team>/`, exactly as today's pages.
- An **All teams** page shows a card per team: its state, agents working, waiting on you, spent today and this month, and Start for a paused team.
- **This computer** sets "Agents working at once" and the computer's daily limit.
- An old address (`/board`, a bookmark) opens the same page of the last team viewed.

With one team, nothing changes: `/` opens that team's Today, and the switcher shows its name.

Out of scope:
- finding projects, "Find more projects" (06), and the Watched section, Undo and "Document it first" (06b);
- the reminders band (07);
- Analytics (08);
- the "next scheduled job" line on a card (07, which reads jobs' schedules).

## Decisions

- **Two connections.**
  - The hub's socket (`/hub/rpc`) is always open while a session exists. It answers `teams.list`, `computer.*` and the setup methods.
  - The team's socket (`/t/<team>/rpc`) opens for the team in the address and is replaced when the address changes team.

  `ConnectionProvider` gains a `path` prop. `App` renders one provider for the hub and, inside `/t/:team/*`, a second for the team. The hooks (`useQuery`, `useEvents`, the client) read the nearest provider. A new `useHub()` reads the hub's. The setup pages and All teams use the hub's. Rejected: one socket carrying a team on every call, which would change every query of every page.
- **Routes** (`App.tsx`):
  - `/t/:team/*` holds today's Shell routes unchanged: index Today, `board`, `channel`, `files`, `team`, `costs`, `settings/*` and so on.
  - `/teams` is All teams, in the Shell with the computer's rail.
  - `/computer` is This computer.
  - `/setup/*` is unchanged and on the hub.
  - Any other address with no `/t/`, the old routes included, goes through `TeamRedirect`. It sends to `/t/<last viewed>/<same path>` when that team is in `teams.list`, else to the only team when there is one. With two or more teams and none remembered, it sends to `/teams`. With none, it goes to the landing as today.
  - The last team viewed is kept in `localStorage` (`catervas.lastTeam`), read and written in try/catch.
- **The landing** (`landing.ts`) reads the hub's `serve.status`. With no project, setup is as today, and `projectRoot` means "at least one team". `TeamRedirect` decides which team.
- **The rail.** Its top is a switcher button with the current team's name, or "All teams" on `/teams` and `/computer`. It opens a menu with the label "Teams" (`role="menu"`), as the mockup's frame 2 draws:
  - "All teams";
  - a divider;
  - one item per team: name, and a status line of "N waiting", "working", "paused", "driven from a terminal", "can't be found" or "stopped: <first 40 characters>";
  - a divider;
  - "Add a team", which goes to `/setup/project` and becomes "Find more projects" in 06.

  Inside a team, the rail's places are today's `PLACES`, prefixed with `/t/<team>`. On `/teams` and `/computer` the rail is "All teams" and "This computer". The phone bar follows the same rule. The rail's foot keeps its connected dot and the team's pause control; the project name and "Change" move into the team's Settings, Computer.
- **Changing team** calls `teams.current { slug }` on the hub, so that `/rpc` and `catervas open` follow the owner, then navigates to the same page of the new team. A task, request or file page goes to the team's Today instead, since its id is the old team's.
- **All teams** (`pages/AllTeams.tsx`), from `teams.list`, refreshed on any hub event and every 10 seconds:
  - H1 "All teams".
  - One card per team, in `teams.list`'s order:
    - The name links to `/t/<team>/`.
    - A badge reads "N waiting on you" when `waiting` > 0. Otherwise it reads "Working" when `working_agents` > 0, "Paused", "Driven from a terminal" (`elsewhere`), "Can't be found" (`missing`), or "Stopped" (`failed`, with the message under it).
    - Values: `working_agents` "agents working" (singular "agent working"), `$today` "today", `$month` "this month", in the `formatUsd` the Costs page uses.
    - A paused team has **Start**, which sends the team's `team.resume` on a short-lived team socket and refreshes.
    - A missing team has **Remove**, which confirms with "Remove <name> from Catervas? Its folder isn't touched." before `teams.remove { slug, confirmed: true }`.
  - Below the cards, "Waiting on you" lists each team with `waiting` > 0 as "<name>: N things", each linking to that team's Today.
  - With `account_asleep_until` set, a line reads "Your AI account's limit is reached. Your teams go on at <local time>."
- **This computer** (`pages/ThisComputer.tsx`), from `computer.get`:
  - "Agents working at once": "No limit" (default) or a number from 1 to 16, with the hint "Across all your teams. Useful when they share one Claude account."
  - "Daily limit for this computer": "No limit" or dollars, with the hint "All your teams together stop for the day when they reach it. Each team's own limit still applies."
  - "Today, all teams: $x".
  - Save sends `computer.set`. Refusals map in `refusals.ts`.
- **Strings** go in `en.ts` under `teams*` and `computer*`.

## File map

```
apps/web/src/app/connection.tsx        modifies: path prop; useHub
apps/web/src/app/App.tsx               modifies: /t/:team/*, /teams, /computer, TeamRedirect
apps/web/src/app/landing.ts            modifies: from the hub's status
apps/web/src/app/teams.ts              creates: lastTeam storage, teamPath(team, path), the redirect rule
apps/web/src/shell/Shell.tsx           modifies: the switcher, places by team or computer
apps/web/src/shell/TeamSwitcher.tsx    creates: the switcher and its menu
apps/web/src/pages/AllTeams.tsx        creates
apps/web/src/pages/ThisComputer.tsx    creates
apps/web/src/pages/settings/Computer.tsx  modifies: the project name and Change move here
apps/web/src/app/refusals.ts, strings/en.ts  modifies
apps/web/src/test/render-app.tsx       modifies: a fake hub socket beside the team's
apps/web/src/**/*.test.tsx             tests (below)
apps/web/e2e/fixtures/serve.ts         modifies: startServe({ teams: 2 }) writes two projects in state.json
apps/web/e2e/many-teams.spec.ts        creates: the journey
```

Every link inside a team page that is absolute today (`to="/board"`) goes through `teamPath`. A test lists the `to=` and `navigate(` strings in `src/pages` and `src/shell` and fails on an absolute one, outside setup, `/teams` and `/computer`.

## Tasks

### Task 1: Two connections and the team's routes

Tests:
- `opens_the_team_in_the_address` (`app/teams.test.tsx`): asserts these.
  - Rendering `/t/shop/board` opens a socket to `/t/shop/rpc` and a socket to `/hub/rpc`.
  - The board's queries go to the team's socket.
  - Navigating to `/t/bakery/` closes the shop socket and opens bakery's.
- `sends_an_old_address_to_the_last_team` (`app/teams.test.tsx`): asserts these.
  - With `lastTeam` `shop` and `teams.list` holding bakery and shop, `/board` goes to `/t/shop/board`.
  - With no last team and two teams, `/` goes to `/teams`.
  - With one team, `/` goes to `/t/<it>/`.
  - With none, it goes to `/setup/computer`.
- `keeps_every_team_link_inside_its_team` (`app/links.test.ts`): asserts the rule of the File map.

- [ ] `feat(web): each team's pages under its own address`

### Task 2: The switcher

Tests (`shell/TeamSwitcher.test.tsx`):
- `switches_team_from_the_rail`: asserts these.
  - The rail's button names the current team.
  - Opening it shows "All teams", each team with its status line ("1 waiting", "working", "paused") and "Add a team".
  - Choosing bakery sends `teams.current { slug: "bakery" }` and goes to bakery's same page, or its Today from `/t/shop/tasks/CTV-3`.
  - The menu has `role="menu"` and closes on Escape.

- [ ] `feat(web): switch teams from the rail`

### Task 3: All teams and This computer

Tests:
- `shows_a_card_per_team` (`pages/AllTeams.test.tsx`): asserts these.
  - With `teams.list` holding a working team (2 agents, $4.10, $61.80, 1 waiting), a paused one and a missing one:
    - the cards show "1 waiting on you", "2 agents working", "$4.10 today", "$61.80 this month";
    - the paused card has Start, which sends `team.resume`;
    - the missing card has Remove, which confirms and then sends `teams.remove` with `confirmed`;
    - "Waiting on you" lists the first team.
  - With `account_asleep_until` set, the limit line shows.
- `sets_the_computer_s_limits` (`pages/ThisComputer.test.tsx`): asserts these.
  - Setting 2 agents and $25 sends `computer.set { agentsAtOnce: 2, dailyCapUsd: 25 }`.
  - "No limit" sends `null`.
  - The refusal `agents_at_once_out_of_range` shows its plain words.

- [ ] `feat(web): all teams and this computer`

### Task 4: The journey with two teams

`startServe({ teams: 2 })` makes two projects, each with its team (as `project: true` does), and writes both into `state.json` under the test's `XDG_CONFIG_HOME`.

The test (`e2e/many-teams.spec.ts`), "the owner moves between two teams and sees both on All teams":
1. It opens the link and lands on All teams, since there are two teams and none is remembered. Both cards show.
2. It opens the first team, sees its Today, switches to the second from the rail, and sees the second's Today.
3. It opens `/board` and lands on the second team's board.

- [ ] `test(web): move between two teams`

### Task 5: Spec

`docs/SPEC.md` revision 0.145 (2026-10-22), "records phase 10 step 05b, several teams in the web app". Insert it above 0.144. It covers 4.1 and 4.4: the switcher, the addresses, All teams and This computer. Then the project plan's row 05b: "planned and executed 2026-10-22 (`step-05b-several-teams-in-the-web-app.md`)".

- [ ] `docs(spec): several teams in the web app, revision 0.145`

## Verification

```
python3 reaper.py cargo xtask check
pnpm -C apps/web exec playwright test e2e/many-teams.spec.ts
# expected: xtask check: ok; the journey passes
```
