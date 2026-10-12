# Phase 9, step 07b: A one-repo project gains a second repository

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.4, 8.4, 8.5; F1
Depends on these steps of this phase, planned (01 to 06 ready, 07 drafted), not yet executed:
- step 01: the project folder, `repos.yaml`, `member_root`, `check_member`, the member refusals, `repos.list`, `repo.added`, `home_of`
- step 02: the contract's `repo`, `task_repository`
- step 04: member folders (`docs/catervas/repos/<name>/<segment>/`)
- step 07: `Repositories.tsx`'s one-repo view, `FolderBrowser` in a dialog, `repoNameOf`

Nothing of step 08.
Mockups: `docs/design/mockups/AddSecondRepository.dc.html` on the canvas page "Multi-repo projects", drawn 2026-10-12 (c179d5b); approved by the founder on 2026-10-12 ("Approve", ADR 0032), with what it draws: the old files left on `catervas/moved-out` for the owner to merge, and the move waiting while work is in progress.
Readiness confirmed by: (pending; one round)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at c179d5b.

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

- **The word.** `project.convert { first_name, parent, name, second: { name, path, integration_branch? } }`, a method answered in drive mode (`crates/runtime/src/daemon/team.rs` `METHODS`, `:58`), whose result is `emptyResult`.
  - `first_name` names the current repository as a member.
  - `parent` and `name` place the project folder, as `project.create_folder`'s do (step 01, including its `..` and missing-parent rules).
  - `second` is the repository to add, with its path relative to home.
- **Its refusals, in order**, each `REFUSED` with `data.errors: [{ path, message, code }]` (the codes are new unless noted):
  - `not_one_repo` (`/`): the project is a project folder already.
  - `work_in_progress` (`/`): "Your team is working on <n> tasks in <name>: <ids>. Let them finish, or cancel them, before you add a repository." It counts any task `assigned`, `in_progress`, `blocked` or `verifying`, or `accepted` and awaiting integration, and any folder change not integrated and not escalated. Their worktrees and branches belong to this repository's git, and nothing moves them.
  - `repo_name` (`/first_name`).
  - Step 01's parent checks (`/parent`): `outside_home`, and "a folder with that name is already there".
  - Step 01's member checks for `second` (`/second/...`), against the project folder's path and the current repository as the other member, `has_team` among them.
  - `no_account` is not checked: the account is already connected.
- **The page can ask first.** The query `project.convert_blockers {}` answers `{ tasks: [ids], folder_changes: [numbers] }`, what `work_in_progress` would count, so that the dialog can say it before the owner fills anything in (the mockup's alert). Continue is disabled while either is not empty.
- **The switch.** With every check passed, the method stores the asked conversion in `WebState.converting: Mutex<Option<Conversion>>` (`crates/runtime/src/daemon/web.rs:42-76`) and stops the run exactly as `project.leave` does (`team.rs:1845-1858`). `serve.rs`'s drive loop, finding `converting` set when the driver ends (`:302`), answers `Driven::Converted(PathBuf, Conversion)` in place of `Left`.

### What the move does

`crates/cli/src/convert.rs`'s `convert(old_root, conversion, io, now) -> Result<PathBuf, String>` runs between the two drives, with nothing driving either project:

1. **Make the project folder** as `create_folder` does (step 01): `mkdir -p` of the parent, then `git init -b main`, the README, and its commit "Start the project".
2. **Copy `.catervas/`** from `old_root` to the project folder, links never followed, leaving out `local/worktrees/`, `local/run.lock`, `local/daemon.json`, `local/integration.lock` and `local/setup-pending`. The log, contracts, sprints, team, memory, decisions, local settings and the private folders all move with it.
   - The copy runs after the driver has ended and every handle on the log is dropped.
   - The log's `catervas.db` is copied together with its `catervas.db-wal` and `catervas.db-shm` when they are there, so the copy holds every appended event; SQLite replays the WAL when the copy is opened.
3. **Copy the connector keys and the Procurement mailbox** with carry's `copy_keys` and `carry_mailbox` (`crates/cli/src/carry.rs`), from the old root's project id to the project folder's (`local_project_id`, `connectors.rs:88`). No `keys-copied.json` notice is written: the team and its keys are the same.
4. **Move the documents**, read from the old repository's integration branch (`Git::tree_entries` and `file_at`), into the project folder's `docs/catervas/`:
   - `product/`, `delivery/`, `marketing/` and `architecture/plans/` to the same paths
   - the rest of `architecture/`, `engineering/` and `design/` to `repos/<first_name>/<same path>`

   They are committed there as "Move the team's documents from <first_name>". A document in the old checkout that is not on the integration branch is not moved: the work-in-progress check means none is waiting.
5. **Write `repos.yaml`**:
   - the first member: `first_name`, the old root's canonical path, and the team's `policy.integration_branch` when it was set
   - the second member, as asked

   In the copied `team.yaml`, `policy.integration_branch` is cleared: it now names the project folder's own branch, which is `main` (ADR 0057's amendment).
