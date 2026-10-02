# Phase 7, step 05b: Allowances

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 5.5, 5.6, 8.5; F9, F17
Depends on: step 05 of this phase (its `Kit`, `KitAllowance`, `kit_entry`, `matches_kit`, `ToolDeps.kits`, the `source: kit` entry and the boards of its Task 1; it lands first), step 02 (the grant and the ask), phase 6 (merged in #19)
Readiness confirmed by: fresh-session Opus reviewer, 2026-10-02: ready with should-fixes, all folded
Mockups approved by: the founder, 2026-10-02 (step 05's O1; this step draws no boards of its own)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Split from row 05 at its seam: step 05 lets a kit say which spending tools may run without asking and how many calls by default; this step makes it work. When it is done, a user connecting a service that spends their credits (a picture generator, say) sets how many calls each sprint the agent may make without asking, from the kit's default; calls inside it run, the first beyond it asks as any `external_effect` call does, and the user can change the number from the agent page. The board and the Costs page show "14 of 20 images this sprint". Out of scope: pricing another service's credits (an allowance counts calls); allowances on a user's own servers; the kits that ship spending tools (step 08).

## Decisions

- **Where an allowance lives.** A kit entry in `team.yaml` gains `allowances: { <tool>: 0..1000 }`, per agent because the entry is the agent's. `validate_team` refuses one on an entry that is not `source: kit` (`allowance_not_kit`) and one for a tool the entry does not tag `external_effect` (`allowance_not_external`), each at `/agents/<i>/mcp_servers/<j>/allowances/<tool>`. `CustomServer` gains `allowances: BTreeMap<String, u32>`. `spec_sha256` holds `"allowances"` only when it is not empty, so every hash kept before stands, and a commit that raises an allowance makes the server "Connect again": an allowance is a pre-approval, and ADR 0031 keeps a committed file from granting one.
- **Only what the kit offers.** At connect each allowance starts at the kit's `calls`, and the user may set each from 0 to 1,000; one for a tool the kit gives none is refused `allowance_not_offered`, one outside the range `allowance_out_of_range`. A batch tool and a tool that publishes, sends, posts or pays get no allowance in its kit, so it always asks, at any number. 0 means it asks every time. `matches_kit` compares an entry with its `allowances` taken out, and requires each allowance's tool to be one the kit offers one for.
- **Changing it later** is connecting again without typing the key. `connector.allowances { agent, server, allowances }` takes the entry lock, loads the agent's kept entry, refuses unless it is confirmed for the team file's entry now (`connector_not_confirmed`), checks the allowances as connect does, keeps the same keys or grant beside the new entry's hash, and handles `connector_connect` with the new entry, which writes the team file and records `connector.connected`. Raising it does not allow the call that is waiting: the human still allows that one in `ToolApproval`. On the command line, `farik connect <agent> <name> --allowance <tool>=<n>` (repeatable, kit connectors only, `kit_names_these` otherwise) sets them at a connect. Rejected: an event kind of its own (`connector.connected` already says what was connected).
- **`connector.connected` gains `allowances`**, present when the entry has any.
- **The period.** While a sprint is open, the calls made since its `sprint.started`; with none open, the calls since the later of the UTC day's start (the daemon's clock) and the last `sprint.ended`, which covers a project that runs no sprints and the hours between sprints (5.5's daily budget counts the same day) without counting a sprint's calls again after it ends. `AllowancePeriod::Day` carries `from_seq: Option<u64>`, the `sprint.ended`'s sequence when it falls on that day. Rejected: the team's sprint policy deciding, because a team that plans in sprints has hours with none open.
- **What counts.** Every `tool.called` of that agent, server and tool in the period, the calls a grant allowed among them, since each spent the user's credits. Rejected: counting only the calls the allowance let through, which would have the board say less than was made.
- **How it is counted.** `tool.called` is recorded for every allowed call (`Read`, `Grep`, `Bash` and Farik's tools too), so reading the log on each spending call would decode thousands of events under the sessions lock. `DaemonState` keeps `AllowanceCounts`: the period it was read for, and a count per `(agent, server, tool)`. A key is read from the log with `calls_in_period` the first time it is asked for in a period (`EventLog::read` with the agent and `tool.called`, after the period's start, served by the `events_by_agent_and_kind` index); after a `tool.called` the hook appends for a call inside the allowance or under a grant, it is raised by one if it is held (one not yet held is read later, the call included). When `allowance_period` answers a period other than the one held, the counts are dropped. A restart starts again from the log. A count matches `ToolCalledBody.server` to the server's name and `ToolCalledBody.tool` to the full `mcp__<server>__<tool>`, never the bare tool name. Rejected: a migration and a table for a number the daemon can hold.
- **The order in the hook** (5.6): the session's connector, the tag, `denied`, the preview's `url`s, the Designer's plan gate, the 64 KiB limit, then the human's grant for this exact call, then the allowance, then the ask. A grant first, because it is for exactly this call and lapses otherwise. The count and the `tool.called` it is checked against are read and recorded under the daemon's one sessions lock, which the grant already takes, so two calls at the last place cannot both run. A call the allowance lets through records `tool.called` with `allowance: <n>`, the number it ran inside.
- **In `farik-core`.** `SessionConnector` gains `allowances`; `evaluate_connector_call` takes `used`, the count before this call, and answers `ConnectorPass { tag, approval, allowance }`: `approval` when a grant allowed it, else `allowance` when `used` is under it, else `ApprovalNeeded`. A `network` call ignores both.
- **What the pages read.** The query `allowances.list` answers `{ period, rows }`: `period` `{ kind: "sprint", sprint_id }` or `{ kind: "day", day }`, and one row per allowance of a confirmed kit entry of an active agent, `{ agent, server, tool, what, used, of }`, `what` from the kit, `used` read through the same `AllowanceCounts` the hook uses. The board shows each row beside the sprint's progress, "<name>: 14 of 20 images this sprint" (or "today"), and at the limit "20 of 20 images. <name> asks you before making more."; the Costs page lists them under "Made on other services" with "Farik counts what agents made, not what the service charges. Check your bill there."; `ToolApproval`, for a tool with a row, says "<name> has made 20 of 20 images this sprint." with "Change how many", which opens `ConnectorAllowance` on the agent page. `used` may exceed `of`, since granted calls count: the board and Costs show it as it is ("21 of 20 images") with the sentence "Extra images were ones you approved." (the row's `what`), and the asking line shows whenever `used >= of`. If the kit has changed the entry since it was connected, `connector.allowances` fails `connector_not_in_kit` inside `connector_connect`, and the dialog shows "Connect again" instead. The page refreshes on `tool.called` and `connector.connected`, as it does on other events.
- **`ConnectorAllowance`** is a step of `ConnectorAdd` for a kit connector with allowances, after the key or the sign-in, and a dialog from the agent's kit row ("Change how many"), as step 05's Task 1 drew it.
- **ADR 0037** records the hash, the period, what counts, the count's cache and the order, and two limits: a session's allowances are fixed when it registers, so a change made with `connector.allowances` while it runs applies from the agent's next session, as tiers do (spec 4.4); and the count's lock is per daemon, which holds because one process drives a project (the CLI sends its commands to the daemon when one runs).

For the founder: none. The boards are step 05's O1.

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs                  modifies: allowances, the hash (Task 1)
crates/core/src/governor/permissions.rs                                  modifies: SessionConnector, ConnectorPass (Task 2)
crates/runtime/src/daemon/team.rs, orchestrator/human.rs                 modifies: allowances at connect, connector.allowances (Task 3)
docs/schemas/rpc.schema.json, event.schema.json                          modifies: connector.connect, connector.allowances, allowances.list; connector.connected, tool.called (Tasks 3 to 5)
crates/cli/src/connector.rs, main.rs, crates/cli/tests/connector.rs      modifies: --allowance (Task 3)
crates/runtime/src/allowances.rs                                         creates: the period and the count (Task 4)
crates/runtime/src/daemon/hooks.rs, orchestrator/session.rs              modifies: the hook's allowance, the registration's (Task 4)
crates/runtime/src/daemon/board.rs                                       modifies: allowances.list (Task 5)
apps/web/src/pages/{ConnectorAdd,AgentEdit,Board,Costs}.tsx, dialogs/ToolApproval.tsx, dialogs/ConnectorAllowance.tsx, strings/en.ts, allowances.test.tsx   creates/modifies (Task 6)
docs/decisions/0037-allowances-count-calls-per-agent-and-period.md     creates (Task 7)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md       modifies (Task 7)
```

## Interfaces

Consumes: `Kit`, `KitConnector::Server { allowances }`, `KitAllowance`, `kit_entry`, `matches_kit`, `ToolDeps.kits`, `McpServerSource::Kit`, `CustomServer.kit` (step 05); `evaluate_connector_call`, `SessionConnector`, `ConnectorRefusal` (`farik-core`); `judge_connector`, `grant_for`, `ask`, the sessions lock, `connector_connect` and its command, `entry_lock`, `ConnectorEntry` (steps 01 to 03); `Projections::open_sprint`, `EventQuery`, `EventLog::read` (`farik-store`, main); `ToolApproval` (step 02).

Produces:

```rust
// farik-core
pub struct CustomServer { /* as in step 05 */ pub allowances: BTreeMap<String, u32> }
pub struct SessionConnector { /* as now */ pub allowances: BTreeMap<String, u32> }
pub struct ConnectorPass { pub tag: ConnectorTag, pub approval: Option<u64>, pub allowance: Option<u32> }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>,
    granted: Option<u64>, used: u32) -> Result<ConnectorPass, ConnectorRefusal>;
