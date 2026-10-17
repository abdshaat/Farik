# Standalone catervafication-work, plan 4: a chat message becomes the agent's task

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 4.3, 5.1 ("Chat is not command"), 5.2, 5.16, 8.5, F8
Depends on: plans 1 to 3b of this branch
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The owner sends `chat_message_post { agent_id, text, as_task: true }`: the message is posted,
marked `as_task`, and the same command files a request given to that agent (`given_to`), linked to
the message, created by the human, its `assignee_role` the agent's role, triaged small by Catervas.
The chat does not answer it. Rule 10 moves it to `refining` with no session; rule 9 gives the
refine session to that agent, which writes the contract with the Product Manager's standing on that
one task, its role and intent fixed. Readiness and the governor's own check are unchanged (no agent
judges, ADR 0061 plan 7). Once `ready` it is next in that agent's queue: rule 8 assigns it, with no
session, as soon as the agent has room. Its code is reviewed as any code is. Out of scope: the web.

## Decisions

- **One command, one flag**: `chat_message_post` gains `as_task`, optional, default false; chosen
  over a second command, since one message is one act.
- **Checked first, nothing recorded on a refusal**, after the existing checks (on the team, not
  retired):
  - `agent_not_active: <Name> is paused; resume them on the Team page to give them a task`, for
    an agent whose status is not `active`;
  - `task_too_short: say a little more: at least 20 characters`, for text under 20 code points
    once trimmed (`request_from_text`'s floor). Both are `CommandError::Refused`.
- **What is recorded, in order:**
  1. `chat_message.posted { chat, author: human, text, as_task: true }`;
  2. `task.created { created_by: "human", from_chat_message: <that seq>, given_to: <agent id> }`;
  3. `request.triaged { size: small, reason: "a task the owner gave <agent id> in their chat",
     triaged_by: "catervas" }`.

  The report is `said: "given to <Name> as <id>"`, its events those seqs.
- **The contract filed** is `request_from_text(text, placeholder_budget_usd(rules))` with
  `assignee_role` the agent's role and `reviewer_role` `catervas_roles::reviewer_for(team, role)`;
  a `None` is a failure (`CommandError` of kind failed), since every agent's role has a row.
- **`file_given_task`** files it as `file_request` does, under the same `FILING` lock, with
  `Filing.given_to`. In place of `check_proposal`, `check_task_message` refuses, as
  `RequestError::Refused`:
  - a seq not by `human` in chat `given_to`, `as_task` or not (plan 4b files from a plain one):
    "links to message <seq>, which is not the owner's message in <agent>'s chat";
  - a wire whose `intent` is not that message's `text`: "its intent is not the owner's message
    <seq>". So a given task's work is only ever the owner's words, whoever files it (plan 4b);
  - one a `task.created` already names: "was already sent, as <id>".
- **`Given`**: who the task is the agent's for. It is `Given::No` when any of these holds:
  - there is no `given_to`;
  - the agent is not on the team, or has retired;
  - the contract's `assignee_role` is no longer that agent's role (the human may change any field).

  Otherwise it is `Given::Active(agent)` for an active agent and `Given::Waits` for a paused one.
- **The chat does not answer it.** `awaits_a_reply(body)` is `body.author == "human" &&
  body.as_task != Some(true)`, and replaces `body.author == HUMAN` in two places:
  - `newest_from_the_user`, which `pending_chat` and so the chat rule read;
  - `chat_waiting`'s `user_wrote_last`, now whether the newest message that is not `as_task` is
    the human's (read newest first, a page at a time, as `newest_from_the_user` reads).

  `chat_history`'s line keeps `HUMAN`: an `as_task` message reads as the user's. So the chat shows
  no "thinking", and no chat session proposes the same work.
- **Rule 9** (`requests::refining`): the product-plan wait and the governor's `judge` are
  unchanged. The writer comes from `Given`: `Active(agent)`, that agent's refine session, its
  `initial_prompt` from `given_task_message`; `Waits`, `Ok(None)`; `No`, the Product Manager. The
  spent-day and asleep checks use the writer.
- **`GIVEN_TASK_LEAD`** (in `messages.rs`), `{name}` the agent's display name and `{role}` its
  role's wire name:

  > {name}, the owner gave you this task in your one-to-one chat, and you write its contract
  > yourself, as the Product Manager would. Its intent is their words: keep it, and set its scope,
  > requirements, exit criteria, allowed paths and budget. Its assignee_role stays {role}: you do
  > the work once it is ready. Only a code change is reviewed; a task that changes no code is
  > checked by its command, test and artifact criteria alone. Ask the owner first with
  > catervas_ask_human only if you cannot write it without an answer.

  `given_task_message` is that paragraph, a blank line, then `refine_message(contract, first,
  failures)`.
- **The writer's standing** (`tools/contracts.rs` `contract_writer`). A caller that is neither the
  Product Manager nor the Scrum Master writes as `TransitionActor::ProductManager` when both hold:
  - `given_to` names it;
  - the row is `draft` or `refining`.

  `check_contract_write` then allows content in those statuses alone, as for the Product Manager.
  Chosen over a new actor kind, which every row and gate would have to learn for one case.
- **The roles and the words stay.** `write_contract` refuses an agent's write to a given task
  - leaving `assignee_role` other than the given agent's role: `given_task_role_fixed: <id> was
    given to <agent> in its chat, so its assignee_role stays <role>`;
  - leaving `reviewer_role` other than `catervas_roles::default_reviewer_role(team,
    TaskKind::Task, <role>)`, or the role it was filed with when that is `None`:
    `given_task_reviewer_fixed: <id> was given to <agent> in its chat, so its reviewer_role stays
    <reviewer role>`. The doer never picks its reviewer; it matters for code alone (ADR 0061);
  - changing `intent`: `given_task_intent_fixed: <id> was given to <agent> in its chat, so its
    intent stays the owner's words`.

  This holds whoever the writer is, the Product Manager included. The human's
  `contract.save` is not held: the human writes anything, and `Given::No` then returns the task
  to the ordinary rules.
- **Rule 8.**
  - `tick` takes the ready rows in two passes: first those whose `given_to` is set, then the
    rest, each in board order.
  - `ready()` starts by matching `Given`: `Active(agent)` → `ready_given`; `Waits` → `Ok(None)`;
    `No` → as today.
  - `ready_given` returns `Ok(None)` when any of these holds:
    - the agent has no room (`has_room`, `under` none);
    - `refused_since_entering(ready → assigned)`;
    - there is no `assigner(team)`;
    - there is no reviewer;
    - `assignable(deps, team, &contract, assigner, agent)` is false.
  - Its reviewer: the first active agent, in team order, of the contract's `reviewer_role` that
    is not the assignee; else the assignee, when that role is its own and `reviews_its_own()`.
  - Otherwise it asks `ready → assigned`:
    - the actor is `ScrumMaster` or `ProductManager`, after the assigner's role, with the
      assigner's id;
    - the ask is `TransitionAsk { assignee_id, reviewer_id, .. }`;
    - moved, it reports `Acted { what: "assigned it to <agent id>, who was given it in its chat" }`.
  - A full agent's task waits, and is assigned in the first tick the agent has room, before the
    second pass offers it anything else ("Next in his queue"). What counts as busy is unchanged.

## File map

```
docs/schemas/command.schema.json                modifies: chatMessagePostBody.as_task (Task 1)
docs/schemas/event.schema.json                  modifies: chatMessagePostedBody.as_task, taskCreatedBody.given_to (Task 1)
crates/protocol/src/command.rs, event.rs        modifies: Command::ChatMessagePost.as_task; generated bodies (Task 1)
crates/store/src/requests.rs                    modifies: Filing.given_to, file_given_task, check_task_message, given_to, mod tests (Task 2)
crates/runtime/src/chat.rs                      modifies: NewChatMessage.as_task, awaits_a_reply (Task 3)
crates/runtime/src/orchestrator/human.rs        modifies: post_chat_message, mod tests (Task 3)
crates/runtime/src/tools/fixtures.rs            modifies: TestProject::filed_given_to (Task 4)
crates/runtime/src/tools/contracts.rs           modifies: contract_writer, write_contract, mod tests (Task 4)
crates/runtime/src/orchestrator/messages.rs     modifies: GIVEN_TASK_LEAD, given_task_message (Task 5)
crates/runtime/src/orchestrator/requests.rs     modifies: Given, given_agent, refining, mod tests (Task 5)
crates/runtime/src/orchestrator/fixtures.rs     modifies: Harness::file_given (Task 5)
crates/runtime/src/orchestrator/rules.rs        modifies: tick's rule 8 passes, ready, ready_given, mod tests (Task 6)
```

Every `Command::ChatMessagePost` and `NewChatMessage` not setting it gains `as_task: false`. The
web client's types are generated from the schemas; nothing there is edited.

## Interfaces

Consumes: `post_chat`, `NewChatMessage`, `HUMAN`, `chat_waiting` (`catervas-runtime` chat);
`file`, `Filing`, `FILING`, `request_from_text`, `placeholder_budget_usd`, `TOO_SHORT`
(`catervas-store` requests); `reviewer_for`, `default_reviewer_role`, `Role::reviews_its_own` (`catervas-roles`,
`catervas-core`); `check_contract_write`, `ContractWriteActor`, `TransitionActor`;
`refine_message`, `refused_since_entering`, `has_room`, `assignable`, `assigner`, `active`,
`run_session`, `SessionAsk`; `Harness`, `with_the_suggested_six`, `recorded`, `who`; transcript
`implement_stops_early`.

Produces:

```rust
// catervas-protocol
Command::ChatMessagePost { agent_id: String, text: String, as_task: bool }

// catervas-store::requests
pub fn file_given_task(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str, given_to: &str,
    from_chat_message: u64, now: DateTime<Utc>, ids: &EventIds) -> Result<TaskContract, RequestError>;
pub fn given_to(log: &EventLog, task: &TaskId) -> Result<Option<String>, StoreError>;

// catervas-runtime::chat
pub struct NewChatMessage { /* … */ pub as_task: bool }
pub(crate) fn awaits_a_reply(body: &ChatMessagePostedBody) -> bool;

// catervas-runtime::orchestrator
pub(super) enum Given<'a> { No, Active(&'a Agent), Waits }
pub(super) fn given_agent<'a>(deps: &OrchestratorDeps, team: &'a Team, contract: &TaskContract) -> Result<Given<'a>, OrchestratorError>;
pub(super) const GIVEN_TASK_LEAD: &str;
pub(super) fn given_task_message(contract: &TaskContract, agent: &Agent, first: bool, failures: &[String]) -> String;
async fn ready_given(deps: &OrchestratorDeps, team: &Team, board: &[TaskProjection], row: &TaskProjection,
    contract: &TaskContract, agent: &Agent) -> Result<Option<TickReport>, OrchestratorError>;

// fixtures
TestProject::filed_given_to(&self, task: &str, status: &str, agent: &str, change: impl FnOnce(&mut Value));
Harness::file_given(&self, task: &str, status: &str, agent: &str, change: impl FnOnce(&mut Value));
```

Schemas: `chatMessagePostBody.as_task` `{ "type": "boolean" }`, not required;
`chatMessagePostedBody.as_task` `{ "type": "boolean", "const": true }`, described "The owner sent
this message as a task for the chat's agent (ADR 0063); the chat does not answer it.";
`taskCreatedBody.given_to` `{ "type": "string", "minLength": 1, "maxLength": 64 }`, described "The
agent the owner gave this task to in its one-to-one chat (ADR 0063): from_chat_message names that
message, the agent writes the contract and is assigned it." `taskCreatedBody`'s description adds
"With given_to, from_chat_message is the owner's own message in that chat, and created_by is human
or that agent." The command's wire writes `as_task` only when it is true.

## Tasks

### Task 1: the wire

Files: the two schemas, `crates/protocol/src/command.rs`, `crates/protocol/src/event.rs`.

- `reads_a_chat_message_sent_as_a_task` (`command.rs` `mod tests`):
  - `chat_message_post` with `{ agent_id: "dev", text: "Add a dark mode switch", as_task: true }`
    reads as `ChatMessagePost { as_task: true, .. }`;
  - without `as_task` it reads as `as_task: false`;
  - `as_task: "yes"` is refused; an event `chat_message.posted` with `as_task: true` reads, and
    one with `as_task: false` is refused;
  - `task.created` with `given_to: "dev"` reads.

- [x] `feat(protocol): a chat message may be sent as the agent's task`

### Task 2: filing the task

Files: `crates/store/src/requests.rs`. Tests in its `mod tests`, on a temporary project.

- `files_a_given_task_triaged_small`. With a human `chat_message.posted` in `dev`'s chat with
  `as_task: true` at seq `n`, `file_given_task` of a request for `software_developer` given to
  `dev` from `n` files `CTV-1`:
  - its `task.created` has `created_by: "human"`, `from_chat_message: n` and `given_to: "dev"`;
  - the next event is `request.triaged { size: small, reason: "a task the owner gave dev in their
    chat", triaged_by: "catervas" }`;
  - `given_to(log, CTV-1)` is `Some("dev")`;
  - filing again from `n` is refused "was already sent, as CTV-1";
  - filing from `n` with another intent is refused "its intent is not the owner's message n";
  - filing from `dev`'s own reply is refused "links to message <seq>, which is not the owner's
    message in dev's chat".

- [ ] `feat(store): file the task the owner gave an agent in its chat`

### Task 3: the command, and the chat that does not answer it

Files: `crates/runtime/src/chat.rs`, `crates/runtime/src/orchestrator/human.rs`. The test is in
`human.rs`'s `mod tests` (ignored, as its neighbours are).

- `gives_an_agent_a_task_from_its_chat`, on `Harness::with_the_suggested_six`. `ChatMessagePost {
  agent_id: "dev", text: "Add a dark mode switch to the settings page", as_task: true }`:
  - It reports "given to <dev's display name> as CTV-1", and the log holds the three events of
    Decisions, in order.
  - `CTV-1`'s contract has `assignee_role` `software_developer`, `reviewer_role` `architect` and
    the text as its intent.
  - `chat_waiting` for `dev` is `None`.
  - One tick moves `CTV-1` to `refining` and starts no session: no chat session answers the
    message.
  - The same command refuses, each recording nothing:
    - for `dev` paused, with `agent_not_active: `;
    - with the text "too short", with `task_too_short: `.

- [ ] `feat(runtime): the owner gives an agent a task by messaging it`

### Task 4: the given agent writes its contract, and only it

Files: `crates/runtime/src/tools/fixtures.rs` (`filed_given_to`: as `filed_with` with kind `task`
and no parent, its `task.created` body also `given_to`), `crates/runtime/src/tools/contracts.rs`.

- `only_its_agent_writes_a_given_task` (the writer guarantee). On `a_team_of_three`, `CTV-1` is
  filed with `filed_given_to("CTV-1", "refining", "dev-a", …)`, `assignee_role`
  `software_developer`:
  - `dev-a`'s `catervas_write_contract` of `scope` from a refine session records
    `contract.written`;
  - `dev-b`'s is refused as `NotAContractWriter`;
  - `dev-a` setting `assignee_role` to `product_manager` is refused with
    `given_task_role_fixed: CTV-1 was given to dev-a in its chat, so its assignee_role stays
    software_developer`;
  - `pm` setting it to `architect` is refused the same way;
  - `dev-a` setting `reviewer_role` to `product_manager` is refused with `given_task_reviewer_fixed:
    CTV-1 was given to dev-a in its chat, so its reviewer_role stays software_developer`;
  - `dev-a` changing `intent` is refused with `given_task_intent_fixed: CTV-1 was given to dev-a
    in its chat, so its intent stays the owner's words`;
  - once `CTV-1` is `ready`, `dev-a`'s write of `scope` is refused as `NotAContractWriter`: its
    standing ended with `refining`.

- [ ] `feat(runtime): an agent writes the contract of the task it was given`

### Task 5: rule 9 gives the refine session to that agent

Files: `crates/runtime/src/orchestrator/messages.rs`, `crates/runtime/src/orchestrator/requests.rs`,
`crates/runtime/src/orchestrator/fixtures.rs` (`file_given`: as `file_under` with no parent, through
`filed_given_to`). The test is ignored, as its neighbours are.

- `the_given_agent_refines_its_own_task`, on `with_the_suggested_six`:
  - With `CTV-1` given to `dev` and `refining` (`file_given`), the next tick starts one session,
    `who` `[("dev", SessionPurpose::Refine)]`, with the transcript `implement_stops_early`.
  - Its `initial_prompt` starts with `GIVEN_TASK_LEAD` filled with dev's display name and
    `software_developer`.
  - With `dev` paused, a fresh harness's tick starts no session.

- [ ] `feat(runtime): the agent given a task refines it, not the Product Manager`

### Task 6: rule 8 assigns it to that agent first

Files: `crates/runtime/src/orchestrator/rules.rs`. The test is ignored, as its neighbours are.

- `assigns_a_given_task_to_its_agent_first_without_a_session`. The team is
  `with_the_suggested_six` with `policy.wip_limit_per_agent` 1. `CTV-1` is filed `ready` for
  `software_developer` with `file`, and `CTV-2` `ready` and given to `dev` with `file_given`,
  `reviewer_role` `architect`. The tick:
  - reports "assigned it to dev, who was given it in its chat" about `CTV-2`, and leaves it
    `assigned`, assignee `dev`, reviewer `ada`;
  - starts no session;
  - leaves `CTV-1` `ready`.

- [ ] `feat(runtime): a task given in a chat is assigned to its agent first`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Tasks 3, 5 and 6's ignored tests among them)
```
