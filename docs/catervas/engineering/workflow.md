# Workflow

**This records the project's own process; it does not adopt Catervas's recommended workflow.** The
project documents its process in `docs/standards/workflow.md`, which `CONTRIBUTING.md` and
`CLAUDE.md` make mandatory; CI runs the full check, tests included, on every pull request and on
`main` (`.github/workflows/check.yml`); and commits follow one style, Conventional Commits, checked by
a `commit-msg` hook (`xtask/src/commit_message.rs`). Where this page and
`docs/standards/workflow.md` disagree, that document wins.

Read on 2026-10-11 from the worktree of `feature/CTV-4`, branched from `main` at `6e82eab2`.

**What could not be read.** The skill asks for the last fifty commits and their branch names. This
session's shell refuses git, and the repository's `.git` directory is outside the worktree, so only
the five commits shown when the session started were read (listed under "What history shows"). The
rest of this page rests on the documents and configuration named beside each rule.

## Two ways work lands

The repository is worked in two ways, and both reach `main`:

1. **Phases**, the project's own process, for product work: a phase branch, a pull request, reviews,
   a merge commit (`docs/standards/workflow.md`; `CLAUDE.md` hard rule 11 and "Current state", which
   names pull requests #4 to #22 and the branches `phase/6-web-ui`, `phase/7-role-kits`,
   `phase/9-milestones`).
2. **Catervas tasks**, for work this team does under contracts: a `feature/CTV-<n>` branch in
   `.catervas/local/worktrees/CTV-<n>`, merged to `main` by Catervas as `Merge CTV-<n>: <title>`
   (this worktree and branch; `docs/catervas/architecture/structure.md`; ADR 0052 for the branch
   names; the merge commits `6e82eab2` and `7b0c91b0`).

The rest of this page describes the first in detail, because it is what the project wrote down; a
Catervas task follows its contract, and its commits follow the same commit rules.

## From idea to `main`: the six stages

Every change that adds or changes behaviour goes through six stages in order. A typo or a one-line
doc fix skips to stage 5. Skipping a stage needs the change owner's written reason in the pull
request (`docs/standards/workflow.md`, The sequence).

1. **Brainstorm.** Refine the idea against `docs/SPEC.md` (section and F-number); out of scope stated.
   Output: a two-to-twenty-line design, and an ADR in `docs/decisions/` when a decision of the spec
   is reversed or added to (`workflow.md` stage 1; `CLAUDE.md` Standards).
2. **Plan.** Two levels: phases and steps in `docs/plans/project-plan.md`; one plan per step at
   `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, copied from `docs/plans/step-template.md`.
   Work outside a phase is planned in `docs/plans/standalone/<name>.md`. A plan carries decisions,
   signatures, a file map and named tests, not code, and stays near 300 lines (ADR 0008). One
   readiness review by a session that did not write it, one round unless an undecided question is
   found (`workflow.md` stage 2; `code.md` Documents; ADRs 0003, 0008, 0010).
3. **Execute.** On the phase branch, in a git worktree, one plan task at a time under TDD: a failing
   test watched to fail for the right reason, the simplest code to pass, then refactor. Code written
   before its test is deleted. One task, one commit, which ticks the task's checkbox in the plan
   (`workflow.md` stage 3; `CLAUDE.md` hard rules 1, 3, 4).
4. **Verify.** Run `cargo xtask check` freshly and paste the command and its output; "should work" is
   not evidence (`workflow.md` stage 4; `CLAUDE.md` hard rule 2).
5. **Pull request and review.** Self-review, then the phase's pull request to `main` from
   `.github/pull_request_template.md`, opened as a draft when the phase's first step is pushed and
   marked ready when the last step verifies. Each task is reviewed once after its commit, by a
   session that did not write it; the bar is mutation (re-introduce the bug each test claims to
   catch). Findings come back as one fix report; fixes are new commits, never force-pushes. A step
   lands when its tasks are reviewed and fixed, `cargo xtask check` passes locally and the `check`
   workflow passes on its last pushed commit (`workflow.md` stage 5; `CLAUDE.md` hard rules 10, 11).
6. **Finish.** Check once more on the final commit, merge with a merge commit (never squash) so every
   task commit survives, delete the phase branch (`workflow.md` stage 6; `code.md` Merge commit).

Planning and reviews run on Opus 5.5; execution of a passed plan and its fixes on Sonnet 5.5
(`workflow.md` Who does what; ADR 0032).

## Branches

| Branch | Use | Read in |
|---|---|---|
| `main` | default; `cargo xtask check` is never red on it | `code.md` (Repository and git, Toolchain) |
| `phase/<n>-<name>` | one per phase, from `main`, one pull request, merged once | `code.md`; `CLAUDE.md` hard rule 11 |
| `<type>/<short-kebab>` | work outside a phase: a standalone fix, a docs change; its own pull request | `code.md`; `CLAUDE.md` hard rule 11 |
| `feature/CTV-<n>`, `fix/CTV-<n>`, `docs/CTV-<n>` | a Catervas task's branch | ADR 0052; this branch |
| `claude/...` | a harness-assigned branch, kept as assigned | `code.md` |

## Commits

- `<type>(<scope>): <imperative subject>`, lower-case subject, no trailing period, at most 72
  characters; the body says why (`code.md`; `CLAUDE.md` hard rule 4).
- Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`, `ci`, `perf`
  (`xtask/src/commit_message.rs` `TYPES`).
