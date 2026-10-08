//! The Procurement Specialist's purchase orders and renewals (`docs/SPEC.md` 6.10, ADR 0039): the
//! locks under which an order is numbered, decided, placed, received, closed and expired, the rules
//! of an order's follow-up status, how an order is worded on the wire, and the two rules the clock
//! runs with no model: the orders that close by themselves and the renewals coming up.

use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use calamine::{Data, Range, Reader as _, Xlsx, open_workbook_from_rs};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use farik_core::contract::{Role, TaskId};
use farik_core::renewals::{RegisterRow, due_renewals};
use farik_core::team::{Team, private_folder};
use farik_protocol::event::{
    EventBody, EventIds, PurchaseOrderExpiredBody, PurchaseOrderStatus, RenewalCheckedBody,
    RenewalFlaggedBody, new_event,
};
use farik_store::purchase_orders::{PurchaseOrderRecord, expires_at, overdue, purchase_orders};
use farik_store::renewals::{last_check, renewals};
use farik_store::{EventLog, StoreError};
use serde_json::{Value, json};

use crate::tools::{ToolDeps, ToolError};

/// Held from the first read of the orders to the record that changes them: by the agent's draft,
/// by each of the owner's commands, by a status and by the expiry, so that two of them never take
/// one number, decide one order twice or expire an order being placed. One lock for every project
/// in the process.
pub(crate) static ORDERS: Mutex<()> = Mutex::new(());

/// Held from the first read of the renewals to the record that changes them: by the daily check
/// and by the owner's dismissal.
pub(crate) static RENEWALS: Mutex<()> = Mutex::new(());

/// Held from the first read of the data pipeline requests to the record that changes them: by an
/// agent's request, by each decision (the Product Manager's and the owner's) and by Farik's
/// escalation after three tries, so that two of them never take one name or the limit, decide one
/// request twice, or file its request twice. One lock for every project in the process.
pub(crate) static PIPELINES: Mutex<()> = Mutex::new(());

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

/// `purchase_orders.list`: every order the log holds, oldest first, as the Orders section and the
/// command line word it, with `overdue` as of `today`.
///
/// # Errors
///
/// What the log refused.
pub fn purchase_orders_list(log: &EventLog, today: NaiveDate) -> Result<Value, StoreError> {
    Ok(json!({
        "orders": purchase_orders(log)?
            .iter()
            .map(|order| order_row(order, today, false))
            .collect::<Vec<_>>()
    }))
}

/// `renewals.list`: the renewals Farik flagged that the owner has not dismissed, oldest first, and
/// how many rows the last daily check could not read.
///
/// # Errors
///
/// What the log refused.
pub fn renewals_list(log: &EventLog) -> Result<Value, StoreError> {
    let open: Vec<Value> = renewals(log)?
        .into_iter()
        .filter(|one| !one.dismissed)
        .map(|one| {
            json!({
                "renewal": one.renewal, "vendor": one.vendor,
                "renews_on": one.renews_on.to_string(), "decide_by": one.decide_by.to_string(),
                "flagged_at": time(one.flagged_at),
            })
        })
        .collect();
    let unreadable = last_check(log)?.map_or(0, |check| check.unreadable);
    Ok(json!({ "open": open, "unreadable": unreadable }))
}

/// An event of Farik's own, recorded at `now`: about `task` when it is about one, and with no
/// agent and no session.
fn record_event(
    tools: &ToolDeps,
    task: Option<&TaskId>,
    body: EventBody,
    now: DateTime<Utc>,
) -> Result<(), ToolError> {
    let ids = EventIds {
        task_id: task.cloned(),
        ..tools.ids.clone()
    };
    let failed = |error: &dyn std::fmt::Display| ToolError::Failed {
        detail: error.to_string(),
    };
    let event = new_event(body, now, ids).map_err(|error| ToolError::Failed {
        detail: format!("the event cannot be stamped: {error:?}"),
    })?;
    let appended = tools.log.append(&event).map_err(|error| failed(&error))?;
    tools
        .projections
        .apply(&appended)
        .map_err(|error| failed(&error))
}

