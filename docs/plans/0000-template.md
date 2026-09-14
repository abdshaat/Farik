# Plan: <feature name>

Date: YYYY-MM-DD
Spec: `docs/SPEC.md` section <n>, F<n>
Status: draft | in progress | done | abandoned
Branch: `<type>/<feature-kebab>`

## Goal

One paragraph. What will be true when this plan is done that is not true now, in terms a user of Farik would recognize.

## Design

The outcome of brainstorming. Two to twenty lines. Include what is out of scope.

## Architecture notes

Which packages are touched, which boundaries are crossed, which existing interfaces are consumed. Link ADRs.

## Tech stack for this change

Only what is new or unusual for this change. "Nothing beyond the standard toolchain" is a fine answer.

## Global constraints

Rules that apply to every task below (for example: `core` does no I/O; all events use `<entity>.<past_tense_verb>`).

## File map

Every file this plan creates, modifies, or tests, with one line each on what it is for. Draw this before writing tasks so no two tasks touch the same file for different reasons.

```
packages/core/src/governor/transition-table.ts      creates: the table from SPEC 5.2 as data
packages/core/src/governor/transition-table.test.ts creates: one test per row, one per refusal
```

## Tasks

Each task is the smallest unit with its own test cycle. Steps inside a task are two to five minutes. Write the actual code; do not write "add validation". Repeat code rather than referencing earlier tasks. Define every type and signature here.

### Task 1: <name>

Files: created `...`, modified `...`, tested by `...`

Consumes: nothing | `<signature>` from Task <n>
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

The commands that prove the whole plan is done, with expected output. `pnpm check` at minimum.

## Open questions

Anything unresolved that a task depends on. If this section is not empty, the plan is not ready to execute.
