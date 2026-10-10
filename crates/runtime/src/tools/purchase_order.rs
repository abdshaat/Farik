//! `catervas_draft_purchase_order` (`docs/SPEC.md` 6.10, ADR 0039): the Procurement Specialist
//! suggests a purchase order and never places, pays for, confirms or cancels one. The order is a
//! record and a document, not a gate: Catervas writes its workbook, `orders/PO-<n>.xlsx` in the
//! role's private folder, and records `purchase_order.drafted`; the task goes on, and the order
//! waits for the owner alone. The agent cannot write that folder with `catervas_write_sheet`.

use std::fs;
use std::path::Path;

use catervas_core::contract::Role;
use catervas_core::governor::sites::site_of;
use catervas_core::marketing::{Amount, parse_amount};
use catervas_core::order::{OrderError, OrderLine, line_total, order_total};
use catervas_core::team::private_folder;
use catervas_protocol::event::{EventBody, PurchaseOrderDraftedBody, PurchaseOrderUpdatedBody};
use catervas_store::purchase_orders::{OrderState, PurchaseOrderRecord, purchase_orders};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::evaluation::is_a_name;
use super::refusal::Refusal;
use super::sheets::{
    CellInput, SheetInput, in_its_own_implement_session, private_path, write_new_workbook,
};
use super::sites::{approved_set, shown};
use super::{Call, ToolError, failed};
use crate::procurement::{ORDERS, check_follow_up, order_row};

/// The most lines an order has.
const MOST_LINES: usize = 50;
/// The most one line is bought in.
const MOST_QUANTITY: u32 = 1_000_000;
/// The most orders that wait for the owner in a project at once.
const MOST_WAITING: usize = 20;
/// The most characters of an address.
const MOST_URL: usize = 2_000;
/// The folder, in the role's private folder, that holds each order's workbook.
const ORDERS_FOLDER: &str = "orders";

/// `catervas_draft_purchase_order`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftPurchaseOrderInput {
    /// The seller, 1 to 100 characters on one line. At most one order for a seller is open on a
    /// task: wait for the owner to decide it.
    seller: String,
    /// How the owner reaches the seller: an address, a phone number or a name, 0 to 200
    /// characters on one line. Shown as text.
    #[serde(default)]
    seller_contact: String,
    /// The lines, 1 to 50.
    lines: Vec<OrderLineInput>,
    /// The currency of every price: three capital letters, such as USD.
    currency: String,
    /// How often the lines are paid: `once`, `month` or `year`. A subscription's prices are per
    /// period.
    period: OrderPeriodInput,
    /// Delivery, as the seller said it, 0 to 600 characters.
    #[serde(default)]
    delivery: String,
    /// Payment terms and conditions, as the seller said them, 0 to 600 characters.
    #[serde(default)]
    terms: String,
    /// The seller's page for these goods, an address that starts with https:// on a site the
    /// owner allowed (`catervas_read_sites` lists them; `catervas_request_sites` asks for another),
    /// at most 2000 characters. Leave it empty for a seller with no page, met by phone or in
    /// person.
    #[serde(default)]
    url: String,
    /// The comparison this order rests on: `evaluations/<name>.md`, written with
    /// `catervas_write_evaluation`.
    evaluation: String,
    /// Why this seller and these goods, 20 to 600 characters, in your own words. The owner reads
    /// it beside the order.
    why: String,
}

/// One line of an order.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrderLineInput {
    /// What is bought, 1 to 200 characters on one line.
    item: String,
    /// How many, a whole number from 1 to 1,000,000.
    quantity: u32,
    /// The price of one, as a decimal number with no sign, no separator, at most eight digits
    /// before the point and two after: `19.99`.
    unit_price: String,
    /// What one is counted in (`kg`, `box`), 0 to 20 characters on one line; empty for none.
    #[serde(default)]
    unit: String,
}

/// How often an order's lines are paid.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrderPeriodInput {
    /// Once.
    Once,
    /// Every month.
    Month,
    /// Every year.
    Year,
}

impl OrderPeriodInput {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::PurchaseOrder {
        code,
        detail: detail.into(),
    }
    .into()
}

/// Whether `text` is on one line: it holds no line break and no other control character.
fn is_one_line(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

/// Whether `text` holds no control character but a line break or a tab, as a paragraph of the
/// seller's words may.
fn is_text(text: &str) -> bool {
    !text
        .chars()
        .any(|one| one.is_control() && !matches!(one, '\n' | '\r' | '\t'))
}

/// The seller and how to reach it, held to their rules.
fn check_seller(input: &DraftPurchaseOrderInput) -> Result<(), ToolError> {
    let length = input.seller.chars().count();
    if !(1..=100).contains(&length) || !is_one_line(&input.seller) {
        return Err(refused(
            "purchase_order_seller_invalid",
            "seller is 1 to 100 characters on one line",
        ));
    }
    if input.seller_contact.chars().count() > 200 || !is_one_line(&input.seller_contact) {
        return Err(refused(
            "purchase_order_seller_invalid",
            "seller_contact is 0 to 200 characters on one line",
        ));
    }
    Ok(())
}

/// The lines, each held to its rules and read as amounts.
fn check_lines(input: &DraftPurchaseOrderInput) -> Result<Vec<OrderLine>, ToolError> {
    if !(1..=MOST_LINES).contains(&input.lines.len()) {
        return Err(refused(
            "purchase_order_lines_invalid",
            format!(
                "lines is 1 to {MOST_LINES} lines, and this order has {}",
                input.lines.len()
            ),
        ));
    }
    let mut lines = Vec::new();
    for (index, line) in input.lines.iter().enumerate() {
        let at = index + 1;
        let bad = |field: &str, rule: &str| {
            refused(
                "purchase_order_line_invalid",
                format!("line {at} {field}: {rule}"),
            )
        };
        if !(1..=200).contains(&line.item.chars().count()) || !is_one_line(&line.item) {
            return Err(bad("item", "1 to 200 characters on one line"));
        }
        if !(1..=MOST_QUANTITY).contains(&line.quantity) {
            return Err(bad("quantity", "a whole number from 1 to 1000000"));
        }
        let Some(unit_price) = parse_amount(&line.unit_price) else {
            return Err(bad(
                "unit_price",
                &format!(
                    "{} is not a price: write it like 19.99, with no sign or separator, at most \
                     eight digits before the point and two after",
                    shown(&line.unit_price)
                ),
            ));
        };
        if line.unit.chars().count() > 20 || !is_one_line(&line.unit) {
            return Err(bad("unit", "0 to 20 characters on one line"));
        }
        lines.push(OrderLine {
            item: line.item.clone(),
            quantity: line.quantity,
            unit_price,
            unit: line.unit.clone(),
        });
    }
    Ok(lines)
}

/// The currency, and the delivery and terms the seller gave.
fn check_currency_and_terms(input: &DraftPurchaseOrderInput) -> Result<(), ToolError> {
    let currency = input.currency.as_bytes();
    if currency.len() != 3 || !currency.iter().all(u8::is_ascii_uppercase) {
        return Err(refused(
            "purchase_order_currency_invalid",
            "currency is three capital letters, like USD",
        ));
    }
    for (field, text, code) in [
        (
            "delivery",
            &input.delivery,
            "purchase_order_delivery_invalid",
        ),
        ("terms", &input.terms, "purchase_order_terms_invalid"),
    ] {
        if text.chars().count() > 600 || !is_text(text) {
            return Err(refused(code, format!("{field} is 0 to 600 characters")));
        }
    }
    Ok(())
}

/// The seller's page: empty, or an address on a site the owner allowed. Answers its site.
fn check_page(call: &Call<'_>, url: &str) -> Result<(), ToolError> {
    if url.is_empty() {
        return Ok(());
    }
    if url.chars().count() > MOST_URL {
        return Err(refused(
            "purchase_order_url_invalid",
            format!(
                "{} the address is longer than {MOST_URL} characters",
                shown(url)
            ),
        ));
    }
    let host = site_of(url).map_err(|fault| {
        refused(
            "purchase_order_url_invalid",
            format!("{} {fault}", shown(url)),
        )
    })?;
    if !approved_set(&call.deps().log)
        .map_err(failed)?
        .contains(&host)
    {
        return Err(refused(
            "purchase_order_site_not_approved",
            format!(
                "{host} is not a site the owner allowed; ask for it with catervas_request_sites \
                 first, then end your turn"
            ),
        ));
    }
    Ok(())
}

/// The comparison the order rests on: `evaluations/<name>.md`, reached without a link, there.
fn check_evaluation(call: &Call<'_>, folder: &str, evaluation: &str) -> Result<(), ToolError> {
    let named = evaluation
        .strip_prefix("evaluations/")
        .and_then(|rest| rest.strip_suffix(".md"))
        .is_some_and(is_a_name);
    if !named {
        return Err(refused(
            "purchase_order_evaluation_invalid",
            format!(
                "{} is not a comparison: write evaluations/<name>.md, the name of a note made \
                 with catervas_write_evaluation",
                shown(evaluation)
            ),
        ));
    }
    let root = call.deps().files.root();
    let at = private_path(root, folder, evaluation)?;
    if !at.is_file() {
        return Err(refused(
            "evaluation_missing",
            format!(
                "there is no comparison at {evaluation}; write it with catervas_write_evaluation \
                 first"
            ),
        ));
    }
    Ok(())
}

/// The number after the highest the log and the orders folder hold.
fn next_number(records: &[PurchaseOrderRecord], orders: &Path) -> u64 {
    let logged = records.iter().map(|record| record.order).max().unwrap_or(0);
    let filed = fs::read_dir(orders)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| numbered(&entry.file_name().to_string_lossy()))
        .max()
        .unwrap_or(0);
    logged.max(filed) + 1
}

