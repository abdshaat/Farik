# Phase 6, step 06: Team setup

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1, 4.4, 5.1, 5.3 (the configurable judgment), 5.6, 5.12, 10 (foolproof configuration), F1, F2, F15, F16
Depends on: steps 01 to 05 of this phase (landed)
Readiness confirmed by: fresh-session reviewer, 2026-09-29. Round one was not ready: four unmade decisions (three the planner's, one the founder's) and sixteen findings. All are settled below. Round two, limited to the four, found them settled (ready with findings, folded in).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A project that step 05 has just set up opens on the wizard's steps 4 to 8, in this order:
1. What we found: the scan, read back in rows.
2. Your team: five agents suggested, each with a name, an avatar and a persona.
3. What they may do: two explicit permission questions.
4. Spending: an optional daily limit.
5. Finishing work: the integration policy.

An Advanced area covers team rules, checks, and plan checking.

"Start the team" saves everything and resumes the paused team. After that, the Team page and the agent editor change the same settings. The daemon validates every change before it is saved, and each change shows its effect in plain words and can be put back to its default (spec 10).

Built for the first time:
- the configurable plan check (spec 5.3), with the founder's new rule for who checks;
- a permission change that waits for the agent's next session (spec 4.4).

Out of scope: skills and connectors on the agent page (phases 8 and 9), the Finance Specialist (phase 8), Today and the gates (step 07).

## Decisions

- **Who checks plans** (the founder, 2026-09-29). This replaces the 2026-09-24 decision that the Scrum Master judges. The judge is the first active agent of these roles, in order: Architect, Scrum Master, Product Manager.
  - The Product Manager checks its own plans only when the team has neither an Architect nor a Scrum Master. This is the founder's stated exception to spec 5.1's "nobody grades their own homework", and spec 5.1 and 5.3 record it.
  - `policy.judgment.judge` is `auto` (that order, the default), `architect`, or `scrum_master`. A named judge must be an active agent. `product_manager` is never named; it is reached only through `auto`.
  - Since every team has an active Product Manager (D18), `auto` always finds a judge.
- **The policy** in `team.schema.json`:

  ```
  judgment: { required: "always" | "never", questions: string[], judge: "auto" | "architect" | "scrum_master" }
  ```

  - Defaults: `required: "always"`; `judge: "auto"`; `questions`: the two of spec 5.3 as pinned strings, "Does the task fit its budget?" and "Would its checks notice if the work went wrong the way its intent worries about?".
  - The mockup's third question is the pinned string "Is it small enough to finish in one go?", off by default.
  - Questions are 10 to 200 characters, at most 5.
  - Foolproof rules, in `validate_team`:
    - a named judge no active agent holds is refused ("no active <role> can check plans; choose auto or add one");
    - `required: always` with no questions is refused ("checking plans needs at least one question").
  - With the defaults, every existing `team.yaml` still validates. The 2-agent base fixture judges through the Product Manager.
  - This changes behaviour: a Product Manager and Developer team now judges every contract (one short Product Manager session per plan), where spec 5.3 skipped judging without a Scrum Master. It is the founder's rule. The shared test fixtures whose tests are not about judging (`an_agent_wire`/the base team in `farik-core`'s fixtures, the orchestrator `Harness`, and the CLI `a_team`) set `judgment.required: never` explicitly, so each existing test keeps testing what it names. Judging tests set `always`.
- **The judgment's code path** (the plan file map gains these files).
  - `requests.rs` starts the judgment session for the resolved judge, not `scrum_master(team)`.
  - `judgment_message` (`messages.rs`) and the tool's description (`tools.rs`) take the team's numbered questions.
  - `farik_record_judgment { answers: [{ pass: boolean, reason: string }], reason: string }` takes one answer per question in order, plus an overall reason. It refuses a count that differs from the rubric (`judgment_answers: expected <n> answers, one per question`). It refuses any agent but the resolved judge.
  - `contract.judged` becomes `{ judged_by, answers: [{ question, pass, reason }] (1 to 5), reason }`. `fits_budget` and `criteria_detect_failure` become optional, and new events leave them out. It stays one `eventBodyWire` branch, distinguished by the kind.
  - In core, `JudgmentReview` becomes `{ answers: Vec<JudgmentAnswer>, reason: String }`. `JudgmentAnswer { question, pass, reason }`.
  - An old event maps to two answers under the pinned strings.
  - The readiness rules `JudgmentFitsBudget` and `JudgmentCriteriaDetectFailure` are replaced by `JudgmentAnswers`, which fails listing each failed question and its reason. A judgment made under an older rubric still counts; changing the rubric does not reopen judged contracts.
  - The judge transcripts `judge_frk_1_passes` and `judge_frk_1_fails` are rewritten, and `judge_frk_1_by_architect` is added.
