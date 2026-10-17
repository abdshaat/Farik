# Standalone internal-task-ids, plan 6: the record

Status: draft
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md`, the sections below (hard rule 8)
Depends on: plans 1 to 5 of this branch
Readiness confirmed by: the planner, 2026-10-17 (ADR 0065)

## Goal

`docs/SPEC.md` revision 0.119 says what plans 1 to 5 built, in the spec's own voice. Each change is
marked "(added in 0.119; ADR 0066)" or "(changed in 0.119; ADR 0066)", as earlier revisions mark
theirs. The plans' ticks and statuses are brought up to date. Nothing else changes.

## Decisions

- One revision, 0.119, written last, after ADR 0064's 0.118. Its sentences sit after 0.118's text
  in each section, and replace only the sentences named below.
- Old names stay where the spec speaks of tasks started before 0.119 (5.15's example
  `'feature/CTV-1' is already used by worktree at …` among them). The spec says once, in 5.14,
  that such tasks keep them.

## What each section says

- **Header.** A revision line after 0.118's: "Revision 0.119 (2026-10-17) keeps task ids inside
  Catervas (standalone plan `internal-task-ids`, ADR 0066). In 5.14 a task's branch is
  `feat/<slug>`, `fix/<slug>` or `docs/<slug>` from its type and title, chosen when its worktree is
  first made and recorded in its contract's `branch`; its pull request is titled `<Type>: <title>`
  and its title and body name no task id; the start merge says `chore: start from <branch>`;
  `catervas_git_commit` refuses a message naming a task; a folder change's branch is
  `docs/<slug>` of its documents' names, recorded in its event, and its pull request `Docs: Update
  <names>`. In 5.17 the Files page also reads a pull request number. In 8.4 new branches no longer
  move the id counter. `folder_doc.written`, `folder_doc.edited` and `folder_doc.approved` gain
  `branch` (5.11, 5.14, 5.17, 8.4, 8.5)."
- **5.11**: in "The store's are `id`, `created_by`, `created_at`, and `updated_at`", add `branch`:
  "...and `branch` (added in 0.119; ADR 0066), which Catervas writes once when the task's worktree
  is first made (5.14). No actor writes it through a contract tool. A filing that sets it is
  refused. The human's save leaves it as the file holds it, whatever the edited contract says."
- **5.14, first paragraph**: the sentences from "The branch says what the task changes (changed in
  0.11)" to "renaming that branch fixes it" become (changed in 0.119; ADR 0066):
  - A task's change has a type, from its contract alone: Docs when its assignee's role changes no
    code or every one of its `allowed_paths` starts with `docs/`; else Fix when its `change` is
    `fix`; else Feat. A bug a hunt files is filed with `change: fix`.
  - Its branch is `<type in lower case>/<slug>`. The slug is the title in lower-case ASCII, each
    run of anything else one `-`, trimmed of `-`, and cut to 40 characters at the last `-` (at 40
    when there is none); `change` when nothing is left.
  - A name another branch of the repository has (locally or under a remote, or as the start of a
    name followed by `/`), or another contract or folder change records, takes `-2`, `-3`, and so
    on.
  - Catervas chooses the name when the task's worktree is first made and records it in the
    contract's `branch` before the worktree. It is never changed, and every reader of the task's
    branch reads it. Nothing reads a task's id out of a branch made since 0.119.
  - A task started before 0.119 has no `branch`. It keeps `feature/CTV-<n>`, `fix/CTV-<n>` or
    `docs/CTV-<n>` as 0.11 named them.
  - A branch `catervas/CTV-<n>` made before 0.11 is neither renamed nor read.
  - A repository with a branch named exactly `feat`, `fix` or `docs` (or, for an old task,
    `feature`) cannot hold a branch under that name, so git refuses the task's worktree at
    assignment, and the refusal reaches the user as any git failure does; renaming that branch
    fixes it.
  The worktree folder `.catervas/local/worktrees/CTV-<n>` and the ref `refs/catervas/start/<id>`
  keep the id: both are local, and Catervas pushes `refs/heads/<branch>` alone.
- **5.14, the pull request paragraph**: "titled with the task's id and title, whose body is..."
  becomes (changed in 0.119; ADR 0066):
  - The pull request is titled `<Type>: <title>` (`Feat: Add a login page`).
  - Its body is the contract's intent, the completion note and the review note.
  - In the title and the body, each task id that is not part of a path is replaced by that task's
    title in quotes, or "another task" when the board does not know it.
  - Nothing Catervas sends to the forge names a task id.
- **5.14, "Who commits"**: add (added in 0.119; ADR 0066): "`catervas_git_commit` refuses a
  message that names a task id (`commit_message_names_a_task`: '<id> is a task id, which stays
  inside Catervas; describe the change instead'). Its description asks for Conventional Commits.
  The merge Catervas makes when a task starts from several branches says `chore: start from
  <branch>`." In the start paragraph, "with a merge commit `Start <id> from <branch>`" becomes
  "with a merge commit `chore: start from <branch>` (changed in 0.119)".
- **5.14, "Folder changes"**: (changed in 0.119; ADR 0066)
  - "on a branch of its own, `docs/folder-<n>`, which cannot clash with a task's `docs/CTV-<n>`"
    becomes "on a branch of its own, `docs/<slug>`, the slug made from the documents' plain names
    joined by ' and ' as a task's is from its title, free by the same rule, and recorded as
    `branch` in the event that made the change; a change of an older log has none and is
    `docs/folder-<n>`".
  - "Its number `n` is the first above..." stays; add that the number names the change inside
    Catervas alone.
  - In the integration paragraph, "titled `Folder change <n>: <those paths>`" becomes "titled
    `Docs: Update <the documents' plain names>`".
