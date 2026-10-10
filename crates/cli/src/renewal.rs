//! `catervas renewal list`: the renewals coming up that nobody dismissed (`docs/SPEC.md` 6.10, ADR
//! 0039). Dismissing one is a command, sent as `catervas site approve` sends its own.

use catervas_runtime::procurement::renewals_list;
use serde_json::Value;

use crate::Report;
use crate::project::Project;

/// The renewals coming up, oldest first, and how many rows of the register Catervas could not read,
/// and the same as `renewals.list` answers with `--json`.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn list(project: &Project) -> Result<Report, String> {
    let wire = renewals_list(&project.log).map_err(|error| error.to_string())?;
    Ok(Report {
        lines: lines(&wire),
        json: wire,
        json_lines: None,
    })
}

/// The lines a person reads for `renewals.list`'s answer.
fn lines(wire: &Value) -> Vec<String> {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_string();
    let mut lines: Vec<String> = wire["open"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            format!(
                "{}  {}  renews {}, decide by {}",
                row["renewal"].as_u64().unwrap_or_default(),
                text(row, "vendor"),
                text(row, "renews_on"),
                text(row, "decide_by"),
            )
        })
        .collect();
    if lines.is_empty() {
        lines.push("no renewal is coming up".to_string());
    }
    match wire["unreadable"].as_u64().unwrap_or_default() {
        0 => {}
        1 => lines.push("1 row in the register has a renewal date Catervas can't read".to_string()),
        many => lines.push(format!(
            "{many} rows in the register have a renewal date Catervas can't read"
        )),
    }
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::lines;

    #[test]
    fn says_each_open_renewal_and_how_many_rows_cannot_be_read() {
        assert_eq!(
            lines(&json!({ "open": [], "unreadable": 0 })),
            ["no renewal is coming up"]
        );
        let open = json!([{
            "renewal": 7, "vendor": "Vercel", "renews_on": "2026-11-30",
            "decide_by": "2026-10-31", "flagged_at": "2026-10-17T09:00:00Z"
        }]);
        assert_eq!(
            lines(&json!({ "open": open, "unreadable": 0 })),
            ["7  Vercel  renews 2026-11-30, decide by 2026-10-31"]
        );
        // Rows that cannot be read are said even when nothing else is coming up.
        assert_eq!(
            lines(&json!({ "open": [], "unreadable": 1 })),
            [
                "no renewal is coming up",
                "1 row in the register has a renewal date Catervas can't read"
            ]
        );
        assert_eq!(
            lines(&json!({ "open": open, "unreadable": 2 }))[1],
            "2 rows in the register have a renewal date Catervas can't read"
        );
    }
}
