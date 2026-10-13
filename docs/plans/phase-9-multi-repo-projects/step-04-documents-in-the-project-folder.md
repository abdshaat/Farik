# Phase 9, step 04: Documents in the project folder

Status: ready (readiness review 2026-10-12, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.5, 5.3, 5.14, 5.17, 8.5; F5
Depends on steps 01 and 02 of this phase, planned and ready, not yet executed:
- from step 01: `read_repos`, `Repos`, `repo.remove` in `daemon/repos.rs` and its lock, `EVERY_KIND` at 115 (step 01's 114, plus `document_rewrite.started` from main's HTML documents)
- from step 02: `ReadinessContext.repos`, `TestProject::project_folder`, `Harness::project_folder`, `Harness::project_folder_with`
- from step 03: `rules_section`'s new argument tuple and `PromptInput.members`, which this step extends
- ADR 0057 with its amendment ("where a task works": a builder's task with no `repo` writes the project folder's documents)

Nothing of steps 02b, 02c or 05 to 08.
Mockups: none; the Files page's picker is step 07's.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12 (one round): ready, no blocking finding; S1–S6 and N1–N7 folded below.
Amended 2026-10-13 for the HTML documents merged from main (ADR 0058); status: ready.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 93593b6.

## Goal

In a project folder, each member has its own role folders in the project folder: `docs/catervas/repos/<name>/architecture/`, `…/engineering/` and `…/design/`. Each is owned by the same role as its top-level counterpart. Every phase 8 rule knows them:
- readiness's `folder_owned`
- the folder write tool
- the Files page's tree, read and save
- each session's folders line

A folder change in a project folder, which has no remote, is merged into its `main` at once, whatever the team's `integration` policy. Removing a member removes its `repos/<name>/` folder in one folder change.

A one-repo project is unchanged: it has no `repos/` folders.

Out of scope:
- the Architect's `system`, `interfaces` and `deployment` documents and their skills (05)
- the per-member Catervafication tasks (06)
- the Files page's repository picker (07)

## Decisions

- **Member folders.** Core's `MEMBER_FOLDERS: [(Role, &str); 3] = [(Architect, "architecture"), (SoftwareDeveloper, "engineering"), (UiUxDesigner, "design")]`. A member's folder is `docs/catervas/repos/<name>/<segment>`, `member_folder(role, name)`.
  - The product, delivery and marketing folders stay project-wide. The top-level `architecture/`, `engineering/` and `design/` stay too, owned as in phase 8: in a project folder the top-level `architecture/` holds the system documents (step 05) and the plans.
  - Member names are passed as `&[&str]` everywhere in this step.
  - `folder_of(path, members) -> Option<(Role, String)>` answers the folder holding a path and its owner. It tries the role folders, then each member's member folders, by segment, as `check_folder_doc_path` strips its prefix today (`crates/core/src/folders.rs:233-257`).
  - A document is `<stem>.md` or `<stem>.html` (ADR 0058); every rule here is by folder, so member folders hold either format, and a new member document is written in the team's format, as any new document is.
  - Rejected: one `repos/` folder per member owned by the Architect alone, which would take the Developer's conventions from it.
- **Readiness.** `folder_owned` (`crates/core/src/governor/readiness.rs:693`) also holds every member folder of every name in `context.repos` (step 02), owned by its role, with the same "while the role has an active agent" test and `reaches_the_folder` (`governor/paths.rs:141`). Its message names a member folder in full, as it names a top-level one (`readiness.rs:724`): `docs/catervas/repos/<name>/<segment>/ (<plain role>)`. A glob `docs/catervas/repos/**` reaches them all and fails closed as today. In a one-repo project `repos` is `None` and nothing changes.
- **The folder write tool.** `check_folder_doc_path` (`folders.rs:233`) gains `members: &[&str]` and accepts a path under the caller's role folder, or under that role's member folder of a named member. `write_folder_doc` (`tools/folders.rs:398`) passes `read_repos`' names, or `&[]` in a one-repo project; a `repos.yaml` it cannot read fails the call (`ToolError::Failed` with the files' sentence). `check_input` (`:447`) takes the folder from `folder_of` in place of `role_folder` (`:457`), so `Checked.folder` (`:383`) becomes a `String`, and `checked_path`'s `Outside` words (`:585`) name the role folder and its member folders. The format check after it (`check_document_format`, `:470-482`) is unchanged. `WriteFolderDocInput.path`'s description (`:37`) names the member folders.
  - **Commit messages.** `folder_doc_message` (`folders.rs:350`) is given the folder `folder_of` found, so the paths are stripped as for a top-level folder. Its scope for a member folder is `<name>-<segment>`: `docs(app-architecture): features.html by Ada`. The owner's save (`documents.rs:139-141`) likewise gives `docs(app-architecture): edit features.html`.