/// Closes the orders nobody decided or placed in time: records `purchase_order.expired` for each
/// order drafted 30 days ago or more and not decided, and each approved 30 days ago or more and
/// not placed, whatever its task's state. A placed order never closes by itself. It runs under the
/// orders' lock, so that an expiry and the owner's step on the same order are not both taken.
///
/// # Errors
///
/// `Failed` when the log cannot be read or written.
pub(crate) fn expire_orders(tools: &ToolDeps, now: DateTime<Utc>) -> Result<(), ToolError> {
    let _held = crate::locked(&ORDERS);
    let records = purchase_orders(&tools.log).map_err(|error| ToolError::Failed {
        detail: error.to_string(),
    })?;
    for record in records
        .iter()
        .filter(|record| expires_at(record).is_some_and(|at| at <= now))
    {
        let body: PurchaseOrderExpiredBody =
            serde_json::from_value(json!({ "order": record.order })).map_err(|error| {
                ToolError::Failed {
                    detail: error.to_string(),
                }
            })?;
        record_event(
            tools,
            Some(&record.task_id),
            EventBody::PurchaseOrderExpired(body),
            now,
        )?;
    }
    Ok(())
}

/// A cell as the text a person sees: a date cell as its ISO day, a whole number as its digits.
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::String(text) | Data::DateTimeIso(text) | Data::DurationIso(text) => text.clone(),
        Data::Int(number) => number.to_string(),
        Data::Float(number) => number.to_string(),
        Data::Bool(flag) => flag.to_string(),
        Data::DateTime(moment) if !moment.is_duration() => {
            let (year, month, day, ..) = moment.to_ymd_hms_milli();
            format!("{year:04}-{month:02}-{day:02}")
        }
        Data::DateTime(moment) => moment.as_f64().to_string(),
        Data::Empty | Data::Error(_) => String::new(),
    }
}

/// How many rows of the sheet below its first row hold something; `range` starts at the first row
/// that holds something, so when that is not the sheet's first row, every row of it is below it.
fn rows_below_the_first(range: &Range<Data>) -> u32 {
    let first_row_is_empty = range.start().is_some_and(|at| at.0 != 0);
    let held = range
        .rows()
        .skip(usize::from(!first_row_is_empty))
        .filter(|row| row.iter().any(|cell| !cell_text(cell).is_empty()))
        .count();
    u32::try_from(held).unwrap_or(u32::MAX)
}

