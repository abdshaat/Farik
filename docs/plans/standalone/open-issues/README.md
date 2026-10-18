# Standalone: the open issues of the note-chat2 journey

Status: done
Branch: `fix/open-issues` (work outside a phase, one pull request to `main`), cut from
`fix/stray-changes` (ADRs 0061 to 0064, 0066 to 0068 executed; SPEC 0.121)
Decision: ADR 0069 (`docs/decisions/0069-the-open-issues-of-the-note-chat2-journey.md`)
Readiness confirmed by: none (ADR 0065); the author held every plan to the three rules (every
decision made, no ambiguity, no forward dependency)

## Why

The journey of 2026-10-17 (`reports/note-chat2-journey-2026-10-17.html`) left fourteen issues open, and the founder added a fifteenth: pull request bodies too long and messy.
The worst: a reviewer that cannot finish has no legal move to the owner (three `no_such_transition`
refusals of `verifying -> blocked`), and any refusal in `verifying` parks the task for good, so
CTV-7, CTV-14, CTV-24 and CTV-25 never moved again. The founder: "fix the open issues in Catervas,
especially the governor escalation". The visual redesign is planned on its own branch; only words
are fixed here.

## Before the first task

1. `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.
2. Run `cargo xtask check` once for a clean baseline, and `cargo xtask check --integration` when
   Docker and git are there (plans 3 and 5 add Docker and git tests).
3. Never open or write `~/note-chat2/.catervas/local/catervas.db` or this repository's `.catervas/`.

The plans name files and symbols as they are at `6ba32d2a`. Where a line has moved, find the
symbol by name.

## Order

One executor (Sonnet 5.5) runs the plans in this order, one task at a time, each task one commit,
each task leaving `cargo xtask check` passing (it runs `pnpm check` too).

| Order | Plan | Tasks |
|---|---|---|
| 1 | `01-blocks-reach-the-owner.md` | 1 the table's new rows and where a block returns; 2 the holder blocks as itself; 3 the choices fit who is stuck; 4 the sessions are told how to block |
| 2 | `02-no-silent-stops.md` | 5 an agent's refusal does not park a review; 6 a refusal of the governor's own escalates, and documents are not UI; 7 the governor asks `ready` first; 8 the reviewer sees the Designer's checks |
| 3 | `03-checks-see-git.md` | 9 the store finds a worktree's git store and a password in its settings; 10 the checks' container mounts it read-only |
| 4 | `04-tidy-before-work.md` | 11 Catervas tidies before each work session, and never during a merge |
| 5 | `05-pull-requests-that-stop-merging.md` | 12 the forge reads whether a pull request merges; 13 the store merges into a kept branch and keeps the conflicts; 14 rule 2 refreshes or sends back; 15 the assignee is told and cannot commit markers; 16 a delivery counts a task once |
| 6 | `06-engine-fixes.md` | 17 a project with no lockfile is npm's; 18 a delivery report is titled by its name; 19 an added agent gets its role's defaults; 20 the credential line is honest; 21 the serving tests keep to their own folder; 22 a pull request's body is short |
| 7 | `07-web-and-words.md` | 23 the spending example reads as one; 24 Settings sets the integration branch; 25 `/chats` opens the chats; 26 the stale words |
| 8 | `08-record.md` | 27 `docs/SPEC.md` revision 0.122, and the ADRs it amends |

Issue to plan: 1 governor escalation, plans 1 and 2; 2 git in checks, plan 3; 3 tidy, plan 4; 4
pull requests, plan 5; 5 the reviewer's browser, plan 2 Task 8; 6 spend, plan 7 Task 23; 7 words,
plan 7 Task 26; 8 scan, plan 6 Task 17; 9 integration branch, plan 7 Task 24; 10 report titles,
plan 6 Task 18; 11 `/chats`, plan 7 Task 25; 12 persona, plan 6 Task 19; 13 credential, plan 6
Task 20; 14 test isolation, plan 6 Task 21; 15 short pull request bodies, plan 6 Task 22.

Not in these plans (ADR 0069, decision 9): the product plan's and a Catervafication's need for an
Architect or a Scrum Master, task ids in file names and document text, judging "suite passes"
against the base's own failures, and the visual redesign.
