# Phase 8, step 06: A new project's interview and product plan

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 4.1, 4.3, 5.3, 5.16, 6.1, 8.5; F5, F8
Depends on: steps 01 and 01b of this phase (planned, not yet executed: `catervas_core::folders`, the Product Manager's docs tasks on `docs/CTV-<n>`, `REVIEWER_ROLE_FOR`'s Product Manager row, `no_task_no_write`); step 03b (planned, not yet executed: `result_needs_the_human`, which makes a task whose `allowed_paths` may change `spec.md` or `roadmap.md` wait for the owner's acceptance); step 05 (ready, not yet executed: `CHECKS` 24, `EVERY_RULE` 26); phase 7 and the Catervas rename, and task ids `CTV-<n>` (merged on main). Step 06b, next, builds the pages on what this step answers; step 07 consumes `product_plan_approved` and `Transitions::awaits_the_product_plan`.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (two rounds, ADR 0032): ready. Round 1 found one Blocking (the send-back notes cut after a long interview), decided as the reviewer proposed: the notes go last and stay whole, the interview gets a byte budget (round 2's Should, folded); the Should items and nits carried into execution
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19): `docs/design/mockups/ProductInterview.dc.html` (built in 06b on this step's `heard`, command and query) and `ProductPlanApproval.dc.html` (06b).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at dd4f1f6f; steps 01 to 05 move some, and the names are what count.

## Goal

A project the first run made from a paragraph is recorded as new (`project.started`). Once setup ends, its Product Manager opens an interview in their one-to-one chat and asks, one question at a time, who the customers are, the problem, what the product must do, what is out, the constraints and how success is measured, then sums up in a structured "What I heard"; the chat stays read-only (4.3). The owner's word, `product_plan_draft`, files the product plan task: the Product Manager writes `spec.md`, `roadmap.md` and their twins from the interview, its reviewer checks it, and the owner accepts it (03b). Until both documents are on the integration branch, readiness refuses every other task of the project (`product_plan_first`), and the orchestrator holds their contracts in `refining` without spending a session. Out of scope: every screen (06b); filing the Catervafication epic on approval (07); any change to an existing repository's first run.

## Decisions

- **Split.** Row 06 becomes 06 (this: the event, the rule, the interview, the task, the command and the query) and 06b (the chat's "What I heard" with its button, Today's product plan card and the team band's lines). Rejected: one plan of about 450 lines.
- **`project.started { name, description }`**, recorded by `CliHost::create` (`crates/cli/src/setup.rs:162`) after `taken_on` and before the first request is filed (`:217`), with the project's ids, about no task: `name` the folder's name (`^[a-z0-9-]{1,64}$`, create's own rule at `:169-176`), `description` the paragraph, 20 to 2,000 characters (`:178-188`), recorded trimmed. `project.scanned` (`crates/cli/src/init.rs:99`) stays recorded by the scan for every project, a new one included; a project is new when its log holds `project.started`. Not wrapped in anything: the paragraph is the owner's own words (ADR 0011). TypeScript needs no hand mapping: the types are generated from the schema and `toCamel` (`packages/protocol-client/src/mapping.ts`) maps the keys. Rejected: a flag in the team file, which a template carries to other projects.
- **The paragraph is still filed as the first request** (4.1, unchanged): it is triaged, then held in `refining` until the plan is approved, and then refined against the spec, giving step 07 the first feature epic to plan. Rejected: not filing it, which leaves the owner's first request nowhere once the roadmap is approved.
- **Approved is read from git, nothing stored** (ADR 0051): `product_plan_approved`, pure in `catervas_core::folders`, is true when both paths of `PRODUCT_PLAN` (`docs/catervas/product/spec.md`, `roadmap.md`), compared after `normalise` and with letter case, are among the paths given. The paths come from `product_plan_on` (`catervas_store::git`, beside `integration_branch`, `git.rs:733`), which asks the new `Git::has_file` of each on the team's integration branch (`git ls-tree --name-only <rev> -- <path>`: present when it names the path, absent when empty, an error for a revision git does not have). `product_plan_on` answers false when the integration branch is not in the repository (`rev-parse --verify` fails); any other git error is the caller's error. The integration branch, not the default branch, since a folder change and a task are integrated there (5.14). Under `pull_request` and `manual` the plan is approved when its branch is merged. Rejected: `OWNER_ACCEPTED` (step 03), which may grow past the product plan.
- **`Transitions::awaits_the_product_plan(team)`**: false with no `project.started` (git not asked); otherwise `!product_plan_on(..)`. Every caller asks this one method: readiness, rule 9, the chat session's prompt, the opening, the command and the query.
- **Which task is the product plan's**: one whose `task.created` has `product_plan: true` (optional, `const: true`), which only `file_product_plan_request` writes; `catervas_store::product_plan::is_the_product_plan(history)` reads it from a task's events. Rejected: reading it from the contract's paths, which any Product Manager task could copy.
- **`product_plan_first`.** `ReadinessContext` (`crates/core/src/governor/readiness.rs:120`) gains `pub awaits_the_product_plan: bool` and `pub is_the_product_plan: bool`, filled by `readiness_parts` (`crates/runtime/src/transitions.rs:659`, the literal at `:680`). `ReadinessRule::ProductPlanFirst`, last in `CHECKS` (25 after step 05's 24; `EVERY_RULE`, `plain.rs:73`, 27), fails a task or an epic when the first is true and the second false. Message, exact: `this new project's product plan is not approved yet: only its own task is planned until docs/catervas/product/spec.md and docs/catervas/product/roadmap.md are on the integration branch`. Plain words: "A new project plans nothing until the owner approves its product plan." Wire name `product_plan_first` by `rule_name`.
- **Held, not refined.** Rule 9 (`refining`, `crates/runtime/src/orchestrator/requests.rs:134`) passes over a contract while `awaits_the_product_plan` and it is not the product plan's: no refine or judgment session and no governor ask, so no readiness attempt is spent and nothing is escalated after three (5.2). Triage (rule 10) still runs. Once approved, the next tick refines it against the spec.
- **The product plan task is judged as filed**: `is_to_be_judged` (`requests.rs:274`) also holds, with no write since refining began and no refusal since, for a contract whose `task.created` has `product_plan`, so no refine session rewrites Catervas's contract; a refused one goes to the Product Manager as any contract does.
- **It skips the sprint queue**, as a raise does (spec 3's exceptions): the projection's `task.created` arm (`crates/store/src/projections.rs:636-645`) sets `skips_sprints` for `product_plan` as for `raises`. Rejected: waiting for a sprint the owner has no reason to start yet.
- **The contract**, `product_plan_request(reviewer, max_cost_usd)` in `catervas_store::requests`, filed by `file_product_plan_request`, which `file` (`requests.rs:259`) files with `Filing.product_plan` and `creation_bodies` (`:356`) follows with `request.triaged { small }` by `catervas`, as for `raises`. `created_by` is `human`: the owner pressed the button. Fields, exactly: title `Draft the product plan`; intent `Write this new project's product plan from the owner's interview in your one-to-one chat, which From the human holds: docs/catervas/product/spec.md, the product description, and docs/catervas/product/roadmap.md, the roadmap, each for the owner to read, with its .agent.md version for the team. Nothing else is planned until the owner approves it.`; summary `The Product Manager writes the product description and the roadmap from your interview, for you to read and approve. Nothing else is planned until you approve them.`; scope in `[docs/catervas/product/spec.md and roadmap.md, each with its .agent.md version]`, out `[Code, and every other document]`; R1 `spec.md has the product's name as its title, a paragraph on what it is and the problem it solves, and the sections Who it is for, What it must do, Not now and How we'll know it works, with Limits when the owner named any, in the owner's words where they gave them.`; R2 `roadmap.md has the sections Now, Next and Later, each item one line: its name, then one sentence.`; R3 `spec.agent.md and roadmap.agent.md say what spec.md and roadmap.md say, written for agents.`; C1 `review`, satisfies R1 and R2, rubric `spec.md has the title, the paragraph and the sections R1 names.`, `roadmap.md has Now, Next and Later, each item one line.`, `Both are in plain words, with no technical term the owner did not use.`; C2 `review`, satisfies R3, rubric `Every fact, decision and limit in spec.md and roadmap.md is in their .agent.md versions.`, `Nothing in an .agent.md version contradicts its version for the owner.`; assignee role `product_manager`; reviewer role as below; risk `low`; budget `placeholder_budget_usd` (`requests.rs:80`); `allowed_paths` the four files. Its acceptance is the owner's by 03b's `result_needs_the_human`; nothing here adds to it. Under `all` its contract comes to the owner as every contract does; nothing exempts it. Residual: a team whose rules require a `test` or `command` criterion refuses it at readiness, as it refuses every docs task (5.3).
- **The command**: `product_plan_draft {}` (`emptyBody`, `docs/schemas/command.schema.json:520`; `Command::ProductPlanDraft`), on `POST /command` or the browser's `command`, handled in `human.rs` (`handle`, beside `ChatMessagePost` at `:156`) by `draft_product_plan` under a lock of its own (as `REPLYING`, `crates/runtime/src/tools/chat.rs:34`), so two presses file one. Refusals, in order, each `<code>: <words>` as `CommandError::Refused`: `not_a_new_project: this project was not started in Catervas, so it has no product plan to draft`; `product_plan_approved: the product plan is approved already`; `product_plan_open: <id> drafts the product plan already` (a product plan task that is not `cancelled`, and not `accepted` with its integration done, `awaiting_integration` false); `product_plan_no_writer: the team has no active Product Manager to write it` (`product_manager`, `requests.rs:46`); `product_plan_no_reviewer: an Architect or a Scrum Master checks the product plan before you read it, and the team has no active one; add one on the Team page` (`default_reviewer_role(team, Task, ProductManager)`, `crates/roles/src/reviewer.rs:34`, is `None`). Filed, it answers `said` `<PM name> drafts the product plan as <id>. It comes to you on Today to approve.` and the events' seqs.
- **The interview's prompt.** In `session_spec_without` (`crates/runtime/src/orchestrator/session.rs:971`) a `chat` session of a Product Manager about no task, while `awaits_the_product_plan`, gets `INTERVIEW_INSTRUCTION` (`prompt.rs`) as `closing` in place of `closing_instruction(ask)` (`:943`), and `From the human` from `chat::interview` in place of `chat_history` (`:1026-1027`). It stays read-only on `CHAT_TOOLS` (`rules.rs:719`), its one write its reply (4.3; 01b). `INTERVIEW_INSTRUCTION`, exactly: "This session is your interview of the owner about their new project, in your one-to-one chat. `From the human` holds what they wrote when they started it, then the chat so far. Nothing is planned until the owner approves the product plan you write from it. Find out, one question at a time, who the customers are, the problem they have, what the product must do, what is out for now, any constraints, and how the owner will know it works; ask only what the chat has not answered, in your persona's voice, short and in plain words. When you know enough, or the owner asks you to, sum up: put the summary in `heard` (constraints under `must` or `not_now`, as they fit) and say in your text that they can correct it or have you draft the plan. Put no request in your reply: nothing is planned before the plan. Answer once with `catervas_chat_reply`, then end the session."
- **`chat::interview(log, agent_id, budget)`**: "When they started the project, the owner wrote:\n<description>\n\nYour interview so far:\n" and the agent's chat as `chat_history` writes it, the owner's lines as their own and the agent's wrapped `untrusted`; the chat's older messages dropped first so the whole fits `budget`; the chat session passes `HISTORY_BYTES` (`chat.rs:349`); nothing is cut with `prompt.rs`'s `cut`; the description and the newest message are kept even past the budget; with an empty chat, the first part alone. Rejected: quoting the chat into the contract, which the prompt wraps whole as untrusted (ADR 0011), demoting the owner's words, and which a later answer would not reach.
- **The task's session reads the interview the same way.** The product plan task's `implement` session's `From the human` is `chat::interview(log, its agent, budget)` with `budget` 16 KiB minus the length of `human_message` (`crates/runtime/src/orchestrator/messages.rs:227`) and a blank line, followed by `human_message`, so the owner's notes (a send-back's among them) are never cut. `ponytail:` the assignee's own chat; a team of two Product Managers where the other interviewed gives the paragraph alone; name the chat in the task if that ever happens.
- **The opening.** A rule after the chat rule in `tick` (`rules.rs:168`), `open_the_interview`, and not in `chat_alone` (`:735`), so it waits while the team is paused, setup's pause included (4.1): under `All` in a tick scoped to no task, while `awaits_the_product_plan`, `product_manager(team)`, when its chat holds no message and it never started a `chat` session, awake and within the day's budget (`day_is_spent`, `asleep`), gets one `chat` session as the chat rule asks it (`:788-805`) with `in_reply_to: None` and the first message, exactly, "<name>, the owner has just started this project. Open your interview in your one-to-one chat: greet them and ask your first question." A session that ends without a reply is not retried (as a chat's, `chat.rs:180-185`); the owner can write first.
- **`heard`**, the structured "What I heard" the approved mockup draws: `{ customers, problem, must, not_now, works_when }`, each 1 to 1,000 code points and not blank, line breaks kept. `ChatReplyInput` (`tools/chat.rs:18`) gains `heard: Option<Heard>`, `NewChatMessage` (`chat.rs:41`) `heard`, checked by `post_chat` (`:112`) with `within` (`:91`) as `heard` (`blank_heard`, `heard_out_of_bounds`), and `chat_message.posted` gains optional `heard` (`$defs/heard`, all five required, no other). Any agent may send one; only the interview's prompt asks for it, and 06b's button follows the project's state, not it. `chat_history` (`:358`) writes it inside the agent's untrusted block after the text as "What I heard:" and the five, each "<label>: <text>" with the labels "Your customers", "The problem", "It must", "Not now", "It works when". `chat.messages` (`crates/runtime/src/daemon/board.rs:330-344`) answers `heard`, null when none. The tool's description gains: "During a new project's interview, put your summary in heard." Rejected: the summary in the text alone, which the chat shows as plain text, not as drawn.
- **The query**, `product_plan.state {}` in `gates.rs`'s `query` (`crates/runtime/src/daemon/gates.rs:232`): `{ new_project: { name } | null, approved, task: { task_id, status, created_at } | null, documents: [{ path, text }] }`. `approved` is false with no new project. `task` is the newest product plan task that is open as the command reads it. `documents` holds `spec.md` then `roadmap.md` read from the task's branch (`task_branch`, `crates/core/src/branch.rs:14`, via `Git::file_at`, `git.rs:243`), each left out when git has none there, while the task is `verifying`, and is empty otherwise. Rejected: the documents on every row of `waiting.list`.
- **No new screen and no new tool**; the command line gains nothing (the wizard alone makes a new project).

## File map

```
crates/core/src/folders.rs, governor/readiness.rs, governor/readiness/fixtures.rs, governor/plain.rs   modifies (Task 1)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,event/fixtures.rs,lib.rs}   modifies: project.started, task.created's product_plan (Task 2); heard (Task 4)
crates/store/src/product_plan.rs, crates/store/src/lib.rs     creates, modifies: new_project, is_the_product_plan (Task 2)
crates/store/src/projections.rs                              modifies: skips_sprints (Task 2)
crates/cli/src/setup.rs, crates/cli/tests/serving.rs         modifies, tests (Task 2)
crates/store/src/git.rs, crates/store/tests/git.rs           modifies, tests: has_file, product_plan_on (Task 3)
crates/runtime/src/transitions.rs                            modifies: awaits_the_product_plan, readiness_parts (Task 3)
crates/runtime/src/orchestrator/requests.rs                  modifies: the hold (Task 3); is_to_be_judged (Task 6)
crates/runtime/src/chat.rs, tools/chat.rs, tools.rs, daemon/board.rs, docs/schemas/rpc.schema.json   modifies: heard (Task 4)
crates/runtime/src/chat.rs, prompt.rs, orchestrator/session.rs, orchestrator/rules.rs   modifies: the interview (Task 5)
crates/store/src/requests.rs                                 modifies: product_plan_request, file_product_plan_request (Task 6)
docs/schemas/command.schema.json, crates/protocol/src/command.rs, crates/runtime/src/orchestrator/human.rs   modifies: the command (Task 6)
crates/runtime/src/orchestrator/session.rs, daemon/gates.rs, docs/schemas/rpc.schema.json   modifies: the task's interview, the query (Task 6)
docs/SPEC.md, docs/design/catervas-folders.md, docs/plans/project-plan.md   modifies (Task 7)
```

## Interfaces

Consumes: `normalise` (`paths.rs:87`); `ReadinessContext`, `CHECKS`, `plain_readiness` (`catervas-core`); `task_branch`; `Git`, `file_at`, `integration_branch`; `file`, `Filing`, `creation_bodies`, `placeholder_budget_usd` (`catervas_store::requests`); `chat_history`, `post_chat`, `within`, `HISTORY_BYTES`; `session_spec_without`, `closing_instruction`, `run_session`, `CHAT_TOOLS`, `day_is_spent`, `asleep`, `ran`; `human_message`; `product_manager`, `default_reviewer_role`; `CommandReport`, `CommandError`; all on main. From steps not yet executed: 01b's `REVIEWER_ROLE_FOR` row and the Product Manager's `write_workspace` and `git_local`; 03b's `result_needs_the_human`; 05's `CHECKS` 24 and `EVERY_RULE` 26.

Produces:

```rust
// catervas_core::folders
pub const PRODUCT_PLAN: [&str; 2];   // docs/catervas/product/spec.md, docs/catervas/product/roadmap.md
pub fn product_plan_approved(on_integration_branch: &[String]) -> bool;
// catervas_core::governor::readiness: ReadinessRule::ProductPlanFirst;
// ReadinessContext gains `pub awaits_the_product_plan: bool` and `pub is_the_product_plan: bool`
// catervas_store::product_plan
pub struct NewProject { pub name: String, pub description: String }
pub fn new_project(log: &EventLog) -> Result<Option<NewProject>, StoreError>;
pub fn is_the_product_plan(history: &[CatervasEvent]) -> bool;
// catervas_store::git
impl Git { pub fn has_file(&self, rev: &str, path: &str) -> Result<bool, GitError>; }
pub fn product_plan_on(git: &Git, team: &Team) -> Result<bool, GitError>;
// catervas_store::requests
pub fn product_plan_request(reviewer: Role, max_cost_usd: f64) -> Value;
pub fn file_product_plan_request(files: &ProjectFiles, log: &EventLog, wire: Value,
    (now, ids): (DateTime<Utc>, &EventIds)) -> Result<TaskContract, RequestError>;
// catervas_runtime::transitions::Transitions
pub fn awaits_the_product_plan(&self, team: &Team) -> Result<bool, TransitionError>;
// catervas_runtime::chat
pub struct Heard { pub customers: String, pub problem: String, pub must: String, pub not_now: String, pub works_when: String }
pub fn interview(log: &EventLog, agent_id: &str, budget: usize) -> Result<Option<String>, StoreError>;
// NewChatMessage and ChatReplyInput gain `heard: Option<Heard>`
// catervas_runtime::prompt
pub const INTERVIEW_INSTRUCTION: &str;
// EventBody::ProjectStarted; TaskCreatedBody gains `product_plan: Option<bool>`; ChatMessagePostedBody gains `heard`
// Command::ProductPlanDraft; query product_plan.state
```

## Tasks

### Task 1: The plan's approval and `product_plan_first`

Files: `folders.rs` (`PRODUCT_PLAN`, `product_plan_approved`, tests); `readiness.rs` (the two fields, the rule, the check, tests); `readiness/fixtures.rs:25` and the `ReadinessContext` literals at `readiness.rs:1376` and `:1797` (both new fields `false`); `plain.rs` (the sentence, `EVERY_RULE`, `listed`).

- `the_plan_is_approved_when_both_documents_are_there` (`folders.rs`) — true for spec and roadmap in either order, with other paths among them, and with `./docs/catervas/product/spec.md`; false for `[]`, either one alone, the two `.agent.md` twins, and `Docs/Catervas/Product/spec.md` with the roadmap. RED: no function.
- `refuses_everything_but_the_product_plan_until_it_is_approved` (`readiness.rs`) — `a_ready_context()` with `awaits_the_product_plan: true`: `a_contract()` fails `[ProductPlanFirst]` alone with exactly the Decisions' message, and an epic of it too; with `is_the_product_plan: true` it passes; with `awaits_the_product_plan: false` it passes either way. RED: no rule.
- `the_product_plan_sentence_is_plain` (`plain.rs`) — exactly "A new project plans nothing until the owner approves its product plan.". RED.

- [ ] `feat(core): plan nothing in a new project before its product plan`

### Task 2: A new project is recorded, and its plan's task marked

Files: `event.schema.json` (`project.started` in the kinds, `projectStartedBody`; `taskCreatedBody.product_plan`, `{ "const": true }`, and its description); `event.rs` (the variant, `EVERY_KIND` one longer, not about one contract), `lib.rs` (`KINDS`), `event/fixtures.rs` (`a_body_wire`, `:54`); created `crates/store/src/product_plan.rs`, `pub mod product_plan;`; `projections.rs` (the arm; a test); `setup.rs` (`create`); `serving.rs`.

- `names_every_event_kind_as_an_entity_and_a_past_tense_verb` and `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` (`event.rs`) hold with it. RED: unknown kind.
- `project_started_holds_its_bounds` (`event.rs`) — `name: "Bakery"`, a 65-character name, and a 19-character `description` are refused; `task.created` with `product_plan: false` is refused. RED.
- `reads_the_new_project_and_its_plans_task` (`product_plan.rs`) — a log with none: `new_project` is `None`; with `project.started { name: "little-oak-cakes", description }`: both fields; `is_the_product_plan` is true for a history holding a `task.created` with `product_plan` and false for one without. RED: no module.
- `a_product_plan_task_skips_the_sprints` (`projections.rs`) — its row's `skips_sprints` is true; a plain request's stays false. RED.
- `creates_a_project_paused_with_its_first_request` (`serving.rs:1232`, updated) — the log holds exactly one `project.started { name: "bakery", description }`, and CTV-1 is still filed with the description as its intent. RED: no event. `takes_on_the_chosen_project_on_the_same_port` (`:1292`) asserts its log holds no `project.started`.

- [ ] `feat(protocol): record a project started from a paragraph`

### Task 3: Readiness asks git, and rule 9 holds the rest

Files: `git.rs` (`has_file`, `product_plan_on`), `tests/git.rs`; `transitions.rs` (`awaits_the_product_plan`; `readiness_parts` fills both fields, `is_the_product_plan` from the task's history; `readiness_context`'s `# Errors` now names git for a new project; tests); `orchestrator/requests.rs` (the hold in `refining`; a test).
Consumes: Tasks 1 and 2.

- `tells_whether_a_file_is_on_a_branch` (`tests/git.rs`) — after committing `docs/catervas/product/spec.md` on `main`: true for it, false for `roadmap.md` and for a file only in the working tree, and `Err` for branch `nope`. RED: no method.
- `reads_the_product_plan_from_the_integration_branch` (`tests/git.rs`) — spec alone on `main`: false; both: true; both on `main` with the team's `integration_branch: develop`, where neither is: false; `integration_branch: develop` with no such branch: false, not an error. RED.
- `refuses_another_task_of_a_new_project_until_the_plan_is_on_main` (`transitions.rs`, integration) — with `project.started` in the log, a written contract's governor `ready` is refused with `product_plan_first` among its failures; a contract whose `task.created` has `product_plan` is not refused for it; once both documents are committed on `main`, the first moves to `ready`. Without `project.started` nothing is refused for it, whatever git holds. RED: no field filled.
- `holds_a_new_projects_contracts_until_the_plan` (`requests.rs`, integration) — a refining raw request in a project with `project.started`: the tick starts no session and records no `contract.evaluated` or `transition.refused` for it; with both documents on `main`, the Product Manager's refine session runs. RED: the refine session runs.

- [ ] `feat(runtime): hold a new project's work until its product plan is approved`

### Task 4: What the Product Manager heard

Files: `event.schema.json` (`heard`, its `$defs` and the body's description), the generated types; `chat.rs` (`Heard`, `NewChatMessage.heard`, `post_chat`'s check, `chat_history`); `tools/chat.rs` (`ChatReplyInput.heard`, passed on); `tools.rs:310-314` (the description); `board.rs` (`chat_messages`); `rpc.schema.json` (`chatMessagesResult`'s message gains `heard`, `$ref` or null); every other `NewChatMessage` literal gains `heard: None`.

- `records_a_reply_with_what_it_heard` (`tools/chat.rs`, integration) — a chat session's reply with `heard` records `chat_message.posted` holding the five fields; a blank `problem` is refused starting `chat_reply_refused: blank_heard`, a 1,001-character `must` `chat_reply_refused: heard_out_of_bounds`, neither records anything, and the session's reply after them is recorded. RED: unknown field.
- `lists_what_a_reply_heard` (`board.rs`, integration) — `chat.messages` answers that reply's `heard` and `null` for the owner's message, held to `chatMessagesResult`. RED.
- `shows_what_the_agent_heard_as_its_words` (`chat.rs`) — `chat_history` of that reply holds "What I heard:" and "Your customers: <text>" inside the agent's `untrusted` block. RED.

- [ ] `feat(runtime): let a chat reply say what it heard`

### Task 5: The interview

Files: `chat.rs` (`interview`, sharing `chat_history`'s reading with a byte budget); `prompt.rs` (`INTERVIEW_INSTRUCTION`); `orchestrator/session.rs` (`session_spec_without`; a test); `orchestrator/rules.rs` (`open_the_interview`, its call after `chat` in `tick`; tests).
Consumes: Tasks 2 to 4.

- `keeps_the_paragraph_and_the_newest_of_the_interview` (`chat.rs`) — a 2,000-character ASCII description and 20 KiB of ASCII chat: the text starts with "When they started the project, the owner wrote:\n" and the whole description, holds the newest message, leaves out the oldest, and is at most `HISTORY_BYTES`; with no chat, the first part alone. RED: no function.
- `interviews_the_owner_in_the_product_managers_chat` (`session.rs`, integration) — with `project.started`: the Product Manager's chat session's prompt ends with `INTERVIEW_INSTRUCTION` and its `From the human` starts with the description, the owner's lines outside every untrusted block; the Developer's chat in the same project keeps the chat's closing and history; with both documents on `main`, the Product Manager's chat is the plain chat again; the session is offered no tool outside `CHAT_TOOLS`. RED.
- `opens_the_interview_once_the_team_runs` (`rules.rs`, integration) — with `project.started`, the team resumed and the Product Manager's chat empty: one tick runs its `chat` session with no `in_reply_to` and exactly the opening message; the next tick runs none, whether it replied or not; none runs while the team is paused (`tick_within`), without `project.started`, or with the plan on `main`. RED: no rule.

- [ ] `feat(runtime): interview the owner of a new project in the Product Manager's chat`

### Task 6: The owner's word drafts the plan

Files: `store/requests.rs` (`Filing.product_plan`, `product_plan_request`, `file_product_plan_request`, tests); `command.schema.json` (`product_plan_draft` with `emptyBody`), `command.rs` (the variant, both arms, a test); `human.rs` (`draft_product_plan`, tests); `orchestrator/requests.rs` (`is_to_be_judged`; a test); `orchestrator/session.rs` (the implement session's `From the human`; a test); `gates.rs` (the query; a test); `rpc.schema.json` (`product_plan.state` in the query names, `productPlanStateQuery` `{}`, `productPlanStateResult` as Decisions, every field required, `status` the task statuses).
Consumes: Tasks 1 to 5.

- `writes_the_product_plans_contract` (`store/requests.rs`) — `product_plan_request(Architect, 20.0)` passes the contract schema and readiness in a context with an active Architect, with the Decisions' title, the four `allowed_paths`, the roles and two `review` criteria; filed, `task.created` has `created_by: human` and `product_plan: true`, followed by `request.triaged { small }` by `catervas`. RED: no function.
- `reads_product_plan_draft` (`command.rs`) — `{ command: "product_plan_draft", body: {} }` reads and writes back; a body with a field is refused. RED.
- `drafts_the_product_plan_on_the_owners_word` (`human.rs`, integration) — a new project with Mira and Ada: the command files CTV-<n> as above and says "Mira drafts the product plan as CTV-<n>. It comes to you on Today to approve."; then rule 10 and rule 9 move it to `ready` with no refine session, and with sprints on and none open it is not held: the Scrum Master's plan session takes it to `assigned`. RED: unknown command.
- `refuses_a_draft_out_of_place` (`human.rs`, integration) — each refused with its code and nothing filed: no `project.started`; both documents on `main`; an open product plan task (and, once it is cancelled, the next draft files); no active Product Manager; no active Architect or Scrum Master. Two drafts at once file one. RED.
- `gives_the_plans_task_the_interview` (`session.rs`, integration) — with a 20 KiB interview, the product plan task's `implement` session's `From the human` starts with the interview, cut, and ends with the owner's whole send-back message; another task's holds no interview. RED.
- `answers_where_the_product_plan_stands` (`gates.rs`, integration) — an existing project: `{ new_project: null, approved: false, task: null, documents: [] }`; a new one: its `name`, `approved: false`, `task: null`; after the draft, its task; `verifying` with both files on its branch: their texts, spec first; both on `main`: `approved: true`; each answer held to `productPlanStateResult`. RED: unknown query.

- [ ] `feat(runtime): draft the product plan on the owner's word`

### Task 7: Spec

`docs/SPEC.md`: 3 (the sprint exceptions gain the product plan task); 4.1 (a new project records `project.started`; its first request waits; once setup ends the Product Manager opens its interview, in place of the first reading in the channel); 4.3 (the interview: its instruction, `heard`, one session per message, still read-only; `product_plan_draft`); 5.3 (`product_plan_first`, its message and plain words, approval read from the integration branch, rule 9's hold); 5.16 (the product plan task, which Catervas triages: its contract, judged as filed, the interview as the human's words); 6.1 (the interview and the product plan task); 8.5 (`project.started`, `task.created`'s `product_plan`, `chat_message.posted`'s `heard`, and `session.started`'s description: a chat session with no `in_reply_to` opens the interview); F5 (`product_plan_first` in the governor) and F8 (`heard` in `chat.messages`, the interview, `product_plan.state`); the next spec revision after the branch's last. `docs/design/catervas-folders.md`: "A new project" says approved on the integration branch, the paragraph still filed and held, and the opening; the steps table splits row 06 into 06 and 06b. `docs/plans/project-plan.md`: row 06 becomes this step's Delivers, Spec `3, 4.1, 4.3, 5.3, 5.16, 6.1, 8.5`, and row 06b, "The interview and the plan in the web app", Spec `4.1, 4.2, 4.3`.

- [ ] `docs(spec): record a new project's interview and product plan`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Tasks 2 to 6 name integration tests
# expected: xtask check: ok
```
