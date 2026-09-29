# 0022. The model credential falls back to a private file

Date: 2026-09-29
Status: accepted

## Context

ADR 0021 put the model credential that the first run collects in the OS keychain, through the `keyring` crate. Spec 8.6 said that it is "never written to a file".

On Linux, `keyring` 4.2.0's default store is the desktop's Secret Service, reached over D-Bus. A machine without one has no keychain at all. That covers a server, a minimal install, and WSL. The founder's own WSL machine was checked on 2026-09-29: it has a session bus but no secret service. So the wizard could not finish there on its own.

The options were:
- **A private file when no keychain answers.** This is what Claude Code itself does on Linux.
- **The keychain only.** The screen tells the user to set an environment variable or install a keyring. The wizard cannot finish on WSL.
- **The kernel keyring on Linux.** Nothing goes to disk, but the key is lost at every reboot.

## Decision

The founder decided on 2026-09-29 that Farik tries the OS keychain first: service `farik`, account `anthropic`. When no keychain answers, Farik writes the credential to `credential.json` in the user's Farik state folder. That folder has mode 0700, and the file has mode 0600, readable by its owner alone.

The "Connect your AI account" screen says which of the two was used. `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN` in the environment still win over both. Spec 8.6's "never written to a file" becomes "in the keychain, or, where the computer has none, a file only its owner can read".

## Consequences

On a machine without a keychain, a program running as the same user can read the key. That is the same exposure as `daemon.json`'s token and Claude Code's own credentials file, and the no-sandbox warning already names it (spec 8.6).

Backups of the home folder can carry the file. The screen's line about where the key went lets the user know this.

The credential is read in order: environment, then keychain, then file. A key the user removes from one source may still be found in a later one. Disconnecting the account removes it from both stores; that control is step 06's, on the Team or Settings page.
