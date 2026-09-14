# Phase 0, step 02: Result type

Status: draft
Branch: `phase/0-foundation` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section 8.1 (`core` has no I/O and holds every rule); `docs/standards/code.md`, "Function that may fail" and "Errors are values at package boundaries"
Depends on: step 01 of this phase (not yet committed; record the sha here when it lands)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`@farik/core` has one `Result` type that every fallible function in every package returns, so that a governor refusal, a validation failure, or a store error is a value the caller must look at rather than an exception the caller can forget. When this step is done a contributor can write `evaluate(): Result<Decision, Refusal>` and the type, the two constructors, the two guards, and the three combinators exist and are tested.

## Decisions

- Shape: a hand-written discriminated union `{ ok: true, value } | { ok: false, error }` in `packages/core/src/result.ts`: `docs/plans/project-plan.md`, phase 0 decisions. Rejected: a third-party result library, because the surface needed is seven functions and the type sits on every package boundary.
- Generic parameter names are `Value` and `Failure`. Rejected: `Error`, because it shadows the built-in `Error` type inside the file that defines the type and in every file that reads it.
- The properties are `readonly`, so a result cannot be edited after construction.
- Combinators are plain functions, not methods: `map`, `mapErr`, `andThen`. Rejected: a class with methods, because `docs/standards/code.md` prefers plain functions and data, and a class instance would not survive `structuredClone` or JSON.
- `isOk` and `isErr` are type guards so that `if (isOk(result))` narrows; the `.ok` property narrows on its own too, and both spellings are allowed.
- The file exports the type and its helpers together. `docs/standards/code.md` asks for one primary export per file; the primary export is the type, and the helpers are its constructors and combinators.

## Design

`result.ts` defines `Ok<Value>`, `Err<Failure>`, and `Result<Value, Failure>`, and the functions `ok`, `err`, `isOk`, `isErr`, `map`, `mapErr`, `andThen`. Each function is total: nothing throws. The test file has one `describe` per function and states each behavior in plain words. The package barrel re-exports everything from `result.ts`.

Out of scope: `unwrap`, `unwrapOr`, `all`, async variants, and a `match` helper. Each is added by the first step that needs it, with its own test.

## Architecture notes

Only `packages/core` is touched. Nothing is consumed from any other package. `packages/core/src/index.ts` from step 01 exports `CORE_PACKAGE_NAME` and gains one `export *` line here.

## Global constraints

- `packages/core` does no I/O. `packages/core/tsconfig.json` sets `"types": []` (step 01), so an import of any `node:` module fails to typecheck.
- Tests import from `vitest` explicitly; no globals.
- Commits follow `docs/standards/code.md`: `feat(core): ...`, one task per commit, with this plan's checkboxes ticked in the same commit.

## File map

```
packages/core/src/result.ts        creates: the Result type, ok, err, isOk, isErr, map, mapErr, andThen
packages/core/src/result.test.ts   creates: one describe per function
packages/core/src/index.ts         modifies: adds `export * from './result';`
packages/core/src/index.test.ts    modifies: adds a test that the barrel re-exports `ok`
docs/plans/phase-0-foundation/step-02-result-type.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: The type and its constructors

Files: created `packages/core/src/result.ts`, `packages/core/src/result.test.ts`

Consumes: nothing
Produces: `type Ok<Value>`, `type Err<Failure>`, `type Result<Value, Failure>`, `ok<Value>(value: Value): Ok<Value>`, `err<Failure>(error: Failure): Err<Failure>`

- [ ] Write the failing test, the whole file as it stands after this task:

  ```ts
  import { describe, expect, expectTypeOf, it } from 'vitest';
  import { err, ok } from './result';

  describe('ok', () => {
    it('wraps a value in a successful result', () => {
      const result = ok(42);
      expect(result).toEqual({ ok: true, value: 42 });
      if (result.ok) expectTypeOf(result.value).toEqualTypeOf<number>();
    });
  });

  describe('err', () => {
    it('wraps an error in a failed result', () => {
      const result = err('boom');
      expect(result).toEqual({ ok: false, error: 'boom' });
      if (!result.ok) expectTypeOf(result.error).toEqualTypeOf<string>();
    });
  });
  ```

- [ ] Run it and confirm it fails because the module is missing:

  ```
  pnpm vitest run packages/core/src/result.test.ts
  # expected, among the output:
  # Error: Cannot find module './result' imported from .../packages/core/src/result.test.ts
  #  Test Files  1 failed (1)
  ```

- [ ] Write the minimal implementation:

  ```ts
  export type Ok<Value> = { readonly ok: true; readonly value: Value };
  export type Err<Failure> = { readonly ok: false; readonly error: Failure };
  export type Result<Value, Failure> = Ok<Value> | Err<Failure>;

  export function ok<Value>(value: Value): Ok<Value> {
    return { ok: true, value };
  }

  export function err<Failure>(error: Failure): Err<Failure> {
    return { ok: false, error };
  }
  ```

- [ ] Run the test and the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #  Test Files  2 passed (2)
  #       Tests  3 passed (3)
  ```

