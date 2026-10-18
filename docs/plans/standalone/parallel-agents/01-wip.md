# Standalone (parallel agents), plan 1: a task under review frees its agent

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2 (`ready -> assigned`, the WIP limit), 5.12 (`wip_limit_per_agent`); recorded by plan 5 (revision 0.118)
Depends on: CTV-9 plans 1 to 8 (`feat/no-sprints`, rebased under this branch; see the README)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A Developer whose task is in `verifying` has room for its next task: `verifying` stops counting
towards an agent's WIP limit, in the orchestrator's `has_room`, the governor's assignment gate and
CTV-9's `within_limit`. A task the reviewer sends back counts again from the moment of its
rejection, so its author takes no new work while it waits, and it goes back to its author, never to
another agent. Out of scope: running sessions at once (plan 2), the private folder's hold (still
counts a task in `verifying`, SPEC 6.6), and the limit's value.

## Decisions

- One definition of what counts, in core: `catervas_core::governor::task_status::counts_towards_wip
  (status: TaskStatus) -> bool`, true for `Assigned`, `InProgress` and `Rejected`, false for every
  other status. CTV-9 plan 06 listed its counted statuses (`assigned`, `in_progress`, `verifying`,
  `rejected`) where it needed them: in `transitions::open_tasks`, in `transitions::within_limit`,
  and in the projection's `counted_seq` rule (`crates/store/src/projections.rs`). Each of those
  lists is replaced by a call to `counts_towards_wip`; the waiting-on-the-human and epic exclusions
  of plan 06 stay where they are. Chose one function over three edited lists because three lists
  drift (ADR 0064).
- **Amends CTV-9 plan 06, decision "Counted":** `verifying` is no longer counted. Chose this over
  keeping it (the founder: "While the review is happening, the developer may pickup another task").
  Plan 06's reason for counting it (SPEC 0.3: the limit could be "walked through `verifying`") no
  longer holds the way it did: a task under review is the reviewer's work, and when it comes back it
  counts again.
- **Amends plan 06's `counted_seq` rule by consequence:** since `verifying` is not counted, the move
  `verifying -> rejected` goes from not counted to counted and sets `counted_seq` to the rejection's
  seq. So a task sent back while its author works on a newer one ranks after it in `within_limit`,
  and is the one within the limit once the newer one goes to review: that is the founder's "after
  finishing the second task, he then may go back and fix the first". Chose this over keeping the
  task's first `counted_seq` (it would rank first and, with a limit of one, take the limit from the
  task under way).
- **Amends plan 06, decision "The resume rule":** `resume_elsewhere` takes returning rows in
  `assigned` or `in_progress` only, no longer `rejected`. A sent-back task waits for its author.
  Chose this over handing it to the first free agent of its role (ADR 0064: the author knows the
  work and the reviewer's note); blocked, escalated and waiting tasks still move on as plan 06 says.
  `check_reassignment`'s refusal of a status other than `assigned`, `in_progress` or `rejected`
  becomes a refusal of a status other than `assigned` or `in_progress`, with the same words shape:
  `"<task> is <status>: only an assigned or in-progress task is handed over"`.
- The governor's `AssignmentInput::assignee_open_tasks` doc says what it counts now: "The tasks the
  assignee holds that count towards its limit (`counts_towards_wip`: assigned, in progress, sent
  back), not one waiting on the human, the epic under assignment left out. A task under review is
  its reviewer's work and is not counted (ADR 0064)." `has_room`'s and `open_tasks`'s docs say the
  same in one line each.
- Existing tests that filled an agent's limit with a `verifying` task: the suite names them when
  Task 1's change lands; each fills the limit with an `in_progress` task instead (`harness.in_progress`
  in place of `harness.verifying`) and keeps its assertion. A test that asserted a `verifying` task
  counts is changed to assert it does not, with the commit saying why.
- No new event, migration or wire field: `counted_seq` and the projection exist (plan 06 Task 1).
  No ADR beyond 0064.

## File map

