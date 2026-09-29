# Phase 6, step 05: Project, computer, and account

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1 (first run), 8.3 (sandbox), 8.6 (credential), F2
Depends on: steps 01 to 04 of this phase (step 04 landed at the commit its landing review records)
Readiness confirmed by: fresh-session reviewer, 2026-09-29, round one: not ready, with three planner decisions and one founder decision unmade. All four are made below. Round two, limited to them, found all four settled (ready with findings, which are folded in).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A user runs `farik serve` in any folder. With no project there, and no remembered one, the browser opens the first-run wizard's first three screens:
1. **"Checking your computer".** Farik looks for Claude Code, git, Docker, and its sandbox image, which it can build on request. The user can continue without Docker after a warning.
2. **"Connect your AI account".** The user pastes a Claude subscription token or an API key. Farik keeps it in the keychain, or in a private file on a computer without one (ADR 0022).
3. **"Your project".** The user picks an existing git folder from a folder browser of their home, or describes a new project, which Farik creates with `git init`.

When a project is chosen, `farik serve` takes it on without a restart, the browser reconnects by itself, and the team stays paused until setup finishes. Step 06's "Start the team" resumes it.

Out of scope:
- the rest of the wizard (step 06);
- disconnecting the account (step 06);
- providers and engines other than Claude, which come in the new phase 7 (ADR 0023).

## Decisions

- **The founder's approval** (2026-09-29). The founder approved the three screens and the wizard's new order ("generally good"): 1 Your computer, 2 Your AI account, 3 Your project, 4 What we found, 5 Your team, 6 What they may do, 7 Spending, 8 Finishing work. The mockup is "Farik first-run screens". Computer and account come first because neither needs a project, and taking a project on starts the team's driver, which needs the key. Task 6 changes spec 4.1's order, and adds an amendment line to ADR 0021.
- **One provider now, the provider recorded.** The founder also asked for other providers and engines, which the new phase 7 designs (ADR 0023). This step's credential records its provider: `{ "provider": "anthropic", "kind": …, "secret": … }`. The account screen names Claude alone, and phase 7 adds choices without migrating anything.
- **The setup host** (round one, decision 1). `farik-runtime` cannot call the CLI's `init`, `state_dir`, or request filing, because the CLI depends on runtime and not the other way round. So:
  - runtime defines `trait SetupHost: Send + Sync` with `open`, `create`, and `connect` (signatures below) and `enum SetupError { Refused(String), Failed(String) }`;
  - `DaemonState::setup(host: Arc<dyn SetupHost>, web: WebState) -> DaemonState` builds a setup-mode state, with `deps` absent;
  - the CLI implements `SetupHost` in `crates/cli/src/setup.rs`;
  - a chosen project is reported through a `tokio::sync::watch::Sender<Option<PathBuf>>` that the host holds, and `serve`'s setup loop awaits the receiver;
  - runtime's tests use a fake host;
  - the assertions about init, the pause, and the first request live in the CLI's tests.
- **Setup mode.** `farik serve` looks for a project in three places:
  1. the working directory's project;
  2. else `state.json`'s `last_project`, when it is still a Farik project;
  3. else setup mode.

  In setup mode:
  - `DaemonState.deps` is `Option<Arc<ToolDeps>>`: `DaemonState::new` keeps its signature and wraps `Some`, and `setup` sets `None`.
  - The hook, MCP, and `/command` routes answer 503 with "farik has no project yet".
  - `serve.status` answers `project_root: null`, `paused: false`, and `credential` read afresh from `load_credential` on each query, not from the `OnceLock`.
  - Every other query answers `-32004 no_project`.
  - `WebState` gains `clock: Arc<dyn Clock>`.
  - The setup daemon writes no `daemon.json`: `DaemonConfig.daemon_file` becomes `Option<PathBuf>`, and it is `None` in setup mode.
  - Ctrl-C exits 130.
