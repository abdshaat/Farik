# Standalone: four owner-facing fixes (who is thinking, the merge stall, the safe box's progress, honest finish words)

Status: ready
Branch: `feat/owner-polish` (work outside a phase, its own pull request to `main`), cut from `feat/ux-pass` (SPEC 0.123)
Spec: `docs/SPEC.md` 4.1 (setup), 4.2 (Today), 5.6 (`git_remote`), 5.14 (integration), F3 (board); recorded as revision 0.124 by Task 9
Depends on: `feat/ux-pass` at `483a8118`; ADR 0070 (committed with this plan)
Readiness confirmed by: the planner itself, 2026-10-18 (ADR 0065: every decision made, no ambiguity, no forward dependency)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder, 2026-10-18, four things. (1) The board does not say who is working on a card: every card an agent has a session on now shows "Mira · Product Manager — thinking" with a quiet moving mark. (2) After the owner merges finished work, its row on Today stays for up to a minute: Catervas checks a waiting merge every 15 seconds, and the row's own button checks it at once, with no detour through the task's page. (3) Preparing the safe box shows only a busy button for minutes: the build runs in the background, the row shows a progress bar, and Ready comes by itself. (4) Setup's last screen says finished work opens a pull request while its summary says "Send work to your online repository: No": the words now say what really happens. Out of scope: any other card or row, the lanes, how a pull request is opened, the Designer's browser row.

## Decisions

