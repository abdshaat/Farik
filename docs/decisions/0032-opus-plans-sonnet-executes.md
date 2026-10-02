# 0032. Opus 5.5 plans and judges, Sonnet 5.5 executes

Date: 2026-10-02
Status: accepted

Numbering: ADRs 0030 and 0031 exist on the `phase/7-role-kits` branch and are not on `main` yet. This is 0032 so that the three do not collide when that branch merges.

Amends ADR 0001 and ADR 0010 in who performs each stage. The stages, the hard rules and the pass bars are unchanged.

## Context

The workflow (`docs/standards/workflow.md`) has always said who may not review (the author) and never which model does which work. Since ADRs 0008 and 0010, a step plan carries decisions, interfaces and test lists but not function bodies, so execution is the part of a step that follows a decided plan. Planning, design and judging reviews need the strongest reasoning available. Executing a decided plan under TDD, with the compiler and the suite as the check, needs less, and costs less on a smaller model.

The founder decided on 2026-10-02: "Change the current workflow structure to opus 5.5 to plan while sonnet 5.5 executes the plan."

Options considered:

- **Opus 5.5 for everything.** Highest quality per task, and the most expensive. It spends the strongest model on transcription-like work that a decided plan, the type checker and a watched-to-fail test already constrain.
- **Sonnet 5.5 for everything.** Cheapest. It puts design judgement and the independent check on the same, weaker model, and the reviews are where ADR 0008 found the defects that mattered.
- **Sonnet 5.5 reviewing its own execution.** Breaks hard rule 10 outright: the author accepts its own work. A different model identity does not help if the same session wrote and judged the code.

## Decision

Opus 5.5 does the planning and judging: brainstorm, design, step plans, mockups, readiness reviews, landing reviews and re-reviews. The reviews stay on Opus because they are the independent check on the executor (hard rule 10).

Sonnet 5.5 executes a plan that has passed its readiness review: the tasks under TDD, the fix waves that answer a review, the recording of the spec, and the runbook preparation.

Unchanged: every hard rule, the pass bars (including mutation as the landing review's bar), and one task, one commit.

## Consequences

A plan must be complete enough to execute without design judgement. The readiness review's three rules (every decision made, no ambiguity, no forward dependencies) now also decide whether Sonnet can execute the plan alone, so the reviewer applies them with that reader in mind.

Any decision found missing during execution goes back to the planning role. The executor does not decide it silently; it stops, records the question, and a planner answers it in the plan or an ADR before the task continues. This is the same failure ADR 0008 names, an undecided question decided differently by whoever hits it next, now with a rule for who resolves it.

Planning and review cost stays at Opus rates and execution drops to Sonnet rates. Reviews are expected to find more of the executor's slips, not fewer, since the landing review is still the only review a step gets (ADR 0010); the mutation bar is what keeps that honest.

Nothing enforces the split mechanically. It is a process rule applied by whoever dispatches the work, like the size target in ADR 0008.
