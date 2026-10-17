# 0067. The preview reaches the app's backend

Date: 2026-10-17
Status: accepted (the founder, 2026-10-17: the UI/UX Designer must actually run and user-test the project's app, backend included; the mechanism decided by the controller and the planner the same day)

Amends:
- **ADR 0026** (A6, and its amendment of 2026-09-30 that the browser reaches only the preview): the preview's `start` runs on the network, and the Designer's browser may also reach the owner's computer at `host.docker.internal`.
- **ADR 0054** is unchanged: the port is still found from the container's own `/proc/net/tcp` and `/proc/net/tcp6`, which list only the container's own sockets on any network.

Notes **ADR 0061** ("research is free; only secrets stay hidden"): the owner's environment file is such a secret, and stays hidden from every agent.

## Context

Since phase 6 step 12 the preview's `start` runs with `--network none`, gets no environment values, and the Designer's Playwright browser, in the preview's network namespace, is held by a dead proxy to `localhost`. A plain static site previews; an app with a backend does not. A typical case: a Next.js app that needs `NEXT_PUBLIC_SUPABASE_URL` and `NEXT_PUBLIC_SUPABASE_ANON_KEY` in the browser, `SUPABASE_SERVICE_ROLE` and `OPENAI_API_KEY` on its server, and a Supabase running on the owner's computer. In the preview today it cannot start, or starts and shows only errors, so the Designer cannot user-test it.

## Decision

1. **The owner's environment file**, `.catervas/local/preview.env`. The owner puts it there by hand; this change adds no upload. When it is a file, Catervas passes it with `--env-file <absolute path>` to the preview's `prepare` and `start` containers, and to no other container, session or process. Its format is Docker's env-file format, which Docker reads itself: one `NAME=value` per line, `#` comments, no quotes (a quote is part of the value), and a line without `=` passes Catervas's own variable of that name. The values never pass through Catervas's command line, so they are never in a `docker run` argument, a process list or a log of the arguments.
2. **No agent reads it.** `.catervas/local/**` is one of the protected paths every team has (`DEFAULT_PROTECTED_PATHS`, which `Team::rules` always includes): the hook refuses a read of it from a session in the project folder (`path_protected`), from a task's worktree (`path_outside_workspace`), and through the `project` read root of a project of several repositories (`path_protected`). Claude Code's own `Read` deny rule for the folder holding a session's working directory is deliberately absent (`read_root_rules` in `crates/runtime/src/claude.rs`), so the hook is the guard there. The sandbox mounts a task's worktree at `/workspace` and the other members at `/repos/<name>`, and `sandbox_for` leaves the project folder out of those (`crates/runtime/src/orchestrator.rs`), so no command an agent runs in Docker's sandbox can open the file. In no-sandbox mode a command runs on the computer and reaches what the owner can, `.env` as much as this file; the Designer never runs there, so no preview reads the file there either.
3. **It is never committed.** `.catervas/local/.gitignore` ignores everything under that folder (`crates/store/src/files.rs`).
4. **Catervas never shows its values.** The only text that leaves the preview's containers is a failure's last lines: `prepare`'s output, `start`'s `docker logs`, or Docker's own refusal (which quotes a malformed line of the file). Before such text becomes `preview.stopped`'s reason, the escalation's detail, a session's end or a log line, every value of the file of at least 4 characters is replaced with `[hidden]`, longest first. `team.get` answers only whether the file is there (`preview_env_present`), never its name list or contents.
5. **`start` runs on the network**: `--network bridge` and `--add-host host.docker.internal:host-gateway`, so the app can reach the internet and the services on the owner's computer at `host.docker.internal`. `prepare` already ran on `bridge`; it gains the same `--add-host`, so a build that reads an address on the owner's computer resolves it too.
6. **The Designer's browser may also reach the owner's computer.** Chromium's proxy bypass becomes `localhost,host.docker.internal` (comma-separated, as Playwright splits it), and `--allowed-origins` becomes the preview's origin plus `http://host.docker.internal:*` and `https://host.docker.internal:*` (semicolon-separated; the pinned server turns `scheme://host:*` into a pattern of every port). The page check (`catervas_check_page`) takes the same bypass. The browser and the check join the preview's network namespace (`--network container:<preview>`), and Docker gives such a container the preview's own `/etc/hosts`, mapping included; Docker refuses `--add-host` with that network mode ("conflicting options"), checked on 2026-10-17, so they take none.
7. **Unchanged:** who gets a preview and a browser, `prepare`'s cache key (the tree and the `prepare` text; editing the environment file does not prepare again, and `start` reads it each time), the 120 seconds, the port and the first page, the governor's check of every `url` the Designer gives the browser (the preview's origin alone: the Designer opens the app, and the app's own requests reach the backend), the dead proxy for every other host, and the browser's denied tools.

Rejected:
- Parsing the file as dotenv and passing each value with `-e NAME=value`: every value would sit in `docker run`'s arguments, readable in the computer's process list.
- Writing a normalised copy of the file for Docker: a second copy of the secrets on disk, for quotes the owner can leave out.
- Giving the browser the file with Playwright's `--secrets`, which hides the values in its answers: the founder decided the file goes to the preview's two containers alone, and `--secrets` also lets an agent type a value into a page by its name.
- Letting the Designer navigate to `host.docker.internal` itself: it opens the app to test it; other services on the owner's computer (a database's admin page) are not its work.
- Hiding values of every length: a value of 1 to 3 characters (`1`, `on`, a port's digits) is not a secret, and hiding it would turn every failure's text into `[hidden]`.

## Consequences

- An app with a backend previews: its server and its pages reach a Supabase or any other service on the owner's computer at `host.docker.internal`, and the internet, with the owner's keys.
- The code the team writes runs with those values, as the app does on the owner's own computer: a change that prints a key on a page, or sends it away, is code the Architect reviews like any other (ADR 0061). The owner should put development keys in the file, not production ones; the Settings words say so.
- A screenshot the Designer takes of a page that shows a value shows it; Catervas hides values only in the text it records.
- The preview's own network no longer stops the browser; the dead proxy, `--allowed-origins` and the governor's `url` check do: spec 8.3's four barriers become three. The browser's own network stack (WebRTC among it) is no longer stopped by an empty namespace; its tools stay those of browsing alone, and it runs no script the agent writes.
- On Linux, `host.docker.internal` is the Docker bridge's address: a service on the owner's computer that listens only on `127.0.0.1` is not reachable there, and must listen on all addresses (a Supabase started with `supabase start` does). Docker Desktop maps it for loopback services too.
- `docs/SPEC.md` 4.1, 5.6, 8.3 and 8.6 change in revision 0.120 (standalone plans `preview-backend`).
