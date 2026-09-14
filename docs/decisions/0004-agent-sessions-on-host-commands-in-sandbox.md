# 0004. Agent sessions run on the host; only commands run in the sandbox

Date: 2026-09-14
Status: proposed

## Context

`docs/SPEC.md` section 8.2 builds the first runtime adapter on the Claude Agent SDK, and section 8.3 says agents with `execute` run commands inside a container per task with the project directory mounted. The spec does not say where the SDK session itself runs, and the two obvious answers have different consequences.

Running the whole session inside the container means installing Node, the SDK, and the model API key into every task container. The governor hooks would run inside the container too, next to the code the agent can execute, and the API key would sit in an environment the agent can read with one command. Network isolation would have to be relaxed for the model API on every container. The built-in Bash tool would work unchanged, which is the one attraction.

Running the session on the host and routing only command execution into the container keeps the API key, the governor hooks, the event log, and the project files on the host. The SDK's built-in Bash tool is disallowed for every session; `execute` is exposed as a Farik tool, `farik_exec`, whose handler runs the command in the task's container through Docker, or on the host when the user has chosen no-sandbox mode. The SDK's WebFetch and WebSearch tools are allowed only when the agent holds the `network` tier, which is what gives the Product Manager its research ability. File tools (Read, Glob, Grep, Edit, Write) operate on the mounted project directory from the host, governed by the same `PreToolUse` hook that checks tiers and `allowed_paths`. Network access for the model is the host's; network access for commands is the container's, off unless the role has `network`.

Git needs its own tool because two permission tiers, `git_local` and `git_remote`, name git actions and a shell tool cannot tell them apart. `farik_git` runs on the host through the store's git adapter: `status`, `diff`, and `commit` carry `git_local`, `push` carries `git_remote`, and each is checked as its own tool. `farik_exec` refuses a command whose first word is `git`. No git credential is ever mounted into a container, so a push attempted from inside one fails on authentication whatever wrapper hides it; a local commit hidden behind a shell wrapper inside the container is the one thing this arrangement cannot stop, and it is recorded below as accepted.

The cost of the second option is that the agent's shell is a Farik tool rather than the SDK's own Bash tool, so command output streaming and timeouts are Farik's to implement, and the sandbox boundary protects the project from commands but not from the host-side file tools, which the governor's path check protects instead.

## Decision

Agent sessions run on the host through the Claude Agent SDK. The built-in Bash tool is disallowed in every session; the built-in WebFetch and WebSearch tools are allowed only under the `network` tier. Command execution is a Farik tool backed by an `Executor` interface with a Docker implementation and a host implementation, and the same interface is what no-sandbox mode switches. Git commits and pushes are a Farik tool on the host, tiered `git_local` and `git_remote`. Credentials, for the model API and for git remotes alike, never enter a container.

## Consequences

The governor stays in one process with the event log, which keeps the hot-path evaluation local and the audit trail complete. No-sandbox mode becomes a one-line configuration difference rather than a second architecture, which makes the decision to ship it a product decision rather than an engineering one.

Farik owns the shell tool: output limits, timeouts, working directory, and environment are Farik's responsibility, and a coding agent that expects the SDK's Bash tool has to be told, in its role prompt, that `farik_exec` is its shell. Prompt tuning for the developer role has to account for this.

Process isolation covers commands only. A file write by the host-side Edit tool is stopped by the path check, not by a container boundary, so the path check is on the critical path for safety and is tested as such. If a future runtime wants full isolation, the `Executor` interface is the seam, and this record is revisited.

The `git_local` tier is enforced at the tool boundary and by the first-word check in `farik_exec`, not by the container. An agent with `execute` but without `git_local` (the Architect on a spike) can still commit inside the container by wrapping git in a shell script. That commit lands on the task branch, where the reviewer's diff and the path check see it, and it is the accepted residual of this decision. The `git_remote` tier has no such residual because the container holds no credentials.
