# Phase 7, step 14: Milestones 0 and 1 in the web UI

Status: draft; moved from phase 6 step 16 by the project plan's revision 26 (ADR 0029), on the founder's decision of 2026-10-01 that the runs test the fully equipped team once, on Claude. It was step 12 until revision 27 split step 01 and added a sign-in step. It is re-planned when the steps before it are planned, when this runbook is re-planned for the equipped team and reviewed for readiness again. Before the move: phase 6's steps 01 to 15 landed; renumbered from step 15 by revision 25 (ADR 0028); readiness finding B1 decided, the founder choosing the policy (ADR 0028), built in phase 6 step 15. The text below is phase 6's, unchanged.
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` section 11 (Milestones 0 and 1), F17; the flow it exercises is 4.1, 5.2 to 5.9, 5.14, and 5.16
Depends on: phase 6, merged (its step 15, sprints gather ready work, is what holds both requests in the Backlog until S1 opens); steps 01 to 13 of this phase, landed, the kit check (step 13) among them. A start gate applies: stage 1 does not begin until `cargo xtask check --integration` passes on the phase branch. The start-gate sha is the phase branch head when stage 1 begins, and it is recorded.
Readiness confirmed by: fresh-session reviewer, 2026-10-01, for phase 6's team, not ready → findings folded in; to be reviewed again when phase 7 is planned

This step writes no product code. It is a runbook, like phase 3's step 18 and phase 4's step 08, and it keeps their roles:
- **[A]** is the agent preparing and recording the run.
- **[F]** is the founder, or a test user. The agent stops, hands over, and waits.

The agent never acts in the human's name in the browser or on the command line. It does not file, answer, approve, accept, resolve, lock, post, pause or resume, or start or end a sprint. It does not run `farik init` (which records `criteria.updated` by `human`), and it does not run a command that starts sessions. It does not set, read, copy or move the credential.

## Goal

Two recorded runs that close Milestones 0 and 1 (spec 11).

1. **The team sprint** (phase 4 step 08's decisions, driven from the browser). It uses a fresh copy of Farik's own repository, seven agents (phase 4's six and the UI/UX Designer, the cap), and one sprint started by the founder in the browser. It carries two requests, a small one and a large one, through triage, plan checking, the Product Manager's questions and plans, the founder's approval, the breakdown, the building, the Architect's review, the Marketing Specialist's CHANGELOG entry, acceptance, and integration. Chats → Team carries reactions and meetings throughout, and the sprint ends with a review and a look back. The founder reads the gates, the diffs, the chats, and the log in the browser, and writes whether each task was done as its plan said.
2. **The thirty-minute test.** Five test users, at least three of them non-technical. Each starts at `farik serve` on a machine prepared with Farik, Claude Code, git, and Docker installed (the phase's "Ends with"). Each must reach an accepted task on their own repository within thirty minutes, with no help.

The records are `docs/milestones/m1-team-exit.md`, `docs/milestones/m1-web-exit.md`, and their event log exports.

## Decisions

- **The team sprint keeps phase 4 step 08's decisions**, except as below:
  - its team of six on the founder's models, plus the UI/UX Designer as the seventh agent, which is the cap (ADR 0026, approved 2026-09-30);
  - no dollar limit;
  - the founder's subscription token;
  - its loop limits and rules;
  - its two requests, word for word;
  - `auto_merge` into `test/m1-exit` in a bare clone.

  The changes:
  - **Driving and records.** Every human action is taken in the browser: the request box, questions, plan approval, sprint start, posts in Chats → Team, acceptance, and help. The founder's only terminal actions are `farik init`, `farik doctor` and `farik serve` (stage 1). The command line is otherwise used only by [A], for the record's readings (`farik metrics`, `farik log --json`, `farik channel`), after `farik serve` has stopped.
  - **Who checks plans.** Under the founder's rule of 2026-09-29, the Architect checks plans, because the team has one. Phase 4 had the Scrum Master judge. The pass criteria change to match.
  - **Models.** The six keep phase 4's models. Triage and ceremonies now run on `claude-sonnet-5-5` (spec 5.9), not `claude-sonnet-5`, and the record says so.
  - **The seventh agent**, the founder to confirm at readiness (the role's defaults):
    `- { id: ux, display_name: Iris, role: ui_ux_designer, status: active, persona: "Checks every screen the Developer builds, in the browser.", model: { id: claude-opus-5-5, effort: high }, mcp_servers: [{ name: playwright, source: builtin }] }`
  - **The Designer, with the preview of Farik with sample data** (the founder, 2026-10-01). `team.yaml` carries ADR 0026 D2's preview, so the Designer is `Ready`, not refused with `preview_not_set`. [A] writes this block at the end of `team.yaml` in stage 1:

        preview:
          prepare: "pnpm install --frozen-lockfile && pnpm -r --if-present generate && pnpm --filter @farik/web build && cargo build -p farik --features e2e --bin farik-e2e-serve"
          start: "target/debug/farik-e2e-serve --preview --port 4400 --pace 600 --transcripts triage_frk_1_small_by_pm,refine_writes_high_risk_frk_1,judge_frk_1_by_architect,plan_assigns_frk_1_to_theo,implement_finishes_frk_1,review_writes_note,implement_after_send_back_frk_1,review_writes_note"
          port: 4400
          path: /

    - `prepare` is D2's word for word. It runs in the sandbox image, whose crates layer (stage 1) lets cargo build offline. The pnpm in the image switches itself to the repository's pinned pnpm, with the network on.
    - `start` is D2's, plus the sample data. `--preview` serves Farik's web app on a recorded team of its own, Mira (Product Manager), Ada (Architect) and Theo (Developer), with no credential and the network off. `--transcripts` is the accept journey's list (`apps/web/e2e/accept.spec.ts`), so a request filed in the preview plays a recorded high-risk task through its plan gate, a send-back and the acceptance gate, and no AI session starts. `--pace 600` lets each state show. Its recorded team writes `plan_in_sprints: false` (step 15, B2), so the preview's journey does not stall at `ready`.
    - The first page is `/`, Today.
    - Caches: only the task's worktree is mounted, so no target or pnpm store can be shared between tasks. `target/` and `node_modules/` stay in the worktree (both ignored by git), so a task's later prepares are warm. Measured on 2026-10-01 at e71ec5f: a cold prepare 78 s, a warm one 16 s, both within the 15-minute limit.
    - What it shows: Farik's web app built from the task's tree, on sample data, not the run's own team. Neither request touches `ui_paths`, so no design review is expected. The Designer gets work only if planning gives it some, and the record says which happened.
  - **Order of actions: decided.** The founder chose the policy "Plan work in sprints" (ADR 0028, 2026-10-01), built in step 15. Under `farik serve` a ready task used to be assigned at once (readiness finding B1), and holding it by pausing both Developers is refused by the last-of-role rule. With the policy on in `team.yaml`, the team gets work ready at any time but assigns and builds nothing outside the open sprint, so both requests wait in the Backlog until S1 opens. The order in the browser:
    1. On Today, file request 1, then request 2, pasting `brief1.txt` and `brief2.txt`.
    2. Answer the Product Manager's questions.
    3. Open FRK-2's gate and check phase 4's three things (`allowed_paths` include `CHANGELOG.md`; the requirements or criteria name the CHANGELOG entry; they name the decision the Architect records). If all hold, approve the epic, and approve FRK-1 if it waits; if one is missing, that is an incident: re-run from stage 1 (phase 4 stage 3).
    4. Both wait in the Backlog: on the Board, FRK-1 is in the Backlog lane, and so is FRK-2, the epic, from its approval; Today's count reaches 2 once FRK-1 is ready ("2 pieces of work are ready and wait in the Backlog"). Nothing is assigned yet.
    5. Start the sprint, from Today's link or the Board, with "No limit".
    6. Planning takes both: S1's planning ceremony plans FRK-1 and FRK-2 with every task under it.

    Then the founder watches the work, acting on whatever Today lists (questions, help, escalations); mentions `@arch` once, in Chats → Team; before accepting the epic, checks on its first filter task's page that the contract carries the decision's `review` criterion (phase 4 stage 4.3), an incident if missing; accepts the epic with a note, and FRK-1's result too if it came out high risk; and reads the review and the "Looking back" meeting in Chats → Team. No pause or resume is needed. Any the founder takes are by `human`, and the record lists them.
  - **Chats.** Once, after the sprint's planning, the founder asks one agent one question in its one-to-one chat (Chats → the agent). The founder never presses "Send as a request": the run has two requests, word for word. The record lists the `chat` session and its reply. It is not a pass criterion.
  - **Templates.** Not exercised in the team sprint. The founder does not use "Use a saved team" or change any agent's model during the run, whatever the Team page suggests, since its suggestions are now `claude-opus-5-5` and `claude-sonnet-5-5`. "Save as a template" may be pressed after Task 1's record, and it writes only to `~/farik-m1/home/.config/farik/templates`. The thirty-minute test meets templates as new users do: "A saved team" is offered and cannot be chosen.
- **The thirty-minute test.**
  - **The five users.** The founder recruits them. At least three are non-technical, meaning they do not write code for work.
  - **The machine.** Each user's machine has `farik` (a release build of the phase branch head), Claude Code, git, and Docker installed and working. Each user brings a repository of their own, a git folder, or starts a new project in the wizard. Each uses their own Claude subscription or API key, or one the founder provides for the test.
  - **Before the clock:** Docker's sandbox image is built and the Designer's browser image pulled (the computer check shows both rows ready), so neither counts against the thirty minutes. `farik` is built with `pnpm -C apps/web build` then `cargo build --release -p farik`.
  - **Sprints.** A new team plans work in sprints (step 15, on by default), so a user's first request waits in the Backlog until they start a sprint, from Today's line or the Board. The clock keeps running. A user who stalls there is recorded with the time, as a finding for the fixes list.
  - **The Designer's preview** is the user's own to set, with no help. A first task held at "<Designer> needs to know how to open your app" is recorded with its time. It is a finding for the fixes list, not help to give.
  - **Start.** The clock starts when the user types `farik serve` in a terminal. That one command is given to them. It stops at the first `human.accepted` of a result, or at thirty minutes.
  - **The observer.** The founder, or someone the founder names, watches without helping. Every question the user asks, and every place they get stuck, is written down with its time.
  - **Pass.** At least four of the five reach an accepted task within thirty minutes, with no help given. At least two of those four are among the non-technical users. The milestone's criterion says "a new user with no help". The founder confirmed this bar on 2026-09-29 ("as long as it passes it's okay, we can refine later"); it may be raised before the first session, and the record says which bar was used.
  - **Consent.** Each user agrees in writing to the session being observed and to their repository's name appearing in the record. The record never includes their code or their key.
- **Records.** Both follow step 18's rules. The export is `farik log --json`, committed whole and never edited, with its line count, highest seq, and sha256. Screenshots of each gate the founder decided go into `docs/milestones/m1-team-exit/`. The team record says in one line that a first attempt was prepared on 2026-09-30 at 084fed4 and stopped; the founder's notes from it (`~/farik-m1-step11/notes.md`) led to ADR 0026. Its folder is kept, not counted.

## File map

```
docs/milestones/m1-team-exit.md (+ .events.jsonl, and a screenshot folder)   creates (Task 1, Task 2)
docs/milestones/m1-web-exit.md                                              creates (Task 3, Task 4)
docs/plans/phase-3-runtime/step-18-milestone-0-exit.md, docs/plans/phase-4-team/step-08-milestone-1-exit.md   modifies: closed by this run
```

## Runbook

### Stage 1: prerequisites [A]

This is phase 4 step 08's stage 1, in a fresh `~/farik-m1/`. The attempt of 2026-09-30 (084fed4, abandoned for ADR 0026) is moved aside whole first: `mv ~/farik-m1 ~/farik-m1-step11` (nothing deleted). Its stored credential stays there, and [A] never copies it. Every item is rebuilt at the start-gate sha:
- `cargo xtask check --integration` at the phase branch head (the start gate), its sha recorded;
- `pnpm -C apps/web build`, then `cargo build --release -p farik`, copied to `~/farik-m1/bin/farik` with its sha256;
- a new bare clone of `/home/ashaat/Farik` (it must hold the start-gate sha), `test/m1-exit` at that sha, the run's clone, and the local identity;
- the sandbox image rebuilt from the start-gate sha's `crates/runtime/sandbox` and `crates.Dockerfile`, with the two smoke tests (phase 6 changed the image since phase 4);
- `team.yaml`, with the seven agents (the Designer's line in Decisions), the preview block in Decisions, `policy.plan_in_sprints: true` (step 15), and `judgment` left at its defaults, so the Architect checks plans, with its sha256;
- the Designer's browser image pulled by its digest, `docker pull mcr.microsoft.com/playwright/mcp@sha256:77dccc5ce9e94cb8ae7ebea87ddbb6cd54b05760c4d63c54e16accf2726b8734` (`playwright.yaml`'s pin), with `docker image inspect`'s id recorded;
- the preview checked by hand, since no command runs a preview on its own: in a throwaway clone at the start-gate sha, [A] runs `prepare` and then `start` with `docker run` exactly as `DockerPreviewFactory` does (the sandbox image, the clone mounted at `/workspace`, `--user <uid>:<gid>`, `bridge` for `prepare` and `none` for `start`), fetches `http://localhost:4400/` from inside the preview container with `curl`, records prepare's time, and removes both containers;
- `brief1.txt` and `brief2.txt` copied from `~/farik-m1-step11` once `sha256sum` matches stage-1.md's (2fedf82e…, 37529479…);
- `env.sh` as before, with a fresh, empty `home/`.

