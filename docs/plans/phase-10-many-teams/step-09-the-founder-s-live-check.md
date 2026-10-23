# Phase 10, step 09: The founder's live check

Status: ready (a runbook; the founder runs it)
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 11; the flows it exercises are 4.1, 4.4, 5.5, 8.4, F1, F12, F17
Depends on: steps 01 to 08 committed, and `cargo xtask check` passing on the phase branch's head, whose sha is recorded at the start of the record
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

On the founder's own computer, with their real Claude Code history, the founder runs the whole journey in the web app. Each run is recorded in `docs/milestones/many-teams.md` with the journey's measures, and signed off:

- find the projects;
- answer every proof;
- create the teams;
- run two unrelated teams at once;
- act on a reminder;
- read the analytics.

Every wrong proof, confusing screen or failure found is filed as a fix on this branch before the phase merges.

## Before

1. Build the phase branch's head: `scripts/catervas-start.sh`, or `cargo build --release` and `pnpm -C apps/web build`.
2. Make sure Claude Code is signed in (`claude auth status`), which Catervas asks from the AI account page since pull request #92.
3. Stop any `catervas serve` running. A copy of `~/.config/catervas` may be kept so it can be put back after.
4. Have at least one project of two or more related repositories on the computer (the founder's choice, for example a web app and its API), and one unrelated project.

## The run

Each line is done in the web app, with no terminal after `catervas serve` starts. What to record is in brackets.

1. Start `catervas serve`, open the link, and pass the computer check. The offer "Find your projects?" shows. [time at **Look for my projects**]
2. Watch the scan to Done. [counts shown; "Couldn't read" if any; time to Done]
3. On "Here's what you work on", check every row: the name, last worked, jobs and services. Correct any choice. [each row that was wrong, and why]
4. Answer every "Do these work together?". [each pair: the proof shown, the answer, and whether the proof was right]
5. Open "Maybe related". [each pair and the answer]
6. On "Your teams", press **Suggest names with Claude**. [the estimate shown and the cost after] Then create the teams. [time at Create; the number of questions asked in all]
7. Undo one new team, then create it again from the Watched section's **Make a team**.
8. Start two unrelated teams. Give each a small request, set "Agents working at once" to 1 on This computer, and watch them take turns. [whether both progressed; the order of their sessions]
9. Set a computer daily limit of $1 and watch both teams stop for the day at it. Then clear it.
10. Next morning (or with the reminder time set a few minutes ahead), receive the reminder:
    - with a tab open, as a browser notice;
    - with none open, from the computer (Windows' under WSL).
    - On "Needs you today", give one forgotten session to its team. [the notice's text; the request filed]
11. Open Analytics, this week and this month. [whether the numbers look right against Claude's own usage page; anything counted twice]
12. On This computer, press **Forget everything**, and check that the teams still run.

## The record

`docs/milestones/many-teams.md` gets a section "Founder's live check, <date>". It holds:

- the sha;
- the computer (OS, and WSL or not);
- the Claude Code version;
- every bracketed note above;
- the journey's measures (design section 0): time from **Look for my projects** to teams created, questions asked, Nos to a proof, undos, and whether the founder can say why each team's projects are together;
- the fixes found, each with its commit;
- "Signed off by the founder, <date>."

- [ ] `docs(milestones): the founder's live check of many teams`
