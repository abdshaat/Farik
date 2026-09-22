# Phase 3, step 12: Orchestrator, verification

Status: draft
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.4, 8.5, F6
Depends on: step 11 of this phase (`Orchestrator`, the session mechanics, `RecordedAdapter::with_tools`, `orchestrator::fixtures`), a start gate: Task 1 does not begin until step 11's last commit is on this branch; step 06 (`run_criteria`); step 05 (the tools)
Readiness confirmed by: pending

## Goal

A task that asks for `verifying` is checked the way 5.4 says: Farik runs the contract's mechanical criteria itself, as the reviewer, in the task's sandbox; the reviewer's fresh `verify` session sees the contract, the diff, the completion note, and those results, never the assignee's transcript, and answers the `review` criteria and writes its review note; a failed criterion sends the task back to its assignee through `rejected`, a bounded number of times; a passed review goes to the Product Manager's own `verify` session, which accepts it. A blocked task past its limit escalates. Tested end to end: one task from `ready` to `accepted` with four recorded sessions. Out of scope: what happens after `accepted` (step 13); the human's acceptance of a `high` risk task (step 14, which adds `human.accepted`).

## Decisions

- `rejected` (rule 3 of step 11's order): `rejected → in_progress` asked as `Governor`; when refused (the iteration limit), `rejected → escalated` as `Governor`. Step 04's effects do the rest (the iteration, the escalation). The implement session that follows (step 11's rule 6) gets, in its first message, the reasons of the last `task.transitioned` into `rejected` and the failed criterion ids, inside `untrusted_block`, because they are an agent's words.
- `blocked` past its limit (rule 4): only when the last move into `blocked` is at least `blocked_limit_hours` before the clock's now, `blocked → escalated` asked as `Governor`, whose `BlockedAge` gate decides; a younger block has no rule.
- `verifying` (rule 5), in this order, each step reading the context through `Transitions::context` for `verifying → accepted` so that the orchestrator judges on exactly what the governor will (`done.results`, `done.review_note`, counted since the last move into `in_progress`, step 05):
  1. If no `criterion.recorded` with `recorded_by: "governor"` exists since the last move into `verifying`, Farik runs `run_criteria(contract, sandbox, RunBy::Reviewer, Some(new_tests))` in the task's sandbox (step 11's map; the base-branch check with base the integration branch and head `farik/<id>`) and records each `Result` as `criterion.recorded { run_by: reviewer, recorded_by: "governor" }`. `recorded_by` is `governor` because Farik ran them, as `requested_by: "governor"` names the governor's own moves; the Definition of Done reads `run_by`, so they count as the reviewer's runs, which is what 5.4 asks ("run by the reviewer, independently of the assignee"). Rejected: the reviewer's id, which would say the reviewer ran a command it never saw. `NeedsReview` and `NeedsHuman` are not recorded; the rubric goes into the reviewer's message.
  2. No review note: the reviewer's `verify` session (below), in the same tick as step 1.
  3. A review note and a failed reviewer result: Farik asks `verifying → rejected` as `Reviewer` with the reviewer's id and `Rejection { failed_criterion_ids, reasons: <the review note> }`, because the reviewer found the failure and the gate's reasons are its words. Rejected: another reviewer session told to reject, which spends a session to get the same words and may again not comply. The ids come from the contract and the note is non-blank, so `RejectionReasons` opens; a refusal is returned as the tick's report and the task is left.
  4. A review note, nothing failed, and a criterion with no reviewer result (a `review` criterion the reviewer did not answer): the reviewer's session again, its message naming the criteria still unanswered; `max_sessions` bounds it (step 11).
  5. A review note and every criterion passed by the reviewer: when `requires_human_acceptance(contract)` (risk `high`), no rule until step 14 adds the human's acceptance, so no session is spent on a request the Definition of Done must refuse; otherwise the Product Manager's `verify` session. A refusal of its `accepted` (a path outside `allowed_paths`, a missing completion note) leaves the task `verifying` and the next tick starts it again, bounded by `max_sessions`.
- The reviewer's session: `purpose: Verify`, the contract's reviewer, `cwd` the task's worktree so that it reads the real code, `builtin_tools` from `allowed_builtins` of the read tier alone, and no executor in its registration, so `farik_exec` is `ToolError::Failed`. Chose the worktree over a detached read-only worktree (step 06's `create_detached_worktree`): the hazard is the reviewer changing the work under review, and the Farik git tools act on the task's worktree whatever the session's `cwd`, so a second worktree would not close it; the two rules below do, and the read-only built-ins keep Claude Code's own writers out of the session (the hook still judges by the agent's tiers, so the `--tools` allowlist is what stops them, a residual 5.4 records). Its first message, built in `messages.rs`: the task's id and title, the rubric of each `review` criterion, each Farik result (id, passed, evidence), the completion note, and the diff (`Git::diff` from the integration branch to `farik/<id>`, cut at 64 KiB), each inside `untrusted_block`; nothing from any implement session.
- The Product Manager's session: `purpose: Verify`, `cwd` the worktree, read-tier built-ins, no executor; its first message holds the review note and the reviewer's results inside `untrusted_block` and tells it the review passed every criterion and to request `accepted` with `farik_request_transition`. These are Farik's words in the first message rather than `human_message`, which step 10 keeps for the human's own words and caps at 16 KiB, too small for a diff. Step 10's `This session` text for `verify` covers both sessions (step 10's plan, amended with this one).
- Two tool rules close the reviewer's reach into the work (5.1: nobody grades their own homework, and nobody rewrites what they grade): `farik_record_criterion_result` refuses the reviewer a `command`, `test`, or `artifact` criterion (`criterion_run_by_farik: <id> is a <method> criterion, which Farik runs for the reviewer`), so a reviewer cannot record a pass over Farik's failure, the latest result per runner being what the gate reads; `farik_git_commit` and `farik_git_push` refuse anyone but the task's assignee (`not_the_named_agent`). Both in `tools/refusal.rs` with the rest.
- `review.recorded { reviewer, criteria_run, passed }` (8.5 already names it) is appended when a reviewer's session ends and the context holds a review note: `criteria_run` the number of the contract's criteria with a reviewer result, `passed` true exactly when every criterion's latest reviewer result passed. It is about one contract; `reviewer` is its attribution. Nothing reads it in phase 3 but the metrics (step 16), which is why it is a summary rather than a gate.
- `OrchestratorError` gains `Criterion(CriterionError)`.

