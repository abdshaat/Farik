# Multi-repo projects: one team across several repositories

Status: approved by the founder on 2026-10-12, in conversation (ADR 0057). It is the design input to phase 9, Multi-repo projects. Its screens are mocked up and approved before they are built (ADR 0032).

## Why

A business's software is often more than one repository: a product and its infrastructure, or a set of microservices, each its own repository on GitHub. Until now a Catervas project is exactly one repository, so a team could see and change only one of them. The founder wants one team to work across a set of interconnected repositories, bundled as one project in Catervas while each stays a separate repository on GitHub, and wants the Architect to understand how the repositories connect and draw it.

## The founder's decisions (2026-10-12, in conversation)

- A task changes one repository: "One task, one repo". A change that crosses repositories is an epic whose plan orders a task per repository.
- Repositories join from folders already on the computer, at the first run or later in Settings: "Local folders". Cloning from GitHub is not in this phase.
- Every member repository is equal. No repository is the primary: "In microservice architecture, there is really no primary repo."
- A multi-repo project's documentation stays on the computer, not in any repository: "Keep the documentation local rather than in a repo." It lives in a project folder that Catervas makes, which is a local git repository that is never pushed ("Local project folder, git"). All of the project's documents go there, each repository's own included ("All of a multi-repo project's"). A one-repo project keeps `docs/catervas/` committed in its repository, as ADR 0051 says.
- An agent reads every repository in the project and writes only its task's repository: "Every repo, write one".
- One integration policy for the team, and an integration branch per repository: "One policy, per-repo branch".
- The Architect writes all four cross-repository documents: the system map, the interfaces between repositories, the deployment view, and plans that span repositories.
- It is phase 9, right after Catervas folders, which it builds on. Catervas Cloud and every later phase move down by one ("New phase after folders").

## Two kinds of project

- **A one-repo project** is today's project, unchanged: `.catervas/` and `docs/catervas/` are in the repository, and `docs/catervas/` is committed (ADR 0051).
- **A multi-repo project** has a project folder and one or more member repositories. The project folder holds the team and every document. The members hold only their code; Catervas writes nothing into them except a task's branch.

A one-repo project becomes a multi-repo one when the owner adds a second repository (below). A multi-repo project never turns back into a one-repo project: removing members down to one leaves a multi-repo project with one member.

## The project folder

- Made by Catervas when the owner creates a multi-repo project. By default it is `~/Catervas/<slug>/`, where the slug comes from the project's name by the template slug rule (spec 3). The wizard lets the owner choose another folder. Catervas runs `git init` there with no remote and refuses a folder that is inside any member repository, or that holds one.
- It holds `.catervas/` exactly as a repository does today, laid out the same way (team, contracts, sprints, `local/` with the log, worktrees and locks), plus `docs/catervas/`.
- It is committed to its own local git repository, so phase 8's machinery works unchanged: who changed a document last, staleness read from git, folder changes, approvals.
- It has no remote, so a folder change in it is merged into its `main` at once under the integration lock, whatever the team's `integration` policy says. The policy governs the member repositories. The owner's approval of a human document still comes first, as in phase 8.
- **Members.** `.catervas/repos.yaml`, held to `docs/schemas/repos.schema.json`, lists 1 to 16 members. Each has:
  - `name`: 1 to 32 characters, lower-case letters, digits and `-`, starting with a letter, unique in the project
  - `path`: the absolute path of the repository's top folder on this computer
  - `integration_branch`: the branch accepted work goes into, which defaults to the repository's default branch, read as spec 5.14 says
- A member must be a git repository's top folder. It may not lie inside another member or the project folder, and two members may not be the same repository.
- A member whose folder is gone when serve starts, or later, is reported as `repo_missing`. Today shows "Can't find <name>" with "Locate…", which records the new path (`repo.moved`). Tasks in that repository wait until it is found.

## Tasks across repositories

Checks, branches and epics per repository are ADR 0057's amendment of 2026-10-12.

- A task contract gains `repo`, the member's name.
  - In a multi-repo project every task whose work is code names a `repo`: the Developer's and the UI/UX Designer's tasks.
  - A docs task names none and works in the project folder.
  - In a one-repo project `repo` is refused.
- Readiness adds three refusals:
  - `repo_unknown`: the name is not a member.
  - `repo_required`: a builder's task names no repository.
  - `repo_not_allowed`: a one-repo project names one, or a docs task does.
