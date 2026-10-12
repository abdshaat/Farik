# 0054. Setup asks less: the preview finds its own port and start, and the permissions start at the recommended answers

Date: 2026-10-10

Status: accepted (the founder, 2026-10-10, approving the design of `docs/plans/standalone/setup-friction.md` from a brainstorm in a Claude Code session). Amends ADR 0026 (the preview is two commands the user sets) and SPEC 4.1's "asked to confirm two things explicitly".

## Context

The founder, 2026-10-10: "make the UI/UX developer setup automatic rather than asking the user for a port to reduce the friction", and of "What they may do": "reduce the number of text in that page and do not say things like (Nothing runs until you answer) this is not good UX". Asked how automatic, the founder chose "Find + detect": Catervas reads the start command from the project and finds the port itself. Asked whether the two permission questions start answered, the founder chose "Pre-select recommended".

Catervas is for non-technical users (ADR 0016). A port is a number most of them have never seen, and today the UI/UX Designer waits until one is typed. The preview's container has its own network namespace with only loopback and no root, so whatever listens there at or above 1024 is the app.

## Decision

- **The preview's `port` is optional.** Left out, Catervas finds it: once `start` runs, it reads the container's `/proc/net/tcp` and `/proc/net/tcp6` once a second, takes the listening ports at or above 1024 in ascending order, and uses the first that answers `http://localhost:<port><path>`, within the same 120 seconds. A port that is set is used as before. The port found is the preview's origin for the browser and the governor's URL rule, and is the port `preview.started` records.
- **A browsing session is told where the preview is.** Once the preview answers, Catervas adds "The app's preview is at http://localhost:<port><path>." to the session's first message. The design review's message no longer carries the address itself, which it built before the preview started.
- **The scan suggests how to start the app.** For a project whose toolchain is a JavaScript package manager (pnpm, bun, yarn, npm), the first of the root `package.json`'s `dev`, `start` and `preview` scripts becomes `start` as `<manager> run <script>`, and `prepare` becomes `<manager> install`. Nothing is run and nothing is guessed from a name; other projects get no suggestion. `team.propose` carries it as the proposed team's `preview` when the team has none.
- **The two permission questions start at the recommended answers**, which are `settings.defaults`' (run commands: yes; push: no). The user may change either; the page no longer says that nothing runs until they answer.
- Rejected: guessing the port from the framework (Vite 5173, Next 3000), wrong whenever a project configures its own; a port the user must still type; a probe of every port from 1024 to 65535 (slow, and `/proc/net` already lists the few that listen).

## Consequences

- An app that listens on several ports gets the lowest that answers its first page; a user who wants another sets the port in Settings.
- An app with no `package.json` script, or on another toolchain, still asks for its start command; the port stays optional for it too.
- `docs/schemas/team.schema.json` loses `port` from the preview's `required`; a team file with a port is unchanged.
- `docs/SPEC.md` 4.1 and 8.3 change in revision 0.106 (0.87 when written; renumbered on 2026-10-12, when `main` had reached 0.103).
