# Phase 6, step 13: Chats

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3, 4.3, 4.4, 5.1, 5.2, 5.5, 5.9, 8.2, 8.4, 8.5, F7, F8
Depends on: steps 01 to 12 of this phase (renumbered from step 12 by the project plan's revision 23); ADR 0026 and `docs/design/designer-chats-templates.md` (section B), both binding
Readiness confirmed by: fresh-session reviewer, 2026-09-30, ready with findings, folded in
Mockups approved by: the founder, 2026-09-30 (Chats, PhoneChats, OneOnOne and Costs on the canvas's Chats page; Sol included in the list, as every agent not retired has a chat)
As built (Task 2, 2026-09-30):
- Step 12's names hold: `SessionRegistration.connectors` and `ToolContext.connectors` (`Vec<SessionConnector>`), `evaluate_connector_call` (`farik_core::governor::permissions`), `connector_not_in_session` (`daemon/hooks.rs`). Since this plan was written, `offered_connector` gives Playwright only to explore, implement and a Designer's verify sessions with a task, and `run_session` adds `network` to the tiers only when a connector is given; a `chat` session gets neither, and Task 3 sets its tiers to `[Read]` as explore's are set. `designer_browser`, `NoConnector` and `has_playwright` do not touch chats.
- `pending_chat` moves to Task 3: it reads `session.started { purpose: chat }`, which Task 3 adds, and no Task 2 test drives it.
- `chat.messages` answers `{ messages: [{ seq, at, author, text, in_reply_to, request }] }`; Task 4 adds `waiting`, Task 6 `sent_as`.
- `farik chat` is in `crates/cli/src/lib.rs` (the commands live there) and `chat.rs`. The `farik channel` part of `keeps_chats_out_of_the_channel` is asserted in `chats_from_the_command_line`, since a runtime test cannot run the command line.
- Refusals: an unknown agent is `NotFound`, a blank or too long text `Invalid`, a retired agent `Refused` (`agent_retired`). `chat::ProposedRequest` is its own type (Task 3's tool input), mapped to the generated one.
- Envelope readers checked: `farik log` has no agent filter; activity, `pending_mentions` and `farik_post_message`'s allowance read by kind. The Team page's removal check counts any event naming the agent, so an agent the user chatted with is retired, not deleted, which keeps its chat under "Past teammates": kept.

As built (Task 3, 2026-09-30):
- The chat's history is the prompt's `From the human` section (`chat::chat_history`: the user's lines as "The user:", the agent's as "You:" inside `untrusted source="chat"`, a proposal's title and text with them), and its closing is `CLOSING_INSTRUCTIONS`' `Chat` entry. `farik_chat_reply` answers `{ seq, said: "Sent. Your turn is over." }`; a second reply is found by the session's own `chat_message.posted` after its `in_reply_to`.
- `session.started`'s `chat` is `sessions.rs`' own, from the spec's purpose and agent; `team.activity`'s line is `farik-store`'s `activity.rs`, where every line is worded.
- The daily budget and sleep are checked by the rule as the conversation's are; their tests are Task 4's `says_why_a_chat_waits`.

As built (Task 4, 2026-09-30):
- `tools::may_work(status, purpose)` is the one active check the hook's `judge` and `call_tool` share: `active`, or `paused` in a `chat` session. The chat rule reads every agent not retired. `rules::chat_alone` is the chat rule as `tick_within` runs it while the team is paused and `pause::key_refused` is false.
- `ChatWaiting` gains `NoAnswer`; `chat::chat_waiting(log, projections, team, agent_id, now)` answers it when the user wrote last, and `cost::day_spent` is the spent-day check it shares with the rules' `day_is_spent`. A session is running while its `chat` `session.started` has no `session.ended` of its session id.
- Pausing an agent still stops every session of its, a chat session among them (`update_agent_with`): the message is then `no_answer`, and the user asks again. Kept, as 4.4's "at once" reads.
- `wakes_serve_on_a_chat` is in `daemon.rs` (`POST /command`), with the team paused, so "within one tick" is the next tick answering.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Channel page becomes a chat list: **Team**, the channel of step 10 unchanged, then one private chat per agent, each with its avatar, name, role and last line. In a one-to-one the user asks one agent something and the agent answers by itself, from its memory and read access to the project, and changes nothing: the governor gives the session the read tier and one writing tool, its reply. When work is needed the reply carries a proposed request, and only the user's "Send as a request" files it. Chats are kept in the log, never reach the channel or another agent, are answered while the team is paused, and cost shows on the Costs page as "Conversations".

Out of scope: phase 8 step 04's memory history with revert and the decisions view; notifications; the agent filing anything itself (ADR 0026 replaces phase 8's `farik_propose_task`); group chats other than Team.

## Decisions

Design and ADR 0026 decide the what. These are the points they leave open, each decided here.

- **Gate.** Task 1's mockups are approved by the founder before Task 2 starts. The approval is recorded in this header ("Mockups approved by: the founder, <date>"), and any change the founder asks for at the gate is written into this plan in the same commit. The rail label is "Chats" (the founder, 2026-09-30).
- **Task order.** The event and its queries (Task 2) come before the session (Task 3), because the reply the session writes is that event: the no-forward-dependency rule orders them, not the brief.
- **Envelope.** A `chat_message.posted` envelope names the chat's agent on both the user's message and the reply (the channel names none on the human's), so `EventQuery { agent_id, kinds: [chat_message.posted], before_seq, newest_first }` reads one chat a page at a time with no filtering after the limit. The body's `chat` repeats it, as the design writes.
- **Pending.** A chat is pending when the seq of its newest message by the user is above the `in_reply_to` of every `session.started { purpose: chat }` of that agent, whoever wrote last. A message sent while a session runs is answered next, with the reply in its history. The session's `in_reply_to` is that message's seq, as a conversation's is (8.5), so a failed session is not retried in a loop: the user asks again.
- **The rule's place.** The chat rule runs first in `rules::tick`, before `sprint_rules`, and only in an unscoped `TickRules::All` tick, as `conversation` does: `farik run FRK-n` answers no chat.
- **Session shape.** `SessionAsk { agent, purpose: Chat, contract: None, cwd: project root, executor: None, read_only: true, only_tool: None, tools: Some(CHAT_TOOLS), in_reply_to: Some(seq), thread: None, initial_prompt }`, where `initial_prompt` is "<Name>, the user wrote to you in your one-to-one chat. Answer the newest message." followed by that message's text; the history and the closing instruction are in the system prompt. The agent's own model (`session_model`), effort `low`; the conversation's limits. The session's tiers (`ToolContext::tiers`, `SessionRegistration`) are `[Read]` whatever the agent's grants: the governor, not the prompt, bounds it. It is given no connector: step 12's `SessionRegistration.connectors` and `ToolContext.connectors` are empty for a `chat` session.
- **The reply tool.** `farik_chat_reply` is tier `Read`, as every Farik write tool is, so `session_spec`'s and `permit`'s tier filters keep it. It is offered only in `chat` sessions: `session_spec` drops it from every other session's list, as `CONVERSATION_TOOLS` narrows conversations, and `call_tool` refuses it outside a chat (`chat_reply_refused`).
- **`session.started` names the chat.** It gains `chat: <agent_id>`, as the design writes, on `purpose: chat` alone; it repeats the envelope's agent, and keeps the event readable without the envelope.
- **Ending.** `farik_chat_reply` records the reply and answers "Sent. Your turn is over."; a second call is refused `chat_reply_refused`, and the model's turn ends the session. A call refused for its limits does not use up the one reply. Rejected: aborting the session from inside a tool, which no tool does today and which the refusal makes unnecessary.
- **Paused agent.** The hook's and `call_tool`'s `agent_not_active` check lets a `chat` session's agent be `active` or `paused`; `retired` is refused. Every other purpose is unchanged, so pausing still stops work at once (4.4).
- **Paused team.** `tick_within` runs the chat rule alone while the team is paused, except when the pause is `credential_refused`: a key the provider refuses cannot answer either.
- **Why no answer.** `chat.messages` answers `waiting: null | { because, until? }`. No event is written for a chat that did not run; the page words the reason (the design's "the chat then says so"). When several apply, the first wins:
  - `retired`;
  - `key_refused`;
  - `day_spent`;
  - `asleep`, with `until`;
  - `answering`: pending, or a `chat` `session.started` of that agent with no `session.ended` yet ("Mira is thinking…");
  - `no_answer`: the user's newest message is at or below an ended `chat` session's `in_reply_to` and no reply followed ("Mira could not answer. Ask again."). It also covers a key refused while the human had paused the team, which records no `credential_refused` pause.
- **Live `waiting`.** The open chat re-queries `chat.messages` on its agent's `session.started` and `session.ended` with purpose `chat`, and on its reply.
- **`team.activity`.** `rpc.schema.json`'s activity `purpose` enum gains `chat`, and the line is "Answering your chat", with no chat text; a paused agent answering shows the same line.
- **Limits.** A message's and a reply's text: 1 to 4,000 code points, not blank, line breaks kept. A proposed request: `title` 1 to 120 code points, one line; `text` 20 to 4,000 (the `request.file` floor).
- **Sending a proposal.** The box under the reply is prefilled `<title>\n\n<text>` and editable; "Send as a request" calls `request.file { text, from_chat_message }`. `from_chat_message` must be the seq of an agent's `chat_message.posted` that carries `request`, and not one already sent; otherwise `REFUSED` with a sentence. The check and the write run under the lock `file_request` writes in, so two tabs cannot file one proposal twice. `task.created` gains `from_chat_message`, and `created_by` stays `human`: the user sent it.
- **Store change.** `file_request` gains a trailing `from_chat_message: Option<u64>` parameter; its callers pass `None`.
- **Costs.** `SessionPurpose::Chat` is recorded as `chat` (session and cost purpose enums). `CostScope::Purpose` sums by purpose; `costs.summary` gains `conversations_today_usd`, and the Costs page shows "Conversations today: $x.xx. Your chats with the team; they count toward the daily limit." under the table. Agents' rows already include it, as costs by agent.
- **Chat list.** `chats.list {}` answers `{ team_last, chats: [{ agent_id, retired, last }] }` in team order, `last` and `team_last` `{ seq, at, author, text } | null` (`team_last`: the newest `message.posted` that is not `system`).
- **Addresses and layout.** `/channel` is Team (so step 10's `#thread-…` anchors keep working) and `/channel/<agent_id>` a one-to-one. On a wide screen the list is a left column beside the open chat; on a phone it is one row of avatars above the chat, Team first. A retired agent's chat is under "Past teammates", read-only, with no box. An id that names no agent shows "No such teammate" beside the list.
- **Composer.** "Message <Name>", a textarea: Enter sends, Shift+Enter makes a new line (the founder, 2026-09-30), in both composers, and "Send" sends. Over 4,000 code points is refused before sending.
- **Envelope exception.** SPEC 8.5 gains that a `chat_message.posted` envelope names the chat's agent even on the user's message, as `agent.slept` is an exception. `farik log --agent` and every reader of the envelope's agent as the actor are checked in Task 2: those that must not count the user's message as the agent's filter on `author`.
- **Waits for step 12.** Step 12's plan (`step-12-designer-preview-and-review.md`) produces `SessionRegistration.connectors`, `ToolContext.connectors` and the hook's connector check (`evaluate_connector_call`, refusal `connector_not_in_session`). This plan names them, and `denies_a_chat_everything_else` runs with a Designer whose `mcp_servers` holds `playwright`. When step 12 lands, these names are re-checked against its commits before Task 2 starts.
- **Decided by the founder, 2026-09-30:** Enter sends and Shift+Enter makes a new line, in both composers; the rail label is "Chats"; "Conversations" means one-to-one chats only, and the channel's `conversation` purpose stays under "Meetings and talk".
- **Decided by the founder at the gate, 2026-09-30:** ADR 0026's "decided here" points, as this step builds them: a paused agent answers, a spent day stops chats (the message is kept), and a chat runs on the agent's own model at `low`.
- **Live.** A `chat_message.posted` from `useEvents` whose `chat` is the open one is appended without a query, as step 10 appends channel messages; a `task.created` with `from_chat_message` turns that reply's box into "Sent as FRK-n".
- **Command line.** `farik chat <agent> <text>` sends `chat_message_post`; `farik chat <agent>` prints the chat, one message per block, oldest first.
- **Transcript.** `chat_answers_with_a_request`: Mira reads the board and calls `farik_chat_reply` with a text and a request titled "Let customers pay with Apple Pay", the OneOnOne mockup's words.

## Security

- **Agent text is never HTML.** Replies and proposals render through `renderMessageText` (React text and elements), with `white-space: pre-wrap` for line breaks. No `dangerouslySetInnerHTML` anywhere in the step; the test `renders_agent_text_as_text` pins it for the reply, the proposal's title and the chat list's last line.
- **A chat never files a request.** The session is registered with `CHAT_TOOLS` alone, so `farik_create_task`, `farik_post_message`, `farik_write_memory` and any connector or web tool are denied by the hook (`tool_not_in_session`, `tool_not_allowed`); `farik_chat_reply` writes only a `chat_message.posted` and refuses outside a chat. The one path to work is `request.file`, a browser method only a signed-in human session reaches; `from_chat_message` only links.
- **The log stays unread.** Chat privacy rests on `.farik/local/**` staying protected: the log `.farik/local/farik.db` lives there, and a chat's `Read`, `Grep` and `Glob` run in the project root. `denies_a_chat_everything_else` names that file, and SPEC 5.9 says privacy depends on that default.
- **The terminal.** `farik chat <agent>` prints through `printable`, as `farik channel` does, so agent text cannot send escape sequences.
- **Prompt.** The agent's own earlier lines return wrapped `untrusted`, as agent-written text is (ADR 0011).
- **The author is fixed.** `chat_message_post` has no author field: Farik writes `human`.

## File map

```
docs/design/mockups/{Chats,PhoneChats}.dc.html, OneOnOne.dc.html, Costs.dc.html, canvas.json   creates / modifies (T1)
docs/schemas/event.schema.json            chat_message.posted; purpose chat; task.created from_chat_message (T2, T3, T6)
docs/schemas/command.schema.json          chat_message_post (T2)
docs/schemas/rpc.schema.json              chats.list, chat.messages, team.activity's chat (T2, T3, T4, T5, T6)
crates/runtime/src/chat.rs (+ tests), lib.rs                creates: the chat record, pending, the waiting reason (T2, T4)
crates/runtime/src/orchestrator/human.rs                    modifies: Command::ChatMessagePost (T2)
crates/runtime/src/daemon/board.rs                          modifies: chats.list, chat.messages, costs.summary (T2, T4, T5)
crates/cli/src/{main.rs,chat.rs}                            modifies / creates: `farik chat`, through printable.rs (T2)
crates/runtime/src/session.rs, sessions.rs, cost.rs, orchestrator/recover.rs, crates/store/src/metrics.rs   SessionPurpose::Chat (T3)
crates/runtime/src/tools.rs, tools/chat.rs, tools/refusal.rs                  farik_chat_reply, chat_reply_refused (T3)
crates/runtime/src/orchestrator/{rules.rs,session.rs}, orchestrator.rs, prompt.rs   the chat rule, TickReport::Chat, prompt (T3, T4)
crates/runtime/src/daemon/hooks.rs, daemon.rs, tools.rs     a chat session's paused agent (T4)
crates/runtime/src/daemon/gates.rs                          team.activity's line for a chat (T3)
crates/runtime/src/recorded/fixtures.rs, transcripts/chat_answers_with_a_request.jsonl   (T3)
crates/cli/src/run.rs                                       prints TickReport::Chat (T3)
crates/store/src/projections.rs                             CostScope::Purpose (T5)
crates/store/src/requests.rs, crates/runtime/src/daemon/gates.rs   from_chat_message (T6)
apps/web/src/pages/{Chats,OneToOne}.tsx (+ css, tests), Costs.tsx, app/App.tsx, shell/Shell.tsx, strings/en.ts   (T7)
apps/web/e2e/channel.spec.ts              modifies: the rail link's new name (T7)
crates/cli/src/bin/farik-e2e-serve.rs, apps/web/e2e/chats.spec.ts   (T8)
docs/SPEC.md, docs/plans/project-plan.md  (T9)
```

## Interfaces

Consumes: `useEvents`, `useQuery` (step 04); `renderMessageText` and `channel.messages` (step 10); `team.get`, `waiting.list`; `request.file` (step 07); `EventQuery { agent_id, before_seq, newest_first }` (step 10); `session_model`, `run_session`, `SessionAsk`, `day_is_spent`, `asleep`, `allowed_builtins` (phases 3 and 4); `pause::paused`, `pause::key_refused`; `SessionPurpose`, `SessionRegistration.connectors`, `ToolContext.connectors` and `evaluate_connector_call` from step 12 (`step-12-designer-preview-and-review.md`), which must land first (re-checked then).

Produces:

```rust
// crates/runtime/src/chat.rs
pub const CHAT_TEXT_MAX: usize = 4000;
pub struct ProposedRequest { pub title: String, pub text: String }
pub struct NewChatMessage { pub chat: String, pub author: String, pub text: String,
    pub in_reply_to: Option<u64>, pub request: Option<ProposedRequest>, pub session_id: Option<String> }
pub enum ChatError { Refused { reason: String }, Store(StoreError) }
pub fn post_chat(log: &EventLog, clock: &dyn Clock, ids: &EventIds, message: NewChatMessage) -> Result<u64, ChatError>;
pub fn chat_page(log: &EventLog, agent_id: &str, before_seq: Option<u64>, limit: usize) -> Result<Vec<FarikEvent>, StoreError>;
pub fn pending_chat(log: &EventLog, agent_id: &str) -> Result<Option<u64>, StoreError>; // the user's message to answer
pub enum ChatWaiting { Answering, DaySpent, Asleep { until: DateTime<Utc> }, KeyRefused, Retired } // T4
// crates/runtime/src/session.rs
SessionPurpose::Chat
// crates/runtime/src/orchestrator.rs
TickReport::Chat { agent_id: String, what: String }
// crates/runtime/src/orchestrator/rules.rs
const CHAT_TOOLS: &[&str] = &["farik_read_task", "farik_read_board", "farik_read_rules",
    "farik_read_criteria", "farik_read_decisions", "farik_chat_reply"];
// crates/runtime/src/tools/chat.rs
pub struct ChatReplyInput { pub text: String, pub request: Option<ProposedRequest> }
pub(super) fn chat_reply(call: &Call<'_>, input: ChatReplyInput) -> Result<Value, ToolError>;
// crates/store/src/projections.rs
CostScope::Purpose
// crates/store/src/requests.rs
pub fn file_request(files, log, wire, created_by, parent, now, ids, from_chat_message: Option<u64>) -> Result<TaskContract, RequestError>;
```

Wire: event `chat_message.posted { chat, author, text, in_reply_to?, request?: { title, text } }`; command `chat_message_post { agent_id, text }`; queries `chats.list {}`, `chat.messages { agent_id, before_seq?, limit }` (limit 1 to 200, default 100) → `{ messages: [{ seq, at, author, text, in_reply_to, request, sent_as }], waiting }`; `costs.summary`'s `conversations_today_usd`; `request.file { text, from_chat_message? }`; `task.created`'s `from_chat_message?`; purpose `chat` in `session.started` and `cost.recorded`. TypeScript: `pages/Chats.tsx` `Chats()`, `pages/OneToOne.tsx` `OneToOne()`.

## Tasks

### Task 1: The mockups, and the founder's gate

Files: created `docs/design/mockups/Chats.dc.html`, `PhoneChats.dc.html`; modified `OneOnOne.dc.html`, `Costs.dc.html`, `canvas.json`; published to the design canvas (`docs/design/web-ui.md`'s link).

In the style and tokens of the other screens:
- **Chats** (1440 wide): the rail with the chosen label; the list, Team first, then Mira, Ada and Theo, each with avatar, name, role and last line, and "Past teammates" with one retired agent; the Team chat open, as step 10 built it.
- **PhoneChats** (360 wide): the avatar row above an open one-to-one.
- **OneOnOne**, redrawn: "Talking with Mira", the read-only intro, "Back to Team"; your lines labelled You; Mira's reply with the proposed request in an editable box and "Send as a request"; after sending, "Sent as FRK-12" linked; the "Mira is thinking…" line; the budget-spent note in plain words.
- **Costs**: the "Conversations today" line under the table.

No test: this task is a design, checked by the founder. `canvas.json` stays valid JSON (`python3 -m json.tool` exits 0).

**Gate: no later task starts until the founder approves these mockups, recorded in this plan's header.**

- [x] `docs(design): mock up the chats`

### Task 2: The chat record, its queries and the command

Consumes nothing from later tasks. Tests:

- `records_a_chat_message` — the user's message and a reply with `in_reply_to` and `request` validate against `event.schema.json`, line breaks kept, the envelope naming the chat's agent.
- `refuses_a_chat_message_out_of_bounds` — blank, 4,001 code points, an unknown agent and a retired agent are each refused with a sentence; 4,000 is accepted.
- `keeps_chats_out_of_the_channel` — with chats in the log, `channel.messages`, `channel_summary`, `pending_mentions` and `farik channel` read none, and `@theo` in a chat to Mira starts no conversation.
- `pages_one_chat` — `chat.messages { agent_id: mira }` holds Mira's chat alone, oldest first; `before_seq` gives the page before; a limit of 0 or 201 is refused by the schema.
- `lists_the_chats` — `chats.list` gives every agent in team order with its last message (null when none), `retired` set for a retired one, and `team_last` skipping system lines.
- `chats_from_the_command_line` — `farik chat mira "<text>"` sends `chat_message_post` and records the user's message; `farik chat mira` prints it.

- [x] `feat(runtime): record one-to-one chats apart from the channel`

### Task 3: The chat session under the read tier

Consumes `post_chat`, `pending_chat` from Task 2. Tests:

- `starts_a_chat_for_the_oldest_pending` — with two pending chats, one tick starts one `chat` session, for the older, before any sprint rule, with the agent's own model, effort `low`, `in_reply_to` the user's message and `chat` on `session.started`, and reports `TickReport::Chat`; a chat already answered starts none; a message sent while a session ran is pending after it; a tick scoped to FRK-1 starts none.
- `records_a_chat_cost_as_chat` — a chat session's `cost.recorded` has purpose `chat`, no task, and counts toward the day's spending that `check_budgets` reads; `team.activity` reads "Answering your chat" and validates.
- `gives_a_chat_the_read_tier_alone` — for a Developer granted every tier, the spec's `farik_tools` are exactly `CHAT_TOOLS`, its built-ins exactly `allowed_builtins({Read})`, no MCP server besides Farik's, and the registration's tiers `[Read]`.
- `denies_a_chat_everything_else` — for a Designer with `playwright` in its `mcp_servers`, through the hook, a chat session's `Edit`, `Bash`, `WebFetch`, `farik_create_task`, `farik_post_message`, `farik_write_memory` and an `mcp__playwright__` tool are denied, and its connectors are empty; `Read` inside the project is allowed and `Read` of `.farik/local/farik.db` is denied.
- `offers_the_reply_only_in_a_chat` — `farik_chat_reply` is in no `implement`, `verify` or conversation session's tools.
- `records_the_reply` — `farik_chat_reply` appends one `chat_message.posted` by the agent with `in_reply_to` and `request`; a second call, a call from a non-chat session, and a request outside its limits are refused `chat_reply_refused`; after the last, a good call still records.
- `prompts_with_the_chat_alone` — the prompt holds this chat's last 16 KiB oldest first, the agent's lines inside `untrusted`, the closing instruction, and no line of another agent's chat; the same agent's `implement` prompt holds none of its chats.
- `answers_through_the_recorded_transcript` — with `chat_answers_with_a_request`, a posted question ends in Mira's reply carrying the Apple Pay request, and no task, message or memory write.

- [x] `feat(runtime): answer a chat in a read-only session`

### Task 4: Answering while paused, and why not

Consumes Task 3's rule. Tests:

- `answers_while_the_team_is_paused` — with the team paused, a tick starts the chat's session and no other rule runs; with the pause `credential_refused`, it starts none.
- `a_paused_agent_answers_its_chat` — a paused agent's chat session passes the hook's and `call_tool`'s active check; its `implement` session is still refused `agent_not_active`; a retired agent's chat starts no session.
- `says_why_a_chat_waits` — `chat.messages`' `waiting` is `answering` when pending, `day_spent` when the daily limit is spent (and no session starts), `asleep` with `until`, `key_refused`, `retired`, in that precedence, `answering` while a session runs, and null when answered.
- `says_no_answer_after_a_failed_session` — a chat session that ends without `farik_chat_reply` leaves `waiting: no_answer`, and no second session starts until the user writes again.
- `wakes_serve_on_a_chat` — `chat_message_post` through the daemon wakes a waiting `farik serve` loop within one tick.

- [x] `feat(runtime): answer chats while the team is paused`

### Task 5: Conversations on the Costs page's figures

Tests:

- `sums_costs_by_purpose` — `costs_for(CostScope::Purpose, CostWindow::Day(d))` sums one day's rows by purpose.
- `answers_the_conversations` — `costs.summary`'s `conversations_today_usd` is today's `chat` spending and 0 with none.

- [ ] `feat(runtime): count chats as conversations`

### Task 6: Sending a proposal as a request

Tests:

- `files_a_chat_proposal` — `request.file { text, from_chat_message }` files a draft with `created_by: human` and `task.created`'s `from_chat_message` set; the edited words are the ones filed.
- `refuses_a_bad_proposal_link` — a seq that is not a chat reply, a reply without `request`, and a reply already sent are refused with a sentence, and nothing is filed.
- `files_a_proposal_once` — two `request.file` calls with one `from_chat_message`, raced on two threads, file one task; the other is refused as already sent.
- `shows_what_was_sent` — `chat.messages` gives that reply `sent_as: "FRK-1"` and null for others.

- [ ] `feat(runtime): file a chat's proposed request by the human's hand`

### Task 7: The chat list and the one-to-one in the browser

Vitest and axe. Tests:

- `lists_the_chats` — Team first, then each agent with avatar, name, role and last line, and "Past teammates" for a retired one; the rail reads the approved label.
- `opens_a_one_to_one` — `/channel/mira` shows the history with your lines labelled You and line breaks kept, and "Back to Team".
- `sends_a_chat` — Send and Enter send `chat_message_post`; Shift+Enter makes a line; 4,001 code points are refused before sending.
- `shows_no_such_teammate` — `/channel/nobody` shows "No such teammate" beside the list.
- `appends_chat_replies_live` — a `chat_message.posted` event for Mira is appended without a query; one for Theo is not.
- `sends_a_proposal_as_a_request` — the box holds the title and text, editable; the button calls `request.file` with `fromChatMessage`; then "Sent as FRK-12" links to its request.
- `says_why_no_answer` — each `waiting` reason shows its sentence, `no_answer` among them; a `session.started` or `session.ended` of the open chat's agent with purpose `chat` re-queries it.
- `keeps_a_past_teammate_read_only` — a retired agent's chat has no box.
- `renders_agent_text_as_text` — `<img src=x onerror=alert(1)>` as a reply, a proposal's title and a list's last line shows as those characters and adds no `img`.
- `shows_conversations_on_costs` — the Costs page's line with `conversationsTodayUsd`.
- `passes_axe_on_the_chats` — the list and a one-to-one have no axe violations.

- [ ] `feat(web): add the chat list and one-to-one chats`

### Task 8: The journey through the real server

`farik-e2e-serve` gains `chat_answers_with_a_request`. `chats.spec.ts`, team `pm-architect-developer`, transcripts `chat_answers_with_a_request`, `triage_frk_1_small_by_pm`, `ask_with_choices_frk_1`:

1. open the chat list and Mira's chat;
2. ask "Could customers also pay with Apple Pay?";
3. see Mira's reply appear by itself, with the proposed request, and assert no `message.posted` was recorded so far;
4. press "Send as a request", see "Sent as FRK-1";
5. open Today and find FRK-1 there ("Mira has a question" about it);
6. take screenshots at 360 and 1280 px.

- [ ] `test(web): chat with an agent through the real server and browser`

### Task 9: The spec and the plan

`docs/SPEC.md`, in the next free revision when this step lands (0.34: 0.31 is step 11, 0.32 the DevOps Engineer, 0.33 step 12), as the design's table lists: 3 (the channel and chats), 4.3 (the one-to-one as built), 5.1 (chat is not command), 5.2 (the chat rule runs while paused), 4.4 (a paused agent still answers its chat), 5.5 (a chat's cost, the daily limit), 5.9 (chats apart from the channel; their privacy depends on `.farik/local/**` staying protected), 8.2 (the `chat` session and its tools), 8.4 (chats in the log), 8.5 (`chat_message.posted`, `from_chat_message`, purpose `chat`, `session.started`'s `chat`, and the attribution exception: the envelope names the chat's agent on the user's message too), F7 and F8. The project plan's step 13 line records the landing.

- [ ] `docs(spec): specify one-to-one chats`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed, with 24 new tests (T2 6, T3 8, T4 5, T5 2, T6 4);
#   @farik/web: step 12's landed count plus 11; playwright: step 12's count plus 1;
#   last line: xtask check: ok
```
