# 0019. A Finance Specialist with private spreadsheet books

Date: 2026-09-27
Status: accepted

## Context

On 2026-09-27 the founder asked for a sixth role: a financial analyst and an accountant in one agent. It tracks every cost and expense of the product (billing, infrastructure, cloud, storage, and the team's own AI spending), forecasts long-term expenses, and runs the books.

The founder then decided the following:
- The role is optional.
- The books are private and outside the repository, and they are spreadsheets.
- Receipts come from the user's email alone: the main mailbox, filtered, on Gmail or Google Workspace and on Outlook or Microsoft 365. The user never uploads a bill.
- The role reads Stripe, read-only, when the product uses it.
- It is built in phase 7, after MCP connections. The first placement, before the web UI, was withdrawn when email and Stripe were added, because both need what phase 7 step 01 builds.

The design is `docs/design/finance-specialist.md`.

Three facts constrained the design:
- Farik works in the user's repository, which may be public; this one is.
- Every session's files are its own git worktree (spec 8.6).
- A mailbox holds far more than receipts, and anyone can send an email.

The options for where the books live were these:
- **In the repository.** Simple, but a public repository would publish the company's finances.
- **A second, private repository.** Private, but hard for a non-technical user to set up.
- **Farik's machine-local folder, `.farik/local/finance/`.** It is never committed, and nothing needs setting up. This is the chosen place.

The options for the books' form were these:
- **A schema-checked ledger.** Farik could validate each entry, but the founder would need a Farik view to read it.
- **A plain-text accounting journal.** It is readable by accounting tools but not by most founders.
- **Events in the log.** The most auditable form, but a subsystem rather than a role.
- **Spreadsheets.** The founder's choice.

The options for reading email were these:
- **A general mail MCP server.** Nothing new to build, but the agent could search the whole mailbox, and the filter would be a request in its prompt rather than a rule.
- **Farik's own read-only connector for Gmail and Microsoft Graph.** It applies the filter in code and has no call that changes the mailbox. This is the chosen way.

## Decision

Add a sixth role, the Finance Specialist (`finance_specialist`). It is optional in the team builder, the Product Manager is its reviewer, and it has the `read` and `network` tiers.

Its books are `.xlsx` workbooks in `.farik/local/finance/`, with each receipt filed under `receipts/<yyyy-mm>/`.

It reads receipts through Farik's own read-only email connector, filtered by a Gmail label or an Outlook folder. It reads Stripe through Stripe's official MCP server with a read-only restricted key, and Farik allows only Stripe's read tools.

A daily receipts sweep records new receipts while Farik runs. Other finance work comes as tasks, whose sessions run in the finance folder and which end at `accepted`, with nothing to integrate. One piece of finance work touches the folder at a time.

It is phase 7 steps 02 (the role, the books, Stripe) and 03 (receipts from email).

## Consequences

Easier:
- The user does nothing to keep the books but label receipts in their own mail, and a filter can do even that.
- The founder and any accountant can open the books in the spreadsheet program they already use.
- A public repository never carries financial data.

Harder:
- Reading a mailbox is the most sensitive access Farik asks for. The filter, the read-only scope, the untrusted-content notice, and the absence of any call that changes mail are each tested. Even so, the OAuth grant itself reaches the whole mailbox, and the user has to trust Farik's code with it, which is one more reason the code is public.
- Google treats mailbox read access as a restricted scope. Until Google verifies the client, only 100 test users can connect Gmail, and for a public app a paid yearly security assessment is likely required. Microsoft wants publisher verification. The founder has to apply early, and the launch may ship with Gmail limited.
- Farik takes on two provider integrations and OAuth, and has to follow their API changes.
- The books sit on one machine, unversioned apart from the `.history/` copy of each workbook's last version. Until the hosted tier's sync, the user should back the folder up.
- A spreadsheet cannot validate an accounting entry. The Product Manager's review and the human's reading are the check.
- The daily sweep is a new kind of scheduled session, and it costs money every day it finds receipts.
- A task with no branch and no worktree needs an exception in five of the harness's rules (spec 5.2, 5.3, 5.4, 5.8, 5.14) and in the program's `permissions.deny` list. Each is one more special case to test.
- Formulas are an injection path into the founder's spreadsheet program, so the write tool refuses the ones that reach outside the workbook and writes untrusted content as values.
- Its numbers are management accounting. The product must keep saying they are not a tax filing or financial advice.
- The web UI's team builder is built for five roles and gains the sixth in phase 7.
