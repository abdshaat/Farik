# Phase 3, step 05: Farik tools

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.1, 5.2, 5.6, 5.7, 5.9, 5.11, 5.12, 5.13, 5.16; ADR 0004
Depends on: step 04 of this phase (`Transitions`); step 02 (`Executor`); step 09 (`default_reviewer_role`). Pull request #12's id fix is not a start gate (the founder, 2026-09-22: merge conflicts are resolved once the whole phase is implemented): this step moves the filing code as it is on this branch, and when `main` is merged in at the end of the phase, #12's fix to `farik task create` is applied to `file_request` as part of resolving that merge, with its tests.
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the six decisions the first found open; findings folded in)

## Goal

An agent does its work through Farik's own tools, each checked against the agent's tier and against the governance rule that owns it, and each leaving an event: triage a request, read a task, the board, the rules, the criteria, write a contract, ask for a transition, assign, record a criterion result, write a note, file a task or a child task, declare a block, ask the human, write a product document, run a command, and use git. Out of scope: serving the tools over MCP and deciding which session is calling (the daemon, step 07), ending the session after `farik_ask_human` (step 14), creating the sandbox a command runs in (step 11).

## Decisions

- The tools are plain Rust: `tool_descriptors()` lists every tool with its tier and its input's JSON Schema, and `call_tool(context, name, input)` runs one. The `rmcp` server that exposes them is step 07's, so this step has no MCP dependency and every tool is tested by calling it. Input schemas come from `schemars` =1.2.2 derives on one input struct per tool (the major `rmcp` 3.3.0 uses), and an input is read with `serde_json::from_value` into that struct; a value that does not fit is `ToolError::InvalidInput` with serde's words.
- The nineteen tools: `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_criteria`; `farik_triage_request`, `farik_write_contract`, `farik_create_task`; `farik_request_transition`, `farik_assign_task`, `farik_declare_blocked`, `farik_record_criterion_result`, `farik_write_note`, `farik_ask_human`, `farik_write_product_doc`; `farik_exec`; `farik_git_status`, `farik_git_diff`, `farik_git_commit`, `farik_git_push`.
- Tiers: every coordination tool is `Read`, which every role holds, because what governs them is the rule each names (chose this over inventing a tier per tool, which 5.6's seven tiers do not have; `docs/SPEC.md` 5.6 gains the sentence that says so); `farik_exec` is `Execute`; git is four tools rather than one `farik_git` with an operation, so that each carries one tier and is checked as its own tool (ADR 0004), with ADR 0004's tiers: status, diff, and commit `GitLocal`, push `GitRemote`. `docs/SPEC.md` 8.2 and 8.3 name the four instead of `farik_git`. `call_tool` checks the tier again with `evaluate_tool_call`, because the MCP endpoint is reachable by anything holding the daemon's token, not only through the hook. Its `ToolCallRequest.paths` is empty for every tool but two: `farik_write_product_doc` passes `.farik/product/<path>` and `farik_git_commit` its `paths`; `input_hash` is empty (no Farik tool is `ExternalEffect`); `ToolCallContext` carries the contract's `allowed_paths` and the team's protected paths. A caller whose agent is not `active` in the team is refused before anything else (`agent_not_active`).
- `ToolContext` is the calling session: agent id, the task (if any), the session id, the task's executor when it has one, and shared `ToolDeps` (log, projections, files, `Transitions`, git, clock, event ids). Role and tiers are not in it: the team is read from `.farik/team.yaml` on every call and the agent's role and `Agent::tiers()` taken from it, so a pause, a grant, or a revoke changes the next call, not the next session. Every tool that acts on a task acts on the session's task, except `farik_read_task` and `farik_assign_task`, which name one; a session with no task calling one that needs it is refused (`no_task`).
- Refusal words: `ToolError::Refused { reason }` starts with the refusal's kind in snake_case, then `: `, then its details, so that a test and an agent can both read what kind it was: `tier_not_granted: execute`, `contract_locked: ...`, `command_forbidden: ...`, `git_via_exec: use farik_git_status, farik_git_diff, farik_git_commit, or farik_git_push`, `gate_failed: <each detail>`, `not_the_named_agent: ...`, and so on, one variant each, written in `crates/runtime/src/tools/refusal.rs` with no fallback arm, as the command line's refusal module is.
- Which actor an agent is, for a transition: among the table's rows for the contract's status and the target, the first whose actor the caller is — `Assignee` when the board names it assignee, `Reviewer` when reviewer, `ProductManager` or `ScrumMaster` by role. None: `ToolError::Refused` (`actor_not_allowed`) naming the rows' actors, with no `transition.refused`, because nothing was asked of the governor.
- `farik_triage_request { size, reason }` on the session's task: the Scrum Master on an untriaged `draft` without a parent; the Product Manager on the same when the team has no active Scrum Master; the Product Manager of a `refining` standalone task (no parent, kind `task`) for `large` only, which turns it into an epic; anything else refused, including a `draft` already triaged, because the human's triage wins when it comes first and an agent does not overrule it. It sets `kind` in the file and emits `request.triaged` with `triaged_by` the agent. Re-triaging a refining task starts its refining over in the one sense the log can hold: step 04's readiness-attempt count starts from the later of the last move into `refining` and the last `request.triaged` (step 04's plan is amended to say so). A blank reason is refused, as `farik triage` refuses one.
- `farik_write_contract { fields, criteria }`: `fields` is a JSON object merged shallowly onto the current contract (each top-level key replaced); `criteria` is a list of `{ id, name }` expanded from the library by `expand_criteria` and appended to `exit_criteria`. The changed fields are the keys whose values differ, with `exit_criteria` among them whenever `criteria` is non-empty. `check_contract_write` judges them with the actor chosen by role first (`ProductManager`, `ScrumMaster`), then by relation (`Assignee`, `Reviewer`, from the board); anyone else is refused (`not_a_contract_writer`). An agent is never `Human`, so the outcome is `Allowed` or a refusal; `ReturnsToRefining` would be a bug and is `ToolError::Failed`. An epic's contract is refused while a `question.asked` for it has no `question.answered` (none exists until step 14, so every asked question is unanswered, which is the truth). The result is held to `validate_contract`, written, and recorded as `contract.written` with its summary and `written_by`.
- `farik_create_task { contract, parent? }`: without `parent`, a new `draft` request filed exactly as `farik task create` files one (the same refused fields, the same id rule); with `parent`, a child `task` of that epic after `check_child_creation` with `ParentEpic { status, assignee_id }` from the board (which has the assignee since step 04). A child is a `draft` whose triage is its epic's breakdown, so `file_request` with a parent appends `request.triaged { size: small, reason: "a task of <epic>", triaged_by }` (`triaged_by` the `created_by` given to `file_request`) after its `task.created`, which is what the `Triaged` gate and the board read; who then moves it to `refining` is the orchestrator's (step 14). To make "exactly as" one piece of code, the filing moves from `crates/cli/src/task.rs` into `farik_store::requests::file_request`, which both call. `RequestError::Refused` carries the command line's sentences without the file path ("sets id, which a request does not: ..." and "is not a contract Farik can file: ...", the reminders of `farik triage` and `farik contract lock` put as "triage" and "the human's lock"), and the command line puts the path in front, so its existing assertions hold.
- Reviewer role (D7, the step 09 landing review): when `farik_create_task` files, or `farik_write_contract` writes, a task whose `assignee_role` is given and whose `reviewer_role` is not (in the input, nor already in the contract), `reviewer_role` is filled with `farik_roles::default_reviewer_role(team, kind, assignee_role)`; when that is `None` it is left unset, and the schema's refusal names it, so the Product Manager learns the team has no reviewer and asks the human (D7). A `reviewer_role` the writer gives is kept as given. This is the caller step 09 left the function without.
- `farik_request_transition { to, blocker?, resolution?, rejection? }`, `farik_assign_task { task_id, assignee_id, reviewer_id? }` (`ready → assigned` as the caller's role), and `farik_declare_blocked { description, needed }` (`in_progress → blocked` as assignee) go through `Transitions::request`; a `Refused` outcome is `ToolError::Refused` with every detail; `Moved` returns the new status.
- `farik_record_criterion_result { criterion_id, passed, evidence }` emits `criterion.recorded { criterion_id, passed, evidence, run_by, recorded_by }`, `run_by` `assignee` or `reviewer` by the caller's relation to the contract; anyone else is refused, and so is an id the contract does not have. `farik_write_note { kind, text }` emits `note.written { kind: completion | review | progress, text, written_by }`; `completion` only from the assignee, `review` only from the reviewer, `progress` from either; a note does not touch the contract's `notes` field, which is the author's. Both widen `Transitions::context`: `assignee_results` and `done.results` are the latest `criterion.recorded` per criterion and runner, as `CriterionResult`s; `completion_note` and `review_note` the latest of each kind; all counted only since the task's last `task.transitioned` into `in_progress`, so a rejected iteration's evidence does not pass the next. `review.recorded` (8.5) is the orchestrator's summary of a verify session, step 12's.
- `farik_ask_human { question }` emits `question.asked { question, asked_by }` and returns the event's sequence number as the question's id, telling the agent to end its turn. The id is the sequence number because it is unique, needs no second counter, and is what `farik answer` (step 15) takes.
- `farik_write_product_doc { path, content }` on the session's task: `check_product_doc_write` with the epic's kind and status, `user_approved` false until step 14 adds `human.accepted` (so it is refused today, truthfully), and the caller's role; then `ProjectFiles::write_product_doc` and `product_doc.written { path, written_by }`. The tool acts on the session's task, which must be the epic.
- `farik_exec { command, cwd?, timeout_seconds? }`: `evaluate_command` against the team rules, whose `GitViaExec` refusal is ADR 0004's git rule with 5.6's segment and wrapper handling (so `sudo git push` is refused too), and no first-word check of its own; `cwd` defaults to `""` and the environment is empty; run through the context's executor with `spawn_blocking`; `timeout_seconds` defaults to 600 and is capped at 1800. The answer is `{ exit_code, stdout, stderr, timed_out }`, each stream cut at 65,536 bytes, on the last UTF-8 character boundary at or before it, with `[output cut at 64 KiB]` appended when it was. No executor in the context is `ToolError::Failed`.
- Git tools work on the task's worktree, `.farik/local/worktrees/<id>`: status is the new `Git::status(worktree) -> String` (`git status --porcelain`), diff is `Git::diff` from the integration branch (resolved as step 04 resolves it) to `farik/<id>`, commit is the new `Git::commit(worktree, message, paths) -> sha` (`git add -- <paths>` then `git commit -m`), push is the new `Git::push(remote, branch)` of `farik/<id>` to `origin`. A session with no task is refused.
- Events added: `criterion.recorded`, `note.written`, `question.asked`, `product_doc.written`, bodies as above, all `additionalProperties: false` with every field required; all about one contract except `question.asked`, which a conversation with no task can raise; attribution is `recorded_by`, `written_by`, `asked_by`, `written_by`. Every event a tool appends carries the session id, and so does a transition a tool asks for: step 04's `TransitionAsk` gains `session_id: Option<String>`, put on the envelopes it appends (step 04's plan is amended). `docs/SPEC.md` 8.5 gains `criterion.recorded` and `note.written`.
- JSON shapes: `farik_read_board` returns the objects the command line's `farik board --json` prints; `farik_read_rules` and `farik_read_criteria` return the objects `farik rules show --json` and `farik criteria list --json` print, built by the same functions moved beside `file_request` if they are not already shared (none of `TaskProjection`, `TeamRules`, `CriteriaLibrary` serialises).
- `ToolError` has hand-written `Display` (ADR 0006).