[A] may self-check `farik serve --no-open` only before `farik init`, with no credential in the environment or in `home/`, and stops it with Ctrl-C.

[A] then hands over to [F], who types in a new terminal:

    source ~/farik-m1/env.sh
    export CLAUDE_CODE_OAUTH_TOKEN="$(cat /home/ashaat/.config/farik/claude-oauth-token)"   # the founder only, as step 18
    farik init      # exit 0, "kept the team already in .farik/team.yaml"; sha256sum -c ~/farik-m1/team.yaml.sha256 OK
    farik doctor    # exit 0
    farik serve     # opens the page; use the link it prints

`farik init` and `farik serve` are the founder's only terminal actions besides `farik doctor`. Every other human action is in the browser. [F] uses the link `farik serve` prints, not a remembered port: other `farik serve` processes from review worktrees have held ports 7420 and 7421.

### Stage 2: the team sprint [F]

In the browser, the founder follows the order of actions above. Wherever the run pauses on the founder, it is visible on Today. Today's "In the channel" and its "Open the channel" link lead to the same place as Chats → Team. Anything that goes wrong in the browser is an incident, recorded with its time and a screenshot. The founder decides whether to go on or to re-run from stage 1. At the end of the stage, [F] stops `farik serve` with Ctrl-C.

### Stage 3: the record [A]

