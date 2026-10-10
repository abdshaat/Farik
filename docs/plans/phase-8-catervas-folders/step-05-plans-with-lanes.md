# Phase 8, step 05: Plans with lanes

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.2, 5.3, 5.13, 5.16, 6.1, 6.2, 6.3; F4, F5, F6, F14
Depends on: steps 01 and 01b of this phase (planned, not yet executed: `catervas_core::folders`, `FolderOwned`, the planners' folder sentence, every owner's folder line; `no_task_no_write`, the Product Manager's docs tasks); phase 7 and the Catervas rename (merged on main, 2c28b555); the task id rename to `CTV-<n>` (the founder's decision, a separate pull request that lands on main first, as for step 01b); ADR 0051 and `docs/design/catervas-folders.md` ("Planning a feature"). Step 02 (ready) also edits the Product Manager's `system.md` and `writing-task-contracts`, `folders.rs` and `rules.rs`; this plan keeps its lines. Steps 03 and 04 touch nothing here.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): ready; no Blocking; carried into execution S1–S9 and N1–N6

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 623be48d, before the task id rename; steps 01, 01b and 02 move lines in `readiness.rs`, `paths.rs`, `plain.rs` and the role files, whose items this plan names by function or section.

## Goal

An approved epic is planned in the order the founder chose: the Product Manager triages and writes it and the owner approves it, as today; the Architect writes its implementation plan, `docs/catervas/architecture/plans/<epic-id>.md`, in lanes no wider than the team's active builders; the Product Manager writes one task per planned task from that plan; the Scrum Master assigns them, each lane to one builder. Readiness refuses two lanes of an epic whose paths could meet (`lanes_overlap`) and a lane past the builders (`lane_beyond_builders`), and the assignment gate refuses giving a lane's task to anyone but its builder (`lane_taken`). With no Architect the Product Manager plans the lanes itself; with no Scrum Master it assigns, as today. The Scrum Master no longer breaks epics down. Out of scope: any screen (the Board shows no lane; a "Lane 2" on its cards is a screen change, mocked up first, in a later step); `catervas_write_folder_doc` (step 03); Catervafication's plans (07).

## Decisions

The founder's (2026-10-09, ADR 0051) and the controller's rulings fix the order, the fallbacks, the two fields, both readiness rules and the overlap test. This plan decides the rest:

