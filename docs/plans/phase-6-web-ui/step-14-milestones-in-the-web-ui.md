# Phase 6, step 14: Milestones 0 and 1 in the web UI

Status: draft, waits on steps 01 to 13 landing. The founder runs it (the founder, 2026-09-29: "I will run step 10", then step 11 after the split of step 07, and step 14 since revision 22 added steps 11 to 13, ADR 0026).
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` section 11 (Milestones 0 and 1), F17; the flow it exercises is 4.1, 5.2 to 5.9, 5.14, and 5.16
Depends on: steps 01 to 13 of this phase, landed. A start gate applies: stage 1 does not begin until step 13 has landed and `cargo xtask check --integration` passes on the phase branch.
Readiness confirmed by: (pending, once step 13 lands)

This step writes no product code. It is a runbook, like phase 3's step 18 and phase 4's step 08, and it keeps their roles:
- **[A]** is the agent preparing and recording the run.
- **[F]** is the founder, or a test user. The agent stops, hands over, and waits.

The agent never acts in the human's name in the browser or on the command line. It does not file, answer, approve, accept, resolve, lock, post, or start or end a sprint. It does not run a command that starts sessions, and it does not set or read the credential.

## Goal

Two recorded runs that close Milestones 0 and 1 (spec 11).

1. **The team sprint** (phase 4 step 08's decisions, driven from the browser). It uses a fresh copy of Farik's own repository, seven agents (phase 4's six and the UI/UX Designer, the cap), and one sprint started by the founder in the browser. It carries two requests, a small one and a large one, through triage, plan checking, the Product Manager's questions and plans, the founder's approval, the breakdown, the building, the Architect's review (after the Designer's check, for any UI change), the Marketing Specialist's CHANGELOG entry, acceptance, and integration. The channel carries reactions and meetings throughout, and the sprint ends with a review and a look back. The founder reads the gates, the diffs, the channel, and the log in the browser, and writes whether each task was done as its plan said.
2. **The thirty-minute test.** Five test users, at least three of them non-technical. Each starts at `farik serve` on a machine prepared with Farik, Claude Code, git, and Docker installed (the phase's "Ends with"). Each must reach an accepted task on their own repository within thirty minutes, with no help.

The records are `docs/milestones/m1-team-exit.md`, `docs/milestones/m1-web-exit.md`, and their event log exports.

## Decisions

- **The team sprint keeps phase 4 step 08's decisions**, except as below:
  - its team of six on the founder's models, plus the UI/UX Designer as the seventh agent, which is the cap (ADR 0026, approved 2026-09-30);
  - no dollar limit;
  - the founder's subscription token;
  - its loop limits and rules;
  - its two requests, word for word;
  - `auto_merge` into `test/m1-exit` in a bare clone.

  The Designer's review of UI changes will occur only if the run touches UI files. Phase 4 step 08's two requests are CLI work, so the run may never touch one, and the Designer may sit idle. The record says so rather than counting it a failure, and this step adds no request for it: the requests stay word for word.

  The changes:
  - **Driving and records.** Every human action is taken in the browser: the request box, questions, plan approval, sprint start, channel posts, acceptance, and help. The command line is used only by [A], for the record's readings (`farik metrics`, `farik log --json`, `farik channel`).
  - **Who checks plans.** Under the founder's rule of 2026-09-29, the Architect checks plans, because the team has one. Phase 4 had the Scrum Master judge. The pass criteria change to match.
  - **Order of actions** in the browser:
    1. file request 1, then request 2, on Today;
    2. answer the questions;
    3. approve the epic;
    4. start the sprint from the board with no budget;
    5. watch the work;
    6. mention `@arch` once in the channel;
    7. accept the epic with a note;
    8. read the review and the look back in the channel.
- **The thirty-minute test.**
  - **The five users.** The founder recruits them. At least three are non-technical, meaning they do not write code for work.
  - **The machine.** Each user's machine has `farik` (a release build of the phase branch head), Claude Code, git, and Docker installed and working. Each user brings a repository of their own, a git folder, or starts a new project in the wizard. Each uses their own Claude subscription or API key, or one the founder provides for the test.
  - **Start.** The clock starts when the user types `farik serve` in a terminal. That one command is given to them. It stops at the first `human.accepted` of a result, or at thirty minutes.
  - **The observer.** The founder, or someone the founder names, watches without helping. Every question the user asks, and every place they get stuck, is written down with its time.
  - **Pass.** At least four of the five reach an accepted task within thirty minutes, with no help given. At least two of those four are among the non-technical users. The milestone's criterion says "a new user with no help". The founder confirmed this bar on 2026-09-29 ("as long as it passes it's okay, we can refine later"); it may be raised before the first session, and the record says which bar was used.
  - **Consent.** Each user agrees in writing to the session being observed and to their repository's name appearing in the record. The record never includes their code or their key.
- **Records.** Both follow step 18's rules. The export is `farik log --json`, committed whole and never edited, with its line count, highest seq, and sha256. Screenshots of each gate the founder decided go into `docs/milestones/m1-team-exit/`.

## File map

```
docs/milestones/m1-team-exit.md (+ .events.jsonl, and a screenshot folder)   creates (Task 1, Task 2)
docs/milestones/m1-web-exit.md                                              creates (Task 3, Task 4)
docs/plans/phase-3-runtime/step-18-milestone-0-exit.md, docs/plans/phase-4-team/step-08-milestone-1-exit.md   modifies: closed by this run
```

## Runbook

### Stage 1: prerequisites [A]

This is phase 4 step 08's stage 1, in `~/farik-m1/`:
- the bare clone, `test/m1-exit`, the run's clone, and the sandbox image with its smoke test;
- the release build;
- `team.yaml`, written with the seven agents and `judgment` left at its defaults, so the Architect checks plans;
- the two briefs, saved to paste into the request box.

[A] then hands over to [F] with the one command, `cd ~/farik-m1/farik && farik serve`, after the founder has exported the token.

### Stage 2: the team sprint [F]

In the browser, the founder follows the order of actions above. Wherever the run pauses on the founder, it is visible on Today. Anything that goes wrong in the browser is an incident, recorded with its time and a screenshot. The founder decides whether to go on or to re-run from stage 1.

### Stage 3: the record [A]

This is phase 4 step 08's stage 5, plus the screenshots of each gate. The pass criteria are phase 4 step 08's, with these changes:
- `contract.judged` is by `arch`, not `sm`;
- every human action appears in the log as `human`, taken from the browser.

- [ ] Task 1 [A]: `docs(docs): record the milestone 1 team exit run in the web UI`

### Stage 4: the founder's review [F]

- [ ] Task 2 [F]: `docs(docs): add the founder's milestone 1 team review`

### Stage 5: the five sessions [F]

For each user:
1. prepare the machine;
2. hand over the one command;
3. observe for thirty minutes;
4. write the notes the same day.

### Stage 6: the record [A]

[A] writes the record for each user: the start and stop times, whether an accepted task was reached, the questions and stuck points with their times, and the screens where time went. It adds a summary against the pass bar and the fixes the sessions suggest. The fixes are listed, not made: they go in a later change.

- [ ] Task 3 [A]: `docs(docs): record the milestone 1 thirty-minute test`
- [ ] Task 4 [F]: `docs(docs): add the founder's milestone 1 verdict`

## Pass criteria

- **Team sprint:** phase 4 step 08's criteria, with the two changes in stage 3.
- **Thirty-minute test:** the pass bar in Decisions.
