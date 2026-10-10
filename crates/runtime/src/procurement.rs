//! The Procurement Specialist's purchase orders and renewals (`docs/SPEC.md` 6.10, ADR 0039): the
//! locks under which an order is numbered, decided, placed, received, closed and expired, the rules
//! of an order's follow-up status, how an order is worded on the wire, and the two rules the clock
//! runs with no model: the orders that close by themselves and the renewals coming up.

mod mail;
mod replies;
#[cfg(test)]
mod story;

pub(crate) use mail::{
    Mailer, SendAsk, discard_message, draft_text, order_is_sending, prepare, record_failed,
    record_sent, send_message, sent_parts, transmit,
};
pub use mail::{add_order_send_fields, seller_messages_list};
pub(crate) use replies::reply_folder;
pub(crate) use replies::{check_by_hand, dismiss_reply, start_check};
pub use replies::{reply_attachment, seller_replies_list};

use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use calamine::{Data, Range, Reader as _, Xlsx, open_workbook_from_rs};
use catervas_core::contract::{Role, TaskId};
use catervas_core::pipeline::PipelineCost;
use catervas_core::renewals::{RegisterRow, due_renewals};
use catervas_core::team::{Team, private_folder};
use catervas_protocol::event::{
    EventBody, EventIds, MailboxConnectedBody, MailboxDisconnectedBody, MailboxPurpose,
    PurchaseOrderExpiredBody, PurchaseOrderStatus, RenewalCheckedBody, RenewalFlaggedBody,
    new_event,
};
use catervas_roles::{Kit, KitConnector};
use catervas_store::pipelines::{PipelineRecord, data_pipelines};
use catervas_store::purchase_orders::{PurchaseOrderRecord, expires_at, overdue, purchase_orders};
use catervas_store::renewals::{last_check, renewals};
use catervas_store::requests::{
    RequestError, file_request, placeholder_budget_usd, request_from_text,
};
use catervas_store::seller_mail::{seller_mail, sent_on};
use catervas_store::waiting::PipelineAsk;
use catervas_store::{EventLog, StoreError};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::claude::Secret;
use crate::credential::CredentialError;
use crate::mailbox::{
    Ledger, MailboxAt, MailboxError, MailboxSecrets, MailboxSettings, ProviderChoice, ProviderOf,
    Server, Trust, check_login, provider_of, servers, validate_settings,
};
use crate::tools::{ToolDeps, ToolError};

/// Held from the first read of the orders to the record that changes them: by the agent's draft,
/// by each of the owner's commands, by a status and by the expiry, so that two of them never take
/// one number, decide one order twice or expire an order being placed. One lock for every project
/// in the process.
pub(crate) static ORDERS: Mutex<()> = Mutex::new(());

/// Held by everything that numbers, writes, discards or counts a message to a seller or a reply,
/// and by connecting the mailbox: drafting, the first and last steps of a send, discarding, the
/// check, and connecting, so that two of them never take one number or send one message twice.
/// A std lock, as `ORDERS` is: every tool handler is a plain function called from async code, so
/// an async lock could not be taken by a draft, and a std guard cannot be held across a server.
/// A send holds this lock to claim its message, lets it go while it talks to the servers, and takes
/// it again to record, with the claim (`SENDING`) refusing a second send meanwhile.
pub(crate) static MAIL: Mutex<()> = Mutex::new(());

/// Held from the first read of the renewals to the record that changes them: by the daily check
/// and by the owner's dismissal.
pub(crate) static RENEWALS: Mutex<()> = Mutex::new(());

/// Held from the first read of the data pipeline requests to the record that changes them: by an
/// agent's request, by each decision (the Product Manager's and the owner's) and by Catervas's
/// escalation after three tries, so that two of them never take one name or the limit, decide one
/// request twice, or file its request twice. One lock for every project in the process.
pub(crate) static PIPELINES: Mutex<()> = Mutex::new(());

/// The title of the service of `kit` that `name` names, without regard to case: the service's name
/// or its title. A data pipeline request for such a service asks the owner to connect it.
pub(crate) fn kit_connector_title(kit: &Kit, name: &str) -> Option<String> {
    let wanted = name.trim().to_lowercase();
    kit.connectors.iter().find_map(|connector| match connector {
        KitConnector::Server { entry, copy, .. }
            if entry.name.as_str().to_lowercase() == wanted
                || copy.title.to_lowercase() == wanted =>
        {
            Some(copy.title.clone())
        }
        _ => None,
    })
}

