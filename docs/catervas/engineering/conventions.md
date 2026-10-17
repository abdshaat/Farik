# Conventions

The rules the code in this repository follows, each with where it was read. The project writes its
own rules down in `docs/standards/code.md` and `CLAUDE.md`; this page points at them, says which a
tool enforces, and says where the code varies from them. Where this page and `docs/standards/code.md`
disagree, `code.md` wins: it is the project's own document.

Read on 2026-10-11 from the worktree of `feature/CTV-4`, branched from `main` at `6e82eab2`.

## The toolchain

| Rule | Enforced by | Read in |
|---|---|---|
| Rust 1.98.1, with `rustfmt` and `clippy`; edition 2024 | `rust-toolchain.toml`; CI installs the same version | `rust-toolchain.toml`, `Cargo.toml` (`edition`, `rust-version`), `.github/workflows/check.yml` |
| One Cargo workspace: `crates/*` and `xtask`; `Cargo.lock` committed | Cargo | `Cargo.toml` (`members`), `Cargo.lock` |
| Every workspace dependency pinned to an exact version with `=` | nothing; by habit, and every entry follows it | `Cargo.toml` `[workspace.dependencies]`, `docs/standards/code.md` (Toolchain) |
| A dependency that is "already locked through" another crate, or needs a reason, carries a comment saying why it is direct | nothing; by habit | `Cargo.toml` (comments above `async-imap`, `oauth2`, `reqwest`, `socket2`, `url` and others) |
| The front end is a pnpm workspace (`packages/*`, `apps/*`), dependencies pinned to exact versions, installed with `--frozen-lockfile` | pnpm, CI | `pnpm-workspace.yaml`, `package.json`, `apps/web/package.json`, `.github/workflows/check.yml` |
| TypeScript with `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` | `tsc` in `pnpm check` | `tsconfig.base.json`, `docs/standards/code.md` |

## Formatting and lint

