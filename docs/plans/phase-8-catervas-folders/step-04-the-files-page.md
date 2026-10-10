# Phase 8, step 04: The Files page's server

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.4, 5.14, 5.17, 8.5; F5
Depends on: steps 01 and 01b of this phase (planned, not yet executed: `catervas_core::folders` with `ROLE_FOLDERS`, `role_folder`, `is_human_document`, `agent_twin`, `human_of_twin`, `pair_changed_alone`, `folders_line`; `DoneRule::PairChangedAlone`; `PromptInput.active_roles` and `rules_section`'s folders line; the Product Manager's `write_workspace` and `git_local` and `REVIEWER_ROLE_FOR`'s Product Manager row); step 03 of this phase (planned, not yet executed: `Git::commit_files` and `CommitOutcome`, `transitions::integration_lock`, `tools::folders::{land, Landed, LandRefusal}`, the fold `folder_docs`, `Subject::FolderChange` through `integrate_locked` and `record_integrated`, the events `folder_doc.written`, `folder_doc.approved` and `folder_change.integrated`); phase 7 and the Catervas rename (merged on main, 2c28b555); task ids `CTV-<n>` (merged on main, #33). Nothing of steps 02, 03b, 05, 06 or 07.
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19)
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): not ready on B1 (the fold must learn `folder_doc.edited`), fixed as the reviewer wrote; S1–S8 and nits carried into execution, folded below.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at ac1a606e; step 03 moves some in `integrate.rs`, and the names are what count. Step 04b, next, builds the page on what this step answers.

## Goal

