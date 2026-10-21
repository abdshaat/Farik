# Claude Code keeps the AI account, plan 1: reading Claude Code's answers, and running them

Status: ready
Branch: `docs/claude-code-signs-in`
Spec: `docs/SPEC.md` 8.6; ADR 0084 decisions 3 to 6
Depends on: `main` at `d6599bd`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-21 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

Two new modules hold everything Catervas knows about Claude Code's account:

- `catervas_core::claude_account` reads Claude Code's answers from strings: the status, the sign-in address and the check's outcome.
- `catervas_runtime::account` runs `claude auth status`, `claude auth login`, `claude auth logout` and the check, as processes with limits, and holds one sign-in at a time.

Nothing calls them yet; plan 2 wires them in. Out of scope: the daemon, the command line, every screen.

## Decisions

- **Parsing is in core, running is in runtime**, as `jobs` splits its work. Rejected: parsing in runtime, which would leave the outcome table without core's fast tests.
- **The runner takes the program's path and the session environment** (`ClaudeConfig.program` and `ClaudeConfig.env`, the same ones a session uses), so status, sign-in and check see exactly the account a session sees. Rejected: looking `claude` up on `PATH` again, which could find another install.
- **Processes**: `tokio::process::Command`, `env_clear().envs(env)`, `process_group(0)`, `kill_on_drop(true)`, and the folder as the current directory. On a limit the group is killed with `libc::killpg(pid, SIGKILL)`, as `claude.rs` ends a session's group. Standard error is read, never printed, and only the check's `failed` keeps its first 200 characters.
- **Limits**: status 15 seconds; logout 15 seconds; the check 60 seconds; a login's first address 20 seconds; a login without a code is ended at 10 minutes; a login after its code 60 seconds.
- **The login's state** is `Mutex<Option<Login>>` in `ClaudeAccount`, holding the child, its standard input and the time it started. `start_sign_in` replaces and kills any earlier one, and so does a call on an expired one. The code is passed as `&str`, written, and not stored in any field.
- **The check's prompt and flags** are a `const` array in core (`CHECK_ARGS`), so the test asserts them once and the runner cannot drift from ADR 0084.

## File map

```
crates/core/src/claude_account.rs        creates: AccountKind, AccountStatus, CheckOutcome, CHECK_ARGS, parse_status, sign_in_address, check_outcome
crates/core/src/lib.rs                   modifies: one `pub mod claude_account;` line with its doc comment
crates/runtime/src/account.rs            creates: ClaudeAccount (status, check, start_sign_in, finish_sign_in, sign_out), SignInError
crates/runtime/src/lib.rs                modifies: one `pub mod account;` line with its doc comment
crates/runtime/tests/account_process.rs  tests: the runner against a fake `claude` script
```

## Interfaces

Consumes: `ClaudeConfig` (`catervas-runtime`, `claude.rs:185`), `credential_variable` (`claude.rs:145`, kept by plan 2).

Produces:

```rust
// catervas_core::claude_account
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind { Subscription, Console, Environment, Other }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus { pub signed_in: bool, pub kind: Option<AccountKind> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome { Working, KeyRefused, LimitReached, NotSignedIn, Failed(String) }
pub const CHECK_ARGS: [&str; 15];
pub fn parse_status(stdout: &str) -> Option<AccountStatus>;
pub fn sign_in_address(line: &str) -> Option<String>;
pub fn check_outcome(stdout: &str) -> Option<CheckOutcome>;
impl CheckOutcome { pub fn code(&self) -> &'static str; } // "working", "key_refused", "limit_reached", "not_signed_in", "failed"

// catervas_runtime::account
pub struct ClaudeAccount { /* program, env, check_dir, login */ }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInError { NoSignIn, Failed(String), NoAddress(String) }
impl ClaudeAccount {
    pub fn new(program: PathBuf, env: BTreeMap<String, String>, check_dir: PathBuf) -> ClaudeAccount;
    pub async fn status(&self) -> AccountStatus;            // a failed run is { signed_in: false, kind: None }
    pub async fn check(&self) -> CheckOutcome;
    pub async fn start_sign_in(&self, console: bool) -> Result<String, SignInError>;
    pub async fn finish_sign_in(&self, code: &str) -> Result<CheckOutcome, SignInError>;
    pub async fn sign_out(&self) -> Result<(), String>;
}
```

