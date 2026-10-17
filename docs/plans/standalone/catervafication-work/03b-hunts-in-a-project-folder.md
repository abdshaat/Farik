# Standalone catervafication-work, plan 3b: a project folder hunts each member

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.16 ("Its tasks"), 6.4, 6.8; ADR 0057
Depends on: plans 1 to 3 of this branch
Readiness confirmed by: pending

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The founder: "Folders hunt each repo". Today's code Catervafies a project folder as one
repository: its rows are the project-wide documentation tasks, and no task names a `repo`. Phase 9
step 06, the per-member Catervafication, is planned, not executed. This plan stays within today's
code. When a project folder is Catervafied, the breakdown tick also files, for each member present
in `repos.yaml` order:
- the Developer's code bug hunt;
- the Designer's UI bug hunt, when that member has a UI.

Each hunt names that member in `repo`, so it runs in that member's worktree, sandbox and preview.
A member with no UI gets no UI hunt, and the epic carries a progress note saying why. The epic's
budget counts two hunts per member.

Out of scope:
- moving the reports into the project folder's `docs/catervas/repos/<name>/`, which phase 9 step 06
  may do;
- a member added after the Start, which phase 9 step 06's per-member start covers.

## Decisions

- **A member hunt names `repo`.** Readiness allows `repo` on a Developer's or a Designer's task
  in a project folder (`repo_not_allowed`, `repo_unknown`). Phase 9 step 02 makes that task's
  branch and worktree from the member, step 02b runs its criteria there, and step 02c opens its
  pull request in the member. Chosen over a hunt with no `repo` writing
  `docs/catervas/repos/<name>/`: such a task works in the project folder, where the member's
  tests cannot run and its app cannot start.
- **Its report** is at `hunt_report(role, format)` inside the member, the path a one-repo hunt
  uses. So `is_hunt` holds unchanged, and the report reaches the member by its pull request.
- **A member has a UI** when `scan_project(&Git::open(<member path>), now)` answers `app_start:
  Some(_)`: its `package.json` has a `dev`, `start` or `preview` script (ADR 0054). A scan that
  fails counts as no UI. The Designer's hunt also needs `designer_ready`, as in plan 3. The
  preview it opens is the team's one preview, run in the member's worktree, as for any Designer
  task naming that member.
- **The note.** For each member that would get a UI hunt but has no UI (an active, ready Designer),
  Catervas records on the epic, once, `note.written { kind: progress, text: "No UI bug hunt for
  <name>: Catervas found no way to start its app.", written_by: "catervas" }`. "Once" means no
  `note.written` of that text in the epic's history.
- **Missing members.** Only members present when the tick runs (`repo_present`) are hunted. A
  missing member has no hunt filed for it, and is not noted.
- **Hunt wording for a member:**
  - title "Hunt for bugs in <name>'s code" or "Hunt for bugs in <name>'s screens";
  - summary as plan 3's with "your code" replaced by "<name>'s code" and "your app's screens" by
    "<name>'s screens";
  - intent, criterion, reviewer and paths as plan 3's;
  - `repo` set to `<name>`.
- **The rows.** Plan 3's `catervafication_hunts` becomes
  `catervafication_hunts(team, project, designer_ready, members: Option<&[HuntMember]>) -> Hunts`.
  - It answers nothing for `New`.
  - With `members: None` (one repository) it answers plan 3's rows, each `repo: None`.
  - With `Some(members)`, for each member in order: the Developer's hunt (when one is active),
    then the Designer's (when one is active, `designer_ready` and `has_ui`). A member that only
    lacks `has_ui` goes in `no_ui`.
- **`hunt_members(deps)`** answers `None` when there is no `.catervas/repos.yaml`. Otherwise it
  answers the present members, in order, each with `has_ui`.
- **Held, by member.** `file_due` holds a task under the epic as `(assignee_role, is_hunt,
  contract.repo)`. A hunt row is due when no held task matches its role, `true` and its `repo`.
- **The epic's budget.** `epic_contract(project, max_cost_usd, hunted: usize)`. For `Existing` the
  budget is `max_cost_usd * (5 + 2 * hunted)`, and for `New` `max_cost_usd * 3`.
  - `catervafication::start` passes `hunted`: the number of members `hunt_members` answers, or 1
    when it answers `None`.
  - A one-repo epic is unchanged at 7.
- **`catervafication.get`'s `roles`** uses the same rows: a role is added when any row has it.
- **The criterion of a folder's epic** is ADR 0061's to settle (its plan 1, open decision 2:
  `EpicCommandInProjectFolder` refuses the `command` that plan puts on the epic). This plan adds
  none. If that decision is not made when this plan starts, stop and send it back to planning.

## File map

