# Standalone: agents work in parallel

Status: done
Branch: `feat/parallel-agents` (work outside a phase, one pull request to `main`)
Decision: ADR 0064 (`docs/decisions/0064-agents-work-in-parallel.md`)
Depends on: `origin/main` at `74b4a5af` (#45 merged), then `fix/plans-need-no-review` (ADR 0061,
SPEC 0.115), `feat/no-sprints` (ADR 0062, CTV-9, SPEC 0.116) and `feat/catervafication-work`
(ADR 0063, SPEC 0.117), all landed or rebased under this branch
Readiness confirmed by: pending (one round per plan file, against `docs/standards/workflow.md` stage 2)

## Before the first task

1. Rebase this branch onto `feat/catervafication-work`, itself rebased onto `feat/no-sprints` and
   `fix/plans-need-no-review`. ADR 0064 and these plans are the only commit of this branch.
2. `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.
3. Run `cargo xtask check` once for a clean baseline.

The plans name files and symbols of `main`. Where a line has moved, find the symbol by name.

These plans build on the three earlier branches and re-plan none of their work. They assume:
- CTV-9 plan 06 (`docs/plans/standalone/no-sprints/06-wip-and-resume.md`) landed:
  `transitions::open_tasks(board, agent_id, under)`, `TaskProjection.counted_seq`,
  `transitions::within_limit(team, board)` applied to rules 3, 6 and 7, `rules::resume_elsewhere`,
  `Transitions::reassign`, `task.reassigned`. Plan 01 here amends three of its definitions and
  says which.
- CTV-9 plan 05 removed sprints from the engine: `TickReport` may have lost its `Sprint` arm and
  the tick its sprint rules. Every "every arm of `TickReport`" below means the arms it has then.
- ADR 0061 kept a code task's review as it was (Catervas runs `command`, `test` and `artifact`
  criteria, then the reviewer's `verify` session, then a failed review is filed as a rejection in
  the reviewer's name); plan 02's test relies on exactly that for a Software Developer's task
  reviewed by an Architect.
- ADR 0063's chat tasks are tasks: nothing here treats them apart.

## Order

One executor (Sonnet 5.5) runs the plans in this order (2b after 2), one task at a time, each task one commit,
each task leaving `cargo xtask check` passing (it runs `pnpm check` too).

| Order | Plan | What it does |
|---|---|---|
| 1 | `01-wip.md` | A task under review no longer counts towards its assignee's WIP; a sent-back task counts from its rejection and stays with its author (P2) |
| 2 | `02-sessions-run-together.md` | The turn, `Work`, `Orchestrator::next`: each agent's session runs beside the others', one per agent; the founder's scenario as the test (P1, P3, P4 a and c) |
| 2b | `02b-chat-forks.md` | A chat is a fork: it runs at once beside its agent's work session, one per agent, bounded to answer quickly, told what the agent is doing now (the founder's answer of 2026-10-16) |
| 3 | `03-day-budget-stops-all.md` | A daily budget crossed by one session stops every other running session (P1's budgets, P4 b) |
| 4 | `04-driving-processes.md` | `catervas run`, `serve`, `plan` and `contract new` drive through `next`, print each start, and drain on stop or failure |
| 5 | `05-spec.md` | `docs/SPEC.md` revision 0.118 records it (hard rule 8, P5) |

Not in these plans: the web app. `team.activity` already derives each agent's activity from that
agent's own unended `session.started` (`crates/store/src/activity.rs`, `at_work`), Today lists every
agent's line, and the Board and the task page find the working agent by task. Plan 02's test asserts
that two agents read as working at once; no web change follows.

Never edit this repository's `.catervas/`.
