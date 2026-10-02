# 0036. A kit's connector is written whole in the team file and trusted only when it is the kit's

Date: 2026-10-02
Status: accepted (phase 7 step 05, readiness-reviewed 2026-10-02)

Builds on ADR 0030 (a connector's entry is pinned by `spec_sha256` and confirmed on this computer) and ADR 0034 (a kit's skills load through the team level's checks). The kit format and its design are in `docs/design/role-kits.md`.

## Context

A role's kit names the services an agent may be connected to, with every tool already tagged, so the user connects by name and labels nothing. Steps 01 to 03b built every path a connector takes (session setup, the launcher and headers helper, refresh, `team.get`, disconnect, retiring, templates) around an `mcp_servers` entry read through `custom_server`. The tags in a kit are Farik's judgment about another company's tools, so a team file that claims a kit's authority must not be able to widen them: the team file travels with the repository and a clone or a pull can change it.

## Decision

**A kit's connector is written whole.** The entry is `{ name, source: kit, transport, ... }` with every field from the kit and the kit's tags as its `tools`. `custom_server` answers `Some` for `kit` as for `custom`, and `CustomServer` gains `kit: bool`, so every existing path reads it unchanged. Rejected: `{ name, source: kit }` resolved from the binary at each use, which would change every one of those paths. `validate_team` holds a kit entry to every rule of a custom one and does not know kits (`farik-core` cannot depend on `farik-roles`).

**The hash tells a kit entry from a custom one.** `spec_sha256` adds `"source": "kit"` for a kit entry only, so every hash kept today stands, and a commit that relabels `custom` as `kit`, or the reverse, changes the hash: the server needs "Connect again".

**The entry must be the kit's.** `kit_entry` builds the entry from the agent's role's kit; `matches_kit` is true when the kit has a `Server` connector of the entry's name whose `custom_server` equals the entry. `connector_connect` refuses a `source: kit` entry for which it is false (`connector_not_in_kit`), which also refuses a kit's connector for an agent of another role.

**Kit trust holds on every path.** `connector_connect` checks `matches_kit`. Session setup leaves out a kit entry `matches_kit` refuses, so a release that tags a tool `denied` takes effect at the next session; `team.get` says `connect_again` for it, or `not_in_kit` when the role's kit has no connector of that name ("Farik no longer offers this service", "Remove" only). The launch route and the hook take only what the session's registration holds, built at setup, and re-check the kept hash, so the launch route needs no kit check of its own: the kit changes only with the binary, which ends every session. A team file or a clone that forges a `source: kit` entry with wider tags hashes differently from anything kept and fails `matches_kit`. Rejected: running a stale entry until the user reconnects. The cost: after such a release the user connects that service again.

**Connecting lists, but writes the kit's tags.** Listing proves the key or the sign-in works; a tool the service lists and the kit does not tag is not offered (`tool_not_tagged`), and a pinned tool the service dropped costs nothing. Rejected: refusing to connect on a difference, which would lock every user out of a service that added a tool until a release. The pin test is where a difference is caught.

**What a kit file may not ship.** A `stdio` connector that runs a package by name (`npx`, `uvx`, `pipx`, `bunx`) names it at an exact version (`package_not_pinned`), so the code Farik runs changes only with a Farik release and its pin review. A `container` connector is only the UI/UX Designer's built-in browser, its image pinned by digest. A kit's copy never says "MCP", "OAuth" or "token"; the founder allowed one exception on 2026-10-02: a service's own label may be quoted in `setup` ("copy the value labelled ‘Bot User OAuth Token’") so the user finds it on the service's page, in `‘…’`, `“…”` or `"…"`, on one line, at most 60 characters, and nowhere else.

**Kits are injected.** The runtime takes its kits through `ToolDeps.kits` (a `KitSource`), `Arc::new(farik_roles::load_kit)` outside tests; only tests swap it, with a fixture kit whose server is the test's own. Rejected: a cargo feature, a global static (parallel tests would share it) and a fake connector in a shipped kit.

## Consequences

A kit's tags are enforced at connect, at each session's setup and at each call, with no new path to audit. A release that changes a kit's tags makes the user connect that service again, once. Step 05b's allowances are read from the kit and are not in the entry or its hash.