## File map

```
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: review.recorded
crates/runtime/src/orchestrator/rules.rs          modifies: the rejected, blocked, and verifying rules; tests
crates/runtime/src/orchestrator/verify.rs         creates: running the criteria for the reviewer, the verifying decision, review.recorded
crates/runtime/src/orchestrator/messages.rs       modifies: the reviewer's, the Product Manager's, and the rejected task's first messages
crates/runtime/src/orchestrator.rs                modifies: OrchestratorError::Criterion
crates/runtime/src/tools/work.rs, tools/git.rs, tools/refusal.rs   modifies: the two tool rules; tests
crates/runtime/src/recorded/transcripts/review_writes_note.jsonl, review_answers_nothing.jsonl, accept_frk_1.jsonl   creates
crates/runtime/src/recorded/fixtures.rs           modifies: the three transcripts
crates/runtime/tests/one_task.rs                  creates: the end-to-end test, ignored (needs git)
docs/SPEC.md                                      modifies: 5.4 (Farik runs the mechanical criteria as the reviewer, recorded by the governor; the reviewer may not record them or commit; a failed review is rejected with the review note; the read-only built-ins residual)
docs/plans/project-plan.md                        modifies: step 12's interface line
```

## Interfaces

Consumes: `Orchestrator`, `OrchestratorError`, the session mechanics, `orchestrator::fixtures`, `ToolRunner` (step 11); `run_criteria`, `NewTestsInput`, `CriterionOutcome`, `CriterionError` (step 06); `untrusted_block` (step 10); `Transitions::context`, `Rejection`, `requires_human_acceptance` (`farik-core`), `Git::diff` (main).

Produces:

```rust
// event body, wire
ReviewRecordedBody { reviewer: String, criteria_run: u32, passed: bool }
// farik-runtime
pub enum OrchestratorError { .., Criterion(CriterionError) }
```

## Tasks

The harness is step 11's. `review_writes_note` calls `farik_write_note { kind: review }` and nothing else; `review_answers_nothing` the same without recording R1; `accept_frk_1` calls `farik_request_transition { to: accepted }`. Each has one `result` line.

### Task 1: the event, and the tools' two rules

