# Phase 6, step 08: Today and the gate pages

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.4 (what a human gate shows), 5.7, 5.11, 5.13, 5.16, 10, F4, F14
Depends on: steps 01 to 07 of this phase (step 07 supplies every query, method and command these pages use)
Readiness confirmed by: (pending)

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

- **Pages** follow the founder-approved mockups: `Main` (Today), `RequestFiled`, `Questions`, `AnswerQuestion`, `ApprovePlan`, `PlanEditor`, `Gate`, `SendBack`, `HelpNeeded`, `Phone`, and `PhoneGate`.
- **Routes:**
  - `/` is Today. It replaces step 04's redirect to `/events`, except that step 05's and 06's setup redirects still come first.
  - `/requests/:id`
  - `/tasks/:id/questions`
  - `/tasks/:id/plan` (read and approve) and `/tasks/:id/plan/edit`
  - `/tasks/:id/accept`
  - `/tasks/:id/help`

  Each Waiting row links to its route by `kind`: approval to `plan`, acceptance to `accept`, question to `questions`, help to `help`, and integration to `accept` (whose button is "Add to project" and sends `task_integrate`).
- **The rail's places:** Today, then Settings, with Events kept at the bottom of Settings as "Events (for testing)". Steps 09 and 10 add Board, Channel, Team, and Costs where the mockups put them.
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
  - **The sprint line** ("Sprint N is running: X of Y tasks done") appears only while a sprint is open, from `tasks.list` and the open sprint.
- **Request page.**
  - It shows "You asked" (the intent) and "<SM or PM> sized it as a big request / a small request", with the reason from `request.triaged`.
  - The two cards explain each size.
  - "Change this to a small request" (or big) sends `request_triage` with the reason "Changed by you".
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
  - "The plan in N parts": for a task, its requirements with each `exit_criteria` text as "done when"; for an epic, its children's titles with their summaries.
  - "Not in this plan": `scope.out_of_scope`.
  - "What Farik checked": `task.checks`.
  - "See the plan as written": the contract as YAML, read-only.
  - "About this plan": you asked, questions answered, risk in words, the estimate, and the builders (the assignee role in words).
  - Buttons: "Approve the plan" sends `human_accept { subject: contract }`. "Ask for changes" opens a note dialog and sends `human_send_back { subject: contract }`. "Edit the plan yourself" goes to the editor.
- **Plan editor.**
  - Fields are labelled in plain words, with the key in small mono text: intent, summary, requirements, exit criteria, out of scope, budget in dollars, risk, and allowed paths (the last two under Advanced).
  - Exit criteria can be added, each with text and "Farik runs a test" or "You decide". The first becomes a `test` criterion whose command the Developer fills in later. The second is a `human` criterion. No command or glob is asked outside Advanced (the phase decision).
  - Typing calls `contract.check` 400 ms after the last keystroke. It shows "Ready to approve? N of M checks pass. The one left: <plain>".
  - The lock banner shows either "Mira can still change this plan" with "Lock the plan" (`contract_lock`), or "You have locked this plan" with "Unlock" (`contract_unlock`).
  - "Save" calls `contract.save`. When it answers `back_to_refining`, the page says: "Saved. <PM> checks the plan again, then it comes back to you to approve."
- **Acceptance gate.**
  - The assignee's completion-note summary, signed "<name>, your <role>, wrote this for you".
  - The reviewer's review-note summary, signed "<name>, your <role>, reviewed it".
  - "What Farik checked".
  - "See the code changes · N files, +A −R", which opens the collapsed `DiffView`.
  - "About this task": you asked, when the plan was approved, "Tries i of n" (n including extra tries), the cost, and "See the whole history" (to step 09's task detail; until step 09 lands, it goes to `task.history` shown as a list).
  - Buttons: "Accept the work" sends `human_accept { subject: result }`. For an epic a message is required. "Send back with a note" opens the SendBack dialog.
  - **The SendBack dialog:** a checkbox per exit criterion and "Something else"; a required note; "This is try i of n. After the last, Farik stops and asks you." It sends `human_send_back { subject: result, failed_criteria }`.
  - A `review_first` refusal shows its sentence.
- **Help page.**
  - "<agent> needs your help with <title>".
  - The escalation's detail, in the agent's words.
  - "What <agent> tried": the task's progress notes.
  - The choices from `escalation.choices` as radio cards, an optional note, and the button labelled with the choice.
  - "About": waiting since, tries, "Spent $X of $Y", and the reviewer.
- **Mockup differences.**
  - "In the channel" waits for step 10.
  - "R-7" becomes the task id.
  - HelpNeeded's "Pause the task" is dropped (step 07's decision).
  - "Give Theo 2 more tries" appears only for reason `iterations`.
  - Choices show their hints where the agent gave one.
