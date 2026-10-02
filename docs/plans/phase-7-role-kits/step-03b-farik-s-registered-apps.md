# Phase 7, step 03b: Farik's registered apps (GitHub, Google)

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.6; F9
Depends on: step 03 of this phase (committed before this step starts: `start_sign_in`, `SignIn`, `OAuthGrant`, `refreshed`, `revoke`, `SignInError`, the loopback listener, `connector.sign_in` and `connector.sign_in_status`, `ConnectorAdd`'s sign-in)
Readiness: pending: a readiness review by another Opus session
Mockups approved by: pending (Task 1's gate)

## Goal

After step 03, GitHub and Google answer "doesn't let Farik sign in by itself yet", because neither registers a client automatically. When this step is done, a user gives an agent GitHub's MCP server (`https://api.githubcopilot.com/mcp/`) by pressing "Sign in with GitHub", typing the short code Farik shows on GitHub's page, and saying yes; and a Google MCP server (a host ending in `.googleapis.com`) by pressing "Sign in with Google". Farik uses its own apps, registered once by the founder, and keeps, refreshes and hands over the grant exactly as step 03 does. Out of scope: Slack and any service that needs a client secret (step 03c), choosing which GitHub or Google server each kit uses and its scopes (steps 06 and 07), and Google's verification itself, which is the founder's.

## Decisions

ADR 0035 records the routes; this plan builds route 2. Its research (GitHub's and Google's documents and metadata, read 2026-10-02) is in the ADR.

The table:
- **`REGISTERED_APPS` is a static list in `crates/runtime/src/registered_apps.rs`**, one `RegisteredApp` per app: its id (`github`, `google`), the name the screens show (`GitHub`, `Google`), the hosts it serves, its flow, its public `client_id`, its endpoints, its default scopes, extra authorization parameters, and whether it revokes. Rejected: discovering the app from the server's metadata, because a server could name GitHub's authorization server and receive a token from Farik's app (a token sent to a host the app does not serve is the one thing this table prevents); and because Google's metadata names `https://accounts.google.com/` with a slash its issuer does not have.
- **Matched by the server's URL alone**, before step 03's discovery: `https` on port 443, or, as step 03's https-or-loopback rule allows for the test fixture, `http` on a loopback host at any port; GitHub's host is exactly `api.githubcopilot.com`; Google's is any host ending in `.googleapis.com` (a label before the suffix, so `googleapis.com` itself is no match). Matching ignores case. No match, or a team file that gives its own `oauth.client_id`, runs step 03 unchanged.
- **The table ships empty** until Task 7, which adds the two entries with the founder's client ids. Every test before it passes a table of the test's own; `start_sign_in` takes the table as a parameter.
- **Client ids are public and committed.** No client secret is shipped or sent: Google's desktop client takes PKCE without one; GitHub's device flow needs none.

GitHub (device flow, RFC 8628):
- `POST https://github.com/login/device/code` with `client_id`, `Accept: application/json`; the answer's `device_code`, `user_code`, `verification_uri`, `expires_in` and `interval` (default 5 s). A `verification_uri` other than `https://github.com/login/device` is `Failed`.
- Farik polls `POST https://github.com/login/oauth/access_token` with `client_id`, `device_code` and `grant_type=urn:ietf:params:oauth:grant-type:device_code` every `interval`. `authorization_pending` waits; `slow_down` adds 5 s to the interval; `access_denied` is `Denied("access_denied")`; `expired_token` and the 10-minute window (step 03's `SIGN_IN_WINDOW`, whichever comes first) are `TimedOut`; any other `error` is `Failed`.
- No listener, no `state`, no PKCE: the device code never leaves Farik, and the user code is typed by the person at GitHub. No `scope` is sent: a GitHub App's permissions are set on the app (read-only, Task 7).
- The grant: `issuer` `https://github.com/login/oauth`, `resource` the server's URL, `client_id`, `token_endpoint` the access-token URL, `revocation_endpoint` none (revoking needs the secret), `expires_at` from `expires_in` (8 hours), and the refresh token.
- After signing in, the screens say that a private repository needs Farik installed on it, with a link to `https://github.com/apps/<slug>/installations/new`. The slug is in the table.

Google (authorization code with PKCE):
- Step 03's flow with the table's endpoints instead of discovered ones: `authorization_endpoint` `https://accounts.google.com/o/oauth2/v2/auth`, `token_endpoint` `https://oauth2.googleapis.com/token`, `revocation_endpoint` `https://oauth2.googleapis.com/revoke`, `issuer` `https://accounts.google.com`. Farik builds rmcp's `AuthorizationMetadata` from them and calls `set_metadata`, with the table's `client_id` as the pre-registered client and the redirect on a free loopback port (Google accepts any), as step 03 does for DCR.
- The scopes are the team file's `oauth.scopes`. With none, signing in is refused `scopes_needed` (Google has no useful default; each kit names its narrowest read scope, steps 06 and 07). The command line passes `--scope`; the web app's own connector form has no scope field, so Google is signed in to from a kit, or from `farik connect`.
- The table adds `access_type=offline` to the authorization address, so Google returns a refresh token. Google's `iss` in the callback is checked by step 03's rule when present; the table does not require it.

Both:
- **No `resource` parameter** is sent to either service, on authorization, exchange or refresh: neither takes RFC 8707, and the table's hosts already bind the token. The grant still keeps `resource` (the server's URL) for step 03's bookkeeping.
- **`OAuthGrant` gains `app: Option<String>`**, the table's id, `None` for step 03's grants (serde default, so a stored entry reads unchanged). `refreshed` sends no `resource` when `app` is set, and no client secret ever; `revoke` does nothing when the grant has no `revocation_endpoint` (GitHub), as step 03 already does.
- **The wire.** `connector.sign_in`'s answer gains `provider?` (the table's display name) and `user_code?` (GitHub). For GitHub, `authorize_url` is the `verification_uri`. `issuer` stays. `connector.sign_in`'s refusals gain `scopes_needed`.
- **What a user sees** on `ConnectorAdd`: "Sign in with GitHub" or "Sign in with Google" (the provider's name instead of step 03's issuer host; the "for <host>" line is not shown, since the table fixes the host). For GitHub, a waiting board shows the code large, "Copy the code", and "Open github.com/login/device", which opens the page in a new tab in the click; after signing in, the private-repository line and its link. `AgentEdit`'s row reads "Signed in to GitHub". Remove's confirmation for GitHub is step 03's no-revocation sentence, naming GitHub's settings.
- **The command line** prints `Open https://github.com/login/device and enter the code ABCD-1234.`, opens the page, and waits, as step 03 does for its address.

## File map

```
docs/design/mockups/{ConnectorAdd,AgentEdit}.dc.html, canvas.json        Task 1
crates/runtime/src/registered_apps.rs, lib.rs                          creates: RegisteredApp, AppFlow, the table, app_for (Task 2); the entries (Task 7)
crates/runtime/src/sign_in.rs                                          modifies: the app branch, device flow, OAuthGrant.app, refresh without resource (Tasks 3, 4)
crates/runtime/tests/fixture_oauth.rs                                  modifies: a device endpoint and a no-resource check (Tasks 3, 4)
crates/runtime/src/daemon/team.rs, docs/schemas/rpc.schema.json        modifies: provider, user_code, scopes_needed (Task 5)
packages/protocol-client/src/mapping.ts                                modifies: providerName, userCode (Task 5)
crates/cli/src/connector.rs                                            modifies: the device code line (Task 5)
apps/web/src/pages/{ConnectorAdd,AgentEdit}.tsx, connectors.test.tsx, strings/en.ts   modifies (Task 6)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md     modifies (Task 7)
```

## Interfaces

Consumes: `start_sign_in`, `SignIn`, `OAuthGrant`, `refreshed`, `revoke`, `SignInError`, `SIGN_IN_WINDOW`, `OAuthSettings` and the fixture (step 03); the sign-in RPCs and `ConnectorAdd` (step 03).

Produces:

```rust
// farik-runtime, registered_apps.rs
pub enum AppFlow { Device { device_endpoint: &'static str, verification_uri: &'static str },
                   AuthCode { authorization_endpoint: &'static str, extra_params: &'static [(&'static str, &'static str)] } }
pub enum AppHosts { Exact(&'static str), Suffix(&'static str) }
pub struct RegisteredApp { pub id: &'static str, pub name: &'static str, pub hosts: AppHosts, pub flow: AppFlow,
    pub client_id: &'static str, pub issuer: &'static str, pub token_endpoint: &'static str,
    pub revocation_endpoint: Option<&'static str>, pub install_url: Option<&'static str> }
pub static REGISTERED_APPS: &[RegisteredApp];
pub fn app_for<'a>(apps: &'a [RegisteredApp], url: &str) -> Option<&'a RegisteredApp>;
// farik-runtime, sign_in.rs (changed signatures)
pub async fn start_sign_in(url: &str, settings: &OAuthSettings, apps: &[RegisteredApp], now: DateTime<Utc>)
    -> Result<SignIn, SignInError>;
impl SignIn { pub fn user_code(&self) -> Option<&str>; pub fn provider(&self) -> Option<&str>; }
pub struct OAuthGrant { /* step 03's fields */ pub app: Option<String> }
pub enum SignInError { /* step 03's */ ScopesNeeded }
```

Wire (`snake_case`): `connector.sign_in → { attempt, authorize_url, issuer, provider?, user_code? }`, refused also `scopes_needed`.

## Tasks

### Task 1: The registered-app screens, mocked up

A Sonnet agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job, and copies them to `docs/design/mockups/`.

- **`ConnectorAdd`, GitHub offered.** The address `https://api.githubcopilot.com/mcp/`; "GitHub lets you sign in." and the primary button "Sign in with GitHub"; under it "Use a key instead".
- **`ConnectorAdd`, GitHub waiting.** "Enter this code on GitHub:", the code `WDJB-MJHT` large in the monospace face, "Copy the code", the primary button "Open github.com/login/device", "Waiting for you on GitHub…", and "Cancel".
- **`ConnectorAdd`, GitHub signed in.** "Signed in to GitHub." and the line "To let Theo read private repositories, install Farik on them on GitHub." with the link "Install Farik on GitHub", then Next.
- **`ConnectorAdd`, Google offered and waiting.** "Sign in with Google", and step 03's waiting board with "Google" in place of the host.
- **`AgentEdit`.** A row "github · Reached at a web address · Signed in to GitHub · 9 tools: 9 only read", and its Remove confirmation: "Remove github from Theo? Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in GitHub's settings."

Gate: the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date. Task 6 does not start until then; Tasks 2 to 5 do not depend on the boards.

- [ ] `docs(design): mock up signing in with GitHub and Google`

### Task 2: The table

Files: `registered_apps.rs`, `lib.rs`. Produces `RegisteredApp`, `AppFlow`, `AppHosts`, `REGISTERED_APPS` (empty), `app_for`.

- `matches_github_by_its_exact_host`: a table with a GitHub-shaped entry; `https://api.githubcopilot.com/mcp/` and `https://API.githubcopilot.com/mcp/` match; `https://api.githubcopilot.com.evil.example/mcp/`, `http://api.githubcopilot.com/mcp/` and `https://api.githubcopilot.com:8443/mcp/` do not; `http://127.0.0.1:4000/mcp` matches an `Exact("127.0.0.1")` entry.
- `matches_google_by_its_suffix`: `https://bigquery.googleapis.com/mcp` matches; `https://googleapis.com/mcp`, `https://evilgoogleapis.com/mcp` and `https://bigquery.googleapis.com.evil.example/mcp` do not.
- `the_shipped_table_is_empty_until_the_founder_registers`: `REGISTERED_APPS.is_empty()`. Task 7 replaces this test with `the_shipped_table_names_github_and_google`.

- [ ] `feat(runtime): name the apps Farik registers with a service`

### Task 3: Signing in with Farik's apps

Files: `sign_in.rs`, `fixture_oauth.rs`. Produces the new `start_sign_in`, `SignIn::user_code`, `SignIn::provider`, `OAuthGrant.app`, `SignInError::ScopesNeeded`. The fixture gains `/device/code` and device-code answers at `/token` (flags: pending N times, `slow_down` once, `access_denied`, `expired_token`); the tests' table points an AuthCode entry and a Device entry at the fixture over `http` on loopback, which the https-or-loopback check already allows, with `Exact("127.0.0.1")` hosts.

- `signs_in_with_the_device_flow`: `user_code()` is the fixture's, `authorize_url()` its `verification_uri`; after two `authorization_pending` answers the grant holds both tokens, `app: Some("dev")`, the table's `issuer`, the server URL as `resource`, and no `revocation_endpoint`; `/token` got `grant_type=urn:ietf:params:oauth:grant-type:device_code` and no `client_secret`.
- `slows_down_when_asked`: after `slow_down`, the next poll comes at least `interval + 5` seconds later (paused clock).
- `device_flow_reports_denied_and_expired`: `access_denied` gives `Denied("access_denied")`; `expired_token` gives `TimedOut`; ten minutes of `authorization_pending` gives `TimedOut`.
- `refuses_an_unexpected_verification_page`: a `verification_uri` other than the table's gives `Failed`.
- `signs_in_with_farik_s_desktop_client`: the AuthCode entry runs without any discovery request (the fixture saw no `/.well-known/` request), `/authorize` got the table's `client_id`, `code_challenge_method=S256`, `access_type=offline` and the team file's scopes, and neither `/authorize` nor `/token` got `resource` or `client_secret`.
- `asks_for_scopes_where_the_app_needs_them`: the AuthCode entry with empty `oauth.scopes` gives `ScopesNeeded`, with no request made.
- `a_server_s_own_client_id_wins`: with `oauth.client_id` set, the matching host runs step 03's discovery and the fixture's `/register` is not called.
- `an_unmatched_host_runs_step_03`: a URL outside the table signs in by DCR as step 03's test does, `app: None`.

- [ ] `feat(runtime): sign in with Farik's own GitHub and Google apps`

### Task 4: Refreshing an app's grant

Files: `sign_in.rs`.

- `refreshes_an_app_grant_without_resource_or_secret`: a grant with `app: Some(..)` refreshes; `/token` got `client_id` and `refresh_token`, no `resource`, no `client_secret`.
- `a_stored_grant_reads_without_app`: step 03's stored form, with no `app`, loads as `app: None`, and its refresh still sends `resource`.
- `a_grant_without_revocation_is_not_revoked`: `revoke` of a device-flow grant makes no request.

- [ ] `feat(runtime): refresh a grant from Farik's own apps`

### Task 5: The daemon and the command line

Files: `daemon/team.rs`, `rpc.schema.json`, `mapping.ts`, `cli/src/connector.rs`. The daemon passes `REGISTERED_APPS`; its tests pass the fixture's table through `DaemonState`'s test constructor.

- `sign_in_answers_the_provider_and_code`: against a Device entry, `connector.sign_in` answers `provider` and `user_code`, and `connector.sign_in_status` reaches `signed_in`; against an AuthCode entry, `provider` and no `user_code`.
- `sign_in_refuses_without_scopes`: `scopes_needed`.
- `mapping_names_the_new_fields`: `providerName` and `userCode` map both ways (Vitest).
- `farik_connect_prints_the_device_code`: prints `Open <verification_uri> and enter the code <user_code>.`, then `Signed in to GitHub.` with the fixture's provider name.

- [ ] `feat(runtime): offer Farik's own apps from the web app and the command line`

### Task 6: The screens

Files: `ConnectorAdd.tsx`, `AgentEdit.tsx`, `connectors.test.tsx`, `strings/en.ts`. Built from Task 1's approved boards.

- `connector_add_signs_in_with_github_by_a_code`: the code shows; "Open github.com/login/device" calls `window.open(authorize_url, '_blank', 'noopener')` synchronously in the click; "Copy the code" writes the code to the clipboard; on `signed_in`, the install line and its link to the table's `install_url` show.
- `connector_add_names_the_provider`: "Sign in with Google" when `provider` is set, and no "for <host>" line.
- `connector_add_explains_scopes_needed`: `scopes_needed` shows "This Google service needs its kit, or `farik connect` with `--scope`." over the key fields.
- `agent_edit_names_the_provider_and_github_s_settings`: "Signed in to GitHub", and the no-revocation sentence naming GitHub's settings.

- [ ] `feat(web): sign in with GitHub and Google`

### Task 7: Farik's apps, live

Gate: the founder's actions below are done, and the founder gives the GitHub App's client id and slug and the Google desktop client's id in conversation. The executor commits them; it never signs in to the founder's accounts.

Files: `registered_apps.rs` (the two entries: `github`, `GitHub`, `Exact("api.githubcopilot.com")`, Device at `https://github.com/login/device/code`, issuer `https://github.com/login/oauth`, token `https://github.com/login/oauth/access_token`, no revocation, install URL from the slug; `google`, `Google`, `Suffix(".googleapis.com")`, AuthCode with `access_type=offline`, the three Google endpoints of the Decisions), `docs/SPEC.md` (6.7: Farik's registered apps and device flow; 8.6: no secret shipped, the host binding; F9: `provider`, `user_code`, `scopes_needed`), `docs/plans/project-plan.md` (row 03b, corrected if execution changed it), `docs/design/role-kits.md` (its steps table).

- `the_shipped_table_names_github_and_google`: both entries, each `client_id` non-empty, every endpoint `https`.

- [ ] `feat(runtime): ship Farik's GitHub and Google client ids`

Founder's actions (none is an agent's; no agent creates an account or holds the founder's credentials):
- [ ] **The GitHub App**, under the founder's account or organisation: name "Farik"; homepage the project's GitHub page; "Enable Device Flow" on; "Expire user authorization tokens" left on; webhook off; repository permissions Metadata read, Contents read, Issues read, Pull requests read, nothing else; installable by any account. Give the client id and the slug. No client secret is generated for Farik's use.
- [ ] **The Google Cloud project**, owned by the founder: the OAuth consent screen (External, app name Farik, the founder's support email, the homepage and privacy policy URLs on the founder's verified domain), the scopes the Product Manager's kit names (`analytics.readonly`; `drive.readonly` only if the founder accepts its yearly security assessment), the founder as a test user, and an OAuth client of type "Desktop app". Give its client id. Its client secret is not used.
- [ ] **Google's verification**, submitted by the founder once the scopes are final; until it passes, only test users sign in and their sign-ins end after 7 days.

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```

The founder's live check, recorded in the pull request: from the web app, as one agent, sign in to `https://api.githubcopilot.com/mcp/` with GitHub, list its tools, install Farik on one private repository and run one session that reads it, then Remove and read the settings sentence. Then `farik connect <agent> bigquery --url https://bigquery.googleapis.com/mcp --sign-in --scope https://www.googleapis.com/auth/bigquery` as a Google test user, list its tools, and Remove (the BigQuery scope is added to the consent screen for testing only and removed afterwards).
