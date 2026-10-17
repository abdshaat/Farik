# 0063. The Catervafication creates work, and a chat message can be a task

Date: 2026-10-16
Status: accepted (the founder, 2026-10-16, quoted below)

Amends ADR 0051 (the Catervafication documents the project and nothing else; the Marketing
Specialist's task waits for the product plan) and ADR 0026 part B (a one-to-one chat files nothing
but a request the agent proposed). Builds on ADR 0061 (only code is reviewed, only payments ask,
research is free, Catervas never merges) and ADR 0062 (deliveries replace sprints), which land
first.

The founder, in order:
1. "Change the process of Catervafication to both document and create work for agents. The UI/UX
   developer must start immediately hunting for UI bugs, the Developer must immediately begin
   looking for bugs in the code and reporting them, the marketing agent must also create a
   marketing plan and start executing on it immediately upon catervafication. The marketing agent
   may ask the user for connections that are necessary for the plan. Also, make the user able to
   create a task to a specific agent by messaging him."
2. Asked what happens to a bug: "Filed, then fixed". Each bug becomes a request on the board with
   its evidence; the Product Manager writes its contract and the team fixes it under the usual
   flow (code reviewed, a pull request to the human).
3. Asked when: "Alongside". The hunts and the marketing plan start in the first tick of the
   Catervafication, beside the documentation tasks, reading the code and the running app.
4. Asked who writes a chat task's contract: "The agent writes it". The owner's message becomes a
   task that agent writes its own contract for and starts on at once, with no Product Manager;
   code it changes is still reviewed.

## Context

The Catervafication (ADR 0051, spec 5.16) files one documentation task per role with a folder and
nothing else: the team reads, writes down what it knows, and waits for the owner. The Marketing
Specialist's task waited until the Product Manager's product plan was integrated (ADR 0061 plan 5
turns that wait into a dependency). A one-to-one chat is read-only and files work only when the
owner presses "Send as a request" under a proposal the agent wrote, which then goes through triage
and the Product Manager like any request.

## Decision

**Two hunts join an existing repository's Catervafication.** At the epic's breakdown point, beside
the documentation tasks, Catervas files:
- a code bug hunt for the Software Developer, when one is active: it reads the code and runs the
  project's tests and checks in the sandbox;
- a UI bug hunt for the UI/UX Designer, when one is active and its browser is ready (6.8): it goes
  through the app's preview with its Playwright connector.

A hunt changes no code. Its `allowed_paths` are exactly one report, `docs/catervas/engineering/bug-hunt`
or `docs/catervas/design/ui-bug-hunt` in the team's format, and its one criterion is an `artifact`
on that report holding a heading and the words "Bugs filed". A task whose `allowed_paths` are
exactly its role's hunt report is a hunt (`is_hunt`), as a task whose paths are exactly its plan is
a plan task.
- **A hunt is not reviewed.** ADR 0061 reviews every task of a code-changing role. A hunt is the
  one exception: `reviewed_by_an_agent` is false for it, so the governor accepts it on its
  criterion. Chosen over reviewing it, because then a team of a Product Manager and one Developer,
  the smallest common team, could not hunt at all (one Developer cannot review itself).
- Its `reviewer_role` is the Product Manager, who is always active (ADR 0055); no session of it
  runs.
- A Designer's hunt skips the explore and design-plan stage (ADR 0026), since it changes no
  screen; its implement session is given the preview and the browser, as every implement session
  of the Designer is.
- **One-off.** The hunts are filed once, with the Catervafication. A later hunt is the owner's
  word to the agent in its chat (below).
- **Not for a new project**, which has no code to hunt in, and not for a project folder (ADR
  0057), whose members a single report cannot cover. Both are left for a later decision.
- An epic filed before this change carries no hunts: only an epic whose contract holds the hunts'
  requirement gets them, so an old epic's budget is never overrun.

**A hunter files each bug as a request**, with a new tool, `catervas_file_bug`, in its hunt's
implement session alone. It takes a title, where it is, the steps, what was expected, what
happened, a severity and the evidence, and files a `draft` request:
- created by the hunter, its intent the bug's parts in fixed words;
- `task.created` names the hunt in `found_by`;
- triaged small by Catervas in the same filing, so no triage session runs.

The Product Manager then writes its contract like any request's, and the team fixes it under the
usual flow. Reusing `catervas_create_task` was rejected: its contract is free-form, so a bug's
evidence would have no shape the Product Manager can rely on, and nothing would bound or link it.
- **Bounded:** at most 10 bugs per hunt (`MAX_BUGS_PER_HUNT`).
- **No duplicates:** a bug whose title, trimmed, lower-cased and with its spaces collapsed, matches
  the title of an open task that any hunt filed is refused, `bug_already_filed`, naming that task.
