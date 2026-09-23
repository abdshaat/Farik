# Phase 3, step 07: Daemon service and hooks

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 8.2, 8.6, 5.6; ADR 0004, ADR 0005
Depends on: step 05 of this phase (`tool_descriptors`, `call_tool`, `ToolContext`), a start gate: Task 1 does not begin until step 05's last commit is on this branch; step 04 (`Transitions`)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the two path decisions the first found wrong; findings folded in)

## Goal

Farik's governor sits in front of every tool call a session makes. A local service, started in-process, answers Claude Code's `PreToolUse` hook with allow or deny and a reason, records the `PostToolUse` hook, serves Farik's own tools over MCP to the session that is registered for them, and refuses every permission prompt the hook did not already decide. The `farik hook` commands carry a hook's JSON from Claude Code to the service and the answer back. Out of scope: spawning Claude Code with these settings (step 08), registering and ending sessions from the lifecycle (step 11).

## Decisions

Measured on `claude` 2.1.280, 2026-09-22 (a haiku session with hooks that dumped their input, recorded under `crates/runtime/src/daemon/fixtures/`): the hook's stdin is `{ session_id, transcript_path, cwd, prompt_id, permission_mode, hook_event_name, tool_name, tool_input, tool_use_id }`, plus `tool_response` and `duration_ms` for `PostToolUse`; `session_id` is the `--session-id` Farik passed; a `PreToolUse` answer of `{ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "<r>" } }` reaches the model as the error `tool_result` `PreToolUse:<tool> hook error: <r>`, with no `permission_denied` line in the stream.

