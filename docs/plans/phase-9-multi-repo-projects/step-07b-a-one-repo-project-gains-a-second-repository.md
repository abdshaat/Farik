# Phase 9, step 07b: A one-repo project gains a second repository

Status: ready (readiness review 2026-10-12, three rounds, findings folded; amended 2026-10-23 for phase 10, below)
Branch: `feat/one-team-across-repositories`, its own pull request to `main`. Phase 9 merged in #45 without this step, and phase 10 merged in #91.
Spec: `docs/SPEC.md` 4.4, 8.4, 8.5; F1
Depends on these steps of this phase, planned (01 to 06 ready, 07 drafted), not yet executed:
- step 01: the project folder, `repos.yaml`, `member_root`, `check_member`, the member refusals, `repos.list`, `repo.added`, `home_of`, `Git::has_local_branch`
- step 02: the contract's `repo`, `task_repository`
- step 04: member folders (`docs/catervas/repos/<name>/<segment>/`)
- step 02b: `rescan_members`, `CATERVAS_ACTOR`
- step 02c: `Git::prune_worktrees`
- step 06: `covered_repos`, `Scope`, the member-scope system row
- step 07: `Repositories.tsx`'s one-repo view, `FolderBrowser` in a dialog, `repoNameOf`, `fillOf`, `folders.list` while driving

Nothing of step 08.
Mockups: `docs/design/mockups/AddSecondRepository.dc.html` on the canvas page "Multi-repo projects", drawn 2026-10-12 (c179d5b); approved by the founder on 2026-10-12 ("Approve", ADR 0032), with what it draws: the old files left on `catervas/moved-out` for the owner to merge, and the move waiting while work is in progress.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12. Round 1: not ready on B1 to B7 (the blockers overtaken, rejected and escalated work, document-only tasks, a crash part-way, undecided codes, a wrong expected code, the failure path); folded. Round 2: B1 to B7 resolved; not ready on N1 (a worktree left registered after a failed step 8) and N2 (`holds_project_folder`, not `inside_project_folder`); folded with its nits. Round 3: ready, its nit on an empty folder folded.
Amended 2026-10-13 for the HTML documents merged from main (ADR 0058); status: ready.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 93593b6.


## Amended 2026-10-23, for the several teams of phase 10

The founder, 2026-10-23: "Yes 1 team could operate across repositories". The answer was to the question whether to build this step, now that phase 10's "Your teams" refuses to link a project to a one-repo team (`conversion_not_available`).

The plan below stands, except where this section says otherwise. Line numbers below are at `93593b6`; the code on `main` (`101aaf2e`) wins, so find the named function.

- **Several teams in one serve (phase 10 step 05).**
  - Serve no longer switches one project. It drives every team behind a hub (`crates/runtime/src/daemon/hub.rs`), and each team is a drive on serve's one task (`crates/cli/src/serve.rs`, `FuturesUnordered`).
  - The conversion stops only the converting team's drive, as `project.leave` does for one team. It runs `convert`, then:
    - takes the old root out of `state.json`'s `projects` and the hub (`set_removals`);
    - remembers the project folder and starts its drive through the path that adds a team (`set_additions`).

    Every other team keeps running.
  - `Driven::Converted` is that drive's outcome, not a serve-wide mode.
  - On a failure before the commit point, the old root's drive starts again with `said` set. Its `WebState.take_on_error` shows the sentence as the plan says.
- **The page.** Step 06e built `apps/web/src/pages/settings/Repositories.tsx`, with "This doesn't belong here" per row. This step adds "Add a repository" there in its one-repo view.
  - The team's pages are under `/t/<team>/` (step 05b). After the move the team's address may change slug, since it is the project folder's team name and its team is the same. The page reopens on the hub and lands on `/t/<new slug>/settings/repositories`, found through `teams.list` by the project folder's root.
- **Linking into a one-repo team (phase 10 step 06e).**
  - `proposal.create`'s `add_to_team` card for a one-repo team no longer answers `conversion_not_available`. It answers `conversion_needed { team: slug }`.
  - The "Your teams" card then shows "Adding <project> makes <team> a team of several repositories." with a **Continue** button. Continue opens this step's `AddSecondRepository` dialog for that team, with the second repository filled in from the card.
  - The owner confirms there, as for any conversion. Nothing is converted without the dialog.
  - The test `adds_a_linked_project_to_the_team_it_works_with` (`crates/cli/tests/serving.rs`) changes to assert `conversion_needed`.
