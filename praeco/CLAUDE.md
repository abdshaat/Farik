# Praeco: instructions for Claude Code sessions

Praeco is a marketing agent that lives in a company's Slack and works constantly, in bounded cycles, on research, prospects and SEO, under a governor that decides every tool call in code. Read `docs/SPEC.md` before touching anything; section 5 is the product. "Praeco" is a working name.

## Standards (mandatory)

- Workflow: `docs/standards/workflow.md`. Brainstorm, plan, execute under TDD, verify, review, finish. In that order. It is Catervas's workflow (ADR 0001).
- Code: `docs/standards/code.md`. Naming, style, and the toolchain (ADR 0003).
- Models: Opus 5.5 plans (brainstorm, plans, mockups); Sonnet 5.5 executes the plan. No agent reviews a plan or code: the type check, the tests and the founder's own testing are the check.
- Decisions: `docs/decisions/`. Read the existing ADRs before proposing a change to architecture, tooling or process. Add one when you make such a change.

If the superpowers plugin is installed, its skills implement this workflow; use them, except `writing-plans`: write a step plan by copying `docs/plans/step-template.md`.

## Hard rules

1. No production code before a failing test. Watch it fail for the right reason. Code written before its test is deleted, not adapted. One test per major functionality; a security guarantee counts as one.
2. No completion claim without fresh evidence. Run the check, read the output, paste it.
3. Planning is two-level: `docs/plans/project-plan.md` holds phases and steps; each step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, copied from `docs/plans/step-template.md`, meeting the three readiness rules of `workflow.md` stage 2, checked by its author. Tick its checkboxes in the same commits.
4. Commits follow Conventional Commits with a package scope. One task, one commit.
5. `@praeco/core` does no I/O, reads no clock and makes no id. `scripts/core-io.ts` checks it.
6. Wire, file and database formats use `snake_case`; TypeScript uses `camelCase`; the mapping happens in `@praeco/schemas` and `@praeco/store` and nowhere else.
7. Event kinds are `<entity>.<past_tense_verb>`. Events carry ids and reasons, never a page's text or a prospect's words.
8. Every effect outside the agent's sandbox is a Praeco tool the governor decides. No outward act without an approval or an approved plan. Praeco never contacts a prospect.
9. When behavior changes, `docs/SPEC.md` changes in the same pull request.
10. Never skip, disable or quarantine a failing test to get green.
11. Do not accept your own work. The founder reviews and merges. A step lands when `pnpm check --integration` passes locally and the `check` workflow passes on GitHub.
12. One phase is one branch (`phase/<n>-<name>`) and one pull request to `main`, opened as a draft when the phase's first step is pushed and marked ready when the last step's verification passes, following `.github/pull_request_template.md`. Work outside a phase gets its own branch and pull request.

## Commands

`pnpm check` is the full check (generated files fresh, Biome, the type check, unit tests, the bare-TODO check, the core no-I/O check); `pnpm check --integration` adds the tests that need Docker and builds the image. They exist from phase 0 step 01; until then there is no check command, and a verification section says so.

## Current state

As of 2026-10-24: the plan is written (`docs/SPEC.md` 0.1, ADRs 0001 to 0013, `docs/plans/project-plan.md` revision 1, step plans for phases 0 and 1), every decision proposed and waiting for the founder's acceptance. No code exists. Phase 0 starts when the founder accepts the ADRs.