- **Reaching the setup screens** (round one, decision 1).
  - Step 05's setup host writes a machine-local marker `.farik/local/setup-pending` when it ran `farik init` (the step 05 plan's host gains this; it has not been built yet).
  - `serve.status` gains `setup_pending: boolean`.
  - `/` redirects to `/setup/scan` while it is true.
  - "Start the team" removes the marker through `team.start`.
  - A project that already had Farik skips screens 4 to 8, because it has no marker and no pause from setup.
  - "Your team" starts from `team.propose`, and `team.start` replaces `farik init`'s two-agent starter team. That is allowed only while the marker exists, because no work has used those ids.
- **Ids** (round one, decision 2).
  - An agent's id is the slug of its display name: `mira`, `sol`, `ada`, `theo`, `kai`.
  - A second agent whose slug is taken gets `-2`, `-3`, and so on.
  - Extra suggested names come from a fixed pool: Noor, Ivo, Lena, Sami, Rui.
  - An id never changes after it is saved, even if the name does.
- **The AI account** (round one, decision 3).
  - `CredentialStore` gains `fn delete(&self) -> Result<(), CredentialError>`.
  - `WebState` holds the credential stores in both modes.
  - `account.disconnect {}` deletes from every store, appends `team.paused { by: human }` if the team is running, and answers `{ removed_from: [source], paused: bool }`.
  - The Settings row then says: "Disconnected. The team is paused; start Farik again to connect another account." The running driver keeps the key it already loaded, so nothing new may start.
  - A key from the environment cannot be removed, and the row says which variable holds it.
- **Replacing an agent.** Replace is Retire, done through `agent_update`, followed by Add someone of the same role, through `team.save`. The retired agent's memory stays (spec 4.4).
- **Saving the team.**
  - `team.validate { team }` is a query. It answers `{ errors: [{ path, message }], effects: [string] }`, so the effect of a change shows before it is saved (spec 10).
  - `team.save { team }` writes the team only with no errors, then appends `team.updated { updated_by: "human" }`.
    - It refuses a status change ("pause, retire or resume an agent from its card").
    - It refuses removing an id the log has seen ("<name> has done work; retire it instead").
  - `team.start { team, criteria }` is the setup form: it writes both, removes the marker, and appends `team.updated`, `criteria.updated` and `team.resumed`.
  - `describe_change(old, new) -> Vec<String>` gives each effect. The sentences are pinned in its tests.
- **Permissions.** The team policy gains `permissions: { run_commands: boolean (default true), push: boolean (default false) }`.
  - `Agent::tiers` (core) is: the role's defaults; minus `execute` for Developers and Architects when `run_commands` is false; plus `git_remote` for Developers when `push` is true; then the agent's own `grants` and `revokes`.
  - An agent added later therefore follows the answers.
  - A "No" to commands adds the line "Farik still runs every check itself; the agents cannot run commands".
- **Session tiers** (spec 4.4). `SessionRegistration` and `ToolContext` gain `tiers: Vec<PermissionTier>`, taken at session start. The hook and `call_tool` both decide with them. A pause or retire still stops sessions at once.
- **Putting a setting back.** The defaults are the schema's, plus these starter values:
  - `human_accepts_contracts: high_risk`, `wip_limit_per_agent: 1`, `blocked_limit_hours: 24`, `max_iterations: 3`;
  - `integration: auto_merge`;
  - `ambient_messages_per_sprint: 1`, `escalation_age_hours: 24`, `memory_cap_tokens: 8000`;
  - `permissions` as above, `judgment` as above, no `daily_usd`.

  `farik_core::team::defaults() -> TeamDefaults` holds them in one place, and both `starter_team` and the page read them.
