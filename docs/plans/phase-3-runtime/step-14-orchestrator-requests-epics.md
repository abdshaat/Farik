# Phase 3, step 14: Orchestrator, requests, epics, and the human

Status: draft (readiness review pending)
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.2, 5.4, 5.7, 5.11, 5.16, 8.5, F1
Depends on: step 13 of this phase (`integrate`, `recover`, `awaiting_integration`), a start gate: Task 1 does not begin until step 13's last commit is on this branch; step 12 (the verifying rules, `review_writes_note`, `accept_frk_1`, `crates/runtime/tests/one_task.rs`); step 11 (`Orchestrator`, the session mechanics, the order of work, `orchestrator::fixtures`, `UsageThenWaitAdapter`, `implement_finishes_frk_1`); step 10 (`PromptInput::human_message`, `CLOSING_INSTRUCTIONS`); step 07 (`DaemonState`, `decide_pre_tool_use`); step 05 (the tools); step 04 (`Transitions`); phase 2 (`file_request`, the human's triage and lock in `crates/cli`, on main)
Readiness confirmed by: pending

## Goal

A request goes the whole way the contract architecture says without anyone driving it by hand. An untriaged request gets a triage session on the cheaper model; a triaged one is refined by the Product Manager, who asks its questions first when it is an epic and waits for the answers; a written contract is judged by the governor and goes to `ready`, back to the Product Manager with its failures, or to the human for approval. An approved epic is assigned to its Product Manager, broken down in `plan` sessions, its tasks assigned and worked as steps 11 to 13 work any task, closed out when they are done, and accepted once the human has accepted it. The human acts through one door, `Orchestrator::handle`: answering a question, approving a contract or accepting a result, resolving an escalation, moving, locking, triaging, and integrating a task, stopping a session or the run, and pausing an agent, which ends its session at the next hook and blocks its work. Tested end to end with the recorded adapter: one request to an accepted epic. Out of scope: the command line (step 15), which prints what `handle` and `tick` report; a second terminal reaching a running `farik run` (step 15); the Scrum Master (phase 4, whose role does not ship before then: a team with an active one is outside this phase); the channel and one-on-ones (phase 4).

## Decisions

