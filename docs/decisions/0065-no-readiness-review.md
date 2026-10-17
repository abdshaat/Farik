# 0065. No readiness review of plans

Date: 2026-10-16
Status: accepted (the founder, 2026-10-16: "Stop reviewing everything, let the compiler and user testing do the most of the heavy work").

Supersedes, in part:
- ADR 0060's "a fresh Opus session still checks each plan's readiness before execution".
- CLAUDE.md hard rule 3's "passed by one readiness review".
- `docs/standards/workflow.md` stage 2's readiness review by a reviewer other than the author.

## Context

ADR 0060 removed every review of code but kept one readiness review of each plan by an Opus session that had not written it. On 2026-10-16 four standalone changes (ADRs 0061 to 0064) were planned and each plan went through such a review, some through a second round; the reviews found real gaps, mostly ones the compiler or the tests would also have found during execution. The founder found it still too slow.

## Decision

- **No agent reviews a plan.** Opus 5.5 writes the plan and holds it to the three readiness rules itself (every decision made, no ambiguity, no forward dependency); Sonnet 5.5 executes it.
- **No agent reviews code**, as ADR 0060 already says.
- **What carries the check:** test-first with one test per major functionality (hard rule 1, ADR 0060), the full check locally and the `check` workflow on GitHub, and the founder's own testing of the product before merging.
- **Unchanged:** a decision found missing during execution goes back to the planner, not decided by the executor; the founder reviews and merges every pull request.

## Consequences

- A step costs one plan and one execution.
- Gaps a reviewer would have found in a plan now surface as compile errors, failing tests or the founder's testing, later and sometimes more expensively.
