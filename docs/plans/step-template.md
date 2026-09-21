# Phase <n>, step <nn>: <name>

Status: draft | ready | in progress | done | abandoned
Branch: `phase/<n>-<name-kebab>` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section <n>, F<n>
Depends on: none | phase <n> (merged in #<pr>) | step <nn> of this phase (committed as <sha>), ...

A plan is `ready` when a reviewer other than its author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. That review is one round. Record who confirmed and when here.

Readiness confirmed by: <name>, <date>

## What a plan is, and is not

A plan states what has been decided and what the boundaries are. It does not contain the implementation. Function bodies belong in `.rs` files, where the type checker and the test suite read them; a plan that carries them is the codebase written twice, and the second copy is the one nobody can compile. ADR 0008 records why, with the measurements.

So: signatures yes, bodies no. Test names and what each asserts yes, test code no. Exact file paths yes. Expected command output yes, because that is the verification and nothing else checks it.

A plan that runs past roughly 300 lines is usually a step that wants splitting, or a plan that has started writing the code. Check which before adding to it.

## Goal

One paragraph. What will be true when this step is done that is not true now, in terms a user of Farik would recognize.

## Decisions

Every decision this step rests on, each with the alternative that was rejected and a one-line reason. A decision that affects more than this step is an ADR and is linked, not restated. This section is what the readiness review actually reads: an undecided question here is the one thing that sends a plan back.

- <decision>: chose X over Y because Z.
- <decision>: see ADR 000N.

## Design

The outcome of brainstorming. Two to twenty lines. Include what is out of scope for this step.

## Architecture notes

Which crates and packages are touched, which boundaries are crossed, which existing interfaces are consumed and where they live on `main` today.

## Global constraints

Rules that apply to every task below (for example: `core` does no I/O; all events use `<entity>.<past_tense_verb>`; every fallible function crossing a crate boundary returns `Result<T, E>` with the crate's error enum).

## File map

Every file this step creates, modifies, or tests, one line each on what it is for. Draw this before writing tasks so that no two tasks touch one file for different reasons.

```
crates/core/src/governor/transition_table.rs       creates: the table from SPEC 5.2 as data
crates/core/src/governor/transition_table.rs       tests:   one per row, one per refusal (in `mod tests`)
```

## Interfaces

What this step consumes and what it produces, as exact signatures. This is where the no-forward-dependency rule is checked by reading, so it is the one section that has to be complete: a step may consume only what an earlier step committed on this branch, or what an earlier phase merged to `main`.

Consumes:

```rust
pub fn validate_contract(input: &serde_json::Value) -> Result<TaskContract, Vec<ValidationError>>  // farik-core, on main
```

Produces:

```rust
pub struct TransitionRow { pub from: Status, pub to: TaskStatus, pub actor: Actor, pub gate: Gate }
pub fn rows_from(status: TaskStatus) -> &'static [TransitionRow];
```

## Tasks

Each task is the smallest unit that carries its own test cycle and its own commit. A task names its files, its signatures, and its tests; the code is written during execution, under the red-green-refactor loop in `docs/standards/workflow.md` stage 3.

Name each test and say what it asserts. That list is the contract the implementation has to satisfy, and it is what a landing reviewer checks the tests against — a test that exists but asserts nothing the plan asked for is the defect that a green check hides.

### Task 1: <name>

Files: created `...`, modified `...`, tested by `...`
Produces: `<signature>`
Consumes: nothing | `<signature>` from Task <n> | `<signature>` from `<path>`

Tests, each written and watched to fail before the code that satisfies it:

- `<test_name>` — asserts that <the behavior, specifically enough that two people would write the same assertion>.
- `<test_name>` — asserts that <...>, which is the refusal in SPEC <n.n>.

- [ ] Tests above written, run, and each watched to fail for the stated reason (not a typo, not a missing import)
- [ ] Minimal implementation written; the tests and the rest of the suite pass
- [ ] Refactored if there is duplication; suite still green
- [ ] Commit: `feat(core): <subject>`

### Task 2: <name>

...

## Verification

The commands that prove the whole step is done, with their expected output. `cargo xtask check` at minimum.

```
cargo xtask check
# expected: xtask check: ok
```

## Open questions

Must read "none" before the plan is marked ready. If it is not empty, the plan is not ready to execute.
