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
    MailboxError, MailboxSettings, Provider, Security, Server, Trust, check_login,
};
use greenmail::{BUYING, GreenMail};

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