- **Taking a project on** (round one, decision 3).
  1. `open` and `create` check, before they change anything, that `try_lock` on the chosen root would succeed and that `load_credential` finds a credential.
  2. Each refuses with a sentence when either check fails: "another farik is already running this project", or "connect your AI account first".
  3. They then do their work, answer the request, and send the root on the watch.
  4. The setup loop shuts the setup daemon down and starts the driver: `start_holding` with `StartOptions { port: PortChoice::Exact(<the same port>), web: true }`, and the interrupt receiver created once by the loop and passed in, so Ctrl-C still works after the take-on.
  5. `remember` records `last_project` only after the driver has started, as `writes_no_state_before_the_driver_starts` already requires.
  6. If the start fails, the loop goes back to setup mode on the same port, and `serve.status` carries `take_on_error: string | null`, which the page shows.
  7. `PortChoice::Exact(p)` binds `p` or fails. tokio sets `SO_REUSEADDR` on Unix, so TIME_WAIT does not block the bind. If the bind still fails, serve prints and opens a new link, because the cookie does not depend on the port.
- **The page across a take-on.** `connection.tsx` gains the status `reopening`. The setup pages set it when `open` or `create` answers. A close during `reopening` retries `/session` at once and then every 500 ms for up to 30 s, without showing the Connect page. The page shows "Opening your project…" and goes to `/` when the socket is open again.
- **The team starts paused.** When setup ran `farik init` in a folder, the host appends `team.paused { by: human }` before it answers. Spec 4.1 says "Nothing runs until the two permissions are set". Opening a folder that already had a Farik project leaves its pause state alone.
- **No Docker** (round one, decision 2). `project.open` and `project.create` take `no_sandbox: boolean`. When it is true, the host writes `.farik/local/settings.json` `{"sandbox":"none"}` into the project, and the driver prints `NO_SANDBOX_WARNING` as it starts (spec 8.3). Nothing about the sandbox goes into `state.json`.
- **The wire** (`rpc.schema.json`).
  - Queries:
    - `folders.list { path?: string }`, answering `{ path, parent: string | null, entries: [{ name, git: boolean }] }`;
    - `computer.check {}`, answering `{ claude, git, docker, sandbox_image }`, each `{ state: 'ready' | 'missing' | 'too_old' | 'not_running', version?: string }`;
    - `account.status {}`, answering `{ provider: 'anthropic' | null, kind: 'api_key' | 'subscription_token' | null, source: 'environment' | 'keychain' | 'file' | null }`.
  - Methods:
    - `project.open { path, no_sandbox }` and `project.create { parent, name, description, no_sandbox }`, each answering `{ project_root }`;
    - `account.connect { kind, secret }`, answering `{ stored_in: 'keychain' | 'file' }`;
    - `sandbox.build {}`, answering `{ image }`.
  - Errors: `-32004 no_project`, and `-32005 refused` with the sentence as its `message`.
  - A refused `account.connect` frame never echoes its params. `answer` drops `error.data` for every method whose params hold a secret, which is `account.connect`, because `rpc_request_from_value`'s messages quote the whole frame.
  - `@farik/protocol-client` gains `call(method, params)`.
- **Folders.** `path` is relative to the user's home (`HOME` from `CliIo.env`). The listing:
  - holds sub-folders only, and leaves out names starting with `.`;
  - canonicalizes home and each folder, and leaves out any folder outside home, comparing with the existing `same_directory` so that `/var` versus `/private/var` on macOS agrees;
  - sorts case-insensitively and caps at 500 entries (a `ponytail:` note);
  - sets `git` when the folder has `.git`.

  A path outside home is refused with "that folder is outside your home folder".
- **`open`.** The root must be a repository root inside home. It is refused otherwise with "that folder is not a git project; choose another, or start a new project", or "that folder is inside a git project; choose <root> instead". It runs `init` when `.farik/team.yaml` is missing. `init` is safe to run again, since it keeps an existing team.
- **`create`.**
  - `name` is lowercase letters, digits, and `-`, 1 to 64 characters.
  - `description` is 20 to 2000 characters, which is the contract's `intent` minimum, checked before any folder is made. The refusal is "say a little more about the project: at least 20 characters".
  - `parent/name` must not exist, or the refusal is "a folder with that name is already there".
  - It runs `git -c user.name=farik -c user.email=farik@localhost -c commit.gpgsign=false init -b main`, then commits a `README.md` of `# <name>` plus the description, with `git` resolved from `CliIo.env`'s `PATH`. A missing git is refused with "git is not installed".
  - It then runs `init` and the pause, and files the description as the first request through the functions `contract new --brief` uses: `request_from_brief`, made `pub(crate)` for `setup.rs`, then `file_request`. The title is the project's name.
