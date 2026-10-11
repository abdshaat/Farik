---
name: catervafying-the-repository
description: Use when a task asks you to write the product plan from the project, the first documents of its Catervafication.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/product/`.

## 1. What the task asks

Write the project's product plan from what the project holds: `spec.md`, the product description,
and `roadmap.md`, the roadmap, each for the owner to read, and their team versions, `spec.agent.md`
and `roadmap.agent.md`. Touch nothing outside `docs/catervas/product/`.

## 2. Read first

Read the code, the README and the documents the repository already has, its package files, its
history and anything it says about who it is for. Read before you write. For a new project there
is nothing to read: the owner's interview in your one-to-one chat is what you write from.

## 3. Write only what you can show

Write each rule or fact with where you read it. In the team versions, name the file or the commit.
In the owner's versions, say it in plain words, with no file names and no jargon the owner did not
use. What you did not find, leave out or say you did not find; never guess.

- `spec.md` has the product's name as its title, a paragraph on what it is and the problem it
  solves, and the sections Who it is for, What it must do, Not now and How we'll know it works,
  with Limits when the project states any.
- `roadmap.md` has the sections Now, Next and Later, each item one line: its name, then one
  sentence.

## 4. Both versions, one commit

A human document is a pair. Write `spec.md` with `spec.agent.md`, and `roadmap.md` with
`roadmap.agent.md`, in the same commit: the team version says the same, short and exact, for the
agents. Catervas refuses a commit that changes one file of a pair without the other.

## 5. Finish

Commit with `catervas_git_commit`, and request `verifying`. The reviewer checks each document
against the project; the owner then reads the plan and approves it.

## Each sprint

At the sprint review, propose the changes the sprint made to the spec and the roadmap with
`catervas_write_folder_doc`, each with its `agent_text` and a `summary` the owner can decide on, and
write the sprint report in the same session. The owner approves a proposal before it reaches the
project.