- **Item 4's premise, checked against the code.** `permissions.push` gives the Developer and the Designer the `git_remote` tier, their own `catervas_git_push` while they work (`crates/core/src/team.rs:1281`). It does not decide whether finished work becomes a pull request: Catervas itself pushes and opens it whenever the repository has an `origin` (`integrate.rs::open`, `has_remote("origin")`), "whatever the team file says" (SPEC 5.6, ADR 0061); with no `origin` the task waits for a merge by hand (`merged_by_hand`). So the contradiction is in the words, not the behaviour: "No" claims "Everything stays on this computer" though finished work goes out. Fix: the push question and its summary row speak of *work in progress*; the finish line speaks of *finished work* and depends on `origin`. Rejected: making a "No" stop the pull request (every ready-made team ships `push: false`, so a non-technical owner would get no pull request and have to merge with git by hand; it would also reverse ADR 0061's rule); a "turn pushing on" button (it would flip a lever that does not open pull requests).
- **Whether the project is online** is a new `team.get` field, `online: bool`: one repository, `deps.git.has_remote("origin")`; a project folder (`read_repos` with members), every member's repository (`member_root(home, path)`, `Git::open`) has `origin`; a git error counts as `false`. On `team.get` because setup's last screen and Settings both read it already. Rejected: `project.scan` (a whole scan for one fact, and Settings would rescan on every event).
- **Who is thinking** comes from `team.activity` as it is: an entry with `state: "working"`, a `session_id`, and a `task_id` is that agent thinking on that card. A task under way with no session running (`under_way`, no `purpose`) is not thinking and shows no line. The line names the *working* agent (the reviewer on a card in review), not the card's assignee, whose picture stays. `DOING` gains `plan: "markPlanning"`, which today shows no mark.
- **A chat fork no longer hides the work.** `at_work` (store) takes the newest open session of the agent that is not a `chat`; a `chat` session only when it is the only one open. Since ADR 0064 the fork runs beside the work session, and the newest-first rule made the card lose its line while the agent answers a chat. Today's band line then reads the work ("Building …") during a chat, which is at most 3 minutes. Rejected: two entries per agent (the band shows one line per agent).
- **The thinking mark**: three 6 px dots in `--catervas-color-status-working` (the working mark's own colour, one colour for the job), opacity 0.3 to 1, 1.2 s each, staggered by 0.2 s; `@media (prefers-reduced-motion: reduce)` stops them, full opacity. `aria-hidden`; the words carry the meaning for a screen reader.
- **The merge stall** is Catervas noticing a merge done on GitHub or by hand: rule 2 reads the forge once a tick, and an idle driver waits up to `RECHECK` (60 s). `MERGE_RECHECK` = 15 s caps both waits (`wait_until`, `wait_for_a_tick`) while any task awaits integration. 15 s is 240 `gh pr view` calls an hour per open pull request, far under GitHub's 5,000. Rejected: 5 s (rate), a webhook (no server to receive it).
- **The merge row checks in place.** Today's "Ready for you to merge" row of a task gets a `Button` "I merged it" that sends `task_integrate` where it stands, as the folder-change row already does; the row's title becomes a `Link` to `/tasks/<id>/accept` (the Gate page, where the owner went before and where "Add to project" stays), so the way to the task is kept. An answer with no events (`AwaitingForge`: the pull request is still open) shows "GitHub doesn’t show it merged yet. Merge it there, then press again." under the row; `not_merged` (no `origin`, branch not in) is worded "It isn’t merged yet. Merge it on this computer, then press again."; success with events leaves the row to the refetch (≤ 250 ms after the event, `useQuery`). Rejected: optimistic removal (events already reach the page live after each answer, `web.rs` `push`, and a reply with no events would need undoing); the Gate page's "Add to project" stays for owners who open the task.
- `recheck()` also keeps the 15 s cap for an accepted task whose integration was escalated (rule 2 skips it, so the tick costs nothing); `online` runs `git remote` on each `team.get`, a few milliseconds, accepted.
- **Pages that act and go back to Today** (Gate, HelpNeeded, PlanPage) need no change: Today mounts fresh and its first `waiting.list` is answered after the command; Questions and TaskDetail stay where they are and refetch on the event.
- **The safe box**: ADR 0070. Words: state `preparing` reads "Preparing"; with a step, "Step {done} of {of}. The first time takes a few minutes."; without, "Getting it ready. The first time takes a few minutes."; a failed build, `setupBuildFailed` with Docker's words behind an `InfoTip`. A native `<progress>` (`value`/`max` with a step, neither without), `aria-label` "Preparing the safe box", width 100% of the row's text, `accent-color: var(--catervas-color-status-working)`. The page asks again 1 s after each answer while `preparing`; "Check again" stays for the other rows.
- **Words** (en.ts; changed keys listed per task). Settings' "How finished work is added" uses the same two finish lines as setup; `rulesFinishPullRequest` is deleted.
- Spec revision 0.124 (Task 9), one header sentence and the sections named in Task 9.

## File map

```
crates/store/src/activity.rs                            modifies, tests (T1)  at_work skips a chat fork
apps/web/src/pages/Board.tsx, Board.module.css          modifies (T2)  the thinking line, DOING.plan
apps/web/src/pages/Board.test.tsx                       tests (T2)
crates/runtime/src/orchestrator.rs                      modifies, tests (T3)  MERGE_RECHECK
apps/web/src/app/command.ts                             modifies (T4)  onDone gets the reply
apps/web/src/pages/Today.tsx, Today.test.tsx            modifies, tests (T4)  "I merged it"
apps/web/src/app/refusals.ts                            modifies (T4)  not_merged
apps/web/e2e/accept.spec.ts                             tests (T4)
crates/runtime/src/computer.rs                          modifies (T5)  ImageBuild, Item's fields
crates/runtime/src/daemon.rs                            modifies (T5)  DaemonState holds the job
crates/runtime/src/daemon/web.rs                        modifies, tests (T5)
docs/schemas/rpc.schema.json                            modifies (T5 computerItem, T7 teamGetResult)
apps/web/src/pages/setup/SetupComputer.tsx, setup.module.css, SetupComputer.test.tsx   (T6)
crates/runtime/src/daemon/team.rs                       modifies, tests (T7)  online
apps/web/src/pages/setup/SetupFinish.tsx, SetupPermissions.tsx                         (T8)
apps/web/src/pages/TeamRules.tsx, settings/Work.tsx, AgentEdit.tsx                     (T8)
apps/web/src/pages/setup/team.test.tsx                  tests (T8)
apps/web/e2e/setup-team.spec.ts                         tests (T8)
apps/web/src/strings/en.ts                              modifies (T2, T4, T6, T8)
docs/SPEC.md, docs/design/web-ui.md                     modifies (T9)
```

## Interfaces

```rust
// crates/runtime/src/orchestrator.rs
const MERGE_RECHECK: chrono::Duration = chrono::Duration::seconds(15);
impl Orchestrator { fn recheck(&self) -> chrono::Duration; } // MERGE_RECHECK while any board row is a task with awaiting_integration, else RECHECK; a store error reads as RECHECK

// crates/runtime/src/computer.rs
pub enum ItemState { Ready, Missing, TooOld, NotRunning, Preparing }   // "preparing"
pub struct Item { pub state: ItemState, pub version: Option<String>,
    pub done: Option<u32>, pub of: Option<u32>, pub failed: Option<String> } // each skip_serializing_if none
#[derive(Default)] pub struct ImageBuild { now: Arc<Mutex<Building>> }
enum Building { #[default] Idle, Running { done: Option<u32>, of: Option<u32> }, Failed(String) }
impl ImageBuild {
    /// Starts `docker build` in the background, or joins the one running. Err: Docker is not installed.
    pub fn start(&self, env: &BTreeMap<String, String>) -> Result<(), String>;
    /// The check's sandbox_image item with the job laid over it (ADR 0070, 2).
    pub fn over(&self, checked: Item) -> Item;
}
fn build_step(line: &str) -> Option<(u32, u32)>; // "#6 [2/4] RUN …", "#6 [stage-1 2/4] …", "Step 2/4 : …"
// build_sandbox_image is removed; its body becomes the job's (same args, stdin and env, plus BUILDKIT_PROGRESS=plain)

// crates/runtime/src/daemon.rs
impl DaemonState { pub(crate) fn image_build(&self) -> &crate::computer::ImageBuild; }
// crates/runtime/src/daemon/team.rs
fn online(state: &DaemonState, deps: &ToolDeps) -> bool;
```
```ts
// apps/web/src/app/command.ts
export function useCommand(onDone: (done: { said: string; events: number[] }) => void, fill?: Record<string, string>);
```
```json
"computerItem": { "state": "ready|missing|too_old|not_running|preparing", "version": "…", "done": 0, "of": 0, "failed": "…" }
"teamGetResult": { "…": "…", "online": true }   // required
```

## Tasks

### Task 1: a chat fork leaves the agent's work line

Files: `crates/store/src/activity.rs`

Tests:

- `keeps_the_work_line_while_a_chat_fork_runs` (new) — an agent with an open `implement` session on CTV-1 and a later open `chat` session: its activity is `Working`, "Building <title>", `task_id` CTV-1, purpose `implement`; once the implement session ends, the chat's "Answering your chat" with no task. Fails first because the newest open session, the chat, wins. No existing test asserts a chat line beside a work session (none in `activity.rs`, none in `Today.test.tsx`), so none changes.

- [x] `fix(store): an agent's chat fork leaves its work line`

### Task 2: the board says who is thinking

Files: `Board.tsx`, `Board.module.css`, `Board.test.tsx`, `en.ts` (`boardThinking`: "{name} · {role} — thinking", `boardThinkingBare`: "{name} — thinking")
Consumes: Task 1

`row` adds, after the meta line and before the mark, when an activity entry has `state === "working"`, a `sessionId`, and `taskId === task.taskId`: `<span className={styles.thinking}>{t("boardThinking", { name, role: roleName(agent.role) })}<span className={styles.dots} aria-hidden="true"><span /><span /><span /></span></span>` (`Activity` type gains `sessionId?: string`; an agent the team does not hold: its id as the name, no role, `boardThinkingBare`). `DOING` gains `plan: "markPlanning"`.

Tests:

- `names_who_is_thinking_on_a_card` (new) — wide; activity: Mira `refine` on CTV-3 (session s-2), Kai `implement` on CTV-7 (as `ACTIVITY`), Theo `working` on CTV-4 with no session. "Gift cards" reads "Mira · Product Manager — thinking" and the mark `markWriting`; "Launch post" reads "Kai · Marketing Specialist — thinking"; "Gift card page" has no "thinking"; each `.dots` is `aria-hidden="true"`; `Board.module.css` read raw holds the dots' `animation` and `@media (prefers-reduced-motion: reduce) { .dots span { animation: none; …` (as `Shell.test.tsx`'s `keeps_the_connected_dot_still_under_reduced_motion`); rendered again with Mira's entry's purpose `plan`, "Gift cards" shows `markPlanning`; no axe violations. Fails first because no card names who works on it.
- `apps/web/e2e/board.spec.ts`: unchanged; re-run.

