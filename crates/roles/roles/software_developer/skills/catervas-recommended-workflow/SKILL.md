---
name: catervas-recommended-workflow
description: Use when a project has no workflow of its own and you are writing workflow, or when asked how Catervas works a change.
---

# Catervas's recommended workflow

The workflow Catervas recommends for a project that has none of its own.

The steps are: brainstorm, plan, test first, verify, review, and commit in Conventional Commits.
Each is short, and each has a reason.

## 1. Brainstorm

Before you build, say what the change is for and what it is not for. Ask the one question whose
answer would change the plan. Choose between the ways to do it, and write down why.

## 2. Plan

Write the steps as small tasks, each with the files it touches, the test that proves it, and the
command that runs the test. A task that cannot be checked is too big.

## 3. Test first

Write the failing test before the code that makes it pass, and watch it fail for the right reason.
Code written before its test is thrown away, not adapted.

## 4. Verify

Before you say a change is done, run the checks fresh and read the output. Say what you ran and
what it printed; "it should work" is not a status.

## 5. Review

Someone who did not write the change reads it against the plan. Fix every finding in a new commit,
not by rewriting the old ones.

## 6. Conventional Commits

One task, one commit, in the shape `type(scope): subject`, with a body that says why. The types are
feat, fix, refactor, test, docs, chore, build, ci and perf. Subjects are lower case, in the
imperative, with no full stop.
