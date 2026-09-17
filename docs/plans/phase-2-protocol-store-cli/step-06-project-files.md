# Phase 2, step 06: Project files

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 3 (a project is a git repository plus `.farik/`), 5.6 (protected paths and where a product document may be written), 5.8 (the three memories), 5.12 and 5.13 (the team file and the criterion library), 5.14 (a task's worktree, which lives under `.farik/local/`), 8.4 (the files are the source of truth for what the team knows); F2
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01 to 05 of this phase (last commit `020d269`), and ADR 0007 for the YAML parser

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, nothing ambiguous, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: <pending>

## Goal

`.farik/` is a directory a person can read, diff and edit, and this step is the code that reads and writes it. `ProjectFiles` makes the layout a project starts with, and carries the team, the criterion library, every contract, an agent's notebook, the project scan, the product documents, the price override and this machine's own settings, each to and from the file the spec puts it in.

Two promises hold it together. **A file read back is held to exactly the rules the same thing is held to arriving on the wire** — `validate_team`, `validate_criteria`, `validate_contract`, `validate_price_table`, the same functions, the same words in the refusal. And **what a writer accepts, the matching reader returns**: every writer runs the same validator before it writes, so a value built in memory cannot become a file nothing can open.

## Decisions

- **YAML is read and written by `yaml_serde`, and only as `serde_json::Value`.** ADR 0007 records why that crate: it is the only maintained one of the three, it lives under the YAML organisation, and it is dual-licensed to match this repository. Nothing here derives serde for a YAML shape — the text becomes a `Value`, the `Value` goes to the validator that owns that file, and only then is it typed. Replacing the crate would be a change to two functions.
- **A write goes to a file beside the one being written and is renamed over it.** A rename within a directory is the one file operation that is all or nothing. A crash in the middle of `team.yaml` would otherwise leave the team unreadable, and the team is what every session is assembled from.
- **`init` never takes anything away.** It makes the directories whether or not they hold anything yet, so that a person opening `.farik/` sees where things go, and it writes the team only when there is no team. A second `farik init` on a project that has one is a command with nothing to do.
- **`.farik/local/.gitignore` holds `*`.** A task's worktree lives at `.farik/local/worktrees/FRK-<n>` (5.14) and the event log at `.farik/local/farik.db` (8.4). Without it every repository Farik touches is dirty for good and `Git::is_clean` on the root never answers true again — which the step 04 landing review found and recorded on this step's row of the project plan.
- **A product document's path is checked by `farik-core`'s own path rule**, which this step makes public. The path comes from a tool call, so it is a string an agent chose; 5.6 puts product documents under `product/` and nowhere else. Two answers to "does this path climb out" would be two definitions of a safe path, and the one already written is the one the governor uses.
- **A contract lives in the file its own id names, and a file that disagrees is refused.** The file name and the id inside it are two claims about the same thing; a board that believed the file name would show a task that does not exist.
- **An agent that has never written a notebook has an empty one, not a missing file.** Every session for an agent includes its notebook (5.8), and "there is no file" is not something to tell an agent about.
- **A missing price override and a machine that was never asked about its sandbox are answers, not refusals.** `read_prices` gives `None` and the shipped table stands; `read_settings` gives the defaults. Both are ordinary states of a new project.
- **`LocalSettings` is JSON under `.farik/local/`, not YAML beside the team.** Nobody hand-edits it, it is never committed, and it is a fact about one laptop rather than about the project.
- **A file under `contracts/` that is not a contract's is not an error.** The directory is a person's to keep notes in; `list_contracts` passes over what it does not recognise, and orders what it does by the number in the id, so that the tenth task does not come before the ninth.
- **The tests live in `crates/store/tests/project_files.rs` and run in the default check.** They need somewhere to write and nothing else, which is exactly what `event_log_file.rs` established in step 02; `--integration` is for what needs a program.
- **`TempProject` is a public fixture**, not a test-only helper, because step 07's scan and step 08's command line both need a project to work in, and `docs/standards/code.md` says a crate's fixtures are public so another crate's tests can use them.
- `farik-core` exports `AgentId`, which it had generated but not named. Task 5 records both that and the path rule becoming public.

## Design

`crates/store/src/files.rs` holds `FilesError`, `ProjectFiles`, `LocalSettings` and `Sandbox`, the layout as a constant per file, and four private helpers that every reader and writer goes through: `read_text`, `write_text`, `read_yaml`, `write_yaml`. `crates/store/src/files/fixtures.rs` holds `TempProject` and `a_team`.

The layout, as `init` makes it:

```
.farik/
  team.yaml              the team, its policy and its rules (5.12)
  team/criteria.yaml     the criterion library (5.13)
  contracts/FRK-<n>.yaml one contract per task
  agents/<id>/memory.md  an agent's own notebook (5.8)
  decisions/             architecture and product decisions (5.8), written from phase 4
  product/               product documents, the only place one may be written (5.6)
  project.md             what the project scan read back (5.8)
  prices.json            this project's price override, when it has one (5.5)
  local/                 this machine's, never committed (8.4)
    .gitignore           `*`
    settings.json        where the sandbox runs
```

Out of scope: the project scan that writes `project.md` and the reconciliation of these files against the log, which are step 07; every command that calls any of this, which is step 08; `decisions/` and `team/retro.md`, which nothing writes until phase 4.

## Architecture notes

- Modified: `farik-store` gains `files`, a sibling of `event_log`, `projections` and `git`. It reads no database and runs no program.
- Consumed: `farik-core`'s four validators, its `TaskId` and `AgentId`, and its path rule. `serde`, `serde_json` and `yaml_serde`.
- New dependency: `yaml_serde`, pinned exactly as every other one is. `Cargo.toml` and `Cargo.lock` both change, which is the first time in this phase.
- `farik-core` does no I/O and is not touched except to export a type and make one function public; `cargo xtask core-io` still passes.

## Global constraints

- Every structured file goes through its own validator on the way in and on the way out.
- No `unwrap` or `expect` outside tests and fixtures.
- Every refusal names the path from the project root, as `.farik/team.yaml`, whatever the machine's separator is.
- No test is skipped, ignored, or quarantined to get green.

## File map

```
crates/store/src/files.rs                     creates: FilesError, ProjectFiles, LocalSettings, Sandbox
crates/store/src/files/fixtures.rs            creates: TempProject, a_team
crates/store/tests/project_files.rs           creates: the files against a real directory
crates/store/src/lib.rs                       modifies: declares files
crates/store/Cargo.toml                       modifies: serde and yaml_serde
Cargo.toml                                    modifies: yaml_serde in the workspace dependencies
Cargo.lock                                    modifies: written by cargo
crates/core/src/team.rs                       modifies: exports AgentId
crates/core/src/governor/paths.rs             modifies: normalise becomes public
docs/plans/project-plan.md                    modifies: records what this step's interface became
docs/plans/phase-2-protocol-store-cli/step-06-project-files.md modifies: this plan, ticked as it goes
```

## Tasks

Blocks are separated by exactly one blank line. `rustfmt.toml` allows no more, and `cargo xtask check` runs the format check before it runs a test, so two blank lines at a seam fail a task before anything is tried.

### Task 1: The layout, and the team that makes it a project

Files: created `crates/store/src/files.rs`, `crates/store/src/files/fixtures.rs`, `crates/store/tests/project_files.rs`; modified `crates/store/src/lib.rs`, `crates/store/Cargo.toml`, `Cargo.toml`, `Cargo.lock`, `docs/plans/phase-2-protocol-store-cli/step-06-project-files.md`

Consumes: nothing from this plan
Produces: `farik_store::files::{FilesError, ProjectFiles}`, `ProjectFiles::{open, root, init, read_team, write_team}`, and the fixtures every later task's tests use

- [ ] Add the dependency. In the workspace `Cargo.toml`, after the `typify` line:

  ```toml
  yaml_serde = "=0.10.7"
  ```

  and in `crates/store/Cargo.toml`, replace `serde_json.workspace = true` with:

  ```toml
  serde.workspace = true
  serde_json.workspace = true
  yaml_serde.workspace = true
  ```

- [ ] Write the failing tests. Create `crates/store/src/files.rs` with the module doc and the fixtures declaration:

  ```rust
  //! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
  //!
  //! The event log is the source of truth for what happened; these files are the source of truth for
  //! what the team knows. They are what travels with the repository, so every one of them is text a
  //! person can read, diff and edit — and every structured one is held, when it is read back, to
  //! exactly the rules it would be held to arriving on the wire.

  /// Wire fixtures and a temporary project, for tests in this crate and in others.
  pub mod fixtures;
  ```

- [ ] Create `crates/store/src/files/fixtures.rs`:

  ```rust
  use std::path::PathBuf;

  use farik_core::team::{Team, validate_team};

  use super::ProjectFiles;

  /// A project of its own, in a directory removed when the value is dropped however the test ends.
  pub struct TempProject {
      /// The repository root, which is where `.farik/` goes.
      pub root: PathBuf,
  }

  impl TempProject {
      /// A directory nothing else is using, named after the test that asked for it.
      ///
      /// # Panics
      ///
      /// When the directory cannot be made, which is the machine refusing rather than the code.
      #[must_use]
      pub fn new(name: &str) -> Self {
          let root = std::env::temp_dir().join(format!(
              "farik-files-{name}-{}-{:?}",
              std::process::id(),
              std::thread::current().id()
          ));
          let _ = std::fs::remove_dir_all(&root);
          std::fs::create_dir_all(&root).expect("a directory under the temporary directory");
          Self { root }
      }

      /// The files of this project.
      #[must_use]
      pub fn files(&self) -> ProjectFiles {
          ProjectFiles::open(self.root.clone())
      }
  }

  impl Drop for TempProject {
      fn drop(&mut self) {
          let _ = std::fs::remove_dir_all(&self.root);
      }
  }

  /// The team `farik_core`'s own fixture describes, typed.
  ///
  /// # Panics
  ///
  /// When that fixture stops being a team, which is a change to `farik-core`'s own tests.
  #[must_use]
  pub fn a_team() -> Team {
      validate_team(&farik_core::team::fixtures::a_team_wire()).expect("the fixture is a team")
  }
  ```

- [ ] Create `crates/store/tests/project_files.rs`:

  ```rust
  //! The files under `.farik/`, against a real directory.
  //!
  //! Every test here needs somewhere to write and nothing else, so they run in the default
  //! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

  use farik_store::files::FilesError;
  use farik_store::files::fixtures::{TempProject, a_team};

  #[test]
  fn makes_the_layout_a_project_starts_with() {
      let project = TempProject::new("init");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      for directory in [
          "team",
          "contracts",
          "agents",
          "decisions",
          "product",
          "local",
      ] {
          assert!(
              project.root.join(".farik").join(directory).is_dir(),
              "{directory} is where a person will look for it"
          );
      }
      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Farik"
      );
      // Without this, every repository Farik touches is dirty for good: a task's worktree and the
      // event log both live under .farik/local/ (5.14, 8.4).
      assert_eq!(
          std::fs::read_to_string(project.root.join(".farik/local/.gitignore"))
              .expect("the ignore file"),
          "*\n"
      );
  }

  #[test]
  fn a_second_init_takes_nothing_away() {
      let project = TempProject::new("init-twice");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      let mut changed = a_team();
      changed.name = "Renamed by hand".parse().expect("a name");
      files.write_team(&changed).expect("the team is written");
      files.init(&a_team()).expect("init runs again");

      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Renamed by hand",
          "a second init is a command with nothing to do, not one that throws the team away"
      );
  }

  #[test]
  fn reads_back_the_team_it_wrote() {
      let project = TempProject::new("round-trip-team");
      let files = project.files();
      files.write_team(&a_team()).expect("the team is written");

      assert_eq!(files.read_team().expect("the team"), a_team());
      // YAML, because a person edits this by hand and a person does not edit JSON by hand.
      let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
      assert!(text.starts_with("agents:"), "{text}");
  }

  #[test]
  fn says_there_is_no_team_when_there_is_none() {
      let project = TempProject::new("no-team");
      assert_eq!(
          project.files().read_team(),
          Err(FilesError::NotFound {
              path: ".farik/team.yaml".to_string(),
          })
      );
  }

  #[test]
  fn refuses_a_team_file_a_person_broke() {
      let project = TempProject::new("broken-team");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      // A file read back is held to what a team on the wire is held to: the same rule, the same
      // words, whichever door it came through.
      std::fs::write(
          project.root.join(".farik/team.yaml"),
          "name: Farik\nagents: []\nbudgets:\n  daily_usd: 20\npolicy:\n  human_accepts_contracts: high_risk\n  wip_limit_per_agent: 2\n  blocked_limit_hours: 24\n  max_iterations: 3\n  integration: manual\nrules: {}\n",
      )
      .expect("the file is written");
      let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
          panic!("a team of nobody is not a team");
      };
      assert_eq!(path, ".farik/team.yaml");
      assert!(detail.contains("/agents"), "{detail}");

      std::fs::write(project.root.join(".farik/team.yaml"), "name: [\n").expect("the file");
      let Err(FilesError::Invalid { detail, .. }) = files.read_team() else {
          panic!("this is not YAML at all");
      };
      assert!(!detail.is_empty(), "the parser's own words");
  }

  #[test]
  fn leaves_nothing_half_written() {
      // A write goes to a file beside the one being written and is renamed over it, because a rename
      // within a directory is the one file operation that is all or nothing. A crash in the middle of
      // team.yaml would otherwise leave the team unreadable, and the team is what every session is
      // assembled from.
      let project = TempProject::new("atomic");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");
      files.write_team(&a_team()).expect("and written again");

      let left_behind: Vec<String> = std::fs::read_dir(project.root.join(".farik"))
          .expect("the directory reads")
          .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
          .filter_map(|name| name.to_str().map(std::string::ToString::to_string))
          .filter(|name| name.contains("writing"))
          .collect();
      assert!(left_behind.is_empty(), "{left_behind:?}");
  }

  #[test]
  fn refuses_to_write_a_team_that_could_not_be_read_back() {
      // The round trip is the promise: what write_team accepts, read_team returns. A value built in
      // memory that breaks a rule of the file is refused here rather than becoming a file nothing
      // can open.
      let project = TempProject::new("write-invalid");
      let files = project.files();
      let mut team = a_team();
      team.agents.truncate(1);
      let refused = files.write_team(&team);
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(!project.root.join(".farik/team.yaml").exists());
  }
  ```

- [ ] Append the tests module to `crates/store/src/files.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::FilesError;

      #[test]
      fn says_what_it_could_not_use_and_why_in_plain_words() {
          let said: Vec<String> = [
              FilesError::NotFound {
                  path: ".farik/team.yaml".to_string(),
              },
              FilesError::Invalid {
                  path: ".farik/team.yaml".to_string(),
                  detail: "/agents an agent id names one agent".to_string(),
              },
              FilesError::Io {
                  path: ".farik/team.yaml".to_string(),
                  detail: "Permission denied (os error 13)".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no .farik/team.yaml",
                  ".farik/team.yaml is not usable: /agents an agent id names one agent",
                  ".farik/team.yaml could not be used: Permission denied (os error 13)",
              ]
          );
      }
  }
  ```

- [ ] Declare the module in `crates/store/src/lib.rs`, after `event_log` and before `git`:

  ```rust
  /// The files under `.farik/`.
  pub mod files;
  ```

- [ ] Run them and confirm they fail because nothing of the files exists:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice — the fixtures name a type that is not there, and
  # so do the tests:
  # error[E0432]: unresolved import `super::ProjectFiles`
  # error[E0432]: unresolved import `super::FilesError`
  # error: could not compile `farik-store` (lib) due to 1 previous error
  # error: could not compile `farik-store` (lib test) due to 2 previous errors
  ```

- [ ] Write the minimal implementation. Replace the module doc of `crates/store/src/files.rs` with the doc and its imports:

  ```rust
  //! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
  //!
  //! The event log is the source of truth for what happened; these files are the source of truth for
  //! what the team knows. They are what travels with the repository, so every one of them is text a
  //! person can read, diff and edit — and every structured one is held, when it is read back, to
  //! exactly the rules it would be held to arriving on the wire.

  use std::fmt;
  use std::path::{Path, PathBuf};

  use farik_core::contract::ValidationError;
  use farik_core::team::{Team, validate_team};
  use serde::Serialize;
  use serde_json::Value;

  /// Wire fixtures and a temporary project, for tests in this crate and in others.
  pub mod fixtures;
  ```

- [ ] Insert, between that and the tests module, the error:

  ```rust
  /// Why a file could not be read or written.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum FilesError {
      /// There is no file there. A caller that can go on without one says so by asking for an
      /// `Option` instead; everything else treats this as the answer.
      NotFound {
          /// The path, relative to the project root.
          path: String,
      },
      /// The file is there and is not what it should be: it is not the format, or it is the format
      /// and breaks a rule the format has.
      Invalid {
          /// The path, relative to the project root.
          path: String,
          /// What was wrong, in the words of whatever refused it.
          detail: String,
      },
      /// The operating system refused.
      Io {
          /// The path, relative to the project root.
          path: String,
          /// What it said.
          detail: String,
      },
  }

  impl fmt::Display for FilesError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::NotFound { path } => write!(formatter, "there is no {path}"),
              Self::Invalid { path, detail } => write!(formatter, "{path} is not usable: {detail}"),
              Self::Io { path, detail } => write!(formatter, "{path} could not be used: {detail}"),
          }
      }
  }

  impl std::error::Error for FilesError {}
  ```

- [ ] Then, after it, the project and what `init` makes:

  ```rust
  /// The `.farik/` directory of one project, at the root of its git repository.
  pub struct ProjectFiles {
      root: PathBuf,
  }

  /// What `.farik/local/.gitignore` holds: everything under it, including itself.
  ///
  /// A task's worktree lives at `.farik/local/worktrees/FRK-<n>` (5.14) and the event log at
  /// `.farik/local/farik.db` (8.4). Without this, every repository Farik touches is dirty for good
  /// and `Git::is_clean` on the root never answers true again.
  const LOCAL_GITIGNORE: &str = "*\n";

  impl ProjectFiles {
      /// The `.farik/` directory of the repository at `root`. Nothing is read and nothing is written
      /// until a method is called.
      #[must_use]
      pub fn open(root: PathBuf) -> Self {
          Self { root }
      }

      /// The project root, which is the git repository's root.
      #[must_use]
      pub fn root(&self) -> &Path {
          &self.root
      }

      /// Makes `.farik/` and everything under it that a project starts with, and writes the team.
      ///
      /// Nothing that is already there is overwritten: a second `farik init` on a project that has a
      /// team is a command that has nothing to do, not one that throws the team away. The directories
      /// are made whether or not they hold anything yet, so that a person opening `.farik/` sees where
      /// things go.
      ///
      /// # Errors
      ///
      /// `Io` when a directory or a file cannot be made, `Invalid` when the team is not one
      /// `validate_team` accepts — which it must be, since it is what a later read will be held to.
      pub fn init(&self, team: &Team) -> Result<(), FilesError> {
          for directory in [
              "",
              "team",
              "contracts",
              "agents",
              "decisions",
              "product",
              "local",
          ] {
              self.make_directory(&self.farik().join(directory))?;
          }
          self.write_if_absent(&self.local().join(".gitignore"), LOCAL_GITIGNORE)?;
          if self.path_of(TEAM).exists() {
              return Ok(());
          }
          self.write_team(team)
      }
  ```

- [ ] Then the team, and the brace that closes the block those three live in:

  ```rust
      /// The team, held to the rules a team on the wire is held to.
      ///
      /// # Errors
      ///
      /// `NotFound` when there is no team file, `Invalid` when it is not YAML or not a team,
      /// `Io` otherwise.
      pub fn read_team(&self) -> Result<Team, FilesError> {
          let value = self.read_yaml(TEAM)?;
          validate_team(&value).map_err(|errors| self.refused(TEAM, &errors))
      }

      /// Writes the team, after holding it to the same rules.
      ///
      /// # Errors
      ///
      /// `Invalid` when the team is not one `validate_team` accepts, `Io` when it cannot be written.
      pub fn write_team(&self, team: &Team) -> Result<(), FilesError> {
          let value = self.as_wire(TEAM, team)?;
          validate_team(&value).map_err(|errors| self.refused(TEAM, &errors))?;
          self.write_yaml(TEAM, &value)
      }
  }
  ```

- [ ] Then the layout, which grows a line per task:

  ```rust
  /// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
  /// the whole layout at once and a change to it is one line.
  const TEAM: &str = "team.yaml";
  ```

- [ ] Then the paths, which is where the second `impl` block opens:

  ```rust
  impl ProjectFiles {
      /// `.farik/`, where everything this module touches lives.
      fn farik(&self) -> PathBuf {
          self.root.join(".farik")
      }

      /// `.farik/local/`, which is this machine's and is never committed (8.4).
      fn local(&self) -> PathBuf {
          self.farik().join("local")
      }

      /// One file's whole path, from its place in the layout.
      fn path_of(&self, relative: &str) -> PathBuf {
          self.farik().join(relative)
      }

      /// The path as a person reads it in a refusal: from the project root, with `/` separators
      /// whatever the machine uses.
      fn named(relative: &str) -> String {
          format!(".farik/{relative}")
      }

      /// Every refusal a validator gave, as one `Invalid`.
      fn refused(&self, relative: &str, errors: &[ValidationError]) -> FilesError {
          let _ = self;
          FilesError::Invalid {
              path: Self::named(relative),
              detail: errors
                  .iter()
                  .map(|error| format!("{} {}", error.path, error.message))
                  .collect::<Vec<_>>()
                  .join("; "),
          }
      }

      fn make_directory(&self, path: &Path) -> Result<(), FilesError> {
          std::fs::create_dir_all(path).map_err(|error| FilesError::Io {
              path: path
                  .strip_prefix(&self.root)
                  .unwrap_or(path)
                  .display()
                  .to_string(),
              detail: error.to_string(),
          })
      }

      /// Writes a file only when there is nothing there, so that `init` never takes anything away.
      fn write_if_absent(&self, path: &Path, text: &str) -> Result<(), FilesError> {
          if path.exists() {
              return Ok(());
          }
          std::fs::write(path, text).map_err(|error| FilesError::Io {
              path: path
                  .strip_prefix(&self.root)
                  .unwrap_or(path)
                  .display()
                  .to_string(),
              detail: error.to_string(),
          })
      }
  ```

- [ ] And last the reading and the writing, which closes it:

  ```rust
      /// One file's text.
      fn read_text(&self, relative: &str) -> Result<String, FilesError> {
          let path = self.path_of(relative);
          match std::fs::read_to_string(&path) {
              Ok(text) => Ok(text),
              Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                  Err(FilesError::NotFound {
                      path: Self::named(relative),
                  })
              }
              Err(error) => Err(FilesError::Io {
                  path: Self::named(relative),
                  detail: error.to_string(),
              }),
          }
      }

      /// Writes one file, making the directory it lives in, and leaving nothing half-written.
      ///
      /// The text goes to a file beside the one being written and is renamed over it, because a
      /// rename within a directory is the one file operation that is all or nothing. A crash in the
      /// middle of writing `team.yaml` would otherwise leave the team unreadable, and the team is
      /// what every session is assembled from.
      fn write_text(&self, relative: &str, text: &str) -> Result<(), FilesError> {
          let path = self.path_of(relative);
          if let Some(directory) = path.parent() {
              self.make_directory(directory)?;
          }
          let beside = path.with_extension(format!(
              "{}.writing",
              path.extension()
                  .and_then(std::ffi::OsStr::to_str)
                  .unwrap_or_default()
          ));
          std::fs::write(&beside, text).map_err(|error| FilesError::Io {
              path: Self::named(relative),
              detail: error.to_string(),
          })?;
          std::fs::rename(&beside, &path).map_err(|error| FilesError::Io {
              path: Self::named(relative),
              detail: error.to_string(),
          })
      }

      /// One YAML file as an untrusted value, for a validator to hold to its rules.
      fn read_yaml(&self, relative: &str) -> Result<Value, FilesError> {
          let text = self.read_text(relative)?;
          yaml_serde::from_str(&text).map_err(|error| FilesError::Invalid {
              path: Self::named(relative),
              detail: error.to_string(),
          })
      }

      /// A typed value as the wire sees it, which is what a validator reads.
      ///
      /// Every writer takes this step and then holds the result to its own file's rules, so that a
      /// value built in memory cannot become a file that cannot be read back. The round trip is the
      /// promise: what `write_team` accepts, `read_team` returns.
      fn as_wire<T: Serialize>(&self, relative: &str, value: &T) -> Result<Value, FilesError> {
          let _ = self;
          serde_json::to_value(value).map_err(|error| FilesError::Invalid {
              path: Self::named(relative),
              detail: error.to_string(),
          })
      }

      /// Writes a wire value as the YAML a person reads and edits.
      fn write_yaml(&self, relative: &str, value: &Value) -> Result<(), FilesError> {
          let text = yaml_serde::to_string(value).map_err(|error| FilesError::Invalid {
              path: Self::named(relative),
              detail: error.to_string(),
          })?;
          self.write_text(relative, &text)
      }
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 264 passed (farik-core)
  #   test result: ok. 45 passed (farik-store)
  #   test result: ok. 7 passed (crates/store/tests/project_files.rs)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```


