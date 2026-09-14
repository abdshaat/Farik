# Phase 0, step 03: Contract schema types

Status: draft
Branch: `phase/0-foundation` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section 3 (Contract), section 5.3 (the structural checks build on these types in phase 1), F4 (validation against the JSON schema); `docs/standards/code.md`, "Schema validation" and "Wire and file formats"
Depends on: step 01 of this phase (not yet committed), step 02 of this phase (not yet committed); record the shas here when they land

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`docs/schemas/task-contract.schema.json` is the single source of truth for what a contract is, and `@farik/core` can take any unknown value, say exactly why it is not a contract, or hand back a typed `TaskContract` with `camelCase` names and the schema's defaults filled in, and turn a `TaskContract` back into the exact `snake_case` shape that goes to disk. When this step is done, phase 1 can write the governor against `TaskContract`, and phase 2 can read and write contract files without hand-writing the schema a second time.

## Decisions

- JSON Schema is the source of truth; TypeScript wire types are generated, committed, and checked for staleness by a test: `docs/plans/project-plan.md`, every-phase decisions ("Schemas own their types"). Rejected: Zod as source of truth, because the schema is a published artifact.
- The generator is `scripts/generate.ts`, using the `json-schema-to-typescript` API (not its CLI) with `additionalProperties: false` so that objects whose schema does not set `additionalProperties` do not gain an index signature, and a fixed `style`. It also writes the schema itself as a TypeScript module (`task-contract.schema.ts`) so that `core` never reads a JSON file at runtime and stays self-contained when published. Rejected: importing the JSON with an import attribute, because it ties `core` to a path outside the package and to bundler support for attributes.
- Staleness is a Vitest test in the `scripts` project, so `pnpm check` keeps its four parts (typecheck, lint, format check, tests) and the check runs where the file I/O is allowed. Rejected: a fifth `pnpm check` part, because `docs/standards/code.md` names four.
- Generated files are excluded from Biome (`!**/generated` in `biome.json`) because their formatting is the generator's and they are never edited.
- The domain type `TaskContract` is hand-written in `camelCase` with `readonly` properties; the mapping pair `contractFromWire` and `contractToWire` is the one mapping layer for this format in `core`, tested by a round trip. Optional fields are absent, never `undefined` (`exactOptionalPropertyTypes` is on). Non-empty arrays from the schema's `minItems: 1` are typed `readonly [Item, ...Item[]]` in the domain too, so the invariant survives the mapping.
- Schema defaults (`max_sessions` 5, `max_iterations` 3, `iteration` 0, `locked` false, `expect.exit_code` 0, `new_tests_required` false) are applied by `contractFromWire`, and `contractToWire` writes them explicitly, so files on disk always carry them. Rejected: `ajv`'s `useDefaults`, because it mutates the input and ignores defaults inside `oneOf` branches.
- `validateContract` validates with `ajv` (`Ajv2020`, `allErrors: true`, `strict: true`, `ajv-formats` for `date-time`) and then maps; it is the only exported entry point that produces a `TaskContract`. `contractFromWire` is exported from its module for its own unit test but not from the package barrel. Errors are `{ path, message }` with the JSON pointer of the offending value, `/` for the root.
- `validateContract` checks the schema only. Definition of Ready rules (reviewer differs from assignee, budget within the sprint) are phase 1.
- `references` entries carry `format: uri`, which `ajv-formats` checks; a contract written from an issue keeps the issue's link.
- `TaskId` is the template literal type `` `FRK-${number}` ``. The two casts from `string` in `contractFromWire` are justified by the validator having checked the pattern, and each carries a `// reason:` comment.
- Fixtures are builder functions in `packages/core/src/contract/fixtures/contract.ts`, per `docs/standards/code.md` ("Fixture"): `aContractWire(overrides)` for the minimal valid contract and `aFullContractWire()` for one with every optional field and every verification method.
- Versions: `json-schema-to-typescript` 16.0.0 (root devDependency), `ajv` 8.20.0 and `ajv-formats` 3.0.1 (dependencies of `@farik/core`).

## Design

Generation: `pnpm generate` runs `scripts/generate.ts`, which for each entry in `GENERATED_SCHEMAS` writes the types file and the schema module. `scripts/generate.test.ts` regenerates in memory and compares with the committed files.

Core: `contract.types.ts` (domain types, plus `TaskContractWire` as an alias of the generated `FarikTaskContract`), `contract-from-wire.ts`, `contract-to-wire.ts`, `validate-contract.ts`, and `fixtures/contract.ts`, under `packages/core/src/contract/`. The barrel exports the types, `validateContract`, `contractToWire`, `ValidationError`, and the three default constants.

Out of scope: YAML reading and writing (phase 2), any semantic check beyond the schema (phase 1), schemas other than the task contract (each is added by the step that owns it).

## Architecture notes

Touches `packages/core` (new `contract/` and `generated/` directories, new dependencies), `scripts/` (generator and its test), the root `package.json` (`generate` script and the generator dependency), and `biome.json`. Consumes `Result`, `ok`, `err` from `packages/core/src/result.ts` (step 02). Reads `docs/schemas/task-contract.schema.json` at generation time only.

## Global constraints

