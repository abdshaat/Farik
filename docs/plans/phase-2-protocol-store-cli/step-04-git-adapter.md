# Phase 2, step 04: The git adapter

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 8.1 (the git adapter lives in `farik-store`), 5.14 (a branch and a worktree per task, and what integration does), 5.6 (allowed paths, which is what asks about a change's paths); `docs/standards/code.md`, "Rust integration test"
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01, 02 and 03 of this phase (committed as 1f93550, 50b264e and e99deae)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: <pending>

## Goal

Farik can drive the repository it lives in. `Git` answers what is at the tip of a branch and which branch is the default, makes the branch and the worktree a task works in and takes the worktree away again, says what a branch changed and how many commits it took, and merges a finished task into the integration branch or says what conflicted without leaving a half-merged tree behind. Everything runs the `git` program as a child process: a repository is the user's own, and the only behaviour anyone can rely on is the program's.

This step also gives the repository a way to run tests that need a program the machine may not have. Eight of the tests here need `git`, so they are marked `#[ignore]` and run by `cargo xtask check --integration`, which is what CI runs.

## Decisions

- Every operation shells out to `git` rather than using a library: `docs/SPEC.md` 8.1 says so, and the reason is that a repository belongs to the user — `git status` in their terminal and Farik's must agree, and the only way to guarantee that is to ask the same program.
- The tests that need `git` are marked `#[ignore]`, not put behind a cargo feature and not skipped by an environment variable at run time: chose `#[ignore]` because the default check still *compiles* them, so `cargo fmt` and `clippy` hold them to the same standard and they cannot rot, and because `cargo test` prints them as ignored rather than passing silently. A feature would hide them from the default build; an environment variable would make a test that ran nothing look like a test that passed.
- `cargo xtask check --integration` adds `--include-ignored` to the test run and changes nothing else, so one command is the whole check rather than half of it.
- **CI runs one job, not two.** The project plan records this step as adding "the CI job for `cargo xtask check --integration`", a second job in the `check` workflow. A second job would compile the workspace again to run a strict superset of what the first ran, because `--integration` only adds the ignored tests to the same `cargo test` invocation, and `ubuntu-latest` already has `git`. The workflow runs `cargo xtask check --integration` in the one job it has, and carries a comment saying that a second job starts paying for itself when a test needs Docker, which is phase 3 step 02. Task 6 records the change.
- `GitError` gains a third variant, `NotInstalled { detail }`: the recorded interface has `NotARepository` and `CommandFailed { command, stderr }`, and neither says "the `git` program is not on this machine". Folding that into `CommandFailed` would report an empty `stderr` for a command that never ran. Task 6 records it.
- Every method asks `require_repository` first and answers `NotARepository` itself, rather than passing on whatever git printed about a directory it has never heard of. `farik init` is what reports this to a user, and it should read the same whichever method noticed.
- `changed_paths` passes `--no-renames`, so a move comes back as the old path and the new one: the governor is asked about each path a change touched (5.6), and a rename reported only by its new name would let work land at a path nobody allowed.
- `changed_paths` and `diff` use the three-dot range `base...head` — what the branch did since it and `base` last agreed — while `commit_count` uses two dots, `base..head`, which is how many commits `head` has that `base` does not. A reviewer reading a task's work wants the first; a gate counting the task's commits wants the second.
- Every path git prints comes back through `-z`, so nothing is split on a newline: a path may hold one, and a path list that loses a file is worse than one that refuses.
- A conflicted merge is undone before `merge` returns: the tree it would otherwise leave is one nobody is watching, and every later command would trip over it. The task escalates with reason `integration` instead (5.14), and `MergeOutcome::Conflicts` is what says so.
- `remove_worktree` passes `--force`, because a finished task's worktree holds whatever its session built — untracked output git would otherwise refuse to remove, and that nothing wants kept.
- `default_branch` asks `origin/HEAD` and falls back to the branch `HEAD` is on: a repository with no remote records its default branch nowhere at all. `.farik/team.yaml` is where a team says otherwise, and the integration branch is its to set (5.14).
- `Git::open` cannot fail and checks nothing; `is_repository` is what asks. A constructor that ran a program would make every caller handle an error before it had asked for anything.
- The parsing is separate from the running — `head_summary_of`, `default_branch_of`, `changed_paths_of`, `path_argument` are free functions of text — so the shapes git prints are held by unit tests that need no git at all, and only the running needs the program.
- No new dependency, and `Cargo.lock` does not change.

## Design

`crates/store/src/git.rs` holds `Git`, `GitError`, `HeadSummary`, `MergeOutcome` and one free function per shape git prints. `crates/store/tests/git.rs` drives all of it against real repositories it makes in the temporary directory and removes when each test ends.

`xtask` grows a `Tests` enum and a `--integration` flag on `check`; `.github/workflows/check.yml` runs the check with that flag.

Out of scope: `commit` and `push`, which phase 3 step 03 adds with the tools that need them; anything that reads or writes `.farik/` (step 05); any caller of this adapter at all — nothing outside its own tests uses it yet.

## Architecture notes

- Modified: `crates/store` gains `git`, a sibling of `event_log` and `projections`. It touches neither of them and shares nothing with them: a repository is not a database.
- Modified: `xtask/src/main.rs` gains the flag; `.github/workflows/check.yml` uses it.
- Consumed: nothing from the other crates. `git.rs` uses only `std`.
- `farik-core` does no I/O and is not touched; `cargo xtask core-io` still passes.

## Global constraints

- Every method runs `git` through `run_git`, which is the one place a child process is spawned and the one place a failure becomes a `GitError`.
- No `unwrap` or `expect` outside tests.
- A path reaches git as a `&str` through `path_argument`, which refuses one that is not text rather than mangling it.
- Every test that needs the `git` program carries `#[ignore = "needs the git program: cargo xtask check --integration"]`, spelled exactly so, so that `cargo test` prints the reason.
- No test is skipped, ignored, or quarantined to get green: the eight ignored tests are gated on a program, and they run in CI on every pull request.

## File map

```
crates/store/src/git.rs                        creates: Git, GitError, HeadSummary, MergeOutcome; tested by its own tests module
crates/store/tests/git.rs                      creates: the adapter against real repositories
crates/store/src/lib.rs                        modifies: the git module and what it re-exports
xtask/src/main.rs                              modifies: check takes --integration
.github/workflows/check.yml                    modifies: CI runs the check with --integration
docs/plans/project-plan.md                     modifies: records what this step's interface became, and the one CI job
docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md modifies: this plan, ticked as it goes
```

`Cargo.toml` and `Cargo.lock` do not change: this step adds no dependency.

## Tasks

### Task 1: A repository, and the check that can run these tests

Files: created `crates/store/src/git.rs`, `crates/store/tests/git.rs`; modified `crates/store/src/lib.rs`, `xtask/src/main.rs`, `.github/workflows/check.yml`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: nothing from this plan
Produces: `farik_store::{Git, GitError}`, `Git::{open, is_repository}`, and `cargo xtask check --integration`

The flag arrives with the first test that needs it. Without it the eight tests this step writes would be written and never run until the end, which is not a red-green cycle at all.

- [ ] Write the failing tests. Create `crates/store/src/git.rs` with the module doc:

  ```rust
  //! The repository Farik works in, driven through the `git` program rather than reimplemented
  //! (`docs/SPEC.md` sections 8.1 and 5.14).
  ```

  then append the tests module:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::GitError;

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              GitError::NotARepository,
              GitError::NotInstalled {
                  detail: "No such file or directory (os error 2)".to_string(),
              },
              GitError::CommandFailed {
                  command: "branch farik/FRK-1 main".to_string(),
                  stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no git repository here",
                  "git could not be run: No such file or directory (os error 2)",
                  "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                   exists",
              ]
          );
      }
  }
  ```

- [ ] Create `crates/store/tests/git.rs`:

  ```rust
  //! The git adapter against a real repository.
  //!
  //! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
  //! `cargo xtask check --integration`. They are ignored rather than compiled out: the default check
  //! still builds them, so `cargo fmt` and `clippy` hold them to the same standard as everything else
  //! and they cannot rot unnoticed (`docs/standards/code.md`, "Rust integration test").

  use std::path::{Path, PathBuf};
  use std::process::Command;

  use farik_store::Git;

  /// A repository of its own, removed when the test ends however the test ends.
  struct TempRepo {
      path: PathBuf,
  }

  impl TempRepo {
      /// A repository with one commit on `main`, holding `README.md`.
      fn new(name: &str) -> Self {
          let path = std::env::temp_dir().join(format!(
              "farik-git-{name}-{}-{:?}",
              std::process::id(),
              std::thread::current().id()
          ));
          let _ = std::fs::remove_dir_all(&path);
          std::fs::create_dir_all(&path).expect("a directory under the temporary directory");
          let repository = Self { path };
          repository.git(&["init", "-b", "main"]);
          // A test must not depend on the machine's git configuration, and signing would ask for a
          // key nobody has here.
          repository.git(&["config", "user.name", "Farik Test"]);
          repository.git(&["config", "user.email", "test@farik.invalid"]);
          repository.git(&["config", "commit.gpgsign", "false"]);
          repository.write("README.md", "the first line\n");
          repository.commit("the first commit");
          repository
      }

      fn adapter(&self) -> Git {
          Git::open(self.path.clone())
      }

      fn write(&self, name: &str, text: &str) {
          let path = self.path.join(name);
          if let Some(directory) = path.parent() {
              std::fs::create_dir_all(directory).expect("the directory the file is in");
          }
          std::fs::write(path, text).expect("the file is written");
      }

      fn commit(&self, message: &str) {
          self.git(&["add", "-A"]);
          self.git(&["commit", "-m", message]);
      }

      fn git(&self, arguments: &[&str]) -> String {
          run_git(&self.path, arguments)
      }
  }

  impl Drop for TempRepo {
      fn drop(&mut self) {
          let _ = std::fs::remove_dir_all(&self.path);
      }
  }

  /// Runs git directly, for the setup a test needs before the adapter is the thing under test.
  fn run_git(directory: &Path, arguments: &[&str]) -> String {
      let output = Command::new("git")
          .args(arguments)
          .current_dir(directory)
          .output()
          .expect("git runs");
      assert!(
          output.status.success(),
          "git {}: {}",
          arguments.join(" "),
          String::from_utf8_lossy(&output.stderr)
      );
      String::from_utf8_lossy(&output.stdout).trim().to_string()
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn knows_a_repository_from_a_directory_that_is_not_one() {
      let repository = TempRepo::new("is-a-repository");
      assert!(repository.adapter().is_repository());
      let plain = repository.path.join("not-a-repository");
      std::fs::create_dir_all(&plain).expect("a plain directory");
      // A directory inside the repository is still inside it; one outside is not.
      assert!(Git::open(plain).is_repository());
      assert!(!Git::open(std::env::temp_dir().join("farik-nowhere-at-all")).is_repository());
  }
  ```

- [ ] Declare the module in `crates/store/src/lib.rs`. rustfmt keeps both lists alphabetical, so this goes between `event_log` and `migrations` rather than at the end — replace

  ```rust
  /// The event log.
  pub mod event_log;
  ```

  with:

  ```rust
  /// The event log.
  pub mod event_log;
  /// The repository Farik works in.
  pub mod git;
  ```

- [ ] Run them and confirm they fail because there is no adapter:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::GitError`
  # error: could not compile `farik-store` (lib test) due to 1 previous error
  # (cargo stops there, so the integration test's own errors are not printed yet)
  ```

- [ ] Write the minimal implementation. Insert into `crates/store/src/git.rs`, between the module doc and the tests module:

  ```rust
  use std::fmt;
  use std::path::{Path, PathBuf};
  use std::process::Command;
  ```

  then, after that:

  ```rust
  /// Why a git operation did not happen.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum GitError {
      /// The path Farik was given is not inside a git repository. `farik init` is what reports this
      /// to a user; nothing else in Farik can do anything useful without one.
      NotARepository,
      /// The `git` program could not be run at all.
      NotInstalled {
          /// What the operating system said.
          detail: String,
      },
      /// `git` ran and refused.
      CommandFailed {
          /// The arguments it was given, so that a person can run the same thing by hand.
          command: String,
          /// What it said on standard error, trimmed.
          stderr: String,
      },
  }

  impl fmt::Display for GitError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::NotARepository => write!(formatter, "there is no git repository here"),
              Self::NotInstalled { detail } => write!(formatter, "git could not be run: {detail}"),
              Self::CommandFailed { command, stderr } => {
                  write!(formatter, "git {command} refused: {stderr}")
              }
          }
      }
  }

  impl std::error::Error for GitError {}
  ```

  then, after that:

  ```rust
  /// One repository, at a path.
  ///
  /// Every method runs `git` as a child process. Farik does not reimplement git: a repository is the
  /// user's own, and the only behaviour anyone can rely on is the program's.
  pub struct Git {
      root: PathBuf,
  }
  ```

  then, after that:

  ```rust
  impl Git {
      /// The repository at `root`. Nothing is run and nothing is checked until a method is called, so
      /// this cannot fail; `is_repository` is what asks.
      #[must_use]
      pub fn open(root: PathBuf) -> Self {
          Self { root }
      }

      /// Whether `root` is inside a git repository.
      #[must_use]
      pub fn is_repository(&self) -> bool {
          self.at_root(&["rev-parse", "--git-dir"]).is_ok()
      }

      /// Runs git at the repository's root.
      fn at_root(&self, arguments: &[&str]) -> Result<String, GitError> {
          run_git(&self.root, arguments)
      }
  }
  ```

  and after that:

  ```rust
  /// Runs git in `directory` and hands back what it said on standard output, trimmed.
  fn run_git(directory: &Path, arguments: &[&str]) -> Result<String, GitError> {
      let output = Command::new("git")
          .args(arguments)
          .current_dir(directory)
          .output()
          .map_err(|error| GitError::NotInstalled {
              detail: error.to_string(),
          })?;
      if !output.status.success() {
          return Err(GitError::CommandFailed {
              command: arguments.join(" "),
              stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
          });
      }
      Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
  }
  ```

- [ ] Re-export them from `crates/store/src/lib.rs`, after the `event_log` line and before the `projections` one:

  ```rust
  pub use git::{Git, GitError};
  ```

- [ ] Give the check a way to run a test that needs a program. In `xtask/src/main.rs`, replace

  ```rust
          Some("check") => check(&root),
  ```

  with:

  ```rust
          Some("check") => check(
              &root,
              match args.get(1).map(String::as_str) {
                  None => Tests::WithoutTheOnesThatNeedAProgram,
                  Some("--integration") => Tests::All,
                  Some(flag) => {
                      bail!("unknown flag {flag}; usage: cargo xtask check [--integration]")
                  }
              },
          ),
  ```

  replace the usage line

  ```rust
          _ => bail!(
              "usage: cargo xtask <check|generate [--check]|pre-commit|commit-msg <file>|todos|core-io|install-hooks>"
          ),
  ```

  with:

  ```rust
          _ => bail!(
              "usage: cargo xtask <check [--integration]|generate [--check]|pre-commit|commit-msg <file>|todos|core-io|install-hooks>"
          ),
  ```

  and replace `fn check`, whole, with:

  ```rust
  /// Which tests the check runs.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  enum Tests {
      /// Every test that needs nothing but a temporary directory. The default, so that the check runs
      /// on a machine without the programs the rest need.
      WithoutTheOnesThatNeedAProgram,
      /// Those, and the ones marked `#[ignore]` because they need `git` or Docker.
      All,
  }

  fn check(root: &Path, tests: Tests) -> anyhow::Result<()> {
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
      match tests {
          Tests::WithoutTheOnesThatNeedAProgram => cargo(root, &["test", "--workspace"])?,
          // `--include-ignored` rather than `--ignored`: this runs everything, so one command is the
          // whole check rather than half of it.
          Tests::All => cargo(root, &["test", "--workspace", "--", "--include-ignored"])?,
      }
      generate(root, true)?;
      todos(root)?;
      core_io(root)?;
      println!("xtask check: ok");
      Ok(())
  }
  ```

- [ ] Have CI run it. Replace `.github/workflows/check.yml`, whole, with:

  ```yaml
  name: check
  on:
    pull_request:
    push:
      branches: [main]
  jobs:
    # One job, running the check with `--integration`, because the runner has git and the flag's only
    # effect is to include the tests marked `#[ignore]` for needing it. A second job would compile the
    # workspace again to run a superset of what this one ran. When a test needs Docker (phase 3 step
    # 02), the separation starts paying for itself and this becomes two jobs.
    check:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v5
        - uses: dtolnay/rust-toolchain@stable
          with:
            toolchain: 1.98.1
            components: rustfmt, clippy
        - uses: Swatinem/rust-cache@v2
        - run: git --version
        - run: cargo xtask check --integration
  ```

- [ ] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 36 passed (the store's modules)
  #           test result: ok. 8 passed (event_log_file)
  #           test result: ok. 0 passed; 1 ignored (git)
  ```

