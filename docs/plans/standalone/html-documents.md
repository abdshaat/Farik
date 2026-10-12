# Standalone: the team writes its documents as HTML

Status: ready
Branch: `feat/html-documents` (work outside a phase, its own pull request to `main`, #47)
Spec: `docs/SPEC.md` 4.5, 5.12 and 5.17 (and the kit lines of 6.x), revision 0.107; ADR 0058
Depends on: `origin/main` at `9e3d4f3c` (#46 merged; phase 8 in #34)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-12 (one round against `docs/standards/workflow.md` stage 2: NOT READY on three unmade decisions, which the founder's design left open and the planner decided within it: no "Ask for a change" on the Scrum Master's documents, a rewrite of zero tasks, an HTML marketing plan read framed before approval; recorded in the design and here. Its other twenty-one findings are folded into this revision. A second round the same day found the three made: READY, with eight minor findings folded in.)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

`docs/design/html-documents.md` (2026-10-12) is the source of truth for behavior; this plan does not restate it. Done means: a team has `policy.document_format` (`html` unless set); agents write new documents under `docs/catervas/` in it, HTML pages with inline SVG and no scripts, while an existing `.md` stays Markdown; every fixed document (spec, roadmap, plans, Catervafication documents, marketing plans) is found in either format; the Files page, the product plan card and a waiting marketing plan show HTML in a sandboxed frame from `GET /documents/raw`, fitted to the screen, with "Open full page"; the Files page has "Ask for a change" and, under Advanced, "Edit source"; Settings → Work has the format switch and "Rewrite documents as HTML". Out of scope: as the design's list.

## Decisions

- **Format type.** `DocumentFormat { Markdown, Html }` in `catervas_core::folders`, with `extension()` (`"md"`, `"html"`) and `of(path)`. The schema's `policy.document_format` (`"html" | "markdown"`) generates its own type; `Team::document_format()` maps it, `Html` when absent. Chose a core type so the folder rules do not depend on the wire's naming.
- **Stems.** A fixed document is named by its stem (`docs/catervas/product/spec`). The lists are renamed so the compiler names every user: `DOCUMENT_NAMES` → `DOCUMENT_STEMS`, `OWNER_ACCEPTED` → `OWNER_ACCEPTED_STEMS`, `PRODUCT_PLAN` → `PRODUCT_PLAN_STEMS`. Every comparison goes through `stem_of(path)` (the path without `.md` or `.html`, `None` for any other extension). An agent version's stem ends `.agent`.
- **The format rule.** `check_document_format(path, format, on_branch)` → `Ok(Some(other))` (the same stem's other-format file, to remove), `Ok(None)`, `Err(WrongFormat { instead })`, or `Err(OtherFormatExists { existing })` (only for `OWNER_ACCEPTED_STEMS`). Pure; the tool passes `git.has_file` on the integration branch. Refusal words:
  - `wrong_format`: "`<path>` is a new document and this team writes new documents as `<HTML|Markdown>`: write `<instead>`".
  - `other_format_exists`: "`<existing>` is this document: change it there; its rewrite as HTML reaches the owner through a task".
- **Removing a file.** `Git::commit_files` and `land` gain `removed: &[String]`; every other caller passes `&[]`.
  - The `Busy` and `Linked` checks cover removed paths too, so an owner's uncommitted change is never deleted.
  - A removed path is deleted at the root, staged with `git add --` and committed with `commit --only`.
  - `take_back` restores it from `from`.
  - `Unchanged` needs every write already held and every removed path absent at `from`.
  - `land` refuses a removed path that a waiting change holds.
  - `folder_doc.written` names only the written path. `ponytail:` a write of the removed file while its removal waits conflicts at integration and is escalated there, not refused at `land`; record `removed` in the event if that happens.
- **The document route** (`documents::raw`, `GET /documents/raw`, in `daemon.rs`'s browser router).
  - It serves only `.html`, and only regular files (`is_file`).
  - Its session check is a new `web::has_session(headers, web) -> bool` (own host and a verified cookie), which `web::session` also uses.
  - Answers: 403 for a foreign host; 401 with no session; 404 for anything else, including no project (`state.deps()` `None`) and no browser state. Every error has an empty body.
  - Without `task`, it answers the text `files.read` shows for the path's human document: the owner's waiting change, or the integration branch's text. A twin path answers the twin's integration text.
  - With `task`, the value is parsed as a `TaskId` (404 when it is not one, or no contract). The route reads `task_branch(contract)`, then falls back to the task's worktree file `.catervas/local/worktrees/<id>/<path>`, which must be a regular file with no link on the way.
  - Headers: the CSP of the design, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `X-DNS-Prefetch-Control: off`, `Cache-Control: no-store`. A `v` query parameter is ignored; the page uses it to reload the frame.
- **Fit block.** `FIT` and `fit_to_screen` are private to `daemon/documents.rs`. The block goes after the first `<head` tag's `>`, else after `<html`'s, else after `<!doctype`'s (ASCII case-insensitive), else at 0. `ponytail:` the tag is found by text, not by parsing; use a parser if a document defeats it.
- **Methods, not commands.** `files.ask_change { path, words }` answers `{ task_id }`, and `files.rewrite {}` answers `{ tasks }`. Both are methods of `documents.rs`, and `call` dispatches by method name (it saves for any method today). Chose methods: neither moves a task the orchestrator owns.
- **Filing.**
  - Both file through `file_small_request(files, log, wire, created_by: "human", filed_as, reason, (now, ids))`: `task.created` with the new optional `filed_as` (`"owner_change" | "document_rewrite"`), then `request.triaged { small, reason, triaged_by: "catervas" }`.
  - `is_to_be_judged` counts `filed_as` as filed whole, so no refine session rewrites these contracts.
  - Budget: `placeholder_budget_usd(&team.rules())`.
  - A change task's title is "Change <document name>", cut to 120 characters with "…" as its last.
- **Reviewers.**
  - An ask's reviewer is `catervas_roles::reviewer_for(team, owner)`: the choice `file_rederive` makes today (`default_reviewer_role`, else the first preference), moved to `crates/roles/src/reviewer.rs` and re-exported. `file_rederive` calls it.
  - A rewrite uses `default_reviewer_role(team, TaskKind::Task, owner)` alone, and a role it answers `None` for gets no task.
- **Who gets a rewrite task.** Each role in `folders(team, entries)` other than the Scrum Master, whose folder holds a `.md` human document or agent-only document other than `marketing/plans/MP-*`, with a reviewer.
  - `document_rewrite.started { tasks }` holds 0 to 6 ids. It is recorded whenever a task is filed, or `delivery/` holds a `.md` file.
  - `rewrite_tasks(log)` returns the last event's ids.
  - `rewrite_open`: any of them is on the board and neither `accepted` nor `cancelled`.
  - The order of refusals: `not_html`, `rewrite_open`, `nothing_to_rewrite`.
- **The Scrum Master's documents.** `files.ask_change` on `delivery/` is refused with `no_task_taker`, and `files.read` answers `askable: false` there; elsewhere `askable` equals `editable`. `retro_message` gains `rewrite_delivery: bool`, true when all of these hold:
  - the runner is the Scrum Master;
  - a `document_rewrite.started` exists;
  - the format is `Html`;
  - the integration branch holds a `.md` file under `docs/catervas/delivery/`.
- **Marketing plans.**
  - `marketing_plan.proposed` gains optional `format: "html"`, and `marketing_plan.get` answers `format` (`"markdown"` when absent).
  - An HTML plan is the page `html_plan_page(id, title, body)`: doctype, `lang="en"`, the charset and viewport metas, `<title>` and `<h1>` both `<id>: <title>`, with the title escaped (`& < > "`), then the body.
  - `tools.rs`'s `paths_of` permits both `plan_file(0, Markdown)` and `plan_file(0, Html)`.
- **Headings and changed lines.** An HTML heading is a trimmed line that starts `<h1`..`<h6` and then `>` or a space. `changed_lines` treats it as `#` lines are treated.
  - The web's `plain(text, path)` strips `<…>` for an `.html` path, then decodes `&amp; &lt; &gt; &quot; &#39;` and no other entity.
  - It marks a heading line as it marks `#` lines today, and its output stays a React text node.
- **Prompts.** The format reaches the prompt as `PromptInput.document_format`, set from `team.document_format()` in `orchestrator/session.rs` (and in every `PromptInput` the tests build). `rules_section` appends `format_line(format)` after `folders_line`. Kits name documents by stem.
- **Web.**
  - `isHtml(path)` in `files.ts`.
  - `DocumentFrame { path, task?, version?, title }` builds its `src` as `/documents/raw?path=…[&task=…][&v=<version>]`, so a new blob reloads the frame. An HTML text never reaches `<Markdown>`.
  - The fixed-name maps of `DocumentChanges.tsx` and `ProductPlanCard.tsx` are keyed by stem.
  - Settings' part key is `"documents"`.
  - `vite.config.ts` proxies `/documents` to the daemon.
- **`team.updated` does not carry the format.** `TeamSetup.tsx`'s policy type gains `documentFormat?`, and `template.rs`'s policy key list gains `document_format`.
- **No new dependency.**

## File map

```
crates/core/src/folders.rs                   Task 1: format, stems, pairs, the format rule, format_line, headings
crates/core/src/governor/{readiness,done}.rs Task 1: messages naming .md; Task 5: is_plan_of
crates/store/src/git.rs                      Task 1: product_plan_on by stem; Task 3: removals
crates/store/tests/git.rs                    Task 1, Task 3: tests
crates/runtime/src/{transitions.rs,orchestrator/fixtures.rs}  Task 1: the renamed lists
docs/schemas/{team,team-template}.schema.json, crates/core/src/team.rs, crates/core/src/team/template.rs, apps/web/src/pages/setup/TeamSetup.tsx  Task 2
crates/runtime/src/tools/folders.rs          Task 3: land; Task 4: the format check, texts; Task 9: reviewer_for
crates/runtime/src/tools.rs                  Task 4: description; Task 5: paths_of
docs/schemas/event.schema.json               Task 4: folderDocPath; Task 5: marketing format; Task 9: filed_as, rederives; Task 10: the event
docs/schemas/task-contract.schema.json       Task 5: plan pattern
crates/runtime/src/orchestrator/{rules,messages,requests}.rs  Task 5: fixed paths, refreshes; Task 9: is_to_be_judged; Task 10: retro line
crates/runtime/src/daemon/gates.rs           Task 5: product_plan.state, marketing_plan.get format
crates/store/src/requests.rs                 Task 5: product_plan_request; Task 9: filing, change_request; Task 10: rewrite_request, rewrite_tasks
crates/runtime/src/tools/marketing.rs        Task 5: plan files, html_plan_page
crates/runtime/src/catervafication.rs        Task 5: rows, criteria, messages
crates/runtime/src/{prompt.rs,orchestrator/session.rs}  Task 6
crates/roles/roles/** (six roles), crates/roles/src/lib.rs  Task 6
crates/runtime/src/daemon/documents.rs       Tasks 7 to 10
crates/runtime/src/{daemon.rs,daemon/web.rs} Task 8
crates/roles/src/reviewer.rs                 Task 9
docs/schemas/rpc.schema.json                 Task 7: wording; Task 9, Task 10: methods; Task 5: marketing_plan.get format
crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}, crates/store/src/projections.rs  Task 10: the event's lists
packages/protocol-client/src/client.ts       Task 11: files.ask_change; Task 12: files.rewrite
apps/web/src/components/DocumentFrame.{tsx,module.css,test.tsx}  Task 11
apps/web/src/pages/{FileDocument,ProductPlanCard,DocumentChanges,MarketingPlan}.tsx, files.ts, Files.module.css, app/refusals.ts, strings/en.ts, vite.config.ts  Task 11
apps/web/e2e/files.spec.ts                   Task 11
apps/web/src/pages/{TeamRules.tsx,settings/Work.tsx}, app/refusals.ts, strings/en.ts  Task 12
docs/SPEC.md, docs/design/catervas-folders.md  Task 13
```

## Interfaces

Consumes (on main): `normalise`, `role_folder`, `is_human_document`, `agent_twin`, `human_of_twin`, `check_owner_edit`, `changed_lines`, `land`, `file_rederive`, `rederive_request`, `file`, `Filing`, `task_branch`, `integration_branch`, `default_reviewer_role`, `placeholder_budget_usd`; web `useRuleParts`, `ShowAdvanced`, `Switch`, `Bullets`, `daemonSaid`/`said`.

Produces:

```rust
// catervas_core::folders
pub enum DocumentFormat { Markdown, Html }
impl DocumentFormat { pub fn extension(self) -> &'static str; pub fn of(path: &str) -> Option<Self>; }
pub fn stem_of(path: &str) -> Option<&str>;
pub fn with_format(stem: &str, format: DocumentFormat) -> String;
pub fn plan_stem(epic: &str) -> String;               // replaces plan_path
pub fn is_plan_of(path: &str, epic: &str) -> bool;
pub enum FormatRefusal { WrongFormat { instead: String }, OtherFormatExists { existing: String } }
pub fn check_document_format(path: &str, format: DocumentFormat, on_branch: &dyn Fn(&str) -> bool) -> Result<Option<String>, FormatRefusal>;
pub fn format_line(format: DocumentFormat) -> String;
pub const DOCUMENT_STEMS: [(&str, &str); 12]; pub const OWNER_ACCEPTED_STEMS: [&str; 2]; pub const PRODUCT_PLAN_STEMS: [&str; 2];
// NotMarkdown becomes NotADocument in FolderDocPathRefusal and OwnerEditRefusal (code "not_a_document")
impl Team { pub fn document_format(&self) -> DocumentFormat; }
// catervas_store
impl Git { pub fn commit_files(&self, branch: &str, from: &str, files: &[(String, String)], removed: &[String], message: &str, author: Option<&str>) -> Result<CommitOutcome, GitError>; }
pub fn change_request(path: &str, twin: Option<&str>, name: &str, words: &str, owner: Role, reviewer: Role, max_cost_usd: f64) -> Value;
pub fn rewrite_request(folder: &str, documents: &[String], owner: Role, reviewer: Role, max_cost_usd: f64) -> Value;
pub fn file_small_request(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str, filed_as: TaskCreatedBodyFiledAs, reason: &str, at: (DateTime<Utc>, &EventIds)) -> Result<TaskContract, RequestError>;
pub fn rewrite_tasks(log: &EventLog) -> Result<Vec<TaskId>, StoreError>;
pub fn product_plan_request(reviewer: Role, max_cost_usd: f64, format: DocumentFormat) -> Value;
// catervas_roles
pub fn reviewer_for(team: &Team, owner: Role) -> Option<Role>;
// catervas_runtime (crate-private)
fn html_plan_page(id: &str, title: &str, body: &str) -> String;   // tools/marketing.rs
pub(crate) fn plan_file(number: u32, format: DocumentFormat) -> String;
```

```ts
export function isHtml(path: string): boolean;                                       // pages/files.ts
export function DocumentFrame(props: { path: string; task?: string; version?: string; title: string }): JSX.Element;
```

## Tasks

### Task 1: documents in either format, in the core

Files: `crates/core/src/folders.rs` (tests in its `mod tests`); every user of a renamed item, the compiler's list, including `crates/core/src/governor/{readiness,done}.rs`, `crates/store/src/git.rs` (`product_plan_on` checks each stem in either format), `crates/runtime/src/transitions.rs`, `crates/runtime/src/orchestrator/fixtures.rs`, `crates/store/tests/git.rs`; `readiness.rs:966` switches to `is_plan_of` here; the message at `readiness.rs:1067` says "…until `docs/catervas/product/spec` and `docs/catervas/product/roadmap`, as `.md` or `.html`, are on the integration branch", and `done.rs:401`'s says "the document and its agent version"; the test in `crates/runtime/src/daemon/documents.rs` asserting `"not_markdown"` (`:1023`) asserts `"not_a_document"`.

- `stem_drops_md_or_html_and_nothing_else` — `stem_of`: `a/b.md` and `a/b.html` give `a/b`; `a/b.txt` and `a/b` give `None`; `a/spec.agent.html` gives `a/spec.agent`.
- `pairs_in_either_format` — `is_human_document`:
  - holds for `product/spec.md`, `spec.html`, `roadmap.html` and `marketing/plans/MP-3.html`;
  - not for `MP-03.html` or `spec.htm`.
  
  `agent_twin("…/spec.html")` is `…/spec.agent.html`; `human_of_twin("…/spec.agent.html")` is `…/spec.html`; `…/spec.agent.md` still maps to `…/spec.md`.
- `pair_changed_alone_holds_html_pairs` — `[spec.html]` is reported; `[spec.html, spec.agent.html]` is not; `[spec.md, spec.agent.html]` reports both.
- `owner_documents_and_product_plan_by_stem` — `may_change_an_owner_document(["docs/catervas/product/spec.html"])` holds; `product_plan_approved` holds for `spec.html` with `roadmap.md`, not for `spec.html` alone.
- `folder_doc_path_takes_md_and_html` — `check_folder_doc_path` for the Architect:
  - accepts `architecture/overview.html`;
  - refuses `overview.txt` with `NotADocument`, `spec.agent.html` with `AgentTwin`, and `marketing/plans/MP-2.html` with `MarketingPlan`.
  
  `product/spec.html` is `owner_accepted`.
- `owner_edit_takes_md_and_html` — `check_owner_edit` accepts `product/roadmap.html`; refuses `.txt` as `not_a_document` and `roadmap.agent.html` as `agent_twin`.
- `format_check_writes_the_teams_format` — `Html`, nothing on the branch: `overview.html` is `Ok(None)`; `overview.md` is `WrongFormat { instead: "docs/catervas/architecture/overview.html" }`.
- `format_check_keeps_an_existing_documents_format` — with `Html` and `delivery/cadence.md` there:
  - `cadence.md` is `Ok(None)`;
  - `cadence.html` is `Ok(Some("docs/catervas/delivery/cadence.md"))`.
  
  With `Markdown` and `overview.html` there, `overview.html` is `Ok(None)`.
- `format_check_refuses_a_spec_changing_format` — `Html` with `product/spec.md` there: `product/spec.html` is `OtherFormatExists { existing: "docs/catervas/product/spec.md" }`.
- `document_names_by_stem` — `document_name`:
  - "Product description" for `…/product/spec.html`;
  - "Plan: Title" for `plans/CTV-3.html` with title "Title";
  - "Notes on x" for `notes-on-x.html`.
  
  `document_order` places `architecture/overview.html` second after the product's two.
- `plans_in_either_format` — `is_plan_of(".../plans/CTV-3.html", "CTV-3")` and `.md` hold; `CTV-30.md` does not.
- `format_line_says_each_format` — both start `- documents:`:
  - `format_line(Html)` contains `.agent.html`, `inline SVG`, `no scripts`, `viewport`, `one block element per line` and `stays Markdown`;
  - `format_line(Markdown)` contains `.agent.md`, `Mermaid` and `stays HTML`.
- `html_headings_head_a_run` — `changed_lines` of a page whose changed line sits under `<h2>Risks</h2>` shows that line as the run's heading; under `<header>` it does not.
- `folders_line_names_either_agent_version` — `folders_line` contains `.agent.html or .agent.md`.
- `product_plan_on_takes_either_format` (`crates/store/tests/git.rs`) — true for an integration branch holding `spec.html` and `roadmap.md`; false for `spec.html` alone.

- [ ] `feat(core): a team document is Markdown or HTML`

### Task 2: the team's document format

Files: `docs/schemas/team.schema.json` (`$defs/policy.document_format`, enum `["html","markdown"]`, described as in the design); `docs/schemas/team-template.schema.json`; `crates/core/src/team.rs`; `crates/core/src/team/template.rs` (carried as `plan_in_sprints` is, and in the policy key list at `:466`); `apps/web/src/pages/setup/TeamSetup.tsx` (the type). Tests in their `mod tests`.

- `document_format_is_html_unless_markdown` — absent or `"html"` gives `Html`; `"markdown"` gives `Markdown`; `"pdf"` is refused by validation.
- `templates_carry_the_document_format` — a template from a Markdown team holds `"markdown"` and a team made from it does too; a template without the key leaves the team's absent.

- [ ] `feat(core): policy.document_format, HTML unless set`

### Task 3: a folder change may remove files

Files: `crates/store/src/git.rs` (`commit_files`, `first_link`, `uncommitted`, `commit_files_on_the_new_branch`, `take_back`); `crates/runtime/src/tools/folders.rs` (`land`); the callers at `daemon/documents.rs`, `orchestrator/human.rs` and `orchestrator/fixtures.rs` pass `&[]`. Tests in `crates/store/tests/git.rs` and `tools/folders.rs`'s `mod tests`.

- `commits_a_removal_with_the_writes` — writing `b.html` and removing `b.md` makes one commit whose tree has `b.html` and no `b.md`; the root is back on its branch with `b.md` present.
- `a_removal_of_a_missing_file_alone_is_unchanged` — with every write already held and the removed path absent at `from`, the result is `Unchanged`.
- `a_removal_of_a_busy_file_is_busy` — an uncommitted root change to `b.md` gives `Busy(["b.md"])`, and `b.md` still holds the change.
- `takes_back_a_removal_when_the_commit_fails` — when a commit hook exits 1, `b.md` is restored at the root and the branch is gone.
- `land_refuses_removing_a_file_a_change_holds` — `Waiting`.

- [ ] `feat(store): a folder change removes files as well as writing them`

### Task 4: the write tool holds the format

Files:
- `crates/runtime/src/tools/folders.rs`:
  - `check_input` runs `check_document_format` against the integration branch after `checked_path`, and a removal goes to `land`;
  - the `WriteFolderDocInput` doc comments and the messages at `:493`, `:530` and `:534` name `.md or .html` and "agent version";
  - `plain_names` uses `document_name`.
- `crates/runtime/src/tools.rs`: the description names documents by stem, in the format the Team rules say.
- `docs/schemas/event.schema.json`: `folderDocPath` becomes `\.(md|html)$`.

Tests in `tools/folders.rs`'s `mod tests` and `crates/protocol`'s event tests:

- `writes_a_new_document_in_the_teams_format` — an HTML team's Architect at a review writes `architecture/features.html`: `folder_doc.written` is recorded and the file is on the change's branch.
- `refuses_a_new_document_in_the_other_format` — `features.md` is refused `wrong_format` with the decided words; nothing is recorded.
- `keeps_an_existing_markdown_document` — with `features.md` on the integration branch, writing it lands.
- `rewrites_an_agent_document_in_place` — with `delivery/cadence.md` there, the Scrum Master's retro writes `cadence.html`: the branch has it and no `cadence.md`, and `folder_doc.written` names `cadence.html`.
- `refuses_the_spec_changing_format` — with `spec.md` there, a review proposal of `spec.html` is refused `other_format_exists`; no `folder_doc.proposed` is recorded.
- `proposes_an_html_spec_with_its_agent_version` — in an HTML team with no spec, the proposal's approval writes `spec.html` and `spec.agent.html`.
- `folder_doc_paths_take_html` (protocol) — `folder_doc.written` with `…/overview.html` reads; one with `…/overview.txt` is refused.

- [ ] `feat(runtime): catervas_write_folder_doc writes the team's format`

### Task 5: fixed documents found in either format

Files:
- `docs/schemas/task-contract.schema.json`: `plan` matches `\.(md|html)$`.
- `crates/runtime/src/orchestrator/rules.rs`: the refreshes are owed when the spec's or the structure's stem is there in either format.
- `crates/runtime/src/orchestrator/messages.rs`:
  - `product_refresh_asks(sprint_id, spec, roadmap, report)` names the files that are there, or the team's format;
  - `architecture_refresh_message` names `features`, `jobs` and `structure` in the format each has, or the team's;
  - `CADENCE_LINE` becomes a function of the format.
- `crates/runtime/src/orchestrator/requests.rs`: the plan is read by `plan_stem` in either format, and a new one is named in the team's.
- `crates/runtime/src/daemon/gates.rs`:
  - `product_plan.state` reads each stem in either format and answers the path it found;
  - (`marketing_plan.get`'s `format` comes from `crates/runtime/src/marketing.rs`'s `whole`, `:686`, which answers it from the proposal.)
- `crates/store/src/requests.rs`: `product_plan_request(.., format)`, whose `allowed_paths`, intent, requirements and rubric name the format's files.
- `crates/runtime/src/tools/marketing.rs`:
  - `plan_file(number, format)`, with numbering over both extensions and `html_plan_page`;
  - the input doc comments at `:38` and `:41`;
  - the event's `format`.
- `crates/runtime/src/tools.rs`: `paths_of`.
- `docs/schemas/event.schema.json`: `marketingPlanProposedBody.format`.
- `docs/schemas/rpc.schema.json`: `marketing_plan.get`'s `format`.
- `crates/runtime/src/catervafication.rs`: rows by stem, paths by the team's format, criteria `<h1` and `<svg` for HTML, and the messages that name `.md` (`:51`).

Tests in each file's `mod tests`:

- `readiness_lets_a_task_name_its_html_plan` — `plan` `…/plans/CTV-3.html` under `CTV-3` is ready; `CTV-4.html` is refused as today.
- `the_refresh_is_owed_for_an_html_spec` — an integration branch holding `spec.html` owes the Product Manager's refresh.
- `the_review_asks_for_the_files_that_are_there` — `spec.md` there in an HTML team: the ask names `spec.md` and `reports/S2.html`.
- `the_architects_refresh_names_its_files` — `structure.md` there, no `features` in an HTML team: the message names `structure.md` and `features.html`.
- `the_plan_is_read_in_either_format` — with `plans/CTV-3.html` there, the Product Manager's message carries its text.
- `product_plan_state_reads_html_documents` — a verifying plan task's branch holding `spec.html` and `roadmap.html`: both are answered with those paths.
- `the_product_plan_task_names_the_teams_format` — `Html`: the `allowed_paths` are the four `.html` paths, and no field of the contract contains `.md`.
- `an_html_teams_marketing_plan_is_a_page` — writes `MP-1.html` and `MP-1.agent.html`, each starting `<!doctype html>`, with the viewport meta, `<title>MP-1: A &amp; B</title>` and `<h1>MP-1: A &amp; B</h1>` for the title "A & B", then the text. The event has `format: "html"`, and `marketing_plan.get` answers `"html"`.
- `plan_numbers_count_both_formats` — with `MP-1.md` there, the next plan in an HTML team is `MP-2.html`.
- `catervafication_files_html_documents` — an HTML team's Architect task names `overview.html`, with `must_contain ["<h1", "<svg"]` for the overview and `["<h1"]` for the others. A Markdown team keeps `["# ", "```mermaid"]`.

- [ ] `feat(runtime): find each fixed document in either format`

### Task 6: what agents are told

Files:
- `crates/runtime/src/prompt.rs`: `PromptInput.document_format`, and `rules_section` appends `format_line`; every test constructor sets the field.
- `crates/runtime/src/orchestrator/session.rs:1140`: sets it from `team.document_format()`.
- In the six folder-owning roles (`product_manager`, `architect`, `software_developer`, `ui_ux_designer`, `scrum_master`, `marketing_specialist`), each `system.md`, `role.yaml`, `kit.yaml` and `skills/*/SKILL.md` the test below catches. The Finance and Procurement Specialists' private `.md` files stay.
- `crates/roles/src/lib.rs`: its tests, rewritten with the wording.

Tests in `prompt.rs`'s and `lib.rs`'s `mod tests`:

- `the_rules_say_the_teams_document_format` — an HTML team's prompt has `format_line(Html)` right after the folders line, and a Markdown team's has `format_line(Markdown)`.
- `kits_name_documents_without_an_extension` — no text embedded for the six roles contains any of these:
  - `.agent.md`;
  - `docs/catervas/` followed by a path ending `.md`;
  - `spec.md`, `roadmap.md`, `overview.md`, `features.md`, `jobs.md`, `structure.md`, `conventions.md`, `workflow.md`, `ui-inventory.md`, `cadence.md`, `brand-kit.md` or `persona.md`;
  - `MP-<n>.md`.
  
  No Architect text contains "Mermaid".

- [ ] `feat(roles): name documents by stem and tell agents the team's format`

### Task 7: the Files server reads HTML

Files: `crates/runtime/src/daemon/documents.rs` (`is_markdown` becomes `is_document`; the `NotADocument` words are "Only a document that ends in .md or .html can be saved here."; `files.read` answers `askable`); `docs/schemas/rpc.schema.json` (`not_markdown` becomes `not_a_document` at `:2608`, ".md files" becomes ".md and .html files" at `:2650`, and `filesReadResult.askable`). Tests in its `mod tests`.

- `lists_html_documents_with_their_agent_versions` — a folder with `spec.html`, `spec.agent.html`, `overview.md` and `notes.txt` lists `spec.html` (twin `spec.agent.html`) and `overview.md`.
- `saves_the_owners_edit_of_an_html_document` — `files.save` of `roadmap.html` lands as a folder change; once integrated, `files.read` answers `stale: true`.
- `askable_where_editable_but_delivery` — `askable` is true for `architecture/overview.html`, and false for `delivery/cadence.md`; `files.read` of `spec.agent.html` is not found, as today.

- [ ] `feat(runtime): the Files page lists and saves HTML documents`

### Task 8: the document route

Files: `crates/runtime/src/daemon/documents.rs` (`raw`, `FIT`, `fit_to_screen`); `crates/runtime/src/daemon/web.rs` (`has_session`, used by `session`); `crates/runtime/src/daemon.rs` (the route). Tests in `documents.rs`'s `mod tests`, sending requests through the router as `web.rs`'s tests do.

- `serves_an_html_document_sandboxed` — with a session, `…/overview.html` answers:
  - 200, `text/html; charset=utf-8`;
  - the design's CSP, exactly;
  - the four other headers;
  - a body holding the document's text.
- `fits_the_document_to_the_screen` — `fit_to_screen` puts `FIT`:
  - after `<head lang="en">`;
  - after `<HTML>` when there is no head;
  - after `<!DOCTYPE html>` when there is neither;
  - at the start when there is none of them.
  
  A served body holds `FIT` once.
- `serves_the_owners_waiting_change_and_the_agent_version` — the waiting edit of `roadmap.html` is served; `roadmap.agent.html` serves the twin's integration text.
- `serves_a_tasks_branch_then_its_worktree` — `&task=CTV-2` serves `spec.html` from the task's branch. A plan written only in the task's worktree is served from there. A worktree path through a symlink gives 404.
- `refuses_what_it_does_not_serve` — no session gives 401; another `Host` gives 403. Each of these gives 404 with an empty body: `.md`; `../`; a path outside `docs/catervas/`; a missing file; `task=nope`; an unknown task; a daemon with no project.

- [ ] `feat(runtime): serve HTML documents sandboxed and fitted to the screen`

### Task 9: ask for a change

Files:
- `crates/roles/src/reviewer.rs`: `reviewer_for`, re-exported.
- `crates/runtime/src/tools/folders.rs`: `file_rederive` calls it.
- `docs/schemas/event.schema.json`: `taskCreatedBody.filed_as`; the `rederives` pattern becomes `\.agent\.(md|html)$`.
- `crates/store/src/requests.rs`: `Filing.filed_as`, `file_small_request`, `change_request`.
- `crates/runtime/src/orchestrator/requests.rs`: `is_to_be_judged`.
- `docs/schemas/rpc.schema.json`: `files.ask_change`, with params `{ path: 1..512, words: 1..2000 }` and result `{ task_id }`.
- `crates/runtime/src/daemon/documents.rs`: `ask_change`, and `call` dispatching by method.

`change_request`'s contract: R1 "`<path>` makes the change the owner asked for."; C1, by review, its rubric "`<path>` makes the change the owner's words ask for.", "Nothing else in `<path>` changes.", and for a pair "`<twin>` says what `<path>` now says."; `in_scope` the paths; `out_of_scope` "Every other file".

Refusals are coded through `refusal(code, "/path", words)`:
- `not_editable`: "This document can't be changed from here."
- `no_owner`: "No one on your team owns this folder right now. Resume or add its role first."
- `no_task_taker`: "The Scrum Master keeps this document up to date at its retro."

Tests in each `mod tests`:

- `files_the_owners_change_for_the_folders_role` — on `architecture/overview.html`, the task has:
  - `assignee_role: architect`, `created_by: human`, `filed_as: owner_change`;
  - title "Change Overview";
  - an intent quoting the words;
  - `allowed_paths` `[overview.html]`;
  - `request.triaged { small }` after it.
  
  The answer names the task.
- `a_pairs_change_reaches_both_files` — on `product/spec.html`, the paths are `spec.html` and `spec.agent.html`.
- `a_long_name_is_cut_to_the_titles_limit` — the title is 120 characters, ending "…".
- `an_owners_change_is_judged_without_refining` — `is_to_be_judged` holds for a contract whose `task.created` has `filed_as`, with no `contract.written`.
- `refuses_an_ask_the_owner_may_not_make` — gives `not_editable` for an agent version or a marketing plan, `no_owner` for a folder with no active agent, and `no_task_taker` for `delivery/cadence.md`. Nothing is filed.
- `rederive_keeps_its_reviewer` — `reviewer_for` answers what `file_rederive` chose before, for a team without the preferred reviewer.

- [ ] `feat(runtime): the owner asks the team to change a document`

### Task 10: rewrite the documents as HTML

Files:
- `docs/schemas/event.schema.json`: `document_rewrite.started`, with body `{ tasks: [taskId] 0..6 }`.
- The event's hand lists: `crates/protocol/src/event.rs` (the kind-to-definition map, `attribution`, `ALL` and `kind()`), `crates/protocol/src/lib.rs:146`, `crates/protocol/src/event/fixtures.rs` and `crates/store/src/projections.rs:786`.
- `crates/store/src/requests.rs`: `rewrite_request` and `rewrite_tasks`.
- `docs/schemas/rpc.schema.json`: `files.rewrite`.
- `crates/runtime/src/daemon/documents.rs`: `rewrite`.
- `crates/runtime/src/orchestrator/{rules,messages}.rs`: `rewrite_delivery`.

`document_rewrite.started` is not added to `is_about_one_contract` (`event.rs:266`), since it names many tasks; `attribution` answers `None` for it; `EVERY_KIND` goes from 111 to 112.

Refusals:
- `not_html`: "Turn on Write documents as HTML first."
- `rewrite_open`: "Your team is still rewriting its documents."
- `nothing_to_rewrite`: "Every document your team can rewrite is already HTML."

Tests in each `mod tests`:

- `files_a_rewrite_task_for_each_folder_with_markdown` — with `spec.md` and `spec.agent.md` in the Product Manager's folder, `overview.md` in the Architect's, and only `plans/MP-1.md` in Marketing's, two tasks are filed:
  - each has `filed_as: document_rewrite`, `allowed_paths` its folder, and an artifact criterion for each named document's `.html`;
  - the Product Manager's names `spec.md`, and the Architect's names `overview.md`.
  
  `document_rewrite.started` lists both.
- `a_role_without_a_reviewer_gets_no_rewrite_task` — a team in which no one can review the UI/UX Designer's work files no Designer task.
- `a_delivery_only_rewrite_records_no_tasks` — with only `delivery/cadence.md` as Markdown, `document_rewrite.started { tasks: [] }` is recorded and the answer is `{ tasks: [] }`.
- `refuses_a_rewrite_it_cannot_start` — gives:
  - `not_html` for a Markdown team;
  - `nothing_to_rewrite` with no `.md` but plans and no delivery `.md`;
  - `rewrite_open` while a task of the last rewrite is `ready`.
  
  With one task `accepted` and one `cancelled`, a new rewrite is filed.
- `the_retro_rewrites_delivery_after_a_rewrite` — with a rewrite started and `delivery/cadence.md` there, the Scrum Master's retro message holds the design's line. It does not without the rewrite, with `cadence.html` only, or with a Product Manager running the retro.
- `reads_document_rewrite_started` (protocol) — the event round-trips with zero tasks and with two.

- [ ] `feat(runtime): rewrite the team's documents as HTML`

### Task 11: reading and changing HTML in the web app

Files:
- `apps/web/src/components/DocumentFrame.{tsx,module.css,test.tsx}`, created.
- `apps/web/src/pages/files.ts`: `isHtml`, and `askable` on `FileRead`.
- `FileDocument.tsx`:
  - for HTML, the title is the document's name and `DocumentFrame` (with `version` set to the blob) replaces `Markdown`;
  - "What should change?" with "Send to <owner>" calls `files.ask_change` wherever `askable`, Markdown included;
  - `ShowAdvanced` holds "Edit source" for HTML, which opens today's editor without the Preview tab.
- `ProductPlanCard.tsx`: frames HTML with `task`, and its names are keyed by stem.
- `DocumentChanges.tsx`: `plain(text, path)`, and names keyed by stem.
- `MarketingPlan.tsx`: for `format: "html"`, `DocumentFrame` with `task={plan.taskId}` and `path` `docs/catervas/marketing/plans/${plan.plan}.html` replaces the `<pre>`.
- `Files.module.css`: the frame is 100% wide, with `height: calc(100dvh - 12rem)` and `min-height: 60vh`, and no border.
- `packages/protocol-client/src/client.ts`: `files.ask_change`.
- `app/refusals.ts`: `not_editable`, `no_owner` and `no_task_taker`, each with its words from Task 9.
- `strings/en.ts`:
  - `docOpenFull` "Open full page";
  - `docAskLabel` "What should change?";
  - `docAskSend` "Send to {owner}";
  - `docAskSent` "Sent. {owner} has it as {task}.";
  - `docEditSource` "Edit source";
  - `refuseNotEditable`, `refuseNoOwner` and `refuseNoTaskTaker`.
- `apps/web/vite.config.ts`: `/documents`.
- `apps/web/e2e/files.spec.ts`.

Tests in `DocumentFrame.test.tsx`, `pages/Files.test.tsx`, `ProductPlanCard.test.tsx`, `DocumentChanges.test.tsx`, `MarketingPlan.test.tsx` and the end-to-end spec:

- `frames_the_document_sandboxed` — an `iframe` whose `src` is `/documents/raw?path=<encoded>&v=<version>` (with `&task=` when given). Its `sandbox` attribute is present and empty, and it has its title. "Open full page" links to the same URL with `target="_blank"` and a `rel` containing `noopener`.
- `shows_an_html_document_in_its_frame` — `overview.html`: no Markdown output, and a frame for the path. "For the team" frames the twin. A new blob after Save changes `src`.
- `asks_for_a_change` — Send calls `files.ask_change { path, words }` and shows the sent line; `no_owner` shows its words in an alert; there is no box when `askable` is false.
- `edits_html_source_under_advanced` — no Edit for HTML until Advanced is open. "Edit source" has no Preview tab, and Save calls `files.save`.
- `the_product_plan_card_frames_html` — `spec.html` and `roadmap.html` give two frames with `task`, named "Product description" and "Roadmap".
- `html_changes_read_as_words` — `<li>Ship <b>v2</b> &amp; more</li>` in `spec.html` shows "Ship v2 & more"; `<h2>Risks</h2>` shows as a heading; spec.html is named "Product description".
- `an_html_marketing_plan_is_framed` — `format: "html"` gives a frame and no `<pre>`; `"markdown"` keeps the `<pre>`.
- e2e `files.spec.ts`, "an HTML document fits a phone":
  - at 375 × 812 px, the frame shows the document's `<h1>`;
  - the page's `scrollWidth <= clientWidth`;
  - the frame's document cannot run a script (a `<script>` setting `document.title` leaves it unchanged).

- [ ] `feat(web): show HTML documents in a sandboxed frame`

### Task 12: the setting and the rewrite in Settings

Files:
- `apps/web/src/pages/TeamRules.tsx`: `Documents(part)`, with part key `"documents"`.
- `settings/Work.tsx`: renders it after `Planning`.
- `packages/protocol-client/src/client.ts`: `files.rewrite`.
- `app/refusals.ts`: `not_html`, `nothing_to_rewrite` and `rewrite_open`.
- `strings/en.ts`:
  - `rulesDocuments` "Documents";
  - `documentsHtml` "Write documents as HTML";
  - `documentsHtmlNote` "New documents are web pages with diagrams and charts.\nDocuments already in Markdown stay as they are.";
  - `documentsRewrite` "Rewrite documents as HTML";
  - `documentsRewriteNote` "Each role rewrites its own documents as HTML.\nMarketing plans stay as they were approved.";
  - `documentsRewriteStarted` "Your team will rewrite its documents as HTML.";
  - the three refusals' words from Task 10.

Tests in `apps/web/src/pages/settings/settings.test.tsx`, whose Work heading list at `:236` gains "Documents":

- `switches_the_document_format` — on for a team without the key; turning it off saves `policy.documentFormat: "markdown"`.
- `offers_the_rewrite_while_markdown_remains` — an HTML team with `overview.md` in the tree shows the button, which calls `files.rewrite` and then the started line. There is no button with only `.html` documents and plans, nor in a Markdown team.
- `says_why_a_rewrite_was_refused` — `rewrite_open` shows its words under the button.

- [ ] `feat(web): the document format and the rewrite in Settings`

### Task 13: the spec

Files: `docs/SPEC.md`:
- the revision 0.107 line;
- 4.5: the frame, the fit, Open full page, Ask for a change, Edit source, and the framed marketing plan;
- 5.12: `document_format`;
- 5.17: either format, the format rule, the route, the methods and the event, the rewrite, the retro's delivery line;
- each kit's `catervafying-the-repository` line, by stem.

`docs/design/catervas-folders.md`: a line under its status pointing to `html-documents.md`.

- [ ] `docs(spec): the team's documents in HTML (0.107)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
# (in a container where it cannot run as is: /tmp/claude-0/fullcheck.sh, same expected line)
```