- [ ] Commit: `feat(core): add result type with ok and err constructors`

### Task 2: Type guards

Files: modified `packages/core/src/result.ts`, `packages/core/src/result.test.ts`

Consumes: `ok`, `err`, `Result` from Task 1
Produces: `isOk<Value, Failure>(result: Result<Value, Failure>): result is Ok<Value>`, `isErr<Value, Failure>(result: Result<Value, Failure>): result is Err<Failure>`

- [ ] Add to the test file's import line `isErr, isOk` (keep the import sorted: `import { err, isErr, isOk, ok } from './result';`) and append:

  ```ts
  describe('isOk and isErr', () => {
    it('narrow a result to its success or failure side', () => {
      const success = ok(1) as ReturnType<typeof ok<number>> | ReturnType<typeof err<string>>;
      expect(isOk(success)).toBe(true);
      expect(isErr(success)).toBe(false);
      const failure = err('no') as ReturnType<typeof ok<number>> | ReturnType<typeof err<string>>;
      expect(isOk(failure)).toBe(false);
      expect(isErr(failure)).toBe(true);
    });
  });
  ```

- [ ] Run it and confirm it fails because the exports are missing (Vitest resolves a missing named export from TypeScript source to `undefined`, so the failure is a call on `undefined`):

  ```
  pnpm vitest run packages/core/src/result.test.ts
  # expected, among the output:
  # TypeError: isOk is not a function
  #  Test Files  1 failed (1)
  ```

- [ ] Append the minimal implementation to `result.ts`:

  ```ts
  export function isOk<Value, Failure>(result: Result<Value, Failure>): result is Ok<Value> {
    return result.ok;
  }

  export function isErr<Value, Failure>(result: Result<Value, Failure>): result is Err<Failure> {
    return !result.ok;
  }
  ```

- [ ] Run the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #  Test Files  2 passed (2)
  #       Tests  4 passed (4)
  ```

- [ ] Commit: `feat(core): add isOk and isErr guards to result`

### Task 3: map and mapErr

Files: modified `packages/core/src/result.ts`, `packages/core/src/result.test.ts`

Consumes: `ok`, `err`, `Result` from Task 1
Produces: `map<Value, Failure, Next>(result: Result<Value, Failure>, fn: (value: Value) => Next): Result<Next, Failure>`, `mapErr<Value, Failure, Next>(result: Result<Value, Failure>, fn: (error: Failure) => Next): Result<Value, Next>`

- [ ] Extend the import to `import { err, isErr, isOk, map, mapErr, ok } from './result';` and append:

  ```ts
  describe('map', () => {
    it('applies the function to a successful value', () => {
      expect(map(ok(2), (n) => n * 3)).toEqual({ ok: true, value: 6 });
    });
    it('passes a failure through untouched', () => {
      expect(map(err('bad'), (n: number) => n * 3)).toEqual({ ok: false, error: 'bad' });
    });
  });

  describe('mapErr', () => {
    it('applies the function to a failure', () => {
      expect(mapErr(err('bad'), (e) => e.length)).toEqual({ ok: false, error: 3 });
    });
    it('passes a success through untouched', () => {
      expect(mapErr(ok(1), (e: string) => e.length)).toEqual({ ok: true, value: 1 });
    });
  });
  ```

- [ ] Run it and confirm it fails because the exports are missing:

  ```
  pnpm vitest run packages/core/src/result.test.ts
  # expected, among the output:
  # TypeError: map is not a function
  ```

- [ ] Append the minimal implementation:

  ```ts
  export function map<Value, Failure, Next>(
    result: Result<Value, Failure>,
    fn: (value: Value) => Next,
  ): Result<Next, Failure> {
    return result.ok ? ok(fn(result.value)) : result;
  }

  export function mapErr<Value, Failure, Next>(
    result: Result<Value, Failure>,
    fn: (error: Failure) => Next,
  ): Result<Value, Next> {
    return result.ok ? result : err(fn(result.error));
  }
  ```

- [ ] Run the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #       Tests  8 passed (8)
  ```

