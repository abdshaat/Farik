//! Sending the messages the Procurement Specialist drafts, when the owner presses Send (step 10f,
//! `docs/SPEC.md` 6.10): the one path a message takes to a seller, and the lists Today shows. Only
//! the owner's command reaches it; no tool of an agent does. A send holds no lock across a server:
//! it claims its message (and its order), lets the locks go while it talks to the servers, and
//! takes them again to record, the claim refusing a second send of the same message meanwhile.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::PathBuf;
use std::sync::Mutex;

use catervas_core::contract::Role;
use catervas_core::governor::sites::site_of;
use catervas_core::team::private_folder;
use catervas_protocol::event::{
    EventBody, SellerMessageDiscardedBody, SellerMessageFailedBody, SellerMessagePurpose,
    SellerMessageSentBody,
};
use catervas_store::StoreError;
use catervas_store::seller_mail::{
    MessageState, SellerMail, SellerMessageRecord, seller_mail, sent_on,
};
use catervas_store::waiting::OrderSend;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{
    MAIL, MOST_SENT_A_DAY, MailboxRefusal, mail_dir, mail_failed, mailbox_settings,
    record_unattended, store_refusal,
};
use crate::claude::Secret;
use crate::connectors::ConnectorSecrets;
use crate::mailbox::{
    MailboxAt, MailboxError, MailboxSecrets, MailboxSettings, Outgoing, Trust, send,
};
use crate::tools::ToolDeps;
use crate::tools::seller::{body_fault, hex, subject_fault};

/// The line Catervas adds under the signature when the owner has it on.
fn disclosure(name: &str) -> String {
    format!("Written with an AI assistant and sent by {name} after reading it.")
}

/// The text a message goes out with: the body the owner read, a blank line and the signature when
/// there is one, and, when the disclosure is on, a blank line and the line saying an AI assistant
/// wrote it and the owner read it.
#[must_use]
pub(crate) fn compose(settings: &MailboxSettings, body: &str) -> String {
    let mut text = body.to_string();
    if !settings.signature.trim().is_empty() {
        text.push_str("\n\n");
        text.push_str(settings.signature.trim());
    }
    if settings.disclose_ai {
        text.push_str("\n\n");
        text.push_str(&disclosure(&settings.name));
    }
    text
}

/// A send in flight: which project, which message, and which order when it is an order's.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Claim {
    root: PathBuf,
    message: u64,
    order: Option<u64>,
}

/// The sends in flight in this process.
static SENDING: Mutex<Vec<Claim>> = Mutex::new(Vec::new());

/// Gives a claim back when it is dropped, however the send ended, even when the command is cut
/// off with its client.
struct Held(Claim);

impl Drop for Held {
    fn drop(&mut self) {
        crate::locked(&SENDING).retain(|claim| *claim != self.0);
    }
}

/// Whether the email of order `order` is on its way in this project: the order is then not decided
/// or expired until it has gone or failed.
#[must_use]
pub(crate) fn order_is_sending(deps: &ToolDeps, order: u64) -> bool {
    let root = deps.files.root();
    crate::locked(&SENDING)
        .iter()
        .any(|claim| claim.order == Some(order) && claim.root == root)
}

/// Where the mailbox's password is and how its certificates are trusted: what a send needs beside
/// the project.
pub(crate) struct Mailer<'a> {
    /// Where the connectors' keys, and with them the mailbox's password, are kept.
    pub(crate) secrets: &'a dyn ConnectorSecrets,
    /// Which password this project's mailbox is.
    pub(crate) at: MailboxAt,
    /// The certificates trusted: the platform's, always, but in a test.
    pub(crate) trust: Trust,
}

/// What the owner asks to send: the message with the subject and body they saw, and the order when
/// the message is its.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendAsk<'a> {
    pub(crate) message: u64,
    pub(crate) subject: &'a str,
    pub(crate) body: &'a str,
    pub(crate) order: Option<u64>,
}

/// A send that passed every check and holds its claim: what the servers are given, and what is
/// recorded when they answer. Dropping it gives the claim back.
pub(crate) struct Prepared {
    _held: Held,
    settings: MailboxSettings,
    password: Secret,
    trust: Trust,
    record: SellerMessageRecord,
    /// The subject and the body as the owner sent them, and the text with what Catervas adds.
    subject: String,
    text: String,
    message_id: String,
    edited: bool,
    attachment: Option<(String, Vec<u8>)>,
}

impl Prepared {
    /// The seller's name, for the sentence the owner reads back.
    #[must_use]
    pub(crate) fn seller(&self) -> String {
        crate::tools::sites::shown(self.record.drafted.seller.as_str())
    }
}

fn refusal(code: &'static str, words: impl Into<String>) -> MailboxRefusal {
    MailboxRefusal {
        code,
        words: words.into(),
    }
}

