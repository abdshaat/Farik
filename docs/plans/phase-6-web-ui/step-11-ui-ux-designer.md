# Phase 6, step 11: The UI/UX Designer and its plan gate

Status: in progress (Tasks 1 to 6 landed 2026-09-30; the landing review is next)
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3, 4.1, 5.1 to 5.4, 5.6 (the Designer's tiers), 5.12 (`document_paths`), 6 (6.1 to 6.5, a new 6.8), 8.2 (the `explore` session), 8.5, F1. The same list is in the project plan's step 11 row and the design's placement table.
Depends on: steps 01 to 10 of this phase (landed); ADR 0026 and `docs/design/designer-chats-templates.md` (accepted 2026-09-30), whose decisions are binding and not restated here
Readiness confirmed by: fresh-session reviewer, 2026-09-30; round one not ready; round two ready with findings, folded in

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A team can have a sixth suggested agent, the UI/UX Designer. It changes code on its own tasks, as the Developer does, but first it explores the app, writes a plan, and waits for the Product Manager's approval. The governor enforces that order. The Architect reviews its work. The user sees it in setup and on the Team page, and reads its plan and the Product Manager's decision on the task's page.

This step also draws every mockup of the Designer, including step 12's screens, so that the founder approves them once.

Out of scope, and step 12's: the preview, the Playwright connector, `farik_check_page`, and the Designer's design review of a Developer's UI change. Until step 12 lands, an `explore` session reads the project's files and has no browser. The branch says so honestly: the explore message says "no browser yet", and step 12 removes that line.

Also out of scope: chats (step 13) and templates (step 14).

## Decisions

The ADR's and the design's decisions hold as written. This plan decides only what they left open.

- **The founder's gate (standing rule of 2026-09-26).** Task 1's mockups are approved by the founder before Task 2 starts, here and in step 12. The approval is recorded in this section in Task 1's commit, with the character, tag colour and name the founder chose.
- **Approved by the founder, 2026-09-30:** the seven boards on the canvas's "UI/UX Designer" page (DesignerTeam, SettingsPreview, DesignPlan, GateDesignReview, and the phone boards for the last three). Choices: picture A (`extra-1`), tag colour C, pale clay `#CDB29C` (9.00:1 with Midnight Terminal text), name Iris with the line "Makes it clear, calm and easy to use", tag "UX". The boards sit on their own canvas page, `designer`, rather than on the team, settings, daily and gates pages. A choice other than the defaults below changes only data (`role.yaml`, `tokens.json`, the avatar key), never a task.
- **Name and persona**, the defaults the mockup offers: Iris, "Makes it clear, calm and easy to use". Its id is `iris`. The pool of extra names (Noor, Ivo, Lena, Sami, Rui) stays for extra agents.
- **Character options:** the existing `extra-1` to `extra-5` (`docs/brand/assets/characters/`). No new art. Once the founder chooses one, extra agents' avatars are drawn from the other four extras, so no two agents look alike (F10).
- **Tag colour options:** dusty teal `#9EC3BE`, soft peach `#E3B89A`, pale clay `#CDB29C`. Each is muted, light, distinct from the five role colours, and passes the contrast test against `role-ink` #161616. The chosen one becomes the token `role-ui-ux-designer` in both themes.
- **The plan gate is read from the log at each call.** The gate is the latest `design_plan.*` event of the session's task. The hook and `call_tool` both judge it, and refuse the Designer's `write_workspace`, `execute` and `git_local` calls with `design_plan_not_approved`. Rejected: fixing the gate at session start, because a plan proposed after the session started must close it.
- **The tools' tiers.** `farik_propose_design_plan` and `farik_decide_design_plan` are `read`: they write only the log. Each also checks its session's purpose and role, and is refused with `design_plan_refused` outside the session that gives it.
- **The explore session in this step:** the read tier's built-ins in the task's worktree, and five Farik tools: `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_criteria` and `farik_read_decisions`. Plus `farik_propose_design_plan`. Step 12 adds the connector and `farik_check_page`.
- **A session that ends without its one answer** (D6). An `explore` session that ends with no `farik_propose_design_plan` is started again, and so is a Product Manager's decision session that ends with no `farik_decide_design_plan`. This is what `verify.rs` does for a reviewer who wrote no note. Each restart counts toward the contract's sessions allowance, which escalates with `sessions` as today.
- **Without Docker's sandbox** (the founder, 2026-09-30, D3), the Designer is unavailable. Everything about that is step 12's, since the browser is: the unticked "Needs Docker's sandbox" row in `team.propose` (tested there by `proposes_the_designer_with_its_connector`) and the assignment refusal. Step 11 draws the row in its mockups only. Until step 12 lands, an explore session needs no Docker.
- **The mockups are new files**, so the approved ones stay untouched: `DesignerTeam`, `SettingsPreview`, `DesignPlan` and `GateDesignReview`. They are registered in `canvas.json` under the pages `team`, `settings`, `daily` and `gates`.

## File map

```
docs/design/mockups/{DesignerTeam,SettingsPreview,DesignPlan,GateDesignReview}.dc.html, canvas.json   creates / modifies (T1)
docs/schemas/{team,task-contract,role,event}.schema.json                  modifies (T2): the role, the design_plan events
crates/core/src/{team.rs,team/defaults.rs,team/describe.rs,branch.rs}     modifies (T2)
crates/core/src/governor/{permissions.rs,readiness.rs}                    modifies (T2), + tests
crates/protocol/src/event.rs                                              modifies (T2)
crates/roles/roles/ui_ux_designer/{role.yaml,system.md,skills/*/SKILL.md} creates (T3)
crates/roles/roles/*/system.md, crates/roles/src/{lib.rs,reviewer.rs}      modifies (T3)
crates/runtime/src/daemon/team.rs                                         modifies (T3 propose)
crates/runtime/src/daemon/web.rs                                          modifies (T4): `task.get`'s `design_plan`
packages/brand/tokens/tokens.json, packages/brand/assets/avatars/          modifies (T3)
crates/runtime/src/{session.rs,tools.rs,tools/design.rs,daemon/hooks.rs}   modifies / creates (T4)
crates/runtime/src/orchestrator/{rules.rs,design.rs,messages.rs}           modifies / creates (T4)
crates/runtime/src/recorded/{fixtures.rs,transcripts/*.jsonl}              modifies / creates (T4)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/*  modifies (T4)
apps/web/src/pages/setup/{SetupTeam,TeamSetup}.tsx, pages/{Team,TaskDetail}.tsx, strings/en.ts (+ tests)  modifies (T5)
docs/SPEC.md, docs/plans/project-plan.md                                  modifies (T6)
```

## Interfaces

Consumes: `Team`, `Agent::tiers`, `default_tiers`, `evaluate_tool_call`, `evaluate_readiness`, `task_branch` (`farik-core`, on this branch); `default_reviewer_role` (`farik-roles`); `SessionRegistration`, `ToolContext`, `SessionAsk`, `run_session`, `RecordedAdapter::with_tools` (`farik-runtime`).

Produces:

```rust
// farik-core
Role::UiUxDesigner; RoleWire::UiUxDesigner;  plain_role(Role::UiUxDesigner) == "UI/UX Designer"
pub fn changes_code(role: Role) -> bool;                       // SoftwareDeveloper | UiUxDesigner
ToolRefusal::DesignPlanNotApproved;  pub fn check_design_plan(role: Role, tier: PermissionTier, approved: bool) -> Result<(), ToolRefusal>;
// farik-runtime
SessionPurpose::Explore
```

Wire:
- events: `design_plan.proposed { plan }`, `design_plan.approved { reason }`, `design_plan.returned { reason }`;
- tools: `farik_propose_design_plan { plan }`, `farik_decide_design_plan { approve, reason }`;
- RPC: `task.get` gains `design_plan: { plan, state: proposed|approved|returned, reason? } | null`; `team.propose` answers six.

As built (recorded 2026-09-30 by Task 6 from the reports of Tasks 2 to 5; spec 0.31 describes this, not the first plan's guesses):
- Core: `TransitionContext` gains `design_plan_returns: u32`; the governor's `any -> escalated` row answers `iterations` when the returns reach `max_iterations` plus extra tries, after the budgets and before a denied permission. `check_design_plan` also withholds `git_remote` (the ledger's ruling).
- Schemas: `design_plan.approved` and `.returned` share `designPlanDecidedBody { reason }`; `explore` joins `session.started`'s and `cost.recorded`'s purpose enums and `team.activity`'s; metrics count eight purposes; the board words `explore` as "Planning".
- The Designer's picture `extra-1` lives in `team.propose`'s list (the role schema has no avatar key); added agents take `extra-2`, `extra-3`, `extra-5`, never `extra-1` or `extra-4`.
- The decision tool refuses a blank reason and a task with no plan waiting; a Designer's task waits while the team has no active Product Manager; an explore session has the read tier's built-ins and no executor whatever the agent's tiers; a Designer's session about no task is held as not approved.
- Web: faces ringed in their role's colour; the Team page's Designer cost line links to Costs; the task page's plan tab, status card and heading word read `task.get`'s `design_plan`, the returns shown against `tries.of - 1`.
- The first-day sentence (`firstDay`, SPEC 10) covers the suggested six as an estimate, under twenty-five dollars: the old figure scaled by six agents to five, since no derivation of the twenty dollars is recorded (Task 6).
- Not built, parked for the landing review or step 12: the task page's "How Iris works" aside and the setup row's "Checks every screen Theo builds" (step 12, browser); the Team card's "Waiting for Mira to approve a plan"; setup rows show the persona rather than the mockup's role description; copy drifts ("on {day} at {time}", "If {of} are sent back", "Include UI/UX Designer"); a Designer's task passed over while the Product Manager is paused says nothing on the board.

## Tasks

### Task 1: Mockups, and the founder's approval

Files: created `docs/design/mockups/{DesignerTeam,SettingsPreview,DesignPlan,GateDesignReview}.dc.html`; modified `canvas.json`. In the style of the existing boards: the rail, the palette, Space Grotesk, the 1440 px frame, and a phone frame where a screen has one.
- `DesignerTeam`: the "Your team" row for the Designer, ticked, and its unticked "Needs Docker's sandbox" variant; the Designer's Team card; the five characters and the three tag colours side by side, labelled for the founder to choose; the agent editor's "Connectors" list with the Playwright switch.
- `SettingsPreview`: Settings' "How to open your app" (a prepare command, a start command, a port and a first page), the Designer's setup card asking for it, and the Today waiting row "Tell Farik how to open your app".
- `DesignPlan`: the task page's "The plan", "Waiting for Mira to approve the plan", then approved, then returned with the reason; the board card and task page with "Waiting on the Designer" (paused) and "Checking the screens" (in review).
- `GateDesignReview`: the gate's review letter from the Designer, with its four checks (phone and desktop, light and dark), each with its screenshot and violations, and the Architect's review below it.

**Gate: the founder approves these mockups, with the character, the tag colour and the name, before Task 2 starts.** The approval is recorded in Decisions in this task's commit. Nothing later starts without it, here or in step 12.

- [x] `docs(design): mock up the UI/UX Designer's team, preview, plan and review screens`

### Task 2: The role and its plan gate in core, and the schemas

Produces: the `farik-core` items above. Consumes: nothing new.

Tests, in each module's `mod tests`:
- `changes_code_for_the_developer_and_the_designer_only`: true for those two roles, and false for the other four and the human.
- `gives_the_designer_the_developers_tiers_and_permission_answers`: `read`, `write_workspace`, `execute` and `git_local`. `run_commands: false` removes `execute`, and `push: true` adds `git_remote`, for a Designer as for a Developer.
- `holds_neither_the_developer_nor_the_designer_to_the_document_paths`: a Designer's task with `src/**` passes `DocumentPathsOnly`, and a Marketing Specialist's still fails.
- `puts_a_designers_task_on_a_feature_or_fix_branch`: `task_branch` gives `feature/FRK-n` and `fix/FRK-n`, by `change`.
- `refuses_a_write_before_the_plan_is_approved`: `check_design_plan` refuses a Designer's `write_workspace`, `execute` and `git_local` while the plan is not approved (`design_plan_not_approved`). It allows `read`, allows all three once approved, and allows a Developer anything.
- `reads_the_design_plan_events`: each of the three bodies round-trips through `event.schema.json`.

- [x] `feat(core): add the UI/UX Designer and its plan gate`

### Task 3: The role's content, its reviewer, and the suggested six

Produces: `roles/ui_ux_designer/` and `team.propose`'s six. Consumes: `Role::UiUxDesigner` from Task 2.

Files:
- `role.yaml`: `id: ui_ux_designer`, the mandate, the persona, the Developer's model (Opus 5, `high`), and the chosen avatar key;
- `system.md`, covering explore, plan, approval and implement;
- six skills in the Agent Skills format (`SKILL.md` with `name` and `description` front matter), named as the design's table names them;
- the other roles' `system.md`, where "only the Developer changes code" becomes "only the Developer and the UI/UX Designer";
- `REVIEWER_ROLE_FOR` gains `(UiUxDesigner, [Architect, SoftwareDeveloper])`;
- `team.propose`;
- the tag colour token.

Tests:
- `ships_the_designer_with_its_six_skills`: the role loads, and the skill names equal the design's six.
- `says_who_changes_code_in_every_prompt`: no role's `system.md` contains "only the Developer changes code" or "Only the Software Developer", and the Developer's and the Designer's both contain the new sentence.
- `sends_a_designers_task_to_the_architect_then_a_developer`: `default_reviewer_role` for a Designer's task gives the Architect, and a Developer when there is no active Architect.
- `proposes_the_suggested_six`: the ids, the roles, and the order (PM, SM, Architect, Developer, Designer, Marketing).
- `contrast.test.ts`: the existing test gains a pair, `role-ui-ux-designer` against `role-ink`, which passes AA in both themes.

- [x] `feat(roles): add the UI/UX Designer's role, skills and reviewer, and suggest six`

### Task 4: The Designer's flow: explore, plan, approval, implement

Produces: `SessionPurpose::Explore`, the two tools, the flow and `task.get.design_plan`. Consumes: Tasks 2 and 3.

Files: `orchestrator/design.rs`; `rules.rs` (the `in_progress` branch for a Designer's task); `messages.rs`; `tools/design.rs`; the hook; `daemon/web.rs` (`task.get`); the RPC schema and client.

The latest `design_plan.*` event decides the session:
- none, or `returned`: the Designer's `explore` session, with the tools in Decisions;
- `proposed`: the Product Manager's `verify` session, with only `farik_decide_design_plan`;
- `approved`: `implement`, with the plan in its message.

When the number of returns reaches the contract's `max_iterations` plus any extra tries, the task escalates with `iterations`.

The recorded transcripts are `explore_plans_frk_1`, `decide_design_plan_approves_frk_1`, `decide_design_plan_returns_frk_1` and `implement_by_iris_frk_1`.

Tests:
- `explores_first_then_asks_the_product_manager`: the order and purposes of the sessions, and the explore session's tools as listed.
- `implements_only_after_approval`: after `approves`, an `implement` session with the plan in its message.
- `explores_again_with_the_returned_reason`: the reason is in the new message.
- `escalates_after_too_many_returns`: at 3 returns, the task is `escalated` with `iterations`.
- `refuses_the_plan_outside_explore`: `design_plan_refused` in `implement` and for a Developer; the plan's length and summary bounds are each refused with their sentence.
- `refuses_the_decision_but_from_the_product_manager`: `design_plan_refused` for the Architect, and for the Product Manager outside a `verify` session about the task.
- `refuses_a_designer_write_before_approval` (hook and `call_tool`): `Edit` and `farik_exec` are denied with `design_plan_not_approved`.
- `starts_again_without_an_answer`: an explore session with no plan is followed by a new explore session, and a decision session with no decision by a new decision session; at the sessions allowance, the task escalates with `sessions`.
- `sends_the_designers_work_to_the_architect`: after the completion, the reviewer's session is the Architect's.
- `answers_the_task_with_its_plan`: `task.get`'s `design_plan`, in each state.

- [x] `feat(runtime): run the Designer's explore, plan, approval and implement flow`

### Task 5: The Designer in setup and on the Team page, and the plan on the task page

Produces: the page changes. Consumes: `team.propose`'s six and `task.get.design_plan` from Tasks 3 and 4.

Tests, each also running axe:
- `offers_the_designer_among_the_six`: six rows with the Designer ticked; unticking it saves five.
- `shows_the_designer_card_in_its_colour`: the card's role tag uses `--role-ui-ux-designer`, with the chosen avatar.
- `shows_the_plan_waiting_for_the_product_manager`, then approved, then returned with its reason.

- [x] `feat(web): add the Designer to setup and the Team page, and show its plan`

### Task 6: Spec and plan

`docs/SPEC.md`, from the design's list, in the parts this step builds:
- 3: the agent and the role;
- 4.1: the suggested six;
- 5.1 and 5.2: the plan gate's text;
- 5.3 and 5.12: `document_paths` for two roles;
- 5.6: the Designer's tiers and the team's permission answers;
- 5.4: the Designer's reviewer;
- 6.1 to 6.5: "only the Developer and the UI/UX Designer";
- a new 6.8, the Designer;
- 8.2: the `explore` session;
- 8.5: the three events;
- F1.

The header gains "Revision 0.31 (<date>) …", from phase 6 step 11, in the form of 0.29 and 0.30.

`docs/plans/project-plan.md`: step 11's line gains "Built <date> (spec 0.31): …" with the founder's choices.

- [x] `docs(spec): the UI/UX Designer and the Product Manager's plan gate`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T2 6 new, T3 4, T4 10);
#   @farik/brand: the count at this step's start (one pair added to an existing test);
#   @farik/web: the count at this step's start plus 3 (T5);
#   playwright: the count at this step's start, unchanged;
#   last line: xtask check: ok
# landed 2026-09-30 (Task 6): cargo 1630 passed, 0 failed (step start 1610, plus T2 6, T3 4, T4 10);
#   protocol-client 8, brand 30 (unchanged), ui 43 (step start 42, plus T3's RoleTag test),
#   @farik/web 133 (step start 129, plus T5's 3 and its Board test); playwright 9 passed; xtask check: ok
# landing fix wave 2026-09-30: cargo 1637 passed, 0 failed (+7); protocol-client 8, brand 30, ui 43,
#   @farik/web 133 (tests strengthened, none added); playwright 10 passed (+design.spec.ts); xtask check: ok
```
