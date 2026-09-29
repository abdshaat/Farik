# Phase 6, step 05: Project, computer, and account

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1 (first run), 8.3 (sandbox), 8.6 (credential), F2
Depends on: steps 01 to 04 of this phase
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A user runs `farik serve` in any folder. With no project there, and no remembered one, the browser opens on the first-run wizard's first three screens:
1. **"Checking your computer".** Farik checks for Claude Code, git, Docker, and Farik's sandbox image. It builds the image when the user asks, or lets the user continue without Docker after a warning.
2. **"Connect your AI account".** The user pastes a subscription token or an API key. Farik keeps it in the keychain, or, on a computer without one, in a private file (ADR 0022).
3. **"Your project".** The user picks an existing git folder from a folder browser of their home folder, or describes a new project, which Farik creates with `git init`.

Once a project is chosen, `farik serve` takes it on without the user restarting anything. The browser reconnects by itself, and the team is paused until setup finishes (step 06's "Start the team").

Out of scope: the rest of the wizard (step 06), disconnecting the account (step 06), and every screen after setup.

## Decisions

- **The founder's screens.** The three screens and the wizard's new order (1 Your computer, 2 Your AI account, 3 Your project, 4 What we found, 5 Your team, 6 What they may do, 7 Spending, 8 Finishing work) are the mockup published on 2026-09-29, "Farik first-run screens". Task 5, which builds the pages, starts only once the founder's approval is recorded here. Tasks 1 to 4 do not depend on it.
- **Setup mode.** `farik serve` looks for a project in three places, in order:
  1. the working directory's project;
  2. else `state.json`'s `last_project`, if it is still a Farik project;
  3. else setup mode.

  Setup mode runs the daemon with no project. `DaemonState.deps` becomes `Option<Arc<ToolDeps>>`, `None` in setup mode. With no project:
  - the hook, MCP, and `/command` routes answer 503 with "farik has no project yet";
  - `serve.status` answers `project_root: null`, `paused: false`, `credential` from the account;
  - every other query answers `-32004 no_project`.

  No orchestrator runs. `WebState` gains its own `clock`, because the browser routes used `deps.clock`.
- **Taking a project on.** When `project.open` or `project.create` succeeds, the setup loop does four things:
  1. answers the request;
  2. records `last_project`;
  3. shuts the setup daemon down;
  4. starts the ordinary driver (`start_holding` with `StartOptions { port: Preferred(<the same port>), web: true }`) in the same process.

  The page's socket closes, and the page reconnects through `/session` with its stored browser session (step 04's reconnect). No new link is printed or opened, because the browser already has a session. `serve`'s loop is `loop { match mode { Setup => …, Drive(project) => … } }`. Rejected: making the running daemon's project swappable, which would mean a lock on every tool call for a switch that happens once.
- **The team starts paused.** When setup writes `.farik/` into a folder, meaning `farik init` ran there, it appends `team.paused { by: human }` before the driver starts. Step 06's "Start the team" resumes it. Spec 4.1 says "Nothing runs until the two permissions are set". Opening a folder that already had a Farik project does not pause it.
- **Methods and queries** (`rpc.schema.json`).
  - Three new queries, which only read:
    - `folders.list { path?: string }` answers `{ path, parent: string | null, entries: [{ name, git: boolean }] }`.
    - `computer.check {}` answers `{ claude: Check, git: Check, docker: Check, sandbox_image: Check }`, where `Check = { state: 'ready' | 'missing' | 'too_old' | 'not_running', version?: string }`.
    - `account.status {}` answers `{ kind: 'api_key' | 'subscription_token' | null, source: 'environment' | 'keychain' | 'file' | null }`.
  - Four new methods, which write, each a new request `$def` in `rpcRequest.oneOf`, beside `subscribe`, `command`, and `query`:
    - `project.open { path }` answers `{ project_root }`.
    - `project.create { parent, name, description }` answers `{ project_root }`.
    - `account.connect { kind, secret }` answers `{ stored_in: 'keychain' | 'file' }`.
    - `sandbox.build {}` answers `{ image }` when the build ends.
  - Their errors:
    - `-32005 refused`, whose `message` is the plain sentence;
    - `-32004 no_project`.
  - `@farik/protocol-client` gains `call(method, params): Promise<unknown>` for the new methods.
