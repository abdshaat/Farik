# Autopilot, plan 3: money, outward acts, merging and idle agents

Status: ready
Branch: `feat/autopilot`, after plan 2
Spec and decisions: see `README.md`; ADR 0082 decisions 5 to 9, 11 (merging with no checks), 12

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths are under `crates/runtime/src/` unless they say otherwise.

## Decisions

- Money (decision 5) follows `autopilot_on`, not `team_decides`: a paid call needs no decider. Every other act here that needs a decider (data pipelines) follows `team_decides`.
- A paid or `ask` connector call (`daemon/hooks.rs` `judge_connector`, near 629): it passes `autopilot_on(&deps.log)` to `evaluate_connector_call`; on a pass with `autopilot_approved`, it appends `tool.approved { server, tool, input, input_sha256, by: "autopilot" }` (the call's whole input as compact JSON, as `ask` builds `tool_approval.requested`, near 797) with the session's ids, before the hook records `tool.called`. No grant is looked up and no allowance is counted under autopilot. The day's limit is the stop (ADR 0015, ADR 0064), unchanged.
- The rate of those calls (decision 12), in the same path, before `tool.approved` is appended: `recent_paid_calls(deps, agent, per_hour)` reads the agent's `tool.approved` events with one `EventQuery { agent_id, kinds: [ToolApproved], newest_first: true, limit: Some(per_hour) }` and maps each to a `RecentCall` (its body's server, tool and `input_sha256`, its envelope's time); `rate_limit(.., team.paid_calls_per_hour(), deps.clock.now())` judges the call. The `per_hour` newest are enough: when all of them are in the hour, the call is over the all-tools limit whatever else, and when one is older, every call of the hour is among them; so the read under the sessions lock stays at most 1000 rows. Counting from the log is what makes the count survive a restart (decision 12). A refusal is a `Denial` whose reason is, by `RateLimit`, with the time as `HH:MM` UTC:
  - `SameInput`: "rate_limited: you have called <tool> of <server> with this same input 3 times in the past hour, which is a loop, not work: take another way. The same call is allowed again at <time> UTC."
  - `ThisTool`: "rate_limited: you have made <calls> calls of <tool> of <server> in the past hour, the most one agent may make of one tool (a third of the team's <per_hour> paid calls an hour); the next is allowed at <time> UTC."
  - `AllTools`: "rate_limited: you have made <calls> paid or asking calls in the past hour, the most one agent may make (the team's limit); the next is allowed at <time> UTC."
- The per-session count of those refusals lives in memory, on the daemon's `Session` (`daemon.rs` near 161, beside `tool_calls`), as `rate_limited: u32`, 0 at `register_session` (planner's: the count is per session, and a session does not outlive the daemon, so nothing about it needs the log; the calls themselves are counted from the log). `Denial` gains `rate_limited: bool`; `decide_pre_tool_use` (`hooks.rs` near 129) raises the session's count on such a denial, under the sessions lock it already holds. At `RATE_LIMITED_PER_SESSION` (the third), the denial's reason gains " This is the third refusal for the rate in this session: it ends, and the task waits for the owner.", the session's `stop_reason` is set to "rate_limited: <agent> kept calling <tool>" (`<agent>` its name by `catervas_store::waiting::name_of`, `<tool>` the tool's name) and the stops are notified, as an agent paused by the user is stopped (so `read_to_end` aborts it); and, when the session is about a task, `task.flagged { needs: "<agent> kept calling <tool>", flagged_by: "catervas" }` is appended about it with the session's ids before the denial is recorded. A session about no task only ends. `orchestrator/session.rs` `ended_detail` (near 1634) gives the stop for an aborted session whose stop starts `rate_limited`, as it does for an approval, so the session's end says why.
- A flagged task holds while autopilot is on (decision 4, "the task stays where it is"): a rate-limited task is often `in_progress`, which no rule of plan 2 passes over. `rules.rs` `tick` reads, once, when `autopilot_on`, the task ids of `catervas_store::autopilot::flags`, and `waiting_on_nobody` (near 1227), through which rules 3 to 10 read the board, leaves them out as it leaves out a task waiting on the owner's answer; this replaces plan 2's own check in rule 4. With autopilot off a standing flag holds nothing: the task runs as now, and a paid call asks the owner, which is the stop. The owner frees it with any act that ends a flag (plan 1: the task's next move, a resolution, an acceptance) or by turning autopilot off.
- A marketing plan (`tools/marketing.rs` `propose_plan`, near 312): under autopilot a plan that spends is approved as one that spends nothing is, through `crate::marketing::decide_plan` (`marketing.rs` near 142), which gains `by: Option<MarketingPlanApprovedBodyBy>` (the owner and the free plan pass `None`), with the note `AUTOPILOT_PLAN_NOTE`: "Catervas approved it under autopilot; the marketing budget stays its hard stop (ADR 0082)." and the answer the free plan gives ("… is approved … Start on it now …", "spends nothing" replaced by "autopilot is on"). A raised budget is a plan that `replaces` the approved one, so it is the same path. The hard stop (step 08g) is unchanged.
- A data pipeline (`tools/pipeline.rs` `settle`, near 274): unchanged, a paid, unknown-cost or data-out request is still `data_pipeline.escalated` with Catervas's sentence. While the team decides, plan 2's autopilot rule gives the oldest escalated, undecided request a `decide` session of the Product Manager about the task its `data_pipeline.requested` names, offered `DECIDE_PIPELINE_TOOLS` (plan 2's reads, `catervas_post_message`, `catervas_write_memory`, `catervas_decide_data_pipeline`); after `DECIDE_SESSIONS_PER_WAIT` sessions with no decision Catervas flags that task, needs "a decision on the data source <name>". The decision is the owner's code moved: `human.rs` `pipeline_decide` (near 1286) keeps its note check and the `PIPELINES` lock and calls `tools/pipeline.rs` `decide_pipeline`, which holds the rest; the tool calls it with `by: product_manager` and the session's ids (the envelope names the Product Manager and its session, as an older log's Product Manager decision did). `waiting.rs` `pipelines_waiting` lists none while the team decides.
- A seller message (decision 6): sent by the tick, with no model, after the pause check in `Orchestrator`'s tick (`orchestrator.rs` near 588, only when the team is not paused and the scope is about no task): each message in `MessageState::Waiting`, drafted after the newest `autopilot.changed { on: true }`, whose `purpose` is not `purchase_order` (an order's email waits with its order), and with no failed try (a failed one waits for the owner: retrying on every tick would loop), oldest first, while fewer than `SELLER_MESSAGES_PER_DAY` messages were sent with `sent_by: autopilot` this UTC day. The subject is the drafted one and the body `procurement::draft_text`; `procurement::send_message` (`procurement/mail.rs` near 433) gains `sent_by: Option<SellerMessageSentBodySentBy>` (the owner passes `None`), carried to `record_sent`. The mailer is made as `human.rs` `mailer_of` makes it, from `deps.daemon`. A message beyond the day's 50 waits, on Today with Send as now, and goes the next day. Sending runs while the day's money is spent too: it costs no model.
- A post outside the approved plan already goes out with `approved_by: catervas` (ADR 0061, `tools/posts.rs` near 504): decision 6 changes nothing there, and plan 5 says so in 6.5.
- Merging (decision 7): `integrate.rs` (`orchestrator/integrate.rs` near 660) asks `merges_on_its_own(&tools.log, team)` in place of `merging_allowed`; so does Today's line (`store/src/waiting.rs` near 475, `tools/folders.rs` `integration_rows` near 395). `merging_allowed` and `team.get`'s `merging_allowed` stay ADR 0073's own, so the Finish switch shows the owner's setting. What a check does is ADR 0073's, as decision 11 confirms: a failed check is never merged over and escalates `integration`, a pending one is waited for, and a repository with no checks at all merges once Catervas's own Definition of Done has passed (the task is `accepted`, 5.4). `reopen_integration`: an integration escalation followed by the task's `team.decided { choice: reopen_integration }` is no longer open, in `ceremonies.rs` `open_escalations` (near 39) and in `integrate.rs` `escalated_since_accepted` (near 1450), so the next tick reads the pull request again and merges or escalates it as ADR 0073 says.
- Idle agents (decision 8): "no session running" is `!deps.work.is_busy(agent)` (`orchestrator/work.rs` near 59). Rule 8 (`rules.rs` `ready` near 2069, `ready_given` near 2143) asks `may_take_work` in place of `has_room`: `has_room`, or autopilot on and the agent not busy. The governor's gate agrees: `Transitions` gains `set_busy`, set once in `Orchestrator::new` (`orchestrator.rs` near 545) from `Arc::clone(&deps.work)`, and `transitions.rs` `context` sets `AssignmentInput.past_limit_allowed` to `autopilot_on && !busy(assignee)` (false until set, as in a test with none). The tick's `within` (`rules.rs` near 176) becomes `within_limit_or_idle`: `within_limit` plus, under autopilot, every counted task of an active agent that is not busy, so rule 7 starts the task it took; once its session runs the agent is busy again and its other tasks are handed over or wait, as today. The epic rule (`requests.rs` `ready_epic`), the hand-over (`resume_elsewhere`) and the Architect building a Developer's task (`ready`'s `with_room(Role::Architect)`, ADR 0077) keep `has_room`: none is an idle agent taking its own role's work ("Only its own role"). Nothing files work for an idle agent (ADR 0082, Alternatives).

