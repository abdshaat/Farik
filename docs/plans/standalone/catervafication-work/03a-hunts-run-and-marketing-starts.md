# Standalone catervafication-work, plan 3a: the hunts run and the marketing starts

Status: done
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.16 ("Its tasks"), 6.5, 6.8
Depends on: plans 1 to 3 of this branch
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (split from plan 3 in that review)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The tasks plan 3 files can be carried out:
- a Designer's hunt starts in its implement session, with no explore session and no design plan,
  and its writes (its report, its commit, its commands) pass the Designer's plan gate;
- the Marketing Specialist's propose answer tells it to start at once when Catervas approved the
  plan;
- the three skills say how to hunt and how to start the marketing.

Out of scope: hunts in a project folder (plan 3b); the web's words (plan 5).

## Decisions

- **The Designer's stage** (`rules.rs` `in_progress`): the design stage runs only for a Designer's
  task that is not a hunt (`!is_hunt(&contract)`). A hunt goes straight to its implement session,
  which `offered_connector` already gives the browser.
- **The Designer's plan gate passes a hunt.** ADR 0063 has a hunt skip the design plan, so the
  gate that holds a Designer's writes until its plan is approved (`design_plan_gate`,
  `crates/runtime/src/tools/design.rs`, ADR 0026) would otherwise refuse every write of the hunt.
  `design_plan_gate` takes `deps: &ToolDeps` in place of `log: &EventLog`. For a UI/UX Designer
  with a task, it passes when that task's contract (`deps.files.read_contract`) `is_hunt`, and
  otherwise as today, on the latest plan being approved. A contract that cannot be read is the
  gate's failure, as `failed`, as an unreadable log is. Its two callers change with it:
  `Call::permit` (`tools.rs`) and `plan_gate` (`daemon/hooks.rs`). The core rule
  `check_design_plan` is unchanged: the runtime passes it `approved: true` for a hunt.
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
  - Marketing, rewritten whole: it reads the product plan under `docs/catervas/product/` first,
    then the rest of the project, then writes and proposes the plan, committing `MP-<n>` and its
    agent version together. It asks for each missing service the plan needs with
    `catervas_request_connection` and carries on. When the answer says the plan is approved, it
    schedules the first posts with `catervas_schedule_post` now, writes
    `docs/catervas/marketing/kickoff` (naming `MP-<n>`, the posts scheduled and the connections
    asked for), commits it and asks for `verifying`. A plan with money waits for the owner: it
    ends its turn, and the sentence "Do not ask for `verifying` while the plan waits" stays; its
    next session, after the owner's decision, schedules the posts and writes the kickoff. The
    description becomes "Use when a task of the project's Catervafication asks you to write the
    marketing plan and start on it."
  - The Developer's and the Designer's descriptions end "…, or to hunt for bugs in its code" and
    "…, or to hunt for bugs in its screens".

## File map

```
crates/runtime/src/orchestrator/rules.rs         modifies: in_progress's design stage (Task 1)
crates/runtime/src/tools/design.rs               modifies: design_plan_gate, mod tests (Task 1)
crates/runtime/src/tools.rs, daemon/hooks.rs     modifies: the gate's two callers (Task 1)
crates/runtime/src/orchestrator/design.rs        tests:    a hunt skips it (Task 1)
crates/runtime/src/tools/marketing.rs            modifies: propose_plan's answer, mod tests (Task 2)
crates/roles/roles/{software_developer,ui_ux_designer,marketing_specialist}/skills/catervafying-the-repository/SKILL.md   modifies (Task 3)
crates/roles/src/kit.rs                          tests:    the skills (Task 3)
```

## Interfaces

Consumes: `is_hunt` (plan 2); `hunt_contract` (plan 3); `catervas_file_bug` (plan 2) and
`catervas_request_connection` (plan 1), by name in words; `a_designers_task`, `ticked`, `who`,
transcript `implement_by_iris_ctv_1` (`orchestrator::design::tests`); `TestProject`,
`filed_with`, `browsing` (`tools::fixtures`); `load_kit` (`catervas-roles`).

