# Phase 7, step 03c: The sign-in relay

Status: deferred by the founder, 2026-10-02: Slack is a later step ("Connecting slack is a later step keep it simple for now"; "Write the slack integeration plan in the last phase"). Reviewed and folded; lands as phase 15 step 02, in its Slack integration (project plan revisions 30 to 32), where it is reviewed for readiness again, moved and renumbered. Where it says phase 11 or phase 13 for Slack's listing, read phase 15 step 04; the website (phase 11 step 01) is live by then.
Branch: `phase/14-premium` when taken up (written on `phase/7-role-kits`)
Spec: `docs/SPEC.md` 8.6, 9
Depends on: none in code (the relay is a new package, `infra`); step 03d, which signs in through it, depends on this step
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 5 Blocking, all folded with the founder's decisions; no second round (ADR 0032)
Mockups approved by: pending (Task 1's gate)

## Goal

Slack's MCP server's authorization server takes only `client_secret_post`, and no program on the user's computer can keep a secret. When this step is done, Farik's sign-in relay runs at `https://signin.<domain>`: a small function on AWS, deployed by the founder and then by CI, that adds Farik's Slack client secret to a code exchange and to each refresh and hands the answer straight back, keeping no token. The founder can prove it from a terminal: a Slack code works only with the ticket it was issued for. Out of scope: Farik signing in through it (step 03d), the homepage on the same domain (step 03e), the website (phase 11 step 01), incoming hooks (ADR 0035's route 4, premium), and the Scrum Master's choice of Slack scopes beyond the first set below (step 06). Slack's app stays private to the founder's workspace until the Slack Marketplace lists it (ADR 0035's amendment; phase 11). Folding the readiness review split the reviewed plan in two at the relay's API: this step is the relay and its AWS stack, step 03d the local side.

## Decisions

ADR 0035 records the relay's shape and its security: the ticket, the limits, TLS, the secrets, what happens when it is down, and what it logs. This plan builds it and does not restate the reasons.

