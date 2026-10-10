# Phase 8, step 01: Role folders and ownership

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.3, 5.4, 5.6, 5.16, 5.17 (new), 6.1, 6.2, 6.3, 6.4, 6.5, 6.8, 8.5; F5, F6
Depends on: phase 7 and the Catervas rename (merged on main, 2c28b555); ADR 0051 and `docs/design/catervas-folders.md` (approved by the founder, 2026-10-09)
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): ready; no Blocking; carried into execution S1–S6 and N1–N5, folded below.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at ac1a606e.

## Goal

Every role but the Finance and Procurement Specialists owns a folder under `docs/catervas/`, and Catervas holds it: while a role has an active agent, no other role's task may name a path that could reach its folder, and no accepted diff changes one file of a human document's pair without the other. The Marketing Specialist's `docs/marketing/` becomes `docs/catervas/marketing/`, and each marketing plan gains its agent twin. The Product Manager's `.catervas/product/` and `catervas_write_product_doc` go; its folder is `docs/catervas/product/`. Every session's Team rules name the team's folders, and each owner's prompt names its own. Out of scope: the Marketing Specialist's read limit (step 02); `catervas_write_folder_doc`, the `folder_doc.*` events and approving human documents (03); staleness, the re-derive exception and the Files page (04); lanes (05); `product_plan_first` (06); the Product Manager's docs tasks, which step 01b, next, adds; Catervafication (07); the DevOps Engineer's `operations/` (its phase). No UI, no mockups, no migration (ADR 0050, 0051).

## Decisions

