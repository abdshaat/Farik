# Standalone: only code is reviewed, only payments ask, research is free, Catervas never merges

Status: done
Branch: `fix/plans-need-no-review` (work outside a phase, one pull request to `main`)
Decision: ADR 0061 (`docs/decisions/0061-only-code-is-reviewed-only-payments-ask.md`)
Depends on: `origin/main` at `74b4a5af` (#45 merged)
Readiness confirmed by: pending (one round per plan file, against `docs/standards/workflow.md` stage 2)

Eight plans, each copied from `docs/plans/step-template.md` and executed in this order. A plan consumes only what the plans before it committed. The parallel CTV-9 work (no sprints, ADR 0062) is rebased on this branch, so no plan here touches sprints, the sprint budget or ceremonies' scheduling.

| # | File | What it does | Commits |
|---|---|---|---|
| 1 | `01-review-only-code.md` | The governor accepts and rejects every task that is not code on Catervas's own runs; readiness refuses `review` and `human` criteria outside code; no result waits on the human; templates carry machine criteria | `8470eec9`, `519aa1fe`, `3557a17f`, `b75062a7`, `78acc55d` |
| 2 | `02-pull-requests-only.md` | Catervas never merges: every policy reads as `pull_request`, a project with no `origin` waits for a hand merge, setup and settings lose the choice, Today lists each merge waited on | `e6d271bc`, `261c68b5`, `5161e2f7` |
| 3 | `03-nothing-waits-on-a-merge.md` | A dependency counts once accepted and a task starts from its dependencies' branches; an epic's plan tasks follow at once, start from the plan's branch, and the plan task opens no pull request | `19763edb`, `bbdd477f`, `9fc6edd0`, `11ce4b84`, `6c856162` |
| 4 | `04-designer-plan.md` | Catervas approves a Designer's plan at once; the decision session, its tool and prompt go; the task page and activity follow | `b2144357`, `fb289495` |
| 5 | `05-only-payments-ask.md` | The owner is asked only before money is spent: contract approval, the product plan's approval, document proposals, free marketing plans, posts outside a plan, free connector calls and the PM's pipeline decision stop asking | `baeee659`, `204ebda4`, `794d3177`, `bfee25b9`, `e8d3161e`, `08623820` |
| 6 | `06-research-is-free.md` | Every role holds `network`, every session may search and fetch, every sandbox has the network, the Marketing read limit and the approved sites go, every role reads the private workbooks; secrets stay | `4a098ee6`, `1418a4a5`, `e4b8f9ae`, `f1c7773a` |
| 7 | `07-no-contract-judging.md` | No agent judges a contract: the judgment session, its tool and readiness rules go; `policy.judgment` is ignored on load and dropped on write; setup and settings lose "Who checks" | `63a2de50`, `58c6c86a`, `793ac90f` |
| 8 | `08-record.md` | SPEC 0.115, the design documents, and the plans' ticks (hard rule 8) | this commit (`docs(spec): only code is reviewed, only payments ask, revision 0.115`) |

## The rule for "changes code" (plan 1)

`catervas_core::team::reviewed_by_an_agent(contract) = contract.kind == Task && changes_code(contract.assignee_role)`. Chosen over a rule on paths (`document_paths_only` or the diff's paths) for three reasons:
- It is known when the contract is written, before any diff exists, so the reviewer can still be named at assignment.
- `document_paths_only` already holds every other role's task to the team's document paths, so a non-code role cannot change code.
- It is the rule `task_branch` (`feature/`, `fix/` against `docs/`) already uses.

A Developer's task that only touches documents is still reviewed. That is rare, and it errs toward review.

## Human gates: kept or removed (R4)

| Gate (where) | Money? | Decision | Plan |
|---|---|---|---|
| Epic and `high` risk contract approval, `refining -> escalated` `ContractRequiresHuman`, policy `human_accepts_contracts` (`transition.rs` `human_must_accept_the_contract`, `transitions.rs` `asks_every_contract`) | no | removed; the row stays, never opens; open old escalations still resolve by `human_accept` | 5 |
| Result acceptance `requires_human_acceptance` / `result_needs_the_human` / `result_awaits_human` / `DoneRule::HumanAccepted`, epic reviewed by the human (ADR 0013), `human` criteria | no | removed; `human` criteria refused outside code by readiness and allowed on code tasks as today | 1 |
| Human send-back of a result (`HumanRejection`, ADR 0024) | no | the row stays, never opens for a new result | 1 |
| New project's product plan approval (`product_plan_first`, `product_plan_on`, `ProductPlanCard`) | no | removed: the plan task is accepted by the governor, rule 9 waits for that acceptance, not for the merge; the card goes | 5 |
| Catervafication start (`catervafication_start`) | no | kept: it is the owner choosing to start, not a check; its epic is accepted by the governor | 1 |
| `folder_doc.proposed` (owner decides spec/roadmap changes) | no | removed: the change lands as a folder change at once; old proposals stay decidable | 5 |
| `folder_change.opened` integration | no | pull request only | 2 |
| `catervas_ask_human` questions | no | kept: an agent's question, not a check | none |
| Escalations: budget, sessions, iterations, blocker age, readiness, integration, preview | budget yes | kept: failures and AI spend, not reviews | none |
| `external_effect` connector call approval (ADR 0031) | some | only a tool tagged `paid` asks; `external_effect` runs and is recorded | 5 |
| Allowances (ADR 0037) | yes | kept, on `paid` tools only | 5 |
| Connector connect, `skill_confirm` (ADR 0030, 0034) | no | kept: the owner adds what runs with its keys; a guard against planted code | none |
| Marketing plan approval (ADR 0042) | when it has money | kept when any budget in it is above zero; a plan with none is approved by Catervas at once | 5 |
| Post outside the plan (`social_post.requested`) | no (Buffer is free) | removed: scheduled at once, `approved_by: catervas`, still shown with Stop | 5 |
| Google Ads writes, the budget's hard stop, raised budget request | yes | kept | none |
| `site.requested` approved sites | no | removed | 6 |
| Purchase orders (decide, send, place, receive, close) | yes | kept | none |
| Data pipeline: the PM's decision | no | removed: filed at once unless it needs the owner | 5 |
| Data pipeline: paid or unknown cost to the owner | yes | kept | none |
| Data pipeline: free but `sends_project_data` to the owner | no | kept (the founder, 2026-10-16: "Still ask me") | none |
| Seller messages sent by the owner | no | kept: Catervas has no mailbox (a missing capability) | none |
| Merging into the integration branch | no | the human's alone, always (R2) | 2 |
| Agent judging of contracts (`contract.judged`, `judgment.required: always`) | no | removed (the founder, 2026-10-16: "Remove it"); not a human gate, but a plan review | 7 |

## Read limits: removed or kept (R3)

| Limit (where) | Decision | Plan |
|---|---|---|---|
| Marketing reads only `product/` and `marketing/` (`folders::read_access`, `ReadAccess`, `hooks::judge_reads`, `read_not_allowed`, `members_read`'s exception) | removed | 6 |
| Procurement held to approved sites (`sites::web_access`, `check_site_urls`, `hooks::judge_fetch`, `judge_sites_of_call`, `site_not_approved`, `sites_unreadable`, `catervas_request_sites`, `catervas_read_sites`, `purchase_order_site_not_approved`) | removed | 6 |
| Procurement's sandbox with no network (`sandbox_for`) | removed: the network follows the tier for every role | 6 |
| `network` tier missing from the Developer, the Designer and the Scrum Master (`default_tiers`); explore and chat registered with `Read` only; verify sessions' built-ins | removed: every role and every session holds `network` | 6 |
| Private workbooks readable by their owner, Finance's register, and the reviewer (`sheets::folder_to_read`) | removed: any role reads either folder's workbooks through `catervas_read_sheet`; writes stay the owner's | 6 |
| A private-folder task's session confined to its folder (`workspace_paths`) | kept: that is where it writes; reading goes through the tools | none |
| Read roots in a project folder (ADR 0057) | kept as they are, Marketing and private-folder tasks now included | 6 |
| Protected paths (`DEFAULT_PROTECTED_PATHS`, `.catervas/local/**`), keychain, model credential, connector keys, OAuth tokens, `~` expansion, `Bash` | kept: secrets | none |
| Playwright confined to the preview (`url_outside_preview`) | kept: it drives the app under test, not research; the web is open through `WebFetch` | none |
| Built-in connectors at fixed endpoints (osv, fx, recalls, ebay) | kept: a server's own design, no agent limit | none |

## The founder's answers (2026-10-16)

1. Contract judging: "Remove it". Plan 7 removes it entirely.
2. A free data source that sends the project's data to a third party: "Still ask me". Plan 5 keeps `pipeline_needs_owner` as it is.
