//! The Finance Specialist's spreadsheet tools (`docs/SPEC.md` 6.6): `farik_write_sheet` writes a
//! whole `.xlsx` workbook in the role's private folder, `.farik/local/finance/`, keeping every
//! previous version and refusing any formula that could reach outside the workbook.

use std::collections::BTreeSet;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use farik_core::contract::Role;
use farik_core::team::private_folder;
use rust_xlsxwriter::utility::{check_sheet_name, row_col_to_cell};
use rust_xlsxwriter::{ExcelDateTime, Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::session::SessionPurpose;

/// The most sheets a workbook holds.
const MOST_SHEETS: usize = 20;
/// The most headings a sheet has.
const MOST_COLUMNS: usize = 100;
/// The most rows a sheet has, the headings not counted.
const MOST_ROWS: usize = 10_000;
/// The most cells a row has.
const MOST_CELLS: usize = 100;
/// The most characters a string cell holds: Excel's own limit.
const MOST_TEXT: usize = 32_767;
/// The most bytes a workbook file is, written or read.
const MOST_BYTES: usize = 10 * 1024 * 1024;
/// The most characters of a path, and of one part of it.
const MOST_PATH: usize = 200;
const MOST_PART: usize = 100;
/// The most parts a path has: two folders and the file.
const MOST_PARTS: usize = 3;

/// `farik_write_sheet`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteSheetInput {
    /// The workbook's path inside your folder: 1 to 200 characters, at most 3 parts joined by `/`,
    /// each of letters, digits, spaces, `.`, `_` and `-` and starting with a letter or a digit,
    /// the last ending in `.xlsx` in lower case.
    path: String,
    /// The whole workbook, 1 to 20 sheets: it replaces the file, and the previous version is kept.
    sheets: Vec<SheetInput>,
}

/// One sheet of a workbook.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SheetInput {
    /// 1 to 31 characters, none of `[]:*?/\|`, not starting or ending with an apostrophe, not
    /// History, and different from the other sheets' names without regard to case.
    name: String,
    /// The headings, 0 to 100, written as the sheet's first row when there are any.
    #[serde(default)]
    columns: Vec<String>,
    /// The rows, 0 to 10,000 of 0 to 100 cells each, below the headings.
    #[serde(default)]
    rows: Vec<Vec<CellInput>>,
}

/// One cell: a number, a string (up to 32,767 characters, always written as text, whatever it
/// starts with), a boolean, `null` for an empty cell, `{ "date": "YYYY-MM-DD" }`, or
/// `{ "formula": "=..." }`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum CellInput {
    /// A number.
    Number(f64),
    /// Text, never a formula.
    Text(String),
    /// A boolean.
    Flag(bool),
    /// An empty cell.
    Empty(()),
    /// A date.
    Date(DateCell),
    /// A formula.
    Formula(FormulaCell),
}

/// A date cell.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DateCell {
    /// An ISO date, YYYY-MM-DD, from 1900 on.
    date: String,
}

/// A formula cell.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormulaCell {
    /// The formula's text, starting with `=`. It stays inside the workbook: no `[`, no `|`, and no
    /// function that reads or sends anything outside it, such as HYPERLINK or WEBSERVICE.
    formula: String,
}

/// What a workbook write did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WrittenWorkbook {
    /// Each sheet's name and the rows written to it, the headings not counted.
    pub(crate) sheets: Vec<(String, usize)>,
    /// Whether a workbook was there, and its previous version was kept.
    pub(crate) replaced: bool,
}

/// A refusal of a sheet tool under the code it names.
fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::Finance {
        code,
        detail: detail.into(),
    }
    .into()
}

fn sheet_refused(detail: impl Into<String>) -> ToolError {
    refused("sheet_refused", detail)
}

/// Whether `part` is a name a path may have: 1 to 100 characters of letters, digits, spaces, `.`,
/// `_` and `-`, starting with a letter or a digit, so that `.history` and `..` are not names.
fn is_a_name(part: &str) -> bool {
    let mut bytes = part.bytes();
    part.len() <= MOST_PART
        && bytes
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || b" ._-".contains(&byte))
}

