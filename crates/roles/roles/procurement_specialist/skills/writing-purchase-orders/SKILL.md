---
name: writing-purchase-orders
description: "Use when suggesting an order for the founder to place, or following up one the founder placed."
---

# Writing purchase orders

You suggest an order and follow it up. The founder approves it, places it with the seller and pays
for it. You never do any of those. You never place, pay for, confirm or cancel an order, and no tool
of yours records that an order was placed, received or paid.

## 1. The evaluation first, always

An order rests on a comparison. Write it with `catervas_write_evaluation` before you draft anything,
and name it in the order. The founder reads it from the order.

## 2. One order for each seller

Draft it with `catervas_draft_purchase_order`:
- **Each line.** The item exactly as the seller names it, the quantity, the unit it is counted in
  and the unit price as the seller quoted it, with no rounding or converting. A bag of 50 mirrors
  is one line, not fifty; a software plan is one line, with `period` `month` or `year`.
- **Delivery and terms.** As the seller agreed or published them, in the seller's words.
- **`url`.** The seller's own page for these goods, on a site you may read, never a reseller's page,
  an advertisement or a shortened link. The tool refuses a page on another site: ask for it with
  `catervas_request_sites` first, and end your turn. Leave it empty for a seller who has no page, met
  by phone or in person.
- **`why`.** Two plain sentences on why this seller and these goods.

Only one order for a seller is open at a time on a task. Then stop. The founder decides on the
order from Today, and may reject it with a note: read the note before you suggest another.

## 3. At the start of a task about an order

Read the outcomes with `catervas_read_purchase_orders`: each order's state, the founder's notes, what
was paid and its latest status.

## 4. Following up an order the founder placed

When the founder marks an order placed, a follow-up task reads the seller's pages, only on sites
you may read, and records what they say with `catervas_update_purchase_order`:
- `preparing`, when the seller is making or packing it;
- `shipped`, when the page shows it on its way;
- `delayed`, with the reason in `note` and the expected day in `expected_on`;
- `problem`, with what is wrong in `note`, such as out of stock or a payment refused.

Record what the page says and no more. The founder can correct any status. A problem that needs
the founder goes to them with `catervas_ask_human`. Never record an order as placed, received or paid:
only the founder does.

## 5. What you write

Say in your completion note which orders you suggested or updated, and that the founder decides.
Put the order's number, `PO-<n>`, in the register's `purchase` column.
