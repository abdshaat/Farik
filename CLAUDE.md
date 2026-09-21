# Farik: instructions for Claude Code sessions

Farik is an operating system for small teams of AI agents with a governance harness at its core. Read `docs/SPEC.md` before touching anything; section 5 is the product.

## Standards (mandatory)

- Workflow: `docs/standards/workflow.md`. Brainstorm, plan, execute under TDD, verify, review, finish. In that order.
- Code: `docs/standards/code.md`. Naming (branches, commits, files, identifiers, wire formats, events), style, and the toolchain.
- Decisions: `docs/decisions/`. Read the existing ADRs before proposing a change that touches architecture, tooling, or process. Add one when you make such a change.

If the superpowers plugin is installed, its skills implement this workflow; use them, with one exception: its planning skill writes the implementation into the plan and hard rule 3 does not, so edit a plan it drafts down to decisions, signatures, the file map and the test list before review (ADR 0008). If the plugin is not installed, follow the workflow document by hand. Either way the rules below hold.

## Hard rules

1. No production code before a failing test. Watch the test fail for the right reason. Code written before its test gets deleted, not adapted.
2. No completion claim without fresh evidence. Run the check, read the output, paste it. "Should work" is not a status.
3. Planning is two-level: `docs/plans/project-plan.md` holds phases and steps; each step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, written from `docs/plans/step-template.md` before execution. A plan is ready only when every decision is made, nothing is ambiguous, and it has no forward dependencies. It carries decisions, signatures, the file map, and a named test list saying what each test asserts; it does not carry function bodies, because the implementation is written once, in `.rs`, where the compiler reads it (ADR 0008). Readiness review is one round, ambiguity inside an implementation is the type checker's job rather than a reviewer's, and a plan past roughly 300 lines wants splitting. Tick checkboxes as you go, in the same commits.
4. Commits follow Conventional Commits with a package scope. One task, one commit.
5. `crates/core` (`farik-core`) does no I/O. Ever. `cargo xtask core-io` checks it.
6. Wire and file formats use `snake_case`; Rust fields match them; TypeScript uses `camelCase`; one mapping layer per crate or package at the edge.
7. Event kinds are `<entity>.<past_tense_verb>`.
8. When behavior changes, `docs/SPEC.md` changes in the same pull request.
9. Never skip, disable, or quarantine a failing test to get green.
10. Do not accept your own work. A pull request is reviewed by someone, or by a fresh session, that did not write it. Every step gets a landing review of its running code as it lands, and its bar is mutation: re-introduce the bug each test claims to catch and confirm the suite notices.
11. One phase is one branch (`phase/<n>-<name>`) and one pull request to `main`. Open it as a draft when the phase's first step is pushed, without waiting to be asked; mark it ready when the last step's verification passes. Its description explains what changed and why it was necessary, and follows `.github/pull_request_template.md`. Never leave a pushed branch without a pull request. Work outside a phase (a standalone fix, a docs change) gets its own branch and pull request the same way.

## Commands

`cargo xtask check` is the full check (format, clippy, tests, bare-TODO check, the core no-I/O check; plus the front end's `pnpm check` once it exists) once the workspace is scaffolded. Until the scaffold exists there is no check command; say so in any verification section rather than implying one ran.

## Current state

Phase 0 (foundation) is done and merged (pull request #4): the Cargo workspace, `cargo xtask check`, the contract types generated from the schema, and `validate_contract` exist.

Phase 1 (harness core) is done and merged (pull request #5): `farik-core` decides every rule of `docs/SPEC.md` section 5 that is a decision rather than an effect. Three rules of section 5 are recorded in `docs/plans/project-plan.md` as needing a decision in the spec rather than a function in `core`; read that note before adding one of them by hand.

Phase 2 (protocol, store, and the first command line) is done and merged (pull request #6): the event log, projections, the git adapter, the `.farik/` files, the project scan, reconciliation, and eleven `farik` commands. Two of its steps carry a gap the pull request records: step 08's landing review was cut off by a rate limit and step 09 was not reviewed at all.

Phase 3 (runtime and Milestone 0) is next and is the first phase planned under ADR 0008. The step plans of phases 0 to 2 were written under the old template, ten to fifty times the size the new one asks for, and are deleted: what they decided lives in the code, the ADRs and `docs/plans/project-plan.md`, and the plans themselves are in history (`git show a191001:docs/plans/`).
