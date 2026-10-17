# Standalone: the preview reaches the app's backend

Status: ready
Branch: `feat/preview-backend` (work outside a phase, one pull request to `main`)
Decision: ADR 0067 (`docs/decisions/0067-the-preview-reaches-the-apps-backend.md`)
Depends on: `feat/parallel-agents` (ADRs 0061 to 0064, SPEC 0.118), from which this branch was cut, and
`feat/internal-task-ids` (ADR 0066, SPEC 0.119), which lands beneath this branch before execution
Readiness confirmed by: none (ADR 0065); the author held both plans to the three rules (every
decision made, no ambiguity, no forward dependency)

## Why

The founder wants the UI/UX Designer to run and user-test the project's app, backend included. Today
the preview's `start` runs with `--network none`, gets no environment values, and the Designer's
browser is held by a dead proxy to `localhost`, so an app that needs keys or a Supabase on the
owner's computer cannot work in the preview.

## Before the first task

1. Rebase this branch onto `feat/internal-task-ids` once it is there. ADR 0067, the line added to
   ADR 0026, and these plans are this branch's only commit. ADR 0066 changes branch naming, pull
   request titles and commit messages; nothing here touches those files.
2. `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree.
3. Run `cargo xtask check` once for a clean baseline, and `cargo xtask check --integration` if
   Docker and git are there (the Docker tests of `crates/runtime/tests/playwright_connector.rs`
   need the pinned Playwright image and `alpine:3.22`).

The plans name files and symbols as they are at `2006a978`. Where a line has moved, find the symbol
by name.

## Order

One executor (Sonnet 5.5) runs the plans in this order, one task at a time, each task one commit,
each task leaving `cargo xtask check` passing (it runs `pnpm check` too).

| Order | Plan | Tasks |
|---|---|---|
| 1 | `01-runtime.md` | 1 the browser reaches the owner's computer; 2 the preview runs on the network with the owner's file; 3 no agent reads the file and Catervas never shows its values |
| 2 | `02-settings-and-spec.md` | 4 Settings says whether the file is there; 5 `docs/SPEC.md` revision 0.120 |

Not in these plans: an upload of the file from the web app (the owner places it), the setup
wizard's card (Settings alone gets the line), and any change to who gets a preview or a browser.

Never edit this repository's `.catervas/`.
