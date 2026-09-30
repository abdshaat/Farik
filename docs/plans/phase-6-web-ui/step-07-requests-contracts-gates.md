# Phase 6, step 07: The gates' runtime

Status: done (landed and landing-reviewed 2026-09-29)
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 5.1, 5.2, 5.4 (what a human gate shows), 5.5, 5.7, 5.11, 5.13, 5.16, 8.5, F4, F14
Depends on: steps 01 to 06 of this phase
Readiness confirmed by: fresh-session reviewer, 2026-09-29, round one: not ready, with six unmade decisions, one of them the founder's (made through the approved mockups, now recorded in ADR 0024). The step was split in two on its advice: the pages are step 08. Round two, limited to the six, found them settled (ready with findings, folded in: the extra-tries arithmetic, `budget_state`'s real signature, `human_approves` exactly, the integration help case, and `review.recorded`'s description).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The runtime gives step 08's pages what spec 5.4 and 5.16 ask of a human gate. It:
- requires the plain-language summaries each gate leads with;
- lets the human send a plan or a finished result back (ADR 0024);
- lets the human grant more tries (ADR 0024);
- lets agents offer choices with a question;
- files a request from plain words;
- checks an unsaved plan as the user types, and saves the human's edit;
- answers the queries the pages read: what waits on the human, a contract, a task's history, diff and checks, the questions, each agent's activity, and what moved.

`farik send-back` is the command line's form of sending back. Out of scope: every page (step 08).

## Decisions

- **The summary** (spec 5.4).
  - `task-contract.schema.json` gains `summary`: a string of 20 to 600 characters, optional in the schema. The bound lives in the schema, so `validate_contract` holds it.
  - It is a content field: `FIELDS_OF_THE_CONTENT` becomes `[&str; 15]`.
  - The name also appears on `task.created` and `contract.written`, where it is the log's one-line summary. Those event fields keep their name, and a comment in `contract.rs` says the two differ.
  - A new readiness rule, `SummaryPresent`, binds the contracts a human gate shows: every epic, and every task whose approval the team's `human_accepts_contracts` policy asks of the human. That is a `high` risk task under `high_risk`, or any task under `all`. It fails with "the plan has no summary for the user; write two or three plain sentences they can decide on". The Scrum Master's breakdown tasks are bound only when that policy asks.
  - `ReadinessContext` gains `human_approves: bool`, computed by `readiness_context` as exactly `policy.human_accepts_contracts == all || requires_human_acceptance(contract)`, the expression `transition.rs` already uses.
  - A completion or review note must open with a summary: its first paragraph, up to the first blank line, 20 to 600 characters. `farik_write_note` refuses one that does not, with `summary_missing: open the note with two or three plain sentences for the user, then a blank line`. Every progress note, whether an agent's or Farik's, is exempt.
- **Sending back** (ADR 0024).
  - Core gains `GateId::HumanRejection` for the new transition-table row `verifying → rejected` by the human. It opens when `TransitionContext.result_awaits_human` is true, and, for a task, when `TransitionContext.review_passed` is true. An epic needs only the first.
  - The human's message is required. `failed_criteria` is required on the wire and may be `[]`, because the command schema's `oneOf` would otherwise match `human_accept` too; the page sends `failed_criteria: []`.
  - `Transitions::context` computes the two flags with the same predicate `human.rs` uses for acceptance: `verifying` and (an epic, or risk `high`, or a `human` criterion). `review_passed` means the latest `review.recorded` since `verifying` passed.
  - The command is `human_send_back { task_id, subject: AcceptSubject, message, failed_criteria: [string] }`, reusing `AcceptSubject` (`contract | result`).
    - For `contract`, while approval is awaited, it runs the existing escalation resolve to `refining` with the message.
    - For `result`, it moves `verifying → rejected`, requested by the human with the message as the reason. The rejection counts as a try, as the governor's `rejected → in_progress` row already makes it.
  - Refusals:
    - `not_awaiting_approval`, `not_waiting_for_the_human`, as today;
    - `review_first: the reviewer has not finished; send back once the review is in`.
  - The command line: `farik send-back <task> <message> [--criterion <id>]…`, routed like `farik accept`.
