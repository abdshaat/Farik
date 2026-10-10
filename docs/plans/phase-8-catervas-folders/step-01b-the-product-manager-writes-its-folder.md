# Phase 8, step 01b: The Product Manager writes its folder

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.1, 5.6, 5.17, 6.1, 8.2; F5, F6
Depends on: step 01 of this phase (planned, not yet executed: `ROLE_FOLDERS`, `FolderOwned`, the Product Manager's folder and its prompt); phase 7 and the Catervas rename (merged on main, 2c28b555); the task id rename to `CTV-<n>` (the founder's decision, a separate pull request that lands on main first)
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): ready; no Blocking; carried into execution findings 1–9, folded below (finding 10, an ADR 0051 amendment, is the controller's).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at ac1a606e; step 01 moves none of the lines cited here except in the Product Manager's role files, which Task 3 names by section.

## Goal

The Product Manager keeps `docs/catervas/product/` in docs tasks of its own, as the Architect and the Marketing Specialist keep their documents: it writes, commits on `docs/CTV-<n>`, and is reviewed. Before it gets the tiers for that, no session that is not an implement session about a task may change the project, for any role: the hook and Catervas's tool server refuse every writing tool there with `no_task_no_write`, and such a session is not offered one. So the Product Manager's refine, plan and triage sessions, which run at the project root, stay read-only. Out of scope: the owner accepting a change to a human document, and `product_plan_first` (06); the folder write tool (03); the Scrum Master's write tiers (none). No UI.

## Decisions

The controller's ruling of 2026-10-10: (a) every session that is not about a task is read-only for every role, whatever its tiers, refused with a code this plan names; (b) the Product Manager's default tiers gain `write_workspace` and `git_local`, as the Architect's and the Marketing Specialist's did (spec 5.6), and its prompt and skills say "docs tasks".

- **What works on a task**: a session whose purpose is `implement` and which has a task, `works_on_a_task(purpose, has_task)`. Every other session changes nothing: `triage`, `refine` (the judge's included), `plan`, `explore`, `verify`, `ceremony`, `conversation`, `chat`, and an `implement` session with no task. A private-folder task's implement session works on a task; its sheet and evaluation tools are `read` tier and unaffected.
- **What changes the project**: `writes_the_project(name, tier)`, true for a tool of tier `write_workspace`, `execute` or `git_remote`, and for `catervas_git_commit`, whose `git_local` tier it shares with `catervas_git_status` and `catervas_git_diff`, reads a reviewer needs. That is the built-ins `Edit`, `Write`, `MultiEdit`, `NotebookEdit` (`builtin_tool_tier`, `crates/runtime/src/daemon/hooks.rs:110`) and the Catervas tools `catervas_exec`, `catervas_git_commit`, `catervas_git_push`, `catervas_propose_marketing_plan` (`crates/runtime/src/tools.rs:393-418`). Catervas's `read`-tier tools that write under `.catervas/` (contracts, notes, decisions, memory, the retro) keep their own rules; a connector stays judged by its tag (5.6).
- **Where it lives: `catervas-runtime`**, `writes_the_project` in `crates/runtime/src/tools.rs` and `works_on_a_task` in `crates/runtime/src/session.rs` beside `SessionPurpose` (`:22`). Rejected: `catervas-core` beside `check_design_plan` (`crates/core/src/governor/permissions.rs:356`), since core names no Catervas tool and this rule needs one.
- **Where it is checked, and in which order**: in the hook's `judge_call` after `plan_gate` (`hooks.rs:380`), and in `Call::permit` after `design_plan_gate` (`tools.rs:686`), so every reason given today stays the first: a Product Manager without the tier still gets `tier_not_granted`, and a Designer's command in a session about no task still gets `design_plan_not_approved` (`hooks.rs:3171`). The refusal is a new `Refusal::NoTaskNoWrite { tool: String }` (`crates/runtime/src/tools/refusal.rs:14`), `tool` being a Catervas tool's name without the `mcp__catervas__` prefix, as `ToolNotInSession` names it, and a built-in's own name; reason exactly `no_task_no_write: <tool> changes the project, and only an implement session working on a task changes it`.
- **Such a session is not offered them.** `offered_tools` drops every Catervas tool `writes_the_project` names unless the session works on a task. That replaces `NOT_FOR_READ_ONLY` (`crates/runtime/src/orchestrator/session.rs:850`, `:867`), whose three tools it covers, since no read-only session is an implement session. `session_spec_without` (`:1002-1008`) gives such a session the built-ins of its tiers without `write_workspace`'s. `works_on_a_task` also replaces the `purpose == SessionPurpose::Implement && contract.is_some()` tests in `offered_tools` (`session.rs:875-915`) and the same test in `how_to_ask` (`hooks.rs:392`), so one function says what working on a task is. The doc comments that state the old behaviour change with it: `decide_pre_tool_use`'s refusal order (`hooks.rs:120-130`, `no_task_no_write` after the plan gate), `judge_call`'s (`:313-314`), and `SessionAsk.read_only`'s (`session.rs:77-79`, which no longer withholds the Catervas tools that write: working on a task does). Rejected: making the Product Manager's planning sessions `read_only`, which would also drop the `WebFetch` and `WebSearch` its refine sessions research with.
- **Nobody writes in such a session today.** The sessions outside `implement` that are not read-only are triage (`crates/runtime/src/orchestrator/requests.rs:89`), refine (`:166`, the judge's, given one Catervas tool and no built-in by `session.rs:1002`; `:195`) and plan (`requests.rs:474`, `orchestrator/rules.rs:1488`). They are run by the Product Manager or the Scrum Master, and neither holds a writing tier today. Ceremonies (`rules.rs:437`, `:563`, `:658`), chats (`:794`), conversations (`:849`), explore (`orchestrator/design.rs:147`) and verify (`design.rs:191`, `orchestrator/pipeline.rs:81`, `orchestrator/verify.rs:669`) are read-only already. The Architect's spikes are implement sessions of its own tasks, and the Designer's preview is prepared and started by Catervas, not by the agent's tools. The tests that call a writing tool outside a task's implement session are, at ac1a606e: `tools/git.rs:454` (its commit and push now get `no_task_no_write`, its status and diff keep `no_task`); the `exec` helper of `tools/exec.rs:127`, which files FRK-1 (CTV-1 after the id rename) and calls on it, its tests' subjects unchanged; and `refuses_another_role_or_session` (`tools/marketing.rs:949`), whose Verify and Chat cases assert `no_task_no_write: catervas_propose_marketing_plan`, while its Developer case and its no-task case (`path_outside_allowed`) are unchanged. Any other test the full check finds follows the same rule: it gains the task its subject needs, or asserts `no_task_no_write`.
- **The Product Manager's tiers** become `read`, `network`, `write_workspace` and `git_local`, the Marketing Specialist's, in one arm of `default_tiers` (`permissions.rs:52-55`). Not `execute`: Catervas runs a docs task's criteria. Not `git_remote`, which nobody holds by default.
- **Its reviewer**: `REVIEWER_ROLE_FOR` (`crates/roles/src/reviewer.rs:11`) gains `(ProductManager, &[Architect, ScrumMaster])`, so `fill_reviewer_role` (`crates/runtime/src/tools/contracts.rs:461`) fills it. Rejected: the human, whom `ReviewerAvailable` admits for an epic alone; the owner's acceptance of a human document's change is step 06's. Until step 06, the Product Manager accepts its own docs task after its reviewer verifies it, as for every task; the reviewer is the independent check. `REVIEWER_ROLE_FOR`'s doc comment (`reviewer.rs:8-10`, "no row until phase 4 decides them") names the new row.
- **Its docs tasks are ordinary docs tasks**: on `docs/<id>` (`task_branch`, `crates/core/src/branch.rs:14`), held to the document paths (`DocumentPathsOnly`) and kept off other owners' folders (step 01's `FolderOwned`). The Product Manager writes the contract and is its assignee. The Scrum Master assigns it, or the Product Manager itself on a team with no active one (`check_assignment`, `crates/core/src/governor/gates.rs:242`). Task ids are `CTV-<n>` (the founder's decision; the rename of `docs/schemas/task-contract.schema.json:25`'s `FRK-` lands on main before this phase), so the branches are `docs/CTV-<n>`. A test fixture this plan cites at an existing `FRK-1` line reads CTV-1 after that rename.
- **No new event kind, no schema change, no migration.**

## File map

```
crates/runtime/src/session.rs                      modifies: works_on_a_task (Task 1)
crates/runtime/src/tools.rs                        modifies: writes_the_project; Call::permit (Task 1)
crates/runtime/src/tools/refusal.rs                modifies: NoTaskNoWrite (Task 1)
crates/runtime/src/daemon/hooks.rs                 modifies: judge_call; tests (Task 1)
crates/runtime/src/daemon/fixtures.rs              modifies: register_for (Task 1)
crates/runtime/src/orchestrator/session.rs         modifies: offered_tools, builtins (Task 1); tests (Tasks 1, 2)
crates/runtime/src/tools/git.rs, tools/exec.rs, tools/marketing.rs   tests (Task 1); git.rs tests (Task 2)
crates/core/src/governor/permissions.rs            modifies: default_tiers (Task 2)
crates/core/src/team.rs, crates/runtime/src/daemon/team.rs   tests (Task 2)
crates/roles/src/reviewer.rs                       modifies: REVIEWER_ROLE_FOR (Task 2)
crates/runtime/src/orchestrator/rules.rs, crates/runtime/src/prompt.rs   tests (Task 2)
crates/roles/roles/product_manager/{role.yaml,system.md,skills/writing-requirements/SKILL.md,skills/scoping-a-release/SKILL.md,skills/writing-task-contracts/SKILL.md}, crates/roles/roles/scrum_master/skills/keeping-work-flowing/SKILL.md, crates/roles/src/lib.rs   modifies (Task 3)
docs/SPEC.md                                       modifies (Task 4)
```

## Interfaces

Consumes: `SessionPurpose` (`crates/runtime/src/session.rs:22`); `SessionRegistration.purpose` and `.task_id` (`crates/runtime/src/daemon.rs:115-117`); `ToolContext.purpose` and `.task_id` (`tools.rs:117-126`); `builtin_tool_tier` (`hooks.rs:110`), `allowed_builtins` (`crates/runtime/src/claude.rs:617`), `tool_descriptors`; `default_tiers` (`permissions.rs:39`); `default_reviewer_role` (`reviewer.rs:34`); step 01's Product Manager folder text and `FolderOwned`; all on main except step 01's.

Produces:

```rust
// catervas_runtime::session
pub(crate) fn works_on_a_task(purpose: SessionPurpose, has_task: bool) -> bool;
// catervas_runtime::tools
pub(crate) fn writes_the_project(name: &str, tier: PermissionTier) -> bool;
// catervas_runtime::tools::refusal::Refusal gains NoTaskNoWrite { tool: String }   // "no_task_no_write"
// catervas_runtime::daemon::fixtures::TestDaemon gains
pub(crate) fn register_for(&self, session_id: &str, agent: &str, task: Option<&str>, purpose: SessionPurpose);
// default_tiers(Role::ProductManager) == [Read, Network, WriteWorkspace, GitLocal]
// REVIEWER_ROLE_FOR gains (Role::ProductManager, &[Role::Architect, Role::ScrumMaster])
```

## Tasks

### Task 1: Nothing changes the project outside a task's implement session

Files: `session.rs` (runtime), `tools.rs`, `tools/refusal.rs`, `daemon/hooks.rs`, `daemon/fixtures.rs` (`register_for`, as `register_with_tools` at `:125` with every Catervas tool and the given purpose), `orchestrator/session.rs`, and the tests of `tools/git.rs:454`, `tools/exec.rs:127` and `tools/marketing.rs:949` the Decisions name.
Produces: `works_on_a_task`, `writes_the_project`, `Refusal::NoTaskNoWrite`, `register_for`.

- `only_an_implement_session_about_a_task_works_on_one` (`session.rs`, runtime) — true for `(Implement, true)` alone; false for `Implement` with no task and for every other purpose with one. RED: no function.
- `writes_the_project_names_the_tools_that_change_it` (`tools.rs`) — of `tool_descriptors()`, exactly `catervas_exec`, `catervas_git_commit`, `catervas_git_push` and `catervas_propose_marketing_plan` write; of the built-ins, by `builtin_tool_tier`, `Edit`, `Write`, `MultiEdit` and `NotebookEdit` do, and `Read`, `Glob`, `Grep`, `LS`, `ToolSearch`, `WebFetch` and `WebSearch` do not. RED: no function.
- `denies_a_write_outside_a_tasks_implement_session` (`hooks.rs`, integration) — `dev-a`, who holds every writing tier, registered with `register_for` on CTV-1 for each of `triage`, `refine`, `plan`, `explore`, `verify`, `ceremony`, `conversation` and `chat`: a `Write` of `src/login/form.ts` and `mcp__catervas__catervas_git_commit` of it are each denied with a reason starting `no_task_no_write: `, and a `Read` and `mcp__catervas__catervas_git_diff` are allowed. Its `implement` session on CTV-1 (`DEV_SESSION`) is allowed the same `Write`. An `implement` session with no task is denied `catervas_git_commit` with `no_task_no_write`. RED: the writes are allowed.
- `refuses_a_commit_outside_the_tasks_implement_session` (`tools/git.rs`, integration) — CTV-1 `in_progress`, `dev-a` its assignee, its worktree made with a file: `catervas_git_commit` from a context of purpose `refine` is refused `no_task_no_write: catervas_git_commit …` and the branch has no commit; from purpose `implement` it commits. RED: `refine` commits.
- `offers_no_write_outside_a_tasks_implement_session` (`orchestrator/session.rs`, integration) — `dev-a`'s session about CTV-1 for `refine`, asked with `read_only: false`: `builtin_tools` holds none of `Edit`, `Write`, `MultiEdit`, `NotebookEdit`; `catervas_tools` holds `catervas_git_diff` and none of `catervas_exec`, `catervas_git_commit`, `catervas_git_push`. Its `implement` session holds `Write` and `catervas_git_commit`. RED: `Write` is offered to `refine`.
- `offers_a_verify_session_no_tool_that_runs_or_writes` (`rules.rs:5222`) holds unchanged with `NOT_FOR_READ_ONLY` gone.

- [x] `feat(runtime): change the project only in a task's implement session`

### Task 2: The Product Manager's docs tasks

Files: `permissions.rs` (`default_tiers`, the test at `:1114`); `reviewer.rs` (the row and a test); `tools/git.rs` and `orchestrator/session.rs` (a test each); `orchestrator/rules.rs:5168`, `prompt.rs:965`, `crates/core/src/team.rs:2200` (`applies_the_permission_answers_to_every_agent_of_the_role`: the Product Manager's `tiers(0)` becomes `[Read, Network, WriteWorkspace, GitLocal]`) and `crates/runtime/src/daemon/team.rs:3754` (`answers_what_each_agent_may_do_and_who_checks_plans`: the Product Manager's `tiers` and `base_tiers` become `["read", "network", "write_workspace", "git_local"]`) (expectations); `REVIEWER_ROLE_FOR`'s doc comment.
Consumes: `works_on_a_task`, `writes_the_project` (Task 1).

- `gives_each_role_the_default_tiers_of_the_spec_table` (`permissions.rs:1114`, updated) — the Product Manager's are `[Read, Network, WriteWorkspace, GitLocal]`. RED: `[Read, Network]`.
- `the_architect_reviews_the_product_managers_task` (`reviewer.rs`) — `default_reviewer_role` for a Product Manager's task is the Architect when one is active, the Scrum Master when the Architect is paused or absent, and `None` with neither; for an epic it is `None`, as for every role. RED: no row.
- `the_product_manager_commits_in_its_docs_task` (`tools/git.rs`, integration) — CTV-1 assigned to `pm`, reviewer the Architect, `allowed_paths: [docs/catervas/product/**]`, its worktree on `docs/CTV-1` holding new `spec.md` and `spec.agent.md` there: `pm`'s `catervas_git_commit` of both commits once on `docs/CTV-1`. RED: `tier_not_granted: git_local`.
- `the_product_manager_writes_nothing_while_it_plans` (`orchestrator/session.rs`, integration) — the Product Manager's `refine` session about CTV-1, asked as `requests.rs:195` asks it, and its `plan` session, asked as `orchestrator/rules.rs:1488` asks it on a team with no Scrum Master: `builtin_tools` holds `WebFetch` and neither `Write` nor `Edit`, and `catervas_tools` holds no `catervas_git_commit`; its `implement` session about its own docs task holds `Write` and `catervas_git_commit`. RED: the implement half, with no tier.
- `gives_a_session_the_catervas_tools_of_its_tiers` (`rules.rs:5168`, updated): the Product Manager's plan session's expected tools leave out every tool `writes_the_project` names (its `catervas_git_status` and `catervas_git_diff` now in), and it still holds `catervas_assign_task` and not `catervas_exec`.
- `lists_only_the_tools_the_agent_can_call` (`prompt.rs:965`, updated): its Product Manager `refine` half takes `inputs.tools` without the tools `writes_the_project` names, as `offered_tools` gives that session; its assertions are unchanged.

- [x] `feat(core): let the Product Manager write and commit in its docs tasks`

### Task 3: The Product Manager is told to keep its folder in docs tasks

Files: the Product Manager's `system.md`, `role.yaml`, three skills, from step 01's text; the Scrum Master's `keeping-work-flowing/SKILL.md`; `crates/roles/src/lib.rs`.
Text, exactly where it is quoted:
- `system.md`, "What you produce": the folder line becomes "- Your folder, `docs/catervas/product/`, which only you write and everyone reads, kept in your docs tasks: the product's `spec.md` and `roadmap.md`, each for people with its `.agent.md` twin for agents, the two changed together, and your sprint reports."
- `system.md`, "What you may not do", first item: "- Write application code. You write only documents, in your docs tasks, within the contract's `allowed_paths`, and you do not ask another agent to write code outside a contract."
- `system.md`, "How a session ends", item 3 gains: "In a docs task of yours, the work is done when the documents are written, committed with `catervas_git_commit`, and you have a completion note: request `verifying`."
- `role.yaml`, the `produces` line step 01 wrote gains ", in its docs tasks".
- `writing-requirements` §1 and `scoping-a-release` §4: step 01's sentence "You have no tool that writes your folder yet: do not file a task for yourself; give the text to the user in your answer." is removed, and in its place: write them in a docs task of yours, `spec.md` and `spec.agent.md` (or the notes) changed and committed together.
- `keeping-work-flowing/SKILL.md:58-59` after step 01 (`:57-58` at ac1a606e): "Nobody reviews their own work, and the Product Manager and the Scrum Master are never a task's assignee." becomes "Nobody reviews their own work. The Scrum Master is never a task's assignee, and the Product Manager is the assignee only of its own docs tasks."
- `writing-task-contracts`: "A docs task of your own has assignee role `product_manager`, reviewer role the Architect, else the Scrum Master, and `allowed_paths` within `docs/catervas/product/`, or `CHANGELOG.md` alone for your changelog task."

- `the_product_manager_works_its_folder_in_docs_tasks` (`lib.rs`) — the prompt holds "kept in your docs tasks", "`catervas_git_commit`" and "request `verifying`", and not "You have no tool that writes to the repository"; `writing-requirements` holds "docs task" and `spec.agent.md`, and neither it nor `scoping-a-release` holds "You have no tool that writes your folder yet"; `writing-task-contracts` and `keeping-work-flowing` hold their sentences above word for word. RED: step 01's text.
- `kit_skills_name_only_tools_catervas_lists` (`crates/runtime/src/daemon/team.rs:5354`) holds unchanged.

- [ ] `feat(roles): tell the Product Manager to keep its folder in docs tasks`

### Task 4: Spec

`docs/SPEC.md`: 5.1 (the Product Manager's task is reviewed by the Architect, else the Scrum Master; until step 06 the Product Manager accepts its own docs task after its reviewer verifies it, as for every task, the reviewer being the independent check); 5.6 (the table's `write_workspace` and `git_local` rows gain the Product Manager; the paragraph on why the Architect and the Marketing Specialist hold them gains it; a new paragraph: a session that is not an implement session about a task changes nothing, whatever its tiers: the hook and `call_tool` refuse the built-ins and Catervas tools `writes_the_project` names with `no_task_no_write`, after the Designer's plan gate, and such a session is not offered them); 6.1 (Default tools: read, network, write_workspace, git_local; its docs tasks on `docs/CTV-<n>`, held to the document paths and its folder); 8.2 (the `PreToolUse` paragraph's list gains `no_task_no_write`, after the Designer's plan gate); 5.17 (the Product Manager writes its folder in docs tasks); revision 0.85.

- [ ] `docs(spec): record the Product Manager's docs tasks and read-only sessions outside a task`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Tasks 1 and 2 name integration tests
# expected: xtask check: ok
git grep -n "NOT_FOR_READ_ONLY\|You have no tool that writes to the repository" -- crates
# expected: no output
```
