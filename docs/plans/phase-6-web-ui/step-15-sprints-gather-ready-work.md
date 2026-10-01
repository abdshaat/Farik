# Phase 6, step 15: Sprints gather ready work

Status: built; landing review pending
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3 (Sprint), 4.1, 4.4, 5.2, 5.5, 5.7, 5.9, 5.12 (team policy), 8.5, F3
Depends on: steps 01 to 14 of this phase, landed (step 14 recorded at 9512424; step 09's board, sprints and `lanes.ts`; step 14's templates). Added by the project plan's revision 25 on the founder's decision of 2026-10-01 (ADR 0028); the milestone runbook moves to step 16 and waits on this step.
Readiness confirmed by: fresh-session reviewer, 2026-10-01, not ready → findings folded in
Mockups approved by: the founder, 2026-10-01 (canvas version 1790857656-ceac)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A team can plan its work in sprints. With "Plan work in sprints" on, the team keeps getting work ready at any time: triage, questions, plans and their check, approvals, and an epic's breakdown. But no task is assigned or built outside the open sprint. Ready work waits in a Backlog that the Board shows as a lane and Today counts, and starting a sprint plans it. Work that becomes ready during a sprint waits for the next one. Work already under way when the policy is switched on finishes; a sprint ended early sends its unfinished tasks to the Backlog.

The policy is on for every team setup makes and for `farik init`'s starter team, off for every existing team file, and switched in Settings. This is what lets step 16's milestone run gather both requests into one sprint from the browser.

Out of scope:
- the incident fix's exception (ADR 0027), which phase 9 step 06 adds to `waits_for_a_sprint`, and to `in_the_open_sprint`'s membership rule with the policy off;
- `farik board`'s listing;
- any new event kind.

Binding inputs: ADR 0028 and `docs/design/sprint-backlog.md`. Where this plan differs from the design, the Decisions say so.

The founder's answers of 2026-10-01, cited by number throughout: (1) the starter team plans in sprints; (2) Today's line sits in the team band only; (3) started work finishes; (4) an early end sends work to the Backlog.

## Decisions

