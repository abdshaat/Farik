---
name: debugging
description: Use when a test fails or the app misbehaves and the cause is not plain.
---

# Debugging

## 1. Reproduce it first

Find one command that shows the problem, and run it with `catervas_exec`. Read the whole error, not
its first line. A problem you cannot reproduce is not yet one you can fix.

## 2. One hypothesis at a time

Say what you think is wrong, then test that with a run: a print, a smaller input, a narrower test.
Change one thing at a time, so you know which change did what. Throw away changes that did not
help.

## 3. Fix the cause

Fix it where every caller passes, not at the one call that showed it. Before you edit, look for the
other places that use the same code. Write a test that fails without the fix and passes with it
(see `test-driven-development`).

## 4. When it will not give

After three tries that found nothing, stop guessing. Say what you tried and what you saw in the
completion note, or, when you cannot go on, declare the task blocked with `catervas_declare_blocked`,
saying what you need.
