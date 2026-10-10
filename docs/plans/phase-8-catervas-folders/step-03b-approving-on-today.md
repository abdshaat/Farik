# Phase 8, step 03b: Approving on Today

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4, 5.1, 5.4, 5.7, 6.1, 8.6; F3, F5
Depends on: step 03 of this phase (planned, not yet executed: `OWNER_ACCEPTED`, `folder_docs`, `tools::folders`, `folder_doc_decide`, and an approval made a folder change integrated by the team's policy); steps 01 and 01b (planned, not yet executed: `catervas_core::folders`, the Product Manager's docs tasks); phase 6's Today (merged in #19); phase 7 and the Catervas rename (merged on main, 2c28b555).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): ready; no Blocking; S1–S4 and nits carried into execution
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19): `docs/design/mockups/DocumentApproval.dc.html` (Today on a computer) and `PhoneDocumentApproval.dc.html` (on a phone), as drawn.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 623be48d, whose code is ac1a606e's; the names are what count.

## Goal

The owner approves the team's changes to the product plan on Today. Each sprint review's waiting proposals are one card, "Product plan changes to approve", from the proposing agent: its summary, each document's changes line by line, a notes box, "Approve" and "Send back", on a computer and on a phone as the approved mockups draw them. The agent's line in the team band says "Waiting on you: product plan changes", and "What moved since yesterday" says who proposed how many changes from which review. Under `manual`, a folder change waiting to be added is a row of "Waiting on you" as an accepted task waiting to be added is (step 03's B1). A task whose `allowed_paths` may change `docs/catervas/product/spec.md` or `roadmap.md` is accepted by the owner, not by an agent, through the acceptance Today already shows. Out of scope: "Open in Files" on the card (step 04b, with the page it opens); the phone mockup's band sentence and section order, which are Today's own phone layout; the product plan's task shown as a document to approve with the version for people first (06); the line "Ada brought the architecture notes up to date with sprint 4" (07, with the Architect's review session); the command line.

## Decisions

- **The query**: `folder_doc_proposals.list {}` → `{ integration, cards }`, `integration` the team's policy (`auto_merge`, `pull_request` or `manual`), and the cards from the proposals pending in step 03's fold (`folder_docs`), grouped by sprint and agent, a card for each, ordered by its first proposal. A card is `{ sprint_id, agent_id, summary, documents }`: `summary` its proposals' summaries in proposal order, joined by one space (the mockup's paragraph is the roadmap's and the spec's); `documents` its proposals in order, each `{ proposal, path, lines }`, `lines` being `changed_lines` of the integration branch's file (`file_at`, `crates/store/src/git.rs:243`; empty when git has none there) and the proposed `text`, each `{ change: same | added | removed, text }`. It answers in `gates.rs`'s `query` (`crates/runtime/src/daemon/gates.rs:232`) beside `sites.list` (`:295`), from `tools::folders::proposals_list`. It is not a `waiting.list` row: those are about a task (`Waiting.task_id`, `crates/store/src/waiting.rs:80-82`) and a proposal holds none. Rejected: an optional `task_id` across `waiting.list`, the command line's lines and Today.
- **`changed_lines(before, after)`**, pure in `catervas_core::folders`: the lines of each text, a trailing `\r` dropped and blank lines left out; the common first and last lines kept as `Same`; the middle compared by longest common subsequence, taking the added line first where removing and adding keep as long a common sequence; within each run of changed lines, its removed lines then its added ones. Shown: every changed line, the unchanged line just before and just after each run, and the nearest unchanged line starting with `#` above each run; each once, in order; every other line left out. So the mockup's roadmap gives exactly its nine lines. `ponytail:` a middle whose two lengths multiply past 1,000,000 is shown whole, every old line removed then every new one added, rather than compared; compare it in pieces if a real document's change reads badly.
- **The card**, `DocumentChanges` (`apps/web/src/pages/DocumentChanges.tsx`), as drawn: the agent's picture (`Avatar`, as `Today.tsx:473`), "Product plan changes to approve" (`docChangesTitle`), "From {name}, after the sprint {n} review" (`docChangesFrom`, `n` the sprint id without its `S`), the summary, "Lines marked + are added; lines marked − are removed." (`docChangesLegend`); then each document under its name, "Roadmap" (`docNameRoadmap`) for `docs/catervas/product/roadmap.md`, "Product description" (`docNameSpec`) for `spec.md`, else its path; its lines as a list, an added one marked `+` and a removed one `−` (both `aria-hidden`) after the visually hidden "Added: " or "Removed: " (`docChangeAdded`, `docChangeRemoved`); a line starting with `#` shown without its `#`s and spaces, in the heading style, and a line starting with `- `, `* ` or `+ ` without that mark. Then "Notes for {name}" (`docChangesNotes`) with "(needed to send it back)" (`docChangesNotesHint`) over a text box, "Approve" (`docChangesApprove`), "Send back" (`docChangesSendBack`) and the note under them, which says what approving does under the team's policy (the controller's ruling of 2026-10-10: an approval is a folder change, integrated as a task's branch is, so nothing promises "at once"): under `auto_merge` "Nothing changes until you approve." (`docChangesNothingYet`, the mockup's); under `pull_request` "Nothing changes until you approve. Then Catervas opens a pull request, and the change is in your project once it is merged." (`docChangesPullRequest`); under `manual` "Nothing changes until you approve. Then it waits under Waiting on you for you to add it to your project." (`docChangesManual`), which is the row below. Step 08's live check is where the founder first sees the `pull_request` and `manual` sentences. The summary and the lines are the agent's words: shown as text through `visibly` (`apps/web/src/pages/dialogs/ToolApproval.tsx:36`), never as markup, and with no `untrusted` frame, as drawn (8.6).
- **A document with no line to show** (its version for people unchanged, only its twin changed) shows under its name "Only the version for the team changes." (`docChangesTeamOnly`).
- **The buttons.** Approve sends `{ command: "folder_doc_decide", body: { proposals, decision: "approve", note? } }` with every proposal of the card, the note trimmed and left out when empty; Send back with blank notes sends nothing and says "Write what {name} should change first." (`docChangesNeedNotes`) under the box, and otherwise sends `decision: "return"` with the note. Both go through `useCommand` (`apps/web/src/pages/dialogs/StartSprint.tsx:12`), whose `again` asks the query again, and a refusal shows its words under the buttons. `refusals.ts`'s `WORDS` gains, keys `refuseFolderDocDecided`, `refuseFolderDocSuperseded`, `refuseUnknownFolderDoc`, `refuseFolderDocMixed`, `refuseFolderDocReasonNeeded`, `refuseFolderDocBusy`, `refuseFolderDocLink`, `refuseFolderDocWaiting` in `en.ts`, in that order: `folder_doc_decided` "These changes were decided already."; `folder_doc_superseded` "A newer version of these changes is waiting."; `unknown_folder_doc` "These changes are no longer there."; `folder_doc_mixed` "These changes come from more than one agent."; `folder_doc_reason_needed` "Write what to change before sending it back."; `folder_doc_busy` "One of these files has changes of yours that are not committed. Commit or undo them, then approve."; `folder_doc_link` "One of these files is a link, which Catervas does not write through."; `folder_doc_waiting` "An earlier change to one of these files is not in your project yet. Approve once it is.".
- **The name** on the card and its strings is `agent?.displayName ?? card.agentId`.
- **A folder change waiting under `manual`** (B1): `waiting.list` gains, only under `manual` as tasks' `integration` rows (`waiting.rs:465`), one `integration` row per folder change neither integrated nor escalated in step 03's fold, built in `gates.rs` beside `design_reviews_waiting` (`gates.rs:78`) by `tools::folders::integration_rows`: `{ kind: "integration", folder_change, agent_id, title, line }` with no `task_id`; `title` the documents' plain names ("Roadmap", "Product description", else the path within `docs/catervas/`, joined by " and ") then, for an approval, " from the sprint <n> review" (its proposals' sprint), and for a write " by <name>"; `line` "You approved it; it waits for you to add it" or "<name> wrote it; it waits for you to add it". `rpc.schema.json`'s row (`:2669`) no longer requires `task_id`, gains `folder_change` (integer from 1) and `anyOf` `[{ required: [task_id] }, { required: [folder_change] }]`. On Today the same `WaitingRow` (`Today.tsx:458`) and title (`waitingIntegration`, "Add {title} to your project"), its "Add" (`waitingAdd`) a button sending `{ command: "folder_change_integrate", body: { change } }` through `useCommand` when the row has `folderChange`, in place of the link to `/tasks/<id>/accept`; the row's key and title id use `folder-<n>` when it has no task. The Rust `Waiting` (`waiting.rs:78`) is unchanged, and the command line's lines do not list these rows.
- **On Today** (`apps/web/src/pages/Today.tsx:157`): the query asked beside `waiting.list`; each card an item of the "Waiting on you" list after every waiting row, and counted in its heading (`waitingTitle`, `:250`); with cards and no waiting row, "Nothing waits on you." (`waitingNone`, `:253`) is not shown. On a phone (`@media not (min-width: 640px)`, as `Today.module.css:197`) the two buttons span the width, one under the other, and the policy's note goes under them.
- **The team band.** `activity` (`crates/store/src/activity.rs:70`) gives an agent with no waiting row and a pending proposal the state `waiting` and the line "Waiting on you: product plan changes", with no task, after the waiting rows' check (`:148-159`).
- **What moved.** `moved_since` (`activity.rs:291`) gains, for the proposals recorded since `since`, one line per agent and sprint at the latest one's time, `k` the distinct paths proposed: "<name> proposed a change to the product plan from the sprint <n> review.", or "<name> proposed <k> changes to the product plan from the sprint <n> review.", each ending with its full stop as drawn. Only the product's documents are proposed (step 03), so "the product plan" is exact.
- **A task the owner accepts.** A task (not an epic, which the human accepts already) whose `allowed_paths` may change a document of `OWNER_ACCEPTED` waits for the owner's acceptance after its review passes, whoever its assignee is. `may_change_an_owner_document(allowed_paths)` in `catervas_core::folders` is true when either path passes `check_allowed_paths` (`crates/core/src/governor/paths.rs:45`) against them, the Definition of Done's own test of a diff (item 2), so a task whose paths cannot reach the spec or the roadmap cannot change them either; or when a glob does not compile, failing closed. `result_needs_the_human(contract)` (`crates/core/src/governor/done.rs`) is `requires_human_acceptance(contract)` or that, for a task; it replaces `requires_human_acceptance` in `result_awaits_human` (`done.rs:122`), in the Definition of Done's `human_accepted` (`:397`, whose message for such a task is "a task that may change the product's spec or roadmap is accepted by the human, and the human has not accepted"), and in `verify.rs:120`'s wait. So `waiting.list`'s acceptance row (`waiting.rs:432-460`), the Gate page, `catervas accept`'s `accept_result` (`crates/runtime/src/orchestrator/human.rs:1456`) and `send_result_back` (`:1555`) take it unchanged. `requires_human_acceptance` itself is unchanged: the contract's approval before work (`crates/runtime/src/transitions.rs:701`, `crates/core/src/governor/transition.rs:485`, `:545`) is not asked again. Rejected: a readiness rule asking such a contract for a `human` criterion, which the Product Manager can forget and which then refuses the plan; and reading the diff at acceptance, which the waiting list and the human's commands do not read. Residual: a task whose paths could reach the spec but changed only notes waits for the owner too; narrower paths avoid it. This replaces, for these tasks, 01b's "until step 06 the Product Manager accepts its own docs task"; step 06 shows the product plan's task as a document to approve.
- **Tests**: component tests (vitest, `apps/web/src/pages/DocumentChanges.test.tsx`, `Today.test.tsx`) with `answerQuery` (`apps/web/src/test/render-app.tsx:55`) and every sent body held to `command.schema.json` by `refusedBy` (`apps/web/src/test/schema.ts:121`), as step 10e's dialog test does; the daemon's query by an integration test in `gates.rs`. No e2e spec: the harness replays recorded sessions, and a sprint review that proposes is step 07's to record; step 08's live check runs the card in the web app.

## File map

```
crates/core/src/folders.rs                                   modifies: changed_lines (Task 1); may_change_an_owner_document (Task 2)
crates/core/src/governor/done.rs                             modifies: result_needs_the_human and its three uses (Task 2)
crates/runtime/src/orchestrator/verify.rs                    modifies: the wait for the human (Task 2)
crates/runtime/src/orchestrator/rules.rs, crates/store/src/waiting.rs   tests (Task 2)
crates/runtime/src/tools/folders.rs, crates/runtime/src/daemon/gates.rs   modifies: proposals_list, the query (Task 3)
docs/schemas/rpc.schema.json                                 modifies: the query and its result (Task 3)
crates/store/src/activity.rs                                 modifies: the band's line, the moved line (Task 3)
apps/web/src/pages/DocumentChanges.{tsx,module.css,test.tsx} creates (Task 4)
apps/web/src/pages/{Today.tsx,Today.module.css,Today.test.tsx}   modifies (Task 4)
apps/web/src/strings/en.ts, apps/web/src/app/refusals.ts     modifies (Task 4)
apps/web/src/test/folderDocs.ts                              creates: the card the tests answer with (Task 4)
docs/SPEC.md, docs/design/catervas-folders.md, docs/plans/project-plan.md   modifies (Task 5)
```

## Interfaces

Consumes: `OWNER_ACCEPTED`, `folder_docs`, `FolderDocProposal`, `ProposalState`, `tools::folders`, `folder_doc_decide` (step 03, not yet executed); `check_allowed_paths` (`paths.rs:45`); `requires_human_acceptance`, `result_awaits_human` (`done.rs:114`, `:122`); `Git::file_at`, `integration_branch` (`catervas-store`); `activity`, `moved_since` (`catervas-store`); `query` (`gates.rs:232`); `useQuery` (`apps/web/src/app/store.ts:29`), `useCommand`, `visibly`, `Avatar`, `commandSaid`; all on main except step 03's.

Produces:

```rust
// catervas_core::folders
pub enum LineChange { Same, Added, Removed }
pub struct ShownLine { pub change: LineChange, pub text: String }
pub fn changed_lines(before: &str, after: &str) -> Vec<ShownLine>;
pub fn may_change_an_owner_document(allowed_paths: &[String]) -> bool;
// catervas_core::governor::done
pub fn result_needs_the_human(contract: &TaskContract) -> bool;
// catervas_runtime::tools::folders
pub(crate) fn proposals_list(deps: &ToolDeps, team: &Team) -> Result<Value, StoreError>;
pub(crate) fn integration_rows(deps: &ToolDeps, team: &Team) -> Result<Vec<Value>, StoreError>;   // empty unless manual
```

```ts
// apps/web/src/pages/DocumentChanges.tsx
export type Integration = "auto_merge" | "pull_request" | "manual";
// Today.tsx's Waiting: taskId becomes optional and gains folderChange?: number
export type DocumentChangesCard = { sprintId: string; agentId: string; summary: string;
	documents: { proposal: number; path: string; lines: { change: "same" | "added" | "removed"; text: string }[] }[] };
export function DocumentChanges(props: { card: DocumentChangesCard; integration: Integration; name: string; avatar?: AvatarKey;
	again: () => void }): JSX.Element;
```

## Tasks

### Task 1: The changed lines

Files: modified `crates/core/src/folders.rs` (`changed_lines`, its tests).

- `shows_the_roadmaps_changes_as_drawn` — before `# Roadmap`, `## Done`, `## Now`, `- Pie pre-orders. Customers order Thanksgiving pies ahead and collect them on 25 or 26 November.`, `## Next`, `- Gift cards. Buy a card online, spend it in the shop.`, `- Opening hours on every page.` (blank lines between); after with `- Pie pre-orders. 41 pies ordered in the first week.` under Done, `- Gift cards. …` under Now, `- Christmas orders. Stollen, yule logs and mince pies, collected from 19 to 24 December.` before Opening hours under Next: exactly `Same ## Done`, `Added - Pie pre-orders. 41 …`, `Same ## Now`, `Removed - Pie pre-orders. Customers …`, `Added - Gift cards. …`, `Same ## Next`, `Removed - Gift cards. …`, `Added - Christmas orders. …`, `Same - Opening hours on every page.`. RED: no function.
- `shows_the_heading_above_a_change` — before `# Little Oak Bakery`, `What we sell.`, `## Ordering ahead`, `Customers pick a day and a time to collect their order.`; after with `Pre-orders close 3 days before collection, so the kitchen can plan.` at the end: exactly `Same ## Ordering ahead`, `Same Customers pick …`, `Added Pre-orders close …`. RED.
- `shows_a_new_document_whole_and_an_unchanged_one_empty` — before empty: every non-blank line `Added`, in order; equal texts, or texts that differ in blank lines or a `\r` alone: nothing. RED.
- `shows_a_change_too_big_to_compare_whole` — a middle of 1,001 old and 1,001 new distinct lines: the 1,001 removed lines, then the 1,001 added, with the common first and last lines around them. RED.

- [ ] `feat(core): show a document's changes line by line`

### Task 2: The owner accepts a task that may change the product plan

Files: `folders.rs` (`may_change_an_owner_document`), `done.rs` (`result_needs_the_human`, `result_awaits_human`, `human_accepted`, tests), `verify.rs:118-124`; tests in `rules.rs` and `waiting.rs`.

- `knows_which_paths_may_change_the_product_plan` (`folders.rs`) — true for `[docs/catervas/product/**]`, `[docs/catervas/product/*.md]`, `[docs/catervas/product/spec.md]`, `[docs/catervas/product/{spec,notes}.md]`, `[**]`, `[docs/**]` and `[docs/[]` (a glob that does not compile); false for `[docs/catervas/product/reports/**]`, `[docs/catervas/product/spec.agent.md]`, `[docs/catervas/product]`, `[Docs/Catervas/Product/spec.md]`, `[src/**]` and `[]`, each as `check_allowed_paths` would let a diff through. RED: no function.
- `the_human_accepts_a_task_that_may_change_the_product_plan` (`done.rs`) — a `verifying` task with `allowed_paths: [docs/catervas/product/**]`: `result_awaits_human` is true, `requires_human_acceptance` false, `failed_rules` is `[HumanAccepted]` with exactly the message above, and passes with `human_accepted`; with `[docs/catervas/product/reports/**]` `result_awaits_human` is false and no `HumanAccepted` fails; an epic and a `high` risk task keep their messages. RED: `result_awaits_human` false.
- `leaves_a_task_that_may_change_the_product_plan_for_the_human` and `accepts_it_after_the_human` (`rules.rs`, integration, as `leaves_a_high_risk_task_for_the_human` and `accepts_a_high_risk_task_after_the_human` at `:3889` and `:3913` with `allowed_paths: [docs/catervas/product/**]` in place of `risk: high`): after the review the tick is idle with no acceptance session; after `HumanAccept` the Product Manager's session accepts it. RED: the Product Manager accepts it at once.
- `lists_a_product_plan_task_for_the_owner_to_accept` (`waiting.rs`, beside `lists_a_result_once_its_review_passed` at `:1048`) — such a task whose review passed is listed `acceptance`; with `reports/**` it is not. RED.

- [ ] `feat(core): let the owner accept a task that may change the product plan`

### Task 3: The query, the band and what moved

Files: `tools/folders.rs` (`proposals_list`, `integration_rows`), `gates.rs` (the arm, the rows in `waiting.list`, tests), `rpc.schema.json`'s waiting row (`:2669`, as Decisions), `rpc.schema.json` (the name in the query list at `:40-75`; `folderDocProposalsListQuery`, `{}` params; `folderDocProposalsListResult`, its `integration` (`auto_merge`, `pull_request`, `manual`) and its `cards` with `sprint_id` `^S[1-9][0-9]{0,5}$`, `agent_id`, `summary`, `documents` of `proposal` (integer from 1), `path` and `lines` of `change` (`same`, `added`, `removed`) and `text`, all required, no other property), `activity.rs` (the band's line, the moved line, tests).
Consumes: `changed_lines` (Task 1).

