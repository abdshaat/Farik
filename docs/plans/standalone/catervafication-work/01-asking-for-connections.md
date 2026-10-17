# Standalone catervafication-work, plan 1: an agent asks for a connection

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 6.5, 6.7 ("Suggested connections on Today"), 8.5, F9
Depends on: the README's "Before the first task"
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

An agent that needs a service its kit offers and the agent lacks can say so with
`catervas_request_connection { server, why }`. Catervas records `connection.requested`, and Today's
"Suggested connections" lists that pair first, with "<Agent> asks: <why>" in place of the kit's
description and the same Connect and Not now. The Marketing Specialist uses it for the plan's
channels (plan 3 writes that into its skill), and any agent may use it for its own kit. Out of
scope:
- a separate section or card;
- undoing Not now;
- a request for a service outside the kit.

## Decisions

- One tool, `catervas_request_connection`, tier `Read`, offered to every session given every tool
  (not to chat, explore or ceremony lists). Chosen over `catervas_ask_human`, whose question holds
  the task until answered (ADR 0063).
- Input `{ server: string, why: string }`, `deny_unknown_fields`:
  - `server` is a `KitConnector::Server` name of the agent's role's kit, read through `deps.kits`
    as `connector_suggestion.hide` reads it (`daemon/team.rs`);
  - `why` is 20 to 300 code points once trimmed.
- Refusals, as `ToolError::Refused { reason }` with the code first, checked in this order:
  - `connection_why: why is <n> characters; it must be 20 to 300`;
  - `connection_not_in_kit: <server> is not a service your kit offers`;
  - `connection_held: you already have <server>`, when the calling agent's team entry has any
    `mcp_servers` entry of that name, whatever its source.

  Nothing is recorded on a refusal. A service that is `at_launch` is not refused, since the tool
  cannot see the registered apps; Today already leaves it out.
- Recorded: `connection.requested { server, why }`. The envelope has the agent, the session and
  the task when the session has one, as `call.append` stamps it. The answer is:

  ```json
  { "requested": "<server>",
    "next": "The owner sees this on Today with Connect. Carry on without it; do not wait for it." }
  ```

  Asking again for the same pair records again. The newest `why` wins.
- `team.get` answers `requested_connections`, required: one `{ agent, server, why }` per
  `(agent, server)` pair, with the newest `why`, ordered by each pair's newest event. It is read
  from the log on every call, as `hidden_suggestions` is read from its file. TypeScript reads it
  as optional (`requestedConnections?`), as it does `hiddenSuggestions?`.
- The browser rule `suggestedConnections(team, kits, hidden, requested)`:
  - takes the same candidates as today;
  - puts first those whose pair is in `requested`, in `requested`'s order; the rest keep the
    team's and the kits' order;
  - shows a requested row's line as `suggestionsAsked` "{agent} asks: {why}", in place of
    `suggestionsLine`.

  A hidden pair stays hidden even when requested: Not now is the owner's word. A held or
  `atLaunch` pair is never shown. The four-row cap applies after ordering.
- Event kinds follow the precedent of `catervafication.started` (commit `d283e1e4`): the schema,
  `event.rs`, a fixture in `event/fixtures.rs`, `lib.rs`'s list, and the projections passing over
  it.

## File map

```
docs/schemas/event.schema.json                 modifies: kind connection.requested, connectionRequestedBody (Task 1)
crates/protocol/src/event.rs, event/fixtures.rs, lib.rs   modifies: the kind and its body (Task 1)
crates/store/src/projections.rs                modifies: passes over the kind (Task 1)
crates/runtime/src/tools/connections.rs        creates:  RequestConnectionInput, request_connection, mod tests (Task 2)
crates/runtime/src/tools.rs                    modifies: `mod connections`, TOOLS entry, dispatch arm (Task 2)
crates/runtime/src/connectors.rs               modifies: requested_connections (Task 3)
docs/schemas/rpc.schema.json                   modifies: teamGetResult.requested_connections (Task 3)
crates/runtime/src/daemon/team.rs              modifies: team.get's answer, mod tests (Task 3)
apps/web/src/pages/SuggestedConnections.tsx    modifies: the rule and the line (Task 4)
apps/web/src/pages/Today.tsx                   modifies: passes requestedConnections (Task 4)
apps/web/src/strings/en.ts                     modifies: suggestionsAsked (Task 4)
apps/web/src/pages/Today.test.tsx              tests:    the requested row (Task 4)
```

