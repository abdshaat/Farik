---
name: catervafying-the-repository
description: Use when a task asks you to document how the project is built and worked on, a task of its Catervafication, or to hunt for bugs in its code.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/engineering/`.

## 1. What the task asks

Write `conventions`, the rules the code follows, and `workflow`, how work is done in this
project. These documents are for agents. For a new project, set the workflow and the conventions
the stack calls for, from the approved product plan in `docs/catervas/product/`. Touch nothing
outside `docs/catervas/engineering/`.

## 2. The evidence

Read these, and nothing you cannot name:

- the lint and format configuration;
- the test layout, and how the tests are run;
- the last fifty commits' messages and branch names;
- the continuous integration files;
- a contributing guide, if there is one.

## 3. Write only what you can show

Write each rule with where you read it: the config file, the commit, the workflow file. A rule
nobody follows is not a rule here; say what the code does, and say when it varies. Never guess.

- `conventions`: naming, formatting, structure, how errors and tests are written.
- `workflow`: how a change goes from an idea to the main branch here: branches, commits,
  reviews, checks.

## 4. Say which workflow it is

The top of `workflow` says whether it records the project's own process or adopts Catervas's
recommended workflow, and the evidence that supports the choice.

With no documented process, no CI that runs tests and no consistent commit style, adopt Catervas's recommended workflow (your skill catervas-recommended-workflow) and say so at the top of the `workflow` document.

## 5. Finish

Commit with `catervas_git_commit`, and request `verifying`. The reviewer checks each rule against
the evidence you name.

## Hunting for bugs

When your task is the bug hunt, its one allowed path is `docs/catervas/engineering/bug-hunt` in
the team's format, and you change no code.

- Read the code, and run the project's tests and checks with `catervas_exec`.
- File each real bug with `catervas_file_bug`, with its evidence: the command and the lines of
  output, or the file and line. File at most 10, never the same bug twice, and a failing test only
  when the code, not the test, is wrong.
- Remove anything the commands left in the worktree.
- Write the report: what you checked, then a section headed "Bugs filed" with each id and title,
  or "None found".
- Commit with `catervas_git_commit`, and request `verifying`.
