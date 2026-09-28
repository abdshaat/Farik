# The Finance Specialist

Status: approved by the founder on 2026-09-27, in conversation; revised the same day to take receipts from email and read Stripe; and revised again that evening, on the market evaluation's evidence, to record the team's AI spending first, read a dedicated receipts mailbox over IMAP after the web release, and leave a connector to the user's main mailbox unplanned. It is the design input to phase 7 step 02 and phase 10 step 02. ADR 0019 records the decision, and spec 0.21 (sections 1 and 6.6, with exceptions in 5.2, 5.3, 5.4, 5.5, 5.6, 5.8 and 5.14) carries its rules.

## Why

The founder wants one agent that is both a financial analyst and an accountant. It keeps track of every cost and expense of the product the team builds: billing, infrastructure, cloud, storage, and the team's own AI spending. It forecasts long-term expenses and runs the books.

The founder's answers shaped the design:
- The agent covers both the product's business finance and the team's own spending.
- It is optional: the team builder's default stays five agents, and the cap stays seven.
- The books are private, kept outside the repository, and they are spreadsheets.
- Receipts come from email; the user never uploads a bill.
- It reads Stripe, read-only, when the product uses Stripe.
- It is built in phase 7, after MCP connections exist, because Stripe needs them.

The market evaluation of 2026-09-27 (`~/farik-research/reports/`, outside the repository) changed the order. The founder accepted three findings: the one finance feature with proven demand inside an agent-team product is AI-spend accounting; reading the user's main mailbox through Google's or Microsoft's mail API needs a restricted-scope verification with a paid yearly assessment, and its label filter is Farik's promise rather than the provider's enforcement; and receipts alone are the wrong source of truth, since every incumbent reconciles against the bank or card statement. So the role ships in two steps. Phase 7 step 02, before the web launch: the role, the books, the AI-spend accounting and forecast, and Stripe. Phase 10 step 02, in the first release after the web release check: receipts from a dedicated receipts mailbox over IMAP, and bank statement reconciliation.

## The role

The role is the Finance Specialist, with the id `finance_specialist`.

Mandate:
- Track every cost and expense of the product. That covers the team's AI spending, from Farik's own cost records, first. It also covers revenue, fees, payouts and refunds, from Stripe. And, once receipts intake ships, everything the user's receipts and bank statements show: cloud, infrastructure, storage, hosting, domains, software subscriptions, and payment and billing fees.
- Keep the books: take each receipt from the receipts mailbox, categorise it, and record it. Reconcile against the statement, and close each month.
- Forecast expenses over 12 to 36 months, the AI spending among them from the first day.
- Analyse pricing and unit economics.
- Recommend budgets, in plain words.

What it produces is management accounting, meaning numbers a founder runs the product by. It is not a tax filing, statutory accounts, or financial advice. Its system prompt says so, and so does the team builder's line about the role.

It cannot:
- write application code, or anything in the repository;
- pay, refund, move money, change Farik's budgets, or change anything in Stripe or the mailbox;
- send, delete, move, or mark any email;
- publish anything;
- write anywhere but its finance folder.

The rest of its setup:
- **Tiers:** `read` and `network`, the network to research prices. It has no `write_workspace`, `execute`, git, or `external_effect` tier.
- **Reviewer:** the Product Manager, as for the Marketing Specialist.
- **Model:** `claude-sonnet-5` at medium effort, the Marketing Specialist's default. The team file can override it.
- **In the team builder:** it is optional and not among the five suggested. The user adds it with one click. Its avatar is `extra-4` (the character with glasses and the green cardigan), chosen on 2026-09-27; the brand's `characters/extra-4.png` and `avatars/extra-4.png` become `finance-specialist.png` in phase 7 step 02, and the character keeps its place among the extras for any other agent.

## The finance folder

The books live in `.farik/local/finance/`. The folder is on the user's machine, under the `.farik/local/` that Farik already keeps out of git (D5), so it is never committed, even when the repository is public.

```
.farik/local/finance/
  books.xlsx           Expenses, Revenue, Categories, Monthly summary; each row names its receipt file, statement line or Stripe object
  forecast.xlsx        the long-term forecast, the AI spending from phase 7 on
  <name>.xlsx          further workbooks a task asks for, such as pricing.xlsx
  receipts/<yyyy-mm>/  each receipt filed from the receipts mailbox: its attachment (PDF or image), or the email itself when it has none (phase 10)
  imports/             bank and card statements the user exports as CSV, the source the books are reconciled against (phase 10)
  mailbox.json         the receipts mailbox's settings, the approved senders, and the ledger of filed messages (phase 10)
  .history/            the previous version of each workbook, kept on every overwrite, and under <task-id>/ the copy of every workbook taken when a finance task is assigned, the baseline its reviewer compares against
```

The workbooks are `.xlsx`, which Excel, Google Sheets, Numbers and LibreOffice all open. The user may edit them by hand. The agent reads the file as it is before it writes, so a hand edit is kept.