- [ ] Commit: `feat(store): make the files a project starts with`

### Task 2: The criterion library and the contracts

Files: modified `crates/store/src/files.rs`, `crates/store/tests/project_files.rs`, `docs/plans/phase-2-protocol-store-cli/step-06-project-files.md`

Consumes: everything Task 1 made
Produces: `ProjectFiles::{read_criteria, write_criteria, read_contract, write_contract, list_contracts}`

- [ ] Write the failing tests. Replace `crates/store/tests/project_files.rs`, whole, with:

  ```rust
  //! The files under `.farik/`, against a real directory.
  //!
  //! Every test here needs somewhere to write and nothing else, so they run in the default
  //! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

  use farik_core::contract::{TaskId, validate_contract};
  use farik_core::criteria::{fixtures::a_criteria_library_wire, validate_criteria};
  use farik_store::files::FilesError;
  use farik_store::files::fixtures::{TempProject, a_team};

  /// The contract `farik-core`'s own fixture describes, with the id a test asks for.
  fn a_contract(id: &str) -> farik_core::contract::TaskContract {
      let mut wire = farik_core::contract::fixtures::a_contract_wire();
      wire["id"] = serde_json::json!(id);
      validate_contract(&wire).expect("the fixture is a contract")
  }

  #[test]
  fn makes_the_layout_a_project_starts_with() {
      let project = TempProject::new("init");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      for directory in [
          "team",
          "contracts",
          "agents",
          "decisions",
          "product",
          "local",
      ] {
          assert!(
              project.root.join(".farik").join(directory).is_dir(),
              "{directory} is where a person will look for it"
          );
      }
      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Farik"
      );
      // Without this, every repository Farik touches is dirty for good: a task's worktree and the
      // event log both live under .farik/local/ (5.14, 8.4).
      assert_eq!(
          std::fs::read_to_string(project.root.join(".farik/local/.gitignore"))
              .expect("the ignore file"),
          "*\n"
      );
  }

  #[test]
  fn a_second_init_takes_nothing_away() {
      let project = TempProject::new("init-twice");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      let mut changed = a_team();
      changed.name = "Renamed by hand".parse().expect("a name");
      files.write_team(&changed).expect("the team is written");
      files.init(&a_team()).expect("init runs again");

      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Renamed by hand",
          "a second init is a command with nothing to do, not one that throws the team away"
      );
  }

  #[test]
  fn reads_back_the_team_it_wrote() {
      let project = TempProject::new("round-trip-team");
      let files = project.files();
      files.write_team(&a_team()).expect("the team is written");

      assert_eq!(files.read_team().expect("the team"), a_team());
      // YAML, because a person edits this by hand and a person does not edit JSON by hand.
      let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
      assert!(text.starts_with("agents:"), "{text}");
  }

  #[test]
  fn reads_back_the_criterion_library_it_wrote() {
      let project = TempProject::new("round-trip-criteria");
      let files = project.files();
      let library = validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library");
      files
          .write_criteria(&library)
          .expect("the library is written");

      assert_eq!(files.read_criteria().expect("the library"), library);
      assert!(
          project.root.join(".farik/team/criteria.yaml").is_file(),
          "beside the team's other files, which is where 5.13 puts it"
      );
  }

  #[test]
  fn says_there_is_no_team_when_there_is_none() {
      let project = TempProject::new("no-team");
      assert_eq!(
          project.files().read_team(),
          Err(FilesError::NotFound {
              path: ".farik/team.yaml".to_string(),
          })
      );
  }

  #[test]
  fn refuses_a_team_file_a_person_broke() {
      let project = TempProject::new("broken-team");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      // A file read back is held to what a team on the wire is held to: the same rule, the same
      // words, whichever door it came through.
      std::fs::write(
          project.root.join(".farik/team.yaml"),
          "name: Farik\nagents: []\nbudgets:\n  daily_usd: 20\npolicy:\n  human_accepts_contracts: high_risk\n  wip_limit_per_agent: 2\n  blocked_limit_hours: 24\n  max_iterations: 3\n  integration: manual\nrules: {}\n",
      )
      .expect("the file is written");
      let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
          panic!("a team of nobody is not a team");
      };
      assert_eq!(path, ".farik/team.yaml");
      assert!(detail.contains("/agents"), "{detail}");

      std::fs::write(project.root.join(".farik/team.yaml"), "name: [\n").expect("the file");
      let Err(FilesError::Invalid { detail, .. }) = files.read_team() else {
          panic!("this is not YAML at all");
      };
      assert!(!detail.is_empty(), "the parser's own words");
  }

  #[test]
  fn leaves_nothing_half_written() {
      // A write goes to a file beside the one being written and is renamed over it, because a rename
      // within a directory is the one file operation that is all or nothing. A crash in the middle of
      // team.yaml would otherwise leave the team unreadable, and the team is what every session is
      // assembled from.
      let project = TempProject::new("atomic");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");
      files.write_team(&a_team()).expect("and written again");

      let left_behind: Vec<String> = std::fs::read_dir(project.root.join(".farik"))
          .expect("the directory reads")
          .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
          .filter_map(|name| name.to_str().map(std::string::ToString::to_string))
          .filter(|name| name.contains("writing"))
          .collect();
      assert!(left_behind.is_empty(), "{left_behind:?}");
  }

  #[test]
  fn refuses_to_write_a_team_that_could_not_be_read_back() {
      // The round trip is the promise: what write_team accepts, read_team returns. A value built in
      // memory that breaks a rule of the file is refused here rather than becoming a file nothing
      // can open.
      let project = TempProject::new("write-invalid");
      let files = project.files();
      let mut team = a_team();
      team.agents.truncate(1);
      let refused = files.write_team(&team);
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(!project.root.join(".farik/team.yaml").exists());
  }

  #[test]
  fn writes_a_contract_to_the_file_its_own_id_names() {
      let project = TempProject::new("contracts");
      let files = project.files();
      let contract = a_contract("FRK-7");
      files.write_contract(&contract).expect("it is written");

      assert!(project.root.join(".farik/contracts/FRK-7.yaml").is_file());
      let id = TaskId::try_from("FRK-7").expect("an id");
      assert_eq!(files.read_contract(&id).expect("it reads back"), contract);
  }

  #[test]
  fn refuses_a_contract_that_says_it_is_another() {
      // The file name and the id inside it are two claims about the same thing, and a board that
      // believed the file name would show a task that does not exist.
      let project = TempProject::new("contract-id");
      let files = project.files();
      files.write_contract(&a_contract("FRK-7")).expect("written");
      std::fs::rename(
          project.root.join(".farik/contracts/FRK-7.yaml"),
          project.root.join(".farik/contracts/FRK-8.yaml"),
      )
      .expect("a person moves it");

      let id = TaskId::try_from("FRK-8").expect("an id");
      let Err(FilesError::Invalid { path, detail }) = files.read_contract(&id) else {
          panic!("the contract inside says FRK-7");
      };
      assert_eq!(path, ".farik/contracts/FRK-8.yaml");
      assert!(detail.contains("says it is FRK-7"), "{detail}");
  }

  #[test]
  fn lists_contracts_in_the_order_a_board_shows_them() {
      let project = TempProject::new("list");
      let files = project.files();
      for id in ["FRK-10", "FRK-2", "FRK-1"] {
          files.write_contract(&a_contract(id)).expect("written");
      }
      // A person's own notes in the same directory are not contracts and are not a problem either.
      std::fs::write(project.root.join(".farik/contracts/notes.md"), "mine\n").expect("a note");
      std::fs::write(project.root.join(".farik/contracts/FRK-3.txt"), "?\n").expect("not a contract");

      assert_eq!(
          files
              .list_contracts()
              .expect("they list")
              .iter()
              .map(|id| id.as_str().to_string())
              .collect::<Vec<_>>(),
          ["FRK-1", "FRK-2", "FRK-10"],
          "by the number in the id, so the tenth does not come before the ninth"
      );
  }

  #[test]
  fn a_project_with_nothing_in_it_has_no_contracts() {
      assert!(
          TempProject::new("list-empty")
              .files()
              .list_contracts()
              .expect("an empty list, not a refusal")
              .is_empty()
      );
  }
  ```