- **Folders** (the founder, 2026-09-28, in-app browser).
  - `path` is relative to the user's home (from `CliIo.env`'s `HOME`); `""` is home itself.
  - The listing is sub-folders only, never files. Names starting with `.` are left out, and so is any folder whose canonical path leaves home (symbolic links out are not followed).
  - It is sorted case-insensitively, capped at 500 entries (a `ponytail:` note), and `git` is true when the folder has a `.git`.
  - A `path` with `..` components, or that resolves outside home, is refused with "that folder is outside your home folder".
- **`project.open`.** The path must be a git repository's root, which `repository_root` gives, and inside home. Refusals:
  - "that folder is not a git project; choose another, or start a new project";
  - "that folder is inside a git project; choose <root> instead".

  When `.farik/team.yaml` is missing it runs `farik init` as a library call (`farik::init::init(root, now)`, which already starts over cleanly when run again), then the pause. Otherwise it opens the project as it is.
- **`project.create`.**
  - `name` is lowercase letters, digits, and `-`, 1 to 64 characters.
  - `parent` is a home-relative folder.
  - The folder `parent/name` must not exist; if it does, the refusal is "a folder with that name is already there".
  - It creates the folder, runs `git init -b main` and a first commit of a `README.md` whose text is `# <name>` and the description, with Farik's author (`farik <farik@localhost>`) because the repository has no user yet, then runs `init` and the pause.
  - It files the description as the first request, through the store's request filing that `farik task create` uses, so the Product Manager starts from it once the team is resumed.
- **Computer.**
  - `claude`: `on_path("claude")`, `claude --version`, and `check_version` against `MIN_CLAUDE_VERSION`.
  - `git`: `git --version`.
  - `docker`: `docker version` answers `ready`; a `docker` on `PATH` that cannot reach its daemon is `not_running`.
  - `sandbox_image`: `docker image inspect farik/sandbox:<version>`.

  Each check has a 10-second timeout, and a timeout reads as `missing`. The web page shows the image as part of Docker's row, with "Prepare it" calling `sandbox.build`. That runs `docker build -t farik/sandbox:<version> -` on the Dockerfile embedded with `include_str!` from `crates/runtime/sandbox/Dockerfile`, and takes minutes. It is an ordinary method call the page awaits, with a busy button.
- **Continuing without Docker.** Before a project exists the choice is remembered in `state.json` as `"sandbox": "none"`. It is written into the project's `.farik/local/settings.json` when the project is taken on, with `NO_SANDBOX_WARNING` printed, as spec 8.3 requires.
- **The credential** (ADR 0022).
  - `farik_runtime::credential::{CredentialStore, KeychainStore, FileStore, MemoryStore, load_credential(env, stores) -> Option<(ClaudeCredential, Source)>}` reads in the order environment, keychain, file.
  - `KeychainStore` uses `keyring` `=4.2.0` with its default features, service `farik`, account `anthropic`, and stores `{ "kind": …, "secret": … }` as the password. Any error from the keychain is treated as no keychain, and the file is used.
  - `FileStore` writes `<state_dir>/credential.json` with mode 0600, through `write_private`.
  - `account.connect` checks the secret's prefix: `sk-ant-oat` for a subscription token, `sk-ant-api` for an API key. The refusal is "that does not look like a Claude subscription key; it starts with sk-ant-oat", or the API wording for a key.
  - `start_holding` takes its credential from `load_credential`, and `run`'s credential line names the source.
  - The secret never appears in a log, an event, or a reply.
- **Web pages** (Task 5): `SetupComputer`, `SetupAccount`, and `SetupProject` (with a `FolderBrowser` dialog and the new-project form), under `/setup/computer`, `/setup/account`, and `/setup/project`. `/` goes to `/setup/computer` while `serve.status.project_root` is null. Each page uses `@farik/ui`'s `Stepper` with the eight steps. Every string lives in `en.ts`. After `project.open` or `project.create` answers, the page shows "Opening your project…" until the reconnect finishes, then goes to `/`. Step 06 adds the next screens.
- **Tests.** The Rust unit and route tests follow step 04's. The Playwright journey `setup-project.spec.ts`:
  1. starts `farik-e2e-serve` in an empty folder with `HOME` set to a temporary home that holds one git project;
  2. passes `CLAUDE_CODE_OAUTH_TOKEN`-free env and a fake `claude` script on `PATH` that prints `2.1.300 (Claude Code)`;
  3. walks the three screens: continue without Docker, paste `sk-ant-oat01-test` (stored in the file, since CI has no keychain), then choose the folder;
  4. asserts that the app reconnects with `serve.status.project_root` set and the team paused.

  `farik-e2e-serve` gains a no-project start and uses `MemoryStore` or `FileStore` in place of the keychain (`--no-keychain`).

