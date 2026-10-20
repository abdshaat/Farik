# Customizable governor, plan 2: the runtime

Status: ready
Branch: `feat/customizable-governor`, after plan 1
Spec and decisions: see `README.md`; ADR 0080 decisions 1 to 10

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/`.

## Decisions

- Every place that reads the protected paths for a session or a task reads `team.rules_for(<agent>)`:
  - the PreToolUse hook (`runtime/src/daemon/hooks.rs`, the `ToolCallContext` near line 379 and the read roots near 524): `registration.agent_id`;
  - Catervas's own tools (`runtime/src/tools.rs` near 734; `tools/folders.rs` near 631 and 742; `tools/exec.rs` line 40): `call.agent_id()`;
  - the done check (`runtime/src/transitions.rs` near 726): the contract's `assignee`, `Team::rules()` when there is none;
  - the session's Claude Code settings (`runtime/src/claude.rs` `protected_paths`, `settings_json`): `spec.agent_id`;
  - the prompt's rules section (`runtime/src/prompt.rs` `rules_section`): the session's agent.
  The project scan (`daemon/team.rs` `scanned`), `catervas doctor`, `catervas team` and `store/src/requests.rs`'s rules JSON stay on `Team::rules()`.
- The done check refuses a diff touching any of `rules.unwritable()` (protected then read only): a read-only file is not to be changed by a task either. `DoneEvidence.protected_paths` keeps its name and receives `unwritable()`.
- Claude Code's `permissions.deny` (`settings_json`): `Read(<glob>)` for each of `protected_paths` and each of `CATERVAS_KEYS`; `Edit(<glob>)` for each of `read_only_paths` and for `CATERVAS_STATE`. `refuse_an_unexpressible_glob` runs over all four lists. The read roots' rules (`read_root_rules`) take `protected_paths` plus `CATERVAS_KEYS`.
- The read roots (`hooks.rs` `judge_in_root`) keep adding `.git/**` and add `CATERVAS_KEYS`; a read root is read only by construction, so read-only paths need nothing there.
- `catervas_git_push` takes `PushInput { remote: Option<String> }` (`#[serde(deny_unknown_fields)]`, `snake_case`). A `remote` not in `team.push_remotes()` is refused `remote_not_allowed` naming the remote and the list; absent, the first listed. The tool's description becomes "Push the task branch to a remote the owner allows (origin unless the owner set others)." The result JSON keeps `{ "remote", "branch" }`.
- The prompt's rules section adds `read_only_paths` and `git_subcommands` lines, as the other lists are (said only when not empty), and a `push_remotes` line when the agent holds `git_remote`. It never lists `CATERVAS_STATE` or `CATERVAS_KEYS`.
- `budget_state` gains `agent_id: Option<&str>`; after `team.budgets.session` it applies the agent's `session` field by field the same way. Its five callers pass the session's or the task assignee's id (`None` where there is no agent).
- The readiness limit: `transitions.rs` near 740 sets `readiness_attempt_limit: team.readiness_attempts()`.
- `refusal.rs` maps `ToolRefusal::PathReadOnly` to `("path_read_only", "<path> is read only: the owner lets agents read it, not change it")` and the push's refusal to `remote_not_allowed`. Any `event.schema.json` enum of refusal reasons gains both.

## File map

```
runtime/src/daemon/hooks.rs              modifies: rules_for at both sites, keys in read roots (T7); tests
runtime/src/tools.rs                     modifies: rules_for, read_only_paths (T7); push tool input and description (T9)
runtime/src/tools/folders.rs             modifies: rules_for (T7)
runtime/src/tools/exec.rs                modifies: rules_for (T7); tests
runtime/src/tools/refusal.rs             modifies: path_read_only, remote_not_allowed (T7, T9)
runtime/src/transitions.rs               modifies: done check unwritable (T8), readiness limit (T11); tests
runtime/src/claude.rs                    modifies: per-agent deny rules (T8); tests
runtime/src/tools/git.rs                 modifies: push to an allowed remote (T9); tests
runtime/src/prompt.rs                    modifies: rules section (T10); tests
runtime/src/cost.rs, its five callers    modifies: agent session limits (T12); tests
runtime/src/daemon/team.rs               modifies: kit rows' plan_approved (T12b); tests
runtime/src/daemon/hooks.rs              modifies: ask()'s words for an ask tool (T12b); tests
cli/src/team.rs, cli/src/doctor.rs       modifies: show and check the new lists (T10)
docs/schemas/event.schema.json           modifies: refusal names, if enumerated (T7, T9)
```

## Interfaces

Consumes: `Team::rules_for`, `Team::push_remotes`, `Team::readiness_attempts`, `TeamRules::unwritable`, `CATERVAS_STATE`, `CATERVAS_KEYS`, `ToolRefusal::PathReadOnly`, `TransitionContext.readiness_attempt_limit` (plan 1); `GitRepo::push` (`store/src/git.rs`, on main).

Produces:

```rust
// runtime/src/tools/git.rs
#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]
pub struct PushInput { pub remote: Option<String> }
// runtime/src/cost.rs
pub fn budget_state(projections: &Projections, team: &Team, role: Role, agent_id: Option<&str>,
    task: Option<&TaskContract>, session: &SessionLedger, now: DateTime<Utc>) -> Result<BudgetState, CostError>;
```

## Tasks

### Task 7: each agent's calls are judged by its own rules

Files: `runtime/src/daemon/hooks.rs`, `runtime/src/tools.rs`, `runtime/src/tools/folders.rs`, `runtime/src/tools/exec.rs`, `runtime/src/tools/refusal.rs`
Tests:
- `hooks.rs`: `judges_a_call_by_the_agents_own_rules` — with a team protecting `infra/**` and agent `ada` lifting it, a `Read` of `infra/main.tf` from `ada`'s registration is allowed and from `theo`'s is denied `path_protected`; an `Edit` of a team read-only `vendor/x.js` inside the allowed paths is denied `path_read_only`; a `Read` of `.catervas/local/daemon.json` with every shipped path lifted is denied `path_protected`.
- `exec.rs`: `runs_the_git_the_agent_is_allowed` — `git fetch` through `catervas_exec` is refused for an agent without `fetch` and passes the governor for one whose `rules.git_subcommands` holds it (the command's own run uses the test sandbox, as its neighbours do).

- [x] `feat(runtime): each agent's calls are judged by its own rules`

### Task 8: a session's settings and the done check use the agent's rules

Files: `runtime/src/claude.rs`, `runtime/src/transitions.rs`
Tests:
- `claude.rs`: `denies_reads_of_off_limits_and_edits_of_read_only_paths` (extends `wires_both_hooks_and_the_protected_paths_into_the_settings`) — for agent `ada` with `.env` lifted by the team and `read_only_paths: [".env"]`, `permissions.deny` holds `Edit(.env)`, no `Read(.env)`, `Read(.catervas/local/daemon.json)`, `Read(.catervas/local/sessions/**)` and `Edit(.catervas/local/**)`.
- `transitions.rs`: `hands_the_done_check_the_assignees_unwritable_paths` (extends `hands_the_done_check_the_teams_protected_paths`) — the evidence's `protected_paths` for a task assigned to `ada` holds the team's read-only `vendor/**` and not the `infra/**` `ada` lifted.

- [x] `feat(runtime): a session's settings and the done check follow the agent's rules`

### Task 9: an agent pushes to a remote the owner allows

Files: `runtime/src/tools/git.rs`, `runtime/src/tools.rs`, `runtime/src/tools/refusal.rs`
Tests (`git.rs` `mod tests`, `#[ignore = "needs the git program: cargo xtask check --integration"]` like their neighbours):
- `pushes_to_a_remote_the_owner_allows` — with `push_remotes: ["origin", "fork"]` and two bare repositories as remotes, `{ "remote": "fork" }` puts `refs/heads/<branch>` on `fork` and not on `origin`; `{}` pushes to `origin`; `{ "remote": "upstream" }` is refused `remote_not_allowed` and pushes nothing.

- [x] `feat(runtime): an agent pushes to the remotes the owner allows`

### Task 10: the agent is told its rules, and the CLI shows the team's

Files: `runtime/src/prompt.rs`, `cli/src/team.rs`, `cli/src/doctor.rs`
Tests:
- `prompt.rs`: the two existing rules-section tests gain `read_only_paths` and `git_subcommands` and assert their lines; a third, `says_the_remotes_to_an_agent_that_pushes`, asserts the `push_remotes` line with `git_remote` and its absence without.
- `doctor.rs`: `names_a_read_only_glob_that_does_not_compile` — a team with `read_only_paths: ["a/[b"]` gets the same message shape as `protected_paths`, naming `read_only_paths`; agents' globs are checked the same, naming `agents[<id>].rules`.

- [x] `feat(runtime): agents are told their own rules`

### Task 11: the team's readiness attempts are used

Files: `runtime/src/transitions.rs`
Tests: `escalates_readiness_at_the_teams_limit` — a team with `readiness_attempts: 1` escalates on the first failed readiness check; the default team still retries twice.

- [x] `feat(runtime): readiness escalates at the team's limit`

### Task 12: an agent's own session budget

Files: `runtime/src/cost.rs` and its callers in `transitions.rs`, `orchestrator/human.rs`, `orchestrator/rules.rs`, `orchestrator/session.rs`
Tests (`cost.rs`): `an_agents_session_limits_override_the_teams` — team `budgets.session.max_tool_calls: 100`, agent `ada` `session.max_tool_calls: 50` and `max_wall_clock_seconds: 600`: `ada`'s state has 50 calls and 10 minutes, `theo`'s 100 and the role's 30 minutes, `None` the team's.

- [x] `feat(runtime): an agent's own session budget`

### Task 12b: the web app learns which tools the owner's plan marks, and an asking tool does not say it costs money

Added 2026-10-20 after plan 3: its `ToolChoice` hides the choice for a plan-marked tool (decision 10), which it reads from a kit row's `plan_approved` (`apps/web/src/pages/Team.tsx` `KitService.planApproved`), and nothing sends it.

Files: `runtime/src/daemon/team.rs` (the kit rows built near line 754), `runtime/src/daemon/hooks.rs` (`ask`), `docs/schemas/rpc.schema.json` (the kit row, if it is described there)
Decisions: a kit row gains `plan_approved`, an array of tool names, sorted, set only when not empty: the tools `plan_tools_of` would mark for that connector (read from the kit entry itself, the same source). `ask(...)` gains the tool's `ConnectorTag`; for a session about no task it says "{tool} of {server} costs money" for `paid` and "{tool} of {server} asks you first" for `ask`, the rest of the sentence unchanged.
Tests:
- `team.rs`: `a_kit_row_names_the_tools_the_owners_plan_approves` — the Marketing Specialist's kit row for a connector with plan-marked tools carries them in `plan_approved`; a connector with none has no `plan_approved` key.
- `hooks.rs`: `an_ask_tool_outside_a_task_says_it_asks_not_that_it_costs` — an `ask` tool called from a session about no task is denied with text containing "asks you first" and not "costs money".

- [ ] `feat(runtime): the web app learns which tools the owner's plan marks`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```
