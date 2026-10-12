# Phase 9, step 01: The project folder and its members

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 4.1, 8.4, 8.5; F1
Depends on: phase 8 (merged in #34); the phase's design and ADR 0057 (`docs/design/multi-repo-projects.md`, pull request #44, on this branch as 39923b4 and 4de2aa1). Nothing of steps 02 to 08.
Mockups: none; this step has no screen (step 07 draws them).
Readiness confirmed by: (pending; one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 4de2aa1.

## Goal

A Catervas project can be a project folder: a local git repository with no remote, holding `.catervas/` and `docs/catervas/`, whose `.catervas/repos.yaml` lists two to sixteen member repositories, each a git repository elsewhere on the computer (ADR 0057). The first-run wizard's host can make one (`project.create_folder`). Serve opens one as it opens any project. A driving daemon lists the members, saying which it cannot find (`repos.list`), and adds, removes and locates them (`repo.add`, `repo.remove`, `repo.locate`), each recorded in the log.

A one-repo project is unchanged: it has no `repos.yaml`, `repos.list` answers it with `null`, and `repo.add` refuses it.

Out of scope, each a later step's:
- tasks in a member, the contract's `repo` and `repo_in_use` (02)
- reading members from a session (03)
- the documents' `repos/<name>/` folders, and removing them with their member (04)
- a member's Catervafication (06)
- every screen (07)
- turning a one-repo project into a project folder (07b)

## Decisions

### The project folder

- **A project folder is a git repository's root.** `git init -b main`, a `README.md`, and one commit, made as `project.create` makes a new project (`crates/cli/src/setup.rs:257-266`). So `repository_root`, `scan_project`'s whole-repository check, `init`, `open_project`, `taken_on`, carry, connector keys (`local_project_id`, by the root) and serve's `found` all work on it unchanged.
  - The `README.md` is `# <name>\n\nThe Catervas project folder for <name>: its team and its documents. Its repositories are listed in .catervas/repos.yaml.\n`.
  - Rejected: a project root that is not a git repository. Every reader of `files.root()` (about 106) assumes one, and phase 8 reads documents from git.
- **What makes it one is `.catervas/repos.yaml`**, held to `docs/schemas/repos.schema.json`: `{ repos: [{ name, path, integration_branch? }] }`, 1 to 16 entries.
  - `name` matches `^[a-z][a-z0-9-]{0,31}$`.
  - `path` is an absolute path: `^(/|[A-Za-z]:[\\/])`, 1 to 4096 characters.
  - `integration_branch` is 1 to 255 characters, with no whitespace. When absent, the member's default branch is read when it is needed (step 02), as `integration_branch` (`crates/store/src/git.rs:1031`) reads the team's.
  - There is no `version` field. Unknown keys are refused.
  - The file is written before the event that records its change, as a sprint's is (spec 8.4).
  - Rejected: a field in `team.yaml`, which a template copies (spec 3) and which holds no machine paths.
- **The rules beyond the schema live in core**, on strings, since core does no I/O. Paths are compared segment by segment, after the caller has canonicalised them. The rules:
  - names are unique (`/repos/<i>/name`: "a repository's name names one repository, and <name> more than one")
  - paths are unique, and none is inside another (`/repos/<i>/path`: "<path> is inside <other path>, another of the project's repositories")
- **A member's checks, for create, add and locate, in this order.** These come from store, on the file system:
  1. `not_there`: the path does not exist.
  2. `outside_home`: the path, followed through its links, is not inside home.
  3. `home_itself`: the path is home.
  4. `not_a_repository`: the path is not a git repository.
  5. `not_the_top`: it is inside one; the refusal names the top folder.
  6. `has_team`: it has `.catervas/team.yaml`.

  Then these come from core, against the project folder and the other members:
  1. `repo_name`
  2. `name_taken`
  3. `path_taken` (the same canonical path as a member's)
  4. `nested` (inside a member, or holding one)
  5. `inside_project_folder`
  6. `holds_project_folder`
  7. `too_many` (a seventeenth member)

  Then store: `no_such_branch` (an `integration_branch` given that is not a local branch of the member).

  Each refusal has one sentence, used by both the wizard (`SetupError::Refused`) and the daemon (`REFUSED` with `data.errors: [{ path, message, code }]`, as `documents.rs` refuses). Paths in requests are relative to home, as `project.open`'s are (`setup.rs:65`); the canonical absolute path is what is stored.
  - Rejected: letting a repository with its own team join. Turning a one-repo project into a project folder is step 07b's.

### Making and opening

- **`project.create_folder { parent, name, repos: [{ name, path, integration_branch? }], no_sandbox }`**, a setup method beside `project.create`, answers `projectCreateResult { project_root }`.
  - `name` follows `project.create`'s rule (`[a-z0-9-]`, 1 to 64 characters, `setup.rs:227-233`). There must be 2 to 16 `repos` (the schema's `minItems` and `maxItems`).
  - Every check runs before anything is made: the name; the parent inside home; `parent/name` not already there; each member as above against `parent/name` and the members before it; the account; git present.
  - The parent may not exist yet, since the wizard's default is `Catervas` under home (step 07). Its nearest existing ancestor, followed through its links, must be inside home, else `outside_home`. Rejected: refusing a missing parent as `project.create` does, which would fail the default on every first project folder.
  - Then, in order:
    1. `mkdir -p` of the parent, then `mkdir`, `git init -b main`, the README, and its commit "Start the project".
    2. `write_repos`, which makes `.catervas/` when it is missing.
    3. `taken_on`: `init`, then the carried team while leaving, else paused with the setup marker.
    4. One `repo.added` for each member, in request order.
    5. `choose`.
  - It records no `project.started` and files no first request. Its members are existing repositories, so it is not a new project (spec 4.1), and the Catervafication is offered as for any existing repository.
  - Rejected: one `project.create` with an optional `repos`, which would make every existing check branch on it.
- **Opening a project folder is `project.open` as it is.** A folder whose `.catervas/repos.yaml` exists is never cleared: `replace` on it is refused with "that folder is a project of several repositories; open it as it is, or choose another". A leftover `.catervas/` without `team.yaml` is taken on without clearing when it holds `repos.yaml`. Everything else about `open` is unchanged, Stay on included.

### The daemon's members

- **The daemon knows home.** `DaemonState::set_home` is set by `connected_daemon` (`crates/cli/src/start.rs:258`) from `home_of(io)`, which is today's derivation in `serve.rs:164-168` (`HOME` when set and not empty, else the working directory) moved to `crates/cli/src/state.rs` and used by both. With no home set, `repo.add` and `repo.locate` answer `INTERNAL_ERROR` "the home folder is not known".
- **`repos.list {}`** answers `{ repos: null }` for a one-repo project. For a project folder it answers `{ repos: [{ name, path, integration_branch, missing }] }` in file order, with `integration_branch` as written or `null`. `missing` is `!repo_present(path)`: the path is not a directory, or git's top folder of it is not the path itself.
- **`repo.add { name, path, integration_branch? }`**:
  - It is refused with `not_a_project_folder` at `/` ("This project is one repository; adding another comes later.") when there is no `repos.yaml`. Step 07b lifts this.
  - Otherwise the checks above run, then `write_repos` with the member appended, then `repo.added`. It answers `{}`.
- **`repo.remove { name }`**:
  - `NOT_FOUND` when no member has that name.
  - `last_repo` at `/name` ("A project keeps at least one repository.") when it is the only member.
  - Otherwise `write_repos` without it, then `repo.removed`. It touches no file of the member. Step 02 adds `repo_in_use`; step 04 removes the member's documents.
- **`repo.locate { name, path }`** records where a member is now:
  - `NOT_FOUND` for an unknown name.
  - Otherwise the member's checks, the member itself left out of the others, then `write_repos` with the new path, then `repo.moved`.
  - It works whether or not the member is missing. Catervas cannot tell whether the folder is the same repository, and the page says so (step 07).
- **One writer at a time.** `repo.add`, `repo.remove` and `repo.locate` hold `transitions::integration_lock` (`crates/runtime/src/transitions.rs:65`) from reading `repos.yaml` to recording the event, off the async workers (`gates::off_the_worker`). From step 02, integrations read the members under the same lock. Rejected: a lock of its own, which step 02 would have to take beside this one.
- **Events, about no contract and naming no actor**, as `project.started` (`crates/protocol/src/event.rs:459`):
  - `repo.added { name, path, integration_branch: string | null }`. All three are required, so its shape differs from `repo.moved`'s.
  - `repo.removed { name }`
  - `repo.moved { name, path }`

  The shapes share `name`'s pattern and `path`'s bounds from `repos.schema.json`. The projections pass over all three; nothing on the board changes.

## File map

```
docs/schemas/repos.schema.json                                creates (Task 1)
crates/core/src/generated/mod.rs, crates/core/src/lib.rs      modifies: the repos module (Task 1)
crates/core/src/repos.rs, crates/core/src/repos/fixtures.rs   creates, tests (Task 1)
crates/store/src/files.rs                                     modifies, tests: REPOS, read_repos, write_repos, is_project_folder (Task 2)
crates/store/src/repos.rs, crates/store/src/lib.rs            creates: member_root, repo_present, MemberPathRefusal (Task 2)
crates/store/src/git.rs, crates/store/tests/git.rs            modifies, tests: has_local_branch (Task 2)
crates/store/tests/repos.rs                                   creates: member_root's and repo_present's tests (Task 2)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies (Task 3)
crates/store/src/projections.rs                               modifies: passes over repo.* (Task 3)
crates/runtime/src/daemon/setup.rs, crates/runtime/src/daemon/web.rs   modifies: create_folder in SetupHost, SETUP_METHODS (Task 4)
crates/cli/src/setup.rs                                       modifies, tests: create_folder, open's guard (Task 4)
crates/runtime/src/daemon/repos.rs, crates/runtime/src/daemon.rs   creates, modifies: the queries and methods, set_home (Task 5)
crates/cli/src/state.rs, crates/cli/src/serve.rs, crates/cli/src/start.rs   modifies: home_of, set_home (Task 5)
docs/schemas/rpc.schema.json                                  modifies: project.create_folder (Task 4); repos.list, repo.* (Task 5)
packages/protocol-client/src/client.ts                        modifies: MethodName (Tasks 4, 5)
crates/cli/tests/serving.rs                                   modifies, tests: a project folder served (Task 4)
docs/SPEC.md, docs/design/multi-repo-projects.md              modifies (Task 6)
```

## Interfaces

Consumes: `ValidationError`, `pointer` (`crates/core/src/contract.rs:134`, `:294`); `ProjectFiles` with `read_yaml`, `write_yaml`, `as_wire`, `refused` (`files.rs:1060`, `:1066`, `:925`, `:904`); `Git::open`, `top_level`, `check_branch_name` (`git.rs:128`, `:147`, `:445`); `TempRepo` (`git/fixtures.rs:22`); `EventBody`, `EVERY_KIND`, `body_def_name`, `attribution` (`event.rs:615`, `:466`, `:146`, `:348`); `CliHost` with `inside_home`, `has_account`, `git`, `taken_on`, `choose` (`setup.rs:65`, `:78`, `:145`, `:88`, `:139`); `Project::event`, `Project::append` (`project.rs:78`, `:112`); `team::append` (`daemon/team.rs:1746`); `integration_lock`; `off_the_worker` (`gates.rs:806`); `Failure`, `REFUSED`, `NOT_FOUND`, `INTERNAL_ERROR` (`web.rs:606-615`); `TestProject::new` (`tools/fixtures.rs:182`), `TestDaemon` (`daemon/fixtures.rs:38`), `gates::tests::{query, call}`.

Produces:

```rust
// catervas_core::repos
pub use crate::generated::repos::{Repos, Repo, RepoName};
pub const MAX_REPOS: usize = 16;
pub fn validate_repos(input: &Value) -> Result<Repos, Vec<ValidationError>>;
pub enum MemberRefusal { RepoName, NameTaken, PathTaken { name: String }, Nested { name: String },
                         InsideProjectFolder, HoldsProjectFolder, TooMany }
impl MemberRefusal { pub fn code(&self) -> &'static str; pub fn message(&self) -> String; pub fn pointer(&self) -> &'static str; }
pub fn check_member(project_folder: &str, others: &[Repo], name: &str, path: &str) -> Result<(), MemberRefusal>;
pub fn is_within(path: &str, folder: &str) -> bool;   // equal or below, segment by segment, `/` or `\`
// catervas_store::files::ProjectFiles
pub fn read_repos(&self) -> Result<Option<Repos>, FilesError>;   // None: no repos.yaml
pub fn write_repos(&self, repos: &Repos) -> Result<(), FilesError>;
pub fn is_project_folder(&self) -> bool;
// catervas_store::repos
pub enum MemberPathRefusal { NotThere, OutsideHome, HomeItself, NotARepository, NotTheTop { top: PathBuf },
                             HasTeam, NoSuchBranch }
impl MemberPathRefusal { pub fn code(&self) -> &'static str; pub fn message(&self) -> String; pub fn pointer(&self) -> &'static str; }
pub fn member_root(home: &Path, path: &str) -> Result<PathBuf, MemberPathRefusal>;   // canonical
pub fn repo_present(path: &Path) -> bool;
// catervas_store::git::Git
pub fn has_local_branch(&self, name: &str) -> Result<bool, GitError>;
// catervas_runtime::daemon
pub trait SetupHost { /* gains */ fn create_folder(&self, parent: &str, name: &str, repos: &[MemberRequest], no_sandbox: bool) -> Result<PathBuf, SetupError>; }
pub struct MemberRequest { pub name: String, pub path: String, pub integration_branch: Option<String> }
impl DaemonState { pub fn set_home(&self, home: PathBuf) -> bool; }
// catervas_cli::state
pub(crate) fn home_of(io: &CliIo<'_>) -> PathBuf;
// events: repo.added { name, path, integration_branch|null }, repo.removed { name }, repo.moved { name, path }
```

Wire (`rpc.schema.json`): `project.create_folder` as Decisions; `repos.list {}` gives `reposListResult { repos: [{ name, path, integration_branch: string|null, missing }] | null }`; `repo.add { name, path, integration_branch? }`, `repo.remove { name }` and `repo.locate { name, path }` each give `emptyResult`.

## Tasks

### Task 1: A project's repositories in core

Files: `repos.schema.json`, whose `$defs.repoName` gives typify's `RepoName`; `generated/mod.rs` (`pub mod repos`, as `sprint`); `repos.rs` (validator as `sprint.rs:20-43`, rules as `validate_team`'s, `team.rs:258`); `repos/fixtures.rs` (`two_repos()`: `app` at `/home/me/app`, `infra` at `/home/me/infra` with `integration_branch: main`).

- `reads_a_projects_repositories`: `two_repos()` reads back equal; one entry reads; 17 are refused at `/repos`, an empty list at `/repos`, `name: "App"` and `name: "1app"` at `/repos/0/name`, a 33-character name likewise, `path: "app"` at `/repos/0/path`, `C:\\work\\app` reads, an unknown key at `/repos/0`, and `integration_branch: ""` at `/repos/0/integration_branch`. RED: no module.
- `refuses_a_name_or_a_path_twice`: two `app` entries are refused at `/repos/1/name` with the Decisions' sentence; `/home/me/app` and `/home/me/app/web` at `/repos/1/path`, naming both; `/home/me/app` and `/home/me/apples` read. RED.
- `checks_a_new_member`: against project folder `/home/me/shop` and `two_repos()`:
  - `("web", "/home/me/web")` is `Ok`
  - `("App", ..)` is `RepoName`
  - `("app", "/home/me/web")` is `NameTaken`
  - `("web", "/home/me/app")` is `PathTaken { name: "app" }`
  - `("web", "/home/me/app/web")` and `("web", "/home/me")` are each `Nested { name: "app" }`
  - `("web", "/home/me/shop/web")` is `InsideProjectFolder`
  - `("web", "/home")` is `Nested` before `HoldsProjectFolder`, as Decisions orders them
  - with 16 members, `TooMany`

  Each `code()` is its snake_case name (`repo_name`, …, `too_many`), and `pointer()` is `/name` for the first two and `/path` for the rest. RED.
- `is_within_by_segment`: `/a/b` within `/a`, `/a` within `/a`, `/ab` not within `/a`, `C:\\a\\b` within `C:\\a`, and `/a/../b` is not handled here (callers pass canonical paths; the doc comment says so). RED.

- [ ] `feat(core): hold a project's repositories to their rules`

### Task 2: The members on disk

Files: `files.rs` (`REPOS = "repos.yaml"` with the consts at `:750`; read and write as `read_team`/`write_team`, `:176`, `:186`; `write_repos` makes `.catervas/` first); `store/src/repos.rs`; `git.rs` (`rev-parse --verify --quiet refs/heads/<name>`); tests in `files.rs` on `TempProject`, and `#[ignore = "needs the git program: cargo xtask check --integration"]` integration tests on `TempRepo`.

- `reads_and_writes_the_repositories` (`files.rs`): with no file `read_repos` is `None` and `is_project_folder` false. `write_repos(two_repos())` into a folder with no `.catervas/` makes it, and reads back equal, with `is_project_folder` true. A hand-edited file with a duplicate name is `FilesError::Invalid` naming `.catervas/repos.yaml` and `/repos/1/name`. RED.
- `finds_a_members_root` (`tests/repos.rs`), with home a temp folder holding repository `app`, a plain folder `notes`, a repository `team` with `.catervas/team.yaml`, and a link `ln` to `app`:
  - `app` and `ln` give `<home>/app`, canonical
  - `nowhere` is `NotThere`
  - `../` is `OutsideHome`
  - `""` and `.` are `HomeItself`
  - `notes` is `NotARepository`
  - `app/src` is `NotTheTop { top: <home>/app }`
  - `team` is `HasTeam`

  Each `code()` is its snake_case name and `pointer()` is `/path`. RED.
- `says_whether_a_member_is_present` (`tests/repos.rs`): `app` is present; once moved away it is not; a plain folder put at its path is not. RED.
- `knows_a_local_branch` (`tests/git.rs`): `main` true, `feature/x` false, and true once made. RED.

- [ ] `feat(store): read and write a project folder's repositories`

### Task 3: The members in the log

Files: `event.schema.json` (the three kinds after `catervafication.started`, :170, and their bodies in `eventBodyWire`'s `oneOf`); `event.rs` (bodies re-exported in a group with a doc comment, variants, `body_def_name`, `kind()`, `attribution`'s `None` arm with a sentence in its doc comment, `EVERY_KIND` to 114 and its five length asserts); `lib.rs` `KINDS`; `fixtures.rs` `a_body_wire`; `projections.rs` `apply_to`'s pass arm (`:784`).

- `reads_the_repository_events`: each kind reads and writes back unchanged and `is_about_one_contract` is false for each. `repo.added` without `integration_branch` is refused at `/body`; with `null` it reads; `name: "App"` is refused at `/body/name`; `path: ""` at `/body/path`; `repo.removed { name, path }` at `/body`. RED.
- `projects_past_the_repository_events` (`projections.rs`): a log with one of each projects the same board as without them. RED.

- [ ] `feat(protocol): record a project's repositories`

### Task 4: Making and opening a project folder

Files: `daemon/setup.rs` (`create_folder`, `MemberRequest`); `web.rs` (`SETUP_METHODS` to 6, its dispatch in `setup_call`, `:1003-1034`); `cli/src/setup.rs` (`create_folder`; `open`'s guard and `clear`'s skip, `:204-215`); `rpc.schema.json` (`projectCreateFolderRequest` beside `projectCreateRequest`, `:1470`); `client.ts` (`"project.create_folder"`); tests in `setup.rs` with `host(None)` (`:427`) and in `tests/serving.rs`.

- `makes_a_project_folder_of_its_repositories` (`setup.rs`): home holding repositories `app` and `infra`. `create_folder("Catervas", "shop", [app, infra with integration_branch main], false)` makes `<home>/Catervas/shop`, whose README is the Decisions' text, committed. Its `.catervas/repos.yaml` holds both, with canonical paths and in order. The log holds `team.paused`, then `repo.added` for `app` (`integration_branch: null`), then for `infra` (`main`), and no `project.started` and no task. The setup marker is there, and the answer is the folder. RED: no method.
- `refuses_a_project_folder_before_making_anything` (`setup.rs`): each refused with its sentence, `<home>/Catervas/shop` not made, nothing recorded:
  - a bad name
  - `shop` already there
  - parent `../elsewhere` (`outside_home`)
  - `app` named twice (`name_taken`)
  - `app` and `app/src` (`not_the_top`)
  - a member at `Catervas/shop/x` (`not_there`, since it is not made yet)
  - `app` with branch `nope` (`no_such_branch`)
  - a member with a team (`has_team`)
  - no account
  RED.
- `carries_the_team_into_a_new_project_folder` (`setup.rs`): with `leaving` a one-repo project, `create_folder` carries its team as `leaving_creates_a_project_with_the_carried_team` (`:884`) checks, with no setup marker, and records the two `repo.added`. RED.
- `never_clears_a_project_folder` (`setup.rs`): while leaving, `open` of a project folder with `replace: true` is refused with the Decisions' sentence and its `.catervas/` is untouched. A project folder whose `.catervas/` holds `repos.yaml` but no `team.yaml` is taken on with `repos.yaml` kept. RED.
- `serves_a_project_folder` (`tests/serving.rs`, as `takes_on_the_chosen_project_on_the_same_port`, `:1313`): `project.create_folder` over the wizard on the held port. `serve.status`'s `project_root` is the project folder, and `repos.list` answers both members, `missing: false`. RED.

- [ ] `feat(cli): make a project folder of several repositories`

### Task 5: The daemon lists, adds, removes and locates members

Files: `daemon/repos.rs` (`QUERIES: [&str; 1] = ["repos.list"]`, `METHODS: [&str; 3]`, `query`, `call`, refusals as `documents.rs`'s `refusal`); `daemon.rs` (`mod repos;`, `home: OnceLock<PathBuf>`, `set_home`); `web.rs` (method and query dispatch beside `documents`, `:651`, `:874`); `cli/src/state.rs` (`home_of`); `serve.rs:164` and `start.rs:258` (use it); `rpc.schema.json`; `client.ts`. Tests in `repos.rs` on a `TestProject` whose repository is the project folder, two `TempRepo` members under a temp home, and `set_home`.

- `lists_the_members_and_which_are_missing`: `repos.list` answers both in file order, `infra`'s `integration_branch` `main`, `app`'s `null`, each `missing: false`. With `app`'s folder moved away, `app` is `missing: true`. On a one-repo `TestProject`, `{ repos: null }`. RED: unknown query.
- `adds_a_member`: `repo.add { name: "web", path: "web" }` (relative to home) appends it with its canonical path and records `repo.added`. Each refusal is answered by its code and pointer, with nothing written or recorded: `not_a_project_folder` on a one-repo project, `name_taken`, `nested`, `outside_home`, `no_such_branch`, and `too_many` at sixteen. RED.
- `removes_a_member_but_not_the_last`: removing `app` leaves `infra` and records `repo.removed`, and `app`'s folder is untouched. Removing `infra` then is `last_repo` at `/name`, and `nope` is `NOT_FOUND`. RED.
- `locates_a_moved_member`: `app` moved to `<home>/apps/app`; `repo.locate { name: "app", path: "apps/app" }` writes the new path, records `repo.moved`, and `repos.list` then says `missing: false`. Locating `app` at `infra`'s folder is `path_taken`, and at its own present path is accepted. RED.
- `changes_the_members_one_at_a_time`: with `integration_lock` held by the test, a `repo.add` on another thread has written nothing 300 ms in, as `waits_for_the_lock` (`tools/fixtures.rs:585`) checks, and adds once it is let go. RED: no lock taken.

- [ ] `feat(runtime): list, add, remove and locate a project's repositories`

### Task 6: Spec

`docs/SPEC.md`, at the next revision (0.105 as written):
- 3: a project is one repository or a project folder with member repositories (ADR 0057)
- 4.1: `project.create_folder`, its checks and its events; a project folder is opened as it is and never replaced
- 8.4: `.catervas/repos.yaml` and its schema; the project folder is a local git repository
- 8.5: `repo.added`, `repo.removed`, `repo.moved`
- the daemon's `repos.list` and `repo.*` in the section that lists the web app's queries and methods

`docs/design/multi-repo-projects.md`: `repo.added` carries `integration_branch` as `null` when absent.

- [ ] `docs(spec): record project folders and their repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
