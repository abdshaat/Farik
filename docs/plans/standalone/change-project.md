# Standalone: changing the project from the web app

Status: ready
Branch: `feat/change-project` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1 (first run, the project page), 4.4 (changing the team), 8.1 (the web UI paragraph), 8.4 (task ids)
Depends on: phase 7 (merged in #22)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-09 (one round, against docs/standards/workflow.md stage 2)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The founder approved `docs/design/change-project.md` on 2026-10-09; it is the source of truth for behavior and this plan does not restate its copy. Done means: the rail's foot names the project folder above "Connected"; Settings, "This computer", has **Change project…**, which asks, then stops the team (`project.leave`) and sends the page to the wizard's project page on the same port and browser session; there the page says which project the team is moving from and offers **Stay on**; opening or creating a folder takes it on with the team carried (retired agents dropped, every other agent active, pictures, the sandbox setting, pinned skills' folders, and connector keys copied to the new project's id), a folder with a Farik team being replaced only after a second confirmation; the new project drives with no setup screens and shows the copied-keys notice until **Keep them** or **Choose different keys**. A new task is numbered past every old task branch in the repository, so it never collides with one. Out of scope (design): recent projects, two projects at once, moving memory, tasks or history, changing project from the command line.

## Decisions

- One process drives one project at a time and can switch it from the browser: ADR 0050, amending ADR 0021 decision 2 ("One process serves one project"). Rejected: a second `farik serve` per project (two ports, two links, the design's single tab).
- `project.leave` stops the orchestrator through `Command::RunStop` (`daemon.rs` `handled`), as `farik stop` does, after setting `WebState.leaving` to the project root. serve reads it with `DaemonState::left()` before `finish`. Rejected: a new orchestrator state or a channel to serve (the stop path and `ticks`' `Ended::Stopped` already exist).
- `WebState.leaving: Mutex<Option<PathBuf>>` is one field for both modes: drive mode sets it on `project.leave`; serve sets it on the wizard's web state, as it sets `take_on_error`; `serve.status` reports it as `leaving` in both. Rejected: a `SetupHost::leaving()` method (two sources for one fact).
- serve's `Mode::Setup` becomes `Setup { waiting, leaving }`; `drive` answers `Driven::Ended(code)` or `Driven::Left(root)`, `Left` only when `left()` is set and `finish` answered 0 (Ctrl-C during a leave still exits 130). A take-on that fails keeps `leaving`, so **Stay on** still works. On a leave, `drive` moves `driver.interrupts` back into `io.interrupts` (`Interrupts::Channel`) before `finish`, as `start_holding` does on a refusal: `start_holding` left `never()` there, and the receiver `finish` drops is the only one Ctrl-C reaches, so without it the wizard after a leave would never hear Ctrl-C. The run lock is `Driver::_lock`, dropped when `finish` returns, so the old root is unlocked before the wizard's `open` calls `try_lock` on it.
- **Stay on** sends `project.open { path: <the absolute old root> }`. `CliHost::open`, while leaving, compares `home.join(path)` canonicalized with the old root canonicalized first; equal, it checks the lock and the account and chooses the old root, with no home check (an old root outside home, served from its own folder, can still be stayed on), no `init`, nothing written. Rejected: a separate `project.stay` method (the design names only open and create).
- `replace` is an optional boolean on `project.open` only: `project.create` refuses an existing folder already, so it never meets a team. `SetupHost::open` gains `replace: bool`.
- The refusal is a setup sentence starting with its code, `HAS_TEAM = "has_team: that folder already has a Farik team"`, answered `-32005` like every setup refusal; the page recognises the prefix `has_team`. Rejected: a structured `errors[]` refusal (setup refusals are sentences, `refusals.ts` `SENTENCES`).
- Replacing removes the target's whole `.farik/` with `remove_dir_all` (the target being the canonical git root `open` already checked is inside home and not home); a `.farik` that is a symbolic link is removed as a link (`symlink_metadata`, then `remove_file`), never followed to what it points at, then runs `git worktree prune` in the target, since the removed `.farik/local/worktrees/` leaves registrations behind. Task branches are code and are not touched (design: "your code is not touched"); new task numbers skip them (next decision).
- Task numbering skips old task branches (the founder, 2026-10-09): `file_request`'s existing floor, the highest committed contract number passed to `EventLog::next_task_id_above`, becomes the higher of that and the highest number of any branch, local or remote-tracking, named `feature/FRK-<n>`, `fix/FRK-<n>` or `docs/FRK-<n>` (the three shapes `task_branch` makes). That floor is the one place a task id is handed out (`crates/store/src/requests.rs`), so it holds for every project, a fresh clone and a hand-deleted `.farik/` included, not only the carried take-on. The name parsing is pure, in `farik-core` beside `task_branch`; listing the refs is `farik-store`'s `Git`. A folder whose refs cannot be listed (git fails) counts as having none, so filing never fails on it; a collision then shows as today, when the worktree is made. Rejected: a starting number recorded at `init` (a second counter to keep, and blind to branches made after `init`, by another clone among them); deleting or renaming old branches (touches the user's code).
- The carried team is `farik_core::team::carried_team`: the team minus retired agents, every other agent `active`, re-held to `validate_team` (its serialized form). Rejected: `template_from_team` (drops connectors, design says so).
- Pinned skills: the team file's pins are carried, so their folders are copied too (`.farik/skills/<name>/` for the team's pins, `.farik/agents/<id>/skills/<name>/` for each kept agent's), or a pin would be `Missing`. Their confirmations are not carried: confirmations are `skill.*` events of a log (ADR 0034) and the new log is fresh, so each pinned skill shows "Review" until the user confirms it on the agent page. The design names "pinned skills" among the carried and is silent on their folders; this is the plan's reading, flagged in the report.
- Pictures: the whole `.farik/team/avatars/` folder is copied (a retired agent's picture comes along, unused). Criteria are not carried: the new repository keeps the library its own `init` seeded from its scan (the founder, 2026-10-09). Rejected: carrying the old `criteria.yaml` (one repository's checks, `cargo test` say, would run in another). Settings: the old `local/settings.json` is written; the wizard's `no_sandbox: true` still wins after it, as today.
- Events: the carried team is recorded as `team.updated` (`updated_by: human`, `template: None`, `plan_in_sprints: Some(team.plans_in_sprints())` as runtime's `team_updated` sets it) after `init`'s own; no new event kind. No `team.paused`, no `setup-pending`.
- Connector keys: `farik_runtime::connectors::copy_keys` copies, for each agent of the carried team and each of its `mcp_servers` names, the entry under the old project's id to the new one's, the whole `ConnectorEntry` (OAuth included); and the mailbox entry `mailbox:<id>:procurement` when the carried team has a Procurement Specialist. An entry the store cannot read or write is skipped and not counted; the agent page then shows it as not connected. No state folder: nothing is copied.
- The mailbox moves whole (decided 2026-10-09, from the founder's "copy the keys by default, notify the user, offer different keys"; the readiness review found that the password alone leaves a mailbox the new project shows as not connected). When `copy_keys` copied the mailbox's password (the carried team has a Procurement Specialist and the old project has the entry), `carry` also copies `.farik/local/procurement/mail/mailbox.json` and `mail/ledger.json` from the old project with `farik_runtime::procurement::carry_mailbox` (the folder made owner-only and refused if any part of it is a link, as `mail_dir` does; each file written with `write_private`), and records `mailbox.connected` in the new log with the body `farik_runtime::procurement::mailbox_connected` builds, which `connect_mailbox` is changed to build its own with (no new event kind). The event is appended through `Project::event(body, now, None)`, whose envelope names no agent and no session, so `seller_mail` reads it as Farik's own, as it reads `connect_mailbox`'s. The copied ledger keeps the old position, so mail already read in the old project is not read again. The mailbox counts in `keys-copied.json` as before (`agent_id` `""`, `server` `"procurement"`); choosing different keys for it is the existing connect-mailbox flow. Old project without `mailbox.json`: the password is still copied and counted, and no file or event is written. Rejected: (b) leaving the mailbox to be connected again (the founder asked that keys be copied by default).
- On replace, a key the target folder already had under its own project id for an agent and server the carry does not overwrite stays there (`.farik/` is deleted, the keychain is not). Accepted: deleting keys is outside the design, and a key the carried team does not name is never loaded.
- `.farik/local/keys-copied.json` is `{ "from": "<old root>", "keys": [{ "agent_id", "server" }] }`, mirroring `SecretAt` (the mailbox's `agent_id` is `""`, `server` `"procurement"`), written only when at least one key was copied; runtime owns its writer and reader. `serve.status` `keys_copied` is `{ from, count }` or `null`, read only when the daemon has a project (as `setup_pending` is: in setup mode `web.project_root` is empty and would resolve against the working folder); `keys_copied.dismiss` removes the file, answers `{}` also when it is absent, and records no event (local UI state, like saving a template).
- Settings, once `project.leave` answers, calls `useConnection().reopen()`, as `SetupProject` does after a take-on, so the socket's close is a reopening and not "lost"; the reconnected `serve.status` (`project_root: null`, `leaving` set) sends `Shell` to `/setup/project` through `landing`. A leave while a session runs waits, like `farik stop`, for that tick to end; the page shows the reopening state meanwhile, and past `REOPEN_FOR_MS` the connection's ordinary retry reconnects it.
- The copied-keys notice lives in `Shell`, under the paused banner's place, so it shows on every page until chosen (design: "stays until one is chosen"). **Choose different keys** dismisses and goes to `/team`.
- `ServeStatus` gains optional `leaving` and `keysCopied` in TypeScript, as `setupPending` is optional, so existing fixtures stay valid; the wire makes both required and nullable.
- Button words the design leaves open: the Settings confirmation's buttons are "Change project" and "Cancel"; the `has_team` confirmation's are "Replace it" and "Choose another". The wizard's Back is hidden while leaving (Stay on is the way back).
- A take-on that fails after `init` ran leaves the target with a `.farik/`; choosing it again meets the `has_team` confirmation. Accepted: the old project is untouched (design).

## File map

```
crates/core/src/team.rs                         modifies: carried_team (Task 1)
crates/core/src/branch.rs                       modifies: task_number_of_branch (Task 2)
crates/store/src/git.rs, requests.rs            modifies: task_branch_numbers, the id floor (Task 2)
docs/schemas/rpc.schema.json                    modifies: project.leave, keys_copied.dismiss, replace, serve.status (Task 3)
crates/protocol/src/rpc.rs                      tests:    the new frames (Task 3)
crates/runtime/src/connectors.rs                modifies: copy_keys, KEYS_COPIED, write_keys_copied, keys_copied (Task 4)
crates/runtime/src/procurement.rs               modifies: carry_mailbox, mailbox_connected (Task 4)
crates/runtime/src/daemon/web.rs                modifies: WebState.leaving, serve.status (Task 5); open's replace (Task 7)
crates/runtime/src/daemon.rs                    modifies: DaemonState::left (Task 5)
crates/runtime/src/daemon/team.rs               modifies: project.leave, keys_copied.dismiss (Task 5)
crates/runtime/src/daemon/setup.rs              modifies: SetupHost::open gains replace (Task 7)
crates/cli/src/start.rs                         modifies: WebState literal gains leaving (Task 5)
crates/cli/src/serve.rs                         modifies: Mode::Setup { waiting, leaving }, Driven (Task 6); secrets to CliHost (Task 7)
crates/cli/src/setup.rs                         modifies: CliHost.leaving, Stay on (Task 6); has_team, replace, carry call, tests (Task 7)
crates/cli/src/carry.rs, crates/cli/src/lib.rs  creates/modifies: carry (Task 7)
crates/cli/tests/serving.rs                     tests:    leave, stay on (Task 6); change project end to end (Task 7)
packages/protocol-client/src/client.ts          modifies: MethodName gains two names (Task 8)
apps/web/src/app/store.ts, landing.ts, landing.test.ts   modifies/tests (Task 8)
apps/web/src/shell/Shell.tsx, Shell.module.css, Shell.test.tsx  modifies/tests (Task 8)
apps/web/src/pages/Settings.tsx, pages.test.tsx modifies/tests (Task 9)
apps/web/src/pages/setup/SetupProject.tsx, setup.test.tsx  modifies/tests (Task 9)
apps/web/src/strings/en.ts                      modifies: Task 8's keys, Task 9's keys
docs/SPEC.md, docs/decisions/0050-*.md, docs/decisions/0021-*.md, this plan  (Task 10)
```

## Interfaces

Consumes (all on `main`): `Team`, `AgentStatus`, `validate_team`, `ValidationError` (`farik-core`); `ConnectorSecrets`, `ConnectorEntry`, `SecretAt::of`, `SecretAt::mailbox`, `local_project_id` (`farik-runtime` connectors); `Command::RunStop`, `DaemonState`, `WebState`, `SetupHost`, `SETUP_PENDING`; `CliHost`, `init::init`, `open_project`, `Project`, `try_lock`, `state_dir`, `remember`; `ProjectFiles` `read_team`/`write_team`/`read_settings`/`write_settings`/`list_contracts`; `task_branch` (`farik-core` branch); `Git::open`, `EventLog::next_task_id_above`, `file_request` (`farik-store`); `skill_folder`, `SkillLevel` (`farik-runtime` skills); `TempRepo` (`farik_store::git::fixtures`); `useConnection().reopen`, `daemonSaid`.

Produces:

```rust
// farik-core
pub fn carried_team(team: &Team) -> Result<Team, Vec<ValidationError>>;
// farik-runtime::connectors
pub const KEYS_COPIED: &str = ".farik/local/keys-copied.json";
pub fn copy_keys(secrets: &dyn ConnectorSecrets, state: &Path, from: &Path, to: &Path, team: &Team)
    -> std::io::Result<Vec<SecretAt>>;           // each copied, under the new project's id
pub fn write_keys_copied(root: &Path, from: &Path, copied: &[SecretAt]) -> std::io::Result<()>;
pub fn keys_copied(root: &Path) -> Option<(String, usize)>;   // (from, count); None absent or unreadable
// farik-runtime::procurement
pub fn carry_mailbox(from: &Path, to: &Path) -> std::io::Result<Option<String>>; // the address, when mailbox.json was there
pub fn mailbox_connected(address: &str) -> Result<EventBody, String>;          // purpose Procurement
// farik-runtime::daemon
pub struct WebState { /* … */ pub leaving: Mutex<Option<PathBuf>> }
impl DaemonState { pub fn left(&self) -> Option<PathBuf>; }
trait SetupHost { fn open(&self, path: &str, no_sandbox: bool, replace: bool) -> Result<PathBuf, SetupError>; /* … */ }
// farik (cli), crate-private
enum Mode { Setup { waiting: Option<PathBuf>, leaving: Option<PathBuf> }, Drive(PathBuf) }
enum Driven { Ended(i32), Left(PathBuf) }
pub(crate) const HAS_TEAM: &str = "has_team: that folder already has a Farik team";
pub(crate) fn carry(from: &Path, to: &Project, secrets: &dyn ConnectorSecrets, state: Option<&Path>,
    now: DateTime<Utc>) -> Result<(), String>;
// CliHost gains: leaving: Option<PathBuf>, secrets: Arc<dyn ConnectorSecrets>
```

Wire (`rpc.schema.json`): `projectLeaveRequest` (`project.leave`, params `{}`, `additionalProperties: false`) and `keysCopiedDismissRequest` (`keys_copied.dismiss`, params `{}`), both answering `emptyResult` and listed in `rpcRequest.oneOf`; `projectOpenRequest.params.replace?: boolean`; `serveStatusResult` gains required `leaving: string | null` and `keys_copied: null | { from: string, count: integer ≥ 1 }` (`additionalProperties: false`). TypeScript: `ServeStatus.leaving?: string | null`, `ServeStatus.keysCopied?: { from: string; count: number } | null`; `MethodName` gains `"project.leave" | "keys_copied.dismiss"`.

## Tasks

### Task 1: the carried team

Files: modified `crates/core/src/team.rs` (function and `mod tests`). Produces `carried_team`.

- `carried_team_drops_retired_agents_and_activates_the_rest` — from a team of an active Product Manager, an active Developer, a paused Architect and a retired Marketing Specialist: the agent ids are the first three in their order, each `AgentStatus::Active`.
- `carried_team_keeps_everything_else` — `serde_json::to_value` of the result equals the input's with the retired agent removed and each `status` `"active"`: names, personas, avatars, models, grants, `mcp_servers`, agent and team `skills`, budgets, policy and `plan_in_sprints` all equal.

- [x] `feat(core): carry a team to another project without its retired agents`

### Task 2: task numbers past old task branches

Files: modified `crates/core/src/branch.rs` (function and `mod tests`), `crates/store/src/git.rs` (`task_branch_numbers`, `mod tests`), `crates/store/src/requests.rs` (the floor, `mod tests`); the tests that need git are `#[ignore = "needs the git program: cargo xtask check --integration"]`.
Produces:

```rust
// farik-core::branch
pub fn task_number_of_branch(name: &str) -> Option<u64>;  // "feature/FRK-7", "origin/fix/FRK-7" -> Some(7)
// farik-store::git
impl Git { pub fn task_branch_numbers(&self) -> Result<Vec<u64>, GitError>; } // refs/heads and refs/remotes
```

- `task_number_of_branch_reads_the_three_shapes` — `feature/FRK-7`, `fix/FRK-12`, `docs/FRK-3` and `origin/feature/FRK-9` answer 7, 12, 3, 9; `main`, `feature/login`, `feature/FRK-`, `feature/FRK-x`, `wip/FRK-4` and `feature/FRK-1/more` answer `None`.
- `task_number_of_branch_inverts_task_branch` — for a contract `FRK-41` of each shape `task_branch` makes (docs, fix, feature), `task_number_of_branch(&task_branch(&contract))` is `Some(41)`.
- `task_branch_numbers_lists_local_and_remote_task_branches` — a `TempRepo` with branches `feature/FRK-4`, `docs/FRK-9`, `topic` and a remote-tracking `origin/fix/FRK-11`: answers 4, 9 and 11 in any order, nothing for `topic`.
- `files_a_request_past_the_highest_old_task_branch` — a repository with no contracts and a branch `feature/FRK-7`: the first filed request is `FRK-8`; with contract `FRK-10` and branch `feature/FRK-7`, `FRK-11`.
- `files_a_request_when_git_cannot_list_branches` — a project folder that is not a git repository files `FRK-1`.

- [ ] `feat(store): number new tasks past old task branches`

### Task 3: the wire

Files: modified `docs/schemas/rpc.schema.json`; tested in `crates/protocol/src/rpc.rs` `mod tests`.

- `reads_the_change_project_requests` — `project.leave {}`, `keys_copied.dismiss {}`, `project.open { path, no_sandbox, replace: true }` and `project.open` without `replace` each pass `rpc_request_from_value`.
- `refuses_bad_change_project_requests` — `project.leave { "x": 1 }`, `keys_copied.dismiss { "x": 1 }` and `project.open` with `replace: "yes"` are each refused.

- [ ] `feat(protocol): add leaving a project and the copied keys to the wire`

### Task 4: copying connector keys

Files: modified `crates/runtime/src/connectors.rs` (functions and `mod tests`, `MemoryConnectorSecrets`, temp folders; no git), `crates/runtime/src/procurement.rs` (`carry_mailbox`, `mailbox_connected`, `connect_mailbox` builds its body with it; `mod tests`). Produces `copy_keys`, `KEYS_COPIED`, `write_keys_copied`, `keys_copied`, `carry_mailbox`, `mailbox_connected`.

- `copy_keys_copies_each_kept_agents_entries` — entries for (old, theo, github, with `oauth` set) and (old, theo, notion), theo holding both servers: `load` under the new id of each equals the old entry, `oauth` included; `load` under the old id is still `Some`; the answer is those two `SecretAt` with the new project's id.
- `copy_keys_leaves_agents_off_the_team_and_servers_with_no_entry` — an entry for (old, iris, x) with no iris on the team is not under the new id; a server of theo's with no entry is not in the answer.
- `copy_keys_copies_the_mailbox_with_a_procurement_specialist` — the old `mailbox:<old>:procurement` entry is under `mailbox:<new>:procurement` when the team has a Procurement Specialist, and not when it has none.
- `copy_keys_skips_an_entry_the_store_cannot_read` — a store whose `load` fails for (old, theo, github) alone: notion is copied and answered, github is neither.
- `carry_mailbox_copies_the_settings_and_the_ledger` — old `mail/mailbox.json` with address `buy@shop.test` and `mail/ledger.json` with `last_uid` 42: the answer is `Some("buy@shop.test")`, both files under the new root have the old bytes and mode `0o600`, and the `mail` folder mode `0o700`; with no old `mailbox.json`, the answer is `None` and the new root has no `mail` folder.
- `carry_mailbox_refuses_a_linked_mail_folder` — the new root's `.farik/local/procurement` is a symbolic link: the answer is an error and nothing is written through the link.
- `mailbox_connected_is_what_connect_mailbox_records` — `mailbox_connected("buy@shop.test")` is `EventBody::MailboxConnected` with `purpose` procurement and that address; the existing `connect_mailbox` tests still pass on the body it now builds with it.
- `keys_copied_file_only_when_something_was_copied` — `write_keys_copied(root, from, &[])` leaves no file; with two, `keys_copied(root)` is `Some((from as a string, 2))` and the file's `keys[*]` hold `agent_id` and `server`.
- `keys_copied_is_none_without_a_readable_file` — no file, and a file that is not JSON, both answer `None`.

- [ ] `feat(runtime): copy an agent's connector keys to another project`

### Task 5: leaving, in the daemon

Files: modified `crates/runtime/src/daemon/web.rs` (`WebState.leaving`, `serve_status`, existing `WebState` literals and exact `serve.status` assertions gain the fields), `daemon.rs` (`left`), `daemon/team.rs` (`METHODS` gains the two; their arms), `crates/cli/src/start.rs` (`leaving: Mutex::default()`), the test `WebState` literals in `daemon/team.rs` and `daemon/templates.rs`, and the exact `serve.status` object in `crates/cli/tests/serving.rs` `serve_status_has_no_credential_under_a_given_engine` (gains `"leaving": null, "keys_copied": null`); tests in `web.rs` `mod tests`. Consumes Tasks 3 and 4.

- `project_leave_stops_the_run_and_marks_the_project_left` — on a `TestDaemon` with a recording `set_command_handler`, as `refuses_to_stop_farik_from_the_browser` builds it: the answer is `{}`, the handler received exactly `[Command::RunStop]`, `left()` is the project root, and `serve.status` `leaving` is its string.
- `project_leave_needs_a_project` — in setup mode the answer is error `-32004`.
- `serve_status_names_the_project_being_left_in_setup` — a setup state whose `web.leaving` is `/h/old`: `leaving` `"/h/old"`, `project_root` `null`, `keys_copied` `null`.
- `serve_status_reports_copied_keys_until_dismissed` — `write_keys_copied` with three keys: `keys_copied` is `{ "from": <old>, "count": 3 }`; `keys_copied.dismiss` answers `{}`, the file is gone and `keys_copied` is `null`; a second dismiss answers `{}`.

- [ ] `feat(runtime): leave the project from the browser`

### Task 6: serve goes from drive to setup and back

Files: modified `crates/cli/src/serve.rs` (`Mode`, `Driven`, loop, `set_up` sets `web.leaving` and prints `the team left <root>: choose its next project in the browser`), `crates/cli/src/setup.rs` (`CliHost.leaving`; `open` reopens the old root as it is); tested in `crates/cli/tests/serving.rs` (`#[ignore = "needs the git program: cargo xtask check --integration"]`). Consumes Task 5.

- `leaves_the_project_for_the_wizard_on_the_same_port` — serving `a_team`: `project.leave` answers `{}` and the socket closes; serve keeps running; `serve.status` across the restart has `project_root` `null`, `leaving` the root and the same `port`; the output holds one link; the run lock is free (`try_lock` on the root succeeds); then an interrupt (`serving.interrupted()`) ends serve with 130.
- `stays_on_the_project_it_left` — with `HOME` a scratch folder that does not hold the root: after leaving, `project.open { path: <absolute root>, no_sandbox: false }` answers the root; `daemon.json` is back; `.farik/team.yaml` bytes are those before leaving; no `setup-pending` and no `keys-copied.json`; `farik stop` ends serve with 0.

- [ ] `feat(cli): switch farik serve from a project back to the wizard`

### Task 7: the carried take-on

Files: created `crates/cli/src/carry.rs`; modified `crates/cli/src/lib.rs` (`mod carry`), `setup.rs` (`HAS_TEAM`, `secrets`, `taken_on` carries while leaving, `open`'s `replace`; `mod tests`), `serve.rs` (passes `io.connector_secrets`), `crates/runtime/src/daemon/setup.rs` and `web.rs` (`open`'s `replace` from `params["replace"]`, default false; test hosts updated); tested in `setup.rs` `mod tests` (`TempRepo`, `MemoryConnectorSecrets`, a temp state folder; each `#[ignore = "needs the git program: cargo xtask check --integration"]`) and `serving.rs`. Consumes Tasks 1, 4, 6.

Order in `open` while leaving: the existing checks (home, git root, not home, lock, account), then `has_team` unless `replace`, then (replace) remove `.farik/` and `git worktree prune`, then `init`, then `carry`, then `no_sandbox`.

- `leaving_carries_the_team_to_a_fresh_folder` — old: active theo, paused ada, retired iris, a team skill pinned with its folder, an avatar file, a criterion of the user's, sandbox `none`, a memory note for theo. The target's team is `carried_team(old)`; the avatar's bytes, the skill folder's files and the sandbox setting equal the old's; the target's criteria library is the one its `init` seeded and holds no criterion of the old one's name; `read_memory(theo)` is empty; there is no `setup-pending`; the log holds no `team.paused` and its last `team.updated` names theo and ada, with no `criteria.updated` after it.
- `leaving_copies_the_kept_agents_keys_and_says_so` — old keys for (theo, github), (iris, notion) and the mailbox, with a kept Procurement Specialist: under the new id are theo's github and the mailbox, not iris's; the old still holds theo's; `keys-copied.json` has `from` the old root and two keys. The old project's `mail/mailbox.json` (address `buy@shop.test`) and `mail/ledger.json` (`last_uid` 42) are under the new root with the same bytes, `farik_store::seller_mail::seller_mail(&new.log)` has `address` `Some("buy@shop.test")`, and the new log's `mailbox.connected` names no agent and no session.
- `leaving_without_a_procurement_specialist_moves_no_mailbox` — the old project has a mailbox and its Procurement Specialist is retired: no mailbox entry under the new id, no `mail` folder under the new root, no `mailbox.connected` in the new log.
- `leaving_writes_no_keys_file_when_no_key_was_copied` — no keys kept: no `keys-copied.json`.
- `leaving_refuses_a_folder_with_a_team_unless_replaced` — target with its own team and a memory note: `open(.., replace: false)` is `Refused(HAS_TEAM)` and its `team.yaml` bytes are unchanged; `open(.., replace: true)` answers the target, its team is the carried one, its memory note is gone, and `git status --porcelain` lists nothing outside `.farik/`.
- `leaving_reopens_the_old_folder_as_it_is` — `open(<absolute old root>)` answers it; its `team.yaml` bytes and the log's event count are unchanged; no `keys-copied.json`.
- `leaving_creates_a_project_with_the_carried_team` — `create` while leaving: the team is the carried one, FRK-1 is filed, no `setup-pending`, not paused.
- `replacing_removes_a_linked_farik_folder_not_what_it_points_at` — target whose `.farik` is a symbolic link to a folder outside the target holding `team.yaml` and a file `keep`: `open(.., replace: true)` answers the target, the outside folder and its `keep` are still there, and the target's `.farik/` is a real folder with the carried team.
- `opens_a_folder_with_a_team_as_before_when_not_leaving` — `leaving: None`, target with a team, `replace: false`: answers the target, its `team.yaml` unchanged.
- `serving.rs` `changes_project_from_the_browser` — serving `a_team` under home: `project.leave`, then `project.open { path: <target's name> }`: the target is driven (its `daemon.json`), `serve.status` `project_root` is the target, `state.json` `last_project` is the target, `farik stop` ends serve with 0.

- [ ] `feat(cli): take the team to another project`

### Task 8: the rail, the landing and the copied-keys notice

Files: modified `packages/protocol-client/src/client.ts`, `apps/web/src/app/store.ts`, `landing.ts`, `landing.test.ts`, `shell/Shell.tsx`, `Shell.module.css` (tokens only), `Shell.test.tsx`, `strings/en.ts` (`keysCopied`: "Your agents' connections were copied from {from} ({count} services). Use different keys for this project?", `keysCopiedOne` the same with "1 service", `keysKeep` "Keep them", `keysChoose` "Choose different keys"). Consumes Task 3's wire.

- `landing.test.ts` `sends_a_project_being_left_to_the_project_page` — `{ projectRoot: null, takeOnError: null, leaving: "/h/old" }` lands on `/setup/project`; with `leaving: null` still `/setup/computer`.
- `Shell.test.tsx` `names_the_project_folder_above_connected` — wide, `projectRoot` `/h/work/old-repo`: the rail shows "old-repo" with `title` the full path, and it comes before the "Connected" line (`compareDocumentPosition` is `DOCUMENT_POSITION_FOLLOWING` from it to that line).
- `offers_other_keys_until_one_is_chosen` — `keysCopied { from: "/h/old-repo", count: 2 }`: a status names "old-repo" and "2 services"; Keep them sends `keys_copied.dismiss` with `{}` (held by `test/schema.ts` to `keysCopiedDismissRequest`); once `serve.status` answers `keysCopied: null` the notice is gone.
- `choosing_different_keys_goes_to_the_team_page` — Choose different keys sends `keys_copied.dismiss` and the location is `/team`.

- [ ] `feat(web): name the project on the rail and offer other keys after a move`

### Task 9: Change project and the moving wizard

Files: modified `apps/web/src/pages/Settings.tsx`, `pages/pages.test.tsx`, `pages/setup/SetupProject.tsx`, `pages/setup/setup.test.tsx`, `strings/en.ts` (`changeProject` "Change project…", `changeProjectConfirm` the design's sentence with `{name}`, `changeProjectYes` "Change project", `movingFrom` "Moving your team from {name}", `stayOn` "Stay on {name}", `replaceTeam` the design's sentence with `{target}`, `replaceYes` "Replace it", `replaceNo` "Choose another"). Consumes Task 8.

- `pages.test.tsx` `settings_changes_the_project_after_asking` — Change project… shows the confirmation naming the folder; Cancel sends nothing; confirming sends `project.leave` with `{}` once (held to `projectLeaveRequest`), and once it answers the connection's `reopen` is called once.
- `setup.test.tsx` `moving_the_team_offers_to_stay` — `serve.status { projectRoot: null, leaving: "/h/old-repo" }`: the page says "Moving your team from old-repo", has no Back button, and Stay on old-repo sends `project.open { path: "/h/old-repo", no_sandbox: false }`.
- `setup.test.tsx` `asks_before_replacing_a_team` — `project.open` answered with the message `has_team: …`: a confirmation names the chosen folder; Replace it sends `project.open` with the same `path` and `replace: true`; Choose another shows the folder browser again and sends nothing.

- [ ] `feat(web): change the project from Settings`

### Task 10: docs

Files: `docs/SPEC.md` (header: "Revision 0.80 (2026-10-09) records changing the project from the web app (standalone plan `change-project`, ADR 0050)…"; 8.4, the sentence on task ids past every committed contract: a new task's number is past every committed contract and every `feature|fix|docs/FRK-<n>` branch, local or remote; 4.1: the project page while a project is being left, Stay on, `has_team` and `replace`, the carried take-on with no setup screens; 4.4: Change project, what is carried and what is not (the criteria are not: the new repository keeps its own), connector keys copied with the refresh-token caveat, the copied-keys notice; 8.1, web UI paragraph: `project.leave`, `keys_copied.dismiss`, `serve.status` `leaving` and `keys_copied`); created `docs/decisions/0050-one-process-drives-one-project-at-a-time.md` (Context, Decision, Consequences; Status: accepted, the founder, 2026-10-09, approving `docs/design/change-project.md`; amends ADR 0021); `docs/decisions/0021-*.md` gains an "Amended 2026-10-09 by ADR 0050" line; this plan's checkboxes and Status.

- [ ] `docs: record changing the project from the web app`

## Verification

```
cargo xtask check
# expected: xtask check: ok   (Rust and the front end's pnpm check)
cargo xtask check --integration
# expected: xtask check: ok   (the git-needing tests of Tasks 2, 6 and 7 run here)
```

`/tmp/claude-0/fullcheck.sh` (CLAUDE.md) exists only in the cloud container; on this machine the two commands above are the check. Then the founder, in `farik serve`, changes from one repository to another with a connected service, sees the notice, stays on the old one once, and replaces a folder that has a team.
