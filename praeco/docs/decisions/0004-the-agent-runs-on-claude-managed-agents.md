# 0004. The agent runs on Claude Managed Agents, and every outside effect is a Praeco tool

Date: 2026-10-24
Status: proposed

## Context

Praeco's agent runs for minutes at a time, many times a day, on servers, with no person watching. The options:

- **Claude Managed Agents** (beta): Anthropic runs the agent loop and a sandbox per session; agents are versioned objects; sessions take a hard spend cap (a session budget); memory stores mount into the sandbox, read-only if asked; custom tools are executed by the caller, which receives `agent.custom_tool_use` and answers with `user.custom_tool_result`.
- **The Claude API with a tool loop in Praeco's process**: no sandbox, Praeco owns the loop, context management and compaction.
- **The Claude Agent SDK (Claude Code as a library) on Praeco's servers**: Catervas's engine, which brings back a sandbox to run and secure ourselves.

## Decision

The agent runs on Claude Managed Agents. One agent definition (`agents/praeco/agent.md` and its tools), synced by `praeco agent sync`, which records the agent's id and version in the database; one cloud environment per installation; one session per cycle.

- **Built-in tools:** the file tools on the sandbox's scratch folder, and `web_search` with the source policy's `blocked_domains` (SPEC 5.8). `bash` is off, so the sandbox has no way to reach the network itself, and the environment's networking is `unrestricted` only because `limited` with no hosts would also blank `web_search`. The built-in `web_fetch` is off: pages are read through Praeco's `read_page`, which applies `robots.txt` and the blocked list.
- **Every effect outside the sandbox, and every read of a connected service, is a Praeco custom tool.** Praeco's `work` process receives the call, asks the governor (`@praeco/core` `decide`), executes it with the connection's credentials, which never enter the sandbox, and answers. A refusal is answered as the tool's result, with its reason.
- **Memory stores:** `playbook` and `knowledge`, per workspace, mounted read-only. Only Praeco's process writes them, from the database, after the governor.
- **Session budget** set to the cycle's money limit, the second wall behind Praeco's own count (SPEC 5.5).
- **Model:** Claude Opus 5.5; Claude Sonnet 5.5 when the owner chooses it.

Rejected: the API with our own loop, because the sandbox, compaction and versioned agents are work Managed Agents already does; the Agent SDK, because it brings back the sandbox Catervas spent phases securing.

## Consequences

The runtime Praeco owns is small: a session runner and a tool dispatcher. The governor stays Praeco's own code, which is the product's claim. Self-hosters need only an Anthropic API key.

What cuts against it: Managed Agents is in beta, so its event shapes may change (the SDK is pinned, and the runner's tests run against a fake server whose events are copied from recorded live sessions, refreshed when the SDK is upgraded); Praeco depends on one provider, as Catervas does in practice; and a session's sandbox is Anthropic's, so customer research passes through it, which the privacy policy (phase 8) says.
