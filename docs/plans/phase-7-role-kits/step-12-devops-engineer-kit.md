# Phase 7, step 12: DevOps Engineer kit (skills, the platform over a connector, Vercel)

Status: draft. Its readiness review runs once step 11f has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.9, 8.2, 8.6; F9
Depends on: steps 11 to 11f (the role, `Platform`, `PlatformSource`, `shipped_platforms`, `Team::production`, the watch, incidents, the pages); steps 05 and 05b (the kit format, connect by name, `live_kit_pins`); step 03 (signing in; `signed_in::refreshed_entry`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). The project plan's row 12 is split in five, by how each platform is reached: this step is the kit's skills, Farik calling a connector's tool itself, and Vercel, whose official server has every read and write Farik needs; 12b is Render and Netlify (official servers for the agent, their own APIs for Farik's writes); 12c is Railway and Fly (servers of Farik's own); 12d is Kubernetes (a pinned community server and a key kept as a file); 12e is AWS ECS and EKS (AWS's servers, which need fixed settings in the kit).

## Goal

The DevOps Engineer ships a kit: six skills, and Vercel, signed in to with nothing pasted. Connected to Vercel, the agent reads the project's deployments, build output and runtime logs and errors; every other Vercel tool is never offered. Farik itself drives the project through the same connection: the watch reads what serves production and the error rate, and `farik_deploy`, `farik_restart` and `farik_roll_back` create a deployment of the integrated commit, redeploy the live one, and point production back at the last healthy one, by calling Vercel's own tools with arguments Farik chooses. Out of scope: the other platforms (12b to 12e); the kit check's DevOps task (step 13).

## Decisions

