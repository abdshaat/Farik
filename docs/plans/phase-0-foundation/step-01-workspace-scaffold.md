# Phase 0, step 01: Workspace scaffold

Status: in progress
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 8.1 (a Rust backend in one Cargo workspace; `farik-core` has no I/O), section 9 (Apache 2.0); ADR 0005 (toolchain); `docs/standards/code.md`, "Toolchain"
Depends on: none

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

The repository becomes a Cargo workspace in which `cargo xtask check` runs the format check, clippy, the tests, the bare-TODO check, and the core no-I/O check, locally and in a GitHub Actions workflow, and every commit is checked for the Conventional Commits shape and for formatting before it lands. `farik-core` exists with one passing test, the `LICENSE` file says Apache 2.0, and `CLAUDE.md` and `README.md` tell a contributor how to run the check. Nothing about Farik's behavior exists yet; this step is the floor every later step stands on.

## Decisions

All recorded in `docs/plans/project-plan.md`, phase 0 and the every-phase list, and in ADR 0005; restated here only where the step needs the exact value.

- Rust 1.98.1 in `rust-toolchain.toml` with `rustfmt` and `clippy`; edition 2024; `rust-version = "1.98.1"` in the workspace package table.
- Workspace lints: `unsafe_code = "forbid"`, `missing_docs = "warn"`, `clippy::all` and `clippy::pedantic` at `warn`; `cargo xtask check` runs clippy with `-D warnings`, so every warning fails the check.
- The only dependency in this step is `anyhow` 1.0.104, in `xtask` (a binary crate may use `anyhow`; library crates use named error enums). `farik-core` has no dependencies yet.
- `cargo xtask` is an alias in `.cargo/config.toml` for `cargo run --quiet --package xtask --`. The `xtask` crate is a library (`commit_message`, `todos`) plus a binary (`main.rs`) so that the rules have unit tests and the binary stays thin.
- `cargo xtask check` runs, in order: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the bare-TODO check, the core no-I/O check. Step 02 inserts the generated-file check after the tests.
- Hooks are two shell one-liners written by `cargo xtask install-hooks`: `pre-commit` runs `cargo xtask pre-commit` (format check and bare-TODO check); `commit-msg` runs `cargo xtask commit-msg "$1"`. Hooks are a convenience; CI is the enforcement.
- The commit-msg rule accepts `<type>(<scope>): <subject>` with the nine types from `docs/standards/code.md`, a kebab-case scope, a lower-case first character, no trailing period, and at most 72 characters; `Merge ...` and `Revert ...` subjects pass.
- The bare-TODO rule scans tracked `.rs`, `.ts`, `.tsx`, `.css`, and `.toml` files for `TODO` or `FIXME` not followed by `(FRK-<n>)`, `(#<n>)`, or `(<http link>)`, skipping `xtask/src/`, whose sources describe the rule.
- The core no-I/O rule fails when a tracked file under `crates/core/src` mentions `std::fs`, `std::net`, `std::process`, `std::env`, `std::time::SystemTime`, `tokio`, or `rand`.
- `LICENSE` is the verbatim Apache License 2.0 text from `https://www.apache.org/licenses/LICENSE-2.0.txt` (sha256 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`, 202 lines).
- The CI workflow uses `actions/checkout@v5`, `dtolnay/rust-toolchain@stable` with `toolchain: 1.98.1` and the two components, and `Swatinem/rust-cache@v2`.

## Design

Three tasks. The first creates the workspace root, the `xtask` crate with its two tested rules and its commands, and `farik-core` with a smoke test, installs the hooks, and ends with the first commit passing its own hooks and `cargo xtask check` green. The second adds the CI workflow. The third updates `README.md` and `CLAUDE.md`.

Out of scope: any crate other than `farik-core` and `xtask`, code generation (step 02), validation (step 03), the front-end toolchain (phase 5).

## Architecture notes

Creates `crates/core` (`farik-core`) and `xtask`. Touches the repository root, `.cargo/`, `.github/workflows/`, `README.md`, and `CLAUDE.md`. Consumes nothing from any crate; `docs/standards/code.md` and `.editorconfig` on `main` define the formatting `rustfmt` produces (two-space indentation is Rust's four-space; `.editorconfig` governs the other files).

## Global constraints

- `farik-core` does no I/O; enforced by `cargo xtask core-io`.
- Every dependency is an exact version (`=`).
- Every public item carries a doc comment (`missing_docs` warns, and warnings fail).
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
LICENSE                                   creates: Apache License 2.0, verbatim
rust-toolchain.toml                       creates: 1.98.1 with rustfmt and clippy
Cargo.toml                                creates: workspace members, package defaults, pinned dependencies, lints
Cargo.lock                                creates (generated by cargo)
.cargo/config.toml                        creates: the `cargo xtask` alias
rustfmt.toml                              creates: edition and style edition 2024
.gitignore                                modifies: adds `target/`
xtask/Cargo.toml                          creates: the xtask crate manifest
xtask/src/lib.rs                          creates: the library root
xtask/src/commit_message.rs               creates: check_commit_message and eight tests
xtask/src/todos.rs                        creates: find_bare_todos and five tests
xtask/src/main.rs                         creates: the commands
crates/core/Cargo.toml                    creates: the farik-core manifest
crates/core/src/lib.rs                    creates: CORE_CRATE_NAME and the smoke test
.github/workflows/check.yml               creates: the check workflow
README.md                                 modifies: getting started and status
CLAUDE.md                                 modifies: the Current state section
docs/plans/phase-0-foundation/step-01-workspace-scaffold.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: Workspace root, xtask, core, and commit hooks

Files: created `LICENSE`, `rust-toolchain.toml`, `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, `rustfmt.toml`, `xtask/Cargo.toml`, `xtask/src/lib.rs`, `xtask/src/commit_message.rs`, `xtask/src/todos.rs`, `xtask/src/main.rs`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`; modified `.gitignore`

Consumes: nothing
Produces: `cargo xtask check`, `cargo xtask pre-commit`, `cargo xtask commit-msg <file>`, `cargo xtask todos`, `cargo xtask core-io`, `cargo xtask install-hooks`; `xtask::commit_message::check_commit_message(message: &str) -> Result<(), String>`; `xtask::todos::find_bare_todos(files: &[(String, String)]) -> Vec<String>`; `farik_core::CORE_CRATE_NAME: &str`; installed git hooks

- [x] Confirm the starting point, on a clean checkout of `phase/0-foundation` created from `main`:

  ```
  cargo xtask check
  # expected:
  # error: could not find `Cargo.toml` in `.../Farik` or any parent directory
  ```

- [x] Fetch the license, pin the toolchain, and ignore build output:

  ```
  curl -sSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE
  sha256sum LICENSE
  # expected: cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
  printf '\n# rust\ntarget/\n' >> .gitignore
  ```

  `rust-toolchain.toml`:

  ```toml
  [toolchain]
  channel = "1.98.1"
  components = ["rustfmt", "clippy"]
  profile = "minimal"
  ```

- [x] Write `Cargo.toml`:

  ```toml
  [workspace]
  resolver = "3"
  members = ["crates/*", "xtask"]

  [workspace.package]
  version = "0.0.0"
  edition = "2024"
  license = "Apache-2.0"
  repository = "https://github.com/abdshaat/Farik"
  rust-version = "1.98.1"

  [workspace.dependencies]
  anyhow = "=1.0.104"

  [workspace.lints.rust]
  unsafe_code = "forbid"
  missing_docs = "warn"

  [workspace.lints.clippy]
  all = { level = "warn", priority = -1 }
  pedantic = { level = "warn", priority = -1 }
  ```

  `.cargo/config.toml`:

  ```toml
  [alias]
  xtask = "run --quiet --package xtask --"
  ```

  `rustfmt.toml`:

  ```toml
  edition = "2024"
  style_edition = "2024"
  ```

- [x] Write the `xtask` crate manifest and library root. `xtask/Cargo.toml`:

  ```toml
  [package]
  name = "xtask"
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  publish = false

  [dependencies]
  anyhow.workspace = true

  [lints]
  workspace = true
  ```

  `xtask/src/lib.rs`:

  ```rust
  //! Repository tasks: the check command, hooks, and code generation. Run with `cargo xtask`.

  /// Commit message rules from `docs/standards/code.md`.
  pub mod commit_message;
  /// The bare `TODO` rule from `docs/standards/code.md`.
  pub mod todos;
  ```

- [x] Write the failing tests for the commit message rule. `xtask/src/commit_message.rs` holds only this for now:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::check_commit_message;

      #[test]
      fn accepts_a_conventional_subject_with_a_type_and_a_scope() {
          assert_eq!(
              check_commit_message("feat(core): add result type\n\nbody\n"),
              Ok(())
          );
      }

      #[test]
      fn accepts_a_merge_commit() {
          assert_eq!(
              check_commit_message("Merge pull request #3 from abdshaat/phase/0-foundation"),
              Ok(())
          );
      }

      #[test]
      fn accepts_a_revert_commit() {
          assert_eq!(
              check_commit_message("Revert \"feat(core): add result type\""),
              Ok(())
          );
      }

      #[test]
      fn skips_comment_lines_when_finding_the_subject() {
          assert_eq!(
              check_commit_message("# Please enter the commit message\nfix(store): keep order"),
              Ok(())
          );
      }

      #[test]
      fn rejects_a_subject_without_a_type_and_a_scope() {
          assert_eq!(
              check_commit_message("Add result type"),
              Err("subject \"Add result type\" is not <type>(<scope>): <subject>".to_string())
          );
      }

      #[test]
      fn rejects_a_subject_longer_than_72_characters() {
          let subject = format!("feat(core): {}", "x".repeat(70));
          assert_eq!(
              check_commit_message(&subject),
              Err("subject is 82 characters; the limit is 72".to_string())
          );
      }

      #[test]
      fn rejects_a_subject_that_ends_with_a_period() {
          assert!(check_commit_message("feat(core): add result type.").is_err());
      }

      #[test]
      fn rejects_a_subject_that_starts_with_an_upper_case_letter() {
          assert!(check_commit_message("feat(core): Add result type").is_err());
      }
  }
  ```

  and `xtask/src/todos.rs` holds only this:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::find_bare_todos;

      fn marker() -> String {
          ["TO", "DO"].concat()
      }

      fn files(entries: &[(&str, String)]) -> Vec<(String, String)> {
          entries
              .iter()
              .map(|(path, text)| ((*path).to_string(), text.clone()))
              .collect()
      }

      #[test]
      fn reports_a_bare_marker_with_its_path_and_line() {
          let text = format!("let a = 1;\n// {} fix this\n", marker());
          assert_eq!(
              find_bare_todos(&files(&[("crates/core/src/a.rs", text)])),
              vec!["crates/core/src/a.rs:2".to_string()]
          );
      }

      #[test]
      fn accepts_a_marker_that_carries_a_task_id() {
          let text = format!("// {}(FRK-12) fix this\n", marker());
          assert!(find_bare_todos(&files(&[("a.rs", text)])).is_empty());
      }

      #[test]
      fn accepts_a_marker_that_carries_an_issue_number_or_a_link() {
          let a = format!("// {}(#12) fix this\n", marker());
          let b = format!(
              "// {}(https://github.com/abdshaat/farik/issues/12) fix\n",
              marker()
          );
          assert!(find_bare_todos(&files(&[("a.rs", a), ("b.rs", b)])).is_empty());
      }

      #[test]
      fn reports_a_bare_fixme_too() {
          let text = format!("// {} later\n", ["FIX", "ME"].concat());
          assert_eq!(
              find_bare_todos(&files(&[("a.rs", text)])),
              vec!["a.rs:1".to_string()]
          );
      }

      #[test]
      fn skips_its_own_source() {
          let text = format!("const BARE: &str = \"{}\";\n", marker());
          assert!(find_bare_todos(&files(&[("xtask/src/todos.rs", text)])).is_empty());
      }
  }
  ```

  with an empty `xtask/src/main.rs` containing `fn main() {}` so the crate builds.

- [x] Run the tests and confirm they fail because the functions do not exist:

  ```
  cargo test --package xtask
  # expected, among the output:
  # error[E0432]: unresolved import `super::check_commit_message`
  # error[E0432]: unresolved import `super::find_bare_todos`
  # error: could not compile `xtask` (lib test) due to 2 previous errors
  ```

- [x] Write the implementations above the tests. `xtask/src/commit_message.rs` in full:

  ```rust
  const TYPES: [&str; 9] = [
      "feat", "fix", "refactor", "test", "docs", "chore", "build", "ci", "perf",
  ];
  const SUBJECT_LIMIT: usize = 72;

  /// Checks the first non-comment line of a commit message against the Conventional Commits shape.
  ///
  /// Accepts `<type>(<scope>): <subject>` with one of the nine types, a kebab-case scope, a
  /// lower-case first character, no trailing period, and at most 72 characters; merge and revert
  /// commits pass as they are.
  ///
  /// # Errors
  ///
  /// Returns the reason the message is rejected.
  pub fn check_commit_message(message: &str) -> Result<(), String> {
      let first_line = message
          .lines()
          .find(|line| !line.starts_with('#'))
          .unwrap_or_default();
      if first_line.starts_with("Merge ") || first_line.starts_with("Revert ") {
          return Ok(());
      }
      if first_line.chars().count() > SUBJECT_LIMIT {
          return Err(format!(
              "subject is {} characters; the limit is {SUBJECT_LIMIT}",
              first_line.chars().count()
          ));
      }
      if !has_conventional_shape(first_line) {
          return Err(format!(
              "subject \"{first_line}\" is not <type>(<scope>): <subject>"
          ));
      }
      Ok(())
  }

  fn has_conventional_shape(line: &str) -> bool {
      let Some((head, subject)) = line.split_once(": ") else {
          return false;
      };
      let Some((kind, scope)) = head.split_once('(') else {
          return false;
      };
      let Some(scope) = scope.strip_suffix(')') else {
          return false;
      };
      let scope_ok = scope.chars().next().is_some_and(|c| c.is_ascii_lowercase())
          && scope
              .chars()
              .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
      let subject_ok = subject
          .chars()
          .next()
          .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
          && subject.chars().count() >= 2
          && !subject.ends_with('.');
      TYPES.contains(&kind) && scope_ok && subject_ok
  }

  #[cfg(test)]
  mod tests {
      use super::check_commit_message;

      #[test]
      fn accepts_a_conventional_subject_with_a_type_and_a_scope() {
          assert_eq!(
              check_commit_message("feat(core): add result type\n\nbody\n"),
              Ok(())
          );
      }

      #[test]
      fn accepts_a_merge_commit() {
          assert_eq!(
              check_commit_message("Merge pull request #3 from abdshaat/phase/0-foundation"),
              Ok(())
          );
      }

      #[test]
      fn accepts_a_revert_commit() {
          assert_eq!(
              check_commit_message("Revert \"feat(core): add result type\""),
              Ok(())
          );
      }

      #[test]
      fn skips_comment_lines_when_finding_the_subject() {
          assert_eq!(
              check_commit_message("# Please enter the commit message\nfix(store): keep order"),
              Ok(())
          );
      }

      #[test]
      fn rejects_a_subject_without_a_type_and_a_scope() {
          assert_eq!(
              check_commit_message("Add result type"),
              Err("subject \"Add result type\" is not <type>(<scope>): <subject>".to_string())
          );
      }

      #[test]
      fn rejects_a_subject_longer_than_72_characters() {
          let subject = format!("feat(core): {}", "x".repeat(70));
          assert_eq!(
              check_commit_message(&subject),
              Err("subject is 82 characters; the limit is 72".to_string())
          );
      }

      #[test]
      fn rejects_a_subject_that_ends_with_a_period() {
          assert!(check_commit_message("feat(core): add result type.").is_err());
      }

      #[test]
      fn rejects_a_subject_that_starts_with_an_upper_case_letter() {
          assert!(check_commit_message("feat(core): Add result type").is_err());
      }
  }
  ```

  `xtask/src/todos.rs` in full:

  ```rust
  const MARKERS: [&str; 2] = ["TODO", "FIXME"];
  const SELF: &str = "xtask/src/";

  /// Reports every `TODO` or `FIXME` that does not carry a task id `(FRK-<n>)`, an issue `(#<n>)`,
  /// or a link `(http...)`, as `path:line`, skipping the xtask sources that describe the rule.
  #[must_use]
  pub fn find_bare_todos(files: &[(String, String)]) -> Vec<String> {
      let mut findings = Vec::new();
      for (path, text) in files {
          if path.starts_with(SELF) {
              continue;
          }
          for (index, line) in text.lines().enumerate() {
              if has_bare_marker(line) {
                  findings.push(format!("{path}:{}", index + 1));
              }
          }
      }
      findings
  }

  fn has_bare_marker(line: &str) -> bool {
      MARKERS.iter().any(|marker| {
          line.match_indices(marker).any(|(start, _)| {
              let before_ok = start == 0
                  || !line[..start]
                      .chars()
                      .next_back()
                      .is_some_and(|c| c.is_ascii_alphanumeric());
              let rest = &line[start + marker.len()..];
              let after_ok = !rest
                  .chars()
                  .next()
                  .is_some_and(|c| c.is_ascii_alphanumeric());
              before_ok && after_ok && !has_reference(rest)
          })
      })
  }

  fn has_reference(rest: &str) -> bool {
      let Some(inner) = rest.strip_prefix('(') else {
          return false;
      };
      let Some((reference, _)) = inner.split_once(')') else {
          return false;
      };
      let is_task = reference
          .strip_prefix("FRK-")
          .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
      let is_issue = reference
          .strip_prefix('#')
          .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
      let is_link = reference.starts_with("http://") || reference.starts_with("https://");
      is_task || is_issue || is_link
  }

  #[cfg(test)]
  mod tests {
      use super::find_bare_todos;

      fn marker() -> String {
          ["TO", "DO"].concat()
      }

      fn files(entries: &[(&str, String)]) -> Vec<(String, String)> {
          entries
              .iter()
              .map(|(path, text)| ((*path).to_string(), text.clone()))
              .collect()
      }

      #[test]
      fn reports_a_bare_marker_with_its_path_and_line() {
          let text = format!("let a = 1;\n// {} fix this\n", marker());
          assert_eq!(
              find_bare_todos(&files(&[("crates/core/src/a.rs", text)])),
              vec!["crates/core/src/a.rs:2".to_string()]
          );
      }

      #[test]
      fn accepts_a_marker_that_carries_a_task_id() {
          let text = format!("// {}(FRK-12) fix this\n", marker());
          assert!(find_bare_todos(&files(&[("a.rs", text)])).is_empty());
      }

      #[test]
      fn accepts_a_marker_that_carries_an_issue_number_or_a_link() {
          let a = format!("// {}(#12) fix this\n", marker());
          let b = format!(
              "// {}(https://github.com/abdshaat/farik/issues/12) fix\n",
              marker()
          );
          assert!(find_bare_todos(&files(&[("a.rs", a), ("b.rs", b)])).is_empty());
      }

      #[test]
      fn reports_a_bare_fixme_too() {
          let text = format!("// {} later\n", ["FIX", "ME"].concat());
          assert_eq!(
              find_bare_todos(&files(&[("a.rs", text)])),
              vec!["a.rs:1".to_string()]
          );
      }

      #[test]
      fn skips_its_own_source() {
          let text = format!("const BARE: &str = \"{}\";\n", marker());
          assert!(find_bare_todos(&files(&[("xtask/src/todos.rs", text)])).is_empty());
      }
  }
  ```

- [x] Run the tests; confirm green:

  ```
  cargo test --package xtask
  # expected, among the output:
  # test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  ```

