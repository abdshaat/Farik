# Phase 3, step 17: Milestone 0 exit

Status: draft (readiness review pending)
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` section 11 (Milestone 0), F17; the flow it exercises is 5.2, 5.4, 5.7, 5.11, 5.14, 5.16
Depends on: steps 01 to 16 of this phase, a start gate: stage 1 does not begin until step 16's last commit is on this branch and `cargo xtask check --integration` passes on it
Readiness confirmed by: pending

This step writes no code and has no interfaces. It is a runbook: stages run in order, and each says who runs it. **[A]** is the agent executing this plan. **[F]** is the founder, and only the founder: the agent stops, hands over with the exact command to type, and waits. The agent never answers a question, approves, accepts, resolves, locks, files a request, runs a command that spends, sets or reads the credential, or writes the review, in the founder's name or otherwise.

## Goal

Milestone 0's exit test, run once for real and recorded. On a copy of Farik's own repository, with real Claude Code sessions, a team of a Product Manager and two Developers takes three requests from the founder (two large, one small) through triage, questions, contracts, approval, breakdown, implementation, verification, acceptance, and integration; one epic is co-written and locked by the founder; the harness metrics are printed; and the founder, reading the diffs and the event log, writes whether each task was done as contracted. The record is `docs/milestones/m0-exit.md` with the log export `docs/milestones/m0-exit.events.jsonl`. Out of scope: merging the team's three improvements into Farik (a separate change after the phase merges), any threshold on a metric (section 11 names none; this run is the baseline), and anything with a UI.

## Decisions

- Where: a fresh clone under `~/farik-m0/`, never the founder's working copy. `~/farik-m0/origin.git` is a bare clone of `/home/ashaat/Farik`, and `~/farik-m0/farik` is cloned from it, so the run's `origin` is that bare repository. Rejected: a branch of the GitHub repository, which a push would reach (and rule 11 would then want a pull request for); a clone with no remote, which would never exercise ADR 0012's push.
- Policy: `auto_merge` into `test/m0-exit`, a branch created in the bare repository at the phase branch's head when stage 1 runs. It is the policy a new project gets (ADR 0012), so the exit tests what users will run; it pushes, but only to the bare repository, so `main`, the phase branch, and GitHub are never touched. Rejected: `manual`, which adds a founder step per task and holds each epic's run until the founder merges (ADR 0013) without testing the default path.
- The binary: `farik` built in release mode from the phase branch at the start gate's commit; that sha, `claude --version`, the image id, and the models go in the record. A fix to Farik means a new binary and a fresh run (below).
- Credential: `ANTHROPIC_API_KEY`, so that the dollars Farik records and the cap are what is billed, and no subscription rate limit ends a session for a reason outside the harness. The founder may use `CLAUDE_CODE_OAUTH_TOKEN` instead if they have no key; the record says which `farik run` named. It is exported in the founder's terminal only, never written to a file, and never seen by the agent.
- A dedicated `HOME` (`~/farik-m0/home`) for every `farik` command, so the founder's own Claude Code settings, plugins, skills, and user memory do not enter agent sessions. Farik's `CLAUDE.md` stays in the repository: it is the project's own, as a user's would be. The clone's git identity is set in its local config (`Farik M0`, `m0@farik.invalid`).
- Sandbox: Docker (no `settings.json`, so the default). The shipped image has the toolchain but no crates, and every sandbox Farik makes for a criterion run on a base or an integration branch has the network off (steps 06, 14), so stage 1 layers the workspace's crates onto the image, `cargo fetch --locked` with `CARGO_NET_OFFLINE=true`, under the same tag. Every brief therefore forbids new dependencies. Rejected: granting `network` to the Developers, which leaves Farik's own offline runs unable to build.
- Sequence: the requests run one at a time, small first, each to acceptance and integration before the next is filed. The small one is the cheapest proof that the loop works end to end before the epics spend; one at a time keeps two epics from editing `crates/cli/src/lib.rs` at once and keeps the log readable.
- Triage is the Product Manager's (section 11). The founder does not pass `--size` and does not overrule. A triage other than the intended size is an incident; the request is re-run from a fresh clone once with the same brief, and a second miss means the brief is unclear, which the founder rewrites.
- Co-writing: request 3's brief and answers are the founder's, the words the Product Manager's. The founder sends the draft back at least once with `farik resolve <id> refining <what to change>` (5.16 item 2), then locks it with `farik contract lock` and approves. No one edits a contract file by hand: no event would record it.
- Budget: `daily_usd: 60` in the team, enforced by Farik; a whole-run cap of 150 dollars, checked by the agent before each `farik run` or `farik contract new`. `max_task_budget_usd: 20`; each brief asks for at most 8 sessions per task, since a clean standalone task already takes five (refine, plan, implement, verify, accept). Session wall clock 3600 s, because a cold Rust build in a new worktree takes minutes; the token limits stay at their defaults, and a hit is an incident.
- The export is `farik log --json`, committed whole and never edited, with its line count, highest seq, and sha256 in the record. Over 50 MiB it is committed gzipped as `m0-exit.events.jsonl.gz`, and the record says so.

## File map

```
docs/milestones/m0-exit.md               creates: the record (Task 1), the founder's review and verdict (Task 2)
docs/milestones/m0-exit.events.jsonl     creates: the event log export (Task 1)
docs/plans/phase-3-runtime/step-17-milestone-0-exit.md   modifies: checkboxes
```

## The team

`~/farik-m0/farik/.farik/team.yaml`, written before `farik init`, which keeps a team file it finds (section 3):

```yaml
name: Farik M0
agents:
  - id: pm
    display_name: Product Manager
    role: product_manager
    status: active
    persona: Owns the backlog and turns every request into a contract.
    model: { id: claude-opus-5, effort: high }
  - id: dev-a
    display_name: Developer A
    role: software_developer
    status: active
    persona: Writes the code and the tests that hold it.
    model: { id: claude-opus-5, effort: high }
  - id: dev-b
    display_name: Developer B
    role: software_developer
    status: active
    persona: Writes the code and the tests that hold it; reads the other's work as a stranger would.
    model: { id: claude-opus-5, effort: high }
