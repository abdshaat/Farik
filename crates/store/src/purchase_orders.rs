//! The purchase orders the Procurement Specialist suggests (`docs/SPEC.md` 6.10, ADR 0039), folded
//! from the eight `purchase_order.` kinds of the project's log. An order is drafted by the agent;
//! approved or rejected, placed, then received or closed by the owner; or expired by Catervas. Only
//! an event whose envelope names no agent and no session counts as a step after the drafting, so
//! no Catervas tool and no agent can move an order or record what was paid; a placed order's follow-up
//! status is its own agent's, or the owner's correction of it.

use catervas_core::contract::TaskId;
use catervas_protocol::event::{
    CatervasEvent, EventBody, EventKind, PurchaseOrderDraftedBody, PurchaseOrderStatus,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::{EventLog, EventQuery, StoreError};

/// How many days an order waits to be decided, or to be marked placed once approved, before Catervas
/// closes it by itself.
pub const EXPIRES_AFTER_DAYS: i64 = 30;
/// How many days after it was placed an order is expected, when no follow-up says another day.
pub const EXPECTED_AFTER_DAYS: i64 = 30;

/// Where an order stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    /// The agent drafted it; the owner has not decided.
    Drafted,
    /// The owner approved it: they place and pay for it themselves.
    Approved,
    /// The owner rejected it.
    Rejected,
    /// The owner placed it and marked it placed; it is on its way, or late.
    Placed,
    /// The owner marked it received.
    Received,
    /// The owner closed it: the seller cancelled or refunded it, or it was lost.
    Closed,
    /// Catervas closed it: nobody decided or placed it in time.
    Expired,
}

impl OrderState {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drafted => "drafted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Placed => "placed",
            Self::Received => "received",
            Self::Closed => "closed",
            Self::Expired => "expired",
        }
    }
}

/// What a follow-up learned about a placed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUp {
    /// The seller is making or packing it.
    Preparing,
    /// It is on its way.
    Shipped,
    /// It will be late.
    Delayed,
    /// Out of stock, a payment refused, cancelled by the seller.
    Problem,
}

impl FollowUp {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Shipped => "shipped",
            Self::Delayed => "delayed",
            Self::Problem => "problem",
        }
    }
}

impl From<PurchaseOrderStatus> for FollowUp {
    fn from(status: PurchaseOrderStatus) -> Self {
        match status {
            PurchaseOrderStatus::Preparing => Self::Preparing,
            PurchaseOrderStatus::Shipped => Self::Shipped,
            PurchaseOrderStatus::Delayed => Self::Delayed,
            PurchaseOrderStatus::Problem => Self::Problem,
        }
    }
}

/// The latest follow-up status of a placed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderStatus {
    /// What was learned.
    pub status: FollowUp,
    /// The reason or what is known, on one line, possibly empty. The agent's words are untrusted.
    pub note: String,
    /// The day the seller expects the order, when it was given.
    pub expected_on: Option<NaiveDate>,
    /// Whether the owner recorded it (a correction) and not the order's agent.
    pub by_owner: bool,
    /// When it was recorded.
    pub at: DateTime<Utc>,
}

/// One purchase order, as the log tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct PurchaseOrderRecord {
    /// The order's number, the n of PO-n.
    pub order: u64,
    /// The task it was drafted in.
    pub task_id: TaskId,
    /// The agent that drafted it.
    pub agent_id: String,
    /// What the agent wrote.
    pub drafted: PurchaseOrderDraftedBody,
    /// When it was drafted.
    pub drafted_at: DateTime<Utc>,
    /// Where it stands.
    pub state: OrderState,
    /// When the owner approved or rejected it.
    pub decided_at: Option<DateTime<Utc>>,
    /// The owner's words on the decision, when they said any.
    pub note: Option<String>,
    /// The day the owner placed it.
    pub placed_on: Option<NaiveDate>,
    /// What the owner paid and in which currency, the latest given at placing or receiving.
    pub paid: Option<(String, String)>,
    /// The day the owner marked it received.
    pub received_on: Option<NaiveDate>,
    /// The day it renews, when the owner said at receiving.
    pub renews_on: Option<NaiveDate>,
    /// When it ended: rejected, received, closed or expired.
    pub ended_at: Option<DateTime<Utc>>,
    /// The owner's words on closing it, when they said any.
    pub close_note: Option<String>,
    /// The latest follow-up status of a placed order.
    pub status: Option<OrderStatus>,
}