- **The scan.**
  - `ScanFacts` mirrors the scan's `Reading`: `language`, `toolchain`, `workspace: bool`, `packages`, `tests_in: Option<String>` (a location), `tracked_files: u32`, and `last_commit`.
  - `read_back` is built from it unchanged.
  - `project.scan` scans again, since it is cheap.
  - `kept_private` lists the protected-path globs that match anything on disk, walked to a depth of 4, skipping `.git` and `node_modules`, capped at 2000 entries.
  - The rows are:
    - "What it is": language and toolchain, and "a workspace of N packages" when there is one.
    - "How it is tested": "tests in <location>", or "no tests found".
    - "How it is checked": the detected checks.
    - "Last change": relative time.
    - "Kept private": the matches.

  The mockup's "a website … React", "3 parts: the shop…", test counts, and "by you" are not scanned, so they are dropped. "Something is wrong" saves the user's words in `.farik/project.md` under "The user says". `project_document` carries that section across every rescan, so a refresh keeps it.
- **Models.**
  - A query, `models.list`, answers the newest model per family from the price table.
  - Labels: `claude-fable-*` "Most capable model"; `claude-opus-*` "Strongest model, thinks hard"; `claude-sonnet-*` "Everyday model"; `claude-haiku-*` "Quick model".
  - Advanced shows the id beside the label.
- **Checks** (Advanced). "Add a check" makes a criterion with the name slugged from its text (kebab, at most 64 characters), the text (at least 10 characters), `source: human`, and `verification: { method: review, rubric: [text] }`. `criteria.save { criteria }` validates it, writes it, and appends `criteria.updated`.
- **Mockup differences.**
  - SetupScan: rows as above.
  - SetupPermissions: "the others send documents" becomes "Farik sends the other agents' documents itself", because only Developers get `git_remote`. The "What each agent may do already" list is shown, from the effective tiers.
  - SetupAdvanced: "Edit as text" is shown on team rules only. The default check "Someone on the team reviewed it" is the reviewer rule that always holds (spec 5.4), shown fixed.
  - Personas: each role's `persona:` in `role.yaml` (a `role.schema.json` change) is the Welcome mockup's line: Mira "Asks the questions that decide what to build"; Sol "Keeps the work moving and nobody stuck"; Ada "Thinks about how it all fits together"; Theo "Builds it and tests it"; Kai "Tells people about what you made".
- **Spending.** "No limit", or a daily limit (the field defaults to $10). The note gives the fixed limits: a session stops after 30 minutes, and a task sent back three times comes to you. The first-day sentence lives in `en.ts`: "A first full day for a team of five costs under twenty dollars on your own key at today's prices" (ADR 0015).
- **Tests.** Every web test also runs axe inside the test. The Playwright journey `setup-team.spec.ts` uses `startServe({ project: true, setupPending: true })` and does the following:
  1. accept the scan;
  2. keep the five;
  3. commands yes, push no;
  4. set a $10 limit;
  5. choose auto;
  6. Start the team;
  7. assert `team.yaml` has five agents with ids `mira`…`kai`, `daily_usd: 10`, `run_commands: true`;
  8. assert the log has `team.updated` and then `team.resumed`;
  9. assert there is no marker and `paused` is false.

## File map

