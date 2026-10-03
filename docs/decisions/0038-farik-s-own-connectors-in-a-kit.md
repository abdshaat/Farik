# 0038. A kit starts Farik's own connector by the bare name `farik`, run as Farik's own program

Date: 2026-10-02
Status: accepted (phase 7 step 07, readiness-reviewed 2026-10-02)

Builds on ADR 0036 (a kit's `stdio` connector starts only a pinned package runner or an absolute path to a binary Farik ships) and ADR 0020 (a service's official server, else a pinned community one, else a thin one of Farik's).

## Context

No usable server reads the open vulnerability database, so the Architect's kit needs a thin one of Farik's own, shipped in the Farik binary. A kit cannot know where Farik is installed, so ADR 0036's absolute-path rule cannot be met by a file the binary ships. Two other ways were weighed: a separate `farik-osv` binary, which needs the same path rule and a second release artefact; and Farik tools (`farik_*`), which follow the agent's tiers, so a Developer without `network` could never have one.

## Decision

**The exact pair.** A kit may write `command: farik` with `args` exactly `[connector, <name>]`, `<name>` one of `FARIK_CONNECTORS` (today `osv`). `is_farik_connector(command, args)` is true for that pair and nothing else. The loader accepts the bare word `farik` only then: `farik` with any other `args`, and every other bare program (`farik-osv`, `./farik`, `bin/farik`, `farikx`, `FARIK`, `farik.exe`, `farik.cmd`), stays `package_not_pinned`. The absolute-path rule of ADR 0036 is unchanged.

**Run as Farik's own executable.** `program(command, args, farik)` answers Farik's own executable for the exact pair, whatever the entry's `source`, and `command` as given for everything else, so a user's own `farik` command (a custom connector) is found on `PATH` as before. Listing a connector's tools (the daemon and `farik connector add`) and `farik connector run` (the session's launcher) both use it. When Farik cannot find its own executable (`current_exe` fails; found once, when the process starts, and handed to the daemon and to each command), listing and launching fail with `farik could not find its own program`; they never fall back to `PATH`, so no `farik` another program put earlier on `PATH` is ever run in its place.

**The hash keeps `farik`.** The entry, and so `spec_sha256`, say `command: farik`, so the server's code changes only with the binary, which is ADR 0036's promise: a new Farik release is a new server, and the same pin review covers it.

**What the server may do** is the plan's: a fixed host, no redirects, no proxy, a 25 second limit, a 4 MiB limit read in chunks, one page of at most 50 advisories, every input checked, and only package names and versions sent (step 07, Task 4).

## Consequences

`farik-e2e-serve`'s executable is not `farik`, so no browser suite connects such a server through it. A user behind a proxy gets a tool error from the OSV server, since it runs with the cleared environment every connector does. Adding a Farik connector means adding its name to `FARIK_CONNECTORS` and a kit entry in a Farik release.
