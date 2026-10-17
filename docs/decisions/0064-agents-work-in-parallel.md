# 0064. Agents work in parallel

Date: 2026-10-16
Status: accepted (the founder, 2026-10-16, quoted below; the mechanism and its rules decided by the controller and the planner the same day)

The founder: "Agents must work in parallel. For example the developer start working on a ticket and then after finishing it, the architect will review it. While the review is happening, the developer may pickup another task and start working on it. After finishing the second task, he then may go back and fix the first task if the architect had some comments. While he is fixing the first task, the architect may review the second task. etc."

Amends:
- **ADR 0014 and SPEC 8.2** ("Driving the team"): a driving process no longer runs one session at a time. The first Ctrl-C stops the run after every running session ends; the second aborts all of them.
- **ADR 0053**: unchanged in what it decides (one process drives one project), but the process now runs several sessions at once.
- **ADR 0062's plan 06** (CTV-9, `docs/plans/standalone/no-sprints/06-wip-and-resume.md`): its counted statuses lose `verifying`, its `resume_elsewhere` no longer hands over a `rejected` task, and its "no two agents" argument no longer rests on one session at a time.
- **ADR 0026** (one-to-one chats): a chat session runs beside its agent's work as a bounded fork.
- **SPEC 4.3 and 5.9** (chats), **5.2** (the WIP limit of `ready -> assigned`), **5.5** (a daily budget crossed while sessions run), **5.14** ("One task integrates at a time": unchanged, now beside concurrent sessions), **6.7** (a `stdio` connector's folder is the agent's, and an agent runs one session at a time).

## Context

`Orchestrator::run_until_idle` and the command line's `ticks` loop call one tick at a time, and a tick (`orchestrator/rules.rs`) starts at most one session and awaits it to its end. Across a whole team at most one agent works at any moment: the Architect's review of one task and the Developer's work on the next are taken in turn, never together. On top of that, the WIP count (`transitions::open_tasks`, used by `has_room` and the governor's `AssignmentInput::assignee_open_tasks`) counts a task in `verifying` against its assignee, so with the default limit of one a Developer whose task is under review may not take another, even if sessions could run at once.

## Decision

**Sessions run at once, one per agent.** Each active agent may have one work session running while other agents' sessions run. An agent never runs two work sessions at once: it is one worker. Its chat is the one fork beside it (below). One process still drives a project (ADR 0053).

**Everything but a session's run is still one at a time.** The driving process holds one *turn*. A tick takes the turn at its start and holds it while it reads the board, asks the governor, runs Catervas's own criteria, integrates, and prepares a session. It hands the turn back just before the session runs, and takes it back when the session has ended, to record the end, the cost, the notes, and whatever follows in the same tick. So every rule, every governor move, every integration and every sandbox or worktree made is serialised exactly as before; only the session's own stream of model turns and tool calls runs beside others. The tool calls of concurrent sessions reach the daemon and the governor as the human's commands always have, through the `Transitions` lock and the event log's own serialisation. The integration lock (one integration at a time, SPEC 5.14) and the worktree per task are unchanged.

**The mechanism.** `Orchestrator::next` drives: it takes the turn, spawns a tick on a `JoinSet`, and returns as soon as the tick either ends (its report) or hands the turn back to run a session (`TickReport::Started`); the tick goes on in the background and reports when it ends. When a fresh tick is idle while ticks still run, `next` waits for the first running tick to end, a command, a filed request, a stop, or the one-minute recheck, and ticks again. It answers `None` once `stop` was called and no tick runs. A per-process registry, `Work`, holds each agent with a session and the task it is about: a rule passes over an agent in it as it passes over a sleeping one (`unavailable`, formerly `asleep`), and every rule passes over a task in it. Starting a session for an agent already in it is refused (`agent_busy`), which no rule should ever reach.