## File map

```
daemon/hooks.rs                     modifies: autopilot calls, tool.approved, the rate, the third refusal; tests (T13)
daemon.rs                           modifies: Session.rate_limited (T13)
orchestrator/session.rs             modifies: ended_detail gives a rate_limited stop (T13)
orchestrator/rules.rs               modifies: waiting_on_nobody leaves out flagged tasks under autopilot; tests (T13)
marketing.rs                        modifies: decide_plan(by) (T14)
tools/marketing.rs                  modifies: approved under autopilot; tests (T14)
orchestrator/human.rs               modifies: decide_plan's callers (T14); pipeline_decide calls decide_pipeline (T15); send_message's caller (T16)
tools/pipeline.rs                   modifies: decide_pipeline (T15)
tools/decide.rs                     modifies: decide_data_pipeline (T15)
tools.rs                            modifies: catervas_decide_data_pipeline (T15)
orchestrator/autopilot.rs           modifies: pipelines in the rule (T15), send_seller_messages (T16); tests
crates/store/src/waiting.rs         modifies: no pipeline rows while the team decides (T15); merges_on_its_own line (T17)
procurement/mail.rs                 modifies: send_message(sent_by), record_sent (T16)
orchestrator.rs                     modifies: tick sends (T16); Orchestrator::new sets busy (T18)
orchestrator/integrate.rs           modifies: merges_on_its_own, reopen clears (T17); tests
ceremonies.rs                       modifies: open_escalations reads reopen (T17)
tools/folders.rs                    modifies: merges_on_its_own line (T17)
transitions.rs                      modifies: set_busy, past_limit_allowed, within_limit_or_idle (T18)
orchestrator/rules.rs               modifies: may_take_work, within (T18); tests
```

