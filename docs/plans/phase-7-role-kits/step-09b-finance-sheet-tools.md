# Phase 7, step 09b: The Finance Specialist's spreadsheet tools

Status: draft. Its readiness review runs once step 09 has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 6.6, 8.4, 8.6; F17
Depends on: step 09 of this phase (`Role::FinanceSpecialist`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 09 (see step 09's header).

## Goal

The Finance Specialist can read the team's AI spending (`farik_read_costs`), and read and write `.xlsx` workbooks in its private folder, `.farik/local/finance/` (`farik_read_sheet`, `farik_write_sheet`), with every previous version kept and no formula that could reach outside the workbook. The Product Manager can read a workbook when it reviews a finance task. Out of scope: running a finance task in the folder (its working directory, readiness, `verifying`, review baseline, integration; step 09c); Stripe (step 10); receipts (phase 13).

## Decisions

- **Two pinned crates** (code.md: every dependency pinned with `=`), added to `[workspace.dependencies]` and to `farik-runtime` alone: `rust_xlsxwriter = { version = "=0.99.1", default-features = false }` (MIT OR Apache-2.0; writes `.xlsx`) and `calamine = { version = "=0.36.1", default-features = false }` (MIT; reads `.xlsx` and `.csv`, giving a formula's last computed value), both checked on crates.io on 2026-10-05; each pulls `zip` 8.x. No feature is turned on unless a test needs it, and the executor records in the Execution notes any it turned on and why. Rejected: `umya-spreadsheet` (reads and writes, but larger and slower to build); writing CSV only (the founder chose spreadsheets, ADR 0019).
- **The folder** is `<project root>/.farik/local/finance/`, made 0700 when first written, never committed (D5). A tool's `path` is relative to it: 1 to 200 characters, parts matching `^[A-Za-z0-9][A-Za-z0-9 ._-]{0,99}$` (no part starts with a dot, so `.history` is not reachable by name), at most 3 parts, ending `.xlsx` (write) or `.xlsx` or `.csv` (read). The resolved path must lie in the folder after resolving links, and no part of it may be a link (`path_refused`). The tools report no paths to `Call::permit` (as `farik_write_memory` does) and hold the folder line themselves, so `.farik/local/**` stays protected for every other call.
- **Who may call** (`sheet_refused` otherwise), checked in each handler and mirrored in `offered_tools`: `farik_read_costs`, `farik_read_sheet` and `farik_write_sheet` only a Finance Specialist; and `farik_read_sheet` also the active Product Manager in a `verify` session whose contract's `assignee_role` is `finance_specialist` (the reviewer, 5.4). All three are of the `read` tier: each writes only to Farik's own private folder, as `farik_write_memory` does.
- **`farik_write_sheet { path, sheets }`** writes one whole workbook. `sheets` is 1 to 20 of `{ name, columns, rows }`: `name` 1 to 31 characters without `[]:*?/\` (Excel's rule), unique; `columns` 0 to 100 headings; `rows` 0 to 10,000, each 0 to 100 cells. A cell is a JSON number, a string (up to 32,767 characters), a boolean, `null` (empty), `{ "date": "<ISO date>" }`, or `{ "formula": "<text starting with =>" }`. A string is always written as text, never read as a formula, whatever it starts with (`rust_xlsxwriter`'s `write_string`), so a value from Stripe or a receipt that starts with `=`, `+`, `-` or `@` stays inert in the user's spreadsheet program. A formula is refused (`formula_refused`, naming the sheet, row and column) when it reaches outside the workbook: a `[` (an external reference), a `|` (a DDE call such as `=cmd|' /C calc'!A0`), or, matched without regard to case as a whole function name followed by `(`, `HYPERLINK`, `WEBSERVICE`, `FILTERXML`, `IMPORTDATA`, `IMPORTXML`, `IMPORTHTML`, `IMPORTRANGE`, `IMPORTFEED`, `RTD`, `CALL`, `REGISTER.ID`, `EXEC`, `INFO`, `CELL`. Before an overwrite the old file is copied to `.history/<path with / as __>.<UTC yyyymmddThhmmssZ>.xlsx`; the new file is written beside the target and renamed over it, so a reader never sees half a file. It answers `{ path, sheets: [{ name, rows }], replaced: bool }`.
- **`farik_read_sheet { path, sheet?, from_row?, rows? }`** reads a workbook or a CSV: every sheet, or the one named; rows from `from_row` (1-based, default 1), at most `rows` (1 to 500, default 200) per sheet, each cell as a number, a string, a boolean, `null`, `{ "date" }` or `{ "error": "<#VALUE! and the like>" }`; a formula comes back as its last computed value, never its text; `{ sheets: [{ name, rows, total_rows, more }] }`. A file over 10 MiB is refused (`sheet_too_large`). What a workbook holds is the user's and the services', so it reaches the agent inside the untrusted-content notice (8.6).
- **`farik_read_costs { by, from?, to? }`**: `by` is `task`, `agent`, `sprint`, `day` or `purpose`; `from` and `to` ISO dates, both or neither, `from` not after `to`, at most 366 days apart (`cost_range_invalid`). It answers `{ by, rows: [{ key, usd, input_tokens, output_tokens, sessions }] }` from `Projections::costs_for`, through a new `CostWindow::Between(NaiveDate, NaiveDate)` (`day BETWEEN ?1 AND ?2`) beside `Day`, `Sprint` and `All`.
- **The skill** `keeping-the-books` (step 09) gains the three tools: read before you write, so a hand edit is kept; every value from a service or a receipt as a value; formulas only for sums and totals inside the workbook.

## File map

```
Cargo.toml, crates/runtime/Cargo.toml                       modifies: the two crates (Task 2)
crates/store/src/projections.rs                             modifies: CostWindow::Between (Task 1)
crates/runtime/src/tools/costs.rs                           creates: farik_read_costs (Task 1)
crates/runtime/src/tools/sheets.rs                          creates: the folder line, farik_read_sheet, farik_write_sheet (Tasks 2, 3)
crates/runtime/src/tools.rs                                 modifies: TOOLS, call_tool; lists_every_tool_with_its_tier (Tasks 1 to 3)
crates/runtime/src/orchestrator/session.rs                  modifies: offered_tools (Task 4)
crates/roles/roles/finance_specialist/skills/keeping-the-books/SKILL.md   modifies (Task 4)
docs/SPEC.md, docs/plans/project-plan.md                    modifies (Task 5)
```

## Interfaces

Consumes: `Call`, `FarikTool`, `tool`, `ToolError`, `offered_tools`, `SessionAsk` (runtime); `Projections::costs_for`, `CostScope`, `CostWindow`, `CostProjection` (store); `Role::FinanceSpecialist` (step 09).

Produces:

```rust
pub enum CostWindow { Day(NaiveDate), Sprint(String), All, Between(NaiveDate, NaiveDate) }   // farik_store::projections
pub(super) fn read_costs(call: &Call<'_>, input: ReadCostsInput) -> Result<Value, ToolError>;   // tools::costs
pub(crate) fn finance_path(root: &Path, path: &str, write: bool) -> Result<PathBuf, ToolError>;  // tools::sheets
pub(crate) fn formula_reaches_outside(formula: &str) -> Option<&'static str>;                   // tools::sheets, pure
pub(super) fn read_sheet(call: &Call<'_>, input: ReadSheetInput) -> Result<Value, ToolError>;
pub(super) fn write_sheet(call: &Call<'_>, input: WriteSheetInput) -> Result<Value, ToolError>;
```

## Tasks

### Task 1: `farik_read_costs`

- `between_sums_the_days_inclusive` (store): rows on 2026-10-01, 10-02 and 10-04 summed for `Between(10-02, 10-04)` give the two later ones. RED.
- `reads_costs_by_each_scope`: a project with recorded costs answers `by: agent` with one row per agent and `by: day` with one per day. RED.
- `refuses_a_bad_range_and_another_role`: `from` without `to`, `from` after `to`, 367 days, and a Product Manager's call, each refused with its code. RED.

- [ ] `feat(runtime): let the Finance Specialist read the team's AI spending`

### Task 2: The folder line and `farik_write_sheet`

- `writes_a_workbook_that_reads_back`: two sheets of numbers, strings, a date and `=SUM(B2:B3)` read back (through `calamine`) as written. RED.
- `a_string_is_never_a_formula`: `"=1+1"`, `"+cmd"` and `"@SUM(A1)"` as strings read back as those strings, not as formulas or results. RED.
- `refuses_a_formula_that_reaches_outside` (pure, then through the tool): each of `=[book.xlsx]S!A1`, `=cmd|' /C calc'!A0`, `=HYPERLINK("x")`, `=webservice("x")`, `=IMPORTXML("x","y")`, `=INFO("os")`, `=CELL("filename")` is `formula_refused` naming its cell, and nothing is written; `=SUM(A1:A3)` and `=A1*B1` pass. RED.
- `keeps_the_previous_version`: a second write keeps the first under `.history/` with its time, and the target holds the second. RED.
- `holds_the_folder_line`: `../books.xlsx`, `/tmp/x.xlsx`, `.history/x.xlsx`, `a/b/c/d.xlsx`, `x.xlsm`, and a path through a link that points out of the folder are `path_refused`, and nothing is written outside. RED.
- `refuses_another_role`: a Marketing Specialist's call is `sheet_refused`. RED.
- `refuses_a_workbook_out_of_bounds`: 21 sheets, a sheet named `a/b`, two sheets of one name, 10,001 rows: each refused before writing. RED.

- [ ] `feat(runtime): let the Finance Specialist write a workbook in its folder`

### Task 3: `farik_read_sheet`

- `reads_a_page_of_rows`: a 1,000-row sheet read with `from_row: 201, rows: 100` gives rows 201 to 300, `total_rows: 1000`, `more: true`. RED.
- `gives_a_formulas_value_not_its_text`. RED.
- `reads_a_csv`. RED.
- `wraps_what_it_read_as_untrusted`: the answer's cell text sits inside the untrusted-content notice. RED.
- `the_reviewer_may_read_a_finance_tasks_workbook`: the Product Manager in a `verify` session about a task whose `assignee_role` is `finance_specialist` reads; in a `verify` session about a Developer's task, or a `plan` session, it is `sheet_refused`. RED.
- `refuses_a_large_file`: 10 MiB + 1 byte is `sheet_too_large`. RED.

- [ ] `feat(runtime): let the Finance Specialist and its reviewer read a workbook`

### Task 4: Offered to whom, and the skill

- `offers_the_sheet_tools_to_the_finance_specialist_alone`: a Finance Specialist's implement session is offered the three; a Developer's and a Marketing Specialist's none of them; a Product Manager's verify session about a finance task `farik_read_sheet` alone. RED.
- `keeping_the_books_names_only_tools_farik_lists` (the existing guard `kit_skills_name_only_tools_farik_lists` covers role skills too, or this test does it for the role's skill). Guard.

- [ ] `feat(runtime): offer the sheet tools to the Finance Specialist and its reviewer`

### Task 5: Spec and plan

`docs/SPEC.md` 6.6: the three tools as built (limits, refusals, the `.history/` name, the formula list, strings never formulas); 5.6 (tools of the `read` tier writing to Farik's private folder); the revision line. Project plan row 09b.

- [ ] `docs(spec): record the Finance Specialist's spreadsheet tools`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, by the founder: open a workbook the Finance Specialist wrote in Excel, Numbers or LibreOffice, and see its sums work and a pasted `=HYPERLINK` text shown as text.

## Execution notes

None yet.