```
crates/runtime/src/catervafication.rs        modifies: HuntMember, Hunt, Hunts, hunt_members, catervafication_hunts, hunt_contract's repo, epic_contract's hunted, start, file_due, mod tests (Tasks 1, 2)
crates/runtime/src/daemon/gates.rs           modifies: catervafication_get (Task 2)
crates/runtime/src/orchestrator/requests.rs  tests:    the folder's first tick (Task 2)
```

## Interfaces

Consumes: plan 3's `catervafication_hunts`, `hunt_contract`, `epic_contract`, `epic_hunts`,
`file_due`; `ProjectFiles::read_repos`, `repo_present`, `scan_project`, `Git::open`, `AppStart`
(`catervas-store`); `ReadinessContext.repos` (`catervas-core`); `Harness::project_folder_with`,
`with_the_suggested_six`, `FakePreviews::ready` (fixtures).

Produces:

```rust
// catervas-runtime::catervafication
#[derive(Debug, Clone, PartialEq, Eq)] pub(crate) struct HuntMember { pub name: String, pub has_ui: bool }
#[derive(Debug, Clone, PartialEq, Eq)] pub(crate) struct Hunt { pub role: Role, pub repo: Option<String> }
#[derive(Debug, Default, PartialEq, Eq)] pub(crate) struct Hunts { pub rows: Vec<Hunt>, pub no_ui: Vec<String> }
pub(crate) fn hunt_members(deps: &ToolDeps) -> Result<Option<Vec<HuntMember>>, CatervaficationError>;
#[must_use] pub(crate) fn catervafication_hunts(team: &Team, project: Project, designer_ready: bool, members: Option<&[HuntMember]>) -> Hunts;
#[must_use] pub(crate) fn hunt_contract(role: Role, max_cost_usd: f64, format: DocumentFormat, repo: Option<&str>) -> Value;
#[must_use] pub(crate) fn epic_contract(project: Project, max_cost_usd: f64, hunted: usize) -> Value;
```

## Tasks

### Task 1: the rows of a folder

Files: `crates/runtime/src/catervafication.rs` (types, `catervafication_hunts`, `hunt_contract`,
`epic_contract`; `mod tests`). Update plan 3's tests to the new signatures; their assertions stay.

- `hunts_each_member_of_a_project_folder`:
  - For the suggested six, `Existing`, ready, and members `[api (no UI), web (UI)]`, the rows
    are `[(SoftwareDeveloper, api), (SoftwareDeveloper, web), (UiUxDesigner, web)]` and `no_ui`
    is `["api"]`.
  - With `designer_ready` false, the rows are the two Developer rows and `no_ui` is empty.
  - `hunt_contract(SoftwareDeveloper, 20.0, Html, Some("api"))`:
    - has `repo` `api`, title "Hunt for bugs in api's code" and `allowed_paths`
      `["docs/catervas/engineering/bug-hunt.html"]`;
    - validated, it `is_hunt`;
    - it passes `evaluate_readiness` with a context whose `repos` is `["api", "web"]` and whose
      parent is the epic.
  - `epic_contract(Existing, 20.0, 2)`'s budget is 180, and `(Existing, 20.0, 1)`'s is 140.

- [ ] `feat(runtime): a project folder's Catervafication hunts each member`

### Task 2: filed in the first tick, with the note

Files: `crates/runtime/src/catervafication.rs` (`hunt_members`, `start`, `file_due`),
`crates/runtime/src/daemon/gates.rs`, tested in `crates/runtime/src/orchestrator/requests.rs`
`mod tests` (ignored, needs git, as its neighbours are).

- `files_a_hunt_per_member_in_the_first_tick`. Set-up:
  - `Harness::project_folder_with("req-catervafication-folder", &["api", "web"],
    with_the_suggested_six)`, its previews `FakePreviews::ready()`;
  - `web` holds a committed `package.json` with `{ "scripts": { "dev": "vite" } }`; `api` holds
    none.

  After `CatervaficationStart` and the ticks of `a_catervafication_in_progress`, the breakdown
  tick:
  - files the five documentation rows of plan 3, then hunts naming `repo` `api`
    (`software_developer`), `web` (`software_developer`) and `web` (`ui_ux_designer`), in that
    order;
  - leaves the epic's history with one `note.written` "No UI bug hunt for api: Catervas found no
    way to start its app.", `written_by` `catervas`;
  - a second tick files nothing and writes no second note;
  - the epic's budget is 9 placeholder budgets;
  - `catervafication.get`, asked before the Start on a fresh harness, lists `ui_ux_designer`
    among its `roles`.

- [ ] `feat(runtime): file each member's hunts and say why a member has no UI hunt`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Task 2's ignored test among them)
```
