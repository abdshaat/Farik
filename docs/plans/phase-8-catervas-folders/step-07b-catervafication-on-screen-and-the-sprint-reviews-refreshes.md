# Phase 8, step 07b: The Catervafication on screen, and the sprint review's refreshes

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 4.2, 5.9, 5.17
Depends on: step 07 of this phase (planned, not yet executed: `catervafication_start`, `catervafication.get`, `catervafication.started`); step 03 (planned, not yet executed: `catervas_write_folder_doc` offered in `CEREMONY_TOOLS` to a ceremony session of a role with a folder, proposals at the review alone, `folder_doc.written`); step 03b (the "What moved" lines' form; it leaves this step the line "Ada brought the architecture notes up to date with sprint 4"); step 04b (planned, not yet executed: `apps/web/src/pages/Files.tsx`); phase 6's setup and Today (merged in #19); task ids `CTV-<n>` (merged on main, 0f2229b7).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (two rounds, ADR 0032): ready. Round 1 found two Blocking (the Architect's refresh missed the sprint's last integration; the PM never heard the owner's decisions), decided as the reviewer proposed; round 2's Should (step 03's pending-proposal lines after the decision lines) folded by the controller
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19): `docs/design/mockups/CatervaficationOffer.dc.html` (the end of setup) and `CatervaficationToday.dc.html` (Today after "Later"); their words are this plan's words.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at dd4f1f6f; the names are what count.

## Goal

When setup ends on an existing repository, the owner sees "Your team is ready" and the recommendation "Start with a Catervafication sprint", one row per agent who will write something, with "Start" and "Later"; "Later" leaves a quiet card on Today and the same Start on the Files page until it is started. At every sprint review the Product Manager proposes the spec's and the roadmap's changes and writes its sprint report, and the Architect brings `features.md`, `jobs.md` and `structure.md` up to date with what the sprint integrated, one session each per sprint; "What moved" says "Ada brought the architecture notes up to date with sprint 4". Out of scope: the "What moved" lines about setup the Today mockup draws as context (Decisions); "Open in Files" on Today's approval card (04b); the command line.

## Decisions

- **The offer is a page of its own after setup**, `/setup/ready` (`apps/web/src/pages/setup/SetupReady.tsx`), in the setup's `Wizard` with every step done, as drawn. `useStart` (`apps/web/src/pages/setup/TeamSetup.tsx:300`) navigates there instead of `/` (`:315`). Its route is beside `/setup/scan` (`apps/web/src/app/App.tsx:53`), outside `<TeamSetup>`, which holds a draft this page does not need. The page asks `catervafication.get` and `team.get`; when `offered` is false (a new project, or one already started) it goes to `/` with `replace`, so a new project lands where it does today. Rejected: the offer on Today, which the mockup draws inside setup.
- **"Every step done."** `Wizard` is given `step={8}`, one past the last (seven with a saved team's answers, `Wizard.tsx:32`), so no step is current; `Stepper` (`packages/ui/src/Stepper.tsx:4`) gives each step before `current` a visually hidden ", done" (`uiStrings.stepDone`), as the mockup's markup has. The setup's earlier pages then say ", done" for the steps behind them too, which is true.
- **The page's words**, exactly (`en.ts`): title "Your team is ready" (`readyTitle`); lead "{names} have joined {team}. They know what Catervas found in your project; next, they can learn it properly." (`readyLead`), `{names}` the active agents' display names in team-file order, joined by ", " with " and " before the last, `{team}` `team.name`; a section labelled by its heading: "Recommended" (`readyRecommended`), heading "Start with a Catervafication sprint" (`readyOfferTitle`), "Your team reads your project and writes down what it learns before building anything. You can read all of it in Files." (`readyOfferLead`); a list, one row per role of `roles` in its order, for the first active agent of that role in team-file order: its `Avatar`, its name in bold, its `RoleTag`, and the role's line: Product Manager "writes what your product is and what comes next, as your product plan to approve." (`readyPm`), Architect "writes how your project is put together, with diagrams." (`readyArchitect`), Software Developer "writes how your project is built and worked on: its habits and rules." (`readyDeveloper`), UI/UX Designer "writes down every screen your project has." (`readyDesigner`, not drawn: the mockup's team has no Designer), Marketing Specialist "writes your marketing plan, after you approve the product plan." (`readyMarketing`); then "Your team only adds its notes; nothing else in your project changes." (`readyNote`), "Start" (primary, `readyStart`), "Later" (`readyLater`) and "Later keeps this on Today." (`readyLaterNote`). The Scrum Master has no row, as drawn: it writes the cadence at the retro, not in a task (step 07).
- **Start** sends `{ command: "catervafication_start", body: {} }` through `useCommand` (`apps/web/src/pages/dialogs/StartSprint.tsx:12`); done, it goes to `/`; refused, the refusal's words show under the buttons as an alert. **Later** sends nothing and goes to `/`. `refusals.ts`'s `WORDS` (`apps/web/src/app/refusals.ts:9`) gains `catervafication_started` → `refuseCatervaficationStarted` "Your team is already documenting this project.", `catervafication_new_project` → `refuseCatervaficationNewProject` "A new project's documenting sprint starts by itself once you approve its product plan.", `catervafication_no_product_manager` → `refuseCatervaficationNoPm` "Your Product Manager is paused. Resume them on the Team page, then start.", `catervafication_no_writers` → `refuseCatervaficationNoWriters` "No one on your team can both write and have their notes checked. Add an Architect or a Scrum Master on the Team page, then start.".
- **The quiet card**, `CatervaficationCard` (`apps/web/src/pages/CatervaficationCard.tsx`), as drawn: "Your team hasn’t documented this project yet" (bold, `cardTitle`, its id the button's `aria-describedby`), "A first sprint where your team reads your project and writes down what it learns, before building anything." (`cardLead`), and "Start", sending the same command, its refusal under it; `again` asks `catervafication.get` again. On Today (`apps/web/src/pages/Today.tsx:244`) it follows the request box while `offered`, outside the "Waiting on you" list and its count (the caption: "not a 'Waiting on you' item"). On the Files page (step 04b's `Files.tsx`) it heads the page while `offered`. Rejected: a waiting row, which the mockup's caption rules out.
- **The setup lines of "What moved"** the Today mockup draws ("You finished setting up your team.", "Catervas read your project, Corner Bakery orders.") are context: no event marks setup's end, and they are about setup, not this step. Left out, and said so in the pull request for the founder.
- **The task page** names the event: `TOLD` (`apps/web/src/pages/TaskDetail.tsx:76`) gains `"catervafication.started": "toldCatervafication"`, "Catervas filed it to document the project.".
- **The refreshes** (design "Each sprint"). In `review_and_retro` (`crates/runtime/src/orchestrator/rules.rs:367`), after the review has run and before the retro: the Product Manager's refresh, then the Architect's, each a `ceremony` session in thread `review` of the first active agent of that role, `read_only: true`, given `CEREMONY_TOOLS` (`rules.rs:679`; step 03 lists `catervas_write_folder_doc` there and offers it only to a role with a folder; a proposal needs thread `review`), once per sprint under the ceremonies' bound. `EndedSprint` (`crates/runtime/src/ceremonies.rs:245`) gains `reviewed_by: Vec<String>`: each agent with a review session that `has_run` (`:149`) after the end, by the session's agent on its envelope. When the review's runner is the Product Manager (no Scrum Master), its review session is its refresh: `sprint_review_message` (`messages.rs:589`) gains the refresh's asks for it, and no second session runs. The Product Manager's runs only when `docs/catervas/product/spec.md` is on the integration branch (`Git::has_file`), the Architect's only when `docs/catervas/architecture/structure.md` is, and the Architect's only when a task of the sprint has a `task.integrated` after the sprint's `sprint.started` (read from the log, not `sprint_events`, which stops at the end). Under `auto_merge` the refreshes, not the review, are passed over while a task of the sprint is `accepted` and `awaiting_integration`, so rule 2 integrates it in that tick first. Passed over on a spent day or while the agent sleeps, as the review is. Rejected: a thread of its own (step 03's tool and proposals name `review`); a session per integration (the founder: one per sprint, for cost).
- **Their messages**, exact openings: `product_refresh_message` "The sprint <n> review is done. Keep the product plan true to what the sprint did." then: propose changes to `docs/catervas/product/spec.md` and `roadmap.md` with `catervas_write_folder_doc`, with `agent_text` and `summary`, only where the sprint changed what the product does or what comes next, and nothing otherwise; write the sprint report, for agents, to `docs/catervas/product/reports/<sprint id>.md`; then the sprint's tasks, each with its status and its completion note in an untrusted block (`untrusted_block`, `crates/runtime/src/prompt.rs:257`); then step 03's lines for the Product Manager's own proposals decided since its last `review`-thread session ("The owner approved …" / "The owner sent back …"), the owner's words unwrapped as step 03 gives them. `architecture_refresh_message` "Bring docs/catervas/architecture/features.md, jobs.md and structure.md up to date with what sprint <n> added to the project." then "Change only what these tasks changed, each file with catervas_write_folder_doc.", then each integrated task's id and title and its completion note in an untrusted block. `<n>` is the sprint id without its `S`.
- **The moved line.** `moved_since` (`crates/store/src/activity.rs:291`) also reads `folder_doc.written`, `session.started` and `sprint.ended`: for each Architect's `review` session with a `folder_doc.written` since `since`, one line at its latest write's time, "<name> brought the architecture notes up to date with sprint <n>", `<n>` the sprint of the last `sprint.ended` before the session started, the line's form 03b's; under the team's `pull_request` policy the line is "<name> sent the architecture notes for sprint <n> as a pull request", since the change reaches the project only when it is merged.
- **Tests**: component tests (vitest) with `answerQuery` (`apps/web/src/test/render-app.tsx:55`), every sent body held to `command.schema.json` by `refusedBy` (`apps/web/src/test/schema.ts:121`); the refreshes by integration tests in `rules.rs` on recorded sessions. No e2e spec: the docs tasks would need recorded transcripts; step 08's live check runs both paths in the web app.

## File map

```
crates/runtime/src/ceremonies.rs                       modifies: reviewed_by (Task 1)
crates/runtime/src/orchestrator/{rules.rs,messages.rs} modifies: the two refreshes and their messages (Task 1)
crates/store/src/activity.rs                           modifies: the moved line (Task 2)
packages/ui/src/{Stepper.tsx,Stepper.test.tsx,strings.ts}   modifies: ", done" (Task 3)
apps/web/src/pages/setup/{SetupReady.tsx,TeamSetup.tsx,setup.module.css,setup.test.tsx}, apps/web/src/app/App.tsx   creates, modifies (Task 3)
apps/web/src/pages/{CatervaficationCard.tsx,CatervaficationCard.module.css}   creates (Task 4)
apps/web/src/pages/{Today.tsx,Today.test.tsx,Files.tsx,Files.test.tsx,TaskDetail.tsx,TaskDetail.test.tsx}   modifies (Task 4)
apps/web/src/strings/en.ts, apps/web/src/app/refusals.ts   modifies (Tasks 3, 4)
docs/SPEC.md, docs/design/catervas-folders.md, docs/plans/project-plan.md   modifies (Task 5)
```

## Interfaces

Consumes: step 07's command, query and event; step 03's tool in `CEREMONY_TOOLS` and `folder_doc.written`; step 04b's `Files.tsx`; `review_and_retro`, `assigner`, `day_is_spent`, `asleep`, `run_session`, `SessionAsk` (`rules.rs`); `has_run`, `ended_sprint`, `sprint_events`, `SprintTask` (`ceremonies.rs`); `moved_since`; `useQuery` (`apps/web/src/app/store.ts:29`), `useCommand`, `Wizard`, `Stepper`, `Avatar`, `RoleTag`, `t`; on main unless named.

Produces:

```rust
// catervas_runtime::ceremonies::EndedSprint gains `pub reviewed_by: Vec<String>`
// catervas_runtime::orchestrator::messages
pub(super) fn product_refresh_message(sprint_id: &str, tasks: &[SprintTask], decisions: &[String], pending: &[String]) -> String;   // decisions, then pending: step 03's lines ("still wait for the owner")
pub(super) fn architecture_refresh_message(sprint_id: &str, integrated: &[SprintTask]) -> String;
pub(super) fn sprint_review_message(sprint_id: &str, tasks: &[SprintTask], budget_usd: Option<f64>, spent_usd: f64, refresh: bool) -> String;
```

```ts
// apps/web/src/pages/setup/SetupReady.tsx
export function SetupReady(): JSX.Element | null;
export function joinedNames(names: string[]): string;   // "A", "A and B", "A, B and C"
// apps/web/src/pages/CatervaficationCard.tsx
export type Catervafication = { offered: boolean; roles: Role[] };
export function CatervaficationCard(props: { again: () => void }): JSX.Element;
// packages/ui strings: uiStrings.stepDone = ", done"
```

## Tasks

### Task 1: The sprint review's refreshes

Files: `ceremonies.rs` (`reviewed_by`, a test); `rules.rs` (`review_and_retro`'s two sessions, tests); `messages.rs` (the two messages, `sprint_review_message`'s `refresh`, tests).

- `knows_who_has_reviewed_an_ended_sprint` (`ceremonies.rs`) — after S1's end, `sm`'s review session completed and `pm`'s started without ending: `reviewed_by` is `["sm"]`; once `pm`'s ends completed, `["sm", "pm"]`. RED: no field.
- `refreshes_the_product_and_the_architecture_after_the_review` (`rules.rs`, integration, a team with a Scrum Master, a Product Manager and an Architect, S1 ended holding CTV-1 accepted and integrated) — the ticks run, in order, `sm`'s review, `pm`'s `review` session whose first message starts "The sprint 1 review is done. Keep the product plan true to what the sprint did." and is offered `catervas_write_folder_doc`, `ada`'s `review` session whose first message starts "Bring docs/catervas/architecture/features.md, jobs.md and structure.md up to date with what sprint 1 added to the project." and holds CTV-1 inside an untrusted block, then `sm`'s retro; no tick runs a second session for `pm` or `ada`. RED: the retro follows the review.
- `skips_the_architecture_when_nothing_was_integrated` (integration) — S1's one task cancelled: `pm`'s refresh runs and `ada`'s does not; the retro follows. With no `structure.md` on main, `ada`'s does not run; with no `spec.md`, `pm`'s does not. RED.
- `integrates_before_it_refreshes` (integration, `auto_merge`) — S1's task `accepted` and `awaiting_integration` after the review: the next tick integrates it (rule 2) and runs no refresh; the tick after runs `pm`'s. RED: the refresh runs first and `ada`'s is skipped.
- `tells_the_product_manager_the_owners_decisions` (`messages.rs`) — with a returned proposal's line, `product_refresh_message` ends with it, outside every untrusted block, and a pending proposal's "still wait for the owner" line follows the decision lines. RED.
- `the_product_managers_review_is_its_refresh` (integration, no Scrum Master) — `pm` runs the review, whose message holds the refresh's asks after the review's facts, and no second `pm` session runs before `ada`'s. RED.
- `writes_the_refresh_messages` (`messages.rs`) — each opening above, word for word; the product one names `docs/catervas/product/reports/S4.md`, `agent_text` and `summary`; a completion note holding `</untrusted>` cannot close its block. RED.

- [ ] `feat(runtime): refresh the product plan and the architecture at each sprint review`

### Task 2: What moved says the architecture notes moved

Files: `activity.rs` (`moved_since`, a test).

- `says_the_architecture_notes_were_brought_up_to_date` — `ada` ("Ada", an Architect) wrote `features.md` and `structure.md` in one `review` session after S4 ended, and `sm` wrote `delivery/cadence.md` in a retro: one line "Ada brought the architecture notes up to date with sprint 4" at the later write's time, none for `sm`; a write before `since` gives none; under `pull_request`, "Ada sent the architecture notes for sprint 4 as a pull request". RED.

- [ ] `feat(store): say when the architecture notes were brought up to date`

### Task 3: The offer after setup

Files: `Stepper.tsx`, `Stepper.test.tsx`, `strings.ts`; created `SetupReady.tsx`; `TeamSetup.tsx` (`useStart`'s `navigate`); `App.tsx` (the route beside `/setup/scan`, `:53`, outside `<TeamSetup>`); `setup.module.css`; `en.ts`, `refusals.ts`; `setup.test.tsx`.

- `marks_the_steps_behind_as_done` (`Stepper.test.tsx`) — `current` 2 of four: steps 1 and 2 hold the hidden ", done", step 3 is current, step 4 holds neither; `current` 4 of four: all four done and none current. RED.
- `offers_the_catervafication_at_the_end_of_setup` (`setup.test.tsx`) — `catervafication.get` `{ offered: true, roles: [pm, architect, developer, marketing] }` and the mockup's team (Mira, Sol, Ada, Theo, Kai; "Corner Bakery orders"): the heading "Your team is ready", the lead with "Mira, Sol, Ada, Theo and Kai have joined Corner Bakery orders.", a region named "Start with a Catervafication sprint" whose list reads, in order, Mira's, Ada's, Theo's and Kai's rows with the lines of Decisions and no Sol; every setup step says ", done". RED: no route.
- `starts_or_leaves_it_for_later` — Start sends one command whose body passes `refusedBy("emptyBody", …)` and then shows Today; Later sends nothing and shows Today; a refusal `catervafication_no_product_manager: …` shows its words as an alert and stays. RED.
- `goes_home_when_nothing_is_offered` — `offered: false`: Today is shown and nothing is sent. RED.
- `start_the_team_goes_to_the_offer` (`setup.test.tsx`, beside the start's test) — after `team.start` answers, the location is `/setup/ready`. RED: `/`.
- `joins_names_as_the_lead_does` — `[]` "", `["A"]` "A", `["A","B"]` "A and B", `["A","B","C"]` "A, B and C". RED.

- [ ] `feat(web): offer the Catervafication at the end of setup`

### Task 4: The quiet card on Today and Files

Files: created `CatervaficationCard.tsx`, `CatervaficationCard.module.css`; `Today.tsx`, `Today.test.tsx`; step 04b's `Files.tsx`, `Files.test.tsx`; `TaskDetail.tsx`; `en.ts`.

- `keeps_a_quiet_card_until_it_starts` (`Today.test.tsx`) — `offered: true` with one waiting question: the card's "Your team hasn’t documented this project yet" follows the request box, its Start is described by it, and the heading still reads "Waiting on you (1)"; Start sends the command and asks `catervafication.get` again; `offered: false`: no card. RED: no card.
- `shows_the_same_start_on_the_files_page` (`Files.test.tsx`) — `offered: true`: the card heads the page; `false`: it does not. RED.
- `names_a_catervafication_in_a_tasks_history` (`TaskDetail.test.tsx`) — an epic's history with `catervafication.started` reads "Catervas filed it to document the project.". RED: "toldOther".

- [ ] `feat(web): keep the Catervafication's Start on Today and Files`

### Task 5: Spec

`docs/SPEC.md`: 4.1 (setup ends, on an existing repository, with the offer; Start or Later); 4.2 (Today's quiet card until started, and the Files page's; F3 is the request box and does not change); 5.9 (the review's refreshes: who, when, once per sprint, the Architect's only after an integration, the Product Manager's inside its own review with no Scrum Master; the moved line); 5.17 (the sprint report's path); the next spec revision. `docs/design/catervas-folders.md`: "Each sprint" names the report's path and the Architect's skip. `docs/plans/project-plan.md`: row 07b as step 07 wrote it, marked planned.

- [ ] `docs(spec): record the Catervafication on screen and the sprint review's refreshes`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Task 1 names integration tests
# expected: xtask check: ok
```