/// The ordinary request an approval of a data pipeline files, in words: its first line is its
/// title, then what the source gives, its page and why the agent asked, one line each (what the
/// agent wrote across several lines is joined into one), and, when the source is a service of the
/// role's kit (`kit_connector` is its title), that connecting it is the owner's, on the role's
/// page (spec 6.7). All of it but the title's two phrases and the labels is the agent's.
pub(crate) fn pipeline_request_text(
    record: &PipelineRecord,
    kit_connector: Option<&str>,
) -> String {
    let body = &record.requested;
    let on_one_line = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut lines = vec![
        format!(
            "Set up {} for the Procurement Specialist.",
            on_one_line(body.name.as_str())
        ),
        format!("What it gives: {}", on_one_line(body.what.as_str())),
        format!("Source: {}", body.source_url.as_str()),
        format!("Asked because: {}", on_one_line(body.why.as_str())),
    ];
    if let Some(title) = kit_connector {
        lines.push(format!(
            "Connect {title} on the Procurement Specialist's page."
        ));
    }
    lines.join("\n")
}

/// The text of the request an approval of data pipeline request `pipeline` files, which the owner
/// reads before they approve, made as the approval makes it: from the log's record and the
/// Procurement Specialist's `kit`. `None` when the log has no such request.
///
/// # Errors
///
/// What the log refused.
pub fn pipeline_text(
    log: &EventLog,
    kit: Option<&Kit>,
    pipeline: u64,
) -> Result<Option<String>, StoreError> {
    Ok(data_pipelines(log)?
        .into_iter()
        .find(|record| record.pipeline == pipeline)
        .map(|record| {
            let connector =
                kit.and_then(|kit| kit_connector_title(kit, record.requested.name.as_str()));
            pipeline_request_text(&record, connector.as_deref())
        }))
}

/// Adds to `row`, a `waiting.list` row of kind `data_pipeline`, the fields of the request that
/// waits: its number, name, what it gives, the source's page as written and its site, why, the
/// agent's three answers, the Product Manager's reason when it passed the request on, when the
/// agent asked and, when given, the text an approval would file. The same row is `catervas pipeline
/// list --json`'s.
pub fn add_pipeline_fields(row: &mut Value, ask: &PipelineAsk, request_text: Option<&str>) {
    row["pipeline"] = json!(ask.pipeline);
    row["name"] = json!(ask.name);
    row["what"] = json!(ask.what);
    row["url"] = json!(ask.url);
    row["host"] = json!(ask.host);
    row["why"] = json!(ask.why);
    row["cost"] = json!(match ask.cost {
        PipelineCost::Free => "free",
        PipelineCost::Paid => "paid",
        PipelineCost::Unknown => "unknown",
    });
    row["needs_account"] = json!(ask.needs_account);
    row["sends_project_data"] = json!(ask.sends_project_data);
    if let Some(reason) = &ask.reason {
        row["reason"] = json!(reason);
    }
    row["at"] = json!(time(ask.at));
    if let Some(text) = request_text {
        row["request_text"] = json!(text);
    }
}

/// Files the request an approval of `record` asks for, as `created_by`, past the filing lock, and
/// answers its id.
///
/// # Errors
///
/// A sentence saying why the request was not filed.
pub(crate) fn file_pipeline_request(
    deps: &ToolDeps,
    team: &Team,
    record: &PipelineRecord,
    created_by: &str,
    ids: &EventIds,
) -> Result<TaskId, String> {
    let connector = (deps.kits)(Role::ProcurementSpecialist)
        .ok()
        .and_then(|kit| kit_connector_title(&kit, record.requested.name.as_str()));
    let text = pipeline_request_text(record, connector.as_deref());
    let wire = request_from_text(&text, placeholder_budget_usd(&team.rules()))?;
    let filed = file_request(
        &deps.files,
        &deps.log,
        wire,
        created_by,
        None,
        deps.clock.now(),
        ids,
        None,
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => format!("the request {reason}"),
        other => other.to_string(),
    })?;
    deps.projections
        .catch_up()
        .map_err(|error| error.to_string())?;
    Ok(filed.id)
}

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

/// One order as `purchase_orders.list` and the agent's `catervas_read_purchase_orders` word it: its
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

/// `renewals.list`: the renewals Catervas flagged that the owner has not dismissed, oldest first, and
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

/// An event of Catervas's own, recorded at `now`: about `task` when it is about one, and with no
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
        // An order being emailed is the owner's press in flight: it is decided or it fails first.
        .filter(|record| !order_is_sending(tools, record.order))
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

/// The folder of the procurement mailbox's files under `root`: `.catervas/local/procurement/mail`.
const MAIL_FOLDER: &str = "mail";

/// Whether the mailbox files can be kept: the folder `.catervas/local/procurement/mail` of `deps`'s
/// project, made owner-only, refused if it or any part of its path is a link.
pub(crate) fn mail_dir(deps: &ToolDeps) -> Result<std::path::PathBuf, MailboxRefusal> {
    mail_dir_in(deps.files.root()).map_err(|why| {
        MailboxRefusal::new(
            "mailbox_files",
            format!("The mailbox\u{2019}s folder could not be made: {why}"),
        )
    })
}

