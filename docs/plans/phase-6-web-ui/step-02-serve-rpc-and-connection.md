# Phase 6, step 02: Serve, RPC, and connection

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3 (`farik serve`, `farik pause`, `farik resume`), 8.1, 8.2 ("Driving the team"), 8.5 (`team.paused`, `team.resumed`), 8.6 (the web UI); F6
Depends on: phase 5 and earlier (merged); step 01 of this phase (done, 77a16c7)
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A user can run `farik serve` in a project and leave it running. It drives the team like `farik run` but never exits when the board is idle. It listens on `127.0.0.1:7420` (or the next free port), prints a one-time link, and remembers the project. The team can be paused and resumed from any terminal, and the pause holds across a restart. A browser that trades the link's code for a session can open a WebSocket at `/rpc` and use JSON-RPC 2.0 over it: stream the event log, send the command-line's commands, and read the board, a task, the team, and the server's status. A foreign `Origin` or `Host`, or a missing or expired session, is refused. `@farik/protocol-client` is the TypeScript side of that wire.

Out of scope, and the step that owns each:
- the page at `/connect`, the embedded web app, and opening the browser (step 04, which is the first step with an app to serve and open);
- `farik serve` outside a project, and choosing one in the browser (step 05);
- every other query (the steps whose screens need them).

## Decisions

- **Pause** (ADR 0021).
  - Schema: two commands, `team_pause` and `team_resume`, both with `emptyBody`. Two events, `team.paused { by: "human" }` and `team.resumed { by: "human" }`, each attributed to `human` and about no contract; `EVERY_KIND` becomes `[EventKind; 43]`.
  - Handling: `human::handle` records them, and refuses with `Refused` "the team is already paused" or "the team is not paused" when the command would change nothing.
  - Read-back: `farik_runtime::pause::paused(log: &EventLog) -> Result<bool, StoreError>` is true when the log's last `team.paused` or `team.resumed` is a `team.paused`.
  - Effect on ticks: while paused, `Orchestrator::tick_within` starts no session and answers `TickReport::Idle { why: "the team is paused; farik resume starts it again", until: None }`. It still records nothing else. Rules that start no session (the aged-escalation line, a finished sprint's end) also wait, so a pause means nothing moves. Running sessions are not stopped (the first Ctrl-C's behaviour, spec 3).
  - Command line: `farik pause` and `farik resume` are sent like `farik sprint end` (`start::command`), so they reach a running `serve` or `run`.
  - Rejected: pausing each agent (`agent_update`), which would lose each agent's own paused status on resume.
- **`farik serve [--port <n>]`** is a new driving process in `crates/cli/src/serve.rs`.
  - It starts exactly as `run` does (`start`, `start_holding`, the credential and sandbox lines), with the rules `TickRules::All`, and refuses outside a project with `run`'s own sentence. `--no-open` is not added here; step 04 adds opening the browser together with the flag.
  - Its loop is `run::ticks` with one difference: on `Idle { until: None }` it does not return. It waits with `Orchestrator::wait_until(now + 24 h)`, which a command, a stop, or the 60 s recheck already wake (orchestrator.rs:476), and then ticks again. So a request filed from another process is picked up within a minute, and the standup's UTC day turns over on a recheck. The idle line prints once per change of `why`, as `ticks` does for waits. `ticks` gains a parameter `idle: OnIdle { Return, Wait }` rather than being copied.
  - Ctrl-C and `farik stop` end it as they end `run` (exit 130 after Ctrl-C, 0 after `farik stop`), with `finish` shutting the daemon down.
- **Port** (spec 8.1): `DaemonConfig::port` becomes `port: PortChoice`, with `enum PortChoice { Any, Preferred(u16) }`.
  - `Preferred(p)` tries `p` to `p + 9` in order and then takes `Any`. `serve` passes `Preferred(7420)`, or `Preferred(n)` for `--port n`.
  - `run`, `plan`, and `contract new` pass `Any`, as today.
  - A port in use is skipped. Any other bind error fails.
