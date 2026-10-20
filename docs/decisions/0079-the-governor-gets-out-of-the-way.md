# 0079. The governor gets out of the way

Date: 2026-10-20
Status: accepted (the founder, 2026-10-20, quoted below; the mechanisms decided by the planner the same day)

The founder, after a watched run: "Okay now fix all the findings, Make the governer more flexible to stop all those problems from happening." Earlier the same night: "The governer is complaining too much we might need to redesign."

Amends SPEC 5.2 (the transition table and the contract gate), 5.5 (the session budget), 5.7 (answering a block), 5.9 (the channel), 5.14 (integration and start points) and 8.6 (the browser's way in). Leaves to ADR 0078 (pull request #83, open) the env templates (`.env.example` writable) and the idle builder's re-plan; nothing here repeats them.

## Context

A team of six (Mira, PM; Ada, Architect; Theo and Noor, Developers; Ivo, Designer; Kai, Marketing) ran an epic, CTV-1, a video studio, on 2026-10-20 between 02:30 and 04:10 UTC. Every finding below is from that run's event log, channel and git.

- CTV-11 went blocked → answered → blocked about ten times. Four answers said the file the developer needed was on `main` ("I pushed it", "its on the main branch", "I committed it"). The task branch never saw `main`: nothing merges `main` into a task branch while it is worked on, and an agent cannot run git.
- An empty answer ("Answer and carry on" with no note) restarted the agent with nothing new; it blocked again for the price of a session (03:17:51, 03:41).
- "Ask Mira to change the plan" moved CTV-11 blocked → refining, and the governor moved it refining → ready in the same second (03:16:11 → 03:16:12, 03:19:01 → 03:19:01) without a Product Manager session, because an epic's child is "filed whole". Mira then wrote: "it's ready and locked, so I can't change them. I've asked the owner to send it back to refining."
- CTV-11 was escalated for spent sessions at 03:00:23, 03:14:19 and 03:21:16, the last while `ready`. The sessions that spent it were mostly blocked tries and assignment sessions. The channel showed the reason as the raw `governor_escalation`.
- CTV-11 was cancelled. CTV-12 and CTV-14 depended on it, and every other task of the epic depended on those two. Nothing said so; the ten tasks sat in To do, passed over on every tick.
- The epic's plan, CTV-8, was accepted at 02:31:01 and never reached `main`: a plan task waits for a child's pull request to carry its file, and its only started child was cancelled. CTV-22, the recreated CTV-11, had no epic parent (a parent is fixed at creation), started from `main`, and had no plan. Theo blocked; then Ada blocked the review for the same reason. The Files page said the plan "waits to merge".
- Ada wrote that plan without the architecture and conventions documents (CTV-5, CTV-6), accepted but not merged: a task starts from `main` and its own dependencies only.
- The owner's "Recreate CTV-11" (03:26:01) was answered by nobody: a channel message without `@` wakes no agent. Repeated as "@Mira Recreate CTV-11" it worked.
- An `@theo` message carrying the plan text was answered by Theo in a chat session that cannot move his task: "This session was only for answering the channel, so I haven't started the build."
- A developer could not run a contract's own criterion (`git ls-files`, `git grep`): `catervas_exec` refuses every git command.
- A failed integration of a task raises an escalation with no channel line.
- A new browser cannot get in while `catervas serve` runs: the start link works once and is printed once.

## Decision

The governor keeps every guarantee it gives (protected paths, budgets, the review, one writer per lane). It stops asking the owner for things the team can settle, and it stops charging for its own friction.

1. **A task's branch is brought up to date before each build session.** Before rule 6 starts an `implement` session on a task whose worktree exists, the governor fetches the integration branch and, when the task branch lacks commits of it, merges it in the worktree with the message `chore: bring up to date with <branch>`. A merge that conflicts is aborted; the session starts as before and its prompt says the branch is behind and conflicts. No new event: the merge commit is the record.
2. **An answer must say something.** The help page refuses "Answer and carry on" and "Ask <PM> to change the plan" without a note; it no longer sends the button's label as the answer. Cancelling needs no note. The note's label names the agent who raised the block (a reviewer's block goes back to the reviewer).
3. **A send-back to refining always reaches the Product Manager.** A task the human moved into refining is not judged "filed whole" until a refine session has run since that move. The refine prompt carries the human's words.
4. **The Product Manager may repoint a task that has not started.** On a task in `draft`, `refining` or `ready`, not locked, the Product Manager may write `dependencies` and `parent` (to an existing epic); the write leaves the status as it is. `parent` stays fixed for everyone else and after assignment.
5. **A cancelled dependency sends its dependents back to the Product Manager.** When a task is cancelled, every `ready` task that depends on it moves to `refining` (actor governor, gate `dependency_cancelled`), and the channel names them. The readiness check then fails on the cancelled dependency, which opens a refine session (3).
6. **The session budget counts work.** A task's sessions are its `refine`, `implement` and `verify` sessions; triage, assignment (`plan`) and chat sessions no longer count. The budget rule leaves `ready`, `assigned` and `blocked` tasks alone: they start no session, so they spend nothing. An escalation for a spent budget says what was spent ("spent 6 of 6 sessions", "spent $20.40 of $20.00").
7. **A plan task opens its own pull request.** On acceptance it integrates as every documentation task does. Its children still start from its branch while the plan is not on the integration branch.
8. **A task starts with the team's accepted documents.** Besides its dependencies, a new worktree merges every accepted, not yet integrated task of the same repository whose allowed paths all lie under `docs/catervas/`. A documents branch that conflicts is left out, not retried.
9. **Read-only git is not a command the governor refuses.** `catervas_exec` runs `git` when every git call in the command is one of `status`, `diff`, `log`, `show`, `ls-files`, `grep`, `rev-parse`, `blame`; any other git subcommand is refused as before.
10. **A message to nobody goes to the Product Manager.** A human channel message with no `@` is answered by the Product Manager (the one agent in solo), as if it named it.
11. **An answer in the channel unblocks.** A human channel message that mentions the assignee of a task blocked on the human and names that task's id also answers the block, as "Answer and carry on" with the message as the note.
12. **The Product Manager does not ask for `ready`.** Its prompt says: finish the contract and end the session; the governor checks readiness.
13. **A failed integration of a task is said in the channel**, as a folder change's already is.
14. **`catervas open` prints a new start link** from the running driver; the "already used" page tells the owner to run it.

### Amended after the live run (2026-10-20)

The build ran on the founder's project from 06:05 UTC. The plan's pull request opened and merged in four seconds, CTV-12 and CTV-14 went back to refining with "CTV-11 was cancelled", and `doctor` found the old log readable. It also showed four gaps, closed by plan 5:
- Decision 2 asks words only of a choice that answers; giving room or tries needs none.
- Decision 1 also runs before a review session: the reviewer of CTV-22 blocked three times on a plan its branch did not have.
- The help page names the agent who raised the block in its heading and explanation, not only in the note's label.
- The owner's answers stay with the task for its whole try: an answer reached only the next session, so the reviewer lost the plan text the owner had pasted after one rework.

## Alternatives rejected

- **Redesign the governor around fewer gates.** Every refusal in the run had one specific cause; removing gates would give up guarantees (protected paths, the review, lanes) that did not cause it.
- **Merge `main` into the task branch only when the human answers a block.** The same staleness hit the reviewer and the planner; every build session is the moment it matters.
- **Let the conversation session move tasks.** Chat stays read-only; decision 11 makes the human's message the answer, which is the human's move to make.
- **Count sessions per try instead of per task.** "Give it more room" already resets per escalation; the waste was in which sessions counted.
- **Wake every agent on an unmentioned message.** It costs a session per agent per line; the Product Manager decides who else is needed.

## Consequences

- An implement session may start on a merge commit the agent did not make. Its prompt says so.
- A send-back to refining always costs one Product Manager session.
- An unmentioned owner message now costs a chat session.
- A plan's pull request waits for the owner to merge it like any other, unless Catervas may merge (ADR 0073).
