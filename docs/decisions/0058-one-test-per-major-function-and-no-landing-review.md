# 0058. One test per major function, and no landing review

Date: 2026-10-13
Status: accepted (the founder, 2026-10-13: "Why does it take so long to review and implement a task? please stop the opus 5.5 review and reduce the testing to 1 test per major functionality."; then "Change the development convention to one major test per major functionality. The tests do not need to be comprehinsive and the implementer stays as sonnet 5.5")

## Context

Phase 9's steps took about three hours each to land in the cloud container:
- about an hour of execution, much of it rebuilding the large Rust workspace for each of the 10 to 20 tests a step plan names, one at a time under TDD
- 20 to 40 minutes for the full check
- 25 to 35 minutes for the Opus landing review, whose mutation bar rebuilds the workspace for each of its 30 to 50 mutations
- 30 to 60 minutes for the fixes and another full check

The reviews found real defects (a revision expression stored as a branch, a missing repository ending `serve` for the whole team, a rescan that churned the log), but the founder judged the time too high for what is left.

## Decision

From 2026-10-13, and until the founder changes it:
- **The development convention is one major test per major functionality.** Each major piece of functionality a step builds gets one test of its main behaviour, still failing first for the right reason before the code (hard rule 1 holds). Tests need not be comprehensive: edge cases, refusals and error paths a plan lists are not each tested. The other tests a plan names are not written. Existing tests stay; none is removed or loosened (hard rule 9 holds). New step plans name one test per major functionality.
- **Sonnet 5.5 stays the implementer** (ADR 0032).
- **No landing review.** A step lands when its tasks are committed, the full check passes and the pull request's `check` workflow passes. The Opus landing review with its mutation bar (hard rule 10's second sentence, workflow stage 5) is not run. Step 02c's review, started before this decision, was stopped.
- **Unchanged:** the readiness reviews of plans, which are already done for phase 9; the full check before each push; the pull request's review by someone who did not write it before it merges (hard rule 10's first sentence); the spec in the same pull request.

## Consequences

- Steps land in roughly an hour and a half rather than three.
- Defects that only a second test or a mutation would have caught reach `main` and the founder's live checks. Step 08's live run, and the pull request's own review, are the remaining nets.
- The step plans written before this decision still name their tests; the executor's report says which test it wrote for each task.