/// The workbook at `path`, relative to the private folder `folder` of the project at `root`, with
/// every rule of the folder line checked: the path's shape, and that no part of it is a link, so
/// that it lies in the folder after links are resolved.
///
/// # Errors
///
/// `private_path_refused`, saying which rule.
pub(crate) fn private_path(root: &Path, folder: &str, path: &str) -> Result<PathBuf, ToolError> {
    let bad = |why: &str| refused("private_path_refused", format!("{path:?} {why}"));
    let parts: Vec<&str> = path.split('/').collect();
    if path.is_empty() || path.chars().count() > MOST_PATH || parts.len() > MOST_PARTS {
        return Err(bad(&format!(
            "is not a path of 1 to {MOST_PATH} characters and at most {MOST_PARTS} parts"
        )));
    }
    if !parts.iter().all(|part| is_a_name(part)) {
        return Err(bad(
            "has a part that is not a name: each part is letters, digits, spaces, `.`, `_` and \
             `-`, starting with a letter or a digit",
        ));
    }
    if parts
        .last()
        .and_then(|last| last.strip_suffix(".xlsx"))
        .is_none()
    {
        return Err(bad("is not a workbook: it ends in `.xlsx`, in lower case"));
    }
    let folder = root.join(folder);
    let mut at = folder.clone();
    for part in parts {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(bad("is, or passes through, a link"));
            }
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(failed(error)),
            _ => {}
        }
    }
    // The deepest part there is, resolved, is in the folder, resolved.
    if let Ok(resolved_folder) = fs::canonicalize(&folder) {
        let mut existing = at.as_path();
        while !existing.exists() {
            match existing.parent() {
                Some(parent) => existing = parent,
                None => break,
            }
        }
        let resolved = fs::canonicalize(existing).map_err(failed)?;
        if !resolved.starts_with(&resolved_folder) {
            return Err(bad("is outside the folder"));
        }
    }
    Ok(at)
}

/// The functions a formula may not call, because they read or send something outside the
/// workbook, or run something.
const OUTSIDE_FUNCTIONS: [&str; 19] = [
    "HYPERLINK",
    "WEBSERVICE",
    "FILTERXML",
    "IMPORTDATA",
    "IMPORTXML",
    "IMPORTHTML",
    "IMPORTRANGE",
    "IMPORTFEED",
    "IMAGE",
    "RTD",
    "DDE",
    "CALL",
    "REGISTER.ID",
    "EXEC",
    "INFO",
    "CELL",
    "COPILOT",
    "TRANSLATE",
    "DETECTLANGUAGE",
];

/// Whether `upper`, a formula in capitals, calls `name`: the name, preceded by the start of the
/// formula, by a character that cannot be part of a name, or by `_XLFN.` or `_XLWS.`, then
/// optional white space, then `(`. A name inside a string literal counts, which is safe.
fn calls(upper: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(found) = upper[from..].find(name) {
        let at = from + found;
        let before = &upper[..at];
        let starts_a_name = before.is_empty()
            || before.ends_with("_XLFN.")
            || before.ends_with("_XLWS.")
            || !before.ends_with(|last: char| last.is_ascii_alphanumeric() || "_.".contains(last));
        if starts_a_name && upper[at + name.len()..].trim_start().starts_with('(') {
            return true;
        }
        from = at + 1;
    }
    false
}

/// Why `formula` reaches outside its workbook or sends its text away: `[`, an external reference,
/// `|`, a DDE call such as `=cmd|' /C calc'!A0`, or the name of a function of
/// [`OUTSIDE_FUNCTIONS`] it calls; or `None` when it stays inside.
pub(crate) fn formula_reaches_outside(formula: &str) -> Option<&'static str> {
    if formula.contains('[') {
        return Some("[");
    }
    if formula.contains('|') {
        return Some("|");
    }
    let upper = formula.to_ascii_uppercase();
    OUTSIDE_FUNCTIONS
        .into_iter()
        .find(|name| calls(&upper, name))
}

/// What `reason` of `formula_reaches_outside` says to the agent.
fn how(reason: &str) -> String {
    match reason {
        "[" => "it holds `[`, an external reference".to_string(),
        "|" => "it holds `|`, a DDE call".to_string(),
        name => format!("it calls {name}"),
    }
}

/// A cell as the agent finds it in a spreadsheet program: `Books!B3 (row 3, column 2)`.
fn named(sheet: &str, row: usize, column: usize) -> String {
    let at = match (u32::try_from(row), u16::try_from(column)) {
        (Ok(row), Ok(column)) if row > 0 => row_col_to_cell(row - 1, column - 1),
        _ => String::new(),
    };
    format!("{sheet}!{at} (row {row}, column {column})")
}

