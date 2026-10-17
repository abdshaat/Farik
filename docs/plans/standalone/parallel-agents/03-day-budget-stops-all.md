# Standalone (parallel agents), plan 3: a spent day stops every running session

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.5 (the daily budget), F17; recorded by plan 5 (revision 0.118)
Depends on: plan 2 of this folder (committed on this branch)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

With sessions running at once, the team's daily budget still stops the team when it is spent: the
session whose cost crosses it ends as before, and every other session running in the process is
asked to stop at once, ends `aborted` with its task where it was, and no session starts until the
day has room. Out of scope: a task's own dollars and sessions (one session per task, unchanged), the
marketing budget (ADR 0042, not a session's), and any estimate or reservation of a session's cost.

## Decisions

- **Where.** Two calls of `stop_the_others(deps, &spec.session_id, DAY_SPENT_STOP)`, both in
  `orchestrator/session.rs`:
  1. In `read_to_end`, at the usage report whose `record(..)` answered a crossing of
     `BudgetScope::DayUsd` (`now` holds it), right after `crossed.extend(..)` and before that
     report's `handle.abort()?`: the others are asked to stop at the moment the day is crossed,
     without waiting for the turn (`request_stop` needs none), and also when the crossing session
     then fails (an abort that fails makes `drive` answer `Err`, with no `end.crossed` to read).
  2. In `run_session`, right after `work::session_ran(..)` (plan 2), when `drive` answered
     `Ok(end)` and `end.crossed` contains `BudgetScope::DayUsd`: once this session holds the turn
     again, a tick that read the budgets before the crossing was recorded, and started a session
     on them, has registered that session (a tick gives the turn back only after
     `register_session`), so it is stopped too; a tick that takes the turn after this reads the
     day as spent. Without it such a session would run to its end past the spent day.
  `crossed` holds a scope only when this session's own cost crossed it (`record_exhaustion`
  records each crossing it sees once). A session asked twice keeps the first reason
  (`request_stop`). Chose this over a check in the tick (a tick would see it only after the
  session's whole tick ended), over the second call alone (it waits for the turn, which a tick
  running criteria or integrating holds for minutes while the others spend) and over the cost
  recorder (it does not know the daemon).
- **How.** `stop_the_others` calls `deps.daemon.request_stop(id, reason)` for every id in
  `deps.daemon.session_ids()` but `own`; the session loop reading each one, woken by the stop,
  aborts it, as the second Ctrl-C does (`crates/cli/src/run.rs` `interrupted`). It records nothing
  itself: each session ends `aborted` through its own end, its task keeps its status, no
  escalation, and an implement session's note is left only for the causes `leave_note` already
  names. `own` is still registered at both calls (`end_session` runs after them) and is skipped.
- **The words.** `const DAY_SPENT_STOP: &str = "the team's daily budget is spent";` in
  `session.rs`, the same words as the tick's idle reason (`rules::DAY_SPENT`); the stop's reason is
  what a stopped session's later tool calls are denied with (`session_stopped: <reason>`).
- **Nothing else changes.** A new session's budgets are read under the turn before it starts, as
  before (`spent`), so a session that ended crossing the day is seen by the next tick. The channel's
  line "the daily budget is spent …" is posted once, by `record_exhaustion`, as now. The second
  Ctrl-C's abort and this stop are the same mechanism; neither escalates.
- No ADR beyond 0064; no wire or event change.

## File map

```
crates/runtime/src/orchestrator/session.rs     modifies: DAY_SPENT_STOP, stop_the_others, run_session calls it (Task 1)
crates/runtime/src/orchestrator.rs             tests:    the stop under concurrent sessions (Task 1)
```

## Interfaces

Consumes: plan 2's `Progress`, `Orchestrator::next`, `HeldAdapter`, `next_progress`, `until`,
`work::session_ran`; on `main`: `read_to_end` and its `record`, `Running`;
`UsageThenWaitAdapter::completing` (`crate::recorded::fixtures`), `catervas_core::pricing::Usage`,
`Harness` (`new`, `spent`, `in_progress`, `row`, `events`), `DaemonState::{session_ids,
request_stop}`, `SessionEnd.crossed`, `BudgetScope::DayUsd`.

Produces:

```rust
// orchestrator::session
/// Why a session is stopped when another's cost spends the day (ADR 0064).
const DAY_SPENT_STOP: &str = "the team's daily budget is spent";
/// Asks every session the daemon answers for in this process, but `own`, to stop for `reason`.
fn stop_the_others(deps: &OrchestratorDeps, own: &str, reason: &str);
```

## Tasks

### Task 1: the day's stop holds with sessions running at once

Files as the map.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `stops_every_running_session_once_the_day_is_spent` (`orchestrator.rs` `mod tests`, ignored
  "needs the git program"; watched to fail in its `until` of the second `Acted`, whose
  `next_progress` times out: nothing stops `dev-b`'s held session once `dev-a`'s ended) — the `Harness` team (`pm`, `dev-a`, `dev-b`; its daily budget is
  `a_team_wire`'s 20 dollars); `harness.spent(None, "s-0", 19.75)`; `in_progress("CTV-1", "dev-a",
  "dev-b")`, `in_progress("CTV-2", "dev-b", "dev-a")`; `HeldAdapter::new(Arc::new(
  UsageThenWaitAdapter::completing(Usage { input_tokens: 300_000, ..Usage::default() })))`, whose
  released session reports usage the shipped table prices between 30 cents and 3 dollars (it
  prices every model at 1 to 10 dollars per million input tokens), so the day crosses and the
  contract's 5 dollars and the default session's 400,000 input tokens do not, and ends
  `completed`. `until` `Started` of `dev-a`, CTV-1; `until` `Started` of `dev-b`, CTV-2;
  `release("dev-a")`; `until` a `Ticked(Acted { .. })` about CTV-1 or CTV-2, then `until` one
  about the other (they may come in either order).
  Then: `running()` is empty; the `session.ended` of `dev-b`'s session has reason `aborted`, and its one `cost.recorded` has `cost_usd` 0 (it
  reported no usage); no
  `escalation.raised` was appended; CTV-1 and CTV-2 are both `in_progress`; one `budget.exhausted`
  of scope `day_usd`; the next `next_progress` is `Ticked(Idle { why: "the team's daily budget is
  spent", until: None })`; `starts()` has 2 entries.

- [ ] `feat(runtime): a spent day stops every session running at once`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
