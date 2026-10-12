# Phase 9, step 02b: Checks and scans per member

Status: ready (readiness review 2026-10-12, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 4.1, 5.3, 5.4, 5.13, 8.3, 8.5; F4
Depends on steps 01 and 02 of this phase, both planned and ready, not yet executed:
- from step 01: `read_repos`, `Repos`, `repo_present`, `repo.add`, `repo.remove`, `repo.locate` in `daemon/repos.rs`, `create_folder`
- from step 02: the contract's `repo`, `ReadinessContext.repos`, `task_repository`, `TaskRepository`, `TaskRepositoryError`, the readiness counts left at `CHECKS` 29, `EVERY_RULE` 31 and totals 30 and 32
- ADR 0057's amendment of 2026-10-12 ("checks, branches and epics per repository", "where a task works")

Nothing of steps 02c to 08.
Mockups: none; this step has no screen.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12 (one round): ready, no blocking finding; S1–S6 and N1–N10 folded below. S2 also tightened step 01's `repoName` to kebab-case.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 4102165.

## Goal

A project folder's criteria library is its members'. Each member is scanned, its found checks join the library as `<repo>-<name>` naming their member, and `project.scanned` reads back every member. A contract's criteria come only from its own repository. An epic in a project folder carries no command. A task's checks, `check_new_tests` included, run against its member's branch.

The scan runs:
- when a project folder is made
- when a member is added or located (its own entries replaced)
- when a member is removed (its entries dropped)

The project folder's own scan never runs, so a docs integration there no longer wipes the members' checks.

A one-repo project works exactly as today.

Out of scope:
- the rescan after a task integrates into its member (02c, on this step's `rescan_members`)
- the screens that show each member's facts (07)

## Decisions

- **The library's `repo`.** `criterionTemplate` (`docs/schemas/criteria.schema.json:22`) gains an optional `repo`, the member's name, its pattern copied from `repos.schema.json`'s `$defs.repoName` and held equal by a test.
  - An entry with no `repo` suits any task. An entry naming a member is not checked against `repos.yaml` when read, since core cannot read it. An entry naming no member any more expands into no contract (below).
  - `criteria_json` (`crates/store/src/requests.rs:1044`) carries `repo` (`null` when absent), so `catervas_read_criteria` (`tools/reading.rs:46`) shows it. `catervas criteria list` (`crates/cli/src/team.rs:53`) adds a `repo` column, empty when absent. The prompt's library YAML (`prompt.rs:233`) carries it as serialised.
- **A member's found checks.**
  - Each `CriterionTemplate` a member's scan finds is renamed `<repo>-<name>` and given `repo: <repo>`, its text and verification unchanged. The scan's names are `the-tests-pass`, `the-build-succeeds`, `clippy-is-clean`, `formatting-is-clean`, `the-linter-is-clean`, `the-types-check` and `the-project-check-passes` (`scan.rs:340-490`), so `app` gives `app-the-tests-pass`.
  - A renamed name that `CriterionTemplateName::try_from` refuses (over 64 characters, or against the pattern) is left out. With step 01's kebab-case member names and the longest scan name at 24 characters, no real scan reaches this; the guard keeps a bad name from making `write_criteria` refuse the whole library.
  - The rejected alternative was truncating the name, which could collide.
- **Merging into the library.** `reseeded(found, existing, members)` keeps:
  - every entry that is not `source: project_scan`
  - every `project_scan` entry whose `repo` is not one of `members`

  It then appends each found entry whose name is not already kept, in order. It stops at 100 entries and returns the names it left out with the library.
  - A human's entry wins a name clash, as in `seeded_library` (`crates/store/src/scan.rs:203`), which stays for one-repo projects.
  - `project.md` gains a line `Left out, the library being full: <names>.` when any are left out, so the owner and the agents can see it.
- **`rescan_members(files, log, ids, now, only, actor)`** scans every present member with `scan_project(&Git::open(path), now)`, in `repos.yaml` order. The members it reseeds are those present whose scan succeeded, narrowed to `only` when given, so a missing or failing member keeps its entries. It then writes the library and `project.md` and answers a `MembersScan`.
  - With no `repos.yaml` it is `MembersScanError::NotAProjectFolder`. A member's own scan error is never an error: it is in the read-back. Everything else is `MembersScanError::Files(FilesError)`, `Store(StoreError)` or `Event(EventError)`, each with its cause's sentence.
  - `actor` is `human` (`init`) or `catervas`, a constant `CATERVAS_ACTOR` in `catervas_store::scan`, since the runtime's own is private (`session.rs:677`).
  - The read-back is `<name>: <read_back>` for each scanned member, joined by `; `. A member that is missing reads `<name>: not found`; a member whose scan fails reads `<name>: not read (<error>)`.
  - It records `project.scanned { read_back, detected_criteria }`, the detected criteria being every found entry's renamed name across the members scanned. It records `criteria.updated { criterion_names, updated_by }` when the library changed, `updated_by` the caller's actor.
  - One function serves every caller. Rejected: a scan of one member alone, which could not write the joined read-back.
- **When the scan runs:**
  - **`init` in a project folder.** When `.catervas/repos.yaml` exists (`create_folder` writes it before `taken_on`, step 01), `init` (`crates/cli/src/init.rs:33`) does not scan the project folder. It runs as today up to and including `team.updated` (`:84-97`), after `files.init` (`:55`) and the log's opening (`:66-67`). In place of its own `seeded_library`, `project.md`, `project.scanned` and `criteria.updated` (`:57-62`, `:98-115`), it calls `rescan_members(.., None, "human")`, with `EventIds` built as `project.rs:84-90` builds them. A `MembersScanError` refuses `init` with its sentence. The one-repo path is unchanged.
  - **`repo.add` and `repo.locate`** call `rescan_members(.., Some(<name>), CATERVAS_ACTOR)` after their event and under the same lock. A member's scan error is in the read-back. A `MembersScanError` answers `INTERNAL_ERROR` with its sentence, and the member stays added or moved.
  - **`repo.remove`** drops the member's `project_scan` entries (`reseeded(&[], existing, &[name])`), writes the library, and records `criteria.updated` by `catervas` when anything was dropped. It then calls `rescan_members(.., None, CATERVAS_ACTOR)`, which scans the remaining members and rewrites `project.md`, so that file no longer names the removed one.
  - **The project folder's own scan never runs.** `refresh_the_scan` (`orchestrator/integrate.rs:363`) checks `files.is_project_folder()` first, before its branch check, and returns `ScanRefresh::Unchanged`, so a docs integration in a project folder reports nothing about the scan. Rejected: `Skipped`, whose `Display` (`orchestrator.rs:359-367`) would add "the project scan was not refreshed" to every such integration's report. 02c rescans the member a task integrated into.
  - **The `project.scan` query** (`daemon/team.rs:1479`) gains `repos: [{ name, facts, checks, error }] | null`: one row per member in `repos.yaml` order, each from `scan_project` of that member, or `null` for a one-repo project. A missing member, or one whose scan fails, has `facts: null`, `checks: []` and `error` its sentence (`not found` or the scan's error); otherwise `error` is `null`. Its `facts` and `checks` stay the project root's own. `rpc.schema.json:5328`'s `projectScanResult` gains the field, required and nullable.
- **A contract's criteria come from its repository.** `expand_criteria(refs, library, repo)` (`crates/core/src/criteria.rs:169`) gains `repo: Option<&str>`, the contract's. A library entry naming a member other than `repo`, or naming one when `repo` is `None`, is `CriteriaError::OtherRepo { name, entry_repo, task_repo }`. Its sentence: "criterion <name> checks <entry_repo>, and this contract works in <task_repo, or 'the project's own folder'>: set repo first, or use one of its own criteria". The name `criterion_of_another_repo` is the spec's (5.13) for this case; on the wire it is `criterion_not_expanded` with this sentence, as every `CriteriaError` is (`tools/refusal.rs:217`), so `refusal.rs` does not change.
  - `write_contract` (`tools/contracts.rs:249`) passes the contract's `repo` after its `fields` are applied, so a write that sets `repo` and names that member's criteria together passes.
  - Rejected: checking at readiness, since an expanded criterion keeps neither its library name nor its `repo` (`criteria.rs:169-209`).
- **An epic in a project folder runs no command.** The readiness rule `EpicCommandInProjectFolder`, after step 02's `RepoUnknown`, fails when `context.repos` is `Some`, the contract is an epic, and any exit criterion's method is `command` or `test`.
  - Message: "an epic in a project of several repositories runs no command: give <ids joined ", "> to its tasks instead".
  - `required_criteria_present` (`readiness.rs:512`) leaves out `command` and `test` for such an epic, as it does for a private-folder task. `run_on_the_integration_branch` (`orchestrator/requests.rs:856`) then runs only an epic's artifact criteria, in the project folder, unchanged.
  - `CHECKS` goes to 30, `EVERY_RULE` to 32, and the daemon's totals to 31 and 33. The code is `epic_command_in_project_folder`.
- **Checks run in the member.**
  - `run_what_catervas_runs` (`orchestrator/verify.rs:153`) resolves `task_repository` once. It uses that repository's `integration_branch` as `base` (`:179`) and its `Git` in `NewTestsInput` (`:189`). A `TaskRepositoryError` is `OrchestratorError::Refused`, as step 02 maps it in `verify.rs`.
  - The sandbox still mounts the task's worktree, under the project folder, so a command runs at the member's root, as `allowed_paths` are relative to it.
  - `NewTestsInput` gains `base_worktree: &'a Path`, the project folder's `.catervas/local/worktrees/<id>-base`. `check_new_tests` (`crates/runtime/src/criteria.rs:254`) makes and removes it there, through `input.git`, instead of `input.git.root()/.catervas/...` (`:272-280`), which in a member would land in the member's own folder.
  - `remove_workspace` (`integrate.rs:964`) still derives `<id>-base` from the project folder, which now agrees. 02c changes which `Git` removes it.

## File map

```
docs/schemas/criteria.schema.json, crates/core/src/criteria.rs   modifies, tests: repo, expand_criteria's repo, OtherRepo (Task 1)
crates/runtime/src/tools/contracts.rs                          modifies, tests: the contract's repo passed (Task 1)
crates/core/src/governor/readiness.rs, plain.rs, readiness/fixtures.rs   modifies, tests: EpicCommandInProjectFolder (Task 2)
crates/runtime/src/daemon/gates.rs                             modifies: the totals (Task 2)
crates/store/src/scan.rs, crates/store/src/lib.rs              modifies, tests: renamed, reseeded, rescan_members, MembersScan, project_document's left-out line (Task 3)
crates/store/tests/project_scan.rs                             modifies, tests: members scanned (Task 3)
crates/store/tests/project_files.rs                            modifies: project_document's new arguments (`:651`, `:667`) (Task 3)
crates/store/src/requests.rs, crates/cli/src/team.rs           modifies: criteria_json's and the list's repo (Task 3)
crates/cli/src/init.rs                                         modifies, tests: a project folder's init (Task 4)
crates/runtime/src/daemon/repos.rs                             modifies, tests: the scans of add, locate and remove (Task 4)
crates/runtime/src/daemon/team.rs, docs/schemas/rpc.schema.json   modifies, tests: project.scan's repos (Task 4)
crates/runtime/src/orchestrator/integrate.rs                   modifies, tests: refresh_the_scan skipped (Task 4)
crates/runtime/src/orchestrator/verify.rs, crates/runtime/src/criteria.rs   modifies, tests: checks in the member, base_worktree (Task 5)
crates/runtime/tests/new_tests.rs                              modifies, tests: NewTestsInput's base_worktree (`:49`), the base under the project folder (Task 5)
docs/SPEC.md                                                   modifies (Task 6)
```

## Interfaces

Consumes:
- step 01's `read_repos` and `repo_present`
- step 02's `task_repository`, `TaskRepository`, `ReadinessContext.repos`, `Harness::project_folder` and `TestProject::project_folder`
- `scan_project`, `ProjectScan`, `seeded_library`, `project_document`, `names_of` (`scan.rs:169`, `:24`, `:203`, `:259`, `:247`)
- `CriteriaLibrary`, `CriterionTemplate`, `CriterionSource`, `CriteriaError`, `validate_criteria` (`criteria.rs:15-19`, `:46`, `:117`)
- `ProjectFiles::read_criteria`, `write_criteria`, `write_project_scan`
- `ScanRefresh` (`orchestrator.rs:342`)
- `NewTestsInput`, `check_new_tests`, `remove_base_worktree` (`criteria.rs:113`, `:254`, `:490`)
- `GOVERNOR` (`verify.rs`), `HUMAN` (cli)

Produces:

```rust
// catervas_core::criteria
// CriterionTemplate gains `pub repo: Option<CriterionTemplateRepo>` (generated; compared by as_str())
pub enum CriteriaError { /* gains */ OtherRepo { name: String, entry_repo: String, task_repo: Option<String> } }
pub fn expand_criteria(refs: &[(String, String)], library: &CriteriaLibrary, repo: Option<&str>) -> Result<Vec<ExitCriterion>, CriteriaError>;
// catervas_core::governor::readiness
pub enum ReadinessRule { /* gains, after RepoUnknown */ EpicCommandInProjectFolder }
// catervas_store::scan
pub const LIBRARY_CAP: usize = 100;
pub fn renamed(repo: &str, found: &[CriterionTemplate]) -> Vec<CriterionTemplate>;   // over-long names left out
pub fn reseeded(found: &[CriterionTemplate], existing: Option<&CriteriaLibrary>, members: &[&str]) -> (CriteriaLibrary, Vec<String>);
pub struct MembersScan { pub read_back: String, pub detected: Vec<String>, pub library_changed: bool, pub left_out: Vec<String> }
pub const CATERVAS_ACTOR: &str = "catervas";
pub enum MembersScanError { NotAProjectFolder, Files(FilesError), Store(StoreError), Event(EventError) }   // Display: its cause's sentence
pub fn rescan_members(files: &ProjectFiles, log: &EventLog, ids: &EventIds, now: DateTime<Utc>, only: Option<&str>, actor: &str) -> Result<MembersScan, MembersScanError>;
pub fn project_document(scan_read_back: &str, library: &CriteriaLibrary, left_out: &[String], previous: Option<&str>) -> String;   // was (scan, library, previous)
// catervas_runtime::criteria::NewTestsInput gains `pub base_worktree: &'a Path`
```

`project_document`'s first argument becomes the read-back alone, since a project folder has no single `ProjectScan`; its callers (`init.rs`, `integrate.rs:414`, `tests/project_files.rs:651`, `:667`) pass `&scan.read_back` and `&[]`.

## Tasks

### Task 1: A criterion names its member, and stays in it

Files: `criteria.schema.json` (`repo` in `criterionTemplate.properties`); `criteria.rs` (`OtherRepo` and its `Display`, `expand_criteria`'s argument, its existing callers in tests passing `None`); `tools/contracts.rs` (`:249`, the contract's `repo` from `object`); `tools/refusal.rs` (the variant's sentence under `criterion_not_expanded`).

- `reads_a_criterion_that_names_its_member`: a library with `{ name: app-test, repo: app, .. }` reads and writes back unchanged. `repo: "App"` is refused at `/criteria/0/repo`. The schema's pattern equals `repos.schema.json`'s `$defs.repoName` pattern. RED.
- `expands_only_a_tasks_own_criteria`: a library with `app-test` (`repo: app`), `infra-lint` (`repo: infra`) and `review-docs` (no `repo`).
  - With `repo: Some("app")`, `app-test` and `review-docs` expand.
  - `infra-lint` is `OtherRepo { name: infra-lint, entry_repo: infra, task_repo: Some(app) }`, with the Decisions' sentence.
  - With `None`, `app-test` is `OtherRepo { .., task_repo: None }`, whose sentence says "the project's own folder".
  - RED.
- `an_agent_names_its_repo_and_its_criteria_together` (`tools/contracts.rs`, `TestProject::project_folder`, its library written by the test): a `write_contract` that sets `repo: app` and names `app-test` in the same call writes the expanded criterion. A later write naming `infra-lint` is refused `criterion_not_expanded` with `OtherRepo`'s sentence, the file unchanged. RED.

- [x] `feat(core): a criterion checks only its own repository`

### Task 2: An epic in a project folder runs no command

Files: `readiness.rs` (the rule after `RepoUnknown` in `CHECKS`, `required_criteria_present`'s exemption at `:512`); `plain.rs` (its sentence, `EVERY_RULE`); `readiness/fixtures.rs`; `daemon/gates.rs` (`:3758`, `:4359`).

- `an_epic_in_a_project_folder_runs_no_command`: with `repos: Some([app])`, an epic whose C1 is `command` and C2 is `review` fails `EpicCommandInProjectFolder` naming C1. The same epic with `repos: None` passes it. A task with a `command` criterion and `repo: app` passes it. RED.
- `requires_no_command_of_such_an_epic`: with `rules.required_criteria: [command]` and `repos: Some([app])`, an epic with only a `review` criterion passes `RequiredCriteriaPresent`; with `repos: None` it fails it. RED.
- `plain_readiness_covers_every_rule` passes with 32, and the daemon's totals read 31 and 33. RED: the counts.

- [x] `feat(core): an epic across repositories gives its commands to its tasks`

### Task 3: Scanning the members

Files: `scan.rs` (the Interfaces' items); `lib.rs` (their re-exports); `tests/project_scan.rs` (integration, `#[ignore = "needs the git program: cargo xtask check --integration"]`, a `TempRepo` `app` with a `package.json` whose `scripts.test` is `vitest` and a committed `package-lock.json`, and `infra` with a `Cargo.toml` and a committed `Cargo.lock`, as `project_scan.rs:89` and `:118` make them); `requests.rs` and `cli/src/team.rs` (`repo` shown).

- `renames_a_members_checks` (unit): `renamed("app", [the-tests-pass, the-linter-is-clean])` gives `app-the-tests-pass` and `app-the-linter-is-clean`, each `repo: app`, text and verification equal. A 32-character member name with a 32-character criterion name, 65 characters in all, is left out; so is any name from member `a-`. RED.
- `reseeds_one_members_checks`: existing `app-the-tests-pass` and `infra-the-tests-pass` (both `project_scan`) and `my-check` (human, `repo: app`). `reseeded([app-the-linter-is-clean], existing, [app])` keeps `infra-the-tests-pass` and `my-check`, drops `app-the-tests-pass`, adds `app-the-linter-is-clean`. With 100 entries already kept, the new one comes back in the left-out names, and `project_document` with it ends with `Left out, the library being full: app-the-linter-is-clean.`. RED.
- `scans_every_member` (integration): a project folder with `app` (vitest) and `infra` (cargo).
  - `rescan_members(.., None, "human")` writes a library with `app-the-tests-pass` (`repo: app`, `method: test`) and `infra`'s four.
  - The read-back is `app: <app's own read_back>; infra: <infra's>`.
  - It records `project.scanned { read_back, detected_criteria: [app-the-tests-pass, infra-the-tests-pass, infra-the-build-succeeds, infra-clippy-is-clean, infra-formatting-is-clean] }` and `criteria.updated { updated_by: human }`.
  - With `infra` moved away and `only: Some("app")`, the read-back ends `infra: not found` and `infra`'s entries are kept. With `infra` still away and `only: None`, they are kept too.
  - In a folder with no `repos.yaml` it is `NotAProjectFolder`.
  - RED.
- `lists_a_criterions_member`: `criteria_json` of `app-the-tests-pass` has `repo: "app"`, and of a human's entry with none `repo: null`. `catervas criteria list` prints each row as `{:<22} {:<9} {:<12} {:<12} {}`, `repo` after `source` and empty when absent. RED.

- [x] `feat(store): scan each member of a project folder`

### Task 4: When the members are scanned

Files: `init.rs` (the project-folder branch, in the Decisions' order); `daemon/repos.rs` (add, locate, remove); `daemon/team.rs` (`scanned`, `:1479`); `rpc.schema.json` (`projectScanResult.repos`); `integrate.rs` (`refresh_the_scan`, `:363`).

- `inits_a_project_folder_from_its_members` (`init.rs`): `init` in a project folder whose `repos.yaml` names `app` and `infra` (as `scans_every_member` makes them) writes their entries. It records `team.updated`, then one `project.scanned` with the joined read-back, then one `criteria.updated` by `human`, and no other `project.scanned`. The project folder itself is not scanned: its README is in no read-back. RED.
- `scans_a_member_as_it_joins_and_leaves` (`daemon/repos.rs`):
  - `repo.add` of `web` (vitest with its lockfile) adds `web-the-tests-pass` and records `project.scanned` and `criteria.updated` by `catervas`. `app-the-tests-pass` is untouched.
  - `repo.locate` of `app` at a new path rescans `app` alone.
  - `repo.remove` of `web` drops `web-the-tests-pass` and records `criteria.updated`; a human's entry naming `web` stays; `project.md` no longer names `web`.
  - RED.
- `answers_each_members_scan` (`daemon/team.rs`): `project.scan` in a project folder answers `repos` with `app` and `infra`, each with its facts, checks and `error: null`. With `infra` moved away its row has `facts: null`, `checks: []` and `error: "not found"`. In a one-repo project it answers `repos: null`. RED: no field.
- `never_rescans_a_project_folder_itself` (`integrate.rs`, `Harness::project_folder_with(.., with_the_suggested_six)`, step 02's): integrating an Architect's docs task, which names no `repo`, leaves `app-the-tests-pass` in the library and answers `Unchanged`. RED: `seeded_library` drops it.

- [x] `feat(runtime): scan a project folder's members when they join, move or leave`

### Task 5: A task's checks run in its member

Files: `verify.rs` (`run_what_catervas_runs`, `:153-199`); `criteria.rs` (`NewTestsInput.base_worktree`, `check_new_tests`, `:254-280`, its other callers passing the project root's).

- `runs_a_members_check_on_its_branch` (`verify.rs`, `Harness::project_folder` with a test command stub): a task with `repo: app` and the expanded `app-the-tests-pass` runs it in the sandbox made for its worktree, and records `criterion.recorded` passed. `new_tests_required` is judged against `app`'s `main`: a test file added on `feature/CTV-1` counts, and the same file already on `main` does not. RED: unknown revision `feature/CTV-1` in the project folder.
- `makes_the_base_worktree_under_the_project_folder` (`tests/new_tests.rs`, integration): `check_new_tests` for that task makes `<project>/.catervas/local/worktrees/CTV-1-base`, never `<app>/.catervas`, and removes it afterwards. RED.

- [ ] `feat(runtime): run a task's checks in its member`

### Task 6: Spec

`docs/SPEC.md`, at the next revision:
- 3, the criteria library: `repo`
- 4.1: a project folder's scan is its members'
- 5.3: `EpicCommandInProjectFolder`
- 5.13: the members' renamed checks, reseeding and the cap's left-out line, `criterion_of_another_repo`
- 5.4: checks run in the task's member
- 8.5: when `project.scanned` and `criteria.updated` are recorded in a project folder

- [ ] `docs(spec): record checks and scans per member`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