| Rule | Enforced by | Read in |
|---|---|---|
| Rust is formatted by `rustfmt`, style edition 2024, no other options | `cargo fmt --all --check` in `cargo xtask check` and the pre-commit hook | `rustfmt.toml`, `xtask/src/main.rs` (`check`, `pre_commit`) |
| Clippy with `-D warnings` on every target; `clippy::all` and `clippy::pedantic` at warn, so both fail the check | `cargo clippy --workspace --all-targets -- -D warnings` in `cargo xtask check` | `Cargo.toml` `[workspace.lints.clippy]`, `xtask/src/main.rs` (`check`) |
| `unsafe_code` is forbidden; `missing_docs` warns, so every public item needs a doc comment | rustc via workspace lints, which every crate and `xtask` inherit (`[lints] workspace = true`) | `Cargo.toml` `[workspace.lints.rust]`, each `crates/*/Cargo.toml` and `xtask/Cargo.toml` |
| A public function's doc comment states what it does and, where they apply, what it refuses and its `# Errors` | review; `missing_docs` only checks that a comment exists | `docs/standards/code.md` (Documentation rules); e.g. `xtask/src/check.rs` (`tests_requested`) |
| Front end: Biome formats and lints `*.json`, `packages/**`, `apps/**`, skipping `generated`, `dist`, `node_modules` | `biome check .` in `pnpm check` | `biome.json`, `package.json` (`check`) |
| Text files: UTF-8, LF, final newline, two-space indent, trailing whitespace trimmed except in Markdown | the editor, through `.editorconfig`; no check runs it | `.editorconfig` |
| A `TODO` or `FIXME` carries a task id or an issue link | `cargo xtask todos`, over tracked `*.rs`, `*.ts`, `*.tsx`, `*.css`, `*.toml` | `xtask/src/main.rs` (`todos`), `xtask/src/todos.rs`, `docs/standards/code.md` |
| `crates/core` performs no I/O: no `std::fs`, `std::net`, `std::process`, `std::env`, `std::time::SystemTime`, `tokio`, `rand` | `cargo xtask core-io` | `xtask/src/main.rs` (`core_io`), `xtask/src/core_io.rs`, `CLAUDE.md` hard rule 5 |
| Nothing builds with `--all-features` (it would switch on the end-to-end server's code-free admission) | a test, `never_builds_with_all_features` | `xtask/src/check.rs` |

## Naming

The full tables are in `docs/standards/code.md` (Naming). What the code shows:

- Crates are `catervas-<name>` in `crates/<name>`: `cli`, `core`, `protocol`, `roles`, `runtime`,
  `store` (`Cargo.toml` `[workspace.dependencies]`, `ls crates`). The binary is `catervas`, in
  `crates/cli` (`docs/standards/code.md`; `xtask/src/check.rs` runs `cargo test -p catervas`).
- Front-end packages are `@catervas/<name>`: `packages/brand`, `packages/protocol-client`,
  `packages/ui`, `apps/web` (`apps/web/package.json` dependencies, `ls packages apps`).
- Rust modules are `snake_case.rs`; a module with children is `<name>.rs` beside a `<name>/`
  directory. `mod.rs` appears only as `src/generated/mod.rs` in `core`, `protocol` and `roles`, as
  `code.md` allows (`find crates xtask -name mod.rs`).
- Errors are `<Subject>Error` enums (`StoreError`, `GitError`, `TransitionError`, `SandboxError`,
  about thirty in all); governor outcomes that are normal are `<Subject>Refusal` or
  `<Subject>Failure` (`TransitionRefusal`, `PathRefusal`, `ToolRefusal`, `GateFailure`,
  `DoneFailure`) (`grep "pub enum .*Error"`, `grep ".*Refusal|.*Failure"` over `crates/`).
- React components are `PascalCase.tsx`, one per file, with a co-located `<Component>.module.css`
  (`apps/web/src/pages/Board.tsx`, `Board.module.css`).
- TypeScript files are meant to be `kebab-case.ts` (`code.md`). Most are one lower-case word
  (`lanes.ts`, `store.ts`) or kebab-case (`service-logo.ts`, `fake-socket.ts`). **It varies:** four
  are camelCase: `apps/web/src/pages/planPosts.ts`, `apps/web/src/pages/sellerMail.ts`,
  `apps/web/src/pages/ordersWords.test.ts`, `apps/web/src/test/sellerMail.ts`. Follow `code.md` in
  new files.
- Wire and file formats use `snake_case` keys and values; Rust fields match them; TypeScript uses
  `camelCase` with one mapping layer at the edge (`CLAUDE.md` hard rule 6, `code.md` Wire and file
  formats).
- Event kinds are `<entity>.<past_tense_verb>` (`CLAUDE.md` hard rule 7).
- Task ids are `CTV-<n>`; the step plans and ADRs written before 2026-10-10 keep `FRK-<n>` as history
  (`docs/decisions/0052-task-ids-are-ctv.md`).
- Roles live in `crates/roles/roles/<role_id>/` with `role.yaml`, `system.md`, `kit.yaml` and
  `skills/` (`ls crates/roles/roles/software_developer`). `code.md` still writes the path as
  `roles/<role_id>/` and lists no `kit.yaml`; the code is as above.

## Structure

- Plain functions and data over structs with methods; a struct with methods holds state with
  invariants (`code.md`, Rules the tools cannot enforce). `xtask/src/check.rs` is the pattern: free
  functions returning data, which `main.rs` runs.
- Comments explain why, never what (`code.md`). The code holds to it: e.g. the comments in
  `.github/workflows/check.yml` and `xtask/src/check.rs` give reasons, not descriptions.
- Schema types are generated at compile time with `typify::import_types!` from
  `docs/schemas/*.schema.json`; nothing generated is committed (`code.md`, ADR 0009).
- No feature flags in the first release (`code.md`); the one feature in use is `e2e`, for the
  end-to-end server, which the check lints and builds separately (`xtask/src/check.rs`,
  `integration_steps`).

## Errors

- At crate boundaries errors are values: a named `<Subject>Error` enum per crate, with `Display` and
  `std::error::Error` written by hand, since `thiserror` is not a dependency (`code.md`, ADR 0006;
  no `Cargo.toml` names `thiserror`). `xtask` and binaries may use `anyhow` (`xtask/src/main.rs`).
- A panic is only for a violated invariant inside a crate (`code.md`).
- No `unwrap` or `expect` outside tests, `LazyLock` initialisers and `serde_json::to_value` on a
  derived `Serialize`, and never without a message saying why it cannot fail (`code.md`). No lint
  enforces this (no `clippy::unwrap_used` is configured in `Cargo.toml`); review does.

## Tests

- Rust unit tests sit in `#[cfg(test)] mod tests` at the bottom of the module they test
  (`code.md`; e.g. `xtask/src/check.rs`, `xtask/src/commit_message.rs`).
- Integration tests are `crates/<name>/tests/<subject>.rs`: `crates/cli/tests/` (17 entries),
  `crates/runtime/tests/` (11), `crates/store/tests/` (6). `core`, `protocol` and `roles` have none
  (`ls crates/*/tests`).
- A test that needs a program (git, Docker, a browser) is `#[ignore = "needs …: cargo xtask check
  --integration"]`; plain `cargo test --workspace` skips it and `cargo xtask check --integration`
  runs it with `--include-ignored` (`crates/store/tests/project_scan.rs`, `xtask/src/check.rs`
  `test_arguments`). About 1,500 lines under `crates/` mention `#[ignore` (`grep -rc`), so most of
  the integration suite runs only with `--integration`.
- Live tests against a real service are `tests/live_<subject>.rs`, run only with
  `CATERVAS_LIVE_TESTS=1` (`code.md`; `crates/cli/tests/live_claude.rs`,
  `crates/runtime/tests/live_kit_pins.rs`).
- Rust test names are `snake_case` sentences starting with a verb that state the behaviour
  (`code.md`; e.g. `rejects_a_subject_longer_than_72_characters` in `xtask/src/commit_message.rs`).
- Fixtures are builder functions in `<module>/fixtures.rs` (`code.md`).
- Front end: Vitest, co-located `*.test.ts(x)` (`apps/web/package.json` `test`,
  `apps/web/src/app/lanes.test.ts`); the browser journey is Playwright in `apps/web/e2e/*.spec.ts`
  (`apps/web/package.json` `e2e`).
- Tests never depend on wall-clock time, the network or execution order; snapshots are avoided; a
  failing test is fixed or deleted with a reason, never skipped (`code.md` Testing rules, `CLAUDE.md`
  hard rule 9).

## The checks

`cargo xtask check` is the project's single "is this mergeable" command. It runs, in order:
`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace`; the bare-TODO check; the core no-I/O check; then, when `package.json` exists,
`pnpm install --frozen-lockfile` and `pnpm check` (generate, typecheck, `biome check .`, tests).
`--integration` first generates the brand tokens and builds the web app, adds `--include-ignored` to
the tests, and afterwards lints, tests and builds the `e2e` server and runs the Playwright journey
(`xtask/src/main.rs` `check`, `xtask/src/check.rs`, `package.json` `check`).

Catervas's own criteria for this project (the criterion library) run the first three of those, and a
build, one by one: `cargo build --workspace`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. They do not
run the TODO, core-I/O or front-end checks; `cargo xtask check` does.