- `packages/core` does no I/O; the generator and its test live in `scripts/`, where I/O is allowed. `packages/core/tsconfig.json` has `"types": []`, so any `node:` import in `core` fails to typecheck.
- Wire keys are `snake_case`; domain properties are `camelCase`; only `contract-from-wire.ts` and `contract-to-wire.ts` know both.
- No `any`; the two `as` casts each carry a `// reason:` comment.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
scripts/generate.ts                                        creates: GENERATED_SCHEMAS, generateTypes, generateSchemaModule, main
scripts/generate.test.ts                                   creates: staleness test per generated file
package.json                                               modifies: adds the `generate` script and the generator devDependency
biome.json                                                 modifies: excludes `**/generated`
packages/core/package.json                                 modifies: adds ajv and ajv-formats dependencies
packages/core/src/generated/task-contract.ts               creates (generated): FarikTaskContract, Role, ExitCriterion
packages/core/src/generated/task-contract.schema.ts        creates (generated): taskContractSchema
packages/core/src/contract/contract.types.ts               creates: TaskContractWire alias and the domain types
packages/core/src/contract/fixtures/contract.ts            creates: aContractWire, aFullContractWire
packages/core/src/contract/contract-from-wire.ts           creates: contractFromWire and the default constants
packages/core/src/contract/contract-from-wire.test.ts      creates: mapping and defaults tests
packages/core/src/contract/validate-contract.ts            creates: validateContract, ValidationError
packages/core/src/contract/validate-contract.test.ts       creates: acceptance and refusal tests
packages/core/src/contract/contract-to-wire.ts             creates: contractToWire
packages/core/src/contract/contract-to-wire.test.ts        creates: round-trip tests
packages/core/src/index.ts                                 modifies: exports the contract API
packages/core/src/index.test.ts                            modifies: adds a test that the barrel exports validateContract
docs/plans/phase-0-foundation/step-03-contract-schema-types.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: Generator and generated files

Files: created `scripts/generate.ts`, `scripts/generate.test.ts`, `packages/core/src/generated/task-contract.ts`, `packages/core/src/generated/task-contract.schema.ts`; modified `package.json`, `biome.json`

Consumes: `docs/schemas/task-contract.schema.json` on `main`
Produces: `GENERATED_SCHEMAS: readonly GeneratedSchema[]`, `generateTypes(entry, root): Promise<string>`, `generateSchemaModule(entry, root): string`, the generated `FarikTaskContract`, `Role`, `ExitCriterion` types and the `taskContractSchema` constant

- [ ] Add the dependency and the script. In the root `package.json`, add `"generate": "node scripts/generate.ts"` to `scripts` (after `"prepare"`) and `"json-schema-to-typescript": "16.0.0"` to `devDependencies` (keep the list alphabetical, so it goes after `"@vitest/coverage-v8"`), then run:

  ```
  pnpm install
  # expected, among the output:
  # devDependencies:
  # + json-schema-to-typescript 16.0.0
  ```

- [ ] Write `scripts/generate.ts`:

  ```ts
  import { readFileSync, writeFileSync } from 'node:fs';
  import { dirname, resolve } from 'node:path';
  import { compileFromFile } from 'json-schema-to-typescript';

  export type GeneratedSchema = {
    readonly schema: string;
    readonly types: string;
    readonly schemaModule: string;
    readonly exportName: string;
  };

  export const GENERATED_SCHEMAS: readonly GeneratedSchema[] = [
    {
      schema: 'docs/schemas/task-contract.schema.json',
      types: 'packages/core/src/generated/task-contract.ts',
      schemaModule: 'packages/core/src/generated/task-contract.schema.ts',
      exportName: 'taskContractSchema',
    },
  ];

  const banner = (entry: GeneratedSchema): string =>
    `/* Generated from ${entry.schema} by \`pnpm generate\`. Do not edit. */`;

  export async function generateTypes(entry: GeneratedSchema, root: string): Promise<string> {
    const schemaPath = resolve(root, entry.schema);
    return compileFromFile(schemaPath, {
      additionalProperties: false,
      bannerComment: banner(entry),
      cwd: dirname(schemaPath),
      style: { printWidth: 100, semi: true, singleQuote: true, trailingComma: 'all' },
    });
  }

  export function generateSchemaModule(entry: GeneratedSchema, root: string): string {
    const json = JSON.parse(readFileSync(resolve(root, entry.schema), 'utf8'));
    return `${banner(entry)}\n\nexport const ${entry.exportName} = ${JSON.stringify(json, null, 2)};\n`;
  }

  if (process.argv[1] !== undefined && import.meta.filename === process.argv[1]) {
    const root = resolve(import.meta.dirname, '..');
    for (const entry of GENERATED_SCHEMAS) {
      writeFileSync(resolve(root, entry.types), await generateTypes(entry, root));
      writeFileSync(resolve(root, entry.schemaModule), generateSchemaModule(entry, root));
      console.log(`generated ${entry.types} and ${entry.schemaModule}`);
    }
  }
  ```

- [ ] Write the failing test `scripts/generate.test.ts`:

  ```ts
  import { readFileSync } from 'node:fs';
  import { resolve } from 'node:path';
  import { describe, expect, it } from 'vitest';
  import { GENERATED_SCHEMAS, generateSchemaModule, generateTypes } from './generate';

  const root = resolve(import.meta.dirname, '..');

  describe('generated files', () => {
    for (const entry of GENERATED_SCHEMAS) {
      it(`keeps ${entry.types} in sync with ${entry.schema} (run pnpm generate if not)`, async () => {
        expect(readFileSync(resolve(root, entry.types), 'utf8')).toBe(
          await generateTypes(entry, root),
        );
      });
      it(`keeps ${entry.schemaModule} in sync with ${entry.schema} (run pnpm generate if not)`, () => {
        expect(readFileSync(resolve(root, entry.schemaModule), 'utf8')).toBe(
          generateSchemaModule(entry, root),
        );
      });
    }
  });
  ```

