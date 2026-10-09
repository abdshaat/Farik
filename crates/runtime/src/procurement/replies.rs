//! Reading the sellers' replies from the procurement mailbox (step 10f, `docs/SPEC.md` 6.10, 8.6):
//! the check Farik runs by itself every 15 minutes with no model, what it keeps of each reply, and
//! the lists and the dismissal Today uses. A reply is a seller's words and untrusted.

use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use farik_protocol::event::{EventBody, SellerReplyDismissedBody, SellerReplyReceivedBody};
use farik_store::StoreError;
use farik_store::seller_mail::{MessageState, SellerMail, seller_mail};
use serde_json::{Value, json};

use super::{
    MAIL, MailboxRefusal, Mailer, mail_dir, mail_failed, mailbox_ledger, mailbox_settings,
    record_unattended, store_refusal,
};
use crate::daemon::DaemonState;
use crate::mailbox::{
    Known, Ledger, MailboxSecrets, MailboxSettings, Reply, extension_of, fetch_replies,
};
use crate::tools::ToolDeps;

/// How long after a check the next one is due.
const EVERY: Duration = Duration::minutes(15);

/// Farik's words when a reply cannot be kept or recorded: in the ledger and in the refusal, never
/// the store's detail, which repeats what the seller wrote.
const COULD_NOT_KEEP: &str =
    "Farik could not keep a reply from the mailbox; it tries again in 15 minutes.";

/// The projects whose mailbox is being read now: at most one check at a time in each, so that a
/// slow server never holds a tick and two checks never read one message twice.
static CHECKING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// What Farik knows of the mail it sent, from the log: the `Message-ID` of every message sent and
/// the addresses they went to.
fn known_of(mail: &SellerMail, settings: &MailboxSettings) -> Known {
    let sent = mail
        .messages
        .iter()
        .filter(|record| record.state == MessageState::Sent);
    Known {
        address: settings.address.clone(),
        sent_ids: sent
            .clone()
            .filter_map(|record| record.sent.as_ref())
            .map(|sent| sent.message_id.to_string())
            .collect(),
        written_to: sent.map(|record| record.drafted.to.to_string()).collect(),
    }
}

/// The sent message a reply answers: the one its headers name, else the latest sent to its sender.
fn answered(mail: &SellerMail, reply: &Reply) -> Option<u64> {
    let named = mail.messages.iter().find(|record| {
        record.sent.as_ref().is_some_and(|sent| {
            reply
                .answers
                .iter()
                .any(|id| id.trim_matches(['<', '>']) == sent.message_id.as_str())
        })
    });
    let from = reply
        .from
        .rsplit_once('<')
        .map_or(reply.from.as_str(), |(_, address)| {
            address.trim_end_matches('>')
        })
        .to_ascii_lowercase();
    named
        .or_else(|| {
            mail.messages
                .iter()
                .filter(|record| {
                    record.state == MessageState::Sent
                        && record.drafted.to.as_str().eq_ignore_ascii_case(&from)
                })
                .max_by_key(|record| record.sent_at)
        })
        .map(|record| record.message)
}

/// One more than the highest reply number the log and the folders under `in/` hold.
fn next_reply(mail: &SellerMail, inbox: &Path) -> u64 {
    let on_disk = std::fs::read_dir(inbox)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|month| std::fs::read_dir(month.path()).ok())
        .flatten()
        .flatten()
        .filter_map(|folder| folder.file_name().to_string_lossy().parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let logged = mail
        .replies
        .iter()
        .map(|record| record.reply)
        .max()
        .unwrap_or(0);
    on_disk.max(logged) + 1
}