- **The epic keeps its assignee, reviewer and close-out; the breakdown moves to the Product Manager.** `ready_epic` (`crates/runtime/src/orchestrator/requests.rs:325`) still assigns an approved epic to the Scrum Master, else the Product Manager, and the assignee still closes it out (completion note, `verifying`, and the tasks a rejection or the human's message asks for, `close_out_message`, `messages.rs:177`). Rule 6's breakdown, today a `plan` session of the epic's assignee when no task under it is live (`in_progress_epic`, `requests.rs:414`; `breakdown_message`, `messages.rs:157`), becomes the Product Manager's `plan` session (`product_manager`, `requests.rs:46`), in two steps below. Rejected: assigning epics to the Product Manager, whose WIP limit (default 1, `open_tasks`, `transitions.rs:1290`) would then hold its own docs tasks (step 01b) while an epic is open, and whose review of the Scrum Master's epics would pass to the human; moving the close-out, since only the assignee asks `in_progress -> verifying` (`transition_table.rs:139-143`).
- **The Architect writes the plan in a docs task of its own, not at sprint planning.** The breakdown is a `plan` session at the project root, which step 01b makes write nothing (`no_task_no_write`); sprint planning is the Scrum Master's ceremony, once a sprint, and the ceremony write tool is step 03's. So the Product Manager's session files the plan task with `catervas_create_task` (`parent` the epic, `assignee_role` `architect`, `plan` and `allowed_paths` `[plan_path(epic)]`, no `lane`); the Scrum Master assigns it in rule 8 (`ready`, `rules.rs:1438`) with `catervas_assign_task`; the Architect writes it in that task's `implement` session with `Write` and commits with `catervas_git_commit` on `docs/CTV-<n>`; the Product Manager reviews it (`REVIEWER_ROLE_FOR`, `crates/roles/src/reviewer.rs:20`). Under `plan_in_sprints` it is a task: it waits in the Backlog with its epic, and the sprint that plans the epic writes the plan and then builds it.
- **What comes next is read from the epic's tasks, nothing stored.** `epic_step` reads each task under the epic: its status, `awaiting_integration`, and whether it is a plan task. A plan task is one whose `allowed_paths` are exactly `[plan]`. Every task cancelled, or none: **break down**. Any task neither accepted nor cancelled: nothing. Write the plan's tasks when every task not cancelled is a plan task, one is accepted and none awaits integration; nothing while every task not cancelled is a plan task and one awaits integration; otherwise close out. For "write the plan's tasks", Catervas reads `plan_path(epic)` on the integration branch (`Git::file_at`, `crates/store/src/git.rs:243`; `integration_branch`, `:733`); a `CommandFailed` reads as no plan and the step is **break down** again. The session's message lists the epic's tasks so far. Rejected: a stored stage or a new event; telling the plan task by a missing `lane`, which a built task filed without its lane would share. If the Architect is retired after its plan task is filed, the human cancels the plan task; break down then runs without the Architect.
- **Break down**: with an active Architect, `plan_task_message`; without, `breakdown_message` in lanes, the Product Manager filing each task with its `lane` and no `plan`, with no plan document: a document needs a docs task and a reviewer (step 01b: the Architect, else the Scrum Master), which a team of a Product Manager and Developers lacks. Rejected: the Product Manager's own plan docs task. With no active builder the message names no lane.
- **Who files an epic's tasks**: `check_child_creation` (`crates/core/src/governor/gates.rs:677`) admits the Product Manager beside the epic's assignee and the human. Rejected: the Product Manager alone, since the assignee's close-out files the tasks a rejection or the human's message asks for.
- **The fields.** `plan`: a string, pattern `^docs/catervas/architecture/plans/CTV-[0-9]{1,6}\.md$` (the `id` pattern after the rename), "the implementation plan this task writes or is built from"; `lane`: an integer, 1 to 16, "which lane of its epic's plan; read only on a task with a parent". A lane is read only on a task with a parent. Both are content (`FIELDS_OF_THE_CONTENT`, `gates.rs:986`): written while refining, frozen from `ready` (5.11). typify generates them at compile time (ADR 0009; there is no `cargo xtask generate`): `lane` as `Option<NonZeroU64>`, as `budget.max_sessions` is, read through `.get()`. The YAML store serialises the generated type and changes nothing. No `rpc`, `event` or `command` schema changes: `contract.get` answers the contract as `task-contract.schema.json` shapes it (`rpc.schema.json:1829`), `toCamel` maps every key (`packages/protocol-client/src/mapping.ts`), the web app's `Contract` names only the fields pages read (`apps/web/src/pages/PlanPage.tsx:33`), "the plan as written" shows every field as YAML (`:246`), and the editor saves the read contract with the person's changes over it (`PlanEditor.tsx:139-145`), so both survive a save. No rule checks that `plan` names its own epic: the steps above read `plan_path(epic)`, not the field.
- **`lanes_overlap`** reads, through `ParentState.lanes`, every other task of the epic that is not cancelled and has a lane; a task fails when an allowed path of its own could meet one of a task in another lane. Each pair is checked when the later of the two becomes ready. **`could_overlap(a, b)`**, in `crates/core/src/governor/paths.rs`: each glob's braces expanded (`expand_braces`, `:186`), backslashes as `/`, empty and `.` segments dropped; a glob holding a `..` segment meets anything; else the two meet unless, over the segments before each one's first wildcard segment (`is_a_wildcard`, `:180`), some segment differs ignoring letter case. Marked `// ponytail: literal prefixes only, so src/*.rs and src/*.ts are taken to meet; exact glob intersection if it refuses real plans.`
- **`lane_beyond_builders` is readiness, not the scheduling tool**: the lane is content, frozen from `ready`, so only refining can renumber it, and a readiness failure goes back to the Product Manager who writes it; readiness already counts active agents by role (`ReadinessContext.active_agents_by_role`). Builders are active agents whose role `changes_code` (`crates/core/src/team.rs:105`). A builder paused later does not strand a lane: one builder may hold several lanes.
- **The plan lies inside its epic.** `paths_within_parent` (`readiness.rs:876`) passes an allowed path equal to the task's `plan` when `plan` is `plan_path(<the task's parent>)`, so an epic need not name the plans folder. Rejected: asking every epic to name it.
- **Lane to builder is recorded nowhere new.** The Scrum Master, else the Product Manager, assigns with `catervas_assign_task` in rule 8's `plan` session, as today; `catervas_plan_sprint` (`tools/contracts.rs:369`) still brings an epic with every task under it. A lane's builder is read from the board: of the epic's other tasks in that lane with the task's assignee role, not cancelled, whose assignee is active, the assignee of the numerically highest one. The gate refuses anyone else; rule 8 offers only the builder (and waits while it has no room); the lane's first task goes to any agent of the role with room. Rejected: an event field (`task.transitioned` already names the assignee) or a contract field (frozen from `ready`; `assignee` is the governor's).
- **Messages**, exact openings: `breakdown_message` keeps today's text, which opens "Break the epic <id> down", drops its last sentence, "Assign each once it is ready.", and adds, with one active builder or more: "The team has no active Architect, so plan its lanes yourself: give each task its `lane`, lanes 1 to <builders>, one per active builder. A lane is the tasks one builder does in order; tasks in different lanes run side by side, so their `allowed_paths` must not meet."; `plan_task_message` opens "Plan the epic <id> with the Architect"; `plan_tasks_message` opens "The Architect's plan for the epic <id> is on the integration branch at <path>", the plan's text in an untrusted block cut at `RESULTS_CAP_BYTES` (`messages.rs:25`), then the epic's tasks so far. `close_out_message` is unchanged.

## File map

```
docs/schemas/task-contract.schema.json                  modifies: plan, lane, the description (Task 1)
crates/core/src/folders.rs                              modifies: plan_path (Task 1)
crates/core/src/contract.rs                             tests (Task 1)
crates/core/src/governor/gates.rs                       modifies: FIELDS_OF_THE_CONTENT (1); lane_builder, lane_taken (4); check_child_creation (5)
crates/core/src/governor/paths.rs                       modifies: could_overlap (Task 2)
crates/core/src/governor/readiness.rs                   modifies: LaneBeyondBuilders, LanesOverlap, ParentState.lanes, paths_within_parent (Task 3)
crates/core/src/governor/plain.rs                       modifies: two sentences (Task 3)
crates/core/src/governor/transition.rs                  modifies: a test's AssignmentInput (Task 4)
crates/runtime/src/transitions.rs                       modifies: parent_state (3); lane_builder, assignment, context (4)
crates/runtime/src/orchestrator/rules.rs                modifies: ready (Task 4)
crates/runtime/src/orchestrator/messages.rs             modifies: plan_message (4); breakdown_message, plan_task_message, plan_tasks_message (5)
crates/runtime/src/orchestrator/requests.rs             modifies: tests (4); in_progress_epic, epic_step (5)
crates/runtime/src/tools/contracts.rs                   tests (Task 5)
crates/roles/roles/architect/{system.md,kit.yaml,skills/planning-in-lanes/SKILL.md}, crates/roles/src/kit.rs   modifies, creates the skill (Task 6)
crates/roles/roles/product_manager/{system.md,role.yaml,skills/writing-task-contracts/SKILL.md}   modifies (Task 6)
crates/roles/roles/scrum_master/{system.md,role.yaml,skills/keeping-work-flowing/SKILL.md}, crates/roles/src/lib.rs   modifies (Task 6)
docs/SPEC.md, docs/design/catervas-folders.md          modifies (Task 7)
```

## Interfaces

Consumes: `catervas_core::folders` (step 01, not yet executed), to which `plan_path` is added; `FolderOwned` (step 01), which keeps others' tasks out of `docs/catervas/architecture/`; `no_task_no_write` (step 01b), which is why the plan is a task; `expand_braces`, `is_a_wildcard` (`paths.rs`); `ParentState`, `ReadinessContext` (`readiness.rs:109`, `:120`); `AssignmentInput`, `check_assignment`, `ParentEpic`, `check_child_creation` (`gates.rs:82`, `:242`, `:663`, `:677`); `changes_code`; `parent_state`, `private_folder_busy`, `assignment` (`transitions.rs:783`, `:1234`, `:1175`); `Git::file_at`, `integration_branch`; `product_manager`, `in_progress_epic`, `last_rejection` (`requests.rs`); `untrusted_block` (`crates/runtime/src/prompt.rs:257`); on main unless named.

Produces:

```rust
// catervas_core::folders
pub fn plan_path(epic: &str) -> String;                     // docs/catervas/architecture/plans/<epic>.md
// catervas_core::governor::paths
pub fn could_overlap(a: &str, b: &str) -> bool;
// catervas_core::governor::readiness
pub struct LanedTask { pub task_id: String, pub lane: u64, pub allowed_paths: Vec<String> }
// ParentState gains `pub lanes: Vec<LanedTask>`; ReadinessRule gains LaneBeyondBuilders, LanesOverlap (after BudgetWithinParent)
// catervas_core::governor::gates::AssignmentInput gains `pub lane_builder: Option<String>`
// catervas_runtime::transitions
pub(crate) fn lane_builder(files: &ProjectFiles, team: &Team, board: &[TaskProjection], row: &TaskProjection, contract: &TaskContract) -> Result<Option<String>, TransitionError>;
// catervas_runtime::orchestrator::messages
pub(super) fn plan_message(contract: &TaskContract, assignees: &[String], reviewers: &[String], lane_builder: Option<&str>) -> String;
pub(super) fn breakdown_message(contract: &TaskContract, builders: usize) -> String;
pub(super) fn plan_task_message(contract: &TaskContract, plan: &str, builders: usize) -> String;
pub(super) fn plan_tasks_message(contract: &TaskContract, plan: &str, text: &str, tasks: &[(String, String, String)]) -> String;
// catervas_runtime::orchestrator::requests (private)
enum EpicStep { Nothing, BreakDown, WriteThePlansTasks, CloseOut }
struct Child { status: TaskStatus, awaiting_integration: bool, plan_task: bool }   // plan_task: `allowed_paths` exactly `[plan]`
fn epic_step(children: &[Child]) -> EpicStep;
```

## Tasks

### Task 1: A contract's `plan` and `lane`

Files: the schema (two properties after `ui_change`; the top description's "written by the epic's assignee" becomes "written by the Product Manager, from the Architect's plan when the team has one, or at close-out by the epic's assignee"); `gates.rs` (`FIELDS_OF_THE_CONTENT`, 18); `folders.rs` (`plan_path`, a test); `contract.rs` (a test).

- `reads_a_plan_and_a_lane` (`contract.rs`) — `validate_contract` of a contract with `plan: docs/catervas/architecture/plans/CTV-3.md` and `lane: 2` reads both back; `lane` 0, 17 and `"2"`, and `plan` `docs/catervas/architecture/CTV-3.md`, `docs/catervas/architecture/plans/CTV-3.md/x` and `docs/catervas/architecture/plans/T-3.md`, are each refused with an error whose path names the field. RED: `additionalProperties: false` refuses both.
- `gives_every_field_of_the_schema_to_exactly_one_owner` (`gates.rs:2696`) holds with both in `FIELDS_OF_THE_CONTENT`. RED: they are in no set.
- `the_product_manager_writes_a_plan_and_a_lane` (`gates.rs`) — `check_contract_write` allows the Product Manager's change of `plan` and `lane` on a refining task and refuses it on a ready one as frozen. RED: unknown fields.
- `names_an_epics_plan` (`folders.rs`) — `plan_path("CTV-3")` is `docs/catervas/architecture/plans/CTV-3.md`. RED: no function.

- [ ] `feat(core): add a contract's plan and lane`

