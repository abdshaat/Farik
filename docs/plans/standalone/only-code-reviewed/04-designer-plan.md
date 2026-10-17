# Standalone 4 of 8: a Designer's plan needs no approval

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.2 (the Designer paragraph), 5.6 (`design_plan_not_approved`), 6.8; F9
Depends on: plan 1 committed on this branch; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin the old behaviour change in the task that changes it.

## Goal

When a UI/UX Designer's explore session proposes its plan (`design_plan.proposed`), Catervas records `design_plan.approved` at once, and the implement session follows. The Product Manager's decision session, its tool `catervas_decide_design_plan`, and its prompt go.

Unchanged:
- The governor's `check_design_plan` gate: explore stays read-only, and writes wait for the approval.
- The Architect's review of the finished change, and the Designer's browser check of a Developer's change.
- Older logs holding `design_plan.returned` still load.

## Decisions

- **The approval is recorded by the orchestrator.** `design::stage`, seeing the latest plan `Proposed`, appends `design_plan.approved { reason: "Catervas approved it: no agent approves a design plan, and the finished change is reviewed (ADR 0061)." }`. It uses `append` from `verify.rs`, with no agent and no session. It then returns `Stage::Approved(plan)` in the same tick, so the implement session starts then.
  - This was chosen over the tool appending it, because `Call::append` stamps the Designer's id.
  - A plan an older log left proposed is approved on the first tick after the upgrade.
- **What goes:**
  - `orchestrator/design.rs`'s `decide` and `DECIDE_TOOL`;
  - `tools/design.rs`'s `DecideDesignPlanInput` and `decide`;
  - the tool in `tools.rs` (descriptor, dispatch, and `lists_every_tool_with_its_tier`);
  - `session.rs`'s `(None, Some(DECIDE_TOOL))` arm (1125) and its rows in the tests at 4397 and 4726;
  - `prompt.rs`'s `DESIGN_DECISION_INSTRUCTION`;
  - `messages.rs`'s `decide_design_plan_message`;
  - the transcripts `decide_design_plan_approves_ctv_1.jsonl` and `decide_design_plan_returns_ctv_1.jsonl`, their functions in `recorded/fixtures.rs`, and their entry in `crates/cli/src/bin/catervas-e2e-serve.rs`;
  - `crates/store/src/activity.rs`'s `plan_waiting` and its call: a plan no longer waits on anyone.
- **What stays for older logs:** `PlanState::Returned`, `returns()` and the escalation on returns.
- **Words:**
  - `explore_message`'s "The Product Manager approves it before you change anything." becomes "Catervas records it, and your next session carries it out.", and so does the end of `prompt.rs`'s explore line.
  - `with_the_approved_plan`'s "The Product Manager approved your plan." becomes "Your plan is approved."
  - `ui_ux_designer/system.md` drops step 3 (Approval) and says the plan is recorded by Catervas and the finished change reviewed.
  - `skills/writing-mockups/SKILL.md` speaks of the reviewer of the finished change in place of a Product Manager who approves.
  - The event schema's description (`docs/schemas/event.schema.json`, the body shared by `design_plan.approved`) says that since ADR 0061 the approval is Catervas's, recorded with no agent, and that it was the Product Manager's decision before.
- **The e2e.** `apps/web/e2e/designer.spec.ts` drops `decide_design_plan_approves_ctv_1`, and asserts the log entry `design_plan.approved` (`logOf` trims the missing agent) after `design_plan.proposed iris` and before `review.recorded ada`, in place of `design_plan.approved mira`.
- **The task page:**
  - A proposed plan reads as `designBeingBuilt`, in the header and in the plan panel.
  - The decision line shows only when there is a decision, its name `nameOf(decision.agentId)`: "Catervas" for no agent, Mira for an older one.
  - The steps lose `designerStepApprove`, and `reached` is: accepted 4, verifying 3, approved or proposed 2, else 0.
  - `designLead` becomes "{designer} looked at your app and wrote this, then changes the page as it says. The finished change is reviewed, as for any task."
  - `designWaiting`, `designWaitingNote` and `designerStepApprove` go.

