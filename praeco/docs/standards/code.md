# Code: naming, style, and tooling

One convention per kind of thing, decided once, and a toolchain chosen so that style is never a review comment. If a formatter or linter can decide something, the tool decides it. When two rows could apply, the more specific one wins; anything not listed follows the nearest row, and a missing row is added by pull request. This is Catervas's `docs/standards/code.md` with its Rust rows dropped and its TypeScript rows made the whole of it; ADR 0003 records the tool choices.

## Toolchain

| Concern | Choice | Notes |
|---|---|---|
| Language | TypeScript 7.0.2, `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, ES modules only | No `any` outside a `// reason:` comment; no `@ts-ignore`; no non-null `!` outside tests. |
| Runtime | Node.js 24.14.0 (`.node-version`) | Scripts in `scripts/` are `.ts` run by Node's type stripping, with no build step. |
| Package manager | pnpm 12.3.4 workspace; lockfile committed; `pnpm install --frozen-lockfile` in CI | Every dependency an exact version. |
| Lint and format | Biome 2.5.14, one `biome.json` at the root | Warnings fail the check. |
| Tests | Vitest 5.0.2, co-located `*.test.ts`; integration tests `*.integration.test.ts`, run only by `pnpm check --integration`; live tests `*.live.test.ts`, run by hand with `PRAECO_LIVE_TESTS=1`, never in CI | Coverage reported, not gated. |
| Schemas | JSON Schema 2020-12 in `docs/schemas/`; `pnpm generate` writes types (`json-schema-to-typescript`) and standalone Ajv validators into `packages/schemas/src/generated/`, committed | `pnpm check` fails when a generated file is stale. |
| Database | Postgres 18; Kysely; migrations `packages/store/migrations/<nnnn>_<subject>.sql`, applied in order, never edited once landed | |
| Repository tasks | `scripts/check.ts` (`pnpm check [--integration]`), `scripts/generate.ts` (`pnpm generate [--check]`), `scripts/todos.ts`, `scripts/core-io.ts`, `scripts/commit-msg.ts`, `scripts/install-hooks.ts` | |
| Commit hooks | `pnpm hooks` writes `pre-commit` (format check, bare-TODO check) and `commit-msg` | CI is the enforcement. |
| Continuous integration | GitHub Actions; one workflow, `check`, runs `pnpm check --integration` on every pull request and on `main` | |
| Container | `Dockerfile` at the root builds one image with `praeco` and the web app | |

`pnpm check` runs, in order: `generate --check`, `biome check .`, `tsc -p tsconfig.json` (no emit: Node runs the TypeScript as written), `vitest run --project unit`, the bare-TODO check, the core no-I/O check. It is never red on `main`.

## Naming

### Repository and git

| Thing | Convention | Example |
|---|---|---|
| Phase branch | `phase/<n>-<name-kebab>` | `phase/1-the-loop` |
| Other branch | `<type>/<short-kebab>` | `fix/budget-rounding` |
| Commit | Conventional Commits, `<type>(<scope>): <imperative subject>`, lower-case, no period, under 72 characters | `feat(core): decide tool calls by tier` |
| Commit type | `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`, `ci`, `perf` | |
| Commit scope | a package name without `@praeco/` (`core`, `schemas`, `store`, `agent`, `tools`, `slack`, `web`), `praeco` for the app, or `docs`, `repo`, `scripts`, `skills`, `ee` | |
| Pull request title | `phase <n>: <name>`, else a commit subject | `phase 0: foundation` |
| Merge | merge commit, never squash | |
| Tag | `v<semver>` | `v0.1.0` |

### Documents

As Catervas: `docs/plans/project-plan.md`; `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`; `docs/decisions/NNNN-<title>.md`; `docs/standards/<topic>.md`; `docs/schemas/<subject>.schema.json`; headings in sentence case.

### Packages and files

| Thing | Convention | Example |
|---|---|---|
| Package | `@praeco/<name>` in `packages/<name>`; the program `praeco` in `apps/praeco`; the web app `@praeco/web` in `apps/web` | |
| Source file | `kebab-case.ts`, one primary export, named after it | `decide-tool-call.ts` exports `decideToolCall` |
| Barrel | `index.ts` at the package root only | |
| React component | `PascalCase.tsx`, styles `<Component>.module.css` | |
| Fixture | `src/fixtures/<subject>.ts` exporting builders, exported from the package's `fixtures` entry for other packages' tests | `aResearchJob()` |
| Agent skill | `agents/praeco/skills/<kebab-verb-phrase>/SKILL.md` | `skills/researching-the-audience/SKILL.md` |