### Task 2: Two globs that could meet

Files: `paths.rs` (`could_overlap` with its `ponytail:` comment; a test).

- `two_globs_meet_unless_their_literal_prefixes_part` — each pair asserted both ways. Meet: `src/**` and `src/a.rs`; `src/*.rs` and `src/*.ts` (the ceiling); `**/*.md` and `apps/x`; `Src/a/**` and `src/a/b.rs`; `src/{a,b}/**` and `src/b/x`; `src` and `src/a.rs`; `src/a.rs` and `src/a.rs`; `a/../b/**` and `c/x`; the empty glob and `x`; `./src/**` and `src/a.rs`; `src\a\**` and `src/a/b.rs`. Part: `apps/web/**` and `crates/**`; `src/a/**` and `src/b/**`; `src/a.rs` and `src/b.rs`; `src/{a,b}/**` and `src/c/x`; `./src/a/**` and `src\b\x`; `docs/x.md` and `docs/y.md`. RED: no function.

- [ ] `feat(core): tell whether two globs could meet`

### Task 3: Readiness keeps an epic's lanes apart

Files: `readiness.rs` (`LanedTask`, `ParentState.lanes`, two rules after `BudgetWithinParent` so `CHECKS` is 24, `paths_within_parent`; each `ParentState` literal in its tests gains `lanes: vec![]`; `readiness/fixtures.rs` builds none and is unchanged), `plain.rs` (`EVERY_RULE` 26, `listed`), `transitions.rs` (`parent_state` fills `lanes` in its sibling loop, `:802-812`; a test).
Messages, exact: `lane <n> is beyond the team's builders: it has <b> active, Software Developers and UI/UX Designers, one lane each`; `lane <n> could meet <clauses joined by ", ">; tasks in different lanes are built side by side, so their allowed paths must part before their first wildcard: narrow the paths, or put the tasks in one lane`, a clause per other-lane task with a meeting pair, in `lanes` order, `<id> in lane <m> (<ours> and <theirs>)`, the first pair by our paths' order, then theirs. Plain words: "The plan splits the work into more parallel parts than your team has builders." and "Two parts of this work that run side by side could change the same files."

