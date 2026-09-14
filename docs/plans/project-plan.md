# Farik project plan

Status: draft, revision 2. Phase 0 is fully decided and its step plans are written under `docs/plans/phase-0-foundation/`. Every later phase lists its decisions as made or open. The open ones are collected in the decision register at the end of this document, each with a recommendation, and each must be closed here or in an ADR before the first step plan of its phase is written.
Owner: project founder.
Last decision change: 2026-09-14, revision 2: every phase broken into steps with the interfaces each step adds, the missing pieces of the spec (contract editor, premium hooks, orchestrator, Farik tools, git adapter, team model, milestone exits) given steps, Milestone 0 split into a store phase and a runtime phase so that each phase ends in something usable, and the open decisions gathered into one register.

This document divides the project into phases and each phase into steps. Vocabulary is as defined in `docs/standards/workflow.md`; where a step name below says "task" it means the product's task contract from `docs/SPEC.md`, not a plan task. A phase ends in something a person can use or verify. A phase is one pull request from the branch `phase/<n>-<name>`. A step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md` and lands as a group of commits on the phase branch. Steps run in the order listed; a step depends only on steps above it in the same phase and on phases already merged. The rules are in `docs/standards/workflow.md` stage 2 (Plan) and ADR 0003.

Phases 0 through 3 together deliver Milestone 0 from `docs/SPEC.md` section 11. Phases 4 and 5 deliver Milestone 1. Phase 6 delivers Milestone 2 and the open-source launch. Phase 7 is Milestone 3.

## How to read a phase

Each phase has four parts. "Ends with" is the thing a person can use or verify when the phase merges. "Decisions" lists what the phase rests on, each marked made (with where it is recorded) or open (with the register entry that must close it, and the step it blocks). "Steps" is the ordered table, one line per step, with the spec references it serves. "Interfaces this phase adds" lists, per step, the public signatures the step produces, so that the no-forward-dependency rule can be checked by reading: a step may consume only signatures listed under an earlier step or an earlier phase. Signatures are given in TypeScript with `camelCase` names; the step plan is the place where they become exact code. Where a later step changes what an earlier interface is passed (for example, the sprint budget replacing the daily budget in a governor context) the later step adds the field; the earlier step never leaves a placeholder for it.

## Decisions that apply to every phase

Made, with the record:

- Workflow and pull request rules: ADR 0001, `docs/standards/workflow.md`.
- Toolchain: ADR 0002. pnpm workspaces, TypeScript strict, Biome, Vitest, lefthook, Changesets, GitHub Actions, `pnpm check`.
- Planning structure: ADR 0003.
- Naming and wire format rules: `docs/standards/code.md`.
- `packages/core` performs no I/O: `docs/standards/code.md`, hard rule 5 in `CLAUDE.md`. Enforced mechanically from Phase 0: `packages/core/tsconfig.json` sets `"types": []`, so an import of any `node:` module fails to typecheck.
- Licensing: Apache 2.0 for everything outside `ee/`: `docs/SPEC.md` section 9. The `LICENSE` file is added by Phase 0 step 01.
- Package layout: `docs/SPEC.md` section 8.1. A package is created by the first step that needs it, so that no empty package sits unused. The table below says which step creates each one.
- Model provider for the first release: Claude via the Claude Agent SDK, one adapter, interface not promised stable: `docs/SPEC.md` section 8.2.
- Execution model: agent sessions run on the host through the SDK; the SDK's Bash tool is disallowed in every session, its WebFetch and WebSearch tools are allowed only under the `network` tier; command execution is a Farik tool backed by an `Executor` with a Docker implementation and a host implementation; git operations that need credentials or a tier are Farik tools on the host: ADR 0004 (proposed; the founder accepts it before the phase 3 step 01 plan is written). Register D1 decides separately whether the host implementation ships to users.
- Versions are exact. Every dependency in every `package.json` is pinned to one version, never a range, and an upgrade is its own `chore` commit. The lockfile is committed. Rejected: caret ranges, because a plan whose expected outputs depend on a version cannot be reproduced against a range.
- Schemas own their types. Every format that leaves a process or is written to disk has a JSON Schema (2020-12) in `docs/schemas/`. `pnpm generate` (`scripts/generate.ts`) turns each schema into a TypeScript wire type and a TypeScript module holding the schema itself, both committed under `packages/<owner>/src/generated/`, and `pnpm check` fails if a committed file is stale. The owner is the package at whose edge the format is read or written. Domain types are `camelCase`, hand-written next to the code that uses them, and one pair of mapping functions per format (`<x>FromWire`, `<x>ToWire`) is the package's single mapping layer, with a round-trip test. Validation uses `ajv` in strict mode with `ajv-formats`.

| Schema | Owner | First generated in |
|---|---|---|
| `task-contract.schema.json` | `@farik/core` | phase 0 step 03 |
| `prices.schema.json` | `@farik/core` | phase 1 step 05 |
| `event.schema.json`, `command.schema.json` | `@farik/protocol` | phase 2 step 01 |
| `team.schema.json` | `@farik/store` | phase 2 step 05 |
| `role.schema.json` | `@farik/roles` | phase 3 step 07 |
| `rpc.schema.json` | `@farik/protocol` | phase 5 step 01 |

| Package | Directory | Created in |
|---|---|---|
| `@farik/core` | `packages/core` | phase 0 step 01 |
| `@farik/protocol` | `packages/protocol` | phase 2 step 01 |
| `@farik/store` | `packages/store` | phase 2 step 02 |
| `@farik/cli` | `apps/cli` | phase 2 step 06 |
| `@farik/runtime` | `packages/runtime` | phase 3 step 01 |
| `@farik/roles` | `packages/roles` | phase 3 step 07 |
| `@farik/ui` | `packages/ui` | phase 5 step 02 |
| `@farik/desktop` | `apps/desktop` | phase 5 step 03 |
| `@farik/web` | `apps/web` | phase 7, not planned |

- Event kinds and commands grow with the code. The step that first emits an event kind, or first handles a command, adds it to the schema. The list in `docs/SPEC.md` section 8.5 is the checklist, and phase 6 step 07 confirms every kind in it exists. Rejected: declaring every kind up front, because a kind nobody emits is a placeholder.
- Tests are split in three. Unit tests are co-located and run by `pnpm check`. Integration tests (`tests/<subject>.integration.test.ts` in the package) need Docker, a git binary, or the file system in ways a unit test must not, run by `pnpm check:integration`, and run in CI as a second job of the same `check` workflow from the step that adds the first one (phase 2 step 04). Live tests (`tests/live/<subject>.live.test.ts`) talk to the model API, cost money, are run by hand with `FARIK_LIVE_TESTS=1`, and never run in CI; phase 3 step 05 adds the row for them to `docs/standards/code.md` when it adds the first one.
- Every function that crosses a package boundary and can fail returns a `Result`, including the asynchronous ones: a store read, a git command, a file read, a command execution all return `Promise<Result<Value, Failure>>` (`docs/standards/code.md`, "Errors are values at package boundaries"). A non-zero exit code from a command is a value, not a failure.
- Time, randomness, and identifiers are injected. Nothing in `core`, `protocol`, `store`, or `runtime` reads the clock or generates an identifier on its own; a `Clock` (`now(): Date`) and an `IdSource` (`sessionId(): string`) are passed in. Task ids are `FRK-<n>` from a counter in the store; session ids are UUIDs from `crypto.randomUUID()` behind `IdSource`; event `recorded_at` fields are ISO 8601 UTC from the clock.
- Only the phase branch's pull request accepts work; a step's review is recorded on that pull request as the step lands (`docs/standards/workflow.md` stage 5).

## Phase 0: Foundation

Ends with: an empty monorepo where `pnpm check` runs typecheck, lint, format check, and tests in CI, and where the task contract schema produces TypeScript types and a validator.

Decisions for this phase, all made here (the step plans carry the exact files):

- Node version: 24.21.0, the current LTS line (Krypton) on the day this revision was written, pinned in `.nvmrc` and in `engines` as `>=24.21.0 <25`. Node 26 is not LTS until October 2026. Rejected: Node 26, because the toolchain's `engines` fields do not all admit it yet.
- pnpm 12.4.1 through the `packageManager` field; TypeScript 7.0.2; Biome 2.5.13; Vitest 5.0.0 with Vite 8.3.0 as its peer; lefthook 2.1.14; Changesets 3.0.3; `@types/node` 24.13.4; `json-schema-to-typescript` 16.0.0; `ajv` 8.20.0; `ajv-formats` 3.0.1. Each was installed together and `pnpm check` run green on 2026-09-14 before this revision was written.
- Module system: ECMAScript modules only, `"type": "module"` in every package. Packages export their TypeScript source (`"exports": { ".": "./src/index.ts" }`); there is no build step until a step needs a runnable artifact (phase 2 step 06 adds one for the command line). Rejected: building every package to `dist/` from day one, because nothing consumes the output yet.
- Packages created in this phase: `@farik/core` only.
- Schema to types: as in the every-phase decision above. Rejected: Zod as source of truth, because the schema is a published artifact that other tools will read.
- `Result` type: a hand-written discriminated union in `packages/core/src/result.ts` (`{ ok: true, value } | { ok: false, error }`) with `ok()`, `err()`, `isOk()`, `isErr()`, `map()`, `mapErr()`, `andThen()`. Generic parameters are named `Value` and `Failure`, not `Error`, so the built-in `Error` is not shadowed. Rejected: a third-party result library, because the surface needed is small and the type is on every package boundary.
- Continuous integration: one GitHub Actions workflow named `check`, on pull requests and pushes to `main`, Ubuntu runner, `pnpm install --frozen-lockfile` then `pnpm check`.
- Commit hook scope: lefthook pre-commit runs `biome check` on staged files only; commit-msg runs `scripts/check-commit-message.ts` (Node runs TypeScript directly), which validates the Conventional Commits shape with one regular expression and accepts merge and revert commits; no commitlint dependency. Build scripts of dependencies are off by default in pnpm 12; only `lefthook` is allowed to run its install script, through `allowBuilds` in `pnpm-workspace.yaml`.
- `pnpm lint` also runs `scripts/check-todos.ts`, so that the code standard's rule that a bare `TODO` fails lint is true from the first commit.
- Scripts under `scripts/` are TypeScript run directly by Node, typechecked by `tsc` through `scripts/tsconfig.json`, and tested by Vitest as a project named `scripts`.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Monorepo scaffold | 8.1, ADR 0002 | Root workspace, tooling config, `pnpm check`, commit hooks, CI workflow, `LICENSE`, `@farik/core` with one passing smoke test |
| 02 | Result type | code standard, "Function that may fail" | `packages/core/src/result.ts` with tests |
| 03 | Contract schema types | 3 (Contract), F4 | `pnpm generate`, generated wire types and schema module, `camelCase` domain types, `validateContract`, `contractToWire`, fixtures, tests against valid and invalid inputs |