- **The state folder** holds what outlives a project: `farik_cli::state::state_dir(env: &BTreeMap<String, String>) -> Option<PathBuf>`. It is `$XDG_CONFIG_HOME/farik`, else `$HOME/.config/farik`, else `%APPDATA%\farik`, and `None` when none is set (then nothing is remembered, and `serve` says so on stderr).
  - `state.json` is `{ "last_project": "<absolute root>" }`, written by `serve` at start, mode 0600, through `write_private` (it is exported from `farik-runtime` if it is not already).
  - The `dirs` crate is rejected because the env is injected (CliIo) and three variables cover the platforms.
- **The one-time code**, in `farik_runtime::daemon::web::ConnectCodes`, holds one live code at a time.
  - `issue()` replaces any earlier code with 32 random bytes, hex-encoded (the existing `random_token`, made `pub(crate)`).
  - `redeem(code) -> bool` compares in constant time (`same_token`) and consumes the code on success.
  - Codes live in memory, so a restart invalidates the last one.
  - `serve` prints `open http://127.0.0.1:<port>/connect#<code> in your browser` once the daemon is up.
- **Browser sessions** are `BrowserSessions`, kept in `<state_dir>/browser-sessions.json` (mode 0600) as `{ "sessions": [{ "hash", "created_at", "expires_at" }] }`.
  - `hash` is the lowercase hex SHA-256 of the session secret (`sha2` `=0.11.0`, a new workspace dependency). The secret is 32 random bytes, hex. The file never holds a secret.
  - `issue(now) -> String` returns the secret and records its hash with `expires_at = now + 30 days`, dropping expired entries as it writes.
  - `verify(secret, now) -> bool` is true for a stored, unexpired hash.
  - `revoke(secret)` drops it.
  - With no state folder, sessions are kept in memory for the process's life.
- **Routes.** `router` gains a sub-router for the browser that does not require the bearer token: `POST /connect` and `GET /rpc`. The bearer layer stays on every other route.
  - `POST /connect` takes `{ "code": "<hex>" }`. A redeemed code answers 204 with `Set-Cookie: farik_session=<secret>; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000`. A wrong or used code answers 401 with `{ "error": "this link has been used or is out of date; start farik serve again for a new one" }`.
  - Both routes first check `Origin` and `Host`. `Host` must equal `127.0.0.1:<port>`, and `Origin` must equal `http://127.0.0.1:<port>`; otherwise the answer is 403 with no body. `localhost` is refused on purpose, because the cookie and the link are bound to `127.0.0.1` (spec 8.6).
  - `GET /rpc` additionally needs a `farik_session` cookie that `verify` accepts (else 401), then upgrades to a WebSocket (axum's `ws` feature, added to the workspace's axum features).
  - The cookie header is parsed by hand (split on `;`, trim, `name=value`), because one cookie needs no library.
- **The wire** is `docs/schemas/rpc.schema.json` (JSON Schema 2020-12), owned by `farik-protocol` and generated with `typify::import_types!` like the others (ADR 0009). Messages are JSON-RPC 2.0 text frames.
  - Requests: `{ jsonrpc: "2.0", id: integer, method, params }`.
  - Responses: `{ jsonrpc, id, result }` or `{ jsonrpc, id, error: { code, message, data? } }`.
  - The server's notifications: `{ jsonrpc, method: "event", params: { event } }`, where `event` is the event wire of `event.schema.json`, referenced by `$ref` to that schema's file.
  - Methods:
    - `subscribe { from_seq: integer ≥ 0 }` answers `{}` and then streams every event with `seq > from_seq`, oldest first, then each new one. A second `subscribe` replaces the first.
    - `unsubscribe {}` answers `{}`.
    - `command { command }`: the command wire of `command.schema.json`, handled by the same `CommandHandler` as `POST /command`. It answers `$defs/commandReply`.
    - `query { name, params }`: the names are listed below.
  - Error codes: JSON-RPC's `-32700` (parse), `-32600` (invalid request), `-32601` (unknown method), and `-32602` (invalid params), plus `-32001` `unknown_query`.
