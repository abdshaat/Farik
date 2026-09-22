# Phase 3, step 13: Orchestrator, integration and recovery

Status: draft
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.6, 5.7, 5.14, 5.15, 8.4, F6
Depends on: step 12 of this phase (`Orchestrator` with its verifying rules, `crates/runtime/tests/one_task.rs`), a start gate: Task 1 does not begin until step 12's last commit is on this branch; step 05 (`Git::push`); step 02 (`SandboxFactory`); phase 2 (`Git::merge`, on main)
Readiness confirmed by: pending

## Goal

Accepted work leaves the way the team's policy says. Under `auto_merge`, which a new project now gets, Farik merges the task's branch into the integration branch, one task at a time even across processes, and pushes it to `origin` when there is one. Under `pull_request` Farik pushes the branch, opens a pull request with `gh`, and records the task as integrated when the human merges it on the forge. Under `manual` Farik does nothing and the human runs `farik integrate`. A merge, a push, or a pull request that cannot land is escalated to the human with the task still `accepted`. A finished task's worktree and container are removed and its branch kept; a dependency counts as integrated once it is. A run that was killed is picked up where it stopped. Out of scope: the `farik integrate` command itself (step 15, which calls `integrate`); detecting a branch the human merged with git alone; forges other than the one `gh` drives.

## Decisions

