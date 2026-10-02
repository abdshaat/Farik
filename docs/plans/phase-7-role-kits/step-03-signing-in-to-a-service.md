# Phase 7, step 03: Signing in to a service

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.2, 8.5, 8.6; F9
Depends on: step 01 of this phase (committed; the custom connector, `ConnectorEntry`, `ConnectorSecrets`, the launch route and the headers helper, `spec_sha256`), step 02 (committed; nothing of it is consumed, it lands first)
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 4 Blocking, all folded; no second round (ADR 0032)
Mockups approved by: the founder, 2026-10-02 (SignInDone, PhoneSignIn, and the revised AgentEdit and ConnectorAdd)

## Goal

After step 01 a web-address connector takes pasted keys only. When this step is done, a user gives an agent a service that offers signing in (Notion, Linear, Stripe, Atlassian, Sentry, PostHog among those checked) by pressing "Sign in with <service>" on `ConnectorAdd`, or with `farik connect … --sign-in`. The service's page opens in the browser, the user says yes there, and Farik keeps that agent's sign-in where step 01 keeps its keys, refreshes it before a session needs it, hands it to the session through the headers helper, and asks the service to forget it on "Remove". A sign-in the service ended shows "Sign in again" on the agent page. Out of scope: kit connectors and their shipped client ids (step 05), a hosted client metadata document (O1), a client secret (Slack's case), and Farik asking for wider scopes when a service answers `insufficient_scope` (the user signs in again).

## Decisions

Research (2026-10-02; MCP specification 2026-07-28, the current one; code.claude.com/docs/en/mcp, undated, read that day; each service's metadata read live that day):
- Claude Code in `-p` cannot sign in ("there's no `/mcp` panel"); it only uses tokens it already holds, keyed by its config folder, not by agent, in one shared credentials blob that concurrent processes overwrite (anthropics/claude-code #65752, 2026-06-05).
- A `headersHelper` whose output holds `Authorization` is used as the server's authentication, and Claude Code then does not try OAuth itself. It runs at session start and on every reconnect, with a 10-second limit, and runs again and retries once when a tool call gets 401 or 403.
- DCR is deprecated in 2026-07-28 in favour of client metadata documents (CIMD), but kept. Notion, Linear, Sentry and PostHog offer both; Stripe and Atlassian DCR only; GitHub, Slack and Google neither (Google Cloud's "authenticate MCP" page, updated 2026-09-30).
- `rmcp` 3.3.0, already pinned, has an `auth` feature: discovery (RFC 9728, RFC 8414 and OIDC), DCR, a pre-registered client, PKCE S256, `resource` (RFC 8707, checked against the server's url), the AS metadata `issuer` check, the callback `iss` rule (RFC 9207) applied in `handle_callback_with_issuer` from the state it recorded with the authorization URL, and refresh. It has no loopback listener and no revocation. Read in its source (`src/transport/auth.rs`): `resolve_metadata` falls back to synthesized `/authorize`, `/token`, `/register` (`AuthorizationMetadataSource::LegacyEndpointFallback`), which `OAuthState::start_authorization` hides; it only warns when `code_challenge_methods_supported` is absent; it reports only `invalid_grant` by code (`TokenRefreshRejected`); and it logs the authorization code at `debug`.

Who does what:
- **Farik runs the sign-in, not Claude Code.** For the web app the daemon runs it, for the command line `farik connect`'s own process, through one function. The tokens go into the agent's `ConnectorEntry`, in the keychain or `connectors.json` (ADR 0030), and reach the session as `Authorization: Bearer <access token>` from the headers helper step 01 built. Rejected: letting Claude Code sign in and keep the tokens. `-p` cannot sign in, its store is not per agent, and the daemon could neither confirm the definition (ADR 0030), refresh before a session, nor show "Sign in again".
- **Through `rmcp`'s `auth` feature**, in this order, not through `OAuthState`:
  1. `AuthorizationManager::new_with_oauth_http_client(url, Arc<dyn OAuthHttpClient>)`, where Farik's `OAuthHttpClient` (over `reqwest`) refuses any request, and any redirect hop, to a URL that is neither `https` nor `http` on a loopback host (`sign_in_failed: <url> is not https`);
  2. an MCP `initialize` POST without a bearer, through the same check; `resolve_metadata_from_challenge(<its 401's WWW-Authenticate, else None>)`. A `source` other than `ProtectedResourceMetadata` is `NotOffered`;
  3. Farik's checks on the metadata: `code_challenge_methods_supported` absent or without `S256` is `PkceNotSupported`; neither `oauth.client_id` nor `registration_endpoint` is `NotSupported`; no `issuer` is `Failed`;
  4. `set_metadata`, then `AuthorizationSession::new(manager, AuthorizationRequest::new(redirect).with_client_name("Farik")[.with_preregistered_client(id)][.with_scopes(scopes)])`;
  5. the grant's `resource` is the `resource` parameter of the session's `auth_url` (rmcp keeps the PRM's `resource` private and sends it there);
  6. at the callback, `handle_callback_with_issuer(code, state, iss)`.

  `set_allow_missing_issuer` stays `false`, its default. `start_sign_in` gives up after 15 seconds (`SIGN_IN_START`), `Failed`. The feature brings `oauth2` 5.0 and its transitive dependencies; `reqwest` becomes a direct dependency of `farik-runtime` at the version `rmcp` already locks. Rejected: hand-writing the flow over `reqwest`, which `rmcp` already does.

The team file:
- **An http custom server gains `oauth: { client_id?, callback_port?, scopes? }`.** Its presence means "signed in". Rejected: a separate `auth` field, which would be two fields saying one thing.
  - `client_id` is a pre-registered public client, at most 200 characters. `callback_port`, 1024 to 65535, comes only with `client_id` (`callback_port_without_client`). `scopes`, at most 16, each matching `^[\x21\x23-\x5B\x5D-\x7E]{1,200}$` (RFC 6749's scope characters). `validate_team` refuses `oauth` on stdio (`oauth_on_stdio`), with `credential_keys` non-empty (`oauth_with_keys`), and beside a header named `Authorization` in any case (`oauth_header_conflict`), each at its field.
  - With no `scopes`, Farik passes none and accepts rmcp's selection: the 401's `scope`, else the PRM's `scopes_supported`, else the AS's `scopes_supported`, plus `offline_access` when the AS lists it. The last fallback is broader than the specification's "omit `scope`"; it is accepted because the tool tags, not scopes, govern each call.
- **`spec_sha256` adds `"oauth": {"client_id": <string|null>, "callback_port": <number|null>, "scopes": [<as written>]}` only when `oauth` is present**, so every entry connected under step 01 keeps its hash. A change to `client_id`, `callback_port` or one scope makes the server "Connect again", as ADR 0030 does for every field.

Registration, in the specification's order less CIMD:
1. `oauth.client_id` when given, with no registration request.
2. Else DCR when the authorization server lists a `registration_endpoint`: `client_name: "Farik"`, `application_type: "native"`, `token_endpoint_auth_method: "none"`, `grant_types: ["authorization_code", "refresh_token"]` (rmcp's request), and this sign-in's exact `redirect_uris`. It runs once per sign-in, and the `client_id` is kept with the tokens, since a client is bound to its issuer.
3. Else refused `sign_in_not_supported`. A client secret is never taken: Farik is a public client. CIMD is not in this step (O1).

Redirect and callback:
- **The redirect is `http://localhost:<port>/callback`**, and the listener binds `127.0.0.1:<port>`, and `[::1]:<port>` when the machine has IPv6, never `0.0.0.0`. `localhost`, not `127.0.0.1`, because authorization servers match a registered URI exactly and Claude Code had to return to `localhost` for that reason (its MCP page: v2.1.229 sent `127.0.0.1` and v2.1.231 went back).
- **The port.** With DCR, Farik binds `127.0.0.1:0`, then `[::1]` on the same port; if that is taken it tries again with a new port, up to 5 times. With a pre-registered `client_id` it is `callback_port`, which the service's app registration names, or 33418 when none is given. A port that cannot be bound (another attempt's, another program's, a privileged one) fails `start_sign_in` with `sign_in_failed: port <n> is in use on this computer`. With a fixed port, a second agent's attempt on it is refused so; only the same agent and server's attempt is ended and replaced.
- **The listener answers the first `GET /callback` whose `state` is the attempt's** (random, single-use), and closes. Any other path is 404, and a callback with another `state` is 400; neither ends the attempt. Farik compares `state` before calling rmcp.
- **The page it serves** says the outcome in a sentence from Farik's reason code and quotes nothing the service sent (no `error_description`). It links nowhere, runs no script, and is served with `Content-Type: text/html; charset=utf-8`, `Content-Security-Policy: default-src 'none'`, `Cache-Control: no-store` and `Referrer-Policy: no-referrer`.
- **An attempt lasts 10 minutes**, then fails `sign_in_timed_out`. A new attempt for the same agent and server ends the old one.

Callback security:
- **PKCE S256 always.** Metadata whose `code_challenge_methods_supported` is absent or lacks `S256` is refused `pkce_not_supported` by Farik's check before any registration, as the specification requires (rmcp only warns when it is absent).
- **`iss`** (RFC 9207): when present, it must equal the metadata's `issuer` by exact string; when absent while the metadata sets `authorization_response_iss_parameter_supported`, it is refused. Both fail `sign_in_mismatch`. rmcp applies this to a code callback from the state it recorded with the authorization URL. An `error` callback is checked for `iss` by Farik with the same rule; on a mismatch it is `sign_in_mismatch` and its `error` is not shown (the specification: "MUST NOT act on or display"). A matching `error=access_denied` is `access_denied`; any other `error` is `sign_in_failed: <host> refused the sign-in`.
- **`resource`** is the PRM's `resource`, which rmcp checks is the server's `url` or a path prefix of it, sent on the authorization, token and refresh requests, and kept in the grant. The token is sent to that server only, and to no other connector.
- **Endpoints must be `https`**, or `http` on a loopback host, which the test fixture needs: the metadata URLs and the authorization, token and registration endpoints through Farik's `OAuthHttpClient`, the token and revocation endpoints of a kept grant through the same check in `refreshed` and `revoke`.
- **Opening the page.** In the browser, `window.open(authorize_url, '_blank', 'noopener')` runs synchronously in the click, with the address `connector.sign_in` already answered at Next, so a pop-up blocker lets it through. The command line passes it as one argument to `open` on macOS or `xdg-open` elsewhere, never through a shell, and prints it too; an opener that fails is not an error (the founder's WSL2 may have no `xdg-open`).
- **What the specification leaves to the client:** any local process can bind a port and receive a code (its security best practices, "localhost impersonation"). PKCE makes such a code useless without the verifier, which stays in the attempt's memory.
- **No token in a log.** Farik installs no `tracing` subscriber that would print rmcp's `debug` lines, which hold the authorization code.

Tokens:
- **Refresh and revocation are Farik's own `reqwest` form POSTs** to the grant's `token_endpoint` (RFC 6749 §6: `grant_type=refresh_token`, `refresh_token`, `client_id`, `resource`) and `revocation_endpoint` (RFC 7009: `token`, `token_type_hint`, `client_id`), each through the https-or-loopback check, because rmcp has no revocation, keeps refresh on a manager the keychain cannot rebuild, and reports only `invalid_grant` by code. The token response's `error` field decides lapsing. A response without `refresh_token` keeps the old one.
- **Kept in the entry.** `ConnectorEntry` gains `oauth: Option<OAuthGrant>`, stored beside `keys` in the same JSON object. An entry stored before this step, with no `oauth`, loads as `None`. `Debug` shows `***` for both tokens.
- **Refresh.** At session setup `run_session` calls `refreshed` with `valid_for = max_wall_clock + 5 min`. The rotated refresh token is saved before the new access token is used, since public clients' refresh tokens rotate (the specification's security considerations). The launch route calls `refreshed` with `valid_for = 60 s`, `timeout = 3 s`, inside `KEY_STORE_DEADLINE`, which still bounds the whole answer. A refresh that fails with the access token expired, or does not finish in 3 s, answers 503 `sign_in_failed`; a lapsed grant answers 403 `sign_in_again`; both take the server from the session. The helper runs again on every reconnect, which is how a long session gets a new token.
- **One change at a time per entry.** Refresh, connect, disconnect, and the deletes of `forget_removed_keys` and retirement (`forget_connector_keys`) take one `tokio::sync::Mutex` per entry, from `DaemonState::entry_lock(&SecretAt)`, keyed by `SecretAt::account()`. A refresh takes it, re-reads the entry (gone: nothing to do; no longer due: uses it), refreshes and saves. The refresh and its save run in a spawned task that finishes even when the launch route's deadline has passed, so a rotated token is always kept. The synchronous deletes `try_lock`: holding it, they delete at once as today; else they spawn a task that waits for it, then deletes. A save that fails after the service rotated leaves the server out of that session; the next refresh lapses it.
- **A refused refresh lapses the grant.** `invalid_grant`, `invalid_client` or `unauthorized_client` sets `lapsed: true` in the entry. The server is then left out of sessions, and `team.get` reports it `sign_in_again`. Any other failure with the access token still valid uses that token; with it expired, the server is left out of that session only.
- **The daemon's cache.** `Kept::Entry` gains `signed_in: Option<SignedIn { lapsed: bool, revokes: bool }>` (`Some` when a grant is kept; `revokes` when it has a `revocation_endpoint`). `Kept::runs` is false for a lapsed grant, and for a server with `oauth` but no grant. Every save of a grant (connect, refresh, lapse) calls `read_kept`.
- **Revocation.** "Remove" (`connector.disconnect`, `farik disconnect`) loads the entry, deletes it, then sends the refresh token, else the access token, to the grant's `revocation_endpoint`, at most 5 seconds, best effort. From `forget_removed_keys` and retirement, which are synchronous, revocation is spawned, not awaited. Connecting again over an entry holding another grant revokes the replaced grant the same way. With no revocation endpoint, the confirmation says where to remove Farik in the service's own settings.

The protocol:
- **RPCs.** `connector.sign_in { agent, server }` → `{ attempt, authorize_url, issuer }`, refused `sign_in_not_offered`, `sign_in_not_supported`, `pkce_not_supported`, or `sign_in_failed`. Its `server` is the same object `connector.tools` takes (name, transport, url, headers, oauth). `connector.sign_in_status { attempt }` → `{ state: waiting | signed_in | failed, reason? }`, where `reason` is `{ code, message }` (`access_denied`, `sign_in_timed_out`, `sign_in_mismatch`, `sign_in_failed`).
  - `connector.tools` and `connector.connect` take `attempt` in place of `keys` when `server.oauth` is set: without it `sign_in_needed`, an unknown, used, expired or unfinished one `sign_in_unknown`. `connect` uses the attempt up. An attempt is bound, at `connector.sign_in`, to its agent and the server's `name`, `url` and `oauth`. `connector.tools` and `connector.connect` with an attempt whose agent, name, `url` or `oauth` differ from the `server` they carry are refused `sign_in_unknown`, and the attempt is ended.
  - Attempts and their tokens live in daemon memory only, and no RPC answer carries a token. No reason message repeats a server's response body, only the step (discovery, registration, token) and the HTTP status, as step 01's `list_tools` does.
- **`team.get`'s connector rows** gain `auth: keys | oauth`, `revokes?` (signed-in rows) and the state `sign_in_again`.
- **`connector.connected`** gains `issuer` when signed in. No new event kind: signing in again is connecting again.
- **The CLI.** `farik connect <agent> <name> --url <url> --sign-in [--client-id <id>] [--callback-port <port>] [--scope <s>]... [--tag …]...`: `--sign-in` conflicts with `--key` and `--command`. It prints `Sign in to <issuer host> in your browser: <url>`, opens it, waits 10 minutes, prints `Signed in to <issuer>.`, then the tools and the store line as step 01 does.

What a non-technical user sees:
- **On `ConnectorAdd`, step 1 tries signing in first** for a web address. Next calls `connector.sign_in`.
  - When the service offers it, the page shows "Sign in with <issuer host>" and, under it, "for <url host>" whenever the two hosts differ (Stripe's MCP host is `mcp.stripe.com`, its sign-in `access.stripe.com`; a changed `url` must not borrow a known service's name). Then "Waiting for you to sign in to <issuer host>…", polling `connector.sign_in_status` every 2 seconds, then "Signed in to <issuer host>", and on to labelling. The link "Use a key instead" shows step 01's key fields (Stripe's Agent-tagged key, step 09).
  - `sign_in_not_offered` shows step 01's key fields, unchanged. `sign_in_not_supported` says "<host> doesn't let Farik sign in by itself yet. If <host> gives you a key, paste it below." over the key fields; `sign_in_failed` shows its sentence over them the same way.
- **The agent page's row** of a signed-in server says "Signed in to <host>". A lapsed one says "<host> ended Farik's sign-in. Sign in again to use it", with "Sign in again", which opens `ConnectorAdd` filled in from the team file at the sign-in.

ADR 0033 records who runs the sign-in, where the grant is kept, the registration order, the redirect, refresh and revocation. Its consequences say the loopback redirect assumes the browser and the daemon share a machine, which ADR 0021 guarantees (the daemon serves only `127.0.0.1`), and that phase 11's hosted web launch needs another redirect. It is written in Task 2's commit.

For the founder, made by this plan and open to the founder's reversal:
- **O1, CIMD.** Not built here. It needs a document at an `https` address Farik owns, listing its redirect URIs. Every service checked that offers CIMD also offers DCR. Each DCR sign-in leaves a client at the service (#65752 shows Notion users hitting this); revoking on replace limits it, CIMD ends it. Recommendation: add it with the site of the web launch (phase 11, ADR 0017), before DCR is removed from the specification.
- **O2, services with neither (GitHub, Slack, Google). Resolved by ADR 0035 (the founder, 2026-10-02).** This step still signs in to them only with a `client_id` the kit or the user supplies. Step 03b adds Farik's own registered app for GitHub, by device flow (Google is deferred until after the launch, ADR 0035's amendment); the sign-in relay for Slack, whose app needs a client secret, was planned as steps 03c and 03d and is deferred with Slack until after the launch, phase 12 (the founder, 2026-10-02: "Connecting slack is a later step keep it simple for now"), so Slack takes a pasted key meanwhile. Pasted keys stay the fallback. The recommendation of a GitHub key was not taken.
- **O3, the mockups.** The founder approves Task 1's boards, or says to approve them automatically. Task 8 does not start until then.

Note for step 09, from the same research (docs.stripe.com/mcp, read 2026-10-02): from 2026-10-31 `mcp.stripe.com` answers 401 to full secret keys and to restricted keys without the Agent tag. Step 09's "tagged read-only restricted key" must be an Agent-tagged one; Stripe also offers DCR, so signing in works through this step.

## File map

```
docs/design/mockups/{ConnectorAdd,AgentEdit,SignInDone}.dc.html, canvas.json   Task 1
docs/decisions/0033-signing-in-to-a-connector-s-service.md      creates: the ADR (Task 2)
docs/schemas/team.schema.json                                   modifies: mcpServer.oauth (Task 2)
crates/core/src/team.rs                                         modifies: OAuthSettings, its refusals, spec_sha256 (Task 2)
Cargo.toml (workspace), crates/runtime/Cargo.toml               modifies: rmcp gains auth; reqwest direct (Task 3)
crates/runtime/src/sign_in.rs, lib.rs                           creates: the sign-in (Task 3), refresh and revoke (Task 4)
crates/runtime/tests/fixture_oauth.rs                           creates: an authorization server and a protected MCP server (Task 3)
crates/runtime/src/connectors.rs                                modifies: ConnectorEntry.oauth, the bearer header, list_tools with a bearer (Task 4)
crates/runtime/src/orchestrator/session.rs, daemon.rs           modifies: refresh at session setup and the launch route, entry_lock, Kept (Task 5)
crates/runtime/src/daemon/{team,web}.rs, orchestrator/human.rs  modifies: sign-in RPCs and routes, attempts, connect with an attempt, states, the deletes under entry_lock, revoke (Task 6)
docs/schemas/{rpc,event}.schema.json, crates/protocol/src/event.rs   modifies: two RPCs, attempt, auth, revokes, sign_in_again, issuer (Task 6)
crates/cli/src/connector.rs, lib.rs                             modifies: --sign-in and its flags, connect_with (Task 7)
packages/protocol-client/src/mapping.ts                         modifies: the new RPCs' camelCase (Task 8)
apps/web/src/pages/{ConnectorAdd,AgentEdit}.tsx, connectors.test.tsx, strings/en.ts   modifies (Task 8)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 9)
```

## Interfaces

Consumes: `CustomServer`, `CustomTransport`, `spec_sha256`, `canonical_json` (`farik-core`, step 01); `ConnectorEntry`, `ConnectorSecrets`, `SecretAt`, `Secret`, `list_tools`, `launch_headers`, `confirmed_entry`, the launch route and `take_from_session`, `custom_entry`, `labelled`, `forget_removed_keys`, `forget_connector_keys`, `Kept`, `read_kept`, `KEY_STORE_DEADLINE` (`farik-runtime`, step 01); `here_or_sent`, `connect` (`farik` cli, main). From `rmcp::transport::auth`: `AuthorizationManager`, `AuthorizationSession`, `AuthorizationRequest`, `AuthorizationMetadataSource`, `OAuthHttpClient`, `OAuthHttpRequest`.

Produces:

```rust
// farik-core
pub struct OAuthSettings { pub client_id: Option<String>, pub callback_port: Option<u16>, pub scopes: Vec<String> }
pub enum CustomTransport { Stdio { command: String, args: Vec<String> },
    Http { url: String, headers: BTreeMap<String, String>, oauth: Option<OAuthSettings> } }
// farik-runtime, sign_in.rs
pub struct OAuthGrant { pub issuer: String, pub resource: String, pub client_id: String,
    pub token_endpoint: String, pub revocation_endpoint: Option<String>, pub access_token: Secret,
    pub refresh_token: Option<Secret>, pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>, pub scopes: Vec<String>, pub lapsed: bool }
pub struct SignIn { /* the listener, the AuthorizationSession, the issuer, the deadline */ }
impl SignIn { pub fn authorize_url(&self) -> &str; pub fn issuer(&self) -> &str;
    pub fn callback_addr(&self) -> std::net::SocketAddr;
    pub async fn finish(self) -> Result<OAuthGrant, SignInError>; }
pub async fn start_sign_in(url: &str, settings: &OAuthSettings, now: DateTime<Utc>)
    -> Result<SignIn, SignInError>;
pub async fn refreshed(grant: &OAuthGrant, now: DateTime<Utc>, valid_for: Duration,
    timeout: Duration) -> Result<Option<OAuthGrant>, SignInError>;
// refreshes when expires_at < now + valid_for, or expires_at is None and now - issued_at > 50 min; Ok(None): not needed.
// No refresh_token: Ok(None) while valid, Err(Lapsed) once expired. Err(Lapsed): invalid_grant, invalid_client,
// unauthorized_client. Err(Failed): anything else, including `timeout` passing.
pub async fn revoke(grant: &OAuthGrant);
pub enum SignInError { NotOffered, NotSupported, PkceNotSupported, Denied(String), Mismatch,
    TimedOut, Lapsed, Failed(String) }
pub const SIGN_IN_WINDOW: Duration = Duration::from_secs(600); pub const SIGN_IN_START: Duration = Duration::from_secs(15);
// farik-runtime, connectors.rs
pub struct ConnectorEntry { pub spec_sha256: String, pub keys: BTreeMap<String, Secret>, pub oauth: Option<OAuthGrant> }
pub async fn list_tools(server: &CustomServer, keys: &BTreeMap<String, Secret>,
    bearer: Option<&Secret>, folder: &Path) -> Result<Vec<ListedTool>, ConnectorError>;
pub(crate) struct SignedIn { pub lapsed: bool, pub revokes: bool }  // Kept::Entry.signed_in
impl DaemonState { pub(crate) fn entry_lock(&self, at: &SecretAt) -> Arc<tokio::sync::Mutex<()>>; }  // daemon.rs
pub(crate) fn connect_with(project: &Project, asked: &Asked<'_>, io: &mut CliIo<'_>,
    open: &dyn Fn(&str)) -> Result<Report, String>;  // cli; `connect` calls it with the system opener
```

`launch_headers` keeps its signature and adds `Authorization: Bearer <access token>` when `entry.oauth` is set.

Wire (`snake_case`): the RPCs `connector.sign_in { agent, server } → { attempt, authorize_url, issuer }` and `connector.sign_in_status { attempt } → { state, reason? }`; `connector.tools` and `connector.connect` gain `attempt?`; `team.get`'s `connectors` rows gain `auth`, `revokes?` and the state `sign_in_again`; `connector.connected` gains `issuer?`; the team file's `mcpServer` gains `oauth { client_id?, callback_port?, scopes? }`.

## Tasks

### Task 1: The sign-in screens, mocked up

A Sonnet agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job. They are copied into `docs/design/mockups/`.

- **`ConnectorAdd`, step 1, signing in** (one board per state):
  - **Offered.** The web address `https://mcp.notion.com/mcp` filled in. Below it, "mcp.notion.com lets you sign in." and the primary button "Sign in with mcp.notion.com". Under the button, small: "Farik opens its sign-in page in a new tab. Come back here when you're done.", and the link "Use a key instead". A variant with differing hosts: `https://mcp.stripe.com`, "Sign in with access.stripe.com", and under it "for mcp.stripe.com".
  - **Waiting.** A quiet spinner, "Waiting for you to sign in to mcp.notion.com…", the link "Open the sign-in page again", and "Cancel".
  - **Signed in.** A check mark, "Signed in to mcp.notion.com.", and Next.
  - **Failures**, one muted error line each above "Try again": "You said no on mcp.notion.com's page, so Farik isn't connected."; "The sign-in took longer than 10 minutes."; "Something didn't match on the way back from mcp.notion.com, so Farik stopped to keep you safe."
  - **Not supported.** "api.githubcopilot.com doesn't let Farik sign in by itself yet. If it gives you a key, paste it below.", over step 01's key fields.
- **`ConnectorAdd`, Done, signed in.** "Signed in. Theo uses Notion as you. Farik keeps the sign-in in your keychain." The file variant reads "…in a private file only you can read."
- **`AgentEdit`, "Added by you"**, two new rows beside step 01's: "notion · Reached at a web address · Signed in to mcp.notion.com · 12 tools: 9 only read, 3 ask you", and a lapsed row, "mcp.linear.app ended Farik's sign-in. Sign in again to use it.", with the button "Sign in again".
  - Remove's confirmation for a signed-in row: "Remove notion from Theo? Farik deletes the sign-in from your keychain and asks mcp.notion.com to forget it." Where the service has no revocation (`revokes` false), it instead reads "…To remove Farik completely, also remove it in mcp.notion.com's settings."
- **`SignInDone`**, the tab the loopback listener serves: the Farik mark, "You're signed in to mcp.notion.com. You can close this tab and go back to Farik." Its failure variant reads "Farik couldn't finish signing in: <sentence>. Close this tab and try again in Farik." Plain, with no links and no script.

Gate (O3): the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date. Task 8 does not start until then; Tasks 2 to 7 do not depend on the boards.

- [x] `docs(design): mock up signing in to a service` (boards committed earlier; approved 2026-10-02)

### Task 2: Signing in, in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, ADR 0033. Produces `OAuthSettings`, `CustomTransport::Http.oauth`.

- `accepts_an_http_server_that_signs_in`: `oauth: {}` and `oauth: { client_id: "abc", callback_port: 33418, scopes: ["read"] }` both validate and read back equal.
- `refuses_oauth_where_it_cannot_be`: each code is among the errors at its path: `oauth_on_stdio` at `/agents/0/mcp_servers/0/oauth`; `oauth_with_keys` at `…/credential_keys`; `oauth_header_conflict` at `…/headers/Authorization` and at `…/headers/authorization`; `callback_port_without_client` at `…/oauth/callback_port`.
- `refuses_a_scope_with_a_space_or_quote`: `"a b"` and `"a\"b"` are schema errors at `…/oauth/scopes/0`.
- `a_server_without_oauth_keeps_its_hash`: step 01's `spec_hash_ignores_key_order_and_sees_every_field` fixture gives the same `spec_sha256` as a literal hex string recorded before this change.
- `oauth_settings_change_the_hash`: `oauth: {}` against none, and changing `client_id`, `callback_port` or one scope, each changes `spec_sha256`; `oauth: {}` hashes as `"oauth":{"callback_port":null,"client_id":null,"scopes":[]}` in `canonical_json`.

- [x] `feat(core): let a web-address connector sign in`

### Task 3: The sign-in

Files: `sign_in.rs`, `lib.rs`, the workspace and runtime `Cargo.toml`, `crates/runtime/tests/fixture_oauth.rs`. Produces `start_sign_in`, `SignIn`, `SignInError`, `OAuthGrant`, `SIGN_IN_WINDOW`, `SIGN_IN_START`.

The fixture is one axum server on loopback that plays the protected MCP server (a 401 with `resource_metadata` without a bearer, one tool listed with it), the protected-resource metadata (its `resource` names the server url exactly), and the authorization server's metadata, `/register`, `/authorize`, `/token` and `/revoke`; `/authorize` answers 302 to the `redirect_uri` with `code`, `state` and `iss`. It records every request; flags turn off DCR, S256, `code_challenge_methods_supported` or `iss`, drop `WWW-Authenticate` and the PRM, hold a route until released, or answer `error=access_denied`. A test follows `authorize_url` with redirects off, then requests the `Location`.

- `signs_in_with_dynamic_registration`: `/register` gets `application_type: native`, `token_endpoint_auth_method: none` and `redirect_uris: ["http://localhost:<callback port>/callback"]`; the grant's `client_id` is the registered one, its `issuer` the fixture's, its `resource` the server url, and it holds both tokens.
- `uses_a_preregistered_client_without_registering`: with `client_id` and `callback_port` set, `/register` is never called and `redirect_uri` names that port; without `callback_port`, it names 33418.
- `sends_pkce_s256_and_the_resource`: `/authorize` gets `code_challenge_method=S256`; `/token`'s `code_verifier` hashes to that challenge; both requests carry `resource=<server url>`.
- `refuses_a_server_without_s256`: metadata listing `["plain"]`, and metadata with no `code_challenge_methods_supported`, each give `PkceNotSupported`, and the fixture saw no `/register`.
- `refuses_with_neither_registration_nor_client`: `NotSupported`.
- `says_not_offered_without_resource_metadata`: an MCP server answering 401 with no `WWW-Authenticate` and no well-known document gives `NotOffered`.
- `does_not_guess_endpoints`: an MCP server answering 401 with no `WWW-Authenticate` and no PRM, but serving `/.well-known/oauth-authorization-server` and `/register` at its origin, gives `NotOffered`, and the fixture saw no `/register`.
- `refuses_the_wrong_state`: a callback with another `state` gives 400, the attempt still completes with the right callback, and `/token` was called once.
- `refuses_another_issuer_or_a_missing_one_when_promised`: `iss` of another value gives `Mismatch`; no `iss` with `authorization_response_iss_parameter_supported: true` gives `Mismatch`; no `iss` without that flag signs in; an `error=access_denied` callback with another `iss` gives `Mismatch`, not `Denied`.
- `reports_access_denied`: `Denied("access_denied")`.
- `answers_one_callback_then_closes`: after the callback, connecting to `callback_addr` is refused; a `GET /other` before it gets 404 and the attempt still completes.
- `the_callback_page_quotes_nothing`: an `error_description` of `<b>x</b>` does not appear in the page, which carries the four headers.
- `listens_on_loopback_only`: `callback_addr().ip().is_loopback()`.
- `says_when_the_callback_port_is_taken`: a fixed `callback_port` already bound gives `Failed` naming the port.
- `gives_up_after_ten_minutes`: with a paused clock, `finish` gives `TimedOut`.
- `gives_up_starting_after_fifteen_seconds`: with a paused clock and the fixture's PRM held, `start_sign_in` gives `Failed`.
- `refuses_an_endpoint_that_is_not_https`: metadata naming `http://auth.example/authorize` gives `Failed`, naming the endpoint; so does a metadata request redirected to an `http` non-loopback URL.

- [x] `feat(runtime): sign in to an MCP server's service with OAuth`

### Task 4: Keeping, refreshing and revoking a grant

Files: `connectors.rs`, `sign_in.rs`. Produces `ConnectorEntry.oauth`, `refreshed`, `revoke`, the new `list_tools`.

- `an_oauth_entry_round_trips_and_never_prints`: the stored form reads back equal; `Debug` of the entry and of `OAuthGrant` shows neither token.
- `an_entry_stored_before_has_no_oauth`: `{"spec_sha256":"…","keys":{}}` loads with `oauth: None`.
- `launch_headers_send_the_bearer`: an entry with a grant and the header `X-Workspace: a` gives both, `Authorization` being `Bearer <access token>`.
- `lists_tools_with_the_signed_in_token`: against the fixture, `list_tools` with the bearer lists its tool, and without it fails.
- `refreshes_a_token_about_to_expire`: a grant expiring in 4 minutes with `valid_for` 35 minutes gives `Some`, with the fixture's new access token and its rotated refresh token.
- `leaves_a_fresh_token_alone`: a grant expiring in 2 hours gives `None`, and `/token` is not called.
- `a_refused_refresh_lapses`: `/token` answering `invalid_grant`, `invalid_client` or `unauthorized_client` gives `Lapsed`; answering 500, or held past `timeout`, gives `Failed`.
- `a_grant_without_a_refresh_token_lapses_once_expired`: valid, it gives `None`; expired, `Lapsed`, and `/token` is not called.
- `refresh_sends_the_kept_resource`: `/token` gets `resource=<grant.resource>` and `client_id`; an answer without `refresh_token` keeps the old one.
- `revokes_the_refresh_token`: `/revoke` gets the refresh token with `token_type_hint=refresh_token`; a grant without one sends the access token; a `/revoke` answering 500 is not an error.

- [x] `feat(runtime): keep, refresh and revoke an agent's sign-in`

### Task 5: Signed-in connectors in sessions

Files: `orchestrator/session.rs`, `daemon.rs` (`entry_lock`, `Kept::Entry.signed_in`, `Kept::runs`, the launch route).

- `a_session_refreshes_a_token_that_would_expire_during_it`: the fixture's grant, expiring in 10 minutes with a 30-minute `max_wall_clock`, is refreshed before `mcp.json` is written, and the entry kept holds the rotated refresh token.
- `a_lapsed_sign_in_is_left_out`: a refresh answering `invalid_grant` saves `lapsed: true`; the server is absent from `mcp.json` and the registration, and its calls are denied `connector_not_in_session`.
- `launch_refreshes_an_expired_token`: the route answers the new token in `Authorization`.
- `launch_refuses_a_lapsed_sign_in`: 403 `sign_in_again`, and the server is taken from the session.
- `launch_says_when_a_refresh_does_not_finish`: with `/token` held, the route answers 503 `sign_in_failed` within `KEY_STORE_DEADLINE`; once released, the rotated refresh token is kept.
- `two_launches_refresh_once`: two concurrent launches of one expired grant make one `/token` request.
- `remove_during_a_refresh_keeps_nothing`: with `/token` held until released, a `connector.disconnect` sent during a session-setup refresh leaves no entry once both finish.
- `a_changed_sign_in_setting_needs_connecting_again`: `oauth.scopes` changed in `team.yaml` leaves the server out, `connect_again`.
- `a_live_session_calls_a_signed_in_connector` (integration, `--integration`): see Verification.

- [x] `feat(runtime): refresh a sign-in before a session needs it`

### Task 6: Signing in through the daemon

Files: `daemon/team.rs`, `daemon/web.rs`, `orchestrator/human.rs`, `rpc.schema.json`, `event.schema.json`, `protocol/src/event.rs`.

- `sign_in_then_connect_keeps_the_grant`: `connector.sign_in` answers an address and the fixture's `issuer`; after the test follows it, `connector.sign_in_status` is `signed_in`. `connector.tools` with the attempt lists the tool, and `connector.connect` with the attempt and tags keeps a grant. The team file's entry has `oauth: {}`, and `connector.connected` carries `issuer`.
- `no_reply_event_or_log_holds_a_token`: across that test, neither token's text appears in any RPC reply (`connector.sign_in_status`'s among them), in `.farik/local/events.db`, in `team.yaml`, in any file of the session's folder (`mcp.json`, the system prompt), or in the `connector_connect` command body.
- `sign_in_status_says_why_it_failed`: `access_denied` gives `{ state: failed, reason: { code: "access_denied" } }`.
- `an_attempt_is_used_once_and_expires`: a second `connect` with one attempt is `sign_in_unknown`, and so is one past `SIGN_IN_WINDOW` (paused clock).
- `an_attempt_signs_in_one_server_only`: after signing in for the fixture's url, `connector.connect` with that attempt and a `server` whose `url` differs (and, separately, whose name or `oauth.scopes` differ) is `sign_in_unknown`, and no entry is kept.
- `connect_needs_an_attempt_to_sign_in`: `server.oauth` set with `keys` is `sign_in_needed`.
- `a_new_attempt_ends_the_old`: after a second `connector.sign_in` for the agent and server, the first attempt's callback address refuses connections.
- `disconnect_deletes_then_revokes`: the entry is gone and `/revoke` got the refresh token; with `/revoke` answering 500 the entry is still gone and the reply is `{}`.
- `connect_again_revokes_the_replaced_grant`: a second sign-in and connect for the same server sends the first grant's refresh token to `/revoke`.
- `team_get_says_auth_and_sign_in_again`: a lapsed grant gives `state: sign_in_again`, `auth: oauth`; a key server, `auth: keys`; a grant whose metadata had no revocation endpoint, `revokes: false`.

- [ ] `feat(runtime): sign an agent in to a service from the web app`

### Task 7: The command line

Files: `cli/src/connector.rs`, `cli/src/lib.rs`. Produces `connect_with`; the tests pass an opener that follows the address.

- `farik_connect_signs_in_and_keeps_the_grant`: against the fixture, prints `Sign in to 127.0.0.1 in your browser:` with the address, then `Signed in to <issuer>.`, and keeps a grant; the command sent to a running daemon holds no token. With an opener that does nothing (a failed `xdg-open`), the address is still printed and a follow by the test completes it.
- `farik_connect_sign_in_takes_no_key`: `--sign-in --key A` and `--sign-in --command x` are refused by the argument parser.
- `farik_connect_says_when_a_service_offers_no_sign_in`: `NotOffered` prints "<host> does not offer signing in; give its key with --key".

- [ ] `feat(cli): sign an agent in to a service`

### Task 8: The screens

Files: `ConnectorAdd.tsx`, `AgentEdit.tsx`, `connectors.test.tsx`, `strings/en.ts`, `mapping.ts`. Built from Task 1's approved boards.

- `connector_add_offers_sign_in_when_the_service_has_one`: after Next on a web address, the button reads "Sign in with mcp.notion.com", and no key field shows.
- `connector_add_names_who_signs_you_in`: the button names the issuer host; "for mcp.stripe.com" shows when the issuer host differs from the url's, and not when they match.
- `connector_add_lets_a_key_be_used_where_sign_in_is_offered`: "Use a key instead" shows step 01's key fields, and Next sends `connector.tools` with `keys` and no attempt.
- `connector_add_waits_for_the_sign_in_then_labels`: the click calls `window.open(authorize_url, '_blank', 'noopener')` synchronously, with no RPC in between; `sign_in_status` is polled until `signed_in`; `connector.tools` is sent with the attempt and no `keys`.
- `connector_add_says_why_a_sign_in_failed`: one assertion per reason code, each the board's sentence.
- `connector_add_falls_back_to_a_key`: `sign_in_not_offered` shows the key fields alone; `sign_in_not_supported` and `sign_in_failed` show them under their sentence.
- `agent_edit_shows_signed_in_and_sign_in_again`: the two rows; "Sign in again" opens `ConnectorAdd` at the sign-in, filled in.
- `agent_edit_remove_says_the_service_is_asked_to_forget`: the confirmation's sentence for a signed-in row, and the settings sentence when `revokes` is false.

- [ ] `feat(web): sign in to a service from the agent page`

### Task 9: Spec and plan

`docs/SPEC.md`: 6.7 (signing in, the grant per agent, refresh, revocation, "Sign in again"), 8.2 (the bearer through the headers helper), 8.5 (`issuer` on `connector.connected`), 8.6 (the callback's security, tokens never in a file Farik writes but the private store), F9 (the two RPCs, `attempt`, `auth`, `revokes`, `sign_in_again`). `docs/plans/project-plan.md`: phase 7's row 03, corrected if execution changed it. `docs/design/role-kits.md`: its steps table.

- [ ] `docs(spec): record signing in to a service`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_calls_a_signed_in_connector passed
```

`a_live_session_calls_a_signed_in_connector` (Task 5): a real Claude Code session is given the fixture's server with a grant kept. The stream's `system/init` line lists `mcp__fixture__<tool>`, the call succeeds, and the fixture saw `Authorization: Bearer <access token>` from the headers helper. This is also the probe of `headersHelper` that step 01 left undone.

The pull request lists every new dependency with its licence from `cargo tree -e normal -p farik-runtime` run after Task 3, not from this plan's names. The founder's live check, recorded in the pull request: sign in to Linear (`https://mcp.linear.app/mcp`, DCR, which advertises `iss` and revocation) from the web app as one agent, list its tools, run one session that calls a `network` tool, then Remove. Optional second check: sign in to Stripe (`https://mcp.stripe.com`, its sign-in at `access.stripe.com`, no revocation), list its tools read-only, then Remove and read the settings sentence.
