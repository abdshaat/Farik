# Phase 2, step 07: The project scan and reconciliation

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 4 (onboarding scans the tree and reads it back), 5.8 (the scan is stored as `project.md`), 5.13 (the criterion library holds the project's own check and test commands, found by the scan), 8.4 (the log is the source of truth for what happened, the files for what the team knows); F2, F16
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01 to 06 of this phase (last code commit `3004cfe`)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, nothing ambiguous, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a review session that did not write this plan, on 2026-09-18, against `7f6f086`. It confirmed the three rules by rebuilding Tasks 1 to 4 from this plan alone in a fresh copy with a target directory of its own: every red exact, error for error and count for count; every green exact at 15, 18, 13 and 9 and at all nine counts of the whole check; Task 5 applied and checked clause by clause against the code the earlier tasks produced; the Verification section run whole, the commit-subject loop included; and the changed-file set exactly the File map's. It also mutated the two behaviours the previous round had added tests for and confirmed each test is the only one that notices, and grepped the deleted `order_of` to confirm one definition of the order is left.

Four rounds. Round one found twelve things, eight of them defects in the code this plan produces: a language tie-break that did the opposite of its own comment, a commit in the future only caught at a whole day, a polyglot project losing the toolchain its language names, a subtree scanning as if it were the project, a one-package workspace calling itself a monorepo, an empty script becoming a criterion that verifies nothing, a test runner matched by prefix anywhere in the tree, and four public items nothing asserted. Round two found six more, the worst of them a record for the project plan that listed four `ScanError` variants where this plan builds five. Round three found five, the worst a second definition of the drift order that disagreed with the first and that no test could catch. Round four found none.

Four observations it recorded rather than refused on, none of them grounds to stop: the sort key is the number in a task id, which is total over every id the store's counter can assign but not over two hand-named files differing only by a leading zero; "the commands those toolchains always have" is exact for cargo and go and a convention for poetry, uv and bundler; the tables' listing above is in declaration order, which is what it now says; and two of Task 2's anchors quote a backtick escaped inside a code span, which is the settled style of this phase's five merged step plans. The first two are taken after the step lands, with the rest of the landing pass.

## Goal

Two things, both about a project Farik has just been pointed at.

**The scan** reads the repository through git and says what it is, in the one line section 4 gives as its example: `TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago`. The same reading gives the criterion library its first entries, so that a contract in this project is verified by the project's own commands rather than by something a Product Manager invented (5.13, F16).

**Reconciliation** says where the files and the log disagree, without picking a side. Two sources of truth about one project can come apart — a process stopped between a write and an append, a person edited a contract by hand — and 8.4 makes each authoritative about different things. A person runs this to find out what is wrong, so one broken file must not hide every other disagreement.

## Decisions

- **The scan's signals are files, not guesses.** Every one is a path the repository tracks or a line in one of its manifests. Nothing is inferred from a directory's name, nothing is executed, and the answer for one tree is the same every time — which is what lets the read-back be asserted character for character.
- **The tree is read through git, not by walking the directory.** `Git::tracked_paths` runs `ls-files -z`, so `.gitignore` decides what is not content and Farik never has to know that `node_modules/` and `target/` exist. A test proves it: twenty TypeScript files git ignores do not make a Rust project a TypeScript one.
- **`scan_project` takes the adapter and a clock, not a root and an adapter.** The project plan's line said `(root: &Path, git: &Git)`. Two roots beside each other can disagree, and then the tree the paths are listed from is not the tree the manifests are read from; the root comes from `Git::root` instead. And `now` is a parameter because the read-back says how long ago the last commit was, which a function that sampled the clock itself could not be held to — the pattern step 02 set with `open_event_log(path, now)`.
- **One toolchain is chosen, not all of them**, because a criterion called `the-tests-pass` can only mean one command. A project with two markers — a Rust workspace with a front end, which is the shape this repository itself takes — gets the one whose language the tree says it mostly is, and falls back to the first marker in table order when the language names none of them. Found by the first readiness review: table order alone gave a Rust workspace pnpm's scripts and dropped `cargo test`, `cargo clippy` and `cargo fmt` from the library every contract in the project verifies with.
- **A language tie goes to whichever comes first in the table.** `Iterator::max_by_key` keeps the last of equal keys, so the table is read backwards to get the first — the first readiness review found the code doing the opposite of what its own comment promised, on the first sentence a person ever sees from Farik.
- **A workspace of one package is a workspace, not a monorepo.** One condition decides the word and the count together, so they cannot disagree; "1 packages" is not a count and a single package is not a monorepo.
- **A commit ahead of now is reported as its stamp, however far ahead.** Two machines whose clocks disagree are hours apart, not days, so the comparison is between instants rather than between day counts.
- **A bare repository is refused in git's words, not Farik's.** It is a repository, so `is_repository` says yes, and it has no working tree, so `rev-parse --show-toplevel` refuses and the person is shown git's sentence. Saying it better needs a refusal of its own; it is recorded on step 08's row of the project plan with the other things `farik doctor` should say plainly, because a bare repository is a thing onboarding can be pointed at and nothing else in this step can see it.
- **A tracked file that is not on disk refuses the whole scan, and a tracked file that is not JSON does not.** The two look alike and are not: half a `package.json` is a project mid-edit, and a `package.json` git tracks that is not there at all is a tree half-checked-out, which is a fact about the checkout rather than about the project. `ScanError::Io` is the second; the first is no criteria and a read-back that says what it can.
- **A subtree of a repository is not a project.** Section 4 asks a person to pick the project's folder; `Git::open` accepts any directory inside a repository, so `scan_project` asks git where the repository begins and refuses when it was pointed somewhere else. Scanning a subtree would answer confidently about a project it had only seen part of.
- **A script name with nothing behind it is not a command.** `"test": ""` would become a criterion whose command exits 0 having verified nothing. What this cannot see is a stub that runs and fails on purpose, as `npm init` writes for `test`; that is a project telling its own tools something, and the person adopting Farik has to look at it.
- **A test runner is named by its own file, not by a file that mentions it.** `vitest.config.ts` says the project tests with vitest; `jest-to-vitest-migration.md` says somebody wrote about it. The basename has to be the runner's name or begin with it and a dot.
- **A contract that cannot be read is one variant, whichever way it failed.** Broken by hand, unreadable to this user, or gone between the listing and the read: `detail` carries the file adapter's own words, and no test can win that race, so a branch nothing can reach would be worse than one variant whose detail tells the truth.
- **A Node project's commands are read, not invented.** F2 says to detect the project's test and build commands. For a toolchain whose commands live in a manifest, the scan reads `package.json`'s scripts and takes the five whose meaning is the same everywhere; for cargo, go, poetry, uv and bundler it uses the commands those toolchains always have. A closed list of script names is also what keeps a criterion's name inside the schema's kebab-case pattern.
- **The scan builds its criteria as wire values and hands them to `validate_criteria`.** The tables in the module describe criteria; the library's own rules are what say whether they are ones. A name that is not kebab-case or a text under ten characters would otherwise be a defect nothing here would catch, and `ScanError::Built` reports it rather than unwrapping.
- **A refresh replaces what a scan found and never touches what a person wrote.** `criteria.schema.json` says so of its `source` field, and `seeded_library` is where it is true: criteria whose source is `project_scan` are dropped, everything else is kept, and a name a person has used is a name the scan leaves alone.
- **`seeded_library` may return more criteria than the schema allows, and `write_criteria` is what refuses it.** One rule in one place, named against the file it is about, rather than a second ceiling here.
- **A manifest that is not JSON is a fact about the project.** Half a `package.json` a person was editing means the read-back says what it can and no criterion is found — not that the whole scan is refused.
- **A stamp the clock cannot parse, or one in the future, is reported as the stamp.** "Four days ago" that nobody can check is worse than the stamp itself, and two machines whose clocks disagree is an ordinary state.
- **`reconcile` takes the projections, not the log.** The project plan's line said `log: &EventLog`. What the log says the state is has one definition already — the projections, which catch up from the log when they are opened — and a second definition here could disagree with the first, which is the defect this function exists to report.
- **Only what the log is authoritative about is a disagreement.** A contract's `status` and its `locked` flag are both moved by events (5.2, 5.11), so a file that says otherwise is a file to fix. Its title, kind, risk and parent are the contract's own (8.4), and the board's copy of them is a cache of the last event that mentioned the task: one that has fallen behind is a projection to rebuild, not a disagreement about the project.
- **`Drift::LockMismatch` is added to the three the project plan named.** 5.11 makes the human's hold on a contract a governance fact, and it is written in both places. A file saying `locked: false` while the log says held would let work start on a contract nobody may touch.
- **`Drift::ContractUnreadable` too.** A person runs this to find out what is wrong; stopping at the first file somebody broke would hide every other disagreement.
- **The answer is ordered, so two runs can be compared.** By the number in the task id, with a stable sort, so two drifts about one task keep the order the loops found them in: a status before a lock. There is no second ranking beside that one — a first draft had an `order_of` function that ranked the five variants, it disagreed with the order the enum declares them in, and no test could tell, because a task whose contract cannot be read is passed over and can never carry a second drift. Found by the third readiness review.
- **`TempRepo` becomes a public fixture** in `crates/store/src/git/fixtures.rs` rather than staying private to `tests/git.rs`, because the scan's tests need a repository and step 08's command line will too, and `docs/standards/code.md` says a crate's fixtures are public so another crate's tests can use them.
- **The fixture's `git` is split into one that runs and one that asks.** A single method returning a `String` most callers ignore cannot satisfy clippy's `must_use_candidate` either way: with the attribute every setup call warns, without it the function does. `git` runs and returns nothing; `git_output` asks.
- **`Git::root` is added and exercised.** A public accessor nothing asserts is what step 06's landing review found; one assertion in the tracked-paths test kills that mutant.
- **`project.md`'s fuller shape is phase 3's.** 5.8 puts it among the memories a session reads, and what a session needs from it is a question phase 3 answers. Step 08 writes the read-back into it; nothing reads it until a session does.
- **The scan does not refresh itself.** 5.8 says the scan is refreshed whenever the tree changes materially. Something has to watch the tree for that, which is phase 3's; this step is the reading.

## Design

`crates/store/src/scan.rs` holds `ProjectScan`, `ScanError`, `scan_project`, `seeded_library`, the detection tables, and the private `Reading` that is what one tree says about itself before any of it is put into words. `crates/store/src/reconcile.rs` holds `Drift`, `ReconcileError` and `reconcile`. `crates/store/src/git/fixtures.rs` holds `TempRepo` and the two free functions that run git in a directory that is not one.

The tables, in the order the module declares them:

```
LANGUAGES         extension -> language; the project's is the one with the most tracked files
TOOLCHAINS        a marker file -> a name, the language whose project it is, and where its
                  commands come from; the language is what the pick turns on
  Commands::Scripts    pnpm, bun, yarn, npm: whatever this project's package.json calls its scripts
  Commands::Fixed      cargo, go, poetry, uv, bundler: the commands those toolchains always have
SCRIPTS           the five script names whose meaning is the same everywhere, and what each is called
TEST_RUNNERS      a config file's prefix or a dependency's name -> the runner a person would name
WORKSPACE_MARKERS what says a project is one workspace of several packages
MANIFESTS         what a package has one of, for counting packages
```

Out of scope: every command that calls any of this, which is step 08; watching the tree to know when a refresh is due, and what `project.md` holds beyond the read-back, which are phase 3's.

## Architecture notes

- Modified: `farik-store` gains `scan` and `reconcile`, siblings of `event_log`, `projections`, `git` and `files`. `git` gains a `fixtures` child and two methods.
- Consumed: `farik-core`'s `validate_criteria`, `CriterionTemplate`, `CriteriaLibrary`, `CriterionSource` and `TaskId`; this crate's own `Git`, `ProjectFiles` and `Projections`. `chrono` for the clock, `serde_json` for a manifest.
- No new dependency. `chrono` and `serde_json` are already in this crate.
- `farik-core` is not touched at all, so `cargo xtask core-io` is unaffected.
- `docs/SPEC.md` does not change: this step implements sections 4, 5.8, 5.13 and 8.4 as written, adds no rule and emits no event.

## Global constraints

- Every signal the read-back rests on is a tracked path or a line in a manifest, never a guess from a name and never a program that is run.
- Every criterion the scan builds goes through `validate_criteria` before it leaves the module.
- One broken file is a finding, not the end of the reconciliation.
- No `unwrap` or `expect` outside tests and fixtures.
- No test is skipped, ignored, or quarantined to get green. The tests that need the `git` program are `#[ignore]`d and run by `cargo xtask check --integration`, as step 04 established.

## File map

```
crates/store/src/scan.rs                      creates: ProjectScan, ScanError, scan_project, seeded_library
crates/store/src/reconcile.rs                 creates: Drift, ReconcileError, reconcile
crates/store/src/git/fixtures.rs              creates: TempRepo, git_in, git_output_in
crates/store/tests/project_scan.rs            creates: the scan against a real repository
crates/store/tests/reconciliation.rs          creates: the files against the log
crates/store/src/git.rs                       modifies: declares fixtures, adds root, top_level and tracked_paths, renames the splitter
crates/store/src/lib.rs                       modifies: declares scan and reconcile
crates/store/tests/git.rs                     modifies: uses the public fixture, gains three tests
docs/plans/project-plan.md                    modifies: records what this step's interface became
docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md modifies: this plan, ticked as it goes
```

## Tasks

Blocks are separated by exactly one blank line. `rustfmt.toml` allows no more, and `cargo xtask check` runs the format check before it runs a test, so two blank lines at a seam fail a task before anything is tried.

### Task 1: The repository's own fixture

Files: created `crates/store/src/git/fixtures.rs`; modified `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md`

Consumes: nothing from this plan
Produces: `farik_store::git::fixtures::{TempRepo, git_in, git_output_in}`, which every later task's tests use

- [x] Write the failing test. Replace `crates/store/tests/git.rs`, whole, with this — the same file with its fixture taken out, importing the public one instead, and with the calls that want git's answer using `git_output`:

  ```rust
  //! The git adapter against a real repository.
  //!
  //! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
  //! `cargo xtask check --integration`. They are ignored rather than compiled out: the default check
  //! still builds them, so `cargo fmt` and `clippy` hold them to the same standard as everything else
  //! and they cannot rot unnoticed (`docs/standards/code.md`, "Rust integration test").

  use farik_store::git::fixtures::{TempRepo, git_in};
  use farik_store::{Git, GitError, MergeOutcome};

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

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_the_commit_at_the_tip_and_says_when_there_is_none() {
      let empty = std::env::temp_dir().join(format!("farik-git-empty-{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&empty);
      std::fs::create_dir_all(&empty).expect("a directory");
      git_in(&empty, &["init", "-b", "main"]);
      assert_eq!(
          Git::open(empty.clone())
              .head_summary()
              .expect("the read works"),
          None,
          "a repository farik init has just made has no commit"
      );
      let _ = std::fs::remove_dir_all(&empty);

      let repository = TempRepo::new("head-summary");
      // Two commits, so that the tip is the tip rather than the only thing there is.
      repository.write("second.txt", "and a second\n");
      repository.commit("the second commit");
      let summary = repository
          .adapter()
          .head_summary()
          .expect("the read works")
          .expect("a repository with a commit has one");
      assert_eq!(summary.sha, repository.git_output(&["rev-parse", "HEAD"]));
      assert_eq!(summary.subject, "the second commit");
      // Strict ISO 8601, which is what a board and a log can order and a person can read. The
      // ordinary format git prints is neither: it separates the date from the time with a space.
      assert_eq!(
          summary.committed_at,
          repository.git_output(&["log", "-1", "--format=%cI"])
      );
      assert!(
          summary.committed_at.contains('T'),
          "{}",
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

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_the_default_branch_from_the_remote_that_records_it() {
      // A repository with a remote records its default branch, and that is the integration branch
      // (5.14) — not whatever a session happens to have checked out at the time.
      let origin = TempRepo::new("default-origin");
      origin.git(&["branch", "-m", "trunk"]);
      let clone = std::env::temp_dir().join(format!(
          "farik-git-clone-{}-{:?}",
          std::process::id(),
          std::thread::current().id()
      ));
      let _ = std::fs::remove_dir_all(&clone);
      git_in(
          &std::env::temp_dir(),
          &[
              "clone",
              "--quiet",
              origin.path.to_str().expect("a path that is text"),
              clone.to_str().expect("a path that is text"),
          ],
      );
      let git = Git::open(clone.clone());
      git_in(&clone, &["checkout", "-b", "farik/FRK-1"]);
      assert_eq!(git.current_branch().expect("the read works"), "farik/FRK-1");
      assert_eq!(
          git.default_branch().expect("the read works"),
          "trunk",
          "what the remote records, not what is checked out"
      );
      let _ = std::fs::remove_dir_all(&clone);
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn hands_back_a_patch_with_no_colour_in_it() {
      // Whatever the repository is configured to show. A patch is read by a reviewer and by whatever
      // reads a reviewer's answer; escape codes are neither.
      let repository = TempRepo::new("colour");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      repository.write("src/added.rs", "fn added() {}\n");
      repository.commit("feat: add a thing");
      repository.git(&["config", "color.ui", "always"]);

      let patch = git.diff("main", "farik/FRK-1").expect("the read works");
      assert!(patch.contains("fn added()"), "the patch holds the change");
      assert!(
          !patch.contains('\u{1b}'),
          "and nothing a terminal would paint: {patch:?}"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn makes_a_branch_and_a_worktree_for_a_task_and_takes_the_worktree_away_again() {
      // One worktree per task is what keeps two tasks from sharing a working tree (5.14), and the
      // branch outlives it: the work is on the branch, not in the directory.
      let repository = TempRepo::new("worktree");
      let git = repository.adapter();
      // From `main`, whatever this repository has checked out: a task's branch starts from the
      // integration branch as it is at that moment (5.14), and starting it somewhere else is not an
      // error anyone would see.
      repository.git(&["checkout", "-b", "elsewhere"]);
      repository.write("only-on-elsewhere.txt", "not where a task starts\n");
      repository.commit("docs: somewhere else entirely");

      git.create_branch("farik/FRK-1", "main")
          .expect("the branch is made");
      assert_eq!(
          repository.git_output(&["rev-parse", "farik/FRK-1"]),
          repository.git_output(&["rev-parse", "main"]),
          "the branch starts where it was told to"
      );
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
      assert!(
          !worktree.join("only-on-elsewhere.txt").exists(),
          "and the worktree starts from main too, not from what was checked out"
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
              .git_output(&["branch", "--list", "farik/FRK-2"])
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

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_tree_that_is_not_a_worktree_of_this_repository() {
      // `is_clean` is asked whether a task's work is finished, so another repository's answer is
      // worse than no answer at all — and a worktree a crashed session took away with it is not the
      // git program failing to run, which is what a user would otherwise be told.
      let repository = TempRepo::new("membership");
      let elsewhere = TempRepo::new("membership-elsewhere");
      let git = repository.adapter();

      let foreign = git.is_clean(&elsewhere.path);
      let Err(GitError::CommandFailed { stderr, .. }) = foreign else {
          panic!("another repository is not an answer about this one: {foreign:?}");
      };
      assert!(
          stderr.contains("not a working tree of this repository"),
          "{stderr}"
      );

      let gone = git.is_clean(&repository.path.join(".farik/local/worktrees/FRK-9"));
      let Err(GitError::CommandFailed { stderr, .. }) = gone else {
          panic!("a worktree that is not there is not git failing to run: {gone:?}");
      };
      assert!(stderr.contains("there is no directory at"), "{stderr}");

      // And a worktree of this repository is still an answer, which is the point of asking by
      // repository rather than by path.
      let worktree = repository.path.join(".farik/local/worktrees/FRK-1");
      git.create_worktree(&worktree, "farik/FRK-1", "main")
          .expect("the worktree is made");
      assert!(git.is_clean(&worktree).expect("the read works"));
  }

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
  fn keeps_a_path_whose_name_begins_with_a_space() {
      // The allowed-paths rule is asked about each path a change touched (5.6), so a path that comes
      // back a byte short is a change checked against a rule it never matched. `-z` is what keeps a
      // newline in a path; this is what keeps a space at the front of one.
      let repository = TempRepo::new("odd-path");
      let git = repository.adapter();
      repository.git(&["checkout", "-b", "farik/FRK-1"]);
      repository.write(" leading.txt", "a path that starts with a space\n");
      repository.commit("feat: a path only a person could name");

      assert_eq!(
          git.changed_paths("main", "farik/FRK-1")
              .expect("the read works"),
          [" leading.txt"]
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
      assert_eq!(sha, repository.git_output(&["rev-parse", "main"]));
      assert_eq!(
          git.current_branch().expect("the read works"),
          "farik/FRK-1",
          "and left the repository on the branch it found it on"
      );
      assert_eq!(
          repository.git_output(&["log", "-1", "--format=%s", "main"]),
          "integrate FRK-1",
          "with a merge commit, so the task's commits survive"
      );
      assert_eq!(
          repository.git_output(&["rev-list", "--count", "--merges", "main"]),
          "1"
      );
      assert_eq!(
          repository.git_output(&["show", "main:src/added.rs"]),
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
      let before = repository.git_output(&["rev-parse", "main"]);
      repository.git(&["checkout", "farik/FRK-1"]);

      let outcome = git
          .merge("main", "farik/FRK-1", "integrate FRK-1")
          .expect("the merge runs and reports");
      assert_eq!(
          outcome,
          MergeOutcome::Conflicts(vec!["README.md".to_string()])
      );
      assert_eq!(
          repository.git_output(&["rev-parse", "main"]),
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
          repository.git_output(&["show", "main:README.md"]),
          "main's line",
          "the integration branch's own work is untouched"
      );
  }
  ```

- [x] Run it and confirm it fails because there is no such module yet:

  ```
  cargo test -p farik-store --test git
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `farik_store::git::fixtures`
  # error: could not compile `farik-store` (test "git") due to 1 previous error
  ```

- [x] Write the minimal implementation. Create `crates/store/src/git/fixtures.rs`:

  ```rust
  //! A repository of its own, for tests in this crate and in others.

  use std::path::{Path, PathBuf};
  use std::process::Command;

  use super::Git;

  /// A git repository of its own, removed when the value is dropped however the test ends.
  pub struct TempRepo {
      /// The repository root.
      pub path: PathBuf,
  }

  impl TempRepo {
      /// A repository with one commit on `main`, holding `README.md`.
      ///
      /// # Panics
      ///
      /// When git is not installed or refuses, which is the machine refusing rather than the code. A
      /// test that uses this is `#[ignore]`d and run by `cargo xtask check --integration`.
      #[must_use]
      pub fn new(name: &str) -> Self {
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

      /// The adapter for this repository.
      #[must_use]
      pub fn adapter(&self) -> Git {
          Git::open(self.path.clone())
      }

      /// Writes a file, making the directory it lives in.
      ///
      /// # Panics
      ///
      /// When the file cannot be written.
      pub fn write(&self, name: &str, text: &str) {
          let path = self.path.join(name);
          if let Some(directory) = path.parent() {
              std::fs::create_dir_all(directory).expect("the directory the file is in");
          }
          std::fs::write(path, text).expect("the file is written");
      }

      /// Stages everything and commits it.
      ///
      /// # Panics
      ///
      /// When git refuses.
      pub fn commit(&self, message: &str) {
          self.git(&["add", "-A"]);
          self.git(&["commit", "-m", message]);
      }

      /// Runs git in this repository, for the setup a test needs.
      ///
      /// # Panics
      ///
      /// When git refuses, which is a fixture that cannot be built rather than a test that failed.
      pub fn git(&self, arguments: &[&str]) {
          git_in(&self.path, arguments);
      }

      /// Asks git something in this repository, for a test that checks the adapter against git itself.
      ///
      /// # Panics
      ///
      /// When git refuses.
      #[must_use]
      pub fn git_output(&self, arguments: &[&str]) -> String {
          git_output_in(&self.path, arguments)
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
  /// purpose, which is the whole reason Farik shells out to git at all. So no assertion resting on
  /// anything a setting can reshape is safe — where one would, the adapter names the flag that makes
  /// the answer its own rather than the configuration's.
  ///
  /// # Panics
  ///
  /// When git is not installed or refuses.
  pub fn git_in(directory: &Path, arguments: &[&str]) {
      let _ = git_output_in(directory, arguments);
  }

  /// Asks git something in a directory that is not a `TempRepo` — a clone, an origin, a directory that
  /// is not a repository at all.
  ///
  /// # Panics
  ///
  /// When git is not installed or refuses.
  #[must_use]
  pub fn git_output_in(directory: &Path, arguments: &[&str]) -> String {
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
  ```

- [x] Declare it in `crates/store/src/git.rs`, between the error's `std::error::Error` line and `HeadSummary`:

  ```rust
  /// A repository of its own, for tests in this crate and in others.
  pub mod fixtures;
  ```

- [x] Run the check and confirm green. Nothing moved but the fixture, so every count is what step 06 left:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 264 passed (farik-core)
  #   test result: ok. 49 passed (farik-store)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 31 passed (crates/store/tests/project_files.rs)
  ```

- [x] Commit: `refactor(store): make the repository fixture public`

### Task 2: What the repository tracks

Files: modified `crates/store/src/git.rs`, `crates/store/tests/git.rs`, `docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md`

Consumes: Task 1's fixture
Produces: `Git::tracked_paths` and `Git::root`

- [x] Write the failing tests. In `crates/store/tests/git.rs`, insert before the `#[test]` of `fn keeps_a_path_whose_name_begins_with_a_space` — before its attributes, not between them and the function:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn lists_what_the_repository_tracks_and_not_what_it_ignores() {
      let repository = TempRepo::new("tracked");
      assert_eq!(
          repository.adapter().root(),
          repository.path,
          "the adapter answers about the directory it was opened on"
      );
      repository.write(".gitignore", "ignored/\n*.log\n");
      repository.write("src/lib.rs", "// code\n");
      repository.write("a path with a space.md", "# spaces\n");
      repository.write("ignored/secret.txt", "not content\n");
      repository.write("noisy.log", "not content\n");
      repository.commit("a tree to scan");

      assert_eq!(
          repository.adapter().tracked_paths().expect("it lists"),
          [
              ".gitignore",
              "README.md",
              "a path with a space.md",
              "src/lib.rs"
          ],
          "what git ignores is not content, and Farik does not have to know what to ignore"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn says_where_the_repository_begins_whatever_directory_it_was_opened_on() {
      // `Git::open` takes any directory inside a repository, so `root` and the repository's own root
      // are two different questions. A caller that means the project rather than a subtree has to be
      // able to tell.
      let repository = TempRepo::new("top-level");
      repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      repository.commit("a subdirectory");
      let inside = Git::open(repository.path.join("crates/core"));

      assert_eq!(
          inside.root(),
          repository.path.join("crates/core"),
          "root is the directory it was opened on"
      );
      assert_eq!(
          std::fs::canonicalize(inside.top_level().expect("git says where it begins"))
              .expect("a real directory"),
          std::fs::canonicalize(&repository.path).expect("a real directory"),
          "and top_level is where the repository does"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn lists_what_is_staged_in_a_repository_with_no_commit() {
      // A repository `farik init` has just made has an index and no commit, and onboarding scans it.
      // The fixture commits, so the commit goes: `git rm --cached` alone would only unstage the file
      // and leave HEAD where it was, and the test would prove nothing about a repository with none.
      let repository = TempRepo::new("tracked-staged");
      repository.git(&["update-ref", "-d", "HEAD"]);
      repository.git(&["rm", "--cached", "-q", "README.md"]);
      assert_eq!(
          repository.adapter().tracked_paths().expect("it lists"),
          Vec::<String>::new(),
          "nothing is tracked once the only file is out of the index"
      );
      repository.write("staged.rs", "// staged\n");
      repository.git(&["add", "staged.rs"]);
      assert_eq!(
          repository.adapter().tracked_paths().expect("it lists"),
          ["staged.rs"]
      );
  }
  ```

- [x] Run them and confirm they fail because the adapter answers neither question yet:

  ```
  cargo test -p farik-store --test git
  # expected: FAIL to compile, one error per call site:
  # error[E0599]: no method named `root` found for struct `Git` in the current scope
  #   (twice)
  # error[E0599]: no method named `top_level` found for struct `Git` in the current scope
  # error[E0599]: no method named `tracked_paths` found for struct `Git` in the current
  #   scope  (three times)
  # error: could not compile `farik-store` (test "git") due to 6 previous errors
  ```

- [x] Write the minimal implementation. First the rename, because the method below calls the helper by its new name and a helper that splits git's `-z` output is no longer only about what changed. In `crates/store/src/git.rs`, replace the unit tests' import, whole — rustfmt sorts a braced group, and `paths_of` sorts after `path_argument` where `changed_paths_of` sorted before both:

  ```rust
      use super::{
          Git, GitError, HeadSummary, default_branch_of, head_summary_of, path_argument, paths_of,
      };
  ```

  then rename the remaining six occurrences of `changed_paths_of` to `paths_of`: its definition, the call in `changed_paths`, the call in `merge`, and the three in `reads_the_paths_git_separated_by_nothing`. Its doc comment names no function and does not change.

- [x] Insert, before `/// Whether \`root\` is inside a git repository.`:

  ```rust
      /// The directory this adapter was opened on, which may be inside a repository rather than at
      /// its root. `top_level` is what git says the root is.
      #[must_use]
      pub fn root(&self) -> &Path {
          &self.root
      }

      /// Where the repository begins, as git reports it.
      ///
      /// `Git::open` accepts any directory inside a repository, so the two can differ, and a caller
      /// that means the project rather than a subtree has to ask.
      ///
      /// # Errors
      ///
      /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses.
      pub fn top_level(&self) -> Result<String, GitError> {
          self.require_repository()?;
          self.at_root(&["rev-parse", "--show-toplevel"])
      }
  ```

- [x] And insert, before `/// What \`head\` changed since it and \`base\` last agreed, as a patch.`:

  ```rust
      /// Every path the repository tracks, in git's own order.
      ///
      /// Through git rather than by walking the directory, so that `.gitignore` decides what is not
      /// content without Farik having to know that `node_modules/` and `target/` exist. What git
      /// tracks is the index, so a repository with no commit yet lists what has been staged.
      ///
      /// # Errors
      ///
      /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses.
      pub fn tracked_paths(&self) -> Result<Vec<String>, GitError> {
          self.require_repository()?;
          let listed = self.at_root(&["ls-files", "-z"])?;
          Ok(paths_of(&listed))
      }
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 18 passed (crates/store/tests/git.rs)
  ```

- [x] Commit: `feat(store): list what the repository tracks`

### Task 3: The scan

Files: created `crates/store/src/scan.rs`, `crates/store/tests/project_scan.rs`; modified `crates/store/src/lib.rs`, `docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md`

Consumes: everything above
Produces: `farik_store::{ProjectScan, ScanError, scan_project, seeded_library}`

- [x] Write the failing tests. Create `crates/store/src/scan.rs` with the module doc and the tests module, and nothing else yet:

  ```rust
  //! The project scan (`docs/SPEC.md` section 4's onboarding, 5.8, 5.13, F2 and F16).
  //!
  //! A person points Farik at a git repository and is told what it thinks it is looking at: one line,
  //! `TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago`. The same
  //! reading gives the criterion library its first entries, so that a contract in this project is
  //! verified by the project's own commands rather than by something a Product Manager invented.
  //!
  //! Every signal is a file the repository tracks or a line in one of its manifests. Nothing is
  //! guessed from a name, nothing is run, and the answer for one tree is the same every time.

  #[cfg(test)]
  mod tests {
      use chrono::{DateTime, Utc};
      use farik_core::criteria::{CriteriaLibrary, CriterionSource, validate_criteria};
      use serde_json::json;

      use super::{
          Reading, ScanError, TOOLCHAINS, how_long_ago, language_of, packages_in, scripts_in,
          seeded_library, tests_in,
      };

      fn at(text: &str) -> DateTime<Utc> {
          DateTime::parse_from_rfc3339(text)
              .expect("a stamp")
              .with_timezone(&Utc)
      }

      fn strings(paths: &[&str]) -> Vec<String> {
          paths.iter().map(ToString::to_string).collect()
      }

      fn toolchain(name: &str) -> &'static super::Toolchain {
          TOOLCHAINS
              .iter()
              .find(|toolchain| toolchain.name == name)
              .expect("a toolchain the table knows")
      }

      #[test]
      fn says_how_long_ago_a_commit_was_in_words_a_person_uses() {
          let now = at("2026-09-17T12:00:00Z");
          assert_eq!(how_long_ago("2026-09-17T09:00:00+00:00", now), "today");
          assert_eq!(how_long_ago("2026-09-16T09:00:00+00:00", now), "yesterday");
          assert_eq!(
              how_long_ago("2026-09-13T12:00:00+00:00", now),
              "4 days ago",
              "which is the read-back section 4 gives as its example"
          );
          assert_eq!(
              how_long_ago("2026-08-19T12:00:00+00:00", now),
              "29 days ago"
          );
          assert_eq!(
              how_long_ago("2026-08-18T12:00:00+00:00", now),
              "a month ago"
          );
          assert_eq!(
              how_long_ago("2026-03-17T12:00:00+00:00", now),
              "6 months ago"
          );
      }

      #[test]
      fn says_the_stamp_itself_when_it_is_not_one_it_can_use() {
          // A length of time nobody can check is worse than the stamp. A commit in the future is the
          // ordinary result of two machines whose clocks disagree.
          let now = at("2026-09-17T12:00:00Z");
          assert_eq!(how_long_ago("yesterday-ish", now), "at yesterday-ish");
          assert_eq!(
              how_long_ago("2026-09-17T13:00:00+00:00", now),
              "at 2026-09-17T13:00:00+00:00",
              "an hour ahead is ahead: two machines whose clocks disagree are hours apart, not days"
          );
          assert_eq!(
              how_long_ago("2026-09-18T12:00:00+00:00", now),
              "at 2026-09-18T12:00:00+00:00"
          );
      }

      #[test]
      fn names_the_language_with_the_most_files() {
          assert_eq!(
              language_of(&strings(&["src/a.ts", "src/b.tsx", "build.rs"])),
              Some("TypeScript")
          );
          assert_eq!(
              language_of(&strings(&["src/lib.rs", "src/main.rs", "web/app.ts"])),
              Some("Rust")
          );
          assert_eq!(language_of(&strings(&["README.md", "LICENSE"])), None);
          assert_eq!(
              language_of(&strings(&["one.ts", "two.rs"])),
              Some("TypeScript"),
              "a tie goes to whichever language comes first in the table"
          );
          assert_eq!(
              language_of(&strings(&["one.rs", "two.py"])),
              Some("Rust"),
              "and the table's order is the order a person would name them in"
          );
          assert_eq!(
              language_of(&strings(&["src/A.RS"])),
              Some("Rust"),
              "an extension is an extension whatever case a person typed it in"
          );
      }

      #[test]
      fn counts_one_package_per_manifest_below_the_root() {
          assert_eq!(
              packages_in(&strings(&[
                  "package.json",
                  "packages/ui/package.json",
                  "packages/web/package.json",
                  "apps/desktop/package.json",
              ])),
              3,
              "the root's own manifest is the workspace, not a package in it"
          );
          assert_eq!(packages_in(&strings(&["package.json"])), 0);
      }

      #[test]
      fn names_the_test_runner_a_project_configures_or_depends_on() {
          assert_eq!(
              tests_in(&strings(&["vitest.config.ts"]), None),
              Some("vitest")
          );
          assert_eq!(tests_in(&strings(&["tests/test_it.py"]), None), None);
          assert_eq!(
              tests_in(&strings(&["packages/ui/vitest.config.ts"]), None),
              Some("vitest"),
              "a package's own configuration counts wherever it sits"
          );
          assert_eq!(
              tests_in(&strings(&["docs/jest-to-vitest-migration.md"]), None),
              None,
              "and a file that writes about a runner is not a project that uses one"
          );
          let manifest = json!({ "devDependencies": { "jest": "^30.0.0" } });
          assert_eq!(tests_in(&[], Some(&manifest)), Some("jest"));
          assert_eq!(tests_in(&[], None), None);
      }

      #[test]
      fn reads_the_script_names_a_manifest_defines() {
          let manifest = json!({ "scripts": { "test": "vitest run", "build": "tsc" } });
          let mut found = scripts_in(Some(&manifest));
          found.sort();
          assert_eq!(found, ["build", "test"]);
          assert!(scripts_in(None).is_empty());

          // A name with nothing behind it would become a criterion whose command verifies nothing.
          let empty = json!({ "scripts": { "test": "", "lint": "   ", "build": 7, "check": "tsc" } });
          assert_eq!(scripts_in(Some(&empty)), ["check"]);
      }

      #[test]
      fn reads_back_the_line_section_4_asks_for() {
          let reading = Reading {
              language: Some("TypeScript"),
              toolchain: Some(toolchain("pnpm")),
              is_workspace: true,
              packages: 3,
              tests: Some("vitest"),
              scripts: Vec::new(),
          };
          let head = crate::git::HeadSummary {
              sha: "a1b2c3".to_string(),
              committed_at: "2026-09-13T12:00:00+00:00".to_string(),
              subject: "a commit".to_string(),
          };
          assert_eq!(
              reading.read_back(
                  &strings(&["package.json"]),
                  Some(&head),
                  at("2026-09-17T12:00:00Z")
              ),
              "TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago"
          );
      }

      #[test]
      fn a_workspace_of_one_package_is_not_a_monorepo() {
          let reading = Reading {
              language: Some("TypeScript"),
              toolchain: Some(toolchain("pnpm")),
              is_workspace: true,
              packages: 1,
              tests: None,
              scripts: Vec::new(),
          };
          assert_eq!(
              reading.read_back(
                  &strings(&["package.json"]),
                  None,
                  at("2026-09-17T12:00:00Z")
              ),
              "TypeScript, pnpm, no commits yet",
              "one package is a workspace and not a monorepo, and `1 packages` is not a count"
          );
      }

      #[test]
      fn reads_back_what_it_can_of_a_project_it_recognises_nothing_in() {
          let reading = Reading {
              language: None,
              toolchain: None,
              is_workspace: false,
              packages: 0,
              tests: None,
              scripts: Vec::new(),
          };
          assert_eq!(
              reading.read_back(&[], None, at("2026-09-17T12:00:00Z")),
              "nothing tracked yet, no commits yet"
          );
      }

      #[test]
      fn builds_a_criterion_the_library_accepts_for_every_toolchain() {
          // The tables in this module describe criteria, and the library's own rules are what say
          // whether they are ones. A name that is not kebab-case or a text under ten characters would
          // be a defect nothing else here would catch.
          for toolchain in TOOLCHAINS {
              let reading = Reading {
                  language: None,
                  toolchain: Some(toolchain),
                  is_workspace: false,
                  packages: 0,
                  tests: None,
                  scripts: super::SCRIPTS
                      .iter()
                      .map(|(script, ..)| (*script).to_string())
                      .collect(),
              };
              let criteria = reading
                  .criteria()
                  .unwrap_or_else(|error| panic!("{}: {error}", toolchain.name));
              assert!(
                  !criteria.is_empty(),
                  "{} offers a project nothing",
                  toolchain.name
              );
              assert!(
                  criteria
                      .iter()
                      .all(|one| one.source == Some(CriterionSource::ProjectScan)),
                  "{}: the scan found these, and a refresh may replace them",
                  toolchain.name
              );
          }
      }

      #[test]
      fn a_project_whose_toolchain_it_does_not_know_gets_no_criteria() {
          let reading = Reading {
              language: Some("Swift"),
              toolchain: None,
              is_workspace: false,
              packages: 0,
              tests: None,
              scripts: Vec::new(),
          };
          assert_eq!(
              reading.criteria().expect("no criteria is not a refusal"),
              []
          );
      }

      #[test]
      fn only_the_scripts_a_project_has_become_criteria() {
          let reading = Reading {
              language: Some("TypeScript"),
              toolchain: Some(toolchain("pnpm")),
              is_workspace: false,
              packages: 0,
              tests: None,
              scripts: strings(&["test", "lint", "release"]),
          };
          let criteria = reading.criteria().expect("two of them");
          assert_eq!(
              criteria
                  .iter()
                  .map(|one| one.name.as_str())
                  .collect::<Vec<_>>(),
              ["the-tests-pass", "the-linter-is-clean"],
              "a script the table does not know is not a criterion, and one it knows that the project \
               does not have is not either"
          );
      }

      #[test]
      fn a_refresh_replaces_what_the_scan_found_and_keeps_what_a_person_wrote() {
          let wire = json!({ "criteria": [
              { "name": "the-tests-pass", "text": "An older scan found this one.",
                "source": "project_scan",
                "verification": { "method": "test", "command": "npm test" } },
              { "name": "the-docs-are-updated", "text": "A person wrote this one by hand.",
                "source": "human",
                "verification": { "method": "review", "rubric": ["Are the docs updated?"] } },
              { "name": "it-looks-right", "text": "And this one, without saying so.",
                "verification": { "method": "human", "question": "Does it look right?" } },
          ]});
          let existing = validate_criteria(&wire).expect("a library");
          let found = validate_criteria(&json!({ "criteria": [
              { "name": "the-tests-pass", "text": "What this scan found instead.",
                "source": "project_scan",
                "verification": { "method": "test", "command": "pnpm run test" } },
              { "name": "clippy-is-clean", "text": "And one it did not find before.",
                "source": "project_scan",
                "verification": { "method": "command", "command": "cargo clippy",
                                  "expect": { "exit_code": 0 } } },
          ]}))
          .expect("a library")
          .criteria;

          let seeded = seeded_library(&found, Some(&existing));
          assert_eq!(
              seeded
                  .criteria
                  .iter()
                  .map(|one| one.name.as_str())
                  .collect::<Vec<_>>(),
              [
                  "the-docs-are-updated",
                  "it-looks-right",
                  "the-tests-pass",
                  "clippy-is-clean"
              ],
              "what a person wrote comes first and is kept; what a scan found is replaced"
          );
          assert_eq!(
              seeded.criteria[2].text.as_str(),
              "What this scan found instead."
          );
      }

      #[test]
      fn a_name_a_person_has_used_is_a_name_the_scan_leaves_alone() {
          let existing = validate_criteria(&json!({ "criteria": [
              { "name": "the-tests-pass", "text": "A person means something else by this.",
                "source": "human",
                "verification": { "method": "review", "rubric": ["Do the tests pass?"] } },
          ]}))
          .expect("a library");
          let found = validate_criteria(&json!({ "criteria": [
              { "name": "the-tests-pass", "text": "What the scan would have called it.",
                "source": "project_scan",
                "verification": { "method": "test", "command": "cargo test" } },
          ]}))
          .expect("a library")
          .criteria;

          let seeded = seeded_library(&found, Some(&existing));
          assert_eq!(seeded.criteria.len(), 1);
          assert_eq!(
              seeded.criteria[0].text.as_str(),
              "A person means something else by this.",
              "a refresh never touches what a person wrote, name included"
          );
      }

      #[test]
      fn a_project_with_no_library_yet_gets_what_the_scan_found() {
          let found = validate_criteria(&json!({ "criteria": [
              { "name": "the-tests-pass", "text": "Every test in the workspace passes.",
                "source": "project_scan",
                "verification": { "method": "test", "command": "cargo test --workspace" } },
          ]}))
          .expect("a library")
          .criteria;
          assert_eq!(
              seeded_library(&found, None),
              CriteriaLibrary {
                  criteria: found.clone()
              }
          );
          assert_eq!(
              seeded_library(&[], None),
              CriteriaLibrary {
                  criteria: Vec::new()
              },
              "a project whose scan found nothing still has a library"
          );
      }

      #[test]
      fn says_what_it_could_not_read_and_why() {
          assert_eq!(
              [
                  ScanError::NotARepository {
                      path: "/home/ada/notes".to_string()
                  }
                  .to_string(),
                  ScanError::Git {
                      detail: "git is not installed".to_string()
                  }
                  .to_string(),
                  ScanError::Io {
                      path: "package.json".to_string(),
                      detail: "Permission denied (os error 13)".to_string()
                  }
                  .to_string(),
                  ScanError::Built {
                      detail: "/criteria/0/name does not match".to_string()
                  }
                  .to_string(),
                  ScanError::NotTheRepositoryRoot {
                      path: "/home/ada/project/crates/core".to_string(),
                      root: "/home/ada/project".to_string()
                  }
                  .to_string(),
              ],
              [
                  "/home/ada/notes is not a git repository",
                  "git refused: git is not installed",
                  "package.json could not be read: Permission denied (os error 13)",
                  "the scan built a criterion that is not one: /criteria/0/name does not match",
                  "/home/ada/project/crates/core is inside the repository at /home/ada/project rather \
                   than its root, and a project is a whole repository",
              ]
          );
      }
  }
  ```

- [x] Create `crates/store/tests/project_scan.rs`:

  ```rust
  //! The project scan against a real repository.
  //!
  //! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
  //! `cargo xtask check --integration`, as `git.rs` is and for the same reasons.

  use chrono::{DateTime, Utc};
  use farik_store::git::fixtures::{TempRepo, git_in};
  use farik_store::{Git, ScanError, scan_project};

  /// A moment to scan at, so that "last commit today" is an answer rather than a guess.
  fn now() -> DateTime<Utc> {
      Utc::now()
  }

  fn read_back(repository: &TempRepo) -> String {
      scan_project(&repository.adapter(), now())
          .expect("it scans")
          .read_back
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_back_a_typescript_monorepo_the_way_section_4_does() {
      let repository = TempRepo::new("scan-monorepo");
      repository.write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
      repository.write("pnpm-workspace.yaml", "packages:\n  - packages/*\n");
      repository.write(
          "package.json",
          r#"{"name":"root","devDependencies":{"vitest":"^3.0.0"},
              "scripts":{"test":"vitest run","build":"tsc -b","lint":"eslint ."}}"#,
      );
      repository.write("packages/ui/package.json", "{\"name\":\"ui\"}\n");
      repository.write("packages/ui/index.ts", "export const one = 1;\n");
      repository.write("packages/web/package.json", "{\"name\":\"web\"}\n");
      repository.write("packages/web/app.tsx", "export const App = () => null;\n");
      repository.write("apps/desktop/package.json", "{\"name\":\"desktop\"}\n");
      repository.write("apps/desktop/main.ts", "export const main = 1;\n");
      repository.commit("a monorepo");

      assert_eq!(
          read_back(&repository),
          "TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit today"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_the_project_own_scripts_rather_than_guessing_them() {
      let repository = TempRepo::new("scan-scripts");
      repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
      repository.write(
          "package.json",
          r#"{"name":"app","scripts":{"test":"node --test","typecheck":"tsc --noEmit",
              "release":"./ship.sh"}}"#,
      );
      repository.write("src/index.js", "module.exports = 1;\n");
      repository.commit("an application");

      let scan = scan_project(&repository.adapter(), now()).expect("it scans");
      assert_eq!(
          scan.detected_criteria
              .iter()
              .map(|one| (one.name.as_str(), command_of(&one.verification)))
              .collect::<Vec<_>>(),
          [
              ("the-tests-pass", "npm run test".to_string()),
              ("the-types-check", "npm run typecheck".to_string()),
          ],
          "a script the project has becomes a criterion; one it does not have does not, and neither \
           does one Farik has no name for"
      );
      assert_eq!(scan.read_back, "JavaScript, npm, last commit today");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_back_a_rust_workspace_and_the_commands_cargo_always_has() {
      let repository = TempRepo::new("scan-rust");
      repository.write("Cargo.lock", "version = 4\n");
      repository.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
      repository.write("crates/core/Cargo.toml", "[package]\nname = \"core\"\n");
      repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      repository.write("crates/store/Cargo.toml", "[package]\nname = \"store\"\n");
      repository.write("crates/store/src/lib.rs", "pub fn two() -> u8 { 2 }\n");
      repository.commit("a workspace");

      let scan = scan_project(&repository.adapter(), now()).expect("it scans");
      assert_eq!(
          scan.read_back,
          "Rust monorepo, cargo, 2 packages, last commit today"
      );
      assert_eq!(
          scan.detected_criteria
              .iter()
              .map(|one| one.name.as_str())
              .collect::<Vec<_>>(),
          [
              "the-tests-pass",
              "the-build-succeeds",
              "clippy-is-clean",
              "formatting-is-clean"
          ],
          "F2 asks for the test and the build commands, and cargo has both"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_nothing_out_of_what_git_ignores() {
      // The scan reads the tree through git, so a vendored dependency directory is not the project.
      let repository = TempRepo::new("scan-ignored");
      repository.write(".gitignore", "node_modules/\n");
      repository.write("Cargo.lock", "version = 4\n");
      repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
      repository.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      for index in 0..20 {
          repository.write(
              &format!("node_modules/dep{index}/index.ts"),
              "export const x = 1;\n",
          );
      }
      repository.commit("a rust project with a node_modules nobody asked for");

      assert_eq!(
          read_back(&repository),
          "Rust, cargo, last commit today",
          "twenty TypeScript files git ignores do not make this a TypeScript project"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_back_a_repository_with_nothing_in_it() {
      let repository = TempRepo::new("scan-empty");
      repository.git(&["rm", "--cached", "-q", "README.md"]);
      repository.git(&["update-ref", "-d", "HEAD"]);
      assert_eq!(
          read_back(&repository),
          "nothing tracked yet, no commits yet"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_back_a_repository_it_recognises_nothing_in() {
      let repository = TempRepo::new("scan-unknown");
      repository.write("notes.txt", "a folder of notes\n");
      repository.commit("notes");
      assert_eq!(read_back(&repository), "last commit today");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_directory_that_is_not_a_repository() {
      let plain = std::env::temp_dir().join(format!("farik-scan-plain-{}", std::process::id()));
      std::fs::create_dir_all(&plain).expect("a plain directory");
      let refused = scan_project(&Git::open(plain.clone()), now());
      let _ = std::fs::remove_dir_all(&plain);
      assert_eq!(
          refused,
          Err(ScanError::NotARepository {
              path: plain.display().to_string()
          }),
          "a project is a git repository plus .farik/, and this is neither"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_tree_that_is_missing_a_file_it_tracks() {
      // The other half of the decision below: a manifest that is not JSON is a project mid-edit, and a
      // manifest git tracks that is not there at all is a tree half-checked-out. The second is a fact
      // about the checkout rather than about the project, so the scan refuses instead of guessing.
      let repository = TempRepo::new("scan-missing-manifest");
      repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
      repository.write(
          "package.json",
          "{\"name\":\"app\",\"scripts\":{\"test\":\"vitest\"}}\n",
      );
      repository.write("src/index.ts", "export const one = 1;\n");
      repository.commit("a project");
      std::fs::remove_file(repository.path.join("package.json"))
          .expect("and a half-checked-out tree");

      let Err(ScanError::Io { path, detail }) = scan_project(&repository.adapter(), now()) else {
          panic!("a file git tracks that is not there is a tree half-written");
      };
      assert_eq!(path, "package.json");
      assert!(detail.contains("No such file or directory"), "{detail}");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn says_what_git_said_about_a_repository_with_no_working_tree() {
      // A bare repository is a repository, so `is_repository` says yes, and it has nothing to scan.
      // Saying that better needs a refusal of its own, which step 08's `farik doctor` row records; what
      // this step promises is that git's own sentence reaches the person rather than being swallowed.
      let bare = std::env::temp_dir().join(format!("farik-scan-bare-{}.git", std::process::id()));
      let _ = std::fs::remove_dir_all(&bare);
      std::fs::create_dir_all(&bare).expect("a directory");
      git_in(&bare, &["init", "--bare", "-q"]);
      let refused = scan_project(&Git::open(bare.clone()), now());
      let _ = std::fs::remove_dir_all(&bare);

      let Err(ScanError::Git { detail }) = refused else {
          panic!("there is no working tree to scan: {refused:?}");
      };
      assert!(detail.contains("must be run in a work tree"), "{detail}");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn a_manifest_that_is_not_json_is_a_fact_about_the_project() {
      // Half a `package.json` is something to say in the read-back, not a reason to refuse the scan.
      let repository = TempRepo::new("scan-broken-manifest");
      repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
      repository.write("package.json", "{\"name\": \"half a manifest\"\n");
      repository.write("src/index.ts", "export const one = 1;\n");
      repository.commit("a manifest a person was editing");

      let scan = scan_project(&repository.adapter(), now()).expect("it still scans");
      assert_eq!(scan.read_back, "TypeScript, npm, last commit today");
      assert!(
          scan.detected_criteria.is_empty(),
          "no scripts could be read, so no criterion was found"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn takes_the_toolchain_the_language_names_when_a_project_has_two() {
      // A Rust workspace with a front end, which is the shape this repository itself takes. One
      // toolchain is chosen, because a criterion called `the-tests-pass` can only mean one command,
      // and the tree says this is mostly Rust.
      let repository = TempRepo::new("scan-polyglot");
      repository.write("Cargo.lock", "version = 4\n");
      repository.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
      repository.write("crates/core/Cargo.toml", "[package]\nname = \"core\"\n");
      repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      repository.write("crates/store/Cargo.toml", "[package]\nname = \"store\"\n");
      repository.write("crates/store/src/lib.rs", "pub fn two() -> u8 { 2 }\n");
      repository.write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
      repository.write("package.json", "{\"scripts\":{\"build\":\"tsc -b\"}}\n");
      repository.write("web/app.ts", "export const one = 1;\n");
      repository.commit("a rust workspace with a front end");

      let scan = scan_project(&repository.adapter(), now()).expect("it scans");
      assert_eq!(
          scan.read_back,
          "Rust monorepo, cargo, 2 packages, last commit today"
      );
      assert_eq!(
          scan.detected_criteria
              .iter()
              .map(|one| one.name.as_str())
              .collect::<Vec<_>>(),
          [
              "the-tests-pass",
              "the-build-succeeds",
              "clippy-is-clean",
              "formatting-is-clean"
          ],
          "the library gets the commands of the language the project is, not of the one it also has"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_directory_inside_a_repository_rather_than_scanning_a_subtree() {
      // Onboarding asks a person to pick the project's folder (spec 4). A subtree would answer
      // confidently about a project it had only seen part of.
      let repository = TempRepo::new("scan-subtree");
      repository.write("Cargo.lock", "version = 4\n");
      repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
      repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      repository.commit("a project with a subdirectory");

      let inside = Git::open(repository.path.join("crates/core"));
      assert!(inside.is_repository(), "it is inside a repository");
      let Err(ScanError::NotTheRepositoryRoot { path, root }) = scan_project(&inside, now()) else {
          panic!("a subtree is not a project");
      };
      assert!(path.ends_with("crates/core"), "{path}");
      assert!(!root.ends_with("crates/core"), "{root}");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_list_what_a_directory_that_is_not_a_repository_tracks() {
      // `tracked_paths` promises this, and `scan_project` asks `is_repository` before it, so nothing
      // else would notice if the adapter stopped checking.
      let plain = std::env::temp_dir().join(format!("farik-tracked-plain-{}", std::process::id()));
      std::fs::create_dir_all(&plain).expect("a plain directory");
      let refused = Git::open(plain.clone()).tracked_paths();
      let _ = std::fs::remove_dir_all(&plain);
      assert_eq!(refused, Err(farik_store::GitError::NotARepository));
  }

  /// The command a template's verification runs, whichever method it is.
  fn command_of(verification: &farik_core::criteria::TemplateVerification) -> String {
      match verification {
          farik_core::criteria::TemplateVerification::Variant0 { command, .. }
          | farik_core::criteria::TemplateVerification::Variant1 { command, .. } => command.clone(),
          other => panic!("a criterion the scan found runs a command: {other:?}"),
      }
  }
  ```

- [x] Declare the module in `crates/store/src/lib.rs`, after `projections` and before `pub use`:

  ```rust
  /// What the repository says it is.
  pub mod scan;
  ```

- [x] Run them and confirm they fail because nothing of the scan exists. One command per target: `cargo test -p farik-store` builds both at once and cancels whichever it had not finished when the other failed, so which errors a run prints is a race.

  ```
  cargo test -p farik-store --lib
  # expected: FAIL to compile, three errors — the tests module names everything the
  # implementation has not brought yet:
  # error[E0432]: unresolved imports `super::Reading`, `super::ScanError`,
  #   `super::TOOLCHAINS`, `super::how_long_ago`, `super::language_of`,
  #   `super::packages_in`, `super::scripts_in`, `super::seeded_library`,
  #   `super::tests_in`
  # error[E0425]: cannot find type `Toolchain` in module `super`
  # error[E0425]: cannot find value `SCRIPTS` in module `super`
  # error: could not compile `farik-store` (lib test) due to 3 previous errors
  ```

  ```
  cargo test -p farik-store --test project_scan
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `farik_store::ScanError`,
  #   `farik_store::scan_project`
  # error: could not compile `farik-store` (test "project_scan") due to 1 previous error
  ```

- [x] Write the minimal implementation. In `crates/store/src/scan.rs`, put the imports between the module doc and the tests module, so that the top of the file reads, whole:

  ```rust
  //! The project scan (`docs/SPEC.md` section 4's onboarding, 5.8, 5.13, F2 and F16).
  //!
  //! A person points Farik at a git repository and is told what it thinks it is looking at: one line,
  //! `TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago`. The same
  //! reading gives the criterion library its first entries, so that a contract in this project is
  //! verified by the project's own commands rather than by something a Product Manager invented.
  //!
  //! Every signal is a file the repository tracks or a line in one of its manifests. Nothing is
  //! guessed from a name, nothing is run, and the answer for one tree is the same every time.

  use std::collections::BTreeMap;
  use std::fmt;
  use std::path::Path;

  use chrono::{DateTime, Utc};
  use farik_core::criteria::{
      CriteriaLibrary, CriterionSource, CriterionTemplate, validate_criteria,
  };
  use serde_json::{Value, json};

  use crate::git::{Git, GitError, HeadSummary};

  /// What the scan read back about a project.
  #[derive(Debug, Clone, PartialEq)]
  pub struct ProjectScan {
      /// The one line a person is shown, and what `project.md` holds.
      pub read_back: String,
      /// The criteria the project's own commands make available, each with `source: project_scan`.
      pub detected_criteria: Vec<CriterionTemplate>,
  }
  ```

- [x] Then, after it, why a project could not be scanned:

  ```rust
  /// Why a project could not be scanned.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ScanError {
      /// There is no git repository there. A project is a git repository plus `.farik/` (spec 3), and
      /// the scan reads the tree through git, so this is the first thing onboarding checks.
      NotARepository {
          /// The directory that was asked about.
          path: String,
      },
      /// There is a repository, and this is a directory inside it rather than its root. Onboarding
      /// asks a person to pick the project's folder (spec 4); scanning a subtree would answer
      /// confidently about a project it had only seen part of.
      NotTheRepositoryRoot {
          /// The directory that was asked about.
          path: String,
          /// Where the repository actually begins.
          root: String,
      },
      /// Git refused.
      Git {
          /// What it said.
          detail: String,
      },
      /// A file the scan wanted to read could not be read. A file git tracks that is not there is a
      /// tree half-written, not a project.
      Io {
          /// The path, relative to the project root.
          path: String,
          /// What the operating system said.
          detail: String,
      },
      /// The scan built a criterion the library's own rules refuse. That is a defect in the tables in
      /// this module rather than anything about the project, and it is reported rather than unwrapped.
      Built {
          /// Which rule, in `validate_criteria`'s words.
          detail: String,
      },
  }

  impl fmt::Display for ScanError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::NotARepository { path } => {
                  write!(formatter, "{path} is not a git repository")
              }
              Self::NotTheRepositoryRoot { path, root } => write!(
                  formatter,
                  "{path} is inside the repository at {root} rather than its root, and a project is a \
                   whole repository"
              ),
              Self::Git { detail } => write!(formatter, "git refused: {detail}"),
              Self::Io { path, detail } => write!(formatter, "{path} could not be read: {detail}"),
              Self::Built { detail } => write!(
                  formatter,
                  "the scan built a criterion that is not one: {detail}"
              ),
          }
      }
  }

  impl std::error::Error for ScanError {}

  impl From<GitError> for ScanError {
      fn from(error: GitError) -> Self {
          Self::Git {
              detail: error.to_string(),
          }
      }
  }
  ```

- [x] Then the scan itself:

  ```rust
  /// Reads a repository and says what it is.
  ///
  /// The root comes from the adapter rather than beside it, so the tree the paths are listed from is
  /// the tree the manifests are read from. `now` is the clock, because the read-back says how long ago
  /// the last commit was and a function that sampled the clock itself could not be held to an answer.
  ///
  /// # Errors
  ///
  /// `NotARepository` when there is no repository there, `Git` when git refuses, `Io` when a file git
  /// tracks cannot be read, `Built` when the tables in this module describe a criterion the library
  /// would refuse.
  pub fn scan_project(git: &Git, now: DateTime<Utc>) -> Result<ProjectScan, ScanError> {
      if !git.is_repository() {
          return Err(ScanError::NotARepository {
              path: git.root().display().to_string(),
          });
      }
      let top = git.top_level()?;
      if !same_directory(git.root(), Path::new(&top)) {
          return Err(ScanError::NotTheRepositoryRoot {
              path: git.root().display().to_string(),
              root: top,
          });
      }
      let tracked = git.tracked_paths()?;
      let commit = git.head_summary()?;
      let reading = Reading::of(git.root(), &tracked)?;
      Ok(ProjectScan {
          read_back: reading.read_back(&tracked, commit.as_ref(), now),
          detected_criteria: reading.criteria()?,
      })
  }
  ```

- [x] Then what a project's library becomes after one:

  ```rust
  /// The criterion library a project has after a scan.
  ///
  /// What the scan found replaces what a previous scan found, and never touches what a person wrote:
  /// `criteria.schema.json` says so of its `source` field, and a person who edited a criterion by hand
  /// should not have it taken away by a refresh. A name a person has used is a name the scan leaves
  /// alone, for the same reason.
  ///
  /// The result may hold more criteria than the library's schema allows, and `write_criteria` is what
  /// refuses it — one rule in one place, named against the file it is about.
  #[must_use]
  pub fn seeded_library(
      found: &[CriterionTemplate],
      existing: Option<&CriteriaLibrary>,
  ) -> CriteriaLibrary {
      let kept: Vec<CriterionTemplate> = existing
          .map(|library| {
              library
                  .criteria
                  .iter()
                  .filter(|criterion| criterion.source != Some(CriterionSource::ProjectScan))
                  .cloned()
                  .collect()
          })
          .unwrap_or_default();
      let theirs: Vec<&str> = kept.iter().map(|one| one.name.as_str()).collect();
      let mut criteria = kept.clone();
      criteria.extend(
          found
              .iter()
              .filter(|one| !theirs.contains(&one.name.as_str()))
              .cloned(),
      );
      CriteriaLibrary { criteria }
  }
  ```

- [x] Then the tables every signal is read from:

  ```rust
  /// The extension a file carries and the language it counts for. The language of a project is the one
  /// with the most tracked files; a tie goes to whichever comes first here.
  const LANGUAGES: &[(&str, &str)] = &[
      ("ts", "TypeScript"),
      ("tsx", "TypeScript"),
      ("rs", "Rust"),
      ("py", "Python"),
      ("go", "Go"),
      ("js", "JavaScript"),
      ("jsx", "JavaScript"),
      ("mjs", "JavaScript"),
      ("cjs", "JavaScript"),
      ("java", "Java"),
      ("kt", "Kotlin"),
      ("swift", "Swift"),
      ("rb", "Ruby"),
      ("php", "PHP"),
      ("cs", "C#"),
  ];

  /// One criterion a toolchain always has.
  struct Verified {
      /// What a contract refers to it by. Kebab-case, because the schema's pattern says so.
      name: &'static str,
      /// What it says, in the words it carries into every contract that uses it.
      text: &'static str,
      /// What is run, from the project root.
      command: &'static str,
      /// Whether it is a test command, which the reviewer may be asked to check adds a test.
      is_test: bool,
  }

  /// Where a toolchain's commands come from.
  enum Commands {
      /// Whatever this project's own `package.json` calls its scripts, run through this manager. The
      /// project's commands are the project's to name; the scan reads them rather than guessing.
      Scripts,
      /// The commands this toolchain always has.
      Fixed(&'static [Verified]),
  }

  /// A toolchain: the file that says a project uses it, what a person calls it, the language whose
  /// project it is, and where its commands come from.
  ///
  /// One toolchain is chosen, not all of them, because a criterion called `the-tests-pass` can only
  /// mean one command. A project with two markers — a Rust workspace with a front end, which is the
  /// shape this repository itself takes — gets the one whose language the tree says it mostly is, and
  /// falls back to the first marker found when the language names none of them.
  struct Toolchain {
      marker: &'static str,
      name: &'static str,
      language: &'static str,
      commands: Commands,
  }

  const CARGO: &[Verified] = &[
      Verified {
          name: "the-tests-pass",
          text: "Every test in the workspace passes: cargo test --workspace.",
          command: "cargo test --workspace",
          is_test: true,
      },
      Verified {
          name: "the-build-succeeds",
          text: "The workspace builds: cargo build --workspace.",
          command: "cargo build --workspace",
          is_test: false,
      },
      Verified {
          name: "clippy-is-clean",
          text: "Clippy has nothing to say about the workspace, warnings included.",
          command: "cargo clippy --workspace --all-targets -- -D warnings",
          is_test: false,
      },
      Verified {
          name: "formatting-is-clean",
          text: "Every file is formatted as rustfmt would format it.",
          command: "cargo fmt --all -- --check",
          is_test: false,
      },
  ];

  const GO: &[Verified] = &[
      Verified {
          name: "the-tests-pass",
          text: "Every test in the module passes: go test ./...",
          command: "go test ./...",
          is_test: true,
      },
      Verified {
          name: "the-build-succeeds",
          text: "The module builds: go build ./...",
          command: "go build ./...",
          is_test: false,
      },
      Verified {
          name: "vet-is-clean",
          text: "go vet has nothing to say about the module.",
          command: "go vet ./...",
          is_test: false,
      },
  ];

  const PYTEST: &[Verified] = &[Verified {
      name: "the-tests-pass",
      text: "Every test passes: pytest.",
      command: "pytest",
      is_test: true,
  }];

  const RSPEC: &[Verified] = &[Verified {
      name: "the-tests-pass",
      text: "Every specification passes: bundle exec rspec.",
      command: "bundle exec rspec",
      is_test: true,
  }];

  const TOOLCHAINS: &[Toolchain] = &[
      Toolchain {
          marker: "pnpm-lock.yaml",
          name: "pnpm",
          language: "TypeScript",
          commands: Commands::Scripts,
      },
      Toolchain {
          marker: "bun.lockb",
          name: "bun",
          language: "TypeScript",
          commands: Commands::Scripts,
      },
      Toolchain {
          marker: "yarn.lock",
          name: "yarn",
          language: "TypeScript",
          commands: Commands::Scripts,
      },
      Toolchain {
          marker: "package-lock.json",
          name: "npm",
          language: "TypeScript",
          commands: Commands::Scripts,
      },
      Toolchain {
          marker: "Cargo.lock",
          name: "cargo",
          language: "Rust",
          commands: Commands::Fixed(CARGO),
      },
      Toolchain {
          marker: "go.sum",
          name: "go",
          language: "Go",
          commands: Commands::Fixed(GO),
      },
      Toolchain {
          marker: "poetry.lock",
          name: "poetry",
          language: "Python",
          commands: Commands::Fixed(PYTEST),
      },
      Toolchain {
          marker: "uv.lock",
          name: "uv",
          language: "Python",
          commands: Commands::Fixed(PYTEST),
      },
      Toolchain {
          marker: "Gemfile.lock",
          name: "bundler",
          language: "Ruby",
          commands: Commands::Fixed(RSPEC),
      },
  ];

  /// The script names a criterion is made from, and what each one is called in the library. A project
  /// may call a script anything; these are the ones whose meaning is the same everywhere, and keeping
  /// the list closed is also what keeps a criterion's name to the schema's pattern.
  const SCRIPTS: &[(&str, &str, &str, bool)] = &[
      (
          "test",
          "the-tests-pass",
          "Every test passes: the project's own test script.",
          true,
      ),
      (
          "build",
          "the-build-succeeds",
          "The project builds: its own build script.",
          false,
      ),
      (
          "lint",
          "the-linter-is-clean",
          "The linter has nothing to say: the project's own lint script.",
          false,
      ),
      (
          "typecheck",
          "the-types-check",
          "The types check: the project's own typecheck script.",
          false,
      ),
      (
          "check",
          "the-project-check-passes",
          "The project's own check script passes.",
          false,
      ),
  ];

  /// What says a project tests with a given runner: a file whose name begins this way, or this exact
  /// name among a `package.json` dependency.
  const TEST_RUNNERS: &[(&str, &str)] = &[
      ("vitest", "vitest"),
      ("jest", "jest"),
      ("playwright", "Playwright"),
      ("cypress", "Cypress"),
      ("pytest", "pytest"),
      ("mocha", "mocha"),
  ];

  /// The files that say a project is one workspace of several packages.
  const WORKSPACE_MARKERS: &[&str] = &[
      "pnpm-workspace.yaml",
      "turbo.json",
      "nx.json",
      "lerna.json",
      "go.work",
  ];

  /// The manifests a package has one of, for counting packages in a workspace.
  const MANIFESTS: &[&str] = &[
      "package.json",
      "Cargo.toml",
      "pyproject.toml",
      "go.mod",
      "composer.json",
  ];
  ```

- [x] Then what one tree says about itself:

  ```rust
  /// What one tree says about itself, before any of it is put into words.
  struct Reading {
      language: Option<&'static str>,
      toolchain: Option<&'static Toolchain>,
      is_workspace: bool,
      packages: usize,
      tests: Option<&'static str>,
      scripts: Vec<String>,
  }

  impl Reading {
      /// Reads the tracked tree, and the one manifest whose contents matter.
      fn of(root: &Path, tracked: &[String]) -> Result<Self, ScanError> {
          let manifest = read_package_json(root, tracked)?;
          let language = language_of(tracked);
          let present = |toolchain: &&Toolchain| tracked.iter().any(|path| path == toolchain.marker);
          let toolchain = TOOLCHAINS
              .iter()
              .find(|toolchain| present(toolchain) && Some(toolchain.language) == language)
              .or_else(|| TOOLCHAINS.iter().find(present));
          let is_workspace = tracked
              .iter()
              .any(|path| WORKSPACE_MARKERS.contains(&path.as_str()))
              || manifest
                  .as_ref()
                  .is_some_and(|value| value.get("workspaces").is_some())
              || cargo_workspace(root, tracked)?;
          Ok(Self {
              language,
              toolchain,
              is_workspace,
              packages: packages_in(tracked),
              tests: tests_in(tracked, manifest.as_ref()),
              scripts: scripts_in(manifest.as_ref()),
          })
      }

      /// The one line a person is shown, in the shape section 4 gives.
      fn read_back(
          &self,
          tracked: &[String],
          commit: Option<&HeadSummary>,
          now: DateTime<Utc>,
      ) -> String {
          let mut parts: Vec<String> = Vec::new();
          if tracked.is_empty() {
              parts.push("nothing tracked yet".to_string());
          }
          // A workspace of one package is a workspace and not a monorepo, and "1 packages" is not a
          // count. One condition decides the word and the number together, so they cannot disagree.
          let monorepo = self.is_workspace && self.packages > 1;
          if let Some(language) = self.language {
              parts.push(if monorepo {
                  format!("{language} monorepo")
              } else {
                  language.to_string()
              });
          }
          if let Some(toolchain) = self.toolchain {
              parts.push(toolchain.name.to_string());
          }
          if monorepo {
              parts.push(format!("{} packages", self.packages));
          }
          if let Some(tests) = self.tests {
              parts.push(format!("tests in {tests}"));
          }
          parts.push(match commit {
              Some(head) => format!("last commit {}", how_long_ago(&head.committed_at, now)),
              None => "no commits yet".to_string(),
          });
          parts.join(", ")
      }

      /// The criteria this project's own commands make available.
      fn criteria(&self) -> Result<Vec<CriterionTemplate>, ScanError> {
          let Some(toolchain) = self.toolchain else {
              return Ok(Vec::new());
          };
          let wire: Vec<Value> = match &toolchain.commands {
              Commands::Fixed(fixed) => fixed
                  .iter()
                  .map(|one| criterion(one.name, one.text, one.command, one.is_test))
                  .collect(),
              Commands::Scripts => SCRIPTS
                  .iter()
                  .filter(|(script, ..)| self.scripts.iter().any(|have| have == script))
                  .map(|(script, name, text, is_test)| {
                      criterion(
                          name,
                          text,
                          &format!("{} run {script}", toolchain.name),
                          *is_test,
                      )
                  })
                  .collect(),
          };
          validate_criteria(&json!({ "criteria": wire }))
              .map(|library| library.criteria)
              .map_err(|errors| ScanError::Built {
                  detail: errors
                      .iter()
                      .map(|error| format!("{} {}", error.path, error.message))
                      .collect::<Vec<_>>()
                      .join("; "),
              })
      }
  }
  ```

- [x] And last the helpers, which is where the module ends and the tests module begins:

  ```rust
  /// One criterion as the library's schema sees it, for `validate_criteria` to hold to its rules.
  fn criterion(name: &str, text: &str, command: &str, is_test: bool) -> Value {
      let verification = if is_test {
          json!({ "method": "test", "command": command, "new_tests_required": true })
      } else {
          json!({ "method": "command", "command": command, "expect": { "exit_code": 0 } })
      };
      json!({
          "name": name,
          "text": text,
          "source": "project_scan",
          "verification": verification,
      })
  }

  /// The language with the most tracked files, or nothing when the tree holds no code the table knows.
  fn language_of(tracked: &[String]) -> Option<&'static str> {
      let mut counted: BTreeMap<&'static str, usize> = BTreeMap::new();
      for path in tracked {
          let Some(extension) = path.rsplit_once('.').map(|(_, tail)| tail) else {
              continue;
          };
          if let Some((_, language)) = LANGUAGES
              .iter()
              .find(|(known, _)| known.eq_ignore_ascii_case(extension))
          {
              *counted.entry(language).or_default() += 1;
          }
      }
      LANGUAGES
          .iter()
          .filter_map(|(_, language)| counted.get(language).map(|count| (*count, *language)))
          // `max_by_key` keeps the last of equal keys, so the table is read backwards: a tie goes to
          // whichever language comes first in it, which is what its own comment promises.
          .rev()
          .max_by_key(|(count, _)| *count)
          .map(|(_, language)| language)
  }

  /// How many packages a tree holds: one per manifest below the root.
  fn packages_in(tracked: &[String]) -> usize {
      tracked
          .iter()
          .filter(|path| {
              path.contains('/')
                  && path
                      .rsplit_once('/')
                      .is_some_and(|(_, name)| MANIFESTS.contains(&name))
          })
          .count()
  }

  /// The test runner a tree names, by a configuration file of its own or by a dependency.
  fn tests_in(tracked: &[String], manifest: Option<&Value>) -> Option<&'static str> {
      // The runner's own file, not a file that mentions it: `vitest.config.ts` says the project tests
      // with vitest, and `jest-to-vitest-migration.md` says somebody wrote about it.
      let named = |needle: &str| {
          tracked.iter().any(|path| {
              let name = path
                  .rsplit_once('/')
                  .map_or(path.as_str(), |(_, name)| name);
              name == needle || name.starts_with(&format!("{needle}."))
          })
      };
      let depended = |needle: &str| {
          ["dependencies", "devDependencies"].iter().any(|section| {
              manifest
                  .and_then(|value| value.get(section))
                  .and_then(Value::as_object)
                  .is_some_and(|section| section.contains_key(needle))
          })
      };
      TEST_RUNNERS
          .iter()
          .find(|(needle, _)| named(needle) || depended(needle))
          .map(|(_, shown)| *shown)
  }

  /// The scripts a `package.json` defines that actually run something.
  ///
  /// A name with nothing behind it is not a command: `"test": ""` would become a criterion whose
  /// command exits 0 having verified nothing, which is worse than no criterion at all. What this
  /// cannot see is a stub that runs and fails on purpose, as `npm init` writes for `test`; that is a
  /// project telling its own tools something, and the person who adopts Farik has to look at it.
  fn scripts_in(manifest: Option<&Value>) -> Vec<String> {
      manifest
          .and_then(|value| value.get("scripts"))
          .and_then(Value::as_object)
          .map(|scripts| {
              scripts
                  .iter()
                  .filter(|(_, command)| command.as_str().is_some_and(|text| !text.trim().is_empty()))
                  .map(|(name, _)| name.clone())
                  .collect()
          })
          .unwrap_or_default()
  }

  /// The root `package.json`, when the tree tracks one, as an untrusted value.
  fn read_package_json(root: &Path, tracked: &[String]) -> Result<Option<Value>, ScanError> {
      if !tracked.iter().any(|path| path == "package.json") {
          return Ok(None);
      }
      let text = read_tracked(root, "package.json")?;
      // A manifest that is not JSON is a fact about the project rather than a reason to refuse the
      // whole scan: the read-back says what it can and the criteria come out empty.
      Ok(serde_json::from_str(&text).ok())
  }

  /// Whether the root `Cargo.toml` declares a workspace.
  fn cargo_workspace(root: &Path, tracked: &[String]) -> Result<bool, ScanError> {
      if !tracked.iter().any(|path| path == "Cargo.toml") {
          return Ok(false);
      }
      Ok(read_tracked(root, "Cargo.toml")?
          .lines()
          .any(|line| line.trim() == "[workspace]"))
  }

  /// Whether two paths name one directory, once the links in each are followed.
  ///
  /// A path that cannot be resolved is not the same directory as one that can: the question is asked
  /// of a repository git has just answered about, so a failure here is the directory going away
  /// underneath, and the safe answer is no.
  fn same_directory(one: &Path, other: &Path) -> bool {
      match (std::fs::canonicalize(one), std::fs::canonicalize(other)) {
          (Ok(one), Ok(other)) => one == other,
          _ => false,
      }
  }

  /// One tracked file's text. A file git tracks that cannot be read is a tree half-written.
  fn read_tracked(root: &Path, relative: &str) -> Result<String, ScanError> {
      std::fs::read_to_string(root.join(relative)).map_err(|error| ScanError::Io {
          path: relative.to_string(),
          detail: error.to_string(),
      })
  }

  /// How long ago a commit was, in the words a person would use.
  ///
  /// Git's `%cI` is strict ISO 8601. A stamp this cannot parse is reported as the stamp itself rather
  /// than as a length of time nobody can check.
  fn how_long_ago(committed_at: &str, now: DateTime<Utc>) -> String {
      let Ok(committed) = DateTime::parse_from_rfc3339(committed_at) else {
          return format!("at {committed_at}");
      };
      let committed = committed.with_timezone(&Utc);
      // Ahead of now at all, not a whole day ahead: two machines whose clocks disagree are hours
      // apart, and "today" for a commit that has not happened yet is a length of time nobody can
      // check.
      if committed > now {
          return format!("at {committed_at}");
      }
      let days = (now - committed).num_days();
      match days {
          0 => "today".to_string(),
          1 => "yesterday".to_string(),
          2..=29 => format!("{days} days ago"),
          30..=59 => "a month ago".to_string(),
          _ => format!("{} months ago", days / 30),
      }
  }
  ```

- [x] Export it from `crates/store/src/lib.rs`, after the `projections` re-export:

  ```rust
  pub use scan::{ProjectScan, ScanError, scan_project, seeded_library};
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 65 passed (farik-store)
  #   test result: ok. 13 passed (crates/store/tests/project_scan.rs)
  ```

- [x] Commit: `feat(store): read a repository and say what it is`

### Task 4: Where the files and the log disagree

Files: created `crates/store/src/reconcile.rs`, `crates/store/tests/reconciliation.rs`; modified `crates/store/src/lib.rs`, `docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md`

Consumes: everything above
Produces: `farik_store::{Drift, ReconcileError, reconcile}`

- [x] Write the failing tests. Create `crates/store/tests/reconciliation.rs`:

  ```rust
  //! The files against the log, in a real project.
  //!
  //! These need somewhere to write and nothing else, so they run in the default `cargo xtask check`.

  use std::sync::Arc;

  use chrono::{TimeZone, Utc};
  use farik_core::contract::{TaskContract, TaskId, TaskStatus, validate_contract};
  use farik_protocol::event::fixtures::{a_contract_summary_wire, an_event_wire};
  use farik_protocol::event::{EventKind, NewEvent, event_from_value};
  use farik_store::files::ProjectFiles;
  use farik_store::files::fixtures::TempProject;
  use farik_store::{
      Drift, EventLog, IN_MEMORY, Projections, ReconcileError, open_event_log, open_projections,
  };
  use serde_json::json;

  /// A log and the projections of it, both empty.
  fn a_board() -> (Arc<EventLog>, Projections) {
      let at = Utc
          .with_ymd_and_hms(2026, 9, 17, 9, 0, 0)
          .single()
          .expect("a real hour");
      let log = Arc::new(
          open_event_log(std::path::Path::new(IN_MEMORY), at).expect("a log in memory opens"),
      );
      let projections = open_projections(Arc::clone(&log)).expect("the projections open");
      (log, projections)
  }

  /// Puts a task on the board at a status, the way a governed transition does.
  fn on_the_board(
      log: &EventLog,
      projections: &Projections,
      task_id: &str,
      status: &str,
      locked: bool,
  ) {
      let mut summary = a_contract_summary_wire();
      summary["status"] = json!(status);
      let mut wire = an_event_wire(EventKind::ContractWritten);
      wire["task_id"] = json!(task_id);
      wire["body"]["summary"] = summary;
      let event = event_from_value(&wire).expect("the fixture is schema-valid");
      let written = NewEvent {
          recorded_at: event.envelope.recorded_at,
          team_id: event.envelope.team_id,
          project_id: event.envelope.project_id,
          task_id: event.envelope.task_id,
          agent_id: event.envelope.agent_id,
          session_id: event.envelope.session_id,
          body: event.body,
      };
      projections
          .apply(&log.append(&written).expect("appends"))
          .expect("projects");
      if locked {
          let mut wire = an_event_wire(EventKind::ContractLocked);
          wire["task_id"] = json!(task_id);
          let event = event_from_value(&wire).expect("the fixture is schema-valid");
          let held = NewEvent {
              recorded_at: event.envelope.recorded_at,
              team_id: event.envelope.team_id,
              project_id: event.envelope.project_id,
              task_id: event.envelope.task_id,
              agent_id: event.envelope.agent_id,
              session_id: event.envelope.session_id,
              body: event.body,
          };
          projections
              .apply(&log.append(&held).expect("appends"))
              .expect("projects");
      }
  }

  /// A contract of that id, at that status, held or not.
  fn a_contract(id: &str, status: TaskStatus, locked: bool) -> TaskContract {
      let mut wire = farik_core::contract::fixtures::a_contract_wire();
      wire["id"] = json!(id);
      wire["status"] = json!(status.to_string());
      wire["locked"] = json!(locked);
      validate_contract(&wire).expect("the fixture is a contract")
  }

  fn an_id(id: &str) -> TaskId {
      id.parse().expect("a task id")
  }

  fn drifts(files: &ProjectFiles, projections: &Projections) -> Vec<Drift> {
      farik_store::reconcile(files, projections).expect("it reconciles")
  }

  #[test]
  fn a_project_whose_files_and_log_agree_has_nothing_to_report() {
      let project = TempProject::new("reconcile-agree");
      let files = project.files();
      let (log, projections) = a_board();
      for id in ["FRK-1", "FRK-2"] {
          on_the_board(&log, &projections, id, "ready", false);
          files
              .write_contract(&a_contract(id, TaskStatus::Ready, false))
              .expect("written");
      }
      assert_eq!(drifts(&files, &projections), []);
  }

  #[test]
  fn says_when_there_is_a_contract_the_log_has_never_heard_of() {
      let project = TempProject::new("reconcile-orphan-file");
      let files = project.files();
      let (_log, projections) = a_board();
      files
          .write_contract(&a_contract("FRK-7", TaskStatus::Draft, false))
          .expect("written");

      let found = drifts(&files, &projections);
      assert_eq!(found.len(), 1, "{found:?}");
      let Drift::ContractWithoutEvents { task_id, detail } = &found[0] else {
          panic!("a file nothing created: {found:?}");
      };
      assert_eq!(task_id, &an_id("FRK-7"));
      assert!(detail.contains("never heard of this task"), "{detail}");
      // What a person is shown, which is `Drift`'s own two accessors and its `Display`.
      assert_eq!(found[0].task_id(), &an_id("FRK-7"));
      assert_eq!(found[0].detail(), detail);
      assert_eq!(
          found[0].to_string(),
          "FRK-7: there is a contract file and the log has never heard of this task, so no transition \
           of it was ever governed"
      );
  }

  #[test]
  fn says_when_the_log_knows_a_task_with_no_contract_to_work_from() {
      let project = TempProject::new("reconcile-orphan-log");
      let files = project.files();
      let (log, projections) = a_board();
      on_the_board(&log, &projections, "FRK-3", "assigned", false);

      let found = drifts(&files, &projections);
      assert_eq!(found.len(), 1, "{found:?}");
      let Drift::EventsWithoutContract { task_id, detail } = &found[0] else {
          panic!("a task with no contract: {found:?}");
      };
      assert_eq!(task_id, &an_id("FRK-3"));
      assert!(detail.contains("assigned"), "{detail}");
      assert!(detail.contains("no contract file"), "{detail}");
  }

  #[test]
  fn says_when_the_file_and_the_log_disagree_about_where_a_task_is() {
      let project = TempProject::new("reconcile-status");
      let files = project.files();
      let (log, projections) = a_board();
      on_the_board(&log, &projections, "FRK-1", "accepted", false);
      files
          .write_contract(&a_contract("FRK-1", TaskStatus::InProgress, false))
          .expect("written");

      let found = drifts(&files, &projections);
      assert_eq!(found.len(), 1, "{found:?}");
      let Drift::StatusMismatch { task_id, detail } = &found[0] else {
          panic!("two answers about one task: {found:?}");
      };
      assert_eq!(task_id, &an_id("FRK-1"));
      assert_eq!(
          detail, "the log has it at accepted and the file says in_progress",
          "the log's answer first, because transitions are what the log records"
      );
  }

  #[test]
  fn says_when_the_file_and_the_log_disagree_about_who_holds_a_contract() {
      // 5.11: the human may hold a contract, and `contract.locked` is the event that says so. A file
      // that says otherwise would let work start on a contract nobody may touch.
      let project = TempProject::new("reconcile-lock");
      let files = project.files();
      let (log, projections) = a_board();
      on_the_board(&log, &projections, "FRK-1", "ready", true);
      files
          .write_contract(&a_contract("FRK-1", TaskStatus::Ready, false))
          .expect("written");

      let found = drifts(&files, &projections);
      assert_eq!(found.len(), 1, "{found:?}");
      let Drift::LockMismatch { task_id, detail } = &found[0] else {
          panic!("two answers about who holds it: {found:?}");
      };
      assert_eq!(task_id, &an_id("FRK-1"));
      assert_eq!(
          detail,
          "the log has it held by the human and the file says not held"
      );
  }

  #[test]
  fn reports_a_contract_it_cannot_read_rather_than_stopping_at_it() {
      // One file a person broke must not hide every other disagreement, which is the whole use of
      // this: a person runs it to find out what is wrong, not to be told one thing at a time.
      let project = TempProject::new("reconcile-broken");
      let files = project.files();
      let (log, projections) = a_board();
      on_the_board(&log, &projections, "FRK-1", "ready", false);
      on_the_board(&log, &projections, "FRK-2", "accepted", false);
      files
          .write_contract(&a_contract("FRK-1", TaskStatus::Ready, false))
          .expect("written");
      files
          .write_contract(&a_contract("FRK-2", TaskStatus::Ready, false))
          .expect("written");
      std::fs::write(
          project.root.join(".farik/contracts/FRK-1.yaml"),
          "id: FRK-1\ntitle: half a contract\n",
      )
      .expect("a person edits one");

      let found = drifts(&files, &projections);
      assert_eq!(found.len(), 2, "{found:?}");
      let Drift::ContractUnreadable { task_id, detail } = &found[0] else {
          panic!("the broken one, and then the other: {found:?}");
      };
      assert_eq!(task_id, &an_id("FRK-1"));
      assert!(detail.contains(".farik/contracts/FRK-1.yaml"), "{detail}");
      assert!(
          matches!(&found[1], Drift::StatusMismatch { task_id, .. } if task_id == &an_id("FRK-2")),
          "{found:?}"
      );
  }

  #[test]
  fn says_what_it_could_not_compare_and_why() {
      assert_eq!(
          [
              ReconcileError::Files {
                  detail: ".farik/contracts could not be used: Permission denied (os error 13)"
                      .to_string()
              }
              .to_string(),
              ReconcileError::Store {
                  detail: "sqlite refused: database is locked".to_string()
              }
              .to_string(),
          ],
          [
              "the files could not be read: .farik/contracts could not be used: Permission denied \
               (os error 13)",
              "the board could not be read: sqlite refused: database is locked",
          ]
      );
  }

  #[test]
  fn reports_everything_it_found_in_an_order_two_runs_agree_on() {
      let project = TempProject::new("reconcile-order");
      let files = project.files();
      let (log, projections) = a_board();
      for (id, status) in [
          ("FRK-10", "ready"),
          ("FRK-2", "accepted"),
          ("FRK-9", "ready"),
      ] {
          on_the_board(&log, &projections, id, status, false);
      }
      files
          .write_contract(&a_contract("FRK-2", TaskStatus::Draft, true))
          .expect("written");
      files
          .write_contract(&a_contract("FRK-10", TaskStatus::Ready, false))
          .expect("written");
      files
          .write_contract(&a_contract("FRK-11", TaskStatus::Draft, false))
          .expect("written");

      let found = drifts(&files, &projections);
      assert_eq!(
          found
              .iter()
              .map(|drift| (drift.task_id().as_str().to_string(), name_of(drift)))
              .collect::<Vec<_>>(),
          [
              ("FRK-2".to_string(), "StatusMismatch"),
              ("FRK-2".to_string(), "LockMismatch"),
              ("FRK-9".to_string(), "EventsWithoutContract"),
              ("FRK-11".to_string(), "ContractWithoutEvents"),
          ],
          "by the number in the id, so the tenth task does not come before the ninth, and then in one \
           fixed order per task"
      );
      assert_eq!(
          drifts(&files, &projections),
          found,
          "and a second run says the same thing"
      );
  }

  #[test]
  fn a_project_with_no_farik_directory_at_all_has_nothing_to_report() {
      let project = TempProject::new("reconcile-nothing");
      let (_log, projections) = a_board();
      assert_eq!(drifts(&project.files(), &projections), []);
  }

  fn name_of(drift: &Drift) -> &'static str {
      match drift {
          Drift::ContractWithoutEvents { .. } => "ContractWithoutEvents",
          Drift::EventsWithoutContract { .. } => "EventsWithoutContract",
          Drift::StatusMismatch { .. } => "StatusMismatch",
          Drift::LockMismatch { .. } => "LockMismatch",
          Drift::ContractUnreadable { .. } => "ContractUnreadable",
      }
  }
  ```

- [x] Run them and confirm they fail because nothing compares the two yet:

  ```
  cargo test -p farik-store --test reconciliation
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `farik_store::Drift`,
  #   `farik_store::ReconcileError`
  # error[E0425]: cannot find function `reconcile` in crate `farik_store`
  # error: could not compile `farik-store` (test "reconciliation") due to 2 previous
  #   errors
  ```

- [x] Write the minimal implementation. Create `crates/store/src/reconcile.rs` with the module doc and its imports:

  ```rust
  //! Where the files and the log disagree (`docs/SPEC.md` section 8.4).
  //!
  //! The log is the source of truth for what happened; the files under `.farik/` are the source of
  //! truth for what the team knows. Two sources of truth about one project can come apart — a process
  //! stopped between a write and an append, a person edited a contract by hand, a file was restored
  //! from a backup — and this says where, without picking a side. Nothing here writes anything: a
  //! person decides what to do about a disagreement, and `farik doctor` is where they are told of one.
  //!
  //! Only what the log is authoritative about is a disagreement. A contract's `status` and its `locked`
  //! flag are both moved by events (5.2, 5.11), so a file that says something else is a file to fix.
  //! Its title, kind, risk and parent are the contract's own, and the board's copy of them is a cache
  //! of the last event that mentioned the task — one that has fallen behind is a projection to rebuild,
  //! not a disagreement about the project.

  use std::fmt;

  use farik_core::contract::TaskId;

  use crate::error::StoreError;
  use crate::files::{FilesError, ProjectFiles};
  use crate::projections::Projections;
  ```

- [x] Then one disagreement:

  ```rust
  /// One disagreement between the files and the log.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum Drift {
      /// There is a contract file and the log has never heard of the task. Nothing created it, so no
      /// transition of it was ever governed.
      ContractWithoutEvents {
          /// Which task.
          task_id: TaskId,
          /// What is missing, in words a person can act on.
          detail: String,
      },
      /// The log knows the task and there is no contract file. Every session is assembled from the
      /// contract, so a task on the board with no contract cannot be worked on.
      EventsWithoutContract {
          /// Which task.
          task_id: TaskId,
          /// What is missing.
          detail: String,
      },
      /// The file and the log disagree about where the task is in the lifecycle of 5.2.
      StatusMismatch {
          /// Which task.
          task_id: TaskId,
          /// Both answers, the log's first.
          detail: String,
      },
      /// The file and the log disagree about whether the human is holding the contract (5.11).
      LockMismatch {
          /// Which task.
          task_id: TaskId,
          /// Both answers, the log's first.
          detail: String,
      },
      /// A contract file with that id was listed and could not then be read as a contract: broken by
      /// hand, unreadable to this user, or — if it went away between the listing and the read — no
      /// longer there at all. Which of those it was is in `detail`, in the file adapter's own words,
      /// because no test can win that race and a branch nothing can reach is worse than one variant
      /// whose detail tells the truth.
      ///
      /// Reported here rather than refused, so that one file a person broke does not hide every other
      /// disagreement.
      ContractUnreadable {
          /// Which task.
          task_id: TaskId,
          /// Why it could not be read, in the words the file adapter used.
          detail: String,
      },
  }

  impl Drift {
      /// Which task this is about.
      #[must_use]
      pub fn task_id(&self) -> &TaskId {
          match self {
              Self::ContractWithoutEvents { task_id, .. }
              | Self::EventsWithoutContract { task_id, .. }
              | Self::StatusMismatch { task_id, .. }
              | Self::LockMismatch { task_id, .. }
              | Self::ContractUnreadable { task_id, .. } => task_id,
          }
      }

      /// What is wrong, in words a person can act on.
      #[must_use]
      pub fn detail(&self) -> &str {
          match self {
              Self::ContractWithoutEvents { detail, .. }
              | Self::EventsWithoutContract { detail, .. }
              | Self::StatusMismatch { detail, .. }
              | Self::LockMismatch { detail, .. }
              | Self::ContractUnreadable { detail, .. } => detail,
          }
      }
  }

  impl fmt::Display for Drift {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(formatter, "{}: {}", self.task_id().as_str(), self.detail())
      }
  }
  ```

- [x] Then why the two could not be compared at all:

  ```rust
  /// Why the files and the log could not be compared at all.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ReconcileError {
      /// The files could not be listed.
      Files {
          /// What the file adapter said.
          detail: String,
      },
      /// The board could not be read.
      Store {
          /// What the store said.
          detail: String,
      },
  }

  impl fmt::Display for ReconcileError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::Files { detail } => write!(formatter, "the files could not be read: {detail}"),
              Self::Store { detail } => write!(formatter, "the board could not be read: {detail}"),
          }
      }
  }

  impl std::error::Error for ReconcileError {}

  impl From<FilesError> for ReconcileError {
      fn from(error: FilesError) -> Self {
          Self::Files {
              detail: error.to_string(),
          }
      }
  }

  impl From<StoreError> for ReconcileError {
      fn from(error: StoreError) -> Self {
          Self::Store {
              detail: error.to_string(),
          }
      }
  }
  ```

- [x] Then the comparison:

  ```rust
  /// Every disagreement between the contracts on disk and what the log says of them.
  ///
  /// The answer is ordered by the number in the task id, so that the tenth task does not come before
  /// the ninth, and the sort is stable, so two drifts about one task keep the order they were found in:
  /// a status before a lock. Two runs over one project therefore print the same thing and a person can
  /// diff them.
  ///
  /// What the log says is read from the projections rather than from the log itself, so this leans on
  /// the handle being caught up. `open_projections` catches up when it opens, and a command that opens
  /// them per run therefore always is; a handle held across appends without `apply` would report its
  /// own lag as a `StatusMismatch`.
  ///
  /// # Errors
  ///
  /// `Files` when the contracts cannot be listed, `Store` when the board cannot be read. A single
  /// contract that cannot be read is a `Drift`, not an error: one file a person broke does not hide
  /// every other disagreement.
  pub fn reconcile(
      files: &ProjectFiles,
      projections: &Projections,
  ) -> Result<Vec<Drift>, ReconcileError> {
      let on_disk = files.list_contracts()?;
      let board = projections.board()?;
      let mut found: Vec<Drift> = Vec::new();

      for task_id in &on_disk {
          if board.iter().any(|row| row.task_id == *task_id) {
              continue;
          }
          found.push(Drift::ContractWithoutEvents {
              task_id: task_id.clone(),
              detail: "there is a contract file and the log has never heard of this task, so no \
                       transition of it was ever governed"
                  .to_string(),
          });
      }

      for row in &board {
          if on_disk.contains(&row.task_id) {
              continue;
          }
          found.push(Drift::EventsWithoutContract {
              task_id: row.task_id.clone(),
              detail: format!(
                  "the log has this task at {} and there is no contract file for it, so no session \
                   can be assembled for it",
                  row.status
              ),
          });
      }

      for row in &board {
          if !on_disk.contains(&row.task_id) {
              continue;
          }
          let contract = match files.read_contract(&row.task_id) {
              Ok(contract) => contract,
              Err(error) => {
                  found.push(Drift::ContractUnreadable {
                      task_id: row.task_id.clone(),
                      detail: error.to_string(),
                  });
                  continue;
              }
          };
          if contract.status != row.status {
              found.push(Drift::StatusMismatch {
                  task_id: row.task_id.clone(),
                  detail: format!(
                      "the log has it at {} and the file says {}",
                      row.status, contract.status
                  ),
              });
          }
          if contract.locked != row.locked {
              found.push(Drift::LockMismatch {
                  task_id: row.task_id.clone(),
                  detail: format!(
                      "the log has it {} and the file says {}",
                      held(row.locked),
                      held(contract.locked)
                  ),
              });
          }
      }

      // Stable, so what the loops above found about one task stays in the order they found it. There
      // is no second ranking to disagree with that one.
      found.sort_by_key(|drift| number_in(drift.task_id()));
      Ok(found)
  }
  ```

- [x] And last its two helpers:

  ```rust
  /// Whether the human is holding the contract, in words rather than in a boolean.
  fn held(locked: bool) -> &'static str {
      if locked {
          "held by the human"
      } else {
          "not held"
      }
  }

  /// The number in a task id, for ordering, so that the tenth task does not come before the ninth.
  ///
  /// The parse cannot fail: a `TaskId` is `FRK-` and one to six digits, which is what let it be built.
  fn number_in(task_id: &TaskId) -> u64 {
      task_id
          .as_str()
          .trim_start_matches("FRK-")
          .parse()
          .unwrap_or_default()
  }
  ```

- [x] Declare it in `crates/store/src/lib.rs`, before `scan`, and export it before `scan`'s own line:

  ```rust
  /// Where the files and the log disagree.
  pub mod reconcile;
  ```

  ```rust
  pub use reconcile::{Drift, ReconcileError, reconcile};
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 9 passed (crates/store/tests/reconciliation.rs)
  ```

- [x] Commit: `feat(store): say where the files and the log disagree`

### Task 5: The plans say what the step became

Files: modified `docs/plans/project-plan.md`, `docs/plans/phase-2-protocol-store-cli/step-07-scan-and-reconciliation.md`

Consumes: everything above
Produces: a project plan that describes the scan and the reconciliation as they are

This task changes documentation and has no test cycle. The `> ` marker on the block below is this plan's and is not part of the text to write.

- [x] In `docs/plans/project-plan.md`, replace the whole of the one phase 2 line that begins `- Step 07 (` — the whole line, its closing full stop included — with:

  > - Step 07 (`farik-store::scan` and `farik-store::reconcile`): `struct ProjectScan { read_back: String, detected_criteria: Vec<CriterionTemplate> }`; `fn scan_project(git: &Git, now: DateTime<Utc>) -> Result<ProjectScan, ScanError>` (the adapter rather than a root beside it, so the tree the paths are listed from is the tree the manifests are read from, and a clock rather than a sampled one, because the read-back says how long ago the last commit was; both changed 2026-09-18 by the step 07 plan); `enum ScanError { NotARepository { path }, NotTheRepositoryRoot { path, root }, Git { detail }, Io { path, detail }, Built { detail } }`; `fn seeded_library(found: &[CriterionTemplate], existing: Option<&CriteriaLibrary>) -> CriteriaLibrary` (what a previous scan found is replaced and what a person wrote is kept, which is what `criteria.schema.json` says of its `source` field; a name a person has used is left alone; the result may exceed the schema's ceiling and `write_criteria` is what refuses it). Every signal is a tracked path or a line in a manifest: the tree is read through `Git::tracked_paths`, so `.gitignore` decides what is not content. A Node project's commands are read from its own `package.json` scripts; cargo, go, poetry, uv and bundler have the commands they always have. Each criterion is built as a wire value and held to `validate_criteria` before it leaves the module. `enum Drift { ContractWithoutEvents, EventsWithoutContract, StatusMismatch, LockMismatch, ContractUnreadable }`, each with `task_id` and `detail`, with `Drift::{task_id, detail}` and `Display`; `fn reconcile(files: &ProjectFiles, projections: &Projections) -> Result<Vec<Drift>, ReconcileError>` (ordered by the number in the task id, with a stable sort, so two drifts about one task keep the order they were found in); `enum ReconcileError { Files { detail }, Store { detail } }`. Added 2026-09-18 by the step 07 plan: `reconcile` takes the projections rather than the log, because what the log says the state is has one definition already and a second could disagree with it; `LockMismatch`, because 5.11 makes the human's hold a governance fact written in both places; and `ContractUnreadable`, because one file somebody broke must not hide every other disagreement. Only `status` and `locked` are disagreements: the file is the source of truth for a contract's title, kind, risk and parent (8.4), and the board's copy of those is a cache that `rebuild` fixes. Also added: `Git::tracked_paths` and `Git::root`, `git::fixtures::{TempRepo, git_in, git_output_in}` (moved out of `tests/git.rs` so the scan's and step 08's tests can use it), and `changed_paths_of` renamed `paths_of`, and `Git::top_level` so that a caller meaning the project rather than a subtree can ask. One toolchain is chosen and it is the one the tree's language names, a language tie goes to whichever comes first in the table, a workspace of one package is not a monorepo, a commit ahead of now is reported as its stamp, a script with nothing behind it is not a command, a test runner is named by its own file rather than by one that mentions it, a directory inside a repository is refused rather than scanned as a project, and cargo and go gained the build criterion F2 asks for by name — all eight found by the step 07 readiness review, which measured each against the code.

- [x] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [x] Commit: `docs(docs): record what step 07 changed about the scan`

## Verification

- [x] The whole check, from the workspace root:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 264 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 65 passed (farik-store)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 18 passed (crates/store/tests/git.rs)
  #   test result: ok. 31 passed (crates/store/tests/project_files.rs)
  #   test result: ok. 13 passed (crates/store/tests/project_scan.rs)
  #   test result: ok. 9 passed (crates/store/tests/reconciliation.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [x] The tests that need a program are still ignored without the flag, so they cannot pass silently:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 0 passed; 0 failed; 18 ignored (crates/store/tests/git.rs)
  #   test result: ok. 0 passed; 0 failed; 13 ignored
  #     (crates/store/tests/project_scan.rs)
  ```

- [x] `farik-core` still performs no I/O, which this step does not touch:

  ```
  cargo xtask core-io
  # expected: silent
  ```

- [x] No dependency was added:

  ```
  git diff --stat 3004cfe -- Cargo.toml Cargo.lock
  # expected: no output. `3004cfe` is step 06's last code commit, which the header
  #   names; the last change to either file was its serde-saphyr. Against `main` both
  #   files differ by the whole phase, which is not the question.
  ```

- [x] Every commit subject is accepted:

  ```
  for subject in \
    "refactor(store): make the repository fixture public" \
    "feat(store): list what the repository tracks" \
    "feat(store): read a repository and say what it is" \
    "feat(store): say where the files and the log disagree" \
    "docs(docs): record what step 07 changed about the scan"; do
    printf '%s\n' "$subject" > target/commit-subject
    cargo xtask commit-msg target/commit-subject
  done
  # expected: silent, five times. target/ is where cargo writes and git ignores it.
  ```

## Open questions

none