Interfaces this phase adds:

- Step 01: `CORE_PACKAGE_NAME: '@farik/core'` from `@farik/core`; `checkCommitMessage(message: string): { ok: true } | { ok: false; reason: string }` and `findBareTodos(files: ReadonlyArray<{ path: string; text: string }>): string[]` in `scripts/`.
- Step 02: `type Result<Value, Failure> = Ok<Value> | Err<Failure>`; `ok`, `err`, `isOk`, `isErr`, `map`, `mapErr`, `andThen`.
- Step 03: `type TaskContractWire` (generated), `type TaskContract`, `type TaskId`, `type TaskStatus`, `type Role`, `type AgentRole`, `type Risk`, `type ExitCriterion`, `type Verification` (union of `command`, `test`, `artifact`, `review`, `human`), `type VerificationMethod`, `type Budget`, `type Notes`, `type NonEmpty<Item>`; `validateContract(input: unknown): Result<TaskContract, readonly ValidationError[]>`; `contractToWire(contract: TaskContract): TaskContractWire`; `DEFAULT_MAX_SESSIONS`, `DEFAULT_MAX_ITERATIONS`, `DEFAULT_EXPECTED_EXIT_CODE`; fixture builders `aContractWire(overrides)` and `aFullContractWire()`; `GENERATED_SCHEMAS` and `generateTypes`, `generateSchemaModule` in `scripts/generate.ts`.

## Phase 1: Harness core

Ends with: `@farik/core` implements every rule in `docs/SPEC.md` section 5 as pure functions, with a test for every row of the transition table, one for every refusal, one for every Definition of Ready and Definition of Done rule, and one for every budget.

Decisions for this phase:

