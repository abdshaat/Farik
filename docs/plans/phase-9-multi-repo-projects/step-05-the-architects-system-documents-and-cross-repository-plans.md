# Phase 9, step 05: The Architect's system documents and cross-repository plans

Status: draft
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.3, 5.16, 5.17, 6.1, 6.3, 8.2; F5
Depends on these steps of this phase, planned and ready or in review, not yet executed:
- step 01: `read_repos`
- step 02: the contract's `repo`, `ReadinessContext.repos`, `RepoRequired`, the fixtures
- step 03: `MemberLine`, `PromptInput.members`, the repositories line
- step 04: member folders, `folder_of`, `document_name`'s member paths, `DOCUMENT_NAMES`

Nothing of steps 02b, 02c, 06 to 08.
Mockups: none.
Readiness confirmed by: (pending; one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 6e5b303.

## Goal

The Architect of a project folder knows how its repositories connect and writes three documents for it, in the top-level `docs/catervas/architecture/`:
- `system.md`, the map
- `interfaces.md`, the contracts between repositories
- `deployment.md`, what runs where

Their contents come from a new kit skill, `mapping-the-system`. Each member's one-line purpose, from `system.md`, reaches every session's repositories line.

An approved epic's plan may span repositories:
- each planned task names its repository, and a provider's task comes before its consumers'
- the Product Manager writes each contract's `repo` from the plan
- `lanes_overlap` compares only tasks in the same repository
- an epic's paths bound only its tasks in the project folder

At each sprint review, the Architect's refresh in a project folder also covers each member's architecture documents and, when the sprint changed how repositories connect, the three system documents.

A one-repo project is unchanged.

Out of scope: the Catervafication's first writing of the three documents (06).

## Decisions

- **The three documents' shapes.** The Architect writes them as `mapping-the-system` says. Only `system.md`'s `## Repositories` list is read by code (below); the rest is for agents and the owner.
  - **`system.md`:** `# System`; `## Repositories`, one line per member, `` - `<name>`: <purpose, one sentence> ``, in `repos.yaml` order; `## Map`, one fenced `mermaid` `flowchart LR` with a node per member and an edge per link, labelled `calls`, `deploys`, `depends on`, `shares data` or `sends events`; `## Links`, one line per edge with the file each was read from.
  - **`interfaces.md`:** one `## <interface>` per contract between repositories, with `Kind` (HTTP API, gRPC, event, package, schema, configuration), `Provider`, `Consumers` and `Defined in` (a path in the provider).
  - **`deployment.md`:** `# Deployment`, one fenced `mermaid` diagram of what runs where, and one line per running piece with the member that builds it and the file it was read from (Terraform, Kubernetes, Compose, a CI deploy job).
  - All three are agent-only documents, not human documents: the owner reads them on the Files page and may edit them, and their writer is the Architect, through docs tasks and `catervas_write_folder_doc` (phase 8).
- **Names on the Files page.** `DOCUMENT_NAMES` (`crates/core/src/folders.rs:225`) gains `docs/catervas/architecture/system.md` "System map", `…/interfaces.md` "Interfaces" and `…/deployment.md` "Deployment", in that order, after "Overview". It goes to 15.
- **The purpose line.** Core's `member_purposes(system_md: &str) -> BTreeMap<String, String>` reads `## Repositories`' lines of the form above.
  - Each purpose is trimmed, cut to 120 characters at a character boundary, and has every `;`, `(`, `)` and line break replaced by `,`, so that it cannot break the prompt's line.
  - A line not of that form is skipped, and a missing section gives an empty map.
  - `session_spec_without` (`orchestrator/session.rs:1138-1160`) reads `system.md` from the project folder's integration branch (`git.file_at`). A missing file, or one git cannot read, gives no purposes.
  - `MemberLine` (step 03) gains `purpose: Option<String>`, rendered first inside the entry's parentheses, followed by `; `: `infra (the shop's AWS infrastructure; /home/me/infra; /repos/infra in your sandbox)`, and `app (the web shop; your task's repository)` for the task's own.
- **The skill.** `crates/roles/roles/architect/skills/mapping-the-system/SKILL.md`, under 6 KiB, with no ` @`. It is listed in `kit.yaml` and in `kit.rs`'s `embedded_skills()` before `catervafying-the-repository`, which stays last (`kit.rs:2300`), so the Architect's kit goes to 8 skills (`kit.yaml:1`'s comment says so).
  - Its sections: `## 1. Read every repository`, which says the members are listed in the repositories line and read through their paths; `## 2. system.md`, `## 3. interfaces.md` and `## 4. deployment.md`, each the shape above, with a minimal example; `## 5. Each link from evidence`, which says to name the file each fact was read from and to leave out what it could not find; `## 6. Finish`, which says to write through `catervas_write_folder_doc` in a ceremony, or commit in a docs task with `catervas_git_commit`.
  - `architect/system.md`'s folder line (`:23`) names the three documents and the members' `repos/<name>/architecture/`. `role.yaml`'s `produces` (`:115-118`) gains "the system map, the interfaces between repositories and the deployment view, in a project of several repositories".
- **Plans across repositories.** `planning-in-lanes/SKILL.md` gains a section `## 2b. Repositories` (between "Lanes" and "The outline"):
  - In a project of several repositories, each planned task names its `repo`.
  - A task that changes an interface in `interfaces.md` comes before every task that uses the change, named in its "depends on".
  - When a change must reach two repositories at once, the plan says how it stays compatible in between (add the new field before removing the old).
  - A lane may cross repositories.

  The outline's per-task fields (`:42-44`) gain `repo`.
  - The Product Manager's `writing-task-contracts/SKILL.md` §7 (`:114-117`) says each task gets its `repo` from the plan, and with no Architect the Product Manager names it itself.
  - `plan_tasks_message` (`orchestrator/messages.rs:200`) adds "its `repo`" to the fields it lists.
- **Lanes and parents compare within a repository.**
  - `LanedTask` (`readiness.rs:120-129`) gains `repo: Option<String>`. `parent_state` (`transitions.rs:839-884`) fills it from each sibling's contract.
  - `lanes_overlap` (`readiness.rs:1028`) compares only siblings whose `repo` equals the contract's, `None` with `None`: allowed paths in two repositories never meet.
  - `paths_within_parent` (`readiness.rs:953`) leaves out a task with a `repo`, since an epic's `allowed_paths` are the project folder's and bound only its tasks there. A member task's paths are bounded by its own contract and the Definition of Done in its member.
  - `lane_beyond_builders` is unchanged: builders are counted across the team.
- **The sprint review's refresh in a project folder.**
  - `due_refresh`'s Architect branch (`rules.rs:698-726`) owes it, in a project folder, when `docs/catervas/architecture/system.md` is on the integration branch, in place of `structure.md` (`:609-612`). It still skips a sprint that integrated nothing.
  - `architecture_refresh_message` (`messages.rs:806`) gains `members: &[String]` and each `SprintTask`'s `repo`. In a project folder it opens: "Bring each repository's docs/catervas/repos/<name>/architecture/features.md, jobs.md and structure.md up to date with what sprint <n> added to it, and docs/catervas/architecture/system.md, interfaces.md and deployment.md when a task changed how the repositories connect." Each listed task carries `(in <repo>)` or `(in the project's own folder)`.
  - Whether a task changed how they connect is the Architect's judgement from the tasks' notes and diffs, which it reads. Rejected: Catervas deciding it from changed paths, which cannot tell an interface from any other file.
  - In a one-repo project the message and the condition are today's exactly.

## File map

```
crates/core/src/folders.rs                         modifies, tests: DOCUMENT_NAMES, member_purposes (Task 1)
crates/runtime/src/prompt.rs, orchestrator/session.rs   modifies, tests: MemberLine.purpose, read from system.md (Task 1)
crates/roles/roles/architect/skills/mapping-the-system/SKILL.md   creates (Task 2)
crates/roles/roles/architect/{kit.yaml,system.md,role.yaml}       modifies (Task 2)
crates/roles/src/kit.rs, crates/roles/src/lib.rs   modifies, tests: the eighth skill, its wording (Task 2)
crates/roles/roles/architect/skills/planning-in-lanes/SKILL.md, crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md   modifies (Task 3)
crates/runtime/src/orchestrator/messages.rs        modifies, tests: plan_tasks_message (Task 3); architecture_refresh_message (Task 4)
crates/core/src/governor/readiness.rs, readiness/fixtures.rs   modifies, tests: LanedTask.repo, lanes_overlap, paths_within_parent (Task 3)
crates/runtime/src/transitions.rs                  modifies, tests: parent_state's repo (Task 3)
crates/runtime/src/orchestrator/rules.rs           modifies, tests: due_refresh in a project folder (Task 4)
docs/SPEC.md                                       modifies (Task 5)
```

## Interfaces

Consumes:
- step 02's `repo`, `ReadinessContext.repos`, the fixtures
- step 03's `MemberLine`, `rules_section`, `session_spec_without`'s members
- step 04's `DOCUMENT_NAMES` place, member folders
- `LanedTask`, `ParentState`, `lanes_overlap`, `paths_within_parent`, `parent_state`
- `embedded_skills`, `load_kit`
- `plan_tasks_message`, `architecture_refresh_message`, `due_refresh`, `Refresh`, `SprintTask`
- `Git::file_at`, `integration_branch`

Produces:

```rust
// catervas_core::folders
pub const DOCUMENT_NAMES: [(&str, &str); 15];
pub fn member_purposes(system_md: &str) -> BTreeMap<String, String>;
// catervas_core::governor::readiness::LanedTask gains `pub repo: Option<String>`
// catervas_runtime::prompt::MemberLine gains `pub purpose: Option<String>`
// orchestrator::messages
pub(super) fn architecture_refresh_message(sprint_id: &str, integrated: &[SprintTask], members: &[String]) -> String;   // was two arguments
// SprintTask gains `repo: Option<String>`
```

## Tasks

### Task 1: The system map's purposes reach every session

Files: `folders.rs`; `prompt.rs`; `orchestrator/session.rs`.

- `reads_each_members_purpose`: a `system.md` whose `## Repositories` holds `` - `app`: The web shop (React). `` and `` - `infra`: AWS; Terraform. ``, a line `- web is new`, and a `## Map` after, gives `{app: "The web shop ,React,.", infra: "AWS, Terraform."}`. A 200-character purpose is cut to 120 characters. Text with no `## Repositories` gives `{}`. RED.
- `names_the_system_documents`: `document_name` of `docs/catervas/architecture/system.md` is "System map", of `interfaces.md` "Interfaces", of `deployment.md` "Deployment", ordered after "Overview". RED.
- `the_repositories_line_carries_each_purpose` (`prompt.rs`): with `infra`'s purpose "AWS" and `app` the task's own with purpose "The web shop", the line reads `app (The web shop; your task's repository), infra (AWS; /home/me/infra; /repos/infra in your sandbox)`; with no purposes it is step 03's line exactly. RED.
- `a_session_reads_the_purposes_from_the_system_map` (`orchestrator/session.rs`, `Harness::project_folder`): with `system.md` on the project folder's `main` naming `app`'s purpose, a Developer's implement prompt carries it. With no `system.md` the line has no purposes. RED.

- [ ] `feat(runtime): tell each session what each repository is for`

### Task 2: The Architect maps the system

Files: the new `SKILL.md`; `kit.yaml`; `system.md`; `role.yaml`; `kit.rs` (`embedded_skills`, the tests); `lib.rs` (the tests).

- `the_system_skill_names_its_three_documents` (`kit.rs`, beside `the_lanes_skill_names_the_plan_and_its_limits`, `:2476`): `mapping-the-system` holds `docs/catervas/architecture/system.md`, `interfaces.md`, `deployment.md`, `## Repositories`, a fenced `mermaid`, `catervas_write_folder_doc` and `catervas_git_commit`. It is under 6 KiB with no ` @`. RED.
- `architect_kit_carries_its_skills` (`:2451`) lists eight, `mapping-the-system` before `catervafying-the-repository`. `every_owner_carries_catervafying_the_repository` (`:2300`) pins the Architect at 8. RED: seven.
- `the_architects_prompt_names_the_system_documents` (`lib.rs`, beside `planning_follows_the_architects_plan`, `:1638`): the Architect's prompt holds `system.md`, `interfaces.md`, `deployment.md` and `repos/<name>/architecture/`. RED.

- [ ] `feat(roles): the Architect maps how the repositories connect`

### Task 3: Plans span repositories

Files: the two skills; `messages.rs` (`plan_tasks_message`); `readiness.rs`; `readiness/fixtures.rs`; `transitions.rs`.

- `lanes_in_different_repositories_never_meet` (`readiness.rs`): an epic's tasks in lanes 1 and 2 both with `allowed_paths: [src/**]`. With `repo` `app` and `infra` they pass `LanesOverlap`; both in `app` fail it as today; both `None` fail it as today. RED.
- `an_epic_bounds_only_its_tasks_in_the_project_folder` (`readiness.rs`): an epic with `allowed_paths: [docs/catervas/**]`, a child with `repo: app` and `[src/**]` passes `PathsWithinParent`; a child with no `repo` and `[src/**]` fails it as today. RED.
- `reads_each_siblings_repository` (`transitions.rs`, beside `reads_the_lanes_of_an_epics_other_tasks`, `:2897`): a sibling with `repo: infra` and lane 2 gives a `LanedTask` with `repo: Some("infra")`. RED.
- `the_plans_name_each_tasks_repository` (`kit.rs` and `messages.rs`): `planning-in-lanes` holds `## 2b. Repositories`, `repo` and `depends on`. `writing-task-contracts` §7 holds "its `repo`". `plan_tasks_message` lists `repo` among the fields. RED.

- [ ] `feat(core): plan an epic across repositories`

### Task 4: The sprint review refreshes the system

Files: `messages.rs` (`architecture_refresh_message`, `SprintTask`); `rules.rs` (`due_refresh`).

- `writes_the_project_folders_refresh_message` (`messages.rs`, beside `writes_the_refresh_messages`, `:2150`): with members `[app, infra]` and tasks `CTV-3` (`repo: app`) and `CTV-4` (no `repo`), the message opens with the Decisions' sentence for sprint 1 and lists `CTV-3 … (in app)` and `CTV-4 … (in the project's own folder)`. With `members: []` it is today's message exactly. RED.
- `refreshes_a_project_folders_architecture` (`rules.rs`, as `refreshes_the_product_and_the_architecture_after_the_review`, `:8474`, on `Harness::project_folder` with `system.md` and no `structure.md` on `main`): after a sprint that integrated a task in `app`, the Architect's refresh session runs with the project folder's message. With `system.md` absent it does not run, and a sprint that integrated nothing does not run it. RED: `structure.md` is not there, so it is never owed.

- [ ] `feat(runtime): refresh the system's documents at the sprint review`

### Task 5: Spec

`docs/SPEC.md`, at the next revision:
- 5.17 and 6.3: the three documents, their shapes, `mapping-the-system`, the purposes
- 5.16 and 6.1: plans naming each task's `repo`, provider before consumer
- 5.3: `lanes_overlap` and `paths_within_parent` within a repository
- 5.17: the sprint review's refresh in a project folder
- 8.2: the purposes in the repositories line

- [ ] `docs(spec): record the system map and plans across repositories`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