/// The owner's words, when they said any.
fn words(note: &str) -> Option<String> {
    Some(note.to_string()).filter(|note| !note.is_empty())
}

/// Takes what the owner paid, when they said: the amount and the currency given beside it, else
/// the order's. The latest amount given, at placing or receiving, is what was paid.
fn pay(record: &mut PurchaseOrderRecord, paid: Option<&str>, currency: Option<&str>) {
    if let Some(paid) = paid {
        let currency = currency.unwrap_or_else(|| record.drafted.currency.as_str());
        record.paid = Some((paid.to_string(), currency.to_string()));
    }
}

/// The order a `purchase_order.drafted` event starts, if it names its task.
fn drafted(event: &CatervasEvent, body: &PurchaseOrderDraftedBody) -> Option<PurchaseOrderRecord> {
    Some(PurchaseOrderRecord {
        order: body.order.get(),
        task_id: event.envelope.ids.task_id.clone()?,
        agent_id: event.envelope.ids.agent_id.clone().unwrap_or_default(),
        drafted: body.clone(),
        drafted_at: event.envelope.recorded_at,
        state: OrderState::Drafted,
        decided_at: None,
        note: None,
        placed_on: None,
        paid: None,
        received_on: None,
        renews_on: None,
        ended_at: None,
        close_note: None,
        status: None,
    })
}

/// The order numbered `number`, if the log has drafted it.
fn find(orders: &mut [PurchaseOrderRecord], number: u64) -> Option<&mut PurchaseOrderRecord> {
    orders.iter_mut().find(|record| record.order == number)
}

/// The order `event` is about, when it is in `from` and the event is the owner's or Catervas's: its
/// envelope names no agent and no session.
fn stepped<'a>(
    orders: &'a mut [PurchaseOrderRecord],
    event: &CatervasEvent,
    number: u64,
    from: &[OrderState],
) -> Option<&'a mut PurchaseOrderRecord> {
    let ids = &event.envelope.ids;
    if ids.agent_id.is_some() || ids.session_id.is_some() {
        return None;
    }
    find(orders, number).filter(|record| from.contains(&record.state))
}