- `refuses_a_lane_beyond_the_builders` — in `a_ready_context()` (one Developer), a task under `an_in_progress_parent()` with lane 2 fails `[LaneBeyondBuilders]` with exactly `lane 2 is beyond the team's builders: it has 1 active, Software Developers and UI/UX Designers, one lane each`; with a UI/UX Designer added it passes, lane 1 passes, and with no Developer lane 1 fails; a lane-2 task with no parent passes; an epic with lane 3 does not fail it. RED: no rule.
- `refuses_lanes_whose_paths_could_meet` — three Developers; a task of CTV-3, lane 2, `[src/a/**, src/c.rs]`, under `an_in_progress_parent()` (`readiness.rs:1005`, `[src/**]`) whose `lanes` hold CTV-7 lane 1 `[src/**]`, CTV-8 lane 2 `[src/a/b.rs]` and CTV-9 lane 3 `[apps/**]`, fails `[LanesOverlap]` alone, exactly `lane 2 could meet CTV-7 in lane 1 (src/a/** and src/**); tasks in different lanes …` (the message above); without CTV-7 it passes; a task with no lane passes. RED: no rule.
- `keeps_an_epics_plan_inside_it` — parent allowed `[src/**]`: a task of parent CTV-3 with `plan` and `allowed_paths` `docs/catervas/architecture/plans/CTV-3.md` passes `PathsWithinParent`; with `plan` and `allowed_paths` both CTV-4's path, or with no `plan`, it fails it. RED: it fails.
- `plain_readiness_covers_every_rule` (`plain.rs`) holds with 26. RED: `listed` does not compile.
- `reads_the_lanes_of_an_epics_other_tasks` (`transitions.rs`) — epic CTV-1 in progress, CTV-2 lane 1 `[src/**]`, CTV-3 no lane, CTV-4 lane 2 cancelled; the context for CTV-5's `refining -> ready` has `parent.lanes` exactly one `LanedTask` for CTV-2, lane 1, `[src/**]`, and with CTV-5 lane 2 `[src/x.rs]` its readiness fails `LanesOverlap`. RED: no field.

