# 0008. Step plans carry decisions and interfaces, not implementation code

Date: 2026-09-21
Status: accepted

Amends ADR 0003. Planning stays two-level and a plan is still ready only when every decision is made, nothing is ambiguous, and it has no forward dependencies. What changes is what a step plan contains, how many rounds its readiness review takes, and where ambiguity is caught.

## Context

ADR 0003 required a plan to be fully decided before code. `docs/plans/step-template.md` implemented "fully decided" as "write the actual code; do not write 'add validation'", together with "repeat code across tasks rather than referencing an earlier task". Three phases later the consequence is measurable. The step plans hold 40,424 lines, of which 33,998 — 84 per cent — are inside code fences, against 24,643 lines of hand-written Rust in the repository. The implementation is written twice: once in Markdown, where neither the compiler nor the test suite can read it, and once in `.rs`.

The cost shows up as wall clock. Across the five steps from phase 1 step 08 to phase 2 step 08, the time from a plan's first commit to the commit marking it ready totals 10 hours 20 minutes over 3 to 7 readiness rounds, and the implementation commits that follow span 14 minutes in total. Execution is that fast because nothing is left to decide; it is transcription. Of the repository's 136 commits, 79 are docs-only and 32 are revisions to plans that had not yet produced a line of code. Phase 1 step 08 is the extreme: 5,019 lines of plan, 4,698 of them code, seven readiness rounds, four hours, and one implementation commit two minutes later.

The defects that mattered were never in the plans. The landing reviews of phase 2 refused sixteen times and found ten correctness defects a green check had not: subscribers handing out sequence numbers out of order, an event applied out of order dropped for good, a deferred transaction refusing 19 times in 600, a flag whose effect nothing tested, `is_clean` answering about a different repository, two writers publishing a file holding both writes, a product boundary failing open. None of those are visible in Markdown. They were found by running code and by re-introducing mutations to see which tests notice. The expensive gate was catching cheap problems and the cheap gate was catching the expensive ones.

Two options were realistically on the table. The first was to break phases into smaller phases, shortening each pull request. The measured cost is per step and not per phase, so this does not touch it: phase 0 had three steps and phase 1 had nine, and per-step cost was the same or worse in the phase with more of them. More phases would add a branch, a pull request and a readiness gate per unit while leaving the double-writing in place. The second option, taken here, is to change what a plan contains.

## Decision

A step plan states its decisions, the public signatures it produces and consumes, the file map, the tests to write with what each asserts, and the verification commands. It does not contain function bodies. Readiness review is one round against those things, with a second round only when a decision the plan rests on turns out not to be made. Ambiguity inside an implementation is caught by the type checker and by a test watched to fail, not by a reader. The landing review of running code, with mutation as its acceptance bar, is kept and is written into `docs/standards/workflow.md` rather than practised as folklore. The instruction to repeat code across tasks rather than reference earlier ones is removed; it existed only to make a plan self-contained as code.

## Consequences

Decisions are still made before code and still written down. What moves is where the implementation is first written. Plans get shorter — the target is around 300 lines — so a readiness reviewer reads decisions and boundaries, which is the judgment a reviewer can actually apply, instead of proof-reading a draft of the codebase.

Execution gets slower per step, and that is the point: the work returns to the editor, where `cargo check` answers in seconds what a readiness round answered in an hour. Some of the time saved in planning is spent there, and the expected net is roughly half the current cost per step rather than an elimination of it.

A plan that no longer carries its code can be under-specified in a way the old template made impossible. The guard is that its signatures and its test list are exact. A step whose interfaces cannot be written down without a forward reference is still a step in the wrong order, and the no-forward-dependency rule is unaffected, because it is a statement about interfaces and plans keep those.

This is the first place Farik's workflow departs from the superpowers plugin adopted in ADR 0001 rather than merely adding to it. The plugin's planning skill writes the implementation into the plan, which is where the habit came from, and it will go on doing so. `docs/standards/workflow.md` is the contract and the plugin is the implementation, so a plan the plugin drafts is edited down before review — a manual step, and the most likely way this decision quietly stops being followed.

Nothing enforces the size target mechanically. A `cargo xtask` check over plan files was considered and not built: a line count is a poor proxy for the thing being limited, and a target a reviewer applies is enough while there is one reviewer.

The three phases already merged were planned under the old template. Their plans are left as they are; they are the evidence for this decision and rewriting them would cost what this decision is meant to save.