`status()` gives `kind: Some(Environment)` when `credential_variable(env)` names a variable, whatever `authMethod` says.

The daemon holds the runner as a port, so its tests use a fake. This is the pattern of `sleep.rs:38`:

```rust
type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub trait AccountPort: Send + Sync {
    fn status(&self) -> Boxed<'_, AccountStatus>;
    fn check(&self) -> Boxed<'_, CheckOutcome>;
    fn start_sign_in(&self, console: bool) -> Boxed<'_, Result<String, SignInError>>;
    fn finish_sign_in<'a>(&'a self, code: &'a str) -> Boxed<'a, Result<CheckOutcome, SignInError>>;
    fn sign_out(&self) -> Boxed<'_, Result<(), String>>;
}
impl AccountPort for ClaudeAccount { /* each boxes the inherent method */ }
#[cfg(test)] pub(crate) mod fixtures {
    // FakeAccount: answers set by the test (status, check outcome, the sign-in
    // address, finish's answer), and records each call by name, the code included
    pub(crate) struct FakeAccount { /* Mutex fields */ }
}
```

`FakeAccount` is made `pub` behind the `fixtures` feature only if a test outside the crate needs it. Plan 2's tests are all inside `catervas-runtime` or use a fake `claude` script.

## Tasks

### Task 1: Claude Code's status and the sign-in address, read from strings

Files: created `crates/core/src/claude_account.rs`; modified `crates/core/src/lib.rs`.
Produces: `AccountKind`, `AccountStatus`, `parse_status`, `sign_in_address`.

`parse_status` reads a JSON object and takes `loggedIn` (a bool, else `None`) and `authMethod`, mapped as the README's table. A missing `authMethod` with `loggedIn: true` is `Other`; with `loggedIn: false` the kind is `None`.

`sign_in_address` finds the first `https://` in the line and takes the address up to the next whitespace. It keeps it only when its host is one of the four in the README, and holds it to at most 2048 characters.

Tests:

- `reads_whether_claude_code_is_signed_in`: asserts these.
  - The 2.1.306 answer recorded in ADR 0084 (`"loggedIn": true, "authMethod": "oauth_token"`) is signed in, `Subscription`.
  - `"authMethod": "api_key"` is `Console`; `"apiKey"` is also `Console`.
  - `"loggedIn": false` is not signed in, kind `None`.
  - Text that is not JSON is `None`.
- `takes_the_sign_in_address_only_from_anthropic`: asserts these.
  - The ADR's line "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=x&state=y" gives the address from `https` to `state=y`.
  - `https://console.anthropic.com/oauth/authorize?x` is kept.
  - `https://evil.example/claude.com` gives `None`, as do `https://claude.com.evil.example/` and `http://claude.com/`.
  - A line with no address gives `None`.

- [x] `feat(core): read claude code's sign-in status and address`

### Task 2: The check's outcome, read from strings

Files: modified `crates/core/src/claude_account.rs`.
Produces: `CheckOutcome`, `CheckOutcome::code`, `CHECK_ARGS`, `check_outcome`.

`CHECK_ARGS` holds, in order:

```
-p, Reply with the single word OK., --model, haiku, --max-turns, 1, --tools, "", --system-prompt, Reply briefly., --strict-mcp-config, --setting-sources, "", --output-format, json
```

The two `""` entries are empty strings.

`check_outcome` maps the README's table; `NotSignedIn` is never its answer, since the runner decides that from the status first. `Failed` carries `result`, else "Claude Code gave no answer".

Tests:

- `reads_the_outcome_of_a_check`: asserts these.
  - ADR 0084's good answer is `Working`.
  - Its 401 answer is `KeyRefused`; 403 is also `KeyRefused`.
  - 429 is `LimitReached`, and so is `"result": "Claude AI usage limit reached|1760000000"` with `is_error: true`.
  - `is_error: true`, `api_error_status: 500`, `"result": "Overloaded"` is `Failed("Overloaded")`.
  - Text that is not JSON is `None`.
  - `CHECK_ARGS` equals, element by element, the 15 arguments listed above.

