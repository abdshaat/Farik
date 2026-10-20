# Governor flexibility, plan 5: after the live run

Status: ready
Branch: `fix/governor-flexibility`, after plan 4
Spec and decisions: ADR 0079, "Amended after the live run"; spec 0.134 (amended in place, the revision is not merged yet)

The build from plans 1 to 4 ran on the founder's project on 2026-10-20 from 06:05 UTC. Four things it showed are fixed here.

## Decisions

- R1 (web): on the help page a note is required only for a choice that answers (`to` is the block's return target, or `refining`); "Give it more room", "Give it more tries" and cancelling need none. With no note, those send their label as today.
- R2 (runtime): `bring_up_to_date` also runs before a `verify` session, at the same place in the verify rule (after any tidy, before `run_session`), on the task's worktree. A conflict is aborted and the verify prompt gets the same sentence as the implement prompt.
- R3 (web): the help page's heading ("<name> needs your help"), the explainer's label ("<name>, your <role>, explains what happened") and "What <name> tried" name the agent who raised the block, the same agent the note's label names (plan 3, Task 16).
- R4 (runtime): `human_message` shows the owner's answers to this task's blocks and escalations (`escalation.resolved` with `resolved_by: human`) from the last time the task entered `in_progress` from `assigned` or `ready` (the start of its current try), not only those after the last session started; the newest three, oldest first, under "The owner's answers on this task:". Answers after the last session are shown as now.

## Tasks

### Task 21: answers need words, room does not

Files: `apps/web/src/pages/HelpNeeded.tsx`, `HelpNeeded.test.tsx`
- `gives_room_without_a_note` — with an escalation offering "Give it more room", the button is enabled with an empty note and sends `extra_room: true`; "Answer and carry on" on a block stays disabled until a note is typed.
- [ ] `fix(web): giving room needs no note`

### Task 22: the help page names who asked, everywhere

Files: `apps/web/src/pages/HelpNeeded.tsx`, `HelpNeeded.test.tsx`
- `names_the_reviewer_in_the_heading` — the reviewer-block history of Task 16 shows "Ada needs your help", "Ada, your Architect, explains what happened" and "What Ada tried", and no "Theo".
- [ ] `fix(web): the help page names the agent who raised the block`

### Task 23: a review starts from an up-to-date branch

Files: `crates/runtime/src/orchestrator/rules.rs`, `messages.rs` (verify prompt)
- `brings_a_task_up_to_date_with_main_before_its_review` (ignored, git) — a task in verifying whose worktree lacks main's newest commit; after one tick the verify session ran and main's commit is an ancestor of the worktree's HEAD.
- [ ] `fix(runtime): a review starts from a branch brought up to date with main`

### Task 24: the owner's answers stay with the task

Files: `crates/runtime/src/orchestrator/messages.rs`
- `keeps_the_owners_answers_for_the_whole_try` — history: task enters in_progress; owner answers a block ("plan text: X"); a session starts and ends; the task is rejected and back in progress; a new session's `human_message` contains "plan text: X" under "The owner's answers on this task:".
- [ ] `fix(runtime): the owner's answers reach every session of the task's try`

### Task 25: the spec and the ADR say it

Files: `docs/SPEC.md` (5.7 and the 0.134 revision line), `docs/decisions/0079-the-governor-gets-out-of-the-way.md` is already amended by the planner.
- [ ] `docs(spec): room needs no note, reviews start up to date, answers stay with the task`
