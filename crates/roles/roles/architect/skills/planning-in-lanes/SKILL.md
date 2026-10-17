---
name: planning-in-lanes
description: Use when a task asks you to write an approved epic's implementation plan in docs/catervas/architecture/plans/.
---

# Planning in lanes

The plan is for agents, not for the owner. When you commit it, the Product Manager writes one task
for each task you planned, and builders work from those tasks. Write each planned task so that a
builder who has read only it and the plan knows what to build and where to stop.

## 1. Lanes

A lane is the tasks one builder does, in order. Tasks in different lanes run side by side.

- Plan at most as many lanes as the builders the contract names, Software Developers and UI/UX
  Designers who are active. One builder takes one lane, so a lane has more tasks, not more builders.
- All the tasks of a lane are for one role, `software_developer` or `ui_ux_designer`.
- Part the lanes' paths before their first wildcard: `apps/web/**` and `crates/**`, never
  `src/*.rs` and `src/*.ts`. Catervas refuses lanes whose `allowed_paths` could meet
  (`lanes_overlap`), and the Product Manager has to send the plan back.
- A task that needs another task's result says so under "depends on", and sits later in its lane
  or after the other lane's task is accepted.

## 2. The outline

Write `docs/catervas/architecture/plans/<epic-id>`, followed by the team's extension, in this
shape; the outline gives the headings, written in the team's format:

```
# Plan for <epic-id>: <title>

## Lane 1

### 1.1 <task title>
Role, what it builds, `allowed_paths`, depends on, constraints.

### 1.2 <task title>

## Lane 2
```

Each planned task names its role, what it builds in a sentence or two, the `allowed_paths` it may
change, what it depends on, and the constraints the builder keeps (a pattern to follow, a decision
to respect).

## 3. Finish

Commit the plan with `catervas_git_commit`, and request `verifying`. Catervas checks its criteria itself: only code is reviewed.
