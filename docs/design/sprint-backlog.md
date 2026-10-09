# Sprints gather ready work

Status: decided by the founder on 2026-10-01, in conversation, with the answers to the open questions below given the same day.
Mockups approved by: the founder, 2026-10-01 (BoardBacklog, TodayBacklog, SettingsSprints and SetupSprints, canvas version 1790857656-ceac).

ADR 0028 records the decision. It is the design input to phase 6 step 15, and the spec changes land with that step (hard rule 8).

## Why

`farik serve` runs every rule all the time. With no sprint open, a task that becomes ready is assigned at once, so a sprint can never gather two requests (the milestone runbook's readiness review, finding B1; the runbook is now step 16). The founder decided that a team may plan work in sprints:
- the team keeps preparing work at any time;
- nothing new is assigned or built outside an open sprint, and work already under way when the policy is switched on finishes;
- ready work waits in a Backlog ("Should be in a backlog");
- work that becomes ready during a sprint waits for the next one;
- starting a sprint plans what is waiting.

The policy is on for new teams, `farik init`'s starter team among them, and off for existing projects, and Settings switches it.

## The policy on the wire

`team.yaml`'s `policy` is a flat object (`wip_limit_per_agent`, `escalation_age_hours`, `ambient_messages_per_sprint`, and so on), so the policy is one more key beside them:

```
policy:
  plan_in_sprints: true      # boolean, optional; absent means false
```

`team.schema.json` gives it `"type": "boolean", "default": false` and the description "Whether work waits for a sprint: with it on, nothing is assigned or built outside the open sprint, and ready work waits in the Backlog (spec 5.5)". The generated Rust field is `plan_in_sprints: Option<bool>`, and `Team::plans_in_sprints() -> bool` reads it, absent as `false`.

The defaults:
- **An old team file** has no key, so the policy is off and the project behaves exactly as before.
- **A team that setup makes** (`team.propose`, then `team.start`) has `plan_in_sprints: true`, written out so the user sees it in the file. `team.propose` answers `true` whatever the file says, since setup only ever makes a new team.
- **`settings.defaults`**, which Settings' "Put back the default" reads, answers `true`.
- **`farik init`'s starter team** has `plan_in_sprints: true` (the founder, 2026-10-01; open question 1). It takes its policy from `farik_core::team::defaults()`, which `settings.defaults` answers too, so one value serves both. "Put back the default" on an old project's Planning section therefore turns the policy on. `farik run` on a new project ends idle until `farik sprint start`. Tests and journeys that drive work without a sprint say `plan_in_sprints: false` in their team.

## What is gated, and what is not

With the policy on, Farik holds two kinds of **task** (`kind: task`) outside the open sprint, or any such task while no sprint is open:
- **a `ready` task**: it is not assigned (rule 8, and the assignment gate refuses an agent's or the human's ask). This is the one hold on new work, and it is enough: every task is built only after `ready` to `assigned`.
- **a task an ended sprint left for the Backlog**: it is not started (rule 7, `assigned` to `in_progress`), worked on (rule 6, an implement session) or reworked after a rejection (rule 3) until a sprint plans it.

**Work already under way when the policy is switched on finishes** (the founder, 2026-10-01). "Under way" is read from the status at each tick, with no mark taken at the switch: a task in `assigned`, `in_progress`, `blocked`, `verifying` or `rejected`, with no Backlog mark, carries on to acceptance through every rule, rework included. Whether a session is running for it does not matter. Only `ready` tasks wait. So the set of tasks built outside a sprint can only shrink after the switch: no new task joins it.

A session already running is left to finish, held or not.

Everything else runs at any time, sprint or not:
- triage, refining, and the Product Manager's questions;
- the plan check (`contract.judged`);
- the human's approvals and send-backs;
- **an epic's breakdown**: an approved epic is assigned to its breaker-down and broken into tasks outside a sprint. That assignment is preparation, and the epic's tasks then wait in the Backlog with it;
- an epic's close-out session;
- review and verification (rule 5), the Designer's design review among them;
- integration (rules 1 and 2);
- conversations in Chats → Team, one-to-one chats, and the ceremonies;
- the budget and channel rules.

With the policy off, nothing here changes: `in_the_open_sprint` keeps its rule ("with none open, any task; with one open, a task in it, or one under an epic in no sprint").

The core rule, one function both the gate and the orchestrator read:
- **`waits_for_a_sprint`** is true when the policy is on, the row is a task (not an epic), it is not in the open sprint (or no sprint is open), and it is `ready` or carries the Backlog mark.
- **The assignment gate** refuses such a task with "this team plans work in sprints, and <task> waits in the Backlog until a sprint plans it". Under the policy this sentence replaces the sprint membership sentence, so a refusal carries one sprint reason.
- **Under the policy, an epic's assignment passes the sprint's membership rule.** The sprint budget is checked only for a row in the open sprint.
- **The exception "a task under an epic in no sprint"** does not hold under the policy, so an unplanned epic's tasks wait with it.

**A task filed under an epic the open sprint holds** joins that sprint, as today (spec 3). It is the planned epic's own work, not new work.

**Incident fixes** (ADR 0027) skip planning and run even outside a sprint. Phase 9 step 06 (now phase 11 step 05e) builds the incident's fix contract and adds its exception to `waits_for_a_sprint`, and to the open sprint's membership rule with the policy off. This step adds nothing for it, because nothing exists to mark a contract as an incident fix yet.

**A sprint ended early** under the policy: its unfinished tasks leave it, as today, and now wait in the Backlog for the next sprint, where today they carry on (the founder kept this, 2026-10-01; answer 4). The end-early dialog says so. What tells them from work that was under way at the switch is the **Backlog mark**:
- `sprint.ended` gains an optional `backlog: boolean`, true when the team planned in sprints as the sprint ended, absent otherwise;
- the projection's task row gains `left_for_the_backlog: bool`, set for each task in `left` by a `sprint.ended` with `backlog: true`, and cleared by the `sprint.planned` that puts the task in a sprint;
- a task left by a sprint ended with the policy off has no mark, so switching the policy on later lets it finish, as any work under way;
- **switching the policy off clears every mark** (the controller's ruling, 2026-10-01, following answer 3, "started work finishes"): `team.updated` gains an optional `plan_in_sprints: boolean`, written by every team write, and the projection clears `left_for_the_backlog` on all tasks on a `team.updated` with `plan_in_sprints: false`. A task built while the policy was off is therefore never held again mid-work when the policy is switched back on.

## The Backlog

A row is **in the Backlog** when:
- the policy is on;
- it is not in the open sprint; and
- its status is `ready`, `assigned`, `in_progress` or `rejected`; and
- it is an epic, or a task that `waits_for_a_sprint`.

That covers a ready task, an epic being broken down or broken down, its ready tasks, and work left by a sprint ended early. Work under way at the switch keeps its lane (To do, In progress, Review), because it is being built. A `blocked`, `escalated` or `verifying` row keeps its lane. The predicate is `in_the_backlog` in `farik-core`, and the daemon answers it per row, so the Board does not work it out again.

**Waiting in the Backlog is not blocked.** A Backlog row stays in its status, so:
- the blocked-age rule (`blocked_limit_hours`, `blocker_age`) never counts the wait;
- escalation aging (`escalation_age_hours`) reads only `escalated` rows;
- the escalation digest is unchanged.

Nothing escalates because a sprint was not started.

A marked task under an epic that left the Backlog (the epic `escalated`, say) cannot be planned alone, because `farik_plan_sprint` refuses a task with a parent. It waits until its epic is back in the Backlog.

## The planning ceremony

Starting a sprint plans the Backlog, through the planning ceremony that exists (spec 5.9):
- **With the policy on, the candidates are the Backlog's rows with no parent**: ready tasks, and epics that are ready, being broken down, or broken down. An epic brings every task under it, as today.
- **Off,** the candidates stay "`ready`, no parent, in no sprint".
- **`farik_plan_sprint` accepts what the candidates are.** Under the policy that means a Backlog row with no parent, so a task under way since the switch, which is not in the Backlog, is not planned. Off, it accepts only `ready` tasks and approved epics, as today.
- **The planning message** names its list "the candidates, each waiting in the Backlog", and gives an epic's task count beside its contract.
- **The rest is unchanged:** planning runs once per sprint, and a planning that plans nothing is not asked again.

Work that becomes ready after the planning ran is not in the sprint, and waits.

## The Board

A **Backlog** lane between Planning and To do, shown only while the policy is on. `Lane` gains `backlog`, and `laneOf` returns it first for a row the daemon marks `backlog`.
- The lane's note: "Ready work waits here until you start a sprint."
- A card's status word: "Ready" while no sprint is open; "Waits for the next sprint" while one is.
- An epic's card keeps its parts line ("3 parts, 0 done").
- The sprint line with no sprint open, under the policy: "No sprint is running. Ready work waits in the Backlog until you start one." Off, it keeps "The team works through the board in order."
- While a sprint runs and the Backlog holds work: "Sprint 3 is running: 1 of 5 tasks done. 1 more waits in the Backlog for the next sprint."
- **"Start a sprint"** lists what waits in the Backlog under "Waiting in the Backlog" (id, title, "Epic, 3 tasks" or "Task"). Its planner line adds "Work that becomes ready later waits for the next sprint."
- **"End the sprint early"**, under the policy: "{count} tasks are not finished. They leave the sprint and wait in the Backlog for the next one. A session already running finishes. {name} will still run the review and the look back."
- On a phone, the lanes are tabs, and Backlog is one of them.

## Today

The line sits where the sprint line does, in the team band:
- **Policy on, no sprint, Backlog not empty:** "2 pieces of work are ready and wait in the Backlog. Start a sprint to begin them." "Start a sprint" is a link to `/board?start=sprint`, which opens the Board with the start dialog open. One piece of work: "1 piece of work is ready and waits in the Backlog."
- **A sprint running and the Backlog not empty:** the sprint line, then "1 more waits in the Backlog for the next sprint."
- **Otherwise,** as today.

The count is the Backlog's rows with no parent, so an epic counts once. It comes from a new query, `backlog.summary {}`, which answers `{ plan_in_sprints, count }`.

## Settings

"Your team's rules" gains a section after "How finished work is added": **Planning work**, with the switch **Plan work in sprints**. The line under it reads: "The team gets work ready at any time: it asks its questions, writes the plans and has them checked, and breaks big requests into tasks. Nobody starts building until you start a sprint. Work that becomes ready during a sprint waits in the Backlog for the next one."

It saves through `team.save`, like every rule there, with "Put back the default", Save changes, Cancel, and "What this changes" from `team.validate`'s effects (`describe_change`, which gains the board's counts: how many tasks are under way, and the titles of the Backlog's rows with no parent):
- **On:** "Ready work now waits in the Backlog until you start a sprint." When work is under way outside a sprint, also: "The <n> tasks already under way finish first." (The founder's answer of 2026-10-01; the approved mockup shows only the off state, so this line is not drawn.)
- **Off:** "Ready work starts as soon as someone is free, without waiting for a sprint." When the Backlog holds work, also: "The <n> pieces of work in the Backlog, <titles>, can start now." And always: "You can still start sprints from the Board."

## Setup

Setup asks no new question. The policy is on, and the last screen, Finishing work, says so once, in a box:
- "Your team works in sprints."
- "The team gets your requests ready straight away, and starts building when you start a sprint from the Board. Until then, ready work waits in the Backlog."
- "Change this under advanced settings, or later in Settings."

Behind "Show advanced settings", SetupAdvanced has the same switch as Settings. `team.start` carries the answer.

## Templates

A template holds `policy.plan_in_sprints`, optional in `team-template.schema.json`, so templates saved before this step still validate:
- **Saving** writes the team's effective value.
- **Using a template on a live team** takes the template's value when it has one, and keeps the project's when it has none.
- **Setup from a saved team** takes the template's value, and `true` when it has none, as any new team.

## Events and the command line

**No new event kind.**
- The switch is a `team.save`, which records `team.updated` as today.
- Starting and planning a sprint record `sprint.started` and `sprint.planned` as today.
- Holding a task records nothing, as a full agent's does not.
- `sprint.ended` gains the optional `backlog: boolean` above; old events without it read as `false`.
- `team.updated` gains the optional `plan_in_sprints: boolean` above; old events without it clear nothing.

**`tasks.list` rows gain `backlog: boolean`**, false whenever the policy is off.

**An idle tick** whose Backlog holds work while no sprint is open says "the ready work waits for a sprint". So `farik run` ends with that line, and after its closing list of what waits on the user it prints "start a sprint: <n> waits in the Backlog (`farik sprint start`)" (with `--json`, `{"backlog": {"count": n}}`). The line is not a kind of waiting, so Today's "Waiting on you" does not show it.

`farik board` is not changed in this step.

## Open questions for the founder, answered on 2026-10-01

1. `farik init`'s starter team: **on**, as setup's teams. The command line ends idle until `farik sprint start`.
2. A Backlog waiting with no sprint open as a row in Today's "Waiting on you": not asked separately; the approved TodayBacklog mockup has the line in the team band only, so that is what is built.
3. Turning the policy on while tasks are under way outside a sprint: **the started work finishes; only new work waits for a sprint**, over the design's "stop after the current session". The rule is in "What is gated, and what is not".
4. A sprint ended early under the policy: **its unfinished tasks wait in the Backlog** for the next sprint, as the design proposed.