- [ ] Replace the tests module of `crates/store/src/files.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use farik_core::contract::TaskId;

      use super::{FilesError, contract_path};

      #[test]
      fn says_what_it_could_not_use_and_why_in_plain_words() {
          let said: Vec<String> = [
              FilesError::NotFound {
                  path: ".farik/team.yaml".to_string(),
              },
              FilesError::Invalid {
                  path: ".farik/team.yaml".to_string(),
                  detail: "/agents an agent id names one agent".to_string(),
              },
              FilesError::Io {
                  path: ".farik/team.yaml".to_string(),
                  detail: "Permission denied (os error 13)".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no .farik/team.yaml",
                  ".farik/team.yaml is not usable: /agents an agent id names one agent",
                  ".farik/team.yaml could not be used: Permission denied (os error 13)",
              ]
          );
      }

      #[test]
      fn names_the_file_a_contract_lives_in() {
          assert_eq!(
              contract_path(&TaskId::try_from("FRK-12").expect("an id")),
              "contracts/FRK-12.yaml"
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because nothing names a contract's file:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0432]: unresolved import `super::contract_path`
  # error: could not compile `farik-store` (lib test) due to 1 previous error
  ```

- [ ] Write the minimal implementation. In `crates/store/src/files.rs`, replace the `farik_core::contract` import line with:

  ```rust
  use farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract};
  use farik_core::criteria::{CriteriaLibrary, validate_criteria};
  ```