Access:
- Only a Finance Specialist session and the Product Manager, as the reviewer of a finance task, can read the folder.
- Every other agent is refused the folder as it is today: it lies outside a task's worktree, and `.farik/local/**` is one of the default protected paths, which also keeps it from a conversation session in the project root.
- That protection stays for every session. A finance session gets one narrow exception, for `.farik/local/finance/**` alone.

## Phase 7 step 02: the role, the books, the AI spending, and Stripe

The first deliverable is the accounting of the team's own AI spending, because it needs no connector, no verification programme, and answers the market's top complaint: `farik_read_costs` gives the role the totals Farik already keeps, and the role writes them into `books.xlsx` and `forecast.xlsx` with a forecast of the next sprints and a recommended budget.

Stripe is read through Stripe's official MCP server, configured for this agent the way phase 7 step 01 configures any MCP server. The user signs in to Stripe (OAuth), which they can revoke from Stripe's dashboard; a scheduled run uses a restricted key with read permissions only, tagged for agents as Stripe requires from 2026-10-31. Read-only is locked twice: at Stripe by the key's permissions, and in Farik, where only Stripe's read tools are tagged `read` and `stripe_api_write` keeps `external_effect`, which this role is never granted, so the harness refuses a write before Stripe would. Stripe's `stripe_analytics` gives revenue metrics the role uses rather than re-deriving them. Stripe is optional: a product that takes no payments through Stripe skips it.

Three Farik tools, which only this role may call:

| Tool | What it does |
|---|---|
| `farik_read_costs` | The team's AI spending, from `Projections::costs`: totals by task, agent, sprint or day, with tokens and session counts, and optionally a date range |
| `farik_read_sheet` | One workbook or CSV in the finance folder, as its sheets of rows. Formulas come back with their last computed values. The Product Manager may also call it when it reviews a finance task |
| `farik_write_sheet` | Writes a whole `.xlsx` workbook in the finance folder: sheets, their columns, and rows of values or formulas. It refuses any other extension or any path outside the folder, and any formula that reaches outside the workbook: external references, `HYPERLINK`, `WEBSERVICE`, `IMPORTDATA` and its kin, `RTD`, DDE. A cell whose value came from a receipt, an email, a statement or Stripe is written as a value, never a formula, because the founder opens the workbook in Excel and a formula runs there. Before overwriting a file, it copies the old one to `.history/` |

Every Farik tool in this design has the `read` tier. Each writes only to Farik's private folder, and the tool itself holds that line, as `farik_write_memory` does.

The web app's team builder gains the role, with a short setup: the Stripe sign-in, optional.

## How a finance task runs

A finance task is an ordinary contract: a month's close, a forecast, a pricing analysis, or the first AI-spend books. It is filed, triaged, made ready, assigned, reviewed and accepted, with these differences.

- **Where its session runs.** The session runs with the finance folder as its working directory, not a git worktree, much as a conversation session runs in the project root. Claude Code's built-in `Read`, `Glob` and `Grep` then work on the folder and nothing outside it. The task's `allowed_paths` are under the folder. Two readiness rules stand in the way today, and each gets one exception for this role, recorded in spec 5.3: the document-paths rule, and the rule that no `allowed_paths` entry reaches under `.farik/` at all (`no_farik_paths`). The task may carry no `command` or `test` criterion, since it has no worktree to run one in; its criteria are `artifact`, `review` and `human`.
- **No branch and nothing to integrate.** Three more rules take an exception. When the task is assigned, Farik copies every workbook to `.history/<task-id>/`. It reaches `verifying` when every workbook in the `workbooks` list the assignee gives `farik_request_transition` exists (spec 5.2). Its reviewer receives each changed workbook beside that copy in place of a diff (spec 5.4). Once `accepted` it is finished, with nothing to integrate, and it counts as integrated for any task that depends on it (spec 5.14).
- **One session in the folder at a time.** No sweep starts while a finance task is `in_progress` or `verifying`, and no finance task is assigned while a sweep runs or while another finance task is `in_progress` or `verifying`, whatever the WIP limit and however many Finance Specialists the team has, so the folder changes under one session only, as a worktree does for code.

The explanation for the human goes in the task's completion note, which already opens with a plain-language summary (spec 5.4).

## Phase 10 step 02: receipts intake

Receipts come from a mailbox that holds nothing else, so the boundary is structural rather than a filter Farik promises to apply, which is how every incumbent (Expensify, Dext, Hubdoc, Kick) takes receipts.