## Interfaces

Consumes (on the branch): `Call`, `call.append`, `call.team`, `call.agent_id()`, `call.role()`,
`ToolDeps.kits`, `KitConnector::Server`, `TOOLS`, `ToolError::Refused` (`catervas-runtime`
tools); `EventLog`, `EventQuery`, `StoreError` (`catervas-store`); `TestProject`, `set_kit`,
`a_developer_kit`, `a_kit_server`, `a_team_of_three` (`tools::fixtures`); `keeping_a_kit`,
`query` (`daemon::team::tests`); `suggestedConnections`, `SuggestedConnections`, `today()`
(`Today.test.tsx`), `KIT_NOTION`, `KIT_LINEAR`.

Produces:

```rust
// catervas-protocol (generated from the schema, as every body is)
pub struct ConnectionRequestedBody { pub server: String, pub why: String }
// EventKind::ConnectionRequested, wire "connection.requested"

// catervas-runtime::tools::connections
#[derive(Debug, Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]
pub(crate) struct RequestConnectionInput { server: String, why: String }
pub(super) fn request_connection(call: &Call<'_>, input: &RequestConnectionInput) -> Result<Value, ToolError>;

// catervas-runtime::connectors
/// Each (agent_id, server, why), the newest why per pair, ordered by each pair's newest event.
pub fn requested_connections(log: &EventLog) -> Result<Vec<(String, String, String)>, StoreError>;
```

```ts
// apps/web/src/pages/SuggestedConnections.tsx
type Requested = { agent: string; server: string; why: string };
export type Suggestion = { agent: Team["agents"][number]; service: KitService; why?: string };
export function suggestedConnections(team: Team, kits: RoleKit[], hidden: Pair[], requested?: Requested[]): Suggestion[];
export function SuggestedConnections(props: { team: Team; kits: RoleKit[]; hidden: Pair[]; requested?: Requested[] }): JSX.Element | null;
```

The schema body is `connectionRequestedBody`: `{ server: string (1 to 64), why: string (1 to 300) }`,
both required, `additionalProperties: false`. Its description says it is an agent's ask for a
service of its own kit (ADR 0063), shown on Today.

## Tasks

### Task 1: the event

Files: `docs/schemas/event.schema.json`, `crates/protocol/src/event.rs`,
`crates/protocol/src/event/fixtures.rs`, `crates/protocol/src/lib.rs`,
`crates/store/src/projections.rs`. Produces `ConnectionRequestedBody`,
`EventKind::ConnectionRequested`.

- `reads_a_connection_requested_event` (`crates/protocol/src/event.rs` `mod tests`):
  - `event_from_value` reads a fixture of kind `connection.requested` with body
    `{ "server": "buffer", "why": "The plan posts to Instagram every Tuesday." }`;
  - it refuses the same with an extra body field `"x": 1`;
  - it refuses one with no `why`.

- [x] `feat(protocol): record an agent's request for a connection`

### Task 2: the tool

