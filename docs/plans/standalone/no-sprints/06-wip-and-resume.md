# Standalone CTV-9, plan 6: a stuck agent moves on, and a cleared task resumes

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2, 5.12 (`wip_limit_per_agent`), 8.5; recorded by plan 8 (revision 0.116)
Depends on: plans 1 to 5 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A task that is `blocked`, `escalated` or waiting on the human no longer counts towards its
assignee's WIP limit, so that agent is assigned the next ready task of its role. When the wait
clears, the task resumes with its previous assignee if they have room, else it is handed to the
first free active agent of its role that is not its reviewer, else it waits. A task is never held by
two agents, and nobody reviews their own work. The team's activity shows an agent with a stuck task
and another under way as working. Out of scope: the WIP limit's value, the escalation reasons, and
the private folder's hold (`private_folder_busy` keeps counting a blocked task, as today).

## Decisions

A later change, ADR 0064 (agents work in parallel, in concurrent sessions; `verifying` stops
counting towards the WIP limit; rejected work waits for a free assignee and then goes first),
amends this plan's `open_tasks` and the rule order after it lands. This plan is executed as written;
ADR 0064's own plans change what it counts.

Ada's section 5, with these corrections and additions:

- Counted: a task an agent holds (`assignee_id`) in `assigned`, `in_progress`, `verifying` or
  `rejected`, whose row is not `waiting_on_human`, the epic `under` left out as today. Not counted:
  `blocked`, `escalated`, `accepted`, `cancelled`, and any other status. `has_room` and the
  governor's `AssignmentInput::assignee_open_tasks` both read `transitions::open_tasks`; its
  comment and the field's doc say so.
