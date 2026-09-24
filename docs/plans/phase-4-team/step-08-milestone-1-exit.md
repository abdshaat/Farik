# Phase 4, step 08: Milestone 1 team exit

Status: moved, not run. The founder decided on 2026-09-24 that all live testing is done in the web UI once the product side is built, so this run is carried out at the web UI phase's exit (ADR 0016), in the browser instead of at the command line. Its decisions (the team, the two requests, the pass criteria) are that exit's starting point. Stage 1 was prepared in `~/farik-m1` at a826b2d and is no longer needed.
Branch: `phase/4-team`
Spec: `docs/SPEC.md` section 11 (Milestones 0 and 1), F17; the flow it exercises is 5.2 to 5.9, 5.14, 5.16
Depends on: steps 01 to 07 of this phase; a start gate: stage 1 does not begin until step 07 has landed on this branch and `cargo xtask check --integration` passes on it
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the briefs and the decision the first found unworkable; its findings folded in)

This step writes no product code and has no interfaces. It is a runbook like phase 3's step 18, whose format and rules it keeps. Its stages run in order, and each says who runs it:
- **[A]** is the agent executing this plan.
- **[F]** is the founder, and only the founder. The agent stops, hands over with the exact command to type, and waits.

The agent never does any of these, in the founder's name or otherwise:
- answer a question, approve, accept, resolve, lock, or say anything in the channel;
- file a request;
- start or end a sprint;
- run `farik init` or a command that starts sessions;
- set or read the credential;
- write the review.

Each of those is recorded in the log as the human.

## Goal

