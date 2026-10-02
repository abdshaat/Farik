# Phase 7, step 03b: Farik's registered apps (GitHub)

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.6; F9
Depends on: step 03 of this phase (committed before this step starts: `start_sign_in`, `SignIn`, `OAuthGrant`, `refreshed`, `revoke`, `SignInError`, the loopback listener, `connector.sign_in` and `connector.sign_in_status`, `SignedIn`, `read_kept`, `connect_with`, `ConnectorAdd`'s sign-in)
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 5 Blocking, all folded with the founder's decisions; no second round (ADR 0032)
Mockups approved by: pending (Task 1's gate)

## Goal

After step 03, GitHub answers "doesn't let Farik sign in by itself yet", because it registers no client automatically. When this step is done, a user gives an agent GitHub's MCP server (`https://api.githubcopilot.com/mcp/`) by pressing "Sign in with GitHub", typing the short code Farik shows on GitHub's page, and saying yes. Farik uses its own GitHub App, registered once by the founder, and keeps, refreshes and hands over the grant exactly as step 03 does. Out of scope: Slack, whose sign-in relay is deferred to phase 11 (the founder, 2026-10-02) and which takes a pasted key until then; choosing which GitHub server each kit uses (steps 06 and 07); and Google, deferred until after the launch with Drive (ADR 0035's amendment of 2026-10-02): at launch Google Analytics, the one Google service a kit ships, has no remote MCP server (Google's `analytics-mcp` runs locally), so step 06 connects it as a local server with a key, Notion covers product docs, and nothing before the launch needs Farik's Google app.

## Decisions

ADR 0035 records the routes and, in its amendment, the reviewed facts this plan rests on; this plan builds route 2 for GitHub.

The table:
- **`REGISTERED_APPS` is a static list in `crates/runtime/src/registered_apps.rs`**, one `RegisteredApp` per app: its id (`github`), the name the screens show (`GitHub`), the one host it serves, its flow, its public `client_id`, its issuer, its token endpoint, its revocation endpoint (none for GitHub) and its install address. Rejected: discovering the app from the server's metadata, because a server could name GitHub's authorization server and receive a token from Farik's app; a token sent to a host the app does not serve is the one thing this table prevents.
- **Matched by the server's URL alone**, before step 03's discovery. Parsed with `url::Url` (already in the lock through `reqwest` and `rmcp`; it becomes a direct dependency of `farik-runtime`). The host is `host_str()`, compared to the entry's host ignoring ASCII case; a trailing dot does not match. The scheme is `https` on port 443 (`port_or_known_default() == Some(443)`), or, as step 03's https-or-loopback rule allows for the test fixture, `http` on a loopback host at any port. A URL with a username or a password matches nothing. GitHub's host is exactly `api.githubcopilot.com`.
- **Farik's client ids are used only on their own hosts.** When the team file's `oauth.client_id` equals a table entry's `client_id` and the URL does not match that entry, signing in is refused `Failed("this sign-in is only for <name>'s own servers")` (the RPC's `sign_in_failed`) before any request. When it matches, the table entry is used, as if no `client_id` were given. Any other `oauth.client_id`, or no match, runs step 03 unchanged.
- **The table ships empty** until Task 7, which adds the GitHub entry with the founder's client id. Every test before it passes a table of the test's own: `start_sign_in` and `connect_with` take the table as a parameter, and the daemon reads it from `DaemonState` (Interfaces). Tests build their table with `Box::leak` for the fixture's addresses; the leak is per test and accepted.
- **The client id is public and committed. No client secret is shipped or sent:** GitHub's device flow needs none, for the sign-in or the refresh.

GitHub (device flow, RFC 8628):
- `POST https://github.com/login/device/code` with `client_id`; the answer's `device_code`, `user_code`, `verification_uri`, `expires_in` and `interval` (default 5 s). A `verification_uri` other than the table's (`https://github.com/login/device`) is `Failed`.
- Farik polls `POST https://github.com/login/oauth/access_token` with `client_id`, `device_code` and `grant_type=urn:ietf:params:oauth:grant-type:device_code`, first after `interval`, then every `interval`. `authorization_pending` waits; `slow_down` adds 5 s to the interval for every later poll; `access_denied` is `Denied("access_denied")`; `expired_token` and the 10-minute window (step 03's `SIGN_IN_WINDOW`, whichever comes first) are `TimedOut`; any other `error` is `Failed`.
- **Every request to the device and token endpoints** (the device code, each poll, each refresh) sends `Accept: application/json`, since GitHub otherwise answers form-encoded. GitHub answers errors with HTTP 200, so Farik reads `error` from the JSON body whatever the status, before the status.
- A device attempt polls only inside `SignIn::finish`. Dropping the future (the daemon aborts the old attempt's task, as step 03 does for a new attempt) stops the polling.
- No listener, no `state`, no PKCE: the device code never leaves Farik, and the user code is typed by the person at GitHub. No `scope` is sent: a GitHub App's permissions are set on the app (read-only, Task 7). For a Device entry the team file's `oauth.scopes` are ignored and the grant's `scopes` is empty.
- The grant: `issuer` `https://github.com/login/oauth` (GitHub's metadata issuer, read 2026-10-02), `resource` the server's URL (kept for step 03's bookkeeping), `client_id`, `token_endpoint` the access-token URL, `revocation_endpoint` none (revoking needs the secret), `expires_at` from `expires_in` (8 hours), the refresh token (6 months), and `app: Some("github")`.
- **Refresh** is step 03's form POST with three changes, applied to every grant (they are harmless to step 03's services): `Accept: application/json`; `error` read from the body whatever the status; `bad_refresh_token` and `incorrect_client_credentials` lapse the grant, as `invalid_grant` does. For a grant with `app` set, no `resource` is sent; no client secret is ever sent.
- **Tokens accumulate.** Since Farik cannot revoke, every "Connect again" leaves a live grant at GitHub, and GitHub keeps at most 10 tokens per user, app and scope, revoking the oldest. An old agent's grant can then lapse; with the rule above it shows "Sign in again". Accepted.
- After signing in, the screens say that a private repository needs Farik installed on it, with a link to the table's `install_url`, `https://github.com/apps/<slug>/installations/new`.

What a user sees and the wire:
- **The wire.** `connector.sign_in`'s answer gains `provider?` (the table's display name), `user_code?` and `install_url?`. For GitHub, `authorize_url` is the `verification_uri`. `issuer` stays. `team.get`'s connector rows gain `provider?`, the table's display name for a kept grant with `app`.
- **`ConnectorAdd`:** "Sign in with GitHub" (the provider's name instead of step 03's issuer host; the "for <host>" line is not shown, since the table fixes the host). Then a waiting board shows the code large, "Copy the code", and "Open github.com/login/device", which opens the page in a new tab in the click, and under the code, small: "Only enter a code that this page shows you. Farik never sends you a code in a chat." (Farik's public id lets any program, an agent among them, start a device flow that GitHub labels "Farik"; the warning is the defence, ADR 0035.) After signing in, the private-repository line and its link.
- **`AgentEdit`'s row** reads "Signed in to GitHub". Remove's confirmation is step 03's no-revocation sentence, naming GitHub's settings.
- **The command line** prints `Open https://github.com/login/device and enter the code ABCD-1234.`, then the same warning sentence, opens the page, and waits, as step 03 does for its address.

## File map

```
docs/design/mockups/{ConnectorAdd,AgentEdit}.dc.html, canvas.json        Task 1
crates/runtime/src/registered_apps.rs, lib.rs, Cargo.toml              creates: RegisteredApp, AppFlow, the table, app_for; `url` direct (Task 2); the entry (Task 7)
crates/runtime/src/sign_in.rs                                          modifies: the app branch, device flow, OAuthGrant.app, callback_addr (Task 3); refresh (Task 4)
crates/runtime/tests/fixture_oauth.rs                                  modifies: the device endpoints, request recording (Tasks 3, 4)
crates/runtime/src/daemon.rs                                           modifies: set_registered_apps, SignedIn.provider (Task 5)
crates/runtime/src/daemon/team.rs, docs/schemas/rpc.schema.json        modifies: provider, user_code, install_url (Task 5)
packages/protocol-client/src/mapping.ts                                modifies: providerName, userCode, installUrl (Task 5)
crates/cli/src/connector.rs                                            modifies: connect_with's table, the device code line (Task 5)
apps/web/src/pages/{ConnectorAdd,AgentEdit}.tsx, connectors.test.tsx, strings/en.ts   modifies (Task 6)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md     modifies (Task 7)
```

## Interfaces

Consumes: `start_sign_in`, `SignIn`, `OAuthGrant`, `refreshed`, `revoke`, `SignInError`, `SIGN_IN_WINDOW`, `OAuthSettings`, `SignedIn`, `read_kept`, `connect_with` and the fixture (step 03); the sign-in RPCs and `ConnectorAdd` (step 03).

Produces:

```rust
// farik-runtime, registered_apps.rs
pub enum AppFlow { Device { device_endpoint: &'static str, verification_uri: &'static str } }
pub struct RegisteredApp { pub id: &'static str, pub name: &'static str, pub host: &'static str, pub flow: AppFlow,
    pub client_id: &'static str, pub issuer: &'static str, pub token_endpoint: &'static str,
    pub revocation_endpoint: Option<&'static str>, pub install_url: Option<&'static str> }
pub static REGISTERED_APPS: &[RegisteredApp];
pub fn app_for<'a>(apps: &'a [RegisteredApp], url: &str) -> Option<&'a RegisteredApp>;
// farik-runtime, sign_in.rs (changed signatures)
pub async fn start_sign_in(url: &str, settings: &OAuthSettings, apps: &[RegisteredApp], now: DateTime<Utc>)
    -> Result<SignIn, SignInError>;
impl SignIn { pub fn user_code(&self) -> Option<&str>; pub fn provider(&self) -> Option<&str>;
    pub fn install_url(&self) -> Option<&str>;
    pub fn callback_addr(&self) -> Option<std::net::SocketAddr>; }  // None for the device flow; step 03's tests unwrap it
pub struct OAuthGrant { /* step 03's fields */ #[serde(default)] pub app: Option<String> }
// farik-runtime, daemon.rs
impl DaemonState { pub fn set_registered_apps(&self, apps: &'static [RegisteredApp]) -> bool; }
// a OnceLock, like set_state_dir; unset means REGISTERED_APPS
pub(crate) struct SignedIn { pub lapsed: bool, pub revokes: bool, pub provider: Option<String> }
// read_kept sets provider from the grant's app, looked up by id in the daemon's table
// farik cli
pub(crate) fn connect_with(project: &Project, asked: &Asked<'_>, io: &mut CliIo<'_>,
    open: &dyn Fn(&str), apps: &[RegisteredApp]) -> Result<Report, String>;  // `connect` passes REGISTERED_APPS
```

Wire (`snake_case`): `connector.sign_in → { attempt, authorize_url, issuer, provider?, user_code?, install_url? }`; `team.get`'s connector rows gain `provider?`.

## Tasks

### Task 1: The registered-app screens, mocked up

A Sonnet agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job, and copies them to `docs/design/mockups/`.

- **`ConnectorAdd`, GitHub offered.** The address `https://api.githubcopilot.com/mcp/`; "GitHub lets you sign in." and the primary button "Sign in with GitHub"; under it "Use a key instead".
- **`ConnectorAdd`, GitHub waiting.** "Enter this code on GitHub:", the code `WDJB-MJHT` large in the monospace face, under it small "Only enter a code that this page shows you. Farik never sends you a code in a chat.", "Copy the code", the primary button "Open github.com/login/device", "Waiting for you on GitHub…", and "Cancel".
- **`ConnectorAdd`, GitHub signed in.** "Signed in to GitHub." and the line "To let Theo read private repositories, install Farik on them on GitHub." with the link "Install Farik on GitHub", then Next.
- **`AgentEdit`.** A row "github · Reached at a web address · Signed in to GitHub · 9 tools: 9 only read", and its Remove confirmation: "Remove github from Theo? Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in GitHub's settings."

Gate: the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date. Task 6 does not start until then; Tasks 2 to 5 do not depend on the boards.

- [ ] `docs(design): mock up signing in with GitHub`

### Task 2: The table

Files: `registered_apps.rs`, `lib.rs`, `crates/runtime/Cargo.toml` (`url`, at the version already in the lock). Produces `RegisteredApp`, `AppFlow`, `REGISTERED_APPS` (empty), `app_for`.

- `matches_github_by_its_exact_host`: a table with a GitHub-shaped entry (host `api.githubcopilot.com`). Match: `https://api.githubcopilot.com/mcp/`, `https://API.githubcopilot.com/mcp/`, `https://api.githubcopilot.com:443/mcp/`. No match: `https://api.githubcopilot.com.evil.example/mcp/`, `http://api.githubcopilot.com/mcp/`, `https://api.githubcopilot.com:8443/mcp/`, `https://api.githubcopilot.com./mcp/`, `https://api.githubcopilot.com@evil.example/mcp/`, `https://user@api.githubcopilot.com/mcp/`, and a string that is not a URL.
- `matches_a_loopback_fixture_over_http`: `http://127.0.0.1:4000/mcp` matches an entry with host `127.0.0.1`; `http://10.0.0.1:4000/mcp` does not match an entry with host `10.0.0.1`.
- `the_shipped_table_is_empty_until_the_founder_registers`: `REGISTERED_APPS.is_empty()`. Task 7 replaces this test with `the_shipped_table_names_github`.

- [ ] `feat(runtime): name the apps Farik registers with a service`

### Task 3: Signing in with Farik's GitHub App

Files: `sign_in.rs`, `fixture_oauth.rs`. Produces the new `start_sign_in`, `SignIn::user_code`, `SignIn::provider`, `SignIn::install_url`, `callback_addr` as `Option`, `OAuthGrant.app`. Step 03's tests that call `callback_addr` unwrap it.

The fixture gains `/device/code` and device-code answers at `/token`, always with status 200, form-encoded when the request lacks `Accept: application/json` and JSON with it. Flags: `authorization_pending` N times, `slow_down` once, `access_denied`, `expired_token`. Each `/device/code` answer has a distinct `device_code` (`dc-<n>`) and `interval: 1`. The fixture records every request's path, headers and form. The tests' table points a Device entry, id `dev`, name `Dev`, at the fixture over `http` on loopback with host `127.0.0.1`, which the https-or-loopback check already allows.

- `signs_in_with_the_device_flow`: with `oauth.scopes: ["repo"]` in the settings, `user_code()` is the fixture's, `authorize_url()` its `verification_uri`, `provider()` is `Dev`, `install_url()` the table's, `callback_addr()` is `None`; after two `authorization_pending` answers the grant holds both tokens, `app: Some("dev")`, the table's `issuer`, the server URL as `resource`, empty `scopes` and no `revocation_endpoint`. `/device/code` got no `scope`; `/token` got `grant_type=urn:ietf:params:oauth:grant-type:device_code` and no `client_secret`; every request to `/device/code` and `/token` carried `Accept: application/json`.
- `slows_down_when_asked`: with `interval: 1` and `slow_down` answered to the first poll, on the paused clock the second poll comes no sooner than 6 s after the first.
- `device_flow_reports_denied_and_expired`: `access_denied` gives `Denied("access_denied")`; `expired_token` gives `TimedOut`; ten minutes of `authorization_pending` gives `TimedOut`.
- `refuses_an_unexpected_verification_page`: a `verification_uri` other than the table's gives `Failed`.
- `dropping_a_device_attempt_stops_polling`: a `finish()` future dropped after the first poll: over 30 s of paused clock the fixture sees no further poll with that `device_code`.
- `farik_s_client_id_is_refused_elsewhere`: with `oauth.client_id` set to the test table's `client_id` and a URL outside the table, `start_sign_in` gives `Failed` whose text names `Dev`, and the fixture saw no request at all.
- `farik_s_client_id_on_its_own_host_uses_the_table`: with `oauth.client_id` set to the table's `client_id` on the entry's URL, the device flow runs and the fixture saw no `/.well-known/` request.
- `a_server_s_own_client_id_wins`: with another `oauth.client_id` set, the matching host runs step 03's discovery and the fixture's `/register` is not called.
- `an_unmatched_host_runs_step_03`: a URL outside the table signs in by DCR as step 03's test does, `app: None`.

- [ ] `feat(runtime): sign in with Farik's own GitHub App`

### Task 4: Refreshing an app's grant

Files: `sign_in.rs`, `fixture_oauth.rs`.

- `refreshes_a_device_grant_without_resource_or_secret`: a grant with `app: Some("dev")` refreshes; `/token` got `client_id`, `grant_type=refresh_token` and `refresh_token`, no `resource`, no `client_secret`, and `Accept: application/json`.
- `a_github_refresh_refused_at_200_lapses`: `/token` answering `200 {"error":"bad_refresh_token"}`, or `200 {"error":"incorrect_client_credentials"}`, to that grant gives `Lapsed`; answering `200 {"error":"something_else"}` gives `Failed`.
- `a_stored_grant_reads_without_app`: step 03's stored form, with no `app`, loads as `app: None`, and its refresh still sends `resource`.
- `a_grant_without_revocation_is_not_revoked`: `revoke` of a device-flow grant makes no request.

- [ ] `feat(runtime): refresh a grant from Farik's own apps`

### Task 5: The daemon and the command line

Files: `daemon.rs`, `daemon/team.rs`, `rpc.schema.json`, `mapping.ts`, `cli/src/connector.rs`. The daemon reads its table from `set_registered_apps`, else `REGISTERED_APPS`; its tests call `set_registered_apps` with the fixture's leaked table. `connect` passes `REGISTERED_APPS` to `connect_with`; the CLI's tests pass the fixture's.

- `sign_in_answers_the_provider_and_code`: against the Device entry, `connector.sign_in` answers `provider: "Dev"`, `user_code` and `install_url`, and `connector.sign_in_status` reaches `signed_in`.
- `a_new_device_attempt_ends_the_old`: after a second `connector.sign_in` for the same agent and server, the fixture sees no further poll carrying the first attempt's `device_code`.
- `team_get_names_the_provider`: a kept Device grant's row has `provider: "Dev"`; a step 03 grant's row has none.
- `mapping_names_the_new_fields`: `providerName`, `userCode` and `installUrl` map both ways (Vitest).
- `farik_connect_prints_the_device_code`: prints `Open <verification_uri> and enter the code <user_code>.`, then the warning sentence, then `Signed in to Dev.`.

- [ ] `feat(runtime): offer Farik's own apps from the web app and the command line`

### Task 6: The screens

Files: `ConnectorAdd.tsx`, `AgentEdit.tsx`, `connectors.test.tsx`, `strings/en.ts`. Built from Task 1's approved boards.

- `connector_add_signs_in_with_github_by_a_code`: the code shows, with the warning "Only enter a code that this page shows you. Farik never sends you a code in a chat."; "Open github.com/login/device" calls `window.open(authorize_url, '_blank', 'noopener')` synchronously in the click; "Copy the code" writes the code to the clipboard; on `signed_in`, the install line and its link to the answer's `installUrl` show.
- `connector_add_names_the_provider`: "Sign in with GitHub" when `provider` is set, and no "for <host>" line.
- `agent_edit_names_the_provider_and_github_s_settings`: a row with `provider: "GitHub"` reads "Signed in to GitHub", and its Remove confirmation is the no-revocation sentence naming GitHub's settings.

- [ ] `feat(web): sign in with GitHub`

### Task 7: Farik's GitHub App, live

Gate: the founder's actions below are done, and the founder gives the GitHub App's client id and slug in conversation. The executor commits them; it never signs in to the founder's accounts.

Files: `registered_apps.rs` (the entry: `github`, `GitHub`, host `api.githubcopilot.com`, Device at `https://github.com/login/device/code` with `verification_uri` `https://github.com/login/device`, issuer `https://github.com/login/oauth`, token `https://github.com/login/oauth/access_token`, no revocation, `install_url` `https://github.com/apps/<slug>/installations/new`), `docs/SPEC.md` (6.7: Farik's registered apps and the device flow; 8.6: no secret shipped, the host binding, Farik's ids only on their own hosts, the code warning; F9: `provider`, `user_code`, `install_url`), `docs/plans/project-plan.md` (row 03b, corrected if execution changed it), `docs/design/role-kits.md` (its steps table).

- `the_shipped_table_names_github`: one entry, id `github`, `client_id` non-empty, every endpoint and `install_url` `https`.

- [ ] `feat(runtime): ship Farik's GitHub client id`

Founder's actions (none is an agent's; no agent creates an account or holds the founder's credentials):
- [ ] **The GitHub App**, under the founder's account or organisation: name "Farik"; homepage `https://github.com/abdshaat/Farik`, the repository's page (GitHub accepts any address, and nothing in this phase needs Farik's own domain; the website may replace it after phase 11); Callback URL empty and "Request user authorization (OAuth) during installation" off, so no web flow exists to misuse with the id; "Enable Device Flow" on; "Expire user authorization tokens" left on; webhook off; repository permissions Metadata read, Contents read, Issues read, Pull requests read, nothing else; account permissions none; installable by any account. Give the client id and the slug. No client secret is generated for Farik's use.
- [ ] **A GitHub account with no paid Copilot** for the live check (ADR 0035's amendment: whether GitHub's remote MCP server needs a Copilot licence is unconfirmed).

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```

The founder's live check, recorded in the pull request: from the web app, as one agent, with the account that has no paid Copilot, sign in to `https://api.githubcopilot.com/mcp/` with GitHub, list its tools, install Farik on one private repository and run one session that reads it, then Remove and read the settings sentence. Record whether the sign-in and the tool listing worked without Copilot; if they did not, GitHub's route goes back to the planner before step 06.