## Interfaces

Consumes: plan 1's `autopilot_on`, `team_decides`, `merges_on_its_own`, `flags`, `ConnectorPass.autopilot_approved`, `rate_limit`, `RecentCall`, `RateLimited`, `RATE_LIMITED_PER_SESSION`, `Team::paid_calls_per_hour`, `AssignmentInput.past_limit_allowed`, `SELLER_MESSAGES_PER_DAY`, `DECIDE_SESSIONS_PER_WAIT`; plan 2's autopilot rule, `tools/decide.rs`, `DECIDE_QUESTION_TOOLS`; on main `decide_plan`, `data_pipelines`, `file_pipeline_request`, `send_message`, `draft_text`, `seller_mail`, `Mailer`, `through_the_forge`, `within_limit`, `has_room`, `Work::is_busy`.

Produces:

```rust
// daemon.rs
pub(crate) struct Session { /* existing */ pub(crate) rate_limited: u32 }
// daemon/hooks.rs
fn recent_paid_calls(deps: &ToolDeps, agent: &str, per_hour: u32) -> Result<Vec<RecentCall>, Denial>; // oldest first
// orchestrator/rules.rs
fn waiting_on_nobody<'a>(board: &'a [TaskProjection], status: TaskStatus, held: &'a BTreeSet<TaskId>)
    -> impl Iterator<Item = &'a TaskProjection>;
// marketing.rs
pub(crate) fn decide_plan(tools: &ToolDeps, plan: &str, approve: bool, note: Option<String>,
    by: Option<MarketingPlanApprovedBodyBy>) -> Result<CommandReport, CommandError>;
// tools/pipeline.rs
pub(crate) struct PipelineDecided { pub seq: u64, pub said: String }
pub(crate) enum PipelineDecideError { Refused { code: &'static str, detail: String }, Failed { detail: String } }
pub(crate) fn decide_pipeline(deps: &ToolDeps, pipeline: u64, approve: bool, reason: &str,
    by: DataPipelineDecidedBy, ids: &EventIds) -> Result<PipelineDecided, PipelineDecideError>;
// tools/decide.rs
pub(crate) struct DecidePipelineInput { pipeline: u64, approve: bool, reason: String }
pub(super) fn decide_data_pipeline(call: &Call<'_>, input: &DecidePipelineInput) -> Result<Value, ToolError>;
// procurement/mail.rs
pub(crate) async fn send_message(deps: &ToolDeps, mailer: &Mailer<'_>, message: u64, subject: &str, body: &str,
    sent_by: Option<SellerMessageSentBodySentBy>) -> Result<(u64, String), MailboxRefusal>;
// orchestrator/autopilot.rs
pub(super) const DECIDE_PIPELINE_TOOLS: &[&str];
pub(super) async fn send_seller_messages(deps: &OrchestratorDeps) -> Result<Vec<u64>, OrchestratorError>;
// transitions.rs
impl Transitions { pub fn set_busy(&self, busy: Arc<dyn Fn(&str) -> bool + Send + Sync>); }
pub(crate) fn within_limit_or_idle(team: &Team, board: &[TaskProjection], autopilot: bool,
    busy: &dyn Fn(&str) -> bool) -> BTreeSet<TaskId>;
// orchestrator/rules.rs
pub(super) fn may_take_work(deps: &OrchestratorDeps, team: &Team, board: &[TaskProjection], agent: &Agent,
    under: Option<&TaskId>, autopilot: bool) -> bool;
```

