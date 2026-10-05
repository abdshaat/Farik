# Phase 7, step 11f: Production on the pages and the command line

Status: draft. Its readiness review runs once step 11e has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.2, 6.9, 7 (F9, F12); F9
Depends on: step 11 (the approved mockups `DevOpsCard`, `Production`, `TodayIncident`, `Incident`, `DeployPanel`, `PhoneIncident`, `PhoneProduction`); steps 11b to 11e (`Team::production`, `production.status`, `incidents.list`, `incident.get`, the four incident commands, the deploy events); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 11 (see step 11's header). Every screen here is built from step 11's approved boards; no new screen is drawn.

## Goal

A non-technical user sees and runs production from the browser: Settings has "Your production", where the user picks the platform the DevOps Engineer is connected to, names the service, gives the health address and the settling period, and reads that Farik watches only while the computer is on; the DevOps Engineer's card says what the watch last saw and when, so a stopped watch is visible; Today puts an open incident first, with its steps as they happen and "Stop", "Restart", "Roll back" and "Mark as fixed"; each incident has a page; and a deploy task shows its deploy where other tasks show their changes. `farik production` and `farik incident` do the same at the command line. Out of scope: the platforms' own setup copy (step 12's kits).

## Decisions

- **Built as the boards.** Each component follows its approved board, desktop and phone; a difference found in execution goes back to the founder before code, as phase 6's pages did.
- **"Your production"** is `Production.tsx` in `apps/web/src/pages/`, beside `HowToOpen` (`TeamRules.tsx:316`), rendered by `Settings.tsx` after it, at `#production`. It reads `team.get` and saves the team file's `production` through `team.save`, as the preview does. The platform field lists the services connected to the team's DevOps Engineers that are not retired (`team.get`'s `connectors`, state `connected`), each by its kit title; with none, the section says "Connect your platform on <name>'s page first" with a link to that agent's page. Fields: "Service name on <platform>" (under it: "Your platform's setup text, on the DevOps Engineer's page, says what to write here."), "Health address" (`https://…`), "Watch a new deploy for" (1 to 60 minutes, default 5), and "Count it as broken when more than __% of requests fail" (optional, "only where your platform reports it"). The daemon's refusals show in words: `production_connector_unknown` "Choose a platform the DevOps Engineer is connected to", `health_url_invalid` "Use the full https address of a page that answers when your service is up". The line "Farik watches only while this computer is on and Farik is running. Keep your platform's own alerts on too." is always shown. Rejected: a page of its own; the preview's section is the pattern the founder approved.
- **The card line** (`Team.tsx`'s cards): a DevOps Engineer's card shows one line from `production.status`: "Production healthy. Checked at <HH:MM>."; "Deploying <short sha>. Watching: <m> of <n> minutes healthy."; "Incident open since <HH:MM>."; "Not watching. <why>" (from `why`), and, when `last_check_at` is more than three minutes old while Farik serves, "Not watching. Farik last checked at <HH:MM>." Times are local, as the boards show them.
- **Today** (`Today.tsx`): an "Incidents" section before "Waiting on you", for each open incident from `incidents.list`: its title ("Production is down since <HH:MM>" or "A deploy failed at <HH:MM>"), its steps in order, the sentence for `waiting_on_you` when present, and the four buttons, each sending its command (`incident_stop`, `incident_restart`, `incident_roll_back`, `incident_resolve`); "Stop" and "Roll back" ask "Are you sure?" with the board's words before sending, and "Mark as fixed" takes an optional note. A stopped incident says "You stopped this. Farik does nothing more on its own." The waiting count in the title adds the open incidents. A refusal shows in words through `saidAll`.
- **The incident page** `Incident.tsx`, route `/incidents/:incident` in `App.tsx`: the timeline from `incident.get` (opened, each restart and rollback with who did it, each note, the fix task linked to `/tasks/<id>`, its deploy, the resolution); a note is shown as plain text, never as markup, since it is an agent's words about untrusted logs.
- **The deploy panel** (`TaskDetail.tsx` and `Gate.tsx`): for a task whose contract's `change` is `deploy`, the Changes tab is replaced by "Deploy": "Deploys <short sha>, which holds <ids>", then the state from the new query `task.deploys { task_id }`: building, watching ("<m> of <n> minutes healthy"), live and healthy since, failed with the incident's link. `task.deploys` answers each `deployment.started` of the task with its outcome, newest first, read from the log by kind.
- **The mapping.** `packages/protocol-client`'s `mapping.ts` gains the new fields in camelCase, in one place, as the code standard asks; no page maps a key itself.
- **Strings.** Every string above is in `apps/web/src/strings/en.ts`. None says "token", "MCP" or "OAuth".
- **The command line.** `farik production` prints `production.status` (`--json` prints it as JSON alone). `farik incident list [--json]`, `farik incident stop <n>`, `farik incident restart <n>`, `farik incident roll-back <n>`, and `farik incident resolve <n> [--note <text>]`, each through `here_or_sent`, as `farik tool approve` is, so a running daemon handles it.

## File map

```
apps/web/src/pages/Production.tsx(+test), apps/web/src/pages/Settings.tsx       creates/modifies (Task 1)
apps/web/src/pages/Team.tsx, team.test.tsx                                      modifies (Task 2)
apps/web/src/pages/Today.tsx, Today.module.css, Today.test.tsx                  modifies (Task 3)
apps/web/src/pages/Incident.tsx(+test), apps/web/src/app/App.tsx                creates/modifies (Task 3)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/board.rs                modifies: task.deploys (Task 4)
apps/web/src/pages/TaskDetail.tsx, Gate.tsx, DeployPanel.tsx(+test)             modifies/creates (Task 4)
packages/protocol-client/src/mapping.ts(+test)                                  modifies (Tasks 1 to 4)
apps/web/src/strings/en.ts                                                      modifies (Tasks 1 to 4)
crates/cli/src/production.rs, incident.rs, lib.rs                               creates/modifies (Task 5)
docs/SPEC.md, docs/plans/project-plan.md                                        modifies (Task 6)
```

## Interfaces

Consumes: `team.get`, `team.save`, `production.status` (11c), `incidents.list`, `incident.get`, `Command::{IncidentStop, IncidentRestart, IncidentRollBack, IncidentResolve}` (11d), the deploy events (11b to 11e); `useQuery`, `useConnection`, `saidAll`, `HowToOpen`, `here_or_sent`, `CliIo` (on main).

Produces:

```ts
export function Production(): JSX.Element;                  // apps/web/src/pages/Production.tsx
export function Incident(): JSX.Element;                    // apps/web/src/pages/Incident.tsx
export function DeployPanel(props: { taskId: string }): JSX.Element;
```

```rust
// query task.deploys { task_id } -> { deploys: [{ started, commit, holds, outcome, live_since?, healthy_minutes?, incident? }] }
pub fn production(io: &mut CliIo<'_>, as_json: bool) -> i32;                 // farik_cli::production
pub fn incident(io: &mut CliIo<'_>, command: IncidentCommands) -> i32;        // farik_cli::incident
```

## Tasks

### Task 1: "Your production"

- `lists_only_the_devops_engineers_connected_platforms`: with a DevOps Engineer connected to `vercel` and a Product Manager connected to `notion`, the field offers Vercel alone; with none, the link to the agent's page. RED.
- `saves_the_settings_through_the_team_file`: `team.save` is sent with `production` holding the five fields. RED.
- `says_a_refusal_in_words`: a `health_url_invalid` reply shows its sentence at the field. RED.
- `always_says_when_farik_watches`. RED.

- [ ] `feat(web): set production in Settings`

### Task 2: The card

- `the_card_says_what_the_watch_saw`: each of the four states from a `production.status` fixture; an old `last_check_at` reads "Not watching. Farik last checked at …". RED.
- `only_a_devops_engineers_card_has_the_line`. RED.

- [ ] `feat(web): show production on the DevOps Engineer's card`

### Task 3: Incidents on Today and their page

- `an_open_incident_comes_first_with_its_steps`. RED.
- `each_button_sends_its_command`: "Restart" sends `incident_restart { incident }`; "Stop" and "Roll back" only after "Are you sure?"; "Mark as fixed" with its note. RED.
- `a_stopped_incident_says_so`. RED.
- `the_incident_page_shows_its_timeline_and_its_notes_as_text`: a note holding `<img src=x onerror=alert(1)>` renders as text. RED.
- At phone width, as `PhoneIncident`. Guard in the existing phone suite.

- [ ] `feat(web): put incidents first on Today, with their own page`

### Task 4: The deploy panel

- `task_deploys_answers_each_deploy_and_its_outcome` (runtime): building, succeeded with minutes, failed with its incident. RED.
- `a_deploy_task_shows_its_deploy_not_a_diff` (TaskDetail and Gate). RED.

- [ ] `feat(web): show a deploy task's deploy`

### Task 5: The command line

- `production_prints_the_status`; `production_json_is_pure`. RED each.
- `incident_list_prints_open_incidents`; `incident_restart_sends_the_command_to_the_daemon`; `incident_resolve_sends_the_note`. RED each.

- [ ] `feat(cli): show production and handle incidents`

### Task 6: Spec and plan

`docs/SPEC.md` 4.2 (where production shows in the working loop), 6.9 (the pages and commands as built), 7 (F9's and F12's lines name them); the revision line. Project plan row 11f; row 11 says step 11's row is done.

- [ ] `docs(spec): record production on the pages`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

No platform is driven before step 12, so the founder's check of these screens against a real service is step 12's Verification (Vercel): set production in Settings, watch the card change, break a deploy, and use the incident's row and page, at phone width too. Until then the component tests above are the evidence.

## Execution notes

None yet.
