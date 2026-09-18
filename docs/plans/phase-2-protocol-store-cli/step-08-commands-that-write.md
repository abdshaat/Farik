# Phase 2, step 08: the command line, and the commands that write

Status: draft
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned branch this phase is on; steps do not get their own)
Spec: `docs/SPEC.md` sections 5.11 and 5.16, F2, F3
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), and steps 01 to 07 of this phase (committed as `72a6d4c`, `b128d9a`, `5defee2`, `e1551d8`, `46be1ed`, `bb7b7de`, `b745067`, `ef0cdcf`)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: <name>, <date>

## Goal

A person with a git repository and no Farik can run `farik init` and have a project: `.farik/` with a
team, a criterion library seeded from what the repository says about itself, `project.md` holding the
line section 4 shows, and an event log holding the three events that say so. They can then write a
contract in YAML, file it with `farik task create`, size it with `farik triage`, and take it from the
team with `farik contract lock`. Every one of those writes a file and an event, and every refusal
names the rule in `docs/SPEC.md` that decided it. Nothing reads the board yet — that is step 09 — and
no agent runs, which is phase 3.

## Decisions

- **The binary is the `farik` package in `crates/cli`, with a library beside it.** `docs/standards/code.md` names the binary crate `farik` in `crates/cli`; `[lib]` and `[[bin]]` both carry that name. Every command is a function in the library and `main.rs` is the twenty lines that build `CliIo` from the process: chose this over a binary-only crate because a test that spawns a process cannot read what a command printed without a temporary file, and cannot inject a clock at all.
- **`CliIo` borrows its streams: `Box<dyn Write + 'a>`.** `docs/plans/project-plan.md` records `Box<dyn Write>`, which is `Box<dyn Write + 'static>` and cannot hold `&mut Vec<u8>`; measured, it fails to compile with `` `out` does not live long enough ``. Chose a lifetime parameter on `CliIo` over a shared `Arc<Mutex<Vec<u8>>>` writer in the tests, because the buffer a test reads back should be the test's own value and the lifetime costs one `'_` in three signatures. The project plan's interface line is corrected in the same commit as this step's last.
- **Exit codes are 0, 1 and 2.** 0 when the command did what it said, 1 when Farik refused, 2 when the command line itself was wrong. Chose 2 for a wrong invocation because that is what clap exits with and the shape of a wrong invocation is not ours to redefine; `--help` and `--version` are what a person asked for, so they print to stdout and exit 0 although clap reports them as errors.
- **A refusal goes to stderr, prefixed `farik: `, and a `--json` refusal goes to stderr as `{"error": "..."}`.** Chose printing JSON for the failure too over English on stderr with JSON on stdout, because a script that asked for JSON should not have to parse English to find out why a run failed.
- **Every command works from anywhere inside the repository.** The project is found by asking git for the top level (`Git::top_level`, step 07), not by looking for `.farik/` in the current directory: a project is a whole repository (`docs/SPEC.md` section 3), so `farik init` run in `src/` initialises the repository, not `src/`. Chose this over taking a path argument because `cd` is how a person chooses a repository and a path argument would need the same refusals twice.
- **`team_id` and `project_id` are fixed by `farik init` and read back from the log's first event.** `init` derives them as the slug of the team's name and the slug of the repository root's directory name; every command after it reads the ids off the first event in the log. Chose this over recomputing them per run (renaming the directory would split one project's log in two) and over a field in `.farik/local/settings.json` (machine-specific and gitignored is the wrong home for a project's identity, and the log is already the source of truth for what happened — 8.4).
- **A slug with nothing in it is `farik`.** A team called `プロジェクト` slugs to nothing, and `new_event` refuses a blank id. Chose a fallback over refusing the name, because the name is the person's and the id is Farik's problem.
- **`farik init` writes a starter team of two agents when there is none.** `validate_team` wants two to seven agents with an active Product Manager and an active Software Developer (D18), and there is no team editor until phase 5, so a project with no team could not be initialised at all. They are called `Product Manager` and `Developer`, ids `product-manager` and `developer`: chose naming them after their roles over inventing human names, because the person renames them in the editor and a fake name in a file a person reads is a small lie. Both run `claude-opus-5` at effort `high`, which is what `docs/SPEC.md` 8.2 ships as the default for the Product Manager, the Architect and the Developer, and both ids are rows of the shipped price table. The policy it writes is the spec's own numbers — `blocked_limit_hours: 24` and `max_iterations: 3` (5.2), `integration: manual` (5.14), `daily_usd: 20` (5.5), `human_accepts_contracts: high_risk` (the default `team.schema.json` names) — except `wip_limit_per_agent`, which neither the spec nor the schema gives a default for: it is `1`, the number 6.2 calls the default, so a two-agent team holds two open tasks and a person meets the limit rather than discovering it later. `budgets.session` is left out, which means the session limits `farik-core` ships (5.5) rather than a second copy of them in every project's team file.
- **A second `farik init` is a rescan.** It keeps the team and everything a person wrote, reads the repository again, and replaces the criteria the last scan found while keeping the ones a person added (`seeded_library`, step 07). It records `project.scanned` and `criteria.updated` and records `team.updated` only when it wrote the team. Chose this over refusing a second run, because the command a person reaches for when the project's test command changes is this one.
- **A file that is there and cannot be read is not a file that is absent.** `init` reads the team and the criterion library through one helper that answers `None` only for `FilesError::NotFound` and refuses on anything else. Chose refusing over `.ok()`: `ProjectFiles::init` leaves a broken team file where it is and answers `Ok`, so a run that swallowed the difference would print "wrote .farik/team.yaml", record a `team.updated` naming two agents that are not in the file, and leave the file broken — and the log is append-only, so that record could never be taken back. `seeded_library` keeps only the criteria it is handed, so the same mistake on `.farik/team/criteria.yaml` — the file 5.13 most expects a person to hand-edit — would delete every criterion they wrote because one line has a typo in it. Both are measured in tests of their own.
- **`farik init` refuses a directory that is not a git repository rather than running `git init`.** F2 gives "create a new one" to the app's project flow; one command that quietly makes a repository is a command that can make one in the wrong place. The refusal says to run `git init` first.
- **`farik task create` refuses a file that sets a field that is not the author's**, naming them, rather than overwriting: the store's four (`id`, `created_by`, `created_at`, `updated_at`), the governor's five, the human's `locked`, and the two fixed at creation (`kind`, `parent`). The list is composed from `farik-core`'s public constants, so there is one list of who writes what. Chose refusing over overwriting because a command that silently replaced what somebody wrote would make the file and the filed contract two different things; chose it over accepting a person's `kind` because 5.16 gives the kind to the triage.
- **`farik task create` refuses `parent`, which closes 5.16 item 3's human route until phase 3.** 5.16 item 3 says a task under an epic may be created by its epic's assignee *or by the human*, and `check_child_creation` in `farik-core` is the gate for it. Asking that gate needs the epic's `assignee_id`, and `TaskProjection` (step 03) does not carry one, so asking it here would be a forward dependency — and filing a child without asking it would put a task under an epic with no gate at all. So the command refuses `parent`, the refusal says which rule refuses it and what will open it, and the human's route into a breakdown waits for phase 3 step 03. Recorded in "out of scope" below rather than left to be noticed.
- **A filed request is one event, `task.created`.** Its body already carries the summary the board needs, and step 03's projections insert the row from it. Chose one event over `task.created` plus `contract.written`, because two events about one act would count the same write twice in the audit (F11) and in the cost report.
- **`farik triage` writes the contract's `kind` as well as the event.** 5.11 says the kind is fixed at creation *and* that the triage's own tool changes it; step 03's projections already take the kind from `request.triaged`. Writing the file too keeps the file and the board saying the same thing, which matters because `reconcile` compares only `status` and `locked` and would not report this.
- **A task id a person typed wrongly is the command line's refusal, not the schema's.** `farik triage` parses the id before it builds the command, so that a person reads `nine is not a task id` rather than the `oneOf` sentence a JSON Schema refuses a whole body with. The command is still built and validated afterwards, so a triage from a terminal is held to exactly the rules one from an agent is; the schema's own refusal for `task_id` is then unreachable, which is the point. `farik contract lock` already parses first, and step 09's `doctor` carries the same complaint about `criteria.yaml` on the project plan.
- **Whether the human may triage is a predicate in `farik-core`, `check_human_triage(status, has_parent)`.** 5.16 gives the user the size "until refining starts", and a task under an epic was never triaged. Chose adding the predicate to `governor::gates` over deciding it in the command line, because a rule of section 5 that lives in a binary is a rule the daemon and the app will each decide again. It is the human's gate only: the Product Manager's re-triage of a `refining` standalone task is the one tool gate of section 5 with no predicate in `core`, and `docs/plans/project-plan.md` records that it needs a decision in the spec first, so this step does not close it.
- **Whether a contract may be locked is `check_contract_write`'s answer**, asked with `locked` as the only changed field and the human as the actor. Chose asking the existing gate over a new one: it already says that a task which is `accepted` or `cancelled` takes no write but a note, and that the lock is the human's field. The status comes from the log, which decides a task's status (8.4); the kind and the lock come from the file, which decides a contract's content (8.4). On a project where the two disagree — which is what `farik doctor` is for — the governor is asked about the status the log knows, because that is the status the transition table answers for. The gate is asked *before* the command notices that the contract is already held, so that a task nothing can be written to says so rather than answering about the lock; "already yours" is a sentence about what the person wants and the rule comes first.
- **A path a person typed is relative to where they typed it.** `farik task create` resolves a relative file argument against `CliIo.cwd`, not against the process's own current directory and not against the repository root. Chose threading `cwd` into the command over reading through the process, because the whole reason the command is a library function is that a test can run it from a directory of its own, and a path that only worked in the binary would leave that seam with a hole in it.
- **The governor's refusals become English in `crates/cli/src/refusal.rs`.** `farik-core` answers with values, and of its eight refusal and error enums only `CriteriaError` implements `Display` (`farik-store`'s do, because they carry what the operating system and git said); chose keeping that convention and putting the command line's words in one module of its own over adding `Display` to `ContractWriteRefusal`, because the daemon (phase 3) and the app (phase 5) will each say them their own way, and a `match` with no fallback arm means a variant added to the governor is a compilation error here rather than a Rust value in somebody's terminal.
- **The one YAML reader becomes public: `farik_store::files::yaml_value(text, named)`.** A contract handed to `farik task create` comes from anywhere in the repository and must be held to the same dialect as the files under `.farik/` — no duplicate mapping key, no second document, the alias budget, `true` spelled `true` (ADR 0007). Chose lifting the store's private `read_yaml` body into a public function over giving the command line its own `serde-saphyr` dependency, which would be a second place where the dialect is decided.
- **Out of scope for this step**: filing a task under an epic (`parent`), which waits for a projection that carries the epic's assignee — phase 3 step 03; every command that reads (`task show`, `board`, `log`, `doctor`, `rules show`, `criteria list`) — step 09; any transition (`farik task start` and the rest) — phase 3's runtime is what asks the governor for one; editing the team or the criterion library from the command line (F15, F16 beyond listing) — phase 5; `--json` for step 09's tables.

## Design

`crates/cli/src/lib.rs` holds `CliIo`, `Report`, `run_cli` and the clap tree, and nothing else: it
parses, picks a function, and writes what that function answered. Each command is a module —
`init.rs`, `task.rs`, `triage.rs`, `contract.rs` — whose one public function takes what it needs and
answers `Result<Report, String>`, where the string is the sentence a person reads. `project.rs` holds
the project a command runs against and `refusal.rs` the governor's refusals in words.

A command never prints. It builds both a list of lines and a JSON object and hands them back, and
`run_cli` writes one or the other, so a command that told a person one thing and a script another
would have to be two commands.

Everything a command decides that is a rule of `docs/SPEC.md` section 5 is asked of `farik-core`:
`check_human_triage` for a triage, `check_contract_write` for a lock, `validate_contract` (through
`command_from_value`) for a filed contract, `validate_team` for the team `init` writes. What the
command line decides for itself is only what a command line decides: where the project is, what a
person may leave out of a file, and how a refusal reads.

Out of scope: reading the board, transitions, agents, and anything that runs a model.

## Architecture notes

`crates/cli` is new. It depends on `farik-core` (the rules), `farik-protocol` (the events, the
commands, `Clock`), `farik-store` (the log, the projections, the files, git), `clap` and
`serde_json`, and on nothing else; `farik-store` is added to the workspace's dependency table for the
first time, as is `clap`. The crate is a leaf: nothing depends on it.

Two crates change. `farik-core` gains one pure function, `governor::gates::check_human_triage`, which
does no I/O and is tested in `core` beside the other gates. `farik-store` makes one private helper
public, `files::yaml_value`, and `read_yaml` is rewritten to call it, so the dialect stays decided in
one place.

Consumed as they are on the phase branch: `Git::{open, top_level}` and `scan_project`,
`seeded_library` (step 07); `ProjectFiles::{open, init, read_team, write_team, read_criteria,
write_criteria, read_contract, write_contract, list_contracts, read_project_scan,
write_project_scan}` (step 06); `open_event_log`, `EventLog::{append, read, next_task_id}`,
`EventQuery` (step 02); `open_projections`, `Projections::task` (step 03); `validate_team`,
`CriteriaLibrary` (step 05); `new_event`, `EventIds`, `EventBody`, the generated bodies,
`command_from_value`, `Command`, `RequestSize`, `Clock`, `FixedClock` (step 01); `check_contract_write`,
`ContractWriteActor`, `ContractWriteRefusal`, the four field lists `task create` composes its refusal
from, `TransitionActor` (phase 1).

## Global constraints

- `farik-core` does no I/O. The function this step adds to it is pure; `cargo xtask core-io` checks it.
- Every event kind is `<entity>.<past_tense_verb>` and already in `event.schema.json`; this step adds none.
- Wire and file formats are `snake_case`. The JSON `--json` prints is a report, not a wire format, and its keys are `snake_case` too.
- No `unwrap` or `expect` outside tests and `LazyLock` initialisers, and none without a message saying why it cannot fail.
- Every test that needs the `git` program is `#[ignore]`d with the same reason string the store's tests use, and run by `cargo xtask check --integration`.
- One task, one commit, Conventional Commits with the scope of the crate the commit is about.

