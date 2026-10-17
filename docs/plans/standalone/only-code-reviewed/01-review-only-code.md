# Standalone 1 of 7: review only code

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.1, 5.2, 5.3, 5.4, 5.16 items 3 and 4; F9
Depends on: `origin/main` at `74b4a5af`; ADR 0061
Readiness confirmed by: pending

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008). One test per major functionality (ADR 0060). A test that pins the old behaviour is changed in the task that changes the behaviour, never skipped.

## Goal

A task that does not change code is accepted or rejected by the governor on the criteria Catervas runs (`command`, `test`, `artifact`), with no reviewer session and no Product Manager acceptance session. This covers an epic, a plan, a document, research, and the Marketing, Finance and Procurement Specialists' tasks. A code task (`reviewed_by_an_agent`) is reviewed exactly as today. Readiness refuses `review` and `human` criteria outside code, and every contract Catervas files whole carries machine criteria. No result waits on the human.

Out of scope:
- integration, which is plan 2;
- the plan's tasks and lanes, which are plan 3;
- the contract's approval, which is plan 5;
- contract judging (R5).

## Decisions

- **"Changes code"** is `reviewed_by_an_agent(contract) = kind == Task && changes_code(assignee_role)` (the README says why).
- **Catervas verifies alone** a contract that `catervas_verifies_alone` holds: `!reviewed_by_an_agent` and every exit criterion is `command`, `test` or `artifact`. A contract that is not reviewed but still carries a `review` or `human` criterion (an older one, written before readiness refused them) takes the old path unchanged, so old logs finish.
- **The reviewer field** stays as `fill_reviewer_role` and the assignment gate set it, and is never run for such a task. Chosen over clearing it, which would change `ReviewerAvailable`, the assignment gate, `default_reviewer_role` and every fixture for no behaviour.
- **Two governor rows**:
  - `verifying -> accepted` (`Governor`, `DefinitionOfDone`), after the PM's row;
  - `verifying -> rejected` (`Governor`, `RejectionReasons`), after the reviewer's row.

  `check_gate` refuses both, when `!catervas_verifies_alone`, with the detail `Catervas accepts or rejects on its own only a task that changes no code and whose every criterion it runs (ADR 0061)`. Agents cannot ask as `Governor` (`actor_for`).

  This was chosen over filing the move in an agent's name, which would put a false actor in the log.
- **The Definition of Done keeps every rule but `HumanAccepted`.** Catervas writes the review note itself as `note.written { kind: review, written_by: "catervas" }`, with no agent and no session on the envelope. Its text is `Catervas ran every criterion of this task itself; only a code change is reviewed by an agent (ADR 0061).`, then a blank line, then one line per criterion, `<id>: passed|failed: <evidence>`. No `review.recorded` is written.
- **Results need no human.** These are removed with their callers:
  - `result_needs_the_human`;
  - `result_awaits_human`;
  - `DoneRule::HumanAccepted` and its check `human_accepted`;
  - the result half of `requires_human_acceptance`.

  `requires_human_acceptance` stays, used by the contract gate alone until plan 5. `DoneRule::HumanCriterionAccepted` stays for code tasks' `human` criteria. `HumanRejection` stays in the table; its gate opens only while a result waits on a `human` criterion of a code task.
- **An epic** is verified by `verifying_epic` as today up to its criteria: it waits for its tasks' integration, then runs its mechanical criteria on the integration branch. After that, when `catervas_verifies_alone(epic)` holds, the governor writes the note and accepts or rejects, with no Product Manager session and no human. Otherwise the old path runs.
- **Readiness rule `NoReviewOutsideCode`** (`no_review_outside_code`), placed after `CriteriaMethodsValid`. For a contract that `!reviewed_by_an_agent` with any `review` or `human` criterion, it refuses: `<ids> ask for a review or the human, and only a code change is reviewed (ADR 0061): use command, test or artifact criteria`. The plain words, in `plain.rs`, are "This task asks for a review, and only code is reviewed. Check it with a command, a test or a file instead." `private_folder_task`'s "use artifact, review and human criteria" becomes "use artifact criteria".
- **Whole-filed templates** use only `artifact` criteria when their assignee's role does not change code: one per file of the contract's `allowed_paths` (none of these templates holds a glob), `{ "method": "artifact", "path": <file> }`, numbered C1 onwards, each with the `satisfies` its review criterion had.
  - The functions: `rederive_request`, `product_plan_request`, `change_request` and `rewrite_request` (in `crates/store/src/requests.rs`), and every task of `crates/runtime/src/catervafication.rs` (its last, `review` criterion goes; its path criteria stay).
  - The Catervafication epic's C1 (`file_catervafication_epic`) becomes `{ "method": "command", "command": "test -d docs/catervas" }`, run on the integration branch once its tasks are merged.
  - A template whose owner changes code (`change_request` for the Developer's or the Designer's folder) keeps its `review` criterion.
