# Phase 3, step 18: Milestone 0 exit

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` section 11 (Milestone 0), F17; the flow it exercises is 5.2, 5.4, 5.5, 5.7, 5.11, 5.14, 5.16
Depends on: steps 01 to 17 of this phase, a start gate: stage 1 does not begin until step 17's last commit is on this branch and `cargo xtask check --integration` passes on it; ADR 0015 (no dollar limit ships, an unpriced model is recorded at no cost)
Readiness confirmed by: fresh-session reviewer, 2026-09-23 (two rounds; findings folded in)

This step writes no product code and has no interfaces. It is a runbook: stages run in order, and each says who runs it. **[A]** is the agent executing this plan. **[F]** is the founder, and only the founder: the agent stops, hands over with the exact command to type, and waits. The agent never answers a question, approves, accepts, resolves, locks, files a request, runs `farik init` or a command that starts sessions, sets or reads the credential, or writes the review, in the founder's name or otherwise: each of those is recorded in the log as the human.

## Goal

Milestone 0's exit test, run once for real and recorded. On a copy of Farik's own repository, with real Claude Code sessions, a team of a Product Manager and two Developers takes three requests from the founder (two large, one small) through triage, questions, contracts, approval, breakdown, implementation, verification, acceptance, and integration; one epic is co-written and locked by the founder; the harness metrics are printed; and the founder, reading the diffs and the event log, writes whether each task was done as contracted. The record is `docs/milestones/m0-exit.md` with the log export `docs/milestones/m0-exit.events.jsonl`. Out of scope: merging the team's three improvements into Farik (a separate change after the phase merges), any threshold on a metric (section 11 names none; this run is the baseline), and anything with a UI.

## Decisions

- Where: a fresh clone under `~/farik-m0/`, never the founder's working copy. `~/farik-m0/origin.git` is a bare clone of `/home/ashaat/Farik`, and `~/farik-m0/farik` is cloned from it, so the run's `origin` is that bare repository. Rejected: a branch of the GitHub repository, which a push would reach (and rule 11 would then want a pull request for); a clone with no remote, which would never exercise ADR 0012's push.
- Policy: `auto_merge` into `test/m0-exit`, a branch created in the bare repository at the start gate's commit. It is the policy a new project gets (ADR 0012), so the exit tests what users will run; it pushes, but only to the bare repository, so `main`, the phase branch, and GitHub are never touched. Rejected: `manual`, which adds a founder step per task and holds each epic's run until the founder merges (ADR 0013) without testing the default path.
- The binary: `farik` built in release mode at the start gate's commit; that sha, `claude --version`, the image id, the models, and `team.yaml` (text and sha256) go in the record. A fix to Farik means a new binary and a fresh run (below).
- No dollar limit (the founder, 2026-09-23; ADR 0015): the team sets neither `daily_usd` nor `max_task_budget_usd`. Costs are recorded and reported, not capped. Each contract still carries `max_cost_usd` as its estimate, which the Definition of Ready and an epic's child arithmetic measure (a child fits within the epic's `max_cost_usd` less the epic's own spend and its siblings' budgets), so the briefs state generous ones.
- Models, each at effort `high`, written explicitly: `pm` and `dev-a` on `claude-opus-5-5`, the newest Opus the table prices after step 17, for the contract writing and the first implementation the milestone judges; `dev-b` on `claude-opus-5`, so that a task `dev-a` implements is verified by a different model from the one that wrote it, and the run exercises agents on different models (ADR 0015). Triage runs on `TRIAGE_MODEL` (`claude-sonnet-5`, step 14). All four are priced, so `farik run` prints no unpriced-model warning and `farik doctor` has no such finding.
- Loop bounds, generous but kept (ADR 0015 keeps them): session `max_input_tokens` 2,000,000, `max_output_tokens` 128,000, `max_wall_clock_seconds` 3600 (a cold Rust build in a new worktree takes minutes), `max_tool_calls` 400 (the turn limit is this plus one, step 08); `blocked_limit_hours: 1`, because a blocked task has no command-line way out but its escalation (`farik resolve` acts on `escalated` only). Sessions per task are the contract's `max_sessions`, which the briefs set: 10 for a task, 15 for an epic. A triage session names its request as its task (step 14), so it counts: a clean standalone task takes six (triage, refine, plan, implement, verify, accept), and an epic about eight of its own (triage, refines, breakdown, close-out, the acceptance) besides its tasks'.
- Credential: `ANTHROPIC_API_KEY`, so that the recorded dollars are what is billed and no subscription rate limit ends a session for a reason outside the harness. The founder may use `CLAUDE_CODE_OAUTH_TOKEN` instead if they have no key; the record says which `farik run` named. It is exported in the founder's terminal only, never written to a file, and never seen by the agent.
- A dedicated `HOME` (`~/farik-m0/home`) for every `farik` command, so Claude Code's state from the run (`~/.claude.json`, its transcripts) stays out of the founder's own. Sessions load no user or project settings (`--setting-sources ""`, step 08), so Farik's `CLAUDE.md` does not reach the agents: what they know of the repository comes from the prompt Farik assembles and the files they read. The clone's git identity is set in its local config (`Farik M0`, `m0@farik.invalid`). Commands other than `farik` (`cargo`, `docker`, `git`) run under the normal `HOME`, where the toolchain is.
- Sandbox: Docker (no `settings.json`, so the default). The shipped image has the toolchain but no crates, and every sandbox Farik makes for a criterion run on a base or an integration branch has the network off (steps 06, 14), so stage 1 layers the workspace's crates onto the image, `cargo fetch --locked` with `CARGO_NET_OFFLINE=true`, under the same tag. Every brief therefore forbids new dependencies. Rejected: granting `network` to the Developers, which leaves Farik's own offline runs unable to build. The Dockerfile's first comment says `farik run` builds the image (step 13), and step 15 decided it is neither built nor probed, so Task 1 corrects the comment.
- Sequence: the requests run one at a time, small first, each to acceptance and integration before the next is filed. The small one is the cheapest proof that the loop works end to end before the epics spend; one at a time keeps two epics from editing `crates/cli/src/lib.rs` at once and keeps the log readable. Within an epic, the briefs ask that tasks editing one file depend on each other, so they do not conflict at integration.
- Triage is the Product Manager's (section 11). The founder does not pass `--size` and does not overrule. The size that counts is the request's kind when it leaves `refining`, since 5.16 lets the Product Manager re-size a standalone task as large while refining. A kind other than the intended one is an incident, and the whole run starts again from stage 1 with the same briefs (a fresh clone and `.farik/`, because the log must be one run); a second miss means the brief is unclear, and the founder rewrites it.
- Co-writing: request 3's brief and answers are the founder's, the words the Product Manager's. The founder sends the draft back at least once with `farik resolve <id> refining <what to change>` (5.16 item 2), then locks it with `farik contract lock` and approves. No one edits a contract file by hand: no event would record it.
- The export is `farik log --json`, committed whole and never edited, with its line count, highest seq, and sha256 in the record. Over 50 MiB it is committed gzipped as `m0-exit.events.jsonl.gz`, and the record says so.

## File map

```
crates/runtime/sandbox/Dockerfile        modifies: its first comment, how the image is built (Task 1)
docs/milestones/m0-exit.md               creates: the record (Task 2), the founder's review and verdict (Task 3)
docs/milestones/m0-exit.events.jsonl     creates: the event log export (Task 2)
docs/plans/phase-3-runtime/step-18-milestone-0-exit.md   modifies: checkboxes
```

## The team

`~/farik-m0/farik/.farik/team.yaml`, written by [A] before `farik init`, which keeps a team file it finds (section 3):

```yaml
name: Farik M0
agents:
  - id: pm
    display_name: Product Manager
    role: product_manager
    status: active
    persona: Owns the backlog and turns every request into a contract.
    model: { id: claude-opus-5-5, effort: high }
  - id: dev-a
    display_name: Developer A
    role: software_developer
    status: active
    persona: Writes the code and the tests that hold it.
    model: { id: claude-opus-5-5, effort: high }
  - id: dev-b
    display_name: Developer B
    role: software_developer
    status: active
    persona: Writes the code and the tests that hold it; reads the other's work as a stranger would.
    model: { id: claude-opus-5, effort: high }
