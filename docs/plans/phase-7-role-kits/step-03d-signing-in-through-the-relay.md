# Phase 7, step 03d: Signing in through the relay

Status: deferred by the founder, 2026-10-02: Slack is a later step ("Connecting slack is a later step keep it simple for now"; "Write the slack integeration plan in the last phase"). Reviewed and folded; lands as phase 14 step 03, in its Slack integration (project plan revisions 30 to 32), where it is reviewed for readiness again, moved and renumbered. Where it says phase 11 or phase 12 for Slack's listing, read phase 14 step 04; the website (phase 11 step 01) is live by then.
Branch: `phase/14-premium` when taken up (written on `phase/7-role-kits`)
Spec: `docs/SPEC.md` 6.7, 8.6; F9
Depends on: step 03 (committed before this step starts: the sign-in, the loopback listener, `refreshed`, `OAuthGrant`, the sign-in RPCs and screens), step 03b (committed before this step starts: `RegisteredApp`, `AppFlow`, `app_for`, `REGISTERED_APPS`, `OAuthGrant.app`, `set_registered_apps`, `provider` on `connector.sign_in`), step 03c (committed and deployed before Task 6: the relay's API at `signin.<domain>`)
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 5 Blocking, all folded with the founder's decisions; no second round (ADR 0032)
Mockups approved by: pending (Task 1's gate)

## Goal

After step 03c, Farik's sign-in relay runs, but Farik does not use it. When this step is done, the founder presses "Sign in with Slack" for `https://mcp.slack.com/mcp` in the founder's own Slack workspace, says yes on Slack's page, and the agent is connected; Farik signs in and refreshes through the relay, and keeps the tokens only in the agent's key store, as in step 03. When the relay is down, the user is told so and offered a key. Every other workspace connects Slack with a pasted key until the Slack Marketplace lists Farik, a launch dependency (ADR 0035's amendment; phase 11), after which one line in the table offers the sign-in to everyone. Out of scope: the relay itself (step 03c), and the Scrum Master's Slack scopes beyond 03c's first set (step 06). This step is the local side of step 03c's reviewed plan, split from it when its readiness review was folded; the review's Readiness line covers both.

## Decisions

- **`AppFlow` gains `Relay { relay_url: &'static str, service: &'static str }`.** A Relay entry runs step 03's loopback listener on a free port; Farik makes a 32-byte `relay_verifier` with `rand` (already a dependency), base64url without padding (43 characters), and sends `relay_challenge` = base64url of SHA-256 of the verifier's ASCII, as PKCE does, with `service` and the port, to `<relay_url>/v1/start`. The attempt's `state` is the answered ticket. At the callback, Farik compares `state` with it (step 03's rule) and posts `{ ticket, code, relay_verifier }` to `<relay_url>/v1/token` instead of rmcp's exchange.
- **The grant**: `issuer` the table's, `app: Some(<the entry's id>)`, `client_id` the table's, `token_endpoint` `<relay_url>/v1/refresh` (for display; refresh does not use it, below), `revocation_endpoint` the table's (Slack: `https://slack.com/api/auth.revoke`, which takes the token and no secret), the scopes from the answer's `scope` split on spaces.
- **`RegisteredApp` gains `listed: bool`.** `true` for GitHub. The Slack entry ships `false` while the Slack app is private to the founder's workspace (public distribution off, since Slack's MCP server refuses unlisted distributed apps); phase 11 sets it `true` when Slack lists Farik. `connector.sign_in`'s answer carries `listed` for a table entry. With `listed: false`:
  - `ConnectorAdd` shows step 01's key fields under the line "Slack hasn't listed Farik yet, so connect Slack with a key.", and under them the link "Sign in with Slack instead (works only in Farik's own Slack workspace)", which opens step 03's waiting board for the attempt `connector.sign_in` already answered;
  - the command line's `--sign-in` still signs in, after printing `Slack hasn't listed Farik yet: signing in works only in Farik's own Slack workspace.`
- **`refreshed` takes the table.** For a grant with `app`, it looks the id up: a Relay entry posts `{ service, refresh_token }` to the **table's** `<relay_url>/v1/refresh`, never the stored `token_endpoint`; a Device entry refreshes as step 03b does; an id no longer in the table is `Failed("app not found")` and does not lapse. A grant without `app` refreshes as step 03 does. The call sites in `orchestrator/session.rs` and `daemon.rs` pass the daemon's table (03b's `set_registered_apps`, else `REGISTERED_APPS`), in the same task as the signature change.
- **Calls to the relay** go through step 03's https-or-loopback `reqwest` client, send `Accept: application/json`, follow no redirect, and give up after 10 seconds (`RELAY_TIMEOUT`; the relay answers within 8). No answer, a timeout, a redirect, a 429, a 5xx, or a body that is not the API's is `RelayUnavailable`; `invalid_grant` is `Lapsed`; `invalid_ticket`, `ticket_expired` and `ticket_used` are `Mismatch`; any other `error` is `Failed`.
- **At sign-in**, `RelayUnavailable` is the RPC refusal `relay_unavailable`, and its error carries no address of the relay's. **At refresh**, it is a failure that does not lapse the grant (step 03's rule).
- **The screens** show "Sign in with Slack" (03b's provider name) and, for `relay_unavailable`, "Farik's sign-in service isn't answering. Try again in a few minutes, or use a key instead.", "Try again", and step 01's key fields below. The command line prints the same sentence.

## File map

```
docs/design/mockups/ConnectorAdd.dc.html, canvas.json                                  Task 1
crates/runtime/src/relay.rs, lib.rs                                                    creates: the relay client (Task 2)
crates/runtime/src/registered_apps.rs, sign_in.rs                                      modifies: Relay flow, listed; refreshed takes the table (Tasks 2, 3)
crates/runtime/src/orchestrator/session.rs, daemon.rs                                  modifies: refreshed's call sites (Task 3)
crates/runtime/tests/fixture_relay.rs                                                  creates: a fake relay on loopback (Task 2)
crates/runtime/src/daemon/team.rs, docs/schemas/rpc.schema.json, packages/protocol-client/src/mapping.ts, crates/cli/src/connector.rs   modifies: relay_unavailable, listed (Task 4)
apps/web/src/pages/ConnectorAdd.tsx, connectors.test.tsx, strings/en.ts                modifies (Task 5)
registered_apps.rs, docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md  Task 6
```

## Interfaces

Consumes: step 03's listener, `SignIn`, `OAuthGrant`, `refreshed`, `SignInError`; step 03b's `RegisteredApp`, `AppFlow`, `app_for`, `REGISTERED_APPS`, `set_registered_apps`; step 03c's relay API (`/v1/start`, `/v1/token`, `/v1/refresh` and their answers).

Produces:

```rust
// farik-runtime
pub enum AppFlow { /* 03b's */ Relay { relay_url: &'static str, service: &'static str } }
pub struct RegisteredApp { /* 03b's fields */ pub listed: bool }
pub const RELAY_TIMEOUT: Duration = Duration::from_secs(10);
pub async fn refreshed(grant: &OAuthGrant, apps: &[RegisteredApp], now: DateTime<Utc>, valid_for: Duration,
    timeout: Duration) -> Result<Option<OAuthGrant>, SignInError>;
pub enum SignInError { /* 03b's */ RelayUnavailable }
impl SignIn { pub fn listed(&self) -> Option<bool>; }  // Some for a table entry
```

Wire: `connector.sign_in → { …03b's, listed? }`, refused also `relay_unavailable`.

## Tasks

### Task 1: The relay's sign-in screens, mocked up

On the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), desktop and phone width, the canvas's tokens, muted and light, copied to `docs/design/mockups/`:
- **`ConnectorAdd`, Slack not yet listed**: `https://mcp.slack.com/mcp`, "Slack hasn't listed Farik yet, so connect Slack with a key.", step 01's key fields, and the link "Sign in with Slack instead (works only in Farik's own Slack workspace)".
- **`ConnectorAdd`, Slack offered** (once listed): "Slack lets you sign in.", "Sign in with Slack", "Use a key instead".
- **`ConnectorAdd`, relay unavailable**: the muted error line "Farik's sign-in service isn't answering. Try again in a few minutes, or use a key instead.", "Try again", and step 01's key fields below.