pub const MAX_ALLOWANCE: u32 = 1000;

// farik-runtime
pub enum AllowancePeriod { Sprint { sprint_id: String, started_seq: u64 }, Day { day: NaiveDate, from_seq: Option<u64> } }
pub struct AllowanceCounts { pub period: Option<AllowancePeriod>, pub used: HashMap<(String, String, String), u32> }   // in DaemonState
impl AllowanceCounts {
    pub fn used(&mut self, log: &EventLog, period: &AllowancePeriod, agent: &str, server: &str, tool: &str) -> Result<u32, StoreError>;
    pub fn raise(&mut self, agent: &str, server: &str, tool: &str);
}
pub fn allowance_period(log: &EventLog, projections: &Projections, now: DateTime<Utc>)
    -> Result<AllowancePeriod, StoreError>;
pub fn calls_in_period(log: &EventLog, agent: &str, server: &str, tool: &str,
    period: &AllowancePeriod) -> Result<u32, StoreError>;
pub fn checked_allowances(kit: &Kit, name: &str, asked: &BTreeMap<String, u32>)
    -> Result<BTreeMap<String, u32>, String>;   // the kit's defaults overlaid by asked; refusal codes as above
```

Wire: `connector.connect` gains optional `allowances` (tool to integer); `connector.allowances { agent, server, allowances }` answers `{}`; `allowances.list {}` answers as above; `connector.connected` and `team.schema.json`'s `mcpServer` gain `allowances`; `tool.called` gains `allowance` (1 to 1000).

## Tasks

### Task 1: Allowances in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, and every `CustomServer { … }` literal (step 05's Task 3 list: `runtime/src/connectors.rs`, `runtime/src/daemon/team.rs`, `runtime/tests/fixture_mcp.rs`, `runtime/tests/fixture_oauth.rs`, core's tests), given `allowances: BTreeMap::new()`.

- `accepts_allowances_on_a_kit_entrys_external_tools`: `allowances: { generate_image: 20 }` on a kit entry tagging it `external_effect` validates and reads back in `CustomServer.allowances`. RED: no field.
- `refuses_an_allowance_on_a_custom_entry`: `allowance_not_kit` at its field.
- `refuses_an_allowance_on_a_tool_not_external`: a `network` tool, and a tool the entry does not tag, each give `allowance_not_external`.
- `refuses_an_allowance_over_a_thousand`: 1001 is a schema error.
- `spec_hash_sees_an_allowance`: 20 and 21 hash differently; a kit entry without `allowances` hashes to the kit literal step 05's Task 3 pinned in `a_server_without_oauth_keeps_its_hash`.

- [x] `feat(core): keep a kit connector's allowances in the team file`