budgets: { daily_usd: 60, session: { max_wall_clock_seconds: 3600 } }
policy: { human_accepts_contracts: high_risk, wip_limit_per_agent: 1, blocked_limit_hours: 24, max_iterations: 3, integration: auto_merge, integration_branch: test/m0-exit }
rules:
  allowed_paths_ceiling: ["crates/cli/**", "docs/SPEC.md"]
  required_criteria: [test]
  require_new_tests: true
  max_task_budget_usd: 20
  forbidden_commands: ['^\s*cargo\s+(add|install|publish|update)\b']
```

## The three requests

Each brief ends with this paragraph, `<common>`: "Constraints: no new dependencies; change only crates/cli and docs/SPEC.md, whose section 3 describes the new behavior; tests go in crates/cli/tests/, where a test that needs git is #[ignore] and runs with cargo test -p farik -- --include-ignored; at most 8 sessions per task."

1. Small, standalone. "Let farik log start after a sequence number" / "Add --since <seq> to farik log: only events whose seq is greater are printed, in the same order and format, with --json too, and together with --task, --kind and --limit (the limit counts from the first event after the seq). A seq past the last event prints nothing and exits 0; a value that is not a positive whole number is refused by the argument parser. A second terminal can then follow a running project from where it last looked. <common>"
2. Large, written by the Product Manager. "Filter the board" / "farik board shows every task, and once there are more than a few the human cannot see what needs them. Let the board be filtered by status (--status, repeatable), kind (--kind epic|task), assignee (--assignee <agent id>), epic (--parent FRK-n, that epic's tasks), and --waiting (only tasks waiting on the human: an open question, awaiting approval, or escalated). Filters combine as AND and apply to --json; an agent id not in the team or a parent that is not an epic is refused with a sentence naming it; the line format does not change. <common>"
3. Large, co-written and locked. "Check a command or a path against the team's rules" / "A team writes forbidden_commands as regular expressions and protected_paths and allowed_paths_ceiling as globs, and finds out they were wrong only when an agent is refused. Add farik rules check <command...>, which says whether farik_exec would run the command for this team or refuses it with every reason (a git first word; each forbidden pattern that matches, named), and farik rules check --path <path>, which says whether the path is protected or outside the ceiling. Both use the governor's own functions, so the answer is the one an agent would get; exit 0 when allowed, 1 when refused. <common>"

The first line before " / " is the title; the brief passed is title, a blank line, then the text.

## Runbook

Environment, sourced by both before any `farik` command: `~/farik-m0/env.sh` sets `HOME=~/farik-m0/home`, puts the release `farik` first on `PATH`, and `cd`s to `~/farik-m0/farik`. The founder then exports the credential in that terminal.

### Stage 1: prerequisites [A]

1. `claude --version` → a version at or above `2.1.272`. `docker info` exits 0. The start gate's `cargo xtask check --integration` → `xtask check: ok`; `cargo build --release -p farik` → `Finished`.
2. `git clone --bare /home/ashaat/Farik ~/farik-m0/origin.git && git -C ~/farik-m0/origin.git branch test/m0-exit <sha>`, then `git clone --branch test/m0-exit ~/farik-m0/origin.git ~/farik-m0/farik` and the local identity → `git -C ~/farik-m0/farik remote get-url origin` prints the bare path.
3. `docker build -t farik/sandbox:<version> crates/runtime/sandbox` (version from the workspace `Cargo.toml`), then the crates layer from `~/farik-m0/crates.Dockerfile` (`FROM farik/sandbox:<version>`; `COPY . /tmp/fetch`; `RUN cd /tmp/fetch && cargo fetch --locked && rm -rf /tmp/fetch && chmod -R a+rwX /usr/local/cargo`; `ENV CARGO_NET_OFFLINE=true`), context the clone, same tag. Smoke: in a throwaway clone of `origin.git`, `docker run --rm --network none --user $(id -u):$(id -g) -v <it>:/workspace farik/sandbox:<version> cargo test -p farik --no-run` → exit 0, `Finished`; then delete that clone.
4. Write `team.yaml` as above, then `farik init` → exit 0 and `team.yaml`'s sha256 unchanged; `farik board` prints no task; `farik doctor` exits 0.
5. Hand over to [F] with the stage 2 commands.

### Stage 2: request 1, small [F]

1. `farik contract new --brief "<brief 1>"` → `FRK-1 filed as a draft request: Let farik log start after a sequence number`, a `request.triaged — small by pm: …` line, any `question <n> from pm: …` with `answer> ` (the founder types the answer), `contract.written`, the contract, `readiness: passed`.
2. `farik run` → `credential: ANTHROPIC_API_KEY (an API key)`, no sandbox warning, one `FRK-1: …` line per tick, `idle: nothing on the board needs doing`, nothing waiting. Anything listed as waiting is the founder's to act on with the command printed beside it, then `farik run` again.
3. [A] checks: `farik board` shows FRK-1 `accepted`; `farik task show FRK-1` shows `session.started` for `dev-a` or `dev-b` as `implement` and the other as `verify`, `review.recorded`, `verifying -> accepted by pm`, and `task.integrated — <sha> into test/m0-exit by governor`; `git -C ~/farik-m0/origin.git log -1 test/m0-exit` is that sha. The running cost is recorded.

### Stage 3: request 2, the Product Manager's epic [F]

1. `farik contract new --brief "<brief 2>"` → `FRK-2 filed…`, `request.triaged — large by pm: …`, the Product Manager's questions answered at `answer> `, `readiness: passed the structural checks`, `FRK-2 awaits your approval: farik approve FRK-2, or farik resolve FRK-2 refining <why>`.
2. The founder reads `farik task show FRK-2`, then `farik approve FRK-2` (or sends it back with `farik resolve`) → exit 0; the log gains `escalated -> ready by human` and `human.accepted — contract by human`.
3. `farik run` → the epic assigned to `pm` and broken down (`task.created` with `parent: FRK-2` by `pm`), each child judged, assigned, implemented, verified, accepted, and integrated, the close-out, the epic `verifying`, Farik's `criterion.recorded … run by reviewer` on `test/m0-exit`; ends with `FRK-2 may need your acceptance: farik accept FRK-2 --message <your review>`.
4. The founder reads each child's `farik task show <id> --diff` and the epic's criterion results, then `farik accept FRK-2 --message "<review>"` → exit 0. `farik run` → the Product Manager's `verify` session, `verifying -> accepted by pm`, idle.

### Stage 4: request 3, the co-written epic [F]

1. `farik contract new --brief "<brief 3>"` → as stage 3 step 1, with `FRK-<n>`.
2. `farik resolve FRK-<n> refining "<what to change>"` at least once, then `farik plan` → a `refine` session, idle with the epic awaiting approval again; repeat until the draft is the founder's.
3. `farik contract lock FRK-<n>` → `contract.locked` by `human`; `farik approve FRK-<n>` → `escalated -> ready by human`.
4. Stage 3 steps 3 and 4 for this epic.

### Stage 5: the record [A]

1. `farik doctor` → exit 0. `farik metrics` and `farik --json metrics` → the thirteen lines of step 16 and one object, both copied verbatim.
2. In the clone, `farik log --json > <phase branch worktree>/docs/milestones/m0-exit.events.jsonl` → its line count equals the last line's `seq`.
3. A worktree of `origin.git`'s `test/m0-exit`: `cargo xtask check --integration` → `xtask check: ok`.
4. Write `m0-exit.md` in this order: header (dates, farik sha, `claude` version, image id, models, credential kind, policy, where it ran); the three requests (id, triage size and reason, title); the timeline (each command and when); the pass criteria below, each with the event seqs that show it; metrics (text and JSON); the per-task review table (task, kind, parent, assignee, reviewer, verifications, rejections, interventions, cost, criteria mechanical of total, integrated sha, then "done as contracted" and "why", both left for the founder); costs (per task from `task show`, per purpose from the metrics, total against the cap); incidents (each escalation, refusal that mattered, interrupted session, integration conflict, triage miss, and what the human did, by seq); earlier attempts (each bug found and its fix commit); the export (path, lines, highest seq, sha256); then the empty "Founder's review" and "Verdict" sections.
5. Hand over to [F].

- [ ] Task 1 [A]: `docs(docs): record the milestone 0 exit run`

### Stage 6: the founder's review [F]

The founder fills the table's last two columns for every task and epic, writes the review in their own words from the diffs and the log, and writes the verdict against the criteria below, signed and dated.

- [ ] Task 2 [F]: `docs(docs): add the founder's milestone 0 review`