- `allowed_paths` are relative to the task's repository, or to the project folder for a docs task. `folder_owned` applies to the project folder's `docs/catervas/`. `lanes_overlap` compares only tasks in the same repository.
- A task's branch and its git worktree come from its repository. The worktree goes under the project folder's `.catervas/local/worktrees/CTV-<n>`, and the branch keeps phase 3's naming. The Definition of Done reads the diff from that repository.
- Task ids stay one counter for the whole project. The counter moves past every `feature|fix|docs/CTV-<n>` branch in every member and in the project folder (ADR 0053's rule, extended to each repository).
- A task may depend on a task in another repository, as on any task. It is assigned only once that task is integrated into its own repository's integration branch. This is how a provider's change lands before its consumer's.
- **Integration.** The team's one `integration` policy applies to every member, each with its own integration branch:
  - `auto_merge` merges and pushes in that repository.
  - `pull_request` opens a pull request with `gh` in that repository's checkout.
  - `manual` waits for `catervas integrate CTV-<n>`.
  One integration lock, `.catervas/local/integration.lock` in the project folder, still serialises every merge across all repositories.

## Reading across repositories

- Every agent's session may read every member and the project folder. Each member's path is registered as a read path when the session starts, and the session prompt lists the members with their names, paths and a one-line purpose taken from `system.md` once the Architect has written it.
- An agent writes only its task's worktree. The sandbox mounts that worktree read-write and every other member read-only, at `/repos/<name>`.
- The Marketing Specialist is unchanged: it reads only `docs/catervas/product/` and `docs/catervas/marketing/`, now in the project folder, and no member.

## The documents

In a multi-repo project every document lives in the project folder's `docs/catervas/`, which is committed only locally:

| Path in the project folder | Owner | What it holds |
|---|---|---|
| `product/` | Product Manager | `spec.md`, `roadmap.md` and their twins, sprint reports, for the whole project |
| `marketing/` | Marketing Specialist | as in phase 8 |
| `delivery/` | Scrum Master | as in phase 8 |
| `architecture/system.md` | Architect | the system map: a Mermaid diagram of the members and how they connect (calls an API, deploys, depends on a package, shares a database, a queue or a bucket). Each link carries the file it was read from |
| `architecture/interfaces.md` | Architect | each contract between repositories (an HTTP or gRPC API, an event, a package, a shared schema, configuration and environment), with its provider, its consumers and where it is defined, so that a change to one side names the repositories it touches |
| `architecture/deployment.md` | Architect | what runs where, read from infrastructure code (Terraform, Kubernetes, Compose, CI deploy jobs), and which member builds each running piece, with a Mermaid diagram |
| `architecture/plans/<epic-id>.md` | Architect | an epic's plan; its tasks may span repositories |
| `repos/<name>/architecture/` | Architect | that member's `overview.md`, `features.md`, `jobs.md`, `structure.md` |
| `repos/<name>/engineering/` | Software Developer | that member's `conventions.md` and `workflow.md`, since each repository may have its own stack and habits |
| `repos/<name>/design/` | UI/UX Designer | the UI inventory of a member that has a UI |

- Ownership is phase 8's, by role. The `repos/<name>/` folders belong to the same roles as their top-level counterparts.
- The human documents are unchanged: `product/spec.md`, `product/roadmap.md`, and the marketing plans.
- Removing a member removes its `repos/<name>/` folder in one folder change. Its history stays in the project folder's git.

## Planning a feature across repositories

Phase 8's PM → Architect → PM → Scrum Master flow, with these changes:

- The Architect's plan gives each task a repository and orders them by the interfaces they change: the provider's task first, its consumers depending on it. When a change has to reach two repositories at once, the plan says how it stays compatible in between (a new field added before the old one is removed).
- Lanes stay at most as many as the team's active builders, and a lane may cross repositories.
- The Product Manager writes each contract with its `repo` from the plan.
- With no Architect, the Product Manager names the repositories itself.

## Catervafication across repositories

- **A new multi-repo project.** Every member is an existing repository, so the team page offers the Catervafication sprint (phase 8). Its epic holds:
  - for each member, the Architect's `repos/<name>/architecture/` task and the Developer's `repos/<name>/engineering/` task, plus the Designer's inventory for a member with a UI
  - the Product Manager's product plan for the whole project
  - the Architect's `system.md`, `interfaces.md` and `deployment.md` task, which depends on the per-member architecture tasks and reads them
- **A member added later** gets its own small Catervafication (its per-member tasks), then the Architect's task to update the three system documents (`catervafication.started` with the member's name).
- **At each sprint review** the Architect brings the system documents up to date in the same session as phase 8's refresh, when a task integrated that sprint changed an interface the documents name.

