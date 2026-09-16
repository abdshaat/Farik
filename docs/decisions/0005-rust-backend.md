# 0005. The backend is Rust; TypeScript is for the user interface only

Date: 2026-09-15
Status: accepted (founder's decision of 2026-09-15; this record writes it down)

## Context

ADR 0002 chose a TypeScript monorepo for everything, and `docs/SPEC.md` 0.1 built the first runtime on the TypeScript Claude Agent SDK. Planning the desktop phase raised the question of how the long-running local process (the daemon) ships inside a Tauri app, and the founder answered by deciding that the backend is rewritten in Rust: the governor, the event log and projections, the file and git adapters, the runtime, the orchestrator, the daemon, and the command line. The React front end and the pixel component library stay in TypeScript, because the Tauri webview and the future hosted web app run JavaScript.

Three consequences had to be resolved for the decision to hold.

The agent engine. There is no Rust Claude Agent SDK. The TypeScript SDK is a wrapper around the Claude Code command-line program, which exposes the same engine non-interactively: `claude -p` with `--output-format stream-json` and `--input-format stream-json` streams every assistant message, tool call, tool result, and usage report as JSON lines, and takes MCP servers, permission rules, hooks, a system prompt suffix, a model, and a turn limit as flags. A Rust runtime drives that program as a child process. Hooks are shell commands, so the `PreToolUse` and `PostToolUse` hooks are `farik hook <event>`, which read the hook's JSON on stdin and call the daemon on localhost; the daemon evaluates the governor and answers allow or deny with a reason, and the hook returns that to Claude Code, which enforces it. The Farik tools are an MCP server the daemon serves, and the permission-prompt tool is a second MCP tool that denies anything the hook did not already decide.

The schema pipeline. JSON Schema stays the source of truth (`docs/standards/code.md`). `typify` generates Rust types from each schema, with `serde` attributes that apply the schema's defaults and newtypes that enforce its patterns and lengths; `jsonschema` validates a value against the schema before it is deserialized. Rust field names are `snake_case`, the same as the wire, so the backend needs no naming mapping layer; the rule that wire formats are `snake_case` and TypeScript is `camelCase` still applies to the front end, which keeps one mapping layer at the daemon client.

The toolchain. Cargo replaces pnpm for the backend: `rustfmt`, `clippy` with warnings as errors, `cargo test`, and an `xtask` crate that owns the repository's own commands (`check`, `generate`, `install-hooks`, `commit-msg`, `todos`), so that no Node toolchain is needed until the front end exists. `cargo xtask check` is the single command that means "mergeable"; when the front end arrives it also runs the front end's checks. `git-cliff` replaces Changesets for the changelog, because the changelog is derived from Conventional Commits either way. SQLite is `rusqlite` with the bundled engine.

The alternative, keeping the backend in TypeScript and shipping a Node runtime inside the desktop app as a sidecar, was the plan's recommendation. The founder chose Rust for the backend, and this record supersedes ADR 0002 for everything outside `packages/ui`, `apps/desktop`, and `apps/web`, and amends ADR 0004's mechanism (a child process and hooks instead of the SDK's in-process hooks) without changing its decision that sessions run on the host and only commands run in the sandbox.

## Decision

Every crate under `crates/` is Rust: `farik-core` (no I/O), `farik-protocol`, `farik-store`, `farik-runtime`, `farik-roles`, and the `farik` binary. The runtime drives the Claude Code command-line program as a child process, with governor decisions delivered through hook commands that call the daemon and with Farik tools served over MCP. The front end stays TypeScript and React. `cargo xtask check` is the check command.

## Consequences

The whole backend has one language, one build, one test runner, and a single static binary; the desktop app links the daemon in-process instead of supervising a sidecar, and `farik` on the command line is that same binary.

Every plan and standard written for TypeScript is redone: `docs/standards/code.md` gains Rust naming and toolchain rows, `docs/plans/project-plan.md` revision 4 restates every interface in Rust, and the Phase 0 step plans are rewritten and re-verified. The `Result` type step disappears, because Rust's `Result` is the language's own.

The runtime depends on the Claude Code program being installed and on the stability of its `stream-json` output, its hook contract, and its flags. Those are documented and versioned, and the runtime pins the minimum version it was tested against and refuses to start on an older one, but a breaking change in the program is a breaking change for Farik, and the test suite includes recorded `stream-json` transcripts so that such a change is caught by a test rather than by a user.

Contributors need a Rust toolchain, and the front-end contributor needs both toolchains. Compile times are longer than TypeScript's; the `xtask` crate keeps the day-to-day commands fast by running `cargo check` and tests per crate.
