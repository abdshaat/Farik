# Claude Code keeps the AI account, plan 2: the wire, the daemon and the command line

Status: ready
Branch: `docs/claude-code-signs-in`
Spec: `docs/SPEC.md` 4.1, 5.5, 8.2, 8.6; ADR 0084
Depends on: plan 1 (Tasks 1 to 4 committed on this branch)
Readiness confirmed by: the planner (Opus 5.5), 2026-10-21 (ADR 0065)

## Goal

The following hold when this plan is done:

- Sessions run on Claude Code's own sign-in.
- The daemon answers `account.status`, `account.sign_in`, `account.sign_in_code`, `account.check` and `account.sign_out`, in the first run and with a project open. `account.connect` and `account.disconnect` are gone.
- `catervas start` and `catervas run` refuse to start when Claude Code is not signed in.
- The keychain entry and `credential.json` Catervas kept are deleted, unread, on the first start.

Out of scope:

- The screens (plan 3). Until plan 3 lands, the web app calls methods that no longer exist, so its account tests fail on this branch between the two plans; plan 3 fixes them, and this plan's Verification runs the Rust checks alone.
- The spec text (plan 4).

## Decisions

- **The wire.** `docs/schemas/rpc.schema.json` changes as follows:

  | Def | Shape |
  |---|---|
  | `accountStatusResult` | `{ signed_in: bool, kind: "subscription"\|"console"\|"environment"\|"other"\|null, environment_variable?: string, key_refused?: bool, kept_key_removed?: bool }`; `provider` and `source` are removed |
  | `accountSignInRequest` | method `account.sign_in`, params `{ console: bool }` |
  | `accountSignInResult` | `{ url: string }` |
  | `accountSignInCodeRequest` | method `account.sign_in_code`, params `{ code: string }`, 1 to 512 characters |
  | `accountCheckRequest` | method `account.check`, params `{}` |
  | `accountCheckResult` | answers both `account.check` and `account.sign_in_code`: `{ outcome: "working"\|"key_refused"\|"limit_reached"\|"not_signed_in"\|"failed", detail?: string }`, `detail` only with `failed` |
  | `accountSignOutRequest` | method `account.sign_out`, params `{}` |
  | `accountSignOutResult` | `{ paused: bool, environment_variable?: string }` |

  - `accountConnectRequest`, `accountConnectResult`, `accountDisconnectRequest` and `accountDisconnectResult` are deleted, along with their `oneOf` entries.
  - `serveStatusResult.credential` becomes `signed_in: bool`.
  - Each new request is written in the shape of the deleted `accountConnectRequest` (jsonrpc, id, method `const`, params with `additionalProperties: false`). Each is added to the request `oneOf`, and the result description at line 668 names the new results.
