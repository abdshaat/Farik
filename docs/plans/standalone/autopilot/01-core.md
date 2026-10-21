# Autopilot, plan 1: the wire, the core and the log

Status: ready
Branch: `feat/autopilot`
Spec and decisions: see `README.md`; ADR 0082 decisions 1 to 8, 10, 12

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/` unless they start with `docs/`. `catervas-core` does no I/O (hard rule 5).

## Decisions

- The event kinds live in `catervas-protocol` (`crates/protocol/src/event.rs`), wired at every place `merge_permission.granted` is (the kind-to-def table near line 314, the kinds list near 688, the `EventBody` variant near 1126, `kind()` near 1273, the actor and about-a-contract tables near 1638, `lib.rs` near 174). Generated body types come from `event.schema.json` (ADR 0009); no hand-written body struct.
- The new choices are a core enum, `DecideChoice`, with its wire and its meaning in one place, which plan 2's tool and prompt read. Chose core over a runtime table: the limits (three grants, three reopens) are governance and are tested without I/O.
- `choices_for` takes the wait's reason as the log words it (`&str`), as `crate::choices::escalation_choices` (runtime) does, with three words of its own: `"blocked"` for a task its agent blocked, `"acceptance"` for a result that waits on the owner's acceptance (the word `WaitingKind::Acceptance` uses), and `"folder_change"` for a folder change whose integration failed. The choices per reason are the owner's, by ADR name (decisions 3 and 11):
  - `iterations`: `more_tries`, `back_to_plan`, `cancel`.
  - `budget`, `sessions`: `more_room` (only when `before` is known, as the owner's), `cancel`.
  - `blocker_age`, `blocked`: `answer_block`; `back_to_plan` unless `return_from_block(blocked_from)` is `refining`; `cancel`.
  - `readiness_failures`: `try_readiness_again`, `cancel`.
  - `integration`: `reopen_integration` while fewer than `REOPENS_PER_TASK` were chosen for the task, else nothing but `flag`.
  - `approval`, `risk_gate`: `approve_plan`, `back_to_plan`, `cancel` (the owner's plan page approves or sends back).
  - `acceptance`: `accept_result`.
  - `permission`, `explicit_request`: `carry_on` (only when `before` is known and is neither `refining` nor `escalated`, as the owner's "Carry on"), `back_to_plan`, `cancel`.
  - `preview`: `carry_on` (as above), `cancel`; the owner's "Fix how your app opens" is a Settings change, so it is `flag` (decision 11).
  - `folder_change`: `retry_folder_change` while fewer than `REOPENS_PER_TASK` were chosen for that change.
  - any reason not listed: `back_to_plan`, `cancel`.
  - When the task has had `TEAM_GRANTS_PER_TASK` `more_tries` or `more_room` from the team and the reason is `iterations`, `budget` or `sessions`: `back_to_plan`, `cancel` (decision 3, literally).
  - `flag` is always last.
  `REOPENS_PER_TASK` (for `reopen_integration` per task and `retry_folder_change` per folder change) is the planner's: a failing check retried forever would spend a decide session per tick; the ADR caps grants the same way.
- `move_of(choice, before, blocked_from)`: `more_tries` → `in_progress` with 2 more tries (the owner's "Give 2 more tries"); `more_room` → `before` with room; `back_to_plan`, `try_readiness_again` → `refining`; `answer_block` → `return_from_block(blocked_from)`; `carry_on` → `before`; `cancel` → `cancelled`; `reopen_integration` → `Reopen`; `retry_folder_change` → `RetryFolderChange`; `approve_plan` → `ApprovePlan` (the owner's `approve`: the contract accepted, then `escalated -> ready`); `accept_result` → `AcceptResult` (the owner's `accept_result`: no move); `flag` → `Flag`. `None` for `more_room` or `carry_on` with no `before`.
- The question's answerer: `product` → the Product Manager; `technical` → the Architect when the team has an active one and the asker is not the Architect, else the Product Manager (the second half is the planner's: an Architect does not answer itself).
- The table: every row whose actor is `Human` gets a twin with actor `ProductManager` and gate `Autopilot`, appended after the last row, in table order, except `ready -> refining`, which the Product Manager already holds with gate `None` (ADR 0078). Seven rows: `blocked -> in_progress`, `blocked -> verifying`, `blocked -> refining`, `verifying -> rejected`, `any -> escalated`, `any -> cancelled`, `escalated -> any`. The table becomes 40 rows: 33 specific, 7 `any`.
- The `Autopilot` gate opens when `TransitionContext.autopilot` is true, the task is `blocked` or `escalated` (decision 3's scope: an escalation or a block), and at least one `Human` row of the same move opens its own gate as if the human asked (`check_gate(row.gate, Human, to, context)`). Rejected: a gate that checks autopilot alone, which would let a Product Manager clear a block without a resolution, or cancel any task in any session.
- An assignment past the limit: `AssignmentInput.past_limit_allowed: bool`; `over_the_limit` gives nothing when it is true. The runtime sets it (plan 3) when autopilot is on and the assignee has no session running (decision 8). The limit of zero ("takes no work") is held whatever the flag: an owner who set zero meant it.
- A connector call under autopilot: `evaluate_connector_call` gains `autopilot: bool`; a `paid` or `ask` tool that is not plan-marked passes with `ConnectorPass.autopilot_approved: true` and no approval or allowance, after `InputTooLarge` (the input is recorded whole) and before the grant. A grant or allowance is never used up under autopilot. A plan-marked tool still needs an active plan; `denied` stays denied.
- The rate of paid calls (decision 12) is a pure function, `rate_limit`, beside the constants in `core/src/autopilot.rs`: the runtime hands it the agent's recent autopilot-approved calls (`RecentCall`: server, tool, `input_sha256`, time, read from `tool.approved` by plan 3) and the clock's now, and it counts only the calls of the past hour (`at > now - 1h`). It checks, in this order, and answers the first that holds: the same server, tool and `input_sha256` made `SAME_INPUT_CALLS_PER_HOUR` (3) times (`RateLimit::SameInput`: a loop, so it is named before the counts); the same server and tool made `per_tool_calls_per_hour(per_hour)` times, a third of the team's figure rounded down, at least 1 (`ThisTool`); any calls made `per_hour` times (`AllTools`). "Has made n" refuses the call after the n-th, so a count equal to the limit refuses. `RateLimited.frees_at` is when the call that keeps the count at the limit leaves the hour: with the counted calls oldest first and the limit `l`, the time of the `count - l`-th (0-based) plus one hour, after which one more call is allowed. Chose a function of its own over a further argument to `evaluate_connector_call`: only a call that function already passed with `autopilot_approved` needs the log read, and the hook reads it only then. `per_hour` comes from `Team::paid_calls_per_hour()` (`core/src/team.rs`, beside `readiness_attempts`): the schema's `policy.paid_calls_per_hour`, integer 1 to 1000 with `default` 30, so the generated field is a `NonZeroU64` with no `Option`, as `escalation_age_hours` is, and the accessor returns its `get()` as `u32`.
- The log's answers live in `catervas-store` (`crates/store/src/autopilot.rs`), as `merging_allowed` does (ADR 0073): core does no I/O. A flag lasts until the task's next `task.transitioned`, `escalation.resolved`, `task.integrated`, `human.accepted`, or `team.decided` other than `flag` (decision 4: "when the owner acts, the task resumes as now"); no projection column, so no migration. A folder change has no task, so its flag is a `team.decided { choice: flag, folder_change }` and lasts until that change's next `folder_change.opened` or `folder_change.integrated`, or a later `team.decided { choice: retry_folder_change }` of it.
- A failed folder change is tried again by the tick when a `team.decided { choice: retry_folder_change, folder_change }` follows its `folder_change.escalated`: `store/src/folder_docs.rs` `folder_docs` (near 300) adds `team.decided` to the kinds it reads and sets that change's `escalated` back to false, which is what `integrate.rs` `escalated_since_accepted` reads for a folder change (decision 11).
- A team question holds its task as the owner's does: `team.question_asked` raises `open_questions` and `team.question_answered` lowers it, in `projections.rs` `apply_wait_counts`; an owner's `question.answered` naming a team question lowers it the same.

## File map

```
docs/schemas/event.schema.json        modifies: six kinds and bodies, three optional fields, gateId, purpose (T1)
protocol/src/event.rs, protocol/src/lib.rs  modifies: the kinds wired; tests (T1)
core/src/autopilot.rs                 creates: DecideChoice, QuestionAbout, Spent, DecideMove, choices_for, move_of, answerer, constants; tests (T2)
core/src/lib.rs                       modifies: pub mod autopilot (T2)
core/src/governor/transition_table.rs modifies: GateId::Autopilot, seven rows; tests (T3)
core/src/governor/transition.rs       modifies: TransitionContext.autopilot, the gate; tests (T3)
core/src/governor/gates.rs            modifies: AssignmentInput.past_limit_allowed, over_the_limit; tests (T4)
core/src/governor/permissions.rs      modifies: evaluate_connector_call(autopilot), ConnectorPass.autopilot_approved; tests (T5)
core/src/autopilot.rs                 modifies: RecentCall, RateLimit, RateLimited, rate_limit, per_tool_calls_per_hour, three constants; tests (T5)
docs/schemas/team.schema.json         modifies: policy.paid_calls_per_hour (T5)
core/src/team.rs                      modifies: Team::paid_calls_per_hour; the full fixture and the bounds test (T5)
runtime/src/transitions.rs, runtime/src/daemon/hooks.rs  modifies: the new fields set false at each construction (T3, T4, T5)
store/src/autopilot.rs                creates: the log's answers; tests (T6)
store/src/lib.rs                      modifies: pub mod autopilot (T6)
store/src/projections.rs              modifies: team questions hold the task; tests (T6)
store/src/folder_docs.rs              modifies: a retry clears a folder change's escalation (T6)
```

## Interfaces

Consumes (on `main`): `TransitionRow`, `GateId`, `TRANSITION_TABLE`, `find_transitions`, `check_gate`, `TransitionContext`, `AssignmentInput`, `over_the_limit`, `evaluate_connector_call`, `ConnectorPass`, `return_from_block` (`core`); `EventLog`, `EventQuery`, `Projections`, `name_of` (`store`); `Team::is_solo`, `Team::active_agents`.

Produces:

```rust
// core/src/autopilot.rs
pub const TEAM_GRANTS_PER_TASK: u32 = 3;
pub const REOPENS_PER_TASK: u32 = 3;
pub const DECIDE_SESSIONS_PER_WAIT: u32 = 3;
pub const BLOCK_WAIT_MINUTES: i64 = 60;
pub const SELLER_MESSAGES_PER_DAY: usize = 50;
pub const DONE_DAYS: i64 = 7;
pub const PAID_CALLS_PER_HOUR: u32 = 30;             // decision 12, the default of policy.paid_calls_per_hour
pub const SAME_INPUT_CALLS_PER_HOUR: usize = 3;      // decision 12
pub const RATE_LIMITED_PER_SESSION: u32 = 3;         // decision 12: the third refusal ends the session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecideChoice { MoreTries, MoreRoom, BackToPlan, TryReadinessAgain, ReopenIntegration, AnswerBlock, Cancel,
    ApprovePlan, AcceptResult, CarryOn, RetryFolderChange, Flag }
