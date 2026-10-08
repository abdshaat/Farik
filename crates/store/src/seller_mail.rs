//! The messages the Procurement Specialist drafts to sellers and the replies it receives
//! (`docs/SPEC.md` 6.10), folded from the eight `mailbox.`, `seller_message.` and `seller_reply.`
//! kinds of the project's log. A draft is the agent's; sending, failing, discarding, receiving and
//! dismissing count only from an envelope that names no agent and no session, so that no agent can
//! make any of them happen by recording it.

use chrono::{DateTime, NaiveDate, Utc};
use farik_core::contract::TaskId;
use farik_protocol::event::{
    EventBody, EventKind, FarikEvent, SellerMessageDraftedBody, SellerMessagePurpose,
    SellerMessageSentBody, SellerReplyReceivedBody,
};

use crate::purchase_orders::{OrderState, purchase_orders};
use crate::{EventLog, EventQuery, StoreError};

/// Where a message to a seller stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageState {
    /// Drafted, and not sent or discarded; it may have failed to send and be tried again.
    Waiting,
    /// Sent on the owner's press.
    Sent,
    /// Discarded by the owner.
    Discarded,
    /// An order's message whose order was decided without it (rejected, expired, or approved
    /// alone): nothing is sent.
    Closed,
}

impl MessageState {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Sent => "sent",
            Self::Discarded => "discarded",
            Self::Closed => "closed",
        }
    }
}

/// One message to a seller, as the log tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct SellerMessageRecord {
    /// The message's number (`mail/out/<n>.txt`).
    pub message: u64,
    /// The task whose agent drafted it.
    pub task_id: TaskId,
    /// The agent that drafted it.
    pub agent_id: String,
    /// What the agent wrote of it. Untrusted.
    pub drafted: SellerMessageDraftedBody,
    /// When it was drafted.
    pub drafted_at: DateTime<Utc>,
    /// Where it stands.
    pub state: MessageState,
    /// Farik's sentence about the last try that failed, while the message waits.
    pub why: Option<String>,
    /// The send, once it is sent.
    pub sent: Option<SellerMessageSentBody>,
    /// When it was sent.
    pub sent_at: Option<DateTime<Utc>>,
}

/// One reply from a seller, as the log tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct SellerReplyRecord {
    /// The reply's number (`mail/in/<yyyy-mm>/<n>/`).
    pub reply: u64,
    /// The task of the message it answers.
    pub task_id: TaskId,
    /// What Farik read. Untrusted.
    pub received: SellerReplyReceivedBody,
    /// When Farik read it.
    pub received_at: DateTime<Utc>,
    /// Whether the owner dismissed it on Today.
    pub dismissed: bool,
}

/// Everything the log holds of the procurement mailbox, its messages and its replies.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SellerMail {
    /// The address of the connected procurement mailbox; none when none is connected.
    pub address: Option<String>,
    /// Every message, oldest first.
    pub messages: Vec<SellerMessageRecord>,
    /// Every reply, oldest first.
    pub replies: Vec<SellerReplyRecord>,
}

/// Whether `event` was recorded by the owner or by Farik: its envelope names no agent and no
/// session.
fn is_unattended(event: &FarikEvent) -> bool {
    event.envelope.ids.agent_id.is_none() && event.envelope.ids.session_id.is_none()
}

fn waiting(mail: &mut SellerMail, message: u64) -> Option<&mut SellerMessageRecord> {
    mail.messages
        .iter_mut()
        .find(|record| record.message == message && record.state == MessageState::Waiting)
}