- [ ] `feat(core): keep an epic's lanes apart at readiness`

### Task 4: Each lane goes to one builder

Files: `gates.rs` (`lane_builder` field and its doc; in `check_assignment`, after `without_the_folder`, the reason `lane_taken: lane <n> of <epic> is <builder>'s, who builds its tasks in order; assign <task> to <builder>` when it names another agent, ids compared trimmed; `an_assignment` at `:1181`); `transition.rs:763`; `transitions.rs` (`lane_builder` beside `private_folder_busy`; `assignment` sets `None`, `context` fills it at `:534`); `rules.rs` (`ready` keeps only the builder among `assignees`, and passes it to `plan_message`); `messages.rs` (`plan_message`); a test in `requests.rs`.
`plan_message` adds, for a task with a lane: "It is in lane <n> of <epic>, which <builder> builds." or "It is the first of lane <n> of <epic> to be assigned; whoever you assign builds that lane's other tasks too."

- `gives_a_lane_to_its_builder` (`gates.rs`) — a lane-2 task CTV-5 of CTV-3 with `lane_builder` `dev-a`, assigned to `dev-b`, fails with exactly `lane_taken: lane 2 of CTV-3 is dev-a's, who builds its tasks in order; assign CTV-5 to dev-a`; to `dev-a` it passes; with `None` either passes. RED: no field.
- `finds_the_builder_of_a_lane` (`transitions.rs`) — under CTV-1: CTV-2 lane 1 accepted by `dev-a`, CTV-3 lane 2 in progress by `dev-b`, CTV-4 lane 1 ready: `dev-a`; CTV-5 lane 1 in progress by `dev-b` as well: `dev-b`; CTV-2 cancelled instead: `None`; `dev-a` paused: `None`; CTV-2 a UI/UX Designer's task: `None`; CTV-6 lane 1 under another epic, `dev-b`'s, changes nothing; CTV-4 with no lane: `None`. RED: no function.
- `offers_a_lanes_task_to_its_builder_alone` (`requests.rs`, integration) — `pm`'s epic CTV-1 in progress; CTV-2 lane 1 accepted by `dev-a`; CTV-3 lane 1 ready: `pm`'s plan session for CTV-3 has a first message naming `dev-a` and not `dev-b` among those with room, and holding `lane 1 of CTV-1, which dev-a builds`; with `dev-a` paused it names `dev-b` and `first of lane 1`. RED: both are named.
- `names_a_lanes_builder` (`messages.rs`) — `plan_message` for a task with no lane is unchanged; with lane 2 of CTV-1 it holds each sentence above for `Some("dev-a")` and `None`. RED: no parameter.