- [x] `feat(core): read the outcome of a connection check`

### Task 3: Status, check and sign-out, run as processes

Files: created `crates/runtime/src/account.rs`; modified `crates/runtime/src/lib.rs`; tested by `crates/runtime/tests/account_process.rs`.
Produces: `ClaudeAccount::new`, `status`, `check`, `sign_out`.
Consumes: Tasks 1 and 2.

The fake `claude` is a shell script written by the test into a scratch folder:

- `auth status` prints the JSON from a file the test sets, or sleeps for `slow`.
- `-p` appends its arguments to `args.txt`, prints a JSON answer from another file, and exits 0, or 1 when the answer has `is_error: true`.
- `auth logout` touches `logged_out`.

The test builds its environment with `PATH` only. These tests are `#[ignore = "needs a shell: cargo xtask check --integration"]` like the other process tests, and need the child reaper in this container.

`check()` first asks the status and answers `NotSignedIn` without running `-p` when it is not signed in. It makes `check_dir` if it is missing, and runs `CHECK_ARGS` there.

Tests:

- `asks_claude_code_and_checks_with_one_tiny_request`: asserts these.
  - A signed-in fake answers `Working`. `args.txt` holds exactly `CHECK_ARGS`, and the run's current directory was `check_dir`.
  - With the status `loggedIn: false`, `check()` answers `NotSignedIn` and `args.txt` was never written.
  - A 401 answer is `KeyRefused`.
  - A status that sleeps 30 seconds gives `{ signed_in: false, kind: None }` within 20 seconds.
  - With `ANTHROPIC_API_KEY` set to `x` in the env given to `new`, `status()` has kind `Environment`.
  - `sign_out()` touched `logged_out`.

- [x] `feat(runtime): ask claude code its account, check it, and sign it out`

### Task 4: Signing in through Claude Code's login

Files: modified `crates/runtime/src/account.rs`; tested by `crates/runtime/tests/account_process.rs`.
Produces: `start_sign_in`, `finish_sign_in`, `SignInError`, `AccountPort` and its `impl` for `ClaudeAccount`, `fixtures::FakeAccount`.
Consumes: Task 3.

`start_sign_in(console)` runs `auth login` with `--console` when `console` is true, else `--claudeai`, with standard input piped. It reads standard output line by line until `sign_in_address` gives an address or the 20-second limit passes. Past the limit it kills the group and answers `NoAddress` with the last line read.

`finish_sign_in(code)` does the following:

1. With no live login, or one past 10 minutes, it answers `NoSignIn`. An expired login is killed first.
2. It writes `code` and `\n`, closes standard input and waits up to 60 seconds.
3. Exit 0 runs `check()` and answers its outcome. Any other exit, or the limit, is `Failed` with the last line of output.

The fake's `auth login` prints the two lines recorded in ADR 0084 (the address with `claude.com`). It then reads one line and writes it to `code.txt`. It exits 0 when the line is `good-code`, else prints "Invalid code" and exits 1.

Tests:

- `signs_in_through_claude_code_and_checks_the_account`: asserts these.
  - `start_sign_in(false)` answers the address, and the fake saw `--claudeai`. `start_sign_in(true)` saw `--console`.
  - `finish_sign_in("good-code")` answers `Working`, and `code.txt` holds `good-code`.
  - A second `finish_sign_in` without a new start answers `NoSignIn`.
  - `finish_sign_in("bad")` after a new start answers `Failed("Invalid code")`.
  - A start while another is waiting kills the first, whose process group is gone within a second.
  - A fake that prints no address answers `NoAddress` within 25 seconds.

- [ ] `feat(runtime): sign in through claude code's own login`

## Verification

```
cargo test -p catervas-core claude_account
python3 reaper.py cargo test -p catervas-runtime --test account_process -- --include-ignored
# expected: all pass
```
