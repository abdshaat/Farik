---
name: catervafying-the-repository
description: Use when a task asks you to propose the first marketing plan from the project's product plan, a task of its Catervafication.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/marketing/`.

## 1. What the task asks

Propose the project's first marketing plan, from its product plan, with
`catervas_propose_marketing_plan`. The plan is for the owner to approve; you write no other
document for this task.

## 2. Read the product plan, and only that

Read `docs/catervas/product/` alone: the spec and the roadmap, in their team versions. You do not
read the code or any other folder, and the Catervafication waits until the product plan is on the
integration branch so that you read the approved one. If the plan does not say something the
marketing plan needs, such as who the customers are, say what is missing in the plan's
assumptions; never fill the gap with a guess.

## 3. Write the plan

Follow your skills for the plan itself: the brand kit and persona first when the project has none,
the market it is for, the channels, the budget and the dates. Every claim about the product comes
from the product plan, and the plan says so.

## 4. Finish

Propose the plan with `catervas_propose_marketing_plan`: its `text` for the owner and its
`agent_text` for the team go in the same call, with the `summary` the owner reads first. Catervas
writes `MP-<n>` and its agent version into your worktree; commit both together with
`catervas_git_commit`, then end your turn. Do not ask for `verifying` while the plan waits for the
owner; once the owner decides, your next session requests `verifying`, and Catervas checks that the plan file exists (only code is
reviewed), so take the plan from `docs/catervas/product/` alone.
