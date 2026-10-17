# Standalone (parallel agents), plan 2: each agent's session runs beside the others'

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 8.2 ("Driving the team"), 5.2, 5.14; recorded by plan 5 (revision 0.118)
Depends on: plan 1 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The orchestrator runs one session per agent at once: while the Architect reviews CTV-1 the
Developer builds CTV-2, and while the Developer reworks CTV-1 the Architect reviews CTV-2, the
founder's example. Everything a tick does but its session's run (reading the board, the governor,
Catervas's criteria, integration, worktrees, sandboxes, the session's start and end records) stays
one at a time under the process's turn. An agent never runs two sessions at once and no rule acts
on a task whose session runs. Out of scope: the daily budget's stop of running sessions (plan 3),
the command line's loop and its lines (plan 4), the web app (no change, README).

## Decisions

See ADR 0064 for the rules; this plan fixes the mechanism.

- **`Work`**, a new `pub struct` in `crates/runtime/src/orchestrator/work.rs`, re-exported as
  `catervas_runtime::orchestrator::Work`, `#[derive(Debug, Default)]`: `turn:
  Arc<tokio::sync::Mutex<()>>` and `busy: std::sync::Mutex<BTreeMap<String, Option<TaskId>>>`
  (agent id to the task its session is about). `OrchestratorDeps` gains `pub work: Arc<Work>`,
  doc "Which agents have a session running in this process, and the turn every tick holds but while
  its session runs (ADR 0064)."; its four constructions (`crates/cli/src/start.rs` twice,
  `orchestrator/fixtures.rs` twice) pass `work: Arc::default()`. Chose `OrchestratorDeps` over the
  `Orchestrator` because `run_session` and every rule's guard take `&OrchestratorDeps`.
