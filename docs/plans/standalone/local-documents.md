# Standalone: documents kept on this computer

Status: ready
Branch: `feat/local-documents` (its own pull request to `main`)
Design: `docs/design/local-documents.md`; ADR 0074
Spec: `docs/SPEC.md` 4.1, 4.5, 5.2, 5.4, 5.12, 5.14, 5.17, 8.5 (revision 0.129)
Depends on: `origin/main` at `ca21251b`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-19, against the three readiness rules (ADR 0065); see the end.

## Goal

The founder's brief (design, "The brief"). When done: Settings → How work runs → Documents and setup's advanced settings offer "Keep documents on this computer"; turning it on takes `docs/catervas/` out of the repository in one pull request and keeps the files on disk, ignored through `info/exclude`; from then on the Files page, `catervas_write_folder_doc` and every reader use the root checkout's folder, tasks work on a copy that lands at acceptance, and a documents-only task opens no pull request; turning it off puts them back in one pull request. Out of scope: history, backups, per-folder choices, a location outside the repository.

## Decisions

- Every mechanism decision, with its rejected alternatives: the design, "The mechanism", and ADR 0074 decisions 1 to 6. Not restated.
- Names, fixed: policy key `documents_local` (TS `documentsLocal`); command `documents.set_local { local: boolean }` answering `{ mode, change }`; `team.get` gains `documents_mode`: `"committed" | "taking_out" | "local" | "putting_back"`; event kinds `document_storage.changed { local: boolean, change: integer|null, branch: string|null, sha: string|null }` (by the owner, no agent or session, about no contract) and `local_documents.written { paths: string[], removed: string[], by: "owner"|"agent"|"task", written_by: string|null, task_id: string|null, set_aside: string[] }` (about no contract; agent and session on the envelope for `agent`); 124 kinds in all.
- Refusal codes: `documents.set_local`'s six in the design's order (`project_folder`, `documents_switching`, `documents_in_flight`, `change_waiting`, `documents_busy`, `is_a_link`), each `{ path: "/local", message, code }` under -32005 with the design's words; `files.save` adds `documents_switching` (`/path`) after `is_a_link`; `catervas_write_folder_doc` adds `folder_doc_switching`; `catervas_git_commit` adds `documents_stay_local: <path> is kept on this computer and never committed; it reaches your team when the task is accepted`.
- Paths: start copies `.catervas/local/documents/start/<task-id>/`, set-aside `.catervas/local/documents/set-aside/<task-id>/<path under docs/catervas>`, scratch worktrees `.catervas/local/worktrees/documents-<n>`. Branch slugs from the plain titles "Keep documents on this computer" and "Put documents back in the repository" through `free_branch(ChangeType::Docs, …)`.
- The switch's pull request: titles `Docs: Keep the team's documents on this computer` and `Docs: Put the team's documents back in the repository`; body "## What changed" with "The team's documents in docs/catervas/ leave the repository and stay on this computer." or "… come back into the repository.", "## How it was verified" with "Chosen by the owner in Settings."; Today's row titles "Keep documents on this computer" and "Put documents back in your repository", its line as any folder change the owner made ("Its pull request waits for you to merge it" / "You chose it; it waits for you to merge it").
- The template keeps no `documents_local` (it belongs to the project, as `integration_branch` does): `leaves_out_what_belongs_to_the_project` is unchanged.
- A blob on disk is git's object id of the file's bytes, `git hash-object --no-filters --stdin-paths`, so `files.save`'s `base` keeps its meaning in both modes.
- Writes to disk are atomic: a temporary file in the same directory, then a rename; every path is walked segment by segment from the root and refused when a segment is a link (`plain_file`'s rule, `docs` and `docs/catervas` included).
- One test per major functionality (ADR 0060); the landing's refusal to follow links and the copy-in's refusal to copy them are one security guarantee and keep their assertion inside the copy test.

## File map

```
crates/core/src/folders.rs                         modifies: DocumentsMode, SwitchState, documents_mode, is_documents_only, local_documents_line, twin_is_stale_by_log
crates/core/src/governor/readiness.rs              modifies: is_within_any made pub(crate)
crates/core/src/governor/gates.rs                  modifies: WorkState.documents, DocumentWork, the gate's documents-only arm
crates/core/src/governor/transition.rs             modifies: TransitionContext.documents_stay_local, the NothingToIntegrate arm
crates/core/src/forge_text.rs                      modifies: storage_change_title, storage_change_body
docs/schemas/team.schema.json                      modifies: $defs/policy documents_local
docs/schemas/event.schema.json                     modifies: two kinds and bodies
docs/schemas/rpc.schema.json                       modifies: documents.set_local; teamGetResult documents_mode
crates/protocol/src/event.rs, lib.rs, event/fixtures.rs   modifies: the two kinds (122 → 124)
crates/store/src/projections.rs                    modifies: no-op arms for the two kinds
crates/store/src/folder_docs.rs                    modifies: FolderChange.storage, switch_state, local_writes
crates/store/src/baseline.rs                       modifies: copy_directory, files_under made pub(crate)
crates/store/src/local_documents.rs                creates: copy in, changes, landing, set aside, exclude lines, restore
crates/store/src/document_source.rs                creates: DocumentSource, read from a branch or from disk
crates/store/src/git.rs                            modifies: hash_files, skip_worktree, commit_untracking, commit_folder_from
crates/store/src/lib.rs                            modifies: the two modules
crates/runtime/src/daemon/documents.rs             modifies: set_local; every read through DocumentSource; save on disk
crates/runtime/src/daemon/team.rs                  modifies: team.get documents_mode, team.save carry-over, team.start
crates/runtime/src/orchestrator/integrate.rs       modifies: storage changes' title, body, restore, exclude removal, revert; landing in cleanup
crates/runtime/src/orchestrator/rules.rs           modifies: rule 7 freeze, copy in, detached worktree; readers
crates/runtime/src/orchestrator/requests.rs        modifies: start_point, plan_on, epic criteria copy in
crates/runtime/src/orchestrator/session.rs         modifies: stale_twins_of through the mode
crates/runtime/src/transitions.rs                  modifies: work, context, tidy for documents
crates/runtime/src/tools/git.rs                    modifies: documents_stay_local
crates/runtime/src/tools/folders.rs                modifies: write on disk, check_input, proposals_list, file_rederive_for
crates/runtime/src/daemon/gates.rs                 modifies: product_plan_state from the worktree
crates/runtime/src/prompt.rs                       modifies: rules_section's local line
apps/web/src/pages/TeamRules.tsx, settings/Work.tsx, setup/SetupAdvanced.tsx, setup/TeamSetup.tsx   modifies: the switch
apps/web/src/pages/FileDocument.tsx, Files.tsx, strings/en.ts                                          modifies: local words
crates/runtime/src/orchestrator.rs, orchestrator/messages.rs, orchestrator/verify.rs   modifies (Task 5): task_rev and its callers; the implement message
docs/SPEC.md                                       modifies: revision 0.129
```

Six runtime files are touched by more than one task (`transitions.rs`, `rules.rs`, `requests.rs`, `documents.rs`, `tools/folders.rs`, `integrate.rs`). This is accepted: each task adds or changes different functions in them (Task 4 the switch and its integration hooks, Task 5 the task flow, Task 6 the document reads and writes), named in each task, so no two tasks change one function for different reasons.

## Interfaces

Consumes (on main): `Git::{create_detached_worktree, tree_entries, file_at, has_file, common_dir, uncommitted_in, fetch_fast_forward}`, `TreeEntry`, `integration_branch`, `stale_twins`, `held_document`, `free_branch`, `branch_slug`, `folders::land`'s numbering, `file_rederive`, `task_private_folder`, `reaches_the_folder`, `copy_baseline`'s helpers, `pair_changed_alone`, `plain_file`.

Produces:

```rust
// catervas-core, folders.rs
pub enum DocumentsMode { Committed, TakingOut, Local, PuttingBack }
impl DocumentsMode { pub fn wire(self) -> &'static str; pub fn reads_disk(self) -> bool; pub fn writes_open(self) -> bool }
pub struct SwitchState { pub local: bool, pub waiting: bool }
pub fn documents_mode(policy_local: bool, newest: Option<SwitchState>) -> DocumentsMode;
pub fn is_documents_only(contract: &TaskContract) -> bool;          // a Task, allowed_paths non-empty, each within docs/catervas
pub fn local_documents_line() -> &'static str;                       // the design's Team rules line
pub fn twin_is_stale_by_log(human_seq: Option<i64>, twin_seq: Option<i64>) -> bool;
// catervas-core, gates.rs / transition.rs
pub struct DocumentWork { pub only: bool, pub changed: Vec<String> }  // WorkState gains pub documents: Option<DocumentWork>
// TransitionContext gains pub documents_stay_local: bool
// catervas-core, forge_text.rs
pub fn storage_change_title(local: bool) -> &'static str;
pub fn storage_change_body(local: bool) -> String;
// catervas-store, folder_docs.rs
// FolderChange gains pub storage: Option<bool>
pub fn switch_state(docs: &FolderDocs, log: &EventLog) -> Result<Option<SwitchState>, StoreError>;
pub fn local_writes(log: &EventLog) -> Result<BTreeMap<String, LocalWrite>, StoreError>;
pub struct LocalWrite { pub seq: i64, pub at: DateTime<Utc>, pub by: LocalWriter, pub written_by: Option<String>, pub task_id: Option<String> }
pub enum LocalWriter { Owner, Agent, Task }
pub fn local_stale_twins(log: &EventLog, root: &Path) -> Result<Vec<String>, StoreError>;
// catervas-store, local_documents.rs
pub const DOCUMENTS: &str = "docs/catervas";
pub fn copy_in(git: &Git, root: &Path, worktree: &Path, start: Option<&TaskId>) -> Result<(), StoreError>;
pub fn document_changes(root: &Path, worktree: &Path, task: &TaskId) -> Result<Vec<String>, StoreError>;
pub fn put_back_outside(root: &Path, worktree: &Path, task: &TaskId, allowed: &[String]) -> Result<Vec<String>, StoreError>;
pub struct Landing { pub written: Vec<String>, pub removed: Vec<String>, pub set_aside: Vec<String> }
pub fn land(root: &Path, worktree: &Path, task: &TaskId) -> Result<Landing, StoreError>;
pub fn forget_start(root: &Path, task: &TaskId) -> Result<(), StoreError>;
pub fn write_document(root: &Path, path: &str, text: &str) -> Result<(), StoreError>;
pub fn exclude_documents(git: &Git) -> Result<(), StoreError>;
pub fn include_documents(git: &Git) -> Result<(), StoreError>;
pub fn restore_taken_out(git: &Git, sha: &str) -> Result<Vec<String>, StoreError>;
// catervas-store, document_source.rs
pub enum DocumentSource { Branch(String), Disk(PathBuf) }
impl DocumentSource {
    pub fn entries(&self, git: &Git, under: &str) -> Result<Vec<TreeEntry>, GitError>;
    pub fn text(&self, git: &Git, path: &str) -> Result<Option<String>, GitError>;
    pub fn has_file(&self, git: &Git, path: &str) -> Result<bool, GitError>;
    pub fn held_document(&self, git: &Git, stem: &str, format: DocumentFormat) -> Result<Option<String>, GitError>;
}
pub fn team_documents(team: &Team, git: &Git, mode: DocumentsMode) -> Result<DocumentSource, GitError>;
pub fn stale_twins_now(git: &Git, log: &EventLog, team: &Team, mode: DocumentsMode) -> Vec<String>;
pub fn documents_mode_of(team: &Team, log: &EventLog) -> Result<DocumentsMode, StoreError>;
// catervas-store, git.rs
pub fn hash_files(&self, paths: &[PathBuf]) -> Result<Vec<String>, GitError>;
pub fn skip_worktree(&self, worktree: &Path, under: &str) -> Result<(), GitError>;
pub fn commit_untracking(&self, scratch: &Path, branch: &str, from: &str, folder: &str, message: &str) -> Result<Option<String>, GitError>;
pub fn commit_folder_from(&self, scratch: &Path, branch: &str, from: &str, folder: &str, source: &Path, message: &str) -> Result<Option<String>, GitError>;
// catervas-runtime
pub(super) fn set_local(deps: &ToolDeps, local: bool) -> Result<Value, Failure>;               // daemon/documents.rs
pub(crate) fn file_rederive_for(deps: &ToolDeps, team: &Team, human: &str) -> Result<Option<TaskId>, RequestError>;  // tools/folders.rs
```

## Tasks

### Task 1: core knows the mode and the documents-only task

Files: modified `folders.rs`, `readiness.rs`, `gates.rs`, `transition.rs`, `forge_text.rs`.
`documents_mode`: `Some{local:true,waiting:true}` with the policy on is `TakingOut`; `Some{local:false,waiting:true}` with it off is `PuttingBack`; else on is `Local`, off `Committed`. `reads_disk` is `Local | PuttingBack`; `writes_open` is `Committed | Local`. `twin_is_stale_by_log`: `(Some(h), None)` and `(Some(h), Some(t))` with `h > t` are stale, all else not. The gate: with `documents` `Some{only: true, ..}` the commit check is skipped (the clean check stays) and an empty `changed` adds "the task changed no document in docs/catervas/". The effect: `to == Accepted && (task_private_folder(..).is_some() || context.documents_stay_local)` adds `NothingToIntegrate`.

- `a_task_whose_documents_stay_local_needs_no_commit_and_integrates_nothing` — asserts that a Product Manager task with `allowed_paths: ["docs/catervas/product/spec.*"]` is `is_documents_only` (and one adding `src/**` is not); that `check_criteria_recorded` passes with `commits: 0`, a clean worktree and `documents: Some{only: true, changed: ["docs/catervas/product/spec.html"]}`, and refuses with `changed` empty naming "changed no document"; that the `verifying → accepted` effects contain `NothingToIntegrate` when `documents_stay_local` is true and not when false; and that `documents_mode` gives each of the four modes from the inputs above.

- [x] `feat(core): a task whose documents stay on this computer integrates nothing`

### Task 2: the policy, the two events and their folding

Files: the three schemas, `event.rs`, `lib.rs`, `fixtures.rs`, `projections.rs`, `folder_docs.rs`.
`document_storage.changed` with a `change` adds a `FolderChange` with `storage: Some(local)`, `approved`/`edited` false, `written_by` None; every other change has `storage: None`. `switch_state` is the newest `document_storage.changed`: `local` its value, `waiting` true when it has a change neither integrated nor escalated. `local_writes` maps each path in `paths` and `removed` to its newest write. Every `EVERY_KIND.len()` assert and `KINDS` becomes 124.

- `folds_the_switch_into_a_folder_change_and_the_local_writes_by_path` — asserts that a log of `document_storage.changed{local:true, change:4, branch:"docs/keep-documents-on-this-computer"}` gives a change 4 with `storage: Some(true)` and `switch_state` `{local:true, waiting:true}`, then `{waiting:false}` after `folder_change.integrated{change:4}`; and that two `local_documents.written` naming `docs/catervas/product/spec.html` (by owner, then by task `CTV-3`) give `local_writes` the second, `by: Task`, `task_id: "CTV-3"`.

- [x] `feat(protocol): record the documents' move and every local write`

### Task 3: the store reads, copies and lands documents on disk

Files: created `local_documents.rs`, `document_source.rs`; modified `git.rs`, `baseline.rs`, `lib.rs`.
`copy_in`: `skip_worktree(worktree, DOCUMENTS)`, empty the worktree's `docs/catervas/`, copy the root's regular files (links skipped, a link on the way refused), and with `start` copy the same files to the start copy once. `land`: the design's per-file rule (written, already equal, set aside; removals; the format rule) and then `forget_start`. `exclude_documents` appends the comment and `/docs/catervas` to `<common_dir>/info/exclude` when absent; `include_documents` removes those two lines only. `restore_taken_out(sha)`: every path `git ls-tree -r <sha>^ -- docs/catervas` lists that is missing on disk is written from `<sha>^`. `commit_untracking` and `commit_folder_from` work in `scratch`, make `branch` at the commit, remove `scratch`, and answer `None` when there is nothing to commit. `DocumentSource::Disk` lists regular files (mode `100644`, blob from `hash_files`) and reads text with `plain_file`'s link rule.

- `a_task_copy_lands_what_changed_and_sets_aside_what_moved` — asserts, on a fixture root with `product/spec.html`, `product/roadmap.html` and a link `product/evil.html`, that `copy_in` copies the two files and not the link; that after the worktree changes both and the root's `roadmap.html` is changed meanwhile, `land` writes `spec.html`, leaves the root's `roadmap.html`, copies the task's to `set-aside/<task>/product/roadmap.html` and answers that; and that a second `land` writes nothing.
- `documents_taken_out_and_put_back_stay_on_disk` — asserts, on a git fixture whose `main` tracks `docs/catervas/s.md`: after `exclude_documents` and `commit_untracking`, a `merge --ff-only` of that branch in the root removes the file and `restore_taken_out` brings it back with `git status --porcelain` empty; after `commit_folder_from` and its fast-forward the file is tracked with the same bytes; `include_documents` leaves no Catervas line in `info/exclude`.

- [x] `feat(store): read, copy and land the team's documents on disk`

### Task 4: the switch

Files: `daemon/documents.rs` (set_local, in `METHODS`), `daemon/team.rs`, `orchestrator/integrate.rs`, `orchestrator/rules.rs` (rule 7), `forge_text.rs` callers, `tools/folders.rs` (waiting rows).
`set_local` under the integration lock: refusals in order; on: `exclude_documents`, then `commit_untracking` from the integration branch (a change number and branch as `land` takes them), `document_storage.changed`, `policy.documents_local = true` through `write_team`, wake. Off: `commit_folder_from` the root, the event, the policy false; with no change, `include_documents` at once. `team.get` answers `documents_mode`. `team.save` writes the saved `documents_local` whatever it is sent. `team.start` with `documents_local: true` runs `set_local`'s refusals before writing anything (a refusal at `/team/policy/documents_local`), writes the team with it false, then `set_local(true)`. Integration: a storage change's pull request uses `storage_change_title`/`body`; `record_integrated` of `storage: Some(true)` runs `restore_taken_out(sha)`, of `Some(false)` `include_documents`, and neither files a re-derive; `escalate` of a storage change whose pull request was closed without merging (and only that escalation: a failed push, a forge error or a fast-forward the local branch cannot follow leave the mode as it is, for the human's `catervas integrate`, whose `record_integrated` then restores) writes the policy back (actor `catervas`), runs `include_documents` for a take-out, and posts "Your documents stay as they were: the pull request that moved them was closed.". When a switch is made while tasks that change documents wait to be assigned (setup's Catervafication included), the channel gets once: "<n> tasks wait for the pull request that moves your documents." Rule 7 passes over a task whose `allowed_paths` reach `docs/catervas` while the mode is `TakingOut` or `PuttingBack`, and in those modes `files.save` is refused `documents_switching` (after `is_a_link`) and `catervas_write_folder_doc` `folder_doc_switching` (before its other folder-change checks); Task 6 adds the disk writes of `local`.

- `keeping_documents_local_moves_them_by_one_change_each_way` — asserts, with the daemon harness on a repository with no `origin` whose `main` tracks `docs/catervas/product/spec.md`: `documents.set_local{local:true}` answers `taking_out` and a change; `files.save` is refused `documents_switching`; merging the change's branch by hand and one tick record `folder_change.integrated`, `spec.md` is on disk, ignored and untracked, and `team.get` says `local`; `documents.set_local{local:false}` then answers `putting_back`, and after its merge and a tick the file is tracked and the mode `committed`; and that with an assigned task whose `allowed_paths` is `docs/catervas/product/**` it is refused `documents_in_flight` naming the task's title.

- [x] `feat(runtime): move the team's documents out of the repository and back`

### Task 5: tasks work on a copy that lands at acceptance

Files: `orchestrator/rules.rs` (rule 7: `copy_in` after `start_worktree`; a documents-only task in a disk-reading mode gets `create_detached_worktree` and no `branch`), `orchestrator/integrate.rs` (`stopped_merging` copies in after its merge; `cleanup` of an accepted task runs `land` before `remove_workspace`, records `local_documents.written{by:"task"}` when anything moved, posts each set-aside line; a cancelled one `forget_start`), `orchestrator/requests.rs` (`start_point` skips a documents-only dependency in local mode; `run_on_the_integration_branch` copies in without a start), `transitions.rs` (`work` and `context` add `document_changes` to the changed paths and fill `documents` and `documents_stay_local`; a documents-only task reads no branch; `tidy` runs `put_back_outside` and lists its paths in `worktree.tidied`), `tools/git.rs` (`documents_stay_local`), `daemon/tasks` diff answering no `branch` for such a task.
A failed `land` keeps the worktree and is tried on the next tick.
A branchless documents task: `pub(crate) fn task_rev(git: &Git, files: &ProjectFiles, contract: &TaskContract, stays_local: bool) -> String` (in `orchestrator.rs`, beside `worktree`) answers `task_branch(contract)`, or for such a task the commit its worktree's `HEAD` is at; it replaces `task_branch(` in `tools/git.rs` (:57 diff, :224 push, which for such a task answers "this task has no branch: its documents are not committed"), `orchestrator/messages.rs` (:1017, :1080, :1105, :1200), `orchestrator/verify.rs` (:365, :577, :700), `orchestrator/session.rs` (:653, the preview's tree), `daemon/gates.rs` (:603 `product_plan_state`, and `task_diff` (:504), which answers no `branch` for it), `transitions.rs` (:950), `daemon/documents.rs` (:408, :845), and `orchestrator/requests.rs` (:516, :535, :624, each skipping such a dependency); `rules.rs` :1717 (choosing a new branch) and `integrate.rs` :157 and :1253 (never reached: such a task integrates nothing) keep it. The implement message (`messages.rs`) of a task in a disk-reading mode says, after its paths: "Your documents in docs/catervas/ are kept on this computer and never committed: write them, record your criteria, then ask for verifying." and, for a documents-only task, "This task has no branch and needs no commit." in place of the branch sentence.