- [x] `feat(web): a board card says which agent is thinking on it`

### Task 3: a waiting merge is checked every 15 seconds

Files: `crates/runtime/src/orchestrator.rs`

Tests:

- `caps_a_wait_at_fifteen_seconds_while_a_merge_waits` (new, beside `caps_a_wait_at_a_minute_so_the_board_is_rechecked`, which stays) — `harness.accepted("CTV-1")`; `waited` an hour ahead: `Reached`, and the clock moved by 15 s. Fails first because the cap is 60 s.

- [x] `fix(runtime): check a waiting merge every fifteen seconds`

### Task 4: the merge row checks at once

Files: `command.ts`, `Today.tsx`, `Today.test.tsx`, `refusals.ts` (`not_merged: "refuseNotMerged"`), `en.ts` (`waitingMerged` "I merged it", `waitingNotMergedYet`, `refuseNotMerged`), `apps/web/e2e/accept.spec.ts`

`WaitingRow`: an `integration` row with a `taskId` renders its title (`strong`, same `id`) inside a `Link` to `/tasks/<taskId>/accept`, and the folder-change row's `Button` (`aria-describedby` its title) sending `{ command: "task_integrate", body: { taskId } }`; `useCommand((done) => setNotYet(done.events.length === 0))`; while `notYet`, `<p role="status" className={styles.muted}>{t("waitingNotMergedYet")}</p>`. The folder-change row keeps "Add".

