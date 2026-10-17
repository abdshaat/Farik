# Standalone: plans need no review

Status: draft
Branch: `fix/plans-need-no-review` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2, 5.6, 5.14, 5.16 item 3, 6.1, 6.3, 6.8; F9
Depends on: `origin/main` at `74b4a5af` (#45 merged); ADR 0061 (committed with this plan)
Readiness confirmed by: failed, round 1 (2026-10-16, an Opus 5.5 session that did not write it): two unmade decisions, listed under Readiness review; a second round follows once they are made

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008). One test per major functionality (ADR 0060); a test that pins the old behaviour is changed in the task that changes the behaviour, never skipped.

## Goal

ADR 0061 (the founder, 2026-10-16: "any agent can plan and start working. No other agent need to verify the plan, agents only need to verify final results"). Done means: an epic's plan task is accepted (or rejected) by Catervas on its own criteria with no agent session; the Product Manager files the plan's tasks on the next tick, reading the plan from the plan task's branch; those tasks start from that branch, so their pull requests carry the plan, and no pull request is opened for the plan alone; a Designer's proposed plan is approved by Catervas at once and the implement session follows. Out of scope (D5): contract judging, every human approval, the reviews of finished work (the Architect's of the Designer's change and the Designer's browser check included), the epic's own review and close-out, the integration of finished work, and past events.

## Decisions

The controller's design, not reopened here:

- D1. A plan task (contract `plan` set and `allowed_paths == [plan]`) gets no agent review: its contract carries machine criteria only (the `artifact` criterion that the plan exists) and Catervas accepts it in `verifying` when they pass. The PM's prompt and skill stop asking for a review; `plan_task_message` says so.
- D2. The PM files the plan's tasks as soon as the plan task is accepted. `epic_step` no longer waits for its integration; `product_managers_message` reads the plan from the plan task's branch when the integration branch does not hold it.
- D3. A lane task of that epic starts from the plan task's branch while the plan is not on the integration branch. Under `pull_request` and `manual` no pull request is opened for a plan task: it is integrated on the first tick on which its plan file is on the integration branch. `auto_merge` is unchanged. An epic still closes out only when every task is integrated.
- D4. A Designer's `design_plan.proposed` is followed at once by `design_plan.approved` by Catervas; the PM's `catervas_decide_design_plan` session and prompt go; `check_design_plan` stays; old logs with `design_plan.returned` still load.
- D5. Unchanged: see Goal. D6. ADR 0061; SPEC 0.115; `docs/design/catervas-folders.md`.

Made by this plan:

- The reviewer field of a plan task is kept, filled by `fill_reviewer_role` as today (the Product Manager, `REVIEWER_ROLE_FOR`'s Architect row), and never run. Chose this over no reviewer because the Definition of Ready's `ReviewerAvailable`, the assignment gate and `default_reviewer_role` stay untouched, and because a plan task the PM gives a `review` or `human` criterion anyway falls back to the ordinary review by that reviewer.
- Which plan task Catervas verifies alone: `catervas_verifies_alone(contract)` = `is_plan_task(contract)` and every exit criterion's method is `command`, `test` or `artifact`. Both are pure functions in `catervas-core`, so `children_of_epic`, verify, integrate, the lane start and the governor's gate read one rule.
- The governor gets two table rows, `verifying -> accepted` (`Governor`, `DefinitionOfDone`) and `verifying -> rejected` (`Governor`, `RejectionReasons`), each after the existing row of that move. `check_gate` refuses both for any contract that `catervas_verifies_alone` does not hold, with the detail `Catervas accepts or rejects on its own only an epic's plan task whose every criterion it runs (ADR 0061)`. Chose this over filing the move in the PM's or the reviewer's name with `filed_by_catervas` (the log would say an agent decided) and over a new gate id (the Definition of Done and the rejection gate stay the checks). Agents cannot ask as `Governor` (`actor_for` maps it to false), so no agent reaches these rows.
- The Definition of Done is unchanged: Catervas writes the review note itself, `note.written { kind: review, written_by: "catervas" }` with no agent and no session on the envelope, its text `Catervas ran every criterion of this plan task itself; no agent reviews a plan (ADR 0061).` then a blank line, then one line per criterion `<id>: passed|failed: <evidence>`. No `review.recorded` is written: no reviewer ran.
- A failed criterion is rejected by the governor, the reasons being that note and the failed ids; `rejected -> in_progress` follows as today. A result that needs the human (risk `high`) waits for the human's acceptance as today, then the governor accepts.
- The lane start: a task with `plan` P that is not itself a plan task, with a `parent`, starts from the branch of the last accepted plan task under the same parent whose `plan` is P and whose `repo` equals its own, when P is not on its repository's integration branch (`Git::has_file`) and that branch exists (`Git::has_local_branch`); otherwise from the integration branch, as today. A task in another member of a project folder starts from its own integration branch.
- A plan task is integrated by its plan file, not by ancestry: `Git::has_file(into, plan)`, so a squash merge of a lane's pull request counts. The `sha` is the integration branch's head (`git.merge_base(into, into)`, as `closed_for_the_human` reads it), `integrated_by` `governor` on a tick, `human` on `catervas integrate`. Under `pull_request`, `catervas integrate` of a plan task whose plan is not there yet is refused `carried_by_its_tasks: <id> writes an epic's plan, which reaches <into> with its tasks' pull requests`; under `manual`, it merges as today. No scan refresh follows (a plan is no code).
- The Designer's approval is recorded by the orchestrator, not the tool: `design::stage`, seeing the latest plan `Proposed`, appends `design_plan.approved { reason: "Catervas approved it: no agent approves a design plan, and the finished change is reviewed (ADR 0061)." }` with no agent on the envelope (Catervas's own) and returns `Stage::Approved(plan)` in the same tick, so the implement session starts then. Chose this over the tool appending it (`Call::append` stamps the Designer's id). A plan an older log left `Proposed` is approved on the first tick after the upgrade.
- The Designer's activity line "Waiting for <PM> to approve a plan" (`catervas_store::activity::plan_waiting`) goes: a plan no longer waits for anyone.
- Web: a proposed plan reads as "Being built"; the decision line names whoever recorded it (`nameOf(decision.agentId)`: "Catervas" for an approval with no agent, Mira for an older one); "How Iris works" has four steps (the approval step goes).
- The transcripts `decide_design_plan_approves_ctv_1` and `decide_design_plan_returns_ctv_1` and their fixtures go; `apps/web/e2e/designer.spec.ts` no longer lists the first and expects `design_plan.approved` with no agent.

## File map

```
crates/core/src/folders.rs                          adds: is_plan_task (Task 1)
crates/core/src/governor/done.rs                    adds: catervas_verifies_alone (Task 1)
crates/core/src/governor/transition_table.rs        modifies: two governor rows, its three table tests (Task 1)
crates/core/src/governor/transition.rs              modifies: check_gate's governor refusal; every_row and its two tests; tests (Task 1)
crates/runtime/src/orchestrator/verify.rs           modifies: verify_alone; tests (Task 2)
crates/runtime/src/orchestrator/requests.rs         modifies: epic_step, Child, children_of_epic, product_managers_message (Task 3); lane_start (Task 4); tests
crates/runtime/src/orchestrator/messages.rs         modifies: plan_task_message, plan_tasks_message (Task 3); explore_message, with_the_approved_plan, drops decide_design_plan_message (Task 6)
crates/roles/roles/product_manager/system.md, product_manager/skills/writing-task-contracts/SKILL.md,
crates/roles/roles/architect/skills/planning-in-lanes/SKILL.md                         (Task 3)
crates/runtime/src/orchestrator/rules.rs            modifies: assigned uses lane_start (Task 4)
crates/runtime/src/orchestrator/integrate.rs        modifies: awaiting, integrate_locked, carried_plan; tests (Task 5)
crates/runtime/src/orchestrator/design.rs           modifies: stage approves, drops decide and DECIDE_TOOL; tests (Task 6)
crates/runtime/src/tools/design.rs, crates/runtime/src/tools.rs   drop catervas_decide_design_plan (Task 6)
crates/runtime/src/orchestrator/session.rs, crates/runtime/src/prompt.rs   drop DESIGN_DECISION_INSTRUCTION; explore line (Task 6)
crates/runtime/src/recorded/fixtures.rs, recorded/transcripts/decide_design_plan_*.jsonl, crates/cli/src/bin/catervas-e2e-serve.rs  (Task 6)
crates/store/src/activity.rs                        drops plan_waiting; its test (Task 6)
crates/roles/roles/ui_ux_designer/system.md, ui_ux_designer/skills/writing-mockups/SKILL.md   (Task 6)
docs/schemas/event.schema.json                      the design_plan.approved description (Task 6)
apps/web/e2e/designer.spec.ts                       (Task 6)
apps/web/src/pages/TaskDetail.tsx, apps/web/src/strings/en.ts, apps/web/src/pages/TaskDetail.test.tsx   (Task 7)
docs/SPEC.md, docs/design/catervas-folders.md, this plan   (Task 8)
```

## Interfaces

Consumes (all on `main`): `TaskContract`, `wire_method`, `evaluate_done`, `check_rejection_reasons`, `find_transitions`, `TransitionActor`, `GateId` (`catervas-core`); `Git::has_file`, `Git::has_local_branch`, `Git::file_at`, `Git::merge_base`, `held_document`, `integration_branch`, `TaskRepository` (`catervas-store`); `run_what_catervas_runs`, `governor_results`, `since_verifying`, `history`, `append`, `escalate`, `ran_criteria`, `context`, `task_repository_of`, `governor_moves`, `record_integrated`, `plan_history`, `design_plan`; test harnesses `Harness`, `a_harness`, `ticked`, `an_accepted_plan_task_at`, `a_scrum_masters_epic_in_progress`, `integrated`, `commit_at_root`.

Produces:

```rust
// catervas_core::folders
/// Its `plan` is set and its `allowed_paths` are exactly that plan: it writes an epic's plan.
#[must_use] pub fn is_plan_task(contract: &TaskContract) -> bool;
// catervas_core::governor::done
/// `is_plan_task`, and every exit criterion is `command`, `test` or `artifact` (ADR 0061).
#[must_use] pub fn catervas_verifies_alone(contract: &TaskContract) -> bool;
// catervas_runtime::orchestrator::verify (private)
async fn verify_alone(orchestrator: &Orchestrator, team: &Team, row: &TaskProjection,
    contract: &TaskContract, waiting: &mut Waiting) -> Result<Option<TickReport>, OrchestratorError>;
// catervas_runtime::orchestrator::requests
struct Child { task_id: TaskId, status: TaskStatus, plan_task: bool }   // awaiting_integration goes
fn product_managers_message(deps: &OrchestratorDeps, team: &Team, contract: &TaskContract,
    step: &EpicStep, listed: &[(String, String, String)], plan_task: Option<&TaskId>)
    -> Result<String, OrchestratorError>;
/// The branch `contract`'s worktree starts from (D3), given its repository.
pub(super) fn lane_start(deps: &OrchestratorDeps, contract: &TaskContract,
    repository: &TaskRepository) -> Result<String, OrchestratorError>;
// catervas_runtime::orchestrator::integrate (private)
fn carried_plan(tools: &ToolDeps, subject: &Subject, plan: &str, target: &IntegrationTarget,
    asker: Asker) -> Result<Option<IntegrationOutcome>, OrchestratorError>;
```

Removed: `DecideDesignPlanInput`, `tools::design::decide`, `DECIDE_TOOL`, `orchestrator::design::decide`, `decide_design_plan_message`, `DESIGN_DECISION_INSTRUCTION`, `plan_waiting`, the two transcripts and their fixture functions. Wire: no event or command changes; `task.transitioned` already carries `actor: governor`.

## Tasks

### Task 1: the governor may accept or reject a plan task

Files: modified `crates/core/src/folders.rs`, `crates/core/src/governor/done.rs`, `crates/core/src/governor/transition_table.rs` (the two rows; `has_exactly_twenty_two_distinct_rows` becomes `has_exactly_twenty_four_distinct_rows`, `has_eighteen_specific_rows_and_four_any_rows` becomes `has_twenty_specific_rows_and_four_any_rows`, and `matches_every_line_of_the_spec_table` gains `(A::Governor, G::DefinitionOfDone)` after the PM's and `(A::Governor, G::RejectionReasons)` after the reviewer's), `crates/core/src/governor/transition.rs` (`check_gate`: for `actor == Governor` and gate `DefinitionOfDone` or `RejectionReasons`, the refusal above unless `catervas_verifies_alone`; then the gate as today; in `mod tests`, `every_row` becomes `[Case; 24]` with the rows renumbered in table order and two cases added, `Verifying -> Accepted` and `Verifying -> Rejected` by `A::Governor`, whose `prepare` makes the contract a plan task (`plan` and `allowed_paths` both `docs/catervas/architecture/plans/CTV-1.md`, one `artifact` criterion holding the id `a_result` records, `done.changed_paths` that path) and, for the rejection, sets `rejection` to that criterion with written reasons; `refuses_every_row_of_the_table_to_an_actor_it_does_not_name` asks a `Governor` row from `verifying` as `A::Assignee`, since the reviewer shares `verifying -> rejected`). Produces `is_plan_task`, `catervas_verifies_alone`.

- `the_governor_verifies_only_a_plan_task_alone` (in `transition.rs`) — a verifying contract with `plan` `docs/catervas/architecture/plans/CTV-1.md`, `allowed_paths` exactly that, one `artifact` criterion, done evidence passing (a reviewer-run result, a completion note, a review note): `ask(Accepted, A::Governor, None)` decides with the governor's row; with `rejection` reasons naming the criterion, `ask(Rejected, A::Governor, None)` decides with the governor's row; the same contract with a second allowed path, and with a `review` criterion added, is refused on both moves with `GateFailed` whose details are exactly the refusal sentence above.

- [ ] `feat(core): let the governor accept or reject an epic's plan task alone`

### Task 2: Catervas verifies a plan task with no session

Files: modified `crates/runtime/src/orchestrator/verify.rs`. In `verifying`, after the epic branch and before the reviewer lookup: `if catervas_verifies_alone(&contract) { return verify_alone(...) }`. `verify_alone`: `spent` gate; `run_what_catervas_runs` (its three outcomes handled as `verifying` handles them); the latest governor result per mechanical criterion since `verifying` (`governor_results`); any criterion without one → `ran_criteria`; no review note since work began (`context(..).done.review_note`) → append the note of Decisions; then any failed → `TransitionRequest { to: Rejected, actor: Governor, agent_id: None }` with `Rejection { failed_criterion_ids, reasons: note }`; else when `result_needs_the_human` and not `human_accepted` → `ran_criteria`; else `to: Accepted, actor: Governor`. Reports: `accepted it: Catervas ran its criteria, and no agent reviews a plan`, `rejected it for <ids>: Catervas ran its criteria`, or `the governor would not <accept|reject> it: <details>`. Test `#[ignore = "needs the git program: cargo xtask check --integration"]`, in `mod tests` with `a_harness`; a plan task CTV-2 (`ada`'s, the Architect, reviewer `pm`, the plan's path as `plan`, `allowed_paths` and one `artifact` criterion C1), its worktree on `docs/CTV-2`, a completion note, moved to `verifying`.

- `catervas_verifies_a_plan_task_with_no_session` — with the plan committed on `docs/CTV-2`, two ticks start no session (`adapter.started()` is empty), CTV-2 is `accepted`, its log holds `criterion.recorded` by `governor` for C1, a `note.written` of kind `review` with `written_by` `catervas` and no `agent_id`, and `task.transitioned` to `accepted` with `actor` `governor`; in a second harness with nothing committed, CTV-2 is `rejected` by `governor` with C1 in `failed_criterion_ids`, and no session started.

Tests that pinned a PM's review or acceptance of a plan task change to this flow here.

- [ ] `feat(runtime): accept an epic's plan task on Catervas's own runs`

### Task 3: the plan's tasks follow its acceptance

Files: modified `crates/runtime/src/orchestrator/requests.rs` (`Child` and `children_of_epic` with `is_plan_task`; `epic_step` returns `WriteThePlansTasks` when every live task is an accepted plan task, whatever its integration; `in_progress_epic` passes the last live plan task's id; `product_managers_message` reads the plan with `held_document` on the integration branch, else on `task_branch` of the plan task's contract in its `task_repository_of` repository, a missing branch (`GitError::CommandFailed`) or a missing member counting as no plan; doc comments), `crates/runtime/src/orchestrator/messages.rs`, and the three role files of the File map. `plan_task_message` ends, in place of its last sentence, `No agent reviews the plan: give its task no review criterion. Catervas accepts it once the plan exists, and you are asked for its tasks then.`; `plan_tasks_message` opens `The Architect's plan for the epic {epic} is at {plan}: {text}` (the rest unchanged). Prompts: the PM's `system.md` "once the plan is on the integration branch" becomes "once Catervas has accepted the plan task"; the skill's step 1 adds "and no `review` criterion: no agent reviews a plan", step 2 opens "Once Catervas has accepted the plan task (you are asked with the plan's text)"; `planning-in-lanes` §3's "The Product Manager reviews it." becomes "No agent reviews it: Catervas accepts it once it exists, and the Product Manager files its tasks from it, so write it whole first.".

- `reads_an_epics_next_step` (changed) — `Child` without `awaiting_integration`: an accepted plan task alone is `WriteThePlansTasks`; the other cases as today.
- `writes_the_plans_tasks_from_the_plan_tasks_branch` (replaces `writes_the_plans_tasks_once_the_plan_is_integrated`; ignored, needs git) — `an_accepted_plan_task` with `docs/CTV-2` created from `main` and the plan committed there only: the first tick starts the PM's `plan` session on CTV-1 whose prompt holds `<untrusted source="plan">\n# Plan for CTV-1`, `## Lane 1` and `` `lane` ``, while CTV-2 still awaits integration and `main` has no plan.
- `asks_for_the_architects_plan_task` (in `messages.rs`) also asserts the message ends with `No agent reviews the plan: give its task no review criterion. Catervas accepts it once the plan exists, and you are asked for its tasks then.`; `asks_for_a_task_per_planned_task` asserts it starts with `The Architect's plan for the epic CTV-1 is at docs/catervas/architecture/plans/CTV-1.md: ` (the rest unchanged). `breaks_down_again_when_the_plan_is_missing` and `the_plan_is_read_in_either_format` keep their assertions.

- [ ] `feat(runtime): file an epic's tasks once its plan task is accepted`

### Task 4: a lane starts from its plan

Files: modified `crates/runtime/src/orchestrator/requests.rs` (`lane_start`, by the rule in Decisions, reading the board with `deps.tools.projections.board()`), `crates/runtime/src/orchestrator/rules.rs` (`assigned`: `create_worktree(&worktree, &branch, &lane_start(deps, &contract, &repository)?)`). Test ignored, needs git, in `requests.rs`.

- `a_lane_starts_from_its_plan_tasks_branch` — epic CTV-1, its plan task CTV-2 accepted with the plan on `docs/CTV-2` only, and a lane task CTV-3 (`dev-a`, `plan` the same path, lane 1) `assigned`: after a tick CTV-3 is `in_progress` and its worktree holds the plan file; after the plan is committed on `main`, a second lane task CTV-4 assigned the same way starts from `main` (`git.commit_count("main", "feature/CTV-4")` is 0).

- [ ] `feat(runtime): start an epic's tasks from its plan's branch`

### Task 5: a plan task rides with its tasks

Files: modified `crates/runtime/src/orchestrator/integrate.rs`: `awaiting` lets a plan task through under `manual` (it reads the contract only then); `integrate_locked`, once `target` is resolved and before the policy match, when the policy is `pull_request` or `manual` and the contract `is_plan_task`, returns `carried_plan`'s answer: `Some(Merged { sha, scan: Unchanged })` after `record_integrated` when the plan is on `into`; `None` on a tick otherwise; for `Asker::Human`, the refusal `carried_by_its_tasks` under `pull_request`, and the merge as today under `manual`. Test ignored, needs git, in `mod tests`.

- `integrates_a_plan_task_by_its_plan_not_a_pull_request` — for `pull_request` and for `manual`: an accepted plan task with its plan on its own branch; a tick opens no pull request (no `pull_request.opened`, the fake forge asked nothing) and leaves it awaiting; with the plan committed on `main`, the next tick records `task.integrated { into: "main", integrated_by: "governor" }` and it no longer awaits; under `pull_request`, `integrate` before that answers the refusal starting `carried_by_its_tasks: `.

- [ ] `feat(runtime): integrate a plan task once its plan reaches the integration branch`

### Task 6: a Designer's plan needs no approval

Files: those of the File map for Task 6. `stage` approves as Decisions says (with `append` from `verify.rs`, agent `None`, session `None`); `decide` and `DECIDE_TOOL` go, as does the `(None, Some(DECIDE_TOOL))` arm in `session.rs`; `catervas_decide_design_plan` leaves `tools.rs`'s descriptors, dispatch and `lists_every_tool_with_its_tier`; `explore_message`'s "The Product Manager approves it before you change anything." becomes "Catervas records it, and your next session carries it out."; `with_the_approved_plan`'s "The Product Manager approved your plan." becomes "Your plan is approved."; `prompt.rs`'s explore line ends "Catervas records it, and your next session carries it out."; `plan_waiting` and its call go from `activity.rs`; the Designer's `system.md` drops step 3 (Approval) and says the plan is recorded by Catervas and the finished change reviewed, `writing-mockups` speaks of the reviewer of the finished change in place of the approving PM; the event schema's description says `design_plan.approved` is recorded by Catervas with no agent since ADR 0061, and was the PM's decision before; `designer.spec.ts` drops `decide_design_plan_approves_ctv_1` and asserts the log entry `design_plan.approved` (`logOf` trims the missing agent) after `design_plan.proposed iris` and before `review.recorded ada`, in place of `design_plan.approved mira`.

- `catervas_approves_a_proposed_plan_and_the_designer_implements` (in `design.rs`, replacing `explores_first_then_asks_the_product_manager`, `implements_only_after_approval`, `explores_again_with_the_returned_reason`, `asks_the_active_product_manager_not_a_paused_one` and `starts_again_without_an_answer`; ignored, needs git) — after Iris's explore session proposes (`explore_plans_ctv_1`), the next tick appends `design_plan.approved` with no `agent_id` and starts Iris's `implement` session in that tick, and no Product Manager session is started; a log holding `design_plan.returned` (appended directly) still reads `PlanState::Returned` and starts a new explore session.
- Changed with it: `sends_the_designers_work_to_the_architect` drops `decide_design_plan_approves_ctv_1` and the Product Manager's `verify` session from what it expects; `one_tool_sessions_get_no_skills` and `gives_custom_servers_to_task_sessions_only` (`session.rs`) drop their `(SessionPurpose::Verify, Some(super::DECIDE_TOOL))` row; `refuses_the_decision_but_from_the_product_manager` (`tools/design.rs`) goes with `decide`; `lists_every_tool_with_its_tier` (`tools.rs`) no longer lists `catervas_decide_design_plan`. `escalates_after_too_many_returns` and `explores_again_once_the_human_grants_more_tries` are unchanged.
- `says_a_designer_explores_then_waits_for_its_plans_decision` in `activity.rs` becomes `says_a_designer_explores_then_rests`: after `design_plan.proposed` Iris's line is "Nothing to do right now".

- [ ] `feat(runtime): approve a Designer's plan at once`

### Task 7: the task page says who approved the plan

Files: modified `apps/web/src/pages/TaskDetail.tsx` (status word: `proposed` reads `designBeingBuilt`; the plan panel shows the `designBeingBuilt` block for `proposed` and `approved`, the decision line only with a decision, `pm` there being `nameOf(decision.agentId)`; steps lose `designerStepApprove` and `reached` is accepted 4, verifying 3, approved or proposed 2, else 0), `apps/web/src/strings/en.ts` (`designLead` becomes "{designer} looked at your app and wrote this, then changes the page as it says. The finished change is reviewed, as for any task."; `designWaiting`, `designWaitingNote`, `designerStepApprove` go), tested by `apps/web/src/pages/TaskDetail.test.tsx`.

- `shows_the_designers_plan_and_who_approved_it` (replaces `shows_the_plan_waiting_for_the_product_manager`) — proposed: the header says "Being built" and the panel shows the new lead and "Iris wrote this plan today at 09:52"; approved by an event with no agent: "Catervas approved the plan today at 10:14"; returned by `mira` (an older log): "Mira sent the plan back today at 10:14" and the returns line; no axe violations.
- `shows_how_the_designer_works_beside_its_task` (changed) — with a proposed plan the steps read "Looks at your app in a browser Done", "Writes a plan Done", "Iris changes the page Now", "Ada reviews it, as for any task".

- [ ] `feat(web): show a Designer's plan as approved by Catervas`

### Task 8: the record

Files: modified `docs/SPEC.md` (header revision 0.115 (2026-10-16): ADR 0061; 5.2: the two governor rows, and the Designer paragraph's "Until the Product Manager has approved" becomes "until the task's latest plan is approved, which Catervas records as soon as it is proposed"; 5.6's `design_plan_not_approved` wording likewise; 5.14: a plan task under `pull_request` or `manual` opens no pull request and is integrated once its plan is on the integration branch; 5.16 item 3: "and the Product Manager reviews it (6.1)" becomes "no agent reviews it: Catervas accepts it on its criteria (ADR 0061), and the Product Manager files its tasks once it is accepted, which start from its branch while the plan is not on the integration branch"; 6.1 and 6.3 where they say the PM reviews the plan; 6.8 Mandate, Cannot, "Its flow" (item 2 becomes Catervas's approval, recorded at once; `catervas_decide_design_plan` gone), the restart sentence and the web paragraph's "Waiting for … to approve a plan"; F9 one sentence), `docs/design/catervas-folders.md` ("Planning a feature" gains, after step 2, that no agent reviews the plan and its tasks are filed once Catervas accepts it), this plan (status done, ticks).

- [ ] `docs(spec): plans need no review, revision 0.115`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (the ignored tests of Tasks 2 to 6 among them)
```

## Readiness review

Round 1, 2026-10-16: failed on two decisions the plan rests on and does not make. Neither is invented here.

1. A lane task's diff. A task started from the plan task's branch (Task 4) has the plan file in `git.changed_paths(integration_branch, task_branch)` (`transitions.rs` `work`, a three-dot diff), and still has it after the plan reaches the integration branch by another lane's squash merge, since its merge base does not move. The Definition of Done's `paths_within_allowed` (`done.rs`) then refuses every lane task whose `allowed_paths` are the Architect's paths without the plan (the PM's skill step 2), and the same diff feeds the reviewer's changes, the UI-change check and the twin check. Undecided: whether the plan path is left out of a lane task's changed paths, the diff is taken from the start point, the lane task's `allowed_paths` gain the plan, or something else.
2. A plan task whose result needs the human (risk `high`). The Decisions say no `review.recorded` is written and that it "waits for the human's acceptance as today", but today a task's result reaches the human's waiting list only once `review_passed` (`crates/store/src/waiting.rs` line 446), `catervas send-back` refuses it with `review_first` (`orchestrator/human.rs` line 1760), and the `HumanRejection` gate refuses it (`governor/transition.rs` line 446). Undecided: whether Catervas records `review.recorded` for such a task, those three checks treat a task `catervas_verifies_alone` holds as they treat an epic, or a plan task may not need the human at all.

Fixed in this round: `is_plan_task` and `catervas_verifies_alone` are new (the File map said "modifies"); Task 1 now names `every_row` and its two table-wide tests, which the two new rows break; Task 3 names the two `messages.rs` tests and their exact strings; Task 6 names every test it removes or changes; the e2e log entry is exact.

Notes for the executor (no decision needed): `context()` in `verify.rs` asks as the Product Manager, which is the same Done evidence the governor's row is judged on; the reviewer's own runs are recorded `run_by: reviewer`, which the Definition of Done counts, so `verify_alone` needs no new evidence kind; `carried_plan` returns from `integrate_locked` before the scan refresh at its end, so no scan runs.