- **More tries** (ADR 0024).
  - `escalation_resolve`'s command body and the `escalation.resolved` event body both gain `extra_tries: integer 1..5`, optional.
  - `TransitionContext` gains `extra_iterations: u32`, the sum of the task's `extra_tries` over its `escalation.resolved` events, which `Transitions::context` reads from the history it already loads.
  - Core's `IterationBelowLimit` and `IterationLimitReached` compare against `max_iterations + extra_iterations`.
  - The sessions check allows `max_sessions + 4 × extra_iterations`. `budget_state(projections, team, role, task, session, now)` in `runtime/cost.rs` derives the task's extra tries itself, from its `escalation.resolved` events, so every caller (`transitions.rs`, `rules.rs`, the session start) gets it without a new parameter.
  - "N more tries" means N more attempts, counting the one the resume starts. A resolve that carries `extra_tries` also increments `iteration`, so the resumed attempt is counted. The limit stays `iteration < max_iterations + extra_iterations`, which remains correct for repeated grants. A resolve without `extra_tries` does not change `iteration`, as today.
  - `extra_tries` is accepted only when the escalation's reason is `iterations`. Otherwise the refusal is `extra_tries_only_for_tries`.
- **Help choices, per escalation reason.** These are the exact `escalation_resolve` bodies step 08's page sends.
  - `iterations`: "Give 2 more tries" `{ to: in_progress, extra_tries: 2 }`; "Ask <PM> to change the plan" `{ to: refining }`; "Cancel the task" `{ to: cancelled }`.
  - `budget` and `sessions`: "Change the plan" `{ to: refining }`; "Cancel the task" `{ to: cancelled }`. Resuming is not offered, because spec 5.7 raises these again. The page explains that the plan's budget must change.
  - `integration` is not an escalation the human resolves. It is a `waiting.list` kind on an accepted task, and its page offers one action, "Add to project" (`task_integrate`). An accepted task is terminal, so there is nothing to cancel.
  - `blocker_age`, `permission`, `readiness_failures`, `explicit_request`: "Carry on" `{ to: <the status the task held before the escalation>, from its last task.transitioned into escalated }`; "Change the plan" `{ to: refining }`; "Cancel the task".
  - `approval` is not a help case: the plan page handles it.
  - "Pause the task" is dropped, because leaving `blocked` needs a resolution page this phase does not have.
  - The query `escalation.choices { task_id }` answers `{ choices: [{ label, body }] }` (`[]` unless the task is `escalated`), so the rule lives in one place.
- **Question choices.** `farik_ask_human` (`tools/work.rs`, `AskHumanInput`) gains optional `choices: [{ label (1..80), hint? (0..160) }]`, at most 4. `question.asked` gains the same `choices`. "Let <agent> decide" answers the exact text "Decide as you think best, and say what you chose."
- **Filing** (`request.file { text }`).
  - `request_from_brief`, `placeholder_budget_usd`, and `PLACEHOLDER` move from `crates/cli/src/contract_new.rs` to `farik_store::requests`, and the CLI calls them there.
  - `request.file` derives the title from the first line (cut at 80 characters on a word boundary) and the intent from the whole text. The budget is `placeholder_budget_usd(team)`.
  - A text under 20 characters is refused with "say a little more: at least 20 characters". `request_from_brief`'s own messages are unchanged; the method checks the length first.
  - It files through `file_request` with `created_by: human`, and answers `{ task_id }`.
