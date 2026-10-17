# Standalone catervafication-work, plan 3: the Catervafication creates work

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.16 ("The Catervafication's epic", "Its tasks"), 6.4, 6.5, 6.8, 5.17
Depends on: plans 1 and 2 of this branch
Readiness confirmed by: pending

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

An existing repository's Catervafication files every task in its first breakdown tick:
- the documentation tasks, as before;
- the Marketing Specialist's "Write the marketing plan and start on it", with no dependency;
- a code bug hunt for an active Software Developer;
- a UI bug hunt for an active UI/UX Designer whose browser is ready.

A new project's Catervafication files its three tasks, the Marketing one being the new one, and no
hunt. A Designer's hunt starts in its implement session, with no explore or design plan. The
Marketing Specialist's propose answer tells it to start at once when Catervas approved the plan.
The skills say how. Out of scope:
- hunts for a new project or a project folder (ADR 0063);
- the web's words (plan 5).

## Decisions

- **Hunts are their own rows**, `catervafication_hunts(team, project, designer_ready,
  project_folder) -> Vec<Role>`, beside `catervafication_roles`, whose signature and callers stay.
  It answers `[]` unless `project == Existing && !project_folder`. Otherwise it answers, in this
  order:
  - `SoftwareDeveloper`, when one is active;
  - `UiUxDesigner`, when one is active and `designer_ready`.

  A hunt's reviewer role is always `ProductManager` (ADR 0063), so it needs no
  `default_reviewer_role`.
- **`hunt_contract(role, max_cost_usd, format) -> Value`**. The wording is exact, and `<path>` is
  `hunt_report(role, format)`.
  - Developer:
    - title "Hunt for bugs in the code";
    - intent "Read the code, run its tests and checks in the sandbox, and file each bug you find
      with catervas_file_bug, with its evidence. Change no code. Then write what you checked and
      the bugs you filed in bug-hunt. Follow your skill catervafying-the-repository.";
    - summary "The Software Developer looks for bugs in your code and files each one it finds,
      so the team can fix it."
  - Designer:
    - title "Hunt for bugs in the app's screens";
    - intent "Open the app in your browser, go through its screens at phone and desktop sizes,
      and file each bug you find with catervas_file_bug, with its evidence. Change no code. Then
      write what you checked and the bugs you filed in ui-bug-hunt. Follow your skill
      catervafying-the-repository.";
    - summary "The UI/UX Designer goes through your app's screens and files each problem it
      finds, so the team can fix it."
  - Both:
    - scope `in_scope` `[<path>, "Bugs filed with catervas_file_bug, at most 10."]`;
    - `out_of_scope` `["Changing the code or the screens: each bug is filed, and fixed later as
      its own task."]`;
    - requirement R1 "Each bug found is filed with its evidence, and <file name> says what was
      checked and lists every bug filed under Bugs filed.";
    - one criterion C1 "<path> exists, holds a heading and lists the bugs filed.", `{ method:
      artifact, path: <path>, must_contain: artifact_marks(<path>, format) followed by "Bugs
      filed" }`, satisfies R1;
    - `assignee_role` the role, `reviewer_role` `product_manager`, risk `low`, the budget
      `max_cost_usd`, `allowed_paths` `[<path>]`.

  Every hunt contract `is_hunt` (plan 2).
- **The Marketing row** (both projects):
  - title "Write the marketing plan and start on it";
  - intent "Read the project, write its first marketing plan and propose it with
    catervas_propose_marketing_plan, and start on it at once: schedule the first posts the plan
    allows, and ask for any connection the plan needs with catervas_request_connection. Then
    write down what you started in kickoff." The shared " Follow your skill
    catervafying-the-repository." follows, as every row's does;
  - summary "The Marketing Specialist writes your marketing plan and starts on it straight away;
    anything that costs money waits for you.";
  - documents `["kickoff"]`;
  - its artifact criterion's `must_contain` adds `"MP-"` after the heading mark.

  `Row` gains `also_contains: &'static [&'static str]`, `&[]` on every other row. The
  `paths.is_empty()` branch of `task_contract` and its words go, since no row is without a
  document now.
- **Marketing goes alongside.** `catervafication_roles` drops the clause that lists the
  Marketing Specialist only after a Product Manager's row. `file_due` files the Marketing task
  with no `dependencies`, removing the line ADR 0061's plan 5 added. Its doc comment says so.
