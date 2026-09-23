# Phase 3, step 14: Orchestrator, requests, epics, and the human

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.1, 5.2, 5.4, 5.7, 5.11, 5.16, 8.5, F1
Depends on: step 13 of this phase (`integrate`, `recover`, `awaiting_integration`, the `<id>-base` cleanup), a start gate: Task 1 does not begin until step 13's last commit is on this branch; step 12 (the verifying rules and their per-criterion run, `review_writes_note`, `accept_frk_1`); step 11 (`Orchestrator`, the session mechanics, the order of work, `orchestrator::fixtures`, `UsageThenWaitAdapter`, `implement_finishes_frk_1`, `replays_farik_read_board`); step 10 (`PromptInput::human_message`, `CLOSING_INSTRUCTIONS`, `untrusted_block`); step 07 (`DaemonState`, `decide_pre_tool_use`); step 06 (`run_criteria`, `SandboxFactory::create_base`, `Git::create_detached_worktree`); step 05 (the tools, `file_request`, `RequestError`, committed as ab0b4eb); step 04 (`Transitions`); phase 2 (the human's triage and lock in `crates/cli`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the founder's epic-review decision; findings folded in)

## Goal

A request goes the whole way the contract architecture says without anyone driving it by hand. An untriaged request gets a triage session on the cheaper model. A triaged one is refined by the Product Manager, who asks its questions first when it is an epic and waits for the answers. A written contract is judged by the governor and goes to `ready`, back to the Product Manager with its failures, or to the human for approval. An approved epic is assigned to its Product Manager, broken down in `plan` sessions, its tasks worked as steps 11 to 13 work any task, and closed out when they are done. Farik then runs the epic's mechanical criteria on the integration branch, the human accepts it, and the Product Manager records the acceptance (ADR 0013). The human acts through one door, `Orchestrator::handle`: answering a question, approving a contract or accepting a result, resolving an escalation, moving, locking, triaging, and integrating a task, stopping a session or the run, and pausing an agent, which ends its session at the next hook and blocks its work. Tested end to end with the recorded adapter: one request to an accepted epic. Out of scope: the command line and a second terminal reaching `farik run` (step 15); the Scrum Master (phase 4, whose role does not ship before then, so a team with an active one is outside this phase); the channel and one-on-ones (phase 4).

## Decisions

