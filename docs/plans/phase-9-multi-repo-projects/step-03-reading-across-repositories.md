# Phase 9, step 03: Reading across repositories

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 6.5, 8.2, 8.3; F3
Depends on steps 01 and 02 of this phase, planned and ready, not yet executed:
- from step 01: `read_repos`, `Repos`, `Repo`, `repo_present`
- from step 02: the contract's `repo`, `Harness::project_folder`, `TestProject::project_folder`

Nothing of steps 02b, 02c or 04 to 08.
Mockups: none.
Readiness confirmed by: (pending; one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 47fdf28.

## Goal

In a project folder, every agent session but the Marketing Specialist's may read every member and the project folder, wherever its own working folder is:
- the Developer's task in `app` reads `infra`'s code and the Architect's documents
- the Architect's planning session at the project root reads every member

Writing is unchanged: a session writes only inside its own working folder, under its task's `allowed_paths`. A task's sandbox also mounts every other present member read-only at `/repos/<name>`, so a command can read a sibling. The session prompt lists the members, says where each is on the computer and in the sandbox, and names the task's own.

The Marketing Specialist keeps phase 8's limit. A one-repo project is unchanged.

Out of scope:
- each member's purpose in the prompt, read from `system.md` (step 05)
- the preview container's mounts
- members added while a task's sandbox is running, which are mounted only when the sandbox is next made

## Decisions

- **Read roots.** `SessionRegistration` (`crates/runtime/src/daemon.rs:106`) gains `read_roots: Vec<ReadRoot>`. Each root is a canonical folder a session may read outside its `cwd`. `run_session` (`orchestrator/session.rs:165-183`) fills it, for a session whose `read_access(role)` (`crates/core/src/folders.rs:403`) is `ReadAccess::Open`, in a project folder:
  - every present member except the one whose folder holds `cwd`, by name, at its canonical path
  - the project folder, named `project`, when `cwd` is not the project root

  The Marketing Specialist, a one-repo project, and a member that is not present get none. Rejected: one read root for the whole home folder, which would open every other repository on the computer.
- **The hook reads them, for reads only.** In `judge_call` (`crates/runtime/src/daemon/hooks.rs:319`), beside `in_skill_folder` (`:345`), a built-in `Read`, `Grep`, `Glob` or `LS` whose path, resolved as `workspace_paths` resolves it (`:884-891`, through `resolve`, `:920`), lies outside `cwd` and inside a read root is judged against that root (`in_read_root`):
  - its path is made relative to the root
  - it is checked against the protected paths: the team's `protected_paths`, the default `.env`, `.env.*`, `**/*.pem`, `**/*.key` and `.catervas/local/**` (`team_rules.rs:7`), read relative to the root. A refusal is today's `protected_path` with the root's name, e.g. "infra: .env is protected".
  - it passes `evaluate_tool_call` as a read, which `allowed_paths` never holds (`permissions.rs:394-401`)
  - `judge_reads` (`:417`) is not reached for it, since only `Open` sessions have roots

  Any other tool, and any write, outside `cwd` stays `path_outside_workspace` (`:930`). A `Glob` pattern that is absolute, has `..` or starts with `~` stays refused (`:864-871`): a `Glob` of a member names it in `path`, with a relative pattern.
  - Rejected: widening `workspace_paths` itself, which every write also passes through.
- **Claude is told the roots.** `SessionSpec` (`crates/runtime/src/session.rs:99`) gains `read_roots: Vec<PathBuf>`. `claude_args` (`claude.rs:636`) appends `--add-dir <path>` for each, after `--plugin-dir`'s place (`:689`), so that Claude Code's own directory check does not stand in front of the hook's answer. The `Read(<protected>)` deny rules in `settings_json` (`:880-902`) stay as they are, relative to `cwd`; the hook guards the roots' protected paths.
- **The sandbox mounts the other members read-only.** `SandboxFactory::create` (`sandbox.rs:81`) gains `reads: &[(String, PathBuf)]`, the session's read roots without `project`:
  - **Docker** (`sandbox/docker.rs:52-102`) adds one `--mount type=bind,src=<path>,dst=/repos/<name>,readonly` for each, after the worktree's. A path with a comma is refused as the worktree's is (`:61`).
  - **`HostSandbox`** ignores them: it isolates nothing, and the absolute paths are in the prompt.
  - **`create_base`** (`:99`) mounts none, since a base run checks the task's own tests.
  - **The caller** `orchestrator.rs:671` passes the task's read roots, computed as `run_session` computes them, from a shared `read_roots(files, cwd, role)`.
  - Rejected: mounting the project folder too. Its documents are read by the agent, not by a command.
- **The prompt says where the repositories are.** `PromptInput` (`crates/runtime/src/prompt.rs:20`) gains `members: &'a [MemberLine]`. `rules_section` (`:448`) appends, after the stale line (`:479`), when it is not empty:

  `- repositories: <name> (<path>; /repos/<name> in your sandbox), …. Your task works in <repo>.`

  - "in your sandbox" appears only for a member the sandbox mounts.
  - The last sentence is "Your task works in the project's own folder." for a task with no `repo`, and is left out for a session with no task.
  - The next sentence is "Read any of them; change only your own task's files."
  - `session_spec_without` (`orchestrator/session.rs:1138-1160`) fills it from `read_repos`, in `repos.yaml` order, the present members only. A missing member is named `<name> (not found)`.
  - The Marketing Specialist's sessions, and a one-repo project's, get no line.

## File map

```
crates/core/src/folders.rs                         none: ReadAccess is consumed as it is
crates/runtime/src/daemon.rs                       modifies: SessionRegistration.read_roots (Task 1)
crates/runtime/src/daemon/hooks.rs                 modifies, tests: in_read_root in judge_call (Task 1)
crates/runtime/src/session.rs, crates/runtime/src/claude.rs   modifies, tests: SessionSpec.read_roots, --add-dir (Task 2)
crates/runtime/src/sandbox.rs, sandbox/docker.rs, sandbox/host.rs   modifies, tests: create's reads, the readonly mounts (Task 3)
crates/runtime/src/orchestrator.rs                 modifies: sandbox_for passes the reads (Task 3)
crates/runtime/src/prompt.rs                       modifies, tests: PromptInput.members, the line (Task 4)
crates/runtime/src/orchestrator/session.rs         modifies, tests: ReadRoot, read_roots(), run_session's roots (Task 1); the spec's roots (Task 2); the members line filled (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

`ReadRoot` and `read_roots` live in `crates/runtime/src/orchestrator/session.rs`, beside `run_session`, `pub(crate)` so that `orchestrator.rs` and the hook's tests reach them.

## Interfaces

Consumes:
- step 01's `read_repos`, `repo_present`
- step 02's `Harness::project_folder` and the contract's `repo`
- `ReadAccess`, `read_access` (`folders.rs:393`, `:403`)
- `SessionRegistration`, `register_session` (`daemon.rs:106`, `:674`)
- `judge_call`, `workspace_paths`, `resolve`, `in_skill_folder`, `outside` (`hooks.rs:319`, `:863`, `:920`, `:565`, `:930`)
- `check_protected_paths` (`paths.rs:69`), `DEFAULT_PROTECTED_PATHS`
- `SessionSpec`, `claude_args`
- `SandboxFactory`, `DockerSandbox::create`, `HostSandboxFactory`
- `PromptInput`, `rules_section`, `Inputs::full` (`prompt.rs:662`)

Produces:

```rust
// catervas_runtime (orchestrator/session.rs)
pub(crate) struct ReadRoot { pub name: String, pub path: PathBuf }
pub(crate) fn read_roots(files: &ProjectFiles, cwd: &Path, role: Role) -> Vec<ReadRoot>;
// SessionRegistration gains `pub read_roots: Vec<ReadRoot>`; SessionSpec gains `pub read_roots: Vec<PathBuf>`
// SandboxFactory::create gains `reads: &[(String, PathBuf)]`
pub struct MemberLine { pub name: String, pub path: Option<String>, pub mounted: bool }   // path None: not found
// catervas_runtime::prompt: MemberLine and OwnRepo above; PromptInput gains `pub members: &'a [MemberLine]` and `pub own_repo: Option<OwnRepo>` (None: a session with no task)
pub enum OwnRepo { Member(String), ProjectFolder }
```

## Tasks

### Task 1: A session reads the other repositories

Files: `orchestrator/session.rs` (`ReadRoot`, `read_roots`, the registration's field); `daemon.rs` (the field); `hooks.rs` (`in_read_root`, its place in `judge_call`). Hook tests as the existing `path_outside_workspace` test (`hooks.rs:1943-1977`), on a `TestDaemon` whose project is a project folder with members `app` and `infra`, a session registered with `cwd` the task's worktree in `app` and `read_roots` `infra` and `project`.

- `a_session_reads_the_other_members`: `Read` of `<infra>/src/main.rs` passes. `Grep` with `path: <infra>` and `Glob` with `path: <project>/docs/catervas`, pattern `**/*.md`, pass. `Read` of `<infra>/.env` is refused with `protected_path` naming `infra`, and of `<project>/.catervas/local/catervas.db` likewise naming `project`. RED: `path_outside_workspace`.
- `a_session_writes_only_its_own`: `Write` and `Edit` of `<infra>/src/main.rs`, and `Read` of a file in a third repository that is no root, are each `path_outside_workspace`. An absolute `Glob` pattern under `<infra>` is refused as today. RED: guards the read-only branch.
- `the_marketing_specialist_reads_no_member` (`orchestrator/session.rs`): `read_roots` for the Marketing Specialist in a project folder is empty. For the Architect at the project root it is `app` and `infra`. For a Developer in `app`'s worktree it is `infra` and `project`. With `infra` moved away it is `project` alone. In a one-repo project it is empty for every role. RED.

- [ ] `feat(runtime): let a session read the project's other repositories`

### Task 2: Claude is told the other folders

Files: `session.rs` (`SessionSpec.read_roots`); `claude.rs` (`claude_args`); `orchestrator/session.rs` (filling the spec).

- `adds_each_read_root_as_a_directory` (`claude.rs`): a spec with two read roots gives `--add-dir <a>` and `--add-dir <b>`, in order, after the plugin directory's place; a spec with none gives no `--add-dir`. RED.

- [ ] `feat(runtime): tell Claude Code which other folders a session reads`

### Task 3: The sandbox mounts the other members read-only

Files: `sandbox.rs` (the argument); `sandbox/docker.rs` (the mounts); `sandbox/host.rs` (ignored); `orchestrator.rs:671` (passing them); every `SandboxFactory` fake in tests (the argument added).

- `mounts_each_other_member_read_only` (`sandbox/docker.rs`, the existing docker-argument tests' way): `create` with reads `[(infra, /home/me/infra)]` runs `docker run` with `--mount type=bind,src=/home/me/infra,dst=/repos/infra,readonly` after the worktree's mount. A read path with a comma is refused with the worktree's error. RED.
- `a_tasks_sandbox_holds_its_siblings` (`orchestrator.rs`, `Harness::project_folder` with the fake factory recording its calls): the sandbox made for a task in `app` is given `[(infra, <infra>)]`, and for a docs task in the project folder `[(app, ..), (infra, ..)]`. RED.

- [ ] `feat(runtime): mount a task's sibling repositories read-only`

### Task 4: The prompt names the repositories

Files: `prompt.rs` (`MemberLine`, `OwnRepo`, the line; `Inputs::full` and the literal at `:680` passing `&[]` and `None`); `orchestrator/session.rs` (`:1138-1160`).

- `the_team_rules_name_the_repositories` (`prompt.rs`): with members `app` (mounted: false), `infra` (mounted: true, path `/home/me/infra`) and `web` (path `None`), and `own_repo: Member("app")`, the Team rules end, after the stale line's place, with exactly: `- repositories: app (/home/me/app), infra (/home/me/infra; /repos/infra in your sandbox), web (not found). Your task works in app. Read any of them; change only your own task's files.` With `ProjectFolder` the second sentence is `Your task works in the project's own folder.`. With no members there is no line. RED.
- `a_developers_prompt_names_its_repositories` (`orchestrator/session.rs`, `Harness::project_folder`): the implement session's prompt of a task with `repo: app` holds the line with `app` and `infra`, `infra` mounted. The Marketing Specialist's triage prompt holds none. RED.

- [ ] `feat(runtime): tell each session where the project's repositories are`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.6: read roots, the hook's read-only branch, the protected paths read per root, `--add-dir`
- 6.5: the Marketing Specialist reads no member
- 8.2: the repositories line in the Team rules
- 8.3: the sandbox's read-only `/repos/<name>` mounts

- [ ] `docs(spec): record reading across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