- **Sprints.** "Its sprints" in the Goal reads as its deliveries since ADR 0062. Everything in `.catervas/` moves.
- **Spec.** The spec revision is the next free one above the newest in `docs/SPEC.md`.
- **Task 7 is added after Task 5:** "Linking a project into a one-repo team asks for the conversion". Files: `crates/runtime/src/daemon/finding/proposal.rs`, `apps/web/src/pages/find/YourTeams.tsx`, `crates/cli/tests/serving.rs`, `apps/web/src/pages/find/YourTeams.test.tsx`. Its tests:
  - `asks_to_convert_a_one_repo_team_before_linking` (serving, ignored): asserts that `proposal.create` gives a one-repo team's card `conversion_needed`, and that the team is untouched.
  - `offers_the_conversion_for_a_one_repo_team` (web): asserts that the card shows the sentence, and that **Continue** opens the dialog filled with the project.
  - Commit: `feat(web): linking into a one-repo team asks to make it a team of several repositories`.

## Goal

From Settings › Repositories, the owner of a one-repo project adds a second repository, and the project becomes a project folder. Catervas then:
- makes the project folder and moves the team into it, whole: its tasks with their ids, its log, its sprints, its memory, its settings and its connector keys
- moves the documents into the project folder, each member's into `repos/<name>/`
- names both repositories in `repos.yaml`
- leaves one branch in the first repository, `catervas/moved-out`, which removes the files Catervas kept there, for the owner to merge
- takes the project folder on in the same process, on the same port, as ADR 0053's switch does

Opening the first repository again opens its project folder. The dialog says all this before anything happens, and the move waits while the team has work in progress.

Out of scope: moving a project folder back to one repository; a second conversion of a repository already moved.

## Decisions

### When it may happen

- **The word.** `project.convert { first_name, parent, name, second: { name, path, integration_branch? } }`, a method answered in drive mode (`crates/runtime/src/daemon/team.rs` `METHODS`, `:58`), whose result is `emptyResult`. `first_name` names the current repository as a member; `parent` and `name` place the project folder, as `project.create_folder`'s do (step 01, its `..` and missing-parent rules included); `second`'s path is relative to home.
- **What holds it: `convert_blockers`.** One function, `catervas_store::convert::convert_blockers(files, events, git) -> Result<Blockers, FilesError>`, used by the query, the method and the move itself. It counts:
  - every task `assigned`, `in_progress`, `blocked`, `verifying` or `rejected` (which goes back to `in_progress` with its branch, `transition_table.rs:182-200`), or `accepted` and awaiting integration
  - every `escalated` task whose `task_branch(contract)` (`crates/core/src/branch.rs:14`) is a local branch of the repository (`Git::has_local_branch`, step 01)
  - every folder change not integrated and not escalated
  - every task filed for the owner by main's HTML change, its `task.created` carrying `filed_as: owner_change` or `document_rewrite` (`event.schema.json:327-331`; a rewrite's are `rewrite_tasks`, `crates/store/src/requests.rs:625`), neither accepted nor cancelled, in any other status: counted with the tasks, by id. Their contracts and rubrics name the old folders in their words ("gone from docs/catervas/architecture"), which the migration below does not rewrite, so they finish first.
  - an open one-repo Catervafication epic (neither `done` nor `cancelled`): its rows are filed at the one-repo paths, so documenting finishes first. This is the approved "waits while work is in progress"; rejected: re-placing its rows after the move, which step 06's `file_due` would have to learn.

  Their worktrees and branches belong to this repository's git, and nothing moves them.
- **Its refusals, in order**, each `REFUSED` with `data.errors: [{ path, message, code }]` (new codes unless noted):
  1. `not_one_repo` (`/`): the project is a project folder already.
  2. `work_in_progress` (`/`): "Your team is still working in <name>: <ids>. Let them finish, or cancel them, before you add a repository.", `<ids>` the tasks' ids, then "the Catervafication" for its epic, then "<k> changes to your team's notes" for folder changes, joined by ", " and " and ".
  3. `project_name` (`/name`): `project.create`'s rule (`setup.rs:227-233`), its sentence.
  4. `outside_home` (`/parent`), step 01's.
  5. `folder_there` (`/name`): "a folder with that name is already there", except an empty folder, and a folder whose `.catervas/local/converting` names this old root (a crashed move, below). The method only checks: the clean-up runs in `convert`, after the stop and under the lock.
  6. For the first member, core's `check_member(project_folder, &[], first_name, old_root)` (step 01): `repo_name` at `/first_name`, `holds_project_folder` at `/parent` (a parent inside the old root). Its `inside_project_folder` cannot arise here: the project folder would be an ancestor of the old root, already there, and refused by 5. Store's `member_root` is not run for it: it is this project, its team expected.
  7. For `second`: `path_taken` at `/second/path` when its canonical path is the old root's, checked first; then step 01's `member_root` (`has_team` among them) and `check_member` against the project folder and the first member, at `/second/...`.

  `no_account` is not checked: the account is already connected.
