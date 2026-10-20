# Standalone: the owner is asked less and waits less

Status: ready
Branch: `fix/ux-after-the-studio` (its own pull request to `main`); tasks 1, 2 and 7 on `fix/ux-after-the-studio-b` in a second worktree, merged in before task 9
Decision: ADR 0081
Spec: `docs/SPEC.md` 5.5, 5.9, 5.16, 8.3, 8.6 (revision 0.136; 0.135 is taken by #85 and #86)
Depends on: `origin/main` at `d581bb6b`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-20 (ADR 0065)

Signatures, not bodies; test names and what each asserts (ADR 0008). Paths under `crates/` unless they start with `apps/` or `docs/`. Every cargo command runs with `CARGO_BUILD_BUILD_DIR=<worktree>/target`.

## Goal

Ten of the video studio's eleven tasks finished with the owner answering things the product could have settled (ADR 0081, Context). When done: pages open at once; the owner lists system tools once in Settings; a missing preview is a link, not a question; agents use the current team's names; a re-plan runs only when something changed; a send-back costs no session; a sessions escalation shows sessions; preview errors carry no npm notices. Out of scope: the governor's own rules (#86), the shipped image's packages (#85).

## Decisions

All product decisions are ADR 0081's; these fix the rest.
- `subscribe` params: `from_seq` optional; result `{ head: integer ≥ 0 }` for every subscribe (`subscribeResult` in `docs/schemas/rpc.schema.json`). Head is the newest seq (`EventQuery { newest_first: true, limit: Some(1) }`), 0 for an empty log or setup mode. `push` reads with `limit: Some(200)`.
- Client: the first subscribe of a connection sends `{}`; a reconnect sends `{ from_seq: lastSeq }`; `lastSeq = max(lastSeq, head)`.
- `events.since` gains optional `newest_first: bool` (default false); `pages/Events.tsx` loads `events.since { newest_first: true, limit: 100 }` and appends streamed events.
- `team.schema.json` `$defs.sandbox = { packages: string[] }`, pattern `^[a-z0-9][a-z0-9+.-]*$`, `maxItems: 40`, `uniqueItems: true`; `Team::sandbox_packages(&self) -> Vec<String>` (sorted).
- Package image tag: `format!("{SANDBOX_IMAGE}-p{}", hex(&sha256(packages.join("\n"))[..6]))`. Built by `docker build -t <tag> -` with a Dockerfile on stdin: `FROM <SANDBOX_IMAGE>\nUSER root\nRUN apt-get update && apt-get install -y --no-install-recommends <packages> && rm -rf /var/lib/apt/lists/* && find /tmp -mindepth 1 -delete`. A build failure is `SandboxError::Unavailable("the sandbox's system tools could not be installed: <last 8 lines>")`.
- `SandboxFactory::create` and `create_checking` gain `packages: &[String]`; `Held` keeps the packages it was made with; `held_sandbox` recreates on a difference. The preview factory takes the same list.
- The roster line per agent: `- <display name>, <role label>` for every non-retired agent, the session's own agent marked "(you)".
- Channel lines: retire `"<name> has left the team."`; replace `"<new> replaces <old> as <role label>."`.
- Re-plan gate: `fn replanned_since_change(history_of_epic_and_children, team_changes_seq) -> bool`; a durable read of the log, no new event kind.
- Send-back: in `budget_state`, a `refine` session whose start is the first `session.started { purpose: refine }` after a `task.transitioned { to: refining, actor: human }` is subtracted, as credential-refused sessions are; `budget` skips a `refining` row while `sent_back_unheard` (made `pub(super)`).
- `task.tries` result gains `sessions` and `max_sessions` (integers); `HelpNeeded` shows `helpSessions` "Sessions {used} of {allowed}" for a `sessions` or `budget` escalation.
- `error_lines` fallback drops lines whose trimmed text starts with `npm notice` or `npm warn`.

## Tasks

### Task 1 (worktree b): a page subscribes from now

Files: `docs/schemas/rpc.schema.json`, `runtime/src/daemon/web.rs`, `packages/protocol-client/src/client.ts` (+test), `apps/web/src/app/connection.tsx` (+test), `apps/web/src/pages/Events.tsx` (+ `pages.test.tsx`)
- `subscribes_from_the_head_without_a_sequence` (web.rs, ignored) — a log of 5 events; `subscribe {}` answers `{head: 5}` and streams only an event appended afterwards; `subscribe {from_seq: 3}` streams 4 and 5.
- `sends_a_long_backlog_in_rounds` (web.rs, ignored) — 450 events and `from_seq: 0`: a query sent right after is answered before the 450th event frame.
- `subscribes_from_now_and_resumes_where_it_was` (connection.test.tsx) — the first subscribe frame has no `from_seq`; after a reconnect it carries the last seq seen.
- `lists_the_newest_events_from_the_log` (pages.test.tsx) — the events page queries `events.since` with `newest_first: true` and shows its rows.
- [ ] `fix(runtime): a page subscribes from now, and a backlog is sent in rounds`

### Task 2 (worktree b): the owner gives the sandbox system packages

Files: `docs/schemas/team.schema.json`, `core/src/team.rs`, `runtime/src/sandbox.rs`, `runtime/src/sandbox/docker.rs`, `runtime/src/sandbox/host.rs`, `runtime/src/preview/docker.rs`, `runtime/src/orchestrator.rs`, the fakes in `runtime/src/orchestrator/fixtures.rs` and `runtime/tests/new_tests.rs`, `apps/web/src/pages/setup/TeamSetup.tsx` (type), `apps/web/src/pages/settings/` "How work runs" page (+test), `apps/web/src/strings/en.ts`
- `validates_the_sandbox_packages` (team.rs) — `ffmpeg`, `libnss3` pass; `rm -rf`, `A`, 41 items, a duplicate are refused at their paths.
- `names_an_image_for_the_packages` (docker.rs) — the tag for `[ffmpeg, libnss3]` is `<SANDBOX_IMAGE>-p<12 hex>`, the same for `[libnss3, ffmpeg]`, none for `[]`; the build's Dockerfile text installs both with `--no-install-recommends`.
- `recreates_a_sandbox_whose_packages_changed` (orchestrator.rs, ignored, `CountingSandboxFactory`) — a second session after the team's packages changed creates a new sandbox with the new list; with no change it reuses the held one.
- `edits_the_sandbox_tools` (web settings test) — typing `ffmpeg` and `libnss3` on two lines and saving sends `team.save` with `sandbox.packages: ["ffmpeg", "libnss3"]`.
- [ ] `feat(runtime): the owner gives the sandbox system packages`

### Task 2b (worktree b, after Task 2): an agent installs the system packages it needs

Files: `runtime/src/tools.rs` (registration, the `execute` tier), `runtime/src/tools/sandbox.rs` (creates), the orchestrator's sandbox map (a `recreate(task_id, team)` the tool calls), `runtime/src/channel.rs` use, the roles' `system.md` of software_developer, ui_ux_designer and architect (one line: "A missing system tool: install it with `catervas_install_system_packages`; never block on one.")
- Input `{ packages: [string] (1..=10), why: string (1..=300) }`; refused with `ToolError::InvalidInput` for a name outside the pattern or a total over 40; packages already listed are skipped.
- `installs_a_system_package_for_the_task` (tools/sandbox.rs, with `CountingSandboxFactory`) — Theo's implement session calls it with `[ffmpeg]` and a why: the team file lists `ffmpeg`, `team.updated` has `updated_by: theo`, the channel has "Theo added ffmpeg to the sandbox:", and the task's sandbox was created again with `[ffmpeg]`. A Product Manager session gets no such tool; `["rm -rf /"]` is refused.
- [ ] `feat(runtime): an agent installs the system packages it needs`

### Task 3: a missing preview is a link, not a question

Files: `core/src/governor/gates.rs` (string + its test), `docs/schemas/team.schema.json` (description), `runtime/src/daemon/gates.rs` (`design_reviews_waiting` or a sibling), Today test
- `lists_a_designer_task_waiting_for_a_preview` (daemon/gates.rs) — a team with a Designer, no preview and a ready Designer task: `waiting.list` has a `preview_missing` row; with a preview set, none.
- The refusal text says "set it in Settings, Your app" (updated assertion in `refuses_to_assign_a_designer_without_its_browser`).
- [x] `fix(runtime): a missing preview waits on the owner as a link to Settings`

### Task 4: every session knows the current team

Files: `runtime/src/prompt.rs`, `runtime/src/orchestrator/session.rs` (input), `runtime/src/orchestrator/human.rs` (status_effects, replace), `runtime/src/daemon/team.rs`
- `names_the_current_team` (prompt.rs) — the prompt of a session on a team of Mira, Ada, Ivo (active) and Iris (retired) has a "Your team" section naming Mira, Ada and Ivo with their roles, not Iris, and the sentence about others having left.
- `says_in_the_channel_who_left_and_who_replaced_them` (human.rs) — retiring Iris posts "Iris has left the team."; replacing Iris with Ivo posts "Ivo replaces Iris as UI/UX Designer."
- [ ] `feat(runtime): every session knows the current team, and the channel says who left`

### Task 5: a re-plan runs once per change

Files: `runtime/src/orchestrator/requests.rs` (`replan_for_idle`), `runtime/src/orchestrator/work.rs` (drop `replans`, `first_replan`)
- `re_plans_once_until_something_changes` (requests.rs, ignored) — two idle-builder ticks in a row run one re-plan session; a `contract.written` on a child then allows a second; a change in which builders are idle alone does not.
- [ ] `fix(runtime): a re-plan runs once per change of the epic or the team`

### Task 6: an owner's send-back does not cost the task a session

Files: `runtime/src/cost.rs`, `runtime/src/orchestrator/rules.rs` (`budget`), `runtime/src/orchestrator/requests.rs` (`sent_back_unheard` visibility)
- `does_not_count_the_refine_after_a_send_back` (cost.rs) — a task at 5 of 5 sessions, sent back to refining by the human, whose refine session then runs, reports 5 of 5, not 6.
- `lets_a_sent_back_task_be_refined` (rules.rs, ignored) — a task at N of N moved to refining by the human is not escalated by the budget rule before its refine session.
- [ ] `fix(runtime): an owner's send-back does not cost the task a session`

### Task 7 (worktree b): a sessions escalation shows sessions

Files: `docs/schemas/rpc.schema.json` (`task.tries` result), `runtime/src/daemon/gates.rs` (`task.tries`), `apps/web/src/pages/HelpNeeded.tsx` (+test), `apps/web/src/strings/en.ts`
- `counts_the_sessions_of_a_task` (daemon/gates.rs) — `task.tries` of a task with 3 work sessions, a budget of 6 and one room grant of 6 answers `sessions: 3, max_sessions: 12`.
- `shows_the_sessions_of_a_sessions_escalation` (HelpNeeded.test.tsx) — a `sessions` escalation shows "Sessions 6 of 6".
- [ ] `feat(web): a sessions escalation shows the sessions`

### Task 8: a preview error carries no npm notices

Files: `runtime/src/preview.rs`
- `drops_npm_notices_from_a_preview_error` — a tail with no error line and four `npm notice` lines after the Remotion message shows the Remotion lines and no `npm notice`.
- [ ] `fix(runtime): a preview error carries no npm notices`

### Task 9: the spec says it

Files: `docs/SPEC.md` (5.5, 5.9, 5.16, 8.3, 8.6, the rpc and team file formats, revision 0.136)
- [ ] `docs(spec): the owner is asked less and waits less, revision 0.136 (ADR 0081)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```
Live: the founder's project opens Today in under 3 seconds on its 3000-event log; with `sandbox.packages: [ffmpeg]` a new task container has `/usr/bin/ffprobe`.
