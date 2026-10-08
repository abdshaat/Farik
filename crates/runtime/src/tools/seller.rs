use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use farik_core::contract::Role;
use farik_core::governor::sites::site_of;
use farik_protocol::event::{EventBody, SellerMessageDraftedBody, SellerMessagePurpose};
use farik_store::purchase_orders::{OrderState, PurchaseOrderRecord, purchase_orders};
use farik_store::seller_mail::{MessageState, SellerMail, seller_mail};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::refusal::Refusal;
use super::sheets::in_its_own_implement_session;
use super::sites::shown;
use super::{Call, ToolError, failed};
use crate::procurement::{MAIL, mail_dir};

/// The most characters a seller's name or a subject line has.
const MOST_SELLER: usize = 100;
const MOST_SUBJECT: usize = 200;
/// The most characters a body has.
const MOST_BODY: usize = 8_000;
/// The most characters an address has.
const MOST_ADDRESS: usize = 254;
/// The most messages that wait for the owner at once in a project.
const MOST_WAITING: usize = 20;
/// What the agent whose message was written is told.
const NEXT: &str = "nothing is sent: the owner reads it on Today and sends it when they choose; \
    read what was sent and what came back with farik_read_seller_messages and \
    farik_read_seller_replies, and go on with your task meanwhile";

/// Why a message is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SellerPurpose {
    /// Ask a seller or a maker for a quote or a price list.
    QuoteRequest,
    /// Ask a seller a question; with `purchase_order` naming an order the owner placed, a
    /// follow-up on it.
    Question,
    /// The message that goes with an order you suggested, sent when the owner approves the order
    /// and chooses to send it. Name the order in `purchase_order`.
    PurchaseOrder,
}

impl From<SellerPurpose> for SellerMessagePurpose {
    fn from(purpose: SellerPurpose) -> Self {
        match purpose {
            SellerPurpose::QuoteRequest => Self::QuoteRequest,
            SellerPurpose::Question => Self::Question,
            SellerPurpose::PurchaseOrder => Self::PurchaseOrder,
        }
    }
}

/// `farik_draft_seller_message`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftSellerMessageInput {
    /// The seller's or the maker's name, 1 to 100 characters on one line.
    seller: String,
    /// One email address, with no display name and no list. Its domain must be a site's name.
    to: String,
    /// The subject, 1 to 200 characters on one line.
    subject: String,
    /// The message, 1 to 8000 characters of plain text; no HTML, no attachment. Farik adds the
    /// owner's signature and says an AI assistant wrote it.
    body: String,
    /// `quote_request`, `question` or `purchase_order`.
    purpose: SellerPurpose,
    /// The order the message is about: required with `purchase_order` (a drafted order of yours),
    /// allowed with `question` (an order of yours that the owner placed), refused with
    /// `quote_request`.
    #[serde(default)]
    purchase_order: Option<u64>,
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::Seller {
        code,
        detail: detail.into(),
    }
    .into()
}

/// A line of `text` held to its length, counted after trimming, with no control character; the
/// code and the words of the refusal when it is not.
///
/// # Errors
///
/// `seller_message_field_invalid` naming `field`.
pub(crate) fn line_fault(
    field: &str,
    text: &str,
    most: usize,
) -> Result<String, (&'static str, String)> {
    let trimmed = text.trim();
    let length = trimmed.chars().count();
    if !(1..=most).contains(&length) || trimmed.chars().any(char::is_control) {
        return Err((
            "seller_message_field_invalid",
            format!(
                "{field} is 1 to {most} characters on one line with no control character, and \
                 this one is {length}"
            ),
        ));
    }
    Ok(trimmed.to_string())
}

/// The subject, held to its rules: 1 to 200 characters on one line.
///
/// # Errors
///
/// As [`line_fault`].
pub(crate) fn subject_fault(text: &str) -> Result<String, (&'static str, String)> {
    line_fault("subject", text, MOST_SUBJECT)
}

