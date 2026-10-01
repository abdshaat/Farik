# Phase 6, step 15: Sprints gather ready work

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3 (Sprint), 4.1, 4.4, 5.2, 5.5, 5.7, 5.9, 5.12 (team policy), 8.5, F3
Depends on: steps 01 to 14 of this phase, landed (step 14 recorded at 9512424; step 09's board, sprints and `lanes.ts`; step 14's templates). Added by the project plan's revision 25 on the founder's decision of 2026-10-01 (ADR 0028); the milestone runbook moves to step 16 and waits on this step.
Readiness confirmed by: (pending, one round, against `docs/standards/workflow.md` stage 2)
Mockups approved by: (pending; Task 1)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A team can plan its work in sprints. With "Plan work in sprints" on, the team keeps getting work ready at any time: triage, questions, plans and their check, approvals, and an epic's breakdown. But no task is assigned or built outside the open sprint. Ready work waits in a Backlog that the Board shows as a lane and Today counts, and starting a sprint plans it. Work that becomes ready during a sprint waits for the next one.

The policy is on for every team setup makes, off for every existing team file, and switched in Settings. This is what lets step 16's milestone run gather both requests into one sprint from the browser.

Out of scope:
- the incident fix's exception (ADR 0027), which phase 9 step 06 adds to the same predicate;
- `farik board`'s listing;
- any new event kind.

Binding inputs: ADR 0028 and `docs/design/sprint-backlog.md`. Where this plan differs from the design, the Decisions say so.

## Decisions

- **Mockups first** (the founder's standing gate, 2026-09-26; ADR 0026 D). Task 1's mockups are drawn in the design commit, on the canvas page "Sprints". No later task starts until the founder approves them and this header names the date. Copy settled in them binds Task 5, word for word.
- **The wire:** `policy.plan_in_sprints: boolean`, optional, `default: false`, in `team.schema.json`. A flat key beside the other policy keys, over a nested `policy.sprints` object, because every policy key so far is flat. `Team::plans_in_sprints()` reads an absent key as `false`, so no old file changes behaviour.
- **Who writes `true`:**
  - `team.propose`'s policy and `settings.defaults`;
  - so `team.start` saves it from setup.
  - `farik init`'s starter team leaves it out, so the command line flows as phase 4 tested it (the design's open question 1; the founder may reverse it before readiness).
- **One predicate, in `farik-core`:** `waits_for_a_sprint`. The gate, the orchestrator's rules 3, 6, 7 and 8, and the Backlog all read it. The policy is checked in one place, and phase 9 step 06 adds the incident exception there.
- **An epic's assignment is preparation:**
  - under the policy, an epic passes the sprint membership rule, so its breakdown runs outside a sprint;
  - the sprint budget is checked only for a row in the open sprint;
  - the old exception "a task under an epic in no sprint" does not hold under the policy;
  - off, `in_the_open_sprint` is unchanged.
- **What is held:**
  - assigning a task (rule 8 and the gate);
  - starting it (rule 7);
  - implement sessions (rule 6);
  - rework after a rejection (rule 3).

  Review and verification (rule 5), integration (rules 1 and 2), epic plan sessions, triage, refining, the plan check, chats, the channel's conversation rule and the ceremonies run at any time. A running session is never stopped.
- **The Backlog:** a row with the policy on, not in the open sprint, in `ready`, `assigned`, `in_progress` or `rejected`. `blocked`, `escalated` and `verifying` keep their lanes. Nothing in the Backlog changes status, so the blocked-age rule, escalation aging and the digest never count the wait.
- **Planning's candidates under the policy:**
  - the Backlog's rows with no parent;
  - `farik_plan_sprint` accepts those statuses for an assigner's plan;
  - off, both stay "`ready`, no parent, in no sprint".

  A task filed under an epic in the open sprint still joins it.
- **A sprint ended early under the policy:** its unfinished tasks wait in the Backlog. Over "work in progress keeps going", because the founder's rule is "no task is built outside an open sprint". The dialog's sentence changes to match.
- **No new event kind:**
  - the switch is `team.save`, which records `team.updated`;
  - holding a task records nothing, as a full agent's does not;
  - `describe_change` gives the switch's effect lines.
