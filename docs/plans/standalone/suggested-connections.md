# Standalone: suggested connections on Today

Status: ready
Branch: `feat/suggested-connectors` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 6.7 (role kits; "Suggested connections on Today", added in 0.88), F9
Depends on: `origin/main` at `15d67ec2` (#35 merged; phase 7 in #22, the change-project plan in #32)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-11 (one round, READY, against `docs/standards/workflow.md` stage 2; its fourteen findings are folded into this revision)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

`docs/design/suggested-connections.md` (2026-10-11) is the source of truth for behavior and this plan does not restate its copy. Done means: Today has a section "Suggested connections", under "Waiting on you", with one row per active agent and kit service the agent lacks (no `mcp_servers` entry of that name), showing the service's logo and title, the agent's name and the kit's `about`, the kit's `why` behind the info button, **Connect** (a link to `/team/<agent>?connect=<service>`, where the agent page opens the service's connect dialog on arrival) and **Not now** (`connector_suggestion.hide`, remembered in `.catervas/local/connector-suggestions.json` and answered by `team.get` as `hidden_suggestions`); at most four rows show before "Show all N"; a connected, `at_launch` or hidden service, and a paused or retired agent's services, are not suggested; with nothing to suggest there is no section. Out of scope (design): ranking by the project's content, a row for a stale connection, an undo for Not now, suggestions for a user's own servers.

## Decisions

- The list is worked out in the browser from `team.get`'s `team` (each agent's `status` and `mcp_servers`) and `kits` (each role's services, `at_launch` marked): chose this over a new `suggestions.list` query because every input is already answered to Today, which already calls `team.get`, and a second query would be a second copy of the same rule.
- Only active agents are suggested for, in the team file's order of agents, each agent's services in the kit's order: chose this over including paused agents (design: a paused agent is not working) and over any ranking by the project's content (design: out of scope).
- A service with any `mcp_servers` entry of its name is not suggested, whatever its `connectors` state (`connect_again`, `sign_in_again`, `not_in_kit` included) and whatever its `source`: the agent page handles those (design). This is wider than the agent page's `heldFromKit` (`source === "kit"`, `AgentEdit.tsx:272-274`), on purpose: a name already in use by the user's own server is not suggested. `at_launch` services are not suggested (design: Connect would lead nowhere).
- Not now is kept on this computer in `.catervas/local/connector-suggestions.json`, `{ "hidden": [ { "agent_id": "<agent>", "server": "<service>" } ] }`, shaped like `keys-copied.json`'s `keys` (ADR 0053), written with `crate::write_private` as that file is: chose a local file over the browser's storage (the founder uses a laptop and a phone; a hidden row should stay hidden on both) and over the team file (it travels with the repository, and a preference of this computer's user is not the team). The reader and writer live in `catervas_runtime::connectors` beside `keys_copied`. The daemon resolves the file under `deps.files.root()`, as `team.validate` resolves `SETUP_PENDING` (`team.rs:110`), in both the command and `team.get`; not under `WebState.project_root`, which the test harness behind `keeping_a_kit` never sets.
- `connector_suggestion.hide { agent, server }` answers `{}` and records no event: local UI state, as `keys_copied.dismiss` is. Hiding a pair already hidden writes it once. It refuses, `REFUSED` (-32005) with the message `suggestion_not_offered: <agent> is not offered <server>`, when no agent of that id is in the team or when the agent's role's kit has no `KitConnector::Server` named `server` (through `deps.kits`, as `kits_of` loads a kit); then nothing is written. Chose one refusal for both cases over two codes: the page never sends either, and the check only keeps the file to pairs the team offers.
- `team.get` answers `hidden_suggestions: [{ agent, server }]`, required on the wire and read from the file each time (`hidden_suggestions` of an unreadable or absent file is empty); TypeScript reads it optional (`hiddenSuggestions?`), as `ServeStatus.keysCopied` is, so existing fixtures stay valid. Keys on the wire are `agent` and `server`, as `connectors` rows name them; the file keeps `agent_id` as `keys-copied.json` does.
- Connect is a link to `/team/<agent>?connect=<service>`; `AgentEdit` reads `connect` as it reads `allowances` (`useSearchParams`, `AgentEdit.tsx:338-341`) and, once `team.get` has answered, sets `adding` to `{ kit: service }` when the role's kit offers a service of that name, the agent does not hold it (`heldFromKit` has no entry of that name) and the service is not `atLaunch` (as `KitRow` offers Connect, `AgentEdit.tsx:966`); otherwise the parameter is ignored. The dialog opens once per arrival (the parameter is read into state once, as `allowances` is). Chose this over a dialog on Today itself: `KitConnect` needs the agent page's state (`held`, the sign-in's route), and one connect flow is the design.
- The section is a component of its own, `apps/web/src/pages/SuggestedConnections.tsx`, exporting the pure rule `suggestedConnections` and the section `SuggestedConnections`; chose a new file over growing `Today.tsx` (924 lines). It reuses `Today.module.css`'s `section`, `rows`, `row`, `rowText`, `action` and `muted`, `ServiceLogo` and `InfoTip` (no avatar: the design names the service's logo only), and gets no new CSS file; **Not now** and the `Show all` toggle are `Button`s of kind `quiet` (`packages/ui/src/Button.tsx`).
- Not now sends the named method through the connection, `useConnection().client.call("connector_suggestion.hide", { agent, server })`, as Today's request box calls `request.file` and the shell calls `keys_copied.dismiss`; a refusal or failure is shown with `daemonSaid(error, "refuseCommand")` in a `role="alert"` paragraph of class `muted` under the list, as the request box shows one (`Today.tsx:444`); since `daemonSaid` knows no `suggestion_not_offered` prefix, the words shown are `en.refuseCommand`, never the daemon's. Not now is `disabled={!client}`, as the request box's button is. Chose this over `useCommand` (`dialogs/StartSprint.tsx`), which sends a `command` body, not a named method.
- The row's visible text: `<strong id="suggestion-<agent>-<service>">{title}</strong>` then `{agent name} · {about}` then `<InfoTip id="suggestion-<agent>-<service>-info">{why}</InfoTip>`, mirroring `KitRow` (`AgentEdit.tsx:886`). Strings: `suggestionsTitle` "Suggested connections" (the heading and the list's `aria-label`), `suggestionsConnect` "Connect", `suggestionsNotNow` "Not now", `suggestionsShowAll` "Show all {count}", `suggestionsShowFewer` "Show fewer", `suggestionsLine` "{agent} · {about}", in `apps/web/src/strings/en.ts`; none exists today. The section sits between the Waiting section and `<SellerMailSections>` (`Today.tsx:323-325`). The Connect link's accessible name is `suggestionsConnect` with `aria-describedby` on the row's title, as `WaitingRow` does.
- Four rows show at first (`SUGGESTIONS_SHOWN = 4`); the toggle is a `Button` with `expanded` (its `aria-expanded`) and `controls` naming the list. Chose four over all: the kits hold up to seven services per role, and Today is quiet below the band (`docs/design/web-ui.md`).
- Not now removes the row at once (the page's own state) and sends `connector_suggestion.hide`; a refusal or failure puts the row back and shows the message under the list in `muted`. No optimistic hide survives a failed call.
- No ADR: no spec decision is reversed, and the new file follows ADR 0053's precedent. `docs/SPEC.md` 6.7 gains a paragraph and F9 a sentence, revision 0.88 (0.84 to 0.87 are claimed by open branches: `fix/settings-fewer-words` 0.86, `feat/setup-friction` and `feat/predefined-teams` 0.87; whoever merges second renumbers).

## File map

```
crates/runtime/src/connectors.rs                     modifies: CONNECTOR_SUGGESTIONS, hide_suggestion, hidden_suggestions, mod tests (Task 1)
docs/schemas/rpc.schema.json                         modifies: connectorSuggestionHideRequest, rpcRequest.oneOf (Task 2); teamGetResult.hidden_suggestions (Task 3)
crates/protocol/src/rpc.rs                           tests:    the new frames (Task 2)
crates/runtime/src/daemon/team.rs                    modifies: METHODS, connector_suggestion.hide, team.get's hidden_suggestions, mod tests (Task 3)
packages/protocol-client/src/client.ts               modifies: MethodName gains "connector_suggestion.hide" (Task 4)
apps/web/src/pages/SuggestedConnections.tsx          creates:  suggestedConnections, SuggestedConnections (Task 4)
apps/web/src/pages/Today.tsx                         modifies: its team.get type gains hiddenSuggestions, renders the section (Task 4)
apps/web/src/pages/Today.test.tsx                    tests:    the section (Task 4)
apps/web/src/test/kits.ts                            creates:  KIT_NOTION, KIT_LINEAR, KIT_ADS, moved from connectors.test.tsx (Task 4)
apps/web/src/pages/connectors.test.tsx               modifies: imports the moved fixtures (Task 4); tests ?connect= (Task 5)
apps/web/src/strings/en.ts                           modifies: the seven keys (Task 4)
apps/web/src/pages/AgentEdit.tsx                     modifies: ?connect= opens KitConnect (Task 5)
docs/SPEC.md, docs/design/suggested-connections.md, this plan   (Task 6)
```

## Interfaces

Consumes (all on `main`): `crate::write_private`, `KEYS_COPIED`, `keys_copied` (`catervas-runtime` connectors, the pattern); `ToolDeps.kits`, `KitConnector::Server`, `Role::from(agent.role)`, `kits_of`, `METHODS`, `REFUSED`, `Failure::new`, `internal`, `deps.files.root()` (`catervas-runtime` daemon); `Team`, `AgentStatus` (`catervas-core`); `Team` and `Agent` (`pages/setup/TeamSetup.tsx`, `status` and `mcpServers?`); `KitService`, `RoleKit` (`pages/Team.tsx`); `refusedBy` (`test/schema.ts`); test fixtures `keeping_a_kit`, `fixture_kit`, `team_file`, `an_agent_wire`, `query`, `call`, `rpc` (`daemon::team::tests`, `daemon::gates::tests`); `rpc_request_from_value` and `request` (`catervas-protocol` tests); `useQuery`, `useConnection` (`client: DaemonClient | null`, `client.call(method, params)`), `daemonSaid`, `Button`, `ServiceLogo`, `InfoTip`, `renderApp`, `answerStatus`, `answerQuery`, `expectNoAxeViolations`, `teamGot`, `kitTeam`, `KIT_NOTION`, `KIT_LINEAR`, `KIT_ADS`, `openedWithKit`.

Produces:

```rust
// catervas-runtime::connectors
pub const CONNECTOR_SUGGESTIONS: &str = ".catervas/local/connector-suggestions.json";
/// Adds the pair to the file, once; makes the folder; writes it private.
pub fn hide_suggestion(root: &Path, agent_id: &str, server: &str) -> std::io::Result<()>;
/// Every hidden (agent_id, server), in the file's order; empty when absent or unreadable.
#[must_use] pub fn hidden_suggestions(root: &Path) -> Vec<(String, String)>;
```

```ts
// apps/web/src/pages/SuggestedConnections.tsx
export type Suggestion = { agent: Team["agents"][number]; service: KitService };
export const SUGGESTIONS_SHOWN = 4;
export function suggestedConnections(
	team: Team, kits: RoleKit[], hidden: { agent: string; server: string }[],
): Suggestion[];
export function SuggestedConnections(props: {
	team: Team; kits: RoleKit[]; hidden: { agent: string; server: string }[];
}): JSX.Element | null;   // null when nothing is suggested
```

Wire (`rpc.schema.json`): `connectorSuggestionHideRequest` (`connector_suggestion.hide`, params `{ agent: string, server: string }`, both required, `additionalProperties: false`), answering `emptyResult`, listed in `rpcRequest.oneOf` after `keysCopiedDismissRequest`; `teamGetResult.hidden_suggestions`, required, an array of `{ agent: string, server: string }` (`additionalProperties: false`, both required). TypeScript: `MethodName` gains `"connector_suggestion.hide"`; Today's inline `team.get` type (`Today.tsx:158`) gains `hiddenSuggestions?: { agent: string; server: string }[]`; the agent page reads nothing new.

## Tasks

### Task 1: the hidden pairs on this computer

Files: modified `crates/runtime/src/connectors.rs` (the constant, two functions, `mod tests`, in a temporary folder, no git). Produces `CONNECTOR_SUGGESTIONS`, `hide_suggestion`, `hidden_suggestions`.

- `hide_suggestion_writes_the_pair_once` — after `hide_suggestion(root, "dev-a", "fixture")` twice, the file at `root/.catervas/local/connector-suggestions.json` parses to `{ "hidden": [ { "agent_id": "dev-a", "server": "fixture" } ] }`, exactly one entry, and `hidden_suggestions(root)` is `[("dev-a", "fixture")]`.
- `hide_suggestion_keeps_earlier_pairs_in_order` — hiding `("dev-a", "fixture")` then `("kai", "buffer")` answers both, in that order.
- `hidden_suggestions_is_empty_without_a_readable_file` — with no file, and with the file holding `not json`, the answer is empty; `hide_suggestion` after the unreadable file replaces it with the one pair.

- [x] `feat(runtime): remember the connections the user said Not now to`

### Task 2: the wire

Files: modified `docs/schemas/rpc.schema.json` (the request and its `oneOf` entry only; `teamGetResult` changes in Task 3, so that no commit answers `team.get` short of a required field), `crates/protocol/src/rpc.rs` (`mod tests`). Produces `connectorSuggestionHideRequest`.

- `reads_the_suggestion_hide_request` — `request(1, "connector_suggestion.hide", &json!({ "agent": "dev-a", "server": "fixture" }))` is read by `rpc_request_from_value`.
- `refuses_bad_suggestion_hide_requests` — `{ "agent": "dev-a" }` (no server), `{ "agent": "dev-a", "server": "fixture", "x": 1 }` (an extra field) and `{ "agent": 1, "server": "fixture" }` are each refused.

- [x] `feat(protocol): a request that hides a suggested connection`

### Task 3: the daemon

Files: modified `docs/schemas/rpc.schema.json` (`teamGetResult.hidden_suggestions`, required), `crates/runtime/src/daemon/team.rs` (`METHODS` to 19, the command's arm beside `keys_copied.dismiss` reading `deps.files.root()`, `team.get`'s `answer["hidden_suggestions"]`, `mod tests`). Tests `#[ignore = "needs the git program: cargo xtask check --integration"]` as the other `team.get` tests are. Consumes Task 1 and Task 2.

- `team_get_lists_hidden_suggestions` — on `keeping_a_kit("hidden-suggestions")`: `team.get` answers `hidden_suggestions: []`; `connector_suggestion.hide { agent: "dev-a", server: "fixture" }` answers `{}`; `team.get` then answers `[{ "agent": "dev-a", "server": "fixture" }]` and the file exists at `CONNECTOR_SUGGESTIONS` under `deps.files.root()`; no event is appended (the log's length is unchanged).
- `suggestion_hide_refuses_what_the_team_does_not_offer` — `{ agent: "ghost", server: "fixture" }` and `{ agent: "dev-a", server: "nothing" }` each answer `error.code` -32005 with `error.message` starting `suggestion_not_offered: `, naming the agent and the server; the file does not exist after; `team.get` still answers `[]`.

- [x] `feat(runtime): hide a suggested connection and answer the hidden ones`

### Task 4: the section on Today

Files: modified `packages/protocol-client/src/client.ts`, `apps/web/src/pages/Team.tsx` (the result type), `apps/web/src/pages/Today.tsx`, `apps/web/src/strings/en.ts`; created `apps/web/src/pages/SuggestedConnections.tsx`; tested by `apps/web/src/pages/Today.test.tsx` (its `today()` helper gains `kits?` and `hiddenSuggestions?` fields answered inside `team.get`). Fixtures: `KIT_NOTION`, `KIT_LINEAR` and `KIT_ADS` move from `connectors.test.tsx` to a new `apps/web/src/test/kits.ts` (exported as they are; `connectors.test.tsx` imports them), and `Today.test.tsx` gains an agent fixture with `mcp_servers: [{ name: "linear", source: "kit" }]`. Consumes Task 2's wire.

- `suggests_a_kit_service_an_active_agent_lacks` — Mira (Product Manager, no `mcp_servers`) with kits `[{ role: product_manager, connectors: [KIT_NOTION] }]`: a list named "Suggested connections" has one item whose text holds "Notion" and "Mira · Notion is where your team keeps docs, notes and plans.", whose info button reveals KIT_NOTION's `why`, and whose link "Connect" has `href` `/team/mira?connect=notion`; the item has no avatar image.
- `does_not_suggest_what_is_held_at_launch_hidden_or_a_paused_agents` — Mira holds `linear`, kits offer `[KIT_LINEAR, KIT_NOTION, KIT_ADS]`, `hidden_suggestions: [{ agent: "mira", server: "notion" }]`, a paused Marketing Specialist Kai and a retired Finance Specialist Finn, with kits `{ role: "marketing_specialist", connectors: [KIT_NOTION] }` and `{ role: "finance_specialist", connectors: [KIT_NOTION] }`: the section is absent (no heading "Suggested connections").
- `not_now_hides_the_row_and_sends_the_command` — two suggestions; clicking the first row's "Not now" removes that row at once and the socket receives `connector_suggestion.hide` with params `{ agent: "mira", server: "notion" }`, for which `refusedBy("connectorSuggestionHideRequest", params)` is `[]`; answered `{}`, the row stays gone; the other remains.
- `not_now_puts_the_row_back_when_refused` — the command answered with error -32005 `suggestion_not_offered: mira is not offered notion`: the row is back, an alert shows `en.refuseCommand`, and the daemon's words are not on the page.
- `shows_four_then_all` — six suggestions (TEAM's Mira, Ada and Theo with kits for `product_manager`, `architect` and `software_developer`, each `[KIT_NOTION, KIT_LINEAR]`): four items, a button "Show all 6" with `aria-expanded="false"` and `aria-controls` the list's id; clicked, six items and "Show fewer" with `aria-expanded="true"`.
- `orders_by_team_then_kit` — two agents, each with two services, team order Mira then Ada and kit order as given: the four rows read Mira's two in kit order, then Ada's two.
- `suggestions_have_no_accessibility_violations` — `expectNoAxeViolations(container)` with the section shown.

- [x] `feat(web): suggest the kit services the team's agents lack on Today`

### Task 5: Connect lands on the dialog

Files: modified `apps/web/src/pages/AgentEdit.tsx`; tested by `apps/web/src/pages/connectors.test.tsx`. `openedWithKit` gains a fourth argument, `path = "/team/theo"`, so the two tests open `/team/theo?connect=<name>` through it.

- `agent_page_opens_the_connect_dialog_from_the_suggestion_link` — at `/team/theo?connect=notion`, kit `[KIT_NOTION]`, nothing held: a dialog named "Connect Notion to Theo" is shown without a click.
- `agent_page_ignores_connect_for_a_held_unknown_or_at_launch_service` — at `/team/theo?connect=notion` with `notion` held, at `/team/theo?connect=nothing`, and at `/team/theo?connect=google-ads` with kit `[KIT_ADS]` (`at_launch`): no dialog; the page's heading "Theo, your Developer" shows and the kit section lists as usual.

- [ ] `feat(web): open a kit service's connect dialog from its link`

### Task 6: the record

Files: modified `docs/SPEC.md` (the header's revision 0.88 note; 6.7 a paragraph "Suggested connections on Today (added in 0.88; standalone plan `suggested-connections`)" after "Service logos and info buttons", saying what is suggested and left out, the file, the command and its refusal, `hidden_suggestions`, `?connect=`; F9 one sentence naming the command and the field), `docs/design/suggested-connections.md` (status: built), this plan (status and ticks).

- [ ] `docs(spec): suggested connections on Today, revision 0.88`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (the daemon's ignored tests, Task 3's among them)
```