- **5.14, "The owner's save"**: "on `docs/folder-<n>`" becomes "on its own branch (above)".
- **5.17, the Files page's server**: after "the first task id or folder change in subject order
  (`named_in_subject`)", add (added in 0.119; ADR 0066): "and, when it names neither, a pull
  request number: a subject that starts `Merge pull request #<n>` or ends `(#<n>)`. That number
  is read through the newest `pull_request.opened` of a task that names no `repo`, or
  `folder_change.opened`, with that number."
- **8.4**: after "past the highest number any `feature/CTV-<n>`, `fix/CTV-<n>` or `docs/CTV-<n>`
  branch holds", add (changed in 0.119; ADR 0066): "branches made before 0.119, since a branch made
  since names no task. A fresh clone's counter does not see those tasks, and an id it hands out
  again clashes with nothing on the forge, where no id is."
- **8.5**: after the 0.96 line on `folder_doc.edited`, add (added in 0.119; ADR 0066):
  "`folder_doc.written`, `folder_doc.edited` and `folder_doc.approved` carry `branch`, the branch
  the change was made on, absent in an older log, where it is `docs/folder-<change>`."
- **F3 Board** is unchanged: the task page shows "on the branch <branch>" from `task.diff`'s
  `branch` (5.14), which needs no line of its own.

## File map

```
docs/SPEC.md                                                     modifies: revision 0.119; 5.11, 5.14, 5.17, 8.4, 8.5 (Task 1)
docs/plans/standalone/internal-task-ids/README.md                 modifies: Status done (Task 1)
docs/plans/standalone/internal-task-ids/0[1-6]-*.md               modifies: Status done; ticks (Task 1)
```

## Interfaces

Consumes: what plans 1 to 5 built, read from the code before writing, so that the spec says what
was built and not what was planned. Where the two differ, the plan goes back to planning and the
spec waits.

Produces: nothing in code.

## Tasks

### Task 1: SPEC revision 0.119

Files: as the file map.

Tests: none. This is the record of behaviour plans 1 to 5 already test. Its check is
`cargo xtask check` passing and reading each changed paragraph against the code it describes.

- [ ] `docs(spec): task ids stay inside Catervas, revision 0.119`

## Verification

```
cargo xtask check
# expected: xtask check: ok
grep -c "0.119" docs/SPEC.md
# expected: a count of at least 8 (the header line and each section's mark)
```
