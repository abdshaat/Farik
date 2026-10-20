# Documents kept on this computer

Status: decided by the planner (Opus 5.5), 2026-10-19, on the founder's brief; ADR 0074; SPEC revision 0.129. It amends `docs/design/catervas-folders.md` (ADR 0051) for a team that keeps its documents local; a team that does not is unchanged.

## The brief

The founder, verbatim, 2026-10-19:

1. "I would like to give the user the option to choose not to push documents into the repo and keep all catervas documents local to his machine."
2. Asked where they live, a private folder with its own git history or the repository's folder ignored by git: "Inside the repo, ignored by git".

What the founder accepted with the second answer: the documents have no history, and a task's document changes are not in any pull request, so nobody reviews them there.

## What the owner sees

- **Settings → How work runs → Documents** gains a switch under "Write documents as HTML": **"Keep documents on this computer"**, off by default (today's behaviour). Its ⓘ says, in three bullets: "Your team's documents stay in docs/catervas in this folder." "They are not added to your repository or pushed to GitHub." "No history of changes is kept, and pull requests don't show them."
- **Turning it on** opens a dialog, "Keep documents on this computer?", with "Documents already in your repository are taken out in one pull request.", "The files stay in your folder.", "Until you merge it, documents can't be changed.", and the buttons "Keep them here" and "Cancel".
- **Turning it off** opens "Put documents back in your repository?", with "Your documents go into one pull request." and "Until you merge it, documents can't be changed.", "Put them back" and "Cancel".
- While the pull request waits, the switch is disabled and a line under it says "Taking your documents out of the repository. Merge the pull request to finish." (or "Putting your documents back in the repository. …"). Today lists it under "Ready for you to merge" as "Keep documents on this computer" or "Put documents back in your repository".
- **Setup**: the last screen's "Show advanced settings" has the same switch, with no dialog (nothing is committed yet that a dialog would warn about; an existing repository's committed documents are taken out by the same pull request).
- **Files page**, while documents are kept here: Save says "Saving changes the document on this computer." and, once saved, "Saved."; nothing waits on a pull request. The "Changed by …" line names who wrote it last through Catervas (you, an agent, or a task) and when; a document nobody has written since the switch has no line. While documents are moving, no document has "Edit", "Ask for a change" is not shown, and the page says "Documents can't be changed until you merge the pull request that moves them."
- **A project folder** (several repositories, ADR 0057) shows no switch: its documents are in its own local repository, which is never pushed, so they never leave the computer already.
- **One agent alone** (solo, ADR 0071): unchanged; the switch works as for a team.

## The mechanism

### Ignoring without a committed change

Catervas adds two lines to the repository's `info/exclude` (in git's common directory, `git rev-parse --git-common-dir`, so one file covers the root checkout and every task worktree):

```
# Catervas keeps the team's documents on this computer
/docs/catervas
```

and removes exactly those two lines when the switch is turned off. Rejected: a line in `.gitignore`, which is itself a committed change that every clone and every collaborator would carry; `git update-index --skip-worktree`, which keeps the files tracked (and pushed) and breaks every merge that touches them.

### Four modes

The owner's choice is `policy.documents_local` in `team.yaml` (absent is false). The mode Catervas acts on also reads the newest switch in the log (`document_storage.changed`) and whether its folder change has landed:

| Mode | When | Documents are read from | Who may write them |
|---|---|---|---|
| `committed` | off, nothing moving | the integration branch (today) | as today: folder changes and tasks' branches |
| `taking_out` | on, its pull request not merged | the integration branch | nobody |
| `local` | on, and the documents are out of the repository (or never were in it) | the root checkout's `docs/catervas/` on disk | writes go straight to that folder |
| `putting_back` | off, its pull request not merged | the root checkout's disk | nobody |

"Nobody" while moving is what keeps the switch safe: no write can race git putting files in or taking them out (below). In those two modes `files.save` and `catervas_write_folder_doc` are refused `documents_switching`, and a task whose `allowed_paths` reach `docs/catervas/` is not assigned (rule 7 passes over it, as it passes over a task whose member repository is missing). A switch's pull request closed without merging (that escalation alone; a failed push or a fast-forward the local branch cannot follow leaves the mode for the human's `catervas integrate`) puts the owner's choice back: Catervas writes `documents_local` as it was, removes the exclude lines it added (taking out) or keeps them (putting back), and says so in the channel. Tasks held by the move are said once in the channel: "<n> tasks wait for the pull request that moves your documents."

### Taking documents out, and why the files survive it

Git deletes a tracked file from the working tree when a checkout or a fast-forward moves to a commit that no longer tracks it, ignored or not (tried on 2026-10-19: the untracking commit, fast-forwarded into the checkout, removed `docs/catervas/`). Git deletes only an unmodified file: a modified one stops the merge. Taking out therefore works in three steps:

