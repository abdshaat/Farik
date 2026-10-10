//! `catervas order list`: the purchase orders the Procurement Specialist set up and where each
//! stands (`docs/SPEC.md` 6.10, ADR 0039). The owner's steps on them are commands, sent as
//! `catervas site approve` sends its own.

use catervas_runtime::procurement::purchase_orders_list;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::Report;
use crate::project::Project;

/// The number an order is called by on the command line: `12` or `PO-12`.
///
/// # Errors
///
/// A sentence saying what a number looks like when `text` is neither.
pub fn number(text: &str) -> Result<u64, String> {
    let digits = match text.get(..3) {
        Some(prefix) if prefix.eq_ignore_ascii_case("PO-") => &text[3..],
        _ => text,
    };
    Some(digits)
        .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|digits| digits.parse::<u64>().ok())
        .filter(|order| *order > 0)
        .ok_or_else(|| format!("{text} is not an order: write its number, as 12 or PO-12"))
}

/// Every order, oldest first, with where it stands, and the same as `purchase_orders.list`
/// answers with `--json`.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn list(project: &Project, now: DateTime<Utc>) -> Result<Report, String> {
    let wire =
        purchase_orders_list(&project.log, now.date_naive()).map_err(|error| error.to_string())?;
    Ok(Report {
        lines: lines(&wire),
        json: wire,
        json_lines: None,
    })
}

/// The lines a person reads for `purchase_orders.list`'s answer.
fn lines(wire: &Value) -> Vec<String> {
    let orders = wire["orders"].as_array().cloned().unwrap_or_default();
    if orders.is_empty() {
        return vec!["no purchase order yet".to_string()];
    }
    orders.iter().map(line).collect()
}

/// One order on one line: its number, state, seller, total, task and agent, then what is known of
/// its placing and its latest status.
fn line(order: &Value) -> String {
    let mut parts = vec![
        format!("PO-{}", order["order"].as_u64().unwrap_or_default()),
        text(order, "state"),
        text(order, "seller"),
        format!(
            "{} {} {}",
            text(order, "total"),
            text(order, "currency"),
            text(order, "period")
        ),
        text(order, "task_id"),
        text(order, "agent_id"),
    ];
    if order.get("placed_on").is_some() {
        parts.push(format!("placed {}", text(order, "placed_on")));
    }
    if order["overdue"].as_bool().unwrap_or(false) {
        parts.push("overdue".to_string());
    }
    if let Some(status) = order.get("status") {
        parts.push(status_words(status));
    }
    parts.join("  ")
}

/// A status as a person reads it: what it says, who recorded it and when, then the note and the
/// day the seller expects the order.
fn status_words(status: &Value) -> String {
    let who = if text(status, "by") == "owner" {
        "you"
    } else {
        "the agent"
    };
    let day: String = text(status, "at").chars().take(10).collect();
    let mut words = format!("{} ({who}, {day})", text(status, "status"));
    let note = text(status, "note");
    if !note.is_empty() {
        words = format!("{words}: {note}");
    }
    if status.get("expected_on").is_some() {
        words = format!("{words}, expected {}", text(status, "expected_on"));
    }
    words
}

/// The string at `key` of `value`, or nothing.
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{line, lines, number};

    #[test]
    fn an_order_is_called_by_its_number_or_po_and_its_number() {
        assert_eq!(number("12"), Ok(12));
        assert_eq!(number("PO-12"), Ok(12));
        assert_eq!(number("po-3"), Ok(3));
        for wrong in [
            "", "0", "PO-0", "PO-", "PO-x", "x", "-1", "PO--1", "1.5", " 1", "+1", "ÄÖ-1",
        ] {
            assert!(number(wrong).is_err(), "{wrong:?}");
        }
        assert!(
            number("PO-x").is_err_and(|error| error.contains("PO-x") && error.contains("PO-12"))
        );
    }

    #[test]
    fn the_lines_are_the_orders_oldest_first_or_say_there_is_none() {
        assert_eq!(lines(&json!({ "orders": [] })), ["no purchase order yet"]);
        let order = |number: u64, state: &str| {
            json!({
                "order": number, "state": state, "seller": "Acme", "total": "1.00",
                "currency": "USD", "period": "once", "task_id": "FRK-1", "agent_id": "theo",
                "overdue": false
            })
        };
        let wire = json!({ "orders": [order(4, "approved"), order(5, "rejected")] });
        let found = lines(&wire);
        assert_eq!(found.len(), 2);
        assert!(found[0].starts_with("PO-4  approved  "), "{found:?}");
        assert!(found[1].starts_with("PO-5  rejected  "), "{found:?}");
    }

    #[test]
    fn a_line_says_where_an_order_stands() {
        let drafted = json!({
            "order": 1, "state": "drafted", "seller": "Acme", "total": "59.98",
            "currency": "USD", "period": "once", "task_id": "FRK-1", "agent_id": "theo",
            "overdue": false
        });
        assert_eq!(
            line(&drafted),
            "PO-1  drafted  Acme  59.98 USD once  FRK-1  theo"
        );
        let mut placed = drafted;
        placed["state"] = json!("placed");
        placed["placed_on"] = json!("2026-10-08");
        placed["overdue"] = json!(true);
        placed["status"] = json!({
            "status": "delayed", "note": "Short of flour", "by": "owner",
            "at": "2026-10-20T09:00:00Z", "expected_on": "2026-11-02"
        });
        assert_eq!(
            line(&placed),
            "PO-1  placed  Acme  59.98 USD once  FRK-1  theo  placed 2026-10-08  overdue  \
             delayed (you, 2026-10-20): Short of flour, expected 2026-11-02"
        );
        placed["status"]["by"] = json!("agent");
        placed["status"]["note"] = json!("");
        placed["status"]
            .as_object_mut()
            .expect("an object")
            .remove("expected_on");
        assert!(
            line(&placed).ends_with("overdue  delayed (the agent, 2026-10-20)"),
            "{}",
            line(&placed)
        );
    }
}
