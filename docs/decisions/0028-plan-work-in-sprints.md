# 0028. Plan work in sprints, with ready work waiting in a backlog

Date: 2026-10-01
Status: accepted (the founder, in conversation, 2026-10-01)
Mockups approved by: the founder, 2026-10-01 (canvas version 1790857656-ceac)

## Context

Sprints are optional (spec 3, "Sprint"; phase 4). With none open, the board flows: a task that becomes `ready` is assigned at once (`in_the_open_sprint`: "with none open, any task"). The command line hid this. In phase 4, `farik contract new` ran only the refining rules, and nothing was assigned until `farik run` after `farik sprint start`, so one sprint's planning could take two requests.

`farik serve` runs every rule all the time. The milestone runbook's readiness review (finding B1, 2026-10-01; the runbook was phase 6 step 15 then, and is step 16 now) showed what follows in the browser:
- a ready task is assigned before the human can start a sprint;
- an approved epic is assigned to its breaker-down at once;
- "Start sprint" then opens a sprint with no candidate, its planning is passed over, and no review or retro follows;
- starting the sprint first does not help either: planning fires on the first candidate that becomes ready, and the second request never enters the sprint.

So the milestone run, which has to show one sprint carrying two requests, cannot be driven from the browser. A user who wants their team to work in sprints has no way to gather work for one.

The options:
- **Accept one request outside the sprint.** Amend the run's pass criteria and change nothing else. The product would still have no way to gather work, and phase 4's team exit would be met only in part.
- **Hold the work by pausing.** Pause both Developers until the sprint opens, or pause the team. The product refuses the first: the last active agent of a required role cannot be paused (`last_of_role`, spec 10's foolproof configuration), and a team file with no active Developer does not validate. Pausing the team stops triage and refining too, so the questions and plans wait, and a non-technical user would have to learn a workaround.
- **A team policy, "Plan work in sprints".** With it on, the team keeps preparing work at any time, but nothing is assigned or built outside an open sprint. Ready work waits in a Backlog, and starting a sprint plans it. This is the founder's choice, in the founder's words: "Should be in a backlog".

## Decision

A team policy, `policy.plan_in_sprints`, a boolean in `team.yaml`.

With it on:
- triage, refining, the Product Manager's questions, the plan check, approvals, an epic's breakdown, conversations, chats and ceremonies run at any time;
- no task is assigned outside the open sprint, so nothing new is built outside one;
- ready work waits in the Backlog;
- work already under way outside a sprint when the policy is switched on finishes: a task past `ready` (`assigned`, `in_progress`, `blocked`, `verifying` or `rejected`) carries on to acceptance, rework included, and only `ready` tasks wait (the founder, 2026-10-01);
- a sprint ended early sends its unfinished tasks to the Backlog (the founder, 2026-10-01): each is marked as left for the Backlog, and is neither started, worked on nor reworked until a sprint plans it, a session already running finishing first;
- work that becomes ready during a sprint waits for the next sprint, and is not added to the open one; a task filed under an epic the sprint already holds still joins it, as today;
- starting a sprint plans the Backlog, through the planning ceremony that exists.

The one exception is ADR 0027's incident fix: it skips planning and runs even outside a sprint. Phase 9 step 06 builds that path, and keeps the exception.

It is on for every team setup creates, for `farik init`'s starter team (the founder, 2026-10-01), and for a team made from a template that has it on. It is off when the key is absent, so every existing project behaves as it does today. The user switches it in Settings, under "Your team's rules", or behind setup's Advanced switch.

The design is `docs/design/sprint-backlog.md`. It is built in phase 6 step 15, "Sprints gather ready work", before the milestone runbook, which becomes step 16.

## Consequences

Easier:
- The milestone run can gather both requests into one sprint from the browser: file both, answer, approve, then start the sprint.
- A user sees what is ready and decides when work starts, which is what a sprint is for.
- Nothing changes for a project made before this, or for a script driving `farik run` on such a project.

Harder:
- A new team does nothing visible until its user starts a sprint, so a first user may wait without knowing why. Today and the Board must say so in plain words, and the thirty-minute test will show whether they do.
- The lifecycle gains a condition that is not a state: a `ready` row can be held by a policy, not by a dependency or a full agent. Assignment, the planning candidates, `farik_plan_sprint`, the Board's lanes, Today, and the idle report each learn it.
- An unfinished task left by a sprint ended early stops after its running session and waits for the next sprint, where today it carries on. The end-early dialog says so. Telling it from work that was under way at the switch takes a mark: `sprint.ended` says whether its left tasks wait in the Backlog, and the projection keeps that on each task until a sprint plans it.
- `farik init`'s starter team plans in sprints too, so `farik run` on a new project does nothing until `farik sprint start`. Tests and journeys that drive work without a sprint say `plan_in_sprints: false` in their team.
- Phase 9 step 06 has to keep the incident fix's exception, or a broken production waits for a sprint.