## File map

```
Cargo.toml                                  modifies: clap 4.6.7 (derive) and farik-store in the workspace dependency table
Cargo.lock                                  modifies: clap and its dependencies
crates/cli/Cargo.toml                       creates: the farik package, its library and its binary
crates/cli/src/main.rs                      creates: the process — the real clock, the real streams, the exit code
crates/cli/src/lib.rs                       creates: CliIo, Report, run_cli, the clap tree, the exit codes
crates/cli/src/project.rs                   creates: the project a command runs against, its ids, and the slug they are spelled with
crates/cli/src/refusal.rs                   creates: the governor's refusals in the words a person reads
crates/cli/src/init.rs                      creates: farik init, and the team a project starts with
crates/cli/src/task.rs                      creates: farik task create
crates/cli/src/triage.rs                    creates: farik triage
crates/cli/src/contract.rs                  creates: farik contract lock and farik contract unlock
crates/cli/tests/commands.rs                creates: every command against a real repository
crates/core/src/governor/gates.rs           modifies: check_human_triage, and its three tests
crates/store/src/files.rs                   modifies: yaml_value made public; read_yaml calls it
crates/store/tests/project_files.rs         modifies: one test that the dialect is the same outside `.farik/`
docs/SPEC.md                                modifies: what farik init writes, in section 3
docs/plans/project-plan.md                  modifies: CliIo's lifetime, check_human_triage, yaml_value
docs/plans/phase-2-protocol-store-cli/step-08-commands-that-write.md  modifies: this plan's boxes
```

## Tasks

Each task below is one commit. Task 1 is larger than the rest on purpose: a command line cannot be
tested before the crate that holds it exists, so the smallest thing this step can prove is "the crate
is there and its first command works". Every red below was measured by rebuilding the step task by
task in a scratch copy of this branch's head, and every code block is the text that rebuild ended
with, byte for byte.

Where a task says a block **replaces** text, the text it replaces is given too: an executor that
cannot find it should stop rather than guess.

### Task 1: the crate, and `farik init`

Files: created `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`, `crates/cli/src/lib.rs`,
`crates/cli/src/project.rs`, `crates/cli/src/refusal.rs`, `crates/cli/src/init.rs`,
`crates/cli/tests/commands.rs`; modified `Cargo.toml`, `Cargo.lock`

Consumes: `Git::{open, top_level}`, `scan_project`, `seeded_library`, `ProjectScan` from
`crates/store/src/{git.rs,scan.rs}`; `ProjectFiles::{open, init, read_team, read_criteria,
write_criteria, write_project_scan}` and `FilesError` from `crates/store/src/files.rs`;
`open_event_log`, `EventLog::{append, read}`, `EventQuery` from `crates/store/src/event_log.rs`;
`validate_team`, `Team` from `crates/core/src/team.rs`; `CriteriaLibrary`, `CriterionTemplate` from
`crates/core/src/criteria.rs`; `new_event`, `EventIds`, `EventBody`, `EventError`,
`TeamUpdatedBody`, `ProjectScannedBody`, `CriteriaUpdatedBody`, `Clock`, `FixedClock` from
`crates/protocol/src/{event.rs,clock.rs}`; `TempRepo` from `crates/store/src/git/fixtures.rs`
Produces: `run_cli`, `CliIo<'a>`, `Report`, `HUMAN`, `Project`, `ProjectIds`, `repository_root`,
`slug`, `DATABASE`, `refusal::event`, `init::init`

- [ ] Write the failing tests, which are the whole of `crates/cli/tests/commands.rs` for now:

  ```rust
  //! The command line against a real repository.
  //!
  //! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
  //! `cargo xtask check --integration`, as the store's own do and for the same reasons.

  use std::path::Path;

  use chrono::{DateTime, Utc};
  use farik::{CliIo, run_cli};
  use farik_protocol::clock::FixedClock;
  use farik_store::files::ProjectFiles;
  use farik_store::git::fixtures::TempRepo;

  /// The moment every test runs at, so that "last commit today" is an answer rather than a guess.
  fn at() -> DateTime<Utc> {
      Utc::now()
  }

  /// What one run of the command line did.
  struct Ran {
      code: i32,
      out: String,
      err: String,
  }

  /// Runs one command in a repository, as a person standing in `cwd` would.
  fn run_in(cwd: &Path, args: &[&str]) -> Ran {
      let mut out = Vec::new();
      let mut err = Vec::new();
      let code = {
          let mut io = CliIo {
              stdout: Box::new(&mut out),
              stderr: Box::new(&mut err),
              cwd: cwd.to_path_buf(),
              clock: Box::new(FixedClock::new(at())),
          };
          let arguments: Vec<String> = std::iter::once("farik")
              .chain(args.iter().copied())
              .map(ToString::to_string)
              .collect();
          run_cli(&arguments, &mut io)
      };
      Ran {
          code,
          out: String::from_utf8(out).expect("the command line writes text"),
          err: String::from_utf8(err).expect("the command line writes text"),
      }
  }

  /// A repository with a Rust project in it, ready for `farik init`.
  fn a_repository(name: &str) -> TempRepo {
      let repository = TempRepo::new(name);
      repository.write("Cargo.lock", "version = 4\n");
      repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
      repository.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
      repository.commit("a project");
      repository
  }

  /// A repository that is already a Farik project.
  fn a_project(name: &str) -> TempRepo {
      let repository = a_repository(name);
      let ran = run_in(&repository.path, &["init"]);
      assert_eq!(ran.code, 0, "{}", ran.err);
      repository
  }

  fn files_of(repository: &TempRepo) -> ProjectFiles {
      ProjectFiles::open(repository.path.clone())
  }

  /// Every event kind the log holds, in order, so a test says what a command recorded.
  fn kinds_in(repository: &TempRepo) -> Vec<String> {
      let log = farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
          .expect("the log opens");
      log.read(&farik_store::EventQuery::default())
          .expect("the log reads")
          .iter()
          .map(|event| event.body.kind().to_string())
          .collect()
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn makes_a_project_out_of_a_repository() {
      let repository = a_repository("cli-init");
      let ran = run_in(&repository.path, &["init"]);

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert_eq!(
          ran.out.lines().next(),
          Some("Rust, cargo, last commit today"),
          "the first line is the scan read back, which is what section 4 shows a person"
      );
      assert!(
          ran.out
              .contains("wrote .farik/team.yaml: product-manager, developer"),
          "{}",
          ran.out
      );
      assert!(ran.out.contains("the-tests-pass"), "{}", ran.out);

      let files = files_of(&repository);
      let team = files.read_team().expect("a team was written");
      assert_eq!(team.agents.len(), 2);
      assert!(
          files
              .read_project_scan()
              .expect("a scan was written")
              .contains("Rust, cargo, last commit today")
      );
      assert_eq!(
          files
              .read_criteria()
              .expect("a library was written")
              .criteria
              .iter()
              .map(|one| one.name.as_str().to_string())
              .collect::<Vec<_>>(),
          [
              "the-tests-pass",
              "the-build-succeeds",
              "clippy-is-clean",
              "formatting-is-clean"
          ]
      );
      assert_eq!(
          kinds_in(&repository),
          ["team.updated", "project.scanned", "criteria.updated"],
          "one event per thing it wrote, in the order it wrote them"
      );
      assert!(
          repository.path.join(".farik/local/.gitignore").is_file(),
          "the log is local and not committed (D5)"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn a_second_init_rescans_and_keeps_the_team() {
      let repository = a_project("cli-init-again");
      let mut team = files_of(&repository).read_team().expect("a team");
      team.name = "Renamed".parse().expect("a team name");
      files_of(&repository)
          .write_team(&team)
          .expect("the team is written");

      let ran = run_in(&repository.path, &["init"]);

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert!(
          ran.out
              .contains("kept the team already in .farik/team.yaml"),
          "{}",
          ran.out
      );
      assert_eq!(
          files_of(&repository)
              .read_team()
              .expect("a team")
              .name
              .as_str(),
          "Renamed",
          "a second init is a rescan, not a command that throws the team away"
      );
      assert_eq!(
          kinds_in(&repository),
          [
              "team.updated",
              "project.scanned",
              "criteria.updated",
              "project.scanned",
              "criteria.updated"
          ],
          "and it records the scan it did, not a team it did not write"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_rescan_a_project_whose_team_file_cannot_be_read() {
      // A file that is there and broken is not a file that is absent: `ProjectFiles::init` leaves a
      // team file where it is, so a run that treated the two the same would report a team it had not
      // written, and the log would keep that report for good.
      let repository = a_project("cli-init-broken-team");
      repository.write(".farik/team.yaml", "name: one\nagents: []\n");

      let ran = run_in(&repository.path, &["init"]);

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains(".farik/team.yaml"),
          "the refusal names the file: {}",
          ran.err
      );
      assert!(ran.out.is_empty(), "{}", ran.out);
      assert_eq!(
          kinds_in(&repository),
          ["team.updated", "project.scanned", "criteria.updated"],
          "and it recorded nothing this run: the log is append-only and a team.updated naming agents \
           that are not in the file could never be taken back"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_rescan_a_project_whose_criterion_library_cannot_be_read() {
      // `.farik/team/criteria.yaml` is a file 5.13 expects people to hand-edit, and `seeded_library`
      // keeps only the criteria it is handed: reading it as absent would delete every criterion a
      // person had written because one line of it has a typo in it.
      let repository = a_project("cli-init-broken-criteria");
      let broken = "criteria:\n  - name: the-docs-are-updated\n    text: The documents say what changed.\n    source: human\n    verification:\n      method: comand\n";
      repository.write(".farik/team/criteria.yaml", broken);

      let ran = run_in(&repository.path, &["init"]);

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains(".farik/team/criteria.yaml"),
          "the refusal names the file: {}",
          ran.err
      );
      assert_eq!(
          std::fs::read_to_string(repository.path.join(".farik/team/criteria.yaml"))
              .expect("the file is still there"),
          broken,
          "and what the person wrote is still there to fix"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn works_from_any_directory_under_the_repository_root() {
      let repository = a_repository("cli-subdirectory");
      let inside = repository.path.join("src");
      let ran = run_in(&inside, &["init"]);

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert!(
          repository.path.join(".farik/team.yaml").is_file(),
          "a project is a whole repository, so the project is at the root whatever directory the \
           person stood in"
      );
      assert!(
          !inside.join(".farik").exists(),
          "and not a second project in the subdirectory"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_directory_that_is_not_a_repository() {
      let plain = std::env::temp_dir().join(format!("farik-cli-plain-{}", std::process::id()));
      std::fs::create_dir_all(&plain).expect("a plain directory");
      let ran = run_in(&plain, &["init"]);
      let _ = std::fs::remove_dir_all(&plain);

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("is not a git repository") && ran.err.contains("git init"),
          "{}",
          ran.err
      );
      assert!(ran.out.is_empty(), "{}", ran.out);
  }

  #[test]
  fn refuses_an_invocation_it_cannot_use() {
      let ran = run_in(Path::new("."), &["nonsense"]);

      assert_eq!(ran.code, 2, "two is what a wrong command line exits with");
      assert!(ran.err.contains("nonsense"), "{}", ran.err);
      assert!(ran.out.is_empty(), "{}", ran.out);
  }
  ```

- [ ] Run them and confirm they fail because the crate does not exist:

  ```
  cargo test -p farik --test commands
  # expected: exit 101, and
  #   error: failed to load manifest for workspace member `.../crates/cli`
  #   failed to read `.../crates/cli/Cargo.toml`
  #   No such file or directory (os error 2)
  ```

- [ ] Add the two dependencies the workspace does not have yet, in `Cargo.toml`, each pinned with `=`
      as `docs/standards/code.md` asks: `clap` after `chrono`, `farik-store` after `farik-protocol`,
      both in the table's alphabetical order. `Cargo.lock` gains clap and its dependencies, written by
      cargo on the next build:

  ```toml
  clap = { version = "=4.6.7", features = ["derive"] }
  ```

  ```toml
  farik-store = { path = "crates/store" }
  ```

- [ ] Write `crates/cli/Cargo.toml`. The package is `farik` and so are the library and the binary
      (`docs/standards/code.md`); there are no dev-dependencies, because an integration test in a
      package may use that package's own dependencies:

  ```toml
  [package]
  name = "farik"
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  repository.workspace = true
  rust-version.workspace = true

  [lib]
  name = "farik"
  path = "src/lib.rs"

  [[bin]]
  name = "farik"
  path = "src/main.rs"

  [dependencies]
  chrono.workspace = true
  clap.workspace = true
  farik-core.workspace = true
  farik-protocol.workspace = true
  farik-store.workspace = true
  serde_json.workspace = true

  [lints]
  workspace = true
  ```

