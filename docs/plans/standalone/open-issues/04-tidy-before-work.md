# Standalone (open issues), plan 4: Catervas tidies before each work session

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2 (recorded in plan 8's revision 0.122)
Depends on: plan 3 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

What a tool changed in a task's worktree outside its allowed paths (`next dev` rewriting
`tsconfig.json` and `next-env.d.ts`) is put back before each implement session starts, not only
when the agent asks for review, so a developer mid-task is never stuck on files it cannot restore
(CTV-4 asked the owner again). The tidy never runs while a merge is in progress in the worktree,
which plan 5 leaves there on purpose. Out of scope: what the tidy puts back (ADR 0068, unchanged).

## Decisions

Every decision is ADR 0069's (decision 7); this section pins how.

- **One tidy, two callers.** `put_back_strays` becomes `pub fn tidy(&self, task_id: &TaskId,
  team: &Team, by: Option<&TransitionAsk>) -> Result<(), TransitionError>`, taking the door's
  `requests` lock itself when it is called from outside `request` (`by` is `None`), and not when
  `request` calls it (which holds the lock already, `by` is `Some(ask)`). Its event is stamped with
  the task, and with the request's agent and session when `by` is `Some`, as now; called before a
  session, it carries no agent and no session (Catervas did it).
- **Where.** Rule 6 (`rules.rs::in_progress`) calls `deps.tools.transitions.tidy(&row.task_id,
  team, None)?` after `resume` answers `Some` and before `sandbox_for`, for a task with no private
  folder. No other session type tidies: a review, a design review and an explore session change
  nothing, and the move into `verifying` tidies again.
- **Never during a merge.** `Git::is_merging(worktree)` is `git rev-parse -q --verify MERGE_HEAD`
  in the worktree: exit 0 is `true`, exit 1 `false`, anything else an error. `tidy` returns at once
  when it is `true`: a merge's staged changes outside the paths are the integration branch's, and
  putting them back would undo them in the merge commit.
- No new event kind; `worktree.tidied` is the same event in both places.

## File map

```
crates/store/src/git.rs                       modifies: Git::is_merging (Task 11)
crates/runtime/src/transitions.rs             modifies: put_back_strays becomes tidy, its lock and stamp, skips a merge (Task 11)
crates/runtime/src/orchestrator/rules.rs      modifies: rule 6 tidies before the implement session (Task 11)
crates/runtime/src/orchestrator/rules.rs      tests:    puts_back_what_tools_changed_before_the_work_session (Task 11)
```

## Interfaces

Consumes (on this branch): `Transitions::put_back_strays`, `Transitions::append`, `requests`
(the door's lock), `request` (`transitions.rs`); `Git::uncommitted_in`, `Git::put_back`
(`git.rs`); `in_progress`, `resume` (`rules.rs`); `Harness`, `Harness::in_progress`,
`Harness::worktree`, `Harness::events` (`orchestrator/fixtures.rs`).

Produces:

```rust
// crates/store/src/git.rs
impl Git {
    /// Whether a merge is in progress in the worktree at `worktree`: it has a `MERGE_HEAD`.
    pub fn is_merging(&self, worktree: &Path) -> Result<bool, GitError>;
}
// crates/runtime/src/transitions.rs
impl Transitions {
    /// Puts back what tools changed in the task's worktree outside its allowed paths (ADR 0068),
    /// before a move into `verifying` (`by` is the move's ask) or before a work session (`None`);
    /// nothing while a merge is in progress there (ADR 0069).
    pub fn tidy(&self, task_id: &TaskId, team: &Team, by: Option<&TransitionAsk>) -> Result<(), TransitionError>;
}
```

## Tasks

### Task 11: Catervas tidies before each work session, and never during a merge

Files: modified `crates/store/src/git.rs`, `crates/runtime/src/transitions.rs`,
`crates/runtime/src/orchestrator/rules.rs`; tested by `rules.rs`'s tests
Produces: `is_merging`, `tidy`
Consumes: `put_back_strays`, `in_progress`

`request` calls `self.tidy(&request.task_id, team, Some(ask))` where it called
`put_back_strays`. Inside `tidy`, the event goes through a new private `append_about(&self,
task_id: &TaskId, agent_id: Option<String>, session_id: Option<String>, body: EventBody)`, which
`append` now calls with the request's ids. `puts_back_what_tools_changed_outside_the_tasks_paths`
(`transitions.rs`) keeps passing unchanged.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `puts_back_what_tools_changed_before_the_work_session` (ignored "needs the git program", built as
  `runs_the_implement_session_in_the_worktree_with_the_sandbox` is, with `CountingSandboxFactory`
  and `implement_finishes_ctv_1()`) — the task `in_progress` with its worktree, whose allowed paths
  do not cover `README.md`: `README.md` is changed in the worktree and `stray.txt` written there
  untracked; after one tick, `worktree.tidied` records `put_back: ["README.md"]` and
  `removed: ["stray.txt"]` with no agent id, its sequence number is below the implement session's
  `session.started`, and `README.md` reads as committed. Then, on a second harness, a merge left in
  progress in the worktree (a branch that changes `README.md` against the task branch's own change
  to it, and also adds a line to a tracked `NOTICE` outside the allowed paths, merged with
  `git merge` in the worktree so that it stops on the conflict): the tick records no
  `worktree.tidied`, `README.md` still holds the conflict markers, and `NOTICE` still holds the
  merged line (staged, so the tidy would otherwise have put it back). Fails first: no
  `worktree.tidied` before the session.

- [x] `fix(runtime): put back what tools changed before each work session too`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the git tests of rules.rs and transitions.rs run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
