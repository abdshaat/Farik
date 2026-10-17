# Standalone: task ids stay inside Catervas

Status: done
Branch: `feat/internal-task-ids` (work outside a phase, one pull request to `main`)
Decision: ADR 0066 (`docs/decisions/0066-task-ids-stay-inside-catervas.md`)
Depends on: `feat/catervafication-work` (ADRs 0061 to 0063, SPEC 0.117), then `feat/parallel-agents`
(ADR 0064, SPEC 0.118), both landed or rebased under this branch
Readiness confirmed by: the planner, 2026-10-17, against the three rules (ADR 0065: no readiness
review; every decision made, no ambiguity, no forward dependency)

## Before the first task

1. Rebase this branch onto `feat/parallel-agents`, itself on `feat/catervafication-work`. ADR 0066
   and these plans are the only commit of this branch.
2. `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.
3. Run `cargo xtask check` once for a clean baseline.

The plans name files and symbols as they are on `feat/catervafication-work`. Where a line has
moved, find the symbol by name. ADR 0064's plans rewrite parts of
`crates/runtime/src/orchestrator/rules.rs` (`unavailable`, `in_scope`, the budget rules), `session.rs`,
`orchestrator.rs`, `orchestrator/fixtures.rs` and SPEC 5.14's "One task integrates at a time"
paragraph. These plans touch only `assigned` and `start_worktree` in `rules.rs`, and other
paragraphs of 5.14. They need nothing ADR 0064 adds. If ADR 0064 renamed `assigned` or
`start_worktree`, find the rule-7 code that calls `create_worktree`.

If a symbol a plan names is not there, or does something other than the plan says, stop and send
the task back to planning. Do not decide it.

## Order

Each plan is copied from `docs/plans/step-template.md`. Each one uses only what the plans before it
committed. Every task is one commit and leaves `cargo xtask check` passing (it runs `pnpm check`
too).

| # | File | What it does |
|---|---|---|
| 1 | `01-names.md` | Core: `ChangeType`, `change_type`, `branch_slug`, `free_branch`, `task_branch` reading the recorded `branch`; the contract's `branch` field, owned by the store; `forge_text` (`pull_request_title`, `without_task_ids`, `task_id_in`) |
| 2 | `02-task-branches.md` | A task's branch is chosen and recorded at its start; the start merge's message; a hunt's bug is a fix; the human's save keeps `branch`; the pull request's title and body carry no id (the end-to-end test through integrate with a fake `gh`) |
| 3 | `03-commit-messages.md` | `catervas_git_commit` refuses a message naming a task id, and its description asks for Conventional Commits |
| 4 | `04-folder-changes.md` | A folder change's branch `docs/<slug>`, recorded in its event; its pull request `Docs: Update <names>` |
| 5 | `05-files-page-and-task-page.md` | The Files page names who made a change from a pull request number; `task.diff` answers its branch and the task page shows it |
| 6 | `06-record.md` | `docs/SPEC.md` revision 0.119 and the plans' ticks (hard rule 8) |

## Wire and file changes, all in one place

| Where | Change | Plan |
|---|---|---|
| `task-contract.schema.json` | `branch`, optional string, Catervas's own | 1 |
| `event.schema.json` | `folderDocWrittenBody.branch`, `folderDocEditedBody.branch`, `folderDocApprovedBody.branch`, optional | 4 |
| `rpc.schema.json` | `taskDiffResult.branch`, optional string | 5 |
| `catervas_git_commit` | new refusal `commit_message_names_a_task` | 3 |

## The tests, one per major functionality (ADR 0060)

| Functionality | Test | Plan |
|---|---|---|
| The branch name rule (type, slug, suffix, the old rule) | `names_a_task_branch_from_its_type_and_title` (core) | 1 |
| What leaves for the forge names no task | `forge_text_names_no_task` (core) | 1 |
| A task's branch is chosen once at its start and recorded | `starts_an_assigned_task_in_its_worktree` (changed) | 2 |
| The human's save never clears the recorded branch | `a_save_keeps_the_recorded_branch` | 2 |
| A task's pull request, branch and commits carry no task id | `a_tasks_pull_request_carries_no_task_id` (integrate, fake forge) | 2 |
| An agent's commit names no task | `refuses_a_commit_message_naming_a_task` | 3 |
| A folder change's branch and pull request carry no number | `opens_a_pull_request_for_a_folder_change` (changed) | 4 |
| The Files page reads a pull request's merge | `names_who_made_a_change_merged_by_pull_request` | 5 |
| The task page shows the recorded branch | the task page test of the Changes tab (changed) | 5 |

## Not in these plans

- File names and file contents that hold an id: a plan's path `docs/catervas/architecture/plans/CTV-<n>.md`,
  `.catervas/contracts/CTV-<n>.yaml` when the owner commits it, and an agent's document that names a
  task. ADR 0066's Consequences say why.
- Renaming branches of tasks started before this change.
- More change types than Feat, Fix and Docs.
- The founder's live run. It runs once the branch has landed: a task taken to its pull request on
  GitHub from the web app.

Never edit this repository's `.catervas/`.
