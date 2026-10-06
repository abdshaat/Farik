# Phase 7, step 03b: Farik's registered apps (GitHub)

Status: executed 2026-10-06 (Tasks 1 to 6; Task 7, Farik's GitHub App, waits for the founder); reviewed 2026-10-06 (one landing review, one fix report).
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.6; F9
Depends on: step 03 of this phase (committed before this step starts: `start_sign_in`, `SignIn`, `OAuthGrant`, `refreshed`, `revoke`, `SignInError`, the loopback listener, `connector.sign_in` and `connector.sign_in_status`, `SignedIn`, `read_kept`, `connect_with`, `ConnectorAdd`'s sign-in)
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 5 Blocking, all folded with the founder's decisions; no second round (ADR 0032)
Mockups approved by: the founder, 2026-10-06, as drawn (`docs/design/mockups/GitHubSignIn.dc.html`, `PhoneGitHubSignIn.dc.html`, on the canvas's "connectors" page; drawn as new boards rather than states added to `ConnectorAdd.dc.html` and `AgentEdit.dc.html`)
Decided by the founder, 2026-10-06, with the boards: (1) Remove's confirmation links straight to the page where Farik is removed at the service, so each `RegisteredApp` gains `settings_url` (GitHub `https://github.com/settings/apps/authorizations`, Google `https://myaccount.google.com/connections`) and Task 6 shows it as a link; (2) the "Run by Farik" label beside Farik's own connectors stays; (3) the boards' extra line under GitHub's button, "Farik shows you a short code to type on GitHub's page.", and the Remove confirmation's second line are approved with them. Task 6 also names the provider, not the kit title, in a kit row's "Signed in to <provider>." and in Remove's words for a connector with no web address.
Amended 2026-10-06 by ADR 0043 (the founder: until phase 15, every login is the customer's): no client id of Farik's is committed. Task 7 is superseded: its founder's actions become the GitHub how-to that step 03f ships, the customer registers the GitHub App under their own account, and the code's flow is unchanged with the customer's client id. The live check below is the founder's as a customer, after step 03f.

## Goal

After step 03, GitHub answers "doesn't let Farik sign in by itself yet", because it registers no client automatically. When this step is done, a user gives an agent GitHub's MCP server (`https://api.githubcopilot.com/mcp/`) by pressing "Sign in with GitHub", typing the short code Farik shows on GitHub's page, and saying yes. Farik uses its own GitHub App, registered once by the founder, and keeps, refreshes and hands over the grant exactly as step 03 does (amended 2026-10-06 by ADR 0043: the customer's own GitHub App, which step 03f lets the customer give Farik). Out of scope: Slack, whose sign-in relay is deferred until after the launch, phase 13 (the founder, 2026-10-02) and which takes a pasted key until then; choosing which GitHub server each kit uses (steps 06 and 07); and Google, deferred until after the launch with Drive (ADR 0035's amendment of 2026-10-02): at launch Google Analytics, the one Google service a kit ships, has no remote MCP server (Google's `analytics-mcp` runs locally), so step 06 connects it as a local server with a key, Notion covers product docs, and nothing before the launch needs Farik's Google app.

## Decisions

ADR 0035 records the routes and, in its amendment, the reviewed facts this plan rests on; this plan builds route 2 for GitHub.

The table:
- **`REGISTERED_APPS` is a static list in `crates/runtime/src/registered_apps.rs`**, one `RegisteredApp` per app: its id (`github`), the name the screens show (`GitHub`), the one host it serves, its flow, its public `client_id`, its issuer, its token endpoint, its revocation endpoint (none for GitHub) and its install address. Rejected: discovering the app from the server's metadata, because a server could name GitHub's authorization server and receive a token from Farik's app; a token sent to a host the app does not serve is the one thing this table prevents.
- **Matched by the server's URL alone**, before step 03's discovery. Parsed with `url::Url` (already in the lock through `reqwest` and `rmcp`; it becomes a direct dependency of `farik-runtime`). The host is `host_str()`, compared to the entry's host ignoring ASCII case; a trailing dot does not match. The scheme is `https` on port 443 (`port_or_known_default() == Some(443)`), or, as step 03's https-or-loopback rule allows for the test fixture, `http` on a loopback host at any port. A URL with a username or a password matches nothing. GitHub's host is exactly `api.githubcopilot.com`.
- **Farik's client ids are used only on their own hosts.** When the team file's `oauth.client_id` equals a table entry's `client_id` and the URL does not match that entry, signing in is refused `Failed("this sign-in is only for <name>'s own servers")` (the RPC's `sign_in_failed`) before any request. When it matches, the table entry is used, as if no `client_id` were given. Any other `oauth.client_id`, or no match, runs step 03 unchanged.
- **The table ships empty** until Task 7, which adds the GitHub entry with the founder's client id. Every test before it passes a table of the test's own: `start_sign_in` and `connect_with` take the table as a parameter, and the daemon reads it from `DaemonState` (Interfaces). Tests build their table with `Box::leak` for the fixture's addresses; the leak is per test and accepted.
- **The client id is public and committed (amended 2026-10-06 by ADR 0043: not before phase 15; the customer's own, kept on their computer, step 03f). No client secret is shipped or sent:** GitHub's device flow needs none, for the sign-in or the refresh.

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

- [x] `docs(design): mock up signing in with GitHub`

### Task 2: The table

Files: `registered_apps.rs`, `lib.rs`, `crates/runtime/Cargo.toml` (`url`, at the version already in the lock). Produces `RegisteredApp`, `AppFlow`, `REGISTERED_APPS` (empty), `app_for`.

- `matches_github_by_its_exact_host`: a table with a GitHub-shaped entry (host `api.githubcopilot.com`). Match: `https://api.githubcopilot.com/mcp/`, `https://API.githubcopilot.com/mcp/`, `https://api.githubcopilot.com:443/mcp/`. No match: `https://api.githubcopilot.com.evil.example/mcp/`, `http://api.githubcopilot.com/mcp/`, `https://api.githubcopilot.com:8443/mcp/`, `https://api.githubcopilot.com./mcp/`, `https://api.githubcopilot.com@evil.example/mcp/`, `https://user@api.githubcopilot.com/mcp/`, and a string that is not a URL.
- `matches_a_loopback_fixture_over_http`: `http://127.0.0.1:4000/mcp` matches an entry with host `127.0.0.1`; `http://10.0.0.1:4000/mcp` does not match an entry with host `10.0.0.1`.
- `the_shipped_table_is_empty_until_the_founder_registers`: `REGISTERED_APPS.is_empty()`. Task 7 replaces this test with `the_shipped_table_names_github`.

- [x] `feat(runtime): name the apps Farik registers with a service`

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

- [x] `feat(runtime): sign in with Farik's own GitHub App`

### Task 4: Refreshing an app's grant

Files: `sign_in.rs`, `fixture_oauth.rs`.

- `refreshes_a_device_grant_without_resource_or_secret`: a grant with `app: Some("dev")` refreshes; `/token` got `client_id`, `grant_type=refresh_token` and `refresh_token`, no `resource`, no `client_secret`, and `Accept: application/json`.
- `a_github_refresh_refused_at_200_lapses`: `/token` answering `200 {"error":"bad_refresh_token"}`, or `200 {"error":"incorrect_client_credentials"}`, to that grant gives `Lapsed`; answering `200 {"error":"something_else"}` gives `Failed`.
- `a_stored_grant_reads_without_app`: step 03's stored form, with no `app`, loads as `app: None`, and its refresh still sends `resource`.
- `a_grant_without_revocation_is_not_revoked`: `revoke` of a device-flow grant makes no request.

- [x] `feat(runtime): refresh a grant from Farik's own apps`

### Task 5: The daemon and the command line

Files: `daemon.rs`, `daemon/team.rs`, `rpc.schema.json`, `mapping.ts`, `cli/src/connector.rs`. The daemon reads its table from `set_registered_apps`, else `REGISTERED_APPS`; its tests call `set_registered_apps` with the fixture's leaked table. `connect` passes `REGISTERED_APPS` to `connect_with`; the CLI's tests pass the fixture's.

- `sign_in_answers_the_provider_and_code`: against the Device entry, `connector.sign_in` answers `provider: "Dev"`, `user_code` and `install_url`, and `connector.sign_in_status` reaches `signed_in`.
- `a_new_device_attempt_ends_the_old`: after a second `connector.sign_in` for the same agent and server, the fixture sees no further poll carrying the first attempt's `device_code`.
- `team_get_names_the_provider`: a kept Device grant's row has `provider: "Dev"`; a step 03 grant's row has none.
- `mapping_names_the_new_fields`: `providerName`, `userCode` and `installUrl` map both ways (Vitest).
- `farik_connect_prints_the_device_code`: prints `Open <verification_uri> and enter the code <user_code>.`, then the warning sentence, then `Signed in to Dev.`.

- [x] `feat(runtime): offer Farik's own apps from the web app and the command line`

### Task 6: The screens

Files: `ConnectorAdd.tsx`, `AgentEdit.tsx`, `connectors.test.tsx`, `strings/en.ts`. Built from Task 1's approved boards.

- `connector_add_signs_in_with_github_by_a_code`: the code shows, with the warning "Only enter a code that this page shows you. Farik never sends you a code in a chat."; "Open github.com/login/device" calls `window.open(authorize_url, '_blank', 'noopener')` synchronously in the click; "Copy the code" writes the code to the clipboard; on `signed_in`, the install line and its link to the answer's `installUrl` show.
- `connector_add_names_the_provider`: "Sign in with GitHub" when `provider` is set, and no "for <host>" line.
- `agent_edit_names_the_provider_and_github_s_settings`: a row with `provider: "GitHub"` reads "Signed in to GitHub", and its Remove confirmation is the no-revocation sentence naming GitHub's settings.

- [x] `feat(web): sign in with GitHub`

### Task 7: Farik's GitHub App, live

Superseded 2026-10-06 by ADR 0043: nothing below is committed before phase 15. The settings the founder's actions list are the how-to step 03f ships for the customer's own GitHub App, and step 03f's verification runs this step's live check with an app the founder registers as a customer.

Gate: the founder's actions below are done, and the founder gives the GitHub App's client id and slug in conversation. The executor commits them; it never signs in to the founder's accounts.

Files: `registered_apps.rs` (the entry: `github`, `GitHub`, host `api.githubcopilot.com`, Device at `https://github.com/login/device/code` with `verification_uri` `https://github.com/login/device`, issuer `https://github.com/login/oauth`, token `https://github.com/login/oauth/access_token`, no revocation, `install_url` `https://github.com/apps/<slug>/installations/new`), `docs/SPEC.md` (6.7: Farik's registered apps and the device flow; 8.6: no secret shipped, the host binding, Farik's ids only on their own hosts, the code warning; F9: `provider`, `user_code`, `install_url`), `docs/plans/project-plan.md` (row 03b, corrected if execution changed it), `docs/design/role-kits.md` (its steps table).

- `the_shipped_table_names_github`: one entry, id `github`, `client_id` non-empty, every endpoint and `install_url` `https`.

- [ ] `feat(runtime): ship Farik's GitHub client id`

Founder's actions (none is an agent's; no agent creates an account or holds the founder's credentials):
- [ ] **The GitHub App**, under the founder's account or organisation: name "Farik"; homepage `https://github.com/abdshaat/Farik`, the repository's page (GitHub accepts any address, and nothing in this phase needs Farik's own domain; the website may replace it once phase 11 step 01 is live); Callback URL empty and "Request user authorization (OAuth) during installation" off, so no web flow exists to misuse with the id; "Enable Device Flow" on; "Expire user authorization tokens" left on; webhook off; repository permissions Metadata read, Contents read, Issues read, Pull requests read, nothing else; account permissions none; installable by any account. Give the client id and the slug. No client secret is generated for Farik's use.
- [ ] **A GitHub account with no paid Copilot** for the live check (ADR 0035's amendment: whether GitHub's remote MCP server needs a Copilot licence is unconfirmed).

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```

The founder's live check, recorded in the pull request, run after step 03f by the founder as a customer, with a GitHub App of the founder's own set up in Settings (ADR 0043, 2026-10-06): from the web app, as one agent, with the account that has no paid Copilot, sign in to `https://api.githubcopilot.com/mcp/` with GitHub, list its tools, install Farik on one private repository and run one session that reads it, then Remove and read the settings sentence. Record whether the sign-in and the tool listing worked without Copilot; if they did not, GitHub's route goes back to the planner before step 06.

## Execution notes

Corrections against the code, read at HEAD `38136d5` on 2026-10-06 before Task 2 (the plan was reviewed against the code of 2026-10-02; steps 03 to 08d have landed since). The plan's intent holds in every case; nothing here needed a decision.

- **The fixture's file.** The plan's file map and Task 3 and 4 put "the device endpoints, request recording" in `crates/runtime/tests/fixture_oauth.rs`. That file holds the sign-in tests; the fixture is `crates/runtime/tests/support/oauth_fixture.rs` (`Fixture`, `Flags`, `Recorded`), which the runtime's own unit tests (`lib.rs`), `fixture_oauth.rs` and `crates/cli/tests/{connector,live_claude}.rs` each include by `#[path]`. Tasks 3 and 4 change the support file, using only what every includer has (`axum`, `rmcp`, `reqwest`, `base64`, `sha2`, `tokio`); `Recorded` gains `headers`.
- **`OAuthGrant` has no `serde` derive.** The plan writes `#[serde(default)] pub app: Option<String>`; at HEAD the stored form is hand-written, `to_json` and `from_json` in `sign_in.rs`. `app` is written there and read there, `None` when absent, so `a_stored_grant_reads_without_app` is a unit test in `sign_in.rs`'s `tests` over `from_json` (it is `pub(crate)`). Outside `sign_in.rs` the grant is built with a struct literal in seven places (`daemon.rs`, `daemon/own_calls.rs`, `daemon/team.rs`, `connectors.rs`, `orchestrator/session.rs`, `tests/fixture_oauth.rs`, `cli/tests/live_claude.rs`, all but the two last in unit tests), each of which gains `app: None`.
- **Where the sign-in lives in the daemon.** The plan puts `begin_sign_in`'s changes in `daemon.rs` and `daemon/team.rs`. `begin_sign_in`, `Attempt`, `Binding` and `sign_in_status` are in `daemon/signed_in.rs` (`begin_sign_in` at :318, returning `(attempt, authorize_url, issuer)`); `connector_sign_in` (team.rs:873) builds the answer; `team.get`'s connector rows are built at team.rs:~405 to ~417; `SignedIn` is at daemon.rs:192 and `read_kept` sets it at :387. `DaemonState` is built in two constructors (`new` and the setup one, daemon.rs:246 and :269), each of which gains the `OnceLock`.
- **`refreshed` is unchanged in its signature** (`grant, now, valid_for, timeout`), and its one caller is `daemon/signed_in.rs:158`. It already reads the answer's `error` before the status, so only `Accept: application/json` and the two new lapsing codes change there. `Guarded::post_form` (sign_in.rs:209) is the one place a form is posted, for the refresh and for `revoke`, so `Accept` goes there.
- **`mapping.ts` is a generic key-case mapper** (`toCamel`, `toSnake`): the wire's `provider` stays `provider` (the plan's Interfaces say `provider?`; its Task 5 test line says `providerName`, which the wire never has), and `user_code`, `install_url` and `settings_url` map with no code change. `mapping_names_the_new_fields` is therefore a test that passes when written; the typed contract is `rpc.schema.json`, which the daemon's own tests check every answer against (`conforms`), and those fail until the schema has the fields.
- **`KitConnect.tsx` also signs in** (`apps/web/src/pages/KitConnect.tsx`, step 05: a kit service whose `auth` is `oauth`), beside `ConnectorAdd.tsx`; Task 6's file list names only `ConnectorAdd` and `AgentEdit`. It reads `connector.sign_in`'s `issuer`; it is taken in Task 6 where the founder's decisions of 2026-10-06 say a kit row names the provider.
- **The founder's `settings_url` reaches the page.** Decision (1) of 2026-10-06 has Remove's confirmation link to the table's `settings_url`, so the page must be told it: `team.get`'s connector rows gain `settings_url?` beside `provider?`, and `SignedIn` gains `settings_url: Option<String>` beside `provider`. `RegisteredApp.settings_url` is a `&'static str` (every app has a page where Farik is removed).
- **`callback_addr`** is called only by `fixture_oauth.rs` (four calls); no other crate calls it. `start_sign_in` is called by `daemon/signed_in.rs:339` and `cli/src/connector.rs:416`; `connect_with` has one caller (`connect`) and the CLI's tests call it through `connect`'s fixture path (`cli/tests/connector.rs`).
- **`reqwest::Url` is `url::Url`.** `sign_in.rs` already uses it through `reqwest`; `url` still becomes a direct dependency, as the plan says, for `registered_apps.rs`.

Task 2: RED was a compile failure (`RegisteredApp`, `AppFlow` and `app_for` did not exist), then, with the types and a stub that matched nothing, the two matching tests failed on their first address. New dependency: `url` `=2.5.8` (workspace `Cargo.toml`, `url.workspace = true` in `farik-runtime`), licence `MIT OR Apache-2.0` (read in the registry's `Cargo.toml`), already locked through `reqwest` and `rmcp`, so the build gains no crate; `Cargo.lock` gains the one edge. `RegisteredApp` also has `settings_url: &'static str` (the founder's decision of 2026-10-06); it derives `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`, as every field is `'static`. `registered_apps` is not `#[cfg(unix)]` (it does no I/O); `sign_in` is. Guards, each mutation reverted and failing `matches_github_by_its_exact_host` unless noted: any port on `https`; `http` served anywhere (also fails `matches_a_loopback_fixture_over_http`); every IPv4 host loopback (`matches_a_loopback_fixture_over_http`); every name loopback; no userinfo check; a username-only check and a password-only check (two cases added beyond the plan's list, `https://:secret@api.githubcopilot.com/mcp/` and a table entry written in capitals, because a username-only check and a case-sensitive comparison survived the plan's list); a case-sensitive comparison.

Task 3: RED was a compile failure (`start_sign_in` took three arguments, and `user_code`, `provider`, `install_url`, `OAuthGrant.app` and `registered_apps` use did not exist); the stored form's `app` was backed out and written again after its test to watch that test fail (`a_grant_keeps_its_app_in_the_stored_form`: `app: None` read back for `Some("github")`).

What changed. `SignIn` is now a `Way` (`Redirect`, step 03's, or `Device`) and a `Setup` (what the grant is made from: the guarded client, the page, issuer, resource, client id, endpoints, the app, the start times); `finish` destructures the two, so a device sign-in polls inside the future `finish` returns and dropping it stops the polling. `complete`'s token reading is `Setup::grant_from`, shared by both. `start_sign_in(url, settings, apps, now)` refuses an app's client id on another address before any request, runs the device flow for an address the table serves when the team file gives no client or the app's own, and else step 03's (`start_redirect`, the old `start`). `Guarded::post_token_form` is the form POST that asks for `Accept: application/json`; `post_form` (for `revoke`) is unchanged. A device grant has empty `scopes` whatever the service says, no revocation endpoint, `resource` the server's address, `app` the app's id. Until Task 5, `daemon/signed_in.rs` and `cli/src/connector.rs` pass `REGISTERED_APPS`; the seven grant literals gained `app: None`. The device interval is the service's, else 5 s, and never under 1 s (a service answering `interval: 0` would otherwise be asked in a loop; added beyond the plan).

The fixture. All in `tests/support/oauth_fixture.rs`: `Recorded` gains `headers` (lower-case names) and `at` (the receipt on the test's clock); `Fixture::seen()` is every request in order; `POST /device/code` answers `dc-<n>` with `WDJB-<n>` and `interval` from the flag; a device poll at `/token` answers pending, `slow_down` once, an error from `device_error`, or approval (`bearer`, a refresh token, `scope: "read"`, which Farik ignores); every device answer is status 200, form-encoded without `Accept: application/json`. New flags: `device_pending`, `device_slow_down`, `device_error`, `device_verification_uri`, `device_interval`.

**The paused clock.** On a paused clock the runtime jumps to its next timer whenever every task waits for the network, and reqwest's request timeout is 30 s, so each poll "took" 28 to 33 s of the test's time (read from `Recorded.at`): a test of "no sooner than 6 s" would have passed with or without `slow_down`. The device tests therefore pause the clock and move it by hand, 10 ms at a time (`stepped`: a loop of `tokio::time::advance` and a yield, which lets the network run without the runtime ever idling); the ten-minute case steps 100 ms. They were run three times in a row.

Tests beyond the plan's list: `waits_five_seconds_when_the_service_names_no_interval` (the first poll comes after the service's interval, else 5 s), `never_asks_faster_than_once_a_second`, a case in `device_flow_reports_denied_and_expired` for any other code (`Failed`, after one poll), `slows_down_when_asked` runs three polls and checks both gaps (the five seconds stay for every later poll), and `a_grant_keeps_its_app_in_the_stored_form` (a unit test in `sign_in.rs`, since `from_json` is `pub(crate)`).

Guards, each mutation reverted and failing the test named: `slow_down` adding 1 s, ignored, or only for the next wait (`slows_down_when_asked`); the default interval 1 s or the minimum removed (`waits_five_seconds_…`, `never_asks_faster_…`); no `Accept` header (seven tests); the verification page unchecked (`refuses_an_unexpected_verification_page`); Farik's client id allowed on another address (`farik_s_client_id_is_refused_elsewhere`); a given client id not winning (`a_server_s_own_client_id_wins`); a `scope` sent to `/device/code` and the service's scope kept (`signs_in_with_the_device_flow`); `access_denied`, `expired_token` and any other code mapped wrongly (`device_flow_reports_denied_and_expired`); the app's id dropped from the grant (two tests); `resource` the wrong address; polling detached from `finish` (`dropping_a_device_attempt_stops_polling`); `app` left out of the stored form (the unit test).

Task 4: RED was two assertions: the device grant's refresh sent `resource` and no `Accept`, and `bad_refresh_token` at status 200 was `Failed`. `a_grant_without_revocation_is_not_revoked` and `a_stored_grant_reads_without_app` pass when written (`revoke` already returns when there is no endpoint; an absent `app` already reads as none): they are guards, and their proof is a mutation. The second is a unit test in `sign_in.rs`, since `from_json` is `pub(crate)`; its "refresh still sends `resource`" half is `refresh_sends_the_kept_resource`, whose grant has `app: None`. `refreshed`'s signature is unchanged (step 03d and 08e add the table to it). It posts through `post_token_form` for every grant, leaves `resource` out when `app` is set, never sends a client secret, and `ENDED` gains `bad_refresh_token` and `incorrect_client_credentials`; the error was already read before the status, so a GitHub answer of 200 with an `error` is read as one.

`a_github_refresh_refused_at_200_lapses` also tries a grant with no `app`, since the plan applies the two codes to every grant. Guards, each mutation reverted and failing the test named: `resource` always sent and never sent (`refreshes_a_device_grant_…`, `refresh_sends_the_kept_resource`); the refresh not asking for JSON (`refreshes_a_device_grant_…`); `bad_refresh_token` and `incorrect_client_credentials` each left out of the lapsing codes (`a_github_refresh_refused_at_200_lapses`); an `error` read only at a failing status (the same); `revoke` falling back to the token endpoint (`a_grant_without_revocation_is_not_revoked`); `app` read as `Some("")` when absent (`a_stored_grant_reads_without_app`).

Task 5: RED was a compile failure (`DaemonState::set_registered_apps` and `CliIo::registered_apps` did not exist); `mapping_names_the_new_fields` passes when written, since `mapping.ts` is generic (the correction above), so it guards the contract and its proof is that the three daemon tests would fail against a schema without the fields (`conforms`, and a mutation that removed `user_code` from the schema).

What changed. `DaemonState` has a `registered_apps` `OnceLock` (both constructors), `set_registered_apps` (true once, like `set_state_dir`) and `registered_apps()` (the set table, else `REGISTERED_APPS`); `begin_sign_in` signs in with it and answers a `Started` struct in place of its 3-tuple; `connector_sign_in` adds `provider`, `user_code` and `install_url` only when there are some; `read_kept` looks a grant's `app` up by id in the daemon's table, and `team.get`'s rows gain `provider` and `settings_url` for a kept grant whose app the table has (a step 03 grant's row has neither, which the existing exact-row tests assert). `SignedIn` is no longer `Copy` (it holds two `String`s), so `Kept::runs` reads it through `as_ref()`. The schema gains the five fields, optional, on `connectorSignInResult` and the `team.get` connector row; the generated TypeScript is not committed (`pnpm -r --if-present generate`, then `typecheck`, pass). The command line prints, for an app's code, `Open <verification_uri> and enter the code <user_code>.`, the warning sentence, opens the page, and ends `Signed in to <provider>.`; for any other sign-in nothing changes.

Deviations. (1) The plan has `connect` pass `REGISTERED_APPS` and "the CLI's tests pass the fixture's", but `connect_with` is `pub(crate)` and the CLI's tests run the command through `run_with`, so `CliIo` gained `registered_apps: &'static [RegisteredApp]`, `REGISTERED_APPS` in `CliIo::new`, which `connect` hands to `connect_with(.., open, apps)` as the plan's signature has it. (2) `keep_sign_in` would have had eight arguments, so `open` and `apps` travel as one `SignInWith`. (3) The daemon's tests run on real time, with the fixture's one-second `interval`, so each takes one to four seconds; `Signing::reply` now gives the daemon thirty seconds to answer, because the first run of the mutation "a new attempt does not abort the old task" passed: `begin_sign_in` then waits for the old attempt's task, which ends only at the ten-minute window, and the test waited ten minutes and went on.

Guards, each mutation reverted and failing the test named: the daemon's table ignored, and `begin_sign_in` passed the shipped table (`sign_in_answers_the_provider_and_code`, `a_new_device_attempt_ends_the_old`, `team_get_names_the_provider`); the provider and the settings page left out of `SignedIn` (`team_get_names_the_provider`); `user_code` and `install_url` left out of the answer, and `user_code` left out of the schema (`sign_in_answers_the_provider_and_code`); `provider` written for every grant (`sign_in_then_connect_keeps_the_grant` and the others that compare whole rows); the old attempt's task not aborted (`a_new_device_attempt_ends_the_old`, after the time limit); the command line naming the issuer in place of the provider, leaving out the warning, and ignoring `CliIo::registered_apps` (`farik_connect_prints_the_device_code`).

Not built, and for the planner: `KitConnect.tsx` (a kit service whose server is GitHub's) still opens the page and waits, with no code shown. No kit connects GitHub until steps 06 and 07, and the table ships empty, so nothing reaches it yet; when a kit does, `KitConnect` needs this step's code board, which the boards do not draw. And `connector.sign_in_status`'s words for a failed device sign-in name the issuer's host (`github.com`), as the old ones did.

Task 6: RED was five assertions on the new tests, each for the right reason (the page named the issuer's host, `github.com`, and the address's host, where the boards name GitHub). The tests were written first, parked as a patch while Tasks 4 and 5 landed, and applied here.

What changed. `ConnectorAdd`: `connector.sign_in`'s `provider`, `userCode` and `installUrl` make an `app` of the sign-in state, carried from offered through waiting to signed in or failed. Where there is one, the page says "<Provider> lets you sign in." and "Sign in with <Provider>" with no "for <host>" line and "Farik shows you a short code to type on <Provider>’s page."; the button shows the code board (it opens nothing, as the board says, and the page's own 2-second polling of `connector.sign_in_status` is unchanged); the board holds "Enter this code on <Provider>:", the code in the code face at 32 px, the warning "Only enter a code that this page shows you. Farik never sends you a code in a chat.", "Copy the code" (`navigator.clipboard.writeText`), "Open <page>" (the page's host and path, `github.com/login/device`; `window.open(address, '_blank', 'noopener')` in the click), and "Waiting for <Provider>…"; signed in says "Signed in to <Provider>." and, with an `installUrl` that is `https`, "To let <Name> read private repositories, install Farik on them on <Provider>." and the link "Install Farik on <Provider>" (`target="_blank"`, `rel="noopener noreferrer"`). The three states are in the card the boards draw (`SigningIn`, a `section` named "Signing in"); a service that signs in by itself has none, as before. A failed sign-in names the provider. `AgentEdit`: a custom row says "Signed in to <Provider>" (else the host); a kit row's "Signed in to <Provider>." names the provider, not the kit's title; Remove's words for a sign-in Farik cannot ask the service to forget name the provider, else the address's host, else the connector's own name (a connector with neither), and "<name>’s settings" is a link to the row's `settingsUrl` when it is `https`, in a new tab with `noopener noreferrer`, else plain text as before (the two sentences are now `…also remove it in {settings}.`, with `connectorSettings` the words of the link). `ConnectorState` gains `provider` and `settingsUrl`.

Tests beyond the plan's three: the failed sign-in names GitHub; a kit row and Remove for a connector with a command and no address (Google Ads, step 08e's) name Google and link Google's page; a connector with neither provider nor address is named by its own name, with no link. axe runs on the offered board, the code board and the signed-in board with the clock running (axe waits on timers, so it cannot run under the fake ones the polling test uses).

Guards, each mutation reverted and failing the test named: opening the page when the button only shows the code; Copy copying nothing; Open doing nothing; the lead naming the address's host; the "for <host>" line kept; the old note kept; the install link's address; the install line dropped; `installUrl` dropped from the state; the failed words naming the issuer; the app lost between waiting and signed in; the row naming the host; Remove naming the address's host or, with none, nothing; the settings page never linked; the kit row naming the kit's title.

Compared with the boards, in Chromium (`PLAYWRIGHT_BROWSERS_PATH=/tmp/claude-0/pw`) at 1440 and 360 px, the built app (`vite build`, served statically) against a faked daemon (Playwright's `routeWebSocket` answering `subscribe`, `serve.status`, `team.get`, `models.list`, `skills.list`, `connector.sign_in` with GitHub's answer and `connector.sign_in_status`), against `GitHubSignIn.dc.html` and `PhoneGitHubSignIn.dc.html` boards 1 to 5: offered, waiting with the code, signed in with the install line, Theo’s page with the github row, and Remove github. The words, their order, the controls and which one is primary match on all five at both widths; at 360 px the two code buttons take the full width, as board 2 draws. What differs is not this step's: the shared Dialog and Stepper chrome (a title tile, numbered squares, the dialog's width), the spinner and the check mark before "Waiting…" and "Signed in" (step 03's built screens have none either), the Open button being a button that calls `window.open` where the board draws an anchor (the plan's test asks for the call), and the two-column "How does it start?" choice. Boards 6 and 7, Kai’s Google Ads, are step 08e's and were not built here; the kit row and Remove test above are the ground they stand on. `KitConnect` is not changed (see Task 5's last paragraph).

- **The landing review (2026-10-06, a fresh session, one fix report): lands; mutations caught but one, now caught; fixes F1, F2, R2, R3, Q2, Q3, V.** F1: `expired_token` treated as pending still answered `TimedOut` after the ten minutes, so `device_flow_reports_denied_and_expired` now also counts the polls (one). F2: a refused device sign-in on the command line names the provider, not the address's host. R2: a sign-in the provider ended names the provider in the agent's row, a kit's row and the dialog "Sign in again" opens. R3: `KitConnect` shows the code card, now the shared `CodeCard`, when its service signs in by a code, and opens the page only when pressed. Q2 (the planner's): `connector.sign_in_cancel { attempt }` stops an attempt's polling or listener and drops its grant, sent by Cancel, the dialog's close, "Use a key instead" and a changed address, in `ConnectorAdd` and `KitConnect`. Q3 (the planner's): a failed connection, a reply that is not JSON or a server error with no code is polled again after the interval, and three in a row end the sign-in `Failed`; an address refused as not `https` ends it at once. V: `Dialog` gained `wide`, which `ConnectorAdd` uses (760 px as the boards draw it, where the screen has room), and a phone's sign-in footer puts Cancel above Next, each full width. Compared in Chromium at 1440 and 360 px, offered, waiting and signed in: the dialog was 515, 562 and 562 px wide at 1440 and is 760 throughout; at 360 px Cancel sits above Next, both 310 px wide. `KitConnect` has the code card but neither the 760 width nor the install line (a kit's `signed_in` connects at once), which no board draws.