### Task 2: The call inside its allowance

Files: `crates/core/src/governor/permissions.rs`; `daemon/hooks.rs` (`judge_connector` passes `used: 0` until Task 4); and every `SessionConnector { … }` literal (`daemon/hooks.rs`'s three tests, `orchestrator/session.rs` (2), `daemon.rs` (2), `crates/cli/tests/connector_run.rs`, `crates/cli/tests/live_claude.rs`), given `allowances: BTreeMap::new()`. Produces `ConnectorPass` and the new `evaluate_connector_call`.

- `runs_a_call_inside_the_allowance`: allowance 20, `used` 19, no grant: `allowance: Some(20)`, `approval: None`. RED: no allowance in the decision.
- `asks_for_the_call_beyond_it`: `used` 20 of 20 gives `ApprovalNeeded`.
- `asks_at_any_count_for_a_tool_without_one`: no allowance, `used` 0, gives `ApprovalNeeded`.
- `asks_every_time_at_zero`: allowance 0, `used` 0, gives `ApprovalNeeded`.
- `uses_a_grant_before_the_allowance`: grant 7, `used` 0: `approval: Some(7)`, `allowance: None`.
- `refuses_a_large_input_inside_the_allowance`: over 64 KiB gives `InputTooLarge`.
- `a_network_call_ignores_the_allowance`: neither field is set.

- [x] `feat(core): let a connector call inside its allowance run`

### Task 3: Setting allowances

Files: `daemon/team.rs`, `orchestrator/human.rs`, `rpc.schema.json`, `event.schema.json` (`connector.connected`), `crates/cli/src/connector.rs`, `main.rs`. Tests over step 05's fixture kit, given an `external_effect` tool `make` with `{ calls: 20, what: "pictures" }` and a `post` tool with none.

- `connects_with_the_kits_default_allowances`: `connector.connect` without `allowances` writes `{ make: 20 }` and records it on `connector.connected`. RED: no allowances written.
- `connects_with_the_users_allowances`: `{ make: 5 }` is written.
- `refuses_an_allowance_the_kit_does_not_offer`: `{ post: 3 }` gives `allowance_not_offered` and keeps nothing.
- `refuses_an_allowance_out_of_range`: `{ make: 1001 }` gives `allowance_out_of_range`.
- `changes_allowances_without_the_key`: `connector.allowances` with `{ make: 30 }` writes the entry and its new hash beside the same keys, records `connector.connected`, and the server stays `connected`.
- `refuses_to_change_an_unconfirmed_entry`: after a hand edit of the entry's `url`, `connector_not_confirmed`.
- `matches_a_kit_entry_whose_allowance_differs_from_the_kits_default`: `matches_kit` of an entry with `{ make: 5 }` is true; with `{ post: 3 }` it is false.
- `farik_connect_takes_allowances`: `--allowance make=3` writes 3; with a custom server, `kit_names_these`.

- [x] `feat(runtime): set a kit connector's allowances`

### Task 4: Counting in the hook

Files: `allowances.rs`, `daemon.rs` (`DaemonState`'s `AllowanceCounts`), `daemon/hooks.rs`, `orchestrator/session.rs` (the registration's `SessionConnector.allowances` from the entry), `event.schema.json` (`tool.called`'s `allowance`). Tests drive the hook route as step 02's do.

- `runs_a_call_inside_its_allowance_without_asking`: allowance 2 and one earlier call: the call is allowed, `tool.called` carries `allowance: 2`, and no `tool_approval.requested` is recorded. RED: the call asks.
- `asks_for_the_first_call_beyond_it`: the third call records `tool_approval.requested` and is denied `approval_needed`.
- `counts_per_agent_and_per_tool`: another agent's calls to the tool, and this agent's to another tool, do not count.
- `resets_with_the_sprint`: calls before the open sprint's `sprint.started` do not count.
- `counts_per_utc_day_with_no_sprint_open`: a call at 23:59 UTC yesterday does not count, one at 00:01 today does.
- `counts_from_the_sprints_end_on_the_day_it_ended`: a sprint ended at 15:00 today; its calls from 09:00 do not count, a call at 16:00 does.
- `a_restart_reads_the_count_from_the_log`: two calls, a new `DaemonState` over the same log, and the next call sees `used` 2.
- `counts_by_server_and_full_tool_name`: a `tool.called` of another server whose bare tool name is `make` does not count.
- `counts_a_call_a_grant_allowed`: a granted call uses one place.
- `lets_one_of_two_calls_at_the_last_place_run`: two hook requests at once with one place left: one is allowed and one asks.
- `registers_each_connectors_allowances`: the session's registration carries the entry's allowances.

- [x] `feat(runtime): count a connector's calls against its allowance`

### Task 5: Each allowance's use

Files: `daemon/board.rs`, `rpc.schema.json`.

- `lists_each_allowance_with_its_use_and_period`: with a sprint open and three calls, `{ period: { kind: "sprint", sprint_id: "S1" }, rows: [{ agent, server, tool: "make", what: "pictures", used: 3, of: 20 }] }`. RED: no query.
- `matches_the_tool_called_events`: the row's `used` equals the `tool.called` events in the period, a granted one among them.
- `says_the_day_with_no_sprint_open`: `{ kind: "day", day: "<today, UTC>" }`.
- `leaves_out_an_unconfirmed_entry_and_a_paused_agent`.

- [x] `feat(runtime): answer each allowance's use`

### Task 6: The screens

Files: as the file map's Task 6 line. Built from step 05's approved boards.

- `connector_add_asks_how_many_for_a_kit_with_allowances`: after the key, the allowance step shows each tool's `what` and the kit's default, and Connect sends the numbers.
- `connector_add_skips_the_step_without_allowances`.
- `connector_allowance_changes_the_number`: the dialog from the agent page sends `connector.allowances` and refuses 1001 at its field; on `connector_not_in_kit` it shows "Connect again".
- `board_shows_each_allowance_beside_the_sprint`: "Kai: 14 of 20 pictures this sprint"; the asking line at 20 of 20; at 21 of 20, "21 of 20 pictures", the asking line and "Extra pictures were ones you approved."
- `costs_lists_what_was_made_on_other_services`: the rows, the period and the bill sentence.
- `tool_approval_says_the_count_for_a_tool_with_an_allowance`: the line and "Change how many".
- `allowance_screens_never_name_the_plumbing`: Farik's strings in `strings/en.ts` for these screens and the fixture kit's `what`s hold no "MCP", "OAuth" or "token". Service tool names are not checked.

- [ ] `feat(web): set and show each connector's allowance`

### Task 7: Spec and plan

ADR 0037. `docs/SPEC.md` 6.7 (allowances: where, defaults, changing, the period, what counts, a count past the allowance), 5.6 (the hook's order), 5.5 (the period beside the daily budget), 8.5 (`connector.connected`'s and `tool.called`'s new fields), F9, F17. `docs/plans/project-plan.md` row 05b, corrected if execution changed it; `docs/design/role-kits.md`.

- [ ] `docs(spec): record allowances`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

No shipped kit has a spending tool until step 08, so the flow is proven here by Tasks 3 to 6's tests against the fixture kit, and in the web app by step 08's Higgsfield check (row 08: "the allowance flow proven end to end").