- [ ] Run it and confirm it fails because the generated files do not exist:

  ```
  pnpm vitest run scripts
  # expected, among the output:
  # Error: ENOENT: no such file or directory, open '.../packages/core/src/generated/task-contract.ts'
  # Error: ENOENT: no such file or directory, open '.../packages/core/src/generated/task-contract.schema.ts'
  #  Test Files  1 failed | 2 passed (3)
  #       Tests  2 failed | 13 passed (15)
  ```

- [ ] Generate:

  ```
  mkdir -p packages/core/src/generated
  pnpm generate
  # expected:
  # generated packages/core/src/generated/task-contract.ts and packages/core/src/generated/task-contract.schema.ts
  sha256sum packages/core/src/generated/task-contract.ts packages/core/src/generated/task-contract.schema.ts
  # expected:
  # be64db566d6b07cf6150a8933915a6dd681892769d38a3f02b3cca8e86a32f5a  packages/core/src/generated/task-contract.ts
  # 8db66bd639940563109a34ac1ac5342822e37a98726ddc37fa5821251c361f54  packages/core/src/generated/task-contract.schema.ts
  ```

  The types file begins with the banner and exports `Role`, `FarikTaskContract` (with `in_scope: [string, ...string[]]` tuples for every `minItems: 1` array, `references?: string[]`, `locked?: boolean`, and a `verification` union of the five methods), and `ExitCriterion`. If the hashes differ, the schema on `main` or the generator version has changed since this plan was written; stop and update the plan.

- [ ] Exclude generated files from Biome. In `biome.json`, change `files.includes` to:

  ```json
  "includes": ["**", "!**/generated", "!**/*.yml", "!**/*.yaml"]
  ```

- [ ] Run the scripts tests and the lint; confirm green:

  ```
  pnpm vitest run scripts
  # expected:
  #  Test Files  3 passed (3)
  #       Tests  15 passed (15)
  pnpm lint
  # expected: "Checked N files ... No fixes applied." and no output from check-todos
  ```

- [ ] Commit: `build(repo): generate contract types and schema module from the json schema`

### Task 2: Domain types, fixtures, and contractFromWire

Files: created `packages/core/src/contract/contract.types.ts`, `packages/core/src/contract/fixtures/contract.ts`, `packages/core/src/contract/contract-from-wire.ts`, `packages/core/src/contract/contract-from-wire.test.ts`

Consumes: `FarikTaskContract` from Task 1
Produces: every type in `contract.types.ts` (listed in the project plan, phase 0 step 03), `aContractWire(overrides?: Partial<TaskContractWire>): TaskContractWire`, `aFullContractWire(): TaskContractWire`, `contractFromWire(wire: TaskContractWire): TaskContract`, `DEFAULT_MAX_SESSIONS = 5`, `DEFAULT_MAX_ITERATIONS = 3`, `DEFAULT_EXPECTED_EXIT_CODE = 0`

- [ ] Write `contract.types.ts`:

  ```ts
  import type { FarikTaskContract } from '../generated/task-contract';

  /** The contract exactly as it is read from or written to disk and the wire: `snake_case` keys. */
  export type TaskContractWire = FarikTaskContract;

  export type TaskId = `FRK-${number}`;

  export type TaskStatus =
    | 'draft'
    | 'refining'
    | 'ready'
    | 'assigned'
    | 'in_progress'
    | 'blocked'
    | 'verifying'
    | 'rejected'
    | 'accepted'
    | 'escalated'
    | 'cancelled';

  export type Role =
    | 'product_manager'
    | 'scrum_master'
    | 'architect'
    | 'software_developer'
    | 'marketing_specialist'
    | 'human';

  export type AgentRole = Exclude<Role, 'human'>;

  export type Risk = 'low' | 'medium' | 'high';

  export type NonEmpty<Item> = readonly [Item, ...Item[]];

  export type Requirement = {
    readonly id: string;
    readonly text: string;
    readonly rationale?: string;
  };

  export type CommandVerification = {
    readonly method: 'command';
    readonly command: string;
    readonly expect: {
      readonly exitCode: number;
      readonly stdoutContains?: string;
      readonly stdoutNotContains?: string;
    };
  };

  export type TestVerification = {
    readonly method: 'test';
    readonly command: string;
    readonly newTestsRequired: boolean;
  };

  export type ArtifactVerification = {
    readonly method: 'artifact';
    readonly path: string;
    readonly mustContain?: readonly string[];
  };

  export type ReviewVerification = {
    readonly method: 'review';
    readonly rubric: NonEmpty<string>;
  };

  export type HumanVerification = {
    readonly method: 'human';
    readonly question: string;
  };

  export type Verification =
    | CommandVerification
    | TestVerification
    | ArtifactVerification
    | ReviewVerification
    | HumanVerification;

  export type VerificationMethod = Verification['method'];

  export type ExitCriterion = {
    readonly id: string;
    readonly text: string;
    readonly satisfies?: readonly string[];
    readonly verification: Verification;
  };

  export type Budget = {
    readonly maxCostUsd: number;
    readonly maxSessions: number;
    readonly maxIterations: number;
  };

  export type Notes = {
    readonly completion?: string;
    readonly review?: string;
    readonly escalation?: string;
  };

  /** A task contract with `camelCase` keys and schema defaults applied. Only produced by `validateContract`. */
  export type TaskContract = {
    readonly id: TaskId;
    readonly title: string;
    readonly intent: string;
    readonly scope: {
      readonly inScope: NonEmpty<string>;
      readonly outOfScope: NonEmpty<string>;
    };
    readonly requirements: NonEmpty<Requirement>;
    readonly exitCriteria: NonEmpty<ExitCriterion>;
    readonly constraints?: readonly string[];
    readonly dependencies?: readonly TaskId[];
    readonly references?: readonly string[];
    readonly assigneeRole: Role;
    readonly reviewerRole: Role;
    readonly risk: Risk;
    readonly budget: Budget;
    readonly allowedPaths: NonEmpty<string>;
    readonly status: TaskStatus;
    readonly locked: boolean;
    readonly sprint?: string;
    readonly assignee?: string;
    readonly reviewer?: string;
    readonly iteration: number;
    readonly notes?: Notes;
    readonly createdBy?: string;
    readonly createdAt?: string;
    readonly updatedAt?: string;
  };
  ```

