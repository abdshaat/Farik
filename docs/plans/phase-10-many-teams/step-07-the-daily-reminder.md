# Phase 10, step 07: The daily reminder

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 4.4, 6.7, 8.4, 8.6, F12; ADR 0059
Depends on: steps 02 to 06e (committed); the GitHub connection of step 06d
Mockups: `ManyReminders` frames 1 to 3; `ManyAllTeams` frame 2 (the band); `ManyStates` "GitHub not connected"; `ManyFixMistake` frame 2 (the Reminders section of This computer); approved with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

Once a day, at a time the owner picks (9:00 local by default), Catervas gathers what needs the owner into one digest.

What it gathers:
- sessions they may have forgotten;
- session crons ending soon;
- scheduled runs that failed or went silent, workflows GitHub turned off, and deployments that failed;
- each team's waiting and aged escalations;
- connectors Claude Code says need signing in again.

Where it shows:
- "Needs you today" on All teams, with each team's part on its Today;
- the band on All teams;
- one notification per digest: a browser notice when a Catervas tab is open, else one from the operating system, with Windows' under WSL.

What each item offers: Snooze (a day, a week), Dismiss (until it changes), Copy resume command or Open on claude.ai, Give to the team (a request filed in the project's team), and Open on GitHub.

## Decisions

### The rules, in core (`catervas_core::reminders`)

- `reminders(input: &ReminderInput, now) -> Vec<Reminder>` is pure. Its input:
  - sessions with `last_at`, `open_todos`, `uncommitted`, `unpushed`, `draft_pull_quiet_days` and `waiting_job`;
  - session crons with `expires_at` and `recurring`;
  - repository jobs with runs (`last_run_at`, `last_conclusion`, interval);
  - disabled workflows;
  - latest deployments;
  - teams' waiting and aged escalations;
  - connectors needing sign-in;
  - snoozes and dismissals.
- The rules, in groups:

  | Group | Rule | Item |
  |---|---|---|
  | `forgotten` | last activity between 1 and 14 days ago, and (open todos > 0, or uncommitted changes, or unpushed commits, or a draft pull request quiet ≥ 3 days, or a background job waiting for input) | "<title> · <project>", detail "<n> days ago · <reason>" |
  | `ending` | a session cron expiring within 24 hours, or expired within 7 days while recurring | "“<prompt start>” ends tomorrow" / "ended" |
  | `schedules` | a scheduled run whose last conclusion is failure | "<job> failed" |
  | `schedules` | no run for twice the interval | "<job> hasn't run since <time>" |
  | `schedules` | a workflow GitHub disabled (`disabled_inactivity`) | "GitHub turned off <job>" |
  | `schedules` | the latest deployment failed | "Last deploy failed" |
  | `teams` | a team's oldest waiting item older than 1 day | "<team>: <agent> has waited <n> days for your answer" |
  | `teams` | an escalation aged | as `escalation.aged` |
  | `services` | a connector Claude Code lists in `mcp-needs-auth-cache.json` | "<service> needs you to sign in again in Claude" |
  | `runs` | today's runs, collapsed | — |

  `mcp-needs-auth-cache.json` is read for server names only.
- **Item keys** are stable: `<rule>:<thing key>`, for example `forgotten:<session id>`. A dismissal keeps the item's `fingerprint`, a hash of its detail fields. A dismissed item comes back only when its fingerprint changes. A snooze hides it until its time.
- **Uncommitted and unpushed** are read by the store with `git status --porcelain` and `git rev-list @{u}..HEAD --count` in the session's folder, when it is a present git repository. They are never read for a folder outside home.
- **Open todos** are the count of the session's last `TodoWrite` tool call's items whose status is not `completed`. Step 03b's tool calls gain `open_todos` for `TodoWrite` alone, read at reading time; no todo text is kept.

### Runs through GitHub (step 06d's client)

- For each found repository on GitHub with a scheduled workflow (step 03's jobs of kind `github_actions`), the client reads:
  - `GET /repos/{full}/actions/workflows` for each workflow's `state`;
  - `GET /repos/{full}/actions/workflows/{file}/runs?event=schedule&per_page=1` for the last scheduled run's `conclusion` and `run_started_at`;
  - `GET /repos/{full}/deployments?per_page=1` and that deployment's `statuses?per_page=1` for the latest state. Vercel, Netlify and Render report theirs there.
- They are read at the reminder's time and at the daily rescan, at most 50 repositories, and kept in migration 11's `github_runs(full_name, workflow, state, last_conclusion, last_run_at, PRIMARY KEY (full_name, workflow)) STRICT` and `github_deploys(full_name PRIMARY KEY, environment, state, created_at) STRICT`.
- Without GitHub connected, a schedule's item reads "Connect GitHub to see its runs." with **Connect GitHub** (`ManyStates`), and no failure rule is applied to it.

### The digest and notifications

- **The time.** `state.json`'s `reminder_time: "HH:MM"` (default `"09:00"`) is in the computer's local time zone, read with `chrono::Local`. `reminders.set { time?, browser?: bool, system?: bool }` sets it and the two notification switches (both on by default).
- **The digest** is computed by serve at the time, kept in memory as `Digest { at, items }`, and answered by `reminders.get` (hub): `{ at, count, groups: [{ group, items }], headline }`.
  - `headline` is the first two items' short words: "A price check failed, and Tidepool's last deploy failed."
  - `reminders.get` with no digest yet computes one at once.
  - Each team's part is the `teams` items of that team, plus the items whose project is the team's root or member. It is answered on the team's socket as `reminders.mine`, and shown on its Today.
- **One notification per digest.**
  - At the time, the hub checks whether any page is connected to it (a live `/hub/rpc` or `/t/*/rpc` socket). If one is, it pushes `reminders.due` to the hub's sockets, and the page shows a browser notice (`Notification`, permission asked on All teams with **Allow notices** / **Not now**).
  - If none is, serve sends one through the operating system, through `crates/runtime/src/notify.rs`. That module runs one program with fixed arguments, never a shell, and the title and body are passed as arguments:

    | System | Program |
    |---|---|
    | Linux | `notify-send --app-name=Catervas <title> <body>` |
    | macOS | `osascript -e 'display notification (item 1 of argv) with title (item 2 of argv)' <body> <title>` |
    | Windows | `powershell.exe -NoProfile -Command <a fixed script reading $args>` with a toast from `Windows.UI.Notifications` |
    | WSL (Linux with `WSL_DISTRO_NAME` set) | the Windows program, through `/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe` |

  - The title is "Catervas · <time>" and the body "<count> things need you" and the headline.
  - A program that is missing or fails is recorded once in the digest as `notified: none`, and the digest stays on All teams.
  - With `browser: false`, the browser path is skipped; with `system: false`, the system path is skipped.
  - Clicking a browser notice focuses the tab on `/needs-you`. The operating system's has no click action on Linux, a default open on macOS, and opens `http://127.0.0.1:<port>/needs-you` on Windows through the toast's `launch` argument. The `ManyReminders` frame 2's **Snooze a day** button is dropped: a toast's buttons need a registered app.
- **Actions** (hub):
  - `reminders.snooze { key, for: "day"|"week" }`;
  - `reminders.dismiss { key }`;
  - `reminders.give { key }`. This files a request in the item's team with the Product Manager. Its text is "Finish “<title>” in <project>. The work was left <n> days ago with <reason>, on the branch <branch>.", through that team's daemon's request filing (`request.file`). An item with no team is `no_team_for_this` ("make a team for <project> first").

  Copy resume command and Open on GitHub or claude.ai are the page's. The resume command is `cd <folder> && claude --resume <session id>`; the GitHub address is the repository's `actions` or `deployments` page; claude.ai's is `https://claude.ai/chat/<uuid>` for a web session known from the export.
- Migration 11 also adds `reminder_marks(key TEXT PRIMARY KEY, kind TEXT CHECK (kind IN ('snooze','dismiss')), until TEXT, fingerprint TEXT, at TEXT) STRICT`.

### The screens (step 05b's pages)

- `/needs-you`, "Needs you today", from `ManyReminders` frame 1:
  - "<n> things · from your <time> reminder";
  - the notice offer **Allow notices** / **Not now** while permission is `default`;
  - the groups with their headings ("Sessions you may have forgotten", "Ending soon" with its ⓘ, "Schedules and deploys", "Your teams", "Services", "Today's runs (n)" collapsed);
  - each item with its actions;
  - Give to the team opens the dialog of frame 3, "Give to <team>", with the prefilled request and **Send to the team**;
  - Snooze opens "For a day" and "For a week".
- On All teams, the band from `ManyAllTeams` frame 2: "**<n> things need you today.** <headline>" with **See all** to `/needs-you`.
- On each team's Today, "Needs you today" lists that team's part, with the same actions.
- This computer gains "Reminders" from `ManyFixMistake` frame 2:
  - "Daily reminder at <time>" with a time field;
  - "In the browser · When a Catervas tab is open." with its switch;
  - "From the computer · When no Catervas tab is open." with its switch.

## File map

```
crates/core/src/reminders.rs                     creates: the rules, keys, fingerprints
crates/core/src/claude_history.rs                modifies: ToolUse.open_todos for TodoWrite
crates/store/src/migrations/machine/0011_reminders.sql  creates
crates/store/src/reminders.rs                    creates: ReminderInput from the index and git; marks
crates/runtime/src/github.rs                     modifies: workflows, scheduled runs, deployments
crates/runtime/src/notify.rs                     creates: the operating system's notification
crates/runtime/src/daemon/reminders.rs           creates: reminders.* on the hub, reminders.mine on a team, the timer, reminders.due
crates/cli/src/finding.rs, serve.rs, state.rs    modifies: the input's sources; reminder_time and switches
docs/schemas/rpc.schema.json, event pushes, crates/protocol, packages/protocol-client   modifies
apps/web/src/pages/NeedsYou.tsx, ReminderItem.tsx, GiveToTeam.tsx   creates
apps/web/src/pages/AllTeams.tsx, Today.tsx, ThisComputer.tsx   modifies
docs/SPEC.md                                     modifies: revision 0.151
```

## Tasks

### Task 1: The rules

Tests (`reminders.rs`, a fixed `now`):
- `reminds_of_what_needs_the_owner`: asserts each rule of the table on a fixture input meant to fire it. Its neighbours must not fire: a session 15 days old, 0 days old, or clean; a cron ending in 2 days; a run that succeeded; a schedule within its interval.
- `snoozes_and_dismisses_until_it_changes`: asserts these.
  - A snoozed item is hidden until its time and shows after it.
  - A dismissed item stays hidden while its fingerprint holds, and shows when a field changes (a new failure time).

- [x] `feat(core): decide what needs the owner each day`

### Task 2: The input, from the index and git

Tests (`crates/store/tests/reminders.rs`, ignored, git):
- `gathers_the_reminders_input`: asserts these.
  - A fixture session in a test repository with one uncommitted file and one unpushed commit (a bare remote fixture) is `uncommitted` and `unpushed: 1`.
  - Its last `TodoWrite` with 3 items, 1 completed, gives `open_todos: 2`, and no todo text is in `machine.db`.
  - A `mcp-needs-auth-cache.json` naming `zoho-books` gives that service.
  - Marks round-trip, and the migrations list ends at 11.

- [x] `feat(store): gather what the reminder needs`

### Task 3: Runs and deployments through GitHub

Tests (`github.rs` against the fake server):
- `reads_scheduled_runs_and_deployments`: asserts these.
  - A workflow `disabled_inactivity` is read as disabled.
  - The last scheduled run's `failure` and time are read.
  - The latest deployment's last status `failure` is read.
  - Each response is parsed for those fields alone.

- [x] `feat(runtime): read scheduled runs and deployments from github`

### Task 4: Notifications from the computer

Tests (`notify.rs`, through a seam that records the program and its arguments):
- `notifies_through_the_operating_system`: asserts these.
  - On Linux the program is `notify-send` with the title and body as separate arguments.
  - With `WSL_DISTRO_NAME` set, it is the Windows `powershell.exe` path, with the body as an argument.
  - A title holding `'; rm -rf /` reaches the program as one argument, unchanged.
  - A missing program answers `NotSent`.

- [x] `feat(runtime): send a notification through the operating system`

### Task 5: The digest, its actions, and the one notification

Tests (`crates/runtime/src/daemon/reminders.rs`, with tokio's paused time and fakes):
- `sends_one_notification_per_digest`: asserts these.
  - At 09:00 with a page's socket open, `reminders.due` is pushed and no system notification is sent.
  - The next day with no socket open, one system notification is sent and nothing pushed.
  - With `system: false`, none is sent.
  - `reminders.get` answers the digest's groups and headline.
- `gives_a_forgotten_session_to_its_team`: asserts these.
  - `reminders.give` on a forgotten session in a driven team's project files a request whose text names the session's title, project, days and branch, on that team's log.
  - On a project with no team it is `no_team_for_this`.
  - `reminders.snooze` hides the item from the next `reminders.get`.

- [ ] `feat(runtime): the daily digest, its actions and one notification`

### Task 6: The screens

Tests:
- `shows_what_needs_you_and_acts_on_it` (`pages/NeedsYou.test.tsx`): asserts these.
  - The groups and their items show with the mockup's words.
  - **Copy resume command** copies `cd ~/code/recipe-cards && claude --resume <id>`.
  - **Give to the team** opens "Give to Tidepool" with the prefilled request, and **Send to the team** sends `reminders.give`.
  - **Snooze** offers a day and a week.
  - **Dismiss** sends `reminders.dismiss`.
  - **Allow notices** calls `Notification.requestPermission`.
- `shows_the_band_and_each_team_s_part` (`pages/AllTeams.test.tsx`, `pages/Today.test.tsx`): asserts these.
  - All teams shows "9 things need you today." with the headline and **See all**.
  - A team's Today shows its own part.
- `sets_the_reminder_s_time_and_notices` (`pages/ThisComputer.test.tsx`): asserts that changing the time and the two switches sends `reminders.set`.
- `notifies_in_the_browser_when_the_digest_is_due` (`app/connection.test.tsx`): asserts that a `reminders.due` push with permission granted makes one `Notification`, and a second push for the same digest makes none.

- [ ] `feat(web): needs you today`

### Task 7: Spec

`docs/SPEC.md` revision 0.151 (2026-10-22), "records phase 10 step 07, the daily reminder". It covers:
- F12 and 4.4: the digest, its actions and the screens;
- 6.7: `mcp-needs-auth-cache.json` read for names alone, and GitHub's runs and deployments;
- 8.4: migration 11 and `state.json`'s reminder fields;
- 8.6: the notification programs and their arguments; git read-only in the owner's folders.

Then the project plan's row 07: "planned and executed 2026-10-22".

- [ ] `docs(spec): the daily reminder, revision 0.151`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas-store --test reminders -- --include-ignored
# expected: xtask check: ok; the reminders tests pass
```