- `lists_the_changes_waiting_for_the_owner` (`gates.rs`, integration) — `pm` ("Mira") proposed the roadmap and then the spec in S4, and an older roadmap proposal, a returned one and one of S3 approved are in the log too: `folder_doc_proposals.list` answers one card `{ sprint_id: "S4", agent_id: "pm", summary: "<roadmap's summary> <spec's summary>", documents: [roadmap, spec] }`, each `proposal` its seq, the roadmap's `lines` `changed_lines` of the committed roadmap and the newer proposal's text, the spec's, which git does not have yet, every line `added`; `integration` is the team's policy, `pull_request` for a team that set it; with nothing pending, `cards` is `[]`. RED: unknown query.
- `lists_a_folder_change_waiting_to_be_added_under_manual` (`gates.rs`, integration) — under `manual`, after an approval of the roadmap and the spec from S4 (change 1) and a write of `delivery/cadence.md` by `sm` (change 2): `waiting.list` holds two `integration` rows with `folder_change` 1 and 2, no `task_id`, titles "Roadmap and Product description from the sprint 4 review" and "delivery/cadence.md by Sol", lines "You approved it; it waits for you to add it" and "Sol wrote it; it waits for you to add it"; once change 1 is integrated, or escalated, its row is gone; under `auto_merge` there is none. RED: no rows.
- `an_agent_whose_changes_wait_waits_on_the_owner` (`activity.rs`) — `pm` with a pending proposal and no waiting row: state `waiting`, line "Waiting on you: product plan changes", no task; once the owner decides, `pm` is idle. RED.
- `says_who_proposed_changes_to_the_product_plan` (`activity.rs`) — two proposals by `pm` in S4 since `since` and one before it: one line "Mira proposed 2 changes to the product plan from the sprint 4 review." at the later one's time; two proposals of one path give "Mira proposed a change to the product plan from the sprint 4 review.". RED.

