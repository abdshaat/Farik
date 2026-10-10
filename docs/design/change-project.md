# Changing the project

Status: approved by the founder on 2026-10-09 (brainstorm in a Claude Code session).

## Why

The founder tests and improves Farik by running its team on real repositories. Today one `farik serve` drives the project it started with until it stops; trying the team on another repository means stopping serve and starting it elsewhere. The founder asked for a way to point the team at a different local repository from the web app, after setup, with the team recreated there and no context carried from the old project, and for the web app to always show which repository the team works on.

## What the user sees

1. **The repository, always.** On a wide screen the rail's foot shows the project's folder name directly above "Connected"; the full path is its tooltip. The narrow top bar already shows the folder name.
2. **Change project.** Settings, "This computer", has a **Change project…** button under the project folder. It asks: "The team stops working on *old-repo*. Its work there is kept, and it is all there if you come back." Confirmed, the app goes to the wizard's project page.
3. **Choosing.** The project page says "Moving your team from *old-repo*" and offers **Stay on old-repo** beside the usual open and create. Opening or creating a folder runs the wizard's checks (inside home, a git repository, not home, not run by another Farik).
4. **A folder with a team.** If the chosen folder already has a Farik team, a second confirmation says "*target* already has a Farik team. It will be replaced by yours, starting fresh; your code is not touched." Confirmed, it is replaced.
5. **Arrival.** The app lands on Today in the new project, the team as it was and ready, the rail showing the new folder name. A notice under the paused banner's place says "Your agents' connections were copied from *old-repo* (N services). Use different keys for this project?" with **Keep them** (the notice goes) and **Choose different keys** (to the Team page, where each agent's page connects a service again; the notice goes). The notice stays until one is chosen, across reloads.

## The team in the new project

Same team, fresh. Carried from the old project: `team.yaml` with retired agents dropped and every other agent active (names, roles, personas, pictures, models, grants, connectors, pinned skills, policy, budgets), the uploaded pictures under `.farik/team/avatars/`, and the sandbox setting (`local/settings.json`). Connector keys are copied: the keychain (or `connectors.json`) keeps each per project on this machine (`connector:<project_id>:<agent_id>:<server>`, and the Procurement Specialist's mailbox as `mailbox:<project_id>:procurement`), so each kept agent's entry, an OAuth sign-in included, is loaded under the old project's id and saved under the new one's. The old project keeps its own. One consequence: a service that rotates its refresh token on use (some OAuth sign-ins) may ask the other project to sign in again after one of them refreshes.

Pinned skills' folders (`.farik/skills/`, `.farik/agents/<id>/skills/`) are copied, but each pinned skill asks to be confirmed again in the new project: a confirmation is recorded in the project's own log (ADR 0034), and a new folder is a new place to trust it.

Not carried: the criteria (the new repository keeps the checks its own `init` scan found; the founder, 2026-10-09), memory, the channel and its summary, one-to-one chats, tasks, contracts, sprints, decisions, the retro, sessions, costs, worktrees. The new project starts with its own `init` (a fresh event log and a scan of the new repository) and no setup marker, so no team setup screens.

## The folders

- **The old project** is left as it is: its `.farik/` keeps the team, the board and the memory. Choosing it again later (Change project, or **Stay on**) drives it as it was, with no reset and no copy.
- **A target with a team** has its whole `.farik/` deleted before `init`. Committed `.farik` files show as deleted in git; the user's own files are never touched.
- Old task branches (`feature/FRK-<n>`, `fix/FRK-<n>`, `docs/FRK-<n>`) are left untouched. A new task's number starts past the highest number any such branch in the repository holds, local or remote, as it already starts past every committed contract, so no new task's branch collides with an old one (the founder, 2026-10-09). This holds for every request filed, in any project.
- `state.json` remembers the new folder, so the next `farik serve` opens it.

## How it works

- A new RPC in drive mode, `project.leave`, stops the orchestrator the way `farik stop` does and tells serve the drive ended to switch, not to exit. Sessions already running end as on a stop; their tasks wait in the old project.
- serve's loop goes from `Mode::Drive(old)` to `Mode::Setup` carrying `old` as the project being left. The wizard's daemon listens on the same held port; the browser's session survives (`browser-sessions.json`), its socket reconnects, `serve.status` reports no project and the project being left, and the landing sends it to `/setup/project`.
- `project.open` and `project.create`, while a project is being left: the old root itself is opened as it is; a target with `.farik/team.yaml` is refused with `has_team` unless `replace: true`; otherwise the target's `.farik/` (if any) is removed, `init` runs, and the carried team, pictures and setting are written over the starter team, recorded as `team.updated`; the kept agents' connector keys and the mailbox are copied to the new project's id, and what was copied is written to `.farik/local/keys-copied.json` (`from`, and each `agent_id` and `server`), only when at least one was. No `setup-pending`, and the team is not paused. `serve.status` reports `keys_copied: { from, count }` while that file is there; `keys_copied.dismiss` removes it.
- A failure while taking on the new folder returns to the project page with the reason, as a failed take-on does today; the old project is untouched, so **Stay on** still works.

## Out of scope

A list of recent projects; driving two projects at once; moving memory, tasks or history between projects; changing project from the command line (stop serve and run it elsewhere, as today).

## Testing

Rust: the carried take-on (fresh target; target with a team refused then replaced; old root reopened unchanged; retired agents dropped; memory and board empty; no setup marker; the sandbox setting carried and the new repository's own criteria kept; connector keys and the mailbox copied to the new project's id and still kept under the old one's; the copied-keys file written only when a key was copied, reported by `serve.status`, removed by `keys_copied.dismiss`); `project.leave` ends the drive with a switch; serve goes drive → setup → drive; a request filed in a repository with an old `feature/FRK-<n>` branch is numbered past it. Web: the rail shows the folder above "Connected"; Change project confirms and calls `project.leave`; the project page's moving line, **Stay on**, the `has_team` confirmation that retries with `replace: true`; the copied-keys notice, Keep them and Choose different keys.

Docs: `docs/SPEC.md` 4.1 and 4.4; an ADR amending ADR 0021 (one process drives one project at a time, switchable from the browser, rather than one project per process).
