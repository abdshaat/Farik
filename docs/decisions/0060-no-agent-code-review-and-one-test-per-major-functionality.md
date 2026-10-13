# 0060. No agent review of code, and one test per major functionality

Date: 2026-10-12
Status: accepted (the founder, 2026-10-12: "Change the current coding convention to no review by opus 5.5 after sonnet 5.5 write code and make the testing only 1 test per major functionality. The reason behind this decision is that the current development is slow").

Supersedes, in part:
- ADR 0032 and its review rule: Opus 5.5 no longer reviews each task after Sonnet 5.5 writes it, and there is no fix report or fix wave.
- CLAUDE.md hard rule 10's landing review with mutation as its bar.
- `docs/standards/workflow.md` stage 5's per-task review and stage 2's "every test named, one per case".

## Context

Every step ran: an Opus plan, an Opus readiness review, Sonnet's tasks with a named test for every case, an Opus landing review that re-introduced the bug each test claimed to catch, then a Sonnet fix wave. Phase 10 step 02 took a plan, a review, seven tasks with about 40 tests, an 85-mutant review and twelve fixes. The founder found development too slow.

## Decision

- **No agent reviews code after it is written.** Sonnet 5.5 executes a step's plan and the step lands when the full check passes locally (`cargo xtask check`) and the `check` workflow passes on GitHub. No landing review, no mutation bar, no fix report.
- **One test per major functionality.** A task's plan names one test for each major piece of functionality it adds (the main behaviour a user or a caller relies on), not one per case or refusal. A security guarantee (a key or a secret never stored, a path never followed) counts as a major functionality and keeps its test.
- **Unchanged:** that test is written first and watched to fail for the right reason (hard rule 1); never skip, disable or quarantine a failing test (hard rule 9); no completion claim without fresh evidence (hard rule 2); Opus 5.5 still writes plans, and a fresh Opus session still checks each plan's readiness before execution (hard rule 3); the founder reviews and merges every pull request (rule 10's pull request review); the governor's existing transition tests stay.

## Consequences

- A step costs one plan, one readiness review and one execution, instead of adding a review and a fix wave.
- Fewer tests means defects a per-case test or a mutation review would have caught can reach the phase branch; the founder's live checks and the CI check are what remain to catch them.
- Plans already written with per-case tests (phase 10 steps 03, 03b, 04) are executed with one test per major functionality from the next task on; each records the reduction in its task.