- "When each last became counted" needs a fact the board does not hold, so the projection gains
  `counted_seq` (added to Ada's design): migration `0016_counted_seq.sql`, `ALTER TABLE
  task_projections ADD COLUMN counted_seq INTEGER;` with no replay; `TaskProjection.counted_seq:
  Option<u64>`. It is set to the event's seq by a `task.transitioned` whose `to` is counted and
  whose `from` is not (`ready`, `blocked` or `escalated` into a counted status), by the event that
  turns `waiting_on_human` from true to false while the status is counted, and by
  `task.reassigned`. A row whose `counted_seq` is `None` (made before the migration) ranks as 0.
- `within_limit(team, board)` answers the ids that rules 3, 6 and 7 may work: for each active agent,
  its counted non-epic rows ordered by (`counted_seq` or 0, the task's number), the first
  `max(wip_limit_per_agent, 1)` of them; and every epic row (an epic's assignee coordinates and is
  never handed over). Chose `max(.., 1)` so that a team that set the limit to 0 to stop new work
  still finishes the task under way, as it does today. A counted non-epic row outside that set is
  "returning".
- The resume rule, `resume_elsewhere`, runs between rule 5 and rule 6 under `All` (rule 6's
  `runs`), over returning rows in `assigned`, `in_progress` or `rejected`, not waiting on the
  human, in scope, in board order. For each: a task whose contract has a `lane` is passed over
  (it waits for its lane's builder, ADR 0051); otherwise the candidate is the first active agent in
  team order whose role is the contract's `assignee_role`, that is not the row's assignee, has room
  (`open_tasks` < the limit), and is not the row's `reviewer_id` (unless
  `Role::reviews_its_own()` holds for that role, as `check_assignment` allows). It asks
  `Transitions::reassign`; a reassignment ends the tick with `TickReport::Acted { what: "handed it
  to <to>: <from> has no room" }`. With no candidate the task waits with its assignee and is tried
  on every tick. When the previous assignee has room again the row is within its limit and simply
  goes on.
- `task.reassigned { from, to, reason: "resumed" }`, about one task (its id on the envelope), its
  body naming no actor (the governor reassigns). The projection sets `assignee_id = to` and
  `counted_seq`; `Transitions::reassign` also writes the contract file's `assignee`, as
  `record_move` does for an assignment. The reviewer is unchanged. The worktree and branch are the
  task's (`.catervas/local/worktrees/<id>`), so the new assignee's next session resumes from the
  last commit and note through `rules::resume`, unchanged.
- `check_reassignment` (core, pure) refuses, each reason in words as `check_assignment`'s are: a
  status other than `assigned`, `in_progress` or `rejected`; a task that is not returning; a task
  with a lane; an epic; a candidate that is not active, does not hold the role, has no room, is the
  current assignee, or is the reviewer (with the `reviews_its_own` exception). A refusal appends
  nothing and the rule passes the row over.
- No two agents: a task has one holder, `assignee_id`; only `ready -> assigned` (behind
  `check_assignment`) and `task.reassigned` (behind `check_reassignment`) set it; one process drives
  a project (ADR 0053) and its tick runs one rule at a time, a session to its end, so no
  reassignment lands while the task's session runs; rules 3, 6 and 7 start sessions only for the
  row's current assignee.
- Activity: after the resting check and before the waiting one, an agent with no session running
  that holds a counted row in `assigned`, `in_progress` or `rejected` within its limit is `working`,
  line `Building <title>`, `task_id` that row (the lowest `counted_seq`), no `session_id`.

## File map

```
docs/schemas/event.schema.json, crates/protocol/src/{lib,event}.rs, src/event/fixtures.rs   modifies: task.reassigned (Task 1)
crates/store/src/migrations/0016_counted_seq.sql, src/migrations.rs   creates, modifies (Task 1)
crates/store/src/projections.rs                      modifies: counted_seq, task.reassigned (Task 1)
crates/runtime/src/transitions.rs                    modifies: open_tasks counts (Task 2); within_limit, reassign (Task 3)
crates/runtime/src/orchestrator/rules.rs             modifies: has_room's doc (Task 2); within_limit filters, resume_elsewhere (Task 3); tests (Tasks 2, 3)
crates/core/src/governor/gates.rs                    modifies: assignee_open_tasks doc (Task 2); ReassignmentInput, check_reassignment (Task 3)
crates/store/src/activity.rs                         modifies: working on a task under way (Task 4)
```

## Interfaces

Consumes: plan 5's `open_tasks(board, agent_id, under)`; `TransitionsDeps`, `record_move`'s pattern,
`Role::reviews_its_own`, `Harness` (`in_progress`, `moved`, `ready`, `recorded`, `row`),
`plan_assigns_ctv_2`, `implement_finishes_ctv_1`, `reads_a_file`.

Produces:

```rust
// catervas_core::governor::gates
pub struct ReassignmentInput {
    pub kind: Kind, pub status: TaskStatus, pub returning: bool, pub has_lane: bool,
    pub from: String, pub to: String, pub to_role: Role, pub to_active: bool,
    pub to_open_tasks: u32, pub wip_limit: u32, pub reviewer_id: Option<String>,
}
/// # Errors
/// Every rule the hand-over breaks, each saying why.
pub fn check_reassignment(contract: &TaskContract, input: &ReassignmentInput) -> GateResult;

// catervas_runtime::transitions
/// The tasks an agent holds and works: assigned, in progress, verifying or rejected, not waiting
/// on the human, the epic `under` left out.
pub(crate) fn open_tasks(board: &[TaskProjection], agent_id: &str, under: Option<&TaskId>) -> u32;
/// The rows rules 3, 6 and 7 may work: each agent's first counted rows up to its limit, and epics.
pub(crate) fn within_limit(team: &Team, board: &[TaskProjection]) -> BTreeSet<TaskId>;
impl Transitions {
    /// Hands `task_id` to `to` after `check_reassignment`: `task.reassigned`, then the contract's assignee.
    pub fn reassign(&self, task_id: &TaskId, to: &str, team: &Team) -> Result<ReassignOutcome, TransitionError>;
}
pub enum ReassignOutcome { Reassigned(CatervasEvent), Refused(Vec<String>) }

// catervas_store
pub struct TaskProjection { /* … */ pub counted_seq: Option<u64> }
```

## Tasks

### Task 1: the event and the counting fact

Files as the map's Task 1. The kind follows `delivery.closed`'s precedent (plan 3), but about one
task; its fixture body `{ "from": "dev-a", "to": "dev-b", "reason": "resumed" }`.

- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` (existing) — watched to fail
  for `task.reassigned`, then passes.
- `keeps_when_a_task_last_began_to_count` (`projections.rs` `mod tests`) — CTV-1 moved `ready ->
  assigned` at seq a, `assigned -> in_progress` at b, `in_progress -> blocked` at c, `blocked ->
  in_progress` at d: `counted_seq` is `Some(a)` after b and `Some(d)` after d; a `question.asked`
  then its `question.answered` at e sets it to `Some(e)`; `task.reassigned { to: "dev-b" }` at f sets
  `assignee_id` to `dev-b` and `counted_seq` to `Some(f)`, and the reviewer is unchanged.

- [ ] `feat(store): task.reassigned, and when a task last began to count`

### Task 2: a stuck task frees its agent

Files: `transitions.rs` (`open_tasks` counts as the Decisions say), `gates.rs` (the field's doc),
`rules.rs` (`has_room`'s doc). Existing tests that counted a blocked task as held
(`leaves_cancelled_work_out_of_the_wip_limit` and any the suite shows) keep their cancelled part and
lose the blocked part.

- `an_agent_whose_task_is_blocked_or_escalated_takes_the_next_ready_task` (rules.rs, ignored "needs
  the git program", epic C2 (b)) — the `Harness` team (Developers `dev-a` and `dev-b`) with
  `wip_limit_per_agent` 1 and `dev-b` holding CTV-3 `in_progress` throughout, so `dev-a` is the only
  Developer who could take CTV-2: CTV-1 held by `dev-a` and moved `in_progress -> blocked`, CTV-2
  `ready`; with the transcript `plan_assigns_ctv_2` (it assigns `dev-a`, reviewer `dev-b`), the first tick runs the plan session of CTV-2
  and CTV-2 is then `assigned` to `dev-a`. The same again with CTV-1 moved `in_progress ->
  escalated`, and again with CTV-1 `in_progress` and a `question.asked` on it unanswered: each time
  CTV-2 goes to `dev-a`. With CTV-1 `in_progress` and no wait, no plan session starts.

- [ ] `feat(runtime): a blocked, escalated or waiting task leaves its agent free`

### Task 3: a cleared task resumes, or is handed over

Files as the map's Task 3: `within_limit` replaces nothing (the `goes_on` filter went in plan 5) and
is applied to rules 3, 6 and 7; `resume_elsewhere`; `reassign`; `check_reassignment` with its
input.

- `a_cleared_task_resumes_with_its_agent_or_another_free_one` (rules.rs, ignored as above, epic C2
  (c); a team of `pm`, `dev-a`, `dev-b`, `dev-c`, limit 1; CTV-1's reviewer `dev-c`) — (1) CTV-1 held
  by `dev-a`, blocked then moved `blocked -> in_progress` by the human, nothing else held: the next
  tick starts an implement session of `dev-a` on CTV-1 and appends no `task.reassigned`. (2) CTV-1
  blocked, then CTV-2 assigned to `dev-a` and moved to `in_progress`, then CTV-1 moved back to
  `in_progress`: the next tick appends one `task.reassigned { from: "dev-a", to: "dev-b", reason:
  "resumed" }`, the row's `assignee_id` is `dev-b` and the contract file's `assignee` is `dev-b`;
  over four more ticks (transcripts `reads_a_file` for each session) every session about CTV-1 is
  `dev-b`'s, every one about CTV-2 is `dev-a`'s, and no second `task.reassigned` is appended. (3) As
  (2) with `dev-b` holding CTV-3 in progress: no `task.reassigned` (the only free Developer,
  `dev-c`, reviews CTV-1), and no session about CTV-1 starts while `dev-a` works CTV-2.
- `refuses_a_hand_over_that_breaks_a_rule` (`core/governor/gates.rs` `mod tests`, pure) — from a
  valid input (`in_progress`, returning, no lane, a task, candidate active, of the role, 0 of 1
  held, not the reviewer) `check_reassignment` is `Ok`; each of: status `blocked`, not returning,
  a lane, an epic, the candidate inactive, of another role, at its limit, the current assignee,
  the reviewer, is refused with one reason; the reviewer is accepted when the role
  `reviews_its_own` and assignee and reviewer roles are the same.

- [ ] `feat(runtime): a cleared task goes back to its agent, or to another free one`

### Task 4: the team's activity

Files: `crates/store/src/activity.rs`.

- `an_agent_with_a_stuck_task_and_another_under_way_is_working` (`activity.rs` `mod tests`) — `dev-a`
  holds CTV-1 `blocked` (a waiting item names `dev-a`) and CTV-2 `in_progress`, no session running:
  `dev-a`'s activity is `Working`, line `Building <CTV-2's title>`, `task_id` CTV-2, `session_id`
  `None`; with CTV-2 `accepted` instead it is `Waiting` as today.

- [ ] `feat(store): an agent working past a stuck task shows as working`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
