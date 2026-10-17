# Standalone internal-task-ids, plan 2: a task's branch and pull request

Status: draft
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.11, 5.14 (recorded by plan 6)
Depends on: plan 1 of this branch
Readiness confirmed by: the planner, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

When a task starts, Catervas names its branch from its type and title (`feat/add-a-login-page`),
records it in the contract once, and every reader of a task's branch reads that record. A task's
pull request is titled `Feat: Add a login page`. Its body names no task id. The merge commit
Catervas makes when a task starts from two branches says `chore: start from <branch>`. A bug a
hunt files is a fix. Tasks started before this change keep their `CTV` branches.

Out of scope: agents' commit messages (plan 3), folder changes (plan 4), the Files page and the
task page (plan 5).

## Decisions

- The branch is chosen in rule 7 (`assigned` in `orchestrator/rules.rs`), only when the worktree
  is not there yet, after the task's repository is resolved and before `start_worktree`. A worktree
  that is already there keeps `task_branch`'s answer. A contract that records a `branch` keeps it,
  so a start undone by a conflict makes the same branch on the next try.
- With no `branch` recorded, the name is `free_branch` with `change_type(contract)`,
  `branch_slug(contract.title)`, and every taken name. A task started before this change has its
  worktree already and never reaches this. An `assigned` task whose worktree is gone while an old
  branch of its name is there fails at `create_worktree` exactly as it does today; that is not
  changed here.