- The policy (the founder, 2026-09-22, changing D19): `auto_merge` is the default for a new project, and `farik init` writes it; `pull_request` and `manual` are opt-in. The wire value `local_merge` is renamed `auto_merge` in `docs/schemas/team.schema.json` and every use (the generated `PolicyIntegration`, `crates/core/src/team/fixtures.rs`, `crates/core/src/team.rs`'s tests, SPEC 5.14), with no alias: nothing released reads `local_merge`, the repository being private until launch, so a team file holding it is refused by the schema as any unknown value is. Rejected: accepting both spellings, a second word kept for ever.
- Farik's pushes and pull requests are Farik's own action on the host, under a policy the user chose, with the user's own git and `gh` credentials. They are not an agent's use of `git_remote`: ADR 0004 and the tiers of 5.6 govern what an agent's session may do, and no session pushes here. 5.14 says so and loses "which needs `git_remote`".
- `task.integrated { sha, into, integrated_by }` (8.5 names the kind): `into` the integration branch; `integrated_by` `governor` for Farik's own merge and `human` for `integrate` and for a pull request the human merged on the forge. `pull_request.opened { url, number, branch }` is new (8.5 gains it). Both are about one contract.
- `awaiting_integration` on the board: true on every `task.transitioned` into `accepted`, false on `task.integrated`; the policy is not on the event and the rules read the policy in force. Migration `crates/store/src/migrations/0005_integration.sql` adds the column (`INTEGER NOT NULL DEFAULT 0`) and sets it to 1 for rows already `accepted`, so an older accepted task reads as awaiting rather than as integrated; `migrations.rs` lists it. Rejected: stamping the policy on `task.transitioned`, a second copy of what the team file says.
- Integration (rule 2 of step 11's order), for an `accepted` task that is `awaiting_integration` and has no `escalation.raised { reason: integration }` since its last move into `accepted`, which is how a failure is not retried (kept to that minimum on the founder's word). No transition is ever asked: `accepted` is terminal and no row leaves it (5.2), so an integration escalation is a message on a task that stays `accepted` and awaiting. That closes the phase 1 open item that `EscalationReason::Integration` was unreachable; 5.7 and 5.14 say so.
  - `auto_merge`: `Git::merge(<into>, farik/<id>, "Merge <id>: <title>")` from the branch, which outlives the worktree. `Merged { sha }` appends `task.integrated`; then, when the new `Git::has_remote("origin")` says so, `Git::push("origin", <into>)`, whose failure appends `escalation.raised { reason: integration, detail: "merged locally as <sha>; pushing <into> to origin failed: <git's words>" }` with the merge left in place, because the local integration branch is what dependents start from. Conflicts, or any git error (a dirty checkout), append `escalation.raised { reason: integration, detail }` with the conflicting paths or git's words.
  - `pull_request`, with no `pull_request.opened` since the move into `accepted`: `Git::push("origin", farik/<id>)`, then `Forge::open_pull_request(<into>, farik/<id>, "<id>: <title>", <body>)`, the body the contract's intent, the completion note, and the review note under a heading each; `pull_request.opened` on success. Every such pull request needs the human's approval and merge on the forge. Any failure (no `origin`, the push, `gh` missing or not signed in, the create) is an integration escalation with its words.
  - `pull_request`, with one opened: `Forge::pull_request_state(url)`. `Open` is not an action: the tick goes on to the next rule (`ponytail: one gh call per open pull request per tick; a slower poll when a board holds many`). `Merged { sha }`: the new `Git::fetch_fast_forward("origin", <into>)` brings the local integration branch up to the forge's, because dependents' worktrees and the governor's diffs (step 04) are taken from the local branch, and a dependent started from a base without its dependency would carry the dependency's paths in its diff; then `task.integrated { sha, integrated_by: human }`. A failed fast-forward still records `task.integrated`, the task being integrated on the forge, and adds an escalation saying the local branch could not be brought up to `<sha>`. `Closed`: an escalation with "the pull request was closed without merging". A `gh` failure: an escalation with its words.
  - `manual`: no rule.
- One task integrates at a time (5.14): every merge, push, and fast-forward holds an exclusive `std::fs::File::lock` on `.farik/local/integration.lock`, taken with the work under `spawn_blocking`. Chose a file lock over an in-process `tokio::sync::Mutex`, because `farik integrate` (step 15) runs in its own process beside `farik run`; `flock` locks conflict between two opens in one process too, so one lock serves both. `File::lock` is in the pinned toolchain (1.98.1). Under the lock the board is read again, and a task no longer `awaiting_integration` answers `Merged { sha }` with the sha of its last `task.integrated` and appends nothing, so two callers racing for one task record one integration.
- `integrate(task_id)`, the human's retry: `Refused` for a task not `accepted`; otherwise what the policy does, now, whatever the escalations: `manual` a local merge without a push, `auto_merge` the merge and the push, `pull_request` a check of the recorded pull request, or a new one when none was opened since acceptance. Each failed attempt appends its own `escalation.raised { reason: integration }`, because each is a message to the human about that attempt, and `Escalated { detail }` gives the command the same words. The other answers are `Merged { sha }`, `PullRequestOpened { url }` when a pull request was just opened, and `AwaitingForge` when the recorded one is still open. A branch the human already merged by hand gives `git merge` "already up to date", which is `Merged` at the head, so `farik integrate` records a hand merge; detecting one without it is the known ceiling (upgrade: an ancestor check each tick).
- `farik-runtime::forge` drives the `gh` program through `std::process` as the store drives git, in the repository root, with the user's environment: `open_pull_request` runs `gh pr create --base <b> --head <h> --title <t> --body <body>` and reads the URL from stdout's last line and the number from its last path segment; `pull_request_state` runs `gh pr view <url> --json state,mergeCommit` and reads `state` (`OPEN`, `MERGED`, `CLOSED`) and `mergeCommit.oid`. A program that cannot be spawned is `ForgeError::Missing`; a non-zero exit, `gh`'s own "not logged in" included, is `Failed` with its stderr; output it cannot read is `Failed` naming it. `ponytail: no timeout on gh, as there is none on git; a try_wait loop as step 02's when one hangs`. Hand-written `Display` (ADR 0006). `OrchestratorDeps` gains `forge: Arc<Forge>`. Rejected: an HTTP client for the forge's API, a dependency and a credential store for what `gh` already has.
- Cleanup (rule 1): an `accepted` or `cancelled` task whose worktree directory exists has its sandbox dropped from step 11's map, its container removed by the new `SandboxFactory::remove(project_id, task_id)` (Docker `rm -f farik-<project>-<task>`, a container already gone counting as removed as `discard` counts it; the host's a no-op), then its worktree removed with `Git::remove_worktree`, a "not a working tree" refusal ignored and the directory then deleted if it is still there (as step 06 cleans its base worktree), so a directory git no longer knows is cleaned too; the branch is kept (5.14). Container first, so a run killed in between leaves the worktree, which is the signal for the rule to run again. Chose removing by name through the factory over `Sandbox::discard` on a handle (`Arc::try_unwrap`, which fails while a registration or an in-flight call holds a clone, or `discard(&self)`): after a restart there is no handle to a leftover container at all. A call still in flight after the removal gets `ContainerGone`.
- A dependency is integrated: `Transitions::context` reads `DependencyState::integrated` as the dependency's row being `accepted` and not `awaiting_integration`, which step 04 read as false; step 11's ready rule reads it through the same context.
- Recovery (5.15), `recover()`, which `farik run` (step 15) calls before the first tick: every `session.started` with no `session.ended` gets `session.ended { reason: aborted, detail: "interrupted: farik stopped before the session ended" }` through `record_session_ended`; chose `aborted` with that detail over a new `interrupted` wire value, which nothing reads differently, and 5.15 says so. Then the cleanup above for every finished task with a worktree. `in_progress` tasks are left to step 11's rule 6, whose resume message names the last commit and note and whose lazy `create` replaces a container left from the killed run (step 02's `create` removes one of that name first); a `verifying` task gets its reviewer session again without its recorded criteria being rerun (step 12). `RecoveryReport { sessions_interrupted, worktrees_removed, tasks_resumed }`, `tasks_resumed` the number of tasks `in_progress` when recovery ran.
- `OrchestratorError` gains `Refused { reason: String }`, `Lock { detail: String }`, and `Forge(ForgeError)`.

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs, crates/core/src/team/fixtures.rs   modifies: local_merge becomes auto_merge
crates/cli/src/init.rs, crates/cli/tests/commands.rs   modifies: the starter team writes auto_merge; its test
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: task.integrated, pull_request.opened
crates/store/src/migrations/0005_integration.sql     creates: awaiting_integration
crates/store/src/migrations.rs                       modifies: lists 0005
crates/store/src/projections.rs                      modifies: TaskProjection::awaiting_integration; tests
crates/store/src/git.rs, crates/store/tests/git.rs   modifies: has_remote, fetch_fast_forward; tests
crates/runtime/src/transitions.rs                    modifies: a dependency's `integrated`; test
crates/runtime/src/sandbox.rs, sandbox/docker.rs, sandbox/host.rs   modifies: SandboxFactory::remove; tests
crates/runtime/src/forge.rs                          creates: Forge, ForgeError, PullRequest, PullRequestState; tests against a fake gh
crates/runtime/src/orchestrator/integrate.rs         creates: the lock, the three policies, integrate, cleanup
crates/runtime/src/orchestrator/recover.rs           creates: recover, RecoveryReport
crates/runtime/src/orchestrator/rules.rs             modifies: rules 1 and 2; tests
crates/runtime/src/orchestrator/fixtures.rs          modifies: CountingSandboxFactory counts remove; a fake gh; a bare origin
crates/runtime/src/orchestrator.rs, lib.rs           modifies: the error variants, IntegrationOutcome, OrchestratorDeps::forge, `pub mod forge;`
crates/runtime/tests/one_task.rs                     modifies: after acceptance
docs/SPEC.md                                         modifies: 5.14 (the three policies and auto_merge the default; Farik pushes on the user's behalf, not an agent; an integration escalation keeps the task accepted; the file lock), 5.7 (the same), 8.5 (pull_request.opened), 5.15 (interrupted is `aborted` with its detail; what recovery does)
docs/plans/project-plan.md                           modifies: step 13's interface line; the phase 1 open item on EscalationReason::Integration marked closed here
```

## Interfaces

Consumes: `Orchestrator`, `sandbox_for` and the sandbox map, the rules, `orchestrator::fixtures`, `CountingSandboxFactory` (steps 11 and 12); `record_session_ended` (step 08); `Git::push` (step 05); `SandboxFactory` (step 02); `Transitions` (step 04); `Git::merge`, `MergeOutcome`, `Git::remove_worktree`, `TempRepo` (main).

Produces:

```rust
// wire bodies
TaskIntegratedBody { sha: String, into: String, integrated_by: String }
PullRequestOpenedBody { url: String, number: u64, branch: String }
// farik-store
TaskProjection { .., pub awaiting_integration: bool }
impl Git { pub fn has_remote(&self, name: &str) -> Result<bool, GitError>; pub fn fetch_fast_forward(&self, remote: &str, branch: &str) -> Result<(), GitError>; }
// farik-runtime
trait SandboxFactory { ..; fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError>; }
pub struct Forge { pub program: PathBuf, pub root: PathBuf }
pub struct PullRequest { pub url: String, pub number: u64 }
pub enum PullRequestState { Open, Merged { sha: String }, Closed }
pub enum ForgeError { Missing { program: String }, Failed { detail: String } }
impl Forge { pub fn open_pull_request(&self, base: &str, head: &str, title: &str, body: &str) -> Result<PullRequest, ForgeError>; pub fn pull_request_state(&self, url: &str) -> Result<PullRequestState, ForgeError>; }
pub enum IntegrationOutcome { Merged { sha: String }, PullRequestOpened { url: String }, AwaitingForge, Escalated { detail: String } }
pub struct RecoveryReport { pub sessions_interrupted: u32, pub worktrees_removed: u32, pub tasks_resumed: u32 }
pub enum OrchestratorError { .., Refused { reason: String }, Lock { detail: String }, Forge(ForgeError) }
impl Orchestrator {
    pub async fn integrate(&self, task_id: &TaskId) -> Result<IntegrationOutcome, OrchestratorError>;
    pub fn recover(&self) -> Result<RecoveryReport, OrchestratorError>;
}
```

## Tasks

The harness is step 11's, its team `manual` unless a test writes another policy into the team file. An "accepted" fixture task has its worktree already removed, so rule 1 does not take the tick. A conflict is made by committing a different first line of `a.txt` on `main` after `farik/FRK-1` changed it, and resolved by a commit on `main` restoring `a.txt` to its text at the branch point. `origin`, where a test has one, is a bare repository made by `git init --bare` beside the `TempRepo`. The fake `gh` is a script the fixture writes, which appends its arguments to a file beside it and prints the answer it is given.

### Task 1: auto_merge by name and by default

Files: `team.schema.json`, `team.rs`, `team/fixtures.rs`, `init.rs`, `crates/cli/tests/commands.rs`

- `reads_a_team_with_every_field_it_may_have` (existing, in `team.rs`) gains the assertion `policy.integration == Integration::AutoMerge` for the fixture, which now says `auto_merge`.
- `refuses_the_old_local_merge_spelling` — a team with `integration: local_merge` is refused with a failure at `/policy/integration`.
- `writes_a_starter_team_that_merges_on_its_own` (`crates/cli/tests/commands.rs`) — the team `farik init` writes reads back with `Integration::AutoMerge`.

- [ ] `feat(core): make auto_merge the integration policy a new team starts with`

### Task 2: the events and the board

Files: the event schema, `event.rs`, `event/fixtures.rs`, the migration, `migrations.rs`, `projections.rs`, `transitions.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers `task.integrated` and `pull_request.opened`.
- `awaits_integration_from_acceptance_until_integrated` — false before, true after a `task.transitioned` into `accepted`, false after `task.integrated`.
- `reads_an_older_accepted_task_as_awaiting` — a database at version 4 with an `accepted` row, migrated: the row is `awaiting_integration`.
- `reads_a_dependency_as_integrated_once_merged` — FRK-2 depends on FRK-1: with FRK-1 `accepted` and awaiting, the assignment context's dependency has `integrated: false`; after `task.integrated` for FRK-1, `true`.

- [ ] `feat(store): show accepted tasks awaiting integration`

### Task 3: cleanup

Files: `sandbox.rs`, `sandbox/docker.rs`, `sandbox/host.rs`, `integrate.rs`, `rules.rs`, `fixtures.rs`

- `removes_a_container_by_name` (Docker, ignored under `--integration` as step 02's are) — after `create` for FRK-1, `remove` leaves no container of that name; a second `remove` is `Ok`.
- `cleans_up_a_task_once_it_is_accepted` — FRK-1 `accepted` with its worktree and a sandbox in the map: one tick calls `CountingSandboxFactory::remove` once for FRK-1, `.farik/local/worktrees/FRK-1` is gone, `git worktree list` does not name it, `farik/FRK-1` still exists, and the map holds no FRK-1.
- `cleans_up_a_worktree_git_no_longer_knows` — a plain directory at that path with no registration: the tick removes it.

- [ ] `feat(runtime): remove a finished task's worktree and container`

### Task 4: the forge

Files: `forge.rs`, `lib.rs`

- `opens_a_pull_request_with_gh` — the fake `gh` answering `https://github.com/o/r/pull/7`: `PullRequest { url: that, number: 7 }`, and the recorded arguments are exactly `pr create --base main --head farik/FRK-1 --title FRK-1: Add done --body <body>`.
- `reads_a_pull_requests_state` — `{"state":"MERGED","mergeCommit":{"oid":"abc"}}` is `Merged { sha: "abc" }`; `OPEN` is `Open`; `CLOSED` is `Closed`; the arguments are `pr view <url> --json state,mergeCommit`.
- `says_when_gh_is_missing` — a program path that does not exist: `Missing` naming it.
- `reports_what_gh_said_when_it_fails` — exit 1 with stderr `not logged in`: `Failed` whose detail contains it; stdout `nonsense` from `pr view`: `Failed` naming the output.

- [ ] `feat(runtime): drive pull requests through the gh program`

### Task 5: auto_merge and manual

Files: `integrate.rs`, `rules.rs`, `orchestrator.rs`, `store/src/git.rs`, `store/tests/git.rs`, `one_task.rs`, `docs/SPEC.md` (5.7, 5.14)

- Store (ignored, needs git): `knows_whether_a_remote_exists`; `fast_forwards_a_branch_to_its_remote` — for the branch checked out and for one that is not.
- `merges_and_pushes_under_auto_merge` — FRK-1 `accepted` under `auto_merge` with `origin`: one tick records `task.integrated { into: main, integrated_by: governor }` whose `sha` is `main`'s head, a merge commit with `farik/FRK-1` as a parent; `origin`'s `main` is that sha; the root checkout is on its starting branch.
- `merges_without_a_push_when_there_is_no_origin` — the same without a remote: `task.integrated`, no escalation.
- `escalates_a_failed_push_and_keeps_the_merge` — `origin` pointing at a path that does not exist: `task.integrated`, then `escalation.raised { reason: integration }` whose detail starts `merged locally as`; the next tick is `Idle`.
- `escalates_a_conflict_and_leaves_the_task_accepted` — the conflict above: an escalation whose detail names `a.txt`, no `task.transitioned`, the board `accepted` and awaiting, and the next tick `Idle`.
- `integrates_for_the_human_after_an_escalation` — then `integrate(FRK-1)` without the resolution is `Escalated` and appends a second escalation; after the resolution it is `Merged` with `integrated_by: human`.
- `answers_an_integrated_task_without_merging_again` — `integrate` on FRK-1 once integrated: `Merged` with the same sha and no second `task.integrated`.
- `leaves_an_accepted_task_to_the_human_under_manual` — `manual`: the tick is `Idle` and nothing merged; `integrate` then merges and does not push.
- `refuses_to_integrate_what_is_not_accepted` — a `verifying` task: `Refused`.
- `merges_one_task_at_a_time_across_orchestrators` — two `Orchestrator`s over one repository, each integrating its own accepted task concurrently, twenty times on fresh repositories: every call `Merged`, every checkout left on its starting branch, both merge commits on `main`.
- `takes_one_task_from_ready_to_accepted` (extended; the team `auto_merge`, no remote) — after `run_until_idle`: `task.integrated` for FRK-1, the worktree gone, `farik/FRK-1` kept.

- [ ] `feat(runtime): merge accepted work one task at a time`

### Task 6: pull requests

Files: `integrate.rs`, `rules.rs`, `fixtures.rs`

- `opens_a_pull_request_for_an_accepted_task` — `pull_request`, `origin`, the fake `gh` answering a URL ending `/pull/7`: `origin` has `farik/FRK-1`; `pull_request.opened { number: 7, branch: farik/FRK-1 }`; the body `gh` got holds the intent, the completion note, and the review note.
- `waits_while_the_pull_request_is_open` — `gh` answering `OPEN`: the tick is `Idle` and appends nothing.
- `records_a_pull_request_merged_on_the_forge` — the test pushes a merge of `farik/FRK-1` to `origin`'s `main` from a second clone and `gh` answers `MERGED` with that commit: `task.integrated { sha: it, integrated_by: human }`, and the local `main` is at it.
- `escalates_a_closed_pull_request_once` — `CLOSED`: an escalation with "the pull request was closed without merging"; the next tick is `Idle` and `gh` was not called again.
- `escalates_when_gh_is_missing` — no `gh` program: an escalation naming it and no `pull_request.opened`.

- [ ] `feat(runtime): integrate through pull requests the human merges`

### Task 7: recovery

Files: `recover.rs`, `orchestrator.rs`, `docs/SPEC.md` (5.15, 8.5), `docs/plans/project-plan.md`

- `recovers_an_interrupted_run` — a log with one `session.started` and no end, FRK-1 `accepted` with a worktree left, FRK-2 `in_progress` with its worktree and a seeded commit on `farik/FRK-2`: `recover` answers `sessions_interrupted: 1`, `worktrees_removed: 1`, `tasks_resumed: 1`; the log gains `session.ended { reason: aborted }` whose detail starts `interrupted`; FRK-1's worktree is gone.
- `resumes_an_in_progress_task_in_a_fresh_sandbox` — then one tick: `CountingSandboxFactory` saw one `create`, for FRK-2, and the implement session's first message contains `Resuming: last commit <sha>` with the seeded commit's sha.
- `recovers_nothing_twice` — `recover` again answers all zeros and appends nothing.

- [ ] `feat(runtime): recover an interrupted run`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
