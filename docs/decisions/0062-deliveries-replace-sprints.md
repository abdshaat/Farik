# 0062. Deliveries replace sprints

Date: 2026-10-16
Status: accepted (the founder's approval of the epic CTV-9, question 34: "Remove sprints from the product entirely")

Supersedes ADR 0028.

Builds on ADR 0061 (only code is reviewed, only payments ask, Catervas never merges), which lands first.

Amends ADR 0037 (allowances count per UTC day), ADR 0042 (the raise's exception to the sprint hold has nothing left to except) and ADR 0027 (the incident fix's exception likewise; a deploy is no longer approved by a sprint's start, which phase 13 re-plans; the founder deferred how a deploy is approved to phase 13, 2026-10-16) and ADR 0024 (the human may grant a spent task room, `task.room_granted`, as well as more tries).

## Context

The epic CTV-9 names four frictions. Sprints add planning and ceremony the owner does not want. When the governor escalates, it is not obvious what to answer or what each answer does. An agent whose task is blocked or escalated sits idle while ready work waits. Nobody has used the product end to end as a new user would.

Asked about sprints, the founder chose to remove them from the product entirely: a task is picked up as soon as an agent is free. What the sprint ceremonies produced (the changelog entry, the Product Manager's proposed spec and roadmap changes, the retro notes, the digest of open escalations) still has to happen, on a trigger that needs no sprint. Options ruled out:

- A timer (every week): it runs ceremonies over nothing, or over too much, and is a setting to explain.
- A delivery per accepted task: a review session after every task costs more than it gives.
- A "close delivery" button: it asks the owner for the action the epic removes.

## Decision

- Sprints, sprint planning, the standup, sprint budgets, the Backlog hold and `plan_in_sprints` leave the product. A ready task goes to the first free agent of its role once that agent is under its WIP limit, with no action from the user.
- A delivery replaces a sprint as the unit of ceremony. The governor closes one (`delivery.closed`) when the first of these holds: an epic was accepted; at least `TASKS_PER_DELIVERY` (5, a constant, not a setting) tasks with no parent were accepted since the last delivery; the team is quiet (something was accepted, and no task is `ready`, `assigned`, `in_progress`, `verifying` or `rejected`). Changelog tasks never count towards a delivery. A new delivery closes only once the previous one's retro is held, unless no ceremony can run.
- At a close the governor files the changelog task (a whole contract, Product Manager, only criteria Catervas runs), then the review, the Architect's and Product Manager's refreshes and the retro run, with the digest of open escalations, as they ran after a sprint.
- Old projects keep working. A `team.yaml` that holds `plan_in_sprints` or a sprint budget key loads and the keys are ignored. An event log that holds sprint events loads and its tasks carry on; a task held in the old Backlog is assigned like any other ready task. Past events are never rewritten or deleted, and Catervas appends no sprint event again.
- A task that is blocked, escalated or waiting on the human does not count towards its assignee's WIP limit. When it clears it resumes with its previous assignee if they have room, else with the first free agent of its role (`task.reassigned`). A task has one holder at any time, and nobody reviews their own work.
- Every escalation a user can answer offers named choices. Each says in plain words what happens next, naming the agent; exactly one is suggested; no two send the same command. The web app and the command line read one list. The human may also give a task whose dollars or sessions are spent more room in the same answer (`task.room_granted`), and `catervas resolve` takes `--extra-tries` and `--extra-room`. The end of a run prints a command for each choice of every open escalation.
- Allowances (ADR 0037) and the channel's ambient allowance count per UTC day.

## Consequences

- Easier: there is no ceremony to start, and a ready task goes to the first free agent. A stuck agent takes other work. Every escalation says what each answer does.
- Harder: the user cannot batch work for a review by choice. A delivery's review costs a session after every epic or five tasks. The founder's milestone runs (phase 11 step 03) lose "one sprint carrying two requests" and are re-planned there. Phase 13's DevOps plans lose "a sprint's start approves its deploys"; the founder deferred how a deploy is approved to phase 13's planning, 2026-10-16.
- Cuts against the decision: a task handed to another agent leaves its previous holder's session context behind; the new holder resumes from the last commit and note. The old event kinds and projection columns stay for ever, so that old logs read.