/// [`mail_dir`] for the project at `root`; the reason, when it cannot be made.
fn mail_dir_in(root: &Path) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt as _;
    let folder = private_folder(Role::ProcurementSpecialist)
        .ok_or_else(|| "the role has no folder".to_string())?;
    let mut at = root.to_path_buf();
    for part in folder.split('/').chain([MAIL_FOLDER]) {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("{} is a link", at.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&at)
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(at)
}

/// Copies the procurement mailbox's `mail/mailbox.json` and `mail/ledger.json` from the project at
/// `from` to the one at `to`, so that a project taken on with the same team finds the mailbox
/// connected, and mail already read is not read again. The folder is made as [`mail_dir`] makes
/// it, refused if any part of it is a link; each file is written owner-only. Answers the address,
/// or `None` (and writes nothing) when `from` has no `mailbox.json`. The password is not here: it
/// is a secret, and `connectors::copy_keys` copies it.
///
/// # Errors
///
/// The files could not be read or written, or the folder in `to` is, or lies under, a link.
pub fn carry_mailbox(from: &Path, to: &Path) -> std::io::Result<Option<String>> {
    let Some(folder) = private_folder(Role::ProcurementSpecialist) else {
        return Ok(None);
    };
    let old = from.join(folder).join(MAIL_FOLDER);
    let settings = match std::fs::read(old.join("mailbox.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        read => read?,
    };
    let invalid = |why: String| std::io::Error::new(std::io::ErrorKind::InvalidData, why);
    let address = serde_json::from_slice::<Value>(&settings)
        .map_err(|error| invalid(error.to_string()))?
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("mailbox.json has no address".to_string()))?
        .to_string();
    let ledger = match std::fs::read(old.join("ledger.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        read => Some(read?),
    };
    let _held = crate::locked(&MAIL);
    let dir = mail_dir_in(to).map_err(std::io::Error::other)?;
    crate::write_private(&dir.join("mailbox.json"), &settings)?;
    if let Some(ledger) = ledger {
        crate::write_private(&dir.join("ledger.json"), &ledger)?;
    }
    Ok(Some(address))
}

/// The body of `mailbox.connected` for the procurement mailbox at `address`: what
/// [`connect_mailbox`] records, and what a project taken on with a carried mailbox records.
///
/// # Errors
///
/// `address` is not one the event takes.
pub fn mailbox_connected(address: &str) -> Result<EventBody, String> {
    Ok(EventBody::MailboxConnected(MailboxConnectedBody {
        purpose: MailboxPurpose::Procurement,
        address: address.parse().map_err(|error| format!("{error}"))?,
    }))
}

/// What the owner connects: the mailbox's settings but the password, with the provider as chosen
/// on the page (which can be Microsoft, refused).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MailboxConnect {
    /// The address mail is sent from and read for.
    pub address: String,
    /// The name sellers see.
    pub name: String,
    /// The provider chosen.
    pub provider: ProviderChoice,
    /// Where mail is read.
    pub imap: Server,
    /// Where mail is sent.
    pub smtp: Server,
    /// The sign-in name.
    pub username: String,
    /// The folder Catervas reads.
    pub folder: String,
    /// What Catervas adds under every message.
    pub signature: String,
    /// Whether Catervas says an AI assistant wrote it.
    pub disclose_ai: bool,
}

/// Why a mailbox command was refused: a code and Catervas's words, which never quote the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxRefusal {
    /// The refusal code the owner is shown.
    pub code: &'static str,
    /// Catervas's words.
    pub words: String,
}

impl MailboxRefusal {
    fn new(code: &'static str, words: impl Into<String>) -> MailboxRefusal {
        MailboxRefusal {
            code,
            words: words.into(),
        }
    }
}

impl std::fmt::Display for MailboxRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.words)
    }
}

impl From<MailboxError> for MailboxRefusal {
    fn from(error: MailboxError) -> MailboxRefusal {
        MailboxRefusal::new(error.code(), error.to_string())
    }
}

/// The refusal for a key store that could not keep or forget the password.
pub(crate) fn store_refusal(error: CredentialError) -> MailboxRefusal {
    let words = match error {
        CredentialError::NoKeychain => {
            "this computer has no keychain to keep the password in".to_string()
        }
        CredentialError::Failed(why) => why,
    };
    MailboxRefusal::new("secret_store_unavailable", words)
}

pub(crate) fn mail_failed(error: impl std::fmt::Display) -> MailboxRefusal {
    MailboxRefusal::new(
        "mailbox_files",
        format!("Catervas could not keep the mailbox: {error}"),
    )
}

