# Governor flexibility, plan 1: the governor

Status: ready
Branch: `fix/governor-flexibility`
Spec and decisions: see `README.md`; ADR 0079 decisions 1, 3, 4, 5, 6, 9, 10, 11, 12

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/`.

## Decisions

- Decision 1's merge runs in `rules.rs` `in_progress`, after `resume(...)` and before `tidy(...)`, only when the worktree directory exists. A conflict runs `git merge --abort`; nothing is recorded. The implement prompt gets one sentence: "The branch is behind <into> and merging it conflicts in <paths>; work from the branch as it is." Chose a silent merge commit over an event: the commit is in git, and no projection needs it.
- Decision 3: `is_to_be_judged` returns false when the last move into `refining` has actor `human` and no `session.started { purpose: refine }` for the task follows it. The refine prompt gains the line "The owner sent this back to its plan:" followed by `untrusted_block("owner", <the move's reason>)`, after the blocker lines.
- Decision 4: `check_contract_write` lets `ContractWriteActor::ProductManager` write `dependencies` and `parent` when status is `Draft | Refining | Ready` and the contract is not locked; outcome `Allowed`. `parent` stays in `FIELDS_FIXED_AT_CREATION` for every other actor and status. `write_contract` refuses a `parent` that is not an existing task of kind `epic` with `Refusal::Invalid("parent: <id> is not an epic")`.
- Decision 5 is a rule, `dependency_cancelled`, in `rules.rs`, run under `TickRules::All` before rule 8 (assignment): each `ready` task with a dependency whose status is `cancelled` is moved by `governor_moves` to `refining` with `TransitionAsk { reason: Some("<dep> was cancelled") }`. The table row is `Ready → Refining`, actor Governor, gate `DependencyCancelled`. The existing move line posts the channel message.
- Decision 6: the task-scope session count in `store/src/projections.rs` counts `DISTINCT session_id` only for purposes `refine`, `explore`, `implement`, `verify`; the other scopes are unchanged. The `budget` rule skips rows in `ready`, `assigned`, `blocked`. `escalation_detail` for a budget or sessions escalation is `"spent <n> of <m> sessions"` or `"spent $<x.xx> of $<y.yy>"`, from the same `BudgetState` the rule read; the rule passes it as the ask's words.
- Decision 9: `evaluate_command` splits as now; a segment whose first bare word is `git` (or a wrapper followed by `git`) passes when the next bare word is in `READ_ONLY_GIT`, otherwise `GitViaExec` as now.
- Decision 10: `human.rs::post_message` sets `mentions` to `[<the Product Manager's id>]` (in solo, the one agent's) when `mentions_in` finds none. The text is unchanged.
- Decision 11: after posting, for each task in `blocked` whose `waiting_on_human` is set, whose assignee is mentioned and whose id appears in the text (`\bCTV-<n>\b` as written), `post_message` calls `resolve(tools, task, return_from_block(blocked_from), text, None, false)`. A refusal of that resolve is ignored (the post stands).
- Decision 12: `roles/roles/product_manager/system.md` and `skills/writing-task-contracts/SKILL.md` drop "request `ready`"; they say "When the contract is complete, end the session: the governor checks readiness and moves it." The shared closing text for owner-facing words (`prompt.rs`) gains: "Never name a requirement or criterion by its id (R3, C4) to the owner; say what it checks."

## File map

```
runtime/src/orchestrator/rules.rs        modifies: in_progress merge (T1), dependency_cancelled rule (T3), budget skips (T5); tests
store/src/git.rs                         modifies: merge_or_abort (T1)
runtime/src/orchestrator/messages.rs     modifies: implement_message behind-line (T1), refine_message owner words (T4)
core/src/governor/gates.rs               modifies: PM writes dependencies, parent (T2); tests
runtime/src/tools/contracts.rs           modifies: parent must be an epic (T2); tests
core/src/governor/transition_table.rs    modifies: Ready → Refining by governor (T3)
core/src/governor/*.rs (gate enum)       modifies: DependencyCancelled (T3)
docs/schemas/event.schema.json           modifies: gateId "dependency_cancelled" (T3)
runtime/src/orchestrator/requests.rs     modifies: is_to_be_judged (T4); tests
store/src/projections.rs                 modifies: work-session count (T5); tests
runtime/src/transitions.rs               modifies: escalation_detail figures (T5); tests
core/src/governor/permissions.rs         modifies: READ_ONLY_GIT (T6); tests
runtime/src/orchestrator/human.rs        modifies: post_message (T7); tests
roles/roles/product_manager/system.md, skills/writing-task-contracts/SKILL.md, runtime/src/prompt.rs   modifies (T8)
```

## Interfaces

Consumes (on `main`): `merge_keeping_conflicts`, `fetch_fast_forward`, `commit_count` (`store/src/git.rs`); `IntegrationTarget`; `governor_moves`, `resume`, `Waiting` (rules.rs); `return_from_block` (gates.rs); `resolve`, `written` (human.rs); `untrusted_block` (messages.rs).

Produces:

```rust
// store/src/git.rs
pub enum MergeOutcome { UpToDate, Merged, Conflicted { paths: Vec<String> } }
pub fn merge_or_abort(&self, worktree: &Path, from: &str, message: &str) -> Result<MergeOutcome, GitError>;
// core/src/governor/permissions.rs
pub const READ_ONLY_GIT: [&str; 8] = ["status", "diff", "log", "show", "ls-files", "grep", "rev-parse", "blame"];
```

## Tasks

### Task 1: a task's branch is brought up to date before each build session

Files: `store/src/git.rs`, `runtime/src/orchestrator/rules.rs`, `runtime/src/orchestrator/messages.rs`
Tests (rules.rs `mod tests`, `#[ignore = "needs the git program: cargo xtask check --integration"]` like their neighbours):
- `brings_a_task_up_to_date_with_main_before_its_session` — a task in progress whose worktree exists; main gains a commit touching another file; after one tick the implement session ran and the worktree's HEAD has main's commit as an ancestor, with a merge commit whose subject is `chore: bring up to date with main`.
- `leaves_a_branch_that_conflicts_with_main_and_says_so` — main and the task branch change the same line; after one tick the worktree has no merge in progress, its HEAD is unchanged, and the implement prompt contains "merging it conflicts in".

- [x] `fix(runtime): a task's branch is brought up to date before each build session`

### Task 2: the Product Manager repoints a task that has not started

Files: `core/src/governor/gates.rs`, `runtime/src/tools/contracts.rs`
Tests:
- `lets_the_product_manager_repoint_a_task_that_has_not_started` (gates.rs) — ProductManager writing `["dependencies"]` and `["parent"]` on a `ready`, unlocked task is `Allowed`; the same on `assigned` is `ContractFrozen`; a Scrum Master writing `parent` on `ready` is refused with `CreationFields`.
- `refuses_a_parent_that_is_not_an_epic` (contracts.rs) — a PM refine session's `catervas_write_contract` with `parent` naming a `task` is refused with "is not an epic"; naming an epic, the contract file's parent changes.

- [x] `feat(core): the Product Manager may repoint a task that has not started`

### Task 3: a cancelled dependency sends its dependents back to the Product Manager

Files: `docs/schemas/event.schema.json`, core gate enum, `core/src/governor/transition_table.rs`, `runtime/src/orchestrator/rules.rs`
Tests:
- `sends_a_task_whose_dependency_was_cancelled_back_to_refining` (rules.rs) — CTV-2 `ready` depends on CTV-1 `cancelled`; after one tick CTV-2 is `refining`, its transition has actor `governor`, gate `dependency_cancelled`, and the channel has a system line containing "CTV-1 was cancelled".

- [x] `feat(runtime): a cancelled dependency sends its dependents back to the Product Manager`

### Task 4: a send-back to refining reaches the Product Manager

Files: `runtime/src/orchestrator/requests.rs`, `runtime/src/orchestrator/messages.rs`
Tests (requests.rs):
- `refines_a_task_the_owner_sent_back_before_judging_it` — an epic's child, blocked, moved to refining by the human with reason "Drop the env check"; after one tick a `refine` session started for the PM (no governor move to `ready`), and its initial prompt contains "Drop the env check". A second tick after that session ended with no contract write judges it (the governor asks `ready`).

- [x] `fix(runtime): a send-back to refining reaches the Product Manager with the owner's words`

### Task 5: the session budget counts work

Files: `store/src/projections.rs`, `runtime/src/orchestrator/rules.rs`, `runtime/src/transitions.rs`
Tests:
- `counts_only_work_sessions_toward_a_task` (projections.rs) — a task with cost records from sessions of purposes triage, plan, conversation, refine, implement, verify (one each) reports 3 sessions and the sum of all six costs.
- `leaves_a_ready_task_with_spent_sessions_alone` (rules.rs) — a `ready` task whose work sessions equal `max_sessions` is not escalated after a tick; the same task `in_progress` is.
- `says_what_a_spent_task_spent` (rules.rs) — the channel line of a sessions escalation ends with "spent 2 of 2 sessions", and contains no `governor_escalation`.

- [x] `fix(runtime): the session budget counts work, and says what was spent`

### Task 6: read-only git runs in exec

Files: `core/src/governor/permissions.rs`
Tests:
- `lets_read_only_git_through_and_refuses_the_rest` — `git ls-files src`, `git grep -n KEY`, `cd x && git log --oneline` pass; `git commit -m x`, `git ls-files && git push`, `env A=1 git reset --hard` are `GitViaExec`.

- [x] `feat(core): read-only git is not refused in exec`

### Task 7: the owner's channel message is answered, and can answer a block

Files: `runtime/src/orchestrator/human.rs`
Tests:
- `gives_an_unaddressed_message_to_the_product_manager` — the human posts "Recreate CTV-11"; the `message.posted` event's `mentions` is `["mira"]` (the team's PM).
- `answers_a_block_from_the_channel` — CTV-3 `blocked`, waiting on the human, assignee `theo`; the human posts "@theo For CTV-3: use the text below"; CTV-3 moves out of `blocked` to `in_progress`, and an `escalation.resolved` with that message is recorded. The same post naming no task id leaves CTV-3 blocked.

- [x] `feat(runtime): the owner's channel message reaches the Product Manager and can answer a block`

### Task 8: the Product Manager does not ask for ready, and nobody says "C3" to the owner

Files: `roles/roles/product_manager/system.md`, `roles/roles/product_manager/skills/writing-task-contracts/SKILL.md`, `runtime/src/prompt.rs`
Tests:
- `the_product_manager_is_not_told_to_ask_for_ready` (roles crate, beside its existing prompt tests) — neither file contains "Request `ready`" nor "goes to `ready`"; both contain "the governor checks readiness".
- `tells_every_session_to_name_checks_in_plain_words` (prompt.rs) — the closing text of `implement`, `verify` and `refine` contains "Never name a requirement or criterion by its id".

- [x] `docs(roles): the Product Manager leaves readiness to the governor, and checks are named in plain words`