- Scope: `code.md` lists a crate or package name without its prefix, or `docs`, `repo`, `roles`,
  `skills`, `xtask`. The hook checks only that the scope is present and kebab-case, so other scopes
  pass; recent history has `docs(product)` and `docs(architecture)`.
- Commits starting `Merge ` or `Revert ` pass the hook unchanged (`commit_message.rs`).
- The hooks are local and opt-in: `cargo xtask install-hooks` writes `pre-commit` (format check,
  bare-TODO check) and `commit-msg`. CI does not check commit messages
  (`xtask/src/main.rs`; `code.md` Commit hooks; `.github/pull_request_template.md` checklist).

## Checks

- Locally: `cargo xtask check`, or `cargo xtask check --integration` to include the tests that need
  git, Docker or a browser (`xtask/src/main.rs`; `conventions.md` here lists what each runs).
- CI: one workflow, `check`, on every pull request and on pushes to `main`: Rust 1.98.1 with rustfmt
  and clippy, pnpm and Node from `.node-version`, `docker pull alpine:3.22` and the Designer's
  Playwright image, Chromium for Playwright, then `cargo xtask check --integration`. Nothing in CI
  runs the check without the flag (`.github/workflows/check.yml`).

## Definition of Done

The checklist in `.github/pull_request_template.md`: every plan task ticked by its commit with its
tests watched to fail first; every step reviewed against its plan with mutation as the bar; no file
outside the plan's file map; `docs/SPEC.md` and an ADR updated where behaviour or a decision changed;
commits and branch per `code.md`. A pull request is reviewed by someone, or a fresh session, that did
not write it (`workflow.md` Definition of Done; `CLAUDE.md` hard rules 8, 10).

## What history shows

The five commits read, newest first (the session's starting `git log`):

| Commit | Subject |
|---|---|
| `6e82eab2` | `Merge CTV-3: Document how the project is put together` |
| `7b0c91b0` | `Merge CTV-2: Write the product plan from the project` |
| `d9118f41` | `docs(architecture): document how Catervas is put together` |
| `81438dff` | `docs(product): leave the DevOps Engineer out of the kit check (CTV-2)` |
| `d3663417` | `docs(product): write the product plan from the project (CTV-2)` |

They follow the commit rules above. Two of the three non-merge commits end their subject with the
task id in parentheses, `(CTV-2)`; `d9118f41` does not, so that suffix is a habit, not a rule. The `xtask` test `accepts_a_merge_commit` cites
`Merge pull request #3 from abdshaat/phase/0-foundation`, the phase merges' shape
(`xtask/src/commit_message.rs`).
