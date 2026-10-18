# Standalone (open issues), plan 5: a pull request that stops merging goes back to its author

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2, 5.14, 8.5 (recorded in plan 8's revision 0.122)
Depends on: plans 1 (the `accepted -> assigned` row and its gate) and 4 (`Git::is_merging`, the
tidy that leaves a merge alone) of this folder, committed on this branch
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

When an accepted task's pull request stops merging because other work landed first (PR #22 after
#21, both changing `NoteInput.tsx`), Catervas notices on rule 2's poll and brings the branch up to
date itself: a clean merge is pushed to the same pull request; a conflicted one goes back to the
task's assignee with the conflicted files listed, who resolves them, is reviewed again, and is
pushed to the same pull request. A folder change that stops merging is escalated. A delivery counts
a task once. Out of scope: a pull request the owner resolves on GitHub meanwhile (the next
acceptance pushes over nothing: `open_pull_request` finds it merged or open and rule 2 follows it).

## Decisions

Every decision is ADR 0069's (decision 4); this section pins how.

- **What the forge says.** `pull_request_state` asks `gh pr view <url> --json
  state,mergeCommit,mergeable`. `PullRequestState::Open` carries `mergeable: Mergeable`, read from
  `"MERGEABLE"`, `"CONFLICTING"` and `"UNKNOWN"`; any other value, or none, is `Unknown`.
- **Rule 2, a task's pull request `Open { Conflicting }`** (`through_the_forge`, which gains
  `team: &Team` from `integrate_locked`), under the integration lock it already holds:
  1. `target.git.fetch_fast_forward("origin", into)`; a failure escalates as a merged one's does.
     Then, when `target.git.commit_count(&branch, into)? == 0` (the branch already holds all of
     `into`, so GitHub's answer is stale, as it is for a while after a push), answer
     `AwaitingForge` and do nothing else: this is what keeps a refresh from repeating on every
     tick.
  2. `target.git.add_worktree(&worktree, &branch)` at `.catervas/local/worktrees/<id>` (rule 1 removed
     it at acceptance; an existing one is reused).
  3. `target.git.merge_keeping_conflicts(&worktree, into, &format!("chore: bring up to date with
     {into}"))`.
  4. `Merged { sha }`: `target.git.push("origin", &format!("refs/heads/{branch}"))`,
     `remove_worktree`, append `pull_request.refreshed { url, sha }`, answer `AwaitingForge`. A
     failed push escalates (reason `integration`, "pushing {branch} after bringing it up to date
     failed: …").
  5. `Conflicts(paths)`: append `pull_request.conflicted { url, into, paths }`, then ask the governor
     `accepted -> assigned` as `Governor` with `TransitionAsk { pull_request_conflicts: Some(paths) }`
     and answer the new `IntegrationOutcome::SentBack { paths }`. The worktree keeps the merge with
     its markers. A refusal removes the worktree and escalates with the refusal's words.
  `Open { Mergeable }` and `Open { Unknown }` answer `AwaitingForge`, as `Open` did.
- **Leaving `accepted` clears `awaiting_integration`.** The projection's `CASE` (`projections.rs`,
  "Nothing leaves `accepted`") gains `WHEN status = 'accepted' THEN 0` before its `ELSE`, so a task
  sent back is not listed on Today as waiting for a merge; its next acceptance sets it again. The
  comment says the one row out of `accepted` (ADR 0069).
- **After the move**, rule 7 finds the worktree (`assigned` reuses one that is there) and moves the
  task to `in_progress` as its assignee; rule 6 starts the implement session. The task keeps its
  assignee, reviewer and branch. Its iteration is not counted: this is not a rejection.
- **The assignee is told.** `Resume` gains `conflicts: Option<(String, Vec<String>)>`, the `into`
  and `paths` of the last `pull_request.conflicted`, shown while no move into `verifying` follows
  it. `implement_message` adds: "Your pull request no longer merges into {into}: other work changed
  the same files. Catervas merged {into} into your branch in this worktree and left the conflicts in:
  {paths, one per line, untrusted block `conflicts`}. Resolve each one, keeping what both sides
  meant; run the checks; commit with `catervas_git_commit`, which finishes the merge; then ask for
  `verifying`. If a conflicted file is outside your allowed paths, block the task with
  `catervas_declare_blocked`."
- **No markers get committed.** `catervas_git_commit` (`tools/git.rs::commit`) refuses, before
  committing, any path it is given whose file in the worktree has a line starting with `<<<<<<< ` or
  `>>>>>>> `: `conflict_markers: {path} still holds conflict markers; resolve them first`
  (`Refusal::ConflictMarkers { path }` in `tools/refusal.rs`). `Git::commit` already commits every
  staged change, so during a merge its commit is the merge commit.
- **Re-push for free.** Once accepted again, rule 2 finds no `pull_request.opened` since the new
  move into `accepted` and calls `open`, which pushes the branch and finds the open pull request
  for it with `gh pr list --head`, recording `pull_request.opened` with the same url.
- **A folder change `Open { Conflicting }`** is escalated once, with "the pull request {url} no
  longer merges into {into}": `escalate` is called only when no `folder_change.escalated` for the
  change follows its last `folder_change.opened`.
- **A delivery counts a task once.** `due_delivery` (`deliveries.rs`) leaves out a task with a move
  into `accepted` at or before the delivery's `since_seq`.
- **Two event kinds**, `<entity>.<past_tense_verb>`: `pull_request.conflicted` (`url`, `into`,
  `paths`: strings, `paths` 1 to 200 items) and `pull_request.refreshed` (`url`, `sha`), about the
  task with no agent, in `docs/schemas/event.schema.json`, `crates/protocol` (`EventKind`, bodies,
  fixtures) and ignored by the projections.

## File map

```
crates/runtime/src/forge.rs                    modifies: Mergeable, Open { mergeable }, the --json fields (Task 12)
crates/runtime/src/forge.rs                    tests:    reads_a_pull_requests_state amended (Task 12)
crates/store/src/git.rs                        modifies: Git::add_worktree, Git::merge_keeping_conflicts (Task 13)
crates/store/tests/git.rs                      tests:    merges_into_a_kept_branch_and_leaves_its_conflicts (Task 13)
docs/schemas/event.schema.json                 modifies: the two kinds and bodies (Task 14)
crates/protocol/src/event.rs, lib.rs, event/fixtures.rs   modifies: the two kinds (Task 14)
crates/store/src/projections.rs                modifies: ignores them; a move out of accepted clears awaiting_integration (Task 14)
crates/runtime/src/transitions.rs              modifies: TransitionAsk.pull_request_conflicts into the context (Task 14)
crates/runtime/src/orchestrator.rs             modifies: IntegrationOutcome::SentBack (Task 14)
crates/runtime/src/orchestrator/integrate.rs   modifies: through_the_forge refreshes or sends back (Task 14)
crates/runtime/src/orchestrator/human.rs       modifies: `integrate`'s words for SentBack (Task 14)
crates/runtime/src/orchestrator/integrate.rs   tests:    sends_back_a_pull_request_that_stopped_merging, refreshes_a_pull_request_that_merges_again (Task 14)
crates/runtime/src/orchestrator/rules.rs       modifies: resume reads the conflicts (Task 15)
crates/runtime/src/orchestrator/messages.rs    modifies: Resume.conflicts, implement_message (Task 15)
crates/runtime/src/orchestrator/messages.rs    tests:    tells_the_assignee_which_files_conflict (Task 15)
crates/runtime/src/tools/git.rs, tools/refusal.rs  modifies: the conflict-marker refusal (Task 15)
crates/runtime/src/tools/git.rs                tests:    refuses_to_commit_a_file_with_conflict_markers (Task 15)
crates/runtime/src/deliveries.rs               modifies: a task counts once (Task 16)
crates/runtime/src/deliveries.rs               tests:    counts_a_task_accepted_again_once (Task 16)
```

## Interfaces

Consumes (on this branch): plan 1's `accepted -> assigned` row, `GateId::PullRequestConflicted`,
`TransitionContext.pull_request_conflicts`; plan 4's `Git::is_merging` and the tidy that skips a
merge; `Forge`, `PullRequestState`, `FakeGh` (`fixtures.rs`); `Git::fetch_fast_forward`, `push`,
`remove_worktree`, `merge_in`, `MergeOutcome`; `through_the_forge`, `integrate_locked`,
`escalate`, `opened_url`, `IntegrationTarget`, `a_pull_request` (`integrate.rs` tests);
`Transitions::request`, `TransitionAsk`; `resume`, `Resume`, `implement_message`; `commit`
(`tools/git.rs`); `due_delivery` (`deliveries.rs`).

Produces:

```rust
// crates/runtime/src/forge.rs
pub enum Mergeable { Mergeable, Conflicting, Unknown }
pub enum PullRequestState { Open { mergeable: Mergeable }, Merged { sha: String }, Closed }
// crates/store/src/git.rs
impl Git {
    /// Adds a worktree at `path` on the existing local branch `branch`.
    pub fn add_worktree(&self, path: &Path, branch: &str) -> Result<(), GitError>;
    /// As `merge_in`, but a conflicted merge is left in progress in the worktree, its markers in the files.
    pub fn merge_keeping_conflicts(&self, worktree: &Path, from: &str, message: &str) -> Result<MergeOutcome, GitError>;
}
// crates/runtime/src/transitions.rs
pub struct TransitionAsk { /* … */ pub pull_request_conflicts: Option<Vec<String>> }
// crates/runtime/src/orchestrator.rs
pub enum IntegrationOutcome { /* … */ SentBack { paths: Vec<String> } }
// crates/runtime/src/orchestrator/messages.rs
pub(super) struct Resume { /* … */ pub(super) conflicts: Option<(String, Vec<String>)> }
```

## Tasks

### Task 12: the forge reads whether a pull request merges

Files: modified `crates/runtime/src/forge.rs`, and each `PullRequestState::Open` match the
compiler names (`Open { .. }`, unchanged behaviour); tested by `forge.rs`'s tests

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `reads_a_pull_requests_state` (amended) — `FakeGh` answering
  `{"state":"OPEN","mergeCommit":null,"mergeable":"CONFLICTING"}` gives `Open { Conflicting }`,
  `"MERGEABLE"` gives `Open { Mergeable }`, `"UNKNOWN"` and a missing field give `Open { Unknown }`;
  the call's arguments end with `--json state,mergeCommit,mergeable`. Fails first on the arguments.

- [x] `feat(runtime): read whether a pull request still merges`

### Task 13: the store merges into a kept branch and keeps the conflicts

Files: modified `crates/store/src/git.rs`; tested by `crates/store/tests/git.rs`

Tests:

- `merges_into_a_kept_branch_and_leaves_its_conflicts` (ignored "needs the git program") — branch
  `fix/a` changes line 1 of `f.txt`, its worktree is removed, and `main` changes line 1 differently:
  `add_worktree(path, "fix/a")` makes a worktree on `fix/a`; `merge_keeping_conflicts(path, "main",
  …)` answers `Conflicts(["f.txt"])`, `is_merging(path)` is `true`, and `f.txt` holds `<<<<<<< `;
  on a second branch whose change does not touch `main`'s, it answers `Merged { sha }` with `sha`
  the worktree's `HEAD` and `is_merging` `false`. Fails first: the methods do not exist.

- [x] `feat(store): merge into a kept branch's worktree and leave the conflicts there`

### Task 14: rule 2 refreshes or sends back

Files: as the file map says for Task 14; tested by `integrate.rs`'s tests
Consumes: Tasks 12 and 13; plan 1's row

`Transitions::context` sets `pull_request_conflicts: ask.pull_request_conflicts.clone().unwrap_or_default()`.
`human.rs`'s `integrate` says "{id} no longer merges: it went back to its assignee to resolve
{paths}" for `SentBack`.

Tests:

- `sends_back_a_pull_request_that_stopped_merging` (ignored "needs the git program", from
  `a_pull_request` with `with_origin()`) — the integration branch gains a commit changing the line
  the task's branch changed, and `FakeGh` answers `pr view` with `mergeable: "CONFLICTING"`; after a
  tick: `pull_request.conflicted` names the file, the task is `assigned` with the move's gate
  `pull_request_conflicted` and actor `governor`, and its worktree holds the markers with a merge in
  progress; its board row's `awaiting_integration` is `false`; after the next tick it is
  `in_progress`. Fails first: nothing is recorded.
- `refreshes_a_pull_request_that_merges_again` (same set-up, the integration branch's commit
  touching another file) — after a tick: `pull_request.refreshed`
  holds a sha that `origin`'s task branch now points at, whose parents are the old tip and the
  integration branch; the task stays `accepted` and its worktree is gone. `FakeGh` keeps
  answering `CONFLICTING` (a stale answer): a further tick appends no second
  `pull_request.refreshed` and makes no worktree.

- [x] `feat(runtime): bring a pull request that stopped merging up to date or send it back`

### Task 15: the assignee is told, and cannot commit markers

Files: modified `rules.rs` (`resume`), `messages.rs`, `tools/git.rs`, `tools/refusal.rs`; tested
by `messages.rs`'s and `tools/git.rs`'s tests
Consumes: Task 14's event

Tests:

- `tells_the_assignee_which_files_conflict` — `implement_message` with `conflicts:
  Some(("catervas/next", ["components/NoteInput.tsx"]))` contains "no longer merges into
  catervas/next", the path inside an untrusted block named `conflicts`, and
  `catervas_declare_blocked`; with `None` it is the message it was.
- `refuses_to_commit_a_file_with_conflict_markers` (ignored "needs the git program", built as
  `commits_through_the_tool_on_the_tasks_worktree`) — a file holding `<<<<<<< HEAD` is refused
  `conflict_markers` and no commit is made; once the markers are gone the same call commits.

- [x] `feat(runtime): the assignee resolves a pull request's conflicts and commits no markers`

### Task 16: a delivery counts a task once

Files: modified `crates/runtime/src/deliveries.rs`; tested by its tests

Tests:

- `counts_a_task_accepted_again_once` (built as its neighbours build a log) — a task accepted
  before a closed delivery and accepted again after it does not count towards the next delivery;
  a task accepted only after it does.

- [ ] `fix(runtime): a delivery counts a task once however often it is accepted`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the git tests of integrate.rs, tools/git.rs and the store run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
