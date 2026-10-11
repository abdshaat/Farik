# Standalone: ready-made teams

Status: draft
Branch: `feat/predefined-teams` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 1 (goal 1), 3 (team template), 4.1 (the starts), 4.4 (using a saved team), 5.3 (Definition of Ready), 6.6 (the Finance Specialist's mandate), 6.8, 8.4, F1
Depends on: main at 15d67ec2 (pull requests #32 and #35 merged)
Readiness confirmed by: pending

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

The founder approved `docs/design/predefined-teams.md` on 2026-10-10; it is the source of truth for behavior and this plan does not restate its copy. Done means: setup's "Your team" offers a fourth start, "A ready-made team", listing Software product, Online store and Financial analysis, each filling the rows as a saved team does with its answers carried; the Team page's "Use a saved team" lists the three under "Ready-made" above the saved teams; a team needs an active Product Manager and nothing else, in the editor, in setup and in a template, and a contract whose assignee role nobody active holds is refused readiness naming the role; "From scratch" keeps the Product Manager's row fixed and lets the Developer's be unticked; "Add someone" works on a team without a Developer; the Finance Specialist's mandate covers a business's money. Out of scope: a starting brief, a folder per Finance Specialist, projects that are not repositories, the DevOps Engineer, editing the built-ins.

## Decisions

- A team needs an active Product Manager alone: ADR 0055, amending ADR 0040 G5's timing. `REQUIRED_ROLES` keeps its shape with one row; its message and `needs_product_manager` are unchanged, and `needs_developer` is deleted from `code_of`, `WORDS` and `MISSING` with its string. Rejected: keeping the Developer row in `REQUIRED_ROLES` behind a flag (two rules for one fact).
- The readiness rule is `AssigneeAvailable`, checked right after `ReviewerAvailable` (`CHECKS` becomes 23): `active_agents_by_role[assignee_role] >= 1`, message `nobody can take this task: the team has no active {plain_role}; add one on the Team page`. Rejected: G5's `needs_a_builder` by contract kind and criteria (the assignee rule covers every role, and a code task is a Developer's or a Designer's already by `DocumentPathsOnly`).
- The three teams are files under `crates/roles/teams/`, embedded as `BUILTIN_TEAMS: [(&str, &str); 3]` (slug, yaml) in `catervas-roles`, which already embeds every role as data; parsing stays in `catervas-runtime::templates`, beside the folder's reader, with `catervas_store::files::yaml_value` and `validate_template`. Rejected: a `description` field in the template format (one more schema and typify change for three lines the page words anyway, by slug, as it words each role's job).
- Slugs are `software-product`, `online-store`, `financial-analysis`; names "Software product", "Online store", "Financial analysis"; `saved_at` `2026-10-10T00:00:00Z` in every file (the schema requires it; the page shows no date for a built-in). Agents, in order, each with `id`, `display_name`, `role`, `avatar`, `persona` and no `model` (each takes its role's shipped model where the template is used, `effective`'s `load_role`):
  - Software product: `mira` Mira `product_manager` `product-manager` "Asks the questions that decide what to build"; `ada` Ada `architect` `architect` "Thinks about how it all fits together"; `theo` Theo `software_developer` `developer` "Builds it and tests it"; `kai` Kai `marketing_specialist` `marketing-specialist` "Owns your brand and how you reach people".
  - Online store: `mira` Mira `product_manager` as above; `noor` Noor `procurement_specialist` `extra-5` "Finds the best seller at the right price"; `lena` Lena `finance_specialist` `finance-specialist` "Keeps your numbers straight"; `kai` Kai `marketing_specialist` as above.
  - Financial analysis: `mira` Mira `product_manager` as above; `noor` Noor `finance_specialist` `finance-specialist` "Portfolio desk: what you hold, how it is spread and how it has done"; `ivo` Ivo `finance_specialist` `finance-specialist` "Risk desk: what could go wrong, how likely, and how much it would cost"; `lena` Lena `finance_specialist` `finance-specialist` "Opportunity desk: what could be gained, at what price and what risk".
  - Every file: `policy: { permissions: { run_commands: true, push: false }, judgment: { required: always, judge: auto }, integration: manual }` and `budgets: {}`. No `plan_in_sprints` (setup plans in sprints; applying keeps the project's).
- `templates.list` answers `{ folder: <string|null>, builtin: [{ slug, template }], templates: [...], unreadable: [...] }`: without a state folder `folder` is `null`, `templates` and `unreadable` empty, and the query no longer refuses `no_state_folder`. `template.preview` and `template.apply` look a built-in up by slug before the folder, so both work without a folder for a built-in; a saved slug without a folder is refused `no_state_folder` as before. `template.save`, `template.rename` and `template.delete` refuse `no_state_folder` as before. `call` therefore resolves `templates_of` only for those three; `applying` and `pause_before_dropping_google_ads` take `Option<&Templates>`. The digest of a built-in is the sha256 of its embedded text, as a file's is of its bytes. Rejected: a query of its own for the built-ins (two lists for one picker).
- `TemplateError::Builtin { name }` is refused by `save` (whatever `replace` says) and `rename` when the name's slug is a built-in's, answered `template_builtin` at `/name`, message "Catervas comes with a team called {name}. Choose another name."; the page words it from the code. `rename` and `delete` of a built-in slug answer `NotFound` as today (no file). Rejected: reusing `template_exists` (its words say "saved team").
- The page: `Start` gains `"builtin"`; `teamDraftOf` fills it exactly as `saved` (`from`, the carried answers, members from the template's agents, `on` by availability), so the wizard's `carried` and the finish screen's "What they may do, from <name>" need no change. `noFolder` is read from the listing (`folder === null`), not from an error; `useTemplates` keeps its shape `{ listing, noFolder, again }`. "Add someone" bases its newcomer on `proposed.team.agents`' Developer. From scratch: the Developer's row has a tick box (the Product Manager's row has none), `unnamed` counts ticked team rows only, and Continue is also held while fewer than two rows are ticked, both worded by `teamNameAll`. "Use a saved team" lists `builtin` under a heading `useBuiltinList` ("Ready-made") then the saved under `useList`, one `Choice` with the built-ins first; a built-in's description is `builtinMeta` ("{names} · comes with Catervas"). Settings' "Saved teams" lists saved teams only. Rejected: a fourth start inside the saved picker (a non-technical user looks for "a team for my business" first).
- The three lines on the start, in `en.ts` by slug (`builtinSoftwareProduct`, `builtinOnlineStore`, `builtinFinancialAnalysis`): "Plans, designs, builds and markets a software product."; "Plans the shop, finds and orders stock, keeps the books and markets it."; "Three analysts, one desk each: your portfolio, its risks and its opportunities, managed by a Product Manager.". An unknown slug shows `builtinMeta` alone.
- The Finance Specialist's mandate (ADR 0055 decision 4): `role.yaml`'s `mandate` and `system.md`'s "Your mandate" first paragraph become "Keep track of the business's money: what it owns, spends, earns and has at stake. For a software product, start with the team's own AI spending, then its other costs and its revenue; for any business, keep its books, forecast them, and do the analysis a task asks for, a portfolio's holdings, a risk or an opportunity among them. Record the numbers, forecast them, and recommend budgets in plain words. Use your network access to check prices and plans before you forecast." The guard paragraph is unchanged. Skills are not touched. Rejected: a new skill per desk (the analysis is a task's; the kit check is phase 9's).
- Spec revision 0.87; project plan revision 42; the ADRs amended by 0055 (0040, 0026, 0019) each gain an "Amended ... by ADR 0055" line under their status. A number taken by another branch when this one merges is renumbered then, as ADR 0053 was.

## File map

```
crates/core/src/team.rs                                  modifies: REQUIRED_ROLES, mod tests (Task 1)
crates/core/src/team/template.rs                         tests:   the Developer rule (Task 1)
crates/core/src/governor/readiness.rs                    modifies: ReadinessRule::AssigneeAvailable, CHECKS, assignee_available, mod tests (Task 1)
crates/roles/teams/software-product.yaml                 creates: the team (Task 2)
crates/roles/teams/online-store.yaml                     creates: the team (Task 2)
crates/roles/teams/financial-analysis.yaml               creates: the team (Task 2)
crates/roles/src/lib.rs                                  modifies: BUILTIN_TEAMS, mod tests (Task 2)
crates/roles/roles/finance_specialist/role.yaml          modifies: mandate (Task 2)
crates/roles/roles/finance_specialist/system.md          modifies: the mandate paragraph (Task 2)
crates/runtime/src/templates.rs                          modifies: builtins, builtin, is_builtin, TemplateError::Builtin, save, rename, mod tests (Task 3)
crates/runtime/src/daemon/templates.rs                   modifies: the list, applying, call, failure, mod tests (Task 3)
crates/runtime/src/daemon/team.rs                        modifies: code_of loses needs_developer, its tests (Task 3)
docs/schemas/rpc.schema.json                             modifies: templatesListQuery's description (Task 3)
apps/web/src/pages/SavedTeams.tsx                        modifies: Listing, useTemplates, noFolder, builtinMeta (Task 4)
apps/web/src/pages/setup/TeamSetup.tsx                   modifies: Start, teamDraftOf (Task 4)
apps/web/src/pages/setup/SetupTeam.tsx                   modifies: the fourth start, add, the scratch rows (Task 4)
apps/web/src/app/refusals.ts                             modifies: WORDS (Task 4)
apps/web/src/strings/en.ts                               modifies: Task 4's keys; Task 5's keys
apps/web/src/pages/setup/team.test.tsx                   tests:   Task 4
apps/web/src/pages/dialogs/UseTemplate.tsx               modifies: the list, MISSING (Task 5)
apps/web/src/pages/team.test.tsx                         tests:   Task 5
apps/web/src/pages/pages.test.tsx                        tests:   Settings' saved teams without a folder, if a test there reads noFolder (Task 5)
apps/web/e2e/templates.spec.ts, setup-team.spec.ts       modifies: only where an assertion counts the starts or the list (Task 5)
docs/SPEC.md, docs/plans/project-plan.md, docs/decisions/0040-*.md, 0026-*.md, 0019-*.md, this plan   (Task 6)
```

## Interfaces

Consumes (all on `main`): `validate_team`, `validate_template`, `TeamTemplate`, `template_slug`, `plain_role` (`catervas-core`); `ReadinessContext.active_agents_by_role` (`catervas-core::governor::readiness`); `yaml_value` (`catervas-store::files`); `Templates`, `TemplateError`, `TemplateListing` (`catervas-runtime::templates`); `suggested`, `code_of` (`catervas-runtime::daemon::team`); `Choice`, `Faces`, `templateMeta`, `useTemplates`, `draftOf`, `someone` (web).

Produces:

```rust
// catervas-roles
pub const BUILTIN_TEAMS: [(&str, &str); 3];                 // (slug, yaml text), in the order the page lists them
// catervas-runtime::templates
pub fn builtins() -> Vec<(String, TeamTemplate)>;           // every shipped team, in BUILTIN_TEAMS order
pub fn builtin(slug: &str) -> Option<(TeamTemplate, String)>; // the template and its digest
pub fn is_builtin(slug: &str) -> bool;
pub enum TemplateError { /* as today, plus */ Builtin { name: String } }
// catervas-core::governor::readiness
pub enum ReadinessRule { /* as today, plus */ AssigneeAvailable }
```

```ts
// apps/web/src/pages/setup/TeamSetup.tsx
export type Start = "suggested" | "builtin" | "saved" | "scratch";
// apps/web/src/pages/SavedTeams.tsx
export type Listing = { folder: string | null; builtin: { slug: string; template: Template }[]; templates: ...; unreadable: ... };
export function noFolder(listing: Listing | undefined): boolean;
export function builtinMeta(template: Template): string;
```

Wire: `templates.list` → `{ folder: string | null, builtin: [{ slug, template }], templates: [{ slug, template }], unreadable: [{ slug }] }`; refusal `template_builtin` at `/name`.

## Tasks

### Task 1: a team needs a Product Manager alone

Files: modified `crates/core/src/team.rs` (`REQUIRED_ROLES`, `mod tests`), `crates/core/src/team/template.rs` (`mod tests`), `crates/core/src/governor/readiness.rs` (the rule, `CHECKS`, `mod tests`).

- `accepts_a_team_without_a_developer` (team.rs) — a team of an active Product Manager and an active Architect passes `validate_team`.
- `refuses_a_team_without_a_product_manager` (team.rs, replacing the test that expected both messages) — a team of an Architect and a Scrum Master is refused with exactly one message, "A team needs an active Product Manager to write its plans, and this one has none."
- `reports_a_repeated_id_and_a_missing_role_together` (team.rs, adjusted) — two agents `ada` as Architect and Scrum Master: two refusals, the id's and the Product Manager's.
- `accepts_a_template_without_a_developer` (template.rs, replacing `refuses_a_template_without_a_developer`) — a template of a Product Manager and a Marketing Specialist passes `validate_template`.
- `refuses_a_contract_whose_assignee_role_nobody_holds` (readiness.rs) — the fixture contract for a `software_developer` in a context whose `active_agents_by_role` has no Developer and two Architects fails `AssigneeAvailable` with the message naming "Software Developer"; with one Developer it passes that rule.
- `the_assignee_rule_is_checked_with_the_others` (readiness.rs) — `failed_rules` on that context lists `AssigneeAvailable` beside `ReviewerAvailable`'s outcome, not instead of it.

- [ ] `feat(core): a team needs an active Product Manager alone, and readiness needs the assignee's role`

### Task 2: the three teams and the Finance Specialist's mandate

Files: created `crates/roles/teams/software-product.yaml`, `online-store.yaml`, `financial-analysis.yaml` (the Decisions' content); modified `crates/roles/src/lib.rs` (`BUILTIN_TEAMS`, `mod tests`), `crates/roles/roles/finance_specialist/role.yaml`, `system.md`.

- `ships_three_teams_with_their_slugs` — `BUILTIN_TEAMS` has the three slugs in order, and each yaml names `name:` as the Decisions say.
- `holds_every_shipped_team_to_the_template_schema` — each yaml, through `serde_saphyr` to a `Value`, passes `validate_template`; the Financial analysis one has three `finance_specialist` agents and no `software_developer`.
- `the_finance_specialist_keeps_its_guards` — `load_role(FinanceSpecialist)`'s mandate contains "what it owns, spends, earns and has at stake", "not a tax filing, statutory accounts or financial advice" and "Every number names its source" (the last two already there), and the system prompt contains "has at stake".

- [ ] `feat(roles): ship three ready-made teams, and widen the Finance Specialist's mandate to a business's money`

### Task 3: built-ins on the wire

Files: modified `crates/runtime/src/templates.rs` (functions, the variant, `save`, `rename`, `mod tests`), `crates/runtime/src/daemon/templates.rs` (the list, `applying`, `apply`, `pause_before_dropping_google_ads`, `call`, `failure`, `mod tests`), `crates/runtime/src/daemon/team.rs` (`code_of`, the two tests that expect `needs_developer`), `docs/schemas/rpc.schema.json` (the description only).

- `parses_every_builtin_with_a_stable_digest` (templates.rs) — `builtins()` has three in `BUILTIN_TEAMS` order; `builtin("online-store")` answers the Online store with a 64-hex digest equal to sha256 of the embedded text; `builtin("mine")` is `None`; `is_builtin` true for the three only.
- `refuses_saving_or_renaming_to_a_builtins_name` (templates.rs) — `save` of a template named "Online Store" (slug `online-store`) with `replace` true and false, and `rename` of a saved slug to "financial analysis", each answer `Builtin { name }` with the built-in's name, and no file is written.
- `lists_the_builtins_with_the_saved_and_without_a_folder` (daemon) — served with a folder holding one saved team: `builtin` has three in order, `templates` the one, `folder` the path; served with `None`: `folder` null, `builtin` three, `templates` and `unreadable` empty, no refusal.
- `previews_and_applies_a_builtin` (daemon) — `template.preview { slug: "financial-analysis" }` answers a team of Mira and three Finance Specialists with `digest` equal to `builtin`'s; `template.apply` with that digest writes the team, records `team.updated` with `template: "Financial analysis"`, and answers the same; both work when served with no folder.
- `refuses_a_builtin_name_as_template_builtin` (daemon) — `template.save { name: "online store" }` is refused -32005 with code `template_builtin` at `/name`; so is `template.rename` to it.
- `a_saved_slug_without_a_folder_is_still_refused` (daemon) — served with `None`, `template.preview { slug: "mine" }` and `template.save` are refused `no_state_folder`.
- The daemon's existing tests that expect `needs_developer` (`refuses_in_plain_words`, team.rs's two) are changed to teams without a Product Manager, expecting `needs_product_manager`.

- [ ] `feat(runtime): list, preview and apply the ready-made teams, and reserve their names`

### Task 4: the fourth start, and from scratch

Files: modified `apps/web/src/pages/SavedTeams.tsx`, `apps/web/src/pages/setup/TeamSetup.tsx`, `apps/web/src/pages/setup/SetupTeam.tsx`, `apps/web/src/app/refusals.ts` (`needs_developer` out, `template_builtin` in), `apps/web/src/strings/en.ts` (`startBuiltin`, `startBuiltinNote`, `startWhichBuiltin`, the three `builtin*` lines, `builtinMeta`, `templateBuiltin`, `teamNameAll`; `startScratchNote`, `teamLeadScratch`, `startsLegend` reworded for a Product Manager alone; `teamNameBoth` and `refuseNeedsDeveloper` deleted); tests in `apps/web/src/pages/setup/team.test.tsx`.

- `offers_four_starts` (replacing `offers_three_starts`) — the starts are, in order, the suggested team, a ready-made team, a saved team, from scratch; "A ready-made team" is enabled with no folder and no saved team, "A saved team" disabled, and each of the three is listed by name with its line.
- `starts_from_a_ready_made_team` — picking Online store fills four rows, Mira, Noor, Lena, Kai, each ticked, Lena's and Noor's with their pictures; "Filled from Online store" shows; Continue goes to `/setup/spending`; the finish screen lists "What they may do, from Online store".
- `a_ready_made_designer_is_held_off_without_a_sandbox` — with `designer_needs_sandbox`, a ready-made team's Designer row (a test template with one) starts unticked, as a saved team's does.
- `adds_someone_to_a_team_without_a_developer` — from Online store, "Add someone" adds a fifth row from the spare names with an extra picture.
- `from_scratch_needs_a_product_manager_and_one_more` — from scratch: the Product Manager's row has no tick box, the Developer's has one; unticking the Developer with nobody else ticked holds Continue with `teamNameAll`; ticking the Finance Specialist under "More roles" and naming the Product Manager lets Continue go on to `/setup/permissions`.
- `says_a_team_without_a_developer_is_fine` — `said("needs_developer")` is no longer a known refusal (it falls back to the generic wording), and `said("template_builtin")` is "Catervas comes with a team called … Choose another name.".

- [ ] `feat(web): a ready-made team as setup's fourth start, and from scratch with a Product Manager alone`

### Task 5: the Team page lists the ready-made

Files: modified `apps/web/src/pages/dialogs/UseTemplate.tsx`, `apps/web/src/strings/en.ts` (`useBuiltinList`, `useLead` reworded); tests in `apps/web/src/pages/team.test.tsx`, and in `pages.test.tsx` only if a Settings test reads the folder; `apps/web/e2e/templates.spec.ts` and `setup-team.spec.ts` changed only where an assertion counts the starts or the list's options.

- `lists_the_ready_made_above_the_saved` — "Use a saved team" shows "Ready-made" with the three, then "Saved teams" with the saved; the first option is Software product; a built-in's line is its names and "comes with Catervas".
- `previews_and_uses_a_ready_made_team` — "Show what changes" for Financial analysis calls `template.preview { slug: "financial-analysis" }` and "Use this team" calls `template.apply` with its digest.
- `opens_with_no_saved_team_and_no_folder` — with `folder` null and no saved teams, the dialog still lists the three and says nothing about a missing folder until "Saved teams", which says `templateNoFolder`.
- `refusedLine_knows_no_developer_rule` — a preview refusal `needs_product_manager` names the Product Manager; any other code falls back to `said(code)` and "Choose another saved team."

- [ ] `feat(web): list the ready-made teams on the Team page`

### Task 6: docs

Files: `docs/SPEC.md` (1 goal 1 and the non-goal line; 3, the template paragraph gains the built-ins and the `init` sentence loses "the two a team cannot work without"; 4.1, four starts; 4.4, the list and the paused-Developer example rewritten with a Product Manager; 5.3, the assignee bullet after the reviewer's; 6.6, the mandate; 6.8, the "every team has a Developer" clause; 8.4, built-ins in the binary; F1, enforce a Product Manager; the revision line 0.87), `docs/plans/project-plan.md` (D18's line, phase 14 steps 03 and 05's notes, revision 42), `docs/decisions/0040-*.md`, `0026-*.md`, `0019-*.md` (one "Amended 2026-10-10 by ADR 0055" line each), this plan's status to done.

- [ ] `docs(spec): ready-made teams, and a team needs a Product Manager alone (0.87)`

## Verification

```
cargo xtask check
# expected: xtask check: ok   (Rust and the front end's pnpm check)
cargo xtask check --integration
# expected: xtask check: ok
```

Then the founder, in `catervas serve` on a fresh folder, picks "A ready-made team", Online store, names nobody differently, continues to Start the team, files a request, and sees the Product Manager plan it; then on the Team page uses Financial analysis and sees the preview retire or remove the store's agents and add the three desks.
