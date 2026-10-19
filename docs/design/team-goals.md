# The team works towards the owner's goal

Status: decided by the planner (Opus 5.5), 2026-10-19, on the founder's brief and two answers of that day. ADR 0075. SPEC revision 0.130. Standalone plan `docs/plans/standalone/team-goals.md`.

## The brief

The founder, 2026-10-19, verbatim:

> "I would like the user to define a goal for thoose agents where they will keep working together until achieving it. For example, Reach 10 paying customers, Create a mobile application and reach 10k downloads ,etc."

Asked how progress is known, the founder chose **"Measured, owner confirms"**: the team reads the number from a connected service where one exists (Stripe for paying customers), otherwise asks the owner; progress shows on Today; only the owner marks the goal reached.

Asked what limits it, the founder chose **"Daily budget only, Do not include any budgets other than daily ones"**: no goal budget and no deadline. The team keeps going day after day within the daily budget until the goal is reached or the owner stops it.

## What a goal is

- **The owner's words**, one to 300 characters: "Reach 10 paying customers", "Create a mobile application and reach 10k downloads".
- **A number to reach, when there is one**: a target (a number above 0) and its unit, 1 to 40 characters ("paying customers", "downloads"). Both or neither.
- **How it is counted**, only for a goal with a number:
  - from a **connected service**: a connector (`server`) an active agent on the team has connected, such as `stripe` on the Finance Specialist; that agent reads it once a day;
  - or **the owner says**: the owner types the number on Today when it changes.
- A goal with **no number** ("Create a mobile application") has no readings. It is reached only by the owner's word, as every goal is.
- A goal's id is `goal-<n>`, counted from 1 in the project's log.

**One goal at a time.** A second goal is refused while one stands (active or paused); the owner ends the first. Rejected: several goals at once. One board and one Product Manager would have to split rounds between them, Today would carry a card per goal, and the founder's examples are each one aim. "Create a mobile application and reach 10k downloads" is one goal with a number; the app is the way to it.

## What the owner sees

Today carries one **goal card**, after the request box and before the Catervafication card. It sits outside "Waiting on you" and its count: it is the team's aim, not a question.

**With no goal**, the card is quiet (dashed border, as the Catervafication card is):
- "Give your team a goal"
- "They keep working on it, day after day, within your daily budget."
- **Set a goal**

**Set a goal** opens a dialog:
- **What should your team achieve?** A text box.
- **A number to reach** (optional): a number and its unit, filled in from the words as the owner types ("Reach 10 paying customers" fills 10 and "paying customers"; "10k downloads" fills 10000 and "downloads"). The owner can change or clear both.
- **How do we count it?** Shown only with a number. "You tell us" is first and chosen, then one choice per service an active agent has connected, "From Stripe (Finn reads it)".
- An ⓘ beside the title opens these bullets:
  - Your team plans work towards it, round after round.
  - They spend only within your daily budget.
  - They count progress from a connected service, or ask you.
  - Only you mark it reached.
- **Set goal**. A team with no daily budget is refused in words: "Set a daily budget first: a goal keeps your team working every day." The words link to Settings.

**With a goal**, the card shows:
- the owner's words, large;
- for a goal with a number, a bar and "4 of 10 paying customers", then under it "From Stripe, read by Finn 3 hours ago" or "You told us 2 days ago". With no reading yet it says "Not counted yet";
- for a goal with no number, "Reached when you say so";
- one line for what the team is doing now: "Mira is planning the next steps", "5 tasks under way, 3 done", "Waiting on you (2)" when an open task waits on the owner, or "Paused";
- buttons: **Mark reached**, then **Update the number** (only with a number), then a "More" menu with **Edit**, **Pause** or **Resume**, and **End goal**.

**When the count reaches the target**, the team plans no more. The card says "Your team counts 10 of 10 paying customers. Is the goal reached?" with **Mark reached** first. **Update the number** and **Edit** (to raise the target) stay.

**When the team is stalled** (see the loop), the card says why and offers **Keep going**, **Edit** and **End goal**:
- "Five rounds of work and the count hasn't moved." when the number has not risen;
- "Five rounds of work since you last checked in." for a goal with no number;
- "Mira found nothing more to plan. Tell her more?" when a round filed no work.

**Mark reached** and **End goal** each ask once: "Mark ‘Reach 10 paying customers’ reached?" or "End this goal? Work already planned carries on." Afterwards the quiet card comes back, with one line under it: "Reached on 19 Oct: Reach 10 paying customers".