The daemon answers the Files page: the team's folders and their documents read from the integration branch, the team's integration policy, one document with its twin, who last changed each and whether the twin is stale, and the owner's own saved change while it waits to be added. It saves the owner's edit as a folder change (step 03's `land`), recorded as `folder_doc.edited`, which is integrated under the team's `integration` policy exactly as an accepted task's branch is (5.14). Once an owner's change to a document for people is integrated, its twin is stale, read from git with nothing stored, and Catervas files the owning role's re-derive task, which with sprints on is planned first in the next sprint. Every session is told which twins are stale, and the Definition of Done lets a diff that changes only a stale twin pass `pair_changed_alone`. Out of scope: the page itself (04b); "A change to this waits for your approval", which reads step 03's proposals and is a later step's job, not stubbed here; "Open in Files" on step 03b's card; creating, renaming and deleting documents (ADR 0051).

## Decisions

- **Three RPCs, as the existing kinds are.** `files.tree` and `files.read` are queries (`query { name, params }`, `rpc.schema.json:22-75`), `files.save` a method beside `contract.save` (`:504`, `:2412`), all answered by a new module `crates/runtime/src/daemon/documents.rs` (not `folders.rs`: `folders.list` is the first run's folder browser). Rejected: a `command`, whose reply carries no data a page reads back.
- **Read from git at the integration branch's head** (`integration_branch`, `crates/store/src/git.rs:733`), never from the root checkout's files: the documents are committed (ADR 0051) and the checkout may be on another branch. The one exception is the owner's waiting change, below.
- **What the tree lists.** One folder per row of `ROLE_FOLDERS`, in table order, whose role has an active agent; its `agent_id` is that role's first active agent in team-file order, as `product_manager` (`orchestrator/requests.rs:46`) picks one. Its documents are the regular files (`ls-tree` mode `100644` or `100755`) ending `.md` under the folder, at any depth, except the twin of a human document that is present, which rides on its document as `twin`. A folder with none is listed with `documents: []`. The answer also carries the team's `integration` (`auto_merge`, `pull_request` or `manual`), which the page's words follow.
- **Names.** A fixed list in core, `DOCUMENT_NAMES`, in this order: `product/spec.md` "Product description", `product/roadmap.md` "Roadmap", `architecture/overview.md` "Overview", `architecture/features.md` "Features", `architecture/jobs.md` "Scheduled jobs", `architecture/structure.md` "Structure", `engineering/conventions.md` "Conventions", `engineering/workflow.md` "Workflow", `delivery/cadence.md` "Sprint rhythm", `marketing/brand/brand-kit.md` "Brand kit", `marketing/brand/persona.md` "Brand persona" (each under `docs/catervas/`). A file directly in a folder's `plans/` whose stem is a task on the board or a marketing plan is "Plan: <its title>" (the task projection's title; the plan's as `marketing_plans`, `crates/store/src/marketing.rs:99`, reads it), as the mockup's "Plan: pie pre-orders". Any other is its file stem with `-` and `_` as spaces and the first letter upper case (`ceremony-notes.md` "Ceremony notes"); a folder's name is its last segment so (`Product`). Documents come in `DOCUMENT_NAMES` order, then by path. A step that adds a document adds its name. Rejected: names from each file's first heading, a read per file per tree.
- **Last change and staleness are in `files.read`, not `files.tree`**: the tree shows neither (`Files.dc.html`), and each costs a `git log` per file on every re-query after an event (`apps/web/src/app/store.ts:29`). Rejected: a last change and a stale flag on every document of the tree.
- **Who changed a version last** is read from the last commit on the integration branch's first-parent history that changed the path (`git log -1 --first-parent`), by what its subject names (core's `named_in_subject`): the first token of either kind, in subject order: a `TaskId`, as integration writes `Merge <id>: <title>` (`orchestrator/integrate.rs:501`) and a forge carries `<id>: <title>` (`:437`) or a branch naming it; or a `folder-<n>` (step 03's `Merge docs/folder-<n>: …`, a forge's branch) or `Folder change <n>` (step 03's pull request title). So `Merge docs/folder-3: architecture/plans/CTV-12.md` names folder change 3. A task on the board gives `task_id` and its `assignee_id` as `agent_id`. A folder change gives, from the log, `by_you` when a `folder_doc.edited` names it, else the agent of the `folder_doc.written` (`written_by`) or of the proposals the `folder_doc.approved` naming it settled (step 03's fold). Anything else gives only `author`, the commit's author name. Rejected: a commit trailer, which no Catervas commit carries; git's author, which for a task's commits and an owner's change is the same person.
- **The owner's waiting change.** While a folder change that a `folder_doc.edited` names holds the path and is neither integrated nor escalated (step 03's fold, with this step's `edited`; an escalated change no longer holds its path, step 03, and its escalation is in the channel, so the page shows nothing of it), `files.read` answers `waiting: { change, branch }` and reads the document, its blob and its last change from that branch's head (`by_you: true`), so the owner sees what they saved; otherwise `waiting: null`. Another's waiting change is not shown: the page shows the project as it is.
- **The fold learns the owner's edits** (step 03 owns `FolderChange`; step 04 adds `edited`): `folder_docs` also folds `folder_doc.edited` into `changes` as `FolderChange { change, paths: [path], approved: false, integrated, escalated, edited: true }`, `pub edited: bool` false for the others, so `land`'s `Waiting`, the integration and `waiting` see the owner's change; `projections.rs`'s `apply` passes over `folder_doc.edited`.
- **Stale** (ADR 0051): a twin is stale when the last commit that changed its human document is neither the last commit that changed the twin nor its ancestor, both on the integration branch. Both are `git log -1` with history simplification, not `--first-parent`: a re-derive that started before a second edit and merged after it must stay stale, and its merge commit would hide that. Ancestry is `commit_count(twin_last, human_last) == 0` (`git.rs:399`). Not stale either when `last_commit(.., first_parent: true)` of both paths is the same commit, meaning one integration brought both. A pair with a file missing is no pair. Nothing is stored. The rule is core's `twin_is_stale`; the reads are `catervas_store::git::stale_twins`, beside `integration_branch`, which already knows the team. An owner's change not yet integrated makes nothing stale. `files.read` also answers `rederiving`: whether a task whose `task.created` names `rederives: <twin>` is neither accepted nor cancelled. A twin made stale by the owner's own git commit, outside the Files page, gets no re-derive, and the page then says only "The team's version is older than yours", naming no agent (04b).
- **Save** (`files.save { path, text, base }`), its refusals each `REFUSED` (-32005, `daemon/web.rs:612`) with `data.errors: [{ path, message, code }]` as `contract.save`'s, `path` a JSON pointer (`/path`, `/text`, `/base`) as `gates.rs:828` and `:873` write it, in this order: core's `check_owner_edit`: `outside_folders` (`/path`: `normalise`, `governor/paths.rs:87`, refuses it, or it is not under `docs/catervas/`), `not_markdown` (`/path`: not `.md`), `agent_twin` (`/path`: `.agent.md`), `marketing_plan` (`/path`: `marketing/plans/MP-<n>.md`, "A marketing plan changes through a new version on its page"; the controller's ruling of 2026-10-10: it keeps its own flow, ADR 0042), `too_large` (`/text`: over 262,144 bytes); `outside_folders` also for a path under no folder `files.tree` lists (a folder of no active role), which `files.read` answers `NOT_FOUND`; then, holding `transitions::integration_lock`, `file_changed` (`/base`: no file at the path on the integration branch, or its blob is not `base`); then `land(deps, team, &[(path, text)], message, None)`, whose refusals map as `Busy` `file_busy` (`/path`: the root checkout has the owner's uncommitted change to it), `Link` `is_a_link` (`/path`), `Waiting` `change_waiting` (`/path`) (an earlier change of the path is not integrated yet), and `Git` an `INTERNAL_ERROR` with git's words. `Ok(None)` (the text is the file's) answers `{ change: null, branch: null }` with nothing recorded. The message is `docs(<folder's last segment>): edit <path under the folder>` (`docs(product): edit roadmap.md`); this step widens `land`'s `author` to `Option<&str>` (`commit_files` already takes one; step 03's callers pass `Some`); `None` makes it the person git knows.
- **`folder_doc.edited { path, change, sha }`** is recorded after `land` answers `Landed`, about no contract, with the daemon's ids, and the save answers `{ change, branch }`. Then `state.wakes().notify_one()`, so `auto_merge`'s tick integrates it. Nothing in this step says "at once": that is the page's wording under `auto_merge` (04b).
- **The re-derive, when the owner's change is integrated.** Not at the save: under `pull_request` and `manual` the integration branch, which a re-derive's worktree is made from (`orchestrator/rules.rs:1405`), does not hold the edit yet. After step 03's `record_integrated` records `folder_change.integrated` for a change a `folder_doc.edited` names, of a path `is_human_document` holds, `file_rederive` runs: unless a task whose `task.created` names `rederives: <twin>` is `draft`, `refining`, `ready` or `assigned` (one not yet in its worktree, which will hold the edit), Catervas files a whole contract, `created_by: "catervas"`, with `request.triaged { small }` by `catervas` at once, as `file_raise_request` does (`crates/store/src/requests.rs:229`). Contract: title `Bring <twin file name> up to date with the owner's change`; intent `The owner changed <human> on the Files page. Rewrite <twin>, the version written for agents, so that it says what <human> now says.`; scope in `[<twin>]`, out `[<human>, which stays as the owner left it]`; R1 `<twin> says what <human> says now, written for agents.`; C1 `review`, satisfies R1, rubric `Every fact, decision and limit in <human> is in <twin>.` and `Nothing in <twin> contradicts <human>.`; assignee role the folder's owner; reviewer role `default_reviewer_role` (`crates/roles/src/reviewer.rs:34`), else the first of the owner's `REVIEWER_ROLE_FOR` row (readiness then names the missing reviewer, 5.3); risk `low`; budget `placeholder_budget_usd` (`requests.rs:80`); `allowed_paths: [<twin>]`. A filing refused is posted in the channel by Catervas (`post_system`, `crates/runtime/src/channel.rs:267`) with its reason; the integration stands.
- **A re-derive is judged as filed whole**, like a breakdown's child or a held contract: `is_to_be_judged` (`orchestrator/requests.rs:274`, `:289`) also holds for a contract whose `task.created` names `rederives`, so no refine session rewrites it.
- **Planned first in the next sprint.** With `plan_in_sprints` on, the assigner's plan of a sprint (`plan_sprint` with `PlannedBy::Assigner`, `crates/runtime/src/sprints.rs:274`) also takes every re-derive in the Backlog that it does not name, ahead of its own, outside `fits`' budget (`:418`). Rejected: Catervas planning them at the sprint's start, which `fits` would then refuse as a second plan ("a sprint is planned once"); `skips_sprints` (`projections.rs:638`), which plans it in no sprint.
- **Every session is told** the stale twins in its Team rules, after step 01's folders line: `- stale twins: <paths joined ", ">. Each is behind the document for people beside it, which the owner changed: read that document as well until the twin is brought up to date.`, and nothing with none. A git error reading them leaves the line out rather than failing the session (`ponytail:` the runtime has no log to warn in; warn there once it has one).
- **The Definition of Done**: `pair_changed_alone` leaves out a twin in `DoneEvidence.stale_twins`, which the runtime fills only when a changed path is a human document's twin, from the integration branch.
- **No migration, no new table**: the re-derives are read from `task.created`, the owner's changes from `folder_doc.edited`.

## File map

```
crates/core/src/folders.rs                      modifies: names, check_owner_edit, named_in_subject, twin_is_stale, stale_line (Task 1); pair_changed_alone (Task 2)
crates/core/src/governor/done.rs                modifies: DoneEvidence.stale_twins, PairChangedAlone (Task 2)
crates/store/src/git.rs, crates/store/tests/git.rs   modifies, tests: tree_entries, last_commit, stale_twins (Task 3)
crates/runtime/src/transitions.rs, prompt.rs, orchestrator/session.rs   modifies, tests (Task 4)
crates/store/src/folder_docs.rs, crates/store/src/projections.rs   modifies: the owner's edits in the fold (Task 5)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies (Task 5)
docs/schemas/rpc.schema.json                    modifies: files.tree, files.read (Task 6); files.save (Task 7)
crates/runtime/src/daemon/documents.rs          creates (Task 6); save (Task 7)
crates/runtime/src/daemon.rs, daemon/web.rs     modifies: the module and its dispatch (Tasks 6, 7)
crates/runtime/src/daemon/gates.rs              modifies: off_the_worker pub(super) (Task 7)
crates/runtime/src/tools/folders.rs             modifies: land's author an Option (Task 7)
packages/protocol-client/src/client.ts          modifies: MethodName gains "files.save" (Task 7)
crates/store/src/requests.rs                    modifies: rederive_request, file_rederive_request, rederive_tasks (Task 8)
crates/runtime/src/tools/folders.rs, orchestrator/integrate.rs   modifies: file_rederive, its call after record_integrated (Task 8)
crates/runtime/src/orchestrator/requests.rs, crates/runtime/src/sprints.rs   modifies, tests (Task 8)
docs/SPEC.md, docs/design/catervas-folders.md, docs/plans/project-plan.md   modifies (Task 9)
```

## Interfaces

Consumes: step 01's `catervas_core::folders` items and `DoneRule::PairChangedAlone`, `PromptInput.active_roles`; step 03's `integration_lock`, `land` (its `author` widened here to `Option<&str>`), `Landed`, `LandRefusal`, `folder_docs` (`FolderChange`, `FolderDocDecision`, `FolderDocProposal`), `record_integrated` with `Subject::FolderChange`, `Integration`; `normalise` (`paths.rs:87`); `TaskId` (`contract.rs:17`); `Git`, `run_git`, `commit_count`, `file_at`, `integration_branch` (`git.rs:75`, `:399`, `:243`, `:733`); `ToolDeps` (`tools.rs:98-111`); `Failure`, `REFUSED`, `NOT_FOUND`, `INTERNAL_ERROR` (`web.rs:587`, `:612`, `:609`, `:607`); `gates::tests::{query, call}` (`gates.rs:1197`, `:1209`); `file`, `Filing`, `creation_bodies`, `placeholder_budget_usd` (`requests.rs:259`, `:252`, `:356`, `:80`); `default_reviewer_role`, `REVIEWER_ROLE_FOR`; `marketing_plans`; `post_system`; `plan_sprint_racing`, `PlannedBy` (`sprints.rs:285`, `:32`); `in_the_backlog`, `sprint_hold`.

Produces:

```rust
// catervas_core::folders
pub const DOCUMENT_NAMES: [(&str, &str); 11];
pub fn plain_name(stem: &str) -> String;
pub fn document_name(path: &str, title_of: &dyn Fn(&str) -> Option<String>) -> String;
pub fn document_order(path: &str) -> (usize, String);
pub const OWNER_EDIT_LIMIT_BYTES: usize = 262_144;
pub enum OwnerEditRefusal { OutsideFolders, NotMarkdown, AgentTwin, MarketingPlan, TooLarge }   // code(): "outside_folders", ...
pub fn check_owner_edit(path: &str, text_bytes: usize) -> Result<String, OwnerEditRefusal>; // Ok: normalised
pub enum Named { Task(TaskId), FolderChange(u64) }
pub fn named_in_subject(subject: &str) -> Option<Named>;
pub fn twin_is_stale(human_last: &str, twin_last: &str, human_commits_not_in_twin: u32) -> bool;
pub fn stale_line(stale_twins: &[String]) -> Option<String>;
pub fn pair_changed_alone(changed_paths: &[String], stale_twins: &[String]) -> Vec<String>; // was one argument
// catervas_core::governor::done::DoneEvidence gains `pub stale_twins: Vec<String>`
// catervas_store::git
pub struct TreeEntry { pub mode: String, pub blob: String, pub path: String }
pub struct PathCommit { pub sha: String, pub committed_at: String, pub author_name: String, pub subject: String }
impl Git {
    pub fn tree_entries(&self, rev: &str, dir: &str) -> Result<Vec<TreeEntry>, GitError>;
    pub fn last_commit(&self, rev: &str, path: &str, first_parent: bool) -> Result<Option<PathCommit>, GitError>;
}
pub fn stale_twins(git: &Git, branch: &str) -> Result<Vec<String>, GitError>;
// catervas_store::folder_docs::FolderChange (step 03's, with `escalated`) gains `pub edited: bool` (step 04 adds it)
// catervas_runtime::tools::folders::land: `author: &str` becomes `author: Option<&str>`
// catervas_store::requests
pub fn rederive_request(human: &str, twin: &str, owner: Role, reviewer: Role, max_cost_usd: f64) -> Value;
pub fn file_rederive_request(files: &ProjectFiles, log: &EventLog, wire: Value, twin: &str, (now, ids): (DateTime<Utc>, &EventIds)) -> Result<TaskContract, RequestError>;
pub fn rederive_tasks(log: &EventLog) -> Result<Vec<(TaskId, String)>, StoreError>;
// catervas_runtime::tools::folders
pub(crate) fn file_rederive(deps: &ToolDeps, team: &Team, change: u64) -> Result<Option<TaskId>, RequestError>;
// catervas_runtime::prompt::PromptInput gains `pub stale_twins: &'a [String]`
// event kinds: folder_doc.edited { path, change, sha }; task.created gains optional `rederives`
```

Wire (`rpc.schema.json`, snake_case): `files.tree {}` → `filesTreeResult { integration, folders: [{ folder, name, role, agent_id, documents: [{ path, name, twin: string|null }] }] }`. `files.read { path }` → `filesReadResult { document: fileVersion, twin: fileVersion|null, stale, rederiving, editable, waiting: { change, branch }|null }`, `editable` false for a marketing plan, `fileVersion { path, text, blob, last_change: { at, task_id|null, agent_id|null, author, by_you }|null }`; a path the tree would not list is `NOT_FOUND`. `files.save { path, text, base }` → `filesSaveResult { change: integer|null, branch: string|null }`.

## Tasks

### Task 1: Names, the owner's edit, and staleness in core

Files: `crates/core/src/folders.rs` (items and `mod tests`). Consumes: `normalise`, `is_human_document`, `TaskId`.

- `names_the_known_documents_and_the_rest_plainly` — `docs/catervas/product/spec.md` is "Product description", `…/architecture/jobs.md` "Scheduled jobs", `…/marketing/brand/persona.md` "Brand persona", `…/delivery/cadence.md` "Sprint rhythm", `…/delivery/ceremony-notes.md` "Ceremony notes", `…/engineering/how_we_test.md` "How we test"; `plain_name("product")` is "Product". RED: no function.
- `names_a_plan_by_its_title` — with `title_of` answering `CTV-12` "pie pre-orders" and `MP-3` "Autumn at Corner Bakery": `…/architecture/plans/CTV-12.md` is "Plan: pie pre-orders", `…/marketing/plans/MP-3.md` "Plan: Autumn at Corner Bakery", `…/architecture/plans/CTV-99.md` (unknown) "CTV 99", `…/architecture/plans/old/CTV-12.md` "CTV 12". RED.
- `orders_the_known_documents_first` — sorting `[plans/CTV-9.md, overview.md, notes.md, features.md]` under `architecture/` by `document_order` gives overview, features, notes, plans/CTV-9. RED.
- `checks_the_owners_edit` — `docs/catervas/product/roadmap.md` with 10 bytes is `Ok` of itself, and `./docs/catervas/product/roadmap.md` of the normalised path; `docs/catervas/../../etc/x.md`, `/docs/catervas/x.md`, `docs/other/x.md`, `docs/Catervas/product/x.md` are `OutsideFolders`; `…/product/notes.txt` `NotMarkdown`; `…/product/roadmap.agent.md` `AgentTwin`; `…/marketing/plans/MP-3.md` `MarketingPlan`, and `…/marketing/plans/notes.md` `Ok`; 262,145 bytes `TooLarge` and 262,144 `Ok`; each `code()` is its snake_case name. RED.
- `finds_what_a_merge_subject_names` — `Merge CTV-12: Pie pre-orders` is `Task(CTV-12)`; `CTV-7: Gift cards (#4)` `Task(CTV-7)`; `Merge CTV-12: follow CTV-3` `Task(CTV-12)`; `Merge docs/folder-3: product/roadmap.md` `FolderChange(3)`; `Folder change 4: product/spec.md (#9)` `FolderChange(4)`; `Merge pull request #4 from me/docs/folder-5` `FolderChange(5)`; `Merge docs/folder-3: architecture/plans/CTV-12.md` `FolderChange(3)`; `docs(product): edit roadmap.md`, `folder-x` and `CTV-x` none. RED.
- `a_twin_is_stale_when_its_document_moved_past_it` — equal shas: not stale; different shas with 0 commits of the document's not in the twin's: not stale; with 1: stale. RED.
- `says_which_twins_are_stale` — `stale_line(&[])` is `None`; for `[docs/catervas/product/roadmap.agent.md]` it is exactly the Decisions' line. RED.

- [ ] `feat(core): name, check and date the owner's documents`

### Task 2: A stale twin may change alone

Files: `folders.rs` (`pair_changed_alone`'s second argument; step 01's `finds_a_pair_changed_alone` passes `&[]`); `governor/done.rs` (`stale_twins`, the rule passing it; every `DoneEvidence { .. }` literal in the workspace gains it, `Default` covers the rest).

- `lets_a_stale_twin_change_alone` (`folders.rs`) — `[roadmap.agent.md]` with stale `[roadmap.agent.md]` gives nothing; with stale `[spec.agent.md]` gives `[roadmap.agent.md]`; `[roadmap.md]` with stale `[roadmap.agent.md]` still gives `[roadmap.md]`. RED: one argument.
- `passes_a_diff_that_only_brings_a_stale_twin_up_to_date` (`done.rs`) — allowed `docs/catervas/**`, changed `[…/product/roadmap.agent.md]`: with `stale_twins` naming it, no `PairChangedAlone`; without, `PairChangedAlone` with step 01's message. RED.

- [ ] `feat(core): let a diff that changes only a stale twin pass`

### Task 3: Git reads the folders

Files: `crates/store/src/git.rs` (the items, `--literal-pathspecs` on each path given); `crates/store/tests/git.rs` (each test `#[ignore = "needs the git program: cargo xtask check --integration"]`, on `TempRepo`, `git/fixtures.rs:22`, commits and merges made with its helpers). Consumes: Task 1's `twin_is_stale`, `is_human_document`, `agent_twin`.

- `lists_the_tree_under_a_folder_with_each_mode` — `docs/catervas/product/{spec.md,spec.agent.md}`, a link `docs/catervas/product/l.md` and `src/a.rs` committed: `tree_entries("main", "docs/catervas")` holds the three under it, the link with mode `120000`, each blob as `git rev-parse main:<path>` gives it; a missing dir gives `[]`. RED.
- `finds_the_last_commit_that_changed_a_path` — a branch changing `x.md` merged `--no-ff` with message `Merge CTV-2: t`: with `first_parent` the answer is the merge (subject `Merge CTV-2: t`); without, the branch's commit; a path never changed is `None`; the author name is the commit's. RED.
- `finds_the_twins_the_owner_left_behind` — `spec.md` and `spec.agent.md` committed together: `stale_twins` is `[]`; a branch changing `spec.md` alone, merged: `[docs/catervas/product/spec.agent.md]`; a branch from there changing the twin, merged: `[]`; a branch changing the twin made before a second change of `spec.md` and merged after it: stale again; one merge bringing a branch where `spec.md` changed after its twin, so the twin's own last commit is not after the document's: `[]`, both paths' first-parent commit being that merge. RED.

- [ ] `feat(store): read the folders and their stale twins from git`

### Task 4: Sessions and the Definition of Done know the stale twins

Files: `transitions.rs` (the `DoneEvidence` at `:543` gains `stale_twins` from `stale_twins(&self.git, &integration_branch(team, &self.git)?)` when a changed path is a twin by `human_of_twin`, else empty); `prompt.rs` (`PromptInput.stale_twins`, `rules_section` appending `stale_line`, `Inputs::full` passing `&[]`); `orchestrator/session.rs` (`:1031-1051` fills it; a git error gives `&[]`).

- `fills_the_stale_twins_for_a_diff_that_changes_a_twin` (`transitions.rs`, integration) — main holds `spec.md`, `spec.agent.md`, `roadmap.md`, `roadmap.agent.md`, then a commit of `spec.md` alone; a task's branch changing `spec.agent.md` alone: its done evidence's `stale_twins` is `[…/spec.agent.md]` and `evaluate_done` reports no `PairChangedAlone`; one changing `roadmap.agent.md` alone fails `PairChangedAlone`. RED.
- `the_team_rules_say_which_twins_are_stale` (`prompt.rs`) — with `stale_twins: [docs/catervas/product/roadmap.agent.md]` the Team rules end with the Decisions' line after the folders line; with none they end with the folders line. RED.
- `the_prompt_names_a_stale_twin` (`orchestrator/session.rs`, integration) — after a commit of `roadmap.md` alone on main, the Product Manager's triage session prompt holds `- stale twins: docs/catervas/product/roadmap.agent.md.`; with the integration branch named in the team's policy missing from git, the session still starts, with no stale line. RED.

- [ ] `feat(runtime): tell sessions and the Definition of Done which twins are stale`

### Task 5: The owner's edit and the re-derive in the log

Files: `event.schema.json` (`folder_doc.edited` in `eventKind` and its `oneOf` branch, `folderDocEditedBody { path, change, sha }`, `path` as step 03's `folder_doc.*` paths, `change` an integer from 1, `sha` at least 1 character; `taskCreatedBody` (`:277`) gains optional `rederives`, matching `^docs/catervas/.+\.agent\.md$`); `crates/protocol/src/event.rs` (the variant, the body map, `kind()`, `EVERY_KIND` one longer than step 03 leaves it; not about one contract), `lib.rs`'s `KINDS`, `event/fixtures.rs`; `crates/store/src/folder_docs.rs` (step 03's fold: `edited` and the owner's changes), `crates/store/src/projections.rs` (`apply` passes over `folder_doc.edited`).

- `reads_a_folder_doc_edited_event` — `{ path: "docs/catervas/product/roadmap.md", change: 2, sha: "a1" }` reads and writes back unchanged; `path: "src/a.md"` is refused at `/body/path`, `change: 0` at `/body/change`; it needs no task id. RED.
- `folds_an_owners_edit_as_a_change` (`folder_docs.rs`) — `folder_doc.edited { path: …/roadmap.md, change: 2, sha }` gives `FolderChange { change: 2, paths: [path], approved: false, integrated: false, escalated: false, edited: true }`, integrated once `folder_change.integrated { change: 2 }` follows; a `folder_doc.written` change has `edited: false`; projecting the log with the event passes. RED: the kind is unknown to the fold.
- `reads_a_task_created_that_rederives` — `rederives: "docs/catervas/product/roadmap.agent.md"` reads; `"docs/catervas/product/roadmap.md"` is refused at `/body/rederives`. RED.

- [ ] `feat(protocol): record the owner's edit of a folder document`

### Task 6: The folders and one document, answered

Files: `rpc.schema.json` (both names in `queryName`, `filesTreeQuery`, `filesReadQuery`, their results as Interfaces says); `daemon/documents.rs` (`QUERIES: [&str; 2]`, `query`, catching the projections up first as `board.rs:50` does); `daemon.rs` (`mod documents;`); `web.rs` (the query dispatch, `:866`). Tests in `documents.rs`, integration, on a daemon over `TestProject::new` (`tools/fixtures.rs:165`) with `a_team_of_three` plus `with_the_designer` and `with_the_marketing_specialist` (`:38`, `:49`, `:56`), its state made as `TestDaemon::new` makes it (`daemon/fixtures.rs:45`), answers checked with `gates::tests::query`.

- `lists_each_active_roles_folder_with_its_documents` — main holds `product/{spec,roadmap}.md` with twins, `architecture/{overview.md,plans/CTV-1.md}`, `marketing/brand/persona.md`, and a link `product/l.md`; CTV-1 titled "Pie pre-orders"; the team's policy `pull_request`: `integration` is `pull_request`; folders in table order `product` (`pm`, "Product"), `architecture` (`ada`), `engineering` (`dev-a`, `documents: []`), `design` (`iris`), `marketing` (`kai`), no `delivery`; product's documents spec ("Product description", twin `…/spec.agent.md`) then roadmap, no twin entry, no link; architecture's overview then "Plan: Pie pre-orders". With `kai` paused, no `marketing`. RED: unknown query.
- `reads_a_document_with_its_twin_and_who_changed_it` — after a merge `Merge CTV-1: Pie pre-orders` changing `roadmap.md` and its twin, CTV-1 assigned to `pm`: `files.read { path: …/roadmap.md }` has both texts, each `blob` as `rev-parse`, `last_change` `{ task_id: "CTV-1", agent_id: "pm", by_you: false }`, `stale: false`, `waiting: null`; `overview.md` has `twin: null` and `editable: true`; `…/marketing/plans/MP-1.md` has `editable: false`; after a merge `Merge docs/folder-2: …` of a change that `folder_doc.written { written_by: "ada", change: 2 }` names, overview's `last_change.agent_id` is `ada`. RED.
- `reads_the_owners_waiting_change_from_its_branch` — policy `manual`, the owner's save of `roadmap.md`: `files.read` answers `waiting: { change: 1, branch: "docs/folder-1" }`, the saved text and that branch's blob, `last_change.by_you` true, `stale: false`; after `FolderChangeIntegrate { change: 1 }`, `waiting: null`, the saved text from main, `last_change` `{ by_you: true }` (the merge `Merge docs/folder-1: …`), `stale: true` and `rederiving: true`; an owner's change escalated (`folder_change.escalated`) gives `waiting: null` and main's text. RED.
- `reads_nothing_the_tree_would_not_list` — `…/roadmap.agent.md`, `…/l.md`, `…/missing.md`, `src/a.rs` and `docs/catervas/../README.md` are each `NOT_FOUND`. RED.

- [ ] `feat(runtime): answer the team's folders and their documents`

### Task 7: The owner's save

Files: `rpc.schema.json` (`filesSaveRequest { path, text, base }`, `base` 40 or 64 hex, in the requests' `oneOf` beside `contractSaveRequest`; `filesSaveResult`); `documents.rs` (`METHODS: [&str; 1]`, `call`, run with `off_the_worker`, `gates.rs:806`, made `pub(super)`); `web.rs` (`:674-679`, `:714`); `client.ts` (`"files.save"`, `:30-61`); `tools/folders.rs` (`land`'s `author` to `Option<&str>`, step 03's callers passing `Some`). Tests as Task 6's, with `gates::tests::call`.

- `saves_the_owners_edit_as_a_folder_change` — policy `auto_merge`: `files.save` of `overview.md` with its blob answers `{ change: 1, branch: "docs/folder-1" }`; `docs/folder-1` is main plus one commit with message `docs(architecture): edit overview.md` and the new text; one `folder_doc.edited { path, change: 1, sha }` is recorded; main holds the old text until the next tick, which merges it (step 03); the same text again answers `{ change: null, branch: null }` and records nothing. RED: unknown method.
- `refuses_each_edit_the_owner_may_not_save` — each refusal by its code and its pointer (`/path`, `/text`, `/base`), nothing recorded and no branch made: `docs/catervas/../x.md`, `README.md` and `docs/catervas/delivery/x.md` on a team with no Scrum Master (`outside_folders`), `…/notes.txt`, `…/roadmap.agent.md`, `…/marketing/plans/MP-1.md` (`marketing_plan`), 262,145 bytes, a stale `base`, a deleted file (`file_changed`), `roadmap.md` changed uncommitted in the root checkout (`file_busy`), `docs/catervas` a link (`is_a_link`), and a second save of `overview.md` under `manual` while change 1 waits (`change_waiting`). RED.
- `waits_for_an_integration_to_finish` — with `integration_lock` held by the test, a save on another thread is still running 300 ms in and has recorded nothing, as `waits_for_the_lock` (`tools/fixtures.rs:585`) checks a step for a mutex, and saves once the lock is let go. RED: no lock taken.

- [ ] `feat(runtime): save the owner's edit as a folder change`

### Task 8: The re-derive

Files: `crates/store/src/requests.rs` (the three items; `Filing` gains `rederives`; `creation_bodies` its triage, reason `a re-derive of <twin>`); `crates/runtime/src/tools/folders.rs` (`file_rederive`); `orchestrator/integrate.rs` (after `record_integrated` records `folder_change.integrated`, `file_rederive`, a refusal posted as the Decisions say); `orchestrator/requests.rs:289`; `sprints.rs` (`plan_sprint_racing` at `:349`).

- `files_the_rederive_when_the_owners_change_is_integrated` (`integrate.rs`, integration) — policy `manual`: the owner's save of `roadmap.md` files nothing; `FolderChangeIntegrate { change: 1 }` files a task whose contract is the Decisions' (assignee `product_manager`, reviewer `architect`, `allowed_paths` the twin alone, the title and intent naming the two paths) with `task.created { created_by: "catervas", rederives }` and `request.triaged` by `catervas`; a second integrated save while it is `draft` files none; moved to `in_progress` (`TestProject::moved`, `tools/fixtures.rs:395`), a third files another; an integrated save of `overview.md`, and an agent's integrated `folder_doc.written` of `roadmap.md`, file none. Under `auto_merge` the tick's integration files it. RED.
- `judges_a_rederive_as_filed_whole` (`orchestrator/requests.rs`, integration, `Harness::new` with no judging) — a re-derive filed with `file_rederive_request`, which the first tick moves to `refining`: the next tick starts no session (`adapter.started()` is empty) and the governor's `refining → ready` is recorded, moved or refused by `governor`, as for a breakdown's child. RED: the Product Manager's refine session starts.
- `plans_the_waiting_rederives_first` (`sprints.rs`) — under `plan_in_sprints`, a Backlog re-derive CTV-2 (filed with `file_rederive_request`, then moved to `ready`) and a ready CTV-3, a sprint opened (`TestProject::open_sprint`, `:360`) with CTV-3's budget: the Scrum Master planning `[CTV-3]` leaves the sprint's `task_ids` and `sprint.planned`'s as `[CTV-2, CTV-3]`; naming both plans each once; a governor plan, and the policy off, add none. RED.

- [ ] `feat(runtime): file the twin's re-derive when the owner's change is integrated`

### Task 9: Spec

`docs/SPEC.md`: 3 (the sprint exceptions bullet gains the re-derive, planned first in the sprint's first plan); 5.4 item 2 (`pair_changed_alone` lets a diff change a stale twin alone); 5.14 (the owner's save is a folder change, integrated by the team's policy as step 03's are); 5.17 (the owner's edits, their refusals, `file_changed`, `file_busy`, `marketing_plan` and `change_waiting`, the waiting change shown from its branch, staleness read from git, the Team rules' stale line, the re-derive at integration, its dedupe and its judgment as filed whole); 8.5 (`folder_doc.edited`, `task.created.rederives`); the next spec revision. `docs/design/catervas-folders.md`, "How documents are written" and "The Files page": the owner's edit is a folder change integrated by the team's policy, not a commit at once, and its re-derive is filed when it is integrated; its steps table splits row 04 into 04, "The Files page's server", and 04b, "The Files page in the web app". `docs/plans/project-plan.md`: row 04 becomes this step's Delivers with Spec `3, 5.4, 5.14, 5.17, 8.5`, and row 04b the page's with Spec `4.5`. ADR 0051 is not touched.

- [ ] `docs(spec): record the owner's edits, stale twins and their re-derive`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