- [ ] Write `fixtures/contract.ts`:

  ```ts
  import type { TaskContractWire } from '../contract.types';

  /** A schema-valid wire contract in `draft`, with every required field and no optional ones. */
  export function aContractWire(overrides: Partial<TaskContractWire> = {}): TaskContractWire {
    return {
      id: 'FRK-1',
      title: 'Add a login page',
      intent: 'A user can sign in with an email and password so that their work is private.',
      scope: { in_scope: ['login form'], out_of_scope: ['password reset'] },
      requirements: [{ id: 'R1', text: 'The login form has email and password fields.' }],
      exit_criteria: [
        {
          id: 'C1',
          text: 'Unit tests for the login form pass.',
          satisfies: ['R1'],
          verification: { method: 'test', command: 'pnpm test login' },
        },
      ],
      assignee_role: 'software_developer',
      reviewer_role: 'architect',
      risk: 'low',
      budget: { max_cost_usd: 5 },
      allowed_paths: ['src/login/**'],
      status: 'draft',
      ...overrides,
    };
  }

  /** A wire contract with every optional field present and every default written explicitly. */
  export function aFullContractWire(): TaskContractWire {
    return aContractWire({
      exit_criteria: [
        {
          id: 'C1',
          text: 'The check command exits zero and prints the summary.',
          satisfies: ['R1'],
          verification: {
            method: 'command',
            command: 'pnpm check',
            expect: { exit_code: 0, stdout_contains: 'passed', stdout_not_contains: 'failed' },
          },
        },
        {
          id: 'C2',
          text: 'A new test exists and fails on the base branch.',
          verification: { method: 'test', command: 'pnpm test login', new_tests_required: true },
        },
        {
          id: 'C3',
          text: 'The release notes mention the login page.',
          verification: { method: 'artifact', path: 'CHANGELOG.md', must_contain: ['login'] },
        },
        {
          id: 'C4',
          text: 'The form follows the design system.',
          verification: { method: 'review', rubric: ['Does the form use the shared Button?'] },
        },
        {
          id: 'C5',
          text: 'The founder has tried the login flow.',
          verification: { method: 'human', question: 'Did you sign in successfully?' },
        },
      ],
      requirements: [
        { id: 'R1', text: 'The login form has email and password fields.', rationale: 'Baseline.' },
      ],
      constraints: ['Use the existing session store.'],
      dependencies: ['FRK-2'],
      references: ['https://github.com/abdshaat/farik/issues/1'],
      locked: true,
      budget: { max_cost_usd: 5, max_sessions: 5, max_iterations: 3 },
      sprint: 'S1',
      assignee: 'maya-chen',
      reviewer: 'omar-reyes',
      iteration: 0,
      notes: { completion: 'Done.', review: 'C1 passed: see output.', escalation: 'None.' },
      created_by: 'maya-chen',
      created_at: '2026-09-14T10:00:00Z',
      updated_at: '2026-09-14T11:00:00Z',
    });
  }
  ```

