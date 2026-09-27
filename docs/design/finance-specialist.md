# The Finance Specialist

Status: approved by the founder on 2026-09-27, in conversation. It is the design input to phase 6 step 01. ADR 0019 records the decision, and spec 0.21 (sections 1 and 6.6) carries its rules.

## Why

The founder wants one agent that is both a financial analyst and an accountant. It keeps track of every cost and expense of the product the team builds: billing, infrastructure, cloud, storage, and the team's own AI spending. It forecasts long-term expenses and runs the books.

Five of the founder's answers shaped the design:
- The agent covers both the product's business finance and the team's own spending.
- It is built before the web UI.
- It is optional: the team builder's default stays five agents, and the cap stays seven.
- The books are private, kept outside the repository.
- The finance documents are spreadsheets, in the finance folder.

## The role

The role is the Finance Specialist, with the id `finance_specialist`.

Mandate:
- Track every cost and expense of the product. That covers the team's AI spending, from Farik's own cost records. It also covers cloud, infrastructure, storage, hosting, domains, software subscriptions, and payment and billing fees, from the bills the user supplies.
- Keep the books: categorise each expense and revenue line, reconcile it against its source document, and close each month.
- Forecast expenses over 12 to 36 months.
- Analyse pricing and unit economics.
- Recommend budgets, in plain words.

What it produces is management accounting, meaning numbers a founder runs the product by. It is not a tax filing, statutory accounts, or financial advice. Its system prompt says so, and so does the team builder's line about the role.

It cannot:
- write application code, or anything in the repository;
- pay, move money, change Farik's budgets, or act on any external account;
- publish anything;
- write anywhere but its finance folder.

The rest of its setup:
- **Tiers:** `read` and `network`, the network to research prices. It has no `write_workspace`, `execute`, or git tier.
- **Reviewer:** the Product Manager, as for the Marketing Specialist.
- **Model:** `claude-sonnet-5` at medium effort, the Marketing Specialist's default. The team file can override it.
- **In the team builder:** it is optional and not among the five suggested. The user adds it with one click, and it takes one of the five extra characters as its avatar (the founder picks which).

## The finance folder

The books live in `.farik/local/finance/`. The folder is on the user's machine, under the `.farik/local/` that Farik already keeps out of git (D5), so it is never committed, even when the repository is public.

```
.farik/local/finance/
  inbox/          bills, invoices and provider exports the user drops in (PDF, CSV, XLSX, images)
  books.xlsx      Expenses, Revenue, Categories, Monthly summary
  forecast.xlsx   the long-term forecast
  <name>.xlsx     further workbooks a task asks for, such as pricing.xlsx
  .history/       the previous version of each workbook, kept on every overwrite
```

The workbooks are `.xlsx`, which Excel, Google Sheets, Numbers and LibreOffice all open. The user may edit them by hand. The agent reads the file as it is before it writes, so a hand edit is kept.

Access:
- Only a Finance Specialist session and the Product Manager, as the reviewer of a finance task, can read the folder.
- No agent may reach anything else under `.farik/local/`.
- Every other agent is refused the folder as it is today: it lies outside a task's worktree, and `.farik/local/**` is one of the default protected paths, which also keeps it from a conversation session in the project root.
- That protection stays for every session. A finance session gets one narrow exception, for `.farik/local/finance/**` alone.

## How a finance task runs

A finance task is an ordinary contract. It is filed, triaged, made ready, assigned, reviewed and accepted, with two differences:
- **Where its session runs.** The session runs with the finance folder as its working directory, not a git worktree, much as a conversation session runs in the project root. Claude Code's built-in `Read`, `Glob` and `Grep` then work on the bills in `inbox/` and nothing outside the folder. The task's `allowed_paths` are under `.farik/local/finance/`, and the Definition of Ready's document-paths rule accepts that folder for this role alone.
- **No branch and nothing to integrate.** The task makes no commits. Once accepted, it goes straight to done, and the integration step is skipped for it.

## Tools

Three new Farik tools. Each handler refuses a caller whose role is not listed, as `farik_write_decision` refuses today.

| Tool | Tier | Roles | What it does |
|---|---|---|---|
| `farik_read_costs` | `read` | Finance Specialist | The team's AI spending, from `Projections::costs`: totals by task, agent, sprint or day, with tokens and session counts, and optionally a date range |
| `farik_read_sheet` | `read` | Finance Specialist, and the Product Manager when it reviews a finance task | One workbook or CSV in the finance folder, as its sheets of rows. Formulas come back with their last computed values |
| `farik_write_sheet` | `read` | Finance Specialist | Writes a whole `.xlsx` workbook in the finance folder: sheets, their columns, and rows of values or formulas. It refuses any other extension or any path outside the folder. Before overwriting a file, it copies the old one to `.history/` |

`farik_write_sheet` needs only the `read` tier. It writes nowhere but the private folder, and the tool itself holds that line, as `farik_write_memory` holds its own.

The explanation for the human goes in the task's completion note, which already opens with a plain-language summary (spec 5.4). No other document is written.

The daemon uses two new dependencies, pinned per the repository's rules: `rust_xlsxwriter` to write workbooks, and `calamine` to read `.xlsx`, `.xls`, `.ods` and CSV.

## The web UI

The team builder and the Team page show the Finance Specialist as an optional role. The mockups gain it in phase 6. A finance page that shows the books in the browser, and an upload button for `inbox/`, are not in this step. They come when the web UI's steps are planned, or later, if the founder asks.

## Tests

The step plan turns each of these into a test that fails first:
- The role loads, and is refused application code.
- Every new tool refuses every other role.
- A path that climbs out of the finance folder, is absolute, or names another part of `.farik/local` is refused.
- An `.xlsx` with values and formulas survives a write and a read.
- An overwrite keeps the previous version in `.history/`.
- A finance task goes from accepted to done without integration.
- A finance session cannot read the worktrees, the event log, or `settings.json`.