- [ ] Insert into the first `impl ProjectFiles` block, after `write_team`:

  ```rust
      /// The criterion library, held to the rules one on the wire is held to.
      ///
      /// # Errors
      ///
      /// `NotFound` when there is no library, `Invalid` when it is not YAML or not a library, `Io`
      /// otherwise.
      pub fn read_criteria(&self) -> Result<CriteriaLibrary, FilesError> {
          let value = self.read_yaml(CRITERIA)?;
          validate_criteria(&value).map_err(|errors| self.refused(CRITERIA, &errors))
      }

      /// Writes the criterion library, after holding it to the same rules.
      ///
      /// # Errors
      ///
      /// `Invalid` when the library is not one `validate_criteria` accepts, `Io` when it cannot be
      /// written.
      pub fn write_criteria(&self, library: &CriteriaLibrary) -> Result<(), FilesError> {
          let value = self.as_wire(CRITERIA, library)?;
          validate_criteria(&value).map_err(|errors| self.refused(CRITERIA, &errors))?;
          self.write_yaml(CRITERIA, &value)
      }

      /// One task's contract, held to the rules a contract on the wire is held to.
      ///
      /// # Errors
      ///
      /// `NotFound` when there is no contract with that id, `Invalid` when the file is not YAML or
      /// not a contract, or when the contract in it carries a different id from the one asked for.
      pub fn read_contract(&self, id: &TaskId) -> Result<TaskContract, FilesError> {
          let path = contract_path(id);
          let value = self.read_yaml(&path)?;
          let contract = validate_contract(&value).map_err(|errors| self.refused(&path, &errors))?;
          if contract.id == *id {
              Ok(contract)
          } else {
              Err(FilesError::Invalid {
                  path: Self::named(&path),
                  detail: format!(
                      "the contract in it says it is {}, and a contract lives in the file its own id \
                       names",
                      contract.id.as_str()
                  ),
              })
          }
      }

      /// Writes a contract to the file its own id names, after holding it to the same rules.
      ///
      /// # Errors
      ///
      /// `Invalid` when the contract is not one `validate_contract` accepts, `Io` when it cannot be
      /// written.
      pub fn write_contract(&self, contract: &TaskContract) -> Result<(), FilesError> {
          let path = contract_path(&contract.id);
          let value = self.as_wire(&path, contract)?;
          validate_contract(&value).map_err(|errors| self.refused(&path, &errors))?;
          self.write_yaml(&path, &value)
      }

      /// Every contract there is, by id, in the order a board shows them: by the number in the id, so
      /// that the tenth task does not come before the ninth.
      ///
      /// A file under `contracts/` that is not a contract's is not one of them and is not an error
      /// either: the directory is a person's to keep notes in.
      ///
      /// # Errors
      ///
      /// `Io` when the directory cannot be read. A project with no `.farik/` has no contracts, which
      /// is not an error.
      pub fn list_contracts(&self) -> Result<Vec<TaskId>, FilesError> {
          let directory = self.farik().join("contracts");
          if !directory.is_dir() {
              return Ok(Vec::new());
          }
          let entries = std::fs::read_dir(&directory).map_err(|error| FilesError::Io {
              path: ".farik/contracts".to_string(),
              detail: error.to_string(),
          })?;
          let mut ids: Vec<TaskId> = Vec::new();
          for entry in entries {
              let entry = entry.map_err(|error| FilesError::Io {
                  path: ".farik/contracts".to_string(),
                  detail: error.to_string(),
              })?;
              let name = entry.file_name();
              let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".yaml")) else {
                  continue;
              };
              if let Ok(id) = TaskId::try_from(name) {
                  ids.push(id);
              }
          }
          ids.sort_by_key(|id| {
              (
                  id.as_str()
                      .trim_start_matches("FRK-")
                      .parse::<u64>()
                      .unwrap_or(u64::MAX),
                  id.to_string(),
              )
          });
          Ok(ids)
      }
  ```

