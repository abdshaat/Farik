# 0074. The owner may keep the team's documents on this computer, out of git

Date: 2026-10-19
Status: accepted (the founder, 2026-10-19, quoted in `docs/design/local-documents.md`; the mechanism decided by the planner)

Amends:
- **ADR 0051** (each role owns a documentation folder): the folders stay `docs/catervas/<folder>/`, and the owner may choose that they are not committed. In that mode "every document is in git" and "a save commits at once as the owner's change" no longer hold: documents are written on disk, a task's documents reach the folder when it is accepted, and staleness and the last change are read from the log, not from git.
- **ADR 0061** (Catervas never merges; only code is reviewed): unchanged in spirit. The switch itself goes through a pull request the owner merges; a task that changes only documents opens none, as a private-folder task opens none.
- **SPEC 4.1, 4.5, 5.2, 5.4, 5.12, 5.14, 5.17, 8.5** (revision 0.129).

## Context

The founder asked: "I would like to give the user the option to choose not to push documents into the repo and keep all catervas documents local to his machine." Asked where they live, the founder chose "Inside the repo, ignored by git" over a private folder with its own git history, accepting that local documents have no history and that a task's document changes are in no pull request.

Facts that shaped the mechanism (code read and git tried on 2026-10-19):
- Every reader of `docs/catervas/` reads git objects on the integration branch or a task's branch; nothing reads the root checkout's files.
- A task's session runs in its own worktree; the hook resolves links and refuses a write outside it; the Docker sandbox mounts only the worktree; `artifact` criteria `cat` files inside it.
- `.git/info/exclude` lives in git's common directory, so it applies to every worktree.
- A fast-forward to a commit that untracks a file deletes the unmodified file from the working tree, ignored or not; a fast-forward that adds a file at an ignored path overwrites it silently.
- Ripgrep, and so Claude Code's Grep, skips ignored files.

## Decision

1. **A policy, and a command that moves the documents.** `policy.documents_local` (boolean, absent false) is the owner's choice, changed only by `documents.set_local { local }` (and by setup's `team.start`, which runs the same command); `team.save` carries the saved value over. The default stays committed. A project folder has no choice (`project_folder`): its documents never leave the computer.
2. **Ignored through `info/exclude`, never `.gitignore`.** Two lines, a comment and `/docs/catervas`, added on and removed off. Nothing about the choice is committed except the move itself.
3. **The move is one folder change each way, and nothing is written while it waits.** Taking out removes `docs/catervas/` from the integration branch in a scratch worktree; putting back adds the files on disk with `git add -f`. Each is a folder change (5.14) recorded as `document_storage.changed`, opened as a pull request, or waiting for a merge by hand with no `origin`. In the modes `taking_out` and `putting_back` no document is written (`documents_switching`) and no task that changes documents is assigned. When the take-out lands, Catervas restores every file git deleted from the change's parent commit; when the put-back lands, it removes the exclude lines. A closed pull request puts the choice back. The switch is refused while a task that changes documents is in flight or a folder change waits. Rejected: staging the removal in the owner's own index (dirty for the whole wait, lost to `git reset --hard`, refused by a non-fast-forward merge); guarding only Catervas's own fast-forward (the owner's own pull or merge is not covered).
4. **One home, and tasks work on a copy.** The root checkout's `docs/catervas/` is where documents live. A task's worktree gets a copy at its start (and a start copy under `.catervas/local/documents/start/`); its document changes are what differs from the start copy, held by the Definition of Done to `allowed_paths` and `pair_changed_alone`; on acceptance they land in the root, a document someone else changed meanwhile set aside rather than overwritten. A task whose paths all lie in `docs/catervas/` has no branch, needs no commit, and its acceptance has nothing to integrate. Rejected: a link in each worktree (refused by the hook, dangling in the container, unread by criteria); a second write root in the hook (a wider boundary, half-written documents left by a failed task, no per-task change to check); private-folder-style tasks with no worktree (the Designer's bug hunt needs its preview); a Docker bind mount (no-sandbox mode has none).
5. **The log replaces git's history.** `local_documents.written { paths, removed, by, written_by, task_id, set_aside }` records every write in local mode: the owner's save, an agent's `catervas_write_folder_doc`, a task's landing. The Files page's last change and a twin's staleness are read from it; the owner's save of a human document files the re-derive at once. An edit made outside Catervas is not seen by either.
6. **Agents are told.** Git's ignore rules hide the folder from Grep and Glob, so the Team rules say to find documents with LS and read them with Read, and that they are never committed. Catervas writes no ignore override into the owner's folder.

## Consequences

- Positive: the owner can keep everything the team writes off GitHub; documents change at once, with no pull request for each.
- Negative: no history and no review of document changes; an edit made in an editor is invisible to staleness and to the "Changed by" line; two writers of one document at once leave one version set aside for the owner to compare; the switch costs one merge each way, during which documents cannot change.
- Neutral: the committed mode, the default, is untouched; two event kinds are added, 124 in all; a project folder and solo mode are unchanged.