/// Every message and reply the log holds, oldest first. A draft counts when its envelope names the
/// task and the agent; a failure, a send, a discard, a reply and a dismissal count only from an
/// envelope naming no agent and no session, and a send, a failure or a discard only while the
/// message waits. A reply counts when it answers a message the log holds. A waiting message of an
/// order that is not `drafted` any more is closed, unless it was sent.
///
/// # Errors
///
/// What the log refused.
pub fn seller_mail(log: &EventLog) -> Result<SellerMail, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::MailboxConnected,
            EventKind::MailboxDisconnected,
            EventKind::SellerMessageDrafted,
            EventKind::SellerMessageSent,
            EventKind::SellerMessageFailed,
            EventKind::SellerMessageDiscarded,
            EventKind::SellerReplyReceived,
            EventKind::SellerReplyDismissed,
        ],
        ..EventQuery::default()
    })?;
    let mut mail = SellerMail::default();
    for event in &events {
        let at = event.envelope.recorded_at;
        match &event.body {
            EventBody::MailboxConnected(body) if is_unattended(event) => {
                mail.address = Some(body.address.to_string());
            }
            EventBody::MailboxDisconnected(_) if is_unattended(event) => mail.address = None,
            EventBody::SellerMessageDrafted(body) => {
                let (Some(task_id), Some(agent_id)) = (
                    event.envelope.ids.task_id.clone(),
                    event.envelope.ids.agent_id.clone(),
                ) else {
                    continue;
                };
                if mail
                    .messages
                    .iter()
                    .all(|record| record.message != body.message.get())
                {
                    mail.messages.push(SellerMessageRecord {
                        message: body.message.get(),
                        task_id,
                        agent_id,
                        drafted: body.clone(),
                        drafted_at: at,
                        state: MessageState::Waiting,
                        why: None,
                        sent: None,
                        sent_at: None,
                    });
                }
            }
            EventBody::SellerMessageFailed(body) if is_unattended(event) => {
                if let Some(record) = waiting(&mut mail, body.message.get()) {
                    record.why = Some(body.why.to_string());
                }
            }
            EventBody::SellerMessageSent(body) if is_unattended(event) => {
                if let Some(record) = waiting(&mut mail, body.message.get()) {
                    record.state = MessageState::Sent;
                    record.why = None;
                    record.sent = Some(body.clone());
                    record.sent_at = Some(at);
                }
            }
            EventBody::SellerMessageDiscarded(body) if is_unattended(event) => {
                if let Some(record) = waiting(&mut mail, body.message.get()) {
                    record.state = MessageState::Discarded;
                }
            }
            EventBody::SellerReplyReceived(body) if is_unattended(event) => {
                let task = mail
                    .messages
                    .iter()
                    .find(|record| record.message == body.message.get())
                    .map(|record| record.task_id.clone());
                if let Some(task_id) = task
                    && mail
                        .replies
                        .iter()
                        .all(|record| record.reply != body.reply.get())
                {
                    mail.replies.push(SellerReplyRecord {
                        reply: body.reply.get(),
                        task_id,
                        received: body.clone(),
                        received_at: at,
                        dismissed: false,
                    });
                }
            }
            EventBody::SellerReplyDismissed(body) if is_unattended(event) => {
                if let Some(record) = mail
                    .replies
                    .iter_mut()
                    .find(|record| record.reply == body.reply.get())
                {
                    record.dismissed = true;
                }
            }
            _ => {}
        }
    }
    close_orders_messages(log, &mut mail)?;
    Ok(mail)
}

/// Closes each waiting message of an order the owner decided without it.
fn close_orders_messages(log: &EventLog, mail: &mut SellerMail) -> Result<(), StoreError> {
    if mail.messages.iter().all(|record| {
        record.state != MessageState::Waiting
            || record.drafted.purpose != SellerMessagePurpose::PurchaseOrder
    }) {
        return Ok(());
    }
    let orders = purchase_orders(log)?;
    for record in &mut mail.messages {
        if record.state != MessageState::Waiting
            || record.drafted.purpose != SellerMessagePurpose::PurchaseOrder
        {
            continue;
        }
        let order = record.drafted.purchase_order.as_ref().map(|n| n.get());
        let open = orders.iter().any(|candidate| {
            Some(candidate.order) == order && candidate.state == OrderState::Drafted
        });
        if !open {
            record.state = MessageState::Closed;
        }
    }
    Ok(())
}