The relay's API (`snake_case` JSON; `Cache-Control: no-store` and `Strict-Transport-Security: max-age=31536000` on every answer the function makes; JSON answers with `Content-Type: application/json`; no CORS). `infra/relay`'s request and answer types use the wire's field names: it is a wire edge with no camelCase model, so it is its own mapping layer (hard rule 6).
- **Routing** is on method and path together: `POST /v1/start`, `GET /v1/callback/{id}`, `POST /v1/token`, `POST /v1/refresh`. Any other pair is 404 `not_found`.
- **Bodies.** A body whose `Buffer.byteLength` after base64 decoding (when API Gateway set `isBase64Encoded`) is over 8192 is `bad_request`; so is one that is not a JSON object, or a field missing or failing its pattern.
- `POST /v1/start` `{ service, port, relay_challenge }` → 200 `{ ticket, authorize_url }`. `port` is an integer from 1024 to 65535; `relay_challenge` matches `^[A-Za-z0-9_-]{43}$`; a service missing from the services table or from the `clients` secret is `unknown_service`. The `authorize_url` is the service's authorization endpoint with, built by `URLSearchParams`: `client_id` from the `clients` secret, `scope` the service's fixed scopes, `redirect_uri` `<callbackUrl>/<ticket id>` (Slack accepts a subdirectory of the registered Redirect URL, and refuses an exchange whose `redirect_uri` differs, which binds the code to this ticket), and `state` the ticket.
- `GET /v1/callback/{id}?code&state`, or `?error&state` → 302 to `http://localhost:<the ticket's port>/callback` with, built by `URLSearchParams`, `code` and `state` = the ticket, or `error` and `state`. When both `code` and `error` are present, `error` wins. `error` matching `^[a-z_]{1,64}$` is kept, anything else becomes `server_error`. The 400 page answers instead when: neither is present; `code` does not match `^[\x21-\x7E]{1,512}$`; the ticket is bad or expired; or the path `id` is not the ticket's `id`. The page is the relay's (Task 1), with `Content-Type: text/html; charset=utf-8`, `Content-Security-Policy: default-src 'none'`, `Referrer-Policy: no-referrer`, and no `Location`. The callback does not use up the ticket.
- `POST /v1/token` `{ ticket, code, relay_verifier }`, in this order: the fields' patterns (`relay_verifier` `^[A-Za-z0-9_-]{43}$`, `code` `^[\x21-\x7E]{1,512}$`); `readTicket`; `BASE64URL(SHA-256(relay_verifier's ASCII)) == challenge`, compared with `timingSafeEqual`, a mismatch `invalid_ticket` **without** marking the ticket used (so whoever sees a `state` cannot cancel a sign-in); then `markUsed` (`false` is `ticket_used`); then the exchange, posting `redirect_uri` `<callbackUrl>/<ticket id>` built from the presented ticket.
- `POST /v1/refresh` `{ service, refresh_token }` → refreshes. `refresh_token` matches `^[\x21-\x7E]{1,2048}$`.
- **Answers** to `/v1/token` and `/v1/refresh`: 200 `{ access_token, token_type: "bearer", expires_in?, refresh_token?, scope? }`, `scope` space-separated (Slack's split on commas and spaces); else `{ error }`: 400 `bad_request`, `unknown_service`, `invalid_ticket`, `ticket_expired`, `ticket_used`, `invalid_grant`; 502 `provider_unavailable`; 500 `server_error`.
- **When a dependency throws.** Any exception from `deps` (the secrets, `markUsed`, `fetch` other than the timeout) or from parsing a secret answers 500 `{ error: "server_error" }` and logs one line with outcome `server_error`; no error's message or stack is ever logged. `markUsed` returns `false` on `ConditionalCheckFailedException` and throws on anything else. `handler.ts` wraps `relay()` in `try/catch`, never rethrows, and writes only through `deps.log` (`console.log(JSON.stringify(line))`); no other `console` call exists in `infra/relay`.

The ticket: `BASE64URL(payload JSON) "." BASE64URL(HMAC-SHA256(key, that first part))`, the payload `{ v: 1, id, service, port, challenge, exp }`: `id` is 16 random bytes in base64url (`^[A-Za-z0-9_-]{22}$`), `exp` is unix **seconds**, now + 600. `now()` is unix seconds too, and a ticket is valid while `now <= exp`. `readTicket` compares the MAC with `timingSafeEqual` before it parses the payload; a bad MAC, unparseable JSON, `v` other than 1, or a field failing its pattern is `invalid_ticket`. The key is the `farik/relay/ticket-key` secret. Single use is a DynamoDB conditional put of `{ id, expires_at }` with `attribute_not_exists(id)`; the table's TTL attribute is `expires_at`.

The services table, `infra/relay/services.ts`, has one entry: `slack`, authorization `https://slack.com/oauth/v2_user/authorize`, token `https://slack.com/api/oauth.v2.user.access` (both from `mcp.slack.com`'s metadata, read 2026-10-02), scopes `channels:read,channels:history,chat:write,users:read`, no PKCE (Slack's PKCE mode is one-way and makes the app public). The exchange posts `client_id`, `client_secret`, `code`, `redirect_uri`; the refresh posts `client_id`, `client_secret`, `grant_type=refresh_token`, `refresh_token`. Each Slack call gives up after 6 seconds (`AbortSignal.timeout(6000)`), so the relay always answers inside the local client's 10. Slack's answer is read as:
- `ok: false` with `error` in `invalid_code`, `code_already_used`, `bad_redirect_uri`, `invalid_refresh_token`, `token_expired`, `token_revoked`, `invalid_grant` → `invalid_grant`;
- any other `ok: false`, a non-JSON body, an HTTP status of 500 or more, or the 6 seconds passing → `provider_unavailable`;
- `ok: true` takes each of `access_token`, `refresh_token`, `expires_in`, `scope` from the top level, else from `authed_user`; no `access_token` in either is `provider_unavailable`.