- [ ] Write the failing test `contract-from-wire.test.ts`:

  ```ts
  import { describe, expect, it } from 'vitest';
  import { contractFromWire } from './contract-from-wire';
  import { aContractWire, aFullContractWire } from './fixtures/contract';

  describe('contractFromWire', () => {
    it('maps snake_case keys to camelCase and applies the schema defaults', () => {
      const contract = contractFromWire(aContractWire());
      expect(contract.scope).toEqual({ inScope: ['login form'], outOfScope: ['password reset'] });
      expect(contract.assigneeRole).toBe('software_developer');
      expect(contract.allowedPaths).toEqual(['src/login/**']);
      expect(contract.budget).toEqual({ maxCostUsd: 5, maxSessions: 5, maxIterations: 3 });
      expect(contract.iteration).toBe(0);
      expect(contract.locked).toBe(false);
    });

    it('leaves optional fields absent rather than undefined', () => {
      const contract = contractFromWire(aContractWire());
      expect('sprint' in contract).toBe(false);
      expect('notes' in contract).toBe(false);
      expect('references' in contract).toBe(false);
      expect('rationale' in contract.requirements[0]).toBe(false);
    });

    it('maps every verification method', () => {
      const contract = contractFromWire(aFullContractWire());
      expect(contract.exitCriteria.map((criterion) => criterion.verification)).toEqual([
        {
          method: 'command',
          command: 'pnpm check',
          expect: { exitCode: 0, stdoutContains: 'passed', stdoutNotContains: 'failed' },
        },
        { method: 'test', command: 'pnpm test login', newTestsRequired: true },
        { method: 'artifact', path: 'CHANGELOG.md', mustContain: ['login'] },
        { method: 'review', rubric: ['Does the form use the shared Button?'] },
        { method: 'human', question: 'Did you sign in successfully?' },
      ]);
    });

    it('applies the default exit code to a command criterion that omits it', () => {
      const contract = contractFromWire(
        aContractWire({
          exit_criteria: [
            {
              id: 'C1',
              text: 'The check command passes.',
              verification: { method: 'command', command: 'pnpm check', expect: {} },
            },
          ],
        }),
      );
      expect(contract.exitCriteria[0].verification).toEqual({
        method: 'command',
        command: 'pnpm check',
        expect: { exitCode: 0 },
      });
    });
  });
  ```

- [ ] Run it and confirm it fails because the module is missing:

  ```
  pnpm vitest run packages/core/src/contract/contract-from-wire.test.ts
  # expected, among the output:
  # Error: Cannot find module './contract-from-wire' imported from .../contract-from-wire.test.ts
  #  Test Files  1 failed (1)
  ```

- [ ] Write `contract-from-wire.ts`:

  ```ts
  import type {
    ExitCriterion,
    Notes,
    Requirement,
    TaskContract,
    TaskContractWire,
    TaskId,
    Verification,
  } from './contract.types';

  export const DEFAULT_MAX_SESSIONS = 5;
  export const DEFAULT_MAX_ITERATIONS = 3;
  export const DEFAULT_EXPECTED_EXIT_CODE = 0;

  type WireRequirement = TaskContractWire['requirements'][number];
  type WireCriterion = TaskContractWire['exit_criteria'][number];
  type WireVerification = WireCriterion['verification'];

  function requirementFromWire(wire: WireRequirement): Requirement {
    return {
      id: wire.id,
      text: wire.text,
      ...(wire.rationale !== undefined ? { rationale: wire.rationale } : {}),
    };
  }

  function verificationFromWire(wire: WireVerification): Verification {
    switch (wire.method) {
      case 'command':
        return {
          method: 'command',
          command: wire.command,
          expect: {
            exitCode: wire.expect.exit_code ?? DEFAULT_EXPECTED_EXIT_CODE,
            ...(wire.expect.stdout_contains !== undefined
              ? { stdoutContains: wire.expect.stdout_contains }
              : {}),
            ...(wire.expect.stdout_not_contains !== undefined
              ? { stdoutNotContains: wire.expect.stdout_not_contains }
              : {}),
          },
        };
      case 'test':
        return {
          method: 'test',
          command: wire.command,
          newTestsRequired: wire.new_tests_required ?? false,
        };
      case 'artifact':
        return {
          method: 'artifact',
          path: wire.path,
          ...(wire.must_contain !== undefined ? { mustContain: wire.must_contain } : {}),
        };
      case 'review':
        return { method: 'review', rubric: wire.rubric };
      case 'human':
        return { method: 'human', question: wire.question };
    }
  }

  function criterionFromWire(wire: WireCriterion): ExitCriterion {
    return {
      id: wire.id,
      text: wire.text,
      ...(wire.satisfies !== undefined ? { satisfies: wire.satisfies } : {}),
      verification: verificationFromWire(wire.verification),
    };
  }

  function mapNonEmpty<In, Out>(
    list: readonly [In, ...In[]],
    fn: (item: In) => Out,
  ): readonly [Out, ...Out[]] {
    const [head, ...tail] = list;
    return [fn(head), ...tail.map(fn)];
  }

  /**
   * Maps a schema-valid wire contract to the domain shape, applying the schema's defaults.
   * Callers must validate first; `validateContract` is the only public entry point.
   */
  export function contractFromWire(wire: TaskContractWire): TaskContract {
    return {
      // reason: the schema pattern ^FRK-[0-9]{1,6}$ has been checked by the validator before this runs.
      id: wire.id as TaskId,
      title: wire.title,
      intent: wire.intent,
      scope: { inScope: wire.scope.in_scope, outOfScope: wire.scope.out_of_scope },
      requirements: mapNonEmpty(wire.requirements, requirementFromWire),
      exitCriteria: mapNonEmpty(wire.exit_criteria, criterionFromWire),
      ...(wire.constraints !== undefined ? { constraints: wire.constraints } : {}),
      // reason: every dependency matches the same id pattern, checked by the validator.
      ...(wire.dependencies !== undefined ? { dependencies: wire.dependencies as TaskId[] } : {}),
      ...(wire.references !== undefined ? { references: wire.references } : {}),
      assigneeRole: wire.assignee_role,
      reviewerRole: wire.reviewer_role,
      risk: wire.risk,
      budget: {
        maxCostUsd: wire.budget.max_cost_usd,
        maxSessions: wire.budget.max_sessions ?? DEFAULT_MAX_SESSIONS,
        maxIterations: wire.budget.max_iterations ?? DEFAULT_MAX_ITERATIONS,
      },
      allowedPaths: wire.allowed_paths,
      status: wire.status,
      locked: wire.locked ?? false,
      ...(wire.sprint !== undefined ? { sprint: wire.sprint } : {}),
      ...(wire.assignee !== undefined ? { assignee: wire.assignee } : {}),
      ...(wire.reviewer !== undefined ? { reviewer: wire.reviewer } : {}),
      iteration: wire.iteration ?? 0,
      ...(wire.notes !== undefined ? { notes: notesFromWire(wire.notes) } : {}),
      ...(wire.created_by !== undefined ? { createdBy: wire.created_by } : {}),
      ...(wire.created_at !== undefined ? { createdAt: wire.created_at } : {}),
      ...(wire.updated_at !== undefined ? { updatedAt: wire.updated_at } : {}),
    };
  }

  function notesFromWire(wire: NonNullable<TaskContractWire['notes']>): Notes {
    return {
      ...(wire.completion !== undefined ? { completion: wire.completion } : {}),
      ...(wire.review !== undefined ? { review: wire.review } : {}),
      ...(wire.escalation !== undefined ? { escalation: wire.escalation } : {}),
    };
  }
  ```

