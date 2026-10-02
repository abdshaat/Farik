# 0035. Connect services without pasted keys, through a stateless sign-in relay

Date: 2026-10-02
Status: accepted (the founder, in conversation, 2026-10-02; the relay's shape ruled by the controller the same day)

Resolves step 03's O2 (`docs/plans/phase-7-role-kits/step-03-signing-in-to-a-service.md`). ADR 0033, written in step 03's Task 2, keeps its registration order; this ADR adds two routes before its refusal.

## Context

Phase 7 step 03 signs an agent in to a remote MCP server's service with OAuth, as a public client: a `client_id` the user gives, else dynamic client registration (DCR), else `sign_in_not_supported`. Its research found that GitHub, Slack and Google offer neither DCR nor a client metadata document. Farik is for non-technical users (ADR 0016), and the kits need exactly these three services (`docs/design/role-kits.md`): GitHub for the Product Manager's issue import and the Architect's code search, Google for the Product Manager's analytics and documents, Slack for the Scrum Master's chat bridge. Pasting a fine-grained token is a developer's chore.

Read on 2026-10-02:
- **GitHub.** `api.githubcopilot.com/mcp/` names `https://github.com/login/oauth`, whose metadata lists `device_authorization_endpoint` and S256. A GitHub App with device flow enabled refreshes its user tokens without a client secret ("Required unless the user access token was generated using the device flow"). The web flow needs the secret even with PKCE. Revoking a grant needs the secret.
- **Google.** For desktop clients the `client_secret` is optional in the exchange and the refresh, PKCE is supported, and any loopback port is accepted. Its MCP servers' metadata names `https://accounts.google.com/`, with a slash the issuer does not have. `drive.readonly` is a restricted scope (a yearly third-party security assessment); `analytics.readonly` is sensitive (verification). An app in testing gives refresh tokens that end after 7 days, to at most 100 named test users.
- **Slack.** `mcp.slack.com`'s authorization server lists `token_endpoint_auth_methods_supported: ["client_secret_post"]` alone. Slack's PKCE mode makes the app public, one way, with user scopes only and refresh tokens that end after 30 days, and its MCP server's metadata does not offer it. Without PKCE, Slack accepts only `https` redirect addresses.

So three kinds of service:
1. Those that register a client automatically: step 03 already signs in to them.
2. Those that take a pre-registered public client: Farik can register its own app once and ship its id.
3. Those that need a client secret on a server: no program on the user's computer can hold one, because the repository and the binary are public.

The options for the third kind were:
- **Pasted keys only.** No cloud service, and a non-technical user stops at "create a Slack app".
- **The broker in the premium tier only.** Slack would be a paid feature, though spec 9 makes kits free forever.
- **A broker that keeps every customer's tokens** and calls the service on the user's behalf, or hands tokens out on request. Rejected: Farik would have custody of every user's Slack, a single breach would expose all of them, and holding them brings the compliance burden (data protection duties, breach notification, audits) of a data processor, for a free feature.
- **A stateless relay that adds the secret and passes the answer straight back.** The chosen shape.

## Decision

**Four routes, tried in this order, with pasted keys always available.**

1. **The service registers Farik itself** (step 03): a pre-registered `client_id` the user gives, else DCR. A client metadata document joins this route with the launch site (step 03's O1).
2. **Farik's own registered public apps** (step 03b), for a service that takes a public client but does not register one. A table built into the binary, `REGISTERED_APPS`, names each app by the MCP server's host, never by what the server's metadata claims, so a token from Farik's app is sent only to that service's own hosts:
   - **GitHub**, a GitHub App owned by the founder, with device flow, expiring user tokens refreshed without a secret, read-only repository permissions, for `api.githubcopilot.com`. A grant cannot be revoked without the secret, so "Remove" says where to remove Farik in GitHub's settings.
   - **Google**, a desktop OAuth client in a Google Cloud project owned by the founder, with PKCE, no secret sent, and the narrowest read scopes the kit names, for hosts ending in `.googleapis.com`.
3. **The sign-in relay** (step 03c), in the free version, for a service whose app needs a server-side secret: Slack (`mcp.slack.com`), and any later one the same way.
4. **Incoming hooks**: a service pushing events to Farik through a public address. Premium, later, phase 14. Not planned now.

Routes 2 and 3 are used only when the server's host is in the table and the team file gives no `client_id` of its own. Pasted keys (step 01) stay the fallback everywhere: "Use a key instead" is offered on every sign-in, and is what the user is told when the relay is down.

**The relay is stateless and adds only the secret.** It is a function behind an API at `signin.<domain>`, on AWS (ADR 0017), in `infra/`:
- It adds Farik's client secret to the authorization-code exchange and to each refresh, and returns the service's answer straight to the user's computer, in one standard shape.
- It stores no customer token, no refresh token, no authorization code and no log of a request's or answer's body. Tokens stay in the agent's own key store, as in steps 01 and 03.
- Its only state is a list of used ticket ids with their expiry, deleted by the database's TTL, which holds nothing about the user.

**How a local Farik proves the sign-in is its own.** The relay cannot tell real Farik from someone running Farik's code, since both are public; what it proves is that the exchange comes from the program that started this sign-in, as PKCE does for a public client.
- At start, Farik makes a random 32-byte `relay_verifier` and sends `relay_challenge = BASE64URL(SHA-256(relay_verifier))` with the service and its loopback port. The relay answers a **ticket** and the service's authorization address.
- The ticket is the relay's signed statement `{ v, id, service, port, challenge, exp }`, HMAC-SHA256 under a key in AWS Secrets Manager, valid 10 minutes. It is the OAuth `state`.
- The service redirects to the relay's `https` callback (Slack takes nothing else). The relay checks the ticket's signature and expiry, and redirects the browser to `http://localhost:<the ticket's port>/callback` with the code and the ticket as `state`. It never redirects anywhere else, so it is no open redirect.
- Farik's listener accepts only its own ticket as `state` (step 03's rule), then sends `{ ticket, code, relay_verifier }`. The relay checks the signature, the expiry and the challenge, marks the ticket used with a conditional write (a second use is refused `ticket_used`), and only then adds the secret. A program that grabs the code on the loopback port has no verifier.
- A refresh carries no ticket: it happens with no person present. Anyone holding a refresh token can refresh it through the relay. That is the cost of a free relay and is stated below.
- Revocation needs no secret at Slack (`auth.revoke` takes the token), so it goes straight to the service.

**What the relay sees.** It sees each access and refresh token in transit, in memory, once per exchange and once per refresh. It cannot avoid this: the service's token endpoint answers whoever presents the secret, and only the relay holds it; Slack offers no other client authentication. It means that a relay whose code or AWS account were taken over could copy the tokens of users who sign in or refresh while it is taken. What it does not see: any token issued before, or any call an agent makes with a token. The defences are the code being open, deployment only from `main` by a role the founder's CI assumes through OpenID Connect, CloudTrail, short-lived Slack access tokens (token rotation on, 12 hours), and the user's ability to revoke Farik in Slack.

**Abuse and limits.** An AWS WAF rule blocks an address after 100 requests in 5 minutes; the API stage throttles at 50 requests a second with bursts of 100; the function has a reserved concurrency of 20 and a 10-second limit; a body over 8 KiB is refused; every field has a fixed pattern; the relay offers only the services and scopes in its own table, so a caller cannot widen a scope. An AWS Budgets alarm mails the founder at the monthly amount the founder sets.

**TLS.** A regional API Gateway REST API on the custom domain only (its `execute-api` address turned off), with an AWS Certificate Manager certificate validated by DNS, minimum TLS 1.2, and HSTS on every answer. Farik's client sends to the relay only over `https`, with rustls and its built-in roots, following no redirect.

**The relay's secrets.** Two AWS Secrets Manager secrets: the client credentials by service (`farik/relay/clients`), whose value the founder sets in the AWS console or CLI and no agent or file ever holds, and the ticket key (`farik/relay/ticket-key`), generated by Secrets Manager at creation and read by no person. Only the function's role may read them. The founder rotates either by putting a new value; a new ticket key ends the sign-ins in flight, at most 10 minutes of them.

**When the relay is down.** Signing in fails with `relay_unavailable`, after 10 seconds at most: "Farik's sign-in service isn't answering. Try again in a few minutes, or use a key instead", over step 01's key fields. A refresh that cannot reach the relay uses the access token while it is valid, and with it expired leaves the server out of that session only, as step 03 does for any refresh that fails; the grant does not lapse.

**What the relay logs.** One line per request: the time, the route, the service, the outcome code, the service's HTTP status and the duration. No address, no ticket, no code, no token, no query string, no header, no body. API Gateway's access and execution logs are off, and the WAF keeps metrics with sampled requests off, since a sampled callback would hold its code. Lines are kept 30 days.

**Free.** Routes 1 to 3 are in the free version, because kits are free forever (spec 9). Route 4 is premium. A relay is a sign-in aid, not hosted execution: F13's premium hooks are untouched by it.

## Consequences

Easier: a non-technical user connects GitHub, Google and Slack by signing in, as for Notion or Linear, and never creates an app or copies a token. No customer token exists anywhere but the user's own key store.

Harder, and what this commits Farik to:
- **Farik runs a cloud service before its launch**, from the kit check (phase 7 step 13) on. Cost: about $10 a month at launch traffic (the WAF's $5 and $1 a rule, two secrets at $0.40, the hosted zone's $0.50, the rest near zero), plus the domain's yearly fee. Uptime: one region, `us-east-1`, with no promise beyond AWS's own; when it is down, Slack sign-in and refresh wait, and keys still work. The domain must be registered before step 03c can be deployed, and phase 11 step 01 builds the website in the same `infra` package, which step 03c creates.
- **An AWS account the founder provides and owns**, with IAM Identity Center and MFA, CDK bootstrapped, and the first deploy run by the founder; CI deploys afterwards through an OpenID Connect role restricted to one GitHub environment the founder approves. An agent never creates an account or holds the founder's cloud credentials.
- **Farik's app registrations are founder-owned accounts and launch dependencies**: the GitHub App, the Google Cloud project and its consent screen, and the Slack app with public distribution. Their client ids are public and committed; Slack's secret lives only in Secrets Manager.
- **Google's verification has a lead time.** Sensitive scopes take days to weeks of review, and a restricted scope (`drive.readonly`) adds a third-party security assessment, repeated yearly. Google also needs a homepage and a privacy policy on a domain the founder has verified. Until verified, only named test users can sign in, and their refresh tokens end after 7 days. The founder starts it as soon as the scopes are known.
- **A stolen refresh token is usable through the relay.** For a confidential client the secret would stop it; the relay removes that protection. It is the same exposure as a public client's, and the token is in the user's own keychain.
- **Anyone can call the relay with Farik's Slack app**, since Farik is open source; the consent page still names Farik, and abuse could get the app suspended by Slack. The limits slow it; they do not prevent it.
- **The premium tier.** Route 4, incoming hooks, needs a public address that receives a service's events for a user, which a stateless relay cannot offer; it is premium and planned with phase 14, distinct from F13's stubs (phase 9 step 04). Hosted execution (spec 9) would run the same routes from the cloud.
