# The Catervas Folder System: each role's own documentation

Status: approved by the founder on 2026-10-09, in conversation (ADR 0051); its mockups, the canvas page "Catervas folders" (version 19, sources in `docs/design/mockups/`), approved by the founder on 2026-10-10 ("Approve as drawn"). It is the design input to phase 8, Catervas folders.

## Why

The founder wants every agent to keep its knowledge of the project as documents it owns, not only as memory notes: the Product Manager the product's spec and roadmap, the Architect the architecture with its diagrams and the implementation plans, the Developer the repository's conventions and workflow, the Marketing Specialist its marketing plan built from the product, and the DevOps Engineer its incident response plan and the project's logging conventions. A team that starts on an existing repository documents it first, in a sprint of its own, "Catervafication"; a team that starts from nothing documents nothing until the owner has approved the product plan. The owner reads every document in the web app and may change any of them.

## The founder's decisions (2026-10-09, in conversation)

- Each role's folder is in the repository, committed, under `docs/catervas/<folder>/`: "In the repo, committed". Stored on the user's computer; paying customers' cloud copies are Premium's.
- Each agent owns its folder with read and write; other agents may read it; no agent writes a folder it does not own. Agents may change any document in a folder they own, human documents included. The Marketing Specialist reads the Product Manager's folder and its own, and no other.
- A document the owner approves has two versions: one for people, "humanized and optimized for a human to read", clear and concise; and one for agents, "fully optimized for agent's use". Internal documents (workflows, lifecycle, plans, internal planning) are written for agents only.
- An approved feature is planned PM → Architect → PM → Scrum Master: the Product Manager specifies it and writes its task contracts, the Architect writes its implementation plan with parallel lanes sized to the team, the Scrum Master schedules it.
- Writing is a hybrid: big documents through reviewed docs tasks, small recurring updates through one tool held to the caller's own folder.
- A new project starts with the Product Manager's interview in the one-to-one chat and an approval screen for the product plan; Catervafication follows the approval.
- The owner edits in the web app; Save commits at once as the owner's own change, and the folder's agent re-derives its own version when the owner edits a human version.
- Catervas folders is its own phase, built first: "Replace phase 8 with 9. Build Catervas folder system first before cloud." It is phase 8, Catervas Cloud phase 9, and the phases after it move down by one, so the milestone runs (phase 10) exercise it.

## The folders

| Folder | Owner | What it holds |
|---|---|---|
| `docs/catervas/product/` | Product Manager | `spec.md` and `roadmap.md` (human, with agent twins), sprint reports |
| `docs/catervas/architecture/` | Architect | `overview.md` with Mermaid diagrams, `features.md`, `jobs.md` (cron and scheduled work found in code, CI and config), `structure.md`, `plans/<epic-id>.md` |
| `docs/catervas/engineering/` | Software Developer | `conventions.md`, `workflow.md`, each with the evidence it was read from |
| `docs/catervas/design/` | UI/UX Designer | the UI inventory, design notes |
| `docs/catervas/delivery/` | Scrum Master | the sprint cadence, ceremony notes |
| `docs/catervas/marketing/` | Marketing Specialist | today's `docs/marketing/` moved here: brand kit, persona, plans, research |
| `docs/catervas/operations/` | DevOps Engineer | incident response plan, logging conventions, incident write-ups; arrives with the role (phase 12 after this ADR) |

- A folder belongs to a role, not to one agent: two Developers both own `engineering/`. A folder appears with its first document (git keeps no empty folder); the Files page lists every active role's folder, an empty one as "Nothing here yet". The Finance and Procurement Specialists keep their private, gitignored folders (`.catervas/local/<role>/`); their books are not documentation.
- `.catervas/product/` and `catervas_write_product_doc` go: the Product Manager's documents are `docs/catervas/product/`. `docs/marketing/` moves to `docs/catervas/marketing/` and every rule naming it follows. No migration: no Catervas install predates this (ADR 0050).

## Ownership and reading

- **Writing.** Readiness generalizes today's `marketing_paths_owned` to `folder_owned`: while a role has an active agent, another role's task may not name a path that could reach its folder in `allowed_paths`. Ownership is held at readiness: the Definition of Done holds the diff to `allowed_paths`, which `folder_owned` checked when the task became ready; an owner made active after that does not reopen it. The write tool refuses any path outside the caller's folder, a path through a link, and anything but `.md`.
- **Reading.** Every agent reads every folder, except the Marketing Specialist: its sessions are registered with read paths `docs/catervas/product/**` and `docs/catervas/marketing/**`, fixed when the session starts, and the hook refuses a `Read`, `Grep` or `Glob` outside them (`read_not_allowed`), one without a path included. `catervas_exec` and the diff of a task that is not its own are refused too. Catervas's own read tools are otherwise unchanged; the residual is a `catervas_git_diff` of the agent's own branch, which holds only its own work.

## Two versions

