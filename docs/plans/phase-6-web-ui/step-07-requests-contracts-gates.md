# Phase 6, step 07: Requests, contracts, and human gates

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.2, 5.4 (what a human gate shows), 5.7, 5.11, 5.13, 5.16, F4, F14
Depends on: steps 01 to 06 of this phase
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Today is the home page. It shows the team band (each agent's face and what it is doing), the request box, "Waiting on you", and "What moved". Spec 5.16's working loop runs through it:
- The user asks for something in plain words. The triage is shown and can be overruled.
- The Product Manager's questions are answered, with its suggested choices.
- The plan is read as a letter from the Product Manager, with Farik's checks.
- The plan is edited in a plain-language editor that checks it as the user types, or approved, or sent back.
- Finished work is accepted, or sent back with a note, on a page that leads with the assignee's and the reviewer's summaries, then Farik's checks, with the code changes one click away.
- An escalation is resolved from a "needs your help" page.

The runtime now requires the summaries these pages lead with (spec 5.4).

Out of scope, and where each goes:
- the board and task detail (step 08);
- the channel preview on Today (step 09);
- one-on-ones (phase 8).

## Decisions

- **The summary for the human** (spec 5.4).
  - The contract gains `summary`: plain language for the user, 20 to 600 characters. It is a content field (`FIELDS_OF_THE_CONTENT` becomes 15), written by the Product Manager or the human.
  - A new readiness rule, `SummaryPresent`, fails "the plan has no summary for the user; write two or three plain sentences they can decide on".
  - A completion or review note must open with a summary: its first paragraph, up to the first blank line, of 20 to 600 characters.
  - `farik_write_note` refuses one that does not, with `summary_missing: open the note with two or three plain sentences for the user, then a blank line`.
  - Farik's own progress notes are exempt.
  - The recorded transcripts that write contracts or completion and review notes are updated to carry summaries. The Product Manager's and the reviewers' skills (`crates/roles/roles/*/skills`) say so in one line each.
- **Sending back.**
  - A new command, `human_send_back { task_id, subject: contract | result, message, failed_criteria?: [criterion_id] }`.
  - For a contract waiting for approval, it resolves the escalation to `refining` with the message. This is what `farik resolve <id> refining` does today.
  - For a result that is `verifying` and waiting on the human, it records the human's rejection, `task.transitioned verifying → rejected` with `requested_by: human` and the message as the reason. That needs a new transition-table row, `verifying → rejected` by the human when the result waits on the human's acceptance, which spec 5.2 gains.
  - The rejection counts as a try, as a reviewer's does.
  - `farik send-back <task> <message>` is the command line's form.
- **Questions with choices.**
  - `question.asked` gains optional `choices: [string]`, at most 4, each 1 to 120 characters.
  - The ask tool, `farik_ask_question`, takes them optionally.
  - "Let <agent> decide" sends the answer "Decide as you think best, and say what you chose."
- **More tries** (HelpNeeded, reason `iterations`).
  - `escalation_resolve` gains optional `extra_tries: 1..5`.
  - The iteration gate allows `max_iterations` plus the sum of `extra_tries` resolved on that task. The core's `ReadinessContext` or `AssignmentInput` gets the sum from the log.
  - The page's choices by reason:
    - iterations: "Give 2 more tries" (`in_progress`, `extra_tries: 2`), "Ask <PM> to change the plan" (`refining`), "Pause the task" (`blocked`), "Cancel the task" (`cancelled`);
    - other reasons: "Carry on" (`in_progress` or the status before), "Change the plan", "Cancel".
- **Filing from the web.**
  - `request_from_brief` moves from the CLI to `farik_store::requests`, unchanged, and the CLI calls it there.
  - The method `request.file { text }` answers `{ task_id }`.
  - The title is the first line, cut to 80 characters. The intent is the whole text, at least 20 characters, refused with "say a little more: at least 20 characters".
- **Checking a draft as the user types.**
  - `Transitions::context` is split so that `readiness_context(files, log, team, contract)` takes the contract.
  - The query `contract.check { task_id, contract }` answers `{ failures: [{ rule, message, plain }] }` without saving.
  - `plain` is a user-facing sentence per `ReadinessRule`, from a `plain_readiness(rule) -> &'static str` table in core, pinned by a test that lists every variant.
  - The page calls it 400 ms after typing stops.
- **Saving an edit.**
  - The method `contract.save { task_id, contract }` writes content fields as the human, through the existing contract write gate (spec 5.11: the human may write content, and lock).
  - It appends `contract.written { written_by: human }` and answers `{ saved, failures }`.
  - Lock and unlock are the existing `contract_lock` and `contract_unlock` commands.
- **Queries**, each a function in store or runtime so that the CLI and the daemon share it:
  - `waiting.list {}` answers `[{ task_id, kind: approval | acceptance | question | help | integration, agent_id, title, line }]`. It comes from `waiting()`, moved from `crates/cli/src/waiting.rs` to `farik_store::waiting`. Acceptances also cover a `verifying` task with a `human` criterion, which the CLI list missed.
  - `contract.get { task_id }` answers the contract.
  - `task.history { task_id }` answers the task's events.
  - `task.diff { task_id }` answers `{ diff, files, added, removed }`. It uses `diff_of`, moved from `crates/cli/src/show.rs` to `farik_store::diff`. For an epic it answers its tasks' integrated diffs joined in id order.
  - `task.checks { task_id }` answers `[{ criterion_id, text, passed, evidence }]` from `criterion.recorded` since the task last entered `verifying`, or, for a contract, the readiness results.
  - `questions.list { task_id? }` answers `[{ question_id, task_id, asked_by, question, choices, answer? }]`.
  - `team.activity {}` answers `[{ agent_id, state: working | resting | waiting_on_you | paused | idle, line, task_id?, until? }]`. It comes from `farik_store::activity(log, projections, team, now)`:
    - `working` means a `session.started` has no `session.ended`, and the line comes from the session's purpose and task title ("Writing the plan for …", "Building …", "Reviewing …", "Sizing a request", "Planning the sprint", "Posting the standup");
    - `resting` means `agent.slept` with `until` in the future ("Resting until <time>. It reached its usage limit.");
    - `waiting_on_you` means the agent's task is in `waiting.list`;
    - `paused` means the agent's status is paused, or the team is paused.
  - `moved.since { since }` answers the transitions, integrations and acceptances since `since` as plain lines. Today asks for the last 24 hours.
- **Pages** follow the approved mockups: `Main` (Today), `RequestFiled`, `Questions`, `AnswerQuestion`, `ApprovePlan`, `PlanEditor`, `Gate`, `SendBack`, `HelpNeeded`, `Phone`, and `PhoneGate`. Routes:
  - `/` is Today (replacing the redirect to `/events`);
  - `/requests/:id`;
  - `/tasks/:id/questions`;
  - `/tasks/:id/plan` (approve) and `/tasks/:id/plan/edit`;
  - `/tasks/:id/accept` (the gate);
  - `/tasks/:id/help`.

  A Waiting row links to its page. The rail gains Today first, and Events stays under Settings as "Events (for testing)".
- **Differences from the mockups.**
  - Today's "In the channel" block waits for step 09.
  - The sprint line ("Sprint 2 is running: 4 of 7 tasks done") shows only when a sprint is open, from `tasks.list`.
  - RequestFiled's "R-7" is the task id (`FRK-7`).
  - ApprovePlan's "Builders" row is the contract's `assignee_role` in words.
  - "See the plan as written" shows the contract as YAML, read-only.
  - PlanEditor labels every field in plain words, with the key in small mono text as the mockup has it.
  - The step 04 minor finding, the rail stopping at the window's height, is fixed here: the rail is `position: sticky; height: 100dvh`.
- **The diff's size line** ("3 files, +142 −18") comes from `task.diff`'s counts. The `DiffView` stays collapsed until "See the code changes" is pressed (spec 5.4).
- **Tests.**
  - Rust unit and route tests, and Vitest with axe on each page.
  - Three Playwright journeys run on recorded transcripts through `farik-e2e-serve`'s name map, which gains the needed names:
    - `request.spec.ts`: file a request, see it sized, answer the Product Manager's question, and reach the plan;
    - `approve.spec.ts`: open the plan, see the summary and checks, edit a field with its live check, approve, and see the task leave Waiting;
    - `accept.spec.ts`: open the finished task, read both summaries and the checks, open the diff, send back once with a note, then accept, and see `human.accepted` in the log.

## File map

```
docs/schemas/{task-contract,event,command,rpc}.schema.json                  modifies (T1, T2)
crates/core/src/{contract,governor/readiness.rs,governor/transition_table.rs,governor/gates.rs,governor/plain.rs} (+ tests)  modifies / creates (T1)
crates/protocol/src/{command.rs,event.rs,rpc.rs}                           modifies (T1, T2)
crates/runtime/src/{tools/work.rs,tools/questions.rs,orchestrator/human.rs,transitions.rs,daemon/web.rs}, recorded/transcripts/*.jsonl  modifies (T1, T2)
crates/roles/roles/{product_manager,architect,software_developer,marketing_specialist}/skills/*/SKILL.md  modifies (T1)
crates/store/src/{requests.rs,waiting.rs,diff.rs,activity.rs,lib.rs} (+ tests)  modifies / creates (T2)
crates/cli/src/{contract_new.rs,waiting.rs,show.rs,lib.rs}, crates/cli/src/bin/farik-e2e-serve.rs  modifies (T2, T6)
packages/protocol-client/src/client.ts                                      modifies (T2)
apps/web/src/pages/{Today,RequestFiled,Questions,AnswerQuestion,ApprovePlan,PlanEditor,Gate,HelpNeeded}.tsx (+ css, tests), shell/Shell.tsx, app/App.tsx, strings/en.ts  creates / modifies (T3, T4, T5)
apps/web/e2e/{request,approve,accept}.spec.ts                               creates (T6)
docs/SPEC.md (5.2, 5.4, 5.7, 5.11, 8.5), docs/plans/project-plan.md        modifies (T7)
```

## Interfaces

```rust
TaskContract::summary: Option<String>   ReadinessRule::SummaryPresent   pub fn plain_readiness(rule: ReadinessRule) -> &'static str;
Command::HumanSendBack { task_id: TaskId, subject: Subject, message: String, failed_criteria: Vec<String> }
EscalationResolveBody.extra_tries: Option<u8>   QuestionAskedBody.choices: Vec<String>
pub fn farik_store::requests::request_from_brief(title: &str, brief: &str, max_cost_usd: f64) -> Result<Value, String>;
pub fn farik_store::waiting::waiting(projections: &Projections, log: &EventLog, team: &Team) -> Result<Vec<Waiting>, StoreError>;
pub fn farik_store::diff::diff_of(git: &Git, contract: &TaskContract, history: &[FarikEvent]) -> Result<TaskDiff, StoreError>;
pub fn farik_store::activity::activity(log: &EventLog, projections: &Projections, team: &Team, now: DateTime<Utc>) -> Result<Vec<AgentActivity>, StoreError>;
pub fn readiness_context(/* files, log, team */, contract: &TaskContract) -> Result<ReadinessContext, TransitionError>;
```

RPC:
- queries: `waiting.list`, `contract.get`, `contract.check`, `task.history`, `task.diff`, `task.checks`, `questions.list`, `team.activity`, `moved.since`;
- methods: `request.file`, `contract.save`.

## Tasks

### Task 1: Summaries, send back, choices, extra tries

- `refuses_a_plan_without_a_summary`: the readiness rule's message, and the `plain` sentence.
- `plain_readiness_covers_every_rule`: every `ReadinessRule` variant has a non-empty sentence.
- `refuses_a_note_without_an_opening_summary`: completion and review notes, with the exact refusal, and a progress note is accepted.
- `sends_a_contract_back_to_refining` and `sends_a_result_back_as_a_rejection`: the events, the try counted, and a refusal when the result does not wait on the human.
- `records_question_choices`: up to 4, and their bounds.
- `grants_extra_tries`: after `extra_tries: 2`, the iteration gate allows two more tries and then escalates again.
- The updated transcripts replay with summaries, and every existing orchestrator test passes.

- [ ] `feat(runtime): require the human's summaries, and let the human send work back`

### Task 2: The shared queries and the moves to the store

- `files_a_request_from_plain_words`: `request.file` makes a draft with the title and intent, and too short a text is refused.
- `checks_a_draft_without_saving`: `contract.check` with a changed intent answers failures, and the file is unchanged.
- `saves_the_human_edit`: `contract.save` appends `contract.written { written_by: human }`, and a locked contract stays the human's.
- `lists_what_waits_on_the_human`: an approval, an acceptance with a `human` criterion, a question, a help request, and an integration; the CLI's `farik waiting` output is unchanged for the old cases.
- `answers_the_task_diff_and_checks`: counts and files for a task, and the joined children for an epic.
- `derives_each_agents_activity`: working (the purpose's line), resting (until), waiting on you, paused, idle.
- `lists_questions_with_choices`, and `says_what_moved_since`.

- [ ] `feat(runtime): answer the working loop's queries, and file and edit contracts from the browser`

### Task 3: Today

- `shows_the_team_band`: one entry per agent, with the avatar, the name, the role tag, and the activity line; `waiting_on_you` uses the waiting word.
- `sends_a_request_to_the_team`: the text box and "Send to the team" call `request.file`, then open `/requests/:id`.
- `lists_what_waits_on_you_with_links`: each kind links to its page, and the count sits in the heading.
- `says_what_moved`: the lines from `moved.since`.
- `keeps_the_rail_the_full_height`: the rail's computed position is sticky.

- [ ] `feat(web): add Today, with the team band, the request box and what waits on you`

### Task 4: The request, the questions, the plan

- `shows_the_request_and_its_size`: the triage reason, the two cards, and "Change this to a small request" sending `request_triage`.
- `answers_a_question_with_a_choice_or_words`: a choice, free text, or "Let <agent> decide" sends `question_answer`; the next question stays locked until the current one is answered.
- `reads_the_plan_as_a_letter`: the summary is signed by the Product Manager, followed by the parts with their done-when lines, "Not in this plan", and Farik's checks; Approve sends `human_accept { subject: contract }`.
- `edits_the_plan_with_live_checks`: typing in the intent calls `contract.check` after 400 ms and shows the plain sentence; Lock sends `contract_lock`; Save calls `contract.save`.
- `asks_for_changes`: "Ask for changes" sends `human_send_back { subject: contract }` with the note.

- [ ] `feat(web): add the request, question and plan pages`

### Task 5: The gates and help

- `leads_with_the_summaries_then_the_checks`: the assignee's and the reviewer's opening paragraphs, each signed; the checks; and the diff collapsed until pressed, with its size line.
- `accepts_the_work`: sends `human_accept { subject: result }`; an epic requires the message field.
- `sends_the_work_back_with_a_note`: the dialog lists the criteria and "Something else", the note is required, and the dialog says which try this is.
- `offers_the_choices_for_the_reason`: iterations offers the four choices with their resolve bodies; other reasons offer theirs.
- `fits_a_phone`: at 360 px, PhoneGate's buttons are visible without scrolling sideways.

- [ ] `feat(web): add the acceptance gate, sending back, and the help page`

### Task 6: The journeys (Playwright)

- `request.spec.ts`, `approve.spec.ts`, `accept.spec.ts`, as in the Tests decision, each with screenshots at 360 and 1280 px.

- [ ] `test(web): walk a request from asking to acceptance through the real server and browser`

### Task 7: Spec and plan

- Spec 5.2: the human's `verifying → rejected` row.
- Spec 5.4: the summary field and the opening-summary rule.
- Spec 5.7: extra tries.
- Spec 5.11: `summary` as content.
- Spec 8.5: the event and command changes.
- The project plan's step 07 line.

- [ ] `docs(spec): the summaries the gates lead with, sending back, choices and extra tries`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/web: step 06's landed count plus 15 (T3 5, T4 5, T5 5); playwright: step 06's 4 plus 3 = 7 passed;
#   last line: xtask check: ok
```