fn one_line(field: &str, text: &str, most: usize) -> Result<String, ToolError> {
    line_fault(field, text, most).map_err(|(code, detail)| refused(code, detail))
}

/// The address held to its rules: one address, no display name, no list, at most 254 characters,
/// a domain `site_of` accepts as `https://<domain>/` (so no IP address and no host with no dot),
/// and not the procurement mailbox's own.
fn address(text: &str, procurement: Option<&str>) -> Result<String, ToolError> {
    let trimmed = text.trim();
    let bad = |why: &str| {
        refused(
            "seller_address_invalid",
            format!("to {} {why}", shown(trimmed)),
        )
    };
    if trimmed.chars().count() > MOST_ADDRESS
        || trimmed
            .chars()
            .any(|one| one.is_control() || one.is_whitespace())
    {
        return Err(bad("is not one email address of at most 254 characters"));
    }
    let parsed: lettre::Address = trimmed
        .parse()
        .map_err(|_| bad("is not one email address, without a name or a list"))?;
    if site_of(&format!("https://{}/", parsed.domain())).is_err() {
        return Err(bad("is not at a domain name: write a seller's own address"));
    }
    if procurement.is_some_and(|own| own.eq_ignore_ascii_case(trimmed)) {
        return Err(bad("is the procurement mailbox itself"));
    }
    Ok(trimmed.to_string())
}

/// The body, trimmed, its line ends made `\n`, with no control character but a line break and a
/// tab; at most 8000 characters. The code and the words of the refusal when it is not.
///
/// # Errors
///
/// `seller_message_field_invalid` or `seller_message_too_long`.
pub(crate) fn body_fault(text: &str) -> Result<String, (&'static str, String)> {
    let body = text.replace("\r\n", "\n");
    let body = body.trim();
    let length = body.chars().count();
    if length == 0
        || body
            .chars()
            .any(|one| one.is_control() && !matches!(one, '\n' | '\t'))
    {
        return Err((
            "seller_message_field_invalid",
            "body is plain text of 1 to 8000 characters, with line breaks and tabs but no other \
             control character"
                .to_string(),
        ));
    }
    if length > MOST_BODY {
        return Err((
            "seller_message_too_long",
            format!("body is at most {MOST_BODY} characters, and this one is {length}; say less"),
        ));
    }
    Ok(body.to_string())
}

fn body_of(text: &str) -> Result<String, ToolError> {
    body_fault(text).map_err(|(code, detail)| refused(code, detail))
}

/// The order `number`, when it is the calling agent's own.
fn own_order<'a>(
    call: &Call<'_>,
    orders: &'a [PurchaseOrderRecord],
    number: u64,
) -> Option<&'a PurchaseOrderRecord> {
    orders
        .iter()
        .find(|order| order.order == number && order.agent_id == call.agent_id())
}

/// What the purpose says of the order the message is about: an order's message names a drafted
/// order of the caller's with no message waiting, a question names a placed one of its own to
/// follow it up, a quote request names none.
fn check_order(
    call: &Call<'_>,
    input: &DraftSellerMessageInput,
    mail: &SellerMail,
    orders: &[PurchaseOrderRecord],
) -> Result<(), ToolError> {
    let invalid = |why: &str| refused("seller_message_order_invalid", why.to_string());
    match (input.purpose, input.purchase_order) {
        (SellerPurpose::QuoteRequest, Some(_)) => {
            return Err(invalid("a quote request is about no order"));
        }
        (SellerPurpose::QuoteRequest | SellerPurpose::Question, None) => {}
        (SellerPurpose::PurchaseOrder, None) => {
            return Err(invalid(
                "an order's message names its order in purchase_order",
            ));
        }
        (SellerPurpose::PurchaseOrder, Some(number)) => {
            if !own_order(call, orders, number)
                .is_some_and(|order| order.state == OrderState::Drafted)
            {
                return Err(invalid(&format!(
                    "PO-{number} is not an order of yours that waits for the owner"
                )));
            }
            if mail.messages.iter().any(|record| {
                record.state == MessageState::Waiting
                    && record.drafted.purchase_order.as_ref().map(|n| n.get()) == Some(number)
                    && record.drafted.purpose == SellerMessagePurpose::PurchaseOrder
            }) {
                return Err(refused(
                    "seller_order_message_waiting",
                    format!(
                        "PO-{number} has a message that waits for the owner; read it with farik_read_seller_messages"
                    ),
                ));
            }
        }
        (SellerPurpose::Question, Some(number)) => {
            if !own_order(call, orders, number)
                .is_some_and(|order| order.state == OrderState::Placed)
            {
                return Err(invalid(&format!(
                    "PO-{number} is not an order of yours that the owner placed: a question \
                     names only a placed order, to follow it up"
                )));
            }
        }
    }
    Ok(())
}

