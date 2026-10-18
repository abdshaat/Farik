# 0068. Catervas puts back what tools changed outside a task, and a blocked task waits on the owner

Date: 2026-10-17
Status: accepted (the controller, 2026-10-17, after two stalls found in a live Catervafication run on a Next.js project)
Amended 2026-10-17 by ADR 0069: the tidy also runs before each implement session and never during a merge (decision 7); a blocked task's choices go back where the block came from and offer "Ask {pm} to change the plan" (item 2; decisions 1 and 2).

Amends:
- **SPEC 5.2**'s `in_progress -> verifying` row: the clean-worktree rule is unchanged, and is judged after Catervas tidies.
- **ADR 0062**'s choices: a task its agent blocked has choices too, though no escalation is open.

## Context

Two stalls stopped three of eight Catervafication tasks.

1. `next dev` (the Designer's preview) and `npm run build` (the Developer's commands in the sandbox) rewrite tracked files such as `next-env.d.ts` and `tsconfig.json` in the task's worktree. The move to `verifying` asks for a clean worktree, and `git status` counts those files. They are outside the task's `allowed_paths`, the agent's file tools cannot write there, and the sandbox has no git, so the agent could not restore them. It blocked the task and asked the owner, and could not carry out the owner's "undo the changes" answer either.
2. A task an agent moved to `blocked` was not shown to the owner. Today's "Waiting on you" listed only escalations, the task page said "Stuck" with no reason and offered only Cancel, and `catervas resolve` refused it with `not_escalated`. Only the 24-hour `blocker_age` escalation would have shown it.

## Decision

1. **Catervas puts back what tools changed outside a task's paths.** Before a move into `verifying` is judged, Catervas reads the task's worktree on the host, where it has git (`git status --porcelain=v1 -z --untracked-files=all`). A tracked file changed, deleted or retyped (staged or not) that matches none of the contract's `allowed_paths` is restored from the task branch's last commit (`git restore --source=HEAD --staged --worktree`). An untracked file outside the paths that git does not ignore is deleted. The clean-worktree rule counts untracked files, so leaving them would leave the stall. Neither the agent's tools nor its commits can reach those paths, so nothing there can be the task's work. Catervas then records `worktree.tidied { put_back, removed }` on the task, so that the reviewer and the owner see it in the task's history. A change inside the allowed paths is the agent's own uncommitted work: Catervas never touches it, and the rule still refuses the move. Added, renamed, copied and conflicted paths are left alone. A task in a private folder, a task with no worktree, and a task whose member repository is gone have nothing to tidy. The paths decision is `catervas-core`'s pure `check_allowed_paths`. The git work is in `catervas-store` (`Git::uncommitted_in`, `Git::put_back`), called by the runtime's transitions door. The move into `verifying` is the only move whose gate asks for a clean worktree.
2. **A blocked task waits on the owner.** Every task in `blocked` is listed in "Waiting on you" as a `help` row with the reason `blocked`. Its line is "{agent} is stuck: {the blocker's description}". Its choices (`escalation.choices`) are "Answer and carry on" (to `in_progress`) and "Cancel the task". `escalation_resolve`, and with it `catervas resolve`, accepts a blocked task as it accepts an escalation, with no `extra_tries` or `extra_room`. The owner's message becomes the move's blocker resolution, and `escalation.resolved` carries it to the next session as for any escalation. The task page of a blocked task shows the blocker's words, with "Answer and carry on" opening the help page. Once the blocker ages into an escalation, the task is listed once, as that escalation.

## Consequences

- A preview or a build that rewrites settings files no longer stalls a task, and what Catervas put back is in the task's history.
- A file an agent deliberately left outside its paths, which it could never commit, is lost at the move. Nothing the task delivers is lost.
- The owner sees a blocked task at once and answers it with one press, and the 24-hour escalation is no longer the only way the owner hears of it.
- One event kind is added, `worktree.tidied`.