- The taken names are every branch of the task's repository (the member's git for a task that names a
  `repo`, ADR 0057), local and remote-tracking, each
  without its `refs/heads/` or `refs/remotes/<remote>/` (a remote's `HEAD` left out), and every
  other contract's recorded `branch`. A contract that cannot be read is passed over. Plan 4 adds
  the folder changes' branches.
- The record is written by `Transitions::record_branch` under `Transitions`' own `requests` lock,
  on the contract as the file holds it then, as `reassign` writes `assignee`. No event is recorded
  (ADR 0066).
- `catervas_file_bug` puts `change: fix` in the wire it files, after `request_from_brief`.
- The human's save (`judged` in `daemon/gates.rs`) drops `"branch"` from the fields it changed,
  before the governor's check. `written_over` writes over the file as it is now, so the recorded
  branch stays.
- A pull request's title is `pull_request_title(change_type(&contract), &row.title, &title_of)`.
  The intent and both notes in its body each go through `without_task_ids`. `title_of` answers the
  board's title of a task (`projections.task(id)`), `None` for one it does not know or cannot
  read. The merge message `Subject::titles` also built has had no reader since 0.115 and goes, so
  `titles` becomes `title`.
- A folder change's title is unchanged here; plan 4 changes it.
- Tests that spell an old name (`feature/CTV-1`) for a task that rule 7 started now read
  `harness.branch(task)` (it reads the contract). A test whose task's worktree was made by a
  fixture keeps the old name, which `task_branch` still answers.

## File map

```
crates/store/src/git.rs                         modifies: Git::branch_names (Task 1)
crates/store/src/branches.rs                    creates:  taken_branches (Task 1)
crates/store/src/lib.rs                         modifies: pub mod branches (Task 1)
crates/runtime/src/transitions.rs               modifies: Transitions::record_branch (Task 1)
crates/runtime/src/orchestrator/rules.rs        modifies: assigned, chosen_branch, start_worktree's merge message (Task 1)
crates/runtime/src/orchestrator/rules.rs        tests:    starts_an_assigned_task_in_its_worktree, starts_a_document_task_on_a_docs_branch (Task 1)
crates/runtime/src/orchestrator/*.rs, crates/runtime/src/daemon/*.rs   tests: old names of tasks rule 7 started read harness.branch (Task 1)
crates/runtime/src/tools/bugs.rs                modifies: file_bug's wire sets change: fix; tests: files_a_bug_with_its_evidence (Task 2)
crates/runtime/src/daemon/gates.rs              modifies: judged drops "branch"; tests: a_save_keeps_the_recorded_branch (Task 3)
crates/runtime/src/orchestrator/integrate.rs    modifies: Subject::title, pull_request_body (Task 4)
crates/runtime/src/orchestrator/integrate.rs    tests:    a_tasks_pull_request_carries_no_task_id (Task 4)
```

## Interfaces

Consumes: `ChangeType`, `change_type`, `branch_slug`, `free_branch`, `old_task_branch`,
`task_branch` (plan 1, `catervas_core::branch`); `pull_request_title`, `without_task_ids` (plan 1,
`catervas_core::forge_text`); the contract's `branch` (plan 1);
`ProjectFiles::list_contracts`, `read_contract` (store); `TaskRepository` (`catervas_store::repos`).

Produces:

```rust
// crates/store/src/git.rs
impl Git {
    /// Every branch name, local and remote-tracking, without `refs/heads/` or
    /// `refs/remotes/<remote>/`; a remote's `HEAD` is left out.
    pub fn branch_names(&self) -> Result<Vec<String>, GitError>;
}
// crates/store/src/branches.rs
/// The branch names a new task's or folder change's branch may not take (ADR 0066): `git`'s
/// branches and every contract's recorded `branch` but `except`'s. Git's or the files' failure is
/// `StoreError::Io` with its words.
pub fn taken_branches(
    files: &ProjectFiles, git: &Git, except: Option<&TaskId>,
) -> Result<Vec<String>, StoreError>;
// crates/runtime/src/transitions.rs
impl Transitions {
    /// Records `branch` as the task's, unless its contract already records one; answers the
    /// recorded one either way. `Files` when the contract cannot be read or written, or `branch`
    /// is not one the schema takes.
    pub fn record_branch(&self, task_id: &TaskId, branch: &str) -> Result<String, TransitionError>;
}
// crates/runtime/src/orchestrator/rules.rs (private)
fn chosen_branch(
    deps: &OrchestratorDeps, contract: &TaskContract, repository: &TaskRepository,
) -> Result<String, OrchestratorError>;
// crates/runtime/src/orchestrator/integrate.rs (private)
impl Subject { fn title(&self, tools: &ToolDeps) -> Result<String, OrchestratorError>; }
```

Plan 4 adds a `log: &EventLog` parameter to `taken_branches`, to read the folder changes' branches,
and updates `chosen_branch`, its one caller.

## Tasks

### Task 1: a task's branch is chosen at its start and recorded

Files: modified `crates/store/src/git.rs`, `crates/store/src/lib.rs`,
`crates/runtime/src/transitions.rs`, `crates/runtime/src/orchestrator/rules.rs`; created
`crates/store/src/branches.rs`; tests updated where they spell an old name for a task rule 7
started.
Produces: `Git::branch_names`, `taken_branches`, `Transitions::record_branch`, `chosen_branch`.
Consumes: plan 1's names.

`assigned`'s worktree arm becomes:
- the worktree is there: `task_branch(&contract)`;
- else: resolve the repository (as now, with its `Missing` wait), then `chosen_branch`, then
  `start_worktree` with that name.

The report still says "in its worktree on <branch>". `start_worktree`'s merge message becomes
`format!("chore: start from {from}")`.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `starts_an_assigned_task_in_its_worktree` (changed): before the tick, a branch
  `feat/add-a-login-page` is made from `main` at the root (the fixture's title is "Add a login
  page"). After the tick:
  - the worktree's `HEAD` is `feat/add-a-login-page-2`;
  - the contract file's `branch` is that name;
  - `harness.branch("CTV-1")` is that name.
  The rest of its assertions stay. It fails first on `feature/CTV-1`.
- `starts_a_document_task_on_a_docs_branch` (changed): the worktree's `HEAD` is
  `docs/add-a-login-page`.

- [x] `feat(runtime): a task's branch is named from its title at its start and recorded`

### Task 2: a bug a hunt files is a fix

Files: modified `crates/runtime/src/tools/bugs.rs`.
Consumes: nothing new.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `files_a_bug_with_its_evidence` (changed): the filed contract's `change` is `fix` as well.

- [ ] `feat(runtime): a bug a hunt files is a fix`

### Task 3: the human's save keeps the recorded branch

Files: modified `crates/runtime/src/daemon/gates.rs`.
Consumes: the contract's `branch` (plan 1).

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `a_save_keeps_the_recorded_branch`:
  - a `refining` task's contract file records `branch: feat/add-a-login-page`, written with
    `files.write_contract`;
  - the human's `contract.save` sends `contract.get`'s answer with `branch` removed and `intent`
    changed;
  - the answer is `saved: true`;
  - the file then holds the new intent and still records `feat/add-a-login-page`.

  Before the change it fails with the store's-fields refusal.

- [ ] `fix(runtime): the human's save keeps the branch Catervas recorded`

### Task 4: a task's pull request carries no task id

Files: modified `crates/runtime/src/orchestrator/integrate.rs`.
Produces: `Subject::title`.
Consumes: `change_type` (plan 1), `pull_request_title`, `without_task_ids` (plan 1), Task 1's record.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `a_tasks_pull_request_carries_no_task_id`, beside `opens_a_pull_request_for_an_accepted_task`
  and built like it (`under(.., "pull_request")`, `with_origin`, the fake `gh`):
  1. `CTV-2` is filed `cancelled` with the title "Pie pre-orders", so the board knows its title
     and no rule acts on it.
  2. `CTV-1` is filed `ready` with the title "Character counter on the note input" and moved to
     `assigned` to `dev-a`, reviewed by `dev-b`.
  3. One tick starts it (rule 7).
  4. In its worktree, `counter.txt` is committed with the message
     `feat: count the note's characters`.
  5. `note.written` records a completion note "Added the counter; CTV-2 still owes the pies." and
     a review note "C1 passes.".
  6. It is moved `in_progress` → `verifying` → `accepted`, its delivery settled and its worktree
     removed, as `accepted_adding` does.
  7. `gh` answers `list` with `[]` and `create` with the pull request's URL, and one more tick
     runs.

  It asserts:
  - `harness.branch("CTV-1")` is `feat/character-counter-on-the-note-input`, and `origin` holds
    that branch at the root's commit;
  - the `create` call passes `--head` with that branch and `--title` with
    `Feat: Character counter on the note input`;
  - the body (`stdin_of("create")`) holds `"Pie pre-orders" still owes the pies` and no `CTV-`;
  - `git log --format=%B main..<branch>` at the root holds no `CTV-`;
  - the one `pull_request.opened` names that branch.

  It fails first on the title, `CTV-1: Character counter on the note input`.
- `opens_a_pull_request_for_an_accepted_task` and the other pull request tests stay as they are.
  Their tasks' worktrees are made by fixtures, on old branches, which is the old task's path.

- [ ] `feat(runtime): a task's pull request is titled by its type and names no task id`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok (the git tests above are #[ignore]d without it)
```
