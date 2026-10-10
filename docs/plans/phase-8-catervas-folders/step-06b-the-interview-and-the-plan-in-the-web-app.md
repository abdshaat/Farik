# Phase 8, step 06b: The interview and the plan in the web app

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 4.2, 4.3; F3, F8
Depends on: step 06 of this phase (planned, not yet executed: `product_plan.state`, `heard` in `chat.messages`, `product_plan_draft`, `Transitions::awaits_the_product_plan`, `is_the_product_plan`); step 03b (planned, not yet executed: the owner's `acceptance` row for the product plan task, and the strings `docNameSpec`, `docNameRoadmap`, `docChangesNotes`, `docChangesNotesHint`, `docChangesNeedNotes`, `docChangesApprove`, `docChangesSendBack`); step 04b (planned, not yet executed: `Markdown`, `apps/web/src/components/Markdown.tsx`); phase 6's Chats and Today (merged in #19).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): no Blocking; two Should (the `draftPlanSent` condition, the card's note under `pull_request`/`manual`) and nits folded
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19): `docs/design/mockups/ProductInterview.dc.html` and `ProductPlanApproval.dc.html`, as drawn; their words are this plan's words.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at dd4f1f6f.

## Goal

The owner of a new project sees the interview as drawn: Mira's chat says what it is for, her summary reads as "What I heard" with "Draft the product plan" under it, and the plan she drafts comes to Today as "Your product plan to approve", both documents in full, with Approve and Send back; the team band says the team waits for the plan. Out of scope: anything for an existing repository; the Catervafication card (07); the command line.

## Decisions

- **One query for the state**, step 06's `product_plan.state`, asked by the Chats page (for the Product Manager's chat alone) and by Today, with `useQuery` (`apps/web/src/app/store.ts:29`), which asks again after events. "Interviewing" means `newProject` not null and `approved` false.
- **The chat's intro.** While interviewing, the Product Manager's chat head (`OneToOne.tsx`, the muted line under the title) says "Before your team builds anything, {name} asks what you want to make. This chat only talks: your product plan is written next, and you approve it before anything is built." (`chatsInterviewIntro`), and on a phone it is shown in place of `chatsIntroPhone` (`OneToOne.tsx:147`); every other chat, and this one once approved, keeps `chatsIntro`.
- **"What I heard"**, `Heard.tsx` beside `ProposalBox` (`OneToOne.tsx`): a reply whose `heard` is not null shows, under its text, a section labelled by its heading "What I heard" (`heardTitle`) holding a description list: "Your customers", "The problem", "It must", "Not now", "It works when" (`heardCustomers`, `heardProblem`, `heardMust`, `heardNotNow`, `heardWorksWhen`), each value as text with its line breaks (the agent's words: React text, never markup, 8.6). Any agent's `heard` is shown so; the button below follows the state alone.
- **The button.** While interviewing and `task` is null, "Draft the product plan" (`draftPlan`, primary) and the line "I write it as my first task: a product description and a roadmap. It comes to you on Today to approve." (`draftPlanLine`) end the Product Manager's newest message: inside its "What I heard" when it has one, as drawn, else under its text, so a summary the model never sent cannot strand the owner. It sends `{ command: "product_plan_draft", body: {} }` through `useCommand` (`apps/web/src/pages/dialogs/StartSprint.tsx:12`); while interviewing and `task` is not null, the place shows instead of the button, as `role="status"`, "Drafting it as {id}. It comes to you on Today to approve." (`draftPlanSent`, `{id}` the query's `task.taskId`, which the filing's event makes it ask again); a refusal shows its words under the button. `refusals.ts`'s `WORDS` gain, through `en.ts` keys `refuseNotANewProject`, `refusePlanApproved`, `refusePlanOpen`, `refusePlanNoWriter`, `refusePlanNoReviewer`: `not_a_new_project` "This project was not started here, so it has no product plan to draft."; `product_plan_approved` "Your product plan is approved already."; `product_plan_open` "{name} is drafting your product plan already."; `product_plan_no_writer` "Your team has no Product Manager at work to write it."; `product_plan_no_reviewer` "An Architect or a Scrum Master checks the plan before you read it. Add one on the Team page.".
- **Today's card.** The `acceptance` row of `waiting.list` (03b) whose `taskId` is `product_plan.state`'s `task.taskId` is shown as `ProductPlanCard` (`apps/web/src/pages/ProductPlanCard.tsx`) in its place, counted as one in "Waiting on you" (`waitingTitle`): the author's avatar, "Your product plan to approve" (`planToApprove`), "From {name}, written from your chat {when}" (`planFrom`, `{when}` from the task's `createdAt` in the viewer's time: the same day "this morning" before 12:00, "this afternoon" before 18:00, else "this evening"; the day before "yesterday"; older "on {weekday}"), "Here is what we will build and in what order. Read both parts, then approve it or send it back with what to change." (`planIntro`); then each of `documents` under its name (03b's `docNameSpec`, `docNameRoadmap`), its text through `Markdown` (04b: raw HTML off, unsafe links left as text); "Nothing else is planned until you approve. Then your team documents the plan in a first sprint." (`planGate`); 03b's notes label and hint over a text box; Approve and Send back. Approve sends `{ command: "human_accept", body: { taskId, subject: "result" } }`; Send back with blank notes sends nothing and says 03b's `docChangesNeedNotes`, else `{ command: "human_send_back", body: { taskId, subject: "result", message, failedCriteria: [] } }`, as the Gate page sends them (`Gate.tsx:374`, `:400`). Under the card's buttons, the note is 03b's `docChangesPullRequest` or `docChangesManual` under those integration policies (the policy as `team.get` gives it), and nothing under `auto_merge`. Until `documents` holds both, the row stays the generic acceptance row (`WaitingRow`, `Today.tsx:458`), whose page still accepts. On a phone the buttons span the width, as 03b's card. Rejected: a new command, which the human's acceptance already is.
- **The band line.** While interviewing, the band's first line (`Today.tsx:212-232`) is "{project}: your team waits for your product plan" (`todayWaitsForPlan`) in place of the sprint or Backlog line, `{project}` the new project's `name` with each `-` a space and each word's first letter upper case (`little-oak-cakes` "Little Oak Cakes").
- **The agents' lines** come from the daemon, as every band line does: `while_the_plan_waits` in `catervas_store::activity` rewrites `activity`'s answer (`crates/store/src/activity.rs:70`) when the `team.activity` arm (`crates/runtime/src/daemon/gates.rs:251`) finds `awaits_the_product_plan`: an `Idle` agent says "Nothing to plan until your plan is approved" for a Scrum Master and "Nothing to do until your plan is approved" for every other role; a `Waiting` agent whose task is the product plan's says "Waiting on you: your product plan". Rejected: rewriting them in the page, which would split one line's words between two places.
- **Tests**: component tests (vitest) with `answerQuery` (`apps/web/src/test/render-app.tsx:55`) and each sent body held to `command.schema.json` by `refusedBy` (`apps/web/src/test/schema.ts:121`); the activity rewrite by a unit test in the store. No e2e spec: the harness replays recorded sessions, and an interview is step 08's live check.

