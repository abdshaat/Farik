# Standalone internal-task-ids, plan 4: a folder change's branch and pull request

Status: draft
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.14 "Folder changes", 5.17, 8.5 (recorded by plan 6)
Depends on: plans 1 and 2 of this branch
Readiness confirmed by: the planner, 2026-10-17 (ADR 0066, ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

A document an agent writes, the owner approves or the owner saves goes to the forge on a branch
named for the documents (`docs/roadmap`). Its pull request is titled `Docs: Update Roadmap`, not
`docs/folder-<n>` and "Folder change <n>: ...". The branch is recorded in the event that made the
change. A change in an older log keeps `docs/folder-<n>`. The change's number stays the internal
name `catervas integrate folder-<n>` and Today's button use.

## Decisions

- The words are the documents' plain names joined by " and ", `plain_names` in
  `crates/runtime/src/tools/folders.rs`, over the written paths and then the removed ones in the
  order `land` receives them. That function becomes `pub(crate)`.
- In `land`:
  - the change's number is chosen as now, still past every old `docs/folder-<n>` branch;
  - the branch is `free_branch(ChangeType::Docs, &branch_slug(&plain_names(..)), &taken)`, with
    `taken` from `taken_branches(&deps.files, &deps.log, &deps.git, None)`, mapped to
    `LandRefusal::Git` with its words on failure.
- `taken_branches` gains `log: &EventLog`, after `files`, and adds every folder change's
  `branch` from `folder_docs(log)` to the names it answers. `chosen_branch` (plan 2) passes
  `&deps.tools.log`.
- The schema gains `$defs.folderChangeBranch`: a string, maxLength 64, pattern
  `^docs/[a-z0-9]+(-[a-z0-9]+)*$`. `folderDocWrittenBody`, `folderDocEditedBody` and
  `folderDocApprovedBody` gain an optional `branch` referring to it, described as "The branch the
  change was made on (ADR 0066); absent in a log from before 0.119, where it is docs/folder-<change>".
  It is absent on an approval that made no change.
- Each of the three writers puts `landed.branch` in its event:
  - `catervas_write_folder_doc` (`tools/folders.rs`);
  - `files.save` (`daemon/documents.rs`);
  - the owner's approval (`orchestrator/human.rs`).
- `FolderChange` gains `pub branch: String`: the event's `branch`, else `docs/folder-<change>`.
  `add_change` takes it as a parameter. The `folder_doc.edited` arm sets it the same way.
- In `integrate.rs`:
  - `Subject::branch` for a folder change answers `folder_change(tools, change)?.branch`;
  - `Subject::title` answers `format!("Docs: Update {}", plain_names(&paths))` over the change's
    paths.
  The pull request's body (its paths, one per line, and the approval's line) is unchanged; it
  names no number.
- `waiting_change` in `daemon/documents.rs` answers the change's `branch`, not
  `docs/folder-<n>`.
- Unchanged: the commit's message, the agent's `folder_doc_waiting` words (`folder-<n>`, inside
  Catervas), `catervas integrate folder-<n>`, `folder_change.opened` and the other events' numbers.
- `Harness::folder_change` (`orchestrator/fixtures.rs`) records `"branch": landed.branch` in its
  `folder_doc.written`. A new `Harness::folder_branch(change) -> String` reads the change's branch
  from `folder_docs`. Tests that spell `docs/folder-<n>` for a change `land` made use it (or the
  name the test's own answer gives). A test that makes `docs/folder-<n>` by hand with git, as an
  older log's change, keeps it.

## File map

```
docs/schemas/event.schema.json                  modifies: folderChangeBranch; branch on three bodies (Task 1)
crates/protocol/src/event/fixtures.rs           modifies: the three bodies' fixtures carry branch where they carry change (Task 1)
crates/store/src/folder_docs.rs                 modifies: FolderChange.branch, add_change, the edited arm (Task 1)
crates/store/src/branches.rs                    modifies: taken_branches takes log, adds folder branches (Task 1)
crates/runtime/src/orchestrator/rules.rs        modifies: chosen_branch passes the log (Task 1)
crates/runtime/src/tools/folders.rs             modifies: plain_names pub(crate); land names the branch; the write records it (Task 1)
crates/runtime/src/daemon/documents.rs          modifies: files.save records it; waiting_change reads it (Task 1)
crates/runtime/src/orchestrator/human.rs        modifies: the approval records it (Task 1)
crates/runtime/src/orchestrator/integrate.rs    modifies: Subject::branch and Subject::title for a folder change (Task 1)
crates/runtime/src/orchestrator/fixtures.rs     modifies: folder_change records branch; folder_branch (Task 1)
crates/runtime/src/orchestrator/integrate.rs    tests:    opens_a_pull_request_for_a_folder_change (changed) (Task 1)
crates/runtime/src/{tools/folders,daemon/documents,orchestrator/integrate,orchestrator/human}.rs   tests: docs/folder-<n> of land-made changes read the recorded name (Task 1)
```

## Interfaces

Consumes: `ChangeType::Docs`, `branch_slug`, `free_branch` (plan 1); `taken_branches` (plan 2);
`folder_docs`, `FolderChange` (`catervas_store::folder_docs`); `Landed` (`tools/folders.rs`).

Produces:

```rust
// crates/store/src/branches.rs (changed)
pub fn taken_branches(
    files: &ProjectFiles, log: &EventLog, git: &Git, except: Option<&TaskId>,
) -> Result<Vec<String>, StoreError>;
// crates/store/src/folder_docs.rs
pub struct FolderChange { /* ... */ pub branch: String }
// crates/runtime/src/tools/folders.rs
pub(crate) fn plain_names(paths: &[&str]) -> String;
// crates/runtime/src/orchestrator/fixtures.rs
impl Harness { pub(crate) fn folder_branch(&self, change: u64) -> String; }
```

## Tasks

### Task 1: a folder change's branch and pull request carry no number

Files: as the file map.
Produces: the above.
Consumes: as above.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `opens_a_pull_request_for_a_folder_change` (changed). With
  `name = document_name(CADENCE, &|_| None)` and `branch = format!("docs/{}", branch_slug(&name))`:
  - `origin` holds `branch` at the root's commit, where it checked `docs/folder-1`;
  - the `create` call passes `--head` with `branch` and `--title` with
    `format!("Docs: Update {name}")`, where it checked "Folder change 1: delivery/cadence.md";
  - `folder_doc.written`'s body has `"branch": branch`;
  - `merge_branch_on_the_forge` is given `branch`;
  - the rest is unchanged: the change is still 1, the body still the path.

  It fails first on the pushed branch.
- `files_the_rederive_when_the_owners_change_is_integrated` and the other tests that merge a
  folder change by hand pass with `harness.folder_branch(change)` in place of
  `format!("docs/folder-{change}")`. They prove the recorded branch is the one integration reads.

- [x] `feat(runtime): a folder change's branch and pull request are named for its documents`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