/// Connects the procurement mailbox: refuses Microsoft before any connection, checks the fields,
/// logs in to both servers (sending nothing), then keeps the password as a secret in `secrets`,
/// writes `mail/mailbox.json` and `mail/ledger.json` (which starts after the last message there
/// is) and records `mailbox.connected`. Connecting again replaces all of it.
///
/// # Errors
///
/// `mailbox_provider_unsupported` for Microsoft, `mailbox_settings_invalid` naming the field, any
/// of `MailboxError`'s codes from the logins, `mailbox_files` when the files or the record could
/// not be written, and the key store's own words when the password could not be kept.
pub(crate) async fn connect_mailbox(
    deps: &ToolDeps,
    secrets: &(impl MailboxSecrets + ?Sized),
    at: &MailboxAt,
    input: MailboxConnect,
    password: &Secret,
    trust: &Trust,
) -> Result<(), MailboxRefusal> {
    let unsupported = || {
        MailboxRefusal::new(
            "mailbox_provider_unsupported",
            "Microsoft mailboxes are not supported yet.",
        )
    };
    let provider = input.provider.known().ok_or_else(unsupported)?;
    if provider_of(&input.address) == ProviderOf::Microsoft {
        return Err(unsupported());
    }
    let (imap, smtp) = match servers(provider) {
        Some((imap, smtp)) => (imap, smtp),
        None => (input.imap.clone(), input.smtp.clone()),
    };
    let settings = MailboxSettings {
        address: input.address,
        name: input.name,
        provider,
        imap,
        smtp,
        username: input.username,
        folder: input.folder,
        signature: input.signature,
        disclose_ai: input.disclose_ai,
    };
    validate_settings(&settings).map_err(|field| {
        MailboxRefusal::new(
            "mailbox_settings_invalid",
            format!("The {field} of the mailbox does not fit."),
        )
    })?;
    let mut ledger = check_login(&settings, password, trust).await?;
    let _held = crate::locked(&MAIL);
    ledger.checked_at = None;
    let dir = mail_dir(deps)?;
    secrets.save(at, password).map_err(store_refusal)?;
    let write = |name: &str, value: &Value| {
        crate::write_private(&dir.join(name), value.to_string().as_bytes()).map_err(mail_failed)
    };
    write(
        "mailbox.json",
        &serde_json::to_value(&settings).map_err(mail_failed)?,
    )?;
    write(
        "ledger.json",
        &serde_json::to_value(&ledger).map_err(mail_failed)?,
    )?;
    let body = mailbox_connected(&settings.address).map_err(mail_failed)?;
    record_unattended(deps, body, None)
        .map(|_| ())
        .map_err(mail_failed)
}

/// Records `body` as Catervas's own: the envelope names no agent and no session, and `task` when the
/// event is about one. Answers the event's number.
pub(crate) fn record_unattended(
    deps: &ToolDeps,
    body: EventBody,
    task: Option<TaskId>,
) -> Result<u64, ToolError> {
    record_unattended_at(deps, body, task, deps.clock.now())
}

/// [`record_unattended`], stamped `at` and not with the time of the record: a reply is filed in the
/// month `at` names, and must be found there.
pub(crate) fn record_unattended_at(
    deps: &ToolDeps,
    body: EventBody,
    task: Option<TaskId>,
    at: DateTime<Utc>,
) -> Result<u64, ToolError> {
    let ids = EventIds {
        task_id: task,
        ..deps.ids.clone()
    };
    let event = new_event(body, at, ids).map_err(|error| ToolError::Failed {
        detail: format!("the event cannot be stamped: {error:?}"),
    })?;
    let appended = deps.log.append(&event).map_err(|error| ToolError::Failed {
        detail: error.to_string(),
    })?;
    deps.projections
        .apply(&appended)
        .map_err(|error| ToolError::Failed {
            detail: error.to_string(),
        })?;
    Ok(appended.envelope.seq)
}

/// Where `daemon` keeps the procurement mailbox's password in the project of `deps`.
fn mailbox_at_of(
    daemon: &crate::daemon::DaemonState,
    deps: &ToolDeps,
) -> Result<MailboxAt, MailboxRefusal> {
    daemon.mailbox_at(deps.files.root()).map_err(|error| {
        MailboxRefusal::new(
            "secret_store_unavailable",
            format!("this project\u{2019}s id could not be read: {error}"),
        )
    })
}

/// [`connect_mailbox`] for a process that is not the daemon (the command line), keeping the
/// password where `daemon` keeps the connectors' keys and trusting the platform's certificates.
///
/// # Errors
///
/// As [`connect_mailbox`].
pub async fn connect_mailbox_on(
    daemon: &crate::daemon::DaemonState,
    deps: &ToolDeps,
    input: MailboxConnect,
    password: &Secret,
) -> Result<(), MailboxRefusal> {
    let at = mailbox_at_of(daemon, deps)?;
    let secrets = daemon.connector_secrets();
    connect_mailbox(deps, &*secrets, &at, input, password, &daemon.mail_trust()).await
}