budgets:
  session: { max_input_tokens: 2000000, max_output_tokens: 128000, max_wall_clock_seconds: 3600, max_tool_calls: 400 }
policy: { human_accepts_contracts: high_risk, wip_limit_per_agent: 1, blocked_limit_hours: 1, max_iterations: 3, integration: auto_merge, integration_branch: test/m0-exit }
rules:
  allowed_paths_ceiling: ["crates/cli/**", "docs/SPEC.md"]
  required_criteria: [test]
  require_new_tests: true
  forbidden_commands: ['^\s*cargo\s+(add|install|publish|update)\b']
```

## The three requests

The founder kept these three (2026-09-23). Each brief ends with this paragraph, `<common>`: "Constraints: no new dependencies; change only crates/cli and docs/SPEC.md, whose section 3 describes the new behavior; tests go in crates/cli/tests/, where a test that needs git is #[ignore], so every test criterion's command ends with -- --include-ignored (for example cargo test -p farik --test reading -- --include-ignored); cargo fmt --check and cargo clippy -p farik --all-targets -- -D warnings pass."

1. Small, standalone. "Let farik log start after a sequence number" / "Add --since <seq> to farik log: only events whose seq is greater are printed, in the same order and format, with --json too, and together with --task, --kind and --limit (the limit counts from the first event after the seq). A seq at or past the last event prints nothing at all (not the line an empty log prints), and with --json no lines, and exits 0; a value that is not a positive whole number is refused by the argument parser. A second terminal can then follow a running project from where it last looked. Budget: at most 20 dollars and 10 sessions. <common>"
2. Large, written by the Product Manager. "Filter the board" / "farik board shows every task, and once there are more than a few the human cannot see what needs them. This has three parts. (1) Filter by status (--status, repeatable) and by kind (--kind epic|task). (2) Filter by assignee (--assignee <agent id>) and by epic (--parent FRK-n, that epic's tasks); an agent id not in the team or a parent that is not an epic is refused with a sentence naming it. (3) --waiting: only the tasks waiting on the human (an open question, awaiting approval, or escalated). Filters combine as AND and apply to --json; the line format does not change. Budget: the epic at most 60 dollars and 15 sessions; each task at most 10 sessions and a budget that fits within what remains of the epic's; tasks that edit the same file depend on one another. <common>"
3. Large, co-written and locked. "Check a command or a path against the team's rules" / "A team writes forbidden_commands as regular expressions and protected_paths and allowed_paths_ceiling as globs, and finds out they were wrong only when an agent is refused. This has two parts. (1) farik rules check <command...> says what farik_exec would answer for this team: allowed, or the refusal the governor's evaluate_command returns, in its words. (2) farik rules check --path <path> says whether the path is protected or outside the ceiling, through the governor's check_protected_paths and check_allowed_paths. Both call the governor's own functions, so the answer is the one an agent would get; exit 0 when allowed, 1 when refused. Budget: the epic at most 40 dollars and 15 sessions; each task at most 10 sessions and a budget that fits within what remains of the epic's; tasks that edit the same file depend on one another. <common>"

The first line before " / " is the title; the brief passed is title, a blank line, then the text.

## Runbook

`~/farik-m0/env.sh`, sourced before any `farik` command, sets `HOME=~/farik-m0/home`, puts the release `farik` first on `PATH`, and `cd`s to `~/farik-m0/farik`. The founder then exports the credential in that terminal.

### Stage 1: prerequisites [A]

1. Task 1, then: `claude --version` → at or above `2.1.272`; `docker info` exits 0; `cargo xtask check --integration` at the phase branch head → `xtask check: ok`; `cargo build --release -p farik` → `Finished`. That head is the start-gate sha, and it must be the local branch `phase/3-runtime` in `/home/ashaat/Farik` (fetched and fast-forwarded first if the work was pushed from elsewhere), because a bare clone copies local branches only.
2. `git clone --bare /home/ashaat/Farik ~/farik-m0/origin.git` → `git -C ~/farik-m0/origin.git cat-file -e <sha>` exits 0; `git -C ~/farik-m0/origin.git branch test/m0-exit <sha>`; `git clone --branch test/m0-exit ~/farik-m0/origin.git ~/farik-m0/farik`, then the local identity → `git -C ~/farik-m0/farik remote get-url origin` prints the bare path.
3. `docker build -t farik/sandbox:<version> crates/runtime/sandbox` (version from the workspace `Cargo.toml`), then the crates layer from `~/farik-m0/crates.Dockerfile` (`FROM farik/sandbox:<version>`; `COPY . /tmp/fetch`; `RUN cd /tmp/fetch && cargo fetch --locked && rm -rf /tmp/fetch && chmod -R a+rwX /usr/local/cargo`; `ENV CARGO_NET_OFFLINE=true`), context the clone, same tag. Smoke: in a throwaway clone of `origin.git`, `docker run --rm --network none --user $(id -u):$(id -g) -v <it>:/workspace farik/sandbox:<version> cargo test -p farik --no-run` → exit 0, `Finished`; then the same `docker run` with `cargo test -p farik --test reading -- --include-ignored` → exit 0, `test result: ok`, which proves git and the offline crates inside the container as the user's uid; then delete that clone.
4. Write `team.yaml` as above and record its sha256. Hand over to [F].

- [ ] Task 1 [A]: `docs(runtime): say the sandbox image is built by hand for now`

### Stage 2: the project [F]

The founder reads `team.yaml`, then `farik init` → exit 0, `team.yaml`'s sha256 unchanged; `farik board` prints no task; `farik doctor` exits 0.

### Stage 3: request 1, small [F]

1. `farik contract new --brief "<brief 1>"` → `FRK-1 filed as a draft request: Let farik log start after a sequence number`, a `request.triaged — small by pm: …` line, any `question <n> from pm: …` with `answer> ` (the founder types the answer), `contract.written`, the contract, `readiness: passed`. If the Product Manager marked it `high` risk, it ends awaiting approval (`risk_gate`) instead, which is allowed: the founder reads it and runs `farik approve FRK-1`.
2. `farik run` → `credential: ANTHROPIC_API_KEY (an API key)`, no warning, one `FRK-1: …` line per tick, `idle: nothing on the board needs doing`, nothing waiting. Anything listed as waiting is the founder's to act on with the command printed beside it, then `farik run` again.
3. [A] checks: `farik board` shows FRK-1 `accepted` and no task `blocked` or `escalated`, and for every task not accepted, `farik task show`'s `cost:` and `sessions:` are below its contract's `max_cost_usd` and `max_sessions` (a task at either waits unlisted, see On failure); `farik task show FRK-1` shows `ready -> assigned by pm`, an `implement` session for one Developer and a `verify` session for the other, `review.recorded`, `verifying -> accepted by pm`, and `task.integrated — <sha> into test/m0-exit by governor`; `git -C ~/farik-m0/farik branch --list farik/FRK-1` lists the branch; `git -C ~/farik-m0/origin.git log --format=%H test/m0-exit` contains the sha.

### Stage 4: request 2, the Product Manager's epic [F]

1. `farik contract new --brief "<brief 2>"` → `FRK-2 filed…`, `request.triaged — large by pm: …`, the Product Manager's questions answered at `answer> `, `readiness: passed the structural checks`, `FRK-2 awaits your approval: farik approve FRK-2, or farik resolve FRK-2 refining <why>`.
2. The founder reads `farik task show FRK-2`, then either `farik approve FRK-2` → exit 0, the log gaining `escalated -> ready by human` and `human.accepted — contract by human`; or `farik resolve FRK-2 refining "<why>"`, then `farik plan` (a `refine` session, idle awaiting approval again), and back to reading.
3. `farik run` → the epic assigned to `pm` and broken down (`task.created` with `parent: FRK-2` by `pm`), each child judged, assigned, implemented, verified, accepted, and integrated, the close-out, the epic `verifying`, Farik's `criterion.recorded … run by reviewer` on `test/m0-exit`; ends with `FRK-2 may need your acceptance: farik accept FRK-2 --message <your review>`.
4. The founder reads each child's `farik task show <id> --diff` and the epic's criterion results, then `farik accept FRK-2 --message "<review>"` → exit 0. `farik run` → the Product Manager's `verify` session, `verifying -> accepted by pm`, idle. [A] repeats stage 3's checks for each child.

### Stage 5: request 3, the co-written epic [F]

1. `farik contract new --brief "<brief 3>"` → as stage 4 step 1, with `FRK-<n>`.
2. `farik resolve FRK-<n> refining "<what to change>"` at least once, then `farik plan` → a `refine` session, idle with the epic awaiting approval again; repeat until the draft is the founder's.
3. `farik contract lock FRK-<n>` → `contract.locked` by `human`; `farik approve FRK-<n>` → `escalated -> ready by human`.
4. Stage 4 steps 3 and 4 for this epic.

### Stage 6: the record [A]

1. `farik doctor` → exit 0. `farik metrics` and `farik --json metrics` → the thirteen lines of step 16 and one object, both copied verbatim.
2. In the clone, `farik log --json > /home/ashaat/Farik-p3/docs/milestones/m0-exit.events.jsonl` (the phase branch's checkout, where Tasks 2 and 3 commit; not `/home/ashaat/Farik`, whose checkout is on another branch with uncommitted work) → its line count equals the last line's `seq`.
3. A worktree of `origin.git`'s `test/m0-exit`: `cargo xtask check --integration` → `xtask check: ok`.
4. Write `m0-exit.md` in this order: header (dates, farik sha, `claude` version, image id, models, credential kind, policy, where it ran, `team.yaml` and its sha256); the three requests (id, triage size and reason, kind on leaving refining, title); the timeline (each command and when); the pass criteria below, each with the event seqs or git output that show it; metrics (text and JSON); the per-task review table (task, kind, parent, assignee and model, reviewer and model, verifications, rejections, interventions, cost, criteria mechanical of total, integrated sha, then "done as contracted" and "why", both left for the founder); costs (per task from `task show`, per purpose from the metrics, the total, and any unpriced report, which ADR 0015 says the dollars leave out); incidents (each escalation, block, refusal that mattered, interrupted session, integration conflict, triage miss, and what the human did, by seq); earlier attempts (each bug found and its fix commit); the export (path, lines, highest seq, sha256); then the empty "Founder's review" and "Verdict" sections. Hand over to [F].

- [ ] Task 2 [A]: `docs(docs): record the milestone 0 exit run`

### Stage 7: the founder's review [F]

The founder fills the table's last two columns for every task and epic, writes the review in their own words from the diffs and the log, and writes the verdict against the criteria below, signed and dated.

- [ ] Task 3 [F]: `docs(docs): add the founder's milestone 0 review`