- [ ] Run the check both ways, and confirm the flag is what runs the ignored test:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, the git test reported as 1 ignored
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, the git test reported as 1 passed
  cargo xtask check --nonsense
  # expected: xtask: unknown flag --nonsense; usage: cargo xtask check [--integration]
  ```

- [ ] Commit: `feat(store): open a repository, and run the tests that need git`

### Task 2: What is at the tip, and which branch

Files: modified `crates/store/src/git.rs`, `crates/store/src/lib.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: `Git`, `GitError`, `at_root`, `run_git` from Task 1
Produces: `farik_store::HeadSummary`, `Git::{head_summary, default_branch, current_branch}`

- [ ] Write the failing tests. Replace the whole tests module of `crates/store/src/git.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::{GitError, HeadSummary, default_branch_of, head_summary_of};

      /// What `git log --format=%H%x1f%cI%x1f%s` prints for one commit.
      fn a_log_line(subject: &str) -> String {
          format!("a1b2c3\u{1f}2026-09-17T10:00:00+00:00\u{1f}{subject}")
      }

      #[test]
      fn reads_a_commit_out_of_the_line_git_prints() {
          assert_eq!(
              head_summary_of(&a_log_line("feat(store): add a git adapter")),
              Some(HeadSummary {
                  sha: "a1b2c3".to_string(),
                  committed_at: "2026-09-17T10:00:00+00:00".to_string(),
                  subject: "feat(store): add a git adapter".to_string(),
              })
          );
      }

      #[test]
      fn reads_a_subject_that_holds_the_field_separator_is_not_possible() {
          // The separator is a unit separator, which git will not print inside a subject and no
          // commit message can hold: that is why it is the separator rather than a tab or a space.
          let summary = head_summary_of(&a_log_line("a subject\twith a tab"))
              .expect("a line with a tab in its subject is still a line");
          assert_eq!(summary.subject, "a subject\twith a tab");
      }

      #[test]
      fn refuses_a_line_that_is_not_a_commit() {
          for not_a_line in ["", "a1b2c3", "a1b2c3\u{1f}2026-09-17T10:00:00+00:00"] {
              assert_eq!(head_summary_of(not_a_line), None, "{not_a_line:?}");
          }
          assert_eq!(head_summary_of("\u{1f}\u{1f}a subject"), None, "no sha");
      }

      #[test]
      fn keeps_a_subject_that_is_empty() {
          // `git commit --allow-empty-message` makes one, and a board would rather show a commit with
          // no subject than no commit.
          let summary = head_summary_of(&a_log_line("")).expect("a commit with no subject");
          assert_eq!(summary.subject, "");
      }

      #[test]
      fn takes_the_remote_off_the_default_branch() {
          assert_eq!(default_branch_of("origin/main"), "main");
          assert_eq!(default_branch_of("upstream/trunk"), "trunk");
          // A branch whose own name holds a slash keeps the rest of it.
          assert_eq!(default_branch_of("origin/release/2.0"), "release/2.0");
          assert_eq!(default_branch_of("main"), "main", "no remote to take off");
      }

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              GitError::NotARepository,
              GitError::NotInstalled {
                  detail: "No such file or directory (os error 2)".to_string(),
              },
              GitError::CommandFailed {
                  command: "branch farik/FRK-1 main".to_string(),
                  stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no git repository here",
                  "git could not be run: No such file or directory (os error 2)",
                  "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                   exists",
              ]
          );
      }
  }
  ```