- **Role prompts.** Every sentence in these files that says a non-code task is reviewed, or tells the author to give one a `review` or `human` criterion, says instead that Catervas checks its criteria and that only code is reviewed:
  - `finance_specialist/system.md` and `skills/keeping-the-books/SKILL.md`;
  - `procurement_specialist/system.md`, `skills/sourcing-a-product/SKILL.md` and `skills/defining-the-need/SKILL.md`;
  - `product_manager/system.md`, and its skills `writing-task-contracts`, `writing-requirements`, `catervafying-the-repository` and `using-product-sources`;
  - `architect/system.md` (it still reviews code), and its skills `planning-in-lanes` and `catervafying-the-repository`;
  - `marketing_specialist/skills/marketing-what-ships/SKILL.md` and `skills/catervafying-the-repository/SKILL.md`;
  - `scrum_master/system.md` and `skills/keeping-work-flowing/SKILL.md`.

  All are under `crates/roles/roles/`. The Developer's and the Designer's files are unchanged.
- **The task page.** For a task that `!reviewedByAnAgent` (the same rule in TypeScript), the lead's `taskReviews` part ("{name} reviews it", `TaskDetail.tsx:206`) becomes `taskChecked` ("Catervas checks it"), and the summary's signature of a review note `written_by: catervas` (line 371) uses `gateChecked` ("Catervas checked it") in place of `gateReviewed`.

## File map

```
crates/core/src/team.rs                         modifies: reviewed_by_an_agent (Task 1)
crates/core/src/folders.rs                      modifies: is_plan_task (Task 1; consumed by plan 3)
crates/core/src/governor/done.rs                modifies: catervas_verifies_alone (Task 1); results need no human (Task 2)
crates/core/src/governor/transition_table.rs    modifies: the two rows, its three table tests (Task 1)
crates/core/src/governor/transition.rs          modifies: check_gate (Task 1); result callers (Task 2)
crates/core/src/governor/readiness.rs, plain.rs modifies: NoReviewOutsideCode, private_folder_task's words (Task 2)
crates/core/src/governor/gates.rs               modifies: result_needs_the_human's caller (Task 2)
crates/runtime/src/orchestrator/verify.rs       modifies: verify_alone; tests (Task 3)
crates/runtime/src/orchestrator/requests.rs     modifies: verifying_epic's governor ending; tests (Task 3)
crates/runtime/src/transitions.rs, orchestrator/human.rs, crates/store/src/waiting.rs   result callers (Task 2 or 3, where they break)
crates/store/src/requests.rs, crates/runtime/src/catervafication.rs   templates (Task 4)
crates/roles/roles/** (the files above)         prompts (Task 4)
apps/web/src/pages/TaskDetail.tsx, apps/web/src/strings/en.ts, TaskDetail.test.tsx    (Task 5)
```

## Interfaces

Consumes (on `main`): `changes_code`, `task_private_folder`, `wire_method`, `evaluate_done`, `check_rejection_reasons`, `run_what_catervas_runs`, `governor_results`, `since_verifying`, `history`, `append`, `context`, `escalate`, `ran_criteria`, `run_on_the_integration_branch`; harnesses `a_harness`, `Harness`, `ticked`.

Produces:

