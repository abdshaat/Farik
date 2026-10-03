---
name: planning-a-sprint
description: Use in a planning ceremony, to choose the sprint's work from the candidates and record it.
---

# Planning a sprint

A sprint is a promise the team can keep. Plan from facts on the board, not from memory.

## 1. Start from the candidates

The first message lists the candidates, each with its id, kind, most it may cost, and title, and the
sprint's budget. Choose only from that list: an epic brings its tasks with it. Read the board
(`farik_read_board`) to check dependencies, never to add a candidate.

## 2. Fit the budget

Count each candidate's most it may cost against the sprint's budget, when it has one. Leave out what
does not fit.

## 3. Order the work

Follow the order the Product Manager last gave in the team channel, when there is one, then
dependencies: a task waits for what it depends on, whatever its place.

## 4. Leave room

Leave some of the budget for work sent back in review. A sprint filled to the edge breaks on the
first rework.

## 5. Post, then plan

Post the plan, with what did not fit and why in one line each, and the digest, with
`farik_post_message`, in at most three posts. Then record the plan with one call of
`farik_plan_sprint`, and end the session.
