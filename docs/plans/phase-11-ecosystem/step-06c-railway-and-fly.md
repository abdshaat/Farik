# Phase 11, step 06c: Railway and Fly.io, through Farik's own servers

Status: draft. Its readiness review runs once step 06b has landed.
Branch: `phase/11-ecosystem` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.9, 8.6; F9
Depends on: step 06 (`call_tool`, `ADAPTERS`, `Connection`); phase 7 step 10g (`FARIK_CONNECTORS` as `[osv, google-ads, fx, recalls, ebay]`, the offline pin through the built binary); phase 7 step 07 (`farik_runtime::osv`, the pattern; ADR 0038); step 05f (`Production.tsx`); phase 6 (merged in #19)
Readiness confirmed by: not yet run
Moved 2026-10-09 by ADR 0049 (project plan revision 41; the founder: "DevOps later, rest after Cloud"): phase 7 step 12c until then (its file was `step-12c-railway-and-fly.md` in phase 7's folder). The DevOps Engineer is built in the Ecosystem phase, phase 11, after its own steps 01 to 04: phase 7's steps 11 to 11f are steps 05 to 05f here, and 12 to 12e are 06 to 06e. The text below names them by their new numbers, and phase 7's other steps as phase 7's; the dated lines above, and the founder's words, keep the numbers of their day. Phase 7 step 10h, ask or auto, is phase 9 step 01; step 13, the kit check, is phase 9 step 02 and has no DevOps task, so this phase checks the DevOps Engineer's kit itself; the phases after phase 8 moved up by one.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 06 (see step 06's header).

## Goal

A team on Railway or on Fly.io connects it to the DevOps Engineer with one pasted key that reaches only that project's environment (Railway) or that app (Fly.io). Neither platform has an official server a kit can run that reads deployments and logs and can restart or roll back, so each is a small server of Farik's own, shipped in the Farik binary as `osv` is: the agent reads deployments and logs through it, and Farik deploys, restarts and rolls back through its other tools, which the agent is never offered. Fly.io deploys images, not commits, so the production settings gain the image's name, with `{commit}` where the commit goes, which steps 06d and 06e use too. Out of scope: Kubernetes (06d); AWS (06e).

## Decisions

- **Why Farik's own** (ADR 0020's third choice; ADR 0038), probed 2026-10-05:
  - **Railway.** Its remote server, `https://mcp.railway.com/mcp`, signs in by route 1 (`backboard.railway.com`'s metadata: `registration_endpoint`, S256, `none`, scopes down to `project:viewer`), but its tools (docs.railway.com/reference/mcp-server: `whoami`, `list-projects`, `create-project`, `list-services`, `list-feature-flags`, `get-feature-flag`, `set-feature-flag`, `delete-feature-flag`, `redeploy`, `accept-deploy`, `railway-agent`) list no deployment, read no log and cannot roll back. Its local package, `@railway/mcp-server` (0.1.12), drives the `railway` program, which a kit cannot start (ADR 0036) and which keeps its own sign-in outside Farik's key store. Railway's API does all of it (docs.railway.com/reference/public-api and /integrations/api/manage-deployments, read 2026-10-05): `https://backboard.railway.com/graphql/v2`, a project key in the `Project-Access-Token` header, "scoped to a specific environment within a project"; the queries `deployments`, `deploymentLogs`, `buildLogs`, `httpLogs`; the mutations `deploymentRestart`, `deploymentRollback`, and `serviceInstanceDeployV2(commitSha, environmentId, serviceId)`.
  - **Fly.io.** Its server is inside the `flyctl` program (`fly mcp server`, docs.fly.io/flyctl/mcp-server, read 2026-10-05), which a kit cannot start, and its tool names are not published; `mcp.fly.io` did not answer. Fly's Machines API (`https://api.machines.dev/v1`: `GET /apps/{app}/machines`, `POST /apps/{app}/machines/{id}` with a new config, `POST /apps/{app}/machines/{id}/restart`) and its logs (`https://api.fly.io/api/v1/apps/{app}/logs`) do all of it, with a deploy key Fly makes for one app.
  Rejected: the official remote for Railway's reads plus Farik's writes through Railway's API (two credentials, and the agent still without logs); a community Fly server (none is maintained and pinned on npm or PyPI).
- **The servers**, `farik_runtime::railway` and `farik_runtime::fly`, hand-written `rmcp` `ServerHandler`s over stdio as `farik_runtime::osv` is, `serverInfo.name` `farik-railway` and `farik-fly`; started by `farik connector railway` and `farik connector fly` (`ConnectorCommands::Railway`, `::Fly`); `FARIK_CONNECTORS` becomes `["osv", "google-ads", "fx", "recalls", "ebay", "railway", "fly"]`. Each address is a constant, never an input; one `reqwest::Client` with `redirect::Policy::none()`, `no_proxy()`, no cookies, a 25-second timeout, at most 1 MiB read in chunks; the key comes from the environment the launcher gives (`RAILWAY_KEY`, `FLY_KEY`), never from an input, and a missing key is a tool error saying which. Every input is checked before any request; a non-2xx answer is a tool error with the platform's message cut at 500 characters; nothing the platform says is followed.
- **Railway's tools.** For the agent, `network`: `deployments { service }` (the newest 20 for the service in the key's environment: id, status, commit sha, created), `deployment_logs { deployment, lines? }`, `build_logs { deployment, lines? }`, `http_logs { deployment, lines? }` (`lines` 1 to 500, default 100). Farik's, `denied` to the agent: `deploy_commit { service, commit }`, `restart { deployment }`, `roll_back { deployment }`. Ids are UUIDs and a commit 40 hex characters, checked. The environment is the key's own, read once per call with the `projectToken` query, so no input names it.
- **Fly.io's tools.** For the agent, `network`: `machines { app }` (each machine's id, state, region, image and checks) and `logs { app, lines? }`. Farik's, `denied` to the agent: `set_image { app, image }`, which updates each machine's config to the image one machine at a time, waiting up to 60 seconds for it to start before the next, and stops at the first that fails; and `restart { app }`. `app` matches `^[a-z0-9-]{1,63}$`; `image` is a registry reference with a tag or digest, no whitespace, at most 300 characters.
- **The image name.** `production` gains `image`, a reference with `{commit}` exactly once (e.g. `registry.fly.io/my-app:{commit}`), at most 300 characters (`image_template_invalid` otherwise), optional; `Production.image: Option<String>` and `Production::image_for(commit) -> Option<String>`. `Production.tsx` shows the field always, labelled "Image name, with {commit} where the commit goes" and "Only for platforms that run images: Fly.io, AWS and Kubernetes". Farik never builds an image: the user's pipeline pushes one per commit; an image that does not exist fails the rollout, which the watch sees.
- **The adapters** (`platforms/railway.rs`, `platforms/fly.rs`) call their server's tools through `call_tool`, as Vercel's does. Railway: `production.service` is the service's id; `deployments` and `live` from `deployments` (`SUCCESS` is `Live` for the newest such, older ones `Superseded`; `FAILED`, `CRASHED` are `Failed`; the rest `Building`); `deploy` `deploy_commit`; `restart` `restart` of the live deployment; `roll_back` `roll_back` to `to`. Fly.io: `production.service` is the app's name and `production.image` is required (`Refused` naming the field otherwise); a deployment is the image the machines run, its `id` the image and its `version` the commit Farik recorded with it, so `live` is `Live` when every machine is `started` on one image and `Building` otherwise; `deploy(commit)` is `set_image(image_for(commit))`; `restart` is `restart`; `roll_back(to)` is `set_image(to.id)`. `error_rate` is `None` on both.
- **The copy.** Railway: title "Railway"; about "Railway runs your app's services from your code, each in its own environment."; why "So the DevOps Engineer can read your service's deployments and logs, and Farik can deploy, restart or roll back the service when a planned deploy or an incident calls for it. The agent itself only reads."; setup "In your Railway project's settings, open ‘Tokens’ and create one for your production environment, then paste it here. It reaches only that environment. Turn off the service's automatic deploys from your branch, so that only planned deploys reach production. In Farik's Settings, the service name is the service's ID." Fly.io: title "Fly.io"; about "Fly.io runs your app on machines close to your users, from an image your build makes."; why "So the DevOps Engineer can read your app's machines and logs, and Farik can move them to the image of your planned work, restart them, or move them back when a planned deploy or an incident calls for it. The agent itself only reads."; setup "Your build must push an image for every commit, named by the commit. In the Fly.io dashboard, open your app, then ‘Tokens’, and create a deploy key; paste it here. It reaches only this app. In Farik's Settings, the service name is the app's name and the image name is like registry.fly.io/my-app:{commit}." Both say nothing of "MCP", "OAuth" or "token" outside the quoted labels.
- **Pins.** Each is pinned offline through the built binary, `crates/cli/tests/railway_server.rs` and `fly_server.rs`, as `osv_server.rs` is; the live run skips them with its line (phase 7 step 10d).

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs       modifies: production.image, image_for (Task 1)
apps/web/src/pages/Production.tsx(+test), apps/web/src/strings/en.ts   modifies (Task 1)
crates/runtime/src/railway.rs, crates/runtime/src/fly.rs, crates/runtime/src/lib.rs   creates (Tasks 2, 4)
crates/cli/src/connector_run.rs, crates/cli/src/lib.rs       modifies: farik connector railway | fly (Tasks 2, 4)
crates/cli/tests/railway_server.rs, crates/cli/tests/fly_server.rs   creates (Tasks 3, 5)
crates/roles/src/kit.rs, crates/roles/roles/devops_engineer/kit.yaml   modifies: FARIK_CONNECTORS, the entries (Tasks 3, 5)
crates/runtime/src/platforms.rs, platforms/railway.rs, platforms/fly.rs   modifies/creates: the adapters (Tasks 3, 5)
crates/runtime/src/daemon/team.rs                            tests: connect by name (Task 6)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md   modifies (Task 7)
```

## Interfaces

Consumes: `call_tool`, `ADAPTERS`, `Connection` (06); `osv::serve_stdio` as the pattern, `FARIK_CONNECTORS`, `is_farik_connector`, `CliIo`, `ConnectorCommands` (on this branch); `Production`, `Production.tsx` (05b, 05f).

Produces:

```rust
impl Production { pub fn image_for(&self, commit: &str) -> Option<String>; }   // Production.image: Option<String>
pub const RAILWAY_API: &str = "https://backboard.railway.com/graphql/v2";     // farik_runtime::railway
pub fn tool_names() -> Vec<&'static str>;
pub async fn serve_stdio(api: &str) -> Result<(), RailwayError>;
pub const MACHINES_API: &str = "https://api.machines.dev/v1";                 // farik_runtime::fly
pub const FLY_LOGS_API: &str = "https://api.fly.io/api/v1";
pub fn tool_names() -> Vec<&'static str>;
pub async fn serve_stdio(machines: &str, logs: &str) -> Result<(), FlyError>;
pub fn railway(io: &mut CliIo<'_>) -> i32;  pub fn fly(io: &mut CliIo<'_>) -> i32;   // farik_cli::connector_run
// FARIK_CONNECTORS = ["osv", "google-ads", "fx", "recalls", "ebay", "railway", "fly"]; ADAPTERS gains railway and fly
```

## Tasks

### Task 1: The image name

- `reads_an_image_template`: `image_for("bbb…")` gives `registry.fly.io/app:bbb…`. RED.
- `refuses_a_bad_image_template`: none or two `{commit}`, whitespace, 301 characters: `image_template_invalid` at `/production/image`. RED.
- `production_page_saves_the_image_name`. RED.

- [ ] `feat(core): name the image a commit deploys as`

### Task 2: Railway's server

Tests against a local GraphQL fixture whose address is passed to the function.

- `lists_exactly_the_seven_tools`. RED.
- `reads_deployments_in_the_keys_environment`: one `projectToken` query, then `deployments` for that environment and the service, mapped as decided. RED.
- `deploys_restarts_and_rolls_back`: the fixture sees `serviceInstanceDeployV2` with the commit, environment and service, `deploymentRestart`, `deploymentRollback`, each with `Project-Access-Token`. RED.
- `refuses_bad_input_before_sending`; `follows_no_redirect_and_no_proxy`; `cuts_an_oversized_answer`; `says_when_the_key_is_missing`. RED each.

- [ ] `feat(runtime): serve Railway through Farik's own server`

### Task 3: Railway in the kit, and its adapter

`railway`: `stdio`, `command: farik`, `args: [connector, railway]`, `credential_keys: [RAILWAY_KEY]`, `key_page: https://railway.com/dashboard`. Labels: `deployments` "list deployments", `deployment_logs` "read logs", `build_logs` "read build output", `http_logs` "read requests".

- `railway_only_reads_for_the_agent`: the four `network`, labelled; the three writes `denied`; the copy exactly. RED.
- `railway_server_lists_the_kits_tools` (`crates/cli/tests/railway_server.rs`). RED.
- `the_railway_adapter_calls_its_tools` (against the fixture through `call_tool`). RED.

- [ ] `feat(roles): give the DevOps Engineer Railway`

### Task 4: Fly.io's server

- `lists_exactly_the_four_tools`. RED.
- `sets_the_image_one_machine_at_a_time`: three machines, the second failing to start: the third is untouched, and the error names the second. RED.
- `restarts_each_machine`; `reads_machines_and_logs`; `refuses_a_bad_app_or_image`; `follows_no_redirect_and_no_proxy`; `says_when_the_key_is_missing`. RED each.

- [ ] `feat(runtime): serve Fly.io through Farik's own server`

### Task 5: Fly.io in the kit, and its adapter

`fly`: `stdio`, `command: farik`, `args: [connector, fly]`, `credential_keys: [FLY_KEY]`, `key_page: https://fly.io/dashboard`. Labels: `machines` "read machines", `logs` "read logs".

- `fly_only_reads_for_the_agent`; `fly_server_lists_the_kits_tools`. RED each.
- `deploying_on_fly_sets_the_commits_image`: `deploy("bbb…")` calls `set_image` with `image_for`; with no image name, `Refused` naming the field. RED.
- `a_fly_rollback_sets_the_last_healthy_image`. RED.

- [ ] `feat(roles): give the DevOps Engineer Fly.io`

### Task 6: Connected by name

- `connects_each_devops_service_by_name` gains `railway` and `fly`. Guard.
- `is_farik_connector` holds for `[connector, railway]` and `[connector, fly]`, and not `[connector, fly, x]`. RED.

- [ ] `test(runtime): connect the DevOps Engineer's Railway and Fly.io by name`

### Task 7: Spec and plan

`docs/SPEC.md` 6.7 ("Farik's own connectors" names `railway` and `fly`, their addresses, tools, limits and checks), 6.9's kit paragraph (both, the keys' reach, Farik's tools denied to the agent, the image name); the revision line. `docs/design/role-kits.md`, `docs/design/devops-engineer.md`. Project plan row 06c.

- [ ] `docs(spec): record Railway and Fly.io in the DevOps Engineer's kit`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check); railway_server and fly_server pass offline
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok; railway and fly skipped with their offline line
```

Then, by the founder, on a test Railway service and a test Fly.io app whose build pushes an image per commit: one planned deploy each, then one broken deploy each restored.

## Execution notes

None yet.