```rust
// catervas_core::team
#[must_use] pub fn reviewed_by_an_agent(contract: &TaskContract) -> bool;
// catervas_core::folders
/// `plan` is set and `allowed_paths` are exactly it: the task writes an epic's plan.
#[must_use] pub fn is_plan_task(contract: &TaskContract) -> bool;
// catervas_core::governor::done
#[must_use] pub fn catervas_verifies_alone(contract: &TaskContract) -> bool;
// catervas_core::governor::readiness
ReadinessRule::NoReviewOutsideCode
// catervas_runtime::orchestrator::verify
pub(super) async fn verify_alone(orchestrator: &Orchestrator, team: &Team, row: &TaskProjection,
    contract: &TaskContract, waiting: &mut Waiting) -> Result<Option<TickReport>, OrchestratorError>;
/// The note Catervas writes for a task it verified alone, from its latest results.
pub(super) fn catervas_note(results: &[CriterionResult]) -> String;
```

```ts
// apps/web/src/pages/TaskDetail.tsx
export function reviewedByAnAgent(contract: Contract): boolean;
```

## Tasks

### Task 1: the governor may verify what no agent reviews

Files:
- `crates/core/src/team.rs` and `crates/core/src/folders.rs`: the new functions.
- `crates/core/src/governor/done.rs`: `catervas_verifies_alone`.
- `transition_table.rs`:
  - add the two rows;
  - `has_exactly_twenty_two_distinct_rows` becomes `has_exactly_twenty_four_distinct_rows`;
  - `has_eighteen_specific_rows_and_four_any_rows` becomes `has_twenty_specific_rows_and_four_any_rows`;
  - `matches_every_line_of_the_spec_table` gains the two pairs.
- `transition.rs`: `check_gate`, for `actor == Governor` on `DefinitionOfDone` or `RejectionReasons`, gives the refusal above unless `catervas_verifies_alone`, then the gate as today. In `mod tests`:
  - `every_row` becomes `[Case; 24]`, the rows renumbered in table order, with two cases added: `Verifying -> Accepted` and `Verifying -> Rejected` by `A::Governor`. Their `prepare` makes the contract an Architect's docs task, with `allowed_paths` `docs/catervas/architecture/overview.md`, one `artifact` criterion holding the id `a_result` records, and `done.changed_paths` that path. For the rejection, it sets `rejection` to that criterion with written reasons.
  - `refuses_every_row_of_the_table_to_an_actor_it_does_not_name` asks a `Governor` row from `verifying` as `A::Assignee`, since the reviewer shares `verifying -> rejected`.
  - These two fixes were found in the readiness round of 2026-10-16 on the earlier plan.

Tests:
- `the_governor_verifies_only_what_no_agent_reviews` (`transition.rs`). Take a verifying Architect task with one `artifact` criterion and passing done evidence:
  - `ask(Accepted, A::Governor, None)` decides on the governor's row;
  - with rejection reasons, `ask(Rejected, A::Governor, None)` decides on the governor's row;
  - the same contract assigned to `software_developer`, and the Architect's with a `review` criterion added, are each refused on both moves with `GateFailed`, and details exactly the refusal sentence.
- `is_plan_task` and `reviewed_by_an_agent` are covered by the assertion above and by plan 3.

- [ ] `feat(core): let the governor accept or reject what no agent reviews`

### Task 2: no result waits on the human, and no review outside code

Files:
- `done.rs`, `transition.rs`, `gates.rs`: the removals of Decisions. Their callers in `crates/runtime/src/transitions.rs`, `orchestrator/human.rs`, `orchestrator/verify.rs`, `orchestrator/requests.rs` and `crates/store/src/waiting.rs` stop asking; an `Acceptance` waiting row is listed only for a task with an unanswered `human` criterion.
- `readiness.rs`: `no_review_outside_code` in `CHECKS` after `criteria_methods_valid`, and the `private_folder_task` wording.
- `plain.rs`: the plain words.

Tests:
- `refuses_review_and_human_criteria_outside_code` (`readiness.rs`):
  - a Product Manager docs task with a `review` criterion fails `NoReviewOutsideCode` with the message above naming its id;
  - an epic with a `human` criterion fails it too;
  - a Developer's task with both passes it;
  - a Finance Specialist's private-folder task with only an `artifact` criterion passes every rule.
- `accepts_an_epic_and_a_high_risk_task_without_the_human` (`done.rs`, replacing the tests that asserted `HumanAccepted` failures, e.g. those at lines 791, 886, 927): an epic, a `high` risk task and a task whose `allowed_paths` name `docs/catervas/product/spec.md`, each with passing evidence and `human_accepted: false`, pass `evaluate_done`.

- [ ] `feat(core): review only code, and let no result wait on the human`

