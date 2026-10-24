# 0085. Keep everything on this computer

Date: 2026-10-22
Status: accepted (the founder, 2026-10-22, quoted below; the mechanisms decided by the planner the same day)

The founder: "Include the option to work and keep everything local. Currently we have under /settings/allowed-actions, May Kai send its work in progress online?. I answered No, keep work in progress on this computer, yet it is opening pull requests to remote repo. Can you make this option force the agent to keep everything local and physically block any pull requests."

Asked three questions, the founder answered: the choice is "Third answer", beside the two the question has, so a team that answered "No" keeps its pull requests until the owner picks the new one; finished work "doesnt [go into main]. Everything is local and no git is involved. All agents work together on a local project"; and, asked whether that meant no git at all or no git for the owner, "No git for me": Catervas keeps its branches and worktrees out of sight, nothing goes online, and accepted work lands in the project folder by itself.

Amends ADR 0061 (Catervas only opens pull requests) and ADR 0070's push question (owner-polish item 4, which rejected making "No" stop the pull request: the new answer is a third one, so that rejection holds for "No"). Amends SPEC 5.6 and 5.14.

## Context

The push question sets `policy.permissions.push`, which only gives Developers and Designers the `git_remote` tier for their own branches. Finished work goes online whatever it says: `integrate.rs::open` pushes to `origin` and opens a pull request whenever the repository has one (ADR 0061), and the question's note says so in its last line. The owner read "No" as "nothing goes online".

## Decision

1. **A third integration policy, `local`.** `policy.integration` gains the value `local` (the older `auto_merge` and `manual` still read as `pull_request`). The push question gains a third answer, "No, keep everything on this computer", which writes `push: false` and `integration: local`; "Yes" and "No" write `integration: pull_request`. A team file is the owner's to edit, and no agent writes it (protected path), so this lives in `team.yaml` like the other answers: unlike autopilot (ADR 0082), it makes the team do less outside, never more.
2. **Nothing goes online.** Under `local`:
   - Catervas never pushes, never opens, reads or merges a pull request, and never fetches (`bring_up_to_date` merges the local integration branch only);
   - no agent holds `git_remote`, whatever `push` or its own grants say;
   - `catervas_exec` refuses a pushing git subcommand (`push`, `send-pack`) even when `rules.git_subcommands` or the agent's own `git_subcommands` lists it, with `kept_local: this team keeps everything on this computer`;
   - `team.get` answers `online: false`, so the merge switch (ADR 0073) is refused `merging_offline` and the finish line says where work goes.
   A connector is not touched: none in any kit opens a pull request, the GitHub connectors being read-only or issues-only, and a connector is the owner's own act of connecting a service.
3. **Accepted work lands in the project folder by itself.** An accepted task or folder change under `local` is merged by Catervas into its integration branch with a merge commit (`Git::merge`), in its member for a multi-repo task, and recorded `task.integrated { integrated_by: governor }` (or `folder_change.integrated`). Only when that repository's checkout is on the integration branch: otherwise the subject is escalated, "Your project folder is on {branch}, not {into}. Switch it back to {into} and Catervas adds the finished work.", so Catervas never lands work where the owner does not look. A merge git refuses for another reason (an unsaved change of the owner's in a file the merge touches) is escalated with git's words.
4. **A conflict goes back to the builder.** A task whose branch conflicts with the integration branch is recorded `task.conflicted { into, paths }` and moved back to `assigned` as a pull request's conflict is (5.14), its worktree holding the merge in progress for the agent to resolve; a folder change that conflicts is escalated.
5. **Work already online is left alone.** A task or folder change that opened a pull request before the switch keeps following it, so that work the owner already sees on GitHub is not landed twice; only work accepted after the switch lands locally.
6. **A project with no `origin` is unchanged under "No".** It still waits for the owner's merge by hand. Under `local` it lands by itself as above.

## Consequences

- A local team needs no GitHub, no `gh` and no network for its work; its owner never sees a branch.
- Catervas merges into the integration branch without the owner's merge, for local teams only. The checks it merges on are its own Definition of Done and review, as under autopilot (ADR 0082 decision 11).
- One new event kind, `task.conflicted`; `policy.integration` gains `local` in both team schemas.
- Agents' own residuals stay ADR 0004's: a git call hidden in a script inside the sandbox, which carries no credentials and no `gh`.

## Alternatives rejected

- Making "No" mean everything local: the founder chose a third answer; every ready-made team ships `push: false` and would silently lose its pull requests.
- No git at all (agents editing the project folder directly): the founder chose "No git for me"; parallel agents and reviews need separate worktrees.
- The owner merging by hand under `local`: the founder wants no git for the owner.
- A `pre-push` hook in the owner's repository: it would block the owner's own pushes too, and Catervas does not write the owner's git configuration.
- Landing on another branch than the checked-out one: the owner would not see the work in the folder.
