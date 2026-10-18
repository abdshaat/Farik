# Standalone (open issues), plan 6: six fixes in the engine

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1, 5.14, 6.7, 8.6 (recorded in plan 8's revision 0.122)
Depends on: plan 5 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The project scan finds a lockfile-less npm project's checks; a delivery's report is titled by what
it holds; an agent added to a saved team gets its role's persona (and a Designer its browser); the
server's credential line says honestly where the key is; two serving tests stop writing the real
settings folder; and every pull request Catervas opens has a short body a person reads: what
changed and how it was verified. Out of scope: the web app's screens (plan 7).

## Decisions

Every decision is ADR 0069's (decisions 8, 10 and 11); this section pins how.

- **Scan** (`crates/store/src/scan.rs`, `Reading::of`). After the lockfile search finds nothing, a
  tracked `package.json` gives the `npm` toolchain (the one whose marker is `package-lock.json`).
  `TOOLCHAINS` gains `bun.lock` (name `bun`, TypeScript, `Commands::Scripts`) before `bun.lockb`.
  Criteria and the preview's suggestion follow from the toolchain as they do now (`npm run lint`,
  `npm run test`, `npm install`).
- **The report's name.** `document_name` gives "Report for delivery <n>" for a path whose
  project-wide form is `docs/catervas/product/reports/D<n>.<ext>` (`<n>` all digits), checked
  before `plain_name`. Today's line, the branch slug (`docs/report-for-delivery-1`) and the pull
  request follow from it. `pub fn folder_change_title(paths: &[&str], names: &str) -> String` in
  `forge_text.rs`: "Docs: {names}" when every path is a delivery report, else "Docs: Update
  {names}"; `Subject::title` (`integrate.rs`) calls it. Commit subjects are unchanged.
- **An added agent's defaults.** `team.save` (`daemon/team.rs`) runs `with_role_defaults` on the
  wire team before `checked`: for each agent whose id is not in the saved `team.yaml`, a missing or
  blank `persona` becomes its role's `persona` (`catervas_roles::load_role`), and a `ui_ux_designer`
  with no `mcp_servers` gets `[{"name": "playwright", "source": "builtin"}]`, as `suggested()`
  gives one. An agent already saved is never touched, so a persona the owner cleared stays cleared.
- **The credential line** (`crates/cli/src/run.rs`), through `fn credential_line(kind:
  CredentialKind, source: Source) -> String`: `Source::File` says "credential: {what}, kept in a
  private file only you can read, credential.json in catervas's settings folder: catervas could not
  use this computer's keychain" (ADR 0022's fallback, reached when the keychain is missing, as on
  WSL, or refuses to load). The environment and keychain lines are unchanged.
- **The serving tests** `the_e2e_binary_serves_with_recorded_sessions` and
  `answers_the_setup_page_while_docker_info_hangs` (`crates/cli/tests/serving.rs`) spawn
  `catervas-e2e-serve` with `XDG_CONFIG_HOME` and `HOME` set to their own `scratch(..)` folders
  and `--no-keychain`, as `setup_env` does for the in-process tests.
- **A short pull request body** (the founder: "short, describing the feature or fix, and how they
  verified"), composed by Catervas with no session. `pub fn pull_request_body(changed: &str, checks:
  &[String], review: Option<(&str, &str)>) -> String` in `forge_text.rs`, exactly:
  `## What changed\n\n{changed}\n\n## How it was verified\n\n{one "- {check}" line each}` then, for
  `review: Some((name, sentence))`, a line `- Reviewed by {name}: {sentence}`. Nothing else: no
  intent, no notes, no task id (each text passes `without_task_ids`). `changed` is the contract's
  `summary`, else its title, cut to its first three sentences (`. `, `! `, `? ` end one) and 600
  characters. `checks` is each exit criterion's `text`, in order (the task is accepted, so each
  passed). The review is the reviewer's display name and the first sentence of its last review note
  written by an agent (not `catervas`), cut to 200 characters. The whole body is at most
  `PULL_REQUEST_BODY_CAP = 1500` characters: past it, check lines are dropped from the end and
  `- and {n} more checks` closes the list. A folder change's body is `## What changed\n\n{names,
  one "- " line each}\n\n## How it was verified\n\n- Written by {author} at the delivery's review;
  the team's documents are not reviewed (ADR 0061).`, with `- Approved by the owner on Today.` in
  its place when the change was approved, under the same cap.

## File map

```
crates/store/src/scan.rs                    modifies: bun.lock, the npm fallback (Task 17)
crates/store/src/scan.rs                    tests:    a_package_json_without_a_lockfile_is_an_npm_project (Task 17)
crates/core/src/folders.rs                  modifies: document_name names a delivery report (Task 18)
crates/core/src/forge_text.rs               modifies: folder_change_title (Task 18); pull_request_body, PULL_REQUEST_BODY_CAP (Task 22)
crates/core/src/forge_text.rs               tests:    titles_a_delivery_report_by_its_name (Task 18), writes_a_short_pull_request_body (Task 22)
crates/runtime/src/orchestrator/integrate.rs modifies: Subject::title (Task 18); pull_request_body builds its inputs (Task 22)
crates/runtime/src/orchestrator/integrate.rs tests:   opens_a_pull_request_for_an_accepted_task amended (Task 22)
crates/runtime/src/daemon/team.rs           modifies: with_role_defaults on team.save (Task 19)
crates/runtime/src/daemon/team.rs           tests:    gives_an_added_agent_its_roles_persona_and_a_designer_its_browser (Task 19)
crates/cli/src/run.rs                       modifies: credential_line (Task 20)
crates/cli/src/run.rs                       tests:    says_where_the_credential_is_kept (Task 20)
crates/cli/tests/serving.rs                 modifies: the two spawns' environment (Task 21)
```