Gate: the founder approves the boards, or says to approve them automatically; the approval and its date go into this plan's header. Task 5 waits for it.

- [ ] `docs(design): mock up signing in through the relay`

### Task 2: Signing in through the relay

Files: `relay.rs`, `lib.rs`, `registered_apps.rs`, `sign_in.rs`, `fixture_relay.rs`. Produces `AppFlow::Relay`, `RegisteredApp.listed`, `RELAY_TIMEOUT`, `SignInError::RelayUnavailable`, `SignIn::listed`. Every existing table entry, the shipped GitHub entry and the tests' tables, gains `listed: true`.

The fixture is an axum server on loopback implementing the relay's routes over its own HMAC key and an in-memory used set, with the relay's checks. Its `/v1/start` answers an `authorize_url` at its own `/authorize`, which 302s to `<fixture>/v1/callback/<id>?code=c-<n>&state=<ticket>`; its `/v1/callback/{id}` 302s to the listener as the relay does. Flags: answer 503, hang, answer 302 to another of its paths, answer `invalid_grant`, or bounce with another `state`. The tests' table has a Relay entry, id `relay-test`, `listed: false`, pointing at it with host `127.0.0.1`.

- `signs_in_through_the_relay`: `/v1/start` got the listener's port and a 43-character challenge; following `authorize_url` reaches the listener through the fixture's callback; `/v1/token` got the ticket, the code and a verifier hashing to the challenge; the grant has `app: Some("relay-test")`, `token_endpoint` `<fixture>/v1/refresh`, and the table's `revocation_endpoint`; `listed()` is `Some(false)`.
- `refuses_a_bounce_with_another_state`: 400 at the listener, and `/v1/token` is not called.
- `says_when_the_relay_is_down`: a closed port, a 503, and a hang past `RELAY_TIMEOUT` (paused clock) each give `RelayUnavailable` from `start_sign_in`.
- `never_follows_a_relay_redirect`: a relay answering 302 to `/v1/start` gives `RelayUnavailable`, and the redirect target saw no request.
- `maps_the_relay_s_refusals`: `/v1/token` answering `ticket_used` gives `Mismatch`, and `invalid_grant` gives `Lapsed`.