- [ ] Run the test and the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #  Test Files  3 passed (3)
  #       Tests  16 passed (16)
  ```

- [ ] Commit: `feat(core): add contract domain types and the wire-to-domain mapping`

### Task 3: validateContract

Files: created `packages/core/src/contract/validate-contract.ts`, `packages/core/src/contract/validate-contract.test.ts`; modified `packages/core/package.json`

Consumes: `taskContractSchema` from Task 1; `contractFromWire`, `aContractWire`, `aFullContractWire`, the types from Task 2; `Result`, `ok`, `err` from step 02
Produces: `type ValidationError = { readonly path: string; readonly message: string }`, `validateContract(input: unknown): Result<TaskContract, readonly ValidationError[]>`

- [ ] Add the dependencies to `packages/core/package.json` (a new `"dependencies"` object after `"scripts"`):

  ```json
  "dependencies": {
    "ajv": "8.20.0",
    "ajv-formats": "3.0.1"
  }
  ```

  then:

  ```
  pnpm install
  # expected, among the output:
  # packages/core
  # dependencies:
  # + ajv 8.20.0
  # + ajv-formats 3.0.1
  ```

- [ ] Write the failing test `validate-contract.test.ts`:

  ```ts
  import { describe, expect, it } from 'vitest';
  import { aContractWire, aFullContractWire } from './fixtures/contract';
  import { validateContract } from './validate-contract';

  describe('validateContract', () => {
    it('accepts a schema-valid contract and maps it to camelCase with defaults applied', () => {
      const result = validateContract(aContractWire());
      expect(result.ok).toBe(true);
      if (!result.ok) return;
      expect(result.value.id).toBe('FRK-1');
      expect(result.value.scope.outOfScope).toEqual(['password reset']);
      expect(result.value.assigneeRole).toBe('software_developer');
      expect(result.value.budget).toEqual({ maxCostUsd: 5, maxSessions: 5, maxIterations: 3 });
      expect(result.value.iteration).toBe(0);
      expect(result.value.exitCriteria[0].verification).toEqual({
        method: 'test',
        command: 'pnpm test login',
        newTestsRequired: false,
      });
    });

    it('refuses a value that is not an object', () => {
      const result = validateContract('not a contract');
      expect(result).toEqual({ ok: false, error: [{ path: '/', message: 'must be object' }] });
    });

    it('refuses a task id that does not match FRK-<n>', () => {
      const result = validateContract(aContractWire({ id: 'TASK-1' }));
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error).toEqual([
        { path: '/id', message: 'must match pattern "^FRK-[0-9]{1,6}$"' },
      ]);
    });

    it('refuses an empty out_of_scope list', () => {
      const result = validateContract(
        aContractWire({ scope: { in_scope: ['a'], out_of_scope: [] as unknown as [string] } }),
      );
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error).toEqual([
        { path: '/scope/out_of_scope', message: 'must NOT have fewer than 1 items' },
      ]);
    });

    it('refuses a command criterion without an expect block and reports every violation', () => {
      const result = validateContract(
        aContractWire({
          exit_criteria: [
            {
              id: 'C1',
              text: 'The check command passes.',
              verification: { method: 'command', command: 'pnpm check' } as never,
            },
          ],
        }),
      );
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error.length).toBeGreaterThan(1);
      expect(result.error).toContainEqual({
        path: '/exit_criteria/0/verification',
        message: "must have required property 'expect'",
      });
    });

    it('refuses a reference that is not a uri', () => {
      const result = validateContract(aContractWire({ references: ['not a uri'] }));
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error).toEqual([{ path: '/references/0', message: 'must match format "uri"' }]);
    });

    it('refuses an unknown top-level property', () => {
      const result = validateContract({ ...aContractWire(), owner: 'someone' });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error).toEqual([{ path: '/', message: 'must NOT have additional properties' }]);
    });

    it('accepts every verification method and every optional field', () => {
      const result = validateContract(aFullContractWire());
      expect(result.ok).toBe(true);
      if (!result.ok) return;
      expect(result.value.exitCriteria.map((criterion) => criterion.verification.method)).toEqual([
        'command',
        'test',
        'artifact',
        'review',
        'human',
      ]);
      expect(result.value.dependencies).toEqual(['FRK-2']);
      expect(result.value.references).toEqual(['https://github.com/abdshaat/farik/issues/1']);
      expect(result.value.locked).toBe(true);
      expect(result.value.notes).toEqual({
        completion: 'Done.',
        review: 'C1 passed: see output.',
        escalation: 'None.',
      });
    });
  });
  ```

- [ ] Run it and confirm it fails because the module is missing:

  ```
  pnpm vitest run packages/core/src/contract/validate-contract.test.ts
  # expected, among the output:
  # Error: Cannot find module './validate-contract' imported from .../validate-contract.test.ts
  #  Test Files  1 failed (1)
  ```

- [ ] Write `validate-contract.ts`:

  ```ts
  import Ajv2020 from 'ajv/dist/2020.js';
  import addFormats from 'ajv-formats';
  import { taskContractSchema } from '../generated/task-contract.schema';
  import { type Result, err, ok } from '../result';
  import { contractFromWire } from './contract-from-wire';
  import type { TaskContract, TaskContractWire } from './contract.types';

  export type ValidationError = {
    /** JSON pointer into the input, `/` for the root. */
    readonly path: string;
    readonly message: string;
  };

  const ajv = new Ajv2020({ allErrors: true, strict: true });
  addFormats(ajv);
  const validateWire = ajv.compile<TaskContractWire>(taskContractSchema);

  /**
   * Checks an unknown value against `docs/schemas/task-contract.schema.json` and, when it conforms,
   * returns the domain contract with defaults applied. Refuses anything the schema refuses, with one
   * error per violation; it does not check Definition of Ready rules.
   */
  export function validateContract(input: unknown): Result<TaskContract, readonly ValidationError[]> {
    if (validateWire(input)) return ok(contractFromWire(input));
    const errors = (validateWire.errors ?? []).map((error) => ({
      path: error.instancePath === '' ? '/' : error.instancePath,
      message: error.message ?? 'is invalid',
    }));
    return err(errors);
  }
  ```

- [ ] Run the test and the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #  Test Files  4 passed (4)
  #       Tests  24 passed (24)
  ```