- **A module of its own: `crates/core/src/folders.rs`** (`catervas_core::folders`), pure. Readiness, the Definition of Done and the prompt read it. Rejected: `crates/core/src/team.rs`, 3,701 lines about the team file, whose `private_folder` (`team.rs:114`) stays the Finance and Procurement Specialists' and is unrelated.
- **The table.** `ROLE_FOLDERS`, six rows in the design's order: `product_manager` `docs/catervas/product`, `architect` `docs/catervas/architecture`, `software_developer` `docs/catervas/engineering`, `ui_ux_designer` `docs/catervas/design`, `scrum_master` `docs/catervas/delivery`, `marketing_specialist` `docs/catervas/marketing`. `role_folder` reads it; `finance_specialist`, `procurement_specialist` and `human` have none. A folder belongs to a role: two Developers both own `engineering/`. The DevOps role does not exist yet; its phase adds `operations`.
- **Human documents are a fixed list**: `docs/catervas/product/spec.md`, `docs/catervas/product/roadmap.md`, and `docs/catervas/marketing/plans/MP-<n>.md`, `n` decimal digits not starting with `0` (as `plan_file` writes them, `tools/marketing.rs:120`), directly in `plans/`. Paths are repository-relative, normalised by `normalise` (`crates/core/src/governor/paths.rs:87`; a path it refuses is no document), and compared with letter case (git records the index's spelling). A twin is the path with `.agent` before its final `.md`; `agent_twin` and `human_of_twin` return normalised paths, so `./docs/catervas/product/spec.md` gives `docs/catervas/product/spec.agent.md`. Rejected: a glob list, one more matcher to get wrong for three names.
- **`pair_changed_alone`** returns, for each pair exactly one of whose files is in the list, that file, normalised, in first-seen order, each once. Staleness and the re-derive exception (a diff of a stale twin alone) are step 04's, with the owner's edits; in this step a twin changed alone is refused like any other half pair.
- **`reaches_the_folder(glob, folder)`** generalises `reaches_the_marketing_directory` (`paths.rs:143`) to a folder of any number of segments, failing closed as it does: braces expanded (`expand_braces`, `paths.rs:186`), backslashes as `/`, `.` segments dropped; then segment by segment against the folder's: one holding `**` or not compiling reaches; one that does not name the folder's segment (case-insensitive, `names`, `paths.rs:173`) does not; a glob that ends before the folder's depth, or exactly at it, reaches when its last segment is no wildcard (a literal directory names what is under it). So `docs`, `docs/catervas` and `docs/catervas/product` reach `docs/catervas/product`; `docs/*`, `docs/catervas/*.md` and `docs/catervas/product*` do not. A `**` before the folder's depth reaches every folder, as a leading `**` already reaches `.catervas/` for `no_catervas_paths` (`readiness.rs:666`). Rejected: reading the last segment's extension, since a folder holds more than `.md` (`marketing/brand/assets/`). `MARKETING_DIRECTORY` and the old function are removed.
- **`FolderOwned` replaces `MarketingPathsOwned`** (`readiness.rs:58`, `:628`) at its place in rule order, so `CHECKS` stays 22 and `EVERY_RULE` (`plain.rs:73`) 24. Its wire name `folder_owned` comes from `rule_name`'s Debug-to-`snake_case` (`crates/runtime/src/daemon/gates.rs:435`); nothing else names the old rule. A task (not an epic, which names a ceiling) whose assignee role is R is refused when, for a role O ≠ R in `ROLE_FOLDERS` with at least one active agent (`active_agents_by_role`, `readiness.rs:126`), an allowed path reaches O's folder. Message, exact: `allowed paths <the reaching paths, in the contract's order, joined by ", "> could reach folders other roles own: <each reached folder as "<folder>/ (<plain_role>)", in table order, joined by ", ">; name narrower paths, or give the task to the folder's owner`. A folder whose owner has no active agent (none on the team, paused or retired) is anyone's: the Product Manager's task may name `docs/catervas/architecture/plans/**` on a team with no active Architect. Plain words: "Only a folder's owner changes the documents in it, and this plan lets someone else."
- **No Definition of Done rule for ownership**: the diff is held to `allowed_paths` (`PathsWithinAllowed`), which readiness checked against the owners when the task became ready. Rejected: a second reading of the same paths at acceptance.
- **`PairChangedAlone`** is a new `DoneRule` after `NoCatervasPathChanged`, judged on `DoneEvidence.changed_paths` (`done.rs:59`) with no new evidence field. Message, exact: `the diff changes <"<changed> without <its partner>" per file pair_changed_alone returns, joined by ", ">; a document for people and its .agent.md twin change together`.
- **The marketing folder moves** with every rule, prompt, skill, tool text and test that names it (`git grep "docs/marketing" -- crates apps packages docs/schemas`). Task 4 is that one rename; a file it shares with a later task is named in both, the later starting from its text.
- **A marketing plan gets its twin from the tool**, since `PairChangedAlone` would otherwise refuse every plan task. `catervas_propose_marketing_plan` gains a required `agent_text`, "the plan written for agents, 200 to 16,000 characters", checked in the tool beside `check_proposal`'s faults as code `marketing_plan_text`, field `agent_text`, so `PlanProposal` and the store are untouched; it is written to `MP-<n>.agent.md` as `# MP-<n>: <title>\n\n<agent_text>\n` beside `MP-<n>.md`, `call.permit` (`tools/marketing.rs:229`) is given both paths, and both are removed when the event is refused. If the twin cannot be written, the plan file is removed before the refusal returns. `marketing_plan.proposed` keeps `text` alone: the owner reads the human version. `highest_number` (`:455`) already ignores `MP-<n>.agent.md`. Rejected: the agent writing the twin with `Write` after the tool answers (the design gives the tool the twin, and two writes can be split).
- **The Product Manager's product documents leave `.catervas/`**: `catervas_write_product_doc` (`crates/runtime/src/tools.rs:261-265`, `:505`, `:561-565`), `WriteProductDocInput` and `write_product_doc` (`tools/work.rs:149-157`, `:392-430`), `Files::read_product_doc`, `write_product_doc` (`crates/store/src/files.rs:697-714`), `inside_product` (`:1063`) with its helpers `product_path` (`:937`) and `resolved` (`:985`), which nothing else calls, `"product"` in `init`'s directories (`:167`), and `check_product_doc_write` (`crates/core/src/governor/gates.rs:619`), whose one caller was the tool. `product_doc.written` stays in the protocol, the schema and the projections: logs are append-only, and removing a kind is a protocol change for no gain.
- **The Product Manager gets no write tier in this step.** It holds `read` and `network` (`crates/core/src/governor/permissions.rs:52`), and a docs task reaches `verifying` only with a commit (spec 5.6). Giving it `write_workspace` would let its refine and plan sessions, which run at the project root and are not read-only (`crates/runtime/src/orchestrator/requests.rs:169`, `orchestrator/rules.rs:1491`), write the contract's paths in the main checkout. So its prompt names its folder and what goes there, and `writing-requirements` and `scoping-a-release` say, exactly, "You have no tool that writes your folder yet: do not file a task for yourself; give the text to the user in your answer." (step 01b removes that sentence). Step 01b, next, makes every session that is not an implement session about a task read-only and then gives the Product Manager `write_workspace` and `git_local`, and its prompt and skills say "docs tasks" there (the controller's ruling, 2026-10-10). Rejected: the tiers here, before that rule.
- **The prompt knows the active roles** through a new `PromptInput.active_roles`, filled by `session_spec_without` (`crates/runtime/src/orchestrator/session.rs:971`) from `Team::active_agents` (`crates/core/src/team.rs:1221`). `rules_section` (`crates/runtime/src/prompt.rs:433`) ends with the line `folders_line` builds. No new section (ADR 0011). The line lists each role of `ROLE_FOLDERS` that is active or is the reader's own, in table order, as `<folder>/ (<plain_role>)`; then `Yours is <folder>/: write no other folder named here.` or, for a role with none, `You have none: write none of them.`; then `Read any of them, and where a document has an .agent.md twin beside it, read the twin, which is written for agents.`; with nothing to list it is `- folders: none`. Every role is told it reads every folder until step 02.
- **Each owner's prompt names its folder** in one line under "What you produce", from the design's table (exact lines in Tasks 4, 6 and 8). The Scrum Master's names a folder it has no tool to write until step 03; the line says what goes there, not how. The Finance and Procurement prompts are unchanged.

