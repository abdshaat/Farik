# Phase 9, step 06: Catervafication across repositories

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.1, 5.16, 5.17, 8.5; F5
Depends on these steps of this phase, planned, not yet executed:
- step 01: `read_repos`, `repo_present`, `repo.add`
- step 02: `RepoRequired` and "a builder's task with no `repo` keeps to the document paths", the fixtures
- step 03: read roots, so a project-folder task reads its members
- step 04: member folders, `member_folder`, `folder_owned`'s member folders
- step 05: `mapping-the-system`, the three system documents, `DOCUMENT_NAMES`

Nothing of steps 02b, 02c, 07 or 08.
Mockups: none; the offer's screens are step 07's.
Readiness confirmed by: (pending; one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 09bbc12.

## Goal

A project folder's Catervafication documents every member. The owner's Start (phase 8's `catervafication_start`) files one epic with:
- the Product Manager's product plan for the whole project
- for each member: the Architect's, the Developer's and the UI/UX Designer's documents in `docs/catervas/repos/<name>/`
- the Architect's system map, interfaces and deployment view, which depends on every member's architecture task
- the Marketing Specialist's plan, after the product plan, as in phase 8

A member added later is offered its own Catervafication. `catervafication_start { repo }` files an epic for that member alone, with its own three tasks and an Architect's task to bring the system documents up to date.

A one-repo project's Catervafication is unchanged.

Out of scope: the offer's screens (07).

## Decisions

- **The tasks of a project folder's Catervafication** (`Project::Existing` in a project folder; a project folder never records `project.started`, step 01). They are filed in this order:
  1. The Product Manager's row, unchanged (`catervafication.rs:102`). It reads every member through its read roots (step 03).
  2. For each member in `repos.yaml` order that is present, three rows. Each lists what it writes in `docs/catervas/repos/<name>/<segment>/`, and its `allowed_paths` is `[docs/catervas/repos/<name>/<segment>/**]`. None names a `repo`: they are document tasks in the project folder (step 02).
     - **Architect**, "Document <name>'s architecture": `overview.md`, `features.md`, `jobs.md`, `structure.md`.
     - **Software Developer**, "Document how <name> is built": `conventions.md`, `workflow.md`, with phase 8's workflow question.
     - **UI/UX Designer**, "List <name>'s screens": `ui-inventory.md`. It is filed only when the Designer's browser is ready, as phase 8 does. Its skill says a member with no screens gets the one line "This repository has no screens.".
  3. The Architect's system row, "Map how the repositories connect": `docs/catervas/architecture/system.md`, `interfaces.md` and `deployment.md` as its exact `allowed_paths`. Its `dependencies` are every member's architecture task under the epic. It is filed only once every member is present and every member's architecture task is filed.
  4. The Marketing Specialist's row, unchanged: it waits for the product plan.

  A member that is missing has none of its rows filed. `file_due` files them on a later tick once it is found, while the epic is open.
- **The rows' criteria.** As phase 8's: one artifact criterion per document with `must_contain: ["# "]`. The member's `overview.md` keeps `` ```mermaid `` (`catervafication.rs:252-254`, whose `ends_with` already matches). `system.md` must also contain `## Repositories` and `` ```mermaid ``, and `deployment.md` `` ```mermaid ``. Each row's review asks phase 8's two questions. The system row adds "Does system.md name every repository the project has, and does every link name the file it was read from?".
- **Which tasks are already filed.** `file_due` (`catervafication.rs:484-503`) keys a row by its `assignee_role` and its `allowed_paths`, not its role alone, since one role now has a row per member. A held task under the epic whose contract has the same two is that row, in any status: a task the owner cancelled is not filed again, as today. A one-repo project's rows have one path each, so their keys behave as today's.
- **A row's place.** `task_contract` (`:219`) gains `place: Place`:
  - `Place::Project` is today's folder
  - `Place::Member(name)` is `member_folder(role, name)` (step 04)
  - `Place::System` is the three documents

  `row()` (`:102`) gains `Place`'s titles and the system row. `catervafication_roles` (`:62`) is unchanged: it answers which roles get a task, for `catervafication.get`'s `roles`.
- **What a Catervafication covers.** `catervafication.started` (`event.schema.json:1847`) gains an optional `repos`, 1 to 16 names in `repoName`'s pattern, copied: the members it documents.
  - The project-wide start in a project folder records every present member.
  - A member's own start records `[<name>]`.
  - A one-repo project's records none.

  `catervafication_scope(history) -> Option<(Project, Option<Vec<String>>)>` replaces `catervafication_of` (`:294`) at its callers (`orchestrator/requests.rs:306`, `:556`), and `file_due` files only the covered members' rows.
- **A member's own Catervafication.** The owner's word `catervafication_start` gains an optional `repo` (`command.schema.json`), refused in order with:
  - `repo_unknown`: not a member
  - `catervafication_not_started`: "Start your project's Catervafication first; it covers every repository it has then.", when no project-wide one has started
  - `catervafication_started`: "Your team is already documenting <name>.", when a started body's `repos` names it
  - `catervafication_no_product_manager`, as today
  - `catervafication_no_writers`: no Architect, Developer or Designer row can be staffed with a reviewer

  It files the epic "Catervafy <name>" with `allowed_paths: [docs/catervas/**]`, its three member rows, and an Architect's system row titled "Bring the system map up to date with <name>", depending on `<name>`'s architecture task. There is no Product Manager row and no Marketing row. It records `catervafication.started { project: existing, repos: [<name>] }`. Sprint and approval follow `start` (`:397`) unchanged.
- **The offer.** `catervafication.get` (`daemon/gates.rs:336`) gains `offered_repos: [names]`: the present members that no started body covers, once a project-wide Catervafication has started; `[]` before it, and in a one-repo project. `offered` and `roles` are unchanged. A member added after the project-wide start is thus offered alone. Nothing starts by itself: the owner's Start approves each one (phase 8).
- **The skills.** Each of the Architect's, the Developer's and the Designer's `catervafying-the-repository/SKILL.md` gains `## In a project of several repositories`:
  - your task names the repository it documents in its title and paths; read it at its path in the repositories line; write only `docs/catervas/repos/<name>/<your folder>/`
  - the Architect's adds: the system task follows `mapping-the-system`
  - the Designer's adds: a repository with no screens gets the one line

  `every_owner_carries_catervafying_the_repository` (`kit.rs:2300`) keeps its bounds (under 6 KiB).

## File map

```
crates/runtime/src/catervafication.rs              modifies, tests: Place, the rows, file_due's key and scope, catervafication_scope, start with a repo (Tasks 1–3)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,event/fixtures.rs}   modifies: catervafication.started's repos (Task 1)
crates/runtime/src/orchestrator/requests.rs        modifies, tests: catervafication_scope at :306, :556 (Task 2)
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies, tests: catervafication_start's body gains an optional repo; it leaves `empty_command` (`command.rs:579`) for a body of its own, `Command::CatervaficationStart { repo: Option<String> }` (`:829`, `:970`), `reads_catervafication_start` (`:1774`) also reading `{ repo: "web" }` and refusing `{ repo: "Web" }` (Task 3)
crates/runtime/src/orchestrator/human.rs           modifies, tests: catervafication_start_for with a repo (:397-445) (Task 3)
crates/runtime/src/daemon/gates.rs, docs/schemas/rpc.schema.json   modifies, tests: offered_repos (Task 3)
crates/roles/roles/{architect,software_developer,ui_ux_designer}/skills/catervafying-the-repository/SKILL.md, crates/roles/src/kit.rs   modifies, tests (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 01's `read_repos` and `repo_present`
- step 04's `member_folder`
- step 05's system documents
- `catervafication_roles`, `row`, `task_contract`, `epic_contract`, `start`, `file_due`, `FileDue`, `catervafication_of`, `started`
- `catervafication_start_for` (`human.rs:397`), `catervafication_get`
- `file_request`, `join_epics_sprint`
- `Harness::project_folder`

Produces:

```rust
// catervas_runtime::catervafication
pub(crate) enum Place { Project, Member(String), System }
pub(crate) fn task_contract(project: Project, role: Role, reviewer: Role, epic: &TaskId, max_cost_usd: f64, place: &Place) -> Value;   // was five arguments
pub(crate) fn catervafication_scope(history: &[CatervasEvent]) -> Option<(Project, Option<Vec<String>>)>;
pub(crate) fn covered_repos(log: &EventLog) -> Result<BTreeSet<String>, StoreError>;
pub(crate) fn start(deps: &ToolDeps, team: &Team, project: Project, by: &str, note: Option<String>, repo: Option<&str>) -> Result<Started, CatervaficationError>;   // was five
// event: catervafication.started gains optional `repos: [name]`
// command: catervafication_start gains optional `repo`: Command::CatervaficationStart { repo: Option<String> }
// rpc: catervafication.get's result gains `offered_repos: [string]`
```

## Tasks

### Task 1: The rows of a project folder's Catervafication

Files: `catervafication.rs` (`Place`, `row`, `task_contract`, the system row's criteria); the event's `repos` (schema, `event.rs`, fixtures).

- `writes_a_members_rows`: `task_contract(Existing, Architect, ScrumMaster, CTV-1, 1.0, &Member("app"))` has the title "Document app's architecture", `allowed_paths: [docs/catervas/repos/app/architecture/**]`, no `repo`, and artifact criteria for its four documents, `overview.md`'s with `` ```mermaid ``. The Developer's and the Designer's are likewise, each in its own folder. `&Place::Project` gives today's contract exactly. RED.
- `writes_the_system_row`: `task_contract(Existing, Architect, .., &System)` has `allowed_paths` the three paths, `system.md`'s criterion with `## Repositories` and `` ```mermaid ``, and the review's added question. RED.
- `reads_a_catervafication_started_with_repos`: `{ project: existing, repos: [app, infra] }` reads and writes back unchanged. `repos: []` and `repos: ["App"]` are refused at `/body/repos`. RED.

- [ ] `feat(runtime): write a Catervafication's rows for each repository`

### Task 2: Filing them

Files: `catervafication.rs` (`file_due`, `start`'s `repos`, `catervafication_scope`); `orchestrator/requests.rs`. Tests are integration tests on `Harness::project_folder("x", &["app", "infra"])` with the suggested six and the Designer's browser ready.

- `files_every_members_rows`: the owner's Start records `catervafication.started { repos: [app, infra] }`. The first tick files, in order, the Product Manager's task, then `app`'s Architect, Developer and Designer tasks, then `infra`'s, then the system task with `dependencies` `app`'s and `infra`'s Architect tasks. The second tick files nothing new. A cancelled `app` Developer task is not filed again. RED: one task per role.
- `waits_for_a_missing_members_rows`: with `infra` moved away at the Start, the tick files `app`'s rows and no `infra` rows and no system task. Once `infra` is back, the next tick files `infra`'s three and then the system task. RED.
- `keeps_a_one_repo_catervafication_as_it_is`: in a one-repo `Harness`, the Start records no `repos` and files phase 8's rows exactly. RED: guards the one-repo path; it fails if `Place::Project` changes a contract.

- [ ] `feat(runtime): file a Catervafication's tasks for every repository`

### Task 3: A member added later

Files: `command.schema.json` and `command.rs` (`repo`); `human.rs` (`catervafication_start_for`); `catervafication.rs` (`start`'s `repo`, `covered_repos`); `daemon/gates.rs` and `rpc.schema.json` (`offered_repos`). There is no command-line spelling of this word today, and this step adds none.

- `offers_a_member_added_later` (`daemon/gates.rs`): before any Start, `offered_repos` is `[]`. After the project-wide Start it is `[]`. After `repo.add` of `web`, it is `[web]`. With `web` moved away, `[]`. In a one-repo project it is always `[]`. RED.
- `catervafies_a_member_alone` (`human.rs`): `catervafication_start { repo: "web" }` files "Catervafy web" with `web`'s three rows and the system update, which depends on `web`'s Architect task, and records `catervafication.started { repos: [web] }` and the owner's approval. It files no Product Manager and no Marketing task. RED.
- `refuses_a_members_catervafication_in_order` (`human.rs`): `repo: "nope"` is `repo_unknown`. `repo: "web"` before the project-wide Start is `catervafication_not_started`. A second `repo: "web"` is `catervafication_started` naming `web`. With no Architect, Developer or Designer active, it is `catervafication_no_writers`. RED.

- [ ] `feat(runtime): document a repository the project gains later`

### Task 4: The skills

Files: the three `SKILL.md`; `kit.rs` (the tests).

- `the_catervafying_skills_name_the_repositorys_folder` (`kit.rs`, beside `every_owner_carries_catervafying_the_repository`, `:2300`): the Architect's, the Developer's and the Designer's each hold `## In a project of several repositories` and `docs/catervas/repos/`. The Architect's holds `mapping-the-system`. The Designer's holds "This repository has no screens.". Each stays under 6 KiB with no ` @`. RED.

- [ ] `feat(roles): document each repository in its own folder`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 4.1 and 5.16: the project folder's Catervafication, its rows and their order, the system task's dependencies, a missing member's rows
- 5.17: the per-member documents and the system documents' first writing
- 4.1: `offered_repos`, `catervafication_start { repo }` and its refusals
- 8.5: `catervafication.started`'s `repos`

- [ ] `docs(spec): record the Catervafication across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
