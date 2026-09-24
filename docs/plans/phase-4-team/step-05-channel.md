# Phase 4, step 05: Channel

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 3, 5.1, 5.7, 5.9, 8.2, 8.5; F7, F17; D11
Depends on: phase 3 (merged in #11); steps 01 to 04 of this phase (committed on this branch before this step starts)
Readiness confirmed by: fresh-session reviewer, 2026-09-24 (one round, no decision open; its findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The team has a channel the human can read and write from the command line. An agent says in one or two sentences what it just did to its own task, from the session that did it; Farik says in a plain line what the governor or the human did, with no model; an agent or the human can mention an agent by `@<id>`, which gets it one short conversation session to answer in; each agent has one unprompted message per sprint. Nothing said in the channel creates work: the only way from the channel to work is `farik_create_task` without a parent, a request that goes to triage (5.9). Out of scope: ceremonies and their threads' content (step 06), the channel's view in the desktop app (phase 5), and one-on-ones (phase 6).

## Decisions

- A message is a `message.posted` event, the channel's only storage (revision 11): `{ author, kind, text, mentions, thread, in_reply_to }`. `author` is an agent id, `human`, or `farik`, and is the event's attribution; the envelope carries the agent id for an agent's message (none for `human` or `farik`) and the task the message is about, when there is one. A message is not about one contract, since most have none. `kind` is one of `reaction`, `ambient`, `reply`, `ceremony`, `system`, `human`. `text` is non-empty, at most 2,000 characters, and Farik turns each line break in it into a space, so a message is one line. `mentions` are agent ids, unique. `thread` is `planning`, `standup`, `review`, or `retro`, absent otherwise. `in_reply_to` is the seq of the message a reply answers. `ceremony` and `thread` are in the schema from this step; the tool's ceremony branch is step 06's.
- Mentions are parsed by Farik from the text, never taken from the agent: each `@<id>` whose id is an agent of the team, active or not, once each, the author left out. `@human` is not a mention: the human reads the channel. System lines carry no mentions.
- `farik_post_message { text }`, tier `read`, is offered in every session but a one-tool session (triage, judgment, sprint planning). Its `kind` is decided by Farik from the session's purpose, which `SessionRegistration` and `ToolContext` now carry (`purpose: SessionPurpose`, set by `run_session`):
  - `reply` in a `conversation` session, one post, a second refused;
  - `reaction` for the first post of any other session;
  - `ambient` for every further post of such a session, refused past the agent's allowance: `policy.ambient_messages_per_sprint` (team schema, default 1, 0 to 20) counts the agent's `ambient` messages since the open sprint's `sprint.started`, or since the UTC day began with no sprint open.
  - The refusal says `channel_limit: <why>`. A reaction's and an ambient message's envelope names the session's task.
- Reactions are asked for in the prompt. The `This session` text of `refine`, `plan`, `implement`, and `verify` gains one sentence: after asking for a move, post one or two sentences about it with `farik_post_message`, in your persona's voice, naming the task. The tool's description carries the style guide in one line: one or two sentences, what happened and what is next, no instruction to anyone. No session is started to react (the founder, 2026-09-24).
- System lines are posted for a move whose actor is the governor or the human (the founder's decision: a move the governor or the human makes, which has no session), and for the rejection Farik files in a reviewer's name (verify.rs `reject`, whose session has ended), so that 5.9's "rejected with the one-line reason" holds. Bookkeeping moves Farik makes on an agent's behalf (a triaged draft to `refining`, an assigned task to `in_progress`, an epic's assignment) post nothing, which keeps the register minimal (D11). `Transitions::record_move` posts `message.posted { author: farik, kind: system }` after the move's own events (`task.transitioned`, then `escalation.raised` when there is one): `<id> <from> → <to> (by <who>)`, then `: <reason>` when the move carries one (the escalation reason, the human's reason, the blocker, the resolution, or the rejection's reasons), cut to 2,000 characters ending with `…`, so the line never fails a move that is recorded. Farik also posts a system line when `budget.exhausted` records `DayUsd` or `SprintUsd`, and when `agent.slept` is recorded. A failure to append a system line fails the call as the move's own event would.
- `post` takes the log, not the tools' dependencies, so that `record_move` and `record_exhaustion`, which hold no `ToolDeps`, can call it: `post(log, clock, ids, NewMessage) -> Result<u64, ChannelError>`.
- A mention starts a session. The channel rule runs after the budget rule and before rule 3, under `TickRules::All` only, when the tick has no `task_id` scope. For the first agent in team order that has pending mentions, is `active`, is not asleep (step 04), and while the daily budget is not spent (step 03's day-only check), it starts one `conversation` session:
  - on `claude-sonnet-5` at `low` effort whatever the agent's model (revision 11: channel sessions use Sonnet 5), following the triage override;
  - about no task, `cwd` the files' root, the read tier's built-ins alone;
  - its Farik tools given by `SessionAsk::tools: Option<&'static [&'static str]>`, an explicit list intersected with what the agent's tiers allow: the reading tools (`farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_criteria`), `farik_post_message`, and `farik_create_task`, which refuses a `parent` when its session's purpose is `conversation`;
  - its first message `mention_message(agent, pending, summary)`: each pending message with its author, seq, and text in one `untrusted` block cut at 16 KiB, then the channel summary;
  - its `This session` text, the `Conversation` closing instruction rewritten: answer the mentions above once with `farik_post_message`, and file any work as a request with `farik_create_task`;
  - its one post a `reply` with `in_reply_to` the latest pending message's seq.
  - The tick reports it as `TickReport::Conversation { agent_id, what }`, printed `<agent>: <what>`, and with `--json` as `{"agent_id": ..., "what": ...}`.
- A mention of agent X is pending while it is newer than X's last `conversation` session start: `pending_mentions` reads every `message.posted` and keeps those whose `mentions` hold X and whose kind is neither `reply` nor `system`, after the seq of X's last `session.started` of purpose `conversation` (read with `EventQuery { agent_id: X, kinds: [session.started] }`). Mentions in a `reply` start no session: replies are one deep. A mention of a paused, retired, or sleeping agent waits.
- The channel summary is derived with no model (revision 11): `channel_summary(log, files) -> String` is the most recent messages that fit 2,000 tokens counted as `ceil(characters / 4)`, oldest first, one line each (`<author> [<thread>]: <text>`). It is written to `.farik/local/channel-summary.md` whenever it is computed, so the human can read what the agents were shown. It reaches mention sessions here and ceremony sessions in step 06, always in the first message, inside an `untrusted` block, so ADR 0011's prompt sections do not change.
- The human posts with `farik say <text>`: command `MessagePost { text }` (body `messagePostBody { text }`), handled by `human::handle`, recorded with `author: human, kind: human`, mentions parsed, its `said` "posted in the channel". It is routed as the human's other commands are (`start::command`).
- `farik channel [--last <n>] [--json]` prints the channel from the log (`EventQuery { kinds: [message.posted] }`), oldest first, default the last 50: `<time> <author> [<thread>] <text>`, each through `printable` as every agent-written text is (8.6); `--json` gives one object per line. The project plan's `Projections::channel` is not built: the log's kind filter serves the command line, and phase 5's view adds a projection if it needs one.
- `farik metrics` counts the channel (D11): `HarnessMetrics` gains `messages: MessageCounts { reaction, ambient, reply, ceremony, system, human }`, counted from the log's events (messages are not projected); a sprint's are the messages whose seq falls after its `sprint.started` and, once it ended, before its `sprint.ended`. The costs of `conversation` and `ceremony` sessions are already split by purpose (F17).

## File map

```
docs/schemas/event.schema.json, docs/schemas/command.schema.json, docs/schemas/team.schema.json   modifies: message.posted, message_post, policy.ambient_messages_per_sprint
crates/core/src/team.rs                          modifies: the policy's field and default
crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs,command.rs}   modifies
crates/store/src/metrics.rs                      modifies: MessageCounts
crates/runtime/src/channel.rs, crates/runtime/src/lib.rs   creates / modifies: mentions_in, channel_summary, post, pending_mentions
crates/runtime/src/tools.rs, crates/runtime/src/tools/channel.rs, crates/runtime/src/tools/refusal.rs, crates/runtime/src/tools/contracts.rs   modifies / creates: farik_post_message; create_task's no-parent rule in a conversation
crates/runtime/src/daemon/mcp.rs                 modifies: the tool count
crates/runtime/src/transitions.rs                modifies: record_move's system line
crates/runtime/src/cost.rs, crates/runtime/src/orchestrator/session.rs   modifies: system lines for DayUsd, SprintUsd, and agent.slept
crates/runtime/src/prompt.rs                     modifies: the reaction sentence in four closing instructions
crates/runtime/src/orchestrator/{rules,messages,human,session,verify}.rs   modifies: the channel rule, mention_message, MessagePost, the conversation session's tools, the rejection's line
crates/runtime/src/daemon.rs, crates/runtime/src/tools.rs, crates/runtime/src/orchestrator.rs, crates/cli/src/run.rs   modifies: the session's purpose in its registration and tool context; TickReport::Conversation and its printing
crates/runtime/src/recorded/transcripts/reply_to_a_mention.jsonl, implement_reacts_frk_1.jsonl   creates
crates/cli/src/lib.rs, crates/cli/src/channel.rs, crates/cli/src/metrics.rs, crates/cli/tests/{human,reading}.rs   modifies / creates
docs/SPEC.md, docs/plans/project-plan.md         modifies
```

## Interfaces

Consumes: `record_move`, `TransitionAsk::session_id`, `run_session`, `SessionAsk` (with `contract: Option`, `only_tool`), `asleep` (step 04), `Projections::open_sprint` (step 03), `human::handle`, `start::command`, `untrusted_block`, `escape` of the command line.

Produces:

```rust
// farik-protocol: EventBody::MessagePosted(MessagePostedBody { author, kind: MessageKind, text, mentions: Vec<String>, thread: Option<Thread>, in_reply_to: Option<u64> }); Command::MessagePost { text: String }
// farik-core::team: TeamPolicy::ambient_messages_per_sprint: u32 (default 1)
// farik-runtime::channel
pub fn mentions_in(text: &str, team: &Team, author: &str) -> Vec<String>;
pub struct NewMessage { pub author: String, pub agent_id: Option<String>, pub kind: MessageKind, pub text: String, pub mentions: Vec<String>, pub task_id: Option<TaskId>, pub thread: Option<Thread>, pub in_reply_to: Option<u64>, pub session_id: Option<String> }
pub fn post(log: &EventLog, clock: &dyn Clock, ids: &EventIds, message: NewMessage) -> Result<u64, ChannelError>;
// farik-runtime: SessionRegistration and ToolContext gain purpose: SessionPurpose; SessionAsk gains tools: Option<&'static [&'static str]>; TickReport::Conversation { agent_id: String, what: String }
pub fn pending_mentions(log: &EventLog, agent_id: &str) -> Result<Vec<FarikEvent>, StoreError>;
pub fn channel_summary(log: &EventLog, files: &ProjectFiles) -> Result<String, ChannelError>;
pub enum ChannelError { Store(StoreError), Files(FilesError), Refused { reason: String } }
// farik-store::metrics
pub struct MessageCounts { pub reaction: u32, pub ambient: u32, pub reply: u32, pub ceremony: u32, pub system: u32, pub human: u32 }
```

## Tasks

### Task 1: the message and the human's post

Files: the three schemas, `team.rs`, protocol, `channel.rs`, `orchestrator/human.rs`, `cli/src/lib.rs`, `cli/src/channel.rs`, `cli/tests/human.rs`, `cli/tests/reading.rs`
- `finds_the_mentions_in_a_message` — "@dev-a and @dev-b, and @dev-a again, @nobody, @human" in a team with dev-a and dev-b, written by dev-b: `[dev-a]`.
- `posts_the_humans_message` — `MessagePost { text: "@dev-a how is FRK-1?" }`: one `message.posted { author: human, kind: human, mentions: [dev-a] }`.
- `refuses_an_empty_message` — text "   ": `Invalid`, nothing recorded.
- `refuses_a_message_too_long` — 2,001 characters: `Invalid`.
- `says_and_shows_the_channel` (cli) — `farik say "hello @dev-a"` then `farik channel`: a line with `human` and `hello @dev-a`; `--json` has `"kind":"human"`.
- `escapes_what_an_agent_wrote_in_the_channel` (cli) — a message holding `\u001b[31m`: printed as the six characters `\u001b`.
- `defaults_the_ambient_allowance` (core) — a team file without it: 1.

- [x] `feat(runtime): let the human post in the team's channel and read it`

### Task 2: agents post

Files: `tools.rs`, `tools/channel.rs`, `tools/refusal.rs`, `daemon/mcp.rs`, `channel.rs`, `prompt.rs`, the `implement_reacts_frk_1` transcript
- `posts_a_reaction_from_a_session` — an implement session's first `farik_post_message`: `kind: reaction`, `task_id` FRK-1, the agent the author.
- `counts_a_second_post_as_ambient` — its second post in an open sprint: `kind: ambient`; a third: refused `channel_limit`, nothing recorded.
- `counts_the_allowance_per_day_without_a_sprint` — no sprint, an ambient message yesterday (UTC): today's is allowed.
- `offers_no_post_to_a_one_tool_session` (guard) — a triage session's registered tools do not include `farik_post_message`.
- `asks_for_a_reaction_after_a_move` — the `implement` closing instruction holds `farik_post_message`; `triage`'s does not.
- `lists_every_tool_with_its_tier` (changed) and the MCP tool count.

- [x] `feat(runtime): let an agent post in the channel from its session`

### Task 3: system lines

Files: `transitions.rs`, `cost.rs`, `orchestrator/session.rs`
- `posts_a_line_for_the_governors_move` — `refining -> ready` by the governor: a `message.posted { author: farik, kind: system }` holding `FRK-1 refining → ready (by the governor)`, after the `task.transitioned`.
- `posts_the_humans_reason` — the human's `escalated -> in_progress` with reason "go on": the line ends `: go on`.
- `posts_no_line_for_a_bookkeeping_move` (guard) — a triaged draft moved to `refining` for the Product Manager: no system line.
- `posts_a_line_for_a_rejection_farik_filed` — Farik files a rejection from a review note: a system line with the failed criterion's id.
- `cuts_a_long_line` — a human reason of 3,000 characters: the line is 2,000 characters ending `…`, and the move stands.
- `mentions_nobody_in_a_system_line` — a human reason "go on @dev-a": the line's `mentions` is empty.
- The human command test that asserts the move is its report's last event (human.rs ~1081) changes to the system line after it.
- `posts_a_line_when_the_day_is_spent` — a cost crossing the daily budget: a system line naming the daily budget; the same for a sprint's budget.
- `posts_a_line_when_an_agent_sleeps` — `agent.slept` recorded: a system line naming the agent and the time.

- [x] `feat(runtime): post what the governor and the human did in the channel`

### Task 4: mentions

Files: `orchestrator/{rules,messages,session}.rs`, `channel.rs`, `tools/contracts.rs`, the `reply_to_a_mention` transcript
- `answers_a_mention_in_a_conversation` — the human's "@dev-a status?": the tick starts dev-a's `conversation` session on `claude-sonnet-5` at `low`, no task, first message holding the mention's text in an `untrusted` block and the summary; the replay's post is a `reply` with `in_reply_to` that seq.
- `answers_each_mention_once` — after the reply, a tick starts no second session for dev-a.
- `answers_no_mention_on_a_spent_day` (guard) — the daily budget spent: no conversation session.
- `closes_a_conversation_with_its_reply` — the conversation session's prompt ends with the rewritten instruction naming `farik_post_message`.
- `prints_a_conversation_line` (cli, `farik run` with the recorded adapter) — the output holds `dev-a: `.
- `does_not_answer_a_reply` (guard) — dev-a's reply mentions dev-b: no session for dev-b.
- `waits_for_a_sleeping_agent_to_answer` — dev-a asleep: no session; after it wakes, the session starts.
- `files_a_request_from_the_channel` — in the conversation, `farik_create_task` without a parent files a draft request; with a parent it is refused.
- `limits_a_conversation_to_one_post` — a second post in it: refused `channel_limit`.
- `writes_the_summary_it_shows` — `.farik/local/channel-summary.md` holds what the first message's summary held, and no more than 8,000 characters.

- [x] `feat(runtime): answer a mention in a short conversation session`

### Task 5: counting the channel

Files: `crates/store/src/metrics.rs`, `crates/cli/src/metrics.rs`, `crates/cli/tests/reading.rs`
- `counts_messages_by_kind` (store) — two reactions, one ambient, one system: `MessageCounts { reaction: 2, ambient: 1, system: 1, .. }`, in the project's and a sprint's metrics.
- `prints_the_channel_counts` (cli) — `farik metrics` prints a messages line; `--json` has `messages`.

- [x] `feat(store): count the channel's messages by kind`

### Task 6: the spec

Revision 0.14 in the header, naming each change:
- 5.9: what a message is and who writes each kind; the reaction from the session that made the move, on that session's model, replacing "Ambient and reaction messages use a cheaper model"; system lines for the governor's and the human's moves and Farik's rejections; the ambient allowance and its day without a sprint; mentions and one-deep replies on Sonnet 5 at low effort; the summary.
- 5.1: chat is not command, through the one tool.
- Section 3: `farik say` and `farik channel`.
- 8.2: the conversation session's tools.
- 8.5: `message.posted`.
- F7: on the command line.
- F17: the message counts.

The step's interface line in the project plan is written as landed.

- [ ] `docs(docs): record the team channel in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