### Task 3: Catervas verifies a task and an epic with no session

Files:
- `crates/runtime/src/orchestrator/verify.rs`. In `verifying`, after the epic branch and before the reviewer lookup, `if catervas_verifies_alone(&contract) { return verify_alone(..) }`. `verify_alone` does, in order:
  1. checks `spent`;
  2. calls `run_what_catervas_runs`, handling its three outcomes as `verifying` does;
  3. takes the latest governor result per criterion (`governor_results`); while any criterion lacks one, returns `ran_criteria`;
  4. with no review note since work began, appends `catervas_note`;
  5. when any criterion failed, files `to: Rejected, actor: Governor, agent_id: None` with `Rejection { failed_criterion_ids, reasons: note }`;
  6. otherwise files `to: Accepted, actor: Governor`.

  Its reports are `accepted it on its criteria, which Catervas ran: only code is reviewed`, `rejected it for <ids>, which Catervas ran`, or `the governor would not <accept|reject> it: <details>`.
- `requests.rs`: `verifying_epic` ends the same way after its integration-branch runs, when `catervas_verifies_alone(contract)`.
- Tests that pinned a Product Manager's review or acceptance of a non-code task, or the human's of an epic, change to this flow here. For example, in `requests.rs`: `reviewed_by_the_product_manager` cases, 2945, 3187, 3682 and 3923; and the `rules.rs` tests from 4717 to 4827.
- Tests are `#[ignore = "needs the git program: cargo xtask check --integration"]`.

Tests:
- `catervas_verifies_a_task_no_agent_reviews` (`verify.rs`, with `a_harness`):
  - An Architect's task CTV-2 (reviewer `pm`, one `artifact` criterion C1 on `docs/catervas/architecture/overview.md`) is verifying with the file committed on `docs/CTV-2` and a completion note. Two ticks start no session. CTV-2 is `accepted`. The log holds a `criterion.recorded` by `governor`, a `note.written` of kind `review` with `written_by` `catervas` and no `agent_id`, and a `task.transitioned` to `accepted` with `actor` `governor`.
  - In a second harness with nothing committed, CTV-2 is `rejected` by `governor` with C1 failed, and no session starts.
  - A Finance Specialist's task in its private folder with an `artifact` criterion on `books.xlsx` is accepted the same way.
- `catervas_accepts_an_epic_once_its_tasks_are_integrated` (`requests.rs`): an epic in `verifying` whose one task is accepted and integrated, with a `command` criterion `true`, is accepted by `governor` with no session. While its task still awaits integration, nothing happens.

- [ ] `feat(runtime): accept what no agent reviews on Catervas's own runs`

### Task 4: what Catervas files whole carries machine criteria

Files: `crates/store/src/requests.rs` and `crates/runtime/src/catervafication.rs` (the templates of Decisions), and the role files of Decisions. Tests that read the templates' criteria change here.

Tests:
- `whole_filed_contracts_need_no_review` (`crates/store/src/requests.rs`): for each owner role that does not change code (Product Manager, Architect, Marketing Specialist), the contracts of `rederive_request`, `product_plan_request`, `change_request` and `rewrite_request` validate (`validate_contract`) and hold no `review` or `human` criterion, and each criterion is an `artifact` on a path of `allowed_paths`; `change_request` for `software_developer` keeps its `review` criterion; `file_catervafication_epic`'s criterion is the `command` `test -d docs/catervas`.

- [ ] `feat(store): file whole contracts with machine criteria outside code`

### Task 5: the task page says Catervas checks it

Files:
- `apps/web/src/pages/TaskDetail.tsx`: `reviewedByAnAgent` (`kind === "task"` and `assigneeRole` is `software_developer` or `ui_ux_designer`), the lead and the signature.
- `apps/web/src/strings/en.ts`: `taskChecked` "Catervas checks it" and `gateChecked` "Catervas checked it".
- `TaskDetail.test.tsx`.

Tests:
- `says_catervas_checks_what_no_agent_reviews`:
  - An Architect's task's lead holds "Catervas checks it" and no "reviews it".
  - Its summary, with a `note.written` review by `catervas`, shows "Catervas checked it".
  - A Developer's task's lead still holds "Ada reviews it".
  - No axe violations.

- [ ] `feat(web): say Catervas checks a task no agent reviews`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