### TypeScript identifiers

| Thing | Convention | Example |
|---|---|---|
| Type, interface, union | `PascalCase`, no prefixes or `Type` suffix | `JobContract` |
| Function, variable, property | `camelCase` | `nextJob` |
| Constant | `UPPER_SNAKE_CASE` for literal tables and numbers | `TOOL_RULES`, `MAX_QUOTE_CHARS` |
| Boolean | `is`, `has`, `can`, `should` | `isPaused` |
| Enum-like values | string literal unions, values `snake_case` like the wire | `type CycleStatus = 'queued' \| 'running' \| ...` |
| Function that may fail | returns `Result<T, E>` from `@praeco/core` (`{ ok: true, value } \| { ok: false, error }`); never throws across a package boundary; a promise of one is `Promise<Result<T, E>>` | |
| Normal refusal | a value named `<Subject>Refusal`, in the `error` of a `Result` | `ToolRefusal` |
| Error | `<Subject>Error`, a union of `{ kind: '<reason>', ... }` objects | `StoreError` |

### Wire, files and the database

| Thing | Convention | Example |
|---|---|---|
| JSON keys, enum values | `snake_case` | `allowed_tools`, `in_progress` |
| Event kind | `<entity>.<past_tense_verb>` | `cycle.stopped`, `tool.refused` |
| Tool name (offered to the agent) | `snake_case` verb phrase | `record_finding` |
| Job number | `J-<n>` per workspace | `J-42` |
| Environment variable | `PRAECO_` + `UPPER_SNAKE_CASE` | `PRAECO_DATABASE_URL` |
| Table | `snake_case`, plural | `events`, `findings` |
| Column | `snake_case`; `<entity>_id`; `<verb>_at`; money `<what>_usd_micros` | `limit_usd_micros` |
| Index, trigger, policy | `<table>_by_<column>`, `<table>_<what_it_does>`, `<table>_in_workspace` | `events_refuse_update` |

TypeScript is `camelCase`; the mapping is done in `@praeco/schemas` (`parse`, `toWire`) and in `@praeco/store` (Kysely's `CamelCasePlugin`), nowhere else.

### Tests

`describe` names the unit; `it` states the behavior in plain words starting with a verb (`it('refuses a record with no source')`). Builders, not raw objects. No snapshots.

## Rules the tools cannot enforce

`@praeco/core` performs no I/O: no file system, network, process, environment, clock, randomness or database, and no package that does them. `scripts/core-io.ts` fails the check when `packages/core/src` imports `node:fs`, `node:net`, `node:http`, `node:https`, `node:child_process`, `node:process`, `node:crypto`, `pg`, `kysely`, `pg-boss`, `@anthropic-ai/sdk`, `@slack/bolt`, `@notionhq/client` or `undici`, or mentions `Date.now`, `new Date()` with no argument, `Math.random` or `process.env`.

Errors are values across package boundaries. Plain functions and data over classes; a class only for state with invariants (a session runner, a store).

Dependencies are added reluctantly, with the reason in the pull request. Comments explain why. A `TODO` carries an issue, `(#<n>)`, or a link, `(<https link>)`; a bare one fails the check. (Job numbers are the product's, not the repository's, so `J-<n>` does not count.)

Feature flags are not used before `v0.1.0`.

## Testing rules

One test per major functionality; a security guarantee counts as one (Catervas ADR 0060). Time and ids are injected (`Clock`, `Ids` from `@praeco/core`). Nothing in a unit test touches the network, Postgres or the clock. Anything that talks to Anthropic, Slack, Notion, Google, GitHub or Exa is behind an interface with a fake for tests; the Managed Agents fake replays events recorded from live sessions. A failing test is never skipped, disabled or quarantined.

## Documentation rules

Every export of `@praeco/core`, `@praeco/store` and `@praeco/agent` has a TSDoc comment saying what it does and what it refuses. When behavior changes, `docs/SPEC.md` changes in the same pull request.