- [ ] Add the library's line to the layout, which becomes:

  ```rust
  /// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
  /// the whole layout at once and a change to it is one line.
  const TEAM: &str = "team.yaml";
  const CRITERIA: &str = "team/criteria.yaml";
  ```

- [ ] And insert, after the layout and before the second `impl` block:

  ```rust
  /// The file a contract lives in: the one its own id names.
  fn contract_path(id: &TaskId) -> String {
      format!("contracts/{}.yaml", id.as_str())
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 46 passed (farik-store)
  #   test result: ok. 12 passed (crates/store/tests/project_files.rs)
  ```

- [ ] Commit: `feat(store): read a contract back from the file its id names`

### Task 3: What an agent writes, and what a person writes

Files: modified `crates/store/src/files.rs`, `crates/store/tests/project_files.rs`, `crates/core/src/team.rs`, `crates/core/src/governor/paths.rs`, `docs/plans/phase-2-protocol-store-cli/step-06-project-files.md`

Consumes: everything above
Produces: `ProjectFiles::{read_memory, write_memory, read_project_scan, write_project_scan, read_product_doc, write_product_doc}`, `farik_core::team::AgentId`, and `farik_core::governor::paths::normalise` made public

- [ ] Write the failing tests. Replace `crates/store/tests/project_files.rs`, whole, with:

  ```rust
  //! The files under `.farik/`, against a real directory.
  //!
  //! Every test here needs somewhere to write and nothing else, so they run in the default
  //! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

  use farik_core::contract::{TaskId, validate_contract};
  use farik_core::criteria::{fixtures::a_criteria_library_wire, validate_criteria};
  use farik_core::team::AgentId;
  use farik_store::files::FilesError;
  use farik_store::files::fixtures::{TempProject, a_team};

  /// The contract `farik-core`'s own fixture describes, with the id a test asks for.
  fn a_contract(id: &str) -> farik_core::contract::TaskContract {
      let mut wire = farik_core::contract::fixtures::a_contract_wire();
      wire["id"] = serde_json::json!(id);
      validate_contract(&wire).expect("the fixture is a contract")
  }

  #[test]
  fn makes_the_layout_a_project_starts_with() {
      let project = TempProject::new("init");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      for directory in [
          "team",
          "contracts",
          "agents",
          "decisions",
          "product",
          "local",
      ] {
          assert!(
              project.root.join(".farik").join(directory).is_dir(),
              "{directory} is where a person will look for it"
          );
      }
      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Farik"
      );
      // Without this, every repository Farik touches is dirty for good: a task's worktree and the
      // event log both live under .farik/local/ (5.14, 8.4).
      assert_eq!(
          std::fs::read_to_string(project.root.join(".farik/local/.gitignore"))
              .expect("the ignore file"),
          "*\n"
      );
  }

  #[test]
  fn a_second_init_takes_nothing_away() {
      let project = TempProject::new("init-twice");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      let mut changed = a_team();
      changed.name = "Renamed by hand".parse().expect("a name");
      files.write_team(&changed).expect("the team is written");
      files.init(&a_team()).expect("init runs again");

      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Renamed by hand",
          "a second init is a command with nothing to do, not one that throws the team away"
      );
  }

  #[test]
  fn reads_back_the_team_it_wrote() {
      let project = TempProject::new("round-trip-team");
      let files = project.files();
      files.write_team(&a_team()).expect("the team is written");

      assert_eq!(files.read_team().expect("the team"), a_team());
      // YAML, because a person edits this by hand and a person does not edit JSON by hand.
      let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
      assert!(text.starts_with("agents:"), "{text}");
  }

  #[test]
  fn reads_back_the_criterion_library_it_wrote() {
      let project = TempProject::new("round-trip-criteria");
      let files = project.files();
      let library = validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library");
      files
          .write_criteria(&library)
          .expect("the library is written");

      assert_eq!(files.read_criteria().expect("the library"), library);
      assert!(
          project.root.join(".farik/team/criteria.yaml").is_file(),
          "beside the team's other files, which is where 5.13 puts it"
      );
  }

  #[test]
  fn says_there_is_no_team_when_there_is_none() {
      let project = TempProject::new("no-team");
      assert_eq!(
          project.files().read_team(),
          Err(FilesError::NotFound {
              path: ".farik/team.yaml".to_string(),
          })
      );
  }

  #[test]
  fn refuses_a_team_file_a_person_broke() {
      let project = TempProject::new("broken-team");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      // A file read back is held to what a team on the wire is held to: the same rule, the same
      // words, whichever door it came through.
      std::fs::write(
          project.root.join(".farik/team.yaml"),
          "name: Farik\nagents: []\nbudgets:\n  daily_usd: 20\npolicy:\n  human_accepts_contracts: high_risk\n  wip_limit_per_agent: 2\n  blocked_limit_hours: 24\n  max_iterations: 3\n  integration: manual\nrules: {}\n",
      )
      .expect("the file is written");
      let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
          panic!("a team of nobody is not a team");
      };
      assert_eq!(path, ".farik/team.yaml");
      assert!(detail.contains("/agents"), "{detail}");

      std::fs::write(project.root.join(".farik/team.yaml"), "name: [\n").expect("the file");
      let Err(FilesError::Invalid { detail, .. }) = files.read_team() else {
          panic!("this is not YAML at all");
      };
      assert!(!detail.is_empty(), "the parser's own words");
  }

  #[test]
  fn leaves_nothing_half_written() {
      // A write goes to a file beside the one being written and is renamed over it, because a rename
      // within a directory is the one file operation that is all or nothing. A crash in the middle of
      // team.yaml would otherwise leave the team unreadable, and the team is what every session is
      // assembled from.
      let project = TempProject::new("atomic");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");
      files.write_team(&a_team()).expect("and written again");

      let left_behind: Vec<String> = std::fs::read_dir(project.root.join(".farik"))
          .expect("the directory reads")
          .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
          .filter_map(|name| name.to_str().map(std::string::ToString::to_string))
          .filter(|name| name.contains("writing"))
          .collect();
      assert!(left_behind.is_empty(), "{left_behind:?}");
  }

  #[test]
  fn refuses_to_write_a_team_that_could_not_be_read_back() {
      // The round trip is the promise: what write_team accepts, read_team returns. A value built in
      // memory that breaks a rule of the file is refused here rather than becoming a file nothing
      // can open.
      let project = TempProject::new("write-invalid");
      let files = project.files();
      let mut team = a_team();
      team.agents.truncate(1);
      let refused = files.write_team(&team);
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(!project.root.join(".farik/team.yaml").exists());
  }

  #[test]
  fn writes_a_contract_to_the_file_its_own_id_names() {
      let project = TempProject::new("contracts");
      let files = project.files();
      let contract = a_contract("FRK-7");
      files.write_contract(&contract).expect("it is written");

      assert!(project.root.join(".farik/contracts/FRK-7.yaml").is_file());
      let id = TaskId::try_from("FRK-7").expect("an id");
      assert_eq!(files.read_contract(&id).expect("it reads back"), contract);
  }

  #[test]
  fn refuses_a_contract_that_says_it_is_another() {
      // The file name and the id inside it are two claims about the same thing, and a board that
      // believed the file name would show a task that does not exist.
      let project = TempProject::new("contract-id");
      let files = project.files();
      files.write_contract(&a_contract("FRK-7")).expect("written");
      std::fs::rename(
          project.root.join(".farik/contracts/FRK-7.yaml"),
          project.root.join(".farik/contracts/FRK-8.yaml"),
      )
      .expect("a person moves it");

      let id = TaskId::try_from("FRK-8").expect("an id");
      let Err(FilesError::Invalid { path, detail }) = files.read_contract(&id) else {
          panic!("the contract inside says FRK-7");
      };
      assert_eq!(path, ".farik/contracts/FRK-8.yaml");
      assert!(detail.contains("says it is FRK-7"), "{detail}");
  }

  #[test]
  fn lists_contracts_in_the_order_a_board_shows_them() {
      let project = TempProject::new("list");
      let files = project.files();
      for id in ["FRK-10", "FRK-2", "FRK-1"] {
          files.write_contract(&a_contract(id)).expect("written");
      }
      // A person's own notes in the same directory are not contracts and are not a problem either.
      std::fs::write(project.root.join(".farik/contracts/notes.md"), "mine\n").expect("a note");
      std::fs::write(project.root.join(".farik/contracts/FRK-3.txt"), "?\n").expect("not a contract");

      assert_eq!(
          files
              .list_contracts()
              .expect("they list")
              .iter()
              .map(|id| id.as_str().to_string())
              .collect::<Vec<_>>(),
          ["FRK-1", "FRK-2", "FRK-10"],
          "by the number in the id, so the tenth does not come before the ninth"
      );
  }

  #[test]
  fn a_project_with_nothing_in_it_has_no_contracts() {
      assert!(
          TempProject::new("list-empty")
              .files()
              .list_contracts()
              .expect("an empty list, not a refusal")
              .is_empty()
      );
  }

  #[test]
  fn an_agent_that_never_wrote_a_notebook_has_an_empty_one() {
      // Every session for an agent includes its notebook (5.8). A missing file is not something to
      // tell an agent about; it is an agent that has not written anything down yet.
      let project = TempProject::new("memory");
      let files = project.files();
      let ada = AgentId::try_from("ada").expect("an id");
      assert_eq!(files.read_memory(&ada).expect("an empty notebook"), "");

      files
          .write_memory(&ada, "The login form is in src/login.\n")
          .expect("it is written");
      assert_eq!(
          files.read_memory(&ada).expect("it reads back"),
          "The login form is in src/login.\n"
      );
  }

  #[test]
  fn reads_back_the_project_scan_and_says_when_there_is_none() {
      let project = TempProject::new("scan");
      let files = project.files();
      assert_eq!(
          files.read_project_scan(),
          Err(FilesError::NotFound {
              path: ".farik/project.md".to_string(),
          })
      );
      files
          .write_project_scan("# Farik\n\nA Rust workspace.\n")
          .expect("it is written");
      assert!(
          files
              .read_project_scan()
              .expect("it reads back")
              .contains("A Rust workspace")
      );
  }

  #[test]
  fn writes_a_product_document_and_refuses_one_that_climbs_out() {
      let project = TempProject::new("product");
      let files = project.files();
      files
          .write_product_doc("areas/login.md", "# Login\n")
          .expect("it is written");
      assert!(project.root.join(".farik/product/areas/login.md").is_file());
      assert_eq!(
          files
              .read_product_doc("areas/login.md")
              .expect("it reads back"),
          "# Login\n"
      );

      let refused = files.write_product_doc("../team.yaml", "not here\n");
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(
          !project.root.join(".farik/team.yaml").exists(),
          "and nothing was written where it pointed"
      );
  }
  ```

