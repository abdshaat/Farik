# Phase 6, step 08: Today and the gate pages

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.4 (what a human gate shows), 5.7, 5.11, 5.13, 5.16, 10, F4, F14
Depends on: steps 01 to 07 of this phase (step 07 supplies every query, method and command these pages use)
Readiness confirmed by: fresh-session reviewer, 2026-09-29, round one: not ready (two blockers, seven planner decisions, three interface gaps now added to step 07). All are settled below. Round two found them settled apart from two journey transcripts whose agent ids did not match the team; step 07 now promises matching ones, and the lists use them.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Today becomes the home page. It shows:
- the team band: each agent's face, name, role tag, and what it is doing;
- the request box;
- "Waiting on you";
- "What moved since yesterday".

From Today the user can go through the working loop in the browser:
- file a request, see how it was sized, and resize it;
- answer the Product Manager's questions, from its choices or in their own words;
- read the plan as a letter with Farik's checks, then approve it, send it back, or edit it in a plain-language editor that checks as the user types;
- accept finished work or send it back with a note, on a page that leads with the two summaries and the checks, with the code changes one click away;
- answer a request for help with the choices that fit its reason.

Out of scope: the board and task detail (step 09); the channel preview on Today (step 10).

## Decisions

- **Where `/` goes** is one function, `landing(status: ServeStatus): string`, in `src/app/landing.ts`, with its own test:
  - with `project_root` null, `/setup/computer`, or `/setup/project` when `take_on_error` is set;
  - otherwise with `setup_pending`, `/setup/scan`;
  - otherwise Today.

  Nothing renders, and no project query is sent, until `serve.status` has arrived. During setup, every other path (deep links included) redirects to `landing`'s answer.
- **Pages** follow the founder-approved mockups: `Main` (Today), `RequestFiled`, `Questions`, `AnswerQuestion`, `ApprovePlan`, `PlanEditor`, `Gate`, `SendBack`, `HelpNeeded`, `Phone`, and `PhoneGate`.
- **Routes:**
  - `/` is Today. It replaces step 04's redirect to `/events`, except that step 05's and 06's setup redirects still come first.
  - `/requests/:id`
  - `/tasks/:id/questions`
  - `/tasks/:id/plan` (read and approve) and `/tasks/:id/plan/edit`
  - `/tasks/:id/accept`
  - `/tasks/:id/help`

  Each Waiting row links to its route by `kind`, with this button word:
  - approval: `plan`, "Review";
  - acceptance: `accept`, "Review";
  - question: `questions`, "Answer";
  - help: `help`, "Help";
  - integration: `accept`, "Add". There the page shows only "Add to project" (`task_integrate`), with Accept and Send back hidden.

  AnswerQuestion's layout is the one question's view of `/tasks/:id/questions` when the task has a single open question.
- **The rail's places:** Today, Team (step 06 shipped it), then Settings, with Events kept at the bottom of Settings as "Events (for testing)". Steps 09 and 10 add Board, Channel, and Costs where the mockups put them.
- **Today.**
  - **Team band.** It comes from `team.activity`. Each agent shows its avatar, name, role tag, and `line`, dark (`band`) in both themes.
  - **Request box.** The label is "What should the team do next?" and the hint is "<PM name> reads every request and asks you if anything is unclear." It has a `>` prompt and a blinking block cursor (the only idle motion, stopped under reduced motion). "Send to the team" calls `request.file` and opens `/requests/:id`.
  - **Waiting on you (N).** From `waiting.list`: the agent's avatar, a title line per kind, and the `line`. The titles:
    - approval: "Approve the plan for <title>";
    - acceptance: "Accept <title>";
    - question: "<agent> has a question";
    - help: "<agent> needs your help";
    - integration: "Add <title> to your project".

    Each button's word is "Review", "Answer", "Help", or "Add".
  - **What moved since yesterday.** From `moved.since` for the last 24 hours: time and line.
  - **The sprint line** ("Sprint N is running: X of Y tasks done") appears only while `sprint.current` answers a sprint.
  - **Acceptance rows** add "All N of Farik's checks passed" when every `task.checks` row passed.
