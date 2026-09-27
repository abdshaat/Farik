# The Finance Specialist

Status: approved by the founder on 2026-09-27, in conversation, and revised the same day to take receipts from email and read Stripe. It is the design input to phase 7 steps 02 and 03. ADR 0019 records the decision, and spec 0.21 (sections 1 and 6.6, with exceptions in 5.2, 5.3, 5.4, 5.5, 5.6, 5.8 and 5.14) carries its rules.

## Why

The founder wants one agent that is both a financial analyst and an accountant. It keeps track of every cost and expense of the product the team builds: billing, infrastructure, cloud, storage, and the team's own AI spending. It forecasts long-term expenses and runs the books.

The founder's answers shaped the design:
- The agent covers both the product's business finance and the team's own spending.
- It is optional: the team builder's default stays five agents, and the cap stays seven.
- The books are private, kept outside the repository.
- The finance documents are spreadsheets.
- Receipts come from email alone; the user never uploads a bill. The agent reads the user's main mailbox, filtered, from Gmail or Google Workspace and from Outlook or Microsoft 365.
- It reads Stripe, read-only, when the product uses Stripe.
- It is built in phase 7, after MCP connections exist, because Stripe needs them and the email connector shares their credential store.

## The role

The role is the Finance Specialist, with the id `finance_specialist`.

Mandate:
- Track every cost and expense of the product. That covers the team's AI spending, from Farik's own cost records. It also covers everything the user's receipts show: cloud, infrastructure, storage, hosting, domains, software subscriptions, and payment and billing fees. It also covers revenue, fees, payouts and refunds, from Stripe.
- Keep the books: take each receipt from the mailbox, categorise it, and record it. Then reconcile, and close each month.
- Forecast expenses over 12 to 36 months.
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
- **In the team builder:** it is optional and not among the five suggested. The user adds it with one click, and it takes one of the five extra characters as its avatar (the founder picks which).

## The finance folder

The books live in `.farik/local/finance/`. The folder is on the user's machine, under the `.farik/local/` that Farik already keeps out of git (D5), so it is never committed, even when the repository is public.

```
.farik/local/finance/
  receipts/<yyyy-mm>/  each receipt filed from the mailbox: its attachment (PDF or image), or the email itself when it has none
  books.xlsx           Expenses, Revenue, Categories, Monthly summary; each row names its receipt file or Stripe object
  forecast.xlsx        the long-term forecast
  <name>.xlsx          further workbooks a task asks for, such as pricing.xlsx
  .history/            the previous version of each workbook, kept on every overwrite, and under <task-id>/ the copy of every workbook taken when a finance task is assigned, the baseline its reviewer compares against
```

The workbooks are `.xlsx`, which Excel, Google Sheets, Numbers and LibreOffice all open. The user may edit them by hand. The agent reads the file as it is before it writes, so a hand edit is kept.

Access:
- Only a Finance Specialist session and the Product Manager, as the reviewer of a finance task, can read the folder.
- Every other agent is refused the folder as it is today: it lies outside a task's worktree, and `.farik/local/**` is one of the default protected paths, which also keeps it from a conversation session in the project root.
- That protection stays for every session. A finance session gets one narrow exception, for `.farik/local/finance/**` alone.

## Receipts from email

Email goes through a read-only connector of Farik's own, not a general mail MCP server. A general server would let the agent search the whole mailbox; with Farik's own connector, the filter is enforced by code the agent cannot talk its way around.

- **Connecting.** The user presses "Connect Google" or "Connect Microsoft" and signs in. Farik asks for read-only mail access and nothing else: Gmail's `gmail.readonly` scope, or Microsoft Graph's `Mail.Read`. The sign-in is OAuth for an installed app: PKCE, with the redirect to the local daemon on 127.0.0.1. The tokens are kept in the OS keychain, where phase 7 step 01 keeps MCP credentials.
- **The filter.** The user picks a Gmail label or an Outlook folder, such as "Receipts", and may add a list of senders. Farik asks the provider only for messages that match. The agent never sees any other mail.
- **Nothing in the mailbox changes.** The connector cannot send, delete, move, label, or mark a message as read.
- **No duplicates.** Farik remembers each message the agent has filed and never hands it over again.
- **Untrusted content.** A message's text, and every attachment it files, reaches the agent under the untrusted-content notice of spec 8.6, since anyone can send an email that tries to instruct an agent. An attachment is filed only if it is a PDF or an image and at most 10 MB.
- **The client id.** OAuth for an installed app ships a client id in the source. The providers define it as public, and no client secret exists, so the repository still holds no secret.

Two Farik tools, which only this role may call:

| Tool | What it does |
|---|---|
| `farik_read_receipts` | The matching messages not yet filed: sender, date, subject, text, and the names and types of their attachments |
| `farik_file_receipt` | Files one message. Either it saves the receipt (the attachment it names, or the email itself) under `receipts/<yyyy-mm>/` and marks the message `recorded`, or it marks the message `not_a_receipt`. Either way, the message is not handed over again |

The agent then writes the expense into `books.xlsx`, with the path of the receipt file on its row.

## Stripe

Stripe is read through Stripe's official MCP server, configured for this agent the way phase 7 step 01 configures any MCP server. Read-only is locked twice:
- **At Stripe.** The setup guides the user to create a restricted key with read permissions only, so Stripe itself refuses a write.
- **In Farik.** Only Stripe's read tools are tagged `read`. Every other tool keeps the default `external_effect`, which this role is never granted.

Stripe is optional. A product that takes no payments through Stripe skips it.

