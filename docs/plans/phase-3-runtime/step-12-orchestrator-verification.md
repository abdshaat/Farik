# Phase 3, step 12: Orchestrator, verification

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.4, 8.5, F6
Depends on: step 11 of this phase (`Orchestrator`, the session mechanics, `RecordedAdapter::with_tools`, `orchestrator::fixtures`), a start gate: Task 1 does not begin until step 11's last commit is on this branch; step 06 (`run_criteria`); step 05 (the tools)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (step 12: two rounds; step 13: one round after its rewrite; findings folded in)

## Goal

A task that asks for `verifying` is checked the way 5.4 says: Farik runs the contract's mechanical criteria itself, as the reviewer, in the task's sandbox; the reviewer's fresh `verify` session sees the contract, the diff, the completion note, and those results, never the assignee's transcript, and answers the `review` criteria and writes its review note; a failed criterion sends the task back to its assignee through `rejected`, a bounded number of times; a passed review goes to the Product Manager's own `verify` session, which accepts it. A blocked task past its limit escalates. Tested end to end: one task from `ready` to `accepted` with four recorded sessions. Out of scope: what happens after `accepted` (step 13); the human's acceptance of a `high` risk task (step 14, which adds `human.accepted`).

## Decisions

- `rejected` (rule 3 of step 11's order): `rejected → in_progress` asked as `Governor`; when refused (the iteration limit), `rejected → escalated` as `Governor`. Step 04's effects do the rest (the iteration, the escalation). The implement session that follows (step 11's rule 6) gets, in its first message, the reasons of the last `task.transitioned` into `rejected` and the failed criterion ids, inside `untrusted_block`, because they are an agent's words.
- `blocked` past its limit (rule 4): only when the last move into `blocked` is at least `blocked_limit_hours` before the clock's now, `blocked → escalated` asked as `Governor`, whose `BlockedAge` gate decides; a younger block has no rule.
- `verifying` (rule 5). A task with a `transition.refused` since its last move into `verifying` has no rule: a refused acceptance or rejection would otherwise be asked again on every tick, and a rejection Farik files spends no session, so no budget would stop `run_until_idle` from spinning; the refusal is on the board for the human (step 14 gives them the commands). Otherwise, in this order, each step reading the context through `Transitions::context` for `verifying → accepted` so that the orchestrator judges on exactly what the governor will (`done.results`, `done.review_note`, counted since the last move into `in_progress`, step 05):
  1. Each `command`, `test`, or `artifact` criterion with no `criterion.recorded { recorded_by: "governor" }` since the last move into `verifying` is run, one at a time, by `run_criteria` over a copy of the contract holding that criterion alone (step 06's signature takes a contract), `RunBy::Reviewer`, the base-branch check with base the integration branch and head `farik/<id>`, in the sandbox from step 11's `sandbox_for`, under `spawn_blocking`; its result is recorded as `criterion.recorded { run_by: reviewer, recorded_by: "governor" }` before the next one runs, so a run killed half way is taken up at the first criterion it did not record. `recorded_by` is `governor` because Farik ran them, as `requested_by: "governor"` names the governor's own moves; the Definition of Done reads `run_by`, so they count as the reviewer's runs, which is what 5.4 asks ("run by the reviewer, independently of the assignee"). Rejected: the reviewer's id, which would say the reviewer ran a command it never saw. `review` criteria go into the reviewer's message as rubrics; `human` criteria are nobody's to run in this phase (item 5).
  2. No review note: the reviewer's `verify` session (below), in the same tick as step 1.
  3. A review note and a failed reviewer result: Farik files `verifying → rejected` on the reviewer's behalf, as `Reviewer` with the reviewer's id, `Rejection { failed_criterion_ids, reasons: <the review note> }`, and `TransitionAsk::session_id` the session on the review note's `note.written` envelope, so that the log ties the rejection to the session whose words it carries; 5.4 says Farik files it. Rejected: another reviewer session told to reject, which spends a session to get the same words and may again not comply. The ids come from the contract and the note is non-blank, so `RejectionReasons` opens; were it refused, the refusal passes the task over (above).
  4. A review note, nothing failed, and a criterion other than a `human` one with no reviewer result (a `review` criterion the reviewer did not answer): the reviewer's session again, its message naming the criteria still unanswered; `max_sessions` bounds it (step 11).
  5. A review note and every criterion other than a `human` one passed by the reviewer: when `requires_human_acceptance(contract)` (risk `high`) or the contract has any `human` criterion, no rule until step 14 adds `human.accepted`, which is the only thing that satisfies either, so no session is spent on a request the Definition of Done must refuse; otherwise the Product Manager's `verify` session. A refused `accepted` (a path outside `allowed_paths`, a missing completion note) passes the task over (above).
- The human's acceptance and a `human` criterion are two separate facts in `done.rs`. `DoneEvidence::human_accepted` answers `HumanAccepted` (risk `high`, and every epic). A `human` criterion needs a passing `CriterionResult { run_by: Human }` (`HumanCriterionAccepted`), and nothing on the wire can carry one, because `criterion.recorded`'s `run_by` is `assignee` or `reviewer` only. Step 14's choice, written on its interface line in the project plan with this plan: `Transitions::context` reads `human.accepted { subject: result }` twice, as `done.human_accepted` and as one passing `CriterionResult { run_by: Human, evidence: "human.accepted at seq <n>" }` for each `human` criterion of the contract. So one acceptance answers them all. Chose it over adding `human` to `criterion.recorded`'s `run_by`, because 5.4 item 1 says a `human` criterion is satisfied "only by an explicit human acceptance event", and one event keeps one path to it. The ceiling is that the human cannot pass one `human` criterion and fail another. A per-criterion answer is a `run_by: human` added when a contract needs one.
- The reviewer's session: `purpose: Verify`, the contract's reviewer, `cwd` the task's worktree so that it reads the real code, `builtin_tools` from `allowed_builtins` of the read tier alone, and no executor in its registration, so `farik_exec` is `ToolError::Failed` (the test runner takes the executor from `tool_context` at each call, step 11, so it sees the same none). Chose the worktree over a detached read-only worktree (step 06's `create_detached_worktree`): the hazard is the reviewer changing the work under review, and the Farik git tools act on the task's worktree whatever the session's `cwd`, so a second worktree would not close it; the two rules below do, and the read-only built-ins keep Claude Code's own writers out of the session (the hook still judges by the agent's tiers, so the `--tools` allowlist is what stops them, a residual 5.4 records). Its first message, built in `messages.rs`: the task's id and title, the rubric of each `review` criterion, each Farik result (id, passed, evidence), the completion note, and the diff (`Git::diff` from the integration branch to `farik/<id>`, cut at 64 KiB), each inside `untrusted_block`; nothing from any implement session.
- The Product Manager's session: `purpose: Verify`, `cwd` the worktree, read-tier built-ins, no executor; its first message holds the review note and the reviewer's results inside `untrusted_block` and tells it the review passed every criterion and to request `accepted` with `farik_request_transition`. These are Farik's words in the first message rather than `human_message`, which step 10 keeps for the human's own words and caps at 16 KiB, too small for a diff. Step 10's `This session` text for `verify` covers both sessions (step 10's plan, amended with this one).
- Two tool rules close the reviewer's reach into the work (5.1: nobody grades their own homework, and nobody rewrites what they grade): `farik_record_criterion_result` refuses the reviewer a `command`, `test`, or `artifact` criterion (`criterion_run_by_farik: <id> is a <method> criterion, which Farik runs for the reviewer`), so a reviewer cannot record a pass over Farik's failure, the latest result per runner being what the gate reads; `farik_git_commit` and `farik_git_push` refuse anyone but the task's assignee (`not_the_named_agent`). Both in `tools/refusal.rs` with the rest.
- `farik_record_criterion_result` also refuses a `human` criterion from any agent (`criterion_answered_by_the_human: <id> is a human criterion, which only the human answers`). Without this, a reviewer's failed H1 would send the task to `rejected` by rule 3 over a criterion only the human answers, and a passing H1 would be a result the Definition of Done ignores. The reviewer therefore records `review` criteria alone. Chose refusing every agent over refusing the reviewer alone: the assignee's result for a `human` criterion counts for nothing either, because `check_criteria_recorded` skips `human` criteria, and one rule with no role in it is the smaller one.
- `review.recorded { reviewer, criteria_run, passed }` (8.5 already names it), once per verification: appended when a reviewer's session ends, no `review.recorded` exists since the last move into `verifying`, and every criterion other than a `human` one has a reviewer result; `criteria_run` the number of criteria with a reviewer result, `passed` true exactly when each one's latest passed. It is about one contract; `reviewer` is its attribution. Nothing reads it in phase 3, not even the metrics (step 16 counts first pass from the moves into `verifying` and `rejected`); it stays an audit summary for the human reading the log, which is why it is a summary rather than a gate (amended 2026-09-22 by the step 16 plan).
- `OrchestratorError` gains `Criterion(CriterionError)`.
- Changed 2026-09-23 in execution (Task 1): the contract schema holds an exit criterion's id to `C<n>`, so the `review` criterion the tests call R1 and the `human` criterion they call H1 are `C2` (or `C3` beside two commands) in the tests. The tools' fixture's C1 is a `test` criterion, so `refuses_the_reviewer_a_criterion_farik_runs` refuses the reviewer both it and a `command` C2. The two existing tool tests that had the reviewer record C1 now have it answer a `review` C2. The reviewer is refused `farik_git_push` as well as `farik_git_commit`, which the decision names and the test holds; `not_the_named_agent` is a tool refusal of its own, with the task's assignee in its words.
- Changed 2026-09-23 in execution (Task 2): decided where the plan was silent: the rejection an implement session is told of is the last move into `rejected`'s, given while the task's last move into `in_progress` came from `rejected` (so a resumed session of the same iteration is told it too), inside `untrusted_block("rejection", .., 16 KiB)` holding the failed criterion ids and the reasons. A rejected task the governor would neither return nor escalate is reported as `Acted` with its reasons, as rule 7 reports a refused start; the two gates are complementary, so one of them opens. The tools' fixture gained `moved_at` and `record_at`, and the orchestrator's `blocked_hours_ago` and `rejected`, to age a block and to file a rejection.
- Changed 2026-09-23 in execution (Task 3): decided where the plan was silent: the governor's `criterion.recorded` names no agent and no session on its envelope, and `review.recorded` names the reviewer and the session whose end completed the review. A tick that ran criteria but could start no session (a spent budget, or the human's acceptance awaited) reports `Acted` with "ran <n> of its criteria as its reviewer"; a `verifying` task whose reviewer is not an active agent, or on a team with no active Product Manager when it is that session's turn, has no rule, as rules 6 and 7 pass over an inactive assignee. `SessionAsk` gained `read_only` (the read tier's built-ins whatever the agent's tiers) and `SessionEnd` the session's id. The reviewer's message puts the title, Farik's results, the rubrics, the completion note (16 KiB), and the diff (64 KiB) each in its own `untrusted_block` (`title`, `results`, `rubric`, `completion_note`, `diff`), and a second asking opens with `Still unanswered: <ids>`; the Product Manager's holds `review_note` and `results`. A panic inside the criteria's `spawn_blocking` is resumed on the tick rather than made an error. `runs_the_criteria_as_the_reviewer_before_the_review` sees the registration's executor through `orchestrator::fixtures::ExecutorWitness`, an adapter that reads `tool_context` as each session starts; `records_the_review_once_per_verification` gets its second reviewer session from a first one that writes no note (`reads_a_file`), the only way a complete review is asked again. 5.4's paragraph is marked "added in 0.8", and revision 0.8's paragraph names it, as step 11's 5.5 sentence is.
- Changed 2026-09-23 in execution (Task 4): `takes_one_task_from_ready_to_accepted` is in `crates/runtime/src/orchestrator.rs`'s tests rather than `crates/runtime/tests/one_task.rs`, because step 11 made `orchestrator::fixtures` (and `tools::fixtures` beneath it) test-only inside the crate, and an integration test under `tests/` cannot reach them; rebuilding the harness and the tool runner there from public items is the second copy step 11 rejected. It is `#[ignore]`d for git as the plan says. "The adapter has no transcript left" needed `RecordedAdapter::transcripts_left`, added with its own test in `recorded.rs`.
- Changed 2026-09-23 by the landing review of step 11: the reviewer's and the Product Manager's `verify` sessions run through step 11's `run_session`, so a failure after either starts ends it as step 11's plan now says (aborted, its zero cost and `session.ended { reason: error }` recorded as far as they can be, its registration ended, then the error); nothing in `verify.rs` changed for it.

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
crates/runtime/tests/one_task.rs                  creates: the end-to-end test, ignored (needs git); landed in orchestrator.rs's tests (Task 4's note)
docs/SPEC.md                                      modifies: 5.4 (Farik runs the mechanical criteria as the reviewer, recorded by the governor; the reviewer may not record them or commit; no agent records a `human` criterion; Farik files a failed review's rejection with the review note; a task with a `human` criterion waits for the human; the read-only built-ins residual)
docs/plans/project-plan.md                        modifies: step 12's interface line, as landed (step 14's line took the human's acceptance with this plan)
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

### Task 1: the event, and the tools' rules

Files: the schema, `event.rs`, `event/fixtures.rs`, `tools/work.rs`, `tools/git.rs`, `tools/refusal.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers `review.recorded`.
- `refuses_the_reviewer_a_criterion_farik_runs` — `dev-b`, FRK-1's reviewer, recording C1 (`command`) is `Refused` starting `criterion_run_by_farik`, and nothing is appended; recording a `review` criterion R1 is accepted with `run_by: reviewer`; `dev-a` recording C1 is accepted with `run_by: assignee`.
- `refuses_anyone_a_human_criterion` — FRK-1 with a `human` criterion H1: `dev-b` recording H1, failed, is `Refused` starting `criterion_answered_by_the_human`, and `dev-a` recording it passed is refused the same way; nothing is appended.
- `refuses_a_commit_from_anyone_but_the_assignee` — `dev-b`'s `farik_git_commit` on FRK-1 is `Refused` starting `not_the_named_agent` and the branch has no new commit; `dev-a`'s commits.

- [x] `feat(runtime): keep a task's reviewer out of the work it reviews`

### Task 2: rejected and blocked

Files: `rules.rs`, `messages.rs`

- `returns_a_rejected_task_to_its_assignee` — FRK-1 `rejected` at iteration 0 with reasons "C1: done.txt missing": one tick moves it to `in_progress` with iteration 1 and `requested_by: governor`; the next tick's implement spec's first message contains "C1: done.txt missing".
- `escalates_a_task_rejected_too_often` — at iteration equal to `max_iterations`: `escalated`, `escalation.raised { reason: iterations }`.
- `escalates_a_block_past_its_limit` — blocked 25 hours before the clock with the default 24: `escalated` with `reason: blocker_age`; blocked 1 hour before: the tick is `Idle`.
- `picks_a_rejected_task_before_a_ready_one` — FRK-1 `ready`, FRK-2 `rejected`: the tick acts on FRK-2.

- [x] `feat(runtime): return rejected tasks and escalate old blocks`

### Task 3: verifying

Files: `verify.rs`, `rules.rs`, `messages.rs`, `orchestrator.rs`, the transcripts, `docs/SPEC.md` (5.4)

- `runs_the_criteria_as_the_reviewer_before_the_review` — FRK-1 `verifying` with `done.txt` committed: the log holds `criterion.recorded { criterion_id: C1, passed: true, run_by: reviewer, recorded_by: governor }` before `dev-b`'s `session.started { purpose: verify }`; the spec's `cwd` is the worktree, `builtin_tools` equals `allowed_builtins({Read})`, and its registration had no executor.
- `shows_the_reviewer_the_diff_and_the_note_and_not_the_transcript` — after the implement session of step 11, the reviewer spec's first message contains `done.txt` from the diff inside `<untrusted source="diff">`, the completion note's text, and C1's evidence, and none of the implement transcript's model text.
- `does_not_run_the_criteria_twice_in_one_verification` — a second reviewer session in the same `verifying` (after `review_answers_nothing`) is preceded by no new governor `criterion.recorded`.
- `runs_only_the_criteria_farik_has_not_run` — commands C1 and C2 with a governor result for C1 already recorded since the move into `verifying`: the tick records one governor `criterion.recorded`, for C2.
- `rejects_a_failed_review_with_the_reviewers_note` — `done.txt` absent: C1 recorded failed, `review_writes_note` runs, and the next tick records `task.transitioned` `verifying → rejected` with `actor: reviewer`, `requested_by: dev-b`, `rejection.failed_criterion_ids == ["C1"]`, `reasons` equal to the note's text, and on its envelope the session id of the review note's `note.written`; no Product Manager session started.
- `asks_the_reviewer_again_for_an_unanswered_criterion` — with a `review` criterion R1 and `review_answers_nothing`: the next session is `dev-b`'s `verify` again and its first message names R1.
- `hands_a_passed_review_to_the_product_manager` — after `review_writes_note` with C1 passed: `review.recorded { reviewer: dev-b, criteria_run: 1, passed: true }`, then the next session is `pm`'s `verify`, whose first message holds the review note and the word `accepted`.
- `leaves_a_high_risk_task_for_the_human` — the same with `risk: high`: after the review, the tick is `Idle` and no `pm` session starts.
- `leaves_a_task_with_a_human_criterion_for_the_human` — C1 and a `human` criterion H1: after `review_writes_note`, `review.recorded { criteria_run: 1, passed: true }`, the tick is `Idle`, and neither `dev-b` nor `pm` gets another session.
- `passes_over_a_task_whose_request_was_refused` — FRK-1 `verifying`, reviewed and passed, with its completion note missing: `accept_frk_1` is refused (`transition.refused` recorded) and the next tick is `Idle` with no session started.
- `records_the_review_once_per_verification` — a second reviewer session in the same `verifying` appends no second `review.recorded`.

- [x] `feat(runtime): verify tasks in fresh sessions and hand them on`

### Task 4: one task end to end

Files: `crates/runtime/tests/one_task.rs` (landed in `crates/runtime/src/orchestrator.rs`, see Task 4's note), `crates/runtime/src/recorded.rs`, `docs/plans/project-plan.md`

- `takes_one_task_from_ready_to_accepted` — step 11's fixture and the transcripts `plan_assigns_frk_1`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`: after `run_until_idle`, FRK-1 is `accepted`; `farik/FRK-1` has one commit, adding `done.txt`; the log holds, in this order among themselves, four `session.started` (`plan`/`pm`, `implement`/`dev-a`, `verify`/`dev-b`, `verify`/`pm`), the transitions `ready → assigned → in_progress → verifying → accepted`, two `criterion.recorded` for C1 (`run_by: assignee`, then `run_by: reviewer` with `recorded_by: governor`), a `note.written { kind: completion }` and a `note.written { kind: review }`, one `review.recorded { passed: true }`, and four `cost.recorded` each carrying FRK-1; `adapter.started().len() == 4` and the adapter has no transcript left.

- [x] `test(runtime): take one task from ready to accepted end to end`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