Files: the schema, `event.rs`, `event/fixtures.rs`, `tools/work.rs`, `tools/git.rs`, `tools/refusal.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers `review.recorded`.
- `refuses_the_reviewer_a_criterion_farik_runs` — `dev-b`, FRK-1's reviewer, recording C1 (`command`) is `Refused` starting `criterion_run_by_farik`, and nothing is appended; recording a `review` criterion R1 is accepted with `run_by: reviewer`; `dev-a` recording C1 is accepted with `run_by: assignee`.
- `refuses_a_commit_from_anyone_but_the_assignee` — `dev-b`'s `farik_git_commit` on FRK-1 is `Refused` starting `not_the_named_agent` and the branch has no new commit; `dev-a`'s commits.

- [ ] `feat(runtime): keep a task's reviewer out of the work it reviews`

### Task 2: rejected and blocked

Files: `rules.rs`, `messages.rs`

- `returns_a_rejected_task_to_its_assignee` — FRK-1 `rejected` at iteration 0 with reasons "C1: done.txt missing": one tick moves it to `in_progress` with iteration 1 and `requested_by: governor`; the next tick's implement spec's first message contains "C1: done.txt missing".
- `escalates_a_task_rejected_too_often` — at iteration equal to `max_iterations`: `escalated`, `escalation.raised { reason: iterations }`.
- `escalates_a_block_past_its_limit` — blocked 25 hours before the clock with the default 24: `escalated` with `reason: blocker_age`; blocked 1 hour before: the tick is `Idle`.
- `picks_a_rejected_task_before_a_ready_one` — FRK-1 `ready`, FRK-2 `rejected`: the tick acts on FRK-2.

- [ ] `feat(runtime): return rejected tasks and escalate old blocks`

### Task 3: verifying

Files: `verify.rs`, `rules.rs`, `messages.rs`, `orchestrator.rs`, the transcripts, `docs/SPEC.md` (5.4)

- `runs_the_criteria_as_the_reviewer_before_the_review` — FRK-1 `verifying` with `done.txt` committed: the log holds `criterion.recorded { criterion_id: C1, passed: true, run_by: reviewer, recorded_by: governor }` before `dev-b`'s `session.started { purpose: verify }`; the spec's `cwd` is the worktree, `builtin_tools` equals `allowed_builtins({Read})`, and its registration had no executor.
- `shows_the_reviewer_the_diff_and_the_note_and_not_the_transcript` — after the implement session of step 11, the reviewer spec's first message contains `done.txt` from the diff inside `<untrusted source="diff">`, the completion note's text, and C1's evidence, and none of the implement transcript's model text.
- `does_not_run_the_criteria_twice_in_one_verification` — a second reviewer session in the same `verifying` (after `review_answers_nothing`) is preceded by no new governor `criterion.recorded`.
- `rejects_a_failed_review_with_the_reviewers_note` — `done.txt` absent: C1 recorded failed, `review_writes_note` runs, and the next tick records `task.transitioned` `verifying → rejected` with `actor: reviewer`, `requested_by: dev-b`, `rejection.failed_criterion_ids == ["C1"]`, and `reasons` equal to the note's text; no Product Manager session started.
- `asks_the_reviewer_again_for_an_unanswered_criterion` — with a `review` criterion R1 and `review_answers_nothing`: the next session is `dev-b`'s `verify` again and its first message names R1.
- `hands_a_passed_review_to_the_product_manager` — after `review_writes_note` with C1 passed: `review.recorded { reviewer: dev-b, criteria_run: 1, passed: true }`, then the next session is `pm`'s `verify`, whose first message holds the review note and the word `accepted`.
- `leaves_a_high_risk_task_for_the_human` — the same with `risk: high`: after the review, the tick is `Idle` and no `pm` session starts.

- [ ] `feat(runtime): verify tasks in fresh sessions and hand them on`

### Task 4: one task end to end

Files: `crates/runtime/tests/one_task.rs`, `docs/plans/project-plan.md`

- `takes_one_task_from_ready_to_accepted` — step 11's fixture and the transcripts `plan_assigns_frk_1`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`: after `run_until_idle`, FRK-1 is `accepted`; `farik/FRK-1` has one commit, adding `done.txt`; the log holds, in this order among themselves, four `session.started` (`plan`/`pm`, `implement`/`dev-a`, `verify`/`dev-b`, `verify`/`pm`), the transitions `ready → assigned → in_progress → verifying → accepted`, two `criterion.recorded` for C1 (`run_by: assignee`, then `run_by: reviewer` with `recorded_by: governor`), a `note.written { kind: completion }` and a `note.written { kind: review }`, one `review.recorded { passed: true }`, and four `cost.recorded` each carrying FRK-1; the adapter has no transcript left.

- [ ] `test(runtime): take one task from ready to accepted end to end`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
