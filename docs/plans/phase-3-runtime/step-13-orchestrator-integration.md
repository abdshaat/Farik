# Phase 3, step 13: Orchestrator, integration and recovery

Status: draft
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.7, 5.14, 5.15, 8.4, F6
Depends on: step 12 of this phase (`Orchestrator` with its verifying rules, `crates/runtime/tests/one_task.rs`), a start gate: Task 1 does not begin until step 12's last commit is on this branch; step 02 (`SandboxFactory`); phase 2 (`Git::merge`, on main)
Readiness confirmed by: pending

## Goal

Accepted work leaves the way the team's policy says: under `manual` (the default) the board shows the task as awaiting integration until the human merges it through `farik integrate`; under `local_merge` Farik merges it, one task at a time even across processes, and a merge that cannot land is escalated to the human with the task still `accepted`. A finished task's worktree and container are removed and its branch kept. A dependency counts as integrated once it is. And a run that was killed is picked up where it stopped: interrupted sessions are ended, finished tasks cleaned up, and tasks in progress resumed. Out of scope: `pull_request` (phase 6, which has a forge); the `farik integrate` command itself (step 15, which calls `integrate`); detecting a branch the human merged with git alone.

## Decisions

- `task.integrated { sha, into, integrated_by }` (8.5 already names the kind): `into` the integration branch, `integrated_by` `governor` for the tick's merge and `human` for `integrate`, the attribution. About one contract.
- `awaiting_integration` on the board (C3, the simpler derivation): true on every `task.transitioned` into `accepted`, false on `task.integrated`; the policy is not on the event, and the rules filter by the policy in force. Migration `crates/store/src/migrations/0005_integration.sql` adds the column (`INTEGER NOT NULL DEFAULT 0`) and sets it to 1 for rows already `accepted`, so an older accepted task reads as awaiting rather than as integrated; `migrations.rs` lists it. Rejected: stamping the policy on `task.transitioned`, a second field that says what the team file already says.
- Integration (rule 2 of step 11's order): only under `local_merge`, for an `accepted` task that is `awaiting_integration` and has no `escalation.raised { reason: integration }` since its last move into `accepted`. `Git::merge(<integration branch>, farik/<id>, "Merge <id>: <title>")` from the branch, which outlives the worktree. `Merged { sha }` appends `task.integrated`. Anything else, conflicts or a git error such as a dirty checkout, appends `escalation.raised { reason: integration, detail }` with the conflicting paths or git's words, and nothing more: no transition, because `accepted` is terminal and no row leaves it (C2); the task stays `accepted` and `awaiting_integration`; the rule does not retry it; the human resolves it and runs `farik integrate`. Kept to that minimum on the founder's word (2026-09-22). 5.7 and 5.14 say that an integration escalation is a message without a state.
- `integrate(task_id)`, the human's: refused (`OrchestratorError::Refused`) for a task not `accepted`; under `pull_request`, `NotInThisPhase`, as the tick passes over those tasks and leaves them awaiting as under `manual`; otherwise the same merge with `integrated_by: human`, whatever the escalations, because the human asking is the retry. A branch the human already merged by hand gives `git merge` "already up to date", which is `Merged` at the head, so `farik integrate` records a hand merge too; detecting one without it is the known ceiling (upgrade: an ancestor check each tick).
- One task integrates at a time (5.14): every merge holds an exclusive `std::fs::File::lock` on `.farik/local/integration.lock`, taken and the merge run under `spawn_blocking`. Chose a file lock over the in-process `tokio::sync::Mutex` of the draft, because `farik integrate` (step 15) runs in its own process beside `farik run`, and the lock must hold across the two; `flock` locks conflict between two opens in one process as well, so the one lock serves both. No dependency: `File::lock` is in the pinned toolchain (1.98.1).
- Cleanup (rule 1, I9): an `accepted` or `cancelled` task whose worktree directory exists has its sandbox dropped from step 11's map, its container removed by the new `SandboxFactory::remove(project_id, task_id)` (Docker `rm -f farik-<project>-<task>`, a container already gone counting as removed as `discard` counts it; the host's a no-op), then the worktree removed with `Git::remove_worktree`; the branch is kept (5.14). Container first, so a run killed in between leaves the worktree, which is the signal for the rule to run again. Chose removing by name through the factory over `Sandbox::discard` on a handle (either `Arc::try_unwrap`, which fails while the daemon's registration or an in-flight tool call still holds a clone, or `discard(&self)`): after a restart there is no handle to a leftover container at all, and one method covers the live case and the recovery case. A call still in flight after the removal gets `ContainerGone`.
- A dependency is integrated (I11): `Transitions::context` reads `DependencyState::integrated` as the dependency's board row being `accepted` and not `awaiting_integration`, which step 04 read as false until an event said otherwise.
- Recovery (5.15), `recover()`, which `farik run` (step 15) calls before the first tick: every `session.started` with no `session.ended` gets `session.ended { reason: aborted, detail: "interrupted: farik stopped before the session ended" }` through `record_session_ended`. Chose `aborted` with that detail over a new `interrupted` wire value, which nothing reads differently; 5.15 is amended to say so. Then the cleanup above for every finished task with a worktree. `in_progress` tasks are left for step 11's rule 6, whose resume message already names the last commit and note, and whose lazy `SandboxFactory::create` replaces a container left from the killed run (step 02's `create` removes one of that name first). A `verifying` task whose reviewer session was interrupted gets a new reviewer session without its criteria being run again (step 12). `RecoveryReport { sessions_interrupted, worktrees_removed, tasks_resumed }`, `tasks_resumed` being the number of tasks `in_progress` when recovery ran.
- `OrchestratorError` gains `Refused { reason: String }`, `NotInThisPhase { what: String }`, and `Lock { detail: String }`.