- **The event stream** re-reads the log every 500 ms with `EventQuery { after_seq: last, .. }`, because appends come from other processes too (the `RECHECK` reason, orchestrator.rs:249). The in-process `EventLog::subscribe` channel is rejected: it misses those appends. A `ponytail:` comment marks the poll: a cross-process notify replaces it if 500 ms ever shows. The socket's task ends when the socket closes.
- **Queries** in this step:
  - `events.since { after_seq, limit ≤ 500 }` returns `{ events }`, from `EventLog::read`.
  - `tasks.list {}` returns `{ tasks }`, from `Projections::board()`.
  - `task.get { task_id }` returns `{ task }` or error `-32002` `not_found`.
  - `team.get {}` returns `{ team }`, from `ProjectFiles::read_team`.
  - `serve.status {}` returns `{ project_root, paused, credential: "api_key" | "subscription_token", port }`.

  The query answers' shapes are `$defs` in `rpc.schema.json`, whose `TaskProjection` wire mirrors the projection's fields in `snake_case`. `DaemonState` gains `web: Option<WebState>` (codes, sessions, the project root, the credential kind, the port) so that the routes can read them. The query code is a `match` in `daemon::web::query`, not a registry, until there are enough queries to want one.
- **`@farik/protocol-client`** (`packages/protocol-client`) is the one `camelCase` mapping layer (rule 6).
  - Its types are generated by `json-schema-to-typescript` `=16.0.0` from `rpc.schema.json`, `event.schema.json`, and `command.schema.json` into `src/generated/` (gitignored), by `generate` (`node src/generate.ts`).
  - `src/mapping.ts` exports `toCamel(value: unknown): unknown` and `toSnake(value: unknown): unknown`. They convert object keys recursively and leave values alone, except that the contents of an event's `body.input`/`body.output` strings stay as they are.
  - `src/client.ts` exports `connect(url: string, socket?: SocketLike): DaemonClient`. `SocketLike` is the part of the browser `WebSocket` the client uses, so tests pass a fake.
  - `DaemonClient` has:
    - `subscribe(fromSeq, onEvent)`;
    - `command(command)`, returning `Promise<CommandReply>`;
    - `query(name, params)`, returning `Promise<unknown>`;
    - `onStatus(cb: (s: 'connecting' | 'open' | 'closed') => void)`;
    - `close()`.
  - It numbers requests from 1, matches responses by `id`, and rejects a pending promise on an error response with an `RpcError { code, message }`. Reconnecting is the web shell's (step 04).
- **Test transport:** axum's `ws` for the server; `tokio-tungstenite` as a dev-dependency of `farik-runtime`, at the exact 0.29 version `Cargo.lock` resolves for axum. Real sockets on `127.0.0.1:0`, like the existing raw-TCP daemon tests.

## File map

```
docs/schemas/command.schema.json, event.schema.json   modifies: team_pause/resume; team.paused/resumed (T1)
crates/protocol/src/{command.rs,event.rs,lib.rs}, event/fixtures.rs  modifies: the new variants, EVERY_KIND 43 (T1)
crates/store/src/projections.rs                       modifies: the exhaustive EventBody match (T1)
crates/runtime/src/pause.rs                           creates: paused() (T2)
crates/runtime/src/orchestrator.rs, orchestrator/human.rs  modifies: pause in ticks; handle the commands (T2)
crates/cli/src/lib.rs                                 modifies: pause, resume, serve subcommands (T2, T3)
crates/cli/tests/human.rs                             tests: pause and resume from the command line (T2)
crates/runtime/src/daemon.rs                          modifies: PortChoice; DaemonState.web; the browser sub-router (T3, T5)
crates/cli/src/state.rs, serve.rs, run.rs, start.rs   creates/modifies: state dir, serve loop, OnIdle, port choice (T3)
crates/cli/tests/serving.rs                           creates: serve's tests (T3)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, generated/mod.rs  creates: the wire (T4)
Cargo.toml, crates/runtime/Cargo.toml, Cargo.lock    modifies: axum ws, sha2, tokio-tungstenite dev (T5)
crates/runtime/src/daemon/web.rs                      creates: codes, sessions, /connect, /rpc, queries (T5, T6)
packages/protocol-client/**                           creates: the TypeScript client (T7)
docs/plans/project-plan.md                            modifies: step 02, 04, 05 lines for the moves above (T3)
```