- The service is `axum` =0.8.9 on `tokio` (features `rt-multi-thread`, `net`, `macros`, `sync`, `time`), bound to `127.0.0.1` on a port the OS picks unless one is given. `serve` returns a `DaemonHandle` with the `DaemonInfo` and a `shutdown` that first cancels the MCP service's `CancellationToken` (`tokio-util`, which `rmcp` already brings; a direct dependency at its version), because Claude Code holds an SSE stream open with keep-alives that a graceful shutdown would otherwise wait on forever, then stops the server and removes `daemon.json`. A `daemon.json` left by a daemon that crashed is overwritten: `serve` is only ever called by the one `farik run` that owns the project (step 15).
- The token is 32 bytes from `/dev/urandom`, hex-encoded (Unix, as the host sandbox is; no `rand` dependency). Every route refuses a request without `Authorization: Bearer <token>` with 401.
- `.farik/local/daemon.json` is `{ "port", "token", "pid" }`, written with mode 0600 when the service is up, removed by `shutdown`. It is under `.farik/local/`, which is ignored and never committed.
- Routes: `POST /hook/pre-tool-use`, `POST /hook/post-tool-use` (Claude Code's JSON in, the answer out, the decision and its log append run under `spawn_blocking`), and `/mcp` (`route_service`), the Farik MCP server over streamable HTTP (`rmcp` =3.3.0, `default-features = false`, features `server`, `transport-streamable-http-server`), with `StreamableHttpServerConfig { json_response: true, .. }` and the stateful sessions Claude Code uses; `get_info` declares the tools capability; `list_tools` and `call_tool` are written by hand over `tool_descriptors` and `call_tool` (no macros). A session reaches `/mcp` with the token and `X-Farik-Session: <session_id>` headers (step 08 puts both in `--mcp-config`); an axum middleware checks both, puts the resolved registration in the request's extensions, and answers 403 for an unregistered session; the MCP handler reads it from the `http::request::Parts` that `rmcp` 3.3.0 inserts into each request context's extensions (read in the 3.3.0 source by the readiness review; Task 3's first test holds it).
- Sessions: `DaemonState::register_session(SessionRegistration)` and `end_session(id)`, the only way in and out; a registration holds the agent id, the task, the session's working directory (its worktree), its executor, and a tool-call count. The team is read from `.farik/team.yaml` on every decision, as the tools read it (step 05).
- `decide_pre_tool_use` refuses, in this order, with a reason starting with its kind as step 05's refusals do: an unregistered session (`unknown_session`); an agent not `active` (`agent_not_active`); the session's tool calls at `max_tool_calls` of its limits (`tool_call_limit`), checked and raised under one lock because Claude Code runs read tools in parallel, and raised only by allowed calls, which is intended: a denied call did nothing. The daemon's count is the source of truth for a session's tool calls, read by `DaemonState::tool_calls(session_id)` (step 11 fills `SessionLedger::tool_calls` from it); a tool that is neither Farik's nor a built-in with a tier (`tool_not_allowed`), which is every other MCP server's tool in this phase, where `--strict-mcp-config` (step 08) means no other server is configured; 5.6's "untagged tools default to `external_effect`" arrives with MCP servers in phase 6; then `evaluate_tool_call` with the tool's tier, its paths, the agent's tiers, the contract's `allowed_paths`, and the team's protected paths (`tier_not_granted`, `path_outside_allowed`, `path_protected`, `paths_missing`, `invalid_glob`, `requires_human_approval`).
- `builtin_tool_tier` moves here from step 08 (revision 8), because the hook is its first reader: `Read`, `Glob`, `Grep`, `LS`, and `ToolSearch` (which only lists tools, and may be how Claude Code reaches Farik's) are `Read`; `Edit`, `Write`, `MultiEdit`, `NotebookEdit` are `WriteWorkspace`; `WebFetch`, `WebSearch` are `Network`; every other built-in is `None` and denied. Step 08 builds `--disallowedTools` from it.
- A built-in's paths are `file_path` (`Read`, `Write`, `Edit`, `MultiEdit`), `notebook_path` (`NotebookEdit`), `path` (`Glob`, `Grep`, `LS`). A relative path is resolved against the registration's `cwd`; the path (its nearest existing ancestor for one that does not exist yet) and the `cwd` are canonicalised, so that a committed symlink out of the worktree is judged by where it points; one outside the `cwd` is denied (`path_outside_workspace`) for every tool, reads included, because a session's files are its worktree's and nothing else on the machine (spec 8.6); one inside is made relative to it. The worktree root itself, and a `Glob`, `Grep`, or `LS` with no `path`, contribute no path, so `evaluate_tool_call` is asked with `paths: []` (a `Read` call with none passes), because `normalise` has no answer for the root. A `Glob` whose `pattern` is absolute or has a `..` segment is denied (`path_outside_workspace`). A Farik tool (`mcp__farik__<name>`) is asked with no paths here, because `call_tool` checks again with its real ones (step 05).
- Protected files and searches: a search over a directory reads the files under it, and a protected glob (`.env`, `**/*.pem`) protects files, not their directories, so the hook alone cannot keep a `Grep` from reading one. Step 08 therefore gives Claude Code `permissions.deny` rules `Read(<glob>)` for each protected path in `--settings`, which Claude Code applies to its reading tools, `Grep` and `Glob` included; what Farik does not enforce itself here is written into `docs/SPEC.md` 5.6 as the residual it is.
- The hook's decisions are the log's record of tool use, because only the daemon sees every one: an allowed call is `tool.called { tool, tool_use_id, input }`, a denied one (the limit's included) `tool.denied { tool, tool_use_id, reason }`, and `PostToolUse` is `tool.returned { tool, tool_use_id, output, duration_ms }`; `input` and `output` are strings, the compact JSON of the hook's value, cut at the last character boundary at or before 4,096 bytes with `[cut at 4 KiB]` appended when cut. A tool that fails reaches Claude Code's `PostToolUseFailure`, not `PostToolUse`, and leaves a `tool.called` with no `tool.returned`, which is accepted. None of the three is about one contract (a conversation has no task); none has an attribution field. If appending the decision's event fails, the answer becomes a deny with reason `record_failed: <detail>`, because a tool call the log cannot record is one the governor cannot vouch for. Revision 8 put the three kinds in step 08; they are here because the hook is what emits them, and step 08 does not record them again from the stream. Envelopes carry the session, the agent, and the task.
- The permission-prompt tool is `mcp__farik__permission`, listed only for Claude Code's `--permission-prompt-tool` and not in `tool_descriptors`; it answers, as the text content of a `CallToolResult`, `{ "behavior": "deny", "message": "farik decides tool calls in its PreToolUse hook; this one was not allowed there" }` to everything, because Claude Code asks it only for a call the hook did not allow.
- `farik hook pre-tool-use --daemon <path to daemon.json>` and `farik hook post-tool-use --daemon <path>` read the hook JSON from stdin, POST it with the token over a plain HTTP/1.1 exchange on a `std::net::TcpStream` (about forty lines; no HTTP client dependency, because the peer is always this daemon on localhost), and print the answer. The daemon file comes as an argument, not by looking upward from the working directory, because a session's directory is a worktree whose own `.farik/` has no `local/`. The exchange has 10 s connect, read, and write timeouts, well under Claude Code's 60 s hook timeout, because a hook that times out does not block the call. Anything that goes wrong (no file, no daemon, a daemon that never answers, a bad answer) prints a deny with the reason and exits 0; a panic in the hook is caught and exits 2 with the reason on stderr, because exit 2 is the only non-zero code Claude Code treats as blocking. So the hook fails closed. `post-tool-use` prints nothing and exits 0 either way. `CliIo` gains `stdin: Box<dyn Read + 'a>` (`std::io::empty()` at every existing construction), and the hook arm writes its JSON and returns without going through `Report`, as `doctor` returns its own exit code.
- Cross-step contracts, written into the project plan's step 07 and 08 lines: the MCP server's name `farik` (tools `mcp__farik__<name>`), the `X-Farik-Session` header, `--daemon`, `builtin_tool_tier` and the three tool events moving here.
- `DaemonError { Bind { detail }, Io { detail } }`, hand-written `Display` (ADR 0006).
- Changed 2026-09-22 in execution (Task 1): a cut value is at most 4,096 bytes with the marker included (cut at the last character boundary at or before 4,096 bytes less the marker's length), because `records_a_tool_return_cut_at_four_kib` holds the whole `output` to 4,096 bytes. `tool_use_id` and `duration_ms` are optional in the three event bodies, as they are in `HookRequest`. A decision whose team file or contract cannot be read is a deny with reason `team_unreadable` or `contract_unreadable`, since the list above has no kind for either and the hook fails closed. The test fixture is `crates/runtime/src/daemon/fixtures.rs` (the file map had none; `code.md` puts a module's fixtures there), built on step 05's `tools::fixtures`, which is made `pub(crate)`, as is `tools::refusal`, whose reasons the hook reuses. Task 2: `axum` is taken with `default-features = false` and the features `http1`, `json`, and `tokio`, the ones the service uses; `tower` (dev) is =0.5.3, the version `axum` =0.8.9 resolves, with `util` for `oneshot`. The `daemon` module is `#[cfg(unix)]`, since its token and file mode are. Task 3: `rmcp` 3.3.0's `StreamableHttpServerConfig` is `#[non_exhaustive]` and has no `stateful_mode`; the stateful sessions Claude Code opens with `initialize` are its `legacy_session_mode`, on by default, and the config is built with `with_json_response(true)` and `with_cancellation_token`. In that mode `rmcp` answers a request as an event stream whatever `json_response` says, so the tests read either shape. The middleware puts a `CallingSession` (the registration's session, agent, task, and executor) in the request's extensions rather than the registration itself, which holds the limits the MCP handler has no use for. One test beyond the plan's, `shuts_down_while_a_session_holds_an_event_stream_open`, holds the cancellation the first decision rests on: without it, `shutdown` waits on an open `GET /mcp` stream forever. `tokio-util` is =0.7.19, the version `rmcp` 3.3.0 resolves. Task 4: a deny the hook command prints itself has the reason kind `hook_failed`; a `--daemon` path that is relative is taken from the directory the hook runs in. `docs/SPEC.md` 8.2's sentence that the hooks fail closed lands with this task, the one that makes it true.
- Changed 2026-09-22 by the landing review: a built-in's `file_path`, `notebook_path`, or `path`, and a `Glob`'s `pattern`, with a part that starts with `~` is denied (`path_outside_workspace`), because Claude Code 2.1.280 expands a leading `~` to the home directory, so `~/.ssh/id_rsa` resolved to `<worktree>/~/.ssh/id_rsa` and was allowed. Any part starting with `~` is refused, not only a leading one, which also covers `~user`; `docs/SPEC.md` 8.2 says so.

## File map

```
Cargo.toml                                   modifies: axum, rmcp, tower (dev), tokio features
crates/runtime/Cargo.toml                    modifies: the same
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: tool.called, tool.denied, tool.returned
crates/runtime/src/daemon.rs                 creates: DaemonConfig, DaemonInfo, DaemonHandle, DaemonState, SessionRegistration, DaemonError, serve
crates/runtime/src/daemon/hooks.rs           creates: HookRequest, HookDecision, decide_pre_tool_use, record_post_tool_use, builtin_tool_tier; tests
crates/runtime/src/daemon/mcp.rs             creates: the rmcp handler over tool_descriptors/call_tool, the permission tool; tests
crates/runtime/src/daemon/fixtures/*.json    creates: the three recorded hook inputs, paths rewritten to /workspace
crates/runtime/src/lib.rs                    modifies: `pub mod daemon;`
crates/cli/src/hook.rs                       creates: the two hook commands and the HTTP exchange
crates/cli/src/lib.rs                        modifies: `farik hook`
crates/cli/tests/hook.rs                     creates: the commands against a served daemon
crates/cli/Cargo.toml                        modifies: farik-runtime and tokio as dev-dependencies
docs/SPEC.md                                 modifies: 5.6 (the search residual; no other MCP server this phase), 8.2 (hooks fail closed; paths outside the worktree), 8.5 (the three kinds)
docs/plans/project-plan.md                   modifies: step 07's and step 08's interface lines
```

## Interfaces

Consumes: `tool_descriptors`, `call_tool`, `ToolContext`, `ToolDeps`, `FarikTool` (step 05); `Executor` (step 02); `evaluate_tool_call`, `PermissionTier`, `AgentGrants`, `ToolCallRequest`, `ToolCallContext` (`farik-core`); `SessionLimits`.

Produces:

```rust
pub struct DaemonConfig { pub port: Option<u16>, pub daemon_file: PathBuf }
pub struct DaemonInfo { pub port: u16, pub token: String, pub pid: u32 }   // the daemon.json shape
pub struct DaemonHandle { pub info: DaemonInfo, /* shutdown */ }  impl DaemonHandle { pub async fn shutdown(self) -> Result<(), DaemonError>; }
pub struct SessionRegistration { pub session_id: String, pub agent_id: String, pub task_id: Option<TaskId>, pub cwd: PathBuf, pub executor: Option<Arc<dyn Executor>>, pub limits: SessionLimits }
pub struct DaemonState { /* tool deps, sessions */ }
impl DaemonState { pub fn new(deps: Arc<ToolDeps>) -> DaemonState; pub fn register_session(&self, registration: SessionRegistration); pub fn end_session(&self, session_id: &str); pub fn tool_calls(&self, session_id: &str) -> Option<u32>; }
pub enum DaemonError { Bind { detail: String }, Io { detail: String } }
pub async fn serve(config: DaemonConfig, state: Arc<DaemonState>) -> Result<DaemonHandle, DaemonError>;
#[derive(Deserialize)] pub struct HookRequest { pub session_id: String, pub cwd: PathBuf, pub hook_event_name: String, pub tool_name: String, pub tool_input: Value, pub tool_use_id: Option<String>, pub tool_response: Option<Value>, pub duration_ms: Option<u64> }
pub struct HookDecision { pub allow: bool, pub reason: String }   // serialised in Claude Code's hookSpecificOutput shape
pub fn decide_pre_tool_use(request: &HookRequest, state: &DaemonState) -> HookDecision;
pub fn record_post_tool_use(request: &HookRequest, state: &DaemonState) -> Result<(), DaemonError>;
pub fn builtin_tool_tier(tool: &str) -> Option<PermissionTier>;
```

## Tasks

Tests needing git are ignored as in step 04. Each builds a `DaemonState` on a `TempRepo` with `.farik/` initialised, the team of step 05, a ready task FRK-1 assigned to `dev-a` with `allowed_paths: ["src/**"]`, and `dev-a`'s session registered with its worktree as `cwd`. A recorded fixture is loaded with `/workspace` replaced by that worktree's path and its `session_id` by the registered one.

### Task 1: the three tool events and the hook's decision

Files: the schema, `event.rs`, `event/fixtures.rs`, `daemon.rs` (`DaemonState`, `SessionRegistration`), `daemon/hooks.rs`, the fixtures, `lib.rs`, `Cargo.toml`s, `docs/SPEC.md`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers the three.
- `reads_the_hook_input_claude_code_sends` — each recorded fixture deserialises to `HookRequest` with its tool, id, and (for the post one) response.
- `allows_a_read_inside_the_worktree_and_records_it` — `Read` of `<worktree>/src/a.rs`: allowed, one `tool.called` with the session on its envelope.
- `denies_a_read_outside_the_worktree` — `Read` of `/etc/passwd`: denied, reason starts `path_outside_workspace`, one `tool.denied`.
- `denies_a_read_through_a_symlink_out_of_the_worktree` — a committed `notes -> /etc/passwd`: `Read` of `<worktree>/notes` is `path_outside_workspace`.
- `allows_a_grep_of_the_whole_worktree` — `Grep` with no `path`, and with the worktree's own path: allowed.
- `denies_a_glob_outside_the_worktree` — `Glob` with `pattern: "../**"` and with `/etc/*`: `path_outside_workspace`.
- `denies_when_the_decision_cannot_be_recorded` — with the log unable to append (a log whose file was made read-only): the answer is a deny starting `record_failed`.
- `denies_a_write_outside_the_allowed_paths` — `Write` of `<worktree>/README.md`: reason starts `path_outside_allowed`.
- `denies_bash_and_every_tool_without_a_tier` — `Bash`, `Task`, `mcp__github__create_issue`: each `tool_not_allowed`.
- `denies_a_farik_tool_the_agent_has_no_tier_for` — the Product Manager's registered session calling `mcp__farik__farik_exec`: `tier_not_granted`.
- `denies_an_unknown_session_and_a_paused_agent` — `unknown_session`; after pausing `dev-a` in the team file, `agent_not_active`.
- `denies_past_the_tool_call_limit` — with `max_tool_calls: 2`, the third allowed-looking call is `tool_call_limit`.
- `records_a_tool_return_cut_at_four_kib` — a post-tool-use with a 10,000-byte response: one `tool.returned` whose `output` is 4,096 bytes or fewer.
- `serialises_a_decision_as_claude_code_reads_it` — a deny is exactly `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"<r>"}}`, an allow the same with `"allow"` and its reason.

- [x] `feat(runtime): decide tool calls in the pre-tool-use hook`

### Task 2: the service

Files: `daemon.rs` (`serve`, `DaemonHandle`, the routes, the token, `daemon.json`)

Tests with `tower::ServiceExt::oneshot` on the router, and one on a bound port:

- `refuses_a_request_without_the_token` — each route answers 401.
- `answers_the_pre_tool_use_hook` — the recorded pre fixture for the registered session, with the token: 200 and an allow body.
- `writes_the_daemon_file_and_removes_it_on_shutdown` — after `serve`, the file holds the port and token with mode 0600; after `shutdown`, it is gone and the port refuses connections.

- [x] `feat(runtime): serve the hooks on a local port behind a token`

### Task 3: Farik's tools over MCP

Files: `daemon/mcp.rs`, `daemon.rs` (the `/mcp` route and its middleware)

JSON-RPC over the router, each request with `Host: 127.0.0.1`, `Accept: application/json, text/event-stream`, `Content-Type: application/json`, the token, and `X-Farik-Session`; `initialize` first, and every later request with the `Mcp-Session-Id` it answered and `MCP-Protocol-Version`:

- `reads_the_calling_session_from_the_request_headers` — a tool call with `X-Farik-Session` reaches the handler with that session id (the `rmcp` 3.3.0 fact the decisions rely on).
- `lists_every_farik_tool_and_the_permission_tool` — `tools/list` names the nineteen of step 05 and `permission`.
- `calls_a_tool_as_the_registered_session` — `tools/call farik_read_board` answers the board.
- `refuses_mcp_for_an_unregistered_session` — 403.
- `denies_every_permission_prompt` — `permission` answers `behavior: deny` with the fixed message.

- [x] `feat(runtime): serve farik tools and the permission tool over mcp`

### Task 4: the hook commands

Files: `crates/cli/src/hook.rs`, `crates/cli/src/lib.rs`, `crates/cli/tests/hook.rs`

- `carries_a_pre_tool_use_hook_to_the_daemon_and_back` — against a served daemon, stdin the pre fixture: stdout is the daemon's allow.
- `fails_closed_when_the_daemon_is_not_there` — `--daemon` naming a missing file, one naming a port nothing listens on, and one naming a listener that accepts and never answers (returns within 12 s): stdout is a deny whose reason says so; exit 0.
- `says_nothing_after_a_post_tool_use` — stdout empty, exit 0, and the daemon recorded `tool.returned`.

- [x] `feat(cli): add the farik hook commands`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
