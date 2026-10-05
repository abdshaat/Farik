# Phase 7, step 09c: Finance tasks in the private folder

Status: ready
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.2, 5.3, 5.4, 5.6, 5.14, 6.6, 8.2, 8.5, 8.6
Depends on: steps 09 and 09b of this phase (the role; the folder line, `farik_read_sheet`, `farik_write_sheet`); phase 6 (merged in #19)
Readiness confirmed by: a fresh Opus session, 2026-10-05 (one round, against `docs/standards/workflow.md` stage 2), with steps 09 and 09b; folded below

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 09 (see step 09's header). Every exception here is keyed by a role's private folder, `private_folder(role)` (step 09b), so that step 10b adds the Procurement Specialist's with one arm (ADR 0039, the project plan's revision 33); the code map of 2026-10-05 located each exception, and its paths are cited below.

## Goal

A finance task runs where the books are: its session works in `.farik/local/finance/`, not a git worktree, with no branch, no commit and nothing to integrate; it is ready with `artifact`, `review` and `human` criteria; it reaches `verifying` when the workbooks it names exist; its reviewer is given each changed workbook beside the copy taken when the task was assigned; once accepted it is finished, and a task that depends on it may start; and only one piece of finance work touches the folder at a time. Every other session is still refused the folder. Out of scope: the receipts sweep (phase 13); a `finance` cost purpose (F17 splits it out with the sweep).

## Decisions

- **One mapping:** step 09b's `private_folder(role)`. Every rule below asks it of the contract's `assignee_role` (or the session's agent's role); none names finance.
- **Where its sessions run** (map §1). In rule 6's `in_progress()` (`orchestrator/rules.rs:1124`) and in `verify.rs`'s `review()` and `accept()` (`:409`, `:544`), a task whose assignee role has a private folder gets `cwd: root.join(folder)` and `executor: None`, skipping `sandbox_for`; the folder is made 0700 if missing before the spawn. The implement message (`messages.rs:592`) says "in your private folder, where your books are; nothing here is committed" in place of "in this worktree, on the branch". `resume()` needs no change: with no worktree it reads no commit (`rules.rs:1190`); `review()` and `accept()` already run with no executor (`verify.rs:589`).
- **What the session may reach** (map §2). The hook keeps judging paths relative to the session's working directory (`workspace_paths`, `hooks.rs:667`), so a finance session reaches its folder and nothing outside it (`path_outside_workspace`), and `.farik/local/**` does not match a path inside it. `settings_json` (`claude.rs:879`) is unchanged: Claude Code reads a bare `Read(<glob>)` rule relative to the session's working directory, so `Read(.farik/local/**)` names nothing a session in the folder can reach, and the agent reads its workbooks through `farik_read_sheet` anyway, which no deny rule governs. Rejected: `SessionSpec.private_folder` and a deny list without `.farik/local/**`, which would touch 25 struct literals in 8 files for no reachable difference. Rejected: root-relative paths for these sessions, which would need a second exception in the hook for what the working directory already confines.
- **Readiness** (map §3), in `readiness.rs`. A new rule, `PrivateFolderTask`, checked only for a contract whose `assignee_role` has a folder: every `allowed_paths` entry lies within the folder; no criterion is `command` or `test`; an `artifact` criterion has no `must_contain` (a workbook is not text); the contract has no `parent` (an epic's paths cannot name `.farik/`). `CHECKS` grows to 20 and `plain.rs` gains its sentence: "A finance task works only in the private folder: no commands, no tests, no text searched in a workbook, and no parent epic." For such a contract, `no_farik_paths` accepts entries within the folder, `document_paths_only` and `allowed_paths_within_ceiling` do not apply, and `required_criteria_present` does not require `command` or `test`.
- **Paths of a private-folder task are folder-relative.** An `artifact` criterion's path and each `workbooks` entry name a file in the folder (`books.xlsx`), by 09b's path rule in its write form (`.xlsx` only, matching 10b:20, which widens it for procurement); `PrivateFolderTask` refuses an artifact path starting `.farik/` or failing that rule. `allowed_paths` stay root-relative globs (`.farik/local/finance/**`), as every contract's are.
- **`verifying`** (map §4). `farik_request_transition`'s input gains `workbooks: Option<Vec<String>>` (1 to 20 folder-relative paths), carried as `TransitionAsk.workbooks`. `WorkState` gains `folder: Option<FolderWork>`, `FolderWork { named: Vec<String>, missing: Vec<String>, changed: Vec<String> }`, and drops `Copy` (`gates.rs:397`); it is filled by `Transitions::work` (`transitions.rs:669`) for a private-folder task from the folder and the task's `.history/<task-id>/` copy; `check_criteria_recorded` (`gates.rs:398`) skips the commit and clean-worktree checks for it and refuses with "name the workbooks you wrote" when `named` is empty and "<path> is not in your folder" for each `missing`.
- **The reviewer's artifact run** (map §4). `criteria.rs` gains `pub fn check_artifact_in(folder: &Path, criterion: &ExitCriterion, run_by: RunBy) -> CriterionOutcome`: the path is a regular file in the folder, not a link. `run_what_farik_runs` (`verify.rs:147-237`) uses it for a private-folder task's `artifact` criteria and makes no sandbox, no `integration_branch` and no `task_branch` for it.
- **The Definition of Done** (map §5). `changed` (files that differ from, are new to, or are gone from the `.history/<task-id>/` copy, `.history/` itself excluded) is the evidence's `changed_paths`, each prefixed with the folder; `no_protected_path_changed` and `no_farik_path_changed` (`done.rs:294`, `:317`) except paths within the task's own folder; `paths_within_allowed` applies unchanged.
- **The review** (map §5). For a private-folder task, `review()` gives the reviewer, in place of `git.diff`, a list: each changed file, "new", "changed" or "removed", with its size and the reminder to read it with `farik_read_sheet`, and its assignment copy with `farik_read_sheet { …, baseline: true }`. `farik_read_sheet` gains `baseline: bool`: in a session about a private-folder task, it reads `.history/<task-id>/<path>` of that task; refused elsewhere (`sheet_refused`). The reviewer's session runs in the folder (above).
- **Nothing to integrate** (map §6). A new transition effect, `nothing_to_integrate` (core `TransitionEffect::NothingToIntegrate`; the `effects` enum in `event.schema.json`), recorded by `record_move` (`transitions.rs:303`) on a private-folder task's move into `accepted`; `apply_move` (`projections.rs:701`) leaves `awaiting_integration` 0 when the effects hold it, so `dependency_states` (`transitions.rs:1063`) counts the task integrated and its dependants may be assigned (D20). The human's `integrate` on such a task is refused `nothing_to_integrate`. `farik_store::diff::diff_of` (`store/src/diff.rs:33`, called by `task.diff` and by `farik show`, `cli/show.rs:95`) returns early for it: `task.diff` answers `{ diff: "", files: [<changed, folder-relative>], added: 0, removed: 0, private_folder: true }`, and `taskDiffResult` (`rpc.schema.json:2405-2433`, `additionalProperties: false`) gains `"private_folder": { "type": "boolean" }`, so the tests that check answers against it (`daemon/gates.rs:746`, `:1564`) hold.
- **One piece of work in the folder** (map §7). `AssignmentInput` gains `private_folder_busy: bool`: another task whose assignee role has the same folder is assigned and neither `accepted` nor `cancelled` (the set `assignee_open_tasks` counts, `gates.rs:90-95`), since `rejected -> in_progress` and `blocked -> in_progress` pass no assignment gate, so a task sent back, blocked or escalated still holds the folder; `check_assignment` refuses it `private_folder_busy` whatever the WIP limit, and rule 8's `assignable` (`rules.rs:1375`) returns false when `assignment.private_folder_busy`, so no doomed assignment is planned; `has_room` is unchanged, since it never sees the task.
- **The baseline at assignment** (map §7). Rule 7's `assigned()` (`rules.rs:1238`), for a private-folder task, copies every file of the folder but `.history/` to `.history/<task-id>/` once, when that folder is absent, in place of creating a worktree, so a task sent back keeps its first baseline.
- **The web app**: a private-folder task's Changes tab, on its page and at its gate, says "This task changed the <assignee role's plain name>'s private files, which are not shown in the browser. Its reviewer read each changed file beside the copy taken when the task started.", with the changed files' names below it, so step 10b changes nothing here. No mockup: one sentence in an existing panel, in place of the diff. The `Diff` types (`TaskDetail.tsx:38`, `Gate.tsx:48`) gain `privateFolder?: boolean`; `packages/protocol-client`'s generic `toCamel` maps it, and the generated TypeScript is gitignored and regenerated by `pnpm check`.
- **The prompt.** `keeping-the-books` and `system.md` gain: "work in your private folder; ask for `verifying` naming your `workbooks`; nothing there is committed" (step 09 left this line out).

## File map

```
crates/core/src/governor/readiness.rs, plain.rs           modifies: PrivateFolderTask and the exemptions (Task 1)
crates/core/src/governor/gates.rs, transition.rs, done.rs modifies: FolderWork, private_folder_busy, NothingToIntegrate (Tasks 2, 3)
crates/runtime/src/transitions.rs                         modifies: effect_wire's arm and `folder: None` (Task 2); work(), private_folder_busy (Tasks 3, 5)
docs/schemas/event.schema.json                            modifies: nothing_to_integrate (Task 2)
crates/runtime/src/orchestrator/rules.rs                  modifies: assigned, in_progress, assignable (Tasks 3, 4)
crates/runtime/src/orchestrator/verify.rs, messages.rs    modifies: review, accept, run_what_farik_runs, the messages (Tasks 4, 5, 6)
crates/runtime/src/tools/work.rs, criteria.rs             modifies: workbooks, check_artifact_in, record_move (Task 5)
crates/store/src/projections.rs, store/src/diff.rs        modifies: apply_move, diff_of (Task 5)
crates/runtime/src/orchestrator/integrate.rs              modifies: the refusal (Task 5)
docs/schemas/rpc.schema.json                              modifies: taskDiffResult.private_folder (Task 5)
crates/runtime/src/tools/sheets.rs                        modifies: baseline (Task 6)
apps/web/src/pages/TaskDetail.tsx, Gate.tsx (+tests)      modifies (Task 7)
crates/roles/roles/finance_specialist/{system.md,skills/keeping-the-books/SKILL.md}   modifies (Task 8)
docs/SPEC.md, docs/plans/project-plan.md                  modifies (Task 9)
```

## Interfaces

Consumes: as cited above; 09b's `private_folder`, `private_path`, `read_sheet`.

Produces:

```rust
pub struct FolderWork { pub named: Vec<String>, pub missing: Vec<String>, pub changed: Vec<String> }  // governor::gates
pub struct WorkState { pub commits: u32, pub worktree_clean: bool, pub folder: Option<FolderWork> }
// ReadinessRule::PrivateFolderTask; TransitionEffect::NothingToIntegrate; AssignmentInput.private_folder_busy: bool
// RequestTransitionInput.workbooks: Option<Vec<String>>; ReadSheetInput.baseline: bool
pub fn check_artifact_in(folder: &Path, criterion: &ExitCriterion, run_by: RunBy) -> CriterionOutcome;  // runtime criteria
```

## Tasks

### Task 1: The folder, and readiness

- `a_finance_task_is_ready_in_its_folder`: `allowed_paths: [.farik/local/finance/**]`, `artifact`, `review` and `human` criteria pass every rule. RED.
- `a_finance_task_may_not_run_or_test`: a `command` criterion, a `test` criterion, an `artifact` with `must_contain`, an artifact path `.farik/local/finance/books.xlsx` or `notes.txt`, an `allowed_paths` entry outside the folder, and a `parent` each fail `PrivateFolderTask` with its reason. RED.
- `a_team_requiring_tests_still_readies_a_finance_task`, and `the_ceiling_does_not_hold_the_folder`. RED each.
- `another_roles_task_still_may_not_name_farik`: a Developer's task with `.farik/local/finance/**` fails `no_farik_paths`. RED.

- [ ] `feat(core): let a finance task be ready in its private folder`

### Task 2: `verifying`, done, and nothing to integrate (core)

- `a_finance_task_verifies_by_its_workbooks`: `folder: Some` with `named` non-empty and `missing` empty passes with no commit; empty `named` and a `missing` path each refuse with their sentences. RED.
- `its_own_folder_is_not_a_protected_change`: `changed_paths` within the folder pass `no_protected_path_changed` and `no_farik_path_changed` for a finance task and fail them for a Developer's. RED.
- `acceptance_of_a_finance_task_integrates_nothing`: the move to `accepted` carries `NothingToIntegrate`. RED.

The commit compiles across the workspace: `effect_wire` (`transitions.rs:1026-1031`) gains the new effect's arm and the `WorkState` built at `transitions.rs:688` gains `folder: None`, filled in Task 5.

- [ ] `feat(core): let a finance task verify by its workbooks and end at acceptance`

### Task 3: One at a time, and the baseline

- `a_second_finance_task_waits`: with one finance task `assigned`, `in_progress`, `verifying`, `rejected`, `blocked` or `escalated`, another is refused `private_folder_busy` under a WIP limit of two and with two Finance Specialists; rule 8 plans no such assignment; once the first is `accepted` or `cancelled`, the second is assignable. RED. (`private_folder_busy` is filled at `transitions.rs:1103`.)
- `assignment_copies_the_books_once`: assigning copies `books.xlsx` and `forecast.xlsx` to `.history/FRK-1/` and creates no worktree; after a send-back and a second assignment, the copy is the first. RED.

- [ ] `feat(runtime): give finance work its folder one task at a time`

### Task 4: Its sessions run in the folder

- `a_finance_session_runs_in_its_folder`: the implement session's `cwd` is `<root>/.farik/local/finance` (made 0700), with no executor; the review session's too. RED.
- `it_cannot_reach_out`: the hook refuses a finance session's `Read` of `../worktrees/FRK-2/x`, of the project's `farik.db` by an absolute path, and of `../../settings.json` as `path_outside_workspace`. Guard (`workspace_paths` already judges against the working directory; the landing review's mutation, running the session at the root, proves it).

- [ ] `feat(runtime): run a finance task's sessions in its private folder`

### Task 5: `verifying`, done and integration in the runtime

- `request_transition_takes_the_workbooks`: `workbooks: ["books.xlsx"]` reaches `verifying` with the file present, and is refused with it missing. RED.
- `farik_checks_a_finance_artifact_on_the_host`: an `artifact` criterion `books.xlsx` passes when it is a file in the folder and fails when missing or a link, with no sandbox and no branch read. RED.
- `an_accepted_finance_task_unblocks_its_dependants`: after acceptance `awaiting_integration` is 0 and a dependant is assignable; the human's `integrate` is `nothing_to_integrate`; `task.diff` answers `private_folder: true` with the changed files, valid against `taskDiffResult`; `farik show` prints no diff. RED.

- [ ] `feat(runtime): finish a finance task at acceptance, with nothing to integrate`

### Task 6: The reviewer's view

- `the_reviewer_is_told_what_changed`: the review message lists `books.xlsx` changed and `forecast.xlsx` new, inside the untrusted notice, and no diff. RED.
- `the_baseline_is_readable_in_the_tasks_sessions`: `farik_read_sheet { path: "books.xlsx", baseline: true }` reads `.history/FRK-1/books.xlsx` in the reviewer's session and is `sheet_refused` in a session about no finance task. RED.

- [ ] `feat(runtime): show a finance task's reviewer each changed workbook beside its baseline`

### Task 7: The web app

- `a_finance_tasks_changes_tab_says_where_its_books_are` (TaskDetail and Gate): with `privateFolder: true` the sentence above, naming "Finance Specialist", and the changed files show in place of the diff. RED.
- [ ] `feat(web): say where a finance task's changes are`

### Task 8: The prompt

- `keeping_the_books_says_where_to_work`: the role's prompt holds the folder line and names `workbooks`, and its "How a session ends" item that asks for `verifying` is replaced, not joined, by the one naming `workbooks` (step 09's landing review). RED.

- [ ] `feat(roles): tell the Finance Specialist to work in its private folder`

### Task 9: Spec and plan

`docs/SPEC.md` 5.2, 5.3, 5.4, 5.6, 5.14, 6.6 ("one piece of finance work at a time" as the open set above), 8.2, 8.5 as built; the revision line. Project plan row 09c.

- [ ] `docs(spec): record finance tasks in the private folder`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, by the founder: a Finance Specialist's task "Record and forecast the team's AI spending" runs in the folder, writes `books.xlsx` and `forecast.xlsx`, reaches `verifying` naming them, is reviewed by the Product Manager, accepted with nothing to integrate, and `git status` shows nothing new.

## Execution notes

None yet.