- [x] Write the commands. `xtask/src/main.rs` in full:

  ```rust
  //! `cargo xtask <command>`: the repository's own commands.

  use std::env;
  use std::fs;
  use std::path::{Path, PathBuf};
  use std::process::{Command, ExitCode};

  use anyhow::{Context, bail};

  const CORE_FORBIDDEN: [&str; 7] = [
      "std::fs",
      "std::net",
      "std::process",
      "std::env",
      "std::time::SystemTime",
      "tokio",
      "rand",
  ];

  fn main() -> ExitCode {
      let args: Vec<String> = env::args().skip(1).collect();
      match run(&args) {
          Ok(()) => ExitCode::SUCCESS,
          Err(error) => {
              eprintln!("xtask: {error:#}");
              ExitCode::FAILURE
          }
      }
  }

  fn run(args: &[String]) -> anyhow::Result<()> {
      let root = workspace_root();
      match args.first().map(String::as_str) {
          Some("check") => check(&root),
          Some("pre-commit") => pre_commit(&root),
          Some("commit-msg") => commit_msg(
              args.get(1)
                  .context("usage: cargo xtask commit-msg <file>")?,
          ),
          Some("todos") => todos(&root),
          Some("core-io") => core_io(&root),
          Some("install-hooks") => install_hooks(&root),
          _ => bail!(
              "usage: cargo xtask <check|pre-commit|commit-msg <file>|todos|core-io|install-hooks>"
          ),
      }
  }

  fn workspace_root() -> PathBuf {
      PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .parent()
          .expect("xtask lives one level below the workspace root")
          .to_path_buf()
  }

  fn cargo(root: &Path, args: &[&str]) -> anyhow::Result<()> {
      let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
          .args(args)
          .current_dir(root)
          .status()
          .with_context(|| format!("running cargo {}", args.join(" ")))?;
      if !status.success() {
          bail!("cargo {} failed", args.join(" "));
      }
      Ok(())
  }

  fn check(root: &Path) -> anyhow::Result<()> {
      cargo(root, &["fmt", "--all", "--check"])?;
      cargo(
          root,
          &[
              "clippy",
              "--workspace",
              "--all-targets",
              "--",
              "-D",
              "warnings",
          ],
      )?;
      cargo(root, &["test", "--workspace"])?;
      todos(root)?;
      core_io(root)?;
      println!("xtask check: ok");
      Ok(())
  }

  fn pre_commit(root: &Path) -> anyhow::Result<()> {
      cargo(root, &["fmt", "--all", "--check"])?;
      todos(root)
  }

  fn commit_msg(file: &str) -> anyhow::Result<()> {
      let message = fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
      xtask::commit_message::check_commit_message(&message)
          .map_err(|reason| anyhow::anyhow!("commit message rejected: {reason}"))
  }

  fn tracked_files(root: &Path, patterns: &[&str]) -> anyhow::Result<Vec<(String, String)>> {
      let output = Command::new("git")
          .arg("ls-files")
          .arg("-z")
          .arg("--")
          .args(patterns)
          .current_dir(root)
          .output()
          .context("running git ls-files")?;
      if !output.status.success() {
          bail!("git ls-files failed");
      }
      String::from_utf8(output.stdout)?
          .split('\0')
          .filter(|path| !path.is_empty())
          .map(|path| {
              let text =
                  fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))?;
              Ok((path.to_string(), text))
          })
          .collect()
  }

  fn todos(root: &Path) -> anyhow::Result<()> {
      let files = tracked_files(root, &["*.rs", "*.ts", "*.tsx", "*.css", "*.toml"])?;
      let findings = xtask::todos::find_bare_todos(&files);
      if findings.is_empty() {
          return Ok(());
      }
      bail!(
          "bare TODO or FIXME without a task id or issue link:\n{}",
          findings.join("\n")
      );
  }

  fn core_io(root: &Path) -> anyhow::Result<()> {
      let files = tracked_files(root, &["crates/core/src/*.rs", "crates/core/src/**/*.rs"])?;
      let mut findings = Vec::new();
      for (path, text) in &files {
          for (index, line) in text.lines().enumerate() {
              if let Some(token) = CORE_FORBIDDEN.iter().find(|token| line.contains(*token)) {
                  findings.push(format!("{path}:{}: uses {token}", index + 1));
              }
          }
      }
      if findings.is_empty() {
          return Ok(());
      }
      bail!(
          "farik-core performs no I/O (hard rule 5):\n{}",
          findings.join("\n")
      );
  }

  fn install_hooks(root: &Path) -> anyhow::Result<()> {
      let hooks = root.join(".git").join("hooks");
      fs::create_dir_all(&hooks)?;
      write_hook(
          &hooks.join("pre-commit"),
          "#!/bin/sh\nexec cargo xtask pre-commit\n",
      )?;
      write_hook(
          &hooks.join("commit-msg"),
          "#!/bin/sh\nexec cargo xtask commit-msg \"$1\"\n",
      )?;
      println!("installed pre-commit and commit-msg hooks");
      Ok(())
  }

  fn write_hook(path: &Path, body: &str) -> anyhow::Result<()> {
      fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
      #[cfg(unix)]
      {
          use std::os::unix::fs::PermissionsExt;
          fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
      }
      Ok(())
  }
  ```

