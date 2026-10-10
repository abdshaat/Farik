# Phase 12, step 06b: Render and Netlify

Status: draft. Its readiness review runs once step 06 has landed.
Branch: `phase/12-ecosystem` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.9, 8.6; F9
Depends on: step 06 (`ADAPTERS`, `Connection`, `connected_platforms`, the six skills); steps 05b and 05c (`Platform`, `Deployment`); phase 7 steps 05 and 05b; phase 6 (merged in #19)
Readiness confirmed by: not yet run
Moved 2026-10-09 by ADR 0049 (project plan revision 41; the founder: "DevOps later, rest after Cloud"): phase 7 step 12b until then (its file was `step-12b-render-and-netlify.md` in phase 7's folder). The DevOps Engineer is built in the Ecosystem phase, phase 12, after its own steps 01 to 04: phase 7's steps 11 to 11f are steps 05 to 05f here, and 12 to 12e are 06 to 06e. The text below names them by their new numbers, and phase 7's other steps as phase 7's; the dated lines above, and the founder's words, keep the numbers of their day. Phase 7 step 10h, ask or auto, is phase 10 step 01; step 13, the kit check, is phase 10 step 02 and has no DevOps task, so this phase checks the DevOps Engineer's kit itself; the phases after phase 9 moved up by one.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 06 (see step 06's header).

## Goal

A team on Render or Netlify connects it to the DevOps Engineer with one pasted key. The agent reads through each platform's official server: on Render the services, deploys, events, logs and metrics; on Netlify the sites and deploys (Netlify's server reads no logs). Catervas drives the service with the same key through the platform's own API, since neither server can restart or roll back: on Render a deploy of the integrated commit, a restart and a rollback to a deploy; on Netlify the publishing of the finished build of the integrated commit, and the publishing of the live or the last healthy deploy again. Out of scope: Railway and Fly (06c); Kubernetes (06d); AWS (06e).

## Decisions

- **No mockups.** Phase 7 step 05's key form shows each.
- **Why a pasted key, and why the platforms' APIs** (ADR 0020, ADR 0035), probed 2026-10-05:
  - **Render.** `https://mcp.render.com/mcp` answers 401 naming `https://mcp.render.com/.well-known/oauth-protected-resource/mcp`: resource `https://mcp.render.com/mcp`, authorization server `https://api.render.com`, whose metadata has S256 and `none` but no `registration_endpoint`, so route 1 cannot register Catervas; Render documents its server with an API key (render.com/docs/mcp-server). Its tools (github.com/render-oss/render-mcp-server, README, read 2026-10-05) can `trigger_deploy` but not restart or roll back; Render's API can (api-docs.render.com, read 2026-10-05: `POST /services/{serviceId}/deploys` with `commitId`, "The SHA of a specific Git commit to deploy"; `POST /services/{serviceId}/restart`; `POST /services/{serviceId}/rollback` with `deployId`). So the agent reads through the official server, and Catervas's adapter calls the API at the fixed address `https://api.render.com/v1` with the same key. Rejected: Catervas's writes split between `trigger_deploy` and the API (two ways in for one adapter); a server of Catervas's own for the agent (the official one reads logs and metrics).
  - **Netlify.** The remote `https://mcp.netlify.com/mcp` signs in by route 1 (its metadata: `registration_endpoint` `/oauth-server/reg`, S256, `none`, scopes `offline_access`, `read`, `write`), but its sign-in is its own server's and reaches Netlify's API only through that server, which neither publishes nor restores a site's deploy (its remote `deploy-site` answers a command for the user to run, `dist/netlify-mcp.js` of `@netlify/mcp@1.17.0`). So the agent's server is the official package run locally, `npx @netlify/mcp@1.17.0` (npm, published 2026-09-30), which reads `NETLIFY_PERSONAL_ACCESS_TOKEN`, and Catervas's adapter uses the same key at the fixed address `https://api.netlify.com/api/v1` (open-api.netlify.com, read 2026-10-05: `GET /sites/{site_id}`, `GET /sites/{site_id}/deploys`, `POST /sites/{site_id}/deploys/{deploy_id}/restore`). Rejected: signing in for the agent and a pasted key for Catervas, two credentials for one service.
- **Each API client** (`crates/runtime/src/platforms/render.rs`, `netlify.rs`): `reqwest` with no redirect, a 15-second timeout, at most 1 MiB read in chunks, `Authorization: Bearer <key>` from `Connection.keys`, the address a constant, never an input; every id Catervas puts in a path is checked against the platform's pattern first (Render `^srv-[a-z0-9]{1,40}$` and `^dep-[a-z0-9]{1,40}$`; Netlify a UUID for the site, `^[0-9a-f]{24}$` for a deploy). A non-2xx answer is `Refused` for 4xx and `Failed` for 5xx, with the platform's `message` cut at 500 characters.
- **Render's adapter.** `production.service` is the service's id (`srv-…`). `deployments`: `GET /services/{id}/deploys?limit=20` (`status` `live` is `Live`; `build_failed`, `update_failed`, `canceled`, `pre_deploy_failed` are `Failed`; `deactivated` is `Superseded`; the rest `Building`; `version` the deploy's `commit.id`); `live`: the newest `live` one; `error_rate`: `None` (the agent reads metrics with `get_metrics`; the watch judges Render by the address and the live deploy); `deploy(commit)`: `POST …/deploys { commitId }`; `restart`: `POST …/restart`, answering no new deployment; `roll_back(to)`: `POST …/rollback { deployId }`.
- **Netlify's adapter.** `production.service` is the site's id. `live`: `GET /sites/{id}`'s `published_deploy`; `deployments`: `GET /sites/{id}/deploys?per_page=20` (`state` `ready` is `Live` when it is the published one and `Superseded` otherwise, `error` is `Failed`, the rest `Building`; `version` its `commit_ref`); `error_rate`: `None`. **Deploying is publishing** (Netlify builds every push itself): with auto publishing stopped, as the setup copy asks, each push leaves a finished deploy unpublished, and `deploy(commit)` publishes, through `restore`, the newest `ready` deploy whose `commit_ref` is the commit or holds it (`Git::merge_base`); with none it is `Refused` with "Netlify has no finished build of <short sha> yet. Push the integration branch, let Netlify build it, then deploy again." `restart(live)` publishes the live deploy again; `roll_back(to)` publishes `to`. Rejected: triggering a build (`POST /sites/{id}/builds`), which builds whatever the branch holds then, not the commit Catervas chose.
- **What each tag is.** Render, from the README's names: `network` (11) `list_workspaces`, `select_workspace` (it chooses which workspace this connection reads and changes nothing at Render), `get_selected_workspace`, `list_services`, `get_service`, `list_deploys`, `get_deploy`, `list_events`, `list_logs`, `list_log_label_values`, `get_metrics`; `denied` (15) every create and update (`create_web_service`, `create_static_site`, `create_cron_job`, `update_environment_variables`, `update_web_service`, `update_static_site`, `update_cron_job`), `trigger_deploy` (Catervas's), and every database tool (`query_render_postgres`, which reads the business's rows; `list_postgres_instances`, `get_postgres`, `create_postgres`, `list_key_value`, `get_key_value`, `create_key_value`, whose details hold connection strings). Netlify, from the package's source, which registers one reader and one updater per domain and `netlify-coding-rules`: `network` (2) `netlify-deploy-services-reader` and `netlify-project-services-reader`; `denied` (7) `netlify-team-services-reader` (it reads the team's environment variables, which hold secrets, in the same tool as the rest), `netlify-user-services-reader`, `netlify-extension-services-reader`, the three updaters (`netlify-deploy-services-updater`, `netlify-project-services-updater`, `netlify-extension-services-updater`), and `netlify-coding-rules` (Netlify's instructions; the role's are Catervas's skills, and a service's words are data, 8.6). Anything else either live listing gives is `denied` unlabelled (phase 7 step 06's rule).
- **The narrow key.** Render's keys reach everything the account can, and Netlify's every site of the account; neither offers a key for one service, so the setup copy says so and asks for the narrowest each allows (a Render account that belongs only to this workspace; a Netlify key with an expiry). The design's "scoped to the one project" cannot be met on either (reported in spec 6.9).
- **The copy.** Render: title "Render"; about "Render runs web services, workers and databases from your code."; why "So the DevOps Engineer can read your services' deploys, logs and metrics, and Catervas can deploy, restart or roll back your service when a planned deploy or an incident calls for it. The agent itself only reads."; setup "In Render, open your account settings and create an API key, then paste it here. Render's keys reach everything your account can, so if you can, make it from an account that belongs only to this service's workspace. In the service's settings, set ‘Auto-Deploy’ to ‘No’, so that only planned deploys reach production. In Catervas's Settings, the service name is the service's ID, starting srv-." `key_page: https://dashboard.render.com/u/settings#api-keys`. Netlify: title "Netlify"; about "Netlify builds and hosts websites and web apps from your code."; why "So the DevOps Engineer can read your site's deploys, and Catervas can publish the build of your planned work, or an earlier one, when a planned deploy or an incident calls for it. The agent itself only reads."; setup "This needs Node.js on your computer. In Netlify, open your user settings, then ‘Personal access tokens’, and choose ‘New access token’ with an expiry date; paste the value here. It reaches every site your account can. On your site's Deploys page, choose ‘Stop auto publishing’, so that only planned deploys go live. In Catervas's Settings, the service name is the site's ID." `key_page: https://app.netlify.com/user/applications#personal-access-tokens`. The quoted labels are Netlify's and Render's own, each under 60 characters, the one exception to the words rule (ADR 0036).

## File map

```
crates/roles/roles/devops_engineer/kit.yaml                  modifies: render, netlify (Tasks 1, 3)
crates/roles/src/kit.rs                                      modifies: tests (Tasks 1, 3)
crates/runtime/src/platforms/render.rs, netlify.rs           creates (Tasks 2, 4)
crates/runtime/src/platforms.rs                              modifies: ADAPTERS (Tasks 2, 4)
crates/runtime/src/daemon/team.rs                            tests: connect by name (Task 5)
crates/runtime/tests/live_kit_pins.rs                        modifies: header comment (Task 5)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md   modifies (Task 6)
```

## Interfaces

Consumes: `ADAPTERS`, `Adapter`, `Connection`, `Platform`, `Deployment`, `DeploymentState`, `PlatformError`, `Production` (steps 05b, 06); `Git::merge_base` (on main); `load_kit`, `kit_entry`, `matches_kit`.

Produces:

```rust
pub const RENDER_API: &str = "https://api.render.com/v1";                               // platforms::render
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;
pub const NETLIFY_API: &str = "https://api.netlify.com/api/v1";                         // platforms::netlify
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;
// ADAPTERS = [("vercel", …), ("render", render::platform), ("netlify", netlify::platform)]
```

## Tasks

### Task 1: Render in the kit

`render` after `vercel`; `loads_every_shipped_kit`: 2. `transport: http`, `url: https://mcp.render.com/mcp`, `headers: { Authorization: "Bearer {RENDER_API_KEY}" }`, `credential_keys: [RENDER_API_KEY]`. Labels: `list_workspaces` "list workspaces", `select_workspace` "choose a workspace", `get_selected_workspace` "read the chosen workspace", `list_services` "list services", `get_service` "read a service", `list_deploys` "list deploys", `get_deploy` "read a deploy", `list_events` "read a service's events", `list_logs` "read logs", `list_log_label_values` "list log filters", `get_metrics` "read metrics".

- `render_only_reads_for_the_agent`: the entry as above; the 11 `network` exactly, labelled; the 15 `denied`, among them `trigger_deploy` and `query_render_postgres`; 26 in all; the four copy fields exactly. RED.

- [ ] `feat(roles): give the DevOps Engineer Render`

### Task 2: Render's adapter

Tests against a local fixture of Render's API, its address passed to the client's constructor in tests.

- `deploys_restarts_and_rolls_back_on_render`: the fixture sees `POST /services/srv-a1/deploys {"commitId":"bbb…"}`, `POST …/restart`, `POST …/rollback {"deployId":"dep-c3"}`, each with the bearer. RED.
- `reads_render_deploys_and_the_live_one`: the statuses map as decided, and `live` is the newest `live`. RED.
- `refuses_a_bad_render_id_before_sending`: `srv-../x` and `dep-A` send nothing. RED.
- `follows_no_redirect_and_cuts_a_large_answer`. RED.

- [ ] `feat(runtime): deploy, restart and roll back on Render`

### Task 3: Netlify in the kit

`netlify` after `render`; `loads_every_shipped_kit`: 3. `transport: stdio`, `command: npx`, `args: ["@netlify/mcp@1.17.0"]`, `credential_keys: [NETLIFY_PERSONAL_ACCESS_TOKEN]`. Labels: `netlify-deploy-services-reader` "read deploys", `netlify-project-services-reader` "read sites".

- `netlify_only_reads_for_the_agent`: `npx` with that exact pin, the one key, the key page; the 2 `network`, labelled; the 7 `denied`, among them `netlify-team-services-reader`; 9 in all; the copy exactly. RED.

- [ ] `feat(roles): give the DevOps Engineer Netlify`

### Task 4: Netlify's adapter

- `deploying_publishes_the_finished_build_of_the_commit`: with ready deploys of `aaa…` and of `ccc…` (a descendant of `aaa…`), `deploy("aaa…")` restores the `ccc…` deploy when it is newer; the fixture sees `POST /sites/<id>/deploys/<ccc deploy>/restore`. RED.
- `refuses_when_no_finished_build_holds_it`: the sentence above, and no request but the reads. RED.
- `restart_and_roll_back_publish_again`. RED.
- `reads_netlify_deploys_and_the_published_one`. RED.

- [ ] `feat(runtime): publish, republish and restore on Netlify`

### Task 5: Connected by name

- `connects_each_devops_service_by_name` gains `render` (keyed) and `netlify` (keyed). Guard.
- `live_kit_pins.rs`'s header names Render (`CATERVAS_KIT_RENDER_RENDER_API_KEY`) and Netlify (`CATERVAS_KIT_NETLIFY_NETLIFY_PERSONAL_ACCESS_TOKEN`).

- [ ] `test(runtime): connect the DevOps Engineer's Render and Netlify by name`

### Task 6: Spec and plan

`docs/SPEC.md` 6.9's kit paragraph (Render and Netlify, the pasted key, Catervas's calls to their APIs, the keys' reach, publishing on Netlify, no logs on Netlify); the revision line. `docs/design/role-kits.md` and `docs/design/devops-engineer.md` (the table's Vercel and Netlify, Render rows as built). Project plan row 06b.

- [ ] `docs(spec): record Render and Netlify in the DevOps Engineer's kit`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
CATERVAS_LIVE_TESTS=1 cargo test -p catervas-runtime --test live_kit_pins
# expected: ok, Render and Netlify listed with no drift
```

The run needs Node.js for Netlify's package and the two keys. Then, by the founder, on a test Render service and a test Netlify site: one planned deploy each, then one broken deploy each restored.

## Execution notes

None yet.
