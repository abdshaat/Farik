# Standalone 8 of 8: the record

Status: done
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md`, revision 0.115
Depends on: plans 1 to 7 committed on this branch; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

## Goal

`docs/SPEC.md` says what plans 1 to 7 built (hard rule 8), at revision 0.115. The design documents that state the old flows are amended, and the plans are ticked. This plan touches documents only, so it has no test; its check is the full check and a reading against each plan's Decisions.

## Decisions

- **One revision for the whole change**, 0.115 (`main` is at 0.114), dated 2026-10-16 and naming ADR 0061 and each of the seven plans. Each changed paragraph carries "(changed in 0.115; ADR 0061)".
- **Sections changed in `docs/SPEC.md`**, each from the plan named:
  - 5.1: "Nobody grades their own homework" concerns code, and only code is reviewed by an agent; every other task is checked by Catervas on its criteria (plan 1).
  - 5.2:
    - the table gains the governor's `verifying -> accepted` and `verifying -> rejected` rows, open only to a task that changes no code and whose every criterion Catervas runs (plan 1);
    - `refining -> escalated` for approval never opens for a new contract (plan 5);
    - the human's `verifying -> rejected` opens only while a result waits on the human: a code task's `human` criterion, or an epic filed before ADR 0061 that Catervas does not verify alone, which keeps the old path (plan 1);
    - the Designer paragraph says the plan is approved by Catervas as it is proposed (plan 4);
    - the dependency paragraph says a dependency counts once accepted (plan 3).
  - 5.3: the judgment of a contract by an agent, its rules `judgment_recorded` and `judgment_answers`, and the judge go (plan 7); the rule `no_review_outside_code` (plan 1); the rule `summary_present` goes, and a contract's `summary` is optional (plan 5); `product_plan_first` waits for the product plan task's acceptance (plan 5); `human_accepts_contracts` is ignored (plan 5).
  - 5.4:
    - item 5 (the human's acceptance) goes;
    - the paragraph on what a human gate shows ("What a human gate shows", added in 0.18) keeps only the gates that remain: a code task's `human` criterion, and the money gates of 5.7; its sentences on a contract's required summary go (plan 5);
    - Catervas's own review note for a task it verifies alone (plan 1).
  - 5.5 and 5.7: the owner is asked only before money is spent. The list of what waits for the owner is cut to a marketing plan with money in it, a raised budget, a purchase order, a data source that is paid, of unknown cost or that sends the project's data out, and a `paid` connector call (plan 5); the site requests and `catervas site approve|decline` go from 5.7 (plan 6). Seller messages are unchanged.
  - 5.6:
    - the tier table gives `network` to every role (plan 6);
    - connector tags gain `paid`, and `external_effect` runs without asking; a custom connector's unlabelled tool is `paid`; an allowance and a plan mark go on a `paid` tool; an older kit entry pinned `external_effect` runs nothing until connected again (plan 5);
    - "A role held to read paths" and "A role held to approved sites" go (plan 6);
    - "Read roots" covers every role (plan 6);
    - the hook order loses the sites and the read paths (plan 6).
  - 5.12: `integration` is read as `pull_request` whatever is written; `human_accepts_contracts` is ignored (plans 2 and 5); `policy.judgment` is accepted, ignored and dropped on write (plan 7).
  - 5.14: integration only by pull request; no `origin` means a merge by hand recorded when the branch is in the integration branch; `catervas integrate` never merges; a plan task opens no pull request; a task starts from its unmerged dependencies' branches (plans 2 and 3).
  - 5.16:
    - item 2: no approval of the epic (plan 5);
    - item 3: the plan task needs no review, its tasks are filed once Catervas accepts it, and they start from its branch (plans 1 and 3);
    - item 4: the epic is accepted by the governor once its tasks are integrated (plan 1);
    - the product plan task and the new project's Catervafication start on its acceptance; the Catervafication's Marketing task depends on the Product Manager's (plan 5).
  - 5.17: changes to `spec` and `roadmap` land like any document (plan 5).
  - 4.1 and 4.3: setup's last step loses the integration choice (plan 2), and its advanced step the plan check (plan 7); the interview ends in a plan the Product Manager finishes, and Today has no product plan card (plan 5).
  - 6.1, 6.3, 6.5, 6.6, 6.8 and 6.10: each role's paragraph says who reviews what and what it may read, per plans 1, 4, 5 and 6.
    - 6.5: a plan that spends nothing is approved at once, and posts outside a plan go out with Stop.
    - 6.10: no sites, no `catervas_request_sites` or `catervas_read_sites`, no `catervas site` command (plan 6); every role reads both private folders' workbooks through `catervas_read_sheet` with `folder` (plan 6); a data source is filed at once, `by: catervas`, unless it costs money, its cost is unknown, or it sends the project's data out, which ask the owner; `catervas_decide_data_pipeline` and the skill `deciding-data-pipelines` go (plan 5).
  - 8.2, 8.3 and 8.6: every session's web tools, every sandbox's network, and what still protects secrets (plan 6).
  - F9 and F16: one sentence each.
- **Design documents:**
  - `docs/design/catervas-folders.md`, each changed sentence ending "(ADR 0061)":
    - "Planning a feature: PM → Architect → PM → Scrum Master" (line 68) gains, after its step 2, that no agent reviews the plan and its tasks are filed once Catervas accepts it and start from its branch.
    - Line 14's "The Marketing Specialist reads the Product Manager's folder and its own, and no other." and line 40's **Reading** bullet become "Every agent reads every folder; only secrets stay out of reach."
    - Line 9's "documents nothing until the owner has approved the product plan", line 53's owner acceptance of a task that changes a human document, line 56's `folder_doc.proposed` flow, and line 87's Marketing wait say instead: the Product Manager's product plan task is accepted by Catervas, a change to `spec` or `roadmap` lands at the sprint review as any document does, and the Marketing Specialist's task depends on the Product Manager's.
  - `docs/design/marketing-specialist.md`: the owner approves a plan only when it spends money.
  - `docs/design/designer-chats-templates.md`: one line saying that ADR 0061 replaced the Product Manager's approval of the plan.
- **The plans:** each of the eight plan files and the README gets `Status: done`, each plan's boxes ticked, and the README's table a fourth column, `Commits`, holding each plan's commit hashes, short form, in order.

## Tasks

### Task 1: the spec and the design documents

Files: `docs/SPEC.md`, `docs/design/catervas-folders.md`, `docs/design/marketing-specialist.md`, `docs/design/designer-chats-templates.md`, and the nine files of this folder.

- [x] `docs(spec): only code is reviewed, only payments ask, revision 0.115`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```