- [ ] Replace the tests module of `crates/store/src/files.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use farik_core::contract::TaskId;
      use farik_core::team::AgentId;

      use super::{FilesError, contract_path, memory_path, product_path};

      #[test]
      fn says_what_it_could_not_use_and_why_in_plain_words() {
          let said: Vec<String> = [
              FilesError::NotFound {
                  path: ".farik/team.yaml".to_string(),
              },
              FilesError::Invalid {
                  path: ".farik/team.yaml".to_string(),
                  detail: "/agents an agent id names one agent".to_string(),
              },
              FilesError::Io {
                  path: ".farik/team.yaml".to_string(),
                  detail: "Permission denied (os error 13)".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "there is no .farik/team.yaml",
                  ".farik/team.yaml is not usable: /agents an agent id names one agent",
                  ".farik/team.yaml could not be used: Permission denied (os error 13)",
              ]
          );
      }

      #[test]
      fn names_the_file_a_contract_lives_in() {
          assert_eq!(
              contract_path(&TaskId::try_from("FRK-12").expect("an id")),
              "contracts/FRK-12.yaml"
          );
      }

      #[test]
      fn names_the_file_a_notebook_lives_in() {
          assert_eq!(
              memory_path(&AgentId::try_from("ada").expect("an id")),
              "agents/ada/memory.md"
          );
      }

      #[test]
      fn keeps_a_product_document_under_product() {
          assert_eq!(
              product_path("roadmap.md").expect("a path"),
              "product/roadmap.md"
          );
          assert_eq!(
              product_path("./areas/login.md").expect("a path"),
              "product/areas/login.md",
              "a . segment is dropped rather than refused"
          );
          assert_eq!(
              product_path("areas\\login.md").expect("a path"),
              "product/areas/login.md",
              "and a backslash is a separator"
          );
      }

      #[test]
      fn refuses_a_product_path_that_climbs_out_of_product() {
          // The path comes from a tool call, so it is a string an agent chose. 5.6 puts product
          // documents under product/ and nowhere else, and one `..` would put this one in the
          // repository's own source.
          for path in [
              "../team.yaml",
              "../../etc/passwd",
              "/etc/passwd",
              "",
              "a/../../b",
          ] {
              let refused = product_path(path);
              let Err(FilesError::Invalid {
                  path: named,
                  detail,
              }) = refused
              else {
                  panic!("{path:?} climbs out: {refused:?}");
              };
              assert_eq!(named, format!("product/{path}"));
              assert!(detail.contains("climbs out of it"), "{detail}");
          }
      }
  }
  ```

- [ ] Run them and confirm they fail because nothing names a notebook or a product document:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::memory_path`, `super::product_path`
  # error: could not compile `farik-store` (lib test) due to 1 previous error
  ```

- [ ] Give the store the two things `farik-core` has and does not offer. In `crates/core/src/team.rs`, add `AgentId` to the generated re-export, which becomes:

  ```rust
  pub use crate::generated::team::{
      Agent, AgentId, AgentStatus, Budgets as TeamBudgets, FarikTeam as Team, Model as AgentModel,
      ModelEffort as Effort, PermissionTier as PermissionTierWire, Policy as TeamPolicy,
      PolicyHumanAcceptsContracts as HumanAcceptsContracts, PolicyIntegration as Integration,
      Role as RoleWire, Rules as RulesWire, SessionLimits as SessionLimitsWire,
  };
  ```

- [ ] In `crates/core/src/governor/paths.rs`, replace the doc comment and signature of `normalise` with:

  ```rust
  /// The path with backslashes as `/` and `.` segments dropped, or `None` when it is empty,
  /// absolute (a leading separator or a drive letter), or has a `..` segment.
  ///
  /// Public because the same question is asked twice: here, of a path a change touched, and in
  /// `farik-store`'s file adapter, of a path a tool call wants to write under `.farik/product/`. Two
  /// answers to "does this path climb out" would be two definitions of a safe path.
  #[must_use]
  pub fn normalise(path: &str) -> Option<String> {
  ```

- [ ] Write the minimal implementation. In `crates/store/src/files.rs`, add after the `farik_core::criteria` import line:

  ```rust
  use farik_core::governor::paths::normalise;
  ```

  and add `AgentId` to the team import, which becomes:

  ```rust
  use farik_core::team::{AgentId, Team, validate_team};
  ```

- [ ] Insert into the first `impl ProjectFiles` block, after `list_contracts`:

  ```rust
      /// An agent's own notebook, which is included in every session it runs (5.8).
      ///
      /// An agent that has never written one has an empty notebook rather than no notebook, because
      /// every session includes it and a missing file is not a thing to tell an agent about.
      ///
      /// # Errors
      ///
      /// `Io` when the file is there and cannot be read.
      pub fn read_memory(&self, agent_id: &AgentId) -> Result<String, FilesError> {
          match self.read_text(&memory_path(agent_id)) {
              Err(FilesError::NotFound { .. }) => Ok(String::new()),
              other => other,
          }
      }

      /// Writes an agent's notebook.
      ///
      /// # Errors
      ///
      /// `Io` when the directory cannot be made or the file cannot be written.
      pub fn write_memory(&self, agent_id: &AgentId, text: &str) -> Result<(), FilesError> {
          self.write_text(&memory_path(agent_id), text)
      }

      /// What the project scan read back about this repository (5.8).
      ///
      /// # Errors
      ///
      /// `NotFound` when the project has not been scanned, `Io` when the file cannot be read.
      pub fn read_project_scan(&self) -> Result<String, FilesError> {
          self.read_text(PROJECT_SCAN)
      }

      /// Writes what the project scan read back.
      ///
      /// # Errors
      ///
      /// `Io` when the file cannot be written.
      pub fn write_project_scan(&self, text: &str) -> Result<(), FilesError> {
          self.write_text(PROJECT_SCAN, text)
      }

      /// A product document, by its path under `product/`.
      ///
      /// # Errors
      ///
      /// `Invalid` when the path climbs out of `product/`, `NotFound` when there is no such document,
      /// `Io` when it cannot be read.
      pub fn read_product_doc(&self, path: &str) -> Result<String, FilesError> {
          self.read_text(&product_path(path)?)
      }

      /// Writes a product document.
      ///
      /// # Errors
      ///
      /// `Invalid` when the path climbs out of `product/`, `Io` when it cannot be written.
      pub fn write_product_doc(&self, path: &str, text: &str) -> Result<(), FilesError> {
          self.write_text(&product_path(path)?, text)
      }
  ```

- [ ] Add the scan's line to the layout, which becomes:

  ```rust
  /// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
  /// the whole layout at once and a change to it is one line.
  const TEAM: &str = "team.yaml";
  const CRITERIA: &str = "team/criteria.yaml";
  const PROJECT_SCAN: &str = "project.md";
  ```

- [ ] And insert, after `contract_path`:

  ```rust
  /// The file an agent's notebook lives in. An agent id is a slug the team schema pinned, so this
  /// path cannot climb anywhere.
  fn memory_path(agent_id: &AgentId) -> String {
      format!("agents/{}/memory.md", agent_id.as_str())
  }
  ```

- [ ] Then, after that:

  ```rust
  /// A product document's path, or a refusal when it climbs out of `product/`.
  ///
  /// The path comes from a tool call, so it is a string an agent chose. `farik-core`'s own path rule
  /// is what answers: empty, absolute, or holding a `..` segment is refused, and `.` segments and
  /// backslashes are dropped on the way. Everything else is somewhere under `product/`, which is the
  /// only place 5.6 lets a product document be written.
  fn product_path(path: &str) -> Result<String, FilesError> {
      let normalised = normalise(path).ok_or_else(|| FilesError::Invalid {
          path: format!("product/{path}"),
          detail: "a product document lives under product/, and this path climbs out of it"
              .to_string(),
      })?;
      Ok(format!("product/{normalised}"))
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 49 passed (farik-store)
  #   test result: ok. 15 passed (crates/store/tests/project_files.rs)
  ```

- [ ] Commit: `feat(store): keep a product document under product and nowhere else`

### Task 4: What this machine knows

Files: modified `crates/store/src/files.rs`, `crates/store/tests/project_files.rs`, `docs/plans/phase-2-protocol-store-cli/step-06-project-files.md`

Consumes: everything above
Produces: `farik_store::files::{LocalSettings, Sandbox}` and `ProjectFiles::{read_prices, read_settings, write_settings}`

