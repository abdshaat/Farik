# Phase 1, step 01: The tools and the governor

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 5.1, 5.4, 5.8; F6
Depends on: phase 0 (merged)
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`@praeco/core` knows every Praeco tool that phase 1 builds, the tier of each, and which program may use which, and decides any tool call the agent makes: allow, ask a person, or refuse with a reason the agent can read. It also holds the built-in list of hosts Praeco never reads (SPEC 5.8). Nothing here runs a tool; this is the table and the decision, in code, with no I/O.

Out of scope: running tools (steps 04, 05), the contract that names a cycle's tools (step 02), approvals (phase 2 step 04), allowances for paid tools (phase 9).

## Decisions

- **Phase 1's tools and tiers** (`TOOL_RULES`, in this order): `read_page` `read`; `record_finding` `record`; `update_product_profile` `record`; `finish_cycle` `control`. Each later step that builds a tool adds its row in the same commit as the tool. Rejected: listing every tool SPEC 5.4 names now, which would offer the agent tools that do not exist.
- **Programs** (`PROGRAM_TOOLS`): `research` and `request` may each use all four. Later phases add programs and rows.
- **The decision, in this order** (`decideToolCall`):
  1. a name not in the rules: refuse `unknown_tool`;
  2. a `control` tool: allow, always, even while paused, so a cycle can always end;
  3. the workspace paused: refuse `paused`;
  4. a `denied` tool: refuse `denied`;
  5. a name not in the job's `allowedTools`: refuse `not_in_job`;
  6. an `outward` tool: ask, reason `outward`; a `paid` tool: ask, reason `paid`;
  7. otherwise allow.
- **Stopping on refusals:** `REFUSALS_BEFORE_STOP = 5`; `shouldStopForRefusals(n)` is true at five refusals in a row (SPEC 5.2). The runner counts; an allowed call resets the count.
- **Messages to the agent** (`refusalMessage`), plain words, one sentence each, as the tool's error result:
  - `unknown_tool`: "There is no tool named <name>."
  - `paused`: "The workspace is paused; record nothing more and call finish_cycle."
  - `denied`: "<name> is never allowed."
  - `not_in_job`: "<name> is not one of this job's tools: <comma-separated list>."
- **Blocked hosts** (`BUILT_IN_BLOCKED_HOSTS`, ADR 0009): `linkedin.com`, `facebook.com`, `instagram.com`, `x.com`, `twitter.com`, `tiktok.com`. `isBlockedHost(host, extra)` is true when the host, lower-cased and without a trailing dot, equals an entry of either list or ends with `.` plus one. An entry is a bare host name; `extra` is the workspace's own list (phase 3 lets an owner add to it).
- **Tool names are typed** as the union `ToolName` of the table's names, and `decideToolCall` accepts any string, since the agent's call may name anything.
- **Module layout:** `packages/core/src/tools.ts` (the table, `ToolName`, `Tier`, `Program`, `PROGRAM_TOOLS`), `packages/core/src/decide-tool-call.ts`, `packages/core/src/blocked-hosts.ts`.

## File map

```
packages/core/src/tools.ts                         creates: TOOL_RULES, PROGRAM_TOOLS, types (Task 1)
packages/core/src/decide-tool-call.ts              creates: decideToolCall, refusalMessage, refusals (Task 1)
packages/core/src/decide-tool-call.test.ts         tests (Task 1)
packages/core/src/blocked-hosts.ts, .test.ts       creates, tests (Task 2)
packages/core/src/index.ts                         modifies: exports (Tasks 1, 2)
docs/SPEC.md                                       modifies: 5.4 notes which tools exist after phase 1 (Task 1)
```

## Interfaces

Consumes: nothing beyond phase 0.

Produces:

```ts
// @praeco/core
export type Tier = 'read' | 'record' | 'draft' | 'outward' | 'paid' | 'control' | 'denied';
export type ToolName = 'read_page' | 'record_finding' | 'update_product_profile' | 'finish_cycle';
export interface ToolRule { name: string; tier: Tier }
export const TOOL_RULES: readonly ToolRule[];
export type Program = 'research' | 'request';
export const PROGRAM_TOOLS: Readonly<Record<Program, readonly ToolName[]>>;
export interface ToolContext { allowedTools: readonly string[]; isPaused: boolean }
export type ToolRefusal =
  | { reason: 'unknown_tool'; tool: string }
  | { reason: 'paused'; tool: string }
  | { reason: 'denied'; tool: string }
  | { reason: 'not_in_job'; tool: string; allowedTools: readonly string[] };
export type ToolDecision =
  | { kind: 'allow'; tool: string; tier: Tier }
  | { kind: 'ask'; tool: string; tier: 'outward' | 'paid'; reason: 'outward' | 'paid' }
  | { kind: 'refuse'; refusal: ToolRefusal };
export function decideToolCall(call: { name: string }, context: ToolContext, rules?: readonly ToolRule[]): ToolDecision;
export function refusalMessage(refusal: ToolRefusal): string;
export const REFUSALS_BEFORE_STOP: 5;
export function shouldStopForRefusals(refusalsInARow: number): boolean;
export const BUILT_IN_BLOCKED_HOSTS: readonly string[];
export function isBlockedHost(host: string, extra: readonly string[]): boolean;
```

## Tasks

### Task 1: The table and the decision

Files: created `packages/core/src/tools.ts`, `decide-tool-call.ts`, `decide-tool-call.test.ts`; modified `packages/core/src/index.ts`, `docs/SPEC.md`.
Produces: everything above but the blocked hosts.

Tests:

- `decideToolCall` follows SPEC 5.4 — asserts, with a test rule table holding one tool of each tier (`r` read, `w` record, `d` draft, `o` outward, `p` paid, `c` control, `x` denied) and a job allowing all but `d`: `r` and `w` are allowed; `d` is refused `not_in_job` naming the allowed list; `o` asks with reason `outward`; `p` asks with reason `paid`; `c` is allowed; `x` is refused `denied`; `nope` is refused `unknown_tool`; and with `isPaused: true` every one but `c` and `nope` is refused `paused` (`nope` stays `unknown_tool`).
- `shouldStopForRefusals` and `refusalMessage` — asserts that 4 is false and 5 is true, and that the `not_in_job` message for `d` reads exactly "d is not one of this job's tools: r, w, o, p, c, x."

- [ ] `feat(core): decide every tool call by its tier and the job`

### Task 2: Blocked hosts

Files: created `packages/core/src/blocked-hosts.ts`, `blocked-hosts.test.ts`; modified `packages/core/src/index.ts`.
Produces: `BUILT_IN_BLOCKED_HOSTS`, `isBlockedHost`.

- `isBlockedHost` — asserts that `linkedin.com`, `www.linkedin.com`, `WWW.LinkedIn.com.` and `de.linkedin.com` are blocked, that `notlinkedin.com` and `linkedin.com.example.org` are not, and that `docs.acme.io` is blocked only when `acme.io` is in `extra`.

- [ ] `feat(core): name the hosts praeco never reads`

## Verification

```
pnpm check
# expected, last line: check: ok
```
