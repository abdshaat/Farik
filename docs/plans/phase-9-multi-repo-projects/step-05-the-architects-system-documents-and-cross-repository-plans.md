# Phase 9, step 05: The Architect's system documents and cross-repository plans

Status: ready (readiness review 2026-10-12, two rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.3, 5.9, 5.16, 5.17, 6.1, 6.3, 8.2; F5
Depends on these steps of this phase, planned and ready or in review, not yet executed:
- step 01: `read_repos`
- step 02: the contract's `repo`, `ReadinessContext.repos`, `RepoRequired`, the fixtures
- step 03: `MemberLine`, `PromptInput.members`, the repositories line
- step 04: member folders, `folder_of`, `document_name`'s member paths, `DOCUMENT_STEMS`

Nothing of steps 02b, 02c, 06 to 08.
Mockups: none.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on B1 (agent-written purposes in the Team rules, against ADR 0011), decided below by route (a), and B2 (the refresh test's team); S1–S6 and N1–N8 folded. Round 2 (the same reviewer, confirming): ready; its S1 and N1 folded. Re-checked after the amendment by a fresh-session Opus reviewer, 2026-10-13: not ready on a purpose spanning lines and two tests of one function (ADR 0060); both folded, with its minor notes. Ready.
Amended 2026-10-13 for the HTML documents merged from main (ADR 0058); status: ready.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 93593b6.

## Goal

The Architect of a project folder knows how its repositories connect and writes three documents for it, in the top-level `docs/catervas/architecture/`, each `<stem>.html` or `<stem>.md` as any document is (ADR 0058: new ones in the team's format, HTML unless set; an existing one keeps its format):
- `system`, the map
- `interfaces`, the contracts between repositories
- `deployment`, what runs where

Their contents come from a new kit skill, `mapping-the-system`. Each member's one-line purpose, from `system`, reaches every session as untrusted text beside the project scan (ADR 0011).

An approved epic's plan may span repositories:
- each planned task names its repository, and a provider's task comes before its consumers'
- the Product Manager writes each contract's `repo` from the plan
- `lanes_overlap` compares only tasks in the same repository
- an epic's paths bound only its tasks in the project folder

At each sprint review, the Architect's refresh in a project folder also covers each member's architecture documents and, when the sprint changed how repositories connect, the three system documents.

A one-repo project is unchanged.

Out of scope: the Catervafication's first writing of the three documents (06).

## Decisions

- **The three documents' shapes.** The Architect writes them as `mapping-the-system` says, in the format the Team rules' documents line gives (`format_line`, `folders.rs:327`): an HTML page with inline SVG diagrams, or Markdown with Mermaid blocks. Only `system`'s Repositories list is read by code (below); the rest is for agents and the owner.
  - **`system`:** a first heading "System"; a second-level heading "Repositories" (`<h2>Repositories</h2>`, or `## Repositories`) holding a list, one item per member in `repos.yaml` order, each the member's name as code, a colon and its purpose in one sentence (`<li><code>app</code>: The web shop.</li>`, or `` - `app`: The web shop. ``); a "Map" section with one diagram, a node per member and an edge per link, labelled `calls`, `deploys`, `depends on`, `shares data` or `sends events`; a "Links" section, one line per edge with the file each was read from.
  - **`interfaces`:** one second-level section per contract between repositories, with `Kind` (HTTP API, gRPC, event, package, schema, configuration), `Provider`, `Consumers` and `Defined in` (a path in the provider).
  - **`deployment`:** a first heading "Deployment", one diagram of what runs where, and one line per running piece with the member that builds it and the file it was read from (Terraform, Kubernetes, Compose, a CI deploy job).
  - All three are agent-only documents, not human documents: the owner reads them on the Files page (an HTML one in its sandboxed frame) and may edit them, and their writer is the Architect, through docs tasks and `catervas_write_folder_doc` (phase 8).
- **Names on the Files page.** `DOCUMENT_STEMS` (`crates/core/src/folders.rs:372`), keyed by stem since main's HTML change, gains `docs/catervas/architecture/system` "System map", `…/interfaces` "Interfaces" and `…/deployment` "Deployment", in that order, between "Overview" and "Features". It goes to 15. (A `….md` entry would compile and never match: the lookup compares `stem_of(path)`, `:404`.)
- **The purposes are untrusted.** `system` is written by the Architect from what it read in the repositories, so its text is untrusted under ADR 0011, which wraps what an agent or a repository file wrote and leaves only the user's own words, the Team rules among them, unwrapped (`prompt.rs:256-296`). So the purposes are not put into step 03's repositories line, which stays as step 03 has it.
  - They are a block `untrusted("system_map", <lines>, 4 * KIB)` in `The project`, after `project_scan` (`prompt.rs:259-262`), one line per member: `<name>: <purpose>`. `The project`'s body is the `project_scan` block, then the `system_map` block, each optional, joined by a blank line; the section appears when either is there. The sections stay eleven (`bodies`, `prompt.rs:256`, zipped with `PROMPT_SECTIONS`, `:95`), as ADR 0011 has them.
  - `PromptInput` gains `system_map: Option<&'a str>`, the block's text, `None` when there are no purposes. (It already gained `document_format`, `prompt.rs:66`.)
  - Rejected: the purposes in the Team rules with ADR 0011 amended. The line's sanitising keeps it from breaking, but it does not make agent text the user's.
- **Reading the purposes.** Core's `member_purposes(text: &str, format: DocumentFormat) -> Vec<(String, String)>` reads the Repositories list of either format, in its order:
  - Markdown: the lines after `## Repositories` up to the next `## `, each of the form `` - `<name>`: <purpose> ``.
  - HTML: the span from an `<h2…>` whose text is `Repositories` to the next `<h2`, split on `<li`; in each item, the text of its first `<code>…</code>` is the name, and what follows the first `:` after that `</code>` up to `</li>` is the purpose, with every tag removed and `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;` decoded in one pass (so `&amp;lt;` stays `&lt;`). An item with no `<code>…</code>`, no `:` after it, or no `</li>` is not of that form. Nothing is read by line: the one-block-per-line rule is asked of agents and not enforced (ADR 0058).
  - In each purpose every run of whitespace, line breaks included, is written as one space; then it is trimmed, cut to 120 characters at a character boundary, and trimmed again. An item not of that form, or whose purpose is empty after trimming, is skipped. A name given twice keeps its first purpose. A missing section gives nothing.
  - `session_spec_without` (`orchestrator/session.rs:1223`) finds the document with `held_document(git, branch, "docs/catervas/architecture/system", team.document_format())` (`crates/store/src/git.rs:1134`) on the project folder's integration branch, reads it with `git.file_at` and its format with `DocumentFormat::of`, for a session whose read roots are not empty (step 03), and keeps only names in `repos.yaml`. A missing file, or one git cannot read, gives no block.
- **The skill.** `crates/roles/roles/architect/skills/mapping-the-system/SKILL.md`, under 6 KiB, with no ` @`. It is listed in `kit.yaml` and in `kit.rs`'s `embedded_skills()` before `catervafying-the-repository`, which stays last (`kit.rs:2300`), so the Architect's kit goes to 8 skills (`kit.yaml:1`'s comment says so).
  - Its sections: `## 1. Read every repository`, which says the members are listed in the repositories line and read through their paths; `## 2. The system map`, `## 3. The interfaces` and `## 4. The deployment view`, each the shape above with a minimal example, naming each document by stem (`docs/catervas/architecture/system`, `interfaces`, `deployment`) and its drawing as "a diagram"; `## 5. Each link from evidence`, which says to name the file each fact was read from and to leave out what it could not find; `## 6. Finish`, which says to write through `catervas_write_folder_doc` in a ceremony, or commit in a docs task with `catervas_git_commit`.
  - The skill leaves how to write HTML or Markdown to the Team rules' documents line (`format_line`). The one format-specific thing it shows is a Repositories item, once in each format (`<li><code>app</code>: The web shop.</li>` and `` - `app`: The web shop. ``), since code reads it. It must pass the merge's `kits_name_documents_without_an_extension` (`crates/roles/src/lib.rs:1522`), which fails any Architect file holding "mermaid" or a `…/x.md` path.
  - Its frontmatter `description` is "Use when the project has several repositories: map how they connect, the contracts between them and what runs where, in docs/catervas/architecture/.", written as a quoted YAML scalar since it holds `: `.
  - `architect/system.md`'s folder line (`:24`) names the three documents by stem, `system`, `interfaces` and `deployment`, and the members' `repos/<name>/architecture/`. `role.yaml`'s `produces` (`:10-13`) gains "the system map, the interfaces between repositories and the deployment view, in a project of several repositories".
- **Plans across repositories.** `planning-in-lanes/SKILL.md` gains a section `## 2b. Repositories` (between "Lanes" and "The outline"):
  - In a project of several repositories, each planned task names its `repo`.
  - A task that changes an interface in the `interfaces` document comes before every task that uses the change, named in its "depends on".
  - When a change must reach two repositories at once, the plan says how it stays compatible in between (add the new field before removing the old).
  - A lane may cross repositories.

  The outline's per-task fields (`:43-45`) gain `repo`, "in a project of several repositories".
  - The Product Manager's `writing-task-contracts/SKILL.md` §7 (`:105-121`) says each task gets its `repo` from the plan when the plan names one, and with no Architect, in a project of several repositories, the Product Manager names it itself.
  - `plan_tasks_message` (`orchestrator/messages.rs:201`) adds "its `repo` when the plan names one" to the fields it lists, so a one-repo project's plan asks for none (step 02's `RepoNotAllowed`).
- **Lanes and parents compare within a repository.**
  - `LanedTask` (`readiness.rs:133`) gains `repo: Option<String>`. `parent_state` (`transitions.rs:872`, its `LanedTask` at `:900`) fills it from each sibling's contract.
  - `lanes_overlap` (`readiness.rs:1152`) compares only siblings whose `repo` equals the contract's, `None` with `None`: allowed paths in two repositories never meet.
  - `paths_within_parent` (`readiness.rs:1077`) leaves out a task when `context.repos.is_some()` and the task has a `repo`, since an epic's `allowed_paths` are the project folder's and bound only its tasks there. A member task's paths are bounded by its own contract and the Definition of Done in its member.
  - `lane_beyond_builders` is unchanged: builders are counted across the team.
- **The sprint review's refresh in a project folder.**
  - `due_refresh`'s Architect branch (`rules.rs:743`, its `owed` at `:765`) switches on `files.is_project_folder()` (step 01). In a project folder it owes the refresh when a new stem `ARCHITECTURE_SYSTEM = "docs/catervas/architecture/system"` (beside the stems at `rules.rs:651-660`) is held on the integration branch in either format (`held_on_the_integration_branch`, `:719`), in place of `ARCHITECTURE_STRUCTURE`. It still skips a sprint that integrated nothing. The members are `read_repos`' names; an unreadable `repos.yaml` owes no refresh that review, as a missing `structure` owes none today.
  - The system documents' paths are `document_on_the_integration_branch` (`rules.rs:679`) of the `system`, `interfaces` and `deployment` stems, built beside `architecture_files` (`:826-830`): each as the branch holds it, else in the team's format.
  - `SprintTask` (`messages.rs:666`) gains `repo: Option<String>`, which `sprint_tasks` (`rules.rs:628-649`) fills by reading each row's contract. A contract it cannot read gives `None`.
  - `architecture_refresh_message` (`messages.rs:829`), which since main's HTML change takes the three architecture paths as held (`[features, jobs, structure]: &[String; 3]`), gains `members: &[&str]` and `system: &[String; 3]`. With members it opens with exactly: `Bring docs/catervas/repos/<name>/architecture/features, jobs and structure up to date for each repository a task below went into, each in the format it is in (a new one in the team's), with what sprint <n> added to it, and {system}, {interfaces} and {deployment} when a task changed how the repositories connect. The repositories: <members joined ", ">.` Here `<name>` stays literal in the text, as an instruction, `<n>` is the sprint's number, `{system}` is the system path in full and `{interfaces}` and `{deployment}` are file names (`file_name`, `messages.rs:765`), as the one-repo message names its files. The rest of the message is today's text, from "Change only what these tasks changed" on.
  - Each listed task's line in the block (today `{id} ({title}): {note}`, `messages.rs:834-845`) becomes `{id} ({title}) (in {repo}): {note}`, or `(in the project's own folder)` for none.
  - Whether a task changed how they connect is the Architect's judgement from the tasks' notes and diffs, which it reads. Rejected: Catervas deciding it from changed paths, which cannot tell an interface from any other file.
  - With `members: &[]`, in a one-repo project, the message and the condition are today's exactly (`system` unused).

## File map

```
crates/core/src/folders.rs                         modifies, tests: DOCUMENT_STEMS, member_purposes (Task 1)
crates/runtime/src/prompt.rs, orchestrator/session.rs   modifies, tests: PromptInput.system_map, the untrusted block, read from the system document (Task 1)
crates/roles/roles/architect/skills/mapping-the-system/SKILL.md   creates (Task 2)
crates/roles/roles/architect/{kit.yaml,system.md,role.yaml}       modifies (Task 2)
crates/roles/src/kit.rs, crates/roles/src/lib.rs   modifies, tests: the eighth skill, its wording (Task 2); the Product Manager's §7 (lib.rs, Task 3)
crates/roles/roles/architect/skills/planning-in-lanes/SKILL.md, crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md   modifies (Task 3)
crates/runtime/src/orchestrator/messages.rs        modifies, tests: plan_tasks_message (Task 3); architecture_refresh_message, SprintTask's literal at `:2187`, the call at `:2255` with `&[]` for `members` and the three held paths for `system` (Task 4)
crates/core/src/governor/readiness.rs, readiness/fixtures.rs   modifies, tests: LanedTask.repo (and the `a_lane` literal, `readiness.rs:2592`), lanes_overlap, paths_within_parent (Task 3)
crates/runtime/src/transitions.rs                  modifies, tests: parent_state's repo, the literal at `:3086` (Task 3)
crates/runtime/src/orchestrator/rules.rs           modifies, tests: due_refresh in a project folder (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 02's `repo`, `ReadinessContext.repos`, the fixtures
- step 03's `MemberLine`, `rules_section`, `session_spec_without`'s members
- step 04's `DOCUMENT_STEMS` place, member folders
- `LanedTask`, `ParentState`, `lanes_overlap`, `paths_within_parent`, `parent_state`
- `embedded_skills`, `load_kit`
- `plan_tasks_message`, `architecture_refresh_message`, `due_refresh`, `Refresh`, `SprintTask`
- `Git::file_at`, `integration_branch`, `held_document` (`git.rs:1134`), `DocumentFormat`; `held_on_the_integration_branch`, `document_on_the_integration_branch` (`rules.rs:719`, `:679`)
- step 01's `read_repos` and `ProjectFiles::is_project_folder`
- `untrusted` (`prompt.rs`)
- `Harness::project_folder_with` (step 02)

Produces:

```rust
// catervas_core::folders
pub const DOCUMENT_STEMS: [(&str, &str); 15];
pub fn member_purposes(text: &str, format: DocumentFormat) -> Vec<(String, String)>;
// catervas_core::governor::readiness::LanedTask gains `pub repo: Option<String>`
// catervas_runtime::prompt::PromptInput gains `pub system_map: Option<&'a str>`
// orchestrator::messages
pub(super) fn architecture_refresh_message(sprint_id: &str, integrated: &[SprintTask], files: &[String; 3], members: &[&str], system: &[String; 3]) -> String;   // was three arguments
// SprintTask gains `repo: Option<String>`
```

## Tasks

### Task 1: The system map's purposes reach every session

Files: `folders.rs`; `prompt.rs` (the block; `Inputs::full`, `:744`, sets `system_map: None`); `orchestrator/session.rs` (`:1223`).

- `reads_each_members_purpose`: a Markdown `system` whose `## Repositories` holds `` - `app`: The web shop (React). ``, `` - `infra`: AWS; Terraform. ``, `` - `app`: again ``, `` - `empty`:   ``, a line `- web is new`, and a `## Map` after, gives `[(app, "The web shop (React)."), (infra, "AWS; Terraform.")]`; the same list as HTML, written on one line, with `<h2 id="r">Repositories</h2>`, `<li><code>app</code>: The web shop <em>(React)</em>.</li>`, `<li><code>infra</code>: AWS &amp; Terraform.</li>` and an `<h2>Map</h2>` after, gives `app`'s purpose and `infra`'s as `AWS & Terraform.`. A 200-character purpose is cut to 120 characters with no trailing space. Text with no Repositories section gives `[]`. RED.
- `names_the_system_documents`: `document_name` of `docs/catervas/architecture/system.html` is "System map", of `interfaces.md` "Interfaces", of `deployment.html` "Deployment", ordered between "Overview" and "Features". RED.
- `a_session_reads_the_purposes_from_the_system_map` (`orchestrator/session.rs`, `Harness::project_folder`): with `system.html` on the project folder's `main` (the Harness team writes HTML) naming `app`'s and `ghost`'s purposes (`ghost` not a member), a Developer's implement prompt carries an `<untrusted source="system_map">` block with `app`'s line alone, after the project scan's block and before the Team rules, and the Team rules' repositories line is step 03's exactly. With no `system` document there is no block. RED.

- [ ] `feat(runtime): tell each session what each repository is for`

### Task 2: The Architect maps the system

Files: the new `SKILL.md`; `kit.yaml`; `system.md`; `role.yaml`; `kit.rs` (`embedded_skills`, the tests); `lib.rs` (the tests).

- `the_system_skill_names_its_three_documents` (`kit.rs`, beside `the_lanes_skill_names_the_plan_and_its_limits`, `:2476`): its description starts with "Use when", and `mapping-the-system` holds `docs/catervas/architecture/system`, `interfaces`, `deployment`, `Repositories`, `diagram`, `catervas_write_folder_doc` and `catervas_git_commit`. It is under 6 KiB with no ` @`, and `kits_name_documents_without_an_extension` still passes. RED.
- `architect_kit_carries_its_skills` (`:2451`) lists eight, `mapping-the-system` before `catervafying-the-repository`. `every_owner_carries_catervafying_the_repository` (`:2300`) pins the Architect at 8. RED: seven.
- `the_architects_prompt_names_the_system_documents` (`lib.rs`, beside `planning_follows_the_architects_plan`, `:1778`): the Architect's prompt holds `` `system` ``, `` `interfaces` ``, `` `deployment` `` and `repos/<name>/architecture/`. RED.

- [ ] `feat(roles): the Architect maps how the repositories connect`

### Task 3: Plans span repositories

Files: the two skills; `messages.rs` (`plan_tasks_message`); `readiness.rs`; `readiness/fixtures.rs`; `transitions.rs`; `crates/roles/src/lib.rs` (the test).

- `lanes_in_different_repositories_never_meet` (`readiness.rs`): an epic's tasks in lanes 1 and 2 both with `allowed_paths: [src/**]`. With `repo` `app` and `infra` they pass `LanesOverlap`; both in `app` fail it as today; both `None` fail it as today. RED.
- `an_epic_bounds_only_its_tasks_in_the_project_folder` (`readiness.rs`): an epic with `allowed_paths: [docs/catervas/**]`, under a context with `repos: Some(["app"])`, a child with `repo: app` and `[src/**]` passes `PathsWithinParent`; a child with no `repo` and `[src/**]` fails it as today. RED.
- `reads_each_siblings_repository` (`transitions.rs`, beside `reads_the_lanes_of_an_epics_other_tasks`, `:3062`): a sibling with `repo: infra` and lane 2 gives a `LanedTask` with `repo: Some("infra")`. RED.
- `the_plans_name_each_tasks_repository` (`kit.rs` for `planning-in-lanes`, `lib.rs` beside `planning_follows_the_architects_plan`, `:1778`, for the Product Manager's role skill, and `messages.rs`): `planning-in-lanes` holds `## 2b. Repositories`, `repo` and `depends on`. `writing-task-contracts` §7 holds "its `repo` from the plan when the plan names one". `plan_tasks_message` holds "its `repo` when the plan names one". RED.

- [ ] `feat(core): plan an epic across repositories`

### Task 4: The sprint review refreshes the system

Files: `messages.rs` (`architecture_refresh_message`, `SprintTask`); `rules.rs` (`due_refresh`).

- `writes_the_project_folders_refresh_message` (`messages.rs`, beside `writes_the_refresh_messages`, `:2227`): with the HTML paths `…/architecture/features.html`, `jobs.html`, `structure.md` and `system.html`, `interfaces.html`, `deployment.html`, members `[app, infra]` and tasks `CTV-3` titled "Pay" with note "done" (`repo: app`) and `CTV-4` (no `repo`), the message opens with the Decisions' two sentences for sprint 1, naming `docs/catervas/architecture/system.html, interfaces.html and deployment.html`, the second `The repositories: app, infra.`, and its block holds `CTV-3 (Pay) (in app): done` and `CTV-4 (…) (in the project's own folder): …`. With `members: &[]` it is today's message exactly. RED.
- `refreshes_a_project_folders_architecture` (`rules.rs`, as `refreshes_the_product_and_the_architecture_after_the_review`, `:8950`, on `Harness::project_folder_with("orch-refresh-x", &["app", "infra"], with_a_scrum_master_and_an_architect)` as `rules.rs:8888` makes its team, with `system.html` and no `structure` on `main`): `a_sprint_of_one_task` runs, and after `accepted` the test writes `repo: app` into CTV-1's contract. Then the Architect's refresh session runs with the project folder's message. With no `system` document, and with `structure.html` present but no `system`, it does not run. A sprint that integrated nothing does not run it. RED: no `structure` is there, so it is never owed.

- [ ] `feat(runtime): refresh the system's documents at the sprint review`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.17 and 6.3: the three documents by stem, their shapes in either format, `mapping-the-system`, the purposes and how they are read from HTML or Markdown
- 5.16 and 6.1: plans naming each task's `repo`, provider before consumer
- 5.3: `lanes_overlap` and `paths_within_parent` within a repository
- 5.9: the sprint review's refresh condition and message in a project folder (where 5.9 says it runs only when `docs/catervas/architecture/structure` is there)
- 6.3: the Architect's kit count, now eight
- 8.2: the `system_map` untrusted block in `The project`

- [ ] `docs(spec): record the system map and plans across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
