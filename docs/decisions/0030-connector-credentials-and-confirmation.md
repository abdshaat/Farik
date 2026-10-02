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

**A launcher carries the keys.** A session's `mcp.json` names a stdio connector as `farik connector run --daemon <daemon.json> --session <id> --server <name>`, and gives an http connector the `headersHelper` `farik connector headers` with the same arguments. Both ask the daemon's authenticated `POST /connector/launch`, which refuses a session it did not register and a server that session was not given. The launcher clears its environment, sets `PATH`, `HOME`, `LANG`, `TMPDIR` and the server's keys, changes to a folder Farik keeps for the server outside the repository, `connectors/<project id>/<agent>/<server>` in the user's state folder, and execs the server; listing its tools at connect runs it in the same folder. No key is written to `mcp.json`, and the model credential never reaches a connector.

A stdio server never runs in the task's worktree, where Claude Code starts the launcher (corrected after the landing review), nor anywhere in the repository (corrected again after the re-review). Agents write to the worktree, and `npx`, `python -m`, `uv run` and a relative argument all resolve there, so a planted `node_modules/.bin` or module would run on the host with the agent's keys. The first correction used `.farik/local/connectors/<agent>/<server>`, which is still in the repository: git checks out a force-added file there on a clone or a pull, and `npx`, `uvx` and `uv run` look upward and adopt the root's `.npmrc`, `node_modules/.bin` and `pyproject.toml`. So the folder is in the state folder, beside `connectors.json`, and is removed and made again, empty and 0700, for every listing and every launch, so nothing from before is in it. Being outside the repository is all it promises: a program on `PATH` that reads a project of its own accord, or is given one as an argument, is the user's choice. For the same reason the team file's `command` is a bare name found on `PATH` or a full path: one holding a `/` that is not absolute is refused `command_not_absolute`.

A refusal is not enough for an http server. Claude Code (probed on 2.1.287) connects it without its headers when the helper fails, and offers its tools. So the launch route takes a server it refuses from the session's registration, and the hook denies its calls `connector_not_in_session`; session setup gives a server only when every key it names is kept. The server, at its confirmed address, is sent no key.

**What was connected on this machine is what runs.** `connect` keeps `spec_sha256` beside the keys: the sha256 of the canonical JSON (compact, object keys sorted) of the entry's `transport`, `command`, `args`, `url`, `headers`, `credential_keys` and `tools`. The name is left out, because it is part of where the keys are kept. A server whose entry hashes differently, or has none, is left out of the session, and the launch route refuses it with `connector_not_confirmed` as a second check. The agent page shows it as "Connect again". `tools` is in the hash, so a commit cannot relabel a `denied` tool `network`.

**Keys are per agent.** They are kept in the keychain under service `farik`, account `connector:<project_id>:<agent_id>:<server>`, as one JSON object `{ spec_sha256, keys }`. `<project_id>` is the project's id on this machine, 32 random hex digits kept in the user's state folder at `projects/<sha256 of the project's canonical path>`, not the event log's `project_id`, which is the folder's name: with it, `~/work/app` and `~/clients/app` shared an agent's keys, and a clone into a folder of the same name found them (corrected after the landing review; nothing kept under the old address was released, so nothing is moved). The first correction kept the id in `.farik/local/project_id`, which `cp -r` copies, so a copied project found the first one's keys; bound to the path it is not (corrected again after the re-review). A project moved to another path is connected again. On a computer without a keychain they go to `connectors.json` in the user's Farik state folder, keyed by the same account string, as ADR 0022 does for the model credential: the folder 0700, the file 0600. With neither, connecting is refused `no_secret_store`.

**Connecting is step 01's.** `farik connect`, `farik disconnect`, their RPCs and the events `connector.connected` and `connector.disconnected` land with custom servers. Step 05 adds a kit's connectors to the same commands and screens.

## Consequences

A cloned project, or a teammate's change to a connector, runs nothing until the user connects it again on their own machine. That protects the user, and it is one more step after every change, including their own edit by hand.

Two agents with the same server each need the key entered. The user types it twice. In return, removing one agent's access never touches another's.

Without the sandbox, a program running as the user can reach a key three new ways: by running the launcher with `daemon.json`'s token, by calling the launch route directly, or by reading a connector's `/proc/<pid>/environ`. Spec 8.6's no-sandbox warning names them. A user's stdio server also runs on the host with the user's rights: it is the program the user named, found on `PATH` or by its full path, run in an empty folder outside the repository, which neither an agent nor a commit writes to. The hash pins the strings, not what `PATH` finds, which is the user's own.

On a computer without a keychain, the keys are in a file that any program running as the user can read, as with ADR 0022.

The canonical JSON sorts object keys itself, at every depth, so a dependency turning on `serde_json`'s `preserve_order` does not change a hash. A test builds a map in reverse order to pin it.
