# 0066. Task ids stay inside Catervas

Date: 2026-10-17
Status: accepted (the founder, 2026-10-17, quoted below; the rules decided by the planner the same day)

The founder: "make the task ID 'CTV-num' visible only internally, any PR open to github must be
named things like Docs, Fix, Feat, etc." Asked where, the founder answered: everywhere on GitHub.
A pull request is titled like "Feat: Character counter on the note input", its branch is named
like `feat/character-counter`, and neither its commits nor its description carry a `CTV-` id.
Catervas keeps the id internally and maps the branch to its task.

Amends:
- **SPEC 5.14**: a task's branch is no longer `feature/CTV-<n>`, `fix/CTV-<n>` or `docs/CTV-<n>`
  derived whenever it is needed; a folder change's is no longer `docs/folder-<n>`; a pull
  request's title no longer starts with the task's id or "Folder change <n>"; the start merge's
  message no longer names the task.
- **SPEC 5.11**: the contract gains `branch`, which Catervas writes.
- **SPEC 8.4**: new branches no longer move the task id counter (old ones still do).
- **SPEC 8.5**: `folder_doc.written`, `folder_doc.edited` and `folder_doc.approved` gain `branch`.
- **SPEC 5.17**: the Files page names who last changed a document from the pull request number in
  the merge's subject as well.

## Context

A task's id reaches git and the forge in five places today:
1. its branch, which `catervas_core::branch::task_branch` derives from the contract every time
   (about twenty callers in `catervas-store` and `catervas-runtime`, and the web's own copy of the
   rule on the task page);
2. its pull request's title, `CTV-<n>: <title>`, built by `Subject::titles` in
   `crates/runtime/src/orchestrator/integrate.rs`;
3. its pull request's body: the contract's intent and the completion and review notes, which agents
   write freely and which name other tasks (a hunt's bug's intent names its hunt);
4. the merge commit `Start <id> from <branch>` that `start_worktree` makes in a task's worktree
   when it starts from two branches, which is pushed with the branch;
5. the agents' own commit messages through `catervas_git_commit`, which nothing constrains.

