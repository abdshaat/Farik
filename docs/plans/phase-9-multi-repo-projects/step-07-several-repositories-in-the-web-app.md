# Phase 9, step 07: Several repositories in the web app

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 4.2, 4.4, 4.5, 8.5; F12
Depends on these steps of this phase, planned and ready, not yet executed:
- step 01: `project.create_folder`, `repos.list`, `repo.add`, `repo.remove`, `repo.locate`, their refusals' codes and sentences; their request and result defs in `rpc.schema.json` and their names in `client.ts`'s `MethodName` and `queryName`, all of which step 01 adds
- step 02: the contract's `repo`
- step 04: `files.tree`'s rows with `repo`
- step 06: `catervafication.get`'s `offered_repos`, `catervafication_start { repo }`

Nothing of 07b or 08.
Mockups: drawn 2026-10-12 on the canvas page "Multi-repo projects" (`docs/design/mockups/SetupSeveralRepos.dc.html`, `SettingsRepositories.dc.html`, `TodayRepos.dc.html`, `BoardRepos.dc.html`, `FilesRepos.dc.html`); waiting on the founder's approval (ADR 0032). This plan is reviewed for readiness after it.
Readiness confirmed by: (pending; one round, after the mockups' approval)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 36fcf33.

## Goal

The owner sets up, sees and changes a project of several repositories in the web app, as the mockups draw:
- **The first run** offers "Yes, several that work together" beside the two existing choices. The owner names the project and each repository, sees where the team's notes will live, and can change that.
- **Settings › Repositories** lists each repository with Add, Remove and Locate….
- **Today** says when a repository can't be found, and offers to document a repository that joined later.
- **The board's cards and Today's "What moved"** name each task's repository, or "notes" for the team's own.
- **Files** has a picker of the project and each repository.
- **The rail** names the project and, on hover or focus, its repositories.

A one-repo project's pages are unchanged, except that Settings › Repositories says it has one repository. Adding a second is step 07b's.

## Decisions

### The daemon's two additions

- **A task's repository on the board.** `contractSummary` (`docs/schemas/event.schema.json:280`) gains an optional `repo`, the pattern copied from `repos.schema.json`'s `$defs.repoName`, with the same drift test as step 02's copy. `summary_of` (`crates/store/src/requests.rs:989`) fills it. The projection's `TaskProjection` (`crates/store/src/projections.rs:27`) keeps the last summary's `repo` as `repo: Option<String>`, and `task_wire` (`crates/runtime/src/daemon/web.rs:1052`) answers it as `repo: string | null`. `tasksListResult` (`rpc.schema.json:820`) gains it, required and nullable. A log written before this step has no `repo` in its summaries, which reads as `null`. Rejected: reading each contract per `tasks.list`.
- **What moved.** `Moved` (`crates/store/src/activity.rs:315`) gains `task: Option<TaskId>` (the event's task, when it is about one) and `repo: Option<String>` (that task's projection's `repo`). `moved.since` (`daemon/gates.rs:295-305`) answers both, `task_id` and `repo`, each nullable, and its result schema gains them.

### The pages

- **The first run's choice** (`apps/web/src/pages/setup/SetupProject.tsx`). `Mode` (`:12`) gains `"several"`, between `"existing"` and `"new"`, labelled "Yes, several that work together" with the mockup's note. Its form, in `Choice`'s `after` slot (`packages/ui/src/Choice.tsx:5-31`), holds:
  - **The project's name:** `TextField id="several-name"`, slugged by `slug` (`:15`).
  - **The repositories:** a list of rows, each with a name `TextField` (labelled "Name for <path>"), its folder in `<code>`, and Remove. "Add a repository" opens `FolderBrowser mode="project"` (`FolderBrowser.tsx:19-24`) as the existing choice does (view `"repo"`). A picked folder becomes a row whose name is the folder's last segment, lower-cased, each run of other characters one `-`, trimmed of `-`, cut to 32 characters, and prefixed `r-` if it does not start with a letter. The owner may edit it.
  - **The notes' parent:** `"Catervas"`, shown as `~/Catervas/<name>` with "Change", which opens `FolderBrowser mode="parent"` (view `"parent"`) as the new-project choice does.
  - **Continue** is disabled until the name slugs to something, there are 2 to 16 rows, and every row's name matches step 01's `repoName` pattern, with no name twice. A hint under the list says which of these is missing, in the mockup's words.
  - **Continue sends** `take("project.create_folder", { parent, name, repos: [{ name, path }] })` through the existing `take` (`:49-71`), whose `method` type gains `"project.create_folder"`. It lands as `project.create` does.
  - **A refusal** is a sentence with no pointer (step 01's `SetupError::Refused`), shown once under the list as `role="alert"`, through `daemonSaid(e, "setupRefused")`. It is never placed on a row: the mockup was redrawn so (36fcf33).
- **Settings › Repositories.** `SECTIONS` (`apps/web/src/pages/settings/SettingsLayout.tsx:17-51`) gains `repositories` after `work`, with route `/settings/repositories` (`App.tsx:98-111`) and a new `apps/web/src/pages/settings/Repositories.tsx`, which queries `repos.list`.
  - With `repos: null` it shows the one-repo text and no button (07b adds one).
  - Otherwise it shows `Table` (`packages/ui/src/Table.tsx:4-19`): name, folder, "Finished work goes to" (the branch, or "its usual branch" when `null`), and status ("Found", or "Can't find it", bold). Each row has Remove, plus Locate… on a missing one. Under the table: "Add a repository" and "Up to 16.".
  - **Each action is a `Dialog`** (`packages/ui/src/Dialog.tsx`), mounted only while open:
    - `AddRepository`: `FolderBrowser mode="project"`, name `TextField` defaulted from the folder as above, optional branch `TextField` → `repo.add { name, path, integration_branch? }`, leaving `integration_branch` out when it is empty.
    - `RemoveRepository`: the mockup's two paragraphs → `repo.remove { name }`.
    - `LocateRepository`: `FolderBrowser mode="project"` and the mockup's warning → `repo.locate { name, path }`.
  - **After a success** the dialog closes and the table asks `repos.list` again.
  - **A refusal** is shown in the dialog through `saidAll(e)` (`apps/web/src/app/refusals.ts:177`). `WORDS` (`:9-151`) gains each of step 01's codes and step 02c's `repo_in_use`, with the mockup's sentences:
    - `repo_in_use`: "{count} of your team's tasks still work in {name}: {tasks}. Finish or cancel them first.", filled from the refusal's sentence when the code is known
    - each other code: its step 01 sentence, made a capitalised sentence with a full stop
- **Today** (`apps/web/src/pages/Today.tsx`) also queries `repos.list`. Right after `<RequestBox>` (`:293-295`) it shows:
  - **for each repository with `missing: true`:** a notice "Can't find <name>. It was at <path>. Its tasks wait until you show Catervas where it is." with Locate…, which opens `LocateRepository`
  - **for each name in `catervafication.get`'s `offeredRepos`:** a card, `apps/web/src/pages/RepositoryCatervaficationCard.tsx`, titled "<name> joined your project". It lists the active Architect, Software Developer and UI/UX Designer, first of each role in team-file order, each with the mockup's line, and has Start, sending `catervafication_start { repo }` through `useCommand`. It stays until started, as phase 8's card does, with "This stays on Today until you start it.". `Catervafication` (`CatervaficationCard.tsx:8`) gains `offeredRepos: string[]`.
- **The repository tag.** A new `RepoTag({ name })` in `packages/ui/src/RepoTag.tsx`, the mockup's outlined monospace tag. `RepoTag` with `name = null` is the dashed "notes" tag. It is shown only in a project folder (`repos.list`'s `repos` not `null`):
  - on the board, after the task's id (`Board.tsx:183`), from `TaskRow.repo` (`apps/web/src/app/lanes.ts:13-27` gains `repo: string | null`)
  - in What moved's head (`Today.tsx:543-549`), for an entry with a `taskId`, from its `repo` (`Moved`, `:114`, gains `taskId: string | null` and `repo: string | null`)
- **Files' picker** (`apps/web/src/pages/Files.tsx`). In a project folder, `FolderTree` (`:88-149`) gets, above its `ul.folders`, a group of toggle buttons named "Show" (`aria-pressed`): "Project", then each member in `files.tree`'s row order, each button a member's name.
  - "Project" lists the rows with `repo: null`; a member lists its rows.
  - The selection follows the open document: a path under `docs/catervas/repos/<name>/` selects `<name>`, any other selects "Project". Picking another clears the open document's highlight and opens nothing.
  - On a phone the same group sits above `Picker`'s `select` (`:152-203`), whose options are narrowed the same way.
  - In a one-repo project there is no picker.
  - `FilesTree`' rows (`files.ts:8-17`) carry step 04's `repo`; the breadcrumb (`FileDocument.tsx`) reads "<repo> › <folder>" for a member's document.
- **The rail's name** (`apps/web/src/shell/Shell.tsx:112-122`). In a project folder, the project's name gains an `InfoTip` (`packages/ui/src/InfoTip.tsx`) whose content is "<name>", "Repositories: <names joined ', '>" and "Notes in <projectRoot>". The phone's top bar (`:138-146`) is unchanged. `Shell` queries `repos.list` only when `projectRoot` is not null.
- **Strings** go in `apps/web/src/strings/en.ts`, one key per sentence of the mockups, filled with `t(key, fill)`.

## File map

```
docs/schemas/event.schema.json, crates/store/src/requests.rs, crates/store/src/projections.rs, crates/protocol/src/event.rs   modifies, tests: contractSummary.repo, TaskProjection.repo (Task 1)
crates/runtime/src/daemon/web.rs, docs/schemas/rpc.schema.json   modifies, tests: tasks.list's repo (Task 1); moved.since's task_id and repo (Task 1)
crates/store/src/activity.rs, crates/runtime/src/daemon/gates.rs   modifies, tests: Moved.task, Moved.repo (Task 1)
apps/web/src/pages/setup/SetupProject.tsx, setup.module.css, setup.test.tsx   modifies, tests: the several-repositories choice (Task 2)
apps/web/e2e/setup-project.spec.ts                  modifies, tests: a project folder made through the wizard (Task 2)
apps/web/src/pages/settings/{Repositories.tsx,Repositories.module.css,RepositoryDialogs.tsx}, SettingsLayout.tsx, App.tsx, settings.test.tsx   creates, modifies, tests (Task 3)
apps/web/src/app/refusals.ts, refusals.test.ts      modifies, tests: the repositories' codes (Task 3)
packages/ui/src/RepoTag.tsx, packages/ui/src/index.ts   creates (Task 4)
apps/web/src/pages/Today.tsx, RepositoryCatervaficationCard.tsx, CatervaficationCard.tsx, Today.test.tsx   modifies, creates, tests (Task 4)
apps/web/src/app/lanes.ts, apps/web/src/pages/Board.tsx, Board.test.tsx   modifies, tests (Task 4)
apps/web/src/pages/Files.tsx, files.ts, FileDocument.tsx, Files.test.tsx   modifies, tests (Task 5)
apps/web/src/shell/Shell.tsx, Shell.test.tsx        modifies, tests (Task 5)
apps/web/src/strings/en.ts                          modifies (Tasks 2–5)
docs/SPEC.md                                        modifies (Task 6)
```

## Interfaces

Consumes:
- step 01's RPCs and refusals
- step 02's `repo`
- step 04's `files.tree` rows
- step 06's `offered_repos` and `catervafication_start { repo }`
- `SetupProject`'s `take`, `slug`, `FolderBrowser`, `Choice`, `TextField`, `Dialog`, `Table`, `InfoTip`, `Button`
- `useQuery`, `useCommand` (`pages/dialogs/StartSprint.tsx:12-38`), `saidAll`, `daemonSaid`, `WORDS`, `t`
- the tests' `renderApp`, `answerQuery`, `answerStatus`, `FakeSocket`'s `reply`/`fail`/`calls`, `refusedBy`, `expectNoAxeViolations`, `media`
- e2e's `gitProject`, `startServe`, `screenshots`

Produces:

```ts
// packages/ui
export function RepoTag(props: { name: string | null }): JSX.Element;   // null: "notes"
// apps/web
type Mode = "existing" | "several" | "new";
export function repoNameOf(folder: string): string;   // the default name from a folder's last segment
export function Repositories(): JSX.Element;
export function AddRepository(props: { onDone: () => void; onClose: () => void }): JSX.Element;
export function RemoveRepository(props: { name: string; onDone: () => void; onClose: () => void }): JSX.Element;
export function LocateRepository(props: { name: string; was: string; onDone: () => void; onClose: () => void }): JSX.Element;
export function RepositoryCatervaficationCard(props: { repo: string; again: () => void }): JSX.Element;
export type Repo = { name: string; path: string; integrationBranch: string | null; missing: boolean };
// TaskRow gains repo: string | null; Moved gains taskId: string | null, repo: string | null; Catervafication gains offeredRepos: string[]
```

```rust
// catervas_store
// ContractSummary gains `repo: Option<…>`; TaskProjection gains `pub repo: Option<String>`; Moved gains `pub task: Option<TaskId>`, `pub repo: Option<String>`
```

## Tasks

### Task 1: The board and What moved know each task's repository

Files: the event schema, `summary_of`, `projections.rs`, `event.rs` (the drift test), `web.rs`'s `task_wire`, `activity.rs`, `gates.rs`'s `moved.since`, `rpc.schema.json`'s `tasksListResult` and `movedSinceResult`.

- `the_summary_carries_the_repo` (`requests.rs`): `summary_of` a contract with `repo: app` holds `repo: "app"`, and one without leaves it out. A `contract.written` with it reads, and its `repo` pattern equals `repos.schema.json`'s. RED.
- `projects_a_tasks_repo` (`projections.rs`): after `task.created` and then `contract.written` with `repo: app`, the row's `repo` is `Some("app")`. A log with no `repo` reads `None`. RED.
- `lists_each_tasks_repo` (`daemon/gates.rs`, beside the existing `tasks.list` tests): `tasks.list` answers `repo: "app"` and `repo: null`. RED.
- `says_where_each_move_happened` (`activity.rs`): the move of a task in `app` has `task: CTV-1` and `repo: "app"`; a ceremony's post has neither. `moved.since` answers `task_id` and `repo`. RED.

- [ ] `feat(store): carry each task's repository to the board and What moved`

### Task 2: The first run takes several repositories

Files: `SetupProject.tsx`; `setup.module.css`; `en.ts`; `setup.test.tsx`; the e2e spec.

- `offers_several_repositories` (`setup.test.tsx`):
  - The three radios read, in order, "Yes, one project folder", "Yes, several that work together" and "No, start a new one".
  - Picking the second shows the name field, an empty list, "Add a repository", and `~/Catervas/` with Change.
  - Continue is disabled.

  RED.
- `adds_repositories_from_the_folder_browser`:
  - "Add a repository" shows the folder browser.
  - Picking `Projects/Shop App` adds a row named `shop-app`, and `Projects/2024` one named `r-2024`.
  - Renaming the first to `app` and adding a third, then removing it, leaves two rows.
  - Continue is enabled once the project is named `shop`.

  RED.
- `sends_the_project_folder`: Continue sends `project.create_folder` with `{ parent: "Catervas", name: "shop", repos: [{ name: "app", path: "Projects/Shop App" }, { name: "r-2024", path: "Projects/2024" }], no_sandbox: false }`, which `refusedBy("projectCreateFolderRequest", ..)` accepts. Then it reconnects as `starts_a_new_project` (`:445`) does. RED.
- `refuses_in_words_under_the_list`: a refusal `-32005` "that repository has a Catervas team of its own" shows under the list as an alert, with the rows unchanged. Two rows both named `app` show the duplicate-name hint, and Continue stays disabled. RED.
- `takes_on_a_project_folder` (`e2e/setup-project.spec.ts`): with two `gitProject`s in home, the wizard's "several" choice with both added lands on `/setup/scan`, and `screenshots()` at 360 and 1280 pass. RED.

- [ ] `feat(web): set up a project of several repositories`

### Task 3: Settings › Repositories

Files: the three new files; `SettingsLayout.tsx`; `App.tsx`; `refusals.ts`; `en.ts`; the tests.

- `lists_the_repositories` (`settings.test.tsx`): `/settings/repositories` with `repos.list` answering `app` (found, `main`) and `infra` (missing, `null`) shows the table's rows as the mockup does, "its usual branch" for `infra`, and Locate… on `infra` alone. With `repos: null` it shows the one-repo sentence and no table. With `expectNoAxeViolations`, no violations. RED.
- `adds_a_repository`: Add, pick `Projects/shop-web`, keep `shop-web`, leave the branch empty: it sends `repo.add { name: "shop-web", path: "Projects/shop-web" }`, and after the reply asks `repos.list` again. With the branch `release` it sends `integration_branch: "release"`. RED.
- `refuses_removing_a_repository_with_work`: Remove `app`, then a refusal with `data.errors: [{ code: "repo_in_use", path: "/name", message: "2 of the project's tasks still work in app: CTV-12, CTV-14; finish or cancel them first" }]`, shows the `repo_in_use` words in the dialog, and the dialog stays open. RED.
- `locates_a_missing_repository`: Locate… on `infra`, pick `Projects/infra-moved`: it sends `repo.locate { name: "infra", path: "Projects/infra-moved" }`. RED.
- `words_every_repository_refusal` (`refusals.test.ts`): each of step 01's codes and `repo_in_use` has a `WORDS` entry. RED.

- [ ] `feat(web): list, add, remove and locate the project's repositories`

### Task 4: Today and the board

Files: `RepoTag.tsx`; `ui/index.ts`; `Today.tsx`; `RepositoryCatervaficationCard.tsx`; `CatervaficationCard.tsx`; `lanes.ts`; `Board.tsx`; `en.ts`; the tests.

- `says_a_repository_is_missing` (`Today.test.tsx`): with `repos.list`'s `infra` missing, Today shows the notice with its path, and Locate… opens the locate dialog. With none missing there is no notice. RED.
- `offers_to_document_a_repository_that_joined`: with `offeredRepos: ["web"]` and an active Architect, Developer and Designer, Today shows "web joined your project" with the three lines. Start sends `catervafication_start { repo: "web" }`, which `refusedBy("catervaficationStartBody", ..)` accepts, then asks `catervafication.get` again. With `offeredRepos: []` there is no card. RED.
- `tags_each_move_with_its_repository`: in a project folder a move with `repo: "app"` shows the `app` tag, a task's move with `repo: null` the "notes" tag, and a move with no `taskId` no tag. In a one-repo project no tag is shown. RED.
- `tags_each_card_with_its_repository` (`Board.test.tsx`): in a project folder `CTV-21` (`repo: "app"`) shows `app` after its id, and `CTV-18` (`null`) "notes". In a one-repo project neither does. RED.

- [ ] `feat(web): show where each task works, and the repositories to find or document`

### Task 5: Files' picker and the rail

Files: `Files.tsx`; `files.ts`; `FileDocument.tsx`; `Shell.tsx`; `en.ts`; the tests.

- `picks_the_project_or_a_repository` (`Files.test.tsx`), with a tree of the top-level rows and `app`'s:
  - The "Show" group reads "Project", "app", "infra", with "Project" pressed and the top-level folders shown.
  - Pressing `app` shows `app`'s folders alone.
  - Opening `/files/repos/app/architecture/overview.md` presses `app`, and the breadcrumb reads `app › Architecture`.
  - In a one-repo project there is no group.

  RED.
- `picks_on_a_phone`: the group sits above the `select`, whose options follow the pressed button. RED.
- `names_the_projects_repositories_on_the_rail` (`Shell.test.tsx`): in a project folder the rail's name has an `InfoTip` whose text holds "Repositories: app, infra" and "Notes in /home/me/Catervas/shop". In a one-repo project it is today's `title` alone (`names_the_project_folder_above_connected`, `:210`, unchanged). RED.

- [ ] `feat(web): pick a repository in Files, and name them on the rail`

### Task 6: Spec

`docs/SPEC.md`, at the next revision:
- 4.1: the first run's third choice and its form
- 4.4: Settings › Repositories
- 4.2: Today's missing-repository notice and the repository card; the repository tags on the board and What moved
- 4.5: Files' picker
- 8.5: `contractSummary.repo`
- the web queries' section: `tasks.list`'s `repo`, `moved.since`'s `task_id` and `repo`

- [ ] `docs(spec): record several repositories in the web app`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; includes pnpm check
# expected: xtask check: ok
pnpm --filter web e2e -- setup-project
# expected: 2 passed
```
