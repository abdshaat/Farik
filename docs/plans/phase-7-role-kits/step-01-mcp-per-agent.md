# Phase 7, step 01: MCP per agent

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 5.6, 6.7, 8.2, 8.5, 8.6; F9
Depends on: phase 6 (merged in #19), whose step 05 keeps the model credential in the keychain (`crates/runtime/src/credential.rs`) and whose step 12 built the connector base (ADR 0026)
Readiness confirmed by: not yet

## Goal

Today an agent can be given one connector, the built-in Playwright, confined to the preview, and every `external_effect` connector tool is refused. When this step is done, the user can give one agent any MCP server, started by a command (stdio) or reached at a web address (http), with that agent's own key kept in the OS keychain. Farik lists the server's tools, the user labels each one, and the agent's sessions get the server. A `network` tool runs. An `external_effect` tool stops the session and asks the human, who allows that one call or refuses it, in the web app or with `farik tool approve` and `farik tool refuse`. A `denied` tool is never offered. Connecting and disconnecting are `farik connect` and `farik disconnect`, the agent page's connector list, and the events `connector.connected` and `connector.disconnected`. Out of scope, all in step 03: `kit.yaml`, connectors chosen from a kit with their tags already applied, allowances, `ConnectorAllowance`, the live drift test of shipped pins, and moving Playwright into a kit. Signing in with OAuth is out of scope too (open decision O1).

## Decisions

The split with step 03:
- **Connecting moves to this step.** Step 01 moves `farik connect` and `farik disconnect`, their two events, and the mockups of the agent page's connector list and `ConnectorAdd` here from step 03. A per-agent key needs a way in, and SPEC 5.6 already describes connecting a server and labelling its tools. Step 03 adds a kit connector to the same commands and screens: it is connected by name, with the kit's tags and allowances applied. Task 10 updates the project plan's rows 01 and 03 and `docs/design/role-kits.md` to match. Rejected: a separate `farik connector key set` command, which step 03's `connect` would duplicate.
- **The tag governs a connector call whatever the agent's tiers.** This holds from this step on, for user-labelled tools too (SPEC 5.6, 6.7). Step 03's line "`network`-tagged tools running whatever the agent's tiers" is therefore already done when step 03 starts, and the project plan's row says so.
- **Pinning.** For a user's server, the pin is the tool list recorded in `team.yaml` when it is connected. A tool the server lists later and the pin does not name is refused (`tool_not_tagged`), and the user reconnects to label it. The test that fails on drift against the live service applies to shipped pins and stays in step 03. Playwright's Docker drift test is untouched.

The team file:
- **The shape** (it extends step 12's `{ name, source: builtin }`):
  `{ name, source: custom, transport: stdio | http, command, args, url, headers, credential_keys, tools }`
  - `command` and `args` are for stdio only. `url` and `headers` are for http only. The schema's `oneOf` enforces this.
  - `credential_keys` are key names matching `^[A-Z][A-Z0-9_]{0,63}$`, at most 8.
  - `headers` are templates that may hold `{KEY}` for a key named in `credential_keys`, and nothing else, so no secret is ever written in the file.
  - `tools` maps every listed tool to `network`, `external_effect` or `denied`. It is not `tool_tiers`, which the project plan wrote, because `denied` is not a tier. The name matches `playwright.yaml`.
- **Names.** A custom server may not be named `farik` or the name of a built-in connector (`connector_name_reserved`).
- **Untagged tools.** One the user leaves unlabelled is written as `external_effect` (SPEC 5.6).

Transport and process:
- **Transports.** Both `stdio` and `http` (streamable HTTP), as the project plan decided. The old SSE transport is refused by the schema, since current servers serve streamable HTTP.
- **Where a stdio server runs.** On the host, as the user, as `docs/design/role-kits.md` says of every MCP server and as ADR 0004 puts sessions. It does not run in the sandbox. Rejected: a container per server, because the image would need every server's runtime (Node, Python, uv), and a remote call needs the network anyway. The ceiling: a user's server is code the user chose, running with the user's rights. The Advanced section's copy says so.
- **How credentials reach the server: through a launcher, never through a file.** A session's `mcp.json` names a stdio connector as:
  `farik connector run --daemon <daemon.json> --session <id> --server <name>`
  - It asks the daemon's authenticated route `POST /connector/launch`. The daemon refuses a session it did not register, and a server that session was not given.
  - The daemon reads the agent's keys from the keychain and answers with the command, the arguments and the environment.
  - The launcher clears its environment, sets `PATH`, `HOME`, `LANG`, `TMPDIR` and the keys, and execs the server.
  - An http connector gets Claude Code's `headersHelper`, `farik connector headers ...` with the same arguments, which prints the filled headers.
  - So no secret is written into `mcp.json`, and the model credential that Claude Code's environment holds never reaches a connector, which is what an inherited environment would do (`child_env`, 8.2).
  - Rejected: an `env` or `headers` map in `mcp.json`. The file is mode 0600 but kept on disk, and SPEC 8.6 says the key goes to the process environment from the keychain.
  - The residual: in no-sandbox mode, an agent's command can run the launcher itself. This joins the residuals 8.6 already names.
- **Listing tools.** `rmcp` as a client, with its `client`, `transport-child-process` and `transport-streamable-http-client` features turned on. That is a new feature of an existing dependency, not a new crate. The listing has a 30-second limit and is run through the same launcher environment.

Credentials:
- **Where keys are kept.** Per agent, in the keychain, through the `keyring` adapter of phase 6 step 05.
  - Service `farik`, account `connector:<project_id>:<agent_id>:<server>`.
  - The entry holds one JSON object of that connector's keys.
  - The project id is in the account, because two projects can both have an agent `theo`.
  - Disconnecting deletes the entry. Another agent's entry is never touched.
- **No keychain: see O2.**
- **Secrets on the command line.** `farik connect` reads each key's value from standard input, with echo off when it is a terminal, through `rpassword` (pinned: one small new dependency, chosen over a terminal left echoing the secret). A key is never taken from an argument, because arguments are visible in `ps`.

Approvals:
- **An `external_effect` call waits like a question (5.7).**
  - With no matching grant, the hook denies the call with `approval_needed: <server> <tool> waits for the human (approval <seq>)` and records `approval.requested { approval: seq, server, tool, input }`, the input cut as on `tool.called`. It stops the session, as a pause does.
  - The task keeps its status and shows as waiting on the human while an approval is open.
  - `tool.approve { approval, note? }` records `approval.granted`, and `tool.refuse { approval, note? }` records `approval.refused`.
  - The next session about the task is given the decision as the human's message: "You may call `<tool>` once with the input you asked for", or the refusal with its note.
  - A grant allows exactly one later call by the same agent on the same task, with the same server, tool and input. "The same input" means the sha256 of the input's canonical JSON. That call's `tool.called` carries `approval: <seq>`, which uses the grant up.
  - Rejected: holding the hook open until the human answers, because the hook has a 10-second limit and a session must not sit for hours. Also rejected: an escalation with reason `permission`, because it moves the task to `escalated`, while asking is not something going wrong.
- **Which sessions get custom connectors.** Those about a task: `refine`, `plan`, `explore`, `implement`, `verify`, and the Designer's design review. These are the sessions an approval can wait in.
  - `triage`, which is given one Farik tool, gets none.
  - Neither do `ceremony`, `conversation` or `chat`, which are about no task and have nowhere to wait.
  - Step 04 decides how the Scrum Master's bridge reaches ceremonies. Playwright keeps step 12's rule.
- **Allowances are step 03's** (project plan). This step's grant is per call, and an allowance later pre-approves calls before this check.
- **ADR 0030 records** the launcher, approvals waiting like questions, and the move of connecting into step 01. It is written in Task 2's commit, because each of these binds later steps.

What a non-technical user sees:
- **Approvals: Today's list and one dialog.** Today's list of what waits on the user gets the row "<name> wants to use <service>". It opens `ToolApproval`, which shows the tool's plain name, the input as the agent wrote it in an `untrusted` frame, "Allow once" and "Don't allow".
- **The agent page's connector list.** It shows each connected server with "Remove", which asks to confirm.
- **Adding a custom connector is under Advanced.** That is where the canvas's newer `AgentEdit` already puts it: "Farik has not checked it, so you label each of its tools yourself."
  - `ConnectorAdd` takes the user through three steps: how to start it (command or web address, and its keys), label its tools, done.
  - The labels are "Only reads" (`network`), "Changes things, asks you" (`external_effect`) and "Never" (`denied`).
  - The canvas's `ConnectorAdd` breaks two made decisions, and the mockup task removes both:
    - Its "Don't ask me" box is pre-authorisation. That is the allowance, which step 03 builds, and only for tools that spend credits.
    - "They reuse this sign-in" breaks per-agent credentials.
  - The kit's "Recommended" connectors, with their setup copy, are step 03's.

Security:
- **Secrets stay out.** Never in `team.yaml`, `mcp.json`, an event, a log line, a reply or the prompt.
  - `connector.connected` records the key names and never their values.
  - A refused `connector.connect` drops its parameters from the error, as `account.connect` does (8.6).
  - The launcher route's answer is never logged.
- **Deny by default.**
  - A connector not given to the session: `connector_not_in_session`.
  - A tool the pin does not name: `tool_not_tagged`.
  - `denied`: refused, and passed to `--disallowedTools`.
  - `external_effect`: asks.
  - A launch for an unregistered session, or a server the session lacks: refused.
  - A keychain that fails at launch: the server does not start. The session runs without that connector, and the hook refuses the connector's calls.
- **Untrusted output.** Everything a connector returns is untrusted (8.6). The prompt's untrusted-content notice names each connector the session was given. Claude Code hands MCP results to the model directly, so wrapping each result is phase 8's job: its governed gateway proxies connectors, and the governor moves out of the `PreToolUse` hook there (research notes, 2026-10-01). On Claude Code the hook sees every connector call, which is enough for this phase.

Open, for the founder:
- **O1, sign in with OAuth.** Remote MCP servers sign in by OAuth: PKCE, dynamic client registration, refresh tokens per agent in the keychain. Recommendation: a step of its own after step 03 and before the first kit, renumbering the rest. This step handles pasted keys and tokens only, and step 07's Stripe sign-in needs OAuth. Rejected: folding it into step 01, which doubles this step, or into step 03, which is already the largest step.
- **O2, a computer with no keychain.** Recommendation: as ADR 0022 does for the model credential, a file only its owner can read, `connectors.json` in the user's Farik state folder (0600 in 0700), and `ConnectorAdd` says which was used. The alternative is to refuse to connect without a keychain. WSL and headless Linux often lack one.
- **O3, the mockups.** Task 1 is approved by the founder before Task 9's code.

## File map

```
docs/design/mockups/{AgentEdit,ConnectorAdd,ToolApproval}.dc.html, canvas.json   creates/modifies: Task 1
docs/decisions/0030-connector-credentials-and-approvals.md      creates: the ADR above
docs/schemas/team.schema.json                                   modifies: mcpServer custom variant
crates/core/src/team.rs                                         modifies: validation; tests
crates/core/src/governor/permissions.rs                         modifies: connector call with approvals; tests
crates/runtime/src/connectors.rs                                creates: keychain store, listing, launch spec; tests
crates/runtime/src/daemon/hooks.rs                              modifies: any session connector, approval_needed
crates/runtime/src/daemon/app.rs                                modifies: POST /connector/launch
crates/runtime/src/daemon/team.rs                               modifies: connector.connect/disconnect/list_tools
crates/runtime/src/claude.rs                                    modifies: launcher and headersHelper in mcp.json
crates/runtime/src/orchestrator/session.rs                      modifies: custom servers into SessionSpec
crates/runtime/src/orchestrator/messages.rs                     modifies: approval decisions as the human's message
crates/runtime/src/prompt.rs                                    modifies: the notice names connectors
crates/runtime/tests/fixture_mcp.rs                             creates: a stdio MCP server for tests (rmcp server)
docs/schemas/{event,command,rpc}.schema.json                    modifies: events, tool.approve/refuse, connector RPCs
crates/store/src/waiting.rs, projections.rs                     modifies: open approvals wait on the human
crates/cli/src/connector.rs                                     creates: connect, disconnect, connector run/headers
crates/cli/src/human.rs                                         modifies: farik tool approve|refuse
apps/web/src/pages/{AgentEdit,ConnectorAdd,ToolApproval}.tsx    creates/modifies, with tests
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies
```

## Interfaces

Consumes: `ConnectorTag`, `SessionConnector`, `evaluate_connector_call` (`farik-core`, main); `SessionRegistration.connectors`, `ToolContext.connectors`, `McpServerConfig`, `SessionSpec.disallowed_tools`, `write_session_files`, `child_env`, `KeychainStore`'s `keyring` use (`farik-runtime`, main); `builtin_connector` (`farik-roles`, main); `question.asked`'s waiting projection (`farik-store`, main).

Produces:

```rust
// farik-core
pub enum McpServer { Builtin { name: String }, Custom(CustomServer) }
pub struct CustomServer { pub name: String, pub transport: CustomTransport,
    pub credential_keys: Vec<String>, pub tools: BTreeMap<String, ConnectorTag> }
pub enum CustomTransport { Stdio { command: String, args: Vec<String> },
    Http { url: String, headers: BTreeMap<String, String> } }
pub struct SessionConnector { pub server: String, pub origin: Option<String>,
    pub tools: BTreeMap<String, ConnectorTag> }            // origin: Some for preview-confined
pub enum ConnectorRefusal { /* existing */, ApprovalNeeded }
pub struct ApprovalKey { pub agent_id: String, pub task_id: TaskId, pub server: String,
    pub tool: String, pub input_sha256: String }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>,
    granted: Option<u64>) -> Result<(ConnectorTag, Option<u64>), ConnectorRefusal>;
// farik-runtime
pub trait ConnectorSecrets: Send + Sync {
    fn load(&self, at: &SecretAt) -> Result<BTreeMap<String, Secret>, CredentialError>;
    fn save(&self, at: &SecretAt, keys: &BTreeMap<String, Secret>) -> Result<(), CredentialError>;
    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError>; }
pub struct SecretAt { pub project_id: String, pub agent_id: String, pub server: String }
pub struct LaunchSpec { pub command: String, pub args: Vec<String>, pub env: BTreeMap<String, Secret> }
pub async fn list_tools(server: &CustomServer, keys: &BTreeMap<String, Secret>)
    -> Result<Vec<ListedTool>, ConnectorError>;            // ListedTool { name, description }
```

Wire (`snake_case`): events `connector.connected { agent, server, transport, credential_keys, tools }`, `connector.disconnected { agent, server }`, `approval.requested { approval, server, tool, input }`, `approval.granted { approval, note }`, `approval.refused { approval, note }`; `tool.called` gains `approval?`; commands `tool.approve`, `tool.refuse`; RPCs `connector.list_tools`, `connector.connect`, `connector.disconnect`.

## Tasks

### Task 1: The connector screens, mocked up

Files: on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, page "Team and settings") and copied into `docs/design/mockups/`:
- `AgentEdit`: the canvas's newer version, 1560 high, which the repository lacks. It gains a connected custom server row with "Remove" and its confirmation.
- `ConnectorAdd`: on the canvas only until now. It is revised to the three steps and three labels above, without "Don't ask me" or a shared sign-in.
- `ToolApproval`: new. Today's row and the dialog, at desktop and phone width.

Gate: the founder approves the three boards, and the approval is written into this plan's Decisions with its date. No task after this one starts until then.

- [ ] `docs(design): mock up the connector screens and the tool approval`

### Task 2: Custom servers in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, ADR 0030. Produces `McpServer`, `CustomServer`, `CustomTransport`.

- `accepts_a_custom_stdio_and_a_custom_http_server`: both shapes validate and read back equal.
- `refuses_a_custom_server_named_farik_or_playwright`: each gives `connector_name_reserved` at `/agents/0/mcp_servers/0/name`.
- `refuses_url_on_stdio_and_command_on_http`: each is a schema error at its field.
- `refuses_a_header_naming_an_undeclared_key`: `{TOKEN}` with `credential_keys: [API_KEY]` gives `header_key_unknown`.
- `refuses_a_tool_tagged_read`: a schema error at `/agents/0/mcp_servers/0/tools/x`.
- `keeps_builtin_entries_as_they_were`: step 12's fixture still validates, unchanged.

- [ ] `feat(core): describe custom MCP servers in the team file`

### Task 3: Per-agent keys in the keychain

Files: `crates/runtime/src/connectors.rs`. Produces `ConnectorSecrets`, `SecretAt`, a keychain implementation and an in-memory one for tests.

- `keeps_keys_under_the_project_agent_and_server`: the account is `connector:p:theo:github` and the service is `farik`.
- `deleting_one_agents_keys_leaves_anothers`: after deleting theo's, iris's same server still loads.
- `maps_no_keychain_to_no_keychain`: `NoDefaultStore` gives `CredentialError::NoKeychain`, as `credential.rs` does.
- `a_secret_never_prints`: the `Debug` form of `LaunchSpec` shows key names and `***`.

- [ ] `feat(runtime): keep each agent's connector keys in the keychain`

### Task 4: Listing a server's tools

Files: `connectors.rs`, `crates/runtime/tests/fixture_mcp.rs`, the workspace `Cargo.toml` (rmcp features). Produces `list_tools`, `launch_spec`.

- `lists_a_stdio_servers_tools`: against the fixture, the list is its three tools by name.
- `the_server_sees_its_keys_and_not_the_model_key`: the fixture's `env` tool reports `API_KEY` set and neither `ANTHROPIC_API_KEY` nor `CLAUDE_CODE_OAUTH_TOKEN`, with both set in the test's own environment.
- `fills_http_headers_from_keys`: `Bearer {API_KEY}` becomes `Bearer k`, and an http fixture sees it.
- `gives_up_after_thirty_seconds`: a fixture that never answers gives `ConnectorError::Timeout` (paused clock).

- [ ] `feat(runtime): list an MCP server's tools with the agent's keys`

### Task 5: The governor's connector check, with approvals

Files: `crates/core/src/governor/permissions.rs`. Produces the new `evaluate_connector_call`, `ApprovalKey`, `ConnectorRefusal::ApprovalNeeded`.

- `a_network_tool_runs_with_no_origin`: `origin: None` runs any `url`.
- `a_preview_connector_still_checks_urls`: step 12's `url_outside_preview` cases are unchanged.
- `external_effect_without_a_grant_needs_approval`: `ApprovalNeeded`.
- `external_effect_with_a_grant_runs_and_names_it`: `Ok((ExternalEffect, Some(7)))`.
- `denied_is_refused_even_with_a_grant`: `ToolDenied`.
- `the_approval_key_ignores_key_order`: `{a:1,b:2}` and `{b:2,a:1}` give one `input_sha256`.

- [ ] `feat(core): let an external_effect connector call run once the human allows it`

### Task 6: The hook, and approvals that wait

Files: `daemon/hooks.rs`, the event and command schemas, `store/src/waiting.rs` and `projections.rs`, `orchestrator/messages.rs`, `cli/src/human.rs`.

- `the_hook_judges_any_connector_the_session_has`: a custom server's `network` tool is allowed for an agent without the `network` tier.
- `an_unknown_mcp_server_is_still_not_in_session`: `connector_not_in_session`.
- `an_external_effect_call_asks_and_stops`: the call is denied `approval_needed`, `approval.requested` is recorded, and the session's stop reason is set.
- `an_open_approval_waits_on_the_human`: the task is listed in `waiting.list` and the orchestrator starts no session for it.
- `approve_then_the_same_call_runs_once`: after `tool.approve`, the matching call is allowed with `approval` on `tool.called`, and a second one asks again.
- `a_different_input_is_not_approved`: one changed field asks again.
- `refuse_tells_the_next_session`: its human's message carries the refusal and the note.
- `farik_tool_approve_sends_the_command`: `farik tool approve 12` sends `tool.approve { approval: 12 }` and prints the plain line.

- [ ] `feat(runtime): ask the human before a connector changes anything`

### Task 7: Custom connectors in sessions, through the launcher

Files: `orchestrator/session.rs`, `claude.rs`, `daemon/app.rs`, `prompt.rs`, `cli/src/connector.rs`.

- `gives_custom_servers_to_task_sessions_only`: present for refine, plan, explore, implement and verify; absent for triage, ceremony, conversation and chat.
- `mcp_json_holds_no_secret`: a stdio entry is the launcher command, an http entry has `headersHelper`, and no key's value appears in the file.
- `denied_tools_join_disallowed_tools`: `mcp__github__delete_repo` follows `Bash`.
- `launch_refuses_an_unregistered_session_or_server`: 404 with `unknown_session`, and 403 with `connector_not_in_session`.
- `connector_run_execs_with_a_clean_environment`: the launcher run against the fixture shows only `PATH`, `HOME`, `LANG`, `TMPDIR` and the keys.
- `the_notice_names_the_connectors`: the untrusted-content section names `github` when given.
- `a_live_session_calls_a_custom_connector` (integration, `--integration`): Claude Code calls the fixture's `network` tool through the launcher, and a `denied` tool is never called.

- [ ] `feat(runtime): load each agent's connectors into its sessions`

### Task 8: Connect and disconnect

Files: `daemon/team.rs`, the RPC and event schemas, `cli/src/connector.rs`, `crates/cli/Cargo.toml` (`rpassword`).

- `connect_lists_tags_saves_and_records`: `connector.connect` keeps the keys, writes the entry, and records `connector.connected` with key names only.
- `an_unlabelled_tool_is_written_external_effect`.
- `a_refused_connect_echoes_no_secret`: the error's text has no key value.
- `disconnect_removes_entry_and_keys_and_records`: another agent's same server is untouched.
- `farik_connect_reads_keys_from_stdin`: `--key API_KEY` reads its line from stdin, and an argument like `--key API_KEY=v` is refused.

- [ ] `feat(cli): connect and disconnect an MCP server for one agent`

### Task 9: The screens

Files: `apps/web/src/pages/AgentEdit.tsx`, `ConnectorAdd.tsx`, `ToolApproval.tsx`, their tests, the strings. Built from Task 1's approved boards.

- `agent_edit_lists_connectors_and_removes_after_confirming`.
- `connector_add_offers_three_labels_and_defaults_to_asks`.
- `connector_add_clears_the_key_field_after_sending`.
- `today_lists_an_approval_and_the_dialog_sends_approve_or_refuse`.
- `the_input_is_shown_as_untrusted_text`: markup in the input renders as text.

- [ ] `feat(web): connectors on the agent page and tool approvals`

### Task 10: Spec and plan

`docs/SPEC.md`: 5.6 (custom servers, labels, approvals), 6.7 (connect, per-agent keys), 8.2 (the launcher in `mcp.json`), 8.5 (the five events), 8.6 (the launcher, the clean environment, the no-sandbox residual). `docs/plans/project-plan.md`: rows 01 and 03. `docs/design/role-kits.md`: its step table.

- [ ] `docs(spec): connectors per agent with keys and approvals`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_calls_a_custom_connector passed
```
