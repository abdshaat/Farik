//! What leaves Catervas for the forge (a pull request's title and body, a commit's message) names
//! no task id (ADR 0066). The id stays inside Catervas; a pull request says what the task is.

use crate::branch::ChangeType;
use crate::contract::TaskId;
use crate::folders::is_delivery_report;

/// Where each `CTV-<digits>` match in `text` is: its start, its end, and whether it starts a word
/// (the character before it is not ASCII alphanumeric, or there is none).
fn matches(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find("CTV-") {
        let start = at + offset;
        let digits = bytes[start + 4..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let starts_a_word = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if digits > 0 && starts_a_word {
            found.push((start, start + 4 + digits));
        }
        at = start + 4;
    }
    found
}

/// `<word>: <title>`, the title passed through `without_task_ids`.
#[must_use]
pub fn pull_request_title(
    kind: ChangeType,
    title: &str,
    title_of: &dyn Fn(&TaskId) -> Option<String>,
) -> String {
    format!("{}: {}", kind.word(), without_task_ids(title, title_of))
}

/// A folder change's pull request title: `Docs: {names}` when every path is a delivery's report
/// (the name already says what it is), else `Docs: Update {names}` (ADR 0069).
#[must_use]
pub fn folder_change_title(paths: &[&str], names: &str) -> String {
    if !paths.is_empty() && paths.iter().all(|path| is_delivery_report(path)) {
        format!("Docs: {names}")
    } else {
        format!("Docs: Update {names}")
    }
}

/// `text` with each task id replaced by `"<its title>"`, or by `another task` when `title_of`
/// answers `None`. An id is `CTV-` and digits at a word's start; one after a `/` is part of a path
/// and stays, and so does a number no task id can hold.
#[must_use]
pub fn without_task_ids(text: &str, title_of: &dyn Fn(&TaskId) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, end) in matches(text) {
        let in_a_path = start > 0 && text.as_bytes()[start - 1] == b'/';
        let Ok(id) = text[start..end].parse::<TaskId>() else {
            continue;
        };
        if in_a_path {
            continue;
        }
        out.push_str(&text[copied..start]);
        match title_of(&id) {
            Some(title) => {
                out.push('"');
                out.push_str(&title);
                out.push('"');
            }
            None => out.push_str("another task"),
        }
        copied = end;
    }
    out.push_str(&text[copied..]);
    out
}

/// The first task id `text` names (`CTV-12`), after a `/` too: a path named in a commit message is
/// still a task id in a commit.
#[must_use]
pub fn task_id_in(text: &str) -> Option<String> {
    matches(text)
        .first()
        .map(|&(start, end)| text[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{folder_change_title, pull_request_title, task_id_in, without_task_ids};
    use crate::branch::ChangeType;
    use crate::contract::TaskId;
    use crate::folders::document_name;

    fn known(id: &TaskId) -> Option<String> {
        (id.as_str() == "CTV-2").then(|| "Pie pre-orders".to_string())
    }

    #[test]
    fn forge_text_names_no_task() {
        assert_eq!(
            pull_request_title(
                ChangeType::Feat,
                "Character counter on the note input",
                &known
            ),
            "Feat: Character counter on the note input"
        );
        assert_eq!(
            pull_request_title(ChangeType::Fix, "Undo CTV-2's rounding", &known),
            "Fix: Undo \"Pie pre-orders\"'s rounding"
        );
        assert_eq!(
            without_task_ids(
                "Follows CTV-2; see docs/catervas/architecture/plans/CTV-2.md. CTV-9 is next. (CTV-2)",
                &known
            ),
            "Follows \"Pie pre-orders\"; see docs/catervas/architecture/plans/CTV-2.md. another task is next. (\"Pie pre-orders\")"
        );
        for unchanged in ["ACTV-2 and CTV-1234567", "nothing to see"] {
            assert_eq!(without_task_ids(unchanged, &known), unchanged);
        }
        for (text, id) in [
            ("feat: add the counter (CTV-1)", Some("CTV-1")),
            ("docs: write plans/CTV-12.md", Some("CTV-12")),
            ("feat: CTV-x and ACTV-3", None),
            ("fix: round the total", None),
        ] {
            assert_eq!(task_id_in(text).as_deref(), id, "{text}");
        }
    }

    #[test]
    fn titles_a_delivery_report_by_its_name() {
        let name = |path: &str| document_name(path, &|_: &str| None);
        assert_eq!(
            name("docs/catervas/product/reports/D1.html"),
            "Report for delivery 1"
        );
        assert_eq!(
            name("docs/catervas/product/reports/D12.md"),
            "Report for delivery 12"
        );
        assert_eq!(name("docs/catervas/product/reports/Dx.md"), "Dx");
        let report = "docs/catervas/product/reports/D1.html";
        assert_eq!(
            folder_change_title(&[report], "Report for delivery 1"),
            "Docs: Report for delivery 1"
        );
        assert_eq!(
            folder_change_title(&[report, "docs/catervas/product/roadmap.html"], "Roadmap"),
            "Docs: Update Roadmap"
        );
    }
}
