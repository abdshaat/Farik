# Phase 7, step 01: Connectors per agent

Status: built; landing review pending
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 5.6, 6.7, 8.2, 8.5, 8.6; F9
Depends on: phase 6 (merged in #19), whose step 05 keeps the model credential in the keychain (`crates/runtime/src/credential.rs`, with ADR 0022's file fallback) and whose step 12 built the connector base (ADR 0026)
Readiness: fresh-session reviewer, 2026-10-01: round 1 not ready, round 2 one Blocking (R2-B1), all folded; confirmed by the controller
Mockups approved by: the founder, 2026-10-01 (AgentEdit, ConnectorAdd, ToolApproval, PhoneToolApproval on the canvas's Connectors page, version 1790878525-fbae)

## Goal

Today an agent can be given one connector, the built-in Playwright, confined to the preview, and every `external_effect` connector tool is refused. When this step is done, the user can give one agent any MCP server, started by a command (stdio) or reached at a web address (http), with that agent's own keys kept in the OS keychain, or in a private file on a computer without one. Farik lists the server's tools, the user labels each one, and the agent's task sessions get the server. A `network` tool runs. A `denied` tool is never offered. An `external_effect` tool is still refused, with a sentence that says so, until step 02 lets the human approve one call. A server runs only as it was connected on this machine: a team file changed since, by a clone, a template or a hand edit, runs nothing and sends no key. Connecting and disconnecting are `farik connect` and `farik disconnect`, the agent page's connector list and `ConnectorAdd`, and the events `connector.connected` and `connector.disconnected`. Out of scope: approving a call (step 02); signing in with OAuth (step 03); `kit.yaml`, kit connectors with their tags applied, allowances, the drift test of shipped pins, and moving Playwright into a kit (step 05).

## Decisions

The split with later steps:
- **Connecting is this step's.** `farik connect` and `farik disconnect`, their events, and the mockups of the agent page's connector list and `ConnectorAdd` move here from the kit step (now 05): a per-agent key needs a way in, and SPEC 5.6 already describes labelling. Step 05 adds a kit connector to the same commands and screens. Rejected: a separate `farik connector key set`, which step 05's `connect` would duplicate.
- **Approvals are step 02's** (founder's split, 2026-10-01). Here an `external_effect` connector tool is refused `external_effect_refused: <tool> of <server> changes something outside the sandbox, and Farik cannot ask you about one call yet, so it is refused`. It stays offered, so the agent can tell the human why it stopped.
- **OAuth is step 03's** (O1, the founder, 2026-10-01). This step takes pasted keys and tokens only.
- **The tag governs a connector call whatever the agent's tiers** (SPEC 5.6, 6.7), for user-labelled tools too. Step 05's "`network`-tagged tools running whatever the agent's tiers" is done when it starts.
- **Pinning.** For a user's server, the pin is the tool list written to `team.yaml` at `connect`. A tool the server lists later is refused `tool_not_tagged` until the user connects again. The live drift test of shipped pins stays in step 05. Playwright's Docker drift test is untouched.

The team file:
- **The shape** (extending phase 6 step 12's `{ name, source: builtin }`): `{ name, source: custom, transport: stdio | http, command, args, url, headers, credential_keys, tools }`.
  - `command` and `args` for stdio only, `url` and `headers` for http only. `mcpServer` stays one flat object, with every field optional beside `name` and `source`. `validate_team` checks the rest on the typed value, as it does its other rules, each error at its own field: `command` and `args` only with `transport: stdio`, `url` and `headers` only with `http`, `transport` required with `source: custom` and absent with `builtin`. Rejected: `oneOf`, which jsonschema reports once at the item, and `if`/`then`, which typify 0.8 does not generate (finding R2-S1).
  - `credential_keys` match `^[A-Z][A-Z0-9_]{0,63}$`, at most 8.
  - `headers` are templates that may hold `{KEY}` for a key in `credential_keys` and nothing else, so no secret is written in the file.
  - `tools` maps every listed tool to `network`, `external_effect` or `denied`. Not `tool_tiers`, as the project plan wrote, because `denied` is not a tier.
- **Names.** A custom name matches `^[a-z][a-z0-9-]{0,39}$`: no `_`, so never `__`, which the hook splits `mcp__<server>__<tool>` at, and nothing Claude Code rewrites. `farik` and a built-in's name are `connector_name_reserved`. One name twice on an agent is `connector_name_twice` (`uniqueItems` misses two `github` entries with different commands). `unknown_connector` applies to `source: builtin` only. `mcp_servers`' description is corrected to "task sessions" (it still says "explore, implement and design review").
- **Tool names.** Claude Code rewrites characters outside `[A-Za-z0-9_-]`, so `repo.delete` would match neither the hook's lookup nor `--disallowedTools`. A listed tool whose name falls outside `^[A-Za-z0-9_-]{1,64}$` is shown as "Farik can't use this tool" and not written; a call to it fails safe, `tool_not_tagged`.
- **Untagged tools.** One the user leaves unlabelled is written `external_effect` (SPEC 5.6).
- **A secret in a URL.** `url` is never a template and lands in the committed `team.yaml`. A URL with userinfo or a query is `url_holds_secret`, and `ConnectorAdd` says a service whose address holds a key is not supported yet (Zapier is one).

Transport and process:
- **Transports.** `stdio` and streamable `http`, as the project plan decided. The old SSE transport is refused by the schema.
- **Where a stdio server runs.** On the host, as the user, as `docs/design/role-kits.md` says of every MCP server and ADR 0004 puts sessions. Rejected: a container per server, whose image would need every server's runtime (Node, Python, uv). The ceiling: a user's server is code the user chose, with the user's rights. The Advanced section's copy says so.
- **Credentials reach the server through a launcher, never a file.** A session's `mcp.json` names a stdio connector as `farik connector run --daemon <daemon.json> --session <id> --server <name>`.
  - It asks the daemon's authenticated `POST /connector/launch`, which refuses a session it did not register (404 `unknown_session`) and a server that session was not given (403 `connector_not_in_session`), then reads the agent's keys and answers `{ command, args, env }` for stdio, `{ headers }` for http.
  - The launcher clears its environment, sets `PATH`, `HOME`, `LANG`, `TMPDIR` and the keys, and execs the server. So the model credential in Claude Code's environment never reaches a connector, which an inherited environment would do (`child_env`, 8.2).
  - An http connector gets Claude Code's `headersHelper`: `farik connector headers` with the same arguments, which asks the same route and prints the filled headers as a JSON object. Claude Code runs the helper through a shell, so each argument is single-quoted for POSIX `sh`, each `'` written `'\''`.
  - Rejected: an `env` or `headers` map in `mcp.json`, which is 0600 but kept on disk, while 8.6 says the key goes to the process environment from the keychain.
- **What was connected on this machine is what runs** (finding B1). `connect` keeps, beside the keys in the same keychain entry or file entry, `spec_sha256`: the sha256 of the canonical JSON of the entry's `transport`, `command`, `args`, `url`, `headers`, `credential_keys` and `tools`. A server with no keys still gets an entry. `run_session` leaves an unconfirmed custom server (its entry hashes differently, or there is none) out of `mcp.json`, out of the registration's `connectors` and out of the untrusted-content notice (finding R2-B1). The launch route refuses it again with 403 `connector_not_confirmed`, as the second check. On that refusal, the headers helper prints nothing and exits non-zero, so Claude Code fails the connection rather than connecting without a header. Claude Code runs the session without that server, and `team.get` reports it `connect_again`, which the agent page shows as "Connect again". `tools` is in the hash too, so a commit cannot retag a `denied` tool `network`. Why: `.farik/team.yaml` is committed (8.4), so a clone or a pulled branch could otherwise run `sh -c "curl … | sh"` on the host, or move a connected server's `url` and receive the user's key. Agents cannot write `.farik/` (5.3), so this is a supply-chain threat; 8.6 expects hostile repository content.
- **Listing tools.** `rmcp` as a client: features `client`, `transport-child-process`, `transport-streamable-http-client-reqwest` and `reqwest` (rustls). In rmcp 3.3.0 the plain `transport-streamable-http-client` gives only a transport generic over a client trait. This brings `reqwest` (0.13), `rustls` and, through `transport-child-process`, `process-wrap` (10) in as new transitive dependencies, recorded with their licences in the pull request (code.md line 154). Rejected: listing over the existing `hyper`, which means writing the client rmcp already has. The listing runs with the launcher's clean environment and gives up after 30 seconds.

Credentials:
- **Where keys are kept.** Per agent, in the keychain, through phase 6 step 05's `keyring` adapter. Service `farik`, account `connector:<project_id>:<agent_id>:<server>` (two projects can both have an agent `theo`). The entry is one JSON object `{ spec_sha256, keys }`. Disconnecting deletes it, and never another agent's. `credential.rs`'s `map_keyring_error` and `read_keychain` become `pub(crate)`.
- **A computer with no keychain** (O2, the founder, 2026-10-01; as ADR 0022 does for the model credential): the entry goes to `connectors.json` in the user's Farik state folder, the folder 0700 and the file 0600, written beside and renamed over, keyed by the same account string. `ConnectorAdd` and `farik connect` say which store was used ("Kept in your computer's keychain" or "Kept in a private file only you can read"). With no state folder either, connect is refused `no_secret_store`.
- **Secrets on the command line.** Each `--key` reads its value from standard input, echo off on a terminal, through `rpassword = "=7.5.4"` (one small new dependency, Apache-2.0, bringing `rtoolbox` 0.0.6, Apache-2.0, from the same author; 7.5.4 was the newest on crates.io at Task 8, 2026-10-01, over the 7.4.0 this plan first named), over a terminal echoing the secret. Never from an argument, which `ps` shows.
- **`farik connect <agent> <name> (--command <program> [--arg <a>]... | --url <url> [--header '<Name>: <template>']...) [--key <NAME>]... [--tag <tool>=network|external_effect|denied]...`** (finding B4). The command lists the tools in its own process with the keys, prints each with its tag (one not named by `--tag` is `external_effect`), and saves the entry through `ConnectorSecrets` there too, so a key never crosses a socket. The team file and `connector.connected` go through `here_or_sent` (`cli/src/start.rs`): written here, holding the run lock, when nothing drives the project, else sent as the command `connector_connect`, whose body carries names, tags and `spec_sha256`, never a value. `farik disconnect <agent> <name>` deletes the entry in its own process and sends `connector_disconnect`. The browser's RPCs carry the keys to the daemon, which keeps them, as `account.connect` does.

Which sessions get custom connectors:
- Those about a task: `refine`, `plan`, `explore`, `implement`, `verify`, and the Designer's design review.
- A session given one Farik tool gets none: triage, the judgment (5.3), and the Product Manager's design-plan decision (a `verify` session with `farik_decide_design_plan` alone).
- Neither do `ceremony`, `conversation` or `chat`, which are about no task. Step 06 decides how the Scrum Master's bridge reaches ceremonies. Playwright keeps phase 6 step 12's rule.

The governor:
- **A connector call skips the tier check** (finding S2). A call `evaluate_connector_call` passes is not given to `evaluate_tool_call`'s tier check, and the session's tiers are not widened for a custom connector: today `run_session` adds `network` whenever any connector is given (`orchestrator/session.rs`), which would also let `WebFetch` and `WebSearch` through the hook for an agent without `network`. Playwright keeps step 12's registration.
- **`preauthorized_external_tools` does not apply to connector tools** (finding B3). The hook never consults it for an `mcp__<server>__*` call. An `external_effect` connector tool is refused here, and from step 02 asks, one grant per call, until step 05's allowances. `ApprovedCall`, `ToolCallContext.approved_calls` and `RequiresHumanApproval` stay unused by connector calls.

What a non-technical user sees:
- **The agent page's connector list** shows each connected server, "Connect again" when unconfirmed, and "Remove", which asks to confirm.
- **Adding a custom connector is under Advanced**, where the canvas's newer `AgentEdit` puts it: "Farik has not checked it, so you label each of its tools yourself." `ConnectorAdd` has three steps: how to start it (command or web address, and its keys), label its tools, done. The labels are "Only reads" (`network`), "Changes things, asks you" (`external_effect`) and "Never" (`denied`); steps 01 and 02 land on one branch before any release, so the second label is true when a user sees it.
- The canvas's `ConnectorAdd` breaks two made decisions, and Task 1 removes both: "Don't ask me" (pre-authorisation, which is step 05's allowance, and only for tools that spend credits), and "They reuse this sign-in" (credentials are per agent). The kit's "Recommended" connectors are step 05's.

Security:
- **Secrets stay out** of `team.yaml`, `mcp.json`, an event, a log line, a reply, the prompt and a command body. `connector.connected` records key names only. A refused `connector.connect` or `connector.tools` drops its parameters from the error, as `account.connect` does (8.6): both join `SECRET_METHODS` in `daemon/web.rs`. The launch route's answer is never logged.
- **Deny by default.** A connector not given to the session: `connector_not_in_session`. A tool the pin does not name: `tool_not_tagged`. `denied`: refused, and passed to `--disallowedTools`. `external_effect`: `external_effect_refused`. A launch for an unregistered session, a server the session lacks, or an unconfirmed server: refused. A keychain failing at launch: the route answers 503 `secret_store_unavailable`, the launcher and the helper exit non-zero, and Claude Code runs the session without that server.
- **Untrusted output.** Everything a connector returns is untrusted (8.6). The prompt's untrusted-content notice names each connector the session was given. Wrapping each result is not done in this phase.
- **8.6's no-sandbox residuals** gain three routes to a key, beside the Claude Code process's environment: running the launcher (the token in `daemon.json` reaches the launch route), calling the route directly, and reading a connector process's `/proc/<pid>/environ`. In sandbox mode none applies: the container sees neither `daemon.json`, `mcp.json` nor the host's `/proc`.
- **Risk: `headersHelper`'s 10-second limit.** A macOS keychain prompt can take longer, which fails the connection with Claude Code's own message. Accepted; the Advanced copy says to allow Farik "Always".

Note for the engines phase (phase 8), not decided here: Claude Code hands MCP results to the model directly, so a governed gateway that proxies connectors, wraps each result and moves the governor out of the `PreToolUse` hook is a candidate for phase 8 (research notes, 2026-10-01). Nothing in the project plan, an ADR or the spec records it yet; phase 8's brainstorm decides.

ADR 0030 records the launcher, the definition hash, per-agent keys with the file fallback, and connecting in step 01. It is written in Task 2's commit, because each binds later steps.

Open, for the founder:
- **O3, the mockups.** The founder approves Task 1's boards when they are drawn; Task 9 does not start until then.

## File map

```
docs/design/mockups/{AgentEdit,ConnectorAdd}.dc.html, canvas.json   Task 1
docs/decisions/0030-connector-credentials-and-confirmation.md      creates: the ADR (Task 2)
docs/schemas/team.schema.json                                      modifies: mcpServer custom variant (Task 2)
crates/core/Cargo.toml                                             modifies: sha2, a workspace dependency (Task 2)
crates/core/src/team.rs                                            modifies: validation, spec_sha256 (Task 2)
crates/runtime/src/connectors.rs                                   creates: secrets, listing, launch spec (Tasks 3, 4)
crates/runtime/src/credential.rs                                   modifies: two helpers pub(crate) (Task 3)
Cargo.toml (workspace)                                             modifies: rmcp client features, rpassword (Tasks 4, 8)
crates/runtime/tests/fixture_mcp.rs                                creates: stdio and http fixture servers (Task 4)
crates/core/src/governor/permissions.rs                            modifies: origin optional, external_effect_refused (Task 5)
crates/runtime/src/daemon/hooks.rs                                 modifies: any session connector, no tier widening (Task 5)
crates/runtime/src/orchestrator/session.rs                         modifies: custom servers into SessionSpec (Task 5 call site; Task 6)
crates/runtime/src/claude.rs                                       modifies: launcher and headersHelper in mcp.json (Task 6)
crates/runtime/src/daemon/app.rs, daemon.rs                        modifies: POST /connector/launch (Task 6)
crates/runtime/src/prompt.rs                                       modifies: the notice names connectors (Task 6)
crates/cli/src/connector_run.rs                                    creates: farik connector run|headers (Task 6)
crates/runtime/src/daemon/team.rs                                  modifies: connector RPCs and commands (Task 7)
crates/runtime/src/daemon/web.rs                                   modifies: SECRET_METHODS gains connector.connect and connector.tools; team.get's connector states (Task 7)
docs/schemas/{event,command,rpc}.schema.json                       modifies: two events, two commands, three RPCs (Task 7)
crates/protocol/src/event.rs, command.rs                           modifies: hand-written EventKind, EventBody, Command (Task 7)
crates/cli/src/connector.rs, lib.rs, crates/cli/Cargo.toml          creates/modifies: connect, disconnect, clap (Task 8)
packages/protocol-client/src/mapping.ts                            modifies: the RPCs' camelCase mapping (Task 9)
apps/web/src/pages/{AgentEdit,ConnectorAdd}.tsx, tests             creates/modifies (Task 9)
apps/web/src/strings/en.ts                                         modifies: the copy (Task 9)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md modifies (Task 10)
```

## Interfaces

Consumes: `ConnectorTag`, `SessionConnector`, `ConnectorRefusal`, `evaluate_connector_call` (`farik-core`, main); `SessionRegistration.connectors`, `ToolContext.connectors`, `McpServerConfig`, `SessionSpec.disallowed_tools`, `write_session_files`, `child_env`, `KeychainStore`, `FileStore`'s write-and-rename (`farik-runtime`, main); `builtin_connector` (`farik-roles`, main); `here_or_sent`, `send` (`farik` cli, main).

Produces:

```rust
// farik-core
// hand-written, built from the generated `McpServer` by `custom_server(&McpServer) -> Option<CustomServer>` in team.rs, the crate's one mapping
pub enum McpServer { Builtin { name: String }, Custom(CustomServer) }
pub struct CustomServer { pub name: String, pub transport: CustomTransport,
    pub credential_keys: Vec<String>, pub tools: BTreeMap<String, ConnectorTag> }
pub enum CustomTransport { Stdio { command: String, args: Vec<String> },
    Http { url: String, headers: BTreeMap<String, String> } }
pub fn canonical_json(value: &serde_json::Value) -> String;  // compact, object keys sorted at every depth
pub fn spec_sha256(server: &CustomServer) -> String;        // sha256 hex of canonical_json of the definition
pub struct SessionConnector { pub server: String, pub origin: Option<String>,
    pub tools: BTreeMap<String, ConnectorTag> }              // origin: Some for preview-confined
pub enum ConnectorRefusal { /* existing */, ExternalEffectRefused }
// evaluate_connector_call keeps its signature
// farik-runtime
pub struct SecretAt { pub project_id: String, pub agent_id: String, pub server: String }
pub struct ConnectorEntry { pub spec_sha256: String, pub keys: BTreeMap<String, Secret> }
pub enum SecretStore { Keychain, File }
pub trait ConnectorSecrets: Send + Sync {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError>;
    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError>;
    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError>; }
pub struct KeychainConnectorSecrets; pub struct FileConnectorSecrets { /* path */ }
pub struct ConnectorSecretStores { /* keychain, then file */ }  // ConnectorSecrets: save tries the keychain
    // then the file; load looks in both, so an entry saved to the file before a keychain appeared is still found
pub struct LaunchSpec { pub command: String, pub args: Vec<String>, pub env: BTreeMap<String, Secret> }
pub fn launch_spec(server: &CustomServer, entry: &ConnectorEntry) -> Result<LaunchSpec, ConnectorError>;
pub async fn list_tools(server: &CustomServer, keys: &BTreeMap<String, Secret>)
    -> Result<Vec<ListedTool>, ConnectorError>;              // ListedTool { name, description }
```

Wire (`snake_case`): events `connector.connected { agent, server, transport, credential_keys, tools, spec_sha256 }`, `connector.disconnected { agent, server }`; commands `connector_connect { agent, server, spec_sha256 }` (`server` the entry, no values) and `connector_disconnect { agent, server }`; RPCs `connector.tools`, `connector.connect`, `connector.disconnect`; `team.get`'s result gains `connectors: [{ agent, server, state: connected | connect_again }]` beside `team`, which stays the file as it is (the daemon reads each stored `spec_sha256` from the store once per connect and disconnect and at daemon start, and compares it with the team file on each query, so a macOS keychain is not asked on each `team.get`); route `POST /connector/launch { session, server }`.

## Tasks

### Task 1: The connector screens, mocked up

Files: on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, page "Team and settings"), copied into `docs/design/mockups/`:
- `AgentEdit`: the canvas's newer version, 1560 high, which the repository lacks, gaining a connected custom server row with "Remove" and its confirmation, and a "Connect again" row.
- `ConnectorAdd`: revised to the three steps and three labels above, saying which store kept the keys, without "Don't ask me" or a shared sign-in, and with the `url_holds_secret` line.

Gate (O3): the founder approves both boards, and the approval is written into this plan's Decisions with its date. Task 9 does not start until then; Tasks 2 to 8 do not depend on the boards.

- [x] `docs(design): mock up the connector screens` (done in 4fe98dc, `docs(design): mock up connectors per agent and approving their calls`)

### Task 2: Custom servers in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, `crates/core/Cargo.toml`, ADR 0030. Produces `McpServer`, `CustomServer`, `CustomTransport`, `canonical_json`, `spec_sha256`.

- `accepts_a_custom_stdio_and_a_custom_http_server`: both shapes validate and read back equal.
- `refuses_a_custom_server_named_farik_or_playwright`: each gives `connector_name_reserved` at `/agents/0/mcp_servers/0/name`.
- `refuses_a_server_name_with_an_underscore`: `my_server` is a schema error at that `name`.
- `refuses_one_name_twice`: two `github` entries with different commands give `connector_name_twice` at the second.
- `refuses_url_on_stdio_and_command_on_http`: each is a schema error at its field.
- `refuses_a_url_holding_a_secret`: `https://u:p@x.example/mcp` and `https://x.example/mcp?key=v` each give `url_holds_secret`.
- `refuses_a_header_naming_an_undeclared_key`: `{TOKEN}` with `credential_keys: [API_KEY]` gives `header_key_unknown`.
- `refuses_a_tool_tagged_read`: a schema error at `/agents/0/mcp_servers/0/tools/x`.
- `keeps_builtin_entries_as_they_were`: step 12's fixture still validates, unchanged.
- `keeps_the_source_error_at_its_field`: `source: npm` is still a schema error at `/agents/1/mcp_servers/1/source` (the existing case in `team.rs`).
- `spec_hash_ignores_key_order_and_sees_every_field`: `{a:1,b:2}` and `{b:2,a:1}` give one `canonical_json`; changing `url`, `command`, one arg, one header, one key name or one tag each changes `spec_sha256`.

- [x] `feat(core): describe custom MCP servers in the team file`

### Task 3: Per-agent keys, in the keychain or a private file

Files: `connectors.rs`, `credential.rs` (`map_keyring_error`, `read_keychain` to `pub(crate)`). Produces `ConnectorSecrets`, `SecretAt`, `ConnectorEntry`, `SecretStore`, `KeychainConnectorSecrets`, `FileConnectorSecrets`, `ConnectorSecretStores`, an in-memory one for tests.

- `keeps_keys_under_the_project_agent_and_server`: the account is `connector:p:theo:github`, the service `farik`, and the entry reads back with its `spec_sha256`.
- `deleting_one_agents_keys_leaves_anothers`: after deleting theo's, iris's same server still loads.
- `maps_no_keychain_to_no_keychain`: `NoDefaultStore` gives `CredentialError::NoKeychain`, as `credential.rs` does.
- `falls_back_to_the_private_file_without_a_keychain`: with the keychain answering `NoKeychain`, `save` answers `SecretStore::File` and the entry loads back equal.
- `connectors_json_is_owner_only`: the file is 0600 and its folder 0700 after a save.
- `refuses_with_no_store_at_all`: no keychain and no state folder give `no_secret_store`.
- `a_secret_never_prints`: the `Debug` form of `ConnectorEntry` shows key names and `***`.

- [x] `feat(runtime): keep each agent's connector keys in the keychain or a private file`

### Task 4: Listing a server's tools

Files: `connectors.rs`, `crates/runtime/tests/fixture_mcp.rs`, the workspace `Cargo.toml` (rmcp features). Produces `list_tools`, `launch_spec`.

- `lists_a_stdio_servers_tools`: against the fixture, the list is its three tools by name.
- `the_server_sees_its_keys_and_not_the_model_key`: the fixture's `env` tool reports `API_KEY` set and neither `ANTHROPIC_API_KEY` nor `CLAUDE_CODE_OAUTH_TOKEN`, both set in the test's own environment.
- `fills_http_headers_from_keys`: `Bearer {API_KEY}` becomes `Bearer k`, and the http fixture sees it.
- `marks_a_tool_name_claude_code_would_rewrite`: a fixture tool `repo.delete` is listed as unusable and never offered for a tag.
- `a_launch_spec_never_prints`: `LaunchSpec`'s `Debug` shows key names and `***`.
- `gives_up_after_thirty_seconds`: a fixture that never answers gives `ConnectorError::Timeout` (paused clock).

- [x] `feat(runtime): list an MCP server's tools with the agent's keys`

### Task 5: The governor's connector check

Files: `permissions.rs`, `daemon/hooks.rs`, and as call sites only `orchestrator/session.rs` (`give_browser`). Produces `SessionConnector.origin: Option<String>`, `ConnectorRefusal::ExternalEffectRefused`.

- `a_network_tool_runs_with_no_origin`: `origin: None` runs any `url`.
- `a_preview_connector_still_checks_urls`: step 12's `url_outside_preview` cases are unchanged.
- `external_effect_is_refused_with_a_sentence`: `ExternalEffectRefused`, and the hook's reason starts `external_effect_refused:` and names the tool and server.
- `a_preauthorized_external_tool_is_still_refused`: an agent listing `mcp__github__create_issue` in `preauthorized_external_tools` is refused it.
- `the_hook_judges_any_connector_the_session_has`: a custom server's `network` tool is allowed for an agent without the `network` tier.
- `a_custom_connector_does_not_let_webfetch_through`: that agent's `WebFetch` is refused `tool_not_allowed` or `tier_not_granted`.
- `an_unknown_mcp_server_is_still_not_in_session`: `connector_not_in_session`.

- [x] `feat(core): judge any connector by its tag, and refuse external_effect for now`

### Task 6: Custom connectors in sessions, through the launcher

Files: `orchestrator/session.rs`, `claude.rs`, `daemon/app.rs`, `daemon.rs`, `prompt.rs`, `cli/src/connector_run.rs`.

- `gives_custom_servers_to_task_sessions_only`: present for refine, plan, explore, implement and verify; absent for triage, the judgment, the design-plan decision, ceremony, conversation and chat.
- `mcp_json_holds_no_secret`: a stdio entry is the launcher command, an http entry has `headersHelper`, and no key's value appears in the file.
- `headers_helper_quotes_a_path_with_a_space`: a daemon path `/tmp/a b/it's/daemon.json` round-trips through `sh -c` as one argument.
- `denied_tools_join_disallowed_tools`: `mcp__github__delete_repo` follows `Bash`.
- `launch_refuses_an_unregistered_session_or_server`: 404 `unknown_session`, and 403 `connector_not_in_session`.
- `launch_refuses_a_server_changed_since_connect`: after the entry's `url`, and separately its `command`, is changed in `team.yaml`, launch answers 403 `connector_not_confirmed`, and the headers helper prints nothing and exits non-zero.
- `an_unconfirmed_server_is_left_out_of_the_session`: a custom http server in `team.yaml` with no entry, and one whose `url` changed since connect, are absent from `mcp.json` and from the registration's connectors, and `mcp__<server>__<tool>` is denied `connector_not_in_session`.
- `a_custom_connector_adds_no_tier`: `run_session` for an agent without `network`, given a custom connector, registers its tiers unchanged; given Playwright, `network` is added as in step 12.
- `launch_answers_503_when_the_store_fails`: with an in-memory store answering an error, launch answers 503 `secret_store_unavailable`.
- `connector_run_execs_with_a_clean_environment`: against the fixture, only `PATH`, `HOME`, `LANG`, `TMPDIR` and the keys.
- `the_notice_names_the_connectors`: the untrusted-content section names `github` when given.
- `a_live_session_calls_a_custom_connector` (integration, `--integration`): Claude Code calls the fixture's `network` tool through the launcher; the stream's `system/init` line's `tools` list holds `mcp__fixture__<network tool>` and not `mcp__fixture__<denied tool>`; and the log has no `tool.called` for the denied tool.

- [x] `feat(runtime): load each agent's connectors into its sessions`

### Task 7: Connect and disconnect in the daemon

Files: `daemon/team.rs`, `daemon/web.rs`, the event, command and RPC schemas, `protocol/src/event.rs`, `command.rs`.

- `connect_lists_tags_saves_and_records`: `connector.connect` keeps the entry, writes the team file, and records `connector.connected` with key names and `spec_sha256` only.
- `an_unlabelled_tool_is_written_external_effect`: asserts the written entry's `tools.<name>` is `external_effect`.
- `a_refused_connect_echoes_no_secret`: the error's text has no key value; for `connector.connect` and `connector.tools` alike, a frame the schema refuses has no `error.data`.
- `a_connect_command_body_holds_no_secret`: `connector_connect` validates without any value field, and the daemon records it unchanged.
- `disconnect_removes_entry_and_keys_and_records`: another agent's same server is untouched.
- `team_get_says_connect_again_for_an_unconfirmed_server`: a hand-edited `url` gives `state: connect_again` in `connectors`, and `team` still validates against `team.schema.json`.

- [x] `feat(runtime): connect and disconnect an agent's MCP server`

### Task 8: The command line

Files: `cli/src/connector.rs`, `cli/src/lib.rs`, `crates/cli/Cargo.toml`, the workspace `Cargo.toml` (`rpassword`).

- `farik_connect_reads_keys_from_stdin`: `--key API_KEY` reads its line from stdin, and `--key API_KEY=v` is refused.
- `farik_connect_labels_with_tag_flags_and_defaults_to_external_effect`: `--tag search=network` gives `network`, every other listed tool `external_effect`, and the printed list says so.
- `farik_connect_says_which_store_kept_the_keys`: with the in-memory keychain answering `NoKeychain`, the last line is "Kept in a private file only you can read".
- `farik_connect_sends_names_when_something_drives`: with a daemon running, the command sent is `connector_connect` and holds no key value.

- [x] `feat(cli): connect and disconnect an MCP server for one agent`

### Task 9: The screens

Files: `apps/web/src/pages/AgentEdit.tsx`, `ConnectorAdd.tsx`, their tests, `strings/en.ts`, `packages/protocol-client/src/mapping.ts`. Built from Task 1's approved boards.

- `agent_edit_lists_connectors_and_removes_after_confirming`.
- `agent_edit_shows_connect_again`: a `connect_again` server shows the button, which opens `ConnectorAdd` filled in.
- `connector_add_offers_three_labels_and_defaults_to_asks`.
- `connector_add_clears_the_key_field_after_sending`.
- `connector_add_says_which_store_kept_the_keys`.

- [x] `feat(web): connectors on the agent page`

### Task 10: Spec and plan

`docs/SPEC.md`: 5.6 (custom servers, labels, `external_effect_refused` until approvals), 6.7 (connect, per-agent keys, the file fallback), 8.2 (the launcher in `mcp.json`), 8.5 (the two events), 8.6 (the launcher, the clean environment, `connector_not_confirmed`, and the three no-sandbox routes to a key). `docs/plans/project-plan.md`: phase 7's row 01 and the `mcp_servers` decision bullet (`credential_keys`, `tools`, `source: custom`), already amended by the readiness commit, corrected if execution changed them. `docs/design/role-kits.md`: its steps table, likewise.

- [x] `docs(spec): record connectors per agent`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_calls_a_custom_connector passed
```

As built (fix wave A, 2026-10-01, the landing review's runtime, core and CLI findings; wave B does the pages):
- C1: a stdio server runs in `.farik/local/connectors/<agent>/<server>` (0700, `connectors::working_folder`), never the worktree: `list_tools` takes the folder, the launch route answers it as `cwd`, and the launcher changes to it and refuses an answer without one. `validate_team` refuses a `command` holding a `/` that is not absolute (`command_not_absolute`). Tests: the fixture's `env` tool reports `PWD` (`the_server_sees_its_keys_and_not_the_model_key`), `connector_run_starts_the_server_in_a_folder_farik_keeps` (a `whereami` server, the `pwd` program, started from the repository), `launch_answers_a_confirmed_servers_command_keys_and_headers`, `refuses_a_relative_command_with_a_folder`.
- I1: keys are kept under `local_project_id`, 32 random hex digits in `.farik/local/project_id` (0600, written beside and hard-linked into place, so two processes agree); `SecretAt::of(root, agent, server)` is the one way every caller builds an address. Keys kept under the folder's name are not moved: the step is not released. Test: `two_projects_in_folders_of_one_name_keep_their_keys_apart`. M15: the CLI tests' `kept_at` reads the daemon's address and asserts it is neither of the log's ids; keeping keys under `team_id` now fails four of them.
- I2: the launch route takes a server it refuses from the session's registration, so the hook denies its calls `connector_not_in_session`; `Kept::runs` gives a server at session setup, and shows it `connected`, only when the kept hash is the team file's and every key it names is kept. Tests: `a_refused_launch_takes_the_server_from_the_session`, `launch_answers_503_when_the_store_fails` (the hook's denial after the 503), `an_unconfirmed_server_is_left_out_of_the_session` (an `asana` kept without its key). SPEC 8.2 and ADR 0030 no longer say Claude Code runs the session without the server.
- I4: `header_holds_secret` for a header named `Authorization`, or holding `key`, `token`, `secret` or `auth`, with no `{`. Test: `refuses_a_header_holding_a_secret_itself`.
- I6: the CLI's no-sandbox warning says the token also gets a connector's keys, and names `/proc/<pid>/environ` (`warns_on_every_start_in_no_sandbox_mode`). The setup screen's copy is wave B's.
- Minors: `labelled` refuses `tag_unknown_tool`, naming the usable tools (`a_label_for_a_tool_not_listed_is_refused`, and the CLI's `--tag delete_rep=denied`); retiring an agent deletes its keys (`retiring_an_agent_deletes_its_connector_keys`); `team.get`'s connectors carry `stored_in` whenever an entry is kept, through `ConnectorSecrets::locate` (`team_get_says_where_the_keys_are_kept`); `list_tools` no longer repeats a server's own MCP error (`a_servers_own_error_text_is_not_repeated`). Pinned with no behaviour change: unnamed keys dropped (T2), `connector_disconnect` refusing a built-in (H3), one-pass header filling (R8, `fills_a_header_in_one_pass`), and a server given up on being killed (R2, `gives_up_after_thirty_seconds`; rmcp 3.3.0 kills the child on drop itself, so tokio's `kill_on_drop(false)` alone is an equivalent mutation, and the test pins the outcome).
- Not done here: "Remove" of an agent from the team still leaves its keys (only retirement deletes them); the Advanced copy saying relative `args` resolve in the server's folder, the setup screen's warning, and the pages' use of `stored_in` and the new refusal codes are wave B's.

As built (fix wave B, 2026-10-01, the landing review's page findings and the one runtime item wave A left):
- Removing an agent, by `team.save` or by a saved team applied, deletes its connector keys, as retiring does: `forget_removed_agents_keys` compares the team before and after the write. Tests: `removing_an_agent_deletes_its_connector_keys`, `applying_deletes_a_removed_agents_connector_keys`.
- W2: `connector_add_sends_an_untouched_tool_as_asks` asserts the `tags` `connector.connect` sends for tools the user never labelled; the default sent as `network` now fails it.
