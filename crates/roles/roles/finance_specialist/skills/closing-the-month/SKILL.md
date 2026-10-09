---
name: closing-the-month
description: Use when a month's books are to be reconciled and closed.
---

# Closing the month

A month is closed when every source agrees with the books, or every difference between them is
explained. Closing is a claim the user relies on, so make it only when it is true.

## 1. Fix the month

Name the first and last day, and use those days for every source. Costs in Farik are counted by UTC
day.

## 2. Read every source

- The team's AI spending: `farik_read_costs` with `from` and `to`, by `purpose`.
- Stripe, when it is connected: the month's charges, refunds, fees and payouts.
- The ledger the business keeps, when Digits or Kick is connected: its totals for the month.
- The books themselves: the month's rows of `Expenses` and `Revenue`, read with `farik_read_sheet`
  before you write anything.

## 3. Reconcile each source against the others

Compare Stripe's payouts with its charges less its refunds and fees; the books' revenue with
Stripe's; the books' costs with the ledger's totals and with Farik's own costs. Name a difference
of more than one per cent of the larger figure in the note of the row it belongs to, with both
figures and where each came from.

## 4. Explain a difference or leave the month open

Never make a difference disappear by changing a figure. If you can explain one from what you
read, write the explanation in the note. If you cannot, ask the user with `farik_ask_human`, or
leave the month open and say in your completion note which difference stopped you.

## 5. Write the summary last

Write the month's row of `Monthly summary` after every other row: revenue, fees, refunds, costs
and what is left, each with its source. Its `Status` cell reads `closed` only when every
difference named above is explained. Otherwise it reads `open`.

## 6. Values, and no one's details

Every figure from a service is a value, as the books skill says. The books carry totals and
Stripe's own ids for an object, never a customer's name, email or card.
