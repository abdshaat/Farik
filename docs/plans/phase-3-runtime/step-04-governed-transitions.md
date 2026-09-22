# Phase 3, step 04: Governed transitions

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.3, 5.4, 5.7, 8.4
Depends on: step 03 of this phase (`budget_state`, the cost projections); phase 2 (log, projections, files, git adapter, on main); phase 1 (`evaluate_transition`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the one decision the first found open; findings folded in)

## Goal

A request to move a contract, from an agent or the human, is judged by the governor on facts Farik reads from its own store, and the answer is recorded either way: a move is a `task.transitioned` event and the contract file follows it, a refusal is a `transition.refused` event with every reason, an escalation the move causes is an `escalation.raised` event, and every Definition of Ready or Done evaluation is a `contract.evaluated` event. The board shows each task's assignee, reviewer, and iteration. Out of scope: who asks (the tools, step 05; the orchestrator, step 11; the human's commands, step 12), and the facts whose events do not exist yet, listed below.

## Decisions

- `farik-runtime::transitions::Transitions` holds the log, the projections, the files, the git adapter, the clock, and a `Mutex<()>` taken for the whole of `request`, because reading the context, writing the file, and appending the event is a read-check-write, and the tools (step 05) and the orchestrator (step 11) call it at once; the lock covers one process, which is what phase 3 runs (the daemon is in-process); `request` takes the governor's `TransitionRequest`, what the requester brings (`TransitionAsk`), and the team, and answers `Moved` or `Refused`. `TransitionError` is only for the store, the files, git, or costs failing, never for a refusal, which is a value (code.md).
- What the requester brings, `TransitionAsk`: the assignee and reviewer ids for an assignment, a blocker, a blocker resolution, a rejection, and whether a permission was denied. Everything else in `TransitionContext` is read from the store, never taken from the requester, because a requester that supplies its own evidence is not governed.
- A `ready → assigned` request whose ask lacks either id, or names an id that is not an active agent of the team, is refused before the governor is asked, as `TransitionRefusal::GateFailed` with one `GateFailure { gate: Assignment, details }` naming the missing or unknown or inactive id, and recorded as a `transition.refused`; `check_assignment` itself does not ask whether an agent is active. One exception: an epic on a team with no active Scrum Master is the Product Manager's, reviewed by the human (5.16 item 4), who has no agent id, so there the reviewer id may be absent, and `task.transitioned.reviewer` and the board's `reviewer_id` are then none. The claimed-role check below runs before this one, so a request failing both records the claimed-role refusal.
- An actor claimed as `ProductManager` or `ScrumMaster` is checked against the team: the `agent_id` must be an active agent with that role, else the request is refused with `TransitionRefusal::NotTheNamedAgent { actor, named: None, asked }` before the governor is asked. `Assignee` and `Reviewer` are already checked by `evaluate_transition` against the contract; `Human` and `Governor` come from Farik's own code.
- Where each fact in the context comes from:
  - `contract`: the file (`ProjectFiles::read_contract`), with `status`, `assignee`, `reviewer`, and `iteration` overwritten from the board, because 8.4 makes the log the source of truth for what happened.
  - `triaged`, `children` (the board's rows whose `parent` is this task): the board.
  - `readiness.remaining_sprint_budget_usd`: the day's remaining budget from `budget_state` (the phase decision); `dependency_statuses`: the board; `active_agents_by_role`: the team's active agents; `parent`: the parent's board row, file `allowed_paths`, and remaining budget, which is the epic's `max_cost_usd` less its own `cost_usd` less the `max_cost_usd` of its other children that are not `cancelled`, so that children readied one at a time cannot add up past the epic (5.16: "within the epic's remaining budget"); `rules`: `team.rules()`; `requires_judgment_review`: `team.has_active(ScrumMaster)` (D2); `judgment_review`: `None` until phase 4 step 01 records one.
  - `readiness_failed_attempts`: the `contract.evaluated` events with `gate: definition_of_ready` and `passed: false` for this task since the later of its last `task.transitioned` into `refining` and its last `request.triaged` (a re-triage from `small` to `large` starts refining over, step 05), or since its first event when there is neither. A contract that passes the structural checks but waits for the human also fails that gate and so counts; this is intended, because `ReadinessExhausted` re-checks the contract and escalates only one that still fails.
  - `acceptance.required_by_policy`: the team's `human_accepts_contracts` is `all` (`high_risk` is already `requires_human_acceptance`); `acceptance.given`: `false` until step 12 adds `human.accepted`.
  - `assignment`: from the ask's two ids, with their roles from the team; `has_active_scrum_master`: `team.has_active(ScrumMaster)`; `remaining_sprint_budget_usd`: the same day-remaining figure as `readiness`; `assignee_open_tasks`: the board's tasks assigned to that agent whose status is neither `accepted` nor `cancelled`; `wip_limit`: `wip_limit_per_agent`; `dependencies[].integrated`: `false` until step 11 adds `task.integrated`.
  - `assignee_results` and `done.results`: empty until step 05 adds `criterion.recorded`; `done.completion_note` and `done.review_note`: `None` until step 05 adds `note.written`; `done.human_accepted`: `false` until step 12.
  - `work` and `done.changed_paths`: from git only when the task's worktree `.farik/local/worktrees/<id>` exists (`commit_count` and `changed_paths` from the integration branch to `farik/<id>`, `is_clean` on the worktree); otherwise zero commits, not clean, and no paths, which refuses `verifying` truthfully. The integration branch is `policy.integration_branch`, else `Git::default_branch`, resolved only then, so that no git error can arise for a task with no worktree.
  - `blocker`, `blocker_resolution`, `rejection`, `permission_denied`: the ask. `blocked_at`: the `recorded_at` of the task's last `task.transitioned` into `blocked`. `now`: the clock. `blocked_limit`: `blocked_limit_hours`.
  - `budget`: `budget_state` for the contract's `assignee_role` and the contract, with an empty `SessionLedger`, because a transition is judged outside any one session's running count; the orchestrator (step 11) acts on session limits itself.
  - Iterations: the contract's `budget.max_iterations` governs, as `farik-core` reads it; the team's `policy.max_iterations` is the default an author writes into a new contract and is not read here.
  The facts marked "until step N" are what the log holds today, not placeholders: the step that adds the event widens `context`.
- A move is recorded in the order phase 2's commands use: the contract file is written with the new `status`, `assignee`, `reviewer`, and `iteration` (and `updated_at`), then `task.transitioned` is appended and applied to the board. `task.transitioned` body: `{ from, to, actor, requested_by, gate, effects, assignee, reviewer, iteration, blocker?, blocker_resolution?, rejection? }`, the last three optional and copied from the ask (`blocker { description, needed }`, `rejection { failed_criterion_ids, reasons }`) so that the evidence a move was judged on is in the log for the user and the next iteration, where `requested_by` is the agent id or `human` or `governor`, `effects` the snake_case names of the decision's `TransitionEffect`s (`raise_escalation` carries no reason; the reason is on the `escalation.raised` that follows), and the last three are the contract's after the move.
- Effects: `IncrementIteration` adds one to `iteration` before it is written; `RaiseEscalation(reason)` appends `escalation.raised { reason, detail }` after the move, `detail` a string, the gate's snake_case name, or `<gate>: <words>` when there are words: the ask's rejection `reasons`, or the blocker `description`, which for `blocked → escalated` (asked by the governor with an empty ask) is read from the task's last move into `blocked`; `ResetBlocker` and `StampBlockedAt` are recorded in `effects` only, since `blocked_at` is read back from the event's `recorded_at`.
- A refusal is `transition.refused { from, to, actor, requested_by, refusal, details }`, `refusal` naming the `TransitionRefusal` variant in snake_case and `details` every gate failure's words, or, for the refusals that are not `GateFailed`, one sentence of the refusal's own ("not the named agent: asked dev-a"); nothing else changes.
- Envelopes: `task_id` is always the request's; `agent_id` is the request's `agent_id` when an agent asked, else none; `session_id` the ask's `session_id` (the tools, step 05, set it; the orchestrator and the human leave it none).
- `contract.evaluated { gate: definition_of_ready | definition_of_done, passed, failures }` is appended whenever the decided row's gate, or any tried row's gate, was `DefinitionOfReady` or `DefinitionOfDone`, before the `task.transitioned` or `transition.refused` it explains, so that the readiness-attempt count above reads it.
- `TaskProjection` gains `assignee_id: Option<String>`, `reviewer_id: Option<String>`, `iteration: u32`, fed by `task.transitioned`. `sprint_id` is cut until phase 4, which has the sprints (revision 8 listed it). Migration `0004_transitions.sql` adds the three columns.
- Tests of Tasks 3 and 4 need a git repository, so they are `#[ignore = "needs git"]` as phase 2's git tests are, and run under `--integration`; this machine has git, so they are watched to fail and pass here.
- `task.transitioned`, `transition.refused`, `escalation.raised`, and `contract.evaluated` are all in `is_about_one_contract`; their `attribution` is `requested_by` for the first two, `None` for the other two. `docs/SPEC.md` 8.5 gains `transition.refused` and `contract.evaluated`, which it does not list.

## File map

```
docs/schemas/event.schema.json                 modifies: the four kinds
crates/protocol/src/event.rs                   modifies: wiring for the four kinds; tests
crates/protocol/src/event/fixtures.rs          modifies: four arms of a_body_wire
crates/store/src/migrations/0004_transitions.sql creates: the three board columns
crates/store/src/migrations.rs                 modifies: the fourth migration
crates/store/src/projections.rs                modifies: apply task.transitioned; the three fields; tests
crates/runtime/src/transitions.rs              creates: Transitions, TransitionAsk, TransitionOutcome, TransitionError; tests
crates/runtime/src/lib.rs                      modifies: `pub mod transitions;`
docs/SPEC.md                                   modifies: 8.5, the two kinds
docs/plans/project-plan.md                     modifies: step 04's interface line to match
```

## Interfaces

Consumes: `budget_state`, `CostError` (step 03); `evaluate_transition`, `TransitionRequest`, `TransitionContext`, `TransitionDecision`, `TransitionRefusal`, `TransitionEffect`, `Blocker`, `Rejection`, `AssignmentInput`, `EscalationReason`, `requires_human_acceptance` (`farik-core`, on main); `EventLog`, `Projections`, `ProjectFiles`, `Git`, `Team` (on main).

Produces:

```rust
pub struct Transitions { /* log, projections, files, git, clock, ids (team and project), the request lock */ }
impl Transitions { pub fn new(log: Arc<EventLog>, projections: Arc<Projections>, files: Arc<ProjectFiles>, git: Git, clock: Arc<dyn Clock + Send + Sync>, ids: EventIds) -> Transitions; }
#[derive(Default)] pub struct TransitionAsk { pub assignee_id: Option<String>, pub reviewer_id: Option<String>, pub blocker: Option<Blocker>, pub blocker_resolution: Option<String>, pub rejection: Option<Rejection>, pub permission_denied: bool, pub session_id: Option<String> }
pub enum TransitionOutcome { Moved(TransitionDecision), Refused(TransitionRefusal) }
pub enum TransitionError { Store { detail: String }, Files { detail: String }, Git { detail: String }, Cost(CostError), Event { detail: String } }
impl Transitions {
    pub fn request(&self, request: &TransitionRequest, ask: &TransitionAsk, team: &Team) -> Result<TransitionOutcome, TransitionError>;
    pub fn context(&self, request: &TransitionRequest, ask: &TransitionAsk, team: &Team) -> Result<TransitionContext, TransitionError>;
}
TaskProjection { .., pub assignee_id: Option<String>, pub reviewer_id: Option<String>, pub iteration: u32 }
EventKind::{TaskTransitioned, TransitionRefused, EscalationRaised, ContractEvaluated}
```

## Tasks

### Task 1: the four events

Files: modified `docs/schemas/event.schema.json`, `crates/protocol/src/event.rs`, `crates/protocol/src/event/fixtures.rs`, `docs/SPEC.md`, tested in `event.rs`

Tests:

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers the round trip of all four once they are in `EVERY_KIND` and `a_body_wire`.
- `refuses_a_transition_to_a_status_that_does_not_exist` — `task.transitioned` with `to: "done"` is refused at `/body/to`.
- `refuses_an_escalation_for_an_unknown_reason` — `reason: "boredom"` is refused at `/body/reason`.
- `refuses_a_contract_event_without_a_task` — `new_event` of each of the four with `task_id: None` is `Err(EventError::NoContractNamed { .. })`.

- [x] `feat(protocol): add the transition, refusal, escalation, and evaluation events`

### Task 2: the board follows transitions

Files: created `crates/store/src/migrations/0004_transitions.sql`; modified `crates/store/src/migrations.rs`, `crates/store/src/projections.rs`, tested in `projections.rs`

Tests:

- `moves_a_task_on_the_board_when_it_transitions` — after `task.created` and a `task.transitioned` from `ready` to `assigned` with assignee `dev-a`, reviewer `dev-b`, iteration 0: the row's `status` is `assigned`, `assignee_id` is `Some("dev-a")`, `reviewer_id` `Some("dev-b")`, `iteration` 0, and `updated_seq` the event's.
- `leaves_the_board_alone_on_a_refusal` — a `transition.refused` changes nothing in the row but the cursor.
- The existing `known_versions()` assertion becomes `[1, 2, 3, 4]`.

- [ ] `feat(store): show assignee, reviewer, and iteration on the board`

### Task 3: the context from the store

Files: created `crates/runtime/src/transitions.rs` (`Transitions`, `TransitionAsk`, `TransitionError`, `context`); modified `crates/runtime/src/lib.rs`, tested in `transitions.rs` on a `git::fixtures::TempRepo` with `.farik/` initialised and a log in memory

Tests:

- `reads_status_and_people_from_the_board_not_the_file` — a contract file saying `draft` whose board row says `ready` with assignee `dev-a`: `context.contract.status == Ready` and `assignee == Some("dev-a")`.
- `counts_readiness_failures_since_the_task_last_entered_refining` — two failed `contract.evaluated` before a `task.transitioned` into `refining` and one after: `readiness_failed_attempts == 1`.
- `reads_an_assignment_from_the_ask_and_the_team` — ask `dev-a`, `dev-b` on a team where both are Software Developers and `dev-a` holds one open task: `assignment` has both ids, both roles, `assignee_open_tasks == 1`, `wip_limit` the team's, `has_active_scrum_master == false`.
- `reads_the_policy_that_asks_the_human_for_every_contract` — `human_accepts_contracts: all` gives `acceptance.required_by_policy == true`; `high_risk` gives `false`.
- `reads_work_from_the_tasks_worktree` — with `farik/FRK-1` one commit ahead of `main` in a clean worktree at `.farik/local/worktrees/FRK-1`: `work.commits == 1`, `work.worktree_clean`, and `done.changed_paths` the committed file; with no worktree: `commits == 0`, not clean, no paths.
- `reads_the_blocked_time_from_the_last_move_into_blocked` — `blocked_at` equals that event's `recorded_at`.
- `reads_children_from_the_board` — an epic with two child rows gives two `ChildState`s with their statuses.

- [ ] `feat(runtime): build a transition's context from the store`

### Task 4: judging and recording a request

Files: modified `crates/runtime/src/transitions.rs` (`request`, `TransitionOutcome`), `docs/plans/project-plan.md`, tested in `transitions.rs`

Tests:

- `moves_a_ready_task_to_assigned_and_records_it` — the Product Manager, on a team with no Scrum Master and two active Software Developers `dev-a` and `dev-b`, with the contract's `assignee_role` and `reviewer_role` both Software Developer and `daily_usd` at least its `max_cost_usd`, asks `ready → assigned` with `dev-a` and `dev-b` for a contract with no dependencies: `Moved`; one `task.transitioned` with `requested_by` the PM's id, `assignee: dev-a`, `reviewer: dev-b`; the file's `status` is `assigned` and its `assignee` `dev-a`; the board row agrees.
- `refuses_an_assignment_to_someone_not_on_the_team` — asks naming `ghost`, naming a paused agent, and naming only an assignee are each `Refused(GateFailed { .. })` with an `Assignment` failure naming the id or the missing reviewer, each recorded as a `transition.refused`, with the file unchanged; an epic on a team with no Scrum Master, assigned by the Product Manager to itself with no reviewer id, is not refused by this check.
- `serialises_two_requests_that_race_for_one_place` — with `wip_limit_per_agent: 1` and two ready tasks, two threads each assign one to `dev-a` at once: exactly one is `Moved` and the other `Refused`.
- `records_the_blocker_the_escalation_was_about` — `in_progress → blocked` with a blocker `description: "no key"`, then `blocked → escalated` by the governor past `blocked_limit_hours`: the `task.transitioned` into `blocked` carries the blocker, and `escalation.raised.detail` is `blocked_age: no key`.
- `refuses_a_claimed_role_the_agent_does_not_hold` — a Software Developer asking as `ProductManager` is `Refused(NotTheNamedAgent { .. })`; a `transition.refused` is appended; the file is unchanged.
- `records_a_refusal_with_every_reason` — `ready → assigned` with the assignee as its own reviewer: `Refused(GateFailed { .. })`, and the `transition.refused` event's `details` include the gate's words about the reviewer.
- `records_the_readiness_evaluation_before_the_move` — `refining → ready` by the governor for a contract that passes the Definition of Ready (no active Scrum Master, two active Software Developers, risk `low`, `human_accepts_contracts: high_risk`, `max_cost_usd` within the day's remainder and the default task maximum): the log holds `contract.evaluated { gate: definition_of_ready, passed: true }` then `task.transitioned`, in that order.
- `escalates_on_the_third_readiness_failure` — after two failed evaluations since refining, a third failing `refining → ready` then `refining → escalated` by the governor: `Moved`, `escalation.raised { reason: readiness_failures }` follows the `task.transitioned`, and the board says `escalated`.
- `increments_the_iteration_when_a_rejected_task_returns` — `rejected → in_progress` by the governor on a task at iteration 1: the event and file say iteration 2.

- [ ] `feat(runtime): judge transition requests and record the answer`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok (the transitions tests need a git binary and are ignored by default)
```
