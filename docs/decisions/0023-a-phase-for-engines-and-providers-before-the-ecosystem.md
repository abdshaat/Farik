# 0023. A phase for engines and providers before the ecosystem

Date: 2026-09-29
Status: accepted
Amended 2026-09-30 by ADR 0025: phase numbers after 9 moved up by one, and every role kit must be tested and work on every supported engine and provider.
Amended 2026-10-01 by ADR 0029: the role kits come first, as phase 7, built on Claude; engines and providers become phase 8 and re-check every kit; the ecosystem's rest is phase 9; the Milestone 1 test still runs on Claude, at the end of phase 7.

## Context

On 2026-09-29 the founder decided two things. Farik's users must be able to connect any AI account: Claude, OpenAI, and more. And tools, skills and connectors must work with every agent engine, including open-source ones such as Hermes and OpenClaw.

Farik today runs on one engine:
- Agents run through the `RuntimeAdapter` trait (`crates/runtime/src/session.rs`). Its one production implementation drives the Claude Code program as a child process (`ClaudeAdapter`, ADR 0004, ADR 0005); the other, `RecordedAdapter`, replays recorded transcripts in tests.
- Farik's own tools reach agents over Farik's own MCP server, which any engine that speaks MCP can use.
- Skills use the Agent Skills format (`SKILL.md`), and connectors are MCP servers. Both are open standards that other engines have adopted.
- The Claude-specific parts are: the governor's per-tool-call enforcement through Claude Code's `PreToolUse` hook (`farik hook`); the stream parsing that reads cost and the provider's usage limit; the price table; and the models the team builder offers.

The open problem is the governor. It judges every action at the `PreToolUse` hook. An engine without an equivalent hook could act without the governor seeing it.

The plan had no phase for this. The ecosystem (phase 7, MCP and skills per agent) and the role kits (phase 8) would have built connectors and skills against the Claude Code program alone.

## Decision

The founder decided the timing on 2026-09-29:
- Phase 6, the web UI, finishes on Claude as planned. The Milestone 1 test runs on one engine.
- A new phase 7, Engines and providers, comes right after phase 6 and before the ecosystem. It gets its own brainstorm and ADR when it is next to be planned. It must keep the governor's enforcement for every engine: an engine without an equivalent hook is limited to Farik's own MCP tools, so that the governor still sees every action.
- The ecosystem and the role kits follow it, so connectors and skills are built engine-neutral from the start.
- Phase 6 step 05 stores the provider with the credential, so that nothing there is redone.

The phases from the ecosystem on move up by one: Ecosystem 8, Role kits 9, Web launch 10, Desktop 11, Native mobile 12, Premium 13. Milestone 2 now includes engines and providers.

## Consequences

The web launch is one phase further away. In exchange, no connector or skill is built twice.

The `RuntimeAdapter` trait gets its second production implementation, and with it a stability promise (spec 8.2). The stream parsing, the price table and the team builder's model list become per engine or per provider.

An engine with no hook works with fewer tools than Claude Code: only Farik's own MCP tools, not its built-in file or web tools. Phase 7's ADR decides how much of Farik's tool server must grow to cover what those built-ins did.

Accepted ADRs that name a later phase by its current number (0017, 0019, 0020, 0021) carry a one-line amendment pointing here, and keep their text.
