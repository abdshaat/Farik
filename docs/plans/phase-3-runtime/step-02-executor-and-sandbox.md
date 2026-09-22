# Phase 3, step 02: Executor and sandbox

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` section 8.3; ADR 0004
Depends on: step 01 of this phase (committed as the three `feat(runtime)` commits before this plan)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (one round; its findings folded in)

## Goal

A command an agent asks to run has somewhere to run that is not the agent's session: a Docker container per task with the task's worktree at `/workspace` and the network off unless the role has `network`, or, in no-sandbox mode, the host, inside the worktree. Either way it gets a timeout that stops it, bounded output, and an environment that never holds Farik's credentials. Out of scope: which commands are allowed (`evaluate_command`, called by `farik_exec` in step 05), building the image (step 13's `farik run`), and choosing the sandbox from settings (step 11).

## Decisions

- One `Executor` trait, implemented by the two sandboxes; there is no separate `HostExecutor`, because every command Farik runs belongs to a task's worktree and `HostSandbox` is exactly a host executor rooted there (revision 8 listed both; this is the cut).
- `run`'s `cwd` is relative to the sandbox's workspace root (the worktree on the host, `/workspace` in the container); `""` and `"."` are the root. A `cwd` that is empty or only `.` and `/` segments is the root; any other is judged by `farik_core::governor::paths::normalise`, and `None` (absolute, or climbing out) is `ExecError::OutsideWorkspace`, so "does this path leave the workspace" has one definition. The check is lexical, not a boundary: on the host nothing isolates a command anyway, and in the container the mount is the boundary. The project plan's step 02 and step 06 interfaces are amended in Task 1 (`cwd: &str`, no `HostExecutor`), and step 06's base-branch run gets its own sandbox from a `SandboxFactory` for the temporary worktree, since a sandbox rooted at the task's worktree cannot reach it. Chose relative over the host path revision 8's `&Path` implied because the host path means nothing inside a container, and an agent names directories of its project, not of the machine.
- A command is one string run by `sh -c`, on the host and in the container. Host sandboxes are Unix-only in this phase (`#[cfg(unix)]`); the desktop phase decides Windows.
- Output: each of stdout and stderr keeps its first 1 MiB and the rest is read and discarded, with `ExecResult::truncated` set; each stream becomes a `String` through `String::from_utf8_lossy`, so a cut through a multi-byte character costs a replacement character, not an error. Both pipes are drained on their own threads while the process runs, because a child that fills a pipe nobody reads blocks, and the timeout would then report a hang that is Farik's. `farik_exec`'s 64 KiB cap for the model (step 05) is separate and smaller.
- Timeout on the host: the child runs in its own process group (`CommandExt::process_group(0)`), the loop polls `try_wait` every 20 ms, and at the deadline the group is killed through the shell builtin, `sh -c 'kill -KILL -- -<pgid>'` (no `unsafe`, so no `libc`, and `/bin/kill` may be absent). When `sh` exits before the deadline the group is killed too, so a background child still holding a pipe cannot keep the readers waiting; then the readers are joined. `timed_out` is true exactly when the deadline killed it. A command killed by a signal reports `exit_code` 128 plus the signal (`ExitStatusExt::signal`), so a deadline kill is 137. `run` blocks for up to its timeout; async callers (step 05) call it through `spawn_blocking`.
- Timeout in the container: the command runs as `timeout -k 2 <secs> sh -c <command>` inside it, `<secs>` being the timeout rounded up to whole seconds and at least 1, because killing the `docker exec` client leaves the process running in the container; the host loop kills the client 5 s after the deadline in case the container stops answering. `timed_out` is read from the host clock alone: the deadline passed before `docker exec` returned. Busybox and GNU `timeout` exit with different codes, so the code is reported as it is, not interpreted. Busybox signals only its direct child, so a command's background children can outlive a container timeout; `--init` reaps them once they exit, and `discard` ends them. Recorded as the one asymmetry with the host.
- Environment: the host sandbox clears the inherited environment and passes `PATH`, `HOME`, `LANG`, and `TMPDIR` when set, plus the `env` given; the container gets only the `env` given (`docker exec -e`), on top of the image's own. So a command never sees `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, a git credential, or anything else in the user's shell (spec 8.6).
- Docker is the `docker` program through `std::process`, as git is (revision 8). `create` runs `docker run -d --init --name farik-<project>-<task_id> --network none|bridge --mount type=bind,src=<worktree>,dst=/workspace -w /workspace --user <uid>:<gid> --label farik.project=<project_id> --label farik.task=<task_id> <image> sleep infinity`, where `<project>` and `<task_id>` are lowercased with every character outside `[a-z0-9_.-]` turned into `-` (Docker's name rule; the event schema only asks that the id be non-blank), and `--mount` rather than `-v` because a path holding `:` breaks `-v`; the uid and gid come from `id -u` and `id -g`, so that files the command writes in the worktree belong to the user rather than root. A container of that name already there is removed first (`docker rm -f`), because the worktree is the state and the container is disposable; recovery (step 11) relies on this.
- Refusals: `docker version` failing, or the program missing, is `SandboxError::DockerUnavailable`; `docker image inspect <image>` failing is `ImageMissing { image }`, and nothing is pulled or built here; any other `docker run` failure is `ContainerFailed { detail }` with docker's stderr. A `docker exec` answered with `No such container` or `is not running` is `ExecError::ContainerGone`; the process not starting at all is `ExecError::SpawnFailed`.
- `discard` is `docker rm -f` for the container and nothing for the host.
- `SANDBOX_IMAGE` is `farik/sandbox:<crate version>` from `env!("CARGO_PKG_VERSION")`. The `Dockerfile` at `crates/runtime/sandbox/Dockerfile` installs Node 24, pnpm, npm, python3, git, coreutils, and the Rust toolchain pinned in `rust-toolchain.toml`; it ships and is not built by CI (revision 8). The container runs as the user's uid, which has no passwd entry, so the image sets `HOME=/tmp`, puts `CARGO_HOME` and `RUSTUP_HOME` under `/usr/local` and makes both world-writable, so that `cargo`, `npm`, and `pnpm` can write their caches.
- Tests that need Docker are `#[ignore = "needs docker"]` and run under `cargo xtask check --integration`, on `alpine:3.22` (its busybox has `timeout -k`), which CI pulls before the check (`docker pull alpine:3.22` in the workflow), since `create` never pulls; the host sandbox's tests need only `sh` and a temporary directory and run by default. CI stays one job: the Docker tests only add ignored tests to the same `cargo test`, which is the reason the job is one for git, and the workflow's comment that predicted a second job here is corrected.
- `docs/SPEC.md` 8.3's sentence that git operations happen inside the container contradicts ADR 0004 (git runs on the host through `farik_git`) and is corrected in Task 2.
- `SandboxError` and `ExecError` have hand-written `Display` and `Error` (ADR 0006).

## File map

```
crates/runtime/Cargo.toml                 modifies: nothing new; `std` only
crates/runtime/src/lib.rs                 modifies: `pub mod exec; pub mod sandbox;` and re-exports
crates/runtime/src/exec.rs                creates: ExecResult, ExecError, Executor, the pipe draining and deadline loop; tests in `mod tests`
crates/runtime/src/sandbox.rs             creates: SandboxError, Sandbox, SandboxFactory, SANDBOX_IMAGE
crates/runtime/src/sandbox/host.rs        creates: HostSandbox, HostSandboxFactory; tests in `mod tests`
crates/runtime/src/sandbox/docker.rs      creates: DockerSandbox, DockerSandboxFactory
crates/runtime/sandbox/Dockerfile         creates: the image
crates/runtime/tests/docker_sandbox.rs    creates: the Docker tests, ignored
.github/workflows/check.yml               modifies: the comment about a second job; `docker pull alpine:3.22`
docs/plans/project-plan.md                modifies (Task 1): step 02 and step 06 interfaces
docs/SPEC.md                              modifies (Task 2): 8.3, where git runs
```

## Interfaces

Consumes: `farik_core::governor::paths::normalise` (on main), `TaskId` (on main).

Produces:

```rust
pub struct ExecResult { pub exit_code: i32, pub stdout: String, pub stderr: String, pub timed_out: bool, pub truncated: bool }
pub enum ExecError { SpawnFailed { detail: String }, ContainerGone, OutsideWorkspace { cwd: String } }
pub trait Executor: Send + Sync {
    fn run(&self, command: &str, cwd: &str, timeout: Duration, env: &BTreeMap<String, String>) -> Result<ExecResult, ExecError>;
}
pub const OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;
pub enum SandboxError { DockerUnavailable, ImageMissing { image: String }, ContainerFailed { detail: String } }
pub trait Sandbox: Executor { fn discard(self: Box<Self>) -> Result<(), SandboxError>; }
pub trait SandboxFactory: Send + Sync {
    fn create(&self, project_id: &str, task_id: &TaskId, worktree: &Path, network: bool) -> Result<Box<dyn Sandbox>, SandboxError>;
}
pub const SANDBOX_IMAGE: &str;                       // "farik/sandbox:<crate version>"
pub struct HostSandbox { /* root */ }  impl HostSandbox { pub fn new(root: PathBuf) -> HostSandbox; }
pub struct HostSandboxFactory;
pub struct DockerSandbox { /* container name */ }
impl DockerSandbox { pub fn create(project_id: &str, task_id: &TaskId, worktree: &Path, network: bool, image: &str) -> Result<DockerSandbox, SandboxError>; pub fn name(&self) -> &str; }
pub struct DockerSandboxFactory { pub image: String }
```

An `ExecResult` with a non-zero `exit_code` is `Ok`: a failing command is a value (code.md).

## Tasks

### Task 1: the executor on the host

Files: created `crates/runtime/src/exec.rs`, `crates/runtime/src/sandbox.rs` (`SandboxError`, `Sandbox`, `SandboxFactory`, `SANDBOX_IMAGE`), `crates/runtime/src/sandbox/host.rs`; modified `crates/runtime/src/lib.rs`, `docs/plans/project-plan.md`; tested in `host.rs`
Produces: everything above except the Docker items
Consumes: `normalise`, `TaskId`

Tests, each in a fresh directory under `std::env::temp_dir()` named with the process id and the test:

- `runs_a_command_in_the_workspace_and_reports_its_output` — `printf hi; printf err >&2; exit 3` asserts `exit_code == 3`, `stdout == "hi"`, `stderr == "err"`, `timed_out == false`, `truncated == false`.
- `runs_in_a_subdirectory_named_relative_to_the_workspace` — with `sub/` made, `pwd` in `cwd = "sub"` prints the canonical path of `<root>/sub`.
- `treats_an_empty_or_dot_directory_as_the_workspace` — `pwd` with `cwd` `""`, `"."`, and `"./"` each prints the canonical root.
- `refuses_a_directory_outside_the_workspace` — `cwd` of `/tmp` and of `../x` are each `Err(ExecError::OutsideWorkspace { cwd })` with the `cwd` given, and the command did not run (a marker file it would create is absent).
- `kills_a_command_and_its_children_at_the_deadline` — a command with a background child, with a 200 ms timeout returns within 5 s with `timed_out == true` and `exit_code == 137`, and `pgrep -f` for a unique sleep argument (the test runs `sleep 31 & sleep 32; wait` and matches those) finds nothing within a 1 s retry loop.
- `keeps_the_first_mebibyte_of_output_and_says_it_cut` — `head -c 3000000 /dev/zero | tr '\0' a` asserts `stdout.len() == OUTPUT_LIMIT_BYTES`, `truncated == true`, `exit_code == 0` (the pipe was drained, so the command finished).
- `passes_only_the_environment_it_was_given` — `CARGO_MANIFEST_DIR`, which cargo sets for the test process (asserted present first; `set_var` is `unsafe` in edition 2024 and the workspace forbids it), is absent from `env`'s output, which contains `GIVEN=yes` from `env` and `PATH=`.
- `creates_a_host_sandbox_rooted_at_the_worktree` — `HostSandboxFactory.create("p", FRK-1, root, false)` gives a sandbox whose `pwd` prints the canonical root.
- `discards_a_host_sandbox_without_touching_the_workspace` — `discard` is `Ok` and the root still exists.
- `displays_each_error_in_words` — `ImageMissing { image: "farik/sandbox:0.0.0" }` displays a sentence containing the image; `DockerUnavailable` one containing `docker`; `ExecError::OutsideWorkspace { cwd: "../x" }` one containing `../x`.

- [ ] `feat(runtime): run commands in a task's workspace on the host`

### Task 2: the Docker sandbox

Files: created `crates/runtime/src/sandbox/docker.rs`, `crates/runtime/tests/docker_sandbox.rs`, `crates/runtime/sandbox/Dockerfile`; modified `crates/runtime/src/sandbox.rs`, `crates/runtime/src/lib.rs`, `.github/workflows/check.yml`, `docs/SPEC.md`
Produces: `DockerSandbox`, `DockerSandboxFactory`
Consumes: `Executor`, `Sandbox`, `SandboxFactory`, `SandboxError`, `ExecResult`, the output and deadline helpers from Task 1

Tests, all `#[ignore = "needs docker"]`, on `alpine:3.22`, each with its own project id holding the process id so that none collides with another run, and each discarding its container at the end:

- `runs_a_command_in_the_container_at_workspace` — `pwd; cat marker.txt` with `marker.txt` written in the worktree beforehand prints `/workspace` and the marker's text; `exit_code == 0`.
- `writes_files_the_user_owns` — `touch made.txt` then, on the host, the file exists and its owner uid is the test process's (`std::os::unix::fs::MetadataExt::uid`, compared with `id -u`).
- `has_no_network_unless_asked` — with `network: false`, `wget -q -T 3 -O- http://example.com` exits non-zero; the container's `docker inspect` network mode is `none`.
- `stops_a_command_at_the_deadline_inside_the_container` — `sleep 30` with a 1 s timeout returns within 10 s with `timed_out == true`, and `docker exec <name> ps` no longer lists `sleep 30`.
- `refuses_an_image_that_is_not_there` — `DockerSandbox::create` with `farik/no-such-image:0` is `Err(SandboxError::ImageMissing { image })` naming it, and no container of that name exists.
- `replaces_a_container_left_behind` — creating the same project and task twice succeeds, and one container with the name exists.
- `answers_container_gone_after_discard` — create handle A, then B with the same ids (which replaces A's container), discard B: `run` on A is `Err(ExecError::ContainerGone)`.
- `passes_only_the_environment_it_was_given_into_the_container` — `env` in the container lacks `CARGO_MANIFEST_DIR` (present in the test process) and contains `GIVEN=yes`.
- `names_a_container_docker_accepts_from_any_project_id` — project id `My Project/1` gives a container named `farik-my-project-1-frk-1` (the task id lowercased too, since Docker names are case-sensitive and one rule for the whole name is simpler), and `name()` returns it.

- [ ] `feat(runtime): run commands in a docker container per task`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, the eight docker tests among those run (CI; this machine has no Docker)
```