- [ ] Write `crates/cli/src/main.rs`:

  ```rust
  //! The `farik` command line. Everything it does is in the library beside this file, so that the
  //! tests run a command without spawning a process.

  use std::path::PathBuf;

  use chrono::Utc;
  use farik::{CliIo, run_cli};
  use farik_protocol::clock::Clock;

  /// The wall clock, which is the only thing the binary has that a test does not want.
  struct SystemClock;

  impl Clock for SystemClock {
      fn now(&self) -> chrono::DateTime<Utc> {
          Utc::now()
      }
  }

  fn main() -> std::process::ExitCode {
      let arguments: Vec<String> = std::env::args().collect();
      let mut io = CliIo {
          stdout: Box::new(std::io::stdout()),
          stderr: Box::new(std::io::stderr()),
          cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
          clock: Box::new(SystemClock),
      };
      let code = run_cli(&arguments, &mut io);
      std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
  }
  ```

- [ ] Write `crates/cli/src/lib.rs`. `Commands` holds only `Init`; each task below adds its own
      subcommand, its arm, and whatever enum it needs:

  ```rust
  //! The `farik` command line: the commands the first release ships, and the shape they share.
  //!
  //! Everything a command does is a function in one of the modules below, taking what it needs and
  //! returning either a `Report` — the lines a person reads and the JSON a script reads — or the one
  //! sentence that says why Farik would not do it. `run_cli` parses the arguments, picks the
  //! function, and writes the answer to the streams it was given, so a test runs a command without
  //! spawning a process (`docs/SPEC.md` sections 5.11, 5.16, F2, F3).

  /// Making a repository a Farik project.
  pub mod init;
  /// The project a command runs against.
  pub mod project;
  /// The governor's refusals in words.
  pub mod refusal;

  use std::io::Write;
  use std::path::PathBuf;

  use clap::{Parser, Subcommand};
  use farik_protocol::clock::Clock;
  use serde_json::{Value, json};

  pub use project::Project;

  /// Everything the command line needs from outside itself: where to write, where it is run, and what
  /// time it is.
  ///
  /// The streams are boxed writers rather than `std::io::stdout` so that a test reads what a command
  /// printed, and they borrow for `'a` so that the buffer a test reads back is the test's own
  /// `Vec<u8>`; the clock is injected for the same reason an event's `recorded_at` is (`docs/SPEC.md`
  /// section 8.4).
  pub struct CliIo<'a> {
      /// What a person or a script asked for.
      pub stdout: Box<dyn Write + 'a>,
      /// Why Farik would not do something, and nothing else.
      pub stderr: Box<dyn Write + 'a>,
      /// Where the command was run, which is how the project is found.
      pub cwd: PathBuf,
      /// The time every event this run records is stamped with.
      pub clock: Box<dyn Clock>,
  }

  /// What a command did: the lines a person reads, and the same thing as JSON for `--json`.
  ///
  /// Both are built whatever the flag says, because a command that told a person one thing and a
  /// script another would be two commands.
  pub struct Report {
      /// One line per thing that happened, in the order it happened.
      pub lines: Vec<String>,
      /// The same, as an object a script can read.
      pub json: Value,
  }

  /// The command ran and did what it said.
  const OK: i32 = 0;
  /// Farik refused: a project that is not there, a file that is not a contract, a rule in section 5.
  const REFUSED: i32 = 1;
  /// The command line itself was wrong. Two is what clap exits with, and the shape of a wrong
  /// invocation is not ours to redefine.
  const MISUSE: i32 = 2;

  /// Who the command line acts as. Every command here is run by a person at a terminal, so every
  /// event it records says `human` wrote it; an agent's events come from the runtime (phase 3).
  pub const HUMAN: &str = "human";

  #[derive(Parser)]
  #[command(
      name = "farik",
      version,
      about = "An operating system for a small team of AI agents.",
      long_about = None
  )]
  struct Cli {
      /// Print what happened as JSON rather than as lines a person reads.
      #[arg(long, global = true)]
      json: bool,
      #[command(subcommand)]
      command: Commands,
  }

  #[derive(Subcommand)]
  enum Commands {
      /// Make the repository this is run in a Farik project.
      Init,
  }

  /// Runs one command and returns the code the process should exit with: 0 when it did what it said,
  /// 1 when Farik refused, 2 when the command line itself was wrong.
  ///
  /// `args` is the whole invocation, program name first, as `std::env::args` gives it.
  #[must_use]
  pub fn run_cli(args: &[String], io: &mut CliIo<'_>) -> i32 {
      let parsed = match Cli::try_parse_from(args) {
          Ok(parsed) => parsed,
          Err(error) => return usage(&error, io),
      };
      let now = io.clock.now();
      let outcome = match &parsed.command {
          Commands::Init => init::init(&io.cwd, now),
      };
      report(outcome, parsed.json, io)
  }

  /// Writes what the command did, or why it would not, and answers with the exit code.
  fn report(outcome: Result<Report, String>, as_json: bool, io: &mut CliIo<'_>) -> i32 {
      match outcome {
          Ok(done) => {
              if as_json {
                  say(&mut io.stdout, &format!("{}", done.json));
              } else {
                  for line in &done.lines {
                      say(&mut io.stdout, line);
                  }
              }
              OK
          }
          Err(refusal) => {
              if as_json {
                  say(&mut io.stderr, &format!("{}", json!({ "error": refusal })));
              } else {
                  say(&mut io.stderr, &format!("farik: {refusal}"));
              }
              REFUSED
          }
      }
  }

  /// What clap has to say about an invocation it could not use. `--help` and `--version` are the two
  /// it reports as errors and a person asked for, so they go to stdout and the run succeeded.
  fn usage(error: &clap::Error, io: &mut CliIo<'_>) -> i32 {
      let text = error.render().to_string();
      if error.use_stderr() {
          say(&mut io.stderr, text.trim_end());
          MISUSE
      } else {
          say(&mut io.stdout, text.trim_end());
          OK
      }
  }

  /// One line to a stream. A stream that cannot be written to is a pipe that was closed, which is not
  /// something to tell the person about on the stream that just closed.
  fn say(stream: &mut Box<dyn Write + '_>, line: &str) {
      let _ = writeln!(stream, "{line}");
  }
  ```

- [ ] Write `crates/cli/src/project.rs`. `open_project` is task 3's, because task 1's only command is
      the one that makes the project:

  ```rust
  //! The project a command runs against: the repository it is in, the files under `.farik/`, the
  //! event log, and the two ids every event carries.

  use std::path::{Path, PathBuf};
  use std::sync::Arc;

  use chrono::{DateTime, Utc};
  use farik_core::team::Team;
  use farik_protocol::event::{EventBody, EventIds, NewEvent, new_event};
  use farik_store::files::ProjectFiles;
  use farik_store::{EventLog, EventQuery, Git};

  /// Where the event log lives, under the gitignored `.farik/local/` (D5).
  pub(crate) const DATABASE: &str = ".farik/local/farik.db";

  /// An open project: everything a command needs to read what is there and record what it did.
  pub struct Project {
      /// The repository root, which is what a project is (`docs/SPEC.md` section 3).
      pub root: PathBuf,
      /// The files under `.farik/`.
      pub files: ProjectFiles,
      /// The log every command appends to.
      pub log: Arc<EventLog>,
      /// The team, read once because the ids and every refusal about an agent come from it.
      pub team: Team,
      /// What every event this run records says it belongs to.
      pub ids: ProjectIds,
  }

  /// The team and the project an event belongs to (`docs/SPEC.md` section 8.5).
  ///
  /// Both are fixed by `farik init` and read back from the log's first event afterwards, so that
  /// renaming the team or the directory does not split one project's log in two.
  pub struct ProjectIds {
      /// The team the events belong to.
      pub team_id: String,
      /// The project the events belong to.
      pub project_id: String,
  }

  impl ProjectIds {
      /// The ids the log already uses, or the ones this project starts with.
      ///
      /// # Errors
      ///
      /// The sentence the log's refusal reads as.
      pub fn of(log: &EventLog, team: &Team, root: &Path) -> Result<Self, String> {
          let first = log
              .read(&EventQuery {
                  limit: Some(1),
                  ..EventQuery::default()
              })
              .map_err(|error| error.to_string())?;
          if let Some(event) = first.first() {
              return Ok(Self {
                  team_id: event.envelope.team_id.clone(),
                  project_id: event.envelope.project_id.clone(),
              });
          }
          Ok(Self {
              team_id: slug(&team.name),
              project_id: slug(&directory_name(root)),
          })
      }
  }

  impl Project {
      /// One event of this project, ready to append.
      ///
      /// # Errors
      ///
      /// The sentence `new_event`'s refusal reads as, which is a blank id or a body about a contract
      /// with no contract named.
      pub fn event(
          &self,
          body: EventBody,
          at: DateTime<Utc>,
          task_id: Option<farik_core::contract::TaskId>,
      ) -> Result<NewEvent, String> {
          new_event(
              body,
              at,
              EventIds {
                  team_id: self.ids.team_id.clone(),
                  project_id: self.ids.project_id.clone(),
                  task_id,
                  agent_id: None,
                  session_id: None,
              },
          )
          .map_err(|error| crate::refusal::event(&error))
      }

      /// Appends one event and answers with the sequence number the log gave it.
      ///
      /// # Errors
      ///
      /// The sentence the store's refusal reads as.
      pub fn append(&self, event: &NewEvent) -> Result<u64, String> {
          self.log
              .append(event)
              .map(|recorded| recorded.envelope.seq)
              .map_err(|error| error.to_string())
      }
  }

  /// The root of the repository the command was run in, as git reports it.
  ///
  /// A project is a whole repository, so a command run three directories down is a command about the
  /// same project (`docs/SPEC.md` section 3).
  ///
  /// # Errors
  ///
  /// A sentence saying this is not a git repository, or what git said instead.
  pub fn repository_root(cwd: &Path) -> Result<PathBuf, String> {
      let git = Git::open(cwd.to_path_buf());
      match git.top_level() {
          Ok(root) => Ok(PathBuf::from(root)),
          Err(farik_store::GitError::NotARepository) => Err(format!(
              "{} is not a git repository, and a Farik project is one: run git init first",
              cwd.display()
          )),
          Err(error) => Err(error.to_string()),
      }
  }

  /// A directory's own name, for the id a project starts with.
  pub(crate) fn directory_name(root: &Path) -> String {
      root.file_name()
          .map(|name| name.to_string_lossy().to_string())
          .unwrap_or_default()
  }

  /// A kebab-case slug of a name a person chose, which is how an id is spelled everywhere in Farik
  /// (`docs/standards/code.md`).
  ///
  /// A name with nothing a slug can keep — punctuation, another script — answers `farik`, because a
  /// blank id names nobody and `new_event` refuses one.
  pub(crate) fn slug(name: &str) -> String {
      let mut slug = String::new();
      for character in name.chars() {
          if character.is_ascii_alphanumeric() {
              slug.push(character.to_ascii_lowercase());
          } else if !slug.ends_with('-') {
              slug.push('-');
          }
      }
      let slug = slug.trim_matches('-').to_string();
      if slug.is_empty() {
          "farik".to_string()
      } else {
          slug
      }
  }

  #[cfg(test)]
  mod tests {
      use super::slug;

      #[test]
      fn spells_a_name_a_person_chose_the_way_an_id_is_spelled() {
          assert_eq!(slug("Farik"), "farik");
          assert_eq!(slug("Maya Chen"), "maya-chen");
          assert_eq!(slug("  a  b  "), "a-b");
          assert_eq!(slug("my_project.v2"), "my-project-v2");
          assert_eq!(
              slug("プロジェクト"),
              "farik",
              "a name with nothing a slug can keep still names something: a blank id names nobody, \
               and new_event refuses one"
          );
          assert_eq!(slug("---"), "farik");
      }
  }
  ```

- [ ] Write `crates/cli/src/refusal.rs`. Only the event refusals for now; the governor's
      contract-write refusals arrive in task 6, with the command that meets them:

  ```rust
  //! The governor's refusals in the words a person reads.
  //!
  //! `farik-core` answers with values rather than sentences, so that one rule has one answer and every
  //! caller says it its own way (`docs/standards/code.md`). This is the command line's way of saying
  //! them; the daemon and the app will have their own, and every one of them points at the rule in
  //! `docs/SPEC.md` that decided it.

  use farik_protocol::event::EventError;

  /// Why an event could not be built. Either is this program disagreeing with itself rather than
  /// anything the person did, so each says which field was missing.
  #[must_use]
  pub fn event(error: &EventError) -> String {
      match error {
          // The field is `team_id`, `project_id`, or the body's own field naming who acted, so the
          // sentence names it rather than guessing which of the three it was.
          EventError::BlankId { field } => format!(
              "this event needed a {field} and it is blank, and a blank id names nobody: that is a bug \
               in Farik rather than anything you did"
          ),
          EventError::NoContractNamed { kind } => format!(
              "a {kind} event is about one contract and this one names none, which is a bug in Farik \
               rather than anything you did"
          ),
      }
  }

  #[cfg(test)]
  mod tests {
      use farik_protocol::event::{EventError, EventKind};

      use super::event;

      #[test]
      fn says_which_id_an_event_was_missing() {
          assert_eq!(
              [
                  event(&EventError::BlankId {
                      field: "team_id".to_string()
                  }),
                  event(&EventError::NoContractNamed {
                      kind: EventKind::ContractLocked
                  }),
              ],
              [
                  "this event needed a team_id and it is blank, and a blank id names nobody: that is \
                   a bug in Farik rather than anything you did",
                  "a contract.locked event is about one contract and this one names none, which is a \
                   bug in Farik rather than anything you did",
              ]
          );
      }
  }
  ```

- [ ] Write `crates/cli/src/init.rs`:

  ```rust
  //! `farik init`: make the repository this is run in a Farik project (F2).

  use std::path::Path;
  use std::sync::Arc;

  use chrono::{DateTime, Utc};
  use farik_core::criteria::{CriteriaLibrary, CriterionTemplate};
  use farik_core::team::{Team, validate_team};
  use farik_protocol::event::EventBody;
  use farik_protocol::generated::event::{CriteriaUpdatedBody, ProjectScannedBody, TeamUpdatedBody};
  use farik_store::files::{FilesError, ProjectFiles};
  use farik_store::{Git, ProjectScan, open_event_log, scan_project, seeded_library};
  use serde_json::json;

  use crate::project::{DATABASE, ProjectIds, directory_name, repository_root};
  use crate::{HUMAN, Report};

  /// Makes `.farik/`, the event log, the project scan and the criterion library, and records what it
  /// found.
  ///
  /// A second run is a rescan: the team file and everything a person has written are left alone, the
  /// scan is read again, and the criteria the scan found are replaced while the ones a person wrote
  /// are kept (`seeded_library`). That is what makes this the command to run after the project's test
  /// command changes.
  ///
  /// # Errors
  ///
  /// A sentence saying this is not a git repository, what git said about one it could not read, or
  /// what could not be written.
  pub fn init(cwd: &Path, now: DateTime<Utc>) -> Result<Report, String> {
      let root = repository_root(cwd)?;
      let git = Git::open(root.clone());
      let scan: ProjectScan = scan_project(&git, now).map_err(|error| error.to_string())?;
      let files = ProjectFiles::open(root.clone());

      // A file that is there and cannot be read is not a file that is absent. `init` writes nothing
      // over it and refuses instead: `ProjectFiles::init` would leave a broken team file where it is
      // and answer `Ok`, so a run that swallowed the difference would report a team it did not write,
      // and `seeded_library` keeps only what it is given, so it would drop every criterion a person
      // wrote because one line of the library has a typo in it.
      let existing = absent_or(files.read_team())?;
      let team_was_written = existing.is_none();
      let team = match existing {
          Some(team) => team,
          None => starter_team(&directory_name(&root))?,
      };
      files.init(&team).map_err(|error| error.to_string())?;

      let kept = absent_or(files.read_criteria())?;
      let library = seeded_library(&scan.detected_criteria, kept.as_ref());
      files
          .write_criteria(&library)
          .map_err(|error| error.to_string())?;
      files
          .write_project_scan(&project_document(&scan, &library))
          .map_err(|error| error.to_string())?;

      let log =
          Arc::new(open_event_log(&root.join(DATABASE), now).map_err(|error| error.to_string())?);
      let ids = ProjectIds::of(&log, &team, &root)?;
      let project = crate::Project {
          root: root.clone(),
          files,
          log,
          team,
          ids,
      };

      let mut recorded = Vec::new();
      if team_was_written {
          let event = project.event(
              EventBody::TeamUpdated(TeamUpdatedBody {
                  agent_ids: project
                      .team
                      .agents
                      .iter()
                      .map(|agent| agent.id.to_string())
                      .collect(),
                  team_name: project.team.name.to_string(),
                  updated_by: HUMAN.to_string(),
              }),
              now,
              None,
          )?;
          recorded.push(project.append(&event)?);
      }
      let event = project.event(
          EventBody::ProjectScanned(ProjectScannedBody {
              detected_criteria: names_of(&scan.detected_criteria),
              read_back: scan.read_back.clone(),
          }),
          now,
          None,
      )?;
      recorded.push(project.append(&event)?);
      let event = project.event(
          EventBody::CriteriaUpdated(CriteriaUpdatedBody {
              criterion_names: names_of(&library.criteria),
              updated_by: HUMAN.to_string(),
          }),
          now,
          None,
      )?;
      recorded.push(project.append(&event)?);

      let mut lines = vec![scan.read_back.clone()];
      if team_was_written {
          lines.push(format!(
              "wrote .farik/team.yaml: {}",
              project
                  .team
                  .agents
                  .iter()
                  .map(|agent| agent.id.to_string())
                  .collect::<Vec<_>>()
                  .join(", ")
          ));
      } else {
          lines.push("kept the team already in .farik/team.yaml".to_string());
      }
      lines.push(match names_of(&library.criteria).len() {
          0 => "no criteria: nothing in this repository says how it is tested".to_string(),
          count => format!(
              "{count} criteria in .farik/team/criteria.yaml: {}",
              names_of(&library.criteria).join(", ")
          ),
      });
      Ok(Report {
          lines,
          json: json!({
              "root": root.display().to_string(),
              "read_back": scan.read_back,
              "team_written": team_was_written,
              "criteria": names_of(&library.criteria),
              "events": recorded,
          }),
      })
  }

  /// What a file holds, nothing when there is no such file, and a refusal when there is one and it
  /// cannot be read.
  ///
  /// `.farik/team.yaml` and `.farik/team/criteria.yaml` are files 5.13 expects people to hand-edit, so
  /// one of them being unreadable is the ordinary way this command meets a mistake, and the answer is
  /// to say so rather than to write past it.
  fn absent_or<T>(read: Result<T, FilesError>) -> Result<Option<T>, String> {
      match read {
          Ok(value) => Ok(Some(value)),
          Err(FilesError::NotFound { .. }) => Ok(None),
          Err(other) => Err(other.to_string()),
      }
  }

  /// Every criterion's name, in the order they are held in.
  fn names_of(criteria: &[CriterionTemplate]) -> Vec<String> {
      criteria.iter().map(|one| one.name.to_string()).collect()
  }

  /// What `.farik/project.md` holds: the line the scan read back, and the criteria it found.
  ///
  /// This is the file every session is given (`docs/SPEC.md` section 5.8), so it says what the scan
  /// found and nothing it did not.
  fn project_document(scan: &ProjectScan, library: &CriteriaLibrary) -> String {
      let mut parts = vec![format!("# The project\n\n{}", scan.read_back)];
      let names = names_of(&library.criteria);
      if !names.is_empty() {
          parts.push(format!("Criteria: {}.", names.join(", ")));
      }
      format!("{}\n", parts.join("\n\n"))
  }

  /// The team a project starts with: the two agents `validate_team` says a team cannot work without
  /// (D18), named after their roles because the person has not named them yet.
  ///
  /// The team editor (F1) is how a person renames them, adds the other roles, and changes the models.
  /// Both get `claude-opus-5` at `high`, which is what `docs/SPEC.md` 8.2 ships as the default for the
  /// Product Manager, the Architect and the Developer.
  ///
  /// # Errors
  ///
  /// The sentence `validate_team`'s refusal reads as, which would mean this function and the schema
  /// disagree.
  fn starter_team(project: &str) -> Result<Team, String> {
      let wire = json!({
          "name": if project.is_empty() { "Farik".to_string() } else { project.to_string() },
          "agents": [
              {
                  "id": "product-manager",
                  "display_name": "Product Manager",
                  "role": "product_manager",
                  "persona": "Owns the backlog and turns every request into a contract.",
                  "status": "active",
                  "model": { "id": "claude-opus-5", "effort": "high" }
              },
              {
                  "id": "developer",
                  "display_name": "Developer",
                  "role": "software_developer",
                  "persona": "Writes the code and the tests that hold it.",
                  "status": "active",
                  "model": { "id": "claude-opus-5", "effort": "high" }
              }
          ],
          "budgets": { "daily_usd": 20 },
          "policy": {
              "human_accepts_contracts": "high_risk",
              "wip_limit_per_agent": 1,
              "blocked_limit_hours": 24,
              "max_iterations": 3,
              "integration": "manual"
          },
          "rules": {}
      });
      validate_team(&wire).map_err(|errors| {
          format!(
              "the team farik init writes is not one: {}",
              errors
                  .iter()
                  .map(|error| format!("{} {}", error.path, error.message))
                  .collect::<Vec<_>>()
                  .join("; ")
          )
      })
  }

  #[cfg(test)]
  mod tests {
      use farik_core::contract::Role;

      use super::starter_team;

      #[test]
      fn starts_a_project_with_the_two_agents_a_team_cannot_work_without() {
          let team = starter_team("notes").expect("the team farik init writes is a team");
          assert_eq!(team.name.as_str(), "notes", "named after the project");
          assert!(team.has_active(Role::ProductManager));
          assert!(team.has_active(Role::SoftwareDeveloper));
          assert_eq!(
              team.agents
                  .iter()
                  .map(|agent| agent.id.as_str().to_string())
                  .collect::<Vec<_>>(),
              ["product-manager", "developer"],
              "named after their roles, because the person has not named them yet"
          );
          assert_eq!(
              team.agents
                  .iter()
                  .map(|agent| agent.model.as_ref().map(|model| (
                      model.id.as_str().to_string(),
                      model.effort.map(|effort| effort.to_string())
                  )))
                  .collect::<Vec<_>>(),
              [
                  Some(("claude-opus-5".to_string(), Some("high".to_string()))),
                  Some(("claude-opus-5".to_string(), Some("high".to_string())))
              ],
              "8.2 ships Opus 5 at high for the Product Manager, the Architect and the Developer, and \
               this team is two of those three"
          );
      }

      #[test]
      fn names_a_team_after_something_when_the_directory_name_is_nothing() {
          assert_eq!(
              starter_team("").expect("a team").name.as_str(),
              "Farik",
              "a repository at the root of a volume has no directory name, and a blank team name is \
               one validate_team refuses"
          );
      }
  }
  ```

- [ ] Run the tests and the crate's suite; confirm green:

  ```
  cargo test -p farik -- --include-ignored
  # expected: exit 0, and
  #   test result: ok. 4 passed (crates/cli/src/lib.rs)
  #   test result: ok. 7 passed (crates/cli/tests/commands.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(cli): make a repository a farik project`

