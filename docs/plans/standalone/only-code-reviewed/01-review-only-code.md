# Standalone 1 of 8: review only code

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.1, 5.2, 5.3, 5.4, 5.16 items 3 and 4; F9
Depends on: `origin/main` at `74b4a5af`; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (second round on the two decisions the first found unmade, settled by the controller the same day)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008). One test per major functionality (ADR 0060). A test that pins the old behaviour is changed in the task that changes the behaviour, never skipped.

## Goal

A task that does not change code is accepted or rejected by the governor on the criteria Catervas runs (`command`, `test`, `artifact`), with no reviewer session and no Product Manager acceptance session. This covers an epic, a plan, a document, research, and the Marketing, Finance and Procurement Specialists' tasks. A code task (`reviewed_by_an_agent`) is reviewed exactly as today. Readiness refuses `review` and `human` criteria outside code, and every contract Catervas files whole carries machine criteria. No result waits on the human except a code task's `human` criterion and an older epic Catervas does not verify alone.

Out of scope:
- integration, which is plan 2;
- the plan's tasks and lanes, which are plan 3;
- the contract's approval, which is plan 5;
- contract judging (plan 7).

## Decisions

- **"Changes code"** is `reviewed_by_an_agent(contract) = kind == Task && changes_code(assignee_role)` (the README says why).
- **Catervas verifies alone** a contract that `catervas_verifies_alone` holds: `!reviewed_by_an_agent` and every exit criterion is `command`, `test` or `artifact`. A contract that is not reviewed but still carries a `review` or `human` criterion (an older one, written before readiness refused them) takes the old path unchanged, so old logs finish.
- **The reviewer field** stays as `fill_reviewer_role` and the assignment gate set it, and is never run for such a task. Chosen over clearing it, which would change `ReviewerAvailable`, the assignment gate, `default_reviewer_role` and every fixture for no behaviour. `PrivateFolderReviewer` stays for the same reason.
- **Two governor rows**:
  - `verifying -> accepted` (`Governor`, `DefinitionOfDone`), right after the Product Manager's row (table index 14);
  - `verifying -> rejected` (`Governor`, `RejectionReasons`), right after the reviewer's row and before the human's (index 16).

  `TRANSITION_TABLE` becomes `[TransitionRow; 24]`. `check_gate` refuses both, when `!catervas_verifies_alone`, with the detail `Catervas accepts or rejects on its own only a task that changes no code and whose every criterion it runs (ADR 0061)`. Agents cannot ask as `Governor` (`actor_for`, `crates/runtime/src/tools/work.rs:390`).

  This was chosen over filing the move in an agent's name, which would put a false actor in the log.
- **The Definition of Done keeps every rule but `HumanAccepted`.** Catervas writes the review note itself as `note.written { kind: review, written_by: "catervas" }`, with no agent and no session on the envelope. Its text is `Catervas ran every criterion of this task itself; only a code change is reviewed by an agent (ADR 0061).`, then a blank line, then one line per criterion, `<id>: passed|failed: <evidence>`. No `review.recorded` is written. Catervas's runs are recorded as today (`criterion.recorded { run_by: reviewer, recorded_by: "governor" }`), so `CriterionRunByReviewer` holds.
- **Results need no human.** Removed with their callers:
  - `result_needs_the_human`;
  - `DoneRule::HumanAccepted` and its check `human_accepted` (`CHECKS` becomes `[Check; 10]`);
  - the result half of `requires_human_acceptance` (its doc comment; the function stays, used by the contract gate alone until plan 5).

  `result_awaits_human` stays and narrows to `status == Verifying && !catervas_verifies_alone(contract) && (kind == Epic || any human criterion)`: a code task's `human` criterion, an older non-code contract that still carries one, and an older epic that takes the old path. A `high` risk task and a task that may change the product plan no longer wait. Its callers (`transitions.rs:633`, `human.rs:1677` and `:1757`, `waiting.rs:442`) keep calling it, so the `Acceptance` row on Today, the human's acceptance and the human's send-back follow the narrowed rule with no change of their own. `DoneRule::HumanCriterionAccepted` stays. `HumanRejection` stays in the table, its gate unchanged. `verify.rs:125`'s wait becomes `contract.exit_criteria.iter().any(is_human) && !context.done.human_accepted`.
