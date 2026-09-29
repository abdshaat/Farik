# Phase 6, step 06: Team setup

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1, 4.4, 5.3 (the configurable judgment), 5.6, 5.12, 10 (foolproof configuration), F1, F2, F15, F16
Depends on: steps 01 to 05 of this phase
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

After step 05's project screen, the wizard walks steps 4 to 8, then Farik starts the team:
- **What we found:** the scan read back in rows.
- **Your team:** five suggested agents, each with a name, avatar and persona, and a place to add or remove agents.
- **What they may do:** the two explicit permission questions.
- **Spending:** an optional daily limit.
- **Finishing work:** the integration policy.
- **Advanced:** team rules, the checks every piece of work must pass, and how plans are judged.
- **Start the team:** resumes the paused team.

The Team page and the agent editor then let the user change any of it later. Every change is validated by the daemon before it is saved, shows its effect in plain words, and can be put back to its default (spec 10).

Built for the first time:
- the configurable readiness judgment (`policy.judgment`, spec 5.3);
- a permission change that waits for the agent's next session (spec 4.4). Today it reaches a running session at the next tool call.

Out of scope, and where each goes:
- skills and connectors on the agent page (phases 8 and 9);
- the Finance Specialist in the builder (phase 8);
- Today and the gates (step 07).

## Decisions

- **Mockups.** Setup screens 4 to 8 and Advanced follow the approved mockups `SetupScan`, `Welcome`, `SetupPermissions`, `SetupSpending`, `SetupFinish`, `SetupAdvanced`, `Team`, and `AgentEdit`, with the eight-step stepper step 05 introduced.
  - Where a mockup shows something the code does not have, the plan records the difference: `SetupScan`'s "42 of them, and they pass today" and "by you" are not scanned, so the rows show only what the scan reads.
- **Scan facts.** `ProjectScan` gains `facts: ScanFacts { language: Option<String>, toolchain: Option<String>, packages: u32, test_runner: Option<String>, last_commit: Option<DateTime<Utc>> }`.
  - `read_back` is built from the facts, and the same line comes out.
  - A new query, `project.scan {}`, answers `{ facts, read_back, checks: [{ name, text }], kept_private: [string] }`.
  - `checks` are the scan's detected criteria.
  - `kept_private` is the team's protected paths that exist in the tree.
  - "Something is wrong" opens a text box whose words are saved as a human note in `.farik/project.md` under "The user says" (a `ProjectFiles::append_project_note`), so the Product Manager reads them.
