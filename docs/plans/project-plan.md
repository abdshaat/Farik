# Farik project plan

Status: draft. Phase 0 is fully decided and its step plans may be written. Later phases carry open decisions, listed under each phase, that must be closed before that phase's first step plan is written.
Owner: project founder.
Last decision change: 2026-09-14, initial draft.

This document divides the project into phases and each phase into steps. A phase ends in something a person can use or verify. A phase is one pull request from the branch `phase/<n>-<name>`. A step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md` and lands as a group of commits on the phase branch. Steps run in the order listed; a step depends only on steps above it in the same phase and on phases already merged. The rules are in `docs/standards/workflow.md` stage 2 (Plan) and ADR 0003.

Phases 0 through 2 together deliver Milestone 0 from `docs/SPEC.md` section 11. Phases 3 and 4 deliver Milestone 1. Phase 5 delivers Milestone 2 and the open-source launch. Phase 6 is Milestone 3.

## Decisions that apply to every phase

Made, with the record:

- Workflow and pull request rules: ADR 0001, `docs/standards/workflow.md`.
- Toolchain: ADR 0002. pnpm workspaces, TypeScript strict, Biome, Vitest, lefthook, Changesets, GitHub Actions, `pnpm check`.
- Planning structure: ADR 0003.
- Naming and wire format rules: `docs/standards/code.md`.
- `packages/core` performs no I/O: `docs/standards/code.md`, hard rule 5 in `CLAUDE.md`.
- Licensing: Apache 2.0 for everything outside `ee/`: `docs/SPEC.md` section 9.
- Package layout: `docs/SPEC.md` section 8.1.
- Model provider for the first release: Claude via the Claude Agent SDK, one adapter, interface not promised stable: `docs/SPEC.md` section 8.2.

## Phase 0: Foundation

Ends with: an empty monorepo where `pnpm check` runs typecheck, lint, format check, and tests in CI, and where the task contract schema produces TypeScript types.

Decisions for this phase, all made here:

- Node version: the current LTS line at the time the scaffold step executes, pinned in `.nvmrc` and `engines`. The step plan names the exact version.
- Module system: ECMAScript modules only, `"type": "module"` in every package.
- Packages created in this phase: `@farik/core` only. Other packages are created by the first step that needs them, so that no empty package sits unused.
- Schema to types: JSON Schema stays the source of truth; TypeScript types are generated with `json-schema-to-typescript` into `packages/core/src/generated/` and committed; validation at the wire boundary uses `ajv` with the 2020-12 dialect. Rejected: Zod as source of truth, because the schema is a published artifact that other tools will read.
- `Result` type: a hand-written discriminated union in `packages/core/src/result.ts` (`{ ok: true, value } | { ok: false, error }`) with `ok()`, `err()`, `map()`, `andThen()`. Rejected: a third-party result library, because the surface needed is small and the type is on every package boundary.
- Continuous integration: one GitHub Actions workflow named `check`, on pull requests and pushes to `main`, Ubuntu runner, `pnpm install --frozen-lockfile` then `pnpm check`.
- Commit hook scope: lefthook pre-commit runs Biome on staged files only; commit-msg validates the Conventional Commits format with a regular expression in a small script, no commitlint dependency.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | Monorepo scaffold | Root workspace, tooling config, `pnpm check`, CI workflow, `@farik/core` with one passing smoke test |
| 02 | Result type | `packages/core/src/result.ts` with tests |
| 03 | Contract schema types | Generated types from `docs/schemas/task-contract.schema.json`, an `ajv` validator behind a `validateContract(input: unknown): Result<TaskContract, ValidationError[]>` function, tests against valid and invalid fixtures |

## Phase 1: Harness core

Ends with: `@farik/core` implements every rule in `docs/SPEC.md` section 5 as pure functions with a test for every transition and every refusal.

Decisions for this phase:

- Made: the transition table is data (an array of rows), not code branches, so that the test suite can iterate it and the documentation can be generated from it.
- Made: the governor is a set of pure functions taking `(state, request, context)` and returning `Result<Decision, Refusal>`; it never mutates.
- Made: cost is computed from a price table file shipped in `packages/core/src/pricing/prices.json`, versioned, user-overridable at the store layer.
- Made: the diff check for `allowed_paths` operates on a list of changed paths passed in, never on git itself.
- Open: whether the Definition of Ready judgment checks (spec 5.3, Scrum Master) are represented in `core` as a recorded review with a fixed rubric, or only as an event the runtime produces. Decide before step 02.
- Open: exact default budget numbers per role (spec 5.5 gives team-level defaults only). Decide before step 05.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | Task status and transition table | `TaskStatus` union, the table from spec 5.2 as data, tests that every row is reachable and every non-row is refused |
| 02 | Definition of Ready | Structural checks from spec 5.3 as a function over a contract, one test per rule |
| 03 | Transition evaluation | `evaluateTransition` combining actor role, gate, and table; refusals carry a reason |
| 04 | Permission tiers | Tier union, role defaults from spec 5.6, `evaluateToolCall` over a tool descriptor and an agent's grants |
| 05 | Budgets and cost | Budget types from spec 5.5, `recordUsage`, `checkBudget`, price table and cost computation |
| 06 | Allowed paths check | `checkAllowedPaths(changedPaths, allowedGlobs)` |
| 07 | Iteration and escalation rules | Rejection counting, blocked-age rule, budget exhaustion to `escalated`, from spec 5.2 and 5.7 |
| 08 | Definition of Done | Acceptance evaluation from spec 5.4 over verification results, path check, notes, and risk |

## Phase 2: Store, runtime, and the command line

Ends with: Milestone 0. On a public repository, a Product Manager agent writes contracts for three real issues, a Developer agent implements them, the PM verifies, and a human reviewing the diffs and the event log agrees each task was done as contracted. No graphical interface.

Decisions for this phase:

- Made: SQLite through libsql for the event log and projections (spec 8.4).
- Made: `.farik/` file layout as in spec 5.8 and 8.4; contracts as YAML, one file per task, named by task id.
- Made: sandbox is Docker, one container per task, project mounted, network off unless the role has `network` (spec 8.3).
- Made: the runtime adapter is the Claude Agent SDK; governor decisions run in `PreToolUse` hooks; usage is recorded in `PostToolUse` hooks (spec 8.2).
- Made: command-line binary is `farik`, in `apps/cli`.
- Open: whether a no-sandbox mode ships (spec section 12, question 4). Decide before step 07.
- Open: the exact prompt structure for the two roles and how memory is spliced in. Decide before step 08, with an ADR because it affects every later role.
- Open: which public repository is used for the Milestone 0 exit test. Decide before step 10.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | Protocol package | `@farik/protocol`: event envelope, event kinds from spec 8.5, command types |
| 02 | Event log | `@farik/store`: append-only log in SQLite, read by sequence, filter by task and agent |
| 03 | Projections | Board, cost per task and agent, channel summary, rebuilt from the log |
| 04 | Project files | `.farik/` adapters for contracts, decisions, memory; reconciliation with the log on startup |
| 05 | Runtime adapter interface | `@farik/runtime`: `startSession`, `resume`, `abort`, `events()`, and a recorded-response fake |
| 06 | Claude Agent SDK adapter | The real adapter with governor hooks and usage recording |
| 07 | Sandbox | Docker container lifecycle per task |
| 08 | Product Manager and Developer roles | `roles/product_manager/`, `roles/software_developer/`, prompts, default tools |
| 09 | Command line | `farik init`, `farik plan`, `farik run`, `farik board`, `farik log` |
| 10 | Milestone 0 exit | The exit test from spec section 11, recorded with the event log and a written human review |

## Phase 3: The team

Ends with: all five roles, the channel, ceremonies, sprints, and memory, still driven from the command line.

Decisions for this phase:

- Made: ambient message allowance is three per agent per sprint (spec 5.9).
- Made: channel and ambient messages use Claude Sonnet 5; task work uses the role's configured model (spec 8.2).
- Open: default sprint length (spec section 12, question 2). Decide before step 05.
- Open: how much of the channel's conversational register to keep; instrument first (spec section 12, question 5). This does not block any step; it sets what step 02 measures.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | Remaining roles | `scrum_master`, `architect`, `marketing_specialist` |
| 02 | Channel | Message model, posting triggers from spec 5.9, rolling summary, rate limits |
| 03 | Ceremonies | Planning, standup, review, retro as structured channel conversations |
| 04 | Memory | Agent notebooks with size cap, team retro file, decisions directory, project scan refresh |
| 05 | Sprints | Sprint model, sprint budget, WIP limits, escalation digest |

## Phase 4: Desktop

Ends with: a desktop application where a new user goes from an empty office to an accepted task on their own repository inside thirty minutes.

Decisions for this phase:

- Made: Tauri shell with React (spec 8.1).
- Open: renderer for the office scene. Candidates are PixiJS and Phaser. Decide before step 05 with an ADR.
- Open: the pixel design system (palette, tile size, font). Decide before step 01 with an ADR.
- Open: whether the scene shows cost visually (spec section 12, question 3). Decide before step 05.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | Pixel component library | `@farik/ui`: buttons, panels, lists, dialogs in the design system |
| 02 | Desktop shell | `apps/desktop`: Tauri app subscribing to the event stream from a local runtime |
| 03 | Board | Kanban of the lifecycle, task detail with contract, events, diff, cost |
| 04 | Channel and one-on-one | Team chat view and per-agent direct conversation |
| 05 | Office scene | Desks, meeting table, whiteboard, door; agent movement by state; click to open an agent |
| 06 | First-run flow | Project pick, project scan read-back, team builder, permission and budget confirmation |

## Phase 5: Ecosystem and launch

Ends with: the public open-source release.

Decisions for this phase:

- Open: desktop notification mechanism per platform. Decide before step 04.
- Open: launch recording script and repository (per `docs/PRODUCT_ANALYSIS.md`, go to market). Decide before step 05.

Steps:

| Step | Name | Delivers |
|---|---|---|
| 01 | MCP per agent | Server configuration (stdio and remote), tool listing, tier tagging, keychain credentials |
| 02 | Skills per agent | Skill folders at agent, role, and team level; loading into sessions |
| 03 | Audit viewer | Event log view with filters, JSON Lines export, cost reports |
| 04 | Notifications | Escalation and sprint boundary notifications, quiet hours |
| 05 | Launch | README, recording, changelog, release tag |

## Phase 6: Premium

Not yet planned. Its steps are written after the Phase 5 retrospective, because what the open-source launch teaches decides what hosted execution must do first. The spec's section 9 lists the candidate features in priority order.

## Changing this plan

Edits go through a pull request that says which decision changed and why. Reordering steps within a phase is a normal edit. Reordering phases, or adding a dependency from an earlier phase on a later one, requires an ADR because it means the no-forward-dependencies rule was about to be broken.