## File map

```
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: task.integrated
crates/store/src/migrations/0005_integration.sql     creates: awaiting_integration
crates/store/src/migrations.rs                       modifies: lists 0005
crates/store/src/projections.rs                      modifies: TaskProjection::awaiting_integration, fed by the two events; tests
crates/runtime/src/transitions.rs                    modifies: a dependency's `integrated`; test
crates/runtime/src/sandbox.rs, sandbox/docker.rs, sandbox/host.rs   modifies: SandboxFactory::remove; tests
crates/runtime/src/orchestrator/integrate.rs         creates: the merge under the lock, integrate, cleanup
crates/runtime/src/orchestrator/recover.rs           creates: recover, RecoveryReport
crates/runtime/src/orchestrator/rules.rs             modifies: rules 1 and 2; tests
crates/runtime/src/orchestrator.rs                   modifies: the error variants, IntegrationOutcome
crates/runtime/tests/one_task.rs                     modifies: after acceptance
docs/SPEC.md                                         modifies: 5.7 and 5.14 (an accepted task's integration escalation keeps its status; pull_request waits for phase 6; the file lock), 5.15 (interrupted is `aborted` with its detail; what recovery does)
docs/plans/project-plan.md                           modifies: step 13's interface line
```

## Interfaces

Consumes: `Orchestrator`, the sandbox map, the rules, `orchestrator::fixtures` (steps 11 and 12); `record_session_ended` (step 08); `SandboxFactory`, `DockerSandbox` (step 02); `Transitions` (step 04); `Git::merge`, `MergeOutcome`, `Git::remove_worktree` (main).

Produces:

```rust
// event body, wire
TaskIntegratedBody { sha: String, into: String, integrated_by: String }
// farik-store
TaskProjection { .., pub awaiting_integration: bool }
// farik-runtime
trait SandboxFactory { ..; fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError>; }
pub enum IntegrationOutcome { Merged { sha: String }, Escalated { detail: String } }
pub struct RecoveryReport { pub sessions_interrupted: u32, pub worktrees_removed: u32, pub tasks_resumed: u32 }
pub enum OrchestratorError { .., Refused { reason: String }, NotInThisPhase { what: String }, Lock { detail: String } }
impl Orchestrator {
    pub async fn integrate(&self, task_id: &TaskId) -> Result<IntegrationOutcome, OrchestratorError>;
    pub fn recover(&self) -> Result<RecoveryReport, OrchestratorError>;
}
```

## Tasks

### Task 1: the event and the board

