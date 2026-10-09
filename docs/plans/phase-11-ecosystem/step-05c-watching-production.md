# Phase 11, step 05c: Watching production

Status: draft. Its readiness review runs once step 05b has landed.
Branch: `phase/11-ecosystem` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.9, 8.1, 8.5; F9
Depends on: step 05b (`Platform`, `PlatformSource`, `Team::production`, the three `deployment.*` kinds, `DeployWork`); phase 6 (merged in #19)
Readiness confirmed by: not yet run
Moved 2026-10-09 by ADR 0049 (project plan revision 41; the founder: "DevOps later, rest after Cloud"): phase 7 step 11c until then (its file was `step-11c-watching-production.md` in phase 7's folder). The DevOps Engineer is built in the Ecosystem phase, phase 11, after its own steps 01 to 04: phase 7's steps 11 to 11f are steps 05 to 05f here, and 12 to 12e are 06 to 06e. The text below names them by their new numbers, and phase 7's other steps as phase 7's; the dated lines above, and the founder's words, keep the numbers of their day. Phase 7 step 10h, ask or auto, is phase 9 step 01; step 13, the kit check, is phase 9 step 02 and has no DevOps task, so this phase checks the DevOps Engineer's kit itself; the phases after phase 8 moved up by one.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 05 (see step 05's header); ADR 0045, 4 and 5, are this step's.

## Goal

While `farik serve` or `farik run` drives a project whose team has production settings and an active DevOps Engineer, Farik checks production once a minute, with no model and no session: the health address answers `2xx`, the platform reports a deployment live, and, where the platform offers one and the team set a threshold, the error rate is under it. It records `health.changed` only when the answer changes. It follows each deploy `farik_deploy` started: a deploy that goes live and stays healthy for the settling period is recorded `deployment.succeeded` and its deploy task moves to `verifying`; one the platform fails, one that goes unhealthy while settling, and one still building after 30 minutes are recorded `deployment.failed`. The time of the last check is kept, so the card of step 05f can show a stopped watch. Out of scope: incidents (05d); the pages (05f).

## Decisions

- **Its own task, not a rule** (ADR 0045, 4). A tick runs one session to its end (`orchestrator.rs:417`, `rules.rs:68`), so a rule would not run while a session does. `run_watch` is a task `start` (`crates/cli/src/start.rs`) spawns beside the tick loop for every driving process, `farik serve` and `farik run`, and aborts when the driver finishes. It sleeps with the injected `Sleeper` until the next whole minute after the previous tick began, so a slow check does not drift the cadence, and ticks once at start. It runs while the team is paused, since it starts nothing; it appends events through the same log and projections as every other writer, and notifies the daemon's `wakes()` after anything it records, so an idle tick loop looks again.
- **When it ticks.** Only when `team.production()` is set, an active DevOps Engineer has the named connector in its `mcp_servers`, and the daemon's `platforms()` (step 05b) answers a platform. Otherwise the tick records nothing and `WatchStatus` says why in a sentence: "No production settings yet", "No active DevOps Engineer has <connector> connected", or the platform's error ("Farik cannot drive <connector> yet").
- **The health address** is read by `HealthProbe`; the shipped `HttpProbe` sends one `GET` with `reqwest` (already a dependency), follows no redirect, gives up after 10 seconds, reads at most 64 KiB of the body and drops it, sends `User-Agent: farik-watch/<version>`, and uses the environment's proxy settings, since the address is the user's own service. `2xx` is healthy; anything else, a timeout or a refused connection is not, with the status or the error's kind as the detail. The body is never kept, logged or shown.
- **A check** is three answers: the probe, `platform.live()` (a deployment `Live`), and `platform.error_rate(since the previous tick)` when the team set `error_rate_percent`. `judge` is healthy when the probe passed, a deployment is live, and the rate, when there is one, is at most the threshold; else unhealthy with the first reason in that order (`health_url`, `platform`, `error_rate`). When the platform cannot be read (`NotConnected`, `Failed`), the check is judged on the probe alone and `WatchStatus` says the platform did not answer.
- **Two in a row** (ADR 0045, 4). Healthy to unhealthy needs two consecutive unhealthy checks; unhealthy to healthy needs one healthy check. `health.changed { healthy, why?, detail?, live? }` is recorded on a change, and on the first check after Farik starts when the log holds no `health.changed` yet, which makes the first live deployment the first one Farik recorded as healthy (`live` is `{ deployment_id, version }` of the deployment that serves production when healthy). The carried state starts from the newest `health.changed` in the log, so a restart of Farik records no change of its own.
- **Following a deploy.** Each tick reads `platform.deployments()` once and, for every `deployment.started` with no outcome yet, finds its `deployment_id`: `Failed` records `deployment.failed { why: platform, detail }` (the platform's words, at most 500 characters); `Building` 30 minutes after `deployment.started` records `deployment.failed { why: timed_out }`; `Live` starts its settling at the first tick that saw it live. While settling, the service turning unhealthy by the rule above records `deployment.failed { why: unhealthy }`; healthy on every tick for `settling_minutes` records `deployment.succeeded { healthy_minutes }`. A deployment the platform no longer lists counts as `Failed` with "the platform no longer lists it". Settling lives in memory: a restart of Farik starts a deploy's settling again from the next tick that sees it live, which waits longer, never shorter.
- **Moving the deploy task.** After `deployment.succeeded`, the watch asks the governor for `in_progress -> verifying` for the deploy task on its envelope, as its assignee (`TransitionActor::Assignee`, `filed_by_farik: true`, the way rule 7 asks `assigned -> in_progress`); a refusal is recorded and left on the board, as 5.2 says for such moves. After `deployment.failed` the task stays `in_progress`, and rule 6's next session for it may deploy again, which asks the human (step 05b); step 05d replaces this with an incident.
- **The status.** `DaemonState` gains `watch_status: Mutex<WatchStatus>`, set at every tick; the query `production.status {}` answers it with what the log adds: `{ watching, why?, last_check_at?, healthy?, live?, deploying? }`, `deploying` being `{ started, commit, holds, live_since?, healthy_minutes }` for a deploy being followed. Nothing is recorded per tick (spec 6.9: a change of state, not every tick).
- **Pure where it can be.** The judging, the two-in-a-row rule and the settling are `farik_core::production`, given times and answers and doing no I/O; the watch in `farik-runtime` gathers the answers and records.

## File map

```
crates/core/src/production.rs, crates/core/src/lib.rs        creates: Check, judge, next_health, settle (Task 1)
docs/schemas/event.schema.json, crates/protocol/src/event.rs modifies: health.changed (Task 2)
crates/runtime/src/watch.rs, crates/runtime/src/lib.rs       creates: HealthProbe, HttpProbe, WatchStatus, watch_tick, run_watch (Tasks 2 to 4)
crates/runtime/src/daemon.rs                                 modifies: DaemonState.watch_status (Task 4)
crates/runtime/src/daemon/board.rs, docs/schemas/rpc.schema.json   modifies: production.status (Task 5)
crates/cli/src/start.rs, crates/cli/src/run.rs               modifies: spawn and stop the watch (Task 4)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 6)
```

## Interfaces

Consumes: `Platform`, `PlatformSource`, `DaemonState::platforms`, `Deployment`, `DeploymentState`, `Team::production`, `deployment.started`, `deployment.succeeded`, `deployment.failed` (step 05b); `Sleeper`, `Clock`, `Transitions::request`, `TransitionAsk`, `DaemonState::wakes` (runtime, on main).

Produces:

```rust
pub struct Check { pub probe_ok: bool, pub live: bool, pub error_rate: Option<f64> }          // farik_core::production
pub enum Unhealthy { HealthUrl, Platform, ErrorRate }
pub fn judge(check: &Check, threshold: Option<f64>) -> Result<(), Unhealthy>;
pub struct Health { pub healthy: Option<bool>, pub failing: u8 }
pub fn next_health(previous: Health, judged: Result<(), Unhealthy>) -> (Health, Option<bool>);  // the change to record
pub enum Rollout { Building, Live, Failed }
pub enum Settled { Pending, Succeeded { minutes: u16 }, Failed(FailedWhy) }
pub enum FailedWhy { Platform, Unhealthy, TimedOut }
pub fn settle(rollout: Rollout, started_at: DateTime<Utc>, live_since: Option<DateTime<Utc>>,
    unhealthy_since_live: bool, now: DateTime<Utc>, settling_minutes: u16) -> Settled;
pub struct Probe { pub ok: bool, pub status: Option<u16>, pub detail: String }                // farik_runtime::watch
pub trait HealthProbe: Send + Sync { fn probe<'a>(&'a self, url: &'a str) -> Pin<Box<dyn Future<Output = Probe> + Send + 'a>>; }
pub struct HttpProbe;
pub struct WatchStatus { pub watching: bool, pub why: Option<String>, pub last_check_at: Option<DateTime<Utc>>, pub healthy: Option<bool> }
pub struct WatchMemory { /* health carried between ticks, and each followed deploy's live_since and whether it went unhealthy */ }
pub async fn watch_tick(deps: &ToolDeps, platforms: &PlatformSource, probe: &dyn HealthProbe, memory: &mut WatchMemory) -> Result<WatchStatus, WatchError>;
pub async fn run_watch(deps: Arc<ToolDeps>, probe: Arc<dyn HealthProbe>, sleeper: Arc<dyn Sleeper>, daemon: Arc<DaemonState>);
pub enum WatchError { Store(StoreError), Files(FilesError), Transition(TransitionError) }
```

## Tasks

### Task 1: Judging and settling (core)

- `healthy_needs_all_three`: the probe failing, nothing live, and a rate of 3.1 over a threshold of 2.0 each give their reason; a rate with no threshold is ignored. RED.
- `unhealthy_takes_two_checks_in_a_row`: healthy, one failing check (no change), a second (the change to unhealthy); one healthy check changes it back. RED.
- `the_first_check_is_a_change`: from `healthy: None`, a healthy check is a change to healthy, and one failing check is not yet a change. RED.
- `a_deploy_succeeds_after_its_settling`: live at 10:00, settling 5, at 10:04 `Pending`, at 10:05 `Succeeded { minutes: 5 }`. RED.
- `a_deploy_fails_three_ways`: `Rollout::Failed`; still building 30 minutes after it started; unhealthy since it went live. RED.

- [ ] `feat(core): judge production health and a deploy's settling`

### Task 2: The probe and `health.changed`

- `the_probe_reads_only_the_status`: a local server answering 204 is ok; 503 is not, with "503"; a 302 is not followed and is not ok; a 10 MiB body is cut at 64 KiB and the probe still answers. RED.
- `the_probe_gives_up_after_ten_seconds` (the clock-free way: a server that never answers, with the timeout passed in for the test). RED.
- `reads_and_writes_health_changed` (protocol): round trip; `why` outside the three refused. RED.

- [ ] `feat(runtime): check the production health address`

### Task 3: The tick

Files: `watch.rs`; tests with `FakePlatform`, a fake probe and a fixed clock.

- `a_healthy_tick_records_nothing_and_starts_no_session`: after the first change is recorded, ten healthy ticks add no event and no `session.started`. RED.
- `two_failing_ticks_record_one_change`: `health.changed { healthy: false, why: health_url, detail: "503" }` once. RED.
- `the_first_tick_records_the_live_deployment`: `health.changed { healthy: true, live: { deployment_id, version } }`. RED.
- `a_restart_of_farik_records_no_change`: with `health.changed { healthy: true }` in the log, a fresh `WatchMemory` and a healthy tick record nothing. RED.
- `follows_a_deploy_to_success_and_moves_its_task`: started, then live, then five healthy minutes: one `deployment.succeeded { healthy_minutes: 5 }`, the deploy task `verifying`, the move filed by Farik. RED.
- `records_each_failure`: the platform's `Failed`, a 30-minute build, and two unhealthy checks while settling each record `deployment.failed` with its `why`, and the task stays `in_progress`. RED.
- `an_unreadable_platform_judges_by_the_address`: `Failed` from the platform and a healthy probe record nothing and say so in the status. RED.
- `does_not_tick_without_settings_or_an_agent`: no `production`, or the DevOps Engineer paused: no event, and the status says why. RED.

- [ ] `feat(runtime): watch production once a minute`

### Task 4: Running beside the ticks

- `run_watch_ticks_each_minute_until_stopped`: with a test `Sleeper`, three minutes give three ticks at the whole minutes; stopping the driver ends it. RED.
- `a_recorded_change_wakes_the_tick_loop`: `wakes()` is notified after `deployment.succeeded`. RED.
- `start_spawns_the_watch_for_serve_and_run` (`crates/cli`): the driver holds its handle, and `finish` aborts it. RED.

- [ ] `feat(cli): keep the watch running while Farik drives the project`

### Task 5: `production.status`

- `production_status_says_what_the_watch_saw`: `watching`, `last_check_at`, `healthy`, `live`, and `deploying` with `live_since` and `healthy_minutes` while a deploy settles; `why` when not watching. RED.

- [ ] `feat(runtime): answer what production looks like`

### Task 6: Spec and plan

`docs/SPEC.md` 6.9's Watching as built (the cadence, the three answers, two in a row, the address's limits, following a deploy, the 30 minutes), 8.1 (the watch beside the tick loop), 8.5 (`health.changed`); the revision line. Project plan row 05c.

- [ ] `docs(spec): record the production watch`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

No live check: no platform is driven before step 06; step 06's live run watches a real one.

## Execution notes

None yet.
