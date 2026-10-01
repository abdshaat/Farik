# Phase 6, step 14: Team templates

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 1 (the "one team" non-goal), 3 (team), 4.1 (three starts), 4.4, 8.4 (the state folder), F1
Depends on: steps 01 to 13 of this phase (step 06's team setup, step 11's `ui_ux_designer` role, step 12's per-agent connectors, step 13's chats; renumbered from step 13 by the project plan's revision 23)
Readiness confirmed by: fresh-session reviewer, 2026-09-30, ready with findings, folded in
Mockups approved by: auto-approved 2026-09-30 under the founder's standing instruction of that night ("auto approve the design, we can refine it later"), not reviewed by the founder; on the canvas's "Team templates" page (version 1790822068-6ce7) for the founder's later review. Wording the mockups add, which binds Task 6: "Replace" in the `template_exists` state, a templates-specific refusal sentence, "Delete it" / "Keep it", and the folder `~/.config/farik/templates` named in Settings
As built (Task 2, checked against steps 06, 11, 12 and 13 as landed): the role enum's six (`ui_ux_designer` among them), `mcp_servers` `{ name, source: builtin }`, `team.propose`'s six, `update_agent_with`, `errors_wire`, `code_of`, `write_private`, `yaml_value` and `state_dir` are as named. Differences: (1) `template_from_team` returns `Result<TeamTemplate, Vec<ValidationError>>`; it trims the name and goes through `validate_template`, so a name the schema refuses is an error at `/name`, not a panic. (2) The five copied `$defs` are the team's own Rust types in the generated module (`typify`'s `replace`), so a template's role, model and answers move into a team as they are. (3) The team schema's integration enum is at `$defs/policy/properties/integration`. (4) Step 12's per-role connector is inline in `daemon/team.rs`'s `propose` (the Designer gets `playwright`, listed `unavailable` without a sandbox); there is no shared function, so Task 3 gives an added Designer `playwright` itself. (5) Step 13: a chat's events carry the agent's id, so an agent the user chatted with has "worked" under the any-event test `checked` uses, and Task 3 retires it rather than removing it; no change needed. (6) `validate_template` reports an unknown key at its own pointer (`/rules`), where `validate_team` reports `/`.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A user who has tuned a team (names, pictures, personas, models, and the four answers setup asks) saves it once and uses it in any project on the machine:
- setup's "Your team" offers three starts: the suggested team, a saved team, or from scratch;
- the Team page gains "Save as a template" and "Use a saved team", and the second shows who stays, who joins, and who is retired or removed before anything is saved;
- Settings lists the saved teams, where each can be renamed or deleted.

Switching keeps step 06's safety rules: a worked agent is retired, never deleted; a Product Manager and a Developer are required; the cap is seven. A project still has one team (spec 1): a template is reuse, not a second live team.

