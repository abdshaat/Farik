//! The procurement mailbox against a real IMAP and SMTP server (`GreenMail`, in Docker): logging in
//! to both over TLS with the certificate checked, and refusing a server that is not encrypted, a
//! certificate that cannot be trusted, a wrong password and a closed port. Ignored by default; run
//! by `cargo xtask check --integration`, which CI runs after pulling the image.
#![cfg(unix)]

#[path = "support/greenmail.rs"]
mod greenmail;
#[path = "support/ports.rs"]
mod ports;

use farik_runtime::claude::Secret;
use farik_runtime::mailbox::{
    Known, MailboxError, MailboxSettings, Provider, Security, Server, Trust, check_login,
    fetch_replies,
};
use greenmail::{Account, BUYING, GreenMail, Mime};

fn server(port: u16, security: Security) -> Server {
    Server {
        host: "localhost".to_string(),
        port,
        security,
    }
}

/// Settings for `fixture`'s TLS ports.
fn settings(fixture: &GreenMail) -> MailboxSettings {
    MailboxSettings {
        address: BUYING.address.to_string(),
        name: "Sam Ortiz".to_string(),
        provider: Provider::Other,
        imap: server(fixture.imaps, Security::Tls),
        smtp: server(fixture.smtps, Security::Tls),
        username: BUYING.login.to_string(),
        folder: "INBOX".to_string(),
        signature: String::new(),
        disclose_ai: true,
    }
}