## Pass criteria

All must hold. In the log: three requests filed by `human`; `request.triaged` by `pm`, and on leaving `refining` two epics and one standalone task (no parent, never broken down); for each epic, a `question.asked` before its first `contract.written` or an intent saying there were none, `escalation.raised { approval }`, and the founder's `human.accepted { contract }`; request 3 has at least one `escalation.resolved { to: refining }` and a `contract.locked` by `human` before its approval; each epic broken down by `pm` into at least one task; for every task, `ready -> assigned` requested by `pm`, naming one Developer as assignee and the other as reviewer, the `implement` session the assignee's, the `verify` session the reviewer's, `verifying -> accepted by pm`, and `task.integrated` by `governor` into `test/m0-exit`; each epic's mechanical results are `criterion.recorded { run_by: reviewer, recorded_by: governor }` with evidence opening `at <sha>`, a sha on `test/m0-exit`, and the founder's `human.accepted { result }` with a message comes before its `verifying -> accepted by pm`; all three requests end `accepted`. Outside the log, where no event records it: each task's own worktree is shown by its `assigned -> in_progress` followed by the assignee's `implement` session in the log and by its `farik/FRK-<n>` branch in the clone; every `task.integrated` sha is in `git -C ~/farik-m0/origin.git log test/m0-exit`, which is the push; `farik doctor` is clean; the export is complete; `cargo xtask check --integration` is green on `test/m0-exit`; nothing under `.farik/` or the log was edited by hand; the metrics are printed; and the founder's review says "yes" for every task and epic. Escalations, blocks, and rejections do not fail the run: they are what the metrics count.