- **Computer.**
  - `claude`: found through `on_path`, then `claude --version` and `check_version(MIN_CLAUDE_VERSION)`.
  - `git`: `git --version`.
  - `docker`: `docker version`. A `docker` binary whose daemon does not answer is `not_running`.
  - `sandbox_image`: `docker image inspect SANDBOX_IMAGE`, reported `missing` when Docker is not ready.
  - Each check has a 10-second timeout, and a timeout reads as `missing`.
  - `sandbox.build` runs `docker build -t SANDBOX_IMAGE -` with the Dockerfile embedded by `include_str!`. The Dockerfile has no `COPY`, so an empty context works.
- **The credential** (ADR 0022).
  - `farik_runtime::credential::{CredentialStore, KeychainStore, FileStore, MemoryStore, Source, CredentialError, load_credential, save_credential}`.
  - Load order: environment, keychain, file.
  - `KeychainStore` uses `keyring` `=4.2.0` with its default features, service `farik`, account `anthropic`, and the JSON above as the password. On load, `NoEntry` is `Ok(None)`.
  - `CredentialError { NoKeychain, Failed(String) }`. `NoKeychain` comes from `NoDefaultStore` or a platform failure whose D-Bus error is `ServiceUnknown`, and only it falls back to the file. A locked or denied keychain, or any other failure, is refused as "your computer's keychain would not store the key: <why>".
  - `FileStore` writes `<state_dir>/credential.json`, mode 0600, through `write_private`.
  - The key's prefix is checked: `sk-ant-oat` for a subscription token, `sk-ant-api` for an API key, each with a refusal sentence.
  - `start_holding` reads the credential through `load_credential`, and `run`'s credential line names where it came from.
  - `CliIo` gains `credential_stores: Arc<dyn Fn() -> Vec<Arc<dyn CredentialStore>>>`, which `main.rs` sets to keychain then file, and which `CliIo::new` sets to one `MemoryStore`, so no test touches a real keychain.
  - `farik-e2e-serve --no-keychain` uses the file store alone.
- **Web pages** (Task 5): `SetupComputer`, `SetupAccount`, and `SetupProject` (with `FolderBrowser` and the new-project form) at `/setup/computer`, `/setup/account`, and `/setup/project`. `/` goes to `/setup/computer` while `project_root` is null. Each page has the eight-step `Stepper`. All text is in `en.ts`. A `take_on_error` shows on `/setup/project`.
- **Tests.** The Playwright journey `setup-project.spec.ts` uses `startServe({ transcripts: [], project: false, home })`: a temporary `HOME` holding one git project, a fake-bin folder first on `PATH` with a `claude` that prints `2.1.300 (Claude Code)` and a `docker` that exits 1, both `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN` unset, and `--no-keychain`. It walks the three screens: continue without Docker; paste `sk-ant-oat01-test` (stored in the file); choose the project. Then it asserts the reconnect, `project_root` set, and the team paused.

## File map

```
docs/SPEC.md (4.1, 8.3, 8.6), docs/decisions/0021-…md (amendment line)   modifies (T6)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs                  modifies (T1)
packages/protocol-client/src/{client.ts,client.test.ts,generated/rpc.ts}  modifies: call() (T1)
crates/runtime/src/{credential.rs,lib.rs}, Cargo.toml, crates/runtime/Cargo.toml, Cargo.lock  creates / modifies: keyring (T2)
crates/runtime/src/{daemon.rs,daemon/web.rs,daemon/setup.rs,computer.rs,lib.rs}  modifies / creates: setup state, host trait, queries, methods, checks (T3)
crates/cli/src/{setup.rs,serve.rs,start.rs,run.rs,lib.rs,main.rs,contract_new.rs}, src/bin/farik-e2e-serve.rs, tests/serving.rs  creates / modifies (T4)
apps/web/src/app/{App.tsx,connection.tsx}, pages/setup/*.tsx (+ css, tests), strings/en.ts  creates / modifies (T5)
apps/web/e2e/{setup-project.spec.ts,fixtures/serve.ts,fixtures/fake-bin/claude,fixtures/fake-bin/docker}  creates / modifies (T5)
docs/plans/project-plan.md (step 05 line)                                 modifies (T6)
```

