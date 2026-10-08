# 0033. Signing in to a connector's service

Date: 2026-10-02
Status: accepted
Amended 2026-10-06 by phase 7 step 08e: a `stdio` entry that is Farik's own connector may carry `oauth`. See "Amendment of 2026-10-06".
Amended 2026-10-06 by ADR 0043: until phase 15 the app a service's sign-in uses is the customer's own, never Farik's, and client metadata documents wait for phase 15. See "Amendment by ADR 0043".
Amended 2026-10-06 by ADR 0044: the customer's own app is dropped; from the web launch, phase 11, a service that registers no client signs in with Farik's own app through Farik Cloud, and before then takes a pasted key or is not offered. See "Amendment by ADR 0044".
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"): Farik Cloud's sign-ins start in a new phase 8, after the role kits, not at the web launch, now phase 12; whether client metadata documents come with Farik Cloud's site is phase 8's to decide; Premium is phase 16. The phases after phase 7 moved up by one; the numbers below are the old ones.

## Context

After phase 7 step 01 an http connector takes pasted keys only. Most services that matter to a non-technical user (ADR 0016) offer signing in with OAuth instead, and ADR 0035 says Farik should not ask for a pasted key where one is not needed. Claude Code cannot do it for Farik: in `-p` it cannot sign in, it keeps tokens keyed by its config folder and not by agent, in one shared blob that concurrent processes overwrite, and the daemon could neither confirm the definition (ADR 0030), refresh before a session, nor show "Sign in again".

## Decision

**Farik runs the sign-in.** The daemon runs it for the web app and `farik connect`'s own process for the command line, through one function over `rmcp`'s `auth` feature, which does discovery (RFC 9728, 8414), registration, PKCE S256, `resource` (RFC 8707) and the `iss` rule (RFC 9207). Farik adds an https-only HTTP client (https, or http on a loopback host for the test fixture), refuses metadata that does not offer S256, never guesses endpoints, and checks `state` itself before calling `rmcp`.

**The grant is kept per agent where the keys are** (ADR 0030): in the entry, in the keychain or the private file. The access token reaches a session as `Authorization: Bearer` from the headers helper, the only way a secret reaches Claude Code. No RPC answer, event, team file, `mcp.json` or log holds a token.

**Registration, in order:** a `client_id` the team file names (a public client; Farik never takes a client secret), else dynamic registration (`client_name` "Farik", `application_type` native, `token_endpoint_auth_method` none), else refused. Client metadata documents are not built: they need a document at an https address Farik owns, so they wait for the site of the web launch (amended 2026-10-06 by ADR 0043: for phase 15; and by ADR 0044: phase 11's brainstorm decides whether they come with Farik Cloud's site).

**The redirect is `http://localhost:<port>/callback`.** The listener binds `127.0.0.1` (and `[::1]` where there is IPv6), never `0.0.0.0`, answers the first callback whose `state` is the attempt's, serves a page that quotes nothing the service sent, and closes. Services match a registered redirect exactly, and `localhost` is the name that works. The port is any free one with registration, or the client's registered one (33418 by default).

**Refresh and revocation are Farik's own form POSTs** to the grant's endpoints, through the same https check: `rmcp` has no revocation, and keeps refresh on a manager the keychain cannot rebuild. A session refreshes a token that would expire during it and saves the rotated refresh token first. A refusal (`invalid_grant`, `invalid_client`, `unauthorized_client`) lapses the grant: the server is left out of sessions and the page offers "Sign in again". One lock per entry serialises refresh, connect, disconnect and the deletes. "Remove" deletes the entry, then asks the service to forget the grant, best effort.

## Consequences

- The loopback redirect assumes the browser and the daemon share a machine, which ADR 0021 guarantees (the daemon serves only `127.0.0.1`). Phase 11's hosted web launch needs another redirect.
- Each dynamic registration leaves a client at the service. Revoking on replace limits it; client metadata documents end it (the founder's question O1).
- Any local process can bind a port and receive a code; PKCE makes it useless without the verifier, which stays in the attempt's memory.
- `reqwest` and `oauth2` come in through `rmcp`'s `auth` feature; `reqwest` is a direct dependency of `farik-runtime` at the version `rmcp` locks.
- Services with neither registration nor a client id (GitHub, Slack, Google) sign in only with a `client_id` a kit or the user supplies (ADR 0035); until phase 15, the customer's own app (ADR 0043); amended 2026-10-06 by ADR 0044: from phase 11, Farik's own app through Farik Cloud, and before then a pasted key, or no sign-in for Google Ads.

## Amendment of 2026-10-06

Made by phase 7 step 08e (ADR 0042 pulls Google's sign-in forward for Farik's own Google Ads connection).

**`oauth` may be on Farik's own connector.** The text above says an `http` server may sign in and `validate_team` refuses `oauth` on a `stdio` one. One `stdio` entry is exempt: Farik's own connector, the exact pair `farik connector <name>` (ADR 0038), whose `<name>` the kit loader holds to the names Farik ships. Its sign-in is Farik's own app's, from the table of registered apps (ADR 0035, route 2), so the entry takes no `client_id` or `callback_port` of its own (`farik_connector_client`); every other rule holds (no keys beside it, `oauth_with_keys`), and `spec_sha256` holds `oauth` for a `stdio` entry only when present, so every hash kept stands. The connector never receives the sign-in: the grant stays in the daemon and `farik connect`'s own process, and the launch route answers the connector's command with an empty environment.

**The loopback flow is Farik's own requests for this route.** "One function over `rmcp`'s `auth` feature" holds for a server at an address. For Farik's own connector there is no server whose metadata to discover, so Farik builds the authorization address, checks `state` and `iss`, and makes the exchange and the refresh itself, with `oauth2`'s PKCE and `state` generators, over the same https-or-loopback client and the same listener. Rejected: an `http` entry at a Google host, which would hand the access token to a server process.

## Amendment by ADR 0043

Made 2026-10-06 (the founder: until phase 15, every login is the customer's).

- **Farik's own connector signs in with the customer's app for its provider**, not with an app of Farik's. The amendment above says "Its sign-in is Farik's own app's, from the table of registered apps"; from phase 7 step 03f the table holds each provider's fixed facts, and the client id and secret are the ones the customer gave Farik in the machine's settings. The team file still takes no `client_id` or `callback_port` on that entry (`farik_connector_client`), because the customer's app is kept on the computer, not in the project; only the refusal's sentence changes.
- **Client metadata documents** need a document at an address Farik owns, so they are phase 15's (step 03's O1), with Farik's own apps.
- **Registration's order is unchanged**: a `client_id` the team file names, else dynamic registration, else refused. A `client_id` of a customer's sign-in app is used only for its own provider's servers, as step 03b's rule held for Farik's.

## Amendment by ADR 0044

Made 2026-10-06, later the same day (the founder: "Lets change the customer's own app"; Farik Cloud's free tier "At the web launch").

- **Farik's own connector signs in with Farik's own Google app, through Farik Cloud, from phase 11.** The amendment by ADR 0043 above, the customer's app for its provider, is dropped with step 03f. Before phase 11 no build has a Google entry, so a Farik connector's sign-in is `sign_in_not_supported`, and its tests pass tables of their own. The team file still takes no `client_id` or `callback_port` on that entry (`farik_connector_client`): the app is Farik Cloud's, not the project's.
- **No client id of Farik's is in the code.** In the relay's design (ADR 0035, route 3) Farik Cloud answers the authorization address when a sign-in starts.
- **Registration's order is unchanged**: a `client_id` the team file names, else dynamic registration, else refused.