## When it works

- **The receipts sweep.** While a process drives the project, Farik starts a receipts sweep once a day, as a tick rule like the standup. The connector fetches new messages first; when there are none, no session starts and nothing is spent. Otherwise a sweep is a short Finance Specialist session that reads the new receipts, files each one, and records it in `books.xlsx`. Its cost goes under a purpose of its own, `finance`. `sweep.started` and `sweep.ended` are recorded even when nothing is new, so the daily rule knows when it last ran. The user can also start one: "Check now" in the web app, or `farik finance sweep`. A sweep counts against the spending limits like any session. It runs only when the team has an active Finance Specialist and a connected mailbox.
- **Tasks.** Everything else comes as an ordinary contract: a month's close, a forecast, or a pricing analysis. It is filed, triaged, made ready, assigned, reviewed and accepted. A finance task's session runs in the finance folder instead of a git worktree, much as a conversation session runs in the project root, so its built-in `Read`, `Glob` and `Grep` reach the receipts and nothing else. The task's `allowed_paths` are under the folder. Two readiness rules stand in the way today, and each gets one exception for this role, recorded in spec 5.3: the document-paths rule, and the rule that no `allowed_paths` entry reaches under `.farik/` at all (`no_farik_paths`). The task may carry no `command` or `test` criterion, since it has no worktree to run one in; its criteria are `artifact`, `review` and `human`. It makes no commits, so three more rules take an exception. When the task is assigned, Farik copies every workbook to `.history/<task-id>/`. It reaches `verifying` when every workbook in the `workbooks` list the assignee gives `farik_request_transition` exists (spec 5.2). Its reviewer receives each changed workbook beside that copy in place of a diff (spec 5.4). Once `accepted` it is finished, with nothing to integrate, and it counts as integrated for any task that depends on it (spec 5.14). Only one piece of finance work touches the folder at a time: no sweep starts while a finance task is `in_progress` or `verifying`, and no finance task is assigned while a sweep runs, so the folder changes under one session only, as a worktree does for code.

## Other tools

Three more Farik tools, which only this role may call:

| Tool | What it does |
|---|---|
| `farik_read_costs` | The team's AI spending, from `Projections::costs`: totals by task, agent, sprint or day, with tokens and session counts, and optionally a date range |
| `farik_read_sheet` | One workbook or CSV in the finance folder, as its sheets of rows. Formulas come back with their last computed values. The Product Manager may also call it when it reviews a finance task |
| `farik_write_sheet` | Writes a whole `.xlsx` workbook in the finance folder: sheets, their columns, and rows of values or formulas. It refuses any other extension or any path outside the folder, and any formula that reaches outside the workbook: external references, `HYPERLINK`, `WEBSERVICE`, `IMPORTDATA` and its kin, `RTD`, DDE. A cell whose value came from a receipt, an email or Stripe is written as a value, never a formula, because the founder opens the workbook in Excel and a formula runs there. Before overwriting a file, it copies the old one to `.history/` |

Every Farik tool in this design has the `read` tier. Each writes only to Farik's private folder, and the tool itself holds that line, as `farik_write_memory` does.

The explanation for the human goes in the task's completion note, which already opens with a plain-language summary (spec 5.4).

New dependencies, each pinned per the repository's rules and named in the step plans:
- `rust_xlsxwriter` to write workbooks, and `calamine` to read them;
- an OAuth client and HTTP calls to the Gmail and Microsoft Graph APIs.

## Provider approval

Google classes read access to a mailbox as a restricted scope. Until Google has verified Farik's OAuth client, only up to 100 test users listed on that client can connect Gmail, and for a public app a paid third-party security assessment, repeated every year, is likely required. Microsoft asks for publisher verification. Both take weeks, and both need the founder's accounts. So the applications start when phase 7 is planned, not at launch. Until approval comes, the web launch can ship with Gmail limited to test users.

## The web UI

The team builder and the Team page show the Finance Specialist as an optional role. Adding it opens a short setup:
1. connect the mailbox and pick the label or folder;
2. optionally, add a Stripe read-only key.

A page that shows the books in the browser is not in these steps.

## Tests

The step plans turn each of these into a test that fails first:
- The role loads, and is refused application code.
- Every finance tool refuses every other role, except `farik_read_sheet` for the Product Manager reviewing a finance task.
- A path that climbs out of the finance folder, is absolute, or names another part of `.farik/local` is refused.
- An `.xlsx` with values and formulas survives a write and a read.
- A formula that reaches outside the workbook is refused, and a receipt-derived cell comes back as a value.
- An attachment that is not a PDF or an image, or is over 10 MB, is not filed.
- A sweep with no new messages starts no session.
- An overwrite keeps the previous version in `.history/`.
- A finance session cannot read the worktrees, the event log, or `settings.json`.
- The connector asks only for messages that match the filter, and never for any other.
- A finance task with a `command` or `test` criterion, or with an `allowed_paths` entry elsewhere under `.farik/`, fails readiness.
- A finance task reaches `verifying` without a commit and `accepted` without integration, and a task that depends on it is assignable once it is accepted.
- No sweep starts while a finance task is in progress, and no finance task is assigned while a sweep runs.
- The reviewer's baseline is the copy taken at assignment, not the last overwrite.
- The connector has no call that changes the mailbox.
- A filed message is never handed over again.
- Email text reaches the agent under the untrusted-content notice.
- Stripe's write tools are refused to the role.
- A sweep runs once a day while Farik runs, and never without an active Finance Specialist and a connected mailbox.