/// [`disconnect_mailbox`] for a process that is not the daemon.
///
/// # Errors
///
/// As [`disconnect_mailbox`].
pub fn disconnect_mailbox_on(
    daemon: &crate::daemon::DaemonState,
    deps: &ToolDeps,
) -> Result<(), MailboxRefusal> {
    let at = mailbox_at_of(daemon, deps)?;
    let secrets = daemon.connector_secrets();
    disconnect_mailbox(deps, &*secrets, &at)
}

/// [`check_by_hand`] for a process that is not the daemon: how many replies were recorded.
///
/// # Errors
///
/// As [`check_by_hand`].
pub async fn check_mailbox_on(
    daemon: &crate::daemon::DaemonState,
    deps: &ToolDeps,
) -> Result<u32, MailboxRefusal> {
    let at = mailbox_at_of(daemon, deps)?;
    let secrets = daemon.connector_secrets();
    let mailer = Mailer {
        secrets: &*secrets,
        at,
        trust: daemon.mail_trust(),
    };
    check_by_hand(deps, &mailer).await
}

/// Disconnects the procurement mailbox: forgets its password and deletes `mail/mailbox.json`, keeps
/// `mail/out/` and `mail/in/`, and records `mailbox.disconnected`. Nothing connected is nothing to
/// forget and records nothing.
///
/// # Errors
///
/// The key store's or the files' own words, or the record's.
pub(crate) fn disconnect_mailbox(
    deps: &ToolDeps,
    secrets: &(impl MailboxSecrets + ?Sized),
    at: &MailboxAt,
) -> Result<(), MailboxRefusal> {
    let _held = crate::locked(&MAIL);
    let dir = mail_dir(deps)?;
    let settings = dir.join("mailbox.json");
    let was_connected = settings.exists();
    secrets.delete(at).map_err(store_refusal)?;
    match std::fs::remove_file(&settings) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(mail_failed(error));
        }
        _ => {}
    }
    if was_connected {
        record_unattended(
            deps,
            EventBody::MailboxDisconnected(MailboxDisconnectedBody {
                purpose: MailboxPurpose::Procurement,
            }),
            None,
        )
        .map_err(mail_failed)?;
    }
    Ok(())
}

/// The settings kept in `mail/mailbox.json`, when a mailbox is connected.
pub(crate) fn mailbox_settings(deps: &ToolDeps) -> Option<MailboxSettings> {
    let at = deps
        .files
        .root()
        .join(private_folder(Role::ProcurementSpecialist)?)
        .join(MAIL_FOLDER)
        .join("mailbox.json");
    serde_json::from_str(&std::fs::read_to_string(at).ok()?).ok()
}

/// The ledger kept in `mail/ledger.json`, when there is one.
pub(crate) fn mailbox_ledger(deps: &ToolDeps) -> Option<Ledger> {
    let at = deps
        .files
        .root()
        .join(private_folder(Role::ProcurementSpecialist)?)
        .join(MAIL_FOLDER)
        .join("ledger.json");
    serde_json::from_str(&std::fs::read_to_string(at).ok()?).ok()
}

/// The most messages Catervas sends to sellers in a UTC day.
pub const MOST_SENT_A_DAY: u32 = 50;

