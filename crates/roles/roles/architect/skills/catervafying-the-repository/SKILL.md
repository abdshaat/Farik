---
name: catervafying-the-repository
description: Use when a task asks you to document how the project is put together, a task of its Catervafication.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/architecture/`.

## 1. What the task asks

For an existing repository, write `overview`, `features`, `jobs` and `structure`. For a
new project, write the first architecture from the approved product plan: `overview` and
`structure`. These documents are for agents. Touch nothing outside
`docs/catervas/architecture/`.

## 2. Read first

Read the code, the README, the package and build files, the configuration, the continuous
integration files and the documents the repository already has. For a new project, read
`docs/catervas/product/` as it stands on the integration branch.

## 3. The four documents

- `overview` says what the system is made of and how the parts talk to each other, with a
  diagram small enough to read.
- `features` lists what the project does today, one line each, with where it lives.
- `jobs` lists every scheduled or background job found in the code, the continuous integration
  and the configuration: what runs, when, and where it is defined. Say so when there are none.
- `structure` maps the directories: what each holds and which way the dependencies run.

## 4. Write only what you can show

Write each fact with where you read it: a file path or a commit. What you did not find, leave out
or say you did not find; never guess. If two places disagree, say both.

## 5. Finish

Commit with `catervas_git_commit`, and request `verifying`. Catervas checks that the documents exist (only code is reviewed), so write them
against the code.

## Each delivery

At each delivery's review, bring `features`, `jobs` and `structure` up to date with the changes
it integrated, with `catervas_write_folder_doc`: one session for the delivery, not one for
each change. Leave a document alone when nothing changed.