6. **Give the builders' open tasks their repository.** Every task whose assignee changes code (`changes_code`) and that is neither cancelled nor integrated gains `repo: <first_name>`. Its contract file is written directly, as a migration, and `contract.written { written_by: "catervas" }` is recorded for it. This bypasses the freeze, since its work has not moved (`draft`, `refining`, `ready` or in the Backlog, by the check above). Rejected: leaving them to `RepoRequired`, which would send every ready builder task back to refining.
7. **Record, in the copied log:** `repo.added` for each member, then `project.converted { project_folder, first_repo, branch }`.
7b. **Rescan the checks.** The copied criteria library holds the old repository's scanned entries with no `repo` (step 02b). Left as they are, they would expand into a task in any member. So the move drops every `source: project_scan` entry with no `repo`, keeps the human's entries, and then calls step 02b's `rescan_members(.., None, CATERVAS_ACTOR)`. That writes each member's entries, `<first_name>-<name>` among them, records `project.scanned` and `criteria.updated`, and rewrites `project.md`. A contract that already expanded an old entry keeps it: the command runs in its task's member, the first, where it was found.
8. **Leave the removal for the owner.**
   - In the old repository, from its integration branch, Catervas makes the branch `catervas/moved-out` (or `catervas/moved-out-<n>`, the first free name) with one commit, "Remove Catervas's files: the team moved to <project folder>". The commit removes every tracked path under `.catervas/` and `docs/catervas/`.
   - It works in a temporary detached worktree under the project folder's `.catervas/local/moving/`, so the owner's checkout is not touched, and removes that worktree afterwards.
   - It is neither pushed nor merged, under any policy: the old project is driven no more, and nothing else would integrate it.
9. **Mark the old repository.** It writes `<old_root>/.catervas/local/moved-to` holding the project folder's path. `local/` is ignored by git.
10. **Drive the project folder,** `remember` it, and serve goes to `Mode::Drive(project_folder)` on the same port.

**A failure** in steps 1 to 7 removes the project folder made so far, leaves the old repository untouched, and goes back to `Mode::Drive(old_root)`, with the sentence in `WebState.take_on_error`. Settings › Repositories shows it, the page saying "Catervas could not add the repository: <sentence>". A failure in steps 8 or 9 keeps the move, and the sentence is shown in the same place once the project folder is driven.

### After the move

