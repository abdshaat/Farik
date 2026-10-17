# Standalone (parallel agents), plan 5: the spec says agents work at once

Status: draft
Branch: `feat/parallel-agents` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md`, revision 0.118
Depends on: plans 1, 2, 2b, 3 and 4 of this folder (committed on this branch)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16 (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`docs/SPEC.md` says what plans 1, 2, 2b, 3 and 4 built (hard rule 8), in one revision, 0.118: sessions of
different agents run at once, one per agent, everything else one at a time; a task under review
does not count towards its assignee's WIP limit and a sent-back one does and stays with its
author; a chat is a quick fork beside its agent's work; a spent day stops every running session; the
drivers print each start and drain on stop.
This plan touches documents only, so it has no test; its check is the full check and a reading
against those plans' Decisions.

## Decisions

- **One revision**, 0.118, dated 2026-10-16, naming ADR 0064 and the standalone plans
  `parallel-agents` 1 to 4 and 2b, placed after 0.117's line in the revisions list at the top. 0.115 to
  0.117 are ADRs 0061 to 0063's. Each changed paragraph carries "(changed in 0.118; ADR 0064)" or
  "(added in 0.118; ADR 0064)".
- **The revision line**: "Revision 0.118 (2026-10-16) records that agents work in parallel
  (ADR 0064, standalone plans `parallel-agents`): in 3 `catervas stop` stops the run after its
  running sessions; in 5.2 a task in `verifying` no longer counts
  towards its assignee's work-in-progress limit, a `rejected` one counts from its rejection and is
  never handed to another agent; in 4.3 and 5.9 a chat is a fork that runs at once beside its
  agent's work session, one per agent, bounded to answer quickly and told what the agent is doing
  now; in 5.5 a daily budget crossed by one session stops every other
  session running then; in 5.14 integration stays one at a time beside concurrent sessions; in 6.7
  a `stdio` connector's folder is safe because an agent runs one session at a time; in 8.2 one
  session per agent runs at once while everything else a tick does stays one at a time under the
  process's turn, a rule passes over an agent whose session runs and a task whose session runs, a
  driving process prints a line when a session starts, and Ctrl-C stops after, or aborts, every
  running session, and a tick that fails stops the run once they have ended."
- **3**, the paragraph "The team is driven from the command line": "`catervas stop` (the run,
  after its session)" becomes "`catervas stop` (the run, after its running sessions; changed in
  0.118; ADR 0064)".
- **5.2**, the `ready -> assigned` row's gate, the work-in-progress sentence as 0.116 left it:
  the counted statuses become `assigned`, `in_progress` and `rejected` (not waiting on the human,
  as 0.116 says), and the row says why `verifying` is not counted ("a task under review is its
  reviewer's work, and when it comes back it counts again from its rejection, so its author takes
  no new work while it waits") and that a sent-back task is never handed to another agent, unlike a
  blocked, escalated or waiting one (0.116's hand-over). The private folder's hold, in the same
  row, is unchanged.
- **4.3** (asking an agent a question) and **5.9**, at the end of 5.9's paragraph that begins
  "One-to-one chats (added in 0.34; ADR 0026)": add "A chat
  session is a fork (added in 0.118; ADR 0064): it starts at once, before any other work, beside
  the agent's work session, which it does not touch; an agent has at most one chat fork at a time,
  and messages written meanwhile are answered by the next one. It runs in the project's root with
  no worktree, sandbox, executor, connector or browser, on the read tier and the chat's tools, at
  low effort, within 100,000 input and 4,000 output tokens, 3 minutes and 12 tool calls, or the
  team's limits where smaller (the interview keeps the team's limits). Its first message says what
  the agent is doing now: the task of its running work session, else its oldest held task, with
  its title, status and the agent's last note on it." 4.3 gets one sentence pointing to it, right after the one ending "whose one write is its
  reply.": "A chat answers at once, beside the agent's work (5.9; added in 0.118; ADR 0064)."
- **5.5**, the paragraph "This release enforces the table above": after "a task's dollars or a
  daily budget crossed during a session stops the next one from starting rather than cutting that
  one short", add: "; with sessions of several agents running at once (8.2), a daily budget that a
  session's cost crosses also stops every other session running in the process then, with the
  reason `the team's daily budget is spent`: each ends `aborted`, its task keeps its status, and
  nothing is escalated, so the overshoot is what the running sessions had used by then (added in
  0.118; ADR 0064)".
