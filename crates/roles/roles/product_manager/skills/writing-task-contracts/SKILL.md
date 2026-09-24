---
name: writing-task-contracts
description: Use when a request from the user needs triage, or a draft or refining task needs its contract written, whether an epic or a standalone task, so that it passes the Definition of Ready.
---

# Writing task contracts

A contract is the only way work enters the team. The governor checks it against the Definition of
Ready before anyone may start, and every criterion in it is what a reviewer will run to decide
whether the work is done. Write it for the reviewer who has nothing else.

## 1. Triage, when it is yours

When the team has no active Scrum Master, you triage every request before anything else, with
`farik_triage_request`:

- **Large** when the request needs more than one task, touches more than one part of the system,
  or changes what the product is for. It becomes an epic.
- **Small** when one agent can finish it within one task's budget. It becomes a standalone task.

Give the reason in one or two sentences. If you size a request small and find while refining that
it is not, re-triage it as large; its refining starts over as an epic.

## 2. Questions first, for an epic

Before you write an epic, ask the user every question you need, through `farik_ask_human`, one
question per call, and end your turn after each. Ask about the need, the user who has it, what
must not change, and how the user will know it worked. Do not guess an answer you could ask for.
The governor refuses an epic's contract while a question you asked is unanswered.

When you believe you have no questions, say so in the contract's intent. The user's approval is
the check that you were right.

A standalone task may need questions too; ask them first in the same way.

## 3. Write the contract

The team's rules and the criterion library are in this prompt; call `farik_read_rules` or
`farik_read_criteria` only if the prompt's copy says it was cut. Read the board
(`farik_read_board`) when the contract depends on other tasks. Then write, with
`farik_write_contract`:

- **Intent**: the user-facing reason for the work, in the user's terms. Not what to change, but
  why it matters and to whom.
- **Requirements**: numbered `R1`, `R2`, ..., each one thing that must be true afterwards.
- **Scope**: `allowed_paths` as narrow as the work allows and within the team's ceiling, and at
  least one `out_of_scope` item that says where the work stops. An empty exclusion list predicts
  scope creep.
- **Exit criteria**: see below.
- **Assignee role and reviewer role**: a Developer's task is reviewed by an active Architect when
  the team has one, else by another active Developer, which needs two active Developers; an
  Architect's or a Marketing Specialist's by you. Paused and retired agents do not count. When
  the team has neither, the contract fails readiness: ask the user for a reviewer with
  `farik_ask_human` rather than naming one nobody can staff. Nobody reviews their own work.
- **Risk** and **budget**: set both. A task's budget is within the team's maximum when the team
  sets one, what is left of the sprint when it has a budget, and, under an epic, what is left of
  the epic.
- **Dependencies**: only tasks that exist and are at least `ready`.

## 4. Exit criteria

Take criteria from the library whenever one fits: name them by `{ id, name }` in the `criteria`
list and the library's definition is copied in. Write your own only for what the library does not
cover.

A good criterion would fail if the work were wrong, not merely if nothing ran:

- `test` and `command` criteria name the exact command and what a pass looks like (an exit code,
  a line the output must or must not contain). Ask for new tests (`new_tests_required`) when the
  change is behaviour the existing suite cannot see; the team's rules may require it.
- `artifact` criteria name a path and what it must contain.
- `review` criteria are yes-or-no questions the reviewer answers with a cited reason each; use
  them only where no command can decide.
- `human` criteria are questions only the user can answer.

Include every method the team's rules require.

## 5. The Definition of Ready

A contract written as sections 3 and 4 say meets the Definition of Ready. Request `ready` with `farik_request_transition`. If the governor refuses, every failed rule is
in the answer with what to change; fix them all and ask again. A contract that fails three times
escalates to the user. When no agent can review, ask the user to add one, an Architect or a second
Developer; do not change the reviewer role to one nobody on the team holds.

## 6. After approval

An epic always waits for the user's approval before it is ready, and so does a `high` risk task or
any task the team's policy names. Only after an epic is approved do you write its product
documents under `.farik/product/` with `farik_write_product_doc`. When you break an approved epic
down, file each task with `farik_create_task` and its `parent` set, each with clear deliverables and
exit criteria of its own, then assign them with `farik_assign_task`.