/// `(code, words)` of a field's fault, as a refusal.
fn fault((code, words): (&'static str, String)) -> MailboxRefusal {
    refusal(code, words)
}

/// 32 random hex digits formed into a version 4 uuid, as `local_project_id` reads its random
/// bytes: from the system's source.
fn new_uuid() -> std::io::Result<String> {
    use std::io::Read as _;
    let mut bytes = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex(&bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// The draft file `mail/out/<n>.txt`, as the agent wrote it.
pub(crate) fn draft_text(deps: &ToolDeps, message: u64) -> String {
    mail_dir(deps)
        .ok()
        .and_then(|dir| {
            std::fs::read_to_string(dir.join("out").join(format!("{message}.txt"))).ok()
        })
        .unwrap_or_default()
}

/// The message `number` of `mail`, or why not.
fn message_of(mail: &SellerMail, number: u64) -> Result<&SellerMessageRecord, MailboxRefusal> {
    mail.messages
        .iter()
        .find(|record| record.message == number)
        .ok_or_else(|| {
            refusal(
                "unknown_seller_message",
                format!("{number} is no message the Procurement Specialist drafted"),
            )
        })
}

/// A refusal for a message that does not wait.
fn not_waiting(record: &SellerMessageRecord) -> Option<MailboxRefusal> {
    match record.state {
        MessageState::Waiting => None,
        MessageState::Sent => Some(refusal(
            "seller_message_sent",
            "This message was sent already.",
        )),
        MessageState::Discarded => Some(refusal(
            "seller_message_discarded",
            "This message was discarded.",
        )),
        MessageState::Closed => Some(refusal(
            "seller_message_closed",
            "This message went with an order that was decided without it, and is not sent.",
        )),
    }
}

/// The workbook of order `order`, `orders/PO-<order>.xlsx`, as the attachment of its email.
fn workbook_of(deps: &ToolDeps, order: u64) -> Result<(String, Vec<u8>), MailboxRefusal> {
    let missing = || {
        refusal(
            "purchase_order_workbook_missing",
            format!("PO-{order}\u{2019}s workbook is gone, so the order cannot be emailed."),
        )
    };
    let folder = private_folder(Role::ProcurementSpecialist).ok_or_else(missing)?;
    let name = format!("PO-{order}.xlsx");
    let path =
        crate::tools::sheets::private_path(deps.files.root(), folder, &format!("orders/{name}"))
            .map_err(|_| missing())?;
    let bytes = std::fs::read(path).map_err(|_| missing())?;
    Ok((name, bytes))
}

/// Checks a send and claims it: the message exists and waits, is an order's only when it is sent
/// by its order, the mailbox is connected, the subject and body fit, the day's limit holds, and no
/// other send of the message or the order is on its way. Nothing is written or recorded; the
/// locks are let go before this returns, and the claim stays until the answer is dropped.
///
/// # Errors
///
/// `unknown_seller_message`, `seller_message_sent` (also for one on its way),
/// `seller_message_discarded`, `seller_message_closed`, `seller_message_is_an_order`,
/// `seller_message_not_this_order`, `mailbox_not_connected`, `seller_message_field_invalid`,
/// `seller_message_too_long`, `seller_send_limit`, `purchase_order_workbook_missing`.
pub(crate) fn prepare(
    deps: &ToolDeps,
    mailer: &Mailer<'_>,
    ask: SendAsk<'_>,
) -> Result<Prepared, MailboxRefusal> {
    let _held = crate::locked(&MAIL);
    let mail = seller_mail(&deps.log).map_err(mail_failed)?;
    let record = message_of(&mail, ask.message)?;
    if let Some(refused) = not_waiting(record) {
        return Err(refused);
    }
    let is_order = record.drafted.purpose == SellerMessagePurpose::PurchaseOrder;
    match ask.order {
        None if is_order => {
            return Err(refusal(
                "seller_message_is_an_order",
                "Send this message from its order.",
            ));
        }
        Some(order)
            if !(is_order
                && record.drafted.purchase_order.as_ref().map(|n| n.get()) == Some(order)) =>
        {
            return Err(refusal(
                "seller_message_not_this_order",
                "This message is not this order\u{2019}s.",
            ));
        }
        _ => {}
    }
    let not_connected = || {
        refusal(
            "mailbox_not_connected",
            "Connect a procurement mailbox first.",
        )
    };
    let settings = mailbox_settings(deps).ok_or_else(not_connected)?;
    let password = MailboxSecrets::load(mailer.secrets, &mailer.at)
        .map_err(store_refusal)?
        .ok_or_else(not_connected)?;
    let subject = subject_fault(ask.subject).map_err(fault)?;
    let body = body_fault(ask.body).map_err(fault)?;
    let attachment = ask
        .order
        .map(|order| workbook_of(deps, order))
        .transpose()?;

    let claim = Claim {
        root: deps.files.root().to_path_buf(),
        message: ask.message,
        order: ask.order,
    };
    let mut sending = crate::locked(&SENDING);
    if sending
        .iter()
        .any(|held| held.root == claim.root && held.message == claim.message)
    {
        return Err(refusal(
            "seller_message_sent",
            "This message is being sent.",
        ));
    }
    let on_their_way = sending
        .iter()
        .filter(|held| held.root == claim.root)
        .count();
    let today = sent_on(&mail, deps.clock.now().date_naive()) as usize;
    if today + on_their_way >= MOST_SENT_A_DAY as usize {
        return Err(refusal(
            "seller_send_limit",
            format!(
                "You have sent {MOST_SENT_A_DAY} messages to sellers today, the most Catervas sends \
                 in a day. Send the rest tomorrow."
            ),
        ));
    }
    let domain = settings
        .address
        .rsplit_once('@')
        .map_or("localhost", |(_, domain)| domain)
        .to_string();
    let message_id = format!("{}@{domain}", new_uuid().map_err(mail_failed)?);
    let edited =
        subject != record.drafted.subject.as_str() || body != draft_text(deps, ask.message).trim();
    let text = compose(&settings, &body);
    let record = record.clone();
    sending.push(claim.clone());
    drop(sending);
    Ok(Prepared {
        _held: Held(claim),
        settings,
        password,
        trust: mailer.trust.clone(),
        record,
        subject,
        text,
        message_id,
        edited,
        attachment,
    })
}

/// Hands the message to the sending server: the one place a message leaves Catervas, with no lock
/// held.
///
/// # Errors
///
/// What the servers refused.
pub(crate) async fn transmit(prepared: &Prepared) -> Result<(), MailboxError> {
    send(
        &prepared.settings,
        &prepared.password,
        &prepared.trust,
        &Outgoing {
            to: prepared.record.drafted.to.to_string(),
            subject: prepared.subject.clone(),
            text: prepared.text.clone(),
            message_id: prepared.message_id.clone(),
            attachment: prepared.attachment.clone(),
        },
    )
    .await
}

/// Keeps what was sent as `mail/out/<n>.sent.txt`, the subject line, a blank line and exactly the
/// text, and records `seller_message.sent` with the hash of that file; the envelope names the
/// message's task and no agent and no session. Answers the record's number.
///
/// # Errors
///
/// `mailbox_files` when the file or the record could not be written.
pub(crate) fn record_sent(deps: &ToolDeps, prepared: &Prepared) -> Result<u64, MailboxRefusal> {
    let _held = crate::locked(&MAIL);
    let out = mail_dir(deps)?.join("out");
    let number = prepared.record.message;
    let kept = format!("{}\n\n{}", prepared.subject, prepared.text);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(out.join(format!("{number}.sent.txt")))
        .and_then(|mut file| file.write_all(kept.as_bytes()))
        .map_err(mail_failed)?;
    let hash = hex(&Sha256::digest(kept.as_bytes()));
    let body = SellerMessageSentBody {
        message: std::num::NonZeroU64::new(number)
            .map(Into::into)
            .ok_or_else(|| mail_failed("a message is numbered from 1"))?,
        message_id: prepared
            .message_id
            .clone()
            .try_into()
            .map_err(mail_failed)?,
        sha256: hash.try_into().map_err(mail_failed)?,
        edited: prepared.edited,
    };
    record_unattended(
        deps,
        EventBody::SellerMessageSent(body),
        Some(prepared.record.task_id.clone()),
    )
    .map_err(mail_failed)
}

/// Records `seller_message.failed` with Catervas's sentence for `error`, and answers the refusal
/// the owner is shown; the message waits to be tried again.
pub(crate) fn record_failed(
    deps: &ToolDeps,
    prepared: &Prepared,
    error: &MailboxError,
) -> MailboxRefusal {
    let _held = crate::locked(&MAIL);
    let why = error.why();
    let recorded = std::num::NonZeroU64::new(prepared.record.message)
        .map(Into::into)
        .zip(why.clone().try_into().ok())
        .map(|(message, why)| SellerMessageFailedBody { message, why })
        .map(EventBody::SellerMessageFailed)
        .map(|body| record_unattended(deps, body, Some(prepared.record.task_id.clone())));
    if let Some(Err(failure)) = recorded {
        return mail_failed(failure);
    }
    refusal(
        "seller_message_failed",
        format!("The message was not sent, and is kept to try again: {why}."),
    )
}

/// Sends message `message` with the subject and body the owner saw: checks, sends, and records
/// `seller_message.sent` (or `.failed` when a server refuses it). Answers the sent record's
/// number and the seller's name.
///
/// # Errors
///
/// As [`prepare`], and `seller_message_failed` after a server refused the message.
pub(crate) async fn send_message(
    deps: &ToolDeps,
    mailer: &Mailer<'_>,
    message: u64,
    subject: &str,
    body: &str,
) -> Result<(u64, String), MailboxRefusal> {
    let prepared = prepare(
        deps,
        mailer,
        SendAsk {
            message,
            subject,
            body,
            order: None,
        },
    )?;
    match transmit(&prepared).await {
        Ok(()) => record_sent(deps, &prepared).map(|seq| (seq, prepared.seller())),
        Err(error) => Err(record_failed(deps, &prepared, &error)),
    }
}

/// Discards message `message`: records `seller_message.discarded`, nothing sent.
///
/// # Errors
///
/// `unknown_seller_message`, `seller_message_sent` (also while it is being sent),
/// `seller_message_discarded`, `seller_message_closed`.
pub(crate) fn discard_message(deps: &ToolDeps, message: u64) -> Result<u64, MailboxRefusal> {
    let _held = crate::locked(&MAIL);
    let mail = seller_mail(&deps.log).map_err(mail_failed)?;
    let record = message_of(&mail, message)?;
    if let Some(refused) = not_waiting(record) {
        return Err(refused);
    }
    let root = deps.files.root();
    if crate::locked(&SENDING)
        .iter()
        .any(|claim| claim.message == message && claim.root == root)
    {
        return Err(refusal(
            "seller_message_sent",
            "This message is being sent.",
        ));
    }
    let number = std::num::NonZeroU64::new(message)
        .map(Into::into)
        .ok_or_else(|| mail_failed("a message is numbered from 1"))?;
    record_unattended(
        deps,
        EventBody::SellerMessageDiscarded(SellerMessageDiscardedBody { message: number }),
        Some(record.task_id.clone()),
    )
    .map_err(mail_failed)
}

/// The domain of `address` in its ASCII form, as the site rules read it.
fn domain_of(address: &str) -> Option<String> {
    let (_, domain) = address.rsplit_once('@')?;
    site_of(&format!("https://{domain}/")).ok()
}

/// The text of a sent message: after the subject line and the blank line of `out/<n>.sent.txt`.
pub(crate) fn sent_parts(deps: &ToolDeps, message: u64) -> Option<(String, String)> {
    let dir = mail_dir(deps).ok()?;
    let kept = std::fs::read_to_string(dir.join("out").join(format!("{message}.sent.txt"))).ok()?;
    let (subject, text) = kept.split_once("\n\n")?;
    Some((subject.to_string(), text.to_string()))
}

/// Whether a message went to `domain` before: some sent message did.
fn went_to(mail: &SellerMail, domain: &str) -> bool {
    mail.messages.iter().any(|record| {
        record.state == MessageState::Sent
            && domain_of(record.drafted.to.as_str()).as_deref() == Some(domain)
    })
}

/// `seller_messages.list`: every message, oldest first, with its state and what the owner reads
/// before they send it: the address and its domain in ASCII, whether a message went to that domain
/// before, the subject and the body (the draft's, or what was sent), and why the last try failed.
///
/// # Errors
///
/// What the log refused.
pub fn seller_messages_list(deps: &ToolDeps) -> Result<Value, StoreError> {
    let mail = seller_mail(&deps.log)?;
    let time =
        |at: chrono::DateTime<chrono::Utc>| at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
    let messages: Vec<Value> = mail
        .messages
        .iter()
        .map(|record| {
            let drafted = &record.drafted;
            let to = drafted.to.to_string();
            let domain = domain_of(&to);
            let mut row = json!({
                "message": record.message,
                "state": record.state.as_str(),
                "seller": drafted.seller.as_str(),
                "to": to,
                "domain": domain,
                "new_domain": domain.as_deref().is_none_or(|domain| !went_to(&mail, domain)),
                "subject": drafted.subject.as_str(),
                "body": draft_text(deps, record.message),
                "purpose": drafted.purpose.to_string(),
                "task_id": record.task_id,
                "agent_id": record.agent_id,
                "drafted_at": time(record.drafted_at),
            });
            if let Some(order) = &drafted.purchase_order {
                row["purchase_order"] = json!(order.get());
            }
            if let (Some(sent), Some(at)) = (&record.sent, record.sent_at) {
                row["sent_at"] = json!(time(at));
                row["edited"] = json!(sent.edited);
                if let Some((subject, text)) = sent_parts(deps, record.message) {
                    row["subject"] = json!(subject);
                    row["body"] = json!(text);
                }
            }
            if let Some(why) = &record.why {
                row["why"] = json!(why);
            }
            if let Some(at) = record.failed_at {
                row["failed_at"] = json!(time(at));
            }
            row
        })
        .collect();
    Ok(json!({
        "messages": messages,
        "sent_today": sent_on(&mail, deps.clock.now().date_naive()),
        "cap": MOST_SENT_A_DAY,
    }))
}

/// Adds `send` to a waiting order's row: its message, the seller's address and domain, whether
/// a message went there before, and the subject and body the owner will send.
pub fn add_order_send_fields(row: &mut Value, deps: &ToolDeps, send: &OrderSend) {
    let domain = domain_of(&send.to);
    let went = seller_mail(&deps.log).is_ok_and(|mail| {
        domain
            .as_deref()
            .is_some_and(|domain| went_to(&mail, domain))
    });
    row["send"] = json!({
        "message": send.message,
        "to": send.to,
        "domain": domain,
        "new_domain": !went,
        "subject": send.subject,
        "body": draft_text(deps, send.message),
    });
}

#[cfg(test)]
mod tests {
    use catervas_protocol::command::Command;
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_store::purchase_orders::{OrderState, purchase_orders};
    use catervas_store::seller_mail::{MessageState, seller_mail};
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};

    use mail_parser::MimeHeaders as _;

    use super::{
        Mailer, SendAsk, add_order_send_fields, discard_message, prepare, seller_messages_list,
    };
    use crate::greenmail::BUYING;
    use crate::mailbox::{MailboxSecrets as _, Trust};
    use crate::orchestrator::fixtures::Harness;
    use crate::procurement::story::{DANA, Story, drafted, lf, orders_waiting, reason, secret};

    use crate::tools::seller::hex;

    const SUBJECT: &str = "Quote for 500 printed pie boxes";
    const BODY: &str = "Hello,\n\nCould you quote 500 printed pie boxes, 9 inch?\n\nThank you.";

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn send_delivers_exactly_what_the_founder_saw() {
        let story = Story::new("exact").await;
        let message = story.draft(SUBJECT, BODY);
        assert!(story.dana_has().is_empty(), "drafting sends nothing");
        let said = story.send(message, SUBJECT, BODY).await.expect("sent");
        assert!(said.contains("Pie Box Pros"), "{said}");

        let raw = story.dana_has();
        assert_eq!(raw.len(), 1);
        let parsed = mail_parser::MessageParser::default()
            .parse(raw[0].as_bytes())
            .expect("a message");
        let from = parsed.from().and_then(|from| from.first()).expect("a From");
        assert_eq!(from.name(), Some("Sam Ortiz"));
        assert_eq!(from.address(), Some("buying@bakery.test"));
        assert_eq!(parsed.subject(), Some(SUBJECT));
        let text = "Hello,\n\nCould you quote 500 printed pie boxes, 9 inch?\n\nThank you.\n\n\
                    Corner Bakery\n\nWritten with an AI assistant and sent by Sam Ortiz after \
                    reading it.";
        assert_eq!(lf(parsed.body_text(0).expect("a text").trim_end()), text);
        let id = parsed.message_id().expect("a Message-ID").to_string();
        assert!(id.ends_with("@bakery.test"), "{id}");

        // What was sent is kept whole, and the record names it and the Message-ID it went with.
        assert_eq!(story.out("1.sent.txt"), format!("{SUBJECT}\n\n{text}"));
        let sent = story.events(&[EventKind::SellerMessageSent]);
        assert_eq!(sent.len(), 1);
        let ids = &sent[0].envelope.ids;
        assert_eq!((&ids.agent_id, &ids.session_id), (&None, &None));
        let EventBody::SellerMessageSent(body) = &sent[0].body else {
            panic!("a send");
        };
        assert_eq!(body.message.get(), 1);
        assert_eq!(body.message_id.as_str(), id);
        assert_eq!(
            body.sha256.as_str(),
            hex(&Sha256::digest(story.out("1.sent.txt").as_bytes()))
        );
        assert!(!body.edited);
        let mail = seller_mail(&story.harness.project.deps.log).expect("folds");
        assert_eq!(mail.messages[0].state, MessageState::Sent);
        // The same message is not sent again.
        let again = story
            .send(message, SUBJECT, BODY)
            .await
            .expect_err("sent already");
        assert!(again.starts_with("seller_message_sent: "), "{again}");
        assert_eq!(story.dana_has().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn an_edited_message_sends_the_founders_text() {
        let story = Story::new("edited").await;
        let message = story.draft(SUBJECT, BODY);
        story
            .send(
                message,
                "Quote for 500 pie boxes",
                "Please quote 500 boxes.",
            )
            .await
            .expect("sent");
        let raw = story.dana_has();
        let parsed = mail_parser::MessageParser::default()
            .parse(raw[0].as_bytes())
            .expect("a message");
        assert_eq!(parsed.subject(), Some("Quote for 500 pie boxes"));
        assert!(
            lf(parsed.body_text(0).expect("a text").trim_end())
                .starts_with("Please quote 500 boxes.\n\nCorner Bakery")
        );
        assert!(
            story
                .out("1.sent.txt")
                .starts_with("Quote for 500 pie boxes\n\nPlease quote")
        );
        let EventBody::SellerMessageSent(body) =
            &story.events(&[EventKind::SellerMessageSent])[0].body
        else {
            panic!("a send");
        };
        assert!(body.edited, "the founder changed the text");
        // The draft the agent wrote is kept as it was.
        assert_eq!(story.out("1.txt"), BODY);

        // With the disclosure off there is no line saying an assistant wrote it.
        story.connect(false, BUYING.password).await;
        let second = story.draft(SUBJECT, BODY);
        story.send(second, SUBJECT, BODY).await.expect("sent");
        let text = story.out("2.sent.txt");
        assert!(text.contains("Corner Bakery"), "{text}");
        assert!(!text.contains("AI assistant"), "{text}");

        // A change to the body alone, or to the subject alone, is an edit; none is not.
        let edited_of = |message: u64| {
            story
                .events(&[EventKind::SellerMessageSent])
                .iter()
                .find_map(|event| match &event.body {
                    EventBody::SellerMessageSent(body) if body.message.get() == message => {
                        Some(body.edited)
                    }
                    _ => None,
                })
                .expect("a send")
        };
        assert!(!edited_of(2), "sent as drafted");
        let third = story.draft(SUBJECT, BODY);
        story
            .send(third, SUBJECT, "Quote 500, please.")
            .await
            .expect("sent");
        assert!(edited_of(third), "a body changed alone");
        let fourth = story.draft(SUBJECT, BODY);
        story
            .send(fourth, "Quote please", BODY)
            .await
            .expect("sent");
        assert!(edited_of(fourth), "a subject changed alone");
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_refused_send_keeps_the_draft() {
        let story = Story::new("refused").await;
        let message = story.draft(SUBJECT, BODY);
        // The provider changed the password after it was connected.
        story
            .store
            .save(&story.at, &secret("changed-since"))
            .expect("saved");
        let refused = story
            .send(message, SUBJECT, BODY)
            .await
            .expect_err("not sent");
        assert!(refused.starts_with("seller_message_failed: "), "{refused}");
        let failed = story.events(&[EventKind::SellerMessageFailed]);
        assert_eq!(failed.len(), 1);
        let EventBody::SellerMessageFailed(body) = &failed[0].body else {
            panic!("a failure");
        };
        assert_eq!(
            body.why.as_str(),
            "the mailbox did not accept its sign-in; connect it again"
        );
        assert_eq!(
            (
                &failed[0].envelope.ids.agent_id,
                &failed[0].envelope.ids.session_id
            ),
            (&None, &None)
        );
        let mail = seller_mail(&story.harness.project.deps.log).expect("folds");
        assert_eq!(mail.messages[0].state, MessageState::Waiting);
        assert_eq!(
            mail.messages[0].why.as_deref(),
            Some("the mailbox did not accept its sign-in; connect it again")
        );
        assert!(story.out("1.sent.txt").is_empty());
        assert!(story.dana_has().is_empty());
        // Connected again, the same message goes.
        story
            .store
            .save(&story.at, &secret(BUYING.password))
            .expect("saved");
        story
            .send(message, SUBJECT, BODY)
            .await
            .expect("sent this time");
        assert_eq!(story.dana_has().len(), 1);
        assert!(
            seller_mail(&story.harness.project.deps.log)
                .expect("folds")
                .messages[0]
                .why
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "each refusal, one after the other, in one project"
    )]
    async fn refusals_by_state() {
        // No fixture mail is sent here: the mailbox is a settings file and a password.
        let harness = Harness::with_procurement("send-refusals");
        harness.procurement_task("CTV-1", Some("in_progress"));
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let send = |message: u64| {
            orchestrator.handle(Command::SellerMessageSend {
                message,
                subject: SUBJECT.to_string(),
                body: BODY.to_string(),
            })
        };
        let draft = |input: Value| drafted(&harness.project, input);
        let quote = || {
            json!({ "seller": "Pie Box Pros", "to": DANA.address, "subject": SUBJECT,
                    "body": BODY, "purpose": "quote_request" })
        };
        // An unknown number.
        assert!(
            reason(send(99).await.expect_err("no such")).starts_with("unknown_seller_message: ")
        );
        // No mailbox connected yet.
        let first = draft(quote());
        assert!(
            reason(send(first).await.expect_err("no mailbox"))
                .starts_with("mailbox_not_connected: ")
        );
        // A discarded message, once and again.
        let report = orchestrator
            .handle(Command::SellerMessageDiscard { message: first })
            .await
            .expect("discarded");
        assert!(!report.events.is_empty());
        assert_eq!(
            harness.events(&[EventKind::SellerMessageDiscarded]).len(),
            1
        );
        assert!(
            reason(send(first).await.expect_err("discarded"))
                .starts_with("seller_message_discarded: ")
        );
        let again = orchestrator
            .handle(Command::SellerMessageDiscard { message: first })
            .await
            .expect_err("discarded already");
        assert!(reason(again).starts_with("seller_message_discarded: "));
        let unknown = orchestrator
            .handle(Command::SellerMessageDiscard { message: 99 })
            .await
            .expect_err("no such");
        assert!(reason(unknown).starts_with("unknown_seller_message: "));
        assert_eq!(
            harness.events(&[EventKind::SellerMessageDiscarded]).len(),
            1
        );
        // An order's message is sent from its order, never from here.
        harness.project.record_in(
            Some("proc"),
            Some("session-1"),
            "CTV-1",
            "purchase_order.drafted",
            &order_body(12),
        );
        let mut order_message = quote();
        order_message["purpose"] = json!("purchase_order");
        order_message["purchase_order"] = json!(12);
        let ordered = draft(order_message);
        assert!(
            reason(send(ordered).await.expect_err("an order's"))
                .starts_with("seller_message_is_an_order: ")
        );
        // The agent's tool discards nothing either: the function is not offered, and refuses.
        assert!(discard_message(&harness.project.deps, 12345).is_err());

        // With a mailbox connected (its settings and its password; no server is reached to refuse a
        // field), the subject the owner sends is checked as the draft's was: a control character the
        // schema lets through is refused, and nothing is recorded.
        let keys = std::sync::Arc::new(crate::connectors::MemoryConnectorSecrets::default());
        assert!(harness.daemon.set_connector_secrets(keys.clone()));
        let at = harness
            .daemon
            .mailbox_at(harness.project.deps.files.root())
            .expect("the project's id");
        keys.save(&at, &secret(BUYING.password)).expect("saved");
        let server = |port| crate::mailbox::Server {
            host: "localhost".to_string(),
            port,
            security: crate::mailbox::Security::Tls,
        };
        let settings = crate::mailbox::MailboxSettings {
            address: BUYING.address.to_string(),
            name: "Sam Ortiz".to_string(),
            provider: crate::mailbox::Provider::Other,
            imap: server(993),
            smtp: server(465),
            username: BUYING.login.to_string(),
            folder: "INBOX".to_string(),
            signature: String::new(),
            disclose_ai: true,
        };
        std::fs::write(
            harness.procurement_folder().join("mail/mailbox.json"),
            serde_json::to_string(&settings).expect("settings"),
        )
        .expect("kept");
        let waiting = draft(quote());
        let bell = orchestrator
            .handle(Command::SellerMessageSend {
                message: waiting,
                subject: "Bell\u{7}".to_string(),
                body: BODY.to_string(),
            })
            .await
            .expect_err("a control character in the subject");
        assert!(
            reason(bell).starts_with("seller_message_field_invalid: "),
            "the subject is checked when it is sent"
        );
        assert!(harness.events(&[EventKind::SellerMessageSent]).is_empty());
        assert!(harness.events(&[EventKind::SellerMessageFailed]).is_empty());
    }

    /// An order of Ivo's, as `purchase_order.drafted` records it.
    fn order_body(number: u64) -> Value {
        json!({
            "order": number, "seller": "Pie Box Pros", "seller_contact": DANA.address,
            "lines": [{ "item": "Pie box", "quantity": 500, "unit": "piece",
                "unit_price": "0.40", "line_total": "200.00" }],
            "currency": "USD", "period": "once", "total": "200.00", "delivery": "",
            "terms": "", "url": "", "evaluation": "evaluations/boxes.md",
            "why": "It is the cheapest seller that ships to us."
        })
    }

    /// Fifty messages of Ivo's recorded as sent on `when`'s day, without files or a server.
    fn fifty_sent(harness: &Harness, when: chrono::DateTime<chrono::Utc>, first: u64) {
        many_sent(harness, when, first, 50);
    }

    /// `count` messages of Ivo's recorded as sent on `when`'s day, from number `first`.
    fn many_sent(harness: &Harness, when: chrono::DateTime<chrono::Utc>, first: u64, count: u64) {
        for message in first..first + count {
            harness.project.record_in(
                Some("proc"),
                Some("session-1"),
                "CTV-1",
                "seller_message.drafted",
                &json!({
                    "message": message, "seller": "Pie Box Pros", "to": DANA.address,
                    "subject": SUBJECT, "purpose": "quote_request",
                    "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c"
                }),
            );
            harness.project.record_at(
                when,
                "",
                "seller_message.sent",
                &json!({
                    "message": message,
                    "message_id": format!("m{message}@bakery.test"),
                    "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c",
                    "edited": false
                }),
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn the_fifty_first_send_of_a_day_is_refused() {
        let story = Story::new("fifty").await;
        let today = crate::tools::fixtures::at();
        // Fifty went out yesterday (UTC): today's count is none, and an order's email goes.
        fifty_sent(&story.harness, today - chrono::Duration::days(1), 100);
        let ordered = an_order_with_its_message(&story, 12);
        story
            .orchestrator()
            .handle(send_order(12, ordered))
            .await
            .expect("a new day");
        // Forty-nine today, the order's among them: the fiftieth goes, and the next is refused and
        // sends nothing.
        many_sent(&story.harness, today, 200, 48);
        let fiftieth = story.draft(SUBJECT, BODY);
        story
            .send(fiftieth, SUBJECT, BODY)
            .await
            .expect("the fiftieth");
        let second = story.draft(SUBJECT, BODY);
        let refused = story
            .send(second, SUBJECT, BODY)
            .await
            .expect_err("the limit");
        assert!(refused.starts_with("seller_send_limit: "), "{refused}");
        assert_eq!(
            story.dana_has().len(),
            2,
            "the order's email and the fiftieth"
        );
        let state = crate::procurement::mailbox_state(&story.harness.project.deps).expect("state");
        assert_eq!(state["sent_today"], 50);
        assert_eq!(state["cap"], 50);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn two_sends_at_once_send_one() {
        let story = Story::new("twice").await;
        let message = story.draft(SUBJECT, BODY);
        let (one, other) = tokio::join!(
            story.send(message, SUBJECT, BODY),
            story.send(message, SUBJECT, BODY)
        );
        let (done, refused): (Vec<_>, Vec<_>) = [one, other].into_iter().partition(Result::is_ok);
        assert_eq!((done.len(), refused.len()), (1, 1));
        let reason = refused[0].clone().expect_err("refused");
        assert!(reason.starts_with("seller_message_sent: "), "{reason}");
        assert_eq!(story.dana_has().len(), 1, "one went");
        assert_eq!(story.events(&[EventKind::SellerMessageSent]).len(), 1);
    }

    /// Ivo's order PO-12 on CTV-1, with its workbook, and the message that goes with it; answers
    /// the message's number.
    fn an_order_with_its_message(story: &Story, order: u64) -> u64 {
        story.harness.project.record_in(
            Some("proc"),
            Some("session-1"),
            "CTV-1",
            "purchase_order.drafted",
            &order_body(order),
        );
        let folder = story.harness.procurement_folder().join("orders");
        std::fs::create_dir_all(&folder).expect("the orders folder");
        std::fs::write(
            folder.join(format!("PO-{order}.xlsx")),
            b"PK the workbook's bytes",
        )
        .expect("the workbook");
        story.draft_as(json!({
            "seller": "Pie Box Pros", "to": DANA.address, "subject": "Order PO-12",
            "body": "Please find our order attached.", "purpose": "purchase_order",
            "purchase_order": order
        }))
    }

    fn send_order(order: u64, message: u64) -> Command {
        Command::PurchaseOrderSend {
            order,
            message,
            subject: "Order PO-12".to_string(),
            body: "Please find our order attached.".to_string(),
            note: Some("Go ahead".to_string()),
        }
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn approve_and_send_places_the_order() {
        let story = Story::new("order").await;
        let message = an_order_with_its_message(&story, 12);
        let report = story
            .orchestrator()
            .handle(send_order(12, message))
            .await
            .expect("sent and approved");
        assert_eq!(report.events.len(), 3, "{report:?}");

        let kinds = [
            EventKind::PurchaseOrderApproved,
            EventKind::SellerMessageSent,
            EventKind::PurchaseOrderPlaced,
        ];
        let events = story.events(&kinds);
        assert_eq!(
            events
                .iter()
                .map(|event| event.body.kind())
                .collect::<Vec<_>>(),
            kinds,
            "in this order"
        );
        for event in &events {
            let ids = &event.envelope.ids;
            assert_eq!((&ids.agent_id, &ids.session_id), (&None, &None));
            assert_eq!(
                ids.task_id.as_ref().map(|task| task.as_str()),
                Some("CTV-1")
            );
        }
        let EventBody::PurchaseOrderApproved(approved) = &events[0].body else {
            panic!("an approval");
        };
        assert_eq!(approved.note.as_str(), "Go ahead");
        let EventBody::PurchaseOrderPlaced(placed) = &events[2].body else {
            panic!("a placing");
        };
        assert_eq!(
            placed.placed_on,
            crate::tools::fixtures::at().date_naive(),
            "today, in UTC"
        );
        // Dana has the email with the workbook, byte for byte.
        let raw = story.dana_has();
        assert_eq!(raw.len(), 1);
        let parsed = mail_parser::MessageParser::default()
            .parse(raw[0].as_bytes())
            .expect("a message");
        let attachment = parsed.attachment(0).expect("the workbook");
        assert_eq!(attachment.attachment_name(), Some("PO-12.xlsx"));
        assert_eq!(attachment.contents(), b"PK the workbook's bytes");
        // The order is placed, and leaves what waits.
        let records = purchase_orders(&story.harness.project.deps.log).expect("orders");
        assert_eq!(records[0].state, OrderState::Placed);
        assert!(
            orders_waiting(&story).is_empty(),
            "the order leaves what waits"
        );
        // It cannot be decided again.
        let again = story
            .orchestrator()
            .handle(send_order(12, message))
            .await
            .expect_err("decided");
        assert!(reason(again).starts_with("purchase_order_decided: "));
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_failed_order_send_changes_nothing_on_the_order() {
        let story = Story::new("order-failed").await;
        let message = an_order_with_its_message(&story, 12);
        story
            .store
            .save(&story.at, &secret("changed-since"))
            .expect("saved");
        let refused = story
            .orchestrator()
            .handle(send_order(12, message))
            .await
            .expect_err("not sent");
        assert!(reason(refused).starts_with("seller_message_failed: "));
        let recorded: Vec<EventKind> = story
            .events(&[
                EventKind::PurchaseOrderApproved,
                EventKind::PurchaseOrderPlaced,
                EventKind::SellerMessageSent,
                EventKind::SellerMessageFailed,
            ])
            .iter()
            .map(|event| event.body.kind())
            .collect();
        assert_eq!(recorded, [EventKind::SellerMessageFailed]);
        let records = purchase_orders(&story.harness.project.deps.log).expect("orders");
        assert_eq!(
            records[0].state,
            OrderState::Drafted,
            "the order still waits"
        );
        story
            .store
            .save(&story.at, &secret(BUYING.password))
            .expect("saved");

        // Another order's message, a rejected order and an expired one.
        let other = an_order_with_its_message(&story, 13);
        let wrong = story
            .orchestrator()
            .handle(send_order(12, other))
            .await
            .expect_err("not this order's");
        assert!(reason(wrong).starts_with("seller_message_not_this_order: "));
        story.harness.project.record(
            "CTV-1",
            "purchase_order.rejected",
            &json!({ "order": 13, "note": "" }),
        );
        let rejected = story
            .orchestrator()
            .handle(send_order(13, other))
            .await
            .expect_err("decided");
        assert!(reason(rejected).starts_with("purchase_order_decided: "));
        let closed_by_time = an_order_with_its_message(&story, 14);
        story
            .harness
            .project
            .record("CTV-1", "purchase_order.expired", &json!({ "order": 14 }));
        let expired = story
            .orchestrator()
            .handle(send_order(14, closed_by_time))
            .await
            .expect_err("expired");
        assert!(reason(expired).starts_with("purchase_order_expired: "));
        let unknown = story
            .orchestrator()
            .handle(send_order(99, message))
            .await
            .expect_err("no such");
        assert!(reason(unknown).starts_with("unknown_purchase_order: "));
        // The limit.
        fifty_sent(&story.harness, crate::tools::fixtures::at(), 300);
        let capped = story
            .orchestrator()
            .handle(send_order(12, message))
            .await
            .expect_err("the limit");
        assert!(reason(capped).starts_with("seller_send_limit: "));
        assert!(story.dana_has().is_empty(), "nothing went");
        assert_eq!(
            purchase_orders(&story.harness.project.deps.log).expect("orders")[0].state,
            OrderState::Drafted
        );
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn an_order_being_emailed_is_not_decided_meanwhile() {
        let story = Story::new("order-claimed").await;
        let message = an_order_with_its_message(&story, 12);
        // A send in flight holds its order: deciding or expiring it waits for the outcome.
        let mailer = Mailer {
            secrets: &*story.store,
            at: story.at.clone(),
            trust: Trust::Root(story.fixture.ca_der.clone()),
        };
        let prepared = prepare(
            &story.harness.project.deps,
            &mailer,
            SendAsk {
                message,
                subject: "Order PO-12",
                body: "Please find our order attached.",
                order: Some(12),
            },
        )
        .expect("claimed");
        let decided = story
            .orchestrator()
            .handle(Command::PurchaseOrderDecide {
                order: 12,
                approve: false,
                note: None,
            })
            .await
            .expect_err("it is being emailed");
        assert!(reason(decided).starts_with("purchase_order_sending: "));
        let later = crate::tools::fixtures::at() + chrono::Duration::days(60);
        crate::procurement::expire_orders(&story.harness.project.deps, later).expect("expires");
        assert!(
            story.events(&[EventKind::PurchaseOrderExpired]).is_empty(),
            "an order being emailed is not expired"
        );
        // A second send of the same message or the same order is refused while it is claimed.
        let second = story
            .orchestrator()
            .handle(send_order(12, message))
            .await
            .expect_err("in flight");
        assert!(reason(second).starts_with("seller_message_sent: "));
        drop(prepared);
        // Let go, the order can be rejected.
        story
            .orchestrator()
            .handle(Command::PurchaseOrderDecide {
                order: 12,
                approve: false,
                note: None,
            })
            .await
            .expect("rejected");
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_send_on_its_way_counts_against_the_day_and_holds_its_message() {
        let story = Story::new("claimed").await;
        // Forty-nine went today. One more is on its way, so the next is the fifty-first.
        many_sent(&story.harness, crate::tools::fixtures::at(), 400, 49);
        let first = story.draft(SUBJECT, BODY);
        let second = story.draft(SUBJECT, BODY);
        let mailer = Mailer {
            secrets: &*story.store,
            at: story.at.clone(),
            trust: Trust::Root(story.fixture.ca_der.clone()),
        };
        let deps = &story.harness.project.deps;
        let ask = |message: u64| SendAsk {
            message,
            subject: SUBJECT,
            body: BODY,
            order: None,
        };
        let on_its_way = prepare(deps, &mailer, ask(first)).expect("the fiftieth is claimed");
        let refused = story
            .send(second, SUBJECT, BODY)
            .await
            .expect_err("the limit");
        assert!(refused.starts_with("seller_send_limit: "), "{refused}");
        // The message on its way is not discarded or claimed twice meanwhile.
        let discarded = discard_message(deps, first).expect_err("on its way");
        assert_eq!(discarded.code, "seller_message_sent");
        assert!(prepare(deps, &mailer, ask(first)).is_err());
        // Let go of the claim, the day has room again and the first can be discarded.
        drop(on_its_way);
        story
            .send(second, SUBJECT, BODY)
            .await
            .expect("the fiftieth goes");
        discard_message(deps, first).expect("discarded");
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn the_lists_carry_what_today_shows() {
        let story = Story::new("lists").await;
        let first = story.draft(SUBJECT, BODY);
        let listed = seller_messages_list(&story.harness.project.deps).expect("the list");
        assert_eq!(listed["cap"], 50);
        assert_eq!(listed["sent_today"], 0);
        let row = &listed["messages"][0];
        assert_eq!(row["message"], first);
        assert_eq!(row["state"], "waiting");
        assert_eq!(row["seller"], "Pie Box Pros");
        assert_eq!(row["to"], DANA.address);
        assert_eq!(row["domain"], "pieboxpros.test");
        assert_eq!(row["new_domain"], true, "nothing went there yet");
        assert_eq!(row["subject"], SUBJECT);
        assert_eq!(row["body"], BODY);
        assert_eq!(row["purpose"], "quote_request");
        assert_eq!(row["task_id"], "CTV-1");
        assert_eq!(row["agent_id"], "proc");
        assert!(row.get("purchase_order").is_none() && row.get("sent_at").is_none());
        story.send(first, SUBJECT, BODY).await.expect("sent");
        let second = story.draft(SUBJECT, BODY);
        let listed = seller_messages_list(&story.harness.project.deps).expect("the list");
        assert_eq!(listed["sent_today"], 1);
        let sent = &listed["messages"][0];
        assert_eq!(sent["state"], "sent");
        assert_eq!(sent["edited"], false);
        assert!(sent["sent_at"].is_string());
        assert!(
            sent["body"]
                .as_str()
                .expect("the text sent")
                .contains("Corner Bakery"),
            "what was sent, not the draft"
        );
        let waiting = &listed["messages"][1];
        assert_eq!(waiting["message"], second);
        assert_eq!(waiting["new_domain"], false, "a message went there already");

        // An order's waiting message is on its order's row, and gone once the order is rejected.
        let ordered = an_order_with_its_message(&story, 12);
        let rows = orders_waiting(&story);
        assert_eq!(rows.len(), 1);
        let waiting_send = rows[0].send.as_ref().expect("the order's message");
        assert_eq!(
            (waiting_send.message, waiting_send.to.as_str()),
            (ordered, DANA.address)
        );
        assert_eq!(waiting_send.subject, "Order PO-12");
        let mut row = json!({});
        add_order_send_fields(&mut row, &story.harness.project.deps, waiting_send);
        assert_eq!(row["send"]["message"], ordered);
        assert_eq!(row["send"]["to"], DANA.address);
        assert_eq!(row["send"]["domain"], "pieboxpros.test");
        assert_eq!(
            row["send"]["new_domain"], false,
            "a message went there already"
        );
        assert_eq!(row["send"]["subject"], "Order PO-12");
        assert_eq!(row["send"]["body"], "Please find our order attached.");
        story.harness.project.record(
            "CTV-1",
            "purchase_order.rejected",
            &json!({ "order": 12, "note": "" }),
        );
        assert!(orders_waiting(&story).is_empty());
        let closed = seller_messages_list(&story.harness.project.deps).expect("the list");
        assert_eq!(closed["messages"][2]["state"], "closed");
    }
}