The code: TypeScript on Node.js 24 (`Runtime.NODEJS_24_X`, arm64, 256 MB, an 8-second timeout, reserved concurrency 20), bundled by the CDK's `NodejsFunction` with esbuild (a devDependency, so synthesis and the stack's tests never need Docker) and `bundling: { externalModules: [] }`, so the AWS SDK clients are bundled, as AWS recommends. The stack passes `https://signin.<domain>/v1/callback` as `RELAY_CALLBACK_URL`, which `handler.ts` puts in `deps.callbackUrl`. `relay.ts` holds the logic as `relay(request, deps)`, so tests give it a clock, `fetch`, the secrets, the used-ticket write and the log; `handler.ts` adapts API Gateway's proxy event (`httpMethod`, `path`, `queryStringParameters`, `body`, `isBase64Encoded`) and builds the real `deps` with `@aws-sdk/client-secrets-manager` and `@aws-sdk/client-dynamodb`, the secrets cached for 5 minutes. Rejected: a Rust function, which needs cross-compiling and `cargo-lambda` for a hundred lines; and putting it in `apps/`, since ADR 0017 makes `infra` the package for what Farik hosts.

The package: `infra` joins `pnpm-workspace.yaml` as `@farik/infra`, so the root `pnpm check` type-checks, lints and tests it. `infra/package.json` scripts: `typecheck` = `tsc --noEmit -p .`, `test` = `vitest run`, as the other packages; devDependencies `aws-cdk` (the CLI), `aws-cdk-lib`, `constructs`, `esbuild`; dependencies `@aws-sdk/client-secrets-manager`, `@aws-sdk/client-dynamodb`. `tsconfig.json` sets `erasableSyntaxOnly: true`, `allowImportingTsExtensions: true` and `noEmit: true`, and imports carry `.ts`, because the CDK app runs under Node 24's type stripping (`cdk.json` `app`: `node bin/farik.ts`); TypeScript 7 has no JS API, so `ts-node` is not used. Each dependency is pinned exactly at the latest stable version on the day of Task 2, and the pull request lists each with its licence. `biome.json` includes `infra/**` and excludes `!**/cdk.out`.