**The order an idle agent takes its work in** is the order of the rules, unchanged: its chat, a mention, a review it owes (rule 5), its own work (rules 3 and 6: a sent-back task returned to it and resumed), a task assigned to it (rule 7), and only then new work (rule 8's assignment). Within a rule, tasks are taken in the board's order, among those CTV-9's `within_limit` lets through.

**WIP: a task under review does not hold its assignee; a sent-back one does.** The statuses that count towards an agent's WIP limit are `assigned`, `in_progress` and `rejected` (and, from CTV-9, not while the row waits on the human). `verifying` does not count: the reviewer works on it, not the assignee. `rejected` counts from the moment of the rejection, so a Developer whose first task comes back while it works on the second is given no third: when the second goes to review, the first is the one within its limit, and rule 3 then rule 6 return it and resume it. A sent-back task never interrupts the session under way, since its assignee is busy until that session ends. CTV-9's `resume_elsewhere` does not hand a `rejected` task to another agent: the rework is its author's, who knows the work and the reviewer's note. The private folder's hold (SPEC 6.6) is unchanged and still counts a task in `verifying`.

**A chat is a fork** (the founder, 2026-10-16, answering the question this ADR first left open: "The agent does not stop his current task, but it forks itself to either answer the user question or create a new task to the backlog. The process of agents answering questions must be quick."). A chat session is the one exception to one session per agent: it starts at once beside the agent's work session, which it does not touch, and it holds no task. An agent has at most one chat fork at a time; messages written while it runs are answered by the next. It stays what a chat is (no contract, the project's root, no worktree, sandbox, executor, connector or browser, the read tier and the chat's tools, the agent's model at low effort), with any chat tool ADR 0063 adds, and it is bounded to be quick: 100,000 input and 4,000 output tokens, 3 minutes and 12 tool calls, or the team's limits where smaller; the new project's interview keeps the team's limits. Its first message says what the agent is doing now (the task of its running work session, else its oldest held task: id, title, status and its last note), so it can answer about its work. It counts toward the day as a chat did, and a spent day stops it with the others.

**Reviews.** A reviewer reviews while the assignee works on something else. Two reviews owed by one reviewer run one after the other.

**No cap but the team.** The number of sessions at once is bounded by the number of active agents; there is no other limit and no policy key. A team of six may run six sessions, six Claude Code processes and, in sandbox mode, six containers.

**Budgets.** A session is started only when its budgets have room, read under the turn against every cost recorded so far, as before. Costs land when a session reports its usage, at its end. When a session's cost crosses the team's daily budget, Catervas asks every other session running in the process to stop ("the team's daily budget is spent"): each ends `aborted`, its task keeps its status with no escalation, as after the second Ctrl-C, and no session starts until there is room. So the day's hard stop holds at the moment it is crossed, and the overshoot is what the running sessions had used, not what they would go on to use. A task's dollars and sessions concern one task, which never has two sessions at once, so their rule is unchanged. The marketing budget's hard stop (ADR 0042) is not a session's and is unchanged.

**Stop, pause, restart.** `stop` (the first Ctrl-C, `catervas stop`, `project.leave`) starts no new tick and lets every running session end; the driving process then finishes. The second Ctrl-C aborts every running session, as it already loops over all of them. A pause starts no new session but a chat's (SPEC section 3) and lets running sessions end, as it let the one session end before. Recovery on a restart already records every session that started and never ended as `aborted` (SPEC 5.15), however many there were.

**What a driving process prints.** `catervas run`, `serve`, `plan` and `contract new` print a line when a session starts (`CTV-3: dev-a started its implement session`, or `dev-a: started its conversation session` about no task; JSON `{"started": "implement", "agent_id": "dev-a", "task_id": "CTV-3"}`), and the tick's own line when it ends, as today.

## Alternatives rejected

- **A session pool with every rule rewritten to hand back a future instead of awaiting `run_session`.** Twenty-two call sites change shape, and the code after each session (record the review, reject, leave a note) would move out of the rule that owns it. The turn keeps every rule as it is.
- **Run every tick concurrently, with locks where they are needed.** Rules read the board and act on it; two ticks deciding at once would need a lock per decision the governor does not have. The turn gives the same answer the code relies on today.
- **A cap on concurrent sessions below the team's size**, as a constant or a policy key. The founder asked for every agent to work; a laptop that cannot run six containers is a reason to have fewer agents, which the user already chooses.
- **Let running sessions finish when the day is spent.** The overshoot would grow with the team: up to a whole session per agent past a limit the user set as a stop.
- **Count `rejected` only once its rework starts.** A Developer would then take a third task while its first waits to be fixed, which is the opposite of the founder's "then he may go back and fix the first task".
- **Hand a sent-back task to another free Developer.** Faster in a team of several, but the author knows the work and the note; CTV-9's hand-over stays for blocked, escalated and waiting tasks.

## Consequences

- With a team of a Product Manager, an Architect and a Developer, the Architect's review of one task and the Developer's work on the next overlap, as the founder described.
- Concurrent sessions cost at once: a day's budget is spent sooner in wall-clock time, and the daily stop now cuts running sessions short; an aborted session's unrecorded usage is the residual, as for any aborted session.
- A chat is answered while its agent works, within minutes at most; before this change it waited for whichever session the whole team was running. Today a fork can answer and propose a request the owner sends; it files no task itself unless ADR 0063 gives the chat such a tool.
- A tick busy outside a session (Catervas's own criteria, an integration) still holds the turn, so no other session starts during it; running sessions are unaffected.
- Recorded-transcript tests that drive whole runs consume their transcripts in the order sessions start, which can now differ from the order they ended.
- Plans: `docs/plans/standalone/parallel-agents/`. SPEC revision 0.118 records it.