Produces:

```rust
// catervas-runtime::tools::design
pub(crate) fn design_plan_gate(deps: &ToolDeps, role: Role, tier: PermissionTier,
    task: Option<&TaskId>) -> Result<(), ToolError>;
```

## Tasks

### Task 1: a Designer's hunt skips the design plan

Files: `crates/runtime/src/orchestrator/rules.rs` (`in_progress`), `crates/runtime/src/tools/
design.rs` (`design_plan_gate`; `mod tests`), `crates/runtime/src/tools.rs` and
`crates/runtime/src/daemon/hooks.rs` (the callers), tested in `crates/runtime/src/orchestrator/
design.rs` `mod tests` (ignored, as its neighbours are) and `tools/design.rs` `mod tests`. The
`hooks.rs` tests of `design_plan_not_approved` keep their assertions: their tasks are no hunts.

- `a_hunt_starts_in_its_implement_session` (`orchestrator/design.rs`). In
  `a_designers_task("design-hunt", …)` the change sets `allowed_paths` to
  `["docs/catervas/design/ui-bug-hunt.html"]` and the one criterion to that artifact. `ticked`
  with `vec![implement_by_iris_ctv_1()]` once gives:
  - `who` is `[("iris", SessionPurpose::Implement)]`;
  - no `design_plan.proposed` is recorded.
- `the_plan_gate_passes_a_designers_hunt` (`tools/design.rs`). On a `TestProject` of
  `a_team_of_three(browsing)` with no design plan in the log:
  - `design_plan_gate(&deps, UiUxDesigner, GitLocal, Some(CTV-1))` is `Ok(())` when CTV-1 is filed
    (`filed_with`) as a `ui_ux_designer` task whose `allowed_paths` are
    `["docs/catervas/design/ui-bug-hunt.html"]`;
  - for CTV-2, the same with `allowed_paths` `["site/**"]`, it is refused with a reason starting
    `design_plan_not_approved: `.

- [x] `feat(runtime): a Designer hunts without proposing a design plan`

### Task 2: the plan says to start now

Files: `crates/runtime/src/tools/marketing.rs` (`propose_plan`'s answer; `mod tests`). The test is
ignored, needing git, as its neighbours are. `proposes_a_plan_and_writes_its_text`, whose plan
spends 2,000, keeps its "end your turn" assertion.

- `answers_start_now_when_the_plan_spends_nothing`:
  - a proposal whose every budget is zero answers `next` exactly "MP-1 is approved: it spends
    nothing. Start on it now: schedule its first posts with catervas_schedule_post, then write
    your kickoff document and commit it.";
  - a proposal with a total budget of 100 answers `next` "end your turn: the owner's decision
    starts the next session".

- [x] `feat(runtime): a marketing plan approved at once is started in the same session`

### Task 3: the skills

Files: the three `SKILL.md` of the File map; `crates/roles/src/kit.rs` `mod tests`.

- `catervafying_skills_name_the_new_work`:
  - the Software Developer's text contains `` `catervas_file_bug` ``, `bug-hunt` and `Bugs filed`;
  - the UI/UX Designer's contains `` `catervas_file_bug` ``, `ui-bug-hunt`, `Bugs filed` and
    `` `catervas_check_page` ``;
  - the Marketing Specialist's contains `` `catervas_request_connection` ``,
    `` `catervas_schedule_post` `` and `docs/catervas/marketing/kickoff`, and not "and only that";
  - each description starts "Use when".

  `every_owner_carries_catervafying_the_repository` still passes unchanged: each under 6 KiB, each
  naming its folder, and the Marketing Specialist's naming `docs/catervas/product/`,
  `` `catervas_propose_marketing_plan` ``, "its agent version" and "Do not ask for `verifying`
  while the plan waits", which the rewrite keeps (Decisions). If ADR 0061 changed that list, its
  list holds and the rewrite keeps those phrases instead.

- [x] `docs(roles): the skills say how to hunt and how to start the marketing`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Tasks 1 and 2's ignored tests among them)
```