- [ ] Append to `crates/store/tests/git.rs`, a blank line between each of these and the test above it:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_the_commit_at_the_tip_and_says_when_there_is_none() {
      let empty = std::env::temp_dir().join(format!("farik-git-empty-{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&empty);
      std::fs::create_dir_all(&empty).expect("a directory");
      run_git(&empty, &["init", "-b", "main"]);
      assert_eq!(
          Git::open(empty.clone())
              .head_summary()
              .expect("the read works"),
          None,
          "a repository farik init has just made has no commit"
      );
      let _ = std::fs::remove_dir_all(&empty);

      let repository = TempRepo::new("head-summary");
      let summary = repository
          .adapter()
          .head_summary()
          .expect("the read works")
          .expect("a repository with a commit has one");
      assert_eq!(summary.sha, repository.git(&["rev-parse", "HEAD"]));
      assert_eq!(summary.subject, "the first commit");
      assert!(
          summary.committed_at.starts_with("20"),
          "an ISO 8601 date: {}",
          summary.committed_at
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn answers_with_the_branch_it_is_on_when_there_is_no_remote_to_ask() {
      // A repository with no remote records its default branch nowhere, so the branch HEAD is on is
      // the only answer there is; the team file is where a team says otherwise (5.14).
      let repository = TempRepo::new("default-branch");
      let git = repository.adapter();
      assert_eq!(git.current_branch().expect("the read works"), "main");
      assert_eq!(git.default_branch().expect("the read works"), "main");
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      assert_eq!(git.current_branch().expect("the read works"), "farik/FRK-1");
  }
  ```

- [ ] Run them and confirm they fail because nothing reads the tip:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice. These two codes and no others, the E0599s once per
  # call site (head_summary twice, current_branch twice, default_branch once):
  # error[E0432]: unresolved imports `super::HeadSummary`, `super::default_branch_of`,
  #   `super::head_summary_of`
  # error[E0599]: no method named `head_summary` found for struct `Git` in the current scope
  # error[E0599]: no method named `default_branch` found for struct `Git` in the current scope
  # error[E0599]: no method named `current_branch` found for struct `Git` in the current scope
  ```

- [ ] Write the minimal implementation. Insert into `crates/store/src/git.rs`, before the doc comment of `pub struct Git` — above it, not between it and the struct:

  ```rust
  /// What the tip of a branch is, as a board shows it.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct HeadSummary {
      /// The full commit hash.
      pub sha: String,
      /// When it was committed, as git's strict ISO 8601.
      pub committed_at: String,
      /// Its subject line.
      pub subject: String,
  }
  ```

  insert into `impl Git`, before `at_root`:

  ```rust
      /// The tip of the current branch, or nothing when the repository has no commit yet.
      ///
      /// # Errors
      ///
      /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses for another reason.
      pub fn head_summary(&self) -> Result<Option<HeadSummary>, GitError> {
          self.require_repository()?;
          if self
              .at_root(&["rev-parse", "--verify", "--quiet", "HEAD"])
              .is_err()
          {
              // A repository with no commit is the ordinary state of one `farik init` has just made.
              return Ok(None);
          }
          let line = self.at_root(&["log", "-1", "--no-color", "--format=%H%x1f%cI%x1f%s"])?;
          Ok(head_summary_of(&line))
      }
  ```

  then, still before `at_root`:

  ```rust
      /// The branch this repository treats as its default: what `origin/HEAD` points at.
      ///
      /// # Errors
      ///
      /// `NotARepository`, `NotInstalled`, or `CommandFailed` when even the current branch cannot be
      /// read.
      pub fn default_branch(&self) -> Result<String, GitError> {
          self.require_repository()?;
          match self.at_root(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
              // A repository with no remote records its default branch nowhere at all, so the branch
              // HEAD is on is the only answer available. `.farik/team.yaml` is where a team says
              // otherwise, and the integration branch is its to set (5.14).
              Err(_) => self.current_branch(),
              Ok(reference) => Ok(default_branch_of(&reference)),
          }
      }
  ```

  then, still before `at_root`:

  ```rust
      /// The branch that is checked out.
      ///
      /// # Errors
      ///
      /// `NotARepository`, `NotInstalled`, or `CommandFailed`; a detached head refuses, because a
      /// detached head is not a branch.
      pub fn current_branch(&self) -> Result<String, GitError> {
          self.require_repository()?;
          self.at_root(&["symbolic-ref", "--short", "HEAD"])
      }
  ```

  then, still before `at_root`, the check every method makes first:

  ```rust
      /// Refuses before running anything when there is no repository, so that every method says the
      /// same thing about it rather than each passing on whatever git happened to print.
      fn require_repository(&self) -> Result<(), GitError> {
          if self.is_repository() {
              Ok(())
          } else {
              Err(GitError::NotARepository)
          }
      }
  ```

  and append, after `run_git`:

  ```rust
  /// One `git log` line as a summary, or nothing when it is not one.
  fn head_summary_of(line: &str) -> Option<HeadSummary> {
      let mut fields = line.split('\u{1f}');
      let sha = fields.next()?;
      let committed_at = fields.next()?;
      let subject = fields.next()?;
      if sha.is_empty() || committed_at.is_empty() {
          return None;
      }
      Some(HeadSummary {
          sha: sha.to_string(),
          committed_at: committed_at.to_string(),
          subject: subject.to_string(),
      })
  }
  ```

  ```rust
  /// `origin/main` as `main`: the remote's name is not part of the branch's.
  fn default_branch_of(reference: &str) -> String {
      reference
          .split_once('/')
          .map_or_else(|| reference.to_string(), |(_, branch)| branch.to_string())
  }
  ```

- [ ] Add `HeadSummary` to the re-export in `crates/store/src/lib.rs`, which becomes:

  ```rust
  pub use git::{Git, GitError, HeadSummary};
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 41 passed (the store's modules)
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 3 passed (git)
  ```

- [ ] Commit: `feat(store): read the tip of a branch and which branch is which`

### Task 3: A branch and a worktree for every task

Files: modified `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: `Git`, `require_repository`, `at_root` from Tasks 1 and 2
Produces: `Git::{create_branch, create_worktree, remove_worktree, is_clean}`

- [ ] Write the failing tests. Replace the whole tests module of `crates/store/src/git.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::path::PathBuf;

      use super::{GitError, HeadSummary, default_branch_of, head_summary_of, path_argument};

      /// What `git log --format=%H%x1f%cI%x1f%s` prints for one commit.
      fn a_log_line(subject: &str) -> String {
          format!("a1b2c3\u{1f}2026-09-17T10:00:00+00:00\u{1f}{subject}")
      }

      #[test]
      fn reads_a_commit_out_of_the_line_git_prints() {
          assert_eq!(
              head_summary_of(&a_log_line("feat(store): add a git adapter")),
              Some(HeadSummary {
                  sha: "a1b2c3".to_string(),
                  committed_at: "2026-09-17T10:00:00+00:00".to_string(),
                  subject: "feat(store): add a git adapter".to_string(),
              })
          );
      }

      #[test]
      fn reads_a_subject_that_holds_the_field_separator_is_not_possible() {
          // The separator is a unit separator, which git will not print inside a subject and no
          // commit message can hold: that is why it is the separator rather than a tab or a space.
          let summary = head_summary_of(&a_log_line("a subject\twith a tab"))
              .expect("a line with a tab in its subject is still a line");
          assert_eq!(summary.subject, "a subject\twith a tab");
      }

      #[test]
      fn refuses_a_line_that_is_not_a_commit() {
          for not_a_line in ["", "a1b2c3", "a1b2c3\u{1f}2026-09-17T10:00:00+00:00"] {
              assert_eq!(head_summary_of(not_a_line), None, "{not_a_line:?}");
          }
          assert_eq!(head_summary_of("\u{1f}\u{1f}a subject"), None, "no sha");
      }

      #[test]
      fn keeps_a_subject_that_is_empty() {
          // `git commit --allow-empty-message` makes one, and a board would rather show a commit with
          // no subject than no commit.
          let summary = head_summary_of(&a_log_line("")).expect("a commit with no subject");
          assert_eq!(summary.subject, "");
      }

      #[test]
      fn takes_the_remote_off_the_default_branch() {
          assert_eq!(default_branch_of("origin/main"), "main");
          assert_eq!(default_branch_of("upstream/trunk"), "trunk");
          // A branch whose own name holds a slash keeps the rest of it.
          assert_eq!(default_branch_of("origin/release/2.0"), "release/2.0");
          assert_eq!(default_branch_of("main"), "main", "no remote to take off");
      }

      #[test]
      fn refuses_a_path_that_is_not_text_git_can_be_given() {
          assert!(path_argument(&PathBuf::from("crates/store")).is_ok());
          #[cfg(unix)]
          {
              use std::ffi::OsString;
              use std::os::unix::ffi::OsStringExt;
              let not_text = PathBuf::from(OsString::from_vec(vec![0xff, 0xfe]));
              assert!(matches!(
                  path_argument(&not_text),
                  Err(GitError::CommandFailed { .. })
              ));
          }
      }

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              GitError::NotARepository,
              GitError::NotInstalled {
                  detail: "No such file or directory (os error 2)".to_string(),
              },
              GitError::CommandFailed {
                  command: "branch farik/FRK-1 main".to_string(),
                  stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no git repository here",
                  "git could not be run: No such file or directory (os error 2)",
                  "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                   exists",
              ]
          );
      }
  }
  ```

- [ ] The integration test names `GitError` now, so change the import in `crates/store/tests/git.rs` to:

  ```rust
  use farik_store::{Git, GitError};
  ```

  and append to that file, a blank line between it and the test above it:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn makes_a_branch_and_a_worktree_for_a_task_and_takes_the_worktree_away_again() {
      // One worktree per task is what keeps two tasks from sharing a working tree (5.14), and the
      // branch outlives it: the work is on the branch, not in the directory.
      let repository = TempRepo::new("worktree");
      let git = repository.adapter();
      git.create_branch("farik/FRK-1", "main")
          .expect("the branch is made");
      assert_eq!(
          git.create_branch("farik/FRK-1", "main"),
          Err(GitError::CommandFailed {
              command: "branch farik/FRK-1 main".to_string(),
              stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
          }),
          "a name is taken once"
      );

      let worktree = repository.path.join(".farik/local/worktrees/FRK-2");
      git.create_worktree(&worktree, "farik/FRK-2", "main")
          .expect("the worktree is made");
      assert!(
          worktree.join("README.md").is_file(),
          "it has the work in it"
      );
      assert!(git.is_clean(&worktree).expect("the read works"));

      std::fs::write(worktree.join("built-by-the-session.txt"), "output\n")
          .expect("something the session built");
      assert!(
          !git.is_clean(&worktree).expect("the read works"),
          "an untracked file is not clean either"
      );
      // Forced, because that untracked file is exactly what a finished task leaves behind.
      git.remove_worktree(&worktree).expect("it is taken away");
      assert!(!worktree.exists());
      assert!(
          repository
              .git(&["branch", "--list", "farik/FRK-2"])
              .contains("farik/FRK-2"),
          "and the branch is kept"
      );
  }
  ```

- [ ] Run them and confirm they fail because no worktree can be made:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::path_argument`
  # error: could not compile `farik-store` (lib test) due to 1 previous error
  # (cargo stops there; the integration test's own errors — no method named
  # create_branch, create_worktree, is_clean, remove_worktree — follow once it compiles)
  ```

- [ ] Write the minimal implementation. Insert into `impl Git`, before `require_repository`:

  ```rust
      /// Makes a branch at `from`, without checking it out.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when the name is taken or `from` names nothing.
      pub fn create_branch(&self, name: &str, from: &str) -> Result<(), GitError> {
          self.require_repository()?;
          self.at_root(&["branch", name, from])?;
          Ok(())
      }
  ```

  then, still before `require_repository`:

  ```rust
      /// Makes a worktree at `path` on a new branch `branch`, starting from `from`.
      ///
      /// One worktree per task is what keeps two tasks from sharing a working tree (5.14).
      ///
      /// # Errors
      ///
      /// `CommandFailed` when the path is taken, the branch exists, or `from` names nothing.
      pub fn create_worktree(&self, path: &Path, branch: &str, from: &str) -> Result<(), GitError> {
          self.require_repository()?;
          let path = path_argument(path)?;
          self.at_root(&["worktree", "add", "-b", branch, &path, from])?;
          Ok(())
      }
  ```

  then, still before `require_repository`:

  ```rust
      /// Removes the worktree at `path`, keeping its branch.
      ///
      /// Forced, because a task's worktree holds whatever its session built — untracked output that
      /// git would otherwise refuse to remove, and that nothing wants kept once the task is done.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when no worktree is registered at `path`.
      pub fn remove_worktree(&self, path: &Path) -> Result<(), GitError> {
          self.require_repository()?;
          let path = path_argument(path)?;
          self.at_root(&["worktree", "remove", "--force", &path])?;
          Ok(())
      }
  ```

  then, still before `require_repository`:

  ```rust
      /// Whether the tree at `path` has nothing uncommitted, untracked files included.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when `path` is not a working tree of this repository.
      pub fn is_clean(&self, path: &Path) -> Result<bool, GitError> {
          self.require_repository()?;
          Ok(run_git(path, &["status", "--porcelain"])?.is_empty())
      }
  ```

  and append, after `default_branch_of`:

  ```rust
  /// A path as git takes it, refusing one that is not text.
  ///
  /// Every path Farik hands git it built itself, from `.farik/local/worktrees/` and a task id, so
  /// this is about a repository somewhere a user's own path is not UTF-8.
  fn path_argument(path: &Path) -> Result<String, GitError> {
      path.to_str()
          .map(ToString::to_string)
          .ok_or_else(|| GitError::CommandFailed {
              command: "worktree".to_string(),
              stderr: format!("the path {} is not text git can be given", path.display()),
          })
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 42 passed (the store's modules)
  #   test result: ok. 4 passed (git)
  ```

- [ ] Commit: `feat(store): make a branch and a worktree for a task`

### Task 4: What a branch changed

Files: modified `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: `Git`, `require_repository`, `at_root` from Tasks 1 and 2
Produces: `Git::{commit_count, changed_paths, diff}`

- [ ] Write the failing tests. Replace the whole tests module of `crates/store/src/git.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::path::PathBuf;

      use super::{
          GitError, HeadSummary, changed_paths_of, default_branch_of, head_summary_of,
          path_argument,
      };

      /// What `git log --format=%H%x1f%cI%x1f%s` prints for one commit.
      fn a_log_line(subject: &str) -> String {
          format!("a1b2c3\u{1f}2026-09-17T10:00:00+00:00\u{1f}{subject}")
      }

      #[test]
      fn reads_a_commit_out_of_the_line_git_prints() {
          assert_eq!(
              head_summary_of(&a_log_line("feat(store): add a git adapter")),
              Some(HeadSummary {
                  sha: "a1b2c3".to_string(),
                  committed_at: "2026-09-17T10:00:00+00:00".to_string(),
                  subject: "feat(store): add a git adapter".to_string(),
              })
          );
      }

      #[test]
      fn reads_a_subject_that_holds_the_field_separator_is_not_possible() {
          // The separator is a unit separator, which git will not print inside a subject and no
          // commit message can hold: that is why it is the separator rather than a tab or a space.
          let summary = head_summary_of(&a_log_line("a subject\twith a tab"))
              .expect("a line with a tab in its subject is still a line");
          assert_eq!(summary.subject, "a subject\twith a tab");
      }

      #[test]
      fn refuses_a_line_that_is_not_a_commit() {
          for not_a_line in ["", "a1b2c3", "a1b2c3\u{1f}2026-09-17T10:00:00+00:00"] {
              assert_eq!(head_summary_of(not_a_line), None, "{not_a_line:?}");
          }
          assert_eq!(head_summary_of("\u{1f}\u{1f}a subject"), None, "no sha");
      }

      #[test]
      fn keeps_a_subject_that_is_empty() {
          // `git commit --allow-empty-message` makes one, and a board would rather show a commit with
          // no subject than no commit.
          let summary = head_summary_of(&a_log_line("")).expect("a commit with no subject");
          assert_eq!(summary.subject, "");
      }

      #[test]
      fn takes_the_remote_off_the_default_branch() {
          assert_eq!(default_branch_of("origin/main"), "main");
          assert_eq!(default_branch_of("upstream/trunk"), "trunk");
          // A branch whose own name holds a slash keeps the rest of it.
          assert_eq!(default_branch_of("origin/release/2.0"), "release/2.0");
          assert_eq!(default_branch_of("main"), "main", "no remote to take off");
      }

      #[test]
      fn reads_the_paths_git_separated_by_nothing() {
          assert_eq!(
              changed_paths_of("src/lib.rs\0docs/SPEC.md\0"),
              ["src/lib.rs", "docs/SPEC.md"]
          );
          assert_eq!(changed_paths_of(""), Vec::<String>::new());
          // A newline in a path is what `-z` is for: nothing here splits on one.
          assert_eq!(
              changed_paths_of("a file\nwith a newline\0other.rs\0"),
              ["a file\nwith a newline", "other.rs"]
          );
      }

      #[test]
      fn refuses_a_path_that_is_not_text_git_can_be_given() {
          assert!(path_argument(&PathBuf::from("crates/store")).is_ok());
          #[cfg(unix)]
          {
              use std::ffi::OsString;
              use std::os::unix::ffi::OsStringExt;
              let not_text = PathBuf::from(OsString::from_vec(vec![0xff, 0xfe]));
              assert!(matches!(
                  path_argument(&not_text),
                  Err(GitError::CommandFailed { .. })
              ));
          }
      }

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              GitError::NotARepository,
              GitError::NotInstalled {
                  detail: "No such file or directory (os error 2)".to_string(),
              },
              GitError::CommandFailed {
                  command: "branch farik/FRK-1 main".to_string(),
                  stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no git repository here",
                  "git could not be run: No such file or directory (os error 2)",
                  "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                   exists",
              ]
          );
      }
  }
  ```

- [ ] Append to `crates/store/tests/git.rs`, a blank line between each of these and the test above it:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn counts_the_commits_a_branch_added_and_names_every_path_it_touched() {
      let repository = TempRepo::new("changed-paths");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      repository.write("src/added.rs", "fn added() {}\n");
      repository.write("README.md", "the first line\nand a second\n");
      repository.commit("feat: add a thing");
      repository.git(&["rm", "--quiet", "README.md"]);
      repository.commit("feat: and take one away");

      assert_eq!(git.commit_count("main", "farik/FRK-1"), Ok(2));
      assert_eq!(
          git.commit_count("farik/FRK-1", "main"),
          Ok(0),
          "the other way"
      );
      let mut changed = git
          .changed_paths("main", "farik/FRK-1")
          .expect("the read works");
      changed.sort();
      assert_eq!(changed, ["README.md", "src/added.rs"]);
      let patch = git.diff("main", "farik/FRK-1").expect("the read works");
      assert!(patch.contains("fn added()"), "the patch holds the change");
      assert!(patch.contains("--- a/README.md"), "and the removal");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn names_both_sides_of_a_file_that_moved() {
      // The governor asks about each path a change touched (5.6), and a move touches two: a rename
      // reported only by its new name would let work land at a path nobody allowed.
      let repository = TempRepo::new("renames");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      std::fs::create_dir_all(repository.path.join("docs")).expect("the directory it moves into");
      repository.git(&["mv", "README.md", "docs/README.md"]);
      repository.commit("docs: move the readme");

      let mut changed = git
          .changed_paths("main", "farik/FRK-1")
          .expect("the read works");
      changed.sort();
      assert_eq!(changed, ["README.md", "docs/README.md"]);
  }
  ```

