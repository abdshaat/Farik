# Standalone: connect the AI account again after Disconnect

Status: executed
Reviewed by: a fresh Opus 5.5 session, 2026-10-11 (one round; its finding on the refused-key path fixed)
Branch: `fix/reconnect-ai-account` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 8, the web UI's AI account section (revision 0.87)
Depends on: `account.connect` on a project's daemon (revision 0.30, on main)

## Goal

The founder, 2026-10-11: "if I disconnected my AI account I cannot reconnect it again without restarting the local server. Please look into this issue and fix it."

After Disconnect, Settings' AI account section said only "No AI account is connected." and "start Catervas again to connect another account": it offered the key field only while the provider refused the key. The daemon already answers `account.connect` with a project open, keeps the key and puts it in place for the next session (revision 0.30), so the gap is the page. Now, with no account connected (and none from the environment), the section offers the same key field with a **Connect account** button; after connecting it reads the account again, offers Disconnect again, and says to press Resume, since Disconnect paused the team and connecting does not resume a pause the user caused.

Out of scope: a team resumed after Disconnect without a new key still runs on the key the sessions already loaded, as the spec has said since 0.26; and `serve.status` names that kind until another account is connected.

## Decisions

- `ConnectAgain` gains `refused?: boolean` (default `true`). With `refused={false}` it drops the "key did not work" alert and its button reads "Connect account" (`accountConnect`); otherwise it is unchanged.
- `Account` hides Disconnect with a `gone` flag, set by Disconnect and cleared by a connect, rather than by any message being shown, so Disconnect returns once another account is connected.
- A refused key connected again after Disconnect resumes the team (revision 0.30), so that path clears both `gone` and the message rather than saying `accountBack`.
- Words: `accountGone` becomes "Disconnected. The team is paused until you connect an account and press Resume."; new `accountConnect` "Connect account" and `accountBack` "Connected. If your team is paused, press Resume."

## Tasks

### Task 1: connect another account after Disconnect

- [x] `team.test.tsx`: `shows_disconnects_and_connects_the_account_again` extends the disconnect test: after Disconnect the row offers Connect (disabled until a key is typed, no refused-key alert), sends `account.connect { kind, secret }`, reads the account again, says `accountBack`, no longer says `accountGone`, and offers Disconnect again. Watched failing: no Connect button.
- [x] `team.rs`: `disconnects_the_account_pauses_and_connects_another` pins the daemon side: after `account.disconnect`, `account.connect` keeps the key, replaces the credential in use, `account.status` names it, and the team stays paused.
- [x] `team.test.tsx`: `connects_a_refused_key_again_after_disconnecting_it`: with the key refused, Disconnect, then a connect through the refused-key form, shows one key form only, and brings Disconnect back without `accountGone` (from the review).
- [x] Mutation: dropping `setGone(false)`, `refused={false}`, the connect form, the refused path's own handler, or the guard against a second form each fails a web test; not replacing the credential in use fails the Rust test.
- [x] Spec revision 0.87.