The stack, `RelayStack` (`FarikRelay`, `us-east-1`):
- **Inputs** come from the environment at synthesis, never from a committed file: `FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `FARIK_ALERT_EMAIL`, `FARIK_MONTHLY_BUDGET_USD`, and `CDK_DEFAULT_ACCOUNT`. `bin/farik.ts` reads them; a missing one stops synthesis with its name. `infra/cdk.out/` and `infra/cdk.context.json` are gitignored. The hosted zone is `HostedZone.fromHostedZoneAttributes`, never a lookup, so synthesis and tests need no AWS credentials.
- **Two secrets**, `farik/relay/clients` (created with a generated placeholder; the founder puts `{ "slack": { "client_id": …, "client_secret": … } }`, and later deploys never change it) and `farik/relay/ticket-key` (64 generated characters, no punctuation). The function's role is granted `secretsmanager:GetSecretValue` on exactly the two. Each secret has a resource policy (`secret.addToResourcePolicy`) denying `secretsmanager:GetSecretValue` to every principal whose `aws:PrincipalArn` is not the function's role ARN, with `BlockPublicPolicy: true`; `PutSecretValue` is unaffected, so the founder can still set the value. Both are `RemovalPolicy.RETAIN`.
- **The table** `farik-relay-tickets`, on demand, partition key `id`, TTL `expires_at`, `RemovalPolicy.RETAIN`; the role may only `dynamodb:PutItem` on it.
- **The API**: a regional `LambdaRestApi` with `proxy: true` (`ANY /` and `ANY /{proxy+}`), `disableExecuteApiEndpoint: true` (a client reaching the `execute-api` address gets 403), `cloudWatchRole: false` (no `AWS::ApiGateway::Account`), the stage named `live` with throttling 50 requests a second and burst 100, access logging and execution logging off, tracing off. Its custom domain `signin.<domain>`, an ACM certificate validated through the hosted zone, security policy `SecurityPolicy_TLS13_1_2_PFS_PQ_2025_09` with `EndpointAccessMode: STRICT` (set on the L1 `AWS::ApiGateway::DomainName` with `addPropertyOverride` if the L2 lacks them), TLS 1.2 still the minimum; the empty base path mapped to `live`, so request paths are `/v1/...`; an A alias record. The REST API's own TLS policy is moot with its endpoint off.
- **An AWS WAF web ACL** on the stage: one rate-based rule blocking an IP after 100 requests in 5 minutes, its action Block with the custom response code 429; CloudWatch metrics on; `SampledRequestsEnabled: false` on the web ACL and on the rule; no logging configuration. Rejected: a WAF size rule for bodies over 8 KiB, since the function refuses them and the throttling bounds their cost.
- **The function's log group** is an explicit `logs.LogGroup` (30 days, `RemovalPolicy.RETAIN`) passed as `logGroup`; not `logRetention`, which deploys a custom-resource function.
- **An AWS Budgets** monthly cost budget at `FARIK_MONTHLY_BUDGET_USD`, mailing `FARIK_ALERT_EMAIL` at 80% actual and 100% forecast.
- **GitHub's OpenID Connect provider** as `iam.OidcProviderNative` (a plain `AWS::IAM::OIDCProvider`, no custom resource), and the role `farik-infra-deploy`, trusted by `sts:AssumeRoleWithWebIdentity` only when `token.actions.githubusercontent.com:sub` equals `repo:abdshaat/Farik:environment:aws-production` and `aud` equals `sts.amazonaws.com`, allowed only `sts:AssumeRole` on `arn:aws:iam::<account>:role/cdk-hnb659fds-*-<account>-us-east-1`. The stack outputs `DeployRoleArn`. Step 03e and phase 11 step 01 reuse the provider and the role.

Deployment: `.github/workflows/infra.yml`, named for the package because step 03e and phase 11 step 01 add stacks that it deploys too, runs on a push to `main` touching `infra/**` and on `workflow_dispatch`, in the GitHub environment `aws-production` (the founder its required reviewer, `main` its only deployment branch), with `permissions: { id-token: write, contents: read }` and `concurrency: aws-production`. Its steps: checkout; pnpm and Node from `.node-version`; `pnpm install --frozen-lockfile`; `aws-actions/configure-aws-credentials@v5` with `role-to-assume: ${{ vars.AWS_DEPLOY_ROLE_ARN }}` and `aws-region: us-east-1`; `pnpm --filter @farik/infra exec cdk deploy --all --require-approval never` with the stack's inputs from repository variables. The first deploy, which creates that role, is the founder's, from the founder's own machine.

## File map

```
docs/design/mockups/RelayError.dc.html, canvas.json                         Task 1
pnpm-workspace.yaml, biome.json                                             modifies: infra; infra/** and !**/cdk.out (Task 2)
infra/package.json, infra/tsconfig.json                                     creates: the package (Task 2)
infra/relay/{ticket,services,relay,handler}.ts, infra/relay/*.test.ts       creates: the function and its tests (Task 2)
infra/cdk.json, infra/bin/farik.ts, infra/lib/relay-stack.ts                creates: the CDK app and the stack (Task 3)
infra/test/relay-stack.test.ts, .gitignore                                  creates: the stack's assertions; modifies: cdk.out, cdk.context.json (Task 3)
.github/workflows/infra.yml, docs/SPEC.md, docs/plans/project-plan.md        Task 4
```

## Interfaces

Consumes: nothing from earlier steps. Step 03d consumes the API above.

Produces:

```ts
// infra/relay
export interface Deps { callbackUrl: string; now(): number; fetch: typeof fetch;
  secrets(): Promise<{ clients: Record<string, { client_id: string; client_secret: string }>; ticketKey: string }>;
  markUsed(id: string, expiresAt: number): Promise<boolean>; log(line: RelayLog): void }
export interface RelayRequest { method: string; path: string; query: Record<string, string>; body: string | null }
export interface RelayResponse { status: number; headers: Record<string, string>; body: string }
export interface RelayLog { time: string; route: string; service: string | null; outcome: string; provider_status: number | null; ms: number }
export interface TicketPayload { v: 1; id: string; service: string; port: number; challenge: string; exp: number }
export function relay(req: RelayRequest, deps: Deps): Promise<RelayResponse>;
export function mintTicket(p: { service: string; port: number; challenge: string }, key: string, now: number): string;
export function readTicket(ticket: string, key: string, now: number): TicketPayload | "invalid_ticket" | "ticket_expired";
export function handler(event: APIGatewayProxyEvent): Promise<APIGatewayProxyResult>;  // types declared locally, no @types/aws-lambda
// infra/lib
export class RelayStack extends Stack { constructor(scope: Construct, id: string, props: RelayStackProps) }
export interface RelayStackProps extends StackProps { domain: string; hostedZoneId: string; alertEmail: string; monthlyBudgetUsd: number }
```

## Tasks

### Task 1: The relay's page, mocked up

On the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), desktop and phone width, the canvas's tokens, muted and light, copied to `docs/design/mockups/`:
- **`RelayError`**, the relay's own page: the Farik wordmark as text, "This sign-in link has expired or didn't come from Farik. Go back to Farik and start again." No link, no script, no image.

Gate: the founder approves the board, or says to approve it automatically; the approval and its date go into this plan's header. Task 2's page copies `RelayError`'s sentence once approved (until then, the sentence above).

- [ ] `docs(design): mock up the sign-in relay's page`

### Task 2: The relay

Files: as the file map. Produces `relay`, `mintTicket`, `readTicket`, `handler`, `Deps`. Tests (Vitest; a fake `fetch` playing Slack, a `markUsed` over a `Set`, `callbackUrl` `https://signin.example.test/v1/callback`, a fixed clock):

- `mints_and_reads_a_ticket`: a minted ticket reads back its payload; it is valid at `exp` and `ticket_expired` at `exp + 1`; one with a changed character, one signed with another key, and one whose payload has `v: 2` are `invalid_ticket`.
- `start_builds_slack_s_address`: `authorize_url` is on `slack.com/oauth/v2_user/authorize` with the client id, `redirect_uri` `https://signin.example.test/v1/callback/<the ticket's id>`, the four scopes and `state` the ticket; no `code_challenge`.
- `start_refuses_bad_input`: an unknown service, and a service absent from the `clients` secret, are `unknown_service`; ports 1023 and 65536, a challenge of 42 characters, a non-JSON body and a 9 KiB body are `bad_request`; ports 1024 and 65535 are accepted.
- `callback_bounces_only_to_localhost`: a good ticket on its own path gives 302 to `http://localhost:<port>/callback` with `code` and `state`; `error=access_denied` keeps it; `error=<script>` becomes `server_error`; `code` and `error` together keep `error` only; neither gives the 400 page; a `code` holding `&x=` is percent-encoded in the `Location`, and one holding `%0d%0a` decoded to CR LF is refused with the 400 page, so neither adds a parameter or a header; a bad ticket gives 400 with the page and its three headers, and no `Location`.
- `callback_refuses_another_ticket_s_path`: a good ticket on `/v1/callback/<another id>` gives the 400 page and no `Location`.
- `token_needs_the_verifier`: a wrong `relay_verifier` is `invalid_ticket` and Slack is not called; afterwards the same ticket with the right verifier still exchanges (a mutation moving `markUsed` before the challenge check fails here); Slack was called once with `client_secret`, and the answer is the tokens normalised from `authed_user`, its `scope` `"a,b"` answered as `"a b"`.
- `token_exchanges_with_the_ticket_s_redirect`: the fake Slack received `redirect_uri` ending in `/<ticket id>`; when it answers `{ ok: false, error: "bad_redirect_uri" }`, the relay answers 400 `invalid_grant`.
- `token_uses_a_ticket_once`: the second `/v1/token` with the same ticket is `ticket_used`, and Slack was called once.
- `refresh_adds_the_secret`: Slack got `grant_type=refresh_token`, the refresh token, the client id and secret; the answer is normalised.
- `maps_slack_s_answers`: `invalid_refresh_token` gives 400 `invalid_grant`; `ok: false` with `invalid_client` gives 502 `provider_unavailable`; HTTP 503, HTML, and 6 seconds without an answer each give 502 `provider_unavailable`.
- `a_failing_dependency_answers_500_and_logs_only_its_line`: `secrets()` rejecting with an `Error` whose message holds `xoxp-SECRET` answers 500 `server_error`, and the one logged line has the six fields, outcome `server_error`, and does not contain `xoxp-SECRET`; the same holds for `markUsed` throwing, and for a `clients` secret that is not JSON.
- `routes_on_method_and_path`: `GET /v1/start`, `POST /v1/callback/x` and `GET /nothing` are 404 `not_found`.
- `logs_hold_no_secret`: across every test's requests, no logged line contains the code, the ticket, the verifier, either token, the client secret, the ticket key, or a query string, and each line has exactly the six `RelayLog` fields.
- `every_answer_carries_no_store_and_hsts`: start, the callback's 302, the callback's 400 page, token, refresh, the 404 and the 500.
- `handler_adapts_api_gateway_s_event`: a proxy event with `isBase64Encoded: true` and `queryStringParameters` maps to `RelayRequest` with the decoded body, and a `RelayResponse` maps to `{ statusCode, headers, body }`.
- `handler_never_rethrows`: `handler()` with a throwing dependency resolves to a 500 proxy result.

- [ ] `feat(infra): the sign-in relay adds Farik's secret and keeps nothing`

### Task 3: The relay's infrastructure

Files: as the file map. Produces `RelayStack`, `RelayStackProps`. Tests (`infra/test/relay-stack.test.ts`, the CDK's `Template` assertions, with the made-up inputs of the Verification):

- `keeps_two_secrets_readable_only_by_the_function`: two `AWS::SecretsManager::Secret`; the only IAM policy granting `secretsmanager:GetSecretValue` is the function role's, on exactly those two; each secret has an `AWS::SecretsManager::ResourcePolicy` denying `secretsmanager:GetSecretValue` where `aws:PrincipalArn` is not the function's role, with `BlockPublicPolicy: true`; both secrets and the table are retained.
- `uses_tickets_once_and_forgets_them`: the table has partition key `id` and TTL `expires_at`; the function's role allows `dynamodb:PutItem` on it and no other DynamoDB action.
- `answers_only_on_its_domain_over_tls_1_2`: the REST API has `DisableExecuteApiEndpoint: true`; the domain is `signin.<domain>` with `SecurityPolicy: SecurityPolicy_TLS13_1_2_PFS_PQ_2025_09`, `EndpointAccessMode: STRICT` and a DNS-validated certificate; the base path mapping points at the stage `live`.
- `limits_callers`: the stage's throttling is 50 and 100; the web ACL has one rate-based rule of 100 per 300 seconds aggregated by IP, action Block with response code 429, `SampledRequestsEnabled: false` on the web ACL and the rule, and is associated with the stage; the function runs `nodejs24.x` on arm64 with reserved concurrency 20 and an 8-second timeout.
- `logs_nothing_it_should_not`: the stage has no `AccessLogSetting` and no `MethodSettings` with `LoggingLevel` other than `OFF` or `DataTraceEnabled: true`; no `AWS::WAFv2::LoggingConfiguration` and no `AWS::ApiGateway::Account` exist; the function's `LoggingConfig.LogGroup` is the group with 30-day retention.
- `deploys_only_from_the_founder_s_environment`: one `AWS::IAM::OIDCProvider`; the deploy role's trust action is `sts:AssumeRoleWithWebIdentity`, with `sub` StringEquals `repo:abdshaat/Farik:environment:aws-production` and `aud` StringEquals `sts.amazonaws.com`; its only permission is `sts:AssumeRole` on `arn:aws:iam::123456789012:role/cdk-hnb659fds-*-123456789012-us-east-1`; the output `DeployRoleArn` exists.
- `warns_the_founder_on_cost`: one monthly `COST` budget at the given amount, with notifications at 80 actual and 100 forecast to the given email.

- [ ] `feat(infra): the sign-in relay's AWS stack`

### Task 4: The relay, live

Gate: the founder's actions below are done, and the first deploy has run.

Files: `.github/workflows/infra.yml` (as the Decisions say); `docs/SPEC.md` (8.6: what the relay sees, keeps and logs, the code bound to its ticket, and that a consent page naming Farik can still be used to lure a user into pasting back a failed `localhost` address; 9: the relay is free); `docs/plans/project-plan.md` (row 03c, corrected if execution changed it).

- [ ] `ci(infra): deploy the sign-in relay from main`

Founder's actions (no agent creates an account, holds AWS or Slack credentials, or sees a secret's value):
- [ ] **The AWS account**, owned and paid for by the founder: root locked away with MFA, IAM Identity Center with MFA for the founder's own access.
- [ ] **The domain**, registered by the founder in Route 53 Domains (its hosted zone is created with it).
- [ ] **Lambda concurrency**: `aws lambda get-account-settings --query AccountLimit.ConcurrentExecutions`; if it is under 1000, request "Concurrent executions" = 1000 in Service Quotas (Lambda, us-east-1) and wait for it before the first deploy (a new account's 10 refuses the function's reserved 20).
- [ ] **CDK bootstrap**: `pnpm --filter @farik/infra exec cdk bootstrap aws://<account>/us-east-1`, signed in through Identity Center.
- [ ] **The first deploy**, from the founder's machine with the five inputs set: `pnpm --filter @farik/infra exec cdk deploy FarikRelay`.
- [ ] **The Slack app**, at api.slack.com/apps, owned by the founder, in the founder's workspace: name Farik; redirect URL `https://signin.<domain>/v1/callback` (the prefix of every ticket's address); user token scopes `channels:read`, `channels:history`, `chat:write`, `users:read`; token rotation on; PKCE left off; **public distribution off** until the Slack Marketplace submission (phase 11), since Slack's MCP server refuses unlisted distributed apps; access to Slack's MCP server turned on where Slack's settings offer it. Give the client id (public) in conversation.
- [ ] **Slack's secret**, put by the founder without it entering shell history or the process list: write `{"slack":{"client_id":"…","client_secret":"…"}}` to `slack-client.json` in an editor, run `aws secretsmanager put-secret-value --secret-id farik/relay/clients --secret-string file://slack-client.json`, then `shred -u slack-client.json`; or put it in the console.
- [ ] **GitHub**: the environment `aws-production` with the founder as required reviewer and "Deployment branches and tags: Selected branches, `main` only", and the repository variables `AWS_DEPLOY_ROLE_ARN` (the stack's output `DeployRoleArn`), `FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `FARIK_ALERT_EMAIL`, `FARIK_MONTHLY_BUDGET_USD`.

## Verification

```
cargo xtask check
# expected: xtask check: ok   (it runs pnpm check, which now type-checks, lints and tests infra)
FARIK_DOMAIN=example.test FARIK_HOSTED_ZONE_ID=Z000000000000 FARIK_ALERT_EMAIL=a@example.test \
  FARIK_MONTHLY_BUDGET_USD=20 CDK_DEFAULT_ACCOUNT=123456789012 \
  pnpm --filter @farik/infra exec cdk synth FarikRelay --quiet
# expected: exit 0
```

The founder's live check, recorded in the pull request, with `R=https://signin.<domain>`:
- `curl -sI $R/v1/start` answers 404 with `Strict-Transport-Security`.
- `curl -s -o /dev/null -w '%{http_code}' https://<api id>.execute-api.us-east-1.amazonaws.com/live/v1/start` prints 403.
- **A code works only with its own ticket.** Make two verifier pairs: `v=$(openssl rand 32 | openssl base64 -A | tr '+/' '-_' | tr -d '=')`, `c=$(printf %s "$v" | openssl dgst -sha256 -binary | openssl base64 -A | tr '+/' '-_' | tr -d '=')`, as `v1 c1` and `v2 c2`. `curl -s -X POST $R/v1/start -H 'content-type: application/json' -d '{"service":"slack","port":49152,"relay_challenge":"'$c1'"}'` gives `t1` and an address; the same with `c2` gives `t2`. Open the first address, approve in the founder's workspace, and copy `code` from the `http://localhost:49152/callback?...` address the browser lands on (nothing listens there). `curl -s -X POST $R/v1/token -H 'content-type: application/json' -d '{"ticket":"'$t2'","code":"'$code'","relay_verifier":"'$v2'"}'` answers 400 `{"error":"invalid_grant"}`.
- The function's log group holds only six-field lines, none with a code, ticket or token.
- Run the `infra` workflow once from GitHub and approve it; it succeeds.