- **Only an epic that asks for hunts gets them.** The existing repository's `epic_contract`
  changes:
  - It gains requirement R2, text `HUNTS_REQUIREMENT` = "The Software Developer and the UI/UX
    Designer, when the team has them, hunt for bugs, and every bug found is filed as a request."
  - Its C1 `satisfies` becomes `["R1", "R2"]`.
  - Its budget becomes `max_cost_usd * 7.0`.
  - Its intent becomes "The team reads the project and writes down what it knows, each role in
    its own folder under docs/catervas/, while the Developer and the Designer hunt for bugs and
    the Marketing Specialist starts the marketing plan."
  - Its summary becomes "Your team reads your project and writes down what it learns, looks for
    bugs and starts your marketing. You can read all of it in Files."

  The new project's epic is unchanged. `epic_hunts(epic: &TaskContract) -> bool` is true when a
  requirement's text equals `HUNTS_REQUIREMENT`. An epic filed before this change has none, and
  its 5× budget is never overrun.
- **`file_due` order:**
  1. `catervafication_roles`'s rows, as before;
  2. then, when `epic_hunts`, each role of `catervafication_hunts(team, project, designer_ready,
     deps.files.is_project_folder())`.

  A task under the epic is held as `(assignee_role, is_hunt(&contract))`. A documentation row
  is due when no held task is `(role, false)`, and a hunt when none is `(role, true)`, whatever
  the status, as today. Its report stays "filed the Catervafication's tasks: <ids>".
- **The Designer's stage** (`rules.rs` `in_progress`): the design stage runs only for a Designer's
  task that is not a hunt (`!is_hunt(&contract)`). A hunt goes straight to its implement session,
  which `offered_connector` already gives the browser.
- **`catervafication.get`'s `roles`**: `catervafication_roles`'s roles, then each role of
  `catervafication_hunts(team, Existing, ready, deps.files.is_project_folder())` not already
  listed. `offered` is unchanged.
- **`DOCUMENT_STEMS`** grows to 15: `("docs/catervas/engineering/bug-hunt", "Bug hunt")` after
  `workflow`, `("docs/catervas/design/ui-bug-hunt", "UI bug hunt")` after `ui-inventory`, and
  `("docs/catervas/marketing/kickoff", "Marketing kickoff")` after `persona`.
- **The propose answer.** When `catervas_propose_marketing_plan` has recorded
  `marketing_plan.approved` at once (ADR 0061), its `next` is "<MP-n> is approved: it spends
  nothing. Start on it now: schedule its first posts with catervas_schedule_post, then write
  your kickoff document and commit it." Otherwise it stays "end your turn: the owner's decision
  starts the next session".
- **The skills.** Each role's `catervafying-the-repository` gains a section on the new work and
  stays under 6 KiB. ADR 0061's edits to these files are kept where they do not contradict the
  text below.
  - Developer, "## Hunting for bugs": its one allowed path is
    `docs/catervas/engineering/bug-hunt` in the team's format, and it changes no code. It reads
    the code and runs the project's tests and checks with `catervas_exec`. It files each real bug
    with `catervas_file_bug`, with evidence (the command and the lines of output, or the file and
    line): at most 10, never the same one twice, and a failing test only when the code, not the
    test, is wrong. It removes anything the commands left in the worktree. Its report says what it
    checked, then a section headed "Bugs filed" with each id and title, or "None found". It
    commits with `catervas_git_commit`.
  - Designer, "## Hunting for bugs in the screens": its one path is
    `docs/catervas/design/ui-bug-hunt`, and it changes no code. It goes through every screen of
    the preview with the browser at 375 and 1280 pixels wide, and runs `catervas_check_page`. It
    looks for broken layout, console errors, controls that do nothing, cut-off text, low contrast
    and anything a keyboard cannot reach. Its evidence is the address, the width, what the
    snapshot shows and the console's lines. It files with `catervas_file_bug`, and reports as the
    Developer does.
  - Marketing, rewritten whole: it reads the whole project, then writes and proposes the plan.
    When the answer says the plan is approved, it schedules the first posts with
    `catervas_schedule_post` now; a plan with money waits for the owner, and it ends its turn. It
    asks for each missing service the plan needs with `catervas_request_connection` and carries
    on. It writes `docs/catervas/marketing/kickoff`, naming `MP-<n>`, the posts scheduled and the
    connections asked for, and commits it. The description becomes "Use when a task of the
    project's Catervafication asks you to write the marketing plan and start on it."
  - The Developer's and the Designer's descriptions end "…, or to hunt for bugs in its code" and
    "…, or to hunt for bugs in its screens".