- **Request page.**
  - It shows "You asked" (the intent) and "<SM or PM> sized it as a big request / a small request", with the reason from `request.triaged`.
  - The two cards explain each size.
  - "Change this to a small request" (or big) sends `request_triage` with the reason "Changed by you". It shows only while the task is `draft` (the governor takes human triage only then). Before triage it reads "<SM or PM> is sizing your request…". A refusal shows its sentence.
  - "What happens next" lists four fixed steps for each size.
  - "About this request": sent, sized by, planned by, and the plan's id.
- **Questions.**
  - From `questions.list`. Answered questions show "You answered: …".
  - The current question shows its choices, as radio cards with each choice's hint, plus "Or say it in your own words".
  - "Send answer" sends `question_answer` with the choice's label or the text.
  - "Let <agent> decide" sends step 07's pinned text.
  - Later questions stay hidden behind "<agent> shows this once you answer question <n>".
- **Plan page.**
  - The contract's `summary`, signed "<PM name>, your Product Manager, wrote this for you".
  - "The plan in N parts": the requirements, each with the exit criteria that name it as "done when" (for a task or an epic alike; an epic is approved before it is broken down).
  - "Not in this plan": `scope.out_of_scope`.
  - "What Farik checked": `task.checks`.
  - "See the plan as written": the contract as YAML, read-only.
  - "About this plan": you asked, questions answered, risk in words, the estimate, and the builders (the assignee role in words).
  - Buttons: "Approve the plan" sends `human_accept { subject: contract }`. "Ask for changes" opens a note dialog and sends `human_send_back { subject: contract }`. "Edit the plan yourself" goes to the editor.
- **Plan editor.**
  - Fields are labelled in plain words, with the key in small mono text: intent, summary, requirements, exit criteria, out of scope, budget in dollars, risk, and allowed paths (the last two under Advanced).
  - Exit criteria can be added, each with text and one of two plain choices: "Someone on the team reviews it" (a `review` criterion whose rubric is the text) or "You decide" (a `human` criterion). "Farik runs a command" (`command` or `test`, with its command field required) is offered only with Advanced on, so a saved plan never holds a blank command, which `CommandCriteriaComplete` would refuse. No command or glob is asked outside Advanced (the phase decision).
  - "Advanced" everywhere means the Settings switch (`farik.advanced`) from step 04. The mockup's on-page "Advanced view" toggle is that switch, shown here too.
  - Typing calls `contract.check` 400 ms after the last keystroke. It shows "Ready to approve? N of M checks pass. The one left: <plain>", with M the answer's `total` and N the total minus the failures.
  - The lock banner shows either "<PM name> can still change this plan" with "Lock the plan" (`contract_lock`), or "You have locked this plan" with "Unlock" (`contract_unlock`).
  - "Save" calls `contract.save`. When it answers `back_to_refining`, the page says: "Saved. <PM> checks the plan again, then it comes back to you to approve."
- **Acceptance gate.**
  - The assignee's completion-note summary, signed "<name>, your <role>, wrote this for you".
  - The reviewer's review-note summary, signed "<name>, your <role>, reviewed it".
  - "What Farik checked".
  - "See the code changes · N files, +A −R", which opens the collapsed `DiffView`.
  - The summaries come from `task.history`'s latest completion and review `note.written`, taking each note's first paragraph.
  - "About this task": you asked, when the plan was approved, "Tries i of n" from `task.tries`, the cost, and "See the whole history" (to step 09's task detail; until step 09 lands, it goes to `task.history` shown as a list).
  - Buttons: "Accept the work" sends `human_accept { subject: result }`. For an epic a text area "What you checked" sits above the button and is required. "Send back with a note" opens the SendBack dialog.
  - **The SendBack dialog:** a checkbox per exit criterion and "Something else" (which adds nothing to `failed_criteria`); a required note; "This is try i of n. After the last, Farik stops and asks you." It sends `human_send_back { subject: result, failed_criteria }`.
  - A `review_first` refusal shows its sentence.
- **Help page.**
  - "<agent> needs your help with <title>".
  - The escalation's detail, in the agent's words.
  - "What <agent> tried": the task's progress notes.
  - The choices from `escalation.choices` as radio cards, an optional note, and the button labelled with the choice. A resolve body is sent as `escalation_resolve`, with `message` set to the note, or to the choice's label when the note is empty. "Add it now" is sent as `task_integrate`.
  - "About": waiting since, tries, "Spent $X of $Y", and the reviewer.
- **Mockup differences.** Each one is listed:
  - PlanEditor adds the summary and requirements fields, moves risk and allowed paths under Advanced, and has "Save" rather than "Approve the plan" (approval stays on the plan page, since a save returns the plan to refining).
  - HelpNeeded drops each choice's description line, "See the failing check's output", and the closing note line; step 09's task detail shows the check output.
  - Gate drops "You asked to accept risky changes…" and "Accepting adds … to your project".
  - Questions and RequestFiled drop "Why <PM> asks", the "N questions about…" heading, and "See <PM>'s questions"; the Waiting row links there instead.
  - The ids R-7, E-3 and T-14 all become the task's `FRK-n`.
  - Main's acceptance row keeps "All N of Farik's checks passed" (from `task.checks`).
  - "In the channel" waits for step 10.
  - HelpNeeded's "Pause the task" is dropped (step 07's decision).
  - "Give Theo 2 more tries" appears only for reason `iterations`.
  - Choices show their hints where the agent gave one.
