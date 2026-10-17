# 0061. Agents plan without review; only finished results are verified

Date: 2026-10-16
Status: accepted (the founder, 2026-10-16: "Change it so that any agent can plan and start working. No other agent need to verify the plan, agents only need to verify final results.")

Supersedes, in part:
- ADR 0026's rule that the Product Manager approves the UI/UX Designer's plan before the Designer changes anything (`catervas_decide_design_plan`).

Amends:
- ADR 0051's planning flow ("PM → Architect → PM → Scrum Master"): the Architect's plan task is no longer reviewed by the Product Manager, and the Product Manager files the plan's tasks once the plan task is accepted, not once the plan is on the integration branch.

## Context

The founder saw this live on 2026-10-16: "When I ask for a change, the architect moves the task to completed and opens a PR with the plan and no implementation happens." Under `integration: pull_request`, an epic's plan task (the Architect's docs task whose `allowed_paths` are exactly its `plan`, 5.16 item 3) is reviewed by the Product Manager, accepted by the Product Manager, and opened as a pull request that holds only the plan. The Product Manager files the plan's tasks only once the plan is on the integration branch, that is after the human merges that pull request, and nothing tells the human that the work waits on that merge. The epic looks finished while nothing is built.

The Designer's plan has the same shape: its explore session proposes a plan, and a Product Manager session must approve it before the implement session runs, so every Designer task pays for one more session and waits on one more agent.

The options on the table were these. Keep the reviews and tell the human that a plan's pull request blocks the work: it keeps two agent sessions per plan and a merge the human does not care about, and it is the cause, not a fix. Merge a plan task on its own under every policy: it overrides the human's `pull_request` choice for one kind of task. Or verify plans mechanically only, let the work start from the plan at once, and let the plan reach the integration branch inside the work's own pull requests; the founder chose this one.

## Decision

No agent reviews or approves a plan. An epic's plan task whose criteria Catervas runs itself is accepted or rejected by the governor on those results, with no reviewer session and no Product Manager session; the Product Manager files the plan's tasks as soon as it is accepted, reading the plan from the plan task's branch while the integration branch does not hold it; those tasks start from the plan task's branch, so their pull requests carry the plan, and under `pull_request` and `manual` the plan task opens no pull request of its own and counts as integrated once its plan is on the integration branch. A Designer's proposed plan is approved by Catervas at once, and its implement session follows. Every review of finished work stays: the reviewer's of every other task, the Architect's of the Designer's change, the Designer's browser check of a Developer's UI change, the epic's own review, and every human approval and acceptance.

## Consequences

Asking for a change now leads straight to work: one Architect session writes the plan, and the next tick asks the Product Manager for its tasks. Two agent sessions per epic and one per Designer task are saved, and a human under `pull_request` merges pull requests that hold working changes, never a plan alone.

A wrong plan is no longer caught before work starts; it is caught where the founder wants checks, in the review of each finished task and of the epic, and a bad plan costs the sessions of the tasks built from it. The plan's own quality now rests on the Architect's skill and on the `artifact` criterion that it exists.

The governor gains its first acceptance and rejection rows (`verifying -> accepted` and `verifying -> rejected`, actor `governor`), opened only to a plan task whose every criterion is one Catervas runs; "nobody grades their own homework" (5.1) holds for every other task, and a plan task the Product Manager gives a `review` criterion is reviewed as before. The plan's commit reaches the integration branch through each lane's pull request, so several pull requests may carry the same commit; git takes that without conflict, since the file is the same in each. `catervas_decide_design_plan` and its session go away, while `design_plan.returned` stays readable in older logs, and so do the returns it counts against a task's iterations.