Files: the schema, `event.rs`, `event/fixtures.rs`, the migration, `migrations.rs`, `projections.rs`, `transitions.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers `task.integrated`.
- `awaits_integration_from_acceptance_until_integrated` — the row is false before, true after a `task.transitioned` into `accepted`, false after `task.integrated`.
- `reads_an_older_accepted_task_as_awaiting` — a database at version 4 with an `accepted` row, migrated: the row is `awaiting_integration`.
- `reads_a_dependency_as_integrated_once_merged` — FRK-2 depends on FRK-1: with FRK-1 `accepted` and awaiting, the assignment context's dependency has `integrated: false`; after `task.integrated` for FRK-1, `true`.

- [ ] `feat(store): show accepted tasks awaiting integration`

### Task 2: cleanup

Files: `sandbox.rs`, `sandbox/docker.rs`, `sandbox/host.rs`, `integrate.rs`, `rules.rs`, `one_task.rs`

- `removes_a_container_by_name` (Docker, ignored under `--integration` as step 02's are) — after `create` for FRK-1, `remove` leaves no container of that name; a second `remove` is `Ok`.
- `cleans_up_a_task_once_it_is_accepted` — FRK-1 `accepted` with its worktree and a sandbox in the map: one tick calls the counting factory's `remove` once for FRK-1, `.farik/local/worktrees/FRK-1` is gone, `git worktree list` does not name it, `farik/FRK-1` still exists, and the map holds no FRK-1.
- `takes_one_task_from_ready_to_accepted` (extended) — after `run_until_idle`, the worktree is gone, the branch kept, and the board says `awaiting_integration` with no `task.integrated` (the default `manual`).

- [ ] `feat(runtime): remove a finished task's worktree and container`

### Task 3: integration

Files: `integrate.rs`, `rules.rs`, `orchestrator.rs`, `docs/SPEC.md` (5.7, 5.14)

- `merges_an_accepted_task_under_local_merge` — FRK-1 `accepted` under `local_merge`: one tick records `task.integrated { into: main, integrated_by: governor }` whose `sha` is the integration branch's head, which is a merge commit with `farik/FRK-1` as a parent; the root checkout is on the branch it started on.
- `leaves_an_accepted_task_to_the_human_under_manual` — the same under `manual`: the tick is `Idle` and nothing is merged.
- `escalates_a_conflict_and_leaves_the_task_accepted` — `main` and `farik/FRK-1` changing one line differently: `escalation.raised { reason: integration }` whose detail names the file, no `task.transitioned`, the board still `accepted` and `awaiting_integration`, and the next tick is `Idle`.
- `integrates_for_the_human_after_an_escalation` — then, with the conflict resolved on `main` by hand, `integrate(FRK-1)` is `Merged` and records `integrated_by: human`.
- `refuses_to_integrate_what_is_not_accepted` and `refuses_pull_request_integration_in_this_phase` — `Refused` for a `verifying` task; `NotInThisPhase` under `pull_request`.
- `merges_one_task_at_a_time_across_orchestrators` — two `Orchestrator`s over one repository, each with an accepted task of its own under `local_merge`, `integrate` called on both concurrently, repeated twenty times on fresh repositories: every call `Merged`, every checkout left on its starting branch, and each merge commit on `main`.

- [ ] `feat(runtime): integrate accepted work one task at a time`

### Task 4: recovery

Files: `recover.rs`, `orchestrator.rs`, `docs/SPEC.md` (5.15), `docs/plans/project-plan.md`

- `recovers_an_interrupted_run` — a log with one `session.started` and no end, FRK-1 `accepted` with a worktree left, FRK-2 `in_progress` with its worktree: `recover` answers `sessions_interrupted: 1`, `worktrees_removed: 1`, `tasks_resumed: 1`; the log gains `session.ended { reason: aborted }` whose detail starts `interrupted`; FRK-1's worktree is gone.
- `resumes_an_in_progress_task_in_a_fresh_sandbox` — then one tick: the counting factory's `create` was called once, for FRK-2, and an implement session started whose first message starts `Resuming`.
- `recovers_nothing_twice` — `recover` a second time answers all zeros and appends nothing.

- [ ] `feat(runtime): recover an interrupted run`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
