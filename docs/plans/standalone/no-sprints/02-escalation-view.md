# Standalone CTV-9, plan 2: an escalation view that explains itself

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.7, F3 (Today's "Waiting on you"); recorded by plan 8 (revision 0.116)
Depends on: plan 1 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The help page (`/tasks/<id>/help`, `apps/web/src/pages/HelpNeeded.tsx`) keeps what it shows today
(what happened, in Catervas's or the agent's words; what the agent tried; the facts) and replaces
its radio group and single button with one button per choice: each shows its label, the server's
next words under it, and a "Suggested" mark on the suggested one; pressing it is the whole answer.
An open choice is a link to its page. The optional note stays, above the choices. Today's
"Waiting on you" row for an escalation already links to `/tasks/<id>/help` and keeps doing so; its
button now says "Answer". A Playwright journey answers an escalation from Today in one action (epic
C2 (d)). Out of scope: changing the choices (plan 1), the acceptance and approval gates.

## Decisions

- One button per choice, over today's radio group and a confirm button: R4 asks for one action. The
  suggested one is `kind="primary"`, the others `kind="secondary"` (`packages/ui/src/Button.tsx`);
  under each, a paragraph of class `muted` with the server's `next`, linked to the button by
  `aria-describedby`; beside the suggested one's label, the word `en.helpSuggested` ("Suggested")
  in a `<span>` (no new component). Chose the server's words as they come over composing them on
  the page (Ada 2.2: the page shows them as they are).
- An `open` choice is a `Link` (react-router) to its `path`, styled with `Button`'s classes through
  `className` of `styles.action` from `pages.module.css` (the class Today's rows use for their
  links); it sends nothing.
- A `command` choice sends `client.command({ command: choice.body.command, body: { ...choice.body.body,
  message } })` where `message` is the note trimmed, else the choice's label, as today; on success
  the page goes to `/`, on a refusal it shows `sentence(reply.error.detail)` in the existing alert.
  While one is sending, every choice button is `busy`/disabled.
- The note's label becomes `en.helpNote` = "Add a note for {name} (optional)"; it moves above the
  choices. The page has no radio, select or text field for a status.
- The section heading stays `en.helpNow` ("What should happen now?"); the list of choices is a
  `<ul aria-labelledby>` with one `<li>` per choice. `helpReadPlan` and the special case for
  `approval` with no choices go: the server now sends an `open` choice for it. `helpNothing` stays
  for an empty list.
- Today's escalation row: `KINDS.help.word` becomes `waitingHelpAnswer` = "Answer" (new key;
  `waitingHelpButton` is deleted). The link's target is unchanged.
- The journey's escalation is a `sessions` escalation: a task whose contract allows one session,
  whose one implement session stops early (`implement_stops_early`, which names no agent), is
  escalated by the budget rule on the next tick. Its suggested choice is "Ask Mira to change the
  plan". One new transcript, `refine_writes_one_session_task_for_theo_ctv_1.jsonl`, a copy of
  `refine_writes_task_for_theo_ctv_1.jsonl` whose `budget` is `{"max_cost_usd":5,"max_sessions":1}`
  and whose ids (session, message, tool-use) are new; chose this over a CLI escalation command,
  which does not exist and would be a new user-facing command for a test.