Tests:

- `checks_a_merge_from_its_row` (new) — `waiting.list` one `integration` row for CTV-4: no "Add" link, and its title is a link to `/tasks/CTV-4/accept`; pressing "I merged it" sends `task_integrate` with `{ taskId: "CTV-4" }`; a reply `{ said, events: [] }` shows `waitingNotMergedYet`; pressed again, a refusal `not_merged: …` shows `refuseNotMerged` as an alert; pressed again, a reply with events, an event arrives and `waiting.list` answers empty: the row is gone and `waitingNone` shows; axe passes. `lists_what_waits_on_you` is updated where it reads the integration row's "Add" link. Fails first because the row is a link.
- `accept.spec.ts` — after `mergeByHand`, presses the button "I merged it" on Today (no Gate); `task.integrated` is recorded once and the "Add … to your project" row is gone.

- [x] `feat(web): a finished task's merge is checked from its row on Today`

### Task 5: the safe box is prepared in the background (ADR 0070)

Files: `computer.rs`, `daemon.rs`, `daemon/web.rs`, `rpc.schema.json` (`computerItem`)

`setup_call`'s `sandbox.build` arm: `state.image_build().start(&host.env())` then `{ "image": SANDBOX_IMAGE }`, a refusal `REFUSED`; `build_sandbox_image` has no other caller (`web.rs:36` and `:995` only). The production `setup_query` is renamed `wizard_query(state, host, name, params)`, so the test helper `setup_query` (`web.rs:3026`) keeps its name; its `computer.check` arm lays `state.image_build().over(..)` on `sandbox_image`. The job: `tokio::spawn`; stdout and stderr read line by line together (`tokio::io::BufReader::lines`); each `build_step` raises `done`; success sets `Idle`, failure `Failed(<last 20 stderr lines>)` in the old sentence's words.