## File map

```
crates/core/src/folders.rs, crates/core/src/lib.rs                creates; registers the module (Task 1)
crates/core/src/governor/paths.rs                                 modifies: reaches_the_folder (Task 2); normalise's doc comment (Task 6)
crates/core/src/governor/readiness.rs                             modifies: the one call site (Task 2); FolderOwned and its tests (Task 3)
crates/core/src/governor/plain.rs                                 modifies: FolderOwned (Task 3)
crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md   modifies: the folder rule (Task 3); after approval (Task 6)
crates/roles/roles/scrum_master/skills/keeping-work-flowing/SKILL.md        modifies: the folder rule (Task 3)
crates/roles/src/lib.rs                                           tests: Tasks 3, 4, 6, 8 (each its own tests)
crates/roles/roles/marketing_specialist/{role.yaml,system.md,skills/*/SKILL.md}, ui_ux_designer/skills/brand-and-design-tokens/SKILL.md, crates/roles/src/kit.rs   modifies (Task 4)
crates/runtime/src/tools/marketing.rs, tools/posts.rs, daemon/hooks.rs, docs/schemas/event.schema.json   modifies (Task 4)
crates/runtime/src/tools.rs                                       modifies: a description (Task 4); the tool removed (Task 6)
crates/core/src/governor/gates.rs                                 modifies: a test's path (Task 4); check_product_doc_write removed (Task 6)
crates/core/src/governor/done.rs                                  modifies: PairChangedAlone (Task 5)
crates/runtime/src/tools/work.rs, daemon/mcp.rs, crates/store/src/files.rs, crates/store/tests/project_files.rs, crates/core/src/governor/transition.rs   modifies (Task 6)
docs/schemas/event.schema.json                                    modifies: marketing_plan.proposed (Task 4); product_doc.written (Task 6)
crates/roles/roles/product_manager/{role.yaml,system.md,skills/writing-requirements/SKILL.md,skills/scoping-a-release/SKILL.md}   modifies (Task 6)
crates/runtime/src/prompt.rs, crates/runtime/src/orchestrator/session.rs   modifies (Task 7)
crates/roles/roles/{architect,software_developer,ui_ux_designer,scrum_master}/system.md   modifies (Task 8)
docs/SPEC.md, docs/design/catervas-folders.md, docs/plans/project-plan.md   modifies (Task 9)
```

## Interfaces

Consumes: `normalise`, `expand_braces`, `compiles`, `names`, `is_a_wildcard` (`crates/core/src/governor/paths.rs:87`, `:186`, `:168`, `:173`, `:180`); `Role`, `plain_role` (`team.rs:87`), `Team::active_agents` (`team.rs:1221`); `ReadinessContext` (`readiness.rs:120`); `DoneEvidence` (`done.rs:54`); `PromptInput` (`prompt.rs:19`); `propose_plan`, `plan_file`, `write_new` (`tools/marketing.rs:152`, `:120`, `:472`); `load_role`, `load_kit`, `SHIPPED_ROLES` (`catervas-roles`); all on main.

