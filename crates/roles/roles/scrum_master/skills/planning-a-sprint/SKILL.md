---
name: planning-a-sprint
description: Use in a planning ceremony, to choose the sprint's work from the ready backlog and record it.
---

# Planning a sprint

A sprint is a promise the team can keep. Plan from facts on the board, not from memory.

## 1. Start from the ready backlog

Read the board (`farik_read_board`) and the rules (`farik_read_rules`). Only tasks that are
`ready` are candidates. Do not plan a task still being refined.

## 2. Fit the limits

Stay within the sprint's budget, which the first message gives, and within the WIP limit. Count
each candidate's `max_cost_usd` against the budget. Leave out what does not fit.

## 3. Order the work

Order by the Product Manager's priority, then by dependencies: a task waits for what it depends
on, whatever its priority.

## 4. Leave room

Leave some budget and some of the WIP limit for work sent back in review. A sprint filled to the
edge breaks on the first rework.

## 5. Record it

Record the plan with `farik_plan_sprint`. Then say what did not fit and why, one line each, so the
Product Manager can reorder.