- **A hunt ends** when its report is written and committed: what was checked, and under "Bugs
  filed" each bug's id and title.

**The Marketing Specialist starts alongside.** Its Catervafication task no longer waits for or
depends on the Product Manager's. The `FileDue::MarketingWaits` wait went with ADR 0061, and so
does the dependency that ADR's plan 5 put in its place. Under ADR 0061 it reads the whole project.
- Its task is "Write the marketing plan and start on it". It proposes the plan with
  `catervas_propose_marketing_plan`. A plan that spends nothing is approved at once (ADR 0061), and
  the agent schedules the first posts in the same session. A plan with money waits for the owner,
  as before.
- It writes `docs/catervas/marketing/kickoff`, naming the plan (`MP-<n>`), what it scheduled and
  what it asked for. Its criterion is an `artifact` on that file, which replaces the `review`
  criterion ADR 0061 removed. That removal would have left the task with none.

**An agent asks for a missing connection** with a new tool, `catervas_request_connection
{ server, why }`, which records `connection.requested`.
- It is refused for a service its role's kit does not offer (`connection_not_in_kit`) and for one
  the agent already holds (`connection_held`).
- Today's "Suggested connections" lists a requested pair first, with the agent's reason in place
  of the kit's description, and the same Connect and Not now.
- `catervas_ask_human` was rejected: a question holds the task until it is answered, and the
  founder wants the plan to start at once.
- A new section was rejected: the suggestions already offer exactly this pair with a working
  Connect, and Not now still hides it.
- Any agent may call the tool, since it can only point the owner at its own kit.

**The owner gives an agent a task from its chat.** The one-to-one chat's box gains a second
button, "Send as a task", beside Send.
- Making every message a task was rejected: a question would become work.
- Marking a message a task after it is sent was rejected: the agent may already have answered it.

The message is posted with `as_task: true`, and the same command (`chat_message_post` with
`as_task`) files a request:
- created by the human;
- linked by `from_chat_message`;
- `given_to` the agent;
- its intent the message, its `assignee_role` the agent's role, its reviewer role the usual one
  (`reviewer_for`);
- triaged small by Catervas.

The chat session does not answer such a message, so it cannot propose the same work again; the
message shows the task's link. It is refused for an agent that is not active (`agent_not_active`)
and for fewer than 20 characters.

Then the agent's own sessions take it, with no Product Manager session:
- **Refining.** The refine session is the given agent's, told the owner gave it the task. That
  agent writes the contract with the Product Manager's standing on that one task while it is
  `draft` or `refining`.
- **The role stays.** No writer may change its `assignee_role` (`given_task_role_fixed`), so the
  work cannot be passed to another role.
- **Judging and readiness are unchanged.** Contract judging (ADR 0061 keeps it) and the Definition
  of Ready apply as to any contract.
- **Assignment.** Once `ready`, Catervas assigns it to that agent on the assigner's behalf, with no
  session, as it assigns an approved epic. The reviewer is named as usual, and a code change is
  reviewed under ADR 0061.
- **Ahead of other ready work.** Rule 8 takes given tasks before other ready rows. They are not
  exempt from the WIP limit: a full agent's task waits for room. That limit is the governor's own
  and stays (ADR 0060, ADR 0062).

Every active agent may be given a task. The Product Manager's draft-to-refining move stays
mechanical (rule 10, no session).

## Consequences

- An existing repository's first work produces bugs on the board, and marketing that is already
  running, not only documents.
- The Catervafication of an existing repository is filed with seven tasks, and its epic's budget
  is seven placeholder budgets.
- Bugs a hunt misreads reach the Product Manager as requests. The bound of 10 per hunt and the
  duplicate rule keep the board from flooding. The Product Manager can cancel what is not a bug.
- A hunt's report and its bugs go unreviewed: a wrong report is caught when the owner reads it, a
  wrong bug when the Product Manager refines it.
- A chat task skips triage and the Product Manager, so the owner's words become a contract written
  by the agent that will do the work. The Definition of Ready, the judge, and the review of code
  are what check it.
- Three event shapes grow and one kind is added:
  - `chat_message.posted` gains `as_task`;
  - `task.created` gains `found_by` and `given_to`;
  - the new kind is `connection.requested`.

  Older logs read unchanged.
- Spec revision 0.117 records it.