- **Strings.** Every string is in `en.ts`. Status words follow `web-ui.md`'s lifecycle table, through `statusWord(status: TaskStatus, reason?: EscalationReason): string` in `src/app/words.ts`, pinned by a test over every `TaskStatus`. `escalated` with reason `approval` is "Waiting on you", any other `escalated` is "Needs your help", and `cancelled` is "Cancelled".
- **Tests.**
  - Vitest and axe for each page's logic.
  - Layout checks (phone fit, the sticky rail) run in the Playwright journeys at 360 × 780, since jsdom does not lay out.
  - The journeys start from a team written by `startServe({ team: 'pm-architect-developer' })`: Mira (Product Manager), Ada (Architect, who reviews the Developer and checks plans), and Theo (Developer), under `human_accepts_contracts: high_risk`. The recorded adapter replays transcripts in order, so each journey lists every session:
    - request: `triage_frk_1_small_by_pm`, `ask_with_choices_frk_1`, `refine_writes_task_for_theo_frk_1`, `judge_frk_1_by_architect`;
    - approve: `triage_frk_1_large`, `refine_writes_epic_frk_1`, `judge_frk_1_by_architect`, `refine_writes_epic_frk_1`, `judge_frk_1_by_architect`;
    - accept: `triage_frk_1_small_by_pm`, `refine_writes_high_risk_frk_1`, `judge_frk_1_by_architect`, the human's approval, `plan_assigns_frk_1_to_theo`, `implement_finishes_frk_1`, `review_writes_note`, `implement_after_send_back_frk_1`, `review_writes_note`.
  - `farik-e2e-serve`'s name map gains every one of these names. The steps are:
    - `request.spec.ts`: file a request, see it sized as small, and answer a question by choice. The task then reaches `ready` (a low-risk plan needs no approval), shown as "To do" on its request page.
    - `approve.spec.ts`: open the plan, see the summary and checks, edit the intent and see the live check change, save back to refining, then approve once it returns.
    - `accept.spec.ts`: approve the high-risk plan, then, after its review, accept the task, open the diff, send it back once with a note (`implement_after_send_back_frk_1`), then accept, and find `human.accepted` in `farik log --json`. At 360 px the Accept and Send back buttons are visible without sideways scrolling, and the rail is sticky at 1280 px.

## File map