- A human document is a pair: `roadmap.md` for people and `roadmap.agent.md` for agents, side by side. Every other document is one file, written for agents.
- Which documents are human documents is a fixed list in `catervas-core`, the ones the owner approves: `product/spec.md`, `product/roadmap.md`, and `marketing/plans/MP-<n>.md` (whose proposal tool gains the agent version). The DevOps Engineer's phase adds its own if it puts one to the owner.
- Agents read the `.agent.md`; the session prompt says so, and says when it is stale.
- **Stale** is read from git, with nothing stored: an agent twin is stale when the last commit that changed its human file is not an ancestor of (or equal to) the last commit that changed the twin.
- The Definition of Done refuses a diff that changes one file of a pair without the other (`pair_changed_alone`), except a diff that changes only a stale twin, which is the re-derive.
- Owners write both versions in the same task or tool call, so the pair never goes stale through its owner.

## How documents are written

- **Docs tasks** carry the big documents: the Catervafication documents, implementation plans, the marketing plan, the incident response plan, a rewrite of the spec or the architecture. They are ordinary contracted tasks of the owning role on `docs/CTV-<n>` branches, reviewed and integrated as today (5.14), held to the owner's folder. A task whose diff changes a human document is accepted by the owner, not by an agent: Today shows it as a document to approve, the human version's diff first, with "Approve" and "Send back" with notes.
- **`catervas_write_folder_doc { path, text, agent_text? }`** carries the small recurring updates: the roadmap's and the spec's refresh, the sprint report, the architecture's `features.md`, `jobs.md` and `structure.md` for the sprint's integrated changes, incident write-ups (DevOps phase). `agent_text` is required for a human document and refused for any other. It is offered only in ceremony sessions (sprint planning, sprint review, retro) and refuses outside its caller's folder.
  - An agent-only document is committed at once on the default branch under the integration lock, authored by Catervas for the agent, and recorded as `folder_doc.written`.
  - A human document is proposed, not written: `folder_doc.proposed { path, text, agent_text }`. Today shows "Product plan changes to approve" (one card per sprint review, all its proposals together) with the change as a diff of the human text, "Approve" and "Send back" with notes. `folder_doc.approved` commits both files; `folder_doc.returned` gives the owner's words to the agent's next session as the human's, as a returned marketing plan's are.
- **The owner's edits** from the Files page commit at once on the default branch, authored with the owner's git identity, under the integration lock, recorded as `folder_doc.edited`. An edit of a human document makes its twin stale; Catervas files a re-derive task for the owning role ("Bring roadmap.agent.md up to date with the owner's change"), and with sprints on it is planned first in the next sprint.

## The Files page

A new page in the rail, "Files", mocked up and approved before it is built.