/// The `n` of a file named `PO-<n>.xlsx`.
fn numbered(name: &str) -> Option<u64> {
    let digits = name.strip_prefix("PO-")?.strip_suffix(".xlsx")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// An amount as a number cell: hundredths over a hundred, which no amount an order may hold
/// exceeds in a `u32`, so a cell and its text agree.
fn number_of(amount: Amount) -> f64 {
    f64::from(u32::try_from(amount.0).unwrap_or(u32::MAX)) / 100.0
}

/// The order's workbook: one sheet, `Order`, with a header block, the lines as a table and the
/// total. Quantities, prices and totals are numbers and text is text, never formulas.
fn workbook(
    number: u64,
    day: &str,
    buyer: &str,
    input: &DraftPurchaseOrderInput,
    lines: &[OrderLine],
    total: Amount,
) -> Vec<SheetInput> {
    let text = |text: &str| {
        if text.is_empty() {
            CellInput::empty()
        } else {
            CellInput::text(text)
        }
    };
    let pair = |label: &str, value: CellInput| vec![CellInput::text(label), value];
    let mut rows = vec![
        pair("Order number", CellInput::text(&format!("PO-{number}"))),
        pair("Date", CellInput::date(day)),
        pair("Buyer", text(buyer)),
        pair("Seller", text(&input.seller)),
        pair("Seller contact", text(&input.seller_contact)),
        pair("Currency", text(&input.currency)),
        pair("Period", text(input.period.as_str())),
        pair("Delivery", text(&input.delivery)),
        pair("Terms", text(&input.terms)),
        Vec::new(),
        ["Item", "Quantity", "Unit", "Unit price", "Line total"]
            .into_iter()
            .map(CellInput::text)
            .collect(),
    ];
    for line in lines {
        rows.push(vec![
            text(&line.item),
            CellInput::number(f64::from(line.quantity)),
            text(&line.unit),
            CellInput::number(number_of(line.unit_price)),
            CellInput::number(number_of(line_total(line).unwrap_or(Amount(0)))),
        ]);
    }
    rows.push(vec![
        CellInput::text("Total"),
        CellInput::empty(),
        CellInput::empty(),
        CellInput::empty(),
        CellInput::number(number_of(total)),
    ]);
    vec![SheetInput::new("Order", rows)]
}

/// `catervas_draft_purchase_order`: checks the order, numbers it, writes `orders/PO-<n>.xlsx` as a
/// new file and records `purchase_order.drafted`, in the Procurement Specialist's implement
/// session of a task it is the assignee of. The order waits for the owner; the task goes on. A
/// call is refused whole, with nothing written or recorded, for the first fault in this order: the
/// session, the seller, the lines, the total, the currency, the delivery and terms, the page and
/// its site, the comparison, the reason, an order for the seller still open on the task, and 20
/// orders waiting already.
///
/// # Errors
///
/// `purchase_order_refused` outside that session, then one code for each field:
/// `purchase_order_seller_invalid`, `purchase_order_lines_invalid`,
/// `purchase_order_line_invalid`, `purchase_order_too_large`, `purchase_order_currency_invalid`,
/// `purchase_order_delivery_invalid`, `purchase_order_terms_invalid`, `purchase_order_url_invalid`,
/// `purchase_order_site_not_approved`, `purchase_order_evaluation_invalid`,
/// `evaluation_missing`, `purchase_order_why_invalid`, `purchase_order_open` and
/// `too_many_purchase_orders`; `purchase_order_file_exists` when a workbook of the number is
/// there; `Failed` when the log or a file cannot be written.
pub(super) fn draft_purchase_order(
    call: &Call<'_>,
    input: &DraftPurchaseOrderInput,
) -> Result<Value, ToolError> {
    let refuse = |why: &str| refused("purchase_order_refused", format!("only {why}"));
    let folder = private_folder(call.role())
        .filter(|_| call.role() == Role::ProcurementSpecialist)
        .ok_or_else(|| refuse("the Procurement Specialist drafts a purchase order"))?;
    in_its_own_implement_session(call, "a purchase order", &refuse)?;
    let task = call.task()?.clone();
    check_seller(input)?;
    let lines = check_lines(input)?;
    let total = order_total(&lines).map_err(|error| match error {
        OrderError::TooLarge { .. } => refused("purchase_order_too_large", error.to_string()),
    })?;
    check_currency_and_terms(input)?;
    check_page(call, &input.url)?;
    check_evaluation(call, folder, &input.evaluation)?;
    if !(20..=600).contains(&input.why.chars().count()) || !is_text(&input.why) {
        return Err(refused(
            "purchase_order_why_invalid",
            "why is 20 to 600 characters",
        ));
    }

    // The number, the checks that read the other orders, the file and the record are one step.
    let _held = crate::locked(&ORDERS);
    let deps = call.deps();
    let records = purchase_orders(&deps.log).map_err(failed)?;
    if let Some(open) = records.iter().find(|record| {
        record.task_id == task
            && record
                .drafted
                .seller
                .to_lowercase()
                .eq(&input.seller.to_lowercase())
            && matches!(
                record.state,
                OrderState::Drafted | OrderState::Approved | OrderState::Placed
            )
    }) {
        return Err(refused(
            "purchase_order_open",
            format!(
                "PO-{} from {} is open on this task: it waits for the owner, who approves or \
                 rejects it, and then places and receives it; go on with the task meanwhile",
                open.order,
                shown(&input.seller)
            ),
        ));
    }
    let waiting = records
        .iter()
        .filter(|record| record.state == OrderState::Drafted)
        .count();
    if waiting >= MOST_WAITING {
        return Err(refused(
            "too_many_purchase_orders",
            format!(
                "{MOST_WAITING} orders wait for the owner already; read them with \
                 catervas_read_purchase_orders, and go on without another"
            ),
        ));
    }
    let root = deps.files.root();
    let folder_at = root.join(folder);
    let number = next_number(&records, &folder_at.join(ORDERS_FOLDER));
    let file = format!("{ORDERS_FOLDER}/PO-{number}.xlsx");
    let path = private_path(root, folder, &file)?;
    let day = deps.clock.now().date_naive().to_string();
    let sheets = workbook(
        number,
        &day,
        &call.team.name.to_string(),
        input,
        &lines,
        total,
    );
    // The record is built before the file is written, so that no body that cannot be built leaves
    // a workbook behind. Only a log that refused the event takes the workbook away: once the log
    // has the order, the order is recorded, whatever the projections do, and the owner downloads it.
    let body = drafted_body(number, input, &lines, total)?;
    write_new_workbook(&folder_at, &path, &sheets)?;
    let recorded = kept_if_recorded(
        &path,
        call.record(Some(&task), EventBody::PurchaseOrderDrafted(body)),
    )?;
    call.project(&recorded)?;
    Ok(json!({
        "order": number,
        "file": file,
        "total": total.to_string(),
        "currency": input.currency,
        "next": "the owner approves or rejects it; your task goes on, and you never place, pay \
                 for, confirm or cancel an order",
    }))
}

/// Passes `recorded` on, and removes the order's workbook at `path` when the order was not
/// recorded, so that no number is left taken by a file with no order behind it.
fn kept_if_recorded<Recorded>(
    path: &Path,
    recorded: Result<Recorded, ToolError>,
) -> Result<Recorded, ToolError> {
    if recorded.is_err() {
        let _ = fs::remove_file(path);
    }
    recorded
}

/// The event's body for an order the checks passed, from the input's own words.
fn drafted_body(
    number: u64,
    input: &DraftPurchaseOrderInput,
    lines: &[OrderLine],
    total: Amount,
) -> Result<PurchaseOrderDraftedBody, ToolError> {
    let wire_lines: Vec<Value> = lines
        .iter()
        .map(|line| {
            json!({
                "item": line.item,
                "quantity": line.quantity,
                "unit": line.unit,
                "unit_price": line.unit_price.to_string(),
                "line_total": line_total(line).unwrap_or(Amount(0)).to_string(),
            })
        })
        .collect();
    serde_json::from_value(json!({
        "order": number,
        "seller": input.seller,
        "seller_contact": input.seller_contact,
        "lines": wire_lines,
        "currency": input.currency,
        "period": input.period.as_str(),
        "total": total.to_string(),
        "delivery": input.delivery,
        "terms": input.terms,
        "url": input.url,
        "evaluation": input.evaluation,
        "why": input.why,
    }))
    .map_err(failed)
}

/// `catervas_update_purchase_order`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdatePurchaseOrderInput {
    /// The order's number, the n of PO-n, of an order you drafted that the owner placed.
    order: u64,
    /// What your follow-up learned: `preparing` (the seller is making or packing it), `shipped`,
    /// `delayed` (with what you know in `note` and the day in `expected_on`) or `problem` (out of
    /// stock, a payment refused, cancelled by the seller: say what in `note`). Nothing else: only
    /// the owner marks an order placed or received.
    status: String,
    /// What you know, 0 to 300 characters on one line, from the seller's page; required for
    /// `delayed` and `problem`. Say only what the page says.
    #[serde(default)]
    note: String,
    /// The day the seller expects the order to arrive, `YYYY-MM-DD`, today or later; required for
    /// `delayed`.
    #[serde(default)]
    expected_on: Option<String>,
}