- Layout: the choices stack in one column at every width; each button is at most the page's width
  (360 px check in the journey). Brand tokens only; any new CSS goes in `PlanPage.module.css`
  beside `.letter` (the page's own module), classes `.choices` and `.choice`.

## File map

```
apps/web/src/pages/HelpNeeded.tsx                      modifies: one button per choice, next words, suggested mark (Task 1)
apps/web/src/pages/PlanPage.module.css                 modifies: .choices, .choice (Task 1)
apps/web/src/strings/en.ts                             modifies: helpSuggested, helpNote, waitingHelpAnswer; helpReadPlan, waitingHelpButton deleted (Task 1)
apps/web/src/pages/Today.tsx                           modifies: KINDS.help.word (Task 1)
apps/web/src/test/gate.ts                              modifies: escalation.choices fixtures gain next, suggested, kind (Task 1)
apps/web/src/pages/HelpNeeded.test.tsx                 tests: the view (Task 1)
apps/web/src/pages/Today.test.tsx                      modifies: lists_what_waits_on_you expects "Answer" (Task 1)
crates/runtime/src/recorded/transcripts/refine_writes_one_session_task_for_theo_ctv_1.jsonl   creates (Task 2)
crates/runtime/src/recorded/fixtures.rs                modifies: refine_writes_one_session_task_for_theo_ctv_1() (Task 2)
crates/cli/src/bin/catervas-e2e-serve.rs               modifies: names it and implement_stops_early (Task 2)
apps/web/e2e/escalation.spec.ts                        creates: the journey (Task 2)
```

## Interfaces

Consumes: plan 1's wire (`escalationChoicesResult` items `{label, next, suggested, kind, body?,
path?}`); `useConnection`, `useQuery`, `sentence`, `t`, `Button`, `TextArea`, `Link`; test helpers
`openedGate`, `sentCommand`, `event`, `TASK` (`apps/web/src/test/gate.ts`), `expectNoAxeViolations`;
e2e `startServe`, `events`, `screenshots` (`apps/web/e2e/fixtures/`).

Produces:

```ts
// apps/web/src/pages/HelpNeeded.tsx
type EscalationChoice = {
	label: string; next: string; suggested: boolean; kind: "command" | "open";
	body?: { command: string; body: Record<string, unknown> }; path?: string;
};
```

```rust
// catervas_runtime::recorded::fixtures
#[must_use] pub fn refine_writes_one_session_task_for_theo_ctv_1() -> Transcript;
```

## Tasks

### Task 1: one action per choice

Files: modified `apps/web/src/pages/HelpNeeded.tsx`, `apps/web/src/pages/PlanPage.module.css`,
`apps/web/src/strings/en.ts`, `apps/web/src/pages/Today.tsx`, `apps/web/src/test/gate.ts`; tested by
`apps/web/src/pages/HelpNeeded.test.tsx` (the test `offers_the_choices_for_the_reason` is replaced
by the first below; `sends_a_plan_to_approve_to_its_page` is replaced by the second;
`signs_the_agents_own_words_and_lists_what_it_tried_since` and `says_when_there_is_nothing_to_choose`
stay), `apps/web/src/pages/Today.test.tsx` (`lists_what_waits_on_you` expects "Answer" in place of
"Help").

- `answers_with_one_press_and_says_what_each_choice_does` — with the fixture choices "Give 2 more
  tries" (suggested, next "Theo picks up CTV-1 again with 2 more tries.", resolve `in_progress`
  with `extra_tries: 2`), "Ask Mira to change the plan" and "Cancel the task": a list named "What
  should happen now?" has three items in that order; each item's button has the label as its
  accessible name and the next words as its description; only the first item contains
  "Suggested"; the page has no radio button, no combobox, and one text box, "Add a note for Theo
  (optional)"; typing "Try the slow network." then pressing "Give 2 more tries" sends exactly one
  `escalation_resolve` with `{ task_id: "CTV-1", to: "in_progress", extra_tries: 2, message: "Try
  the slow network." }` and the page goes to `/`; a refusal `extra_tries_only_for_tries: ...` keeps
  the page and shows the sentence in the alert; pressing "Ask Mira to change the plan" with no note
  sends `message: "Ask Mira to change the plan"`; `expectNoAxeViolations(container)` passes.
- `opens_the_page_an_open_choice_names` — with the choices "Review the plan" (`kind: "open"`,
  `path: "/tasks/CTV-1/plan"`, suggested) and "Cancel the task": "Review the plan" is a link whose
  `href` is `/tasks/CTV-1/plan`, and following it sends no command.

- [ ] `feat(web): answer an escalation with one press, each choice saying what it does`

### Task 2: the journey from Today

Files: created the transcript (copy, then change only the `budget` object and the ids, as the
Decisions say; ADR 0061's plan 7 removed `judge_ctv_1_by_architect`, so no judging session runs
between the refine and the plan sessions) and `apps/web/e2e/escalation.spec.ts`; modified
`crates/runtime/src/recorded/fixtures.rs` (the function, doc "Mira writes CTV-1's plan for Theo,
allowing one session"), `crates/cli/src/bin/catervas-e2e-serve.rs` (`transcript()` names
`refine_writes_one_session_task_for_theo_ctv_1` and `implement_stops_early`). The journey runs under
`cargo xtask check --integration` with the other journeys.

- `an escalation is answered from Today in one action` (Playwright, `startServe({ team:
  "pm-architect-developer", transcripts: ["triage_ctv_1_small_by_pm",
  "refine_writes_one_session_task_for_theo_ctv_1",
  "plan_assigns_ctv_1_to_theo", "implement_stops_early"] })`, timeout 90 s) — the request "Add a
  done.txt at the root, so a run can be checked for it" is sent from Today; within 60 s Today's
  "Waiting on you" list has a row "Theo needs your help"; its link "Answer" leads to
  `/tasks/CTV-1/help`; the page shows "Ask Mira to change the plan" marked "Suggested" with the
  words "Mira rewrites the plan of CTV-1; it is assigned again once it is ready."; the page has no
  radio button and no combobox; at 360 px wide every choice button's right edge is at most 360;
  one press of "Ask Mira to change the plan" returns to `/`, and the log (`events(serve.project)`)
  holds one `escalation.resolved` with `to: "refining"` and `resolved_by: "human"`; the row is gone
  from Today. Screenshot `escalation` with `screenshots(page, "escalation")`.

- [ ] `test(web): answer an escalation from Today in one action, through the real server`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (the journey above among the browser journeys)
```
