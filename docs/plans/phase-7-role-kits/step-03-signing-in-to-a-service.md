# Phase 7, step 03: Signing in to a service

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.2, 8.5, 8.6; F9
Depends on: step 01 of this phase (committed; the custom connector, `ConnectorEntry`, `ConnectorSecrets`, the launch route and the headers helper, `spec_sha256`), step 02 (committed; nothing of it is consumed, it lands first)
Readiness: pending: a readiness review by another Opus session
Mockups approved by: pending (Task 1's gate)

## Goal

After step 01 a web-address connector takes pasted keys only. When this step is done, a user gives an agent a service that offers signing in (Notion, Linear, Stripe, Atlassian, Sentry, PostHog among those checked) by pressing "Sign in with <service>" on `ConnectorAdd`, or with `farik connect … --sign-in`. The service's page opens in the browser, the user says yes there, and Farik keeps that agent's sign-in where step 01 keeps its keys, refreshes it before a session needs it, hands it to the session through the headers helper, and asks the service to forget it on "Remove". A sign-in the service ended shows "Sign in again" on the agent page. Out of scope: kit connectors and their shipped client ids (step 05), a hosted client metadata document (O1), a client secret (Slack's case), and Farik asking for wider scopes when a service answers `insufficient_scope` (the user signs in again).

## Decisions

Research (2026-10-02; MCP specification 2026-07-28, the current one; code.claude.com/docs/en/mcp, undated, read that day; each service's metadata read live that day):
- Claude Code in `-p` cannot sign in ("there's no `/mcp` panel"); it only uses tokens it already holds, keyed by its config folder, not by agent, in one shared credentials blob that concurrent processes overwrite (anthropics/claude-code #65752, 2026-06-05).
- A `headersHelper` whose output holds `Authorization` is used as the server's authentication, and Claude Code then does not try OAuth itself. It runs at session start and on every reconnect, with a 10-second limit.
- DCR is deprecated in 2026-07-28 in favour of client metadata documents (CIMD), but kept. Notion, Linear, Sentry and PostHog offer both; Stripe and Atlassian DCR only; GitHub, Slack and Google neither (Google Cloud's "authenticate MCP" page, updated 2026-09-30).
- `rmcp` 3.3.0, already pinned, has an `auth` feature: discovery (RFC 9728, RFC 8414 and OIDC), DCR, a pre-registered client, PKCE S256, `resource` (RFC 8707), `iss` checked by `handle_callback_with_issuer` (RFC 9207), refresh, and a `CredentialStore` with a refresh guard. It has no loopback listener.

Who does what:
- **Farik runs the sign-in, not Claude Code.** For the web app the daemon runs it, for the command line `farik connect`'s own process, through one function. The tokens go into the agent's `ConnectorEntry`, in the keychain or `connectors.json` (ADR 0030), and reach the session as `Authorization: Bearer <access token>` from the headers helper step 01 built. Rejected: letting Claude Code sign in and keep the tokens. `-p` cannot sign in, its store is not per agent, and the daemon could neither confirm the definition (ADR 0030), refresh before a session, nor show "Sign in again".
- **Through `rmcp`'s `auth` feature** (`OAuthState`, `AuthorizationManager`, `AuthorizationRequest`), enabled on the pinned 3.3.0. It brings `oauth2` 5.0, `async-trait` and `url` in as new transitive dependencies, recorded with their licences in the pull request. Rejected: hand-writing the flow over `reqwest`, which `rmcp` already does.

The team file:
- **An http custom server gains `oauth: { client_id?, callback_port?, scopes? }`.** Its presence means "signed in". Rejected: a separate `auth` field, which would be two fields saying one thing.
  - `client_id` is a pre-registered public client, at most 200 characters. `callback_port`, 1024 to 65535, comes only with `client_id` (`callback_port_without_client`). `scopes`, at most 16, each matching `^[\x21\x23-\x5B\x5D-\x7E]{1,200}$` (RFC 6749's scope characters).
  - `validate_team` refuses `oauth` on stdio (`oauth_on_stdio`), beside `credential_keys` (`oauth_with_keys`), and beside a header named `Authorization` (`oauth_header_conflict`), each at its field.
- **`spec_sha256` hashes `oauth` only when present**, so every entry connected under step 01 keeps its hash. A change to `client_id`, `callback_port` or one scope makes the server "Connect again", as ADR 0030 does for every field.

Registration, in the specification's order less CIMD:
1. `oauth.client_id` when given, with no registration request.
2. Else DCR when the authorization server lists a `registration_endpoint`: `client_name: "Farik"`, `application_type: "native"`, `token_endpoint_auth_method: "none"`, `grant_types: ["authorization_code", "refresh_token"]`, and this sign-in's exact `redirect_uris`. It runs once per sign-in, and the `client_id` is kept with the tokens, since a client is bound to its issuer.
3. Else refused `sign_in_not_supported`.

A client secret is never taken: Farik is a public client. CIMD is not in this step (O1).

Redirect and callback:
- **The redirect is `http://localhost:<port>/callback`**, and the listener binds `127.0.0.1:<port>`, and `[::1]:<port>` when the machine has IPv6, never `0.0.0.0`. `localhost`, not `127.0.0.1`, because authorization servers match a registered URI exactly and Claude Code had to return to `localhost` for that reason (its MCP page, v2.1.229).
- **The port.** With DCR it is one the operating system picks, since the URI is registered for that sign-in. With a pre-registered `client_id` it is `callback_port`, which the service's app registration names, or 33418 when none is given.
- **The listener answers one `GET /callback`** and closes. Any other path is 404 and does not end the attempt. The page it serves says the outcome in a sentence, links nowhere, and runs no script.
- **An attempt lasts 10 minutes**, then fails `sign_in_timed_out`. A new attempt for the same agent and server ends the old one.

Callback security:
- **PKCE S256 always.** Metadata without `S256` in `code_challenge_methods_supported` is refused `pkce_not_supported` before any registration, as the specification requires.
- **`state`** is random, single-use and bound to the attempt. A callback with another `state` fails the attempt `sign_in_mismatch`, and no code is exchanged.
- **`iss`** (RFC 9207): when present, it must equal the metadata's `issuer` by exact string; when absent while the metadata sets `authorization_response_iss_parameter_supported`, it is refused. Both cases fail `sign_in_mismatch`. `rmcp`'s `set_allow_missing_issuer` is set from that metadata field.
- **`resource`** is the server's `url`, sent on the authorization and token requests. The token is sent to that server only, and to no other connector.
- **Endpoints must be `https`.** The authorization, token, registration and revocation endpoints, and the metadata URLs, are refused unless `https`, or `http` on a loopback host, which the test fixture needs (`sign_in_failed: <endpoint> is not https`).
- **Opening the page.** The browser opens it from a click on the page (`window.open`, `noopener`). The command line passes it as one argument to `open` on macOS or `xdg-open` elsewhere, never through a shell, and prints it too.
- **What the specification leaves to the client:** any local process can bind a port and receive a code (its security best practices, "localhost impersonation"). PKCE makes such a code useless without the verifier, which stays in the attempt's memory.

Tokens:
- **Kept in the entry.** `ConnectorEntry` gains `oauth: Option<OAuthGrant>`, stored beside `keys` in the same JSON object. An entry stored before this step, with no `oauth`, loads as `None`. `Debug` shows `***` for both tokens.
- **Refresh, at session setup.** `run_session` refreshes a grant whose access token expires before the session's `max_wall_clock` plus five minutes have passed, or has no expiry and was issued more than 50 minutes ago. The rotated refresh token is saved before the new access token is used, since public clients' refresh tokens rotate (the specification's security considerations).
- **Refresh, at the launch route.** The route refreshes only a token already expired or expiring within 60 seconds, within 3 seconds, inside its 5-second deadline. The helper runs again on every reconnect, which is how a long session gets a new token.
- **One refresh at a time per entry**, under a daemon mutex keyed by `SecretAt` (the `CredentialStore` refresh guard).
- **A refused refresh lapses the grant.** `invalid_grant`, `invalid_client` or `unauthorized_client` sets `lapsed: true` in the entry. The server is then left out of sessions, and `team.get` reports it `sign_in_again`. Any other failure with the access token still valid uses that token; with it expired, the server is left out of that session only.
- **Revocation.** "Remove" (`connector.disconnect`, `farik disconnect`) deletes the entry first. It then sends the refresh token, else the access token, to the metadata's `revocation_endpoint` (RFC 7009), at most 5 seconds, best effort. Removing an agent or a server by a save (`forget_removed_keys`) does the same. With no revocation endpoint, the confirmation says where to remove Farik in the service's own settings.

The protocol:
- **RPCs.** `connector.sign_in { agent, server }` → `{ attempt, authorize_url }`, refused `sign_in_not_offered` (the server answered without a `WWW-Authenticate` naming `resource_metadata`, and neither well-known address of RFC 9728 answers), `sign_in_not_supported`, `pkce_not_supported`, or `sign_in_failed`. `connector.sign_in_status { attempt }` → `{ state: waiting | signed_in | failed, reason? }`, where `reason` is `{ code, message }` (`access_denied`, `sign_in_timed_out`, `sign_in_mismatch`, `sign_in_failed`).
  - `connector.tools` and `connector.connect` take `attempt` in place of `keys` when `server.oauth` is set: without it `sign_in_needed`, an unknown, used, expired or unfinished one `sign_in_unknown`. `connect` uses the attempt up.
  - Attempts and their tokens live in daemon memory only, and no RPC answer carries a token.
- **`team.get`'s connector rows** gain `auth: keys | oauth` and the state `sign_in_again`.
- **`connector.connected`** gains `issuer` when signed in. No new event kind: signing in again is connecting again.
- **The CLI.** `farik connect <agent> <name> --url <url> --sign-in [--client-id <id>] [--callback-port <port>] [--scope <s>]... [--tag …]...`: `--sign-in` conflicts with `--key` and `--command`. It prints `Sign in to <host> in your browser: <url>`, opens it, waits 10 minutes, prints `Signed in to <issuer>.`, then the tools and the store line as step 01 does.

What a non-technical user sees:
- **On `ConnectorAdd`, step 1 tries signing in first** for a web address. The page calls `connector.sign_in`.
  - When the service offers it, the page shows "Sign in with <host>", then "Waiting for you to sign in to <host>…", polling `connector.sign_in_status` every 2 seconds, then "Signed in to <host>", and goes on to labelling.
  - `sign_in_not_offered` shows step 01's key fields, unchanged.
  - `sign_in_not_supported` says "<host> doesn't let Farik sign in by itself yet. If <host> gives you a key, paste it below." over the key fields.
- **The agent page's row** of a signed-in server says "Signed in to <host>". A lapsed one says "<host> ended Farik's sign-in. Sign in again to use it", with "Sign in again", which opens `ConnectorAdd` filled in from the team file at the sign-in.

ADR 0033 records who runs the sign-in, where the grant is kept, the registration order, the redirect, refresh and revocation. It is written in Task 2's commit.

For the founder, made by this plan and open to the founder's reversal:
- **O1, CIMD.** Not built here. It needs a document at an `https` address Farik owns, listing its redirect URIs. Every service checked that offers CIMD also offers DCR. Recommendation: add it with the site of the web launch (phase 11, ADR 0017), before DCR is removed from the specification.
- **O2, services with neither (GitHub, Slack, Google).** This step signs in to them only with a `client_id` the kit or the user supplies. Whether Farik registers its own apps (Google requires its app verification for sensitive scopes) is decided before step 06's plan. Recommendation: GitHub by a fine-grained token (a key), Slack by its kit's choice in step 06, and Google by a Farik-registered desktop client with the narrowest read scopes, verification started before the launch.
- **O3, the mockups.** The founder approves Task 1's boards, or says to approve them automatically. Task 8 does not start until then.

Note for step 09, from the same research (docs.stripe.com/mcp, read 2026-10-02): from 2026-10-31 `mcp.stripe.com` answers 401 to full secret keys and to restricted keys without the Agent tag. Step 09's "tagged read-only restricted key" must be an Agent-tagged one; Stripe also offers DCR, so signing in works through this step.

## File map

```
docs/design/mockups/{ConnectorAdd,AgentEdit,SignInDone}.dc.html, canvas.json   Task 1
docs/decisions/0033-signing-in-to-a-connector-s-service.md      creates: the ADR (Task 2)
docs/schemas/team.schema.json                                   modifies: mcpServer.oauth (Task 2)
crates/core/src/team.rs                                         modifies: OAuthSettings, its refusals, spec_sha256 (Task 2)
Cargo.toml (workspace)                                          modifies: rmcp gains the auth feature (Task 3)
crates/runtime/src/sign_in.rs, lib.rs                           creates: the sign-in (Task 3), refresh and revoke (Task 4)
crates/runtime/tests/fixture_oauth.rs                           creates: an authorization server and a protected MCP server (Task 3)
crates/runtime/src/connectors.rs                                modifies: ConnectorEntry.oauth, the bearer header, list_tools with a bearer (Task 4)
crates/runtime/src/orchestrator/session.rs                      modifies: refresh at session setup, lapsed left out (Task 5)
crates/runtime/src/daemon.rs                                    modifies: the launch route's refresh, the refresh mutex (Task 5)
crates/runtime/src/daemon/team.rs                               modifies: sign-in RPCs, attempts, connect with an attempt, states, revoke (Task 6)
crates/runtime/src/daemon/web.rs                                modifies: routes the two RPCs (Task 6)
docs/schemas/{rpc,event}.schema.json                            modifies: two RPCs, attempt, auth, sign_in_again, issuer (Task 6)
crates/protocol/src/event.rs                                    modifies: issuer on connector.connected (Task 6)
crates/cli/src/connector.rs, lib.rs                             modifies: --sign-in and its flags (Task 7)
packages/protocol-client/src/mapping.ts                         modifies: the new RPCs' camelCase (Task 8)
apps/web/src/pages/{ConnectorAdd,AgentEdit}.tsx, connectors.test.tsx, strings/en.ts   modifies (Task 8)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 9)
```

## Interfaces

Consumes: `CustomServer`, `CustomTransport`, `spec_sha256`, `canonical_json` (`farik-core`, step 01); `ConnectorEntry`, `ConnectorSecrets`, `SecretAt`, `Secret`, `list_tools`, `launch_headers`, `confirmed_entry`, the launch route and `take_from_session`, `custom_entry`, `labelled`, `forget_removed_keys`, `Kept::runs` (`farik-runtime`, step 01); `here_or_sent` (`farik` cli, main).

Produces:

```rust
// farik-core
pub struct OAuthSettings { pub client_id: Option<String>, pub callback_port: Option<u16>,
    pub scopes: Vec<String> }
pub enum CustomTransport { Stdio { command: String, args: Vec<String> },
    Http { url: String, headers: BTreeMap<String, String>, oauth: Option<OAuthSettings> } }
// farik-runtime, sign_in.rs
pub struct OAuthGrant { pub issuer: String, pub client_id: String, pub token_endpoint: String,
    pub revocation_endpoint: Option<String>, pub access_token: Secret,
    pub refresh_token: Option<Secret>, pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>, pub scopes: Vec<String>, pub lapsed: bool }
pub struct SignIn { /* the listener, the OAuthState, the deadline */ }
impl SignIn {
    pub fn authorize_url(&self) -> &str;
    pub fn callback_addr(&self) -> std::net::SocketAddr;
    pub async fn finish(self) -> Result<OAuthGrant, SignInError>;
}
pub async fn start_sign_in(url: &str, settings: &OAuthSettings, now: DateTime<Utc>)
    -> Result<SignIn, SignInError>;
pub async fn refreshed(grant: &OAuthGrant, resource: &str, now: DateTime<Utc>,
    valid_for: Duration, within: Duration) -> Result<Option<OAuthGrant>, SignInError>;
    // Ok(None): still valid; Err(Lapsed): refused for good
pub async fn revoke(grant: &OAuthGrant);
pub enum SignInError { NotOffered, NotSupported, PkceNotSupported, Denied(String), Mismatch,
    TimedOut, Lapsed, Failed(String) }
pub const SIGN_IN_WINDOW: Duration = Duration::from_secs(600);
// farik-runtime, connectors.rs
pub struct ConnectorEntry { pub spec_sha256: String, pub keys: BTreeMap<String, Secret>,
    pub oauth: Option<OAuthGrant> }
pub async fn list_tools(server: &CustomServer, keys: &BTreeMap<String, Secret>,
    bearer: Option<&Secret>, folder: &Path) -> Result<Vec<ListedTool>, ConnectorError>;
```

`launch_headers` keeps its signature and adds `Authorization: Bearer <access token>` when `entry.oauth` is set.

Wire (`snake_case`): the RPCs `connector.sign_in { agent, server } → { attempt, authorize_url }` and `connector.sign_in_status { attempt } → { state, reason? }`; `connector.tools` and `connector.connect` gain `attempt?`; `team.get`'s `connectors` rows gain `auth` and the state `sign_in_again`; `connector.connected` gains `issuer?`; the team file's `mcpServer` gains `oauth { client_id?, callback_port?, scopes? }`.

## Tasks

### Task 1: The sign-in screens, mocked up

A Sonnet agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job. They are copied into `docs/design/mockups/`.

- **`ConnectorAdd`, step 1, signing in** (one board per state):
  - **Offered.** The web address `https://mcp.notion.com/mcp` filled in. Below it, "mcp.notion.com lets you sign in." and the primary button "Sign in with mcp.notion.com". Under the button, small: "Farik opens its sign-in page in a new tab. Come back here when you're done."
  - **Waiting.** A quiet spinner, "Waiting for you to sign in to mcp.notion.com…", the link "Open the sign-in page again", and "Cancel".
  - **Signed in.** A check mark, "Signed in to mcp.notion.com.", and Next.
  - **Failures**, one muted error line each above "Try again":
    - "You said no on mcp.notion.com's page, so Farik isn't connected."
    - "The sign-in took longer than 10 minutes."
    - "Something didn't match on the way back from mcp.notion.com, so Farik stopped to keep you safe."
  - **Not supported.** "api.githubcopilot.com doesn't let Farik sign in by itself yet. If it gives you a key, paste it below.", over step 01's key fields.
- **`ConnectorAdd`, Done, signed in.** "Signed in. Theo uses Notion as you. Farik keeps the sign-in in your keychain." The file variant reads "…in a private file only you can read."
- **`AgentEdit`, "Added by you"**, two new rows beside step 01's:
  - "notion · Reached at a web address · Signed in to mcp.notion.com · 12 tools: 9 only read, 3 ask you".
  - A lapsed row: "mcp.linear.app ended Farik's sign-in. Sign in again to use it.", with the button "Sign in again".
  - Remove's confirmation for a signed-in row: "Remove notion from Theo? Farik deletes the sign-in from your keychain and asks mcp.notion.com to forget it." Where the service has no revocation, the confirmation instead reads "…To remove Farik completely, also remove it in mcp.notion.com's settings."
- **`SignInDone`**, the tab the loopback listener serves: the Farik mark, "You're signed in to mcp.notion.com. You can close this tab and go back to Farik." Its failure variant reads "Farik couldn't finish signing in: <sentence>. Close this tab and try again in Farik." Plain, with no links and no script.

Gate (O3): the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date. Task 8 does not start until then; Tasks 2 to 7 do not depend on the boards.

- [ ] `docs(design): mock up signing in to a service`

### Task 2: Signing in, in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, ADR 0033. Produces `OAuthSettings`, `CustomTransport::Http.oauth`.

- `accepts_an_http_server_that_signs_in`: `oauth: {}` and `oauth: { client_id: "abc", callback_port: 33418, scopes: ["read"] }` both validate and read back equal.
- `refuses_oauth_where_it_cannot_be`: `oauth_on_stdio` at `/agents/0/mcp_servers/0/oauth`; `oauth_with_keys` at `…/credential_keys`; `oauth_header_conflict` at `…/headers/Authorization`; `callback_port_without_client` at `…/oauth/callback_port`.
- `refuses_a_scope_with_a_space_or_quote`: `"a b"` and `"a\"b"` are schema errors at `…/oauth/scopes/0`.
- `a_server_without_oauth_keeps_its_hash`: step 01's `spec_hash_ignores_key_order_and_sees_every_field` fixture gives the same `spec_sha256` as a literal hex string recorded before this change.
- `oauth_settings_change_the_hash`: `oauth: {}` against none, and changing `client_id`, `callback_port` or one scope, each changes `spec_sha256`.

- [ ] `feat(core): let a web-address connector sign in`

### Task 3: The sign-in

Files: `sign_in.rs`, `lib.rs`, the workspace `Cargo.toml` (rmcp `auth`), `crates/runtime/tests/fixture_oauth.rs`. Produces `start_sign_in`, `SignIn`, `SignInError`, `OAuthGrant`, `SIGN_IN_WINDOW`.

The fixture is one axum server on loopback that plays:
- the protected MCP server: a 401 with `resource_metadata` without a bearer, and one tool listed with the bearer;
- the protected-resource metadata;
- the authorization server's metadata, `/register`, `/authorize`, `/token` and `/revoke`. `/authorize` answers 302 to the `redirect_uri` with `code`, `state` and `iss`.

The fixture records every request, and flags turn off DCR, S256 or `iss`, or answer `error=access_denied`. A test follows `authorize_url` with redirects off, then requests the `Location`.

- `signs_in_with_dynamic_registration`: `/register` gets `application_type: native`, `token_endpoint_auth_method: none` and `redirect_uris: ["http://localhost:<callback port>/callback"]`; the grant's `client_id` is the registered one, its `issuer` the fixture's, and it holds both tokens.
- `uses_a_preregistered_client_without_registering`: with `client_id` and `callback_port` set, `/register` is never called and `redirect_uri` names that port; without `callback_port`, it names 33418.
- `sends_pkce_s256_and_the_resource`: `/authorize` gets `code_challenge_method=S256`; `/token`'s `code_verifier` hashes to that challenge; both requests carry `resource=<server url>`.
- `refuses_a_server_without_s256`: `PkceNotSupported`, and the fixture saw no `/register`.
- `refuses_with_neither_registration_nor_client`: `NotSupported`.
- `says_not_offered_without_resource_metadata`: an MCP server answering 401 with no `WWW-Authenticate` and no well-known document gives `NotOffered`.
- `refuses_the_wrong_state`: a callback with another `state` gives `Mismatch`, and `/token` is never called.
- `refuses_another_issuer_or_a_missing_one_when_promised`: `iss` of another value gives `Mismatch`; no `iss` with `authorization_response_iss_parameter_supported: true` gives `Mismatch`; no `iss` without that flag signs in.
- `reports_access_denied`: `Denied("access_denied")`.
- `answers_one_callback_then_closes`: after the callback, connecting to `callback_addr` is refused; a `GET /other` before it gets 404 and the attempt still completes.
- `listens_on_loopback_only`: `callback_addr().ip().is_loopback()`.
- `gives_up_after_ten_minutes`: with a paused clock, `finish` gives `TimedOut`.
- `refuses_an_endpoint_that_is_not_https`: metadata naming `http://auth.example/authorize` gives `Failed`, naming the endpoint.

- [ ] `feat(runtime): sign in to an MCP server's service with OAuth`

### Task 4: Keeping, refreshing and revoking a grant

Files: `connectors.rs`, `sign_in.rs`. Produces `ConnectorEntry.oauth`, `refreshed`, `revoke`, the new `list_tools`.

- `an_oauth_entry_round_trips_and_never_prints`: the stored form reads back equal; `Debug` of the entry and of `OAuthGrant` shows neither token.
- `an_entry_stored_before_has_no_oauth`: `{"spec_sha256":"…","keys":{}}` loads with `oauth: None`.
- `launch_headers_send_the_bearer`: an entry with a grant and the header `X-Workspace: a` gives both, `Authorization` being `Bearer <access token>`.
- `lists_tools_with_the_signed_in_token`: against the fixture, `list_tools` with the bearer lists its tool, and without it fails.
- `refreshes_a_token_about_to_expire`: a grant expiring in 4 minutes with `valid_for` 35 minutes gives `Some`, with the fixture's new access token and its rotated refresh token.
- `leaves_a_fresh_token_alone`: a grant expiring in 2 hours gives `None`, and `/token` is not called.
- `a_refused_refresh_lapses`: `/token` answering `invalid_grant` gives `Lapsed`; answering 500 gives `Failed`.
- `revokes_the_refresh_token`: `/revoke` gets the refresh token with `token_type_hint=refresh_token`; a grant without one sends the access token; a `/revoke` answering 500 is not an error.

- [ ] `feat(runtime): keep, refresh and revoke an agent's sign-in`

### Task 5: Signed-in connectors in sessions

Files: `orchestrator/session.rs`, `daemon.rs`.

- `a_session_refreshes_a_token_that_would_expire_during_it`: the fixture's grant, expiring in 10 minutes with a 30-minute `max_wall_clock`, is refreshed before `mcp.json` is written, and the entry kept holds the rotated refresh token.
- `a_lapsed_sign_in_is_left_out`: a refresh answering `invalid_grant` saves `lapsed: true`; the server is absent from `mcp.json` and the registration, and its calls are denied `connector_not_in_session`.
- `launch_refreshes_an_expired_token`: the route answers the new token in `Authorization`.
- `launch_refuses_a_lapsed_sign_in`: 403 `sign_in_again`, and the server is taken from the session.
- `two_launches_refresh_once`: two concurrent launches of one expired grant make one `/token` request.
- `a_changed_sign_in_setting_needs_connecting_again`: `oauth.scopes` changed in `team.yaml` leaves the server out, `connect_again`.
- `a_live_session_calls_a_signed_in_connector` (integration, `--integration`): see Verification.

- [ ] `feat(runtime): refresh a sign-in before a session needs it`

### Task 6: Signing in through the daemon

Files: `daemon/team.rs`, `daemon/web.rs`, `rpc.schema.json`, `event.schema.json`, `protocol/src/event.rs`.

- `sign_in_then_connect_keeps_the_grant`: `connector.sign_in` answers an address; after the test follows it, `connector.sign_in_status` is `signed_in`. `connector.tools` with the attempt lists the tool, and `connector.connect` with the attempt and tags keeps a grant. The team file's entry has `oauth: {}`, and `connector.connected` carries `issuer`.
- `no_reply_event_or_log_holds_a_token`: across that test, neither token's text appears in any RPC reply, in `.farik/local/events.db`, or in `team.yaml`.
- `sign_in_status_says_why_it_failed`: `access_denied` gives `{ state: failed, reason: { code: "access_denied" } }`.
- `an_attempt_is_used_once_and_expires`: a second `connect` with one attempt is `sign_in_unknown`, and so is one past `SIGN_IN_WINDOW` (paused clock).
- `connect_needs_an_attempt_to_sign_in`: `server.oauth` set with `keys` is `sign_in_needed`.
- `a_new_attempt_ends_the_old`: after a second `connector.sign_in` for the agent and server, the first attempt's callback address refuses connections.
- `disconnect_deletes_then_revokes`: the entry is gone and `/revoke` got the refresh token; with `/revoke` answering 500 the entry is still gone and the reply is `{}`.
- `team_get_says_auth_and_sign_in_again`: a lapsed grant gives `state: sign_in_again`, `auth: oauth`; a key server, `auth: keys`.

- [ ] `feat(runtime): sign an agent in to a service from the web app`

### Task 7: The command line

Files: `cli/src/connector.rs`, `cli/src/lib.rs`. `connect` calls `connect_with(project, asked, io, open: &dyn Fn(&str))`, so the test passes an opener that follows the address.

- `farik_connect_signs_in_and_keeps_the_grant`: against the fixture, prints `Sign in to 127.0.0.1 in your browser:` with the address, then `Signed in to <issuer>.`, and keeps a grant; the command sent to a running daemon holds no token.
- `farik_connect_sign_in_takes_no_key`: `--sign-in --key A` and `--sign-in --command x` are refused by the argument parser.
- `farik_connect_says_when_a_service_offers_no_sign_in`: `NotOffered` prints "<host> does not offer signing in; give its key with --key".

- [ ] `feat(cli): sign an agent in to a service`

### Task 8: The screens

Files: `ConnectorAdd.tsx`, `AgentEdit.tsx`, `connectors.test.tsx`, `strings/en.ts`, `mapping.ts`. Built from Task 1's approved boards.

- `connector_add_offers_sign_in_when_the_service_has_one`: after Next on a web address, the button reads "Sign in with mcp.notion.com", and no key field shows.
- `connector_add_waits_for_the_sign_in_then_labels`: the button opens `authorize_url` with `noopener`; `sign_in_status` is polled until `signed_in`; `connector.tools` is sent with the attempt and no `keys`.
- `connector_add_says_why_a_sign_in_failed`: one assertion per reason code, each the board's sentence.
- `connector_add_falls_back_to_a_key`: `sign_in_not_offered` shows the key fields alone; `sign_in_not_supported` shows them under the board's sentence.
- `agent_edit_shows_signed_in_and_sign_in_again`: the two rows; "Sign in again" opens `ConnectorAdd` at the sign-in, filled in.
- `agent_edit_remove_says_the_service_is_asked_to_forget`: the confirmation's sentence for a signed-in row.

- [ ] `feat(web): sign in to a service from the agent page`

### Task 9: Spec and plan

`docs/SPEC.md`: 6.7 (signing in, the grant per agent, refresh, revocation, "Sign in again"), 8.2 (the bearer through the headers helper), 8.5 (`issuer` on `connector.connected`), 8.6 (the callback's security, tokens never in a file Farik writes but the private store), F9 (the two RPCs, `attempt`, `auth`, `sign_in_again`). `docs/plans/project-plan.md`: phase 7's row 03, corrected if execution changed it. `docs/design/role-kits.md`: its steps table.

- [ ] `docs(spec): record signing in to a service`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_calls_a_signed_in_connector passed
```

`a_live_session_calls_a_signed_in_connector` (Task 5): a real Claude Code session is given the fixture's server with a grant kept. The stream's `system/init` line lists `mcp__fixture__<tool>`, the call succeeds, and the fixture saw `Authorization: Bearer <access token>` from the headers helper. This is also the probe of `headersHelper` that step 01 left undone.

The founder's live check, recorded in the pull request: sign in to Linear (`https://mcp.linear.app/mcp`, DCR) from the web app as one agent, list its tools, run one session that calls a `network` tool, then Remove.
