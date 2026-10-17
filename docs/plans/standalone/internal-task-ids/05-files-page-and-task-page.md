# Standalone internal-task-ids, plan 5: the Files page and the task page

Status: draft
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.17, 5.14 (recorded by plan 6)
Depends on: plans 1, 2 and 4 of this branch
Readiness confirmed by: the planner, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The Files page still names who last changed a document when the merge that brought the change in
names no task id: GitHub's "Merge pull request #12 from me/feat/add-a-login-page", or a squash
"Feat: Add a login page (#12)". The task page shows the branch Catervas recorded, read from
`task.diff`, rather than working out the old name itself.

## Decisions

- `Named` (`crates/core/src/folders.rs`) gains `PullRequest(u64)`.
- `named_in_subject` keeps its scan for `CTV-<n>`, `folder-<n>` and `Folder change <n>` first, so
  an older history reads as before. When that finds nothing, it answers `PullRequest(n)` for a
  subject that starts with `Merge pull request #<n>`, or whose end, after trimming whitespace, is
  `(#<n>)`. `n` is one or more ASCII digits that parse as a `u64`. Anything else is `None`.
- `last_change` (`crates/runtime/src/daemon/documents.rs`) turns `PullRequest(n)` into what it
  names, then answers as the existing arms do:
  - read the log's `pull_request.opened` and `folder_change.opened`, newest first;
  - the first event whose `number` is `n` decides. A `pull_request.opened` counts only when the
    contract of its envelope's task names no `repo`: the Files page reads the project folder, and a
    member's pull request numbers are the member's;
  - `pull_request.opened` gives `Named::Task(<its task>)`, `folder_change.opened` gives
    `Named::FolderChange(<its change>)`, and no match gives what `None` gives today.
- `TaskDiff` (`crates/store/src/diff.rs`) gains `pub branch: Option<String>`:
  - `task_branch(contract)` for a task that has a branch, whatever its status;
  - `None` for an epic and for a task in a private folder.
- `task.diff` (`daemon/gates.rs` `task_diff`) answers `branch` when it is `Some`.
  `rpc.schema.json`'s `taskDiffResult` gains `branch`: a string, not required, described as "The
  task's branch, as Catervas recorded it at the task's start (ADR 0066); absent for an epic and a
  task in a private folder."
- The task page (`apps/web/src/pages/TaskDetail.tsx`):
  - drops its own copy of the rule (the `branch` constant and its comment);
  - its `Diff` type gains `branch?: string`;
  - the Changes line reads `diff.branch ?? ""`.
  Gate.tsx's own `Diff` type is not changed, since it shows no branch.

## File map

```
crates/core/src/folders.rs                  modifies: Named::PullRequest, named_in_subject (Task 1)
crates/core/src/folders.rs                  tests:    the named_in_subject test gains the two forms (Task 1)
crates/runtime/src/daemon/documents.rs      modifies: last_change, named_by_pull_request (Task 1)
crates/runtime/src/daemon/documents.rs      tests:    names_who_made_a_change_merged_by_pull_request (Task 1)
crates/store/src/diff.rs                    modifies: TaskDiff.branch (Task 2)
crates/runtime/src/daemon/gates.rs          modifies: task_diff answers branch; tests: task_diff_reads_the_member asserts it (Task 2)
docs/schemas/rpc.schema.json                modifies: taskDiffResult.branch (Task 2)
apps/web/src/pages/TaskDetail.tsx           modifies: Diff.branch; the rule removed (Task 2)
apps/web/src/pages/TaskDetail.test.tsx      tests:    the Changes tab's branch comes from task.diff (Task 2)
apps/web/src/test/gate.ts                   modifies: its task.diff answer carries branch: "feat/gift-cards" (Task 2)
```

## Interfaces

Consumes: `task_branch` (plan 1); `PullRequestOpenedBody`, `FolderChangeOpened` events (on main);
the contract's `repo` (on main); `FolderChange.branch` (plan 4, used by the test).

