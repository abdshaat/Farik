# Phase 2, step 04: The git adapter

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 8.1 (the git adapter lives in `farik-store`), 5.14 (a branch and a worktree per task, and what integration does), 5.6 (allowed paths, which is what asks about a change's paths); `docs/standards/code.md`, "Rust integration test"
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01, 02 and 03 of this phase (committed as 1f93550, 50b264e and e99deae)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a review session that did not write this plan, on 2026-09-17, against `1f19b6b`. It confirmed the three rules the only way that proves them — by rebuilding the whole step from this plan alone in a fresh copy outside the working tree, with no corrections: every predicted red exact, every green exact, `cargo fmt --all --check` silent at all twelve points, and the changed-file set exactly the File map's ten. Three earlier rounds refused, and each refusal was real: a task whose own green command failed at the format check before a test ran; prose that contradicted the code block beside it; and an assertion that, added to prove one flag, silently stopped another assertion from testing anything. The fourth round's one finding — a global `core.attributesFile` marking `*.rs -diff` turning a patch into "Binary files differ" — is taken below, in `TempRepo::new`, before the first commit; it changes no count and no red. One thing is deferred rather than taken: that `docs/plans/step-template.md` should carry the rule about proving a flag in its own fixture. It is a file this step does not touch, and it belongs with step 05, where the template is next used.

## Goal

Farik can drive the repository it lives in. `Git` answers what is at the tip of a branch and which branch is the default, makes the branch and the worktree a task works in and takes the worktree away again, says what a branch changed and how many commits it took, and merges a finished task into the integration branch or says what conflicted without leaving a half-merged tree behind. Everything runs the `git` program as a child process: a repository is the user's own, and the only behaviour anyone can rely on is the program's.

This step also gives the repository a way to run the tests that need a real repository to work in. Eleven of the tests here do, so they are marked `#[ignore]` and run by `cargo xtask check --integration`, which is what CI runs.

## Decisions

- Every operation shells out to `git` rather than using a library: `docs/SPEC.md` 8.1 says so, and the reason is that a repository belongs to the user — `git status` in their terminal and Farik's must agree, and the only way to guarantee that is to ask the same program.
- The tests that need `git` are marked `#[ignore]`, not put behind a cargo feature and not skipped by an environment variable at run time: chose `#[ignore]` because the default check still *compiles* them, so `cargo fmt` and `clippy` hold them to the same standard and they cannot rot, and because `cargo test` prints them as ignored rather than passing silently. A feature would hide them from the default build; an environment variable would make a test that ran nothing look like a test that passed.
- The gate is not about a machine without `git`, and nothing in this step claims it is. `cargo xtask check` has needed `git` since phase 0: `tracked_files` shells out to `git ls-files`, so the bare-TODO check and the no-I/O check both fail without it. What the gate is about is a test that needs a *repository* to work in — slow, and writing to the file system — and the flag is what says to run them.
- `cargo xtask check --integration` adds `--include-ignored` to the test run and changes nothing else, so one command is the whole check rather than half of it.
- **CI runs one job, not two.** The project plan records this step as adding "the CI job for `cargo xtask check --integration`", a second job in the `check` workflow. A second job would compile the workspace again to run a strict superset of what the first ran, because `--integration` only adds the ignored tests to the same `cargo test` invocation, and `ubuntu-latest` already has `git`. The workflow runs `cargo xtask check --integration` in the one job it has, and carries a comment saying that a second job starts paying for itself when a test needs Docker, which is phase 3 step 02. Task 6 records the change. What is lost, and the workflow's own comment says so in the block task 1 writes: nothing in CI runs `cargo xtask check` *without* the flag any more, so a break in the flagless path — a test wrongly marked `#[ignore]` and therefore run by nobody's default command, or the `Tests::WithoutTheOnesThatNeedAProgram` arm itself — would reach `main` unseen. It is the command every contributor runs, and after this step it is guarded only by them running it. A second `cargo test --workspace` step in the same job was considered and refused: it would run the whole suite again to cover a difference of eleven tests, and `xtask::check`'s three unit tests already pin all three arms of the parsing.
- `GitError` gains a third variant, `NotInstalled { detail }`: the recorded interface has `NotARepository` and `CommandFailed { command, stderr }`, and neither says "the `git` program is not on this machine". Folding that into `CommandFailed` would report an empty `stderr` for a command that never ran. Task 6 records it.
- Every method asks `require_repository` first and answers `NotARepository` itself, rather than passing on whatever git printed about a directory it has never heard of. `farik init` is what reports this to a user, and it should read the same whichever method noticed.
- `changed_paths` passes `--no-renames`, so a move comes back as the old path and the new one: the governor is asked about each path a change touched (5.6), and a rename reported only by its new name would let work land at a path nobody allowed.
- `changed_paths` and `diff` use the three-dot range `base...head` — what the branch did since it and `base` last agreed — while `commit_count` uses two dots, `base..head`, which is how many commits `head` has that `base` does not. A reviewer reading a task's work wants the first; a gate counting the task's commits wants the second.
- **A test may not rest on anything a person's git configuration can reshape, and where one would, the adapter names the flag that settles it.** The mechanism is the fixture's *own* configuration, not the helper's environment: local beats global, and both the helper's git and the adapter's children read it, where `GIT_CONFIG_GLOBAL` on the helper's `Command` reaches only one of the two. So `TempRepo::new` sets `core.hooksPath` and `core.excludesFile` to `/dev/null` in the repository it makes — a contributor's global hook manager would otherwise run their hooks inside a fixture, their global ignore file would make a session's output an ignored file rather than an untracked one, and a global attributes file marking `*.rs -diff` would turn a patch into "Binary files differ". Those three are how a person's own configuration says what a path *is*, which is exactly what a fixture cannot let them say. The `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM` and `LC_ALL=C` environment stays on the helper's own `run_git`, because it costs nothing and covers the setup half, but it is not what the rule rests on.
- **A setting a fixture makes stays in force for everything after it, so a test that proves a flag gets its own fixture.** Learned the hard way: setting `status.showUntrackedFiles = no` to prove `is_clean`'s flag also took the dirtiness out of the worktree three lines below, where `git worktree remove` consults the same setting — which silently disarmed the assertion that `remove_worktree` passes `--force`. The proof of a flag lives in its own test, with its own repository.
- Two flags are the adapter's, not the test's. `is_clean` passes `--untracked-files=normal`, because `status.showUntrackedFiles = no` would otherwise tell Farik that a worktree full of a session's output is clean — a governance answer, not a formatting one. `diff` passes `--src-prefix=a/ --dst-prefix=b/`, because `diff.noprefix = true` would otherwise hand a reviewer a patch nothing can apply; the test asserts both sides of a patch, `--- a/` and `+++ b/`, because dropping either flag alone is a mutant that lives if only one side is read. Each is pinned by a test that sets that very setting in the fixture's own configuration. And git's refusals are matched by the `command` that was run and the name inside the message, never by the whole sentence, because the sentence is translated and the command is not.
- **A file git ignores does not make a worktree dirty, and that is deliberate.** `--untracked-files=normal` counts untracked files and not ignored ones, so a user whose global excludes name `*.log` gets a clean worktree with a log file in it. An ignore rule is how a person says a path is not content — `.DS_Store` beside a task's work is not the task's work — and Farik counts what git counts. The alternative, `--ignored=matching` and a filter, is a larger change than this step wants and a worse answer: Farik would be second-guessing the repository's own rules. `is_clean` says so in its doc comment, because the next reader will ask.
- `diff` also passes `--no-ext-diff`, and that one is defensive: no test reaches it. Setting `diff.external` in a fixture would make the test depend on a program on the machine to run the external differ with, which is the dependence the rest of this bullet is removing. It stays because a personal external differ is not a patch.
- Every path git prints comes back through `-z`, so nothing is split on a newline: a path may hold one, and a path list that loses a file is worse than one that refuses.
- **`merge` puts the repository back on the branch it found it on**, whether the merge took, conflicted, or refused. `self.root` is the user's own checkout, not a task's worktree — worktrees are where tasks work (5.14) and the integration branch lives here — so a merge that quietly moved what a person has open would be a surprise nobody asked for. The alternative considered was refusing unless `into` is already checked out, which only moves the checkout into every caller. A detached head refuses, because there is no branch to put back; `current_branch` is what says so, before anything has been run.
- When the merge and the branch that is put back after it both fail, the merge's own refusal is what comes back: a branch that could not be restored is worth reporting, but not in place of the reason the merge failed. This ordering is defensive and no test reaches it: `was_on` is a branch the main worktree has checked out, so nothing else can hold it, and a merge that succeeded leaves a tree clean enough to leave. Swapping the two does not fail anything, and a reader looking for the test that holds it should stop looking.
- A conflicted merge is undone before `merge` returns: the tree it would otherwise leave is one nobody is watching, and every later command would trip over it. The task escalates with reason `integration` instead (5.14), and `MergeOutcome::Conflicts` is what says so.
- `remove_worktree` passes `--force`, because a finished task's worktree holds whatever its session built — untracked output git would otherwise refuse to remove, and that nothing wants kept.
- `default_branch` asks `origin/HEAD` and falls back to the branch `HEAD` is on: a repository with no remote records its default branch nowhere at all. `.farik/team.yaml` is where a team says otherwise, and the integration branch is its to set (5.14).
- `Git::open` cannot fail and checks nothing; `is_repository` is what asks. A constructor that ran a program would make every caller handle an error before it had asked for anything.
- The parsing is separate from the running — `head_summary_of`, `default_branch_of`, `changed_paths_of`, `path_argument` are free functions of text — so the shapes git prints are held by unit tests that need no git at all, and only the running needs the program.
- **The flag is parsed in `xtask/src/lib.rs`, not in `main.rs`.** Hard rule 1 wants a failing test before the production code, and nothing in `main.rs` is testable: it is the binary, and `cargo test -p xtask` reports zero tests in it. So `Tests` and `tests_requested` live in a new `xtask::check` module beside `commit_message`, `todos` and `core_io` — the same split those already make, decisions in the library and effects in the binary — and `main.rs` keeps only the running of cargo. This closes the hole rather than widening it: `generate [--check]` parses its flag inline in `main.rs` today and has no test, and a later step can move it the same way.
- No new dependency, and `Cargo.lock` does not change.

## Design

`crates/store/src/git.rs` holds `Git`, `GitError`, `HeadSummary`, `MergeOutcome` and one free function per shape git prints. `crates/store/tests/git.rs` drives all of it against real repositories it makes in the temporary directory and removes when each test ends.

`xtask/src/check.rs` holds `Tests` and `tests_requested`, the flag `check` takes, with its own unit tests; `xtask/src/main.rs` asks it and runs cargo; `.github/workflows/check.yml` runs the check with that flag.

Out of scope: `commit` and `push`, which phase 3 step 03 adds with the tools that need them; anything that reads or writes `.farik/` (step 05); any caller of this adapter at all — nothing outside its own tests uses it yet.

## Architecture notes

- Modified: `crates/store` gains `git`, a sibling of `event_log` and `projections`. It touches neither of them and shares nothing with them: a repository is not a database.
- Created: `xtask/src/check.rs`, a sibling of `commit_message`, `todos` and `core_io` — what the library decides, with `main.rs` left with what it runs. Modified: `xtask/src/lib.rs` declares it, `xtask/src/main.rs` asks it, `.github/workflows/check.yml` uses the flag.
- Modified: `docs/standards/code.md`, one note on the continuous integration row, in task 6.
- Consumed: nothing from the other crates. `git.rs` uses only `std`.
- `farik-core` does no I/O and is not touched; `cargo xtask core-io` still passes.

## Global constraints

- Every method runs `git` through `run_git`, which is the one place a child process is spawned and the one place a failure becomes a `GitError`.
- No `unwrap` or `expect` outside tests.
- A path reaches git as a `&str` through `path_argument`, which refuses one that is not text rather than mangling it.
- Every test that needs the `git` program carries `#[ignore = "needs the git program: cargo xtask check --integration"]`, spelled exactly so, so that `cargo test` prints the reason.
- No test is skipped, ignored, or quarantined to get green: the eleven ignored tests are gated on a program, and they run in CI on every pull request.

## File map

```
crates/store/src/git.rs                        creates: Git, GitError, HeadSummary, MergeOutcome; tested by its own tests module
crates/store/tests/git.rs                      creates: the adapter against real repositories
crates/store/src/lib.rs                        modifies: the git module and what it re-exports
xtask/src/check.rs                             creates: Tests, tests_requested; tested by its own tests module
xtask/src/lib.rs                               modifies: declares the check module
xtask/src/main.rs                              modifies: check takes --integration
docs/standards/code.md                         modifies: the note on the continuous integration row
.github/workflows/check.yml                    modifies: CI runs the check with --integration
docs/plans/project-plan.md                     modifies: records what this step's interface became, and the one CI job
docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md modifies: this plan, ticked as it goes
```

`Cargo.toml` and `Cargo.lock` do not change: this step adds no dependency.

## Tasks

### Task 1: A repository, and the check that can run these tests

Files: created `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `xtask/src/check.rs`; modified `crates/store/src/lib.rs`, `xtask/src/lib.rs`, `xtask/src/main.rs`, `.github/workflows/check.yml`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: nothing from this plan
Produces: `farik_store::{Git, GitError}`, `Git::{open, is_repository}`, `xtask::check::{Tests, tests_requested}`, and `cargo xtask check --integration`

The flag arrives with the first test that needs it. Without it the eleven tests this step writes would be written and never run until the end, which is not a red-green cycle at all.

- [x] Write the failing tests. Create `crates/store/src/git.rs` with the module doc:

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

- [x] Create `crates/store/tests/git.rs`:

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
          // An identity, because a machine that has none cannot commit at all, and no signing,
          // because that would ask for a key nobody here has.
          repository.git(&["config", "user.name", "Farik Test"]);
          repository.git(&["config", "user.email", "test@farik.invalid"]);
          repository.git(&["config", "commit.gpgsign", "false"]);
          // And nothing of the person's own runs or is read inside a fixture. `Git` spawns its own
          // children and honours their configuration on purpose, so the fixture's own configuration
          // is where this has to be said: local beats global, and both the helper's git and the
          // adapter's read it. A global `core.hooksPath` would otherwise run a contributor's hooks
          // here, a global `core.excludesFile` would make a session's output an ignored file, and a
          // global `core.attributesFile` marking `*.rs -diff` would print a patch as "Binary files
          // differ" — the three ways a person's own configuration says what a path is.
          repository.git(&["config", "core.hooksPath", "/dev/null"]);
          repository.git(&["config", "core.excludesFile", "/dev/null"]);
          repository.git(&["config", "core.attributesFile", "/dev/null"]);
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
  ///
  /// The setup is held away from whatever the person running the tests has configured — a global
  /// `core.hooksPath` would otherwise run their hooks inside the fixture. What this cannot do is
  /// isolate the adapter: `Git` spawns its own children, and it honours the user's configuration on
  /// purpose, which is the whole reason Farik shells out to git at all. So no assertion in this file
  /// may rest on anything a setting can reshape — where one would, the adapter names the flag that
  /// makes the answer its own rather than the configuration's.
  fn run_git(directory: &Path, arguments: &[&str]) -> String {
      let output = Command::new("git")
          .args(arguments)
          .current_dir(directory)
          .env("GIT_CONFIG_GLOBAL", "/dev/null")
          .env("GIT_CONFIG_SYSTEM", "/dev/null")
          .env("LC_ALL", "C")
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

- [x] Declare the module in `crates/store/src/lib.rs`. rustfmt keeps both lists alphabetical, so this goes between `event_log` and `migrations` rather than at the end — replace

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

- [x] Write the flag's failing test too. Create `xtask/src/check.rs` with the module doc:

  ```rust
  //! Which tests `cargo xtask check` runs, and the flag that says so.
  ```

  then append the tests module:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::{Tests, tests_requested};

      #[test]
      fn runs_the_tests_that_need_no_program_when_asked_for_nothing() {
          assert_eq!(
              tests_requested(None),
              Ok(Tests::WithoutTheOnesThatNeedAProgram)
          );
      }

      #[test]
      fn runs_everything_when_asked_for_the_integration_tests() {
          assert_eq!(tests_requested(Some("--integration")), Ok(Tests::All));
      }

      #[test]
      fn says_what_the_usage_is_when_the_flag_is_not_one() {
          // Not silently the default: a flag with a typo in it would then run a smaller check than
          // the one continuous integration is asking for and say nothing about it.
          assert_eq!(
              tests_requested(Some("--intergration")),
              Err(
                  "unknown flag --intergration; usage: cargo xtask check [--integration]".to_string()
              )
          );
      }
  }
  ```

- [x] Declare it in `xtask/src/lib.rs`. rustfmt keeps this list alphabetical as well, so it goes before `commit_message` rather than at the end — replace

  ```rust
  /// Commit message rules from `docs/standards/code.md`.
  pub mod commit_message;
  ```

  with:

  ```rust
  /// Which tests `cargo xtask check` runs.
  pub mod check;
  /// Commit message rules from `docs/standards/code.md`.
  pub mod commit_message;
  ```

- [x] Run them and confirm they fail because there is no adapter and no flag. One command per target: `cargo test --workspace` compiles them in parallel and stops at whichever fails first, so what it prints is not the same twice running.

  ```
  cargo test -p xtask --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::Tests`, `super::tests_requested`
  # error: could not compile `xtask` (lib test) due to 1 previous error

  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::GitError`
  # error: could not compile `farik-store` (lib test) due to 1 previous error

  cargo test -p farik-store --test git
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `farik_store::Git`
  # error: could not compile `farik-store` (test "git") due to 1 previous error
  ```

- [x] Write the minimal implementation. Insert into `crates/store/src/git.rs`, between the module doc and the tests module:

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

- [x] Re-export them from `crates/store/src/lib.rs`, after the `event_log` line and before the `projections` one:

  ```rust
  pub use git::{Git, GitError};
  ```

- [x] Write the flag's implementation. Insert into `xtask/src/check.rs`, between the module doc and the tests module:

  ```rust
  /// Which tests the check runs.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Tests {
      /// Every test but the ones marked `#[ignore]` because they need a program the toolchain does
      /// not bring. The default: the check still compiles and lints those, and `cargo test` prints
      /// them as ignored rather than passing silently.
      WithoutTheOnesThatNeedAProgram,
      /// Those, and the ignored ones. What continuous integration runs.
      All,
  }

  /// What a flag given to `check` asks for.
  ///
  /// # Errors
  ///
  /// The usage line, when the flag is not one `check` knows.
  pub fn tests_requested(flag: Option<&str>) -> Result<Tests, String> {
      match flag {
          None => Ok(Tests::WithoutTheOnesThatNeedAProgram),
          Some("--integration") => Ok(Tests::All),
          Some(unknown) => Err(format!(
              "unknown flag {unknown}; usage: cargo xtask check [--integration]"
          )),
      }
  }
  ```

- [x] Have `check` take it. In `xtask/src/main.rs`, add to the imports, after the `anyhow` line:

  ```rust
  use xtask::check::Tests;
  ```

  replace

  ```rust
          Some("check") => check(&root),
  ```

  with:

  ```rust
          Some("check") => check(
              &root,
              xtask::check::tests_requested(args.get(1).map(String::as_str))
                  .map_err(anyhow::Error::msg)?,
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

- [x] Have CI run it. Replace `.github/workflows/check.yml`, whole, with:

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
    #
    # What that costs: nothing here runs `cargo xtask check` without the flag any more, so a break in
    # the flagless path — a test wrongly marked `#[ignore]` and so run by nobody's default command, or
    # the default arm itself — would reach `main` unseen. The command a contributor runs is what
    # guards it.
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

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 36 passed (the store's modules)
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 1 passed (git)
  #   test result: ok. 27 passed (xtask)
  ```

- [x] Run the check both ways, and confirm the flag is what runs the ignored test:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, the git test reported as 1 ignored
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, the git test reported as 1 passed
  cargo xtask check --nonsense
  # expected: xtask: unknown flag --nonsense; usage: cargo xtask check [--integration]
  ```

- [x] Commit: `feat(store): open a repository, and run the tests that need git`

### Task 2: What is at the tip, and which branch

Files: modified `crates/store/src/git.rs`, `crates/store/src/lib.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: `Git`, `GitError`, `at_root`, `run_git` from Task 1
Produces: `farik_store::HeadSummary`, `Git::{head_summary, default_branch, current_branch}`

- [x] Write the failing tests. Replace the whole tests module of `crates/store/src/git.rs` with:

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

- [x] Append to `crates/store/tests/git.rs`, a blank line between each of these and the test above it:

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

- [x] Run them and confirm they fail because nothing reads the tip. One command per target, as in task 1, so that what is printed is the same every time:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::HeadSummary`, `super::default_branch_of`,
  #   `super::head_summary_of`
  # error: could not compile `farik-store` (lib test) due to 1 previous error

  cargo test -p farik-store --test git
  # expected: FAIL to compile, with one E0599 per call site and nothing else:
  # error[E0599]: no method named `head_summary` found for struct `Git` in the current scope
  # error[E0599]: no method named `head_summary` found for struct `Git` in the current scope
  # error[E0599]: no method named `current_branch` found for struct `Git` in the current scope
  # error[E0599]: no method named `default_branch` found for struct `Git` in the current scope
  # error[E0599]: no method named `current_branch` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (test "git") due to 5 previous errors
  ```

- [x] Write the minimal implementation. Insert into `crates/store/src/git.rs`, before the doc comment of `pub struct Git` — above it, not between it and the struct:

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

- [x] Add `HeadSummary` to the re-export in `crates/store/src/lib.rs`, which becomes:

  ```rust
  pub use git::{Git, GitError, HeadSummary};
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 41 passed (the store's modules)
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 3 passed (git)
  #   test result: ok. 27 passed (xtask)
  ```

- [x] Commit: `feat(store): read the tip of a branch and which branch is which`

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

  and append to that file, a blank line between each of these and the test above it:

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
      let taken = git.create_branch("farik/FRK-1", "main");
      let Err(GitError::CommandFailed { command, stderr }) = taken else {
          panic!("a name is taken once: {taken:?}");
      };
      assert_eq!(command, "branch farik/FRK-1 main");
      // What git said, not how it said it: the sentence is translated, the branch name is not.
      assert!(stderr.contains("farik/FRK-1"), "{stderr}");

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

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn calls_a_worktree_dirty_whatever_the_repository_is_configured_to_show() {
      // A user with `status.showUntrackedFiles = no` does not get to tell Farik that a worktree full
      // of a session's output is clean. Its own fixture, because a setting stays in force for
      // everything after it: left in the test above, it would take the dirtiness out of the very
      // worktree whose removal that test forces.
      let repository = TempRepo::new("untracked-hidden");
      let git = repository.adapter();
      let worktree = repository.path.join(".farik/local/worktrees/FRK-3");
      git.create_worktree(&worktree, "farik/FRK-3", "main")
          .expect("the worktree is made");
      std::fs::write(worktree.join("built-by-the-session.txt"), "output\n")
          .expect("something the session built");
      repository.git(&["config", "status.showUntrackedFiles", "no"]);
      assert!(
          !git.is_clean(&worktree).expect("the read works"),
          "an untracked file is not clean whatever the repository is configured to show"
      );
  }
  ```

- [ ] Run them and confirm they fail because no worktree can be made:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::path_argument`
  # error: could not compile `farik-store` (lib test) due to 1 previous error

  cargo test -p farik-store --test git
  # expected: FAIL to compile, with one E0599 per call site and nothing else:
  # error[E0599]: no method named `create_branch` found for struct `Git` in the current scope
  # error[E0599]: no method named `create_branch` found for struct `Git` in the current scope
  # error[E0599]: no method named `create_worktree` found for struct `Git` in the current scope
  # error[E0599]: no method named `is_clean` found for struct `Git` in the current scope
  # error[E0599]: no method named `is_clean` found for struct `Git` in the current scope
  # error[E0599]: no method named `remove_worktree` found for struct `Git` in the current scope
  # error[E0599]: no method named `create_worktree` found for struct `Git` in the current scope
  # error[E0599]: no method named `is_clean` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (test "git") due to 8 previous errors
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
      /// `--untracked-files=normal` is what makes that promise true rather than hopeful: a user with
      /// `status.showUntrackedFiles = no` in their own configuration would otherwise be told a
      /// worktree full of a session's output is clean.
      ///
      /// A file git ignores is not untracked and does not make a worktree dirty, deliberately: an
      /// ignore rule is how a person says a path is not content, and `.DS_Store` next to a task's
      /// work is not the task's work. Farik counts what git counts.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when `path` is not a working tree of this repository.
      pub fn is_clean(&self, path: &Path) -> Result<bool, GitError> {
          self.require_repository()?;
          Ok(run_git(path, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty())
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
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 5 passed (git)
  #   test result: ok. 27 passed (xtask)
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
          GitError, HeadSummary, changed_paths_of, default_branch_of, head_summary_of, path_argument,
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
      // main moves on after the branch left it, as it does whenever another task lands first. What
      // this branch changed is what it changed since the two last agreed, so none of main's own work
      // belongs to it: counted from the tip instead, the governor would be handed
      // src/only-on-main.rs as a path this task touched (5.6).
      repository.git(&["checkout", "main"]);
      repository.write("src/only-on-main.rs", "fn elsewhere() {}\n");
      repository.commit("feat: something else entirely");
      repository.git(&["checkout", "farik/FRK-1"]);

      assert_eq!(git.commit_count("main", "farik/FRK-1"), Ok(2));
      assert_eq!(
          git.commit_count("farik/FRK-1", "main"),
          Ok(1),
          "the other way is main's own commit, not the branch's two"
      );
      let mut changed = git
          .changed_paths("main", "farik/FRK-1")
          .expect("the read works");
      changed.sort();
      assert_eq!(changed, ["README.md", "src/added.rs"]);
      // git's own prefixes, whatever this repository is configured to use: a patch without them is
      // one nothing can apply, and a reviewer is handed this patch to read (5.6).
      repository.git(&["config", "diff.noprefix", "true"]);
      let patch = git.diff("main", "farik/FRK-1").expect("the read works");
      assert!(patch.contains("fn added()"), "the patch holds the change");
      assert!(patch.contains("--- a/README.md"), "and the removal");
      assert!(patch.contains("+++ b/src/added.rs"), "with both prefixes");
      assert!(
          !patch.contains("only-on-main"),
          "and nothing main did after the branch left it: {patch}"
      );
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
  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::changed_paths_of`
  # error: could not compile `farik-store` (lib test) due to 1 previous error

  cargo test -p farik-store --test git
  # expected: FAIL to compile, with one E0599 per call site and nothing else:
  # error[E0599]: no method named `commit_count` found for struct `Git` in the current scope
  # error[E0599]: no method named `commit_count` found for struct `Git` in the current scope
  # error[E0599]: no method named `changed_paths` found for struct `Git` in the current scope
  # error[E0599]: no method named `diff` found for struct `Git` in the current scope
  # error[E0599]: no method named `changed_paths` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (test "git") due to 5 previous errors
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
      /// The flags are what make this git's own patch in git's own shape, whatever the user has
      /// configured: `--no-ext-diff` because a personal external differ is not a patch, and the two
      /// prefixes because `diff.noprefix` would otherwise hand a reviewer a patch nothing can apply.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown.
      pub fn diff(&self, base: &str, head: &str) -> Result<String, GitError> {
          self.require_repository()?;
          let range = format!("{base}...{head}");
          self.at_root(&[
              "diff",
              "--no-color",
              "--no-ext-diff",
              "--src-prefix=a/",
              "--dst-prefix=b/",
              &range,
          ])
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
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 7 passed (git)
  #   test result: ok. 27 passed (xtask)
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
      use std::path::{Path, PathBuf};

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
              nowhere.create_worktree(Path::new("worktrees/FRK-1"), "farik/FRK-1", "main"),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.remove_worktree(Path::new("worktrees/FRK-1")),
              Err(GitError::NotARepository)
          );
          assert_eq!(
              nowhere.is_clean(Path::new(".")),
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

      // Called from the task's own branch, which is where a finished task leaves the repository. The
      // merge goes to the integration branch and comes back: this checkout is the user's own, not a
      // task's worktree, so what they have open is not the merge's to move.
      let outcome = git
          .merge("main", "farik/FRK-1", "integrate FRK-1")
          .expect("the merge runs");
      let MergeOutcome::Merged { sha } = outcome else {
          panic!("it merged: {outcome:?}");
      };
      assert_eq!(sha, repository.git(&["rev-parse", "main"]));
      assert_eq!(
          git.current_branch().expect("the read works"),
          "farik/FRK-1",
          "and left the repository on the branch it found it on"
      );
      assert_eq!(
          repository.git(&["log", "-1", "--format=%s", "main"]),
          "integrate FRK-1",
          "with a merge commit, so the task's commits survive"
      );
      assert_eq!(
          repository.git(&["rev-list", "--count", "--merges", "main"]),
          "1"
      );
      assert_eq!(
          repository.git(&["show", "main:src/added.rs"]),
          "fn added() {}",
          "and the work is on the integration branch"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_merge_that_failed_for_something_other_than_a_conflict() {
      // Nothing conflicted, so there is nothing to report as a conflict: an empty list of conflicted
      // paths would be a refusal wearing the shape of an answer, and the task would be integrated on
      // paper without a single commit having moved.
      let repository = TempRepo::new("merge-refused");
      let git = repository.adapter();
      let refusal = git.merge("main", "farik/FRK-404", "integrate FRK-404");
      let Err(GitError::CommandFailed { command, stderr }) = refusal else {
          panic!("it refused: {refusal:?}");
      };
      assert_eq!(
          command, "merge --no-ff -m integrate FRK-404 farik/FRK-404",
          "the merge's own refusal, not whatever an abort with nothing to abort says"
      );
      assert!(stderr.contains("farik/FRK-404"), "{stderr}");
      assert_eq!(
          git.current_branch().expect("the read works"),
          "main",
          "and the repository is where it was"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_merge_from_a_detached_head_because_there_is_no_branch_to_put_back() {
      // The repository is put back on the branch it was found on, and a detached head is not one.
      // Refusing before anything has been run is better than merging and leaving a person somewhere
      // they never were.
      let repository = TempRepo::new("detached");
      let git = repository.adapter();
      repository.git(&["checkout", "--detach"]);
      let answer = git.merge("main", "main", "integrate nothing at all");
      assert!(
          matches!(answer, Err(GitError::CommandFailed { .. })),
          "{answer:?}"
      );
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
      let before = repository.git(&["rev-parse", "main"]);
      repository.git(&["checkout", "farik/FRK-1"]);

      let outcome = git
          .merge("main", "farik/FRK-1", "integrate FRK-1")
          .expect("the merge runs and reports");
      assert_eq!(
          outcome,
          MergeOutcome::Conflicts(vec!["README.md".to_string()])
      );
      assert_eq!(
          repository.git(&["rev-parse", "main"]),
          before,
          "nothing was committed"
      );
      assert_eq!(
          git.current_branch().expect("the read works"),
          "farik/FRK-1",
          "and the repository is back on the branch it was on"
      );
      assert!(
          git.is_clean(&repository.path).expect("the read works"),
          "and nothing was left half-merged"
      );
      assert_eq!(
          repository.git(&["show", "main:README.md"]),
          "main's line",
          "the integration branch's own work is untouched"
      );
  }
  ```

- [ ] Run them and confirm they fail because nothing merges:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (lib test) due to 1 previous error

  cargo test -p farik-store --test git
  # expected: FAIL to compile, with one E0599 per call site and nothing else:
  # error[E0432]: unresolved import `farik_store::MergeOutcome`
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error[E0599]: no method named `merge` found for struct `Git` in the current scope
  # error: could not compile `farik-store` (test "git") due to 5 previous errors
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

  and insert into `impl Git`, before `require_repository` — `merge` and the private `merge_what_is_checked_out` it calls, both of them:

  ```rust
      /// Merges `from` into `into` with a merge commit, or reports what conflicted.
      ///
      /// A conflict leaves nothing behind: the merge is undone before this returns, because the tree
      /// it would leave is one nobody is watching and every later command would trip over it. The
      /// task is escalated with reason `integration` instead (5.14).
      ///
      /// The repository is left on the branch it was found on, whether the merge took or not. `root`
      /// is the user's own checkout rather than a task's worktree — worktrees are where tasks work
      /// (5.14) and the integration branch lives here — so a merge that moved it would change what a
      /// person has open in front of them.
      ///
      /// # Errors
      ///
      /// `CommandFailed` when either name is unknown, when the head is detached and there is no
      /// branch to put back, or when the tree is not clean enough to switch branches.
      pub fn merge(&self, into: &str, from: &str, message: &str) -> Result<MergeOutcome, GitError> {
          self.require_repository()?;
          let was_on = self.current_branch()?;
          self.at_root(&["checkout", into])?;
          let outcome = self.merge_what_is_checked_out(from, message);
          if was_on == into {
              return outcome;
          }
          let restored = self.at_root(&["checkout", &was_on]);
          // The merge's own answer comes first. A branch that could not be put back is worth
          // reporting, but not in place of the reason the merge itself refused.
          outcome.and_then(|merged| restored.map(|_| merged))
      }

      /// The merge itself, with `into` already checked out.
      fn merge_what_is_checked_out(
          &self,
          from: &str,
          message: &str,
      ) -> Result<MergeOutcome, GitError> {
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
  #   test result: ok. 8 passed (event_log_file)
  #   test result: ok. 11 passed (git)
  #   test result: ok. 27 passed (xtask)
  ```

- [ ] Commit: `feat(store): merge a finished task or say what conflicted`

### Task 6: The plans say what the adapter became

Files: modified `docs/plans/project-plan.md`, `docs/standards/code.md`, `docs/plans/phase-2-protocol-store-cli/step-04-git-adapter.md`

Consumes: everything above
Produces: a project plan and a standard that describe the adapter and the check as they now are

This task changes documentation and has no test cycle. The `> ` marker on each block below is this plan's and is not part of the text to write.

- [ ] In `docs/plans/project-plan.md`, replace the line beginning `- Step 04 (\`farik-store::git\`):` with:

  > - Step 04 (`farik-store::git`): `enum GitError { NotARepository, NotInstalled { detail }, CommandFailed { command, stderr } }` (the middle one added 2026-09-17 by the step 04 plan: folding "the git program is not on this machine" into `CommandFailed` would report an empty `stderr` for a command that never ran); `struct Git { root: PathBuf }`; `impl Git { fn open(root: PathBuf) -> Git; fn is_repository(&self) -> bool; fn head_summary(&self) -> Result<Option<HeadSummary>, GitError>; fn default_branch(&self) -> Result<String, GitError>; fn current_branch(&self) -> Result<String, GitError>; fn create_branch(&self, name, from) -> Result<(), GitError>; fn create_worktree(&self, path, branch, from) -> Result<(), GitError>; fn remove_worktree(&self, path) -> Result<(), GitError>; fn is_clean(&self, path) -> Result<bool, GitError>; fn commit_count(&self, base, head) -> Result<u32, GitError>; fn changed_paths(&self, base, head) -> Result<Vec<String>, GitError>; fn diff(&self, base, head) -> Result<String, GitError>; fn merge(&self, into, from, message) -> Result<MergeOutcome, GitError> }`; `struct HeadSummary { sha, committed_at, subject }`; `enum MergeOutcome { Merged { sha }, Conflicts(Vec<String>) }`. Every method runs the `git` program; every one asks `is_repository` first and answers `NotARepository` itself rather than passing on what git printed. `changed_paths` and `diff` take the three-dot range `base...head` and `commit_count` the two-dot `base..head`; a rename comes back as both of its paths, because 5.6 asks about each path a change touched. A conflicted merge is undone before `merge` returns, and `merge` leaves the repository on the branch it found it on, because `root` is the user's own checkout rather than a task's worktree. Phase 3 step 03 adds `commit` and `push`.

- [ ] In `docs/plans/project-plan.md`, in the "Tests are split in three" bullet, replace `which runs in CI as a second job of the same \`check\` workflow from the step that adds the first test needing it (phase 2 step 04)` with:

  > which CI runs as the one job it has, because `--integration` only adds the ignored tests to the same `cargo test` invocation and a second job would compile the workspace again to run a superset of the first (changed 2026-09-17 by the step 04 plan; a second job starts paying for itself when a test needs Docker, which is phase 3 step 02)

- [ ] In `docs/plans/project-plan.md`, in the phase 2 step table, replace the last cell of the step 04 row — `Repository queries, branches, worktrees, changed paths, diff, clean check, commit count, merge; the first integration test that needs a git binary, and the CI job for \`cargo xtask check --integration\`` — with:

  > Repository queries, branches, worktrees, changed paths, diff, clean check, commit count, merge; the first tests that need a git binary, and the `--integration` flag CI runs them with

- [ ] In `docs/plans/project-plan.md`, on the phase 2 step 04 line, nothing else changes: the `xtask::check` module the flag is parsed in is an `xtask` internal, and the project plan records what one step hands the next rather than how a repository task is laid out.

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
  #   test result: ok. 44 passed (farik-store, its five modules)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 11 passed (crates/store/tests/git.rs)
  #   test result: ok. 27 passed (xtask)
  ```

- [ ] And without the flag, to confirm the gate is a gate:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with crates/store/tests/git.rs reported as
  #   test result: ok. 0 passed; 0 failed; 11 ignored
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