- [x] Write the `farik-core` manifest and its failing smoke test. `crates/core/Cargo.toml`:

  ```toml
  [package]
  name = "farik-core"
  description = "Schemas, task state machine, governor, and cost model. Performs no I/O."
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  repository.workspace = true
  rust-version.workspace = true

  [dependencies]

  [lints]
  workspace = true
  ```

  `crates/core/src/lib.rs`, holding only the crate doc comment and the test for now:

  ```rust
  //! Farik's harness: schemas, the task state machine, the governor, and the cost model.
  //! This crate performs no I/O.

  #[cfg(test)]
  mod tests {
      use super::CORE_CRATE_NAME;
  
      #[test]
      fn exposes_its_crate_name() {
          assert_eq!(CORE_CRATE_NAME, "farik-core");
      }
  }
  ```

- [x] Run it and confirm it fails because the constant is missing:

  ```
  cargo test --package farik-core
  # expected, among the output:
  # error[E0432]: unresolved import `super::CORE_CRATE_NAME`
  ```

- [x] Make `crates/core/src/lib.rs` exactly:

  ```rust
  //! Farik's harness: schemas, the task state machine, the governor, and the cost model.
  //! This crate performs no I/O.

  /// The crate's package name, as published.
  pub const CORE_CRATE_NAME: &str = "farik-core";

  #[cfg(test)]
  mod tests {
      use super::CORE_CRATE_NAME;

      #[test]
      fn exposes_its_crate_name() {
          assert_eq!(CORE_CRATE_NAME, "farik-core");
      }
  }
  ```