- [ ] `feat(runtime): sign in through Farik's sign-in relay`

### Task 3: Refreshing through the relay

Files: `sign_in.rs`, `orchestrator/session.rs`, `daemon.rs`, and step 03's and 03b's tests that call `refreshed`, which pass a table. Produces the new `refreshed`.

- `refreshes_through_the_relay`: `refreshed` with the table posts `{ service, refresh_token }` to the table's `/v1/refresh`, not to the grant's stored `token_endpoint` (edited in the test to another port that records any request), and keeps the new tokens.
- `a_refresh_the_relay_cannot_make_does_not_lapse`: with the relay answering 503 and the token still valid, `refreshed` gives `Failed`, and the entry's `lapsed` stays false; `invalid_grant` gives `Lapsed`.
- `a_grant_for_a_removed_app_does_not_lapse`: a grant with `app: Some("gone")` and an empty table gives `Failed("app not found")`, makes no request, and does not lapse.

- [ ] `feat(runtime): refresh through Farik's sign-in relay`

### Task 4: The daemon and the command line

Files: `daemon/team.rs`, `rpc.schema.json`, `mapping.ts`, `cli/src/connector.rs`. The daemon's tests use `set_registered_apps` with the fixture's leaked table.

- `sign_in_says_the_relay_is_unavailable`: `connector.sign_in` against a down fixture is refused `relay_unavailable`, and the error carries no address of the relay's.
- `sign_in_says_whether_the_app_is_listed`: against the Relay entry, `connector.sign_in` answers `listed: false`; against a Device entry with `listed: true`, `listed: true`; against a step 03 server, no `listed`.
- `a_session_with_the_relay_down_leaves_the_server_out`: an expired relay grant, the fixture answering 503: the server is absent from `mcp.json`, and `team.get` does not say `sign_in_again`.
- `mapping_names_listed`: `listed` maps both ways (Vitest).
- `farik_connect_says_the_relay_is_unavailable`: prints the Decisions' sentence.
- `farik_connect_says_slack_is_not_listed`: `--sign-in` against the Relay entry prints the not-listed sentence before the address, and still signs in.

- [ ] `feat(runtime): say when the sign-in relay is down`

### Task 5: The screens

Files: `ConnectorAdd.tsx`, `connectors.test.tsx`, `strings/en.ts`. From Task 1's approved boards.

- `connector_add_falls_back_when_the_relay_is_down`: `relay_unavailable` shows the sentence, "Try again", and the key fields; "Try again" sends `connector.sign_in` again.
- `connector_add_offers_a_key_first_while_unlisted`: an answer with `listed: false` shows the not-listed line and the key fields, and the link "Sign in with Slack instead (works only in Farik's own Slack workspace)" calls `window.open(authorize_url, '_blank', 'noopener')` synchronously in the click and shows step 03's waiting board; with `listed: true` the page is step 03b's offered board.

- [ ] `feat(web): sign in to Slack through the relay`

### Task 6: Slack through the relay, live

Gate: step 03c's live check has passed, and the founder gives the domain and Slack's client id in conversation (both public).

Files: `registered_apps.rs` (the entry `slack`, `Slack`, host `mcp.slack.com`, `Relay { relay_url: "https://signin.<domain>", service: "slack" }`, `client_id` Slack's, issuer `https://mcp.slack.com`, `token_endpoint` `https://signin.<domain>/v1/refresh`, `revocation_endpoint` `Some("https://slack.com/api/auth.revoke")`, `install_url` `None`, `listed: false`); `docs/SPEC.md` (6.7: the relay route and `listed`; F9: `relay_unavailable`, `listed`); `docs/plans/project-plan.md` (row 03d, corrected if execution changed it); `docs/design/role-kits.md` (its steps table).

- `the_shipped_table_names_slack_through_the_relay`: the entry exists, `listed` is false, and its `relay_url` and endpoints are `https`.

- [ ] `feat(runtime): sign in to Slack through the relay`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```

The founder's live check, recorded in the pull request, in the founder's own Slack workspace: from the web app, as the Scrum Master, open `https://mcp.slack.com/mcp`, see the not-listed line, choose "Sign in with Slack instead", say yes, list its tools, and run one session that reads a channel. The kept grant holds a refresh token and an `expires_at` about 12 hours out. More than 12 hours later, start another session: it refreshes through `/v1/refresh` (one `/v1/refresh` line in the log group) and the session reads the channel. If Slack gave no refresh token, the step stops and the planner decides. The function's log group holds only six-field lines. Then Remove.
