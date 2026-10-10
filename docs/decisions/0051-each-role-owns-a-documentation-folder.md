# 0051. Each role owns a documentation folder, and a team documents its project first

Date: 2026-10-09
Status: proposed (the founder's decisions of 2026-10-09 in conversation, recorded in `docs/design/catervas-folders.md`; accepted when the founder approves that design). Amends ADR 0042 (the Marketing Specialist's documents move from `docs/marketing/` to `docs/catervas/marketing/`, and its sessions read only the Product Manager's folder and its own), ADR 0028 (the Scrum Master schedules an epic's tasks and no longer breaks the epic down), and ADR 0049 and every ADR that names a phase from 9 on by its number (each moves down by one).

## Context

Agents keep what they know of a project in their memory notebooks (`.catervas/agents/<id>/memory.md`, capped at 8k tokens), the team's decisions and retro, and the Product Manager's `.catervas/product/`, written only for approved epics. Nothing documents the architecture, the conventions or the product as a whole, nothing is written for the owner to read, and an existing repository's team starts building before it has read the project. The founder asked for the Catervas Folder System: each role keeps its own documentation, an existing repository is documented in a first sprint ("Catervafication"), and a new project is documented only after the owner approves its product plan.

The options for where the folders live were:
- **In the repository, committed** (`docs/catervas/<folder>/`): the owner and any other tool read them, git keeps their history, and the later cloud copy is the repository. The founder's choice.
- **Private and gitignored** (`.catervas/local/<folder>/`), as the Finance books: nobody else on the repository sees them, and there is no history.
- **Committed but written only by Catervas's tools** (`.catervas/agents/<id>/docs/`), as memory is: every update becomes a tool call outside review.

The options for how agents write them were docs tasks only (reviewed, slow for small updates), a write tool only (ungoverned), or a hybrid. The founder chose the hybrid.

## Decision

Each role owns `docs/catervas/<folder>/`, committed. Only the owning role writes it; every agent reads it, except the Marketing Specialist, which reads the Product Manager's folder and its own. A document the owner approves is a pair, a version for people and an `.agent.md` version for agents; everything else is written for agents. Big documents are written in reviewed docs tasks, and small recurring ones at ceremonies with `catervas_write_folder_doc`, held to the caller's folder; a human document written that way waits for the owner's approval. The owner reads and edits every document on a Files page, and a save commits at once as the owner's change. An approved feature is planned PM → Architect → PM → Scrum Master, the Architect's plan in lanes no wider than the team's builders. An existing repository's team is offered a Catervafication sprint; a new project's Product Manager interviews the owner and the product plan is approved before anything else is planned. This is phase 9, Catervas folders; the phases from 9 on move down by one (ask or auto and the milestones become 10, and so on to Premium, 18).

## Consequences

- The owner can read what the team knows in plain words, and agents start every task from documents rather than from a fresh read of the code.
- Every document is in git: reviewed when it is big, at once when it is the owner's own edit, and diffable either way.
- Pairs cost two writes for each human document, and a pair can go stale when the owner edits it; the re-derive task and the Definition of Done's `pair_changed_alone` keep that visible and bounded.
- The Catervafication sprint costs a sprint's budget before any feature work; it is recommended, not forced, for an existing repository, and for a new project it replaces planning that would have happened anyway.
- `lanes_overlap` is a conservative check that may refuse two lanes whose paths never meet; the Architect then narrows the paths.
- The Marketing Specialist's read limit is held by the hook on built-in tools; what it reads through its connectors is outside the repository and unchanged.
- The milestone runs move one phase later, and test the folders too.
