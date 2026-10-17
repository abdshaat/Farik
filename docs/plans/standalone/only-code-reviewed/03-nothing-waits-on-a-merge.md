# Standalone 3 of 7: nothing waits on a merge

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.2 (the dependency paragraph), 5.14, 5.16 item 3, 6.1, 6.3; F9
Depends on: plans 1 and 2 committed on this branch (`is_plan_task`, `catervas_verifies_alone`, pull requests only); ADR 0061
Readiness confirmed by: pending

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin the old behaviour change in the task that changes it.

## Goal

Since Catervas never merges (plan 2), no work waits for the human's merge before it starts.
- A dependency counts once it is accepted.
- A task's worktree starts from its accepted, unmerged dependencies' branches.
- An epic's plan works the same way:
  - the Product Manager files the plan's tasks as soon as the plan task is accepted (plan 1: by the governor, with no session), reading the plan from the plan task's branch;
  - those tasks start from that branch;
  - the plan task opens no pull request of its own, and is integrated once its plan file is on the integration branch, carried there by its tasks' pull requests.
- An epic is still verified only once every task under it is integrated (unchanged).

Out of scope: the new project's product plan (plan 5).

## Decisions

- **A dependency counts once accepted.** `check_assignment`'s dependency rule (`crates/core/src/governor/gates.rs`) and rule 8's filter (`crates/runtime/src/orchestrator/rules.rs:2063`) take `status == Accepted` and no longer read `integrated`. `DependencyState.integrated` goes, since nothing reads it. The 5.2 sentence "A dependency counts as integrated when…" becomes "A dependency counts once it is accepted (ADR 0061)".
- **A task's start point** is computed by `start_point(deps, board, contract, repository) -> Result<Vec<String>, OrchestratorError>` (`requests.rs`). It lists the branches the worktree is made from, the first being the base:
  1. each dependency that is accepted, still `awaiting_integration`, has a branch (not a private-folder task), and has the same `repo` as `contract`, in `dependencies` order, as `task_branch`;
  2. then, for a contract with `plan` P that is not itself a plan task and has a `parent`: the branch of the last accepted plan task under that parent whose `plan` is P and whose `repo` equals its own, when P is not on the integration branch (`Git::has_file`);
  3. dropping any branch that does not exist (`Git::has_local_branch`) or appears twice.

  When the list is empty it is `[integration_branch]`.
- **`assigned`** (`rules.rs`) makes the worktree from the first branch. It then merges each further one inside the worktree with `Git::merge_in(worktree, branch, "Start <id> from <branch>")`.
  - On `MergeOutcome::Conflicts`, the worktree and its branch are removed (`remove_worktree`, then `Git::delete_branch`), and the task waits. The next tick tries again. Once a conflicting dependency is integrated it drops out of the list.
  - This was chosen over refusing or escalating, because the conflict resolves itself when the human merges.
  - Merging inside the task's own fresh worktree is not merging code into the integration branch, which stays the human's (ADR 0061).