## A one-repo project gains a second repository

"Add repository" in Settings, on a one-repo project, first explains what changes, then:

1. Makes the project folder.
2. Copies the repository's `.catervas/` and its `docs/catervas/` into it: the project-level folders to the top, and `architecture/`, `engineering/` and `design/` to `repos/<name>/`, where the repository's name is its folder's name, which the owner may change.
3. Records both members.
4. Removes `.catervas/` and `docs/catervas/` from that repository in one commit, integrated by the team's policy, since the documents now live locally.

Tasks that are not done keep their ids and gain the first member's `repo`. Serve restarts the drive on the project folder, as ADR 0053's switch does, and the new member's Catervafication is offered.

## Screens (mocked up first)

- **First run.** On the project page, "One repository" or "Several repositories". "Several" names the project, picks two or more folders, names each member, and shows where the project folder will be, with "Change".
- **Settings › Repositories.** Each member with its name, path and integration branch. "Add repository", "Remove" (refused `repo_in_use` while a task of it is not done or integrated), and "Locate…" for a missing member.
- **Board and Today.** Each task card carries its repository's name as a tag.
- **Files.** A picker above the tree: "Project" (the top-level folders) and each member (its `repos/<name>/` folders). Mermaid renders as in phase 8.
- **Header.** The project's name in place of the repository's path, with the members on hover.

## Events

- `repo.added { name, path, integration_branch }`
- `repo.removed { name }`
- `repo.moved { name, path }`
- `project.converted { project_folder, first_repo }`

`catervafication.started` gains an optional `repo`.

## Phase 9 steps

| Step | Name | Delivers |
|---|---|---|
| 01 | The project folder and its members | `RepoName` and the members' rules in `catervas-core`; `repos.yaml` and its schema; creating a project folder (`git init`, no remote); serve and `catervas init` opening a project folder; `repo.added`, `repo.removed`, `repo.moved`, `repo_missing`; ADR 0053's switch accepting a project folder |
| 02 | Tasks in a member repository | the contract's `repo`, fixed once set (`repo_fixed`); `repo_unknown`, `repo_required` and `repo_not_allowed`; worktrees and branches from the task's repository; one id counter past every repository's task branches; the Definition of Done's diff, the git tools, `diff_of` and `task.diff` from the member |
| 02b | Checks and scans per member | the library's entries with `repo`; each member's scan, its read-back and its entries; `criterion_of_another_repo`; `epic_command_in_project_folder`; verification and `check_new_tests` in the member; the preview |
| 02c | Integration per member | merge, push, fetch and pull requests in the member, under one lock; the member's integration branch; its rescan; its worktrees removed; dependencies across repositories; `repo_in_use` |
| 03 | Reading across repositories | read paths for every member; the sandbox's read-only mounts at `/repos/<name>`; the members in the session prompt; the Marketing Specialist's limit in the project folder |
| 04 | Documents in the project folder | the role folders and `repos/<name>/` in the project folder; folder changes merged locally there; `folder_owned` under `repos/`; the Files page's server grouping by project and member |
| 05 | The Architect's system documents and cross-repository plans | `system.md`, `interfaces.md` and `deployment.md` in the Architect's kit skills; plans whose tasks name repositories, ordered provider before consumer; `lanes_overlap` per repository; the refresh at the sprint review |
| 06 | Catervafication across repositories | the epic per member and the system task that depends on them; a later member's own Catervafication |
| 07 | Several repositories in the web app | the first run's choice, Settings › Repositories, the repository tag on the board and Today, the Files page's picker, the header; mocked up first |
| 07b | A one-repo project gains a second repository | the conversion, `project.converted`, the removal commit, and its dialog; mocked up first |
| 08 | The founder's live check | in the web app, a product repository and an infrastructure repository as one project: Catervafied, the system map read, and one feature carried across both, recorded in `docs/milestones/multi-repo.md` |

## Out of scope

- Cloning a repository from GitHub (after Catervas Cloud's GitHub sign-in).
- A task that changes two repositories at once.
- Pushing the project folder anywhere, and sharing it between computers.
- A repository in two projects at once.
- Monorepo sub-projects.
- Integration policies per repository.
- The cloud copy of a project folder (Premium).
- Business workspaces without repositories, which are phase 16's.
