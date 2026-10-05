# Phase 7, step 08e: Signing in with Google, for Google Ads

Status: draft. Its readiness review runs once step 08d has landed and step 03b's Tasks 2 to 6 are committed (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 8.6; F9
Depends on: step 03b of this phase through its Task 6 (`RegisteredApp`, `AppFlow`, `REGISTERED_APPS`, `app_for`, `start_sign_in` taking the table, `OAuthGrant.app`, the app refresh, `set_registered_apps`, `SignedIn.provider`, `provider` on `connector.sign_in` and `team.get`), which has not run yet; step 03 (the loopback sign-in, `OAuthGrant`, `refreshed`, `refreshed_entry`); step 07 (Farik's own connectors, ADR 0038); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

ADR 0042 pulls Google's route forward from after the launch for one scope, `https://www.googleapis.com/auth/adwords`, so the Marketing Specialist's Google Ads connection (step 08f) signs in with Google through Farik's own Google app, with nothing pasted. ADR 0035's decided Google design holds: a Desktop client in a Google Cloud project the founder owns, PKCE, its secret shipped as a non-confidential value, `iss` required, no revocation. What changes is where it is used: not for a Google MCP server at an address, but for Farik's own `stdio` connector, which never receives the sign-in (08f). After this step a team entry `farik connector <name>` may carry `oauth`, Farik signs it in with the registered app the table names for that connector, keeps, refreshes and lists it as step 03 does, and Remove says where to remove Farik in Google's settings. Out of scope: the `google-ads` connector and how its calls use the grant (08f); every other Google scope and every Google MCP server (after the launch, ADR 0035); Google's verification of the scope, which is a launch dependency below.

## Decisions

- **Verification waits for the launch site** (the founder, 2026-10-05): Google's verification of the scope needs a homepage and a privacy policy on Farik's verified domain, which the web launch's website (phase 11) provides; until then the founder and named test users sign in, again every 7 days. Rejected: moving step 03e's homepage forward into phase 7.
- **What Google says, read 2026-10-05** (each page's last update in brackets):
  - Developer tokens: "Developer tokens were sunset on September 9, 2026"; sending one "is optional and ignored by the API servers"; "Your API access levels are determined by the Google Cloud project you used to generate your OAuth credentials" (developers.google.com/google-ads/api/docs/api-policy/developer-token [2026-09-30]). The founder's claim holds: no developer token, and Farik sends none.
  - Access levels: a project that enables the Google Ads API gets Test access, which reaches test accounts only; Explorer (on applying in the console, possibly granted at once) reaches production accounts with 2,880 operations a day and blocks `KeywordPlanIdeaService`, user management, account creation and billing; Basic (15,000 a day, both kinds of account) needs the project's brand verification first and about 10 business days; Standard is unlimited after a manual audit (…/api-policy/access-levels [2026-09-30]). Every `Search` or `SearchStream` request is one operation, mutates count one per operation, and the limit is per Google Cloud project, shared by every user of Farik's app (…/best-practices/quotas [2026-09-30]).
  - Sign-in: the desktop client's loopback redirect may use `localhost` in place of `127.0.0.1`, with a path, on any port; PKCE S256; authorization `https://accounts.google.com/o/oauth2/v2/auth`, token `https://oauth2.googleapis.com/token`; revoking "removes all OAuth 2.0 scopes previously granted to a project … for all clients registered under that project" (developers.google.com/identity/protocols/oauth2/native-app [2026-09-14]). The client secret is required in practice (ADR 0035's probe).
  - The scope is sensitive. Verification needs authorised domains verified in Search Console, a public homepage and a privacy policy on that domain linked from the consent screen, brand verification, a justification and a demo video, and takes 3 to 5 business days; until then the app is in Testing: a warning screen, at most 100 named test users, and refresh tokens that end after 7 days (…/production-readiness/sensitive-scope-verification [2026-08-19]).
- **A Farik connector may sign in.** `CustomTransport::Stdio` gains `oauth: Option<OAuthSettings>`; `validate_team` keeps `oauth_on_stdio` for every `stdio` entry but the exact pair `command: farik`, `args: [connector, <one word>]` (whose word the kit loader holds to `FARIK_CONNECTORS`, ADR 0038), and refuses `client_id` or `callback_port` on that pair (`farik_connector_client: Farik's own connector signs in with Farik's own app`), since the app's id is the table's. `spec_sha256` adds `oauth` to a `stdio` definition only when present, so every hash kept stands. `CustomServer::oauth()` answers either transport's, and the six places that read `CustomTransport::Http { oauth: Some(_), .. }` read it instead: `daemon.rs:225` (`Kept::runs`), `daemon.rs:1141` (the launch route), `orchestrator/session.rs:356` (the refresh at setup), `daemon/team.rs:367` and `:788` (`team.get`, `signs_in`), `cli/src/connector.rs:213`. The kit's `stdio` shape (`kit.rs`, `shape`) allows `oauth`. Rejected: an `app` field of its own on the entry, a second way of saying "signed in" (ADR 0033 chose one); and an `http` entry at a Google host, which would hand the access token to a server process.
- **Table changes to step 03b's `RegisteredApp`.** `host` becomes `Option<&'static str>` (`app_for` skips an entry without one); new fields `farik_connector: Option<&'static str>`, the one Farik connector it signs in for; `client_secret: Option<&'static str>`, sent on the exchange and on each refresh, looked up by `grant.app`, never kept in the grant (ADR 0035); `scopes: &'static [&'static str]`, the only scopes it asks for. `AppFlow` gains `Loopback { authorization_endpoint: &'static str }`. `app_for_farik_connector(apps, command, args)` matches the exact pair whose word is an entry's `farik_connector`, and nothing else. A `stdio` entry with `oauth` that no entry names is `sign_in_not_supported`. The team file's `oauth.scopes`, when given, must each be among the entry's (`Failed("Farik's Google sign-in asks only for Google Ads")`), and empty means the entry's.
- **Google's entry** (Task 6, the founder's values): id `google`, name `Google`, no `host`, `farik_connector: Some("google-ads")`, `Loopback` at `https://accounts.google.com/o/oauth2/v2/auth`, the Desktop client's id and secret, issuer `https://accounts.google.com`, token `https://oauth2.googleapis.com/token`, no revocation, no install address, `scopes: ["https://www.googleapis.com/auth/adwords"]`. The connector name is data until 08f ships it; nothing in this step starts it.
- **The loopback flow is Farik's own requests**, not `rmcp`'s, since there is no MCP server whose metadata to discover: step 03's listener (`http://localhost:<free port>/callback`, `127.0.0.1` and `[::1]`, five tries, one `GET /callback` with the attempt's `state`, the plain page, ten minutes); the authorization address with `client_id`, `redirect_uri`, `response_type=code`, `scope` (space-joined), `state`, `code_challenge` (S256 of a 64-character verifier) and `code_challenge_method=S256`; no `access_type` and no `resource` (ADR 0035; there is no MCP resource). The callback must carry `iss` equal to the entry's issuer, else `Mismatch`; `error=access_denied` is `Denied`. The exchange posts `code`, `client_id`, `client_secret`, `redirect_uri`, `grant_type=authorization_code` and `code_verifier` with `Accept: application/json`. An answer with no `refresh_token` fails ("Google did not give Farik a lasting sign-in"); one whose `scope` lacks a scope asked for fails ("you did not allow Farik to manage your Google Ads; sign in again and tick it"). The grant: the entry's issuer and token endpoint, `resource` `https://googleads.googleapis.com/` (bookkeeping, as 03b keeps the server's address), the client id, both tokens, `expires_at` from `expires_in`, the granted scopes, no revocation endpoint, `app: Some("google")`. Every endpoint passes step 03's https-or-loopback check.
- **Refresh** is 03b's app refresh (no `resource`, `Accept: application/json`, `error` read at any status) with the table's `client_secret` added for a grant whose app has one; `invalid_grant` lapses it, which is how a Testing app's seven-day refresh tokens end: "Sign in again" on the agent's page. **No revocation**: Remove deletes the local grant only (ADR 0035, Google's revocation ends every agent's grant at once), and its confirmation is 03b's sentence naming Google's settings.
- **Where the grant goes.** Nowhere but the daemon: the launch route answers a signed-in Farik connector with its command and an empty environment (08f adds its ticket), never a token; session setup refreshes it as it does an `http` one (`valid_for` the session's wall clock plus five minutes), so 08f's calls start from a fresh grant; `team.get` says `auth: oauth`, `provider: "Google"`, `revokes: false`.
- **No new screens and no mockups.** `KitConnect` already signs in when the kit service's `auth` is `oauth` (`apps/web/src/pages/KitConnect.tsx:50`), and step 03b names the provider on its button and its row.
- **The founder's actions** (no agent creates an account or holds the founder's credentials), before Task 6, in the Google Cloud console:
  - [ ] A Google Cloud project for Farik, owned by the founder; the Google Ads API enabled (Test access follows).
  - [ ] Google Auth Platform: branding with the app name "Farik", a support address and a developer contact; audience External, publishing status Testing, the founder's own Google accounts as test users; data access with the one scope `https://www.googleapis.com/auth/adwords`.
  - [ ] A client of type "Desktop app"; give its client id and client secret in conversation. The secret ships in Farik as a non-confidential value (ADR 0035).
  - [ ] Apply for Explorer access on the console's Google Ads API page, so the project reaches a real ads account; record the level granted in this plan's Execution notes.
- **Launch dependencies, not this phase's**, recorded in the project plan's phase 11: the scope's verification, which needs the homepage and privacy policy on Farik's verified domain (step 03e is deferred to phase 15 and the website is phase 11 step 01, so verification can start only once one of them is live), the policy's Google section (which Google data is read, why, Limited Use and no AI training, ADR 0035's 03e note), brand verification, a demo video and 3 to 5 business days; Basic access after brand verification (keyword ideas, 15,000 operations a day); Standard access before the launch's users would exceed Basic's shared quota (08g's spend tick alone uses 96 operations a day per active plan, so Basic carries about 150 active plans).

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs                          modifies: oauth on the pair, CustomServer::oauth, the hash (Task 1)
crates/runtime/src/{daemon.rs,daemon/team.rs,orchestrator/session.rs,connectors.rs}, crates/cli/src/connector.rs   modifies: the six reads and the Stdio patterns (Task 1)
crates/runtime/tests/{fixture_mcp.rs,live_kit_pins.rs}                           modifies: their Stdio patterns (Task 1)
crates/roles/src/kit.rs                                                          modifies: the stdio shape allows oauth (Task 1)
crates/runtime/src/registered_apps.rs                                            modifies: host, farik_connector, client_secret, scopes, Loopback (Task 2); Google's entry (Task 6)
crates/runtime/src/sign_in.rs, crates/runtime/tests/{fixture_oauth.rs,support/oauth_fixture.rs}   modifies: start_app_sign_in and the Google-shaped server (Task 3), refresh (Task 4)
crates/runtime/src/daemon/{signed_in.rs,team.rs}, daemon.rs, orchestrator/session.rs   modifies: the daemon's sign-in, launch and setup (Task 5)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md               modifies (Task 7)
```

## Interfaces

Consumes: `OAuthSettings`, `CustomServer`, `CustomTransport`, `spec_sha256`, `validate_team` (`farik-core`); `RegisteredApp`, `AppFlow`, `app_for`, `start_sign_in`, `SignIn`, `OAuthGrant` (with `app`), `refreshed`, `revoke`, `SignInError`, `SIGN_IN_WINDOW`, the loopback listener, `begin_sign_in`, `refreshed_entry`, `Kept`, `SignedIn` (03, 03b); `is_farik_connector`, `FARIK_CONNECTORS` (`farik-roles`).

Produces:

```rust
pub enum CustomTransport { Stdio { command: String, args: Vec<String>, oauth: Option<OAuthSettings> },
    Http { url: String, headers: BTreeMap<String, String>, oauth: Option<OAuthSettings> } }          // farik_core::team
impl CustomServer { pub fn oauth(&self) -> Option<&OAuthSettings>; }
pub enum AppFlow { Device { device_endpoint: &'static str, verification_uri: &'static str },
    Loopback { authorization_endpoint: &'static str } }                                              // registered_apps
pub struct RegisteredApp { pub id: &'static str, pub name: &'static str, pub host: Option<&'static str>,
    pub farik_connector: Option<&'static str>, pub flow: AppFlow, pub client_id: &'static str,
    pub client_secret: Option<&'static str>, pub scopes: &'static [&'static str], pub issuer: &'static str,
    pub token_endpoint: &'static str, pub revocation_endpoint: Option<&'static str>, pub install_url: Option<&'static str> }
pub fn app_for_farik_connector<'a>(apps: &'a [RegisteredApp], command: &str, args: &[String]) -> Option<&'a RegisteredApp>;
pub async fn start_app_sign_in(app: &RegisteredApp, scopes: &[String], now: DateTime<Utc>) -> Result<SignIn, SignInError>;   // sign_in
```

## Tasks

### Task 1: Farik's own connector may sign in

One commit: the variant's new field makes every `CustomTransport::Stdio { command, args }` pattern (sixteen today, in `core/src/team.rs`, `roles/src/kit.rs`, `runtime/src/{daemon.rs,connectors.rs,orchestrator/session.rs}` and `runtime/tests/{fixture_mcp.rs,live_kit_pins.rs}`) fail to compile until it has its `..` or `oauth`. `team.schema.json`'s `oauth` description names the pair.

- `the_farik_connector_may_sign_in` (`team.rs`): `farik connector osv` with `oauth: { scopes: [a] }` validates, and `custom_server` gives it `oauth()`. RED.
- `another_stdio_server_may_not`: `npx x@1.0.0`, `farik serve`, and `farik connector a b` with `oauth` are each `oauth_on_stdio` at `…/oauth`. RED.
- `the_farik_connector_takes_no_client_of_its_own`: `client_id` on the pair is `farik_connector_client` at `…/oauth/client_id`, `callback_port` at its field. RED.
- `a_stdio_server_without_oauth_keeps_its_hash`: step 01's stdio fixture hashes to the literal hex recorded before this change; adding `oauth: {}` changes it. RED.
- `the_kit_takes_oauth_on_its_farik_connector` (`kit.rs`): a fixture kit's `farik connector osv` entry with `oauth` loads; on `npx x@1.0.0` it is `oauth_on_stdio`. RED.

- [ ] `feat(core): let Farik's own connector sign in`

### Task 2: The table, for a Farik connector

- `matches_a_farik_connector_by_its_exact_pair`: an entry with `farik_connector: Some("ads")` matches `farik` with `[connector, ads]` only; not `[connector, ads, x]`, `farik-ads`, `/usr/bin/farik`, or `[connector, osv]`. RED.
- `an_entry_without_a_host_matches_no_address`: `app_for` of any address skips it; 03b's GitHub-shaped tests still pass with `host: Some(…)`. RED.
- `the_shipped_table_has_no_google_yet`: no entry has `farik_connector`. Task 6 replaces it.

- [ ] `feat(runtime): let a registered app serve one of Farik's connectors`

### Task 3: Signing in with Farik's Google app

`tests/support/oauth_fixture.rs` (which `fixture_oauth.rs` and the daemon's own tests both include) gains a Google-shaped server: `/o/oauth2/v2/auth` answering 302 to the redirect with `code`, `state` and `iss`; `/token` requiring `client_secret` and the verifier; flags drop `iss`, the refresh token, or a scope from the answer. The tests' table points a `Loopback` entry, id `google-test`, name `Google test`, `farik_connector: Some("osv")`, at it over loopback.

- `signs_in_with_pkce_and_the_secret`: the address carries `client_id`, `redirect_uri` `http://localhost:<port>/callback`, `response_type=code`, the scope, `state` and S256; no `resource` or `access_type`; `/token` got `client_secret`, the verifier hashing to the challenge, and `Accept: application/json`; the grant has both tokens, `app: Some("google-test")`, the table's issuer, no revocation endpoint. RED.
- `requires_iss`: a callback without `iss`, and one with another, give `Mismatch`. RED.
- `asks_only_the_table_s_scopes`: settings asking for another scope give `Failed`, and the fixture saw nothing. RED.
- `refuses_a_sign_in_that_does_not_last_or_lacks_the_scope`: no refresh token, and a narrower granted scope, each `Failed` with its sentence. RED.
- `reports_access_denied` and `answers_one_callback_then_closes`, as step 03's. RED each.

- [ ] `feat(runtime): sign in with Farik's own Google app`

### Task 4: Keeping a Google sign-in

- `refreshes_with_the_secret_and_without_resource`: a grant with `app: Some("google-test")` refreshes; `/token` got `client_secret`, `grant_type=refresh_token`, no `resource`. RED.
- `an_expired_test_sign_in_lapses`: `invalid_grant` gives `Lapsed`. RED.
- `a_google_grant_is_not_revoked`: `revoke` makes no request. Guard (03b's rule for a grant with no revocation endpoint).

- [ ] `feat(runtime): refresh a Google sign-in`

### Task 5: The daemon signs a Farik connector in

`begin_sign_in` (`daemon/signed_in.rs:318`) takes the pair through `app_for_farik_connector`; `team.get`; the launch route; the setup refresh. The test table is Task 3's (`set_registered_apps`), and the daemon's own program (`set_own_program`) is an executable wrapper the test writes, which runs `tests/fixtures/mcp_server.sh` whatever its arguments, so `farik connector osv` lists that fixture's tools.

- `signs_in_and_connects_a_farik_connector`: `connector.sign_in` for `farik connector osv` with `oauth` answers the address and `provider: "Google test"`; after the test follows it, `connector.connect` with the attempt lists the fixture's tools and keeps a grant. RED.
- `the_grant_never_leaves_the_daemon`: the launch route's answer for it holds no token and an empty environment, and neither token is in `mcp.json`, an event, or a reply. RED.
- `a_session_refreshes_it_and_leaves_it_out_when_lapsed`: a grant expiring within the wall clock is refreshed before `mcp.json` is written; a lapsed one leaves the server out, and `team.get` says `sign_in_again`, `provider: "Google test"`, `revokes: false`. RED.

- [ ] `feat(runtime): sign an agent's own Farik connector in through a registered app`

### Task 6: Farik's Google app, live

Gate: the founder's actions in Decisions are done and the founder gives the client id and secret in conversation. The executor commits them; it never signs in to the founder's accounts.

- `the_shipped_table_names_google_for_google_ads_only`: an entry `google` with `farik_connector: Some("google-ads")`, no `host`, the two Google endpoints exactly, `scopes` exactly the one, a non-empty client id and secret; no other entry has a `farik_connector`. Replaces `the_shipped_table_has_no_google_yet`; if step 03b's Task 7 has not run, its `the_shipped_table_is_empty_until_the_founder_registers` becomes `the_shipped_table_serves_no_address_yet` (no entry has a `host`) in this commit.

- [ ] `feat(runtime): ship Farik's Google client`

### Task 7: Spec and plan

`docs/SPEC.md` 6.7 (Farik's own connector signing in through a registered app; Google's app, its one scope, Testing's seven-day sign-ins and the warning page), 8.6 (the secret is not confidential; the grant never leaves the daemon; no revocation); the revision line. `docs/design/role-kits.md` (the Signing-in row for `google-ads`, route 2). Project plan row 08e, and phase 11's launch dependencies (Decisions' last item).

- [ ] `docs(spec): record signing in with Google for Google Ads`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

The live sign-in with Google is checked in step 08g's verification, the founder's live check of steps 08e to 08g, once `google-ads` exists to sign in for; this step claims no live check.

## Execution notes

None yet.