/// How many messages were sent on `day` (UTC), the order's among them: what the daily limit
/// counts.
#[must_use]
pub fn sent_on(mail: &SellerMail, day: NaiveDate) -> u32 {
    let count = mail
        .messages
        .iter()
        .filter(|record| record.sent_at.is_some_and(|at| at.date_naive() == day))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::{Value, json};

    use super::{MessageState, seller_mail, sent_on};
    use crate::waiting::fixtures::{Board, at};

    fn draft_body(message: u64, purpose: &str, order: Option<u64>) -> Value {
        let mut body = json!({
            "message": message,
            "seller": "Pie Box Pros",
            "to": "sales@pieboxpros.test",
            "subject": "Quote for 500 printed pie boxes",
            "purpose": purpose,
            "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c"
        });
        if let Some(order) = order {
            body["purchase_order"] = json!(order);
        }
        body
    }

    /// Ivo drafts message `message` on FRK-1, in his session.
    fn draft(board: &Board, minute: u32, message: u64, purpose: &str, order: Option<u64>) {
        board.session(
            at(10, minute),
            Some("FRK-1"),
            "ivo",
            "session-1",
            "seller_message.drafted",
            draft_body(message, purpose, order),
        );
    }

    /// The founder's or Farik's step: no task, no agent, no session.
    fn unattended(board: &Board, minute: u32, kind: &str, body: Value) {
        board.put(at(10, minute), None, None, kind, body);
    }

    fn sent_body(message: u64) -> Value {
        json!({
            "message": message,
            "message_id": "0b9d6f7e-1c2a-4f3b-8a5d-6e7f8091a2b3@bakery.test",
            "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c",
            "edited": true
        })
    }

    fn reply_body(reply: u64, message: u64) -> Value {
        json!({
            "reply": reply, "message": message,
            "from": "Dana <sales@pieboxpros.test>", "subject": "Re: Quote",
            "attachments": []
        })
    }

    fn drafted_order(board: &Board, minute: u32, order: u64) {
        board.session(
            at(10, minute),
            Some("FRK-1"),
            "ivo",
            "session-1",
            "purchase_order.drafted",
            json!({
                "order": order, "seller": "Pie Box Pros", "seller_contact": "",
                "lines": [{ "item": "Box", "quantity": 500, "unit": "piece",
                    "unit_price": "0.40", "line_total": "200.00" }],
                "currency": "USD", "period": "once", "total": "200.00",
                "delivery": "", "terms": "", "url": "", "evaluation": "evaluations/boxes.md",
                "why": "It is the cheapest seller that ships to us."
            }),
        );
    }

    #[test]
    fn folds_each_message() {
        let board = Board::new("folds-seller-mail");
        for message in 1..=4 {
            draft(&board, message, u64::from(message), "quote_request", None);
        }
        // 1: failed, then sent (edited). 2: discarded. 3: stays waiting. 4: sent by an agent.
        unattended(
            &board,
            10,
            "seller_message.failed",
            json!({ "message": 1, "why": "the mailbox did not accept its sign-in; connect it again" }),
        );
        let mail = seller_mail(&board.log).expect("folds");
        assert_eq!(mail.messages[0].state, MessageState::Waiting);
        assert_eq!(
            mail.messages[0].why.as_deref(),
            Some("the mailbox did not accept its sign-in; connect it again")
        );
        unattended(&board, 11, "seller_message.sent", sent_body(1));
        unattended(
            &board,
            12,
            "seller_message.discarded",
            json!({ "message": 2 }),
        );
        board.session(
            at(10, 13),
            None,
            "ivo",
            "session-1",
            "seller_message.sent",
            sent_body(4),
        );
        board.put(
            at(10, 14),
            None,
            Some("ivo"),
            "seller_message.discarded",
            json!({ "message": 3 }),
        );
        let mail = seller_mail(&board.log).expect("folds");
        let states: Vec<MessageState> = mail.messages.iter().map(|m| m.state).collect();
        assert_eq!(
            states,
            [
                MessageState::Sent,
                MessageState::Discarded,
                MessageState::Waiting,
                MessageState::Waiting
            ],
            "a send or a discard that names an agent or a session changes nothing"
        );
        let sent = mail.messages[0].sent.as_ref().expect("the send");
        assert!(sent.edited);
        assert!(
            mail.messages[0].why.is_none(),
            "a sent message has no failure"
        );
        assert_eq!(mail.messages[0].task_id.as_str(), "FRK-1");
        assert_eq!(mail.messages[0].agent_id, "ivo");

        // A failure a session records is not the mailbox's word: message 3 keeps no reason.
        board.session(
            at(10, 14),
            None,
            "ivo",
            "session-1",
            "seller_message.failed",
            json!({ "message": 3, "why": "an agent says so" }),
        );
        assert!(
            seller_mail(&board.log).expect("folds").messages[2]
                .why
                .is_none(),
            "a failure an agent recorded"
        );
    }

    #[test]
    fn folds_each_reply_and_the_days_count() {
        let board = Board::new("folds-seller-replies");
        draft(&board, 1, 1, "quote_request", None);
        unattended(&board, 11, "seller_message.sent", sent_body(1));
        // Replies: one counts from Farik and one from a session does not; one is dismissed.
        unattended(&board, 15, "seller_reply.received", reply_body(1, 1));
        board.session(
            at(10, 16),
            None,
            "ivo",
            "session-1",
            "seller_reply.received",
            reply_body(2, 1),
        );
        unattended(&board, 17, "seller_reply.received", reply_body(3, 99));
        board.session(
            at(10, 17),
            None,
            "ivo",
            "session-1",
            "seller_reply.dismissed",
            json!({ "reply": 1 }),
        );
        let mail = seller_mail(&board.log).expect("folds");
        assert_eq!(
            mail.replies.len(),
            1,
            "a reply to no sent message is no reply"
        );
        assert!(!mail.replies[0].dismissed, "a dismissal an agent recorded");
        unattended(&board, 18, "seller_reply.dismissed", json!({ "reply": 1 }));
        let mail = seller_mail(&board.log).expect("folds");
        assert!(mail.replies[0].dismissed);
        assert_eq!(mail.replies[0].task_id.as_str(), "FRK-1");

        // The day's count.
        let day = NaiveDate::from_ymd_opt(2026, 9, 28).expect("a day");
        assert_eq!(sent_on(&mail, day), 1);
        assert_eq!(sent_on(&mail, day.succ_opt().expect("next")), 0);
    }

    #[test]
    fn closes_an_order_message_whose_order_was_decided_without_it() {
        let board = Board::new("closes-order-message");
        drafted_order(&board, 1, 12);
        drafted_order(&board, 2, 13);
        draft(&board, 3, 1, "purchase_order", Some(12));
        draft(&board, 4, 2, "purchase_order", Some(13));
        board.put(
            at(10, 5),
            Some("FRK-1"),
            None,
            "purchase_order.rejected",
            json!({ "order": 12, "note": "" }),
        );
        let mail = seller_mail(&board.log).expect("folds");
        assert_eq!(mail.messages[0].state, MessageState::Closed);
        assert_eq!(mail.messages[1].state, MessageState::Waiting);
        // An order approved with its email: approved, sent, placed, in that order.
        board.put(
            at(10, 6),
            Some("FRK-1"),
            None,
            "purchase_order.approved",
            json!({ "order": 13, "note": "" }),
        );
        board.put(
            at(10, 7),
            Some("FRK-1"),
            None,
            "seller_message.sent",
            sent_body(2),
        );
        let mail = seller_mail(&board.log).expect("folds");
        assert_eq!(mail.messages[1].state, MessageState::Sent);
    }

    #[test]
    fn follows_the_mailbox_connected_and_disconnected() {
        let board = Board::new("folds-mailbox");
        assert_eq!(seller_mail(&board.log).expect("folds").address, None);
        board.session(
            at(10, 0),
            None,
            "ivo",
            "session-1",
            "mailbox.connected",
            json!({ "purpose": "procurement", "address": "evil@sellers.test" }),
        );
        assert_eq!(
            seller_mail(&board.log).expect("folds").address,
            None,
            "a connection an agent recorded"
        );
        unattended(
            &board,
            1,
            "mailbox.connected",
            json!({ "purpose": "procurement", "address": "buying@bakery.test" }),
        );
        assert_eq!(
            seller_mail(&board.log).expect("folds").address.as_deref(),
            Some("buying@bakery.test")
        );
        unattended(
            &board,
            2,
            "mailbox.disconnected",
            json!({ "purpose": "procurement" }),
        );
        assert_eq!(seller_mail(&board.log).expect("folds").address, None);
    }
}