- **Strings.** Every string is in `en.ts`. Status words follow `web-ui.md`'s lifecycle table, through `statusWord(status): string` in `src/app/words.ts`, pinned by a test over every `TaskStatus`.
- **Tests.**
  - Vitest and axe for each page's logic.
  - Layout checks (phone fit, the sticky rail) run in the Playwright journeys at 360 × 780, since jsdom does not lay out.
  - The three journeys run on step 07's transcripts through `farik-e2e-serve`'s name map, which gains the needed names:
    - `request.spec.ts`: file a request, see it sized as small, answer a question by choice (`ask_with_choices_frk_1`), and reach the plan.
    - `approve.spec.ts`: open the plan, see the summary and checks, edit the intent and see the live check change, save back to refining, then approve once it returns.
    - `accept.spec.ts`: accept a high-risk task after its review, open the diff, send it back once with a note (`implement_after_send_back_frk_1`), then accept, and find `human.accepted` in `farik log --json`. At 360 px the Accept and Send back buttons are visible without sideways scrolling, and the rail is sticky at 1280 px.

## File map

```
apps/web/src/pages/{Today,RequestFiled,Questions,PlanPage,PlanEditor,Gate,HelpNeeded}.tsx (+ .module.css, .test.tsx)   creates (T1–T4)
apps/web/src/pages/SendBackDialog.tsx (+ test)                    creates (T4)
apps/web/src/app/{App.tsx,words.ts,words.test.ts}, shell/Shell.tsx (+ .module.css), strings/en.ts   modifies / creates (T1)
crates/cli/src/bin/farik-e2e-serve.rs                              modifies: name map (T5)
apps/web/e2e/{request,approve,accept}.spec.ts, e2e/fixtures/serve.ts   creates / modifies (T5)
docs/plans/project-plan.md (step 08 line)                          modifies (T5)
```

## Interfaces

Consumes: step 07's queries (`waiting.list`, `contract.get`, `contract.check`, `task.history`, `task.diff`, `task.checks`, `questions.list`, `escalation.choices`, `team.activity`, `moved.since`), methods (`request.file`, `contract.save`), and commands (`human_accept`, `human_send_back`, `question_answer`, `request_triage`, `contract_lock`, `contract_unlock`, `escalation_resolve`, `task_integrate`); step 04's app frame; `@farik/ui`.

Produces: the routes above; `statusWord(status: TaskStatus): string`.

## Tasks

### Task 1: Today, the rail, and the status words

- `words_cover_every_status`: `statusWord` gives the table's word for each `TaskStatus`.
- `shows_the_team_band`: one entry per agent with avatar alt, name, role tag, and the activity line.
- `sends_a_request_to_the_team`: "Send to the team" calls `request.file` and navigates to `/requests/FRK-3`. A refusal shows under the box.
- `lists_what_waits_on_you`: the count heading, and each kind's title, button word, and route.
- `says_what_moved`: the time and the line.

- [ ] `feat(web): add Today, with the team band, the request box and what waits on you`

### Task 2: The request and the questions

- `shows_the_request_and_its_size`, and `resizes_it`: the size and reason are shown; the button sends `request_triage` with the other size.
- `answers_by_choice_or_in_words`: a choice sends its label; text sends the text.
- `lets_the_agent_decide`: sends the pinned text.
- `hides_later_questions`.

- [ ] `feat(web): add the request and question pages`

### Task 3: The plan and its editor

- `reads_the_plan_as_a_letter`: the signed summary, the parts, "not in this plan", and the checks.
- `approves_or_asks_for_changes`: the two commands.
- `checks_as_you_type`: `contract.check` runs once, 400 ms after the last keystroke, and the count and plain sentence appear.
- `locks_and_saves_back_to_refining`: the lock and unlock commands, and the save message.

- [ ] `feat(web): add the plan page and the plan editor`

### Task 4: The acceptance gate, sending back, and help

- `leads_with_the_two_summaries_then_the_checks`: both signed summaries in order; the diff is collapsed until pressed, with its size line.
- `accepts_the_work`: sends the command; an epic requires a message.
- `sends_back_with_a_note`: criteria checkboxes, "Something else", a required note, and the try line.
- `offers_the_choices_for_the_reason`: renders `escalation.choices` and sends the chosen body.
- `adds_accepted_work_to_the_project`: the integration kind's "Add to project" sends `task_integrate`.

- [ ] `feat(web): add the acceptance gate, sending back, and the help page`

### Task 5: The journeys

- `request.spec.ts`, `approve.spec.ts`, `accept.spec.ts`, as in the Tests decision, with screenshots at 360 and 1280 px.
- The project plan's step 08 line.

- [ ] `test(web): walk a request from asking to acceptance through the real server and browser`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/web: step 07's landed count plus 19 (T1 5, T2 5, T3 4, T4 5);
#   playwright: step 06's 4 plus 3 = 7 passed; last line: xtask check: ok
```
