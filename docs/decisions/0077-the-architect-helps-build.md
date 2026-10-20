# 0077. The Architect helps build

Date: 2026-10-19
Status: accepted (the founder, 2026-10-19, quoted below; the mechanism decided by the controller the same day)

The founder: "Make the architect able to write code and contribute to the team to do list. When I added an extra dev he did not start picking up tasks and just sat there."

Amends:
- **SPEC 6.3** (the Architect does not write application code, changed in 0.10) and **ADR 0061**'s reading that an Architect's task is a document: its own tasks still are; a Software Developer's task it is given is code.
- **The assignment gate** (`check_assignment`, SPEC 5.2): an Architect may be the assignee of a Software Developer's task, reviewed by another Architect or a Software Developer.

## Context

A team of a Product Manager, an Architect, two Developers and a Designer ran an epic whose eight Developer tasks the Architect had planned in one lane, each depending on the one before. The first Developer held the lane; the second, added after the plan, had no task it could take: rule 8 offers a lane's task to its builder alone (ADR 0051), and none of the chain's tasks could start before the one before it was accepted anyway. Meanwhile the Architect, which holds the same tiers as a Developer (`read`, `write_workspace`, `execute`, `network`, `git_local`), was idle between reviews, because a task goes only to an agent of the contract's `assignee_role`.

## Decision

**The Architect may build a Software Developer's task, after the Developers.** Rule 8 offers a ready task to the active agents of its role with room, as before. When it finds none and the role is `software_developer`, it offers the active Architects with room, under the same lane rule, and tells the assigner that no Developer has room and who may review. Developers always come first: an Architect is never offered a task a Developer could take.

**The task stays the Developer's.** Its branch, criteria, review, readiness and lane are read from the contract's `assignee_role`, which does not change. `changes_code` stays a property of the role, so the Architect's own documents keep their `docs/` branches, `document_paths` and lack of review.

**The session works as a Developer.** An Architect's `implement` session on a Developer's task loads the Software Developer's instructions and kit skills, and holds `git_remote` when the team's `push` is on, as a Developer's would; its chats, reviews and own tasks stay the Architect's.

**Someone else reviews it.** The contract's reviewer role is usually the Architect, which cannot review its own work. For a task an Architect builds, the gate accepts another Architect or a Software Developer as the reviewer (`may_review`).

**Hand-overs stay within the role.** A task whose wait cleared is handed only to an agent of the contract's role (`check_reassignment`, unchanged).

## Alternatives rejected

- **Make `changes_code` true for the Architect.** Every Architect document (plans, ADRs, Catervafication) would become a code task: agent review, code branches, a member repository required, and the Architect counted as a lane builder. The capability is per task, not per role.
- **Offer the Architect alongside the Developers.** The assigner might pick the Architect over a free Developer, and the reviews it owes would wait behind its building.
- **Let any free Developer take a lane's next task.** It would not have helped the case above, a chain of dependencies, and it gives up the lane's guarantee that two builders never change the same paths at once.

## Consequences

- An Architect with no review owed takes up Developer work when every Developer is full, so a small team finishes more in parallel.
- A Developer added after an epic was planned still gets no task from that epic's lanes until the plan is redone with a lane for it, and a chain of dependencies still runs one task at a time. Re-planning an epic when a builder joins is not part of this decision.
- Reviews owed by the Architect wait for its building session to end, since an agent runs one work session at a time (ADR 0064); its reviews still come before new work in the order of the rules.
