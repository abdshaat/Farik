//! The Procurement Specialist's purchase orders and renewals (`docs/SPEC.md` 6.10, ADR 0039): the
//! locks under which an order is numbered, decided, placed, received, closed and expired, the rules
//! of an order's follow-up status, and how an order is worded on the wire.

use std::sync::Mutex;

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use farik_protocol::event::PurchaseOrderStatus;
use farik_store::purchase_orders::{PurchaseOrderRecord, expires_at, overdue};
use serde_json::{Value, json};

/// Held from the first read of the orders to the record that changes them: by the agent's draft,
/// by each of the owner's commands, by a status and by the expiry, so that two of them never take
/// one number, decide one order twice or expire an order being placed. One lock for every project
/// in the process.
pub(crate) static ORDERS: Mutex<()> = Mutex::new(());

/// The most characters a follow-up status's note has.
const MOST_STATUS_NOTE: usize = 300;

/// A follow-up status that passed its rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FollowUpFields {
    /// What the follow-up learned.
    pub(crate) status: PurchaseOrderStatus,
    /// The reason or what is known, possibly empty.
    pub(crate) note: String,
    /// The day the seller expects the order.
    pub(crate) expected_on: Option<NaiveDate>,
}

/// Holds a follow-up status to its rules, as the agent's tool and the owner's correction both
/// have it: one of `preparing`, `shipped`, `delayed` and `problem`, never `placed`, `received` or
/// `paid` (only the owner takes those steps); a note of 300 characters on one line, which
/// `delayed` and `problem` need; and a day the seller expects the order, from `today` on, which
/// `delayed` needs.
///
/// # Errors
///
/// A sentence saying which rule the status breaks.
pub(crate) fn check_follow_up(
    status: &str,
    note: &str,
    expected_on: Option<&str>,
    today: NaiveDate,
) -> Result<FollowUpFields, String> {
    let status = match status {
        "preparing" => PurchaseOrderStatus::Preparing,
        "shipped" => PurchaseOrderStatus::Shipped,
        "delayed" => PurchaseOrderStatus::Delayed,
        "problem" => PurchaseOrderStatus::Problem,
        other => {
            return Err(format!(
                "{} is not a status: it is one of preparing, shipped, delayed and problem, and \
                 only the owner marks an order placed or received",
                crate::tools::sites::shown(other)
            ));
        }
    };
    if note.chars().count() > MOST_STATUS_NOTE || note.chars().any(char::is_control) {
        return Err(format!(
            "the note is at most {MOST_STATUS_NOTE} characters on one line"
        ));
    }
    let needs_a_note = matches!(
        status,
        PurchaseOrderStatus::Delayed | PurchaseOrderStatus::Problem
    );
    if needs_a_note && note.trim().is_empty() {
        return Err("a delayed order or a problem needs what is known in the note".to_string());
    }
    let expected_on = match expected_on {
        None => None,
        Some(text) => {
            let day = (text.len() == 10)
                .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
                .flatten()
                .ok_or_else(|| {
                    format!(
                        "{} is not a day: write expected_on like 2026-10-30",
                        crate::tools::sites::shown(text)
                    )
                })?;
            if day < today {
                return Err(format!(
                    "expected_on is {day}, before today, {today}: write the day the seller \
                     expects the order"
                ));
            }
            Some(day)
        }
    };
    if status == PurchaseOrderStatus::Delayed && expected_on.is_none() {
        return Err("a delayed order needs expected_on, the day the seller expects it".to_string());
    }
    Ok(FollowUpFields {
        status,
        note: note.to_string(),
        expected_on,
    })
}

/// A time as the wire words it.
fn time(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// One order as `purchase_orders.list` and the agent's `farik_read_purchase_orders` word it: its
/// number, state, seller, total, task, agent and every day the log gives it, the latest status
/// with who recorded it, when it closes by itself and whether it is overdue on `today`. With
/// `detail`, also what the agent wrote: the contact, each line, delivery, terms, the seller's page,
/// the comparison and the reason.
#[must_use]
pub fn order_row(record: &PurchaseOrderRecord, today: NaiveDate, detail: bool) -> Value {
    let body = &record.drafted;
    let mut row = json!({
        "order": record.order,
        "state": record.state.as_str(),
        "seller": body.seller.to_string(),
        "total": body.total.as_str(),
        "currency": body.currency.as_str(),
        "period": body.period.to_string(),
        "task_id": record.task_id,
        "agent_id": record.agent_id,
        "drafted_at": time(record.drafted_at),
        "overdue": overdue(record, today),
    });
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            row[key] = value;
        }
    };
    put("decided_at", record.decided_at.map(|at| json!(time(at))));
    put("note", record.note.as_ref().map(|note| json!(note)));
    put(
        "close_note",
        record.close_note.as_ref().map(|note| json!(note)),
    );
    put(
        "placed_on",
        record.placed_on.map(|day| json!(day.to_string())),
    );
    put("paid", record.paid.as_ref().map(|(paid, _)| json!(paid)));
    put(
        "paid_currency",
        record.paid.as_ref().map(|(_, currency)| json!(currency)),
    );
    put(
        "received_on",
        record.received_on.map(|day| json!(day.to_string())),
    );
    put(
        "renews_on",
        record.renews_on.map(|day| json!(day.to_string())),
    );
    put("ended_at", record.ended_at.map(|at| json!(time(at))));
    put("expires_at", expires_at(record).map(|at| json!(time(at))));
    put(
        "status",
        record.status.as_ref().map(|status| {
            let mut wire = json!({
                "status": status.status.as_str(),
                "note": status.note,
                "by": if status.by_owner { "owner" } else { "agent" },
                "at": time(status.at),
            });
            if let Some(day) = status.expected_on {
                wire["expected_on"] = json!(day.to_string());
            }
            wire
        }),
    );
    if detail {
        row["seller_contact"] = json!(body.seller_contact.to_string());
        row["lines"] = json!(
            body.lines
                .iter()
                .map(|line| json!({
                    "item": line.item.to_string(),
                    "quantity": line.quantity.get(),
                    "unit": line.unit.to_string(),
                    "unit_price": line.unit_price.as_str(),
                    "line_total": line.line_total.as_str(),
                }))
                .collect::<Vec<_>>()
        );
        row["delivery"] = json!(body.delivery.to_string());
        row["terms"] = json!(body.terms.to_string());
        row["url"] = json!(body.url.to_string());
        row["evaluation"] = json!(body.evaluation.to_string());
        row["why"] = json!(body.why.to_string());
    }
    row
}
