# Standalone (parallel agents), plan 2b: a chat is a quick fork beside the agent's work

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.3, 5.5, 5.9 (one-to-one chats), 8.2; recorded by plan 5 (revision 0.118)
Depends on: plan 2 of this folder (committed on this branch)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder (2026-10-16): "The agent does not stop his current task, but it forks itself to either
answer the user question or create a new task to the backlog. The process of agents answering
questions must be quick." A chat session is a fork: it starts at once beside the agent's running
work session, which it does not touch, and it is bounded so that it answers in seconds to a few
minutes. One chat fork per agent at a time; messages written meanwhile wait for it. Its first
message says what the agent is doing now, so it can answer about its work. Out of scope: a tool
that files a task from the chat (none exists: ADR 0063 files a given task from the owner's
`as_task`, and the chat's reply can already propose a request the owner sends), the web app.

## Decisions

- **Two kinds of claim.** `Work` (plan 2) gains `chats: std::sync::Mutex<BTreeSet<String>>`, the
  agents with a chat fork running. `Work::claim(agent_id, task_id, purpose: SessionPurpose)` takes
  the purpose: `SessionPurpose::Chat` claims in `chats` and refuses an agent already there with
  `chat_busy: <agent> is answering its chat`; every other purpose claims in `busy` as plan 2 says.
  The `Claim` remembers which and is given back to that set. `is_busy` and `busy_tasks` read `busy`
  alone, so a chat fork holds no task and does not stop a work session from starting, and a work
  session does not stop a chat fork. New: `pub(crate) fn is_chatting(&self, agent_id: &str) ->
  bool` and `pub(crate) fn work_task_of(&self, agent_id: &str) -> Option<TaskId>` (the task of the
  agent's running work session, `None` when it runs none or one about no task). Chose this over
  exempting chats from the claim altogether: two forks of one agent would race on the one reply.
- **The chat rule's guard.** In `rules::chat`, `unavailable(deps, agent, ..)` (plan 2) is replaced
  by `asleep_or_chatting(deps, agent, &mut waiting.slept)`: `deps.work.is_chatting(..)`, then the
  sleep check, as `unavailable` minus its work check. `open_the_interview` uses it too. A message
  written while the fork runs stays pending (`crate::chat::pending_chat`), and the next tick after
  the fork ends answers the newest one, as a chat always has. The chat rule stays first in a tick,
  so a fork starts before any other work; it hands the turn back as any session does (plan 2).
- **What the fork is.** Unchanged from today and kept: no contract, `cwd` the project's root, no
  executor, no sandbox, no worktree, the read tier alone, `CHAT_TOOLS`, no connectors (they are
  given only to task sessions, `gives_connectors`), no browser (`offered_connector`), no sign-in
  refresh, the agent's own model at `Effort::Low` (`session_spec_without`). Plus whatever chat
  tool ADR 0063's plans add to `CHAT_TOOLS`; nothing here filters it out.
- **Made quicker.**
  - `catervas_core::budget::CHAT_SESSION_LIMITS: SessionLimits` = 100,000 input tokens, 4,000
    output tokens, 3 minutes, 12 tool calls (so `--max-turns 13`). `catervas_core::budget::
    tighter(limits, cap) -> SessionLimits` takes each field's smaller value. In
    `session_spec_without`, a `Chat` session that is not the interview (`interviewing` false) gets
    `tighter(<the budget's session_limits>, CHAT_SESSION_LIMITS)`. Chose these numbers from what a
    chat does today: a few reads (`catervas_read_board`, `catervas_read_task`) and one
    `catervas_chat_reply`; three minutes is ten times a recorded chat's `duration_ms`
    (`chat_answers_with_a_request.jsonl`, about 6 s). The interview keeps the team's limits: it
    writes the product plan.
  - A `Chat` session skips `stale_twins` (a `git` call for a rules line about committing; a chat
    commits nothing): `stale_twins` is empty for it.
- **What the agent is doing now.** `messages::work_summary(deps, agent) -> Result<String,
  OrchestratorError>` picks the task: `deps.work.work_task_of(agent)` when its work session is
  about one, else the agent's row in `assigned`, `in_progress` or `rejected`
  (`counts_towards_wip`) with the lowest `counted_seq` (0 when `None`), then the lowest number.
  With a task: `"What you are doing now: <id> \"<title>\" (<status in words>). Your last
  note on it: <text>"`, the status in `catervas_store::activity::status_in_words`'s words, the note the
  latest `note.written` on that task by that agent, any kind, cut to its first 600 code points
  with `…` when cut, and `"You have written no note on it yet."` when there is none. Without one:
  `"What you are doing now: no task is under way."`. The chat rule puts it between its first line
  and the user's message: `"<Name>, the user wrote to you in your one-to-one chat. Answer the
  newest message.\n\n<summary>\n\n<text>"` (the existing test's `starts_with` and `ends_with`
  still hold). The interview's opening is unchanged.
- **Budgets.** A fork counts toward the day and no task, as a chat did (5.5); a spent day stops it
  with the others (plan 3), and no fork starts on a spent day.
- **`HeldAdapter`** gains `release_purpose(agent, purpose)`, which releases that agent's held
  session of that purpose; `release(agent)` releases the agent's oldest held session.
  `HeldStart.alongside` may then name its own agent (a fork beside its work).
- **Plan 2's guarantee test** asserts that no *work* start has its own agent alongside; a chat
  start may.

## File map

```
crates/core/src/budget.rs                          modifies: CHAT_SESSION_LIMITS, tighter (Task 1)
crates/runtime/src/orchestrator/work.rs            modifies: chats, claim's purpose, is_chatting, work_task_of (Task 1)
crates/runtime/src/orchestrator/session.rs         modifies: claim with the purpose; chat limits; no stale_twins for a chat (Task 1)
crates/runtime/src/orchestrator/messages.rs        modifies: work_summary (Task 1)
crates/runtime/src/orchestrator/rules.rs           modifies: asleep_or_chatting; chat's prompt carries the summary (Task 1)
crates/runtime/src/orchestrator/fixtures.rs        modifies: HeldAdapter::release_purpose; release takes the oldest (Task 1)
crates/runtime/src/orchestrator.rs                 modifies: plan 2's guarantee test reads work starts; tests: the fork (Task 1)
```

## Interfaces

Consumes: plan 2's `Work`, `Claim`, `next`, `Progress`, `HeldAdapter`, `next_progress`, `until`;
plan 1's `counts_towards_wip`; `crate::chat::{post_chat, NewChatMessage}`,
`catervas_store::activity::status_in_words`, transcripts `reads_a_file`,
`chat_answers_with_a_request`.

Produces:

```rust
// catervas_core::budget
/// A one-to-one chat's own bounds, so that it answers quickly (ADR 0064).
pub const CHAT_SESSION_LIMITS: SessionLimits;
/// Each of `limits`' fields, no larger than `cap`'s.
#[must_use]
pub fn tighter(limits: SessionLimits, cap: SessionLimits) -> SessionLimits;

// orchestrator::work
impl Work {
    /// # Errors
    /// `Refused`: `agent_busy: …` for a work session, `chat_busy: <agent> is answering its chat` for a chat.
    pub(crate) fn claim(&self, agent_id: &str, task_id: Option<&TaskId>, purpose: SessionPurpose) -> Result<Claim<'_>, OrchestratorError>;
    pub(crate) fn is_chatting(&self, agent_id: &str) -> bool;
    pub(crate) fn work_task_of(&self, agent_id: &str) -> Option<TaskId>;
}

// orchestrator::messages
pub(super) fn work_summary(deps: &OrchestratorDeps, agent: &Agent) -> Result<String, OrchestratorError>;

// orchestrator::rules
fn asleep_or_chatting(deps: &OrchestratorDeps, agent: &Agent, slept: &mut Option<(DateTime<Utc>, String)>) -> Result<bool, OrchestratorError>;

// orchestrator::fixtures
impl HeldAdapter { pub(crate) fn release_purpose(&self, agent: &str, purpose: SessionPurpose); }
```

## Tasks

### Task 1: a chat forks beside the agent's work, quickly

Files as the map.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `answers_a_chat_while_its_agent_works` (`orchestrator.rs` `mod tests`, ignored "needs the git
  program"; watched to fail at its second `until`, which finds no `Started` of a chat: the chat
  rule passes `dev-a` over while its work session runs) — the `Harness` team;
  `in_progress("CTV-1", "dev-a", "dev-b")`; `let inner = harness.recorded(vec![
  chat_answers_with_a_request()])`; `HeldAdapter::new(inner.clone())`. `until` `Started` of
  `dev-a`, CTV-1, `implement`; `post_chat` a message from `human` in `dev-a`'s chat ("What are you
  on?"), then a second ("And after that?"); `until` `Started { agent_id: "dev-a", task_id: None,
  purpose: "chat" }`: the last of `starts()` has `alongside` `["dev-a"]`. `release_purpose("dev-a",
  SessionPurpose::Chat)`; `until` `Ticked(Chat { agent_id: "dev-a", .. })`. Then: a
  `chat_message.posted` by `dev-a` is in the log; no `session.ended` of the implement session; the
  held implement session is still running (`running()` holds `dev-a`); `starts()` has 2 entries
  (the second message did not start a second fork). The chat's spec (`inner.started()[0]`) has
  effort `Low`, `limits` equal to `CHAT_SESSION_LIMITS` (the Harness team's limits are larger),
  and an `initial_prompt` holding `What you are doing now: CTV-1 "<harness.row("CTV-1").title>"
  (In progress).` and ending with `And after that?`.
- `takes_each_limit_at_its_smaller` (`core/src/budget.rs` `mod tests`) — `tighter` of the default
  limits and `CHAT_SESSION_LIMITS` is `CHAT_SESSION_LIMITS`; of a cap with a larger wall clock
  keeps the limits' wall clock.

- [x] `feat(runtime): a chat forks beside its agent's work, briefly`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