## Interfaces

Consumes (on this branch): `Reading::of`, `TOOLCHAINS`, `reading_with` (`scan.rs`);
`document_name`, `plain_name`, `stem_of`, `without_member` (`folders.rs`); `without_task_ids`,
`pull_request_title` (`forge_text.rs`); `Subject::title`, `pull_request_body`, `folder_change`,
`plain_names` (`integrate.rs`, `tools/folders.rs`); `"team.save"`, `checked`, `suggested`,
`load_role` (`daemon/team.rs`, `catervas_roles`); `started`, `Source`, `CredentialKind`
(`run.rs`); `scratch`, `setup_env` (`tests/serving.rs`).

Produces:

```rust
// crates/core/src/forge_text.rs
pub fn folder_change_title(paths: &[&str], names: &str) -> String;
pub const PULL_REQUEST_BODY_CAP: usize = 1500;
pub fn pull_request_body(changed: &str, checks: &[String], review: Option<(&str, &str)>) -> String;
// crates/runtime/src/daemon/team.rs
fn with_role_defaults(deps: &ToolDeps, wire: &mut Value) -> Result<(), Failure>;
// crates/cli/src/run.rs
fn credential_line(kind: CredentialKind, source: Source) -> String;
```

(`ToolDeps` and `Failure` are what `checked` takes and returns.)

## Tasks

### Task 17: a project with no lockfile is npm's

Files: modified `crates/store/src/scan.rs`; tested by its tests

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `a_package_json_without_a_lockfile_is_an_npm_project` — tracked `package.json` with scripts
  `lint` and `test` and no lockfile: the toolchain is `npm`, the criteria run `npm run lint` and
  `npm run test`; with a tracked `bun.lock` it is `bun`. Fails first: no toolchain, no criteria.
  `a_project_whose_toolchain_it_does_not_know_gets_no_criteria` keeps a project with neither a
  lockfile nor a `package.json`.

- [ ] `fix(store): a project with a package.json and no lockfile is an npm project`

### Task 18: a delivery report is titled by its name

Files: modified `folders.rs`, `forge_text.rs`, `integrate.rs`; tested by `forge_text.rs`

Tests:

- `titles_a_delivery_report_by_its_name` — `document_name("docs/catervas/product/reports/D1.html",
  ..)` is "Report for delivery 1", `D12.md` gives "Report for delivery 12", `Dx.md` gives "Dx";
  `folder_change_title(&["docs/catervas/product/reports/D1.html"], "Report for delivery 1")` is
  "Docs: Report for delivery 1", and with a roadmap path beside it "Docs: Update …".
  `opens_a_pull_request_for_a_folder_change` (integrate.rs) keeps passing.

- [ ] `fix(core): a delivery's report is titled by its name`

### Task 19: an added agent gets its role's defaults

Files: modified `crates/runtime/src/daemon/team.rs`; tested by its tests

Tests:

- `gives_an_added_agent_its_roles_persona_and_a_designer_its_browser` — over a saved team,
  `team.save` with a new `software_developer` "noor" with no persona and a new `ui_ux_designer` with
  no `mcp_servers`: the written `team.yaml` gives noor the role's shipped persona and the designer
  `playwright` as a built-in; an agent already saved whose persona the save leaves out stays without
  one. Fails first: noor has no persona.

- [ ] `fix(runtime): an agent added to the team gets its role's persona`

### Task 20: the credential line is honest

Files: modified `crates/cli/src/run.rs`; tested by its unit tests

Tests:

- `says_where_the_credential_is_kept` — `credential_line(SubscriptionToken, File)` contains "a
  private file only you can read" and "could not use this computer's keychain", and not "catervas's
  credential.json"; `Keychain` still says "kept in your computer's keychain".

- [ ] `fix(cli): say plainly that the key is in a private file when there is no keychain`

### Task 21: the serving tests keep to their own folder

Files: modified `crates/cli/tests/serving.rs`. A test-only change: its check is that
`~/.config/catervas` is the same before and after `cargo test -p catervas --features e2e --test
serving -- --ignored the_e2e_binary_serves the_setup_page_while_docker_info_hangs` runs the two
tests (list it with `ls -la --time-style=full-iso` before and after and paste both).

- [ ] `test(cli): the serving tests write their own settings folder, not the real one`

### Task 22: a pull request's body is short

Files: modified `forge_text.rs`, `integrate.rs`; tested by both
Produces: `pull_request_body`, `PULL_REQUEST_BODY_CAP`

`integrate.rs`'s `pull_request_body` keeps its name and gathers: the contract's summary or title,
its criteria's texts, and the last review note by an agent with its reviewer's display name
(`name_of`); the folder change's names (`plain_names`), author and approval.

Tests:

- `writes_a_short_pull_request_body` (`forge_text.rs`) — a summary of five sentences keeps three;
  forty long checks fit within 1500 characters ending "- and {n} more checks"; the body has exactly
  the two `## ` headings, in that order.
- `opens_a_pull_request_for_an_accepted_task` (amended) — the body sent to `gh pr create` has
  exactly the headings `## What changed` and `## How it was verified`, holds each criterion's text
  and "Reviewed by" with the review note's first sentence and none of its second, holds neither the
  completion note nor the intent, and is at most 1500 characters. Fails first on `## Intent`.

- [ ] `fix(runtime): a pull request says what changed and how it was verified, briefly`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the git tests of integrate.rs run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
