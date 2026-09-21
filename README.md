<div align="center">

# Farik

**An operating system for small teams of AI agents, with a governance harness at its core.**

Farik runs two to seven AI agents against one git repository. Every task carries a written
contract before anyone starts, a deterministic governor enforces budgets, permissions and paths,
and nobody accepts their own work.

[![check](https://github.com/abdshaat/Farik/actions/workflows/check.yml/badge.svg)](https://github.com/abdshaat/Farik/actions/workflows/check.yml)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.98.1-dea584.svg)](rust-toolchain.toml)
[![status](https://img.shields.io/badge/status-pre--release-orange.svg)](#project-status)

[Specification](docs/SPEC.md) · [Roadmap](#roadmap) · [Contributing](CONTRIBUTING.md) · [Decisions](docs/decisions/)

</div>

---

## Project status

**Pre-release. Farik does not yet run agents.** The harness underneath them is built and tested; the
runtime that drives model sessions is the phase currently being worked on. There is no installable
release, no desktop application, and no stable API.

| Phase | What it delivers | State |
|---|---|---|
| 0 — Foundation | Cargo workspace, `cargo xtask check`, schema-generated contract types | Merged |
| 1 — Harness core | `farik-core`: every governance rule of the spec as pure functions | Merged |
| 2 — Protocol, store, CLI | Event log, projections, git adapter, `.farik/` files, project scan, eleven commands | Merged |
| 3 — Runtime and Milestone 0 | Claude Code sessions, sandbox, orchestrator; the first end-to-end run | In progress |

What you can run today is the command line in [Try it](#try-it): it makes a repository a project,
files contracts, records events, and reports drift. What it cannot do yet is hand a contract to an
agent and get code back.

## Why

Point a swarm of agents at a repository and the failure is rarely a bad model. It is that nothing
stops the team from talking itself into a rewrite, that no one can reconstruct why a file changed,
and that the agent which wrote the code is the one that declares it correct.

Farik's answer is that governance belongs in code, not in prompts. A system prompt saying "never
push to `main`" is a suggestion. A governor that returns a permission error is a rule.

Five principles hold the design together, and the rest of the spec follows from them:

- **Contracts before work.** No agent starts a task that lacks a contract passing the Definition
  of Ready: intent, exit criteria with a verification method for each, budget, risk, an explicit
  out-of-scope list, and a named reviewer.
- **Nobody grades their own homework.** The reviewer on a contract is never the assignee.
  Verification runs in a fresh session that never sees the assignee's transcript — only the
  contract, the diff, the completion note, and the tools to run the criteria.
- **Governance is code, not prompts.** A deterministic governor sits between every agent and every
  tool, checking permissions, budgets, iteration limits and path allowlists. Agents propose; the
  governor disposes.
- **Bounded everything.** Every session has a token budget, a wall-clock limit, a tool-call limit
  and an iteration limit. Hitting one is a handled outcome that raises an escalation, not a crash.
- **Everything is a file, every action is an event.** Contracts, decisions and memories are plain
  files in `.farik/`. Every tool call, transition, message and expense lands in an append-only log.

Prompt injection through the repository is assumed, not hoped against. An injected instruction can
make an agent *request* a push; the request is denied in code.

## How it works

A **team** of two to seven **agents** is attached to one **project** — a git repository plus a
`.farik/` directory holding the backlog, contracts and decisions as plain files. A request from you
is triaged into an **epic** or a single **task**. Nothing starts until its **contract** passes the
Definition of Ready, and nothing is accepted until it passes the Definition of Done.

```
draft ──▶ refining ──▶ ready ──▶ assigned ──▶ in_progress ──▶ verifying ──▶ accepted
             │            │          │             │              │
             │            │          │             ▼              ▼
             │            │          │          blocked        rejected ──▶ in_progress
             │            │          │             │                          (bounded)
             ▼            ▼          ▼             ▼
         escalated    escalated  escalated ──▶ (human decides) ──▶ any state, or cancelled
```

The governor owns the transitions marked as its own, and it is the only thing that may write a
task's status. `accepted` and `cancelled` are terminal. Every move is an event; a refused move
carries a reason that goes back to the agent and into the log.

Five roles ship at launch — Product Manager, Scrum Master, Architect, Software Developer,
Marketing Specialist — each a file defining a mandate, the artifacts it produces, the gates it
owns, its default tools, and what it is forbidden from doing.

## Try it

Farik is not released. To build from source you need [`rustup`](https://rustup.rs); it reads
`rust-toolchain.toml` and installs the pinned Rust 1.98.1 with `rustfmt` and `clippy` on first use.

```console
$ git clone https://github.com/abdshaat/Farik.git
$ cd Farik
$ cargo build --release
```

The binary lands at `target/release/farik`. The session below assumes it is on your `PATH`.

Make any git repository a project:

```console
$ cd ~/code/your-project
$ farik init
last commit today
wrote .farik/team.yaml: product-manager, developer
no criteria: nothing in this repository says how it is tested
```

`init` scans the repository, writes `.farik/`, opens the event log under `.farik/local/` (machine
local, never committed), and seeds a criterion library from whatever the repository says about how
it is tested. With no team file it writes a starter team of the two roles a team cannot work
without. Running it again is a rescan: it keeps what a person wrote and replaces what the last scan
found.

File a contract as a draft request, size it, and read it back:

```console
$ farik task create request.yaml
FRK-1 filed as a draft request: Show the board without a database client
farik triage says whether it is large or small; nothing starts before that (5.16)

$ farik triage FRK-1 small --reason "One command, one file."
FRK-1 is small: task. One command, one file.

$ farik board
FRK-1     task  draft       low    Show the board without a database client

$ farik task show FRK-1
FRK-1 Show the board without a database client
draft task, low risk

intent: A person can read the board without opening a database.

requirements
  R1 The board prints one line per task.
exit criteria
  C1 Every test in the workspace passes. [test]

events
     4 2026-09-21T18:05:51Z task.created
     5 2026-09-21T18:05:51Z request.triaged
```

Everything that happened is in the log, and `--json` turns any command into machine-readable output:

```console
$ farik log
   1 2026-09-21T18:05:37Z team.updated       -
   2 2026-09-21T18:05:37Z project.scanned    -
   3 2026-09-21T18:05:37Z criteria.updated   -
   4 2026-09-21T18:05:51Z task.created       FRK-1
   5 2026-09-21T18:05:51Z request.triaged    FRK-1

$ farik --json log --limit 1
{"body":{"agent_ids":["product-manager","developer"],"team_name":"demo","updated_by":"human"},"kind":"team.updated","project_id":"demo","recorded_at":"2026-09-21T18:05:37.537714218Z","seq":1,"team_id":"demo"}
```

### Commands

| Command | What it does |
|---|---|
| `farik init` | Make the repository a Farik project; rescan an existing one |
| `farik task create <file>` | File a YAML contract as a draft request |
| `farik task show <id>` | Show one contract and everything that happened to it |
| `farik triage <id> <large\|small>` | Record how big a request is, or overrule the triage that did |
| `farik contract lock <id>` | Take a contract from the team; agents may then only record results |
| `farik contract unlock <id>` | Give the contract back to the team |
| `farik board` | Show the lifecycle, one line per task |
| `farik log` | Show the event log; filter by `--task`, `--kind`, `--limit` |
| `farik rules show` | Print the team rules every command and path check is held to |
| `farik criteria list` | Print every criterion a contract may refer to by name |
| `farik doctor` | Report every way the files and the log disagree; exits 1 when it finds something |

`--json` is global. Add it to any command for machine-readable output.

## Architecture

Local first. The orchestrator and governor run on your machine, agents execute in a sandbox on that
machine, and the project stays where it is. A Rust backend and a TypeScript front end share one
repository ([ADR 0005](docs/decisions/0005-rust-backend.md)).

```
crates/
  core/       farik-core      governance rules, contracts, state machine, cost model — no I/O
  protocol/   farik-protocol  event, command and RPC types shared by daemon and front end
  store/      farik-store     SQLite event log, projections, .farik/ file adapters, git adapter
  cli/        farik           the binary
xtask/                        the repository's own commands (check, generate, hooks)
```

Planned, not yet present: `crates/runtime` (agent sessions, sandbox, orchestrator),
`crates/roles` (shipped role definitions), `packages/ui`, `apps/desktop` (Tauri), `apps/web`.

Three constraints shape the code, and the check command enforces the first two:

- **`farik-core` does no I/O.** Every rule of the spec's harness section lives there as a pure
  function, tested in isolation. `cargo xtask core-io` fails the check if that slips.
- **The schema is the source of truth.** JSON Schema 2020-12 in `docs/schemas/` generates the Rust
  types through `cargo xtask generate`; generated files are committed and checked for staleness.
- **The log says what happened; the files say what the team knows.** The event log is machine-local
  under `.farik/local/`; `.farik/` itself travels with the repository. On startup the two are
  reconciled and any drift is reported rather than silently resolved.

Event kinds are named `<entity>.<past_tense_verb>` — `task.transitioned`, `tool.denied`,
`cost.recorded`, `escalation.raised`. Wire and file formats are `snake_case`; TypeScript is
`camelCase`, mapped once at the edge.

## Roadmap

Milestones, from [the specification](docs/SPEC.md#11-milestones):

- **Milestone 0 — the harness.** Core, store, runtime with one adapter, a command line, and two
  roles, with one Developer reviewing the other. It exists to find out whether the governance loop
  works before anyone draws a pixel. *(Current.)*
- **Milestone 1 — the team.** All five roles, the channel, ceremonies, the desktop shell with the
  board, and a first version of the pixel office.
- **Milestone 2 — the ecosystem.** Per-agent MCP servers and skills, one-on-one conversations,
  memory, the audit viewer, notifications. The public open-source launch closes this milestone.
- **Milestone 3 — premium.** Hosted execution on the same event protocol, licensing and billing,
  cloud sync.

The phase-by-phase breakdown, with a plan per step, is in [`docs/plans/`](docs/plans/).

### Open source and premium

One codebase under Apache 2.0, with premium code confined to an `ee/` directory under a separate
commercial license. There is no `ee/` directory yet.

Free, forever: the full harness, all five roles, the pixel office, local execution with your own
API key, MCP and skills, the audit log. Premium covers hosted execution, cloud sync, extended cost
history, themes and support. Nothing that makes the agents safer or more controllable is ever
premium — a governance layer people cannot audit is not one they will trust with their repositories.

## Documentation

| Document | What is in it |
|---|---|
| [Specification](docs/SPEC.md) | Goals, the harness, roles, architecture, requirements, milestones |
| [Product analysis](docs/PRODUCT_ANALYSIS.md) | The problem, the landscape, where the idea is weak, business model |
| [Project plan](docs/plans/project-plan.md) | Phases and steps, with a plan per step |
| [Decisions](docs/decisions/) | Architecture decision records |
| [Workflow](docs/standards/workflow.md) | How a change moves from idea to `main` |
| [Code standards](docs/standards/code.md) | Naming, style and the toolchain |
| [Schemas](docs/schemas/) | JSON Schema for contracts, events, commands, teams, criteria, prices |

## Contributing

Farik holds itself to the discipline it imposes on the agent teams it runs, so the process is
stricter than most projects this size: no production code before a failing test, no completion
claim without pasted evidence, and no one accepts their own pull request. Read
[CONTRIBUTING.md](CONTRIBUTING.md) first — it is short, and it points at the three standards that
matter.

```console
$ cargo xtask install-hooks   # pre-commit and commit-msg hooks
$ cargo xtask check           # format, clippy, tests, generated-file freshness, TODOs, core no-I/O
```

`cargo xtask check` is the single command that means "is this mergeable", and it is what CI runs on
every pull request. `cargo fmt --all` rewrites files to the house style.

## License

[Apache 2.0](LICENSE).
