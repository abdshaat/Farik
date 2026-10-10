# Standalone: setup asks less and can go back

Status: ready
Branch: `feat/setup-friction` (its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1 (first run, lines 101 and 103), 8.3 (the preview, line 867), F9, F15
Depends on: `main` at `15d67ec2` (pull request #35 merged)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-10 (one round; its findings I1 to I5 and M1 to M5 applied)

## Goal

The founder, 2026-10-10, five setup improvements: (a) "make the UI/UX developer setup automatic rather than asking the user for a port"; (b) on "What they may do", "reduce the number of text in that page and do not say things like (Nothing runs until you answer)"; (c) "Make that page reversible … I am currently in setup/scan and I cannot go back to change my AI account reliably"; (d) "on the spending page add an UI example of what a limit might look like"; (e) on `/setup/finish`, advanced settings open from a link under it, not a switch that takes you to `/setup/advanced`. Approved design (in chat, same day), with the founder's two answers: the preview is "Find + detect" and the permissions are "Pre-select recommended" (ADR 0054). When done: a JavaScript project's Designer card says how the app will start and asks nothing; the port is found when the app starts; the permission page starts answered and says less; every earlier step is a link back, Scan has Back, the project step offers "Keep <name>", and the team's answers survive going back; the daily limit shows an example; Finish opens advanced settings in place. Out of scope: Settings' own advanced button (done in #35), guessing ports from a framework, start commands for projects with no `package.json` script.

## Decisions

- The preview, the permissions' starting answers, and the scan's suggestion: ADR 0054.
- Port finding lives in `crates/runtime/src/preview/docker.rs`. `pub(crate) fn listening_ports(proc_net: &str) -> Vec<u16>` parses the text of `/proc/net/tcp` and `/proc/net/tcp6` concatenated: every data row whose 4th column (`st`) is `0A` (LISTEN), its port the hex after the `:` in the 2nd column; only ports ≥ 1024; ascending, without duplicates. Header rows and malformed rows are skipped. `DockerPreview::answers(&self, url)` becomes `answering_port(&self, port: Option<u16>, path: &str) -> Option<u16>`: each second until `ANSWER_LIMIT`, the candidates are `[port]` when set, else `listening_ports` of `docker exec <name> cat /proc/net/tcp /proc/net/tcp6` (a failed exec gives none, and the same "is not running"/"No such container" stderr ends the wait with `None`); each candidate is probed as today's probe with `http://localhost:<candidate><path>`; the first that answers is returned. `NeverAnswered` is unchanged when none does. Rejected: parsing in a shell `awk` script (untestable without Docker).
- `RunningPreview` (`crates/runtime/src/preview.rs`) gains the required `fn port(&self) -> u16`; every implementation (`DockerPreview`, `CheckedPreview`, `NamedPreview`, the test `BridgePreview`) gives it; `DockerPreview.port` holds the port found: `running` is `let mut`, built with `port: preview.port.unwrap_or(0)`, and after `answering_port` answers `Some(found)`, `running.port = found`. `origin()` is unchanged in meaning.
- `open_preview` (`orchestrator/session.rs:559`) records `PreviewStarted { port: running.port().into() }`. `give_browser` appends `preview_address(running.port(), &preview.path)` to `spec.initial_prompt` after the preview answers; `pub(super) fn preview_address(port: u16, path: &str) -> String` in `orchestrator/messages.rs` returns `"\n\nThe app's preview is at http://localhost:{port}{path}."`. `design_review_message(contract, diff)` loses its `page` parameter; its sentence reads "check it in the browser: the app's preview address is at the end of this message." and `verify.rs:387-389` stops building `page`.
- Core: `docs/schemas/team.schema.json` `$defs/preview.required` becomes `["start"]` and `port`'s description says "Left out, Catervas finds the port the app listens on when it starts (ADR 0054)."; `Preview.port` (`crates/core/src/team.rs:1110`) becomes `Option<u16>`, `Team::preview` maps the wire's optional port with the same `try_from … unwrap_or(u16::MAX)`. Test fakes that need a port use `preview.port.unwrap_or(5173)`.
- Scan: `crates/store/src/scan.rs` gains `pub struct AppStart { pub prepare: String, pub start: String }` and `ProjectScan.app_start: Option<AppStart>`. `Reading::app_start()` answers `Some` only for a `Commands::Scripts` toolchain whose `scripts` holds `dev`, `start` or `preview` (first in that order): `prepare` `"<name> install"`, `start` `"<name> run <script>"`, `<name>` the toolchain's (`pnpm`, `bun`, `yarn`, `npm`). Rejected: suggesting `npx vite` or the like for a project with no script (a guess).
- Daemon: `project.scan`'s answer gains `"app_start": {"prepare","start"}` or `null`. `team.propose` sets `team["preview"] = {"prepare","start","path":"/"}` from `scan_project(...).ok().and_then(|s| s.app_start)` only when the current team has no `preview`; a scan that fails proposes no preview and does not fail `team.propose`.
- Web preview: `Preview.port` becomes optional (`port?: number | string`) in `PreviewFields.tsx`; `previewOf` leaves `port` out when its field is empty. `SetupTeam` (the component, not its `row` function) holds `const [editing, setEditing] = useState(() => !draft.preview.start.trim())`. Each Designer card shows, while `!editing`, the heading `previewTitle`, `previewFound` ("{designer} opens your app with “{start}”."), and a quiet `Button` `previewChange` ("Change how it starts", `expanded={false}`, `controls="preview-<index>-fields"`) that calls `setEditing(true)`; otherwise today's card, its fields in `<div id="preview-<index>-fields">`. Typing never collapses the fields. Words: `previewPort` "Port (optional)"; `previewPortHint` "Leave it empty: Catervas finds it when your app starts.\nSet it only if your app answers on more than one.\nIt must be a number from 1024 to 65535."; `refusePreview` "Catervas cannot use these. Starting your app needs a command, each command is at most 500 characters, and the first page starts with /." Settings' "How to open your app" gets the optional port through the same `PreviewFields`.
- Permissions page (`SetupPermissions.tsx`): a `useEffect` that, when `useDefaults()` has answered and either answer is `undefined`, calls `answer({ commands: commands ?? d.runCommands, push: push ?? d.push })`. `mayAnswerBoth` and its note are deleted. The two notes move into each `Choice`'s `info` as `<Bullets>`; the `<p>` under each choice goes (`mayStillChecks`' warning stays). Words: `mayLead` "You can change these later in Settings."; `mayCommandsNote` "Needed to run the tests and try the app.\nCommands run in a sealed box, away from your other files."; `mayCommandsYesNote` "Recommended."; `mayCommandsNoNote` "The team cannot test anything."; `mayPushNoteOne` "Only {name}, the Developer, sends code, and only to its own branches.\nNothing reaches your main branch without you."; `mayPushNoteMany` "Only the Developers send code, and only to their own branches.\nNothing reaches your main branch without you."; `mayPushNoNote` "The safer choice.". "What each agent may do already" sits behind a quiet `Button` (`expanded`, `controls="may-already-section"`, the `<section>` given `id="may-already-section"`; its `<h2>` keeps `may-already`) reading `mayAlreadyShow` "Show what each agent may do" / `mayAlreadyHide` "Hide what each agent may do"; the section renders only while open. Settings' Allowed actions uses `PermissionChoices` too and gets the same info buttons.
- Stepper (`packages/ui/src/Stepper.tsx`): optional `onPick?: (index: number) => void`; given, each step before `current` renders its label in `<button type="button" className={styles.link}>` calling `onPick(i)`; `current` and later steps stay text. `.link` in `Stepper.module.css`: no background or border, `padding: 0`, `font: inherit`, `color: inherit`, `text-decoration: underline`, `cursor: pointer`. Rejected: forward links (they would skip each step's checks).
- Wizard (`Wizard.tsx`): `PATHS`, the eight `/setup/…` paths in `STEPS`' order, filtered alike when `carried`; `onPick={(i) => navigate(paths[i])}`.
- Scan: a Back button to `/setup/project`, left of "Something is wrong" in a `<span>`. Project: when `serve.projectRoot` is set, `serve.setupPending` is true and `leaving` is not set, the foot's primary button reads `projectKeep` "Keep {name}" (`base(projectRoot)`) and goes to `/setup/scan`; the existing Continue stays, not primary.
- Draft kept: in `TeamSetup.tsx`, `keepDraft(root: string, draft: Draft): void`, `keptDraft(root: string): Draft | undefined` and `forgetDraft(): void` on `sessionStorage` key `catervas.setupDraft`, holding `{ root, draft }`, each wrapped in try/catch as `noSandbox` is. `TeamSetup` waits for `serve.status` as for `team.propose`; its draft is `mine ?? keptDraft(root) ?? draftOf(data, "suggested")`, and `change` sets `mine` and calls `keepDraft`. `useStart` calls `forgetDraft()` after a start succeeds. Spending's Back writes `budgets(...)` into the draft (when not `wrong`) before navigating. Rejected: the daemon holding the draft (a new RPC for a tab's unsaved answers).
- Spending example: inside `useDailyLimit`'s `fields`, when `limit === "daily"` and not `wrong`, a `<figure className={styles.example}>` with `<figcaption>` `spendExample` "For example", a `<p id="limit-example-line">` from `costsTodayOf` with `{spent}` = `money(amount × 0.64)` and `{limit}` = `money(amount)`, `<meter min={0} max={amount} value={amount × 0.64} aria-labelledby="limit-example-line">`, and `<p>` `spendExampleStop` "At {limit}, everyone pauses and you get a message.". `money` is `dollars` from `pages/Gate.tsx`, imported as `money`. `.example` in `setup.module.css`: `padding: var(--catervas-space-4)`, `border: 1px solid var(--catervas-color-rule)`, `border-radius: var(--catervas-radius-control)` (as `.memberCard`), `color: var(--catervas-color-ink-muted)`; `.example meter { width: 100%; }`. It shows wherever the control does (setup, the Costs dialog, Settings › Spending).
- Advanced in place: `SetupAdvanced.tsx`'s page becomes `export function AdvancedSettings()`, a fragment of today's sections and the `elsewhere` refusal line (without `useStart`'s `refused`), no `Wizard`, no hide button, no foot. `SetupFinish.tsx` replaces the `Switch` with `ShowAdvanced` (`controls="finish-advanced"`) and, open, `<div id="finish-advanced"><AdvancedSettings /></div>` above the foot. `App.tsx`: `/setup/advanced` becomes `<Navigate to="/setup/finish" replace />` inside the `TeamSetup` route. `advancedLead` is deleted. Start stays enabled; `team.save` refuses an invalid team and `useStart` says why, as today.
- Web tests: the kept draft makes `TeamSetup` wait for `serve.status`, so from Task 7 on every `team.test.tsx` test that renders a team-setup route calls `await answerStatus(s, false, 1, { setup_pending: true })` after answering `team.propose`, and `describe("team setup")` gains `afterEach(() => sessionStorage.clear())`.
- Spec revision 0.87 (2026-10-10), Task 10.

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs                         Task 1
crates/runtime/src/preview.rs, preview/docker.rs, orchestrator/{session,verify,messages}.rs   Task 1
crates/runtime/tests/playwright_connector.rs                                   Task 1
crates/store/src/scan.rs, crates/store/tests/project_files.rs                  Task 2
crates/runtime/src/daemon/team.rs                                              Task 3
apps/web/src/pages/setup/{PreviewFields,SetupTeam}.tsx                          Task 4
apps/web/src/pages/setup/SetupPermissions.tsx, apps/web/e2e/setup-team.spec.ts  Task 5
packages/ui/src/Stepper.{tsx,module.css,test.tsx}                              Task 6
apps/web/src/pages/setup/{Wizard,SetupScan,SetupProject,TeamSetup}.tsx, setup.test.tsx   Task 7
apps/web/src/pages/setup/SetupSpending.tsx                                     Tasks 7 (Back), 8 (example)
apps/web/src/pages/setup/setup.module.css                                      Task 8
apps/web/src/pages/setup/{SetupAdvanced,SetupFinish}.tsx, apps/web/src/app/App.tsx   Task 9
apps/web/src/strings/en.ts                                                     Tasks 4, 5, 7, 8, 9
apps/web/src/pages/setup/team.test.tsx                                         Tasks 4, 5, 7, 8, 9
docs/SPEC.md, docs/design/web-ui.md                                            Task 10
```

## Interfaces

Consumes: `scan_project`, `ProjectScan` (`catervas-store`); `Team::preview` (`catervas-core`); `ShowAdvanced`, `Bullets`, `dollars`, `useDefaults`, `useQuery`, `ServeStatus` (web, on main); `renderApp`, `answerQuery`, `answerStatus` (web tests).

Produces:

```rust
pub struct Preview { pub prepare: Option<String>, pub start: String, pub port: Option<u16>, pub path: String } // core
pub struct AppStart { pub prepare: String, pub start: String }                    // store
pub struct ProjectScan { /* existing */ pub app_start: Option<AppStart> }
pub(crate) fn listening_ports(proc_net: &str) -> Vec<u16>;                         // runtime
pub trait RunningPreview { fn port(&self) -> u16; /* existing */ }
pub(super) fn preview_address(port: u16, path: &str) -> String;
pub(super) fn design_review_message(contract: &TaskContract, diff: &str) -> String;
```
```ts
export type Preview = { prepare?: string; start: string; port?: number | string; path: string };
export function Stepper(props: { steps: string[]; current: number; onPick?: (index: number) => void }): JSX.Element;
export function keepDraft(root: string, draft: Draft): void;
export function keptDraft(root: string): Draft | undefined;
export function forgetDraft(): void;
export function AdvancedSettings(): JSX.Element;
```

## Tasks

### Task 1: the preview's port is optional and found

Files: modified `docs/schemas/team.schema.json`, `crates/core/src/team.rs` (its tests' `port: 4400` and `port: 3000` become `Some(…)`), `crates/runtime/src/preview.rs`, `preview/docker.rs`, `orchestrator/session.rs`, `orchestrator/verify.rs`, `orchestrator/messages.rs`, `crates/runtime/tests/playwright_connector.rs` (`serving()`'s port `Some(PORT)`; `BridgePreview` gains `port()`), and any other compile error the `Option` raises in `crates/`, fixed by the decisions above (test fakes `preview.port.unwrap_or(5173)`).

- `validates_the_preview_and_the_connectors` (core, extended) — a preview of `{start}` alone validates, and `team.preview()` answers `port: None`, `path: "/"`; 1023 and 65536 are still refused at `/preview/port`; `{port}` without `start` is still refused at `/preview`.
- `listening_ports_reads_the_listening_rows` (docker.rs) — given a `/proc/net/tcp` header, a row `0100007F:1F90 … 0A`, a row on `:0050` `0A`, and a row `0100007F:1F91 … 01`, and a tcp6 row on `:1F90` `0A`, answers `[8080]`.
- `listening_ports_orders_and_skips_bad_rows` — rows on `:1F91` then `:1F90` (both `0A`) and a row with no `:` answer `[8080, 8081]`; the empty string answers `[]`.
- `preview_started_records_the_port_found` (session.rs) — a team whose preview has no port, with `FakePreviews::ready()` (`NamedPreview { port: preview.port.unwrap_or(5173) }`), records `preview.started` with `port: 5173`.
- `a_browsing_session_is_told_the_preview_address` (session.rs) — that session's `initial_prompt` ends with "The app's preview is at http://localhost:5173/."; a session not given the browser has no such line.
- `the_design_review_points_to_the_address_at_the_end` (messages.rs, new) — `design_review_message` holds no `http://localhost` and contains "the app's preview address is at the end of this message".
- `finds_the_port_it_was_not_told` (playwright_connector.rs, `#[ignore = "needs docker"]`) — `serving()` with `port: None` starts, `preview.port()` is `PORT`, and `preview.origin()` is `http://localhost:<PORT>`.

- [x] `feat(runtime): find the preview's port when it is not set`

### Task 2: the scan suggests how to start the app

Files: modified `crates/store/src/scan.rs`, `crates/store/tests/project_files.rs` (its two `ProjectScan` literals gain `app_start: None`). `AppStart` derives `Debug, Clone, PartialEq, Eq`.

- `suggests_the_dev_script_to_start_the_app` — pnpm with scripts `["dev","start","test"]` answers `AppStart { prepare: "pnpm install", start: "pnpm run dev" }`.
- `suggests_start_then_preview` — npm with `["start","preview"]` answers `npm run start`; with `["preview"]` alone, `npm run preview`.
- `suggests_nothing_without_a_script_or_on_cargo` — npm with `["test"]` answers `None`; cargo answers `None`; no toolchain answers `None`.

- [ ] `feat(store): suggest how to start the project's app`

### Task 3: setup proposes the suggestion

Files: modified `crates/runtime/src/daemon/team.rs`. Consumes Task 2.

- `scan_answers_how_to_start_the_app` — `project.scan` on a project with `pnpm-lock.yaml` and a `dev` script answers `app_start: {"prepare":"pnpm install","start":"pnpm run dev"}`; without the script, `app_start: null`.
- `proposes_the_preview_the_scan_found` — `team.propose` there answers `team.preview` `{"prepare":"pnpm install","start":"pnpm run dev","path":"/"}` with no `port`; a team that already has a preview keeps its own; a project with no suggestion proposes none.

- [ ] `feat(runtime): propose the preview the scan found`

### Task 4: the Designer's card says how the app starts

Files: modified `PreviewFields.tsx`, `SetupTeam.tsx`, `en.ts`, `team.test.tsx`.

- `says_how_the_app_starts_when_the_scan_found_it` — with `team.propose` answering a preview `start: "pnpm run dev"`, the Designer's card shows "Iris opens your app with “pnpm run dev”." and no "Start it" field; pressing "Change how it starts" shows the four fields, "Start it" holding `pnpm run dev`.
- `asks_how_to_open_the_app_when_the_designer_is_on` (existing, kept) — with no proposed preview the fields show as today, the port labelled "Port (optional)".
- `leaves_the_port_out_when_it_is_empty` — Continue with start `npm start` and an empty port sends `team.validate` a `preview` with no `port` key.

- [x] `feat(web): say how the app starts instead of asking for a port`

### Task 5: "What they may do" starts answered and says less

Files: modified `SetupPermissions.tsx`, `en.ts`, `team.test.tsx`, `apps/web/e2e/setup-team.spec.ts`. Its tests answer `settings.defaults` with `{ policy: { permissions: { run_commands: true, push: false } }, budgets: {}, rules: {} }`.

- `asks_both_permission_questions` (existing, changed) — on arrival, once `settings.defaults` is answered, "Yes, the Developer and Architect may" and "No, keep everything on this computer" are checked and Continue is enabled; "Nothing runs until" and "Answer both questions" appear nowhere; each question's note is inside its info button and not in the page's text (`onlyInTips`).
- `shows_what_each_agent_may_do_on_request` — the "What each agent may do already" heading is absent until "Show what each agent may do" is pressed (its `aria-expanded` then `true`), then present; pressing "Hide what each agent may do" removes it.
- e2e: the two `.check()` calls become `expect(...).toBeChecked()`.

- [x] `feat(web): start the permission questions at the recommended answers`

### Task 6: earlier steps are links

Files: modified `packages/ui/src/Stepper.tsx`, `Stepper.module.css`, `Stepper.test.tsx`.

- `links_the_earlier_steps` — with `current={2}` and `onPick`, exactly two buttons, named after steps 1 and 2; pressing the second calls `onPick(1)`; the current and later steps are not buttons.
- `links_nothing_without_on_pick` — no buttons.

- [x] `feat(ui): link a stepper's earlier steps`

### Task 7: setup can go back

Files: modified `Wizard.tsx`, `SetupScan.tsx`, `SetupProject.tsx`, `TeamSetup.tsx`, `SetupSpending.tsx`, `en.ts`, `setup.test.tsx`, `team.test.tsx` (every test rendering a team-setup route answers `serve.status` and the describe clears `sessionStorage`, as the Decisions' web-tests line says). Consumes Task 6.

- `goes_back_to_an_earlier_step_from_the_list` (setup.test.tsx) — on `/setup/scan`, pressing "Your AI account" in the step list lands on `/setup/account`.
- `scan_goes_back_to_the_project` — Scan's Back lands on `/setup/project`.
- `keeps_the_project_already_taken_on` — on `/setup/project` with `serve.status` `project_root: "/home/u/shop"`, `setup_pending: true`, the primary button reads "Keep shop" and lands on `/setup/scan`; with `project_root: null` there is no "Keep" button.
- `keeps_the_team_answers_when_going_back` (team.test.tsx) — rename the Developer to "Theodora", press "What we found" in the step list, then "That's right": the Developer is still "Theodora"; under another `project_root` the suggested name returns.
- `spending_back_keeps_the_typed_limit` — choose a daily limit of 25, Back, then Continue on permissions: the field holds 25.
- `forgets_the_draft_once_the_team_starts` — after a successful start, `sessionStorage` has no `catervas.setupDraft`.

- [x] `feat(web): go back to any earlier setup step`

### Task 8: the daily limit shows an example

Files: modified `SetupSpending.tsx`, `setup.module.css`, `en.ts`, `team.test.tsx`.

- `shows_an_example_of_the_limit` — with "Stop the team after a set amount each day" and 20, the page shows "Today the team has spent $12.80, of $20.00 a day." and "At $20.00, everyone pauses and you get a message.", and a `meter` with `value` 12.8 and `max` 20; with "No limit", or with 0, no example.

- [x] `feat(web): show an example of a daily limit`

### Task 9: advanced settings open in place on Finish

Files: modified `SetupAdvanced.tsx`, `SetupFinish.tsx`, `App.tsx`, `en.ts`, `team.test.tsx`.

- `opens_advanced_settings_in_place_on_finish` — on `/setup/finish`, no switch named "Show advanced settings"; pressing the button of that name keeps the URL, sets its `aria-expanded` to `true` and shows "Team rules"; pressing "Hide advanced settings" hides it.
- `the_old_advanced_address_goes_to_finish` — `/setup/advanced` lands on `/setup/finish`.
- The existing `chooses_how_work_is_finished`, `edits_the_checks_and_the_plan_check`, `says_a_refused_check_or_note_in_plain_words`, `puts_each_setting_back_to_its_default` and `starts_a_team_that_plans_in_sprints` reach advanced settings by the button; their queries for the advanced settings' controls are scoped `within(document.getElementById("finish-advanced"))`, and assert as before.

- [x] `feat(web): open setup's advanced settings in place`

### Task 10: the spec

Files: modified `docs/SPEC.md`, `docs/design/web-ui.md` (any line naming setup's Advanced switch or `/setup/advanced`).

SPEC: header revision 0.87 (2026-10-10, standalone plan `setup-friction`, ADR 0054); 4.1 line 101: the preview card shows how the app starts when the scan found a script, the port optional, and "Behind an Advanced switch on the last screen" becomes "Under the last screen's Show advanced settings button, opened in place"; a sentence that each earlier step in the step list is a link back, Scan has Back, the project step offers "Keep <name>", and the team's answers are kept for the tab while the project is the same; the daily limit shows an example; line 103: the two permissions start at the recommended answers, which the user may change, and "Nothing runs until the two permissions are set." is deleted; 8.3 line 867: `port` optional, found as ADR 0054 says, and the browsing session told the address.

- [ ] `docs(spec): record setup asking less and going back`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (in this container, /tmp/claude-0/fullcheck.sh, if present)
pnpm --filter @catervas/web exec playwright test e2e/setup-team.spec.ts
# expected: all passed
```
