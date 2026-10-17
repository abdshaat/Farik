# Standalone: no sprints, clear escalations, stuck agents move on (CTV-9)

Branch: `feat/no-sprints` (work outside a phase; its own pull request to `main`).
Source: the epic CTV-9 the founder approved (requirements R1 to R6, exit criteria C1 to C8) and the
Architect's accepted plan for it (task CTV-12, "Plan for CTV-9"), adopted by the controller with
the corrections each plan's Decisions section names. The decision is ADR 0062,
`docs/decisions/0062-deliveries-replace-sprints.md`, which supersedes ADR 0028.

Rebase first: this branch is rebased onto `fix/plans-need-no-review` (ADR 0061 and its eight plans,
SPEC 0.115) after that branch's plans are executed and before the first task here runs. These plans
assume what ADR 0061 leaves: only code is reviewed by an agent (other tasks are accepted by the
governor on criteria it runs, so the changelog task carries only such criteria); the human is asked
only before payments and before project data leaves; no contract approval, product plan approval,
result acceptance by the human or contract judging (so no `judge_*` transcript, and no plan-check
or integration choice in setup and Settings); Catervas never merges (the `integration`
escalation's one choice opens the pull request again); work starts from unmerged dependency
branches; the Designer's plan approves itself. The plans name files and symbols of `main`; where a
line number has moved, find the symbol by name.

A later change, ADR 0064 (agents work in parallel), amends plan 6's `open_tasks` after it lands.

One executor (Sonnet 5.5) runs the plans in this order, one task at a time, each task one commit,
and each task leaves `cargo xtask check` passing (it runs `pnpm check` too). Before the first task:
`export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.

| Order | Plan | What it does |
|---|---|---|
| 1 | `01-decision-and-choices.md` | ADR 0062; every escalation's choices say what each does, one suggested; `catervas resolve --extra-tries`; the end of a run lists a command per choice |
| 2 | `02-escalation-view.md` | The web app's escalation view: one button per choice, its next words, the suggested one marked; the Playwright journey of C2 (d) |
| 3 | `03-deliveries.md` | `delivery.closed` and its trigger; the review, the refreshes, the changelog task and the retro run per delivery instead of per sprint |
| 4 | `04-web-without-sprints.md` | Sprints, the Backlog and "Plan work in sprints" leave the web app (C3's web half) |
| 5 | `05-engine-without-sprints.md` | Sprints leave the engine, the command line and the wire; old projects keep working (C2 (a), C3's schema half) |
| 6 | `06-wip-and-resume.md` | Blocked, escalated and waiting tasks free their agent; a cleared task resumes, or is handed over (C2 (b), (c)) |
| 7 | `07-words.md` | Sprint words leave the roles, skills, prompts, tool descriptions, help, README and CONTRIBUTING |
| 8 | `08-spec.md` | `docs/SPEC.md` revision 0.116 records all of it (hard rule 8) |

SPEC is recorded once, by the last plan, in one revision (0.116): the sprint text runs through
almost every section, and rewriting it per plan would rewrite the same paragraphs several times.

Not in these plans: R6, Iris's walkthrough. The controller runs it with the browser once plans 1
to 8 have landed; its outputs are `docs/catervas/design/ux-walkthrough.html` and its agent twin
`docs/catervas/design/ux-walkthrough.agent.html` (C6). C5, C7 and C8 are the review and the
founder's checks, not tasks.

Never edit this repository's `.catervas/`: its leftover `plan_in_sprints` key is ignored (R2).

## The epic's exit criteria, where each lands

- C1 (`pnpm run check`) and C2 (`cargo xtask check --integration`): every plan's verification.
- C2 (a): plan 5 Task 1. C2 (b) and (c): plan 6 Tasks 2 and 3. C2 (d): plan 2 Task 2.
- C3: plan 4 (the page, the journey, `en.ts`) and plan 5 Task 5 (`team-template.schema.json`).
- C4: plan 1 Task 1.