## Tasks

### Task 13: paid and asking calls run under autopilot, within their rate

Files: `daemon/hooks.rs`, `daemon.rs`, `orchestrator/session.rs`, `orchestrator/rules.rs`
Tests (`hooks.rs` `mod tests`, on the harness its `paid` tests use; `rules.rs` `mod tests`):
- `a_paid_call_runs_under_autopilot_and_is_recorded_as_approved` — under autopilot, a `paid` and an `ask` tool's call with no grant are allowed; each appends one `tool.approved { by: "autopilot" }` with the input and its sha256 before its `tool.called`, and no `tool_approval.requested`; from a session about no task the call is allowed too; with autopilot off the same call is denied and appends `tool_approval.requested`.
- `hooks.rs`: `a_paid_call_over_its_rate_is_refused_and_the_third_refusal_ends_the_session_and_flags_its_task` — under autopilot, with the harness's team (no `paid_calls_per_hour`, so 30), `dev-a`'s three calls of `search` with one input on `CTV-1` are allowed and each appends `tool.approved`; the fourth is denied with a reason starting `rate_limited: ` that says "same input" and the `HH:MM` an hour after the first, and appends no `tool.approved`; the sixth call, the session's third such denial, also leaves the session's `stop_reason` "rate_limited: <dev-a's name> kept calling search" and one `task.flagged { flagged_by: "catervas", needs: "<dev-a's name> kept calling search" }` about `CTV-1` in the log, and a `Read` is then denied `session_stopped`; in a new session of `dev-a` (the count read from the log, as after a restart) a call of `search` with another input passes, and with the team's `paid_calls_per_hour: 3` (one per tool) the next is denied naming "a third of the team's 3"; a `tool.approved` of 61 minutes ago is not counted; with autopilot off no call is rate limited.
- `rules.rs`: `a_task_flagged_under_autopilot_gets_no_session_until_the_owner_acts` — under autopilot an `in_progress` task with a standing `task.flagged { flagged_by: "catervas" }` gets no implement session from a tick, and no other rule starts one about it; after `autopilot.set { on: false }` the tick starts its implement session.

- [ ] `feat(runtime): paid and asking calls run under autopilot, within their rate`

### Task 14: marketing plans are approved under autopilot