impl DecideChoice {
    pub const fn wire(self) -> &'static str;              // "more_tries", ...
    pub fn from_wire(wire: &str) -> Option<DecideChoice>;
    pub const fn meaning(self) -> &'static str;           // one sentence each, for the decide prompt
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionAbout { Product, Technical }             // wire "product" | "technical"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent { pub grants: u32, pub reopens: u32 }   // reopens: of the task, or retries of the folder change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecideMove { To { status: TaskStatus, extra_tries: Option<u8>, extra_room: bool }, Reopen,
    RetryFolderChange, ApprovePlan, AcceptResult, Flag }
pub fn choices_for(reason: &str, before: Option<TaskStatus>, blocked_from: Option<TaskStatus>, spent: Spent) -> Vec<DecideChoice>;
pub fn move_of(choice: DecideChoice, before: Option<TaskStatus>, blocked_from: Option<TaskStatus>) -> Option<DecideMove>;
pub fn answerer(about: QuestionAbout, asker: Role, has_architect: bool) -> Role;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCall { pub server: String, pub tool: String, pub input_sha256: String, pub at: DateTime<Utc> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimit { SameInput, ThisTool, AllTools }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimited { pub limit: RateLimit, pub calls: u32, pub frees_at: DateTime<Utc> } // calls: the limit's figure
pub const fn per_tool_calls_per_hour(per_hour: u32) -> u32;                         // max(per_hour / 3, 1)
pub fn rate_limit(recent: &[RecentCall], server: &str, tool: &str, input_sha256: &str, per_hour: u32,
    now: DateTime<Utc>) -> Result<(), RateLimited>;
// core/src/team.rs
impl Team { pub fn paid_calls_per_hour(&self) -> u32; }                              // policy.paid_calls_per_hour, 30 left out
// core/src/governor/transition_table.rs
pub enum GateId { /* existing */ Autopilot }
pub static TRANSITION_TABLE: [TransitionRow; 40];
// core/src/governor/transition.rs
pub struct TransitionContext { /* existing */ pub autopilot: bool }
// core/src/governor/gates.rs
pub struct AssignmentInput { /* existing */ pub past_limit_allowed: bool }
// core/src/governor/permissions.rs
pub struct ConnectorPass { /* existing */ pub autopilot_approved: bool }
pub fn evaluate_connector_call(tool: &str, input: &serde_json::Value, connector: Option<&SessionConnector>,
    granted: Option<u64>, used: u32, active_plan: bool, autopilot: bool) -> Result<ConnectorPass, ConnectorRefusal>;
// store/src/autopilot.rs
pub fn autopilot_on(log: &EventLog) -> Result<bool, StoreError>;                    // newest autopilot.changed says on
pub fn merges_on_its_own(log: &EventLog, team: &Team) -> Result<bool, StoreError>;  // merging_allowed || autopilot_on
pub fn team_decides(log: &EventLog, team: &Team) -> Result<bool, StoreError>;       // autopilot_on && (active PM || solo)
pub struct Flag { pub task_id: Option<TaskId>, pub folder_change: Option<u64>, pub needs: String,
    pub flagged_by: String, pub at: DateTime<Utc> }                                 // exactly one of the two is set
pub fn flags(log: &EventLog) -> Result<Vec<Flag>, StoreError>;                      // standing flags, oldest first
pub struct TeamQuestion { pub seq: u64, pub task_id: Option<TaskId>, pub question: String, pub asked_by: String,
    pub about: QuestionAbout, pub at: DateTime<Utc> }
pub fn open_team_questions(log: &EventLog) -> Result<Vec<TeamQuestion>, StoreError>; // no team or owner answer yet
pub fn team_spent(log: &EventLog, task_id: &TaskId) -> Result<Spent, StoreError>;  // counts team.decided
pub fn folder_change_spent(log: &EventLog, change: u64) -> Result<Spent, StoreError>; // its retry_folder_change
pub struct OnItsOwn { pub at: DateTime<Utc>, pub line: String, pub agent_id: Option<String>,
    pub task_id: Option<TaskId>, pub reason: Option<String> }
pub fn done_on_its_own(log: &EventLog, team: &Team, now: DateTime<Utc>) -> Result<Vec<OnItsOwn>, StoreError>;
```

`done_on_its_own` lists, newest first, at most 100, the events of the last `DONE_DAYS` days: `team.question_answered` ("<answerer> answered <asker>'s question", reason the answer cut to 200 characters); `team.decided` ("<decider> chose <choice in words> for <task>", reason its reason); `tool.approved` ("<agent> used <tool> of <server>"); `marketing_plan.approved` with `by: autopilot` ("Catervas approved marketing plan <plan>"); `data_pipeline.approved` and `.declined` with `by: product_manager` whose envelope names a session ("<pm> approved | declined the data source <name>", reason its reason); `seller_message.sent` with `sent_by: autopilot` ("Catervas sent <seller> “<subject>”"); `pull_request.merged` recorded while autopilot was on, by the `autopilot.changed` events before it in seq order ("Catervas merged <url>"). Names through `waiting::name_of`.

## Tasks

### Task 1: the wire

Files: `docs/schemas/event.schema.json`, `protocol/src/event.rs`, `protocol/src/lib.rs`
Produces: `EventBody::AutopilotChanged`, `TeamQuestionAsked`, `TeamQuestionAnswered`, `TeamDecided`, `TaskFlagged`, `ToolApproved` with their generated bodies; the optional fields of README.
Tests (`event.rs` `mod tests`):
- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` (extended): `EVERY_KIND` and `an_event_wire` gain the six kinds; each reads back as its own kind; `team.decided`, `task.flagged`, `team.question_asked` and `team.question_answered` are about their task, the other two about none, and a `team.decided` with `folder_change` and no task reads back about none; a `marketing_plan.approved` with `by: "autopilot"`, a `seller_message.sent` with `sent_by: "autopilot"`, a `task.transitioned` with gate `autopilot` and a `session.started` with purpose `decide` read back, and each body of the six is refused with one unknown property.

- [x] `feat(protocol): the autopilot's events`

### Task 2: the team's choices and their limits

Files: `core/src/autopilot.rs` (created), `core/src/lib.rs`
Produces: everything under `core/src/autopilot.rs` above.
Tests (`autopilot.rs` `mod tests`):
- `offers_the_owners_choices_for_each_reason_within_the_teams_limits` — asserts the lists of Decisions for `iterations` (with `Spent::default()`), `budget` with and without `before`, `blocked` with `blocked_from` `in_progress` and `refining`, `readiness_failures`, `integration` with 2 and 3 reopens, `approval` (`approve_plan` first), `acceptance` (`[accept_result, flag]`), `permission` with `before` `in_progress` (`carry_on` first) and with `before` `refining` (no `carry_on`), `preview`, `folder_change` with 3 retries (`[flag]`), `something_new`; that `iterations` and `sessions` with `grants: 3` give `[back_to_plan, cancel, flag]`; that every list ends with `flag`; and that `move_of` gives `To { in_progress, Some(2), false }` for `more_tries`, `To { verifying, None, false }` for `answer_block` from `verifying`, `None` for `more_room` with no `before`, `Reopen` for `reopen_integration`, `ApprovePlan` for `approve_plan`, and `To { in_progress, None, false }` for `carry_on` from `in_progress`.
- `asks_the_architect_technical_questions_unless_it_asks_or_is_missing` — asserts `answerer` gives the Product Manager for `product`, the Architect for `technical` from a Developer with an Architect, and the Product Manager for `technical` with no Architect and for the Architect's own question.

- [x] `feat(core): the team's choices for what waits on it`

### Task 3: the Product Manager takes the owner's moves under autopilot

Files: `core/src/governor/transition_table.rs`, `core/src/governor/transition.rs`, `docs/schemas/event.schema.json` (already has `autopilot` in `gateId` from Task 1), `runtime/src/transitions.rs` (every `TransitionContext` built sets `autopilot: false`)
Produces: `GateId::Autopilot`, the seven rows, `TransitionContext.autopilot`.
Tests:
- `transition_table.rs`: `has_exactly_forty_distinct_rows` (replaces `..._thirty_three_...`) and `has_thirty_three_specific_rows_and_seven_any_rows` (replaces `..._twenty_nine_..._four_...`); `escalates_any_non_terminal_status_for_the_governor_and_the_human`, `cancels_any_non_terminal_status_for_the_human_only` (renamed `cancels_any_non_terminal_status_for_the_human_and_the_product_manager_under_autopilot`), `lets_the_human_move_an_escalated_task_to_any_other_status`, `lists_the_rows_that_leave_a_status_in_table_order` and `matches_every_line_of_the_spec_table` gain the `(ProductManager, Autopilot)` pairs in table order.
- `transition.rs`: `the_product_manager_takes_the_owners_moves_only_under_autopilot` — for a blocked task with a resolution, the Product Manager's `blocked -> in_progress` is `GateFailed` with gate `Autopilot` when `autopilot` is false and opens when true; with autopilot on and no resolution it fails on the human row's `BlockerResolved` words; an escalated task goes to `cancelled` and to `refining` for the Product Manager under autopilot; an `in_progress` task's `in_progress -> cancelled` is refused for the Product Manager under autopilot (not blocked or escalated).

- [x] `feat(core): the product manager takes the owner's moves under autopilot`

### Task 4: an idle agent may take work past its limit

Files: `core/src/governor/gates.rs`, `runtime/src/transitions.rs` (`assignment` sets `past_limit_allowed: false`)
Produces: `AssignmentInput.past_limit_allowed`.
Tests (`gates.rs` `mod tests`):
- `an_idle_agent_under_autopilot_takes_work_past_its_limit` — with `wip_limit: 1` and `assignee_open_tasks: 1` the assignment is refused with the limit's words when `past_limit_allowed` is false and passes when true; with `wip_limit: 0` it is refused either way.

- [x] `feat(core): an idle agent takes work past its limit under autopilot`

### Task 5: a paid or asking call runs under autopilot, within its rate

Files: `core/src/governor/permissions.rs`, `runtime/src/daemon/hooks.rs` (the one call passes `false`), `core/src/autopilot.rs`, `docs/schemas/team.schema.json`, `core/src/team.rs`
Produces: `evaluate_connector_call(.., autopilot)`, `ConnectorPass.autopilot_approved`; `RecentCall`, `RateLimit`, `RateLimited`, `rate_limit`, `per_tool_calls_per_hour`, the three constants of decision 12; `policy.paid_calls_per_hour`, `Team::paid_calls_per_hour`.
Tests (`permissions.rs` `mod tests`, `autopilot.rs` `mod tests`, `team.rs` `mod tests`):
- `a_paid_or_ask_call_runs_under_autopilot_and_says_so` — with `autopilot: true` and no grant or allowance, a `paid` and an `ask` tool pass with `autopilot_approved: true`, `approval: None`, `allowance: None`; a grant is not used (`approval: None`); an input over `MAX_APPROVAL_INPUT` is still `InputTooLarge`; a `denied` tool is `ToolDenied`; a plan-marked tool with no active plan is `NoActivePlan`; with `autopilot: false` the existing answers hold (`ApprovalNeeded`).
- `autopilot.rs`: `rate_limits_an_agents_paid_calls_by_tool_by_all_tools_and_by_repeated_input` — with `per_hour` 30 and `now` 12:00, every input distinct unless said: 9 calls of `exa/search` in the hour and a 10th at 10:59 (outside it) pass; 10 in the hour refuse `ThisTool` with `calls: 10` and `frees_at` one hour after the oldest of the ten; 29 calls spread over five tools, none at 10, pass and 30 refuse `AllTools` with `calls: 30`; 3 calls of `exa/search` with one `input_sha256` refuse `SameInput` with `calls: 3` and `frees_at` one hour after the first of the three, while the same three pass for a call of another tool or with another input; with `per_hour` 3 the same three still answer `SameInput`, not `ThisTool` or `AllTools`; `per_tool_calls_per_hour` is 10 for 30, 1 for 1 and for 2, 333 for 1000.
- `team.rs`: `refuses_a_number_outside_what_a_rule_allows` (extended) — `/policy/paid_calls_per_hour` 0 and 1001 are refused; `defaults_the_memory_cap` (renamed `defaults_the_memory_cap_and_the_paid_call_rate`) — a team without the key gives `paid_calls_per_hour()` 30, and `a_full_team_wire` carries `paid_calls_per_hour: 12`, which reads back 12.

- [x] `feat(core): a paid or asking call runs under autopilot, within its rate`

### Task 6: the log's answers

Files: `store/src/autopilot.rs` (created), `store/src/lib.rs`, `store/src/projections.rs`, `store/src/folder_docs.rs`
Consumes: Task 1's kinds, Task 2's `QuestionAbout` and `Spent`.
Produces: everything under `store/src/autopilot.rs` above; team questions in `apply_wait_counts` and its dispatch near line 686.
Tests:
- `autopilot.rs`: `autopilot_is_on_only_by_this_computers_log` (the security guarantee of decision 1) — `autopilot_on` is false on an empty log, true after `autopilot.changed { on: true }`, false after a later `{ on: false }`; `merges_on_its_own` is true under autopilot for a team without `merge_when_checks_pass`; `team_decides` is false for a team with no active Product Manager that is not solo.
- `autopilot.rs`: `a_flag_stands_until_the_task_moves_and_acts_are_listed_for_seven_days` — a `task.flagged` stands in `flags`, and is gone after the task's next `task.transitioned`; `team_spent` counts two `more_tries` and one `reopen_integration`; a `team.decided { choice: flag, folder_change: 4 }` stands as a flag of change 4 until a later `retry_folder_change` of it, which also leaves change 4 not `escalated` in `folder_docs`; `done_on_its_own` lists a `team.decided` of six days ago with its reason and not one of eight days ago, a `pull_request.merged` recorded while autopilot was on and not one recorded before it was turned on, newest first.
- `projections.rs`: `a_team_question_holds_its_task_until_answered` — `waiting_on_human` is true after `team.question_asked` about `CTV-1`, false after `team.question_answered` naming it, and the same with the owner's `question.answered` naming it.

- [x] `feat(store): autopilot's state, flags and acts from the log`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo test -p catervas-core -p catervas-protocol -p catervas-store
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask core-io
# expected: test result: ok (each crate); core-io: ok
```

Each of Tasks 3 to 5 sets its new field or argument at every construction site the compiler names (`false`), so the workspace builds after every commit; plans 2 and 3 replace them with the real values.