```
docs/schemas/{team,event,rpc,role}.schema.json                             modifies (T1, T2, T4, T3)
crates/core/src/{team.rs,team/describe.rs,team/defaults.rs,governor/permissions.rs,governor/readiness.rs} (+ tests)  modifies / creates (T1, T2)
crates/store/src/{scan.rs,files.rs} (+ tests)                              modifies (T3)
crates/roles/roles/*/role.yaml, crates/roles/src/lib.rs                    modifies (T3)
crates/runtime/src/{transitions.rs,prompt.rs,tools.rs,tools/contracts.rs,orchestrator/requests.rs,orchestrator/messages.rs,recorded/fixtures.rs,recorded/transcripts/judge_*.jsonl}  modifies / creates (T2)
crates/runtime/src/{daemon.rs,daemon/hooks.rs,daemon/web.rs,daemon/setup.rs,credential.rs,tools.rs}  modifies (T4)
crates/cli/src/{init.rs,setup.rs}, crates/cli/tests/{hook.rs,serving.rs}   modifies (T3 init defaults, T4)
crates/protocol/src/{event.rs,rpc.rs}, packages/protocol-client/src/client.ts   modifies (T2, T4)
apps/web/src/pages/setup/{SetupScan,SetupTeam,SetupPermissions,SetupSpending,SetupFinish,SetupAdvanced}.tsx (+ tests), app/App.tsx   creates / modifies (T5)
apps/web/src/pages/{Team,AgentEdit,Settings}.tsx (+ tests), strings/en.ts   creates / modifies (T6)
apps/web/e2e/{setup-team.spec.ts,fixtures/serve.ts}                        creates / modifies (T7)
docs/SPEC.md (4.1, 4.4, 5.1, 5.3, 8.5, 10), docs/plans/project-plan.md (step 06 line; D2 in closed decisions)   modifies (T8)
```

## Interfaces

```rust
pub struct JudgmentPolicy { pub required: JudgmentRequired, pub questions: Vec<String>, pub judge: JudgeChoice }
pub enum JudgmentRequired { Always, Never }  pub enum JudgeChoice { Auto, Architect, ScrumMaster }
impl Team { pub fn judgment(&self) -> JudgmentPolicy; pub fn judge(&self) -> Role; }   // resolved: Architect, else Scrum Master, else PM
pub struct JudgmentAnswer { pub question: String, pub pass: bool, pub reason: String }
pub struct JudgmentReview { pub answers: Vec<JudgmentAnswer>, pub reason: String }
pub struct TeamPermissions { pub run_commands: bool, pub push: bool }
impl Agent { pub fn tiers(&self, permissions: &TeamPermissions) -> Vec<PermissionTier>; }   // was tiers(&self); callers pass team.permissions()
pub fn describe_change(old: &Team, new: &Team) -> Vec<String>;  pub fn defaults() -> TeamDefaults;
pub struct ScanFacts { pub language: Option<String>, pub toolchain: Option<String>, pub workspace: bool, pub packages: u32, pub tests_in: Option<String>, pub tracked_files: u32, pub last_commit: Option<DateTime<Utc>> }
CredentialStore::delete(&self) -> Result<(), CredentialError>;
SessionRegistration.tiers / ToolContext.tiers: Vec<PermissionTier>
```

RPC:
- queries: `project.scan`, `team.propose`, `team.validate`, `models.list`;
- methods: `team.save`, `team.start`, `criteria.save`, `account.disconnect`;
- `serve.status.setup_pending`.

## Tasks

### Task 1: Team policy, permissions, defaults, and change descriptions (`farik-core`)

- `resolves_the_judge_in_the_founders_order`: Architect over Scrum Master over Product Manager, and a paused agent is skipped.
- `refuses_a_named_judge_no_active_agent_holds` and `refuses_checking_with_no_questions` (the exact sentences).
- `accepts_every_existing_team`: the base fixture and `starter_team` validate unchanged.
- `applies_the_permission_answers_to_every_agent_of_the_role`: `run_commands: false` removes `execute` from a Developer added later; `push: true` gives `git_remote` to a Developer only.
- `describes_each_change_in_words`: pinned sentences for model, effort, a grant, a revoke, a budget set and cleared, integration, permissions, and the judge.

- [ ] `feat(core): add the plan-check policy, team permissions and defaults, and describe changes`

### Task 2: The configurable judgment

- `starts_the_judgment_for_the_resolved_judge`: with an Architect, the session is the Architect's (`judge_frk_1_by_architect`).
- `judges_with_the_team_questions`: the message lists three numbered questions.
- `records_one_answer_per_question` and `refuses_the_wrong_number_of_answers`.
- `passes_only_when_every_answer_passes`: one fail sends the contract back, naming the failed question.
- `reads_old_judgments`: both booleans true pass as two answers.
- `honours_never`: no judgment session starts.

