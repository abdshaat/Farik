# Standalone (open issues), plan 8: the spec records the fixes

Status: done
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md`, revision 0.122
Depends on: plans 1 to 7 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`docs/SPEC.md` says what plans 1 to 7 built (hard rule 8), in one revision, 0.122, and the ADRs
that ADR 0069 amends say so. Documents only, so no test; its check is the full check and a reading
against ADR 0069 and the plans' Decisions.

## Decisions

- **One revision**, 0.122, dated 2026-10-17, naming ADR 0069 and the standalone plans
  `open-issues` 1 to 7, placed after 0.121's line. Each changed paragraph carries "(changed in
  0.122; ADR 0069)" or "(added in 0.122; ADR 0069)".
- **The revision line**: "Revision 0.122 (2026-10-17) records the open issues of the note-chat2
  journey (ADR 0069, standalone plans `open-issues`): in 5.2 a reviewer in `verifying` and the
  Product Manager in `refining` block what they hold, an answer returns a blocked task where its
  block came from or to `refining`, and an accepted task whose pull request no longer merges goes
  back to `assigned`; in 5.4 a reviewer's refused request no longer parks the review, the
  governor's refused acceptance of a task it verifies alone escalates, a team's documents are never
  a UI change, and the reviewer of a UI change reads the Designer's checks; in 5.7 a blocked task's
  choices name who is stuck and offer the plan's change; in 5.14 rule 2 refreshes a pull request
  that stopped merging or sends it back, and a pull request's body is what changed and how it was
  verified; in 5.2 the tidy runs before each work session too; in 8.3 the checks' container sees
  the repository's git store, read-only; in 4.1 and 5.12 the scan, the integration branch setting,
  a delivery report's title and an added agent's persona; two event kinds, 122 in all (8.5)."
- **5.2**, the table: the rows of plan 1's Decisions, worded as the table words them —
  `refining | blocked | Product Manager | a written blocker with what is needed`;
  `verifying | blocked | reviewer | a written blocker with what is needed`;
  `blocked | in_progress | Scrum Master or human` gains "for a block from `assigned` or
  `in_progress`"; `blocked | verifying | Scrum Master or human | a written resolution, for a block
  from verifying`; `blocked | refining | human | a written resolution, for any block: the Product
  Manager changes the plan`; `accepted | assigned | Governor | the task's pull request no longer
  merges and Catervas's merge of the integration branch into its branch left conflicts`. The
  paragraph after the table says `accepted` is left only by that row. The ASCII diagram gains
  `verifying -> blocked` and `accepted -> assigned` arrows. The tidy paragraph (0.121's) adds
  "and before each implement session, never while a merge is in progress in the worktree".
- **5.4**: the reviewer's refused request, the governor's refused acceptance, `docs/catervas/**`,
  and the Designer's checks in the brief, each one sentence from plan 2's Decisions.
- **5.7**: the blocked row names the agent that blocked; its choices are "Answer and carry on"
  (back where the block came from), "Ask {pm} to change the plan" (unless it came from `refining`),
  and "Cancel the task"; the same for `blocker_age`.
- **5.14**: rule 2 reads `mergeable`; `CONFLICTING` brings the branch up to date (clean: pushed to
  the same pull request, `pull_request.refreshed`; conflicted: `pull_request.conflicted`, back to
  `assigned`, the assignee resolves and the next acceptance pushes to the same pull request); a
  folder change escalates once; a delivery counts a task once. A pull request's body: plan 6's
  shape, the 1500-character cap, and the folder change's body.
- **8.3**: the checks' container mounts the worktree's git store read-only at its own path, unless
  the repository's settings hold a password; agents' containers never do; the base container
  mounts nothing (unchanged).
- **8.5**: `pull_request.conflicted` and `pull_request.refreshed` with their fields; the count
  becomes 122; the gate list gains `pull_request_conflicted`.
- **4.1** (the scan): a tracked `package.json` with no lockfile is npm's; `bun.lock` is Bun's.
  **5.12** (team rules, the policy): the integration branch is set in Settings → How work runs and
  must name a local branch. **5.17** (role folders): a delivery's report is named "Report for
  delivery <n>" and its pull request "Docs: Report for delivery <n>". **4.4** (changing the team):
  an added agent gets its role's persona and a Designer its browser. **7 F8** (line 981): "changes
  nothing" becomes "changes nothing but the work it takes on as its own task (ADR 0063)".
- **ADRs**: ADR 0021 gains under its header "Amended by ADR 0022 (a private file when the computer
  has no keychain) and noted in ADR 0069." ADR 0066 and ADR 0068 each gain "Amended by ADR 0069."
  with the item it changes.

## File map

```
docs/SPEC.md                                   modifies: revision 0.122 (Task 27)
docs/decisions/0021-*.md, 0066-*.md, 0068-*.md modifies: the amended-by lines (Task 27)
docs/plans/standalone/open-issues/*.md          modifies: each plan's Status to done, README's too (Task 27)
```

## Interfaces

Consumes: what plans 1 to 7 committed, and their Decisions. Produces: nothing a program reads.

## Tasks

### Task 27: `docs/SPEC.md` revision 0.122, and the ADRs it amends

Files: as the file map says. No test: documents only (ADR 0060's one test per functionality has
nothing to test here).

- [x] `docs(spec): blocked work reaches the owner, checks see git, pull requests stay mergeable, revision 0.122`

## Verification

```
cargo xtask check
# expected: xtask check: ok
grep -c "0.122" docs/SPEC.md
# expected: at least 2 (the revision line and a marked paragraph)
```