Folder changes reach the forge as `docs/folder-<n>` and "Folder change <n>: <paths>". The ref
`refs/catervas/start/<id>`, the worktree folder `.catervas/local/worktrees/<id>` and the event log
are local and never pushed. Only `task_number_of_branch` parses an id back out of a branch name (to
keep the id counter past old branches), and only `named_in_subject` parses one out of a commit
subject (the Files page's "last changed by").

## Decision

**The type of a task's change.** A task is one of three types, from its contract alone:
- **Docs** when its assignee's role changes no code (`changes_code` is false), or when every one of
  its `allowed_paths` starts with `docs/` (a hunt's report, a plan, a twin's rewrite, a changelog);
- else **Fix** when its contract's `change` is `fix`;
- else **Feat**.

The branch's prefix is the type in lower case (`feat`, `fix`, `docs`); the pull request's title
word is the type as written above. `catervas_file_bug` files a hunt's bug with `change: fix`, so a
bug found by a hunt is a Fix without the log being read; the Product Manager may still change it
while refining, as any content. No other Conventional Commits type (`refactor`, `test`, `chore`)
is given to a task: the contract has nothing that says a task is one, and adding a value to
`change` for each is a separate decision.

**The branch name** is `<prefix>/<slug>`:
- The slug is the task's title, lower-cased in ASCII. Every run of characters that are not
  `a`-`z` or `0`-`9` (any non-ASCII character among them) becomes one `-`. The result is trimmed
  of `-` at both ends.
- A slug longer than 40 characters is cut at the last `-` within its first 41 characters and
  trimmed again; with no `-` there, it is cut at 40. A slug left empty is `change`.
- "Character counter on the note input" is `feat/character-counter-on-the-note-input`.
- The name is free unless another branch of the task's repository has that name, locally or under
  any remote, or a name that starts with it and a `/`, or another task's contract or another folder
  change records it. When it is taken, `-2`, `-3`, ... is appended, the first free one kept.
- A repository with a branch named exactly `feat`, `fix` or `docs` still cannot hold such a branch;
  git refuses the worktree and the refusal reaches the user as before (SPEC 5.14 already says so of
  `feature`).

**Where it is recorded.** The contract gains `branch`, written once by Catervas when the task's
worktree is first made (rule 7), before the worktree. It is written under the `Transitions` lock,
on the contract as it is then, and never changed afterwards: a start that conflicts and is undone
keeps it, and the next try makes the same branch. It joins `FIELDS_THE_STORE_OWNS`, so a filing,
an agent's `catervas_write_contract` and the human's save never write it. The human's save drops it
from what it changed, so an editor opened before the start does not clear it. No event records it:
the contract file is where the team's facts live (8.4), and `pull_request.opened` already names the
branch where the forge is concerned.

**Lookups.** Task to branch: `task_branch(contract)` answers the recorded `branch`, else the old
rule (`feature/`, `fix/`, `docs/` and the id), so every caller is unchanged and a task started
before this change keeps its `CTV` branch: it has its worktree already, so rule 7 never names it
again. Branch to task: the contract's `branch`, or `pull_request.opened`'s
`branch` and the event's task. Nothing parses an id out of a new branch.
`task_number_of_branch` keeps reading old branches for the counter alone.

**The pull request.**
- Its title is `<Type>: <title>`.
- Its body is built as before (intent, completion note, review note).
- In the title and the body, every `CTV-<n>` that is not part of a path (the character before it is
  not `/`) is replaced by that task's title in double quotes, or by `another task` when the board
  does not know the id. A plan's path, `docs/catervas/architecture/plans/CTV-12.md`, is left as it
  is, since it names a file in the change.

**Commit messages.**
- The merge `start_worktree` makes is `chore: start from <branch>`.
- `catervas_git_commit` refuses a message that holds `CTV-` followed by a digit, with
  `commit_message_names_a_task`: "<id> is a task id, which stays inside Catervas; describe the
  change instead".
- Its description tells the agent to write Conventional Commits (`feat: ...`, `fix: ...`,
  `docs: ...`) and to name no task id. The Conventional shape is asked for, not enforced: a refusal
  over a missing type would cost a session's turns for nothing the forge needs.
- The folder change's message (`docs(<folder>): ...`) and the owner's save's carry no id already.

**Folder changes.**
- A folder change's branch is `docs/<slug>`, the slug made from the documents' plain names joined
  by " and " (the words Today shows), free by the same rule as a task's.
- It is recorded as `branch` in the event that made the change: `folder_doc.written`,
  `folder_doc.edited` or `folder_doc.approved`. A change from an older log has none and is
  `docs/folder-<n>`.
- Its pull request is titled `Docs: Update <those names>`.
- Its number stays the internal name `catervas integrate folder-<n>` takes. It is still numbered
  past the old `docs/folder-<n>` branches.

**The Files page** reads who last changed a document from the commit subject. It now also
recognises a pull request number: a subject that starts `Merge pull request #<n>` or ends
`(#<n>)`. The number is matched against `pull_request.opened` (a task whose contract names no
`repo`) and `folder_change.opened`, the later of the two when both match. The `CTV-<n>` and
`folder-<n>` forms are still read for an older history.

**The task page** shows the branch `task.diff` names: `TaskDiff` gains `branch`, which is
`task_branch`'s answer, and the web drops its own copy of the rule.

**What stays internal, unchanged.** The id itself, the board, the log, the channel's lines, the
command line (`catervas integrate CTV-3`), `refs/catervas/start/<id>` (Catervas pushes
`refs/heads/<branch>` alone), the worktree folders, and `.catervas/contracts/<id>.yaml`.

## Alternatives rejected

- **Derive the new name whenever it is needed, from the title.** A title can change while
  refining, and a collision suffix depends on what branches existed at the start. The name must be
  made once and kept.
- **An event `task.branched`.** Every caller of `task_branch` would have to read the log, and the
  store's diff, the waiting rows and the Files page have only the contract at hand. The contract
  already carries what Catervas decided about a task.
- **Keep the id in the branch, as `feat/ctv-12-character-counter`.** The founder asked for it
  nowhere on GitHub.
- **More types (`refactor`, `test`, `chore`) guessed from paths.** A guess that calls a feature a
  refactor misleads the reader of the pull request list; three types read from the contract are
  never wrong about what the contract says.
- **Refuse a commit message that is not Conventional.** See above: asked, not enforced.
- **Strip ids from the notes when they are written.** The notes are read inside Catervas, where the
  id is the way an agent names a task; only what leaves for the forge is rewritten.

## Consequences

- A pull request reads `Feat: Add a login page` from `feat/add-a-login-page`, and nothing on the
  forge says `CTV`. Tasks started before this change keep their `CTV` branches, and their pull
  requests, opened after it, take the new title.
- A fresh clone's id counter no longer sees the tasks of branches made after this change. An id it
  reuses collides with nothing on the forge, since nothing there carries one. A contract file
  already there is still refused (8.4).
- A plan file's path keeps its id (`architecture/plans/CTV-12.md`), and so do documents in which an
  agent wrote one. File names and contents are not titles, branches, commits or descriptions, and
  this change leaves them alone.
- A task that starts from an old task's unmerged branch says `chore: start from feature/CTV-3`
  in its start merge: the old branch's name already carries the id, and renaming it is out of
  scope.
- The same document changed twice gets `docs/roadmap`, then `docs/roadmap-2`, since kept branches
  hold their names.
- Plans: `docs/plans/standalone/internal-task-ids/`. SPEC revision 0.119 records it.