- [ ] Commit: `feat(core): validate contracts against the json schema`

### Task 4: contractToWire

Files: created `packages/core/src/contract/contract-to-wire.ts`, `packages/core/src/contract/contract-to-wire.test.ts`

Consumes: the types from Task 2; `validateContract` from Task 3; the fixtures from Task 2
Produces: `contractToWire(contract: TaskContract): TaskContractWire`

- [ ] Write the failing test `contract-to-wire.test.ts`:

  ```ts
  import { describe, expect, it } from 'vitest';
  import { contractToWire } from './contract-to-wire';
  import { aContractWire, aFullContractWire } from './fixtures/contract';
  import { validateContract } from './validate-contract';

  describe('contractToWire', () => {
    it('round-trips a contract that has every field, without losing or renaming anything', () => {
      const wire = aFullContractWire();
      const validated = validateContract(wire);
      expect(validated.ok).toBe(true);
      if (!validated.ok) return;
      expect(contractToWire(validated.value)).toEqual(wire);
    });

    it('writes schema defaults explicitly for a contract that omitted them', () => {
      const validated = validateContract(aContractWire());
      expect(validated.ok).toBe(true);
      if (!validated.ok) return;
      const wire = contractToWire(validated.value);
      expect(wire.budget).toEqual({ max_cost_usd: 5, max_sessions: 5, max_iterations: 3 });
      expect(wire.iteration).toBe(0);
      expect(wire.locked).toBe(false);
      expect(wire.exit_criteria[0].verification).toEqual({
        method: 'test',
        command: 'pnpm test login',
        new_tests_required: false,
      });
    });

    it('produces output the validator accepts again', () => {
      const validated = validateContract(aContractWire());
      if (!validated.ok) throw new Error('fixture must validate');
      expect(validateContract(contractToWire(validated.value)).ok).toBe(true);
    });
  });
  ```

- [ ] Run it and confirm it fails because the module is missing:

  ```
  pnpm vitest run packages/core/src/contract/contract-to-wire.test.ts
  # expected, among the output:
  # Error: Cannot find module './contract-to-wire' imported from .../contract-to-wire.test.ts
  ```