## Interfaces

```rust
pub enum Source { Environment, Keychain, File }
pub enum CredentialError { NoKeychain, Failed(String) }
pub trait CredentialStore: Send + Sync { fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError>; fn save(&self, c: &ClaudeCredential) -> Result<(), CredentialError>; fn source(&self) -> Source; }
pub fn load_credential(env: &BTreeMap<String, String>, stores: &[Arc<dyn CredentialStore>]) -> Option<(ClaudeCredential, Source)>;
pub fn save_credential(c: &ClaudeCredential, stores: &[Arc<dyn CredentialStore>]) -> Result<Source, CredentialError>;
pub enum SetupError { Refused(String), Failed(String) }
pub trait SetupHost: Send + Sync {
    fn open(&self, path: &str, no_sandbox: bool) -> Result<PathBuf, SetupError>;
    fn create(&self, parent: &str, name: &str, description: &str, no_sandbox: bool) -> Result<PathBuf, SetupError>;
    fn connect(&self, kind: CredentialKind, secret: &str) -> Result<Source, SetupError>;
    fn home(&self) -> PathBuf;
    fn env(&self) -> BTreeMap<String, String>;                 // for computer.check in setup mode
    fn account(&self) -> Option<(CredentialKind, Source)>;     // for account.status and serve.status.credential
}
DaemonState::setup(host: Arc<dyn SetupHost>, web: WebState) -> DaemonState;  DaemonConfig.daemon_file: Option<PathBuf>;
pub enum PortChoice { Any, Preferred(u16), Exact(u16) }
pub fn check_computer(env: &BTreeMap<String, String>) -> ComputerCheck;  pub async fn build_sandbox_image() -> Result<String, String>;
```

```ts
DaemonClient.call(method: 'project.open' | 'project.create' | 'account.connect' | 'sandbox.build', params: object): Promise<unknown>;
type Status = 'checking' | 'no_session' | 'connecting' | 'open' | 'lost' | 'reopening';
```

## Tasks

### Task 1: The wire for setup

- `reads_the_setup_queries_and_methods` asserts that each new query and method validates with its params, and that each refuses when a required field is missing.
- `client.call sends a method and resolves its result`.

- [x] `feat(protocol): add the setup queries and methods to the browser's wire`

### Task 2: The credential stores

- `reads_the_environment_before_the_stores` asserts that with `ANTHROPIC_API_KEY` set and a store holding a token, the result is the key, from `Environment`.
- `falls_back_to_the_file_only_without_a_keychain` asserts two things. A keychain store answering `NoKeychain` makes `save_credential` write the file, mode 0600, holding `{ provider, kind, secret }`, and answer `File`. A keychain store answering `Failed("locked")` makes it refuse with the keychain sentence and write no file.
- `treats_no_entry_as_nothing_stored` asserts that `load` on an empty store answers `Ok(None)`, and `load_credential` then tries the next store.
- `refuses_a_key_of_the_wrong_kind` asserts the prefix refusals for both kinds.

- [ ] `feat(runtime): keep the model credential in the keychain or a private file`

### Task 3: The setup daemon

Route tests use `DaemonState::setup` with a fake `SetupHost`.