- Who acts. "The Product Manager" is the first `active` agent of that role in team-file order; "the assignee" and "the reviewer" are the contract's. Every rule that runs a session for an agent, or files a move on an agent's behalf without one (rule 7's `assigned → in_progress`, rule 8's epic assignment, rule 10's `draft → refining`), passes its task over when that agent is not `active`, because F1 says a paused agent takes no work. Rejected: checking only the session rules, which would let a paused Product Manager keep picking requests up.
- Two rules join step 11's order, after rule 8 because they are furthest from done: (9) `refining`; (10) `draft`. Rules 3 to 10 pass over a task that is `waiting_on_human`. Every tick starts with `Projections::catch_up`, so a command another process handled is on the board at the next tick.
- `draft` (rule 10). Untriaged (only a request with no parent can be, because `file_request` triages a child `small` at filing): a `triage` session for the Product Manager, task the request, `cwd` the root, `farik_tools` exactly `["farik_triage_request"]`, no built-ins, model `TRIAGE_MODEL` = `claude-sonnet-5` at effort `low` whatever the agent's own model, because 5.16 runs triage on the cheaper model and 8.2 names Sonnet 5 as it. Triaged: `draft → refining` asked as `ProductManager` with its id, on its behalf and with no session, as step 11's rule 7 asks for the assignee. A session to say "I pick it up" says nothing. The human's overrule window is the draft: `farik triage` before triage runs, or `--size` at filing (step 15). Rejected: a grace period after an agent's triage, a timer 5.16 does not ask for that would stall every request.
- `refining` (rule 9). The contract is judged in either of two cases. The first: a `contract.written` exists after the later of the task's last move into `refining` and its last `request.triaged`, and no `transition.refused` from `refining` with `requested_by: governor` follows that write. The second: there is no such write, the contract was filed whole (it has a parent, so the breakdown wrote it, or it is locked, so the human wrote it), and no such refusal exists since refining began. To judge, Farik asks `refining → escalated` as `Governor`, whose rows open on three readiness failures or on a passing contract the human must accept. When that is refused, it asks `refining → ready` as `Governor`, which records `contract.evaluated`. Escalation goes first because asking for `ready` on a passing epic fails on the missing approval and would count as a readiness failure. Otherwise the Product Manager gets a `refine` session, `cwd` the root. Chose to judge a contract filed whole before any session, and not to judge a raw request, whose brief would fail and spend one of the three attempts 5.2 gives. A locked contract that fails is refined by nobody but the human, and `max_sessions` bounds its sessions (`ponytail: a locked failing contract escalates to its owner`).
- A refine session's first message names the task and its kind. For an epic with no `question.asked` since refining began, it opens with "This is an epic: ask the user every question you need with `farik_ask_human` before you write it; if you have none, say so in the intent". When the last `contract.evaluated` since refining began failed, its failures follow one per line, unwrapped, because they are Farik's words.
- The human's words. The next session about a task gets them as step 10's `human_message` (step 11 passed `None`). They are every `question.answered`, `escalation.resolved`, and `human.accepted` with a message recorded on the task after its last `session.started`, in log order, blocks separated by one blank line:
  - an answer is `Question <id>:`, then the question inside `untrusted_block("question", <question>, 4096)` (the asking agent's words, 8.6), then `Answer: <answer>` unwrapped;
  - a resolution is `The human, moving this to <to>: <message>`;
  - an acceptance is `The human, accepting the result: <message>`.
  The human's own text is never wrapped, because ADR 0011 keeps the human's message unwrapped. The session that asked is the last one started, so "since the last start" is exactly what it has not seen.
- Questions (5.7): `question.answered { question_id, answer, answered_by }`, with the question's task on its envelope. The board's `waiting_on_human` is `open_questions > 0`, from a column counted up by `question.asked` and down by `question.answered`. `farik_write_contract`'s epic check (step 05's `has_asked`) becomes "a question on this task that no `question.answered` names".
- Approval (5.16 item 2). The board's `awaiting_approval` is set by `escalation.raised` with reason `approval` or `risk_gate`, the two reasons of the `ContractRequiresHuman` gate, and cleared by the task's next `task.transitioned`. `HumanAccept { subject: contract }` on a task awaiting approval asks `escalated → ready` as `Human`, and on the move appends `human.accepted { subject: contract }`. Chose the direct move over going through `refining`, because 5.16 item 1 ends an approval on any return to `refining`, and the contract has been frozen (5.11) since the gate that escalated it checked its structure. `contract_accepted(history)` (in `transitions.rs`) is true for a `human.accepted { contract }` after the task's last `contract.written` and its last move into `refining`. It is `acceptance.given` in `Transitions::context` and the approval `farik_write_product_doc` asks for, replacing step 05's `false`.
- One predicate for "the human reviews this": `reviewed_by_the_human(contract, team) = kind == epic && !team.has_active(ScrumMaster)`, `pub(crate)` in `transitions.rs`. It replaces the inline test in `refused_before_the_governor` and is used by `context`, `handle`, and the rules.
- A task's result. `HumanAccept { subject: result }` on a `verifying` task that is `high` risk or has a `human` criterion appends `human.accepted { subject: result, message }` and moves nothing, because `verifying → accepted` stays the Product Manager's row. `Transitions::context` reads one recorded since the task's last move into `verifying` as `done.human_accepted`, and, as step 12 decided, as one passing `CriterionResult { run_by: Human, evidence: "human.accepted at seq <n>" }` per `human` criterion. Step 12's verifying item 5 then runs the Product Manager's `verify` session instead of waiting. A second `HumanAccept { result }` in the same verification is `already_accepted`.
- An epic's result (ADR 0013, the founder, 2026-09-22). Rule 5 for an epic `reviewed_by_the_human`, in place of step 12's items 1 to 4:
  1. While any task under it is `awaiting_integration`: no rule, because the criteria must run on the integrated work.
  2. For each `command`, `test`, or `artifact` criterion with no governor result since the last move into `verifying`: step 12's per-criterion run, recorded as `criterion.recorded { run_by: reviewer, recorded_by: governor }`, its evidence opening with `at <sha>`, the integration branch's head the run checked out. `new_tests` is none, because an epic has no branch to compare, and the one-criterion copy handed to `run_criteria` has `new_tests_required` cleared to `false`, because step 06's `judge` fails a flagged criterion given no base-branch input and `require_new_tests` (5.12) makes the flag mandatory; the new-tests check stays with the epic's tasks (ADR 0013). The runs happen on a detached worktree at `.farik/local/worktrees/<id>-base`, made with `Git::create_detached_worktree` at the integration branch's head after removing any left over with step 06's `remove_base_worktree` (made `pub(crate)` for this), in a sandbox from `SandboxFactory::create_base`, whose network is off as for the base run: a criterion that needs the network belongs on a task. The sandbox is discarded and the worktree removed after the last run, on every way out. The run is on the local integration branch: a branch never fetched from the forge, or a child merged on the forge under `pull_request` whose fast-forward failed (step 13 escalates that), makes a criterion fail, which is acceptable, because the failure's evidence names the sha it ran at and the human reads it before accepting. Chose the `-base` name and sandbox because step 13's cleanup and `remove` already know them, and a `<id>` worktree would make `Transitions::work` look for a `farik/<id>` branch the epic does not have.
  3. Then no rule until a `human.accepted { result }` since the move into `verifying`. `handle` accepts one only when every mechanical criterion has a passing governor result since that move (else `criteria_not_run` or `criterion_failed`) and it carries a non-blank `message` (else `Invalid`). `Transitions::context` reads it as a passing `Reviewer` result for each `review` criterion, a passing `Human` result for each `human` criterion, `done.review_note` = the message, and `done.human_accepted`.
  4. Then the Product Manager's `verify` session: `cwd` the root, since an epic has no worktree; no executor; the read tier's built-ins. Its first message lists Farik's results inside `untrusted_block` and says the human accepted the epic and to request `accepted`. The human's message arrives as `human_message`, not as step 12's wrapped review note. When that session ends, step 12's `review.recorded` is appended for the epic with `reviewer: human`, `criteria_run` the number of criteria with a reviewer or human result, and `passed` true, so the log holds an epic's review as it holds a task's, an audit summary that step 16's metrics do not read (amended 2026-09-22 by the step 16 plan); it is the one `review.recorded` not appended at a reviewer session's end.
  After a failed criterion, the human's way on is `TaskTransition → escalated`, then `EscalationResolve` to `in_progress` with a message. The close-out session then carries that message, and the Product Manager may file the missing tasks.
- The epic's other states. `ready` (rule 8): when the Product Manager holds fewer open tasks than `wip_limit_per_agent`, counted as the `Assignment` gate counts them (epics included), Farik asks `ready → assigned` as `ProductManager` on its behalf, with itself as assignee and no reviewer, and runs no session. Otherwise the epic is passed over, as step 11's pre-check passes over a task, so that `run_until_idle` idles rather than refusing it every tick. Rejected: a `plan` session to make a choice with one answer (5.16 item 3). `assigned` (rule 7): `assigned → in_progress` for the assignee, with no worktree. `in_progress` (rule 6), instead of an implement session:
  - every task under it `accepted` or `cancelled` and one `accepted`: the assignee's `plan` session to close it out, whose first message lists each child's id, title, and status and asks for the completion note and `verifying`, or for new tasks when the human's message asks for more;
  - no task under it but `cancelled` ones: a `plan` session to break it down (file tasks with `farik_create_task`, `parent` the epic, within its paths and remaining budget);
  - otherwise no rule.
  `Plan`'s closing instruction gains "When every task under the epic is done, write its completion note with `farik_write_note` of kind `completion` and request `verifying`." A `ready` task under an epic gets step 11's rule 8 plan session with the epic's assignee as the assigner.
- Stopping and pausing go through the daemon, whose hook already reads the team file on every decision (step 07).
  - `DaemonState` gains a stop reason per registration and a `tokio::sync::Notify`. `request_stop` sets the reason and wakes waiters.
  - `decide_pre_tool_use` denies every call of a stopped session as `session_stopped: <reason>`. On its `agent_not_active` denial it also requests the stop, with "agent paused by the user" or "agent retired by the user".
  - Step 11's session loop selects over the next event and the notification, and aborts once when its session has a reason.
  So a pause handled in another process ends the session at its next tool call (F1), and one handled here ends it at once. Rejected: the loop re-reading the team file on each event, which sees nothing while a session thinks.
- `AgentUpdate { agent_id, status }` (only the status in this phase; phase 5's team builder widens it).
  - It writes the team file and appends `agent.updated { agent_id, status, updated_by: human }`, and no `team.updated`, whose schema description gains "a status change alone is `agent.updated`".
  - On `paused` or `retired`, it requests the stop of the agent's registered sessions (`sessions_of`). Each of its `in_progress` tasks moves to `blocked`, asked as `Assignee` on its behalf with `Blocker { description: "agent paused by the user", needed: "the human resumes <id>" }`; for `retired`, "agent retired by the user" and "the human reassigns the task".
  - On `active`, each of its `blocked` tasks whose last blocker reads "agent paused by the user" moves to `in_progress` as `Human`, with the resolution "agent resumed by the user".
  - A pause longer than `blocked_limit_hours` escalates the task through step 12's rule 4. That is intended: a long pause is the human's to decide.
- `SessionStop { session_id }` is `NotFound` unless the session is registered in this process's daemon. It requests the stop with "stopped by the human", and, for a task that is neither terminal nor `escalated`, asks `→ escalated` as `Human` with that reason (5.2: a user's stop takes the human's row). `RunStop` is step 11's `stop`, which lets the running session finish.
- The human's moves. `TransitionAsk` gains `reason: Option<String>`, recorded as `task.transitioned`'s new optional `reason`. On the human's `→ escalated` it is also the escalation's words (`escalation_detail` reads it after the rejection and the blocker).
  - `TaskTransition { task_id, to, reason }` works from any status but `escalated`, asked as `Human`; from `blocked` the reason is also the `blocker_resolution`.
  - `EscalationResolve { task_id, to, message }` works from `escalated` only, never to `ready` while awaiting approval. It asks `escalated → to` as `Human`, and on the move appends `escalation.resolved { to, message, resolved_by: human }`, so that a refused move leaves only its `transition.refused`. It carries `to` because a blocked or rejected task has nowhere to go back to that would not escalate again.
- `handle(command) -> Result<CommandReport, CommandError>` returns one sentence and the seqs it appended, for step 15 to print.
  - `Refused { reason }` reasons start with a `snake_case` kind then `: `, as the tools' refusals do: `already_answered`, `not_a_question`, `not_awaiting_approval`, `not_waiting_for_the_human`, `already_accepted`, `criteria_not_run`, `criterion_failed`, `not_escalated`, `use_human_accept`, `use_escalation_resolve`, `same_status`, and `transition_refused`, which is followed by the governor's details.
  - `NotFound` names the task, question, agent, or session. `Failed { detail }` carries a store, file, git, or orchestrator error.
  - `TaskIntegrate` is step 13's `integrate`. `ContractLock`, `ContractUnlock`, and `RequestTriage` call the store (Task 1). `TaskCreate` is `Invalid`: its contract already carries an id, and a request gets its id from `file_request`; phase 5 step 01, its first sender, reshapes it.
- The command wire (`command.schema.json`), `commandName` in this order: `task_create`, `request_triage` (both existing), `task_transition`, `human_accept`, `escalation_resolve`, `question_answer`, `contract_lock`, `contract_unlock`, `task_integrate`, `agent_update`, `session_stop`, `run_stop`. The bodies: `taskTransitionBody { task_id, to, reason }`; `humanAcceptBody { task_id, subject: contract | result, message? }`; `escalationResolveBody { task_id, to, message }`; `questionAnswerBody { question_id: integer ≥ 1, answer }`; `taskIdBody { task_id }`, shared by `contract_lock`, `contract_unlock`, and `task_integrate`; `agentUpdateBody { agent_id (the team schema's id pattern), status: active | paused | retired }`; `sessionStopBody { session_id (minLength 1) }`; `emptyBody {}` for `run_stop`. Every field is required unless it is marked `?`; `to` is the event schema's status list; `task_id` is `^FRK-[0-9]{1,6}$`; every body has `additionalProperties: false`. The bodies share one `taskIdBody` because `commandBodyWire` is a `oneOf`, which fails a body two branches match, and the reader pairs a body with `command` after the schema has passed. Blankness (a reason, a message, an answer) is `handle`'s check, as the human's triage checks its reason.
- Commands take effect through the store and the files, so any process may handle them. The exception is `SessionStop`, which reaches only a session registered in the handling process.
- Files shared between tasks, where the split is cheaper than a new module: tasks 6 and 7 both extend `requests.rs`, `rules.rs`, and `messages.rs`, because the epic's rules build on the refining ones; tasks 4 and 5 both extend `human.rs`, because the stop and the pause are commands. Each `docs/SPEC.md` section is edited by one task.

## File map

```
crates/store/src/requests.rs, crates/cli/src/contract.rs, crates/cli/src/triage.rs   modifies: the human's triage and lock move to the store; tests
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies: the ten commands; tests
docs/schemas/event.schema.json, crates/protocol/src/event.rs, event/fixtures.rs   modifies: question.answered, human.accepted, escalation.resolved, agent.updated; task.transitioned's reason; team.updated's description
crates/store/src/migrations/0006_human.sql, migrations.rs   creates, modifies: open_questions, awaiting_approval, backfilled from the log
crates/store/src/projections.rs                      modifies: waiting_on_human, awaiting_approval; tests
crates/runtime/src/transitions.rs                    modifies: TransitionAsk::reason, contract_accepted, reviewed_by_the_human, the human's results; tests
crates/runtime/src/tools/contracts.rs, tools/work.rs modifies: open questions, the approval; tests
crates/runtime/src/criteria.rs                       modifies: remove_base_worktree made pub(crate)
crates/runtime/src/sandbox.rs                        modifies: create_base's doc comment names the epic's run on the integration branch
crates/runtime/src/daemon.rs, daemon/hooks.rs        modifies: request_stop, stop_reason, sessions_of, the notification, session_stopped; tests
crates/runtime/src/prompt.rs                         modifies: Plan's closing instruction
crates/runtime/src/orchestrator.rs                   modifies: handle, CommandReport, CommandError, TRIAGE_MODEL, catch_up
crates/runtime/src/orchestrator/human.rs             creates: every command's handling; tests
crates/runtime/src/orchestrator/requests.rs          creates: rules 9 and 10, the epic's rules and its criterion run; tests
crates/runtime/src/orchestrator/session.rs           modifies: human_message, the triage model and tool, watching for a stop
crates/runtime/src/orchestrator/rules.rs             modifies: the order, the passes over, epics in rules 5 to 8; tests
crates/runtime/src/orchestrator/messages.rs          modifies: triage, refine, breakdown, close-out, and the epic's verify messages; the human's words
crates/runtime/src/orchestrator/fixtures.rs          modifies: a_request; CountingSandboxFactory counts create_base
crates/runtime/src/recorded/transcripts/{triage_frk_1_large, refine_asks_frk_1, refine_writes_epic_frk_1, refine_writes_task_frk_1, plan_breaks_down_frk_1, plan_assigns_frk_2, plan_closes_epic_frk_1}.jsonl   creates
crates/runtime/src/recorded/fixtures.rs              modifies: the seven transcripts
crates/runtime/tests/one_epic.rs                     creates: the end-to-end test, ignored (needs git)
docs/SPEC.md                                         modifies: 5.1 (Farik runs an epic's mechanical criteria for its human reviewer), 5.2 (a stop aborts the session), 5.4 (item 1 for an epic, ADR 0013), 5.7 (answers; resolution with a status and a message; the next session's words), 5.16 (triage model; draft to refining; approval; the epic's assignment, breakdown, close-out, run, and acceptance), F1 (the pause blocks, the resume unblocks), 8.5 (agent.updated)
docs/plans/project-plan.md                           modifies: step 14's interface line
```

## Interfaces

Consumes: `Orchestrator`, `OrchestratorError`, `TickReport`, `stop`, the session mechanics, `orchestrator::fixtures`, `UsageThenWaitAdapter` (step 11); the verifying rules and their per-criterion run (step 12); `integrate`, `awaiting_integration` (step 13); `untrusted_block`, `CLOSING_INSTRUCTIONS`, `PromptInput` (step 10); `DaemonState`, `decide_pre_tool_use` (step 07); `run_criteria`, `SandboxFactory::create_base`, `Git::create_detached_worktree` (step 06); the tools, `file_request`, `RequestError` (step 05); `Transitions`, `TransitionAsk`, `integration_branch` (step 04); `check_human_triage`, `check_contract_write`, `check_product_doc_write`, `requires_human_acceptance`, `Blocker` (`farik-core`); `remove_base_worktree` (step 06, made `pub(crate)` here); `ProjectFiles::write_team`, `Projections::catch_up`, `apply_through` (main, steps 05 and 13).

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
pub(crate) fn contract_accepted(history: &[FarikEvent]) -> bool;                  // transitions
pub(crate) fn reviewed_by_the_human(contract: &TaskContract, team: &Team) -> bool; // transitions
impl DaemonState { pub fn request_stop(&self, session_id: &str, reason: &str) -> bool; pub fn stop_reason(&self, session_id: &str) -> Option<String>; pub fn sessions_of(&self, agent_id: &str) -> Vec<String>; }
pub struct CommandReport { pub said: String, pub events: Vec<u64> }
pub enum CommandError { Invalid { detail: String }, Refused { reason: String }, NotFound { what: String }, Failed { detail: String } }
impl Orchestrator { pub async fn handle(&self, command: Command) -> Result<CommandReport, CommandError>; }
```

## Tasks

The harness is step 11's, its integration `auto_merge` with no remote. `fixtures::a_request(title)` files, by `human` through `file_request`, step 11's FRK-1 contract with that title and no `kind`: an untriaged draft (`assignee_role: software_developer`, `reviewer_role: software_developer`, `allowed_paths: [done.txt]`, C1 `command` `test -f done.txt`, one `out_of_scope` item, risk `low`, `max_cost_usd: 5`). Each transcript has one `result` line and calls exactly these tools:
- `triage_frk_1_large`: `farik_triage_request { size: large, reason: "A file and the check that it exists." }`.
- `refine_asks_frk_1`: `farik_ask_human { question: "Should done.txt be empty?" }`.
- `refine_writes_epic_frk_1`: `farik_write_contract` with `fields` `intent`, `assignee_role: product_manager`, `reviewer_role: human` (readiness lets only an epic name the human), `allowed_paths: [done.txt]`, `exit_criteria` C1 (`command`, `test -f done.txt`) and R1 (`review`, rubric "Does done.txt say what the request asked?"), `scope` with one `out_of_scope` item, risk `low`, and `budget { max_cost_usd: 5, max_sessions: 10 }`.
- `refine_writes_task_frk_1`: `farik_write_contract` with `a_request`'s fields restated.
- `plan_breaks_down_frk_1`: `farik_create_task { parent: FRK-1, contract: <a_request's fields, title "Add done.txt", max_cost_usd: 2> }`.
- `plan_assigns_frk_2`: `farik_assign_task { task_id: FRK-2, assignee_id: dev-a, reviewer_id: dev-b }`.
- `plan_closes_epic_frk_1`: `farik_write_note { kind: completion, text: "FRK-2 added done.txt; nothing left out." }`, then `farik_request_transition { to: verifying }`.

Step 11's `implement_finishes_frk_1` and step 12's `review_writes_note` and `accept_frk_1` name no task, so they replay for FRK-2 and for the epic. A failing contract is seeded by the test calling `call_tool` as `pm` with `farik_write_contract { fields: { budget: { max_cost_usd: 50 } } }`: the schema allows it and the Definition of Ready refuses it against the default `max_task_budget_usd` of 5 (5.12).

### Task 1: the human's triage and lock in the store

Files: `requests.rs`, `crates/cli/src/contract.rs`, `crates/cli/src/triage.rs`

- `records_the_humans_triage` — on a draft, `triage_by_human(.., Large, "Three deliverables.")` writes `kind: epic` to the file and appends `request.triaged { size: large, triaged_by: human }`; the board row is an epic and is triaged.
- `refuses_the_humans_triage_once_refining_started` — on a `refining` task, `Refused` containing `check_human_triage`'s words and nothing appended; a blank reason is `Refused` before anything is read.
- `locks_and_gives_back_a_contract` — `hold_contract(.., true)` sets `locked` and appends `contract.locked { locked_by: human }`; a second call is `Refused` containing `already`; `false` appends `contract.unlocked`.
- The phase 2 tests of `farik triage` and `farik contract lock` pass unchanged.

- [x] `refactor(store): record the human's triage and lock for every caller`

### Task 2: the commands, the events, and the board

Files: the two schemas, `command.rs`, `event.rs`, `event/fixtures.rs`, the migration, `migrations.rs`, `projections.rs`, `docs/SPEC.md` (8.5)

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) — now covers the four new kinds and a `task.transitioned` with `reason`.
- `reads_every_human_command` — each of the ten wires reads into its variant with its fields; `{ "task_id": "FRK-3" }` under `contract_lock`, `contract_unlock`, and `task_integrate` reads as three different variants; `run_stop` with `{}` is `RunStop`; `human_accept` without `message` reads `message: None`.
- `refuses_a_body_that_is_another_commands` — `question_answer` with a `task_integrate` body passes the schema (the body matches `taskIdBody`) and is refused by the reader with exactly one error at `/body` whose message starts `a question_answer command does not carry this body`, as the landed `refuses_a_body_that_belongs_to_another_command` asserts; `human_accept` with `subject: approve` matches no branch and is refused by the schema with exactly one error, at `/body`.
- `waits_on_the_human_while_a_question_is_open` — two `question.asked` on FRK-1: `waiting_on_human`; one answered: still waiting; both answered: not.
- `awaits_approval_from_the_escalation_until_the_next_move` — `escalation.raised { approval }` sets it; `{ risk_gate }` on FRK-2 sets it; `{ iterations }` on FRK-3 does not; FRK-1's `escalated → ready` clears it.
- `reads_an_older_log_into_the_new_columns` — a version-5 database (`apply_through(.., 5, ..)`) holding an `escalated` row whose last escalation is `approval` and a row with a `question.asked`, once opened: the first awaits approval and the second waits on the human.

- [x] `feat(protocol): add the human's commands and the events they record`

### Task 3: the gates read the human

Files: `transitions.rs`, `tools/contracts.rs`, `tools/work.rs` (tests needing git are ignored, as step 04's are)

- `reads_the_approval_of_the_contract_the_task_has_now` — an epic awaiting approval: `acceptance.given` false; after `human.accepted { contract }`, true; after a move into `refining` and a `contract.written`, false.
- `accepts_a_high_risk_task_once_the_human_has` — a `high` risk task in `verifying` with reviewer results and both notes: `pm`'s `accepted` is refused with the `HumanAccepted` message; a `human.accepted { result }` recorded before the last move into `verifying` still leaves it refused; one recorded since lets it move.
- `answers_every_human_criterion_with_one_acceptance` — with `human` criteria H1 and H2, `done.results` holds two passing `Human` results with evidence `human.accepted at seq <n>`.
- `takes_the_humans_acceptance_as_an_epics_review_answers` — an epic reviewed by the human with C1 `command`, R1 `review`, and H1 `human`, a governor result for C1, and `human.accepted { result, message: "Both look right." }`: a passing `Reviewer` result for R1, a passing `Human` result for H1, C1's result still the governor's run, `review_note` equal to the message, and `human_accepted` true. A task with reviewer `dev-b` gains no `Reviewer` result from an acceptance.
- `names_the_human_as_reviewer_of_epics_only_without_a_scrum_master` — `reviewed_by_the_human` is true for an epic on step 11's team, false for a task, and false for an epic once an active Scrum Master is in the team file.
- `records_the_humans_reason_on_their_move` — the human's `in_progress → escalated` with the reason "stopped by the human": `task.transitioned.reason` equals it, and the detail of `escalation.raised { explicit_request }` ends with it.
- `lets_an_epic_be_written_once_its_question_is_answered` — `question_unanswered` while the question is open; after a `question.answered` naming its seq, the write is accepted.
- `writes_a_product_document_once_the_epic_is_approved` — after `human.accepted { contract }` with the epic `ready`: the document is written and `product_doc.written` appended; with the same epic `cancelled`: refused. Step 05's refusal test passes unchanged.

- [x] `feat(runtime): read the human's approvals, acceptances, and answers in every gate`

### Task 4: the human's commands

Files: `orchestrator.rs`, `orchestrator/human.rs`, `docs/SPEC.md` (5.7)

- `answers_a_question_once` — with `question.asked` at seq n on FRK-1, `QuestionAnswer { n, "Yes." }` appends `question.answered { question_id: n, answer: "Yes.", answered_by: human }` with FRK-1 on the envelope, and `events` is `[its seq]`. A second answer is `Refused` starting `already_answered`; the seq of a `task.created` gives `not_a_question`; a seq past the end of the log is `NotFound`; a blank answer is `Invalid`.
- `approves_a_contract_awaiting_approval` — epic FRK-1 awaiting approval: `task.transitioned` `escalated → ready` with `actor: human`, then `human.accepted { subject: contract }`; the board says `ready` and not awaiting. On a `ready` task it is `Refused` starting `not_awaiting_approval`.
- `accepts_a_task_result_only_where_the_human_is_asked` — a `high` risk task in `verifying`: `human.accepted { subject: result }` and no move; a second call is `already_accepted`; a `low` risk task with no `human` criterion is `not_waiting_for_the_human`.
- `accepts_an_epic_only_after_farik_ran_its_criteria` — an epic in `verifying` with C1 `command`: with no governor result, `criteria_not_run`; with a failed one, `criterion_failed`; with a passing one and no message, `Invalid`; with a message, `human.accepted` carrying that message.
- `resolves_an_escalation_with_a_status_and_a_message` — `EscalationResolve { refining, "Split it by page." }` records `escalated → refining`, then `escalation.resolved { to: refining, message, resolved_by: human }`. `to: ready` while awaiting approval is `use_human_accept`; on a `ready` task, `not_escalated`; a blank message, `Invalid`; a move the governor refuses appends no `escalation.resolved`.
- `moves_a_task_for_the_human` — `blocked → in_progress` with "Key rotated.": both `blocker_resolution` and `reason` equal it; `→ cancelled` moves; from `escalated` it is `use_escalation_resolve`; `in_progress → accepted` is `Refused` starting `transition_refused` with the governor's details, and the log holds the `transition.refused`.
- `locks_triages_and_integrates_through_one_door` — `ContractLock` and `ContractUnlock` append their events; `RequestTriage` appends `request.triaged { triaged_by: human }`; `TaskIntegrate` on an accepted task under `manual` appends `task.integrated { integrated_by: human }` with `said` containing `merged`; `TaskCreate` is `Invalid`.
- `stops_the_run_through_handle` — `RunStop` answers with `events` empty, and a following `run_until_idle` runs no tick (step 11's `stop`, which never aborts a session, is what it calls; step 11 tests that).

- [x] `feat(runtime): handle the human's commands`

### Task 5: stopping and pausing

Files: `daemon.rs`, `daemon/hooks.rs`, `orchestrator/session.rs`, `orchestrator/human.rs`, `docs/SPEC.md` (5.2, F1)

- `denies_every_call_of_a_stopped_session` — after `request_stop(s, "stopped by the human")`, `stop_reason(s)` is that reason and a pre-tool-use for `s` is denied `session_stopped: stopped by the human`; `request_stop` of an unregistered id returns false.
- `stops_the_session_of_an_agent_paused_in_the_team_file` — with `dev-a` paused by writing the team file, the next hook is denied `agent_not_active` and `stop_reason` is "agent paused by the user".
- `aborts_the_session_of_an_agent_paused_mid_session` — FRK-1 `in_progress` with `UsageThenWaitAdapter` waiting, and `AgentUpdate { dev-a, paused }` handled from a spawned task: `abort` is called once; the log holds `agent.updated { dev-a, paused, human }`, `session.ended { reason: aborted }`, and `in_progress → blocked` with the blocker "agent paused by the user", and no `team.updated`; the team file says `paused`; the next tick is `Idle`.
- `resumes_what_the_pause_blocked` — then `AgentUpdate { dev-a, active }`: FRK-1 goes `blocked → in_progress` with `actor: human` and `blocker_resolution: "agent resumed by the user"`, while FRK-2, blocked by `dev-a` for another reason, stays `blocked`. The same update again is `same_status`, and an unknown agent is `NotFound`.
- `stops_a_running_session_and_escalates_its_task` — `SessionStop` of the waiting session: `abort` is called once, the log holds `session.ended { aborted }`, and FRK-1 is `escalated` with an `escalation.raised { explicit_request }` whose detail ends "stopped by the human". An unknown id is `NotFound`.

- [ ] `feat(runtime): stop a session and pause an agent at the next hook`

### Task 6: triage and refining

Files: `orchestrator/requests.rs`, `rules.rs`, `session.rs`, `messages.rs`, `fixtures.rs`, the triage and refine transcripts

- `triages_a_request_on_the_cheaper_model` — `a_request` with `triage_frk_1_large`: one spec, with `purpose: Triage`, `agent_id: pm`, `model: TRIAGE_MODEL`, effort `low`, `farik_tools == ["farik_triage_request"]`, and empty `builtin_tools`; the log holds `request.triaged { large, triaged_by: pm }` and the board says epic.
- `starts_refining_a_triaged_request_without_a_session` — the next tick records `draft → refining` with `actor: product_manager` and `requested_by: pm`, and starts no spec.
- `acts_for_no_agent_that_is_paused` — `pm` paused: a triaged draft stays `draft` and the tick is `Idle`; with `dev-a` paused, FRK-2 `assigned` to `dev-a` is not moved and no spec starts.
- `asks_first_when_refining_an_epic` — the next tick, with `refine_asks_frk_1`: `purpose: Refine`, and the first message contains `farik_ask_human`; the log holds `question.asked`; the tick after is `Idle` with no spec.
- `hands_the_answer_to_the_next_refine_session` — after `QuestionAnswer { n, "Yes." }`, with `replays_farik_read_board` as the refine session (it writes nothing): its system prompt contains `Question <n>:`, then `<untrusted source="question">` around the question, then `Answer: Yes.`; the refine session started by the next tick, with nothing answered since, contains neither.
- `judges_a_written_contract_before_refining_again` — a standalone task with `refine_writes_task_frk_1`: the next tick records `contract.evaluated { passed: true }` and `refining → ready` with `requested_by: governor`, and starts no spec.
- `returns_a_failing_contract_with_its_failures` — the seeded failing write: the tick records `contract.evaluated { passed: false }` whose failures contain `exceeds the team's cap of 5 USD`; the next tick's refine session has a first message containing `exceeds the team's cap of 5 USD`.
- `escalates_an_epic_for_approval_once_it_passes` — `refine_writes_epic_frk_1`: the next tick records `refining → escalated` and `escalation.raised { approval }`, and no `contract.evaluated { passed: false }`; the board awaits approval; the tick after is `Idle`.
- `escalates_a_contract_that_failed_three_times` — three `contract.evaluated { passed: false }` appended since refining began, then the seeded failing write: the tick records `escalation.raised { readiness_failures }`.
- `judges_a_child_filed_whole_without_a_session` — FRK-2 filed by `plan_breaks_down_frk_1` under an `in_progress` epic: two ticks record `draft → refining` and `refining → ready`, and no spec names FRK-2.
- `passes_over_a_task_waiting_on_the_human` — FRK-1 `ready` with an open question and FRK-2 `ready`: the tick's plan session is FRK-2's.
- `catches_up_with_another_process_before_each_tick` — a `question.answered` appended with `EventLog::append` alone, as another process would: the next tick starts FRK-1's refine session.

- [ ] `feat(runtime): triage requests and refine their contracts`

### Task 7: epics from approval to acceptance

Files: `orchestrator/requests.rs`, `rules.rs`, `messages.rs`, `prompt.rs`, `criteria.rs`, `sandbox.rs`, the plan transcripts, `docs/SPEC.md` (5.1, 5.4, 5.16)

- `assigns_an_approved_epic_to_its_product_manager` — an epic `ready`: one tick records `ready → assigned` with `actor: product_manager`, assignee `pm`, no reviewer, and no spec; the next records `assigned → in_progress`, and `.farik/local/worktrees/FRK-1` does not exist.
- `waits_to_assign_a_second_epic_while_the_first_is_open` — two epics `ready` on a team with a WIP limit of 1: the first tick assigns FRK-1, and the next two ticks are FRK-1's `in_progress` move and then `Idle` rather than a refused assignment of FRK-2; the log holds no `transition.refused` for FRK-2.
- `breaks_an_epic_down_in_a_plan_session` — with `plan_breaks_down_frk_1`: `purpose: Plan`, `agent_id: pm`, `task_id: FRK-1`, `cwd` the root; FRK-2 is on the board with `parent: FRK-1`, triaged, `draft`.
- `assigns_an_epics_task_through_its_assignee` — FRK-2 `ready`: `plan_assigns_frk_2`'s session belongs to `pm` with `task_id: FRK-2`, and FRK-2 is `assigned` to `dev-a` with reviewer `dev-b`.
- `leaves_an_epic_alone_while_its_tasks_are_open` — FRK-2 `in_progress`: the tick acts on FRK-2, and no spec names FRK-1.
- `closes_an_epic_whose_tasks_are_done` — FRK-2 `accepted` and integrated, with `plan_closes_epic_frk_1`: its first message contains `FRK-2` and `accepted`, and FRK-1 gains `note.written { completion }` and `in_progress → verifying`.
- `breaks_down_again_when_every_task_was_cancelled` — FRK-2 `cancelled`: the next plan session's first message is the breakdown's.
- `waits_for_an_epics_tasks_to_be_integrated` — FRK-1 `verifying` under `manual` with FRK-2 awaiting integration: `Idle`, no `criterion.recorded`, and no `FRK-1-base` worktree.
- `runs_an_epics_criteria_on_the_integration_branch` — FRK-2 integrated with `done.txt` on `main`: one tick records `criterion.recorded { criterion_id: C1, passed: true, run_by: reviewer, recorded_by: governor }` for FRK-1; `CountingSandboxFactory` saw one `create_base` for FRK-1; afterwards `.farik/local/worktrees/FRK-1-base` is gone; the next tick is `Idle` and runs nothing again.
- `hands_the_humans_acceptance_to_the_product_manager` — then `HumanAccept { FRK-1, result, "Both look right." }`: `pm`'s `verify` spec has `cwd` the root and a registration with no executor, its system prompt contains `The human, accepting the result: Both look right.` unwrapped, and its first message contains C1's evidence inside `untrusted_block`; `accept_frk_1` moves FRK-1 to `accepted`, the session's end appends `review.recorded { reviewer: human, criteria_run: 2, passed: true }` for FRK-1, and there is no `task.integrated` for FRK-1.
- `runs_an_epics_test_criterion_without_the_new_tests_check` — the epic's C1 a `test` criterion `test -f done.txt` with `new_tests_required: true` (the team's `require_new_tests` on): with `done.txt` on `main`, the run records C1 passed, its evidence starting `at <main's head sha>` and containing no `base-branch check`.
- `sends_a_failed_epic_back_for_more_work` — C1 failed on the run: `HumanAccept { result }` is `criterion_failed`; after `TaskTransition { FRK-1, escalated, "C1 failed" }` and `EscalationResolve { in_progress, "Add the missing file." }`, the next spec is `pm`'s `plan` session for FRK-1, whose system prompt contains `Add the missing file.`
- `accepts_a_high_risk_task_after_the_human` — step 12's `leaves_a_high_risk_task_for_the_human` continued: after `HumanAccept { result }`, `pm`'s `verify` session runs and `accept_frk_1` accepts the task.

- [ ] `feat(runtime): take an approved epic through its breakdown to acceptance`

### Task 8: one request to an accepted epic, end to end

Files: `crates/runtime/tests/one_epic.rs`, `docs/plans/project-plan.md`

- `takes_one_request_to_an_accepted_epic` — `a_request("Add done.txt and its check")` with these transcripts, in order: `triage_frk_1_large`, `refine_asks_frk_1`, `refine_writes_epic_frk_1`, `plan_breaks_down_frk_1`, `plan_assigns_frk_2`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`, `plan_closes_epic_frk_1`, `accept_frk_1`. The test runs `run_until_idle`; `QuestionAnswer` of the one question with "No, one line."; `run_until_idle`; `HumanAccept { FRK-1, contract }`; `run_until_idle`; `HumanAccept { FRK-1, result, "done.txt is on main." }`; `run_until_idle`.
  - Afterwards FRK-1 is an `accepted` epic, FRK-2 is an `accepted`, integrated task under it, and `main` holds `done.txt`.
  - The log holds ten `session.started`, in this order among themselves: `triage`/`pm`; `refine`/`pm` twice; `plan`/`pm` for FRK-1; `plan`/`pm` for FRK-2; `implement`/`dev-a`; `verify`/`dev-b`; `verify`/`pm` for FRK-2; `plan`/`pm` for FRK-1; `verify`/`pm` for FRK-1.
  - It also holds, in this order among themselves: `request.triaged`, `question.asked`, `question.answered`, `escalation.raised { approval }`, FRK-1's `escalated → ready`, `human.accepted { contract }`, FRK-1's `ready → assigned → in_progress`, FRK-2's `task.created`, `task.integrated` for FRK-2, FRK-1's `in_progress → verifying`, the governor's `criterion.recorded` for FRK-1's C1, `human.accepted { result }`, and FRK-1's `verifying → accepted`.
  - There is no `contract.evaluated { passed: false }`, and the adapter has no transcript left.

- [ ] `test(runtime): take one request to an accepted epic end to end`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
