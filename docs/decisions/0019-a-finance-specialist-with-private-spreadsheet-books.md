# 0019. A Finance Specialist with private spreadsheet books

Date: 2026-09-27
Status: accepted

## Context

On 2026-09-27 the founder asked for a sixth role: a financial analyst and an accountant in one agent. It tracks every cost and expense of the product (billing, infrastructure, cloud, storage, and the team's own AI spending), forecasts long-term expenses, and runs the books.

The founder then decided the following:
- The role is optional.
- The books are private and outside the repository, and they are spreadsheets.
- Receipts come from the user's email; the user never uploads a bill.
- The role reads Stripe, read-only, when the product uses it.
- It is built in phase 7, after MCP connections. The first placement, before the web UI, was withdrawn when email and Stripe were added, because Stripe needs what phase 7 step 01 builds.

That afternoon's design read the user's main mailbox through Google's and Microsoft's mail APIs, filtered by a label or folder, before the web launch. The same evening a market evaluation (kept outside the repository, in `~/farik-research/`) found three things against it, and the founder accepted them:
- The one finance feature with proven demand inside an agent-team product is accounting of the agents' own spending, which needs no connector.
- `gmail.readonly` is a restricted scope: brand verification, a paid third-party security assessment repeated every year, and weeks to months of process; Microsoft asks for publisher verification. The label filter is applied after a grant that reaches the whole mailbox, so it is Farik's promise, not the provider's enforcement. And email read by an agent is the best-documented prompt-injection channel. Every incumbent takes receipts through a dedicated address instead.
- Receipts alone are the wrong source of truth: every bookkeeping product reconciles against the bank or card statement, and receipts plus Stripe alone miss card spend and double-count.

The design is `docs/design/finance-specialist.md`.

Three facts constrained the design:
- Farik works in the user's repository, which may be public; this one is.
- Every session's files are its own git worktree (spec 8.6), so a task with no worktree needs an exception in each rule that assumes one.
- A mailbox holds more than receipts, and anyone can send an email.

The options for where the books live were these:
- **In the repository.** Simple, but a public repository would publish the company's finances.
- **A second, private repository.** Private, but hard for a non-technical user to set up.
- **Farik's machine-local folder, `.farik/local/finance/`.** It is never committed, and nothing needs setting up. This is the chosen place.

The options for the books' form were these:
- **A schema-checked ledger.** Farik could validate each entry, but the founder would need a Farik view to read it.
- **A plain-text accounting journal.** Readable by accounting tools but not by most founders.
- **Events in the log.** The most auditable form, but a subsystem rather than a role.
- **Spreadsheets.** The founder's choice.

The options for taking receipts were these:
- **A general mail MCP server.** Nothing new to build, but the agent could search the whole mailbox.
- **Farik's own connector to the main mailbox, filtered.** The afternoon's choice, withdrawn for the reasons above.
- **A dedicated receipts mailbox at the user's own provider, read over IMAP.** The boundary is the mailbox itself; no provider verification applies; the user forwards receipts to it or has vendors bill it. This is the chosen way, after the web release.

## Decision

Add a sixth role, the Finance Specialist (`finance_specialist`). It is optional in the team builder, the Product Manager is its reviewer, it has the `read` and `network` tiers, and its avatar is the brand's `extra-4` character.

Its books are `.xlsx` workbooks in `.farik/local/finance/`. It records and forecasts the team's own AI spending first, and reads Stripe through Stripe's official MCP server with a read-only key, Farik allowing only Stripe's read tools. That is phase 7 step 02, before the web launch.

In the first release after the web release check, phase 8 step 02 (phase 10 step 02 since ADR 0020 inserted the role-kits phase), it takes receipts from a dedicated receipts mailbox over IMAP, with approved senders, and reconciles the books against bank or card statements the user exports as CSV. A daily receipts sweep files new receipts while a process drives the project.

A connector to the user's main mailbox is not planned. It is built only if users ask once receipts intake has shipped.

A finance task's session runs in the finance folder instead of a git worktree, it names its workbooks when it declares done, it ends at `accepted` with nothing to integrate, and one piece of finance work touches the folder at a time.

## Consequences

Easier:
- The launch carries the finance feature with proven demand and no compliance cost, and the web release check tests it.
- Nothing about receipts waits on Google or Microsoft, and the user's main mailbox is never granted to anything.
- The user does nothing to keep the books but forward receipts and export a statement, and a forwarding rule does the first.
- The founder and any accountant can open the books in the spreadsheet program they already use.
- A public repository never carries financial data.

Harder:
- The user has to create a second address or alias, and set a forwarding rule or tell vendors to bill it; setup asks that of a non-technical user, and the web app must walk them through it.
- IMAP with an app password is the older, less polished way to reach a mailbox, and some providers make app passwords awkward to find.
- A task with no branch and no worktree needs an exception in seven of the harness's rules (spec 5.2, 5.3, 5.4, 5.5, 5.8, 5.14 and the tier table in 5.6) and in the program's `permissions.deny` list. Each is one more special case to test.
- Formulas are an injection path into the founder's spreadsheet program, so the write tool refuses the ones that reach outside the workbook and writes untrusted content as values.
- The books sit on one machine, unversioned apart from `.history/`. Until the hosted tier's sync, the user should back the folder up.
- A spreadsheet cannot validate an accounting entry. The Product Manager's review and the human's reading are the check.
- The daily sweep is a new kind of scheduled session, and it costs money every day it finds receipts.
- Its numbers are management accounting. The product must keep saying they are not a tax filing or financial advice.
- The web UI's team builder is built for five roles and gains the sixth in phase 7.
