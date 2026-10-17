# Standalone 5 of 8: only payments ask

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 4.1, 4.3, 5.2, 5.3, 5.6, 5.7, 5.12, 5.16 item 2, 5.17, 6.5, 6.7, 6.10; F9, F12
Depends on: plans 1 to 3 committed on this branch; ADR 0061
Readiness confirmed by: pending

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin a removed gate change in the task that removes it, never skipped. The README's table lists every human gate kept or removed. This plan removes the ones marked plan 5.

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
  - Rule 9 (`requests.rs:130`) no longer asks for the escalation.
  - An `approval` or `risk_gate` escalation already open in a log is still resolved by `human_accept { subject: contract }` (`human.rs` `approve`), which is unchanged.
  - The Definition of Ready's detail "this contract needs the human's acceptance…" goes.
  - The Product Manager's `system.md` and `writing-task-contracts` §6 stop saying an epic or a `high` risk task waits for the user's approval. The PM still asks its questions first (5.16 item 1).
- **The product plan is the Product Manager's to finish.**
  - Its task is accepted by the governor (plan 1).
  - `Transitions::awaits_the_product_plan` becomes: `project.started` is in the log, and no task created with `product_plan: true` is `accepted`. It no longer reads git, and `product_plan_on` goes once it has no caller.
  - `catervafy_a_new_project` (`rules.rs:309`) starts on that acceptance, and no longer records `human.accepted { subject: contract }` for the epic, since no epic waits for approval.
  - `product_plan.state` (`daemon/gates.rs:615`) answers `approved` = that task is accepted.
  - `ProductPlanCard.tsx` and its place on Today go.
  - The Product Manager's chat still interviews the owner (4.3); `Draft the product plan` stays.