Tests:

- `prepares_the_sandbox_image_in_the_background` (new, `web.rs`) — a scripted `docker` whose `build` records its args and stdin, prints `#6 [2/4] RUN true` to stderr, waits for a file the test makes, then exits 0, and whose `image inspect` succeeds only once that file is there: `sandbox.build` answers `{ image }` before the file exists; a second `sandbox.build` starts no second build (one args record); `computer.check` conforms and its `sandbox_image` is `{ state: "preparing", done: 2, of: 4 }`; the file made, the check (asked until it changes, 5 s at most) reads `ready`; a `build` that prints `boom` and exits 1 leaves `{ state: "missing", failed: "…boom…" }`. `answers_the_computer_check` keeps its args and stdin assertions, read once the check stops reading `preparing`. Fails first because `sandbox.build` answers only when the build ends.

- [x] `feat(runtime): prepare the sandbox image in the background and report its steps`

### Task 6: the safe box's progress bar

Files: `SetupComputer.tsx`, `setup.module.css` (`.progress`), `SetupComputer.test.tsx`, `en.ts` (`preparingWord` "Preparing", `imageStep`, `imagePreparing`, `imagePreparingLabel` "Preparing the safe box")
Consumes: Task 5's `preparing`, `done`, `of`, `failed`

`Item` gains `done?`, `of?`, `failed?`; `WORD.preparing = "preparingWord"`, tone `working`. `prepare()` calls `sandbox.build`, then `again()`; a refusal shows as today. While `sandboxImage.state === "preparing"`: no Prepare button; `<progress className={styles.progress} aria-label={t("imagePreparingLabel")} {...(of ? { value: done, max: of } : {})} />` and `imageStep`/`imagePreparing`; `useEffect` on `data`: when preparing, `setTimeout(again, 1000)`, cleared on change. With `failed`: `setupBuildFailed` as an alert, an `InfoTip` (`id="image-failed"`) holding `failed`, and Prepare again.

Tests:

- `shows_the_safe_box_being_prepared` (new) — the check answers the image `missing`; pressing Prepare sends `sandbox.build`; replied, the next check answers `preparing` 2 of 4: a progressbar named "Preparing the safe box" with `value` 2 and `max` 4, "Step 2 of 4" in the row, Continue disabled, no Prepare button; within 3 s the page asks `computer.check` again unprompted, answered `preparing` with no step: the bar has no `value` and `imagePreparing` shows; asked again, answered `ready`: the row reads Ready, Continue is enabled, and "Check again" was never pressed; then a fresh render answering `missing` with `failed: "boom"` shows `setupBuildFailed` and Prepare; axe passes at each. Fails first because the row knows no `preparing`.

- [x] `feat(web): setup shows the safe box being prepared and goes on by itself`

### Task 7: `team.get` says whether the project is online

Files: `daemon/team.rs`, `rpc.schema.json` (`teamGetResult.online`, required)

Tests:

- `team_get_says_whether_the_project_is_online` (new, beside `team_get_says_whether_sessions_run_in_the_sandbox`) — the harness's repository: `online` false; after `git remote add origin <a bare repository>` in it: true. Every `team.get` test conforms to the schema, which requires `online`. Fails first because there is no `online`.

- [x] `feat(runtime): team.get says whether the project is online`

### Task 8: the finish words say what happens

Files: `SetupFinish.tsx`, `SetupPermissions.tsx` (strings only), `TeamRules.tsx`, `settings/Work.tsx`, `AgentEdit.tsx` (strings only), `team.test.tsx`, `apps/web/e2e/setup-team.spec.ts`, `en.ts`
Consumes: Task 7's `online`

