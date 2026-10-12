# Phase 9, step 06: Catervafication across repositories

Status: ready (readiness review 2026-10-12, two rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 5.16, 5.17, 8.5; F5
Depends on these steps of this phase, planned, not yet executed:
- step 01: `read_repos`, `repo_present`, `repo.add`
- step 02: "a builder's task with no `repo` keeps to the document paths", `RepoRequired`
- step 03: read roots, so a project-folder task reads its members
- step 02: `Harness::project_folder_with`
- step 04: member folders, `member_folder`, `folder_owned`'s member folders
- step 05: `mapping-the-system`, the three system documents

Nothing of steps 02b, 02c, 07 or 08.
Mockups: none; the offer's screens are step 07's.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on B1 (the epic's budget), B2 (a project-wide start against a member's own, and a member missing at Start) and B3 (the system row's dependencies), all decided below; S1–S8 and N1–N7 folded, the plan rewritten. Round 2 (the same reviewer, confirming): ready; its nits N1–N7 folded. Step 07b changes two of its decisions after a move, in 07b's Task 4: `covered_repos` counts a `Repository`-scope start as covering `project.converted`'s `first_repo`, and a member's own Catervafication files the project-wide system row when no `system.md` is on the integration branch.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at a66f5d3.

## Goal

A project folder's Catervafication documents every member present at its Start. The owner's Start (phase 8's `catervafication_start`) files the epic "Catervafy the project", and its ticks file:
- the Product Manager's product plan for the whole project
- for each covered member: the Architect's, the Developer's and the UI/UX Designer's documents in `docs/catervas/repos/<name>/`
- once every member's architecture task is done, the Architect's system map, interfaces and deployment view
- the Marketing Specialist's plan, after the product plan, as in phase 8

A member not covered (added after the Start, or missing at it) is offered its own Catervafication. `catervafication_start { repo }` files "Catervafy <name>", with that member's three tasks and then an Architect's task to bring the system documents up to date.

A one-repo project's Catervafication is unchanged.

Out of scope: the offer's screens (07).

## Decisions

### What a Catervafication covers

- **Its scope.** `catervafication.started` (`event.schema.json:1847`) gains two optional properties, which never appear together:
  - `repos`, 1 to 16 names in `repoName`'s pattern, copied: a project-wide start in a project folder, listing the members present at the Start
  - `repo`, one name: a member's own start, as the design has it (`multi-repo-projects.md:104`)

  `catervafication_scope(history) -> Option<Scope>` replaces `catervafication_of` (`:294`) at its callers (`orchestrator/requests.rs:306`, `:556`), with `Scope { Repository(Project), Project(Vec<String>), Member(String) }`. A one-repo project's epic is `Repository(Existing | New)`, as today. The started body's `project` stays `existing` for the other two. Its test `knows_a_catervafication_from_its_history` becomes `knows_a_catervafications_scope`.
- **A member missing at the Start** is not covered: the project-wide `repos` lists the present members only. Once found, it is offered alone (below), never filed into the project-wide epic. A covered member that goes missing after the Start has its rows filed when it is found again, while the epic is open. A project-wide start with no member present is refused, after `catervafication_new_project` and before `catervafication_no_product_manager` in `human.rs:405-427`'s order, with `catervafication_no_repositories`: "None of your project's repositories can be found; locate them in Settings, then start."
- **The store carries the scope.** `file_catervafication_epic` and `Filing` (`crates/store/src/requests.rs:462-497`, `:688`) take a `CatervaficationScope { project, repos: Option<&[String]>, repo: Option<&str> }`, a `Copy` view, in place of `CatervaficationStartedBodyProject`, and write the started body from it.

### The rows

- **The rows of a project-wide start**, filed by `file_due` (`:484`) in this order:
  1. The Product Manager's row, unchanged (`:102`). It reads every member through its read roots (step 03).
  2. For each covered member that is present, in `repos.yaml` order, three rows. Each writes in `docs/catervas/repos/<name>/<segment>/`, and its `allowed_paths` is `[docs/catervas/repos/<name>/<segment>/**]`. None names a `repo`: they are document tasks in the project folder (step 02), within the default document paths `docs/**` (`team_rules.rs:19`).
     - **Architect**, "Document <name>'s architecture": `overview.md`, `features.md`, `jobs.md`, `structure.md`.
     - **Software Developer**, "Document how <name> is built": `conventions.md`, `workflow.md`, with phase 8's workflow question.
     - **UI/UX Designer**, "List <name>'s screens": `ui-inventory.md`, filed only when the Designer's browser is ready, as phase 8 does. Its skill says a member with no screens gets `# UI inventory` and the line "This repository has no screens.", which meets its `# ` criterion.
  3. The Architect's system row, "Map how the repositories connect", whose `allowed_paths` are exactly `docs/catervas/architecture/system.md`, `interfaces.md` and `deployment.md`. It is not filed with the others; it waits (below).
  4. The Marketing Specialist's row, unchanged: it waits for the product plan.
- **The rows of a member's own start:** that member's three rows, then the Architect's system row titled "Bring the system map up to date with <name>". It has the same paths, and it waits on that member's architecture task alone. There is no Product Manager and no Marketing row.
- **The system row waits as the Marketing row waits** (`catervafication.rs:506-521`).
  - It is filed once every covered member's architecture task under the epic is filed and is either cancelled, or accepted and not awaiting integration.
  - Its `dependencies` are the accepted ones; with none accepted it is not filed.
  - While it waits, `file_due` answers `FileDue::Waits` (renamed from `MarketingWaits`, which also covers the Marketing row), so `in_progress_epic` (`orchestrator/requests.rs:570`) holds off the epic's close-out as it does today.
  - It is filed whole with its dependencies already accepted, so it is judged as any Catervafication row is, with no refine session.
  - A team with no Architect has no system row, and nothing waits for it.
  - A covered member gone for good (moved and never located) keeps its rows unfiled, so the system row waits for ever and the epic stays open. The owner cancels the epic, or removes the member (step 01's `repo.remove`). A removed member no longer counts as covered for `file_due` or the system row's wait.
- **The rows' criteria.** As phase 8's: one artifact criterion per document with `must_contain: ["# "]`. The member's `overview.md` keeps `` ```mermaid `` (`catervafication.rs:252-254`, whose `ends_with` already matches). `system.md` must also contain `## Repositories` and `` ```mermaid ``, and `deployment.md` `` ```mermaid ``. Each row's review asks phase 8's two questions. The system row adds "Does system.md name every repository the project has, and does every link name the file it was read from?".
- **A row's place.** `task_contract` (`:219`) gains `place: &Place`, with `Place { Project, Member(String), System }`:
  - `Place::Project` is today's folder
  - `Place::Member(name)` is `member_folder(role, name)` (step 04)
  - `Place::System` is the three documents
- **The epic's contract.** `epic_contract(max_cost_usd, scope: &Scope)` (`:169`), the `project` parameter dropped since `Scope::Repository` carries it. Its budget is `max_cost_usd` times the rows its scope can file, so that `budget_within_parent` (`readiness.rs:985-998`) lets every row through:
  - `Repository`: today's 5 or 3
  - `Project(repos)`: 3 + 3 × `repos.len()`
  - `Member(_)`: 4

  Its title is "Catervafy the project" for `Project`, and "Catervafy <name>" for `Member`. Its intent and summary say "each repository" in place of "the project".
- **Which rows are filed already.** `file_due`'s key is a row's `assignee_role` and its title as filed, read from the child's `task.created` summary. The log never rewrites that title, while `allowed_paths` is a content field the owner or the Product Manager may rewrite. So a task the owner cancelled is not filed again, and a one-repo project's rows key as today's, one title per role.

### Starting a member's own

- **The word.** `catervafication_start`'s body becomes `emptyBody` or a new `catervaficationStartBody { repo }` with `required: ["repo"]`, so no body matches two branches of `commandBodyWire`'s `oneOf` (`command.schema.json:228-231`, as `sprintStartBody` does). The reader (`command.rs:579`, `:821-829`, `:970`) accepts either. `Command::CatervaficationStart { repo: Option<String> }` is updated at its call sites: `human.rs:168`, `:3178-3352`; `orchestrator/requests.rs:2137`, `:2274`; `daemon/gates.rs:4582`.
- **Its refusals, in order:**
  - `repo_not_allowed`: "This project is one repository." (a one-repo project). It is a command's refusal, a different one from step 02's readiness rule of that name, and SPEC 4.1 says so.
  - `repo_unknown`: not a member
  - `repo_missing`: "<name> cannot be found; locate it in Settings, then start." (the member is missing)
  - `catervafication_not_started`: "Start your project's Catervafication first; it covers every repository it can find then."
  - `catervafication_started`: "Your team is already documenting <name>." (a started body's `repos` or `repo` names it)
  - `catervafication_no_product_manager`, as today
  - `catervafication_no_writers`: no Architect, Developer or Designer row can be staffed with a reviewer
- **Where its epic goes.** `start` (`:397`) plans the epic into a new sprint only when none is open (`:420-426`). A member's epic, which usually meets an open sprint, waits in the Backlog for the next sprint's planning, as any epic does under sprints, and is approved by the Start as today.
- **The offer.** `catervafication.get` (`daemon/gates.rs:336`) gains a required `offered_repos: [names]` (`rpc.schema.json:3636`, its description updated): the present members that no started body covers, once a project-wide Catervafication has started; `[]` before it, and in a one-repo project. `offered` and `roles` are unchanged.
- **The retro's cadence.** `started()` (`:377`) still answers the oldest epic, so the Scrum Master's cadence at the retro (`rules.rs:517`) follows the project-wide sprint alone. This is intended: the cadence is the project's, not a member's.

### The skills

Each of the Architect's, the Developer's and the Designer's `catervafying-the-repository/SKILL.md` gains `## In a project of several repositories`:
- your task names the repository it documents in its title and paths; read it at its path in the repositories line; write only `docs/catervas/repos/<name>/<your folder>/`
- the Architect's adds: the system task follows `mapping-the-system`
- the Designer's adds: a repository with no screens gets `# UI inventory` and "This repository has no screens."

## File map

```
crates/runtime/src/catervafication.rs              modifies, tests: Scope, Place, the rows, epic_contract, file_due's key, waits and scope, start's scope (Tasks 1–3)
crates/store/src/requests.rs                       modifies: CatervaficationScope in file_catervafication_epic and Filing; its tests' calls at :1247, :1306, :1333 (Task 1)
crates/runtime/src/orchestrator/rules.rs           modifies: `catervafication::start` at :333 with `repo: None` (Task 3)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,event/fixtures.rs}   modifies, tests: catervafication.started's repos and repo (Task 1)
crates/runtime/src/orchestrator/requests.rs        modifies, tests: catervafication_scope at :306, :556; FileDue::Waits at :570 (Task 2); `epic_contract`'s test call at :2100 (Task 1)
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies, tests: catervaficationStartBody (Task 3)
crates/runtime/src/orchestrator/human.rs           modifies, tests: catervafication_start_for with a repo (:397-445), the call sites (Task 3)
crates/runtime/src/daemon/gates.rs, docs/schemas/rpc.schema.json   modifies, tests: offered_repos; the call site at :4582 (Task 3)
crates/roles/roles/{architect,software_developer,ui_ux_designer}/skills/catervafying-the-repository/SKILL.md, crates/roles/src/kit.rs   modifies, tests (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 01's `read_repos` and `repo_present`
- step 04's `member_folder`, step 02's `Harness::project_folder_with`
- step 05's system documents
- `catervafication_roles`, `row`, `task_contract`, `epic_contract`, `start`, `file_due`, `FileDue`, `catervafication_of`, `started`
- `file_catervafication_epic`, `Filing`
- `catervafication_start_for`, `catervafication_get`
- `file_request`, `join_epics_sprint`
- `a_catervafication_in_progress` (`orchestrator/requests.rs:2132`)

Produces:

```rust
// catervas_runtime::catervafication
pub(crate) enum Scope { Repository(Project), Project(Vec<String>), Member(String) }
pub(crate) enum Place { Project, Member(String), System }
pub(crate) fn epic_contract(max_cost_usd: f64, scope: &Scope) -> Value;
pub(crate) fn task_contract(project: Project, role: Role, reviewer: Role, epic: &TaskId, max_cost_usd: f64, place: &Place) -> Value;
pub(crate) fn catervafication_scope(history: &[CatervasEvent]) -> Option<Scope>;
pub(crate) fn covered_repos(log: &EventLog) -> Result<BTreeSet<String>, StoreError>;
pub(crate) fn start(deps: &ToolDeps, team: &Team, project: Project, by: &str, note: Option<String>, repo: Option<&str>) -> Result<Started, CatervaficationError>;
pub(crate) enum FileDue { Filed(Vec<TaskId>), Waits, Nothing }   // MarketingWaits renamed
// catervas_store::requests
#[derive(Clone, Copy)] pub struct CatervaficationScope<'a> { pub project: CatervaficationStartedBodyProject, pub repos: Option<&'a [String]>, pub repo: Option<&'a str> }
// event: catervafication.started gains optional `repos: [name]` or `repo: name`, never both
// command: Command::CatervaficationStart { repo: Option<String> }
// rpc: catervafication.get's result gains required `offered_repos: [string]`
```

## Tasks

### Task 1: The scope and the rows

Files: `catervafication.rs` (`Scope`, `Place`, `row`, `task_contract`, `epic_contract`, `catervafication_scope`); `crates/store/src/requests.rs` (`CatervaficationScope`); the event's files.

- `writes_a_members_rows`: `task_contract(Existing, Architect, ScrumMaster, CTV-1, 1.0, &Member("app"))` has the title "Document app's architecture", `allowed_paths: [docs/catervas/repos/app/architecture/**]`, no `repo`, and artifact criteria for its four documents, `overview.md`'s with `` ```mermaid ``. The Developer's and the Designer's are likewise, each in its own folder. `&Place::Project` gives today's contract exactly. RED.
- `writes_the_system_row`: the system row's `allowed_paths` are the three paths, `system.md`'s criterion has `## Repositories` and `` ```mermaid ``, and the review has the added question. RED.
- `sizes_the_epic_to_its_rows`: `epic_contract`'s budget is `max_cost_usd` times 5 for `Repository(Existing)`, 3 for `Repository(New)`, 9 for `Project([app, infra])` and 4 for `Member("web")`. The titles are "Catervafy the repository", "Catervafy the project" and "Catervafy web". `every_template_passes_the_definition_of_ready` also passes each member row and the system row in a project-folder context, its parent's budget reduced by every sibling of a two-member project-wide epic. RED.
- `reads_a_catervafication_started_with_its_scope`: `{ project: existing, repos: [app, infra] }` and `{ project: existing, repo: web }` read and write back unchanged. `{ repos: [] }`, `{ repos: ["App"] }`, and a body with both `repos` and `repo` are refused at `/body`. `catervafication_scope` reads each as its `Scope`, and a body with neither as `Repository`. RED.

- [ ] `feat(runtime): write a Catervafication's rows for each repository`

### Task 2: Filing them

Files: `catervafication.rs` (`file_due`: the key, the scope, the system row's wait, `FileDue::Waits`); `orchestrator/requests.rs`. Tests are integration tests on `Harness::project_folder_with("x", &["app", "infra"], with_the_suggested_six)`, the Designer's browser ready, the epic moved into progress with `a_catervafication_in_progress` (`orchestrator/requests.rs:2132`).

- `files_every_members_rows`:
  - The owner's Start records `catervafication.started { repos: [app, infra] }`.
  - The first filing tick files, in order: the Product Manager's task, then `app`'s Architect, Developer and Designer tasks, then `infra`'s. No system row is filed. `file_due` answers `Filed`.
  - With both Architect tasks accepted and integrated, the next tick files the system row, with `dependencies` the two.
  - A tick after that files nothing new.
  - The owner rewrites `app`'s Developer task's `allowed_paths` with `contract.save` while it is in `refining` (`gates.rs:1108-1140`), then cancels it: it is not filed again.

  RED: one task per role.
- `waits_for_the_system_row`: with `app`'s Architect task accepted and `infra`'s in progress, `file_due` answers `Waits` and the epic is not closed. With `infra`'s cancelled, the system row is filed depending on `app`'s alone. With both cancelled, it is never filed, and `file_due` answers `Nothing`. RED.
- `covers_only_the_members_present_at_the_start`: with `infra` moved away at the Start, it records `repos: [app]` and files `app`'s rows only. Once `infra` is back, no `infra` rows are filed under the epic, and `catervafication.get`'s `offered_repos` is `[infra]`. A covered `app` moved away after the Start has its rows filed once it is back. With both moved away, the Start is refused `catervafication_no_repositories`. RED.
- `keeps_a_one_repo_catervafication_as_it_is`: a guard, in a one-repo `Harness`: the Start records no `repos` and the ticks file phase 8's rows exactly. It fails if `Place::Project`, the key or the budget changes a one-repo contract.

- [ ] `feat(runtime): file a Catervafication's tasks for every repository`

### Task 3: A member's own

Files: `command.schema.json` and `command.rs`; `human.rs`; `catervafication.rs` (`start`'s `repo`, `covered_repos`); `daemon/gates.rs` and `rpc.schema.json`. There is no command-line spelling of this word, and this step adds none.

- `reads_catervafication_start` (`command.rs:1774`) also reads `{ repo: "web" }` as `CatervaficationStart { repo: Some("web") }` and refuses `{ repo: "Web" }`. `{}` still reads as `repo: None`. RED.
- `offers_a_member_added_later` (`daemon/gates.rs`): before any Start, `offered_repos` is `[]`. After the project-wide Start it is `[]`. After `repo.add` of `web`, it is `[web]`. With `web` moved away, `[]`. In a one-repo project it is always `[]`. RED.
- `catervafies_a_member_alone` (`human.rs`): `catervafication_start { repo: "web" }` records `catervafication.started { repo: web }` and the owner's approval, and files "Catervafy web" into the Backlog while a sprint is open. Once it is in progress (`a_catervafication_in_progress`), the next tick files `web`'s three rows. Once `web`'s Architect task is accepted and integrated, the next tick files "Bring the system map up to date with web". No Product Manager and no Marketing task is filed. RED.
- `refuses_a_members_catervafication_in_order` (`human.rs`): in a one-repo project, `repo_not_allowed`; `repo: "nope"`, `repo_unknown`; `web` moved away, `repo_missing`; `web` before the project-wide Start, `catervafication_not_started`; a second `repo: "web"`, `catervafication_started` naming `web`; with no Architect, Developer or Designer active, `catervafication_no_writers`. RED.

- [ ] `feat(runtime): document a repository the project gains later`

### Task 4: The skills

Files: the three `SKILL.md`; `kit.rs` (the tests).

- `the_catervafying_skills_name_the_repositorys_folder` (`kit.rs`, beside `every_owner_carries_catervafying_the_repository`, `:2300`): the Architect's, the Developer's and the Designer's each hold `## In a project of several repositories` and `docs/catervas/repos/`. The Architect's holds `mapping-the-system`. The Designer's holds `# UI inventory` and "This repository has no screens.". Each stays under 6 KiB with no ` @`. RED.

- [ ] `feat(roles): document each repository in its own folder`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 4.1 and 5.16: the project-wide Catervafication's scope, rows, order and budget; the system row's wait; a member missing at the Start
- 5.17: the per-member documents and the system documents' first writing
- 4.1: `offered_repos`, `catervafication_start { repo }` and its refusals, a member's epic in the Backlog
- 8.5: `catervafication.started`'s `repos` and `repo`

`docs/design/multi-repo-projects.md`: the Catervafication section says the system row waits for the members' architecture tasks, and that a member missing at the Start is offered alone.

- [ ] `docs(spec): record the Catervafication across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
