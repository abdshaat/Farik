# Governor flexibility, plan 3: the web app

Status: ready
Branch: `fix/governor-flexibility-web`, from `fix/governor-flexibility` at the commit adding this plan, in its own worktree; merged into `fix/governor-flexibility` when done
Spec and decisions: see `README.md`; ADR 0079 decisions 2, 10, 14 (their words), and the web findings

All files under `apps/web/src/`. Tests are vitest beside the component (`pnpm --filter @catervas/web test`), with the helpers in `test/` (`renderApp`, `openedGate`). Strings go in `strings/en.ts` and are read with `t()`.

## Decisions

- Connect: `app/connection.tsx` listens to `hashchange`; a 64-hex hash arriving after mount is redeemed as on mount. `en.ts` `linkUsed` becomes "This start link was already used. In your project folder, run catervas open for a new one."
- Loading: while the connection status is `checking` or `connecting`, and while `Shell` has no `serve.status` answer, the page shows the logo and "Opening Catervas…" (`en.ts` `opening`) instead of nothing.
- Help page (`pages/HelpNeeded.tsx`): a choice whose target is not `cancelled` is disabled until the note has text, with the hint "Write what <name> should know" under the note (`helpNoteNeeded`). The note's label names the agent who raised the block: the `requested_by` of the newest `task.transitioned` into `blocked` in `task.history`, falling back to the assignee. A choice never sends its label as the message; cancelling with no note sends "Cancelled by the owner".
- Channel (`components/MessageText.tsx`): "(waiting on you)" follows only the first link to a waiting task in one message. A task id directly preceded by `/` or followed by `.` and a letter is plain text. The Chats header (`en.ts` key of "Where the team talks…") becomes "Where the team talks. A message to no one goes to <PM name>. To ask for work, use the request box on Today."
- Today: the goal card drops its "Waiting on you (n)" line (`GoalCard.tsx` `Doing`); Suggested connections render only when the Waiting on you section is empty.
- Team (`pages/Team.tsx`): when both a designer and a developer exist (as now), the cost sentence is "<designer> works with <model label>, so a day costs more." (`teamCostDesigner`), with the designer's own `effective.model.label`. The words "the same model" are removed.

## Tasks

### Task 15: a new link opens in the same tab, and the app never paints blank

Tests:
- `redeems_a_link_opened_in_the_same_tab` (`app/connection.test.tsx`) — mounted at `/connect` with no hash and a 401 session; setting `location.hash` to a 64-hex code and dispatching `hashchange` posts `/connect` with that code.
- `shows_opening_while_it_connects` (`app/connection.test.tsx` or `shell/Shell.test.tsx`) — before `serve.status` answers, the page shows "Opening Catervas…".

- [x] `fix(web): a start link opened in the same tab works, and the app says it is opening`

### Task 16: an answer must say something, to the agent who asked

Tests (`pages/HelpNeeded.test.tsx`):
- `wants_a_note_before_answering` — "Answer and carry on" and "Ask Mira to change the plan" are disabled with an empty note and enabled after typing; "Cancel the task" is enabled throughout; answering sends the typed note as `message`.
- `names_the_reviewer_who_raised_the_block` — a history whose last move into `blocked` was requested by `ada` while the assignee is `theo` labels the note "Add a note for Ada".

- [ ] `fix(web): an answer to a block must say something, to the agent who asked`

### Task 17: channel words

Tests (`components/MessageText.test.tsx`, created if absent):
- `marks_a_waiting_task_once` — "CTV-22 … CTV-22 … CTV-22" with CTV-22 waiting renders "(waiting on you)" once.
- `leaves_a_file_name_alone` — "plans/CTV-1.html" renders no link; "CTV-1 is done." renders one.
- `says_who_answers_an_unaddressed_message` (`pages/Channel.test.tsx`) — the header contains "A message to no one goes to Mira".

- [ ] `fix(web): the channel marks a waiting task once, leaves file names alone, and says who answers`

### Task 18: Today says each thing once

Tests:
- `does_not_repeat_what_waits_on_you` (`pages/GoalCard.test.tsx`) — a goal with `waitingOnOwner: 1` shows no "Waiting on you".
- `holds_suggestions_while_something_waits_on_you` (`pages/Today.test.tsx`) — with one waiting row, no "Suggested connections" heading; with none, the heading shows.

- [ ] `fix(web): Today says what waits on you once, and suggests connections when nothing does`

### Task 19: the Team page names the real model

Tests (`pages/team.test.tsx`):
- `names_the_designers_own_model` — a designer on "Opus 5.5" and a developer on "Sonnet 5.5" show "Ivo works with Opus 5.5, so a day costs more." and never "the same model".

- [ ] `fix(web): the Team page names the designer's own model`
