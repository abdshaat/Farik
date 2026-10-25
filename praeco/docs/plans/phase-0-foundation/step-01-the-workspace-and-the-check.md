# Phase 0, step 01: The workspace and the check

Status: draft
Branch: `phase/0-foundation`
Spec: `docs/SPEC.md` 8.1; no functional requirement (the repository's own tooling)
Depends on: none (the repository holds only `docs/`)
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`pnpm check` exists and passes on an almost empty workspace: it runs Biome, the type check, the unit tests, the bare-TODO check and the core no-I/O check, in that order, stops at the first failure, and prints `check: ok` at the end. CI runs `pnpm check --integration` on every pull request and on `main`. `pnpm hooks` installs the two commit hooks. `@praeco/core` exists with `Result`, `Clock` and `Ids`, and the root carries the license, the README, the contributing guide, `CLAUDE.md` and the pull request template.

Out of scope: `pnpm generate` and its freshness check (step 02), anything with Postgres (step 03), the program and the image (step 04).

## Decisions

- **No build step.** TypeScript runs as written: `tsconfig.base.json` has `noEmit`, `allowImportingTsExtensions`, `erasableSyntaxOnly`, `verbatimModuleSyntax`, `module`/`moduleResolution` `nodenext`, `target` `es2024`, plus the three strict flags (copied from Catervas's `tsconfig.base.json`). Node runs `.ts` files by stripping types; relative imports end in `.ts`; each package's `package.json` has `"type": "module"` and `"exports": { ".": "./src/index.ts", "./fixtures": "./src/fixtures/index.ts" }`. Workspace packages resolve through pnpm's symlinks to their real paths outside `node_modules`, where Node strips types. Rejected: `tsc` emitting `dist/` per package, which adds a build to every test run and two copies of every file.
- **Type check** is one program: `tsc -p tsconfig.json` at the root, whose `include` is `packages/*/src`, `apps/*/src`, `scripts` and `vitest.config.ts`, extending `tsconfig.base.json`. Each package also has a `tsconfig.json` extending the base, for editors only. Rejected: project references and `tsc -b`, which want `composite` and declaration output that a no-emit workspace does not produce.
- **Pinned versions** (ADR 0003), installed together in Task 1 and recorded with the day the check ran green: `typescript` 7.0.2, `@biomejs/biome` 2.5.14, `vitest` 5.0.2, `@types/node` 24.19.0. Node 24.14.0 in `.node-version`; `"packageManager": "pnpm@12.3.4"`.
- **Vitest projects** in the root `vitest.config.ts`: `unit` includes `packages/*/src/**/*.test.ts`, `apps/*/src/**/*.test.ts` and `scripts/**/*.test.ts` and excludes `*.integration.test.ts` and `*.live.test.ts`; `integration` includes `**/*.integration.test.ts`; `live` includes `**/*.live.test.ts`. The `integration` and `live` projects set `passWithNoTests`. Rejected: one project filtered by environment variables, which hides which tests ran.
- **`scripts/check.ts`** runs, each as a child process with inherited output, stopping at the first non-zero exit: `biome check .`; `tsc -p tsconfig.json`; `vitest run --project unit` (plus `--project integration` when `--integration` is passed); then `todos` and `core-io` in-process. It prints `check: ok` and exits 0, or `check: failed at <stage>` and exits 1. `--pre-commit` runs only `biome format .` (check mode, no writes) and `todos`.
- **Bare TODOs:** `findBareTodos` scans tracked `.ts`, `.tsx`, `.css`, `.json` and `.sql` files (from `git ls-files`) for `TODO` or `FIXME` not followed by `(#<digits>)` or `(<https link>)`, skipping `scripts/todos.ts` and `scripts/todos.test.ts`, which describe the rule.
- **Core no-I/O:** `findCoreIo` scans `packages/core/src/**/*.ts` for an import or `import()` of any module in `CORE_FORBIDDEN_MODULES` (`node:fs`, `node:fs/promises`, `node:net`, `node:http`, `node:https`, `node:child_process`, `node:process`, `node:crypto`, `node:os`, `pg`, `kysely`, `@anthropic-ai/sdk`, `@slack/bolt`, `@notionhq/client`, `undici`, `fastify`, and every `@praeco/` package but `@praeco/schemas`), and for the text `Date.now(`, `new Date()`, `Math.random(` or `process.env`. Comments count: the rule is about the text, so a comment that needs one of these words says it differently.
- **Commit messages:** `checkCommitMessage` accepts `<type>(<scope>): <subject>` with a type from `docs/standards/code.md`, a kebab-case scope, a subject starting lower-case, no trailing period, the first line at most 72 characters; and accepts first lines starting `Merge ` or `Revert `. The scope is not checked against a list.
- **Hooks:** `scripts/install-hooks.ts` writes `.git/hooks/pre-commit` (`#!/bin/sh` and `exec node scripts/check.ts --pre-commit`) and `.git/hooks/commit-msg` (`exec node scripts/commit-msg.ts "$1"`), mode 755, overwriting.
- **CI** (`.github/workflows/check.yml`): one job on `ubuntu-latest`, on pull requests and on pushes to `main`: checkout, `pnpm/action-setup`, `actions/setup-node` with `node-version-file: .node-version` and pnpm cache, `pnpm install --frozen-lockfile`, `pnpm check --integration`. Action versions as Catervas's workflow pins them (`actions/checkout@v5`, `pnpm/action-setup@v4`, `actions/setup-node@v5`).
- **`@praeco/core`'s first exports:** `Result`, `ok`, `err`, `Clock`, `Ids`; fixtures `fixedClock`, `sequentialIds`. Doc comments: `Result`: "A value or the reason there is none; Praeco's functions return one instead of throwing across a package boundary." `Clock`: "The one way code below `apps/` learns the time." `Ids`: "The one way code below `apps/` gets a new id; ids are UUID version 7."
- **Root documents:** `LICENSE` is the Apache License 2.0 text with `Copyright 2026 the Praeco authors`; `README.md` and `CLAUDE.md` exist with this plan and are updated: the README's status says "pre-release; nothing to install yet" and its License line drops "(added by phase 0 step 01)", and `CLAUDE.md`'s Current state names phase 0 step 01 as landed; `CONTRIBUTING.md` points to `docs/standards/workflow.md` and `code.md`; `.github/pull_request_template.md` is Catervas's with `cargo xtask check` replaced by `pnpm check --integration` and Catervas's ADR numbers prefixed "Catervas".

## File map

```
package.json, pnpm-workspace.yaml, .node-version, .gitignore       creates: the workspace (Task 1)
tsconfig.base.json, tsconfig.json, biome.json, vitest.config.ts     creates: the toolchain (Task 1)
packages/core/package.json, packages/core/tsconfig.json             creates (Task 1)
packages/core/src/index.ts, result.ts, clock.ts                     creates: Result, Clock, Ids (Task 1)
packages/core/src/fixtures/index.ts, fixtures/clock.ts              creates: fixedClock, sequentialIds (Task 1)
scripts/todos.ts, scripts/todos.test.ts                             creates, tests (Task 2)
scripts/core-io.ts, scripts/core-io.test.ts                         creates, tests (Task 3)
scripts/check.ts                                                    creates (Task 4)
scripts/commit-msg.ts, scripts/commit-msg.test.ts                   creates, tests (Task 5)
scripts/install-hooks.ts                                            creates (Task 5)
.github/workflows/check.yml, .github/pull_request_template.md       creates (Task 6)
LICENSE, CONTRIBUTING.md                                            creates (Task 6)
README.md, CLAUDE.md                                                modifies (Task 6)
```

`pnpm-workspace.yaml` lists `packages/*`, `apps/*` and `ee/*`. `.gitignore`: `node_modules/`, `.env`, `*.log`, `coverage/`.

## Interfaces

Consumes: nothing.

Produces:

```ts
// @praeco/core
export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };
export function ok<T>(value: T): Result<T, never>;
export function err<E>(error: E): Result<never, E>;
export interface Clock { now(): Date }
export interface Ids { uuid(): string }
// @praeco/core/fixtures
export function fixedClock(at: Date): Clock & { advance(ms: number): void };
export function sequentialIds(): Ids;   // 00000000-0000-7000-8000-000000000001, ...002, in order

// scripts
export interface SourceFile { path: string; text: string }
export interface Violation { path: string; line: number; text: string; rule: string }
export function findBareTodos(files: readonly SourceFile[]): Violation[];
export const CORE_FORBIDDEN_MODULES: readonly string[];
export function findCoreIo(files: readonly SourceFile[]): Violation[];
export function checkCommitMessage(message: string): { ok: true } | { ok: false; reason: string };
```

## Tasks

### Task 1: The workspace and `@praeco/core`

Files: created as in the file map for Task 1.
Produces: `Result`, `ok`, `err`, `Clock`, `Ids`, `fixedClock`, `sequentialIds`.

No test: these are types and two one-line constructors, and the fixtures are exercised by every later test (Catervas ADR 0060: one test per major functionality). The task is done when `pnpm install`, `pnpm exec biome check .` and `pnpm exec tsc -p tsconfig.json` all succeed; their output goes in the commit body with the date.

- [ ] `build(repo): add the pnpm workspace and @praeco/core`

### Task 2: The bare-TODO check

Files: created `scripts/todos.ts`, tested by `scripts/todos.test.ts`.
Produces: `findBareTodos`. `scripts/todos.ts`, when run directly, reads `git ls-files`, prints each violation as `<path>:<line>: bare TODO` and exits 1 if there is any.

- `findBareTodos` — asserts that a `TODO` with no reference and a `FIXME (` with something other than an issue or link are each reported with their path and line, while `TODO(#12)`, `TODO(https://example.com/x)` and a file under the skipped paths are not.

- [ ] `feat(scripts): fail the check on a bare todo`

### Task 3: The core no-I/O check

Files: created `scripts/core-io.ts`, tested by `scripts/core-io.test.ts`.
Produces: `CORE_FORBIDDEN_MODULES`, `findCoreIo`. Run directly, it reads `packages/core/src/**/*.ts`, prints `<path>:<line>: <rule>` per violation and exits 1 if any.

- `findCoreIo` — asserts that `import { readFile } from 'node:fs/promises'`, `await import('pg')`, `import type { Kysely } from 'kysely'` (a type import still names a package that does I/O), `Date.now()`, `new Date()` and `process.env.X` in a core file are each reported with the rule that matched, and that `new Date(input)` and the same text in a file outside `packages/core/src` are not.

- [ ] `feat(scripts): fail the check when core does i/o`

### Task 4: `pnpm check`

Files: created `scripts/check.ts`; modified `package.json` (`"check": "node scripts/check.ts"`).
Consumes: `findBareTodos` (Task 2), `findCoreIo` (Task 3).

No unit test: the script is a list of commands, and its proof is running it. The task's commit body carries `pnpm check` printing `check: ok`, and the same command after adding a bare `TODO` to `packages/core/src/result.ts` printing `check: failed at todos` (the line removed before the commit).

- [ ] `feat(scripts): run the full check with pnpm check`

### Task 5: Commit messages and hooks

Files: created `scripts/commit-msg.ts`, `scripts/install-hooks.ts`; modified `package.json` (`"hooks": "node scripts/install-hooks.ts"`); tested by `scripts/commit-msg.test.ts`.
Produces: `checkCommitMessage`. `scripts/commit-msg.ts <file>` reads the message's first line, prints the reason and exits 1 when refused.

- `checkCommitMessage` — asserts that `feat(core): add result` and `Merge branch 'x'` are accepted, and that `feature(core): add`, `feat(Core): add`, `feat(core): Add`, `feat(core): add.` and a 73-character first line are each refused with a reason naming what is wrong.

- [ ] `feat(scripts): check commit messages and install the hooks`

### Task 6: CI and the root documents

Files: created `.github/workflows/check.yml`, `.github/pull_request_template.md`, `LICENSE`, `CONTRIBUTING.md`; modified `README.md`, `CLAUDE.md`.

No test. Done when the phase's draft pull request shows the `check` workflow green on this commit.

- [ ] `ci(repo): run pnpm check on every pull request`

## Verification

```
pnpm install --frozen-lockfile
pnpm check
# expected, last line: check: ok
pnpm check --integration
# expected, last line: check: ok   (no integration tests yet; the project allows none)
```

The `check` workflow green on the pull request's last commit.