## Pass criteria

All must hold, each shown in the log: three requests filed by `human`; `request.triaged` by `pm`, two large and one small; the small one a standalone task (no parent, never broken down); for each epic, a `question.asked` before its first `contract.written` or an intent saying there were none, `escalation.raised { approval }`, and the founder's `human.accepted { contract }`; request 3 has at least one `escalation.resolved { to: refining }` and a `contract.locked` by `human` before its approval; each epic broken down by `pm` into at least one task; every accepted task implemented by one Developer in its own worktree, verified by the other, accepted by `pm`, and integrated by `governor` into `test/m0-exit`, pushed to `origin`; each epic has Farik's governor-recorded criterion results, the founder's acceptance with a message, and `verifying -> accepted by pm`; all three requests end `accepted`; the metrics are printed; `farik doctor` is clean, the export complete, and `cargo xtask check --integration` green on `test/m0-exit`; the total cost is within 150 dollars; nothing under `.farik/` or the log was edited by hand; and the founder's review says "yes" for every task and epic. Escalations and rejections do not fail the run: they are what the metrics count.

## Budget cap

Farik stops the day at 60 dollars (`idle: the team's daily budget is spent`): stop, and resume with `farik run` after 00:00 UTC; nobody raises `daily_usd` mid-run. Before each spending command the agent sums `cost_usd` over `farik log --kind cost.recorded --json`; at 150 dollars or more it does not hand over, the run ends, and the verdict is fail (stopped at the cap). A re-run with a higher cap is the founder's written decision.