- **The Board reads the daemon:** `tasks.list` rows gain `backlog: boolean`, worked out by `in_the_backlog`. `laneOf` does not compute it again.
- **Today's count** comes from a new query, `backlog.summary {}`, over making Today load `tasks.list`.
- **The idle line:** a tick with Backlog work and no open sprint says "the ready work waits for a sprint". `farik run`'s closing list adds "start a sprint: <n> waits in the Backlog (`farik sprint start`)".
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
crates/runtime/src/transitions.rs                                       modifies (T3: AssignmentInput's plan_in_sprints)
crates/runtime/src/orchestrator/rules.rs, orchestrator/requests.rs      modifies (T3: rules 3, 6, 7, 8; candidates; idle line)
crates/runtime/src/sprints.rs, orchestrator/messages.rs                 modifies (T3: plan_sprint's statuses; planning message)
crates/cli/src/run.rs                                                   modifies (T3: the closing list)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/{web.rs,gates.rs,team.rs}   modifies (T4)
packages/protocol-client/src/client.ts                                  modifies (T4)
apps/web/src/app/lanes.ts, apps/web/src/pages/{Board,Today,TeamRules}.tsx (+ tests), strings/en.ts   modifies (T5)
apps/web/src/pages/setup/{SetupFinish,SetupAdvanced}.tsx, setup/team.test.tsx   modifies (T5)
apps/web/e2e/sprints.spec.ts, apps/web/e2e/fixtures/serve.ts          creates / modifies (T6: the `sprints` option)
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
pub struct SprintHold<'a> { pub plan_in_sprints: bool, pub open_sprint: Option<&'a str>, pub kind: Kind, pub status: TaskStatus, pub sprint: Option<&'a str> }
pub fn waits_for_a_sprint(hold: &SprintHold<'_>) -> bool;   // policy on, kind Task, sprint != open (or none open)
pub fn in_the_backlog(hold: &SprintHold<'_>) -> bool;       // policy on, sprint != open, status in {ready, assigned, in_progress, rejected}
pub fn in_the_open_sprint(kind: Kind, input: &AssignmentInput) -> bool;   // gains kind
AssignmentInput.plan_in_sprints: bool
// farik-runtime
const WAITS_FOR_A_SPRINT: &str = "the ready work waits for a sprint";   // orchestrator/rules.rs
```

Wire (`rpc.schema.json`; `camelCase` in `protocol-client`):
- `tasks.list` rows gain `backlog: boolean`;
- query `backlog.summary {}` → `{ plan_in_sprints: boolean, count: integer }`, where `count` is the Backlog's rows with no parent;
- `team.yaml` `policy.plan_in_sprints`; `templates/<slug>.yaml` `policy.plan_in_sprints`.

## Tasks

### Task 1: Mockups, approved by the founder

Files (drawn on 2026-10-01 in `docs(design): mock up the sprint backlog for the founder's approval`):
- `BoardBacklog`: the Board with a Backlog lane, no sprint and work waiting; "Start sprint 3" listing the Backlog; a sprint running with one late request waiting for the next;
- `TodayBacklog`: Today's line in both states;
- `SettingsSprints`: "Planning work" with the switch, on, then off with "What this changes";
- `SetupSprints`: the Finishing work box and SetupAdvanced's switch.

All in the canvas's "Sprints" page, in `@farik/brand`'s tokens only, one colour per job. No test. The gate is the founder's approval, recorded in this header in the commit that ticks this box.

- [ ] `docs(plans): record the founder's approval of the sprint backlog mockups`

### Task 2: The policy and its predicates (`farik-core`)

Files and produces: as the file map and Interfaces say.

- `reads_an_absent_policy_as_off`: the base fixture without the key gives `plans_in_sprints() == false`; with `plan_in_sprints: true` gives `true`; the schema refuses `"yes"` at `/policy/plan_in_sprints`.
- `holds_a_task_outside_the_sprint`: policy on: a `Task` with no sprint open is held; one in `S1` with `S1` open is not; one in no sprint with `S1` open is held. Policy off: never held.
- `never_holds_an_epic`: policy on, an `Epic` in no sprint with or without a sprint open, `waits_for_a_sprint` is false.
- `places_rows_in_the_backlog`: policy on, no sprint open: `ready`, `assigned`, `in_progress`, `rejected` are in; `draft`, `refining`, `escalated`, `blocked`, `verifying`, `accepted`, `cancelled` are out. A row in the open sprint is out. Policy off: every row is out.
- `assigns_an_epic_outside_the_sprint_under_the_policy`: `check_assignment` passes an approved epic with no sprint open, and with `S1` open and the epic in none, with a sprint budget of $1 and an epic budget of $5; off, the second case refuses as today.
- `refuses_a_held_task_in_plain_words`: policy on, no sprint open: `check_assignment` fails with "this team plans work in sprints, and FRK-1 waits in the Backlog until a sprint plans it".
- `drops_the_unsprinted_epic_exception_under_the_policy`: a task with `parent_sprint: Some(None)` while `S1` is open passes with the policy off and fails with it on.
- `carries_the_policy_in_a_template`: `template_from_team` writes `plan_in_sprints: true` for a team with it; `apply_template` with the key absent keeps the project's `true`, and with `false` sets `false`; a template without the key validates.
- `describes_the_switch`: `describe_change` from off to on gives "Ready work now waits in the Backlog until you start a sprint."; on to off gives "Ready work starts as soon as someone is free, without waiting for a sprint." and "You can still start sprints from the Board.".

- [ ] `feat(core): let a team plan its work in sprints, holding ready tasks in a backlog`

### Task 3: The orchestrator holds work and plans the Backlog (`farik-runtime`, `farik-cli`)

Consumes: Task 2.

- `holds_a_ready_task_until_a_sprint_opens` (rules.rs): policy on, FRK-1 `ready`, no sprint: ticks are idle with `WAITS_FOR_A_SPRINT`, no `plan` session starts, FRK-1 stays `ready` and no `escalation.raised` is recorded.
- `breaks_an_epic_down_outside_a_sprint`: policy on, an approved epic FRK-2, no sprint: Farik assigns it to `sm`, the transcript `plan_breaks_down_frk_1` breaks it down, and every task under it reaches `ready` and none is assigned on the ticks that follow.
- `plans_both_on_the_first_tick_of_the_sprint`: policy on, FRK-1 `ready` and the broken-down epic FRK-2 waiting; the human starts S1; the first tick is S1's planning ceremony, its message lists FRK-1, and FRK-2 with the number of tasks under it, and after `farik_plan_sprint [FRK-1, FRK-2]` FRK-1, FRK-2 and every task under FRK-2 are in S1. The readiness review's B1 sequence, without pausing.
- `keeps_late_work_for_the_next_sprint`: policy on, S1 planned and running; FRK-3 becomes `ready`: it is not assigned and not in S1, while S1's own tasks are; after S1 ends and S2 starts, S2's planning lists FRK-3.
- `stops_unfinished_work_of_an_ended_sprint`: policy on, FRK-1 `assigned` in S1; `sprint_end`: FRK-1 leaves S1, and no `implement` session starts for it until S2 plans it.
- `runs_review_and_integration_outside_a_sprint`: policy on, no sprint, FRK-1 `verifying` from before: its review session runs; an accepted task integrates.
- `flows_as_before_with_the_policy_off`: the existing `assigns_the_backlog_without_a_sprint` passes unchanged, with the key absent.
- `plans_a_backlog_epic_under_way` (sprints.rs): policy on, `plan_sprint` of an `in_progress` epic in no sprint is accepted and brings its tasks; off, it is refused with "only a ready task or an approved epic is planned".
- `says_the_backlog_waits` (cli run.rs): `farik run` on a policy-on project with FRK-1 `ready` exits idle and prints "start a sprint: 1 waits in the Backlog (`farik sprint start`)".

- [ ] `feat(runtime): hold work outside the open sprint and plan the backlog when one starts`

### Task 4: The Backlog over the wire

Files: as the file map says.

- `marks_backlog_rows` (daemon/web.rs): `tasks.list` on a policy-on project answers `backlog: true` for a ready FRK-1 with no sprint, `false` after S1 plans it; every row `false` with the policy off.
- `summarises_the_backlog` (daemon/gates.rs): `backlog.summary` answers `{ plan_in_sprints: true, count: 2 }` for a ready task and a broken-down epic with three tasks; `{ plan_in_sprints: false, count: 0 }` with the policy off.
- `proposes_sprints_for_a_new_team` (daemon/team.rs): `team.propose` and `settings.defaults` answer `policy.plan_in_sprints: true`; `team.start` saves it into `team.yaml`.
- The protocol client (not counted above): `client.test.ts` maps `plan_in_sprints` → `planInSprints` and the row's `backlog`.

- [ ] `feat(runtime): answer which work waits in the backlog, and propose sprints for new teams`

### Task 5: The pages

Every test also runs axe (step 06's rule). Copy from the approved mockups.

- `places_backlog_rows_in_their_lane` (lanes.test.ts): `laneOf` answers `backlog` for a row with `backlog: true` in each of the four statuses, and the step 09 mapping for every row without it; `LANES` is Planning, Backlog, To do, In progress, Stuck, Review, Done.
- `shows_the_backlog_lane_only_under_the_policy` (Board.test.tsx): with `planInSprints` true the lane, its note and the sprint line "No sprint is running. Ready work waits in the Backlog until you start one." show; with it false there is no Backlog lane and the line is step 09's.
- `lists_the_backlog_when_starting_a_sprint`: the start dialog lists FRK-12 "Epic, 3 tasks" and FRK-14 "Task" under "Waiting in the Backlog"; `/board?start=sprint` opens it.
- `says_what_ending_early_does_under_the_policy`: the end-early dialog says the tasks "wait in the Backlog for the next one".
- `counts_the_backlog_on_today` (Today.test.tsx): `backlog.summary` `{ count: 2 }` and no sprint shows "2 pieces of work are ready and wait in the Backlog." with a "Start a sprint" link to `/board?start=sprint`; with a sprint, "1 more waits in the Backlog for the next sprint."; `count: 0` shows no line.
- `switches_planning_in_sprints` (pages/team.test.tsx, TeamRules): the switch reflects `team.get`; turning it off shows `team.validate`'s effect lines and Save sends `team.save` with `plan_in_sprints: false`.
- `starts_a_team_that_plans_in_sprints` (setup/team.test.tsx): the Finishing work screen shows "Your team works in sprints"; SetupAdvanced's switch is on; `team.start` carries `plan_in_sprints: true`, and `false` once the switch is turned off.

- [ ] `feat(web): show the backlog on the board and today, and switch sprint planning in settings and setup`

### Task 6: The sprints journey (Playwright)

Files: created `apps/web/e2e/sprints.spec.ts`; modified `apps/web/e2e/fixtures/serve.ts`: `startServe` takes `sprints?: boolean`, and `writeTeam` then adds `  plan_in_sprints: true` under `policy:`.

1. `startServe({ team: "pm-architect-developer", sprints: true, transcripts: ["triage_frk_1_small_by_pm", "refine_writes_task_for_theo_frk_1", "judge_frk_1_by_architect", "planning_ceremony_frk_1", "plan_assigns_frk_1_to_theo", "implement_finishes_frk_1", "review_writes_note", "accept_frk_1", "review", "retro"] })`.
2. File the request on Today. Wait until FRK-1 is `ready`. Assert that:
   - the Board's Backlog lane holds FRK-1;
   - Today says "1 piece of work is ready and waits in the Backlog.";
   - after 3 seconds the log has no `task.transitioned` to `assigned`.
3. Today's "Start a sprint" opens the dialog on the Board, which lists FRK-1. Start with "No limit".
4. Assert that the log has `sprint.started`, then `sprint.planned` with FRK-1, then FRK-1 `assigned` to `theo`, in that order, and that the Backlog lane is empty.
5. The run goes on to the sprint's review and look back, as `board.spec.ts`.

Screenshots `sprints-{backlog,start,today}` at 360 and 1440 px, with no sideways scroll at 360.

- [ ] `test(web): a request waits in the backlog until the sprint that plans it`

### Task 7: Spec and plan

Under the next free spec revision when this step lands (0.36 if none lands first), each change marked "added in 0.NN":
- 3 (Sprint): the policy, the Backlog, what is held, and that ready work waits for the next sprint;
- 4.1: setup's default and the Finishing work box;
- 4.4: the switch;
- 5.2 and 5.5: the assignment gate's sprint rule under the policy;
- 5.7: Backlog rows are not blocked and do not age;
- 5.9: planning's candidates;
- 5.12: `plan_in_sprints` among the team policy keys;
- 8.5: `team.updated` covers the switch;
- F3: the Board's Backlog lane.

The project plan: step 15's row "Built <date> (spec 0.NN)", and its interface line from this plan's Interfaces.

- [ ] `docs(spec): plan work in sprints, with ready work waiting in a backlog`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (farik-core +9 from T2, farik-runtime +11 from T3 (8) and T4 (3), farik-cli +1 from T3);
#   @farik/web: step 14's landed count plus 7 (T5); @farik/protocol-client plus 1;
#   playwright: step 14's landed count plus 1 (sprints.spec.ts);
#   last line: xtask check: ok
```
