# Code: naming, style, and tooling

One convention per kind of thing, decided once, and a toolchain chosen so that style is never a review comment. If a formatter or linter can decide something, the tool decides it and a human never mentions it. When two naming rows could apply, the more specific one wins. Anything not listed follows the nearest listed row; if nothing is near, add a row through a pull request rather than inventing locally. ADR 0002 records the tool choices.

## Toolchain

| Concern | Tool | Notes |
|---|---|---|
| Language | TypeScript, `strict: true`, `noUncheckedIndexedAccess: true`, `exactOptionalPropertyTypes: true` | No `any` outside a `// reason:` comment. No `@ts-ignore`; `@ts-expect-error` with a reason is allowed. |
| Package manager | pnpm with workspaces | Lockfile committed. `pnpm install --frozen-lockfile` in CI. |
| Runtime | Node, current LTS, pinned in `.nvmrc` and `package.json` `engines` | |
| Lint and format | Biome | One tool, one config at the root. Format on save. Lint rules are errors, not warnings; a warning nobody fixes is noise. |
| Tests | Vitest | Co-located unit tests. Coverage is reported, not gated; a coverage gate rewards bad tests. |
| Schema validation | JSON Schema 2020-12 at the wire boundary; TypeScript types generated from the schemas, never hand-written twice | The schema in `docs/schemas/` is the source of truth. |
| Commit hooks | lefthook | Pre-commit runs format and lint on staged files; commit-msg checks the Conventional Commits format. Hooks are a convenience; CI is the enforcement. |
| Versioning | Changesets | Every user-visible change adds a changeset file. |
| Continuous integration | GitHub Actions | One workflow, `check`, runs `pnpm check` on every pull request and on `main`. |

`pnpm check` is the single command that means "is this mergeable". It runs, in order: typecheck, lint, format check, unit tests. It exists from the first scaffold commit onward and it is never allowed to be red on `main`.

## Naming

### Repository and git

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

### Documents

| Thing | Convention | Example |
|---|---|---|
| Plan | `docs/plans/YYYY-MM-DD-<feature-kebab>.md` | `docs/plans/2026-09-21-governor-state-machine.md` |
| Architecture decision record | `docs/decisions/NNNN-<title-kebab>.md`, four-digit, never reused | `docs/decisions/0001-adopt-superpowers-workflow.md` |
| Standard | `docs/standards/<topic>.md`, lower-case | `docs/standards/code.md` |
| Top-level project documents | `UPPER_CASE.md` at the level they describe | `README.md`, `CONTRIBUTING.md`, `docs/SPEC.md` |
| Schema | `docs/schemas/<subject>.schema.json` | `docs/schemas/task-contract.schema.json` |
| Headings | sentence case | "Definition of Ready", not "Definition Of Ready" |

### Packages and files

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

### TypeScript identifiers

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

### Wire and file formats

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

### Agents, roles, and skills

| Thing | Convention | Example |
|---|---|---|
| Role directory | `roles/<role_id>/` with `role.yaml`, `system.md`, `skills/` | `roles/product_manager/` |
| Skill directory | `kebab-case` verb phrase, containing `SKILL.md` | `skills/writing-task-contracts/SKILL.md` |
| Skill name in frontmatter | same as the directory | |
| Agent display name | free text, chosen by the user | |
| Agent id | `kebab-case` slug of the display name, unique within a team | `maya-chen` |
| Team-wide file | under `.farik/team/` | `.farik/team/retro.md` |
| Per-agent file | under `.farik/agents/<agent_id>/` | `.farik/agents/maya-chen/memory.md` |

### Tests

| Thing | Convention | Example |
|---|---|---|
| Test description | `describe` names the unit; `it` states the behavior in plain words starting with a verb | `it('refuses a transition to ready when no exit criteria exist')` |
| Fixture | `fixtures/<subject>.ts` exporting builder functions, not raw objects | `fixtures/contract.ts` exporting `aReadyContract()` |
| Snapshot | avoided; assert on specific fields | |

## Rules the tools cannot enforce

Prefer plain functions and data over classes. A class is fine when it holds state with invariants (the state machine, the governor); it is not fine as a namespace for functions.

`packages/core` performs no input or output. No file system, no network, no clock, no randomness, no environment variables. Anything it needs from the world is passed in. This is what makes it exhaustively testable and what makes the governance layer auditable. A pull request that adds an import of `node:fs` to `core` is rejected without discussion.

Errors are values at package boundaries. Inside a package, throwing is fine for programmer errors (a violated invariant). Across packages, functions return a `Result`. The reason: the governor's refusals are normal outcomes that agents and the UI must handle, not exceptions.

Dependencies are added reluctantly. Before adding one, check that it is maintained, that its license is compatible with Apache 2.0, and that the standard library or an existing dependency cannot do the job. Record the reason in the pull request.

Comments explain why, never what. If the what needs a comment, rename or restructure until it does not. A `TODO` must carry a task id or an issue link; a bare `TODO` fails lint.

Feature flags are not used in the first release. Unfinished work stays on its branch.

## Testing rules

Unit tests test one unit through its public interface. They do not reach into private state, and they do not mock what they can construct.

Every transition in the governor's table has at least one test that exercises it and one that exercises its refusal. This is a standing requirement, not a suggestion; the table in `docs/SPEC.md` section 5.2 is the checklist.

Tests never depend on wall-clock time, on the network, or on execution order. Time is injected. Anything that talks to a model API is behind an adapter, and the adapter has a recorded-response fake for tests.

A failing test is never skipped, disabled, or quarantined to get green. It is fixed or, if it was wrong, deleted with a commit message that says why.

## Documentation rules

Public functions in `core`, `protocol`, and `runtime` carry a doc comment stating what they do and what they refuse. Everything else is documented by its name and its tests.

When behavior changes, `docs/SPEC.md` changes in the same pull request. The spec is not a historical document; it describes what the code does now.

## Things deliberately left open

Locale-specific naming in the UI (avatar packs, office themes) will be decided with the design system in Milestone 1. Names for the hosted tier's cloud resources are out of scope until Milestone 3.