This is phase 4 step 08's stage 5, plus the screenshots of each gate. [A] reads after [F] has stopped `farik serve` with Ctrl-C at the end of stage 2, so no command runs beside the daemon, with `env.sh` sourced. The pass criteria are phase 4 step 08's, with these changes:
- `contract.judged` is by `arch`, not `sm`;
- every human action after `farik init`'s `criteria.updated` appears in the log as `human`, taken from the browser.

- [ ] Task 1 [A]: `docs(docs): record the milestone 1 team exit run in the web UI`

### Stage 4: the founder's review [F]

- [ ] Task 2 [F]: `docs(docs): add the founder's milestone 1 team review`

### Stage 5: the five sessions [F]

For each user:
1. prepare the machine, including what Decisions puts before the clock;
2. hand over the one command;
3. observe for thirty minutes;
4. write the notes the same day.

### Stage 6: the record [A]

[A] writes the record for each user: the start and stop times, whether an accepted task was reached, the questions and stuck points with their times, and the screens where time went. It adds a summary against the pass bar and the fixes the sessions suggest. The fixes are listed, not made: they go in a later change.

- [ ] Task 3 [A]: `docs(docs): record the milestone 1 thirty-minute test`
- [ ] Task 4 [F]: `docs(docs): add the founder's milestone 1 verdict`

## Pass criteria

- **Team sprint:** phase 4 step 08's criteria, with the two changes in stage 3. Phase 4's "On failure" items still apply: the CHANGELOG task may stall in judgment, and planning may take only one request. The second is the expected failure if both requests are not held until S1 opens; step 15's policy, on in `team.yaml`, is the guard, and the record shows both in `sprint.planned`.
- **Thirty-minute test:** the pass bar in Decisions.
