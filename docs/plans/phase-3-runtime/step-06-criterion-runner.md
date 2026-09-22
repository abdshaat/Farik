# Phase 3, step 06: Criterion runner

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.4, 5.13, F16; `task-contract.schema.json` (`verification`)
Depends on: step 02 of this phase (`Executor`, `SandboxFactory`); phase 2 (`Git`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the two decisions the first found open; findings folded in)

## Goal

A contract's exit criteria can be run by Farik rather than described: a `command` criterion runs and is judged by its exit code and output, a `test` criterion runs its command and, when it asks for new tests, is judged also on whether the diff adds a test that fails on the base branch, and an `artifact` criterion is judged by the file's presence and content. `review` and `human` criteria come back as the rubric or the question someone has to answer. Each result carries evidence a reviewer and the human can read. Out of scope: who runs them and records the results (the orchestrator's verify session, step 11) and the tools an agent uses to record its own (step 05).

## Decisions

- `run_criterion(criterion, executor, run_by, timeout)` runs one criterion in the executor's workspace root (`cwd` `""`: a criterion's command is written against the project root, as the scan seeds them); the clock and `cwd` parameters revision 8 sketched are cut, because nothing in a result is timed and a criterion has no directory of its own. `run_by` is stamped on the `CriterionResult`, since the same criterion is run by the assignee and by the reviewer (5.4).
- The timeout is 900 s per criterion, a constant `CRITERION_TIMEOUT` the caller passes, so that a test suite that hangs is a failed criterion, not a stuck verify session.
- `command`: passed when the exit code equals `exit_code` and, when set, stdout contains `stdout_contains` and does not contain `stdout_not_contains`. A timed-out run fails whatever its exit code.
- `test`: passed when the command exits 0 and did not time out; with `new_tests_required`, also only when `check_new_tests` says the diff adds a test and that test fails on the base branch. `run_criterion` alone cannot know the diff, so a `test` criterion with `new_tests_required` given to it without a `NewTestsInput` fails with evidence saying the base-branch check was not run; `run_criteria` passes one.
- A test file, for "adds a test": a path the new `Git::added_or_modified_paths(base, head)` returns (`git diff --no-renames --name-only --diff-filter=AM -z base...head`, so a renamed test counts as added, a known ceiling) with a segment `tests`, `test`, `__tests__`, or `spec`, or a file name matching `test_*`, `*_test.*`, `*.test.*`, `*.spec.*`, or `*_spec.*`. Unit tests inside a source file (Rust's `#[cfg(test)]` modules) are not separable from the code they test and do not count; recorded as the known ceiling, with the upgrade being a per-language hunk filter.
- "Fails on the base branch": a detached worktree (the new `Git::create_detached_worktree(path, at)`, `git worktree add --detach`) at the merge base of `base` and `head` (the new `Git::merge_base`, the same commit `base...head` diffs from), at `<git.root()>/.farik/local/worktrees/<id>-base`; a leftover one from a crash is removed first (`remove_worktree`, a "not a working tree" refusal ignored, and then the directory itself deleted if it is still there, which covers one git no longer has registered); the head's version of each test file written into it (the new `Git::file_at(rev, path)`, `git show <rev>:<path>`); the test command run there with `CRITERION_TIMEOUT` in a sandbox from the new `SandboxFactory::create_base(project_id, task_id, worktree)`, which Docker names `farik-<project>-<task>-base` with the network off, so it does not collide with the task's own container, which the verify session holds; then the sandbox discarded and the worktree removed on every path out, including an error. When the run and the cleanup both fail, the first error is returned, the cleanup having still been tried. Fails means a non-zero exit or a timeout. Chose a second factory method over a free-form name, because the base run is the one other sandbox Farik makes for a task, and over taking an executor from the caller, because the worktree is made here.
- `artifact`: the path, normalised as step 02 normalises a `cwd` (absolute, climbing out, or the root itself fail the criterion with evidence saying so, and nothing runs), is read through the executor with `cat -- '<path>'` (single-quoted, a `'` inside written `'\''`), so the same code reads a host worktree and a container's; passed when it exits 0, the output was not cut at the executor's 1 MiB (a larger file fails with `file larger than 1 MiB`), and the output contains every `must_contain` string. `run_criteria` uses `CRITERION_TIMEOUT` for every run, including the artifact read and the base run.
- `review` and `human` are `NeedsReview { rubric }` and `NeedsHuman { question }`, never results: a person or an agent answers them and records the answer (5.4).
- Evidence is lines of text, in this order: `$ <command>`; `exit <n>` or `timed out`; for each expectation set, `stdout contains "<s>": yes` or `no`, `stdout does not contain "<s>": yes` or `no`, `contains "<s>": yes` or `no` (artifacts); for new tests, `adds tests: yes|no`, `fails on base: yes|no`, `test files: <comma-separated>`; then `stdout (last 2000 bytes):` and the tail, and `stderr (last 2000 bytes):` and its tail, each cut on a character boundary. Tests assert whole lines. Chose the tail over the head because a test runner's summary is at the end.
- A criterion's method comes from `Verification::from(&criterion.verification)` and its id from `criterion.id`.
- `CriterionError { Exec(ExecError), Git(GitError), Sandbox(SandboxError) }`, hand-written `Display` (ADR 0006); a criterion that fails is a result, not an error.

## File map

```
crates/runtime/src/criteria.rs              creates: CriterionOutcome, CriterionError, NewTestsInput, NewTestsCheck, run_criterion, run_criteria, check_new_tests, CRITERION_TIMEOUT; tests in `mod tests`
crates/runtime/tests/new_tests.rs           creates: check_new_tests on a real repository, ignored (needs git)
crates/runtime/src/lib.rs                   modifies: `pub mod criteria;`
crates/store/src/git.rs                     modifies: merge_base, create_detached_worktree, file_at, added_or_modified_paths
crates/store/tests/git.rs                   modifies: their tests
crates/runtime/src/sandbox.rs, sandbox/host.rs, sandbox/docker.rs   modifies: SandboxFactory::create_base
docs/plans/project-plan.md                  modifies: step 06's interface line
```

## Interfaces

Consumes: `Executor`, `ExecResult`, `ExecError`, `SandboxFactory`, `Sandbox`, `SandboxError` (step 02); `Git`, `GitError` (on main); `ExitCriterion`, `Verification`, `TaskContract`, `CriterionResult`, `RunBy`, `TaskId` (`farik-core`).

Produces:

```rust
pub const CRITERION_TIMEOUT: Duration = Duration::from_secs(900);
pub enum CriterionOutcome { Result(CriterionResult), NeedsReview { rubric: Vec<String> }, NeedsHuman { question: String } }
pub enum CriterionError { Exec(ExecError), Git(GitError), Sandbox(SandboxError) }
pub struct NewTestsCheck { pub adds_tests: bool, pub fails_on_base: bool, pub test_files: Vec<String> }
pub struct NewTestsInput<'a> { pub git: &'a Git, pub base: &'a str, pub head: &'a str, pub sandboxes: &'a dyn SandboxFactory, pub project_id: &'a str, pub task_id: &'a TaskId }
pub fn run_criterion(criterion: &ExitCriterion, executor: &dyn Executor, run_by: RunBy, timeout: Duration) -> Result<CriterionOutcome, CriterionError>;
pub fn run_criteria(contract: &TaskContract, executor: &dyn Executor, run_by: RunBy, new_tests: Option<&NewTestsInput<'_>>) -> Result<Vec<CriterionOutcome>, CriterionError>;   // in the contract's order
pub fn check_new_tests(command: &str, input: &NewTestsInput<'_>) -> Result<NewTestsCheck, CriterionError>;
pub fn is_test_file(path: &str) -> bool;
// farik-store
impl Git { pub fn merge_base(&self, a: &str, b: &str) -> Result<String, GitError>; pub fn create_detached_worktree(&self, path: &Path, at: &str) -> Result<(), GitError>; pub fn file_at(&self, rev: &str, path: &str) -> Result<String, GitError>; pub fn added_or_modified_paths(&self, base: &str, head: &str) -> Result<Vec<String>, GitError>; }
// step 02's trait, widened
trait SandboxFactory { ..; fn create_base(&self, project_id: &str, task_id: &TaskId, worktree: &Path) -> Result<Box<dyn Sandbox>, SandboxError>; }
```

## Tasks

### Task 1: command, artifact, review, and human criteria

Files: created `crates/runtime/src/criteria.rs`; modified `crates/runtime/src/lib.rs`; tested in `criteria.rs` on a `HostSandbox` in a temporary directory

- `passes_a_command_that_exits_as_expected_with_the_output_asked_for` — `printf ok` expecting 0 and `stdout_contains: ok`: passed, `run_by` as given, evidence has the lines `$ printf ok`, `exit 0`, `stdout contains "ok": yes`.
- `fails_a_command_whose_output_lacks_what_it_must_contain` — the same expecting `stdout_contains: done`: not passed, evidence has the line `stdout contains "done": no`.
- `fails_a_command_whose_output_holds_what_it_must_not` — `printf error` with `stdout_not_contains: error`: not passed.
- `fails_a_command_that_times_out_whatever_its_code` — `sleep 5` expecting 137 with a 1 s timeout: not passed, evidence has the line `timed out`.
- `passes_an_artifact_that_holds_every_string` and `fails_an_artifact_that_is_missing` — a file holding `alpha beta` with `must_contain: [alpha, beta]` passes; an absent path fails with evidence naming it.
- `fails_an_artifact_outside_the_workspace` — `../etc/passwd`, `/etc/passwd`, and `.`: each not passed with evidence saying why, and a marker command in the workspace shows nothing ran (a counting fake `Executor` records no `run`).
- `reads_an_artifact_whose_name_has_a_quote` — `it's.txt` is read and passes.
- `asks_for_a_review_or_a_human_instead_of_running` — `review` gives `NeedsReview` with its rubric, `human` `NeedsHuman` with its question.
- `keeps_the_last_two_thousand_bytes_of_output` — output of 5,000 `a`s then `END`: the evidence ends with `END` and holds at most 2,000 bytes of stdout.

- [ ] `feat(runtime): run command and artifact criteria and ask for the rest`

### Task 2: test criteria and new tests

Files: modified `crates/runtime/src/criteria.rs`, `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `crates/runtime/src/sandbox.rs`, `sandbox/host.rs`, `sandbox/docker.rs`, `docs/plans/project-plan.md` (step 02's and step 06's interface lines); created `crates/runtime/tests/new_tests.rs`, `#[ignore = "needs the git program: cargo xtask check --integration"]`, on `git::fixtures::TempRepo` and `HostSandboxFactory`

- `names_test_files_by_their_path` (unit) — `tests/a.rs`, `src/__tests__/x.js`, `pkg/test_util.py`, `a_test.go`, `b.test.ts`, `c.spec.js`, `d_spec.rb` are test files; `src/lib.rs`, `latest.txt`, `contest/x.rs` are not.
- `passes_a_test_criterion_that_exits_zero` and `fails_one_that_does_not` (unit).
- `fails_a_new_tests_criterion_run_without_the_diff` (unit) — evidence says the base-branch check was not run.
- The fixture: `main` holds `src/x.sh` (`echo 1`) and `run_tests.sh` (`for t in tests/*.sh; do [ -e "$t" ] || continue; sh "$t" || exit 1; done`); head is the branch `farik/FRK-1` from `main`; the command is `sh run_tests.sh`.
- `finds_a_new_test_that_fails_on_the_base_branch` — head changes `src/x.sh` to `echo 2` and adds `tests/new.sh` (`[ "$(sh src/x.sh)" = 2 ]`): `adds_tests`, `fails_on_base`, `test_files == ["tests/new.sh"]`.
- `finds_no_new_test_when_only_code_changed` — head changes `src/x.sh` only: `adds_tests == false`, and a counting factory saw no `create_base`.
- `says_a_new_test_that_passes_on_base_does_not_count` — `tests/new.sh` is `exit 0`: `fails_on_base == false`, and the criterion through `run_criteria` is not passed.
- `removes_the_base_worktree_whatever_happens` — after each of the three above, `.farik/local/worktrees/<id>-base` is gone and `git worktree list` does not name it; with a factory whose `create_base` answers `Err(SandboxError::DockerUnavailable)`, `check_new_tests` is `Err(CriterionError::Sandbox(DockerUnavailable))` and the worktree is gone too.
- `replaces_a_base_worktree_left_by_a_crash` — with a registered worktree already at that path, and separately with a plain directory git does not know, the check runs and cleans up.
- Store tests (`crates/store/tests/git.rs`, ignored): `makes_a_detached_worktree_at_a_commit`, `reads_a_file_at_a_revision`, `lists_added_and_modified_paths_but_not_deleted_ones`, `finds_the_merge_base_of_two_branches`.

- [ ] `feat(runtime): run test criteria and check that new tests fail on the base branch`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
