# 0037. Allowances count calls per agent and period

Date: 2026-10-02
Status: accepted (phase 7 step 05b, readiness-reviewed 2026-10-02)

Builds on ADR 0031 (a connector's `external_effect` call waits for the human, one grant per call) and ADR 0036 (a kit's connector is written whole in the team file and trusted only when it is the kit's). ADR 0031 named allowances as the one pre-authorisation for connector tools.

## Context

A service that spends the user's credits, a picture generator for one, would ask the human for every call under ADR 0031. A kit can say how many calls each sprint an agent may make without asking. That number is a pre-approval, so it must be held where the user set it, counted from what was done, and never granted by a committed file.

## Decision

**Where it lives, and the hash.** A kit entry in `team.yaml` holds `allowances: { <tool>: 0..1000 }`, per agent. `spec_sha256` holds `"allowances"` only when it is not empty, so every hash kept before stands, and a commit that raises an allowance makes the server "Connect again": a committed file cannot grant one (ADR 0031). `validate_team` refuses one on an entry that is not `source: kit` (`allowance_not_kit`) and one for a tool the entry does not tag `external_effect` (`allowance_not_external`). `matches_kit` compares an entry with its allowances taken out and requires each allowance's tool to be one the kit offers one for.

**The period.** While a sprint is open, the calls since its `sprint.started`. With none open, the calls since the later of the UTC day's start (the daemon's clock) and the last `sprint.ended`: a project that runs no sprints, and the hours between sprints, count the day (as 5.5's daily budget does) without counting a sprint's calls again after it ends.

**What counts.** Every `tool.called` of that agent, server and tool in the period, the calls a grant allowed among them, since each spent the user's credits. The count matches `ToolCalledBody.server` to the server's name and `ToolCalledBody.tool` to the full `mcp__<server>__<tool>`, never the bare name another service may share. A count may exceed the allowance, since granted calls count; the screens show it as it is ("21 of 20 images") with "Extra images were ones you approved."

**The order in the hook.** The session's connector, the tag, `denied`, the preview's `url`s, the Designer's plan gate, the 64 KiB limit, then the human's grant for this exact call, then the allowance, then the ask. A grant is used before the allowance, because it is for exactly this call and lapses otherwise. A call the allowance lets through records `tool.called` with `allowance: <n>`. An allowance of 0 asks every time, and a tool with none always asks.

**How it is counted.** `tool.called` is recorded for every allowed call, so reading the log on each spending call would decode thousands of events. The daemon keeps a count per (agent, server, tool) for the period it last read, filled from the log the first time a key is asked for (`calls_in_period`) and raised by one after each `tool.called` that used a grant or an allowance. A different period drops every count; a restart starts again from the log. The count is taken, and the `tool.called` it is checked against recorded, under the daemon's one sessions lock, which the grant already takes, so two calls at the last place cannot both run. Rejected: a migration and a table for a number the daemon can hold.

**Changing it later** is `connector.allowances { agent, server, allowances }`: with the entry's lock held, the kept entry must be the team file's entry now (`connector_not_confirmed`), the numbers are checked as at connect over the ones the entry holds, the same keys or sign-in are kept beside the new hash, and `connector_connect` is handled, which records `connector.connected` with the allowances. A kit that changed the service since is `connector_not_in_kit`: connect it again. Raising a number does not allow a call that is waiting.

## Consequences

Limits, kept honest: a session's allowances are fixed when it registers, so a change applies from the agent's next session, as tiers do (spec 4.4); and the count's lock is per daemon, which holds because one process drives a project (the CLI sends its commands to the daemon when one runs). The day's start is found by reading back from the newest call, which assumes the log's times rise with its sequence. An allowance counts calls, not money: Farik cannot price another service's credits. A page reading the count at the instant a granted call is recorded can count it twice until the period changes or the daemon restarts; that asks one call early, never late.