- **The Catervafication's Marketing task** is filed with `dependencies: [<the Product Manager's task>]`:
  - for an existing repository, the PM's Catervafication task;
  - for a new project, the product plan's task.

  It is filed when the others are. `FileDue::MarketingWaits` goes, because plan 3 lets a dependency count once accepted and starts the task from its branch.
- **Spec and roadmap change at once.**
  - `write_folder_doc` (`crates/runtime/src/tools/folders.rs:398`) no longer proposes: `checked.proposal` is never set. A change to `docs/catervas/product/spec` or `roadmap` lands as a folder change like any other document, and so reaches a pull request (plan 2).
  - `propose` (`folders.rs:633`), and the sprint review's words asking the PM to propose (`messages.rs` `product_refresh_asks`), say "write" in place of "propose".
  - `folder_doc_decide` (`human.rs:2037`) and `DocumentChanges.tsx` stay, for proposals already open in a log.
  - `OWNER_ACCEPTED_STEMS` and `may_change_an_owner_document` go if nothing still reads them; `folders.rs` 289, 337 and 536 are checked by the compiler.
- **A marketing plan that spends nothing is approved at once.** In `catervas_propose_marketing_plan` (`tools/marketing.rs:230`), when the plan's total budget and every channel's and campaign's budget are zero, Catervas records `marketing_plan.approved { plan, note: "Catervas approved it: it spends nothing (ADR 0061)." }` right after `marketing_plan.proposed`. The envelope carries no agent and no session, as the owner's approval does.

  A plan with any money in it waits for the owner as today, and so do its ads and their hard stop. The schema's description of `marketingPlanApprovedBody` says Catervas approves a plan that spends nothing.
- **A post outside the plan goes out like one inside it.** `schedule_post` (`tools/posts.rs:454`) records `social_post.scheduled { approved_by: "catervas" }` for a post that fills no slot of the active plan, or with no active plan. It is scheduled at least three hours ahead and shown on Today with Stop, as a planned post is.
  - `approved_by` gains `"catervas"` in `docs/schemas/event.schema.json`.
  - `social_post.requested` is never written again, and `social_post_decide` stays for open requests.
- **A connector tool is `paid` when the service charges for the call.**
  - `ConnectorTag` (`crates/core/src/governor/permissions.rs:154`) gains `Paid`, with the wire value `paid`. The connector-tag enum in `docs/schemas/team.schema.json:217` gains `"paid"`, with "paid, it costs money at the service and asks the human".
  - `evaluate_connector_call`: `Paid` asks as `ExternalEffect` asks today, grants and allowances included. `ExternalEffect` runs without asking, recorded as every call is (`tool.called`), with no approval.
  - The hook's `ask` and `external_effect_refused` (`hooks.rs`) apply to `paid`.
  - ADR 0037's allowances may be given only to a `paid` tool (`kit.rs:656`'s `allowance_not_external` becomes `allowance_not_paid`, "only a tool tagged paid has an allowance, and {tool} is not"). `plan_approved` marks need `paid` (`plan_mark_not_external` becomes `plan_mark_not_paid`).
  - In the kits (`crates/roles/roles/*/kit.yaml`), every tool tagged `external_effect` becomes `paid`, by server:
    - Marketing: `higgsfield`, `recraft` and `google-ads`;
    - Procurement: `serpapi`.
  - `kit`'s and `github`'s stay `external_effect` and run without asking; `kit`'s allowances are removed.
  - The web's tag labels (`AgentEdit.tsx`, `en.ts`) gain `paid` "Costs money: asks you first".
- **The Product Manager no longer decides a data source.**
  - `pipeline_needs_owner` (`crates/core/src/pipeline.rs:22`) is unchanged: a request that costs money, whose cost is unknown, or that sends the project's data to a third party (`sends_project_data`) asks the owner (the founder, 2026-10-16: "Still ask me").
  - `catervas_request_data_pipeline` (`tools/pipeline.rs:215`) files a free request at once, as `file_the_approved` files an approved one, recording `data_pipeline.approved { by: "catervas", reason: "", request }`. `dataPipelineDecidedBy` gains `"catervas"` in `docs/schemas/event.schema.json`.
  - A request `pipeline_needs_owner` holds is recorded `data_pipeline.escalated { reason }` at once, with no agent and no session on the envelope; `reason` is "It costs money, or its cost is not known, so you decide (ADR 0061)." or, for one that is free and sends data out, "It sends your project's data to another service, so you decide (ADR 0061)."
  - `catervas_decide_data_pipeline`, its decision session (`orchestrator/pipeline.rs`) and `PIPELINE_DECISION_INSTRUCTION` go. `data_pipeline_decide` (`human.rs:1326`) stays the owner's.

## File map

```
crates/core/src/governor/transition.rs, done.rs, readiness.rs; crates/runtime/src/transitions.rs; orchestrator/requests.rs   contract approval (Task 1)
crates/roles/roles/product_manager/system.md, skills/writing-task-contracts/SKILL.md                                        (Task 1)
crates/runtime/src/transitions.rs, orchestrator/rules.rs, catervafication.rs, daemon/gates.rs; crates/store/src/git.rs; apps/web/src/pages/ProductPlanCard.tsx, Today.tsx   (Task 2)
crates/runtime/src/tools/folders.rs, orchestrator/messages.rs, crates/core/src/folders.rs                                    (Task 3)
crates/runtime/src/tools/marketing.rs, tools/posts.rs, docs/schemas/event.schema.json                                       (Task 4)
crates/core/src/governor/permissions.rs, crates/runtime/src/daemon/hooks.rs, crates/roles/src/kit.rs, crates/roles/roles/*/kit.yaml, docs/schemas/team.schema.json, apps/web/src/pages/AgentEdit.tsx, apps/web/src/strings/en.ts   (Task 5)
crates/core/src/pipeline.rs, crates/runtime/src/tools/pipeline.rs, orchestrator/pipeline.rs, prompt.rs, tools.rs, session.rs, docs/schemas/event.schema.json (Task 6)
```

## Interfaces

Consumes: `catervas_verifies_alone`, `start_point` (plans 1 and 3), `new_project`, `ConnectorTag`, `evaluate_connector_call`, `file_the_approved`, `marketing_plans`; harnesses of each file's `mod tests`.
Produces: `ConnectorTag::Paid` (wire `paid`); `approved_by: "catervas"` on `social_post.scheduled`. Removed: `requires_human_acceptance`, `FileDue::MarketingWaits`, `catervas_decide_data_pipeline`, `PIPELINE_DECISION_INSTRUCTION`.

## Tasks

### Task 1: no contract waits for approval

Files: those of the File map for Task 1. These tests change to the new flow here: in core `transition.rs`, 1168, 1224, 1300, 1356 and 2113; `transitions.rs` 2677, 3270 and 4389; `requests.rs` 2006, 2679 and 3255; `projections.rs` 2045 and 2383; `gates.rs` 1950.

Tests:
- `readies_an_epic_and_a_high_risk_contract_without_the_human` (`transition.rs`): an epic, a `high` risk task, and a task under the policy `human_accepts_contracts: all`, each passing the structural checks, move `refining -> ready` for the governor. `refining -> escalated` with `ContractRequiresHuman` is refused for each.

- [ ] `feat(core): let no contract wait for the human's approval`

