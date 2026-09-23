# 0014. Commands reach a running farik through its daemon

Date: 2026-09-23
Status: accepted

## Context

`farik run`, `farik plan`, and `farik contract new` each drive a project: an orchestrator with its sessions, its stop flag, and the `Transitions` lock that serialises what one process asks the governor (phase 3 step 04, which is per process). A person answers a question, approves a contract, or stops the run from another terminal while one of them runs. Step 14 left open how such a command reaches the process it concerns.

Three options were on the table.

- Append the command to the log and let the driver's next tick read it. That reaches no session and no stop flag, so `farik stop` could not stop a run, and two processes would ask the governor about one task at the same time, which the per-process lock does not prevent.
- Route only the stops to the driver and handle every other command in the terminal it was typed in. The stops work, but the race on the governor stays for every other command.
- Send every command to the driver while one runs, through its daemon, and handle it in-process when none does.

Which process drives is decided by a lock, not by `daemon.json`: a crashed run leaves the file behind, and two runs starting together both find it missing.

## Decision

One process drives a project: `run`, `plan`, and `contract new` hold an exclusive `File::try_lock` on `.farik/local/run.lock` for their lifetime. A command typed in another terminal is sent, while the lock is held, as the command wire to `POST /command` on the holder's daemon, with the token from `daemon.json`, and the reply is printed; while the lock is free, the command is handled in the terminal's own process, which holds the lock while it runs. The reply has a schema of its own, `$defs/commandReply` in `command.schema.json`. A command the daemon cannot be asked about, or that goes unanswered for 120 seconds, is refused with a sentence that says it may still take effect; it is never retried in-process, which could apply it twice. Filing a request stays a direct store call in any process, because it creates a new task and races with nothing.

## Consequences

Every command reaches the one orchestrator whose sessions, stop flag, and governor lock it concerns, so `farik stop` and `farik stop <session>` work from any terminal, and no two processes ask the governor about one task at once. Phase 5's front end sends the same commands to the same route.

The daemon now takes writes as well as hooks, behind the same token, so anything that can read `daemon.json` (its owner alone, mode 0600) can drive the team. A command typed while the lock holder is starting and has not yet written `daemon.json` is refused and must be typed again. A command that timed out leaves the person to read `farik log` to learn whether it took effect. The run lock is advisory: a process that does not take it, such as an older `farik`, is not kept out.