## File map

```
crates/core/src/folders.rs                       modifies: DOCUMENT_STEMS (Task 1)
crates/runtime/src/catervafication.rs            modifies: rows, hunts, epic, mod tests (Task 1); file_due (Task 2)
crates/runtime/src/orchestrator/requests.rs      tests:    the first tick (Task 2)
crates/runtime/src/daemon/gates.rs               modifies: catervafication_get, mod tests (Task 2)
crates/runtime/src/orchestrator/rules.rs         modifies: in_progress's design stage (Task 3)
crates/runtime/src/orchestrator/design.rs        tests:    a hunt skips it (Task 3)
crates/runtime/src/tools/marketing.rs            modifies: propose_plan's answer, mod tests (Task 4)
crates/roles/roles/{software_developer,ui_ux_designer,marketing_specialist}/skills/catervafying-the-repository/SKILL.md   modifies (Task 5)
crates/roles/src/kit.rs                          tests:    the skills (Task 5)
```

## Interfaces

Consumes: `hunt_report`, `is_hunt` (plan 2); `catervas_file_bug` (plan 2) and
`catervas_request_connection` (plan 1), by name in words; `artifact_marks`, `with_format`,
`role_folder`, `DocumentFormat` (`catervas-core`); `file_request`, `placeholder_budget_usd`,
`ProjectFiles::is_project_folder` (`catervas-store`); `Harness::with_the_suggested_six`,
`a_catervafication_in_progress`, `roles_under_the_epic` (`orchestrator::requests::tests`);
`a_designers_task`, `ticked`, `who`, transcript `implement_by_iris_ctv_1`
(`orchestrator::design::tests`); `load_kit` (`catervas-roles`).

Produces:

```rust
// catervas-runtime::catervafication
pub(crate) const HUNTS_REQUIREMENT: &str;
#[must_use] pub(crate) fn catervafication_hunts(team: &Team, project: Project, designer_ready: bool, project_folder: bool) -> Vec<Role>;
#[must_use] pub(crate) fn hunt_contract(role: Role, max_cost_usd: f64, format: DocumentFormat) -> Value;
#[must_use] pub(crate) fn epic_hunts(epic: &TaskContract) -> bool;
```

## Tasks

### Task 1: the templates

Files: `crates/core/src/folders.rs` (`DOCUMENT_STEMS`), `crates/runtime/src/catervafication.rs`
(the rows, `hunt_contract`, `catervafication_hunts`, `HUNTS_REQUIREMENT`, `epic_hunts`,
`epic_contract`, `catervafication_roles`'s Marketing clause; `mod tests`). Update the module's
existing tests to the new rows:
- the Marketing row's title, documents and marks in `writes_each_template_as_the_table_says`;
- the 7.0 count in `every_template_passes_the_definition_of_ready`, which also runs each
  `hunt_contract` through `validate_contract` and `evaluate_readiness` and expects `Ok(())`;
- `lists_the_roles_that_get_a_task_with_their_reviewers`'s cases, which no longer depend on the
  Product Manager's row for the Marketing Specialist.

- `files_hunts_for_an_existing_repository_alone`:
  - `catervafication_hunts` for the suggested six, `Existing`, ready, not a folder, is
    `[SoftwareDeveloper, UiUxDesigner]`; not ready, `[SoftwareDeveloper]`; `New`, `[]`; a project
    folder, `[]`; a Product Manager and one Developer, `Existing`, `[SoftwareDeveloper]`.
  - `hunt_contract(SoftwareDeveloper, 20.0, Html)`:
    - title "Hunt for bugs in the code";
    - `allowed_paths` `["docs/catervas/engineering/bug-hunt.html"]`;
    - one criterion, `must_contain` `["<h1", "Bugs filed"]`;
    - `reviewer_role` `product_manager`;
    - `is_hunt` of it validated is true.
  - `hunt_contract(UiUxDesigner, 20.0, Markdown)`: `allowed_paths`
    `["docs/catervas/design/ui-bug-hunt.md"]`, `must_contain` `["# ", "Bugs filed"]`.
  - `epic_hunts` of `epic_contract(Existing, 20.0)` validated is true, and its budget is 140;
    of `epic_contract(New, 20.0)`, false.
  - The Marketing task in Markdown has `allowed_paths` `["docs/catervas/marketing/**"]`, and one
    artifact on `docs/catervas/marketing/kickoff.md` with `must_contain` `["# ", "MP-"]`.

- [ ] `feat(runtime): the Catervafication's templates hunt for bugs and start the marketing`

### Task 2: every task in the first tick

Files: `crates/runtime/src/catervafication.rs` (`file_due`), `crates/runtime/src/orchestrator/
requests.rs` (`mod tests`), `crates/runtime/src/daemon/gates.rs` (`catervafication_get`; `mod
tests`). Tests `#[ignore = "needs the git program: cargo xtask check --integration"]`, as their
neighbours are.

Delete each test that asserts the Marketing task waits for, or depends on, the Product Manager's
task. On `main` those are `files_the_marketing_task_once_the_product_plan_is_in` and
`files_no_marketing_task_for_a_cancelled_product_plan`, or whatever ADR 0061's plan 5 renamed them
to. Their behaviour is removed by ADR 0063.

- `files_every_task_in_the_first_tick` replaces `files_one_task_per_role_with_a_folder`, with
  the same harness:
  - The tick reports "filed the Catervafication's tasks: CTV-2, CTV-3, CTV-4, CTV-5, CTV-6,
    CTV-7, CTV-8".
  - `roles_under_the_epic` is `[product_manager, architect, software_developer,
    ui_ux_designer, marketing_specialist, software_developer, ui_ux_designer]`.
  - `CTV-6`'s contract has no `dependencies`; `CTV-7` and `CTV-8` are hunts (`is_hunt`).
  - No session started, and a second tick files nothing.
  - The unready harness files `[product_manager, architect, software_developer,
    marketing_specialist, software_developer]`.
- `an_epic_without_the_hunts_requirement_files_no_hunt`. In the same set-up, before the
  breakdown tick, write CTV-1's contract again without R2 (`requirements` R1 alone, C1 satisfying
  `["R1"]`). The tick then files five tasks, none a hunt.
