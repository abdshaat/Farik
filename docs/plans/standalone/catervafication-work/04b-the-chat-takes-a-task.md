# Standalone catervafication-work, plan 4b: the chat decides to take a task

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 4.3, 5.1 ("Chat is not command"), F8
Depends on: plans 1 to 4 of this branch
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The founder, verbatim: "The agent does not stop his current task, but it forks itself to either
answer the user question or create a new task to the backlog. The process of agents answering
questions must be quick."

The agent's chat session is that fork. It decides by itself:
- A question: it answers, as today.
- A request for work the agent can do: it takes the work with `catervas_take_task`. That files the
  task given to itself, the same way plan 4 files one. It then replies in one line naming the
  task.
- Work for another role: it proposes a request in its reply, as today.

"Send as a task" (plan 4) stays, as the owner's way to force a task. A task taken this way follows
plan 4 from there: the agent writes its own contract, and the task is next in its queue.

Out of scope:
- Running the fork beside the agent's current session, and making the answer quick. ADR 0064
  (parallel agents, planned separately) owns both. Today's chat rule runs first in each tick, and
  this plan changes nothing about when it runs.

## Decisions

- **One tool, chat sessions only.** `catervas_take_task { text: string }` has tier `Read` and is
  listed in `CHAT_TOOLS` alone, so no other session is offered it.
  - It files from the message the session answers, `call.context.in_reply_to`.
  - Chosen over letting the chat call `catervas_create_task`: that files an untriaged request for
    the Product Manager, and the founder wants the agent to take the work itself.
- **Input.** `text` is the task's title in the agent's words: one line, 3 to 80 code points once
  trimmed. It is the title and nothing else.
- **The intent is the owner's message, never the agent's words** (the guarantee). `take_task`
  reads the `text` of the `chat_message.posted` at `in_reply_to`, and the contract is that text
  through `given_request`, its `title` then replaced by the input's `text`. So nothing the session
  read elsewhere (a file, a task, a seller's reply) becomes the work: it can take, or not take,
  what the owner wrote. Plan 4's `check_task_message` refuses any other intent, and
  `given_task_intent_fixed` keeps it through refining.
- **The contract** is otherwise the one plan 4's command files. `given_request(team, agent, text,
  rules) -> Result<Value, String>` is extracted from `post_chat_message` into
  `crates/runtime/src/chat.rs` and called by both, with the owner's message as `text`:
  - `request_from_text`;
  - the agent's role as `assignee_role`;
  - `reviewer_for` as reviewer.
- **Recorded:**
  - `file_given_task` with `created_by` the agent (it decided this is work), `given_to` the agent,
    and `from_chat_message` the answered seq;
  - `task.created`, then `request.triaged { size: small, reason: "a task the owner gave <agent id>
    in their chat", triaged_by: "catervas" }`, exactly as plan 4.

  The owner's message then shows "Given to <Name> as <id>" through `sent_as` (plan 5), with no
  web change.
- **Refusals**, `ToolError::Refused`, checked in this order, nothing recorded on any:
  1. `take_task_refused: only your chat session takes a task, for the message it answers`, when
     the purpose is not `Chat` or `in_reply_to` is `None`;
  2. `agent_not_active: you are paused, so you cannot take work; answer instead`, since a paused
     agent's chat still runs (4.3);
  3. `task_title: a title is one line of 3 to 80 characters`;
  4. `task_too_short: the owner's message is under 20 characters, so answer it and ask what they
     need`;
  5. `already_a_task: <id>`, when the store answers "was already sent, as <id>", for example after
     "Send as a task" or a second call.
- **Answer.** `{ "task_id": "<id>", "next": "Reply now in one line naming <id>: you have taken it
  and start once your current work allows." }`.
- **The chat's closing instruction** (`prompt.rs`, `CLOSING_INSTRUCTIONS`' `Chat` entry). The
  sentence "When work is needed, put a request in your reply, a title and what it asks for, for
  the user to send." becomes:

  > When the message asks you for work your role does, take it with `catervas_take_task` and
  > reply in one line naming the task. When it asks for work another role does, put a request in
  > your reply, a title and what it asks for, for the user to send. Otherwise answer.

  The rest of the entry stays. The Product Manager's interview instruction is unchanged.
  `catervas_chat_reply`'s `TOOLS` description, "When work is needed, add a request", becomes
  "When work another role does is needed, add a request" (Task 1, in `tools.rs`).
- **The chat stays otherwise read-only.** The session still writes nothing in the project and
  moves no task. Its one new act files a task given to itself, which the Definition of Ready, the
  WIP limit and the review of code then hold (ADR 0063 amends ADR 0026 part B).

