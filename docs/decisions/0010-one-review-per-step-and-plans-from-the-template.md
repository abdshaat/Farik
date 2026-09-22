# 0010. One review per step, and step plans drafted from the template

Date: 2026-09-22
Status: accepted

Amends ADR 0001 and ADR 0008.

## Context

Before phase 3 starts, the workflow documents were reviewed for overhead that buys no quality. Three things stood out. The superpowers `writing-plans` skill drafts a plan with the implementation in it, and ADR 0008 then required that draft to be edited down by hand, which is two drafting passes per step and, by ADR 0008's own account, the step most likely to be skipped. The workflow asked for a two-stage review of every subagent task, on top of the landing review of every step and the final review of the phase; ADR 0008's measurements put the defects in the landing reviews of running code, not in reviews of single tasks. And the full check was run at the start of every step, even when the head of the branch was the commit the previous step's landing review had just checked.

The same rules were also restated in several places (CLAUDE.md, `workflow.md`, the step template, the pull request template, ADRs 0001 and 0003), so a change to one rule meant editing four files and the copies were starting to drift.

## Decision

Step plans are written by copying `docs/plans/step-template.md`, not with the plugin's `writing-plans` skill. Tasks are not reviewed one by one; the landing review of each step, with mutation as its bar, is the review. The full check runs at the start of a phase, and at the start of a step only when the branch's head is not the commit the last landing review verified. Each rule is stated in one place and the others link to it: the Definition of Done is the pull request template's checklist, and the step template no longer restates the plan rules or CLAUDE.md's hard rules. The project plan carries interface lists only for the phase being planned next.

## Consequences

Each step saves a drafting pass, a round of per-task reviews, and usually one full run of the check. The landing review now carries the whole review load for a step, so a step whose landing review is skipped or cut short (as happened to phase 2's steps 08 and 09) has had no review at all; that gap is recorded on the pull request and closed before the phase is marked ready. The interface sketches for phases 4 to 6 leave the tree; they stay in history and are rewritten when those phases are planned, against the code that exists by then.
