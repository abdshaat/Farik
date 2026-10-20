# Autopilot, plan 2: the switch, the team's questions and decisions, Today

Status: ready
Branch: `feat/autopilot`, after plan 1
Spec and decisions: see `README.md`; ADR 0082 decisions 1 to 4, 8 (goals), 10, 11

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/runtime/src/` unless they say otherwise.

## Decisions

- `autopilot.set` is built as `merging.set` is (`daemon/team.rs` `set_merging`, near line 2010): `METHODS` becomes `[&str; 21]` with `"autopilot.set"` after `"merging.set"`; the handler runs off the worker under `team_writes()`. `on: true`: without `confirmed: true` refused `autopilot_unconfirmed`; with `team.budgets.daily_usd` `None` refused `daily_limit_required`; already on, it records nothing; else it appends `autopilot.changed { on: true }` and wakes the orchestrator. `on: false`: already off records nothing; else appends `{ on: false }`. It answers `{ autopilot }` read after. It never writes `team.yaml` (decision 1).
- `checked()` (`daemon/team.rs` near 1766), which every `team.save`, `agent.replace` and template application goes through, refuses `daily_limit_required` at `/budgets/daily_usd` when `autopilot_on(&deps.log)` and the saved team has no `daily_usd`. `team.get` answers `autopilot: autopilot_on(..)`.
- The decide session: `SessionPurpose::Decide` (`session.rs`), about the task it decides (about the request's task for a data pipeline, plan 3), `read_only: true`, `executor: None`, `tools: Some(<list>)`, run by `run_session` as `goal_session` runs one (`orchestrator/goal.rs` near 168). Its closing instruction, `CLOSING_INSTRUCTIONS` (`prompt.rs`, becomes `[_; 11]`): "This session decides for the team what would otherwise wait on the owner; the first message says what waits and what you may choose. Read what you need, weigh the options, and answer once, with your reason in one or two sentences, using the tool the first message names. Flag only what only the owner can give: a sign-in, a key, a connector to connect, a confirmation of a skill's or connector's hash, a purchase to place, a legal or account step, or money above the day's limit. Change nothing in the project. Then end the session." A decide session does not count against the task's sessions (ADR 0079 decision 6 counts refine, implement and verify alone).
- What an agent is told: `PromptInput` gains `autopilot: bool`, set to `team_decides(log, team)` by the session's builder. When true, the `Team rules` body ends with one paragraph:
  - to the Product Manager and a solo agent, `AUTOPILOT_DECIDE`: "Autopilot is on: the owner is asked nothing. Where your role or a skill says to ask the owner or the user with `catervas_ask_human`, decide yourself: weigh the options, write your decision and its reason with `catervas_write_note` of kind `progress`, and go on. Block a task only for what only the owner can give: a sign-in, a key, a connector to connect, a purchase, a legal or account step, or money above the day's limit.";
  - to every other agent, `AUTOPILOT_ASK`: "Autopilot is on: the owner is asked nothing. Where your role or a skill says to ask the owner or the user with `catervas_ask_human`, ask the team with `catervas_ask_team` instead, `about` `product` for what the product should do and `technical` for how to build it, then end your turn: the answer starts your next session. Block a task only for what only the owner can give: a sign-in, a key, a connector to connect, a purchase, a legal or account step, or money above the day's limit."
  The role files and skills keep `catervas_ask_human` (autopilot is off by default); the paragraph is what overrides them. Rejected: editing every `system.md` and skill, which would change the owner-asking default.
- Which tools are offered (`orchestrator/session.rs` `offered_tools`, near 1090, gains `team_decides: bool`, computed by its caller): when true, `catervas_ask_human` is offered to no session, and `catervas_ask_team` wherever `catervas_ask_human` would have been, except to the Product Manager and a solo agent; when false, `catervas_ask_team` is offered to none. `catervas_answer_question`, `catervas_decide` and `catervas_decide_data_pipeline` are offered only to a `decide` session (and there only through its `tools` list). In `tools.rs` `TOOLS` (near 190), `catervas_ask_team` follows `catervas_ask_human`, and the three others follow `catervas_record_goal_progress`; all `Read`.
- A team question: `catervas_ask_team` (`tools/work.rs`, beside `ask_human` near 357) appends `team.question_asked` about the session's task. Its answer goes everywhere an owner's answer goes: `orchestrator/messages.rs` `human_message` (near 403) shows a `team.question_answered` as "Question <seq>, asked of the team:" with the question and the answer each in an `untrusted_block` (an agent's words), and an owner's `question.answered` finds its question among `team.question_asked` too; `orchestrator/requests.rs` near 192 counts `team.question_asked` as a question asked; `tools/contracts.rs` `has_asked` (near 397) reads both pairs of kinds.
- The owner can still answer: `orchestrator/human.rs` `answer_question` (near 838) accepts the seq of a `team.question_asked` and refuses `already_answered` for one with either answer; `daemon/gates.rs` `questions` (near 719) lists open team questions as it lists the owner's; `store/src/waiting.rs` lists an open team question as a `Question` row only while the team does not decide (decision 1: "waits on the owner again").
- The autopilot rule (`orchestrator/autopilot.rs`, created), called in `rules.rs` `tick` right after `budget_and_channel` and before rule 3, only when `scope.rules == TickRules::All`, `scope.task_id` is `None` and `team_decides`. Its order, one session per tick: (1) open team questions, oldest first; (2) what waits for a decision on a task (Tasks 10 and 11: escalations, blocks past the hour, results awaiting acceptance), oldest first by when the wait began; (3) folder changes whose integration failed (Task 11), oldest first; (4) data pipelines (plan 3). Every wait is passed over when its task is flagged, when its decider is busy (`deps.work.is_busy`), asleep (`unavailable`), or the day is spent (`day_is_spent`). A wait with `DECIDE_SESSIONS_PER_WAIT` `decide` sessions about its task since it began and still open is flagged by Catervas (`task.flagged { flagged_by: "catervas" }`, needs "an answer to: <question, cut to 200 characters>" or "a decision the team could not make"), so a loop of sessions that decide nothing ends (planner's bound).
- Who decides: questions by `core::autopilot::answerer`; everything else by the planner (`crate::goals::planner`: the active Product Manager, or the one agent in solo).
- `catervas_decide` (`tools/decide.rs`, created) reads a task's wait through `crate::choices::waits_of` (extracted from `offer_for`, which then calls it: the reason, `before` and `blocked_from` it already computes), or, for a `verifying` task with none, `awaits_acceptance` (reason `acceptance`), a folder change's as Task 11 says; it checks the choice with `choices_for(.., team_spent(..))`, then:
  - a move: `task.room_granted { granted_by: <agent> }` first for `more_room`; asks the governor through `tools/work.rs` `ask_governor` (near 441, made `pub(super)`) as `TransitionActor::ProductManager`, which names the calling agent, and a `TransitionAsk { reason, grants_tries, blocker_resolution }` as `human.rs` `resolve` builds it, the text being `answer` for `answer_block` and `reason` otherwise; on a move, appends `escalation.resolved { to, message, resolved_by: <agent>, extra_tries }`; a refusal records nothing and returns the gate's words;
  - `reopen_integration`: nothing but the decision (plan 3's tick merges again);
  - `flag`: `task.flagged { needs, flagged_by: <agent> }` after the decision.
  Each ends with `team.decided`. `TransitionContext.autopilot` is set from `autopilot_on(&self.log)` in `transitions.rs` `context`, replacing plan 1's `false`. The next session is told the decision: `human_message` shows an `escalation.resolved` whose `resolved_by` is not `human` as "<name> decided for the team, moving this to <to>:" and the message in an `untrusted_block`.
- A block waits `BLOCK_WAIT_MINUTES` from its last move into `blocked` before its decide session (decision 3). Rule 4 (`rules.rs` `blocked`, near 1274) passes over a flagged task while the team decides, and the aged rule (`aged`, near 1148) passes over every escalation while the team decides: neither waits on the owner.
- Today (`store/src/waiting.rs` `waiting`, near 342): while the team decides, no `Help` row for an escalated or blocked task; `autopilot.today` (`daemon/gates.rs` `query`) answers `on`, `flagged` (`flags` with each task's title, then a stalled goal's row while autopilot is on: `goal`, its words as `title`, needs "The last planning round found nothing more to do." or "Five planning rounds since the last progress.", `at` the newest `goal.planned`'s time) and `done` (`done_on_its_own`). A stalled goal is otherwise as ADR 0075 says (decision 8).
- What was already waiting on the owner when autopilot was turned on (a connector call's approval, a marketing plan, a seller message) keeps waiting on the owner (planner's: the switch acts on what comes after it).
- Approvals, acceptances and the rest of decision 11 (Task 11):
  - `approve_plan` (reasons `approval`, `risk_gate`, while the row is `awaiting_approval`): `human.accepted { subject: contract, accepted_by: <agent>, message: <reason> }`, then `escalated -> ready` through `ask_governor` as the Product Manager (the `Autopilot` twin of the owner's `escalated -> any`), as `human.rs` `approve` (near 1573) does for the owner.
  - `accept_result` (the `acceptance` wait: a `verifying` task whose result waits on the owner, by the same test `waiting.rs` uses for its `Acceptance` row, extracted as `catervas_store::waiting::awaits_acceptance`): the owner's checks of `human.rs` `accept_result` (near 1608), then `human.accepted { subject: result, accepted_by: <agent>, message: <reason> }`; no move, as for the owner. `mechanical_criteria_passed` moves from `human.rs` to `transitions.rs` beside `result_accepted`, returning its refusal as `String`, so both callers share it. The wait begins at the task's last move into `verifying`.
  - `carry_on`: the owner's "Carry on", a move to `before`, as the other moves are.
  - `retry_folder_change`: a `decide` session about no task (`contract: None`) is given each escalated, not integrated folder change (`folder_docs`), with `DECIDE_TOOLS`; its first message names the change, its documents, and the escalation's `detail` in an `untrusted_block`. `catervas_decide { folder_change }` checks it is escalated and not flagged, offers `choices_for("folder_change", None, None, folder_change_spent(..))`, and records `team.decided { choice, folder_change }`, which makes the next tick integrate it again (plan 1's fold). Its `flag` is `team.decided { choice: flag, needs, folder_change }`, with no `task.flagged` (there is no task). Its three-session bound counts the `decide` sessions about no task started since its `folder_change.escalated`; Catervas's own flag of it is `team.decided { choice: flag, decided_by: "catervas", folder_change }`. Chose a decide session with an argument of the one tool over a tool of its own: one decision tool, one prompt.
  - "Fix how your app opens" is a Settings change: the `preview` reason offers `flag` for it (decision 11).
- Today, beyond the help rows: while the team decides, `waiting.rs` lists no `Approval` and no `Acceptance` row, and `tools/folders.rs` `integration_rows` (near 381) no row for an escalated folder change; a flagged folder change is a `flagged` row of `autopilot.today` with no `task_id`, its title "Documents change <n>".


## File map

```
docs/schemas/rpc.schema.json        modifies: autopilot.set, teamGetResult.autopilot (T7); autopilot.today (T12)
daemon/team.rs                      modifies: METHODS, set_autopilot, checked(), team.get; tests (T7)
session.rs                          modifies: SessionPurpose::Decide and its wire (T8)
prompt.rs                           modifies: PromptInput.autopilot, the two paragraphs, Decide's closing; tests (T8)
orchestrator/session.rs             modifies: offered_tools(team_decides), PromptInput.autopilot (T8)
tools.rs                            modifies: four tools, dispatch; lists_every_tool_with_its_tier (T8, T9, T10)
tools/work.rs                       modifies: ask_team; tests (T8); ask_governor pub(super) (T10)
tools/decide.rs                     creates: answer_question (T9), decide (T10); approvals, acceptances, carry on, folder changes (T11); tests
orchestrator/autopilot.rs           creates: the autopilot rule (T9, T10, T11); tests
orchestrator.rs                     modifies: mod autopilot (T9)
orchestrator/rules.rs               modifies: tick calls it (T9); blocked and aged pass over (T10); tests (T8)
orchestrator/messages.rs            modifies: team answers (T9), team resolutions (T10)
orchestrator/requests.rs, tools/contracts.rs  modifies: team questions count as asked (T9)
orchestrator/human.rs               modifies: answer_question takes a team question (T9); mechanical_criteria_passed moved out (T11)
choices.rs                          modifies: waits_of extracted from offer_for (T10)
transitions.rs                      modifies: context's autopilot (T10); mechanical_criteria_passed (T11)
daemon/gates.rs                     modifies: questions lists team questions (T9); autopilot.today (T12); tests
tools/folders.rs                    modifies: no escalated folder change row while the team decides (T12)
store/src/waiting.rs (crates/store) modifies: team questions as rows (T9); awaits_acceptance extracted (T11); no help, approval or acceptance rows while the team decides (T12)
```

## Interfaces

Consumes: plan 1's `DecideChoice`, `choices_for`, `move_of`, `answerer`, `Spent`, constants, `autopilot_on`, `team_decides`, `flags`, `open_team_questions`, `team_spent`, `folder_change_spent`, `done_on_its_own`, `GateId::Autopilot`, the folder-change retry fold; on main `run_session`, `SessionAsk`, `open_escalations` (`ceremonies.rs`), `planner` (`goals.rs`), `goal_book`, `day_is_spent`, `unavailable`, `return_from_block`, `ask_governor` (`tools/work.rs`), `untrusted_block`, `reason_in_words`, `folder_docs` (`store/src/folder_docs.rs`), `approve` and `accept_result` (`orchestrator/human.rs`, as the model of Task 11).

Produces:

```rust
// session.rs
pub enum SessionPurpose { /* existing */ Decide }
// prompt.rs
pub struct PromptInput<'a> { /* existing */ pub autopilot: bool }
pub const AUTOPILOT_DECIDE: &str; pub const AUTOPILOT_ASK: &str;
// tools/work.rs
#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]
pub(crate) struct AskTeamInput { question: String, about: QuestionAboutInput }   // enum, snake_case
// tools/decide.rs
pub(crate) struct AnswerQuestionInput { question_seq: u64, answer: String }
pub(crate) struct DecideInput { #[serde(default)] task_id: Option<String>, #[serde(default)] folder_change: Option<u64>,
    choice: String, reason: String, #[serde(default)] answer: Option<String>, #[serde(default)] needs: Option<String> }
pub(super) fn answer_question(call: &Call<'_>, input: &AnswerQuestionInput) -> Result<Value, ToolError>;
pub(super) fn decide(call: &Call<'_>, input: &DecideInput) -> Result<Value, ToolError>;
// choices.rs
pub(crate) struct Waits { pub reason: String, pub before: Option<TaskStatus>, pub blocked_from: Option<TaskStatus> }
pub(crate) fn waits_of(log: &EventLog, projections: &Projections, task_id: &TaskId) -> Result<Option<Waits>, StoreError>;
// transitions.rs (moved from orchestrator/human.rs)
pub(crate) fn mechanical_criteria_passed(contract: &TaskContract, history: &[CatervasEvent]) -> Result<(), String>;
// crates/store/src/waiting.rs (extracted from waiting's Acceptance rows)
pub fn awaits_acceptance(log: &EventLog, files: &ProjectFiles, row: &TaskProjection) -> Result<bool, StoreError>;
// orchestrator/autopilot.rs
pub(super) const DECIDE_QUESTION_TOOLS: &[&str];   // the five reads, catervas_write_decision, catervas_post_message,
                                                   // catervas_write_memory, catervas_answer_question
pub(super) const DECIDE_TOOLS: &[&str];            // the same with catervas_decide in place of catervas_answer_question
pub(super) async fn autopilot_rule(deps: &OrchestratorDeps, scope: &TickScope, team: &Team,
    board: &[TaskProjection], waiting: &mut Waiting) -> Result<Option<TickReport>, OrchestratorError>;
```

## Tasks

### Task 7: the owner turns autopilot on, with a daily limit

Files: `docs/schemas/rpc.schema.json`, `daemon/team.rs`
Tests (`daemon/team.rs` `mod tests`):
- `autopilot_turns_on_only_confirmed_and_with_a_daily_limit` (the guarantee of decision 1) — on a team with no `daily_usd`, `{ on: true, confirmed: true }` is refused `daily_limit_required` at `/budgets/daily_usd` and nothing is appended; with `daily_usd: 5`, `{ on: true }` is refused `autopilot_unconfirmed` at `/autopilot`; `{ on: true, confirmed: true }` appends one `autopilot.changed { on: true }`, leaves `team.yaml`'s bytes unchanged and makes `team.get` answer `autopilot: true`; a `team.save` without `daily_usd` is then refused `daily_limit_required`; `{ on: false }` appends `{ on: false }`, and a second `{ on: false }` appends nothing.

- [x] `feat(runtime): the owner turns autopilot on with a daily limit`

### Task 8: under autopilot agents ask the team, not the owner

Files: `session.rs`, `prompt.rs`, `orchestrator/session.rs`, `tools.rs` (`catervas_ask_team` only), `tools/work.rs`, `orchestrator/rules.rs` (test)
Tests:
- `rules.rs` `mod tests`, with its `tools_of`: `offers_the_team_and_not_the_owner_under_autopilot` — with autopilot on, a Developer's implement session is offered `catervas_ask_team` and not `catervas_ask_human`, and its system prompt's Team rules end with `AUTOPILOT_ASK`; the Product Manager's refine session is offered neither and its prompt holds `AUTOPILOT_DECIDE`; with autopilot off the Developer is offered `catervas_ask_human` and not `catervas_ask_team`, and no prompt holds either paragraph.
- `tools/work.rs`: `asks_the_team_and_holds_the_task` — under autopilot a Developer's `catervas_ask_team` on `CTV-1` appends `team.question_asked` with its `about` and the row is `waiting_on_human`; the same call is refused `autopilot_off` with autopilot off, `decide_yourself` from the Product Manager, and `no_task` from a session about no task.

`CLOSING_INSTRUCTIONS`' length, `[_; 11]`, is what makes the compiler name a purpose left without its closing; no test of its own.

- [x] `feat(runtime): under autopilot agents ask the team, not the owner`

### Task 9: the team answers its own questions

Files: `tools.rs`, `tools/decide.rs`, `orchestrator.rs`, `orchestrator/autopilot.rs`, `orchestrator/rules.rs` (the call), `orchestrator/messages.rs`, `orchestrator/requests.rs`, `tools/contracts.rs`, `orchestrator/human.rs`, `daemon/gates.rs` (`questions`), `crates/store/src/waiting.rs` (`unanswered`)
Message of a question's session: "<answerer>, <asker> asked the team a <product | technical> question about <task> “<title>”:", the question in an `untrusted_block`, then "Answer it with `catervas_answer_question` (question <seq>), as the owner would: decide, and say why in the answer."
Tests:
- `orchestrator/autopilot.rs`: `the_architect_answers_a_technical_question_and_the_task_resumes` — under autopilot, with `CTV-1` in progress and the Developer's open technical question, a tick runs one `decide` session of the Architect offered exactly `DECIDE_QUESTION_TOOLS`; its recorded call of `catervas_answer_question` appends `team.question_answered` about `CTV-1`; the row is no longer `waiting_on_human`; the next implement session's `From the human` holds the answer; with autopilot off the same tick runs no decide session and `waiting.list` holds the question as a row.
- `tools/decide.rs`: `answers_only_an_open_question_put_to_it` — the Developer answering is refused `not_yours`, a second answer `already_answered`, a `question.asked` seq `not_a_team_question`; the owner's `question_answer` of the same open team question is accepted once.

- [x] `feat(runtime): the team answers its own questions`

### Task 10: the Product Manager takes the owner's decisions

Files: `tools.rs`, `tools/decide.rs`, `choices.rs`, `orchestrator/autopilot.rs`, `orchestrator/rules.rs` (`blocked`, `aged`), `orchestrator/messages.rs`, `transitions.rs`
Message of a decision's session: the task, its title and status, the reason in words (`reason_in_words`), the escalation's `detail` or the blocker's words in an `untrusted_block`, the iteration and sessions spent, and each offered choice as "`<wire>`: <meaning>", then "Decide with `catervas_decide`."
Tests:
- `orchestrator/autopilot.rs`: `the_product_manager_decides_an_escalation_with_the_owners_choices` — an `iterations` escalation under autopilot gets one `decide` session of the Product Manager offered exactly `DECIDE_TOOLS`, whose first message names `more_tries`, `back_to_plan`, `cancel` and `flag`; its `catervas_decide { choice: "more_tries" }` moves the task to `in_progress` as actor `product_manager` with gate `autopilot`, appends `escalation.resolved { resolved_by: "pm", extra_tries: 2 }` and `team.decided`, and the task's iteration limit grows by 2 as the owner's would; after three team grants on the task, `more_tries` is refused `choice_not_offered`.
- `orchestrator/autopilot.rs`: `a_block_waits_an_hour_and_a_flag_holds_it_for_the_owner` — a task blocked 59 minutes ago starts no decide session, at 60 minutes one; `catervas_decide { choice: "flag", needs: "a Stripe key" }` appends `task.flagged`, and later ticks start no decide session for it and rule 4 does not escalate it; a second blocked task whose three decide sessions end without a decision is flagged by `catervas`; `answer_block` without `answer` is refused `answer_required`.

- [x] `feat(runtime): the product manager takes the owner's decisions`

### Task 11: the Product Manager approves, accepts, carries on and retries folder changes

Files: `tools/decide.rs`, `orchestrator/autopilot.rs`, `orchestrator/human.rs`, `transitions.rs`, `crates/store/src/waiting.rs` (`awaits_acceptance` only)
Message of an acceptance's session: the task and its title, that its review passed (or that it is an epic, with its tasks), its criteria and each recorded result, then "Accept it with `catervas_decide` (`accept_result`) once you have read the work, or flag it."
Tests (`orchestrator/autopilot.rs`):
- `the_product_manager_approves_a_plan_and_accepts_a_result_for_the_owner` — under autopilot an epic escalated `approval` gets a decide session offering `approve_plan`, `back_to_plan`, `cancel`, `flag`; `approve_plan` appends `human.accepted { subject: contract, accepted_by: "pm" }` and moves it to `ready` with gate `autopilot`; a high-risk task in `verifying` whose review passed gets a decide session, and `accept_result` appends `human.accepted { subject: result, accepted_by: "pm" }`, after which the Product Manager's `verifying -> accepted` passes the Definition of Done (`DoneEvidence.human_accepted` read from it by `with_the_humans_acceptance`, transitions.rs near 1890, unchanged), as after the owner's `accept_result`; a second `accept_result` is refused `already_accepted`; neither row is in `waiting.list`.
- `the_product_manager_carries_on_and_retries_a_folder_change` — a `permission` escalation from `in_progress` offers `carry_on` first and `carry_on` moves the task back to `in_progress`; a `preview` escalation offers `carry_on`, `cancel`, `flag` and no Settings choice; an escalated folder change gets a decide session about no task, `catervas_decide { folder_change, choice: "retry_folder_change" }` records `team.decided` naming it and the next tick opens its integration again; `catervas_decide` with both `task_id` and `folder_change` is refused `name_one`; after three retries only `flag` is offered, and its flag is a `flagged` row with no task.

- [ ] `feat(runtime): the product manager approves, accepts, carries on and retries`

### Task 12: Today shows what the team flagged and did on its own

Files: `docs/schemas/rpc.schema.json`, `daemon/gates.rs`, `crates/store/src/waiting.rs`, `tools/folders.rs`
Tests (`daemon/gates.rs` `mod tests`):
- `today_shows_flags_and_acts_and_no_help_under_autopilot` — under autopilot with a Product Manager: an escalated task, a blocked one, a contract awaiting approval, a result awaiting acceptance and an escalated folder change are not in `waiting.list`; a flagged task is in `autopilot.today`'s `flagged` with its `needs` and `by`; a `team.decided` of today is in `done` with its `reason`; a stalled goal is a `flagged` row with its `goal`; after `autopilot.set { on: false }`, `waiting.list` holds the escalated, blocked and flagged tasks as `help` rows and `autopilot.today` answers `on: false` and no goal row.

- [ ] `feat(runtime): today shows what the team flagged and did on its own`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo test -p catervas-runtime -p catervas-store
# expected: test result: ok
```
