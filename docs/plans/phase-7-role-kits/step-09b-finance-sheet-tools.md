# Phase 7, step 09b: The Finance Specialist's spreadsheet tools

Status: executed 2026-10-06; reviewed 2026-10-06 (one landing review, one fix report).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 6.6, 8.4, 8.6; F17
Depends on: step 09 of this phase (`Role::FinanceSpecialist`); phase 6 (merged in #19)
Readiness confirmed by: a fresh Opus session, 2026-10-05 (one round, against `docs/standards/workflow.md` stage 2), with steps 09 and 09c; folded below

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 09 (see step 09's header).

## Goal

The Finance Specialist can read the team's AI spending (`farik_read_costs`), and read and write `.xlsx` workbooks in its private folder, `.farik/local/finance/` (`farik_read_sheet`, `farik_write_sheet`), with every previous version kept and no formula that could reach outside the workbook. The Product Manager can read a workbook when it reviews or accepts a finance task. Out of scope: running a finance task in the folder (its working directory, readiness, `verifying`, review baseline, integration; step 09c); Stripe (step 10); receipts and CSV statements (phase 13, which adds CSV reading with `imports/`).

## Decisions

- **Two pinned crates** (code.md: every dependency pinned with `=`), added to `[workspace.dependencies]` and to `farik-runtime` alone: `rust_xlsxwriter = { version = "=0.99.1", default-features = false }` (MIT OR Apache-2.0; writes `.xlsx`) and `calamine = { version = "=0.36.1", default-features = false }` (MIT; reads `.xlsx`, giving a formula's text and its stored value), both checked on crates.io on 2026-10-05; each pulls `zip` 8.x. No feature is needed: `calamine` gives dates through `ExcelDateTime::to_ymd_hms_milli()`, and `rust_xlsxwriter` writes them with `ExcelDateTime::parse_from_str` and a `yyyy-mm-dd` number format, so `calamine` reads them back as dates. Rejected: `umya-spreadsheet` (reads and writes, but larger and slower to build); writing CSV only (the founder chose spreadsheets, ADR 0019); reading CSV now (`calamine` has no CSV reader, and no CSV is in the folder before phase 13's `imports/`).
- **The folder is keyed by role from this step** (so step 10b adds the procurement folder with one arm): `private_folder(role: Role) -> Option<&'static str>` in `farik-core`'s `team.rs`, `Some(".farik/local/finance")` for `FinanceSpecialist` and `None` for every other role. The folder is made 0700 when first written (as `skills.rs:825-829` does), never committed (D5). A tool's `path` is relative to the caller's folder: 1 to 200 characters, parts matching `^[A-Za-z0-9][A-Za-z0-9 ._-]{0,99}$` (no part starts with a dot, so `.history` is not reachable by name), at most 3 parts, ending `.xlsx` in lower case. The resolved path must lie in the folder after resolving links, and no part of it may be a link (`private_path_refused`, the code step 10b uses). The tools report no paths to `Call::permit` (as `farik_write_memory` does) and hold the folder line themselves, so `.farik/local/**` stays protected for every other call.
- **Who may call** (`sheet_refused` otherwise), checked in each handler and mirrored in `offered_tools`:
  - `farik_read_costs`: a Finance Specialist, in any session.
  - `farik_write_sheet`: a Finance Specialist in an `implement` session whose task's assignee is the caller, so a chat or a conversation never writes the books and step 09c's one-task-at-a-time rule and its baseline hold (as step 10b's `farik_write_evaluation`).
  - `farik_read_sheet`: a Finance Specialist in any session; and, against the reviewed task's assignee's folder, the caller who is a private-folder task's `reviewer` in its `verify` session, or the Product Manager in that task's accept session (`verify.rs:544`).
  - All three are of the `read` tier: each writes only to Farik's own private folder, as `farik_write_memory` does.
- **`farik_write_sheet { path, sheets }`** writes one whole workbook. `sheets` is 1 to 20 of `{ name, columns, rows }`: `name` passes `rust_xlsxwriter::utility::check_sheet_name` (1 to 31 characters, no `[]:*?/\`, no leading or trailing apostrophe, not "History") and holds no `|`, and the names are unique ignoring case; `columns` 0 to 100 headings; `rows` 0 to 10,000, each 0 to 100 cells. A cell is a JSON number, a string (up to 32,767 characters), a boolean, `null` (empty), `{ "date": "<ISO date>" }`, or `{ "formula": "<text starting with =>" }`. A string is always written as text (`write_string`), never read as a formula, whatever it starts with, so a value from Stripe or a receipt that starts with `=`, `+`, `-` or `@` stays inert in the user's spreadsheet program. Every sheet is written with `set_formula_result_default("")`, so a formula's stored value is empty until a spreadsheet program computes it, never a misleading 0.
- **A formula is refused** (`formula_refused`, naming the sheet, row and column, and nothing is written) when it reaches outside the workbook or sends its text away: it holds a `[` (an external reference) or a `|` (a DDE call such as `=cmd|' /C calc'!A0`), or it calls one of these functions: `HYPERLINK`, `WEBSERVICE`, `FILTERXML`, `IMPORTDATA`, `IMPORTXML`, `IMPORTHTML`, `IMPORTRANGE`, `IMPORTFEED`, `IMAGE`, `RTD`, `DDE`, `CALL`, `REGISTER.ID`, `EXEC`, `INFO`, `CELL`, `COPILOT`, `TRANSLATE`, `DETECTLANGUAGE`. "Calls" means: the name, matched without regard to case, preceded by the start of the formula, by a character outside `[A-Za-z0-9_.]`, or by `_xlfn.` or `_xlws.`, then optional spaces, then `(`. A name inside a string literal is refused too, which is safe; a cell reference cannot spell one, since columns end at `XFD`.
- **Writing.** Before an overwrite the old file is copied to `.history/<path with / as __>.<UTC yyyymmddThhmmssZ>.xlsx`, with `-<n>` added when that name exists (two writes in one second). The new file is written beside the target and renamed over it, so a reader never sees half a file; a written file over 10 MiB is refused `sheet_too_large` before the rename, and the target is left as it was. It answers `{ path, sheets: [{ name, rows }], replaced: bool }`. The writer is `pub(crate) fn write_workbook(path: &Path, sheets: &[SheetInput]) -> Result<WrittenWorkbook, ToolError>` beside the handler, so step 10c's purchase orders reuse it outside a tool call.
- **`farik_read_sheet { path, sheet?, from_row?, rows? }`** reads a workbook: every sheet, or the one named; rows from `from_row` (1-based, default 1), at most `rows` (1 to 500, default 200) per sheet, each cell as a number, a string, a boolean, `null`, `{ "date" }` or `{ "error": "<#VALUE! and the like>" }`; a formula cell as `{ "formula": "<text>", "value": <its stored value, or null> }`, read with `worksheet_formula` and `worksheet_range` (the value is what a spreadsheet program last computed; a workbook only Farik wrote has none). A file over 10 MiB is refused (`sheet_too_large`). What a workbook holds is the user's and the services', so the answer is `{ sheets: "<untrusted block>", more: bool }`: the JSON of `[{ name, rows, total_rows, more }]` wrapped whole by `untrusted_block` (`prompt.rs:246`) with source `sheet`, cut at 256 KiB, `more` true when cut or when any sheet has more rows (8.6).
- **`farik_read_costs { by, from?, to? }`**: `by` is `task`, `agent`, `sprint`, `day` or `purpose`; `from` and `to` ISO dates, both or neither, `from` not after `to`, at most 366 days apart (`cost_range_invalid`). It answers `{ by, rows: [{ key, usd, input_tokens, output_tokens, sessions }] }` from `Projections::costs_for`, through a new `CostWindow::Between(NaiveDate, NaiveDate)` (`day BETWEEN ? AND ?`) beside `Day`, `Sprint` and `All`; under the task scope `?1` is already the number offset (`projections.rs:302`), so `Between` binds `?2` and `?3` there.
- **Where the tools sit.** `TOOLS` gains the three after `farik_chat_reply`, and `lists_every_tool_with_its_tier` (`tools.rs:645`) pins them there, its slice growing from `[..26]` to `[..29]`.
- **The skill** `keeping-the-books` (step 09) gains the three tools: read before you write, so a hand edit is kept; every value from a service or a receipt as a value; formulas only for sums and totals inside the workbook, and a total the reviewer must check also written as a value beside it, since Farik never computes a formula.

## File map

```
Cargo.toml, crates/runtime/Cargo.toml                       modifies: the two crates (Task 2)
crates/store/src/projections.rs                             modifies: CostWindow::Between (Task 1)
crates/runtime/src/tools/costs.rs                           creates: farik_read_costs (Task 1)
crates/core/src/team.rs                                     modifies: private_folder (Task 2)
crates/runtime/src/tools/sheets.rs                          creates: the folder line, write_workbook, farik_write_sheet, farik_read_sheet (Tasks 2, 3)
crates/runtime/src/tools.rs                                 modifies: TOOLS, call_tool; lists_every_tool_with_its_tier (Tasks 1 to 3)
crates/runtime/src/orchestrator/session.rs                  modifies: offered_tools (Task 4)
crates/roles/roles/finance_specialist/skills/keeping-the-books/SKILL.md   modifies (Task 4)
docs/SPEC.md, docs/plans/project-plan.md                    modifies (Task 5)
```

## Interfaces

Consumes: `Call`, `FarikTool`, `tool`, `ToolError`, `offered_tools`, `SessionAsk`, `untrusted_block` (runtime); `Projections::costs_for`, `CostScope`, `CostWindow`, `CostProjection` (store); `Role::FinanceSpecialist` (step 09).

Produces:

```rust
pub fn private_folder(role: Role) -> Option<&'static str>;                                      // farik_core::team
pub enum CostWindow { Day(NaiveDate), Sprint(String), All, Between(NaiveDate, NaiveDate) }      // farik_store::projections
pub(super) fn read_costs(call: &Call<'_>, input: ReadCostsInput) -> Result<Value, ToolError>;   // tools::costs
pub(crate) fn private_path(root: &Path, folder: &str, path: &str) -> Result<PathBuf, ToolError>; // tools::sheets
pub(crate) fn formula_reaches_outside(formula: &str) -> Option<&'static str>;                   // tools::sheets, pure
pub(crate) fn write_workbook(path: &Path, sheets: &[SheetInput]) -> Result<WrittenWorkbook, ToolError>;
pub(super) fn read_sheet(call: &Call<'_>, input: ReadSheetInput) -> Result<Value, ToolError>;
pub(super) fn write_sheet(call: &Call<'_>, input: WriteSheetInput) -> Result<Value, ToolError>;
```

## Tasks

### Task 1: `farik_read_costs`

- `between_sums_the_days_inclusive` (store): rows on 2026-10-01, 10-02 and 10-04 summed for `Between(10-02, 10-04)` give the two later ones, under the project and the task scope. RED.
- `reads_costs_by_each_scope`: a project with recorded costs answers `by: agent` with one row per agent and `by: day` with one per day. RED.
- `refuses_a_bad_range_and_another_role`: `from` without `to`, `from` after `to`, 367 days, and a Product Manager's call, each refused with its code. RED.

- [x] `feat(runtime): let the Finance Specialist read the team's AI spending`

### Task 2: The folder line and `farik_write_sheet`

- `only_the_finance_specialist_has_a_folder` (core): `private_folder` is `Some(".farik/local/finance")` for it and `None` for every other role. RED.
- `writes_a_workbook_that_reads_back`: two sheets of numbers, strings, a date and `=SUM(B2:B3)` read back through `calamine` as written: the date as a date, the formula's text as written and its stored value empty. RED.
- `a_string_is_never_a_formula`: `"=1+1"`, `"+cmd"` and `"@SUM(A1)"` as strings read back as those strings, with no formula. RED.
- `refuses_a_formula_that_reaches_outside` (pure, then through the tool): each of `=[book.xlsx]S!A1`, `=cmd|' /C calc'!A0`, `=HYPERLINK("x")`, `=webservice("x")`, `=IMPORTXML("x","y")`, `=IMAGE("https://x")`, `=DDE("a","b","c")`, `=_xlfn.WEBSERVICE("x")`, `=INFO ("os")`, `=CELL("filename")` and `=COPILOT("x")` is `formula_refused` naming its cell, and nothing is written; `=SUM(A1:A3)`, `=A1*B1` and `=MYCELLS(1)` pass. RED.
- `keeps_the_previous_version`: a second write keeps the first under `.history/` with its time, a third in the same second gets `-1`, and the target holds the last. RED.
- `holds_the_folder_line`: `../books.xlsx`, `/tmp/x.xlsx`, `.history/x.xlsx`, `a/b/c/d.xlsx`, `x.xlsm`, `x.XLSX`, and a path through a link that points out of the folder are `private_path_refused`, and nothing is written outside. RED.
- `writes_only_in_its_own_task`: a Marketing Specialist's call, and a Finance Specialist's in a chat and in an `implement` session of a task assigned to another agent, are `sheet_refused`. RED.
- `refuses_a_workbook_out_of_bounds`: 21 sheets, a sheet named `a/b`, `a|b` and `History`, two sheets named `Books` and `books`, 10,001 rows: each refused before writing. RED.
- `refuses_a_workbook_over_ten_mebibytes`: a write whose file would pass 10 MiB is `sheet_too_large`, and the old target is unchanged. RED.

- [x] `feat(runtime): let the Finance Specialist write a workbook in its folder`

### Task 3: `farik_read_sheet`

- `reads_a_page_of_rows`: a 1,000-row sheet read with `from_row: 201, rows: 100` gives rows 201 to 300, `total_rows: 1000`, `more: true`. RED.
- `gives_a_formulas_text_and_stored_value`: a workbook Farik wrote gives `{ formula: "=SUM(B2:B3)", value: null }`; one written in the test with `Formula::set_result("5")`, as a spreadsheet program saves a computed value, gives `value: 5`. RED.
- `wraps_what_it_read_as_untrusted`: the answer's `sheets` is one untrusted block of source `sheet`, and a 300 KiB sheet is cut at 256 KiB with `more: true`. RED.
- `the_reviewer_may_read_a_finance_tasks_workbook`: the reviewer in a `verify` session about a task whose `assignee_role` is `finance_specialist`, and the Product Manager in its accept session, read the assignee's folder; in a `verify` session about a Developer's task, or a `plan` session, it is `sheet_refused`. RED.
- `refuses_a_large_file`: 10 MiB + 1 byte is `sheet_too_large`. RED.

- [x] `feat(runtime): let the Finance Specialist and its reviewer read a workbook`

### Task 4: Offered to whom, and the skill

- `offers_the_sheet_tools_to_the_finance_specialist_alone`: a Finance Specialist's implement session of its own task is offered the three; its chat `farik_read_costs` and `farik_read_sheet` alone; a Developer's and a Marketing Specialist's none of them; a Product Manager's verify session about a finance task `farik_read_sheet` alone. RED.
- `kit_skills_name_only_tools_farik_lists` (step 09 extended it to role skills): `keeping-the-books` names the three, which Farik now lists. Guard.

- [x] `feat(runtime): offer the sheet tools to the Finance Specialist and its reviewer`

### Task 5: Spec and plan

`docs/SPEC.md` 6.6: the three tools as built (limits, refusals, the `.history/` name, the formula list and what "calls" means, strings never formulas, a formula read back as its text and stored value, `farik_write_sheet` only in the role's own task, no CSV until phase 13); 5.6 (tools of the `read` tier writing to Farik's private folder); the revision line. Project plan row 09b.

- [x] `docs(spec): record the Finance Specialist's spreadsheet tools`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, by the founder: open a workbook the Finance Specialist wrote in Excel, Numbers or LibreOffice, and see its sums work and a pasted `=HYPERLINK` text shown as text.

## Execution notes

- Task 1. `read_costs` takes `&ReadCostsInput`, not `ReadCostsInput` by value as Interfaces says: clippy's `needless_pass_by_value` (pedantic, `-D warnings`) refuses a by-value input the body only reads. `daemon/mcp.rs` (`lists_every_farik_tool_and_the_permission_tool`) pins the number of tools (32); it went to 33 with this commit. Refusals of the three tools and of `farik_read_costs` share one `Refusal::Finance { code, detail }`, as `Refusal::MarketingPlan` carries its code, so the five codes of this step (`sheet_refused`, `cost_range_invalid`, `formula_refused`, `sheet_too_large`, `private_path_refused`) are one variant and one match arm.
- Task 2. `write_workbook` takes `folder` and `now` besides the path and the sheets (`write_workbook(folder, path, sheets, now)`): `.history/` lies at the folder's root and its names carry the folder-relative path and the clock's time, which a path alone has neither of, and a test on the fixed clock needs the third write of a second to get `-1` without waiting for a real second. `write_sheet` takes `&WriteSheetInput` (clippy, as Task 1). The `.history/` name is read literally: the path keeps its `.xlsx` (`books.xlsx.20260922T120000Z.xlsx`, then `books.xlsx.20260922T120000Z-1.xlsx`; `2026/pricing.xlsx` is `2026__pricing.xlsx...`). The pure half of `refuses_a_formula_that_reaches_outside` is also its own test, `names_what_a_formula_may_not_reach`, which needs no git program and so is not ignored. Faults the plan leaves unnamed are `sheet_refused`, except a formula that does not start with `=`, which is `formula_refused`; a date is checked as `YYYY-MM-DD` and then passed to `ExcelDateTime::parse_from_str` (so 1900 on). A refusal names a cell as `Books!B3 (row 3, column 2)`, rows counted from the headings row when there are headings. `farik_write_sheet` is refused to any role but the Finance Specialist by name (`private_folder` of a later role does not give it the tool). `daemon/mcp.rs` pinned 33 tools; it is 34. Mutations proved, each reverted: no `_XLFN.` prefix in `calls` fails the formula tests; no link check and no containment check fails `holds_the_folder_line`; no `keep_previous` fails `keeps_the_previous_version`; writing a `=` string as a formula fails `a_string_is_never_a_formula`. `cargo build --offline` cannot resolve `calamine`'s own dependencies (`atoi_simd` and the rest are not in the local index); with the network they resolved, and `Cargo.lock` gained 13 packages and no change to an existing one; both pins are as the plan says.
- Task 3. `read_sheet` takes `&ReadSheetInput`. The tools sit in the order they were built, `farik_read_costs`, `farik_write_sheet`, `farik_read_sheet`, after `farik_chat_reply`, so the slice is `[..29]`; `daemon/mcp.rs` pinned 34 and is 35. Row numbers are the sheet's own, counted from 1 with the headings as row 1, and a row's cells end at its last cell that holds something, so a row is `[]` when it is empty and a formula's column keeps its place; `total_rows` counts down to the last row holding anything. A whole number comes back as an integer (`5`, not `5.0`), a time is `YYYY-MM-DDTHH:MM:SS` when it has one, and a duration is its number of days. Besides the plan's tests, `reads_a_date_a_boolean_and_an_empty_cell` holds the cell kinds the plan's tests do not reach, and `gives_a_formulas_text_and_stored_value` also covers a formula whose stored value is an error (`{ "error": "#DIV/0!" }`). The page stops adding rows once the answer's JSON passes 256 KiB, so a sheet cut that way has `more: true` and a JSON that is whole; `untrusted_block`'s own cut at 256 KiB, which can cut the JSON mid-way, is what `more` also reads. Mutations proved, each reverted: the Product Manager clause, the reviewer clause, the verify-session clause and the assignee role of `folder_to_read` each fail `the_reviewer_may_read_a_finance_tasks_workbook`; a size check one byte late fails `refuses_a_large_file`; whole numbers kept as floats fail three tests.
- Task 4. `offered_tools` cannot read the assignee from `ask.contract` (the contract file's `assignee` is not kept up to date; the board's row is the truth, 8.4), so `farik_write_sheet` is offered to the Finance Specialist in an `implement` session of a task, and the handler holds the rule that the task's assignee is the caller; the orchestrator runs an implement session for the assignee only. `farik_read_sheet` is offered to the Finance Specialist in any session and to any verify session about a task whose `assignee_role` has a private folder (the reviewer's or the Product Manager's accept), the handler holding which of them may read. `orchestrator/rules.rs`'s `gives_a_session_the_farik_tools_of_its_tiers` listed every Read-tier tool a Product Manager is offered and now leaves the three out. The plan's two tests are `offers_the_sheet_tools_to_the_finance_specialist_alone` and the guard `kit_skills_name_only_tools_farik_lists`; a third, `the_books_skill_names_the_spending_and_sheet_tools` (farik-roles, written first and failing on the skill as it was), holds that the skill names the three and keeps the three rules, which the guard does not (it checks only that a name is listed). The skill's text could not name a tool that is not listed, and it avoids the characters the skill check reads as a file attachment (an at sign after a backtick), so it names the four starting characters in words. Guard proved by a temporary change of `TOOLS` (never of the skill): `farik_read_sheet` listed as `farik_read_sheets` fails `kit_skills_name_only_tools_farik_lists` with `finance_specialist/keeping-the-books: farik_read_sheet`; reverted. Mutations of `offered_tools` proved, each reverted: the implement check of `farik_write_sheet`, the role check of `farik_read_costs`, and the verify clause of `farik_read_sheet` each fail the offering test.
- Task 5. Spec revision 0.56 (the latest was 0.55): 6.6 gains the as-built paragraphs (the folder line, the three tools, who may call and is offered each) in place of its formula paragraph, and its default-tools and as-built-so-far sentences are brought to match; 5.6 gains one paragraph on a `read` tool writing to a private folder. The project plan's row 09b records the step as executed.
- The landing review's fixes (2026-10-06; one commit each, after Task 5; every test of them written first and watched fail, bar the two proved by mutation).
  - F1, `fix(runtime): refuse a link anywhere on a finance workbook's path`. `private_path` walks with `symlink_metadata` from the project root through each part of the folder and then each part of the path, so a link at `.farik`, `.farik/local` or `.farik/local/finance` is refused as one inside is; containment is checked against the resolved root joined with the folder, and a folder not made yet passes, since the deepest part there is lies above it. `keep_previous` refuses a `.history` that is a link, `private_path_refused`. Tests: `refuses_a_link_at_or_above_the_folder` (the folder a link, on a write and on a read; `.farik` and `.farik/local` links through `private_path` on a scratch root, since the fixture's `.farik/local` holds the event log) and `refuses_a_history_that_is_a_link`, each RED first; the `alias -> inner` case is in `holds_the_folder_line` and was already green (the walk of the path's parts refused it), so it is a guard, proved by a mutation that stops the walk at the folder: `alias/x.xlsx` is written. The two new tests sit beside `holds_the_folder_line`, not in it, since each needs a project of its own.
  - F2, `test(runtime): refuse another role's workbook write in its own task`. `writes_only_in_its_own_task` gains the Marketing Specialist assigned its own task, FRK-4, in its implement session; the case it had was FRK-1, assigned to the Finance Specialist, so the assignee check held and the role check of `folder_to_write` never did. Proved by letting any role write, which writes; reverted.
  - F3, `fix(runtime): refuse a formula that passes an outside function by name`. `calls` is `uses`: `_XLETA.` and `_XLUDF.` are prefixes beside `_XLFN.` and `_XLWS.`; a listed name counts when followed, after optional white space, by `(`, `)`, `,` or the end of the formula, and not by anything else, so a digit, a letter or `:` after it makes it a cell or a name of its own; `PY`, `STOCKHISTORY`, `GOOGLETRANSLATE`, `GOOGLEFINANCE` and `AI` are listed (24 names). One thing beyond the planner's rule: a name with no call after `:` or `$` is a column, which `=SUM(RTD:RTD)` needs to pass (its second `RTD` is followed by `)`), and so `=SUM($AI:$AI)` passes; `=SUM(A1:INFO("os"))` is refused, being a call. The refusal reads "it uses INFO". The pure test lists every miss at once.
  - F4, `test(runtime): prove a workbook is renamed into place`. `keeps_the_previous_version` asserts the target's inode changes between two writes. Proved by writing in place, which keeps one inode; reverted.
  - F5, `fix(runtime): keep a workbook's sheet names inside the untrusted notice`. Both reasons quote a name through `untrusted_block("sheet", ...)`, cut at 2 KiB (`NAMES_CAP`): the list of names when the sheet asked for is not there, and the name of a sheet that cannot be read. `keeps_a_sheets_name_inside_the_untrusted_notice` reaches the second by flipping bytes in the first sheet's compressed XML, which leaves the workbook opening and the sheet unreadable. Left as it is, for the planner: calamine's own error text in those two reasons, and in "is not a workbook", can carry text of the file (an attribute value, a relationship type) and is not wrapped.
  - Q2, `fix(runtime): name each workbook's history copy uniquely`. `history_name` writes `%` as `%25` and `/` as `%2F`, in place of `/` as `__`, which gave `a/b.xlsx` and `a__b.xlsx` one name: Task 2's note above (`2026__pricing.xlsx...`) is now `2026%2Fpricing.xlsx...`. `%` is not a character a path holds, so `%25` cannot arise through the tools and is tested on the function alone; `names_each_workbooks_history_uniquely` writes `a/b/c.xlsx`, `a__b/c.xlsx`, `a/b__c.xlsx` and `a__b__c.xlsx` twice each, which the old rule gave one name and `-1` to `-3`.
- Planner's notes from the review, for later steps.
  - Review deviation 3 (Task 2's `write_workbook(folder, path, sheets, now)` in place of the plan's `write_workbook(path, sheets)`): `SheetInput`'s fields are private and it is built only by deserializing, so a caller outside a tool call needs a public constructor, which step 10c adds; and `write_workbook` trusts its caller to have run `private_path` first, which step 10c's plan carries (its Decisions).
  - A hand-made workbook with a huge used extent (a cell at the sheet's last row and column, in a file well under 10 MiB) could make `calamine` allocate a range of the whole extent and exhaust memory on a read. Recorded for phase 13, whose `imports/` brings in files from outside; the tools today read only what the folder holds.
  - Windows reserved names (`CON.xlsx`, `NUL.xlsx` and the rest) and a trailing space or dot in a folder name pass `is_a_name` and are not safe file names there. Recorded for the desktop phase.
