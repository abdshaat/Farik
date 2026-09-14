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
3. Plans live in `docs/plans/YYYY-MM-DD-<feature>.md` and are written before execution, from the template in that directory. Tick checkboxes as you go, in the same commits.
4. Commits follow Conventional Commits with a package scope. One task, one commit.
5. `packages/core` does no I/O. Ever.
6. Wire and file formats use `snake_case`; TypeScript uses `camelCase`; one mapping layer per package at the edge.
7. Event kinds are `<entity>.<past_tense_verb>`.
8. When behavior changes, `docs/SPEC.md` changes in the same pull request.
9. Never skip, disable, or quarantine a failing test to get green.
10. Do not accept your own work. A pull request is reviewed by someone, or by a fresh session, that did not write it.
11. When the work you were asked to do is complete and pushed, open a pull request to `main` without waiting to be asked. Its description explains what changed and why it was necessary, and follows `.github/pull_request_template.md`. Never leave a pushed branch without a pull request.

## Commands

`pnpm check` is the full check (typecheck, lint, format, tests) once the monorepo is scaffolded. Until the scaffold exists there is no check command; say so in any verification section rather than implying one ran.

## Current state

Specification stage. The first code change is the monorepo scaffold, and it goes through the full workflow like everything else: plan first, in `docs/plans/`.