## File map

```
crates/store/src/activity.rs, crates/runtime/src/daemon/gates.rs   modifies: while_the_plan_waits and its call (Task 1)
apps/web/src/pages/Heard.tsx, apps/web/src/pages/OneToOne.tsx, Chats.module.css, Chats.test.tsx   creates, modifies (Task 2)
apps/web/src/pages/ProductPlanCard.{tsx,module.css,test.tsx}  creates (Task 3)
apps/web/src/pages/Today.tsx, Today.test.tsx                 modifies (Task 3)
apps/web/src/test/productPlan.ts                             creates: the mockups' state and documents (Task 2)
apps/web/src/strings/en.ts, apps/web/src/app/refusals.ts     modifies (Tasks 2, 3)
docs/SPEC.md                                                 modifies (Task 4)
```

## Interfaces

Consumes: from step 06, `product_plan.state`'s result, `heard`, `product_plan_draft`, `Transitions::awaits_the_product_plan`, `is_the_product_plan`; from 03b, the acceptance row and its strings; from 04b, `Markdown`; on main, `useQuery`, `useCommand`, `commandSaid`, `Avatar`, `ProposalBox`'s place in `Row` (`OneToOne.tsx`), `WaitingRow`, `activity`, `AgentActivity`.

Produces:

```rust
// catervas_store::activity
pub fn while_the_plan_waits(all: &mut [AgentActivity], team: &Team, plan_task: Option<&TaskId>);
```

```ts
// apps/web/src/pages/ProductPlanCard.tsx
export type ProductPlanState = { newProject: { name: string } | null; approved: boolean;
	task: { taskId: string; status: TaskStatus; createdAt: string } | null; documents: { path: string; text: string }[] };
export function ProductPlanCard(props: { state: ProductPlanState; name: string; avatar?: AvatarKey; again: () => void }): JSX.Element;
export function writtenWhen(at: string, now: Date): string;
export function projectName(name: string): string;
// apps/web/src/pages/Heard.tsx
export type HeardSummary = { customers: string; problem: string; must: string; notNow: string; worksWhen: string };
export function Heard(props: { heard: HeardSummary; children?: ReactNode }): JSX.Element;
```

## Tasks

### Task 1: The band's lines while the plan waits

Files: `activity.rs` (`while_the_plan_waits`, tests); `gates.rs` (the `team.activity` arm calls it with the product plan task from the log, when `awaits_the_product_plan`; a test).