- Two rules join step 11's order, after its rule 8 because they are furthest from done: (9) `refining`; (10) `draft`. Rules 3 to 10 pass over a task that is `waiting_on_human` (an open question, 5.7), and a rule whose session's agent is not `active` in the team file passes its task over. Every tick starts with `Projections::catch_up`, so a command another process handled (step 15's `farik answer` beside `farik run`) is on the board at the next tick.
- `draft` (rule 10). Untriaged (only a request with no parent can be; `file_request` triages a child `small` at filing): a `triage` session for the first active Product Manager in team-file order, whose task is the request, `cwd` the root, `farik_tools` exactly `["farik_triage_request"]`, no built-ins, and the model `TRIAGE_MODEL` = `claude-sonnet-5` with effort `low` whatever the agent's own model, because 5.16 runs triage on the cheaper model and 8.2 names Sonnet 5 as it. Triaged: `draft → refining` asked as `ProductManager` with that agent's id, on its behalf, no session, as step 11's rule 7 asks `assigned → in_progress` for the assignee: the row is the Product Manager picking the request up, and a session to say "I pick it up" says nothing. The human's overrule window is the draft: `farik triage` before triage runs, or `--size` at filing (step 15); rejected: a grace period after an agent's triage, a timer nothing in 5.16 asks for that would stall every request to wait for an overrule that rarely comes.
- `refining` (rule 9). The contract is judged when a `contract.written` exists after the later of the task's last move into `refining` and its last `request.triaged`, and no `transition.refused` from `refining` requested by `governor` follows that write; or, with no such write, when the contract was filed whole (the task has a parent, which the breakdown wrote, or is locked, which the human wrote) and no such refusal exists since refining began. To judge: ask `refining → escalated` as `Governor` (its rows open on three readiness failures, or on a passing contract the human must accept: an epic, `high` risk, or the policy `all`); when refused, ask `refining → ready` as `Governor`, whose Definition of Ready records `contract.evaluated`. Escalation is asked first because asking `ready` of a passing epic fails its gate on the missing approval and would count as a readiness failure. Otherwise: the Product Manager's `refine` session, `cwd` the root. Chose judging a contract filed whole before any session, so a child the breakdown wrote passes without spending one; and not judging a raw request, whose brief would fail and spend one of the three attempts 5.2 gives the Product Manager. A locked contract that fails can only be refined by the human; its refine sessions write nothing and `max_sessions` bounds them (`ponytail: a locked failing contract escalates to its owner`).
- A refine session's first message (`messages.rs`) names the task and its kind; for an epic with no `question.asked` since refining began it says, before anything else, "This is an epic: ask the user every question you need with `farik_ask_human` before you write it; if you have none, say so in the intent"; when the last `contract.evaluated` since refining began failed, its failures follow, one per line, as Farik's own words, unwrapped.
- The human's words reach the next session about the task as step 10's `human_message` (it was `None` in step 11): every `question.answered` on the task and every `escalation.resolved` message recorded after the task's last `session.started`, in log order, an answer as `Question <id>: <question>` then `Answer: <answer>`, a resolution as `The human, moving this to <to>: <message>`, blocks separated by one blank line. Chose "since the last session started" because the session that asked is the last one started, and the answer arrived after it.
- Questions (5.7): `question.answered { question_id, answer, answered_by }`, the envelope's task the question's. The board's `waiting_on_human` is `open_questions > 0`, counted up by `question.asked` and down by `question.answered` for the task (a column `open_questions INTEGER NOT NULL DEFAULT 0`, the field a `bool`). `farik_write_contract`'s epic check (step 05's `has_asked`) becomes "a question on this task with no `question.answered` naming its seq".
- Approval (5.16 item 2). `awaiting_approval` on the board: true on `escalation.raised` with reason `approval` or `risk_gate` (the two reasons of the `ContractRequiresHuman` gate), false on the task's next `task.transitioned`. `HumanAccept { subject: contract }` on a task awaiting approval appends `human.accepted { subject: contract, accepted_by: human }` and asks `escalated → ready` as `Human`. Chose the direct move over `escalated → refining → ready`, because 5.16 item 1 says a return to `refining` ends the approval, and the contract has been frozen (5.11) since the gate that escalated it passed the structural checks. `Transitions::context` reads `acceptance.given` as a `human.accepted { subject: contract }` after the task's last `contract.written` and last move into `refining`; `farik_write_product_doc` asks the same function (`contract_accepted`, `transitions.rs`) instead of step 05's `false`.
- The human's acceptance of a result. `HumanAccept { subject: result }` on a `verifying` task that is `high` risk, an epic, or holds a `human` criterion appends `human.accepted { subject: result, accepted_by: human, message }` and moves nothing: `verifying → accepted` stays the Product Manager's row. `Transitions::context` reads one since the task's last move into `verifying` as `done.human_accepted`, and as step 12 decided, as a passing `CriterionResult { run_by: Human, evidence: "human.accepted at seq <n>" }` per `human` criterion. Step 12's verifying item 5 then runs the Product Manager's `verify` session instead of waiting for ever.
- An epic's end, in this phase. With no Scrum Master, every epic's assignee is the Product Manager and its reviewer the human (5.16 item 4). The human's `human.accepted { subject: result }` on an epic whose reviewer is the human is its reviewer's work: `Transitions::context` reads it also as a passing `CriterionResult { run_by: Reviewer }` with that evidence for each criterion other than a `human` one, and its `message` as `done.review_note`; `handle` refuses the acceptance without a non-blank `message` (`Invalid`). Farik runs no criterion for an epic and no reviewer session starts: step 12's items 1 and 2 are skipped for an epic, which has no branch to run them on, and the rule waits until the acceptance exists. This is the phase's default and the first item for the founder below; 5.1 says it "costs nothing extra because the human accepts every epic anyway".
- The epic's other states. `ready` (rule 8): Farik asks `ready → assigned` as `ProductManager` on the first active Product Manager's behalf, assignee that agent and no reviewer, no session, because 5.16 item 3 fixes every fact of it; rejected: a `plan` session to make a choice that has one answer. `assigned` (rule 7): `assigned → in_progress` as that assignee, and no worktree: an epic has no branch. `in_progress` (rule 6), instead of an implement session: every task under it `accepted` or `cancelled` and one `accepted` → the assignee's `plan` session to close it (first message lists each child's id, title, and status and asks for the completion note and `verifying`); no task under it other than `cancelled` ones → the assignee's `plan` session to break it down (first message: file its tasks with `farik_create_task`, `parent` the epic, within its paths and remaining budget); otherwise no rule. `Plan`'s closing instruction (step 10) gains: "When every task under the epic is done, write its completion note with `farik_write_note` of kind `completion` and request `verifying`." A `ready` task under an epic (rule 8) gets step 11's plan session with its epic's assignee as the assigner, which step 11 left out.
- Stopping and pausing reach a session through the daemon, where the hook already reads the team file on every decision (step 07). `DaemonState` gains a stop reason per registration and a `tokio::sync::Notify`; `request_stop(session_id, reason)` sets the reason and wakes waiters; `decide_pre_tool_use` denies a stopped session's every call as `session_stopped: <reason>`, and on its existing `agent_not_active` denial requests the stop with "agent paused by the user" (or "retired"). Step 11's session loop selects over the next event and the notification, and aborts once when its session has a stop reason. So a pause handled in another process ends the session at its next tool call (F1), and one handled in this process ends it at once. Rejected: the session loop reading the team file on every event, which sees nothing while a session thinks and duplicates the hook's check.
- `AgentUpdate { agent_id, status }` (status only in this phase; phase 5's team builder widens it): writes the team file, appends `agent.updated { agent_id, status, updated_by: human }`, and on `paused` or `retired` requests the stop of the agent's registered sessions and moves each of its `in_progress` tasks to `blocked`, asked as `Assignee` on its behalf with `Blocker { description: "agent paused by the user", needed: "the human resumes <id>" }` (or "retired", "reassigns the task"). On `active`, each of its `blocked` tasks whose last block's description is "agent paused by the user" moves to `in_progress` as `Human` with the resolution "agent resumed by the user", because the human resuming the agent is the human's word that the blocker is gone. `SessionStop { session_id }`: `NotFound` unless registered in this process's daemon; requests the stop with "stopped by the human" and, for a session about a task that is neither terminal nor `escalated`, asks `→ escalated` as `Human` with the reason "stopped by the human" (5.2: a user's stop takes the human's row). `RunStop`: step 11's `stop`, which lets the running session finish.
- The human's moves. `TransitionAsk` gains `reason: Option<String>`, recorded as the new optional `reason` of `task.transitioned`, and for the human's `→ escalated` it is the escalation's words (`escalation_detail` reads it after the rejection and the blocker). `TaskTransition { task_id, to, reason }`: any status but `escalated`, asked as `Human`, `reason` also the `blocker_resolution` from `blocked`. `EscalationResolve { task_id, to, message }` (the `to` added to the project plan's line: an escalation of a blocked or rejected task has no status to go "back" to that would not re-escalate it), from `escalated` only, a non-blank message, `to` never `ready` while awaiting approval (that is `HumanAccept`): `escalation.resolved { to, message, resolved_by: human }`, then `escalated → to` as `Human`.
- `handle(command) -> Result<CommandReport, CommandError>`, one sentence and the seqs it appended, for step 15 to print (changed from the line's `()`: `integrate`'s outcome and a question's answer would otherwise be lost). Refusals are `Refused { reason }` whose reason starts with a `snake_case` kind then `: `, as the tools' do: `already_answered`, `not_a_question`, `not_awaiting_approval`, `not_waiting_for_the_human`, `not_escalated`, `use_human_accept`, `use_escalation_resolve`, `same_status`, `transition_refused` (then the governor's details). `same_status` is an `AgentUpdate` to the status the agent has. `NotFound` names the task, question, agent, or session. `Failed { detail }` carries a store, file, git, or orchestrator error (added to the line: those are not the human's fault). `TaskIntegrate` is step 13's `integrate`. `ContractLock`, `ContractUnlock`, and `RequestTriage` call the store (Task 1). `TaskCreate` is `Invalid`: its contract carries an id, and a request is given its id by `file_request`; phase 5 step 01, its first sender, reshapes it.
- One body shape, `taskIdBody`, serves `contract_lock`, `contract_unlock`, and `task_integrate`, and `run_stop`'s body is `{}` (`emptyBody`): `commandBodyWire` is a `oneOf`, which fails a body two branches match; the reader pairs the body with `command`.
- Commands take effect through the store and the files, so any process may handle them, except `SessionStop`, which reaches only a session registered in the handling process; how a second terminal reaches `farik run` is step 15's.

## Open for the founder

1. An epic's review in this phase: the human's acceptance, with its message, stands for the reviewer's run of every criterion and the review note, and Farik runs none of the epic's criteria (above). This changes what 5.4 item 1 means for an epic. The alternative is Farik running the epic's `command`, `test`, and `artifact` criteria as the reviewer on a detached worktree of the integration branch before asking the human, which is roughly one more task of work here. The plan is written on the first; a "no" swaps the epic branch of rule 5 for step 12's items 1 and 2 on that worktree.

## File map

```
crates/store/src/requests.rs, crates/cli/src/contract.rs, crates/cli/src/triage.rs   modifies: the human's triage and lock move to the store; tests
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies: the ten commands; tests
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: question.answered, human.accepted, escalation.resolved, agent.updated; task.transitioned's reason
crates/store/src/migrations/0006_human.sql, migrations.rs   creates, modifies: open_questions, awaiting_approval, backfilled
crates/store/src/projections.rs                      modifies: waiting_on_human, awaiting_approval; tests
crates/runtime/src/transitions.rs                    modifies: TransitionAsk::reason, contract_accepted, the human's results, the epic's review; tests
crates/runtime/src/tools/contracts.rs, tools/work.rs modifies: open questions, the approval; tests
crates/runtime/src/daemon.rs, daemon/hooks.rs        modifies: request_stop, stop_reason, the notification, session_stopped; tests
crates/runtime/src/prompt.rs                         modifies: Plan's closing instruction
crates/runtime/src/orchestrator.rs                   modifies: handle, CommandReport, CommandError, TRIAGE_MODEL, catch_up
crates/runtime/src/orchestrator/human.rs             creates: every command's handling; tests
crates/runtime/src/orchestrator/requests.rs          creates: rules 9 and 10, the epic's rules; tests
crates/runtime/src/orchestrator/session.rs           modifies: human_message, the triage model and tool, watching for a stop
crates/runtime/src/orchestrator/rules.rs             modifies: the order, waiting_on_human, epics in rules 5 to 8; tests
crates/runtime/src/orchestrator/messages.rs          modifies: triage, refine, breakdown, and close-out messages; the human's words
crates/runtime/src/orchestrator/fixtures.rs          modifies: a_request (an untriaged draft filed by the human)
crates/runtime/src/recorded/transcripts/{triage_frk_1_large, refine_asks_frk_1, refine_writes_epic_frk_1, refine_writes_task_frk_1, plan_breaks_down_frk_1, plan_assigns_frk_2, plan_closes_epic_frk_1}.jsonl   creates
crates/runtime/src/recorded/fixtures.rs              modifies: the seven transcripts
crates/runtime/tests/one_epic.rs                     creates: the end-to-end test, ignored (needs git)
docs/SPEC.md                                         modifies: 5.2 (a stop), 5.7 (answers, resolution with a status and a message, the next session's words), 5.16 (triage model; draft to refining; approval; the epic's assignment, breakdown, close-out, and review in this phase), F1 (the pause blocks and the resume unblocks), 8.5 (agent.updated)
docs/plans/project-plan.md                           modifies: step 14's interface line
```

## Interfaces

Consumes: `Orchestrator`, `OrchestratorError`, `TickReport`, `stop`, the session mechanics, `orchestrator::fixtures`, `UsageThenWaitAdapter` (step 11); the verifying rules, `review_writes_note`, `accept_frk_1` (step 12); `integrate`, `IntegrationOutcome`, `awaiting_integration` (step 13); `untrusted_block`, `CLOSING_INSTRUCTIONS`, `PromptInput` (step 10); `DaemonState`, `decide_pre_tool_use`, `SessionRegistration` (step 07); `call_tool` and the tools (step 05); `Transitions`, `TransitionAsk`, `integration_branch` (step 04); `check_human_triage`, `check_contract_write`, `check_product_doc_write`, `requires_human_acceptance`, `Blocker` (`farik-core`); `file_request`, `RequestError`, `Projections::catch_up`, `ProjectFiles::write_team`, `apply_through` (main and step 13).

Produces:

```rust
// wire bodies (events)
QuestionAnsweredBody { question_id: u64, answer: String, answered_by: String }
HumanAcceptedBody { subject: HumanAcceptedBodySubject /* contract | result */, accepted_by: String, message: Option<String> }
EscalationResolvedBody { to: TaskStatusWire, message: String, resolved_by: String }
AgentUpdatedBody { agent_id: String, status: AgentUpdatedBodyStatus /* active | paused | retired */, updated_by: String }
TaskTransitionedBody { .., reason: Option<String> }
// farik-protocol
pub enum AcceptSubject { Contract, Result }
pub enum Command { .., TaskTransition { task_id: TaskId, to: TaskStatus, reason: String }, HumanAccept { task_id: TaskId, subject: AcceptSubject, message: Option<String> },
    EscalationResolve { task_id: TaskId, to: TaskStatus, message: String }, QuestionAnswer { question_id: u64, answer: String }, ContractLock { task_id: TaskId },
    ContractUnlock { task_id: TaskId }, TaskIntegrate { task_id: TaskId }, AgentUpdate { agent_id: String, status: AgentStatus }, SessionStop { session_id: String }, RunStop }
// farik-store
TaskProjection { .., pub waiting_on_human: bool, pub awaiting_approval: bool }
pub fn triage_by_human(files: &ProjectFiles, log: &EventLog, projections: &Projections, task_id: &TaskId, size: RequestSize, reason: &str, now: DateTime<Utc>, ids: &EventIds) -> Result<FarikEvent, RequestError>;
pub fn hold_contract(files: &ProjectFiles, log: &EventLog, projections: &Projections, task_id: &TaskId, held: bool, now: DateTime<Utc>, ids: &EventIds) -> Result<FarikEvent, RequestError>;
// farik-runtime
pub const TRIAGE_MODEL: &str = "claude-sonnet-5";
pub struct TransitionAsk { .., pub reason: Option<String> }
pub(crate) fn contract_accepted(history: &[FarikEvent]) -> bool;   // transitions
impl DaemonState { pub fn request_stop(&self, session_id: &str, reason: &str) -> bool; pub fn stop_reason(&self, session_id: &str) -> Option<String>; pub fn sessions_of(&self, agent_id: &str) -> Vec<String>; }
pub struct CommandReport { pub said: String, pub events: Vec<u64> }
pub enum CommandError { Invalid { detail: String }, Refused { reason: String }, NotFound { what: String }, Failed { detail: String } }
impl Orchestrator { pub async fn handle(&self, command: Command) -> Result<CommandReport, CommandError>; }
```

## Tasks

The harness is step 11's, its integration `auto_merge` with no remote. `fixtures::a_request(title)` files an untriaged draft by `human` with `file_request`. The seven transcripts, each with one `result` line, call exactly: `triage_frk_1_large` `farik_triage_request { size: large, reason: "A file and the check that it exists." }`; `refine_asks_frk_1` `farik_ask_human { question: "Should done.txt be empty?" }`; `refine_writes_epic_frk_1` `farik_write_contract` with every field an epic needs (intent, `allowed_paths: [done.txt]`, one `review` criterion R1, an `out_of_scope` item, risk `low`, `budget { max_cost_usd: 5, max_sessions: 10 }`); `refine_writes_task_frk_1` the same for a standalone task with step 11's FRK-1 fields; `plan_breaks_down_frk_1` `farik_create_task { parent: FRK-1 }` with step 11's FRK-1 contract fields and `max_cost_usd: 2`; `plan_assigns_frk_2` `farik_assign_task { task_id: FRK-2, assignee_id: dev-a, reviewer_id: dev-b }`; `plan_closes_epic_frk_1` `farik_write_note { kind: completion, text: "FRK-2 added done.txt; nothing left out." }` then `farik_request_transition { to: verifying }`. Step 11's `implement_finishes_frk_1` and step 12's `review_writes_note` and `accept_frk_1` name no task, so they replay for FRK-2 and the epic.

### Task 1: the human's triage and lock in the store

Files: `requests.rs`, `crates/cli/src/contract.rs`, `crates/cli/src/triage.rs`

- `records_the_humans_triage` — a draft: `triage_by_human(.., Large, "Three deliverables.")` writes `kind: epic` to the file and appends `request.triaged { size: large, triaged_by: human }`; the board's row is an epic and triaged.
- `refuses_the_humans_triage_once_refining_started` — `refining`: `Refused` containing `check_human_triage`'s words, nothing appended; a blank reason is `Refused` before anything is read.
- `locks_and_gives_back_a_contract` — `hold_contract(.., true)` sets `locked` in the file and appends `contract.locked { locked_by: human }`; again is `Refused` containing `already`; `false` appends `contract.unlocked`.
- The phase 2 tests of `farik triage` and `farik contract lock` in `crates/cli/tests/commands.rs` pass unchanged.

- [ ] `refactor(store): record the human's triage and lock for every caller`

### Task 2: the commands, the events, and the board

Files: the two schemas, `command.rs`, `event.rs`, `event/fixtures.rs`, the migration, `migrations.rs`, `projections.rs`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers the four kinds and a `task.transitioned` with a `reason`.
- `reads_every_human_command` — each of the ten wires reads into its variant with its fields; one `{ "task_id": "FRK-3" }` body under `contract_lock`, `contract_unlock`, and `task_integrate` reads as three variants; `run_stop` with `{}` is `RunStop`.
- `refuses_a_body_that_is_another_commands` — `question_answer` with a `task_integrate` body is refused with one error at `/body`; `human_accept` with `subject: approve` at `/body/subject`.
- `waits_on_the_human_while_a_question_is_open` — two `question.asked` on FRK-1: `waiting_on_human`; one answered: still; both: not.
- `awaits_approval_from_the_escalation_until_the_next_move` — `escalation.raised { approval }`: true; `{ risk_gate }` on FRK-2: true; `{ iterations }` on FRK-3: false; FRK-1's `escalated → ready`: false.
- `reads_an_older_log_into_the_new_columns` — a database at version 5 (`apply_through(.., 5, ..)`) holding an `escalated` row whose last escalation is `approval` and a row with a `question.asked`, opened: the first awaits approval, the second waits on the human.

- [ ] `feat(protocol): add the human's commands and the events they record`

### Task 3: the gates read the human

Files: `transitions.rs`, `tools/contracts.rs`, `tools/work.rs` (tests needing git ignored as step 04's)

- `reads_the_approval_of_the_contract_the_task_has_now` — an epic awaiting approval: `acceptance.given` false; after `human.accepted { contract }` true; after a move into `refining` and a `contract.written`, false.
- `accepts_a_high_risk_task_once_the_human_has` — `verifying`, `high` risk, reviewer results and both notes: `pm`'s `accepted` is refused with the `HumanAccepted` message; with a `human.accepted { result }` from before the last move into `verifying` still refused; after one since, moved.
- `answers_every_human_criterion_with_one_acceptance` — H1 and H2 `human`: `done.results` holds two passing `Human` results with evidence `human.accepted at seq <n>`.
- `takes_the_humans_acceptance_of_an_epic_as_its_review` — an epic with no reviewer, C1 `command` and R1 `review`, after `human.accepted { result, message: "Both look right." }`: passing `Reviewer` results for C1 and R1 with that evidence, `review_note` the message, `human_accepted` true; a task with reviewer `dev-b` gains no `Reviewer` result from it.
- `records_the_humans_reason_on_their_move` — the human's `in_progress → escalated` with reason "stopped by the human": `task.transitioned.reason` is it, and `escalation.raised { explicit_request }`'s detail ends with it.
- `lets_an_epic_be_written_once_its_question_is_answered` — `question_unanswered` while open; after `question.answered` naming its seq, the write is accepted.
- `writes_a_product_document_once_the_epic_is_approved` — after `human.accepted { contract }` and the epic `ready`: written and `product_doc.written` appended; the same epic `cancelled`: refused. Step 05's refusal test passes unchanged.

- [ ] `feat(runtime): read the human's approvals, acceptances, and answers in every gate`

### Task 4: the human's commands

Files: `orchestrator.rs`, `orchestrator/human.rs`, `docs/SPEC.md` (5.7)

- `answers_a_question_once` — `question.asked` at seq n on FRK-1: `QuestionAnswer { n, "Yes." }` appends `question.answered { question_id: n, answer: "Yes.", answered_by: human }` with FRK-1 on the envelope and `events == [its seq]`; again is `Refused` starting `already_answered`; a seq of a `task.created` starts `not_a_question`; a seq past the log is `NotFound`; a blank answer is `Invalid`.
- `approves_a_contract_awaiting_approval` — epic FRK-1 awaiting approval: `human.accepted { subject: contract }` then `task.transitioned` `escalated → ready`, `actor: human`; the board `ready`, not awaiting; on a `ready` task `Refused` starting `not_awaiting_approval`.
- `accepts_a_result_only_where_the_human_is_asked` — `verifying`, `high` risk: `human.accepted { subject: result }` and no move; `low` risk with no `human` criterion: `not_waiting_for_the_human`; an epic whose reviewer is the human without a message: `Invalid`.
- `resolves_an_escalation_with_a_status_and_a_message` — `EscalationResolve { refining, "Split it by page." }` appends `escalation.resolved { to: refining, message, resolved_by: human }` then `escalated → refining`; `to: ready` while awaiting approval starts `use_human_accept`; on a `ready` task `not_escalated`; a blank message `Invalid`.
- `moves_a_task_for_the_human` — `blocked → in_progress` with "Key rotated.": `blocker_resolution` and `reason` both it; `→ cancelled` moves; from `escalated` `use_escalation_resolve`; `in_progress → accepted` is `Refused` starting `transition_refused` with the governor's details, and `transition.refused` is in the log.
- `locks_triages_and_integrates_through_one_door` — `ContractLock` and `ContractUnlock` append their events; `RequestTriage` appends `request.triaged { triaged_by: human }`; `TaskIntegrate` on an accepted task under `manual` appends `task.integrated { integrated_by: human }` and says `merged`; `TaskCreate` is `Invalid`.
- `stops_the_run_after_the_session_it_is_in` — `RunStop`, then `run_until_idle`: no tick runs.

- [ ] `feat(runtime): handle the human's commands`

### Task 5: stopping and pausing

Files: `daemon.rs`, `daemon/hooks.rs`, `orchestrator/session.rs`, `orchestrator/human.rs`, `docs/SPEC.md` (5.2, F1, 8.5)

- `denies_every_call_of_a_stopped_session` — after `request_stop(s, "stopped by the human")`: `stop_reason(s)` is it, and a pre-tool-use for `s` is denied `session_stopped: stopped by the human`; `request_stop` of an unregistered id is false.
- `stops_the_session_of_an_agent_paused_in_the_team_file` — `dev-a` paused by writing the file: the next hook is denied `agent_not_active` and `stop_reason` is "agent paused by the user".
- `aborts_the_session_of_an_agent_paused_mid_session` — FRK-1 `in_progress`, `UsageThenWaitAdapter` waiting, `AgentUpdate { dev-a, paused }` handled from a spawned task: `abort` called once; the log holds `agent.updated { dev-a, paused, human }`, `session.ended { reason: aborted }`, and `in_progress → blocked` with blocker "agent paused by the user"; the team file says `paused`; the next tick is `Idle`.
- `resumes_what_the_pause_blocked` — then `AgentUpdate { dev-a, active }`: FRK-1 `blocked → in_progress`, `actor: human`, `blocker_resolution: "agent resumed by the user"`; FRK-2, blocked by `dev-a` for another reason, stays `blocked`; `AgentUpdate { dev-a, active }` again is `Refused` starting `same_status`; an unknown agent is `NotFound`.
- `stops_a_running_session_and_escalates_its_task` — `SessionStop` of the waiting session: `abort` once, `session.ended { aborted }`, FRK-1 `escalated` with `escalation.raised { explicit_request }` whose detail ends "stopped by the human"; an unknown id is `NotFound`.

- [ ] `feat(runtime): stop a session and pause an agent at the next hook`

### Task 6: triage and refining

Files: `orchestrator/requests.rs`, `rules.rs`, `session.rs`, `messages.rs`, `fixtures.rs`, the triage and refine transcripts, `docs/SPEC.md` (5.16)

- `triages_a_request_on_the_cheaper_model` — `a_request`, `triage_frk_1_large`: one spec, `purpose: Triage`, `agent_id: pm`, `model: TRIAGE_MODEL`, effort `low`, `farik_tools == ["farik_triage_request"]`, `builtin_tools` empty; `request.triaged { large, triaged_by: pm }`; the board says epic.
- `starts_refining_a_triaged_request_without_a_session` — the next tick: `draft → refining`, `actor: product_manager`, `requested_by: pm`; no spec started. A child filed by `farik_create_task` goes the same way with no triage session.
- `asks_first_when_refining_an_epic` — the next tick, `refine_asks_frk_1`: `purpose: Refine`, first message contains `farik_ask_human`; `question.asked`; the tick after is `Idle` with no spec started.
- `hands_the_answer_to_the_next_refine_session` — after `QuestionAnswer { n, "Yes." }`, with step 11's `replays_farik_read_board` as the refine session (it writes nothing): its system prompt contains `Question <n>: Should done.txt be empty?` and `Answer: Yes.`; the refine session the next tick starts, nothing answered since, has neither.
- `judges_a_written_contract_before_refining_again` — standalone, `refine_writes_task_frk_1`: the next tick records `contract.evaluated { passed: true }` and `refining → ready` with `requested_by: governor`, and starts nothing.
- `returns_a_failing_contract_with_its_failures` — the contract without `out_of_scope`: the tick records `contract.evaluated { passed: false }`; the next starts a refine session whose first message contains that failure's text.
- `escalates_an_epic_for_approval_once_it_passes` — `refine_writes_epic_frk_1`: the next tick records `refining → escalated` and `escalation.raised { approval }`, no `contract.evaluated { passed: false }`; the board awaits approval; the tick after is `Idle`.
- `escalates_a_contract_that_failed_three_times` — three failed evaluations since refining began and a failing contract: `escalation.raised { readiness_failures }`.
- `judges_a_child_filed_whole_without_a_session` — FRK-2 filed by `plan_breaks_down_frk_1` under an `in_progress` epic: two ticks record `draft → refining` and `refining → ready`, and no spec names FRK-2.
- `passes_over_a_task_waiting_on_the_human` — FRK-1 `ready` with an open question, FRK-2 `ready`: the tick's plan session is FRK-2's.
- `catches_up_with_another_process_before_each_tick` — a `question.answered` appended with `EventLog::append` alone, as another process would: the next tick starts FRK-1's refine session.

- [ ] `feat(runtime): triage requests and refine their contracts`

### Task 7: epics from approval to acceptance

Files: `orchestrator/requests.rs`, `rules.rs`, `messages.rs`, `prompt.rs`, the plan transcripts, `docs/SPEC.md` (5.16)

- `assigns_an_approved_epic_to_its_product_manager` — epic `ready`: one tick records `ready → assigned`, `actor: product_manager`, assignee `pm`, no reviewer, no spec; the next records `assigned → in_progress` and `.farik/local/worktrees/FRK-1` does not exist.
- `breaks_an_epic_down_in_a_plan_session` — `plan_breaks_down_frk_1`: `purpose: Plan`, `agent_id: pm`, `task_id: FRK-1`, `cwd` the root; FRK-2 is on the board with `parent: FRK-1`, triaged, `draft`.
- `assigns_an_epics_task_through_its_assignee` — FRK-2 `ready`: `plan_assigns_frk_2`'s session is `pm`'s with `task_id: FRK-2`; FRK-2 `assigned` to `dev-a`, reviewer `dev-b`.
- `leaves_an_epic_alone_while_its_tasks_are_open` — FRK-2 `in_progress`: the tick acts on FRK-2, and no spec names FRK-1.
- `closes_an_epic_whose_tasks_are_done` — FRK-2 `accepted` and integrated: `plan_closes_epic_frk_1`'s first message contains `FRK-2` and `accepted`; `note.written { completion }` and `in_progress → verifying` on FRK-1.
- `breaks_down_again_when_every_task_was_cancelled` — FRK-2 `cancelled`: the next plan session's first message is the breakdown's.
- `waits_for_the_human_to_accept_an_epic` — FRK-1 `verifying`: `Idle`, no spec, no `criterion.recorded`.
- `hands_the_humans_acceptance_to_the_product_manager` — after `HumanAccept { FRK-1, result, "Both look right." }`: `pm`'s `verify` session's first message contains the message; `accept_frk_1` moves FRK-1 to `accepted`; no `task.integrated` for FRK-1.
- `accepts_a_high_risk_task_after_the_human` — step 12's `leaves_a_high_risk_task_for_the_human` continued: after `HumanAccept { result }`, `pm`'s `verify` session runs and `accept_frk_1` accepts.

- [ ] `feat(runtime): take an approved epic through its breakdown to acceptance`

### Task 8: one request to an accepted epic, end to end

Files: `crates/runtime/tests/one_epic.rs`, `docs/plans/project-plan.md`

- `takes_one_request_to_an_accepted_epic` — `a_request("Add done.txt and its check")` and, in order, `triage_frk_1_large`, `refine_asks_frk_1`, `refine_writes_epic_frk_1`, `plan_breaks_down_frk_1`, `plan_assigns_frk_2`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`, `plan_closes_epic_frk_1`, `accept_frk_1`; `run_until_idle`, then `QuestionAnswer` of the one question with "No, one line.", `run_until_idle`, `HumanAccept { FRK-1, contract }`, `run_until_idle`, `HumanAccept { FRK-1, result, "done.txt is on main." }`, `run_until_idle`. Afterwards FRK-1 is an `accepted` epic and FRK-2 an `accepted`, integrated task under it; `main` holds `done.txt`; the log holds, in this order among themselves, ten `session.started` (`triage`/`pm`, `refine`/`pm` twice, `plan`/`pm` for FRK-1, `plan`/`pm` for FRK-2, `implement`/`dev-a`, `verify`/`dev-b`, `verify`/`pm` for FRK-2, `plan`/`pm` for FRK-1, `verify`/`pm` for FRK-1), `request.triaged`, `question.asked`, `question.answered`, `escalation.raised { approval }`, `human.accepted { contract }`, FRK-1's `escalated → ready → assigned → in_progress`, FRK-2's `task.created`, `task.integrated` for FRK-2, FRK-1's `in_progress → verifying`, `human.accepted { result }`, and FRK-1's `verifying → accepted`; no `contract.evaluated { passed: false }`; the adapter has no transcript left.

- [ ] `test(runtime): take one request to an accepted epic end to end`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
