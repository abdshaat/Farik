# Standalone: the Catervafication creates work, and a chat message can be a task

Status: draft
Branch: `feat/catervafication-work` (work outside a phase, one pull request to `main`)
Decision: ADR 0063 (`docs/decisions/0063-catervafication-creates-work-and-chats-create-tasks.md`)
Depends on: `origin/main` at `74b4a5af` (#45 merged), then `fix/plans-need-no-review` (ADR 0061,
SPEC 0.115) and `feat/no-sprints` (ADR 0062, SPEC 0.116), both landed or rebased under this branch
Readiness confirmed by: pending (one round per plan file, against `docs/standards/workflow.md` stage 2)

## Before the first task

1. Rebase this branch onto `feat/no-sprints`, itself rebased onto `fix/plans-need-no-review`.
2. `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.
3. Run `cargo xtask check` once for a clean baseline.

The plans name files and symbols of `main`. Where a line has moved, find the symbol by name.

These plans assume the following of the two earlier branches, and change none of it:
- `catervas_core::team::reviewed_by_an_agent(contract)` exists (ADR 0061 plan 1). A task that does
  not change code is accepted by the governor on its `command`, `test` and `artifact` criteria,
  and readiness refuses `review` and `human` criteria on it (`NoReviewOutsideCode`).
- The Catervafication's templates carry no `review` criterion, and its epic's C1 is the
  `command` `test -d docs/catervas` (ADR 0061 plan 1, Task 4).
- `FileDue::MarketingWaits` is gone, and the Marketing task is filed with `dependencies` naming the
  Product Manager's task (ADR 0061 plan 5).
- `catervas_propose_marketing_plan` records `marketing_plan.approved` at once for a plan that
  spends nothing (ADR 0061 plan 5, Task 4).
- The Marketing Specialist reads the whole project (ADR 0061 plan 6).
- No sprint is opened or joined: the Catervafication opens none, and `join_epics_sprint` and
  `Started.sprint` are gone (ADR 0062 plan 5).

If any of these is not so when a task starts, stop and send it back to planning. Do not decide it.

## Order

Each plan is copied from `docs/plans/step-template.md`. Each consumes only what the plans before
it committed. Every task is one commit and leaves `cargo xtask check` passing.

| # | File | What it does |
|---|---|---|
| 1 | `01-asking-for-connections.md` | `catervas_request_connection` and `connection.requested`; `team.get` answers `requested_connections`; Today's suggestions list a requested pair first, with the agent's reason |
| 2 | `02-filing-bugs.md` | `is_hunt` and `hunt_report` in core, a hunt not reviewed by an agent; `task.created.found_by`; `catervas_file_bug`, bounded and deduplicated |
| 3 | `03-catervafication-creates-work.md` | The two hunts and the Marketing task filed in the first tick; the kickoff document; a Designer's hunt skips explore; the skills; the propose answer once approved at once |
| 4 | `04-tasks-from-chat.md` | `chat_message_post { as_task }`, `chat_message.posted.as_task` and `task.created.given_to`; the agent refines and is assigned its own task, its role fixed; the chat does not answer it |
| 5 | `05-web.md` | "Send as a task" in a one-to-one chat and the task's link under the message; the Catervafication's words; the activity word for a connection request |
| 6 | `06-record.md` | `docs/SPEC.md` revision 0.117 and the plans' ticks (hard rule 8) |

## Wire changes, all in one place

| Where | Change | Plan |
|---|---|---|
| `event.schema.json` | new kind `connection.requested`, body `{ server, why }` | 1 |
| `rpc.schema.json` | `teamGetResult.requested_connections`, required: `[{ agent, server, why }]` | 1 |
| `event.schema.json` | `taskCreatedBody.found_by`, a task id | 2 |
| `event.schema.json` | `chatMessagePostedBody.as_task`, `const: true` | 4 |
| `event.schema.json` | `taskCreatedBody.given_to`, an agent id | 4 |
| `command.schema.json` | `chatMessagePostBody.as_task`, boolean, optional | 4 |

## Not in these plans

- Hunts for a new project (no code yet) and for a project folder (ADR 0057). ADR 0063 leaves both
  for a later decision.
- A recurring hunt. The owner gives one with "Send as a task" in the agent's chat.
- Screenshots as bug evidence. Evidence is text: a URL, a console line, a command and its output.
- The founder's live run. It runs once the branch has landed, from the web app on an existing
  repository.
