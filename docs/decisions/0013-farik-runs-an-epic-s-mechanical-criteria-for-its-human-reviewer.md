# 0013. Farik runs an epic's mechanical criteria for its human reviewer

Date: 2026-09-22
Status: accepted

## Context

An epic's reviewer is the Product Manager when the Scrum Master broke it down, and the human when the Product Manager did (`docs/SPEC.md` 5.1, 5.16 item 4). No Scrum Master ships until phase 4, so every epic in phase 3 and in the Milestone 0 exit (phase 3 step 17) is reviewed by the human. The Definition of Done (5.4 item 1) says every exit criterion is run by the reviewer, independently of the assignee, and passed. A human does not run commands in a sandbox, and an epic has no branch of its own: its tasks' branches are merged into the integration branch.

Two options were on the table when the step 14 plan was written. In the first, the human's acceptance of the epic counts as the reviewer's run of every criterion, and its message counts as the review note. Farik runs nothing. That is cheap, but a `command` criterion would then be "passed" by someone who never ran it, which is the "I ran the tests and they passed" that 5.4 exists to refuse. In the second, Farik runs the epic's `command`, `test`, and `artifact` criteria as the reviewer's run, on a detached worktree of the integration branch. That is the same thing step 12 does for a task's reviewer, whose mechanical criteria Farik runs and records with `recorded_by: governor`. The human then answers the `review` and `human` criteria through the acceptance.

## Decision

The founder chose the second option on 2026-09-22.

When an epic reviewed by the human is `verifying`, and none of its tasks is still awaiting integration, Farik runs each of its `command`, `test`, and `artifact` criteria. The runs happen in a sandbox on a detached worktree at the integration branch's head, and each result is recorded as `criterion.recorded { run_by: reviewer, recorded_by: governor }`. The human's `human.accepted { subject: result }` is accepted only once every one of those runs has passed. Its message is then read as the review note, and the acceptance stands for the reviewer's answer to each `review` criterion and the human's answer to each `human` criterion. 5.4 item 1 holds for epics without an exception.

## Consequences

An epic's own checks run against the code that was actually integrated, not against any one task's branch, so a check that only the combined work passes, or that one task broke for another, is caught before the human is asked. The run waits until every accepted task under the epic is integrated. Under the `manual` and `pull_request` policies, that means the epic waits on the human's merges before it can be accepted.

It costs one more sandbox per epic verification and one more worktree, which the step 13 cleanup removes with the epic's own. A `test` criterion that sets `new_tests_required` cannot be checked against a base branch for an epic, because an epic has no branch to compare. Its run records only whether the tests pass, and the new-tests check stays with the epic's tasks.

When a mechanical criterion fails, the epic cannot be accepted. The human's way on is to escalate it and send it back to `in_progress` with a message, and the Product Manager then files the work that is missing.

Phase 4, which ships the Scrum Master, reviews the epics the Scrum Master broke down with the Product Manager as reviewer. For those, step 12's reviewer session applies over the same detached-worktree run.