- [ ] Write the failing tests. Replace `crates/store/tests/project_files.rs`, whole, with:

  ```rust
  //! The files under `.farik/`, against a real directory.
  //!
  //! Every test here needs somewhere to write and nothing else, so they run in the default
  //! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

  use farik_core::contract::{TaskId, validate_contract};
  use farik_core::criteria::{fixtures::a_criteria_library_wire, validate_criteria};
  use farik_core::team::AgentId;
  use farik_store::files::fixtures::{TempProject, a_team};
  use farik_store::files::{FilesError, LocalSettings, Sandbox};

  /// The contract `farik-core`'s own fixture describes, with the id a test asks for.
  fn a_contract(id: &str) -> farik_core::contract::TaskContract {
      let mut wire = farik_core::contract::fixtures::a_contract_wire();
      wire["id"] = serde_json::json!(id);
      validate_contract(&wire).expect("the fixture is a contract")
  }

  #[test]
  fn makes_the_layout_a_project_starts_with() {
      let project = TempProject::new("init");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      for directory in [
          "team",
          "contracts",
          "agents",
          "decisions",
          "product",
          "local",
      ] {
          assert!(
              project.root.join(".farik").join(directory).is_dir(),
              "{directory} is where a person will look for it"
          );
      }
      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Farik"
      );
      // Without this, every repository Farik touches is dirty for good: a task's worktree and the
      // event log both live under .farik/local/ (5.14, 8.4).
      assert_eq!(
          std::fs::read_to_string(project.root.join(".farik/local/.gitignore"))
              .expect("the ignore file"),
          "*\n"
      );
  }

  #[test]
  fn a_second_init_takes_nothing_away() {
      let project = TempProject::new("init-twice");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      let mut changed = a_team();
      changed.name = "Renamed by hand".parse().expect("a name");
      files.write_team(&changed).expect("the team is written");
      files.init(&a_team()).expect("init runs again");

      assert_eq!(
          files
              .read_team()
              .expect("the team reads back")
              .name
              .as_str(),
          "Renamed by hand",
          "a second init is a command with nothing to do, not one that throws the team away"
      );
  }

  #[test]
  fn reads_back_the_team_it_wrote() {
      let project = TempProject::new("round-trip-team");
      let files = project.files();
      files.write_team(&a_team()).expect("the team is written");

      assert_eq!(files.read_team().expect("the team"), a_team());
      // YAML, because a person edits this by hand and a person does not edit JSON by hand.
      let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
      assert!(text.starts_with("agents:"), "{text}");
  }

  #[test]
  fn reads_back_the_criterion_library_it_wrote() {
      let project = TempProject::new("round-trip-criteria");
      let files = project.files();
      let library = validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library");
      files
          .write_criteria(&library)
          .expect("the library is written");

      assert_eq!(files.read_criteria().expect("the library"), library);
      assert!(
          project.root.join(".farik/team/criteria.yaml").is_file(),
          "beside the team's other files, which is where 5.13 puts it"
      );
  }

  #[test]
  fn says_there_is_no_team_when_there_is_none() {
      let project = TempProject::new("no-team");
      assert_eq!(
          project.files().read_team(),
          Err(FilesError::NotFound {
              path: ".farik/team.yaml".to_string(),
          })
      );
  }

  #[test]
  fn refuses_a_team_file_a_person_broke() {
      let project = TempProject::new("broken-team");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");

      // A file read back is held to what a team on the wire is held to: the same rule, the same
      // words, whichever door it came through.
      std::fs::write(
          project.root.join(".farik/team.yaml"),
          "name: Farik\nagents: []\nbudgets:\n  daily_usd: 20\npolicy:\n  human_accepts_contracts: high_risk\n  wip_limit_per_agent: 2\n  blocked_limit_hours: 24\n  max_iterations: 3\n  integration: manual\nrules: {}\n",
      )
      .expect("the file is written");
      let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
          panic!("a team of nobody is not a team");
      };
      assert_eq!(path, ".farik/team.yaml");
      assert!(detail.contains("/agents"), "{detail}");

      std::fs::write(project.root.join(".farik/team.yaml"), "name: [\n").expect("the file");
      let Err(FilesError::Invalid { detail, .. }) = files.read_team() else {
          panic!("this is not YAML at all");
      };
      assert!(!detail.is_empty(), "the parser's own words");
  }

  #[test]
  fn leaves_nothing_half_written() {
      // A write goes to a file beside the one being written and is renamed over it, because a rename
      // within a directory is the one file operation that is all or nothing. A crash in the middle of
      // team.yaml would otherwise leave the team unreadable, and the team is what every session is
      // assembled from.
      let project = TempProject::new("atomic");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");
      files.write_team(&a_team()).expect("and written again");

      let left_behind: Vec<String> = std::fs::read_dir(project.root.join(".farik"))
          .expect("the directory reads")
          .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
          .filter_map(|name| name.to_str().map(std::string::ToString::to_string))
          .filter(|name| name.contains("writing"))
          .collect();
      assert!(left_behind.is_empty(), "{left_behind:?}");
  }

  #[test]
  fn refuses_to_write_a_team_that_could_not_be_read_back() {
      // The round trip is the promise: what write_team accepts, read_team returns. A value built in
      // memory that breaks a rule of the file is refused here rather than becoming a file nothing
      // can open.
      let project = TempProject::new("write-invalid");
      let files = project.files();
      let mut team = a_team();
      team.agents.truncate(1);
      let refused = files.write_team(&team);
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(!project.root.join(".farik/team.yaml").exists());
  }

  #[test]
  fn writes_a_contract_to_the_file_its_own_id_names() {
      let project = TempProject::new("contracts");
      let files = project.files();
      let contract = a_contract("FRK-7");
      files.write_contract(&contract).expect("it is written");

      assert!(project.root.join(".farik/contracts/FRK-7.yaml").is_file());
      let id = TaskId::try_from("FRK-7").expect("an id");
      assert_eq!(files.read_contract(&id).expect("it reads back"), contract);
  }

  #[test]
  fn refuses_a_contract_that_says_it_is_another() {
      // The file name and the id inside it are two claims about the same thing, and a board that
      // believed the file name would show a task that does not exist.
      let project = TempProject::new("contract-id");
      let files = project.files();
      files.write_contract(&a_contract("FRK-7")).expect("written");
      std::fs::rename(
          project.root.join(".farik/contracts/FRK-7.yaml"),
          project.root.join(".farik/contracts/FRK-8.yaml"),
      )
      .expect("a person moves it");

      let id = TaskId::try_from("FRK-8").expect("an id");
      let Err(FilesError::Invalid { path, detail }) = files.read_contract(&id) else {
          panic!("the contract inside says FRK-7");
      };
      assert_eq!(path, ".farik/contracts/FRK-8.yaml");
      assert!(detail.contains("says it is FRK-7"), "{detail}");
  }

  #[test]
  fn lists_contracts_in_the_order_a_board_shows_them() {
      let project = TempProject::new("list");
      let files = project.files();
      for id in ["FRK-10", "FRK-2", "FRK-1"] {
          files.write_contract(&a_contract(id)).expect("written");
      }
      // A person's own notes in the same directory are not contracts and are not a problem either.
      std::fs::write(project.root.join(".farik/contracts/notes.md"), "mine\n").expect("a note");
      std::fs::write(project.root.join(".farik/contracts/FRK-3.txt"), "?\n").expect("not a contract");

      assert_eq!(
          files
              .list_contracts()
              .expect("they list")
              .iter()
              .map(|id| id.as_str().to_string())
              .collect::<Vec<_>>(),
          ["FRK-1", "FRK-2", "FRK-10"],
          "by the number in the id, so the tenth does not come before the ninth"
      );
  }

  #[test]
  fn a_project_with_nothing_in_it_has_no_contracts() {
      assert!(
          TempProject::new("list-empty")
              .files()
              .list_contracts()
              .expect("an empty list, not a refusal")
              .is_empty()
      );
  }

  #[test]
  fn an_agent_that_never_wrote_a_notebook_has_an_empty_one() {
      // Every session for an agent includes its notebook (5.8). A missing file is not something to
      // tell an agent about; it is an agent that has not written anything down yet.
      let project = TempProject::new("memory");
      let files = project.files();
      let ada = AgentId::try_from("ada").expect("an id");
      assert_eq!(files.read_memory(&ada).expect("an empty notebook"), "");

      files
          .write_memory(&ada, "The login form is in src/login.\n")
          .expect("it is written");
      assert_eq!(
          files.read_memory(&ada).expect("it reads back"),
          "The login form is in src/login.\n"
      );
  }

  #[test]
  fn reads_back_the_project_scan_and_says_when_there_is_none() {
      let project = TempProject::new("scan");
      let files = project.files();
      assert_eq!(
          files.read_project_scan(),
          Err(FilesError::NotFound {
              path: ".farik/project.md".to_string(),
          })
      );
      files
          .write_project_scan("# Farik\n\nA Rust workspace.\n")
          .expect("it is written");
      assert!(
          files
              .read_project_scan()
              .expect("it reads back")
              .contains("A Rust workspace")
      );
  }

  #[test]
  fn writes_a_product_document_and_refuses_one_that_climbs_out() {
      let project = TempProject::new("product");
      let files = project.files();
      files
          .write_product_doc("areas/login.md", "# Login\n")
          .expect("it is written");
      assert!(project.root.join(".farik/product/areas/login.md").is_file());
      assert_eq!(
          files
              .read_product_doc("areas/login.md")
              .expect("it reads back"),
          "# Login\n"
      );

      let refused = files.write_product_doc("../team.yaml", "not here\n");
      assert!(
          matches!(refused, Err(FilesError::Invalid { .. })),
          "{refused:?}"
      );
      assert!(
          !project.root.join(".farik/team.yaml").exists(),
          "and nothing was written where it pointed"
      );
  }

  #[test]
  fn reads_no_prices_when_a_project_does_not_override_them() {
      let project = TempProject::new("prices");
      let files = project.files();
      files.init(&a_team()).expect("a project is made");
      assert_eq!(
          files.read_prices().expect("no override is not a refusal"),
          None
      );

      let shipped = farik_core::pricing::prices::PRICES_JSON;
      std::fs::write(project.root.join(".farik/prices.json"), shipped).expect("an override");
      assert!(
          files
              .read_prices()
              .expect("it reads back")
              .is_some_and(|table| table.version.get() == 1)
      );

      std::fs::write(
          project.root.join(".farik/prices.json"),
          "{\"version\": 1}\n",
      )
      .expect("broken");
      let Err(FilesError::Invalid { path, .. }) = files.read_prices() else {
          panic!("half a price table is not one");
      };
      assert_eq!(path, ".farik/prices.json");
  }

  #[test]
  fn a_machine_that_was_never_asked_runs_the_sandbox_the_spec_asks_for() {
      let project = TempProject::new("settings");
      let files = project.files();
      assert_eq!(
          files.read_settings().expect("the defaults"),
          LocalSettings {
              sandbox: Sandbox::Docker,
          }
      );

      files
          .write_settings(&LocalSettings {
              sandbox: Sandbox::None,
          })
          .expect("it is written");
      assert_eq!(
          files.read_settings().expect("it reads back").sandbox,
          Sandbox::None
      );
      assert_eq!(
          std::fs::read_to_string(project.root.join(".farik/local/settings.json")).expect("the file"),
          "{\n  \"sandbox\": \"none\"\n}\n",
          "snake_case on the wire, and a file a person can read"
      );
  }
  ```