- **An epic** is verified by `verifying_epic` as today up to its criteria: it waits for its tasks' integration, then runs its mechanical criteria on the integration branch. After that, when `catervas_verifies_alone(epic)` holds, the governor writes the note and accepts or rejects, with no Product Manager session and no human. Otherwise the old path runs.
- **Readiness rule `NoReviewOutsideCode`** (`no_review_outside_code`), placed after `CriteriaMethodsValid` in the enum, in `CHECKS` (which becomes `[Check; 31]`) and in `plain.rs`'s `EVERY_RULE` and `listed`. For a contract that `!reviewed_by_an_agent` with any `review` or `human` criterion, it refuses: `<ids> ask for a review or the human, and only a code change is reviewed (ADR 0061): use command, test or artifact criteria`, `<ids>` as `listed("criterion", "criteria", ..)` words them. The plain words, in `plain.rs`, are "This task asks for a review, and only code is reviewed. Check it with a command, a test or a file instead." `private_folder_task`'s "use artifact, review and human criteria" becomes "use artifact criteria".
- **Required criteria follow the same rule.** `required_criteria_present` skips a required `review` or `human` method for a contract that `!reviewed_by_an_agent`, as it already skips `command` and `test` for a task that runs none. Otherwise a team whose rules require `review` (as `team.rs:1643`'s fixture does) could never ready a non-code task.
- **Whole-filed templates** use only machine criteria when their assignee's role does not change code. Each criterion is `{ "id": "C<n>", "text": "<path> exists.", "satisfies": [..], "verification": { "method": "artifact", "path": <path> } }`, numbered C1 onwards in `allowed_paths` order:
  - `rederive_request` (`crates/store/src/requests.rs:257`): one, on `twin`, satisfying `R1`.
  - `product_plan_request` (`:367`): one per allowed path: `spec.<x>` satisfies `R1`, `roadmap.<x>` `R2`, and each `.agent.<x>` file `R3`.
  - `change_request` (`:504`): one per path of `paths` (the document, then its twin), satisfying `R1`.
  - `rewrite_request` (`:551`): its allowed path is the glob `<folder>/**`, so it adds none. Its per-document `artifact` criteria stay and its last, `review` criterion goes.
  - Every task of `crates/runtime/src/catervafication.rs` (`task_contract`): its last, `review` criterion goes; its path criteria stay.
  - The Marketing row, which has no documents (`row`'s `(_, Role::MarketingSpecialist)`), gets in their place one criterion (the controller, 2026-10-16): `{ "id": "C1", "text": "A marketing plan is written in docs/catervas/marketing/plans/", "satisfies": ["R1"], "verification": { "method": "command", "command": "ls docs/catervas/marketing/plans/MP-* >/dev/null 2>&1", "expect": { "exit_code": 0 } } }`. A `command` is allowed here: no readiness rule refuses one on a Marketing task (`private_folder_task` holds only the Finance and Procurement Specialists, `EpicCommandInProjectFolder` only epics), Catervas runs it in the task's sandbox from the root of its repository through `sh -c`, so the glob expands, and `catervas_propose_marketing_plan` writes `MP-<n>` into the task's worktree (`tools/marketing.rs:265`), committed on the task's branch before `verifying`.
  - The Catervafication epic's C1 (`epic_contract`, `catervafication.rs:171`), in a single repository and in a project folder alike (the controller, 2026-10-16), becomes `{ "method": "artifact", "path": "docs/catervas/product/spec.<x>", "must_contain": [<heading>] }`, `<x>` the team format's extension and `<heading>` the format's heading mark (`artifact_marks`: `"# "` for Markdown, `"<h1"` for HTML); its `id`, `text` and `satisfies` stay. `epic_contract` gains the format: `epic_contract(project: Project, max_cost_usd: f64, format: DocumentFormat) -> Value`, its caller `start` passing `team.document_format()` and its tests (`catervafication.rs:709`, `:772`, `requests.rs:2156`) `DocumentFormat::Markdown`. This holds:
    - `EpicCommandInProjectFolder` refuses only `command` and `test` criteria (`runs_a_command`, `readiness.rs:688`), not an `artifact`.
    - An epic's mechanical criteria run on the integration branch (`run_on_the_integration_branch`, `requests.rs:861`), in a detached worktree of `deps.tools.git`, which in a project folder is the project folder's own repository, where `docs/catervas/` lives (ADR 0057); an `artifact` is read there with `cat`.
    - The spec is always there by then: an existing repository's Catervafication always has the Product Manager's task (`catervafication_start_for` refuses otherwise), and a new project's starts after its product plan.
    - A spec kept in the other format (a team that changed its format afterwards) fails the criterion and sends the epic back; that is accepted, as the owner chose the format.
  - A template whose owner changes code (`change_request` for the Developer's or the Designer's folder) keeps its `review` criterion.
- **Role prompts.** Every sentence in these files that says a non-code task is reviewed, or tells the author to give one a `review` or `human` criterion, says instead that Catervas checks its criteria and that only code is reviewed:
  - `finance_specialist/system.md` and `skills/keeping-the-books/SKILL.md`;
  - `procurement_specialist/system.md`, `skills/sourcing-a-product/SKILL.md` and `skills/defining-the-need/SKILL.md`;
  - `product_manager/system.md`, and its skills `writing-task-contracts`, `writing-requirements`, `catervafying-the-repository` and `using-product-sources`;
  - `architect/system.md` (it still reviews code), and its skills `planning-in-lanes` and `catervafying-the-repository`;
  - `marketing_specialist/skills/marketing-what-ships/SKILL.md` and `skills/catervafying-the-repository/SKILL.md`;
  - `scrum_master/system.md` and `skills/keeping-work-flowing/SKILL.md`.

  All are under `crates/roles/roles/`. The Developer's and the Designer's files are unchanged.
- **The task page.** For a task that `!reviewedByAnAgent` (the same rule in TypeScript: `contract.kind !== "epic"` and `assigneeRole` is `software_developer` or `ui_ux_designer`), the lead's `taskReviews` part ("{name} reviews it", `TaskDetail.tsx:206`) becomes `taskChecked` ("Catervas checks it"). The summary (`TaskDetail.tsx:367`) also renders when its note's `writtenBy` is `catervas`, which `agentOf` does not find, and then signs it with `t("gateChecked")` ("Catervas checked it") in place of `signed("gateReviewed", ..)`. `riskHighWhy` ("High. You approve its plan, and you accept the work yourself.") becomes "High. You approve its plan." (plan 5 removes the plan's approval and must change it again).

## File map

```
crates/core/src/team.rs                         modifies: reviewed_by_an_agent (Task 1)
crates/core/src/governor/done.rs                modifies: catervas_verifies_alone (Task 1); results need no human (Task 3)
crates/core/src/governor/transition_table.rs    modifies: the two rows, its three table tests (Task 1)
crates/core/src/governor/transition.rs          modifies: check_gate (Task 1)
crates/store/src/requests.rs, crates/runtime/src/catervafication.rs   templates (Task 2)
crates/roles/roles/** (the files above)         prompts (Task 2)
crates/core/src/governor/readiness.rs, plain.rs modifies: NoReviewOutsideCode, required criteria, private_folder_task's words (Task 3)
crates/runtime/src/orchestrator/verify.rs       modifies: verify.rs:125's wait (Task 3); verify_alone; tests (Task 4)
crates/runtime/src/orchestrator/rules.rs        tests: the human's acceptance of a high risk or product plan task (Task 3)
crates/runtime/src/orchestrator/requests.rs     modifies: verifying_epic's governor ending; tests (Task 4)
apps/web/src/pages/TaskDetail.tsx, apps/web/src/strings/en.ts, TaskDetail.test.tsx    (Task 5)
```

## Interfaces

Consumes (on `main`): `changes_code`, `task_private_folder`, `wire_method`, `evaluate_done`, `check_rejection_reasons`, `run_what_catervas_runs`, `governor_results`, `since_verifying`, `history`, `append`, `context`, `escalate`, `ran_criteria`, `run_on_the_integration_branch`, `spent`; harnesses `a_harness`, `Harness`, `ticked`.

Produces:

```rust
// catervas_core::team
#[must_use] pub fn reviewed_by_an_agent(contract: &TaskContract) -> bool;
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
- `crates/core/src/team.rs`: `reviewed_by_an_agent`.
- `crates/core/src/governor/done.rs`: `catervas_verifies_alone`.
- `transition_table.rs`:
  - add the two rows;
  - `has_exactly_twenty_two_distinct_rows` becomes `has_exactly_twenty_four_distinct_rows`;
  - `has_eighteen_specific_rows_and_four_any_rows` becomes `has_twenty_specific_rows_and_four_any_rows`;
  - `matches_every_line_of_the_spec_table` gains `(A::Governor, G::DefinitionOfDone)` after the Product Manager's pair of `verifying -> accepted`, and `(A::Governor, G::RejectionReasons)` between the reviewer's and the human's pairs of `verifying -> rejected`.
- `transition.rs`: `check_gate`, for `actor == Governor` on `DefinitionOfDone` or `RejectionReasons`, gives the refusal above unless `catervas_verifies_alone`, then the gate as today. In `mod tests`:
  - `every_row` becomes `[Case; 24]`, the rows renumbered in table order, with two cases added: `Verifying -> Accepted` and `Verifying -> Rejected` by `A::Governor`. Their `prepare` makes the contract an Architect's docs task (`assignee_role` `architect`), with `allowed_paths` `docs/catervas/architecture/overview.md`, one `artifact` criterion with the id `C1` that `a_result` records, and `done.changed_paths` that path. `a_context`'s `rejection` already names `C1` with written reasons.
  - `refuses_every_row_of_the_table_to_an_actor_it_does_not_name` asks a `Governor` row from `verifying` as `A::Assignee`, since the reviewer shares `verifying -> rejected`.
  - These two fixes were found in the readiness round of 2026-10-16 on the earlier plan.

Tests:
- `the_governor_verifies_only_what_no_agent_reviews` (`transition.rs`). Take a verifying Architect task with one `artifact` criterion and passing done evidence:
  - `ask(Accepted, A::Governor, None)` decides on the governor's row;
  - with rejection reasons, `ask(Rejected, A::Governor, None)` decides on the governor's row;
  - the same contract assigned to `software_developer`, and the Architect's with a `review` criterion added, are each refused on both moves with `GateFailed`, one failure each, whose details are exactly `[the refusal sentence]`.
- `reviewed_by_an_agent` is covered by the assertion above.

- [x] `feat(core): let the governor accept or reject what no agent reviews`

### Task 2: what Catervas files whole carries machine criteria

Files: `crates/store/src/requests.rs` and `crates/runtime/src/catervafication.rs` (the templates of Decisions), and the role files of Decisions. Tests that read the templates' criteria change here: in `requests.rs`, `writes_the_product_plans_contract` (1411: four criteria, each an `artifact`, in place of two `review`s; its `result_needs_the_human` assertion stays until Task 3); in `catervafication.rs`, `writes_each_template_as_the_table_says` (785) and `every_template_passes_the_definition_of_ready` (704).

This task comes before Task 3, so that no template Catervas files is refused by `NoReviewOutsideCode`. Until Task 4, such a task still goes through its reviewer's session, which reviews Catervas's runs as it does today.

Tests:
- `whole_filed_contracts_need_no_review` (`crates/store/src/requests.rs`): for each owner role that does not change code (Product Manager, Architect, Marketing Specialist), the contracts of `rederive_request`, `product_plan_request`, `change_request` and `rewrite_request` validate (`validate_contract`) and hold no `review` or `human` criterion, and each criterion is an `artifact` whose path is one of `allowed_paths` or, for `rewrite_request`, lies under its folder; `change_request` for `software_developer` keeps its `review` criterion.
- In `writes_each_template_as_the_table_says`: `epic_contract`'s criterion, for Markdown and for HTML, is the `artifact` of Decisions (`docs/catervas/product/spec.md` with `"# "`, `docs/catervas/product/spec.html` with `"<h1"`); the Marketing task's one criterion is the `command` of Decisions; and in `every_template_passes_the_definition_of_ready` the epic also passes with `repos` set (a project folder).

- [x] `feat(store): file whole contracts with machine criteria outside code`

### Task 3: no result waits on the human, and no review outside code

Files:
- `done.rs`, `transition.rs`: the removals and the narrowed `result_awaits_human` of Decisions.
- `verify.rs:125`: the narrowed wait.
- `readiness.rs`: `no_review_outside_code` in `CHECKS` after `criteria_methods_valid`, the required-criteria skip, and the `private_folder_task` wording.
- `plain.rs`: the plain words, `EVERY_RULE` (33 rules) and `listed`.
- Tests that pinned the removed behaviour change here:
  - in `done.rs`, `the_human_accepts_a_task_that_may_change_the_product_plan` (886) and `needs_the_human_for_a_high_risk_task_and_for_every_epic` (927) give way to the test below, and `reports_every_failure_in_rule_order` (953) no longer lists `R::HumanAccepted`;
  - in `crates/runtime/src/orchestrator/rules.rs`, `leaves_a_high_risk_task_for_the_human` (4717), `accepts_a_high_risk_task_after_the_human` (4741), `leaves_a_task_that_may_change_the_product_plan_for_the_human` (4770) and `accepts_it_after_the_human` (4796) become one test, `accepts_a_high_risk_or_product_plan_task_without_the_human`, in which each goes from the reviewer's session to the Product Manager's `verify` session and `accepted` with no human. `leaves_a_task_with_a_human_criterion_for_the_human` (4827) is unchanged;
  - `crates/store/src/requests.rs:1448`'s `result_needs_the_human` assertion goes;
  - any other test whose contract is a non-code task or an epic with a `review` or `human` criterion, and that expects it to pass readiness, gives it an `artifact` criterion instead.

Tests:
- `refuses_review_and_human_criteria_outside_code` (`readiness.rs`):
  - a Product Manager docs task with a `review` criterion fails `NoReviewOutsideCode` with the message above naming its id;
  - an epic with a `human` criterion fails it too;
  - a Developer's task with both passes it;
  - a Finance Specialist's private-folder task (`reviewer_role` `product_manager`) with only an `artifact` criterion passes every rule;
  - with team rules requiring `review`, the Product Manager's docs task with only an `artifact` criterion passes `RequiredCriteriaPresent`, and a Developer's task with no `review` criterion still fails it.
- `accepts_an_epic_and_a_high_risk_task_without_the_human` (`done.rs`): an epic, a `high` risk task and a task whose `allowed_paths` name `docs/catervas/product/spec.md`, each with passing evidence and `human_accepted: false`, pass `evaluate_done`.

- [ ] `feat(core): review only code, and let no result wait on the human`

### Task 4: Catervas verifies a task and an epic with no session

Files:
- `crates/runtime/src/orchestrator/verify.rs`. In `verifying`, after the epic branch and before the reviewer lookup, `if catervas_verifies_alone(&contract) { return verify_alone(..) }`. `verify_alone` does, in order:
  1. checks `spent`, returning `None` when it is spent;
  2. calls `run_what_catervas_runs`, handling its three outcomes as `verifying` does;
  3. reads the history again and takes the latest governor result per criterion (`governor_results`); while any criterion lacks one, returns `ran_criteria`;
  4. with no review note since work began, appends `catervas_note` (`append`, no agent, no session);
  5. when any criterion failed, files `to: Rejected, actor: Governor, agent_id: None` with `TransitionAsk { rejection: Some(Rejection { failed_criterion_ids, reasons: <the note> }), .. }`;
  6. otherwise files `to: Accepted, actor: Governor, agent_id: None`.

  Its `TickReport::Acted` words are `accepted it on its criteria, which Catervas ran: only code is reviewed`, `rejected it for <ids>, which Catervas ran`, or `the governor would not <accept|reject> it: <details>` (`refusal_details` joined with `; `).
- `requests.rs`: `verifying_epic` ends the same way after its integration-branch runs, when `catervas_verifies_alone(contract)`, before `the_human_reviews_the_epic`.
- Tests that pinned a Product Manager's review or acceptance of a non-code task, or the human's of an epic whose criteria Catervas now runs alone, change to this flow here. For example, in `requests.rs`: the tests of `reviewed_by_the_product_manager`, `hands_the_humans_acceptance_to_the_product_manager` (2945), `records_an_epics_review_once_with_its_human_criterion` (3187), `refuses_the_humans_acceptance_of_an_epic_before_catervas_ran_its_criteria` (3682) and `keeps_a_human_reviewed_epic_with_the_human_once_a_scrum_master_activates` (3923). An epic test whose epic keeps a `review` or `human` criterion keeps the old path and its assertions.
- Tests are `#[ignore = "needs the git program: cargo xtask check --integration"]`.

Tests:
- `catervas_verifies_a_task_no_agent_reviews` (`verify.rs`, with `a_harness`):
  - An Architect's task CTV-2 (reviewer `pm`, one `artifact` criterion C1 on `docs/catervas/architecture/overview.md`) is verifying with the file committed on `docs/CTV-2` and a completion note. Two ticks start no session. CTV-2 is `accepted`. The log holds a `criterion.recorded` by `governor`, a `note.written` of kind `review` with `written_by` `catervas` and no `agent_id`, and a `task.transitioned` to `accepted` with `actor` `governor`.
  - In a second harness with nothing committed, CTV-2 is `rejected` by `governor` with C1 failed, and no session starts.
  - A Finance Specialist's task in its private folder with an `artifact` criterion on `books.xlsx` (the file in `.catervas/local/finance/`) is accepted the same way.
- `catervas_accepts_an_epic_once_its_tasks_are_integrated` (`requests.rs`): an epic in `verifying` whose one task is accepted and integrated, with the criterion `{ "method": "command", "command": "true", "expect": { "exit_code": 0 } }`, is accepted by `governor` with no session. While its task still awaits integration, a tick does nothing to it.

- [ ] `feat(runtime): accept what no agent reviews on Catervas's own runs`

### Task 5: the task page says Catervas checks it

Files:
- `apps/web/src/pages/TaskDetail.tsx`: `reviewedByAnAgent`, the lead, the summary's condition and signature.
- `apps/web/src/strings/en.ts`: `taskChecked` "Catervas checks it", `gateChecked` "Catervas checked it", and `riskHighWhy`.
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
