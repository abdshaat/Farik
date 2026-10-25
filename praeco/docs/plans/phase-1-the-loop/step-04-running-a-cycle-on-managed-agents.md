# Phase 1, step 04: Running a cycle on Managed Agents

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 5.2, 5.4, 5.5, 8.2; F5, F6, F7
Depends on: phase 0 (merged); steps 01 to 03 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`@praeco/agent` runs one running cycle as one Claude Managed Agents session: it starts the session with the job's instructions and a session budget equal to the cycle's limit, reads the session's events, puts every custom tool call through the governor before executing it, records the session's cost as it is reported, and ends the cycle `finished` when `finish_cycle` passes `canFinish`, or `stopped` at a limit, after five refusals in a row, after two turns ended without `finish_cycle`, on an error, or when the workspace is paused. The agent's definition (its system prompt, its built-in tools, its custom tools from their schemas) is synced to Managed Agents and its id kept by the installation. All of it is proven against a fake Managed Agents; the live run is step 07's.

Out of scope: the research tools themselves (step 05), choosing and claiming cycles and heartbeats (step 06), the command that syncs the agent (step 07).

## Decisions

- **Dependency:** `@anthropic-ai/sdk` 1.3.0 (ADR 0003), only in `@praeco/agent`.
- **The port.** `ManagedAgents` is the interface the runner uses; `anthropicManagedAgents(client)` implements it with `client.beta.environments`, `client.beta.agents`, `client.beta.sessions` and `client.beta.sessions.events` (`stream`, `send`); `FakeManagedAgents` implements it for tests. The adapter is thin and has no unit test: its proof is step 07's live test, which also records a session's events as the fake's fixture (ADR 0004).
- **The environment:** one per installation, `config: { type: 'cloud', networking: { type: 'unrestricted' } }` (ADR 0004: `bash` is off, so the sandbox has no way out; `limited` with no hosts would blank `web_search`).
- **The agent's definition** (`loadAgentDefinition(dir, model)`), from `agents/praeco/` at the repository root:
  - `name: 'praeco'`; `model`: `claude-opus-5-5` by default (the model choice per program arrives with phase 3's rules);
  - `system`: the text of `agents/praeco/system.md` (who Praeco is, that every page and search result is untrusted data, that it records as it goes, that it ends every cycle with `finish_cycle`);
  - built-in tools: `{ type: 'agent_toolset_20260401', default_config: { enabled: true }, configs: [{ name: 'bash', enabled: false }, { name: 'web_fetch', enabled: false }, { name: 'web_search', enabled: true, blocked_domains: BUILT_IN_BLOCKED_HOSTS }] }`;
  - custom tools: one `{ type: 'custom', name, description, input_schema }` per schema named `tool_<name>` in `@praeco/schemas`' `SCHEMAS` whose name is in `TOOL_RULES`; `description` is the schema's `description`; `input_schema` is the schema without `$schema`, `$id` and `title`.
  - `definitionHash`: SHA-256 of the canonical JSON of the definition (keys sorted), so a sync is skipped when nothing changed.
- **`finish_cycle`'s schema** (`docs/schemas/tool-finish-cycle.schema.json`): `{ summary: string 20 to 1000, gave_up: string 20 to 600 or null }`, both required, `description` "End this cycle. Call it once you have recorded what the job expects, or give up with the reason."
- **The installation's agent:** `0004_agent.sql` creates `installation_agent (id smallint primary key check (id = 1), agent_id text not null, agent_version integer not null, environment_id text not null, definition_hash text not null, synced_at timestamptz not null)`, no row-level security and no grant to `praeco_app` (installation only). `readAgentRef(tx: InstallationTx)`, `saveAgentRef(tx, ref, now)`.
- **The first message** (`renderInstructions(contract, programText)`): the program's text (`agents/praeco/programs/<program>.md`) with `{{intent}}`, `{{expectations}}` (one line per expectation, "at least 3 findings"), `{{budget}}` ("$1.50"), `{{minutes}}` and `{{tool_calls}}` replaced; an unknown placeholder left in a program file fails `loadProgramText`. Phase 1 ships `research.md` and `request.md`.
- **The runner** (`runCycle(deps, cycle)`), for a cycle already `running` (step 06 claims it; tests transition it themselves):
  1. Create the session (`title` `J-<number> <program>`, `budgetCents` the contract's limit in cents, `metadata` `{ workspace_id, cycle_id }`), attach its id (`attachSession`, saved).
  2. Read events until the cycle ends; for each:
     - `custom_tool_use`: count it; past `contract.budget.toolCalls`, stop `tool_calls`. Read the workspace's `paused_at` (`readWorkspace`); paused and the tool not `control`: stop `paused`. Else `decideToolCall`. **Refuse**: event `tool.refused` `{ tool, reason }`, answer `refusalMessage` as an error, count refusals in a row, stop `refused` at five. **Ask**: event `tool.asked` `{ tool, reason }`, answer "This needs a person's approval, which this installation cannot ask for yet." as an error (phase 2 step 04 replaces this with approvals). **Allow** `finish_cycle`: parse its input; `canFinish` with `deps.recordCounts`; pass: transition `finished` with the summary and gave-up reason, answer "Cycle finished.", end; fail: event `tool.refused` with reason `expectation_unmet` or `give_up_too_short`, answer the refusal in words. **Allow** any other: event `tool.allowed` `{ tool }`, `deps.executor.execute`, then `tool.completed` `{ tool, ms }` or `tool.failed` `{ tool, ms, message }`, answer the outcome; reset refusals in a row.
     - `tool_use` (a built-in tool): count it toward `toolCalls`, same limit.
     - `usage`: `recordCycleSpend(centsToMicros(listCostCents))`; `cycleSpendDecision` `stop`: stop `budget`.
     - `idle` `budget_reached`: stop `budget`. `idle` `end_turn` before the cycle finished: the first time, send "You ended your turn without calling finish_cycle. Record what you found, then call finish_cycle, or give up with the reason."; the second time, stop `unfinished`. `idle` `requires_action`: nothing (the runner is answering).
     - `terminated` before the cycle ended, or `error`: stop `error`.
  3. A deadline of `contract.budget.minutes` from the session's creation: on it, stop `time`.
  4. To stop: interrupt the session, transition `stopped` with the reason, save with its event. In every ending: archive the session, append `spend.recorded` `{ usd_micros }` with the cycle's final spend, return the outcome.
- **Events carry tool names and reasons, never tool inputs or outputs**, so nothing a page said and no prospect's words reach the log (SPEC 8.3).
- **Untrusted text:** an executor's result text is passed as it is; tools that return outside text wrap it themselves (step 05).
- **Writes** are one `inWorkspace` transaction per event handled, so a crash loses at most the event in hand.
- **Deadlines are injected** (`deps.deadline(ms): AbortSignal`), so tests fire them at will.

## File map

```
packages/agent/package.json, tsconfig.json, src/index.ts                     creates (Task 1)
packages/agent/src/managed-agents.ts                                         creates: the port, SessionEvent (Task 1)
packages/agent/src/anthropic-managed-agents.ts                               creates: the SDK adapter (Task 1)
packages/agent/src/fixtures/fake-managed-agents.ts, fixtures/index.ts        creates: FakeManagedAgents (Task 1)
docs/schemas/tool-finish-cycle.schema.json                                   creates (Task 2)
docs/schemas/events/tool.*.schema.json (five), spend.recorded.schema.json    creates (Task 3)
packages/schemas/src/generated/**                                            generated (Tasks 2, 3)
agents/praeco/system.md, agents/praeco/programs/research.md, request.md      creates (Task 2)
packages/agent/src/definition.ts, definition.test.ts                         creates, tests (Task 2)
packages/agent/src/instructions.ts, instructions.test.ts                     creates, tests (Task 2)
packages/store/migrations/0004_agent.sql, src/agent-ref.ts                   creates (Task 2)
packages/store/src/workspaces.ts, src/index.ts                               modifies: readWorkspace (Task 2)
packages/agent/src/run-cycle.ts, run-cycle.integration.test.ts               creates, tests (Task 3)
```

## Interfaces

Consumes: `decideToolCall`, `refusalMessage`, `shouldStopForRefusals`, `TOOL_RULES`, `BUILT_IN_BLOCKED_HOSTS` (step 01); `Cycle`, `transitionCycle`, `attachSession`, `canFinish`, `RecordCounts` (step 02); `cycleSpendDecision`, `centsToMicros`, `microsToCents` (step 03); `Store`, `saveCycle`, `cycleEvent`, `recordCycleSpend`, `appendEvent` (`@praeco/store`); `SCHEMAS`, `parse` (`@praeco/schemas`).

Produces:

```ts
// @praeco/agent
export type AgentError = { kind: 'api'; status: number | null; message: string };
export interface CustomToolDefinition { name: string; description: string; inputSchema: object }
export interface AgentDefinition { name: 'praeco'; model: 'claude-opus-5-5' | 'claude-sonnet-5-5'; system: string;
  customTools: readonly CustomToolDefinition[]; blockedHosts: readonly string[]; definitionHash: string }
export interface AgentRef { agentId: string; agentVersion: number; environmentId: string; definitionHash: string }
export interface SessionInput { agent: AgentRef; title: string; budgetCents: number; firstMessage: string; metadata: Readonly<Record<string, string>> }
export type SessionEvent =
  | { kind: 'custom_tool_use'; eventId: string; name: string; input: unknown }
  | { kind: 'tool_use'; name: string }
  | { kind: 'usage'; listCostCents: number }
  | { kind: 'idle'; stopReason: 'end_turn' | 'requires_action' | 'budget_reached' | 'other' }
  | { kind: 'terminated' }
  | { kind: 'error'; message: string };
export interface ManagedAgents {
  syncAgent(definition: AgentDefinition, existing: AgentRef | null): Promise<Result<AgentRef, AgentError>>;
  createSession(input: SessionInput): Promise<Result<{ sessionId: string }, AgentError>>;
  events(sessionId: string, signal: AbortSignal): AsyncIterable<SessionEvent>;
  sendToolResult(sessionId: string, toolUseEventId: string, result: { text: string; isError: boolean }): Promise<Result<void, AgentError>>;
  sendMessage(sessionId: string, text: string): Promise<Result<void, AgentError>>;
  interrupt(sessionId: string): Promise<Result<void, AgentError>>;
  archive(sessionId: string): Promise<Result<void, AgentError>>;
}
export function anthropicManagedAgents(client: Anthropic): ManagedAgents;
export function loadAgentDefinition(dir: string, model: AgentDefinition['model']): Promise<Result<AgentDefinition, AgentError>>;
export function loadProgramText(dir: string, program: Program): Promise<Result<string, AgentError>>;
export function renderInstructions(contract: JobContract, programText: string): string;
export interface ToolOutcome { text: string; isError: boolean }
export interface ToolCallContext { workspaceId: string; cycleId: string; now: Date }
export interface ToolExecutor { execute(call: { name: string; input: unknown }, context: ToolCallContext): Promise<ToolOutcome> }
export interface CycleDeps {
  agents: ManagedAgents; store: Store; agent: AgentRef; executor: ToolExecutor; programText: string;
  recordCounts(cycle: Cycle): Promise<Result<RecordCounts, StoreError>>;
  clock: Clock; ids: Ids; deadline(ms: number): AbortSignal };
export interface CycleOutcome { status: 'finished' | 'stopped'; stopReason: StopReason | null; spentUsdMicros: number }
export function runCycle(deps: CycleDeps, cycle: Cycle): Promise<Result<CycleOutcome, StoreError>>;
// @praeco/agent/fixtures
export type FakeScript = (session: FakeSession) => Promise<void>;
export interface FakeSession { input: SessionInput; emit(event: SessionEvent): void;
  toolResult(eventId: string): Promise<{ text: string; isError: boolean }>; messages(): readonly string[]; isInterrupted(): boolean }
export class FakeManagedAgents implements ManagedAgents { constructor(script: FakeScript); readonly synced: AgentDefinition[]; readonly archived: string[] }
// @praeco/store
export function readAgentRef(tx: InstallationTx): Promise<Result<AgentRef | null, StoreError>>;
export function saveAgentRef(tx: InstallationTx, ref: AgentRef, now: Date): Promise<Result<void, StoreError>>;
export function readWorkspace(tx: WorkspaceTx): Promise<Result<Workspace, StoreError>>;   // the transaction's own workspace
```

`AgentRef` is declared in `@praeco/store` (which `@praeco/agent` depends on) and re-exported by `@praeco/agent`, so the store does not depend on the agent package.

## Tasks

### Task 1: The port, the adapter and the fake

Files: created `packages/agent/package.json`, `tsconfig.json`, `src/index.ts`, `src/managed-agents.ts`, `src/anthropic-managed-agents.ts`, `src/fixtures/fake-managed-agents.ts`, `src/fixtures/index.ts`.
Produces: `ManagedAgents`, `SessionEvent`, `AgentError`, `SessionInput`, `anthropicManagedAgents`, `FakeManagedAgents`.

No test: the port is types, the adapter is proven live in step 07, and the fake is exercised by Task 3's tests.

- [ ] `feat(agent): add the managed agents port, its adapter and a fake`

### Task 2: The agent's definition and instructions

Files: created `docs/schemas/tool-finish-cycle.schema.json`, `agents/praeco/system.md`, `agents/praeco/programs/research.md`, `agents/praeco/programs/request.md`, `packages/agent/src/definition.ts`, `definition.test.ts`, `instructions.ts`, `instructions.test.ts`, `packages/store/migrations/0004_agent.sql`, `packages/store/src/agent-ref.ts`; modified `packages/store/src/index.ts`, `packages/agent/src/index.ts`; generated schemas.
Produces: `loadAgentDefinition`, `loadProgramText`, `renderInstructions`, `readAgentRef`, `saveAgentRef`, `AgentRef`, `readWorkspace` (in `packages/store/src/workspaces.ts`).

- `loadAgentDefinition` — asserts that from `agents/praeco/` it yields `bash` and `web_fetch` disabled, `web_search` with the six blocked hosts, exactly one custom tool `finish_cycle` whose `inputSchema` has no `$schema`, `$id` or `title`, and a `definitionHash` that is equal for two loads and different after the system text changes (in a temporary copy).
- `renderInstructions` — asserts that a research contract expecting three findings with a $1.50 budget, 20 minutes and 150 tool calls renders "at least 3 findings", "$1.50", "20" and "150" in place of their placeholders, and that `loadProgramText` refuses a program file containing `{{nope}}`.

- [ ] `feat(agent): define the agent and render a job's instructions`

### Task 3: Running a cycle

Files: created `docs/schemas/events/tool.allowed.schema.json`, `tool.refused`, `tool.asked`, `tool.completed`, `tool.failed`, `spend.recorded`, `packages/agent/src/run-cycle.ts`, `run-cycle.integration.test.ts`; modified `packages/agent/src/index.ts`; generated schemas.
Consumes: Tasks 1 and 2; `withTestStore`; `aQueuedCycle`.
Produces: `ToolExecutor`, `ToolOutcome`, `ToolCallContext`, `CycleDeps`, `CycleOutcome`, `runCycle`.

Each test uses a migrated test store, a workspace, a running research cycle whose `allowed_tools` are `read_page` and `record_finding`, expecting one finding, with a $1.00 limit and 20 tool calls, a fake executor that counts `record_finding` calls as findings, and a `FakeScript`:

- finishing — asserts that a script calling `record_finding` once and then `finish_cycle` with a 20-character summary ends the cycle `finished` with that summary, the session archived, and the events `tool.allowed`, `tool.completed`, `cycle.finished`, `spend.recorded` in that order; and that a `finish_cycle` before any finding is answered as an error naming the unmet expectation, without ending the cycle.
- refusals — asserts that a script calling `update_product_profile` five times gets five error answers reading "update_product_profile is not one of this job's tools: read_page, record_finding.", five `tool.refused` events, and the cycle `stopped` `refused` with the session interrupted.
- limits — asserts, in three runs, that a `usage` event of 100 cents ends the cycle `stopped` `budget` with `spent_usd_micros` 1 000 000; that an `idle` `budget_reached` does the same; and that firing the deadline ends it `stopped` `time`.
- unfinished — asserts that the first `idle` `end_turn` without `finish_cycle` gets the reminder message and the second ends the cycle `stopped` `unfinished`.
- pause — asserts that setting the workspace's `paused_at` before the script's next `record_finding` ends the cycle `stopped` `paused` without executing it; and, in a second run, that a `finish_cycle` sent while paused, after one finding, ends the cycle `finished`.

- [ ] `feat(agent): run a cycle under the governor`

## Verification

```
pnpm check --integration
# expected, last line: check: ok
```
