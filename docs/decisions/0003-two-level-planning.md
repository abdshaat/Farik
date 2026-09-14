# 0003. Two-level planning: phases and steps, fully decided before code

Date: 2026-09-14
Status: accepted

## Context

The workflow adopted in ADR 0001 requires a plan per change but says nothing about how the changes relate to each other. For a project this size, with agents doing most of the execution, that leaves two failure modes open. The first is a plan that reads well but rests on a decision nobody made, so the agent executing it makes the decision silently, differently from the agent executing the next plan. The second is a plan that assumes something a later plan will provide, so the work lands with a stub or a placeholder and the project accumulates the kind of half-finished seams that are expensive to close later.

The alternative is to keep single-level plans and rely on review to catch both problems. Review does catch them, sometimes, after the code is written.

## Decision

Planning is two-level. The project plan divides the project into phases and phases into steps. Each phase is one branch and one pull request; each step has its own plan and lands as commits on the phase branch. A plan is ready to execute only when every decision it rests on is written down, nothing in it is ambiguous, and it depends on nothing that is not already merged or already committed earlier on the same phase branch. These three rules are checked by a reviewer before the first task starts. Details in `docs/standards/workflow.md` stage 2 (Plan).

## Consequences

Planning takes longer and happens earlier. Decisions that would otherwise be made in the moment are made up front, some of them before the information that would make them easy is available. When such a decision turns out wrong, the fix is an ADR and a change to the project plan, not a quiet workaround.

Phases must be ordered so that no step needs anything from a later phase. This constrains the order of the project plan; in particular, the desktop shell cannot be built before the protocol it displays, and the runtime cannot be built before the governor it calls.

A phase's pull request can be large. The per-step reviews recorded on it as steps land are what keep the final review tractable, and merging with a merge commit rather than a squash keeps the task-level history that those reviews refer to.

The project plan is a living document with a single owner. It is edited through pull requests like everything else, and each edit says which decision changed and why.
