# 0051. One process drives one project at a time, and can switch it from the browser

Date: 2026-10-09
Status: accepted (the founder, 2026-10-09, approving `docs/design/change-project.md` from a brainstorm in a Claude Code session). Amends ADR 0021 decision 2, "One process serves one project". Numbered 0051 because ADR 0050 is claimed by the rename branch (`rebrand/catervas`).

## Context

The founder tests and improves Farik by running its team on real repositories. Under ADR 0021 one `farik serve` drives the project it started with until it stops, so trying the team on another repository meant stopping serve and starting it elsewhere. The founder asked for a way to point the team at a different local repository from the web app, after setup, with the team recreated there and no context carried from the old project, and for the web app to always show which repository the team works on.

## Decision

- **One process still drives one project at a time, but it can switch.** Settings has "Change project…". It calls `project.leave`, which stops the orchestrator as `farik stop` does and tells serve the drive ended to switch, not to exit. Serve goes from drive to setup, carrying the project being left, on the same held port; the browser's session survives; the page lands on the wizard's project page, which offers "Stay on" the old project beside open and create.
- **The take-on carries the team, fresh.** `team.yaml` with retired agents dropped and the rest active, pictures, pinned skills' folders (each confirmed again), the sandbox setting, and each kept agent's connector keys and the Procurement mailbox, copied under the new project's id. Not carried: the criteria (the new repository keeps its own `init` scan's), memory, the channel, chats, tasks, contracts, sprints, the retro, sessions, costs. No setup screens, no setup marker, the team not paused.
- **A target with a team is refused** with `has_team` unless `replace: true`, then its `.farik/` is deleted before `init`; the user's own files are never touched. The old project is left as it is.
- **Copied keys are announced** by `.farik/local/keys-copied.json`, reported by `serve.status` as `keys_copied` and removed by `keys_copied.dismiss`; the user keeps them or chooses different ones.
- **A new task's number starts past every old task branch**, `feature|fix|docs/FRK-<n>`, local or remote, as it already starts past every committed contract, so no new task's branch collides with an old one.
- Rejected: a second `farik serve` per project (two ports, two links; the design wants a single tab). Rejected: a list of recent projects, driving two projects at once, moving memory or history between projects, and changing project from the command line (stop serve and run it elsewhere, as before); all out of scope.

## Consequences

- The project page has two states, a first choice and a move, and serve has a path from drive back to setup that the wizard's daemon shares.
- Connector keys are now shared between two projects on one machine. A service that rotates its refresh token on use may ask the other project to sign in again after one refreshes.
- Replacing a team deletes that folder's `.farik/`; committed `.farik` files show as deleted in git. The confirmation says so in words.
- Task numbering reads branch names in the repository, for every request filed in any project.
- `docs/SPEC.md` 4.1, 4.4, 8.1 and 8.4 change in revision 0.80.