Out of scope: sharing templates between machines or users (phase 8's ecosystem); templates of criteria, rules or checks; a model fallback per provider (phase 7, which the risk in the design names); uploading a picture (no upload exists yet).

Binding inputs: ADR 0026 (C), and "C. Team templates" in `docs/design/designer-chats-templates.md`. Where this plan differs from the design, the Decisions say so and why.

## Decisions

- **Mockups first** (the founder's standing gate, 2026-09-26; ADR 0026 D). Task 1 draws them; no later task starts until the founder approves them and the header's "Mockups approved by" names the date. Copy settled in the mockups is binding on Task 6, word for word.
- **Where the templates list lives: Settings, a "Saved teams" section**, over a page of its own or the "Use a saved team" dialog. Settings already holds what outlives one project (the AI account), and renaming or deleting inside the dialog that applies one mixes two jobs in one place.
- **The file.** `<state folder>/templates/<slug>.yaml`, the state folder being the CLI's `state_dir` (`$XDG_CONFIG_HOME/farik`, else `$HOME/.config/farik`, else `%APPDATA%\farik`). `Templates::save` makes `templates/` with `DirBuilder::new().recursive(true).mode(0o700)`, so every folder it creates is 0700, the state folder itself when this is the first thing to make it; a state folder that exists keeps its mode. Each file is written 0600 with `farik_runtime::write_private`. Held to `docs/schemas/team-template.schema.json`:

  ```
  version: 1                      # const
  name: string (1..60, trimmed, not blank)
  saved_at: date-time
  agents: [2..7] { id, display_name, role, persona?, avatar?, model? }   # the team schema's own constraints
  policy: { permissions, judgment, integration }                        # all three required
  budgets: { daily_usd? }                                               # absent: no daily limit
  additionalProperties: false at every level
  ```

  `role`, `model`, `permissions`, `judgment` and the integration enum are copied into the template schema's `$defs` from `team.schema.json`, because `typify` takes no external reference (the note in `team.rs`'s validator). A test pins each copy equal to the team schema's.
- **The slug** is `template_slug` in `farik-core`: the rule of the CLI's `slug` (`crates/cli/src/project.rs`: ASCII letters and digits lower-cased, every run of anything else one `-`, trimmed), capped at 64 like `SetupProject.tsx`'s, and `None` when nothing is left. The CLI's `slug` becomes `template_slug(name).unwrap_or_else(|| "farik".into())`, so the rule lives once; project ids gain the 64 cap the team schema already puts on an id. A template name with no slug is refused. The wire keys every method but `template.save` by `slug`, the file's name, which is stable and unambiguous; `templates.list` answers both.
- **What a template holds.** Per agent: `id`, `display_name`, `role`, `persona`, `avatar`, `model` (id and effort). The team: `policy.permissions`, `policy.judgment`, `policy.integration`, `budgets.daily_usd`. It holds the agent's `id` as well as its name (the design's sample had none) because an id never changes after it is saved (step 06) and a renamed agent's slug would otherwise never match its own id.
- **What it leaves out:** `rules` (paths, commands, `ui_paths`, protected paths), the criterion library, `preview`, `integration_branch`, the team's `name`, each agent's `status`, `grants`, `revokes`, `preauthorized_external_tools` and `mcp_servers`, and every other `policy` or `budgets` key. Applying keeps the project's values for all of these.
- **Pictures.** A shipped avatar's name is held. An `avatar` that is a path under `.farik/team/avatars/` is left out, and the agent takes its role's shipped avatar where the template is used, because nothing uploads a picture yet; the design's "uploaded image copied with the template" waits for the upload.
- **Validating a template** is `validate_template`: the schema, then the team the template makes (every agent `active`, over `defaults()`) held to `validate_team`. So a hand-edited template gets the same plain sentences a team does ("A team needs an active Software Developer to do the work, and this one has none."), each at its template path.
- **Saving** writes no project event (the design; no project's log owns the file). A name whose slug is taken is refused with `template_exists`, "A saved team is called <name> already. Replace it, or choose another name.", unless `replace: true`. `saved_at` is the daemon clock's now.
- **Applying** (the design's switching rules, made exact; `apply_template` in `farik-core`, pure):
  1. an agent of the template whose `id` and `role` match a project agent that is not retired is **kept**: it takes the template's name, and its persona, avatar and model where the template has them (a field the template leaves out, an uploaded picture among them, keeps the project's value), and keeps its status, grants, revokes and connectors. A paused match stays paused (the design says "active"; matching only active agents would retire a paused Mira to add a second Mira, and a status is the card's to change, step 06);
  2. every other agent that is not retired and has **worked**, meaning the log has an event whose agent is its id (step 06's test in `checked`), is **retired**;
  3. every other agent that never worked is **removed** from the file;
  4. every template agent not kept is **added**, `active`, with no grants or revokes, its role's persona, shipped avatar and model where the template has none, and its role's connectors as `team.propose` gives them (step 12); its id takes `-2`, `-3`… when a retired or kept agent holds it (step 06's rule);
  5. the team takes the template's `permissions`, `judgment`, `integration` and `daily_usd` (absent clears the limit);
  6. the result is held to `validate_team`, and its errors are returned with the lists, not instead of them. It can fail: a kept match that is paused leaves its role, or a named judge, without an active agent (a paused Developer `theo` kept while the active Developer is retired). `template.preview` then answers the errors (`errors_wire`, the step 06 codes such as `needs_developer`), the dialog shows them from `en.ts` with "Use this team" disabled, and `template.apply` refuses with -32005 and the same `data.errors`, writing nothing.
  Already retired agents are untouched.
- **What applying records.** One write of `team.yaml`, then, for each retired agent in order, the retirement's own effects as `agent_update` does them today (its sessions stopped; `agent.updated { status: retired }`; each `in_progress` task it holds blocked with "agent retired by the user"), then one `team.updated` with the new optional `template: <name>`. `team.updated` is the kind that exists; the field is optional, so older events validate.
- **RPC shape.** The brief's five, following the codebase's query/method split (`team.validate` is a query, `team.save` a method), so the preview is its own query rather than a flag on a write:
  - query `templates.list {}`, query `template.preview { slug }`;
  - methods `template.save { name, replace? }`, `template.apply { slug }`, `template.rename { slug, name }`, `template.delete { slug }`.
  This replaces the design's `template.get` and `team.propose`'s `from`: `templates.list` answers each template whole, setup fills its screens from that, and "from scratch" is the page's own. The project plan's step 14 interface line changes to match (Task 8).
- **The preview is recomputed on apply**, not handed back: one user, one browser, and the apply answers the changes it made, which the page shows. A stale preview costs a second look, not a wrong save.
- **Setup with a saved team** does not use the switching rules: `team.start` replaces the starter team under the setup marker, even one that has recorded work (step 06's `starts_the_team_from_setup`), so `template.preview` would wrongly retire it. Setup takes the chosen template from `templates.list`: its agents fill the builder, `active`, with the role defaults `team.propose` gives for any field the template leaves out; Spending, Finishing work and SetupAdvanced's plan check (`policy.judgment`) open with the template's answers. **The permission answers carry over** (the founder, 2026-09-30): the template's `run_commands` and `push` are applied and What they may do is not shown again; the Finish screen lists them in words, and Settings changes them later.
- **From scratch** is two rows, a Product Manager and a Developer, with their roles fixed and names empty, and "Add someone"; Continue stays disabled until both are named. Models, pictures and personas are the roles' defaults from `team.propose`.
- **Renaming** writes the template under the new slug with the new name and the same `saved_at`, then removes the old file; a new slug another template holds is refused with `template_exists`; a name that only changes case keeps the file. **Deleting** removes the file. Neither records an event. An unknown slug is `-32002`, "There is no saved team <slug>."
- **An unreadable file** in `templates/` (not YAML, or failing `validate_template`) is not listed as a template; `templates.list` names it under `unreadable: [{ slug }]`, and Settings shows it with Delete only and `en.ts`'s fixed line "This saved team cannot be read. Delete it, or fix the file by hand." Previewing or applying it is refused with `template_unreadable`.
- **No state folder** (none of the three variables set): every template method and query is refused with `no_state_folder`, "Farik has no folder on this computer to keep saved teams in.", and the three starts show "A saved team" disabled with that line.
- **List order**: by name, case-insensitive, so a rename moves an entry predictably.
- **Refusal codes.** `daemon/templates.rs` maps each `TemplateError` straight to a `Failure` with `data.errors: [{ path, message, code }]`, not through `code_of`'s message matching:

  | Error | JSON-RPC | code | path |
  |---|---|---|---|
  | `Exists` | -32005 | `template_exists` | `/name` |
  | `Name` | -32005 | `template_name` | `/name` |
  | `Unreadable` | -32005 | `template_unreadable` | `/slug` |
  | no state folder | -32005 | `no_state_folder` | `/` |
  | `NotFound` | -32002 | (none) | (none) |

  A result `validate_team` refuses carries step 06's codes through `errors_wire`. The daemon's `message` is for the log and the command line; **the page words every refusal from `en.ts` by its code, never from the daemon's text.**
- **Decided by the founder, 2026-09-30:** a saved team's permission answers carry over with no re-asking (above), and `template.rename` is in scope.
- **Steps 11 and 12's names** (`ui_ux_designer` and `team.propose`'s six from step 11; `mcp_servers` and its per-role connectors from step 12) come from their plans, not landed code. Re-check them here when those steps land; the schema-copy test catches a drift in the role enum.

## File map

```
docs/design/mockups/{SetupTeamStarts,SaveTemplate,UseTemplate,PhoneUseTemplate,SavedTeams}.dc.html, canvas.json   creates / modifies (T1)
docs/schemas/team-template.schema.json                                    creates (T2)
docs/schemas/{event,rpc}.schema.json                                      modifies (T5: team.updated's template; the six names)
crates/core/src/generated/mod.rs                                          modifies (T2: the template types)
crates/core/src/team/template.rs (+ tests), crates/core/src/team.rs       creates / modifies (T2, T3)
crates/cli/src/project.rs                                                 modifies (T2: slug through template_slug)
crates/store/src/files.rs                                                 modifies (T4: template_yaml)
crates/runtime/src/templates.rs (+ tests), crates/runtime/src/lib.rs      creates / modifies (T4)
crates/runtime/src/daemon/templates.rs (+ tests), daemon/web.rs, daemon/team.rs   creates / modifies (T5)
crates/runtime/src/orchestrator/human.rs                                  modifies (T5: the retirement's effects shared)
crates/cli/src/start.rs                                                   modifies (T5: WebState's templates folder)
crates/protocol/src/{event.rs,rpc.rs}, packages/protocol-client/src/client.ts   modifies (T5)
apps/web/src/pages/setup/{SetupTeam,TeamSetup,SetupFinish}.tsx, setup/team.test.tsx   modifies (T6)
apps/web/src/pages/dialogs/{SaveTemplate,UseTemplate}.tsx (+ tests), pages/SavedTeams.tsx (+ test)   creates (T6)
apps/web/src/pages/{Team,Settings}.tsx, pages/team.test.tsx, strings/en.ts   modifies (T6)
apps/web/e2e/templates.spec.ts, apps/web/e2e/fixtures/pair-template.yaml  creates (T7)
apps/web/e2e/fixtures/serve.ts                                            modifies (T7: exports stateFolder)
docs/SPEC.md (1, 3, 4.1, 4.4, 8.4, 8.5, F1), docs/plans/project-plan.md (step 14 line and interface line)   modifies (T8)
```

## Interfaces

Consumes: `validate_team`, `Team`, `Agent`, `MAX_AGENTS`, `defaults()`, `describe_change` (`farik-core`, step 06); `write_private` (`farik-runtime`); `yaml_value` (`farik-store::files`); `state_dir` (`crates/cli/src/state.rs`); `update_agent_with`'s retirement (`orchestrator/human.rs`, phase 4 and step 06); `team.propose`'s per-role connector default (step 12); `Refused`, `errors_wire`, `code_of` (`daemon/team.rs`, step 06); `Dialog`, `Choice`, `TextField`, `List` (`@farik/ui`, step 03).

Produces:

```rust
// farik-core
pub use generated::team_template::{TeamTemplate, TemplateAgent};               // team/template.rs
pub fn template_slug(name: &str) -> Option<String>;
pub fn validate_template(input: &Value) -> Result<TeamTemplate, Vec<ValidationError>>;
pub fn template_from_team(team: &Team, name: &str, saved_at: DateTime<Utc>) -> TeamTemplate;
pub struct TemplateApplied { pub team: Team, pub kept: Vec<String>, pub retired: Vec<String>, pub removed: Vec<String>, pub added: Vec<String>, pub errors: Vec<ValidationError> }   // agent ids; errors empty when validate_team passes
pub fn apply_template(current: &Team, template: &TeamTemplate, worked: &BTreeSet<String>, suggested: &[Agent]) -> TemplateApplied;   // suggested: team.propose's agents, one per role (persona, avatar, model, connectors), since farik-core cannot read farik-roles
// farik-store::files
pub fn template_yaml(template: &TeamTemplate) -> Result<String, FilesError>;
// farik-runtime
pub struct Templates { dir: PathBuf }
pub enum TemplateError { Exists { name: String }, Name, NotFound { slug: String }, Unreadable { slug: String }, Io { detail: String } }
pub struct TemplateListing { pub templates: Vec<(String, TeamTemplate)>, pub unreadable: Vec<String> }   // (slug, template); slugs
impl Templates {
    pub fn new(dir: PathBuf) -> Templates;
    pub fn list(&self) -> Result<TemplateListing, TemplateError>;
    pub fn read(&self, slug: &str) -> Result<TeamTemplate, TemplateError>;
    pub fn save(&self, template: &TeamTemplate, replace: bool) -> Result<String, TemplateError>;   // the slug
    pub fn rename(&self, slug: &str, name: &str) -> Result<String, TemplateError>;
    pub fn delete(&self, slug: &str) -> Result<(), TemplateError>;
}
WebState.templates: Option<Templates>        // None when there is no state folder
pub(crate) fn retire_effects(tools: &ToolDeps, daemon: &DaemonState, team: &Team, agent_id: &str) -> Result<Vec<u64>, CommandError>;   // human.rs, shared with update_agent_with
```

Wire (`rpc.schema.json`; `snake_case`, camelCase in `protocol-client`):
- `templates.list {}` → `{ templates: [{ slug, template }], unreadable: [{ slug }] }`, each `template` whole (agents with persona, avatar and model; policy; budgets);
- `template.preview { slug }` → `{ team, kept: [id], retired: [id], removed: [id], added: [id], effects: [string], errors: [{ path, message, code }] }` (`effects` is `describe_change(current, result)`);
- `template.save { name, replace? }` → `{ slug }`; `template.apply { slug }` → the preview's shape, as applied, with `errors` empty; `template.rename { slug, name }` → `{ slug }`; `template.delete { slug }` → `{}`;
- event `team.updated` gains optional `template: string` (the template's name).

## Tasks

### Task 1: Mockups, approved by the founder

Files: created `docs/design/mockups/{SetupTeamStarts,SaveTemplate,UseTemplate,PhoneUseTemplate,SavedTeams}.dc.html`, modified `canvas.json` and the design canvas artifact (`docs/design/web-ui.md`'s link), with a new canvas page "Team templates".
- `SetupTeamStarts`: "Your team" with three `Choice` cards (the suggested six; a saved team, with its picker of names and faces; from scratch), then the builder filled from the choice, and the Finish screen of a saved team listing its carried-over permission answers in words.
- `SaveTemplate`: the Team page's dialog, a name field, Save, and the `template_exists` state with Replace.
- `UseTemplate`: pick a saved team, then the preview: four groups (Stays, Joins, Retired, Removed), each agent with face, name and role; the effects in words; "Use this team"; and the refused state, the errors in words with "Use this team" disabled. `PhoneUseTemplate` is the same at 360 px, the densest of the five.
- `SavedTeams`: Settings' section, each saved team with its faces, Rename (inline) and Delete (confirm), an unreadable file's row with Delete only, and the no-state-folder line.
Colours from `@farik/brand`'s tokens only, muted and light, one colour per job (`docs/brand/brand.md`). No test; the gate is the founder's approval, recorded in this plan's header in the same commit.

- [x] `docs(design): mock up team templates for the founder's approval`

### Task 2: The template format (`farik-core`)

Files: created `docs/schemas/team-template.schema.json`, `crates/core/src/team/template.rs`; modified `generated/mod.rs`, `team.rs` (module), `crates/cli/src/project.rs`.
Produces: `template_slug`, `TeamTemplate`, `TemplateAgent`, `validate_template`, `template_from_team`.

- `copies_the_team_schemas_definitions`: each of `role`, `model`, `permissions`, `judgment` and the integration enum in the template schema equals the team schema's, as JSON values.
- `holds_each_agent_and_the_four_answers`: from the base fixture with a persona, a shipped avatar and `claude-sonnet-5` at `low`, and with `policy.permissions` and `policy.judgment` left out, the template has each agent's id, name, role, persona, avatar and model, the effective `team.permissions()` and `team.judgment()` written out, and the team's integration and `daily_usd`.
- `leaves_out_what_belongs_to_the_project`: a team with `rules`, grants, revokes, preauthorized tools, `mcp_servers`, a paused and a retired agent: the template has no `rules`, no status or grant keys, and no retired agent; a paused agent is held.
- `leaves_out_an_uploaded_picture`: an avatar `.farik/team/avatars/mira.png` is absent in the template.
- `validates_a_saved_template`.
- `refuses_a_template_without_a_developer` and `refuses_a_template_without_a_product_manager`: each error is at `/agents` with `validate_team`'s exact sentence.
- `refuses_eight_agents`: the schema's `maxItems` error at `/agents`.
- `refuses_a_judge_the_template_does_not_hold`: `judge: architect` with no Architect, at `/policy/judgment/judge`, with `validate_team`'s sentence.
- `refuses_unknown_keys`: `rules:` at the top level is refused at `/rules`.
- `slugs_a_template_name`: "My usual team" → `my-usual-team`; "Team #2!" → `team-2`; "!!!" → `None`; a name of 100 `a`s → 64 `a`s; "ab " repeated 30 times → 64 characters or fewer with no trailing `-`; the CLI's `slug` tests pass unchanged.

- [x] `feat(core): add the team template format, kept apart from the project`

### Task 3: Applying a template (`farik-core`)

Files: modified `crates/core/src/team/template.rs`. Produces: `TemplateApplied`, `apply_template`. Consumes: Task 2.

- `keeps_an_agent_by_id_and_role`: `mira` Product Manager in both is kept and takes the template's name, persona, avatar and model; its grants and revokes are the project's.
- `keeps_a_paused_match_paused`.
- `keeps_the_projects_value_where_the_template_has_none`: a template `mira` with no persona, avatar or model keeps the project's `mira`'s three; an added agent with none gets its role's.
- `refuses_when_a_kept_match_is_paused`: project Ada, the active Developer, who worked, and Theo, a paused Developer; a template whose Developer is `theo`: `retired` holds `ada`, `kept` holds `theo`, and `errors` holds `validate_team`'s needs-an-active-Developer sentence at `/agents`.
- `retires_a_worked_agent_it_does_not_hold`: `theo` in `worked` and absent from the template is `retired` in the result and in `retired`.
- `removes_an_agent_that_never_worked`: absent from the file, and in `removed`.
- `adds_the_rest_with_a_free_id`: a template `theo` Developer where the project's `theo` is retired gets `theo-2` (`theo-3` when `theo-2` is held too); the same id with another role retires the project's worked agent and adds `<id>-2`, or removes a never-worked one and adds `<id>` (rule 4: a removed agent holds no id); a 64-character id's free id stays within 64.
- `leaves_retired_agents_alone`.
- `takes_the_four_answers`: permissions, judgment and integration are the template's; a template with no `daily_usd` clears the project's; `rules`, `name` and other policy keys are the project's.
- `never_passes_the_cap`: a seven-agent template over a seven-agent project whose four worked: seven active, four retired, `errors` empty.

- [x] `feat(core): apply a template to a team by the retirement and required-role rules`

### Task 4: Where templates are kept (`farik-runtime`)

Files: created `crates/runtime/src/templates.rs`; modified `crates/store/src/files.rs`, `crates/runtime/src/lib.rs`. Produces: `template_yaml`, `Templates`, `TemplateError`, `TemplateListing`. Consumes: Task 2.

- `saves_privately`: in a fresh temporary folder with no state folder yet, `save` makes the state folder and `templates/` 0700 and `my-usual-team.yaml` 0600 (Unix), and `read` gives back an equal template.
- `refuses_a_taken_name_unless_replacing`: `Exists` with the stored name; with `replace` the file is rewritten.
- `refuses_a_name_with_no_slug`: `Name`.
- `round_trips_through_yaml`: `template_yaml` then `yaml_value` then `validate_template` gives back an equal template.
- `lists_by_name_and_names_unreadable_files`: three templates in case-insensitive name order; a `broken.yaml` of `{` appears in `unreadable`, not in `templates`.
- `renames_and_deletes`: `rename` answers the new slug, the old file is gone, `saved_at` unchanged; a case-only rename keeps the slug; a rename onto another's slug is `Exists`; `delete` removes the file; an unknown slug is `NotFound` for `read`, `rename` and `delete`.

As built (Task 4): `round_trips_through_yaml` is in `farik-store`'s `files.rs`, beside `template_yaml`; the rest are in `templates.rs`, with two more: `maps_a_refused_name_from_the_team` (`impl From<Vec<ValidationError>> for TemplateError`, every refusal `Name`, since for a valid team `template_from_team` can only refuse the name) and `refuses_a_path_through_a_name_or_a_slug` (a slug that is not its own `template_slug` is `NotFound`, so `..` or a path names nothing). A file is written beside as `.<slug>.yaml.saving` and renamed over. A file in `templates/` whose stem is not a slug is not listed. The folder Settings' mockup names, `~/.config/farik/templates`, is the path only when `XDG_CONFIG_HOME` is unset: `state_dir` takes `$XDG_CONFIG_HOME/farik` first and `%APPDATA%\farik` last, so Task 6 shows the real path from the daemon rather than the mockup's fixed text.

- [x] `feat(runtime): keep team templates privately in Farik's state folder`

### Task 5: Templates over the wire

Files: created `crates/runtime/src/daemon/templates.rs`; modified `event.schema.json`, `rpc.schema.json`, `daemon/web.rs` (routing and `WebState.templates`), `daemon/team.rs` (`errors_wire` shared), `orchestrator/human.rs` (`retire_effects` extracted, `update_agent_with` calling it), `crates/cli/src/start.rs` (`state_dir(env).map(|d| Templates::new(d.join("templates")))`), `crates/protocol/src/{event.rs,rpc.rs}`, `packages/protocol-client/src/client.ts`.
Consumes: Tasks 3 and 4.

- `lists_saves_renames_and_deletes`: through `rpc`, each answer as the Wire section says.
- `saving_records_no_event`: the log's last `seq` is unchanged after `template.save`, `template.rename` and `template.delete`.
- `previews_without_writing`: `template.preview` answers kept, retired, removed, added, effects and empty errors; `team.yaml` and the log are unchanged.
- `lists_whole_templates`: `templates.list` answers each template with its agents' personas and models, its policy and budgets, and a broken file under `unreadable` by slug alone.
- `applies_with_the_retirements_effects`: on the harness team whose `dev-b` holds an `in_progress` task and has worked, applying a template without `dev-b` writes the team once, then appends `agent.updated { dev-b, retired }`, the task's `task.transitioned` to `blocked` with "agent retired by the user", then `team.updated { template: "Pair" }`, in that order; a never-worked agent is gone with no event.
- `refuses_in_plain_words`: each row of the Decisions' table, with its JSON-RPC code, `data.errors[0].code` and `path`; and applying the paused-match template of Task 3 answers -32005 with `needs_developer`, `team.yaml` and the log unchanged, while its preview answers the same error in `errors`.
- `still_retires_one_agent_as_before`: `agent_update` retire and `agent.replace` keep their existing tests' events (the refactor's guard).
- The protocol client (not counted above): `client.test.ts` maps `saved_at` → `savedAt` and `team.updated`'s `template`.

As built (Task 5): `templates.list` also answers `folder`, the templates folder as the daemon uses it (`$XDG_CONFIG_HOME/farik/templates` when that is set), which Settings shows (Task 6 carry). The retirement's effects are `status_effects(tools, daemon, team, agent_id, status)` in `human.rs`, covering pause and resume too, rather than a retire-only `retire_effects`. `team.propose`'s agent building is `suggested()` in `daemon/team.rs`, used by `team.propose`, `template.preview` and `template.apply`; whether an agent worked is `worked()` there, shared with `checked`. Every write of `team.yaml` (`team.save`, `team.start`, `update_agent_with` and so `agent.update` and `agent.replace`, and `template.apply`) holds one lock, `DaemonState::team_writes`, from its read to its write; `template.apply` works the template out again under it. The validation error item is the shared `$defs/teamError` (`teamValidateResult` and `templateAppliedResult`). Two tests beyond the seven: `checks_again_when_applying_a_preview_gone_stale` and `applies_only_under_the_lock_that_writes_the_team`, so farik-runtime gains 9 (8 in `daemon/templates.rs`, `still_retires_one_agent_as_before` in `human.rs`). `crates/protocol/src/{event.rs,rpc.rs}` needed no change: the types are generated from the schemas, and the runtime tests hold every new request and answer to them.

- [x] `feat(runtime): list, save, preview, apply, rename and delete team templates`

### Task 6: The pages

Files as the file map says. Every test also runs axe (step 06's rule).

- `offers_three_starts`: "Your team" shows the three choices; the suggested one is selected and fills six rows (step 11's six).
- `starts_from_a_saved_team`: choosing "Three of us" from `templates.list` fills the builder with its agents and never calls `template.preview`; Spending, Finishing work and the plan check open with its answers; What they may do is skipped, and `team.start` carries the template's `run_commands` and `push`; the Finish screen lists both in words.
- `starts_from_scratch`: two rows, Product Manager and Developer, names empty, Continue disabled until both are named.
- `disables_saved_with_no_state_folder_or_none_saved`, with the mockup's line for each.
- `saves_the_team_as_a_template`: the dialog sends `template.save { name }`; `template_exists` shows the mockup's sentence and Replace sends `replace: true`.
- `shows_what_changes_before_using`: the dialog lists Stays, Joins, Retired and Removed from `template.preview`, with the effects; "Use this team" sends `template.apply` and the Team page shows the new team.
- `disables_use_when_the_result_is_refused`: a preview with `needs_developer` shows `en.ts`'s sentence for it, not the daemon's message, and "Use this team" is disabled.
- `renames_and_deletes_in_settings`: Rename sends `template.rename`; Delete asks, then sends `template.delete`; an unreadable row offers Delete only, with `en.ts`'s fixed line.

- [ ] `feat(web): start a team from a saved one, and save, use, rename and delete templates`

### Task 7: The templates journey (Playwright)

Files: created `apps/web/e2e/templates.spec.ts`, `apps/web/e2e/fixtures/pair-template.yaml` (Mira, Product Manager, and Noor, Developer, each with a persona; the four answers). Modified `apps/web/e2e/fixtures/serve.ts`: it exports `stateFolder`, `join(XDG_CONFIG_HOME, "farik")`, which both serves share.
1. Project A: `startServe({ project: true, team: "pm-architect-developer", transcripts: ["reply_to_a_mention"] })`. Post `@theo` in the channel and wait for the reply, so Theo has worked. Team → Save as a template "Three of us". Assert `${stateFolder}/templates/three-of-us.yaml` exists at 0600 in a 0700 folder, and A's event count did not change.
2. Project B: `startServe({ project: true, setupPending: true, transcripts: [] })`. Your team → A saved team → Three of us: rows Mira, Ada, Theo. Answer commands yes, push no; keep the spending and finishing shown; Start the team. Assert B's `team.yaml` has `mira`, `ada`, `theo` with personas "Mira.", "Ada.", "Theo." and the log has `team.updated` then `team.resumed`.
3. Project A again: copy `pair-template.yaml` into `${stateFolder}/templates/pair.yaml`. Team → Use a saved team → Pair. The preview shows Stays: Mira; Joins: Noor; Retired: Theo; Removed: Ada. Use this team. Assert A's `team.yaml` has `mira` active, `noor` active, `theo` retired and no `ada`, and the log's last events are `agent.updated { theo, retired }` then `team.updated { template: "Pair" }`.
Screenshots of "Your team" with the three starts and of the preview, at 360 and 1280 px.

- [ ] `test(web): save a team, start a project from it, and use one on a live team`

### Task 8: Spec and plan

Under the next free spec revision when this step lands (0.34 if steps 11, 12 and 13 take 0.31 to 0.33), each change marked "added in 0.NN":
- section 1's non-goal: "One human, one team, for now. Team templates are reuse, not several live teams.";
- section 3: **Team template**, a saved team kept on the machine, what it holds and leaves out;
- 4.1: "Your team" offers three starts, and a saved team's permission answers carry over without being asked again;
- 4.4: "Save as a template" and "Use a saved team", the switching rules and the preview;
- 8.4: `templates/<slug>.yaml` in the state folder, 0700 and 0600, no event;
- 8.5: `team.updated`'s `template`;
- F1: a template must hold a Product Manager and a Developer, at most seven;
- the project plan: step 14's line, "Built <date> (spec 0.NN)", and its interface line as the Decisions' RPC shape.

- [ ] `docs(spec): team templates kept on the machine, and the one-team non-goal`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (farik-core +21 from T2 (11) and T3 (10), farik-runtime +13 from T4 (6) and T5 (7));
#   @farik/web: step 13's landed count plus 8 (T6); @farik/protocol-client plus 1;
#   playwright: step 13's landed count plus 1 (templates.spec.ts);
#   last line: xtask check: ok
```