/// What `procurement_mailbox.get` answers: whether a mailbox is connected and which, when Catervas
/// last read it and what failed, and how many messages went today of the most Catervas sends.
///
/// # Errors
///
/// What the log refused.
pub fn mailbox_state(deps: &ToolDeps) -> Result<Value, StoreError> {
    let mail = seller_mail(&deps.log)?;
    let mut state = json!({
        "connected": false,
        "sent_today": sent_on(&mail, deps.clock.now().date_naive()),
        "cap": MOST_SENT_A_DAY,
    });
    if let Some(settings) = mailbox_settings(deps) {
        state["connected"] = json!(true);
        state["address"] = json!(settings.address);
        state["name"] = json!(settings.name);
        state["provider"] = json!(settings.provider);
        state["folder"] = json!(settings.folder);
        state["signature"] = json!(settings.signature);
        state["disclose_ai"] = json!(settings.disclose_ai);
        if let Some(ledger) = mailbox_ledger(deps) {
            if let Some(checked) = ledger.checked_at {
                state["checked_at"] = json!(checked);
            }
            if let Some(error) = ledger.error {
                state["error"] = json!(error);
            }
            if let Some(restarted) = ledger.restarted_at {
                state["restarted_at"] = json!(restarted);
            }
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use catervas_protocol::clock::MovableClock;
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_store::purchase_orders::{OrderState, overdue, purchase_orders};
    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;

    use crate::orchestrator::fixtures::Harness;
    use crate::tools::fixtures::waits_for_the_lock;
    use crate::tools::sheets::{CellInput, SheetInput, write_new_workbook};

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a time")
            .with_timezone(&Utc)
    }

    /// An order `number` that `proc` drafted on CTV-1 at `when`.
    fn drafted(harness: &Harness, number: u64, when: DateTime<Utc>) {
        harness.project.record_by(
            Some("proc"),
            when,
            "CTV-1",
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
                .record_at(start + Duration::days(5), "CTV-1", kind, &body);
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
            Some("CTV-1")
        );
        assert_eq!(
            (&ids.agent_id, &ids.session_id),
            (&None, &None),
            "Catervas's own"
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
                .handle(catervas_protocol::command::Command::AgentUpdate {
                    agent_id: id.to_string(),
                    status: catervas_core::team::AgentStatus::Retired,
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
            catervas_core::renewals::RegisterRow {
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
        use catervas_protocol::event::event_from_value;
        // A register can hold more rows than the event's schema counts. The log reads every event
        // back through the schema, so one it refuses would fail every later read of its kind, the
        // next tick's included, and stop the team: the check records the most the schema counts.
        let wire = |body: &super::RenewalCheckedBody| {
            json!({
                "seq": 1, "recorded_at": "2026-10-05T08:00:00Z", "team_id": "catervas",
                "project_id": "catervas", "kind": "renewal.checked",
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
            task_id: Some("CTV-1".parse().expect("a task id")),
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

    // The procurement mailbox (step 10f): connecting, disconnecting and what the page is told.
    fn carry_scratch(test: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("catervas-carry-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    fn old_mail(old: &std::path::Path) -> std::path::PathBuf {
        let mail = old.join(".catervas/local/procurement/mail");
        std::fs::create_dir_all(&mail).expect("the old mail folder");
        mail
    }

    #[cfg(unix)]
    #[test]
    fn carry_mailbox_copies_the_settings_and_the_ledger() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = carry_scratch("copies");
        let (old, new) = (base.join("old"), base.join("new"));
        std::fs::create_dir_all(&new).expect("the new root");
        let mail = old_mail(&old);
        let settings = br#"{"address":"buy@shop.test","name":"Buy"}"#;
        let ledger = br#"{"last_uid":42}"#;
        std::fs::write(mail.join("mailbox.json"), settings).expect("settings");
        std::fs::write(mail.join("ledger.json"), ledger).expect("ledger");

        assert_eq!(
            super::carry_mailbox(&old, &new).expect("carried"),
            Some("buy@shop.test".to_string())
        );
        let there = new.join(".catervas/local/procurement/mail");
        for (name, bytes) in [
            ("mailbox.json", &settings[..]),
            ("ledger.json", &ledger[..]),
        ] {
            assert_eq!(std::fs::read(there.join(name)).expect("copied"), bytes);
            let mode = std::fs::metadata(there.join(name))
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let mode = std::fs::metadata(&there)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);

        let (bare_old, bare_new) = (base.join("bare-old"), base.join("bare-new"));
        std::fs::create_dir_all(&bare_old).expect("a root");
        std::fs::create_dir_all(&bare_new).expect("a root");
        assert_eq!(
            super::carry_mailbox(&bare_old, &bare_new).expect("none"),
            None
        );
        assert!(!bare_new.join(".catervas/local/procurement/mail").exists());
    }

    #[cfg(unix)]
    #[test]
    fn carry_mailbox_refuses_a_linked_mail_folder() {
        let base = carry_scratch("linked");
        let (old, new) = (base.join("old"), base.join("new"));
        let mail = old_mail(&old);
        std::fs::write(mail.join("mailbox.json"), br#"{"address":"buy@shop.test"}"#)
            .expect("settings");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("a target");
        std::fs::create_dir_all(new.join(".catervas/local")).expect("the new root");
        std::os::unix::fs::symlink(&elsewhere, new.join(".catervas/local/procurement"))
            .expect("a link");

        assert!(super::carry_mailbox(&old, &new).is_err());
        assert_eq!(std::fs::read_dir(&elsewhere).expect("reads").count(), 0);
    }

    #[test]
    fn mailbox_connected_is_what_connect_mailbox_records() {
        let EventBody::MailboxConnected(body) =
            super::mailbox_connected("buy@shop.test").expect("a body")
        else {
            panic!("a connection");
        };
        assert_eq!(body.address.as_str(), "buy@shop.test");
        assert_eq!(
            body.purpose,
            catervas_protocol::event::MailboxPurpose::Procurement
        );
        assert!(super::mailbox_connected("not an address").is_err());
    }

    mod mailbox {
        use catervas_protocol::event::{EventBody, EventKind};

        use crate::claude::Secret;
        use crate::connectors::MemoryConnectorSecrets;
        use crate::greenmail::{BUYING, GreenMail};
        use crate::mailbox::{
            MailboxAt, MailboxSecrets as _, Provider, ProviderChoice, Security, Server, Trust,
        };
        use crate::orchestrator::fixtures::Harness;
        use crate::procurement::{
            MailboxConnect, connect_mailbox, disconnect_mailbox, mailbox_state,
        };

        const PROJECT: &str = "0123456789abcdef0123456789abcdef";

        fn at() -> MailboxAt {
            MailboxAt {
                project_id: PROJECT.to_string(),
            }
        }

        fn secret(word: &str) -> Secret {
            Secret::new(word.to_string())
        }

        fn server(port: u16) -> Server {
            Server {
                host: "localhost".to_string(),
                port,
                security: Security::Tls,
            }
        }

        fn input(fixture: &GreenMail) -> MailboxConnect {
            MailboxConnect {
                address: BUYING.address.to_string(),
                name: "Sam Ortiz".to_string(),
                provider: ProviderChoice::Other,
                imap: server(fixture.imaps),
                smtp: server(fixture.smtps),
                username: BUYING.login.to_string(),
                folder: "INBOX".to_string(),
                signature: String::new(),
                disclose_ai: true,
            }
        }

        fn mail_folder(harness: &Harness) -> std::path::PathBuf {
            harness.procurement_folder().join("mail")
        }

        #[tokio::test]
        #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
        async fn connects_after_logging_in_to_both() {
            let fixture = GreenMail::start("connect", &[&BUYING]);
            let harness = Harness::with_procurement("mailbox-connect");
            let deps = &harness.project.deps;
            let store = MemoryConnectorSecrets::default();
            let trust = Trust::Root(fixture.ca_der.clone());
            // A message that was in the mailbox before the connection is never read.
            fixture.deliver(
                BUYING.address,
                "From: a@sellers.test\r\nTo: buying@bakery.test\r\nSubject: old\r\n\r\nold\r\n",
            );
            connect_mailbox(
                deps,
                &store,
                &at(),
                input(&fixture),
                &secret(BUYING.password),
                &trust,
            )
            .await
            .expect("the mailbox connects");

            let events = harness.events(&[EventKind::MailboxConnected]);
            assert_eq!(events.len(), 1);
            let EventBody::MailboxConnected(body) = &events[0].body else {
                panic!("a connection");
            };
            assert_eq!(body.address.as_str(), "buying@bakery.test");
            assert_eq!(
                (
                    &events[0].envelope.ids.agent_id,
                    &events[0].envelope.ids.session_id
                ),
                (&None, &None)
            );
            // The password is kept at mailbox:<id>:procurement, and written nowhere else.
            let kept = store.load(&at()).expect("reads").expect("kept");
            assert_eq!(kept.expose(), BUYING.password);
            assert_eq!(at().account(), format!("mailbox:{PROJECT}:procurement"));
            let settings = std::fs::read_to_string(mail_folder(&harness).join("mailbox.json"))
                .expect("the settings are written");
            assert!(settings.contains("buying@bakery.test"));
            assert!(!settings.contains(BUYING.password), "{settings}");
            let ledger: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(mail_folder(&harness).join("ledger.json"))
                    .expect("the ledger is written"),
            )
            .expect("json");
            assert_eq!(ledger["last_uid"], 1, "after the message already there");
            assert_eq!(fixture.inbox(&BUYING).len(), 1, "nothing was sent");
            let state = mailbox_state(deps).expect("the state");
            assert_eq!(state["connected"], true);
            assert_eq!(state["address"], "buying@bakery.test");
            assert_eq!(state["provider"], "other");
            assert_eq!(state["sent_today"], 0);
            assert_eq!(state["cap"], 50);
            assert!(state.get("password").is_none());
        }

        #[tokio::test]
        #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
        async fn a_wrong_password_keeps_nothing() {
            let fixture = GreenMail::start("wrong", &[&BUYING]);
            let harness = Harness::with_procurement("mailbox-wrong");
            let deps = &harness.project.deps;
            let store = MemoryConnectorSecrets::default();
            let trust = Trust::Root(fixture.ca_der.clone());
            let refused = connect_mailbox(
                deps,
                &store,
                &at(),
                input(&fixture),
                &secret("not-the-word"),
                &trust,
            )
            .await
            .expect_err("the sign-in is refused");
            assert_eq!(refused.code, "mailbox_login_failed");
            assert!(store.load(&at()).expect("reads").is_none());
            assert!(!mail_folder(&harness).join("mailbox.json").exists());
            assert!(!mail_folder(&harness).join("ledger.json").exists());
            assert!(harness.events(&[EventKind::MailboxConnected]).is_empty());
            assert_eq!(mailbox_state(deps).expect("the state")["connected"], false);
        }

        #[tokio::test]
        #[ignore = "needs the git program: cargo xtask check --integration"]
        async fn refuses_microsoft_before_connecting() {
            let harness = Harness::with_procurement("mailbox-microsoft");
            let deps = &harness.project.deps;
            let store = MemoryConnectorSecrets::default();
            // The servers named are a closed port: any connection would be `mailbox_unreachable`.
            let closed = |port: u16| Server {
                host: "127.0.0.1".to_string(),
                port,
                security: Security::Tls,
            };
            let port = crate::ports::free_port();
            let mut microsoft = MailboxConnect {
                address: "ivo@bakery.test".to_string(),
                name: "Ivo".to_string(),
                provider: ProviderChoice::Microsoft,
                imap: closed(port),
                smtp: closed(port),
                username: "ivo".to_string(),
                folder: "INBOX".to_string(),
                signature: String::new(),
                disclose_ai: true,
            };
            for address in ["ivo@bakery.test", "ivo@outlook.com", "ivo@Hotmail.com"] {
                if address != "ivo@bakery.test" {
                    microsoft.provider = ProviderChoice::Other;
                }
                microsoft.address = address.to_string();
                let refused = connect_mailbox(
                    deps,
                    &store,
                    &at(),
                    microsoft.clone(),
                    &secret("anything"),
                    &Trust::Platform,
                )
                .await
                .expect_err("Microsoft is not supported yet");
                assert_eq!(refused.code, "mailbox_provider_unsupported", "{address}");
            }
            assert!(store.load(&at()).expect("reads").is_none());
            assert!(harness.events(&[EventKind::MailboxConnected]).is_empty());
            // Gmail is a known provider with servers of its own; the closed port is not used for it.
            assert_eq!(
                Provider::Gmail,
                match ProviderChoice::Gmail.known() {
                    Some(provider) => provider,
                    None => panic!("Gmail is known"),
                }
            );
        }

        #[tokio::test]
        #[ignore = "needs the git program: cargo xtask check --integration"]
        async fn disconnecting_forgets_the_password() {
            let harness = Harness::with_procurement("mailbox-disconnect");
            let deps = &harness.project.deps;
            let store = MemoryConnectorSecrets::default();
            store
                .save(&at(), &secret("open-sesame"))
                .expect("the password is kept");
            let mail = mail_folder(&harness);
            for file in [
                "mailbox.json",
                "ledger.json",
                "out/1.txt",
                "in/2026-10/1/text.txt",
            ] {
                let path = mail.join(file);
                std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
                std::fs::write(path, "kept").expect("written");
            }
            disconnect_mailbox(deps, &store, &at()).expect("disconnects");
            assert!(store.load(&at()).expect("reads").is_none());
            assert!(!mail.join("mailbox.json").exists());
            assert!(mail.join("out/1.txt").exists(), "messages stay");
            assert!(mail.join("in/2026-10/1/text.txt").exists(), "replies stay");
            let events = harness.events(&[EventKind::MailboxDisconnected]);
            assert_eq!(events.len(), 1);
            assert_eq!(mailbox_state(deps).expect("the state")["connected"], false);
            // Disconnecting again forgets nothing and records nothing more.
            disconnect_mailbox(deps, &store, &at()).expect("disconnects again");
            assert_eq!(harness.events(&[EventKind::MailboxDisconnected]).len(), 1);
        }

        #[tokio::test]
        #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
        async fn the_password_is_never_shown() {
            let fixture = GreenMail::start("never-shown", &[&BUYING]);
            let harness = Harness::with_procurement("mailbox-never-shown");
            let deps = &harness.project.deps;
            let store = MemoryConnectorSecrets::default();
            let word = "lemon-curd-7731";
            let trust = Trust::Root(fixture.ca_der.clone());
            let mut said = Vec::new();
            // A wrong password, a server that is not encrypted and a port nothing listens on.
            let mut plain = input(&fixture);
            plain.imap = Server {
                host: "localhost".to_string(),
                port: fixture.imap,
                security: Security::StartTls,
            };
            let mut closed = input(&fixture);
            closed.imap.port = crate::ports::free_port();
            for attempt in [input(&fixture), plain, closed] {
                let refused = connect_mailbox(deps, &store, &at(), attempt, &secret(word), &trust)
                    .await
                    .expect_err("refused");
                said.push(format!("{refused} {refused:?}"));
            }
            said.push(format!("{:?}", secret(word)));
            let events: Vec<String> = harness
                .events(&[])
                .iter()
                .map(|event| format!("{event:?}"))
                .collect();
            for text in said.iter().chain(events.iter()) {
                assert!(!text.contains(word), "the password is shown: {text}");
            }
            assert_eq!(said[3], "[redacted]");
            assert!(store.load(&at()).expect("reads").is_none());
        }
    }
}
