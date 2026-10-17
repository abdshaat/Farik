# Standalone CTV-9, plan 7: sprint words leave the roles, tools, help and README

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 6.1 to 6.6, 6.7; recorded by plan 8 (revision 0.116)
Depends on: plans 1 to 6 of this folder (committed on this branch), so the words describe what shipped
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

No role prompt, role file, kit, skill, tool description, ceremony instruction, CLI help text, CLI
output, schema description, README or CONTRIBUTING line mentions a sprint, sprint planning, a
standup, a sprint budget, the Backlog as a holding area or "Start sprint". The Scrum Master runs
each delivery's review and retro and keeps work flowing; the Product Manager's changelog task is
filed by Catervas. Words only: no behaviour changes. What is left of "sprint" in the tree is code
and schema text that reads old logs, keys and contracts, each listed with its reason in the last
commit's body (epic C5's first question).

## Decisions

- After the rebase, ADR 0061's plan 1 Task 4 has already rewritten review sentences in several of
  these files (`scrum_master/system.md`, `keeping-work-flowing`, the Product Manager's
  `writing-task-contracts` and `catervafying-the-repository`, the Architect's and the Marketing
  Specialist's `catervafying-the-repository`): this plan changes only their sprint, standup and
  sprint-budget words and keeps ADR 0061's.
- The Scrum Master's kit loses `planning-a-sprint` (its folder, its `kit.yaml` entry, its
  `include_str!` in `crates/roles/src/kit.rs`): chose deleting it over rewriting it, because nothing
  is planned any more. `running-ceremonies` keeps its name and covers "the review and the retro of
  each delivery" (its standup section goes; description: "Use in a delivery's review or retro, to
  run it from the board and post its result to the team channel."). `writing-escalation-digests`'
  description: "Use at a delivery's review, when escalations are open, to give the human one short
  list of what they must decide."; its body says the review's first message holds the digest.
  `keeping-work-flowing` replaces "more of the sprint than the contract allows" by "more than the
  contract allows". `system.md` and `role.yaml`: "You run each delivery's review and retro, and you
  keep escalations clean"; it produces "review summaries, retro notes, and escalation digests"; its
  folder holds "the team's cadence and ceremony notes".
- Its `catervafying-the-repository` writes the cadence "at the retro of the delivery that
  documented the project": how a delivery closes (an accepted epic, five accepted tasks, or no work
  left), when the review and the retro run, what the owner is asked and when; its "## Each sprint"
  becomes "## Each delivery". The document's plain name in `crates/core/src/folders.rs`
  `DOCUMENT_STEMS` becomes "Delivery rhythm".
- The Product Manager's changelog sentence, in `system.md` and `scoping-a-release`, becomes:
  "`CHANGELOG.md`. Each time a delivery closes, Catervas files a docs task of yours whose
  `allowed_paths` name `CHANGELOG.md`; in it, add what the delivery's accepted tasks changed, in the
  user's words." `writing-task-contracts` drops "what is left of the sprint when it has a budget";
  `prioritising-the-backlog` keeps its name and the word backlog (the Product Manager's ordered list
  of requests, not a holding area), asks "what gets worse if it waits until the next delivery", and
  says the team's assigner reads the channel; its `catervafying-the-repository` says "## Each
  delivery" and "At each delivery's review"; its "sprint reports" are "delivery reports" (in
  `system.md` and `role.yaml`). `deciding-data-pipelines` is not here: ADR 0061's plan 5 deleted
  it.
- The Architect's `catervafying-the-repository`: "At each delivery's review, bring features, jobs
  and structure up to date with the changes it integrated ... one session for the delivery".
  Marketing: "the roadmap and the delivery reports". Finance: `keeping-the-books` and
  `catervas_read_costs`' by-list drop "sprint"; `forecasting` reads "the last three months";
  `recommending-a-budget` recommends a daily limit only and says "The user sets the daily limit in
  Settings"; the stalled-sprint sentence becomes "A limit set too low stalls the team's work".
  `the_finance_skills_say_what_protects_the_user` pins "The user sets the daily limit in Settings".
- Tools (`crates/runtime/src/tools.rs`): `catervas_write_folder_doc`'s description starts "At a
  delivery's review or retro: write a document of your own folder under docs/catervas/." and keeps
  the rest of what ADR 0061's plan 5 left of it, with "at the sprint review" said "at the review"
  (since ADR 0061 the spec and roadmap are written there as a folder change, not proposed);
  `catervas_append_retro` "Record in team/retro.md what the team should remember from this
  retro."; `catervas_read_costs` "summed by task, agent, day or purpose". (`PIPELINE_DECISION_INSTRUCTION`
  is gone with ADR 0061's plan 5.) Whatever ADR 0061 left of the folder tools' "told at your next
  sprint review" (`tools/folders.rs`) and `human.rs`'s "next sprint review" says "next review".
- Schema descriptions that name sprints and are not about old logs are reworded:
  `command.schema.json` `folderDocDecideBody`; `rpc.schema.json` `filesAskChangeRequest`,
  `filesRewriteRequest`, `marketingBudgetRaiseRequest`; `event.schema.json` `taskCreatedBody`
  (raises and product plan "worked on as filed"), `folderDocWrittenBody`, `folderDocReturnedBody`;
  `team.schema.json` and `kit.schema.json` allowances "each day", `budgets.daily_usd` without the
  sprint sentence; `team-template.schema.json`'s `templateBudgets`.