Produces:

```rust
// catervas_core::folders
pub const ROLE_FOLDERS: [(Role, &str); 6];
pub fn role_folder(role: Role) -> Option<&'static str>;
pub fn is_human_document(path: &str) -> bool;
pub fn agent_twin(path: &str) -> Option<String>;
pub fn human_of_twin(path: &str) -> Option<String>;
pub fn pair_changed_alone(changed_paths: &[String]) -> Vec<String>;
pub fn folders_line(role: Role, active_roles: &[Role]) -> String;
// catervas_core::governor::paths
pub fn reaches_the_folder(glob: &str, folder: &str) -> bool;
// ReadinessRule::FolderOwned replaces ReadinessRule::MarketingPathsOwned; DoneRule::PairChangedAlone
// catervas_runtime::prompt::PromptInput gains `pub active_roles: &'a [Role]`
// ProposeMarketingPlanInput gains `agent_text: String`
```

## Tasks

### Task 1: The folders and their human documents

Files: created `crates/core/src/folders.rs` (tests in its `mod tests`); modified `crates/core/src/lib.rs` (`pub mod folders;` with its doc comment).
Produces: every `catervas_core::folders` item above. Consumes: `normalise`, `plain_role`.

- `each_owning_role_has_its_folder` — `role_folder` gives the six paths of the table for the six roles, and `None` for the Finance Specialist, the Procurement Specialist and the human; `ROLE_FOLDERS` is in the table's order. RED: no module.
- `names_the_human_documents_and_no_other` — true for `docs/catervas/product/spec.md`, `docs/catervas/product/roadmap.md`, `docs/catervas/marketing/plans/MP-1.md`, `docs/catervas/marketing/plans/MP-12.md` and `./docs/catervas/product/spec.md`; false for `docs/catervas/product/spec.agent.md`, `docs/catervas/product/Spec.md`, `docs/catervas/product/notes.md`, `docs/catervas/architecture/spec.md`, `product/spec.md`, `../docs/catervas/product/spec.md`, and `MP-0.md`, `MP-01.md`, `MP-x.md`, `MP-1.agent.md` and `sub/MP-1.md` under `docs/catervas/marketing/plans/`. RED: no function.
- `pairs_a_human_document_with_its_twin` — `agent_twin` of each human document above is its `.agent.md` path, and `human_of_twin` of that is the document again; `agent_twin` of `notes.md` and of a twin, and `human_of_twin` of `notes.agent.md` and of `spec.md`, are `None`. RED: no functions.
- `finds_a_pair_changed_alone` — `[spec.md, src/x.rs]` gives `[spec.md]`; `[spec.agent.md]` gives `[spec.agent.md]`; `[spec.md, spec.agent.md]` gives nothing, and so does `[./docs/catervas/product/spec.md, spec.agent.md]`; `[roadmap.md, spec.agent.md, spec.md, roadmap.md]` gives `[roadmap.md]` once; `[docs/catervas/marketing/plans/MP-2.md]` gives it; `[notes.md]` gives nothing. Every bare name here stands for its full path under `docs/catervas/product/`, and the result holds full, normalised paths. RED: no function.
- `lists_the_active_roles_folders_in_table_order` — for the Product Manager with active `[SoftwareDeveloper, ProductManager, MarketingSpecialist, FinanceSpecialist]` the line is exactly `- folders: docs/catervas/product/ (Product Manager), docs/catervas/engineering/ (Software Developer), docs/catervas/marketing/ (Marketing Specialist). Yours is docs/catervas/product/: write no other folder named here. Read any of them, and where a document has an .agent.md twin beside it, read the twin, which is written for agents.` RED: no function.
- `tells_a_role_without_a_folder_to_write_none` — for the Finance Specialist with active `[ProductManager]` the line is `- folders: docs/catervas/product/ (Product Manager). You have none: write none of them. Read any of them, …` (the sentence above); with no active role it is `- folders: none`. RED.
- `lists_the_readers_own_folder_when_it_is_not_active` — for the Architect with active `[ProductManager]`, both folders are listed, the Architect's second. RED.

- [x] `feat(core): add each role's folder and its human documents`

