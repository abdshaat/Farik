# Farik: instructions for Claude Code sessions

Farik is an operating system for small teams of AI agents with a governance harness at its core. Read `docs/SPEC.md` before touching anything; section 5 is the product.

## Standards (mandatory)

- Workflow: `docs/standards/workflow.md`. Brainstorm, plan, execute under TDD, verify, review, finish. In that order.
- Code: `docs/standards/code.md`. Naming (branches, commits, files, identifiers, wire formats, events), style, and the toolchain.
- Decisions: `docs/decisions/`. Read the existing ADRs before proposing a change that touches architecture, tooling, or process. Add one when you make such a change.

If the superpowers plugin is installed, its skills implement this workflow; use them. If it is not, follow the workflow document by hand. Either way the rules below hold.

## Hard rules

1. No production code before a failing test. Watch the test fail for the right reason. Code written before its test gets deleted, not adapted.
2. No completion claim without fresh evidence. Run the check, read the output, paste it. "Should work" is not a status.
3. Planning is two-level: `docs/plans/project-plan.md` holds phases and steps; each step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, written from `docs/plans/step-template.md` before execution. A plan is ready only when every decision is made, nothing is ambiguous, and it has no forward dependencies. Tick checkboxes as you go, in the same commits.
4. Commits follow Conventional Commits with a package scope. One task, one commit.
5. `crates/core` (`farik-core`) does no I/O. Ever. `cargo xtask core-io` checks it.
6. Wire and file formats use `snake_case`; Rust fields match them; TypeScript uses `camelCase`; one mapping layer per crate or package at the edge.
7. Event kinds are `<entity>.<past_tense_verb>`.
8. When behavior changes, `docs/SPEC.md` changes in the same pull request.
9. Never skip, disable, or quarantine a failing test to get green.
10. Do not accept your own work. A pull request is reviewed by someone, or by a fresh session, that did not write it.
11. One phase is one branch (`phase/<n>-<name>`) and one pull request to `main`. Open it as a draft when the phase's first step is pushed, without waiting to be asked; mark it ready when the last step's verification passes. Its description explains what changed and why it was necessary, and follows `.github/pull_request_template.md`. Never leave a pushed branch without a pull request. Work outside a phase (a standalone fix, a docs change) gets its own branch and pull request the same way.

## Commands

`cargo xtask check` is the full check (format, clippy, tests, generated-file freshness, bare-TODO check, the core no-I/O check; plus the front end's `pnpm check` once it exists) once the workspace is scaffolded. Until the scaffold exists there is no check command; say so in any verification section rather than implying one ran.

## Current state

Phase 0 (foundation) is done and merged (pull request #4): the Cargo workspace, `cargo xtask check`, the contract types generated from the schema, and `validate_contract` exist.

Phase 1 (harness core) is complete on `claude/phase-0-implementation-izm38y` (the harness-assigned branch, reused for phase 1 because a session may not push to another branch without permission) and awaits review in pull request #5. All nine step plans under `docs/plans/phase-1-harness/` are `done`, each confirmed ready by a fresh session before execution and each landing reviewed by another; `farik-core` decides every rule of `docs/SPEC.md` section 5 that is a decision rather than an effect, and `cargo xtask check` passes. Three rules of section 5 are recorded in `docs/plans/project-plan.md` as needing a decision in the spec rather than a function in `core`; read that note before adding one of them by hand.

Phase 2 (protocol, store, and the first command line) is the next phase, and its step plans are written one at a time under `docs/plans/phase-2-*/` from `docs/plans/project-plan.md` once pull request #5 merges.