- **The claim.** `run_session` (`orchestrator/session.rs`) calls `deps.work.claim(agent, task)`
  first of all, before `refresh_signed_in`, and holds the `Claim` to its end; dropping it removes
  the agent from `busy` (also on a panic's unwind). `claim` refuses an agent already in `busy` with
  `OrchestratorError::Refused { reason: "agent_busy: <agent> has a session running" }`, before
  anything is recorded. No rule reaches it once `unavailable` guards (below); it is the guarantee.
- **The turn.** A tick spawned by `next` runs inside `work::holding_the_turn`, a
  `tokio::task_local!` slot (`RefCell<TickSlot>`, `TickSlot { turn: Option<OwnedMutexGuard<()>>,
  started: Option<oneshot::Sender<Started>> }`). In `run_session`, just before `drive(..)`:
  `work::session_runs(Started { agent_id, task_id: spec.task_id.clone(), purpose })`, which in such
  a slot drops the turn guard and sends `started` the first time a tick calls it (a later session
  of the same tick sends nothing); right after `drive(..).await`, before
  `deps.daemon.end_session`: `work::session_ran(&deps.work).await`, which in such a slot waits for
  `Arc::clone(&work.turn).lock_owned()` and puts the guard back. Outside a slot (a test's
  `tick()`, `catervas plan` before plan 4) both do nothing, so serial ticks behave as today.
  `purpose` is `crate::sessions::purpose_wire(spec.purpose).to_string()`, the word
  `session.started` records (`purpose_wire` becomes `pub(crate)`). Chose a task-local over a new
  parameter on `run_session` and `SessionAsk`, whose 22 callers would all change for one value.
- **Rules pass over a busy agent as over a sleeping one.** `rules::asleep` is renamed
  `rules::unavailable` with the same signature; it answers `Ok(true)` first when
  `deps.work.is_busy(agent.id.as_str())`, without touching `slept`, then the sleep check as now. Every
  caller (`rules.rs`, `verify.rs`, `design.rs`, `pipeline.rs`, `requests.rs`, and any CTV-9 or ADR
  0061/0063 added; `grep -n "asleep(deps," crates/runtime/src/orchestrator` lists them) calls
  `unavailable`. Its doc: "Whether `agent` can start no session now: one of its runs in this
  process (ADR 0064), or it is asleep until its provider's limit resets (5.5) ...", the rest as now.
- **Rules pass over a task whose session runs.** `rules::tick` reads `let busy =
  deps.work.busy_tasks();` before the board; `in_scope(scope)` becomes `in_scope(scope, &busy)`
  and also refuses a row whose `task_id` is in `busy`; `budget` gains `busy: &BTreeSet<TaskId>` and
  skips such a row. The `board` passed to `ready`, `has_room` and `within_limit` stays whole, so a
  busy task still counts towards its agent's limit.
- **`next`**, on `Orchestrator`, holding `ticks: tokio::sync::Mutex<JoinSet<Result<TickReport,
  OrchestratorError>>>` (new field, empty in `new`). It locks `ticks` for the whole call, then loops:
  1. stopped (`is_stopped`): with the set empty answer `None`; else wait for the first tick to end
     and answer its report, or loop to 1 when that report is `Idle`.
  2. Take the turn (`lock_owned`), spawn `holding_the_turn(turn, sender, me.tick_within(&scope))`
     on the set with `me = Arc::clone(self)` and `scope` cloned, and keep its task id.
  3. `select!` on the receiver (an `Ok(started)` answers `Progress::Started`; an `Err`, the tick
     ended with no session, disables that branch) and `join_next_with_id`.
  4. A tick that ended which is not the one just spawned: answer its report, or loop to 1 when it
     is `Idle` (only a fresh tick's idleness with nothing running is ever answered, so a driver
     never ends while sessions run).
  5. The one just spawned, `Ok(TickReport::Idle { until, .. })`, while the set still holds ticks:
     wait for the first of: a tick ending (answer its report, `Idle` excepted as in 4); `commands`, `daemon.wakes()` or
     `stops` notified (taken as `wait_until` takes them); `sleeper.sleep_until` of `until`, capped
     at `RECHECK` from now (`until` none: `RECHECK` from now). Then loop to 1.
  6. Anything else: answer it.
  A `JoinError` is `OrchestratorError::Refused { reason: "tick_failed: <error>" }`. Chose a fresh
  tick per call over ticking only when a session ends, so that a request filed or a command given
  while sessions run reaches an idle agent within a tick.
- **`Progress`**, a new `pub enum` beside `TickReport` in `orchestrator.rs`: `Started { agent_id:
  String, task_id: Option<TaskId>, purpose: String }` and `Ticked(TickReport)`. Chose a new type
  over a new `TickReport` arm, so that every existing match of `TickReport` (tests, the command line
  until plan 4) stays as it is.
- **`run_until_idle`** becomes `pub async fn run_until_idle(self: &Arc<Self>) -> Result<(),
  OrchestratorError>` over `next(&TickScope::default())`: `None` or `Ticked(Idle { until: None })`
  ends it `Ok`; `Ticked(Idle { until: Some(u) })` waits `wait_until(u)` and goes on; `Started` and
  every other report go on; the first `Err` calls `stop`, keeps calling `next` until `None`
  (ignoring what those answer), and is returned. Its callers in tests hold an `Arc<Orchestrator>`
  (`Arc::new(harness.orchestrator(..))`); `drive_one_request` takes `&Arc<super::Orchestrator>`.
  A whole-run test whose transcripts were listed in the order sessions ended under one session at a
  time, and which now fails because a transcript is played by another session, lists them in the
  order sessions start (the `session.started` events show it); its assertions stay.
- **Docs.** `orchestrator.rs`'s module doc: "Each tick reads the board and does the first thing on
  it that needs doing, and says what it did; a session it starts runs beside the sessions of other
  agents while the next ticks go on (ADR 0064)." The `Orchestrator` doc loses its "ponytail" line.
  `stop`: "Stops `next` and `run_until_idle` from ticking again, and ends a wait at once. Sessions
  already running run to their end." `tick`'s "running at most one session to its end" stays true
  of a tick alone.
- **The fixture `HeldAdapter`** (`orchestrator/fixtures.rs`): a `RuntimeAdapter` over an inner one
  whose every session is held, unstarted, until the test releases it by agent. `release(agent)`
  starts it on the inner adapter then and forwards every inner event, so it plays as the inner
  adapter plays it. A held session aborted before its release ends `aborted` at once; abort after
  release reaches the inner session. `resume` goes to the inner adapter, unheld. It records each
  start with the agents whose sessions were running then; a session runs from its start until its
  `Ended` event is delivered. Tests run under `#[tokio::test]` (one thread), so the order of starts
  is decided by the turn alone.

## File map

```
crates/runtime/src/orchestrator/work.rs        creates: Work, Claim, Started, holding_the_turn, session_runs, session_ran (Task 1)
crates/runtime/src/orchestrator.rs             modifies: mod work, pub use Work, OrchestratorDeps.work, Orchestrator.ticks, Progress, next, run_until_idle, docs; mod tests (Task 1)
crates/runtime/src/orchestrator/session.rs     modifies: run_session claims, hands the turn back and takes it again (Task 1)
crates/runtime/src/sessions.rs                 modifies: purpose_wire is pub(crate) (Task 1)
crates/runtime/src/orchestrator/rules.rs       modifies: unavailable, in_scope and budget pass over busy tasks; run_until_idle callers (Task 1)
crates/runtime/src/orchestrator/{verify,design,pipeline,requests}.rs   modifies: call unavailable (Task 1)
crates/runtime/src/orchestrator/human.rs       modifies: run_until_idle callers hold an Arc (Task 1)
crates/runtime/src/orchestrator/fixtures.rs    modifies: work in both constructions; HeldAdapter, HeldStart; in_progress_with, assigned_with (Task 1)
crates/cli/src/start.rs                        modifies: work: Arc::default() in both OrchestratorDeps (Task 1)
```

## Interfaces

Consumes: plan 1's `counts_towards_wip`; CTV-9's `within_limit`; on `main`: `run_session`,
`drive`, `SessionSpec { agent_id, task_id, purpose }`, `DaemonState::{end_session, wakes}`,
`Orchestrator::{tick_within, wait_until, stop, is_stopped}`, `RECHECK`, `Sleeper`, `Harness`
(`new`, `file`, `project.moved`, `worktree`, `branch`, `recorded`, `orchestrator`, `row`, `events`),
transcripts `implement_css_ctv_2`, `review_writes_note`, `implement_finishes_ctv_1`,
`reads_a_file`, `catervas_store::activity::{activity, ActivityState}`, `an_agent_wire`.

Produces:

```rust
// catervas_runtime::orchestrator
#[derive(Debug, Default)]
pub struct Work { /* turn, busy */ }
impl Work {
    pub(crate) fn is_busy(&self, agent_id: &str) -> bool;
    pub(crate) fn busy_tasks(&self) -> BTreeSet<TaskId>;
    /// # Errors
    /// `Refused` with `agent_busy: <agent> has a session running` when `agent_id` holds one.
    pub(crate) fn claim(&self, agent_id: &str, task_id: Option<&TaskId>) -> Result<Claim<'_>, OrchestratorError>;
}
pub(crate) struct Claim<'a> { /* work, agent_id */ }   // Drop: the agent leaves `busy`
pub struct OrchestratorDeps { /* … */ pub work: Arc<Work> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// A tick started a session, which runs on while the next ticks go on; its tick reports when it ends.
    Started { agent_id: String, task_id: Option<TaskId>, purpose: String },
    /// A tick ended, and said this.
    Ticked(TickReport),
}
impl Orchestrator {
    /// The next thing the ticks driving the project did (ADR 0064); `None` once `stop` was called and no tick runs.
    pub async fn next(self: &Arc<Self>, scope: &TickScope) -> Option<Result<Progress, OrchestratorError>>;
    pub async fn run_until_idle(self: &Arc<Self>) -> Result<(), OrchestratorError>;
}

// orchestrator::work (pub(super))
pub(super) struct Started { pub(super) agent_id: String, pub(super) task_id: Option<TaskId>, pub(super) purpose: String }
pub(super) async fn holding_the_turn<T>(turn: tokio::sync::OwnedMutexGuard<()>, started: tokio::sync::oneshot::Sender<Started>, tick: impl Future<Output = T>) -> T;
pub(super) fn session_runs(started: Started);
pub(super) async fn session_ran(work: &Work);

// orchestrator::fixtures
pub(crate) struct HeldAdapter { /* inner, held, starts, running */ }
pub(crate) struct HeldStart { pub(crate) agent: String, pub(crate) task: Option<String>, pub(crate) purpose: SessionPurpose, pub(crate) alongside: Vec<String> }
impl HeldAdapter {
    pub(crate) fn new(inner: Arc<dyn RuntimeAdapter>) -> Arc<Self>;
    /// Starts `agent`'s oldest held session on the inner adapter. Panics when none is held.
    pub(crate) fn release(&self, agent: &str);
    pub(crate) fn starts(&self) -> Vec<HeldStart>;
    /// The agents whose sessions run now, sorted.
    pub(crate) fn running(&self) -> Vec<String>;
}
impl RuntimeAdapter for HeldAdapter {}
impl Harness {
    /// `assigned` with `change` applied to the contract's wire, as `file` applies it.
    pub(crate) fn assigned_with(&self, task: &str, assignee: &str, reviewer: &str, change: impl FnOnce(&mut Value));
    /// `in_progress` with `change`, its worktree made.
    pub(crate) fn in_progress_with(&self, task: &str, assignee: &str, reviewer: &str, change: impl FnOnce(&mut Value));
}
```

`assigned` and `in_progress` call the new ones with `|_| {}`.

## Tasks

### Task 1: sessions run together, one per agent

Files as the map. Test helpers in `orchestrator.rs` `mod tests`: `next_progress(orchestrator) ->
Progress` (`next(&TickScope::default())` under a ten-second `tokio::time::timeout`, expecting
`Some(Ok(_))` with messages naming what was missing) and `until(orchestrator, matches: impl
Fn(&Progress) -> bool) -> Vec<Progress>` (calls `next_progress` until one matches, at most ten
times, and answers every progress seen, the match last).

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `reviews_one_task_while_its_developer_builds_the_next` (`orchestrator.rs` `mod tests`, ignored
  "needs the git program"; watched to fail with a first `next` that awaits `tick_within` to its end
  and answers `Some(Ok(Progress::Ticked(report)))`: step (b) times out, since `dev-a`'s CTV-2
  session cannot start while `arch`'s review is held) — `Harness::new` with `wire["agents"]` set
  to `pm` (`product_manager`), `dev-a` (`software_developer`) and `arch` (`architect`), limit 1.
  CTV-1: `in_progress_with("CTV-1", "dev-a", "arch", ..)` and CTV-2: `assigned_with("CTV-2",
  "dev-a", "arch", ..)`, both with `reviewer_role` `architect` and `allowed_paths` `["done.txt",
  "site/style.css"]`. `HeldAdapter::new(harness.recorded(vec![implement_css_ctv_2(),
  review_writes_note(), implement_finishes_ctv_1()]))`, an `Arc` of `harness.orchestrator(adapter)`.
  (a) `until` `Started { agent_id: "dev-a", task_id: CTV-1, purpose: "implement" }`; `release("dev-a")`;
  `until` a `Ticked(Acted { task_id: CTV-1, .. })`; CTV-1 is `verifying`.
  (b) `until` `Started` of `arch`, CTV-1, `verify`, then `until` `Started` of `dev-a`, CTV-2,
  `implement`: `running()` is `["arch", "dev-a"]`; the last of `starts()` has `alongside`
  `["arch"]`; `activity(..)` at `at()` gives `dev-a` `Working` with `task_id` CTV-2 and the line
  `Building <harness.row("CTV-2").title>`, and `arch` `Working` with `task_id` CTV-1 and the line
  `Reviewing <harness.row("CTV-1").title>`.
  (c) `release("arch")`; `until` a `Ticked(Acted { task_id: CTV-1, what })` whose `what` starts
  `rejected it for C1`; CTV-1 is `rejected`, `running()` is `["dev-a"]`, `starts()` has 3 entries.
  (d) `release("dev-a")`; `until` `Started` of `dev-a`, CTV-1, `implement`: what it gathered holds a
  `Started` of `arch`, CTV-2, `verify`; `running()` is `["arch", "dev-a"]`; the last start's
  `alongside` is `["arch"]`; CTV-1's row has `iteration` 1.
- `runs_one_session_per_agent_at_a_time` (`orchestrator.rs` `mod tests`, ignored as above; watched
  to fail before `unavailable` asks `Work`: the second `next` answers `Err` with `agent_busy`, the
  claim refusing `dev-a`'s CTV-2) — the
  `Harness` team with `wip_limit_per_agent` 2; `in_progress("CTV-1", "dev-a", "dev-b")`,
  `in_progress("CTV-2", "dev-a", "dev-b")`, `in_progress("CTV-3", "dev-b", "dev-a")`;
  `HeldAdapter` over `harness.recorded(vec![reads_a_file()])`. The first
  `next_progress` is `Started` of `dev-a`, CTV-1; the second is `Started` of `dev-b`, CTV-3;
  `release("dev-a")`, `until` `Ticked(Acted { task_id: CTV-1, .. })`, then `until` the next
  `Started` of `dev-a`. Over `starts()`, no start's `alongside` holds its own agent; no start is
  `dev-a`'s about CTV-2 before the third; `harness.events(&[EventKind::SessionStarted])` count 3.

- [x] `feat(runtime): each agent's session runs beside the others', one per agent`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
