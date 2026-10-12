# Phase 9, step 02c: Integration per member

Status: ready (readiness review 2026-10-12, two rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.2, 5.14, 5.15; F6
Depends on steps 01, 02 and 02b of this phase, planned and ready or in review, not yet executed:
- from step 01: `repo.remove` in `daemon/repos.rs`
- from step 02: `task_repository`, `TaskRepository`, `TaskRepositoryError`, `Harness::project_folder`, a member task's worktree made by the member's `Git`
- from step 02b: `rescan_members`, `MembersScan`, `refresh_the_scan`'s skip in a project folder, the `<id>-base` worktree made by the member's `Git` under the project folder
- ADR 0057's amendment of 2026-10-12

Nothing of steps 03 to 08.
Mockups: none.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on B1 (`remove_workspace` when the task's repository cannot be resolved), decided below; S1–S5 and N1–N8 folded. Round 2 (the same reviewer, confirming): ready; its S1, S2 and N1–N4 folded.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at c99ea62.

## Goal

An accepted task in a member is integrated in that member, under the team's one `integration` policy (5.14):
- `auto_merge` merges into the member's integration branch and pushes it to the member's `origin`
- `manual` merges there when the human runs `catervas integrate`
- `pull_request` pushes the branch and opens the pull request with `gh` in the member, and follows its state there

`task.integrated` names the member's branch. After the integration the member is rescanned and its checks are refreshed. A finished task's worktrees are removed by the member's git, or as plain folders when the member is missing. A task that depends on a task in another member waits until that task is integrated there. `repo.remove` refuses a member that still has work.

A one-repo project, a folder change, and a docs task in a project folder integrate exactly as today: in the project root, with its `git` and `gh`.

Out of scope:
- a folder change in a project folder merged at once whatever the policy (step 04)
- the screens (07)

## Decisions

- **One target per integration.** `integrate_locked` (`crates/runtime/src/orchestrator/integrate.rs:268`) resolves an `IntegrationTarget { repo, git, forge, into }` before it reads the policy, and passes it to `merge` (`:689`), `through_the_forge` (`:453`), `open` (`:579`), `closed_for_the_human` (`:534`) and `refresh_the_scan` (`:363`) in place of `tools.git`, `forge` and `integration_branch(team, &tools.git)` (`:302`). Nothing in those five functions reads `tools.git` or `forge` afterwards.
  - **`Subject::Task`** reads its contract and calls `task_repository(files, team, &tools.git, contract.repo)`. A member's target is its `Git`, `Forge { program: forge.program.clone(), root: <member's path> }`, and its `integration_branch`. A task with no `repo` gets the project root's, today's answer.
  - **`Subject::FolderChange`** gets the project root's target.
  - **A `Missing` member makes the integration wait**, as the design's tasks in a missing member wait (`multi-repo-projects.md:38`; step 02). On a tick `integrate_locked` returns `Ok(None)`, and the task integrates by itself on the first tick after `repo.locate`. The human's `catervas integrate` escalates it with `Missing`'s sentence, so they hear why nothing happened.
  - **Any other resolution failure** escalates the subject with its sentence through `escalate`, as an unreadable integration branch does today (`:302-305`). This covers `UnknownRepo`, `NotAProjectFolder`, `Files`, `Git`, and a contract that cannot be read, which today propagates through `?` (`Subject::branch`, `:140-145`) and now escalates, since it is about the subject and not the tick. Once escalated, a tick asks no more (`escalated_since_accepted`).
  - The target is resolved where `:302` resolves the branch today: after `refuse_unless_integrable`, the `awaiting` check and `escalated_since_accepted`. `task_repository` takes `contract.repo.as_ref().map(|repo| repo.as_str())`.
  - Rejected: a `Forge` per member held by the orchestrator. Members change while serve runs (step 01), and the one `program` is all a `Forge` keeps besides its root.
- **The one lock.** The integration lock stays the project folder's `.catervas/local/integration.lock` (`attempt`, `:242`), so integrations into different members are serialised too. This is ADR 0057's decision, and step 01's `repo.*` methods already take the same lock.
- **Merging into a member** checks out the member's integration branch in the member's own checkout, merges with `--no-ff` and `Merge <id>: <title>`, and puts back what was checked out, exactly as `Git::merge` (`crates/store/src/git.rs:635`) does in a one-repo project today. A member with uncommitted changes to a file the merge touches makes git refuse, which escalates as today.
- **`task.integrated { sha, into, integrated_by }`** is unchanged: `into` is the member's branch name and `sha` the member's merge commit. `diff_of` (step 02) resolves the task's repository before reading them, so they are read where they were made. No `repo` field is added; the contract names it.
- **The rescan after an integration into a member.** In a project folder, `refresh_the_scan` for a task with a `repo`:
  - checks that the member's checkout is on `into` (the target's `git.current_branch()`), else `Skipped { why: "<repo>'s checkout is on <branch>, not <into>" }`
  - then calls `rescan_members(files, log, ids, now, Some(<repo>), GOVERNOR)`
  - and answers `Refreshed` when `library_changed`, else `Unchanged`; a scan error is `Failed { error }`

  A task with no `repo` keeps 02b's `Unchanged`. `rescan_members` records `project.scanned` on every such integration, one event per member integration, since a member's read-back reads its last commit. Rejected: comparing `material` first (`scan.rs:235`), which needs the other members' last read-backs that no event keeps apart.
- **Removing a finished task's worktrees.** `remove_workspace` (`integrate.rs:959`) still returns early when neither folder exists (`:965-967`). Only then does it read the task's contract and the team and resolve the task's repository, so ticks and recovery read nothing for finished rows.
  - **When the repository resolves to a member**, that member's `Git` removes `<id>-base` and `<id>` (`remove_worktree`, `:1001`), then runs `git worktree prune` in the member (`Git::prune_worktrees`, new) so that no stale registration keeps the task's branch from being checked out there. Any error from the member's own removal (a member moved and located answers `'…/.git' is not a .git file`) removes that folder as a plain folder and still prunes.
  - **When the repository is the project root, or when resolving fails** (any `TaskRepositoryError`, an unreadable contract or team), `remove_worktree(&deps.tools.git, ..)` runs exactly as today. For a member's worktree the root's git answers `is not a working tree`, which `:1006` tolerates, and then removes the folder. A one-repo project is unchanged.
  - A member whose folder is missing keeps a stale registration of the worktree until a later finished task in that member prunes it, and until then the task's kept branch cannot be checked out there (`'feature/CTV-1' is already used by worktree at …`). 5.15 says so.
  - The sandbox, the browser folder and the order are unchanged. `recover.rs:88` calls it as before.
- **Dependencies across repositories need nothing new.** A dependency counts as integrated when it is `accepted` and not awaiting integration (`transitions.rs:1218-1229`), whichever repository it went into. The dependent task's worktree is made from its own member's integration branch (step 02). A test pins it.
- **`repo.remove` refuses a member with work.** Step 01's `repo.remove` gains `repo_in_use` at `/name`, checked before `last_repo`. It fails when any task on the board names the member and is neither `cancelled` nor `accepted` and integrated (`!awaiting_integration`). A contract that cannot be read answers `INTERNAL_ERROR` with the files' sentence, and nothing is removed. The sentence: "<n> of the project's tasks still work in <name>: <ids joined ", ">; finish or cancel them first". An epic names no repository, so it never counts.
- **The fake `gh` says where it ran.** `FakeGh` (`orchestrator/fixtures.rs:1339`) also appends `pwd -P` to `<dir>/cwds` for each call, read by a new `cwds() -> Vec<PathBuf>`, so a test can tell the project root from the member, compared with each one's canonicalised path. `Forge::run` sets `current_dir(self.root)` (`forge.rs:197`), so the member's `Forge` runs there.
- **Fixtures for members.** `orchestrator/fixtures.rs` gains three helpers, which the existing ones (`with_origin`, `:591`; `merge_branch_on_the_forge`, `:612`; `accepted_with_worktree`, `:435-504`) delegate to with the project's repository:
  - `with_origin_in(repo: &Path) -> PathBuf`
  - `merge_branch_on_the_forge_in(repo: &Path, origin: &Path, branch: &str) -> String`
  - `accepted_in(task: &str, member: &Path, repo: Option<&str>)`: files the task with `repo` when given, makes its worktree with `Git::open(member)` from `main`, commits `done.txt`, and moves it to `accepted`

## File map

```
crates/runtime/src/orchestrator/integrate.rs        modifies, tests: IntegrationTarget, its resolution and use (Task 1); refresh_the_scan per member (Task 2); remove_workspace (Task 3)
crates/runtime/src/orchestrator/fixtures.rs         modifies: FakeGh's cwds, with_origin_in, merge_branch_on_the_forge_in, accepted_in (Task 1)
crates/store/src/git.rs, crates/store/tests/git.rs  modifies, tests: prune_worktrees (Task 3)
crates/runtime/src/orchestrator/rules.rs            tests: a dependency in another repository (Task 1)
crates/runtime/src/daemon/repos.rs                  modifies, tests: repo_in_use (Task 4)
docs/schemas/rpc.schema.json                        none: repo.remove's refusal travels in data.errors as its others do
docs/SPEC.md                                        modifies (Task 5)
```

## Interfaces

Consumes:
- step 02's `task_repository`, `TaskRepository`, `TaskRepositoryError`, `Harness::project_folder`
- step 02b's `rescan_members`, `MembersScan`
- step 01's `repo.remove` and its refusal helper
- `Forge` (`crates/runtime/src/forge.rs:15`)
- `escalate`, `merge`, `through_the_forge`, `open`, `closed_for_the_human`, `refresh_the_scan`, `remove_workspace`, `remove_worktree`, `ScanRefresh`, `GOVERNOR`
- `TaskProjection.awaiting_integration`
- `FakeGh`, `with_origin`, `merge_branch_on_the_forge`, `accepted_with_worktree` (`orchestrator/fixtures.rs`), `TempRepo`

Produces:

```rust
// catervas_runtime::orchestrator::integrate (private to the module)
struct IntegrationTarget { repo: Option<String>, git: Git, forge: Forge, into: String }
enum Unresolved { Missing(String), Other(String) }   // the sentence; Missing waits on a tick
fn target(tools: &ToolDeps, forge: &Forge, team: &Team, subject: &Subject) -> Result<IntegrationTarget, Unresolved>;
// catervas_store::git::Git
pub fn prune_worktrees(&self) -> Result<(), GitError>;   // git worktree prune
// the five functions take `target: &IntegrationTarget` in place of their `into: &str` and their reads of tools.git / forge
// FakeGh
pub(crate) fn cwds(&self) -> Vec<PathBuf>;
```

## Tasks

### Task 1: A member task integrates in its member

Files: `integrate.rs` (the target, its resolution in `integrate_locked`, the five functions' arguments); `orchestrator/fixtures.rs` (`FakeGh`'s `cwds`); `rules.rs` (a test). Tests are integration tests on `Harness::project_folder("x", &["app", "infra"])`, which returns the harness and the members' `TempRepo`s; each member is given an origin by `with_origin_in`. The task is `CTV-1`, made by `accepted_in("CTV-1", <app>, Some("app"))`.

- `merges_into_its_member_and_pushes_it`: under `auto_merge`, the tick:
  - leaves on `app`'s `main` a merge commit `Merge CTV-1: <title>` with two parents
  - pushes `app`'s `main` to its origin
  - records `task.integrated { into: "main", sha }` with that commit's sha
  - leaves the project folder's `main` and `infra` untouched
  - leaves `app`'s checkout on `topic`, the branch the test checked out before the tick

  RED: the merge runs in the project folder and escalates.
- `merges_in_the_member_when_the_human_asks`: under `manual` the tick does nothing, and `catervas integrate CTV-1` (`Orchestrator::integrate`) merges into `app`'s `main` without pushing. RED.
- `opens_and_follows_the_pull_request_in_the_member`: under `pull_request`, the tick pushes `feature/CTV-1` to `app`'s origin and calls `gh pr create`, with `cwds()` naming `app`'s canonical path. The test merges it with `merge_branch_on_the_forge_in` and the fake answers the pull request merged; the next tick fast-forwards `app`'s `main` from its origin and records `task.integrated`, every `gh` call in `app`. RED.
- `waits_while_its_member_is_missing`: under `auto_merge`, with `app` moved away, two ticks record nothing and leave `CTV-1` awaiting integration. With `app` moved back, the next tick merges it, with no escalation recorded. Separately, with `app` moved away again for a second accepted task `CTV-2`, the human's `catervas integrate CTV-2` records one `escalation.raised` whose words say `app is not at <path>`; once `app` is back, `catervas integrate CTV-2` merges it (a resolved escalation does not clear `escalated_since_accepted`, `integrate.rs:875-893`, so ticks leave it to the human). RED.
- `waits_for_a_dependency_in_another_repository` (`rules.rs`, shaped as `waits_for_an_accepted_dependency_until_it_is_integrated`, `:2781`): `CTV-2` (`repo: infra`, `ready`) depends on `CTV-1`. Under `manual`, while `CTV-1` awaits integration, the tick is `Idle` and `adapter.started()` is empty. After `catervas integrate CTV-1` has merged it into `app`, the next tick acts on `CTV-2` with one session started. RED: `catervas integrate CTV-1` escalates, so `CTV-2` never moves.
- `still_integrates_a_docs_task_and_a_folder_change_in_the_project_folder` (a guard: it passes before the change and must still pass after it; on step 02's `Harness::project_folder_with(.., with_the_suggested_six)`): under `auto_merge`, an Architect's task with no `repo` and a folder change each land on the project folder's `main`, and `app`'s and `infra`'s `main` are untouched. Under `pull_request`, with an origin added to the project folder by `with_origin_in`, every `cwds()` entry is the project root's canonical path.

- [ ] `feat(runtime): integrate a task into its member repository`

### Task 2: The member is rescanned after its integration

Files: `integrate.rs` (`refresh_the_scan`).

- `rescans_the_member_it_integrated_into`: `app` has a committed `package-lock.json` (as 02b's fixture) and `app-the-tests-pass` in the library. The task adds `scripts.lint` to `app`'s `package.json`. After `auto_merge` the library has `app-the-linter-is-clean` (`repo: app`), `infra`'s entries are unchanged, `criteria.updated { updated_by: governor }` and `project.scanned` are recorded, and the outcome's scan is `Refreshed`. `updated_by: governor` is deliberate: it is the integration's actor, as today's rescan records it (`integrate.rs:428`), where 02b's are the human's and `catervas`'s own. RED: 02b answers `Unchanged` for every integration in a project folder.
- `skips_the_rescan_when_the_members_checkout_is_elsewhere`: with `app`'s checkout on another branch at integration time, the outcome's scan is `Skipped` with the Decisions' words naming `app`. RED.

- [ ] `feat(runtime): rescan a member after work lands in it`

### Task 3: A member task's worktrees

Files: `integrate.rs` (`remove_workspace`); `git.rs` (`prune_worktrees`) and `tests/git.rs`.

- `prunes_stale_worktrees` (`tests/git.rs`, with the file's `#[ignore = "needs the git program: cargo xtask check --integration"]`): a worktree whose folder was deleted is listed `prunable`, and after `prune_worktrees` is not listed. RED.
- `removes_a_member_tasks_worktrees_with_its_git`: after `CTV-1` is integrated, the cleanup tick removes `<project>/.catervas/local/worktrees/CTV-1` and `CTV-1-base`. `git -C app worktree list` no longer lists either, and `app` keeps `feature/CTV-1`, which `git -C app checkout feature/CTV-1` can check out. RED: `git -C app worktree list` still lists `CTV-1` as `prunable`.
- `removes_the_folders_of_a_task_whose_repository_is_gone` (a guard: it passes before the change, and fails if the fallback is replaced by `?`): `CTV-1` cancelled with its worktree still there, then `repo.remove app` (no work holds it). The next tick removes both folders and reports it, with no error. Also, with `app` moved away and `CTV-2` cancelled in it, the tick removes its folders the same way.
- `removes_the_folders_of_a_moved_and_located_member`: `app` moved and located, then `CTV-1` cancelled. The tick removes its folders, though `app`'s git refuses the worktree as not its own, and `git -C app worktree list` no longer lists `CTV-1`. RED: it stays listed as `prunable`.

- [ ] `feat(runtime): remove a member task's worktrees with its member's git`

### Task 4: A member with work is kept

Files: `daemon/repos.rs` (`repo.remove`).

- `refuses_to_remove_a_member_with_work`: with `CTV-1` (`repo: app`) `in_progress`, `repo.remove { name: "app" }` is refused `repo_in_use` at `/name` with the Decisions' sentence naming `CTV-1`, nothing written or recorded. Once `CTV-1` is cancelled, the removal goes through. A `CTV-2` naming `app` that is accepted and integrated does not hold it. With `CTV-3`'s contract file made unreadable, the answer is `INTERNAL_ERROR` and nothing is removed. RED.

- [ ] `feat(runtime): keep a member that still has work`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.14: a member task integrated in its member under the one policy and the one lock; its push and its pull request there; the rescan of that member; `task.integrated`'s `into` being the member's branch
- 5.2: a dependency in another repository
- 5.15: a task in a missing member waits to integrate; its worktrees removed as folders, and the stale registration until the member is next pruned
- the section that lists the web app's queries and methods, where step 01 records `repo.*` and their refusals: `repo_in_use`

- [ ] `docs(spec): record integration per member`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