In **one agent alone** (ADR 0071) the card is the same, with that agent's name.

## The mechanism

### Events (`goal.<past_tense_verb>`)

All recorded in the log; the goal has no file of its own.

| Kind | Body | Who |
|---|---|---|
| `goal.set` | `goal`, `words`, `target?`, `unit?`, `server?` | the owner (`goal_set`, and `goal_edit` records it again with the same `goal`) |
| `goal.paused` | `goal` | the owner |
| `goal.resumed` | `goal` | the owner (Resume, and Keep going) |
| `goal.ended` | `goal`, `outcome`: `reached` or `dropped` | the owner |
| `goal.measured` | `goal`, `value`, `source`: `connector` or `owner`, `server?`, `note?` | an agent's tool, or the owner's `goal_report` |
| `goal.planned` | `goal`, `filed`: the task ids the round filed | Catervas, after the planning session ends |

`task.created` gains an optional `goal`, the goal a request was filed for. Sessions gain the purpose `goal`.

### Owner's commands

| Command | Body | Refusals |
|---|---|---|
| `goal_set` | `words`, `target?`, `unit?`, `server?` | `goal_active`, `goal_needs_daily_budget`, `goal_target_unit`, `goal_server_without_target`, `goal_server_not_connected` |
| `goal_edit` | `goal`, `words`, `target?`, `unit?`, `server?` | `goal_unknown`, as `goal_set` but `goal_active` |
| `goal_pause` | `goal` | `goal_unknown`, `goal_already_paused` |
| `goal_resume` | `goal` | `goal_unknown` (on a goal that is not paused it is Keep going: the owner's word, which clears a stall) |
| `goal_end` | `goal`, `outcome` | `goal_unknown` |
| `goal_report` | `goal`, `value` | `goal_unknown`, `goal_not_measured` (no target), `goal_value` (not finite or below 0) |

The query `goal.get` answers the standing goal, with its readings, status and counts, and the last ended goal.

### The loop

Catervas runs the loop with two rules. Both come last in a tick, after rule 10 and before the tick says it is idle, so they act only when nothing else did. Both run only in a tick of every rule scoped to no task, and neither runs while the team is paused.

**The planning rule.** The planner gets one session, purpose `goal`, when all of these hold:
1. A goal stands and is not paused.
2. The team has a daily budget set, and today's is not spent.
3. There is a planner, awake and not running a session. The planner is the active Product Manager, or the one agent in solo.
4. A new project's product plan is not awaited.
5. **The board is quiet**: no task in any status but `accepted` and `cancelled`. "Toward the goal" is the whole board, since every open task holds the same team.
6. The goal is not met: no reading at or above its target.
7. The goal is not **stalled**. It is stalled when the last round filed nothing and nothing new happened since, or when five rounds have filed work with no progress since.

Progress means one of these: the owner's word (a `goal.set`, a `goal.resumed`, or a `goal.measured` from the owner), or a reading above every earlier one. A round is one `goal.planned`.

The session's first message carries:
- the owner's words, the target and unit;
- the last five readings, each with its source and age;
- the measure, with who reads it;
- each earlier round's tasks with their status;
- the work accepted and waiting for the owner's merge;
- the method (below).

The session runs read-only on the read tier, with the agent's connectors. It is offered `catervas_file_goal_work`, `catervas_record_goal_progress`, the reading tools, `catervas_post_message` and `catervas_write_memory`. When it ends, Catervas records `goal.planned { goal, filed }`, where `filed` is each `task.created` with that `goal` since the session started. A session that ended `aborted` (a stop, or another session's cost spending the day, ADR 0064) records nothing, so the next tick with room plans again; one that ended at a limit counts as a round.

**The measuring rule.** Once per UTC day, the agent that reads the goal gets one session, purpose `goal`, offered `catervas_record_goal_progress` alone. That agent is the first active agent, in team order, with the goal's `server` connected. The rule needs: a goal that stands, is not paused, and has a `server`; no `goal.measured` from a connector today; no `goal` session of that agent started today; a day not spent; and the agent free. When the reader is also the planner, its planning session's message asks it to read first, so one session does both that day. The measure does not wait for a quiet board.