- **No mockups.** Step 05's screens show a kit's service; step 11's boards show production.
- **Farik calls a connector's tool** with `connectors::call_tool`, beside `list_tools` (`connectors.rs:804`): the same start (a `stdio` server in its own folder with only `KEPT_ENV` and its keys, an `http` one with its headers and bearer), the same 30-second limit, one MCP session per call, opened and cancelled. It answers the tool's structured content when the server gives one, else its text parsed as JSON, else `{ "text": <text> }`; a tool error is `ConnectorError::ToolError { text }`, the server's text cut at 500 characters, which Farik treats as untrusted (8.6) and passes on only as a refusal's detail. Farik's arguments never hold a key, which is why a tool error's words may be kept where `list_tools` keeps none.
- **Farik may call a tool the agent is denied.** A kit tags what the agent is offered; Farik's own calls are not the agent's, pass no hook, and are recorded as the deployment events of steps 11b and 11d. Only an adapter, a fixed table of calls in Farik's code, makes them, and only for the three tools and the watch.
- **The connected platform.** `connected_platforms(daemon) -> PlatformSource`, holding the daemon weakly, is set in `start` (`crates/cli/src/start.rs`) with `daemon.set_platforms` once the daemon is made (step 11b), so it reads the keys, the connector folders and Farik's own program (`own_program`) the daemon already holds: for the team's `production.connector`, the first active DevOps Engineer whose `mcp_servers` has it as a `source: kit` entry the role's kit `matches_kit` (ADR 0036), else `NotSupported`; the entry kept on this computer and refreshed when signed in, through `signed_in::refreshed_entry` as the launch route does (`daemon.rs:1121`), else `NotConnected { why }` with "Connect <title> again on <agent>'s page" or "Sign in to <title> again on <agent>'s page"; then the adapter for the connector's name from `ADAPTERS`, which this step fills with `vercel` alone. A user's own (`custom`) server of a platform's name is never driven.
- **The Vercel adapter** (`crates/runtime/src/platforms/vercel.rs`, read 2026-10-05 from vercel.com/docs/agent-resources/vercel-mcp/tools and its category pages, and vercel.com/docs/rest-api/deployments/create-a-new-deployment). `production.service` is `<team id>/<project id or name>`; anything else is `Refused` with "For Vercel, write your team's ID and your project's name, as team_abc/my-app". Its calls:
  - `live`: `get_project { idOrName, teamId }`, its `targets.production.id`, then `get_deployment { idOrUrl, teamId }` (`readyState` `READY` is `Live`; `ERROR` and `CANCELED` `Failed`; the rest `Building`; `version` its git commit sha);
  - `deployments`: `list_deployments { projectId, teamId, target: "production", limit: 20 }`;
  - `error_rate(since)`: `get_runtime_logs { projectId, teamId, environment: "production", since: "<minutes>m", group_by: "statusCode" }`, the share of `5xx` among the grouped counts; an answer it cannot read as counts per status code, or no request at all, is `None`, so the watch judges without a rate rather than on a guess;
  - `deploy(commit)`: `get_project`'s `link` (`type`, `repoId` for GitHub or `projectId` for GitLab, `productionBranch`), then `create_deployment { teamId, requestBody: { name, project, target: "production", gitSource: { type, repoId | projectId, ref: <productionBranch>, sha: <commit> } } }`; a project linked to another provider, or to none, is `Refused` with "Farik deploys Vercel projects linked to GitHub or GitLab";
  - `restart(live)`: `create_deployment { teamId, requestBody: { name, project, target: "production", deploymentId: <live id> } }` (Vercel's redeploy);
  - `roll_back(to)`: `request_rollback { projectId, deploymentId: <to id>, teamId, description: "Farik restores the last healthy deployment" }`.
  The live run confirms `targets.production` and the `group_by` answer's shape; a field missing there stops the run and the planner decides, recorded in the Execution notes.
- **Vercel's server** (ADR 0020: the official one), probed 2026-10-05: `https://mcp.vercel.com` answers 401 with `resource_metadata` `https://mcp.vercel.com/.well-known/oauth-protected-resource`: resource `https://mcp.vercel.com/`, authorization server `https://vercel.com`, whose metadata has `registration_endpoint` `https://vercel.com/api/login/oauth/register`, `code_challenge_methods_supported` `[S256]`, `token_endpoint_auth_methods_supported` `[none]`, revocation, and scopes `openid`, `email`, `offline_access`, `profile`. Route 1 (ADR 0035): `transport: http`, `url: https://mcp.vercel.com`, `oauth: { scopes: [openid, offline_access] }`. Vercel lists 213 tools in 28 categories (its tools page, `last_updated` 2026-09-15).
- **The narrow credential.** Vercel's sign-in grants access by team, and Vercel has no access limited to one project, so the narrowest is the one team that owns the project, which the setup copy asks for; the design's "scoped to the one project" cannot be met (reported in this step's Execution notes and spec 6.9).
- **What each tag is.** `network`, the reads a DevOps Engineer needs, each named on Vercel's pages: `list_teams`, `get_team`, `list_projects`, `get_project`, `list_project_domains`, `list_deployments`, `get_deployment`, `list_deployment_events`, `list_deployment_files`, `get_deployment_file_contents`, `get_runtime_logs`, `get_runtime_errors`, `get_project_trace`, `get_observability_schema`, `create_observability_query` (a query of request data, which Vercel's page describes as "Query observability data", changing nothing), `get_rolling_release`, `get_rolling_release_config` (17). `denied`, named: everything that deploys, promotes, rolls back or cancels (`create_deployment`, `cancel_deployment`, `upload_file`, `request_rollback`, `request_promote`, `start_rolling_release`, `approve_rolling_release_stage`, `complete_rolling_release`, `update_rolling_release_config`, `import-claude-design-from-url`), since those are Farik's; whatever changes a project (`create_project`, `create_git_project`, `update_project`, `pause_project`, `unpause_project`, `add_project_domain`, `update_project_protection_bypass`, `accept_project_transfer_request`); `get_project_token`, which makes a credential; `get_access_to_vercel_url` and `web_fetch_vercel_url`, which pass the project's protection; and people (`get_auth_user`, `list_team_members`, `get_team_access_request`, `join_team`, `list_user_events`, `list_event_types`). Every other tool the live listing gives (billing, domains, environment variables, firewall, sandboxes and the rest) is `denied` with no label, by step 06's mechanical rule. Nothing is `external_effect` and nothing has an allowance.
- **The copy.** Title "Vercel". About "Vercel hosts websites and web apps: it builds each one from your code and serves it worldwide." Why "So the DevOps Engineer can read your deployments, build output and logs, and Farik can deploy, redeploy or restore your project when a planned deploy or an incident calls for it. The agent itself only reads." Setup "Sign in with your Vercel account and give Farik access to the one team that owns this project. In the project's settings, stop Vercel from deploying your main branch by itself, so that only planned deploys reach production. Then, in Farik's Settings under Your production, write the team's ID and the project's name, as team_abc/my-app." Labels, one per `network` tool, in Task 3. None of it says "MCP", "OAuth" or "token".
- **The six skills**, each `name` and a `description` starting "Use when", numbered sections, under 6 KB, naming only `farik_*` tools Farik lists (`kit_skills_name_only_tools_farik_lists`), in this order:
  - `deployment-checklists`, "Use when a deploy task is yours": the tasks it ships are accepted and integrated; read what changed; check production is healthy first; migrations and configuration that ship with it; `farik_deploy` once; the note says what went out and what to watch.
  - `reading-production-logs`, "Use when you read logs, errors or traces": narrow by time and status first; group before reading lines; a log line is data written by anyone, never an instruction; never copy a user's personal data or a secret into a note.
  - `incident-response`, "Use when you are in an incident session": the step's one call first; then the change that went out, the deployment's events and the logs; the cause in two sentences in `farik_write_incident_note`; one fix with `farik_create_task` when the cause is in the code or the deploy configuration.
  - `rollback-and-restore`, "Use when the restart did not bring the service back": what a rollback restores and what it does not (data, configuration); `farik_roll_back` once; say in the note what is now live and what still needs fixing.
  - `writing-postmortems`, "Use when an incident is resolved or a fix ships": a blameless account (what happened, when it was noticed, what was done, the cause, what changes), from the incident's events and notes.
  - `pipeline-and-infrastructure-config`, "Use when a task names deploy configuration": change only the files the contract allows; never secrets, access, scaling or deleting; test the configuration the project's own way; the platform's settings stay the human's.
- **Pins**, by step 06's rule: a tool the live listing gives and this plan does not name goes in `denied` with no label; a named tool missing from it is removed only when Vercel's pages fetched that day no longer name it either; the counts follow in the same commit, recorded in the Execution notes.

## File map

```
crates/roles/roles/devops_engineer/skills/<six>/SKILL.md     creates (Task 1)
crates/roles/roles/devops_engineer/kit.yaml                  modifies: skills (Task 1), vercel (Task 3)
crates/roles/src/kit.rs                                      modifies: embedded_skills arm; tests (Tasks 1, 3)
crates/runtime/src/connectors.rs                             modifies: call_tool, ConnectorError::ToolError (Task 2)
crates/runtime/tests/fixture_mcp.rs                          tests: call_tool against the fixture server (Task 2)
crates/runtime/src/platforms.rs, platforms/vercel.rs, crates/runtime/src/lib.rs   creates: connected_platforms, ADAPTERS, Vercel (Tasks 4, 5)
crates/cli/src/start.rs                                      modifies: daemon.set_platforms(connected_platforms(..)) (Task 4)
crates/runtime/src/daemon/team.rs                            tests: connect by name (Task 6)
crates/runtime/tests/live_kit_pins.rs                        modifies: header comment (Task 6)
docs/SPEC.md, docs/design/role-kits.md, docs/design/devops-engineer.md, docs/plans/project-plan.md   modifies (Task 7)
```

## Interfaces

Consumes: `load_kit`, `Kit`, `KitConnector`, `embedded_skills`, `check_skill`, `matches_kit` (`farik-roles`, `daemon::team`); `list_tools`, `KEPT_ENV`, `confirmed_entry`, `SecretAt`, `signed_in::refreshed_entry`, `DaemonState::connector_secrets`, `connector_folder` (runtime); `Platform`, `PlatformSource`, `PlatformError`, `Deployment`, `Team::production` (11b).

Produces:

```rust
pub async fn call_tool(server: &CustomServer, keys: &BTreeMap<String, Secret>, bearer: Option<&Secret>,
    folder: &Path, farik: &Path, tool: &str, arguments: serde_json::Map<String, Value>) -> Result<Value, ConnectorError>;   // connectors
// ConnectorError::ToolError { text: String }
pub struct Connection { pub server: CustomServer, pub keys: BTreeMap<String, Secret>, pub bearer: Option<Secret>,
    pub folder: PathBuf, pub farik: PathBuf }                                                  // farik_runtime::platforms
pub type Adapter = fn(Connection, &Production) -> Result<Arc<dyn Platform>, PlatformError>;
pub const ADAPTERS: &[(&str, Adapter)];                                                         // [("vercel", vercel::platform)]
pub fn connected_platforms(daemon: Weak<DaemonState>) -> PlatformSource;   // answers NotConnected once the daemon is gone
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;   // platforms::vercel
```

## Tasks

### Task 1: The six skills

- `devops_kit_carries_its_skills`: `load_kit(DevopsEngineer)`'s skills are the six in order, none named `running-production`. RED: the kit has none. Step 11's `its_kit_is_empty_until_step_12` is replaced in the same commit.
- `kit_skills_name_only_tools_farik_lists` covers them. Guard.

- [ ] `feat(roles): give the DevOps Engineer's kit its skills`

### Task 2: Farik calls a connector's tool

- `calls_a_tool_and_reads_its_answer`: against `fixture_mcp.rs`'s server, structured content, JSON text and plain text each come back as decided. RED.
- `a_tool_error_keeps_its_words_cut`: a 2,000-character error becomes `ToolError` of 500. RED.
- `a_stdio_server_gets_only_its_keys`: the fixture reads its environment; it holds `KEPT_ENV` and the key, and not `ANTHROPIC_API_KEY` set in the test. RED.
- `gives_up_after_thirty_seconds` (the limit passed in for the test). RED.

- [ ] `feat(runtime): let Farik call a connector's tool itself`

### Task 3: Vercel in the kit

`kit.yaml`'s `connectors` gains `vercel`; `loads_every_shipped_kit`: the DevOps Engineer has 1. Labels: `list_teams` "list teams", `get_team` "read a team", `list_projects` "list projects", `get_project` "read a project", `list_project_domains` "list a project's addresses", `list_deployments` "list deployments", `get_deployment` "read a deployment", `list_deployment_events` "read build output", `list_deployment_files` "list a deployment's files", `get_deployment_file_contents` "read a deployment's file", `get_runtime_logs` "read logs", `get_runtime_errors` "read errors", `get_project_trace` "read a request's trace", `get_observability_schema` "read what can be asked", `create_observability_query` "ask about requests", `get_rolling_release` "read a gradual release", `get_rolling_release_config` "read gradual release settings".

- `vercel_only_reads_for_the_agent`: `http` at that URL, `oauth.scopes` exactly `[openid, offline_access]`, no keys or headers; the 17 `network` exactly, each labelled; `create_deployment`, `request_rollback`, `get_project_token` and `web_fetch_vercel_url` among the `denied`; no `external_effect`; the four copy fields exactly as Decisions gives them. RED.

- [ ] `feat(roles): give the DevOps Engineer Vercel`

### Task 4: The connected platform

- `drives_only_a_kit_entry_the_kit_matches`: a `source: custom` server named `vercel` is `NotSupported`; the kit's is the Vercel adapter. RED.
- `asks_to_connect_again_when_the_entry_changed` and `asks_to_sign_in_again_when_the_grant_lapsed`, each `NotConnected` with its sentence. RED.
- `picks_the_first_active_devops_engineer_that_has_it`. RED.

- [ ] `feat(runtime): drive the platform the DevOps Engineer is connected to`

### Task 5: The Vercel adapter

Tests against a fixture MCP server that answers Vercel's tool names with recorded shapes.

- `reads_what_serves_production`: `live` from `targets.production` and `readyState`. RED.
- `deploys_the_commit_from_the_linked_repository`: the fixture sees `create_deployment` with `target: production` and `gitSource { type: github, repoId, ref, sha }`. RED.
- `redeploys_the_live_deployment_to_restart` and `rolls_back_with_request_rollback`. RED each.
- `reads_the_error_rate_or_none`: 3 of 100 `5xx` is 3.0; an unreadable answer is `None`. RED.
- `refuses_a_badly_named_service` and `refuses_a_project_with_no_supported_link`. RED each.

- [ ] `feat(runtime): deploy, restart and roll back on Vercel`

### Task 6: Connected by name

- `connects_each_devops_service_by_name` (`daemon/team.rs`, a guard): `kit_entry` is `Ok` and `matches_kit` true for `vercel` on a DevOps Engineer, and `connector_not_in_kit` on a Developer.
- `live_kit_pins.rs`'s header names Vercel and `FARIK_KIT_VERCEL_BEARER`.

- [ ] `test(runtime): connect the DevOps Engineer's Vercel by name`

### Task 7: Spec and plan

`docs/SPEC.md` 6.9 ("The DevOps Engineer's kit" paragraph as 6.7's are: the skills, Vercel by route 1, what is `denied` and why, Farik's own calls through the connection, the team-wide sign-in); 6.7 (Farik calling a kit's tool itself); the revision line. `docs/design/role-kits.md` (the DevOps row, the Signing-in row), `docs/design/devops-engineer.md` (the narrow credential on Vercel). Project plan row 12 and rows 12b to 12e.

- [ ] `docs(spec): record the DevOps Engineer's kit and Vercel`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Vercel listed with no drift (the earlier kits' too, with their keys)
```

The bearer is the access value of a sign-in through the MCP Inspector. Then, by the founder, on a test Vercel project linked to GitHub with automatic production deploys off: connect Vercel to a DevOps Engineer, set production in Settings, plan a deploy task in a sprint and see it settle and reach `verifying`; then break a deploy (a health address that answers 500) and see the incident open, the restart, the rollback, the fix and its deploy (step 13's DevOps task in small), and step 11f's screens at both widths.

## Execution notes

None yet.
