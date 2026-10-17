# Standalone (parallel agents), plan 4: the driving processes run agents at once

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 8.2 ("Driving the team"); recorded by plan 5 (revision 0.118)
Depends on: plans 2 and 3 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`catervas run`, `catervas serve`, `catervas plan` and `catervas contract new` drive the project
through `Orchestrator::next`, so their agents work at once. Each prints a line when a session
starts, and the tick's own line when it ends, as today. The first Ctrl-C (or `catervas stop`, or
`project.leave`) lets every running session end before the process finishes; the second aborts
them all. A failed tick stops the run the same way, so no session is left running when the daemon
shuts down. Out of scope: the web app (no change, README), `catervas_runtime`'s own loop (plan 2).

## Decisions

- **One loop.** The three drivers already share `crates/cli/src/run.rs` `ticks`; only it changes.
  It calls `driver.orchestrator.next(scope)` where it called `tick_within(scope)`, still under the
  same `select!` with `driver.interrupts`. The `is_stopped()` check at the top of the loop goes:
  `next` answers `None` once stopped and drained, and that is what prints `stopped` and answers
  `Ended::Stopped`.
- **What each answer does.**
  - `Some(Ok(Progress::Started { agent_id, task_id, purpose }))`: `last_idle = None`; prints, for a
    task, `"<task>: <agent> started its <purpose> session"`, and with no task `"<agent>: started its
    <purpose> session"`; JSON `{"started": "<purpose>", "agent_id": "<agent>", "task_id":
    "<task>"}`, `task_id` `null` with no task. `after` is not called: nothing has moved yet.
  - `Some(Ok(Progress::Ticked(report)))`: every arm of `TickReport` as today (the idle arms with
    their wait, the others printed and `after` called).
  - `Some(Err(error))`: `driver.orchestrator.stop()`, then `next` again until `None`, still hearing
    Ctrl-C, printing a `Started` or a non-idle `Ticked` as above (without `after`) and each further
    `Err` with `report_error`; then `Ended::Failed(error.to_string())`. Chose draining over
    returning at once, which would shut the daemon down under running sessions.
- **Ctrl-C's words.** `STOPPING` becomes `"stopping after the running sessions; Ctrl-C again aborts
  them"`; the second press's note `"aborting the running sessions"`; the third's stays. The second
  press already asks every session in `driver.daemon.session_ids()` to stop. `INTERRUPTED_BY` is
  unchanged.
- **Tests that read the old words.** `stops_after_the_session_then_aborts_it_on_ctrl_c`
  (`crates/cli/tests/running.rs`) asserts `"stopping after the running sessions"`; any other test
  the suite names for `"stopping after the current session"` or `"aborting the current session"`
  reads the new words. No other assertion changes.
- **`serve`'s idle line** is printed only when a fresh tick is idle with nothing running, which is
  when `next` answers `Idle`; so "idle: …" no longer appears while an agent works, as before.
- No change to `serve.rs` or `contract_new.rs`: both call `ticks`.

## File map

```
crates/cli/src/run.rs              modifies: ticks over next, the Started line, the drain on failure, STOPPING and the second press's words (Task 1)
crates/cli/tests/running.rs        modifies: the words of the Ctrl-C test; tests: two agents at once (Task 1)
```

## Interfaces

Consumes: plan 2's `Progress`, `Orchestrator::next`; `catervas_runtime::orchestrator::{Progress,
TickReport, TickScope}`; in `crates/cli/tests/running.rs`: `a_team`, `a_small_request`, `run`,
`run_with`, `given`, `wait_for`, `events`, `joined`, `UsageThenWaitAdapter::waiting`,
`Interrupts::Channel`.

Produces: no new item; `ticks` keeps its signature.

## Tasks

### Task 1: the drivers run agents at once, and say when each starts

Files as the map.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `runs_two_agents_at_once_and_stops_after_both` (`crates/cli/tests/running.rs`, ignored "needs
  the git program: cargo xtask check --integration"; watched to fail at its first wait, which
  times out with one `session.started`, since `ticks` still awaits each tick) — `a_team("run-two-
  at-once")`; `let task = a_small_request(&repository)`; `run(&repository.path, &["say", "@dev-a
  status?"])` exits 0; `UsageThenWaitAdapter::waiting(Usage::default())` as the engine and an
  interrupt channel, `catervas run` on a thread as `exits_130_after_one_ctrl_c_once_the_session_ends`
  runs it. `wait_for(&repository, EventKind::SessionStarted, 2)`; then no `session.ended` is in the
  log. One interrupt; after 300 ms `adapter.aborts()` is 0 and the run's thread has not finished;
  `adapter.complete()`; the run exits 130; its standard output has the line `stopped`, the line
  `dev-a: started its conversation session` and the line `<task>: pm started its refine session`;
  `adapter.started().len()` is 2.

- [ ] `feat(cli): drive agents at once and say when each session starts`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