- [ ] Run them and confirm they fail because nothing reads a range:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice. These two codes and no others, the E0599s once per
  # call site (commit_count twice, changed_paths twice, diff once):
  # error[E0432]: unresolved import `super::changed_paths_of`
  # error[E0599]: no method named `commit_count` found for struct `Git` in the current scope
  # error[E0599]: no method named `changed_paths` found for struct `Git` in the current scope
  # error[E0599]: no method named `diff` found for struct `Git` in the current scope
  ```

- [ ] Write the minimal implementation. Insert into `impl Git`, before `require_repository`:

  ```rust
      /// How many commits `head` has that `base` does not.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown; `InvalidCount` never — git counts.
      pub fn commit_count(&self, base: &str, head: &str) -> Result<u32, GitError> {
          self.require_repository()?;
          let range = format!("{base}..{head}");
          let counted = self.at_root(&["rev-list", "--count", &range])?;
          counted.parse().map_err(|_| GitError::CommandFailed {
              command: format!("rev-list --count {range}"),
              stderr: format!("answered {counted:?}, which is not a number of commits"),
          })
      }
  ```

  then, still before `require_repository`:

  ```rust
      /// Every path `head` changed since it and `base` last agreed.
      ///
      /// Renames are reported as both sides, a removal and an addition, because the governor's
      /// allowed-paths rule is asked about each of them separately (`docs/SPEC.md` 5.6).
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown.
      pub fn changed_paths(&self, base: &str, head: &str) -> Result<Vec<String>, GitError> {
          self.require_repository()?;
          let range = format!("{base}...{head}");
          let listed = self.at_root(&["diff", "--no-renames", "--name-only", "-z", &range])?;
          Ok(changed_paths_of(&listed))
      }
  ```

  then, still before `require_repository`:

  ```rust
      /// What `head` changed since it and `base` last agreed, as a patch.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown.
      pub fn diff(&self, base: &str, head: &str) -> Result<String, GitError> {
          self.require_repository()?;
          let range = format!("{base}...{head}");
          self.at_root(&["diff", "--no-color", &range])
      }
  ```

  and append, after `path_argument`:

  ```rust
  /// git's `-z` output as paths. Nothing is split on a newline, because a path may hold one.
  fn changed_paths_of(listed: &str) -> Vec<String> {
      listed
          .split('\0')
          .filter(|path| !path.is_empty())
          .map(ToString::to_string)
          .collect()
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 43 passed (the store's modules)
  #   test result: ok. 6 passed (git)
  ```

- [ ] Commit: `feat(store): say what a branch changed and how much`

### Task 5: Merging a finished task

Files: modified `crates/store/src/git.rs`, `crates/store/src/lib.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: everything above
Produces: `farik_store::MergeOutcome`, `Git::merge`

- [ ] Write the failing tests. Replace the whole tests module of `crates/store/src/git.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use std::path::PathBuf;

      use super::{
          Git, GitError, HeadSummary, changed_paths_of, default_branch_of, head_summary_of,
          path_argument,
      };

      /// What `git log --format=%H%x1f%cI%x1f%s` prints for one commit.
      fn a_log_line(subject: &str) -> String {
          format!("a1b2c3\u{1f}2026-09-17T10:00:00+00:00\u{1f}{subject}")
      }

      #[test]
      fn reads_a_commit_out_of_the_line_git_prints() {
          assert_eq!(
              head_summary_of(&a_log_line("feat(store): add a git adapter")),
              Some(HeadSummary {
                  sha: "a1b2c3".to_string(),
                  committed_at: "2026-09-17T10:00:00+00:00".to_string(),
                  subject: "feat(store): add a git adapter".to_string(),
              })
          );
      }

      #[test]
      fn reads_a_subject_that_holds_the_field_separator_is_not_possible() {
          // The separator is a unit separator, which git will not print inside a subject and no
          // commit message can hold: that is why it is the separator rather than a tab or a space.
          let summary = head_summary_of(&a_log_line("a subject\twith a tab"))
              .expect("a line with a tab in its subject is still a line");
          assert_eq!(summary.subject, "a subject\twith a tab");
      }

      #[test]
      fn refuses_a_line_that_is_not_a_commit() {
          for not_a_line in ["", "a1b2c3", "a1b2c3\u{1f}2026-09-17T10:00:00+00:00"] {
              assert_eq!(head_summary_of(not_a_line), None, "{not_a_line:?}");
          }
          assert_eq!(head_summary_of("\u{1f}\u{1f}a subject"), None, "no sha");
      }

      #[test]
      fn keeps_a_subject_that_is_empty() {
          // `git commit --allow-empty-message` makes one, and a board would rather show a commit with
          // no subject than no commit.
          let summary = head_summary_of(&a_log_line("")).expect("a commit with no subject");
          assert_eq!(summary.subject, "");
      }

      #[test]
      fn takes_the_remote_off_the_default_branch() {
          assert_eq!(default_branch_of("origin/main"), "main");
          assert_eq!(default_branch_of("upstream/trunk"), "trunk");
          // A branch whose own name holds a slash keeps the rest of it.
          assert_eq!(default_branch_of("origin/release/2.0"), "release/2.0");
          assert_eq!(default_branch_of("main"), "main", "no remote to take off");
      }

      #[test]
      fn reads_the_paths_git_separated_by_nothing() {
          assert_eq!(
              changed_paths_of("src/lib.rs\0docs/SPEC.md\0"),
              ["src/lib.rs", "docs/SPEC.md"]
          );
          assert_eq!(changed_paths_of(""), Vec::<String>::new());
          // A newline in a path is what `-z` is for: nothing here splits on one.
          assert_eq!(
              changed_paths_of("a file\nwith a newline\0other.rs\0"),
              ["a file\nwith a newline", "other.rs"]
          );
      }

      #[test]
      fn refuses_a_path_that_is_not_text_git_can_be_given() {
          assert!(path_argument(&PathBuf::from("crates/store")).is_ok());
          #[cfg(unix)]
          {
              use std::ffi::OsString;
              use std::os::unix::ffi::OsStringExt;
              let not_text = PathBuf::from(OsString::from_vec(vec![0xff, 0xfe]));
              assert!(matches!(
                  path_argument(&not_text),
                  Err(GitError::CommandFailed { .. })
              ));
          }
      }

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              GitError::NotARepository,
              GitError::NotInstalled {
                  detail: "No such file or directory (os error 2)".to_string(),
              },
              GitError::CommandFailed {
                  command: "branch farik/FRK-1 main".to_string(),
                  stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no git repository here",
                  "git could not be run: No such file or directory (os error 2)",
                  "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                   exists",
              ]
          );
      }

      #[test]
      fn says_there_is_no_repository_before_it_runs_anything() {
          // Every method asks first, so that a user is told the one thing that is wrong rather than
          // whatever git prints about a directory it has never heard of.
          let nowhere = Git::open(std::env::temp_dir().join("farik-not-a-repository-at-all"));
          assert!(!nowhere.is_repository());
          assert_eq!(nowhere.head_summary(), Err(GitError::NotARepository));
          assert_eq!(nowhere.default_branch(), Err(GitError::NotARepository));
          assert_eq!(nowhere.current_branch(), Err(GitError::NotARepository));
          assert_eq!(
              nowhere.create_branch("farik/FRK-1", "main"),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.commit_count("main", "farik/FRK-1"),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.changed_paths("main", "farik/FRK-1"),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.diff("main", "farik/FRK-1"),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.merge("main", "farik/FRK-1", "a message"),
              Err(GitError::NotARepository)
          );
      }
  }
  ```

- [ ] The integration test names `MergeOutcome` now, so change the import in `crates/store/tests/git.rs` to:

  ```rust
  use farik_store::{Git, GitError, MergeOutcome};
  ```

  and append to that file, a blank line between each of these and the test above it:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn merges_a_finished_task_into_the_integration_branch() {
      let repository = TempRepo::new("merge");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      repository.write("src/added.rs", "fn added() {}\n");
      repository.commit("feat: add a thing");
      repository.git(&["checkout", "main"]);

      let outcome = git
          .merge("main", "farik/FRK-1", "integrate FRK-1")
          .expect("the merge runs");
      let MergeOutcome::Merged { sha } = outcome else {
          panic!("it merged: {outcome:?}");
      };
      assert_eq!(sha, repository.git(&["rev-parse", "HEAD"]));
      assert_eq!(
          repository.git(&["log", "-1", "--format=%s"]),
          "integrate FRK-1",
          "with a merge commit, so the task's commits survive"
      );
      assert_eq!(
          repository.git(&["rev-list", "--count", "--merges", "HEAD"]),
          "1"
      );
      assert!(repository.path.join("src/added.rs").is_file());
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn names_what_conflicted_and_leaves_the_tree_as_it_was() {
      // A conflict escalates the task with reason `integration` (5.14). What it must not do is leave
      // a half-merged working tree behind for the next command to trip over.
      let repository = TempRepo::new("conflict");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      repository.write("README.md", "the branch's line\n");
      repository.commit("docs: the branch writes it");
      repository.git(&["checkout", "main"]);
      repository.write("README.md", "main's line\n");
      repository.commit("docs: main writes it too");
      let before = repository.git(&["rev-parse", "HEAD"]);

      let outcome = git
          .merge("main", "farik/FRK-1", "integrate FRK-1")
          .expect("the merge runs and reports");
      assert_eq!(
          outcome,
          MergeOutcome::Conflicts(vec!["README.md".to_string()])
      );
      assert_eq!(
          repository.git(&["rev-parse", "HEAD"]),
          before,
          "nothing was committed"
      );
      assert!(
          git.is_clean(&repository.path).expect("the read works"),
          "and nothing was left half-merged"
      );
      assert_eq!(
          std::fs::read_to_string(repository.path.join("README.md")).expect("the file reads"),
          "main's line\n",
          "the integration branch's own work is untouched"
      );
  }
  ```

- [ ] Run them and confirm they fail because nothing merges:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (lib test) due to 1 previous error
  ```

- [ ] Write the minimal implementation. Insert into `crates/store/src/git.rs`, before the doc comment of `pub struct Git`:

  ```rust
  /// What came of a merge.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum MergeOutcome {
      /// It merged, leaving this commit.
      Merged {
          /// The merge commit's hash.
          sha: String,
      },
      /// It did not merge, and these paths are why. The tree is left as it was: a conflicted
      /// working tree nobody is watching is worse than a refusal (`docs/SPEC.md` 5.14).
      Conflicts(Vec<String>),
  }
  ```

  and insert into `impl Git`, before `require_repository`:

  ```rust
      /// Merges `from` into `into` with a merge commit, or reports what conflicted.
      ///
      /// A conflict leaves nothing behind: the merge is undone before this returns, because the tree
      /// it would leave is one nobody is watching and every later command would trip over it. The
      /// task is escalated with reason `integration` instead (5.14).
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown, or when the tree is not clean enough to switch
      /// branches.
      pub fn merge(&self, into: &str, from: &str, message: &str) -> Result<MergeOutcome, GitError> {
          self.require_repository()?;
          self.at_root(&["checkout", into])?;
          match self.at_root(&["merge", "--no-ff", "-m", message, from]) {
              Ok(_) => Ok(MergeOutcome::Merged {
                  sha: self.at_root(&["rev-parse", "HEAD"])?,
              }),
              Err(refusal) => {
                  let conflicted = self.at_root(&["diff", "--name-only", "--diff-filter=U", "-z"])?;
                  let conflicts = changed_paths_of(&conflicted);
                  if conflicts.is_empty() {
                      // It refused for some other reason, and that reason is the answer.
                      return Err(refusal);
                  }
                  self.at_root(&["merge", "--abort"])?;
                  Ok(MergeOutcome::Conflicts(conflicts))
              }
          }
      }
  ```

- [ ] Add `MergeOutcome` to the re-export in `crates/store/src/lib.rs`, which becomes:

  ```rust
  pub use git::{Git, GitError, HeadSummary, MergeOutcome};
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 44 passed (the store's modules)
  #   test result: ok. 8 passed (git)
  ```

- [ ] Commit: `feat(store): merge a finished task or say what conflicted`

### Task 6: The plans say what the adapter became

Files: modified `docs/plans/project-plan.md`, `docs/standards/code.md`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: everything above
Produces: a project plan and a standard that describe the adapter and the check as they now are

This task changes documentation and has no test cycle. The `> ` marker on each block below is this plan's and is not part of the text to write.

- [ ] In `docs/plans/project-plan.md`, replace the line beginning `- Step 04 (\`farik-store::git\`):` with:

  > - Step 04 (`farik-store::git`): `enum GitError { NotARepository, NotInstalled { detail }, CommandFailed { command, stderr } }` (the middle one added 2026-09-17 by the step 04 plan: folding "the git program is not on this machine" into `CommandFailed` would report an empty `stderr` for a command that never ran); `struct Git { root: PathBuf }`; `impl Git { fn open(root: PathBuf) -> Git; fn is_repository(&self) -> bool; fn head_summary(&self) -> Result<Option<HeadSummary>, GitError>; fn default_branch(&self) -> Result<String, GitError>; fn current_branch(&self) -> Result<String, GitError>; fn create_branch(&self, name, from) -> Result<(), GitError>; fn create_worktree(&self, path, branch, from) -> Result<(), GitError>; fn remove_worktree(&self, path) -> Result<(), GitError>; fn is_clean(&self, path) -> Result<bool, GitError>; fn commit_count(&self, base, head) -> Result<u32, GitError>; fn changed_paths(&self, base, head) -> Result<Vec<String>, GitError>; fn diff(&self, base, head) -> Result<String, GitError>; fn merge(&self, into, from, message) -> Result<MergeOutcome, GitError> }`; `struct HeadSummary { sha, committed_at, subject }`; `enum MergeOutcome { Merged { sha }, Conflicts(Vec<String>) }`. Every method runs the `git` program; every one asks `is_repository` first and answers `NotARepository` itself rather than passing on what git printed. `changed_paths` and `diff` take the three-dot range `base...head` and `commit_count` the two-dot `base..head`; a rename comes back as both of its paths, because 5.6 asks about each path a change touched. A conflicted merge is undone before `merge` returns. Phase 3 step 03 adds `commit` and `push`.

- [ ] In `docs/plans/project-plan.md`, in the "Tests are split in three" bullet, replace `which runs in CI as a second job of the same \`check\` workflow from the step that adds the first test needing it (phase 2 step 04)` with:

  > which CI runs as the one job it has, because `--integration` only adds the ignored tests to the same `cargo test` invocation and a second job would compile the workspace again to run a superset of the first (changed 2026-09-17 by the step 04 plan; a second job starts paying for itself when a test needs Docker, which is phase 3 step 02)

- [ ] In `docs/plans/project-plan.md`, in the phase 2 step table, replace the last cell of the step 04 row — `Repository queries, branches, worktrees, changed paths, diff, clean check, commit count, merge; the first integration test that needs a git binary, and the CI job for \`cargo xtask check --integration\`` — with:

  > Repository queries, branches, worktrees, changed paths, diff, clean check, commit count, merge; the first tests that need a git binary, and the `--integration` flag CI runs them with

- [ ] In `docs/standards/code.md`, replace the note on the continuous integration row — `One workflow, \`check\`, runs \`cargo xtask check\` on every pull request and on \`main\`.` — with:

  > One workflow, `check`, runs `cargo xtask check --integration` on every pull request and on `main`. The flag adds the tests marked `#[ignore]` for needing a program the runner has; without it the same command runs everything else.

- [ ] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [ ] Commit: `docs(docs): record what step 04 changed about the adapter`

## Verification

- [ ] The whole check, from the workspace root:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 225 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 44 passed (farik-store, its four modules)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 8 passed (crates/store/tests/git.rs)
  #   test result: ok. 24 passed (xtask)
  ```

- [ ] And without the flag, to confirm the gate is a gate:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with crates/store/tests/git.rs reported as
  #   test result: ok. 0 passed; 0 failed; 8 ignored
  ```

- [ ] Every commit subject is accepted:

  ```
  for subject in \\
    "feat(store): open a repository, and run the tests that need git" \\
    "feat(store): read the tip of a branch and which branch is which" \\
    "feat(store): make a branch and a worktree for a task" \\
    "feat(store): say what a branch changed and how much" \\
    "feat(store): merge a finished task or say what conflicted" \\
    "docs(docs): record what step 04 changed about the adapter"; do
    printf '%s\\n' "$subject" > /tmp/subject && cargo xtask commit-msg /tmp/subject
  done
  # expected: silent, six times
  ```

## Open questions

none
