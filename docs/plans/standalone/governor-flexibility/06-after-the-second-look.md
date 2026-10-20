# Governor flexibility, plan 6: after the second look

Status: ready
Branch: `fix/governor-flexibility`, after plan 5
Spec and decisions: ADR 0079, "Amended after the live run" (second list); spec 0.134 amended in place

The v2 build ran from 06:22 UTC. CTV-22 was accepted and merged and CTV-12 started; then three gaps held the parallel work.

## Decisions

- R5 (web): `TaskDetail` does not wait on `task.diff` when it answers with an error: a task with no branch shows its page, and the Changes tab says "No changes yet: the work starts when it is assigned." (`taskNoBranchYet`). Every other query still gates the page as now.
- R6 (core, web): the owner may send a `ready` task back to its plan. New table row `Ready → Refining`, actor `Human`, gate `None`; the human's words are required (`transition` already calls `written`). The task page of a `ready` task shows "Change the plan" (`taskChangePlan`); it opens a note ("What should change?", required) and sends `task_transition { task_id, to: "refining", reason }`. ADR 0079 decision 3 then gives the Product Manager a refine session with those words.
- R7 (runtime): in a Product Manager session whose task is an epic (purposes `plan` and `refine`), `catervas_write_contract` and `catervas_request_transition` take an optional `task_id`: a child of that epic whose status is `draft`, `refining` or `ready`. Without `task_id` they act on the session's task as now. Any other `task_id` is refused with `Refusal::Invalid("task_id: <id> is not a waiting task of <epic>")`. The gates decide the rest as for a session of that task (decision 4's writes; a move the table allows).
- The PM's prompt for an epic's `plan` session says: "To change a waiting task of this epic, pass its `task_id` to `catervas_write_contract` or `catervas_request_transition`."

## Tasks

### Task 26: a task with no branch opens

Files: `apps/web/src/pages/TaskDetail.tsx`, `TaskDetail.test.tsx`, `strings/en.ts`
- `opens_a_task_that_has_no_branch_yet` — `task.diff` answers an error, every other query answers; the page shows the task's title and, on the Changes tab, "No changes yet".
- [x] `fix(web): a task with no branch yet opens`

### Task 27: the owner sends a ready task back to its plan

Files: `crates/core/src/governor/transition_table.rs` (and its tests' counts), `apps/web/src/pages/TaskDetail.tsx`, `TaskDetail.test.tsx`, `strings/en.ts`
- `the_owner_sends_a_ready_task_back_to_its_plan` (core) — `find_transitions(Ready, Refining)` includes `(Human, None)`; a human move with words is allowed.
- `sends_a_ready_task_back_to_its_plan` (web) — on a ready task, "Change the plan" is disabled until the note has text, then sends `task_transition` with `to: "refining"` and the note as `reason`.
- [x] `feat(web): the owner sends a ready task back to its plan`

### Task 28: the Product Manager changes an epic's waiting tasks from the epic's session

Files: `crates/runtime/src/tools/contracts.rs`, `crates/runtime/src/tools/work.rs`, `crates/runtime/src/tools.rs` (input structs), the plan prompt in `crates/runtime/src/orchestrator/messages.rs`
- `writes_a_waiting_task_of_the_epic_from_its_session` (contracts.rs) — a PM `plan` session on epic CTV-1 writes `dependencies` of its ready child CTV-3 with `task_id: "CTV-3"`; CTV-3's contract changes and CTV-1's does not. The same with `task_id` of a task outside CTV-1, or of an `in_progress` child, is refused with "is not a waiting task of".
- [x] `feat(runtime): the Product Manager changes an epic's waiting tasks from the epic's session`

### Task 29: the spec and the ADR say it

Files: `docs/SPEC.md` (5.2 row, 5.11, 5.16 or wherever the epic session's tools are stated, the 0.134 line)
- [x] `docs(spec): a task with no branch opens, the owner sends a ready task back, the PM changes an epic's waiting tasks`