### Task 2: `reaches_the_folder`

Files: modified `crates/core/src/governor/paths.rs` (`reaches_the_folder`; `MARKETING_DIRECTORY` and `reaches_the_marketing_directory` removed; the test at `:260` replaced); `crates/core/src/governor/readiness.rs` (the import at `:7-10` and the call at `:645` become `reaches_the_folder(path, "docs/marketing")`, only so it compiles).
Produces: `reaches_the_folder`. Consumes: the private helpers of `paths.rs`.

- `reaches_a_folder_as_the_rule_says` — for `docs/catervas/marketing`, each of `docs/**`, `**/*.md`, `**`, `Docs/Catervas/Marketing/x.md`, `docs/catervas/{marketing,adr}/**`, `./docs/catervas/marketing`, `docs`, `docs/catervas`, `docs[/]catervas/marketing/x`, `d*/c*/m*/x`, `docs/catervas/marketing/**`, `DOCS\CATERVAS\MARKETING\x.md`, `docs/catervas/**` and `docs/*/marketing/**` reaches; none of `docs/adr/**`, `src/**`, `*.md`, `docs/*.md`, `*`, `docs/*`, `docs/catervas/*.md`, `docs/catervas/marketing*`, `docsx/catervas/marketing/x`, `docs/catervas/marketingx/**`, `docs/catervas/product/**` and the empty glob does. For `docs/catervas/product`, `docs/*/product/**` reaches and `docs/catervas/productx/**` does not. RED: no function.

- [x] `refactor(core): read any folder's reach from a glob`

### Task 3: Readiness keeps each folder to its owner

Files: modified `readiness.rs` (`FolderOwned`, `folder_owned` in place of `marketing_paths_owned`, its import of `ROLE_FOLDERS` and `plain_role`; tests), `plain.rs` (the sentence, `EVERY_RULE`, `listed`), the two planners' skills (the paragraph at `writing-task-contracts/SKILL.md:55-57`, whose label at `:55` becomes "Keep other roles off the owners' folders", and `keeping-work-flowing/SKILL.md:52-53`), `crates/roles/src/lib.rs` (`:1227`).
Consumes: `ROLE_FOLDERS` (Task 1), `reaches_the_folder` (Task 2).

- `another_role_may_not_name_an_owned_folder` (replaces `:1385`) — in `a_ready_context()` (Product Manager, Scrum Master, Architect, Developer and Marketing Specialist active): an Architect's task reviewed by the Product Manager with `[docs/adr/**, docs/**]` fails `[FolderOwned]` alone, message exactly `allowed paths docs/** could reach folders other roles own: docs/catervas/product/ (Product Manager), docs/catervas/engineering/ (Software Developer), docs/catervas/delivery/ (Scrum Master), docs/catervas/marketing/ (Marketing Specialist); name narrower paths, or give the task to the folder's owner`; an Architect's task with `[docs/**, docs/catervas/delivery/x.md]` fails `[FolderOwned]` with exactly `allowed paths docs/**, docs/catervas/delivery/x.md could reach folders other roles own: docs/catervas/product/ (Product Manager), docs/catervas/engineering/ (Software Developer), docs/catervas/delivery/ (Scrum Master), docs/catervas/marketing/ (Marketing Specialist); name narrower paths, or give the task to the folder's owner`; a Product Manager's task reviewed by the Architect with `[docs/catervas/delivery/**]` fails `[FolderOwned]`, and passes with the Scrum Master removed and with its count 0; the Architect's own `[docs/catervas/architecture/**, docs/adr/**]` and a Developer's `[docs/catervas/engineering/**]` pass; a Developer's `[src/**]` passes and `[docs/**/*.rs]` fails `[FolderOwned]`; with a UI/UX Designer active, a Developer's `[docs/catervas/design/x.md]` fails naming `docs/catervas/design/ (UI/UX Designer)`; an Architect's epic with `[docs/**]` does not fail `FolderOwned`. RED: no such rule.
- Existing tests of other rules whose task names a path reaching an active owner's folder keep their subject and their exact lists: an incidental `docs`, `docs/**` or `docs/**/*.md` narrows to `docs/adr`, `docs/adr/**`, `docs/adr/**/*.md` (`:1316`, `:1333-1340`, `:1364`, `:1715`, `:1790`); where the reaching path is the subject (`**/*.md` at `:1302`, `docs*/**` and `docs{,rc}/**` at `:1346`), the expected list gains `R::FolderOwned` in rule order. `a_context_without_marketing` (`:1376`) and `a_context_without_marketing_under_a_ceiling` (`:1797`) are removed, their callers given `a_ready_context()`, since no single role's absence now frees `docs/**`. `refuses_every_document_task_with_an_empty_list` (`:1913`) names `docs/catervas/marketing/**`.
- `the_folder_sentence_names_no_role` (`plain.rs`) — the plain words of `FolderOwned` contain "folder" and none of "marketing", "product manager", "architect". RED: they name the Marketing Specialist.
- `the_planners_keep_other_tasks_off_the_owners_folders` (replaces `lib.rs:1227`) — both skills hold, word for word, "While a role that owns a folder under docs/catervas/ has an active agent, another role's task names no path that could reach that folder (not docs/**, docs/catervas/**, docs, ** or **/*.md); name the folder it needs, such as docs/adr/** or src/**/*.rs." RED: they hold the marketing sentence.