## File map

```
docs/schemas/event.schema.json            modifies: the four kinds
crates/protocol/src/event.rs, event/fixtures.rs   modifies: their wiring
crates/store/src/requests.rs              creates: file_request, summary_of; tests
crates/store/src/lib.rs                   modifies: `pub mod requests;`
crates/store/src/git.rs                   modifies: commit, push
crates/store/tests/git.rs                 modifies: their tests
crates/cli/src/task.rs                    modifies: create calls file_request
crates/runtime/Cargo.toml                 modifies: schemars, farik-roles
Cargo.toml                                modifies: schemars =1.2.2
crates/runtime/src/tools.rs               creates: ToolError, ToolContext, ToolDeps, tool_descriptors, call_tool
crates/runtime/src/tools/reading.rs       creates: the four read tools
crates/runtime/src/tools/contracts.rs     creates: triage, write contract, create task
crates/runtime/src/tools/work.rs          creates: transition, assign, blocked, criterion result, note, ask human, product doc
crates/runtime/src/tools/exec.rs          creates: farik_exec
crates/runtime/src/tools/git.rs           creates: the four git tools
crates/runtime/src/transitions.rs         modifies: context reads criterion results and notes
docs/SPEC.md                              modifies: 5.6 (coordination tools are `Read`), 8.2 and 8.3 (the four git tools), 8.5
crates/runtime/src/tools/refusal.rs       creates: the refusal words
docs/plans/project-plan.md                modifies: step 05's interface line; step 07's `tools` field
docs/plans/phase-3-runtime/step-04-governed-transitions.md   modifies (in this plan's commit): the readiness count after a re-triage; `TransitionAsk::session_id`
```