Files: created `crates/runtime/src/tools/connections.rs`; modified `crates/runtime/src/tools.rs`
(`mod connections;`, the `TOOLS` entry after `catervas_ask_human` with the description "Ask the
owner to connect a service your kit offers that your work needs, and why. The owner sees it on
Today; carry on without it.", the dispatch arm). Consumes Task 1. `lists_every_tool_with_its_tier` (`tools.rs`) changes here: it
lists `catervas_request_connection` right after `catervas_ask_human`, and the count of leading
`Read` tools it checks (`tools[..41]` on `main`, fewer once ADR 0061 and 0062 removed theirs)
grows by one.

- `requests_a_connection_the_kit_offers` (`tools/connections.rs` `mod tests`), on
  `TestProject::new` of `a_team_of_three(|_| {})` with `set_kit(a_developer_kit(&[],
  Some("network")))`:
  - `dev-a` calling with `{ server: "github", why: "The plan needs the issues to triage bugs." }`
    answers `requested: "github"`;
  - exactly one `connection.requested` is recorded, its body that server and why, its envelope's
    `agent_id` `dev-a`;
  - `{ server: "nothing", why: <the same> }` is refused with a reason starting
    `connection_not_in_kit: `;
  - `{ server: "github", why: "short" }` is refused with `connection_why: `;
  - with `dev-b`'s team entry given `a_kit_server()` in `mcp_servers`, `dev-b` asking for
    `github` is refused with `connection_held: `;
  - after the three refusals the log still holds exactly one `connection.requested`.

- [x] `feat(runtime): an agent asks the owner to connect a service of its kit`

### Task 3: team.get answers the requests

Files: `crates/runtime/src/connectors.rs` (`requested_connections`), `docs/schemas/rpc.schema.json`
(`teamGetResult.requested_connections`, required, an array of `{ agent, server, why }`, each
string, all required, `additionalProperties: false`), `crates/runtime/src/daemon/team.rs`
(`answer["requested_connections"]` beside `hidden_suggestions`; `mod tests`). The test is
`#[ignore = "needs the git program: cargo xtask check --integration"]`, as the other `team.get`
tests are. Consumes Task 1.

- `team_get_lists_requested_connections`, on `keeping_a_kit("requested-connections")`:
  - `team.get` answers `requested_connections: []`;
  - after two `connection.requested` by `dev-a` for `fixture`, with whys "first reason, long
    enough" then "second reason, long enough", and one by `dev-a` for `other`, it answers
    `[{ agent: "dev-a", server: "fixture", why: "second reason, long enough" }, { agent: "dev-a",
    server: "other", why: "third reason, long enough" }]`, in that order. The events are
    recorded in that order with `harness.project.record_by(Some("dev-a"), at(), "CTV-1",
    "connection.requested", &body)`, the third with the why "third reason, long enough";
    `requested_connections` reads them whatever their task.

- [ ] `feat(runtime): answer the connections agents asked for`

### Task 4: Today lists a requested pair first

Files: `apps/web/src/pages/SuggestedConnections.tsx`, `apps/web/src/pages/Today.tsx` (its
`team.get` type gains `requestedConnections?: { agent: string; server: string; why: string }[]`,
passed as `requested`), `apps/web/src/strings/en.ts` (`suggestionsAsked: "{agent} asks: {why}"`),
tested by `apps/web/src/pages/Today.test.tsx` (its `today()` helper gains `requestedConnections?`).
Consumes Task 3's wire.

- `lists_an_asked_connection_first_with_its_reason`. Mira (Product Manager) lacks both, kits are
  `[{ role: product_manager, connectors: [KIT_NOTION, KIT_LINEAR] }]`, and
  `requested_connections: [{ agent: "mira", server: "linear", why: "I need it to track the bugs
  the team files." }]`:
  - the first item of "Suggested connections" holds "Linear" and "Mira asks: I need it to track
    the bugs the team files.", and its Connect links to `/team/mira?connect=linear`;
  - the second item holds "Notion" and KIT_NOTION's `about`;
  - with `hidden_suggestions: [{ agent: "mira", server: "linear" }]` added, Linear is not listed.

- [ ] `feat(web): put the connections an agent asked for first on Today`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Task 3's ignored test among them)
```
