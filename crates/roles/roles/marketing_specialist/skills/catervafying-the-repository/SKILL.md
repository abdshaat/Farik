---
name: catervafying-the-repository
description: Use when a task of the project's Catervafication asks you to write the marketing plan and start on it.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/marketing/`.

## 1. What the task asks

Write the project's first marketing plan, propose it with `catervas_propose_marketing_plan`, and
start on it at once. Then write down what you started in `kickoff`.

## 2. Read the product plan first

Read `docs/catervas/product/` first: the spec and the roadmap, in their team versions. Then read
the rest of the project: its documentation, its site and its code where they tell you who it is for and
what it does. If something the marketing plan needs is missing, such as who the customers are,
say what is missing in the plan's assumptions; never fill the gap with a guess.

## 3. Write the plan

Follow your skills for the plan itself: the brand kit and persona first when the project has none,
the market it is for, the channels, the budget and the dates. Every claim about the product comes
from the project, and the plan says where.

## 4. Propose it

Propose the plan with `catervas_propose_marketing_plan`: its `text` for the owner and its
`agent_text` for the team go in the same call, with the `summary` the owner reads first. Catervas
writes `MP-<n>` and its agent version into your worktree; commit both together with
`catervas_git_commit`.

For each service the plan needs and the project has not connected, ask the owner with
`catervas_request_connection` and carry on.

## 5. Start

- The answer says the plan is approved: it spends nothing. Schedule its first posts with
  `catervas_schedule_post` now. Then write `docs/catervas/marketing/kickoff` in the team's format:
  name `MP-<n>`, the posts scheduled and the connections you asked for. Commit it and request
  `verifying`.
- The plan has money in it: it waits for the owner. End your turn.
  Do not ask for `verifying` while the plan waits.
  Your next session, after the owner's decision, schedules the posts and writes the kickoff.

Catervas checks that the plan file and the kickoff exist; only code is reviewed.
