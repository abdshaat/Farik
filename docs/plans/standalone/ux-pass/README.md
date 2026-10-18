# Standalone: a UX pass over the web app

Branch: `feat/ux-pass` (work outside a phase; its own pull request to `main`), cut from `fix/stray-changes` (SPEC 0.121).
Source: the note-chat2 journey of 2026-10-17 (`reports/note-chat2-journey-2026-10-17.html` and its 26 screenshots) and the founder's "improve the UI/UX"; the design and the reasons are in `docs/design/ux-pass-2026-10.md`.
Plans: written and held to the three readiness rules by the planner (Opus 5.5) with no readiness review (ADR 0065); executed by Sonnet 5.5 (ADR 0032), test-first, one test per major functionality (ADR 0060).

Beside it: `fix/open-issues` (ADR 0069, SPEC 0.122) fixes the functional issues and the stale words the journey found. These plans edit none of its strings: `jobDesigner` and the setup preview card, "Someone on the team reviewed it", the chat panel's line, the question page's headings and leads (`oneQuestionTitle`, `oneQuestionLead`, `questionsTitle`, `questionsLead`, `youAnswered`, `answered`, `laterQuestion`), the integration branch setting, `/chats`, the help page's choices, setup's spending figure. Whichever branch merges second rebases and resolves the top of `docs/SPEC.md` and `apps/web/src/strings/en.ts`.

One executor runs the plans in this order, one task at a time, each task one commit, each task leaving `pnpm check` and `cargo xtask check` passing. Before the first task: `export CARGO_BUILD_BUILD_DIR=$PWD/target` in the worktree. Every plan is web-only; no Rust, no wire change.

| Order | Plan | What it does |
|---|---|---|
| 1 | `01-agent-lines.md` | A waiting agent says only "Waiting on you" on Today's band and its Team card; band lines clamp to two lines |
| 2 | `02-waiting-groups.md` | "Waiting on you" in three groups (set up, answer, merge), the about line and a three-line clamp, merge folded after three; "What moved" before two suggested connections |
| 3 | `03-question-one-press.md` | A suggested answer sends with one press; the question in plain paragraphs; "About questions" behind an ⓘ |
| 4 | `04-task-page.md` | The task page leads with the agent's note, checks in one line, the try count from the second try, "Adding it" only when it applies, "Loading…" |
| 5 | `05-board.md` | Board cards in fewer lines (title clamp, id and epic on one line); Done shows what waits and the five most recent |
| 6 | `06-costs.md` | Costs: two-column table sorted by spend, chats as a row, the four numbers as tiles, notes behind ⓘ |
| 7 | `07-team-words.md` | The Team page and setup's team step in bullets and fewer words |
| 8 | `08-spec.md` | `docs/SPEC.md` revision 0.123, `web-ui.md`, `suggested-connections.md` (hard rule 8) |

SPEC is recorded once, by the last plan: each change is a few words in 4.2, 6.7, F3 and F17, and one revision keeps them together.

Checklist (ticked in the commits that do them):

- [x] Plan 1
- [x] Plan 2
- [x] Plan 3
- [x] Plan 4
- [x] Plan 5
- [x] Plan 6
- [x] Plan 7
- [ ] Plan 8