## File map

```
crates/runtime/src/chat.rs                     modifies: given_request (Task 1)
crates/runtime/src/orchestrator/human.rs       modifies: post_chat_message calls given_request (Task 1)
crates/runtime/src/tools/chat.rs               modifies: TakeTaskInput, take_task, mod tests (Task 1)
crates/runtime/src/tools.rs                    modifies: TOOLS entry, dispatch arm, catervas_chat_reply's description (Task 1)
crates/runtime/src/orchestrator/rules.rs       modifies: CHAT_TOOLS, mod tests (Task 2)
crates/runtime/src/prompt.rs                   modifies: the Chat closing instruction (Task 2)
```

## Interfaces

Consumes: `file_given_task`, `request_from_text`, `placeholder_budget_usd` (`catervas-store`);
`reviewer_for` (`catervas-roles`); `Call`, `call.context.in_reply_to`, `call.team`, `TOOLS`;
`post_chat` (to put the owner's message in the fixture), `TestProject`, `a_team_of_three`,
`run`, `TestProject::context` (`tools::fixtures`); `CLOSING_INSTRUCTIONS`, `CHAT_TOOLS`.

Produces:

```rust
// catervas-runtime::chat
pub(crate) fn given_request(team: &Team, agent: &Agent, text: &str, rules: &TeamRules) -> Result<Value, String>;

// catervas-runtime::tools::chat
#[derive(Debug, Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]
pub(crate) struct TakeTaskInput { text: String }
pub(super) fn take_task(call: &Call<'_>, input: &TakeTaskInput) -> Result<Value, ToolError>;
```

The `TOOLS` entry's description is "In your one-to-one chat: take the work the owner's message
asks for as your own task; text is its one-line title, and its intent is the owner's message. You
write its contract and start once your current work allows. Then reply in one line naming it."

## Tasks

### Task 1: catervas_take_task

Files: `crates/runtime/src/chat.rs`, `crates/runtime/src/orchestrator/human.rs`,
`crates/runtime/src/tools/chat.rs`, `crates/runtime/src/tools.rs`. The fixture is a `TestProject`
of `a_team_of_three(|_| {})` with the owner's message "Please add a dark mode switch to Settings"
posted in `dev-a`'s chat at seq `n`, and "Dark mode?" at seq `m`, through `post_chat`. The calls
come from a `Chat` session of `dev-a`: `project.context("dev-a", None)` with `purpose` `Chat` and
`in_reply_to` `Some(n)` set (as `limits_a_conversation_to_one_post` sets them), then `run`.

- `takes_the_answered_message_as_its_task`:
  - `{ text: "Dark mode switch in Settings" }` answers `task_id` `CTV-1`, and `next` names it;
  - `task.created` has `created_by` `dev-a`, `given_to` `dev-a` and `from_chat_message` `n`, then
    `request.triaged` small by `catervas`;
  - the contract's `assignee_role` is `software_developer`, its title "Dark mode switch in
    Settings" and its intent "Please add a dark mode switch to Settings", the owner's words;
  - plan 4's `gives_an_agent_a_task_from_its_chat` still passes through `given_request`.
- `takes_a_task_only_from_its_own_chat` (the guarantee):
  - each of these is refused with the code named, and the log holds exactly one `task.created`
    afterwards:
    - a second call: `already_a_task: CTV-1`;
    - from an `Implement` session: `take_task_refused: `;
    - a chat session with no `in_reply_to`: `take_task_refused: `;
    - text "ab": `task_title: `;
    - `in_reply_to` `Some(m)`: `task_too_short: `;
    - `dev-a` paused: `agent_not_active: `.

- [x] `feat(runtime): an agent's chat takes the owner's request as its own task`

### Task 2: offered in the chat, and the chat told when

Files: `crates/runtime/src/orchestrator/rules.rs` (`CHAT_TOOLS` gains `"catervas_take_task"`
before `"catervas_chat_reply"`), `crates/runtime/src/prompt.rs`.

- `a_chat_session_may_take_a_task` (`rules.rs` `mod tests`):
  - `CHAT_TOOLS` contains `catervas_take_task` and still ends with `catervas_chat_reply`;
  - the `Chat` closing instruction contains "take it with `catervas_take_task` and reply in one
    line naming the task" and "When it asks for work another role does";
  - no other `CLOSING_INSTRUCTIONS` entry names `catervas_take_task`.

  Any existing test that pins the chat's tool list or its instruction is updated to the new text.

- [ ] `feat(runtime): the chat session takes work its role does and answers the rest`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