/// Every order the log holds, oldest first. A step counts only when its envelope names no agent
/// and no session, only the first of its kind that the order's state takes counts (approve or
/// reject a drafted order; place an approved one; receive or close a placed one; expire a drafted
/// or an approved one), and a step for a number nobody drafted is no one's. A status counts while
/// the order is placed, from the agent that drafted it or from the owner, and the latest wins.
///
/// # Errors
///
/// What the log refused.
pub fn purchase_orders(log: &EventLog) -> Result<Vec<PurchaseOrderRecord>, StoreError> {
    use OrderState::{Approved, Drafted, Placed};
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::PurchaseOrderDrafted,
            EventKind::PurchaseOrderApproved,
            EventKind::PurchaseOrderRejected,
            EventKind::PurchaseOrderPlaced,
            EventKind::PurchaseOrderUpdated,
            EventKind::PurchaseOrderReceived,
            EventKind::PurchaseOrderClosed,
            EventKind::PurchaseOrderExpired,
        ],
        ..EventQuery::default()
    })?;
    let mut orders: Vec<PurchaseOrderRecord> = Vec::new();
    for event in &events {
        let at = event.envelope.recorded_at;
        match &event.body {
            EventBody::PurchaseOrderDrafted(body) => {
                if find(&mut orders, body.order.get()).is_none()
                    && let Some(record) = drafted(event, body)
                {
                    orders.push(record);
                }
            }
            EventBody::PurchaseOrderApproved(body) => {
                if let Some(record) = stepped(&mut orders, event, body.order.get(), &[Drafted]) {
                    record.state = Approved;
                    record.decided_at = Some(at);
                    record.note = words(&body.note);
                }
            }
            EventBody::PurchaseOrderRejected(body) => {
                if let Some(record) = stepped(&mut orders, event, body.order.get(), &[Drafted]) {
                    record.state = OrderState::Rejected;
                    record.decided_at = Some(at);
                    record.ended_at = Some(at);
                    record.note = words(&body.note);
                }
            }
            EventBody::PurchaseOrderPlaced(body) => {
                if let Some(record) = stepped(&mut orders, event, body.order.get(), &[Approved]) {
                    record.state = Placed;
                    record.placed_on = Some(body.placed_on);
                    pay(
                        record,
                        body.paid.as_ref().map(|paid| paid.as_str()),
                        body.currency.as_ref().map(|currency| currency.as_str()),
                    );
                }
            }
            EventBody::PurchaseOrderUpdated(body) => {
                let ids = &event.envelope.ids;
                let from_owner = ids.agent_id.is_none() && ids.session_id.is_none();
                if let Some(record) = find(&mut orders, body.order.get())
                    && record.state == Placed
                    && (from_owner || ids.agent_id.as_deref() == Some(record.agent_id.as_str()))
                {
                    record.status = Some(OrderStatus {
                        status: body.status.into(),
                        note: body.note.to_string(),
                        expected_on: body.expected_on,
                        by_owner: from_owner,
                        at,
                    });
                }
            }
            EventBody::PurchaseOrderReceived(body) => {
                if let Some(record) = stepped(&mut orders, event, body.order.get(), &[Placed]) {
                    record.state = OrderState::Received;
                    record.received_on = Some(body.received_on);
                    record.renews_on = body.renews_on;
                    record.ended_at = Some(at);
                    pay(
                        record,
                        body.paid.as_ref().map(|paid| paid.as_str()),
                        body.currency.as_ref().map(|currency| currency.as_str()),
                    );
                }
            }
            EventBody::PurchaseOrderClosed(body) => {
                if let Some(record) = stepped(&mut orders, event, body.order.get(), &[Placed]) {
                    record.state = OrderState::Closed;
                    record.ended_at = Some(at);
                    record.close_note = words(&body.note);
                }
            }
            EventBody::PurchaseOrderExpired(body) => {
                if let Some(record) =
                    stepped(&mut orders, event, body.order.get(), &[Drafted, Approved])
                {
                    record.state = OrderState::Expired;
                    record.ended_at = Some(at);
                }
            }
            _ => {}
        }
    }
    Ok(orders)
}

/// When Catervas closes the order by itself: 30 days after its drafting while it is drafted, 30 days
/// after its approval while it is approved. A placed order never expires.
#[must_use]
pub fn expires_at(record: &PurchaseOrderRecord) -> Option<DateTime<Utc>> {
    let after = Duration::days(EXPIRES_AFTER_DAYS);
    match record.state {
        OrderState::Drafted => Some(record.drafted_at + after),
        OrderState::Approved => record.decided_at.map(|decided| decided + after),
        _ => None,
    }
}

