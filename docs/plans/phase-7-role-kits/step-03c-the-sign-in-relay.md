# Phase 7, step 03c: The sign-in relay

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7, 8.6, 9; F9
Depends on: step 03 (committed before this step starts: the sign-in, the loopback listener, `refreshed`, `OAuthGrant`, the sign-in RPCs and screens), step 03b (committed before this step starts: `RegisteredApp`, `AppFlow`, `app_for`, `REGISTERED_APPS`, `OAuthGrant.app`, `provider` on `connector.sign_in`)
Readiness: pending: a readiness review by another Opus session
Mockups approved by: pending (Task 1's gate)

## Goal

After step 03b, Slack still cannot be signed in to: its MCP server's authorization server takes only `client_secret_post`, and no program on the user's computer can keep a secret. When this step is done, a user presses "Sign in with Slack" for `https://mcp.slack.com/mcp`, says yes on Slack's page, and the agent is connected. Farik's sign-in relay at `https://signin.<domain>`, a small function on AWS that the founder deploys, adds Farik's Slack client secret to the code exchange and to each refresh and hands the answer straight back. The tokens are kept only in the agent's key store, as in step 03. When the relay is down, the user is told so and offered a key. Out of scope: incoming hooks (ADR 0035's route 4, premium), the website on the same domain (phase 11 step 01), and the Scrum Master's choice of Slack scopes beyond the first set below (step 06).

## Decisions

ADR 0035 records the relay's shape and its security: the ticket, the limits, TLS, the secrets, what happens when it is down, and what it logs. This plan builds it and does not restate the reasons.

The relay's API (`snake_case` JSON, `Cache-Control: no-store` and `Strict-Transport-Security: max-age=31536000` on every answer, no CORS):
- `POST /v1/start` `{ service, port, relay_challenge }` → 200 `{ ticket, authorize_url }`. `port` is an integer from 1024 to 65535, `relay_challenge` matches `^[A-Za-z0-9_-]{43}$`. The `authorize_url` is the service's, with Farik's `client_id`, `redirect_uri` `https://signin.<domain>/v1/callback`, the service's fixed scopes and `state` = the ticket.
- `GET /v1/callback?code&state` or `?error&state` → 302 to `http://localhost:<port>/callback?code=<code>&state=<ticket>`, or `?error=<error>&state=<ticket>` where `error` matching `^[a-z_]{1,64}$` is kept and anything else becomes `server_error`. A bad or expired ticket answers 400 with the relay's page (Task 1), `Content-Type: text/html; charset=utf-8`, `Content-Security-Policy: default-src 'none'`, `Referrer-Policy: no-referrer`. The callback does not use up the ticket.
- `POST /v1/token` `{ ticket, code, relay_verifier }` → checks the signature, the expiry, `BASE64URL(SHA-256(relay_verifier)) == challenge` (constant-time), then marks the ticket used, then exchanges. `relay_verifier` matches `^[A-Za-z0-9_-]{43}$`; `code` is 1 to 512 printable ASCII characters.
- `POST /v1/refresh` `{ service, refresh_token }` → refreshes. `refresh_token` is 1 to 2048 printable ASCII characters.
- Answers to `/v1/token` and `/v1/refresh`: 200 `{ access_token, token_type: "bearer", expires_in?, refresh_token?, scope? }`, else `{ error }`: 400 `bad_request`, `unknown_service`, `invalid_ticket` (signature, challenge or a field), `ticket_expired`, `ticket_used`, `invalid_grant`; 502 `provider_unavailable`; 404 `not_found` for any other route. A body over 8 KiB is `bad_request`.

The ticket: `BASE64URL(payload JSON) "." BASE64URL(HMAC-SHA256(key, that first part))`, the payload `{ v: 1, id: <16 random bytes, base64url>, service, port, challenge, exp: <unix seconds, now + 600> }`. The key is the `farik/relay/ticket-key` secret. Single use is a DynamoDB conditional put of `{ id, expires_at }` with `attribute_not_exists(id)`; the table's TTL attribute is `expires_at`.

The services table, `infra/relay/services.ts`, has one entry: `slack`, authorization `https://slack.com/oauth/v2_user/authorize`, token `https://slack.com/api/oauth.v2.user.access` (both from `mcp.slack.com`'s metadata, read 2026-10-02), scopes `channels:read,channels:history,chat:write,users:read`, no PKCE (Slack's PKCE mode is one-way and makes the app public). The exchange posts `client_id`, `client_secret`, `code`, `redirect_uri`; the refresh posts `client_id`, `client_secret`, `grant_type=refresh_token`, `refresh_token`. Slack's answer is read as: `ok: false` with `error` in `invalid_code`, `code_already_used`, `invalid_refresh_token`, `token_expired`, `token_revoked`, `invalid_grant` gives `invalid_grant`; any other `ok: false`, a non-JSON body, an HTTP status of 500 or more, or 8 seconds passing gives `provider_unavailable`; `ok: true` takes each of `access_token`, `refresh_token`, `expires_in`, `scope` from the top level, else from `authed_user`, and no `access_token` in either is `provider_unavailable`.

The code: the relay is TypeScript on Node.js 22 (`nodejs22.x`, arm64, 256 MB, 10-second timeout, reserved concurrency 20), bundled by the CDK's `NodejsFunction` with esbuild, a devDependency so that synthesis and the stack's tests never need Docker. The stack passes `https://signin.<domain>/v1/callback` to the function as `RELAY_CALLBACK_URL`, which `handler.ts` puts in `deps.callbackUrl`. `relay.ts` holds the logic as `relay(request, deps)`, so tests give it a clock, `fetch`, the secrets, the used-ticket write and the log; `handler.ts` adapts API Gateway's event and builds the real `deps` with `@aws-sdk/client-secrets-manager` and `@aws-sdk/client-dynamodb`, the secrets cached for 5 minutes. Rejected: a Rust function, which needs cross-compiling and `cargo-lambda` for a hundred lines; and putting it in `apps/`, since ADR 0017 makes `infra` the package for what Farik hosts.

The stack, `RelayStack` (`FarikRelay`, `us-east-1`):
- Its inputs come from the environment at synthesis, never from a committed file: `FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `FARIK_ALERT_EMAIL`, `FARIK_MONTHLY_BUDGET_USD`, and `CDK_DEFAULT_ACCOUNT`. `infra/cdk.out/` and `infra/cdk.context.json` are gitignored.
- Two secrets, `farik/relay/clients` (created with a generated placeholder; the founder puts `{ "slack": { "client_id": …, "client_secret": … } }`, and later deploys never change it) and `farik/relay/ticket-key` (64 generated characters, no punctuation). Only the function's role may read them.
- The table `farik-relay-tickets`, on demand, partition key `id`, TTL `expires_at`; the role may only `dynamodb:PutItem` on it.
- A regional REST API, `disableExecuteApiEndpoint: true`, its custom domain `signin.<domain>` with an ACM certificate validated through the hosted zone, security policy TLS 1.2, an A alias record; stage throttling 50 requests a second, burst 100; access logging and execution logging off; tracing off.
- An AWS WAF web ACL on the stage: a rate-based rule blocking an IP after 100 requests in 5 minutes with a 429; CloudWatch metrics on; sampled requests off; no logging configuration.
- The function's log group kept 30 days.
- An AWS Budgets monthly cost budget at `FARIK_MONTHLY_BUDGET_USD`, mailing `FARIK_ALERT_EMAIL` at 80% actual and 100% forecast.
- GitHub's OpenID Connect provider and the role `farik-relay-deploy`, trusted only for `repo:abdshaat/Farik:environment:relay-production` with audience `sts.amazonaws.com`, allowed only `sts:AssumeRole` on the account's `cdk-hnb659fds-*` roles in `us-east-1`. Phase 11 step 01 reuses the provider.
- The workspace: `infra` joins `pnpm-workspace.yaml`, as `@farik/infra`, so the root `pnpm check` type-checks, lints and tests it. Each dependency is pinned exactly at the latest stable version on the day of Task 2, and the pull request lists each with its licence.

Deployment: `.github/workflows/relay.yml` runs on a push to `main` touching `infra/**` and on `workflow_dispatch`, in the GitHub environment `relay-production` (the founder its required reviewer), with `id-token: write`, assumes `vars.AWS_RELAY_DEPLOY_ROLE_ARN`, and runs `pnpm --filter @farik/infra exec cdk deploy FarikRelay --require-approval never` with the stack's inputs from repository variables. The first deploy, which creates that role, is the founder's, from the founder's own machine.

The local side:
- **`AppFlow` gains `Relay { relay_url: &'static str, service: &'static str }`.** A Relay entry runs step 03's loopback listener on a free port; Farik makes a 32-byte `relay_verifier` (`rand`, already a dependency) and calls `/v1/start`; the attempt's `state` is the ticket; at the callback, Farik compares `state` with it (step 03's rule) and calls `/v1/token` instead of rmcp's exchange. The grant: `issuer` the table's, `app: Some("slack")`, `token_endpoint` `<relay_url>/v1/refresh`, `revocation_endpoint` `https://slack.com/api/auth.revoke`.
- **`refreshed` takes the table**, and refreshes a grant whose app is a Relay entry through `/v1/refresh`.
- **Calls to the relay** go through step 03's https-or-loopback `reqwest` client, follow no redirect, and give up after 10 seconds (`RELAY_TIMEOUT`). No answer, a timeout, a 429, a 5xx, or a body that is not the API's is `RelayUnavailable`; `invalid_grant` is `Lapsed`; `invalid_ticket`, `ticket_expired` and `ticket_used` are `Mismatch`; any other `error` is `Failed`.
- **At sign-in**, `RelayUnavailable` is the RPC refusal `relay_unavailable`. **At refresh**, it is a failure that does not lapse the grant (step 03's rule).
- **The screens** show "Sign in with Slack" (03b's provider name) and, for `relay_unavailable`, "Farik's sign-in service isn't answering. Try again in a few minutes, or use a key instead." over step 01's key fields. The command line prints the same sentence.

## File map

```
docs/design/mockups/{ConnectorAdd,RelayError}.dc.html, canvas.json     Task 1
pnpm-workspace.yaml, .gitignore                                       modifies: infra; cdk.out, cdk.context.json (Task 2)
infra/package.json, tsconfig.json, cdk.json, bin/farik.ts             creates: the CDK app (Task 2)
infra/lib/relay-stack.ts, infra/test/relay-stack.test.ts              creates: the stack and its assertions (Task 2)
infra/relay/{ticket,services,relay,handler}.ts, *.test.ts             creates: the function (Task 3)
crates/runtime/src/relay.rs, registered_apps.rs, sign_in.rs, lib.rs   creates/modifies: the client, Relay flow, refresh (Task 4)
crates/runtime/tests/fixture_relay.rs                                 creates: a fake relay on loopback (Task 4)
crates/runtime/src/daemon/team.rs, docs/schemas/rpc.schema.json, crates/cli/src/connector.rs   modifies: relay_unavailable (Task 5)
apps/web/src/pages/ConnectorAdd.tsx, connectors.test.tsx, strings/en.ts   modifies (Task 6)
.github/workflows/relay.yml, registered_apps.rs, docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   Task 7
```

## Interfaces

Consumes: step 03's listener, `SignIn`, `OAuthGrant`, `refreshed`, `SignInError`; step 03b's `RegisteredApp`, `AppFlow`, `app_for`, `REGISTERED_APPS`.

Produces:

```ts
// infra/relay
export interface Deps { callbackUrl: string; now(): number; fetch: typeof fetch; secrets(): Promise<{ clients: Record<string, { client_id: string; client_secret: string }>; ticketKey: string }>;
  markUsed(id: string, expiresAt: number): Promise<boolean>; log(line: RelayLog): void }
export interface RelayRequest { method: string; path: string; query: Record<string, string>; body: string | null }
export interface RelayResponse { status: number; headers: Record<string, string>; body: string }
export interface RelayLog { time: string; route: string; service: string | null; outcome: string; provider_status: number | null; ms: number }
export function relay(req: RelayRequest, deps: Deps): Promise<RelayResponse>;
export function mintTicket(p: { service: string; port: number; challenge: string }, key: string, now: number): string;
export function readTicket(ticket: string, key: string, now: number): TicketPayload | "invalid_ticket" | "ticket_expired";
export class RelayStack extends Stack { constructor(scope: Construct, id: string, props: RelayStackProps) }
export interface RelayStackProps extends StackProps { domain: string; hostedZoneId: string; alertEmail: string; monthlyBudgetUsd: number }
```

```rust
// farik-runtime
pub enum AppFlow { /* 03b's */ Relay { relay_url: &'static str, service: &'static str } }
pub const RELAY_TIMEOUT: Duration = Duration::from_secs(10);
pub async fn refreshed(grant: &OAuthGrant, apps: &[RegisteredApp], now: DateTime<Utc>, valid_for: Duration,
    timeout: Duration) -> Result<Option<OAuthGrant>, SignInError>;
pub enum SignInError { /* 03b's */ RelayUnavailable }
```

Wire: `connector.sign_in` is refused also `relay_unavailable`.

## Tasks

### Task 1: The relay's screens, mocked up

On the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), desktop and phone width, the canvas's tokens, muted and light, copied to `docs/design/mockups/`:
- **`ConnectorAdd`, Slack offered**: `https://mcp.slack.com/mcp`, "Slack lets you sign in.", "Sign in with Slack", "Use a key instead".
- **`ConnectorAdd`, relay unavailable**: the muted error line "Farik's sign-in service isn't answering. Try again in a few minutes, or use a key instead.", "Try again", and step 01's key fields below.
- **`RelayError`**, the relay's own page: the Farik wordmark as text, "This sign-in link has expired or didn't come from Farik. Go back to Farik and start again." No link, no script, no image.

Gate: the founder approves the boards, or says to approve them automatically; the approval and its date go into this plan's header. Task 6 waits for it, and Task 3's page copies `RelayError`'s sentence once approved (until then, the sentence above).

- [ ] `docs(design): mock up the sign-in relay's screens`

### Task 2: The relay's infrastructure

Files: as the file map. Produces `RelayStack`, `RelayStackProps`. Tests (`infra/test/relay-stack.test.ts`, the CDK's `Template` assertions, with made-up inputs):

- `keeps_two_secrets_readable_only_by_the_function`: two `AWS::SecretsManager::Secret`; the only IAM policy granting `secretsmanager:GetSecretValue` is the function role's, on exactly those two.
- `uses_tickets_once_and_forgets_them`: the table has partition key `id` and TTL `expires_at`; the function's role allows `dynamodb:PutItem` on it and no other DynamoDB action.
- `answers_only_on_its_domain_over_tls_1_2`: the REST API has `DisableExecuteApiEndpoint: true`; the domain is `signin.<domain>` with `SecurityPolicy: TLS_1_2` and a DNS-validated certificate.
- `limits_callers`: the stage's throttling is 50 and 100; the web ACL has a rate-based rule of 100 per 300 seconds aggregated by IP, `SampledRequestsEnabled: false`, and is associated with the stage; the function's reserved concurrency is 20 and its timeout 10 seconds.
- `logs_nothing_it_should_not`: the stage has no `AccessLogSetting` and no `MethodSettings` with `LoggingLevel` other than `OFF` or `DataTraceEnabled: true`; no `AWS::WAFv2::LoggingConfiguration` exists; the function's log group retention is 30 days.
- `deploys_only_from_the_founder_s_environment`: the deploy role's trust condition `token.actions.githubusercontent.com:sub` equals `repo:abdshaat/Farik:environment:relay-production`, and its only action is `sts:AssumeRole` on `cdk-hnb659fds-*` roles.
- `warns_the_founder_on_cost`: one monthly `COST` budget at the given amount, with notifications at 80 actual and 100 forecast to the given email.

- [ ] `feat(infra): the sign-in relay's AWS stack`

### Task 3: The relay

Files: `infra/relay/*`. Produces `relay`, `mintTicket`, `readTicket`, `Deps`. Tests (Vitest; a fake `fetch` playing Slack, a `markUsed` over a `Set`):

- `mints_and_reads_a_ticket`: a minted ticket reads back its payload; one with a changed character is `invalid_ticket`; at `exp + 1` it is `ticket_expired`; one signed with another key is `invalid_ticket`.
- `start_builds_slack_s_address`: `authorize_url` is on `slack.com/oauth/v2_user/authorize` with the client id, `redirect_uri` `https://signin.example.test/v1/callback`, the four scopes and `state` the ticket; no `code_challenge`.
- `start_refuses_bad_input`: an unknown service is `unknown_service`; port 80, port 70000, a challenge of 42 characters, and a 9 KiB body are `bad_request`.
- `callback_bounces_only_to_localhost`: a good ticket gives 302 to `http://localhost:<port>/callback` with `code` and `state`; an `error=access_denied` keeps it; `error=<script>` becomes `server_error`; a bad ticket gives 400 with the page and its three headers, and no `Location`.
- `token_needs_the_verifier`: a wrong `relay_verifier` is `invalid_ticket` and Slack is not called; the right one calls Slack once with `client_secret`, and answers the normalised tokens from `authed_user`.
- `token_uses_a_ticket_once`: the second `/v1/token` with the same ticket is `ticket_used`, and Slack was called once.
- `refresh_adds_the_secret`: Slack got `grant_type=refresh_token`, the refresh token, the client id and secret; the answer is normalised.
- `maps_slack_s_answers`: `invalid_refresh_token` gives 400 `invalid_grant`; `ok: false` with `invalid_client` gives 502 `provider_unavailable`; HTTP 503, HTML, and 8 seconds without an answer each give 502 `provider_unavailable`.
- `logs_hold_no_secret`: across every test's requests, no logged line contains the code, the ticket, the verifier, either token, the client secret, the ticket key, or a query string, and each line has exactly the six `RelayLog` fields.
- `every_answer_carries_no_store_and_hsts`.

- [ ] `feat(infra): the sign-in relay adds Farik's secret and keeps nothing`

### Task 4: Signing in through the relay

Files: `relay.rs`, `registered_apps.rs`, `sign_in.rs`, `lib.rs`, `fixture_relay.rs`. The fixture is an axum server on loopback implementing the four routes over the fixture's own secret and an in-memory used set, with flags to answer 503, hang, return `invalid_grant`, or bounce with another `state`; the tests' table has a Relay entry pointing at it with `Exact("127.0.0.1")`.

- `signs_in_through_the_relay`: `/v1/start` got the listener's port and a 43-character challenge; following `authorize_url` to the fixture's callback reaches the listener; `/v1/token` got the ticket, the code and a verifier hashing to the challenge; the grant has `app: Some("relay-test")` and `token_endpoint` `<fixture>/v1/refresh`.
- `refuses_a_bounce_with_another_state`: 400 at the listener, and `/v1/token` is not called.
- `says_when_the_relay_is_down`: a closed port, a 503, and a hang past `RELAY_TIMEOUT` (paused clock) each give `RelayUnavailable` from `start_sign_in`.
- `refreshes_through_the_relay`: `refreshed` with the table posts the refresh token to `/v1/refresh` and keeps the new tokens.
- `a_refresh_the_relay_cannot_make_does_not_lapse`: with the relay answering 503 and the token still valid, `refreshed` gives `Failed`, and the entry's `lapsed` stays false; `invalid_grant` gives `Lapsed`.
- `never_follows_a_relay_redirect`: a relay answering 302 to `/v1/start` gives `RelayUnavailable`, and the redirect target saw no request.

- [ ] `feat(runtime): sign in through Farik's sign-in relay`

### Task 5: The daemon and the command line

Files: `daemon/team.rs`, `rpc.schema.json`, `cli/src/connector.rs`; the call sites of `refreshed` in `orchestrator/session.rs` and `daemon.rs` pass `REGISTERED_APPS`.

- `sign_in_says_the_relay_is_unavailable`: `connector.sign_in` against a down fixture is refused `relay_unavailable`, and the error carries no address of the relay's.
- `a_session_with_the_relay_down_leaves_the_server_out`: an expired relay grant, the fixture answering 503: the server is absent from `mcp.json`, and `team.get` does not say `sign_in_again`.
- `farik_connect_says_the_relay_is_unavailable`: prints the Decisions' sentence.

- [ ] `feat(runtime): say when the sign-in relay is down`

### Task 6: The screens

Files: `ConnectorAdd.tsx`, `connectors.test.tsx`, `strings/en.ts`. From Task 1's approved boards.

- `connector_add_falls_back_when_the_relay_is_down`: `relay_unavailable` shows the sentence, "Try again", and the key fields; "Try again" sends `connector.sign_in` again.

- [ ] `feat(web): say when the sign-in relay is down`

### Task 7: The relay, live

Gate: the founder's actions below are done, the first deploy has run, and the founder gives the domain and Slack's client id in conversation.

Files: `.github/workflows/relay.yml` (as the Decisions say); `registered_apps.rs` (the entry `slack`, `Slack`, `Exact("mcp.slack.com")`, `Relay { relay_url: "https://signin.<domain>", service: "slack" }`, issuer `https://mcp.slack.com`); `docs/SPEC.md` (6.7: the relay route; 8.6: what the relay sees, keeps and logs; 9: the relay is free; F9: `relay_unavailable`); `docs/plans/project-plan.md` (row 03c, corrected if execution changed it); `docs/design/role-kits.md`.

- `the_shipped_table_names_slack_through_the_relay`: the entry exists and its `relay_url` is `https`.

- [ ] `feat(runtime): sign in to Slack through the relay`

Founder's actions (no agent creates an account, holds AWS credentials, or sees a secret's value):
- [ ] **The AWS account**, owned and paid for by the founder: root locked away with MFA, IAM Identity Center with MFA for the founder's own access.
- [ ] **The domain**, registered by the founder in Route 53 Domains (its hosted zone is created with it).
- [ ] **CDK bootstrap**: `pnpm --filter @farik/infra exec cdk bootstrap aws://<account>/us-east-1`, signed in through Identity Center.
- [ ] **The first deploy**, from the founder's machine with the five inputs set: `pnpm --filter @farik/infra exec cdk deploy FarikRelay`. Confirm the budget's email subscription.
- [ ] **The Slack app**, at api.slack.com/apps, owned by the founder: name Farik; redirect URL `https://signin.<domain>/v1/callback`; user token scopes `channels:read`, `channels:history`, `chat:write`, `users:read`; token rotation on; PKCE left off; public distribution on; access to Slack's MCP server turned on where Slack's settings offer it; the Slack Marketplace review submitted if Slack requires it for workspaces other than the founder's.
- [ ] **Slack's secret**, put by the founder: `aws secretsmanager put-secret-value --secret-id farik/relay/clients --secret-string '{"slack":{"client_id":"…","client_secret":"…"}}'`.
- [ ] **GitHub**: the environment `relay-production` with the founder as required reviewer, and the repository variables `AWS_RELAY_DEPLOY_ROLE_ARN` (the stack's output), `FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `FARIK_ALERT_EMAIL`, `FARIK_MONTHLY_BUDGET_USD`.

## Verification

```
cargo xtask check
# expected: xtask check: ok   (it runs pnpm check, which now tests infra)
pnpm --filter @farik/infra exec cdk synth FarikRelay --quiet
# expected: exit 0, with made-up inputs exported
```

The founder's live check, recorded in the pull request: `curl -sI https://signin.<domain>/v1/start` answers 404 with HSTS, and the `execute-api` address does not resolve to the API; from the web app, as the Scrum Master, sign in to `https://mcp.slack.com/mcp`, list its tools, run one session that reads a channel; the function's log group holds only the six-field lines; then Remove. Run the `relay` workflow once from GitHub and approve it.