Milestones 0 and 1, run once, for real, and recorded (the founder, 2026-09-24: one run covers both, since phase 3's step 18 was never run). The run uses a fresh copy of Farik's own repository and real Claude Code sessions, with a team of six agents in the five launch roles, and works one sprint from start to end. The founder starts the sprint and the Scrum Master plans it.

Two requests from the founder, a small one and a large one, go through:
- the Scrum Master's triage and judgment;
- the Product Manager's questions, contracts, and the epic's approval;
- the Scrum Master's breakdown and assignment;
- the Developers' implementation on `feature/` or `fix/` branches;
- the Architect's review;
- the Marketing Specialist's `CHANGELOG.md` entry on a `docs/` branch;
- the Product Manager's acceptance;
- integration.

Along the way the channel carries reactions, Farik's lines, and the ceremonies. The sprint ends with a review and a retro written to `team/retro.md`, and the Architect records a decision. The harness metrics are printed per project and per sprint. The founder reads the diffs, the channel, and the log, and writes whether each task was done as contracted.

The record is `docs/milestones/m1-team-exit.md` with the log export `docs/milestones/m1-team-exit.events.jsonl`, and it notes that it closes Milestone 0 too.

Out of scope:
- merging the team's improvements into Farik, which is a separate change after the phase merges;
- any threshold on a metric;
- the desktop app and its thirty-minute new-user test (spec 11's Milestone 1 criterion proper, which needs phase 5).

## Decisions

- Where: phase 3's `~/farik-m0/` is replaced by `~/farik-m1/`, laid out the same way:
  - `origin.git`: a bare clone of `/home/ashaat/Farik`;
  - `farik`: the run's clone;
  - `home`: the dedicated `HOME`;
  - `env.sh` and the briefs.

  `test/m1-exit` is the integration branch, created in the bare repository at the start gate's commit. Rejected: reusing `~/farik-m0`, whose image and clone predate phase 4.
- Policy: `auto_merge` into `test/m1-exit`, pushing only to the bare repository, as in step 18.
- The team, six agents (the founder, 2026-09-24):
  - `pm`, Product Manager, on `claude-opus-5-5` high;
  - `sm`, Scrum Master, on `claude-sonnet-5` medium;
  - `arch`, Architect, on `claude-opus-5-5` high;
  - `dev-a`, Software Developer, on `claude-opus-5-5` high;
  - `dev-b`, Software Developer, on `claude-opus-5` high;
  - `mkt`, Marketing Specialist, on `claude-sonnet-5` medium.

  Ceremonies and conversations run on `claude-sonnet-5`. Every model is priced.
- No dollar limit and no sprint budget (the founder, 2026-09-24; ADR 0015). The loop limits are step 18's: session tokens 2,000,000 in and 128,000 out, a wall clock of 3,600 s, 400 tool calls, `blocked_limit_hours: 1`, and `max_sessions` from the briefs. `escalation_age_hours` is 1, so that an escalation left over an hour is aged during the run.
- Credential: the founder's subscription token, `CLAUDE_CODE_OAUTH_TOKEN`, as in step 18, exported by the founder. A usage limit met during the run is an incident that exercises step 04's sleep, not a failure.
- Rules:
  - `allowed_paths_ceiling`: `["crates/cli/**", "docs/**", "CHANGELOG.md"]`
  - `document_paths` left out, so the defaults apply
  - `required_criteria` left empty. A team rule applies to every contract, and the Marketing Specialist's `CHANGELOG.md` task has no test to run. So each brief says that every Developer task carries at least one `test` criterion with new tests required, and that the CHANGELOG task carries a `review` criterion. `require_new_tests: true` still holds each `test` criterion to new tests.
  - `forbidden_commands` as in step 18
- The requests, written out in full below under "The two requests"; the founder confirms their words at readiness:
  1. Small: "Let farik log start after a sequence number". It is step 18's brief 1, with two changes: its test criteria require new tests, and the Architect reviews it.
  2. Large: "Filter the board". It is step 18's brief 2, rewritten. It adds a `CHANGELOG.md` entry, which the brief's allowed paths now include, and the Marketing Specialist writes it as a document task. It adds a decision on the filtering design, which the Architect records while reviewing the first filter task. It also tells the Product Manager not to write that decision.
- The decision. The Architect's role prompt and skill (step 01) still tell it to write ADRs into the repository. Step 07 added `farik_write_decision` for that, so step 07's fix wave teaches the Architect's prompt and skill to record decisions with the tool, in a commit with its own test, before this run. The brief then puts the decision in a Developer task's contract, as a `review` criterion the Architect answers ("the filtering design is recorded as a decision with farik_write_decision").
- Sequence:
  1. `farik init`.
  2. File request 1, then request 2, each through `farik contract new`, which takes each to a ready contract (the epic to awaiting approval).
  3. Approve the epic.
  4. `farik sprint start`, with no budget.
  5. `farik run`, whose planning ceremony plans both into S1.

  Rejected: starting the sprint first. Its planning ceremony would find no ready candidate and run no session; once contracts became ready, nothing would plan them into the sprint, which admits work only through its one planning.
- Standups: a standup runs only when a UTC day has turned with moves in it. The run records whether one happened. None, when the run ends within one UTC day, is not a failure.
- Branches: the Developers' tasks work on `feature/FRK-<n>` (or `fix/`), the Marketing Specialist's on `docs/FRK-<n>` (step 02).
- The export and the record follow step 18's rules: `farik log --json`, committed whole and never edited, with its line count, highest seq, and sha256.

## File map

```
docs/milestones/m1-team-exit.md               creates: the record (Task 1) and the founder's review (Task 2)
docs/milestones/m1-team-exit.events.jsonl     creates: the export (Task 1)
docs/plans/phase-3-runtime/step-18-milestone-0-exit.md   modifies: its status, closed by this run
docs/plans/phase-4-team/step-08-milestone-1-exit.md      modifies: checkboxes
```

## Runbook

`~/farik-m1/env.sh` is sourced before any `farik` command, as in step 18.

## The two requests

Each brief is its title, a blank line, then its text. [A] writes them to `~/farik-m1/brief1.txt` and `~/farik-m1/brief2.txt`.

1. Let farik log start after a sequence number

   Add --since <seq> to farik log: only events whose seq is greater are printed, in the same order and format, with --json too, and together with --task, --kind and --limit (the limit counts from the first event after the seq). A seq at or past the last event prints nothing at all (not the line an empty log prints), and with --json no lines, and exits 0; a value that is not a positive whole number is refused by the argument parser. A second terminal can then follow a running project from where it last looked. The task is reviewed by the Architect. Budget: at most 20 dollars and 14 sessions. Constraints: no new dependencies; change only crates/cli and docs/SPEC.md, whose section 3 describes the new behavior; the task carries at least one test criterion with new tests required; tests go in crates/cli/tests/, where a test that needs git is #[ignore], so every test criterion's command ends with -- --include-ignored (for example cargo test -p farik --test reading -- --include-ignored); cargo fmt --check and cargo clippy -p farik --all-targets -- -D warnings pass.

2. Filter the board

   farik board shows every task, and once there are more than a few the human cannot see what needs them. This has four parts. (1) Filter by status (--status, repeatable) and by kind (--kind epic|task). (2) Filter by assignee (--assignee <agent id>) and by epic (--parent FRK-n, that epic's tasks); an agent id not in the team or a parent that is not an epic is refused with a sentence naming it. (3) --waiting: only the tasks waiting on the human (an open question, awaiting approval, or escalated). Filters combine as AND and apply to --json; the line format does not change. (4) Create CHANGELOG.md with an Unreleased section holding an entry for the new filters, written for users; this is a document task for the Marketing Specialist, reviewed by the Product Manager with a review criterion. The first filter task, reviewed by the Architect, carries a review criterion that the filtering design is recorded as a decision with farik_write_decision; the Product Manager does not write that decision itself. Budget: the epic at most 60 dollars and 15 sessions; each task at most 14 sessions and a budget that fits within what remains of the epic's; tasks that edit the same file depend on one another. Constraints: no new dependencies; code changes only in crates/cli and docs/SPEC.md, whose section 3 describes the new behavior, and the CHANGELOG task changes only CHANGELOG.md; every Developer task carries at least one test criterion with new tests required; tests go in crates/cli/tests/, where a test that needs git is #[ignore], so every test criterion's command ends with -- --include-ignored; cargo fmt --check and cargo clippy -p farik --all-targets -- -D warnings pass.

### Stage 1: prerequisites [A]

1. Run `claude --version`, `docker info`, `cargo xtask check --integration` at the phase branch head, and `cargo build --release -p farik`, as step 18's stage 1.
2. Clone the bare repository, create `test/m1-exit` at the start-gate sha, clone the run's copy, and set the local identity `Farik M1 <m1@farik.invalid>`.
3. Build the sandbox image and its crates layer, and run the smoke test, as step 18's stage 1.
4. Write `team.yaml`, below, to `~/farik-m1/farik/.farik/team.yaml` and record its sha256. Write `brief1.txt` and `brief2.txt`. Hand over to [F].

### Stage 2: the project [F]

`farik init` → exit 0 and `team.yaml` unchanged; `farik doctor` exits 0.

### Stage 3: the contracts [F]

1. `farik contract new --brief "$(cat ~/farik-m1/brief1.txt)"` should print, in order:
   - triage by `sm`, `small`;
   - the Product Manager's questions, answered at `answer> `;
   - a tick line for `sm`'s judgment session;
   - `readiness: passed`.

   It may instead end awaiting approval, if the Product Manager marked the task `high` risk; then `farik approve FRK-1`.
2. `farik contract new --brief "$(cat ~/farik-m1/brief2.txt)"` → triage by `sm`, `large`; questions; a tick line for `sm`'s judgment session; `readiness: passed the structural checks`; awaiting approval.
3. The founder reads `farik task show FRK-2` and checks three things:
   - its `allowed_paths` include `CHANGELOG.md`;
   - its requirements or criteria name the CHANGELOG entry;
   - they also name the decision the Architect records.

   If all three hold, the founder runs `farik approve FRK-2`. If any is missing, that is an incident: the brief is not clear enough. Record it and re-run from stage 1 with the brief clarified. Do not use `farik resolve FRK-2 refining` followed by `farik plan` here. With no sprint open, `farik plan` would assign FRK-1 and then the approved epic before any sprint exists, and the planning ceremony would find nothing to plan.

### Stage 4: the sprint [F]

1. `farik sprint start` → `S1` open.
2. `farik run`. This runs the planning ceremony (S1 planned with FRK-1 and FRK-2, the plan and the digest in `#planning`), then the work: the epic assigned to `sm` and broken down, its tasks judged, assigned by `sm`, implemented, reviewed (the Architect for code, the Product Manager for the CHANGELOG task), accepted, integrated. Reactions and system lines go to the channel. The run ends idle, listing what waits on the founder. At least once it waits for the epic's acceptance.
3. The founder acts on whatever is listed and runs `farik run` again. During the run the founder mentions an agent once with `farik say "@arch <question>"`, which exercises a conversation. The founder accepts the epic with `farik accept FRK-2 --message "<review>"`. If FRK-1 came out `high` risk, the founder also accepts its result with `farik accept FRK-1 --message "<review>"`. Before accepting the epic, the founder checks that the first filter task's contract carries the decision's `review` criterion. A child contract without it is an incident, because the Scrum Master's breakdown dropped it.
4. After the founder's acceptance, `farik run`: the Product Manager accepts the epic, the sprint ends by itself, and that run holds the review and then the retro.

### Stage 5: the record [A]

1. Run `farik doctor`, `farik metrics`, `farik metrics --sprint S1` (each also with `--json`), `farik sprint show S1`, and `farik channel --last 200`, each copied verbatim.
2. Export the log into the phase branch's checkout.
3. On a worktree of `test/m1-exit`, run `cargo xtask check --integration`.
4. Write `m1-team-exit.md` in step 18's order, with these sections added: the channel, with each ceremony's posts and the reactions; the retro file's text; the decisions written; each agent's memory as the run left it; and the sessions by purpose and role. Its header says that it closes Milestone 0 (step 18) and why. Hand over to [F].

- [ ] Task 1 [A]: `docs(docs): record the milestone 1 team exit run`

### Stage 6: the founder's review [F]

- [ ] Task 2 [F]: `docs(docs): add the founder's milestone 1 review`

## The team

The team is written by [A] before `farik init`:

```yaml
name: Farik M1
agents:
  - { id: pm,    display_name: Product Manager,      role: product_manager,      status: active, persona: "Owns the backlog and turns every request into a contract.", model: { id: claude-opus-5-5, effort: high } }
  - { id: sm,    display_name: Scrum Master,         role: scrum_master,         status: active, persona: "Keeps work moving and the human informed; says little, precisely.", model: { id: claude-sonnet-5, effort: medium } }
  - { id: arch,  display_name: Architect,            role: architect,            status: active, persona: "Holds the shape of the system; reads every diff for design.", model: { id: claude-opus-5-5, effort: high } }
  - { id: dev-a, display_name: Developer A,          role: software_developer,   status: active, persona: "Writes the code and the tests that hold it.", model: { id: claude-opus-5-5, effort: high } }
  - { id: dev-b, display_name: Developer B,          role: software_developer,   status: active, persona: "Writes the code and the tests that hold it; reads the other's work as a stranger would.", model: { id: claude-opus-5, effort: high } }
  - { id: mkt,   display_name: Marketing Specialist, role: marketing_specialist, status: active, persona: "Explains what shipped to the people who will use it.", model: { id: claude-sonnet-5, effort: medium } }
budgets:
  session: { max_input_tokens: 2000000, max_output_tokens: 128000, max_wall_clock_seconds: 3600, max_tool_calls: 400 }
policy: { human_accepts_contracts: high_risk, wip_limit_per_agent: 1, blocked_limit_hours: 1, escalation_age_hours: 1, max_iterations: 3, integration: auto_merge, integration_branch: test/m1-exit }
rules:
  allowed_paths_ceiling: ["crates/cli/**", "docs/**", "CHANGELOG.md"]
  require_new_tests: true
  forbidden_commands: ['^\s*cargo\s+(add|install|publish|update)\b']
```

## Pass criteria

All must hold.

In the log:
- two requests filed by `human`, triaged by `sm` (one small, one large);
- a `contract.judged` by `sm` before each contract left `refining`;
- the epic approved by `human`;
- `sprint.started` then `sprint.planned` by `sm` in a planning ceremony (a `session.started` with `thread: planning`), with ceremony posts in `#planning`;
- the epic assigned to `sm`, with the Product Manager as reviewer, and broken down by `sm`;
- every task assigned by `sm`;
- each Developer task worked on `feature/` or `fix/`, and reviewed by `arch` where the contract's reviewer role is the Architect;
- the Marketing Specialist's task on `docs/FRK-<n>`, its `allowed_paths` within the document paths;
- every task `verifying -> accepted by pm` and integrated by `governor`;
- the epic's mechanical criteria run by Farik, the founder's acceptance with a message, then its acceptance by `pm`;
- at least one `message.posted` of each kind: `reaction`, `system`, `ceremony`, `human`, `reply`;
- `sprint.ended` by `governor`, then a review and a retro ceremony, a `retro.appended`, and a `decision.written` by `arch`.

Outside the log:
- the integrations in `origin.git`;
- `.farik/team/retro.md` with S1's section;
- a file under `.farik/decisions/` whose `decision.written` is by `arch`;
- `farik doctor` clean;
- the export complete;
- the check green on `test/m1-exit`;
- nothing under `.farik/` edited by hand;
- the metrics, project and sprint, printed;
- the founder's review says "yes" for every task and the epic.

Escalations, blocks, rejections, aged escalations, a sleep, and a missing standup do not fail the run: they are what the record reports.

## On failure

- The Scrum Master's judgment may object to a CHANGELOG task that has only a `review` criterion, which sends the task back to refining. If the loop stalls there, it is an incident, not a failure.

- If the planning ceremony plans only one of the two requests, the other never enters S1. Planning runs once per sprint. This is an incident: record it, `farik sprint end`, and re-run from stage 1 with the briefs unchanged.

Step 18's rules hold, with two changes:
- A task out of dollars or sessions now escalates (step 04), and the founder resolves it.
- A usage limit puts the agent to sleep, and the run waits (step 04). It is an incident, and the founder may Ctrl-C and resume later.

A bug in Farik stops the run. The fix goes on `phase/4-team` under TDD and a landing review, and the run starts again from stage 1.

## Verification

```
cargo xtask check
# expected: xtask check: ok   (after Task 2; this step changes documents only)
```

When the verdict is pass, step 18's status is set to "closed by phase 4 step 08" and the phase's pull request is marked ready (rule 11).
