# 0006. The agent works in bounded cycles that Praeco schedules

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent "long running and working constantly". Three ways to get that: one session kept open forever and fed new work; Managed Agents' scheduled deployments, which start sessions on a cron; or Praeco's own scheduler starting one bounded session per job. An endless session grows its context and its bill without a natural point to check either, and a crash loses whatever it was doing. Scheduled deployments run on Anthropic's clock, which cannot know that the workspace is paused, its day's budget spent, or a member's request waiting.

## Decision

Constant work is a stream of **cycles**: one job, one session, a budget in money, minutes and tool calls (SPEC 5.2). `praeco work` runs the scheduler, and the `cycles` table is the queue: each program has a cadence; a due program becomes a `queued` cycle; a member's request is queued ahead of programs; a `work` process claims the next queued cycle with `FOR UPDATE SKIP LOCKED`, so several processes never take the same one; a partial unique index allows one `running` cycle per workspace; and a cycle starts only when the workspace is not paused and its day and month have room for the cycle's whole budget. Up to `PRAECO_WORKERS` (default 4) cycles of different workspaces run at once per `work` process. A running cycle writes a heartbeat every 30 seconds; one whose heartbeat is two minutes old is marked `stopped` with `error` by any `work` process. Deciding which programs are due is `@praeco/core` `dueCycles`, and whether a queued cycle may start is `canStartCycle`; both are pure functions of the programs' state, the queue, the budget and the clock.

## Consequences

Each cycle is small enough to read in the activity log, priced exactly, and stopped cleanly; work resumes from the store, not from a session's memory. Pause, budgets and requests take effect at the next cycle boundary at the latest, and pause at once for the running one (SPEC 4.7).

What cuts against it: each cycle starts without the last one's conversation, so continuity lives in the records and the knowledge store, and the job's instructions have to point the agent at them; and Praeco owns a small queue instead of taking one off the shelf (pg-boss was the alternative), because the queue's rules (one per workspace, the budget gate, pause) are the product's and belong in `@praeco/core` and the `cycles` table, not in a library's options.