## File map

```
crates/runtime/src/orchestrator/design.rs, orchestrator/session.rs, orchestrator/messages.rs, prompt.rs   (Task 1)
crates/runtime/src/tools/design.rs, crates/runtime/src/tools.rs                                          (Task 1)
crates/runtime/src/recorded/fixtures.rs, recorded/transcripts/decide_design_plan_*.jsonl, crates/cli/src/bin/catervas-e2e-serve.rs  (Task 1)
crates/store/src/activity.rs, docs/schemas/event.schema.json, apps/web/e2e/designer.spec.ts             (Task 1)
crates/roles/roles/ui_ux_designer/system.md, skills/writing-mockups/SKILL.md                            (Task 1)
apps/web/src/pages/TaskDetail.tsx, apps/web/src/strings/en.ts, apps/web/src/pages/TaskDetail.test.tsx  (Task 2)
```

## Interfaces

Consumes: `plan_history`, `design_plan`, `PlanState`, `Stage`, `append` (`verify.rs`), the harness of `design.rs`'s `mod tests`, `explore_plans_ctv_1`.
Produces: nothing new. Removed: the items listed above.

## Tasks

### Task 1: Catervas approves the plan

Files: those of the File map for Task 1.

Tests:
- `catervas_approves_a_proposed_plan_and_the_designer_implements` (`design.rs`; ignored, needs git). It replaces `explores_first_then_asks_the_product_manager`, `implements_only_after_approval`, `explores_again_with_the_returned_reason`, `asks_the_active_product_manager_not_a_paused_one` and `starts_again_without_an_answer`:
  - After Iris's explore session proposes (`explore_plans_ctv_1`), the next tick appends `design_plan.approved` with no `agent_id`, starts Iris's `implement` session in that tick, and starts no Product Manager session.
  - A log holding `design_plan.returned` (appended directly) reads `PlanState::Returned`, and starts a new explore session.
- Changed with it:
  - `sends_the_designers_work_to_the_architect` drops `decide_design_plan_approves_ctv_1` and the Product Manager's `verify` session from what it expects;
  - `one_tool_sessions_get_no_skills` and `gives_custom_servers_to_task_sessions_only` (`session.rs`) drop their `(SessionPurpose::Verify, Some(super::DECIDE_TOOL))` row;
  - `refuses_the_decision_but_from_the_product_manager` (`tools/design.rs`) goes with `decide`;
  - `lists_every_tool_with_its_tier` no longer lists the tool;
  - `escalates_after_too_many_returns` and `explores_again_once_the_human_grants_more_tries` are unchanged.
- `says_a_designer_explores_then_waits_for_its_plans_decision` (`activity.rs`) becomes `says_a_designer_explores_then_rests`: after `design_plan.proposed`, Iris's line is "Nothing to do right now".

- [ ] `feat(runtime): approve a Designer's plan at once`

### Task 2: the task page says Catervas approved it

Files: those of the File map for Task 2.

Tests:
- `shows_the_designers_plan_and_who_approved_it` (replaces `shows_the_plan_waiting_for_the_product_manager`):
  - Proposed: the header says "Being built", and the panel shows the new lead and "Iris wrote this plan today at 09:52".
  - Approved by an event with no agent: "Catervas approved the plan today at 10:14".
  - Returned by `mira` (an older log): "Mira sent the plan back today at 10:14", and the returns line.
  - No axe violations.
- `shows_how_the_designer_works_beside_its_task` (changed): with a proposed plan the steps read "Looks at your app in a browser Done", "Writes a plan Done", "Iris changes the page Now", "Ada reviews it, as for any task".

- [ ] `feat(web): show a Designer's plan as approved by Catervas`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