```
apps/web/src/pages/{Today,RequestFiled,Questions,PlanPage,PlanEditor,Gate,HelpNeeded}.tsx (+ .module.css, .test.tsx)   creates (T1–T4)
apps/web/src/pages/SendBackDialog.tsx (+ test)                    creates (T4)
apps/web/src/app/{App.tsx,landing.ts,landing.test.ts,words.ts,words.test.ts,store.ts,connection.test.tsx}, shell/{Shell.tsx,Shell.module.css,Shell.test.tsx}, pages/Settings.tsx, strings/en.ts   modifies / creates (T1)
crates/cli/src/bin/farik-e2e-serve.rs                              modifies: name map; once the named sessions are played, a session waits until aborted (T5)
crates/runtime/src/recorded/{fixtures.rs,transcripts/*.jsonl}      creates: the six new synthetic transcripts (T5)
apps/web/e2e/{request,approve,accept}.spec.ts, e2e/connect.spec.ts (its `/events` expectations become Today), e2e/fixtures/serve.ts (the `team` option, `events`, a two-press stop), e2e/fixtures/shots.ts   creates / modifies (T5)
docs/plans/project-plan.md (step 08 line)                          modifies (T5)
```

## Interfaces

Consumes: `serve.status`, `tasks.list`, `team.get` (steps 02 and 05); step 07's queries (`task.tries`, `sprint.current`, `waiting.list`, `contract.get`, `contract.check`, `task.history`, `task.diff`, `task.checks`, `questions.list`, `escalation.choices`, `team.activity`, `moved.since`), methods (`request.file`, `contract.save`), and commands (`human_accept`, `human_send_back`, `question_answer`, `request_triage`, `contract_lock`, `contract_unlock`, `escalation_resolve`, `task_integrate`); step 04's app frame; `@farik/ui`.

Produces: the routes above; `statusWord(status: TaskStatus, reason?: EscalationReason): string`.

## Tasks

### Task 1: Today, the rail, and the status words

- `words_cover_every_status`: `statusWord` gives the table's word for each `TaskStatus`, and the two `escalated` words by reason.
- `lands_where_the_status_says`: `landing` for each of the four cases; nothing renders before `serve.status`.
- `shows_the_team_band`: one entry per agent with avatar alt, name, role tag, and the activity line.
- `sends_a_request_to_the_team`: "Send to the team" calls `request.file` and navigates to `/requests/FRK-3`. A refusal shows under the box.
- `lists_what_waits_on_you`: the count heading, and each kind's title, button word, and route.
- `says_what_moved`: the time and the line.

- [x] `feat(web): add Today, with the team band, the request box and what waits on you`

### Task 2: The request and the questions

- `shows_the_request_and_its_size`, and `resizes_it`: the size and reason are shown; the button sends `request_triage` with the other size.
- `answers_by_choice_or_in_words`: a choice sends its label; text sends the text.
- `lets_the_agent_decide`: sends the pinned text.
- `hides_later_questions`.

- [x] `feat(web): add the request and question pages`

### Task 3: The plan and its editor

- `reads_the_plan_as_a_letter`: the signed summary, the parts, "not in this plan", and the checks.
- `approves_or_asks_for_changes`: the two commands.
- `checks_as_you_type`: `contract.check` runs once, 400 ms after the last keystroke, and the count and plain sentence appear.
- `locks_and_saves_back_to_refining`: the lock and unlock commands, and the save message.

- [x] `feat(web): add the plan page and the plan editor`

### Task 4: The acceptance gate, sending back, and help

- `leads_with_the_two_summaries_then_the_checks`: both signed summaries in order; the diff is collapsed until pressed, with its size line.
- `accepts_the_work`: sends the command; an epic requires a message.
- `sends_back_with_a_note`: criteria checkboxes, "Something else", a required note, and the try line; a `review_first` refusal shows its sentence.
- `accepts_an_epic_with_what_you_checked`: the text area is required for an epic.
- `offers_the_choices_for_the_reason`: renders `escalation.choices` and sends the chosen body, with the note or the label as `message`; the About panel shows the spend and the tries.
- `adds_accepted_work_to_the_project`: the integration kind's "Add to project" sends `task_integrate`.

- [x] `feat(web): add the acceptance gate, sending back, and the help page`

### Task 5: The journeys

- `request.spec.ts`, `approve.spec.ts`, `accept.spec.ts`, as in the Tests decision, with screenshots at 360 and 1280 px.
- The project plan's step 08 line.

- [x] `test(web): walk a request from asking to acceptance through the real server and browser`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/web: step 06's landed web count plus 23 (T1 6, T2 5, T3 5, T4 6, T5's fix 1);
#   playwright: step 06's 4 plus 3 = 7 passed; last line: xtask check: ok
```
