# Phase 10, step 06c: Finding projects in the web app

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 4.1, 4.4; ADR 0059
Depends on: step 05b (the hub's socket, `useHub`, All teams, This computer) and step 06 (the `finding.*` methods, `POST /hub/export`), both committed
Mockups: `ManyFindOffer`, `ManyScanning`, `ManyFound` (frames 1 to 3), `ManyWebOnly` (frames 1, 2, 4 and 5; frame 3, GitHub, is step 06d's), `ManyStates`, and `ManyFixMistake` frames 2 and 3 (This computer's finding section, and Forget everything), all approved with step 01. The words below are the mockups'.
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

The owner finds their projects in the browser, following journeys A, C (without GitHub), D and E (section 0 of the design):

1. The first run's computer check offers "Find your projects?" when Claude Code history exists.
2. The scan runs with live counts while setup goes on.
3. "Here's what you work on" lists each project with Team, Watch or Ignore already chosen.
4. An owner with no history picks where they use Claude, drops claude.ai's export, and taps the services they use.
5. This computer pauses finding, scans now, brings ignored projects back, keeps a year of history, and forgets everything.
6. Every empty and error state of `ManyStates` shows where it happens.

"Continue" on "Here's what you work on" leads to step 06b's "Do these work together?". Until 06b lands, it goes to All teams.

## Decisions

- **Where each screen lives.** These pages use the hub's socket.

  | Route | Shows |
  |---|---|
  | `/setup/computer` | today's check; below it, when `finding.offer` is `claude_code`, the offer card |
  | `/setup/where` | "Where do you use Claude?", shown when the offer is `none` |
  | `/find/scanning` | the scan |
  | `/find/found` | "Here's what you work on" |
  | `/find/export` | Bring your claude.ai history |
  | `/find/services` | Which of these do you use with Claude? |

  The `/find/*` pages share the stepper "1 What you work on · 2 Work together? · 3 Your teams" (`ManyFound`'s header) and have no rail, as the setup pages.
- **The offer card**, from `ManyFindOffer`:
  - Title "We found your Claude Code history", then "Find your projects?".
  - The bullets "We look through your Claude Code history.", "We list your projects and scheduled jobs.", "Nothing leaves this computer.".
  - An ⓘ button opens the read and kept list, word for word from the mockup.
  - **Look for my projects** calls `finding.start` and goes to `/find/scanning`. **Skip** keeps today's setup (journey D).
- **Scanning**, from `ManyScanning`:
  - Title "Looking through your history", with "About a minute left." when `finding.status` gives a remaining time, else nothing.
  - Three counts: projects, scheduled jobs, sessions. Then "> Reading sessions from <month>".
  - "Continue setup meanwhile" goes back to setup. **Stop** calls `finding.stop`, with the bullets "What's found so far is kept." and "Pick up later from All teams.".
  - While a scan runs, every setup page shows the strip "Still looking · N projects so far" with "See what's found", a link to `/find/found`.
  - On `Done` it shows "Done" and "Here's what we found." with the counts and **See what you work on**.
  - The page polls `finding.status` every second while `Running`.
- **Here's what you work on**, from `ManyFound`:
  - The heading and the three Team, Watch and Ignore bullets, then "We picked for you. Change any.".
  - One row per `main` row: the name, "2 folders" with an ⓘ when `folders` > 1, the jobs badge "N job(s)", "last worked · service, service", and a three-way choice (Team, Watch, Ignore) showing `choice ?? suggested`. A change calls `finding.choose`.
    - The ⓘ on "2 folders" says "<folder> and <folder> are the same GitHub repository." with **These are different**, which calls `finding.separate`.
    - A connector project's ⓘ gives the mockup's three bullets ("Found from Claude's connectors, not a folder."; "N sessions used <service>, most in your home folder."; "A team for it gets a new folder: ~/Catervas/<slug>.").
    - A row with `already_team` shows "Already a team · <team name>" instead of the choice.
  - Collapsed sections: "Explored (n)", "No longer on this computer (n)" (each row with **Locate…**, a folder picker calling `finding.locate`), "Your home folder" (its line), and "Jobs on this computer (n)" (Watch and Ignore only).
  - **Back** and **Continue**.
- **Where do you use Claude?**, from `ManyWebOnly` frame 1. It has three options:
  - "Claude Code on this computer · Look again in another place." starts a scan.
  - "claude.ai or the Claude app · Bring your history, connect GitHub, pick your services." goes to `/find/export`.
  - "I'm new to Claude · Set up one team." keeps today's setup.
- **Bring your claude.ai history**, from frame 2:
  - "1 of 3 · optional", the three bullets, and the six numbered steps.
  - A drop zone, "Drop your export here · A .zip file from claude.ai", with **Choose file…**. The file is posted as the body of `POST /hub/export`, with a progress line while it uploads.
  - Its summary shows as "N conversations and M Projects brought in".
  - "I'll do it later" calls nothing, and the All teams card (below) waits. **Next** goes to step 2 (GitHub, step 06d; until 06d it goes to step 3).
- **Which of these do you use with Claude?**, from frame 4:
  - "3 of 3 · optional", and "Tap any. Bookkeeping becomes a project of its own.".
  - Tiles from the core catalog's services: name and kind. The catalog is exposed by `finding.catalog`, a query this step adds to the hub: `{ services: [{ id, name, kind }] }` from `catervas_core::claude_services` (step 03b's catalog).
  - "Something else · Type its name". **Continue** calls `finding.declare_services` and goes to `/find/found`.
- **The export card** on All teams and on `/find/found`, from frame 5: "Waiting for your claude.ai export. Drop it here when the email comes." with **Choose file…** and **Dismiss** (`finding.export_dismissed`). It shows while the owner chose "I'll do it later" (kept in `localStorage`, `catervas.exportPending`) and no export was imported.
- **States**, from `ManyStates`, each where the mockup puts it:
  - `unreadable` > 0 on the scan's Done reads "Couldn't read N sessions", with the three bullets and "Everything else was found.". It has no update button; "Check for an update" is dropped until a release exists.
  - An export refusal reads "That isn't a claude.ai export" (`not_an_export`), with the expired-link help.
  - A gone folder offers **Locate…** and **Not mine** (Ignore).
  - `Stopped` on All teams reads "We stopped looking halfway" with **Keep looking** (`finding.start`) and **See what's found**.
  - `Done` with no rows reads "We didn't find any projects", with **I use claude.ai**, **Set up one team** and **Add a project by hand**. The last opens a dialog (name; folder, through the folder browser, or repository address) that calls `finding.add_project`.
- **This computer**, adding to step 05b's page from `ManyFixMistake` frame 2, the "Finding projects" section:
  - "Look for new projects every day" with "Last looked today at 09:00." and an On/Off switch (`finding.pause`), plus **Scan now** (`finding.start`).
  - "Ignored" lists the names with **Bring back…**, which offers Team or Watch.
  - "Keep a year of history?" with the mockup's bullets and **Keep a year** (`finding.keep_longer { days: 365 }`). It is shown only while `finding.retention` reports the default.
  - The "Forget" section: **Forget everything…** opens the dialog of frame 3, "Forget everything?", with its three bullets and **Forget everything**, which calls `finding.forget { confirmed: true }`.
- **All teams** gains, above the cards, "Find your other projects" (`ManyAllTeams` frame 3) when the index is empty and finding is not paused. It has the three bullets, **Look for my projects** and **Not now** (kept in `localStorage`). After a scan, it gains a line "N new projects found · See them" when `finding.list` has rows found since the owner last opened `/find/found` (the time is kept in `localStorage`).

## File map

```
apps/web/src/pages/find/FindOffer.tsx, Scanning.tsx, Found.tsx, WhereClaude.tsx, ExportDrop.tsx, Services.tsx, Stepper.tsx   create
apps/web/src/pages/find/ExportCard.tsx, StillLooking.tsx, AddProjectDialog.tsx   create
apps/web/src/pages/setup/SetupComputer.tsx   modifies: the offer
apps/web/src/pages/AllTeams.tsx, ThisComputer.tsx   modifies: the find offer, new-found line, export card, the finding section
apps/web/src/app/App.tsx                     modifies: /setup/where, /find/*
apps/web/src/strings/en.ts, app/refusals.ts  modifies
crates/runtime/src/daemon/finding.rs          modifies: finding.catalog
docs/schemas/rpc.schema.json, packages/protocol-client   modifies: finding.catalog
apps/web/src/pages/find/*.test.tsx            tests
apps/web/e2e/finding.spec.ts                  creates
apps/web/e2e/fixtures/serve.ts                modifies: a fixture ~/.claude under the test's home
```

## Tasks

### Task 1: The offer and the scan

Tests (`pages/find/finding.test.tsx`):
- `offers_to_find_projects_and_shows_the_scan`: asserts these.
  - With `finding.offer` `claude_code`, the computer check shows "Find your projects?". **Look for my projects** sends `finding.start` and goes to `/find/scanning`.
  - Status counts 7, 4 and 312 show as "7 projects", "4 scheduled jobs" and "312 sessions".
  - **Stop** sends `finding.stop`. `Done` shows **See what you work on**.
  - With `none`, the check goes to `/setup/where`.

- [x] `feat(web): offer to find projects and show the scan`

### Task 2: Here's what you work on

Tests (`pages/find/Found.test.tsx`):
- `lists_what_you_work_on_with_a_choice_each`: asserts these.
  - The rows show their names, jobs and services, with the suggested choice selected. Choosing Watch on one sends `finding.choose { key, choice: "watch" }`.
  - "2 folders"' ⓘ shows **These are different**, which sends `finding.separate`.
  - An `already_team` row shows "Already a team · Tidepool".
  - "No longer on this computer (1)" opens to a row with **Locate…**.
  - With no rows it shows "We didn't find any projects", and **Add a project by hand** sends `finding.add_project`.

- [x] `feat(web): here's what you work on`

### Task 3: Claude on the web, without GitHub

Tests (`pages/find/WebOnly.test.tsx`):
- `brings_in_claude_ai_s_export_and_the_services`: asserts these.
  - "claude.ai or the Claude app" goes to `/find/export`.
  - Dropping a file posts it to `/hub/export` (the fake `fetch` answers a summary) and shows "12 conversations and 2 Projects brought in".
  - An answer of `not_an_export` shows "That isn't a claude.ai export".
  - On `/find/services`, tapping Zoho Books and Excel and pressing **Continue** sends `finding.declare_services { ids: ["zoho_books","microsoft_365"] }`.
  - "I'll do it later" leaves the export card on All teams.

- [x] `feat(web): bring claude.ai's history and the services you use`

### Task 4: This computer's finding section, and All teams' offer

Tests (`pages/ThisComputer.test.tsx`, `pages/AllTeams.test.tsx`):
- `pauses_finding_and_forgets_everything`: asserts these.
  - The switch sends `finding.pause { paused: true }`. **Scan now** sends `finding.start`.
  - **Bring back…** on an ignored name sends `finding.choose` with `watch`.
  - **Keep a year** sends `finding.keep_longer { days: 365 }`.
  - **Forget everything…** asks, and confirming sends `finding.forget { confirmed: true }`.
- `offers_to_find_other_projects_on_all_teams`: asserts these.
  - With an empty index, the offer shows, and **Look for my projects** starts a scan.
  - `Stopped` shows "We stopped looking halfway".

- [ ] `feat(web): finding in this computer and all teams`

### Task 5: The journey

The fixture: `startServe({ project: false, claudeHistory: true })` writes a small `~/.claude/projects/` history under the test's home. It holds two projects of fixture transcripts with invented content, and one of them is a git repository the fixture makes.

The test (`e2e/finding.spec.ts`), "a new owner finds their projects":
1. The computer check offers "Find your projects?". Pressing **Look for my projects** shows the scan, then **See what you work on**.
2. Both projects are listed with Team chosen. The test switches one to Watch and presses **Continue**.

- [ ] `test(web): a new owner finds their projects`

### Task 6: Spec

`docs/SPEC.md` revision 0.147 (2026-10-22), "records phase 10 step 06c, finding projects in the web app". It covers 4.1 (the offer in the computer check, `/setup/where`) and 4.4 (the `/find/*` pages, All teams' offer and new-found line, This computer's finding section, and `finding.catalog`). Then the project plan's row 06c: "planned and executed 2026-10-22".

- [ ] `docs(spec): finding projects in the web app, revision 0.147`

## Verification

```
python3 reaper.py cargo xtask check
pnpm -C apps/web exec playwright test e2e/finding.spec.ts
# expected: xtask check: ok; the journey passes
```