- Made: the transition table is data (an array of rows), not code branches, so that the test suite can iterate it and the documentation can be generated from it. A row is `{ from, to, actor, gate }`; `from` and `to` may be `'any'`; `actor` is who may request the transition: one of the five agent roles by relationship (`product_manager`, `scrum_master`, `assignee`, `reviewer`), `governor` (only the orchestrator, from observed facts, never an agent's tool call), or `human`. Rows whose spec trigger reads "Governor" carry actor `governor`.
- Made: the governor is a set of pure functions taking `(request, context)` and returning `Result<Decision, Refusal>`; it never mutates. Every gate in the table is its own function, built before the function that composes them, so that no step stubs a gate for a later step.
- Made: `verifying → accepted` is requested by the Product Manager with the reviewer's independent criterion results in the context; that is how "reviewer, then Product Manager" is expressed. `any → escalated` on a user `stop` is requested by `human` with gate `none`; on budget or permission it is requested by `governor`.
- Made: the Definition of Ready judgment checks are a fixed rubric in `core` (`JudgmentReview`: fits budget, criteria detect the failure the intent worries about, each with a written reason) recorded as a `review.recorded` event by the runtime. Whether the DoR gate requires the rubric is a field of the readiness context (`requiresJudgmentReview`), set by the orchestrator from the team's composition; `core` does not know whether a Scrum Master exists. The dependencies-ready check is mechanical: `core` receives the statuses of the listed dependencies. Register D2 decides the default for teams that have a Scrum Master.
- Made: cost is computed from a price table shipped as the TypeScript module `packages/core/src/pricing/prices.ts` (so that `core` reads no file at runtime), whose shape is `docs/schemas/prices.schema.json` with `version`, `source_url`, `retrieved_at`, and `prices` keyed by model id; a test validates the shipped table against the schema. The user overrides it with `.farik/prices.json`, read and validated by the store (phase 2 step 05) and passed to the runtime (phase 3 step 06). The numbers are copied from the provider's published table on the day step 05 executes and pasted into the step plan.
- Made: `refining → escalated` has two rows, one per spec trigger: readiness failed three times (`readiness_exhausted`), and the contract itself requires human acceptance (`contract_requires_human`, when risk is `high` or the team policy says every contract is accepted by a human). The transition table has 18 rows: the spec's 16 lines, with `blocked → in_progress` and `any → escalated` each split by actor and `refining → escalated` split by trigger.
- Made: human approval of an `external_effect` tool call is per call. A call is identified by its tool name and a hash of its input; the governor allows it only when that pair is in the context's approved list, which the human fills through a command (phase 6 step 01 adds the command and events; until then no tool carries the tier).
- Made: the path check for `allowed_paths` operates on a list of changed paths passed in, never on git itself, and uses `picomatch` for glob matching. Rejected: a hand-written matcher, because glob edge cases are where safety bugs live and this check is on the safety path (ADR 0004).
- Open: default budget numbers per role (spec 5.5 gives team-level defaults only): register D3. Blocks step 05.
- Open: whether a human may cancel a task from any state or only from `escalated`: register D4. Blocks step 01.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Task status and transition table | 5.2 | `TaskStatus` list, the table from spec 5.2 as 18 data rows, lookup functions, tests that the table has exactly those rows and that every status is reachable |
| 02 | Definition of Ready | 5.3 | Structural checks and the judgment rubric as one function over a contract and a context, one test per rule |
| 03 | Allowed paths check | 5.4 item 2, 5.6 | `checkAllowedPaths(changedPaths, allowedGlobs)` |
| 04 | Permission tiers | 5.6 | Tier list, role defaults, `evaluateToolCall` over a tool descriptor, an agent's grants, and the task's allowed paths |
| 05 | Budgets and cost | 5.5, 10 | Session limits, usage ledger, price table, `computeCostUsd`, `checkBudgets` with the consequence per budget |
| 06 | Iteration and escalation rules | 5.2, 5.7 | Rejection counting, readiness attempt counting, blocked-age rule, the `Escalation` record |
| 07 | Definition of Done | 5.4 | `evaluateDone` over reviewer results, changed paths, notes, risk, and human acceptance, one test per rule |
| 08 | Gate predicates | 5.2 | Assignment, criteria recorded, blocker written, blocker resolved, rejection reasons, as functions returning `GateResult` |
| 09 | Transition evaluation | 5.2, F5 | `evaluateTransition` composing the table, the actor check, and every gate; a test per row that it applies and a test per row that it refuses; a test that a non-row is refused |

Interfaces this phase adds (all in `@farik/core`, all pure):

- Step 01: `TASK_STATUSES: readonly TaskStatus[]`; `type TransitionActor = 'product_manager' | 'scrum_master' | 'assignee' | 'reviewer' | 'governor' | 'human'`; `type GateId = 'none' | 'definition_of_ready' | 'readiness_exhausted' | 'contract_requires_human' | 'assignment' | 'criteria_recorded' | 'blocker_written' | 'blocker_resolved' | 'blocked_age' | 'definition_of_done' | 'rejection_reasons' | 'iteration_below_limit' | 'iteration_limit_reached' | 'governor_escalation'`; `type TransitionRow = { readonly from: TaskStatus | 'any'; readonly to: TaskStatus | 'any'; readonly actor: TransitionActor; readonly gate: GateId }`; `TRANSITION_TABLE: readonly TransitionRow[]`; `findTransitions(from: TaskStatus, to: TaskStatus): readonly TransitionRow[]`; `transitionsFrom(status: TaskStatus): readonly TransitionRow[]`; `isTerminal(status: TaskStatus): boolean` (`accepted`, `cancelled`).
- Step 02: `type ReadinessRule = 'intent_present' | 'criteria_present' | 'criteria_methods_valid' | 'command_criteria_complete' | 'budget_within_sprint' | 'reviewer_differs' | 'risk_set' | 'out_of_scope_present' | 'dependencies_ready' | 'judgment_recorded' | 'judgment_fits_budget' | 'judgment_criteria_detect_failure'`; `type JudgmentReview = { readonly fitsBudget: boolean; readonly criteriaDetectFailure: boolean; readonly reason: string }`; `type ReadinessContext = { readonly remainingSprintBudgetUsd: number; readonly dependencyStatuses: Readonly<Record<string, TaskStatus>>; readonly requiresJudgmentReview: boolean; readonly judgmentReview: JudgmentReview | null }`; `type ReadinessFailure = { readonly rule: ReadinessRule; readonly message: string }`; `type ReadinessResult = { readonly ok: true } | { readonly ok: false; readonly failures: readonly ReadinessFailure[] }`; `evaluateReadiness(contract: TaskContract, context: ReadinessContext): ReadinessResult`.
- Step 03: `type PathCheck = { readonly ok: true } | { readonly ok: false; readonly violations: readonly string[] }`; `checkAllowedPaths(changedPaths: readonly string[], allowedGlobs: readonly string[]): PathCheck`.
- Step 04: `type PermissionTier = 'read' | 'write_workspace' | 'execute' | 'network' | 'git_local' | 'git_remote' | 'external_effect'`; `PERMISSION_TIERS: readonly PermissionTier[]`; `DEFAULT_TIERS_BY_ROLE: Readonly<Record<AgentRole, readonly PermissionTier[]>>`; `type ToolDescriptor = { readonly name: string; readonly tier: PermissionTier }`; `type ToolCallRequest = { readonly tool: ToolDescriptor; readonly paths: readonly string[]; readonly inputHash: string }`; `type AgentGrants = { readonly tiers: readonly PermissionTier[]; readonly preauthorizedExternalTools: readonly string[] }`; `type ApprovedCall = { readonly tool: string; readonly inputHash: string }`; `type ToolCallContext = { readonly allowedPaths: readonly string[]; readonly approvedCalls: readonly ApprovedCall[] }`; `type ToolRefusal = { readonly reason: 'tier_not_granted' | 'path_outside_allowed' | 'requires_human_approval'; readonly tier: PermissionTier; readonly detail: string }`; `evaluateToolCall(request: ToolCallRequest, grants: AgentGrants, context: ToolCallContext): Result<{ readonly allowed: true }, ToolRefusal>`.
- Step 05: `type SessionLimits = { readonly maxInputTokens: number; readonly maxOutputTokens: number; readonly maxWallClockMs: number; readonly maxToolCalls: number }`; `DEFAULT_SESSION_LIMITS`; `DEFAULT_SESSION_LIMITS_BY_ROLE: Readonly<Record<AgentRole, SessionLimits>>` (numbers from D3); `type Usage = { readonly inputTokens: number; readonly outputTokens: number; readonly cacheReadTokens: number; readonly cacheWriteTokens: number }`; `type SessionLedger = { readonly usage: Usage; readonly wallClockMs: number; readonly toolCalls: number; readonly costUsd: number }`; `addUsage(ledger: SessionLedger, usage: Usage, costUsd: number): SessionLedger`; `type PriceTableWire` (generated from `prices.schema.json`: `version`, `source_url`, `retrieved_at`, `prices[modelId] = { input_per_million, output_per_million, cache_read_per_million, cache_write_per_million }`), `type PriceTable` (the `camelCase` domain shape), `validatePriceTable(input: unknown): Result<PriceTable, readonly ValidationError[]>`, `PRICE_TABLE: PriceTable`; `computeCostUsd(usage: Usage, modelId: string, prices: PriceTable): Result<number, { readonly reason: 'unknown_model'; readonly modelId: string }>`; `type BudgetScope = 'session_tokens' | 'session_wall_clock' | 'session_tool_calls' | 'task_usd' | 'sprint_usd' | 'day_usd'`; `type BudgetState = { readonly session: SessionLedger; readonly sessionLimits: SessionLimits; readonly taskSpentUsd: number; readonly taskMaxUsd: number; readonly sprintSpentUsd: number; readonly sprintMaxUsd: number; readonly daySpentUsd: number; readonly dayMaxUsd: number }`; `type BudgetConsequence = 'end_session_and_block_task' | 'escalate_task' | 'stop_new_assignments' | 'pause_team'`; `checkBudgets(state: BudgetState): { readonly exhausted: null } | { readonly exhausted: BudgetScope; readonly consequence: BudgetConsequence }`, which evaluates scopes in the order `session_tokens`, `session_wall_clock`, `session_tool_calls`, `task_usd`, `sprint_usd`, `day_usd` and reports the first exhausted one.
- Step 06: `type EscalationReason = 'budget' | 'iterations' | 'blocker_age' | 'permission' | 'risk_gate' | 'readiness_failures' | 'explicit_request'`; `type Escalation = { readonly taskId: TaskId; readonly reason: EscalationReason; readonly tried: string; readonly options: readonly string[] }`; `evaluateRejection(iteration: number, maxIterations: number): 'return_to_in_progress' | 'escalate'`; `evaluateReadinessAttempts(failedAttempts: number): 'retry' | 'escalate'` (limit 3 per spec 5.2); `evaluateBlockedAge(blockedAtMs: number, nowMs: number, limitMs: number): 'within_limit' | 'exceeded'`.
- Step 07: `type CriterionResult = { readonly criterionId: string; readonly passed: boolean; readonly evidence: string; readonly runBy: 'assignee' | 'reviewer' | 'human' }`; `type DoneEvidence = { readonly reviewerResults: readonly CriterionResult[]; readonly changedPaths: readonly string[]; readonly completionNote: string | null; readonly reviewNote: string | null; readonly humanAccepted: boolean }`; `type DoneRule = 'criterion_run_by_reviewer' | 'criterion_passed' | 'human_criterion_accepted' | 'paths_within_allowed' | 'completion_note_present' | 'review_note_present' | 'high_risk_human_accepted'`; `type DoneResult = { readonly ok: true } | { readonly ok: false; readonly failures: readonly { readonly rule: DoneRule; readonly message: string }[] }`; `evaluateDone(contract: TaskContract, evidence: DoneEvidence): DoneResult`.
- Step 08: `type GateResult = { readonly ok: true } | { readonly ok: false; readonly reasons: readonly string[] }`; `checkAssignment(contract: TaskContract, input: { readonly assigneeRole: AgentRole; readonly assigneeInProgressCount: number; readonly wipLimit: number; readonly remainingSprintBudgetUsd: number }): GateResult`; `checkCriteriaRecorded(contract: TaskContract, assigneeResults: readonly CriterionResult[]): GateResult`; `checkBlockerWritten(blocker: { readonly description: string; readonly needed: string } | null): GateResult`; `checkBlockerResolved(resolution: string | null): GateResult`; `checkRejectionReasons(contract: TaskContract, rejection: { readonly failedCriterionIds: readonly string[]; readonly reasons: string } | null): GateResult`.
- Step 09: `type TransitionRequest = { readonly taskId: TaskId; readonly to: TaskStatus; readonly actor: { readonly kind: TransitionActor; readonly agentId: string | null } }`; `type TransitionContext = { readonly status: TaskStatus; readonly contract: TaskContract; readonly readiness: ReadinessContext; readonly readinessFailedAttempts: number; readonly contractRequiresHumanAcceptance: boolean; readonly contractHumanAccepted: boolean; readonly assignment: Parameters<typeof checkAssignment>[1] | null; readonly assigneeResults: readonly CriterionResult[]; readonly blocker: Parameters<typeof checkBlockerWritten>[0]; readonly blockerResolution: string | null; readonly blockedAtMs: number | null; readonly nowMs: number; readonly blockedLimitMs: number; readonly done: DoneEvidence; readonly rejection: Parameters<typeof checkRejectionReasons>[1]; readonly budget: BudgetState; readonly permissionDenied: boolean }`; `type TransitionEffect = 'increment_iteration' | 'raise_escalation' | 'reset_blocker'`; `type TransitionDecision = { readonly from: TaskStatus; readonly to: TaskStatus; readonly row: TransitionRow; readonly effects: readonly TransitionEffect[] }`; `type TransitionRefusal = { readonly reason: 'no_such_transition' | 'actor_not_allowed' | 'gate_failed'; readonly gate: GateId | null; readonly details: readonly string[] }`; `evaluateTransition(request: TransitionRequest, context: TransitionContext): Result<TransitionDecision, TransitionRefusal>`.

## Phase 2: Protocol, store, and the first command line

Ends with: on any git repository, `farik init` creates `.farik/` and the event log, `farik task create` files a draft task from a YAML contract, `farik board` and `farik log` show it from projections and from the log, and `farik doctor` reports drift between the files and the log. No agents run yet.

Decisions for this phase:

- Made: SQLite through `@libsql/client` (0.18.0) for the event log and projections (spec 8.4). One database file, `.farik/local/farik.db`, holding the `events` table and the projection tables. Migrations are SQL files in `packages/store/src/migrations/`, applied on open, recorded in `schema_migrations`.
- Made: the event log lives under `.farik/local/` and is not committed; the files under `.farik/` are. Reconciliation on startup compares the two and reports every difference as a `drift.detected` event and a `farik doctor` line; `farik doctor --adopt` imports file-only contracts into the log as `task.created` and `contract.written` events. Nothing is picked silently. Register D5 asks the founder to confirm that the log is machine-local.
- Made: `.farik/` layout, fixed here for every later phase:

  ```
  .farik/
    team.yaml                  team, agents, grants, budgets, policy     schema: team.schema.json
    project.md                 the project scan
    contracts/FRK-<n>.yaml     one contract per task, snake_case
    agents/<agent_id>/memory.md
    team/retro.md
    decisions/                 (written from phase 4 step 05)
    sprints/S<n>.yaml          (written from phase 4 step 02)
    local/                     gitignored on init
      farik.db                 event log and projections
      settings.yaml            machine-specific: sandbox mode
      daemon.json              (written from phase 5 step 01)
  ```

  `.farik/prices.json`, when present, overrides the shipped price table (spec 5.5) and is validated against `prices.schema.json` on read.

- Made: YAML through the `yaml` package (2.9.1), ISC licensed. Contracts are written with the schema's key order and two-space indentation so that diffs are stable.
- Made: the git adapter lives in `@farik/store` under `src/git/`, because the project repository is storage the team reads and writes, and `docs/SPEC.md` section 8.1's one-line description of `store` is amended in the same step to say so. Git is driven through the `git` binary with `child_process.execFile`, never a JavaScript reimplementation. Rejected: a separate `@farik/git` package, because the spec fixes the package list.
- Made: the command line is `farik`, in `apps/cli`, built with `commander` (15.0.0), bundled with `tsup` (8.5.1) to `dist/farik.js`, run in development with `tsx` (4.23.13). Output is plain text tables; `--json` prints the same data as JSON for scripts.
- Made: the team model. An agent has `id` (kebab-case slug of the display name, unique in the team), `display_name`, `role`, `persona`, `avatar` (a name from the shipped set until phase 5 adds uploads), `status` (`active`, `paused`, `retired`), `model` (`id`, `effort`), `grants` (tiers), `preauthorized_external_tools`, and later `mcp_servers` and `skills` (added by phase 6 steps 01 and 02). A team has `name`, `agents`, `budgets` (`daily_usd`, `session` limits), and `policy` (`human_accepts_contracts`, `wip_limit_per_agent`, `blocked_limit_hours`, `max_iterations`). The schema enforces three to seven agents.
- Made: the protocol package is `@farik/protocol` and holds the event envelope, the event kinds, the commands, and the mapping layer for both; it depends on `@farik/core` for the contract types and on nothing else.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Protocol package | 8.5 | `event.schema.json` and `command.schema.json`, `@farik/protocol` with generated wire types, `FarikEvent`, the kinds this phase emits, `Command`, mapping functions, `createEvent` |
| 02 | Event log | 5.1 (every action is an event), 8.4 | `@farik/store`: append-only log in SQLite, read by sequence, filter by task, agent, and kind, subscribe |
| 03 | Projections | 8.4, 10 | Board and cost projections rebuilt from the log and updated per event, with a cursor |
| 04 | Git adapter | 8.3, F2 | `isRepository`, `headSummary`, `defaultBranch`, `changedPaths`, `diff`, `createBranch`, `currentBranch`; the first integration test and the CI job for `pnpm check:integration` |
| 05 | Team and project files | 3 (Team, Agent, Project), 5.8, 8.4, 8.6, F2 | `team.schema.json`, `.farik/` initialization, contract, memory, and scan file adapters, the project scan, reconciliation |
| 06 | Command line | F2, F3 (create by hand), F11 (log) | `apps/cli`: `farik init`, `farik task create`, `farik task show`, `farik board`, `farik log`, `farik doctor` |

Interfaces this phase adds:

- Step 01 (`@farik/protocol`): `type EventKind` (this phase: `'task.created' | 'contract.written' | 'drift.detected' | 'project.scanned' | 'team.updated'`); `type EventEnvelope = { readonly seq: number; readonly recordedAt: string; readonly teamId: string; readonly projectId: string; readonly taskId: TaskId | null; readonly agentId: string | null; readonly sessionId: string | null }`; `type FarikEvent = EventEnvelope & EventBody` where `EventBody` is the discriminated union on `kind` with one `payload` type per kind; `type NewEvent = Omit<FarikEvent, 'seq'>`; `createEvent(input: { readonly kind; readonly payload; readonly recordedAt: string; readonly teamId; readonly projectId; readonly taskId?; readonly agentId?; readonly sessionId? }): NewEvent`; `eventFromWire(wire: FarikEventWire): Result<FarikEvent, readonly ValidationError[]>`; `eventToWire(event: FarikEvent): FarikEventWire`; `type Command` (this phase: `{ kind: 'task.create'; contract: TaskContract }`); `commandFromWire`, `commandToWire`; `type Clock = { now(): Date }`; `type IdSource = { sessionId(): string }`.
- Step 02 (`@farik/store`): `openEventLog(options: { readonly url: string }): Promise<EventLog>`; `type EventLog = { append(event: NewEvent): Promise<Result<FarikEvent, StoreError>>; read(query: EventQuery): Promise<Result<readonly FarikEvent[], StoreError>>; subscribe(listener: (event: FarikEvent) => void): () => void; close(): Promise<void> }`; `type EventQuery = { readonly afterSeq?: number; readonly taskId?: TaskId; readonly agentId?: string; readonly kinds?: readonly EventKind[]; readonly limit?: number }`; `type StoreError = { readonly reason: 'io' | 'invalid_event'; readonly detail: string }`; `nextTaskId(log: EventLog): Promise<Result<TaskId, StoreError>>` backed by a `task_counters` table.
- Step 03: `type TaskProjection = { readonly taskId: TaskId; readonly status: TaskStatus; readonly title: string; readonly assigneeId: string | null; readonly reviewerId: string | null; readonly risk: Risk; readonly sprintId: string | null; readonly iteration: number; readonly costUsd: number; readonly updatedSeq: number }`; `type CostScope = 'task' | 'agent' | 'session' | 'sprint' | 'day'`; `type CostProjection = { readonly scope: CostScope; readonly key: string; readonly usd: number; readonly inputTokens: number; readonly outputTokens: number }`; `openProjections(log: EventLog): Promise<Projections>`; `type Projections = { rebuild(): Promise<Result<void, StoreError>>; apply(event: FarikEvent): Promise<Result<void, StoreError>>; board(): Promise<Result<readonly TaskProjection[], StoreError>>; task(taskId: TaskId): Promise<Result<TaskProjection | null, StoreError>>; costs(scope: CostScope): Promise<Result<readonly CostProjection[], StoreError>>; cursor(): Promise<Result<number, StoreError>> }`.
- Step 04: `type GitError = { readonly reason: 'not_a_repository' | 'command_failed'; readonly detail: string }`; `type Git = { isRepository(): Promise<boolean>; headSummary(): Promise<Result<{ readonly sha: string; readonly committedAt: string; readonly subject: string } | null, GitError>>; defaultBranch(): Promise<Result<string, GitError>>; currentBranch(): Promise<Result<string, GitError>>; createBranch(name: string, from: string): Promise<Result<void, GitError>>; changedPaths(base: string, head: string): Promise<Result<readonly string[], GitError>>; diff(base: string, head: string): Promise<Result<string, GitError>> }`; `openGit(root: string): Git`. Phase 3 step 03 widens `Git` with `commit` and `push`.
- Step 05: `type Team`, `type Agent`, `type TeamPolicy`, `type TeamBudgets` (domain), `type TeamWire` (generated); `validateTeam(input: unknown): Result<Team, readonly ValidationError[]>`; `teamToWire`; `type FilesError = { readonly reason: 'not_found' | 'invalid' | 'io'; readonly path: string; readonly detail: string }`; `type ProjectFiles = { init(input: { readonly team: Team }): Promise<Result<void, FilesError>>; readTeam(): Promise<Result<Team, FilesError>>; writeTeam(team: Team): Promise<Result<void, FilesError>>; readContract(taskId: TaskId): Promise<Result<TaskContract, FilesError>>; writeContract(contract: TaskContract): Promise<Result<void, FilesError>>; listContracts(): Promise<Result<readonly TaskId[], FilesError>>; readMemory(agentId: string): Promise<Result<string, FilesError>>; writeMemory(agentId: string, text: string): Promise<Result<void, FilesError>>; readProjectScan(): Promise<Result<string | null, FilesError>>; writeProjectScan(text: string): Promise<Result<void, FilesError>>; readPrices(): Promise<Result<PriceTable | null, FilesError>>; readSettings(): Promise<Result<LocalSettings, FilesError>>; writeSettings(settings: LocalSettings): Promise<Result<void, FilesError>> }`; `openProjectFiles(root: string): ProjectFiles`; `type LocalSettings = { readonly sandbox: 'docker' | 'none' }`; `scanProject(root: string, git: Git): Promise<Result<string, FilesError | GitError>>` (the read-back paragraph: language, package manager, packages, test runner, last commit); `reconcile(files: ProjectFiles, log: EventLog): Promise<Result<readonly Drift[], FilesError | StoreError>>`; `type Drift = { readonly kind: 'contract_without_events' | 'events_without_contract' | 'status_mismatch'; readonly taskId: TaskId; readonly detail: string }`.
- Step 06 (`@farik/cli`): the commands above; `runCli(argv: readonly string[], io: { readonly stdout: Writable; readonly stderr: Writable; readonly cwd: string; readonly clock: Clock }): Promise<number>` so that the command line is tested without spawning a process.

## Phase 3: Runtime and Milestone 0

Ends with: Milestone 0. On a public repository, a Product Manager agent writes contracts for three real issues, a Developer agent implements them, the Product Manager verifies, and a human reviewing the diffs and the event log agrees each task was done as contracted. Command line only.

Decisions for this phase:

- Made, pending the founder's acceptance of ADR 0004: the runtime adapter is the Claude Agent SDK (`@anthropic-ai/claude-agent-sdk`, version pinned in the step 05 plan on the day it executes); governor decisions run in `PreToolUse` hooks; usage is recorded from the SDK's result messages and tool events in `PostToolUse` hooks (spec 8.2). Sessions run on the host; commands run through `farik_exec`; the SDK's Bash tool is never allowed; its WebFetch and WebSearch tools are allowed under `network`; git commits and pushes are Farik tools on the host under `git_local` and `git_remote` (ADR 0004).
- Made: `farik_exec` refuses a command whose first word is `git`, so that commits go through `farik_git` and its tier check; the container never holds git credentials, so a push from inside it cannot succeed whatever the command. The residual, a git command hidden behind a shell wrapper inside the container, is accepted and recorded in ADR 0004.
- Made: Farik tool inputs are described with `zod` (4.6.5) because the SDK's in-process MCP server helper takes zod schemas. JSON Schema stays the source of truth for every file and wire format; a tool input is an in-process API and is neither.
- Made: until phase 4 adds sprints, the budget state has no sprint: `budgetState` sets `sprintMaxUsd` to `Number.POSITIVE_INFINITY` and `sprintSpentUsd` to 0, so the `sprint_usd` scope never fires and daily exhaustion fires as `day_usd` with `pause_team`; the readiness context's `remainingSprintBudgetUsd` is the remaining daily budget. Phase 4 step 02 replaces both with the sprint's numbers.
- Made: the reviewer of a task is the agent whose role is the contract's `reviewer_role`; the orchestrator escalates the task with reason `risk_gate` when no active agent has that role. The Product Manager's prompt in this phase says to write `reviewer_role: product_manager` for the Developer's work, per register D7; phase 4 step 01 adds the preference table for teams with more roles.
- Made: sandbox is Docker, one container per task, project mounted at `/workspace`, network off unless the role has `network` (spec 8.3). The image is built from `packages/runtime/sandbox/Dockerfile` (Node 24, git, pnpm, npm, python3) and tagged `farik/sandbox:<runtime package version>`. Containers are named `farik-<project_id>-<task_id>` and discarded on `accepted` or `cancelled`.
- Made: agents act on the harness only through Farik tools (`farik_*`), served to the session as an in-process MCP server. Every request for a transition goes through `evaluateTransition`; the tool returns the refusal text to the agent and the orchestrator logs `transition.refused`.
- Made: assignment is mechanical. `ready → assigned` is requested as actor `scrum_master`; when the team has no active Scrum Master agent, the orchestrator issues that request itself with `agentId: null`, in `ready` order by task id. Register D6 asks the founder to confirm.
- Made: the reviewer's session is fresh and receives the contract, the diff, the completion note, and the criterion tools, never the assignee's transcript (spec 5.4).
- Made: the prompt structure is fixed sections in fixed order: role mandate and forbidden list from `system.md`, the untrusted-content notice (spec 8.6), the agent's persona line, the project scan, the agent's memory, the contract (when there is one), the tool list with tiers, and the closing instruction for the session's purpose. ADR 0005 records it when step 08 starts, because it affects every later role.
- Made: `human_accepts_contracts` is a team policy with values `high_risk` (spec 5.2) and `all`; register D8 decides the shipped default (spec section 12, question 1). The orchestrator computes `contractRequiresHumanAcceptance` from it and from the contract's risk.
- Open: whether the host `Executor` (no-sandbox mode) is exposed to users: register D1. Blocks step 09.
- Open: which public repository is used for the Milestone 0 exit test: register D9. Blocks step 11.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Runtime adapter interface | 8.2, F6 | `@farik/runtime`: `RuntimeAdapter`, `SessionSpec`, `SessionEvent`, and a recorded-response fake that replays fixtures |
| 02 | Executor and sandbox | 8.3 | `Executor` interface, `HostExecutor`, `DockerExecutor` with container lifecycle per task, integration tests |
| 03 | Farik tools | 5.1, 5.2, 5.6, 5.9 (filing tasks) | Tool definitions with tiers and handlers: read task, read board, write contract, request transition, record criterion result, write note, create task, declare blocked, exec, git; `Git` widened with `commit` and `push` |
| 04 | Criterion runner | 5.4, F4 (verification presets) | Runs `command`, `test`, and `artifact` criteria through an executor; `review` and `human` criteria produce the questions to answer |
| 05 | Claude Agent SDK adapter | 8.2, 8.6 | The real adapter: system prompt, built-in tools filtered by tier (Bash never, web tools under `network`), Farik tools as an MCP server, governor hook, usage recording, wall-clock abort; a live test and the `code.md` row for live tests |
| 06 | Cost recording | 5.5, F6 | `cost.recorded` and `budget.exhausted` events from session usage, the four budgets kept in projections |
| 07 | Role package and the two launch roles | 6, 6.1, 6.4 | `role.schema.json`, `@farik/roles` loader, `product_manager` and `software_developer` with `role.yaml`, `system.md`, and one skill each |
| 08 | Prompt assembly | 5.8, 8.2, 8.6 | `assembleSystemPrompt` with the fixed section order; ADR 0005 |
| 09 | Orchestrator | 5.2, 5.4, 5.5, 5.7, F6 | The loop that moves tasks through the lifecycle with sessions, governor decisions, reviewer resolution by role, escalations, and human commands; tested end to end with the recorded adapter |
| 10 | Command line, second part | F6, F11 | `farik run`, `farik plan`, `farik stop`, `farik resolve`, `farik accept`, `farik task show` with events, diff, and cost |
| 11 | Milestone 0 exit | 11 | The exit test on the chosen repository, recorded with the event log export and the written human review in `docs/milestones/m0-exit.md` |

Interfaces this phase adds:

- Step 01 (`@farik/runtime`): `type SessionSpec = { readonly sessionId: string; readonly agentId: string; readonly taskId: TaskId | null; readonly systemPrompt: string; readonly model: { readonly id: string; readonly effort: 'low' | 'medium' | 'high' }; readonly tools: readonly FarikTool[]; readonly builtinTools: readonly string[]; readonly cwd: string; readonly limits: SessionLimits; readonly onToolCall: (call: ToolCallRequest) => Result<{ allowed: true }, ToolRefusal>; readonly initialPrompt: string }` (`FarikTool` is defined in step 03; step 01 defines it as the minimal `{ readonly name: string; readonly tier: PermissionTier }` and step 03 widens it); `type SessionEvent = { kind: 'tool.called'; tool: string; input: unknown } | { kind: 'tool.returned'; tool: string; output: string } | { kind: 'tool.denied'; tool: string; reason: ToolRefusal } | { kind: 'usage.reported'; usage: Usage } | { kind: 'text.produced'; text: string } | { kind: 'ended'; reason: 'completed' | 'aborted' | 'limit' | 'error'; detail: string }`; `type SessionHandle = { readonly sessionId: string; events(): AsyncIterable<SessionEvent>; abort(): Promise<void> }`; `type RuntimeAdapter = { startSession(spec: SessionSpec): Promise<SessionHandle>; resume(sessionId: string, prompt: string): Promise<SessionHandle>; abort(sessionId: string): Promise<void> }`; `createRecordedAdapter(fixtures: Readonly<Record<string, readonly SessionEvent[]>>): RuntimeAdapter`.
- Step 02: `type ExecResult = { readonly exitCode: number; readonly stdout: string; readonly stderr: string; readonly timedOut: boolean }`; `type ExecError = { readonly reason: 'spawn_failed' | 'container_gone'; readonly detail: string }`; `type Executor = { run(command: string, options: { readonly cwd: string; readonly timeoutMs: number; readonly env: Readonly<Record<string, string>> }): Promise<Result<ExecResult, ExecError>> }`; `type SandboxError = { readonly reason: 'docker_unavailable' | 'image_missing' | 'container_failed'; readonly detail: string }`; `createHostExecutor(root: string): Executor`; `createDockerExecutor(input: { readonly projectId: string; readonly taskId: TaskId; readonly root: string; readonly network: boolean; readonly image: string }): Promise<Result<Executor & { discard(): Promise<Result<void, SandboxError>> }, SandboxError>>`; `SANDBOX_IMAGE: string`.
- Step 03: `type ToolError = { readonly reason: 'invalid_input' | 'refused' | 'failed'; readonly detail: string }`; `type FarikTool = { readonly name: string; readonly description: string; readonly tier: PermissionTier; readonly inputSchema: ZodType; readonly handler: (input: unknown, context: ToolContext) => Promise<Result<string, ToolError>> }`; `type ToolContext = { readonly agentId: string; readonly role: AgentRole; readonly taskId: TaskId | null; readonly log: EventLog; readonly files: ProjectFiles; readonly projections: Projections; readonly executor: Executor | null; readonly requestTransition: (request: TransitionRequest) => Promise<Result<TransitionDecision, TransitionRefusal>>; readonly clock: Clock }`; `farikTools(): readonly FarikTool[]` with the tools `farik_read_task`, `farik_read_board`, `farik_write_contract`, `farik_request_transition`, `farik_record_criterion_result`, `farik_write_note`, `farik_create_task`, `farik_declare_blocked`, `farik_exec` (tier `execute`; refuses a command whose first word is `git`), `farik_git` (input `{ action: 'status' | 'diff' | 'commit' | 'push'; message?; paths? }`; `status`, `diff`, and `commit` are tier `git_local`, `push` is tier `git_remote`, each checked as its own `ToolDescriptor`); `Git` gains `commit(message: string, paths: readonly string[]): Promise<Result<{ readonly sha: string }, GitError>>` and `push(remote: string, branch: string): Promise<Result<void, GitError>>`; event kinds added: `task.transitioned`, `transition.refused`, `criterion.recorded`, `note.written`, `contract.evaluated` (payload `{ gate: 'definition_of_ready' | 'definition_of_done'; passed; failures }`).
- Step 04: `runCriterion(criterion: ExitCriterion, executor: Executor, cwd: string, clock: Clock): Promise<CriterionResult | { readonly needs: 'review'; readonly rubric: readonly string[] } | { readonly needs: 'human'; readonly question: string }>`; `runCriteria(contract: TaskContract, executor: Executor, cwd: string, clock: Clock): Promise<readonly Awaited<ReturnType<typeof runCriterion>>[]>`; `detectVerificationPresets(root: string): Promise<readonly { readonly name: string; readonly command: string }[]>` (F2: the project's test and build commands).
- Step 05: `createClaudeAdapter(options: { readonly apiKey: string; readonly clock: Clock }): RuntimeAdapter`; `BUILTIN_TOOL_TIERS: Readonly<Record<string, PermissionTier>>` (Read, Glob, Grep are `read`; Edit, Write, MultiEdit are `write_workspace`; WebFetch and WebSearch are `network`); `DISALLOWED_BUILTIN_TOOLS: readonly string[]` (Bash, and every other built-in tool not in the tier map); event kinds added: `session.started`, `session.ended`, `tool.called`, `tool.returned`, `tool.denied`.
- Step 06: `recordSessionCost(input: { readonly log: EventLog; readonly projections: Projections; readonly sessionId: string; readonly agentId: string; readonly taskId: TaskId | null; readonly modelId: string; readonly usage: Usage; readonly prices: PriceTable; readonly clock: Clock }): Promise<Result<number, StoreError>>`; `budgetState(projections: Projections, team: Team, taskId: TaskId, session: SessionLedger): Promise<Result<BudgetState, StoreError>>` (sprint fields per the phase decision above); `effectivePrices(files: ProjectFiles): Promise<Result<PriceTable, FilesError>>` (the `.farik/prices.json` override or the shipped table); event kinds added: `cost.recorded`, `budget.exhausted`.
- Step 07 (`@farik/roles`): `type RoleDefinition = { readonly id: AgentRole; readonly mandate: string; readonly produces: readonly string[]; readonly forbidden: readonly string[]; readonly defaultTiers: readonly PermissionTier[]; readonly reviewerRole: AgentRole | null; readonly model: { readonly id: string; readonly effort: 'low' | 'medium' | 'high' }; readonly sessionLimits: SessionLimits; readonly systemPrompt: string; readonly skillsDir: string }`; `type RoleError = { readonly reason: 'not_found' | 'invalid'; readonly roleId: string; readonly detail: string }`; `loadRole(roleId: AgentRole): Promise<Result<RoleDefinition, RoleError>>`; `ROLES_DIR: string`.
- Step 08: `assembleSystemPrompt(input: { readonly role: RoleDefinition; readonly agent: Agent; readonly projectScan: string | null; readonly memory: string; readonly contract: TaskContract | null; readonly tools: readonly { readonly name: string; readonly tier: PermissionTier }[]; readonly purpose: 'refine' | 'implement' | 'verify' }): string`; `PROMPT_SECTIONS: readonly string[]` (the fixed headings, in order).
- Step 09: `createOrchestrator(deps: { readonly log: EventLog; readonly projections: Projections; readonly files: ProjectFiles; readonly git: Git; readonly adapter: RuntimeAdapter; readonly executorFor: (taskId: TaskId, network: boolean) => Promise<Result<Executor & { discard(): Promise<void> }, SandboxError>>; readonly roles: (roleId: AgentRole) => Promise<Result<RoleDefinition, RoleError>>; readonly prices: PriceTable; readonly clock: Clock; readonly ids: IdSource }): Orchestrator`; `type CommandError = { readonly reason: 'invalid' | 'refused' | 'not_found'; readonly detail: string }`; `type Orchestrator = { tick(): Promise<Result<TickReport, CommandError | StoreError>>; run(options: { readonly until: 'idle' | 'stopped' }): Promise<Result<void, CommandError | StoreError>>; stop(): Promise<void>; handle(command: Command): Promise<Result<void, CommandError>> }`; `type TickReport = { readonly sessionsStarted: number; readonly transitions: number; readonly escalations: number; readonly idle: boolean }`; `resolveReviewer(team: Team, contract: TaskContract): Agent | null` (by `reviewerRole`); commands added: `task.transition` (human, from `escalated`), `human.accept` (with `subject: 'contract' | 'result'`), `escalation.resolve`, `session.stop`, `run.stop`; event kinds added: `escalation.raised`, `escalation.resolved`, `human.accepted` (with the same `subject`), `review.recorded`.
- Step 10: the commands above in `@farik/cli`; `farik task show --diff` prints the task branch diff from the git adapter.
- Step 11: no code interfaces; `docs/milestones/m0-exit.md` and `docs/milestones/m0-exit.events.jsonl`.

## Phase 4: The team

Ends with: from the command line, a team of five runs a full sprint on the Milestone 0 repository: planning in the channel, tasks contracted, assigned, implemented, reviewed by the Architect, accepted by the Product Manager, a standup summary each tick boundary, a review and a retro at the end, with memory and decisions written to `.farik/`.

Decisions for this phase:

- Made: ambient message allowance is three per agent per sprint (spec 5.9).
- Made: channel and ambient messages use Claude Sonnet 5; task work uses the role's configured model (spec 8.2).
- Made: the channel is stored as `message.posted` events only; the rolling summary is derived, written to `.farik/local/channel-summary.md`, and rebuilt from the log when missing. The summary is produced by a Sonnet call over the messages since the last summary, capped at 2,000 tokens.
- Made: a sprint is `.farik/sprints/S<n>.yaml` with `id`, `started_at`, `ended_at`, `budget_usd`, `task_ids`, `status`; from this step on `budgetState` fills the sprint fields from the open sprint and the readiness context's `remainingSprintBudgetUsd` is the sprint's remainder, replacing the phase 3 values.
- Made: channel sessions carry `farik_create_task`, so the conversational path leads into the governed one (spec 5.9); nothing else in a channel session can change a task.
- Made: the memory size cap is measured as `Math.ceil(characters / 4)` tokens against the 8k default; the agent is told the count and the cap at the top of its memory section and asked to prune when above 80 percent.
- Made: decisions under `.farik/decisions/` are written through a `farik_write_decision` tool available to the Architect and the Product Manager, numbered `NNNN-<slug>.md` from the existing files, and immutable once written; the tool refuses to overwrite.
- Open: default sprint length and what ends a sprint: register D10. Blocks step 02.
- Open: how much of the channel's conversational register to keep: register D11. Does not block any step; it sets what step 03 instruments.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Remaining roles | 6.2, 6.3, 6.5, 5.1 | `scrum_master`, `architect`, `marketing_specialist` with prompts and skills; the reviewer preference table the Product Manager's prompt uses; Definition of Ready judgment sessions for the Scrum Master |
| 02 | Sprints | 3 (Sprint), 5.5, 6.2 (WIP) | Sprint model and files, sprint budget in the assignment gate, WIP limits per agent, `farik sprint start`, `farik sprint end`, `farik sprint show` |
| 03 | Channel | 5.9, F7 | Message model, posting triggers on transitions and mentions, ambient allowance, rolling summary, `farik channel`, `farik say` |
| 04 | Ceremonies and escalation hygiene | 5.7, 5.9, 6.2 | Planning, standup, review, retro as structured channel conversations run by the Scrum Master; escalation digest at sprint start; `escalation.aged` events |
| 05 | Memory | 5.8 | Notebook cap and prune instruction, `team/retro.md` appended by the Scrum Master, `farik_write_decision`, project scan refresh when the tree changes |
| 06 | Milestone 1 team exit | 11 | A recorded sprint on the Milestone 0 repository with five roles, in `docs/milestones/m1-team-exit.md` |

Interfaces this phase adds:

- Step 01: `REVIEWER_ROLE_FOR: Readonly<Record<AgentRole, readonly AgentRole[]>>` (preference order per spec 5.1: Developer's work to Architect then Product Manager; Architect's to Product Manager; Product Manager's contracts to Scrum Master; Marketing's to Product Manager); `defaultReviewerRole(team: Team, assigneeRole: AgentRole): AgentRole | null` (the first preferred role with an active agent), given to the Product Manager's prompt; the orchestrator's readiness context gains `requiresJudgmentReview` true when an active Scrum Master exists (D2); a `farik_record_judgment` tool for the Scrum Master.
- Step 02: `type Sprint = { readonly id: string; readonly startedAt: string; readonly endedAt: string | null; readonly budgetUsd: number; readonly taskIds: readonly TaskId[]; readonly status: 'open' | 'closed' }`; `ProjectFiles` gains `readSprint`, `writeSprint`, `listSprints`; `Projections` gains `sprint(sprintId)`; commands added: `sprint.start`, `sprint.end`; event kinds added: `sprint.started`, `sprint.ended`; `checkAssignment` receives `assigneeInProgressCount` and `wipLimit` from the sprint's board.
- Step 03: `type Message = { readonly channel: 'team'; readonly authorId: string | 'user'; readonly text: string; readonly mentions: readonly string[]; readonly refs: { readonly taskId?: TaskId; readonly eventSeq?: number }; readonly kind: 'reaction' | 'ambient' | 'ceremony' | 'user' }` as the payload of `message.posted`; `postingTriggers(event: FarikEvent, team: Team): readonly { readonly agentId: string; readonly prompt: string }[]`; `ambientAllowance(projections: Projections, sprintId: string, agentId: string): Promise<Result<number, StoreError>>`; `RuntimeAdapter` gains `complete(input: { readonly model: string; readonly prompt: string; readonly maxTokens: number }): Promise<Result<{ readonly text: string; readonly usage: Usage }, RuntimeError>>` for cheap one-shot calls, with `type RuntimeError = { readonly reason: 'api' | 'aborted' | 'limit'; readonly detail: string }`; `summarizeChannel(input: { readonly messages: readonly Message[]; readonly previousSummary: string | null; readonly adapter: RuntimeAdapter }): Promise<Result<string, RuntimeError>>`; command added: `message.post`; tools added to channel sessions: `farik_post_message`, `farik_create_task`.
- Step 04: `type CeremonyKind = 'planning' | 'standup' | 'review' | 'retro'`; `Orchestrator` gains `runCeremony(kind: CeremonyKind, sprintId: string): Promise<Result<readonly Message[], CommandError | RuntimeError>>`; `escalationDigest(projections: Projections, clock: Clock): Promise<Result<string, StoreError>>`; event kind added: `escalation.aged`; `TeamPolicy` gains `escalation_age_hours`.
- Step 05: `memoryBudget(text: string, capTokens: number): { readonly tokens: number; readonly cap: number; readonly overBudget: boolean }`; `farik_write_decision` and `farik_append_retro` tools; `shouldRefreshScan(git: Git, lastScanSha: string): Promise<Result<boolean, GitError>>`; event kind added: `decision.written`.

## Phase 5: Desktop

Ends with: a desktop application where a new user goes from an empty office to an accepted task on their own repository inside thirty minutes, measured with five test users.

Decisions for this phase:

- Made: Tauri 2 shell (`@tauri-apps/cli` 2.11.4) with React 19 (spec 8.1), Vite for the front end.
- Made: the desktop shell never runs the orchestrator in the webview. A local daemon, `farik serve`, runs the orchestrator and exposes a WebSocket on `127.0.0.1` with a random port and a token written to `.farik/local/daemon.json`; the shell spawns it as a sidecar and connects. The wire is JSON-RPC 2.0: `subscribe` streams events from a sequence, `command` sends a `Command`, `query` reads projections. `apps/web` in phase 7 speaks the same protocol to a remote daemon, which is what spec 8.1 means by "the same event protocol".
- Made: contract validation and Definition of Ready results shown in the editor come from the daemon (`contract.validate` command), never from `ajv` in the webview.
- Made: every action in the office is also reachable from the board (F10); the scene can be disabled in settings and the board is the default view when it is.
- Open: how the daemon and its Node runtime are packaged with the desktop app: register D12, with an ADR. Blocks step 03.
- Open: the pixel design system (palette, tile size, font, avatar set): register D13, with an ADR. Blocks step 02.
- Open: renderer for the office scene, PixiJS 8.20.1 or Phaser 4.2.1: register D14, with an ADR. Blocks step 08.
- Open: whether the scene shows cost visually: register D15. Blocks step 08.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | Daemon and transport | 8.1, 8.5, F6 | `rpc.schema.json`, `farik serve`, WebSocket JSON-RPC with subscribe, command, and query; a client library in `@farik/protocol` |
| 02 | Pixel component library | F10, 10 | `@farik/ui`: buttons, panels, lists, dialogs, form fields, tables in the design system; strings externalized |
| 03 | Desktop shell | 8.1 | `apps/desktop`: Tauri app that spawns the daemon, connects, shows connection state, and renders a raw event ticker; settings for scene on or off |
| 04 | Board | F3 | Kanban of the lifecycle, filters by agent, sprint, and risk; task detail with contract, events, diff, notes, and cost; create a task by hand |
| 05 | Contract editor and human gates | F4, 5.2, 5.4 | Schema-driven editor with validation and Definition of Ready results inline; accept a `high` risk contract; resolve an escalation; human acceptance of a result |
| 06 | Channel view | F7 | Team chat with agent and user posts, mentions, ceremony threads, links to tasks and events |
| 07 | Team builder and editor | F1, 4.4 | Create, edit, pause, retire, replace agents; roles, names, avatars from the shipped set or a 32x32 upload, persona lines, permissions, model and effort; three to seven enforced |
| 08 | Office scene | F10, 10 | Desks, meeting table, whiteboard, door; agent movement by state; click to open an agent; 60 frames per second on the reference laptop; disable switch |
| 09 | First-run flow | 4.1, F2 | Existing or new project, scan read-back, team builder, explicit confirmation of `execute`, `git_remote`, and the daily budget; the office populates and the Product Manager's first questions appear |
| 10 | Milestone 1 exit | 11 | The thirty-minute test with five users, protocol and results in `docs/milestones/m1-exit.md` |

Interfaces this phase adds:

- Step 01: `type RpcRequest`, `type RpcResponse`, `type RpcNotification` (generated from `rpc.schema.json`); `createDaemonClient(url: string, token: string): DaemonClient`; `type QueryName = 'board' | 'task' | 'costs' | 'channel' | 'team' | 'sprint' | 'events'`; `type QueryParams = { board: Record<string, never>; task: { taskId: TaskId }; costs: { scope: CostScope }; channel: { afterSeq: number }; team: Record<string, never>; sprint: { sprintId: string | null }; events: EventQuery }`; `type QueryResult = { board: readonly TaskProjection[]; task: TaskProjection | null; costs: readonly CostProjection[]; channel: readonly Message[]; team: Team; sprint: Sprint | null; events: readonly FarikEvent[] }`; `type DaemonError = { readonly reason: 'disconnected' | 'unauthorized' | 'rejected'; readonly detail: string }`; `type DaemonClient = { subscribe(afterSeq: number, listener: (event: FarikEvent) => void): () => void; command(command: Command): Promise<Result<void, CommandError | DaemonError>>; query<Name extends QueryName>(name: Name, params: QueryParams[Name]): Promise<Result<QueryResult[Name], DaemonError>>; close(): Promise<void> }`; command added: `contract.validate`.
- Step 02 (`@farik/ui`): React components `Button`, `Panel`, `List`, `Dialog`, `TextField`, `Select`, `Table`, `Badge`, `Tabs`; `theme.css` tokens; `t(key)` string lookup over `strings/en.json`.
- Step 03 (`@farik/desktop`): the Tauri shell; `useDaemon()` hook exposing the client and connection state.
- Steps 04 to 09: React views `Board`, `TaskDetail`, `ContractEditor`, `EscalationPanel`, `Channel`, `TeamBuilder`, `AgentPanel`, `OfficeScene`, `FirstRun`; commands added: `team.update`, `agent.update`.

## Phase 6: Ecosystem and launch

Ends with: the public open-source release, `v0.1.0`, with per-agent MCP servers and skills, one-on-one conversations, the audit viewer, notifications, and the premium hooks present as stubs.

Decisions for this phase:

- Made: MCP servers are configured per agent in `team.yaml` (`mcp_servers: [{ name, transport: 'stdio' | 'http', command | url, args, env_keys, tool_tiers }]`); credentials are named by key, stored in the OS keychain through `@napi-rs/keyring` (version pinned in the step plan), read by the daemon at connection time, and passed to the server process environment (spec 8.6). Tool listing uses `@modelcontextprotocol/sdk` (1.30.0) as a client; untagged tools are `external_effect` (spec 5.6).
- Made: skills live at three levels, `packages/roles/roles/<role_id>/skills/`, `.farik/skills/`, and `.farik/agents/<agent_id>/skills/`, and are loaded into a session through the SDK's skills support in that order; the agent panel lists them and allows enabling per agent.
- Made: one-on-one conversations are sessions with `read` tier only, no task, and one Farik tool, `farik_propose_task`, which creates a `draft` task for the Product Manager (F8, spec 4.3).
- Made: premium hooks (F13) are interfaces with open-source implementations: `LicenseCheck` returning `{ tier: 'open_source' }`, `HostedRunToggle` disabled with the reason "not available in this build", `SyncProvider` with a no-op implementation. No `ee/` directory exists in this release.
- Made: the release pipeline is a second GitHub Actions workflow, `release`, that builds the desktop app for macOS, Windows, and Linux on a tag, publishes `@farik/core`, `@farik/protocol`, and `@farik/store` to npm through Changesets, and attaches binaries to the GitHub release. The packages stop being `private` in this step.
- Open: desktop notification mechanism per platform: register D16. Blocks step 05.
- Open: launch recording script and repository: register D17. Blocks step 07.

Steps:

| Step | Name | Spec | Delivers |
|---|---|---|---|
| 01 | MCP per agent | F9, 3 (MCP connection), 5.6, 8.6 | Server configuration (stdio and remote), tool listing, tier tagging, keychain credentials, loading into sessions; per-call human approval of `external_effect` tools from the board and from `farik approve` |
| 02 | Skills per agent | F9, 3 (Skill) | Skill folders at agent, role, and team level; loading into sessions; agent panel listing |
| 03 | One-on-one | F8, 4.3 | Read-only direct conversation with an agent, memory and decisions view in the agent panel, offer to file a task |
| 04 | Audit viewer | F11 | Event log view with filters, JSON Lines export, cost reports per task, agent, and sprint |
| 05 | Notifications | F12, 5.7 | Desktop notifications for `escalation.raised`, `escalation.aged`, `sprint.started`, `sprint.ended`; quiet hours |
| 06 | Premium hooks | F13, 9 | `LicenseCheck`, `HostedRunToggle`, `SyncProvider` stubs wired into settings |
| 07 | Launch | 11, product analysis (go to market) | README rewrite, event kind checklist against spec 8.5, changelog, release workflow, recording, tag `v0.1.0` |

Interfaces this phase adds:

- Step 01: `type McpServerConfig` in `Team` (`Agent` gains `mcpServers`); `type McpError = { readonly reason: 'unreachable' | 'protocol' | 'credentials_missing'; readonly detail: string }`; `listMcpTools(config: McpServerConfig, credentials: CredentialSource): Promise<Result<readonly { readonly name: string; readonly description: string }[], McpError>>`; command added: `tool.approve` (`{ approvalId: string; approved: boolean }`); event kinds added: `approval.requested` (payload: the tool, the input hash, a summary of the input), `approval.granted`, `approval.denied`; the orchestrator keeps the session's `approvedCalls` from these events and the agent retries the identical call after approval; `type CredentialSource = { get(key: string): Promise<string | null>; set(key: string, value: string): Promise<void> }`; `createKeychainCredentials(service: string): CredentialSource`; `SessionSpec` gains `mcpServers`.
- Step 02: `Agent` gains `skills`; `resolveSkillDirs(agent: Agent, role: RoleDefinition, root: string): readonly string[]`; `SessionSpec` gains `skillDirs`.
- Step 03: `Orchestrator` gains `startConversation(agentId: string, firstMessage: string): Promise<Result<{ readonly conversationId: string }, CommandError>>`; `farik_propose_task` tool; command added: `conversation.send` (`{ conversationId; text }`); event kinds added: `conversation.started`, `conversation.ended`.
- Step 04: `exportEvents(log: EventLog, query: EventQuery, sink: Writable): Promise<Result<number, StoreError>>`; `costReport(projections: Projections, scope: CostScope): Promise<Result<readonly CostProjection[], StoreError>>`; views `AuditLog`, `CostReport`.
- Step 05: `type Notifier = { notify(input: { readonly title: string; readonly body: string; readonly taskId: TaskId | null }): Promise<void> }`; `TeamPolicy` gains `quiet_hours`; `shouldNotify(event: FarikEvent, policy: TeamPolicy, clock: Clock): boolean`.
- Step 06: `type LicenseCheck = { check(): Promise<{ readonly tier: 'open_source' | 'premium' }> }`; `type HostedRunToggle = { available(): Promise<{ readonly available: false; readonly reason: string } | { readonly available: true }> }`; `type SyncError = { readonly reason: 'unavailable'; readonly detail: string }`; `type SyncProvider = { push(): Promise<Result<void, SyncError>>; pull(): Promise<Result<void, SyncError>> }`; the open-source implementations of each.
- Step 07: no code interfaces; `CHANGELOG.md`, `.github/workflows/release.yml`, the recording, the tag.

## Phase 7: Premium

Not yet planned. Its steps are written after the Phase 6 retrospective, because what the open-source launch teaches decides what hosted execution must do first. `docs/SPEC.md` section 9 lists the candidate features in priority order: hosted execution with included credits, cloud sync, cost analytics beyond thirty days, office themes and avatar packs, priority support, then multi-user teams and SSO. The premium hooks from phase 6 step 06 are the seams it fills, and `apps/web` speaks the phase 5 daemon protocol to a hosted daemon.

## Coverage

Every functional requirement in `docs/SPEC.md` section 7 and every rule in section 5, with the steps that deliver it. A requirement that spans phases lists the step where it becomes usable first.

| Spec | Delivered by |
|---|---|
| 5.2 lifecycle and table | 1.01, 1.09; applied by 3.09 |
| 5.3 Definition of Ready | 1.02; judgment sessions in 4.01 |
| 5.4 Definition of Done | 1.03, 1.07; reviewer sessions in 3.09 |
| 5.5 budgets | 1.05, 3.06; sprint budget in 4.02 |
| 5.6 permissions | 1.04, 3.05; MCP tagging in 6.01 |
| 5.7 escalation | 1.06, 3.09; digest and age in 4.04; notifications in 6.05 |
| 5.8 memory | 2.05 (files), 3.08 (in prompts), 4.05 (cap, retro, decisions, refresh) |
| 5.9 channel | 4.03, 4.04; view in 5.06 |
| F1 team builder | 2.05 (model), 5.07, 5.09 |
| F2 projects | 2.04, 2.05, 2.06; presets in 3.04; first run in 5.09 |
| F3 board | 2.06 (text), 5.04 |
| F4 contracts | 0.03 (validation), 5.05 (editor) |
| F5 governor | phase 1 |
| F6 runtime | 3.01, 3.05, 3.06, 3.09; stream in 5.01 |
| F7 channel | 4.03, 5.06 |
| F8 one-on-one | 6.03 |
| F9 MCP and skills | 6.01, 6.02 |
| F10 pixel office | 5.08 |
| F11 audit | 2.06 (`farik log`), 6.04 |
| F12 notifications | 6.05 |
| F13 premium hooks | 6.06 |
| 8.1 layout | 0.01 and the package table above |
| 8.2 runtime | 3.05, ADR 0004 |
| 8.3 sandbox | 3.02 |
| 8.4 storage | 2.02, 2.03, 2.05 |
| 8.5 event protocol | 2.01 and every step that adds a kind; checked in 6.07 |
| 8.6 security | 3.05 (disallowed tools, untrusted notice), 6.01 (keychain) |
| 10 non-functional | 1.04 (measured, not gated), 2.03 (projections), 5.08 (frame rate), 5.02 (strings) |

## Spec changes this plan implies

Each is made in the step named, in the same pull request as the code, per hard rule 8.

- 8.1: `store` also holds the git adapter (phase 2 step 04).
- 8.2: the SDK's shell tool is replaced by `farik_exec`, the SDK's web tools run only under `network`, git commits and pushes go through `farik_git`, and `events()` lives on the session handle rather than the adapter (phase 3 steps 01, 03, 05, ADR 0004).
- 5.3: the dependencies-ready check is mechanical, not a Scrum Master judgment (phase 1 step 02).
- 5.5: the Scrum Master's session token defaults are lower than the team default (phase 1 step 05, pending D3).
- 5.9: channel sessions carry `farik_create_task` and nothing else that changes a task (phase 4 step 03).
- 8.4: the event log is under `.farik/local/` (phase 2 step 02, pending D5).
- 8.5: the added event kinds (each step that adds one).
- 5.1 and 11: the Milestone 0 reviewer for the Developer's work is the Product Manager when the team has no Architect (phase 3 step 09, pending D7).
- 5.2: the assignment actor when there is no Scrum Master (phase 3 step 09, pending D6).
- 12: each open question moved into the body of the spec as it is decided (register entries D1, D8, D10, D11, D15).

## Decision register

Open decisions, in the order the plan needs them. Each has the question, the options, a recommendation, and what it blocks. Closing one means editing the phase's decision list above (and the spec, where noted) through a pull request that names the entry.

| Id | Question | Options | Recommendation | Blocks |
|---|---|---|---|---|
| D1 | Does no-sandbox mode ship in the first release? (spec 12 question 4; product analysis decision 4) | (a) Ship it, chosen per machine in `.farik/local/settings.yaml`, with a warning printed on every `farik run` and shown in the setup screen. (b) Do not ship; `farik run` refuses without Docker and the host executor stays test-only. | (a). ADR 0004 makes it a configuration switch, the governor's path and permission checks still apply, and it is what Windows users will use first. | 3.09 |
| D2 | When the team has an active Scrum Master, does `refining → ready` require the judgment rubric to be recorded and all yes? | (a) Yes; without a Scrum Master the structural checks alone gate. (b) Judgment is advisory only; it never blocks. | (a). It is the spec's intent in 5.3 and the only way the judgment check has teeth. | 4.01 |
| D3 | Default budgets per role. | Proposed: session tokens 400k in and 40k out for every role except the Scrum Master at 200k and 20k; wall clock 30 minutes; 200 tool calls; task `max_cost_usd` proposed by the PM, capped at 5 dollars unless the human raises it; sprint 15 dollars; day 20 dollars. | Accept the proposed table; it keeps a first day under twenty dollars (spec 10) and is re-derived at 1.05 from the price table. | 1.05 |
| D4 | May a human cancel a task from any state? | (a) Only from `escalated`, as the table says: `stop` then `cancel`, two events. (b) Add a `any → cancelled` row for `human`. | (a). Two events make the audit trail say why. | 1.01 |
| D5 | Is the event log committed to git? | (a) No; it lives in `.farik/local/` and reconciliation reports drift. (b) Yes; commit `farik.db`. | (a). A binary database in git churns every commit; the contracts and decisions are the shareable record. | 2.02 |
| D6 | Who assigns when the team has no Scrum Master? | (a) The orchestrator, as actor `scrum_master` with no agent, in task-id order. (b) The human, through a `farik assign` command. | (a). Assignment is mechanical (role match, WIP, budget); Milestone 0 should not need the human for it. | 3.09 |
| D7 | Who reviews the Developer's work when there is no Architect? | (a) The Product Manager: the PM's prompt says to write `reviewer_role: product_manager` until an Architect is on the team. (b) A second Developer if one exists, else the Product Manager. | (a). It is what section 11 describes for Milestone 0; (b) can be added with the preference table in phase 4 step 01. | 3.07 |
| D8 | Default for `human_accepts_contracts` (spec 12 question 1). | (a) `high_risk`. (b) `all` for the first sprint, then `high_risk`. | (a) for Milestone 0, where every contract is read by a human anyway; revisit with Milestone 0 data. | 3.09 |
| D9 | The public repository for the Milestone 0 exit. | Any public repository with at least three open, small, well-described issues, a test suite that runs in the sandbox image, and a maintainer who will read the diffs. Candidates are the founder's to name. | Prefer a repository the founder maintains, so the human review is by someone who knows the code. | 3.11 |
| D10 | Default sprint length and what ends a sprint (spec 12 question 2). | (a) A sprint ends when its budget is spent or its task list is done, with a wall-clock cap of 8 hours. (b) Time-boxed only, 24 hours. (c) Task-count only. | (a). Budget is the constraint users feel; the cap keeps a stuck sprint from running overnight. | 4.02 |
| D11 | How much conversational register to keep in the channel (spec 12 question 5). | Instrument first: count reaction, ambient, and ceremony messages and their cost per sprint, then decide after Milestone 1. | Keep the spec's defaults (ambient 3) and instrument. | none |
| D12 | How the daemon ships inside the desktop app. | (a) Bundle a Node 24 binary as a Tauri sidecar and the daemon as one bundled file. (b) Require Node on the user's machine. (c) Rewrite the daemon in Rust. | (a). (b) fails the thirty-minute test for non-developers; (c) forks the orchestrator. ADR at 5.03. | 5.03 |
| D13 | The pixel design system. | Tile size 16 px with 32x32 avatars (spec F1), a 32-colour fixed palette, one bitmap font, scene at 2x scale. The founder picks the palette and font. | Decide with an ADR at 5.02; a visual choice the founder should make by looking. | 5.02 |
| D14 | Office scene renderer. | (a) PixiJS 8.20.1. (b) Phaser 4.2.1. | (a). The scene needs sprites, a tile map, and tweened movement, not a game loop with physics; PixiJS is the smaller dependency. ADR at 5.08. | 5.08 |
| D15 | Does the scene show cost visually (spec 12 question 3)? | (a) No; state only. (b) Yes; desk lamp dims with the task budget. | (a) for the first release; the board shows cost. | 5.08 |
| D16 | Desktop notification mechanism. | (a) Tauri's notification plugin on all three platforms. (b) Per-platform native code. | (a). | 6.05 |
| D17 | Launch recording script and repository. | A real repository, a team of five, one sprint from planning to accepted tasks, ending on the event log and the cost report (product analysis, go to market). The founder names the repository and writes the script. | Use the Milestone 1 exit repository if its owner agrees. | 6.07 |

## Changing this plan

Edits go through a pull request that says which decision changed and why. Reordering steps within a phase is a normal edit. Reordering phases, or adding a dependency from an earlier phase on a later one, requires an ADR because it means the no-forward-dependencies rule was about to be broken. Closing a register entry is an edit to the phase's decision list and a removal of the row here.
