# Standalone: the team writes its documents as HTML

Status: draft
Branch: `feat/html-documents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.5 and 5.17 (and the kit lines of 6.x), revision 0.107; ADR 0058
Depends on: `origin/main` at `9e3d4f3c` (#46 merged; phase 8 in #34)
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

`docs/design/html-documents.md` (2026-10-12) is the source of truth for behavior and copy; this plan does not restate it. Done means: a team has `policy.document_format` (`html` unless set); agents write new documents under `docs/catervas/` in it, HTML pages with inline SVG and no scripts, while an existing `.md` stays Markdown; every fixed document (spec, roadmap, plans, Catervafication documents, marketing plans) is found in either format; the Files page shows an HTML document in a sandboxed frame from `GET /documents/raw`, fitted to the screen, with "Open full page", "Ask for a change" and, under Advanced, "Edit source"; Settings → Work has the format switch and "Rewrite documents as HTML". Out of scope: as the design's list.

## Decisions

- `DocumentFormat { Markdown, Html }` lives in `catervas_core::folders` with `extension()` (`"md"`, `"html"`) and `of(path) -> Option<DocumentFormat>`; the schema's `policy.document_format` (`"html" | "markdown"`) generates its own type, and `Team::document_format()` maps it, `Html` when absent. Chose a core type over using the generated one everywhere: the folder rules are core and must not depend on the wire's naming.
- A fixed document is named by its **stem** (`docs/catervas/product/spec`); `DOCUMENT_NAMES`, `OWNER_ACCEPTED`, `PRODUCT_PLAN` hold stems and every comparison goes through `stem_of(path) -> Option<&str>` (the path without `.md` or `.html`, `None` for any other extension). An agent version's stem ends `.agent`. Chose stems over listing both extensions: one list, one truth.
- `check_document_format(path, format, on_branch: &dyn Fn(&str) -> bool) -> Result<Option<String>, FormatRefusal>` holds a write: `Ok(Some(other))` names the same stem's other-format file to remove, `Ok(None)` writes alone, `Err(WrongFormat { instead })` or `Err(OtherFormatExists { existing })` (only for `OWNER_ACCEPTED` stems). Pure; the tool passes `git.has_file` on the integration branch.
- `Git::commit_files` gains `removed: &[String]` (deleted at the root, staged by `git add --`, committed by `commit --only`; `take_back` restores them from `from`; `Unchanged` also needs each removed path absent at `from`); `land` gains the same parameter, every other caller passing `&[]`. Chose a parameter over `Option<String>` texts: callers that never remove stay as they read.
- The document route answers only `.html`. Its handler is `documents::raw`, routed in `daemon.rs`'s browser router as `GET /documents/raw`; it checks the host and session through a new `web::has_session(headers, web) -> bool` that `web::session` also uses. Without `task` it answers the text `files.read` would show for the path (the human document's waiting change, or the twin's integration text); with `task` it reads `task_branch(contract)`. Errors: 403 foreign host, 401 no session, 404 anything else, all with an empty body.
- The fit block (`FIT`, a `const &str`) and `fit_to_screen(html: &str) -> String` are private to `daemon/documents.rs`; insertion is after the first `<head…>`, else `<html…>`, else `<!doctype…>` (ASCII case-insensitive), else at 0. `ponytail:` a tag found by text, not by parsing; a parser if a document defeats it.
- Ask for a change and the rewrite are RPC methods of `documents.rs` (`METHODS` gains `files.ask_change`, `files.rewrite`), as `request.file` is a method; chose methods over commands, since neither moves a task the orchestrator owns.
- Both file through a new `catervas_store::requests::file_small_request(files, log, wire, created_by, reason, (now, ids))`: `task.created` then `request.triaged { small, reason, triaged_by: "catervas" }`, through a new `Filing.small: Option<&str>`. Contracts: `change_request(path, twin: Option<&str>, words, owner, reviewer, max_cost_usd) -> Value` and `rewrite_request(folder, documents: &[String], owner, reviewer, max_cost_usd) -> Value`, beside `rederive_request`. The reviewer is chosen as `file_rederive` chooses it (moved into `catervas_roles` as `reviewer_for(team, owner) -> Option<Role>`, used by all three).
- `document_rewrite.started { tasks }` (1 to 6 task ids) is a new event; `catervas_store::requests::rewrite_tasks(log) -> Vec<TaskId>` reads the last one's. `rewrite_open` while any of them is on the board and neither `accepted` nor `cancelled`.
- `task.created`'s `rederives` pattern becomes `^docs/catervas/.+\.agent\.(md|html)$`.
- The retro's delivery line: `retro_message` gains `rewrite_delivery: bool`, true when a rewrite was ever started, the format is `Html`, and the integration branch holds a `.md` file under `docs/catervas/delivery/`.
- Prompts and skills name documents by stem; the format line is `catervas_core::folders::format_line(format) -> String`, appended by `rules_section` after `folders_line`; `folders_line` says "an agent version (`.agent.html` or `.agent.md`)".
- Web: `isHtml(path)` in `files.ts`; one component `apps/web/src/components/DocumentFrame.tsx` (`{ path, task?, title }`) draws the frame and "Open full page"; `FileDocument.tsx`, `ProductPlanCard.tsx` use it. Settings: `Documents(part)` in `TeamRules.tsx`, a `Switch` as `SprintsSwitch` is, and the rewrite button under it. `DocumentChanges.tsx`'s `plain()` strips `<…>` for a path ending `.html`.
- No new dependency.

## File map

```
crates/core/src/folders.rs                         modifies: DocumentFormat, stem_of, pairs, fixed lists, check_document_format, format_line, headings (Task 1)
docs/schemas/team.schema.json, team-template.schema.json   modifies: policy.document_format (Task 2)
crates/core/src/team.rs, crates/core/src/team/template.rs  modifies: document_format(), templates carry it (Task 2)
crates/store/src/git.rs                            modifies: commit_files removed, take_back (Task 3)
crates/runtime/src/tools/folders.rs                modifies: land removed, the format check, plain_names (Tasks 3, 4)
crates/runtime/src/tools.rs                        modifies: the tool's description (Task 4)
crates/core/src/governor/readiness.rs              modifies: a plan in either format (Task 5)
crates/runtime/src/orchestrator/{rules,messages,requests}.rs  modifies: fixed paths resolved, retro line (Tasks 5, 10)
crates/runtime/src/daemon/gates.rs                 modifies: product_plan.state in either format (Task 5)
crates/store/src/requests.rs                       modifies: product_plan_request(format), Filing.small, change/rewrite requests, rewrite_tasks (Tasks 5, 9, 10)
crates/runtime/src/tools/marketing.rs              modifies: plan files by format, HTML wrapper (Task 5)
crates/runtime/src/catervafication.rs              modifies: rows by stem, criteria by format (Task 5)
crates/runtime/src/prompt.rs                       modifies: rules_section's format line (Task 6)
crates/roles/roles/**/{system.md,role.yaml,skills/*/SKILL.md}, crates/roles/src/lib.rs  modifies: stems, tests (Task 6)
crates/runtime/src/daemon/documents.rs             modifies: tree/read/save, raw, fit, ask_change, rewrite (Tasks 7, 8, 9, 10)
crates/runtime/src/daemon.rs, daemon/web.rs        modifies: the route, has_session (Task 8)
crates/roles/src/lib.rs                            modifies: reviewer_for (Task 9); its kit tests (Task 6)
docs/schemas/rpc.schema.json, event.schema.json    modifies: the two methods, the event, rederives (Tasks 9, 10)
apps/web/src/components/DocumentFrame.tsx (+ .module.css, .test.tsx)  creates (Task 11)
apps/web/src/pages/{FileDocument,ProductPlanCard,DocumentChanges}.tsx, files.ts, Files.module.css  modifies (Task 11)
apps/web/src/pages/{TeamRules.tsx,settings/Work.tsx}, app/refusals.ts, strings/en.ts  modifies (Task 12)
docs/SPEC.md, docs/design/catervas-folders.md      modifies: revision 0.107 (Task 13)
```

## Interfaces

Consumes: `normalise`, `role_folder`, `is_human_document`, `agent_twin`, `human_of_twin`, `check_owner_edit` (`catervas_core::folders`, on main); `land`, `file_rederive` (`tools/folders.rs`); `rederive_request`, `file`, `Filing` (`store/requests.rs`); `task_branch`, `integration_branch`; `useRuleParts`, `ShowAdvanced`, `Switch`, `Bullets`, `InfoTip`.

Produces:

```rust
// catervas_core::folders
pub enum DocumentFormat { Markdown, Html }
impl DocumentFormat { pub fn extension(self) -> &'static str; pub fn of(path: &str) -> Option<Self>; }
pub fn stem_of(path: &str) -> Option<&str>;
pub fn with_format(stem: &str, format: DocumentFormat) -> String;
pub fn plan_stem(epic: &str) -> String;                      // replaces plan_path
pub fn is_plan_of(path: &str, epic: &str) -> bool;
pub enum FormatRefusal { WrongFormat { instead: String }, OtherFormatExists { existing: String } }
pub fn check_document_format(path: &str, format: DocumentFormat, on_branch: &dyn Fn(&str) -> bool) -> Result<Option<String>, FormatRefusal>;
pub fn format_line(format: DocumentFormat) -> String;
// FolderDocPathRefusal::NotMarkdown and OwnerEditRefusal::NotMarkdown become NotADocument (code "not_a_document")
// catervas_core::team
impl Team { pub fn document_format(&self) -> DocumentFormat; }
// catervas_store
impl Git { pub fn commit_files(&self, branch: &str, from: &str, files: &[(String, String)], removed: &[String], message: &str, author: Option<&str>) -> Result<CommitOutcome, GitError>; }
pub fn change_request(path: &str, twin: Option<&str>, words: &str, owner: Role, reviewer: Role, max_cost_usd: f64) -> Value;
pub fn rewrite_request(folder: &str, documents: &[String], owner: Role, reviewer: Role, max_cost_usd: f64) -> Value;
pub fn file_small_request(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str, reason: &str, at: (DateTime<Utc>, &EventIds)) -> Result<TaskContract, RequestError>;
pub fn rewrite_tasks(log: &EventLog) -> Result<Vec<TaskId>, StoreError>;
pub fn product_plan_request(reviewer: Role, max_cost_usd: f64, format: DocumentFormat) -> Value;
// catervas_roles
pub fn reviewer_for(team: &Team, owner: Role) -> Option<Role>;
```

```ts
// apps/web
export function isHtml(path: string): boolean;                                  // files.ts
export function DocumentFrame(props: { path: string; task?: string; title: string }): JSX.Element;
```

## Tasks

### Task 1: documents in either format, in the core

Files: modified `crates/core/src/folders.rs`, tested in its `mod tests`. Every caller of a renamed item is updated in the same commit (`plan_path` → `plan_stem`/`is_plan_of`, `NotMarkdown` → `NotADocument`).

- `stem_drops_md_or_html_and_nothing_else` — `stem_of` of `a/b.md`, `a/b.html` is `a/b`; of `a/b.txt`, `a/b` is `None`; of `a/spec.agent.html` is `a/spec.agent`.
- `pairs_in_either_format` — `is_human_document` holds for `product/spec.md`, `spec.html`, `roadmap.html`, `marketing/plans/MP-3.html`, not for `MP-03.html` or `spec.htm`; `agent_twin("…/spec.html")` is `…/spec.agent.html`; `human_of_twin("…/spec.agent.html")` is `…/spec.html`; `…/spec.agent.md` still maps to `…/spec.md`.
- `pair_changed_alone_holds_html_pairs` — `[spec.html]` alone is reported; `[spec.html, spec.agent.html]` is not; `[spec.md, spec.agent.html]` reports both.
- `owner_documents_and_product_plan_by_stem` — `may_change_an_owner_document(["docs/catervas/product/spec.html"])` is true; `product_plan_approved` holds for `spec.html` with `roadmap.md`, not for `spec.html` alone.
- `folder_doc_path_takes_md_and_html` — `check_folder_doc_path` accepts `architecture/overview.html` for the Architect, refuses `overview.txt` with `NotADocument`, `spec.agent.html` with `AgentTwin`, `marketing/plans/MP-2.html` with `MarketingPlan`; `owner_accepted` for `product/spec.html`.
- `owner_edit_takes_md_and_html` — `check_owner_edit` accepts `product/roadmap.html`, refuses `.txt` as `not_a_document`, `roadmap.agent.html` as `agent_twin`.
- `format_check_writes_the_teams_format` — Html, nothing on the branch: `overview.html` is `Ok(None)`; `overview.md` is `WrongFormat { instead: "…/overview.html" }`.
- `format_check_keeps_an_existing_documents_format` — Html, `cadence.md` on the branch: `cadence.md` is `Ok(None)`; `cadence.html` is `Ok(Some("…/cadence.md"))`; Markdown, `overview.html` there: `overview.html` is `Ok(None)`.
- `format_check_refuses_a_spec_changing_format` — Html, `product/spec.md` there: `product/spec.html` is `OtherFormatExists { existing: "…/spec.md" }`.
- `document_names_by_stem` — `document_name("…/product/spec.html")` is "Product description"; `document_order` places `architecture/overview.html` where `overview.md` was; `plans/CTV-3.html` with a title is "Plan: …"; `plain_name` of a stem from `notes-on-x.html` is "Notes on x".
- `plans_in_either_format` — `is_plan_of("docs/catervas/architecture/plans/CTV-3.html", "CTV-3")` and `.md` hold; `CTV-30.md` does not.
- `format_line_says_each_format` — `format_line(Html)` contains `.agent.html`, `inline SVG`, `no scripts`, `viewport`, `one block element per line` and `stays Markdown`; `format_line(Markdown)` contains `.agent.md`, `Mermaid` and `stays HTML`; both start `- documents:`.
- `html_headings_head_a_run` — `changed_lines` of an HTML page whose changed line sits under `<h2>Risks</h2>` shows that heading line as the run's nearest heading.
- `folders_line_names_either_agent_version` — `folders_line` says `.agent.html or .agent.md`.

- [ ] `feat(core): a team document is Markdown or HTML`

### Task 2: the team's document format

Files: modified `docs/schemas/team.schema.json` (`$defs/policy.document_format`, enum `["html","markdown"]`, its description per the design), `docs/schemas/team-template.schema.json` (the same key), `crates/core/src/team.rs`, `crates/core/src/team/template.rs` (carried as `plan_in_sprints` is), tested in their `mod tests`.

- `document_format_is_html_unless_markdown` — a team without the key and with `"html"` is `Html`; with `"markdown"` is `Markdown`; a file with `"pdf"` is refused by validation.
- `templates_carry_the_document_format` — a template saved from a Markdown team holds `"markdown"`; a team made from it holds it; a template without it leaves the team's key absent.

- [ ] `feat(core): policy.document_format, HTML unless set`

### Task 3: a folder change may remove files

Files: modified `crates/store/src/git.rs` (`commit_files`, `commit_files_on_the_new_branch`, `take_back`), `crates/runtime/src/tools/folders.rs` (`land` gains `removed`, refuses a removed path held by a waiting change as it does a written one), every caller of both passing `&[]`; tested in `git.rs`'s and `tools/folders.rs`'s `mod tests`.

- `commits_a_removal_with_the_writes` — writing `b.html` and removing `b.md` makes one commit on the new branch whose tree has `b.html` and no `b.md`; the root's checkout is back on its branch with `b.md` present.
- `a_removal_of_a_missing_file_alone_is_unchanged` — removing a path `from` lacks, with every written file already as given, answers `Unchanged`.
- `takes_back_a_removal_when_the_commit_fails` — when the commit fails (a hook that exits 1), `b.md` is restored at the root and the branch is gone.
- `land_refuses_removing_a_file_a_change_holds` — `land` with a removed path that an unintegrated folder change holds is `Waiting`.

- [ ] `feat(store): a folder change removes files as well as writing them`

### Task 4: the write tool holds the format

Files: modified `crates/runtime/src/tools/folders.rs` (`check_input` runs `check_document_format` against the integration branch after `checked_path`; a removal goes to `land`; `plain_names` uses `document_name`), `crates/runtime/src/tools.rs` (description: documents by stem, `.md` or `.html` as the Team rules say), tested in `tools/folders.rs`'s `mod tests`.

- `writes_a_new_document_in_the_teams_format` — an HTML team's Architect at a review writes `architecture/features.html`: `folder_doc.written`, the file on the change's branch.
- `refuses_a_new_document_in_the_other_format` — the same with `features.md`: refused `wrong_format`, the message naming `docs/catervas/architecture/features.html`; nothing recorded.
- `keeps_an_existing_markdown_document` — with `features.md` on the integration branch, writing `features.md` in an HTML team lands.
- `rewrites_an_agent_document_in_place` — with `delivery/cadence.md` there, the Scrum Master's retro writes `cadence.html`: the change's branch has `cadence.html` and no `cadence.md`, and `folder_doc.written` names `cadence.html`.
- `refuses_the_spec_changing_format` — with `spec.md` there, a review proposal of `spec.html` is refused `other_format_exists`; no `folder_doc.proposed`.
- `proposes_an_html_spec_with_its_agent_version` — in an HTML team with no spec, proposing `spec.html` with `agent_text` records a proposal whose approval writes `spec.html` and `spec.agent.html`.

- [ ] `feat(runtime): catervas_write_folder_doc writes the team's format`

### Task 5: fixed documents found in either format

Files: modified `crates/core/src/governor/readiness.rs` (`is_plan_of`), `crates/runtime/src/orchestrator/rules.rs` (the refreshes owed when the spec's or the structure's stem is on the branch in either format), `crates/runtime/src/orchestrator/messages.rs` (`product_refresh_asks(sprint_id, spec, roadmap, report)` names the files there, or the team's format; `CADENCE_LINE` names `cadence.<ext>` by format), `crates/runtime/src/orchestrator/requests.rs` (the plan read as `plan_stem` in either format, a new one named in the team's), `crates/runtime/src/daemon/gates.rs` (`product_plan.state` reads each `PRODUCT_PLAN` stem in either format, answering the path it found), `crates/store/src/requests.rs` (`product_plan_request(.., format)`), `crates/runtime/src/tools/marketing.rs` (`plan_file(number, format)`, numbering over both extensions; HTML wrapper per the design), `crates/runtime/src/catervafication.rs` (rows by stem; paths by the team's format; criteria `<h1` and `<svg` for HTML), tested in each file's `mod tests`.

- `readiness_lets_a_task_name_its_html_plan` — a task of `CTV-3` whose `plan` is `…/plans/CTV-3.html` outside the epic's paths is ready; `CTV-4.html` is refused as today.
- `the_refresh_is_owed_for_an_html_spec` — a team whose integration branch holds `spec.html` owes the Product Manager's refresh as one with `spec.md` does.
- `the_review_asks_for_the_files_that_are_there` — with `spec.md` there and an HTML team, the ask names `spec.md` and `reports/S2.html`.
- `the_plan_is_read_in_either_format` — with `plans/CTV-3.html` on the branch, the Product Manager's message carries its text.
- `product_plan_state_reads_html_documents` — a verifying plan task whose branch holds `spec.html` and `roadmap.html` answers both with those paths.
- `the_product_plan_task_names_the_teams_format` — `product_plan_request(.., Html)`'s `allowed_paths` are the four `.html` paths.
- `an_html_teams_marketing_plan_is_a_page` — proposing in an HTML team writes `MP-1.html`, starting `<!doctype html>`, holding `<meta name="viewport"`, `<title>MP-1: …</title>`, `<h1>MP-1: …</h1>` and the text; and `MP-1.agent.html` alike.
- `plan_numbers_count_both_formats` — with `MP-1.md` there, the next plan in an HTML team is `MP-2.html`.
- `catervafication_files_html_documents` — an HTML team's Architect task names `overview.html` … with criteria `must_contain ["<h1", "<svg"]` for the overview and `["<h1"]` else; a Markdown team's keeps `["# ", "```mermaid"]`.

- [ ] `feat(runtime): find each fixed document in either format`

### Task 6: what agents are told

Files: modified `crates/runtime/src/prompt.rs` (`rules_section` takes the format and appends `format_line` after `folders_line`); in the six folder-owning roles' folders under `crates/roles/roles/` (`product_manager`, `architect`, `software_developer`, `ui_ux_designer`, `scrum_master`, `marketing_specialist`), every `system.md`, `role.yaml`, `kit.yaml` and `skills/*/SKILL.md` that the test below catches (today: each role's `catervafying-the-repository`, `product_manager/{system.md,role.yaml}` and its `writing-requirements`, `scoping-a-release`, `writing-task-contracts`, `architect/{system.md,role.yaml}` and its `planning-in-lanes`, `software_developer/{system.md,role.yaml}` and its `catervas-recommended-workflow`, `scrum_master/role.yaml` and its `keeping-work-flowing`, `marketing_specialist/{system.md,role.yaml}` and its `writing-the-marketing-plan`, `keeping-the-brand-kit`, `writing-the-brand-persona`, `marketing-what-ships`, `ui_ux_designer/{role.yaml,kit.yaml}` and its `brand-and-design-tokens`); the Finance and Procurement Specialists' private `.md` files are not documents and stay; `crates/roles/src/lib.rs` tests; tested in `prompt.rs`'s and `lib.rs`'s `mod tests`.

- `the_rules_say_the_teams_document_format` — an HTML team's prompt has `format_line(Html)` right after the folders line; a Markdown team's has `format_line(Markdown)`.
- `kits_name_documents_without_an_extension` — no text embedded for the six folder-owning roles contains `.agent.md`, `docs/catervas/` followed by a path ending `.md`, or any of `spec.md`, `roadmap.md`, `overview.md`, `features.md`, `jobs.md`, `structure.md`, `conventions.md`, `workflow.md`, `ui-inventory.md`, `cadence.md`, `brand-kit.md`, `persona.md`, or `MP-` followed by `<n>.md` (one check over every embedded text of those roles); the Architect's `catervafying-the-repository` asks for "a diagram" and does not say "Mermaid".
- The existing `lib.rs` assertions on the reworded lines change with them (same commit).

- [ ] `feat(roles): name documents by stem and tell agents the team's format`

### Task 7: the Files server reads HTML

Files: modified `crates/runtime/src/daemon/documents.rs` (`is_markdown` → `is_document`, `.md` or `.html`; `edit_refusal`'s `NotADocument` words "Only a document that ends in .md or .html can be saved here."), tested in its `mod tests`.

- `lists_html_documents_with_their_agent_versions` — a folder with `spec.html`, `spec.agent.html` and `overview.md` lists `spec.html` (twin `spec.agent.html`) and `overview.md`; `notes.txt` is not listed.
- `saves_the_owners_edit_of_an_html_document` — `files.save` of `roadmap.html` lands as a folder change; `files.read` of it then answers its twin and `stale` once integrated.

- [ ] `feat(runtime): the Files page lists and saves HTML documents`

### Task 8: the document route

Files: modified `crates/runtime/src/daemon/documents.rs` (`raw`, `FIT`, `fit_to_screen`), `crates/runtime/src/daemon/web.rs` (`has_session`; `session` uses it), `crates/runtime/src/daemon.rs` (the route), tested in `documents.rs`'s `mod tests` through the router as `web.rs`'s tests send requests.

- `serves_an_html_document_sandboxed` — with a session, `GET /documents/raw?path=…/overview.html` is 200, `text/html; charset=utf-8`, CSP exactly the design's, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store`, and the body holds the document's text.
- `fits_the_document_to_the_screen` — `fit_to_screen` puts `FIT` right after `<head lang="en">`; after `<HTML>` when there is no head; after `<!DOCTYPE html>` when neither; at the start when none; the body served contains `FIT` once.
- `serves_the_owners_waiting_change_and_the_agent_version` — with the owner's edit of `roadmap.html` waiting, the route answers the edit; `?path=…/roadmap.agent.html` answers the twin's integration text.
- `serves_a_tasks_branch_when_asked` — `&task=CTV-2` answers `spec.html` as the task's branch holds it.
- `refuses_what_it_does_not_serve` — no session 401; another `Host` 403; `.md`, `../`, a path outside `docs/catervas/`, a missing file, an unknown task each 404 with an empty body.

- [ ] `feat(runtime): serve HTML documents sandboxed and fitted to the screen`

### Task 9: ask for a change

Files: modified `crates/roles/src/lib.rs` (`reviewer_for`, the choice `file_rederive` makes today, which `crates/runtime/src/tools/folders.rs`'s `file_rederive` then calls), `crates/store/src/requests.rs` (`Filing.small`, `file_small_request`, `change_request`), `docs/schemas/rpc.schema.json` (`files.ask_change` params `{ path: 1..512, words: 1..2000 }`, result `{ task_id }`), `crates/runtime/src/daemon/documents.rs` (`ask_change`), tested in each `mod tests`.

- `files_the_owners_change_for_the_folders_role` — `files.ask_change` on `architecture/overview.html` files a task whose `assignee_role` is `architect`, `created_by` `human`, title "Change Overview", intent quoting the words, `allowed_paths` `[overview.html]`, then `request.triaged { small }`; the answer names it.
- `a_pairs_change_reaches_both_files` — on `product/spec.html` the `allowed_paths` are `spec.html` and `spec.agent.html`.
- `refuses_an_ask_the_owner_may_not_make` — an agent version or a marketing plan: `not_editable`; a folder whose role has no active agent: `no_owner`; nothing filed.

- [ ] `feat(runtime): the owner asks the team to change a document`

### Task 10: rewrite the documents as HTML

Files: modified `docs/schemas/event.schema.json` (`document_rewrite.started`, body `{ tasks: [taskId], 1..6 }`; `rederives`' pattern), every exhaustive match on `EventBody` the compiler names, `crates/store/src/requests.rs` (`rewrite_request`, `rewrite_tasks`), `docs/schemas/rpc.schema.json` (`files.rewrite`, params `{}`, result `{ tasks }`), `crates/runtime/src/daemon/documents.rs` (`rewrite`), `crates/runtime/src/orchestrator/{rules,messages}.rs` (`rewrite_delivery`), tested in each `mod tests`.

- `files_a_rewrite_task_for_each_folder_with_markdown` — Product Manager (`spec.md`, `spec.agent.md`), Architect (`overview.md`), Marketing (`plans/MP-1.md` only): two tasks, the Product Manager's naming `spec.md`, the Architect's `overview.md`, each with an artifact criterion for its `.html`, `allowed_paths` its folder; `document_rewrite.started` lists both.
- `rewrite_skips_marketing_plans_and_the_scrum_master` — a marketing folder of plans alone and a delivery folder with `cadence.md` get no task.
- `refuses_a_rewrite_it_cannot_start` — a Markdown team: `not_html`; no `.md` but plans: `nothing_to_rewrite`; a task of the last rewrite `ready`: `rewrite_open`; one `accepted` and one `cancelled`: a new rewrite files.
- `the_retro_rewrites_delivery_after_a_rewrite` — with a rewrite started and `delivery/cadence.md` on the branch, the Scrum Master's retro message holds the design's line; without the rewrite, or with `cadence.html` only, it does not.

- [ ] `feat(runtime): rewrite the team's documents as HTML`

### Task 11: reading HTML in the web app

Files: created `apps/web/src/components/DocumentFrame.tsx`, `.module.css`, `.test.tsx`; modified `apps/web/src/pages/files.ts` (`isHtml`), `FileDocument.tsx` (HTML: title is the document's name; `DocumentFrame` in place of `Markdown`; under the document "What should change?" with "Send to <owner>" (`files.ask_change`) wherever `editable`, for Markdown too; `ShowAdvanced` holding "Edit source" for HTML, which opens today's editor without the Preview tab), `ProductPlanCard.tsx` (an HTML document as `DocumentFrame` with `task`), `DocumentChanges.tsx` (`plain` strips tags for `.html`), `Files.module.css`, `strings/en.ts` (`docOpenFull` "Open full page", `docAskLabel` "What should change?", `docAskSend` "Send to {owner}", `docAskSent` "Sent. {owner} has it as {task}.", `docEditSource` "Edit source"), tested in `apps/web/src/pages/Files.test.tsx` (where `FileDocument` is tested today), `ProductPlanCard.test.tsx`, `DocumentChanges.test.tsx`, `apps/web/src/components/DocumentFrame.test.tsx`.

- `frames_the_document_sandboxed` — `DocumentFrame` renders an `iframe` whose `src` is `/documents/raw?path=<encoded>` (plus `&task=` when given), whose `sandbox` attribute is present and empty, titled; and a link "Open full page" to the same URL with `target="_blank"` and `rel` containing `noopener`.
- `shows_an_html_document_in_its_frame` — `files.read` of `overview.html`: no `Markdown` output, a frame for the path; "For the team" frames the twin's path.
- `asks_for_a_change` — typing and Send calls `files.ask_change { path, words }` and shows the sent line; a refusal shows its words in an alert.
- `edits_html_source_under_advanced` — no Edit button for HTML until Advanced is open; "Edit source" opens the editor with the text and no Preview tab; Save calls `files.save`.
- `the_product_plan_card_frames_html` — documents `spec.html`, `roadmap.html` render two frames with `task`.
- `html_changes_read_as_words` — a changed line `<li>Ship <b>v2</b></li>` of `spec.html` shows "Ship v2".

- [ ] `feat(web): show HTML documents in a sandboxed frame`

### Task 12: the setting and the rewrite in Settings

Files: modified `apps/web/src/pages/TeamRules.tsx` (`Documents`), `apps/web/src/pages/settings/Work.tsx` (renders it after `Planning`), `apps/web/src/app/refusals.ts` (`not_html`, `nothing_to_rewrite`, `rewrite_open`, `not_editable`, `no_owner`, `wrong_format`), `apps/web/src/strings/en.ts` (`rulesDocuments` "Documents", `documentsHtml` "Write documents as HTML", `documentsHtmlNote` bullets: "New documents are web pages with diagrams and charts." / "Documents already in Markdown stay as they are.", `documentsRewrite` "Rewrite documents as HTML", `documentsRewriteNote` per the design's ⓘ, `documentsRewriteStarted` "Each role has a task to rewrite its documents.", and each refusal's words), tested in `apps/web/src/pages/settings/settings.test.tsx`.

- `switches_the_document_format` — the switch is on for a team without the key; turning it off saves `policy.documentFormat: "markdown"` through `team.save`.
- `offers_the_rewrite_while_markdown_remains` — with an HTML team and `files.tree` listing `overview.md`, the button shows and calls `files.rewrite`, then shows the started line; with only `.html` documents, or a Markdown team, no button.
- `says_why_a_rewrite_was_refused` — `rewrite_open` shows its words under the button.

- [ ] `feat(web): the document format and the rewrite in Settings`

### Task 13: the spec

Files: modified `docs/SPEC.md` (revision 0.107 line; 4.5 the frame, fit, Open full page, Ask for a change, Edit source; 5.12 `document_format`; 5.17 either format, the format rules, the route, the methods and event, the rewrite; each kit's `catervafying-the-repository` line by stem), `docs/design/catervas-folders.md` (a note under its status pointing to `html-documents.md`).

- [ ] `docs(spec): the team's documents in HTML (0.107)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check
# expected: xtask check: ok
```

Then, in the web app against a scratch project: an HTML team's Architect writes `overview.html`; the Files page shows it framed at phone width (375 px) with no page scroll sideways; "Open full page" opens it alone; "Rewrite documents as HTML" files the tasks.
