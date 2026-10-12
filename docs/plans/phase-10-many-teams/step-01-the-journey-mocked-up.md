# Phase 10, step 01: The journey, mocked up

Status: draft
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 4.4, 10; no behaviour changes in this step
Depends on: none. The design is `docs/design/many-teams.md` (approved 2026-10-12, ADR 0059); the house style is phase 9's page "Multi-repo projects" and `docs/brand/brand.md`.
Mockups: drawn 2026-10-12 on the canvas page "Many teams" (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, version 23), sources in `docs/design/mockups/Many*.dc.html`; not yet approved.
Readiness confirmed by: not yet (one round, against `docs/standards/workflow.md` stage 2)

This step writes no code and no test. Its tasks are boards; each is "verified" by the founder clicking it through on the canvas and saying yes, as ADR 0032 has mockups approved before any screen is built.

## Goal

Every screen of the design's journeys A to F, the services found and the web-only picker, the empty and error states of section 0, the daily reminder with its notification, and the analytics, drawn as linked boards on the canvas page "Many teams", in the house style and the founder's words rules (short bullets, details behind an info button, plain words, muted colours, one colour per job). When the founder has clicked them through and approved them, steps 05b, 06, 06b, 07 and 08 build from them and their web tests check these words. Out of scope: the phone layout (each page follows the existing phone pattern when built), the dark theme (the tokens cover it), and any change to the design itself; where a board needed a choice the design did not make, it is listed under Decisions for the founder to confirm or change.

## Decisions

Screen-level choices made while drawing. Each is open to the founder's change at approval.