/// The `Vendors` sheet of the register at `path`: each row's `vendor`, `renews_on`, `notice_days`
/// and `status` as text, the columns found by their heading in the sheet's first row (case and
/// surrounding spaces ignored), so that a column the user moved still reads; and how many rows it
/// could not read for want of a column to read them by. A file that is no workbook, a workbook with
/// no `Vendors` sheet and a sheet with no rows count one; a sheet whose first row lacks any of
/// the four headings counts each of its rows below the first that holds something, and at least
/// one.
///
/// # Errors
///
/// What `read_workbook_file` refuses: a file that is not there or past 10 MiB.
pub(crate) fn read_register(path: &Path) -> Result<(Vec<RegisterRow>, u32), ToolError> {
    let bytes = crate::tools::sheets::read_workbook_file(path, "vendors.xlsx")?;
    let Ok(mut book) = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(bytes)) else {
        return Ok((Vec::new(), 1));
    };
    let Ok(range) = book.worksheet_range("Vendors") else {
        return Ok((Vec::new(), 1));
    };
    let headings: Vec<String> = range
        .rows()
        .next()
        .filter(|_| range.start().is_some_and(|at| at.0 == 0))
        .map(|first| {
            first
                .iter()
                .map(|cell| cell_text(cell).trim().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let column = |name: &str| headings.iter().position(|heading| heading == name);
    let (Some(vendor), Some(renews_on), Some(notice_days), Some(status)) = (
        column("vendor"),
        column("renews_on"),
        column("notice_days"),
        column("status"),
    ) else {
        return Ok((Vec::new(), rows_below_the_first(&range).max(1)));
    };
    let rows = range
        .rows()
        .skip(1)
        .filter(|row| row.iter().any(|cell| !cell_text(cell).is_empty()))
        .map(|row| {
            let at = |column: usize| row.get(column).map(cell_text).unwrap_or_default();
            RegisterRow {
                vendor: at(vendor),
                renews_on: at(renews_on),
                notice_days: at(notice_days),
                status: at(status),
            }
        })
        .collect();
    Ok((rows, 0))
}

/// The register's daily check, once per UTC day and with no model: reads the `Vendors` sheet of
/// `vendors.xlsx` in the Procurement Specialist's folder, records `renewal.flagged` for each
/// renewal whose decision date is two weeks off or nearer that was not flagged before, and
/// `renewal.checked` with how many it flagged and how many rows it could not read, which is how
/// the next tick knows the day's check ran. It runs only when the team has an active Procurement
/// Specialist and the register is a file in its folder; a register it cannot open counts as one
/// row it cannot read.
///
/// # Errors
///
/// `Failed` when the log cannot be read or written.
pub(crate) fn check_renewals(
    tools: &ToolDeps,
    team: &Team,
    now: DateTime<Utc>,
) -> Result<(), ToolError> {
    let held = team
        .active_agents()
        .any(|agent| Role::from(agent.role) == Role::ProcurementSpecialist);
    let Some(folder) = private_folder(Role::ProcurementSpecialist).filter(|_| held) else {
        return Ok(());
    };
    let Ok(path) = crate::tools::sheets::private_path(tools.files.root(), folder, "vendors.xlsx")
    else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let failed = |error: &dyn std::fmt::Display| ToolError::Failed {
        detail: error.to_string(),
    };
    let _held = crate::locked(&RENEWALS);
    let today = now.date_naive();
    if last_check(&tools.log)
        .map_err(|error| failed(&error))?
        .is_some_and(|check| check.at.date_naive() == today)
    {
        return Ok(());
    }
    let (rows, unreadable) = read_register(&path).unwrap_or_else(|_| (Vec::new(), 1));
    let flagged: Vec<(String, NaiveDate)> = renewals(&tools.log)
        .map_err(|error| failed(&error))?
        .into_iter()
        .map(|one| (one.vendor, one.renews_on))
        .collect();
    let (due, unread) = due_renewals(&rows, today, &flagged);
    for renewal in &due {
        let body: RenewalFlaggedBody = serde_json::from_value(json!({
            "vendor": renewal.vendor,
            "renews_on": renewal.renews_on.to_string(),
            "decide_by": renewal.decide_by.to_string(),
        }))
        .map_err(|error| failed(&error))?;
        record_event(tools, None, EventBody::RenewalFlagged(body), now)?;
    }
    let body = checked_body(due.len(), unreadable.saturating_add(unread))
        .map_err(|error| failed(&error))?;
    record_event(tools, None, EventBody::RenewalChecked(body), now)
}

/// The most either count of `renewal.checked` holds: the event's schema takes no more.
const MOST_COUNTED: u32 = 1_000_000;

/// The body of `renewal.checked` for a run that flagged `due` renewals and could not read
/// `unreadable` rows, each count held to what the schema takes, so that a register with more rows
/// than that cannot make every tick fail on it.
fn checked_body(due: usize, unreadable: u32) -> Result<RenewalCheckedBody, serde_json::Error> {
    serde_json::from_value(json!({
        "due": u32::try_from(due).unwrap_or(MOST_COUNTED).min(MOST_COUNTED),
        "unreadable": unreadable.min(MOST_COUNTED),
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use farik_protocol::clock::MovableClock;
    use farik_protocol::event::{EventBody, EventKind};
    use farik_store::purchase_orders::{OrderState, overdue, purchase_orders};
    use serde_json::json;

    use crate::orchestrator::fixtures::Harness;
    use crate::tools::fixtures::waits_for_the_lock;
    use crate::tools::sheets::{CellInput, SheetInput, write_new_workbook};

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a time")
            .with_timezone(&Utc)
    }

    /// An order `number` that `proc` drafted on FRK-1 at `when`.
    fn drafted(harness: &Harness, number: u64, when: DateTime<Utc>) {
        harness.project.record_by(
            Some("proc"),
            when,
            "FRK-1",
            "purchase_order.drafted",
            &json!({
                "order": number, "seller": format!("Seller {number}"), "seller_contact": "",
                "lines": [{ "item": "Box", "quantity": 1, "unit": "", "unit_price": "1.00", "line_total": "1.00" }],
                "currency": "USD", "period": "once", "total": "1.00", "delivery": "", "terms": "",
                "url": "", "evaluation": "evaluations/boxes.md", "why": "A seller of boxes for the team."
            }),
        );
    }

    fn states(harness: &Harness) -> Vec<(u64, OrderState)> {
        purchase_orders(&harness.project.deps.log)
            .expect("the log reads")
            .iter()
            .map(|record| (record.order, record.state))
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_order_closes_by_itself_after_thirty_days() {
        let harness = Harness::with_procurement("procurement-expiry");
        let start = utc("2026-09-01T09:00:00Z");
        // 1 is drafted and left; 2 is approved on day 5 and never placed; 3 is placed on day 5.
        drafted(&harness, 1, start);
        drafted(&harness, 2, start + Duration::days(1));
        drafted(&harness, 3, start + Duration::days(1));
        for (kind, body) in [
            ("purchase_order.approved", json!({ "order": 2, "note": "" })),
            ("purchase_order.approved", json!({ "order": 3, "note": "" })),
            (
                "purchase_order.placed",
                json!({ "order": 3, "placed_on": "2026-09-06" }),
            ),
        ] {
            harness
                .project
                .record_at(start + Duration::days(5), "FRK-1", kind, &body);
        }
        let clock = Arc::new(MovableClock::new(start));
        let orchestrator =
            harness.orchestrator_on(harness.recorded(Vec::new()), Arc::clone(&clock));
        let expired = |harness: &Harness| harness.events(&[EventKind::PurchaseOrderExpired]);

        // Not a minute before the drafted order's thirtieth day.
        clock.set(start + Duration::days(30) - Duration::minutes(1));
        orchestrator.tick().await.expect("a tick");
        assert!(expired(&harness).is_empty());

        clock.set(start + Duration::days(30));
        orchestrator.tick().await.expect("a tick");
        let events = expired(&harness);
        assert_eq!(events.len(), 1);
        let ids = &events[0].envelope.ids;
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(
            (&ids.agent_id, &ids.session_id),
            (&None, &None),
            "Farik's own"
        );
        let EventBody::PurchaseOrderExpired(body) = &events[0].body else {
            panic!("an expiry");
        };
        assert_eq!(body.order.get(), 1);
        assert_eq!(events[0].envelope.recorded_at, start + Duration::days(30));
        assert_eq!(
            states(&harness),
            [
                (1, OrderState::Expired),
                (2, OrderState::Approved),
                (3, OrderState::Placed)
            ]
        );

        // An approved order, thirty days after its approval; two ticks record one expiry.
        clock.set(start + Duration::days(35) - Duration::minutes(1));
        orchestrator.tick().await.expect("a tick");
        assert_eq!(expired(&harness).len(), 1);
        clock.set(start + Duration::days(35));
        orchestrator.tick().await.expect("a tick");
        orchestrator.tick().await.expect("a tick");
        assert_eq!(
            expired(&harness).len(),
            2,
            "one expiry for each, however many ticks"
        );
        assert_eq!(
            states(&harness),
            [
                (1, OrderState::Expired),
                (2, OrderState::Expired),
                (3, OrderState::Placed)
            ]
        );

        // A placed order never closes by itself, and is overdue past its expected day.
        clock.set(start + Duration::days(400));
        orchestrator.tick().await.expect("a tick");
        assert_eq!(states(&harness)[2], (3, OrderState::Placed));
        let placed = purchase_orders(&harness.project.deps.log)
            .expect("reads")
            .remove(2);
        assert!(!overdue(
            &placed,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 6).expect("a date")
        ));
        assert!(overdue(
            &placed,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 7).expect("a date")
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_paused_team_closes_its_orders_too() {
        let harness = Harness::with_procurement("procurement-expiry-paused");
        let start = utc("2026-09-01T09:00:00Z");
        drafted(&harness, 1, start);
        harness
            .project
            .record("", "team.paused", &json!({ "by": "human" }));
        let clock = Arc::new(MovableClock::new(start + Duration::days(31)));
        let orchestrator = harness.orchestrator_on(harness.recorded(Vec::new()), clock);

        orchestrator.tick().await.expect("a tick");

        assert_eq!(states(&harness), [(1, OrderState::Expired)]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_expiry_waits_for_the_orders_lock() {
        let harness = Harness::with_procurement("procurement-expiry-lock");
        let start = utc("2026-09-01T09:00:00Z");
        drafted(&harness, 1, start);

        // An expiry that did not wait could land between the owner's placing of an order, read,
        // and its record: the owner would be told "Marked placed" and the fold would say expired.
        waits_for_the_lock(&super::ORDERS, &harness.project, || {
            super::expire_orders(&harness.project.deps, start + Duration::days(31))
        })
        .expect("the expiry goes on once the lock is free");

        assert_eq!(states(&harness), [(1, OrderState::Expired)]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_renewal_check_waits_for_the_renewals_lock() {
        let harness = Harness::with_procurement("procurement-renewals-lock");
        write_register(
            &harness,
            vec![
                text_row(&["vendor", "renews_on", "notice_days", "status"]),
                vendor_row("Vercel", "2026-10-20", "7", "active"),
            ],
        );
        let team = harness.project.deps.files.read_team().expect("the team");

        // The check and the owner's dismissal hold one lock, so that a dismissal is never taken
        // between the check's read of the flagged renewals and its record.
        waits_for_the_lock(&super::RENEWALS, &harness.project, || {
            super::check_renewals(&harness.project.deps, &team, utc("2026-10-05T08:00:00Z"))
        })
        .expect("the check goes on once the lock is free");

        assert_eq!(harness.events(&[EventKind::RenewalFlagged]).len(), 1);
        assert_eq!(harness.events(&[EventKind::RenewalChecked]).len(), 1);
    }

    /// The Procurement Specialist's register with `rows` under its headings, replacing any.
    fn write_register(harness: &Harness, rows: Vec<Vec<CellInput>>) {
        let folder = harness.procurement_folder();
        let path = folder.join("vendors.xlsx");
        let _ = std::fs::remove_file(&path);
        write_new_workbook(&folder, &path, &[SheetInput::new("Vendors", rows)])
            .expect("a register is written");
    }

    fn text_row(cells: &[&str]) -> Vec<CellInput> {
        cells.iter().map(|cell| CellInput::text(cell)).collect()
    }

    fn vendor_row(vendor: &str, renews_on: &str, notice: &str, status: &str) -> Vec<CellInput> {
        text_row(&[vendor, renews_on, notice, status])
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_tick_runs_once_a_day_without_a_session() {
        let harness = Harness::with_procurement("procurement-renewals");
        let heading = || text_row(&["vendor", "renews_on", "notice_days", "status"]);
        write_register(
            &harness,
            vec![
                heading(),
                vendor_row("Vercel", "2026-10-20", "7", "active"),
                vendor_row("Notion", "next month", "", "active"),
                vendor_row("Old", "2026-10-10", "0", "cancelled"),
                vendor_row("Later", "2027-01-01", "30", "active"),
            ],
        );
        let clock = Arc::new(MovableClock::new(utc("2026-10-05T08:00:00Z")));
        let orchestrator =
            harness.orchestrator_on(harness.recorded(Vec::new()), Arc::clone(&clock));

        orchestrator.tick().await.expect("a tick");
        clock.set(utc("2026-10-05T20:00:00Z"));
        orchestrator.tick().await.expect("a second tick");

        let flagged = harness.events(&[EventKind::RenewalFlagged]);
        assert_eq!(
            flagged.len(),
            1,
            "one flag for a due row, however many ticks"
        );
        let EventBody::RenewalFlagged(body) = &flagged[0].body else {
            panic!("a flag");
        };
        assert_eq!(body.vendor.to_string(), "Vercel");
        assert_eq!(body.renews_on.to_string(), "2026-10-20");
        assert_eq!(body.decide_by.to_string(), "2026-10-13");
        let ids = &flagged[0].envelope.ids;
        assert_eq!(
            (&ids.task_id, &ids.agent_id, &ids.session_id),
            (&None, &None, &None)
        );
        let checked = harness.events(&[EventKind::RenewalChecked]);
        assert_eq!(checked.len(), 1, "one check a UTC day");
        let EventBody::RenewalChecked(body) = &checked[0].body else {
            panic!("a check");
        };
        assert_eq!((body.due, body.unreadable), (1, 1));
        assert!(
            harness.events(&[EventKind::SessionStarted]).is_empty(),
            "no model runs for it"
        );

        // The next day it checks again and flags nothing twice; a row that became due is flagged.
        write_register(
            &harness,
            vec![
                heading(),
                vendor_row("Vercel", "2026-10-20", "7", "active"),
                vendor_row("Later", "2026-10-30", "30", "trial"),
            ],
        );
        clock.set(utc("2026-10-06T00:00:00Z"));
        orchestrator.tick().await.expect("a tick");
        assert_eq!(harness.events(&[EventKind::RenewalChecked]).len(), 2);
        let flagged = harness.events(&[EventKind::RenewalFlagged]);
        assert_eq!(flagged.len(), 2);
        let EventBody::RenewalFlagged(body) = &flagged[1].body else {
            panic!("a flag");
        };
        assert_eq!(body.vendor.to_string(), "Later");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_tick_reads_a_register_only_where_there_is_one() {
        let clock = || Arc::new(MovableClock::new(utc("2026-10-05T08:00:00Z")));
        let register = || {
            vec![
                text_row(&["vendor", "renews_on", "notice_days", "status"]),
                vendor_row("Vercel", "2026-10-20", "7", "active"),
            ]
        };
        // No Procurement Specialist on the team: the register, if any, is nobody's to watch.
        let without = Harness::new("procurement-renewals-no-role", |_| {});
        write_register(&without, register());
        let orchestrator = without.orchestrator_on(without.recorded(Vec::new()), clock());
        orchestrator.tick().await.expect("a tick");
        assert!(
            without
                .events(&[EventKind::RenewalChecked, EventKind::RenewalFlagged])
                .is_empty()
        );
        // The role, and no register.
        let none = Harness::with_procurement("procurement-renewals-no-register");
        let orchestrator = none.orchestrator_on(none.recorded(Vec::new()), clock());
        orchestrator.tick().await.expect("a tick");
        assert!(
            none.events(&[EventKind::RenewalChecked, EventKind::RenewalFlagged])
                .is_empty()
        );
        // A retired Procurement Specialist is no one to watch for.
        let retired = Harness::with_procurement("procurement-renewals-retired");
        write_register(&retired, register());
        let orchestrator = retired.orchestrator_on(retired.recorded(Vec::new()), clock());
        for id in ["proc", "proc-2"] {
            orchestrator
                .handle(farik_protocol::command::Command::AgentUpdate {
                    agent_id: id.to_string(),
                    status: farik_core::team::AgentStatus::Retired,
                })
                .await
                .expect("the agent is retired");
        }
        orchestrator.tick().await.expect("a tick");
        assert!(retired.events(&[EventKind::RenewalChecked]).is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_tick_does_not_read_a_register_that_is_a_link() {
        let harness = Harness::with_procurement("procurement-renewals-link");
        write_register(
            &harness,
            vec![
                text_row(&["vendor", "renews_on", "notice_days", "status"]),
                vendor_row("Vercel", "2026-10-20", "7", "active"),
            ],
        );
        // The register is the folder's own file no longer: its name is a link to another.
        let folder = harness.procurement_folder();
        std::fs::rename(folder.join("vendors.xlsx"), folder.join("elsewhere.xlsx"))
            .expect("the workbook is moved");
        std::os::unix::fs::symlink(folder.join("elsewhere.xlsx"), folder.join("vendors.xlsx"))
            .expect("a link is made");
        let clock = Arc::new(MovableClock::new(utc("2026-10-05T08:00:00Z")));
        let orchestrator = harness.orchestrator_on(harness.recorded(Vec::new()), clock);

        orchestrator.tick().await.expect("a tick");

        assert!(
            harness
                .events(&[EventKind::RenewalChecked, EventKind::RenewalFlagged])
                .is_empty(),
            "a link is not followed to a file outside the folder's rules"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_columns_by_their_header() {
        use super::read_register;
        let harness = Harness::with_procurement("procurement-register");
        let path = harness.procurement_folder().join("vendors.xlsx");
        let rows_of = |harness: &Harness, rows: Vec<Vec<CellInput>>| {
            write_register(harness, rows);
            read_register(&path).expect("the register reads")
        };
        let row = |vendor: &str, renews_on: &str, notice: &str, status: &str| {
            farik_core::renewals::RegisterRow {
                vendor: vendor.to_string(),
                renews_on: renews_on.to_string(),
                notice_days: notice.to_string(),
                status: status.to_string(),
            }
        };

        // The columns in any order and among others, a date cell read as its day, a whole number
        // as its digits, a heading's case and spaces ignored.
        let (rows, unreadable) = rows_of(
            &harness,
            vec![
                text_row(&["Renews_on ", "notes", " VENDOR", "status", "notice_days"]),
                vec![
                    CellInput::date("2026-11-30"),
                    CellInput::text("x"),
                    CellInput::text("Vercel"),
                    CellInput::text("active"),
                    CellInput::number(30.0),
                ],
                vec![
                    CellInput::text("2026-12-15"),
                    CellInput::empty(),
                    CellInput::text("Notion"),
                    CellInput::text("trial"),
                    CellInput::empty(),
                ],
                vec![
                    CellInput::text("soon"),
                    CellInput::empty(),
                    CellInput::text("Odd"),
                    CellInput::text("active"),
                    CellInput::number(2.5),
                ],
            ],
        );
        assert_eq!(unreadable, 0);
        assert_eq!(
            rows,
            [
                row("Vercel", "2026-11-30", "30", "active"),
                row("Notion", "2026-12-15", "", "trial"),
                row("Odd", "soon", "2.5", "active"),
            ]
        );

        // A sheet that lacks a heading: each row below the headings is unreadable.
        let (rows, unreadable) = rows_of(
            &harness,
            vec![
                text_row(&["vendor", "renews_on", "status"]),
                text_row(&["A", "2026-11-30", "active"]),
                text_row(&["B", "2026-11-30", "active"]),
                Vec::new(),
                text_row(&["C", "2026-11-30", "active"]),
            ],
        );
        assert_eq!((rows.len(), unreadable), (0, 3), "the empty row is no row");
        // Headings that are not in the first row are no headings: each of the three rows that
        // hold something is below an empty first row.
        let (rows, unreadable) = rows_of(
            &harness,
            vec![
                Vec::new(),
                Vec::new(),
                text_row(&["vendor", "renews_on", "notice_days", "status"]),
                vendor_row("A", "2026-11-30", "0", "active"),
                vendor_row("B", "2026-11-30", "0", "active"),
            ],
        );
        assert_eq!((rows.len(), unreadable), (0, 3));
        // A sheet with no rows at all, and no sheet named Vendors, each count one.
        let (rows, unreadable) = rows_of(&harness, Vec::new());
        assert_eq!((rows.len(), unreadable), (0, 1));
        let folder = harness.procurement_folder();
        std::fs::remove_file(&path).expect("the register goes");
        write_new_workbook(
            &folder,
            &path,
            &[SheetInput::new(
                "Other",
                vec![text_row(&["a"]), text_row(&["b"])],
            )],
        )
        .expect("a workbook");
        assert_eq!(read_register(&path).expect("it reads"), (Vec::new(), 1));
        // A file that is no workbook counts one too, and a missing file is an error to the tick.
        std::fs::write(&path, b"not a workbook").expect("a file");
        assert_eq!(read_register(&path).expect("it reads"), (Vec::new(), 1));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_check_counts_the_rows_of_a_register_it_cannot_read_by_heading() {
        let harness = Harness::with_procurement("procurement-renewals-headless");
        write_register(
            &harness,
            vec![
                text_row(&["vendor", "renews_on", "status"]),
                text_row(&["Vercel", "2026-10-20", "active"]),
                text_row(&["Notion", "2026-10-21", "active"]),
            ],
        );
        let clock = Arc::new(MovableClock::new(utc("2026-10-05T08:00:00Z")));
        let orchestrator = harness.orchestrator_on(harness.recorded(Vec::new()), clock);

        orchestrator.tick().await.expect("a tick");

        let checked = harness.events(&[EventKind::RenewalChecked]);
        let EventBody::RenewalChecked(body) = &checked[0].body else {
            panic!("a check");
        };
        assert_eq!(
            (body.due, body.unreadable),
            (0, 2),
            "none flagged by guessing"
        );
        assert!(harness.events(&[EventKind::RenewalFlagged]).is_empty());
    }

    #[test]
    fn counts_past_a_million_are_held_to_a_million() {
        use farik_protocol::event::event_from_value;
        // A register can hold more rows than the event's schema counts. The log reads every event
        // back through the schema, so one it refuses would fail every later read of its kind, the
        // next tick's included, and stop the team: the check records the most the schema counts.
        let wire = |body: &super::RenewalCheckedBody| {
            json!({
                "seq": 1, "recorded_at": "2026-10-05T08:00:00Z", "team_id": "farik",
                "project_id": "farik", "kind": "renewal.checked",
                "body": serde_json::to_value(body).expect("a body is a value"),
            })
        };
        let body = super::checked_body(2_000_000, u32::MAX).expect("a body");
        event_from_value(&wire(&body)).expect("the schema takes it");
        assert_eq!((body.due, body.unreadable), (1_000_000, 1_000_000));
        // Fewer are counted as they are.
        let body = super::checked_body(3, 7).expect("a body");
        assert_eq!((body.due, body.unreadable), (3, 7));
        let body = super::checked_body(1_000_000, 1_000_000).expect("a body");
        event_from_value(&wire(&body)).expect("the schema takes it");
        assert_eq!((body.due, body.unreadable), (1_000_000, 1_000_000));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_tick_scoped_to_a_task_leaves_the_clock_s_work_alone() {
        let harness = Harness::with_procurement("procurement-scoped");
        let start = utc("2026-09-01T09:00:00Z");
        drafted(&harness, 1, start);
        let clock = Arc::new(MovableClock::new(start + Duration::days(31)));
        let orchestrator = harness.orchestrator_on(harness.recorded(Vec::new()), clock);

        let scope = crate::orchestrator::TickScope {
            task_id: Some("FRK-1".parse().expect("a task id")),
            ..crate::orchestrator::TickScope::default()
        };
        orchestrator
            .tick_within(&scope)
            .await
            .expect("a scoped tick");
        assert_eq!(states(&harness), [(1, OrderState::Drafted)]);

        orchestrator.tick().await.expect("a tick");
        assert_eq!(states(&harness), [(1, OrderState::Expired)]);
    }
}
