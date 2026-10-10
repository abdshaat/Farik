# 0021. The web UI's driver, connection, and first run

Date: 2026-09-28
Status: accepted
Amended 2026-09-29 by ADR 0023: phase numbers after 6 moved up by one.
Amended 2026-09-29: the first-run order is computer, AI account, project (the founder); the credential falls back to a private file (ADR 0022).
Amended 2026-09-30 by ADR 0025: phase numbers after 9 moved up by one.
Amended 2026-10-05 by ADR 0040: a phase, Business workspaces, follows the web launch as phase 12, so the phases after it moved up by one (Desktop 13, Native mobile 14, Premium 15); the numbers below are the old ones.
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"): a phase, Farik Cloud, follows the role kits as phase 8, so the phases after phase 7 moved up by one (Engines and providers 9, Ecosystem 10, Proof of concept 11, Web launch 12, Business workspaces 13, Desktop 14, Native mobile 15, Premium 16); the numbers below are the old ones.
Amended 2026-10-09 by ADR 0049 (the founder: "Lets stop this phase after completing step 10g. We have to start the next phase"; "DevOps later, rest after Cloud"): a phase, Ask or auto and the milestones, follows Farik Cloud as phase 9, so the phases after phase 8 moved up by one (Engines and providers 10, Ecosystem 11, Proof of concept 12, Web launch 13, Business workspaces 14, Desktop 15, Native mobile 16, Premium 17); the numbers below are the old ones.
Amended 2026-10-09 by ADR 0053 (the founder, approving `docs/design/change-project.md`): decision 2, one process serves one project, now holds one project at a time; `farik serve` can switch it from the browser (`project.leave`), keeping its port and the browser's session.

## Context

Phase 6 builds the web UI for a non-technical user (ADR 0016), and its planning found four things the plan left open or got wrong.

- **The brand's assets were never built.** Phase 5 merged in pull request #16 with the brief and the web UI's design alone. Its step 02 (the pnpm workspace, `@farik/brand`'s tokens and icons, the contrast test, the brand sheet) was never planned or built, and phase 6's component library is built from those tokens.
- **Nothing drives a team for a browser.** `farik run` drives a project until it is idle and then exits, and it must be started inside the project. The first-run wizard's first question, "a project you already have, or a new one", needs a process that starts before any project exists and keeps driving once one does.
- **A browser cannot hand Farik a folder.** The file pickers a page can open give it handles, not paths.
- **A non-technical user cannot set an environment variable.** Sessions read the model credential from `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN` alone. The keychain was planned for MCP credentials in phase 7.

## Decision

The founder decided on 2026-09-28:

1. **Phase 5 step 02 becomes phase 6 step 01**, unchanged in scope. Phase 5 is recorded as merged with its step 01. It is not reopened as a branch of its own, because the web UI is its only consumer and one pull request keeps the two together.
2. **`farik serve` is the always-on driver.**
   - It starts in any folder, opens the project there or the last one it served, and otherwise serves the wizard with no project.
   - It holds `run.lock` as `farik run` does, so the command line reaches it (ADR 0014). It never exits when idle.
   - The team can be paused and resumed (`team.paused`, `team.resumed`). The pause is read back from the log, so it survives a restart.
   - One process serves one project. It listens on `127.0.0.1:7420`, else 7421 to 7429, else any free port.
3. **The browser connects with a one-time code for a browser session.**
   - The link `farik serve` prints and opens carries a code that works once. The page trades it for a session cookie.
   - Browser sessions last 30 days, are kept hashed in the user's Farik state folder, and can be revoked from Settings.
   - `/rpc` needs the cookie, the daemon's own `Origin`, and its own `Host`.
4. **The folder is chosen with an in-app folder browser.** The daemon lists folders under the user's home directory. A new project is a folder Farik creates there and runs `git init` in. A paid member's cloud project is phase 12's.
5. **The first run connects the AI account, and the credential goes into the OS keychain** through the `keyring` crate: an API key, or a subscription token made with `claude setup-token`. The environment variables still win. The keychain adapter moves from phase 7 step 01 to phase 6 step 05, and phase 7 reuses it.
6. **The Milestone 1 test starts with Farik, Claude Code, git, and Docker installed.** The wizard checks each and explains any fix. Installing them is phase 9's.
7. **Playwright end-to-end tests** drive a real daemon with recorded transcripts, under `cargo xtask check --integration`.

## Consequences

Phase 6 has ten steps rather than eight, and its Milestone exit is step 10. The plan's crate table, coverage table, and step references move with it (revision 18).

The daemon gains a second kind of client, a browser with a cookie, beside the bearer token of `daemon.json`. That adds attack surface: a stolen cookie drives the team for up to 30 days. It is `HttpOnly` and `SameSite=Strict`, it is honoured only on loopback with the daemon's own origin and host, and Settings revokes it.

The model credential can now live outside the environment, so `farik doctor` and the no-sandbox warning (spec 8.6) must say where it came from. In no-sandbox mode it still reaches the Claude Code process's environment, as before.

A paused team spends nothing, and a team left running spends while the user is away. The limits the user sets in the wizard (ADR 0015) are what bound that.

Playwright downloads a browser (about 150 MB) for local integration runs and CI.