- CLI: every `///` help line in `crates/cli/src/lib.rs` that says sprint; `init.rs`'s persona text
  keeps "Owns the backlog" (the Product Manager's list); `refusal.rs`'s words list the fields the
  store owns without "sprint". README: the Scrum Master's row "Triages requests, assigns ready work,
  and runs each delivery's review and retro."; the `catervas sprint` row goes; `catervas metrics`
  "The harness metrics for the project". CONTRIBUTING has no sprint word (checked).

## File map

```
crates/roles/roles/scrum_master/{system.md,role.yaml,kit.yaml}, skills/{running-ceremonies,writing-escalation-digests,keeping-work-flowing,catervafying-the-repository}/SKILL.md   modifies (Task 1)
crates/roles/roles/scrum_master/skills/planning-a-sprint/          deletes (Task 1)
crates/roles/src/{lib,kit}.rs, crates/core/src/folders.rs          modifies: embeds, kit tests, the plain name (Task 1)
crates/roles/roles/{product_manager,architect,marketing_specialist,finance_specialist}/**   modifies (Task 1)
crates/runtime/src/{tools,prompt}.rs, tools/folders.rs, orchestrator/human.rs, docs/schemas/{command,rpc,event,team,kit,team-template}.schema.json   modifies (Task 2)
crates/cli/src/{lib,refusal}.rs, README.md                         modifies; lib.rs tests (Task 3)
```

## Interfaces

Consumes: `BUILTIN` role texts and kits as `crates/roles/src/lib.rs` and `kit.rs` embed them;
`tool_descriptors()` (`crates/runtime/src/tools.rs`); the CLI's clap `Cli`. Produces nothing new.

## Tasks

### Task 1: the roles

Files as the map's Task 1 (one task: the guard test below reads every role). `kit.rs`'s
`scrum_master_kit_carries_its_skills` expects `["running-ceremonies", "writing-escalation-digests",
"catervafying-the-repository"]` and `every_owner_carries_catervafying_the_repository` the Scrum
Master's count 3; `lib.rs`'s folder-doc test keeps its cadence sentence; `folders.rs`'s
`names_the_known_documents_and_the_rest_plainly` expects "Delivery rhythm";
`the_product_manager_keeps_the_changelog` pins the new sentence's start, "Each time a delivery
closes, Catervas files a docs task of yours whose `allowed_paths` name `CHANGELOG.md`", in both texts.

- `no_role_text_speaks_of_sprints_or_standups` (`crates/roles/src/lib.rs` `mod tests`) — for every
  shipped role, its `system.md`, `role.yaml` and `kit.yaml`, and every skill a role or kit embeds:
  no match of `(?i)sprint`, `(?i)standup` or "Start sprint". Watched to fail before the words change.

- [x] `docs(roles): no role speaks of sprints; the Scrum Master runs each delivery's review and retro`

### Task 2: tools, prompts and schema words

Files as the map's Task 2. `lists_every_tool_with_its_tier` keeps passing (names unchanged).

- `no_tool_or_instruction_speaks_of_sprints` (`crates/runtime/src/tools.rs` `mod tests`) — no
  `tool_descriptors()` description and no `CEREMONY_INSTRUCTIONS` text matches `(?i)sprint` or
  `(?i)standup`.

- [ ] `docs(runtime): tools and prompts speak of deliveries`

### Task 3: help, README, and what is left

Files as the map's Task 3.

- `the_help_never_mentions_a_sprint` (`crates/cli/src/lib.rs`, a new `#[cfg(test)] mod tests`,
  since `Cli` is private) — walking `Cli::command()` (`clap::CommandFactory`) and every subcommand
  under it, at every depth, no command's `render_long_help()` matches `(?i)sprint`.

Then run the search below and put its file list in the commit body, each file with why its
mention stays. Expected categories, and nothing else: the protocol's event kinds, bodies and
threads (`crates/protocol/**`, `docs/schemas/event.schema.json`); the store's reading of old sprint
events and its migrations (`crates/store/src/projections.rs`, `migrations/0008`–`0010`, `0015`,
and the comments of `0002` and `0004`, which are never edited; `migrations.rs`, the two replay
tests); old proposals' `sprint_id` (`crates/store/src/folder_docs.rs`, `rpc.schema.json`'s
folder-doc proposal card, and the web's reading of it if plan 4 kept one); old keys and contract
fields (`docs/schemas/team.schema.json`'s `plan_in_sprints` and `ambient_messages_per_sprint`,
`task-contract.schema.json`'s `sprint`, `crates/core/src/team.rs`'s `without_old_keys`,
`template.rs`'s strip, `governor/gates.rs`'s `FIELDS_AFTER_FREEZE` and
`FIELDS_THE_GOVERNOR_WRITES`, `crates/core/src/contract/fixtures.rs`); the first delivery's start
(`crates/runtime/src/deliveries.rs`); and the tests that build old logs (plan 5's and plan 3's).
A match outside these is fixed in this task.

- [ ] `docs(cli): no sprint in the help or the README`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
grep -rlis --exclude-dir=node_modules --exclude-dir=dist sprint crates packages apps docs/schemas README.md CONTRIBUTING.md
# expected: only files of the categories in Task 3, each named in its commit body
grep -is sprint CONTRIBUTING.md; echo $?
# expected: 1
```