- **Names on the Files page.** `document_name` and `document_order` (`folders.rs:400`, `:421`) match by stem (`DOCUMENT_STEMS`, `:372`). They read a path under `repos/<name>/` as the same path without `repos/<name>/` before `stem_of`, so a member's `overview.html` is "Overview" and its order's index (`.0`) is the top-level one's. Plans stay top-level only: `document_name` tests "in a role folder's `plans/`" against the original path, so a member's `plans/CTV-1.html` gets no plan title.
- **Folder changes in a project folder merge at once.**
  - `integrate_locked`'s route (`orchestrator/integrate.rs:311-315`), for `Subject::FolderChange` in a project folder (`files.is_project_folder()`), is `merge(.., push: false)` whatever the policy.
  - `awaiting_folder_changes` (`:195`) no longer returns `None` under `manual` (`:199`) in a project folder, so the next tick merges the change.
  - `land`'s `next` sentence (`next_by`, `tools/folders.rs:115`, called at `:183`) in a project folder is `next_by(Integration::AutoMerge, ..)`'s.
  - `files.tree`'s `integration`, and `proposals_list`'s (`tools/folders.rs:276`, which `DocumentChanges.tsx:204` words its note by), answer `auto_merge` in a project folder, since that is what a folder change there gets. `integration_rows` (`:330`), the `manual` folder changes "waiting for you to add" in `waiting.list` (`gates.rs:253`), answers `[]` there. So the page's words (`FileDocument.tsx:25-34`, `:62-68`, `:207`) say "adds it" truthfully.
  - A task's branch in the project folder (a docs task) still follows the team's policy (5.14). Only folder changes are forced, since nothing reviews them on a forge.
  - Rejected: pushing a folder change to an `origin` the owner added to the project folder by hand. Catervas neither pushes nor reads one for a folder change (ADR 0057); a docs task's branch there follows the policy (step 02c).
  - Step 02c restructures `integrate_locked` around `IntegrationTarget` before this step lands, so this route is added to that code, for the project root's target. 02c's guard test of a folder change in the project folder still passes, now because the route is forced.
- **Removing a member's documents** needs no new git code: main's HTML change gave `Git::commit_files` a `removed: &[String]` (`git.rs:747-801`, with its `Linked`, `Busy` and `Unchanged` guards and its rollback) and `land` the same `removed` (`tools/folders.rs:135`), whose `Waiting` check matches each path exactly (`:150-160`).
- **`tools::folders::remove_folder(deps, team, folder, name)`** lists the folder's files on the integration branch (`git.tree_entries(into, folder)`, regular files and links), and calls `land(deps, team, &[], &files, "docs(repos): remove <name>", None)`. Every file is listed, so a waiting change to any of them is found by exact path. An empty list answers `Ok(None)` (`commit_files`' `Unchanged`), and nothing is recorded. Otherwise it records `folder_doc.removed { path, change, sha }`, `path` the folder. After it lands, it wakes the tick (`state.wakes().notify_one()`, as `documents.rs:366-377` does).
- **Step 01's `repo.remove`** calls it after `repo.removed`, under the lock it already holds. A removal that cannot land is answered `INTERNAL_ERROR` with "<name> is removed, but its documents could not be: <the refusal's words>; they stay in docs/catervas/repos/<name>/ until you remove them by hand". The member stays removed. There is no retry: those documents stay on `main`, listed nowhere and owned by no folder.
  - The fold (`crates/store/src/folder_docs.rs:149`) reads `folder_doc.removed` as a `FolderChange { paths: [folder], .. }`, like `folder_doc.written`.
- **The folders line** (`folders.rs:591`) gains `members: &[&str]`. In a project folder, for a role with member folders, it adds `Each repository has its own <segment>/ folder at <list>, which is yours too.`, where `<list>` is each member's `repos/<name>/<segment>/`, joined by `, ` with ` and ` before the last: `Each repository has its own architecture/ folder at repos/app/architecture/ and repos/infra/architecture/, which is yours too.`
  - `PromptInput` (`prompt.rs:20`) gains `member_names: &'a [String]`, which `rules_section` (`:490`, whose last argument is now `format: DocumentFormat`) passes to `folders_line` in its tuple. `Inputs::full` (`:744`) sets it to `&[]`, and `session_spec_without` (`orchestrator/session.rs:1223`) to `read_repos`' names. A `repos.yaml` it cannot read leaves it empty, as an unreadable stale twin leaves its line out (`session.rs:1284`).