- `offers_the_hunts_among_the_roles` (`gates.rs`). For a Product Manager and one Developer,
  `catervafication.get` answers `roles: ["product_manager", "software_developer"]` and `offered:
  true`.

- [ ] `feat(runtime): a Catervafication files its hunts and its marketing in its first tick`

### Task 3: a Designer's hunt skips the design plan

Files: `crates/runtime/src/orchestrator/rules.rs` (`in_progress`), tested in
`crates/runtime/src/orchestrator/design.rs` `mod tests` (ignored, as its neighbours are).

- `a_hunt_starts_in_its_implement_session`. In `a_designers_task("design-hunt", …)` the change
  sets `allowed_paths` to `["docs/catervas/design/ui-bug-hunt.html"]` and the one criterion to
  that artifact. `ticked` with `vec![implement_by_iris_ctv_1()]` once gives:
  - `who` is `[("iris", SessionPurpose::Implement)]`;
  - no `design_plan.proposed` is recorded.

- [ ] `feat(runtime): a Designer hunts without proposing a design plan`

### Task 4: the plan says to start now

Files: `crates/runtime/src/tools/marketing.rs` (`propose_plan`'s answer; `mod tests`).

- `answers_start_now_when_the_plan_spends_nothing`:
  - a proposal whose every budget is zero answers `next` exactly "MP-1 is approved: it spends
    nothing. Start on it now: schedule its first posts with catervas_schedule_post, then write
    your kickoff document and commit it.";
  - a proposal with a total budget of 100 answers `next` "end your turn: the owner's decision
    starts the next session".

- [ ] `feat(runtime): a marketing plan approved at once is started in the same session`

### Task 5: the skills

Files: the three `SKILL.md` of the File map; `crates/roles/src/kit.rs` `mod tests`.

- `catervafying_skills_name_the_new_work`:
  - the Software Developer's text contains `` `catervas_file_bug` ``, `bug-hunt` and `Bugs filed`;
  - the UI/UX Designer's contains `` `catervas_file_bug` ``, `ui-bug-hunt`, `Bugs filed` and
    `` `catervas_check_page` ``;
  - the Marketing Specialist's contains `` `catervas_request_connection` ``,
    `` `catervas_schedule_post` `` and `docs/catervas/marketing/kickoff`, and not "and only that";
  - each description starts "Use when".

  `every_owner_carries_catervafying_the_repository` still passes: each under 6 KiB, each naming
  its folder.

- [ ] `docs(roles): the skills say how to hunt and how to start the marketing`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Tasks 2 and 3's ignored tests among them)
```