- [x] Format, then run the full check; confirm green:

  ```
  cargo fmt --all
  cargo xtask check
  # expected, among the output (the order is fmt, clippy, tests, todos, core-io):
  # test result: ok. 13 passed; 0 failed; ...   (xtask)
  # test result: ok. 1 passed; 0 failed; ...    (farik-core)
  # xtask check: ok
  # exit code 0
  ```

- [x] Prove the core no-I/O check, then remove the probe:

  ```
  printf '//! probe\n/// x\npub fn probe() -> std::io::Result<String> { std::fs::read_to_string("x") }\n' > crates/core/src/probe.rs
  git add crates/core/src/probe.rs
  cargo xtask core-io
  # expected:
  # xtask: farik-core performs no I/O (hard rule 5):
  # crates/core/src/probe.rs:3: uses std::fs
  git rm -f crates/core/src/probe.rs
  ```

- [x] Install the hooks and prove them. Stage everything, then:

  ```
  cargo xtask install-hooks
  # expected: installed pre-commit and commit-msg hooks
  git add -A
  git commit -m "Add scaffold."
  # expected: the commit is refused;
  # xtask: commit message rejected: subject "Add scaffold." is not <type>(<scope>): <subject>
  git commit -m "build(repo): add the cargo workspace, xtask, and commit hooks"
  # expected: the hooks pass and the commit lands
  ```

  Tick this task's boxes in this plan and amend them into the same commit (`git add docs/plans && git commit --amend --no-edit`), which is allowed here because the commit has not been pushed.

