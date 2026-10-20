# Catervas merges, when the owner allows it

Status: decided by the planner (Opus 5.5), 2026-10-19, on the founder's brief; ADR 0073; SPEC revision 0.128. It amends ADR 0061's "Catervas never merges" and SPEC 5.12 and 5.14 (0.115, 0.122).

## The brief

The founder, verbatim, 2026-10-19:
1. "Catervas merge. I want to give the user the option to allow Catervas to merge with the production branch warn the user before turning it on."
2. Asked how Catervas should merge: "Merge the PR when checks pass".

So Catervas still opens the pull request, waits for GitHub's checks to pass, then merges it itself. A failing check stops the merge.

## What the owner sees

**Settings → How work runs → "How finished work is added"** gains one switch under the lead line that is already there:

- Label: **Let Catervas merge finished work**
- Info button (ⓘ), three bullets:
  - "Catervas merges a pull request once its checks pass, or once its own checks pass when your repository has none."
  - "It uses a merge method your repository allows, and never skips a rule of the branch."
  - "It asks only on this computer. Anyone else who clones the project turns it on for themselves."
- Offline (no `origin`; the page already knows it as `online`): the switch is disabled and the line under it reads "Needs your project on GitHub."

**Turning it on** opens a dialog (`Dialog` from `packages/ui`) before anything is saved:

- Title: **Let Catervas merge?**
- Bullets:
  - "Catervas will merge finished work into {branch} by itself."
  - "It merges only when GitHub's checks pass. If your repository has no automatic checks, Catervas merges once its own checks pass."
  - "You won't see the work before it lands."
  - One agent alone (`policy.solo`) adds: "{name} works alone, so no other agent reviews its code."
- Info button (ⓘ) in the dialog, bullets:
  - "A failing check, a needed review or a conflict stops the merge and tells you on Today."
  - "Catervas never uses an administrator's override."
  - "Turn it off here at any time."
- Buttons: **Turn on** (the primary) and **Cancel**.

`{branch}` is the team's `integration_branch`, else "your main branch". Cancel closes the dialog and nothing changes. Turning the switch off saves at once, with no dialog.

**Today**, while merging is on, an accepted task's row whose pull request is open reads "Accepted: Catervas merges its pull request when GitHub's checks pass", and a folder change's row reads "Catervas merges its pull request when GitHub's checks pass". When a merge stops, the task's row is the integration escalation Today already lists, in the words below.

The Settings effect line (`describe_change`) is "Catervas merges finished work once GitHub's checks pass." when it turns on and "Finished work opens a pull request for you to merge." when it turns off.

## The permission

Two things must both hold before Catervas merges:

1. **The team file says so:** `policy.merge_when_checks_pass: true` in `team.yaml`. Absent means false.
2. **This computer's owner saw the warning:** the last of `merge_permission.granted` and `merge_permission.withdrawn` in this project's log is `granted`.

The second rule exists because `team.yaml` is committed and travels with the repository, while the log stays on one computer (SPEC 8). A clone, a `catervas carry` or a hand edit that brings `true` therefore merges nothing until someone on that computer confirms the warning. An agent that edits the file cannot write the event either.

- The new key is not `integration: auto_merge`. Since 0.115 that value loads and is read as `pull_request`; if it came back to life, older team files would start merging without a warning.
- Templates do not carry the key (`team-template.schema.json` is unchanged), and `carried_team` drops it.
- `team.save` and `team.start` never change it: `checked()` copies the value already saved over whatever the incoming team says (`None` at setup).
- One RPC alone changes it: `merging.set { on, confirmed }`.
  - Turning on is refused `merge_warning_unconfirmed` unless `confirmed` is `true`, and refused `merging_offline` when the project is not online.
  - A turn-on writes the key, records `team.updated`, then `merge_permission.granted { by: "human", into }`, `into` being the resolved integration branch.
  - A turn-off removes the key and records `team.updated`, then `merge_permission.withdrawn { by: "human" }`.
  - A call that changes nothing records nothing.
- `team.get` answers `merging_allowed`, the effective answer (both rules). The switch shows that answer, so a clone whose file says `true` shows the switch off until its owner confirms.
- There is no command-line switch. The owner uses Settings.

## The mechanism

Tick rule 2 (`through_the_forge`) already reads each open pull request every tick. Since 0.128 it reads more of it in the same call: `gh pr view <url> --json state,mergeCommit,mergeable,mergeStateStatus,headRefOid,statusCheckRollup`.

When merging is allowed and the tick is Catervas's own (not the human's `catervas integrate`, which never merges), the routine works through these cases in order:

