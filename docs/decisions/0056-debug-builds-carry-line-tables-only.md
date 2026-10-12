# 0056. Debug builds carry line tables only

Date: 2026-10-11
Status: accepted (the founder, 2026-10-11, after an analysis of why WSL reached its 16 GB memory limit and 94% CPU during builds)

## Context

A debug build of the workspace had become the heaviest thing on the founder's machine. `catervas-runtime` is about 131,000 lines in one crate, and each of the 31 files under the crates' `tests/` folders links into its own test program carrying the whole crate and its 436 dependencies with full debug info. Those programs were 650 to 850 MB each, a `target/` reached 60 to 70 GB, and cargo linking up to 16 of them at once took WSL to its 16 GB memory limit. The workspace set no `[profile.dev]`, so everything was built with full debug info. The linker was already LLD, the Rust toolchain's default on Linux since 1.90, so a faster linker would not have helped.

## Decision

- **The workspace's own crates are built with `debug = "line-tables-only"`.** Backtraces and panics keep their file and line numbers; stepping through local variables in a debugger is what goes.
- **Dependencies are built with `debug = false`** (`[profile.dev.package."*"]`). A panic inside a dependency still names the function, but not the file and line.
- **Release builds are unchanged.**
- Rejected: installing mold. The build already links with LLD.
- Rejected, for now: merging the test files into one test program per crate. It would cut linking further but touches every integration test; it can be its own change if builds are still too heavy.
- Not in the repository: sharing one build folder across checkouts and worktrees through `build.build-dir` in the developer's own `~/.cargo/config.toml`. That is a choice for each machine, not for the project. The web journey's fixture reads `target/debug` (`apps/web/e2e/fixtures/serve.ts`), and `build-dir` leaves the final binaries there, so it is compatible.

## Consequences

- On the founder's machine, measured on `origin/main` at `e12ed091`: `cargo test --workspace --no-run` from scratch peaked at 8.5 GB of WSL memory in use, about 5 GB above idle, where it had reached the 16 GB limit; the largest test program went from 849 MB to 294 MB; the build's intermediate files took 9.1 GB.
- Debugging with a debugger in a debug build needs a local override: `CARGO_PROFILE_DEV_DEBUG=true`.
- Continuous integration builds less as well.