### Task 2: Continuous integration

Files: created `.github/workflows/check.yml`

Consumes: `cargo xtask check` from Task 1
Produces: the `check` workflow on pull requests and on pushes to `main`

- [ ] Write `.github/workflows/check.yml`:

  ```yaml
  name: check
  on:
    pull_request:
    push:
      branches: [main]
  jobs:
    check:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v5
        - uses: dtolnay/rust-toolchain@stable
          with:
            toolchain: 1.98.1
            components: rustfmt, clippy
        - uses: Swatinem/rust-cache@v2
        - run: cargo xtask check
  ```

- [ ] Confirm the lockfile is complete and the check is reproducible from a clean build, which is what CI does:

  ```
  cargo clean && cargo xtask check
  # expected: ends with "xtask check: ok", exit code 0; git status shows Cargo.lock unchanged
  ```

- [ ] Commit: `ci(repo): run cargo xtask check on pull requests and main`

- [ ] Push the phase branch and open the phase's draft pull request (`docs/standards/workflow.md` stage 5), then confirm on the pull request that the `check` job ran and passed on this commit. Paste the job's summary lines into the pull request's verification section. If the job fails, the failure is this step's to fix before Task 3.

### Task 3: Documentation

Files: modified `README.md`, `CLAUDE.md`