- **The Files page's tree.** `files.tree`'s folder rows (`rpc.schema.json:2775-2782`, `daemon/documents.rs:496`) gain `repo: string | null`. The top-level folders come first, as today, with `repo: null`. Then, for each member in `repos.yaml` order, each member folder whose role has an active agent, with `repo` the member's name, its `name` `plain_name(segment)` and its `agent_id` as the top-level's. The member rows are made beside `folders()` (`documents.rs:437`), which keeps to `ROLE_FOLDERS` and its `Folder.path: &'static str`, with the same listing: `.md` and `.html` files, the integration branch's and those a task waiting to merge holds (`task`). `files.read` finds a document among the top-level rows and the member rows, where it searches `folders()` today (`documents.rs:634-638`), and `files.save` finds its folder through `folder_of`: `save`'s `ROLE_FOLDERS` loop (`documents.rs:115-123`) becomes `folder_of` with the owner active. A `repos.yaml` that cannot be read makes `files.tree`, `files.read`, `files.save` and `files.ask_change` answer `INTERNAL_ERROR` with the files' sentence. `filesTreeResult`'s description (`rpc.schema.json:2768`) names `repo`. The web's `FilesTree` type (`apps/web/src/pages/files.ts:8-23`) gains `repo: string | null` and nothing reads it yet (07). An HTML member document needs nothing more to be shown: `/documents/raw` serves any `.html` under `docs/catervas/` (`documents.rs:763-770`).
- **Asking for a change to a member's document.** `files.ask_change` (`documents.rs:185`) finds the owner through `folder_of` in place of the `ROLE_FOLDERS` prefix (`:200-204`), and the document among the top-level and member rows as `files.read` does, where it searches `folders()` today (`:222-226`), so a member's document is askable as `files.read` already answers (`:684-691`). Its task's `allowed_paths` are the member document alone: a member folder holds no human document, so there is no twin. The refusals are unchanged.
- **"Rewrite documents as HTML" leaves member folders out.** `rewrite()` (`documents.rs:267`) and its `folders()` loop (`:293`) stay on the role folders. A member's documents are written in the team's format from the start (this phase adds them); a format switch for member folders is not in this phase. The member rows are listed beside `folders()`, not through it, so `rewrite()` never sees them.
- **Events.** `folder_doc.removed { path, change, sha }`, about no contract and naming no actor. `path` is a new `$defs.folderDocFolder`: `^docs/catervas/repos/[a-z][a-z0-9]*(-[a-z0-9]+)*$`, at most 512 characters. It shares `folder_doc.edited`'s property names, but the two patterns are disjoint (a folder has no `.`, and `folderDocPath` always ends in `.md` or `.html`, `event.schema.json:1734-1737`), so `oneOf` matches one branch. `eventBodyWire`'s description (`:177`) is amended to say that branches are told apart by their required properties, or by patterns no value can match both of. `EVERY_KIND` goes to 116. The fold's arm is `FolderDocRemoved(body) if is_unattended(event)`, giving `written_by: None, proposals: []`.

## File map

