# Naming

One convention per kind of thing, decided once. When two conventions could apply, the more specific row wins. Anything not listed here follows the nearest listed row; if nothing is near, add a row through a pull request rather than inventing locally.

## Repository and git

| Thing | Convention | Example |
|---|---|---|
| Default branch | `main` | |
| Working branch | `<type>/<short-kebab-description>`; type from the commit types below | `feat/governor-transition-table`, `fix/budget-rounding`, `docs/naming-standard` |
| Harness-assigned branch | left as assigned (e.g. `claude/...`); the pull request title carries the type | |
| Commit message | Conventional Commits: `<type>(<scope>): <imperative subject>`, subject lower-case, no trailing period, under 72 characters; body explains why, not what | `feat(core): add definition-of-ready structural checks` |
| Commit type | one of `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`, `ci`, `perf` | |
| Commit scope | a package name without the `@farik/` prefix, or `docs`, `repo`, `roles`, `skills` | `core`, `store`, `runtime`, `ui`, `desktop`, `web`, `protocol` |
| Pull request title | same format as a commit subject; becomes the squash commit | |
| Tag | `v<semver>` | `v0.1.0` |

## Documents

| Thing | Convention | Example |
|---|---|---|
| Plan | `docs/plans/YYYY-MM-DD-<feature-kebab>.md` | `docs/plans/2026-09-21-governor-state-machine.md` |
| Architecture decision record | `docs/decisions/NNNN-<title-kebab>.md`, four-digit, never reused | `docs/decisions/0001-adopt-superpowers-workflow.md` |
| Standard | `docs/standards/<topic>.md`, lower-case | `docs/standards/naming.md` |
| Top-level project documents | `UPPER_CASE.md` at the level they describe | `README.md`, `CONTRIBUTING.md`, `docs/SPEC.md` |
| Schema | `docs/schemas/<subject>.schema.json` | `docs/schemas/task-contract.schema.json` |
| Headings | sentence case | "Definition of Ready", not "Definition Of Ready" |

## Packages and files

| Thing | Convention | Example |
|---|---|---|
| Workspace package | `@farik/<name>`, directory `packages/<name>` or `apps/<name>` | `@farik/core` in `packages/core` |
| Source file | `kebab-case.ts`; one primary export per file, file named after it | `task-state-machine.ts` exports `TaskStateMachine` |
| Test file | co-located, same name plus `.test.ts` | `task-state-machine.test.ts` |
| Integration test | `<subject>.integration.test.ts`, under `tests/` in the package | `packages/runtime/tests/session.integration.test.ts` |
| Type-only file | `<subject>.types.ts` | `contract.types.ts` |
| Barrel | `index.ts`, only at the package root; no nested barrels | |
| React component | `PascalCase.tsx`, one component per file | `AgentDesk.tsx` |
| Component styles | co-located, `<Component>.module.css` | `AgentDesk.module.css` |
| Directory | `kebab-case` | `packages/core/src/state-machine/` |
| Asset | `kebab-case` with size suffix for sprites | `assets/avatars/pm-01-32.png` |

## TypeScript identifiers

| Thing | Convention | Example |
|---|---|---|
| Type, interface, class, enum-like union | `PascalCase`; no `I` prefix, no `T` prefix, no `Type` suffix | `TaskContract`, `Governor` |
| Function, method, variable, property | `camelCase` | `evaluateTransition` |
| Module-level constant | `UPPER_SNAKE_CASE` only for true constants (numbers, literal tables); `camelCase` for everything else | `DEFAULT_ITERATION_LIMIT`, `defaultBudgets` |
| Boolean | reads as a question: `is`, `has`, `can`, `should` | `isReady`, `hasReviewer` |
| Function that may fail | returns a `Result` type; never throws across a package boundary | `evaluate(): Result<Transition, GovernorError>` |
| Enums | not used; string literal unions instead, values `snake_case` to match the wire format | `type TaskStatus = 'draft' \| 'refining' \| ...` |
| Generic parameter | descriptive `PascalCase`, single letters only for the obvious (`T` in a container) | `Result<Value, Error>` |
| Private member | no underscore; use `private` or `#` | |
| Unused parameter | prefixed with `_` | `(_event, state) => ...` |

## Wire and file formats

Anything that leaves a process or is written to disk uses `snake_case` keys. TypeScript code uses `camelCase`. The boundary between them is one mapping layer per package, at the edge, never sprinkled through the code. The task contract schema already follows this; it is the reference.

| Thing | Convention | Example |
|---|---|---|
| JSON and YAML keys | `snake_case` | `exit_criteria`, `allowed_paths` |
| Status and enum values | `snake_case` | `in_progress` |
| Event kind | `<entity>.<past_tense_verb>` | `task.transitioned`, `tool.called`, `tool.denied`, `budget.exhausted` |
| Task id | `FRK-<n>` | `FRK-42` |
| Requirement id | `R<n>` within a contract | `R1` |
| Exit criterion id | `C<n>` within a contract | `C3` |
| Role id | `snake_case`, matches the schema enum | `product_manager` |
| Permission tier | `snake_case` | `write_workspace` |
| Environment variable | `FARIK_` prefix, `UPPER_SNAKE_CASE` | `FARIK_DAILY_BUDGET_USD` |
| Database table | `snake_case`, plural | `events`, `task_projections` |
| Database column | `snake_case`; foreign keys `<entity>_id`; timestamps `<verb>_at` | `task_id`, `created_at` |

## Agents, roles, and skills

| Thing | Convention | Example |
|---|---|---|
| Role directory | `roles/<role_id>/` with `role.yaml`, `system.md`, `skills/` | `roles/product_manager/` |
| Skill directory | `kebab-case` verb phrase, containing `SKILL.md` | `skills/writing-task-contracts/SKILL.md` |
| Skill name in frontmatter | same as the directory | |
| Agent display name | free text, chosen by the user | |
| Agent id | `kebab-case` slug of the display name, unique within a team | `maya-chen` |
| Team-wide file | under `.farik/team/` | `.farik/team/retro.md` |
| Per-agent file | under `.farik/agents/<agent_id>/` | `.farik/agents/maya-chen/memory.md` |

## Tests

| Thing | Convention | Example |
|---|---|---|
| Test description | `describe` names the unit; `it` states the behavior in plain words starting with a verb | `it('refuses a transition to ready when no exit criteria exist')` |
| Fixture | `fixtures/<subject>.ts` exporting builder functions, not raw objects | `fixtures/contract.ts` exporting `aReadyContract()` |
| Snapshot | avoided; assert on specific fields | |

## Things deliberately left open

Locale-specific naming in the UI (avatar packs, office themes) will be decided with the design system in Milestone 1. Names for the hosted tier's cloud resources are out of scope until Milestone 3.