## File map

```
docs/decisions/0022-…md, docs/SPEC.md (4.1, 8.3, 8.6)       creates / modifies (T6)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs      modifies: queries, methods, errors (T1)
packages/protocol-client/src/client.ts (+ test)               modifies: call() (T1)
crates/runtime/src/credential.rs (+ tests), Cargo.toml, crates/runtime/Cargo.toml, Cargo.lock   creates / modifies: keyring (T2)
crates/runtime/src/claude.rs                                  modifies: Source in the credential line's data (T2)
crates/runtime/src/daemon.rs, daemon/web.rs, daemon/setup.rs  modifies / creates: deps Option, setup queries and methods (T3)
crates/runtime/src/computer.rs (+ tests)                      creates: the four checks, sandbox build (T3)
crates/cli/src/{serve.rs,start.rs,state.rs,init.rs}, tests/serving.rs, src/bin/farik-e2e-serve.rs  modifies: setup mode, take-on, pause, state sandbox (T4)
apps/web/src/pages/setup/{SetupComputer,SetupAccount,SetupProject,FolderBrowser}.tsx (+ css, tests), app/App.tsx, strings/en.ts  creates / modifies (T5)
apps/web/e2e/{setup-project.spec.ts,fixtures/serve.ts,fixtures/fake-claude.sh}                   creates / modifies (T5)
docs/plans/project-plan.md (step 05 line)                     modifies (T6)
```

## Interfaces

Consumes: `init`, `open_project`, `repository_root`, `Project`, `start_holding`, `StartOptions`, `state_dir`, and `write_private` (CLI); `DaemonState`, `WebState`, `answer`, `query`, and `check_version` (runtime); `rpc_request_from_value`; `connect` and `DaemonClient`; `@farik/ui`; step 04's app frame.

Produces:

```rust
pub enum Source { Environment, Keychain, File }
pub trait CredentialStore: Send + Sync { fn load(&self) -> Result<Option<ClaudeCredential>, String>; fn save(&self, c: &ClaudeCredential) -> Result<(), String>; fn source(&self) -> Source; }
pub fn load_credential(env: &BTreeMap<String, String>, stores: &[&dyn CredentialStore]) -> Option<(ClaudeCredential, Source)>;
pub fn save_credential(c: &ClaudeCredential, keychain: &dyn CredentialStore, file: &dyn CredentialStore) -> Result<Source, String>;
pub struct Check { pub state: CheckState, pub version: Option<String> }  pub enum CheckState { Ready, Missing, TooOld, NotRunning }
pub fn check_computer(env: &BTreeMap<String, String>) -> ComputerCheck;  pub async fn build_sandbox_image() -> Result<String, String>;
DaemonState::new(deps: Option<Arc<ToolDeps>>) -> DaemonState   // None in setup mode
```

```ts
DaemonClient.call(method: 'project.open' | 'project.create' | 'account.connect' | 'sandbox.build', params: object): Promise<unknown>;
```

## Tasks

### Task 1: The wire for setup

Tests:
- `reads_the_setup_queries_and_methods` asserts that each new query and method validates with its params and refuses a missing required field.
- `client.call sends a method and resolves its result` asserts that `call('project.open', { path: 'p' })` sends `{ method: 'project.open', params: { path: 'p' } }` and resolves the result, camelCased.

- [ ] `feat(protocol): add the setup queries and methods to the browser's wire`

### Task 2: The credential stores

Tests:
- `reads_the_environment_before_the_stores` asserts that with `ANTHROPIC_API_KEY` set and a keychain holding a token, the result is the key with `Source::Environment`.
- `falls_back_to_the_file_when_there_is_no_keychain` asserts that a keychain store whose `save` errs leads `save_credential` to write the file and answer `Source::File`, and that the file has mode 0600 and parses as `{ kind, secret }`.
- `keeps_the_secret_out_of_debug` asserts that `format!("{:?}", credential)` does not contain the secret.
- `refuses_a_key_of_the_wrong_kind` asserts the prefix refusals for both kinds.