### Task 2: one YAML reader, for a file from anywhere

Files: modified `crates/store/src/files.rs`, tested by `crates/store/tests/project_files.rs`

Consumes: `yaml_options`, `FilesError` from `crates/store/src/files.rs`
Produces: `farik_store::files::yaml_value`

- [ ] Write the failing test, at the end of `crates/store/tests/project_files.rs`:

  ```rust

  #[test]
  fn reads_one_yaml_dialect_whatever_directory_the_file_came_from() {
      // `farik task create` is handed a contract from anywhere in the repository, and it goes through
      // the same reader as the files under `.farik/`: one duplicate key is one refusal, wherever the
      // file sits (ADR 0007).
      let refused = farik_store::files::yaml_value("name: a\nname: b\n", "request.yaml")
          .expect_err("a duplicate mapping key is not a document Farik reads");
      let text = refused.to_string();
      assert!(
          text.contains("request.yaml") && text.contains("name"),
          "the refusal names the file it is about and the key that is repeated: {text}"
      );
      assert_eq!(
          farik_store::files::yaml_value("on: true\n", "request.yaml"),
          Ok(serde_json::json!({ "on": true })),
          "and `on` is a key rather than a boolean, which is the same dialect the store reads"
      );
  }
  ```

- [ ] Run it and confirm it fails because the function is not there:

  ```
  cargo test -p farik-store --test project_files reads_one_yaml
  # expected: exit 101, and
  #   error[E0425]: cannot find function `yaml_value` in module `farik_store::files`
  #     ... not found in `farik_store::files`
  ```

- [ ] Write the function, above `contract_path` so that it sits with the other free functions of the
      module:

  ```rust
  /// The wire value one piece of YAML holds, named by the path it came from so that a refusal says
  /// which file it is about.
  ///
  /// This is the one place any YAML Farik reads is parsed, whatever directory it came from: a contract
  /// a person hands `farik task create` is held to the same dialect as the files under `.farik/` — no
  /// duplicate mapping key, no second document, the alias budget, and `true` spelled `true` (ADR 0007).
  ///
  /// # Errors
  ///
  /// `Invalid`, carrying the line, the column and the snippet `serde-saphyr` reports.
  pub fn yaml_value(text: &str, named: &str) -> Result<Value, FilesError> {
      serde_saphyr::from_str_with_options(text, yaml_options()).map_err(|error| FilesError::Invalid {
          path: named.to_string(),
          detail: error
              .render_with_formatter(&serde_saphyr::UserMessageFormatter)
              .replace("<input>", named),
      })
  }
  ```

- [ ] Make `read_yaml` call it, so the dialect is decided in one place. This **replaces** the method's
      body, which today inlines `from_str_with_options` and the formatter:

  ```rust
      fn read_yaml(&self, relative: &str) -> Result<Value, FilesError> {
          let text = self.read_text(relative)?;
          yaml_value(&text, &Self::named(relative))
      }
  ```

