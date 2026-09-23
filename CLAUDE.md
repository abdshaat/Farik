# Farik: instructions for Claude Code sessions

Farik is an operating system for small teams of AI agents with a governance harness at its core. Read `docs/SPEC.md` before touching anything; section 5 is the product.

## Standards (mandatory)

- Workflow: `docs/standards/workflow.md`. Brainstorm, plan, execute under TDD, verify, review, finish. In that order.
- Code: `docs/standards/code.md`. Naming (branches, commits, files, identifiers, wire formats, events), style, and the toolchain.
- Decisions: `docs/decisions/`. Read the existing ADRs before proposing a change that touches architecture, tooling, or process. Add one when you make such a change.

If the superpowers plugin is installed, its skills implement this workflow; use them, except `writing-plans`: write a step plan by copying `docs/plans/step-template.md` (ADR 0008, ADR 0010). If the plugin is not installed, follow the workflow document by hand. Either way the rules below hold.

## Hard rules

1. No production code before a failing test. Watch the test fail for the right reason. Code written before its test gets deleted, not adapted.
2. No completion claim without fresh evidence. Run the check, read the output, paste it. "Should work" is not a status.
3. Planning is two-level: `docs/plans/project-plan.md` holds phases and steps; each step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, copied from `docs/plans/step-template.md` and passed by one readiness review (`workflow.md` stage 2) before execution. Tick its checkboxes in the same commits.
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

Phases 0 to 2 are merged (pull requests #4, #5, #6): the workspace and check, `farik-core`'s decisions for `docs/SPEC.md` section 5, and the event log, store, git adapter and first eleven `farik` commands. Three section 5 rules await a decision in the spec, not a function in `core`; `docs/plans/project-plan.md` names them. Phase 2's steps 08 and 09 had their landing review after the merge; its findings were closed in pull request #12. Phase 3 (runtime and Milestone 0) is next; the old step plans are in history (`git show a191001:docs/plans/`), useful for what they decided, not as a format.