### Task 2: a new project's product plan needs no approval

Files: those of the File map for Task 2. Tests at `human.rs` 2992 and 3063, `rules.rs` 4770, 8309 and 8407, `gates.rs` 4501 and 4596, `store/src/product_plan.rs:99`, `requests.rs:1453`, and `ProductPlanCard.test.tsx` (deleted with the card) change here. Ignored tests need git.

Tests:
- `a_new_project_plans_once_its_product_plan_task_is_accepted` (`rules.rs`):
  - In a project whose log holds `project.started`, a ready task is passed over while the product plan task is `verifying`.
  - Once that task is `accepted`, the next tick starts the Catervafication. Its Marketing task is filed with `dependencies` naming the plan task, and the ready task is planned.
  - `main` holds no plan throughout.

- [ ] `feat(runtime): start a new project once its product plan task is accepted`

### Task 3: spec and roadmap change like any document

Files: those of the File map for Task 3; the tests at `human.rs` 7312, 7391 and 7502, `folders.rs:821` and `folder_docs.rs:470` change here.

Tests:
- `writes_the_product_spec_without_a_proposal` (`tools/folders.rs`): in a sprint review session, `catervas_write_folder_doc` on `docs/catervas/product/spec.md` (with `agent_text`) records `folder_change.opened` for both files and no `folder_doc.proposed`. An older log's open `folder_doc.proposed` is still decided by `folder_doc_decide`.

- [ ] `feat(runtime): write the product's spec and roadmap without the owner's approval`

### Task 4: free marketing goes out without asking

Files: those of the File map for Task 4.

Tests:
- `approves_a_plan_that_spends_nothing_and_posts_outside_it` (`tools/marketing.rs`):
  - A proposed plan with every budget zero is followed by `marketing_plan.approved` with the note of Decisions and no `agent_id`.
  - A plan with a 50 USD ads budget is only proposed.
  - With the free plan active, `schedule_post` for a day with no slot records `social_post.scheduled { approved_by: "catervas" }` at least three hours ahead, and no `social_post.requested`.

- [ ] `feat(runtime): approve free marketing at once, ask only where it spends`

### Task 5: only a paid call asks

Files: those of the File map for Task 5. Tests that pinned `external_effect` asking change here: `hooks.rs` 2581 to 3390, `human.rs` 3468, 3521 and 3562, `gates.rs:3374`, and `kit.rs`'s allowance checks.

Tests:
- `only_a_paid_call_asks` (`permissions.rs`):
  - An `external_effect` call with no grant runs.
  - A `paid` call with no grant and no allowance asks.
  - A `paid` call within its allowance runs, and one beyond it asks.
  - The shipped kits validate: every `higgsfield`, `recraft`, `google-ads` and `serpapi` tool tagged other than `denied` or `network` is `paid`, and no `external_effect` tool has an allowance.

- [ ] `feat(core): ask only before a connector call that costs money`

### Task 6: a data source asks only when it costs or sends data

Files: those of the File map for Task 6. The tests at `tools/pipeline.rs` 674, 736 and 884, and `orchestrator/pipeline.rs` 386 and 458, change here; the PM's decision tests are deleted with the session.

Tests:
- `files_a_free_source_at_once_and_asks_the_owner_for_a_paid_one` (`tools/pipeline.rs`):
  - A free request is filed (the request's `task.created`) with no Product Manager session and no `data_pipeline.escalated`.
  - The free requests record `data_pipeline.approved { by: "catervas" }`.
  - A `paid` request, an `unknown` one, and a free one with `sends_project_data`, each record `data_pipeline.escalated` with its reason of Decisions and no `agent_id`, and file nothing.

- [ ] `feat(runtime): file a free data source at once, ask the owner about one that costs or sends data`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
