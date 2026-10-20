# Governor flexibility, plan 4: the record

Status: ready
Branch: `fix/governor-flexibility`, after plans 1 and 2, with plan 3 merged in

## Task 20: the spec says it

Files: `docs/SPEC.md`
Writes revision 0.134 (2026-10-20, ADR 0079) at the end of the revision paragraph, and changes, each in the section that holds the rule now:
- 5.2: the `Ready → Refining` governor row (gate `dependency_cancelled`); the Product Manager's writes of `dependencies` and `parent` before assignment; a send-back to refining is refined before it is judged.
- 5.5: work sessions are `refine`, `explore`, `implement`, `verify`; the budget rule leaves `ready`, `assigned`, `blocked` alone; the escalation says what was spent.
- 5.7: an answer needs words; the branch is brought up to date before each build session; a channel message naming the task and its assignee answers its block.
- 5.9: a message to no one goes to the Product Manager.
- 5.14: a plan task opens its own pull request; a new task starts with the accepted documents; a task's failed integration is said in the channel; read-only git in exec.
- 8.6: `catervas open` and `POST /connect/code`.
- 8.5: the gate `dependency_cancelled`.
No test: the spec is prose (hard rule 8 is the check).

- [x] `docs(spec): the governor gets out of the way, revision 0.134 (ADR 0079)`