/// Checks every limit of a workbook, and every formula and date in it, naming the first fault.
fn check_workbook(sheets: &[SheetInput]) -> Result<(), ToolError> {
    if sheets.is_empty() || sheets.len() > MOST_SHEETS {
        return Err(sheet_refused(format!(
            "a workbook has 1 to {MOST_SHEETS} sheets, and this one has {}",
            sheets.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for sheet in sheets {
        let name = &sheet.name;
        if let Err(error) = check_sheet_name(name) {
            return Err(sheet_refused(format!("the sheet name {name:?}: {error}")));
        }
        if name.contains('|') || name.eq_ignore_ascii_case("History") {
            return Err(sheet_refused(format!(
                "the sheet name {name:?} holds `|` or is History, which a spreadsheet program reserves"
            )));
        }
        if !seen.insert(name.to_lowercase()) {
            return Err(sheet_refused(format!(
                "two sheets are named {name:?}, and names are the same without regard to case"
            )));
        }
        check_sheet(sheet)?;
    }
    Ok(())
}

fn check_sheet(sheet: &SheetInput) -> Result<(), ToolError> {
    let name = &sheet.name;
    if sheet.columns.len() > MOST_COLUMNS || sheet.rows.len() > MOST_ROWS {
        return Err(sheet_refused(format!(
            "{name} has {} columns and {} rows, and a sheet has at most {MOST_COLUMNS} columns \
             and {MOST_ROWS} rows",
            sheet.columns.len(),
            sheet.rows.len()
        )));
    }
    if let Some(long) = sheet
        .columns
        .iter()
        .position(|heading| heading.chars().count() > MOST_TEXT)
    {
        return Err(sheet_refused(format!(
            "{name} has a heading of more than {MOST_TEXT} characters, the {}th",
            long + 1
        )));
    }
    // The headings are the first row, when there are any.
    let first_row = usize::from(!sheet.columns.is_empty()) + 1;
    for (index, cells) in sheet.rows.iter().enumerate() {
        let row = first_row + index;
        if cells.len() > MOST_CELLS {
            return Err(sheet_refused(format!(
                "{name} row {row} has {} cells, and a row has at most {MOST_CELLS}",
                cells.len()
            )));
        }
        for (position, cell) in cells.iter().enumerate() {
            check_cell(cell, &named(name, row, position + 1))?;
        }
    }
    Ok(())
}

fn check_cell(cell: &CellInput, at: &str) -> Result<(), ToolError> {
    match cell {
        CellInput::Text(text) if text.chars().count() > MOST_TEXT => Err(sheet_refused(format!(
            "{at} holds a string of more than {MOST_TEXT} characters"
        ))),
        CellInput::Date(DateCell { date }) => day(date)
            .map(|_| ())
            .map_err(|why| sheet_refused(format!("{at}: {why}"))),
        CellInput::Formula(FormulaCell { formula }) => {
            if !formula.starts_with('=') {
                return Err(refused(
                    "formula_refused",
                    format!("{at}: a formula starts with =, and this one does not"),
                ));
            }
            match formula_reaches_outside(formula) {
                Some(reason) => Err(refused(
                    "formula_refused",
                    format!(
                        "{at}: the formula reaches outside the workbook, because {}; a formula \
                         stays inside it, and what a service or a receipt gave goes in as a value",
                        how(reason)
                    ),
                )),
                None => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// An ISO date, `YYYY-MM-DD`, from 1900 on, which is as far back as a spreadsheet counts.
fn day(text: &str) -> Result<ExcelDateTime, String> {
    let shaped = text.len() == 10
        && NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
        && text.as_bytes().get(4) == Some(&b'-');
    if !shaped {
        return Err(format!("{text:?} is not a date, which is YYYY-MM-DD"));
    }
    ExcelDateTime::parse_from_str(text)
        .map_err(|error| format!("{text:?} is not a date a workbook holds: {error}"))
}

fn xlsx(error: XlsxError) -> ToolError {
    failed(error)
}

/// The workbook as `.xlsx` bytes. Every sheet's formulas are written with an empty stored value,
/// since Farik never computes one: a spreadsheet program does, when it opens the file.
fn build(sheets: &[SheetInput]) -> Result<Vec<u8>, ToolError> {
    let heading = Format::new().set_bold();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let mut workbook = Workbook::new();
    for sheet in sheets {
        let worksheet = workbook.add_worksheet();
        worksheet.set_name(&sheet.name).map_err(xlsx)?;
        worksheet.set_formula_result_default("");
        let mut row = 0_u32;
        if !sheet.columns.is_empty() {
            for (column, text) in (0_u16..).zip(&sheet.columns) {
                worksheet
                    .write_string_with_format(row, column, text, &heading)
                    .map_err(xlsx)?;
            }
            row += 1;
        }
        for cells in &sheet.rows {
            for (column, cell) in (0_u16..).zip(cells) {
                match cell {
                    CellInput::Number(number) => worksheet.write_number(row, column, *number),
                    CellInput::Text(text) => worksheet.write_string(row, column, text),
                    CellInput::Flag(flag) => worksheet.write_boolean(row, column, *flag),
                    CellInput::Empty(()) => Ok(&mut *worksheet),
                    CellInput::Date(DateCell { date: text }) => {
                        let day = day(text).map_err(sheet_refused)?;
                        worksheet.write_datetime_with_format(row, column, &day, &date)
                    }
                    CellInput::Formula(FormulaCell { formula }) => {
                        worksheet.write_formula(row, column, Formula::new(formula))
                    }
                }
                .map_err(xlsx)?;
            }
            row += 1;
        }
    }
    workbook.save_to_buffer().map_err(xlsx)
}

/// Makes `path` and the folders above it that are not there, each private to this user.
fn private_folder_at(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Creates `path`, which must not be there, readable by this user alone.
fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Copies the workbook at `path` to `.history/<its path in the folder, with / as __>.<UTC
/// yyyymmddThhmmssZ>.xlsx`, with `-<n>` before the extension when that name is taken.
fn keep_previous(folder: &Path, path: &Path, now: DateTime<Utc>) -> Result<(), ToolError> {
    let relative = path
        .strip_prefix(folder)
        .map_err(|_| failed(format!("{} is not in {}", path.display(), folder.display())))?;
    let flat = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("__");
    let stamp = now.format("%Y%m%dT%H%M%SZ");
    let history = folder.join(".history");
    private_folder_at(&history).map_err(failed)?;
    let mut previous = File::open(path).map_err(failed)?;
    for attempt in 0..1_000 {
        let name = if attempt == 0 {
            format!("{flat}.{stamp}.xlsx")
        } else {
            format!("{flat}.{stamp}-{attempt}.xlsx")
        };
        match create_private(&history.join(name)) {
            Ok(mut copy) => {
                io::copy(&mut previous, &mut copy).map_err(failed)?;
                return copy.sync_all().map_err(failed);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(error)),
        }
    }
    Err(failed(format!(
        "{} versions of {} were kept in one second",
        1_000,
        relative.display()
    )))
}

/// Makes every temporary file's name its own.
static TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Writes the workbook of `sheets` to `path`, a path in `folder` that [`private_path`] passed.
/// The folder is made private when it is first written. When a workbook is there already its
/// previous version is copied under `.history/` first. The new file is written beside the target
/// and renamed over it, so that a reader never sees half a file.
///
/// # Errors
///
/// `sheet_refused` for a workbook out of bounds, `formula_refused` for a formula that reaches
/// outside it, `sheet_too_large` for a file past 10 MiB, `private_path_refused` for a target that
/// is not a file; `Failed` when the folder or a file cannot be written. Nothing is written for a
/// refusal.
pub(crate) fn write_workbook(
    folder: &Path,
    path: &Path,
    sheets: &[SheetInput],
    now: DateTime<Utc>,
) -> Result<WrittenWorkbook, ToolError> {
    check_workbook(sheets)?;
    let bytes = build(sheets)?;
    if bytes.len() > MOST_BYTES {
        return Err(refused(
            "sheet_too_large",
            format!(
                "the workbook is {} bytes, and a workbook is at most {MOST_BYTES}; write fewer \
                 rows, or split it across workbooks",
                bytes.len()
            ),
        ));
    }
    let replaced = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => {
            return Err(refused(
                "private_path_refused",
                format!("{} is not a file", path.display()),
            ));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(failed(error)),
    };
    let parent = path.parent().unwrap_or(folder);
    private_folder_at(parent).map_err(failed)?;
    if replaced {
        keep_previous(folder, path, now)?;
    }
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        std::process::id(),
        TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let _ = fs::remove_file(&temporary);
        let mut file = create_private(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(failed(error));
    }
    Ok(WrittenWorkbook {
        sheets: sheets
            .iter()
            .map(|sheet| (sheet.name.clone(), sheet.rows.len()))
            .collect(),
        replaced,
    })
}

/// The folder a `farik_write_sheet` call writes in: the Finance Specialist's own, in its implement
/// session of a task it is the assignee of, so that a chat or a conversation never writes the
/// books and a task's baseline holds (spec 6.6).
fn folder_to_write(call: &Call<'_>) -> Result<&'static str, ToolError> {
    let refuse = |why: &str| sheet_refused(format!("only {why}"));
    let folder = (call.role() == Role::FinanceSpecialist)
        .then(|| private_folder(Role::FinanceSpecialist))
        .flatten()
        .ok_or_else(|| refuse("the Finance Specialist writes a workbook"))?;
    let task = match &call.context.task_id {
        Some(task) if call.context.purpose == SessionPurpose::Implement => task,
        _ => {
            return Err(refuse(
                "an implement session of a task writes a workbook: a chat or a conversation does not",
            ));
        }
    };
    if call.row(task)?.assignee_id.as_deref() != Some(call.agent_id()) {
        return Err(refuse("the task's assignee writes a workbook in it"));
    }
    Ok(folder)
}

/// `farik_write_sheet`: writes the whole workbook the input describes at its path in the caller's
/// folder, and answers its path, each sheet's name and rows, and whether it replaced a workbook.
/// It reports no path to the permission check, as `farik_write_memory` reports none, and holds the
/// folder line itself.
pub(super) fn write_sheet(call: &Call<'_>, input: &WriteSheetInput) -> Result<Value, ToolError> {
    let folder = folder_to_write(call)?;
    let root = call.deps().files.root();
    let target = private_path(root, folder, &input.path)?;
    let written = write_workbook(
        &root.join(folder),
        &target,
        &input.sheets,
        call.deps().clock.now(),
    )?;
    let sheets: Vec<Value> = written
        .sheets
        .iter()
        .map(|(name, rows)| json!({ "name": name, "rows": rows }))
        .collect();
    Ok(json!({ "path": input.path, "sheets": sheets, "replaced": written.replaced }))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::BufReader;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::{Path, PathBuf};

    use calamine::{Data, DataType as _, Reader, Xlsx, open_workbook};
    use serde_json::{Value, json};

    use super::formula_reaches_outside;
    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, run, with_the_finance_specialist,
        with_the_marketing_specialist,
    };

    /// A project with the Finance Specialist `fin`, whose task FRK-1 is in progress, and a
    /// Developer's task FRK-2; and the Marketing Specialist `kai`.
    fn a_finance_project(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_marketing_specialist(wire);
            }),
        );
        project.filed_with("FRK-1", "assigned", "task", None, |wire| {
            wire["assignee_role"] = json!("finance_specialist");
            wire["reviewer_role"] = json!("product_manager");
        });
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "fin", "reviewer": "pm" }),
        );
        project.filed("FRK-2", "assigned", "task", None);
        project.moved(
            "FRK-2",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        project
    }

    /// The Finance Specialist's folder.
    fn folder(project: &TestProject) -> PathBuf {
        project.repo.path.join(".farik/local/finance")
    }

    /// `farik_write_sheet` as `fin` in its implement session of FRK-1.
    fn write(project: &TestProject, input: &Value) -> Result<Value, ToolError> {
        project.call("fin", Some("FRK-1"), "farik_write_sheet", input.clone())
    }

    /// One sheet's JSON.
    fn sheet(name: &str, columns: &[&str], rows: &Value) -> Value {
        json!({ "name": name, "columns": columns, "rows": rows })
    }

    /// A workbook of one sheet at `path`.
    fn one_sheet(path: &str, rows: &Value) -> Value {
        json!({ "path": path, "sheets": [sheet("Books", &["Item", "USD"], rows)] })
    }

    fn open(path: &Path) -> Xlsx<BufReader<fs::File>> {
        open_workbook(path).expect("the workbook opens")
    }

    /// The cell at the zero-based `(row, column)` of `sheet`, as the file holds it.
    fn cell(book: &mut Xlsx<BufReader<fs::File>>, sheet: &str, at: (u32, u32)) -> Data {
        book.worksheet_range(sheet)
            .expect("the sheet reads")
            .get_value(at)
            .cloned()
            .unwrap_or(Data::Empty)
    }

    /// Every file under `folder`, as paths relative to it, sorted.
    fn files_under(folder: &Path) -> Vec<String> {
        fn walk(folder: &Path, at: &Path, found: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(at) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(folder, &path, found);
                } else if let Ok(relative) = path.strip_prefix(folder) {
                    found.push(relative.display().to_string());
                }
            }
        }
        let mut found = Vec::new();
        walk(folder, folder, &mut found);
        found.sort();
        found
    }

    fn refusal_of(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_a_workbook_that_reads_back() {
        let project = a_finance_project("sheets-write");
        let input = json!({
            "path": "books.xlsx",
            "sheets": [
                sheet("Expenses", &["Day", "USD", "Item"], &json!([
                    [{ "date": "2026-10-01" }, 12.5, "Claude"],
                    [{ "date": "2026-10-02" }, 7.25, "Hosting"],
                    ["Total", { "formula": "=SUM(B2:B3)" }, null]
                ])),
                sheet("Notes", &["Note"], &json!([["kept"], [true]])),
            ]
        });

        let answer = write(&project, &input).expect("the workbook is written");

        assert_eq!(
            answer,
            json!({
                "path": "books.xlsx",
                "sheets": [{ "name": "Expenses", "rows": 3 }, { "name": "Notes", "rows": 2 }],
                "replaced": false
            })
        );
        let mut book = open(&folder(&project).join("books.xlsx"));
        assert_eq!(book.sheet_names(), ["Expenses", "Notes"]);
        assert_eq!(
            cell(&mut book, "Expenses", (0, 0)),
            Data::String("Day".to_string())
        );
        let Data::DateTime(day) = cell(&mut book, "Expenses", (1, 0)) else {
            panic!("a date reads back as a date");
        };
        assert_eq!(day.to_ymd_hms_milli(), (2026, 10, 1, 0, 0, 0, 0));
        assert_eq!(cell(&mut book, "Expenses", (1, 1)).as_f64(), Some(12.5));
        assert_eq!(
            cell(&mut book, "Expenses", (2, 2)),
            Data::String("Hosting".to_string())
        );
        assert_eq!(
            cell(&mut book, "Notes", (2, 0)),
            Data::Bool(true),
            "a boolean reads back as one"
        );
        // The formula is kept as text, and has no stored value until a spreadsheet program
        // computes one.
        let formulas = book.worksheet_formula("Expenses").expect("formulas read");
        assert_eq!(
            formulas.get_value((3, 1)).map(|text| format!("={text}")),
            Some("=SUM(B2:B3)".to_string())
        );
        assert!(
            matches!(cell(&mut book, "Expenses", (3, 1)), Data::Empty)
                || cell(&mut book, "Expenses", (3, 1)) == Data::String(String::new()),
            "no stored value: {:?}",
            cell(&mut book, "Expenses", (3, 1))
        );
        let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&folder(&project)), 0o700, "the folder is private");
        assert_eq!(mode(&folder(&project).join("books.xlsx")), 0o600);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_string_is_never_a_formula() {
        let project = a_finance_project("sheets-strings");
        let strings = ["=1+1", "+cmd", "@SUM(A1)", "-2+3"];
        let rows: Vec<Value> = strings.iter().map(|text| json!([text, 1])).collect();

        write(&project, &one_sheet("books.xlsx", &json!(rows))).expect("the workbook is written");

        let mut book = open(&folder(&project).join("books.xlsx"));
        for (row, text) in strings.iter().enumerate() {
            let row = u32::try_from(row).expect("a few rows") + 1;
            assert_eq!(
                cell(&mut book, "Books", (row, 0)),
                Data::String((*text).to_string())
            );
        }
        let formulas = book.worksheet_formula("Books").expect("formulas read");
        assert_eq!(formulas.used_cells().count(), 0, "no cell holds a formula");
    }

    #[test]
    fn names_what_a_formula_may_not_reach() {
        for formula in [
            "=[book.xlsx]S!A1",
            "=cmd|' /C calc'!A0",
            "=HYPERLINK(\"x\")",
            "=webservice(\"x\")",
            "=IMPORTXML(\"x\",\"y\")",
            "=IMAGE(\"https://x\")",
            "=DDE(\"a\",\"b\",\"c\")",
            "=_xlfn.WEBSERVICE(\"x\")",
            "=INFO (\"os\")",
            "=CELL(\"filename\")",
            "=COPILOT(\"x\")",
            "=1+HYPERLINK(\"x\")",
            "=Sheet1!INFO(\"os\")",
            "=_xlws.FILTERXML(\"a\",\"b\")",
        ] {
            assert!(
                formula_reaches_outside(formula).is_some(),
                "{formula} reaches outside"
            );
        }
        for formula in [
            "=SUM(A1:A3)",
            "=A1*B1",
            "=MYCELLS(1)",
            "=MYINFO(1)",
            "=SUM(A1:A3)+B2",
        ] {
            assert_eq!(
                formula_reaches_outside(formula),
                None,
                "{formula} stays inside"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_formula_that_reaches_outside() {
        let project = a_finance_project("sheets-formulas");
        for formula in [
            "=[book.xlsx]S!A1",
            "=cmd|' /C calc'!A0",
            "=HYPERLINK(\"x\")",
            "=webservice(\"x\")",
            "=IMPORTXML(\"x\",\"y\")",
            "=IMAGE(\"https://x\")",
            "=DDE(\"a\",\"b\",\"c\")",
            "=_xlfn.WEBSERVICE(\"x\")",
            "=INFO (\"os\")",
            "=CELL(\"filename\")",
            "=COPILOT(\"x\")",
        ] {
            let reason = refusal_of(write(
                &project,
                &one_sheet(
                    "books.xlsx",
                    &json!([["a", 1], ["b", 2], ["c", { "formula": formula }]]),
                ),
            ));
            assert!(
                reason.starts_with("formula_refused: "),
                "{formula}: {reason}"
            );
            assert!(
                reason.contains("Books") && reason.contains("row 4") && reason.contains("column 2"),
                "{formula} names its cell: {reason}"
            );
            assert_eq!(
                files_under(&folder(&project)),
                Vec::<String>::new(),
                "{formula}: nothing is written"
            );
        }
        for formula in ["=SUM(A1:A3)", "=A1*B1", "=MYCELLS(1)"] {
            write(
                &project,
                &one_sheet(
                    "books.xlsx",
                    &json!([["a", 1], ["b", { "formula": formula }]]),
                ),
            )
            .unwrap_or_else(|error| panic!("{formula} is a formula that stays inside: {error}"));
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_the_previous_version() {
        let project = a_finance_project("sheets-history");
        let first = write(&project, &one_sheet("books.xlsx", &json!([["one", 1]])))
            .expect("the first write");
        assert_eq!(first["replaced"], false);
        let second = write(&project, &one_sheet("books.xlsx", &json!([["two", 2]])))
            .expect("the second write");
        assert_eq!(second["replaced"], true);

        // The fixed clock is 2026-09-22 12:00:00 UTC.
        let history = folder(&project).join(".history");
        let kept = history.join("books.xlsx.20260922T120000Z.xlsx");
        assert_eq!(
            cell(&mut open(&kept), "Books", (1, 0)),
            Data::String("one".to_string()),
            "the first version is kept"
        );

        write(&project, &one_sheet("books.xlsx", &json!([["three", 3]])))
            .expect("the third write, in the same second");

        assert_eq!(
            cell(
                &mut open(&history.join("books.xlsx.20260922T120000Z-1.xlsx")),
                "Books",
                (1, 0)
            ),
            Data::String("two".to_string()),
            "the second version is kept beside the first"
        );
        assert_eq!(
            cell(&mut open(&kept), "Books", (1, 0)),
            Data::String("one".to_string()),
            "and the first is not replaced"
        );
        assert_eq!(
            cell(
                &mut open(&folder(&project).join("books.xlsx")),
                "Books",
                (1, 0)
            ),
            Data::String("three".to_string()),
            "the target holds the last"
        );
        // A path in a subfolder keeps its folder in the name, as `__`.
        for text in ["a", "b"] {
            write(
                &project,
                &one_sheet("2026/pricing.xlsx", &json!([[text, 1]])),
            )
            .expect("a workbook in a subfolder");
        }
        assert!(
            history
                .join("2026__pricing.xlsx.20260922T120000Z.xlsx")
                .is_file(),
            "{:?}",
            files_under(&folder(&project))
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn holds_the_folder_line() {
        let project = a_finance_project("sheets-folder-line");
        let outside = project.repo.path.join("outside");
        fs::create_dir_all(&outside).expect("a folder outside");
        fs::create_dir_all(folder(&project)).expect("the folder");
        symlink(&outside, folder(&project).join("out")).expect("a link out of the folder");
        fs::write(outside.join("kept.xlsx"), b"not a workbook").expect("a file outside");
        symlink(
            outside.join("kept.xlsx"),
            folder(&project).join("kept.xlsx"),
        )
        .expect("a link to a file outside");
        let long_name = format!("{}.xlsx", "a".repeat(200));
        for path in [
            "../books.xlsx",
            "/tmp/x.xlsx",
            ".history/x.xlsx",
            "a/b/c/d.xlsx",
            "x.xlsm",
            "x.XLSX",
            "x",
            "",
            "a//b.xlsx",
            " x.xlsx",
            "a/./b.xlsx",
            "caf\u{e9}.xlsx",
            long_name.as_str(),
            "out/x.xlsx",
            "kept.xlsx",
            "out/../x.xlsx",
        ] {
            let reason = refusal_of(write(&project, &one_sheet(path, &json!([["a", 1]]))));
            assert!(
                reason.starts_with("private_path_refused: "),
                "{path:?}: {reason}"
            );
        }
        assert_eq!(
            files_under(&outside),
            ["kept.xlsx"],
            "nothing is written outside"
        );
        assert_eq!(
            fs::read(outside.join("kept.xlsx")).expect("the file outside"),
            b"not a workbook",
            "and what is outside is as it was"
        );
        assert!(!project.repo.path.join(".farik/local/books.xlsx").exists());
        assert!(!folder(&project).join(".history").exists());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_only_in_its_own_task() {
        let project = a_finance_project("sheets-own-task");
        let input = one_sheet("books.xlsx", &json!([["a", 1]]));
        let mut chat = project.context("fin", None);
        chat.purpose = SessionPurpose::Chat;
        let mut chat_of_the_task = project.context("fin", Some("FRK-1"));
        chat_of_the_task.purpose = SessionPurpose::Chat;
        let mut verifying = project.context("fin", Some("FRK-1"));
        verifying.purpose = SessionPurpose::Verify;
        for (who, context) in [
            ("a chat", chat),
            ("a chat about the task", chat_of_the_task),
            ("a verify session", verifying),
            (
                "an implement session about no task",
                project.context("fin", None),
            ),
            (
                "another agent's task",
                project.context("fin", Some("FRK-2")),
            ),
            (
                "the Marketing Specialist",
                project.context("kai", Some("FRK-1")),
            ),
            ("the Product Manager", project.context("pm", Some("FRK-1"))),
        ] {
            let reason = refusal_of(run(&context, "farik_write_sheet", input.clone()));
            assert!(reason.starts_with("sheet_refused: "), "{who}: {reason}");
        }
        assert_eq!(files_under(&folder(&project)), Vec::<String>::new());
        write(&project, &input).expect("its own task's implement session writes");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_workbook_out_of_bounds() {
        let project = a_finance_project("sheets-bounds");
        let named =
            |name: &str| json!({ "path": "books.xlsx", "sheets": [sheet(name, &[], &json!([]))] });
        let too_many: Vec<Value> = (1..=21)
            .map(|number| sheet(&format!("Sheet{number}"), &[], &json!([])))
            .collect();
        let columns: Vec<String> = (0..101).map(|number| format!("c{number}")).collect();
        let big_text = "x".repeat(32_768);
        let cases = [
            (
                "21 sheets",
                json!({ "path": "books.xlsx", "sheets": too_many }),
            ),
            ("no sheet", json!({ "path": "books.xlsx", "sheets": [] })),
            ("a slash in a name", named("a/b")),
            ("a bar in a name", named("a|b")),
            ("History", named("History")),
            ("history", named("history")),
            ("an empty name", named("")),
            ("a name of 32 characters", named(&"n".repeat(32))),
            ("an apostrophe at the start", named("'Books")),
            (
                "two names that differ in case",
                json!({ "path": "books.xlsx", "sheets": [
                    sheet("Books", &[], &json!([])), sheet("books", &[], &json!([]))
                ] }),
            ),
            (
                "10,001 rows",
                json!({ "path": "books.xlsx", "sheets": [
                    sheet("Books", &[], &Value::Array(vec![json!([]); 10_001]))
                ] }),
            ),
            (
                "101 columns",
                json!({ "path": "books.xlsx", "sheets": [{ "name": "Books", "columns": columns }] }),
            ),
            (
                "101 cells in a row",
                json!({ "path": "books.xlsx", "sheets": [
                    sheet("Books", &[], &json!([vec![json!(1); 101]]))
                ] }),
            ),
            (
                "a string of 32,768 characters",
                json!({ "path": "books.xlsx", "sheets": [
                    sheet("Books", &[], &json!([[big_text]]))
                ] }),
            ),
            (
                "a date that is not one",
                one_sheet("books.xlsx", &json!([[{ "date": "2026-02-30" }, 1]])),
            ),
            (
                "a date with a time",
                one_sheet(
                    "books.xlsx",
                    &json!([[{ "date": "2026-02-03T10:00:00" }, 1]]),
                ),
            ),
        ];
        for (what, input) in cases {
            let reason = refusal_of(write(&project, &input));
            assert!(reason.starts_with("sheet_refused: "), "{what}: {reason}");
        }
        let reason = refusal_of(write(
            &project,
            &one_sheet("books.xlsx", &json!([["a", { "formula": "SUM(A1)" }]])),
        ));
        assert!(
            reason.starts_with("formula_refused: ") && reason.contains('='),
            "a formula starts with =: {reason}"
        );
        assert_eq!(
            files_under(&folder(&project)),
            Vec::<String>::new(),
            "each is refused before writing"
        );
        // The bounds themselves are within the limits.
        let edge = json!({ "path": "books.xlsx", "sheets": [
            sheet(&"n".repeat(31), &[], &json!([vec![json!(1); 100]])),
            sheet("Rows", &[], &Value::Array(vec![json!([]); 10_000])),
        ] });
        write(&project, &edge).expect("the largest bounds are written");
    }

    /// `len` characters of noise from `seed`, which a zip cannot make much smaller.
    fn noise(len: usize, seed: &mut u64) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        (0..len)
            .map(|_| {
                *seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let at = usize::try_from((*seed >> 33) % 64).expect("under 64");
                char::from(ALPHABET[at])
            })
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_workbook_over_ten_mebibytes() {
        let project = a_finance_project("sheets-too-large");
        write(&project, &one_sheet("books.xlsx", &json!([["kept", 1]]))).expect("a first write");
        let before = fs::read(folder(&project).join("books.xlsx")).expect("the target");
        let mut seed = 7;
        // 600 cells of 30,000 characters of noise: some 13 MiB once zipped.
        let rows: Vec<Value> = (0..6)
            .map(|_| Value::Array((0..100).map(|_| json!(noise(30_000, &mut seed))).collect()))
            .collect();

        let reason = refusal_of(write(
            &project,
            &one_sheet("books.xlsx", &Value::Array(rows)),
        ));

        assert!(reason.starts_with("sheet_too_large: "), "{reason}");
        assert_eq!(
            fs::read(folder(&project).join("books.xlsx")).expect("the target"),
            before,
            "the old target is unchanged"
        );
        assert_eq!(
            files_under(&folder(&project)),
            ["books.xlsx"],
            "no copy in .history and no half file beside it"
        );
    }
}
