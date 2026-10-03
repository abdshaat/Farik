# 0033. Signing in to a connector's service

Date: 2026-10-02
Status: accepted

## Context

After phase 7 step 01 an http connector takes pasted keys only. Most services that matter to a non-technical user (ADR 0016) offer signing in with OAuth instead, and ADR 0035 says Farik should not ask for a pasted key where one is not needed. Claude Code cannot do it for Farik: in `-p` it cannot sign in, it keeps tokens keyed by its config folder and not by agent, in one shared blob that concurrent processes overwrite, and the daemon could neither confirm the definition (ADR 0030), refresh before a session, nor show "Sign in again".

## Decision

**Farik runs the sign-in.** The daemon runs it for the web app and `farik connect`'s own process for the command line, through one function over `rmcp`'s `auth` feature, which does discovery (RFC 9728, 8414), registration, PKCE S256, `resource` (RFC 8707) and the `iss` rule (RFC 9207). Farik adds an https-only HTTP client (https, or http on a loopback host for the test fixture), refuses metadata that does not offer S256, never guesses endpoints, and checks `state` itself before calling `rmcp`.

**The grant is kept per agent where the keys are** (ADR 0030): in the entry, in the keychain or the private file. The access token reaches a session as `Authorization: Bearer` from the headers helper, the only way a secret reaches Claude Code. No RPC answer, event, team file, `mcp.json` or log holds a token.

**Registration, in order:** a `client_id` the team file names (a public client; Farik never takes a client secret), else dynamic registration (`client_name` "Farik", `application_type` native, `token_endpoint_auth_method` none), else refused. Client metadata documents are not built: they need a document at an https address Farik owns, so they wait for the site of the web launch.

**The redirect is `http://localhost:<port>/callback`.** The listener binds `127.0.0.1` (and `[::1]` where there is IPv6), never `0.0.0.0`, answers the first callback whose `state` is the attempt's, serves a page that quotes nothing the service sent, and closes. Services match a registered redirect exactly, and `localhost` is the name that works. The port is any free one with registration, or the client's registered one (33418 by default).

**Refresh and revocation are Farik's own form POSTs** to the grant's endpoints, through the same https check: `rmcp` has no revocation, and keeps refresh on a manager the keychain cannot rebuild. A session refreshes a token that would expire during it and saves the rotated refresh token first. A refusal (`invalid_grant`, `invalid_client`, `unauthorized_client`) lapses the grant: the server is left out of sessions and the page offers "Sign in again". One lock per entry serialises refresh, connect, disconnect and the deletes. "Remove" deletes the entry, then asks the service to forget the grant, best effort.

## Consequences

- The loopback redirect assumes the browser and the daemon share a machine, which ADR 0021 guarantees (the daemon serves only `127.0.0.1`). Phase 11's hosted web launch needs another redirect.
- Each dynamic registration leaves a client at the service. Revoking on replace limits it; client metadata documents end it (the founder's question O1).
- Any local process can bind a port and receive a code; PKCE makes it useless without the verifier, which stays in the attempt's memory.
- `reqwest` and `oauth2` come in through `rmcp`'s `auth` feature; `reqwest` is a direct dependency of `farik-runtime` at the version `rmcp` locks.
- Services with neither registration nor a client id (GitHub, Slack, Google) sign in only with a `client_id` a kit or the user supplies (ADR 0035).
