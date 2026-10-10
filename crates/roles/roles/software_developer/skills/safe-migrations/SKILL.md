---
name: safe-migrations
description: Use when a change alters stored data or the shape of it.
---

# Safe migrations

## 1. Add before you remove

Add the new column or table first, and make the code read both the old and the new. The removal is
a later task, which the Product Manager writes: name it in the completion note. Never lose data in
one step.

## 2. Forward, and a way back

A migration runs forward and has a way back. Run both with `catervas_exec`, against the database the
project's own tools start in the sandbox, when the project has one. When it has none, say in the
completion note that they were not run.

## 3. Large changes

Change large amounts of data in batches, so that one failure leaves the rest as it was.

## 4. Say what a deploy must do first

The completion note says what has to happen before the change is deployed: which migration runs,
in what order, and what to check after.
