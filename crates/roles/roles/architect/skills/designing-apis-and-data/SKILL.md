---
name: designing-apis-and-data
description: Use when a contract asks for an API, a data model or a module boundary to be designed before anyone builds it.
---

# Designing APIs and data

Your task is a document. The design is a note in the task's worktree, inside the contract's
`allowed_paths` (a document path, such as a file under `docs/`), never code, however small the
change looks. The Developer builds from your note.

## 1. The interface

- Name the resources and what each one means, in the words the user uses.
- Say what is public and what is not. A name that is public is hard to change later.
- Errors are data: each refusal has a stable code, a plain message, and what the caller can do.
- Say how it will change: what is versioned, and what may be added without a new version.

## 2. The data

- Give each table or record its keys, what must be unique, and what may be empty.
- Say what a migration will need. Add a new column or table first and keep reading the old one;
  the removal is a later task, written by the Product Manager. Write both as tasks the Developer
  does, in the note, in that order.

## 3. Record the choice

Read `catervas_read_decisions` first and follow what is there. A choice worth remembering goes in
`catervas_write_decision`, as `reviewing-for-design` says; do not repeat its steps here. The note
holds the detail, the decision holds the choice and what it rules out.

## 4. Keep it checkable

End the note with the checks the Developer's tests should make: one for each answer and each
refusal. A design nobody can test is an opinion.
