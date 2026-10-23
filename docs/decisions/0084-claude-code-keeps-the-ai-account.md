# 0084. Claude Code keeps the AI account; Catervas never reads its key

Date: 2026-10-21
Status: accepted (the founder, 2026-10-12 and 2026-10-21, quoted below; the mechanisms checked live by the planner on 2026-10-21)

Supersedes ADR 0022. Amends ADR 0021 (the first run connects the AI account) and SPEC 4.1, 5.5, 8.2 and 8.6.

## Context

The founder, 2026-10-12, while planning phase 10 (ADR 0059): "We are not storing or keeping any anthropic related keys with us. Anthropic keys will remain local to the user and will never be read by Catervas." Then on 2026-10-21: "Catervas should pass the key for verification without reading it."

What Catervas did until now (ADR 0021, ADR 0022):

- The first run took an API key or a subscription token (`claude setup-token`'s) in the browser.
- It checked the key's prefix and kept the key in the OS keychain, or in `credential.json` where there is no keychain.
- It gave the key to every `claude` session as `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN`.

So Catervas read the key, held it and passed it on. If the key comes in through Catervas's page, it is in Catervas's memory however carefully it is handled. A key is never read only when it goes from the user to Claude Code directly, and Catervas only starts `claude`.

The planner checked Claude Code 2.1.306 on 2026-10-21:

- `claude auth status --json` answers `{ "loggedIn", "authMethod", "apiProvider", ... }`. It costs nothing and holds no secret.
- `claude auth login` run with no terminal prints "If the browser didn't open, visit: <https://claude.com/cai/oauth/authorize?...>" and then "Paste code here if prompted >", and reads the code from its standard input. Its redirect is `https://platform.claude.com/oauth/code/callback`, a page that shows the code, so it works on any computer, WSL and servers included. `--console` signs in to an API (Console) account instead. Claude Code keeps what it receives in its own store: the macOS keychain, else `~/.claude/.credentials.json`.
- A lean check costs $0.00039 on Haiku:

  ```
  claude -p "Reply with the single word OK." --model haiku --max-turns 1 --tools "" --system-prompt "Reply briefly." --strict-mcp-config --setting-sources "" --output-format json
  ```

  A good account answers `subtype: success`, `is_error: false`, `result: "OK"`. A bad key answers `is_error: true`, `api_error_status: 401`, `result: "Invalid API key · Fix external API key"`, with exit status 1. Claude Code's plain `-p` with its default system prompt cost $0.046 for the same word.

The founder's answers, 2026-10-21: the one-time sign-in code may pass through Catervas ("Yeah the one time sign in code may pass by catervas"), and the check may spend a few tokens ("a few tokens for verification is good. You may spend some tokens to verify connection").

## Decision

1. **Claude Code holds the AI account.** Catervas keeps no key and reads none. The keychain entry, `credential.json`, the paste field, the prefix checks and `ClaudeCredential` go. A `claude` session gets Catervas's base environment, with `CLAUDE_CONFIG_DIR` passed through when it is set, and finds its own sign-in.
2. **A key in the user's own environment passes through unread.** When `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN` is set where Catervas was started, Catervas passes the variable on to `claude` as it found it. It looks only at whether the variable is set, to say "your AI account comes from ANTHROPIC_API_KEY". It never parses, prints, logs or keeps the value.
3. **Signing in goes through Claude Code's own login, shown in the browser.** "Sign in with Claude" (a subscription) or "Sign in with an Anthropic Console account" (`--console`) starts `claude auth login` with no terminal. Catervas reads the sign-in address from its output and opens it in a new tab. The owner pastes the code the page shows into Catervas, which writes it to the login's input and drops it. The code is never kept, logged or echoed. One sign-in runs at a time, and it is ended after 10 minutes.
4. **Signed in or not is asked of Claude Code.** `claude auth status --json`, free, answers `account.status` and every start. Catervas reads `loggedIn` and the kind of `authMethod`, nothing else.
5. **The connection is checked with a few tokens.** After a sign-in, and when the owner presses "Check connection", Catervas runs the lean check above in an empty folder of its own, with a 60-second limit. It reads only the answer's outcome: working, the key refused (401 or 403), the plan's limit reached (429, or a rate or usage limit in the text), not signed in, or another failure in Claude Code's words. A check is not a session: it holds no task and is counted in no budget. It costs well under a cent on an API account.
6. **Signing out is Claude Code's logout.** "Sign out" runs `claude auth logout` and pauses a running team, as Disconnect did. An account from the environment cannot be signed out from Catervas; the page says which variable to remove.
7. **The key Catervas kept is removed once, unread.** On the first start of this version, Catervas deletes its keychain entry (`catervas` / `anthropic`) and `credential.json` without reading either. It says so once on the command line and on the AI account's page, and the owner signs in through Claude Code. `scripts/catervas-reset.sh --keep-key` keeps Claude Code's sign-in instead.
8. **A refused key still pauses the team** (SPEC 5.5, unchanged). Signing in again, or a passing check, resumes a team that Catervas paused for its key, as connecting did.

Rejected:

- **Pasting the key into Catervas and handing it on unread.** It is still read into Catervas's memory and kept by Catervas, which is what the founder ruled out.
- **The owner signing in in a terminal only.** Catervas is for people with no terminal (ADR 0016), and the founder allowed the sign-in code through Catervas.
- **`claude auth status` as the only check.** It says a login exists, not that it works: an expired token or a revoked key still answers `loggedIn: true`.
- **A check with Claude Code's default prompt.** It cost about a hundred times the lean one.

## Consequences

- One AI account per computer, Claude Code's. Every team on the computer uses whatever Claude Code is signed in to. Phase 10's several teams share it, as ADR 0059 already said.
- Catervas can no longer say "that is not an API key" before trying one. It reports what Claude Code says.
- A user who already uses Claude Code is signed in with nothing to do. The first run says so and runs the check.
- Claude Code's own store is where the token lives, with the exposure it has today. Catervas never opens it (ADR 0059 already forbids `.credentials.json`).
- Sign-in depends on `claude auth login`'s output. Its address line, "If the browser didn't open, visit:", is matched by the first `https://` address on a line, held to Anthropic's hosts. A change in Claude Code that breaks it fails the sign-in with Claude Code's own words and the terminal command as the fallback, which a test with a fake `claude` keeps honest.
