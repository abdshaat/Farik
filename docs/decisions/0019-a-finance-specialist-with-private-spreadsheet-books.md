# 0019. A Finance Specialist with private spreadsheet books

Date: 2026-09-27
Status: accepted

## Context

On 2026-09-27 the founder asked for a sixth role: a financial analyst and an accountant in one agent. It tracks every cost and expense of the product (billing, infrastructure, cloud, storage, and the team's own AI spending), forecasts long-term expenses, and runs the books. The founder chose that the role is optional, that it is built before the web UI, that the books stay private and outside the repository, and that they are spreadsheets. The design is `docs/design/finance-specialist.md`.

Two facts constrained the design. Farik works in the user's repository, which may be public; this one is. And every session's files are its own git worktree (spec 8.6), so an agent cannot reach anything outside it today.

The options for where the books live were these:
- **In the repository**, under a `finance/` folder. This is simple and versioned, but a public repository would publish the company's finances.
- **A second, private repository.** This is versioned and private, but a non-technical user would have to set up and connect a second repository.
- **Farik's machine-local folder, `.farik/local/finance/`.** It is never committed, like the event log (D5), and nothing needs setting up. It is not versioned, and it lives on one machine until the hosted tier's sync. This is the chosen place.

The options for the books' form were these:
- **A structured ledger written through Farik tools, as JSON Lines.** Each entry is checked against a schema, but the founder would need a Farik view to read it.
- **A plain-text accounting journal.** It is readable by accounting tools but not by most founders.
- **Events in the log.** This is the most auditable form, but it turns one role into a finance subsystem.
- **Spreadsheets.** This is the founder's choice. Everyone can open, check and hand an `.xlsx` to an accountant.

## Decision

Add a sixth role, the Finance Specialist (`finance_specialist`), optional in the team builder, with the Product Manager as its reviewer and the `read` and `network` tiers.

Its books are `.xlsx` workbooks in `.farik/local/finance/`. They are written and read through three Farik tools that only this role may call: `farik_read_costs`, `farik_read_sheet` and `farik_write_sheet`. The Product Manager may also call `farik_read_sheet` when it reviews a finance task.

A finance task's session runs in that folder instead of a git worktree. The task makes no commits, so once accepted it goes straight to done without integration.

It is phase 6 step 01.

## Consequences

Easier:
- The founder and any accountant can open the books in the spreadsheet program they already use.
- A public repository never carries financial data.
- The AI spending Farik already records becomes something an agent can explain and forecast.

Harder:
- The books are on one machine and unversioned, apart from the `.history/` copy of each workbook's last version. A lost disk loses them. Until the hosted tier's sync, the user should back the folder up, and the web UI should say so.
- A workbook tool cannot validate an accounting entry the way a schema-checked ledger could. The Product Manager's review and the human's reading are the check.
- Two dependencies join the workspace: `rust_xlsxwriter` and `calamine`.
- A second kind of session root, and a task with no branch, add two special cases to the runtime, each with its own test.
- Its numbers are management accounting. The product must keep saying it is not a tax filing or financial advice.
- The step in front of the web UI delays it by one step.