```
crates/core/src/folders.rs                         modifies, tests: MEMBER_FOLDERS, member_folder, folder_of, check_folder_doc_path's members, document_name/order, folders_line's members (Task 1)
crates/core/src/governor/readiness.rs, readiness/fixtures.rs   modifies, tests: folder_owned's member folders (Task 1)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies: folder_doc.removed (Task 2)
crates/store/src/folder_docs.rs, crates/store/src/projections.rs   modifies, tests: the fold reads it (Task 2)
crates/runtime/src/tools/folders.rs                modifies, tests: write_folder_doc's members, land's next, remove_folder (Task 3)
crates/runtime/src/orchestrator/integrate.rs       modifies, tests: folder changes merged at once in a project folder (Task 3)
crates/runtime/src/prompt.rs, orchestrator/session.rs   modifies, tests: PromptInput.member_names, folders_line's members (Task 3)
crates/runtime/src/daemon/gates.rs                 modifies: integration_rows in a project folder (Task 3)
crates/runtime/src/daemon/documents.rs, docs/schemas/rpc.schema.json, apps/web/src/pages/files.ts   modifies, tests: the tree's repo, read and save of member folders, integration (Task 4)
crates/runtime/src/daemon/repos.rs                 modifies, tests: repo.remove removes the member's folder (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 01's `read_repos`, `repo.remove` and its lock, `EVERY_KIND` at 115
- step 02's `ReadinessContext.repos` and the project-folder fixtures
- `ROLE_FOLDERS`, `role_folder`, `check_folder_doc_path`, `FolderDocPathRefusal`, `document_name`, `document_order`, `folders_line`, `plain_name` (`folders.rs`)
- `reaches_the_folder` (`paths.rs:141`)
- `land` with its `removed` (`tools/folders.rs:135`), `next_by`, `Landed`, `LandRefusal`, `write_folder_doc` (`tools/folders.rs`)
- `Git::commit_files` with its `removed`, `Git::tree_entries`, `CommitOutcome` (`git.rs:747`, `:529`, `:101`)
- `folder_docs`, `FolderChange` (`folder_docs.rs:149`, `:68`)
- `Subject::FolderChange`, `integrate_locked`, `merge`, `awaiting_folder_changes`
- `documents::{folders, tree, read, save, ask_change}`; `change_request` (`crates/store/src/requests.rs:504`)

Produces:

```rust
// catervas_core::folders
pub const MEMBER_FOLDERS: [(Role, &str); 3];
pub fn member_folder(role: Role, repo: &str) -> Option<String>;
pub fn folder_of(path: &str, members: &[&str]) -> Option<(Role, String)>;
pub fn check_folder_doc_path(role: Role, path: &str, members: &[&str]) -> Result<FolderDocPath, FolderDocPathRefusal>;   // was two arguments
pub fn folders_line(role: Role, active_roles: &[Role], members: &[&str]) -> String;   // was two
// catervas_runtime::tools::folders
pub(crate) fn remove_folder(deps: &ToolDeps, team: &Team, folder: &str, name: &str) -> Result<Option<Landed>, LandRefusal>;
// catervas_runtime::prompt::PromptInput gains `pub member_names: &'a [String]`
// event: folder_doc.removed { path, change, sha }
// rpc: filesTreeResult folder rows gain `repo: string|null`
```

## Tasks

### Task 1: Each member has its own folders

Files: `folders.rs` (the items and their tests); `readiness.rs` (`folder_owned`); `readiness/fixtures.rs`.

- `finds_a_paths_folder_among_the_members`: with members `[app, infra]`:
  - `docs/catervas/repos/app/architecture/overview.md` is `(Architect, docs/catervas/repos/app/architecture)`
  - `…/repos/infra/engineering/conventions.md` is the Developer's
  - `…/repos/web/architecture/x.md` (no such member), `…/repos/app/product/x.md` (no member product folder) and `…/repos/app/x.md` are `None`
  - `docs/catervas/architecture/system.html` is the top-level Architect's

  RED.
- `writes_only_the_callers_member_folders`: `check_folder_doc_path(Architect, "docs/catervas/repos/app/architecture/features.md", &["app"])` is `Ok`. The same path for the Developer, or with members `&[]`, is `Outside`. `…/repos/app/engineering/workflow.md` for the Developer is `Ok`. RED.
- `names_a_members_documents_as_its_own`: `document_name("docs/catervas/repos/app/architecture/overview.html", ..)` is "Overview", and its `document_order(..).0` equals the top-level `overview.md`'s (either format, by stem). `…/repos/app/architecture/plans/CTV-1.html`, with `CTV-1` titled, is "CTV 1", not a plan title. RED.
- `holds_a_members_folders_for_their_owner` (`readiness.rs`), with `repos: Some([app])` and an active Architect:
  - a Developer's task with `allowed_paths: [docs/catervas/repos/app/architecture/**]` fails `FolderOwned` naming `docs/catervas/repos/app/architecture/ (Architect)`
  - `[docs/catervas/repos/**]` fails it too
  - `[docs/catervas/repos/app/engineering/**]` passes
  - with `repos: None` the first passes

  RED.
- `tells_a_role_its_member_folders`: `folders_line(Architect, [..], &["app", "infra"])` ends with exactly `Each repository has its own architecture/ folder at repos/app/architecture/ and repos/infra/architecture/, which is yours too.`; with three members the list is `a, b and c`; with `&[]` it is today's line exactly. A member folder's `folder_doc_message` is `docs(app-architecture): features.html by Ada`. RED.

- [x] `feat(core): give each member its own role folders`

### Task 2: A folder change that removes a folder

Files: the event's files; `folder_docs.rs`; `projections.rs` (pass arm). The removal's commit is `land`'s, tested through `repo.remove` in Task 4.

- `reads_a_folder_doc_removed_event`: `{ path: "docs/catervas/repos/app", change: 3, sha: "a1" }` reads and writes back unchanged. `path: "docs/catervas/product"` is refused at `/body/path`. It needs no task id. RED.
- `folds_a_removal_as_a_change` (`folder_docs.rs`): it gives `FolderChange { change: 3, paths: [docs/catervas/repos/app], approved: false, proposals: [], written_by: None, integrated: false, escalated: false, edited: false }`, and integrated once `folder_change.integrated { change: 3 }` follows. RED.

- [ ] `feat(store): record a folder's removal as a folder change`

### Task 3: The project folder's folder changes land at once

Files: `tools/folders.rs` (`write_folder_doc`'s members, `land`'s sentence, `remove_folder`); `integrate.rs` (the route, `awaiting_folder_changes`); `prompt.rs` and `orchestrator/session.rs` (the folders line's members). Tests are integration tests on step 02's `Harness::project_folder_with("x", &["app"], ..)`, which takes a change to the team's wire as `Harness::new`'s callers make one: here `with_the_designer`'s six and the policy each test names.

- `writes_a_members_document_in_a_ceremony` (`tools/folders.rs`): the Architect's `write_folder_doc` at sprint review of `docs/catervas/repos/app/architecture/features.html` (the team's format, HTML by default) lands `docs/folder-1` and records `folder_doc.written`. Its answer's `next` is `auto_merge`'s sentence though the team's policy is `pull_request`. RED: `Outside`.
- `merges_a_folder_change_at_once_in_a_project_folder` (`integrate.rs`): under `manual`, and again under `pull_request`, the next tick merges `docs/folder-1` into the project folder's `main` with no push and no `gh` call, and records `folder_change.integrated`. `integration_rows` answers `[]` and `proposals_list`'s `integration` is `auto_merge`. In a one-repo `Harness` under `manual` the tick leaves it waiting, as today. RED.
- `a_sessions_folders_line_names_its_member_folders` (`orchestrator/session.rs`): the Architect's planning prompt in the project folder holds the Decisions' sentence naming `repos/app/architecture/`. RED.

- [ ] `feat(runtime): land a project folder's document changes at once`

### Task 4: The Files page sees each member's folders

Files: `daemon/documents.rs` (`tree`, `read`, `save`, `ask_change`); `rpc.schema.json` (`filesTreeResult`'s row gains required, nullable `repo`); `apps/web/src/pages/files.ts`; `daemon/repos.rs` (`repo.remove` calls `remove_folder`). Tests drive `harness.daemon` as `documents.rs`'s do (its `tree` and `read` helpers, `:902`, `:906`), on `Harness::project_folder_with` with members `app` and `infra`, the suggested six, and the policy each test names.

- `lists_each_members_folders_after_the_projects`: with `docs/catervas/repos/app/architecture/overview.html` on `main`, `files.tree` lists the top-level folders with `repo: null`, then `app`'s `architecture` (`name: "Architecture"`, `repo: "app"`, the document "Overview") and `engineering` (no documents), then `infra`'s two. `integration` is `auto_merge` under the team's `manual`. In a one-repo project no row has a `repo` but `null`. RED.
- `reads_and_saves_a_members_document`: `files.read` of `app`'s `overview.html` answers it as a top-level document is answered. `files.save` of it lands a folder change that the next tick merges into `main`. RED: `NOT_FOUND`.
- `removes_a_members_documents_with_it`: `repo.remove { name: "infra" }`, with `docs/catervas/repos/infra/engineering/conventions.md` on `main`, records `repo.removed` then `folder_doc.removed { path: "docs/catervas/repos/infra" }`. After the next tick, `main` has no `docs/catervas/repos/infra`. A member with no documents records no `folder_doc.removed`. With an owner's uncommitted change to `…/repos/infra/engineering/conventions.md`, the answer is `INTERNAL_ERROR` with the Decisions' sentence, the member is removed, and its folder stays on `main`. RED.
- `asks_for_a_change_to_a_members_document`: `files.ask_change { path: "docs/catervas/repos/app/architecture/overview.html", words: "Add the queue." }` files a task for the Architect whose `allowed_paths` are that path alone, and `files.rewrite` with `app`'s folder holding `features.md` and no top-level Markdown document is refused `nothing_to_rewrite`. RED: `not_editable`.

- [ ] `feat(runtime): show and keep each member's documents in the project folder`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.17: member folders, their owners, the write tool, and their names; asking for a change to a member's document; "Rewrite documents as HTML" leaving member folders out
- 5.3: `folder_owned`'s member folders
- 5.14: folder changes in a project folder merged at once, whatever the policy
- 4.5: the Files page's rows with `repo`
- 8.5: `folder_doc.removed`

- [ ] `docs(spec): record documents in the project folder`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