Words (en.ts):
- `finishLead`: "Finished work opens a pull request on GitHub for you to merge." `finishLeadLocal` (new): "Your project isn’t online, so finished work stays on this computer for you to merge yourself." `finishLeadSaved`: "{name}’s other answers are already chosen; change them if this project needs others." (appended after the lead, one space between).
- `finishPush`: "The Developer sends its work in progress online". `finishPushYes`: "It sends its own branches to GitHub while it works." `finishPushNo`: "It keeps its work in progress on this computer."
- `mayPush`: "May the Developer send its work in progress online?" `mayPushYes`: "Yes, to its own branches" (`mayPushYesNote` "Useful if you follow work on GitHub as it happens."). `mayPushNo`: "No, keep work in progress on this computer" (`mayPushNoNote` unchanged). `mayPushNoteOne`/`Many` gain a last line: "Either way, Catervas itself opens a pull request for finished work when your project is online."
- `tierGitRemote`: "Send its own branches to GitHub while it works". `rulesFinishPullRequest` deleted.

`SetupFinish` queries `team.get`; its lead is `finishLead` when `online`, `finishLeadLocal` when not, nothing until answered, then `finishLeadSaved` after it for a saved team. `Part` gains `online?: boolean`, set by `useRuleParts` from its `team.get`; `Finish(part)` shows the same line. The summary rows (`Carried`) keep their layout with the new words.

Tests:

- `finish_says_what_happens_to_finished_work` (new, `team.test.tsx`) — a ready-made team (as `starts_from_a_ready_made_team`), `team.get` answering `online: true`: the lead reads `finishLead` + `finishLeadSaved`, and the summary reads `finishPush`, "No.", `finishPushNo`, with no "stays on this computer" beside a pull request; `online: false`: `finishLeadLocal`. Settings' Work page with `online: false` reads `finishLeadLocal` in "How finished work is added". Fails first because the lead does not depend on `online`.
- `setup-team.spec.ts` — its project has no `origin`: line 73 expects `finishLeadLocal`, line 110 the same in Settings; the permission step's `^No, keep everything on this computer` label becomes `^No, keep work in progress on this computer`.

- [x] `fix(web): setup and Settings say what happens to finished work`

### Task 9: SPEC 0.124

Files: `docs/SPEC.md`, `docs/design/web-ui.md`

- Header, after 0.123: "Revision 0.124 (2026-10-18) records four owner-facing fixes (standalone plan `owner-polish`, ADR 0070): in F3 a card an agent has a session on names it, its role and that it is thinking; in 4.2 a finished task's merge row checks the merge with "I merged it", and in 5.14 Catervas checks a waiting merge every 15 seconds; in 4.1 the sandbox image is prepared in the background with its steps shown, and the last screen says finished work opens a pull request when the project is online and waits on the computer otherwise, the push question being about work in progress; `team.get` gains `online`; no new event or command."
- 4.1 (line 117): the background build, `preparing`, the bar, Ready by itself. 4.1 (line 123, step 8): the two finish lines by `online`. 4.2: the merge row's button. 5.6 (line 385): "The setup screen … say" sentence names `online` and that the push question concerns work in progress. 5.14 (line 579): "once a tick" plus "and an idle Catervas ticks at least every 15 seconds while a merge waits". F3: the thinking line. `web-ui.md`'s board and Today rows, one clause each.

- [x] `docs(spec): owner polish, revision 0.124`

## Verification

```
export CARGO_BUILD_BUILD_DIR=$PWD/target
cargo test -p catervas-store -- --include-ignored keeps_the_work_line_while_a_chat_fork_runs
cargo test -p catervas-runtime -- --include-ignored caps_a_wait_at prepares_the_sandbox_image answers_the_computer_check team_get_says
# expected: each passes
pnpm --filter @catervas/web test -- Board Today SetupComputer team settings
pnpm --filter @catervas/web e2e -- board.spec.ts accept.spec.ts setup-team.spec.ts
pnpm check            # expected: exit 0
cargo xtask check     # expected: xtask check: ok
```