## On failure

- A bug in Farik (it does what section 5 or a step plan says it must not, crashes, refuses a move that should pass, or records a wrong or missing event): stop (Ctrl-C, twice if a session must end now), record it with its seqs, fix it on `phase/3-runtime` under TDD (the failing test first, its own `fix(<crate>): …` commit, a landing review), then re-run from stage 1 with the new binary, a fresh clone, and a fresh `.farik/`. The failed run is summarised under "earlier attempts"; its log is not committed. Never edit the log, the projections, a contract file, or the export, and never resume a run on a different binary.
- A "no" in the founder's review is a failure of the loop, not of the agent: find which gate let the work through, fix that (a role's prompt, a rule, or code) through the workflow, and re-run.
- Outside Farik (the API down, a rate limit, Docker stopped): Ctrl-C, restore it, `farik run` again, which recovers (5.15); an incident, not a failure.
- An integration conflict escalates the accepted task (step 13): the founder merges by hand in the clone and runs `farik integrate <id>`; an incident.

## Open for the founder

- Confirm the three briefs, or replace them with others of the same sizes: they are the founder's requests.
- Confirm the spend: 60 dollars a day and 150 for the run.

## Verification

```
cargo xtask check
# expected: xtask check: ok   (on the phase branch after Task 2; this step changes documents only)
```

When the verdict is pass, the phase's pull request is marked ready (rule 11).