- **The receipts mailbox.** The user keeps an alias or a second address at their own provider, `receipts@` their domain or a Gmail alias, and forwards receipts to it; a forwarding rule in their main mailbox does it for them, and vendors can bill it directly.
- **Connecting.** Farik reads that mailbox over IMAP, with an app password or the provider's IMAP sign-in, kept in the OS keychain where phase 7 step 01 keeps credentials. IMAP needs no restricted-scope verification from Google and no publisher verification from Microsoft, so nothing waits on a provider.
- **Approved senders.** A message is handed over only if its sender is on the user's approved list, which starts with the user's own address; anything else is left unread and never seen by the agent.
- **Nothing in the mailbox changes.** The connector cannot send, delete, move, flag, or mark a message as read; it keeps its own ledger of what it has handed over.
- **No duplicates.** Farik remembers each message the agent has filed and never hands it over again.
- **Untrusted content.** A message's text, and every attachment it files, reaches the agent under the untrusted-content notice of spec 8.6, since anyone who learns the address can send an email that tries to instruct an agent. An attachment is filed only if it is a PDF or an image and at most 10 MB.
- **The statement.** A bank or card statement, exported as CSV, goes under `imports/`; the role reconciles the books against it, so card spend with no receipt is not missed and a Stripe payout and an emailed invoice for the same money are not counted twice. Farik does not connect to a bank.

Two Farik tools, which only this role may call:

| Tool | What it does |
|---|---|
| `farik_read_receipts` | The unread messages from approved senders not yet filed: sender, date, subject, text, and the names and types of their attachments |
| `farik_file_receipt` | Files one message. Either it saves the receipt (the attachment it names, or the email itself) under `receipts/<yyyy-mm>/` and marks the message `recorded`, or it marks the message `not_a_receipt`. Either way, the message is not handed over again |

The agent then writes the expense into `books.xlsx`, with the path of the receipt file on its row.

**The receipts sweep.** While a process drives the project, Farik starts a receipts sweep once a day, as a tick rule like the standup. The connector fetches new messages first; when there are none, no session starts and nothing is spent. Otherwise a sweep is a short Finance Specialist session that reads the new receipts, files each one, and records it in `books.xlsx`. Its cost goes under a purpose of its own, `finance`. `sweep.started` and `sweep.ended` are recorded even when nothing is new, so the daily rule knows when it last ran. The user can also start one: "Check now" in the web app, or `farik finance sweep`. A sweep counts against the spending limits like any session. It runs only when the team has an active Finance Specialist and a connected mailbox.

The web app's finance setup gains the mailbox connection and the approved senders. `farik finance connect`, `farik finance disconnect` and `farik finance sweep` do the same from the command line.

## Not planned

- **A connector to the user's main mailbox** through Google's or Microsoft's mail API, filtered by a label or folder, the design of the afternoon of 2026-09-27. It needs Google's restricted-scope verification for `gmail.readonly`, with a paid third-party assessment repeated every year, and Microsoft's publisher verification; its filter is applied after a grant that reaches the whole mailbox; and email read by an agent is the best-documented prompt-injection channel. It is built only if users ask for it once receipts intake has shipped.
- **Reading a paid ledger** (Kick or Digits expose MCP servers) as an alternative to keeping the books. Noted for phase 12, when the hosted tier decides what it integrates.
- **A page that shows the books in the browser.** The workbooks are the view until a step asks for one.

New dependencies, each pinned per the repository's rules and named in the step plans: `rust_xlsxwriter` to write workbooks and `calamine` to read them (phase 7); an IMAP client (phase 10).

## Tests

The step plans turn each of these into a test that fails first.

Phase 7 step 02:
- The role loads, and is refused application code.
- Every finance tool refuses every other role, except `farik_read_sheet` for the Product Manager reviewing a finance task.
- A path that climbs out of the finance folder, is absolute, or names another part of `.farik/local` is refused.
- An `.xlsx` with values and formulas survives a write and a read.
- A formula that reaches outside the workbook is refused, and a Stripe-derived cell comes back as a value.
- An overwrite keeps the previous version in `.history/`.
- A finance task with a `command` or `test` criterion, or with an `allowed_paths` entry elsewhere under `.farik/`, fails readiness.
- A finance task reaches `verifying` without a commit and `accepted` without integration, and a task that depends on it is assignable once it is accepted.
- The reviewer's baseline is the copy taken at assignment, not the last overwrite.
- A finance session cannot read the worktrees, the event log, or `settings.json`.
- Stripe's write tools are refused to the role.

Phase 10 step 02:
- The connector hands over only unread messages from approved senders, and never any other.
- The connector has no call that changes the mailbox.
- A filed message is never handed over again.
- Email text and a filed attachment reach the agent under the untrusted-content notice.
- An attachment that is not a PDF or an image, or is over 10 MB, is not filed.
- A receipt-derived cell comes back as a value.
- A sweep runs once a day while a process drives the project, never without an active Finance Specialist and a connected mailbox, and never while a finance task is in progress; no finance task is assigned while a sweep runs.
- A second finance task is not assigned while one is `in_progress` or `verifying`, even under a WIP limit of two or with two Finance Specialists.
- A sweep with no new messages starts no session and still records its events.
- A statement line with no receipt and a receipt with no statement line each show in the reconciliation.