## Interfaces

Consumes: `Command`, `command_from_value`, `CommandReply` (`farik-protocol`); `EventLog::{read, append}`, `EventQuery`, `Projections::{board, task}`, `ProjectFiles::read_team` (`farik-store`); `serve`, `router`, `DaemonState`, `CommandHandler`, `random_token`, `same_token`, `write_private` (`farik-runtime::daemon`); `Orchestrator::{tick_within, wait_until}`, `human::handle`; `run::ticks`, `start::{start, command}` (`farik` CLI); the root pnpm workspace and `pnpm check` (step 01).

Produces:

```rust
// farik-protocol
Command::TeamPause, Command::TeamResume;  EventBody::TeamPaused(TeamPausedBody), EventBody::TeamResumed(TeamResumedBody)
pub mod rpc { /* generated: RpcRequest, RpcResponse, RpcNotification, RpcError, QueryName, … */ pub fn rpc_request_from_value(v: &Value) -> Result<RpcRequest, Vec<String>>; }
// farik-runtime
pub fn pause::paused(log: &EventLog) -> Result<bool, StoreError>;
pub enum PortChoice { Any, Preferred(u16) }                    // DaemonConfig.port
pub struct WebState { pub codes: ConnectCodes, pub sessions: BrowserSessions, pub project_root: PathBuf, pub credential: CredentialKind, pub port: u16 }
pub struct ConnectCodes;  impl ConnectCodes { pub fn issue(&self) -> Result<String, DaemonError>; pub fn redeem(&self, code: &str) -> bool; }
pub struct BrowserSessions;  impl BrowserSessions { pub fn open(file: Option<PathBuf>) -> Result<Self, DaemonError>;
  pub fn issue(&self, now: DateTime<Utc>) -> Result<String, DaemonError>; pub fn verify(&self, secret: &str, now: DateTime<Utc>) -> bool; pub fn revoke(&self, secret: &str) -> Result<(), DaemonError>; }
DaemonState::set_web(&self, web: WebState) -> bool
// farik (CLI)
pub(crate) fn state::state_dir(env: &BTreeMap<String, String>) -> Option<PathBuf>;
pub(crate) enum OnIdle { Return, Wait }   // run::ticks gains it
pub(crate) fn serve::serve(project: &Project, port: Option<u16>, io: &mut CliIo<'_>) -> i32;
```

```ts
// @farik/protocol-client
export function connect(url: string, socket?: SocketLike): DaemonClient;
export type DaemonClient = { subscribe(fromSeq: number, onEvent: (e: FarikEvent) => void): Promise<void>;
  command(c: Command): Promise<CommandReply>; query(name: QueryName, params: object): Promise<unknown>;
  onStatus(cb: (s: 'connecting' | 'open' | 'closed') => void): void; close(): void };
export class RpcError extends Error { code: number }
export function toCamel(v: unknown): unknown;  export function toSnake(v: unknown): unknown;
```

## Tasks

### Task 1: Pause on the wire

Files: the schemas, `crates/protocol/src/{command.rs,event.rs,lib.rs}`, `event/fixtures.rs`, `crates/store/src/projections.rs`; tested in those modules' tests.
Tests:
- `reads_and_writes_team_pause_and_team_resume` asserts that both commands round-trip `command_from_value` and `command_to_value` with an empty body, and that a body with any field is refused.
- `every_kind_round_trips`, the existing test extended by `EVERY_KIND`, asserts that `team.paused` and `team.resumed` with `{ by: "human" }` round-trip. `EVERY_KIND.len() == 43`.
- `refuses_a_team_paused_by_anyone_but_the_human` asserts that `{ by: "governor" }` fails the schema.

- [ ] `feat(protocol): add team pause and resume to the wire`

### Task 2: Pausing the team