- [ ] Run the store's suite; confirm green:

  ```
  cargo test -p farik-store
  # expected: exit 0, and
  #   test result: ok. 66 passed (crates/store/src/lib.rs)
  #   test result: ok. 32 passed (crates/store/tests/project_files.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `refactor(store): read one yaml dialect wherever the file came from`

### Task 3: `farik task create`

Files: created `crates/cli/src/task.rs`; modified `crates/cli/src/lib.rs`,
`crates/cli/src/project.rs`, `crates/cli/tests/commands.rs`

Consumes: `yaml_value` from Task 2; `Project`, `ProjectIds`, `repository_root` from Task 1;
`FIELDS_THE_STORE_OWNS`, `FIELDS_THE_GOVERNOR_WRITES`, `FIELDS_ONLY_THE_HUMAN_WRITES`,
`FIELDS_FIXED_AT_CREATION` from `crates/core/src/governor/gates.rs`; `command_from_value`,
`Command`, `TaskCreatedBody`, `ContractSummary` from `crates/protocol/src/{command.rs,event.rs}`;
`EventLog::next_task_id` from `crates/store/src/event_log.rs`; `ProjectFiles::{read_contract,
write_contract, list_contracts}` from `crates/store/src/files.rs`
Produces: `open_project`, `task::create`

- [ ] Add these to `crates/cli/tests/commands.rs`: the three helpers with the helpers, the tests after
      task 1's. The file's `use` block becomes, in full:

  ```rust
  //! The command line against a real repository.
  //!
  //! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
  //! `cargo xtask check --integration`, as the store's own do and for the same reasons.

  use std::path::Path;
  use std::sync::Arc;

  use chrono::{DateTime, Utc};
  use farik::{CliIo, run_cli};
  use farik_core::contract::TaskId;
  use farik_protocol::clock::FixedClock;
  use farik_store::files::ProjectFiles;
  use farik_store::git::fixtures::TempRepo;
  use serde_json::{Value, json};
  ```

  ```rust
  /// A contract a person would write, as YAML, with nothing in it that is not theirs to write.
  fn a_request(title: &str) -> String {
      format!(
          r"title: {title}
  intent: A person can read the board without opening a database.
  scope:
    in_scope:
      - the board command
    out_of_scope:
      - the web app
  requirements:
    - id: R1
      text: The board prints one line per task.
  exit_criteria:
    - id: C1
      text: Every test in the workspace passes.
      satisfies:
        - R1
      verification:
        method: test
        command: cargo test --workspace
        new_tests_required: true
  assignee_role: software_developer
  reviewer_role: architect
  risk: low
  budget:
    max_cost_usd: 5
  allowed_paths:
    - crates/cli/**
  "
      )
  }

  /// Writes a request to a file beside the repository and answers with its path.
  fn a_request_file(repository: &TempRepo, name: &str, title: &str) -> std::path::PathBuf {
      let path = repository.path.join(name);
      std::fs::write(&path, a_request(title)).expect("the request is written");
      path
  }

  /// The board, so a test says what the log makes of what a command recorded.
  fn board_of(repository: &TempRepo) -> Vec<(String, String, String, bool, bool)> {
      let log = Arc::new(
          farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
              .expect("the log opens"),
      );
      farik_store::open_projections(log)
          .expect("the projections open")
          .board()
          .expect("the board reads")
          .iter()
          .map(|row| {
              (
                  row.task_id.as_str().to_string(),
                  row.kind.to_string(),
                  row.status.to_string(),
                  row.triaged,
                  row.locked,
              )
          })
          .collect()
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_every_other_command_until_the_project_exists() {
      let repository = a_repository("cli-no-project");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      let ran = run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("there is no Farik project at") && ran.err.contains("farik init"),
          "{}",
          ran.err
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn files_a_contract_as_a_draft_request() {
      let repository = a_project("cli-create");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      let ran = run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert!(
          ran.out
              .contains("FRK-1 filed as a draft request: A board command"),
          "{}",
          ran.out
      );
      assert!(ran.out.contains("farik triage"), "{}", ran.out);

      let contract = files_of(&repository)
          .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
          .expect("a contract was written");
      assert_eq!(contract.status.to_string(), "draft");
      assert_eq!(contract.created_by.as_deref(), Some("human"));
      assert_eq!(
          contract.kind.to_string(),
          "task",
          "a request is a task until the triage says otherwise (5.16)"
      );
      assert_eq!(
          board_of(&repository),
          [(
              "FRK-1".to_string(),
              "task".to_string(),
              "draft".to_string(),
              false,
              false
          )],
          "and the board knows it from the event alone"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn reads_the_contract_from_where_the_command_was_run() {
      // The path is the person's, so it is relative to the directory they typed it in — not to the
      // repository root, which is where the project is, and not to whatever directory this process
      // happens to be in.
      let repository = a_project("cli-create-relative");
      std::fs::create_dir_all(repository.path.join("notes")).expect("a directory");
      std::fs::write(
          repository.path.join("notes/request.yaml"),
          a_request("A board command"),
      )
      .expect("the request is written");

      let ran = run_in(
          &repository.path.join("notes"),
          &["task", "create", "request.yaml"],
      );

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert!(
          ran.out.contains("FRK-1 filed as a draft request"),
          "{}",
          ran.out
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_request_that_sets_what_is_not_the_authors_to_set() {
      let repository = a_project("cli-create-fields");
      let path = repository.path.join("request.yaml");
      std::fs::write(
          &path,
          format!(
              "{}id: FRK-9\nstatus: ready\nlocked: true\nparent: FRK-2\n",
              a_request("A board command")
          ),
      )
      .expect("the request is written");
      let ran = run_in(
          &repository.path,
          &["task", "create", path.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("id") && ran.err.contains("status") && ran.err.contains("locked"),
          "{}",
          ran.err
      );
      assert!(
          ran.err.contains("farik contract lock") && ran.err.contains("farik triage"),
          "a refusal says what to do instead: {}",
          ran.err
      );
      assert!(
          ran.err.contains("parent") && ran.err.contains("5.16"),
          "and `parent` is the one a person may legitimately write, so its refusal names the rule \
           that will open it: {}",
          ran.err
      );
      assert!(
          files_of(&repository)
              .list_contracts()
              .expect("a list")
              .is_empty(),
          "and nothing was filed"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_request_the_contract_rules_refuse() {
      let repository = a_project("cli-create-invalid");
      let path = repository.path.join("request.yaml");
      std::fs::write(
          &path,
          a_request("A board command").replace("  - id: C1", "  - id: R1"),
      )
      .expect("the request is written");
      let ran = run_in(
          &repository.path,
          &["task", "create", path.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("is not a contract Farik can file"),
          "{}",
          ran.err
      );
      assert!(
          files_of(&repository)
              .list_contracts()
              .expect("a list")
              .is_empty(),
          "and nothing was filed"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_file_that_is_not_yaml() {
      let repository = a_project("cli-create-not-yaml");
      let path = repository.path.join("request.yaml");
      std::fs::write(&path, "title: one\ntitle: two\n").expect("the request is written");
      let ran = run_in(
          &repository.path,
          &["task", "create", path.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("request.yaml"),
          "the refusal names the file it is about: {}",
          ran.err
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn prints_what_it_did_as_json_when_asked() {
      let repository = a_project("cli-json");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      let ran = run_in(
          &repository.path,
          &["--json", "task", "create", file.to_str().expect("a path")],
      );

      assert_eq!(ran.code, 0, "{}", ran.err);
      let printed: Value = serde_json::from_str(ran.out.trim()).expect("one JSON object per run");
      assert_eq!(printed["task_id"], json!("FRK-1"));
      assert_eq!(printed["status"], json!("draft"));
      assert_eq!(printed["path"], json!(".farik/contracts/FRK-1.yaml"));
      assert!(printed["events"].is_array(), "{printed}");
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn says_why_it_refused_as_json_when_asked() {
      let repository = a_project("cli-json-refusal");
      let ran = run_in(
          &repository.path,
          &["--json", "task", "create", "nowhere.yaml"],
      );

      assert_eq!(ran.code, 1);
      let printed: Value = serde_json::from_str(ran.err.trim()).expect("one JSON object per refusal");
      assert!(
          printed["error"]
              .as_str()
              .is_some_and(|text| text.contains("nowhere.yaml")),
          "a script that asked for JSON gets JSON for the refusal too: {printed}"
      );
      assert!(ran.out.is_empty(), "and nothing on stdout: {}", ran.out);
  }
  ```

- [ ] Run them and confirm they fail because the command is not there:

  ```
  cargo test -p farik --test commands -- --include-ignored
  # expected: exit 101, with eight failures, every one of them
  #   `error: unrecognized subcommand 'task'` on stderr and the exit code 2 clap answers
  #   with. Three of the eight expect 0 (`left: 2 / right: 0`) and five expect 1
  #   (`left: 2 / right: 1`), which is the assertion each of them makes about a command
  #   that is not there yet.
  ```

- [ ] Add the module and the subcommand to `crates/cli/src/lib.rs`. The module declaration after
      `pub mod refusal;`:

  ```rust
  /// Filing a request.
  pub mod task;
  ```

      then the re-export, which **replaces** `pub use project::Project;`:

  ```rust
  pub use project::{Project, open_project};
  ```

      the variant and its own enum, after `Init`:

  ```rust
      /// Work with one task.
      Task {
          #[command(subcommand)]
          command: TaskCommands,
      },
  }

  #[derive(Subcommand)]
  enum TaskCommands {
      /// File a contract as a draft request.
      Create {
          /// The YAML contract to file. Farik assigns the id.
          file: PathBuf,
      },
  }
  ```

      and the arm, after `Commands::Init`'s:

  ```rust
          Commands::Task {
              command: TaskCommands::Create { file },
          } => open_project(&io.cwd, now)
              .and_then(|project| task::create(&project, &io.cwd, file, now)),
  ```

- [ ] Add `open_project` to `crates/cli/src/project.rs`, above `repository_root`. Its `use
      farik_store::{...}` line becomes `use farik_store::{EventLog, EventQuery, Git,
      open_event_log};`:

  ```rust
  /// The project the command was run in: the repository root, whatever directory under it the person
  /// stood in.
  ///
  /// # Errors
  ///
  /// A sentence saying that this is not a git repository, that it is not a Farik project yet, or what
  /// the team file or the log got wrong.
  pub fn open_project(cwd: &Path, now: DateTime<Utc>) -> Result<Project, String> {
      let root = repository_root(cwd)?;
      let files = ProjectFiles::open(root.clone());
      let team = files.read_team().map_err(|error| match error {
          farik_store::files::FilesError::NotFound { .. } => format!(
              "there is no Farik project at {}: run farik init to make one",
              root.display()
          ),
          other => other.to_string(),
      })?;
      let log =
          Arc::new(open_event_log(&root.join(DATABASE), now).map_err(|error| error.to_string())?);
      let ids = ProjectIds::of(&log, &team, &root)?;
      Ok(Project {
          root,
          files,
          log,
          team,
          ids,
      })
  }
  ```

- [ ] Write `crates/cli/src/task.rs`:

  ```rust
  //! `farik task create`: file a YAML contract as a draft request (F3, `docs/SPEC.md` section 5.16).

  use std::path::Path;

  use chrono::{DateTime, Utc};
  use farik_core::contract::TaskContract;
  use farik_core::governor::gates::{
      FIELDS_FIXED_AT_CREATION, FIELDS_ONLY_THE_HUMAN_WRITES, FIELDS_THE_GOVERNOR_WRITES,
      FIELDS_THE_STORE_OWNS,
  };
  use farik_protocol::command::{Command, command_from_value};
  use farik_protocol::event::EventBody;
  use farik_protocol::generated::event::TaskCreatedBody;
  use serde_json::{Value, json};

  use crate::project::Project;
  use crate::{HUMAN, Report};

  /// The fields a person writing a request does not fill in: the store's, the governor's, the human's
  /// lock, and the two the triage and the epic's assignee decide (`docs/SPEC.md` section 5.11).
  ///
  /// Refused rather than overwritten: a command that quietly replaced what somebody wrote would make
  /// the file and the contract two different things.
  ///
  /// `parent` is the one of these a person may legitimately write: 5.16 item 3 lets the human create a
  /// task under an epic. The gate for that is `check_child_creation`, which needs the epic's assignee,
  /// and no projection carries one until phase 3 step 03 — so filing a child here would either skip the
  /// gate or depend on what does not exist yet. The refusal says so, and the project plan records it.
  fn not_the_authors() -> Vec<&'static str> {
      let mut fields: Vec<&'static str> = Vec::new();
      fields.extend(FIELDS_THE_STORE_OWNS);
      fields.extend(FIELDS_THE_GOVERNOR_WRITES);
      fields.extend(FIELDS_ONLY_THE_HUMAN_WRITES);
      fields.extend(FIELDS_FIXED_AT_CREATION);
      fields
  }

  /// Reads the contract in `file`, gives it the next id, and files it as a `draft` request.
  ///
  /// The contract goes through `command_from_value` rather than through `validate_contract` alone, so
  /// that a contract filed from a terminal is held to exactly the rules one arriving from an agent is.
  ///
  /// `file` is taken as the person typed it: absolute as it is, relative to `cwd`, which is where the
  /// command was run.
  ///
  /// # Errors
  ///
  /// A sentence saying the file could not be read, that it is not YAML, that it carries a field it is
  /// not the author's to write, every rule the contract breaks, or what could not be written.
  pub fn create(
      project: &Project,
      cwd: &Path,
      file: &Path,
      now: DateTime<Utc>,
  ) -> Result<Report, String> {
      // A path a person typed is relative to where they typed it, and `cwd` is where that was: the
      // project is the whole repository, so the directory the command was run in is not where the
      // project is, and reading the file through the process's own current directory would make the
      // library answer differently from the binary.
      let file = if file.is_absolute() {
          file.to_path_buf()
      } else {
          cwd.join(file)
      };
      let text = std::fs::read_to_string(&file)
          .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
      let mut wire = farik_store::files::yaml_value(&text, &file.display().to_string())
          .map_err(|error| error.to_string())?;
      let object = wire.as_object_mut().ok_or_else(|| {
          format!(
              "{} is not a contract: a contract is a mapping",
              file.display()
          )
      })?;

      let written: Vec<&'static str> = not_the_authors()
          .into_iter()
          .filter(|field| object.contains_key(*field))
          .collect();
      if !written.is_empty() {
          return Err(format!(
              "{} sets {}, which a request does not: Farik assigns the id and the stamps, the \
               governor writes the lifecycle, the lock is yours to take with farik contract lock, \
               farik triage decides whether this is an epic, and a task's parent is set by the epic's \
               assignee when it breaks the epic down (5.16)",
              file.display(),
              written.join(", ")
          ));
      }

      let task_id = project
          .log
          .next_task_id()
          .map_err(|error| error.to_string())?;
      let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
      object.insert("id".to_string(), json!(task_id.to_string()));
      object.insert("status".to_string(), json!("draft"));
      object.insert("created_by".to_string(), json!(HUMAN));
      object.insert("created_at".to_string(), json!(stamp));
      object.insert("updated_at".to_string(), json!(stamp));

      let command = command_from_value(&json!({
          "command": "task_create",
          "body": { "contract": wire }
      }))
      .map_err(|errors| {
          format!(
              "{} is not a contract Farik can file: {}",
              file.display(),
              errors
                  .iter()
                  .map(|error| format!("{} {}", error.path, error.message))
                  .collect::<Vec<_>>()
                  .join("; ")
          )
      })?;
      let Command::TaskCreate { contract } = command else {
          return Err("the command line built a command the reader did not read back".to_string());
      };

      project
          .files
          .write_contract(&contract)
          .map_err(|error| error.to_string())?;
      let event = project.event(
          EventBody::TaskCreated(TaskCreatedBody {
              created_by: HUMAN.to_string(),
              summary: summary_of(&contract),
          }),
          now,
          Some(contract.id.clone()),
      )?;
      let seq = project.append(&event)?;

      Ok(Report {
          lines: vec![
              format!(
                  "{} filed as a draft request: {}",
                  contract.id.as_str(),
                  contract.title.as_str()
              ),
              "farik triage says whether it is large or small; nothing starts before that \
               (5.16)"
                  .to_string(),
          ],
          json: json!({
              "task_id": contract.id.to_string(),
              "title": contract.title,
              "status": "draft",
              "path": format!(".farik/contracts/{}.yaml", contract.id.as_str()),
              "events": [seq],
          }),
      })
  }

  /// The fields of the contract the board shows, taken from the contract itself so that the log can
  /// be replayed into projections without the files (`docs/SPEC.md` section 8.4).
  fn summary_of(contract: &TaskContract) -> farik_protocol::generated::event::ContractSummary {
      let value = json!({
          "kind": contract.kind.to_string(),
          "parent": contract.parent.as_ref().map(|parent| parent.as_str().to_string()),
          "risk": contract.risk.to_string(),
          "status": contract.status.to_string(),
          "title": contract.title,
      });
      serde_json::from_value(strip_nulls(value)).expect(
          "a contract's own kind, risk, status and title are the summary's, and both vocabularies \
           come from task-contract.schema.json, which a test in farik-protocol holds to agreeing",
      )
  }

  /// An absent `parent` is absent rather than null: the summary's schema says `parent` is a string
  /// when it is there at all.
  fn strip_nulls(value: Value) -> Value {
      match value {
          Value::Object(map) => Value::Object(
              map.into_iter()
                  .filter(|(_, value)| !value.is_null())
                  .collect(),
          ),
          other => other,
      }
  }
  ```

- [ ] Run the tests and the crate's suite; confirm green:

  ```
  cargo test -p farik -- --include-ignored
  # expected: exit 0, and
  #   test result: ok. 4 passed (crates/cli/src/lib.rs)
  #   test result: ok. 15 passed (crates/cli/tests/commands.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(cli): file a yaml contract as a draft request`

### Task 4: the gate that says who may size a request

Files: modified `crates/core/src/governor/gates.rs`

Consumes: `GateResult`, `verdict`, `TaskStatus`, `TASK_STATUSES` from
`crates/core/src/governor/{gates.rs,task_status.rs}`
Produces: `farik_core::governor::gates::check_human_triage`

- [ ] Write the failing tests, at the end of `gates.rs`'s `mod tests`, and add `check_human_triage` to
      that module's `use super::{...}` list:

  ```rust
      #[test]
      fn lets_the_human_size_a_request_that_is_still_a_draft() {
          assert_eq!(check_human_triage(TaskStatus::Draft, false), Ok(()));
      }

      #[test]
      fn refuses_a_triage_once_refining_has_started() {
          // 5.16 gives the user the size "until refining starts", and a triage is the first thing
          // that happens to a request, so `draft` is the one status this is open at.
          for status in TASK_STATUSES {
              if status == TaskStatus::Draft {
                  continue;
              }
              assert_eq!(
                  reasons(check_human_triage(status, false)),
                  [format!(
                      "the request is {status} and a triage is the first thing that happens to one: \
                       5.16 gives you the size until refining starts"
                  )],
                  "{status}"
              );
          }
      }

      #[test]
      fn refuses_a_triage_of_a_task_that_belongs_to_an_epic() {
          assert_eq!(
              reasons(check_human_triage(TaskStatus::Draft, true)),
              [
                  "this task belongs to an epic, and an epic's tasks are not triaged: the epic was (5.16)"
              ]
          );
          assert_eq!(
              reasons(check_human_triage(TaskStatus::Refining, true)).len(),
              2,
              "both reasons, so a caller that fixes what it is told makes progress"
          );
      }
  ```

- [ ] Run them and confirm they fail because the gate is not there:

  ```
  cargo test -p farik-core --lib governor::gates
  # expected: exit 101, and
  #   error[E0432]: unresolved import `super::check_human_triage`
  #     ... no `check_human_triage` in `governor::gates`
  ```

- [ ] Write the gate, above `check_blocker_resolved`:

  ```rust
  /// Whether the human may say how big a request is (`docs/SPEC.md` section 5.16): the request is
  /// still a draft, and it is a request rather than a task an epic's breakdown made.
  ///
  /// 5.16 gives the user the triage decision "until refining starts", and the first thing that happens
  /// to a request is triage, so the one status this is open at is `draft`. It is the same rule for the
  /// first triage and for an overrule of one: what makes an overrule an overrule is that a decision is
  /// already recorded, and nothing about who may record one changes.
  ///
  /// A task under an epic was never triaged — its epic was (5.16 item 3) — so sizing one would answer a
  /// question nobody asked and change the kind its readiness was judged against.
  ///
  /// This is the human's gate. The one change the Product Manager may make to a triage — re-sizing a
  /// `refining` standalone task it has found to be larger than the triage thought — is the one tool
  /// gate of section 5 with no predicate here, and `docs/plans/project-plan.md` records why: it needs a
  /// decision in the spec first.
  ///
  /// # Errors
  ///
  /// One reason when refining has started, and one when the task belongs to an epic.
  pub fn check_human_triage(status: TaskStatus, has_parent: bool) -> GateResult {
      let mut reasons = Vec::new();
      if status != TaskStatus::Draft {
          reasons.push(format!(
              "the request is {status} and a triage is the first thing that happens to one: 5.16 \
               gives you the size until refining starts"
          ));
      }
      if has_parent {
          reasons.push(
              "this task belongs to an epic, and an epic's tasks are not triaged: the epic was (5.16)"
                  .to_string(),
          );
      }
      verdict(reasons)
  }
  ```

- [ ] Run the crate's suite and the no-I/O check; confirm green:

  ```
  cargo test -p farik-core
  # expected: exit 0, and
  #   test result: ok. 267 passed (crates/core/src/lib.rs)

  cargo xtask core-io
  # expected: silent
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(core): say who may size a request, and until when`

### Task 5: `farik triage`

Files: created `crates/cli/src/triage.rs`; modified `crates/cli/src/lib.rs`,
`crates/cli/tests/commands.rs`

Consumes: `check_human_triage` from Task 4; `open_project`, `Project` from Tasks 1 and 3;
`command_from_value`, `Command`, `RequestSize`, `RequestTriagedBody`, `RequestTriagedBodySize` from
`crates/protocol/src/{command.rs,event.rs}`; `open_projections`, `Projections::task` from
`crates/store/src/projections.rs`; `ProjectFiles::{read_contract, write_contract}` from
`crates/store/src/files.rs`
Produces: `triage::triage`, `triage::status_of`

- [ ] Add these to `crates/cli/tests/commands.rs`: the five tests after task 3's, and `moved_to` at
      the end of the file with the other helper that is not a test:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn sizes_a_request_as_large_and_makes_it_an_epic() {
      let repository = a_project("cli-triage-large");
      let file = a_request_file(&repository, "request.yaml", "A whole board");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );

      let ran = run_in(
          &repository.path,
          &[
              "triage",
              "FRK-1",
              "large",
              "--reason",
              "it is three screens and a migration",
          ],
      );

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert!(
          ran.out
              .contains("FRK-1 is large: epic. it is three screens and a migration"),
          "{}",
          ran.out
      );
      assert_eq!(
          files_of(&repository)
              .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
              .expect("a contract")
              .kind
              .to_string(),
          "epic",
          "the triage decides the kind and its own tool changes it (5.11)"
      );
      assert_eq!(
          board_of(&repository),
          [(
              "FRK-1".to_string(),
              "epic".to_string(),
              "draft".to_string(),
              true,
              false
          )]
      );
      assert_eq!(
          kinds_in(&repository).last().map(String::as_str),
          Some("request.triaged")
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn overrules_a_triage_while_the_request_is_still_a_draft() {
      let repository = a_project("cli-triage-overrule");
      let file = a_request_file(&repository, "request.yaml", "A whole board");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );
      run_in(
          &repository.path,
          &["triage", "FRK-1", "large", "--reason", "it looked large"],
      );

      let ran = run_in(
          &repository.path,
          &[
              "triage",
              "FRK-1",
              "small",
              "--reason",
              "one screen after all",
          ],
      );

      assert_eq!(ran.code, 0, "{}", ran.err);
      assert_eq!(
          board_of(&repository)[0].1,
          "task",
          "the user may overrule a triage until refining starts (5.16)"
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_triage_of_an_id_that_is_not_one() {
      // What a person typed, not the `oneOf` sentence a schema refuses a whole body with.
      let repository = a_project("cli-triage-bad-id");
      let ran = run_in(
          &repository.path,
          &["triage", "nine", "large", "--reason", "it has no number"],
      );

      assert_eq!(ran.code, 1);
      assert!(ran.err.contains("nine is not a task id"), "{}", ran.err);
      assert!(!ran.err.contains("oneOf"), "{}", ran.err);
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_triage_once_refining_has_started() {
      let repository = a_project("cli-triage-late");
      let file = a_request_file(&repository, "request.yaml", "A whole board");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );
      moved_to(&repository, "FRK-1", "refining");

      let ran = run_in(
          &repository.path,
          &["triage", "FRK-1", "large", "--reason", "too late"],
      );

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("refining") && ran.err.contains("5.16"),
          "{}",
          ran.err
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_triage_of_a_task_that_belongs_to_an_epic() {
      let repository = a_project("cli-triage-child");
      let file = a_request_file(&repository, "request.yaml", "A whole board");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );
      let files = files_of(&repository);
      let id = TaskId::try_from("FRK-1").expect("a task id");
      let mut contract = files.read_contract(&id).expect("a contract");
      contract.parent = Some("FRK-2".parse().expect("a parent id"));
      files
          .write_contract(&contract)
          .expect("the contract is written");

      let ran = run_in(
          &repository.path,
          &["triage", "FRK-1", "large", "--reason", "not mine to size"],
      );

      assert_eq!(ran.code, 1);
      assert!(ran.err.contains("belongs to an epic"), "{}", ran.err);
  }

  /// Puts a task's board row at a status, so that a test can ask what a command does about one.
  ///
  /// `contract.written` is the only event of this phase that moves a row, and its body carries a whole
  /// summary, so this rewrites the row's title, kind, risk and parent from the fixture as well. A
  /// governed transition is a `task.transitioned` event and rewrites none of that; it arrives with the
  /// runtime in phase 3, and so does the command that asks for one.
  fn moved_to(repository: &TempRepo, task_id: &str, status: &str) {
      use farik_protocol::event::fixtures::{a_contract_summary_wire, an_event_wire};
      use farik_protocol::event::{EventKind, NewEvent, event_from_value};

      let log = farik_store::open_event_log(&repository.path.join(".farik/local/farik.db"), at())
          .expect("the log opens");
      let log = Arc::new(log);
      let projections =
          farik_store::open_projections(Arc::clone(&log)).expect("the projections open");
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
  }
  ```

- [ ] Run them and confirm they fail because the command is not there:

  ```
  cargo test -p farik --test commands -- --include-ignored
  # expected: exit 101, with five failures, every one of them
  #   `error: unrecognized subcommand 'triage'` and clap's exit code 2 where the test
  #   expects 0 or 1.
  ```

- [ ] Add the module, the flag's value type and the subcommand to `crates/cli/src/lib.rs`. The module
      declaration after `pub mod task;`:

  ```rust
  /// Sizing a request.
  pub mod triage;
  ```

      then the clap import, which **replaces** `use clap::{Parser, Subcommand};`:

  ```rust
  use clap::{Parser, Subcommand, ValueEnum};
  ```

      the variant, after `Task`'s:

  ```rust
      /// Record how big a request is, or overrule the triage that did (5.16).
      Triage {
          /// The request being sized.
          task_id: String,
          /// Large becomes an epic; small becomes one standalone task.
          size: SizeArgument,
          /// Why, in your own words. The log keeps it.
          #[arg(long)]
          reason: String,
      },
  ```

      the value type and its one conversion, above `run_cli`:

  ```rust
  /// How big triage found a request, as a person types it.
  #[derive(Clone, Copy, ValueEnum)]
  enum SizeArgument {
      /// A large request, which becomes an epic.
      Large,
      /// A small request, which becomes one standalone task.
      Small,
  }

  impl From<SizeArgument> for farik_protocol::command::RequestSize {
      fn from(size: SizeArgument) -> Self {
          match size {
              SizeArgument::Large => Self::Large,
              SizeArgument::Small => Self::Small,
          }
      }
  }
  ```

      and the arm, after `Commands::Task`'s:

  ```rust
          Commands::Triage {
              task_id,
              size,
              reason,
          } => open_project(&io.cwd, now)
              .and_then(|project| triage::triage(&project, task_id, (*size).into(), reason, now)),
  ```

- [ ] Write `crates/cli/src/triage.rs`:

  ```rust
  //! `farik triage`: record how big a request is, or overrule the triage that did (`docs/SPEC.md`
  //! section 5.16).

  use chrono::{DateTime, Utc};
  use farik_core::contract::{TaskId, TaskKind, TaskStatus};
  use farik_core::governor::gates::check_human_triage;
  use farik_protocol::command::{Command, RequestSize, command_from_value};
  use farik_protocol::event::EventBody;
  use farik_protocol::generated::event::{RequestTriagedBody, RequestTriagedBodySize};
  use serde_json::json;

  use crate::project::Project;
  use crate::{HUMAN, Report};

  /// Records the size of a request, and the kind that follows from it.
  ///
  /// Large makes the request an epic and small makes it one standalone task, which is what the kind on
  /// its contract says; the human may record either while the request is still a draft, and this is
  /// both the first triage and the overrule of one, because there is one rule for who may say how big
  /// a request is and when.
  ///
  /// # Errors
  ///
  /// A sentence saying there is no such task, that refining has already started, or what could not be
  /// written.
  pub fn triage(
      project: &Project,
      task_id: &str,
      size: RequestSize,
      reason: &str,
      now: DateTime<Utc>,
  ) -> Result<Report, String> {
      // The id is parsed here rather than left to the schema: a person who typed it wrongly should read
      // what is wrong with what they typed, not the `oneOf` sentence a schema refuses a whole body
      // with. The command is still built and validated, so a triage from a terminal is held to exactly
      // the rules one arriving from an agent is.
      let named: TaskId = task_id
          .parse()
          .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
      let command = command_from_value(&json!({
          "command": "request_triage",
          "body": { "task_id": named.as_str(), "size": wire_size(size), "reason": reason }
      }))
      .map_err(|errors| {
          errors
              .iter()
              .map(|error| format!("{} {}", error.path, error.message))
              .collect::<Vec<_>>()
              .join("; ")
      })?;
      let Command::RequestTriage {
          task_id,
          size,
          reason,
      } = command
      else {
          return Err("the command line built a command the reader did not read back".to_string());
      };

      let mut contract = project
          .files
          .read_contract(&task_id)
          .map_err(|error| error.to_string())?;
      check_human_triage(status_of(project, &task_id)?, contract.parent.is_some())
          .map_err(|reasons| reasons.join("; "))?;

      // The triage decides the kind, and its own tool is what changes it (5.11, 5.16 item 1). The
      // file is written as well as the event, because the board takes the kind from the event and the
      // file is what a person reads.
      contract.kind = match size {
          RequestSize::Large => TaskKind::Epic,
          RequestSize::Small => TaskKind::Task,
      };
      contract.updated_at = Some(now);
      project
          .files
          .write_contract(&contract)
          .map_err(|error| error.to_string())?;
      let kind = contract.kind.to_string();

      let event = project.event(
          EventBody::RequestTriaged(RequestTriagedBody {
              reason: reason.clone(),
              size: match size {
                  RequestSize::Large => RequestTriagedBodySize::Large,
                  RequestSize::Small => RequestTriagedBodySize::Small,
              },
              triaged_by: HUMAN.to_string(),
          }),
          now,
          Some(task_id.clone()),
      )?;
      let seq = project.append(&event)?;

      Ok(Report {
          lines: vec![format!(
              "{} is {}: {kind}. {reason}",
              task_id.as_str(),
              match size {
                  RequestSize::Large => "large",
                  RequestSize::Small => "small",
              }
          )],
          json: json!({
              "task_id": task_id.to_string(),
              "size": wire_size(size),
              "kind": kind,
              "reason": reason,
              "events": [seq],
          }),
      })
  }

  /// The status the log says the task is at, which is the source of truth for it (`docs/SPEC.md`
  /// section 8.4).
  ///
  /// # Errors
  ///
  /// A sentence saying the board has never heard of this task, or what the store refused.
  pub(crate) fn status_of(project: &Project, task_id: &TaskId) -> Result<TaskStatus, String> {
      let projections = farik_store::open_projections(std::sync::Arc::clone(&project.log))
          .map_err(|error| error.to_string())?;
      let row = projections
          .task(task_id)
          .map_err(|error| error.to_string())?
          .ok_or_else(|| {
              format!(
                  "the log has never heard of {}, so there is nothing of it to change: farik \
                   doctor says where the files and the log disagree",
                  task_id.as_str()
              )
          })?;
      Ok(row.status)
  }

  /// How the size is spelled on the wire.
  fn wire_size(size: RequestSize) -> &'static str {
      match size {
          RequestSize::Large => "large",
          RequestSize::Small => "small",
      }
  }
  ```

- [ ] Run the tests and the crate's suite; confirm green:

  ```
  cargo test -p farik -- --include-ignored
  # expected: exit 0, and
  #   test result: ok. 4 passed (crates/cli/src/lib.rs)
  #   test result: ok. 20 passed (crates/cli/tests/commands.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(cli): size a request, and overrule the triage that did`

### Task 6: the governor's refusals in words

Files: modified `crates/cli/src/refusal.rs`

Consumes: `ContractWriteRefusal`, `TaskStatus` from `crates/core/src/governor/gates.rs` and
`crates/core/src/contract.rs`
Produces: `refusal::contract_write`

- [ ] Write the failing test. Its `mod tests` head **replaces** the three lines task 1 wrote
      (`use farik_protocol::event::{EventError, EventKind};`, the blank line, and
      `use super::event;`):

  ```rust
      use farik_core::contract::TaskStatus;
      use farik_core::governor::gates::ContractWriteRefusal;
      use farik_protocol::event::{EventError, EventKind};

      use super::{contract_write, event};

      fn fields(names: &[&str]) -> Vec<String> {
          names.iter().map(ToString::to_string).collect()
      }
  ```

      and the test itself goes above the event test:

  ```rust
      #[test]
      fn says_why_the_governor_would_not_let_a_contract_be_written() {
          // Every refusal a person can meet, in one place, so that a variant added to the governor
          // without a sentence here is a compilation error rather than a Rust value in a terminal.
          assert_eq!(
              [
                  contract_write(&ContractWriteRefusal::ContractLocked),
                  contract_write(&ContractWriteRefusal::ContractFrozen {
                      fields: fields(&["title", "risk"])
                  }),
                  contract_write(&ContractWriteRefusal::TaskTerminal {
                      status: TaskStatus::Accepted
                  }),
                  contract_write(&ContractWriteRefusal::LifecycleFields {
                      fields: fields(&["status"])
                  }),
                  contract_write(&ContractWriteRefusal::HumansFields {
                      fields: fields(&["locked"])
                  }),
                  contract_write(&ContractWriteRefusal::StoresFields {
                      fields: fields(&["id", "created_at"])
                  }),
                  contract_write(&ContractWriteRefusal::CreationFields {
                      fields: fields(&["kind"])
                  }),
                  contract_write(&ContractWriteRefusal::ContentFields {
                      fields: fields(&["title"])
                  }),
                  contract_write(&ContractWriteRefusal::UnknownFields {
                      fields: fields(&["colour"])
                  }),
              ],
              [
                  "the contract is held by the human, and a contract's content is the holder's alone \
                   (5.11): farik contract unlock gives it back to the team",
                  "the contract is frozen: once a task leaves refining only its status, assignee, \
                   reviewer, iteration, sprint and notes change (5.11), and this would change title \
                   and risk",
                  "the task is accepted, and nothing leaves that status (5.2): its notes are all that \
                   still change",
                  "status is the governor's, written when it applies a transition (5.2): ask for the \
                   transition instead",
                  "locked is the human's alone (5.11)",
                  "id and created_at are the store's: Farik assigns the identifier and the stamps",
                  "kind is fixed when a contract is created (5.16): the triage decides the kind, and \
                   a task's epic is the epic that broke it down",
                  "a contract's content is the Product Manager's and the human's, and an epic's tasks \
                   are its assignee's (5.16, 6.2): this actor does not write title",
                  "colour is not a field of a contract: every one is listed by name, so a field added \
                   to the schema is refused until somebody says who writes it",
              ]
          );
      }
  ```

- [ ] Run it and confirm it fails because the function is not there:

  ```
  cargo test -p farik --lib
  # expected: exit 101, and
  #   error[E0432]: unresolved import `super::contract_write`
  #     ... no `contract_write` in `refusal`
  ```

- [ ] Write the function, above `event`, with the import it needs above
      `use farik_protocol::event::EventError;`:

  ```rust
  use farik_core::governor::gates::ContractWriteRefusal;
  ```

  ```rust
  /// Why the governor would not let this write happen (`docs/SPEC.md` sections 5.2, 5.11 and 5.16).
  #[must_use]
  pub fn contract_write(refusal: &ContractWriteRefusal) -> String {
      match refusal {
          ContractWriteRefusal::ContractLocked => {
              "the contract is held by the human, and a contract's content is the holder's alone \
               (5.11): farik contract unlock gives it back to the team"
                  .to_string()
          }
          ContractWriteRefusal::ContractFrozen { fields } => format!(
              "the contract is frozen: once a task leaves refining only its status, assignee, \
               reviewer, iteration, sprint and notes change (5.11), and this would change {}",
              listed(fields)
          ),
          ContractWriteRefusal::TaskTerminal { status } => format!(
              "the task is {status}, and nothing leaves that status (5.2): its notes are all that \
               still change"
          ),
          ContractWriteRefusal::LifecycleFields { fields } => format!(
              "{} {} the governor's, written when it applies a transition (5.2): ask for the \
               transition instead",
              listed(fields),
              is_or_are(fields)
          ),
          ContractWriteRefusal::HumansFields { fields } => format!(
              "{} {} the human's alone (5.11)",
              listed(fields),
              is_or_are(fields)
          ),
          ContractWriteRefusal::StoresFields { fields } => format!(
              "{} {} the store's: Farik assigns the identifier and the stamps",
              listed(fields),
              is_or_are(fields)
          ),
          ContractWriteRefusal::CreationFields { fields } => format!(
              "{} {} fixed when a contract is created (5.16): the triage decides the kind, and a \
               task's epic is the epic that broke it down",
              listed(fields),
              is_or_are(fields)
          ),
          ContractWriteRefusal::ContentFields { fields } => format!(
              "a contract's content is the Product Manager's and the human's, and an epic's tasks are \
               its assignee's (5.16, 6.2): this actor does not write {}",
              listed(fields)
          ),
          ContractWriteRefusal::UnknownFields { fields } => format!(
              "{} {} not a field of a contract: every one is listed by name, so a field added to the \
               schema is refused until somebody says who writes it",
              listed(fields),
              is_or_are(fields)
          ),
      }
  }
  ```

- [ ] Write the two helpers it reads a list with, below `event`:

  ```rust
  /// A list in the words a person would read it out in.
  fn listed(fields: &[String]) -> String {
      match fields {
          [] => "nothing".to_string(),
          [one] => one.clone(),
          [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
      }
  }

  /// Whether the sentence about those fields takes a singular verb.
  fn is_or_are(fields: &[String]) -> &'static str {
      if fields.len() == 1 { "is" } else { "are" }
  }
  ```

- [ ] Run the crate's unit tests; confirm green:

  ```
  cargo test -p farik --lib
  # expected: exit 0, and
  #   test result: ok. 5 passed (crates/cli/src/lib.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(cli): say why the governor would not write a contract`

### Task 7: `farik contract lock` and `farik contract unlock`

Files: created `crates/cli/src/contract.rs`; modified `crates/cli/src/lib.rs`,
`crates/cli/tests/commands.rs`

Consumes: `contract_write` from Task 6; `status_of` from Task 5; `open_project`, `Project` from
Tasks 1 and 3; `check_contract_write`, `ContractWriteActor`, `TransitionActor` from
`crates/core/src/governor/{gates.rs,transition_table.rs}`; `ContractLockedBody`,
`ContractUnlockedBody` from `crates/protocol/src/generated/event.rs`;
`ProjectFiles::{read_contract, write_contract}` from `crates/store/src/files.rs`
Produces: `contract::hold`

- [ ] Add these to `crates/cli/tests/commands.rs`, after task 5's tests.
      `says_what_it_can_do_when_asked` waits until now because only now is the claim it makes true:
      the help it reads lists all four commands:

  ```rust
  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn takes_a_contract_and_gives_it_back() {
      let repository = a_project("cli-lock");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );

      let taken = run_in(&repository.path, &["contract", "lock", "FRK-1"]);
      assert_eq!(taken.code, 0, "{}", taken.err);
      assert!(taken.out.contains("FRK-1 is yours"), "{}", taken.out);
      assert!(
          files_of(&repository)
              .read_contract(&TaskId::try_from("FRK-1").expect("a task id"))
              .expect("a contract")
              .locked
      );
      assert!(
          board_of(&repository)[0].4,
          "the board says the human holds it"
      );

      let given = run_in(&repository.path, &["contract", "unlock", "FRK-1"]);
      assert_eq!(given.code, 0, "{}", given.err);
      assert!(
          given.out.contains("FRK-1 is the team's again"),
          "{}",
          given.out
      );
      assert!(!board_of(&repository)[0].4, "and that it gave it back");
      assert_eq!(
          kinds_in(&repository)
              .iter()
              .rev()
              .take(2)
              .cloned()
              .collect::<Vec<_>>(),
          ["contract.unlocked", "contract.locked"]
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_take_a_contract_that_is_already_yours() {
      let repository = a_project("cli-lock-twice");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );
      run_in(&repository.path, &["contract", "lock", "FRK-1"]);

      let ran = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

      assert_eq!(ran.code, 1);
      assert!(ran.err.contains("already yours"), "{}", ran.err);
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_to_take_a_contract_whose_task_is_finished() {
      let repository = a_project("cli-lock-accepted");
      let file = a_request_file(&repository, "request.yaml", "A board command");
      run_in(
          &repository.path,
          &["task", "create", file.to_str().expect("a path")],
      );
      moved_to(&repository, "FRK-1", "accepted");

      let ran = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

      assert_eq!(ran.code, 1);
      assert!(
          ran.err.contains("accepted") && ran.err.contains("nothing leaves that status"),
          "{}",
          ran.err
      );

      // And the governor is asked before the contract is found to be held already, so a task nothing
      // can be written to says that rather than answering about the lock.
      let files = files_of(&repository);
      let id = TaskId::try_from("FRK-1").expect("a task id");
      let mut contract = files.read_contract(&id).expect("a contract");
      contract.locked = true;
      files
          .write_contract(&contract)
          .expect("the contract is written");
      let again = run_in(&repository.path, &["contract", "lock", "FRK-1"]);

      assert_eq!(again.code, 1);
      assert!(
          again.err.contains("nothing leaves that status"),
          "not `already yours`: {}",
          again.err
      );
  }

  #[test]
  #[ignore = "needs the git program: cargo xtask check --integration"]
  fn refuses_a_task_id_that_is_not_one() {
      let repository = a_project("cli-bad-id");
      let ran = run_in(&repository.path, &["contract", "lock", "nine"]);

      assert_eq!(ran.code, 1);
      assert!(ran.err.contains("nine is not a task id"), "{}", ran.err);
  }

  #[test]
  fn says_what_it_can_do_when_asked() {
      let ran = run_in(Path::new("."), &["--help"]);

      assert_eq!(
          ran.code, 0,
          "help is what a person asked for, not a mistake"
      );
      for command in ["init", "task", "triage", "contract"] {
          assert!(ran.out.contains(command), "{}", ran.out);
      }
      assert!(ran.err.is_empty(), "{}", ran.err);
  }
  ```

- [ ] Run them and confirm they fail because the command is not there:

  ```
  cargo test -p farik --test commands -- --include-ignored
  # expected: exit 101, with five failures: four of them
  #   `error: unrecognized subcommand 'contract'` and clap's exit code 2 where the test
  #   expects 0 or 1, and `says_what_it_can_do_when_asked` panicking on the help text,
  #   which lists only init, task and triage.
  ```

- [ ] Add the module and the subcommand to `crates/cli/src/lib.rs`. The module declaration first,
      because the list is alphabetical:

  ```rust
  /// Taking a contract from the team, and giving it back.
  pub mod contract;
  ```

      the variant, after `Triage`'s:

  ```rust
      /// Take a contract from the team, or give it back (5.11).
      Contract {
          #[command(subcommand)]
          command: ContractCommands,
      },
  ```

      its own enum, between `TaskCommands` and `SizeArgument`:

  ```rust
  #[derive(Subcommand)]
  enum ContractCommands {
      /// Take the contract: from now on it is yours, and agents may only record results and notes.
      Lock {
          /// The task whose contract it is.
          task_id: String,
      },
      /// Give the contract back to the team.
      Unlock {
          /// The task whose contract it is.
          task_id: String,
      },
  }
  ```

      and the two arms, after `Commands::Triage`'s:

  ```rust
          Commands::Contract {
              command: ContractCommands::Lock { task_id },
          } => open_project(&io.cwd, now)
              .and_then(|project| contract::hold(&project, task_id, true, now)),
          Commands::Contract {
              command: ContractCommands::Unlock { task_id },
          } => open_project(&io.cwd, now)
              .and_then(|project| contract::hold(&project, task_id, false, now)),
  ```

- [ ] Write `crates/cli/src/contract.rs`:

  ```rust
  //! `farik contract lock` and `farik contract unlock`: contract ownership (`docs/SPEC.md` section
  //! 5.11).

  use chrono::{DateTime, Utc};
  use farik_core::contract::TaskId;
  use farik_core::governor::gates::{ContractWriteActor, check_contract_write};
  use farik_core::governor::transition_table::TransitionActor;
  use farik_protocol::event::EventBody;
  use farik_protocol::generated::event::{ContractLockedBody, ContractUnlockedBody};
  use serde_json::json;

  use crate::project::Project;
  use crate::triage::status_of;
  use crate::{HUMAN, Report};

  /// Takes a contract, or gives it back.
  ///
  /// Whether the write is allowed is `check_contract_write`'s answer, asked with `locked` as the only
  /// field changing and the human as the actor: the lock is the human's field alone, locking is not a
  /// content change and so does not send the task back to `refining`, and a task that is `accepted` or
  /// `cancelled` takes no write but a note.
  ///
  /// The status comes from the log, which is what decides a task's status (8.4), and the kind and the
  /// lock from the file, which is what decides a contract's content (8.4, and the project plan's note
  /// on step 07). On a project where the two disagree — which is what `farik doctor` is for — the
  /// governor is asked about the status the log knows, because that is the one the transition table
  /// answers for. The gate is asked before the contract is found to be held already, so that a task
  /// nothing can be written to says so rather than answering about the lock.
  ///
  /// # Errors
  ///
  /// A sentence saying there is no such task, that the contract is already held or already the
  /// team's, the governor's own refusal, or what could not be written.
  pub fn hold(
      project: &Project,
      task_id: &str,
      held: bool,
      now: DateTime<Utc>,
  ) -> Result<Report, String> {
      let task_id: TaskId = task_id
          .parse()
          .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
      let contract = project
          .files
          .read_contract(&task_id)
          .map_err(|error| error.to_string())?;
      let status = status_of(project, &task_id)?;
      check_contract_write(
          contract.kind,
          status,
          contract.locked,
          &ContractWriteActor {
              kind: TransitionActor::Human,
              agent_id: None,
          },
          &["locked".to_string()],
      )
      .map_err(|refusal| crate::refusal::contract_write(&refusal))?;
      if contract.locked == held {
          return Err(format!(
              "{} is already {}",
              task_id.as_str(),
              if held {
                  "yours: farik contract unlock gives it back"
              } else {
                  "the team's"
              }
          ));
      }

      let mut written = contract;
      written.locked = held;
      written.updated_at = Some(now);
      project
          .files
          .write_contract(&written)
          .map_err(|error| error.to_string())?;

      let body = if held {
          EventBody::ContractLocked(ContractLockedBody {
              locked_by: HUMAN.to_string(),
          })
      } else {
          EventBody::ContractUnlocked(ContractUnlockedBody {
              unlocked_by: HUMAN.to_string(),
          })
      };
      let event = project.event(body, now, Some(task_id.clone()))?;
      let seq = project.append(&event)?;

      Ok(Report {
          lines: vec![if held {
              format!(
                  "{} is yours: agents may record criterion results and write notes, and nothing \
                   else",
                  task_id.as_str()
              )
          } else {
              format!("{} is the team's again", task_id.as_str())
          }],
          json: json!({
              "task_id": task_id.to_string(),
              "locked": held,
              "events": [seq],
          }),
      })
  }
  ```

- [ ] Run the tests and the crate's suite; confirm green:

  ```
  cargo test -p farik -- --include-ignored
  # expected: exit 0, and
  #   test result: ok. 5 passed (crates/cli/src/lib.rs)
  #   test result: ok. 25 passed (crates/cli/tests/commands.rs)
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(cli): take a contract from the team, and give it back`

### Task 8: what the documents say

Files: modified `docs/SPEC.md`, `docs/plans/project-plan.md`,
`docs/plans/phase-2-protocol-store-cli/step-08-commands-that-write.md`

Consumes: nothing
Produces: nothing

- [ ] `docs/SPEC.md` section 3, after the paragraph that says what a project is: what `farik init`
      writes, because the starter team is behaviour a person meets and nothing in the spec says it
      yet. Add:

  ```markdown
  `farik init` makes a repository a project: it writes `.farik/`, the event log under
  `.farik/local/`, `project.md` holding the scan's read-back, and a criterion library seeded from what
  the scan found. When there is no team file it writes a starter team of two agents — a Product Manager
  and a Software Developer, the two a team cannot work without (5.1) — named after their roles for the
  user to rename in the team editor, on the models 8.2 ships, with one unfinished task per agent
  (`wip_limit_per_agent: 1`) and the rest of the policy as sections 5.2, 5.5 and 5.14 give it. A second
  `farik init` is a rescan: it keeps the team and the criteria a person wrote and replaces the ones the
  last scan found, and it refuses rather than writing past a team file or a criterion library that is
  there and cannot be read. The team and the project are identified in every event by the slug of the
  team's name and of the repository's own directory name, both fixed by the first `farik init` and read
  from the log thereafter.
  ```

- [ ] `docs/plans/project-plan.md`: correct the interface lines this step changed. Step 08's `CliIo`
      gains its lifetime and `task::create`'s `cwd`; phase 1 step 08's line gains
      `check_human_triage`; step 06's line gains `yaml_value`. Record on step 09's row that
      `farik doctor` has nothing to say about a team file or a criterion library that cannot be read
      until it compares them, and on this phase's decisions that filing a task under an epic waits
      for a projection carrying the epic's assignee (phase 3 step 03). Each carries the date and the
      reason, as the other corrections in that file do.

- [ ] Tick every box in this plan that is not yet ticked, and set its `Status` to `done`.

- [ ] Run the whole check; confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`
  ```

- [ ] Commit: `docs(docs): record what the command line writes`

## Verification

- [ ] The whole check, from the workspace root:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 5 passed (crates/cli/src/lib.rs)
  #   test result: ok. 25 passed (crates/cli/tests/commands.rs)
  #   test result: ok. 267 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 66 passed (farik-store)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 18 passed (crates/store/tests/git.rs)
  #   test result: ok. 32 passed (crates/store/tests/project_files.rs)
  #   test result: ok. 16 passed (crates/store/tests/project_scan.rs)
  #   test result: ok. 10 passed (crates/store/tests/reconciliation.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] The tests that need a program are still ignored without the flag, so they cannot pass silently:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 2 passed; 0 failed; 23 ignored (crates/cli/tests/commands.rs)
  ```

- [ ] `farik-core` still performs no I/O, which the gate this step adds does not change:

  ```
  cargo xtask core-io
  # expected: silent
  ```

- [ ] One dependency was added, and it is pinned:

  ```
  git diff a4478aa -- Cargo.toml
  # expected: two added lines, `clap = { version = "=4.6.7", features = ["derive"] }` and
  #   `farik-store = { path = "crates/store" }`. `a4478aa` is the commit that carried this
  #   plan, which is the last one before the step.
  ```

- [ ] The command line answers for itself, run as a person would:

  ```
  cargo run -q -p farik -- --help
  # expected: exit 0, and the four commands listed
  ```

- [ ] Every commit subject is accepted:

  ```
  for subject in \
    "feat(cli): make a repository a farik project" \
    "refactor(store): read one yaml dialect wherever the file came from" \
    "feat(cli): file a yaml contract as a draft request" \
    "feat(core): say who may size a request, and until when" \
    "feat(cli): size a request, and overrule the triage that did" \
    "feat(cli): say why the governor would not write a contract" \
    "feat(cli): take a contract from the team, and give it back" \
    "docs(docs): record what the command line writes"; do
    printf '%s\n' "$subject" > target/commit-subject
    cargo xtask commit-msg target/commit-subject
  done
  # expected: silent, eight times. target/ is where cargo writes and git ignores it.
  ```

## Open questions

none