**Filing work: `catervas_file_goal_work { title, request, size }`.** Offered only in a `goal` session, at most five times per session.
- In a team: Catervas files a draft request from the title and request (the brief's wire), with `goal` on `task.created`. In the same filing it records `request.triaged { size, reason: "planned towards <goal>", triaged_by: catervas }`, so no triage session runs, the Scrum Master's included, and, as for a raise (5.16), the owner has no draft window to overrule the size. The Product Manager then writes its contract as for any request: a large one becomes an epic and is broken down as usual.
- In solo: the request is filed as a task given to the agent (ADR 0063's shape: the agent's role, the usual reviewer role, the request as its intent), small. A large size is refused: `goal_large_in_solo`.
- Refusals: `goal_session_only`, `goal_none` (no goal stands, or it is paused), `goal_work_limit`, the request's own (title 3 to 120 and request 20 to 2,000 characters).

**Recording a reading: `catervas_record_goal_progress { value, note? }`.** Offered only in a `goal` session. It records `goal.measured { source: connector, server }`.
- Refused `goal_session_only`, `goal_none`, `goal_not_measured` (no target or no `server`), `goal_not_your_service` (the caller has not connected the goal's `server`), `goal_value`.

**What the planner is told to do** (the session's message, and the Product Manager's skill `working-towards-the-goal`):
- Read the progress and what earlier rounds did.
- Pick the next one to five steps that move the number or build what the goal names, each small enough for one task or one epic.
- Prefer what the team can finish over research it cannot act on.
- Never repeat work that is done or under way.
- Name a step that needs a service the team lacks as a request for that connection.
- When nothing sensible is left, file nothing and say why in one post; the owner sees "found nothing more to plan".

### Outward acts and money

Nothing changes:
- a post outside the marketing plan, a paid connector call and a purchase still ask the owner (ADR 0061, 0042);
- Catervas opens pull requests and does not merge them;
- the marketing budget's hard stop holds.

The goal adds no budget, deadline or allowance. The team's daily budget is the only money limit. When the day is spent, no goal session starts, as no other session does (5.5), and the team carries on the next UTC day.

**Bounding the loop without money.** No round starts until the work of the round before is done, so rounds follow real work, not the clock. A round that files nothing stops the loop until something new comes in: a reading, an edit, Resume. Five rounds without progress stop it until the owner says Keep going. These bound a loop, as a session's tool-call limit does; they spend nothing.

Rejected:
- **A cap on planning sessions per day.** Rounds already wait for the board to empty, so a cap would only stop a team whose work goes fast.
- **A per-goal or per-round dollar budget.** The founder: "Do not include any budgets other than daily ones".
- **Asking the owner after every round.** The founder asked that they "keep working together until achieving it".

## Edge cases

- **Edit.** An edit records `goal.set` again with the same id. Its readings and rounds are kept, and it counts as the owner's word, which clears a stall. A new `server` takes effect from the next day's reading.
- **The reader leaves or disconnects.** With no active agent holding the goal's `server`, no measuring session runs. The card says "Nobody on the team has Stripe connected" and offers Update the number.
- **The planner is paused, retired or asleep.** No round runs. The card says "Mira is paused". The goal waits; nothing is refused.
- **Paused team, or a refused key (5.5).** Nothing runs, as everywhere.
- **New project.** No round runs before the product plan's task is accepted, since the interview comes first (5.16). The Catervafication's epic is on the board, so rounds wait for it too.
- **Ending or reaching the goal.** Open tasks filed for it carry on; nothing is cancelled. The owner cancels any they no longer want, on the board.
- **The target is met and the owner does not confirm.** No round runs. Measuring goes on daily, and the card keeps asking. Raising the target with Edit starts rounds again.
- **A task filed for the goal is escalated or blocked.** The board is not quiet, so no round runs until the owner answers. The card says "Waiting on you (n)".
- **Deliveries.** A quiet board is ADR 0062's "the team has no work left", so a round's work usually ends in a delivery: its changelog task, review, refreshes and retro run first, being earlier rules, and the next round follows them. That is the main cost of a round besides its work.
- **Unmerged work.** Accepted work waiting for the owner's merge does not hold the next round. The planner is told about it, and names unmerged work as a dependency when a step builds on it (5.14 starts such a task from its dependency's branch).
- **Solo.** The one agent plans, reads its own connected services, and files given tasks, small. Epics and deliveries do not run (ADR 0071).
- **An older log** holds no `goal.*` event: no goal, and the quiet card.

## Out of scope

- A goals page and a history of goals beyond the last one ended.
- Several goals at once, deadlines, and money set aside for a goal.
- Readings that Catervas takes itself with no agent (Catervas calls no connector itself).
- Merging by Catervas. That is a separate feature on another branch, and this one does not depend on it.
- A `catervas goal` command line. The web app is where the owner sets goals (ADR 0016).