- **Readiness and approvals stand.** The migration's `contract.written` does not send a ready task back. A `ready` task's next move, to `assigned`, is gated by the assignment (`transition_table.rs`: `Ready → Assigned`, `G::Assignment`), not by readiness again. The judgment (`judgment_since_written`, `transitions.rs:1471`) is read only for `refining → ready`. An approval counts from the last move into `refining` (`catervafication.rs` `start`'s comment). A `refining` task's judgment is asked again after the write, as after any write.
- **An earlier Catervafication covers the first member.** If the one-repo project was Catervafied, its `catervafication.started` has no `repos` and no `repo` (step 06's `Scope::Repository`). After a `project.converted`, step 06's `covered_repos` counts a `Repository`-scope start as covering that event's `first_repo`, and as the project-wide start, so `offered_repos` offers the second member alone. This is a change to step 06's `covered_repos`, made in this step's Task 4.
- **A member's own Catervafication writes the system map the first time.** When `docs/catervas/architecture/system.md` is not on the project folder's integration branch, step 06's member-scope system row is the project-wide one, "Map how the repositories connect", in place of "Bring the system map up to date with <name>". It depends on that member's architecture task alone. Also changed in this step's Task 4.

- **A moved repository opens its project folder.** Each of these reads `.catervas/local/moved-to` first and uses the folder it names, when that folder is a project folder:
  - `found()` (`serve.rs:122`)
  - `project.open` (`setup.rs:169`), which then opens the project folder as it is
  - `open_project` (`project.rs:127`)

  A command run in the old repository is told, once on its standard error, "this repository is part of the project at <folder> now".
- **Today reminds the owner.** `repos.list`'s rows gain `moved_out: string | null`: the branch `project.converted` named, while that member's integration branch still holds `.catervas/team.yaml` (`Git::has_file`), else `null`. Today shows, for each such row, "<name> still holds Catervas's old files. Merge the branch <branch> in <name> to remove them." Nothing else.
- **The page.** Step 07's one-repo view of Settings › Repositories gains "Add a repository", which opens `AddSecondRepository`, a `Dialog` in two parts:
  - the mockup's explanation, the blockers' alert from `project.convert_blockers`, the project folder's place (`~/Catervas/<name>` with Change) and the first repository's name (defaulted by `repoNameOf`)
  - then step 07's `FolderBrowser`, name and branch for the second

  Continue sends `project.convert`. On success the page shows "Moving your team's notes" and `reopen()`s, as `ChangeProject` does (`apps/web/src/pages/ChangeProject.tsx`), then lands on `/settings/repositories`. A refusal is shown in the dialog through `saidAll`, with `WORDS` gaining `not_one_repo` and `work_in_progress`.

## File map

```
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}, crates/store/src/projections.rs   modifies: project.converted (Task 1)
crates/runtime/src/daemon/team.rs, daemon/web.rs, daemon.rs, docs/schemas/rpc.schema.json, packages/protocol-client/src/client.ts   modifies, tests: project.convert, project.convert_blockers, WebState.converting (Task 2)
crates/cli/src/convert.rs, crates/cli/src/lib.rs    creates, modifies, tests: convert (Task 3)
crates/cli/src/serve.rs, crates/cli/tests/serving.rs   modifies, tests: Driven::Converted, the switch (Task 4)
crates/cli/src/setup.rs, crates/cli/src/project.rs  modifies, tests: moved-to read by found, project.open, open_project (Task 4)
crates/runtime/src/daemon/repos.rs                  modifies, tests: repos.list's moved_out (Task 4)
crates/runtime/src/catervafication.rs               modifies, tests: covered_repos after a move, the first system map of a member's own (Task 4)
crates/runtime/src/orchestrator/rules.rs            tests: a moved ready task assigned (Task 4)
apps/web/src/pages/settings/{Repositories.tsx,AddSecondRepository.tsx}, apps/web/src/pages/Today.tsx, apps/web/src/app/refusals.ts, apps/web/src/strings/en.ts, tests   modifies, creates (Task 5)
docs/SPEC.md                                        modifies (Task 6)
```

## Interfaces

Consumes:
- step 01's `create_folder`'s making of a project folder (its steps 1 and the README), `member_root`, `check_member`, `write_repos`, `repo.added`
- step 07's `Repositories`, `FolderBrowser`, `repoNameOf`
- step 02b's `rescan_members`, `CATERVAS_ACTOR`; step 06's `covered_repos`, `Scope`, the system row
- `project.leave`'s stop (`team.rs:1845`), `WebState`, `Driven`, `drive`, `found`, `remember`
- `copy_keys`, `carry_mailbox`, `local_project_id`
- `Git::tree_entries`, `file_at`, `has_file`, `create_detached_worktree`, `remove_worktree`
- `changes_code`, `ProjectFiles::read_contract`, `write_contract`, `summary_of`

Produces:

```rust
// catervas_runtime::daemon
pub struct Conversion { pub first_name: String, pub parent: String, pub name: String, pub second: MemberRequest }
// WebState gains `converting: Mutex<Option<Conversion>>`; DaemonState gains `pub fn converting(&self) -> Option<Conversion>`
// catervas_cli
enum Driven { Ended(i32), Left(PathBuf), Converted(PathBuf, Conversion) }
pub(crate) fn convert(old_root: &Path, conversion: &Conversion, io: &CliIo<'_>, now: DateTime<Utc>) -> Result<PathBuf, String>;
pub(crate) fn moved_to(root: &Path) -> Option<PathBuf>;
pub(crate) const MOVED_TO: &str = ".catervas/local/moved-to";
// event: project.converted { project_folder, first_repo, branch }
// rpc: project.convert (method), project.convert_blockers (query); repos.list rows gain moved_out
```

```ts
export function AddSecondRepository(props: { root: string; onClose: () => void }): JSX.Element;
```

## Tasks

### Task 1: The move in the log

Files: the event's files; `projections.rs`'s pass arm.

- `reads_a_project_converted_event`: `{ project_folder: "/home/me/Catervas/corner-bakery", first_repo: "corner-bakery", branch: "catervas/moved-out" }` reads and writes back unchanged. `first_repo: "Corner"` is refused at `/body/first_repo`, and an empty `project_folder` at `/body/project_folder`. It is about no contract. RED.

- [ ] `feat(protocol): record a project's move to a project folder`

### Task 2: Asking for the move

Files: `team.rs`, `web.rs`, `daemon.rs`, `rpc.schema.json`, `client.ts`. Tests on a `TestDaemon` over a one-repo `TestProject` with home set (step 01's `set_home`).

- `counts_what_holds_the_move`: with `CTV-1` `in_progress` and folder change 2 waiting, `project.convert_blockers` answers `{ tasks: ["CTV-1"], folder_changes: [2] }`; with neither, both empty. RED.
- `refuses_a_move_in_order`. Each of these is refused by its code and pointer, nothing stored, and the run not stopped:
  - `not_one_repo` in a project folder
  - `work_in_progress` naming `CTV-1`
  - `first_name: "Corner"`
  - parent `../x`
  - `second` naming the current repository (`path_taken`)
  - `second` with a team (`has_team`)

  RED.
- `stops_the_run_for_the_move`: with nothing in the way, `project.convert` answers `{}`, `converting()` holds the asked conversion, and the run is stopped as `project_leave_stops_the_run_and_marks_the_project_left` (`web.rs:2823`) checks. RED.

- [ ] `feat(runtime): ask to move a one-repo project into a project folder`

### Task 3: The move

Files: `convert.rs`; `lib.rs`. Tests on two `TempRepo`s under a temp home, the first a one-repo project made with `catervas init`. On its `main` are `docs/catervas/product/spec.md`, `architecture/overview.md`, `architecture/plans/CTV-3.md` and `engineering/conventions.md`. Its tasks are `CTV-3` (a Developer's, `ready`) and `CTV-4` (an Architect's, `refining`). It has a connector key and `policy.integration_branch: trunk`.

- `moves_the_team_and_its_documents`: `convert` to `Catervas/bakery` with `first_name: "bakery"` and `second: web`:
  - It answers the project folder.
  - The folder's `.catervas/` holds the team, both contracts, the log with every old event, and no `local/worktrees`.
  - Its `main` holds `docs/catervas/product/spec.md`, `architecture/plans/CTV-3.md`, `repos/bakery/architecture/overview.md` and `repos/bakery/engineering/conventions.md`, committed as "Move the team's documents from bakery".
  - `repos.yaml` names `bakery` (at the old root, `integration_branch: trunk`) and `web`.
  - `team.yaml` has no `policy.integration_branch`.
  - `CTV-3`'s contract has `repo: bakery` and a `contract.written` by `catervas`; `CTV-4`'s has no `repo`.
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
- `undoes_a_failed_move`: with the second repository made unreadable after the checks, `convert` fails, the project folder is gone, and the old repository has no new branch and no `moved-to`. RED.

- [ ] `feat(cli): move a one-repo project's team into a project folder`

### Task 4: The switch, and after it

Files: `serve.rs`; `setup.rs`; `project.rs`; `daemon/repos.rs`; `catervafication.rs`; `rules.rs`'s test; `tests/serving.rs`.

- `adds_a_second_repository_from_the_browser` (`tests/serving.rs`, as `changes_project_from_the_browser`, `:1884`): over the held port, `project.convert` is answered. `serve.status`'s `project_root` then becomes the project folder, `repos.list` answers both members, `tasks.list` still holds `CTV-3` with `repo: "bakery"`, and the port and the browser's session are the same. RED.
- `opens_a_moved_repository_s_project_folder` (`setup.rs`, `project.rs`, `serve.rs`'s `found`): with `moved-to` naming a project folder, `open_project(old_root)` opens the project folder, `project.open` of the old root takes the project folder on as it is, and `found()` in the old root drives the project folder. A `moved-to` naming a folder that is no project folder is ignored. RED.
- `a_ready_task_still_goes_to_work` (`orchestrator`, `Harness` over the moved project folder): `CTV-3`, `ready` with its new `repo: bakery`, is assigned on the next tick, and its worktree is made from `bakery`'s integration branch, with no new judgment asked. RED.
- `covers_the_first_member_after_a_move` (`catervafication.rs`): with a `Repository`-scope `catervafication.started` and then `project.converted { first_repo: bakery }`, `covered_repos` is `{bakery}` and `offered_repos` is `[web]`. `catervafication_start { repo: "web" }` with no `system.md` on `main` files "Map how the repositories connect", depending on `web`'s Architect task. RED.
- `reminds_of_the_branch_to_merge` (`daemon/repos.rs`): after the move, `repos.list`'s `bakery` row has `moved_out: "catervas/moved-out"`. Once that branch is merged into its integration branch, it is `null`. `web`'s is `null`. RED.

- [ ] `feat(cli): drive the project folder a repository moved into`

### Task 5: The dialog and Today's reminder

Files: `Repositories.tsx`; `AddSecondRepository.tsx`; `Today.tsx`; `refusals.ts`; `en.ts`; the tests.

- `adds_a_second_repository` (`settings.test.tsx`):
  - In a one-repo project, "Add a repository" opens the dialog with the mockup's three points, `~/Catervas/corner-bakery` and `corner-bakery` as the first name.
  - With `project.convert_blockers` answering `CTV-1`, the alert names it and Continue is disabled.
  - With none, Continue shows the folder browser. Picking `Projects/web` and Continue sends `project.convert { first_name: "corner-bakery", parent: "Catervas", name: "corner-bakery", second: { name: "web", path: "Projects/web" } }`, which `refusedBy("projectConvertRequest", ..)` accepts.
  - Then it shows "Moving your team's notes" and reconnects.

  RED.
- `shows_a_refused_move`: a `work_in_progress` refusal shows its words in the dialog, and the dialog stays. RED.
- `reminds_of_the_branch_to_merge` (`Today.test.tsx`): a `repos.list` row with `moved_out: "catervas/moved-out"` shows the reminder naming the repository and the branch. `null` shows none. RED.

- [ ] `feat(web): add a second repository to a one-repo project`

### Task 6: Spec

`docs/SPEC.md`, at the next revision:
- 4.4: adding a second repository, its blockers, what moves and what is left to merge, the reminder
- 8.4: `moved-to`, and a moved repository opening its project folder
- 8.5: `project.converted`

`docs/design/multi-repo-projects.md`'s "A one-repo project gains a second repository" is changed: the removal is a branch for the owner to merge, not a change integrated by the team's policy; the move waits for work in progress.

- [ ] `docs(spec): record a one-repo project gaining a second repository`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; includes pnpm check
# expected: xtask check: ok
```