1. **Turning on** (`documents.set_local { local: true }`), under the integration lock: the exclude lines are written; when the integration branch tracks anything under `docs/catervas/`, Catervas makes one folder change in a scratch worktree (`.catervas/local/worktrees/documents-<n>`, never the root checkout): `git rm -r -q -- docs/catervas` from the integration branch, committed as `docs: keep the team's documents on this computer`, on the branch `docs/keep-documents-on-this-computer`, recorded as `document_storage.changed { local: true, change, branch, sha }`. The mode is `taking_out`; the change opens its pull request on the next tick like any folder change (with no `origin`, it waits for a merge by hand). With nothing tracked (a new project, or a repository whose team never wrote a document), the event has no change and the mode is `local` at once.
2. **While it waits**, nobody writes a document, so every file on disk is the committed version.
3. **When it lands** (`folder_change.integrated`, whether Catervas fast-forwarded the integration branch or the owner merged and pulled by hand), git has deleted the files from the root checkout. Catervas then writes back every file the change removed that is missing on disk, from the change's parent commit (`<sha>^`), which is exactly what git deleted. They are now untracked, ignored, and `git status` is clean. The mode is `local`.

Rejected: staging `git rm --cached` in the owner's own index until the merge (tried: it keeps the files through a fast-forward, but leaves the owner's index dirty for the whole wait, is lost to a `git reset --hard`, and a non-fast-forward merge refuses over it); moving the folder aside around Catervas's own fast-forward (it does not cover the owner's own pull or merge by hand, which the restore from `<sha>^` does).

### Putting them back