- [ ] `feat(runtime): give each lane of an epic to one builder`

### Task 5: The Product Manager plans an epic with the Architect

Files: `requests.rs` (`Child`, `EpicStep`, `epic_step`; `in_progress_epic` runs the Product Manager's `plan` session for `BreakDown` and `WriteThePlansTasks` and the assignee's close-out, rejection or not, as today, for `CloseOut`; every session still waits while the epic's assignee is not active, as today, and the Product Manager's while none is active; tests); `messages.rs` (the three messages and their tests); `gates.rs` (`check_child_creation` admits `TransitionActor::ProductManager`; its doc; the test at `:2472`, renamed `writes_a_task_under_an_epic_as_its_assignee_the_product_manager_or_the_human`, its refusals now `a task under an epic is written by the epic's assignee, sm-1, by the Product Manager, or by the human`); `tools/contracts.rs` (`:1093`); the comments that say an epic "the Product Manager broke down" (`requests.rs:42`, `gates.rs:289`, `:1350`, `transitions.rs:1188`) say one it "holds".

- `reads_an_epics_next_step` (`requests.rs`) — none and all cancelled: `BreakDown`; one in progress: `Nothing`; an accepted plan task awaiting integration: `Nothing`, integrated: `WriteThePlansTasks`, and so with a cancelled task beside it; it and a built task accepted: `CloseOut`; a built task with no lane, accepted: `CloseOut`; an accepted task that is no plan task: `CloseOut`. RED: no function.
- `files_the_plan_task_with_the_architect` (integration) — a team with `sam` and an Architect, `sam`'s epic CTV-1 in progress with no task: one `plan` session, `pm`'s, about CTV-1, whose first message starts "Plan the epic CTV-1 with the Architect" and holds `docs/catervas/architecture/plans/CTV-1.md` and `architect`. RED: `sam`'s breakdown.
- `breaks_an_epic_down_with_its_scrum_master` (`:2595`) becomes `breaks_an_epic_down_in_lanes_without_an_architect`: the session is `pm`'s, its message starts "Break the epic CTV-1 down" and holds "lanes 1 to 2". RED: `sam`'s.
- `writes_the_plans_tasks_once_the_plan_is_integrated` (integration) — Architect team; CTV-2 under CTV-1, a plan task (`plan` and `allowed_paths` both `docs/catervas/architecture/plans/CTV-1.md`), accepted: no session about CTV-1 on the first tick; then the plan committed at the root with `## Lane 1` and CTV-2 recorded integrated: `pm`'s `plan` session whose message holds the plan's text inside an untrusted block, CTV-2, and `` `lane` ``. RED: the epic is closed out.
- `breaks_down_again_when_the_plan_is_missing` — as above, integrated with no plan file: the message starts "Plan the epic CTV-1 with the Architect". RED: closed out.
- `closes_an_epic_whose_tasks_are_done` (`:2161`), `sends_a_failed_epic_back_for_more_work` (`:2352`) and `passes_over_an_epic_whose_assignee_is_paused` (`:2410`) hold unchanged.
- `files_a_child_of_an_epic_its_assignee_breaks_down` (`contracts.rs:1093`) gains: `pm` files a child of the Scrum Master's epic. RED: refused.
- `apps/web/e2e/sprints.spec.ts` holds: its team is `pm-architect-developer`, so the breakdown session is `pm`'s and its first message is now `plan_task_message`, and the recorded transcript files a Developer's task anyway, which is no plan task and carries no lane.