- **Mockups first** (the founder's standing gate, 2026-09-26; ADR 0026 D). Task 1's mockups, on the canvas page "Sprints", were approved by the founder on 2026-10-01 (version 1790857656-ceac). Copy settled in them binds Task 5, word for word. The one line they do not draw, Settings' "What this changes" when the switch is turned on with work under way, follows answer 3.
- **The wire:** `policy.plan_in_sprints: boolean`, optional, `default: false`, in `team.schema.json`. A flat key beside the other policy keys, over a nested `policy.sprints` object, because every policy key so far is flat. `Team::plans_in_sprints()` reads an absent key as `false`, so no old file changes behaviour.
- **Who writes `true`:** `farik_core::team::defaults()`, which `settings.defaults` and `farik init`'s starter team read; `team.propose` answers `policy.plan_in_sprints: true` whatever the file says, since setup only ever makes a new team; so `team.start` saves it, and `farik init` writes it (answer 1). `farik run` on a new project ends idle until `farik sprint start`.
- **Fixtures that drive work without a sprint say so:** `a_project` (the CLI tests' `farik init` project), `farik-e2e-serve`'s team and `serve.ts`'s `writeTeam` write `plan_in_sprints: false` unless a test asks for sprints, so the tests and journeys of phase 4 and steps 06 to 14 keep their flow unchanged; `a_team_wire` has no key, so it is off already. The `true` default and these fixtures land together in Task 3, after the hold works, so every commit stays green.
- **One predicate, in `farik-core`:** `waits_for_a_sprint`. The gate, the orchestrator's rules 3, 6, 7 and 8, and the Backlog all read it. The policy is checked in one place, and phase 9 step 06 adds the incident exception there. Under the policy, `in_the_open_sprint(kind, input)` for a `Task` is `!waits_for_a_sprint`, read with `status: Ready` and no mark, and its refusal is the Backlog sentence, which **replaces** the membership sentence. Off, the sentence is unchanged.
- **Under way**, defined once: a `Task` not in the open sprint, in `assigned`, `in_progress`, `blocked`, `verifying` or `rejected`, **without** the Backlog mark. Settings' count and `describe_change`'s `SprintWork` read it.
- **An epic's assignment is preparation:**
  - under the policy, an epic passes the sprint membership rule, so its breakdown runs outside a sprint;
  - the sprint budget is checked only for a row in the open sprint;
  - the old exception "a task under an epic in no sprint" does not hold under the policy;
  - off, `in_the_open_sprint` is unchanged.
- **What is held (answer 3):** started work finishes, and only new work waits. The rule, read from status at each tick with no mark taken at the switch: under the policy, a task outside the open sprint is held when it is `ready` (rule 8 and the gate refuse its assignment), or when it carries the Backlog mark (rules 7, 6 and 3 do not start, work on or rework it). A task in `assigned`, `in_progress`, `blocked`, `verifying` or `rejected` without the mark is under way and carries on to acceptance, rework included; a running session does not decide it. Over a mark taken at the switch, because status already tells the two apart, and over gating rules 3, 6 and 7 for every task, which stopped work under way (the design's first choice, reversed).

  Why both answers hold: every task is built only after `ready` to `assigned`, which is held, so nothing new is built outside a sprint and the work under way can only shrink. Review and verification (rule 5), integration (rules 1 and 2), epic plan sessions, triage, refining, the plan check, chats, the channel's conversation rule and the ceremonies run at any time. A running session is never stopped.
- **A sprint ended early under the policy sends its unfinished tasks to the Backlog** (answer 4, keeping the design's choice), over "work in progress keeps going", because the founder's rule is "no task is built outside an open sprint". Status cannot tell a task it left from one under way at the switch, so it takes the **Backlog mark**: `sprint.ended` gains an optional `backlog: boolean`, true when the policy is on; the projection sets `left_for_the_backlog` on each task in `left` by such an event, and the `sprint.planned` that puts the task in a sprint clears it. Over un-assigning the tasks, because the lifecycle has no move back to `ready` (spec 5.2's table), and over `blocked`, which ages and escalates. A sprint ended with the policy off marks nothing, so its tasks finish if the policy is switched on later. The dialog's sentence changes to match.
- **Switching the policy off clears every Backlog mark** (the controller's ruling, 2026-10-01, following answer 3). `team.updated` gains an optional `plan_in_sprints: boolean`, written by every team write; the projection clears `left_for_the_backlog` on all tasks on a `team.updated` with `plan_in_sprints: false`. A task built while the policy was off is never held again mid-work when it is switched back on. Over the review's alternative (the mark stays and re-holds the task), which stops started work.
- **The Backlog:** a row with the policy on, not in the open sprint, in `ready`, `assigned`, `in_progress` or `rejected`, that is an epic or a task that `waits_for_a_sprint`. Work under way at the switch keeps its lane. `blocked`, `escalated` and `verifying` keep their lanes. Nothing in the Backlog changes status, so the blocked-age rule, escalation aging and the digest never count the wait.
- **Planning's candidates under the policy:**
  - the Backlog's rows with no parent;
  - `farik_plan_sprint` accepts those statuses for an assigner's plan;
  - off, both stay "`ready`, no parent, in no sprint".

  A task filed under an epic in the open sprint still joins it.
- **No new event kind:**
  - the switch is `team.save`, which records `team.updated`;
  - holding a task records nothing, as a full agent's does not;
  - `sprint.ended` gains the optional `backlog` field, no new kind;
  - `describe_change` gives the switch's effect lines.
- **The Board reads the daemon:** `tasks.list` rows gain `backlog: boolean`, worked out by `in_the_backlog`. `laneOf` does not compute it again.
- **Today's count** comes from a new query, `backlog.summary {}`, over making Today load `tasks.list`.
- **The idle line:** a tick with Backlog work and no open sprint says "the ready work waits for a sprint". Precedence of `idle()`: a spent day, then a sleeping agent, then `WAITS_FOR_A_SPRINT`, then `NOTHING_TO_DO`. `farik run` prints "start a sprint: <n> waits in the Backlog (`farik sprint start`)" in `run.rs` after `print_waiting`, from `in_the_backlog` over the board. It is not a `WaitingKind`, so `waiting.list` (Today's "Waiting on you") is unchanged (answer 2). With `--json` it is `{"backlog": {"count": n}}`.
- **Templates:**
  - `team-template.schema.json` gains an optional `policy.plan_in_sprints`, so old templates validate;
  - saving writes the effective value;
  - applying takes it when present and keeps the project's otherwise;
  - setup from a template without it takes `true`.

## File map

```
docs/design/mockups/{BoardBacklog,TodayBacklog,SettingsSprints,SetupSprints}.dc.html, canvas.json   created (T1, in the design commit)
docs/schemas/team.schema.json, docs/schemas/team-template.schema.json   modifies (T2)
crates/core/src/generated/mod.rs, crates/core/src/team.rs               modifies (T2: the field, plans_in_sprints)
crates/core/src/governor/gates.rs                                       modifies (T2: waits_for_a_sprint, in_the_backlog, the gate)
crates/core/src/team/template.rs, crates/core/src/team/describe.rs      modifies (T2: carry; effect lines)
crates/core/src/team/defaults.rs                                        modifies (T3: true by default)
docs/schemas/event.schema.json, crates/store/src/projections.rs          modifies (T3: sprint.ended's backlog, team.updated's plan_in_sprints; the mark)
crates/store/src/migrations/0010_backlog_mark.sql, migrations.rs         creates / modifies (T3: the column; MIGRATIONS: [Migration; 10])
packages/protocol-client/src/generated/event.ts                          modifies (T3, generated)
crates/runtime/src/recorded/fixtures.rs, transcripts/planning_ceremony_frk_1_frk_3.jsonl   modifies / creates (T3)
crates/core/src/team.rs                                                  modifies (T3: the defaults test's expectation)
crates/runtime/src/transitions.rs                                       modifies (T3: AssignmentInput's plan_in_sprints)
crates/runtime/src/daemon/team.rs                                       modifies (T3: team.updated's plan_in_sprints; T4)
crates/runtime/src/orchestrator/rules.rs, orchestrator/requests.rs      modifies (T3: rules 3, 6, 7, 8; candidates; idle line)
crates/runtime/src/sprints.rs, orchestrator/messages.rs                 modifies (T3: plan_sprint's statuses; planning message)
crates/cli/src/run.rs, crates/cli/src/init.rs                           modifies (T3: the closing list; the starter team's test)
crates/cli/tests/shared/project.rs, crates/cli/src/bin/farik-e2e-serve.rs   modifies (T3: plan_in_sprints: false)
apps/web/e2e/fixtures/serve.ts                                           modifies (T3: writeTeam writes plan_in_sprints: false)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/{web.rs,gates.rs,team.rs,templates.rs}   modifies (T4)
packages/protocol-client/src/client.ts, generated/rpc.ts                 modifies (T4)
apps/web/src/app/lanes.ts, apps/web/src/pages/{Board,Today,TeamRules,SprintPage}.tsx (+ tests), strings/en.ts   modifies (T5)
apps/web/src/pages/dialogs/{StartSprint,EndSprint}.tsx                  modifies (T5)
apps/web/src/pages/setup/{SetupFinish,SetupAdvanced}.tsx, setup/team.test.tsx   modifies (T5)
apps/web/e2e/sprints.spec.ts, apps/web/e2e/fixtures/serve.ts          creates / modifies (T6: the `sprints?: boolean` option only)
docs/SPEC.md, docs/plans/project-plan.md                                modifies (T7)
```

## Interfaces

Consumes:
- `AssignmentInput`, `in_the_open_sprint`, `fits_the_open_sprint`, `check_assignment` (`governor/gates.rs`, phase 4);
- `Team`, `describe_change`, `template_from_team`, `apply_template` (step 06, step 14);
- `sprint_planning`, `plan_sprint`, `planning_session_spent` (phase 4);
- `tasks.list`, `sprint.current`, `team.propose`, `settings.defaults`, `team.save` (steps 06, 09);
- `Switch`, `Dialog` (`@farik/ui`).

Produces:

```rust
// farik-core
impl Team { pub fn plans_in_sprints(&self) -> bool; }
pub struct SprintHold<'a> { pub plan_in_sprints: bool, pub open_sprint: Option<&'a str>, pub kind: Kind, pub status: TaskStatus, pub sprint: Option<&'a str>, pub left_for_the_backlog: bool }
pub fn waits_for_a_sprint(hold: &SprintHold<'_>) -> bool;   // policy on, kind Task, sprint.is_none() || sprint != open_sprint, status ready or left_for_the_backlog
pub fn in_the_backlog(hold: &SprintHold<'_>) -> bool;       // policy on, sprint != open, status in {ready, assigned, in_progress, rejected}, Epic or waits_for_a_sprint
pub fn in_the_open_sprint(kind: Kind, input: &AssignmentInput) -> bool;   // gains kind
AssignmentInput.plan_in_sprints: bool
pub struct SprintWork<'a> { pub under_way: u32, pub in_the_backlog: Vec<&'a str> }   // titles of the Backlog's rows with no parent
pub fn describe_change(old: &Team, new: &Team, work: &SprintWork<'_>) -> Vec<String>;   // gains work; daemon/team.rs (team.validate) and daemon/templates.rs (preview/apply) pass it
// farik-store
TaskProjection.left_for_the_backlog: bool   // set by sprint.ended { backlog: true } for its `left`; cleared by sprint.planned, and on every task by team.updated { plan_in_sprints: false }
// farik-runtime
const WAITS_FOR_A_SPRINT: &str = "the ready work waits for a sprint";   // orchestrator/rules.rs
```

Wire (`rpc.schema.json`; `camelCase` in `protocol-client`):
- `tasks.list` rows gain `backlog: boolean`, `required` in `rpc.schema.json`;
- query `backlog.summary {}` → `{ plan_in_sprints: boolean, count: integer }`, where `count` is the Backlog's rows with no parent;
- `team.yaml` `policy.plan_in_sprints`; `templates/<slug>.yaml` `policy.plan_in_sprints`;
- event `sprint.ended` gains optional `backlog: boolean` (`event.schema.json`), absent as `false`;
- event `team.updated` gains optional `plan_in_sprints: boolean`, the saved team's effective value; absent clears nothing.

## Tasks

### Task 1: Mockups, approved by the founder

Files (drawn on 2026-10-01 in `docs(design): mock up the sprint backlog for the founder's approval`):
- `BoardBacklog`: the Board with a Backlog lane, no sprint and work waiting; "Start sprint 3" listing the Backlog; a sprint running with one late request waiting for the next;
- `TodayBacklog`: Today's line in both states;
- `SettingsSprints`: "Planning work" with the switch, on, then off with "What this changes";
- `SetupSprints`: the Finishing work box and SetupAdvanced's switch.

All in the canvas's "Sprints" page, in `@farik/brand`'s tokens only, one colour per job. No test. The gate is the founder's approval, recorded in this header in the commit that ticks this box: the founder approved them on 2026-10-01, canvas version 1790857656-ceac, recorded in 64d27d7.

- [x] `docs(plans): record the founder's answers on the sprint backlog`

### Task 2: The policy and its predicates (`farik-core`)

Files and produces: as the file map and Interfaces say.

- `reads_an_absent_policy_as_off`: the base fixture without the key gives `plans_in_sprints() == false`; with `plan_in_sprints: true` gives `true`; the schema refuses `"yes"` at `/policy/plan_in_sprints`.
- `holds_a_task_outside_the_sprint`: policy on: a `ready` `Task` with no sprint open is held; one in `S1` with `S1` open is not; one in no sprint with `S1` open is held; an `assigned`, `in_progress` or `rejected` task in no sprint with `left_for_the_backlog` is held. Policy off: never held, mark or not.
- `lets_work_under_way_finish`: policy on, no sprint open, no mark: a `Task` in `assigned`, `in_progress`, `blocked`, `verifying` or `rejected` is neither held nor in the Backlog (the founder's answer 3).
- `never_holds_an_epic`: policy on, an `Epic` in no sprint with or without a sprint open, `waits_for_a_sprint` is false.
- `places_rows_in_the_backlog`: policy on, no sprint open: a `ready` task is in; a task in `assigned`, `in_progress` or `rejected` is in with the mark and out without it; an epic in any of the four is in; `draft`, `refining`, `escalated`, `blocked`, `verifying`, `accepted`, `cancelled` are out, mark or not. A row in the open sprint is out. Policy off: every row is out.
- `assigns_an_epic_outside_the_sprint_under_the_policy`: `check_assignment` passes an approved epic with no sprint open, and with `S1` open and the epic in none, with a sprint budget of $1 and an epic budget of $5; off, the second case refuses as today.
- `refuses_a_held_task_in_plain_words`: policy on, no sprint open: `check_assignment` fails with exactly one sprint reason, "this team plans work in sprints, and FRK-1 waits in the Backlog until a sprint plans it"; with S1 open and FRK-1 in none, the same sentence.
- `drops_the_unsprinted_epic_exception_under_the_policy`: a task with `parent_sprint: Some(None)` while `S1` is open passes with the policy off and fails with it on.
- `carries_the_policy_in_a_template`: `template_from_team` writes `plan_in_sprints: true` for a team with it; `apply_template` with the key absent keeps the project's `true`, and with `false` sets `false`; a template without the key validates.
- `describes_the_switch`: from off to on, the effects are "Ready work now waits in the Backlog until you start a sprint.", plus "The 2 tasks already under way finish first." when `under_way` is 2, and no second line when it is 0. From on to off, the effects are "Ready work starts as soon as someone is free, without waiting for a sprint.", plus "The 2 pieces of work in the Backlog, Gift cards at checkout and Sold-out badge on the menu, can start now." when two titles are given (the SettingsSprints mockup's line), and in every case "You can still start sprints from the Board.". With the key unchanged, nothing is said.

`defaults()` stays without the key in this task; Task 3 turns it on.

- [x] `feat(core): let a team plan its work in sprints, holding ready tasks in a backlog`

### Task 3: The orchestrator holds work and plans the Backlog (`farik-runtime`, `farik-cli`)

Consumes: Task 2. The same commit turns `defaults()` to `plan_in_sprints: Some(true)` and makes `a_project` (`crates/cli/tests/shared/project.rs`), `farik-e2e-serve.rs`'s `recorded_team` and `serve.ts`'s `writeTeam` write `plan_in_sprints: false`. It also edits the expectation of the existing `defaults()` test in `crates/core/src/team.rs` (around line 1277) to add `plan_in_sprints: Some(true)`: an edited expectation, not a skipped test. The store gains migration 0010 (`ALTER TABLE task_projections ADD COLUMN left_for_the_backlog INTEGER NOT NULL DEFAULT 0;`, no wipe, since old `sprint.ended` events mean 0), and `opens_the_projections_of_a_log_that_has_read_nothing_yet` expects `[1..=10]`. `generated/event.ts` is regenerated (`pnpm --filter @farik/protocol-client generate`).

- `holds_a_ready_task_until_a_sprint_opens` (rules.rs): policy on, FRK-1 `ready`, no sprint: ticks are idle with `WAITS_FOR_A_SPRINT`, no `plan` session starts, FRK-1 stays `ready` and no `escalation.raised` is recorded.
- `breaks_an_epic_down_outside_a_sprint`: policy on, an approved epic FRK-1, no sprint: Farik assigns it to `sm`, `plan_breaks_down_frk_1` breaks it down, its task FRK-2 reaches `ready`, and none is assigned on the ticks that follow.
- `plans_both_on_the_first_tick_of_the_sprint`: policy on, the broken-down epic FRK-1 with its task FRK-2, and the small task FRK-3 `ready`, waiting; the human starts S1; the first tick is S1's planning ceremony, its message lists FRK-1 with the number of tasks under it, and FRK-3, and after the new transcript `planning_ceremony_frk_1_frk_3` (`farik_plan_sprint [FRK-1, FRK-3]`) FRK-1, FRK-2 and FRK-3 are in S1. The readiness review's B1 sequence, without pausing.
- `keeps_late_work_for_the_next_sprint`: policy on, S1 planned and running; FRK-3 becomes `ready`: it is not assigned and not in S1, while S1's own tasks are; after S1 ends and S2 starts, S2's planning lists FRK-3.
- `stops_unfinished_work_of_an_ended_sprint`: policy on, FRK-1 `assigned` and FRK-2 `rejected` in S1; `end_sprint`: `sprint.ended` has `backlog: true` and `left` [FRK-1, FRK-2]; FRK-1 stays `assigned` and FRK-2 stays `rejected` across ticks until S2 plans them, and then both go on (answer 4).
- `finishes_work_under_way_when_the_policy_turns_on` (rules.rs): FRK-1 `in_progress`, FRK-2 `rejected` and FRK-3 `assigned`, in no sprint, reached with the policy off; the team file is then saved with it on and FRK-4 becomes `ready`: FRK-1's implement session runs, FRK-2 is reworked, FRK-3 starts, and FRK-4 stays `ready` with no assignment (answer 3).
- `forgets_the_backlog_when_the_policy_turns_off` (rules.rs, the ruling on S7): policy on, S1 ended early leaves FRK-1 `assigned` with the mark; `team.save` with the policy off records `team.updated` with `plan_in_sprints: false`, the mark is gone and FRK-1 starts; `team.save` with it on again: FRK-1 carries on to `in_progress` and is not in the Backlog.
- `marks_what_a_sprint_leaves_for_the_backlog` (store projections.rs): `sprint.ended` with `backlog: true` sets `left_for_the_backlog` on each task in `left` and no other; one without the field sets nothing; a later `sprint.planned` naming the task clears it; a `team.updated` with `plan_in_sprints: false` clears every mark, and one with `true` or without the field clears none.
- `runs_review_and_integration_outside_a_sprint`: policy on, no sprint, FRK-1 `verifying` from before: its review session runs; an accepted task integrates. (Guard: passes before and after; it pins what must not be held.)
- `flows_as_before_with_the_policy_off`: the existing `assigns_the_backlog_without_a_sprint` (`rules.rs`) passes unchanged, with the key absent. (Guard, not a new test.)
- `plans_a_backlog_epic_under_way` (sprints.rs): policy on, `plan_sprint` of an `in_progress` epic in no sprint is accepted and brings its tasks; off, it is refused with "only a ready task or an approved epic is planned".
- `says_the_backlog_waits` (cli run.rs): `farik run` on a policy-on project with FRK-1 `ready` exits idle and prints "start a sprint: 1 waits in the Backlog (`farik sprint start`)".
- `writes_a_starter_team_that_plans_in_sprints` (cli init.rs): `starter_team("notes")` gives `plans_in_sprints() == true` (answer 1).
- `defaults_to_planning_in_sprints` (core `team/defaults.rs`): `defaults().policy` has `plan_in_sprints: Some(true)`, and a team from `a_team_wire()`, which has no key, reads as off.

- [x] `feat(runtime): hold work outside the open sprint and plan the backlog when one starts`

### Task 4: The Backlog over the wire

Files: as the file map says.

- `marks_backlog_rows` (daemon/web.rs): `tasks.list` on a policy-on project answers `backlog: true` for a ready FRK-1 with no sprint, `false` after S1 plans it, `false` for an `in_progress` FRK-2 under way since before the switch; every row `false` with the policy off.
- `summarises_the_backlog` (daemon/gates.rs): `backlog.summary` answers `{ plan_in_sprints: true, count: 2 }` for a ready task and a broken-down epic with three tasks; `{ plan_in_sprints: false, count: 0 }` with the policy off.
- `proposes_sprints_for_a_new_team` (daemon/team.rs): on the harness's team, which has no key, `team.propose` answers `policy.plan_in_sprints: true` and `settings.defaults` answers `true`; `team.start` with the proposed team writes `plan_in_sprints: true` to `team.yaml`.
- `leaves_the_backlog_out_of_waiting` (daemon): `waiting.list` on a policy-on project with FRK-1 waiting in the Backlog answers no row for it (answer 2).
- The protocol client (not counted above): `client.test.ts` maps `plan_in_sprints` → `planInSprints` and the row's `backlog`. The mapping is generic, so watch it fail on the generated `QueryName` lacking `backlog.summary` (a type error). `generated/rpc.ts` is regenerated.

- [x] `feat(runtime): answer which work waits in the backlog, and propose sprints for new teams`

### Task 5: The pages

Every test also runs axe (step 06's rule). Copy from the approved mockups. Pages learn the policy from `backlog.summary`'s `plan_in_sprints`; no page loads `team.get` for it. `en.ts` gives the plural of the running-sprint line ("{n} more wait in the Backlog for the next sprint.").

- `places_backlog_rows_in_their_lane` (lanes.test.ts): `laneOf` answers `backlog` for a row with `backlog: true` in each of the four statuses, and the step 09 mapping for every row without it; `LANES` is Planning, Backlog, To do, In progress, Stuck, Review, Done.
- `shows_the_backlog_lane_only_under_the_policy` (Board.test.tsx): with `planInSprints` true the lane, its note and the sprint line "No sprint is running. Ready work waits in the Backlog until you start one." show; with it false there is no Backlog lane and the line is step 09's. A card's status word is "Ready" with no sprint open and "Waits for the next sprint" with one; an epic's reads "Ready, broken into 3 tasks"; with a sprint running, the line ends "1 more waits in the Backlog for the next sprint.".
- `lists_the_backlog_when_starting_a_sprint`: the start dialog lists FRK-12 "Epic, 3 tasks" and FRK-14 "Task" under "Waiting in the Backlog"; `/board?start=sprint` opens it.
- `says_what_ending_early_does_under_the_policy` (`EndSprint`, also on `SprintPage`): the end-early dialog says the tasks "wait in the Backlog for the next one".
- `counts_the_backlog_on_today` (Today.test.tsx): `backlog.summary` `{ count: 2 }` and no sprint shows "2 pieces of work are ready and wait in the Backlog." with a "Start a sprint" link to `/board?start=sprint`; with a sprint, "1 more waits in the Backlog for the next sprint."; `count: 0` shows no line.
- `switches_planning_in_sprints` (pages/team.test.tsx, TeamRules): the switch reflects `team.get`; turning it off shows `team.validate`'s effect lines and Save sends `team.save` with `plan_in_sprints: false`.
- `starts_a_team_that_plans_in_sprints` (setup/team.test.tsx): the Finishing work screen shows "Your team works in sprints"; SetupAdvanced's switch is on; `team.start` carries `plan_in_sprints: true`, and `false` once the switch is turned off.

- [x] `feat(web): show the backlog on the board and today, and switch sprint planning in settings and setup`

### Task 6: The sprints journey (Playwright)

Files: created `apps/web/e2e/sprints.spec.ts`; modified `apps/web/e2e/fixtures/serve.ts`: `startServe` takes `sprints?: boolean`, which makes `writeTeam` (false since Task 3) write `true`.

1. `startServe({ team: "pm-architect-developer", sprints: true, transcripts: ["triage_frk_1_small_by_pm", "refine_writes_task_for_theo_frk_1", "judge_frk_1_by_architect", "planning_ceremony_frk_1", "plan_assigns_frk_1_to_theo", "implement_finishes_frk_1", "review_writes_note", "accept_frk_1", "review", "retro"] })`.
2. File the request on Today. Wait until FRK-1 is `ready`. Assert that:
   - the Board's Backlog lane holds FRK-1;
   - Today says "1 piece of work is ready and waits in the Backlog.";
   - after 3 seconds the log has no `task.transitioned` to `assigned`.
3. Today's "Start a sprint" opens the dialog on the Board, which lists FRK-1. Start with "No limit".
4. Assert that the log has `sprint.started`, then `sprint.planned` with FRK-1, then FRK-1 `assigned` to `theo`, in that order, and that the Backlog lane is empty.
5. The run goes on to the sprint's review and look back, as `board.spec.ts`.

Screenshots `sprints-{backlog,start,today}` at 360 and 1440 px, with no sideways scroll at 360.

- [x] `test(web): a request waits in the backlog until the sprint that plans it`

### Task 7: Spec and plan

Under the next free spec revision when this step lands (0.36 if none lands first), each change marked "added in 0.NN":
- 3 (Sprint): the policy, the Backlog, what is held, that work under way at the switch finishes, that a sprint ended early sends its unfinished tasks to the Backlog, that switching the policy off clears the Backlog mark, that ready work waits for the next sprint, that a marked task under an epic that left the Backlog waits for its epic; and (`farik init`) that the starter team plans in sprints;
- 4.1: setup's default and the Finishing work box;
- 4.4: the switch, and that "Put back the default" turns it on;
- 5.2 and 5.5: the assignment gate's sprint rule under the policy;
- 5.7: Backlog rows are not blocked and do not age;
- 5.9: planning's candidates;
- 5.12: `plan_in_sprints` among the team policy keys;
- 8.5: `team.updated` covers the switch, with its `plan_in_sprints` field; `sprint.ended`'s `backlog` field;
- F3: the Board's Backlog lane.

The project plan: step 15's row "Built <date> (spec 0.NN)", and its interface line from this plan's Interfaces. Phase 9 row 06's exemption already names the membership rule (this plan's readiness review, S3).

- [x] `docs(spec): record planning work in sprints`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (farik-core +11 (T2 10, T3 1), farik-runtime +13 (T3 9, T4 4), farik-store +1 and farik-cli +2 from T3);
#   @farik/web: step 14's landed count plus 7 (T5); @farik/protocol-client plus 1;
#   playwright: step 14's landed count plus 1 (sprints.spec.ts);
#   last line: xtask check: ok
```
