# 0061. Only code is reviewed, only payments ask, research is free, and Catervas never merges

Date: 2026-10-16
Status: accepted (the founder, 2026-10-16, quoted below)

The founder, in order:
1. "Change it so that any agent can plan and start working. No other agent need to verify the plan, agents only need to verify final results."
2. "The only thing that needs review is the code produced by the developer. Anything else needs no review and can be planned and executed by an agent. Agents must be free to do any research, no check on what the agent can research. The only check must be for actions that require payment."
3. Asked about the human's approvals: "Catervas will not be allowed to merge code now. Just opening pull requests."
4. Asked which code is reviewed: any code change, the Software Developer's and the UI/UX Designer's. Asked about research: only secrets stay hidden.

## What this supersedes and amends

Supersedes, in part:
- **ADR 0012**: the `auto_merge` and `manual` policies, and `auto_merge` as a new team's default. Integration is always by pull request; older team files load, and their value is read as `pull_request`.
- **ADR 0013**: the human as an epic's reviewer. The governor accepts an epic on the criteria Catervas runs, once its tasks are integrated.
- **ADR 0019 and ADR 0039** (private folders): the read limits on the Finance and Procurement Specialists' workbooks. Every agent may read them through `catervas_read_sheet`; writes stay with the owning role. The founder's ruling of 2026-10-07 ("Only the Product Manager") lapses, since no agent reviews such a task.
- **ADR 0024**: the human's send-back of a result. No result waits on the human any more, so its gate no longer opens; the row stays so older logs read.
- **ADR 0026**: the Product Manager's approval of the UI/UX Designer's plan (`catervas_decide_design_plan`).
- **ADR 0031**: asking the human before every `external_effect` connector call. Only a call tagged `paid` asks now, and ADR 0037's allowances apply to `paid` calls alone.
- **ADR 0039**, further:
  - the Procurement Specialist's approved sites (`site.requested`, `catervas_request_sites`, `site_not_approved`);
  - its sandbox with no network;
  - the Product Manager's decision on a data pipeline request. A request that costs money, or whose cost is unknown, goes to the owner; any other request is filed at once.
- **ADR 0041** (ask or auto, not built): replaced for every team by "only a payment asks".
- **ADR 0042**:
  - the owner's approval of a marketing plan that spends nothing;
  - the rule that a post must fill a slot of the approved plan.

  A plan with money in it still waits for the owner, and the budget's hard stop is unchanged.
- **ADR 0051**:
  - the owner's approval and acceptance of a new project's product plan, and of changes to `spec` and `roadmap`;
  - the Marketing Specialist reading only `docs/catervas/product/` and its own folder;
  - in the planning flow, the Product Manager's review of the Architect's plan, and the wait for the plan to reach the integration branch.
- The human approval of contracts:
  - the `refining -> escalated` approval of every epic and of every `high` risk contract (5.16 item 2);
  - the team policy `human_accepts_contracts`, which still loads and is ignored.

Kept as they are:
- **Agent judging of contracts** before they are ready (`contract.judged`, 5.3). It is put to the founder, below.
- **ADR 0030's and ADR 0034's confirmation** of a connector or a skill that arrived by a clone or a hand edit. It is a guard against code someone else planted, not a review.
- **The governor's own limits**: allowed and protected paths, the worktree, the sandbox, the denial of secrets, and every AI spending limit and hard stop (ADR 0015).
- **Past events**, which are never rewritten. Old logs and old team files load.

## Context

The founder saw this live on 2026-10-16: "When I ask for a change, the architect moves the task to completed and opens a PR with the plan and no implementation happens." That is one case of a general pattern. Today Catervas puts an agent's review, an owner's approval, or a merge in front of almost every step:
- the Product Manager reviews every plan and every task that is not code;
- the owner approves every epic, the product plan, every change to `spec` and `roadmap`, a marketing plan with no money in it, every site, and every outward act;
- a document the next task needs must first be merged.

Each check was added for a local reason. Together they keep the team waiting most of the time, often on a person who does not know it.

The founder kept three checks:
- A code change is reviewed, since code is what breaks.
- An act that costs money asks the owner, since money does not come back.
- Code reaches the integration branch only through a pull request the human merges.

Research is open to any agent; only secrets stay hidden. Two alternatives were rejected because they keep the cost the founder wants gone: keeping the checks and showing who waits on whom, and making each check a team setting.

## Decision

**Review only code.** A task changes code when it is a task, not an epic, whose assignee's role changes code (`changes_code`: the Software Developer and the UI/UX Designer). Readiness already holds every other role's task to the team's document paths, so this is known when the contract is written.
- A code task is reviewed as today: by the Architect, else a Developer. The Designer's browser check of a Developer's change to the interface comes first.
- Every other task, epics and private-folder tasks included, is accepted or rejected by the governor on the criteria Catervas runs itself (`command`, `test`, `artifact`). No reviewer session runs, and no Product Manager acceptance session either.
- Readiness refuses a `review` or a `human` criterion on a task that does not change code.
- A Designer's proposed plan is approved by Catervas at once.

**Only payments ask.** The owner is asked before an act that costs money:
- a marketing plan with money in it, and so its ad campaigns;
- a raised marketing budget;
- a purchase order, which the owner still places;
- a data source that costs money or whose cost is unknown;
- a connector call tagged `paid`, within ADR 0037's allowances.

The AI spending limits stay. No other approval remains: not of a contract, a product plan, a result, a document change, a site, a post, or a free outward act. A seller message is still drafted for the owner to send, because Catervas has no mailbox; that is a missing capability, not a check.

**Research is free.** Every role holds the `network` tier, every session may fetch and search the web, and every sandbox has the network.
- Every role reads the whole project, the other members of a project folder, and the private folders' workbooks.
- The Procurement Specialist has no site list.
- Secrets stay out of reach: the protected paths (`.env`, `.env.*`, `*.pem`, `*.key`, `.catervas/local/**`, and the team's own), the keychain, the model credential, and connector keys and tokens.

**Catervas never merges.** Every accepted task that has a branch is pushed and opened as a pull request, which the human merges. With no `origin` remote, the task waits on Today for a merge by hand, and counts as integrated once its branch is in the integration branch.

Nothing waits on a merge:
- A task starts from the branch of each accepted, unmerged task it depends on.
- An epic's tasks start from its plan's branch.
- An epic's plan task opens no pull request of its own; its plan reaches the integration branch inside its tasks' pull requests.
- A dependency counts once it is accepted.
- An epic is accepted once its tasks are merged.

## Consequences

The team goes from a request to pull requests, waiting only for money and merges. It saves:
- two Product Manager sessions on every task that is not code;
- one session on every Designer task;
- every owner approval of contracts, plans and documents.

The human's recurring duty is to merge pull requests, and Today lists each one waiting.

What cuts against it:
- A wrong plan, contract or document is caught only in the review of the code built from it, or when the owner reads it afterwards.
- Posts and document changes go out unseen. The owner can still stop a post on Today and edit any document on the Files page.
- There is one pull request per finished task. A pull request started from an unmerged branch carries that branch's commits, so the same commit can appear in several.
- Every sandbox has the network, so a Developer's commands can reach the internet.
- The governor gains its first acceptance and rejection rows, open only to tasks that do not change code. "Nobody grades their own homework" (5.1) now concerns code alone.
- Contract judging, which is on for every team by default (`judgment.required: always`), is an agent's check of a contract's plan before work starts. It contradicts the founder's second message, and is kept unchanged until the founder decides whether to turn it off by default.
