# Customizable governor, plan 1: the core

Status: ready
Branch: `feat/customizable-governor`
Spec and decisions: see `README.md`; ADR 0080 decisions 2 to 10

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/` unless they start with `docs/`. `catervas-core` does no I/O (hard rule 5).

## Decisions

- The team file's types are generated from `team.schema.json` at compile time (ADR 0009), so Task 1's schema change is what adds the fields to `Team`; no hand-written wire struct changes.
- `TeamRules` gains `read_only_paths: Vec<String>` and `git_subcommands: Vec<String>`; `Default` leaves both empty. Chose fields on `TeamRules` over a second struct: every consumer already receives `TeamRules`.
- `Team::rules()` stops keeping the shipped globs unconditionally: shipped minus `rules.lifted_paths`, then the team's `protected_paths`. The doc comment that said a team "could not delete `.env`" is replaced by one citing ADR 0080 decision 2.
- The floor (`CATERVAS_STATE`, `CATERVAS_KEYS`) is checked inside `evaluate_tool_call`, not added to `protected_paths`: a rule list the owner edits must not be where the floor lives, and the prompt must not list it as a team rule.
- Order of refusals in `evaluate_tool_call`: tier, Catervas's keys (`PathProtected`), protected paths (`PathProtected`), then for `write_workspace` only: paths missing, read-only paths and `CATERVAS_STATE` (`PathReadOnly`), allowed paths, then approval. Chose read-only before allowed paths so that the owner sees why ("read only") rather than "outside the task".
- `evaluate_command`: a segment's git subcommand passes when it is in `READ_ONLY_GIT` with no acting option (as now), or in `rules.git_subcommands` with any options. `GIT_OPTIONS_THAT_ACT` applies to the read-only eight alone: an owner who lists `diff` in `git_subcommands` has allowed it with every option.
- `evaluate_readiness_attempts(failed_attempts, limit)`; `READINESS_ATTEMPT_LIMIT` stays as the default `Team::readiness_attempts()` returns.
- `ConnectorTag::Ask` (wire `ask`) goes the `Paid` way in `evaluate_connector_call`: a grant or an allowance, else `ApprovalNeeded`. Every exhaustive `match` on `ConnectorTag` the compiler names treats `Ask` as it treats `Paid`, except a display of money ("costs money"), which says "asks first".
- `validate_team` refuses an `agents[].rules.lifted_paths` glob that is in neither the shipped five nor the team's `protected_paths` or `read_only_paths`, with `ValidationError` at `/agents/<i>/rules/lifted_paths/<j>`, code `lifted_path_unknown`: lifting a glob nobody set is a typo that would silently do nothing.

## File map

```
docs/schemas/team.schema.json            modifies: the fields in README (T1), connectorTag ask (T6)
core/src/team.rs                         modifies: rules(), rules_for(), push_remotes(), readiness_attempts(), validation (T1, T2); tests
core/src/team/fixtures.rs                modifies: a fixture with every new field (T1)
core/src/governor/team_rules.rs          modifies: TeamRules fields, CATERVAS_STATE, CATERVAS_KEYS (T2, T3); tests
core/src/governor/permissions.rs         modifies: ToolCallContext, PathReadOnly, evaluate_tool_call (T3), evaluate_command (T4), ConnectorTag::Ask (T6); tests
core/src/governor/escalation.rs          modifies: evaluate_readiness_attempts limit (T5); tests
core/src/governor/transition.rs          modifies: TransitionContext.readiness_attempt_limit (T5); tests
```

## Interfaces

Consumes (on `main`): `TeamRules`, `DEFAULT_PROTECTED_PATHS`, `check_protected_paths`, `evaluate_tool_call`, `evaluate_command`, `READ_ONLY_GIT`, `evaluate_connector_call`, `evaluate_readiness_attempts`, `validate_team`, `Team`.

Produces:

```rust
// core/src/governor/team_rules.rs
pub const CATERVAS_STATE: &str = ".catervas/local/**";
pub const CATERVAS_KEYS: [&str; 2] = [".catervas/local/daemon.json", ".catervas/local/sessions/**"];
pub struct TeamRules { /* existing */ pub read_only_paths: Vec<String>, pub git_subcommands: Vec<String> }
impl TeamRules { pub fn unwritable(&self) -> Vec<String>; } // protected_paths then read_only_paths, deduplicated
// core/src/team.rs
impl Team {
    pub fn rules_for(&self, agent_id: &str) -> TeamRules;
    pub fn push_remotes(&self) -> Vec<String>;        // policy.push_remotes or ["origin"]
    pub fn readiness_attempts(&self) -> u32;          // policy.readiness_attempts or READINESS_ATTEMPT_LIMIT
}
// core/src/governor/permissions.rs
pub struct ToolCallContext { /* existing */ pub read_only_paths: Vec<String> }
pub enum ToolRefusal { /* existing */ PathReadOnly { path: String } }
pub enum ConnectorTag { /* existing */ Ask }
// core/src/governor/escalation.rs
pub fn evaluate_readiness_attempts(failed_attempts: u32, limit: u32) -> ReadinessOutcome;
// core/src/governor/transition.rs
pub struct TransitionContext { /* existing */ pub readiness_attempt_limit: u32 }
```

## Tasks

### Task 1: the team file holds the new rules

Files: `docs/schemas/team.schema.json`, `core/src/team.rs`, `core/src/team/fixtures.rs`
Produces: the generated fields; `Team::push_remotes`, `Team::readiness_attempts`; the `lifted_path_unknown` check
Tests (`team.rs` `mod tests`):
- `reads_every_owner_rule_and_defaults_what_is_left_out` — asserts that the fixture with every new field validates and each reads back as written, that a team with none of them validates with `push_remotes() == ["origin"]` and `readiness_attempts() == 3`, and that `git_subcommands: ["Push"]`, `push_remotes: []` and `lifted_paths: ["secrets/**"]` at team level are each refused at their own pointer.
- `refuses_an_agent_lifting_a_path_nobody_set` — asserts `lifted_path_unknown` at `/agents/0/rules/lifted_paths/0` for `["infra/**"]` when the team sets no such glob, and acceptance when the team's `protected_paths` holds `infra/**` or the glob is `.env`.

- [x] `feat(core): the team file holds the owner's governor rules`

### Task 2: an agent's rules are the team's with its own applied

Files: `core/src/governor/team_rules.rs`, `core/src/team.rs`
Produces: `TeamRules.read_only_paths`, `TeamRules.git_subcommands`, `TeamRules::unwritable`, `Team::rules` (changed), `Team::rules_for`
Tests (`team.rs` `mod tests`):
- `lifts_a_shipped_path_and_keeps_the_others` — asserts that `lifted_paths: [".env"]` with `protected_paths: ["infra/**"]` gives `rules().protected_paths == [".env.*", "**/*.pem", "**/*.key", ".catervas/local/**", "infra/**"]`, and that a team writing only `protected_paths: ["infra/**"]` (a file from before ADR 0080) still has all five shipped globs.
- `applies_an_agents_rules_on_top_of_the_teams` — asserts, for a team with `protected_paths: ["infra/**"]`, `read_only_paths: ["vendor/**"]`, `forbidden_commands: ["^curl"]`, `git_subcommands: ["fetch"]` and agent `ada` with `rules: { lifted_paths: ["infra/**", "vendor/**"], protected_paths: ["notes/**"], forbidden_commands: ["^wget"], git_subcommands: ["merge"] }`: `rules_for("ada")` has `notes/**` and no `infra/**` in `protected_paths`, no `vendor/**` in `read_only_paths`, both patterns, both subcommands; `rules_for("theo")` equals `rules()`; `rules_for("nobody")` equals `rules()`.

- [ ] `feat(core): an agent's rules are the team's with its own applied`

### Task 3: read-only paths and Catervas's floor

Files: `core/src/governor/permissions.rs`, `core/src/governor/team_rules.rs`
Produces: `CATERVAS_STATE`, `CATERVAS_KEYS`, `ToolCallContext.read_only_paths`, `ToolRefusal::PathReadOnly`
Tests (`permissions.rs` `mod tests`):
- `a_read_only_path_is_read_and_never_written` — asserts that with `read_only_paths: [".env"]` and `allowed_paths: [".env", "src/**"]` a `read` call on `.env` passes and a `write_workspace` call on `.env` is `PathReadOnly { path: ".env" }`.
- `catervas_state_is_never_written_and_its_keys_never_read` — asserts that with `protected_paths` empty (everything lifted) and `allowed_paths: ["**"]`: a `read` of `.catervas/local/catervas.db` passes; a `write_workspace` of it is `PathReadOnly`; a `read` of `.catervas/local/daemon.json` and of `.catervas/local/sessions/s1/settings.json` is `PathProtected`.

- [ ] `feat(core): read-only paths, and Catervas's own state is never written`

### Task 4: the owner's git subcommands run in commands

Files: `core/src/governor/permissions.rs`
Produces: `evaluate_command` reading `rules.git_subcommands`
Tests (`permissions.rs` `mod tests`):
- `runs_the_git_the_owner_allowed` — asserts that with `git_subcommands: ["fetch", "diff"]`: `git fetch origin` and `cd x && git diff --output=p.txt` pass; `git push` and `sudo git merge main` are `GitViaExec`; with `git_subcommands` empty, `git diff --output=p.txt` is `GitViaExec` and `git status` passes (today's behavior).

- [ ] `feat(core): commands run the git subcommands the owner allows`

### Task 5: the owner sets how many readiness attempts a task gets

Files: `core/src/governor/escalation.rs`, `core/src/governor/transition.rs`
Produces: `evaluate_readiness_attempts(failed_attempts, limit)`, `TransitionContext.readiness_attempt_limit`
Tests:
- `escalation.rs`: `retries_readiness_until_the_limit_and_escalates_at_it` (replaces `retries_readiness_twice_and_escalates_on_the_third_failure`) — asserts `Retry` for 0..4 and `Escalate` at 5 with limit 5, and `Escalate` at 1 with limit 1.
- `transition.rs`: the existing readiness-escalation test sets `readiness_attempt_limit: 2` and asserts escalation after two failures. Every other `TransitionContext` built in tests sets `READINESS_ATTEMPT_LIMIT`.

- [ ] `feat(core): the owner sets the readiness attempts before escalation`

### Task 6: a connector tool may ask first

Files: `docs/schemas/team.schema.json` (`connectorTag`'s enum and description gain `ask`), `core/src/governor/permissions.rs`
Produces: `ConnectorTag::Ask`
Tests (`permissions.rs` `mod tests`):
- `an_ask_tool_waits_for_the_owner_as_a_paid_one_does` — asserts that an `ask` tool with no grant and no allowance is `ApprovalNeeded`, with a grant passes with `approval: Some(id)`, and with an allowance of 2 and `used: 1` passes with `allowance: Some(2)`.

- [ ] `feat(core): a connector tool may ask the owner first`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo test -p catervas-core
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask core-io
# expected: test result: ok; core-io: ok
```

`catervas-runtime` does not compile between Task 3 and plan 2's Task 7 if a field without a default is added to `ToolCallContext` or `TransitionContext`; each of Tasks 3 and 5 sets the new field at every construction site the compiler names (`read_only_paths: team.rules().read_only_paths`, `readiness_attempt_limit: READINESS_ATTEMPT_LIMIT`), so the workspace builds after every commit; plan 2 replaces them with the per-agent values.
