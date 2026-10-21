# Claude Code keeps the AI account, plan 3: the screens

Status: draft. It waits on the founder's approval of `docs/design/mockups/AccountSignIn.dc.html` (ADR 0032), which the planner draws from the screens below. The plan is folded to match the approved mockup before it runs.
Branch: `feat/claude-code-signs-in`
Spec: `docs/SPEC.md` 4.1, and the AI account section of Settings
Depends on: plan 2 (Tasks 5 to 9 committed on this branch)

## Goal

The first run's "Connect your AI account" step and Settings' "Your AI account" section sign in through Claude Code. Neither asks for a key. Today's and the chats' "the key did not work" lines link to the same section, which offers "Sign in again".

## The screens (what the mockup draws)

**First run, `/setup/account`** (`SetupAccount.tsx`). Title: "Connect your AI account". Lead: "Your agents think with Claude, and your account pays for it. You sign in through Claude Code, which keeps your sign-in on this computer. Catervas never sees your key." It has four states:

1. **Already signed in.**
   - The step says "Claude Code is signed in with your Claude subscription." (or "…with your Anthropic Console account.", "…with ANTHROPIC_API_KEY from where you started Catervas.", "…with another sign-in.") and "Checking the connection…".
   - Then it says "It works." with Continue.
   - When the check fails, it says why: "Your key was refused.", "Your plan's limit is reached for now.", or "It didn't work: <detail>". With that it shows the sign-in choices below and "Check again".
2. **Not signed in.** The step shows two choices:
   - "Sign in with Claude (Pro or Max)", the default and recommended;
   - "Sign in with an Anthropic Console account" (pay for what your agents use).

   Each choice is a button.
3. **Waiting for the code.**
   - Pressing a choice opens Claude's page in a new tab and says "Sign in on the page that opened. It shows a code at the end: paste it here."
   - It has a code field ("Your sign-in code"), "Finish signing in", and "Open the page again", a link to the same address.
   - Below them is "Prefer the Terminal? Run claude auth login, then press Check again."
4. **Checking**, then 1's outcome. A working check that takes the waiting project on reopens as the wizard does today; otherwise it goes to `/setup/project`. `storedKeychain` and `storedFile` go from `SetupProject.tsx`.

**Settings, Your AI account** (`settings/Account.tsx`).
- The row reads "Claude, through Claude Code: signed in with your Claude subscription." (and the other three forms), or "Not signed in.".
- It has the buttons "Check connection" and "Sign out", the second with a confirm: "Sign out of Claude Code? The team pauses until you sign in again." and "Sign out".
- When the kind is `environment`, the section says "It comes from ANTHROPIC_API_KEY where you start Catervas. Remove it there to sign in here instead." and shows no Sign out.
- When the key was refused, or when not signed in, the section shows the first run's sign-in choices (state 2 and 3) under "Sign in again".
- With `kept_key_removed`, a note reads "Catervas no longer keeps your key. It removed the one it kept, so sign in through Claude Code once."
- `ConnectAgain.tsx` is replaced by a shared `ClaudeSignIn.tsx`, which holds states 2 and 3 and their outcome, used by both pages.

**Today and chats.** `waitingKeyRefused*` (`en.ts:1113-1121`) and `chatsKeyRefused` say "Your AI account's key was refused. Sign in again in Settings." and link to `/settings/account`.

## Decisions

- **The sign-in page opens on the press**: `window.open(url, "_blank", "noopener")` inside the click's handler, so a popup blocker allows it. If it returns `null`, the step shows the address as a link, "Open Claude's sign-in page".
- **The code field** is `type="password"`, `autocomplete="off"`, trimmed. It is sent once and cleared on any answer.
- **Refusals** map in `refusals.ts` by plan 2's sentences. These go: `setupNotSubscription`, `setupNotApiKey`, `setupNoKeep`.
- **Strings**: `en.ts`'s account strings (147-167, 506-526, 874-889, 924-926) are replaced by the screens' words above. Unused keys are removed.

## File map

```
docs/design/mockups/AccountSignIn.dc.html        creates (planner, before Task 10): the four states and Settings, on the canvas page "AI account"
apps/web/src/pages/ClaudeSignIn.tsx              creates: the shared sign-in (choices, code, outcome)
apps/web/src/pages/setup/SetupAccount.tsx        rewrites: the four states
apps/web/src/pages/setup/SetupProject.tsx        modifies: no "stored in" line
apps/web/src/pages/settings/Account.tsx          rewrites: status, check, sign out, sign in again
apps/web/src/pages/ConnectAgain.tsx              deletes
apps/web/src/pages/Today.tsx, OneToOne.tsx       modifies: the refused-key line's words
apps/web/src/app/{refusals,store}.ts, landing     modifies: refusals; ServeStatus.signedIn
apps/web/src/strings/en.ts                       modifies: the strings above
apps/web/src/pages/setup/setup.test.tsx, team.test.tsx, Today.test.tsx, app/landing.test.ts   tests
```

## Tasks

### Task 10: Signing in on the first run

Tests (`setup.test.tsx`, replacing `connects_a_subscription_key`, :149, and `says_each_setup_refusal_in_plain_words`, :204):

- `signs_in_through_claude_code_and_continues`: asserts this sequence.
  - With `account.status` not signed in, pressing "Sign in with Claude" calls `account.sign_in { console: false }` and `window.open` with the answered address.
  - Typing `abc` and pressing "Finish signing in" sends `account.sign_in_code { code: "abc" }`.
  - On `{ outcome: "working" }` it navigates to `/setup/project`.
  - The Console button sends `console: true`.
- `checks_an_account_already_signed_in`: asserts these.
  - With status signed in (subscription), the step calls `account.check` with no press and shows "It works." with Continue.
  - On `key_refused` it shows "Your key was refused." and the two sign-in choices.
- `says_each_sign_in_refusal_in_plain_words`: asserts that each of plan 2's sentences shows its plain words.

- [ ] `feat(web): sign in through claude code on the first run`

### Task 11: Settings' AI account

Tests (`team.test.tsx`, replacing those at :767, :1533, :1613, :1677, :1739, :1764):

- `checks_and_signs_out_of_the_ai_account`: asserts these.
  - "Check connection" calls `account.check` and shows its outcome.
  - "Sign out", then the confirm's "Sign out", calls `account.sign_out` and shows "Not signed in." with the sign-in choices.
- `says_the_account_comes_from_the_environment`: asserts that with kind `environment` and `environmentVariable: "ANTHROPIC_API_KEY"`, the section shows the environment sentence and no Sign out.
- `offers_sign_in_again_after_a_refused_key`: asserts that with `keyRefused: true` the section shows "Sign in again" with both choices.

- [ ] `feat(web): check, sign out and sign in again in settings`

### Task 12: The refused key's lines, and the landing

Tests: `says_the_key_did_not_work_and_links_to_settings` (`Today.test.tsx:1031`) asserts the new words and the link. `landing.test.ts:11` asserts that `signedIn: false` lands on `/setup/account`.

- [ ] `feat(web): send a refused key to sign in again`

## Verification

```
pnpm -C apps/web check
python3 reaper.py cargo xtask check
# expected: xtask check: ok
```