fn make_dir(path: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

/// Keeps one reply: its text and the files it keeps under `in/<yyyy-mm>/<r>/`, named by their
/// bytes and numbered from 1, never by the seller's name for them; and records it, with Farik's own
/// envelope. Answers whether it was recorded.
fn keep_reply(
    deps: &ToolDeps,
    mail: &SellerMail,
    reply: &Reply,
    now: DateTime<Utc>,
) -> Result<bool, MailboxRefusal> {
    let Some(message) = answered(mail, reply) else {
        return Ok(false);
    };
    let inbox = mail_dir(deps)?.join("in");
    let number = next_reply(mail, &inbox);
    // The record is built, and so checked against the wire's rules, before any folder is made: a
    // reply the log would refuse leaves nothing on disk.
    let attachments: Vec<Value> = reply
        .attachments
        .iter()
        .map(|file| {
            let mut one = json!({
                "name": file.name, "kept": file.bytes.is_some(), "bytes": file.size,
            });
            if let (Some(_), Some(media)) = (&file.bytes, &file.media_type) {
                one["media_type"] = json!(media);
            }
            one
        })
        .collect();
    let mut body = json!({
        "reply": number, "message": message, "from": reply.from, "subject": reply.subject,
        "attachments": attachments,
    });
    if let Some(date) = reply.date {
        body["date"] = json!(date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    let body: SellerReplyReceivedBody = serde_json::from_value(body).map_err(mail_failed)?;
    let folder = inbox
        .join(now.format("%Y-%m").to_string())
        .join(number.to_string());
    let kept = || -> std::io::Result<()> {
        make_dir(&folder)?;
        write_new(&folder.join("text.txt"), reply.text.as_bytes())?;
        for (index, file) in reply.attachments.iter().enumerate() {
            if let (Some(bytes), Some(media)) = (&file.bytes, &file.media_type)
                && let Some(extension) = extension_of(media)
            {
                write_new(&folder.join(format!("{}.{extension}", index + 1)), bytes)?;
            }
        }
        Ok(())
    };
    if let Err(error) = kept() {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(mail_failed(error));
    }
    if let Err(error) = record_unattended(deps, EventBody::SellerReplyReceived(body), None) {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(mail_failed(error));
    }
    Ok(true)
}

/// Keeps `ledger` in `mail/ledger.json`.
fn write_ledger(deps: &ToolDeps, ledger: &Ledger) -> Result<(), MailboxRefusal> {
    let dir = mail_dir(deps)?;
    crate::write_private(
        &dir.join("ledger.json"),
        serde_json::to_string(ledger)
            .map_err(mail_failed)?
            .as_bytes(),
    )
    .map_err(mail_failed)
}

/// Reads the mailbox now: fetches what is new and for us, keeps and records each reply, and
/// writes the ledger with when it ended and, when it failed, why in Farik's words. Answers how
/// many replies were recorded.
///
/// # Errors
///
/// `mailbox_not_connected`, and a refusal as `fetch_replies`' (the same words the ledger and the
/// agent's page show).
pub(crate) async fn check_now(deps: &ToolDeps, mailer: &Mailer<'_>) -> Result<u32, MailboxRefusal> {
    let not_connected = || MailboxRefusal {
        code: "mailbox_not_connected",
        words: "Connect a procurement mailbox first.".to_string(),
    };
    let now = deps.clock.now();
    let settings = mailbox_settings(deps).ok_or_else(not_connected)?;
    let password = MailboxSecrets::load(mailer.secrets, &mailer.at)
        .map_err(store_refusal)?
        .ok_or_else(not_connected)?;
    let ledger = mailbox_ledger(deps).ok_or_else(not_connected)?;
    let mail = seller_mail(&deps.log).map_err(mail_failed)?;
    let known = known_of(&mail, &settings);
    let fetched = match fetch_replies(&settings, &password, &mailer.trust, &ledger, &known).await {
        Ok(fetched) => fetched,
        Err(error) => {
            let _guard = crate::locked(&MAIL);
            write_ledger(
                deps,
                &Ledger {
                    checked_at: Some(now),
                    error: Some(error.to_string()),
                    ..ledger
                },
            )?;
            return Err(error.into());
        }
    };
    let _guard = crate::locked(&MAIL);
    let mut recorded = 0;
    let mut stuck_at = None;
    // Each reply is kept and recorded against the log as it stands after the one before. A reply
    // that cannot be kept ends the check there: the ones before it are passed, and it is tried again.
    for reply in &fetched.replies {
        let kept = seller_mail(&deps.log)
            .map_err(mail_failed)
            .and_then(|mail| keep_reply(deps, &mail, reply, now));
        match kept {
            Ok(true) => recorded += 1,
            Ok(false) => {}
            Err(_) => {
                stuck_at = Some(reply.uid);
                break;
            }
        }
    }
    let restarted_at = if fetched.restarted {
        Some(now)
    } else {
        fetched.ledger.restarted_at
    };
    if let Some(uid) = stuck_at {
        write_ledger(
            deps,
            &Ledger {
                last_uid: uid.saturating_sub(1),
                checked_at: Some(now),
                error: Some(COULD_NOT_KEEP.to_string()),
                restarted_at,
                ..fetched.ledger
            },
        )?;
        return Err(MailboxRefusal {
            code: "mailbox_files",
            words: COULD_NOT_KEEP.to_string(),
        });
    }
    write_ledger(
        deps,
        &Ledger {
            checked_at: Some(now),
            error: None,
            restarted_at,
            ..fetched.ledger
        },
    )?;
    Ok(recorded)
}

/// Whether a check is due: a mailbox is connected and was never read, or last read 15 minutes ago
/// or more.
#[must_use]
pub(crate) fn check_is_due(deps: &ToolDeps, now: DateTime<Utc>) -> bool {
    mailbox_settings(deps).is_some()
        && mailbox_ledger(deps)
            .is_some_and(|ledger| ledger.checked_at.is_none_or(|at| now - at >= EVERY))
}

/// Starts a check of the mailbox, spawned, when one is due and none is running in this project:
/// a slow server never holds the tick that started it. Answers whether one was started.
pub(crate) fn start_check(deps: &Arc<ToolDeps>, daemon: &Arc<DaemonState>) -> bool {
    if !check_is_due(deps, deps.clock.now()) {
        return false;
    }
    let root = deps.files.root().to_path_buf();
    {
        let mut running = crate::locked(&CHECKING);
        if running.contains(&root) {
            return false;
        }
        running.push(root.clone());
    }
    let (deps, daemon) = (Arc::clone(deps), Arc::clone(daemon));
    tokio::spawn(async move {
        let secrets = daemon.connector_secrets();
        if let Ok(at) = daemon.mailbox_at(deps.files.root()) {
            let mailer = Mailer {
                secrets: &*secrets,
                at,
                trust: daemon.mail_trust(),
            };
            let _ = check_now(&deps, &mailer).await;
        }
        crate::locked(&CHECKING).retain(|running| *running != root);
    });
    true
}

/// `procurement_mailbox.check`: reads the mailbox now, unless a check is running already in this
/// project. Answers how many replies were recorded.
///
/// # Errors
///
/// As [`check_now`].
pub(crate) async fn check_by_hand(
    deps: &ToolDeps,
    mailer: &Mailer<'_>,
) -> Result<u32, MailboxRefusal> {
    let root = deps.files.root().to_path_buf();
    {
        let mut running = crate::locked(&CHECKING);
        if running.contains(&root) {
            return Ok(0);
        }
        running.push(root.clone());
    }
    let done = check_now(deps, mailer).await;
    crate::locked(&CHECKING).retain(|running| *running != root);
    done
}

/// Waits until no check is running, for a test that started one by a tick.
#[cfg(test)]
pub(crate) async fn finished_checks() {
    for _ in 0..3000 {
        if crate::locked(&CHECKING).is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("a check of the mailbox did not end within thirty seconds");
}

/// `seller_replies.list`: every reply Farik read, oldest first, with the message it answers
/// (the seller and the subject Farik sent), what the seller wrote, its attachments (numbered from
/// one) and whether the owner dismissed it. The order a reply is about is set for a reply to an
/// order's message or a follow-up.
///
/// # Errors
///
/// What the log refused.
pub fn seller_replies_list(deps: &ToolDeps) -> Result<Value, StoreError> {
    let mail = seller_mail(&deps.log)?;
    let replies: Vec<Value> = mail
        .replies
        .iter()
        .filter_map(|record| {
            let received = &record.received;
            let message = mail
                .messages
                .iter()
                .find(|message| message.message == received.message.get())?;
            let text = std::fs::read_to_string(
                reply_folder(deps, record.reply, record.received_at)?.join("text.txt"),
            )
            .unwrap_or_default();
            let mut row = json!({
                "reply": record.reply,
                "message": message.message,
                "task_id": message.task_id,
                "seller": message.drafted.seller.as_str(),
                "sent_subject": message.drafted.subject.as_str(),
                "from": received.from.as_str(),
                "subject": received.subject.as_str(),
                "text": text,
                "received_at": record.received_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
                "attachments": received.attachments.iter().enumerate().map(|(index, file)| {
                    let mut one = json!({
                        "index": index + 1, "name": file.name.as_str(), "kept": file.kept,
                        "bytes": file.bytes,
                    });
                    if let Some(media) = &file.media_type {
                        one["media_type"] = json!(media.to_string());
                    }
                    one
                }).collect::<Vec<_>>(),
                "dismissed": record.dismissed,
            });
            if let Some(date) = received.date {
                row["date"] = json!(date.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true));
            }
            if let Some(order) = &message.drafted.purchase_order {
                row["order"] = json!(order.get());
            }
            Some(row)
        })
        .collect();
    Ok(json!({ "replies": replies }))
}

/// The folder a reply was kept in: `mail/in/<yyyy-mm>/<r>/`, the month it was received in.
pub(crate) fn reply_folder(
    deps: &ToolDeps,
    reply: u64,
    received_at: DateTime<Utc>,
) -> Option<PathBuf> {
    Some(
        mail_dir(deps)
            .ok()?
            .join("in")
            .join(received_at.format("%Y-%m").to_string())
            .join(reply.to_string()),
    )
}

/// `seller_reply.attachment`: the bytes of attachment `index` (from 1) of reply `reply` as base64,
/// with its media type and a name made from the reply and the index, never the seller's. None
/// for a reply nobody received, an index it has not, and a file that was not kept.
#[must_use]
pub fn reply_attachment(deps: &ToolDeps, reply: u64, index: u64) -> Option<Value> {
    let mail = seller_mail(&deps.log).ok()?;
    let record = mail.replies.iter().find(|record| record.reply == reply)?;
    let file = record
        .received
        .attachments
        .get(usize::try_from(index.checked_sub(1)?).ok()?)?;
    let media = file.media_type.as_ref()?.to_string();
    let extension = extension_of(&media)?;
    if !file.kept {
        return None;
    }
    let path = reply_folder(deps, reply, record.received_at)?.join(format!("{index}.{extension}"));
    let bytes = std::fs::read(path).ok()?;
    Some(json!({
        "media_type": media,
        "base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        "name": format!("{reply}-{index}.{extension}"),
    }))
}

/// `seller_reply_dismiss`: the owner dismisses reply `reply` on Today; it stays kept.
///
/// # Errors
///
/// `unknown_seller_reply`, `seller_reply_dismissed`.
pub(crate) fn dismiss_reply(deps: &ToolDeps, reply: u64) -> Result<u64, MailboxRefusal> {
    let _held = crate::locked(&MAIL);
    let mail = seller_mail(&deps.log).map_err(mail_failed)?;
    let record = mail
        .replies
        .iter()
        .find(|record| record.reply == reply)
        .ok_or_else(|| MailboxRefusal {
            code: "unknown_seller_reply",
            words: "There is no such reply.".to_string(),
        })?;
    if record.dismissed {
        return Err(MailboxRefusal {
            code: "seller_reply_dismissed",
            words: "You dismissed this reply already.".to_string(),
        });
    }
    let number = std::num::NonZeroU64::new(reply)
        .map(Into::into)
        .ok_or_else(|| mail_failed("a reply is numbered from 1"))?;
    record_unattended(
        deps,
        EventBody::SellerReplyDismissed(SellerReplyDismissedBody { reply: number }),
        None,
    )
    .map_err(mail_failed)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Duration;
    use farik_protocol::clock::MovableClock;
    use farik_protocol::command::Command;
    use farik_protocol::event::{EventBody, EventKind};
    use farik_store::seller_mail::seller_mail;
    use serde_json::{Value, json};

    use super::{
        Mailer, check_now, finished_checks, known_of, reply_attachment, seller_replies_list,
        start_check,
    };
    use crate::greenmail::{BUYING, Mime};
    use crate::mailbox::MailboxSecrets as _;
    use crate::orchestrator::{TickRules, TickScope};
    use crate::procurement::story::{DANA, Story, reason, secret};
    use crate::tools::fixtures::at;

    const SUBJECT: &str = "Quote for 500 printed pie boxes";
    const BODY: &str = "Hello,\n\nCould you quote 500 printed pie boxes, 9 inch?\n\nThank you.";

    /// Dana's answer to the message with `Message-ID` `id`.
    fn answer<'a>(id: &'a str, message_id: &'a str, subject: &'a str) -> Mime<'a> {
        Mime {
            from: "Dana Reyes <sales@pieboxpros.test>",
            to: BUYING.address,
            subject,
            message_id,
            in_reply_to: Some(id),
            text: Some("Hello,\r\n\r\n500 boxes are 0.38 each.\r\n\r\nDana"),
            html: None,
            attachments: Vec::new(),
        }
    }

    /// Ivo's quote request, drafted and sent: answers its number and `Message-ID`.
    async fn sent(story: &Story) -> (u64, String) {
        let message = story.draft(SUBJECT, BODY);
        story.send(message, SUBJECT, BODY).await.expect("sent");
        let id = story
            .events(&[EventKind::SellerMessageSent])
            .iter()
            .find_map(|event| match &event.body {
                EventBody::SellerMessageSent(body) if body.message.get() == message => {
                    Some(body.message_id.to_string())
                }
                _ => None,
            })
            .expect("a send");
        (message, id)
    }

    fn mailer(story: &Story) -> Mailer<'_> {
        Mailer {
            secrets: &*story.store,
            at: story.at.clone(),
            trust: crate::mailbox::Trust::Root(story.fixture.ca_der.clone()),
        }
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

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn records_a_reply_and_keeps_its_files() {
        let story = Story::new("records").await;
        let (message, id) = sent(&story).await;
        let pdf = b"%PDF-1.7 the quote".to_vec();
        story.fixture.deliver(
            BUYING.address,
            &Mime {
                attachments: vec![
                    ("quote.pdf", "application/pdf", pdf.clone()),
                    (
                        "notes.exe",
                        "application/octet-stream",
                        b"MZ a program".to_vec(),
                    ),
                ],
                ..answer(
                    &id,
                    "r1@pieboxpros.test",
                    "Re: Quote for 500 printed pie boxes",
                )
            }
            .build(),
        );
        let deps = &story.harness.project.deps;
        let recorded = check_now(deps, &mailer(&story)).await.expect("checked");
        assert_eq!(recorded, 1);

        let events = story.events(&[EventKind::SellerReplyReceived]);
        assert_eq!(events.len(), 1);
        let ids = &events[0].envelope.ids;
        assert_eq!(
            (&ids.agent_id, &ids.session_id),
            (&None, &None),
            "Farik's own"
        );
        let EventBody::SellerReplyReceived(body) = &events[0].body else {
            panic!("a reply");
        };
        assert_eq!((body.reply.get(), body.message.get()), (1, message));
        assert_eq!(body.from.as_str(), "Dana Reyes <sales@pieboxpros.test>");
        assert_eq!(body.subject.as_str(), "Re: Quote for 500 printed pie boxes");
        assert!(body.date.is_some());
        let kept: Vec<(&str, bool, Option<String>, u64)> = body
            .attachments
            .iter()
            .map(|file| {
                (
                    file.name.as_str(),
                    file.kept,
                    file.media_type.as_ref().map(ToString::to_string),
                    file.bytes,
                )
            })
            .collect();
        assert_eq!(
            kept,
            [
                (
                    "quote.pdf",
                    true,
                    Some("application/pdf".to_string()),
                    pdf.len() as u64
                ),
                ("notes.exe", false, None, 12),
            ]
        );
        // The text and the one kept file are on disk, named by their bytes and never by the seller.
        let month = deps.clock.now().format("%Y-%m").to_string();
        let folder = story
            .harness
            .procurement_folder()
            .join("mail/in")
            .join(&month)
            .join("1");
        assert_eq!(files_in(&folder), ["1.pdf", "text.txt"]);
        assert_eq!(std::fs::read(folder.join("1.pdf")).expect("the pdf"), pdf);
        assert!(
            std::fs::read_to_string(folder.join("text.txt"))
                .expect("the text")
                .contains("500 boxes are 0.38 each.")
        );
        // The ledger says when, and that nothing failed; the reply is still unseen by others.
        let ledger = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert_eq!(ledger.checked_at, Some(deps.clock.now()));
        assert!(ledger.error.is_none());
        assert_eq!(story.fixture.unseen(&BUYING), 1);
        // A second check records nothing.
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 0);
        assert_eq!(story.events(&[EventKind::SellerReplyReceived]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn numbers_a_reply_after_the_highest_in_the_log_and_the_folders() {
        let story = Story::new("numbers").await;
        let (_, id) = sent(&story).await;
        let deps = &story.harness.project.deps;
        let month = deps.clock.now().format("%Y-%m").to_string();
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "r1@pieboxpros.test", "Re: Quote").build(),
        );
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);

        // A reply's number is one more than the highest the log and the folders hold.
        std::fs::create_dir_all(story.harness.procurement_folder().join("mail/in/2026-01/7"))
            .expect("a folder left by an older reply");
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "r2@pieboxpros.test", "Re: Quote again").build(),
        );
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);
        let numbers: Vec<u64> = story
            .events(&[EventKind::SellerReplyReceived])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::SellerReplyReceived(body) => Some(body.reply.get()),
                _ => None,
            })
            .collect();
        assert_eq!(numbers, [1, 8]);
        assert!(
            story
                .harness
                .procurement_folder()
                .join("mail/in")
                .join(&month)
                .join("8/text.txt")
                .exists(),
            "the folder is named by the reply's number, not by its place in the mailbox"
        );
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn checks_every_fifteen_minutes_without_a_session() {
        let story = Story::new("ticks").await;
        let (_, id) = sent(&story).await;
        let adapter = story.harness.recorded(Vec::new());
        let clock = Arc::new(MovableClock::new(at()));
        let orchestrator = story
            .harness
            .orchestrator_on(adapter.clone(), Arc::clone(&clock));
        // Ticks that act on triage alone: the task in progress is left alone, so that a session
        // starting would be the mailbox's doing.
        let unscoped = TickScope {
            task_id: None,
            rules: TickRules::Refining,
        };
        let replies = || story.events(&[EventKind::SellerReplyReceived]).len();
        let deliver = |n: u32| {
            story.fixture.deliver(
                BUYING.address,
                &answer(&id, &format!("r{n}@pieboxpros.test"), &format!("Re: {n}")).build(),
            );
        };

        // The first unscoped tick finds a mailbox never read, and checks.
        deliver(1);
        orchestrator.tick_within(&unscoped).await.expect("a tick");
        finished_checks().await;
        assert_eq!(replies(), 1);
        // Fourteen minutes later it does not; fifteen later it does.
        deliver(2);
        clock.set(at() + Duration::minutes(14));
        orchestrator.tick_within(&unscoped).await.expect("a tick");
        finished_checks().await;
        assert_eq!(replies(), 1, "not yet fifteen minutes");
        clock.set(at() + Duration::minutes(15));
        orchestrator.tick_within(&unscoped).await.expect("a tick");
        finished_checks().await;
        assert_eq!(replies(), 2);
        // A paused team's tick checks too: it starts no session and costs no model.
        story
            .harness
            .project
            .record("", "team.paused", &json!({ "by": "human" }));
        deliver(3);
        clock.set(at() + Duration::minutes(30));
        orchestrator.tick_within(&unscoped).await.expect("a tick");
        finished_checks().await;
        assert_eq!(replies(), 3, "a paused team still checks the mailbox");
        // A tick scoped to a task does not.
        deliver(4);
        clock.set(at() + Duration::minutes(45));
        let scope = TickScope {
            task_id: Some("FRK-1".parse().expect("a task id")),
            rules: TickRules::Refining,
        };
        orchestrator.tick_within(&scope).await.expect("a tick");
        finished_checks().await;
        assert_eq!(replies(), 3, "a tick about one task reads no mail");
        assert!(adapter.started().is_empty(), "no check starts a session");

        // `procurement_mailbox.check` reads at once, however recently it did.
        let deps = &story.harness.project.deps;
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);
        assert_eq!(replies(), 4);
        // A check that fails says why in Farik's words; the next good one clears it.
        story
            .store
            .save(&story.at, &secret("changed-since"))
            .expect("saved");
        deliver(5);
        let refused = check_now(deps, &mailer(&story))
            .await
            .expect_err("not signed in");
        assert_eq!(refused.code, "mailbox_login_failed");
        let ledger = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert_eq!(
            ledger.error.as_deref(),
            Some(refused.words.as_str()),
            "the page shows the same words"
        );
        let state = crate::procurement::mailbox_state(deps).expect("state");
        assert_eq!(state["error"], refused.words);
        story
            .store
            .save(&story.at, &secret(BUYING.password))
            .expect("saved");
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);
        assert!(
            crate::procurement::mailbox_ledger(deps)
                .expect("a ledger")
                .error
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn replies_list_and_dismiss() {
        let story = Story::new("list").await;
        let (message, id) = sent(&story).await;
        let png = b"\x89PNG\r\n\x1a\n a picture".to_vec();
        story.fixture.deliver(
            BUYING.address,
            &Mime {
                attachments: vec![
                    ("shot.png", "image/png", png.clone()),
                    ("tool.exe", "application/octet-stream", b"MZ".to_vec()),
                ],
                ..answer(&id, "r1@pieboxpros.test", "Re: Quote")
            }
            .build(),
        );
        let deps = &story.harness.project.deps;
        check_now(deps, &mailer(&story)).await.expect("checked");

        let listed = seller_replies_list(deps).expect("the list");
        let row = &listed["replies"][0];
        assert_eq!(row["reply"], 1);
        assert_eq!(row["message"], message);
        assert_eq!(row["task_id"], "FRK-1");
        assert_eq!(row["seller"], "Pie Box Pros");
        assert_eq!(row["sent_subject"], SUBJECT);
        assert_eq!(row["from"], "Dana Reyes <sales@pieboxpros.test>");
        assert_eq!(row["subject"], "Re: Quote");
        assert!(row["text"].as_str().expect("text").contains("0.38 each"));
        assert!(row["date"].is_string() && row["received_at"].is_string());
        assert_eq!(row["dismissed"], false);
        assert!(
            row.get("order").is_none(),
            "a quote request's reply holds no order"
        );
        assert_eq!(
            row["attachments"],
            json!([
                { "index": 1, "name": "shot.png", "kept": true, "media_type": "image/png",
                  "bytes": png.len() },
                { "index": 2, "name": "tool.exe", "kept": false, "bytes": 2 },
            ])
        );

        // A kept attachment's bytes, in a name made from the reply and the index; none for one that
        // was not kept, and none for a reply nobody received.
        let file = reply_attachment(deps, 1, 1).expect("the picture");
        assert_eq!(file["media_type"], "image/png");
        assert_eq!(file["name"], "1-1.png");
        assert_eq!(base64_decode(file["base64"].as_str().expect("base64")), png);
        assert!(
            reply_attachment(deps, 1, 0).is_none(),
            "attachments count from 1"
        );
        assert!(reply_attachment(deps, 1, 2).is_none(), "not kept");
        assert!(reply_attachment(deps, 1, 3).is_none(), "no such index");
        assert!(reply_attachment(deps, 9, 1).is_none(), "no such reply");

        // Dismissing records it once.
        let orchestrator = story.orchestrator();
        let report = orchestrator
            .handle(Command::SellerReplyDismiss { reply: 1 })
            .await
            .expect("dismissed");
        assert_eq!(report.events.len(), 1);
        assert_eq!(
            seller_replies_list(deps).expect("list")["replies"][0]["dismissed"],
            true
        );
        let again = orchestrator
            .handle(Command::SellerReplyDismiss { reply: 1 })
            .await
            .expect_err("dismissed already");
        assert!(reason(again).starts_with("seller_reply_dismissed: "));
        let unknown = orchestrator
            .handle(Command::SellerReplyDismiss { reply: 9 })
            .await
            .expect_err("no such reply");
        assert!(reason(unknown).starts_with("unknown_seller_reply: "));
        assert_eq!(story.events(&[EventKind::SellerReplyDismissed]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_reply_to_an_order_s_message_says_so() {
        let story = Story::new("order-reply").await;
        story.harness.project.record_in(
            Some("proc"),
            Some("session-1"),
            "FRK-1",
            "purchase_order.drafted",
            &json!({
                "order": 12, "seller": "Pie Box Pros", "seller_contact": "",
                "lines": [{ "item": "Pie box", "quantity": 500, "unit": "piece",
                    "unit_price": "0.40", "line_total": "200.00" }],
                "currency": "USD", "period": "once", "total": "200.00", "delivery": "",
                "terms": "", "url": "", "evaluation": "evaluations/boxes.md",
                "why": "It is the cheapest seller that ships to us."
            }),
        );
        // A follow-up: a question about a placed order, sent, and answered.
        story.harness.project.record(
            "FRK-1",
            "purchase_order.approved",
            &json!({ "order": 12, "note": "" }),
        );
        story.harness.project.record(
            "FRK-1",
            "purchase_order.placed",
            &json!({ "order": 12, "placed_on": "2026-09-28" }),
        );
        let message = story.draft_as(json!({
            "seller": "Pie Box Pros", "to": DANA.address, "subject": "Where is PO-12?",
            "body": "Has it shipped?", "purpose": "question", "purchase_order": 12
        }));
        story
            .send(message, "Where is PO-12?", "Has it shipped?")
            .await
            .expect("sent");
        let id = story
            .events(&[EventKind::SellerMessageSent])
            .iter()
            .find_map(|event| match &event.body {
                EventBody::SellerMessageSent(body) => Some(body.message_id.to_string()),
                _ => None,
            })
            .expect("a send");
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "r1@pieboxpros.test", "Re: Where is PO-12?").build(),
        );
        let deps = &story.harness.project.deps;
        check_now(deps, &mailer(&story)).await.expect("checked");
        let listed = seller_replies_list(deps).expect("the list");
        assert_eq!(listed["replies"][0]["order"], 12);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_reply_with_no_reference_goes_to_the_latest_message_sent_to_its_sender() {
        let story = Story::new("latest").await;
        let (first, _) = sent(&story).await;
        let (second, _) = sent(&story).await;
        assert_eq!((first, second), (1, 2));
        story.fixture.deliver(
            BUYING.address,
            &Mime {
                in_reply_to: None,
                ..answer("unused", "r1@pieboxpros.test", "Quote?")
            }
            .build(),
        );
        let deps = &story.harness.project.deps;
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);
        let listed = seller_replies_list(deps).expect("the list");
        assert_eq!(listed["replies"][0]["message"], second);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn at_most_one_check_runs_at_a_time() {
        let story = Story::new("one-at-a-time").await;
        let (_, id) = sent(&story).await;
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "r1@pieboxpros.test", "Re: Quote").build(),
        );
        let deps = &story.harness.project.deps;
        assert!(
            start_check(deps, &story.harness.daemon),
            "a check is due and starts"
        );
        assert!(!start_check(deps, &story.harness.daemon), "one is running");
        finished_checks().await;
        assert_eq!(story.events(&[EventKind::SellerReplyReceived]).len(), 1);
        assert!(
            !start_check(deps, &story.harness.daemon),
            "it was read a moment ago, so none is due"
        );
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_reply_goes_to_the_message_it_names() {
        let story = Story::new("names").await;
        // Two quote requests to Dana, each with a `Message-ID` of its own.
        for subject in ["First", "Second"] {
            let message = story.draft(subject, BODY);
            story.send(message, subject, BODY).await.expect("sent");
        }
        let sent_ids: Vec<(u64, String)> = story
            .events(&[EventKind::SellerMessageSent])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::SellerMessageSent(body) => {
                    Some((body.message.get(), body.message_id.to_string()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(sent_ids.len(), 2);
        assert_ne!(sent_ids[0].1, sent_ids[1].1, "each message has its own id");
        // Dana answers the first and then the second: the earlier message is not passed over for
        // the latest one sent to her.
        story.fixture.deliver(
            BUYING.address,
            &answer(&sent_ids[0].1, "r1@pieboxpros.test", "Re: First").build(),
        );
        story.fixture.deliver(
            BUYING.address,
            &answer(&sent_ids[1].1, "r2@pieboxpros.test", "Re: Second").build(),
        );
        let deps = &story.harness.project.deps;
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 2);
        let listed = seller_replies_list(deps).expect("the list");
        let rows: Vec<(u64, &str, &str)> = listed["replies"]
            .as_array()
            .expect("replies")
            .iter()
            .map(|row| {
                (
                    row["message"].as_u64().expect("a message"),
                    row["sent_subject"].as_str().expect("a subject"),
                    row["subject"].as_str().expect("a subject"),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [(1, "First", "Re: First"), (2, "Second", "Re: Second")]
        );
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn known_holds_only_what_was_sent() {
        let story = Story::new("known").await;
        let (_, id) = sent(&story).await;
        // A message to another address that waits: Farik wrote nothing to that address yet, so a
        // message from it is none of Farik's to open.
        story.draft_as(json!({
            "seller": "Other Boxes", "to": "other@sellers.test", "subject": "Quote",
            "body": BODY, "purpose": "quote_request"
        }));
        let deps = &story.harness.project.deps;
        let settings = crate::procurement::mailbox_settings(deps).expect("settings");
        let known = known_of(&seller_mail(&deps.log).expect("the mail"), &settings);
        assert_eq!(known.written_to, [DANA.address]);
        assert_eq!(known.sent_ids, [id]);
        assert_eq!(known.address, BUYING.address);
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_renumbered_mailbox_says_so() {
        let story = Story::new("renumbered").await;
        let deps = &story.harness.project.deps;
        let now = deps.clock.now();
        let ledger = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert!(ledger.restarted_at.is_none());
        // The provider renumbered the folder since the ledger was kept.
        let stale = crate::mailbox::Ledger {
            uidvalidity: ledger.uidvalidity + 1,
            last_uid: 0,
            ..ledger.clone()
        };
        std::fs::write(
            story.harness.procurement_folder().join("mail/ledger.json"),
            serde_json::to_string(&stale).expect("a ledger"),
        )
        .expect("kept");
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 0);
        let after = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert_eq!(after.uidvalidity, ledger.uidvalidity, "started again");
        assert_eq!(after.restarted_at, Some(now));
        let state = crate::procurement::mailbox_state(deps).expect("state");
        assert_eq!(
            state["restarted_at"],
            json!(now),
            "the agent\u{2019}s page says the provider renumbered the mailbox"
        );
    }

    /// The words a reply that cannot be kept leaves in the ledger and the refusal.
    const COULD_NOT_KEEP: &str =
        "Farik could not keep a reply from the mailbox; it tries again in 15 minutes.";

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    async fn a_reply_that_cannot_be_kept_neither_repeats_nor_blocks() {
        let story = Story::new("unkeepable").await;
        let (_, id) = sent(&story).await;
        let deps = &story.harness.project.deps;
        let before = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        // A good reply, one with no From header at all, one with 101 attachments, a good one.
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "a@pieboxpros.test", "Re: A").build(),
        );
        story.fixture.deliver(
            BUYING.address,
            &format!(
                "To: buying@bakery.test\r\nSubject: Re: no sender\r\nMessage-ID: <n@pieboxpros.test>\r\n\
                 Date: Mon, 05 Oct 2026 10:00:00 +0000\r\nIn-Reply-To: <{id}>\r\nReferences: <{id}>\r\n\
                 \r\nWho am I?\r\n"
            ),
        );
        let names: Vec<String> = (1..=101).map(|n| format!("file{n}.txt")).collect();
        story.fixture.deliver(
            BUYING.address,
            &Mime {
                attachments: names
                    .iter()
                    .map(|name| (name.as_str(), "text/plain", b"x".to_vec()))
                    .collect(),
                ..answer(&id, "many@pieboxpros.test", "Re: many files")
            }
            .build(),
        );
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "b@pieboxpros.test", "Re: B").build(),
        );

        // One check records all four, and passes them.
        assert_eq!(
            check_now(deps, &mailer(&story)).await.expect("checked"),
            4,
            "no reply stops the ones after it"
        );
        let received = || -> Vec<(String, String, usize)> {
            story
                .events(&[EventKind::SellerReplyReceived])
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::SellerReplyReceived(body) => Some((
                        body.subject.to_string(),
                        body.from.to_string(),
                        body.attachments.len(),
                    )),
                    _ => None,
                })
                .collect()
        };
        let subject = |at: usize| received()[at].0.clone();
        assert_eq!(received().len(), 4);
        assert_eq!(subject(0), "Re: A");
        assert_eq!(received()[1].1, "unknown sender", "a reply with no sender");
        assert_eq!(received()[2].2, 100, "at most a hundred files are listed");
        assert_eq!(subject(3), "Re: B");
        let after = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert_eq!(after.last_uid, before.last_uid + 4);
        assert!(after.error.is_none());
        let month = deps.clock.now().format("%Y-%m").to_string();
        let inbox = story.harness.procurement_folder().join("mail/in");
        assert_eq!(
            files_in(&inbox.join(&month)),
            ["1", "2", "3", "4"],
            "one folder for each reply recorded"
        );
        // A second check reads none of them again.
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 0);
        assert_eq!(received().len(), 4);

        // A reply Farik cannot keep (its folder cannot be made): the ledger says so in Farik's words
        // and stops just before it; the refusal repeats those words and not the store's detail.
        std::fs::remove_dir_all(&inbox).expect("the folder goes");
        std::fs::write(&inbox, b"not a folder").expect("a file in its place");
        story.fixture.deliver(
            BUYING.address,
            &answer(&id, "c@pieboxpros.test", "Re: C").build(),
        );
        let refused = check_now(deps, &mailer(&story))
            .await
            .expect_err("C cannot be kept");
        assert_eq!(
            (refused.code, refused.words.as_str()),
            ("mailbox_files", COULD_NOT_KEEP)
        );
        assert_eq!(received().len(), 4, "nothing was recorded");
        let stuck = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert_eq!(stuck.checked_at, Some(deps.clock.now()));
        assert_eq!(stuck.error.as_deref(), Some(COULD_NOT_KEEP));
        assert_eq!(
            stuck.last_uid, after.last_uid,
            "just before the reply that was not kept"
        );
        let state = crate::procurement::mailbox_state(deps).expect("state");
        assert_eq!(
            state["error"], COULD_NOT_KEEP,
            "the page shows the same words"
        );
        // The folder comes back: the next check records C once, and clears the error.
        std::fs::remove_file(&inbox).expect("the file goes");
        assert_eq!(check_now(deps, &mailer(&story)).await.expect("checked"), 1);
        assert_eq!(received().len(), 5);
        assert_eq!(subject(4), "Re: C");
        let healed = crate::procurement::mailbox_ledger(deps).expect("a ledger");
        assert!(healed.error.is_none());
        assert_eq!(healed.last_uid, after.last_uid + 1);
    }

    fn base64_decode(text: &str) -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .expect("base64")
    }

    #[allow(dead_code)]
    fn unused(_: Value) {}
}