**Turning off** (`documents.set_local { local: false }`) copies every regular file under the root's `docs/catervas/` into a scratch worktree made from the integration branch, `git add -f -A -- docs/catervas` (forced, the path being still ignored), committed as `docs: put the team's documents back in the repository` on `docs/put-documents-back-in-the-repository`, recorded as `document_storage.changed { local: false, change, branch, sha }`; the mode is `putting_back`, the exclude lines stay. When it lands, the fast-forward writes the same bytes over the ignored files (tried: git overwrites an ignored file silently, which is harmless here because nobody wrote one while it waited), Catervas removes the exclude lines, and the documents are tracked again: `committed`. With nothing on disk the switch is immediate.

### Refusals of the switch

`documents.set_local` refuses, in this order, at `/local`: `project_folder`; `documents_switching` (a switch is already moving); `documents_in_flight` (a task whose `allowed_paths` reach `docs/catervas/` has a worktree or awaits integration: "Finish or merge the work that changes documents first: <titles>."); `change_waiting` (a folder change is neither integrated nor escalated: "A document change waits for you to merge it. Merge it first."); `documents_busy` (turning on, the root checkout has an uncommitted change under `docs/catervas/`: "Some documents have changes on your computer that aren't committed. Commit or undo them first."); `is_a_link` (`docs` or `docs/catervas` is a symbolic link). The same value as now answers the current mode and changes nothing.

### One place, and how tasks reach it: a copy in, a copy back

The documents' one home is the root checkout's `docs/catervas/`. The Files page, `catervas_write_folder_doc`, every session that is not about a task (chats, ceremonies, whose read root is the project) and every reader of documents read and write it there.

A task works in its own worktree (its sandbox mounts that worktree alone, its hook refuses a write whose real path leaves it, and its `artifact` criteria `cat` files inside it). So a task's documents are a copy:

- **Copy in.** When Catervas makes a task's worktree (rule 7, after every start merge; the conflict refresh, after its merge; an epic's detached worktree for its criteria), it copies every regular file of the root's `docs/catervas/`, links not followed, into the worktree's `docs/catervas/`, after marking any path git still tracks there `--skip-worktree` (a dependency's branch made before the switch may still track old copies), and keeps a second copy as the task's start, `.catervas/local/documents/start/<task-id>/`, once (a task sent back keeps its first).
- **Work.** The agent reads and writes the copy with its ordinary tools; its `artifact` criteria read it; its preview and browser see it. `catervas_git_commit` refuses a path under `docs/catervas/` (`documents_stay_local`), since it is never committed.
- **What changed.** The task's document changes are the files of its copy that are new to, differ from, or are gone from its start copy, as repository paths. They join the task's changed paths, so the Definition of Done holds them to `allowed_paths` and to `pair_changed_alone` as it holds a branch's; the tidy before `verifying` puts back from the start copy a document changed outside `allowed_paths` and removes one added there.
- **Copy back, on acceptance.** Before Catervas removes an accepted task's worktree it lands each changed document in the root: a file whose root copy still equals the start copy is written (or removed); one whose root copy already equals the task's is left (so a landing tried twice does nothing twice); one whose root copy changed meanwhile (the owner's save, another task, a ceremony's write) is **set aside**, the root's kept, the task's version copied to `.catervas/local/documents/set-aside/<task-id>/<path>`, and the channel says "<document> changed on this computer while <agent> worked on it, so <agent>'s version was not put in. It is in .catervas/local/documents/set-aside/<task-id>/.". A landed `<stem>.html` that its start copy did not have removes `<stem>.md` and its agent version (the format rule, 5.17), when their root copy is the start copy's. One `local_documents.written { paths, removed, by: "task", task_id, set_aside }` records it. A cancelled task lands nothing. The start copy is removed with the worktree.
- **A documents-only task**, every `allowed_paths` entry within `docs/catervas/` (the Product Manager's, the Architect's plans and documents, the re-derive, a rewrite, Catervafication's documents, the bug hunts' reports), has nothing to commit: its worktree is made detached, with no branch; it reaches `verifying` with no commit once it changed a document ("the task changed no document in docs/catervas/" otherwise); its acceptance carries `nothing_to_integrate`, as a private-folder task's does, so it opens no pull request, counts as integrated for its dependants, and an epic's plan task is one of them (its plan is in the root once accepted, so the epic's tasks copy it in). A mixed task (code and documents) keeps its branch and pull request for the code, and its documents land at acceptance, before the code is merged.

Rejected: **a link** from each worktree to the root's folder (the hook resolves links and refuses a write outside the worktree, the container mounts only the worktree so the link dangles inside it, and an `artifact` criterion run there cannot read through it); **a second write root in the hook** for `<root>/docs/catervas/` (it would change the security boundary, a cancelled or failed task would leave its half-written documents in place, and a task would have no per-task change to hold to `allowed_paths` or `pair_changed_alone`); **running documents tasks in the folder with no worktree**, as private-folder tasks run (the UI/UX Designer's bug hunt needs its preview and browser, which need a worktree, and a task mixing code and documents would still need a second mechanism); **a bind mount** of the root's folder over the worktree's in Docker (no-sandbox mode has no mounts, and the host-side readers of the worktree would still see the empty folder).

### No history: the log answers what git answered

- **Last change.** The newest `local_documents.written` naming the path: `by: "owner"` is "you", `"agent"` its `written_by`, `"task"` the task and its assignee; its time is the event's. A document no such event names has no line.
- **Staleness.** A twin is stale when the newest `local_documents.written` naming its human document comes after the newest naming the twin (a pair landed together is one event, so it is never stale). The owner's save of a human document files the re-derive at once (there is no merge to wait for), under the rule that already keeps one re-derive open per twin.
- **What it cannot see.** An edit made outside Catervas, in an editor, is invisible to both: no last change, no staleness. The design says so; the Files page reads the bytes on disk all the same.

### Agents finding the documents

Git's ignore rules are also ripgrep's, so Claude Code's Grep (and Glob, where it uses ripgrep) skips `docs/catervas/` in the root and in worktrees alike (tried on 2026-10-19 in a detached worktree). Catervas writes no ignore override into the owner's folder. Instead the Team rules carry one more line in `local` and `putting_back`: "- documents kept on this computer: docs/catervas/ is not in git. Grep and Glob skip it: find its files with LS and read them with Read. Never commit them; catervas_git_commit refuses them. Your changes to them reach the team when your task is accepted."

## Edge cases

| Case | What happens |
|---|---|
| Documents already committed, turned on | One folder change takes them out; the files stay on disk through its merge (restored from `<sha>^` when git removes them). Their history stays in git, as before. |
| The owner's `git reset --hard` while taking out | Nothing is lost: nothing was written since the switch. |
| Turned off | One folder change puts back what is on disk; while it waits nothing is written, so the merge brings the same bytes. |
| The switch's pull request is closed | The choice goes back to what it was; the channel says so. |
| A project with no `origin` | Each switch's change waits for a merge by hand, as every folder change does (5.14). |
| A task's document changed meanwhile by someone else | Set aside, root kept, channel line. |
| The owner edits a document in an editor | Read from disk as is; no "Changed by" line and no staleness from it. |
| Marketing plans | Proposed in the task's copy (the tool already numbers from the root and the worktree), landed with the task. |
| `files.rewrite` | Unchanged; a rewrite task is documents-only and its `.html` removes the `.md` on landing. |
| A project folder | No switch; `documents.set_local` refuses `project_folder`. |
| Solo | Unchanged. |
| The default | `committed`: nothing of the above runs. |

## Out of scope

A history of local documents (the founder chose none), a backup of them (the owner's own backup of the folder covers it), moving them outside the repository folder (rejected by the founder), and per-folder choices (all of `docs/catervas/` moves together).