- [ ] `feat(runtime): plan an approved epic with the Architect before its tasks`

### Task 6: The roles are told the order

Files and text, exactly where quoted; step 01's, 01b's and 02's lines are kept.
- `architect/skills/planning-in-lanes/SKILL.md` (new): front matter `name: planning-in-lanes`, `description: Use when a task asks you to write an approved epic's implementation plan in docs/catervas/architecture/plans/.`; sections: the plan is for agents, and the Product Manager writes one task per planned task from it; at most as many lanes as the builders the contract names, a lane being one builder's tasks in order, all of one role (`software_developer` or `ui_ux_designer`), one builder one lane; lanes' paths part before their first wildcard (`apps/web/**` and `crates/**`, never `src/*.rs` and `src/*.ts`), or `lanes_overlap` refuses them; the outline `# Plan for <epic-id>: <title>`, `## Lane <n>`, then `### <n>.<k> <title>` with role, what it builds, `allowed_paths`, depends on, constraints; commit with `catervas_git_commit` and request `verifying`. `kit.yaml` lists it last, its comment "six skills"; `kit.rs` embeds it.
- `architect/system.md`, "What you produce", after step 01's folder line: "- An approved epic's implementation plan, `docs/catervas/architecture/plans/<epic-id>.md`, written for agents in lanes, in the task the Product Manager files for it (`planning-in-lanes`)."
- `product_manager/system.md`: the mandate's last paragraph becomes "You write the tasks of every approved epic: first the Architect's plan task, then one task per planned task once the plan is on the integration branch, or, with no active Architect, the tasks in lanes yourself. When the team has no active Scrum Master, you also triage requests and assign tasks to agents."; "How a session ends" item 2's "(an epic you are breaking down)" becomes "(an epic you hold)". `role.yaml`: the mandate's last sentence becomes "It writes every approved epic's tasks, from the Architect's plan when the team has an Architect. When the team has no active Scrum Master, it also triages requests and assigns tasks."; `produces` gains "task contracts under an approved epic, from the Architect's plan".
- `writing-task-contracts`: the description gains ", or an approved epic needs its tasks"; §6's breakdown sentence ("When you break an approved epic down, …") is removed; a new "## 7. An approved epic's tasks" says, as numbered steps: with an active Architect, file one task for it with `catervas_create_task`, `parent` the epic, `assignee_role` `architect`, `plan` and `allowed_paths` `docs/catervas/architecture/plans/<epic-id>.md`, no `lane`, an `artifact` criterion that the plan exists, and in its requirements how many builders (Software Developers and UI/UX Designers) are active; once the plan is on the integration branch, one task per planned task, with its `plan`, its `lane`, the planned tasks it needs in `dependencies`, the Architect's paths as `allowed_paths` and its constraints as `constraints`, acceptance criteria from the epic's requirements and the spec; with no Architect, the lanes yourself, each task with its `lane` and no `plan`; and that the governor refuses lanes that could meet (`lanes_overlap`) and a lane past the builders (`lane_beyond_builders`).
- `scrum_master/system.md`: the mandate's "You break an approved epic down into tasks with clear deliverables and exit criteria of their own, and assign the ready ones to agents with room under the team's WIP limit." becomes "You do not break an approved epic down: the Architect plans it in lanes and the Product Manager writes its tasks. You assign each ready task to an agent with room under the team's WIP limit, every task of a lane to that lane's builder."; "What you produce"'s task-contract line becomes "- Assignments, through `catervas_assign_task`, and the tasks an epic's close-out asks for, through `catervas_create_task`."; item 2's "(an epic you are breaking down)" becomes "(an epic you are closing out)"; item 3's "a breakdown you finished or an epic you closed out. Assign the tasks you filed" becomes "a task you assigned or an epic you closed out. Assign".
- `scrum_master/role.yaml`: the mandate's "Break an approved epic into tasks with clear deliverables and exit criteria, and assign the ready ones within the team's WIP limit." becomes "Assign each ready task within the team's WIP limit, every task of a lane to one builder."; `produces`' "task contracts under an epic it is breaking down" becomes "assignments, a lane's tasks to one builder".
- `keeping-work-flowing`: the description drops "an approved epic needs breaking down into tasks, "; the opening drops "break epics into tasks a Developer or an Architect can actually finish, "; §3's heading becomes "## 3. Tasks at an epic's close-out" and its first sentence "When you close out an epic whose reviewer rejected it, or whose human's message asks for more, file the tasks that fix it with `catervas_create_task`, `parent` set to the epic, each contract complete in one call:", its other lines (step 01's folder sentence among them) kept; §4 gains "A task with a `lane` goes to its lane's builder, named in your message: the governor refuses anyone else (`lane_taken`). The first task of a lane goes to any agent of its role with room, who then builds the lane."; 01b's sentence there is kept.