- **Which daemon answers what.** In the first run (setup mode, `daemon/web.rs`), `account.status`, `account.sign_in`, `account.sign_in_code` and `account.check` are `SETUP_METHODS`. With a project open, all five are `team.rs` methods; `account.sign_out` is offered only there, since a first run has no team to pause. `account.sign_in_code` joins `SECRET_METHODS`.
- **The daemon's account** is `WebState.account: Arc<dyn AccountPort>`, built once in `start.rs` and `serve.rs` from the session program and `SESSION_ENV`, with `check_dir` set to `<state_dir>/account-check`. `WebState.credential`, `stores` and `in_use` are removed. So are `SetupHost::connect` and `SetupHost::account`: setup mode asks `web.account` itself.
- **Taking on a waiting project.** `SetupHost::connect` answered `taking_on` when the first run had a project waiting for an account (`setup.rs:469-487`). It is replaced by `SetupHost::take_on_waiting(&self) -> Result<bool, SetupError>`, which takes the waiting project on, as `connect` did after saving. In setup mode, `account.sign_in_code` and `account.check` call it when their outcome is `working`. Their answer then gains `taking_on: bool`, added to `accountCheckResult` as optional.
- **Resuming a team paused for its key.** It happens after `account.sign_in_code` or `account.check` answers `working` while `pause::key_refused` holds. It sends `TeamResume` exactly as `team::connect` did (`team.rs:338-370`, including its "the human's own resume won" case).
- **Signing out** runs `sign_out()`. When it succeeds and the team is running, it pauses the team as `team::disconnect` did (`team.rs:2411-2443`), and answers `{ paused }`. With `kind: environment` it runs nothing and answers `{ paused: false, environment_variable }`.
- **Refusals**, each one sentence, lower case, no full stop (the daemon's style):

  | Case | Sentence |
  |---|---|
  | `SignInError::NoSignIn` | no sign-in is waiting for a code: press Sign in again |
  | `NoAddress(x)` | Claude Code did not give a sign-in address: `<x>` |
  | `Failed(x)` | Claude Code did not sign you in: `<x>` |
  | `sign_out` failing with `x` | Claude Code did not sign out: `<x>` |
  | `account.sign_in` while the kind is `environment` | your AI account comes from `<variable>`, which Catervas cannot change: remove it where you start Catervas to sign in here |

  The last one replaces the old connect refusal's sentence.
- **Sessions get no key of Catervas's.** In `claude.rs` these go: `ClaudeCredential`, `CredentialKind`, `SharedCredential`, `credential_from_env`, the insertion in `child_env`, and `ClaudeAdapter::new`'s credential parameter. `child_env` keeps the base env and `QUIET`.
  - `credential_variable` stays.
  - `Secret` stays: connectors use it.
  - `SESSION_ENV` (`start.rs:346`) gains `CLAUDE_CONFIG_DIR`, `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN`, each copied only when set in Catervas's environment, as the others are.
- **The kept key goes once.** `crates/runtime/src/credential.rs` is reduced to `pub fn forget_kept_key(state_dir: &Path, keychain: bool) -> bool`. It runs `keyring::Entry::new("catervas", "anthropic")?.delete_credential()` when `keychain` is true, ignoring `NoEntry` and a missing keychain, and removes `<state_dir>/credential.json` if present. It never calls `get_password` or reads the file. It answers whether anything was removed.
  - `start.rs` and `serve.rs` call it once at start. When it answers `true`, the command line prints `Catervas no longer keeps your AI account's key, so it removed the one it kept. Sign in through Claude Code: in the web app, or with claude auth login.` and `WebState.kept_key_removed` is set, which `account.status` reports until the daemon stops.
  - `system_credential_stores` and `CredentialStores` in `cli/src/lib.rs` go. `--no-keychain` stays and now means `keychain: false` to `forget_kept_key`.
- **Starting without a sign-in.** `start.rs:497-512` asks `account.status()`. When it is not signed in, it refuses: `Claude Code is not signed in: sign in on the AI account page of the web app, or run claude auth login, or set ANTHROPIC_API_KEY where you start Catervas`. `serve.rs:129-135` starts the wizard when not signed in, as it did with no credential. `run.rs:108-170` prints `account: Claude Code, signed in with a subscription` (or `an Anthropic Console account`, `ANTHROPIC_API_KEY from your environment`, or `another sign-in`), and its JSON gives `account: { signed_in, kind }`.

## File map

```
docs/schemas/rpc.schema.json                    modifies: the account defs above, serve.status's signed_in
crates/protocol/src/rpc.rs                      modifies, tests: the account methods' frames
packages/protocol-client/src/client.ts          modifies: the method names
packages/protocol-client/src/generated/rpc.ts   regenerated: `pnpm -F @catervas/protocol-client generate`
crates/runtime/src/claude.rs                    modifies, tests: no credential in a session
crates/runtime/tests/claude_process.rs          modifies: adapters made without a credential; env tests
crates/runtime/src/credential.rs                rewrites: forget_kept_key only
crates/runtime/src/daemon/web.rs                modifies, tests: WebState.account, setup-mode methods, serve.status
crates/runtime/src/daemon/setup.rs              modifies: SetupHost loses connect and account
crates/runtime/src/daemon/team.rs               modifies, tests: the five account methods, sign-out's pause
crates/cli/src/lib.rs                           modifies: no credential stores
crates/cli/src/main.rs, bin/catervas-e2e-serve.rs modifies: no stores; --no-keychain as forget_kept_key's flag
crates/cli/src/start.rs                         modifies: SESSION_ENV, the start's refusal, WebState.account, forget_kept_key
crates/cli/src/serve.rs, setup.rs, run.rs       modifies: the wizard's gate, CliHost, the printed account line
crates/cli/tests/{running,serving,contract_new}.rs modifies, tests: a fake `claude` that answers `auth status`
crates/cli/tests/live_claude.rs                 modifies: an adapter made without a credential
```

## Tasks

### Task 5: The account methods on the wire

Files: modified `docs/schemas/rpc.schema.json`, `crates/protocol/src/rpc.rs`, `packages/protocol-client/src/client.ts`; regenerated `packages/protocol-client/src/generated/rpc.ts`.

Tests (in `rpc.rs`'s tests, replacing those of `account.connect` at :181-224):

- `holds_the_account_methods_to_the_schema`: asserts these.
  - `account.status {}`, `account.sign_in { console: false }`, `account.sign_in_code { code: "abc" }`, `account.check {}` and `account.sign_out {}` are accepted.
  - These are rejected: `account.sign_in_code` with no code, with `""`, or with 513 characters; `account.sign_in {}`, which has no `console`; and `account.connect`.
  - `{ outcome: "working" }` is accepted as an `accountCheckResult`, and `{ outcome: "fine" }` is rejected.

- [x] `feat(protocol): sign in through claude code on the wire`

### Task 6: Sessions on Claude Code's own sign-in

Files: modified `crates/runtime/src/claude.rs`, `crates/runtime/tests/claude_process.rs`, `crates/cli/src/start.rs` (`SESSION_ENV` and `adapter()` only), `crates/cli/tests/live_claude.rs`.

Tests:

- `passes_the_base_environment_and_no_key_of_its_own` (replaces `passes_only_the_base_environment_and_one_credential`, `claude.rs:1493`): asserts that `child_env` with a base env of `PATH` and `HOME` gives exactly those two plus `QUIET`. With `ANTHROPIC_API_KEY: "from-user"` in the base, the key is there unchanged, and no `CLAUDE_CODE_OAUTH_TOKEN` is added.
- `plays_a_session_through_the_process` (`claude_process.rs:169`): its assertion on the child's environment changes from "only `ANTHROPIC_API_KEY`" to "no `ANTHROPIC_API_KEY` and no `CLAUDE_CODE_OAUTH_TOKEN`".
- `gives_a_subscription_session_no_api_key_from_its_base` (`claude_process.rs:552`) is deleted with the behaviour it held.

- [x] `refactor(runtime): sessions run on claude code's own sign-in`

### Task 7: The kept key removed, unread

Files: rewritten `crates/runtime/src/credential.rs`; modified `crates/cli/src/lib.rs`, `main.rs`, `bin/catervas-e2e-serve.rs`, `start.rs`, `serve.rs`.

Tests:

- `forgets_the_kept_key_without_reading_it` (in `credential.rs`): asserts these, with `keychain: false`.
  - A `credential.json` in a scratch state folder is removed, and the call answers `true`.
  - A second call answers `false`.
  - With the file made unreadable (mode 0o000), it is still removed. This is the proof that it is not read.
- `says_once_that_the_kept_key_was_removed` (`crates/cli/tests/running.rs`): asserts these.
  - `catervas start` with a `credential.json` in its state folder prints the removal sentence once, and the file is gone.
  - A second start does not print it.

- [ ] `feat(runtime): remove the key catervas kept, unread`

### Task 8: The daemon signs in, checks and signs out through Claude Code

Files: modified `crates/runtime/src/daemon/web.rs`, `setup.rs`, `team.rs`, `crates/cli/src/start.rs` and `serve.rs` (building `WebState.account`).

Tests (each with `FakeAccount`):

- `answers_the_account_from_claude_code` (web.rs, setup mode; replaces :3610-3634): asserts these.
  - `account.status` answers `{ signed_in: false, kind: null }`, then, after the fake signs in, `{ signed_in: true, kind: "subscription" }`.
  - `serve.status.signed_in` follows the same change.
- `never_echoes_the_sign_in_code` (replaces `never_echoes_the_secret`, :3643): asserts that a refused `account.sign_in_code` with code `secret-code-123` answers an error whose text and data hold no `secret-code-123`. The fake answers `NoSignIn`.
- `signs_in_checks_and_resumes_a_team_paused_for_its_key` (team.rs; replaces `connects_the_account_again_and_resumes_a_team_paused_for_its_key`, :5153): asserts this sequence.
  - With the team paused `credential_refused`, `account.sign_in { console: true }` answers the fake's address, and the fake recorded `start_sign_in(true)`.
  - `account.sign_in_code { code: "c" }` answers `{ outcome: "working" }`, and `team.resumed` is in the log.
  - A second test case has the fake's check answer `key_refused`: no resume, and the answer is `{ outcome: "key_refused" }`.
- `keeps_a_sign_in_whose_resume_the_human_beat`: the case of `keeps_a_connect_whose_resume_the_human_beat` (:5212), with `account.check`.
- `signs_out_and_pauses_the_team` (replaces `disconnects_the_account_pauses_and_connects_another`, :5027): asserts these.
  - With the team running, `account.sign_out` answers `{ paused: true }`, the fake recorded `sign_out`, and `team.paused` is in the log.
  - A second sign-out while paused answers `{ paused: false }`.
- `refuses_to_sign_in_over_a_key_from_the_environment` (replaces :5252): asserts these.
  - With the fake's status kind `environment` and `ANTHROPIC_API_KEY` set, `account.sign_in` is refused with the sentence above.
  - `account.sign_out` answers `{ paused: false, environment_variable: "ANTHROPIC_API_KEY" }`, and the fake recorded no `sign_out`.

- [ ] `feat(runtime): sign in, check and sign out through claude code`

### Task 9: The command line asks Claude Code

Files: modified `crates/cli/src/start.rs`, `serve.rs`, `setup.rs`, `run.rs`; `crates/cli/tests/running.rs`, `serving.rs`, `contract_new.rs`. These tests' fake `claude` (their `setup_env` and fixtures) answers `auth status --json` with `{"loggedIn": true, "authMethod": "oauth_token"}`, or `false` where a test says not signed in, in place of the key they set.

Tests:

- `refuses_to_run_when_claude_code_is_not_signed_in` (replaces `refuses_to_run_without_a_credential`, `running.rs:127`): asserts that `catervas start` with the fake not signed in exits non-zero, printing the start's refusal sentence.
- `names_the_account_and_never_a_key` (replaces `names_the_credential_it_chose_and_never_prints_it`, :160): asserts that with `ANTHROPIC_API_KEY=sk-ant-api-test` in the environment, `catervas run`'s first lines hold `account: ANTHROPIC_API_KEY from your environment`, and no output holds `sk-ant-api-test`.
- `signing_in_takes_the_waiting_project_on` (replaces `connecting_the_account_takes_the_waiting_project_on`, `serving.rs:1724`): asserts that serve on a project in setup mode, with the fake not signed in, shows the wizard. After the fake is set signed in, `account.check` answering `working` takes the project on, as connecting did.
- `says_where_the_credential_is_kept` (`run.rs:483`) becomes `says_which_account_claude_code_has`, asserting the four printed forms of the Decisions.

- [ ] `feat(cli): start only when claude code is signed in`

## Verification

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
python3 reaper.py cargo test --workspace
# expected: all pass. The web app is fixed in plan 3, so `cargo xtask check` passes only from plan 3 on.
```