/// The highest message number named by a file of `out`: `<n>.txt` or `<n>.sent.txt`.
fn highest_on_disk(out: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(out) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name
                .strip_suffix(".sent.txt")
                .or_else(|| name.strip_suffix(".txt"))?;
            stem.parse::<u64>().ok()
        })
        .max()
        .unwrap_or(0)
}

/// `bytes` in lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// `farik_draft_seller_message`: writes `mail/out/<n>.txt` and records `seller_message.drafted`.
/// Nothing is sent. Checked in this order, nothing written or recorded on any refusal: the role and
/// the session, the seller and the subject, the address, the body, then, under the lock, the order
/// the purpose names and the limit of messages waiting.
///
/// # Errors
///
/// `seller_message_refused` outside the implement session of a task the Procurement Specialist
/// holds, `seller_message_field_invalid` naming the field, `seller_address_invalid`,
/// `seller_message_too_long`, `seller_message_order_invalid`, `seller_order_message_waiting`,
/// `seller_drafts_full`; `Failed` when the log or the files cannot be read or written.
pub(super) fn draft_seller_message(
    call: &Call<'_>,
    input: &DraftSellerMessageInput,
) -> Result<Value, ToolError> {
    let refuse = |why: &str| refused("seller_message_refused", format!("only {why}"));
    if call.role() != Role::ProcurementSpecialist {
        return Err(refuse(
            "the Procurement Specialist drafts a message to a seller",
        ));
    }
    in_its_own_implement_session(call, "a message to a seller", &refuse)?;
    let task = call.task()?.clone();
    let deps = call.deps();
    let seller = one_line("seller", &input.seller, MOST_SELLER)?;
    let subject = one_line("subject", &input.subject, MOST_SUBJECT)?;
    let own = seller_mail(&deps.log).map_err(failed)?.address;
    let to = address(&input.to, own.as_deref())?;
    let body = body_of(&input.body)?;

    // The number, the checks that read the other messages, the file and the record are one step.
    let _held = crate::locked(&MAIL);
    let mail = seller_mail(&deps.log).map_err(failed)?;
    let orders = purchase_orders(&deps.log).map_err(failed)?;
    check_order(call, input, &mail, &orders)?;
    if mail
        .messages
        .iter()
        .filter(|record| record.state == MessageState::Waiting)
        .count()
        >= MOST_WAITING
    {
        return Err(refused(
            "seller_drafts_full",
            format!(
                "{MOST_WAITING} messages wait for the owner already; go on without another, and \
                 read them with farik_read_seller_messages"
            ),
        ));
    }
    let out = mail_dir(deps)
        .map_err(|refusal| failed(refusal.words))?
        .join("out");
    {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&out)
            .map_err(failed)?;
    }
    let highest = mail
        .messages
        .iter()
        .map(|record| record.message)
        .max()
        .unwrap_or(0)
        .max(highest_on_disk(&out));
    let number = highest + 1;
    let file = out.join(format!("{number}.txt"));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .and_then(|mut file| file.write_all(body.as_bytes()))
        .map_err(failed)?;
    let drafted = SellerMessageDraftedBody {
        message: std::num::NonZeroU64::new(number)
            .map(Into::into)
            .ok_or_else(|| failed("a message is numbered from 1"))?,
        seller: seller.try_into().map_err(failed)?,
        to: to.try_into().map_err(failed)?,
        subject: subject.try_into().map_err(failed)?,
        purpose: SellerMessagePurpose::from(input.purpose),
        purchase_order: input
            .purchase_order
            .and_then(std::num::NonZeroU64::new)
            .map(Into::into),
        sha256: hex(&Sha256::digest(body.as_bytes()))
            .try_into()
            .map_err(failed)?,
    };
    if let Err(error) = call.append(Some(&task), EventBody::SellerMessageDrafted(drafted)) {
        let _ = std::fs::remove_file(&file);
        return Err(error);
    }
    Ok(json!({ "message": number, "state": "waiting", "next": NEXT }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use farik_protocol::event::{EventBody, EventKind};
    use farik_store::seller_mail::seller_mail;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, run, with_the_finance_specialist,
        with_the_procurement_specialist,
    };

    /// A project with the Procurement Specialists `proc` and `proc-2` and the Finance Specialist
    /// `fin`, each in progress on a task of its own: FRK-1, FRK-2 and FRK-3.
    fn a_project(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_procurement_specialist(wire);
                wire["agents"]
                    .as_array_mut()
                    .expect("a list of agents")
                    .push(farik_core::team::fixtures::an_agent_wire(
                        "proc-2",
                        "procurement_specialist",
                    ));
            }),
        );
        for (task, role, assignee) in [
            ("FRK-1", "procurement_specialist", "proc"),
            ("FRK-2", "finance_specialist", "fin"),
            ("FRK-3", "procurement_specialist", "proc-2"),
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
    }

    /// The message of the story: Ivo asks Pie Box Pros for a quote on 500 printed pie boxes.
    fn quote() -> Value {
        json!({
            "seller": "Pie Box Pros",
            "to": "sales@pieboxpros.test",
            "subject": "Quote for 500 printed pie boxes",
            "body": "Hello,\n\nCould you quote 500 printed pie boxes, 9 inch, delivered to Corner Bakery?\n\nThank you.",
            "purpose": "quote_request"
        })
    }

    fn with(mut input: Value, field: &str, value: Value) -> Value {
        input[field] = value;
        input
    }

    /// `farik_draft_seller_message` as `proc` in its implement session of FRK-1.
    fn draft(project: &TestProject, input: &Value) -> Result<Value, ToolError> {
        project.call(
            "proc",
            Some("FRK-1"),
            "farik_draft_seller_message",
            input.clone(),
        )
    }

    fn refusal_of(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn out_dir(project: &TestProject) -> PathBuf {
        project.repo.path.join(".farik/local/procurement/mail/out")
    }

    fn files_in(folder: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn hex(bytes: &[u8]) -> String {
        super::hex(bytes)
    }

    /// An order `number` that `agent` drafted on `task`.
    fn drafted_order(project: &TestProject, agent: &str, task: &str, number: u64) {
        project.record_in(
            Some(agent),
            Some("session-1"),
            task,
            "purchase_order.drafted",
            &json!({
                "order": number, "seller": "Pie Box Pros", "seller_contact": "",
                "lines": [{ "item": "Pie box", "quantity": 500, "unit": "piece",
                    "unit_price": "0.40", "line_total": "200.00" }],
                "currency": "USD", "period": "once", "total": "200.00", "delivery": "",
                "terms": "", "url": "", "evaluation": "evaluations/boxes.md",
                "why": "It is the cheapest seller that ships to us."
            }),
        );
    }

    /// The owner's step on `order`: no agent, no session.
    fn owner_decides(project: &TestProject, kind: &str, body: &Value) {
        project.record("FRK-1", kind, body);
    }

    /// Every `.rs` file under `folder`, without the tests at its end.
    fn sources_under(folder: &std::path::Path, found: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(folder)
            .expect("the folder reads")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                sources_under(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source");
                let code = text
                    .split("#[cfg(test)]")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                found.push((path, code));
            }
        }
    }

    #[test]
    fn no_tool_of_an_agent_records_a_send_a_failure_a_discard_a_reply_or_a_mailbox() {
        // Only the owner's commands and Farik's own checks record these: the one door that sends
        // is the owner's press, so a tool that could record one is a tool that could send.
        let mut found = Vec::new();
        sources_under(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools"),
            &mut found,
        );
        assert!(
            found.len() > 10,
            "the tools' sources were found: {}",
            found.len()
        );
        for (path, code) in found {
            for kind in [
                "EventBody::SellerMessageSent",
                "EventBody::SellerMessageFailed",
                "EventBody::SellerMessageDiscarded",
                "EventBody::SellerReplyReceived",
                "EventBody::SellerReplyDismissed",
                "EventBody::MailboxConnected",
                "EventBody::MailboxDisconnected",
                "lettre::transport",
                "lettre::AsyncTransport",
                "lettre::Message",
                "mailbox::send",
                "procurement::send",
                "send_message",
            ] {
                assert!(
                    !code.contains(kind),
                    "{} could record or send with {kind}",
                    path.display()
                );
            }
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn drafts_a_message_to_one_seller() {
        let project = a_project("seller-draft");
        let first = draft(&project, &quote()).expect("a draft");
        assert_eq!(first["message"], 1);
        let file = out_dir(&project).join("1.txt");
        let body = std::fs::read_to_string(&file).expect("the draft is written");
        assert_eq!(body, quote()["body"].as_str().expect("a body"));
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = |path: &std::path::Path| {
                std::fs::metadata(path)
                    .expect("exists")
                    .permissions()
                    .mode()
                    & 0o777
            };
            assert_eq!(mode(&file), 0o600, "the draft is the owner's alone");
            assert_eq!(mode(&out_dir(&project)), 0o700, "so is its folder");
        }
        let events = project.events(&[EventKind::SellerMessageDrafted]);
        assert_eq!(events.len(), 1);
        let ids = &events[0].envelope.ids;
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(ids.agent_id.as_deref(), Some("proc"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
        let EventBody::SellerMessageDrafted(drafted) = &events[0].body else {
            panic!("a draft");
        };
        assert_eq!(drafted.message.get(), 1);
        assert_eq!(drafted.seller.as_str(), "Pie Box Pros");
        assert_eq!(drafted.to.as_str(), "sales@pieboxpros.test");
        assert_eq!(drafted.subject.as_str(), "Quote for 500 printed pie boxes");
        assert_eq!(
            drafted.sha256.as_str(),
            hex(&Sha256::digest(body.as_bytes()))
        );
        assert!(drafted.purchase_order.is_none());

        // The next is 2; with a file 3 on disk and no event, the one after is 4.
        assert_eq!(draft(&project, &quote()).expect("a draft")["message"], 2);
        std::fs::write(out_dir(&project).join("3.txt"), "left by a failed record").expect("a file");
        assert_eq!(draft(&project, &quote()).expect("a draft")["message"], 4);
        // A sent copy counts too.
        std::fs::write(out_dir(&project).join("9.sent.txt"), "Subject\n\nBody").expect("a file");
        assert_eq!(draft(&project, &quote()).expect("a draft")["message"], 10);
        let mail = seller_mail(&project.deps.log).expect("folds");
        assert_eq!(
            mail.messages.iter().map(|m| m.message).collect::<Vec<_>>(),
            [1, 2, 4, 10]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_another_role_and_another_session() {
        let project = a_project("seller-draft-session");
        // The Finance Specialist, the Procurement Specialist's chat and verify session, and its
        // implement session of another's task.
        let reason =
            refusal_of(project.call("fin", Some("FRK-2"), "farik_draft_seller_message", quote()));
        assert!(reason.starts_with("seller_message_refused: "), "{reason}");
        for purpose in [SessionPurpose::Chat, SessionPurpose::Verify] {
            let mut context = project.context("proc", Some("FRK-1"));
            context.purpose = purpose;
            let reason = refusal_of(run(&context, "farik_draft_seller_message", quote()));
            assert!(reason.starts_with("seller_message_refused: "), "{reason}");
        }
        let reason =
            refusal_of(project.call("proc", Some("FRK-3"), "farik_draft_seller_message", quote()));
        assert!(reason.starts_with("seller_message_refused: "), "{reason}");
        assert!(files_in(&out_dir(&project)).is_empty());
        assert!(
            project
                .events(&[EventKind::SellerMessageDrafted])
                .is_empty()
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "one table of bad fields")]
    fn refuses_each_bad_field() {
        let project = a_project("seller-draft-fields");
        let wrong: Vec<(&str, Value, &str, Option<&str>)> = vec![
            (
                "seller",
                json!(""),
                "seller_message_field_invalid",
                Some("seller"),
            ),
            (
                "seller",
                json!("x".repeat(101)),
                "seller_message_field_invalid",
                Some("seller"),
            ),
            (
                "seller",
                json!("Pie\nBox"),
                "seller_message_field_invalid",
                Some("seller"),
            ),
            (
                "subject",
                json!("Two\nlines"),
                "seller_message_field_invalid",
                Some("subject"),
            ),
            (
                "subject",
                json!("x".repeat(201)),
                "seller_message_field_invalid",
                Some("subject"),
            ),
            (
                "subject",
                json!("Bell\u{7}"),
                "seller_message_field_invalid",
                Some("subject"),
            ),
            (
                "to",
                json!("a@pieboxpros.test, b@pieboxpros.test"),
                "seller_address_invalid",
                None,
            ),
            (
                "to",
                json!("Dana <sales@pieboxpros.test>"),
                "seller_address_invalid",
                None,
            ),
            ("to", json!("x@[1.2.3.4]"), "seller_address_invalid", None),
            ("to", json!("x@localhost"), "seller_address_invalid", None),
            (
                "to",
                json!("buying@bakery.test"),
                "seller_address_invalid",
                None,
            ),
            (
                "body",
                json!("x".repeat(8001)),
                "seller_message_too_long",
                None,
            ),
            (
                "body",
                json!(""),
                "seller_message_field_invalid",
                Some("body"),
            ),
            (
                "body",
                json!("Hello\u{0}"),
                "seller_message_field_invalid",
                Some("body"),
            ),
            (
                "body",
                json!("Hello\rthere"),
                "seller_message_field_invalid",
                Some("body"),
            ),
        ];
        // The procurement address is known once a mailbox is connected.
        project.record(
            "",
            "mailbox.connected",
            &json!({ "purpose": "procurement", "address": "Buying@Bakery.test" }),
        );
        for (field, value, code, named) in wrong {
            let reason = refusal_of(draft(&project, &with(quote(), field, value.clone())));
            assert!(
                reason.starts_with(&format!("{code}: ")),
                "{field}: {reason}"
            );
            if let Some(named) = named {
                assert!(reason.contains(named), "{field}: {reason}");
            }
        }
        // Tab and line breaks are fine in a body, CRLF too.
        draft(
            &project,
            &with(quote(), "body", json!("Line one\r\n\tLine two")),
        )
        .expect("a body");
        assert_eq!(
            files_in(&out_dir(&project)),
            ["1.txt"],
            "only the good one is written"
        );
        assert_eq!(
            std::fs::read_to_string(out_dir(&project).join("1.txt")).expect("read"),
            "Line one\n\tLine two"
        );
        assert_eq!(project.events(&[EventKind::SellerMessageDrafted]).len(), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one story, an order from drafted to placed"
    )]
    fn an_order_message_follows_its_order() {
        let project = a_project("seller-draft-order");
        let order_message = |order: Value| {
            let mut input = with(quote(), "purpose", json!("purchase_order"));
            input["purchase_order"] = order;
            input
        };
        drafted_order(&project, "proc", "FRK-1", 12);
        drafted_order(&project, "proc-2", "FRK-3", 13);
        drafted_order(&project, "proc", "FRK-1", 14);
        drafted_order(&project, "proc", "FRK-1", 15);
        owner_decides(
            &project,
            "purchase_order.approved",
            &json!({ "order": 14, "note": "" }),
        );
        owner_decides(
            &project,
            "purchase_order.rejected",
            &json!({ "order": 15, "note": "" }),
        );

        // Its own drafted order: drafted; a second while it waits is refused.
        let answer = draft(&project, &order_message(json!(12))).expect("an order message");
        assert_eq!(answer["message"], 1);
        let reason = refusal_of(draft(&project, &order_message(json!(12))));
        assert!(
            reason.starts_with("seller_order_message_waiting: "),
            "{reason}"
        );
        // Another agent's order, an approved one, a rejected one, a missing one and none at all.
        for (order, why) in [
            (json!(13), "another agent's"),
            (json!(14), "an approved"),
            (json!(15), "a rejected"),
            (json!(99), "no such"),
            (Value::Null, "no order named"),
        ] {
            let reason = refusal_of(draft(&project, &order_message(order)));
            assert!(
                reason.starts_with("seller_message_order_invalid: "),
                "{why}: {reason}"
            );
        }
        // A question may name an order of its own that is placed, to follow it up.
        owner_decides(
            &project,
            "purchase_order.placed",
            &json!({ "order": 14, "placed_on": "2026-09-28" }),
        );
        let follow_up = |order: u64| {
            let mut input = with(quote(), "purpose", json!("question"));
            input["purchase_order"] = json!(order);
            input
        };
        draft(&project, &follow_up(14)).expect("a follow-up of a placed order");
        // Not a drafted one, not another agent's, and a quote request names no order at all.
        for order in [12, 13, 15] {
            let reason = refusal_of(draft(&project, &follow_up(order)));
            assert!(
                reason.starts_with("seller_message_order_invalid: "),
                "{order}: {reason}"
            );
        }
        let mut quote_with_order = quote();
        quote_with_order["purchase_order"] = json!(12);
        let reason = refusal_of(draft(&project, &quote_with_order));
        assert!(
            reason.starts_with("seller_message_order_invalid: "),
            "{reason}"
        );
        assert_eq!(project.events(&[EventKind::SellerMessageDrafted]).len(), 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn holds_at_most_twenty_waiting() {
        let project = a_project("seller-draft-twenty");
        for _ in 0..20 {
            draft(&project, &quote()).expect("a draft");
        }
        let reason = refusal_of(draft(&project, &quote()));
        assert!(reason.starts_with("seller_drafts_full: "), "{reason}");
        assert_eq!(files_in(&out_dir(&project)).len(), 20);
        // A discarded one no longer waits.
        project.record("", "seller_message.discarded", &json!({ "message": 1 }));
        assert_eq!(
            draft(&project, &quote()).expect("room again")["message"],
            21
        );

        // An order's message whose order was rejected no longer counts.
        let project = a_project("seller-draft-twenty-order");
        drafted_order(&project, "proc", "FRK-1", 12);
        let mut order_message = with(quote(), "purpose", json!("purchase_order"));
        order_message["purchase_order"] = json!(12);
        draft(&project, &order_message).expect("an order message");
        for _ in 0..19 {
            draft(&project, &quote()).expect("a draft");
        }
        assert!(refusal_of(draft(&project, &quote())).starts_with("seller_drafts_full: "));
        owner_decides(
            &project,
            "purchase_order.rejected",
            &json!({ "order": 12, "note": "" }),
        );
        draft(&project, &quote()).expect("the closed one is not counted");
    }
}
