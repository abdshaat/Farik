# Phase 0, step 01: Monorepo scaffold

Status: draft
Branch: `phase/0-foundation` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section 8.1 (a TypeScript monorepo managed with pnpm; `core` has no I/O), section 9 (Apache 2.0); ADR 0002 (toolchain); `docs/standards/code.md`, "Toolchain"
Depends on: none

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

The repository becomes a pnpm workspace in which `pnpm check` runs typecheck, lint, format check, and tests, locally and in a GitHub Actions workflow, and every commit is checked for the Conventional Commits shape and for formatting before it lands. `@farik/core` exists with one passing test, the `LICENSE` file says Apache 2.0, and `CLAUDE.md` and `README.md` tell a contributor how to run the check. Nothing about Farik's behavior exists yet; this step is the floor every later step stands on.

## Decisions

All recorded in `docs/plans/project-plan.md`, phase 0 and the every-phase list; restated here only where the step needs the exact value.

- Node 24.21.0 in `.nvmrc`, `engines.node` `>=24.21.0 <25`; pnpm 12.4.1 through `packageManager`, `engines.pnpm` `>=12.4.1 <13`.
- Exact versions: TypeScript 7.0.2, Biome 2.5.13, Vitest 5.0.0, Vite 8.3.0 (Vitest's peer), `@vitest/coverage-v8` 5.0.0, lefthook 2.1.14, `@changesets/cli` 3.0.3, `@types/node` 24.13.4. Every one was installed together and `pnpm check` run green on 2026-09-14.
- pnpm 12 runs no dependency build scripts unless allowed. `allowBuilds` in `pnpm-workspace.yaml` allows `lefthook` only; its postinstall is what writes the git hooks, so there is no `prepare` script. Rejected: allowing all builds, because the only script the toolchain needs is lefthook's.
- pnpm 12 refuses versions published less than one day ago (`minimumReleaseAge`, default 1440 minutes) unless it writes them into `minimumReleaseAgeExclude`. lefthook 2.1.14 and `@changesets/cli` 3.0.3 were published on 2026-09-14, so an install on that day writes an exclude list into `pnpm-workspace.yaml`. This step executes later than that; the committed `pnpm-workspace.yaml` has no exclude list, and if `pnpm install` adds one, the executor removes it and installs again. The default one-day delay is kept as a supply-chain safeguard. Rejected: `minimumReleaseAge: 0`, because the delay costs nothing once versions are a day old.
- The hook config, the Biome config, the hook scripts, and their tests land in the first task, because lefthook's postinstall installs the hooks at the first `pnpm install` and the first commit already runs them: the commit-msg hook needs `scripts/check-commit-message.ts`, and the pre-commit hook's `biome check` needs `biome.json` (Biome's default is tab indentation and would reject two-space files).
- `packages/core/tsconfig.json` sets `"types": []` so that no `node:` module resolves inside `core`; the base config sets `"types": ["node"]` for everything else.
- Biome: `preset: "recommended"` (the `recommended: true` key is deprecated in 2.5), `noExplicitAny` as an error, `--error-on-warnings` on the lint command so that a warning fails, organize-imports on, YAML files excluded (Biome does not format them and `pnpm-lock.yaml` must never be touched), `ignoreUnknown` so Markdown and other files pass through.
- Vitest: `projects: ['packages/*', { test: { name: 'scripts', include: ['scripts/**/*.test.ts'] } }]`, coverage by `v8`, reported by `pnpm test:coverage` and never gated.
- Scripts are TypeScript run directly by Node (type stripping is on by default in Node 24) with a `main` guard `import.meta.filename === process.argv[1]`, so the same file exports a function for its test and runs as a hook.
- The commit-msg check accepts `<type>(<scope>): <subject>` with the nine types from `docs/standards/code.md`, a kebab-case scope, a lower-case first character, no trailing period, at most 72 characters, and it accepts `Merge ...` and `Revert ...` subjects. It does not check the scope against a list; the list of packages grows and CI, not the hook, is the enforcement.
- The bare-TODO check scans tracked `.ts`, `.tsx`, `.js`, `.mjs`, `.cjs`, and `.css` files for `TODO` or `FIXME` not followed by `(FRK-<n>)`, `(#<n>)`, or `(<http link>)`, skipping its own source.
- The Changesets configuration is hand-written (`baseBranch: main`, `access: public`); no changeset is added in this step because the scaffold is not a user-visible change.
- `LICENSE` is the verbatim Apache License 2.0 text from `https://www.apache.org/licenses/LICENSE-2.0.txt` (sha256 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`, 202 lines).
- The CI workflow uses `actions/checkout@v5`, `pnpm/action-setup@v4` (reads `packageManager`), and `actions/setup-node@v5` with `node-version-file: .nvmrc` and pnpm caching.

## Design

Four tasks. The first creates the workspace root with every tool configured and the two hook scripts tested, and ends with the first commit passing its own hooks. The second adds the Vitest projects configuration and `@farik/core` with a smoke test, and ends with `pnpm check` green. The third adds the CI workflow. The fourth updates `README.md` and `CLAUDE.md` so that the check command is documented where contributors look.

Out of scope: any package other than `core`, any build step or `dist/` output, code generation (step 03), the `Result` type (step 02), a changeset, and a `pnpm check:integration` command (phase 2 step 04 adds it with the first integration test).

## Architecture notes

Creates `packages/core` (`@farik/core`) and the `scripts/` directory. Touches the repository root, `.github/workflows/`, `.changeset/`, `README.md`, and `CLAUDE.md`. Consumes nothing from any package; `docs/standards/code.md` and `.editorconfig` on `main` define the formatting the Biome config mirrors (two spaces, LF, final newline).

## Global constraints

- `packages/core` does no I/O; enforced by `"types": []` in its `tsconfig.json`.
- Every `package.json` dependency is an exact version.
- Wire and file formats in this step (`biome.json`, `package.json`, YAML) follow their tools' conventions; no Farik wire format exists yet.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
LICENSE                                   creates: Apache License 2.0, verbatim
.nvmrc                                    creates: 24.21.0
package.json                              creates: workspace root, scripts, pinned devDependencies
pnpm-workspace.yaml                       creates: packages/* and allowBuilds for lefthook
pnpm-lock.yaml                            creates (generated by pnpm install)
.changeset/config.json                    creates: Changesets configuration
.changeset/README.md                      creates: one paragraph on adding a changeset
tsconfig.base.json                        creates: strict compiler options shared by every package
biome.json                                creates: lint, format, and organize-imports configuration
lefthook.yml                              creates: pre-commit biome check on staged files; commit-msg script
scripts/tsconfig.json                     creates: typecheck configuration for scripts
scripts/check-commit-message.ts           creates: checkCommitMessage and the commit-msg hook entry point
scripts/check-commit-message.test.ts      creates: eight tests for the accepted and rejected shapes
scripts/check-todos.ts                    creates: findBareTodos and the lint entry point
scripts/check-todos.test.ts               creates: five tests
vitest.config.ts                          creates: projects and coverage configuration
packages/core/package.json                creates: @farik/core manifest
packages/core/tsconfig.json               creates: extends the base, types: []
packages/core/src/index.ts                creates: CORE_PACKAGE_NAME
packages/core/src/index.test.ts           creates: the smoke test
.github/workflows/check.yml               creates: the check workflow
README.md                                 modifies: getting started and status
CLAUDE.md                                 modifies: the Commands and Current state sections
docs/plans/phase-0-foundation/step-01-monorepo-scaffold.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: Workspace root, toolchain, and commit hooks

Files: created `LICENSE`, `.nvmrc`, `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.changeset/config.json`, `.changeset/README.md`, `tsconfig.base.json`, `biome.json`, `lefthook.yml`, `scripts/tsconfig.json`, `scripts/check-commit-message.ts`, `scripts/check-commit-message.test.ts`, `scripts/check-todos.ts`, `scripts/check-todos.test.ts`

Consumes: nothing
Produces: the `pnpm check`, `pnpm typecheck`, `pnpm lint`, `pnpm format`, `pnpm format:check`, `pnpm test`, and `pnpm test:coverage` commands; `checkCommitMessage(message: string): { ok: true } | { ok: false; reason: string }`; `findBareTodos(files: ReadonlyArray<{ path: string; text: string }>): string[]`; installed git hooks

- [ ] Confirm the starting point, on a clean checkout of `phase/0-foundation` created from `main`:

  ```
  pnpm check
  # expected:
  # Error: ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL
  #   × Command "check" not found
  ```

- [ ] Fetch the license and pin Node:

  ```
  curl -sSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE
  sha256sum LICENSE
  # expected: cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
  printf '24.21.0\n' > .nvmrc
  ```

- [ ] Write `package.json`:

  ```json
  {
    "name": "farik",
    "version": "0.0.0",
    "private": true,
    "description": "An operating system for small teams of AI agents with a governance harness at its core.",
    "license": "Apache-2.0",
    "type": "module",
    "packageManager": "pnpm@12.4.1",
    "engines": {
      "node": ">=24.21.0 <25",
      "pnpm": ">=12.4.1 <13"
    },
    "scripts": {
      "check": "pnpm typecheck && pnpm lint && pnpm format:check && pnpm test",
      "typecheck": "pnpm --recursive run typecheck && tsc --noEmit --project scripts/tsconfig.json",
      "lint": "biome lint --error-on-warnings . && node scripts/check-todos.ts",
      "format": "biome format --write .",
      "format:check": "biome format .",
      "test": "vitest run",
      "test:coverage": "vitest run --coverage"
    },
    "devDependencies": {
      "@biomejs/biome": "2.5.13",
      "@changesets/cli": "3.0.3",
      "@types/node": "24.13.4",
      "@vitest/coverage-v8": "5.0.0",
      "lefthook": "2.1.14",
      "typescript": "7.0.2",
      "vite": "8.3.0",
      "vitest": "5.0.0"
    }
  }
  ```

- [ ] Write `pnpm-workspace.yaml`:

  ```yaml
  packages:
    - packages/*
  allowBuilds:
    lefthook: true
  ```

- [ ] Write `.changeset/config.json`:

  ```json
  {
    "changelog": "@changesets/cli/changelog",
    "commit": false,
    "fixed": [],
    "linked": [],
    "access": "public",
    "baseBranch": "main",
    "updateInternalDependencies": "patch",
    "ignore": []
  }
  ```

  and `.changeset/README.md`:

  ```markdown
  # Changesets

  Every user-visible change adds a changeset file here with `pnpm changeset`. See https://github.com/changesets/changesets.
  ```

- [ ] Write `tsconfig.base.json`:

  ```json
  {
    "compilerOptions": {
      "target": "es2024",
      "lib": ["es2024"],
      "module": "esnext",
      "moduleResolution": "bundler",
      "types": ["node"],
      "strict": true,
      "noUncheckedIndexedAccess": true,
      "exactOptionalPropertyTypes": true,
      "noImplicitOverride": true,
      "noFallthroughCasesInSwitch": true,
      "verbatimModuleSyntax": true,
      "isolatedModules": true,
      "skipLibCheck": true,
      "resolveJsonModule": true,
      "noEmit": true
    }
  }
  ```

  and `scripts/tsconfig.json`:

  ```json
  {
    "extends": "../tsconfig.base.json",
    "include": ["*.ts"]
  }
  ```

- [ ] Write `biome.json` (this is the shape Biome's own formatter produces, so `pnpm format:check` passes on it):

  ```json
  {
    "$schema": "https://biomejs.dev/schemas/2.5.13/schema.json",
    "vcs": {
      "enabled": true,
      "clientKind": "git",
      "useIgnoreFile": true
    },
    "files": {
      "ignoreUnknown": true,
      "includes": ["**", "!**/*.yml", "!**/*.yaml"]
    },
    "formatter": {
      "enabled": true,
      "indentStyle": "space",
      "indentWidth": 2,
      "lineWidth": 100,
      "lineEnding": "lf"
    },
    "linter": {
      "enabled": true,
      "rules": {
        "preset": "recommended",
        "suspicious": {
          "noExplicitAny": "error"
        }
      }
    },
    "javascript": {
      "formatter": {
        "quoteStyle": "single",
        "semicolons": "always",
        "trailingCommas": "all"
      }
    },
    "assist": {
      "enabled": true,
      "actions": {
        "source": {
          "organizeImports": "on"
        }
      }
    }
  }
  ```

- [ ] Write `lefthook.yml`:

  ```yaml
  pre-commit:
    jobs:
      - name: biome
        glob: '*.{ts,tsx,js,mjs,cjs,json,jsonc,css}'
        run: pnpm exec biome check --error-on-warnings --no-errors-on-unmatched {staged_files}
  commit-msg:
    jobs:
      - name: conventional commit
        run: node scripts/check-commit-message.ts {1}
  ```

- [ ] Install; lefthook's postinstall writes the hooks:

  ```
  pnpm install
  # expected, among the output:
  # .../node_modules/lefthook postinstall: sync hooks: ✔️
  # devDependencies:
  # + @biomejs/biome 2.5.13
  # + @changesets/cli 3.0.3
  # + @types/node 24.13.4
  # + @vitest/coverage-v8 5.0.0
  # + lefthook 2.1.14
  # + typescript 7.0.2
  # + vite 8.3.0
  # + vitest 5.0.0
  # Done in <n>s using pnpm v12.4.1
  ls .git/hooks | grep -E '^(pre-commit|commit-msg)$'
  # expected: commit-msg and pre-commit
  cat pnpm-workspace.yaml
  # expected: the four lines written above and nothing else. If pnpm appended a
  # minimumReleaseAgeExclude list, a pinned version is under a day old: delete the
  # list, wait until the version is a day old, and run pnpm install again.
  ```

- [ ] Write the failing tests. `scripts/check-commit-message.test.ts`:

  ```ts
  import { describe, expect, it } from 'vitest';
  import { checkCommitMessage } from './check-commit-message';

  describe('checkCommitMessage', () => {
    it('accepts a conventional subject with a type and a scope', () => {
      expect(checkCommitMessage('feat(core): add result type\n\nbody\n')).toEqual({ ok: true });
    });

    it('accepts a merge commit', () => {
      expect(checkCommitMessage('Merge pull request #3 from abdshaat/phase/0-foundation')).toEqual({
        ok: true,
      });
    });

    it('accepts a revert commit', () => {
      expect(checkCommitMessage('Revert "feat(core): add result type"')).toEqual({ ok: true });
    });

    it('skips comment lines when finding the subject', () => {
      expect(checkCommitMessage('# Please enter the commit message\nfix(store): keep order')).toEqual(
        { ok: true },
      );
    });

    it('rejects a subject without a type and a scope', () => {
      const result = checkCommitMessage('Add result type');
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.reason).toBe('subject "Add result type" is not <type>(<scope>): <subject>');
    });

    it('rejects a subject longer than 72 characters', () => {
      const subject = `feat(core): ${'x'.repeat(70)}`;
      const result = checkCommitMessage(subject);
      expect(result).toEqual({ ok: false, reason: 'subject is 82 characters; the limit is 72' });
    });

    it('rejects a subject that ends with a period', () => {
      expect(checkCommitMessage('feat(core): add result type.').ok).toBe(false);
    });

    it('rejects a subject that starts with an upper-case letter', () => {
      expect(checkCommitMessage('feat(core): Add result type').ok).toBe(false);
    });
  });
  ```

  and `scripts/check-todos.test.ts` (the marker word is assembled at runtime so that this file never contains it literally):

  ```ts
  import { describe, expect, it } from 'vitest';
  import { findBareTodos } from './check-todos';

  const marker = ['TO', 'DO'].join('');

  describe('findBareTodos', () => {
    it('reports a bare marker with its path and line', () => {
      const files = [{ path: 'packages/core/src/a.ts', text: `const a = 1;\n// ${marker} fix this\n` }];
      expect(findBareTodos(files)).toEqual(['packages/core/src/a.ts:2']);
    });

    it('accepts a marker that carries a task id', () => {
      const files = [{ path: 'a.ts', text: `// ${marker}(FRK-12) fix this\n` }];
      expect(findBareTodos(files)).toEqual([]);
    });

    it('accepts a marker that carries an issue number or a link', () => {
      const files = [
        { path: 'a.ts', text: `// ${marker}(#12) fix this\n` },
        { path: 'b.ts', text: `// ${marker}(https://github.com/abdshaat/farik/issues/12) fix\n` },
      ];
      expect(findBareTodos(files)).toEqual([]);
    });

    it('reports a bare FIXME too', () => {
      const files = [{ path: 'a.ts', text: `// ${['FIX', 'ME'].join('')} later\n` }];
      expect(findBareTodos(files)).toEqual(['a.ts:1']);
    });

    it('skips its own source', () => {
      const files = [{ path: 'scripts/check-todos.ts', text: `const BARE = /${marker}/;\n` }];
      expect(findBareTodos(files)).toEqual([]);
    });
  });
  ```

- [ ] Run them and confirm they fail because the modules are missing:

  ```
  pnpm vitest run scripts
  # expected, among the output:
  # Error: Cannot find module './check-commit-message' imported from .../scripts/check-commit-message.test.ts
  # Error: Cannot find module './check-todos' imported from .../scripts/check-todos.test.ts
  #  Test Files  2 failed (2)
  ```

- [ ] Write `scripts/check-commit-message.ts`:

  ```ts
  import { readFileSync } from 'node:fs';

  const TYPES = ['feat', 'fix', 'refactor', 'test', 'docs', 'chore', 'build', 'ci', 'perf'] as const;
  const SUBJECT_PATTERN = new RegExp(`^(${TYPES.join('|')})\\([a-z][a-z0-9-]*\\): [a-z0-9].*[^.]$`);

  export type CheckResult = { ok: true } | { ok: false; reason: string };

  export function checkCommitMessage(message: string): CheckResult {
    const firstLine = message.split('\n').find((line) => !line.startsWith('#')) ?? '';
    if (/^(Merge|Revert) /.test(firstLine)) return { ok: true };
    if (firstLine.length > 72)
      return { ok: false, reason: `subject is ${firstLine.length} characters; the limit is 72` };
    if (!SUBJECT_PATTERN.test(firstLine))
      return { ok: false, reason: `subject "${firstLine}" is not <type>(<scope>): <subject>` };
    return { ok: true };
  }

  if (process.argv[1] !== undefined && import.meta.filename === process.argv[1]) {
    const file = process.argv[2];
    if (file === undefined) {
      console.error('usage: node scripts/check-commit-message.ts <file>');
      process.exit(2);
    }
    const result = checkCommitMessage(readFileSync(file, 'utf8'));
    if (!result.ok) {
      console.error(`commit message rejected: ${result.reason}`);
      process.exit(1);
    }
  }
  ```

  and `scripts/check-todos.ts`:

  ```ts
  import { execFileSync } from 'node:child_process';
  import { readFileSync } from 'node:fs';

  const BARE_TODO = /\b(TODO|FIXME)\b(?!\((FRK-[0-9]+|#[0-9]+|https?:\/\/[^)]+)\))/;
  const SELF = 'scripts/check-todos';

  export function findBareTodos(files: ReadonlyArray<{ path: string; text: string }>): string[] {
    const findings: string[] = [];
    for (const file of files) {
      if (file.path.startsWith(SELF)) continue;
      file.text.split('\n').forEach((line, index) => {
        if (BARE_TODO.test(line)) findings.push(`${file.path}:${index + 1}`);
      });
    }
    return findings;
  }

  if (process.argv[1] !== undefined && import.meta.filename === process.argv[1]) {
    const listed = execFileSync(
      'git',
      ['ls-files', '-z', '--', '*.ts', '*.tsx', '*.js', '*.mjs', '*.cjs', '*.css'],
      {
        encoding: 'utf8',
      },
    );
    const files = listed
      .split('\0')
      .filter((path) => path.length > 0)
      .map((path) => ({ path, text: readFileSync(path, 'utf8') }));
    const findings = findBareTodos(files);
    if (findings.length > 0) {
      console.error(`bare TODO or FIXME without a task id or issue link:\n${findings.join('\n')}`);
      process.exit(1);
    }
  }
  ```

- [ ] Run the tests, the typecheck, the lint, and the format check; confirm green:

  ```
  pnpm vitest run scripts
  # expected:
  #  Test Files  2 passed (2)
  #       Tests  13 passed (13)
  pnpm typecheck
  # expected: "Scope: 0 of 1 workspace projects" (no packages yet), then tsc with no diagnostics, exit 0
  pnpm lint
  # expected: "Checked 9 files in <n>ms. No fixes applied." and nothing from check-todos
  pnpm format:check
  # expected: "Checked 9 files in <n>ms. No fixes applied."
  ```

- [ ] Prove the hooks work. Stage everything, then:

  ```
  git add -A
  git commit -m "Add root."
  # expected: the commit is refused;
  # commit message rejected: subject "Add root." is not <type>(<scope>): <subject>
  git commit -m "build(repo): add the workspace root, toolchain, and commit hooks"
  # expected: the pre-commit biome job and the commit-msg job both show ✔️ and the commit lands
  ```

  Tick this task's boxes in this plan and amend them into the same commit (`git commit --amend --no-edit` after `git add docs/plans`), which is allowed here because the commit has not been pushed.

### Task 2: Vitest projects and the core package

Files: created `vitest.config.ts`, `packages/core/package.json`, `packages/core/tsconfig.json`, `packages/core/src/index.ts`, `packages/core/src/index.test.ts`; modified `pnpm-lock.yaml`

Consumes: the commands from Task 1
Produces: `@farik/core` with `CORE_PACKAGE_NAME: '@farik/core'`; `pnpm check` green

- [ ] Write `vitest.config.ts`:

  ```ts
  import { defineConfig } from 'vitest/config';

  export default defineConfig({
    test: {
      projects: ['packages/*', { test: { name: 'scripts', include: ['scripts/**/*.test.ts'] } }],
      coverage: { provider: 'v8', reporter: ['text', 'lcov'], reportsDirectory: 'coverage' },
    },
  });
  ```

- [ ] Write `packages/core/package.json`:

  ```json
  {
    "name": "@farik/core",
    "version": "0.0.0",
    "private": true,
    "description": "Schemas, task state machine, governor, and cost model. Performs no I/O.",
    "license": "Apache-2.0",
    "type": "module",
    "exports": {
      ".": "./src/index.ts"
    },
    "scripts": {
      "typecheck": "tsc --noEmit --project tsconfig.json",
      "test": "vitest run"
    }
  }
  ```

  and `packages/core/tsconfig.json`:

  ```json
  {
    "extends": "../../tsconfig.base.json",
    "include": ["src"],
    "compilerOptions": {
      "types": []
    }
  }
  ```

- [ ] Register the package and confirm the project has no tests yet:

  ```
  pnpm install
  # expected: "Done in <n>ms using pnpm v12.4.1"; pnpm-lock.yaml gains an importer for packages/core
  pnpm vitest run packages/core
  # expected: exit code 1 and, among the output,
  # No test files found, exiting with code 1
  # include: **/*.{test,spec}.?(c|m)[jt]s?(x)
  ```

- [ ] Write the failing test `packages/core/src/index.test.ts`:

  ```ts
  import { describe, expect, it } from 'vitest';
  import { CORE_PACKAGE_NAME } from './index';

  describe('@farik/core', () => {
    it('exposes its package name', () => {
      expect(CORE_PACKAGE_NAME).toBe('@farik/core');
    });
  });
  ```

- [ ] Run it and confirm it fails because the module is missing:

  ```
  pnpm vitest run packages/core
  # expected, among the output:
  # Error: Cannot find module './index' imported from .../packages/core/src/index.test.ts
  #  Test Files  1 failed (1)
  ```

- [ ] Write `packages/core/src/index.ts`:

  ```ts
  export const CORE_PACKAGE_NAME = '@farik/core';
  ```

- [ ] Prove that `core` cannot import a Node module, then remove the probe:

  ```
  printf "import { readFileSync } from 'node:fs';\nexport const probe = readFileSync;\n" > packages/core/src/io-probe.ts
  pnpm --filter @farik/core typecheck
  # expected: error TS2591: Cannot find name 'node:fs'. Do you need to install type definitions for node? ...
  rm packages/core/src/io-probe.ts
  ```

- [ ] Run the full check; confirm green:

  ```
  pnpm check
  # expected, in order:
  # $ pnpm --recursive run typecheck && tsc --noEmit --project scripts/tsconfig.json
  # $ tsc --noEmit --project tsconfig.json          (for @farik/core, no diagnostics)
  # $ biome lint --error-on-warnings . && node scripts/check-todos.ts
  # Checked 14 files in <n>ms. No fixes applied.
  # $ biome format .
  # Checked 14 files in <n>ms. No fixes applied.
  # $ vitest run
  #  Test Files  3 passed (3)
  #       Tests  14 passed (14)
  # exit code 0
  ```

- [ ] Commit: `feat(core): scaffold the core package with a smoke test`

### Task 3: Continuous integration

Files: created `.github/workflows/check.yml`

Consumes: `pnpm check` from Task 2
Produces: the `check` workflow on pull requests and on pushes to `main`

- [ ] Write `.github/workflows/check.yml`:

  ```yaml
  name: check
  on:
    pull_request:
    push:
      branches: [main]
  jobs:
    check:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v5
        - uses: pnpm/action-setup@v4
        - uses: actions/setup-node@v5
          with:
            node-version-file: .nvmrc
            cache: pnpm
        - run: pnpm install --frozen-lockfile
        - run: pnpm check
  ```

- [ ] Confirm the lockfile is frozen-installable, which is what CI will do:

  ```
  rm -rf node_modules && pnpm install --frozen-lockfile
  # expected: "Done in <n>s using pnpm v12.4.1" with no lockfile changes (git status shows pnpm-lock.yaml unchanged)
  ```

- [ ] Commit: `ci(repo): run pnpm check on pull requests and main`

- [ ] Push the phase branch and open the phase's draft pull request (`docs/standards/workflow.md` stage 5), then confirm on the pull request that the `check` job ran and passed on this commit. Paste the job's summary lines into the pull request's verification section. If the job fails, the failure is this step's to fix before Task 4.

### Task 4: Documentation

Files: modified `README.md`, `CLAUDE.md`

Consumes: nothing
Produces: contributor instructions that match the repository

- [ ] In `README.md`, replace the line beginning `Status: specification stage.` with:

  ```markdown
  Status: phase 0 (foundation) in progress; nothing runs yet. Project standards are in place; see [CONTRIBUTING.md](CONTRIBUTING.md) before making a change.

  ## Getting started

  Install Node 24.21.0 (`nvm use` reads `.nvmrc`) and pnpm 12.4.1 (`corepack enable` reads `packageManager`), then:

  ```
  pnpm install
  pnpm check
  ```

  `pnpm check` runs the typecheck, the lint, the format check, and the tests, and is what the `check` workflow runs on every pull request. `pnpm format` rewrites files to the house style. Git hooks are installed by `pnpm install`; if they are missing, run `pnpm exec lefthook install`.
  ```

  (The inner fenced block uses three backticks in the file; it is shown indented here only to nest it.)

- [ ] In `CLAUDE.md`, replace the `## Commands` paragraph with:

  ```markdown
  `pnpm check` is the full check: typecheck, lint (including the bare-TODO check), format check, and unit tests, in that order. `pnpm format` rewrites files. `pnpm test:coverage` reports coverage without gating on it. Run `pnpm check` before claiming anything is done and paste its output.
  ```

  and replace the `## Current state` paragraph with:

  ```markdown
  Phase 0 (foundation) is in progress on `phase/0-foundation`. The monorepo scaffold exists; `@farik/core` has no behavior yet. The next steps are the `Result` type and the contract schema types, per `docs/plans/project-plan.md`.
  ```

- [ ] Run the full check once more; confirm green (Markdown is not checked by Biome, so the output is the same as Task 2's).

- [ ] Commit: `docs(repo): describe the toolchain and the check command`

## Verification

```
pnpm check
# expected: exit code 0 with
#  Test Files  3 passed (3)
#       Tests  14 passed (14)
```

```
git log --oneline -4
# expected, newest first:
# docs(repo): describe the toolchain and the check command
# ci(repo): run pnpm check on pull requests and main
# feat(core): scaffold the core package with a smoke test
# build(repo): add the workspace root, toolchain, and commit hooks
```

The `check` workflow run on the pushed head of `phase/0-foundation` is green; its link and summary are in the pull request.

```
sha256sum LICENSE
# expected: cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
```

## Open questions

none
