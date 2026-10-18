# Standalone internal-task-ids, plan 3: an agent's commit names no task

Status: done
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.14 "Who commits" (recorded by plan 6)
Depends on: plan 1 of this branch
Readiness confirmed by: the planner, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

An agent's commit, made through `catervas_git_commit`, never carries a task id. The tool refuses a
message that names one, and the agent writes it again. The tool's description asks for
Conventional Commits with no task id, so the agent writes them that way the first time.

## Decisions

- The check is `task_id_in(&input.message)` (plan 1). It runs first in `commit`, before any path is
  checked or any file removed, so a refused commit changes nothing.
- The refusal is `Refusal::CommitMessageNamesATask { id: String }`, code
  `commit_message_names_a_task`, detail
  `"{id} is a task id, which stays inside Catervas; describe the change instead"` (ADR 0066). It
  sits in `refusal.rs` beside `PathIsADirectory` and is answered through `workspace()`, the arm
  that already answers a commit's own refusals.
- The description of `catervas_git_commit` in `crates/runtime/src/tools.rs` becomes exactly:
  "Commit the named paths of the task's worktree on the task branch. Write the message as
  Conventional Commits: a type, a colon and what changed (feat: ..., fix: ..., docs: ...). Name no
  task id."
- Only the id is enforced. The Conventional shape is asked for (ADR 0066).
- No role's `system.md` or skill changes. The tool's description is what every agent with the
  tool reads, and no skill names a commit message format today.
- The hooks (`daemon/hooks.rs`) decide the tool's tier only. No second check is added there.

## File map

```
crates/runtime/src/tools/git.rs        modifies: commit checks the message first (Task 1)
crates/runtime/src/tools/git.rs        tests:    refuses_a_commit_message_naming_a_task (Task 1)
crates/runtime/src/tools/refusal.rs    modifies: Refusal::CommitMessageNamesATask, its code and words (Task 1)
crates/runtime/src/tools.rs            modifies: catervas_git_commit's description (Task 1)
```

## Interfaces

Consumes: `task_id_in` (plan 1, `catervas_core::forge_text`).

Produces:

```rust
// crates/runtime/src/tools/refusal.rs
pub(crate) enum Refusal {
    // ...
    /// A commit message that names a task, whose id stays inside Catervas (ADR 0066).
    CommitMessageNamesATask { id: String },
}
```

## Tasks

### Task 1: an agent's commit names no task

Files: modified `crates/runtime/src/tools/git.rs`, `crates/runtime/src/tools/refusal.rs`,
`crates/runtime/src/tools.rs`.
Produces: `Refusal::CommitMessageNamesATask`.
Consumes: `task_id_in`.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `refuses_a_commit_message_naming_a_task`, beside `commits_through_the_tool_on_the_tasks_worktree`
  and built like it (`CTV-1` in progress, held by `dev-a`, its worktree made, `src/login/form.ts` written in it):
  - `catervas_git_commit` called by `dev-a` with `"feat: add the login form (CTV-1)"` naming `src/login/form.ts` is refused
    `commit_message_names_a_task`, its detail starting `CTV-1 is a task id`;
  - the branch's commit count from `main` is still 0;
  - the same call with `"feat: add the login form"` answers a `sha`, and the count is 1.

  Before the change the first call commits.
- No test compares the description's text today, and none is added: the words are not behaviour.

- [x] `feat(runtime): an agent's commit message names no task id`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