- `planning_follows_the_architects_plan` (`lib.rs`) — the Architect's prompt holds "An approved epic's implementation plan" and `planning-in-lanes`; the Product Manager's prompt holds "You write the tasks of every approved epic" and not "break approved epics into", and `writing-task-contracts` does not hold "When you break an approved epic"; `writing-task-contracts` holds "## 7. An approved epic's tasks", `lanes_overlap` and `lane_beyond_builders`; the Scrum Master's prompt holds "You do not break an approved epic down" and not "You break an approved epic down"; `keeping-work-flowing` holds `lane_taken` and not "Break an approved epic into tasks". RED: the old text.
- `architect_kit_carries_its_skills` (`kit.rs:2331`) lists six, `planning-in-lanes` last. RED: five.
- `the_lanes_skill_names_the_plan_and_its_limits` (`kit.rs`) — `planning-in-lanes` holds `docs/catervas/architecture/plans/`, `## Lane`, `allowed_paths`, `builders`, `lanes_overlap` and `catervas_git_commit`, is under 6 KiB, and has no " @". RED: no skill.
- `the_planners_keep_other_tasks_off_the_owners_folders` (step 01) and `kit_skills_name_only_tools_catervas_lists` (`daemon/team.rs:5354`) hold unchanged.

- [ ] `feat(roles): plan an epic Architect first, then contracts, then the schedule`

### Task 7: Spec

`docs/SPEC.md`: 3 (Contract: `plan` and `lane`; Epic: the Architect plans it before the Product Manager writes its tasks; Planning in sprints: an epic's breakdown is preparation, its plan task a task that waits with it; `catervas plan`'s breakdown stops at the plan task); 5.2 (the assignment row: `lane_taken` and the lane's builder); 5.3 (bullets `lane_beyond_builders` and `lanes_overlap`, their messages, plain words and the overlap test with its ceiling); 5.13 (an epic's task contracts come from the Architect's plan, by the Product Manager); 5.16 item 3 (a task's `allowed_paths` within its epic's, its own plan's path aside; the order: the Product Manager's breakdown files the plan task, then the plan's tasks; the Architect writes the plan in its docs task; the assignee assigns and closes out; with no Architect, the Product Manager's lanes; who files an epic's tasks); 6.1, 6.2 (Mandate, Produces, Default tools: no breakdown), 6.3 (Produces: implementation plans; Kit: six skills, `planning-in-lanes`); 5.4 ("one the Scrum Master broke down" and "The Scrum Master's breakdown tasks" say the epic the Scrum Master holds and its tasks); 5.16 item 4 ("when the Scrum Master broke it down" becomes "when the Scrum Master holds it"); the next spec revision: "records phase 8 step 05, plans with lanes". `docs/design/catervas-folders.md`: "With no Architect the Product Manager writes the plan" becomes "With no Architect the Product Manager plans the lanes in its tasks, with no plan document".

- [ ] `docs(spec): record plans with lanes`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Tasks 3 to 5 name integration tests
# expected: xtask check: ok
git grep -n "Break an approved epic into tasks\|You break an approved epic down" -- crates
# expected: no output
```
