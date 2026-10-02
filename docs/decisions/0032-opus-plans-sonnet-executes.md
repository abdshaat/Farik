# 0032. Opus 5.5 plans and judges, Sonnet 5.5 executes

Date: 2026-10-02
Status: accepted (the founder, in conversation, 2026-10-02)

Numbering: ADRs 0030 and 0031 exist on the `phase/7-role-kits` branch and are not on `main` yet. This is 0032 so that the three do not collide when that branch merges.

Amends ADR 0010: each task is reviewed once again, replacing that ADR's "tasks are not reviewed one by one", and a task is not reviewed a second time after its fixes. The stages, the hard rules and the mutation bar are unchanged.

## Context

The workflow (`docs/standards/workflow.md`) has always said who may not review (the author) and never which model does which work. Since ADRs 0008 and 0010, a step plan carries decisions, interfaces and test lists but not function bodies, so execution is the part of a step that follows a decided plan. Planning, design and judging reviews need the strongest reasoning available. Executing a decided plan under TDD, with the compiler and the suite as the check, needs less, and costs less on a smaller model.

The founder decided on 2026-10-02: "Change the current workflow structure to opus 5.5 to plan while sonnet 5.5 executes the plan."

The same day the founder added: "On the current repo workflow, the review on each task must run only once by opus 5.5 then the reviewer (opus 5.5) must send the implementer (sonnet 5.5) a detailed report of what to fix. Then after the implementer (sonnet 5.5) fixes all the issues no other reviews must happen on that task. On the step level, after implementing a step, the entire test suite must pass and github action must also pass."

Options considered:

- **Opus 5.5 for everything.** Highest quality per task, and the most expensive. It spends the strongest model on transcription-like work that a decided plan, the type checker and a watched-to-fail test already constrain.
- **Sonnet 5.5 for everything.** Cheapest. It puts design judgement and the independent check on the same, weaker model, and the reviews are where ADR 0008 found the defects that mattered.
- **Sonnet 5.5 reviewing its own execution.** Breaks hard rule 10 outright: the author accepts its own work. A different model identity does not help if the same session wrote and judged the code.

## Decision

Opus 5.5 does the planning and judging: brainstorm, design, step plans, mockups, readiness reviews, and the one review of each task. A readiness review is run by an Opus session other than the one that wrote the plan. The reviews stay on Opus because they are the independent check on the executor (hard rule 10).

Sonnet 5.5 executes a plan that has passed its readiness review: the tasks under TDD, the fixes a task's review reports, the recording of the spec, and the runbook preparation.

Each task is reviewed once, by Opus 5.5, on its running code, with mutation as the bar. The review ends in one detailed fix report to the Sonnet implementer. For each finding it gives the severity, the file and line, what is wrong, the fix, and the test that must fail without the fix. Sonnet fixes every finding in the report, watches each named test fail before its fix, and commits the fixes as new commits. No further review of that task follows. The reviews of a step's tasks, recorded in the pull request thread, are that step's landing review.

A step lands when every task in it has been reviewed and its fixes committed, the full check passes locally (`cargo xtask check`), and the `check` GitHub Actions workflow passes on the step's last pushed commit.

Unchanged: every hard rule, the pass bars (including mutation as the landing review's bar), and one task, one commit.

## Consequences

A plan must be complete enough to execute without design judgement. The readiness review's three rules (every decision made, no ambiguity, no forward dependencies) now also decide whether Sonnet can execute the plan alone, so the reviewer applies them with that reader in mind.

Any decision found missing during execution goes back to the planning role. The executor does not decide it silently; it stops, records the question, and a planner answers it in the plan or an ADR before the task continues. This is the same failure ADR 0008 names, an undecided question decided differently by whoever hits it next, now with a rule for who resolves it.

Planning and review cost stays at Opus rates and execution drops to Sonnet rates. Reviewing task by task costs more Opus time per step than one landing review did, and finds each defect closer to the commit that made it.

A fix is not reviewed. The report stands in for a second review: it states each fix, and the test that proves it, as precisely as a step plan states a task. Fixing is therefore execution, not judgement, and the step's gate (the full suite and GitHub Actions) checks that the fixes broke nothing else. What this costs: a fix done wrongly, or a test that does not catch what the report said it would, can land unseen. The reviewer writes the report with that in mind. If a fix needs a decision the report does not make, it goes back to the reviewer as a question, not as a second review.

Hard rule 10 still holds. The implementer never accepts its own work: the Opus review accepts the task once the listed fixes are made, and the step's gate is mechanical. The mutation bar stays the reviewer's bar.

Nothing enforces the split mechanically. It is a process rule applied by whoever dispatches the work, like the size target in ADR 0008.