- [ ] `feat(runtime): keep the model credential in the keychain or a private file`

### Task 3: Setup queries, methods, and the computer check

Tests (route tests use a setup-mode `TestDaemon`):
- `lists_only_folders_inside_home` asserts that for a temporary home with `a/` (git), `b/`, `.hidden/`, a file `f.txt`, and a symlink `out -> /tmp`, the entries are exactly `a` (git true) and `b`, sorted, and that `path: "../"` is refused.
- `opens_a_git_folder_and_refuses_others` asserts `project.open` on `a` answers its root. On `b` it answers the not-a-git refusal, and on `a/sub` the inside-a-project refusal naming `a`.
- `creates_a_project_with_its_first_request` asserts that `project.create { parent: "", name: "bakery", description: "An ordering site." }` makes `bakery/` with a `.git`, a first commit whose README holds the description, `.farik/team.yaml`, a `team.paused` event, and a filed request whose text is the description. A second call is refused as already there.
- `answers_the_computer_check` asserts that, with fake `claude`, `git`, and `docker` scripts on `PATH` printing known versions (docker's `image inspect` exiting 1), the answer is `claude ready 2.1.300`, `git ready`, `docker ready`, `sandbox_image missing`, and that an old `claude` gives `too_old`.
- `answers_503_and_no_project_in_setup_mode` asserts `/command` 503 with the sentence, `tasks.list` `-32004`, and `serve.status` `project_root: null`.
- `connects_the_account_without_echoing_it` asserts that `account.connect` answers `stored_in`, that `account.status` names the kind and source, and that the reply frames never contain the secret.

- [ ] `feat(runtime): answer the setup wizard's folder, project, computer and account calls`

### Task 4: `farik serve` without a project, and taking one on

Tests (`crates/cli/tests/serving.rs`):
- `serves_setup_outside_a_project` asserts that in an empty folder with no `last_project`, `serve` prints the link and `serve.status` answers `project_root: null`.
- `reopens_the_last_project` asserts that with `state.json` naming a Farik project, `serve` from an empty folder serves that project.
- `takes_on_the_chosen_project_on_the_same_port` asserts that after `project.open` over the socket, the old socket closes. A new socket with the same cookie then connects on the same port, and `serve.status.project_root` is the project, paused. No second link is printed.
- `writes_the_no_sandbox_choice_into_the_project` asserts that after `state.json`'s `"sandbox": "none"` and a take-on, `.farik/local/settings.json` is `{"sandbox": "none"}`, and stderr has `NO_SANDBOX_WARNING`.

- [ ] `feat(cli): serve the setup wizard outside a project and take the chosen one on`

### Task 5: The three screens

Starts only after the founder's approval of the mockup is recorded in Decisions.

Tests (Vitest; each also passes axe):
- `checks_the_computer_and_offers_the_fixes` asserts that a `computer.check` with docker missing shows the Not found row with its fix text, disables Continue, and enables "Continue without Docker". A missing image shows "Prepare it", which calls `sandbox.build` with a busy button.
- `connects_a_subscription_key` asserts that pasting a key and "Save and continue" calls `account.connect` with `subscription_token`, shows where it was stored, and moves to `/setup/project`; a refusal shows its sentence under the field.
- `browses_folders_and_uses_one` asserts that the browser lists `folders.list` entries, opens a folder on "Open folder", marks non-git ones, and calls `project.open` with the relative path on "Use this folder".
- `starts_a_new_project` asserts that the new-project form calls `project.create` with the slugged name, and shows "Opening your project…".
- `sends_the_user_to_setup_without_a_project` asserts that `/` redirects to `/setup/computer` when `project_root` is null.
- `setup-project.spec.ts` (Playwright) runs the journey in the Tests decision.

- [ ] `feat(web): add the computer, account and project setup screens`

### Task 6: Spec and plan

- `docs/SPEC.md` 4.1: the new order, setup mode, the team paused until setup ends, the image build.
- 8.3: the no-sandbox choice made before a project exists.
- 8.6: ADR 0022's wording.
- `docs/plans/project-plan.md`: the step 05 line and the steps table row.

- [ ] `docs(spec): describe the first run's computer, account and project screens`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/protocol-client 7 tests; @farik/web 20 tests (15 + T5's 5);
#   playwright "3 passed"; last line: xtask check: ok
```