- **The page can ask first.** The query `project.convert_blockers {}` answers `{ tasks: [ids], catervafication: bool, folder_changes: [numbers] }` from `convert_blockers`, so that the dialog says it before the owner fills anything in (the mockup's alert). Continue is disabled while any is set.
- **The switch.** With every check passed, the method stores the conversion in `WebState.converting: Mutex<Option<Conversion>>` (`crates/runtime/src/daemon/web.rs:44`) and stops the run exactly as `project.leave` does (`team.rs:1845-1858`). A run stop lets a running session finish (`orchestrator/human.rs:282-288`), so `convert` counts again (below). `serve.rs`'s drive loop, finding `converting` set when the driver ends (`:302`), answers `Driven::Converted(PathBuf, Conversion)` in place of `Left`, and hands the interrupts back to `io` for it as for `Left` (`:303-308`).

### What the move does

`crates/cli/src/convert.rs`'s `convert(old_root, conversion, io, now) -> Result<PathBuf, String>` runs between the two drives, holding `try_lock(old_root)` (`run.lock`) throughout. Each step is undone, in reverse, when a later step before the commit point fails:

| # | Step | Undo |
|---|---|---|
| 0 | `convert_blockers` again, now that nothing drives; anything there fails the move with `work_in_progress`'s sentence | – |
| 1 | `mkdir -p` the parent; `mkdir` the project folder, unless it is there and empty; write `.catervas/local/converting` holding the old root; `git init -b main`, the README, "Start the project" (as step 01's `create_folder`) | remove the project folder whole (an empty one the owner had included: it held nothing) |
| 2 | copy `.catervas/` from the old root, links never followed, leaving out `local/worktrees/`, `local/run.lock`, `local/daemon.json`, `local/integration.lock`, `local/setup-pending`, `local/moved-to`, `local/converting`; `catervas.db` with its `-wal` and `-shm` when there, after the driver has finished and dropped every handle on the log (SQLite replays the WAL on open). The Procurement mailbox's files are under `local/`, so `carry_mailbox` is not called. | (1's) |
| 3 | `copy_keys` (`runtime/src/connectors.rs:524`) from the old root's id to the project folder's; no `keys-copied.json` notice | `secrets.delete` each `SecretAt` it returned |
| 4 | move the documents (below) and commit "Move the team's documents from <first_name>" | (1's) |
| 5 | `write_repos`: the first member (`first_name`, the old root's canonical path, the team's `policy.integration_branch` when set), then the second as asked; clear `policy.integration_branch` in the copied `team.yaml` (it would name the project folder's branch, `main`; ADR 0057's amendment) | (1's) |
| 6 | the contracts' migration (below) | (1's) |
| 7 | the criteria: drop every `source: project_scan` entry with no `repo`, keep the human's, then step 02b's `rescan_members(.., None, CATERVAS_ACTOR)` | (1's) |
| 8 | in the old repository, from its integration branch, the branch `catervas/moved-out` (or `-<n>`, the first free), one commit "Remove Catervas's files: the team moved to <project folder>" removing every tracked path the move took (below), made in a detached worktree under the project folder's `.catervas/local/moving/`, after `git worktree prune` in the old repository (`Git::prune_worktrees`, step 02c), the worktree removed afterwards; never pushed or merged, under any policy | `remove_worktree` if made, then `prune_worktrees`, then delete the branch |
| 9 | record in the copied log `repo.added` for each member, then `project.converted { project_folder, first_repo, branch }` | (1's) |
| 10 | write `<old_root>/.catervas/local/moved-to` holding the project folder's path (`local/` is ignored by git) | remove it |
| 11 | **the commit point:** remove `.catervas/local/converting` | – |
| 12 | `remember` the project folder; serve goes to `Mode::Drive(project_folder)` on the same port | – |

- **The documents** are read from the old repository's integration branch by `Git::tree_entries` and a new `Git::blob_bytes(blob) -> Result<Vec<u8>, GitError>` (`git cat-file blob`), so a file moves byte for byte. A regular file (mode `100644` or `100755`) keeps its mode, a link (`120000`) is made a link with the same target, and anything else (a submodule) is left where it is and not removed by step 8. Under `docs/catervas/`, `architecture/` other than `plans/`, `engineering/` and `design/` go to `repos/<first_name>/<same path>`; everything else, `plans/` and files outside the role folders included, to the same path. Under `.catervas/`, step 2 copied it all. A document not on the integration branch is not moved: the blockers mean none is waiting.
- **The contracts' migration.** For every task neither cancelled nor integrated:
  - `allowed_paths` and artifact criteria's paths under `docs/catervas/architecture/` (not `plans/`), `engineering/` or `design/` are rewritten to `docs/catervas/repos/<first_name>/…`.
  - When its assignee changes code (`changes_code`) and some of its rewritten `allowed_paths` is not a document path (`is_a_document_path` against `rules.document_paths`, `readiness.rs:765-769`, step 02's rule), it gains `repo: <first_name>`. A builder's task kept to documents gains none and works in the project folder (step 02).
  - The file is written directly, as a migration, and `contract.written { written_by: "catervas" }` recorded. This bypasses the freeze: by the blockers, its work has not moved (`draft`, `refining`, `ready`, or in the Backlog). Rejected: leaving them to `RepoRequired`, which would send every ready builder task back to refining.
- **A crash before the commit point** leaves a project folder holding `converting`. `found()` (`serve.rs:122`), `project.open` (`setup.rs:169`) and `open_project` (`project.rs:127`) refuse a folder holding it ("that folder is a project being made from <old root>; add the repository again to finish"), and a `moved-to` whose folder holds it is ignored, so the old repository drives again as before. Asking again with the same `parent` and `name`, `convert` (step 0, under the lock) removes that folder (refusal 5), runs `prune_worktrees` in the old repository, deletes the old repository's branches `catervas/moved-out*` whose tip's message names that project folder, removes a `moved-to` naming it, and moves again.
- **A failure before the commit point** undoes the steps made, and serve goes back to `Mode::Drive(old_root)` with the sentence: `drive` gains `said: Option<String>`, set into the new daemon's `WebState.take_on_error` after `connected_daemon`, which `serve.status` already answers in any mode (`web.rs:960`). It is shown on Settings › Repositories as "Catervas could not add the repository: <sentence>" until the next drive, which passes `None`; the setup page's `WebState` is a new one and never holds it. A converted drive that fails to start ends serve as any failed drive does today.

### After the move

- **Readiness and approvals stand.** The migration's `contract.written` does not send a ready task back: `Ready → Assigned` is gated by the assignment (`transition_table.rs`, `G::Assignment`), not readiness; the judgment (`judgment_since_written`, `transitions.rs:1504`) is read only for `refining → ready`; an approval counts from the last move into `refining`. A `refining` task's judgment is asked again after the write, as after any write.
- **An earlier Catervafication covers the first member.** After a `project.converted`, step 06's `covered_repos` counts a `Repository`-scope start as covering that event's `first_repo` and as the project-wide start, so `offered_repos` offers the second member alone. A change to step 06, made in Task 4.
- **A member's own Catervafication writes the system map the first time.** When no `docs/catervas/architecture/system` document, in either format, is on the project folder's integration branch (`held_document`, `crates/store/src/git.rs:1134`), step 06's member-scope system row is the project-wide one, "Map how the repositories connect", depending on that member's architecture task alone. Also a change to step 06, in Task 4.
- **A moved repository opens its project folder.** `found()`, `project.open` and `open_project` read `.catervas/local/moved-to` first and use the folder it names when it is a project folder not holding `converting`; then step 01's rules apply to that folder, its refusal of a project folder while a project is being left included. No message is printed.
- **Locating the first member.** Step 01's `member_root` skips `has_team` for a folder whose `moved-to` names this project folder, so `repo.locate` works before `catervas/moved-out` is merged. A change to step 01, in Task 4.
- **Adding to a one-repo project stays refused.** Step 01's `repo.add` keeps `not_a_project_folder`; the page uses `project.convert` instead (step 01's "07b lifts this" is corrected).
- **Today reminds the owner.** `repos.list`'s rows gain `moved_out: string | null`: the branch `project.converted` named, while that member's integration branch still holds `.catervas/team.yaml` (`Git::has_file`), else `null`. Today shows, for each such row, "<name> still holds Catervas's old files. Merge the branch <branch> in <name> to remove them."
- **The page.** Step 07's one-repo view of Settings › Repositories gains "Add a repository", which opens `AddSecondRepository`, a `Dialog` in two parts:
  - the mockup's explanation; the blockers' alert from `project.convert_blockers`, in `work_in_progress`'s words; the project folder's place (`~/Catervas/<name>` with Change, `name` defaulted to `slug` of the repository folder's last segment); the first repository's name, defaulted by `repoNameOf`
  - then step 07's `FolderBrowser`, name and branch for the second

  Continue sends `project.convert`. On success the page shows "Moving your team's notes" and calls `reopen(10 * 60_000)`: `reopen` (`apps/web/src/app/connection.tsx:159`) gains an optional limit, default `REOPEN_FOR_MS`, since a large `local/` may take longer than 30 s to copy. It then lands on `/settings/repositories`. A refusal is shown in the dialog through `saidAll`; `WORDS` gains `not_one_repo` "This project has several repositories already.", `work_in_progress` (its sentence, filled as step 07's `fillOf` fills `repo_in_use`), `project_name` (step 07's `setupName` words), `folder_there` "A folder with that name is already there. Choose another name or place."

## File map

```
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}, crates/store/src/projections.rs   modifies: project.converted (Task 1)
crates/store/src/convert.rs, crates/store/src/lib.rs, crates/store/src/git.rs   creates, modifies, tests: convert_blockers, Git::blob_bytes (Task 2)
crates/runtime/src/daemon/team.rs, daemon/web.rs, daemon.rs, docs/schemas/rpc.schema.json, packages/protocol-client/src/client.ts   modifies, tests: project.convert, project.convert_blockers, WebState.converting (Task 2)
crates/cli/src/convert.rs, crates/cli/src/lib.rs    creates, modifies, tests: convert, its undo, converting (Task 3)
crates/cli/src/serve.rs, crates/cli/tests/serving.rs   modifies, tests: Driven::Converted, drive's said, the interrupts (Task 4)
crates/cli/src/setup.rs, crates/cli/src/project.rs, crates/store/src/repos.rs   modifies, tests: moved-to and converting read by found, project.open, open_project; member_root's has_team skip (Task 4)
crates/runtime/src/daemon/repos.rs                  modifies, tests: repos.list's moved_out (Task 4)
crates/runtime/src/catervafication.rs               modifies, tests: covered_repos after a move, the first system map of a member's own (Task 4)
crates/runtime/src/orchestrator/rules.rs            tests: a moved ready task assigned (Task 4)
apps/web/src/pages/settings/{Repositories.tsx,AddSecondRepository.tsx}, apps/web/src/pages/Today.tsx, apps/web/src/app/{refusals.ts,connection.tsx}, apps/web/src/strings/en.ts, tests   modifies, creates (Task 5)
docs/SPEC.md, docs/design/multi-repo-projects.md, docs/plans/phase-9-multi-repo-projects/{step-01,step-06}*.md   modifies (Task 6; the plans' notes now)
```

## Interfaces

Consumes:
- step 01's `create_folder`'s making of a project folder (its steps 1 and the README), `member_root`, `check_member`, `write_repos`, `repo.added`
- step 07's `Repositories`, `FolderBrowser`, `repoNameOf`
- step 02b's `rescan_members`, `CATERVAS_ACTOR`; step 06's `covered_repos`, `Scope`, the system row
- `project.leave`'s stop (`team.rs:1845`), `WebState`, `Driven`, `drive`, `found`, `remember`
- `copy_keys`, `ConnectorSecrets::delete`, `local_project_id`, `try_lock`
- `task_branch`, `is_a_document_path` (private at `crates/core/src/governor/readiness.rs:765`, made `pub` by Task 3), `changes_code`, `Git::prune_worktrees` (step 02c)
- `Git::tree_entries`, `has_file`, `has_local_branch`, `create_detached_worktree`, `remove_worktree`
- `ProjectFiles::read_contract`, `write_contract`, `summary_of`; the web app's `reopen`

Produces:

```rust
// catervas_runtime::daemon
// catervas_store::convert
pub struct Blockers { pub tasks: Vec<TaskId>, pub catervafication: bool, pub folder_changes: Vec<u64> }   // is_empty()
pub fn convert_blockers(files: &ProjectFiles, events: &[Event], git: &Git) -> Result<Blockers, FilesError>;
// Git gains `pub fn blob_bytes(&self, blob: &str) -> Result<Vec<u8>, GitError>`
pub struct Conversion { pub first_name: String, pub parent: String, pub name: String, pub second: MemberRequest }
// WebState gains `converting: Mutex<Option<Conversion>>`; DaemonState gains `pub fn converting(&self) -> Option<Conversion>`
// catervas_cli
enum Driven { Ended(i32), Left(PathBuf), Converted(PathBuf, Conversion) }
pub(crate) fn convert(old_root: &Path, conversion: &Conversion, io: &CliIo<'_>, now: DateTime<Utc>) -> Result<PathBuf, String>;
pub(crate) fn moved_to(root: &Path) -> Option<PathBuf>;
pub(crate) const MOVED_TO: &str = ".catervas/local/moved-to";
pub(crate) const CONVERTING: &str = ".catervas/local/converting";
pub(crate) fn converting_from(folder: &Path) -> Option<PathBuf>;
// drive(root, held, no_open, linked, io, said: Option<String>)
// event: project.converted { project_folder, first_repo, branch }, branch matching ^catervas/moved-out(-[1-9][0-9]*)?$; EVERY_KIND gains it
// rpc: project.convert (method), project.convert_blockers (query); repos.list rows gain moved_out
```

```ts
reopen(forMs?: number): void;   // connection.tsx, default REOPEN_FOR_MS
```

```ts
export function AddSecondRepository(props: { root: string; onClose: () => void }): JSX.Element;
```

## Tasks

### Task 1: The move in the log

Files: the event's files; `projections.rs`'s pass arm.

- `reads_a_project_converted_event`: `{ project_folder: "/home/me/Catervas/corner-bakery", first_repo: "corner-bakery", branch: "catervas/moved-out" }` reads and writes back unchanged. `first_repo: "Corner"` is refused at `/body/first_repo`, an empty `project_folder` at `/body/project_folder`, and `branch: "main"` at `/body/branch`. It is about no contract; `EVERY_KIND`'s length asserts move by one. RED.

- [x] `feat(protocol): record a project's move to a project folder`

### Task 2: Asking for the move

Files: `store/src/convert.rs`, `git.rs`, `team.rs`, `web.rs`, `daemon.rs`, `rpc.schema.json`, `client.ts`. Tests on a `TestDaemon` over a one-repo `TestProject` with home set (step 01's `set_home`).

- `counts_what_holds_the_move` (`store/src/convert.rs`): one row per counted state, `CTV-1` `in_progress`, `CTV-2` `rejected`, `CTV-3` `escalated` with its branch present (and `CTV-4` `escalated` with none, not counted), `CTV-5` `accepted` awaiting integration, `CTV-6` a `ready` task filed as `document_rewrite`, folder change 2 waiting, an open Catervafication epic; other `ready`, `refining`, `done` and integrated tasks are not counted. RED.
- `answers_what_holds_the_move` (`team.rs`): `project.convert_blockers` answers `{ tasks, catervafication, folder_changes }` from it; with nothing, empty and `false`. RED.
- `reads_a_blob_byte_for_byte` (`git.rs`): a committed file with bytes `0xff 0x00` reads back the same. RED.
- `refuses_a_move_in_order`. Each of these is refused by its code and pointer, nothing stored, and the run not stopped:
  - `not_one_repo` in a project folder
  - `work_in_progress` naming `CTV-1`
  - `name: "Shop!"` (`project_name`)
  - parent `../x` (`outside_home`)
  - `Catervas/corner-bakery` already there (`folder_there`)
  - `first_name: "Corner"` (`repo_name` at `/first_name`)
  - parent inside the old root (`holds_project_folder` at `/parent`)
  - `second` naming the current repository (`path_taken` at `/second/path`)
  - `second` with a team (`has_team` at `/second/path`)

  RED.
- `stops_the_run_for_the_move`: with nothing in the way, `project.convert` answers `{}`, `converting()` holds the asked conversion, and the run is stopped as `project_leave_stops_the_run_and_marks_the_project_left` (`web.rs:2849`) checks. RED.

- [x] `feat(runtime): ask to move a one-repo project into a project folder`

### Task 3: The move

Files: `convert.rs`; `lib.rs`; `readiness.rs` (`is_a_document_path` made `pub`). Tests on two `TempRepo`s under a temp home, the first a one-repo project made with `catervas init`, with `policy.integration_branch: trunk`. On `trunk` are `docs/catervas/product/spec.html` with `spec.agent.html`, `architecture/overview.md`, `architecture/features.html`, `architecture/plans/CTV-3.html` and `engineering/conventions.md`, a team of either format as main's HTML change allows. `docs/catervas/notes.md`, and a link `docs/catervas/engineering/latest.md` to `conventions.md`. Its tasks are `CTV-3` (a Developer's, `ready`, allowed `src/**`), `CTV-4` (an Architect's, `refining`, allowed `docs/catervas/architecture/overview.md`) and `CTV-5` (a Developer's, `ready`, allowed `docs/catervas/engineering/**`). It has a connector key.

- `moves_the_team_and_its_documents`: `convert` to `Catervas/bakery` with `first_name: "bakery"` and `second: web`:
  - It answers the project folder.
  - The folder's `.catervas/` holds the team, both contracts, the log with every old event, and no `local/worktrees`.
  - Its `main` holds `docs/catervas/product/spec.html` and `spec.agent.html`, `architecture/plans/CTV-3.html`, `repos/bakery/architecture/overview.md`, `repos/bakery/architecture/features.html` and `repos/bakery/engineering/conventions.md`, committed as "Move the team's documents from bakery".
  - `repos.yaml` names `bakery` (at the old root, `integration_branch: trunk`) and `web`.
  - `team.yaml` has no `policy.integration_branch`.
  - `docs/catervas/notes.md` is at the same path, `repos/bakery/engineering/latest.md` is a link to `conventions.md`, and a file holding bytes that are not UTF-8 moves byte for byte.
  - `CTV-3`'s contract has `repo: bakery` and a `contract.written` by `catervas`; `CTV-4`'s has no `repo` and allows `docs/catervas/repos/bakery/architecture/overview.md`; `CTV-5`'s has no `repo` and allows `docs/catervas/repos/bakery/engineering/**`.
  - There is no `.catervas/local/converting` left.
  - The log ends with two `repo.added` and `project.converted`.
  - The connector key reads under the project folder's id.
  - The library has no untagged `project_scan` entry, has `bakery`'s and `web`'s rescanned entries, and keeps the human's entry.
  - With a `catervas.db-wal` present before the copy, every event appended before the move is in the copied log.

  RED.
- `leaves_the_removal_for_the_owner`:
  - The old repository has the branch `catervas/moved-out`, one commit past `trunk`, removing every tracked path under `.catervas/` and `docs/catervas/`.
  - The old checkout's branch and files are unchanged.
  - `.catervas/local/moved-to` names the project folder.
  - With `catervas/moved-out` already there, the branch is `catervas/moved-out-2`.

  RED.
- `undoes_a_failed_move`: with a branch named `catervas` in the old repository, step 8 cannot make `catervas/moved-out` (git refuses the ref), so `convert` fails: the project folder is gone, the connector key under the project folder's id is deleted, and the old repository has no new branch, no `moved-to`, no `moving` worktree in `git worktree list`, and its log unchanged. RED.
- `counts_again_before_moving`: with `CTV-3` moved to `assigned` after the ask and before `convert`, `convert` fails with `work_in_progress`'s sentence naming `CTV-3`, and nothing is made. RED.
- `refuses_a_half_made_project_folder`: a project folder holding `converting` (the move stopped after step 9) is refused by `open_project` and `found`, a `moved-to` naming it is ignored, and `convert` asked again with the same place clears it, deletes the stale `catervas/moved-out`, and moves, its branch named `catervas/moved-out` again. With the stop after step 8's worktree was added (the folder removed, the worktree still registered), the retry moves too. An empty folder at the place is used. RED.

- [x] `feat(cli): move a one-repo project's team into a project folder`

### Task 4: The switch, and after it

Files: `serve.rs`; `setup.rs`; `project.rs`; `daemon/repos.rs`; `catervafication.rs`; `rules.rs`'s test; `tests/serving.rs`.

- `adds_a_second_repository_from_the_browser` (`tests/serving.rs`, as `changes_project_from_the_browser`, `:1884`): over the held port, `project.convert` is answered. `serve.status`'s `project_root` then becomes the project folder, `repos.list` answers both members, `tasks.list` still holds `CTV-3` with `repo: "bakery"`, the port and the browser's session are the same, and a Ctrl-C then ends serve. RED.
- `falls_back_to_the_repository_when_the_move_fails` (`tests/serving.rs`): with the `catervas` branch of `undoes_a_failed_move`, `serve.status` answers the old root as `project_root` and the sentence as `take_on_error`; after a `project.leave` and a take-on, it is `null`. RED.
- `opens_a_moved_repository_s_project_folder` (`setup.rs`, `project.rs`, `serve.rs`'s `found`): with `moved-to` naming a project folder, `open_project(old_root)` opens the project folder, `project.open` of the old root takes the project folder on as it is, and `found()` in the old root drives the project folder. A `moved-to` naming a folder that is no project folder, or one holding `converting`, is ignored. `member_root` of the old root, `moved-to` naming this project folder, passes `has_team`; naming another, it is refused. RED.
- `a_ready_task_still_goes_to_work` (`orchestrator`, on step 02's `Harness::project_folder_with` holding `bakery` and `web`, with `CTV-3`'s contract as the migration writes it and `project.converted` in the log): `CTV-3`, `ready` with its new `repo: bakery`, is assigned on the next tick, and its worktree is made from `bakery`'s integration branch, with no new judgment asked. RED.
- `covers_the_first_member_after_a_move` (`catervafication.rs`): with a `Repository`-scope `catervafication.started` and then `project.converted { first_repo: bakery }`, `covered_repos` is `{bakery}` and `offered_repos` is `[web]`. `catervafication_start { repo: "web" }` with no `system` document in either format on `main` files "Map how the repositories connect", depending on `web`'s Architect task. RED.
- `reminds_of_the_branch_to_merge` (`daemon/repos.rs`, on `TestProject`'s project folder with a `project.converted` naming `bakery` and the branch made in `bakery`): at first, `repos.list`'s `bakery` row has `moved_out: "catervas/moved-out"`. Once that branch is merged into its integration branch, it is `null`. `web`'s is `null`. RED.

Built differently, 2026-10-23:
- `covers_the_first_member_after_a_move` and the two changes to step 06 it holds (`covered_repos` after a move; a member's own Catervafication writing the system map first) are not built: step 06's `Scope`, `covered_repos`, `offered_repos` and `catervafication_start { repo }` were never built in phase 9, whose Catervafication hunts each member instead (`catervafication.rs`, `hunt_members`). There is nothing to teach about a move. A second member's own Catervafication waits for step 06.
- `project.converted`'s `branch` is `null` when the repository kept nothing in git (no tracked path under `.catervas/` or `docs/catervas/`), so that no empty branch is made; `repos.list`'s `moved_out` is then `null` too.
- `tasks.list` rows carry no `repo`; `adds_a_second_repository_from_the_browser` reads it in the moved contract file.
- `member_root` gained `member_root_in(home, path, project_folder)`, which `repo.add`, `repo.locate` and `online` call with the project folder; the plain `member_root` stays for the callers that name none.

- [x] `feat(cli): drive the project folder a repository moved into`

### Task 5: The dialog and Today's reminder

Files: `Repositories.tsx`; `AddSecondRepository.tsx`; `Today.tsx`; `refusals.ts`; `en.ts`; the tests.

- `adds_a_second_repository` (`settings.test.tsx`):
  - In a one-repo project, "Add a repository" opens the dialog with the mockup's three points, `~/Catervas/corner-bakery` and `corner-bakery` as the first name.
  - With `project.convert_blockers` answering `CTV-1`, the alert names it and Continue is disabled.
  - With none, Continue shows the folder browser. Picking `Projects/web` and Continue sends `project.convert { first_name: "corner-bakery", parent: "Catervas", name: "corner-bakery", second: { name: "web", path: "Projects/web" } }`, which `refusedBy("projectConvertRequest", ..)` accepts.
  - Then it shows "Moving your team's notes" and reconnects.

  RED.
- `shows_a_refused_move`: a `work_in_progress` refusal shows its words in the dialog, and the dialog stays; `folder_there` and `not_one_repo` read their words. RED.
- `shows_a_failed_move` (`settings.test.tsx`): with `serve.status`'s `take_on_error` set, Settings › Repositories shows "Catervas could not add the repository: …". RED.
- `reopens_for_longer_when_asked` (`connection.test.tsx`): `reopen(600_000)` keeps retrying past 30 s; `reopen()` stops at 30 s as today. RED.
- `reminds_of_the_branch_to_merge` (`Today.test.tsx`): a `repos.list` row with `moved_out: "catervas/moved-out"` shows the reminder naming the repository and the branch. `null` shows none. RED.

- [ ] `feat(web): add a second repository to a one-repo project`

### Task 6: Spec

`docs/SPEC.md`, at the next revision:
- 4.4: adding a second repository, its blockers, what moves and what is left to merge, the reminder
- 8.4: `moved-to`, and a moved repository opening its project folder
- 8.5: `project.converted`
- the section that lists the web app's methods and queries: `project.convert`, `project.convert_blockers`, `repos.list`'s `moved_out`, and the new codes

`docs/design/multi-repo-projects.md`'s "A one-repo project gains a second repository" is changed: the removal is a branch for the owner to merge, not a change integrated by the team's policy; the move waits for work in progress; its Events list gives `project.converted` its `branch`.

- [ ] `docs(spec): record a one-repo project gaining a second repository`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; includes pnpm check
# expected: xtask check: ok
```
