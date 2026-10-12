# Standalone: ready-made teams

Status: done
Branch: `feat/predefined-teams` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 1 (goal 1), 3 (team template), 4.1 (the starts), 4.4 (using a saved team), 5.3 (Definition of Ready), 6.6 (the Finance Specialist's mandate), 6.8, 8.4, F1
Depends on: main at 15d67ec2 (pull requests #32 and #35 merged)
Readiness confirmed by: two fresh Opus sessions, 2026-10-10 and 2026-10-11 (two rounds, against `docs/standards/workflow.md` stage 2: the first found five decisions unmade, closed below; the second found none, and both rounds' other findings are carried into the tasks)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The founder approved `docs/design/predefined-teams.md` on 2026-10-10; it is the source of truth for behavior and this plan does not restate its copy. Done means: setup's "Your team" offers a fourth start, "A ready-made team", listing Software product, Online store and Financial analysis, each filling the rows as a saved team does with its answers carried; the Team page's "Use a saved team" lists the three under "Ready-made" above the saved teams; a team needs an active Product Manager and nothing else, in the editor, in setup, in a template, and when pausing or retiring someone, and a contract whose assignee role nobody active holds is refused readiness naming the role; the Product Manager sees each agent's persona beside its id when it assigns a task, so the three desks are told apart; "From scratch" keeps the Product Manager's row fixed and lets the Developer's be unticked; "Add someone" works on a team without a Developer; the Finance Specialist's mandate covers a business's money. Out of scope: a starting brief, a folder per Finance Specialist, projects that are not repositories, the DevOps Engineer, editing the built-ins.

## Decisions

- A team needs an active Product Manager alone: ADR 0055, amending ADR 0040 G5's timing. `REQUIRED_ROLES` keeps its shape with one row; its message and `needs_product_manager` are unchanged, and `needs_developer` is deleted from `code_of`, the `teamError` enum of `docs/schemas/rpc.schema.json`, `WORDS` and `MISSING` with its string, all in Task 1 so that the suite is green after it. `leaves_a_gap` (`orchestrator/human.rs`) refuses `last_of_role` for the Product Manager alone: `Role::SoftwareDeveloper` leaves its match. The descriptions that say every team has a Developer (`team.schema.json`'s `agents`, `team-template.schema.json`'s `agents`, the comments at `team.rs` `REQUIRED_ROLES` and `validate_team`) are reworded in Task 1. Rejected: keeping the Developer row behind a flag (two rules for one fact).
- The readiness rule is `AssigneeAvailable`, the variant placed right after `ReviewerAvailable` in `ReadinessRule` (the enum's order is the failures' order) and the check right after `reviewer_available` in `CHECKS` (23): `active_agents_by_role[assignee_role] >= 1`, exempting `Role::Human` (the schema allows it; the reviewer rule already refuses a human assignee of a task), message `nobody can take this task: the team has no active {plain_role}; add one on the Team page`. Its plain sentence (`governor/plain.rs`, which also lists it in `EVERY_RULE`, 25, and `listed`): "Nobody on the team has the role this task is for. Add one on the Team page." `fixtures::a_ready_context` gains one Finance Specialist, one Procurement Specialist and one UI/UX Designer at 1, so the finance, procurement and designer fixtures stay ready; `lets_the_human_review_an_epic_but_not_a_task` keeps its map rather than clearing it. `daemon/gates.rs`'s rule counts (23 and 25) become 24 and 26. Rejected: G5's `needs_a_builder` by contract kind and criteria (the assignee rule covers every role; a code task is a Developer's or a Designer's already by `DocumentPathsOnly`).
- The refused-preview scenario, everywhere "no active Developer" served it (`team/template.rs` `refuses_when_a_kept_match_is_paused` and `NEEDS_DEVELOPER`, `daemon/templates.rs`'s three tests, `team.test.tsx` `disables_use_when_the_result_is_refused`, `e2e/templates.spec.ts` section 4, spec 4.4's example): the project team has two Product Managers, `mira` paused and `pat` active, who has worked; the template holds `mira` as Product Manager and `theo`; applying keeps `mira` paused and retires `pat`, so no Product Manager is active and the result is refused `needs_product_manager`; the preview names Mira as paused and says to resume her first. Rejected: dropping the checks (the rule they hold still exists).
- The Product Manager tells desks apart by persona: `plan_message` (`orchestrator/messages.rs`) lists each assignee and reviewer as `id (persona)` when the agent has a persona, `id` alone otherwise; its callers pass the team. Rejected: a tool that reads personas (one line in a message the planner already reads).
- The three teams are files under `crates/roles/teams/`, embedded as `BUILTIN_TEAMS: [(&str, &str); 3]` (slug, yaml) in `catervas-roles`, which already embeds every role as data; parsing stays in `catervas-runtime::templates`, beside the folder's reader, with `catervas_store::files::yaml_value` and `validate_template`. Rejected: a `description` field in the template format (one more schema and typify change for three lines the page words anyway, by slug, as it words each role's job).
- Slugs are `software-product`, `online-store`, `financial-analysis`; names "Software product", "Online store", "Financial analysis"; `saved_at` `2026-10-10T00:00:00Z` in every file (the schema requires it; the page shows no date for a built-in). Agents, in order, each with `id`, `display_name`, `role`, `avatar`, `persona` and no `model` (each takes its role's shipped model where the template is used, `effective`'s `load_role`):
  - Software product: `mira` Mira `product_manager` `product-manager` "Asks the questions that decide what to build"; `ada` Ada `architect` `architect` "Thinks about how it all fits together"; `theo` Theo `software_developer` `developer` "Builds it and tests it"; `kai` Kai `marketing_specialist` `marketing-specialist` "Owns your brand and how you reach people".
  - Online store: `mira` Mira `product_manager` as above; `noor` Noor `procurement_specialist` `extra-5` "Finds the best seller at the right price"; `lena` Lena `finance_specialist` `finance-specialist` "Keeps your numbers straight"; `kai` Kai `marketing_specialist` as above.
  - Financial analysis: `mira` Mira `product_manager` as above; `noor` Noor `finance_specialist` `finance-specialist` "Portfolio desk: what you hold, how it is spread and how it has done"; `ivo` Ivo `finance_specialist` `finance-specialist` "Risk desk: what could go wrong, how likely, and how much it would cost"; `lena` Lena `finance_specialist` `finance-specialist` "Opportunity desk: what could be gained, at what price and what risk".
  - Every file: `policy: { permissions: { run_commands: true, push: false }, judgment: { required: always, judge: auto }, integration: manual }` and `budgets: {}`. No `plan_in_sprints` (setup plans in sprints; applying keeps the project's).
- `templates.list` answers `{ folder: <string|null>, builtin: [{ slug, template }], templates: [...], unreadable: [...] }`, `templatesListResult` in `rpc.schema.json` changed to match (`builtin` required, `folder` `["string", "null"]`): without a state folder `folder` is `null`, `templates` and `unreadable` empty, and the query no longer refuses `no_state_folder`. A saved file whose slug is a built-in's (saved before this change) is listed under `unreadable`, not `templates`, so Settings offers to delete it, and preview and apply by that slug reach the built-in. `template.preview` and `template.apply` look a built-in up by slug before the folder, so both work without a folder for a built-in; a saved slug without a folder is refused `no_state_folder` as before. `template.save`, `template.rename` and `template.delete` refuse `no_state_folder` as before. So `applying` and `apply` take `templates: Option<&Templates>`, their callers (`query`, `call`, `pause_before_dropping_google_ads`) passing `web_of(state)?.templates.as_ref()`, and `call` resolves `templates_of` only for save, rename and delete. The digest of a built-in is the sha256 of its embedded text, as a file's is of its bytes. Rejected: a query of its own for the built-ins (two lists for one picker); hiding the built-in when a file holds its slug (the built-in is the one the page offered).
- `TemplateError::Builtin { name }` is refused by `save` (whatever `replace` says) and `rename` when the name's slug is a built-in's, answered `template_builtin` at `/name` (named in the save, rename and list descriptions of `rpc.schema.json`), message "Catervas comes with a team called {name}. Choose another name." with the built-in's name; the page words it from the code, filling `{name}` with the name the user typed. `rename` and `delete` of a built-in slug answer `NotFound` as today (no file). Rejected: reusing `template_exists` (its words say "saved team").
- The page: `Start` gains `"builtin"`; `teamDraftOf` fills it exactly as `saved` (`from`, the carried answers, members from the template's agents, `on` by availability), so the wizard's `carried` and the finish screen's "What they may do, from <name>" need no change. `SetupTeam` keeps one chosen slug per start (`builtin` and `saved` each defaulting to that start's first option), so switching starts never carries the other's slug. `noFolder` is read from the listing (`folder === null`), not from an error, with the signature `noFolder(listing: Listing | undefined): boolean`; `useTemplates` keeps its shape `{ listing, noFolder, again }`. "Add someone" bases its newcomer on `proposed.team.agents`' Developer. From scratch: the Developer's row has a tick box (the Product Manager's row has none), `unnamed` counts ticked team rows only, and Continue is also held while fewer than two rows are ticked, both worded by `teamNameAll`; the Product Manager's row says `teamScratchNote`, and the Developer's its job (`JOBS`), as any other row. Setup from scratch with the Developer unticked still asks both permission questions and may show `mayPushNoteMany`: accepted. Rejected: a fourth start inside the saved picker (a non-technical user looks for "a team for my business" first).
- "Use a saved team" lists two groups sharing one selection and one radio `name`: a `Choice` with legend `useBuiltinList` ("Ready-made") holding the three, then a `Choice` with legend `useList` ("Saved teams") holding the saved; with none saved, the second group is a `useList` heading over the existing no-saved line, and with `folder` null over `templateNoFolder`. The dialog's default slug and the preview title's name look in `builtin` and `templates` both, the default being the first built-in. A built-in's description is `builtinMeta` ("{names} · comes with Catervas"). The copy written for saved teams shows for a built-in too, on purpose, but `templateResume` and `templateChooseAnother` lose "saved" ("choose another team"). Settings' "Saved teams" lists saved teams only. Rejected: one `Choice` with a heading among its options (`Choice` has one legend).
- The words, all in `en.ts`: `startsLegend` "Start from the team Catervas suggests, from a ready-made team, from a team you saved in another project, or from nothing. Whichever you pick, you can change anyone below."; `startBuiltin` "A ready-made team"; `startBuiltinNote` "Teams Catervas comes with, one for each purpose. Pick one, then change anyone below."; `startWhichBuiltin` "Which ready-made team?"; `builtinSoftwareProduct` "Plans, designs, builds and markets a software product."; `builtinOnlineStore` "Plans the shop, finds and orders stock, keeps the books and markets it."; `builtinFinancialAnalysis` "Three analysts, one desk each, working one task at a time: your portfolio, its risks and its opportunities, managed by a Product Manager."; `builtinMeta` "{names} · comes with Catervas"; `startScratchNote` "Start with a Product Manager and a Developer, keep or untick the Developer, and add whoever else you want."; `teamLeadScratch` "Every team needs a Product Manager to plan. Name them, then add whoever else you want."; `teamNameAll` "Name the Product Manager and at least one more person to continue." (replacing `teamNameBoth`); `templateBuiltin` "Catervas comes with a team called {name}. Choose another name."; `useBuiltinList` "Ready-made"; `useLead` "Pick a ready-made team or one you saved. Catervas shows you who stays, who joins and who leaves before anything changes."; `templateResume` "Resume {paused} on the Team page first, or choose another team."; `templateChooseAnother` "Choose another team."; `teamPageLead` "{count} agents work on {name}. A team needs a Product Manager; everyone else is up to you. You can have up to seven." (its "and a Developer" dropped); `refuseNeedsDeveloper` deleted. Settings' rename refusal fills `template_builtin`'s `{name}` with the name typed, as `SaveTemplate` already does. An unknown built-in slug shows `builtinMeta` alone.
- The Finance Specialist's mandate (ADR 0055 decision 4): `role.yaml`'s `mandate` becomes "Keep track of the business's money: what it owns, spends, earns and has at stake. For a software product, start with the team's own AI spending, then its other costs and its revenue; for any business, keep its books, forecast them, and do the analysis a task asks for, a portfolio's holdings, a risk or an opportunity among them. Record the numbers, forecast them, and recommend budgets in plain words." followed by its two existing guard sentences unchanged ("Your numbers are management accounting…" and "Every number names its source."); `system.md`'s "Your mandate" first paragraph becomes the same new text plus "Use your network access to check prices and plans before you forecast.", its guard paragraph unchanged. Skills are not touched. Rejected: a new skill per desk (the analysis is a task's; the kit check is phase 9's).
- Spec revision 0.90; project plan revision 42; the ADRs amended by 0055 (0040, 0026, 0019) each gain an "Amended 2026-10-10 by ADR 0055" line under their status. Draft pull request #36 (`feat/setup-friction`) also claims 0.87 and ADR 0054 and touches `SetupTeam.tsx`, `TeamSetup.tsx`, `en.ts`, core `team.rs` and daemon `team.rs`: whichever of the two merges second rebases, renumbers its ADR and its spec revision, and resolves those files; this plan's line is updated then, as ADR 0053 was.

## File map

Two tasks touch `crates/runtime/src/daemon/templates.rs`, `docs/schemas/rpc.schema.json` and `apps/web/src/strings/en.ts`, each for the reason its line says.

```
crates/core/src/team.rs                                  modifies: REQUIRED_ROLES and its comments, mod tests (Task 1)
crates/core/src/team/template.rs                         tests:   the Developer rule, the paused-Product-Manager refusal (Task 1)
crates/core/src/governor/readiness.rs                    modifies: ReadinessRule::AssigneeAvailable, CHECKS, assignee_available, mod tests (Task 1)
crates/core/src/governor/readiness/fixtures.rs           modifies: a_ready_context's roles (Task 1)
crates/core/src/governor/plain.rs                        modifies: the sentence, EVERY_RULE, listed (Task 1)
crates/runtime/src/orchestrator/human.rs                 modifies: leaves_a_gap, mod tests (Task 1)
crates/runtime/src/daemon/team.rs                        modifies: code_of loses needs_developer, its tests (Task 1)
crates/runtime/src/daemon/templates.rs                   tests:   needs_developer → the paused Product Manager (Task 1); modifies: the list, applying, call, failure, mod tests (Task 3)
crates/runtime/src/daemon/gates.rs                       tests:   the rule counts (Task 1)
docs/schemas/rpc.schema.json                             modifies: teamError loses needs_developer (Task 1); templatesListResult, the template descriptions (Task 3)
docs/schemas/team.schema.json, team-template.schema.json modifies: the agents descriptions (Task 1)
crates/roles/teams/software-product.yaml                 creates: the team (Task 2)
crates/roles/teams/online-store.yaml                     creates: the team (Task 2)
crates/roles/teams/financial-analysis.yaml               creates: the team (Task 2)
crates/roles/src/lib.rs                                  modifies: BUILTIN_TEAMS, mod tests (Task 2)
crates/roles/roles/finance_specialist/role.yaml          modifies: mandate (Task 2)
crates/roles/roles/finance_specialist/system.md          modifies: the mandate paragraph (Task 2)
crates/runtime/src/templates.rs                          modifies: builtins, builtin, is_builtin, TemplateError::Builtin, list, save, rename, mod tests (Task 3)
crates/runtime/src/orchestrator/messages.rs              modifies: plan_message with personas, its callers, mod tests (Task 3)
apps/web/src/pages/SavedTeams.tsx                        modifies: Listing, useTemplates, noFolder, builtinMeta (Task 4)
apps/web/src/pages/setup/TeamSetup.tsx                   modifies: Start, teamDraftOf (Task 4)
apps/web/src/pages/setup/SetupTeam.tsx                   modifies: the fourth start, the slug per start, add, the scratch rows (Task 4)
apps/web/src/app/refusals.ts                             modifies: WORDS (Task 4)
apps/web/src/strings/en.ts                               modifies: Task 4's keys; Task 5's keys
apps/web/src/pages/setup/team.test.tsx                   tests:   Task 4
apps/web/src/pages/dialogs/UseTemplate.tsx               modifies: the two groups, MISSING (Task 5)
apps/web/src/pages/team.test.tsx                         tests:   Task 5 (the dialog, Settings' saved teams)
apps/web/e2e/templates.spec.ts                           modifies: section 4's scenario (Task 5)
docs/SPEC.md, docs/plans/project-plan.md, docs/decisions/0040-*.md, 0026-*.md, 0019-*.md, this plan   (Task 6)
```

## Interfaces

Consumes (all on `main`): `validate_team`, `validate_template`, `TeamTemplate`, `template_slug`, `plain_role` (`catervas-core`); `ReadinessContext.active_agents_by_role: BTreeMap<Role, u32>` (`catervas-core::governor::readiness`); `yaml_value(text: &str, named: &str) -> Result<Value, FilesError>` (`catervas-store::files`); `Templates`, `TemplateError`, `TemplateListing` (`catervas-runtime::templates`); `suggested`, `code_of` (`catervas-runtime::daemon::team`); `Choice`, `Faces`, `templateMeta`, `useTemplates`, `draftOf`, `someone` (web).

Produces:

```rust
// catervas-roles
pub const BUILTIN_TEAMS: [(&str, &str); 3];                 // (slug, yaml text), in the order the page lists them
// catervas-runtime::templates
pub fn builtins() -> Vec<(String, TeamTemplate)>;           // every shipped team, in BUILTIN_TEAMS order
pub fn builtin(slug: &str) -> Option<(TeamTemplate, String)>; // the template and its digest
pub fn is_builtin(slug: &str) -> bool;
pub enum TemplateError { /* as today, plus */ Builtin { name: String } }
// catervas-runtime::orchestrator::messages
pub(super) fn plan_message(contract: &TaskContract, team: &Team, assignees: &[String], reviewers: &[String]) -> String;
// catervas-core::governor::readiness
pub enum ReadinessRule { /* as today, plus, after ReviewerAvailable */ AssigneeAvailable }
```

```ts
// apps/web/src/pages/setup/TeamSetup.tsx
export type Start = "suggested" | "builtin" | "saved" | "scratch";
// apps/web/src/pages/SavedTeams.tsx
export type Listing = { folder: string | null; builtin: { slug: string; template: Template }[]; templates: { slug: string; template: Template }[]; unreadable: { slug: string }[] };
export function noFolder(listing: Listing | undefined): boolean;
export function builtinMeta(template: Template): string;
```

Wire: `templates.list` → `{ folder: string | null, builtin: [{ slug, template }], templates: [{ slug, template }], unreadable: [{ slug }] }`; refusal `template_builtin` at `/name`.

## Tasks

### Task 1: a team needs a Product Manager alone

Files: modified `crates/core/src/team.rs`, `crates/core/src/team/template.rs` (tests), `crates/core/src/governor/readiness.rs`, `readiness/fixtures.rs`, `governor/plain.rs`, `crates/runtime/src/orchestrator/human.rs`, `crates/runtime/src/daemon/team.rs`, `crates/runtime/src/daemon/templates.rs` (tests only), `crates/runtime/src/daemon/gates.rs` (tests only), `docs/schemas/rpc.schema.json` (`teamError`), `docs/schemas/team.schema.json` and `team-template.schema.json` (descriptions). The whole Rust suite runs green at this task's end.

- `accepts_a_team_without_a_developer` (team.rs) — a team of an active Product Manager and an active Architect passes `validate_team`.
- `refuses_a_team_without_a_product_manager` (team.rs, replacing the test that expected both messages) — a team of an Architect and a Scrum Master is refused with exactly one message, "A team needs an active Product Manager to write its plans, and this one has none."
- `reports_a_repeated_id_and_a_missing_role_together` (team.rs, adjusted) — two agents `ada` as Architect and Scrum Master: two refusals, the id's and the Product Manager's.
- `accepts_a_template_without_a_developer` (template.rs, replacing `refuses_a_template_without_a_developer`) — a template of a Product Manager and a Marketing Specialist passes `validate_template`.
- `refuses_when_a_kept_match_is_paused` (template.rs, rewritten to the paused-Product-Manager scenario of the Decisions) — the result's errors hold `needs_product_manager`'s message at `/agents`.
- `refuses_a_contract_whose_assignee_role_nobody_holds` (readiness.rs) — the fixture contract for a `software_developer` in a context whose map has no Developer and two Architects fails `AssigneeAvailable` with the message naming "Software Developer"; with one Developer it passes that rule; a contract assigned to `human` does not fail it.
- `the_assignee_rule_comes_after_the_reviewers` (readiness.rs) — no Developer and no Architect: `failed_rules` is `[ReviewerAvailable, AssigneeAvailable]`, in that order.
- `plain_readiness_covers_every_rule` (plain.rs, the existing test over `EVERY_RULE`, extended) — `AssigneeAvailable` is listed and has its sentence.
- `retires_the_last_developer` (human.rs) — retiring the only Developer of a team with an active Product Manager is not refused; pausing the only Product Manager still is, with `last_of_role`.
- The daemon's tests that expected `needs_developer` (`daemon/team.rs` `3891`-`3898`, `daemon/templates.rs` `refuses_in_plain_words`, `checks_again_when_applying_a_preview_gone_stale`, `works_the_result_out_again_under_the_lock`) use the paused-Product-Manager scenario and expect `needs_product_manager`; `daemon/gates.rs`'s counts become 24 and 26.

- [x] `feat(core): a team needs an active Product Manager alone, and readiness needs the assignee's role`

### Task 2: the three teams and the Finance Specialist's mandate

Files: created `crates/roles/teams/software-product.yaml`, `online-store.yaml`, `financial-analysis.yaml` (the Decisions' content); modified `crates/roles/src/lib.rs` (`BUILTIN_TEAMS`, `mod tests`), `crates/roles/roles/finance_specialist/role.yaml`, `system.md`.

- `ships_three_teams_with_their_slugs` — `BUILTIN_TEAMS` has the three slugs in order, and each yaml's `name:` is the Decisions'.
- `holds_every_shipped_team_to_the_template_schema` — each yaml, through `serde_saphyr` to a `Value`, passes `validate_template`; the Financial analysis one has three `finance_specialist` agents and no `software_developer`.
- `the_finance_specialist_keeps_its_guards` — `load_role(FinanceSpecialist)`'s mandate contains "what it owns, spends, earns and has at stake", "not a tax filing, statutory accounts or financial advice" and "Every number names its source", and the system prompt contains "has at stake".

- [x] `feat(roles): ship three ready-made teams, and widen the Finance Specialist's mandate to a business's money`

### Task 3: built-ins on the wire, and desks in the plan message

Files: modified `crates/runtime/src/templates.rs`, `crates/runtime/src/daemon/templates.rs` (the list, `applying`, `apply`, `pause_before_dropping_google_ads`, `call`, `failure`, `mod tests`), `crates/runtime/src/orchestrator/messages.rs` (`plan_message`, its callers, `mod tests`), `docs/schemas/rpc.schema.json` (`templatesListResult`, the template descriptions).

- `parses_every_builtin_with_a_stable_digest` (templates.rs) — `builtins()` has three in `BUILTIN_TEAMS` order; `builtin("online-store")` answers the Online store with a 64-hex digest equal to sha256 of the embedded text; `builtin("mine")` is `None`; `is_builtin` true for the three only.
- `refuses_saving_or_renaming_to_a_builtins_name` (templates.rs) — `save` of a template named "Online Store" (slug `online-store`) with `replace` true and false, and `rename` of a saved slug to "financial analysis", each answer `Builtin { name }` with the built-in's name, and no file is written.
- `lists_a_file_under_a_builtins_slug_as_unreadable` (templates.rs) — a folder holding `online-store.yaml`, a valid template: `list` answers it under `unreadable`, not `templates`.
- `lists_the_builtins_with_the_saved_and_without_a_folder` (daemon) — served with a folder holding one saved team: `builtin` has three in order, `templates` the one, `folder` the path, the answer held to `templatesListResult`; served with `None`: `folder` null, `builtin` three, `templates` and `unreadable` empty, no refusal.
- `previews_and_applies_a_builtin` (daemon) — `template.preview { slug: "financial-analysis" }` answers a team of Mira and three Finance Specialists with `digest` equal to `builtin`'s; `template.apply` with that digest writes the team, records `team.updated` with `template: "Financial analysis"`, and answers the same; both work when served with no folder.
- `refuses_a_builtin_name_as_template_builtin` (daemon) — `template.save { name: "online store" }` is refused -32005 with code `template_builtin` at `/name`; so is `template.rename` to it.
- `a_saved_slug_without_a_folder_is_still_refused` (daemon) — served with `None`, `template.preview { slug: "mine" }` and `template.save` are refused `no_state_folder`; `refuses_in_plain_words`'s no-folder loop drops `templates.list`.
- `names_each_desk_beside_its_id` (messages.rs) — a team of Mira and the three desks, a finance contract: the message lists `noor (Portfolio desk: …)`, `ivo (Risk desk: …)`, `lena (Opportunity desk: …)` and the reviewer `mira (Asks the questions that decide what to build)`; an agent without a persona is its id alone.

- [x] `feat(runtime): list, preview and apply the ready-made teams, reserve their names, and name each desk to the planner`

### Task 4: the fourth start, and from scratch

Files: modified `apps/web/src/pages/SavedTeams.tsx`, `apps/web/src/pages/setup/TeamSetup.tsx`, `apps/web/src/pages/setup/SetupTeam.tsx`, `apps/web/src/app/refusals.ts` (`needs_developer` out, `template_builtin` in), `apps/web/src/strings/en.ts` (the Decisions' keys for setup; `teamNameBoth` and `refuseNeedsDeveloper` deleted); tests in `apps/web/src/pages/setup/team.test.tsx`, whose `starts_from_scratch` and `disables_saved_with_no_state_folder_or_none_saved` change with the behavior (the latter mocks `folder: null`, not a refusal).

- `offers_four_starts` (replacing `offers_three_starts`) — the starts are, in order, the suggested team, a ready-made team, a saved team, from scratch; "A ready-made team" is enabled with `folder` null and no saved team, "A saved team" disabled, and each of the three is listed by name with its line and faces.
- `starts_from_a_ready_made_team` — picking Online store fills four rows, Mira, Noor, Lena, Kai, each ticked, Lena's and Noor's with their pictures; "Filled from Online store" shows; Continue goes to `/setup/spending`; the finish screen lists "What they may do, from Online store".
- `keeps_one_slug_per_start` — picking Financial analysis, then "A saved team", then "A ready-made team" again shows Financial analysis's rows, not the suggested ones.
- `a_ready_made_designer_is_held_off_without_a_sandbox` — with `designer_needs_sandbox`, a ready-made team's Designer row (a test template with one) starts unticked, as a saved team's does.
- `adds_someone_to_a_team_without_a_developer` — from Online store, "Add someone" adds a fifth row from the spare names with an extra picture.
- `from_scratch_needs_a_product_manager_and_one_more` — from scratch: the Product Manager's row has no tick box and says `teamScratchNote`, the Developer's has one and says its job; unticking the Developer with nobody else ticked holds Continue with `teamNameAll`; ticking the Finance Specialist under "More roles" and naming the Product Manager lets Continue go on to `/setup/permissions`.
- `words_the_builtin_refusal_and_forgets_the_developers` — `said("template_builtin", { name })` is "Catervas comes with a team called <name>. Choose another name.", Settings' rename refusal shows it with the name typed, and `said("needs_developer")` is the fallback wording.
- `settings_lists_saved_teams_only` — Settings' "Saved teams" with `folder` null shows `templateNoFolder` and never a built-in's name.

- [x] `feat(web): a ready-made team as setup's fourth start, and from scratch with a Product Manager alone`

### Task 5: the Team page lists the ready-made

Files: modified `apps/web/src/pages/dialogs/UseTemplate.tsx`, `apps/web/src/strings/en.ts` (`useBuiltinList`, `useLead`, `templateResume`, `templateChooseAnother`, `teamPageLead`); tests in `apps/web/src/pages/team.test.tsx`, whose `disables_use_when_the_result_is_refused` takes the paused-Product-Manager scenario; `apps/web/e2e/templates.spec.ts` section 4 takes it with the browser's own means: the second Product Manager is added through the Team page's "Add someone" (so it never works and is removed, not retired), Mira is paused, and the preview says "Your team would have no active Product Manager. Mira stays but is paused. Resume Mira on the Team page first, or choose another team.".

- `lists_the_ready_made_above_the_saved` — "Use a saved team" shows "Ready-made" with the three, then "Saved teams" with the saved; the first option is Software product; a built-in's line is its names and "comes with Catervas"; choosing a saved one unselects the built-in.
- `previews_and_uses_a_ready_made_team` — "Show what changes" for Financial analysis calls `template.preview { slug: "financial-analysis" }` and "Use this team" calls `template.apply` with its digest.
- `opens_with_no_saved_team_and_no_folder` — with `folder` null and no saved teams, the dialog lists the three and, under a "Saved teams" heading, `templateNoFolder`.
- `says_the_team_needs_a_product_manager_alone` — the Team page's lead no longer says "and a Developer".
- `refusedLine_knows_no_developer_rule` — a preview refusal `needs_product_manager` names the Product Manager; any other code falls back to `said(code)` and "Choose another team."

- [x] `feat(web): list the ready-made teams on the Team page`

### Task 6: docs

Files: `docs/SPEC.md` (1 goal 1 and the non-goal line; 3, the template paragraph gains the built-ins and the `init` sentence loses "the two a team cannot work without"; 4.1, four starts; 4.4, the two groups and the paused-Product-Manager example; 5.3, the assignee bullet after the reviewer's; 6.6, the mandate; 6.8, the "every team has a Developer" clause; 8.4, built-ins in the binary; F1, enforce a Product Manager, the template rule; the revision line 0.90), `docs/plans/project-plan.md` (D18's line, phase 14 steps 03 and 05's notes, revision 42), `docs/decisions/0040-*.md`, `0026-*.md`, `0019-*.md` (one "Amended 2026-10-10 by ADR 0055" line each), this plan's status to done.

- [x] `docs(spec): ready-made teams, and a team needs a Product Manager alone (0.89)`

## Verification

```
cargo xtask check
# expected: xtask check: ok   (Rust and the front end's pnpm check)
cargo xtask check --integration
# expected: xtask check: ok
```

Then the founder, in `catervas serve` on a fresh folder, picks "A ready-made team", Online store, names nobody differently, continues to Start the team, files a request, and sees the Product Manager plan it; then on the Team page uses Financial analysis and sees the preview keep Mira and Lena (same id and role), retire or remove Noor the Procurement Specialist and Kai, and add Noor the portfolio desk as `noor-2` and Ivo.
