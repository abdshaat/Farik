# Standalone: Claude Code keeps the AI account

Status: draft. The web part waits on the founder's approval of its mockups (ADR 0032); parts 01 and 02 are ready.
Branch: `feat/claude-code-signs-in`, its own pull request to `main`. It must merge before phase 10 step 05 (ADR 0059's "no key read" ruling, project plan revision 46).
Decision: ADR 0084 (supersedes ADR 0022, amends ADR 0021)
Spec: `docs/SPEC.md` 4.1, 5.5, 8.2, 8.6, and the AI account section of Settings
Depends on: `main` at `d6599bd` (pull request #88 merged)
Readiness confirmed by: the planner (Opus 5.5), 2026-10-21, against the three readiness rules (ADR 0065), for parts 01, 02 and 04; part 03 after its mockups are approved

## Goal

Catervas never reads, keeps or passes on the key of the owner's AI account. Claude Code holds it. In the first run and in Settings, the owner presses "Sign in with Claude" or "Sign in with an Anthropic Console account". Catervas starts `claude auth login`, opens its sign-in page in a new tab, takes the code that page shows and hands it to Claude Code, then checks the connection with one tiny request (about $0.0004 on an API account). A user whose Claude Code is already signed in only sees the check. "Check connection" runs the same request again at any time, and "Sign out" runs `claude auth logout`. A key the owner set in their own environment (`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`) passes through to `claude` unread. The key Catervas kept before this version is deleted, unread, on the first start.

Out of scope:

- Other providers and engines (phase 13).
- Several AI accounts on one computer.
- Reading anything of Claude Code's own store (ADR 0059 forbids it).

## Plans, in order

| File | Tasks | Where |
|---|---|---|
| `01-account.md` | 1 to 4 | this branch |
| `02-daemon-and-cli.md` | 5 to 9 | this branch, after 01 |
| `03-web.md` | 10 to 12 | this branch, after 02, once the founder has approved `docs/design/mockups/AccountSignIn.dc.html` |
| `04-record.md` | 13 to 14 | this branch, after 03 |

## Decisions shared by the plans

- **What Catervas reads of Claude Code.** It reads only these, each parsed in `catervas-core` from strings, so core stays free of I/O:
  - `claude auth status --json`: `loggedIn` and `authMethod`.
  - `claude auth login`'s output: the first `https://` address on a line, held to the hosts `claude.com`, `claude.ai`, `platform.claude.com` and `console.anthropic.com`.
  - The check's JSON answer: `is_error`, `api_error_status`, `terminal_reason` and `result`.

  Nothing else is parsed or kept.
- **The kinds.** `authMethod` is mapped to a kind:

  | `authMethod` contains | Kind |
  |---|---|
  | `api_key` or `apiKey` | `console` |
  | `oauth` or `claude` | `subscription` |
  | anything else | `other` |

  When a credential variable is set in Catervas's environment, the kind is `environment`, with the variable's name. The value is never read: only whether it is non-empty after trimming.
- **The check** is exactly ADR 0084's command. It runs in `<state_dir>/account-check/`, an empty folder Catervas makes, with Catervas's base environment for sessions and a 60-second limit; past the limit the whole process group is killed. Its outcomes:

  | Answer | Outcome |
  |---|---|
  | `is_error: false` | `working` |
  | `api_error_status` 401 or 403 | `key_refused` |
  | 429, or `result` containing "usage limit" or "rate limit" (any case) | `limit_reached` |
  | `claude auth status` first said `loggedIn: false` | `not_signed_in` (the request is not sent) |
  | anything else, a timeout or output that is not JSON | `failed`, with `result`, or "Claude Code did not answer in time", or its first 200 characters of standard error |
- **One sign-in at a time, per daemon.** Starting one ends any earlier attempt. An attempt that has had no code after 10 minutes is ended. The code is written with a newline to the login's standard input, after which Catervas waits up to 60 seconds for it to exit. Exit status 0 is followed by the check; any other status is `sign_in_failed` with the login's last line of output.
- **The code and every refusal about it are never echoed or logged.** `account.sign_in_code` joins `SECRET_METHODS` (`daemon/web.rs:655`).
- **No event records a sign-in or a check.** They are the computer's, not a team's. The team's existing `team.paused { reason: credential_refused }` and resume are kept.