- **Browse.** A tree of the team's folders, each with its owner's avatar and tag colour; a pair shows as one entry. On a phone the tree is a picker above the document.
- **View.** Markdown rendered with `markdown-it` (raw HTML off), Mermaid diagrams rendered with `mermaid`, loaded only on a page that has one, `securityLevel: "strict"`. A pair opens on the human version with a "For you / For the team" switch (the team's version is the `.agent.md`). A line under the title says who changed it last, when, and in which task, from `git log`. A stale twin says "Being brought up to date by <agent>"; a human document with a proposal waiting says "A change waits for your approval" and links to Today.
- **Edit.** "Edit" turns the page into a plain Markdown text area with a "Preview" tab, "Save" and "Cancel". An agent twin is read-only; its human version is what the owner edits. No new files, renames or deletes in this phase.
- **Save.** The server takes `{ path, text, base }`, `base` being the blob id the page opened. It refuses a path outside `docs/catervas/`, a `..` segment, a link, a file that is not `.md`, an agent twin, text over 256 KiB, and a `base` that no longer matches (`file_changed`: "This file changed while you were editing", the owner's text kept in the box).

## Planning a feature: PM → Architect → PM → Scrum Master

1. The Product Manager triages a request into an epic, which the owner approves as today (5.16).
2. The Architect writes `architecture/plans/<epic-id>.md`, for agents: the tasks, their dependencies and their lanes. A lane is a sequence of tasks one builder does in order; there are at most as many lanes as the team's active builders (Developers and UI/UX Designers not paused), one lane with one builder.
3. The Product Manager writes a task contract for each planned task, with the acceptance criteria from the spec and the Architect's constraints. A contract gains two optional fields, `plan` (the plan's path) and `lane` (1 to 16).
4. Readiness refuses two tasks of the same epic in different lanes whose `allowed_paths` could overlap (`lanes_overlap`), so parallel lanes cannot collide. Two globs could overlap unless their literal prefixes, up to the first wildcard, part at a path segment. (`ponytail:` a conservative test that may refuse paths that never meet; exact glob intersection if it refuses real plans.)
5. The Scrum Master schedules the contracts into the sprint, one lane to one builder.

With no Architect the Product Manager writes the plan; with no Scrum Master the Product Manager schedules, as today. The Scrum Master no longer breaks epics into tasks.

## Starting: Catervafication

**An existing repository.** After the first run's scan, the team page recommends "Start with a Catervafication sprint: your team documents this project before it builds anything", with "Start" and "Later"; "Later" leaves the card on Today and a button on the Files page. "Start" files one epic, "Catervafy the repository", approved by that click (`catervafication.started`), with one docs task per role on the team, each from its role's kit skill `catervafying-the-repository`:

- Product Manager: `spec.md` and `roadmap.md` read from the code, the README and the existing docs, ending at the owner's approval as the product plan below.
- Architect: `overview.md` with diagrams, `features.md`, `jobs.md`, `structure.md`.
- Developer: `conventions.md` and `workflow.md`, each rule with where it was read (lint and format configs, test layout, the last fifty commits' messages and branch names, CI, a contributing guide). With no documented process, no CI that runs tests and no consistent commit style, it adopts Catervas's recommended workflow, which its kit ships (brainstorm, plan, test first, verify, review, Conventional Commits), and says so.
- UI/UX Designer: the UI inventory. Scrum Master: the cadence.
- Marketing Specialist: its marketing plan, after the product plan is approved, since it reads only `product/`.

The Architect's, the Developer's and the Designer's tasks run in parallel.

**A new project.** The owner writes the paragraph and Catervas runs `git init`, as today, and records the project as new. The Product Manager interviews the owner in their one-to-one chat (who the customer is, the problem, what the product must do, what is out, constraints, how success is measured), in as many rounds as it needs, then summarizes; the chat stays read-only (4.3). A "Draft the product plan" button in that chat files the product plan task, which writes `spec.md`, `roadmap.md` and their twins; since it changes human documents, the owner accepts it ("Product plan to approve" on Today). The plan is approved when `product/spec.md` and `product/roadmap.md` are on the default branch, read from git with nothing stored. Until then, readiness refuses every task of a new project but the product plan's own (`product_plan_first`). On approval Catervas files the Catervafication epic, lighter: the Architect's first architecture from the spec (the first feature, the owner's paragraph, is planned as any epic, PM → Architect → PM → Scrum Master), the Developer's workflow and the stack's conventions, the Marketing Specialist's plan, the Scrum Master's cadence. Building starts in the sprint after.

## Each sprint

At sprint review the Product Manager proposes the spec's and the roadmap's changes and writes its sprint report, and the Architect brings `features.md`, `jobs.md` and `structure.md` up to date with the sprint's integrated changes: one session per sprint, not per integration, for cost. At the retro each role may update its own agent-only notes.

## Events

`folder_doc.written`, `folder_doc.proposed`, `folder_doc.approved`, `folder_doc.returned`, `folder_doc.edited`, `catervafication.started`, `project.started`. A new project is told from an existing one by a new event, `project.started`, recorded when the first run's `project.create` makes the folder and runs `git init` (`project.scanned` stays the existing repository's).

## Phase 8 steps

| Step | Name | Delivers |
|---|---|---|
| 01 | Role folders and ownership | `role_folder`, the human-document list and pairs in `catervas-core`; `folder_owned` and `pair_changed_alone`; `.catervas/product/` and `docs/marketing/` moved; prompts and skills updated |
| 01b | The Product Manager writes its folder | no write outside a task's implement session, then the Product Manager's `write_workspace` and `git_local` and its docs tasks |
| 02 | Marketing's read limit | read paths per session, `read_not_allowed` in the hook |
| 03 | The folder write tool and approving human documents | `catervas_write_folder_doc`, the `folder_doc.*` events, Today's approval card, the re-derive task; mocked up first |
| 04 | The Files page | browse, view with Mermaid, edit and save as the owner's commit; staleness, the re-derive task and `pair_changed_alone`'s re-derive exception; mocked up first |
| 05 | Plans with lanes | contract `plan` and `lane`, `lanes_overlap`, the Architect's planning, the Product Manager's contracts from the plan, the Scrum Master's scheduling by lane |
| 06 | A new project's interview and product plan | the chat's button, the owner accepting a task that changes a human document, `product_plan_first`; mocked up first |
| 07 | Catervafication | the recommendation, the epic and its tasks, every role's `catervafying-the-repository`, Catervas's recommended workflow, the sprint review's refreshes |
| 08 | The founder's live check | in the web app, an existing repository Catervafied and a new project from interview to approved plan to its Catervafication, recorded in `docs/milestones/catervas-folders.md` |

The DevOps Engineer's `operations/` folder, its `catervafying-the-repository` and its incident response plan join its role's steps (phase 12 after this ADR).

## Out of scope

Creating, renaming and deleting files in the Files page; a rich-text editor; the cloud copy of the folders (Premium); folders for the Finance and Procurement Specialists in `docs/catervas/`; a history view of a document beyond its last change.
