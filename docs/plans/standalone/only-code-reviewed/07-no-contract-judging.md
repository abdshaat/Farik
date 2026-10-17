# Standalone 7 of 8: no agent judges a contract

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.3 (the judgment rules), 5.12 (`policy.judgment`), 5.16 item 2, 4.1 (setup's advanced step); F9
Depends on: plans 1 to 6 committed on this branch; ADR 0061 (the founder, 2026-10-16, on contract judging: "Remove it")
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin judging are changed or deleted in the task that removes it, never skipped.

The readiness rules and the session go first (Task 1), while `Team::judgment` and `Team::judge` still exist for the runtime that reads them; the team's policy goes second (Task 2), once nothing but the daemon and the retire refusal read it. Line numbers are those of `74b4a5af`; each item is found by the name given beside it.

## Goal

The agent judgment of contracts goes entirely:
- No judgment session is started.
- Readiness no longer requires `judgment_recorded` or `judgment_answers`.
- `catervas_record_judgment`, its prompt and its message go.
- `policy.judgment` in `team.yaml` is accepted on load and ignored, and is dropped when Catervas writes the file.
- Older `contract.judged` events still load.
- The web's "Who checks" setting, in setup and on the Team rules page, goes.

The governor's own check of a contract, rule 9's `judge` (asking `escalated` then `ready`), is not agent judging and stays.

## Decisions

- **Core.**
  - These go from `crates/core/src/governor/readiness.rs`: `JUDGMENT_CHECKS`, `judgment_recorded`, `judgment_answers`, `ReadinessRule::JudgmentRecorded` and `ReadinessRule::JudgmentAnswers` (and their plain words in `plain.rs`), and the context fields they read (`requires_judgment_review`, the judge's review).
  - These go from `crates/core/src/team.rs`: `JudgmentPolicy`, `JudgeChoice`, `Team::judgment` and `Team::judge`, and the check that `required: always` comes with questions (`team.rs:334`).
  - `validate_team` accepts `policy.judgment`, since the schema keeps it, and reads nothing from it.
  - `team/defaults.rs`, `team/template.rs` and `team/describe.rs` write and say nothing about it.
  - The schema (`docs/schemas/team.schema.json`) keeps `judgment` as an accepted key. Its description says "Ignored since ADR 0061: no agent judges a contract. Older team files may carry it."
  - The template schema (`docs/schemas/team-template.schema.json`) drops `judgment` from its policy's `required` (line 94) and keeps it as an accepted key, with its `$defs/judgment` the same as the team schema's (`copies_the_team_schemas_definitions` holds the two equal), so a template saved before loads.
- **Dropped on write.** `Files::write_team` (`crates/store/src/files.rs:187`) and the daemon's `write_team` (`crates/runtime/src/daemon/team.rs:1817`) write the team with no `policy.judgment`. Templates saved and listed (`crates/runtime/src/daemon/templates.rs`, and `template_of` and the apply functions in `crates/core/src/team/template.rs` 108, 165 and 277) carry none.
- **Runtime.**
  - Rule 9 (`crates/runtime/src/orchestrator/requests.rs` 130 to 270) loses the judgment-session branch (`awaits_judgment`, and the checker's `refine` session with `judgment_message`). A contract to be judged goes straight to the governor's `judge`.
  - These go: `catervas_record_judgment` and `record_judgment` (`tools/contracts.rs:169`, `tools.rs`); `JUDGMENT_INSTRUCTION` (`prompt.rs:225`) and its session arm (`session.rs`); `judgment_message` (`messages.rs:111`); and the judge's-review reading in `transitions.rs` (`requires_judgment_review` at 764 and the `JudgmentReview` mapping at 1501 to 1525).
  - These go: the transcripts `judge_ctv_1_passes`, `judge_ctv_1_fails` and `judge_ctv_1_by_architect`, with their functions (`recorded/fixtures.rs:224-246`) and any `catervas-e2e-serve.rs` entry.
  - `apps/web/e2e/approve.spec.ts` no longer lists them, and no longer expects two `contract.judged`.
  - The retire refusal `last_judge` (`leaves_a_gap`, `orchestrator/human.rs:2462-2466`) goes, since no agent is the judge. So do the daemon's codes `judge_not_held`, `no_questions` and `question_length` (`daemon/team.rs:1652-1654`), and the judges answered beside the team (`daemon/team.rs:1479-1489`): the `judges` property of `team.get`'s and `team.check`'s answers and the `planJudges` definition go from `docs/schemas/rpc.schema.json` (970, 1180, 5482, 6357).
- **Older logs.** `EventBody::ContractJudged` stays in the protocol and in `projections.rs:692`'s list, and nothing reads it.
- **Web.**
  - The plan check section of `apps/web/src/pages/setup/SetupAdvanced.tsx` goes: the "required" switch, the questions, and the `planJudge` "Who checks" choice (lines 88 to 210 and its use at 396). So does its place in `TeamRules.tsx` (around lines 351 to 364, and `Checked.judges`).
  - These go too: `Team.tsx`'s "Who checks plans" (line 92); the `Judgment` type and its fields (`setup/TeamSetup.tsx:42-53`, 217, 231, and `SavedTeams.tsx`); `refusals.ts`'s `judge_not_held` and `last_judge`; and their strings in `en.ts`.
- **Roles.** The Architect's and the Scrum Master's prompts stop speaking of checking a contract's plan, if they do. `scrum_master/system.md` and `architect/system.md` are read for "judg" and "check the plan".

## File map

```
crates/core/src/governor/readiness.rs, readiness/fixtures.rs, plain.rs; crates/runtime/src/orchestrator/requests.rs, messages.rs, session.rs; prompt.rs; transitions.rs; tools.rs; tools/contracts.rs; recorded/fixtures.rs, recorded/transcripts/judge_ctv_1_*.jsonl (deleted); crates/cli/src/bin/catervas-e2e-serve.rs; apps/web/e2e/approve.spec.ts; crates/roles/roles/{architect,scrum_master}/system.md   (Task 1)
crates/core/src/team.rs, team/defaults.rs, team/template.rs, team/describe.rs, team/fixtures.rs; docs/schemas/team.schema.json, team-template.schema.json, rpc.schema.json; crates/store/src/files.rs; crates/runtime/src/daemon/team.rs, daemon/templates.rs, orchestrator/human.rs   (Task 2)
apps/web/src/pages/setup/SetupAdvanced.tsx, setup/TeamSetup.tsx, TeamRules.tsx, Team.tsx, SavedTeams.tsx, app/refusals.ts, strings/en.ts and their tests   (Task 3)
```

## Interfaces

Consumes: `validate_team`, `Files::write_team`, rule 9's `judge`, the harnesses of `team.rs`, `readiness.rs`, `requests.rs` and `daemon/team.rs`.
Produces: nothing. Removed: the items in Decisions.

## Tasks

### Task 1: no judgment session runs, and readiness asks for none

Files: those of the File map for Task 1: the core readiness items of Decisions, and the runtime's of **Runtime**. `Team::judgment` and `Team::judge` stay until Task 2; after this task only `daemon/team.rs` and `human.rs`' `leaves_a_gap` read them. These tests are deleted or changed here: the judgment rules' tests in `readiness.rs` and `plain.rs`; the `requests.rs` tests of the checker's session (those using the three transcripts, from 1041); `transitions.rs`' judgment-review tests; `tools/contracts.rs`' `record_judgment` tests; and the session-arm and tool-list tests in `session.rs` and `tools.rs`.

Tests:
- `readies_a_written_contract_without_a_judgment` (`requests.rs`; ignored, needs git): in a team whose file carries `policy.judgment` `{ required: "always", questions: ["Is the plan small enough to review?"], judge: "architect" }` and has an active Architect, a contract the Product Manager has written in `refining` moves to `ready` on the next tick for the governor. No session is started, and no `contract.judged` is recorded. An older log holding a `contract.judged` for it still loads.

- [ ] `feat(runtime): judge contracts by the governor's rules alone`

### Task 2: the team has no judgment policy

Files: those of the File map for Task 2. Tests of the judgment policy (`team.rs`, `team/template.rs` 425 to 490 and 607, `daemon/team.rs` 4011 to 4021, `templates.rs:675`) and `human.rs`' `last_judge` test (4605) are deleted, except the ones changed below.

Tests:
- `ignores_and_drops_the_judgment_policy` (`team.rs`):
  - A team wire with `policy.judgment` `{ required: "always", questions: ["Is the plan small enough to review?"], judge: "architect" }` and no active Architect validates.
  - The team written by `Files::write_team` and read back has no `policy.judgment`.
  - A template made from that team (`template_of`) has no `policy.judgment`.

- [ ] `feat(core): drop the agent judgment of contracts from the team`

### Task 3: setup and the Team rules page stop asking who checks

Files: those of the File map for Task 3; `setup/team.test.tsx`, `settings/settings.test.tsx`, `team.test.tsx` and `refusals.test.ts` change here.

Tests:
- `setup_and_rules_ask_nothing_about_checking_plans` (`setup/team.test.tsx`): setup's advanced step and the Team rules page have no group named "Who checks" and no plan-check switch or questions. A team whose file carries `policy.judgment` opens on both pages with no error.

- [ ] `feat(web): drop the plan check and who checks it`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