Files: created `crates/runtime/src/pause.rs`; modified `orchestrator.rs`, `orchestrator/human.rs`, `crates/cli/src/lib.rs`; tested in `pause.rs`, `orchestrator` tests (the `Harness` fixture), and `crates/cli/tests/human.rs`.
Tests:
- `a_new_team_is_not_paused` asserts that `paused` is false on an empty log.
- `the_last_of_pause_and_resume_wins` asserts that paused, resumed, paused gives true, and that paused, resumed gives false.
- `records_a_pause_the_human_asks_for` asserts that `handle(TeamPause)` appends `team.paused { by: human }`, and that a second one is `Refused` with "the team is already paused".
- `refuses_a_resume_when_not_paused` asserts `Refused` with "the team is not paused".
- `starts_no_session_while_paused` asserts that, with a ready task and an idle agent, a paused team's `tick` is `Idle { why: "the team is paused; farik resume starts it again", until: None }` and no `session.started` is appended. After `TeamResume`, the next `tick` starts the session.
- `pauses_and_resumes_from_the_command_line` asserts that `farik pause` prints "paused the team" and exits 0, then `farik resume` prints "resumed the team". A second `farik resume` exits 1 with "the team is not paused".

- [ ] `feat(runtime): pause and resume the whole team`

### Task 3: `farik serve`

Files: created `crates/cli/src/state.rs`, `serve.rs`, `crates/cli/tests/serving.rs`; modified `run.rs` (`OnIdle`), `start.rs` (`PortChoice`), `lib.rs`, `crates/runtime/src/daemon.rs` (`PortChoice`), `docs/plans/project-plan.md`.
Tests:
- `prefers_the_asked_port_then_the_next_nine` asserts that with 7420 held by a test listener, `serve` binds 7421, and that with 7420 to 7429 held it binds another port.
- `state_dir_follows_xdg_then_home_then_appdata` asserts that each variable is tried in that order and that `None` is returned with none set.
- `remembers_the_project_it_serves` asserts that after `farik serve` starts, `<state_dir>/state.json` is `{"last_project": "<root>"}` with mode 0600.
- `keeps_serving_when_the_board_is_idle` asserts that with `Engine::Given(recorded(..))` and nothing to do, `serve` prints the idle line once and is still running after a filed task gets its session. The test sends a `task.created` through the store, waits on the injected sleeper, and sees `session.started`.
- `prints_a_one_time_link` asserts that stdout has one line matching `open http://127\.0\.0\.1:\d+/connect#[0-9a-f]{64} in your browser`.
- `stops_on_farik_stop` asserts exit 0 after `farik stop` from another `run_cli`, and that `daemon.json` is gone.

The project-plan edit: step 02's Delivers loses "the embedded web app" and "the origin check for the served page". Step 04's gains the `/connect` page, the embedded app (`rust-embed`), and opening the browser with `--no-open`. Step 05's gains `farik serve` outside a project. The phase's `farik serve` decision says which step each piece lands in.

- [ ] `feat(cli): add farik serve, which keeps driving when idle`

### Task 4: The RPC schema

Files: created `docs/schemas/rpc.schema.json`, `crates/protocol/src/rpc.rs`; modified `crates/protocol/src/generated/mod.rs`, `lib.rs`.
Tests:
- `reads_each_method_request` asserts that `subscribe`, `unsubscribe`, `command`, and `query` requests each validate and deserialize.
- `refuses_a_request_without_jsonrpc_2_0` asserts that `jsonrpc: "1.0"` and a missing `id` are refused.
- `refuses_an_unknown_query_name` asserts that `query { name: "secrets.get" }` fails the schema.
- `an_event_notification_carries_an_event_wire` asserts that a notification whose `event` is `team.paused` validates, and that one whose `event` lacks `seq` does not.

- [ ] `feat(protocol): add the json-rpc schema for the browser`

### Task 5: Connect codes, browser sessions, and `/connect`

