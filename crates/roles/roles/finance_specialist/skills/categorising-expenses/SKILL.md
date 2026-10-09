---
name: categorising-expenses
description: Use when a cost needs a category, or the books' categories need setting up.
---

# Categorising expenses

A category says what a cost was for, in words the user would use, so that a month's totals mean
something. Your categories are for running the product, not for a tax return.

## 1. Read the categories before you choose

The books keep their categories in the `Categories` sheet of `books.xlsx`, one to a row. Read it
with `farik_read_sheet` before you write a row to `Expenses`. When the books or the sheet are not
there yet, write the few categories the user's costs call for first, such as hosting, software,
payment fees and AI work, each with a plain sentence saying what belongs in it.

## 2. One category to a line

Give every row of `Expenses` exactly one category from that sheet. A cost that is really two things,
such as a hosting bill with a support plan on it, is two rows, not a mixed category.

## 3. The team's own AI spending

Read it with `farik_read_costs` by `purpose`, for the dates the books cover, and write one row for
each purpose, so that what the team's AI work cost does not go in as one lump. Name the dates beside
the row. Use the AI work's own category, and add one for it if the sheet has none.

## 4. A new category needs a reason

Add a category only when no existing one fits, and write the reason in the new row's note. Before
you add one, check whether two categories already there should be one. A long list of categories
helps nobody read a month.

## 5. Never give a tax category as advice

Do not name a tax category, a deduction or a rule of any country, and do not say that a cost can
be deducted. If the user asks, say that this is for an accountant to decide.

## 6. When nothing fits and you cannot say why

Do not guess. Ask the user with `farik_ask_human`, with the cost, its source and the two categories
you cannot choose between.
