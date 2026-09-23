# Phase 3, step 13: Orchestrator, integration and recovery

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.6, 5.7, 5.14, 5.15, 8.4, F6
Depends on: step 12 of this phase (`Orchestrator` with its verifying rules, `crates/runtime/tests/one_task.rs`), a start gate: Task 1 does not begin until step 12's last commit is on this branch; step 08 (`record_session_ended`, and `session.started`'s purpose and model); step 06 (the `-base` container and worktree it names); step 05 (`Git::push`, `Projections::catch_up`); step 04 (`Transitions`, `Transitions::context`); step 03 (`record_session_cost`); step 02 (`SandboxFactory`); phase 2 (`Git::merge`, `Git::commit_count`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (step 12: two rounds; step 13: one round after its rewrite; findings folded in)

## Goal

Accepted work leaves the way the team's policy says. Under `auto_merge`, which a new project now gets, Farik merges the task's branch into the integration branch, one task at a time even across processes, and pushes it to `origin` when there is one. Under `pull_request` Farik pushes the branch, opens a pull request with `gh`, and records the task as integrated when the human merges it on the forge. Under `manual` Farik does nothing and the human runs `farik integrate`. A merge, a push, or a pull request that cannot land is escalated to the human with the task still `accepted`. A finished task's worktrees and containers are removed and its branch kept; a dependency counts as integrated once it is. A run that was killed is picked up where it stopped. Out of scope: the human's `TaskIntegrate` command (step 14) and `farik integrate` (step 15), both of which call `integrate`; detecting a branch the human merged with git alone; forges other than the one `gh` drives.

## Decisions

- The policy and who pushes: ADR 0012 (the founder, 2026-09-22, reversing D19). The wire value `local_merge` is renamed `auto_merge` in `docs/schemas/team.schema.json` and every use (the generated `PolicyIntegration`, `crates/core/src/team/fixtures.rs`, `crates/core/src/team.rs`'s tests, SPEC 5.14), with no alias: nothing released reads `local_merge`, the repository being private until launch, so a team file holding it is refused by the schema as any unknown value is. Rejected: accepting both spellings, a second word kept for ever. `farik init` writes `auto_merge`, and when it writes the starter team its report says so (5.6): "Integration: auto_merge. Accepted work is merged into the integration branch and pushed to origin when there is one; policy.integration in .farik/team.yaml changes it." 5.14 loses "which needs `git_remote`".
- `task.integrated { sha, into, integrated_by }` (8.5 names the kind): `into` the integration branch; `integrated_by` a schema enum `[governor, human]`, `governor` for Farik's own merge and `human` for `integrate` and for a pull request the human merged on the forge. `pull_request.opened { url, number, branch }` is new (8.5 gains it). Both are about one contract. `escalation.raised`'s schema description, which says a move sent the contract to the human, gains the integration escalation, which moves nothing.
- `awaiting_integration` on the board: true on a `task.transitioned` into `accepted` for a row whose `kind` is `task`, false on `task.integrated`; the policy is not on the event and the rules read the policy in force. An epic has no branch of its own (its children have them), so an accepted epic is never awaiting and reads as integrated. Migration `crates/store/src/migrations/0005_integration.sql` adds the column (`INTEGER NOT NULL DEFAULT 0`) and sets it to 1 for rows `accepted` with kind `task`, so an older accepted task reads as awaiting rather than as integrated; `migrations.rs` lists it. Its test builds a version-4 database with `migrations::apply_through(connection, 4, now)`, which `apply` becomes a call of with the last version. Rejected: stamping the policy on `task.transitioned`, a second copy of what the team file says.
- Integration (rule 2 of step 11's order), for an `accepted` task that is `awaiting_integration` and has no `escalation.raised { reason: integration }` since its last move into `accepted`, which is how a failure is not retried (kept to that minimum on the founder's word). After an integration escalation the tick neither retries nor polls the pull request again; the human's `integrate` is the way on, and it also records a merge made on the forge meanwhile (5.14 says so). No transition is ever asked: `accepted` is terminal and no row leaves it (5.2), so an integration escalation is a message on a task that stays `accepted` and awaiting. That closes the phase 1 open item that `EscalationReason::Integration` was unreachable; 5.7 and 5.14 say so.
  - `auto_merge`: `Git::merge(<into>, farik/<id>, "Merge <id>: <title>")` from the branch, which outlives the worktree. `Merged { sha }` appends `task.integrated`; then, when the new `Git::has_remote("origin")` says so, `Git::push("origin", "refs/heads/<into>")`, whose failure appends `escalation.raised { reason: integration, detail: "merged locally as <sha>; pushing <into> to origin failed: <git's words>; run git push origin <into> once it can be pushed" }` with the merge left in place, because the local integration branch is what dependents start from. The push is never retried: the task is no longer awaiting, and `integrate` on it answers `Merged` (below). Conflicts, or any git error (a dirty checkout), append `escalation.raised { reason: integration, detail }` with the conflicting paths or git's words.
  - `pull_request`, with no `pull_request.opened` since the move into `accepted`: no `origin` is an escalation saying so; otherwise `Git::push("origin", "refs/heads/farik/<id>")`, then `Forge::open_pull_request(<into>, farik/<id>, "<id>: <title>", <body>)`, then `pull_request.opened`. The body is three headings, `## Intent`, `## Completion note`, `## Review note`, each followed by its text. Every such pull request needs the human's approval and merge on the forge. Any failure (the push, `gh` missing or not signed in, the create) is an integration escalation with its words. `gh` picks the base repository itself, which in a fork with an `upstream` remote can be `upstream`; `gh repo set-default` settles it (one line in 5.14).
  - `pull_request`, with one opened: `Forge::pull_request_state(url)`. `Open` is not an action: the tick goes on to the next rule (`ponytail: one gh call per open pull request per tick; a slower poll when a board holds many`). `Merged { sha }`: the new `Git::fetch_fast_forward("origin", <into>)` brings the local integration branch up to the forge's, because dependents' worktrees and the governor's diffs (step 04) are taken from the local branch, and a dependent started from a base without its dependency would carry the dependency's paths in its diff; then `task.integrated { sha, integrated_by: human }`. A failed fast-forward still records `task.integrated`, the task being integrated on the forge, and adds an escalation saying the local branch could not be brought up to `<sha>`. `Closed`: an escalation with "the pull request <url> was closed without merging". A `gh` failure: an escalation with its words.
  - `manual`: no rule.
- Branch names reach `push` as `refs/heads/<name>`, and `fetch_fast_forward` spells `refs/heads/<branch>` in its own refspec, so a name that is also a tag is not taken for one. The team's `policy.integration_branch` is checked when `integration_branch` (`transitions.rs`) reads it, by the new `Git::check_branch_name` (`git check-ref-format --branch <name>`), whose refusal is `CommandFailed` naming it, so `main:other` or `-f` never reaches a refspec or an option. A name read from git (the default branch) is not checked. Rejected: a pattern in the schema; git's rules for a ref name are not one regular expression, and git is the authority on them.
- No git Farik runs may prompt: `run_git_untrimmed` sets `GIT_TERMINAL_PROMPT=0`, so a push or fetch to an HTTPS `origin` with no credential helper fails with git's words instead of waiting on a terminal nobody watches; `farik_git_push` gets it too. Chose the shared runner over `push` and `fetch_fast_forward` alone, because no git call of Farik's has a person at a terminal to answer it.
- One task integrates at a time (5.14): every merge, push, fast-forward, and pull request opening holds an exclusive `std::fs::File::lock` on `.farik/local/integration.lock`, taken with the work under `spawn_blocking`; for a pull request the push, the create, and the append of `pull_request.opened` are all under it. Chose a file lock over an in-process `tokio::sync::Mutex`, because `farik integrate` (step 15) runs in its own process beside `farik run`; `flock` locks conflict between two opens in one process too, so one lock serves both. `File::lock` is in the pinned toolchain (1.98.1). Under the lock `Projections::catch_up` runs first, so that another process's `task.integrated` or `pull_request.opened` is on this process's board, and then the board and the log are read again: a task no longer `awaiting_integration` answers `Merged { sha }` with the sha of its last `task.integrated` and appends nothing, and a task with a `pull_request.opened` since acceptance is not given a second one, so two callers racing for one task record one integration and one pull request.
- `integrate(task_id)`, the human's retry: `Refused` for a task not `accepted`; `Merged { sha }` of its last `task.integrated` for one no longer awaiting, with nothing pushed; otherwise what the policy does, now, whatever the escalations: `manual` a local merge without a push; `auto_merge` the merge and the push, a failed push answering `Escalated` with the `merged locally as` detail; `pull_request` a new pull request when none was opened since acceptance, else the recorded one's state: `Open` is `AwaitingForge`, `Merged` is as the rule, and `Closed` fetches (`fetch_fast_forward("origin", <into>)`) and then asks whether `farik/<id>` is in `<into>` by `commit_count(<into>, farik/<id>) == 0`: yes (merged by hand, or through another pull request) records `task.integrated { sha: <into>'s head, integrated_by: human }` and answers `Merged`; no is an escalation "the pull request <url> was closed without merging, and farik/<id> is not in <into>: reopen it on the forge or merge the branch, then run farik integrate". Chose that over opening a new pull request: after a hand merge `gh pr create` fails with no commits between the branches, a dead end, and a human who wants the pull request back reopens it, which `integrate` then reads. Chose `commit_count` (main) over a new `Git::is_ancestor`: no commit of the branch missing from `<into>` is the same question. Each failed attempt appends its own `escalation.raised { reason: integration }`, because each is a message to the human about that attempt, and `Escalated { detail }` gives the command the same words. The other answers are `PullRequestOpened { url }` when a pull request was just opened. A branch the human already merged by hand gives `git merge` "already up to date" under `auto_merge` or `manual`, which is `Merged` at the head, so `farik integrate` records a hand merge; detecting one without it is the known ceiling (upgrade: an ancestor check each tick). Until Task 6 lands, the tick has no rule under `pull_request` and `integrate` answers `Refused { reason: "unsupported_policy: pull_request" }`; Task 6 replaces both.
- `farik-runtime::forge` drives the `gh` program through `std::process` as the store drives git, in the repository root, with the user's environment. `open_pull_request` first runs `gh pr list --head <h> --base <b> --state open --json url,number --limit 1` and reuses a pull request it finds, so a run killed between the create and the append opens no second one; otherwise `gh pr create --base <b> --head <h> --title <t> --body-file -` with the body on standard input, reading the URL from stdout's last line and the number from its last path segment. `pull_request_state` runs `gh pr view <url> --json state,mergeCommit` and reads `state` (`OPEN`, `MERGED`, `CLOSED`) and `mergeCommit.oid`, `mergeCommit` being `null` until a merge. A program that cannot be spawned is `ForgeError::Missing`; a non-zero exit, `gh`'s own "not logged in" included, is `Failed` with its stderr; output it cannot read is `Failed` naming it. `ponytail: no timeout on gh, as there is none on git; a try_wait loop as step 02's when one hangs`. Hand-written `Display` (ADR 0006). `OrchestratorDeps` gains `forge: Arc<Forge>` (Task 6). Rejected: an HTTP client for the forge's API, a dependency and a credential store for what `gh` already has.
- Cleanup (rule 1): an `accepted` or `cancelled` task whose worktree directory `.farik/local/worktrees/<id>` or step 06's base worktree `<id>-base` exists has its sandbox dropped from step 11's map, its containers removed by the new `SandboxFactory::remove(project_id, task_id)` (Docker `rm -f` of `farik-<project>-<task>` and `farik-<project>-<task>-base`, a container already gone counting as removed as `discard` counts it; the host's a no-op), then the base worktree and then its worktree removed with `Git::remove_worktree`, a "not a working tree" refusal ignored and the directory then deleted if it is still there (as step 06 cleans its base worktree), so a directory git no longer knows is cleaned too; the branch is kept (5.14). The task's own worktree goes last, so a run killed in between leaves it, which is the signal for the rule to run again. Chose removing by name through the factory over `Sandbox::discard` on a handle (`Arc::try_unwrap`, which fails while a registration or an in-flight call holds a clone, or `discard(&self)`): after a restart there is no handle to a leftover container at all. A call still in flight after the removal gets `ContainerGone`.
- A dependency is integrated: `Transitions::context` reads `DependencyState::integrated` as the dependency's row being `accepted` and not `awaiting_integration`, which step 04 read as false; step 11's ready rule reads it through the same context.
- Recovery (5.15), `recover()`, synchronous, which `farik run` (step 15) calls under `spawn_blocking` before the first tick, because it runs git and Docker: every `session.started` with no `session.ended` gets `session.ended { reason: aborted, detail: "interrupted: farik stopped before the session ended" }` through `record_session_ended`, and, when it has no `cost.recorded`, one `record_session_cost` with `Usage::default()` and the purpose and model its `session.started` names, as step 11 does for a session that ended without usage, so that the cost projection counts it and `max_sessions` bounds it. Chose `aborted` with that detail over a new `interrupted` wire value, which nothing reads differently, and 5.15 says so. Then the cleanup above for every finished task with a worktree. `in_progress` tasks are left to step 11's rule 6, whose resume message names the last commit and note and whose lazy `create` replaces a container left from the killed run (step 02's `create` removes one of that name first); a `verifying` task gets its reviewer session again without its recorded criteria being rerun (step 12). `RecoveryReport { sessions_interrupted, worktrees_removed, tasks_resumed }`, `tasks_resumed` the number of tasks `in_progress` when recovery ran, so a second run answers it again.
- `OrchestratorError` gains `Refused { reason: String }` and `Lock { detail: String }` (Task 5) and `Forge(ForgeError)` (Task 6).
- Changed 2026-09-23 in execution (Task 2): the deferred item from step 11's landing review, the ready rule's dependency check (the `integrated` flag and the count comparison, its mutations M28 and M29), is pinned here, where `integrated` is widened: `waits_for_an_accepted_dependency_until_it_is_integrated` (`rules.rs`: FRK-2 depends on FRK-1 `accepted` and awaiting, so the tick is `Idle`; after `task.integrated` for FRK-1 the next tick starts FRK-2's plan session) and `waits_for_a_dependency_the_board_does_not_hold` (a dependency with no row is not counted as integrated); each was seen to fail with its mutation put back. The migration's column carries `CHECK (awaiting_integration IN (0, 1))`, as the board's other flags do. `task.integrated` names no actor through the event's attribution rule, its `integrated_by` being a closed vocabulary that cannot be blank, and `pull_request.opened` names none, being Farik's own act.
- Changed 2026-09-23 in execution (Task 5): `GIT_TERMINAL_PROMPT=0` alone did not stop a prompt where the environment sets `GIT_ASKPASS` (an editor's terminal sets it, and git asks an askpass program before the terminal), so `pushes_and_fetches_without_a_prompt` hung; the shared runner also sets `GIT_ASKPASS` to empty, which git reads as "no askpass" and so stops its fall-back to `core.askPass` and `SSH_ASKPASS` too, while a credential helper still answers. `farik_git_push` goes through `Git::push` and so the shared runner. Decided where the plan was silent: `integrate` on a task the log does not know is `Refused { reason: "no_such_task: .." }`, and on an accepted epic `Refused { reason: "an_epic: .." }`, since an epic has no branch and no `task.integrated` to answer with; the integration branch refused by `check_branch_name` during an integration is an integration escalation with git's words, as any other git failure is; the conflict's detail names the paths and says to resolve and run `farik integrate <id>`. The integration tests live in `orchestrator/integrate.rs`'s own tests rather than `rules.rs`'s, and `takes_one_task_from_ready_to_accepted` in `orchestrator.rs` (step 12's note), now counting the branch's commit from `main` as it was before the run, since the merge puts it in `main`. `merges_one_task_at_a_time_across_orchestrators` gives FRK-2's branch a commit of its own, because two fixture commits of `done.txt` made in one second on one parent are one commit; it was seen to fail with the lock taken out (git's `index.lock` refusals and a merge escalated).
- Changed 2026-09-23 in execution (Task 6): decided where the plan was silent: a pull request merged on the forge whose fast-forward fails answers `Escalated` (the escalation's words), `task.integrated` recorded first; a pull request's state gh cannot give, and no `origin` under `pull_request`, are integration escalations with their words; a note never written reads "(none was written)" in the body. The integration branch's head, which a closed pull request whose branch is in it is recorded at, is read as `Git::merge_base(<into>, <into>)` rather than a new `Git` method. The harness holds a `FakeGh` of its own, which every orchestrator it makes drives, and `orchestrator_with_forge` gives one another forge.
- Changed 2026-09-23 in execution (Task 7): the tests live in `orchestrator/recover.rs`; the harness gains `started_session` to seed a session a killed run left open. The team's WIP limit is two in them, as FRK-1 and FRK-2 are both `dev-a`'s.
- Changed 2026-09-23 by the landing review: `GIT_TERMINAL_PROMPT` and `GIT_ASKPASS` do not stop ssh, which asks on the terminal for a host key or a passphrase and would block with the integration lock held, so a push or fetch runs with `GIT_SSH_COMMAND="ssh -o BatchMode=yes"` unless the environment sets `GIT_SSH_COMMAND` or `GIT_SSH` or `git config core.sshCommand` is set (`GIT_SSH` is added to the controller's two, because `GIT_SSH_COMMAND` takes its place); other git calls, which never run ssh, are not given it, so that they do not each pay for a `git config` read. `check_branch_name` also refuses `@` and any name whose `git check-ref-format --branch` output differs from it (`@{-1}` prints the branch it stands for). `has_remote` and `fetch_fast_forward`'s `refs/heads/` are pinned by a remote named `origin-mirror` and by tags on `origin` named as the branches.
- Changed 2026-09-23 by the landing review: the catch-up under the lock is pinned by `answers_a_task_another_process_integrated`, a `task.integrated` appended to the log and not projected, as a process stopped between the two leaves it, rather than by two `Projections` on one log: the projections share the log's database, so a second one sees every apply the first makes and cannot tell the catch-up from its absence. The review's tag named `main` in `merges_and_pushes_under_auto_merge` found that `Git::current_branch` said `heads/main` beside it (`symbolic-ref --short`), so the integration branch was misread; it now reads the full reference. `merges_one_task_at_a_time_across_orchestrators` gives FRK-1's branch a commit of its own as well as FRK-2's, since FRK-2's branch held FRK-1's fixture commit and whichever merged second could be already merged.

## File map

```
docs/schemas/team.schema.json, crates/core/src/team.rs, crates/core/src/team/fixtures.rs   modifies: local_merge becomes auto_merge
crates/cli/src/init.rs, crates/cli/tests/commands.rs   modifies: the starter team writes auto_merge and says it pushes; its test
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: task.integrated, pull_request.opened, escalation.raised's description
crates/store/src/migrations/0005_integration.sql     creates: awaiting_integration
crates/store/src/migrations.rs                       modifies: lists 0005; apply_through
crates/store/src/projections.rs                      modifies: TaskProjection::awaiting_integration; tests
crates/store/src/git.rs, crates/store/tests/git.rs   modifies: has_remote, fetch_fast_forward, check_branch_name, GIT_TERMINAL_PROMPT, merge's doc naming step 13; tests
crates/runtime/src/transitions.rs                    modifies: a dependency's `integrated`; the integration branch checked; tests
crates/runtime/src/sandbox.rs, sandbox/docker.rs, sandbox/host.rs   modifies: SandboxFactory::remove; tests
crates/runtime/src/forge.rs                          creates: Forge, ForgeError, PullRequest, PullRequestState; tests against the fake gh
crates/runtime/src/orchestrator/integrate.rs         creates: cleanup, the lock, the three policies, integrate
crates/runtime/src/orchestrator/recover.rs           creates: recover, RecoveryReport
crates/runtime/src/orchestrator/rules.rs             modifies: rules 1 and 2; tests
crates/runtime/src/orchestrator/fixtures.rs          modifies: CountingSandboxFactory counts remove; FakeGh; a bare origin; the conflict
crates/runtime/src/orchestrator.rs, lib.rs           modifies: the error variants, IntegrationOutcome, integrate, OrchestratorDeps::forge, `pub mod forge;`
crates/runtime/tests/one_task.rs                     modifies: after acceptance
docs/SPEC.md                                         modifies: 5.14 (the three policies and auto_merge the default; Farik pushes on the user's behalf, not an agent; an integration escalation keeps the task accepted, is not polled again, and the human's integrate records a forge merge; gh may pick upstream; the file lock), 5.7 (the same), 8.5 (pull_request.opened), 5.15 (interrupted is `aborted` with its detail; what recovery does)
docs/plans/project-plan.md                           modifies: step 13's interface line; the phase 1 open item on EscalationReason::Integration marked closed here
```

## Interfaces

Consumes: `Orchestrator`, `sandbox_for` and the sandbox map, the rules, `orchestrator::fixtures`, `CountingSandboxFactory` (steps 11 and 12); `record_session_ended`, `session.started` (step 08); the base container's and worktree's names (step 06); `Git::push`, `Projections::catch_up` (step 05); `Transitions`, `integration_branch` (step 04); `record_session_cost`, `CostSource` (step 03); `SandboxFactory` (step 02); `Git::merge`, `MergeOutcome`, `Git::remove_worktree`, `Git::commit_count`, `TempRepo` (main).

Produces:

```rust
// wire bodies
TaskIntegratedBody { sha: String, into: String, integrated_by: TaskIntegratedBodyIntegratedBy }   // governor | human
PullRequestOpenedBody { url: String, number: u64, branch: String }
// farik-store
TaskProjection { .., pub awaiting_integration: bool }
pub(crate) fn apply_through(connection: &mut Connection, last_version: i64, now: DateTime<Utc>) -> Result<(), StoreError>;   // migrations
impl Git {
    pub fn has_remote(&self, name: &str) -> Result<bool, GitError>;
    pub fn fetch_fast_forward(&self, remote: &str, branch: &str) -> Result<(), GitError>;
    pub fn check_branch_name(&self, name: &str) -> Result<(), GitError>;
}
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

The harness is step 11's, its team `manual` unless a test writes another policy into the team file. An "accepted" fixture task has its worktree already removed, so rule 1 does not take the tick. A conflict is made by committing a different first line of `a.txt` on `main` after `farik/FRK-1` changed it, and resolved by a commit on `main` restoring `a.txt` to its text at the branch point. `origin`, where a test has one, is a bare repository made by `git init --bare` beside the `TempRepo`. The fake `gh` is `fixtures::FakeGh`, a shell script the fixture writes in a temporary directory: for its second argument (`list`, `create`, `view`) it prints the stdout and stderr the test gave for that subcommand and exits with the code given (0 when none was), appends its arguments NUL-separated, one call per line, to a file beside it, and saves its standard input; `FakeGh::calls()` reads the calls back and `FakeGh::stdin_of("create")` the body. It and every test that uses it are `#[cfg(unix)]`. In Task 6, every test but the first seeds `pull_request.opened { url: "https://github.com/o/r/pull/7", number: 7, branch: "farik/FRK-1" }` after the acceptance, so that the tick goes straight to the state check.

### Task 1: auto_merge by name and by default

Files: `team.schema.json`, `team.rs`, `team/fixtures.rs`, `init.rs`, `crates/cli/tests/commands.rs`

- `reads_a_team_with_every_field_it_may_have` (existing, in `team.rs`) gains the assertion `policy.integration == Integration::AutoMerge` for the fixture, which now says `auto_merge`.
- `refuses_the_old_local_merge_spelling` — a team with `integration: local_merge` is refused with a failure at `/policy/integration`.
- `writes_a_starter_team_that_merges_on_its_own` (`crates/cli/tests/commands.rs`) — the team `farik init` writes reads back with `Integration::AutoMerge`, and the command's output contains `pushed to origin`.

- [x] `feat(core): make auto_merge the integration policy a new team starts with`

### Task 2: the events and the board

Files: the event schema, `event.rs`, `event/fixtures.rs`, the migration, `migrations.rs`, `projections.rs`, `transitions.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers `task.integrated` and `pull_request.opened`.
- `awaits_integration_from_acceptance_until_integrated` — a task is not awaiting before, is after a `task.transitioned` into `accepted`, and is not after `task.integrated`; an epic moved into `accepted` is not awaiting.
- `reads_an_older_accepted_task_as_awaiting` — a database brought to version 4 by `migrations::apply_through(.., 4, ..)` holding an `accepted` task row and an `accepted` epic row, then opened with `open_event_log`: the task is `awaiting_integration` and the epic is not.
- `reads_a_dependency_as_integrated_once_merged` — FRK-2 depends on FRK-1: with FRK-1 `accepted` and awaiting, the assignment context's dependency has `integrated: false`; after `task.integrated` for FRK-1, `true`.

- [x] `feat(store): show accepted tasks awaiting integration`

### Task 3: cleanup

Files: `sandbox.rs`, `sandbox/docker.rs`, `sandbox/host.rs`, `orchestrator/integrate.rs`, `rules.rs`, `orchestrator/fixtures.rs`

- `removes_a_container_by_name` (Docker, ignored under `--integration` as step 02's are) — after `create` and `create_base` for FRK-1, `remove` leaves no container of either name; a second `remove` is `Ok`.
- `cleans_up_a_task_once_it_is_accepted` — FRK-1 `accepted` with its worktree, a detached `FRK-1-base` worktree, and a sandbox in the map: one tick calls `CountingSandboxFactory::remove` once for FRK-1, `.farik/local/worktrees/FRK-1` and `FRK-1-base` are gone, `git worktree list` names neither, `farik/FRK-1` still exists, and the map holds no FRK-1.
- `cleans_up_a_worktree_git_no_longer_knows` — a plain directory at that path with no registration: the tick removes it.

- [x] `feat(runtime): remove a finished task's worktree and container`

### Task 4: the forge

Files: `forge.rs`, `lib.rs`, `orchestrator/fixtures.rs` (`FakeGh`)

- `opens_a_pull_request_with_gh` — `list` answering `[]` and `create` answering `https://github.com/o/r/pull/7`: `PullRequest { url: that, number: 7 }`; the calls are exactly `pr list --head farik/FRK-1 --base main --state open --json url,number --limit 1`, then `pr create --base main --head farik/FRK-1 --title FRK-1: Add done --body-file -`; `stdin_of("create")` is the body.
- `reuses_an_open_pull_request_for_its_branch` — `list` answering `[{"url":"https://github.com/o/r/pull/7","number":7}]`: that `PullRequest`, and no `create` among the calls.
- `reads_a_pull_requests_state` — `{"state":"MERGED","mergeCommit":{"oid":"abc"}}` is `Merged { sha: "abc" }`; `{"mergeCommit":null,"state":"OPEN"}` is `Open`; `{"mergeCommit":null,"state":"CLOSED"}` is `Closed`; the arguments are `pr view <url> --json state,mergeCommit`.
- `says_when_gh_is_missing` — a program path that does not exist: `Missing` naming it.
- `reports_what_gh_said_when_it_fails` — exit 1 with stderr `not logged in`: `Failed` whose detail contains it; `view` printing `nonsense`: `Failed` naming the output; `create` printing `nonsense` after `list` answered `[]`: `Failed` naming the output.

- [x] `feat(runtime): drive pull requests through the gh program`

### Task 5: auto_merge and manual

Files: `orchestrator/integrate.rs`, `rules.rs`, `orchestrator.rs` (`Refused`, `Lock`, `IntegrationOutcome`, `integrate`), `orchestrator/fixtures.rs` (the bare origin, the conflict), `store/src/git.rs`, `store/tests/git.rs`, `transitions.rs`, `one_task.rs`, `docs/SPEC.md` (5.7, 5.14)

- Store (ignored, needs git): `knows_whether_a_remote_exists` — false, then true after `git remote add origin <bare>`; `fast_forwards_a_branch_to_its_remote` — for the branch checked out and for one that is not, and a local branch holding a commit `origin` lacks while `origin` holds another is `CommandFailed` with the local branch where it was; `pushes_and_fetches_without_a_prompt` — `origin` at `http://127.0.0.1:<port>/r.git`, served by a `TcpListener` in the test that answers every request `401` with `WWW-Authenticate: Basic realm="farik"` (plain HTTP, because HTTPS would need a certificate and git asks the same question after a 401): `push` and `fetch_fast_forward` are each `CommandFailed` whose stderr contains `terminal prompts disabled`, within 30 seconds.
- `refuses_an_integration_branch_git_would_not_name` (`transitions.rs`, ignored, needs git) — a team naming `main:other`, and one naming `-f`: `integration_branch` is `CommandFailed` naming it.
- `merges_and_pushes_under_auto_merge` — FRK-1 `accepted` under `auto_merge` with `origin`: one tick records `task.integrated { into: main, integrated_by: governor }` whose `sha` is `main`'s head, a merge commit with `farik/FRK-1` as a parent; `origin`'s `main` is that sha; the root checkout is on its starting branch.
- `merges_without_a_push_when_there_is_no_origin` — the same without a remote: `task.integrated`, no escalation.
- `escalates_a_failed_push_and_keeps_the_merge` — `origin` pointing at a path that does not exist: `task.integrated`, then `escalation.raised { reason: integration }` whose detail starts `merged locally as` and contains `git push origin main`; the next tick is `Idle`; `integrate(FRK-1)` then answers `Merged` with the same sha and appends nothing (a push would have failed and escalated).
- `escalates_a_conflict_and_leaves_the_task_accepted` — the conflict above: an escalation whose detail names `a.txt`, no `task.transitioned`, the board `accepted` and awaiting, and the next tick `Idle`.
- `integrates_for_the_human_after_an_escalation` — then `integrate(FRK-1)` without the resolution is `Escalated` and appends a second escalation; after the resolution it is `Merged` with `integrated_by: human`.
- `answers_an_integrated_task_without_merging_again` — `integrate` on FRK-1 once integrated: `Merged` with the same sha and no second `task.integrated`.
- `leaves_an_accepted_task_to_the_human_under_manual` — `manual`: the tick is `Idle` and nothing merged; `integrate` then merges and does not push.
- `refuses_to_integrate_what_is_not_accepted` — a `verifying` task: `Refused`.
- `merges_one_task_at_a_time_across_orchestrators` — the root checkout on a branch `work` that is not the integration branch `main`; two `Orchestrator`s over one repository, each integrating its own accepted task concurrently, twenty times on fresh repositories: every call `Merged`, the checkout left on `work` every time, both merge commits on `main`.
- `takes_one_task_from_ready_to_accepted` (extended; the team `auto_merge`, no remote) — after `run_until_idle`: `task.integrated` for FRK-1, the worktree gone, `farik/FRK-1` kept.

- [x] `feat(runtime): merge accepted work one task at a time`

### Task 6: pull requests

Files: `orchestrator/integrate.rs`, `rules.rs`, `orchestrator.rs` (`OrchestratorDeps::forge`, `OrchestratorError::Forge`), `orchestrator/fixtures.rs` (the deps' `Forge` at the `FakeGh`), `docs/SPEC.md` (5.14)

- `opens_a_pull_request_for_an_accepted_task` — `pull_request`, `origin`, `list` answering `[]`, `create` answering a URL ending `/pull/7`: `origin` has `farik/FRK-1`; `pull_request.opened { number: 7, branch: farik/FRK-1 }`; `stdin_of("create")` holds `## Intent` and the intent, `## Completion note` and the completion note, `## Review note` and the review note, in that order; then `integrate(FRK-1)` with `view` answering `{"mergeCommit":null,"state":"OPEN"}` is `AwaitingForge`, and the log still holds one `pull_request.opened` and the calls one `create`.
- `waits_while_the_pull_request_is_open` — `view` answering `{"mergeCommit":null,"state":"OPEN"}`: the tick is `Idle` and appends nothing.
- `records_a_pull_request_merged_on_the_forge` — the test pushes a merge of `farik/FRK-1` to `origin`'s `main` from a second clone and `view` answers `MERGED` with that commit: `task.integrated { sha: it, integrated_by: human }`, and the local `main` is at it.
- `escalates_a_closed_pull_request_once` — `CLOSED`: an escalation containing "was closed without merging"; the next tick is `Idle` and `gh` was not called again.
- `integrates_a_closed_pull_request_the_human_merged` — after that escalation, `integrate(FRK-1)` is `Escalated` with a detail containing `reopen it` and appends a second escalation; after the test pushes a merge of `farik/FRK-1` to `origin`'s `main` from a second clone, `integrate(FRK-1)` is `Merged` with that commit, `task.integrated { integrated_by: human }` is recorded, and the local `main` is at it.
- `escalates_when_gh_is_missing` — `origin`, and a `Forge` whose program does not exist: an escalation naming it, and no `pull_request.opened`.

- [x] `feat(runtime): integrate through pull requests the human merges`

### Task 7: recovery

Files: `orchestrator/recover.rs`, `orchestrator.rs`, `docs/SPEC.md` (5.15, 8.5), `docs/plans/project-plan.md`

- `recovers_an_interrupted_run` — a log with one `session.started` and no end or cost, FRK-1 `accepted` with a worktree left, FRK-2 `in_progress` with its worktree and a seeded commit on `farik/FRK-2`: `recover` answers `sessions_interrupted: 1`, `worktrees_removed: 1`, `tasks_resumed: 1`; the log gains `session.ended { reason: aborted }` whose detail starts `interrupted` and a `cost.recorded` with zero tokens for that session; FRK-1's worktree is gone.
- `resumes_an_in_progress_task_in_a_fresh_sandbox` — then one tick: `CountingSandboxFactory` saw one `create`, for FRK-2, and the implement session's first message contains `Resuming: last commit <sha>` with the seeded commit's sha.
- `recovers_nothing_twice` — `recover` again answers `sessions_interrupted: 0`, `worktrees_removed: 0`, `tasks_resumed: 1` (FRK-2 is still `in_progress`) and appends nothing.

- [x] `feat(runtime): recover an interrupted run`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