- `lists_only_folders_inside_home` asserts that a temporary home containing `a/` (git), `b/`, `.hidden/`, `f.txt`, and a symbolic link `out` pointing outside home answers exactly `a` (git) and `b`, and that `path: "../"` is refused.
- `passes_open_and_create_to_the_host_and_refuses_in_words` asserts that the host's calls receive the params, and that a `Refused` from the host answers `-32005` with its sentence.
- `answers_the_computer_check` asserts, with fake `claude`, `git`, and `docker` on `PATH` (docker's `image inspect` exits 1), `claude ready 2.1.300`, `git ready`, `docker ready`, and `sandbox_image missing`. An old `claude` gives `too_old`.
- `answers_503_and_no_project_in_setup_mode` asserts `/command` answers 503, `tasks.list` answers `-32004`, and `serve.status` answers `project_root: null`, `take_on_error: null`.
- `never_echoes_the_secret` asserts that neither a valid `account.connect` nor a malformed one (a missing `kind`) returns a reply frame containing the secret.

- [ ] `feat(runtime): answer the setup wizard's calls through a setup host`

### Task 4: `farik serve` without a project, and taking one on

Tests go in `crates/cli/tests/serving.rs`, with memory credential stores.

- `serves_setup_outside_a_project` asserts that `serve` in an empty folder prints the link, `serve.status` answers `project_root: null`, and no `daemon.json` is written.
- `reopens_the_last_project` asserts that with `last_project` recorded, `serve` from an empty folder serves that project.
- `creates_a_project_paused_with_its_first_request` asserts that `project.create` over the socket makes `bakery/` with a `.git`, a README holding the description, `.farik/team.yaml`, a `team.paused` event, and a filed request whose intent is the description. It also asserts that a 17-character description is refused before any folder exists.
- `takes_on_the_chosen_project_on_the_same_port` asserts, after `project.open`, that the socket closes; a new socket with the same cookie reconnects on the same port; `project_root` is set and the team is paused; no second link is printed; and `state.json` names the project.
- `stays_in_setup_when_the_project_is_busy` asserts that `project.open` on a project whose run lock another process holds is refused with its sentence, and that `state.json` is unchanged.
- `goes_back_to_setup_when_the_driver_cannot_start` asserts that when the driver's start fails after the answer (the project's `.farik/prices.json` made unreadable), serve returns to setup mode on the same port and `serve.status.take_on_error` holds the reason.
- `keeps_ctrl_c_after_the_take_on` asserts that after a take-on, one interrupt ends serve with 130.
- `writes_no_sandbox_into_the_project` asserts that `project.open { no_sandbox: true }` leaves `{"sandbox":"none"}` in the project's `settings.json`, and the warning on stderr.

- [ ] `feat(cli): serve the setup wizard outside a project and take the chosen one on`

### Task 5: The three screens

- `checks_the_computer_and_offers_the_fixes` asserts that with Docker missing, the row shows "Not found" with its fix text, Continue is disabled, "Continue without Docker" moves on with `no_sandbox` kept, and a missing image shows "Prepare it", which calls `sandbox.build` with a busy button.
- `connects_a_subscription_key` asserts that the call is `account.connect` with `subscription_token`, that the page says where the key was stored, that it moves to `/setup/project`, and that a refusal shows under the field.
- `browses_folders_and_uses_one` asserts that the page lists the entries, marks those that are not git projects, opens a folder, and calls `project.open` with the relative path and `no_sandbox`.
- `starts_a_new_project` asserts that `project.create` is called with the slugged name, and that the status becomes `reopening` while the page shows "Opening your project…".
- `sends_the_user_to_setup_without_a_project` asserts that `/` redirects to `/setup/computer`, and to `/setup/project` when `take_on_error` is set, where the error shows.
- `setup-project.spec.ts` is the Playwright journey from the Tests decision.

- [ ] `feat(web): add the computer, account and project setup screens`

### Task 6: Spec and plan

- Spec 4.1: the new order, setup mode, the team paused until setup ends, the image build, and the description filed as the first request.
- Spec 8.3: the no-sandbox choice made in setup.
- Spec 8.6: ADR 0022's wording, and that a reply never echoes a secret.
- ADR 0021 gets an amendment line for the order.
- Project plan: step 05's interface line and its row in the table.

- [ ] `docs(spec): describe the first run's computer, account and project screens`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/protocol-client "Tests  7 passed (7)"; @farik/web "Tests  20 passed (20)";
#   playwright "3 passed"; last line: xtask check: ok  (T4 has 8 tests)
```
