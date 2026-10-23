# Phase 10, step 09b: The usability test

Status: ready (a runbook; the founder runs it with three people)
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 11; ADR 0059 ("User experience and user journey is a huge concern")
Depends on: step 09 signed off
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

Three people other than the founder run the journey unassisted, thinking aloud, each on their own computer:

1. one with several Claude Code repositories;
2. one who uses Claude only on the web (claude.ai or the Claude app);
3. one with a single project.

What they did, how long it took, and what confused them are recorded. Every finding is fixed on this branch before the phase merges.

## How each session runs

- **Setup.** The founder installs Catervas from the phase branch's build on the person's computer, or screen-shares one prepared for them, and starts `catervas serve`. After that the founder says nothing but "What are you thinking?", and helps only when the person is stuck for two minutes. Each help is recorded.
- **The task given.** "Catervas can find your projects and set up a team for each. Get your teams set up, then tell me what each team is for and why its projects are together."
  - Person 2 is also told: "Bring in your claude.ai history if you'd like; the export email may take a while, so drop it in when it comes."
- **Measures**, as in step 09:
  - time to teams created;
  - questions asked;
  - Nos to a proof;
  - undos;
  - whether they can say, unprompted, why each team's projects are together;
  - the helps.
- **After.** Three questions, each answered in their own words: "What was confusing?", "What did you expect that didn't happen?", "Would you trust it to group your projects again?"

## The record

`docs/milestones/many-teams.md` gets a section "Usability test, <date>". For each person (no name, only 1, 2 or 3 and their kind), it holds:

- their computer;
- their measures;
- every proposal shown and their answer;
- every quote that shows confusion;
- every help given.

Then a list of findings, each with its fix's commit or the founder's decision to leave it, and "Findings fixed, <date>."

- [ ] `docs(milestones): the usability test of many teams`