- [ ] Run them and confirm they fail because this machine has nothing to say yet:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, with one error per call site and nothing else:
  # error[E0432]: unresolved imports `farik_store::files::LocalSettings`,
  #   `farik_store::files::Sandbox`
  # error[E0599]: no method named `read_prices` found for struct `ProjectFiles` in the
  #   current scope  (three times)
  # error[E0599]: no method named `read_settings` found for struct `ProjectFiles` in the
  #   current scope  (twice)
  # error[E0599]: no method named `write_settings` found for struct `ProjectFiles` in the
  #   current scope
  # error: could not compile `farik-store` (test "project_files") due to 7 previous errors
  ```

- [ ] Write the minimal implementation. Replace the imports of `crates/store/src/files.rs`, whole, with:

  ```rust
  //! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
  //!
  //! The event log is the source of truth for what happened; these files are the source of truth for
  //! what the team knows. They are what travels with the repository, so every one of them is text a
  //! person can read, diff and edit — and every structured one is held, when it is read back, to
  //! exactly the rules it would be held to arriving on the wire.

  use std::fmt;
  use std::path::{Path, PathBuf};

  use farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract};
  use farik_core::criteria::{CriteriaLibrary, validate_criteria};
  use farik_core::governor::paths::normalise;
  use farik_core::pricing::{PriceTable, validate_price_table};
  use farik_core::team::{AgentId, Team, validate_team};
  use serde::{Deserialize, Serialize};
  use serde_json::Value;

  /// Wire fixtures and a temporary project, for tests in this crate and in others.
  pub mod fixtures;
  ```

- [ ] Insert, between the error and `pub struct ProjectFiles`:

  ```rust
  /// Where the sandbox runs (`docs/SPEC.md` section 8.3). Machine-local, because one person's laptop
  /// has Docker and another's does not, and that is not a fact about the project.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Sandbox {
      /// One container per task, which is what the spec asks for.
      Docker,
      /// No sandbox: every task runs on the machine itself.
      None,
  }

  /// What this machine knows that the repository does not.
  #[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
  pub struct LocalSettings {
      /// Where a task's session runs.
      pub sandbox: Sandbox,
  }

  impl Default for LocalSettings {
      fn default() -> Self {
          Self {
              sandbox: Sandbox::Docker,
          }
      }
  }
  ```

- [ ] Insert into the first `impl ProjectFiles` block, after `write_product_doc`:

  ```rust
      /// The price table this project overrides the shipped one with, or nothing when it does not.
      ///
      /// # Errors
      ///
      /// `Invalid` when the file is there and is not a price table, `Io` when it cannot be read.
      pub fn read_prices(&self) -> Result<Option<PriceTable>, FilesError> {
          let text = match self.read_text(PRICES) {
              Err(FilesError::NotFound { .. }) => return Ok(None),
              other => other?,
          };
          let value: Value = serde_json::from_str(&text).map_err(|error| FilesError::Invalid {
              path: PRICES.to_string(),
              detail: error.to_string(),
          })?;
          validate_price_table(&value)
              .map(Some)
              .map_err(|errors| self.refused(PRICES, &errors))
      }

      /// What this machine knows, or the defaults when it has not been asked.
      ///
      /// # Errors
      ///
      /// `Invalid` when the file is there and is not settings, `Io` when it cannot be read.
      pub fn read_settings(&self) -> Result<LocalSettings, FilesError> {
          let text = match self.read_text(SETTINGS) {
              Err(FilesError::NotFound { .. }) => return Ok(LocalSettings::default()),
              other => other?,
          };
          serde_json::from_str(&text).map_err(|error| FilesError::Invalid {
              path: SETTINGS.to_string(),
              detail: error.to_string(),
          })
      }

      /// Writes what this machine knows.
      ///
      /// # Errors
      ///
      /// `Io` when the file cannot be written.
      pub fn write_settings(&self, settings: &LocalSettings) -> Result<(), FilesError> {
          let text = serde_json::to_string_pretty(settings).map_err(|error| FilesError::Invalid {
              path: SETTINGS.to_string(),
              detail: error.to_string(),
          })?;
          self.write_text(SETTINGS, &format!("{text}\n"))
      }
  ```

- [ ] And complete the layout, which becomes:

  ```rust
  /// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
  /// the whole layout at once and a change to it is one line.
  const TEAM: &str = "team.yaml";
  const CRITERIA: &str = "team/criteria.yaml";
  const PROJECT_SCAN: &str = "project.md";
  const PRICES: &str = "prices.json";
  const SETTINGS: &str = "local/settings.json";
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 49 passed (farik-store)
  #   test result: ok. 17 passed (crates/store/tests/project_files.rs)
  ```

- [ ] Commit: `feat(store): read a price override and the local settings`

### Task 5: The plans say what the step became

Files: modified `docs/plans/project-plan.md`, `docs/plans/phase-2-protocol-store-cli/step-06-project-files.md`

Consumes: everything above
Produces: a project plan that describes the file adapter as it is

This task changes documentation and has no test cycle. The `> ` marker on the block below is this plan's and is not part of the text to write.

- [ ] In `docs/plans/project-plan.md`, replace the phase 2 line beginning `- Step 06 (\`farik-store::files\`):` — everything up to and including `never sees a file.` — with:

  > - Step 06 (`farik-store::files`): `enum FilesError { NotFound { path }, Invalid { path, detail }, Io { path, detail } }`; `struct ProjectFiles { root: PathBuf }` with `open` and `root`; `impl ProjectFiles { fn init(&self, team: &Team) -> Result<(), FilesError>; fn read_team / write_team; fn read_criteria / write_criteria; fn read_contract(&self, id: &TaskId) (through `validate_contract`, and refused when the contract inside names another id) / write_contract / list_contracts (ordered by the number in the id; a file that is not a contract's is passed over); fn read_memory(&self, agent_id: &AgentId) (an agent that never wrote one has an empty notebook, not a missing file) / write_memory; fn read_project_scan / write_project_scan; fn read_product_doc(&self, path: &str) / write_product_doc (under `product/` only, through `farik-core`'s own path rule); fn read_prices(&self) -> Result<Option<PriceTable>, FilesError>; fn read_settings / write_settings }`; `struct LocalSettings { sandbox: Sandbox }` with `enum Sandbox { Docker, None }`, JSON under `.farik/local/` because nobody hand-edits it and it is never committed. Every structured file is held, read and written, to the validator that owns it, so the round trip is a promise: what a writer accepts, the reader returns. A write goes to a file beside the one being written and is renamed over it, because a rename within a directory is the one file operation that is all or nothing. `init` makes the layout, never overwrites what is there, and writes `.farik/local/.gitignore` holding `*`. YAML is read and written by `yaml_serde` (ADR 0007), which is this crate's one new dependency; `farik-core` gains none, because it does no I/O and never sees a file. Added 2026-09-17 by the step 06 plan: `farik-core` exports `AgentId`, and `governor::paths::normalise` is public, because a product document's path comes from a tool call and two answers to "does this path climb out" would be two definitions of a safe path.

- [ ] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [ ] Commit: `docs(docs): record what step 06 changed about the files`

## Verification

- [ ] The whole check, from the workspace root:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 264 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 49 passed (farik-store)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 17 passed (crates/store/tests/project_files.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] `farik-core` still performs no I/O, which this step leans on rather than changes:

  ```
  cargo xtask core-io
  # expected: silent
  ```

- [ ] The one new dependency is the one ADR 0007 chose, pinned:

  ```
  grep yaml Cargo.toml crates/store/Cargo.toml
  # expected: the pinned version in the workspace, and the workspace line in the store
  ```

- [ ] Every commit subject is accepted:

  ```
  for subject in \\
    "feat(store): make the files a project starts with" \\
    "feat(store): read a contract back from the file its id names" \\
    "feat(store): keep a product document under product and nowhere else" \\
    "feat(store): read a price override and the local settings" \\
    "docs(docs): record what step 06 changed about the files"; do
    printf '%s\\n' "$subject" > /tmp/subject && cargo xtask commit-msg /tmp/subject
  done
  # expected: silent, five times
  ```

## Open questions

none