## Interfaces

Consumes: `Transitions`, `TransitionAsk`, `TransitionOutcome` (step 04); `Executor` (step 02); `evaluate_tool_call`, `evaluate_command`, `check_contract_write`, `check_child_creation`, `check_product_doc_write`, `expand_criteria`, `validate_contract` (`farik-core`); `ProjectFiles`, `EventLog`, `Projections`, `Git` (on main, with pull request #12).

Produces:

```rust
pub enum ToolError { InvalidInput { detail: String }, Refused { reason: String }, Failed { detail: String } }
pub struct ToolDeps { pub log: Arc<EventLog>, pub projections: Arc<Projections>, pub files: Arc<ProjectFiles>, pub transitions: Arc<Transitions>, pub git: Git, pub clock: Arc<dyn Clock + Send + Sync>, pub ids: EventIds }
pub struct ToolContext { pub agent_id: String, pub task_id: Option<TaskId>, pub session_id: String, pub executor: Option<Arc<dyn Executor>>, pub deps: Arc<ToolDeps> }
pub struct FarikTool { pub name: &'static str, pub tier: PermissionTier, pub description: &'static str, pub input_schema: Value }
pub fn tool_descriptors() -> Vec<FarikTool>;
pub async fn call_tool(context: &ToolContext, name: &str, input: Value) -> Result<Value, ToolError>;
// farik-store
pub enum RequestError { Refused { reason: String }, Files(FilesError), Store(StoreError) }
pub fn file_request(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str, parent: Option<&TaskId>, now: DateTime<Utc>, ids: &EventIds) -> Result<TaskContract, RequestError>;
impl Git { pub fn status(&self, worktree: &Path) -> Result<String, GitError>; pub fn commit(&self, worktree: &Path, message: &str, paths: &[String]) -> Result<String, GitError>; pub fn push(&self, remote: &str, branch: &str) -> Result<(), GitError>; }
```

## Tasks

Tests that need git are `#[ignore = "needs git"]`, as in step 04. Each tool test builds a `ToolContext` on a `TempRepo` with `.farik/` initialised, a team of a Product Manager `pm`, two Software Developers `dev-a` and `dev-b`, and a log in memory.

### Task 1: the four events, and filing a request in the store

Files: the schema, `event.rs`, `event/fixtures.rs`, `crates/store/src/requests.rs`, `crates/store/src/lib.rs`, `crates/cli/src/task.rs`, `docs/SPEC.md`

- `writes_back_exactly_the_value_it_read_for_every_kind` (existing) covers the four.
- `files_a_request_past_every_committed_contract` — with FRK-3 on disk and an empty log, `file_request` returns FRK-4 and writes it, and the log holds one `task.created`.
- `files_a_child_as_a_triaged_task_of_its_epic` — with `parent: FRK-1`, the contract's `kind` is `task`, `parent` FRK-1, `status` `draft`, the log holds `task.created` then `request.triaged { size: small }`, and the board row is `triaged`.
- `refuses_a_request_that_sets_an_id` — the refusal names `id`; nothing is filed. The command line's existing tests pass unchanged, which is the refactor's check.

- [ ] `refactor(store): file requests in the store for every caller`

### Task 2: the framework and the reading tools

Files: `Cargo.toml`, `crates/runtime/Cargo.toml`, `tools.rs`, `tools/reading.rs`, `lib.rs`

- `lists_every_tool_with_its_tier` — `tool_descriptors()` names the nineteen tools once each, `farik_exec` is `Execute`, `farik_git_commit` `GitLocal`, `farik_git_push` `GitRemote`, and each `input_schema` is an object schema.
- `refuses_a_tool_the_agent_has_no_tier_for` — the Product Manager calling `farik_exec` is `Refused` with reason `tier_not_granted: execute`, and nothing ran.
- `refuses_a_paused_agent` — with `dev-a` paused in the team file, any tool is `Refused` starting `agent_not_active`.
- `refuses_input_that_does_not_fit` — `farik_read_task` with `{ "task_id": 7 }` is `InvalidInput`.
- `refuses_an_unknown_tool` — `farik_nothing` is `InvalidInput` naming it.
- `reads_a_task_with_its_board_row` — `farik_read_task` returns the contract and a `board` object whose `status` is the board's.
- `reads_the_board_the_rules_and_the_criteria` — each returns what `board()`, `team.rules()`, and `read_criteria()` hold, as JSON.

- [ ] `feat(runtime): add the farik tool framework and the reading tools`

### Task 3: triage, contracts, and filing tasks

Files: `tools/contracts.rs`

- `lets_the_product_manager_triage_a_draft_without_a_scrum_master` — `large` sets `kind: epic` in the file and records `request.triaged` with `triaged_by: pm`.
- `refuses_triage_from_a_developer`, `refuses_small_on_a_refining_task`, `refuses_to_triage_what_the_human_already_triaged`, `refuses_to_triage_a_child` — each `Refused`, log unchanged.
- `writes_contract_fields_and_expands_criteria_by_name` — `fields: { intent }`, `criteria: [{ id: C1, name: the-tests-pass }]` gives a file with the intent and C1 expanded from the library, and one `contract.written` with `written_by: pm`.
- `refuses_a_write_to_a_locked_contract` — `Refused` whose reason starts `contract_locked`.
- `fills_the_reviewer_role_the_team_can_staff` — a write setting `assignee_role: software_developer` and no `reviewer_role`, on the team of two Developers and no Architect, gives `reviewer_role: software_developer`; one giving `reviewer_role: architect` keeps it.
- `refuses_a_contract_write_from_a_bystander` — `dev-a` on a task it neither holds nor reviews: reason starts `not_a_contract_writer`.
- `refuses_an_epic_write_while_a_question_is_unanswered` — after a `question.asked` for the epic, `Refused`.
- `files_a_child_of_an_epic_its_assignee_breaks_down` — the epic in progress, assigned to `pm`: `farik_create_task` with `parent` files a child; from `dev-a` it is `Refused` by the child-creation gate.

- [ ] `feat(runtime): add the triage, contract, and task filing tools`

### Task 4: transitions, results, notes, questions, product documents

Files: `tools/work.rs`, `transitions.rs`

- `assigns_a_ready_task_as_the_product_manager` — `Moved`, the board says `assigned`, assignee `dev-a`, reviewer `dev-b`.
- `derives_the_assignee_actor_for_a_move_to_verifying` — `dev-a`'s `farik_request_transition { to: verifying }` is refused for missing results, and the `transition.refused` it records has `actor: assignee` and the session id on its envelope.
- `records_a_criterion_result_as_its_runner` — from `dev-b` on a task it reviews, `run_by: reviewer`; the next `Transitions::context` has it in `done.results`; from `pm` it is `Refused`.
- `reads_the_latest_notes_into_the_done_evidence` — two completion notes from `dev-a`: `done.completion_note` is the second.
- `forgets_an_iterations_evidence_after_a_rejection` — results and a completion note, then a rejection and the return to `in_progress`: the next context has no results and no completion note.
- `asks_the_human_and_answers_with_the_question_id` — returns the `question.asked` event's `seq`.
- `refuses_a_product_document_before_the_user_approves_the_epic` — `Refused` with the gate's words about approval; no file under `product/`.
- `declares_a_block_with_its_blocker` — the board says `blocked` and the `task.transitioned` carries the blocker.

- [ ] `feat(runtime): add the work, question, and product document tools`

### Task 5: commands

Files: `tools/exec.rs`, on a `HostSandbox`

- `runs_a_command_and_cuts_its_output_at_64_kib` — `head -c 100000 /dev/zero | tr '\0' a`: `stdout` is 65,536 characters then the cut note.
- `refuses_git_as_a_command` — `git status` and `sudo git push` are `Refused` starting `git_via_exec` and naming `farik_git_status`; nothing ran.
- `refuses_a_forbidden_command` — with a team rule forbidding `rm -rf *`, that command is `Refused` starting `command_forbidden`.
- `caps_the_timeout` — `timeout_seconds: 99999` runs with 1800 (asserted through a recording executor).

- [ ] `feat(runtime): add farik_exec`

### Task 6: git

Files: `tools/git.rs`, `crates/store/src/git.rs`, `crates/store/tests/git.rs`

- `commits_the_named_paths_and_returns_the_sha` (store) — the commit holds only the named path; the sha is `HEAD`'s.
- `pushes_a_branch_to_a_remote` (store) — to a bare repository as `origin`: the remote has the branch at the sha.
- `commits_through_the_tool_on_the_tasks_worktree` — `farik_git_commit` in the worktree returns the sha; `farik_git_status` then reports clean.
- `refuses_git_tools_without_a_task` — each is `Refused`.

- [ ] `feat(runtime): add the git tools`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
