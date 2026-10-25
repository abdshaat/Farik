# Phase <n>, step <nn>: <name>

Status: draft | ready | in progress | done | abandoned
Branch: `phase/<n>-<name-kebab>` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section <n>, F<n>
Depends on: none | phase <n> (merged in #<pr>) | step <nn> of this phase (committed as <sha>), ...
Readiness confirmed by: its author, <date>, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

One paragraph. What will be true when this step is done that is not true now, in terms a user of Praeco would recognize, and what is out of scope.

## Decisions

Every decision this step rests on, each with the alternative that was rejected and a one-line reason. A decision that affects more than this step is an ADR and is linked, not restated. This section is what the readiness review actually reads: an undecided question here is the one thing that sends a plan back.

- <decision>: chose X over Y because Z.
- <decision>: see ADR 000N.

## File map

Every file this step creates, modifies, or tests, one line each on what it is for. Draw this before writing tasks so that no two tasks touch one file for different reasons.

```
packages/core/src/cycle-transitions.ts         creates: the table from SPEC 5.2 as data
packages/core/src/cycle-transitions.test.ts    tests:   one per row, one per refusal
```

## Interfaces

This is where the no-forward-dependency rule is checked by reading: a step may consume only what an earlier step committed on this branch, or what an earlier phase merged to `main`. Consumed items are named with where they live; the compiler checks their signatures. Produced items are exact signatures.

Consumes: `validateJob` (`@praeco/core`, on main), `CycleStatus` (`@praeco/core`, on main)

Produces:

```ts
export interface TransitionRow { from: CycleStatus | null; to: CycleStatus; actor: Actor; gate: Gate }
export function rowsFrom(status: CycleStatus | null): readonly TransitionRow[];
```

## Tasks

Each task is the smallest unit that carries its own test cycle and its own commit, written under the red-green-refactor loop in `docs/standards/workflow.md` stage 3. The named tests are the contract the implementation has to satisfy: one per major functionality the task adds, not one per case (Catervas ADR 0060).

### Task 1: <name>

Files: created `...`, modified `...`, tested by `...`
Produces: `<signature>`
Consumes: nothing | `<name>` from Task <n> | `<name>` from `<path>`

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `<test_name>` — asserts that <the behavior, specifically enough that two people would write the same assertion>.
- `<test_name>` — asserts that <...>, which is the refusal in SPEC <n.n>.

- [ ] `feat(core): <subject>`

### Task 2: <name>

...

## Verification

The commands that prove the whole step is done, with their expected output. `pnpm check` at minimum.

```
pnpm check
# expected: check: ok
```