Files: `marketing.rs`, `tools/marketing.rs`, `orchestrator/human.rs` (callers pass `None`)
Tests (`tools/marketing.rs` `mod tests`):
- `approves_a_plan_that_spends_under_autopilot` — under autopilot, a plan with a total of 200 and a Google Ads budget is followed by `marketing_plan.approved { by: "autopilot", note: AUTOPILOT_PLAN_NOTE }` naming no agent and no session, and the answer says to start now; with autopilot off nothing is approved (`approves_a_plan_that_spends_nothing` keeps its `by` absent).

- [ ] `feat(runtime): marketing plans are approved under autopilot`

### Task 15: the Product Manager decides data pipelines under autopilot

Files: `tools/pipeline.rs`, `tools/decide.rs`, `tools.rs`, `orchestrator/human.rs`, `orchestrator/autopilot.rs`, `crates/store/src/waiting.rs`
Message of the session: the source's name, what it gives, its page, why, its cost, account and data-out answers (each of the agent's in an `untrusted_block`), Catervas's reason it waits, then "Approve or decline it with `catervas_decide_data_pipeline`, weighing what it costs against what it would change; the daily limit still stops all spending."
Tests:
- `orchestrator/autopilot.rs`: `the_product_manager_decides_a_paid_data_pipeline` — under autopilot an escalated paid request gets one `decide` session of the Product Manager offered exactly `DECIDE_PIPELINE_TOOLS`; its approval appends `data_pipeline.approved { by: "product_manager" }` naming the Product Manager and the session, files the request; `waiting.list` holds no pipeline row; the owner's `pipeline_decide` of it is then refused `pipeline_decided`; with autopilot off no session runs and the row is listed.

- [ ] `feat(runtime): the product manager decides data pipelines under autopilot`

### Task 16: seller messages go out under autopilot, 50 a day

Files: `procurement/mail.rs`, `orchestrator/human.rs`, `orchestrator/autopilot.rs`, `orchestrator.rs`
Tests (`orchestrator/autopilot.rs`, with the fake mail server the seller-mail tests use):
- `sends_drafted_seller_messages_under_autopilot_fifty_a_day` — under autopilot a tick sends a `quote_request` drafted after the switch and records `seller_message.sent { sent_by: "autopilot" }`; it does not send a `purchase_order` message, a draft from before the switch, a message whose try failed, or anything while the team is paused; with 50 sent by autopilot today the 51st waits, and it goes on the next UTC day.

- [ ] `feat(runtime): seller messages go out under autopilot, 50 a day`

### Task 17: the team merges when checks pass under autopilot

Files: `orchestrator/integrate.rs`, `ceremonies.rs`, `crates/store/src/waiting.rs`, `tools/folders.rs`
Tests (`orchestrator/integrate.rs` `mod tests`, with `FakeGh` as ADR 0073's tests use it):
- `merges_under_autopilot_and_tries_again_after_a_reopen` — under autopilot, with no `merge_when_checks_pass` and no grant, an accepted task's pull request whose checks passed is merged (`pull_request.merged`, `task.integrated { integrated_by: governor }`); another's failed check escalates `integration` and the next tick leaves it; after `team.decided { choice: reopen_integration }` and checks that now pass, the next tick merges it; with autopilot off the first pull request waits (`AwaitingForge`).

- [ ] `feat(runtime): the team merges when checks pass under autopilot`

### Task 18: an idle agent takes ready work of its role under autopilot

Files: `transitions.rs`, `orchestrator.rs`, `orchestrator/rules.rs`
Tests (`rules.rs` `mod tests`):
- `an_idle_agent_takes_a_ready_task_of_its_role_past_its_limit_under_autopilot` — with `wip_limit_per_agent: 1`, Developer `dev` holding `CTV-1` `in_progress` with no session running and `CTV-2` a `ready` Developer task: a `Planning` tick under autopilot starts the Product Manager's `plan` session for `CTV-2` naming `dev`, and the assignment of `CTV-2` to `dev` passes the governor; with autopilot off the tick starts no session and the assignment is refused for the limit; with `dev` busy (a claim held in `deps.work`) the tick under autopilot starts none either; a `ready` task of another role is never offered to `dev`.

- [ ] `feat(runtime): an idle agent takes ready work of its role under autopilot`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```
