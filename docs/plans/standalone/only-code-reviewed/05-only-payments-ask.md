# Standalone 5 of 8: only payments ask

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 4.1, 4.3, 5.2, 5.3, 5.6, 5.7, 5.12, 5.16 item 2, 5.17, 6.5, 6.7, 6.10; F9, F12
Depends on: plans 1 to 3 committed on this branch; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin a removed gate change in the task that removes it, never skipped. The README's table lists every human gate kept or removed. This plan removes the ones marked plan 5.

Line numbers are those of `74b4a5af`; plans 1 to 4 move some of them, so each test is found by the name given beside it.

## Goal

The owner is asked only before money is spent. These stop asking:
- the approval of an epic or a `high` risk contract;
- the new project's product plan;
- the proposals to change `spec` and `roadmap`;
- a marketing plan that spends nothing;
- a post outside the plan;
- a connector call that costs nothing;
- the Product Manager's decision on a data source.

These are kept: a marketing plan with money in it and its ads, a raised budget, purchase orders, a paid, unknown-cost or data-sending data source, a `paid` connector call within its allowance, and every AI spending limit.

Out of scope:
- seller messages (Catervas has no mailbox);
- questions an agent asks;
- connector and skill confirmation;
- contract judging (plan 7).

## Decisions

- **No contract waits for the human.**
  - `waits_for_the_human` and `human_must_accept_the_contract` (`crates/core/src/governor/transition.rs`) answer false for every contract. The `refining -> escalated` `ContractRequiresHuman` row stays, so older logs read, but never opens.
  - `requires_human_acceptance` goes. `TransitionContext`'s policy input (`required_by_policy`, `transitions.rs:615`, `asks_every_contract`) goes. `human_accepts_contracts` still loads and is ignored.
  - Rule 9's `judge` (`requests.rs`) is unchanged: it still asks `escalated` first, a row that now opens only on `ReadinessExhausted`, then `ready`. Its comments that speak of a contract the human must approve say instead that `escalated` opens on three readiness failures.
  - An `approval` or `risk_gate` escalation already open in a log is still resolved by `human_accept { subject: contract }` (`human.rs` `approve`), which is unchanged.
  - The Definition of Ready's detail "this contract needs the human's acceptance…" goes.
  - **The summary rule goes with the approval.** `ReadinessContext::human_approves` (`readiness.rs:178`, set at `transitions.rs:766`) goes, and with it the rule `summary_present` (`ReadinessRule::SummaryPresent` and its words in `plain.rs`), since no human decides on a plan before work starts. A contract's `summary` stays optional in the schema and is still shown wherever it is given.
  - The Product Manager's `system.md` and `writing-task-contracts` §6 stop saying an epic or a `high` risk task waits for the user's approval, and `writing-task-contracts`' **Summary** bullet (line 45) stops saying an epic needs one for the user's approval page: a summary is optional. The PM still asks its questions first (5.16 item 1).
