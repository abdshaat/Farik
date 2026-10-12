# Phase 9, step 04: Documents in the project folder

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.5, 5.3, 5.14, 5.17, 8.5; F5
Depends on steps 01 and 02 of this phase, planned and ready, not yet executed:
- from step 01: `read_repos`, `Repos`, `repo.remove` in `daemon/repos.rs` and its lock, `EVERY_KIND` at 114
- from step 02: `ReadinessContext.repos`, `TestProject::project_folder`, `Harness::project_folder`
- ADR 0057 with its amendment ("where a task works": a builder's task with no `repo` writes the project folder's documents)

Nothing of steps 02b, 02c, 03 or 05 to 08.
Mockups: none; the Files page's picker is step 07's.
Readiness confirmed by: (pending; one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at d097921.

## Goal

In a project folder, each member has its own role folders in the project folder: `docs/catervas/repos/<name>/architecture/`, `…/engineering/` and `…/design/`. Each is owned by the same role as its top-level counterpart. Every phase 8 rule knows them:
- readiness's `folder_owned`
- the folder write tool
- the Files page's tree, read and save
- each session's folders line

A folder change in a project folder, which has no remote, is merged into its `main` at once, whatever the team's `integration` policy. Removing a member removes its `repos/<name>/` folder in one folder change.

A one-repo project is unchanged: it has no `repos/` folders.

Out of scope:
- the Architect's `system.md`, `interfaces.md` and `deployment.md` and their skills (05)
- the per-member Catervafication tasks (06)
- the Files page's repository picker (07)

## Decisions

- **Member folders.** Core's `MEMBER_FOLDERS: [(Role, &str); 3] = [(Architect, "architecture"), (SoftwareDeveloper, "engineering"), (UiUxDesigner, "design")]`. A member's folder is `docs/catervas/repos/<name>/<segment>`, `member_folder(role, name)`.
  - The product, delivery and marketing folders stay project-wide. The top-level `architecture/`, `engineering/` and `design/` stay too, owned as in phase 8: in a project folder the top-level `architecture/` holds the system documents (step 05) and the plans.
  - `folder_of(path, members) -> Option<(Role, String)>` answers the folder holding a path and its owner. It tries the role folders, then each member's member folders, by segment, as `check_folder_doc_path` strips its prefix today (`crates/core/src/folders.rs:159-167`).
  - Rejected: one `repos/` folder per member owned by the Architect alone, which would take the Developer's conventions from it.
- **Readiness.** `folder_owned` (`crates/core/src/governor/readiness.rs:693`) also holds every member folder of every name in `context.repos` (step 02), owned by its role, with the same "while the role has an active agent" test and `reaches_the_folder` (`governor/paths.rs:141`). Its message names a member folder as `repos/<name>/<segment>/ (<plain role>)`. A glob `docs/catervas/repos/**` reaches them all and fails closed as today. In a one-repo project `repos` is `None` and nothing changes.
- **The folder write tool.** `check_folder_doc_path` (`folders.rs:159`) gains `members: &[&str]` and accepts a path under the caller's role folder, or under that role's member folder of a named member. `write_folder_doc` (`tools/folders.rs:401`) passes `read_repos`' names, or `&[]` in a one-repo project. `folder_doc_message` keeps the folder's last segment as its scope (`docs(architecture): …`); the paths in the message name the member.
- **Names on the Files page.** `document_name` and `document_order` (`folders.rs:253`, `:271`) read a path under `repos/<name>/` as the same path without `repos/<name>/`, so a member's `overview.md` is "Overview" in the same place. Plans stay top-level only.
- **Folder changes in a project folder merge at once.**
  - `integrate_locked`'s route (`orchestrator/integrate.rs:311-315`), for `Subject::FolderChange` in a project folder (`files.is_project_folder()`), is `merge(.., push: false)` whatever the policy.
  - `awaiting_folder_changes` (`:195`) no longer returns `None` under `manual` (`:199`) in a project folder, so the next tick merges the change.
  - `land`'s `next` sentence (`tools/folders.rs:113`) in a project folder is `next_by(Integration::AutoMerge, ..)`'s.
  - `files.tree`'s `integration` answers `auto_merge` in a project folder, since that is what a folder change there gets, so the page's words (`FileDocument.tsx:171-174`) say "adds it" truthfully.
  - A task's branch in the project folder (a docs task) still follows the team's policy (5.14). Only folder changes are forced, since nothing reviews them on a forge.
  - Rejected: pushing a project folder that has an `origin` the owner added by hand. Catervas neither pushes nor reads one (ADR 0057).
- **Removing a member's documents.** `Git::commit_removal(branch, from, folder, message, author)` makes a branch `docs/folder-<n>` from `from` with one commit that removes `folder` (`git rm -r --quiet`). It answers `CommitOutcome::Unchanged` when nothing is tracked there.
  - `tools::folders::remove_folder(deps, team, folder)` numbers and lands it as `land` does (`:133`), with the message `docs(repos): remove <name>`, and records `folder_doc.removed { path, change, sha }`, `path` the folder.
  - Step 01's `repo.remove` calls it after `repo.removed`, under the lock it already holds. A change that cannot land (another change holds a path in the folder: `Waiting`; uncommitted owner edits: `Busy`) is answered `INTERNAL_ERROR` with the refusal's words, and the member stays removed. The next tick merges it, as above.
  - The fold (`crates/store/src/folder_docs.rs:149`) reads `folder_doc.removed` as a `FolderChange { paths: [folder], .. }`, like `folder_doc.written`.
- **The folders line** (`folders.rs:443`) gains `members: &[String]`. In a project folder, for a role with member folders, it adds "Each repository has its own <segment>/ folder at repos/<name>/<segment>/, which is yours too.", naming the members. `session_spec_without` (`orchestrator/session.rs:1129-1160`) passes `read_repos`' names, or `&[]`.
- **The Files page's tree.** `files.tree`'s folder rows (`rpc.schema.json:2598`, `daemon/documents.rs:205`) gain `repo: string | null`. The top-level folders come first, as today, with `repo: null`. Then, for each member in `repos.yaml` order, each member folder whose role has an active agent, with `repo` the member's name, its `name` `plain_name(segment)` and its `agent_id` as the top-level's. `files.read` and `files.save` find a member folder's documents through `folder_of`. The web's `FilesTree` type (`apps/web/src/pages/files.ts:8`) gains `repo: string | null` and nothing reads it yet (07).
- **Events.** `folder_doc.removed { path, change, sha }`, `path` matching `folderDocPath`'s folder form `^docs/catervas/repos/[a-z][a-z0-9]*(-[a-z0-9]+)*$`, about no contract and naming no actor. `EVERY_KIND` goes to 115.

## File map

```
crates/core/src/folders.rs                         modifies, tests: MEMBER_FOLDERS, member_folder, folder_of, check_folder_doc_path's members, document_name/order, folders_line's members (Task 1)
crates/core/src/governor/readiness.rs, readiness/fixtures.rs   modifies, tests: folder_owned's member folders (Task 1)
crates/store/src/git.rs, crates/store/tests/git.rs modifies, tests: commit_removal (Task 2)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies: folder_doc.removed (Task 2)
crates/store/src/folder_docs.rs, crates/store/src/projections.rs   modifies, tests: the fold reads it (Task 2)
crates/runtime/src/tools/folders.rs                modifies, tests: write_folder_doc's members, land's next, remove_folder (Task 3)
crates/runtime/src/orchestrator/integrate.rs       modifies, tests: folder changes merged at once in a project folder (Task 3)
crates/runtime/src/prompt.rs, orchestrator/session.rs   modifies, tests: folders_line's members (Task 3)
crates/runtime/src/daemon/documents.rs, docs/schemas/rpc.schema.json, apps/web/src/pages/files.ts   modifies, tests: the tree's repo, read and save of member folders, integration (Task 4)
crates/runtime/src/daemon/repos.rs                 modifies, tests: repo.remove removes the member's folder (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 01's `read_repos`, `repo.remove` and its lock, `EVERY_KIND` at 114
- step 02's `ReadinessContext.repos` and the project-folder fixtures
- `ROLE_FOLDERS`, `role_folder`, `check_folder_doc_path`, `FolderDocPathRefusal`, `document_name`, `document_order`, `folders_line`, `plain_name` (`folders.rs`)
- `reaches_the_folder` (`paths.rs:141`)
- `land`, `next_by`, `Landed`, `LandRefusal`, `write_folder_doc` (`tools/folders.rs`)
- `Git::commit_files`, `CommitOutcome` (`git.rs:693`, `:100`)
- `folder_docs`, `FolderChange` (`folder_docs.rs:149`, `:68`)
- `Subject::FolderChange`, `integrate_locked`, `merge`, `awaiting_folder_changes`
- `documents::{folders, tree, read, save}`

Produces:

```rust
// catervas_core::folders
pub const MEMBER_FOLDERS: [(Role, &str); 3];
pub fn member_folder(role: Role, repo: &str) -> Option<String>;
pub fn folder_of(path: &str, members: &[&str]) -> Option<(Role, String)>;
pub fn check_folder_doc_path(role: Role, path: &str, members: &[&str]) -> Result<FolderDocPath, FolderDocPathRefusal>;   // was two arguments
pub fn folders_line(role: Role, active_roles: &[Role], members: &[String]) -> String;   // was two
// catervas_store::git::Git
pub fn commit_removal(&self, branch: &str, from: &str, folder: &str, message: &str, author: Option<&str>) -> Result<CommitOutcome, GitError>;
// catervas_runtime::tools::folders
pub(crate) fn remove_folder(deps: &ToolDeps, team: &Team, folder: &str, name: &str) -> Result<Option<Landed>, LandRefusal>;
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
  - `docs/catervas/architecture/system.md` is the top-level Architect's

  RED.
- `writes_only_the_callers_member_folders`: `check_folder_doc_path(Architect, "docs/catervas/repos/app/architecture/features.md", &["app"])` is `Ok`. The same path for the Developer, or with members `&[]`, is `Outside`. `…/repos/app/engineering/workflow.md` for the Developer is `Ok`. RED.
- `names_a_members_documents_as_its_own`: `document_name("docs/catervas/repos/app/architecture/overview.md", ..)` is "Overview" and its `document_order` equals the top-level `overview.md`'s. RED.
- `holds_a_members_folders_for_their_owner` (`readiness.rs`), with `repos: Some([app])` and an active Architect:
  - a Developer's task with `allowed_paths: [docs/catervas/repos/app/architecture/**]` fails `FolderOwned` naming `repos/app/architecture/ (Architect)`
  - `[docs/catervas/repos/**]` fails it too
  - `[docs/catervas/repos/app/engineering/**]` passes
  - with `repos: None` the first passes

  RED.
- `tells_a_role_its_member_folders`: `folders_line(Architect, [..], [app, infra])` ends with the Decisions' sentence naming `repos/app/architecture/` and `repos/infra/architecture/`; with `[]` it is today's line exactly. RED.

- [ ] `feat(core): give each member its own role folders`

### Task 2: A folder change that removes a folder

Files: `git.rs` (`commit_removal`); `tests/git.rs`; the event's files; `folder_docs.rs`; `projections.rs` (pass arm).

- `commits_a_folders_removal_on_its_own_branch` (`tests/git.rs`): with `docs/catervas/repos/app/architecture/overview.md` on `main`, `commit_removal("docs/folder-3", "main", "docs/catervas/repos/app", ..)` makes `docs/folder-3` one commit past `main` without the folder, leaves `main` and the checkout as they were, and answers `Committed`. A folder with nothing tracked answers `Unchanged` and makes no branch. RED.
- `reads_a_folder_doc_removed_event`: `{ path: "docs/catervas/repos/app", change: 3, sha: "a1" }` reads and writes back unchanged. `path: "docs/catervas/product"` is refused at `/body/path`. It needs no task id. RED.
- `folds_a_removal_as_a_change` (`folder_docs.rs`): it gives `FolderChange { change: 3, paths: [docs/catervas/repos/app], approved: false, integrated: false, escalated: false, edited: false }`, and integrated once `folder_change.integrated { change: 3 }` follows. RED.

- [ ] `feat(store): remove a folder in a folder change`

### Task 3: The project folder's folder changes land at once

Files: `tools/folders.rs` (`write_folder_doc`'s members, `land`'s sentence, `remove_folder`); `integrate.rs` (the route, `awaiting_folder_changes`); `prompt.rs` and `orchestrator/session.rs` (the folders line's members). Tests are integration tests on `Harness::project_folder("x", &["app"])`.

- `writes_a_members_document_in_a_ceremony` (`tools/folders.rs`): the Architect's `write_folder_doc` at sprint review of `docs/catervas/repos/app/architecture/features.md` lands `docs/folder-1` and records `folder_doc.written`. Its answer's `next` is `auto_merge`'s sentence though the team's policy is `pull_request`. RED: `Outside`.
- `merges_a_folder_change_at_once_in_a_project_folder` (`integrate.rs`): under `manual`, and again under `pull_request`, the next tick merges `docs/folder-1` into the project folder's `main` with no push and no `gh` call, and records `folder_change.integrated`. In a one-repo `Harness` under `manual` the tick leaves it waiting, as today. RED.
- `a_sessions_folders_line_names_its_member_folders` (`orchestrator/session.rs`): the Architect's planning prompt in the project folder holds the Decisions' sentence naming `repos/app/architecture/`. RED.

- [ ] `feat(runtime): land a project folder's document changes at once`

### Task 4: The Files page sees each member's folders

Files: `daemon/documents.rs` (`folders`, `tree`, `read`, `save`); `rpc.schema.json` (`filesTreeResult`'s row gains required, nullable `repo`); `apps/web/src/pages/files.ts`; `daemon/repos.rs` (`repo.remove` calls `remove_folder`). Tests are as phase 8 step 04's, on `TestProject::project_folder` with members `app` and `infra` and an active Architect and Developer.

- `lists_each_members_folders_after_the_projects`: with `docs/catervas/repos/app/architecture/overview.md` on `main`, `files.tree` lists the top-level folders with `repo: null`, then `app`'s `architecture` (`name: "Architecture"`, `repo: "app"`, the document "Overview") and `engineering` (no documents), then `infra`'s two. `integration` is `auto_merge` under the team's `manual`. In a one-repo project no row has a `repo` but `null`. RED.
- `reads_and_saves_a_members_document`: `files.read` of `app`'s `overview.md` answers it as a top-level document is answered. `files.save` of it lands a folder change that the next tick merges into `main`. RED: `NOT_FOUND`.
- `removes_a_members_documents_with_it`: `repo.remove { name: "infra" }`, with `docs/catervas/repos/infra/engineering/conventions.md` on `main`, records `repo.removed` then `folder_doc.removed { path: "docs/catervas/repos/infra" }`. After the next tick, `main` has no `docs/catervas/repos/infra`. A member with no documents records no `folder_doc.removed`. RED.

- [ ] `feat(runtime): show and keep each member's documents in the project folder`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.17: member folders, their owners, the write tool, and their names
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