- [x] `feat(core): keep each role's folder to its owner at readiness`

### Task 4: The marketing folder moves, and a plan gets its twin

Files: every `docs/marketing` of `git grep -n "docs/marketing" -- crates apps packages docs/schemas` outside Tasks 2 and 3 (`gates.rs:2218`; the Marketing Specialist's `role.yaml`, `system.md` and nine skills; `brand-and-design-tokens/SKILL.md:13`; `kit.rs:2394`, `:2515`; `lib.rs:1073-1075`, `:1154-1157`, `:1218`; `daemon/hooks.rs:1299`; `tools.rs:419`; `tools/posts.rs:583`; `tools/marketing.rs`; `event.schema.json:1066`). In the Marketing Specialist's `system.md`, `:26` becomes "Your folder is `docs/catervas/marketing/`, which only you write while you are on the team, and everyone reads." and the plan's row names `MP-<n>.md` with its `MP-<n>.agent.md`. `tools/marketing.rs` gains `agent_text` as the Decisions say; `writing-the-marketing-plan/SKILL.md` says to give `agent_text`, the plan for agents, and to commit `MP-<n>.md` and `MP-<n>.agent.md` together.

- `no_shipped_text_names_the_old_marketing_folder` (`lib.rs`) — no shipped role's `system.md`, `role.yaml` lines, role skill or kit skill session file holds `docs/marketing/`. RED: the Marketing Specialist's prompt does.
- `the_plan_skill_commits_the_plan_and_its_twin` (`lib.rs`) — `writing-the-marketing-plan` names `agent_text`, `MP-<n>.agent.md` and `catervas_git_commit` with both files. RED.
- `proposes_a_plan_and_writes_its_text` (`tools/marketing.rs:638`, updated) — `a_plan()` gains `agent_text` of 300 `y`; the worktree holds `docs/catervas/marketing/plans/MP-1.md` as before and `MP-1.agent.md` exactly `# MP-1: Spring launch\n\n` + 300 `y` + `\n`; the project root holds neither; the event's `text` is the 300 `x`. RED: the old path, and no field.
- `refuses_a_plan_without_its_agent_twin` — no `agent_text` is `InvalidInput`; 199 characters is `marketing_plan_faults` with a fault of code `marketing_plan_text` and field `agent_text`; neither refusal writes a file in the worktree's plans folder or records an event; with `MP-1.agent.md` already in the worktree, the call is refused `marketing_plan_file_exists` and no `MP-1.md` remains. RED: the field is unknown.
- The other tests of `marketing.rs`, `posts.rs`, `hooks.rs`, `kit.rs` and `lib.rs` that name the folder follow it unchanged in what they assert.

- [x] `feat(runtime): move the marketing folder under docs/catervas/ with each plan's twin`

### Task 5: Done refuses half a pair

Files: modified `crates/core/src/governor/done.rs` (`PairChangedAlone`, `CHECKS` 11, tests). Consumes: `pair_changed_alone`, `agent_twin`, `human_of_twin` (Task 1).

- `refuses_a_diff_that_changes_one_file_of_a_pair` — allowed `docs/catervas/**`: `[docs/catervas/product/spec.md]` fails `[PairChangedAlone]` with exactly `the diff changes docs/catervas/product/spec.md without docs/catervas/product/spec.agent.md; a document for people and its .agent.md twin change together`; `[docs/catervas/product/roadmap.agent.md]` names `roadmap.agent.md without …/roadmap.md`; `[docs/catervas/marketing/plans/MP-2.md]` fails; `[spec.md, roadmap.agent.md]` names both, joined by `, `; `[spec.md, spec.agent.md, docs/catervas/architecture/overview.md]` passes. RED: no rule.
- `reports_every_failure_in_rule_order` (`:864`, updated) — the changed paths gain `docs/catervas/product/roadmap.md`, the order gains `R::PairChangedAlone` after `R::NoCatervasPathChanged`, and the `PathsWithinAllowed` message names it. RED: no rule.

- [x] `feat(core): refuse a diff that changes one file of a human document's pair`

### Task 6: The Product Manager's documents move to its folder

Files: `tools.rs` (descriptor, call arm, `paths_of` arm and its doc, `lists_every_tool_with_its_tier` at `:740` without the tool and with `[..40]` at `:809`); `tools/work.rs` (input, handler, imports; tests `:991`, `:1012` deleted); `daemon/mcp.rs:483` (47); `files.rs` (the Decisions' items; tests `:1256`, `:1274` deleted); `tests/project_files.rs` (`:58`; tests `:409`, `:727`, `:758`, `:792`, `:817`, `:829` deleted, the first because no writer left takes a name a tool chose); `gates.rs` (`check_product_doc_write`, its tests `:2356`, `:2455`); `transition.rs:1303` (the comment names "the approval 5.16 item 2 asks for"); `paths.rs:80-85` (`normalise`'s doc names its callers, `criteria.rs` and `exec.rs`); `event.schema.json:1708` (`product_doc.written`'s description becomes "No longer written (spec 0.84); kept so older logs read. path was relative to .catervas/product/."); the Product Manager's `role.yaml` (`produces` line 15 becomes "the product's spec and roadmap in docs/catervas/product/, each with its .agent.md twin, and sprint reports"), `system.md` (`:24-25` becomes two bullets, "- Product decisions." and "- Your folder, `docs/catervas/product/`, which only you write and everyone reads: the product's `spec.md` and `roadmap.md`, each for people with its `.agent.md` twin for agents, the two changed together, and your sprint reports."; `:32` loses "The governor refuses the write."), `writing-requirements` (the requirements belong in `spec.md` and `spec.agent.md`, changed together; then, exactly, "You have no tool that writes your folder yet: do not file a task for yourself; give the text to the user in your answer."), `scoping-a-release` (`:31-32`: the notes belong in the folder, then the same sentence), `writing-task-contracts` §6 (`:98`: after approval the epic's requirements go into the spec by `writing-requirements`); `lib.rs`.

- `the_product_manager_keeps_its_folder` (`lib.rs`) — `system.md` names `docs/catervas/product/`, `spec.md`, `roadmap.md` and `.agent.md`; `writing-requirements` names `spec.md` and `spec.agent.md`; neither `system.md` nor any Product Manager skill names `.catervas/product` or `catervas_write_product_doc`; `produces` names `docs/catervas/product/`. RED: they name `.catervas/product/`.
- `makes_the_layout_a_project_starts_with` (`project_files.rs:43`, updated) — `init` makes `team`, `contracts`, `agents`, `decisions` and `local`, and no `.catervas/product`. RED: it is made.
- `lists_every_tool_with_its_tier` and `lists_every_catervas_tool_and_the_permission_tool`, updated as above. RED: the tool is listed.
- `kit_skills_name_only_tools_catervas_lists` (`daemon/team.rs:5354`) holds unchanged: no skill names the removed tool.

- [x] `feat(runtime): keep the Product Manager's documents in docs/catervas/product/`

### Task 7: Every session's Team rules name the folders

Files: `prompt.rs` (`PromptInput.active_roles`, `rules_section` taking it and the reader's role, `Inputs::full` passing `&[]`, tests); `orchestrator/session.rs` (`:1031-1051` fills `active_roles`; a test). Consumes: `folders_line` (Task 1).

- `writes_each_team_rule_on_its_own_line` (`prompt.rs:1065`, updated) — both expected sections end with `- folders: docs/catervas/product/ (Product Manager). Yours is docs/catervas/product/: write no other folder named here. Read any of them, and where a document has an .agent.md twin beside it, read the twin, which is written for agents.` RED: no folders line.
- `the_team_rules_name_the_active_roles_folders` (`prompt.rs`) — an Architect with active `[ProductManager, ScrumMaster, Architect, FinanceSpecialist]`: the Team rules' last line holds `docs/catervas/product/ (Product Manager), docs/catervas/architecture/ (Architect), docs/catervas/delivery/ (Scrum Master). Yours is docs/catervas/architecture/`; the headings are still `PROMPT_SECTIONS`. RED.
- `the_prompt_lists_the_folders_of_the_active_roles` (`session.rs`, integration) — a team of three `with_the_designer` (`tools/fixtures.rs:50`) whose Architect `ada` is paused: the Product Manager's triage session prompt holds `docs/catervas/engineering/ (Software Developer)` and `docs/catervas/design/ (UI/UX Designer)` and not `docs/catervas/architecture/`. RED: no folders line.

- [x] `feat(runtime): tell every session the team's folders`

### Task 8: Each owner's prompt names its folder

Files: the `system.md` of the Architect, the Developer, the UI/UX Designer and the Scrum Master, one line each under "What you produce"; `lib.rs`.
Lines, exactly: "- Your folder, `docs/catervas/architecture/`, which only Architects write while one is active, and everyone reads: `overview.md` with Mermaid diagrams, `features.md`, `jobs.md` (the cron and scheduled work found in code, CI and config), `structure.md`, and `plans/<epic-id>.md`."; "- Your folder, `docs/catervas/engineering/`, which only Developers write while one is active, and everyone reads: `conventions.md` and `workflow.md`, each rule with the evidence it was read from."; "- Your folder, `docs/catervas/design/`, which only UI/UX Designers write while one is active, and everyone reads: the UI inventory and design notes."; "- Your folder, `docs/catervas/delivery/`, which only Scrum Masters write while one is active, and everyone reads: the sprint cadence and ceremony notes."

- `every_owner_s_prompt_names_its_folder` (`lib.rs`) — for each row of `ROLE_FOLDERS`, the role's system prompt holds `<folder>/`; the Finance and Procurement Specialists' hold no `docs/catervas/`. RED: the four prompts name none.

- [ ] `feat(roles): name each owner's folder in its prompt`

### Task 9: Spec

`docs/SPEC.md`: 3 (a project holds `docs/catervas/`, 5.17); 5.3 (the `marketing_paths_owned` bullet becomes `folder_owned`, its message and plain words, and that a folder whose owner has no active agent is anyone's); 5.4 (item 2 gains `pair_changed_alone`, a twin alone included until step 04); 5.6 ("writing a product document" leaves the coordination tools; the Product Manager's tiers unchanged); 5.16 item 1 and the paragraph on `.catervas/product/` (requirements go in `docs/catervas/product/spec.md` and its twin, only for an approved epic, by the prompt; the tool and its gate are gone); 5.17 Role folders (added in 0.84): the table, ownership, the human-document list and twins, the Team rules' line, what later steps add; 6.1 (Produces, Cannot), 6.2, 6.3, 6.4, 6.8 (each folder), 6.5 (paths, `agent_text`); 8.5 (`product_doc.written` no longer written, kept for older logs); revision 0.84. 5.8 names no `.catervas/product/` and is unchanged. `docs/design/catervas-folders.md`: the Writing sentence "The Definition of Done refuses a diff that reaches another role's folder" becomes "Ownership is held at readiness: the Definition of Done holds the diff to `allowed_paths`, which `folder_owned` checked when the task became ready; an owner made active after that does not reopen it."; the steps table gains row 01b, "The Product Manager writes its folder": no write outside a task's implement session, then the Product Manager's `write_workspace` and `git_local` and its docs tasks. `docs/plans/project-plan.md`: row 01's Spec becomes `3, 5.3, 5.4, 5.6, 5.16, 5.17, 6, 8.5` and its Delivers gains "each marketing plan's agent twin (`agent_text`)"; row 03 drops "the marketing plan's agent version".

- [ ] `docs(spec): record role folders and their ownership`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Tasks 4, 6 and 7 name integration tests
# expected: xtask check: ok
git grep -n "docs/marketing\|catervas_write_product_doc\|\.catervas/product" -- crates apps packages
# expected: no output
```
