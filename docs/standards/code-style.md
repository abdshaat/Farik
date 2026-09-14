# Code style and tooling

The tools are chosen so that style is never a review comment. If a formatter or linter can decide it, the tool decides it and a human never mentions it. ADR 0002 records the choices.

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
