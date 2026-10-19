# 0073. Catervas merges a pull request when its checks pass, if the owner allows it

Date: 2026-10-19
Status: accepted (the founder, 2026-10-19, quoted in `docs/design/catervas-merges.md`; the mechanism decided by the planner)

Amends:
- **ADR 0061**, its "Catervas never merges". It still holds by default. A team whose owner turns on "Let Catervas merge finished work", and confirms the warning on this computer, has its pull requests merged by Catervas once GitHub's checks pass. Every other part of ADR 0061 stands. Catervas still opens a pull request for each accepted task and folder change, and `integration` still reads as `pull_request`.
- **SPEC 5.12 and 5.14** (revision 0.128).

## Context

The founder: "I want to give the user the option to allow Catervas to merge with the production branch warn the user before turning it on." Asked how: "Merge the PR when checks pass."

Since ADR 0061, the owner's recurring duty is merging pull requests. Today lists each one. For an owner who trusts the team's review and their repository's checks, that duty is the last thing the team waits on.

Four facts constrain the design:
- `team.yaml` is committed and travels with the repository, while the event log stays on one computer (SPEC 8). A permission held only in the file would reach every clone without its owner seeing the warning.
- Older team files still carry `integration: auto_merge`, read as `pull_request` since 0.115. Reviving that value would make them merge silently.
- GitHub's own auto-merge (`gh pr merge --auto`) waits only for the checks a branch rule requires, and with no rule merges at once.
- Tick rule 2 already reads every open pull request on every tick, under the integration lock, in the project and in each member repository.

## Decision

1. **A new policy, off by default.** The policy is `policy.merge_when_checks_pass: boolean` in `team.schema.json`; absent means false. Team templates do not carry it, `catervas carry` drops it, and `team.save` and `team.start` never change it.
2. **The warning is the gate.**
   - Catervas merges only when the file says `true` and the last of `merge_permission.granted` and `merge_permission.withdrawn` in this project's log is `granted`.
   - One daemon method alone changes the policy: `merging.set { on, confirmed }`. Turning on needs `confirmed: true`, which the web app sends only from the warning's **Turn on**, and needs the project online.
   - Turning off needs no warning.
   - Both directions record their event, with `team.updated`.
3. **Catervas merges when the checks pass.**
   - On its own tick, for an open pull request whose checks have all passed and that GitHub reports clean, Catervas runs `gh pr merge` with the first method the repository allows, in the order merge commit, squash, rebase.
   - It passes `--match-head-commit` with the commit it read, never `--admin`, never `--auto`, and it deletes no branch.
   - It records `pull_request.merged { url, method }`, then `task.integrated` (or `folder_change.integrated`) with `integrated_by: governor`, and fast-forwards the local integration branch.
   - The human's `catervas integrate` still never merges.
4. **What stops a merge stops it visibly.** A failed check, no checks after 10 minutes, a branch rule GitHub enforces (`BLOCKED`), a repository with no allowed method, or gh refusing the merge is an integration escalation with its words. Ticks then leave that work to the human, as after any integration escalation. A conflict, or a branch behind its base, goes through 0.122's refresh as today.
5. **Scope.** The rule covers every pull request Catervas opens: accepted tasks, folder changes, and tasks in member repositories, each through its own repository's `gh`. One agent alone may use it; its warning says that no other agent reviews the code. A project with no `origin` cannot turn it on.

Rejected:
- Reviving `auto_merge`: silent merging in older files.
- GitHub's auto-merge: it does not mean "when checks pass".
- The file alone as the permission: clones would merge unwarned.
- The log alone: no visible team setting.
- Sending a task back to its assignee on a failed check: a flaky check could loop the team; possible later.
- Merging with no checks: nothing passed.
- Squash first: tasks start from unmerged branches, and a merge commit keeps their commits from appearing twice.

## Consequences

- An owner who turns it on merges nothing by hand while checks pass. Work reaches the production branch without the owner seeing it first. The warning says so, and it is the price the founder chose to offer.
- The safety of an automatic merge now rests on the repository's checks and the code review by the Architect, else a Developer. With one agent alone, it rests on the checks only.
- GitHub shows the owner as the one who merged, since `gh` acts with the owner's sign-in. Catervas's log says it was Catervas (`pull_request.merged`, `integrated_by: governor`).
- A repository with no checks never merges automatically. The owner learns it from one escalation per pull request, after 10 minutes.
- Three event kinds are added: `merge_permission.granted`, `merge_permission.withdrawn`, and `pull_request.merged`.