Files: created `crates/runtime/src/daemon/web.rs`; modified `daemon.rs`, `Cargo.toml`, `crates/runtime/Cargo.toml`, `Cargo.lock`.
Tests:
- `a_code_opens_once` asserts that `redeem(issued)` is true and then false, and that an earlier code is false after a new `issue`.
- `sessions_keep_only_hashes` asserts that after `issue`, the file holds 64-hex `hash` values and not the secret, with mode 0600.
- `a_session_lasts_thirty_days` asserts that `verify` is true at `now + 29 days` and false at `now + 30 days + 1 s`, and that `revoke` makes it false.
- `connect_sets_the_session_cookie` asserts that `POST /connect` with the code and the right `Origin`/`Host` answers 204 with `Set-Cookie` containing `farik_session=`, `HttpOnly`, `SameSite=Strict`, and `Max-Age=2592000`.
- `connect_refuses_a_used_code` asserts 401 with the error sentence.
- `connect_refuses_a_foreign_origin_or_host` asserts 403 for `Origin: http://evil.example`, for `Origin: http://localhost:<port>`, and for `Host: attacker.example` with the right Origin.
- `browser_routes_need_no_bearer_and_others_still_do` asserts that `/connect` works without `Authorization`, and that `/command` without it is still 401.

- [ ] `feat(runtime): trade a one-time code for a browser session`

### Task 6: `/rpc`

Files: modified `crates/runtime/src/daemon/web.rs`, `crates/cli/src/start.rs` (sets `WebState` in `serve` only).
Tests, over real sockets with `tokio-tungstenite`:
- `refuses_an_upgrade_without_a_session` asserts 401 without a cookie, 401 with an unknown or expired one, and 403 with a foreign Origin.
- `streams_events_after_the_sequence_asked` asserts that with 3 events in the log, `subscribe { from_seq: 1 }` answers `{}` and then sends seq 2 and 3, and that an event appended through another `EventLog` handle on the same file arrives within 2 s.
- `runs_a_command_like_post_command` asserts that `command { team_pause }` answers the same `commandReply` JSON that `POST /command` answers, and appends `team.paused`.
- `answers_the_queries` asserts that `tasks.list`, `task.get` (a known and an unknown id, the latter `-32002`), `team.get`, `events.since`, and `serve.status` (`paused` true after the pause) answer their schema shapes. Each answer is validated against `rpc.schema.json`.
- `answers_json_rpc_errors` asserts `-32700` for non-JSON, `-32601` for `method: "nope"`, and `-32602` for `subscribe` without `from_seq`, each with the request's `id` where it had one.

- [ ] `feat(runtime): serve json-rpc over a websocket at /rpc`

### Task 7: `@farik/protocol-client`

Files: created `packages/protocol-client/{package.json,tsconfig.json,vitest.config.ts}`, `src/generate.ts`, `src/mapping.ts`, `src/client.ts`, `src/index.ts`, `src/mapping.test.ts`, `src/client.test.ts`.
Tests:
- `maps_keys_to_camel_case_and_back` asserts that `toCamel({ task_id: 1, body: { from_seq: 2 } })` is `{ taskId: 1, body: { fromSeq: 2 } }`, that `toSnake` inverts it, and that arrays are mapped element-wise.
- `leaves_tool_input_and_output_alone` asserts that an event whose `body.input` is a JSON string with `snake_case` keys keeps that string unchanged.
- `numbers_requests_and_matches_responses` asserts that two `query` calls send ids 1 and 2, and that responses arriving in reverse order resolve the right promises.
- `rejects_with_the_error_code` asserts that an error response `{ code: -32601 }` rejects with `RpcError` whose `code` is `-32601`.
- `delivers_event_notifications_in_camel_case` asserts that after `subscribe(0, cb)`, a notification's event reaches `cb` with `recordedAt`, not `recorded_at`.
- `reports_its_status` asserts that `onStatus` sees `connecting`, then `open`, then `closed`, from the fake socket's events.

- [ ] `feat(protocol-client): add the browser's json-rpc client`

## Verification

```
cargo xtask check
# expected: every Rust test passing as before plus this step's; pnpm's summary for @farik/brand (27) and
#   @farik/protocol-client (6 tests); last line: xtask check: ok
cargo run -q -p farik -- serve     # in a project with a credential; prints the link, keeps running; Ctrl-C exits 130
```