fn secret(word: &str) -> Secret {
    Secret::new(word.to_string())
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn logs_in_to_both_servers_and_starts_after_the_last_message() {
    let fixture = GreenMail::start("login", &[&BUYING]);
    let trust = Trust::Root(fixture.ca_der.clone());
    let ledger = check_login(&settings(&fixture), &secret(BUYING.password), &trust)
        .await
        .expect("both logins pass");
    assert_eq!(ledger.last_uid, 0, "an empty mailbox: the next UID is 1");
    assert!(ledger.uidvalidity > 0);
    assert!(ledger.checked_at.is_none() && ledger.error.is_none());

    // Mail already there is older than the connection, and is never read.
    fixture.deliver(
        BUYING.address,
        "From: a@sellers.test\r\nTo: buying@bakery.test\r\nSubject: old\r\n\r\nold\r\n",
    );
    let ledger = check_login(&settings(&fixture), &secret(BUYING.password), &trust)
        .await
        .expect("both logins pass");
    assert_eq!(ledger.last_uid, 1);
    assert_eq!(
        fixture.inbox(&BUYING).len(),
        1,
        "logging in sent and changed nothing"
    );
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn refuses_a_server_without_tls_and_a_certificate_it_cannot_trust() {
    let fixture = GreenMail::start("tls", &[&BUYING]);
    let word = secret(BUYING.password);
    // The plain IMAP port offers no STARTTLS: the login is never sent.
    let mut plain = settings(&fixture);
    plain.imap = server(fixture.imap, Security::StartTls);
    let trust = Trust::Root(fixture.ca_der.clone());
    assert!(matches!(
        check_login(&plain, &word, &trust).await,
        Err(MailboxError::NeedsTls)
    ));
    // Implicit TLS on a plain port is no TLS either.
    plain.imap = server(fixture.imap, Security::Tls);
    assert!(matches!(
        check_login(&plain, &word, &trust).await,
        Err(MailboxError::NeedsTls)
    ));
    // The same for sending.
    let mut plain = settings(&fixture);
    plain.smtp = server(fixture.smtp, Security::StartTls);
    assert!(matches!(
        check_login(&plain, &word, &trust).await,
        Err(MailboxError::NeedsTls)
    ));
    // The fixture's own certificate is not the platform's to trust, and nothing turns the check off.
    assert!(matches!(
        check_login(&settings(&fixture), &word, &Trust::Platform).await,
        Err(MailboxError::Certificate)
    ));
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn a_wrong_password_is_a_login_failure_at_either_server() {
    let fixture = GreenMail::start("wrong", &[&BUYING]);
    let trust = Trust::Root(fixture.ca_der.clone());
    let wrong = secret("not-the-word");
    assert!(matches!(
        check_login(&settings(&fixture), &wrong, &trust).await,
        Err(MailboxError::Login)
    ));
    // The IMAP login passes and the SMTP one is the one refused: the sign-in name is right for
    // IMAP only when the whole settings are; here SMTP's server is another fixture's port.
    let mut closed = settings(&fixture);
    closed.smtp = server(ports::free_port(), Security::Tls);
    assert!(matches!(
        check_login(&closed, &secret(BUYING.password), &trust).await,
        Err(MailboxError::Unreachable)
    ));
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn a_wrong_password_at_the_sending_server_alone_is_a_login_failure() {
    // Reading is one server and sending another that knows the same sign-in name by another
    // password, so only the SMTP login can be the one that fails.
    let reading = GreenMail::start("reads", &[&BUYING]);
    let other = greenmail::Account {
        login: "buying",
        password: "another-word",
        address: "buying@bakery.test",
    };
    let sending = GreenMail::start_beside("sends", &[&other], &reading);
    let mut split = settings(&reading);
    split.smtp = server(sending.smtps, Security::Tls);
    let trust = Trust::Root(reading.ca_der.clone());
    assert!(matches!(
        check_login(&split, &secret(BUYING.password), &trust).await,
        Err(MailboxError::Login)
    ));
    // With the sending server's password too, nothing is wrong.
    assert!(
        check_login(&split, &secret("another-word"), &trust)
            .await
            .is_err(),
        "the reading server does not know it"
    );
}

/// Dana of Pie Box Pros: the seller of the story, whose mailbox the fixture holds.
const DANA: Account = Account {
    login: "sales",
    password: "seller-word",
    address: "sales@pieboxpros.test",
};

/// What Farik knows of the mail it sent: one message, `m1`, to Dana.
fn known() -> Known {
    Known {
        address: BUYING.address.to_string(),
        sent_ids: vec!["m1@bakery.test".to_string()],
        written_to: vec![DANA.address.to_string()],
    }
}

/// A reply of Dana's answering `m1`, addressed to the procurement address.
fn dana_replies<'a>(id: &'a str, subject: &'a str) -> Mime<'a> {
    Mime {
        from: "Dana Reyes <sales@pieboxpros.test>",
        to: BUYING.address,
        subject,
        message_id: id,
        in_reply_to: Some("m1@bakery.test"),
        text: Some("Hello,\r\n\r\n500 boxes are 0.38 each.\r\n\r\nDana"),
        html: None,
        attachments: Vec::new(),
    }
}

/// A fixture with the story's two accounts, the mailbox connected, and its first ledger.
async fn connected(
    test: &str,
) -> (
    GreenMail,
    MailboxSettings,
    Trust,
    farik_runtime::mailbox::Ledger,
) {
    let fixture = GreenMail::start(test, &[&BUYING, &DANA]);
    let trust = Trust::Root(fixture.ca_der.clone());
    let settings = settings(&fixture);
    let ledger = check_login(&settings, &secret(BUYING.password), &trust)
        .await
        .expect("the mailbox connects");
    (fixture, settings, trust, ledger)
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn reads_a_reply_to_a_sent_message() {
    let (fixture, settings, trust, ledger) = connected("reads").await;
    let pdf = b"%PDF-1.7 the quote".to_vec();
    fixture.deliver(
        BUYING.address,
        &Mime {
            attachments: vec![("quote.pdf", "application/pdf", pdf.clone())],
            ..dana_replies("r1@pieboxpros.test", "Re: Quote for 500 printed pie boxes")
        }
        .build(),
    );
    // An HTML-only reply: its words are the text, and no tag.
    fixture.deliver(
        BUYING.address,
        &Mime {
            text: None,
            html: Some(
                "<html><body><p>We can do <b>0.35</b> a box.</p><script>x()</script></body></html>",
            ),
            ..dana_replies("r2@pieboxpros.test", "Re: Quote, better")
        }
        .build(),
    );
    let fetched = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &ledger,
        &known(),
    )
    .await
    .expect("the replies are read");
    assert_eq!(fetched.replies.len(), 2);
    assert_eq!(fetched.bodies.len(), 2, "both bodies were fetched");
    assert!(fetched.skipped.is_empty());
    let first = &fetched.replies[0];
    assert_eq!(first.from, "Dana Reyes <sales@pieboxpros.test>");
    assert_eq!(first.subject, "Re: Quote for 500 printed pie boxes");
    assert_eq!(first.answers, ["m1@bakery.test"]);
    assert!(
        first.text.contains("500 boxes are 0.38 each."),
        "{}",
        first.text
    );
    assert!(first.date.is_some());
    assert_eq!(first.attachments.len(), 1);
    assert_eq!(first.attachments[0].name, "quote.pdf");
    assert_eq!(
        first.attachments[0].media_type.as_deref(),
        Some("application/pdf")
    );
    assert_eq!(first.attachments[0].bytes.as_deref(), Some(pdf.as_slice()));
    let second = &fetched.replies[1];
    assert!(
        second.text.contains("We can do 0.35 a box."),
        "{}",
        second.text
    );
    assert!(
        !second.text.contains('<') && !second.text.contains("x()"),
        "{}",
        second.text
    );
    // Reading marked nothing: both are still unseen by another client.
    assert_eq!(fixture.unseen(&BUYING), 2);
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn never_reads_a_message_for_another_address() {
    let (fixture, settings, trust, ledger) = connected("another").await;
    // Dana's answer, but delivered with the account's main address in To.
    fixture.deliver(
        BUYING.address,
        &Mime {
            to: "owner@bakery.test",
            ..dana_replies("o1@pieboxpros.test", "Re: to the owner")
        }
        .build(),
    );
    // Addressed to us, answering nothing, from an address never written to.
    fixture.deliver(
        BUYING.address,
        &Mime {
            from: "promo@elsewhere.test",
            in_reply_to: None,
            ..dana_replies("p1@elsewhere.test", "Win a prize")
        }
        .build(),
    );
    // Dana's answer, but over 25 MB.
    let huge = "x".repeat(26 * 1024 * 1024);
    fixture.deliver(
        BUYING.address,
        &Mime {
            text: Some(&huge),
            ..dana_replies("h1@pieboxpros.test", "Re: huge")
        }
        .build(),
    );
    // And one that is ours.
    fixture.deliver(
        BUYING.address,
        &dana_replies("r1@pieboxpros.test", "Re: ours").build(),
    );
    let fetched = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &ledger,
        &known(),
    )
    .await
    .expect("the replies are read");
    assert_eq!(fetched.replies.len(), 1);
    assert_eq!(fetched.replies[0].subject, "Re: ours");
    assert_eq!(fetched.bodies.len(), 1, "only one body was fetched");
    assert_eq!(
        fetched.skipped.len(),
        3,
        "three were passed on their headers"
    );
    // The ledger passes all four, so none is looked at again; none was marked read.
    assert_eq!(fetched.ledger.last_uid, ledger.last_uid + 4);
    assert_eq!(fixture.unseen(&BUYING), 4);
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn keeps_only_small_pdfs_and_pictures() {
    let (fixture, settings, trust, ledger) = connected("attachments").await;
    let mut big_pdf = b"%PDF-1.7 ".to_vec();
    big_pdf.resize(11 * 1024 * 1024, b'p');
    let png = b"\x89PNG\r\n\x1a\n a picture".to_vec();
    fixture.deliver(
        BUYING.address,
        &Mime {
            attachments: vec![
                ("big.pdf", "application/pdf", big_pdf),
                (
                    "setup.exe",
                    "application/octet-stream",
                    b"MZ a program".to_vec(),
                ),
                (
                    "quote.pdf",
                    "application/pdf",
                    b"MZ a program in a pdf's name".to_vec(),
                ),
                ("invoice.pdf", "application/pdf", png.clone()),
            ],
            ..dana_replies("r1@pieboxpros.test", "Re: files")
        }
        .build(),
    );
    let fetched = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &ledger,
        &known(),
    )
    .await
    .expect("the replies are read");
    let files: Vec<(&str, bool, Option<&str>)> = fetched.replies[0]
        .attachments
        .iter()
        .map(|file| {
            (
                file.name.as_str(),
                file.bytes.is_some(),
                file.media_type.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("big.pdf", false, None),
            ("setup.exe", false, None),
            ("quote.pdf", false, None),
            ("invoice.pdf", true, Some("image/png")),
        ]
    );
    assert_eq!(fetched.replies[0].attachments[0].size, 11 * 1024 * 1024);
    assert_eq!(
        fetched.replies[0].attachments[3].bytes.as_deref(),
        Some(png.as_slice())
    );
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn reads_each_reply_once() {
    let (fixture, settings, trust, ledger) = connected("once").await;
    fixture.deliver(
        BUYING.address,
        &dana_replies("r1@pieboxpros.test", "Re: once").build(),
    );
    let first = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &ledger,
        &known(),
    )
    .await
    .expect("read");
    assert_eq!(first.replies.len(), 1);
    assert!(!first.restarted);
    let again = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &first.ledger,
        &known(),
    )
    .await
    .expect("read again");
    assert!(again.replies.is_empty(), "it was read already");
    assert!(again.bodies.is_empty() && again.skipped.is_empty());
    assert_eq!(again.ledger, first.ledger);

    // The provider renumbered the folder: the ledger starts again at the next UID and reads
    // nothing older, and a later reply is read.
    let mut renumbered = first.ledger.clone();
    renumbered.uidvalidity += 1;
    renumbered.last_uid = 0;
    let restarted = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &renumbered,
        &known(),
    )
    .await
    .expect("read after a renumbering");
    assert!(restarted.restarted);
    assert!(restarted.replies.is_empty(), "nothing older is read");
    assert_eq!(restarted.ledger.uidvalidity, first.ledger.uidvalidity);
    assert_eq!(restarted.ledger.last_uid, first.ledger.last_uid);
    fixture.deliver(
        BUYING.address,
        &dana_replies("r2@pieboxpros.test", "Re: later").build(),
    );
    let later = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &restarted.ledger,
        &known(),
    )
    .await
    .expect("read");
    assert_eq!(later.replies.len(), 1);
    assert_eq!(later.replies[0].subject, "Re: later");
}

#[tokio::test]
#[ignore = "needs Docker and the GreenMail image: cargo xtask check --integration"]
async fn cuts_a_long_text_at_64_kib() {
    let (fixture, settings, trust, ledger) = connected("long").await;
    let long = format!("{}end", "\u{00e9}".repeat(40_000));
    fixture.deliver(
        BUYING.address,
        &Mime {
            text: Some(&long),
            ..dana_replies("r1@pieboxpros.test", "Re: long")
        }
        .build(),
    );
    let fetched = fetch_replies(
        &settings,
        &secret(BUYING.password),
        &trust,
        &ledger,
        &known(),
    )
    .await
    .expect("the replies are read");
    let text = &fetched.replies[0].text;
    assert!(
        text.len() <= 64 * 1024 && text.len() > 64 * 1024 - 4,
        "{}",
        text.len()
    );
    assert!(!text.ends_with("end"), "cut, not whole");
}
