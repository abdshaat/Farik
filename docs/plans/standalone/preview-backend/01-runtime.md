# Standalone (preview backend), plan 1: the preview reaches the app's backend

Status: ready
Branch: `feat/preview-backend` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.6, 8.3, 8.6 (recorded in plan 2's revision 0.120)
Depends on: `feat/parallel-agents` at `2006a978`, and `feat/internal-task-ids` (ADR 0066) beneath it
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The preview's `prepare` and `start` containers get the owner's `.catervas/local/preview.env` when it
is there, `start` runs on the network with the owner's computer named `host.docker.internal`, and
the Designer's browser reaches that name past its dead proxy. No agent can read the file, and no
event, log line or web answer carries its values. Out of scope: Settings' line and the spec (plan 2).

## Decisions

Every decision is ADR 0067's; this section pins how.

- **The file's place and type.** `PREVIEW_ENV = ".catervas/local/preview.env"` from the project
  folder (`ProjectFiles::root()`), read by `PreviewEnv::read(root)`: `Some` when the path is a file
  (`Path::is_file`, which follows a link), with `path` made absolute by `std::path::absolute`; else
  `None`. Its values are parsed by Docker's env-file rules, for hiding only: each line, leading
  whitespace trimmed, that is empty or starts with `#` is skipped; a line with `=` gives the text
  after the first `=`, as written; a line without `=` gives nothing. A file that cannot be read as
  UTF-8 text gives no values and is still `Some` (Docker reads it, or refuses it in its own words).
  `PreviewEnv` derives `Clone, PartialEq, Eq` and has a hand-written `Debug` that prints
  `PreviewEnv { path: <path>, values: <count> }`, never a value.
- **The factory is told, not the team.** `PreviewFactory::start` gains a last parameter
  `env: Option<&PreviewEnv>`. Rejected: holding the root in `DockerPreviewFactory` (it is built once
  per process, and the project can change, ADR 0053); a field of `Preview` (that is `team.yaml`'s
  wire shape). The orchestrator reads the file each time it opens a preview (`open_preview`), so a
  file placed or edited is used from the next preview on. `prepare`'s cache key is unchanged.
- **The arguments are built by pure functions**, as `sandbox/docker.rs::run_args` is, so a test
  reads them without Docker. `start`'s, in order: `run -d --init --name <name> --network bridge
  --add-host host.docker.internal:host-gateway [--env-file <path>] --mount <mount> -w /workspace
  --user <user> [--label <label>]... --label <tree_label> <image> sh -c <start>`. `prepare`'s:
  `run --rm --name <preparing> --network bridge --add-host host.docker.internal:host-gateway
  [--env-file <path>] --mount <mount> -w /workspace --user <user> [--label <label>]... <image> sh -c
  <PREPARE_SCRIPT> sh <prepare>`. The probe (`docker exec ... curl localhost`) and the port finding
  are unchanged.
- **The browser.** `PROXY_BYPASS = "localhost,host.docker.internal"` replaces `"localhost"` in both
  `connector_server` and `check_page` (comma: Playwright splits `proxy.bypass` on commas, and the
  pinned server's `--proxy-bypass` help says so). `--allowed-origins` becomes
  `"<preview origin>;http://host.docker.internal:*;https://host.docker.internal:*"` (`OWNER_ORIGINS`;
  semicolons, as the server splits that flag; `scheme://host:*` is its every-port pattern).
  `check_page` passes no allowed origins today and gains none. No `--add-host` on the browser or the
  check: `--network container:<preview>` gives them the preview's `/etc/hosts`, and Docker refuses
  the two together (checked 2026-10-17). The governor's `url` rule stays the preview's origin.
- **Hiding.** `PreviewEnv::hide(text)` replaces each value of at least 4 characters with
  `HIDDEN = "[hidden]"`, longest value first (so a value that holds another is hidden whole).
  `preview_failed` (`orchestrator/session.rs`) hides `error.to_string()` with
  `PreviewEnv::read(deps.tools.files.root())` before it is the stop's reason, the escalation's
  detail and the session end's detail; that is every path a preview's failure text takes, for every
  factory. No other text holds a value: `--env-file` is a path, and Docker reads the file itself.
- **The web answer.** `team.get` gains `preview_env_present: bool`, `PreviewEnv::read(root).is_some()`,
  in `docs/schemas/rpc.schema.json`'s `teamGetResult` (properties and `required`). Plan 2 shows it.
- **What guards the file from agents is already there** and gets a test, not code: the always
  protected `.catervas/local/**` (`DEFAULT_PROTECTED_PATHS`, `Team::rules`), the hook's
  `path_outside_workspace` for a task's worktree, its `path_protected` for the project folder as
  `cwd` or as the `project` read root, and `sandbox_for` leaving the project folder out of the
  sandbox's mounts (`crates/runtime/src/orchestrator.rs`, the `reads` filter).
- **One guarantee, two boundaries.** "Agents never read it, Catervas never shows it" is one security
  guarantee (ADR 0060). Its two boundaries live in two harnesses, the daemon's hook and query
  (`TestDaemon`) and the orchestrator's events (`Harness`), so it is two tests, in Task 3.
- **Integration test.** `the_browser_reaches_only_the_preview`'s case (a) asserted the preview's
  network was off; Task 2 rewrites it for the bridge and proves the owner's computer is reached.
- No ADR beyond 0067; no event kind changes.

## File map

```
crates/runtime/src/preview.rs                 modifies: PROXY_BYPASS, OWNER_ORIGINS, connector_server, check_page (Task 1);
                                                        PREVIEW_ENV, PreviewEnv{read,path}, start's `env` on the trait, NoPreviews,
                                                        PolledPreviews, fixtures (Task 2); HIDDEN, PreviewEnv::hide, FakePreviews
                                                        records `env` (Task 3)
crates/runtime/src/preview.rs                 tests:    launches_the_browser_confined (Task 1)
crates/runtime/src/preview/docker.rs          modifies: Launch, start_args, prepare_args, start and prepare use them (Task 2)
crates/runtime/src/preview/docker.rs          tests:    starts_the_preview_on_the_network_with_the_owners_environment_file (Task 2)
crates/runtime/tests/playwright_connector.rs  modifies: start's new argument at every call; case (a) rewritten (Task 2)
crates/runtime/src/orchestrator/session.rs    modifies: open_preview passes None (Task 2), then the read file (Task 3); preview_failed hides (Task 3)
crates/runtime/src/orchestrator/session.rs    tests:    a_failed_preview_never_shows_the_environment_values (Task 3)
crates/runtime/src/daemon/team.rs             modifies: team.get answers preview_env_present (Task 3)
docs/schemas/rpc.schema.json                  modifies: teamGetResult.preview_env_present (Task 3)
crates/runtime/src/daemon/hooks.rs            tests:    no_agent_reads_the_preview_environment_file (Task 3)
```

## Interfaces

Consumes (on this branch): `connector_server`, `check_page`, `BLACKHOLE_PROXY`, `RunningPreview`,
`PreviewFactory`, `NoPreviews`, `PolledPreviews`, `fixtures::{FakePreviews, UnaskedPreviews,
HeldPreviews, NamedPreview}` (`crates/runtime/src/preview.rs`); `DockerPreviewFactory`,
`PREPARE_SCRIPT`, `DockerPreview::prepare` (`preview/docker.rs`); `open_preview`, `preview_failed`,
`escalates_a_preview_that_fails`'s `Harness` use (`orchestrator/session.rs`); `TestDaemon`,
`decide_pre_tool_use`, `denied_for`, `SessionRegistration`, `ReadRoot`, `DEFAULT_SESSION_LIMITS`
(`daemon/hooks.rs` tests, `daemon.rs`); `super::gates::tests::query` (`daemon/gates.rs`, as
`daemon/fixtures.rs::connector_states` uses it); `ProjectFiles::root`.

Produces:

```rust
// crates/runtime/src/preview.rs
/// The hosts the browser reaches past the dead proxy: the preview, and the owner's computer (ADR 0067).
pub const PROXY_BYPASS: &str = "localhost,host.docker.internal";
/// The origins the browser may request beside the preview's: the owner's computer, on any port.
pub const OWNER_ORIGINS: [&str; 2] = ["http://host.docker.internal:*", "https://host.docker.internal:*"];
/// The owner's environment file for the preview, from the project folder (ADR 0067).
pub const PREVIEW_ENV: &str = ".catervas/local/preview.env";
/// What stands for a value of that file in a text Catervas records.
pub const HIDDEN: &str = "[hidden]";
#[derive(Clone, PartialEq, Eq)]
pub struct PreviewEnv { path: PathBuf, values: Vec<String> }
impl PreviewEnv {
    #[must_use] pub fn read(root: &Path) -> Option<Self>;
    #[must_use] pub fn path(&self) -> &Path;
    #[must_use] pub fn hide(&self, text: &str) -> String;          // Task 3
}
impl fmt::Debug for PreviewEnv { /* path and count only */ }
pub trait PreviewFactory: Send + Sync {
    fn start(&self, project_id: &str, task_id: &TaskId, worktree: &Path, preview: &Preview,
             tree: &str, env: Option<&PreviewEnv>) -> Result<Box<dyn RunningPreview>, PreviewError>;
    // available, settle unchanged
}
// fixtures: FakePreviews gains `pub(crate) envs: Mutex<Vec<Option<PathBuf>>>`, each `env`'s path it was asked with (Task 3)

// crates/runtime/src/preview/docker.rs
/// The owner's computer as the preview's containers name it.
const HOST_MAPPING: &str = "host.docker.internal:host-gateway";
/// What `prepare`'s and `start`'s `docker run` share.
pub(crate) struct Launch<'a> {
    pub(crate) name: &'a str, pub(crate) mount: &'a str, pub(crate) user: &'a str,
    pub(crate) labels: &'a [String], pub(crate) image: &'a str, pub(crate) env: Option<&'a Path>,
}
pub(crate) fn start_args(launch: &Launch<'_>, tree_label: &str, start: &str) -> Vec<String>;
pub(crate) fn prepare_args(launch: &Launch<'_>, prepare: &str) -> Vec<String>;
```

## Tasks

### Task 1: the Designer's browser reaches the owner's computer

Files: modified `crates/runtime/src/preview.rs`; tested by its `launches_the_browser_confined`
Produces: `PROXY_BYPASS`, `OWNER_ORIGINS`
Consumes: `connector_server`, `check_page`, `NamedPreview`, `after` (on this branch)

`connector_server` passes `--proxy-bypass PROXY_BYPASS` and `--allowed-origins` with the preview's
origin then `OWNER_ORIGINS`, joined by `;`. `check_page` passes `--proxy-bypass PROXY_BYPASS`. The
doc comments of `BLACKHOLE_PROXY` and `connector_server` say "not to `localhost` or
`host.docker.internal`".

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `launches_the_browser_confined` (amended) — asserts, of the arguments after the image,
  `--proxy-bypass` is `["localhost,host.docker.internal"]` and `--allowed-origins` is
  `["http://localhost:4400;http://host.docker.internal:*;https://host.docker.internal:*"]`; every
  other assertion stays. Fails first on the bypass, `localhost` alone.

- [ ] `feat(runtime): the designer's browser reaches the owner's computer`

### Task 2: the preview runs on the network with the owner's environment file

Files: modified `crates/runtime/src/preview.rs`, `crates/runtime/src/preview/docker.rs`,
`crates/runtime/src/orchestrator/session.rs`, `crates/runtime/tests/playwright_connector.rs`;
tested by `docker.rs`'s tests
Produces: `PREVIEW_ENV`, `PreviewEnv::{read, path}`, its `Debug`, the trait's `env`, `Launch`,
`start_args`, `prepare_args`, `HOST_MAPPING`
Consumes: Task 1's `PROXY_BYPASS` and `OWNER_ORIGINS` (through `connector_server`), `PREPARE_SCRIPT`

`DockerPreviewFactory::start` builds `start`'s arguments with `start_args`, and
`DockerPreview::prepare` gains `env: Option<&Path>` and builds its client's with `prepare_args`;
both pass `env.map(PreviewEnv::path)`. `NoPreviews`, `PolledPreviews` (passing `env` on to
`inner`), `FakePreviews`, `UnaskedPreviews` and `HeldPreviews` take the parameter. `open_preview`
passes `None` until Task 3. Every `.start(` in `tests/playwright_connector.rs` passes `None`. The
module doc of `docker.rs` says "`start` in one on the bridge, the owner's computer at
`host.docker.internal`".

`the_browser_reaches_only_the_preview` becomes `the_browser_reaches_only_the_preview_and_the_owners_computer`;
cases (b) and (c) stay. Case (a): a `std::net::TcpListener` bound to `0.0.0.0:0` on a thread of the
test answers every connection with `HTTP/1.1 200 OK`, `Content-Type: text/html`,
`Connection: close` and `<title>Owner</title>`; the preview from `DockerPreviewFactory` (on the
bridge now) is browsed with every shipped flag (`left_out` empty), and the browser also navigates
to `http://host.docker.internal:<listener port>/`. `browse` gains a fourth navigation, to an
`extra: Option<&str>` address, answered as a fourth string (`None` for (b) and (c), which ignore it).
It asserts: the preview's page; `direct` contains `ERR_BLOCKED_BY_CLIENT` (the allowed origins);
the redirect contains `ERR_PROXY_CONNECTION_FAILED` (the proxy); the owner's page contains
`Page Title: Owner`.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `starts_the_preview_on_the_network_with_the_owners_environment_file` — in a temporary project
  folder: with no file, `PreviewEnv::read` is `None`, and `start_args` and `prepare_args` (a
  `Launch` with `env: None`) each hold `--network bridge` and
  `--add-host host.docker.internal:host-gateway` once, both before the image, and no `--env-file`;
  then with `.catervas/local/preview.env` written as
  `NEXT_PUBLIC_SUPABASE_URL=http://host.docker.internal:54321` and
  `SUPABASE_SERVICE_ROLE=service-role-secret-0123`, `read` is `Some`, its `path()` is absolute and
  ends with `PREVIEW_ENV`, each of the two argument lists holds `--env-file <that path>` exactly
  once before the image, no argument of either holds `service-role-secret-0123`, and
  `format!("{env:?}")` holds neither value. Fails first because `start_args` does not exist (write
  the test against the signatures above, then the functions).

- [ ] `feat(runtime): the preview runs on the network with the owner's environment file`

### Task 3: no agent reads the file, and Catervas never shows its values

Files: modified `crates/runtime/src/preview.rs`, `crates/runtime/src/orchestrator/session.rs`,
`crates/runtime/src/daemon/team.rs`, `docs/schemas/rpc.schema.json`; tested by
`orchestrator/session.rs`'s and `daemon/hooks.rs`'s tests
Produces: `HIDDEN`, `PreviewEnv::hide`, `FakePreviews.envs`, `team.get`'s `preview_env_present`
Consumes: Task 2's `PreviewEnv::{read, path}` and the trait's `env`

`open_preview` reads `PreviewEnv::read(tools.files.root())` and moves it into the blocking closure,
passing `env.as_ref()`. `preview_failed` builds `why` as
`PreviewEnv::read(deps.tools.files.root()).map_or_else(|| error.to_string(), |env| env.hide(&error.to_string()))`.
`team.get` sets `answer["preview_env_present"]`. The schema's description: "Whether the owner's
`.catervas/local/preview.env` is there for the preview (ADR 0067). Never its names or values."

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `a_failed_preview_never_shows_the_environment_values` (`orchestrator/session.rs`, ignored as its
  neighbours are: "needs the git program") — built as `escalates_a_preview_that_fails` is, with
  `.catervas/local/preview.env` in the harness's project folder holding
  `SUPABASE_SERVICE_ROLE=service-role-secret-0123` and `PORT=300`, and an `Arc<FakePreviews>`
  failing with `NeverAnswered { tail: "booting with service-role-secret-0123 on 300" }`; after
  `run_session`: the factory's `envs` is `[Some(<absolute path of the file>)]`; the escalation's
  detail and `preview.stopped`'s reason each contain `booting with [hidden] on 300`; and no event
  of the log, each serialised to JSON, contains `service-role-secret-0123`. Fails first on `envs`
  (`[None]`), then on the detail.
- `no_agent_reads_the_preview_environment_file` (`daemon/hooks.rs`, ignored: "needs the git
  program") — with the file written in `daemon.project.repo.path` holding
  `SUPABASE_SERVICE_ROLE=service-role-secret-0123`: from the developer's task session
  (`daemon.dev_call`), `Read` of its absolute path and `Grep` with `path` its folder are denied
  `path_outside_workspace`; from a session registered with `cwd` the project folder, no task and
  purpose `Chat`, `Read` of `.catervas/local/preview.env` and of its absolute path are denied
  `path_protected`; from a task session registered with the read root
  `ReadRoot { name: "project", path: <canonical project folder> }`, `Read` of its absolute path is
  denied `path_protected`; and `team.get` (`query(&daemon.state, "team.get", &json!({}),
  "teamGetResult")`) answers `preview_env_present: true`, and its JSON text does not contain
  `service-role-secret-0123`. Fails first on `preview_env_present` (the schema refuses the unknown
  field, or it is missing); the hook's denials already hold, which the test pins.

- [ ] `feat(runtime): no agent reads the preview's environment file and no record shows it`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the Docker tests of playwright_connector.rs and the git tests run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
