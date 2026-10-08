---
name: keeping-the-vendor-register
description: "Use when a task touches `vendors.xlsx`."
---

# Keeping the vendor register

`vendors.xlsx` is the register of the sellers the business buys from and the subscriptions it pays
for. It is the same register for a supplier of goods and for a software plan. One row for each, on a
sheet named `Vendors`.

## 1. The columns, in this order

`vendor`, `what_for`, `plan`, `price`, `currency`, `period`, `started_on`, `renews_on`,
`notice_days`, `auto_renews`, `status`, `owner`, `purchase`, `terms_url`, `evaluation`, `notes`.

- `period` is `month`, `year`, `once` or `usage`.
- `status` is `planned`, `trial`, `active` or `cancelled`.
- `purchase` is the order's number, such as `PO-3`, when an order was made.
- `evaluation` is the comparison the row rests on, such as `evaluations/baby-car-mirrors.md`.
- For goods bought once, `plan` holds the item and `period` is `once`; leave the renewal columns
  empty.

## 2. Dates, values and text

Write every date as an ISO date, such as 2026-03-01. Write every cell that came from a seller or a
service as a value, never as a formula, and write text as text. A price is a number, in the
currency of its `currency` cell.

## 3. Read before you write

`farik_write_sheet` replaces the whole workbook, and the founder may have edited it by hand. Read
the register first with `farik_read_sheet`, keep every row and cell you read, and change only what
the task asks. Farik keeps every earlier version.

## 4. Orders

Each order that `farik_read_purchase_orders` gives as received is written into the register, with
what was paid, its `PO-<n>` number and its renewal day if it has one. Record what the founder
recorded: never write that an order was placed or received when the order does not say so.
