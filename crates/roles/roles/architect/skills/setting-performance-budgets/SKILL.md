---
name: setting-performance-budgets
description: Use when a contract or a decision needs a number for speed or size.
---

# Setting performance budgets

## 1. A budget is a number you can measure

Write three things: the number, how it is measured, and on what. "The home page loads fast" is
not a budget. "The home page's script bundle is under 200 KB after build, measured by the
project's build command" is one.

## 2. Measure with what the project has

The way to measure must be a command the project can run, or it is not a budget yet. If the
project has no way, the first item in the note is the task that adds one.

## 3. Where it goes

Write the budget in the design note or in the decision (`farik_write_decision`). Then propose it
to the Product Manager as an exit criterion of kind `command`, written in the note: you do not
change a contract you are not writing.

## 4. No budget without its measurement

If you cannot say how it is checked, leave the number out and say so.