| The pull request | What Catervas does |
|---|---|
| `mergeable: CONFLICTING` or `mergeStateStatus: DIRTY` | As today (0.122): a task's branch is brought up to date or sent back conflicted; a folder change escalates. |
| A check failed | An integration escalation: "A check failed on GitHub: {names}. Catervas did not merge {url}; fix it or merge it yourself." Ticks then leave the task to the human (5.14). |
| Checks pending | Waits for the next tick. |
| No checks at all | Catervas merges as below. The task is accepted, so Catervas's own verification passed (changed 2026-10-19 on the founder's behalf). There is no wait: a repository with no checks has nothing to register. |
| Checks passed, `BEHIND` | The 0.122 refresh: Catervas merges the integration branch into the task's branch and pushes, and the checks run again. A folder change escalates. |
| Checks passed, `BLOCKED` | An integration escalation: "GitHub will not merge {url} yet: it needs a review or another of the branch's rules. Merge it yourself once it can." |
| Checks passed, `CLEAN`, `HAS_HOOKS` or `UNSTABLE` | Catervas merges: see below. |
| `UNKNOWN`, `DRAFT` | Waits. |

A check passes when it is a check run that completed `SUCCESS`, `NEUTRAL` or `SKIPPED`, or a status that is `SUCCESS`. It is pending when a check run has not completed, or a status is `PENDING` or `EXPECTED`. Any other ending fails it.

**The merge.**
1. Catervas reads the methods the repository allows with `gh api repos/{owner}/{repo}` (`allow_merge_commit`, `allow_squash_merge`, `allow_rebase_merge`; a field that is missing counts as allowed). It takes the first allowed one of merge commit, squash, rebase. When none is allowed, that is an integration escalation: "{repo} allows no merge method."
2. It runs `gh pr merge <url> --merge|--squash|--rebase --match-head-commit <headRefOid>`.
   - It never passes `--admin` or `--auto`, and it does not delete the branch, which the repository's own setting may do.
   - `--match-head-commit` means a push made after the checks were read is never merged unchecked.
3. On success it records `pull_request.merged { url, method }` (about no task; its `url` names the pull request), then reads the pull request again. When it is merged, Catervas records `task.integrated { sha, into, integrated_by: "governor" }` (or `folder_change.integrated` with `governor`) and fast-forwards the local integration branch from `origin`, as the existing `Merged` path does. When it is not merged yet (a merge queue), the next tick finds it `Merged`.
4. When gh refuses the merge, that is an integration escalation in gh's words.

On the forge `Merged` path, `integrated_by` is `governor` when the log holds a `pull_request.merged` with that `url`, and `human` otherwise. GitHub itself shows the owner as the one who merged, since `gh` acts with the owner's sign-in.

## Edge cases

- **No GitHub (no `origin`):** the switch is unavailable and the daemon refuses to turn merging on. If `origin` goes away later, the hand-merge path is unchanged and nothing merges.
- **A project of several repositories:** one permission covers every member that has an `origin`. Each member's pull request is read and merged with that member's `gh` (`IntegrationTarget.forge`), against that member's checks and methods.
- **Folder changes:** merged by the same rule. Otherwise documents would still wait on the owner while code did not.
- **An epic's plan task:** no pull request, so nothing changes.
- **Private-folder tasks:** nothing to integrate, so nothing changes.
- **One agent alone:** allowed. The warning says that no other agent reviews the code.
- **Escalated work:** stays the human's. Ticks pass over it (5.14), and `catervas integrate` records a merge but never makes one.
- **Turning it off mid-flight:** the next tick reads it off. A `gh pr merge` already sent completes.
- **The branch the work lands in:** the team's integration branch, which the founder calls the production branch. It is the same branch that pull requests target today.

## Rejected

- **Reviving `integration: auto_merge`:** older team files carry it and would start merging silently.
- **GitHub's auto-merge (`gh pr merge --auto`):** GitHub waits only for the checks a branch rule requires. With no rule it merges at once, which is not "when checks pass". It also needs a repository setting.
- **The file alone as the permission:** the file travels with the repository, so a clone would merge without its owner ever seeing the warning.
- **The log alone, with no key:** the founder asked for an option the owner can see in the team's settings, and the file is where the team's other policies live.
- **Sending a task back to its assignee when a check fails** (a new governor row like 0.122's conflict): a flaky or unrelated check could loop the team. It can be added later. For now a failure stops the merge and tells the owner.
- **Never merging when the repository has no checks (and escalating after 10 minutes):** the founder, through the planner, 2026-10-19: with no checks configured, Catervas merges once its own verification passed. A failing or pending check still blocks.
- **Squash first:** tasks start from accepted, unmerged branches (5.14). A merge commit keeps those commits in history, so later pull requests do not carry them twice.
- **An inline "are you sure" line (Account.tsx's pattern):** the founder asked for a warning. A dialog stops the owner before anything is saved.
