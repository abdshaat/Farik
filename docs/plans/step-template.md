# Phase <n>, step <nn>: <name>

Status: draft | ready | in progress | done | abandoned
Branch: `phase/<n>-<name-kebab>` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section <n>, F<n>
Depends on: none | phase <n> (merged in #<pr>) | step <nn> of this phase (committed as <sha>), ...
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

One paragraph. What will be true when this step is done that is not true now, in terms a user of Farik would recognize, and what is out of scope.

## Decisions

Every decision this step rests on, each with the alternative that was rejected and a one-line reason. A decision that affects more than this step is an ADR and is linked, not restated. This section is what the readiness review actually reads: an undecided question here is the one thing that sends a plan back.

- <decision>: chose X over Y because Z.
- <decision>: see ADR 000N.

## File map

Every file this step creates, modifies, or tests, one line each on what it is for. Draw this before writing tasks so that no two tasks touch one file for different reasons.

```
crates/core/src/governor/transition_table.rs       creates: the table from SPEC 5.2 as data
crates/core/src/governor/transition_table.rs       tests:   one per row, one per refusal (in `mod tests`)
```

## Interfaces

This is where the no-forward-dependency rule is checked by reading: a step may consume only what an earlier step committed on this branch, or what an earlier phase merged to `main`. Consumed items are named with where they live; the compiler checks their signatures. Produced items are exact signatures.

Consumes: `validate_contract` (`farik-core`, on main), `TaskStatus` (`farik-core`, on main)

Produces:

```rust
pub struct TransitionRow { pub from: Status, pub to: TaskStatus, pub actor: Actor, pub gate: Gate }
pub fn rows_from(status: TaskStatus) -> &'static [TransitionRow];
```

## Tasks

Each task is the smallest unit that carries its own test cycle and its own commit, written under the red-green-refactor loop in `docs/standards/workflow.md` stage 3. The named tests are the contract the implementation has to satisfy, and what the landing reviewer checks the tests against.

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

The commands that prove the whole step is done, with their expected output. `cargo xtask check` at minimum.

```
cargo xtask check
# expected: xtask check: ok
```
