# 0040. Farik for every business, software or not

Date: 2026-10-05
Status: accepted. The founder set the direction on 2026-10-05 ("I am looking to generalize farik for non technical products as well. For example car flipping business, selling items on amazon, car rentals, etc.") and, the same day, decided it ("Restructure farik around being for all businesses both software and non software"), answering G1 to G6 below. G6 has its own record, ADR 0041. Amends ADRs 0017, 0021, 0025 and 0035 in one respect only: the phases after the web launch are renumbered, Desktop from 12 to 13, Native mobile from 13 to 14, Premium from 14 to 15, to make room for this phase.

## Context

Spec section 1 listed "non-software teams" as a non-goal: "The role set and the harness are built around shipping software." The founder wants Farik for every business: one that builds software, and one that flips cars, resells goods on Amazon, or rents cars.

What in Farik assumes software:
- **A git repository is the project** (spec 1, goal 2; 5.14). Every task has a worktree, a branch and an integration.
- **The team must have a Software Developer** (D18, spec 1, goal 1).
- **Exit criteria are mostly code's** (5.13): a command passes, a test passes, a file is in the diff.
- **The roles' prompts** speak of a product team shipping software.

What does not, and is the seed of the change:
- **The Finance Specialist and the Procurement Specialist already work in a private folder with no branch and nothing to integrate** (spec 6.6, 6.10): their tasks end at `accepted`, their criteria are `artifact`, `review` and `human`, and their outputs are workbooks and documents. That is how most work in any business runs.
- **The governance is general**: contracts with exit criteria, nobody accepting their own work, budgets, the human's gates, the audit log, approval of anything that reaches outside.
- **The Procurement Specialist researches any product** and contacts any seller (ADR 0039).

## Decision

Farik is for every business, software or not. The founder's answers:

- **G1, when.** After the web launch. The first launch (phase 11) ships for software as planned, with the Procurement Specialist; a new phase 12, Business workspaces, follows it, and the phases after move up by one: 13 Desktop, 14 Native mobile, 15 Premium.
- **G2, the project.** A project is a business workspace: a folder of its own, chosen by the user, holding `.farik/` and the business's documents, kept on the user's computer. Paying customers get cloud hosting of their workspaces in the last phase, Premium (15). A workspace may have a git repository: a software business's workspace is its repository folder, as every project is today, so today's projects become workspaces without moving.
- **G3, the design.** Farik is designed to suit any business. Work that is not code runs in the workspace folder, as finance and procurement tasks run in theirs: no branch, no commit, versions kept, `accepted` as the end. The Procurement Specialist is the first role built for it: it finds suppliers for any product, compares prices, and contacts suppliers.
- **G4, the roles.** Only the Procurement Specialist is added now (ADR 0039). The other roles that serve any business (Product Manager, Scrum Master, Marketing Specialist, Finance Specialist) have their words and skills made business-neutral in phase 12; no other new role is planned.
- **G5, the team.** A team can run without a Software Developer, but it cannot build software without one: a task that changes code needs a Developer or a UI/UX Designer on the team, and is refused readiness otherwise. D18 becomes "at least one Product Manager".
- **G6, approvals.** The user chooses whether to approve every outward act or run on auto (ADR 0041), a phase 7 step, since it changes every role's connectors.

## Consequences

Easier:
- Farik's governance reaches anyone who runs a small business, which is exactly spec 2's first persona.
- The private-folder model, built for two roles in phase 7, becomes the default for every non-code task, so phase 12 generalises rather than invents.
- Software teams lose nothing: their workspace is their repository.

Harder:
- Spec 1's non-goal, goal 2, D18, and section 2's personas change; the project plan gains a phase and renumbers three.
- Every role's prompt and skills must read well for a business that is not software; phase 12 rewrites them and checks them with non-software test businesses.
- Phase 10's benchmark measures software tasks; a business team is launched in phase 12 measured only by its own recorded check, not the benchmark, unless the founder adds business tasks to the benchmark.
- Business connectors (marketplaces, listings, bookings, payments) bring more sending and spending tools, each asking or limited by ADR 0041's mode.