- `a_documents_task_lands_on_acceptance_with_no_pull_request` — asserts, with the orchestrator harness in `local` mode, that a Product Manager task on `docs/catervas/product/spec.*` finds the root's `spec.html` in its worktree, writes it there, reaches `verifying` with no commit, is accepted with `NothingToIntegrate`, opens no pull request, and leaves the new `spec.html` in the root, a `local_documents.written{by:"task"}` and no worktree or start copy; and that its `catervas_git_commit` of `docs/catervas/product/spec.html` is refused `documents_stay_local`.
- `a_code_task_lands_its_documents_and_opens_its_pull_request` — asserts that a Developer task on `src/**` and `docs/catervas/engineering/**` commits `src/a.rs`, writes `engineering/conventions.html`, and once accepted has the document in the root and a pull request whose diff holds only `src/a.rs`; and that a document it wrote outside its `allowed_paths` was put back by the tidy before `verifying`.

- [x] `feat(runtime): a task's documents are a copy that lands when it is accepted`

### Task 6: readers and writers use the one folder

Files: `daemon/documents.rs` (`listing`, `version`, `raw_text`, `pending` (none in a disk mode), `waiting_change` (none), `save` on disk, `ask_change`, `rewrite` through `team_documents`; the last change from `local_writes`), `tools/folders.rs` (`write_folder_doc` on disk with the format rule and `folder_doc_unchanged`, `folder_doc_switching`; `check_input` and `proposals_list` through `DocumentSource`; `file_rederive_for`, which `file_rederive` now calls per human path), `orchestrator/rules.rs` (`delivery_to_rewrite`, `held_on_the_integration_branch`), `orchestrator/requests.rs` (`plan_on`), `daemon/gates.rs` (`product_plan_state` reads a documents-only plan task's worktree), `orchestrator/session.rs` and `transitions.rs` (`stale_twins_now`), `prompt.rs` (`rules_section` gains `documents_local: bool` and adds `local_documents_line()` after the documents line when the mode reads disk).
`files.save` in a disk mode answers `{ change: null, branch: null }`, records `local_documents.written{by:"owner"}` and, for a human document, `file_rederive_for` at once.

- `the_files_page_reads_and_writes_the_folder_on_disk` — asserts, in `local` mode with `product/spec.html` and `spec.agent.html` on disk, that `files.tree` lists the pair; `files.save` of `spec.html` with its blob as `base` writes the disk, makes no branch, and `files.read` then answers `last_change.by_you`, `stale: true`, `rederiving: true` with a re-derive task filed; a second save with the old `base` is `file_changed`; and `catervas_write_folder_doc` at a review writes `delivery/cadence.html` on disk and answers `change: null`.
- `every_session_is_told_the_documents_stay_on_this_computer` — asserts that an implement session's Team rules in `local` mode hold `local_documents_line()` after the documents line, and in `committed` mode do not.

- [x] `feat(runtime): the Files page and the folder tool read and write the documents on disk`

### Task 7: the switch in the web app

Files: `TeamRules.tsx` (`Documents` gains the switch, its ⓘ bullets, the two `Dialog`s, the moving line, the refusal words from `data.errors`), `settings/Work.tsx` (hidden when `membersPresent`), `setup/SetupAdvanced.tsx` and `setup/TeamSetup.tsx` (`teamOf` keeps `documentsLocal`), `FileDocument.tsx`/`Files.tsx` (the local and moving words), `strings/en.ts` (the design's strings, word for word).

- `keeps_documents_on_this_computer` (in `team.test.tsx`) — asserts that turning the switch on opens "Keep documents on this computer?", that "Keep them here" sends `documents.set_local {local: true}`, and that with `documentsMode: "taking_out"` the switch is disabled with "Taking your documents out of the repository. Merge the pull request to finish.".
- `saves_on_this_computer` (in `Files.test.tsx`) — asserts that with `documentsMode: "local"` the save hint is "Saving changes the document on this computer." and after saving "Saved."; with `"putting_back"` there is no "Edit" and the moving line shows.

- [x] `feat(web): keep the team's documents on this computer`

### Task 8: the spec

`docs/SPEC.md` revision 0.129: 4.1 (setup's switch), 4.5 (the Files page's words), 5.2 (no commit, nothing to integrate), 5.4 (document changes in the diff, the tidy), 5.12 (`documents_local`), 5.14 (the switch's folder changes, the landing), 5.17 (the four modes, the copy, the log's last change and staleness; the Product Manager's docs task "reaches `verifying` only with a commit" except in a disk-reading mode), 8.5 (two kinds, 124); `docs/design/catervas-folders.md` gains a line that ADR 0074 amends it.

- [ ] `docs(spec): record the documents kept on this computer (0.129)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check
# expected: xtask check: ok
```

The founder's own test, in the web app, on a copy of a live team's repository: turn the switch on, merge the pull request, see the documents still on Files and absent from `git status`; ask the Product Manager for a change to the spec and see it land with no pull request; turn it off, merge, see the documents in the repository.

## Readiness (ADR 0065, checked by the planner)

1. Every decision is made: the mechanism in the design and ADR 0074; names, codes, paths, words and the event shapes above. No "as needed" or "TBD".
2. No ambiguity: every file and signature is named; each test says what it asserts; the one command has its expected output.
3. No forward dependency: Task 1 is core alone; 2 adds wire and folding; 3 the store, using 1; 4 the switch, using 1 to 3; 5 the task flow, using 1 to 4 (the freeze of rule 7 is 4's); 6 the readers and writers, using 1 to 3 and 5's landing events; 7 the web, using 4 and 6's wire; 8 last. Each task builds and passes alone.