- **Checking and saving a plan.**
  - `Transitions::context` is split: `Transitions::readiness_context(&self, team, contract) -> Result<ReadinessContext, TransitionError>` builds from a given contract, because it needs the board and the budgets the transitions object holds, and `context` shares its body.
  - `contract.check { task_id, contract }` answers `{ failures: [{ rule, message, plain }], total }`, where `total` is the number of checks run (the `ReadinessRule` variants evaluated for this contract, plus one for the schema):
    - schema errors come first, as `rule: "schema"`, with the schema's message as `plain`;
    - then `evaluate_readiness`'s failures, with `plain` from `farik_core::governor::plain::plain_readiness(rule) -> &'static str`, which covers every `ReadinessRule` variant.
  - This is the phase decision's `contract.validate`, renamed because it validates nothing it saves. The project plan records the rename.
  - `contract.save { task_id, contract }` writes content fields as the human, through `check_contract_write`. When the task is frozen (awaiting approval), `ReturnsToRefining` is accepted for the human. The task moves to `refining`, and the judgment and approval are asked again. The answer says so: `{ saved: true, back_to_refining: true }`. A write refused otherwise answers `-32005` with the gate's sentence.
- **Answers are wrapped**, as `tasks.list` is: `{ waiting }`, `{ questions }`, `{ choices }`, `{ activity }`, `{ moved }`, `{ checks }`, `{ events }`, `{ contract }`. `sprint.current` answers the bare object or `null`.
- **Queries.** `farik_store` gains `waiting`, `diff`, and `activity`. `integration_branch` moves from `farik_runtime::transitions` to `farik_store::git` (it needs only `Team` and `Git`), so that `diff` has no runtime dependency.
  - **`waiting.list {}`** answers `[{ task_id, kind: approval | acceptance | question | help | integration, agent_id, title, line }]`.
    - Lines: approval, "<PM name> wrote a plan for you to approve"; acceptance, "<assignee> finished it and <reviewer> reviewed it"; question, the question's text; help, "<assignee> needs your help: <reason in words>"; integration, "Accepted, waiting for you to add it".
    - Acceptances now include a `verifying` task with a `human` criterion.
    - The CLI's `farik waiting` builds its command hints from `kind` and `task_id`, with its output unchanged.
  - **`contract.get { task_id }`** answers the contract.
  - **`task.history { task_id }`** answers its events.
  - **`task.diff { task_id }`** answers `{ diff, files, added, removed }`. For an epic it joins its tasks' integrated diffs in id order, each under a `# FRK-n` line, which is new.
  - **`task.checks { task_id }`** answers `[{ criterion_id, text, passed, evidence }]` from `criterion.recorded` since the task last entered `verifying`. For a contract awaiting approval it answers the readiness results.
  - **`questions.list { task_id? }`** answers `[{ question_id (the seq of its question.asked), task_id, agent_id, text, choices: [{ label, hint? }], answer: string | null }]`, oldest first.
  - **`task.tries { task_id }`** answers `{ try: iteration + 1, of: max_iterations + 1 + extra_iterations }` (changed by step 08's landing review: it answered `{ used: iteration, allowed: max_iterations + extra_iterations }`, which read "try 0 of 3" on a first try).
  - **`sprint.current {}`** answers `{ sprint_id, done, total } | null`, from `Projections::open_sprint` and the sprint's tasks (done = accepted or cancelled).
  - **`escalation.choices`** is described above.
  - **`team.activity {}`** answers `[{ agent_id, state, line, task_id?, until? }]`.
    - Lines by session purpose: triage, "Sizing a request"; refine, "Writing the plan for <title>"; plan, "Planning <title>"; implement, "Building <title>"; verify, "Reviewing <title>"; ceremony, "Running the <thread>"; conversation, "Answering in the channel".
    - Resting: "Resting until <HH:MM>. It reached its usage limit."
    - Waiting: "Waiting on you: <waiting line>".
    - Paused: "Paused".
    - Idle: "Nothing to do right now".
  - **`moved.since { since }`** answers `[{ at, line }]`. Lines:
    - a transition, "<who> moved <title> to <status in words>";
    - `task.integrated`, "<title> was added to the project";
    - `human.accepted`, "You accepted <title>";
    - `agent.slept`, "<agent> reached its usage limit and will pick up again at <HH:MM>";
    - a ceremony `message.posted`, "<agent> posted the <thread>".

  The status words are `web-ui.md`'s lifecycle table.
- **Transcripts.** These are updated to carry summaries:
  - contracts: `refine_writes_epic_frk_1`, `refine_writes_task_frk_1`;
  - completion notes: `implement_finishes_frk_1`, `plan_closes_epic_frk_1`;
  - review notes: `review_writes_note`, `review_answers_nothing`, `review_epic_frk_1`, `review_epic_fails_frk_1`.

  `plan_breaks_down_frk_1` changes only if a fixture's policy binds its children; it does not, since the fixtures use `high_risk` with low-risk children. New synthetic transcripts for step 08's journeys: `ask_with_choices_frk_1`, `implement_after_send_back_frk_1`, `triage_frk_1_small_by_pm`, and `refine_writes_high_risk_frk_1` (a `high` risk task for a Developer, reviewed by the Architect, with a summary), `refine_writes_task_for_theo_frk_1` (a `low` risk task for a Developer, reviewed by the Architect), and `plan_assigns_frk_1_to_theo` (assigns FRK-1 to `theo` with `ada` reviewing); these last three match step 08's Mira, Ada and Theo team.
- **Skills.** The Product Manager's, Scrum Master's, Architect's, Developer's, and Marketing Specialist's skills each gain one line on the summary they write.

## File map

```
docs/schemas/{task-contract,event,command,rpc}.schema.json                        modifies (T1, T2, T3)
docs/decisions/0024-the-human-may-send-work-back-and-grant-more-tries.md          (committed with this plan)
crates/core/src/{contract.rs,governor/readiness.rs,governor/transition_table.rs,governor/transition.rs,governor/gates.rs,governor/escalation.rs,governor/plain.rs,budget.rs} (+ tests, fixtures)  modifies / creates (T1, T2)
crates/protocol/src/{command.rs,event.rs,rpc.rs}, event/fixtures.rs              modifies (T1, T2, T3)
crates/runtime/src/{transitions.rs,tools/work.rs,orchestrator/human.rs,orchestrator/rules.rs,cost.rs,daemon/web.rs}, recorded/{fixtures.rs,transcripts/*.jsonl}  modifies / creates (T1, T2, T3)
crates/roles/roles/*/skills/*/SKILL.md                                           modifies (T1)
crates/store/src/{requests.rs,waiting.rs,diff.rs,activity.rs,git.rs,lib.rs} (+ tests)   modifies / creates (T3)
crates/cli/src/{contract_new.rs,waiting.rs,show.rs,human.rs,lib.rs}, tests/human.rs     modifies (T2, T3)
packages/protocol-client/src/client.ts                                             modifies (T3)
docs/SPEC.md (5.1, 5.2, 5.4, 5.5, 5.7, 5.11, 8.5), docs/plans/project-plan.md     modifies (T4)
```

## Interfaces

Consumes: `evaluate_readiness`, `ReadinessContext`, `TransitionContext`, `check_contract_write`, `file_request`, `Projections`, `Git::diff` (earlier phases); `answer`/`query` in `daemon/web.rs` and the RPC wire (steps 02 and 05).

Produces:

```rust
TaskContract::summary: Option<String>;  ReadinessRule::SummaryPresent;  ReadinessContext::human_approves: bool
pub fn farik_core::governor::plain::plain_readiness(rule: ReadinessRule) -> &'static str;
GateId::HumanRejection;  TransitionContext { result_awaits_human: bool, review_passed: bool, extra_iterations: u32, .. }
Command::HumanSendBack { task_id: TaskId, subject: AcceptSubject, message: String, failed_criteria: Vec<String> }
EscalationResolve body and EscalationResolvedBody: extra_tries: Option<u8>
AskHumanInput::choices / QuestionAskedBody::choices: Vec<QuestionChoice { label: String, hint: Option<String> }>
pub fn farik_store::requests::{request_from_brief, placeholder_budget_usd};  pub const PLACEHOLDER: &str;
pub fn farik_store::git::integration_branch(team: &Team, git: &Git) -> Result<String, GitError>;
pub fn farik_store::waiting::waiting(projections: &Projections, log: &EventLog, files: &ProjectFiles, team: &Team) -> Result<Vec<Waiting>, StoreError>;
pub fn farik_store::diff::diff_of(git: &Git, team: &Team, contract: &TaskContract, history: &[FarikEvent], children: &[(TaskContract, Vec<FarikEvent>)]) -> Result<TaskDiff, String>;
pub fn farik_store::activity::activity(log: &EventLog, projections: &Projections, files: &ProjectFiles, team: &Team, now: DateTime<Utc>) -> Result<Vec<AgentActivity>, StoreError>;
Transitions::readiness_context(&self, team: &Team, contract: &TaskContract) -> Result<ReadinessContext, TransitionError>;
```

RPC queries: `waiting.list`, `contract.get`, `contract.check`, `task.history`, `task.diff`, `task.checks`, `task.tries`, `sprint.current`, `questions.list`, `escalation.choices`, `team.activity`, `moved.since`. RPC methods: `request.file`, `contract.save`.

## Tasks

### Task 1: Summaries

Files: the contract schema, core contract, readiness and plain, `tools/work.rs`, transcripts, skills. Produces `summary`, `SummaryPresent`, `plain_readiness`, and the note rule.

- `binds_the_summary_to_the_plans_a_human_approves`: a `high` risk task under `high_risk` and an epic fail `SummaryPresent` without a summary; a `low` risk task passes.
- `plain_readiness_covers_every_rule`: every variant has a non-empty sentence (an exhaustive `match`).
- `refuses_a_note_without_an_opening_summary`: completion and review notes get the exact refusal; progress notes are accepted.
- The updated transcripts replay, and every existing test passes.

- [x] `feat(core): require the summaries a human gate leads with`

### Task 2: Sending back and more tries

Files: core transition table, transition and gates, budget, escalation; protocol command and event; runtime transitions, human, rules and cost; CLI `send-back`. Produces `HumanRejection`, `HumanSendBack`, `extra_tries`, and `farik send-back`.

- `sends_a_result_back_after_the_review`: the rejection event and the try counted; before the review passed it is refused with `review_first`; a result not waiting on the human is refused.
- `sends_a_plan_back_to_refining`.
- `grants_extra_tries`: with `max_iterations: 3` at iteration 3, a resolve with `extra_tries: 2` sets iteration to 4, and exactly two attempts run (the resumed one and one after a rejection) before `iterations` escalates again; the sessions limit grows by 8.
- `refuses_extra_tries_for_other_reasons`.
- `sends_back_from_the_command_line`: `farik send-back FRK-1 "…"` works through the daemon.

- [x] `feat(runtime): let the human send work back and grant more tries`

### Task 3: Filing, checking, saving, choices, and the shared queries

Files: the store's requests, waiting, diff, activity and git; CLI moves; runtime `daemon/web.rs`; the rpc schema; protocol-client. Produces every RPC query and method above.

- `files_a_request_from_plain_words`: the title is cut on a word boundary; a text under 20 characters is refused.
- `checks_a_draft_without_saving`: a schema error comes first with rule `schema`; readiness failures carry their plain sentence; the file is unchanged.
- `saves_the_human_edit_back_to_refining`: a frozen contract saves, moves to `refining`, and answers `back_to_refining`.
- `lists_what_waits_on_the_human`: the five kinds with their exact lines; the CLI's output is unchanged.
- `answers_the_diff_and_checks`: a task's counts and files; an epic's children joined under their ids.
- `derives_each_agents_activity`: each state's exact line.
- `says_what_moved_since`: each kind's exact line, `agent.slept` included.
- `offers_the_choices_for_each_reason`: the bodies above per reason.
- `records_question_choices`: labels and hints within their bounds, and `questions.list`'s fields.
- `answers_tries_and_the_sprint`: `task.tries` after an extra-tries grant; `sprint.current` counts; `contract.check`'s `total`.

- [x] `feat(runtime): answer the gates' queries, and file, check and save plans from the browser`

### Task 4: Spec and plan

- Spec 5.1: the human's send-back comes after the review.
- Spec 5.2: the new row.
- Spec 5.4: the summary and the opening rule; which plans are bound.
- Spec 5.5 and 5.7: extra tries and sessions.
- Spec 5.11: `summary` as content.
- Spec 8.5: the events, and `review.recorded`'s description in `event.schema.json`, which said nothing gates on it, now says the human's send-back waits for it.
- The project plan: this step's line, and the `contract.validate` rename.

- [x] `docs(spec): the gates' summaries, sending back, and more tries`

## Verification

```
cargo xtask check --integration
# expected: every cargo "test result:" line 0 failed (T1 3, T2 5, T3 9 new named tests); the web and playwright counts
#   unchanged from step 06's landing; last line: xtask check: ok
```