- **Where the finding flow lives:** inside the first run, as its own three-step strip ("What you work on · Work together? · Your teams"), replacing "Your project" when the owner chose Look for my projects; rejected: a separate page after setup, because principle 1 (show before asking) wants the list to be the first thing after the scan.
- **What the created teams skip:** a team made here takes its ready-made team's permission answers and no daily limit (limits stay optional, ADR 0015), so no per-team permission or spending screen is asked; each is changed later in that team's Settings; rejected: running setup's team, permission and spending screens once per team, which multiplies the questions the journey is judged on.
- **Team · Watch · Ignore:** a three-way segmented control on each row, the choice filled in ink; rejected: a dropdown, which hides the default the owner is asked to check.
- **Defaults on the list:** Team for the owner's projects worked on in the last 30 days, Watch for older, Ignore for explored; a job with no project offers only Watch · Ignore.
- **The bookkeeping project defaults to Team** with the Financial analysis team and a new folder `~/Catervas/bookkeeping`; rejected: Watch, from section 5's Proposed rule for a folder with no git, since connector work has no folder to `git init` and section 2d proposes its team. The founder's ruling is asked for (see the report).
- **"2 folders":** a badge on the project row with the reason and These are different behind its info button; rejected: a separate question screen, since same-project is Catervas's call on proof alone.
- **One pair per screen, with "Question 1 of 2"** above it, and Not sure? Keep them separate under the answers; rejected: a list of pairs with checkboxes, which invites ticking without reading the proof.
- **A second proof into a group already joined** (infra deploys web as well as api) is shown as "Also", not asked again; rejected: asking every proven pair, which repeats a question whose answer is already given.
- **Maybe related:** after the last proof, collapsed, each pair set to Separate, with One team selectable once opened; rejected: hiding the choice entirely. This is the owner's explicit choice on a hint, which the design's "groups form only through confirmed pairs" does not name (see the report).
- **The group's second confirmation is the Create button on Your teams**, where each group's sentence ("web uses api's code · infra deploys both") is shown again with its files behind the info button; rejected: an extra "One team from three projects?" screen, one more question for the same answer.
- **Names from Claude:** a quiet card above the teams, the cost on the button ("about $0.02"), the spent cost said once after; each name has Keep the folder name; rejected: a confirm dialog before the run, since the price is on the button.
- **Document it first is not offered to the bookkeeping team:** its folder is new and empty; it has Start only.
- **Team switcher:** a button at the top of the rail with the team's name, opening a menu of All teams, each team with its state, and Find more projects; All teams has its own rail (All teams, Analytics, This computer); rejected: a top bar, which the app does not have.
- **All teams after creation:** the new teams' cards with Start, Document it first and Undo, then Watched with Not mine and Make a team; later, cards show agents working, spend today and this month, the next job, and the reminder band leads. Journey E's finds and new proofs are rows below the cards, never a pop-up.
- **Journey B:** an offer card on All teams, "Find your other projects", with the same consent behind the info button; the list then shows "Already a team · Tidepool" on that project.
- **Web-only:** "Where do you use Claude?" as three radio cards; then three optional screens, one each (export steps with a drop zone, Connect GitHub, the services tiles), each skippable; the export card waits on the list and on All teams. A repository only on GitHub offers Watch · Ignore and Copy to this computer, a dialog that asks for the folder.
- **Services picker:** tiles from the catalog grouped by kind, labelled with the kind, plus Something else; selected tiles in ink.
- **This doesn't belong here:** a dialog with Watch it (chosen) or Make it its own team (created paused), and "We won't suggest them together again".
- **This computer** (All teams' settings): Finding projects (daily look on or off, Scan now, Ignored with Bring back, Keep a year of history with the exact line and backup behind the info button), Reminders (time, in the browser, from the computer), All teams together (Agents working at once, off by default; spending cap for this computer), Forget everything with a confirm that says what stays.
- **Reminder page:** "Needs you today" on All teams, grouped as section 6 lists, with the actions on each row as buttons; a failed run or deploy titled in Clay Text (waiting on you); the browser-notice permission asked as a notice on that page; Snooze opens a day or a week.
- **The computer's notification:** one card, "9 things need you", two named and "and N more", with Open and Snooze a day.
- **Analytics:** on All teams, a horizontal bar per team split into the owner's sessions (ink) and the team's agents (warm grey), every amount written beside its bar and a table view, labelled "What this would cost on the API" with the reason behind the info button; then workflow numbers and blockers. Per team, as an Activity tab on Costs; rejected: a new rail item per team.
- **Empty and error states:** each drawn where it appears, one line of what happened and one next step; none blocks the rest of the journey.
- **Names in the boards:** invented projects (tidepool-web, tidepool-api, tidepool-infra, price-watch, recipe-cards, Bookkeeping through Zoho Books, awesome-llm-tools, old-landing, nightly-backup), never the founder's own repositories.

## File map

```
docs/design/mockups/ManyFindOffer.dc.html     creates: journey A step 1, the offer and what is read
docs/design/mockups/ManyScanning.dc.html      creates: the scan's counts, setup meanwhile, done
docs/design/mockups/ManyFound.dc.html         creates: Here's what you work on, tucked-away sections, journey B's row
docs/design/mockups/ManyLinkPair.dc.html      creates: Do these work together? (two pairs), Maybe related
docs/design/mockups/ManyYourTeams.dc.html     creates: Your teams, names from Claude, Create 3 teams
docs/design/mockups/ManyAllTeams.dc.html      creates: All teams created paused, a day later with the switcher, journey B's offer
docs/design/mockups/ManyWebOnly.dc.html       creates: journey C, from Where do you use Claude? to Copy to this computer
docs/design/mockups/ManyFixMistake.dc.html    creates: journey F, This computer, Forget everything
docs/design/mockups/ManyReminders.dc.html     creates: Needs you today, the computer's notification, Give to the team
docs/design/mockups/ManyAnalytics.dc.html     creates: analytics on All teams and a team's Activity tab
docs/design/mockups/ManyStates.dc.html        creates: the six empty and error states of section 0
docs/design/mockups/canvas.json               modifies: page "Many teams" (id `many`), its eleven boards, their order, a title note
```

The canvas artifact holds the same eleven files and its own `canvas.json` with the page added (version 23).

## Interfaces

None. This step consumes no code and produces none.

## Tasks

One task per board group. Each is verified by the founder's click-through on the canvas page "Many teams" (Play, following the links between boards), and its checkbox is ticked with the date and the founder's words when approved. A change the founder asks for is redrawn on the same board and published to the same canvas.

### Task 1: Finding the projects (journey A, steps 1 to 3; journey B's row)

Files: `ManyFindOffer.dc.html`, `ManyScanning.dc.html`, `ManyFound.dc.html`
Verified when the founder has: opened the consent's info button and read what is read and kept; followed Look for my projects to the counts, Continue setup meanwhile, and Done; checked the defaults on each row, the bookkeeping row from Zoho Books, the "2 folders" reason, the four tucked-away sections, and Already a team.

- [ ] Approved by the founder: <date>, "<words>"

### Task 2: Linking with proof and creating teams (journey A, steps 4 to 6)

Files: `ManyLinkPair.dc.html`, `ManyYourTeams.dc.html`, `ManyAllTeams.dc.html`
Verified when the founder has: read each proof sentence and its files and lines; answered both pairs; opened Maybe related; checked each team card, Suggest names with Claude and its result, Create 3 teams; seen the paused teams with Start, Document it first and Undo, the watched projects with Not mine, the day-later cards, the switcher, journey E's two rows, and journey B's offer.

- [ ] Approved by the founder: <date>, "<words>"

### Task 3: Claude on the web (journey C)

Files: `ManyWebOnly.dc.html`
Verified when the founder has: walked Where do you use Claude?, the export steps and drop, Connect GitHub, the services tiles, the web-only list (Zoho Books bookkeeping, a repository only on GitHub, a claude.ai Project, the waiting export card), and Copy to this computer.

- [ ] Approved by the founder: <date>, "<words>"

### Task 4: Fixing a mistake and this computer (journey F)

Files: `ManyFixMistake.dc.html`
Verified when the founder has: used This doesn't belong here and its two outcomes; read This computer's sections; opened Forget everything's confirm.

- [ ] Approved by the founder: <date>, "<words>"

### Task 5: The daily reminder and its notification

Files: `ManyReminders.dc.html`
Verified when the founder has: read each group and each row's actions (including Open on claude.ai for a web session), the browser-notice ask, the computer's notification, Give to the team and Snooze.

- [ ] Approved by the founder: <date>, "<words>"

### Task 6: Analytics and the empty and error states

Files: `ManyAnalytics.dc.html`, `ManyStates.dc.html`
Verified when the founder has: read usage per team (own sessions against agents, the API label), workflow, blockers, a team's Activity tab; and each of the six states with its next step.

- [ ] Approved by the founder: <date>, "<words>"

## Verification

There is no check command for a mockup-only step; `cargo xtask check` is not run, since nothing it checks changes. The step is done when:

- every task's checkbox carries the founder's approval, with its date and words;
- the Mockups line above says "approved by the founder on <date> ("<words>")", and names any board redrawn on the founder's word;
- the canvas's live version and `docs/design/mockups/` hold the same eleven `Many*.dc.html` files and the "Many teams" page, which `git diff --stat main -- docs/design/mockups` and the canvas's file list show;
- every decision above that the founder changed is carried into `docs/design/many-teams.md` in the same pull request, and steps 05b to 08 plan against the approved boards.