- `says_the_team_waits_for_the_plan` (`activity.rs`) — an idle Scrum Master's line becomes "Nothing to plan until your plan is approved", an idle Developer's and Product Manager's "Nothing to do until your plan is approved", a Product Manager waiting on the product plan task "Waiting on you: your product plan", and a working or paused agent's line is unchanged. RED: no function.
- `tells_the_band_while_the_plan_waits` (`gates.rs`, integration) — with `project.started`, `team.activity` answers the Developer's "Nothing to do until your plan is approved"; with both documents on `main`, "Nothing to do right now". RED.

- [ ] `feat(store): say the team waits for a new project's product plan`

### Task 2: The interview in the chat

Files: created `Heard.tsx`, `apps/web/src/test/productPlan.ts` (the mockup's five answers and summary; a state interviewing with no task); modified `OneToOne.tsx` (`ChatMessage.heard`, `fromEvent`, the intro, the box and the button in `Row` for the Product Manager's newest message), `Chats.module.css`, `Chats.test.tsx`, `en.ts`, `refusals.ts`.

- `introduces_the_interview` — the Product Manager's chat, `product_plan.state` interviewing: the head says `chatsInterviewIntro` with "Mira"; approved, it says `chatsIntro`; the Developer's chat never asks the query. RED.
- `shows_what_mira_heard` — a reply with `heard`: a region named "What I heard" holds the five labels and values in order; a value holding U+202E shows it written out (`showsWhatItHides`, `apps/web/src/test/hidden.ts`). RED: no component.
- `drafts_the_plan_from_the_newest_message` — interviewing with no task: one "Draft the product plan" button, inside the newest reply's "What I heard"; pressing it sends `{ command: "product_plan_draft", body: {} }`, held to the schema, then shows `draftPlanSent` with the task's id once the query answers it; with no `heard` in the chat, the button sits under the newest reply's text; with a task, `draftPlanSent` with its id in place of the button; approved, neither. RED.
- `a_refused_draft_says_why` — refused `product_plan_no_reviewer: …`: shows "An Architect or a Scrum Master checks the plan before you read it. Add one on the Team page.". RED.
- `passes_axe_on_the_chats` (`Chats.test.tsx:519`) holds with the interview shown.

- [ ] `feat(web): interview the owner of a new project in the chat`

### Task 3: The product plan on Today

Files: created `ProductPlanCard.tsx`, `.module.css`, `.test.tsx`; modified `Today.tsx` (the query, the card in place of its acceptance row, the band line), `Today.test.tsx`, `en.ts`.

- `shows_the_plan_in_full` — the mockup's state: a region named "Your product plan to approve" holds "From Mira, written from your chat this morning", `planIntro`, "Product description" then "Roadmap", each document's headings and lines as rendered Markdown ("Who it is for", "Now"), and `planGate`. RED: no component.
- `approve_accepts_the_result` — Approve sends `{ command: "human_accept", body: { taskId: "CTV-2", subject: "result" } }`, held to the schema. RED.
- `send_back_needs_notes` — blank: nothing sent, `docChangesNeedNotes` shown; with "Add gift vouchers to Next": `human_send_back` with that message, `subject: "result"` and `failedCriteria: []`, held to the schema. RED.
- `says_what_approving_does_under_the_policy` — under `pull_request` the note under the buttons is `docChangesPullRequest`'s sentence, under `manual` `docChangesManual`'s, under `auto_merge` none. RED.
- `says_when_it_was_written` — `writtenWhen` at 09:30, 14:00 and 19:00 of `now`'s day: "this morning", "this afternoon", "this evening"; the day before "yesterday"; three days before "on <weekday>". `projectName("little-oak-cakes")` is "Little Oak Cakes". RED.
- `puts_the_plan_in_place_of_its_acceptance_row` (`Today.test.tsx`) — `waiting.list` with the product plan's acceptance row and a question, the state with both documents: "Waiting on you (2)", the card in place of the row; with `documents` empty, the generic acceptance row. RED.
- `says_the_team_waits_for_the_plan` (`Today.test.tsx`) — interviewing: the band's first line is "Little Oak Cakes: your team waits for your product plan", and no sprint or Backlog line; approved: the sprint line as before. RED.

- [ ] `feat(web): approve a new project's product plan on Today`

### Task 4: Spec

`docs/SPEC.md`: 4.1 (the new project's Today: the band's line and the agents' lines while the plan waits); 4.2 (the product plan on Today: the card, both documents rendered, Approve and Send back as the human's acceptance of the task); 4.3 (the interview's intro, "What I heard", the button, where it sits, and its refusals); F3 and F8 where they list Today's rows and the chat; the next spec revision after step 06's.

- [ ] `docs(spec): record the interview and the product plan in the web app`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Task 1 names an integration test
# expected: xtask check: ok
```
