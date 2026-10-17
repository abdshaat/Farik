# Standalone catervafication-work, plan 6: the record

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md`, the sections below (hard rule 8)
Depends on: plans 1 to 5 of this branch (3b and 4b included)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

## Goal

`docs/SPEC.md` revision 0.117 says what plans 1 to 5 built, in the spec's own voice, each change
marked "(added in 0.117; ADR 0063)" or "(changed in 0.117; ADR 0063)", as earlier revisions mark
theirs. The plans' ticks and statuses are brought up to date. Nothing else changes.

## Decisions

- One revision, 0.117, written last, as ADR 0062's plan 8 writes 0.116. Chosen over a revision per
  plan: the Catervafication's paragraphs would be rewritten three times.
- The text below is what each section must come to say. It is written in the spec's style: plain
  sentences, no code beyond names on the wire. The new sentences sit after the 0.115 and 0.116
  text and do not restate it.
- `docs/design/catervas-folders.md` gets one line under its Catervafication section pointing to
  ADR 0063, since it is the design of record for the epic. No other design document changes.

## What each section says

- **Header.** A revision line after 0.116's: "Revision 0.117 (2026-10-16) records the
  Catervafication creating work and a chat message becoming a task (standalone plan
  `catervafication-work`, ADR 0063). In 5.16 an existing repository's Catervafication also files
  a code bug hunt for the Software Developer and a UI bug hunt for the UI/UX Designer, its
  Marketing task writes and starts the marketing plan beside the others, and its epic is filed
  with seven tasks. A hunter files each bug with `catervas_file_bug` as a request Catervas
  triages small, at most 10 per hunt and none twice while open. In 6.7 an agent asks for a
  connection with `catervas_request_connection`, shown first on Today. In 4.3 'Send as a task'
  gives the chat's agent a task it writes its own contract for and is assigned first. One event
  kind, `connection.requested`; `task.created` gains `found_by` and `given_to`, and
  `chat_message.posted` gains `as_task` (4.3, 5.1, 5.2, 5.16, 6.4, 6.5, 6.7, 6.8, 8.5, F8, F9)."
- **4.1** (the end of an existing project's setup): the offer's lines as plan 5 words them.
- **4.3**:
  - The owner may press "Send as a task" in place of Send. The message is posted with `as_task`,
    and no chat session answers it.
  - It is filed as a request given to that agent, linked to the message, with the agent's role as
    `assignee_role` and the usual reviewer role, triaged small by Catervas.
  - It is refused with `agent_not_active` for an agent that is not active, and with
    `task_too_short` under 20 characters.
  - The chat shows "Given to <Name> as <id>" under the message.
  - The chat session decides as well. A request for work its role does it takes with
    `catervas_take_task`, filing the same task given to itself, created by the agent, from the
    message it answers. Its input is the task's one-line title; the task's intent is always the
    owner's message, never the agent's words. It then replies in one line naming it. A question it
    answers, and work for another role it proposes as a request. Its refusals are
    `take_task_refused`, `agent_not_active`, `task_title`, `task_too_short` and `already_a_task`.
- **5.1, "Chat is not command"**: a chat session still files and moves nothing. The two ways from
  a chat to work are "Send as a request" under a proposal and "Send as a task", the owner's, and
  `catervas_take_task`, by which the chat session files a task given to its own agent.
- **5.2**:
  - Rule 8 takes the ready tasks given in a chat first. It assigns each to its agent on the
    assigner's behalf, with no session, when the agent has room under the WIP limit, naming the
    first other active agent of the reviewer role.
  - A given task is next in its agent's queue. While the agent is at its WIP limit the task
    waits, and in the first tick the agent has room it is assigned before any other ready task.
  - A task given to a paused agent waits.
  - A task whose `assignee_role` the human changed is no longer given.
- **5.16**, three places:
  - **Requests.** A request given in a chat is refined by its agent, not the Product Manager. That
    agent writes its contract with the Product Manager's standing while it is `draft` or
    `refining`, and no writer but the human may change its `assignee_role`
    (`given_task_role_fixed`) or its intent, the owner's words (`given_task_intent_fixed`). A bug a hunt files is triaged small by Catervas
    (`request.triaged { reason: "a bug <hunt> found", triaged_by: "catervas" }`) and refined by
    the Product Manager like any request.
  - **The Catervafication's epic.** For an existing repository:
    - its budget is seven placeholder budgets for one repository (see the project folder below);
    - its R2 is the hunts' requirement, and an epic without R2 (filed before 0.117) gets no hunt;
    - its intent and summary say the team also hunts and starts the marketing.
  - **Its tasks.**
    - The Marketing Specialist's task, "Write the marketing plan and start on it", is filed with
      the others, with no dependency, for both kinds of project. Its one criterion is the
      artifact `docs/catervas/marketing/kickoff` holding a heading and `MP-`.
    - For an existing repository of one repository, after the documentation tasks:
      the Developer's "Hunt for bugs in the code", and the Designer's "Hunt for bugs in the app's
      screens" when its browser is ready.
    - A hunt's `allowed_paths` are exactly `docs/catervas/engineering/bug-hunt` or
      `docs/catervas/design/ui-bug-hunt` in the team's format. Its one criterion is an artifact
      holding a heading and "Bugs filed", and its reviewer role is the Product Manager. No agent
      reviews it, since a hunt is the one task of a code-changing role that is not reviewed.
    - A hunt is filed once, with the Catervafication.
    - In a project folder, the hunts are filed for each member present, in `repos.yaml` order,
      each naming that member in `repo` and writing its report inside that member. A member has a
      UI when its scan finds a way to start its app. One without a UI gets no UI hunt, and the
      epic gets the progress note "No UI bug hunt for <name>: Catervas found no way to start its
      app." The epic's budget is five placeholder budgets plus two per member present at the
      Start.
- **5.17**: the Files page names the three new documents "Bug hunt", "UI bug hunt" and "Marketing
  kickoff".
- **6.4** (Developer) and **6.8** (Designer): each hunts once at the Catervafication and files
  bugs with `catervas_file_bug`, whose input, limits, intent wording, refusals (`bug_refused`,
  `bug_out_of_bounds`, `hunt_bug_limit`, `bug_already_filed`) and answer are as plan 2 states. A
  Designer's hunt runs no explore or design-plan stage.
- **6.5** (Marketing): it starts the plan at once. A plan Catervas approved at once is answered
  with the "Start on it now" words, and the agent asks for missing services with
  `catervas_request_connection`.
- **6.7** ("Suggested connections on Today"):
  - `catervas_request_connection { server, why }`, its refusals (`connection_why`,
    `connection_not_in_kit`, `connection_held`) and answer;
  - `team.get`'s `requested_connections`;
  - a requested pair listed first with "<Agent> asks: <why>", and Not now still hiding it.
- **8.5**:
  - the kind `connection.requested` and its body;
  - `task.created`'s `found_by` and `given_to`;
  - `chat_message.posted`'s `as_task`;
  - the count of event kinds, one more than 0.116 states.
- **F8**: "Send as a task" and `catervas_take_task`. **F9**: `catervas_request_connection` and `requested_connections`.

## File map

```
docs/SPEC.md                                                  modifies: the sections above
docs/design/catervas-folders.md                               modifies: one line pointing to ADR 0063
docs/plans/standalone/catervafication-work/*.md               modifies: Status done, checkboxes ticked
```

## Tasks

### Task 1: SPEC 0.117

Files: as the File map. A docs task carries no test (hard rule 1 covers production code). Its
check is the full check, plus a reading against "What each section says": every bullet above has
its sentence in the spec.

- [ ] `docs(spec): the Catervafication creates work and a chat message becomes a task, revision 0.117`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
pnpm check
# expected: exit 0
grep -c "added in 0.117\|changed in 0.117" docs/SPEC.md
# expected: a number of 10 or more
```