Produces:

```rust
// crates/core/src/folders.rs
pub enum Named { Task(TaskId), FolderChange(u64), PullRequest(u64) }
// crates/runtime/src/daemon/documents.rs (private)
/// What pull request `number` was opened for in the project folder, newest first; `None` when the
/// log opened none with that number.
fn named_by_pull_request(deps: &ToolDeps, number: u64) -> Result<Option<Named>, Failure>;
// crates/store/src/diff.rs
pub struct TaskDiff { /* ... */ pub branch: Option<String> }
```

## Tasks

### Task 1: the Files page reads a pull request's merge

Files: modified `crates/core/src/folders.rs`, `crates/runtime/src/daemon/documents.rs`.
Produces: `Named::PullRequest`, `named_by_pull_request`.
Consumes: as above.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- The existing `named_in_subject` test in `folders.rs` (the one with `"Merge CTV-12: Pie
  pre-orders"`) keeps every case and adds:
  - `"Feat: Gift cards (#4)"` is `PullRequest(4)`;
  - `"Merge pull request #12 from me/docs/roadmap"` is `PullRequest(12)`;
  - `"Docs: Update Roadmap (#9) again"` is `None`;
  - `"Merge pull request #4 from me/docs/folder-5"` is still `FolderChange(5)`.
- `names_who_made_a_change_merged_by_pull_request`, in `documents.rs` beside
  `reads_a_document_with_its_twin_and_who_changed_it` and built like it:
  - `CTV-1` is assigned to `pm`. On a branch `feat/pie-pre-orders`, the roadmap is changed and
    committed, then merged into `main` with `--no-ff -m "Merge pull request #4 from
    me/feat/pie-pre-orders"`. `pull_request.opened { url, number: 4, branch:
    "feat/pie-pre-orders" }` is recorded about `CTV-1`. The roadmap's `last_change` is task
    `CTV-1`, agent `pm`, `by_you` false.
  - On a branch `docs/overview`, the overview is changed and committed. It is squashed into `main`
    (`merge --squash`, then a commit `Docs: Update Overview (#9)`). `folder_doc.written { path:
    OVERVIEW, written_by: "ada", change: 2, sha, branch: "docs/overview" }` is recorded by `ada`,
    and `folder_change.opened { change: 2, url, number: 9 }`. The overview's `last_change` is agent
    `ada`, task null.

  It fails first on the roadmap's task, which is null.

- [ ] `feat(runtime): the Files page names who made a change merged through a pull request`

### Task 2: the task page shows the branch Catervas recorded

Files: modified `crates/store/src/diff.rs`, `crates/runtime/src/daemon/gates.rs`,
`docs/schemas/rpc.schema.json`, `apps/web/src/pages/TaskDetail.tsx`,
`apps/web/src/pages/TaskDetail.test.tsx`, `apps/web/src/test/gate.ts` (the `task.diff` answer the
task page's tests and the gate's share; the gate shows no branch, so its tests are unaffected).
Produces: `TaskDiff.branch`, `task.diff`'s `branch`.
Consumes: `task_branch` (plan 1).

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `task_diff_reads_the_member` (`daemon/gates.rs`, changed): the answer's `branch` is
  `harness.branch("CTV-1")`.
- `TaskDetail.test.tsx`: the shared `task.diff` answer in `test/gate.ts` carries
  `branch: "feat/gift-cards"`.
  - The Changes tab reads "3 files, +142 −18, on the branch feat/gift-cards.", where it read
    `feature/CTV-1`.
  - The Designer's case reads `on the branch feat/gift-cards.`.
  - The private-folder case gives no `branch` and still shows no "on the branch".
  - It fails first on `feature/CTV-1`, which the page still works out itself.

- [ ] `feat(web): the task page shows the branch Catervas recorded`

## Verification

```
cargo xtask check
# expected: xtask check: ok (runs pnpm check)
cargo xtask check --integration
# expected: xtask check: ok
```