- **The product plan is the Product Manager's to finish.**
  - Its task is accepted by the governor (plan 1).
  - `Transitions::awaits_the_product_plan` becomes: `project.started` is in the log, and no task created with `product_plan: true` is `accepted`. It no longer reads git. `product_plan_on` (`crates/store/src/git.rs:1159`) goes, with its tests in `crates/store/tests/git.rs` (around 1718, and `product_plan_on_takes_either_format` at 1823), and so does `product_plan_approved` (`crates/core/src/folders.rs:216`) with its tests (around 879 and 1621), since nothing else reads them.
  - `drafts_the_plan` (`crates/store/src/product_plan.rs`) answers false for an `accepted` task, integrated or not, as for a `cancelled` one.
  - `catervafy_a_new_project` (`rules.rs:309`) starts on that acceptance. `catervafication::start` no longer records `human.accepted { subject: contract }` for the epic, for either caller (the owner's Start and a new project), since no epic waits for approval: it loses its `note` parameter, becoming `start(deps: &ToolDeps, team: &Team, project: Project, by: &str) -> Result<Started, CatervaficationError>`, and `plan_acceptance_note` (`rules.rs`) goes.
  - `product_plan.state` (`daemon/gates.rs:615`) answers `approved` = that task is accepted.
  - `ProductPlanCard.tsx`, `ProductPlanCard.module.css`, `ProductPlanCard.test.tsx` and the card's place on Today go. The type `ProductPlanState` moves from `ProductPlanCard.tsx` to `apps/web/src/app/productPlan.ts`, from which `Today.tsx` (whose interview band still reads `product_plan.state`) and `OneToOne.tsx` import it.
  - The Product Manager's chat still interviews the owner (4.3); `Draft the product plan` stays.
- **The Catervafication's Marketing task** is filed with `dependencies: [<the Product Manager's task>]`:
  - for an existing repository, the PM's Catervafication task;
  - for a new project, the product plan's task.

  It is filed when the others are. `FileDue::MarketingWaits` goes, because plan 3 lets a dependency count once accepted and starts the task from its branch.
- **Spec and roadmap change at once.**
  - `write_folder_doc` (`crates/runtime/src/tools/folders.rs:398`) no longer proposes: `checked.proposal` is never set. A change to `docs/catervas/product/spec` or `roadmap` lands as a folder change like any other document, and so reaches a pull request (plan 2).
  - `propose` (`folders.rs:633`), and the sprint review's words asking the PM to propose (`messages.rs` `product_refresh_asks`), say "write" in place of "propose".
  - `folder_doc_decide` (`human.rs:2037`) and `DocumentChanges.tsx` stay, for proposals already open in a log.
  - `write_folder_doc` lands a change to `spec` or `roadmap` as one folder change of both files, the human document and its agent version (`agent_text`), as `folder_doc.approved` did. `agent_text` stays required for them. `summary` is no longer required: given or not, it is not recorded, and the input's description of `summary` says so.
  - They are still written at the sprint review alone (`folder_doc_at_review`, whose words become "{path} is written at the sprint review, not at another ceremony"), so no ceremony's scheduling changes.
  - `OWNER_ACCEPTED_STEMS` (`crates/core/src/folders.rs:203`) stays: it still names the human documents an agent writes with `catervas_write_folder_doc` (`check_folder_doc_path`), that keep one format (`check_document_format`) and that the owner edits (`check_owner_edit`). `FolderDocPath::owner_accepted` keeps its name.
  - `may_change_an_owner_document` (`folders.rs:232`) goes, its last caller (`result_needs_the_human`) gone in plan 1; the assertions on it in `folders.rs`' tests (around 1541), `crates/roles/src/lib.rs:1624` and `crates/runtime/src/daemon/documents.rs:2263` go with it.
  - The Product Manager's `skills/catervafying-the-repository/SKILL.md` (lines 49 to 52) says it writes the sprint's changes to the spec and the roadmap at the sprint review, and no longer that the owner approves a proposal.
- **A marketing plan that spends nothing is approved at once.** In `catervas_propose_marketing_plan` (`tools/marketing.rs:230`), when the plan's total budget and every channel's and campaign's budget are zero, Catervas records `marketing_plan.approved { plan, note: "Catervas approved it: it spends nothing (ADR 0061)." }` right after `marketing_plan.proposed`. The envelope carries no agent and no session, as the owner's approval does.

  A plan with any money in it waits for the owner as today, and so do its ads and their hard stop. The schema's description of `marketingPlanApprovedBody` says Catervas approves a plan that spends nothing.
- **A post outside the plan goes out like one inside it.** `schedule_post` (`tools/posts.rs:454`) records `social_post.scheduled { approved_by: "catervas" }` for a post that fills no slot of the active plan, or with no active plan. It is scheduled at least three hours ahead and shown on Today with Stop, as a planned post is.
  - `approved_by` gains `"catervas"` in `docs/schemas/event.schema.json`.
  - A post with no `slot` whose `at` is less than three hours ahead is refused `post_too_soon` ("a post is written at least three hours before it goes out"), as a planned one is. A post that names a `slot` is checked against the plan as today.
  - `no_active_marketing_plan`'s words become "no marketing plan is active, so a post cannot fill a slot; leave slot out to post it outside the plan". The answer's `next` for a post outside the plan is the planned post's.
  - `social_post.requested` is never written again, and `social_post_decide` stays for open requests.
  - The Marketing Specialist's words follow (`crates/roles/roles/marketing_specialist/`): in `system.md` (lines 13, 47 and 48), `role.yaml` (lines 8 and 20) and the skills `writing-the-marketing-plan`, `planning-a-launch`, `marketing-what-ships`, `running-social-channels`, `making-images-and-video`, `keeping-a-content-calendar` and `catervafying-the-repository`, every sentence that says a post outside the plan waits for the owner, or that a plan that spends nothing waits for the owner's approval, or that a call waits for the owner to allow it, says instead: a post outside the plan goes out with Stop on Today, a plan that spends nothing is approved by Catervas, and only a call or a plan that costs money waits for the owner.
- **A connector tool is `paid` when the service charges for the call.**
  - `ConnectorTag` (`crates/core/src/governor/permissions.rs:154`) gains `Paid`, with the wire value `paid`. The connector-tag enum in `docs/schemas/team.schema.json:217` gains `"paid"`, with "paid, it costs money at the service and asks the human".
  - `evaluate_connector_call`: `Paid` asks as `ExternalEffect` asks today, grants and allowances included. `ExternalEffect` runs without asking, recorded as every call is (`tool.called`), with no approval.
  - The hook's `ask` and `external_effect_refused` (`hooks.rs`) apply to `paid`.
  - ADR 0037's allowances may be given only to a `paid` tool (`kit.rs:656`'s `allowance_not_external` becomes `allowance_not_paid`, "only a tool tagged paid has an allowance, and {tool} is not"). `plan_approved` marks need `paid` (`plan_mark_not_external` becomes `plan_mark_not_paid`).
  - `hooks.rs`'s connector branch (around line 785: the plan gate, the grant lookup and the allowance count) and the Google Ads writes' check (`crates/runtime/src/daemon/ads_calls.rs:190`, `tool_not_plan_marked`) test for `ConnectorTag::Paid` in place of `ExternalEffect`, so the ads writes still run only inside the owner's approved plan.
  - A team file's allowance (`allowance_errors`, `crates/core/src/team.rs:496`) is accepted on a kit entry's tool tagged `paid`, and still on one tagged `external_effect`, so that an older team file loads. Its refusal becomes `allowance_not_paid: {tool} is not a tool that costs money, so it has no allowance.` An older kit entry pinned with `external_effect` no longer `matches_kit` (`daemon/team.rs:651`), since the kit now tags those tools `paid`, so it runs nothing until the owner connects it again (ADR 0036), and a paid call never runs unasked.
  - A custom connector's tool the user did not label is `paid`, so a tool whose cost nobody stated still asks: `labelled` (`crates/runtime/src/daemon/team.rs:778`), `ConnectorAdd.tsx`'s default (lines 534 and 969) and `rpc.schema.json`'s description at 5939 say `paid` in place of `external_effect`.
  - The tag enums gain `paid`: `docs/schemas/kit.schema.json:52` (and its allowance and `plan_approved` descriptions at 206 and 216, `allowance_not_paid` and `plan_mark_not_paid`), `docs/schemas/team.schema.json:217` (and its allowance description at 183), `docs/schemas/event.schema.json:1921` (and `toolApprovalRequestedBody`'s description at 1020, which says `paid`), `docs/schemas/rpc.schema.json:6217`, and `ConnectorTagWire`'s mappings (`hooks.rs:1111`, `crates/core/src/team.rs:860`).
  - In the kits (`crates/roles/roles/*/kit.yaml`), every tool tagged `external_effect` becomes `paid`, by server:
    - Marketing: `higgsfield`, `recraft` and `google-ads`;
    - Procurement: `serpapi`.
  - `kit`'s and `github`'s stay `external_effect` and run without asking; `kit`'s allowances are removed.
  - The web: `ConnectorAdd.tsx`'s `Tag` type and `TAGS` gain `["paid", "tagPaid", "tagPaidNote"]`, offered between `network` and `external_effect`, with `tagPaid` "Costs money, asks you" and `tagPaidNote` "{name} asks you before each use, and you see exactly what {name} wants to send." `tagExternal` becomes "Changes things" and `tagExternalNote` "{name} may use it without asking; each use is recorded." `TeamSetup.tsx:40`'s tag type gains `paid`. `KitConnect.tsx`'s done step lists `network` and `external_effect` tools under `kitCan` and `paid` ones under `kitAsks` (line 467). `AgentEdit.tsx`'s `external_effect` row (line 92) is the permission tier, not a tag, and is unchanged.
- **The Product Manager no longer decides a data source.**
  - `pipeline_needs_owner` (`crates/core/src/pipeline.rs:22`) is unchanged: a request that costs money, whose cost is unknown, or that sends the project's data to a third party (`sends_project_data`) asks the owner (the founder, 2026-10-16: "Still ask me").
  - `catervas_request_data_pipeline` (`tools/pipeline.rs:215`) files a free request at once, as `file_the_approved` files an approved one, recording `data_pipeline.approved { by: "catervas", reason: "", request }`. `dataPipelineDecidedBy` gains `"catervas"` in `docs/schemas/event.schema.json`.
  - A request `pipeline_needs_owner` holds is recorded `data_pipeline.escalated { reason }` at once, with no agent and no session on the envelope; `reason` is "It costs money, or its cost is not known, so you decide (ADR 0061)." or, for one that is free and sends data out, "It sends your project's data to another service, so you decide (ADR 0061)."
  - The ordinary request a free source files (`file_the_approved`) has `created_by: "catervas"`.
  - `catervas_decide_data_pipeline` (`decide_data_pipeline`, `tools/pipeline.rs:288`, and its entry in `tools.rs`) goes, and so does its decision session: the module `crates/runtime/src/orchestrator/pipeline.rs` with its tests, its `mod` line (`orchestrator.rs:46`), its rule in the tick (`rules.rs:219`), `decide_data_pipeline_message` (`messages.rs:1178`), `PIPELINE_DECISION_INSTRUCTION` (`prompt.rs:240`), `DECIDE_PIPELINE_TOOL`'s filter and closing arm in `session.rs` (1105 to 1108, 1126), and `SessionAsk::pipeline` with its use in `session.rs:326`. `session.started`'s `pipeline` stays in the protocol, for older logs. `data_pipeline_decide` (`human.rs:1326`) stays the owner's.
  - The Product Manager's skill `deciding-data-pipelines` goes: its folder, its line in `product_manager/kit.yaml:14`, its name in `crates/roles/src/kit.rs:200`, and the `kit.rs` test that reads it (around 2272). The Procurement Specialist's `skills/requesting-a-data-pipeline/SKILL.md` says a free source is filed at once and one that costs money, whose cost is unknown or that sends the project's data out waits for the owner.

## File map

```
crates/core/src/governor/transition.rs, done.rs, readiness.rs, readiness/fixtures.rs, plain.rs; crates/runtime/src/transitions.rs, catervafication.rs (its test at 717); orchestrator/requests.rs; apps/web/src/strings/en.ts (`riskHighWhy`)   contract approval (Task 1)
crates/roles/roles/product_manager/system.md, skills/writing-task-contracts/SKILL.md                                        (Task 1)
crates/runtime/src/transitions.rs, orchestrator/rules.rs, orchestrator/human.rs, catervafication.rs, daemon/gates.rs; crates/store/src/git.rs, product_plan.rs, crates/store/tests/git.rs; crates/core/src/folders.rs; apps/web/src/pages/ProductPlanCard.tsx (.module.css, .test.tsx deleted), Today.tsx, OneToOne.tsx; apps/web/src/app/productPlan.ts (new)   (Task 2)
crates/runtime/src/tools/folders.rs, tools.rs (the input's description), orchestrator/messages.rs, daemon/documents.rs; crates/core/src/folders.rs; crates/roles/src/lib.rs; crates/roles/roles/product_manager/skills/catervafying-the-repository/SKILL.md   (Task 3)
crates/runtime/src/tools/marketing.rs, tools/posts.rs, docs/schemas/event.schema.json; crates/roles/roles/marketing_specialist/{system.md,role.yaml,skills/*/SKILL.md}   (Task 4)
crates/core/src/governor/permissions.rs, crates/core/src/team.rs; crates/runtime/src/daemon/hooks.rs, daemon/ads_calls.rs, daemon/team.rs; crates/roles/src/kit.rs, crates/roles/roles/*/kit.yaml; docs/schemas/kit.schema.json, team.schema.json, event.schema.json, rpc.schema.json; apps/web/src/pages/ConnectorAdd.tsx, KitConnect.tsx, setup/TeamSetup.tsx, apps/web/src/strings/en.ts   (Task 5)
crates/core/src/pipeline.rs; crates/runtime/src/tools/pipeline.rs, tools.rs, orchestrator.rs, orchestrator/pipeline.rs (deleted), orchestrator/rules.rs, orchestrator/messages.rs, orchestrator/session.rs, prompt.rs; crates/roles/src/kit.rs, crates/roles/roles/product_manager/{kit.yaml,skills/deciding-data-pipelines (deleted)}, crates/roles/roles/procurement_specialist/skills/requesting-a-data-pipeline/SKILL.md; docs/schemas/event.schema.json   (Task 6)
```

## Interfaces

Consumes: `catervas_verifies_alone`, `start_point` (plans 1 and 3), `new_project`, `ConnectorTag`, `evaluate_connector_call`, `file_the_approved`, `marketing_plans`; harnesses of each file's `mod tests`.
Produces: `ConnectorTag::Paid` (wire `paid`); `approved_by: "catervas"` on `social_post.scheduled`; `by: "catervas"` on `data_pipeline.approved`; `catervafication::start(deps, team, project, by)`; `ProductPlanState` in `apps/web/src/app/productPlan.ts`. Removed: `requires_human_acceptance`, `ReadinessContext::human_approves`, `ReadinessRule::SummaryPresent`, `may_change_an_owner_document`, `plan_acceptance_note`, `FileDue::MarketingWaits`, `catervas_decide_data_pipeline`, `PIPELINE_DECISION_INSTRUCTION`, the skill `deciding-data-pipelines`.

## Tasks

### Task 1: no contract waits for approval

Files: those of the File map for Task 1. These tests change to the new flow here, or are deleted where they pin only the approval:
- core `transition.rs`: `keeps_a_contract_the_human_has_not_accepted_out_of_ready` (1168), `asks_the_human_only_once_the_contract_passes_the_structural_checks` (1224), `asks_the_human_about_a_contract_that_now_passes_however_often_it_failed_before` (1300), `escalates_a_contract_that_waits_for_the_human_and_names_why` (1356) and `names_a_reason_for_every_row_of_the_table_that_escalates` (2113);
- `transitions.rs`: `reads_the_policy_that_asks_the_human_for_every_contract` (2677), `binds_the_summary_to_the_plans_a_human_approves` (3270, deleted with the rule) and `asks_the_owner_about_the_product_plans_contract_under_all` (4389);
- `requests.rs`: `escalates_an_epic_for_approval_once_it_passes` (2006), `assigns_an_approved_epic_to_its_product_manager` (2679) and `assigns_an_approved_epic_to_the_scrum_master` (3255);
- `catervafication.rs`'s test that sets `human_approves` (717);
- any test of `readiness.rs` and `plain.rs` that names `SummaryPresent`.

The risk line of the task page changes with the approval: `riskHighWhy` (`apps/web/src/strings/en.ts`), which plan 1 left as "High. You approve its plan.", becomes "High. The team may accept it without you, as for any task." No web test pins it.

`crates/store/src/projections.rs`' `awaits_approval_from_the_escalation_until_the_next_move` (2045) and `counts_every_escalation_but_the_two_the_process_asks_for` (2383), and `daemon/gates.rs`' `waiting_lists_a_plan_to_approve` (1950), read events an older log holds and stay as they are.

Tests:
- `readies_an_epic_and_a_high_risk_contract_without_the_human` (`transition.rs`): an epic, a `high` risk task, and a task under the policy `human_accepts_contracts: all`, each passing the structural checks and none with a `summary`, move `refining -> ready` for the governor. `refining -> escalated` with `ContractRequiresHuman` is refused for each.

- [x] `feat(core): let no contract wait for the human's approval`

### Task 2: a new project's product plan needs no approval

Files: those of the File map for Task 2. These tests change here: `human.rs`' `drafts_the_product_plan_on_the_owners_word` (2992), `a_lone_product_manager_reviews_its_own_product_plan` (3063) and `starts_a_catervafication_approved_by_the_owner` (3191, which no longer finds a `human.accepted`); `rules.rs`' `catervafies_a_new_project_once_its_plan_is_approved` (8309) and `leaves_an_existing_repository_to_its_owner` (8407); `gates.rs`' `answers_where_the_product_plan_stands` (4501) and `product_plan_state_reads_html_documents` (4596); `crates/store/src/product_plan.rs`' `a_task_drafts_the_plan_until_it_is_cancelled_or_accepted_and_integrated` (99); `requests.rs`' `judges_the_product_plan_task_as_filed_and_sends_a_refused_one_to_the_product_manager` (1453); `catervafication.rs`' tests of `file_due` and `MarketingWaits`; and `Today.test.tsx`'s product plan card cases. `ProductPlanCard.test.tsx` is deleted with the card. (`rules.rs`' `leaves_a_task_that_may_change_the_product_plan_for_the_human`, 4770, is plan 1's.) Ignored tests need git.

Tests:
- `a_new_project_plans_once_its_product_plan_task_is_accepted` (`rules.rs`):
  - In a project whose log holds `project.started`, a ready task is passed over while the product plan task is `verifying`.
  - Once that task is `accepted`, the next tick starts the Catervafication. Its Marketing task is filed with `dependencies` naming the plan task, and the ready task is planned.
  - `main` holds no plan throughout.

- [x] `feat(runtime): start a new project once its product plan task is accepted`

### Task 3: spec and roadmap change like any document

Files: those of the File map for Task 3; these tests change here: `human.rs`' `approves_a_card_as_one_folder_change` (7312), `sends_back_with_the_owners_words` (7391) and `approves_files_that_already_say_it` (7502), which now record their `folder_doc.proposed` directly as an older log holds it; `tools/folders.rs`' `proposes_the_product_plan_at_the_review` (846), which becomes the test below; and `crates/store/src/folder_docs.rs:470`.

Tests:
- `writes_the_product_spec_without_a_proposal` (`tools/folders.rs`): in a sprint review session, `catervas_write_folder_doc` on `docs/catervas/product/spec.md` (with `agent_text`) records `folder_change.opened` for both files and no `folder_doc.proposed`. An older log's open `folder_doc.proposed` is still decided by `folder_doc_decide`.

- [x] `feat(runtime): write the product's spec and roadmap without the owner's approval`

### Task 4: free marketing goes out without asking

Files: those of the File map for Task 4. `posts.rs`' `requests_a_post_outside_the_plan` (812) becomes part of the test below, and the `SocialPostRequested` counts in `refuses_the_wrong_buffer_channel` (956) and `refuses_a_post_the_plan_does_not_hold` change to `social_post.scheduled`.

Tests:
- `approves_a_plan_that_spends_nothing_and_posts_outside_it` (`tools/marketing.rs`):
  - A proposed plan with every budget zero is followed by `marketing_plan.approved` with the note of Decisions and no `agent_id`.
  - A plan with a 50 USD ads budget is only proposed.
  - With the free plan active, `schedule_post` with no `slot`, four hours ahead, records `social_post.scheduled { approved_by: "catervas" }` and no `social_post.requested`; one an hour ahead is refused `post_too_soon`.

- [ ] `feat(runtime): approve free marketing at once, ask only where it spends`

### Task 5: only a paid call asks

Files: those of the File map for Task 5. Tests that pinned `external_effect` asking change here, to `paid`: `hooks.rs` 2581 to 3390, `human.rs`' `approve_records_the_grant_on_the_task` (3468), `approve_refuses_an_unknown_or_decided_approval` (3521) and `two_decisions_racing_on_one_approval_let_one_through` (3562), `gates.rs`' `an_open_approval_waits_on_the_human` (3374), `ads_calls.rs`' tests and fixtures that tag the ads tools, `kit.rs`'s allowance and plan-mark checks, `team.rs`' allowance tests, and the web's `connectors.test.tsx` and `allowances.test.tsx`.

Tests:
- `only_a_paid_call_asks` (`permissions.rs`):
  - An `external_effect` call with no grant runs.
  - A `paid` call with no grant and no allowance asks.
  - A `paid` call within its allowance runs, and one beyond it asks.
- `the_shipped_kits_tag_what_costs_money_paid` (`crates/roles/src/kit.rs`, a payment guarantee; core cannot read the kits): the shipped kits validate, every `higgsfield`, `recraft`, `google-ads` and `serpapi` tool tagged other than `denied` or `network` is `paid`, and no tool tagged `external_effect` has an allowance.

- [ ] `feat(core): ask only before a connector call that costs money`

### Task 6: a data source asks only when it costs or sends data

Files: those of the File map for Task 6. `tools/pipeline.rs`' `approve_is_refused_for_what_needs_the_owner` (674), `a_free_request_can_be_approved` (736), `escalate_and_decline_record_their_events` (884), `only_its_decision_session_decides` (957) and `two_decisions_at_once_record_one` (1041) are deleted or become the test below; the tests of `orchestrator/pipeline.rs` are deleted with the module, and those of `session.rs` and `kit.rs` that name the decision tool or the skill change here.

Tests:
- `files_a_free_source_at_once_and_asks_the_owner_for_a_paid_one` (`tools/pipeline.rs`):
  - A free request is filed (the request's `task.created`, `created_by: "catervas"`) with no Product Manager session and no `data_pipeline.escalated`.
  - The free requests record `data_pipeline.approved { by: "catervas", reason: "" }` with no `agent_id`.
  - A `paid` request, an `unknown` one, and a free one with `sends_project_data`, each record `data_pipeline.escalated` with its reason of Decisions and no `agent_id`, and file nothing.

- [ ] `feat(runtime): file a free data source at once, ask the owner about one that costs or sends data`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