- [ ] `feat(runtime): list the product plan changes waiting for the owner`

### Task 4: The card on Today

Files: created `DocumentChanges.tsx`, `DocumentChanges.module.css`, `DocumentChanges.test.tsx`, `apps/web/src/test/folderDocs.ts` (the mockup's card: Mira, S4, its summary, the roadmap's nine lines and the description's three, proposals 7 and 8); modified `Today.tsx`, `Today.module.css`, `Today.test.tsx`, `en.ts` (the strings of Decisions), `refusals.ts`.

- `shows_the_changes_line_by_line` (`DocumentChanges.test.tsx`) — the card's region is named "Product plan changes to approve"; it holds "From Mira, after the sprint 4 review", the summary, the legend, then "Roadmap" and "Product description" in that order; the roadmap's list items read, in order, "Done", "Added: Pie pre-orders. 41 pies ordered in the first week.", "Now", "Removed: Pie pre-orders. …", "Added: Gift cards. …", "Next", "Removed: Gift cards. …", "Added: Christmas orders. …", "Opening hours on every page."; no text holds `## ` or a leading `- `; a summary holding U+202E shows it written out (`showsWhatItHides`, `apps/web/src/test/hidden.ts`). RED: no component.
- `approve_sends_every_proposal_of_the_card` — Approve sends one `command` whose body is `{ proposals: [7, 8], decision: "approve" }`; with notes "  Thanks  ", `note: "Thanks"`; each body `refusedBy("folderDocDecideBody", …)` is empty. RED.
- `send_back_needs_notes` — with the box blank, Send back sends nothing and shows "Write what Mira should change first."; with "Keep gift cards in Now" it sends `{ proposals: [7, 8], decision: "return", note: "Keep gift cards in Now" }`, held to the schema. RED.
- `says_what_approving_does_under_the_policy` — the note under the buttons is "Nothing changes until you approve." under `auto_merge`, and `docChangesPullRequest`'s and `docChangesManual`'s sentences under `pull_request` and `manual`; none says "at once". RED.
- `adds_a_folder_change_from_its_row` (`Today.test.tsx`) — an `integration` row with `folderChange: 1` and no `taskId` reads "Add Roadmap and Product description from the sprint 4 review to your project", and its "Add" is a button that sends `{ command: "folder_change_integrate", body: { change: 1 } }`, held to the schema by `refusedBy("folderChangeIntegrateBody", …)`; a task's `integration` row still links to `/tasks/<id>/accept`. RED.
- `shows_a_document_whose_twin_alone_changes` — a document with `lines: []` shows "Only the version for the team changes.". RED.
- `a_refusal_shows_its_words` — a command refused `folder_doc_busy: …` shows "One of these files has changes of yours that are not committed. Commit or undo them, then approve.". RED.
- `counts_and_lists_the_document_changes` (`Today.test.tsx`) — `waiting.list` with one question and `folder_doc_proposals.list` with the card: the heading reads "Waiting on you (2)" and the list holds the card after the question; with `cards: []` it reads "Waiting on you (1)"; with `waiting: []` and the card it reads "Waiting on you (1)" and "Nothing waits on you." is absent. RED: the query is not asked.

- [ ] `feat(web): approve the product plan's changes on Today`

### Task 5: Spec

`docs/SPEC.md`: 4.2 (the owner approves the team's changes to the product plan from Today); 5.1 and 6.1 (a task that may change `spec.md` or `roadmap.md` is accepted by the owner after its reviewer's review, replacing 01b's "until step 06" for those tasks); 5.4 (item 5 and "What a human gate shows" gain such a task, with `may_change_an_owner_document`'s rule and its residual; the sprint waits while such a task waits for the owner, as for a `high` risk task, since a sprint ends only when its tasks are accepted, `finished_sprint`, `rules.rs:329`); 5.14 (a folder change waiting under `manual` is listed on Today as a task awaiting integration); 5.7 (the card: its query, one per sprint review and agent, the lines shown, Approve and Send back, the note by the team's integration policy, the band's line and the moved line; a proposal is not in `waiting.list`); 8.6 (the summary and the lines are the agent's words, shown as text); F3 (Today's card); the revision after step 03's, 0.88. `docs/design/catervas-folders.md`: the steps table's row 06's Delivers becomes, exactly, "the chat's button; `product_plan_first`; the product plan's task, which the owner accepts (03b), shown on Today as a document to approve, its version for people first", and row 04b gains "Open in Files" on the card. `docs/plans/project-plan.md`: row 03b, as step 03 wrote it, marked planned.

- [ ] `docs(spec): record approving the product plan's changes on Today`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Tasks 2 and 3 name integration tests
# expected: xtask check: ok
```
