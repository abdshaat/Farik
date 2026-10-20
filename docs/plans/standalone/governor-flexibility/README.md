# Standalone: the governor gets out of the way

Status: ready
Branch: `fix/governor-flexibility` (its own pull request to `main`); the web plan runs on `fix/governor-flexibility-web` in its own worktree and is merged into this branch before the record task
Decision: ADR 0079
Spec: `docs/SPEC.md` 5.2, 5.5, 5.7, 5.9, 5.14, 8.6 (revision 0.134)
Depends on: `origin/main` at `88b96c11`. Pull request #83 (ADR 0078, env templates, idle re-plan) is independent and open; the two merge in either order.
Readiness confirmed by: the planner (Opus 5.5), 2026-10-20, against the three readiness rules (ADR 0065)

## Goal

The founder watched a team stall for an hour on governor refusals and owner round trips (ADR 0079, Context). When done, the eleven owner round trips of that run do not happen: a task's branch sees `main` before each build session; an empty answer cannot be sent; a send-back to refining reaches the Product Manager with the owner's words; the Product Manager repoints a task whose dependency was cancelled; the session budget counts only work; a plan reaches `main` by its own pull request; a new task starts with the team's accepted documents; read-only git runs; the owner's channel message is answered and can answer a block; and a new browser gets in with `catervas open`. Out of scope: env templates and the idle builder's re-plan (ADR 0078, #83), a governor redesign.

## Plans, in order

| File | Tasks | Where |
|---|---|---|
| `01-governor.md` | 1 to 8 | this worktree |
| `02-integration-and-server.md` | 9 to 14 | this worktree, after 01 |
| `03-web.md` | 15 to 19 | worktree `fix/governor-flexibility-web`, in parallel with 01 and 02 |
| `04-record.md` | 20 | this worktree, after 02 and after 03 is merged in |

## Decisions shared by the plans

- Every product decision is in ADR 0079; the numbers below are its decision numbers.
- New gate wire name `dependency_cancelled` (decision 5): added to `$defs/gateId` in `docs/schemas/event.schema.json` and to the core gate enum. Adding an enum value keeps every existing log readable.
- No other new event kind or field, except `tasks.list` rows' `waits_on` and `costs.summary`'s `cancelled_today_usd` in `docs/schemas/rpc.schema.json` (both optional).
- Work sessions for the budget (decision 6) are the purposes `refine`, `explore`, `implement`, `verify`.
- Read-only git subcommands (decision 9): `status`, `diff`, `log`, `show`, `ls-files`, `grep`, `rev-parse`, `blame`.
- The merge before a build session (decision 1) uses the integration branch named by the task's repository (`IntegrationTarget`), after `fetch_fast_forward("origin", into)` when an `origin` exists.
- Every cargo command in these plans runs with `CARGO_BUILD_BUILD_DIR=$PWD/target` (shared build-dir collisions).

## Verification of the whole change

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```

Then the live run (the founder's project `~/catervas-services`, the driver built from this branch merged with #83), watched until one task of epic CTV-1 is accepted:
- no channel line contains `governor_escalation`;
- CTV-12 and CTV-14 move back to refining with a channel line naming CTV-11, and Mira repoints them;
- CTV-8's plan has a pull request;
- an unmentioned owner message gets Mira's reply.