- [ ] Commit: `feat(core): add map and mapErr to result`

### Task 4: andThen

Files: modified `packages/core/src/result.ts`, `packages/core/src/result.test.ts`

Consumes: `ok`, `err`, `Result` from Task 1
Produces: `andThen<Value, Failure, Next>(result: Result<Value, Failure>, fn: (value: Value) => Result<Next, Failure>): Result<Next, Failure>`

- [ ] Extend the import to `import { andThen, err, isErr, isOk, map, mapErr, ok } from './result';` and append:

  ```ts
  describe('andThen', () => {
    it('chains a successful result into the next fallible step', () => {
      expect(andThen(ok(2), (n) => ok(n + 1))).toEqual({ ok: true, value: 3 });
    });
    it('returns the first failure without calling the next step', () => {
      let called = false;
      const result = andThen(err('first'), () => {
        called = true;
        return ok(1);
      });
      expect(result).toEqual({ ok: false, error: 'first' });
      expect(called).toBe(false);
    });
    it('returns the failure of the next step', () => {
      expect(andThen(ok(2), () => err('second'))).toEqual({ ok: false, error: 'second' });
    });
  });
  ```

- [ ] Run it and confirm it fails because the export is missing:

  ```
  pnpm vitest run packages/core/src/result.test.ts
  # expected, among the output:
  # TypeError: andThen is not a function
  ```

- [ ] Append the minimal implementation:

  ```ts
  export function andThen<Value, Failure, Next>(
    result: Result<Value, Failure>,
    fn: (value: Value) => Result<Next, Failure>,
  ): Result<Next, Failure> {
    return result.ok ? fn(result.value) : result;
  }
  ```

- [ ] Run the package suite; confirm green:

  ```
  pnpm --filter @farik/core test
  # expected:
  #       Tests  11 passed (11)
  ```

- [ ] Commit: `feat(core): add andThen to result`

### Task 5: Export from the package barrel

Files: modified `packages/core/src/index.ts`, `packages/core/src/index.test.ts`

Consumes: everything from Tasks 1 to 4
Produces: the same names from `@farik/core`

- [ ] Add to `packages/core/src/index.test.ts` (the import line becomes `import { CORE_PACKAGE_NAME, ok } from './index';`):

  ```ts
  it('re-exports the result helpers', () => {
    expect(ok(1)).toEqual({ ok: true, value: 1 });
  });
  ```

  inside the existing `describe('@farik/core', ...)` block.

- [ ] Run it and confirm it fails because the barrel does not export `ok`:

  ```
  pnpm vitest run packages/core/src/index.test.ts
  # expected, among the output:
  # TypeError: ok is not a function
  ```

- [ ] Make `packages/core/src/index.ts` exactly:

  ```ts
  export const CORE_PACKAGE_NAME = '@farik/core';
  export * from './result';
  ```

- [ ] Run the full check; confirm green:

  ```
  pnpm check
  # expected: typecheck, lint, format check, and tests all pass;
  #  Test Files  4 passed (4)
  #       Tests  25 passed (25)
  ```

- [ ] Commit: `feat(core): export result from the package barrel`

## Verification

```
pnpm check
# expected, in order: `tsc` for @farik/core and scripts with no output,
# `biome lint` "Checked N files ... No fixes applied.", `biome format` the same,
# vitest "Test Files  4 passed (4)" and "Tests  25 passed (25)" (10 in result.test.ts, 2 in
# index.test.ts, 13 in the scripts project from step 01), exit code 0.
```

```
git log --oneline -5
# expected: the five task commits above, newest first, each subject matching
# `feat(core): ...` and each accepted by the commit-msg hook.
```

## Open questions

none