## On failure

- A bug in Farik (it does what section 5 or a step plan says it must not, crashes, refuses a move that should pass, or records a wrong or missing event): stop (Ctrl-C, twice if a session must end now), record it with its seqs, fix it on `phase/3-runtime` under TDD (the failing test first, its own `fix(<crate>): …` commit, a landing review), then re-run from stage 1 with the new binary, a fresh clone, and a fresh `.farik/`. The failed run is summarised under "earlier attempts"; its log is not committed. Never edit the log, the projections, a contract file, or the export, and never resume a run on a different binary.
- A "no" in the founder's review is a failure of the loop, not of the agent: find which gate let the work through, fix that (a role's prompt, a rule, or code) through the workflow, and re-run.
- A blocked task escalates only when a tick runs, and `farik run` exits when idle, so a run can end idle with a task `blocked`, which stage 3's check shows. After `blocked_limit_hours` (one hour) the founder runs `farik run`, which escalates it (`blocker_age`), then `farik resolve <id> in_progress "<what unblocks it>"` and `farik run`. An incident, not a failure.
- A session that reaches its tokens, wall clock, or tool calls ends, and the next tick starts another. An incident.
- A task rejected past its iterations escalates (`iterations`): `farik resolve <id> in_progress "<message>"`, then `farik run`. An incident.
- A task at its contract's `max_sessions` or `max_cost_usd` (enforced as `TaskUsd`) does not escalate: it waits, unlisted, and nothing resumes it (step 11). Stop the run and re-run from stage 1 with the brief's figures raised, recorded under "earlier attempts".
- Outside Farik (the API down, a rate limit, Docker stopped): Ctrl-C, restore it, `farik run` again, which recovers (5.15); an incident.
- An integration conflict escalates the accepted task (step 13): the founder merges by hand in the clone and runs `farik integrate <id>`; an incident.

## Verification

```
cargo xtask check
# expected: xtask check: ok   (on the phase branch after Task 3; this step changes a comment and documents only)
```

When the verdict is pass, the phase's pull request is marked ready (rule 11).