/// Whether a placed order's expected day has passed on `today`: the day its latest status gives,
/// or with none, 30 days after it was placed.
#[must_use]
pub fn overdue(record: &PurchaseOrderRecord, today: NaiveDate) -> bool {
    if record.state != OrderState::Placed {
        return false;
    }
    record
        .status
        .as_ref()
        .and_then(|status| status.expected_on)
        .or_else(|| {
            record
                .placed_on
                .map(|placed| placed + Duration::days(EXPECTED_AFTER_DAYS))
        })
        .is_some_and(|expected| today > expected)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate};
    use serde_json::{Value, json};

    use super::{FollowUp, OrderState, expires_at, overdue, purchase_orders};
    use crate::waiting::fixtures::{Board, at};

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
    }

    /// The body of an order Ivo drafted: three mirrors and a kit, 59.98 USD.
    fn drafted_body(order: u64, seller: &str) -> Value {
        json!({
            "order": order, "seller": seller, "seller_contact": "sales@acme.example",
            "lines": [
                { "item": "Baby car mirror", "quantity": 3, "unit": "piece",
                  "unit_price": "19.99", "line_total": "59.97" },
                { "item": "Mounting kit", "quantity": 1, "unit": "",
                  "unit_price": "0.01", "line_total": "0.01" }
            ],
            "currency": "USD", "period": "once", "total": "59.98",
            "delivery": "3 days", "terms": "Net 30",
            "url": "https://www.acme.example/shop", "evaluation": "evaluations/mirrors.md",
            "why": "It is the cheapest seller that ships to us."
        })
    }

    /// Ivo drafts order `order` on FRK-1, in his session, at 10:`minute`.
    fn draft(board: &Board, minute: u32, order: u64, seller: &str) {
        board.session(
            at(10, minute),
            Some("FRK-1"),
            "ivo",
            "session-1",
            "purchase_order.drafted",
            drafted_body(order, seller),
        );
    }

    /// The owner's step: an event with the order's task and no agent and no session.
    fn owner(board: &Board, minute: u32, kind: &str, body: Value) {
        board.put(at(10, minute), Some("FRK-1"), None, kind, body);
    }

    fn only(board: &Board) -> super::PurchaseOrderRecord {
        let mut all = purchase_orders(&board.log).expect("the store reads");
        assert_eq!(all.len(), 1);
        all.remove(0)
    }

    #[test]
    fn only_the_owner_moves_an_order() {
        let board = Board::new("orders-fold");
        draft(&board, 0, 1, "Acme");
        let drafted = only(&board);
        assert_eq!(drafted.state, OrderState::Drafted);
        assert_eq!(drafted.order, 1);
        assert_eq!(drafted.task_id.as_str(), "FRK-1");
        assert_eq!(drafted.agent_id, "ivo");
        assert_eq!(drafted.drafted_at, at(10, 0));
        assert_eq!(drafted.drafted.seller.as_str(), "Acme");
        assert_eq!(drafted.drafted.total.as_str(), "59.98");

        owner(
            &board,
            1,
            "purchase_order.approved",
            json!({ "order": 1, "note": "Go." }),
        );
        let approved = only(&board);
        assert_eq!(approved.state, OrderState::Approved);
        assert_eq!(approved.note.as_deref(), Some("Go."));
        assert_eq!(approved.decided_at, Some(at(10, 1)));

        owner(
            &board,
            2,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        board.session(
            at(10, 3),
            Some("FRK-1"),
            "ivo",
            "session-2",
            "purchase_order.updated",
            json!({ "order": 1, "status": "shipped", "note": "Left the depot.", "expected_on": "2026-10-02" }),
        );
        owner(
            &board,
            4,
            "purchase_order.received",
            json!({
                "order": 1, "received_on": "2026-10-01", "paid": "1450.00", "currency": "EUR",
                "renews_on": "2027-10-01"
            }),
        );
        let received = only(&board);
        assert_eq!(received.state, OrderState::Received);
        assert_eq!(received.placed_on, Some(day("2026-09-28")));
        assert_eq!(received.received_on, Some(day("2026-10-01")));
        assert_eq!(received.renews_on, Some(day("2027-10-01")));
        assert_eq!(
            received.paid,
            Some(("1450.00".to_string(), "EUR".to_string()))
        );
        assert_eq!(received.ended_at, Some(at(10, 4)));
        let status = received.status.expect("the agent's status is kept");
        assert_eq!(status.status, FollowUp::Shipped);
        assert_eq!(status.note, "Left the depot.");
        assert_eq!(status.expected_on, Some(day("2026-10-02")));
        assert!(!status.by_owner);
        assert_eq!(
            expires_at(&only(&board)),
            None,
            "a received order never expires"
        );
    }

    #[test]
    fn a_step_that_names_an_agent_or_a_session_changes_nothing() {
        let board = Board::new("orders-forged");
        draft(&board, 0, 1, "Acme");
        // The agent cannot approve its own order, whatever the envelope's other fields say.
        board.session(
            at(10, 1),
            Some("FRK-1"),
            "ivo",
            "session-1",
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        board.put(
            at(10, 2),
            Some("FRK-1"),
            Some("ivo"),
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        board.put_with(
            at(10, 3),
            Some("FRK-1"),
            None,
            Some("session-1"),
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        assert_eq!(only(&board).state, OrderState::Drafted);

        owner(
            &board,
            4,
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        // Placed, received, closed and expired each need the owner's envelope too.
        let by_agent = |minute, kind: &str, body: Value| {
            board.session(
                at(10, minute),
                Some("FRK-1"),
                "ivo",
                "session-1",
                kind,
                body,
            );
        };
        by_agent(
            5,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        assert_eq!(only(&board).state, OrderState::Approved);
        owner(
            &board,
            6,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        by_agent(
            7,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-01" }),
        );
        by_agent(
            8,
            "purchase_order.closed",
            json!({ "order": 1, "note": "" }),
        );
        by_agent(9, "purchase_order.expired", json!({ "order": 1 }));
        let placed = only(&board);
        assert_eq!(placed.state, OrderState::Placed);
        assert_eq!(placed.received_on, None);
        assert_eq!(placed.paid, None);
    }

    #[test]
    fn a_step_the_state_does_not_take_changes_nothing() {
        let board = Board::new("orders-states");
        draft(&board, 0, 1, "Acme");
        // A drafted order is not received, placed, closed or corrected.
        owner(
            &board,
            1,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-01" }),
        );
        owner(
            &board,
            2,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        owner(
            &board,
            3,
            "purchase_order.closed",
            json!({ "order": 1, "note": "" }),
        );
        assert_eq!(only(&board).state, OrderState::Drafted);
        // The first decision is the only one.
        owner(
            &board,
            4,
            "purchase_order.approved",
            json!({ "order": 1, "note": "first" }),
        );
        owner(
            &board,
            5,
            "purchase_order.rejected",
            json!({ "order": 1, "note": "second" }),
        );
        let approved = only(&board);
        assert_eq!(approved.state, OrderState::Approved);
        assert_eq!(approved.note.as_deref(), Some("first"));
        // An approved order is not received or closed, and a placed one is not placed again.
        owner(
            &board,
            6,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-01" }),
        );
        owner(
            &board,
            7,
            "purchase_order.closed",
            json!({ "order": 1, "note": "" }),
        );
        assert_eq!(only(&board).state, OrderState::Approved);
        owner(
            &board,
            8,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        owner(
            &board,
            9,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-30", "paid": "9.00" }),
        );
        let placed = only(&board);
        assert_eq!(placed.placed_on, Some(day("2026-09-28")));
        assert_eq!(placed.paid, None);
        // A placed order never expires, and a second drafting of its number is no order.
        owner(&board, 14, "purchase_order.expired", json!({ "order": 1 }));
        draft(&board, 15, 1, "Another seller");
        let placed = only(&board);
        assert_eq!(placed.state, OrderState::Placed);
        assert_eq!(placed.drafted.seller.as_str(), "Acme");
        // A closed order stays closed, and an expiry after it changes nothing.
        owner(
            &board,
            10,
            "purchase_order.closed",
            json!({ "order": 1, "note": "It was lost." }),
        );
        owner(
            &board,
            11,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-03" }),
        );
        owner(&board, 12, "purchase_order.expired", json!({ "order": 1 }));
        let closed = only(&board);
        assert_eq!(closed.state, OrderState::Closed);
        assert_eq!(closed.close_note.as_deref(), Some("It was lost."));
        assert_eq!(closed.ended_at, Some(at(10, 10)));
        // An event for an order nobody drafted is no order.
        owner(
            &board,
            13,
            "purchase_order.approved",
            json!({ "order": 9, "note": "" }),
        );
        assert_eq!(purchase_orders(&board.log).expect("reads").len(), 1);
    }

    #[test]
    fn a_rejected_or_expired_order_ends() {
        let board = Board::new("orders-ends");
        draft(&board, 0, 1, "Acme");
        draft(&board, 1, 2, "Bolt");
        draft(&board, 2, 3, "Cog");
        owner(
            &board,
            3,
            "purchase_order.rejected",
            json!({ "order": 1, "note": "Too dear." }),
        );
        owner(&board, 4, "purchase_order.expired", json!({ "order": 2 }));
        owner(
            &board,
            5,
            "purchase_order.approved",
            json!({ "order": 3, "note": "" }),
        );
        owner(&board, 6, "purchase_order.expired", json!({ "order": 3 }));
        // An expired order takes no step.
        owner(
            &board,
            7,
            "purchase_order.approved",
            json!({ "order": 2, "note": "" }),
        );
        owner(
            &board,
            8,
            "purchase_order.placed",
            json!({ "order": 3, "placed_on": "2026-09-28" }),
        );
        let all = purchase_orders(&board.log).expect("reads");
        let states: Vec<_> = all
            .iter()
            .map(|record| (record.order, record.state))
            .collect();
        assert_eq!(
            states,
            [
                (1, OrderState::Rejected),
                (2, OrderState::Expired),
                (3, OrderState::Expired)
            ]
        );
        assert_eq!(all[0].note.as_deref(), Some("Too dear."));
        assert_eq!(all[2].note, None, "no words is no note");
        assert_eq!(all[0].ended_at, Some(at(10, 3)));
        assert_eq!(all[1].ended_at, Some(at(10, 4)));
        assert_eq!(all[2].decided_at, Some(at(10, 5)));
        assert_eq!(all[2].ended_at, Some(at(10, 6)));
    }

    #[test]
    fn a_status_is_its_agent_s_or_the_owner_s() {
        let board = Board::new("orders-status");
        draft(&board, 0, 1, "Acme");
        let by = |who: &str, minute: u32, status: &str, note: &str| {
            let body = json!({ "order": 1, "status": status, "note": note });
            if who == "owner" {
                owner(&board, minute, "purchase_order.updated", body);
            } else {
                board.session(
                    at(10, minute),
                    Some("FRK-1"),
                    who,
                    "session-9",
                    "purchase_order.updated",
                    body,
                );
            }
        };
        // Not placed yet: nobody's status counts.
        by("ivo", 1, "preparing", "");
        by("owner", 2, "preparing", "");
        owner(
            &board,
            3,
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        by("ivo", 4, "preparing", "");
        assert!(only(&board).status.is_none());

        owner(
            &board,
            5,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        assert!(only(&board).status.is_none());
        by("ivo", 6, "preparing", "Packing it.");
        let agent = only(&board).status.expect("the agent's status");
        assert_eq!((agent.status, agent.by_owner), (FollowUp::Preparing, false));
        assert_eq!(agent.at, at(10, 6));

        // Another agent's follow-up changes nothing.
        by("kai", 7, "problem", "Out of stock.");
        assert_eq!(
            only(&board).status.expect("kept").status,
            FollowUp::Preparing
        );

        // The owner's correction replaces it, and the agent's next follow-up replaces that.
        by("owner", 8, "delayed", "The seller called.");
        let corrected = only(&board).status.expect("corrected");
        assert_eq!(
            (corrected.status, corrected.by_owner),
            (FollowUp::Delayed, true)
        );
        assert_eq!(corrected.note, "The seller called.");
        by("ivo", 9, "shipped", "On its way.");
        let latest = only(&board).status.expect("latest");
        assert_eq!((latest.status, latest.by_owner), (FollowUp::Shipped, false));

        // After it was received, none counts.
        owner(
            &board,
            10,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-01" }),
        );
        by("ivo", 11, "problem", "Late.");
        by("owner", 12, "problem", "Late.");
        let received = only(&board);
        assert_eq!(received.state, OrderState::Received);
        assert_eq!(received.status.expect("kept").status, FollowUp::Shipped);
    }

    #[test]
    fn a_status_that_names_an_agent_or_a_session_alone_is_not_the_owner_s() {
        let board = Board::new("orders-status-alone");
        draft(&board, 0, 1, "Acme");
        owner(
            &board,
            1,
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        owner(
            &board,
            2,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        let problem = json!({ "order": 1, "status": "problem", "note": "Out of stock." });

        // Another agent's, with no session: not the owner's correction, and not its agent's.
        board.put(
            at(10, 3),
            Some("FRK-1"),
            Some("kai"),
            "purchase_order.updated",
            problem.clone(),
        );
        // A session with no agent: nothing the owner's command records.
        board.put_with(
            at(10, 4),
            Some("FRK-1"),
            None,
            Some("session-9"),
            "purchase_order.updated",
            problem,
        );
        assert!(only(&board).status.is_none(), "neither counts");

        // The order's own agent with no session is the agent's, never the owner's.
        board.put(
            at(10, 5),
            Some("FRK-1"),
            Some("ivo"),
            "purchase_order.updated",
            json!({ "order": 1, "status": "shipped", "note": "" }),
        );
        let status = only(&board).status.expect("the agent's status counts");
        assert_eq!(status.status, FollowUp::Shipped);
        assert!(!status.by_owner);
    }

    #[test]
    fn a_later_amount_paid_replaces_the_first() {
        let board = Board::new("orders-paid");
        for (order, minute) in [(1, 0), (2, 1)] {
            draft(
                &board,
                minute,
                order,
                if order == 1 { "Acme" } else { "Bolt" },
            );
            owner(
                &board,
                minute + 2,
                "purchase_order.approved",
                json!({ "order": order, "note": "" }),
            );
            owner(
                &board,
                minute + 4,
                "purchase_order.placed",
                json!({ "order": order, "placed_on": "2026-09-28", "paid": "100.00", "currency": "USD" }),
            );
        }
        let paid = |board: &Board, order: usize| {
            purchase_orders(&board.log).expect("reads")[order]
                .paid
                .clone()
        };
        assert_eq!(
            paid(&board, 0),
            Some(("100.00".to_string(), "USD".to_string()))
        );
        // A received step with no amount keeps what was paid at placing; one with an amount
        // replaces it.
        owner(
            &board,
            10,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-10-01" }),
        );
        owner(
            &board,
            11,
            "purchase_order.received",
            json!({ "order": 2, "received_on": "2026-10-01", "paid": "120.00", "currency": "EUR" }),
        );
        assert_eq!(
            paid(&board, 0),
            Some(("100.00".to_string(), "USD".to_string()))
        );
        assert_eq!(
            paid(&board, 1),
            Some(("120.00".to_string(), "EUR".to_string()))
        );
        // An amount with no currency beside it is in the order's.
        draft(&board, 20, 3, "Cog");
        owner(
            &board,
            21,
            "purchase_order.approved",
            json!({ "order": 3, "note": "" }),
        );
        owner(
            &board,
            22,
            "purchase_order.placed",
            json!({ "order": 3, "placed_on": "2026-09-28", "paid": "5.00" }),
        );
        assert_eq!(
            paid(&board, 2),
            Some(("5.00".to_string(), "USD".to_string()))
        );
    }

    #[test]
    fn an_order_expires_thirty_days_after_its_drafting_or_approval() {
        let board = Board::new("orders-expiry");
        draft(&board, 0, 1, "Acme");
        draft(&board, 1, 2, "Bolt");
        owner(
            &board,
            2,
            "purchase_order.approved",
            json!({ "order": 2, "note": "" }),
        );
        let all = purchase_orders(&board.log).expect("reads");
        assert_eq!(expires_at(&all[0]), Some(at(10, 0) + Duration::days(30)));
        assert_eq!(expires_at(&all[1]), Some(at(10, 2) + Duration::days(30)));
        owner(
            &board,
            3,
            "purchase_order.placed",
            json!({ "order": 2, "placed_on": "2026-09-28" }),
        );
        let placed = purchase_orders(&board.log).expect("reads");
        assert_eq!(expires_at(&placed[1]), None, "a placed order never expires");
    }

    #[test]
    fn a_placed_order_is_overdue_once_its_expected_day_has_passed() {
        let board = Board::new("orders-overdue");
        draft(&board, 0, 1, "Acme");
        owner(
            &board,
            1,
            "purchase_order.approved",
            json!({ "order": 1, "note": "" }),
        );
        assert!(!overdue(&only(&board), day("2030-01-01")), "not placed");
        owner(
            &board,
            2,
            "purchase_order.placed",
            json!({ "order": 1, "placed_on": "2026-09-28" }),
        );
        // With no status, the expected day is 30 days after it was placed.
        let record = only(&board);
        assert!(!overdue(&record, day("2026-10-28")));
        assert!(overdue(&record, day("2026-10-29")));
        // A status that gives a day replaces it.
        board.session(
            at(10, 3),
            Some("FRK-1"),
            "ivo",
            "session-2",
            "purchase_order.updated",
            json!({ "order": 1, "status": "delayed", "note": "Short of flour.", "expected_on": "2026-11-10" }),
        );
        let delayed = only(&board);
        assert!(!overdue(&delayed, day("2026-11-10")));
        assert!(overdue(&delayed, day("2026-11-11")));
        // A status with no day leaves the 30 days.
        board.session(
            at(10, 4),
            Some("FRK-1"),
            "ivo",
            "session-2",
            "purchase_order.updated",
            json!({ "order": 1, "status": "problem", "note": "Out of stock." }),
        );
        let problem = only(&board);
        assert!(overdue(&problem, day("2026-10-29")));
        // Received, it is no longer overdue.
        owner(
            &board,
            5,
            "purchase_order.received",
            json!({ "order": 1, "received_on": "2026-11-12" }),
        );
        assert!(!overdue(&only(&board), day("2027-01-01")));
    }
}
