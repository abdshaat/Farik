# Phase 10, step 08: Analytics

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 4.4, 8.4, F17; ADR 0059
Depends on: steps 02 to 07 (committed)
Mockups: `ManyAnalytics` frames 1 and 2, approved with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

**Analytics** on All teams shows every team side by side, for this week or this month, in three parts:

- **Usage**, split between the owner's own Claude Code sessions in the team's projects and the team's Catervas agents, counted once per reply. On a subscription it is labelled as what it would cost on Claude's API. claude.ai conversations count as activity only.
- **Workflow:** sessions, active hours, commits by the owner and by the team, pull requests, the tools used most, and sessions per day.
- **Blockers:**
  - waiting on you;
  - failing or silent schedules;
  - failed deploys;
  - tasks blocked over three days;
  - forgotten sessions;
  - permission denials.

In each team, the Costs page gains an **Activity** tab with the same split for that team, its task cycle time and its throughput.

## Decisions

- **Usage of the owner's own sessions** comes from `machine.db`'s `replies` (one row per reply, step 02), for sessions placed in the team's project roots or members. Each reply is priced by `catervas_core::pricing`'s table by its model and four token counters. A model missing from the table counts its tokens with no cost, and the page says "n replies on models without a price".
- **Usage of the team's agents** comes from the team's log, `session.ended`'s `cost_usd`, summed as the Costs page does.
- **The two never overlap.** Catervas's own agents' sessions are placed `catervas` by step 02 and are excluded from the owner's.
- **"What this would cost on the API"** labels the whole usage when the account kind (`account.status`, pull request #92) is `subscription`. On `console` or `environment` it reads "What you spent".
- **Workflow:**
  - **sessions:** the owner's sessions with a reply in the period, plus the team's `session.started` count;
  - **active hours:** for the owner, from reply times, summing gaps under 5 minutes; for the team, from session durations;
  - **commits:** `git rev-list --count --since --until` on the integration branch of each root. The owner's are those not authored by Catervas's identity (`catervas_store::git::CATERVAS_IDENTITY`), and the team's are those authored by it;
  - **pull requests:** from GitHub when connected (`GET /repos/{full}/pulls?state=all&per_page=100`, created in the period); else the team's own `pr.opened` events; else omitted;
  - **used most:** the top 3 tool names of the owner's sessions in the period. Step 03b's `tool_calls` gives the call kind. Built-ins are named in words ("Editing files" for `Edit`, `Write` and `MultiEdit`; "Running commands" for `Bash`; "Reading files" for `Read`), and a service by its catalog name;
  - **sessions per day:** seven bars for the week, or the month's days.
- **Blockers**, from step 07's reminder input and each team's log:
  - **waiting on you:** `waiting.list`'s count and oldest age;
  - **failing or silent schedules** and **failed deploys:** step 07's schedule items, by name;
  - **blocked over 3 days:** tasks in `blocked` for more than 3 days, by title;
  - **forgotten sessions:** step 07's count;
  - **permission denials:** the team's `tool.denied` events in the period.
- **The method** is `analytics.get { period: "week"|"month" }` (hub). It answers:

  `{ period, billing: "api"|"spent", teams: [{ slug, name, owner_usd, agents_usd, unpriced_replies }], watched_usd, claude_ai_conversations, workflow: {…}, blockers: [{ kind, count, examples: [string] }] }`

  - `watched_usd` is the owner's usage in watched projects with no team.
  - A team's own `analytics.mine { period }` answers its part, plus `cycle_days` (the mean from `in_progress` to `done` for tasks done in the period) and `throughput` (tasks done).
  - The week is Monday to Sunday, in the computer's local time; the month is the calendar month.
- **No new table.** Everything is read from what is kept. A heavy query is computed on demand, with a 60-second cache in the hub per period.

### The screens

- `/analytics` (the rail's "Analytics" on All teams), from `ManyAnalytics` frame 1:
  - **This week** and **This month** tabs.
  - "Usage per team", with its ⓘ bullets (the plan line only on a subscription), and the legend "You, in Claude Code · The team's agents".
  - One stacked bar per team, its two amounts written beside it as "$41.20 + $18.60", and "Watched $1.30".
  - The claude.ai line: "claude.ai: 14 conversations this week, no usage figures in the export."
  - **See as a table**, which shows the same numbers in a table.
- "Workflow": the four numbers ("23 sessions", "17 h active", "31 · 12 commits: you · team", "6 pull requests"), the sessions-per-day bars with the number above each, and "Used most" with counts.
- "Blockers": one row per kind with a count and its examples.
- Charts are plain SVG in the app's tokens. Every bar carries its number in text, and each chart has a table alternative. They are drawn by a small `BarChart` in `packages/ui`.
- In a team, the Costs page gains tabs **Spending** (today's page) and **Activity**, from frame 2: the team's usage bar, "1.6 days a task, start to done", "9 tasks done", "14 · 12 commits: you · team", "1 waiting on you".

## File map

```
crates/store/src/analytics.rs            creates: the owner's usage, sessions, active hours and tools from machine.db
crates/runtime/src/analytics.rs          creates: the team's usage, cycle time, throughput, denials and blocked from its log; commits through git
crates/runtime/src/daemon/analytics.rs   creates: analytics.get (hub) and analytics.mine (team), the cache
crates/runtime/src/github.rs             modifies: pull requests created in a period
docs/schemas/rpc.schema.json, crates/protocol, packages/protocol-client   modifies
packages/ui/src/BarChart.tsx             creates (with its test)
apps/web/src/pages/Analytics.tsx         creates
apps/web/src/pages/Costs.tsx             modifies: Spending and Activity tabs
apps/web/src/shell/Shell.tsx             modifies: Analytics in the computer's rail
docs/SPEC.md                             modifies: revision 0.152
```

## Tasks

### Task 1: The owner's usage and workflow from the index

Tests (`crates/store/src/analytics.rs`):
- `counts_the_owner_s_usage_once_per_reply`: asserts these.
  - Over fixture replies in two projects, a resumed session repeating 3 replies, and a Catervas agent's session, the owner's usage for project A in the week equals the sum of its distinct replies, priced by the table.
  - The agent's session is excluded.
  - A reply on an unknown model counts as unpriced.
  - Active hours sum gaps under 5 minutes: replies at 10:00, 10:03 and 10:20 give 3 minutes.
  - "Used most" names "Editing files" for `Edit` and `Write` calls.

- [x] `feat(store): the owner's usage and workflow, counted once per reply`

### Task 2: The team's activity from its log

Tests (`crates/runtime/src/analytics.rs`):
- `measures_a_team_s_activity`: asserts these.
  - From a seeded log, agents' usage is the sum of `session.ended` costs in the period.
  - Cycle time is the mean of `in_progress` to `done` for tasks done in the period: 1 and 2.2 days give 1.6.
  - Throughput is 2.
  - A task blocked for 4 days is listed and one for 2 days is not.
  - The `tool.denied` count is right.
  - Commits split by Catervas's identity in a test repository (ignored, git).

- [ ] `feat(runtime): measure a team's activity`

### Task 3: The methods

Tests (`crates/runtime/src/daemon/analytics.rs`):
- `answers_analytics_for_every_team_and_each`: asserts these.
  - `analytics.get { period: "week" }` answers both teams' usage, watched usage, workflow and blockers.
  - `billing` is `api` under a subscription account and `spent` otherwise.
  - A second call within 60 seconds is served from the cache (the store is called once).
  - `analytics.mine` answers the team's part with cycle time.
  - The schema holds.

- [ ] `feat(runtime): analytics for all teams and each`

### Task 4: The screens

Tests:
- `draws_bars_with_their_numbers` (`packages/ui/src/BarChart.test.tsx`): asserts that each bar's value is in its text and the table alternative holds the same numbers.
- `shows_usage_workflow_and_blockers` (`pages/Analytics.test.tsx`): asserts these.
  - "Tidepool $41.20 + $18.60" and "Watched $1.30" show.
  - The ⓘ says what the API would cost under `billing: "api"`.
  - **This month** sends `analytics.get { period: "month" }`.
  - The workflow numbers and blockers show with the mockup's words.
- `shows_a_team_s_activity` (`pages/Costs.test.tsx`): asserts that the Activity tab shows "1.6 days a task, start to done" and "9 tasks done", and that Spending is today's page.

- [ ] `feat(web): analytics on all teams and in each team`

### Task 5: Spec

`docs/SPEC.md` revision 0.152 (2026-10-22), "records phase 10 step 08, analytics". It covers F17 and 4.4: what is counted and from where, the once-per-reply rule, the API label, and the screens. Then the project plan's row 08: "planned and executed 2026-10-22".

- [ ] `docs(spec): analytics, revision 0.152`

## Verification

```
python3 reaper.py cargo xtask check
# expected: xtask check: ok
```