Consumes: nothing
Produces: contributor instructions that match the repository

- [ ] In `README.md`, replace the line beginning `Status: specification stage.` with:

  ```markdown
  Status: phase 0 (foundation) in progress; nothing runs yet. Project standards are in place; see [CONTRIBUTING.md](CONTRIBUTING.md) before making a change.

  ## Getting started

  Install `rustup`; it reads `rust-toolchain.toml` and installs Rust 1.98.1 with `rustfmt` and `clippy` on first use. Then:

  ```
  cargo xtask install-hooks
  cargo xtask check
  ```

  `cargo xtask check` runs the format check, clippy, the tests, the bare-TODO check, and the core no-I/O check, and is what the `check` workflow runs on every pull request. `cargo fmt --all` rewrites files to the house style.
  ```

  (The inner fenced block uses three backticks in the file; it is shown indented here only to nest it.)

- [ ] In `CLAUDE.md`, replace the `## Current state` paragraph with:

  ```markdown
  Phase 0 (foundation) is in progress on `phase/0-foundation`. The Cargo workspace exists; `farik-core` has no behavior yet. The next steps are the schema generation pipeline and contract validation, per `docs/plans/project-plan.md`.
  ```

- [ ] Run the full check once more; confirm green (Markdown is not checked, so the output is the same as Task 1's).

- [ ] Commit: `docs(repo): describe the toolchain and the check command`

## Verification

```
cargo xtask check
# expected: exit code 0, ending with
# xtask check: ok
# with "13 passed" for xtask and "1 passed" for farik-core among the test results.
```

```
git log --oneline -3
# expected, newest first:
# docs(repo): describe the toolchain and the check command
# ci(repo): run cargo xtask check on pull requests and main
# build(repo): add the cargo workspace, xtask, and commit hooks
```

The `check` workflow run on the pushed head of `phase/0-foundation` is green; its link and summary are in the pull request.

```
sha256sum LICENSE
# expected: cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
```

## Open questions

none