```
crates/core/src/governor/task_status.rs           modifies: counts_towards_wip, mod tests (Task 1)
crates/core/src/governor/gates.rs                 modifies: assignee_open_tasks doc; check_reassignment's status refusal (Task 1)
crates/runtime/src/transitions.rs                 modifies: open_tasks and within_limit call counts_towards_wip (Task 1)
crates/store/src/projections.rs                   modifies: counted_seq's rule calls counts_towards_wip (Task 1)
crates/runtime/src/orchestrator/rules.rs          modifies: has_room's doc; resume_elsewhere's statuses; mod tests (Task 1)
```

## Interfaces

Consumes: from CTV-9 plan 06 (on this branch): `transitions::open_tasks(board, agent_id, under)`,
`transitions::within_limit(team, board)`, `rules::resume_elsewhere`, `check_reassignment`,
`ReassignmentInput`, `TaskProjection.counted_seq`, `EventKind::TaskReassigned`. On `main`:
`Harness` (`new`, `ready`, `in_progress`, `verifying`, `rejected`, `recorded`, `orchestrator`,
`row`, `events`), `TickScope`, `TickRules::Planning`, transcripts `plan_assigns_ctv_2` and
`reads_a_file`, `an_agent_wire`.

Produces:

```rust
// catervas_core::governor::task_status
/// Whether a task in `status` counts towards its assignee's WIP limit (ADR 0064): assigned, in
/// progress, or sent back. Not `verifying`, which is its reviewer's work.
#[must_use]
pub fn counts_towards_wip(status: TaskStatus) -> bool;
```

## Tasks

### Task 1: a task under review frees its agent, a sent-back one holds it

Files as the map. `counts_towards_wip` is pure and `catervas-core` does no I/O (hard rule 5).

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `an_agent_whose_task_is_under_review_takes_the_next_task` (`rules.rs` `mod tests`, `#[ignore =
  "needs the git program: cargo xtask check --integration"]`; watched to fail at part (1), where no
  plan session starts because `CTV-1` in `verifying` fills `dev-a`'s limit) — the `Harness` team
  (`pm`, `dev-a`, `dev-b`, limit 1) plus `dev-c` (`an_agent_wire("dev-c", "software_developer")`
  pushed onto `wire["agents"]` in `Harness::new`'s change):
  (1) `harness.verifying("CTV-1")` (held by `dev-a`, reviewed by `dev-b`),
  `harness.in_progress("CTV-3", "dev-b", "pm")`, `harness.in_progress("CTV-4", "dev-c", "pm")`,
  `harness.ready("CTV-2")`; one `tick_within(&TickScope { task_id: None, rules:
  TickRules::Planning })` with the transcript `plan_assigns_ctv_2` runs one session, the
  Product Manager's `plan`, and then `harness.row("CTV-2")` is `assigned` with `assignee_id`
  `Some("dev-a")`.
  (2) A fresh harness of the same team with `harness.rejected("CTV-1", 0, "C1 failed")` in place of
  `verifying`, and CTV-3, CTV-4 and CTV-2 as in (1): the same Planning tick, with the same
  transcript given, starts no session (`adapter.started()` is empty) and CTV-2 stays `ready`.
  (3) A fresh harness of the same team: `harness.in_progress("CTV-2", "dev-a", "dev-b")` first,
  then `harness.rejected("CTV-1", 0, "C1 failed")` (`dev-a`, reviewed by `dev-b`), so `dev-c` is a
  free Developer who reviews neither; one `tick()` with the transcript `reads_a_file` appends no
  `task.reassigned`, leaves CTV-1 `rejected` with `assignee_id` `Some("dev-a")`, and its one
  session is `dev-a`'s `implement` about CTV-2.
- `counts_assigned_in_progress_and_rejected_towards_wip` (`task_status.rs` `mod tests`) — for each
  status of `TaskStatus`, `counts_towards_wip` is true exactly for `Assigned`, `InProgress` and
  `Rejected`.

- [x] `feat(runtime): a task under review frees its agent, a sent-back one waits for it`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