- [ ] Write `contract-to-wire.ts`:

  ```ts
  import type {
    ExitCriterion,
    Requirement,
    TaskContract,
    TaskContractWire,
    Verification,
  } from './contract.types';

  type WireRequirement = TaskContractWire['requirements'][number];
  type WireCriterion = TaskContractWire['exit_criteria'][number];
  type WireVerification = WireCriterion['verification'];

  function requirementToWire(requirement: Requirement): WireRequirement {
    return {
      id: requirement.id,
      text: requirement.text,
      ...(requirement.rationale !== undefined ? { rationale: requirement.rationale } : {}),
    };
  }

  function verificationToWire(verification: Verification): WireVerification {
    switch (verification.method) {
      case 'command':
        return {
          method: 'command',
          command: verification.command,
          expect: {
            exit_code: verification.expect.exitCode,
            ...(verification.expect.stdoutContains !== undefined
              ? { stdout_contains: verification.expect.stdoutContains }
              : {}),
            ...(verification.expect.stdoutNotContains !== undefined
              ? { stdout_not_contains: verification.expect.stdoutNotContains }
              : {}),
          },
        };
      case 'test':
        return {
          method: 'test',
          command: verification.command,
          new_tests_required: verification.newTestsRequired,
        };
      case 'artifact':
        return {
          method: 'artifact',
          path: verification.path,
          ...(verification.mustContain !== undefined
            ? { must_contain: [...verification.mustContain] }
            : {}),
        };
      case 'review':
        return { method: 'review', rubric: [...verification.rubric] };
      case 'human':
        return { method: 'human', question: verification.question };
    }
  }

  function criterionToWire(criterion: ExitCriterion): WireCriterion {
    return {
      id: criterion.id,
      text: criterion.text,
      ...(criterion.satisfies !== undefined ? { satisfies: [...criterion.satisfies] } : {}),
      verification: verificationToWire(criterion.verification),
    };
  }

  function mapNonEmpty<In, Out>(
    list: readonly [In, ...In[]],
    fn: (item: In) => Out,
  ): [Out, ...Out[]] {
    const [head, ...tail] = list;
    return [fn(head), ...tail.map(fn)];
  }

  /** Maps a domain contract to the `snake_case` wire shape, writing defaults explicitly. */
  export function contractToWire(contract: TaskContract): TaskContractWire {
    return {
      id: contract.id,
      title: contract.title,
      intent: contract.intent,
      scope: { in_scope: [...contract.scope.inScope], out_of_scope: [...contract.scope.outOfScope] },
      requirements: mapNonEmpty(contract.requirements, requirementToWire),
      exit_criteria: mapNonEmpty(contract.exitCriteria, criterionToWire),
      ...(contract.constraints !== undefined ? { constraints: [...contract.constraints] } : {}),
      ...(contract.dependencies !== undefined ? { dependencies: [...contract.dependencies] } : {}),
      ...(contract.references !== undefined ? { references: [...contract.references] } : {}),
      assignee_role: contract.assigneeRole,
      reviewer_role: contract.reviewerRole,
      risk: contract.risk,
      budget: {
        max_cost_usd: contract.budget.maxCostUsd,
        max_sessions: contract.budget.maxSessions,
        max_iterations: contract.budget.maxIterations,
      },
      allowed_paths: [...contract.allowedPaths],
      status: contract.status,
      locked: contract.locked,
      ...(contract.sprint !== undefined ? { sprint: contract.sprint } : {}),
      ...(contract.assignee !== undefined ? { assignee: contract.assignee } : {}),
      ...(contract.reviewer !== undefined ? { reviewer: contract.reviewer } : {}),
      iteration: contract.iteration,
      ...(contract.notes !== undefined ? { notes: { ...contract.notes } } : {}),
      ...(contract.createdBy !== undefined ? { created_by: contract.createdBy } : {}),
      ...(contract.createdAt !== undefined ? { created_at: contract.createdAt } : {}),
      ...(contract.updatedAt !== undefined ? { updated_at: contract.updatedAt } : {}),
    };
  }
  ```

- [ ] Run the test and the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #  Test Files  5 passed (5)
  #       Tests  27 passed (27)
  ```

- [ ] Commit: `feat(core): map contracts back to the snake_case wire shape`

### Task 5: Export from the package barrel

Files: modified `packages/core/src/index.ts`, `packages/core/src/index.test.ts`

Consumes: everything from Tasks 2 to 4
Produces: the contract API from `@farik/core`

- [ ] Add to `packages/core/src/index.test.ts` (import line becomes `import { CORE_PACKAGE_NAME, ok, validateContract } from './index';`), inside the existing `describe`:

  ```ts
  it('exports the contract validator', () => {
    expect(validateContract(null).ok).toBe(false);
  });
  ```

- [ ] Run it and confirm it fails because the barrel does not export `validateContract`:

  ```
  pnpm vitest run packages/core/src/index.test.ts
  # expected, among the output:
  # TypeError: validateContract is not a function
  ```

- [ ] Make `packages/core/src/index.ts` exactly:

  ```ts
  export const CORE_PACKAGE_NAME = '@farik/core';
  export * from './result';
  export type * from './contract/contract.types';
  export { contractToWire } from './contract/contract-to-wire';
  export {
    DEFAULT_EXPECTED_EXIT_CODE,
    DEFAULT_MAX_ITERATIONS,
    DEFAULT_MAX_SESSIONS,
  } from './contract/contract-from-wire';
  export { type ValidationError, validateContract } from './contract/validate-contract';
  ```

- [ ] Run the full check; confirm green:

  ```
  pnpm check
  # expected:
  #  Test Files  8 passed (8)
  #       Tests  43 passed (43)
  ```

- [ ] Commit: `feat(core): export the contract api from the package barrel`

## Verification

```
pnpm check
# expected, in order: tsc for @farik/core and scripts with no diagnostics; biome lint "Checked N files"
# with no fixes; check-todos silent; biome format "Checked N files"; vitest
#  Test Files  8 passed (8)
#       Tests  43 passed (43)
# (28 in @farik/core, 15 in the scripts project), exit code 0.
```

```
pnpm generate && git status --porcelain packages/core/src/generated
# expected: the generate lines, then no output from git status (nothing changed).
```

```
pnpm test:coverage
# expected: the coverage table reports packages/core/src/contract at 100% statements. Reported, not gated.
```

```
git log --oneline -5
# expected: the five task commits above, newest first.
```

## Open questions

none
