# Phase <n>, step <nn>: <name>

Status: draft | ready | in progress | done | abandoned
Branch: `phase/<n>-<name-kebab>` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section <n>, F<n>
Depends on: none | phase <n> (merged in #<pr>) | step <nn> of this phase (committed as <sha>), ...

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: <name>, <date>

## Goal

One paragraph. What will be true when this step is done that is not true now, in terms a user of Farik would recognize.

## Decisions

Every decision this step rests on, each with the alternative that was rejected and a one-line reason. Decisions that affect more than this step link an ADR instead of being restated.

- <decision>: chose X over Y because Z.
- <decision>: see ADR 000N.

## Design

The outcome of brainstorming. Two to twenty lines. Include what is out of scope for this step.

## Architecture notes

Which packages are touched, which boundaries are crossed, which existing interfaces are consumed and where they live on `main` today.

## Global constraints

Rules that apply to every task below (for example: `core` does no I/O; all events use `<entity>.<past_tense_verb>`).

## File map

Every file this step creates, modifies, or tests, with one line each on what it is for. Draw this before writing tasks so no two tasks touch the same file for different reasons.

```
packages/core/src/governor/transition-table.ts      creates: the table from SPEC 5.2 as data
packages/core/src/governor/transition-table.test.ts creates: one test per row, one per refusal
```

## Tasks

Each task is the smallest unit with its own test cycle. Its checklist items take two to five minutes each. Write the actual code; do not write "add validation". Repeat code rather than referencing earlier tasks. Define every type and signature here. A task consumes only what earlier tasks in this plan produce, what earlier steps committed on the phase branch, or what is already on `main`.

### Task 1: <name>

Files: created `...`, modified `...`, tested by `...`

Consumes: nothing | `<signature>` from Task <n> | `<signature>` from `<path on main>`
Produces: `<signature>`

- [ ] Write the failing test:

  ```ts
  // exact test code
  ```

- [ ] Run it and confirm it fails because the behavior is missing:

  ```
  pnpm --filter @farik/core test transition-table
  # expected: FAIL ... "evaluate is not a function" (or the specific assertion)
  ```

- [ ] Write the minimal implementation:

  ```ts
  // exact implementation
  ```

- [ ] Run the test and the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected: all passing
  ```

- [ ] Refactor if there is duplication; keep green.
- [ ] Commit: `feat(core): <subject>`

### Task 2: <name>

...

## Verification

The commands that prove the whole step is done, with expected output. `pnpm check` at minimum.

## Open questions

Must read "none" before the plan is marked ready. If it is not empty, the plan is not ready to execute.