- **A task's diff is taken from where it started.** This settles the earlier plan's readiness finding 1. A task started from another task's branch would otherwise show that task's changes in its own diff, since the diff is three-dot from the integration branch, and its merge base does not move when the other task is squash-merged. `paths_within_allowed` would then refuse it, and its reviewer, its UI-change check and its twin check would read the other task's work.
  - Once its worktree is made and merged, `assigned` writes the ref `refs/catervas/start/<id>` at the worktree's `HEAD` (`Git::set_ref`). The ref is written only when the start point is not `[integration_branch]`.
  - `catervas_store::git::task_base(git, integration_branch, task_id) -> String` answers that ref when it exists, else `integration_branch`.
  - Every place that takes a task's diff or base from its integration branch takes `task_base` instead:
    - `transitions.rs:819` (`work`'s changed paths and commit check);
    - `verify.rs:207` (the base of the new-tests check);
    - `verify.rs:412` (the design review's diff) and `verify.rs:510` (the review's diff);
    - `tools/git.rs:53` (`catervas_git_diff`);
    - `rules.rs:1767` (the commit count);
    - `crates/store/src/diff.rs:143` (the web's diff).
  - Integration is unchanged: the pull request is still from the task's branch into the integration branch.
  - This was chosen over adding the plan, or each dependency's paths, to `allowed_paths`, which would let the task change them, and over leaving the plan out of the diff by name, which handles plans only.
- **`Git::delete_branch(name)`** is `git branch -D <name>` at the root, refused for the branch checked out there.
- **`Git::merge_in`** (`crates/store/src/git.rs`) is `git -C <worktree> merge --no-ff -m <message> <from>`, as `merge_what_is_checked_out` does. On conflicts it aborts and answers `Conflicts`. It never checks out at the root, unlike `Git::merge`.
- **The epic's step** (`requests.rs`):
  - `Child` becomes `{ task_id: TaskId, status: TaskStatus, plan_task: bool }`, losing `awaiting_integration`.
  - `children_of_epic` uses `is_plan_task`.
  - `epic_step` answers `WriteThePlansTasks` when every live task is an accepted plan task, whatever its integration.
  - `in_progress_epic` passes the last live plan task's id to `product_managers_message(deps, team, contract, step, listed, plan_task: Option<&TaskId>)`. That reads the plan with `held_document` on the integration branch, else on `task_branch` of the plan task's contract in its `task_repository_of` repository. A missing branch (`GitError::CommandFailed`) or a missing member counts as no plan.
- **The messages** (`messages.rs`):
  - `plan_task_message`'s last sentence becomes "No agent reviews the plan: give its task only an `artifact` criterion that the plan exists. Catervas accepts it once the plan exists, and you are asked for its tasks then."
  - `plan_tasks_message` opens "The Architect's plan for the epic {epic} is at {plan}: {text}".
- **The prompts.** Plan 1 already removed the review sentences. Here:
  - the Product Manager's `system.md`: "once the plan is on the integration branch" becomes "once Catervas has accepted the plan task";
  - `writing-task-contracts` step 2 opens "Once Catervas has accepted the plan task (you are asked with the plan's text)".
- **A plan task opens no pull request.** `integrate_locked` (`integrate.rs`), for a contract that `is_plan_task`, answers from `carried_plan(tools, subject, plan, target, asker) -> Result<Option<IntegrationOutcome>, OrchestratorError>`:
  - with `target.git.has_file(into, plan)`: `record_integrated` (`sha` = `merge_base(into, into)`; `integrated_by` `governor` on a tick, `human` on `catervas integrate`) and `Merged`;
  - otherwise `None` on a tick, and for `catervas integrate` the refusal `carried_by_its_tasks: <id> writes an epic's plan, which reaches <into> with its tasks' pull requests`.

  It goes by the file, not by ancestry, so a squash merge counts. No scan refresh follows.

## File map

```
crates/core/src/governor/gates.rs, crates/runtime/src/transitions.rs   dependency once accepted (Task 1)
crates/runtime/src/orchestrator/rules.rs                               rule 8's filter (Task 1); assigned's start (Task 3)
crates/store/src/git.rs, crates/store/tests/git.rs                     merge_in (Task 2)
crates/runtime/src/orchestrator/requests.rs                            start_point (Task 3); epic step and message (Task 4)
crates/runtime/src/transitions.rs, orchestrator/verify.rs, tools/git.rs, crates/store/src/diff.rs   diffs from task_base (Task 3)
crates/runtime/src/orchestrator/messages.rs                            plan messages (Task 4)
crates/roles/roles/product_manager/system.md, skills/writing-task-contracts/SKILL.md   (Task 4)
crates/runtime/src/orchestrator/integrate.rs                           carried_plan (Task 5)
```

## Interfaces

Consumes: `is_plan_task` (plan 1), `task_branch`, `held_document`, `integration_branch`, `task_repository_of`, `TaskRepository`, `Git::has_file`, `Git::has_local_branch`, `Git::create_worktree`, `Git::remove_worktree`, `MergeOutcome`, `record_integrated`; test harnesses `an_accepted_plan_task_at`, `a_scrum_masters_epic_in_progress`, `integrated`, `commit_at_root`.

Produces:

```rust
// catervas_store::git::Git
pub fn merge_in(&self, worktree: &Path, from: &str, message: &str) -> Result<MergeOutcome, GitError>;
pub fn delete_branch(&self, name: &str) -> Result<(), GitError>;
pub fn set_ref(&self, name: &str, rev: &str) -> Result<(), GitError>;   // git update-ref
// catervas_store::git
pub fn task_base(git: &Git, integration_branch: &str, task_id: &TaskId) -> String;
// catervas_runtime::orchestrator::requests
pub(super) fn start_point(deps: &OrchestratorDeps, board: &[TaskProjection],
    contract: &TaskContract, repository: &TaskRepository) -> Result<Vec<String>, OrchestratorError>;
```

## Tasks

### Task 1: a dependency counts once accepted

Files: `gates.rs` (the rule and its words "<id> is not accepted yet"), `transitions.rs` (`dependency_states` without `integrated`), `rules.rs:2063`.

Tests:
- `assigns_a_task_once_every_dependency_is_accepted` (renames `assigns_a_task_only_once_every_dependency_is_accepted_and_integrated`, `gates.rs:1872`): a dependency `accepted` and still awaiting integration lets the assignment through; one `verifying` refuses it, naming the dependency.
- `rules.rs`'s `waits_for_a_dependency_to_be_integrated` and `waits_for_an_accepted_dependency_until_it_is_integrated` become `plans_a_task_whose_dependency_is_accepted`, asserting that the ready task gets its plan session while its dependency is accepted and not integrated.

- [ ] `feat(core): count a dependency once it is accepted`

### Task 2: merge into a task's own worktree

Files: `crates/store/src/git.rs`, `crates/store/tests/git.rs`.

Tests:
- `merges_a_branch_into_a_worktree_only` (`crates/store/tests/git.rs`):
  - In a repository with `main` checked out at the root, branches `a` and `b` each add a different file, and a worktree is made on `t` from `a`. `merge_in(worktree, "b", "m")` answers `Merged`; the worktree holds both files; the root is still on `main`, its tree unchanged.
  - When `b` and `t` change one line differently, it answers `Conflicts` naming the file, and the worktree is clean; after `remove_worktree`, `delete_branch("t")` leaves no branch `t` (`has_local_branch` false).

- [ ] `feat(store): merge a branch inside a task's own worktree`

### Task 3: a task starts from what it needs

Files: `requests.rs` (`start_point`), `rules.rs` (`assigned` uses it and writes the start ref), `crates/store/src/git.rs` (`set_ref`, `task_base`), and the diff sites of Decisions. Tests are ignored and need git, in `requests.rs`.

Tests:
- `a_task_starts_from_its_unmerged_dependencies_and_its_plan`:
  - Epic CTV-1 has its plan task CTV-2 accepted, with the plan only on `docs/CTV-2`. CTV-3 (`dev-a`, the same `plan`, lane 1) is assigned. After a tick, CTV-3's worktree holds the plan.
  - CTV-4 depends on CTV-3, which is accepted and awaiting integration with `site/a.txt` on `feature/CTV-3`. Once CTV-4 is assigned, its worktree holds the plan and `site/a.txt`.
  - A task with nothing unmerged starts from `main` (`git.commit_count("main", "feature/CTV-5")` is 0).
  - With CTV-3 and the plan changing one file differently, CTV-4 has no worktree after the tick and stays `assigned`.
  - A lane task CTV-3 started from the plan's branch, whose `allowed_paths` exclude the plan, commits its own file and moves to `verifying`. Its Definition of Done context's `changed_paths` holds only that file (`refs/catervas/start/CTV-3` exists).

- [ ] `feat(runtime): start a task from its unmerged dependencies and its plan`

### Task 4: the plan's tasks follow its acceptance

Files: `requests.rs`, `messages.rs`, the two prompt files.

Tests:
- `reads_an_epics_next_step` (changed): `Child` without `awaiting_integration`; an accepted plan task alone is `WriteThePlansTasks`; the other cases as today.
- `writes_the_plans_tasks_from_the_plan_tasks_branch` (replaces `writes_the_plans_tasks_once_the_plan_is_integrated`; ignored, needs git): `an_accepted_plan_task`, with `docs/CTV-2` made from `main` and the plan committed only there. The first tick starts the Product Manager's `plan` session on CTV-1 whose prompt holds `<untrusted source="plan">\n# Plan for CTV-1`, `## Lane 1` and `` `lane` ``, while CTV-2 still awaits integration and `main` has no plan.
- `asks_for_the_architects_plan_task` (`messages.rs`) also asserts the message ends with the sentence of Decisions. `asks_for_a_task_per_planned_task` asserts that it starts with `The Architect's plan for the epic CTV-1 is at docs/catervas/architecture/plans/CTV-1.md: `, the rest unchanged.
- `breaks_down_again_when_the_plan_is_missing` and `the_plan_is_read_in_either_format` keep their assertions.

- [ ] `feat(runtime): file an epic's tasks once its plan task is accepted`

### Task 5: a plan task rides with its tasks

Files: `integrate.rs`. The test is ignored and needs git.

Tests:
- `integrates_a_plan_task_by_its_plan_not_a_pull_request`:
  - An accepted plan task has its plan on its own branch. A tick opens no pull request (no `pull_request.opened`; the fake forge was asked nothing) and leaves the task awaiting.
  - `integrate` answers the refusal starting `carried_by_its_tasks: `.
  - With the plan committed on `main`, the next tick records `task.integrated { into: "main", integrated_by: "governor" }`, and the task no longer awaits.

- [ ] `feat(runtime): integrate a plan task once its plan reaches the integration branch`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