/// `catervas_read_purchase_orders`: every order of the project, oldest first, with its state, lines
/// and total, the owner's notes as they wrote them, when it was placed, what was paid and in
/// which currency, when it was received and when it renews, its latest follow-up status with who
/// recorded it, and whether it is overdue. Records nothing.
///
/// # Errors
///
/// `purchase_order_refused` for a role other than the Procurement Specialist; `Failed` when the
/// log cannot be read.
pub(super) fn read_purchase_orders(call: &Call<'_>) -> Result<Value, ToolError> {
    if call.role() != Role::ProcurementSpecialist {
        return Err(refused(
            "purchase_order_refused",
            "only the Procurement Specialist reads the purchase orders",
        ));
    }
    let deps = call.deps();
    let today = deps.clock.now().date_naive();
    let records = purchase_orders(&deps.log).map_err(failed)?;
    Ok(json!({
        "orders": records
            .iter()
            .map(|record| order_row(record, today, true))
            .collect::<Vec<_>>(),
    }))
}

/// `catervas_update_purchase_order`: records what a follow-up learned about an order the owner
/// placed, as `purchase_order.updated` with the agent's envelope and the order's task, so that it
/// never counts as the owner's. Only the agent that drafted the order records one, in the
/// implement session of a task it is the assignee of, which is its follow-up task; it sends
/// nothing and cannot mark an order placed, received or paid.
///
/// # Errors
///
/// `purchase_order_update_refused` outside that session or for another role,
/// `unknown_purchase_order`, `purchase_order_not_yours` for an order another agent drafted,
/// `purchase_order_not_placed` for an order the owner has not placed, or has received or closed,
/// `purchase_order_status_invalid` for a status or note or day its rules refuse; `Failed` when the
/// log cannot be read or written.
pub(super) fn update_purchase_order(
    call: &Call<'_>,
    input: &UpdatePurchaseOrderInput,
) -> Result<Value, ToolError> {
    let refuse = |why: &str| refused("purchase_order_update_refused", format!("only {why}"));
    if call.role() != Role::ProcurementSpecialist {
        return Err(refuse("the Procurement Specialist records a follow-up"));
    }
    in_its_own_implement_session(call, "a follow-up", &refuse)?;
    let _held = crate::locked(&ORDERS);
    let deps = call.deps();
    let records = purchase_orders(&deps.log).map_err(failed)?;
    let Some(record) = records.iter().find(|record| record.order == input.order) else {
        return Err(refused(
            "unknown_purchase_order",
            format!("there is no order PO-{}", input.order),
        ));
    };
    if record.agent_id != call.agent_id() {
        return Err(refused(
            "purchase_order_not_yours",
            format!("PO-{} was drafted by another agent", record.order),
        ));
    }
    if record.state != OrderState::Placed {
        return Err(refused(
            "purchase_order_not_placed",
            format!(
                "PO-{} is {}, and a follow-up is of an order the owner placed and has not yet \
                 received or closed",
                record.order,
                record.state.as_str()
            ),
        ));
    }
    let today = deps.clock.now().date_naive();
    let fields = check_follow_up(
        &input.status,
        &input.note,
        input.expected_on.as_deref(),
        today,
    )
    .map_err(|why| refused("purchase_order_status_invalid", why))?;
    let mut body = json!({
        "order": record.order,
        "status": fields.status.to_string(),
        "note": fields.note,
    });
    if let Some(day) = fields.expected_on {
        body["expected_on"] = json!(day.to_string());
    }
    let body: PurchaseOrderUpdatedBody = serde_json::from_value(body).map_err(failed)?;
    call.append(Some(&record.task_id), EventBody::PurchaseOrderUpdated(body))?;
    Ok(json!({
        "order": record.order,
        "status": input.status,
        "next": "the owner sees it on your page and may correct it; you never mark an order \
                 placed or received",
    }))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use calamine::{Data, DataType as _, Range, Reader as _, Xlsx, open_workbook};
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_roles::sites::catervas_sites;
    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, run, waits_for_the_lock, with_the_finance_specialist,
        with_the_procurement_specialist,
    };

    /// A project with the Finance Specialist `fin` and the Procurement Specialist `proc`, whose
    /// tasks CTV-1 and CTV-4 are in progress, a finance task CTV-2 and a Developer's task CTV-3,
    /// and the comparison `evaluations/mirrors.md` written.
    fn a_project(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_procurement_specialist(wire);
            }),
        );
        for (task, role, assignee) in [
            ("CTV-1", "procurement_specialist", "proc"),
            ("CTV-2", "finance_specialist", "fin"),
            ("CTV-4", "procurement_specialist", "proc"),
        ] {
            project.filed_with(task, "assigned", "task", None, |wire| {
                wire["assignee_role"] = json!(role);
                wire["reviewer_role"] = json!("product_manager");
            });
            project.moved(
                task,
                "assigned",
                "in_progress",
                &json!({ "assignee": assignee, "reviewer": "pm" }),
            );
        }
        project.filed("CTV-3", "assigned", "task", None);
        project.moved(
            "CTV-3",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        project
            .call(
                "proc",
                Some("CTV-1"),
                "catervas_write_evaluation",
                json!({ "name": "mirrors", "text": "# Baby car mirrors\n\nAcme is the cheapest." }),
            )
            .expect("the comparison is written");
        project
    }

    fn folder(project: &TestProject) -> PathBuf {
        project.repo.path.join(".catervas/local/procurement")
    }

    /// One of Catervas's own hosts, taken from the shipped list so that the launch review edits the
    /// YAML alone.
    fn a_catervas_host() -> &'static str {
        &catervas_sites()[0].host
    }

    /// An order for three mirrors and a kit, 59.98 USD, from `seller`, with its page on a site
    /// Catervas ships.
    fn an_order(seller: &str) -> Value {
        json!({
            "seller": seller,
            "seller_contact": "sales@acme.example",
            "lines": [
                { "item": "Baby car mirror", "quantity": 3, "unit_price": "19.99", "unit": "piece" },
                { "item": "Mounting kit", "quantity": 1, "unit_price": "0.01", "unit": "" }
            ],
            "currency": "USD",
            "period": "once",
            "delivery": "Ships in 3 days",
            "terms": "Net 30",
            "url": format!("https://www.{}/mirrors", a_catervas_host()),
            "evaluation": "evaluations/mirrors.md",
            "why": "It is the cheapest seller that ships to us with a safety mark."
        })
    }

    /// `catervas_draft_purchase_order` as `proc` in its implement session of CTV-1.
    fn draft(project: &TestProject, input: &Value) -> Result<Value, ToolError> {
        project.call(
            "proc",
            Some("CTV-1"),
            "catervas_draft_purchase_order",
            input.clone(),
        )
    }

    fn refusal_of(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The first two cells of every row of the workbook's `Order` sheet.
    fn rows_of(path: &PathBuf) -> (Xlsx<std::io::BufReader<fs::File>>, Range<Data>) {
        let mut book: Xlsx<_> = open_workbook(path).expect("the workbook opens");
        let range = book.worksheet_range("Order").expect("an Order sheet");
        (book, range)
    }

    /// The cell after `label` in the first column.
    fn after<'a>(range: &'a Range<Data>, label: &str) -> &'a Data {
        let row = range
            .rows()
            .find(|row| row.first().and_then(|cell| cell.get_string()) == Some(label))
            .unwrap_or_else(|| panic!("a row labelled {label}"));
        &row[1]
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn drafts_an_order_and_its_workbook() {
        let project = a_project("po-draft");
        let team = project.deps.files.read_team().expect("the team");

        let first = draft(&project, &an_order("Acme")).expect("an order is drafted");
        assert_eq!(first["order"], 1);
        assert_eq!(first["file"], "orders/PO-1.xlsx");
        assert_eq!(first["total"], "59.98");
        let second = draft(&project, &an_order("Bolt")).expect("a second order is drafted");
        assert_eq!(second["order"], 2);

        let drafted = project.events(&[EventKind::PurchaseOrderDrafted]);
        assert_eq!(drafted.len(), 2);
        let ids = &drafted[0].envelope.ids;
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.as_str()),
            Some("CTV-1")
        );
        assert_eq!(ids.agent_id.as_deref(), Some("proc"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
        let EventBody::PurchaseOrderDrafted(body) = &drafted[0].body else {
            panic!("a purchase_order.drafted event carries its body");
        };
        assert_eq!(body.order.get(), 1);
        assert_eq!(body.seller.to_string(), "Acme");
        assert_eq!(body.seller_contact.to_string(), "sales@acme.example");
        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].item.to_string(), "Baby car mirror");
        assert_eq!(body.lines[0].quantity.get(), 3);
        assert_eq!(body.lines[0].unit.to_string(), "piece");
        assert_eq!(body.lines[0].unit_price.as_str(), "19.99");
        assert_eq!(body.lines[0].line_total.as_str(), "59.97");
        assert_eq!(body.lines[1].line_total.as_str(), "0.01");
        assert_eq!(body.total.as_str(), "59.98");
        assert_eq!(body.currency.as_str(), "USD");
        assert_eq!(body.period.to_string(), "once");
        assert_eq!(body.delivery.to_string(), "Ships in 3 days");
        assert_eq!(body.terms.to_string(), "Net 30");
        assert_eq!(
            body.url.to_string(),
            format!("https://www.{}/mirrors", a_catervas_host()),
            "the address is kept exactly as the agent wrote it"
        );
        assert_eq!(body.evaluation.to_string(), "evaluations/mirrors.md");
        assert!(body.why.to_string().starts_with("It is the cheapest"));

        let (mut book, range) = rows_of(&folder(&project).join("orders/PO-1.xlsx"));
        assert_eq!(book.sheet_names(), ["Order"]);
        assert_eq!(after(&range, "Order number").get_string(), Some("PO-1"));
        assert_eq!(
            after(&range, "Buyer").get_string(),
            Some(team.name.to_string().as_str())
        );
        assert_eq!(after(&range, "Seller").get_string(), Some("Acme"));
        assert_eq!(
            after(&range, "Seller contact").get_string(),
            Some("sales@acme.example")
        );
        assert_eq!(after(&range, "Currency").get_string(), Some("USD"));
        assert_eq!(after(&range, "Period").get_string(), Some("once"));
        assert_eq!(
            after(&range, "Delivery").get_string(),
            Some("Ships in 3 days")
        );
        assert_eq!(after(&range, "Terms").get_string(), Some("Net 30"));
        let Data::DateTime(day) = after(&range, "Date") else {
            panic!("the date is a date cell");
        };
        let (year, month, of_month, ..) = day.to_ymd_hms_milli();
        assert_eq!((year, month, of_month), (2026, 9, 22));
        // The lines are a table below the header block: quantities, prices and totals are numbers.
        let table: Vec<&[Data]> = range
            .rows()
            .skip_while(|row| row.first().and_then(|cell| cell.get_string()) != Some("Item"))
            .collect();
        let words = |row: &[Data]| -> Vec<String> { row.iter().map(ToString::to_string).collect() };
        assert_eq!(
            words(table[0]),
            ["Item", "Quantity", "Unit", "Unit price", "Line total"]
        );
        let number = |cell: &Data| match cell {
            Data::Float(number) => *number,
            #[allow(clippy::cast_precision_loss, reason = "a small whole number")]
            Data::Int(number) => *number as f64,
            other => panic!("a number cell, not {other:?}"),
        };
        assert_eq!(table[1][0].get_string(), Some("Baby car mirror"));
        assert!((number(&table[1][1]) - 3.0).abs() < 1e-9);
        assert_eq!(table[1][2].get_string(), Some("piece"));
        assert!((number(&table[1][3]) - 19.99).abs() < 1e-9);
        assert!((number(&table[1][4]) - 59.97).abs() < 1e-9);
        assert_eq!(table[2][0].get_string(), Some("Mounting kit"));
        assert!((number(&table[2][4]) - 0.01).abs() < 1e-9);
        assert_eq!(table[3][0].get_string(), Some("Total"));
        assert!((number(&table[3][4]) - 59.98).abs() < 1e-9);
        // Nothing is a formula: the seller and the owner read the same numbers in any program.
        let formulas = book.worksheet_formula("Order").expect("formulas");
        assert!(formulas.rows().all(|row| row.iter().all(String::is_empty)));
        assert!(folder(&project).join("orders/PO-2.xlsx").is_file());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn numbers_past_a_workbook_already_there() {
        let project = a_project("po-number");
        let orders = folder(&project).join("orders");
        fs::create_dir_all(&orders).expect("the folder is made");
        fs::write(orders.join("PO-3.xlsx"), b"theirs").expect("a workbook is there");
        // Names that are no order's number count for nothing.
        fs::write(orders.join("PO-x.xlsx"), b"x").expect("a file");
        fs::write(orders.join("notes.txt"), b"x").expect("a file");

        let answer = draft(&project, &an_order("Acme")).expect("an order is drafted");

        assert_eq!(answer["order"], 4, "one more than the workbook there");
        assert!(orders.join("PO-4.xlsx").is_file());
        assert_eq!(
            fs::read(orders.join("PO-3.xlsx")).expect("it is there"),
            b"theirs",
            "a workbook that is there is never replaced"
        );

        // The log counts too: an order drafted and its workbook gone is still a number taken.
        project.record_by(
            Some("proc"),
            crate::tools::fixtures::at(),
            "CTV-1",
            "purchase_order.drafted",
            &json!({
                "order": 9, "seller": "Old", "seller_contact": "",
                "lines": [{ "item": "x", "quantity": 1, "unit": "", "unit_price": "1.00", "line_total": "1.00" }],
                "currency": "USD", "period": "once", "total": "1.00", "delivery": "", "terms": "",
                "url": "", "evaluation": "evaluations/mirrors.md",
                "why": "An order whose workbook was deleted."
            }),
        );
        let next = draft(&project, &an_order("Bolt")).expect("an order is drafted");
        assert_eq!(next["order"], 10);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn removes_the_workbook_when_the_order_is_not_recorded() {
        let project = a_project("po-rollback");
        let path = project.repo.path.join("PO-1.xlsx");
        fs::write(&path, b"workbook").expect("a workbook");

        let kept = super::kept_if_recorded(&path, Ok(7));
        assert_eq!(kept, Ok(7));
        assert!(path.is_file(), "a recorded order keeps its workbook");

        let lost = super::kept_if_recorded::<u8>(
            &path,
            Err(ToolError::Failed {
                detail: "the log is full".to_string(),
            }),
        );
        assert!(matches!(lost, Err(ToolError::Failed { .. })));
        assert!(!path.exists(), "an order not recorded leaves no workbook");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaves_no_workbook_when_the_log_refuses_the_order() {
        let project = a_project("po-log-refuses");
        catervas_store::event_log::fixtures::refuse_appends_of(
            &project.deps.log,
            EventKind::PurchaseOrderDrafted,
        );
        let before = project.event_count();

        let result = draft(&project, &an_order("Acme"));

        assert!(
            matches!(result, Err(ToolError::Failed { .. })),
            "{result:?}"
        );
        assert_eq!(project.event_count(), before, "nothing was recorded");
        assert!(
            !folder(&project).join("orders/PO-1.xlsx").exists(),
            "an order the log refused leaves no workbook behind, which would take its number"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_the_workbook_of_an_order_the_log_took() {
        let project = a_project("po-projection-refused");
        catervas_store::event_log::fixtures::refuse_projecting(&project.deps.log);

        let result = draft(&project, &an_order("Acme"));

        // The order is in the log, which is the truth: the projections catch up from it, and its
        // workbook is what the owner downloads, so it stays.
        assert_eq!(project.events(&[EventKind::PurchaseOrderDrafted]).len(), 1);
        assert!(
            folder(&project).join("orders/PO-1.xlsx").is_file(),
            "the workbook of a recorded order stays"
        );
        assert!(
            matches!(result, Err(ToolError::Failed { .. })),
            "{result:?}"
        );
        // The agent that is told it failed and tries again is told the order is there.
        let reason = refusal_of(draft(&project, &an_order("Acme")));
        assert!(reason.starts_with("purchase_order_open:"), "{reason}");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_comparison_reached_through_a_link() {
        let project = a_project("po-evaluation-link");
        let before = project.event_count();
        let procurement = folder(&project);

        // The note is there to read, but through a link: its folder is one.
        fs::rename(
            procurement.join("evaluations"),
            procurement.join("elsewhere"),
        )
        .expect("the folder is moved");
        std::os::unix::fs::symlink(
            procurement.join("elsewhere"),
            procurement.join("evaluations"),
        )
        .expect("a link is made");
        let reason = refusal_of(draft(&project, &an_order("Acme")));
        assert!(reason.starts_with("private_path_refused:"), "{reason}");

        // Or the note itself is one.
        fs::remove_file(procurement.join("evaluations")).expect("the link goes");
        fs::rename(
            procurement.join("elsewhere"),
            procurement.join("evaluations"),
        )
        .expect("the folder is back");
        fs::rename(
            procurement.join("evaluations/mirrors.md"),
            procurement.join("evaluations/original.md"),
        )
        .expect("the note is moved");
        std::os::unix::fs::symlink(
            procurement.join("evaluations/original.md"),
            procurement.join("evaluations/mirrors.md"),
        )
        .expect("a link is made");
        let reason = refusal_of(draft(&project, &an_order("Acme")));
        assert!(reason.starts_with("private_path_refused:"), "{reason}");

        assert_eq!(project.event_count(), before, "nothing was recorded");
        assert!(!procurement.join("orders").exists(), "nothing was written");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_another_role_and_another_session() {
        let project = a_project("po-role");
        let before = project.event_count();
        let input = an_order("Acme");

        let mut chat = project.context("proc", None);
        chat.purpose = SessionPurpose::Chat;
        let mut verify = project.context("proc", Some("CTV-1"));
        verify.purpose = SessionPurpose::Verify;
        let mut chat_about_a_task = project.context("proc", Some("CTV-1"));
        chat_about_a_task.purpose = SessionPurpose::Chat;
        for (what, context) in [
            (
                "the Finance Specialist",
                project.context("fin", Some("CTV-2")),
            ),
            ("a Developer", project.context("dev-a", Some("CTV-3"))),
            ("the Procurement Specialist's chat", chat),
            ("a chat about its task", chat_about_a_task),
            ("a verify session of its task", verify),
            (
                "an implement session of a task another agent has",
                project.context("proc", Some("CTV-3")),
            ),
            (
                "an implement session of no task",
                project.context("proc", None),
            ),
        ] {
            let reason = refusal_of(run(
                &context,
                "catervas_draft_purchase_order",
                input.clone(),
            ));
            assert!(
                reason.starts_with("purchase_order_refused:"),
                "{what}: {reason}"
            );
        }

        assert_eq!(project.event_count(), before, "nothing was recorded");
        assert!(
            !folder(&project).join("orders").exists(),
            "nothing was written"
        );
    }

    /// The valid order with `changes` put in, each a field of the order and its new value.
    fn changed(changes: &[(&str, Value)]) -> Value {
        let mut input = an_order("Acme");
        for (field, value) in changes {
            input[*field] = value.clone();
        }
        input
    }

    /// One line of an order, as the agent writes it.
    fn line(item: &str, quantity: u64, price: &str, unit: &str) -> Value {
        json!({ "item": item, "quantity": quantity, "unit_price": price, "unit": unit })
    }

    /// What was done to the valid order (the changes), why it is refused (a name for the case, the
    /// refusal's code and the field the refusal names).
    type Case<'a> = (&'a str, &'a str, &'a str, Vec<(&'a str, Value)>);

    /// Drafts each of `cases` on a project of its own, and holds the refusal to start with its
    /// code and to name its field, with nothing recorded or written on any.
    fn refuses_each(name: &str, cases: &[Case<'_>]) {
        let project = a_project(name);
        let before = project.event_count();
        for (what, code, field, changes) in cases {
            let reason = refusal_of(draft(&project, &changed(changes)));
            assert!(reason.starts_with(&format!("{code}:")), "{what}: {reason}");
            assert!(reason.contains(field), "{what} names {field}: {reason}");
        }
        assert_eq!(project.event_count(), before, "nothing was recorded on any");
        assert!(
            !folder(&project).join("orders").exists(),
            "nothing was written on any"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each fault the tool names"
    )]
    fn refuses_each_bad_field() {
        let long = |length: usize| "x".repeat(length);
        let lines = |lines: Vec<Value>| vec![("lines", json!(lines))];
        let seller = "purchase_order_seller_invalid";
        let bad_line = "purchase_order_line_invalid";
        refuses_each(
            "po-fields",
            &[
                (
                    "an empty seller",
                    seller,
                    "seller",
                    vec![("seller", json!(""))],
                ),
                (
                    "a seller with a line break",
                    seller,
                    "seller",
                    vec![("seller", json!("A\nB"))],
                ),
                (
                    "a seller past 100",
                    seller,
                    "seller",
                    vec![("seller", json!(long(101)))],
                ),
                (
                    "a contact past 200",
                    seller,
                    "seller_contact",
                    vec![("seller_contact", json!(long(201)))],
                ),
                (
                    "a contact with a line break",
                    seller,
                    "seller_contact",
                    vec![("seller_contact", json!("a\nb"))],
                ),
                (
                    "no lines",
                    "purchase_order_lines_invalid",
                    "lines",
                    lines(vec![]),
                ),
                (
                    "51 lines",
                    "purchase_order_lines_invalid",
                    "lines",
                    lines(vec![line("x", 1, "1.00", ""); 51]),
                ),
                (
                    "a quantity of 0",
                    bad_line,
                    "line 2 quantity",
                    lines(vec![line("a", 1, "1.00", ""), line("b", 0, "1.00", "")]),
                ),
                (
                    "a quantity past a million",
                    bad_line,
                    "line 1 quantity",
                    lines(vec![line("a", 1_000_001, "1.00", "")]),
                ),
                (
                    "a price with a separator",
                    bad_line,
                    "line 1 unit_price",
                    lines(vec![line("a", 1, "1,000", "")]),
                ),
                (
                    "a price with three decimals",
                    bad_line,
                    "line 1 unit_price",
                    lines(vec![line("a", 1, "10.999", "")]),
                ),
                (
                    "a negative price",
                    bad_line,
                    "line 1 unit_price",
                    lines(vec![line("a", 1, "-1", "")]),
                ),
                (
                    "a price of nine whole digits",
                    bad_line,
                    "line 1 unit_price",
                    lines(vec![line("a", 1, "123456789", "")]),
                ),
                (
                    "an empty item",
                    bad_line,
                    "line 1 item",
                    lines(vec![line("", 1, "1.00", "")]),
                ),
                (
                    "an item with a line break",
                    bad_line,
                    "line 1 item",
                    lines(vec![line("a\nb", 1, "1.00", "")]),
                ),
                (
                    "an item past 200",
                    bad_line,
                    "line 1 item",
                    lines(vec![line(&long(201), 1, "1.00", "")]),
                ),
                (
                    "a unit past 20",
                    bad_line,
                    "line 1 unit",
                    lines(vec![line("a", 1, "1.00", &long(21))]),
                ),
                (
                    "a unit with a line break",
                    bad_line,
                    "line 1 unit",
                    lines(vec![line("a", 1, "1.00", "a\nb")]),
                ),
                (
                    "a total past 10,000,000.00",
                    "purchase_order_too_large",
                    "10000000.01",
                    lines(vec![
                        line("a", 1_000_000, "10.00", ""),
                        line("b", 1, "0.01", ""),
                    ]),
                ),
                (
                    "a lower-case currency",
                    "purchase_order_currency_invalid",
                    "currency",
                    vec![("currency", json!("usd"))],
                ),
                (
                    "a four-letter currency",
                    "purchase_order_currency_invalid",
                    "currency",
                    vec![("currency", json!("USDT"))],
                ),
                (
                    "a two-letter currency",
                    "purchase_order_currency_invalid",
                    "currency",
                    vec![("currency", json!("US"))],
                ),
                (
                    "delivery past 600",
                    "purchase_order_delivery_invalid",
                    "delivery",
                    vec![("delivery", json!(long(601)))],
                ),
                (
                    "delivery with a bell",
                    "purchase_order_delivery_invalid",
                    "delivery",
                    vec![("delivery", json!("two days\u{7}"))],
                ),
                (
                    "terms past 600",
                    "purchase_order_terms_invalid",
                    "terms",
                    vec![("terms", json!(long(601)))],
                ),
            ],
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_page_comparison_or_reason() {
        let long = |length: usize| "x".repeat(length);
        let url = "purchase_order_url_invalid";
        let evaluation = "purchase_order_evaluation_invalid";
        let why = "purchase_order_why_invalid";
        let head = format!("https://{}/", a_catervas_host());
        refuses_each(
            "po-page",
            &[
                (
                    "a plain http address",
                    url,
                    "https",
                    vec![("url", json!("http://x.example/"))],
                ),
                (
                    "an address with a password",
                    url,
                    "user name",
                    vec![("url", json!("https://u:p@x.example/"))],
                ),
                (
                    "an IP address",
                    url,
                    "IP address",
                    vec![("url", json!("https://1.2.3.4/"))],
                ),
                (
                    "an address of 2001",
                    url,
                    "2000",
                    vec![("url", json!(format!("{head}{}", long(2_001 - head.len()))))],
                ),
                (
                    "a comparison that is not there",
                    "evaluation_missing",
                    "evaluations/none.md",
                    vec![("evaluation", json!("evaluations/none.md"))],
                ),
                (
                    "a comparison above the folder",
                    evaluation,
                    "../x.md",
                    vec![("evaluation", json!("../x.md"))],
                ),
                (
                    "a comparison that is no evaluation",
                    evaluation,
                    "notes/x.md",
                    vec![("evaluation", json!("notes/x.md"))],
                ),
                (
                    "a comparison that is no note",
                    evaluation,
                    "evaluations/mirrors.txt",
                    vec![("evaluation", json!("evaluations/mirrors.txt"))],
                ),
                ("a reason of 19", why, "why", vec![("why", json!(long(19)))]),
                (
                    "a reason past 600",
                    why,
                    "why",
                    vec![("why", json!(long(601)))],
                ),
                (
                    "a reason with a bell",
                    why,
                    "why",
                    vec![("why", json!("The cheapest seller that ships here.\u{7}"))],
                ),
            ],
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn names_the_first_fault_and_takes_every_limit() {
        let project = a_project("po-limits");
        // The first fault in the order the tool checks them is the one it names.
        let reason = refusal_of(draft(
            &project,
            &changed(&[("currency", json!("usd")), ("seller", json!(""))]),
        ));
        assert!(
            reason.starts_with("purchase_order_seller_invalid:"),
            "{reason}"
        );
        // A period is one of three words, and the input holds nothing else.
        for bad in [("period", json!("week")), ("paid", json!("1.00"))] {
            assert!(matches!(
                draft(&project, &changed(&[bad])),
                Err(ToolError::InvalidInput { .. })
            ));
        }
        // The edges that are allowed: the longest seller and reason (a reason may run to a
        // second line), a unit left empty, a seller with no page, delivery and terms of 600.
        let mut edges = an_order(&"s".repeat(100));
        edges["why"] = json!(format!("{}\nand\tmore{}", "w".repeat(300), "w".repeat(291)));
        edges["url"] = json!("");
        edges["seller_contact"] = json!("c".repeat(200));
        edges["delivery"] = json!(format!("{}\r\n\t{}", "d".repeat(300), "d".repeat(297)));
        edges["terms"] = json!("t".repeat(600));
        edges["lines"] = json!([line(&"i".repeat(200), 1_000_000, "10.00", &"u".repeat(20))]);
        let answer = draft(&project, &edges).expect("every limit may be reached");
        assert_eq!(answer["total"], "10000000.00");
        // An address of 2,000 characters is the longest taken.
        let head = format!("https://{}/", a_catervas_host());
        let mut long_page = an_order("Long page");
        long_page["url"] = json!(format!("{head}{}", "x".repeat(2_000 - head.len())));
        draft(&project, &long_page).expect("an address of 2,000 characters");
        // Fifty lines are the most an order has, and a reason of 20 characters the shortest.
        let mut fifty = an_order("Fifty lines");
        fifty["lines"] = json!(vec![line("Box", 1, "1.00", ""); 50]);
        fifty["why"] = json!("w".repeat(20));
        let answer = draft(&project, &fifty).expect("50 lines and a reason of 20 characters");
        assert_eq!(answer["total"], "50.00");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_page_not_on_an_approved_site() {
        let project = a_project("po-site");
        let before = project.event_count();

        let reason = refusal_of(draft(
            &project,
            &changed(&[("url", json!("https://acme.example/x"))]),
        ));
        assert!(
            reason.starts_with("purchase_order_site_not_approved:"),
            "{reason}"
        );
        assert!(reason.contains("acme.example"), "{reason}");
        assert!(reason.contains("catervas_request_sites"), "{reason}");
        assert_eq!(project.event_count(), before);
        assert!(!folder(&project).join("orders").exists());

        // Once the owner allowed the site, its page is taken, with or without www.
        project.record("", "site.approved", &json!({ "host": "acme.example" }));
        let mut input = an_order("Acme");
        input["url"] = json!("https://www.acme.example/x");
        draft(&project, &input).expect("an allowed site's page is taken");

        // A page on a site Catervas ships is taken; once the owner turned that site off, it is not.
        let mut shipped = an_order("Bolt");
        shipped["url"] = json!(format!("https://{}/p", a_catervas_host()));
        draft(&project, &shipped).expect("a site Catervas ships is taken");
        project.record("", "site.removed", &json!({ "host": a_catervas_host() }));
        let mut turned_off = an_order("Cog");
        turned_off["url"] = json!(format!("https://{}/p", a_catervas_host()));
        let reason = refusal_of(draft(&project, &turned_off));
        assert!(
            reason.starts_with("purchase_order_site_not_approved:"),
            "{reason}"
        );

        // A seller with no page is met by phone or in person.
        let mut none = an_order("Dot");
        none["url"] = json!("");
        draft(&project, &none).expect("an order with no page is taken");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_second_open_order_for_a_seller() {
        let project = a_project("po-open");
        draft(&project, &an_order("Acme")).expect("the first order");

        let reason = refusal_of(draft(&project, &an_order("acme")));
        assert!(reason.starts_with("purchase_order_open:"), "{reason}");
        assert!(reason.contains("PO-1"), "{reason}");

        // Approved, and then placed, it is open still.
        project.record(
            "CTV-1",
            "purchase_order.approved",
            &json!({ "order": 1, "note": "" }),
        );
        assert!(refusal_of(draft(&project, &an_order("ACME"))).starts_with("purchase_order_open:"));
        project.record(
            "CTV-1",
            "purchase_order.placed",
            &json!({ "order": 1, "placed_on": "2026-09-22" }),
        );
        assert!(refusal_of(draft(&project, &an_order("Acme"))).starts_with("purchase_order_open:"));

        // On another task the same seller is another order.
        let other = project
            .call(
                "proc",
                Some("CTV-4"),
                "catervas_draft_purchase_order",
                an_order("Acme"),
            )
            .expect("another task may draft for the same seller");
        assert_eq!(other["order"], 2);

        // Rejected, closed or expired, an order no longer stops a new one for its seller.
        for (kind, seller) in [
            ("purchase_order.rejected", "Bolt"),
            ("purchase_order.expired", "Cog"),
        ] {
            let drafted = draft(&project, &an_order(seller)).expect("a new seller");
            let number = drafted["order"].as_u64().expect("a number");
            assert!(
                refusal_of(draft(&project, &an_order(seller))).starts_with("purchase_order_open:")
            );
            let body = if kind == "purchase_order.expired" {
                json!({ "order": number })
            } else {
                json!({ "order": number, "note": "" })
            };
            project.record("CTV-1", kind, &body);
            draft(&project, &an_order(seller)).expect("a new order once it ended");
        }
        project.record(
            "CTV-1",
            "purchase_order.received",
            &json!({ "order": 1, "received_on": "2026-09-23" }),
        );
        draft(&project, &an_order("Acme")).expect("a new order once the first was received");

        // A placed order the owner closed (it did not come) is over too.
        let drafted = draft(&project, &an_order("Dot")).expect("a new seller");
        let number = drafted["order"].as_u64().expect("a number");
        project.record(
            "CTV-1",
            "purchase_order.approved",
            &json!({ "order": number, "note": "" }),
        );
        project.record(
            "CTV-1",
            "purchase_order.placed",
            &json!({ "order": number, "placed_on": "2026-09-22" }),
        );
        assert!(refusal_of(draft(&project, &an_order("Dot"))).starts_with("purchase_order_open:"));
        project.record(
            "CTV-1",
            "purchase_order.closed",
            &json!({ "order": number, "note": "It did not come." }),
        );
        draft(&project, &an_order("Dot")).expect("a new order once the first was closed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn holds_at_most_twenty_waiting() {
        let project = a_project("po-cap");
        for number in 1..=20 {
            draft(&project, &an_order(&format!("Seller {number}"))).expect("an order is drafted");
        }
        let before = project.event_count();

        let reason = refusal_of(draft(&project, &an_order("Seller 21")));
        assert!(reason.starts_with("too_many_purchase_orders:"), "{reason}");
        assert!(
            reason.contains("catervas_read_purchase_orders"),
            "it names the tool that reads the orders: {reason}"
        );
        assert_eq!(project.event_count(), before);
        assert!(!folder(&project).join("orders/PO-21.xlsx").exists());

        // Orders approved are no longer waiting for the owner.
        project.record(
            "CTV-1",
            "purchase_order.approved",
            &json!({ "order": 1, "note": "" }),
        );
        let answer = draft(&project, &an_order("Seller 21")).expect("a place is free");
        assert_eq!(answer["order"], 21);
    }
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_draft_waits_for_the_orders_lock() {
        let project = a_project("po-draft-lock");

        // Held by another step, the draft has read no order and numbered none; once the lock is
        // free it goes on. Without the lock, two drafts racing past the open-order check and the
        // cap would both pass it.
        let answer = waits_for_the_lock(&crate::procurement::ORDERS, &project, || {
            draft(&project, &an_order("Acme"))
        })
        .expect("the draft goes on once the lock is free");

        assert_eq!(answer["order"], 1);
        assert_eq!(project.events(&[EventKind::PurchaseOrderDrafted]).len(), 1);
    }

    /// Records the owner's step on order `number`: an event with the order's task and no agent.
    fn owner(project: &TestProject, kind: &str, body: &Value) {
        project.record("CTV-1", kind, body);
    }

    /// `catervas_read_purchase_orders` as `proc` in its implement session of CTV-1.
    fn read_orders(project: &TestProject) -> Result<Value, ToolError> {
        project.call(
            "proc",
            Some("CTV-1"),
            "catervas_read_purchase_orders",
            json!({}),
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one order for each outcome the tool shows"
    )]
    fn reads_every_order_and_its_outcome() {
        let project = a_project("po-read");
        for seller in ["A", "B", "C", "D", "E", "F", "G", "H"] {
            draft(&project, &an_order(seller)).expect("an order is drafted");
        }
        let approve = |order: u64, note: &str| {
            owner(
                &project,
                "purchase_order.approved",
                &json!({ "order": order, "note": note }),
            );
        };
        let place = |order: u64, on: &str, extra: Value| {
            let mut body = json!({ "order": order, "placed_on": on });
            for (field, value) in extra.as_object().into_iter().flatten() {
                body[field] = value.clone();
            }
            owner(&project, "purchase_order.placed", &body);
        };
        // 1 stays drafted; 2 is approved with a note; 3 is rejected with one.
        approve(2, "Go ahead, <b>B</b>.");
        owner(
            &project,
            "purchase_order.rejected",
            &json!({ "order": 3, "note": "Too dear." }),
        );
        // 4 is placed long ago, with an amount, and followed up by its agent.
        approve(4, "");
        place(
            4,
            "2026-08-01",
            json!({ "paid": "1450.00", "currency": "EUR" }),
        );
        project.record_by(
            Some("proc"),
            crate::tools::fixtures::at(),
            "CTV-1",
            "purchase_order.updated",
            &json!({ "order": 4, "status": "shipped", "note": "Left the depot." }),
        );
        // 5 came, and the owner paid in the order's currency; 6 was closed.
        approve(5, "");
        place(5, "2026-09-20", json!({}));
        owner(
            &project,
            "purchase_order.received",
            &json!({ "order": 5, "received_on": "2026-09-21", "paid": "99.50", "renews_on": "2027-09-21" }),
        );
        approve(6, "");
        place(6, "2026-09-01", json!({}));
        owner(
            &project,
            "purchase_order.closed",
            &json!({ "order": 6, "note": "It was lost." }),
        );
        // 7 expired; 8 is placed lately, with the owner's own status.
        owner(&project, "purchase_order.expired", &json!({ "order": 7 }));
        approve(8, "");
        place(8, "2026-09-20", json!({}));
        owner(
            &project,
            "purchase_order.updated",
            &json!({ "order": 8, "status": "delayed", "note": "The seller called.", "expected_on": "2026-10-30" }),
        );

        let answer = read_orders(&project).expect("the orders are read");

        let orders = answer["orders"].as_array().expect("a list of orders");
        let states: Vec<&str> = orders
            .iter()
            .map(|one| one["state"].as_str().expect("a state"))
            .collect();
        assert_eq!(
            states,
            [
                "drafted", "approved", "rejected", "placed", "received", "closed", "expired",
                "placed"
            ],
            "oldest first, each with its state"
        );
        let numbers: Vec<u64> = orders
            .iter()
            .map(|one| one["order"].as_u64().expect("a number"))
            .collect();
        assert_eq!(numbers, [1, 2, 3, 4, 5, 6, 7, 8]);
        let first = &orders[0];
        assert_eq!(first["seller"], "A");
        assert_eq!(first["total"], "59.98");
        assert_eq!(first["currency"], "USD");
        assert_eq!(first["period"], "once");
        assert_eq!(first["lines"][0]["item"], "Baby car mirror");
        assert_eq!(first["lines"][0]["quantity"], 3);
        assert_eq!(first["lines"][0]["line_total"], "59.97");
        assert_eq!(first["task_id"], "CTV-1");
        assert_eq!(first["overdue"], false);
        assert!(
            first.get("note").is_none() && first.get("status").is_none(),
            "{first}"
        );
        // The owner's words are theirs: shown as written, not wrapped as untrusted content.
        assert_eq!(orders[1]["note"], "Go ahead, <b>B</b>.");
        assert_eq!(orders[2]["note"], "Too dear.");
        assert_eq!(orders[5]["close_note"], "It was lost.");
        assert!(!answer.to_string().contains("<untrusted"), "{answer}");
        // A placed order: its day, what was paid, the agent's status, and overdue past 30 days.
        let placed = &orders[3];
        assert_eq!(placed["placed_on"], "2026-08-01");
        assert_eq!(placed["paid"], "1450.00");
        assert_eq!(placed["paid_currency"], "EUR");
        assert_eq!(placed["status"]["status"], "shipped");
        assert_eq!(placed["status"]["note"], "Left the depot.");
        assert_eq!(placed["status"]["by"], "agent");
        assert_eq!(placed["overdue"], true);
        // A received one: what was paid in the order's own currency, and when it renews.
        let received = &orders[4];
        assert_eq!(received["received_on"], "2026-09-21");
        assert_eq!(received["paid"], "99.50");
        assert_eq!(received["paid_currency"], "USD");
        assert_eq!(received["renews_on"], "2027-09-21");
        assert_eq!(
            received["overdue"], false,
            "a received order is never overdue"
        );
        // The owner's correction says who made it, and its day keeps the order from being late.
        let corrected = &orders[7];
        assert_eq!(corrected["status"]["by"], "owner");
        assert_eq!(corrected["status"]["expected_on"], "2026-10-30");
        assert_eq!(corrected["overdue"], false);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_orders_in_a_chat_and_to_no_one_else() {
        let project = a_project("po-read-who");
        draft(&project, &an_order("Acme")).expect("an order is drafted");
        let mut chat = project.context("proc", None);
        chat.purpose = SessionPurpose::Chat;
        let answer = run(&chat, "catervas_read_purchase_orders", json!({})).expect("a chat reads");
        assert_eq!(answer["orders"].as_array().map(Vec::len), Some(1));
        for (who, task) in [("fin", "CTV-2"), ("dev-a", "CTV-3"), ("pm", "CTV-1")] {
            let reason = refusal_of(project.call(
                who,
                Some(task),
                "catervas_read_purchase_orders",
                json!({}),
            ));
            assert!(
                reason.starts_with("purchase_order_refused:"),
                "{who}: {reason}"
            );
        }
    }

    /// A project with a second Procurement Specialist, `proc-b`, whose task CTV-5 is in progress,
    /// and the orders 1 to 5 of `proc` on CTV-1 in the states a follow-up meets: 1 drafted, 2
    /// approved, 3 placed, 4 received, 5 closed.
    fn a_project_with_orders(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_procurement_specialist(wire);
                wire["agents"]
                    .as_array_mut()
                    .expect("a list of agents")
                    .push(catervas_core::team::fixtures::an_agent_wire(
                        "proc-b",
                        "procurement_specialist",
                    ));
            }),
        );
        for (task, role, assignee) in [
            ("CTV-1", "procurement_specialist", "proc"),
            ("CTV-4", "procurement_specialist", "proc"),
            ("CTV-5", "procurement_specialist", "proc-b"),
            ("CTV-2", "finance_specialist", "fin"),
        ] {
            project.filed_with(task, "assigned", "task", None, |wire| {
                wire["assignee_role"] = json!(role);
                wire["reviewer_role"] = json!("product_manager");
            });
            project.moved(
                task,
                "assigned",
                "in_progress",
                &json!({ "assignee": assignee, "reviewer": "pm" }),
            );
        }
        project
            .call(
                "proc",
                Some("CTV-1"),
                "catervas_write_evaluation",
                json!({ "name": "mirrors", "text": "# Mirrors" }),
            )
            .expect("the comparison is written");
        for seller in ["A", "B", "C", "D", "E"] {
            draft(&project, &an_order(seller)).expect("an order is drafted");
        }
        for order in 2..=5 {
            owner(
                &project,
                "purchase_order.approved",
                &json!({ "order": order, "note": "" }),
            );
        }
        for order in 3..=5 {
            owner(
                &project,
                "purchase_order.placed",
                &json!({ "order": order, "placed_on": "2026-09-20" }),
            );
        }
        owner(
            &project,
            "purchase_order.received",
            &json!({ "order": 4, "received_on": "2026-09-21" }),
        );
        owner(
            &project,
            "purchase_order.closed",
            &json!({ "order": 5, "note": "" }),
        );
        project
    }

    /// `catervas_update_purchase_order` as `proc` in its implement session of its follow-up task CTV-4.
    fn follow_up(project: &TestProject, input: &Value) -> Result<Value, ToolError> {
        project.call(
            "proc",
            Some("CTV-4"),
            "catervas_update_purchase_order",
            input.clone(),
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_follow_up_on_its_own_placed_order() {
        let project = a_project_with_orders("po-update");

        // A follow-up task of its own records on the order, which stays in the order's task.
        follow_up(
            &project,
            &json!({ "order": 3, "status": "shipped", "note": "Left the depot.", "expected_on": "2026-10-02" }),
        )
        .expect("the follow-up is recorded");
        let updated = project.events(&[EventKind::PurchaseOrderUpdated]);
        assert_eq!(updated.len(), 1);
        let ids = &updated[0].envelope.ids;
        assert_eq!(
            ids.agent_id.as_deref(),
            Some("proc"),
            "it never counts as the owner's"
        );
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.as_str()),
            Some("CTV-1")
        );
        let EventBody::PurchaseOrderUpdated(body) = &updated[0].body else {
            panic!("a purchase_order.updated event carries its body");
        };
        assert_eq!(body.order.get(), 3);
        assert_eq!(body.status.to_string(), "shipped");
        assert_eq!(body.note.to_string(), "Left the depot.");
        assert_eq!(
            body.expected_on.map(|day| day.to_string()),
            Some("2026-10-02".to_string())
        );
        // The note and the day are optional for a status that needs neither.
        follow_up(
            &project,
            &json!({ "order": 3, "status": "preparing", "note": "" }),
        )
        .expect("preparing");
        follow_up(&project, &json!({ "order": 3, "status": "delayed", "note": "Short of flour.", "expected_on": "2026-09-22" }))
            .expect("a day that is today is not before it");
        follow_up(
            &project,
            &json!({ "order": 3, "status": "problem", "note": "Out of stock." }),
        )
        .expect("a problem");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_follow_up_waits_for_the_orders_lock() {
        let project = a_project_with_orders("po-update-lock");

        // The follow-up reads the order's state under the lock, so that the owner's step on the
        // order (closing it, say) and the follow-up are not both taken.
        waits_for_the_lock(&crate::procurement::ORDERS, &project, || {
            follow_up(
                &project,
                &json!({ "order": 3, "status": "shipped", "note": "Left the depot." }),
            )
        })
        .expect("the follow-up goes on once the lock is free");

        assert_eq!(project.events(&[EventKind::PurchaseOrderUpdated]).len(), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_follow_up_it_may_not_record() {
        let project = a_project_with_orders("po-update-refused");
        let before = project.event_count();

        let invalid = [
            json!({ "order": 3, "status": "delayed", "note": "Late." }),
            json!({ "order": 3, "status": "delayed", "note": "", "expected_on": "2026-10-02" }),
            json!({ "order": 3, "status": "problem", "note": "" }),
            json!({ "order": 3, "status": "problem", "note": "   " }),
            json!({ "order": 3, "status": "shipped", "note": "x".repeat(301) }),
            json!({ "order": 3, "status": "shipped", "note": "a\nb" }),
            json!({ "order": 3, "status": "shipped", "note": "", "expected_on": "2026-09-21" }),
            json!({ "order": 3, "status": "shipped", "note": "", "expected_on": "soon" }),
            json!({ "order": 3, "status": "shipped", "note": "", "expected_on": "2026-9-30" }),
            json!({ "order": 3, "status": "placed", "note": "Done.", "expected_on": "2026-10-02" }),
            json!({ "order": 3, "status": "received", "note": "Done.", "expected_on": "2026-10-02" }),
            json!({ "order": 3, "status": "paid", "note": "Done.", "expected_on": "2026-10-02" }),
            json!({ "order": 3, "status": "confirmed", "note": "Done.", "expected_on": "2026-10-02" }),
            json!({ "order": 3, "status": "cancelled", "note": "Done.", "expected_on": "2026-10-02" }),
        ];
        for input in &invalid {
            let reason = refusal_of(follow_up(&project, input));
            assert!(
                reason.starts_with("purchase_order_status_invalid:"),
                "{input}: {reason}"
            );
        }
        // Not placed: drafted, approved, received or closed; and a number nobody drafted.
        for order in [1, 2, 4, 5] {
            let reason = refusal_of(follow_up(
                &project,
                &json!({ "order": order, "status": "shipped", "note": "" }),
            ));
            assert!(
                reason.starts_with("purchase_order_not_placed:"),
                "{order}: {reason}"
            );
        }
        let reason = refusal_of(follow_up(
            &project,
            &json!({ "order": 99, "status": "shipped", "note": "" }),
        ));
        assert!(reason.starts_with("unknown_purchase_order:"), "{reason}");
        // Another Procurement Specialist's order is not this one's to follow up.
        let reason = refusal_of(project.call(
            "proc-b",
            Some("CTV-5"),
            "catervas_update_purchase_order",
            json!({ "order": 3, "status": "shipped", "note": "" }),
        ));
        assert!(reason.starts_with("purchase_order_not_yours:"), "{reason}");
        // Its chat, another role and a session of a task another agent has follow up nothing.
        let mut chat = project.context("proc", None);
        chat.purpose = SessionPurpose::Chat;
        for (what, context) in [
            ("its chat", chat),
            (
                "the Finance Specialist",
                project.context("fin", Some("CTV-2")),
            ),
            (
                "a task another agent has",
                project.context("proc", Some("CTV-5")),
            ),
            ("no task", project.context("proc", None)),
        ] {
            let reason = refusal_of(run(
                &context,
                "catervas_update_purchase_order",
                json!({ "order": 3, "status": "shipped", "note": "" }),
            ));
            assert!(
                reason.starts_with("purchase_order_update_refused:"),
                "{what}: {reason}"
            );
        }
        assert_eq!(project.event_count(), before, "nothing was recorded on any");
    }
}
