# Phase 12, step 05e: The incident's fix

Status: draft. Its readiness review runs once step 05d has landed.
Branch: `phase/12-ecosystem` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.2, 5.5, 5.16, 6.9, 8.5; F9
Depends on: step 05d (`incident_step`, `IncidentFacts`, incident sessions, `INCIDENT_TOOLS`, `production_gate` in an incident); step 05b (`farik_deploy`, `deployment.started`, `DeployWork`); step 05c (settling); phase 6 step 15 (`SprintHold`, `waits_for_a_sprint`, `in_the_open_sprint`, `PlannedBy::Governor`); phase 6 (merged in #19)
Readiness confirmed by: not yet run
Amended 2026-10-07 by step 08g (the founder's "Skip the queue" for a raised marketing budget, ADR 0042): 08g builds ADR 0028's exception first, as `skips_sprints` on `SprintHold`, `AssignmentInput` and `TaskProjection` (column `task_projections.skips_sprints`); this step fills it from `task.created`'s `incident` too, in place of the `incident_fix` fields its Decisions name (its `incident` column stays), and still joins the open sprint with `PlannedBy::Governor`.
Moved 2026-10-09 by ADR 0049 (project plan revision 41; the founder: "DevOps later, rest after Cloud"): phase 7 step 11e until then (its file was `step-11e-the-incident-fix.md` in phase 7's folder). The DevOps Engineer is built in the Ecosystem phase, phase 12, after its own steps 01 to 04: phase 7's steps 11 to 11f are steps 05 to 05f here, and 12 to 12e are 06 to 06e. The text below names them by their new numbers, and phase 7's other steps as phase 7's; the dated lines above, and the founder's words, keep the numbers of their day. Phase 7 step 10h, ask or auto, is phase 10 step 01; step 13, the kit check, is phase 10 step 02 and has no DevOps task, so this phase checks the DevOps Engineer's kit itself; the phases after phase 9 moved up by one.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 05 (see step 05's header); ADR 0045, 7, is this step's, and it closes the exception ADR 0028 left for it.

## Goal

An incident ends with its cause fixed. In its first session the DevOps Engineer files one fix with `farik_create_task`, which in an incident session files a standalone task marked as that incident's fix and skips triage. The fix goes through the Product Manager's contract, the Definition of Ready, the judgment when the team has it on and the human's gates as any task does, so a `high` risk fix waits for the human's acceptance; but it is never held for a sprint: it joins the open sprint without planning, or runs outside one, and is not paid from the sprint's budget, its own and the day's still holding. One agent reviews it. Once it is integrated, a third incident session deploys it with `farik_deploy`; when that deploy settles healthy the incident resolves, and a deploy task whose deploy failed and whose work the fix's commit holds moves on to `verifying`. Out of scope: the pages (05f).

## Decisions

- **Filing** (`tools/contracts.rs` `create_task`). In an incident session (step 05d's `ToolContext.incident`), `farik_create_task` is offered in the `restart` and `roll_back` steps, and it: refuses a `parent` (`incident_fix_parent`) and a second fix for the incident (`incident_fix_filed`, the one fix being a `task.created` naming it); sets `change: fix` in the contract it files, whatever the agent wrote; files the request through `file_request` as any request; records on `task.created` the new field `incident: <seq>`; and records `request.triaged { size: small, reason: "the fix of incident <n>", triaged_by: "farik" }` at once, so no triage session runs. Everything after is the ordinary path (spec 5.16): the Product Manager refines it, and its refine session's message carries the incident's detail and notes inside `untrusted`. Rejected: Farik writing the contract itself, which would skip the Definition of Ready's author.
- **The mark.** `TaskProjection.incident: Option<u64>`, filled from `task.created`, kept in a new column `task_projections.incident` by a migration at the next free number when this step runs (`<n>_incident_fix.sql`, after the numbers phase 7 steps 10c, 10e and 10f take). It is what spec 6.9 says nothing marked until this step.
- **Never held for a sprint** (ADR 0045, 7; ADR 0028's exception). `SprintHold` gains `incident_fix: bool`, which `sprint_hold` (`sprints.rs:93`) fills from the row: `waits_for_a_sprint` (`gates.rs:138`) and `in_the_backlog` (`gates.rs:148`) are false for it. `AssignmentInput` gains `incident_fix: bool`: `in_the_open_sprint` (`gates.rs:171`) is true for it, and `the_sprint_pays` does not hold it, so neither the open sprint's membership nor its remaining budget keeps it from its assignee. Its own `max_cost_usd`, the task budget cap and the team's daily budget apply as to any task (5.5), and a fix stopped by them escalates as any task does, which Today shows. **Joining the open sprint:** when a sprint is open as the fix is filed, Farik records `sprint.planned` for it with `PlannedBy::Governor`, as `join_epics_sprint` (`sprints.rs:461`) does for a breakdown's task, so the sprint's page shows it; with none open, it runs outside any.
- **The fix's steps** (`farik_core::incident`). `IncidentFacts` gains `fix: Option<FixFacts>`. With a fix filed, a healthy service no longer resolves on the restart: the step is `WaitingForFix { task }` until the fix is integrated (`task.integrated`); a fix `cancelled` is `Human(FixCancelled)`; integrated with no deploy of the incident yet is `DeployFix`; the incident's deploy settling is `Settling`; its `deployment.succeeded` is `Resolve(Deploy)`; its `deployment.failed` is `Human(FixDeployFailed)`. An unhealthy service still takes the restart and rollback steps first, whatever the fix's state. `Resolution` gains `Deploy`; `HumanWhy` gains `FixCancelled` and `FixDeployFailed`; `incident.resolved`'s `how` gains `deploy`.
- **The fix's deploy.** The incident rule starts an incident session with step `deploy` (`session.started.step` gains `deploy`) for the `DeployFix` step, offered `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_decisions`, `farik_write_incident_note` and `farik_deploy`. `production_gate` allows `farik_deploy` there once per incident (`approved_by: incident`), and refuses it in any other incident step (`deploy_refused`) or a second time (`incident_step_used`), without asking. The commit is the fix's `task.integrated` `sha`, `holds: [<fix>]`, and `deployment.started` gains `incident: <seq>`. The watch settles it as any deploy (step 05c).
- **Resolving.** On the incident's `deployment.succeeded`, the incident rule records `incident.resolved { how: deploy }` and posts "Production is fixed: <fix> is live." in the channel. **The deploy task it unblocks:** `DeployWork.settled` is also true for a deploy task whose newest deploy failed when a later incident deploy succeeded whose commit holds the deploy task's commit (`Git::merge_base(commit, deployed) == commit`); the watch then asks its `in_progress -> verifying` as it does for its own deploy (step 05c). A deploy task the fix did not cover waits, and its next session's `farik_deploy` asks the human (step 05b).
- **The skill.** `running-production` gains "The fix": one fix per incident, filed with `farik_create_task` before ending the first session when the cause is in the code or the deploy configuration; the smallest change that removes the cause, with a test that fails without it; what the fix must not touch (secrets, access, scaling); then, in the deploy step, `farik_deploy` once.

## File map

```
crates/core/src/incident.rs                                  modifies: FixFacts and the fix's steps (Task 1)
crates/core/src/governor/gates.rs                            modifies: SprintHold.incident_fix, AssignmentInput.incident_fix (Task 2)
docs/schemas/event.schema.json, crates/protocol/src/event.rs modifies: task.created.incident, deployment.started.incident, step deploy, how deploy (Task 3)
crates/store/src/migrations/<n>_incident_fix.sql, crates/store/src/projections.rs   creates/modifies: the mark (Task 3)
crates/runtime/src/tools/contracts.rs                        modifies: create_task in an incident session (Task 3)
crates/runtime/src/sprints.rs, crates/runtime/src/transitions.rs   modifies: sprint_hold, the assignment input, joining the open sprint (Task 4)
crates/runtime/src/orchestrator/requests.rs, messages.rs     modifies: the refine message carries the incident (Task 3)
crates/runtime/src/incidents.rs, orchestrator/rules.rs      modifies: the deploy step, resolving (Task 5)
crates/runtime/src/tools/production.rs, daemon/hooks.rs      modifies: farik_deploy in an incident (Task 5)
crates/runtime/src/transitions.rs, crates/runtime/src/watch.rs   modifies: DeployWork through a fix's deploy (Task 6)
crates/roles/roles/devops_engineer/skills/running-production/SKILL.md   modifies (Task 5)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 7)
```

## Interfaces

Consumes: `incident_step`, `IncidentFacts`, `incident_facts`, the incident rule, `INCIDENT_TOOLS`, `production_gate` (05d); `farik_deploy`, `DeployWork`, `Transitions::work` (05b); `watch_tick` (05c); `file_request`, `create_task`, `SprintHold`, `sprint_hold`, `AssignmentInput`, `in_the_open_sprint`, `PlannedBy::Governor`, `Git::merge_base` (on main).

Produces:

```rust
pub struct FixFacts { pub task_id: String, pub status: TaskStatus, pub integrated_sha: Option<String>,
    pub deploy: Option<Settled> }                         // farik_core::incident; IncidentFacts.fix: Option<FixFacts>
// IncidentStep::{WaitingForFix { task: String }, DeployFix}; Resolution::Deploy; HumanWhy::{FixCancelled, FixDeployFailed}
pub struct SprintHold<'a> { /* as on main */ pub incident_fix: bool }      // governor::gates
pub struct AssignmentInput { /* as on main */ pub incident_fix: bool }
pub incident: Option<u64>                                 // TaskProjection, farik_store
pub fn join_open_sprint(deps: &ToolDeps, task: &TaskId) -> Result<Option<Sprint>, SprintError>;   // farik_runtime::sprints
```

## Tasks

### Task 1: The fix's steps (core)

- `a_filed_fix_waits_rather_than_resolving`: healthy after the restart with a fix filed and not integrated is `WaitingForFix`, for either cause. RED.
- `an_integrated_fix_is_deployed_once`: `DeployFix`, then `Settling` while its deploy settles, then `Resolve(Deploy)`; a failed one is `Human(FixDeployFailed)`. RED.
- `a_cancelled_fix_waits_on_the_human`. RED.
- `an_unhealthy_service_restarts_first_whatever_the_fix`: a fix filed and unhealthy with no restart is `Restart`. RED.

- [ ] `feat(core): carry an incident through its fix`

### Task 2: Never held for a sprint (core)

- `an_incident_fix_never_waits_for_a_sprint`: under `plan_in_sprints`, a `ready` fix outside the open sprint, and one left for the Backlog, are not held; an ordinary task beside it is. RED.
- `an_incident_fix_is_not_in_the_backlog`. RED.
- `an_incident_fix_passes_the_open_sprints_membership_and_budget`: a sprint open, the fix in no sprint, the sprint's budget spent: `in_the_open_sprint` and `fits_the_open_sprint` are true; the same task without the mark is refused. RED.
- `its_own_budget_still_holds`: a fix whose `max_cost_usd` is over the team's cap fails readiness as any task. Guard.

- [ ] `feat(core): never hold an incident's fix for a sprint`

### Task 3: Filing the fix

- `an_incident_session_files_a_fix_and_skips_triage`: `task.created { incident: 412 }`, then `request.triaged { size: small, triaged_by: farik }`; the contract has `change: fix`; the row's `incident` is 412; no triage session runs on the next tick. RED.
- `one_fix_per_incident`: a second is `incident_fix_filed`; a `parent` is `incident_fix_parent`. RED.
- `outside_an_incident_nothing_changes`: a conversation's `farik_create_task` files an untriaged request with no mark. Guard.
- `the_product_manager_refines_it_knowing_the_incident`: the refine session's message holds the incident's detail and notes inside `untrusted`. RED.
- `the_mark_survives_a_rebuild`: rebuilding the projections from the log gives `incident: 412`. RED.

- [ ] `feat(runtime): file an incident's fix and skip its triage`

### Task 4: It runs at once

- `a_fix_filed_during_a_sprint_joins_it`: `sprint.planned { planned_by: governor }` names it, and rule 8 assigns it while the sprint's budget is spent. RED.
- `a_fix_runs_with_no_sprint_open_under_the_policy`: `plan_in_sprints: true`, none open: rule 8 assigns it, and an ordinary ready task waits. RED.
- `a_fix_waits_for_a_spent_day`: the day's budget spent, no session starts for it. Guard.

- [ ] `feat(runtime): run an incident's fix outside sprint planning`

### Task 5: Deploying the fix and resolving

- `an_integrated_fix_gets_its_deploy_session`: step `deploy`, offered `farik_deploy`; the call deploys the fix's integration sha with `deployment.started { incident: 412, holds: [FRK-31] }` and `approved_by: incident`. RED.
- `deploys_the_fix_once`: a second call is `incident_step_used`; `farik_deploy` in the restart step is `deploy_refused`; nothing is asked. RED.
- `a_healthy_fix_resolves_the_incident`: after its settling, `incident.resolved { how: deploy }` and the channel line. RED.
- `a_failed_fix_waits_on_the_human`: `incidents.list` says so, and no further session starts. RED.
- `running_production_teaches_the_fix` (guard over the skill's text). Guard.

- [ ] `feat(runtime): deploy an incident's fix and resolve it`

### Task 6: The deploy task the fix covers

- `a_fix_that_holds_its_work_settles_the_deploy_task`: FRK-20's deploy of `aaa…` failed; the fix's deploy of `ccc…`, a descendant of `aaa…`, succeeded: `Transitions::work` gives FRK-20 `settled: true` and the watch moves it to `verifying`. RED.
- `a_fix_that_does_not_hold_it_leaves_it`: the fix's commit not descending from `aaa…` leaves FRK-20 `in_progress`. RED.

- [ ] `feat(runtime): move a deploy task on when an incident's fix ships its work`

### Task 7: Spec and plan

`docs/SPEC.md` 3 (the incident fix's exception to "Plan work in sprints", with its reason), 5.2 (assignment: the fix and the open sprint), 5.5 (what budget pays for it), 5.16 (a request filed by an incident session, triaged by Farik), 6.9's incident steps 4 to 6 as built, 8.5 (`task.created.incident`, `deployment.started.incident`, the values added); the revision line. Project plan row 05e.

- [ ] `docs(spec): record the incident's fix`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

## Execution notes

None yet.
