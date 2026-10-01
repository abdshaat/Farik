# 0030. Connector credentials and confirmation

Date: 2026-10-01
Status: accepted

## Context

Phase 7 step 01 lets the user give one agent any MCP server: a program started on the host (stdio) or a web address (http). Such a server needs keys of its own, it is defined in `.farik/team.yaml`, and that file is committed (spec 8.4). Four questions bind the steps after this one.

How a key reaches a server. The options were:
- **An `env` or `headers` map in the session's `mcp.json`.** Claude Code supports both. The file is 0600 but stays on disk, and spec 8.6 says a key goes from the keychain to the process environment and is never written to a file.
- **An inherited environment.** The server would also receive the model credential that Claude Code runs with (`child_env`, spec 8.2).
- **A launcher.** `mcp.json` names a Farik command, which asks the daemon for the server's keys and starts the server with only those.

What a team file may run. A clone, a pulled branch, a template or a hand edit can change a connected server's `command` or `url`. Without a check, the next session would run `sh -c "curl … | sh"` on the host, or send the user's key to a new address. Agents cannot write `.farik/` (spec 5.3), so this is a supply-chain threat, which 8.6 expects.

Where the keys are kept. The model credential is in the OS keychain, with a private file where there is none (ADR 0022). The options for connector keys were one key per server shared by every agent, or one per agent.

When connecting is built. The project plan put it in the kit step (now 05). A per-agent key needs a way in sooner.

## Decision

**A launcher carries the keys.** A session's `mcp.json` names a stdio connector as `farik connector run --daemon <daemon.json> --session <id> --server <name>`, and gives an http connector the `headersHelper` `farik connector headers` with the same arguments. Both ask the daemon's authenticated `POST /connector/launch`, which refuses a session it did not register and a server that session was not given. The launcher clears its environment, sets `PATH`, `HOME`, `LANG`, `TMPDIR` and the server's keys, and execs the server. No key is written to `mcp.json`, and the model credential never reaches a connector.

**What was connected on this machine is what runs.** `connect` keeps `spec_sha256` beside the keys: the sha256 of the canonical JSON (compact, object keys sorted) of the entry's `transport`, `command`, `args`, `url`, `headers`, `credential_keys` and `tools`. The name is left out, because it is part of where the keys are kept. A server whose entry hashes differently, or has none, is left out of the session, and the launch route refuses it with `connector_not_confirmed` as a second check. The agent page shows it as "Connect again". `tools` is in the hash, so a commit cannot relabel a `denied` tool `network`.

**Keys are per agent.** They are kept in the keychain under service `farik`, account `connector:<project_id>:<agent_id>:<server>`, as one JSON object `{ spec_sha256, keys }`. On a computer without a keychain they go to `connectors.json` in the user's Farik state folder, keyed by the same account string, as ADR 0022 does for the model credential: the folder 0700, the file 0600. With neither, connecting is refused `no_secret_store`.

**Connecting is step 01's.** `farik connect`, `farik disconnect`, their RPCs and the events `connector.connected` and `connector.disconnected` land with custom servers. Step 05 adds a kit's connectors to the same commands and screens.

## Consequences

A cloned project, or a teammate's change to a connector, runs nothing until the user connects it again on their own machine. That protects the user, and it is one more step after every change, including their own edit by hand.

Two agents with the same server each need the key entered. The user types it twice. In return, removing one agent's access never touches another's.

Without the sandbox, a program running as the user can reach a key three new ways: by running the launcher with `daemon.json`'s token, by calling the launch route directly, or by reading a connector's `/proc/<pid>/environ`. Spec 8.6's no-sandbox warning names them. A user's stdio server also runs on the host with the user's rights: it is code the user chose.

On a computer without a keychain, the keys are in a file that any program running as the user can read, as with ADR 0022.

The canonical JSON relies on `serde_json` being built without `preserve_order`. If a dependency ever turns that feature on, every hash changes and each server asks to be connected again, which fails safe. A test pins the order.
