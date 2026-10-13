# Phase 9, step 03: Reading across repositories

Status: ready (readiness review 2026-10-12, three rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 5.12, 6.5, 8.2, 8.3; F3
Depends on steps 01 and 02 of this phase, planned and ready, not yet executed:
- from step 01: `read_repos`, `Repos`, `Repo`, `repo_present`
- from step 02: the contract's `repo`, `Harness::project_folder`, `TestProject::project_folder`

Nothing of steps 02b, 02c or 04 to 08.
Mockups: none.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on B1 (which member is the task's own) and B2 (what bounds a search in a read root), both decided below; S1–S9 and N1–N6 folded, the plan rewritten. Round 2 (the same reviewer): B1 and B2 decided; new B3 (the absolute deny rule for `.catervas/local/**` covering every task's own worktree) and B4 (a root path Claude Code's settings cannot hold), both decided below, with S1–S3. Round 3 (confirming B3 and B4): ready; its S1 and N1 folded.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 9e3f662.

## Goal

In a project folder, every agent session but the Marketing Specialist's, and but a task in a private folder, may read every other member and the project folder's documents, wherever its own working folder is:
- the Developer's task in `app` reads `infra`'s code and the Architect's documents
- the Architect's planning session at the project root reads every member

Writing is unchanged: a session writes only inside its own working folder, under its task's `allowed_paths`. Searches into a root are bounded so that nothing protected is reached: secrets, git's internals, and the project folder's `.catervas/local/`. A task's sandbox also mounts every other present member read-only at `/repos/<name>`, so a command can read a sibling. The session prompt lists the members and says where each is.

A one-repo project is unchanged.

Out of scope:
- each member's purpose in the prompt, read from `system.md` (step 05)
- the preview container's mounts
- members added while a task's sandbox runs, which are mounted only when the sandbox is next made

## Decisions

### Which folders a session reads

- **`read_roots(files, contract, cwd, role) -> Vec<ReadRoot>`**: the canonical folders a session may read outside its `cwd`. The roots are none, as phase 8 has them, when:
  - the project is one repository
  - `read_access(role)` (`crates/core/src/folders.rs:403`) is not `Open`: the Marketing Specialist
  - the contract is a task in a private folder (`task_private_folder`), since "the folder is all it reaches" (`hooks.rs:1708`) stays true for the Finance and Procurement Specialists
  - `repos.yaml` cannot be read: reads fail closed and the session still starts

  Otherwise the roots are every present member except the contract's own `repo`, by name, in `repos.yaml` order, then the project folder, named `project`, unless the canonical `cwd` is the project root.
  - **A root whose path cannot be written into a rule is left out**, failing closed: a path holding `(`, `)`, `\`, a control character (as `refuse_an_unexpressible_glob`, `claude.rs:862-876`, refuses a glob), or a glob character `*`, `?`, `[`, `]`, `{`, `}`. In `-p` mode Claude Code ignores a settings value it cannot parse, hooks and all (`claude.rs:858-861`), and a glob character would change what the rule matches. On Windows, whose paths hold `\`, no member is a root in this step.
  - The task's own member is known from its contract, not from `cwd`. A worktree always sits under the project folder (step 02), so no member ever holds `cwd`.
  - Rejected: a root for the whole home folder.
- **Where the type lives.** `pub struct ReadRoot { pub name: String, pub path: PathBuf }`, documented, in `crates/runtime/src/daemon.rs`, beside `SessionRegistration`, which gains `pub read_roots: Vec<ReadRoot>`. `read_roots` is in `orchestrator/session.rs` and re-exported `pub(crate) use session::read_roots;` from `orchestrator.rs`, since `mod session` is private (`orchestrator.rs:49`). `run_session` (`orchestrator/session.rs:165-183`) fills the registration, and `sandbox_for` (`orchestrator.rs:641-680`) reads the task's contract to compute the sandbox's.

### The hook reads them, and bounds a search

- **Order.** In `judge_call` (`crates/runtime/src/daemon/hooks.rs:319`), the new `in_read_root` branch runs beside `in_skill_folder` (`:345`), for `Read`, `Grep`, `Glob` and `LS` only. Their paths are taken as `workspace_paths` takes them (`:863-905`): `file_path` for `Read`, `path` for the others, `LS` included (`in_skill_folder` does not read `LS`, `:570-574`). They are resolved with `resolve` (`:920`), which canonicalises, so a link is judged by where it points.
  1. A path inside `cwd` is the existing path's: the branch does not take it, whatever root also holds it, since `cwd` lies under the `project` root.
  2. A path outside `cwd` inside a read root is the branch's.
  3. Anything else, and every other tool or write outside `cwd`, stays `path_outside_workspace` (`:930`).
- **The protected paths per root.** The team's `protected_paths` (the defaults at `team_rules.rs:7` and the team's own), plus `.git/**`, read relative to the root. A member's `.git/config` can hold a token in a remote's address, and no session needs git's internals through `Read`.
- **A single path.** `Read` of a file, and `LS` of a folder, is made relative to its root. An `LS` of the root's top, whose relative path is empty, is refused with the search's "its top" wording below. It is refused when `check_protected_paths` (`paths.rs:69`) holds the root-relative path, with `path_protected: <name>: <relative path> is protected, and no tool reads or writes it (5.12)`, the code being `refusal.rs:364`'s. The root's name is only in the message: `evaluate_tool_call` (`permissions.rs:383`) is given the root-relative path, so `.env` still matches its glob.
- **A search** (`Grep`, `Glob`) in a root is refused, with the same code and "<name>: searching <relative dir or 'its top'> could reach protected files", when its root-relative directory:
  - is empty, the root's top, where `.env`, `.git` and `.catervas/local` sit
  - or holds, or is held by, the literal folder part of any protected glob, before its first wildcard: `.catervas/local` for `.catervas/local/**`, `.git` for `.git/**`. This is tested with `reaches_the_folder(<dir>/**, <folder part>)` (`paths.rs:141`) in either direction.

  A protected glob with no literal folder part (`**/*.pem`, `**/*.key`) is guarded by layer two.
- **Layer two: Claude Code's own deny rules.** `settings_json` (`claude.rs:880`) gains, for each read root and each of its protected globs (the team's and `.git/**`, which `settings_json` adds per root itself), `Read(//<root path without its leading />/<glob>)`. That is Claude Code's absolute form, and Claude Code holds its search tools to it as well (`claude.rs:878-879`).
  - **A rule is left out when the glob has a literal folder part and the folder it names holds `cwd`.** The literal folder part is the glob's segments before the first segment holding a wildcard; for a file glob, its segments but the last. So `.catervas/local/**` has `.catervas/local`, while `.env`, `.env.*`, `**/*.pem` and `**/*.key` have none and their rules are always kept. Inside `cwd` the relative `Read(**/*.pem)` already refuses the same files. So the `project` root gives no `Read(//<project>/.catervas/local/**)`, which would cover every task's own worktree (`<project>/.catervas/local/worktrees/CTV-<n>`). Claude Code checks its deny rules whatever the hook answers, so such a rule would refuse the session its own files. Layer one guards `.catervas/local` and `.git` in a root.
  - `claude_args` (`:636`) builds the settings with `spec.read_roots`, `spec.cwd` and the protected globs in hand. A glob its `Read` rule cannot express is refused as today (`:866-875`).

### Claude, the sandbox and the prompt

- **Claude is told the roots.** `SessionSpec` (`crates/runtime/src/session.rs:99`) gains `read_roots: Vec<PathBuf>`. `claude_args` appends `--add-dir <path>` for each, where `--plugin-dir` is added (`:689`).
  - The flag is an assumption the code has not used before: that Claude Code accepts `--add-dir` with `-p` under `--setting-sources ""`, and that it lets a hook-allowed `Read` outside `cwd` through. Task 2 checks it live.
  - Claude Code may also offer skills found in an added folder's `.claude/skills/`. The hook's `catervas:` check (`hooks.rs`) still refuses calling one.
- **The sandbox mounts the other members read-only.** `SandboxFactory::create` (`sandbox.rs:81`) gains `reads: &[(String, PathBuf)]`: the task's read roots without `project`.
  - **Docker** (`sandbox/docker.rs:52-102`) adds `--mount type=bind,src=<path>,dst=/repos/<name>,readonly` for each, after the worktree's. A read path with a comma is refused with the worktree's error (`:61`). The arguments are built by a new pure `run_args(name, worktree, reads, network, user, labels, image) -> Result<Vec<String>, SandboxError>`, which `start` uses, so the mounts are unit-tested.
  - **`HostSandbox`** ignores them, since it isolates nothing.
  - **`create_base`** (the trait's, `sandbox.rs:99`; Docker's, `docker.rs:201`) mounts none.
  - Rejected: mounting the project folder too, whose documents the agent reads, not a command.
- **The prompt.** `PromptInput` (`crates/runtime/src/prompt.rs:20`) gains `members: &'a [MemberLine]` and `own_repo: Option<OwnRepo>`. `rules_section` (`:448`) becomes `rules_section(rules, role, (active_roles, stale_twins, members, own_repo))`, and appends after the stale line (`:479`), when `members` is not empty:

  `- repositories: <entries joined ", ">. <own sentence>Read the others; change only your own task's files.`

  - Each entry is `<name> (<path>)`, with `; /repos/<name> in your sandbox` before the `)` when `mounted`.
  - The task's own member is `<name> (your task's repository)`, with no path.
  - A missing member is `<name> (not found)`.
  - `<own sentence>` is `Your task works in <repo>. ` for a member, `Your task works in the project's own folder. ` for a task with no `repo`, and nothing for a session with no task.
  - `session_spec_without` (`orchestrator/session.rs:1138-1160`) fills it from `read_repos`, in `repos.yaml` order, all members.
  - `mounted` is true only for a session with a task and a sandbox, under Docker: `ProjectFiles::read_settings()`'s `LocalSettings.sandbox` is not `Sandbox::None` (written by `cli/src/setup.rs:109-115`).
  - A member whose path was left out of the roots (above) is `<name> (not readable here)`, and its `MemberLine.readable` is false.
  - The line is left out where `read_roots` would give none (the Marketing Specialist, a private-folder task, a one-repo project, an unreadable `repos.yaml`).

## File map

```
crates/runtime/src/daemon.rs                        modifies: ReadRoot, SessionRegistration.read_roots (Task 1)
crates/runtime/src/orchestrator/session.rs, orchestrator.rs   modifies, tests: read_roots and its re-export, run_session's roots (Task 1); the spec's roots (Task 2); the members line (Task 4)
crates/runtime/src/daemon/hooks.rs                  modifies, tests: in_read_root, its bounds (Task 1)
every SessionRegistration literal: daemon.rs, daemon/hooks.rs, daemon/mcp.rs, daemon/fixtures.rs, daemon/ads_calls/fixtures.rs, orchestrator/human.rs, orchestrator/rules.rs, crates/cli/tests/{connector_run,serving,live_claude,hook}.rs   modifies: read_roots: vec![] (Task 1)
crates/runtime/src/session.rs, crates/runtime/src/claude.rs   modifies, tests: SessionSpec.read_roots, --add-dir, the deny rules per root (Task 2)
every SessionSpec literal: recorded/fixtures.rs:29, sessions.rs:162, claude.rs:1025, tests/claude_process.rs:108, crates/cli/tests/live_claude.rs:262   modifies (Task 2)
crates/cli/tests/live_claude.rs                     tests: a live read of a root (Task 2)
crates/runtime/src/sandbox.rs, sandbox/docker.rs, sandbox/host.rs   modifies, tests: create's reads, run_args, the mounts (Task 3)
every SandboxFactory and create call: orchestrator/fixtures.rs:1099 (CountingSandboxFactory, which records reads), :1142, :1221, :1279; tests/new_tests.rs:75, :104; host.rs:339, :365; tests/docker_sandbox.rs:270, :311   modifies (Task 3)
crates/runtime/src/prompt.rs                        modifies, tests: MemberLine, OwnRepo, rules_section, Inputs::full (:663) (Task 4)
docs/SPEC.md                                        modifies (Task 5)
```

## Interfaces

Consumes:
- step 01's `read_repos`, `repo_present`
- step 02's `Harness::project_folder` and the contract's `repo`
- `ReadAccess`, `read_access`, `task_private_folder`
- `SessionRegistration`, `register_session`
- `judge_call`, `workspace_paths`, `resolve`, `in_skill_folder`, `outside`
- `check_protected_paths`, `reaches_the_folder`, `DEFAULT_PROTECTED_PATHS`, `evaluate_tool_call`
- `SessionSpec`, `claude_args`, `settings_json`
- `SandboxFactory`, `DockerSandbox`, `HostSandboxFactory`, `CountingSandboxFactory`
- `PromptInput`, `rules_section`, `Inputs::full`
- `TestDaemon::new` (`daemon/fixtures.rs:46`)
- `ProjectFiles::read_settings`, `LocalSettings`, `Sandbox`

Produces:

```rust
// catervas_runtime::daemon
pub struct ReadRoot { pub name: String, pub path: PathBuf }
// SessionRegistration gains `pub read_roots: Vec<ReadRoot>`
// catervas_runtime::orchestrator (re-exported from session.rs)
pub(crate) fn read_roots(files: &ProjectFiles, contract: Option<&TaskContract>, cwd: &Path, role: Role) -> Vec<ReadRoot>;
// SessionSpec gains `pub read_roots: Vec<PathBuf>`
// SandboxFactory::create gains `reads: &[(String, PathBuf)]`
pub(crate) fn run_args(name: &str, worktree: &Path, reads: &[(String, PathBuf)], network: bool, user: &str, labels: &[(String, String)], image: &str) -> Result<Vec<String>, SandboxError>;   // sandbox/docker.rs
// catervas_runtime::prompt
pub struct MemberLine { pub name: String, pub path: Option<String>, pub mounted: bool, pub own: bool, pub readable: bool }   // path None: not found
pub enum OwnRepo { Member(String), ProjectFolder }
// PromptInput gains `pub members: &'a [MemberLine]`, `pub own_repo: Option<OwnRepo>` (None: no task)
```

## Tasks

### Task 1: A session reads the other repositories, and no secret

Files: `daemon.rs`; `orchestrator/session.rs` and `orchestrator.rs`; `hooks.rs`; every `SessionRegistration` literal. The hook's tests use `TestDaemon::new` plus two `TempRepo`s, `infra` and `third`, and a project-folder path, a session registered with `cwd` the task's worktree under the project folder and `read_roots` `infra` and `project`. In `infra`, `src/main.rs`, `.env`, `src/key.pem` and `.git/` are present.

- `a_session_reads_the_other_members`:
  - `Read` of `<infra>/src/main.rs` passes
  - `Grep` with `path: <infra>/src`, `LS` of `<infra>/src`, and `Glob` with `path: <project>/docs/catervas` and pattern `**/*.md` each pass
  - `Read` of a file in the session's own worktree passes through the existing branch, unchanged

  RED: `path_outside_workspace`.
- `a_session_reaches_nothing_protected_in_a_root`. Each of these is refused, with its message naming the root:
  - `Read` of `<infra>/.env`, `<infra>/.git/config` and `<project>/.catervas/local/catervas.db`, each with `path_protected`
  - `Grep` with `path: <infra>`, and with `path: <project>` (the top)
  - `Grep` with `path: <project>/.catervas` (it holds `.catervas/local`)
  - `LS` of `<infra>/.git`, and `LS` of `<infra>` (its top)

  RED.
- `a_session_writes_only_its_own`. Each stays `path_outside_workspace`:
  - `Write` and `Edit` of `<infra>/src/main.rs`
  - `Read` in `third`, which is no root
  - an absolute `Glob` pattern under `<infra>`, refused as today

  RED: guards the read-only branch.
- `knows_which_folders_a_session_reads` (`orchestrator/session.rs`, `TestProject::project_folder` with `app` and `infra`):
  - the Architect with no contract, `cwd` the project root: `[app, infra]`
  - a Developer's task with `repo: app`, `cwd` its worktree: `[infra, project]`
  - a docs task with no `repo`: `[app, infra, project]`
  - the Marketing Specialist: `[]`
  - a Finance Specialist's private-folder task: `[]`
  - with `infra` moved away, the Developer's: `[project]`
  - with `repos.yaml` made unreadable: `[]`
  - in a one-repo project: `[]` for every role
  - with `infra` at a path holding `(`, or `*`, the Developer's: `[project]`

  RED.

- [x] `feat(runtime): let a session read the project's other repositories`

### Task 2: Claude is told the other folders and their protected files

Files: `session.rs`; `claude.rs`; every `SessionSpec` literal; `crates/cli/tests/live_claude.rs`.

- `adds_each_read_root_as_a_directory` (`claude.rs`): a spec with roots `/r/a` and `/r/b` gives `--add-dir /r/a` and `--add-dir /r/b`, in order, where `--plugin-dir` goes; none gives no `--add-dir`. RED.
- `denies_each_roots_protected_files` (`claude.rs`): with the team's protected `[.env, .catervas/local/**, **/*.pem]`, root `/r/a` and `cwd` `/p/.catervas/local/worktrees/CTV-1`, the deny list holds the relative `Read(.env)`, `Read(.catervas/local/**)` and `Read(**/*.pem)`, and for the root `Read(//r/a/.env)`, `Read(//r/a/.catervas/local/**)`, `Read(//r/a/**/*.pem)` and `Read(//r/a/.git/**)`. With root `/p` and the same `cwd`, there is no `Read(//p/.catervas/local/**)`, and `Read(//p/.env)` and `Read(//p/**/*.pem)` are there. RED.
- `a_live_session_reads_a_root` (`live_claude.rs`, ignored as the file's other live tests are): a session whose `cwd` is a worktree under a project folder's `.catervas/local/worktrees/`, given the project folder and a member as roots, reads a file in the member and a file in its own worktree, and is refused `<member>/.env`. RED.

- [x] `feat(runtime): tell Claude Code which other folders a session reads`

### Task 3: The sandbox mounts the other members read-only

Files: `sandbox.rs`; `sandbox/docker.rs` (`run_args`, used by `start`); `sandbox/host.rs`; `orchestrator.rs:671`; every factory and `create` call listed in the file map. `CountingSandboxFactory` records each call's `reads`.

- `mounts_each_other_member_read_only` (`docker.rs`): `run_args` with reads `[(infra, /home/me/infra)]` holds `--mount type=bind,src=/home/me/infra,dst=/repos/infra,readonly` after the worktree's `--mount`. A read path with a comma is the worktree's comma error. With no reads, the arguments equal today's. RED.
- `a_tasks_sandbox_holds_its_siblings` (`orchestrator.rs`, `Harness::project_folder` with `app` and `infra`): the sandbox made for a task with `repo: app` is given `[(infra, <infra>)]`, and for a docs task in the project folder `[(app, ..), (infra, ..)]`. RED.

- [ ] `feat(runtime): mount a task's sibling repositories read-only`

### Task 4: The prompt names the repositories

Files: `prompt.rs`; `orchestrator/session.rs` (`:1138-1160`).

- `the_team_rules_name_the_repositories` (`prompt.rs`), with members `app` (own), `infra` (path `/home/me/infra`, mounted) and `web` (path `None`), and `own_repo: Member("app")`, the Team rules end, after the stale line's place, with exactly:

  `- repositories: app (your task's repository), infra (/home/me/infra; /repos/infra in your sandbox), web (not found). Your task works in app. Read the others; change only your own task's files.`

  With `own_repo: None` and no `own`, the sentence `Your task works in …` is left out. With no members there is no line. RED.
- `a_developers_prompt_names_its_repositories` (`orchestrator/session.rs`, `Harness::project_folder`): the implement session's prompt of a task with `repo: app` holds the line with `app` as its own and `infra` mounted. Under no-sandbox mode `infra` has no `/repos/infra`. The Marketing Specialist's triage prompt holds no line. RED.

- [ ] `feat(runtime): tell each session where the project's repositories are`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.6: read roots and who gets them; the hook's read-only branch; a search's bounds; `--add-dir`
- 5.12: `.git/**` protected in a read root; the deny rules per root
- 6.5: the Marketing Specialist reads no member
- 8.2: the repositories line in the Team rules
- 8.3: the sandbox's read-only `/repos/<name>` mounts

- [ ] `docs(spec): record reading across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
