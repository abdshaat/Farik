# Phase 4, step 01: Remaining roles

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 5.1, 5.3, 5.4, 5.6, 5.16, 6.2, 6.3, 6.5, 8.2, 8.5; D2, D6
Depends on: phase 3 (merged in #11), the audit cuts (merged in #13); nothing from this phase
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the three decisions the first found open, and one more it found, folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A team may hold all five launch roles, and a team with an active Scrum Master works end to end: the Scrum Master triages every request, judges each contract against the Definition of Ready's judgment rules before the governor passes it, is assigned approved epics and breaks them down, and assigns the ready tasks; the Architect reviews a Developer's work; and an epic the Scrum Master broke down is reviewed by the Product Manager, as 5.16 item 4 says. Today such a team cannot work at all: `load_role(ScrumMaster)` is `NotFound`, the orchestrator gives triage to the Product Manager, which the triage tool refuses when a Scrum Master is active, readiness requires a judgment nothing records, and an approved epic is assigned to the Product Manager, which the assignment gate refuses. Out of scope: the document-only rule for roles other than the Developer (step 02), sprints and ceremonies (steps 03 and 05), the channel (step 04), memory and decision tools (step 06).

## Decisions

- The three roles, from 6.2, 6.3, and 6.5, in `crates/roles/roles/<role_id>/` like the first two: the Scrum Master on `claude-sonnet-5` at `medium`, the Architect on `claude-opus-5` at `high`, the Marketing Specialist on `claude-sonnet-5` at `medium` (8.2; revision 11). One skill each: `keeping-work-flowing` (Scrum Master: triage by size, the judgment rubric, breaking an epic into tasks with deliverables and criteria, assigning within WIP), `reviewing-for-design` (Architect: reviewing a Developer's diff for design, writing a design note and the review note), `marketing-what-ships` (Marketing Specialist: market research with the web tools, a marketing plan, release notes and README copy). Each `system.md` states the mandate, the forbidden list, the untrusted-content warning (8.6), and how a session ends, as the first two do; every role but the Developer's forbids writing application code (the founder, 2026-09-24; enforced in step 02). The Architect's 6.3 "spikes" are dropped: an Architect's task is a document.
- `default_tiers`: the Architect becomes `read, write_workspace, execute, network, git_local`; the Marketing Specialist `read, network, write_workspace, git_local`, because a task reaches `verifying` only with a commit on its branch. Chose this over a per-role path ceiling (revision 11). Spec 5.6's table changes.
- Triage goes to the triager: the Scrum Master when the team has an active one, else the Product Manager, the rule `farik_triage_request` already applies (5.16). The move from `draft` to `refining` stays the Product Manager's, whose row it is (5.2).
- The judgment (D2): a contract that is to be judged (the rule 9 test phase 3 wrote) on a team with an active Scrum Master, whose Definition of Ready, evaluated on the context the governor would use, fails on `JudgmentRecorded` alone, gets one Scrum Master session first, purpose `refine`, its one tool `farik_record_judgment`, no built-in tool, on the Scrum Master's own model. Chose previewing the readiness over judging first, because a governor refusal counts as one of the three readiness attempts and a structurally broken contract should go back to the Product Manager without spending a Scrum Master session. The preview records nothing.
- `farik_record_judgment { fits_budget: bool, criteria_detect_failure: bool, reason: String }`, tier `read`, accepts the Scrum Master alone, on a task `refining`, with a non-blank reason, and records `contract.judged { judged_by, fits_budget, criteria_detect_failure, reason }`, about one contract. Chose a new kind over widening `review.recorded`, whose readers count verifications (F17).
- The judgment rules (`JudgmentRecorded`, `JudgmentFitsBudget`, `JudgmentCriteriaDetectFailure`) are evaluated only when every other rule passes, in `farik-core`: a contract that fails a structural, team, or parent rule is refused on those alone, so the Product Manager is not told a judgment is missing that the Scrum Master is not asked for until the contract is otherwise ready. Chose this over telling the Product Manager about a judgment it cannot give. The epic approval row (`ContractRequiresHuman`), which reads the same failures, therefore still stays shut until the judgment passes.
- A judgment session that ends without recording one is asked again on the next tick, as a refine session that writes nothing is; `max_sessions` bounds both. `farik_record_judgment` is tier `read` and so listed in every session whose agent holds `read`; the tool refuses everyone but the Scrum Master, as `farik_triage_request` refuses by role.
- Readiness reads the judgment as the last `contract.judged` after both the last `contract.written` and where refining last began, else none: a contract written after its judgment needs a new one. A failed judgment is a failed Definition of Ready like any other, counts as an attempt, and reaches the Product Manager's next refine message as its failures (phase 3's `last_readiness_failures`).
- A session with one tool: `SessionAsk` gains `only_tool: Option<&'static str>`, which gives the session that Farik tool alone and no built-in tool; triage uses it with `farik_triage_request` (its model stays `TRIAGE_MODEL` by its purpose), the judgment with `farik_record_judgment`. `PromptInput` gains `closing: Option<&'a str>`, the `This session` text when it is not the purpose's own; the judgment passes `JUDGMENT_INSTRUCTION`. ADR 0011's sections do not change.
- An approved epic on a team with an active Scrum Master is assigned to the Scrum Master on its behalf (actor `ScrumMaster`), with the Product Manager as its reviewer, under the WIP rule phase 3 applies to the Product Manager; without one, as phase 3 does. `reviewed_by_the_human` is unchanged.
- Rule 5 routes on the kind: every `verifying` epic goes to `verifying_epic`, never to the task path, which diffs a branch and runs in a worktree an epic does not have. `verifying_epic` keeps its start for both reviewers: nothing while a task under it awaits integration, then Farik's runs of its mechanical criteria on the integration branch, whose unrunnable wording says "for its reviewer" rather than "for the human". Then, for an epic the human reviews, phase 3's flow unchanged. For one the Product Manager reviews: the Product Manager's read-only `verify` session as reviewer, in the project root (`cwd` the files' root, as the human-reviewed flow's accept session), its first message `epic_review_message`: the epic's contract, Farik's results, and each task under it with its status and its completion note, in place of a diff; it answers the `review` criteria and writes the review note. A failed criterion is a rejection Farik files in the Product Manager's name as `reject` (verify.rs, made `pub(super)`) does for a task. With every criterion passed, the epic waits for the human's acceptance (every epic, 5.4 item 5), then the Product Manager's session, told the human accepted, asks for `accepted`, and `review.recorded` names the Product Manager.
- The human may accept an epic's result only once every one of Farik's runs of its mechanical criteria passed, whoever reviews it: `accept_result`'s check (human.rs) applies to every epic, not only one the human reviews (5.4's rule for the human-reviewed epic, extended).
- A rejected epic goes back to `in_progress` by the governor's row, as a task does; its assignee's next `plan` session, when every task under it is done, is given the last rejection (the failed criteria's ids and the review note) in `close_out_message` and asked to file the tasks that fix it rather than close it out again. The last rejection is read from the epic's last `task.transitioned` into `rejected` (its `rejection`: failed criterion ids and reasons, the review note among them), and it is shown only while no task under the epic was created after that move; once the fixing tasks are filed and done, the close-out is the plain one. A new rejection raises the iteration, which the limit bounds.
- For an epic the Product Manager reviews, `review.recorded` is recorded by `record_review` (verify.rs) at the end of the Product Manager's review session, naming it, as for a task; `record_epic_review`, which names the human, runs only on the human-reviewed path. The human's `HumanAccept` of any epic needs a message, as it does today for the human-reviewed one; it may come before or after the Product Manager's review, and the acceptance session waits for both.
- The Product Manager and the Scrum Master are never an ordinary task's assignee: `REVIEWER_ROLE_FOR` gains no row (revision 11).

## File map

```
crates/roles/roles/scrum_master/role.yaml, system.md, skills/keeping-work-flowing/SKILL.md            creates
crates/roles/roles/architect/role.yaml, system.md, skills/reviewing-for-design/SKILL.md               creates
crates/roles/roles/marketing_specialist/role.yaml, system.md, skills/marketing-what-ships/SKILL.md    creates
crates/roles/src/lib.rs                          modifies: load_role's three arms; tests
crates/core/src/governor/permissions.rs          modifies: default_tiers; its test
crates/core/src/governor/readiness.rs            modifies: the judgment rules only when the rest pass; JudgmentReview's doc; tests
crates/runtime/src/cost.rs                       modifies: the stale NotFound arm's comment and test message
docs/schemas/event.schema.json                   modifies: contract.judged and contractJudgedBody
crates/protocol/src/event.rs                     modifies: body_def_name, is_about_one_contract, attribution, EVERY_KIND
crates/protocol/src/lib.rs                       modifies: KINDS
crates/protocol/src/event/fixtures.rs            modifies: a contract.judged fixture
crates/runtime/src/tools.rs                      modifies: farik_record_judgment in TOOLS and the dispatch
crates/runtime/src/tools/contracts.rs            modifies: record_judgment; tests
crates/runtime/src/tools/refusal.rs              modifies: JudgmentNotAllowed
crates/runtime/src/transitions.rs                modifies: judgment_review read from the log; tests
crates/runtime/src/prompt.rs                     modifies: PromptInput::closing, JUDGMENT_INSTRUCTION; tests
crates/runtime/src/orchestrator/session.rs       modifies: SessionAsk::only_tool
crates/runtime/src/orchestrator/messages.rs      modifies: judgment_message, epic_review_message, close_out_message with a rejection
crates/runtime/src/orchestrator/requests.rs      modifies: triager, draft, refining, ready_epic, verifying_epic, in_progress_epic; tests
crates/runtime/src/orchestrator/verify.rs        modifies: rule 5 routes on the kind, reject pub(super), SessionAsk literal; tests
crates/runtime/src/orchestrator/human.rs         modifies: accept_result's criteria check for every epic; tests
crates/runtime/src/orchestrator/rules.rs         modifies: the Scrum Master assigner test (Task 1), SessionAsk literals (Task 5)
crates/runtime/src/prompt.rs                     modifies (Task 2): the execute-alone fixture no longer an Architect
crates/runtime/src/recorded/transcripts/         creates: triage_by_sm_frk_1.jsonl, judge_frk_1_passes.jsonl, judge_frk_1_fails.jsonl, review_epic_frk_1.jsonl
docs/SPEC.md                                     modifies: revision 0.10; 5.3, 5.6, 5.16 item 3 and 4, 6.2, 6.3, 6.5, 8.5
docs/plans/project-plan.md                       modifies: step 01's interface line
```

## Interfaces

Consumes: `Role`, `default_tiers`, `ReadinessContext`, `JudgmentReview`, `evaluate_readiness`, `ReadinessRule` (`farik-core`); `EventBody`, `EventKind` (`farik-protocol`); `Transitions::context`, `refining_began`, `reviewed_by_the_human` (`farik-runtime::transitions`); `SessionAsk`, `run_session`, `product_manager`, `run_on_the_integration_branch`, `read_only` (`farik-runtime::orchestrator`); `RecordedAdapter::with_tools`, `Harness` (tests).

Produces:

```rust
// farik-protocol (from the schema by import_types!): ContractJudgedBody { judged_by: String, fits_budget: bool, criteria_detect_failure: bool, reason: String }
// EventKind::ContractJudged, EventBody::ContractJudged(ContractJudgedBody); EVERY_KIND: [EventKind; 32]
// farik-runtime::tools
pub(crate) struct RecordJudgmentInput { pub fits_budget: bool, pub criteria_detect_failure: bool, pub reason: String }
// farik-runtime::transitions
pub(crate) fn judgment_since_written(history: &[FarikEvent]) -> Option<JudgmentReview>;
// farik-runtime::prompt
pub const JUDGMENT_INSTRUCTION: &str;
pub struct PromptInput<'a> { /* as before */ pub closing: Option<&'a str> }
// farik-runtime::orchestrator (crate-private)
pub(super) struct SessionAsk<'a> { /* as before */ pub(super) only_tool: Option<&'static str> }
pub(super) fn triager(team: &Team) -> Option<&Agent>;
pub(super) fn judgment_message(contract: &TaskContract) -> String;
pub(super) fn epic_review_message(contract: &TaskContract, results: &[CriterionResult], tasks: &[(TaskProjection, Option<String>)]) -> String;
pub(super) fn close_out_message(contract: &TaskContract, tasks: &[(String, String, String)], rejection: Option<(&[String], &str)>) -> String; // gains the last rejection: failed ids, review note
```

A test marked "guard" passes before its task's change: it is written after the task's first test passes and proven by mutation at the landing review (re-introduce the bug, watch it fail), not by a red run.

## Tasks

### Task 1: the three roles as data

Files: creates the three role directories; modifies `crates/roles/src/lib.rs`, `crates/runtime/src/orchestrator/rules.rs` (the one test that asserts the Scrum Master does not load), `crates/runtime/src/cost.rs` (the `NotFound` arm's comment and the "an Architect with no model is skipped" test message, which no longer describe a role that does not load)
Produces: `load_role` for every agent role
Consumes: nothing new

- `loads_the_scrum_master` — `model == "claude-sonnet-5"`, `effort == Medium`, `default_tiers == default_tiers(ScrumMaster)`, one skill `keeping-work-flowing` with a non-empty description and body, a system prompt containing `untrusted` and `farik_triage_request`.
- `loads_the_architect` — `claude-opus-5`, `High`, skill `reviewing-for-design`, a system prompt containing `untrusted` and `farik_write_note`.
- `loads_the_marketing_specialist` — `claude-sonnet-5`, `Medium`, skill `marketing-what-ships`, a system prompt containing `untrusted`.
- `forbids_application_code_to_every_role_but_the_developer` — for each of the four other roles, `forbidden` holds `write application code`.
- `refuses_a_role_that_is_not_shipped` — changed: `Human` alone is `NotFound`.
- `holds_every_shipped_role_to_its_schema` — unchanged, now over five roles.
- `takes_the_scrum_master_as_assigner_when_there_is_one` (rules.rs) — rewritten here, since this task makes its old assertion false: a ready standalone task on a team with an active Scrum Master starts the Scrum Master's `plan` session (the replayed `plan_assigns_frk_1`) and no other.

- [x] `feat(roles): ship the scrum master, the architect, and the marketing specialist`

### Task 2: the tier changes

Files: modifies `crates/core/src/governor/permissions.rs`; `crates/runtime/src/prompt.rs` (the fixture of `names_the_shell_for_an_agent_with_either_execute_or_git_local` that uses an Architect as "execute alone" becomes a Developer that revokes `git_local`)
Produces: `default_tiers(Architect)`, `default_tiers(MarketingSpecialist)` as decided
Consumes: nothing

- `gives_each_role_the_default_tiers_of_the_spec_table` — changed: the Architect holds `read, write_workspace, execute, network, git_local` and the Marketing Specialist `read, network, write_workspace, git_local`, each in that order; the others unchanged.

- [x] `feat(core): let the architect and the marketing specialist commit their documents`

### Task 3: `contract.judged` and `farik_record_judgment`

Files: modifies the event schema (whose types `import_types!` builds, ADR 0009), `event.rs`, `protocol/src/lib.rs`, `event/fixtures.rs`, `tools.rs`, `tools/contracts.rs`, `tools/refusal.rs`
Produces: `ContractJudgedBody`, `RecordJudgmentInput`, the tool
Consumes: `Call`, `append` of the tools

- `records_a_judgment_by_the_scrum_master` — the Scrum Master on a `refining` task: one `contract.judged` with its three fields and `judged_by` the agent, and the tool answers the recorded seq.
- `refuses_a_judgment_by_anyone_else` — the Product Manager, a Developer: `Refused` with a reason starting `judgment_not_allowed`, nothing recorded.
- `refuses_a_judgment_outside_refining` — a task `ready`: `judgment_not_allowed`.
- `refuses_a_blank_reason` — `reason: "  "`: `blank_reason`.
- `names_the_contract_a_judgment_is_about` — `is_about_one_contract(ContractJudged)` is true, and appending one with no task id is refused as the other contract kinds are.
- `lists_every_kind_the_schema_lists` — unchanged, now 32.

- [x] `feat(runtime): record the scrum master's judgment of a contract`

### Task 4: readiness reads the judgment

Files: modifies `crates/core/src/governor/readiness.rs` (the judgment rules only when the rest pass; `JudgmentReview`'s doc names `contract.judged`), `crates/runtime/src/transitions.rs`
Produces: `judgment_since_written`
Consumes: `ContractJudgedBody` (Task 3)

- `asks_no_judgment_of_a_contract_that_fails_another_rule` (readiness.rs) — `requires_judgment_review: true`, no review, a contract with no `out_of_scope`: the failures are exactly `[OutOfScopePresent]`.
- `asks_the_judgment_of_an_otherwise_ready_contract` (readiness.rs, guard) — the same with `out_of_scope` filled: exactly `[JudgmentRecorded]`.
- `passes_a_contract_the_scrum_master_judged` — a team with an active Scrum Master, a structurally complete contract written, then judged with both answers true: `refining -> ready` as the governor moves it.
- `refuses_a_contract_the_scrum_master_judged_too_large` — `fits_budget: false`: refused, one failed `contract.evaluated` recorded whose failures hold the text "too large for its budget" and the judgment's reason.
- `refuses_a_contract_judged_before_its_last_write` (guard) — judged, then written again: refused, a failure holding "judgment review is not recorded".
- `ignores_a_judgment_from_before_refining_began` (guard) — judged, then re-triaged large: the same failure.
- `passes_a_child_filed_whole_once_judged` — a breakdown's task filed whole (no `contract.written` since refining began), judged after refining began with both answers true: `refining -> ready`.
- `asks_no_judgment_without_a_scrum_master` (guard) — a team with none passes without one.

- [x] `feat(runtime): judge readiness on the scrum master's last judgment`

### Task 5: sessions with one tool and the judgment session

Files: modifies `prompt.rs`, `orchestrator/session.rs`, `orchestrator/messages.rs`, `orchestrator/requests.rs`, and the `SessionAsk` literals in `orchestrator/rules.rs` and `orchestrator/verify.rs`; creates `judge_frk_1_passes.jsonl`, `judge_frk_1_fails.jsonl`, `triage_by_sm_frk_1.jsonl`
Produces: `SessionAsk::only_tool`, `PromptInput::closing`, `JUDGMENT_INSTRUCTION`, `triager`, `judgment_message`
Consumes: Task 3's tool, Task 4's readiness

- `closes_with_the_given_instruction` — `PromptInput { closing: Some("x"), .. }`: the `This session` section is `x`; with `None`, the purpose's own, as today.
- `gives_a_one_tool_session_that_tool_alone` — a session asked with `only_tool: Some("farik_record_judgment")`: its spec's `farik_tools` is that one name and its built-in tools are empty.
- `triages_with_the_scrum_master_when_there_is_one` — a draft on a team with an active Scrum Master: the triage session is the Scrum Master's, on `TRIAGE_MODEL`, with `farik_triage_request` alone, and its replay records `request.triaged` by the Scrum Master.
- `asks_the_scrum_master_to_judge_a_written_contract` — a written, structurally complete contract `refining` with no judgment: the tick starts the Scrum Master's `refine` session with `farik_record_judgment` alone and `JUDGMENT_INSTRUCTION` in its prompt; after the passing replay, the next tick moves the task to `ready`.
- `sends_a_badly_judged_contract_back_to_the_product_manager` — the failing replay (`criteria_detect_failure: false`), then two ticks: one failed `contract.evaluated` whose failures hold "would not detect the failure", then the Product Manager's refine session whose first message holds the judgment's reason.
- `judges_a_structurally_broken_contract_without_the_scrum_master` — a contract with no `out_of_scope`: no Scrum Master session starts; the governor's refusal holds the `out_of_scope` failure and no judgment failure (Task 4's core change).
- Phase 3's `triages_*` tests on a team with no Scrum Master pass unchanged.

- [x] `feat(runtime): give the scrum master triage and the judgment of each contract`

### Task 6: epics under a Scrum Master

Files: modifies `orchestrator/requests.rs`, `orchestrator/verify.rs`, `orchestrator/messages.rs`, `orchestrator/human.rs`; creates `review_epic_frk_1.jsonl`, `review_epic_fails_frk_1.jsonl`
Produces: the epic rules on a team with an active Scrum Master, `epic_review_message`, `close_out_message` with a rejection
Consumes: `run_on_the_integration_branch`, `reject` (made `pub(super)`), `read_only`

- `assigns_an_approved_epic_to_the_scrum_master` — a `ready` epic, Scrum Master active: moved to `assigned` with the Scrum Master as assignee and the Product Manager as reviewer, `requested_by` the Scrum Master, no session.
- `breaks_an_epic_down_with_its_scrum_master` (guard) — the epic `in_progress` with no task: the Scrum Master's `plan` session with the breakdown message.
- `has_the_product_manager_review_an_epic` (`#[ignore]`, needs git) — every task under the epic accepted and integrated, the epic `verifying`: the first tick runs its mechanical criteria on the integration branch; the second starts the Product Manager's read-only `verify` session as reviewer, in the project root, its first message holding each task's id and completion note and no diff; after the replay's review note, a tick starts no session until the human's `HumanAccept` with a message; then the Product Manager's session asks for `accepted`; the one `review.recorded` names the Product Manager as reviewer.
- `closes_out_plainly_once_the_fixing_tasks_are_filed` — a rejected epic back `in_progress` with a task created after the rejection and accepted: the Scrum Master's `plan` session's first message holds no rejection.
- `rejects_an_epic_whose_criterion_the_product_manager_failed` (`#[ignore]`, needs git) — the replay fails a `review` criterion: Farik files `rejected` in the Product Manager's name with the criterion's id; after the governor's return to `in_progress`, the Scrum Master's `plan` session's first message holds that id and the review note and asks for tasks that fix it.
- `refuses_the_humans_acceptance_of_an_epic_before_farik_ran_its_criteria` — a Scrum Master's epic `verifying` with no governor run recorded: the human's `HumanAccept` is refused, naming the criteria not run (as for a human-reviewed epic).
- `takes_one_request_to_an_accepted_epic` (orchestrator.rs, the existing end-to-end test, no Scrum Master) passes unchanged.

- [x] `feat(runtime): run an epic with its scrum master and its product manager reviewer`

### Task 7: the spec

Files: modifies `docs/SPEC.md`, `docs/plans/project-plan.md`

The header gains revision 0.10 naming each change: 5.3's judgment recorded by `farik_record_judgment` as `contract.judged` from a Scrum Master session, asked only once every other rule passes, a later write needing a new one; 5.4's human acceptance of any epic only after Farik's runs passed; 5.6's tier table (Architect `git_local`, Marketing Specialist `write_workspace`, `git_local`); 5.16's triage sentence (the triager's model, not "whatever the Product Manager's own"), item 3 (Farik assigns an approved epic to the Scrum Master on its behalf when the team has one, the Product Manager its reviewer) and item 4 (how the Product Manager reviews an epic, and a rejected epic's close-out); 6.2's triage; 6.3 without spikes; 6.5's tiers; 8.2's sentence on which Farik tools a session is registered with (a triage session `farik_triage_request` alone, a judgment session `farik_record_judgment` alone); 8.5's `contract.judged`. The step's interface line in the project plan is written as landed.

Also, on the founder's decision of 2026-09-24, specified now and built with the team editor (phase 5 step 07), not in this step: 5.3 gains the configurable judgment. A team policy `judgment` in `team.yaml` holds `required` (default on while the team has an active agent of the judging role), `questions` (the rubric, default the two of 5.3: the task fits its budget; the criteria would detect the failure the intent worries about), and `judge` (the role that judges, default the Scrum Master); the judge answers each question pass or fail with a reason, and any fail is a readiness failure. Until it is built, this release judges with the defaults, as this step implements them. Section 10 gains a requirement that configuring the harness is user-friendly and foolproof, and F1 and F15 say the team editor meets it: every setting has a safe default and a one-line plain-language explanation; a change that would leave the team unable to work (a judge role nobody active holds, an empty rubric with judgment on, a team without the two required roles) is refused before it is saved, in words that say what to change; each change shows its effect before it is saved; and every setting can be put back to its default. The judgment's record stays the `contract.judged` event in the log (the founder: the log is durable enough).

- [x] `docs(docs): record the scrum master's sessions and the new roles in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
