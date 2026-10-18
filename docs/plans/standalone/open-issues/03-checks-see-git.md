# Standalone (open issues), plan 3: the checks Catervas runs can read git

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.4, 8.3 (recorded in plan 8's revision 0.122)
Depends on: plan 2 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A `command` or `test` criterion that runs git (`git check-ignore -q package-lock.json`,
`git ls-files --error-unmatch …`) works when Catervas runs it in Docker's sandbox: the container
made for the checks mounts the worktree's git store read-only at its own absolute path, so the
worktree's `.git` file resolves. No agent session ever gets that mount, and a repository whose git
settings hold a password gets none. Out of scope: the base container of a `new tests` criterion,
and no-sandbox mode (the host has git already).

## Decisions

Every decision is ADR 0069's (decision 6); this section pins how.

- **Which store.** `Git::common_dir(worktree)` runs `git rev-parse --path-format=absolute
  --git-common-dir` in the worktree (the `COMMON_DIRECTORY` arguments `git.rs` already holds) after
  `require_worktree_of_this_repository`, and answers the path. A member's task gets its member's
  store, because `run_what_catervas_runs` asks the task's repository (`task_repository_of`).
- **A password keeps git out.** `Git::config_holds_a_secret()` reads `git config --get-regexp
  '^remote\..*\.url$'` and `git config --get-regexp '^http\..*extraheader$'` at the root (exit 1,
  nothing set, is `false`). It is `true` when any remote URL has a user part (text between `://`
  and the next `/` holding `@`, checked by `fn url_has_user_part(url: &str) -> bool`) or any
  `extraheader` is set. Then the checks run with no mount, as today, and git in them fails as it
  did; the escalation or rejection that follows says so in git's own words.
- **Mounted read-only at its own path**: `--mount type=bind,src=<store>,dst=<store>,readonly`, after
  the read-only member mounts, with the same refusal of a relative path or a comma as the worktree's
  (`refuses_a_worktree_the_mount_cannot_name`). Read-only: a check may read refs, the index and
  objects, and cannot write a hook, a config, a ref or a lock that the host's git would run or read
  next. The image already has git (`crates/runtime/sandbox/Dockerfile`).
- **Only for checks.** `SandboxFactory` gains `create_checking`, whose default calls `create` (the
  host sandbox has git; the fakes need nothing). `DockerSandboxFactory` overrides it. The
  orchestrator gains `sandbox_for_checks`, which builds the task's sandbox as `sandbox_for` does,
  through `create_checking` when it has a store, and holds it in the same slot.
  `run_what_catervas_runs` already forgets the task's sandbox before its checks; it now forgets it
  after them too, so the assignee's or reviewer's next session gets a container with no store.
- **Network unchanged**: the checks' container keeps the network the task's sandbox has (ADR 0061,
  research is free). That is why a store with a secret is never mounted.
- Rejected: running git criteria on the host (a criterion is any shell text an agent wrote); a
  writable mount (hooks and `core.fsmonitor` run on the host); `GIT_DIR` in each exec (the store's
  own `worktrees/<id>/gitdir` names the host path anyway); refusing git criteria at readiness (the
  founder's checks are real checks).

## File map

```
crates/store/src/git.rs                    modifies: Git::common_dir, Git::config_holds_a_secret, url_has_user_part (Task 9)
crates/store/tests/git.rs                  tests:    finds_a_worktrees_git_store_and_a_password_in_its_settings (Task 9)
crates/runtime/src/sandbox.rs              modifies: SandboxFactory::create_checking with its default (Task 10)
crates/runtime/src/sandbox/docker.rs       modifies: run_args(.., git_store), create_checking (Task 10)
crates/runtime/src/sandbox/docker.rs       tests:    mounts_each_other_member_read_only_after_the_worktree amended (Task 10)
crates/runtime/src/orchestrator.rs         modifies: sandbox_for_checks (Task 10)
crates/runtime/src/orchestrator/verify.rs  modifies: run_what_catervas_runs finds the store and forgets the sandbox after (Task 10)
crates/runtime/tests/docker_sandbox.rs     tests:    a_check_reads_git_and_an_agent_does_not (Task 10)
```

## Interfaces

Consumes (on this branch): `Git`, `COMMON_DIRECTORY`, `require_worktree_of_this_repository`,
`run_git` (`git.rs`); `SandboxFactory`, `Sandbox`, `SandboxError` (`sandbox.rs`); `run_args`,
`DockerSandboxFactory`, `DockerSandbox::start` (`sandbox/docker.rs`); `Orchestrator::sandbox_for`,
`forget_sandbox`, `worktree` (`orchestrator.rs`); `run_what_catervas_runs`, `task_repository_of`
(`verify.rs`); `TempRepo` and the worktree helpers of `crates/store/tests/git.rs`.

Produces:

```rust
// crates/store/src/git.rs
impl Git {
    /// The git store a worktree of this repository uses, as an absolute path.
    pub fn common_dir(&self, worktree: &Path) -> Result<PathBuf, GitError>;
    /// Whether the repository's settings hold a password: a remote URL with a user part, or an `http.*.extraheader`.
    pub fn config_holds_a_secret(&self) -> Result<bool, GitError>;
}
fn url_has_user_part(url: &str) -> bool;
// crates/runtime/src/sandbox.rs
pub trait SandboxFactory {
    /// A sandbox for Catervas's checks, which also sees `git_store` read-only at its own path (ADR 0069).
    fn create_checking(&self, project_id: &str, task_id: &TaskId, worktree: &Path, network: bool,
                       reads: &[(String, PathBuf)], git_store: &Path) -> Result<Box<dyn Sandbox>, SandboxError> {
        let _ = git_store;
        self.create(project_id, task_id, worktree, network, reads)
    }
}
// crates/runtime/src/sandbox/docker.rs
pub(crate) fn run_args(name: &str, worktree: &Path, reads: &[(String, PathBuf)], git_store: Option<&Path>,
                       network: bool, user: &str, labels: &[(String, String)], image: &str) -> Result<Vec<String>, SandboxError>;
// crates/runtime/src/orchestrator.rs
fn sandbox_for_checks(&self, task_id: &TaskId, team: &Team, git_store: Option<&Path>) -> Result<Arc<dyn Sandbox>, OrchestratorError>;
```

## Tasks

### Task 9: the store finds a worktree's git store and a password in its settings

Files: modified `crates/store/src/git.rs`; tested by `crates/store/tests/git.rs`
Produces: `common_dir`, `config_holds_a_secret`, `url_has_user_part`
Consumes: `COMMON_DIRECTORY`, `run_git`

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `finds_a_worktrees_git_store_and_a_password_in_its_settings` (ignored "needs the git program") —
  in a `TempRepo` with a worktree at `.catervas/local/worktrees/CTV-1` made by `create_worktree`:
  `common_dir` of it equals the canonical `<repo>/.git`; `config_holds_a_secret` is `false` with no
  remote, `true` after `git remote add origin https://someone:tok3n@github.com/a/b.git`, `false`
  after `git remote set-url origin https://github.com/a/b.git` and with
  `git@github.com:a/b.git`, and `true` after `git config http.https://github.com/.extraheader
  "AUTHORIZATION: basic eA=="`. Fails first: the methods do not exist (write the test against the
  signatures above).

- [ ] `feat(store): find a worktree's git store and whether its settings hold a password`

### Task 10: the checks' container mounts it read-only

Files: modified `sandbox.rs`, `sandbox/docker.rs`, `orchestrator.rs`, `orchestrator/verify.rs`;
tested by `sandbox/docker.rs`'s tests and `crates/runtime/tests/docker_sandbox.rs`
Produces: `create_checking`, `run_args`'s `git_store`, `sandbox_for_checks`
Consumes: Task 9's methods

`DockerSandboxFactory::create` passes `None`; `create_checking` passes `Some(git_store)`.
`run_what_catervas_runs`, after it has the task's repository, computes
`let git_store = if repository.git.config_holds_a_secret()? { None } else {
Some(repository.git.common_dir(&worktree(deps, &contract.id))?) };`, calls
`orchestrator.sandbox_for_checks(&contract.id, team, git_store.as_deref())` in place of
`sandbox_for` in its loop, and calls `orchestrator.forget_sandbox(&contract.id)` once the loop ends,
on every path out of it (a failing criterion included). The module doc of `docker.rs` adds "the
checks' container also sees the repository's git store, read-only".

Tests:

- `mounts_each_other_member_read_only_after_the_worktree` (amended) — with `git_store:
  Some("/home/u/p/.git")` the arguments hold
  `type=bind,src=/home/u/p/.git,dst=/home/u/p/.git,readonly` as the last `--mount`, after the members;
  with `None` the mounts are as before.
- `a_check_reads_git_and_an_agent_does_not` (`tests/docker_sandbox.rs`, ignored "needs docker and
  the git program") — a real repository with one committed `README.md` and a linked worktree made
  by `Git::create_worktree`: in the container from `create_checking(.., common_dir)`, `git ls-files
  --error-unmatch README.md` exits 0, `git check-ignore -q README.md` exits 1, and
  `git commit --allow-empty -m x` exits non-zero with the store unchanged (its `HEAD` sha the same
  after); in the container from `create` for the same worktree, `git status` exits 128 and its
  standard error contains "not a git repository". Fails first: the first command exits 128.

- [ ] `fix(runtime): the checks catervas runs read the repository's git store`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with tests/docker_sandbox.rs and the store's git tests run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