- **5.14**, "One task integrates at a time": after "it is integration alone that is serialised",
  add: "Sessions of different agents run at once (8.2); integration runs in a tick, which holds the
  process's turn as well as this lock (added in 0.118; ADR 0064)."
- **6.7**, "Connecting a server again while a session runs it …": "and Catervas runs one session
  at a time" becomes "and an agent runs one session at a time, the folder being the agent's
  (changed in 0.118; ADR 0064)".
- **8.2**, "Driving the team":
  - after "It then ticks until nothing needs doing, printing what each tick did", add: "A tick
    takes the process's turn and holds it while it reads the board, asks the governor, runs
    Catervas's own criteria, integrates, and prepares a session; it gives the turn back while its
    session runs and takes it again when the session ends. So every agent may have one session
    running beside other agents' sessions, while everything else is done one at a time; an agent
    never runs two work sessions at once, and a rule passes over an agent whose session runs as it
    passes over a sleeping one, and over a task whose session runs; an agent's chat is the one
    session beside its work session, a fork (5.9). There is no other limit than
    the team's active agents. A session's start prints `<task>: <agent> started its <purpose>
    session` (`<agent>: started its <purpose> session` about no task; JSON `{"started", "agent_id",
    "task_id"}`), and its tick's line follows when it ends. While sessions run and the next tick
    finds nothing, the process waits for a session to end, a command, a filed request, or the
    minute's recheck, and ticks again; it says it is idle, or that it waits for a sleeping agent,
    only when nothing runs (added in 0.118; ADR 0064)."
  - "The first Ctrl-C stops the run after the session running now; the second aborts that
    session" becomes "The first Ctrl-C stops the run once every session running now has ended;
    the second aborts them all" (changed in 0.118), and the ", leaving its task where it is"
    that follows becomes ", leaving each task where it is"; "the second press has no session to
    abort" becomes "the second press aborts only the sessions running beside it".
  - a tick that fails, one sentence right after the "(added in 0.9)." that closes the Ctrl-C
    sentences: "A tick that fails stops the run as the first Ctrl-C does, and the process exits 1
    once the running sessions have ended (130 when Ctrl-C was pressed; added in 0.118; ADR 0064)."
- **The design and decision documents.** ADR 0062's plan 06
  (`docs/plans/standalone/no-sprints/06-wip-and-resume.md`) is not edited: it is done and its
  amendment is ADR 0064 and plan 1 here. This folder's plans are ticked by their own commits.
  `docs/plans/standalone/parallel-agents/README.md`'s `Status` becomes `done`.
- No change to `docs/standards/` and no new ADR.

## File map

```
docs/SPEC.md                                           modifies: revision 0.118; 3, 4.3, 5.2, 5.5, 5.9, 5.14, 6.7, 8.2 (Task 1)
docs/plans/standalone/parallel-agents/README.md        modifies: Status done (Task 1)
docs/plans/standalone/parallel-agents/05-spec.md       modifies: its checkbox (Task 1)
```

## Interfaces

Consumes: plans 1, 2, 2b, 3 and 4's behaviour as their Decisions state it. Produces: nothing.

## Tasks

### Task 1: SPEC 0.118

Files as the map. No test (documents only); the check is the full check and a read of each
changed paragraph against plans 1, 2, 2b, 3 and 4.

- [ ] `docs(spec): agents work in parallel, revision 0.118`

## Verification

```
cargo xtask check
# expected: xtask check: ok
grep -c "0.118; ADR 0064" docs/SPEC.md
# expected: 8 (the lines of 3's, 4.3's, 5.2's row, 5.5's, 5.9's, 5.14's, 6.7's and 8.2's
# paragraphs; 8.2's changes are all on its one "Driving the team" line)
```