- **The suggested team.** `team.propose {}` answers the five roles, each with:
  - the role's default model and effort (`load_role`);
  - a suggested name: Mira (Product Manager), Sol (Scrum Master), Ada (Architect), Theo (Developer), Kai (Marketing Specialist), the names the founder approved in the mockups;
  - an avatar key (the role's own);
  - a one-line persona per role, kept in `crates/roles/roles/<role>/role.yaml` as a new `persona:` key.

  "Add someone" picks a role, and its avatar is the first unused `extra-N`. A second agent of a role takes a name from a fixed pool (`Noor`, `Ivo`, `Lena`, `Sami`, `Rui`) and gets an id with a numeral (`developer-2`). The builder keeps 2 to 7 agents, with at least one Product Manager and one Developer (D18); the daemon enforces it.
- **Validation and save.** `team.validate { team }` is a query and `team.save { team }` a method. Both run `validate_team` plus the foolproof rules below and answer `{ errors: [{ path, message }] }`. `team.save` writes only when there are no errors, then appends `team.updated { updated_by: "human" }`, and answers `{ saved: true, effects: [string] }`. The effect sentences are computed by `farik_core::team::describe_change(old, new) -> Vec<String>`, one per change, for example "Theo will use the Everyday model from its next piece of work".

  The foolproof rules, added to `validate_team` in `farik-core` and reported like its other errors:
  - a judge role no active agent holds: "no active agent is a <role> to check plans; choose another checker or turn checking off";
  - `judgment.required` with an empty rubric: "checking plans needs at least one question";
  - any rubric question under 10 or over 200 characters.

  Every setting shows a "Put back" control that restores the schema default.
- **Permissions** (spec 4.1). Two questions, both unanswered at first. Continue is enabled only once both are answered.
  - "May the team run commands?" Yes changes nothing, because the Developer and Architect hold `execute` by default. No writes `revokes: [execute]` on every Developer and Architect.
  - "May the Developer send its work to your online repository?" Yes writes `grants: [git_remote]` on every Developer. No changes nothing.

  The answers are written with the rest of the team at "Start the team", which is also the only way the paused team starts. That keeps spec 4.1's "Nothing runs until the two permissions are set".
- **Spending.** Either "No limit", or a daily limit in dollars (`budgets.daily_usd`, above 0, defaulting to 10 in the field). The note under it lists the limits that always hold, as the mockup does.
  - **The first-day figure** (spec 10) is stated on the Team page and at "Your team" as the sentence "A first full day for a team of five costs under twenty dollars on your own key at today's prices", held in `en.ts`. It is a property of the shipped defaults, re-derived by hand when prices change (ADR 0015), not computed.
- **Finishing work.** A `Choice` with three options, which map to `policy.integration` `auto_merge` (the default), `pull_request`, and `manual`.
- **Advanced** (the Settings switch from step 04 shows it inside the wizard and on the Team page).
  - **Team rules:**
    - keep private files private: protected paths, read-only in the view;
    - every code change comes with new tests: `rules.require_new_tests`;
    - only the Developer changes code: `document_paths`, shown as fixed;
    - "at most $ per piece of work": `rules.max_task_budget_usd`.
    - "Edit as text" shows `team.yaml` read-only, with a line saying edits there are checked the same way. The file is still the source of truth, and nothing in the UI writes raw YAML.
  - **Checks for every piece of work:** the criterion library as a list. The scan's checks are shown. "Add a check" adds a `human` criterion by name and text, of method `review`, since no screen asks for a command or glob (phase decision). Criteria go through `criteria.save { criteria }` (a method that validates, writes, and appends `criteria.updated`).
  - **Checking plans:** the judgment policy below.
- **The configurable judgment** (spec 5.3). `policy.judgment` is added to `team.schema.json`:

  ```
  judgment: {
    required: boolean | "while_judge_active",   // default "while_judge_active"
    questions: string[],                        // 0 to 5; default the two of 5.3
    judge: role                                 // default scrum_master
  }
  ```

  - **Readiness** (`requires_judgment_review` in `transitions.rs`) reads it. `true` always, `false` never, and `"while_judge_active"` while an active agent holds the judge role.
  - **The judgment tool** becomes `farik_record_judgment { answers: [{ pass: boolean, reason: string }] }`, one answer per question in order. It refuses a count that differs from the rubric with `judgment_answers: expected <n> answers, one per question`, and refuses any agent but one of the judge role.
  - **The judgment prompt** (`JUDGMENT_INSTRUCTION`) lists the team's questions, numbered.
  - **The event.** `contract.judged` gains `answers: [{ question, pass, reason }]`. `fits_budget` and `criteria_detect_failure` become optional in the schema, so older logs stay readable, and new events leave them out.
  - **The review.** `judgment_since_written` gives a `JudgmentReview` that passes when every answer passes. An old event with the two booleans passes when both are true.
  - **Transcripts.** The recorded transcripts `judge_frk_1_passes` and `judge_frk_1_fails` are rewritten to the new tool shape.
  - **The screen:** a switch "Check every plan before work starts", the question checkboxes as in the mockup (the two defaults and "Is it small enough to finish in one go?", off by default), an editable list, and "Who checks" as a select of the active agents' roles.
- **Permission changes wait for the next session** (spec 4.4).
  - `SessionRegistration` gains `tiers: Vec<PermissionTier>`, the agent's tiers when the session started.
  - The hook (`daemon/hooks.rs`) decides with the registered tiers, not the team file's.
  - A pause or retire still stops sessions at once, as `update_agent` does now.
  - Model and effort were already fixed at start.
- **The agent editor** (`AgentEdit`):
  - picture: the ten avatars;
  - name: `display_name`;
  - "How <name> talks": `persona`;
  - "How carefully <name> works": Quick, Balanced, or Careful, mapped to effort `low`, `medium`, or `high`;
  - model: under Advanced, a select of the price table's models with plain labels ("Strongest model, thinks hard" for Opus, "Everyday model" for Sonnet);
  - "What <name> may do": the effective tiers in words;
  - under Advanced, seven tier toggles, which write `grants` and `revokes` against the role's defaults.

  It saves through `team.save`, and the page says "Changes start with <name>'s next piece of work". Pause, retire, and replace use the existing `agent_update` command.
- **The AI account in Settings** (ADR 0022). A row names the provider, kind, and where the key is kept, with "Disconnect". The method `account.disconnect {}` removes the key from both stores and answers `{ removed_from: [source] }`. A key set in the environment cannot be removed by Farik, and the row says so.
- **Start the team.** On the last screen, "Start the team" does three things in order:
  1. saves the team, and the criteria if they changed;
  2. sends `team_resume`;
  3. goes to `/`.

  A refusal from `team.save` shows its errors on the screen of the setting at fault, and the stepper links to it.
- **Tests.** Rust unit and route tests. Vitest and axe tests for every screen. The Playwright journey `setup-team.spec.ts` continues from `setup-project.spec.ts`'s end state, using `startServe({ project: true, paused: true })` on an initialised project. It:
  1. accepts the scan;
  2. keeps the five;
  3. answers both permission questions (commands yes, push no);
  4. sets a $10 limit;
  5. chooses auto;
  6. starts the team.

  It then asserts that `team.yaml` has 5 agents and `daily_usd: 10`, that the log has `team.updated` then `team.resumed`, and that `serve.status.paused` is false.

## File map

```
docs/schemas/team.schema.json, event.schema.json, rpc.schema.json          modifies: judgment; contract.judged answers; queries/methods (T1, T2, T4)
crates/core/src/team.rs (+ tests), crates/core/src/team/describe.rs        modifies / creates: foolproof rules; describe_change (T1)
crates/store/src/scan.rs, files.rs (+ tests)                               modifies: ScanFacts; append_project_note (T3)
crates/roles/roles/*/role.yaml, crates/roles/src/lib.rs                    modifies: persona (T3)
crates/runtime/src/{transitions.rs,prompt.rs,tools/*judgment*,daemon.rs,daemon/hooks.rs,daemon/web.rs}, recorded/transcripts/judge_*.jsonl  modifies (T2, T4)
crates/protocol/src/{event.rs,rpc.rs}                                      modifies (T2, T4)
crates/cli/src/setup.rs (account.disconnect via the host), crates/runtime/src/credential.rs  modifies (T4)
packages/protocol-client/src/client.ts                                    modifies: method names (T4)
apps/web/src/pages/setup/{SetupScan,SetupTeam,SetupPermissions,SetupSpending,SetupFinish,SetupAdvanced}.tsx (+ tests)  creates (T5)
apps/web/src/pages/{Team,AgentEdit}.tsx, pages/Settings.tsx (+ tests), app/App.tsx, strings/en.ts   creates / modifies (T6)
apps/web/e2e/setup-team.spec.ts, e2e/fixtures/serve.ts                     creates / modifies (T7)
docs/SPEC.md (4.1, 4.4, 5.3, 8.5), docs/plans/project-plan.md              modifies (T8)
```

## Interfaces

```rust
pub struct ScanFacts { pub language: Option<String>, pub toolchain: Option<String>, pub packages: u32, pub test_runner: Option<String>, pub last_commit: Option<DateTime<Utc>> }
pub fn describe_change(old: &Team, new: &Team) -> Vec<String>;
pub struct JudgmentPolicy { pub required: JudgmentRequired, pub questions: Vec<String>, pub judge: Role }  pub enum JudgmentRequired { Always, Never, WhileJudgeActive }
impl Team { pub fn judgment(&self) -> JudgmentPolicy; }   // the defaults when absent
pub struct JudgmentAnswer { pub question: String, pub pass: bool, pub reason: String }
SessionRegistration.tiers: Vec<PermissionTier>
ProjectFiles::append_project_note(&self, text: &str, date: NaiveDate) -> Result<(), FilesError>;
```

RPC: queries `project.scan`, `team.propose`, and `team.validate`; methods `team.save`, `criteria.save`, and `account.disconnect`.

## Tasks

### Task 1: Foolproof team rules and the change description (`farik-core`)

- `refuses_a_judge_no_active_agent_holds` asserts the sentence at `/policy/judgment/judge` for a judge role no agent holds, and also for one held only by a paused agent.
- `refuses_checking_with_no_questions` asserts that `required: true` with `questions: []` is refused, and that `required: false` with none is accepted.
- `refuses_a_question_too_short_or_long` asserts the bounds.
- `describes_each_change_in_words` asserts exact sentences for a model change, an effort change, a grant, a revoke, a budget set and cleared, and an integration change.
- `defaults_the_judgment_when_absent` asserts that `Team::judgment()` is `WhileJudgeActive`, the two questions, and the Scrum Master.

- [ ] `feat(core): hold the team to foolproof rules and describe each change`

### Task 2: The configurable judgment

- `judges_with_the_team_questions` asserts that the judgment prompt lists the team's three questions, numbered.
- `records_one_answer_per_question` asserts that `farik_record_judgment` with 3 answers appends `contract.judged` with 3 `answers`, each carrying its question text.
- `refuses_the_wrong_number_of_answers` asserts the refusal for 2 answers.
- `passes_only_when_every_answer_passes` asserts that one fail gives a readiness failure sent back to the Product Manager, as today.
- `reads_old_judgments` asserts that an old event with both booleans true still passes.
- `honours_required` asserts that `Never` skips the judgment even with a Scrum Master, and that `Always` needs a judge.
- `refuses_all_but_the_judge_role` asserts the refusal when the judge is the Architect and the Scrum Master calls.

- [ ] `feat(runtime): judge plans by the team's own questions`

### Task 3: Scan facts, notes, and personas

- `reads_the_same_line_from_its_facts` asserts that for the existing scan fixtures, `read_back` is unchanged and `facts` holds each part.
- `keeps_the_user_note_in_project_md` asserts that `append_project_note` adds the text under "The user says" with the date, and that a second note is appended, not replaced.
- `ships_a_persona_per_role` asserts that `load_role` gives a non-empty persona of at most 200 characters for each of the five roles.

- [ ] `feat(store): read the scan back in parts and keep the user's notes`

### Task 4: Team and account over the wire, and permissions from the session's start

- `proposes_the_suggested_five` asserts names, roles, avatars, models, and personas.
- `validates_without_saving_and_saves_with_effects` asserts that `team.validate` returns errors and writes nothing, and that `team.save` of a valid change writes it, appends `team.updated`, and answers its effect sentences.
- `refuses_an_invalid_save_in_words` asserts that a save with no Developer is refused with the core sentence at its path.
- `saves_the_criteria` asserts that `criteria.save` validates, writes, and appends `criteria.updated`.
- `keeps_a_session_to_the_tiers_it_started_with` asserts that after a session registers with `execute`, a `team.save` revoking `execute` still lets that session's `Bash` hook call through, and that the agent's next session is refused.
- `disconnects_the_account` asserts that `account.disconnect` removes the key from both stores and answers them, and that with the key in the environment it answers `[]` with the environment note.
- `answers_the_scan` asserts `project.scan`'s shape on a fixture project.

- [ ] `feat(runtime): propose, validate and save the team, and hold sessions to their starting permissions`

### Task 5: The wizard's screens 4 to 8 and Advanced

- `reads_the_scan_back_in_rows` asserts the rows, the kept-private list, and that "Something is wrong" sends the note.
- `builds_the_team_from_the_five` asserts five rows with name fields; unticking the Scrum Master leaves four; "Add someone" adds a Developer as `developer-2`; a daemon validation error shows at its row.
- `asks_both_permission_questions` asserts Continue is disabled until both are answered, and that "No, nobody may" gives `revokes: [execute]` on the Developer and Architect in the saved team.
- `sets_or_clears_the_daily_limit` asserts that `daily_usd` is `10` with the limit on and absent with No limit.
- `chooses_how_work_is_finished` asserts the three options' `integration` values.
- `edits_the_checks_and_the_judgment` asserts that adding a check calls `criteria.save`, the third question's checkbox adds it, and unticking all questions with checking on shows the daemon's refusal.
- `starts_the_team` asserts the order: `team.save`, then `team_resume`, then navigation to `/`.

- [ ] `feat(web): add the wizard's team, permissions, spending, finishing and advanced screens`

### Task 6: The Team page, the agent editor, and the account row

- `lists_the_team_with_its_costs_line` asserts one card per agent, the first-day sentence, and Pause sending `agent_update`.
- `edits_an_agent_and_says_when_it_applies` asserts that changing effort to Quick saves `low`, and that the page shows "Changes start with Theo's next piece of work" and the effect sentence.
- `toggles_tiers_in_advanced` asserts that the seven toggles show only with Advanced on, and that turning off `execute` for the Developer writes `revokes: [execute]`.
- `shows_and_disconnects_the_account` asserts the Settings row and that Disconnect calls `account.disconnect`.

- [ ] `feat(web): add the team page and the agent editor, and the account row in settings`

### Task 7: The team journey (Playwright)

- `setup-team.spec.ts` is the journey in the Tests decision, with screenshots of "Your team" and "What they may do" at 360 and 1280 px.

- [ ] `test(web): walk the team setup through the real server and browser`

### Task 8: Spec and plan

- Spec 5.3: the paragraph on the configurable judgment says it is built, and gives the tool's new shape.
- Spec 8.5: `contract.judged` gets `answers`, and the two booleans become optional.
- Spec 4.4: say that permission changes wait for the next session, as they now do.
- Spec 4.1: the setup screens and "Start the team".
- Project plan: the step 06 line.

- [ ] `docs(spec): the configurable judgment is built, and team changes wait for the next session`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; @farik/web "Tests  31 passed (31)" (step 05's 20, T5 7, T6 4);
#   playwright "4 passed"; last line: xtask check: ok
```
