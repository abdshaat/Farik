# Phase 4, step 04: Budget consequences

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 5.2, 5.5, 5.7, 8.2, 8.5; F1
Depends on: phase 3 (merged in #11); steps 01 to 03 of this phase (committed on this branch before this step starts)
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the two decisions the first found open, and the test clock it found, folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A budget running out has the consequence the founder chose (2026-09-24) rather than phase 3's minimum, where a session was only stopped and the task passed over. A session that ends at one of its own limits leaves a progress note naming the limit, and the next session resumes from it; the task is not blocked. A task whose dollars or sessions are spent is escalated to the human with the reason. And an agent whose model's provider refuses it for a usage or rate limit sleeps until the limit resets: its task keeps its status, the other agents keep working, it is still assigned work, and `farik run` waits for it rather than ending while its work is left. Out of scope: posting any of this in the channel (step 05) and the escalation digest (step 06).

## Decisions

- A provider's limit is read from the session's stream (`stream.rs`). The parser keeps the last `rate_limit_event`'s `rate_limit_info.status` and `resetsAt` (seconds since the epoch; an event with no `rate_limit_info` is ignored, as it is today), and the `result` line's `is_error` and `api_error_status`. A `result` whose `subtype` is `success` with `is_error: true` ends `Error`, where today it ends `Completed`. A result that ends `Error` is a provider's limit when the last rate-limit status does not start with `allowed` (so `allowed_warning` is not one), or `api_error_status` is 429, or its detail says `usage limit` or `rate limit` (ASCII case-insensitive). Chose reading all three over the status alone, because no capture of a refused session exists yet (Claude Code 2.1.280's captures show only `allowed`); each form has its own test on a synthetic transcript and the spec names the residual. `EndReason` gains `ProviderLimit`; `SessionEvent::Ended` gains `resets_at: Option<DateTime<Utc>>`, the raw last `resetsAt`, set only when the reason is `ProviderLimit` (the parser has no clock; the orchestrator judges it); `session.ended`'s wire `reason` gains `provider_limit`.
- `drive` returns the budget scopes the session's reported usage crossed with its end, so `SessionEnd` gains `crossed: Vec<BudgetScope>` and `resets_at: Option<DateTime<Utc>>`. A session gets one note, and only an `implement` session, the one purpose whose next session reads the task's last note (`resume()`): when it ended `Limit`, or `ProviderLimit`, or crossed `SessionTokens`, `SessionWallClock`, or `SessionToolCalls`, Farik writes one `note.written { kind: progress, written_by: farik }` naming each cause and the detail, and quoting the agent's own last progress note of that session when it wrote one (so the agent's words are kept, `resume()` showing only the last note): "This session stopped at <causes>: <detail>. Your last note in it: <text>. Resume from the last commit and this note." Other purposes are asked again as today, from their own first message. `BudgetConsequence::EndSessionAndBlockTask` is renamed `EndSessionWithNote`; `budget.exhausted`'s wire `consequence` gains `end_session_with_note`, which new events carry, and keeps `end_session_and_block_task`, which logs recorded before this step hold and which reads as the same consequence.
- A session ending `ProviderLimit` makes its agent sleep: Farik records `agent.slept { until, detail }` (the sleeping agent on its envelope, which is what `EventQuery { agent_id }` finds; not about one contract; no attribution field, since Farik observed it), `until` being `resets_at` when it is later than the clock's now, else one hour after now. Such a session counts toward its task's `max_sessions` like any started session; the resolution below is how the human gives a task room.
- An agent is asleep while the last `agent.slept` for it has an `until` later than now, read by `asleep_until(log, agent_id, now) -> Result<Option<DateTime<Utc>>, StoreError>` (`EventQuery { agent_id, kinds: [agent.slept] }`). Sleep is not a status: the team file is not written, the agent stays `active`, no `agent.updated`. Each rule checks it beside its `spent` check, before it makes a sandbox or starts a session for an agent (Farik's own criterion runs in `verify.rs` are not an agent's and are not gated), through `asleep(deps, agent, slept: &mut Option<DateTime<Utc>>) -> Result<bool, OrchestratorError>`, which on `true` keeps the earliest `until` in the tick's accumulator (as `day_spent` is kept) and the rule does nothing; `run_session`'s signature does not change. Work is still assigned to a sleeping agent within its WIP limit: rule 8's list of assignees is unchanged; only its own sessions wait.
- The tick says why it idles: `TickReport::Idle` gains `until: Option<DateTime<Utc>>`, the accumulator's earliest `until`, and its `why` is then "waiting for <agent>, asleep until <time> (its model's usage limit)". Waiting is `OrchestratorDeps::sleeper: Arc<dyn Sleeper>`, `trait Sleeper { fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>; }` in `farik-runtime`: `TokioSleeper` waits with `tokio::time::sleep` for `until` less the clock's now; a test sleeper, `MovingSleeper` (in the orchestrator's test fixtures), moves a `MovableClock` to `until` and returns. `MovableClock` (today private to `transitions.rs`'s tests) moves to `farik_protocol::clock` as a public test clock with `set(DateTime<Utc>)`, and the harness gives the one `Arc` to the tools, the transitions, and the sleeper, so a wait moves every clock the tick reads. The command line takes its sleeper through `CliIo::sleeper: Option<Arc<dyn Sleeper>>` (`None` meaning `TokioSleeper`), which `start.rs` (`start` and `command_orchestrator`) passes into `OrchestratorDeps`. `wait_until` does not lose a stop that lands before it listens: it enables its `notified()`, then checks `is_stopped()`, then selects, the pattern `drive` uses for a session's stop. The command line waits inside `ticks()`'s select on its interrupts, so the first Ctrl-C calls `stop()` and ends the wait, for `run`, `plan`, and `contract new` alike. When a tick is idle for both a spent day and a sleeping agent, `why` names the day, and `until` is still set. `Orchestrator::wait_until(until) -> Waited { Reached, Stopped }` races the sleeper with a `tokio::sync::Notify` that `Orchestrator::stop()` now also notifies, so `farik stop` and the first Ctrl-C end a wait at once. `run_until_idle` and the command line's driving loop (`farik run`, `farik plan`, `farik contract new`) wait on an idle tick with an `until` and then tick again; only the command line prints the waiting line. Chose waiting over ending, because a run that ended would leave the sleeping agent's work for nobody (the founder: it "starts working from where it left off").
- Step 03's sprint planning session is asked again when the one before ended `Limit` or `ProviderLimit`: a sprint has had its planning session once a task-less `plan` session whose `session.ended` reason is `completed`, `aborted`, or `error` follows its `sprint.started`, or once three task-less `plan` sessions have, whatever their ends, so that a planning session that keeps hitting its wall clock is not asked for ever.
- A task whose `TaskUsd` or `TaskSessions` is exhausted is escalated by the governor through the `any -> escalated` row (`GovernorEscalation`, reason `budget` or `sessions`), by a rule the tick runs after rule 2 and before rule 3, under `TickRules::All` only (`farik plan` and `farik contract new` keep passing such a task over, since they do not govern work in progress); the numbered rules keep their numbers, this one being the budget rule. It takes any row that is neither terminal, nor `escalated`, nor waiting on the human, nor refused that move since entering its status; it runs before rule 5, so a `verifying` task out of sessions is escalated before Farik runs its criteria. Chose a rule of its own over escalating inside `spent`, a guard many rules call. When the human resolves such an escalation to a working status without raising the budget, which is a content change that sends the task back to `refining` (5.11), the task is escalated again on the next tick: that is how Farik says the budget is still spent (spec 5.7 says so).
- Existing tests: those whose point is running out of a budget change to expect the escalation (`passes_over_a_task_out_of_sessions`, `passes_over_a_task_out_of_dollars`, `finishes_the_last_session_a_task_is_allowed`'s second tick, `runs_the_criteria_of_a_task_out_of_sessions`); those whose point is something else get room in their fixture so it still holds (`escalates_a_task_whose_criterion_farik_could_not_run`, `runs_until_a_tick_is_idle`, `plans_without_running_criteria_or_merging`); every `TickReport::Idle { why }` comparison gains `until: None`. `aborts_a_session_whose_usage_crosses_a_budget` gains its note.
- A spent daily budget is unchanged (no session starts for anyone; no agent is paused); its channel line comes with step 05.

## File map

```
crates/core/src/budget.rs                        modifies: EndSessionWithNote; tests
crates/runtime/src/stream.rs                     modifies: rate_limit_event kept, is_error and api_error_status read, ProviderLimit; tests
crates/runtime/src/session.rs                    modifies: EndReason::ProviderLimit, Ended.resets_at
crates/runtime/src/sessions.rs                   modifies: provider_limit on the wire
crates/runtime/src/claude.rs, crates/runtime/src/recorded.rs, crates/runtime/src/recorded/fixtures.rs, crates/runtime/src/orchestrator/recover.rs, crates/runtime/tests/claude_process.rs, crates/cli/tests/live_claude.rs   modifies: Ended's new field, ProviderLimit arms
crates/runtime/src/cost.rs                       modifies: end_session_with_note on the wire
docs/schemas/event.schema.json                   modifies: session.ended provider_limit, budget.exhausted end_session_with_note, agent.slept
crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies: agent.slept
crates/store/src/projections.rs                  modifies: agent.slept applied as nothing (no projection)
crates/runtime/src/sleep.rs, crates/runtime/src/lib.rs   creates / modifies: asleep_until, Sleeper, TokioSleeper
crates/runtime/src/orchestrator/session.rs       modifies: drive returns crossed scopes; the note and the sleep at a session's end
crates/runtime/src/orchestrator/{rules,requests,verify,human,integrate}.rs   modifies: asleep checks before sandboxes and sessions; the budget rule; acted's words for ProviderLimit; Idle comparisons
crates/runtime/src/orchestrator.rs               modifies: TickReport::Idle.until, sleeper in OrchestratorDeps, wait_until, stop notifies, run_until_idle waits
crates/cli/src/run.rs, crates/cli/src/contract_new.rs   modifies: waiting on an idle tick with until
crates/runtime/src/recorded/transcripts/         creates: provider_limit_rejected.jsonl, provider_limit_429.jsonl, provider_limit_text.jsonl, success_with_is_error.jsonl
docs/SPEC.md, docs/plans/project-plan.md         modifies
```

## Interfaces

Consumes: `check_budgets`, `governor_escalation_reason` (core); `run_session`, `SessionAsk`, the tick's rules, `refused_since_entering` (runtime); `EventLog::read` with `EventQuery { agent_id, kinds }` (store).

Produces:

```rust
pub enum BudgetConsequence { EndSessionWithNote, EscalateTask, StopNewAssignments, PauseTeam }
// farik-protocol: budget.exhausted consequence end_session_with_note (new events) and end_session_and_block_task (read from older logs)
pub enum EndReason { Completed, Aborted, Limit, Error, ProviderLimit }
pub enum SessionEvent { /* as before */ Ended { reason: EndReason, detail: String, resets_at: Option<DateTime<Utc>> } }
// farik-protocol: EventBody::AgentSlept(AgentSleptBody { until: DateTime<Utc>, detail: String }); session.ended reason provider_limit
// farik-runtime::sleep
pub fn asleep_until(log: &EventLog, agent_id: &str, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, StoreError>;
// farik-runtime::orchestrator
pub enum TickReport { Idle { why: String, until: Option<DateTime<Utc>> }, Acted { .. }, Sprint { .. } }
pub(super) struct SessionEnd { /* as before */ pub(super) crossed: Vec<BudgetScope>, pub(super) resets_at: Option<DateTime<Utc>> }
// farik-protocol::clock: pub struct MovableClock with fn new(DateTime<Utc>) and fn set(&self, DateTime<Utc>)
// farik (cli): CliIo gains pub sleeper: Option<Arc<dyn Sleeper>>
pub(super) fn asleep(deps: &OrchestratorDeps, agent: &Agent, slept: &mut Option<DateTime<Utc>>) -> Result<bool, OrchestratorError>;
pub trait Sleeper: Send + Sync { fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>; }
pub struct TokioSleeper { /* the clock */ }
pub enum Waited { Reached, Stopped }
impl Orchestrator { pub async fn wait_until(&self, until: DateTime<Utc>) -> Waited; }
```

## Tasks

### Task 1: reading the provider's limit

Files: `stream.rs`, `session.rs`, `sessions.rs`, `claude.rs`, `recorded.rs`, the event schema's `session.ended`, the four transcripts
- `ends_a_successful_result_with_an_error_as_an_error` — a `result` with `subtype: success, is_error: true`: `Ended { reason: Error, .. }`.
- `reads_a_rejected_rate_limit_as_the_providers_limit` — a `rate_limit_event` with `status: rejected, resetsAt: <in an hour>`, then an error result: `ProviderLimit` with `resets_at` that time.
- `reads_a_429_as_the_providers_limit` — an error result with `api_error_status: 429` and no rate-limit event: `ProviderLimit`, `resets_at: None`.
- `reads_a_usage_limit_in_the_words` — an error result whose `result` text is "Claude AI usage limit reached": `ProviderLimit`.
- `keeps_an_ordinary_error_an_error` (guard) — an error result with an `allowed` rate-limit event, no 429, other words: `Error`.
- `keeps_a_warning_an_ordinary_error` (guard) — an error result after an `allowed_warning` status: `Error`.
- `passes_no_reset_time_without_a_limit` — an `allowed` event with a `resetsAt`, then a success: `resets_at: None`.
- `records_the_providers_limit_as_the_sessions_end` — `session.ended` for such a session has reason `provider_limit`.

- [x] `feat(runtime): tell a model provider's usage limit from any other failed session`

### Task 2: the limit note

Files: `budget.rs`, `orchestrator/session.rs`, `rules.rs` (tests)
- `leaves_a_note_when_a_session_hits_its_wall_clock` — an implement session ending `Limit` with the wall-clock detail: one `note.written { kind: progress, written_by: farik }` on the task naming "wall clock", the task still `in_progress`, and the next implement message holding that note.
- `leaves_a_note_when_a_session_crosses_its_tokens` — usage over the session's input tokens, the session `completed`: the note names the input tokens.
- `keeps_the_agents_own_note_in_farks_note` — the agent wrote a progress note in the session, which then hit its wall clock: Farik's note quotes it.
- `leaves_no_note_for_a_verify_session` — a verify session ending `Limit`: no note.
- `records_the_new_consequence_and_reads_the_old` (protocol) — a new `budget.exhausted` carries `end_session_with_note`; an event with `end_session_and_block_task` still reads.
- `aborts_a_session_whose_usage_crosses_a_budget` (changed) — now also asserts the note.
- `names_the_session_consequence_after_its_note` (core) — `check_budgets` maps the three session scopes to `EndSessionWithNote`.

- [ ] `feat(runtime): leave a note where a session stopped at its own limit`

### Task 3: sleep

Files: the event schema, protocol, `projections.rs`, `sleep.rs`, `orchestrator/session.rs`, the callers
- `puts_an_agent_to_sleep_at_its_providers_limit` — an implement session ending `ProviderLimit` with `resets_at` in two hours: a progress note naming the provider's limit, `agent.slept { until: that time }` with the agent on its envelope, the task still `in_progress`, and the team file unchanged.
- `sleeps_an_hour_without_a_reset_time` — `resets_at: None`: `until` is the clock's now plus one hour.
- `ignores_a_reset_time_in_the_past` — `resets_at` before the clock's now: `until` is now plus one hour.
- `makes_no_sandbox_for_a_sleeping_agent` — dev-a asleep with FRK-1 `in_progress` and no sandbox yet: the tick creates none for it.
- `plans_a_sprint_again_after_its_planner_slept` — the sprint's planning session ended `ProviderLimit`; after the sleep, the next tick starts a new planning session.
- `starts_no_session_for_a_sleeping_agent` — dev-a asleep for an hour with FRK-1 `in_progress`, dev-b with FRK-2 `in_progress`: the tick starts dev-b's session; a second tick starts none for dev-a.
- `wakes_an_agent_when_its_sleep_ends` — the clock moved past `until`: dev-a's implement session starts, its first message holding the provider-limit note.
- `still_assigns_work_to_a_sleeping_agent` — dev-a asleep and the only Developer with room, FRK-3 `ready`: the Scrum Master's plan session offers dev-a as an assignee.
- `answers_asleep_until_from_the_last_sleep` (store-backed unit) — two `agent.slept` for one agent: the later's `until`; none for another agent.

- [ ] `feat(runtime): put an agent to sleep until its provider's limit resets`

### Task 4: waiting for a sleeping agent

Files: `orchestrator.rs`, `sleep.rs`, `orchestrator/rules.rs`, `orchestrator/fixtures.rs`, `crates/protocol/src/clock.rs`, `transitions.rs` (its tests use the moved clock), `cli/src/run.rs`, `cli/src/contract_new.rs`, `cli/src/lib.rs` (`CliIo::sleeper`), `cli/src/start.rs`
- `idles_until_the_first_agent_wakes` — the only work belongs to dev-a, asleep until T: the tick is `Idle { until: Some(T) }`.
- `idles_without_a_time_when_nobody_sleeps` (guard) — nothing to do: `Idle { until: None }`.
- `waits_for_a_sleeping_agent_then_goes_on` — `run_until_idle` with the test sleeper: it waits until T (the clock is then T), runs dev-a's session, then ends.
- `stops_a_wait` — `wait_until(T)` with a sleeper that never returns, then `stop()`: `Waited::Stopped` at once.
- `sees_a_stop_before_the_wait` — `stop()`, then `wait_until(T)` with a sleeper that never returns: `Waited::Stopped`.
- `plans_a_sprint_at_most_three_times` — three planning sessions ending `Limit`: no fourth.
- `prints_the_wait` (cli, recorded adapter and the test sleeper) — `farik run` with dev-a asleep: the output holds "waiting for dev-a, asleep until", then dev-a's session.

- [ ] `feat(runtime): wait for a sleeping agent instead of ending the run`

### Task 5: escalating a spent task

Files: `orchestrator/rules.rs`, tests
- `escalates_a_task_out_of_sessions` (changes `passes_over_a_task_out_of_sessions`) — FRK-1 `in_progress` with its `max_sessions` used: the tick moves it to `escalated` as the governor with reason `sessions`; no session starts.
- `escalates_a_task_out_of_dollars` — its `max_cost_usd` spent: reason `budget`.
- `escalates_a_refining_contract_out_of_sessions` — a `refining` task with its sessions used: escalated, reason `sessions`.
- `asks_the_escalation_once` (guard) — a `transition.refused` of that move seeded with the tests' `refused(...)` helper: the tick does not ask again while the task keeps its status.
- `escalates_again_a_task_resumed_without_room` — the human resolved the sessions escalation to `in_progress` without a budget change: the next tick escalates it again, reason `sessions`.
- `leaves_the_budget_to_farik_plan` (guard) — under `TickRules::Planning`, a `refining` task out of sessions is not escalated.
- `leaves_a_task_with_room_alone` (guard) — sessions left: no escalation.

- [ ] `feat(runtime): escalate a task whose dollars or sessions are spent`

### Task 6: the spec

Revision 0.13 in the header naming each change: 5.5's first row (a note on an implement session, not a block), `budget.exhausted`'s new consequence value, the "This release enforces" paragraph rewritten (the note, the escalation of a task's dollars and sessions, the unchanged day, the sprint as step 03 left it), and a paragraph on the provider's limit (how it is read and the residual that no refused capture exists, the sleep, the waiting run); 5.2's `any -> escalated` row now asked by Farik for a spent task; 8.2 (`session.ended` `provider_limit`, and that the parser reads `is_error` and the rate-limit events); 8.5 (`agent.slept`); F1 (a sleeping agent stays active and is not paused); 5.7 (a budget escalation resolved without more room is raised again); 8.2 (`farik run` waits for a sleeping agent and says so). The step's interface line in the project plan is written as landed.

- [ ] `docs(docs): record the budget consequences and the provider-limit sleep in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
