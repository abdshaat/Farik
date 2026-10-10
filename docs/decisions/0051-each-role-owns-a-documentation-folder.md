# 0051. Each role owns a documentation folder, and a team documents its project first

Date: 2026-10-09
Status: accepted (the founder's decisions of 2026-10-09 in conversation, recorded in `docs/design/catervas-folders.md`, which the founder approved the same day with "Replace phase 8 with 9. Build Catervas folder system first before cloud."). Amends ADR 0042 (the Marketing Specialist's documents move from `docs/marketing/` to `docs/catervas/marketing/`, and its sessions read only the Product Manager's folder and its own), ADR 0028 (the Scrum Master schedules an epic's tasks and no longer breaks the epic down), ADR 0048 (Catervas Cloud is phase 9, after this phase: the founder, 2026-10-09, "Replace phase 8 with 9. Build Catervas folder system first before cloud."), and ADR 0049 and every ADR that names a phase from 8 on by its number (each moves down by one). Merged step plans and earlier design documents keep the numbers they were written with; read a phase number from 8 on in a document dated before 2026-10-10 one higher.

## Context

Agents keep what they know of a project in their memory notebooks (`.catervas/agents/<id>/memory.md`, capped at 8k tokens), the team's decisions and retro, and the Product Manager's `.catervas/product/`, written only for approved epics. Nothing documents the architecture, the conventions or the product as a whole, nothing is written for the owner to read, and an existing repository's team starts building before it has read the project. The founder asked for the Catervas Folder System: each role keeps its own documentation, an existing repository is documented in a first sprint ("Catervafication"), and a new project is documented only after the owner approves its product plan.

The options for where the folders live were:
- **In the repository, committed** (`docs/catervas/<folder>/`): the owner and any other tool read them, git keeps their history, and the later cloud copy is the repository. The founder's choice.
- **Private and gitignored** (`.catervas/local/<folder>/`), as the Finance books: nobody else on the repository sees them, and there is no history.
- **Committed but written only by Catervas's tools** (`.catervas/agents/<id>/docs/`), as memory is: every update becomes a tool call outside review.

The options for how agents write them were docs tasks only (reviewed, slow for small updates), a write tool only (ungoverned), or a hybrid. The founder chose the hybrid.

## Decision

Each role owns `docs/catervas/<folder>/`, committed. Only the owning role writes it; every agent reads it, except the Marketing Specialist, which reads the Product Manager's folder and its own. A document the owner approves is a pair, a version for people and an `.agent.md` version for agents; everything else is written for agents. Big documents are written in reviewed docs tasks, and small recurring ones at ceremonies with `catervas_write_folder_doc`, held to the caller's folder; a human document written that way waits for the owner's approval. The owner reads and edits every document on a Files page, and a save commits at once as the owner's change. An approved feature is planned PM → Architect → PM → Scrum Master, the Architect's plan in lanes no wider than the team's builders. An existing repository's team is offered a Catervafication sprint; a new project's Product Manager interviews the owner and the product plan is approved before anything else is planned. So that the Product Manager can work docs tasks in its own folder, every session that is not about a task is read-only for every role (`no_task_no_write`), and the Product Manager's tiers gain `write_workspace` and `git_local`, as the Architect's and the Marketing Specialist's did (step 01b; added 2026-10-10, when step 01's plan found the Product Manager, which wrote only through `catervas_write_product_doc`, could not commit). Every commit of a folder document, an agent's write at a ceremony, an approved human document and the owner's own edit alike, is integrated under the team's `integration` policy as an accepted task's branch is (spec 5.14): merged and pushed under `auto_merge`, a pull request under `pull_request`, waiting for `catervas integrate` under `manual`; a change escalated or not yet integrated holds its document only while it waits (steps 03 and 04, 2026-10-10, refining the design's "committed at once on the default branch"). A proposal carries a `summary` for the owner, and the spec and the roadmap are proposed only at the sprint review. This is phase 8, Catervas folders, built before Catervas Cloud; the phases from 8 on move down by one: 9 Catervas Cloud, 10 Ask or auto and the milestones, 11 Engines and providers, 12 Ecosystem, 13 Proof of concept, 14 Web launch, 15 Business workspaces, 16 Desktop, 17 Native mobile, 18 Premium.

## Consequences

- The owner can read what the team knows in plain words, and agents start every task from documents rather than from a fresh read of the code.
- Every document is in git: reviewed when it is big, at once when it is the owner's own edit, and diffable either way.
- Pairs cost two writes for each human document, and a pair can go stale when the owner edits it; the re-derive task and the Definition of Done's `pair_changed_alone` keep that visible and bounded.
- The Catervafication sprint costs a sprint's budget before any feature work; it is recommended, not forced, for an existing repository, and for a new project it replaces planning that would have happened anyway.
- `lanes_overlap` is a conservative check that may refuse two lanes whose paths never meet; the Architect then narrows the paths.
- The Marketing Specialist's read limit is held by the hook on built-in tools; what it reads through its connectors is outside the repository and unchanged.
- Catervas Cloud, and the founder's live checks that wait on it, start one phase later; nothing in this phase depends on it.
- The milestone runs move one phase later, and test the folders too.
