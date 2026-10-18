# Standalone (open issues), plan 1: whoever holds a task reaches the owner

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2, 5.7 (recorded in plan 8's revision 0.122)
Depends on: `fix/stray-changes` at `6ba32d2a` (ADR 0068's blocked rows and choices)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A reviewer in `verifying` and a Product Manager in `refining` can block the task they hold, as an
assignee can in `assigned` and `in_progress`; the owner sees the blocking agent's words on Today and
answers it back where it stood, or sends it to the Product Manager to change the plan, in one press.
The table also gains the one row out of `accepted` that plan 5 uses, so the pinned row counts change
once. Out of scope: the review freeze and the reviewer's brief (plan 2), the pull request's
conflicts (plan 5).

## Decisions

Every decision is ADR 0069's (decisions 1, 2 and 4); this section pins how.

- **Six rows**, placed in `TRANSITION_TABLE` in spec order (30 rows: 26 specific, 4 `any`):
  `refining -> blocked` (ProductManager, BlockerWritten) right after the two `refining -> escalated`
  rows; `blocked -> verifying` (ScrumMaster, BlockerResolved), `blocked -> verifying` (Human,
  BlockerResolved), `blocked -> refining` (Human, BlockerResolved) right after
  `blocked -> in_progress` (Human); `verifying -> blocked` (Reviewer, BlockerWritten) right after
  `verifying -> rejected` (Human); `accepted -> assigned` (Governor, the new
  `GateId::PullRequestConflicted`) right after `rejected -> escalated`.
- **A terminal status is left only by a row that names it.** `find_transitions` and
  `transitions_from` keep returning nothing for `cancelled`; for `accepted` they return only rows
  whose `from` is `Status::Is(Accepted)`, never an `any` row. `is_terminal` is unchanged, so WIP,
  cleanup, budgets and dependents still count `accepted` as done.
- **Where a block returns.** `TransitionContext` gains `blocked_from: Option<TaskStatus>`, the
  `from` of the last move into `blocked` in the task's history (`None` with no such move).
  `pub fn return_from_block(blocked_from: Option<TaskStatus>) -> TaskStatus` in `gates.rs`:
  `Verifying` gives `Verifying`, `Refining` gives `Refining`, anything else or `None` gives
  `InProgress`. `check_blocker_resolved` gains the move's target: it opens with a written
  resolution and when `to` is `Refining` or `return_from_block(blocked_from)`; otherwise it says
  "the task was blocked while {from}, so an answer sends it back to {return}". `check_gate` gains a
  `to: TaskStatus` parameter for it.
- **Effects.** `ResetBlocker` is recorded for a move to `in_progress` (as now) and for any move out
  of `blocked` or `escalated` into `verifying` or `refining` (an escalated block answered back into
  its review leaves no blocker behind). `StampBlockedAt` stays on every move into `blocked`.
  `accepted -> assigned` records no effect.
- **The conflicts gate** (plan 5 fills its input): `TransitionContext` gains
  `pull_request_conflicts: Vec<String>`; `check_pull_request_conflicted(conflicts: &[String])`
  opens when it is not empty, else "the task's pull request merges, so it stays accepted". The
  runtime sets it empty until plan 5. Its wire name, `pull_request_conflicted`, is added to the gate
  enum of `docs/schemas/event.schema.json` (line 501's list) and to `gate_wire`.
- **The holder blocks as itself.** `declare_blocked` asks with `actor_for(call, &contract,
  TaskStatus::Blocked)` instead of `TransitionActor::Assignee`: the reviewer of a `verifying` task
  as `Reviewer`, the Product Manager (or the given agent, which writes with its standing while
  `refining`, ADR 0063) as `ProductManager`. For that, `actor_for`'s `ProductManager` arm also
  matches when the contract is `draft` or `refining` and `given_to(&call.deps().log, &contract.id)`
  names the caller, the rule `contract_writer` (`tools/contracts.rs`) applies to contract writes
  (make `given_to` `pub(super)` if it is private); `actor_for` stops being a plain `find` because
  that check can fail. A caller who holds nothing on the task is refused `actor_not_allowed` with
  nothing recorded, as `request_transition` is.
- **Who is stuck** is the agent of the last move into `blocked` (its envelope's `agent_id`), else
  the assignee. Today's line ("{agent} is stuck: …", `catervas_store::waiting`) and the choices'
  `who` both use it.
- **The choices** (`choices.rs`) of reason `blocked`, and of `blocker_age` when the escalation came
  from `blocked`: "Answer and carry on" (resolve to `return_from_block(blocked_from)`, next "{who}
  reads your answer and carries on with {id}."), then `Ask {pm} to change the plan` (resolve to
  `refining`, next as `rewrite`) unless the block came from `refining`, then "Cancel the task".
  `Offer` gains `blocked_from: Option<TaskStatus>`; `offer_for` reads it from the last move into
  `blocked` when the task is blocked or its escalation left `blocked`.
- **Sessions are told.** `CLOSING_INSTRUCTIONS`: Verify gains "If something you cannot change stops
  your review, a check Catervas could not run or something only the owner can give, end it with
  `catervas_declare_blocked`, saying what is in the way and what is needed. How a page looks and
  behaves in a browser is the UI/UX Designer's check, shown in the first message when the change
  touches the interface; never ask the owner to run a command or look at a page." Refine gains "If
  you cannot write it without something only the owner can give, end it with
  `catervas_declare_blocked`." `refine_message` gains `stuck: Option<&Blocker>`: when the task's
  last move into `refining` came from `blocked`, the refine session is told "This task came back
  to its plan because the agent working on it was stuck:" followed by the blocker's description and
  what was needed in an untrusted block named `blocker`, then "Change the contract so the work can
  be done."
- **The owner's words reach the stuck agent** through `escalation.resolved` and `human_message`, as
  now; nothing changes there (only a reviewer's session about its own task is not shown it).
- Rejected: a separate "help" status, a second tool for reviewers, sending a reviewer's answer to
  the assignee.

## File map

```
crates/core/src/governor/transition_table.rs   modifies: GateId::PullRequestConflicted, six rows, terminal rule (Task 1)
crates/core/src/governor/transition_table.rs   tests:    counts, spec lines, terminal, blocked rows (Task 1)
crates/core/src/governor/gates.rs              modifies: return_from_block, check_blocker_resolved(.., to), check_pull_request_conflicted (Task 1)
crates/core/src/governor/transition.rs         modifies: blocked_from, pull_request_conflicts, check_gate's `to`, effects (Task 1)
crates/core/src/governor/transition.rs         tests:    lets_every_holder_block_its_task, returns_a_blocked_task_where_its_block_came_from,
                                                         sends_back_an_accepted_task_whose_pull_request_conflicts (Task 1)
crates/runtime/src/transitions.rs              modifies: context fills blocked_from and an empty pull_request_conflicts; gate_wire (Task 1)
docs/schemas/event.schema.json                 modifies: gate enum gains pull_request_conflicted (Task 1)
crates/runtime/src/tools/work.rs               modifies: declare_blocked asks as the caller's actor (Task 2)
crates/runtime/src/tools/work.rs               tests:    blocks_a_task_as_its_reviewer (Task 2)
crates/runtime/src/choices.rs                  modifies: Offer.blocked_from, the blocked and blocker_age arms, offer_for (Task 3)
crates/runtime/src/choices.rs                  tests:    answers_a_block_where_it_came_from (Task 3)
crates/store/src/waiting.rs                    modifies: the blocked row names the agent that blocked (Task 3)
crates/runtime/src/orchestrator/human.rs       tests:    a_reviewers_block_is_answered_back_into_its_review (Task 3)
crates/runtime/src/prompt.rs                   modifies: Verify and Refine closing instructions (Task 4)
crates/runtime/src/orchestrator/messages.rs    modifies: refine_message(.., stuck), given_task_message passes it (Task 4)
crates/runtime/src/orchestrator/requests.rs    modifies: the refining rule finds the blocker it came back from (Task 4)
crates/runtime/src/orchestrator/messages.rs    tests:    tells_the_plan_why_the_work_came_back (Task 4)
```

## Interfaces

Consumes (on this branch): `TRANSITION_TABLE`, `find_transitions`, `transitions_from`,
`is_terminal`, `TransitionContext`, `evaluate_transition`, `check_gate`, `effects`, `Blocker`,
`check_blocker_resolved` (core); `Transitions::context`, `gate_wire`, `last_move_into`
(runtime, store); `declare_blocked`, `actor_for` (`tools/work.rs`); `Offer`, `escalation_choices`,
`offer_for`, `change`, `rewrite`, `cancel` (`choices.rs`); `waiting`, `name_of`, `BLOCKED`
(`catervas_store::waiting`); `CLOSING_INSTRUCTIONS`; `refine_message`, `given_task_message`,
`untrusted_block`; the refining rule in `orchestrator/requests.rs`.

Produces:

```rust
// crates/core/src/governor/transition_table.rs
pub enum GateId { /* … */ PullRequestConflicted }
pub static TRANSITION_TABLE: [TransitionRow; 30];
// crates/core/src/governor/gates.rs
#[must_use] pub fn return_from_block(blocked_from: Option<TaskStatus>) -> TaskStatus;
pub fn check_blocker_resolved(resolution: Option<&str>, blocked_from: Option<TaskStatus>, to: TaskStatus) -> GateResult;
pub fn check_pull_request_conflicted(conflicts: &[String]) -> GateResult;
// crates/core/src/governor/transition.rs
pub struct TransitionContext { /* … */ pub blocked_from: Option<TaskStatus>, pub pull_request_conflicts: Vec<String> }
// crates/runtime/src/choices.rs
pub struct Offer<'a> { /* … */ pub blocked_from: Option<TaskStatus> }
// crates/runtime/src/orchestrator/messages.rs
pub(super) fn refine_message(contract: &TaskContract, ask_first: bool, failures: &[String], stuck: Option<&Blocker>) -> String;
```

## Tasks

### Task 1: the table's new rows, and where a block returns

Files: modified `transition_table.rs`, `gates.rs`, `transition.rs` (core), `crates/runtime/src/transitions.rs`,
`docs/schemas/event.schema.json`; tested by the core modules' tests
Produces: the rows, `GateId::PullRequestConflicted`, `return_from_block`, the two gate checks, the context fields
Consumes: the table and gates as they are

`check_gate(gate, actor, to, context)` passes `request.to`; its `BlockerResolved` arm calls
`check_blocker_resolved(context.blocker_resolution.as_deref(), context.blocked_from, to)`, and
`PullRequestConflicted` calls `check_pull_request_conflicted(&context.pull_request_conflicts)`.
`Transitions::context` sets `blocked_from` from `last_move_into(&history, TaskStatus::Blocked)`'s
`TaskTransitioned` body `from` and `pull_request_conflicts: Vec::new()`. Every literal
`TransitionContext` in tests (`a_context` in `transition.rs`, and any other the compiler names)
gains `blocked_from: None, pull_request_conflicts: Vec::new()`.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `has_exactly_thirty_distinct_rows` (renamed from `…twenty_four…`) and
  `has_twenty_six_specific_rows_and_four_any_rows` (renamed) — 30 distinct, 26 specific, 4 `any`.
- `matches_every_line_of_the_spec_table` (amended) — gains `(Refining, Blocked,
  [(ProductManager, BlockerWritten)])`, `(Blocked, Verifying, [(ScrumMaster, BlockerResolved),
  (Human, BlockerResolved)])`, `(Blocked, Refining, [(Human, BlockerResolved)])`, `(Verifying,
  Blocked, [(Reviewer, BlockerWritten)])`, `(Accepted, Assigned, [(Governor,
  PullRequestConflicted)])`; the array is `[Line; 19]`.
- `refuses_to_leave_a_terminal_status` (amended) — `cancelled` has no way out; `accepted` has
  exactly `[(Governor, PullRequestConflicted)]` from `transitions_from`, and
  `find_transitions(Accepted, Escalated)` and `(Accepted, Cancelled)` are empty.
- `lists_the_rows_that_leave_a_status_in_table_order` (amended) — `blocked` gives
  `[(ScrumMaster, BlockerResolved), (Human, BlockerResolved), (ScrumMaster, BlockerResolved),
  (Human, BlockerResolved), (Human, BlockerResolved), (Governor, BlockedAge), (Governor,
  GovernorEscalation), (Human, None), (Human, None)]`.
- `lets_every_holder_block_its_task` — with a written blocker: `verifying -> blocked` as `Reviewer`
  agent `arch-1` (the contract's reviewer) moves with `[StampBlockedAt]`, and as `Reviewer` agent
  `dev-1` is `NotTheNamedAgent`; `refining -> blocked` as `ProductManager` moves; with no blocker
  each is `GateFailed` on `BlockerWritten`. Fails first: `no_such_transition`.
- `returns_a_blocked_task_where_its_block_came_from` — `blocked_from: Some(Verifying)` with a
  resolution: Human to `verifying` moves with `[ResetBlocker]` (and an `escalated` task's Human move
  to `verifying` records `ResetBlocker` too), Human to `in_progress` is
  `GateFailed` whose detail contains "sends it back to verifying"; `Some(InProgress)`: to
  `in_progress` moves, to `verifying` refused; `None`: to `in_progress` moves; any of them to
  `refining` moves with `[ResetBlocker]`; no resolution refuses every one.
- `sends_back_an_accepted_task_whose_pull_request_conflicts` — `accepted -> assigned` as Governor
  with `pull_request_conflicts: ["components/NoteInput.tsx"]` moves with no effect; with none it is
  `GateFailed` saying "merges"; as Human it is `ActorNotAllowed`.

- [ ] `feat(core): a reviewer or a planner blocks what it holds and the answer returns it there`

### Task 2: the holder blocks as itself

Files: modified `crates/runtime/src/tools/work.rs`; tested by its tests
Consumes: Task 1's rows; `actor_for`, `call.contract`

`declare_blocked` reads `let (contract, _) = call.contract(&task)?;` and asks with
`actor_for(call, &contract, TaskStatus::Blocked)?`. Its doc comment says "as the agent that holds
it: the assignee, the reviewer of a task in review, or the Product Manager writing its contract".

Tests:

- `blocks_a_task_as_its_reviewer` — a task in `verifying` whose reviewer is the session's agent:
  `catervas_declare_blocked` moves it to `blocked`, and the `task.transitioned` body has
  `actor: reviewer` and `requested_by` the reviewer's id; a `refining` task given to a developer in
  its chat (as the ADR 0063 tests of `contracts.rs` give one) is blocked by that developer with
  `actor: product_manager`; from a session of an agent that holds nothing on the task, the call is
  refused `actor_not_allowed` and no `transition.refused` is appended. Fails first: the move is
  refused with `actor: assignee`.

- [ ] `fix(runtime): an agent blocks a task as the one who holds it`

### Task 3: the choices fit who is stuck

Files: modified `crates/runtime/src/choices.rs`, `crates/store/src/waiting.rs`; tested by
`choices.rs`, `waiting.rs` and `orchestrator/human.rs` tests
Consumes: Task 1's `return_from_block` and rows; Task 2's reviewer block

In `waiting`, the blocked row's `agent` is the last move into `blocked`'s
`envelope.ids.agent_id`, else `row.assignee_id`. In `offer_for`, `blocked_from` is that move's
`from` when the task is `blocked` or `before == Some(Blocked)`; `who` is that move's agent's name
when there is one. Every `Offer { … }` literal gains `blocked_from`.

Tests:

- `answers_a_block_where_it_came_from` (`choices.rs`) — reason `blocked`, who `Ada`,
  `blocked_from: Some(Verifying)`: labels `["Answer and carry on", "Ask Mira to change the plan",
  "Cancel the task"]`, actions resolve to `verifying`, `refining`, `cancelled`, the first `next` is
  "Ada reads your answer and carries on with CTV-14."; `Some(Refining)`: labels `["Answer and carry
  on", "Cancel the task"]`, the first resolving to `refining`; reason `blocker_age`, `before:
  Some(Blocked)`, `blocked_from: Some(InProgress)`: the first resolves to `in_progress`, the second
  is "Ask Mira to change the plan". Fails first: the blocked arm always resolves to `in_progress`.
- `names_the_agent_that_blocked_the_task` (`waiting.rs`, built with its `fixtures`) — a task whose
  reviewer `grace` recorded the move into `blocked` with description "the checks cannot run git":
  its row's line is "Grace is stuck: the checks cannot run git" and its agent is `grace`.
- `a_reviewers_block_is_answered_back_into_its_review` (`orchestrator/human.rs`, ignored "needs the
  git program", built as the ADR 0068 blocked-task test at line ~3948 is; that test is amended for
  the third choice, "Ask … to change the plan") — a `verifying` task the
  reviewer blocked: `escalation_resolve` to `verifying` with "Here is the output" moves it to
  `verifying` and records `escalation.resolved`; `escalation_resolve` to `in_progress` on the same
  kind of block is refused and nothing moves.

- [ ] `fix(runtime): the owner's answer sends a blocked task back to whoever was stuck`

### Task 4: the sessions are told how to block

Files: modified `crates/runtime/src/prompt.rs`, `crates/runtime/src/orchestrator/messages.rs`,
`crates/runtime/src/orchestrator/requests.rs`; tested by `prompt.rs` and `messages.rs` tests
Consumes: Task 1's `blocked -> refining`; `Blocker`

The refining rule (`requests.rs`, where `refine_message`/`given_task_message` are built) reads the
task's history: when the last move into `refining` came from `blocked`, `stuck` is the blocker of
the last move into `blocked` (its `TaskTransitioned` body's `blocker`, as a core `Blocker`).

Tests:

- `closes_with_the_purposes_instruction` (amended) — the Verify closing also names
  `catervas_declare_blocked` and "never ask the owner"; the Refine closing names
  `catervas_declare_blocked`.
- `tells_the_plan_why_the_work_came_back` — `refine_message` with a `Blocker` "the contract allows
  colour only, the contrast comes from the fade-in" / "allow components/BoardDemo.tsx" contains
  "came back to its plan", both texts inside an untrusted block named `blocker`, and "Change the
  contract"; with `None` it is the message it was. Fails first: the parameter does not exist.

- [ ] `feat(runtime): the reviewer and the planner are told to block what stops them`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the git tests of orchestrator/human.rs run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
