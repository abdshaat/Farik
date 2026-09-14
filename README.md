# Farik

An open-source operating system for small teams of AI agents, with a pixel-art office on the front and a strict governance layer underneath.

You assemble three to seven agents, give each a name, an avatar, and a role (Product Manager, Scrum Master, Architect, Software Developer, Marketing Specialist), and point them at a repository. The Product Manager writes contracts with explicit exit criteria before any work starts. A deterministic governor enforces budgets, permissions, and the rule that nobody accepts their own work. You can watch the team in the office, talk to any agent, or read along in the team channel.

Status: specification stage. Nothing runs yet. Project standards are in place; see [CONTRIBUTING.md](CONTRIBUTING.md) before making a change.

## Documents

- [Specification](docs/SPEC.md): goals, the harness (task lifecycle, definitions of ready and done, budgets, permissions), launch roles, architecture, open-source and premium split, milestones.
- [Product analysis](docs/PRODUCT_ANALYSIS.md): the problem, the competitive landscape, where the idea is weak, business model, go-to-market, what to measure.
- [Task contract schema](docs/schemas/task-contract.schema.json): the JSON Schema for the document that makes a task ready.

## Standards

- [Contributing](CONTRIBUTING.md): the entry point.
- [Workflow](docs/standards/workflow.md), [code: naming, style, and tooling](docs/standards/code.md).
- [Decisions](docs/decisions/): architecture decision records, starting with why this workflow was adopted.
- [Plans](docs/plans/): one plan per change, written before the code.
