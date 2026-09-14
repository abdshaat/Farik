# Contributing to Farik

Thank you for considering it. Farik holds itself to the same discipline it imposes on the agent teams it runs, so the process is stricter than most open-source projects of its size. The standards are short; please read them before your first change.

1. [Workflow](docs/standards/workflow.md): how a change moves from idea to `main`. Brainstorm, plan, test-driven execution, verification with evidence, review, finish.
2. [Code](docs/standards/code.md): naming, style, and the toolchain.
3. [Decisions](docs/decisions/): architecture decision records. Read them before proposing a change to architecture, tooling, or process.

## If you use Claude Code

Install the [superpowers](https://github.com/obra/superpowers) plugin; its skills implement the workflow above and trigger on their own. The repository's `CLAUDE.md` carries the Farik-specific rules. Check the plugin's README for the current install command.

## If you work by hand

Follow `docs/standards/workflow.md` step by step. The project plan is `docs/plans/project-plan.md` and the step plan template is `docs/plans/step-template.md` and the ADR template in `docs/decisions/0000-template.md`. The pull request template asks for the evidence the workflow requires; a pull request without it will be sent back.

## Reporting a bug

Open an issue with a reproduction. A failing test is the best reproduction. If you can, follow the debugging section of the workflow document and include what you found about the root cause.

## Licensing

Contributions to everything outside `ee/` are under Apache 2.0. There is no `ee/` directory yet. When one exists, its license and contribution terms will be documented here first.