- [ ] `feat(runtime): check plans by the team's questions, by the founder's judge`

### Task 3: Scan facts, the user's note, personas, and the defaults in init

- `builds_the_same_line_from_its_facts`: every scan fixture's `read_back` is unchanged.
- `keeps_the_user_note_across_a_rescan`: after `append_project_note` and a `project_document` rewrite, the "The user says" section remains.
- `ships_the_mockup_persona_per_role`: the five exact lines.

- [ ] `feat(store): read the scan back in parts and keep the user's note across rescans`

### Task 4: Team, criteria and account over the wire; session tiers; the setup marker

- `proposes_the_suggested_five`: ids, names, roles, avatars, models, personas.
- `validates_with_effects_and_saves`: validate writes nothing and answers effects; save writes and appends `team.updated`.
- `refuses_status_changes_and_removing_a_worked_agent` (the sentences).
- `starts_the_team_from_setup`: `team.start` replaces the starter team, writes criteria, removes the marker, and appends the three events in order; without the marker, replacing a worked id is refused.
- `keeps_a_session_to_the_tiers_it_started_with`: after a revoke of `execute`, the running session's `Bash` hook call and its `farik_exec` call both pass, and the next session is refused.
- `disconnects_the_account_and_pauses`: stores emptied, `team.paused` appended, answer as specified; an environment key answers `[]` with the variable named.
- `lists_the_models_with_labels`: the newest per family with the pinned labels.
- `reports_setup_pending`: `serve.status.setup_pending` follows the marker.

- [ ] `feat(runtime): propose, validate, save and start the team, and hold sessions to their starting tiers`

### Task 5: The wizard's screens 4 to 8 and Advanced

- `sends_a_new_project_to_the_scan`: `/` goes to `/setup/scan` while `setup_pending` is true.
- `reads_the_scan_back_in_rows`: the rows as specified; "Something is wrong" sends the note.
- `builds_the_team_from_the_five`: five rows; untick the Scrum Master; Add someone gives `noor` as a second Developer; a daemon error shows at its row.
- `asks_both_permission_questions`: Continue stays disabled until both are answered; "No" sets `run_commands: false` and shows the Farik-still-checks line.
- `sets_or_clears_the_daily_limit`, and `chooses_how_work_is_finished`.
- `edits_the_checks_and_the_plan_check`: Add a check calls `criteria.save` with the review rubric; the third question toggles; a named judge that is not held shows the daemon's refusal.
- `starts_the_team`: `team.start`, then navigation to `/`.

- [ ] `feat(web): add the wizard's team, permissions, spending, finishing and advanced screens`

### Task 6: The Team page, the agent editor, the account row

- `lists_the_team_with_the_first_day_line`: the cards, and Pause sending `agent_update`.
- `edits_an_agent_and_shows_the_effect_first`: changing effort to Quick shows the validate effect before Save, and saves `low`.
- `toggles_tiers_in_advanced`: seven toggles only with Advanced on; an Architect's `network` off writes `revokes: [network]`.
- `replaces_an_agent`: Retire, then Add someone of the same role.
- `shows_and_disconnects_the_account`: the row, and the paused note after Disconnect.

- [ ] `feat(web): add the team page and the agent editor, and the account row`

### Task 7: The team journey (Playwright)

- `setup-team.spec.ts`: the journey in the Tests decision, with screenshots of "Your team" and "What they may do" at 360 and 1280 px.

- [ ] `test(web): walk the team setup through the real server and browser`

### Task 8: Spec and plan

- Spec 5.1 and 5.3: the judge order, and the Product Manager's exception.
- Spec 5.3 also: the policy and the tool.
- Spec 8.5: `contract.judged`.
- Spec 4.4: tiers wait for the next session.
- Spec 4.1: steps 4 to 8 and Start the team.
- Spec 10: effects before saving.
- The project plan: the step 06 line, and D2's row updated with the founder's 2026-09-29 rule.
- The step 05 plan: a line recording the marker the host gained.

- [ ] `docs(spec): the plan check's judge and questions, and team changes that wait for the next session`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/web: step 05's landed count plus 13 (T5 8, T6 5);
#   playwright 4 passed; last line: xtask check: ok
```
