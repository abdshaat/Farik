//! The procurement mailbox over IMAP and SMTP (`docs/SPEC.md` 6.6, 6.10; phase 7 step 10f): the
//! settings the owner connects, the providers Farik knows, and a login to both servers over TLS
//! whose certificate is always checked. The password is a [`Secret`] and is kept in the OS
//! keychain, never in these settings, an event, a refusal or the log. Phase 14's receipts intake
//! reuses this module.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_imap::Client;
use chrono::{DateTime, Utc};
use lettre::AsyncSmtpTransport;
use lettre::Tokio1Executor;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName};

use crate::claude::Secret;
use crate::connectors::SecretAt;
use crate::credential::CredentialError;

/// How long each step of talking to a server has.
const STEP: Duration = Duration::from_secs(30);

/// How a server is reached: encrypted from the first byte, or upgraded after connecting. Never
/// plain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    /// Encrypted from the start (IMAPS on 993, SMTPS on 465).
    #[serde(rename = "tls")]
    Tls,
    /// Upgraded with STARTTLS before any login; refused if the server does not upgrade.
    #[serde(rename = "starttls")]
    StartTls,
}

/// One server of a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    /// Its name or address.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// How it is made private.
    pub security: Security,
}

impl Server {
    /// A server typed by the owner: encrypted from the start on 993 and 465, STARTTLS on any other
    /// port.
    #[must_use]
    pub fn typed(host: &str, port: u16) -> Server {
        Server {
            host: host.to_string(),
            port,
            security: if matches!(port, 993 | 465) {
                Security::Tls
            } else {
                Security::StartTls
            },
        }
    }
}

/// A provider whose servers Farik knows, or "another provider", whose servers are typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Gmail and Google Workspace.
    Gmail,
    /// iCloud Mail.
    Icloud,
    /// Fastmail.
    Fastmail,
    /// Any other: the servers are typed.
    Other,
}

/// What the owner chose on the page: a provider, or Microsoft, which is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderChoice {
    /// Gmail and Google Workspace.
    Gmail,
    /// iCloud Mail.
    Icloud,
    /// Fastmail.
    Fastmail,
    /// Any other.
    Other,
    /// Outlook.com or Microsoft 365: not supported before Farik Cloud's sign-in.
    Microsoft,
}

impl ProviderChoice {
    /// The provider chosen, unless it is Microsoft.
    #[must_use]
    pub fn known(self) -> Option<Provider> {
        match self {
            Self::Gmail => Some(Provider::Gmail),
            Self::Icloud => Some(Provider::Icloud),
            Self::Fastmail => Some(Provider::Fastmail),
            Self::Other => Some(Provider::Other),
            Self::Microsoft => None,
        }
    }
}

/// What an address's domain says of its provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderOf {
    /// One whose servers Farik knows.
    Known(Provider),
    /// Microsoft's consumer mail, which no longer takes a password from a mail program.
    Microsoft,
    /// Not told from the domain.
    Unknown,
}

/// The provider an address is at, from its domain compared without regard to case.
#[must_use]
pub fn provider_of(address: &str) -> ProviderOf {
    let Some((_, domain)) = address.rsplit_once('@') else {
        return ProviderOf::Unknown;
    };
    match domain.to_ascii_lowercase().as_str() {
        "gmail.com" | "googlemail.com" => ProviderOf::Known(Provider::Gmail),
        "icloud.com" | "me.com" | "mac.com" => ProviderOf::Known(Provider::Icloud),
        "fastmail.com" | "fastmail.fm" => ProviderOf::Known(Provider::Fastmail),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => ProviderOf::Microsoft,
        _ => ProviderOf::Unknown,
    }
}

/// The reading and the sending servers of a known provider; none for "another provider".
#[must_use]
pub fn servers(provider: Provider) -> Option<(Server, Server)> {
    let server = |host: &str, port, security| Server {
        host: host.to_string(),
        port,
        security,
    };
    match provider {
        Provider::Gmail => Some((
            server("imap.gmail.com", 993, Security::Tls),
            server("smtp.gmail.com", 465, Security::Tls),
        )),
        Provider::Icloud => Some((
            server("imap.mail.me.com", 993, Security::Tls),
            server("smtp.mail.me.com", 587, Security::StartTls),
        )),
        Provider::Fastmail => Some((
            server("imap.fastmail.com", 993, Security::Tls),
            server("smtp.fastmail.com", 465, Security::Tls),
        )),
        Provider::Other => None,
    }
}

/// The mailbox the owner connected, as kept in `mail/mailbox.json`: everything but the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxSettings {
    /// The address mail is sent from and read for.
    pub address: String,
    /// The name sellers see, and the disclosure's.
    pub name: String,
    /// The provider chosen.
    pub provider: Provider,
    /// Where mail is read.
    pub imap: Server,
    /// Where mail is sent.
    pub smtp: Server,
    /// The sign-in name.
    pub username: String,
    /// The folder Farik reads.
    pub folder: String,
    /// What Farik adds under every message; empty for none.
    pub signature: String,
    /// Whether Farik says an AI assistant wrote the message.
    pub disclose_ai: bool,
}

/// A line of text with no control character.
fn is_one_line(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

fn is_server(server: &Server) -> bool {
    !server.host.is_empty()
        && server.host.len() <= 253
        && !server
            .host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        && server.port > 0
}

/// The first field of `settings` that does not fit, by name, or `Ok` when all do.
///
/// # Errors
///
/// The name of the field: `address`, `name`, `username`, `imap`, `smtp`, `folder` or `signature`.
pub fn validate_settings(settings: &MailboxSettings) -> Result<(), &'static str> {
    let characters = |text: &str| text.chars().count();
    let address = settings.address.parse::<lettre::Address>();
    if address.is_err() || settings.address.chars().any(char::is_whitespace) {
        return Err("address");
    }
    if !(1..=100).contains(&characters(&settings.name)) || !is_one_line(&settings.name) {
        return Err("name");
    }
    if !(1..=200).contains(&characters(&settings.username)) || !is_one_line(&settings.username) {
        return Err("username");
    }
    if !is_server(&settings.imap) {
        return Err("imap");
    }
    if !is_server(&settings.smtp) {
        return Err("smtp");
    }
    if !(1..=100).contains(&characters(&settings.folder)) || !is_one_line(&settings.folder) {
        return Err("folder");
    }
    if characters(&settings.signature) > 600
        || settings
            .signature
            .chars()
            .any(|c| c.is_control() && c != '\n')
    {
        return Err("signature");
    }
    Ok(())
}

/// Whose certificates a connection trusts. The product always uses `Platform`; `Root` gives a test
/// its certificate authority as a parameter, and nothing turns the check off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trust {
    /// The computer's own certificate store.
    Platform,
    /// The platform's, and this one certificate authority, DER.
    Root(Vec<u8>),
}

/// Where Farik has read to in a mailbox: kept in `mail/ledger.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    /// The folder's `UIDVALIDITY` when the last UID was taken.
    pub uidvalidity: u32,
    /// The last UID looked at; the next check starts after it.
    pub last_uid: u32,
    /// When the last check ended.
    pub checked_at: Option<DateTime<Utc>>,
    /// Why the last check failed, in Farik's words; none when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When the provider renumbered the folder and the ledger started again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restarted_at: Option<DateTime<Utc>>,
}

/// Why a server could not be used, in Farik's words. The words never quote the settings, the
/// server's reply or the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxError {
    /// The server does not offer an encrypted connection.
    NeedsTls,
    /// The provider did not accept the sign-in name and password.
    Login,
    /// The server's certificate cannot be trusted.
    Certificate,
    /// The server could not be reached in time.
    Unreachable,
    /// The server answered in a way Farik cannot use; the words say what, never the server's own.
    Server(String),
}

impl MailboxError {
    /// The refusal code the owner is shown.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NeedsTls => "mailbox_needs_tls",
            Self::Login => "mailbox_login_failed",
            Self::Certificate => "mailbox_certificate",
            Self::Unreachable => "mailbox_unreachable",
            Self::Server(_) => "mailbox_server",
        }
    }
}

impl MailboxError {
    /// Why a send failed, as a sentence for the log and the owner: Farik's, never the server's
    /// words.
    #[must_use]
    pub fn why(&self) -> String {
        match self {
            Self::NeedsTls => "the mail server does not offer an encrypted connection".to_string(),
            Self::Login => "the mailbox did not accept its sign-in; connect it again".to_string(),
            Self::Certificate => {
                "the mail server\u{2019}s certificate cannot be trusted".to_string()
            }
            Self::Unreachable => "the mail server could not be reached; try again".to_string(),
            Self::Server(why) => why.trim_end_matches('.').to_lowercase(),
        }
    }
}

impl std::fmt::Display for MailboxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsTls => formatter.write_str(
                "That server does not offer an encrypted connection, so Farik won\u{2019}t use it.",
            ),
            Self::Login => formatter.write_str(
                "Your provider did not accept that sign-in name and app password. A Microsoft \
                 mailbox can\u{2019}t be used yet.",
            ),
            Self::Certificate => formatter.write_str(
                "That server\u{2019}s certificate can\u{2019}t be trusted, so Farik won\u{2019}t use it.",
            ),
            Self::Unreachable => formatter
                .write_str("Farik could not reach that server. Check its name and port."),
            Self::Server(why) => formatter.write_str(why),
        }
    }
}

impl std::error::Error for MailboxError {}

/// Runs one step of talking to a server within [`STEP`].
async fn within<Done>(
    step: impl Future<Output = Result<Done, MailboxError>>,
) -> Result<Done, MailboxError> {
    tokio::time::timeout(STEP, step)
        .await
        .unwrap_or(Err(MailboxError::Unreachable))
}

/// What a failure of the TLS handshake means: a certificate that cannot be trusted, a peer that
/// does not speak TLS, or a connection that could not be made.
fn tls_failure(error: &(dyn std::error::Error + 'static)) -> MailboxError {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = current {
        let rustls = error.downcast_ref::<rustls::Error>().or_else(|| {
            error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::get_ref)
                .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        });
        match rustls {
            Some(rustls::Error::InvalidCertificate(_)) => return MailboxError::Certificate,
            Some(rustls::Error::InvalidMessage(_)) => return MailboxError::NeedsTls,
            _ => {}
        }
        current = error.source();
    }
    MailboxError::Unreachable
}

/// The TLS configuration for `trust`: the platform verifier with, for a test, one more root.
fn tls_connector(trust: &Trust) -> Result<tokio_rustls::TlsConnector, MailboxError> {
    let broken = || MailboxError::Server("Farik could not set up an encrypted connection.".into());
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let roots = match trust {
        Trust::Platform => Vec::new(),
        Trust::Root(der) => vec![CertificateDer::from(der.clone())],
    };
    let verifier =
        rustls_platform_verifier::Verifier::new_with_extra_roots(roots, Arc::clone(&provider))
            .map_err(|_| broken())?;
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| broken())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

async fn connect_tcp(server: &Server) -> Result<TcpStream, MailboxError> {
    within(async {
        TcpStream::connect((server.host.as_str(), server.port))
            .await
            .map_err(|_| MailboxError::Unreachable)
    })
    .await
}

async fn upgrade(
    stream: TcpStream,
    host: &str,
    trust: &Trust,
) -> Result<TlsStream<TcpStream>, MailboxError> {
    let name = ServerName::try_from(host.to_string()).map_err(|_| MailboxError::Unreachable)?;
    let connector = tls_connector(trust)?;
    within(async {
        connector
            .connect(name, stream)
            .await
            .map_err(|error| tls_failure(&error))
    })
    .await
}

/// A signed-in IMAP session over TLS.
pub(crate) type ImapSession = async_imap::Session<TlsStream<TcpStream>>;

fn imap_error(error: &async_imap::error::Error) -> MailboxError {
    use async_imap::error::Error;
    match error {
        Error::No(_) => MailboxError::Login,
        Error::Io(_) | Error::ConnectionLost => MailboxError::Unreachable,
        _ => MailboxError::Server(
            "The mail server answered in a way Farik can\u{2019}t use.".to_string(),
        ),
    }
}

/// Connects to the reading server, makes the connection private (refusing a server that does not
/// offer it before any login is sent), and signs in.
pub(crate) async fn imap_login(
    settings: &MailboxSettings,
    password: &Secret,
    trust: &Trust,
) -> Result<ImapSession, MailboxError> {
    let server = &settings.imap;
    let stream = connect_tcp(server).await?;
    let client = match server.security {
        Security::Tls => {
            let tls = upgrade(stream, &server.host, trust).await?;
            let mut client = Client::new(tls);
            greeting(&mut client).await?;
            client
        }
        Security::StartTls => {
            let mut plain = Client::new(stream);
            greeting(&mut plain).await?;
            // A server that does not know STARTTLS answers BAD or NO: the login is never sent.
            within(async {
                plain
                    .run_command_and_check_ok("STARTTLS", None)
                    .await
                    .map_err(|error| match error {
                        async_imap::error::Error::Io(_)
                        | async_imap::error::Error::ConnectionLost => MailboxError::Unreachable,
                        _ => MailboxError::NeedsTls,
                    })
            })
            .await?;
            let tls = upgrade(plain.into_inner(), &server.host, trust).await?;
            Client::new(tls)
        }
    };
    within(async {
        client
            .login(&settings.username, password.expose())
            .await
            .map_err(|(error, _)| imap_error(&error))
    })
    .await
}

/// Reads the server's greeting.
async fn greeting<Stream>(client: &mut Client<Stream>) -> Result<(), MailboxError>
where
    Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    match within(async {
        client
            .read_response()
            .await
            .map_err(|_| MailboxError::Unreachable)
    })
    .await?
    {
        Some(_) => Ok(()),
        None => Err(MailboxError::Unreachable),
    }
}

/// The SMTP transport for `settings`: encrypted from the start or upgraded, never plain, signing
/// in with `password`.
fn smtp_transport(
    settings: &MailboxSettings,
    password: &Secret,
    trust: &Trust,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, MailboxError> {
    let broken = || MailboxError::Server("Farik could not set up an encrypted connection.".into());
    let server = &settings.smtp;
    let mut parameters = TlsParameters::builder(server.host.clone());
    if let Trust::Root(der) = trust {
        let certificate = Certificate::from_der(der.clone()).map_err(|_| broken())?;
        parameters = parameters.add_root_certificate(certificate);
    }
    let parameters = parameters.build_rustls().map_err(|_| broken())?;
    let tls = match server.security {
        Security::Tls => Tls::Wrapper(parameters),
        Security::StartTls => Tls::Required(parameters),
    };
    Ok(
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&server.host)
            .port(server.port)
            .tls(tls)
            .credentials(Credentials::new(
                settings.username.clone(),
                password.expose().to_string(),
            ))
            .authentication(vec![Mechanism::Plain, Mechanism::Login])
            .timeout(Some(STEP))
            .build(),
    )
}

/// What a failure of the sending server means.
fn smtp_failure(error: &lettre::transport::smtp::Error) -> MailboxError {
    match tls_failure(error) {
        MailboxError::Unreachable => {}
        other => return other,
    }
    if error.is_permanent() {
        MailboxError::Login
    } else if error.is_client() && error.to_string().contains("STARTTLS") {
        MailboxError::NeedsTls
    } else if error.is_tls() {
        MailboxError::Certificate
    } else {
        MailboxError::Unreachable
    }
}

/// Logs in to both servers with `password` and sends nothing: the reading server's folder is
/// opened with `EXAMINE`, read-only, for its `UIDVALIDITY` and `UIDNEXT`, and the sending server is
/// signed in to and left. The ledger answered starts at `UIDNEXT - 1`, so older mail is never
/// read. Its `checked_at` is none.
///
/// # Errors
///
/// `NeedsTls` for a server that does not offer an encrypted connection, `Certificate`, `Login`,
/// `Unreachable`, or `Server` for an answer Farik cannot use.
pub async fn check_login(
    settings: &MailboxSettings,
    password: &Secret,
    trust: &Trust,
) -> Result<Ledger, MailboxError> {
    let mut session = imap_login(settings, password, trust).await?;
    let examined = within(async {
        session
            .examine(&settings.folder)
            .await
            .map_err(|error| match imap_error(&error) {
                MailboxError::Login => {
                    MailboxError::Server("Farik could not open that folder of the mailbox.".into())
                }
                other => other,
            })
    })
    .await;
    let _ = session.logout().await;
    let mailbox = examined?;
    let (Some(validity), Some(next)) = (mailbox.uid_validity, mailbox.uid_next) else {
        return Err(MailboxError::Server(
            "That mail server does not number its messages, so Farik can\u{2019}t read replies from it."
                .to_string(),
        ));
    };
    let transport = smtp_transport(settings, password, trust)?;
    match transport.test_connection().await {
        Ok(true) => {}
        Ok(false) => return Err(MailboxError::Unreachable),
        Err(error) => return Err(smtp_failure(&error)),
    }
    Ok(Ledger {
        uidvalidity: validity,
        last_uid: next.saturating_sub(1),
        checked_at: None,
        error: None,
        restarted_at: None,
    })
}

/// A message to send: one plain-text part, and an order's workbook when it is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    /// The seller's address.
    pub to: String,
    /// The subject, on one line.
    pub subject: String,
    /// The text, as the owner read it and with what Farik adds.
    pub text: String,
    /// The `Message-ID`, without angle brackets.
    pub message_id: String,
    /// The file name and bytes of the one attachment, an order's workbook.
    pub attachment: Option<(String, Vec<u8>)>,
}

/// What a failure of sending means (not of signing in, which [`smtp_failure`] reads): a refused
/// sign-in, a message the server will not take, or no server.
fn send_failure(error: &lettre::transport::smtp::Error) -> MailboxError {
    match tls_failure(error) {
        MailboxError::Unreachable => {}
        other => return other,
    }
    let code = error.status().map(|code| code.to_string());
    match (code.as_deref(), error.is_permanent(), error.is_transient()) {
        // 530, 534, 535 and 538 are the answers to a sign-in that was not accepted.
        (Some("530" | "534" | "535" | "538"), _, _) => MailboxError::Login,
        (_, true, _) => MailboxError::Server("The mail server refused the message.".to_string()),
        (_, _, true) => {
            MailboxError::Server("The mail server could not take the message just now.".to_string())
        }
        _ if error.is_client() && error.to_string().contains("STARTTLS") => MailboxError::NeedsTls,
        _ if error.is_tls() => MailboxError::Certificate,
        _ => MailboxError::Unreachable,
    }
}

/// Sends `message` from the mailbox `settings` name, as plain UTF-8 text From
/// `"<name> <address>"` (with the workbook attached when it carries one), over the encrypted
/// connection `settings.smtp` describes and the certificate `trust` allows.
///
/// # Errors
///
/// As [`check_login`] for the connection and the sign-in, and `Server` for a message the server
/// would not take.
pub async fn send(
    settings: &MailboxSettings,
    password: &Secret,
    trust: &Trust,
    message: &Outgoing,
) -> Result<(), MailboxError> {
    use lettre::AsyncTransport as _;
    use lettre::message::header::ContentType;
    use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};

    let broken = |why: &str| MailboxError::Server(why.to_string());
    let from = Mailbox::new(
        Some(settings.name.clone()),
        settings
            .address
            .parse()
            .map_err(|_| broken("The mailbox\u{2019}s address does not fit."))?,
    );
    let to = Mailbox::new(
        None,
        message
            .to
            .parse()
            .map_err(|_| broken("The seller\u{2019}s address does not fit."))?,
    );
    let builder = lettre::Message::builder()
        .from(from)
        .to(to)
        .subject(message.subject.clone())
        .message_id(Some(format!("<{}>", message.message_id)));
    let text = SinglePart::plain(message.text.clone());
    let built = match &message.attachment {
        None => builder.singlepart(text),
        Some((name, bytes)) => {
            let workbook = ContentType::parse(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            )
            .map_err(|_| broken("The workbook\u{2019}s type does not fit."))?;
            builder.multipart(
                MultiPart::mixed()
                    .singlepart(text)
                    .singlepart(Attachment::new(name.clone()).body(bytes.clone(), workbook)),
            )
        }
    }
    .map_err(|_| broken("The message could not be built."))?;
    let transport = smtp_transport(settings, password, trust)?;
    match tokio::time::timeout(STEP, transport.send(built)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(send_failure(&error)),
        Err(_) => Err(MailboxError::Unreachable),
    }
}

/// The most bytes of a message Farik reads: bigger ones are passed on their headers.
const MOST_MESSAGE_BYTES: u64 = 25 * 1024 * 1024;
/// The most bytes of a reply's text Farik keeps.
const MOST_TEXT_BYTES: usize = 64 * 1024;
/// The most bytes of one attachment Farik keeps.
const MOST_KEPT_BYTES: usize = 10 * 1024 * 1024;
/// The most attachments of one reply Farik lists; the rest are not (the log holds at most this
/// many for a reply, so a reply with more is listed by its first ones).
const MOST_ATTACHMENTS: usize = 100;
/// Who a reply is from when its `From` header names no one.
const UNKNOWN_SENDER: &str = "unknown sender";
/// The headers fetched of every new message, and nothing else of it.
const HEADER_FIELDS: &str = "BODY.PEEK[HEADER.FIELDS (FROM TO CC DELIVERED-TO MESSAGE-ID \
                             IN-REPLY-TO REFERENCES SUBJECT DATE)]";

/// What the headers of a message say, read before its body is: addresses are bare and `answers`
/// are `Message-ID`s without angle brackets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
    /// The sender's address.
    pub from: String,
    /// The `To` addresses.
    pub to: Vec<String>,
    /// The `Cc` addresses.
    pub cc: Vec<String>,
    /// The `Delivered-To` addresses.
    pub delivered_to: Vec<String>,
    /// The `In-Reply-To` ids.
    pub in_reply_to: Vec<String>,
    /// The `References` ids.
    pub references: Vec<String>,
    /// The size of the whole message, `RFC822.SIZE`.
    pub size: u64,
}

/// What Farik knows of the mail it sent: the procurement address, the `Message-ID` of every message
/// sent, and the addresses they were sent to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Known {
    /// The procurement mailbox's address.
    pub address: String,
    /// The `Message-ID`s of the messages sent.
    pub sent_ids: Vec<String>,
    /// The addresses the messages were sent to.
    pub written_to: Vec<String>,
}

/// An address with no angle brackets and no case: how two are compared.
fn bare(address: &str) -> String {
    address
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_ascii_lowercase()
}

/// Whether a message is one Farik reads: the procurement address is in its `To`, `Cc` or
/// `Delivered-To`, compared without regard to case; and it answers a message Farik sent (its
/// `In-Reply-To` or `References` holds one's `Message-ID`) or comes from an address Farik wrote to;
/// and it is at most 25 MB. This is the alias rule: the login reaches the whole mailbox, and
/// anything this refuses is passed on its headers and its body is never fetched.
#[must_use]
pub fn is_for_us(headers: &Headers, known: &Known) -> bool {
    if headers.size > MOST_MESSAGE_BYTES {
        return false;
    }
    let ours = bare(&known.address);
    let addressed = headers
        .to
        .iter()
        .chain(&headers.cc)
        .chain(&headers.delivered_to)
        .any(|address| bare(address) == ours);
    if !addressed {
        return false;
    }
    let answers = headers
        .in_reply_to
        .iter()
        .chain(&headers.references)
        .any(|id| known.sent_ids.iter().any(|sent| bare(sent) == bare(id)));
    let from = bare(&headers.from);
    answers || known.written_to.iter().any(|written| bare(written) == from)
}

/// One file attached to a reply. `bytes` and `media_type` are set only for one Farik keeps: a PDF,
/// a PNG or a JPEG, as its bytes say whatever its name does, of at most 10 MB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The seller's name for it: text, never a path.
    pub name: String,
    /// What its bytes are, when kept.
    pub media_type: Option<String>,
    /// Its bytes, when kept.
    pub bytes: Option<Vec<u8>>,
    /// How many bytes it has.
    pub size: u64,
}

/// A seller's message Farik read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Its UID in the folder.
    pub uid: u32,
    /// Who it is from: `Name <address>`, or the address; "unknown sender" when the message names
    /// no one.
    pub from: String,
    /// Its subject.
    pub subject: String,
    /// Its `Date`, when it has a valid one.
    pub date: Option<DateTime<Utc>>,
    /// The `Message-ID`s it answers, without brackets.
    pub answers: Vec<String>,
    /// Its text, converted from HTML when it is HTML alone, at most 64 KiB.
    pub text: String,
    /// Its attachments, in order; the first 100 only.
    pub attachments: Vec<Attachment>,
}

/// What one reading of the mailbox found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The replies, oldest first.
    pub replies: Vec<Reply>,
    /// The UIDs whose body was fetched.
    pub bodies: Vec<u32>,
    /// The UIDs passed on their headers.
    pub skipped: Vec<u32>,
    /// The ledger after this reading: every message looked at is passed.
    pub ledger: Ledger,
    /// Whether the folder was renumbered, so that the ledger started again at the next UID.
    pub restarted: bool,
}

/// The media type of `bytes` when Farik keeps them: a PDF, a PNG or a JPEG by their first bytes.
fn kept_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"%PDF-") {
        Some(("application/pdf", "pdf"))
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else {
        None
    }
}

/// The extension a kept media type is saved under.
#[must_use]
pub fn extension_of(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/pdf" => Some("pdf"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        _ => None,
    }
}

/// `text` cut at `most` bytes on a character boundary.
fn cut(text: &str, most: usize) -> String {
    if text.len() <= most {
        return text.to_string();
    }
    let mut end = most;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Text from a header with every control character a space, at most `most` characters.
fn plain(text: &str, most: usize) -> String {
    text.chars()
        .map(|one| if one.is_control() { ' ' } else { one })
        .take(most)
        .collect()
}

/// The addresses and ids of a header value, however mail-parser read it.
fn texts(value: &mail_parser::HeaderValue<'_>) -> Vec<String> {
    if let Some(list) = value.as_text_list() {
        return list.iter().map(ToString::to_string).collect();
    }
    value
        .as_address()
        .map(|addresses| {
            addresses
                .iter()
                .filter_map(|one| one.address().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// What the headers fetched of one message say.
fn headers_of(raw: &[u8], size: u64) -> Headers {
    use mail_parser::{HeaderName, MessageParser};
    let Some(message) = MessageParser::default().parse(raw) else {
        return Headers {
            size,
            ..Headers::default()
        };
    };
    let list = |name: HeaderName<'static>| -> Vec<String> {
        message.header_values(name).flat_map(texts).collect()
    };
    Headers {
        from: message
            .from()
            .and_then(|from| from.first())
            .and_then(|one| one.address())
            .unwrap_or_default()
            .to_string(),
        to: list(HeaderName::To),
        cc: list(HeaderName::Cc),
        delivered_to: list(HeaderName::DeliveredTo),
        in_reply_to: texts(message.in_reply_to()),
        references: texts(message.references()),
        size,
    }
}

/// The messages after `first` among the headers a `UID FETCH first:*` answered, oldest first, with
/// what their headers say. `n:*` always answers the newest message, even one older than `n`, as
/// when the messages after `n` were deleted since the folder was opened: that one was looked at
/// already, and is left out.
fn headers_after(
    first: u32,
    found: impl IntoIterator<Item = (Option<u32>, u32, Vec<u8>)>,
) -> Vec<(u32, Headers)> {
    let mut headers: Vec<(u32, Headers)> = found
        .into_iter()
        .filter_map(|(uid, size, raw)| {
            let uid = uid.filter(|uid| *uid >= first)?;
            Some((uid, headers_of(&raw, u64::from(size))))
        })
        .collect();
    headers.sort_by_key(|(uid, _)| *uid);
    headers
}

/// The reply a whole message is.
fn reply_of(uid: u32, raw: &[u8]) -> Option<Reply> {
    use mail_parser::{MessageParser, MimeHeaders as _};
    let message = MessageParser::default().parse(raw)?;
    let from = message
        .from()
        .and_then(|from| from.first())
        .map_or_else(String::new, |one| match (one.name(), one.address()) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (None, Some(address)) => address.to_string(),
            (Some(name), None) => name.to_string(),
            (None, None) => String::new(),
        });
    let attachments = message
        .attachments()
        .take(MOST_ATTACHMENTS)
        .map(|part| {
            let bytes = part.contents();
            let size = bytes.len() as u64;
            let kind = kept_type(bytes).filter(|_| bytes.len() <= MOST_KEPT_BYTES);
            Attachment {
                name: plain(part.attachment_name().unwrap_or("attachment"), 255),
                media_type: kind.map(|(media, _)| media.to_string()),
                bytes: kind.map(|_| bytes.to_vec()),
                size,
            }
        })
        .collect();
    let from = plain(&from, 320);
    Some(Reply {
        uid,
        from: if from.trim().is_empty() {
            UNKNOWN_SENDER.to_string()
        } else {
            from
        },
        subject: plain(message.subject().unwrap_or_default(), 998),
        date: message
            .date()
            .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0)),
        answers: {
            let mut answers: Vec<String> = Vec::new();
            for id in texts(message.in_reply_to())
                .into_iter()
                .chain(texts(message.references()))
            {
                if !answers.contains(&id) {
                    answers.push(id);
                }
            }
            answers
        },
        text: cut(
            message.body_text(0).as_deref().unwrap_or_default(),
            MOST_TEXT_BYTES,
        ),
        attachments,
    })
}

/// Reads the new messages of the folder that are for us (`is_for_us`): fetches the headers of every
/// message after the ledger's last UID with `EXAMINE` and `BODY.PEEK`, so that nothing is marked
/// read, moved or deleted, and the body of those that are for us alone. A folder the provider
/// renumbered (a changed `UIDVALIDITY`) starts again at the next UID and reads nothing older.
///
/// # Errors
///
/// As [`check_login`] for the connection and the sign-in; `Server` for an answer Farik cannot use.
pub async fn fetch_replies(
    settings: &MailboxSettings,
    password: &Secret,
    trust: &Trust,
    ledger: &Ledger,
    known: &Known,
) -> Result<Fetched, MailboxError> {
    let mut session = imap_login(settings, password, trust).await?;
    let read = read_replies(&mut session, settings, ledger, known).await;
    let _ = session.logout().await;
    read
}

async fn read_replies(
    session: &mut ImapSession,
    settings: &MailboxSettings,
    ledger: &Ledger,
    known: &Known,
) -> Result<Fetched, MailboxError> {
    use futures_util::TryStreamExt as _;
    let unusable = || {
        MailboxError::Server(
            "The mail server answered in a way Farik can\u{2019}t use.".to_string(),
        )
    };
    let mailbox = within(async {
        session
            .examine(&settings.folder)
            .await
            .map_err(|error| imap_error(&error))
    })
    .await?;
    let (Some(validity), Some(next)) = (mailbox.uid_validity, mailbox.uid_next) else {
        return Err(unusable());
    };
    let mut ledger = ledger.clone();
    let restarted = validity != ledger.uidvalidity;
    if restarted {
        ledger.uidvalidity = validity;
        ledger.last_uid = next.saturating_sub(1);
    }
    let mut fetched = Fetched {
        replies: Vec::new(),
        bodies: Vec::new(),
        skipped: Vec::new(),
        ledger,
        restarted,
    };
    if next.saturating_sub(1) <= fetched.ledger.last_uid {
        return Ok(fetched);
    }
    let first = fetched.ledger.last_uid + 1;
    let query = format!("(UID RFC822.SIZE {HEADER_FIELDS})");
    let headers: Vec<(u32, Headers)> = within(async {
        let stream = session
            .uid_fetch(format!("{first}:*"), &query)
            .await
            .map_err(|error| imap_error(&error))?;
        let found: Vec<async_imap::types::Fetch> = stream
            .try_collect()
            .await
            .map_err(|error| imap_error(&error))?;
        let fetched_headers = found.iter().map(|one| {
            (
                one.uid,
                one.size.unwrap_or(0),
                one.header().unwrap_or_default().to_vec(),
            )
        });
        let headers = headers_after(first, fetched_headers);
        Ok(headers)
    })
    .await?;
    for (uid, header) in &headers {
        if is_for_us(header, known) {
            fetched.bodies.push(*uid);
        } else {
            fetched.skipped.push(*uid);
        }
    }
    for uid in fetched.bodies.clone() {
        let raw: Vec<u8> = within(async {
            let stream = session
                .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
                .await
                .map_err(|error| imap_error(&error))?;
            let found: Vec<async_imap::types::Fetch> = stream
                .try_collect()
                .await
                .map_err(|error| imap_error(&error))?;
            Ok(found
                .iter()
                .find(|one| one.uid == Some(uid))
                .and_then(|one| one.body())
                .map(<[u8]>::to_vec)
                .unwrap_or_default())
        })
        .await?;
        if let Some(reply) = reply_of(uid, &raw) {
            fetched.replies.push(reply);
        }
    }
    if let Some((uid, _)) = headers.last() {
        fetched.ledger.last_uid = *uid;
    }
    Ok(fetched)
}

/// Where the procurement mailbox's password is kept: one per project on this computer, in the
/// keychain at `mailbox:<project id>:procurement`, or without a keychain in `connectors.json`
/// under that key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxAt {
    /// The project's id on this computer ([`crate::connectors::local_project_id`]).
    pub project_id: String,
}

impl MailboxAt {
    /// The keychain account, and the key in `connectors.json`.
    #[must_use]
    pub fn account(&self) -> String {
        self.secret_at().account()
    }

    fn secret_at(&self) -> SecretAt {
        SecretAt::mailbox(&self.project_id, "procurement")
    }
}

/// A place the mailbox's password can be kept: the stores the connectors' keys are kept in.
pub trait MailboxSecrets: Send + Sync {
    /// The password kept for `at`; `Ok(None)` when none is.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    fn load(&self, at: &MailboxAt) -> Result<Option<Secret>, CredentialError>;
    /// Keeps `password` for `at`, replacing any other, and says where.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn save(
        &self,
        at: &MailboxAt,
        password: &Secret,
    ) -> Result<crate::connectors::SecretStore, CredentialError>;
    /// Removes the password kept for `at`; nothing kept is nothing to remove.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn delete(&self, at: &MailboxAt) -> Result<(), CredentialError>;
}

/// The key under which the password is kept in a stored entry.
const PASSWORD_KEY: &str = "password";

impl<Store: crate::connectors::ConnectorSecrets + ?Sized> MailboxSecrets for Store {
    fn load(&self, at: &MailboxAt) -> Result<Option<Secret>, CredentialError> {
        Ok(
            crate::connectors::ConnectorSecrets::load(self, &at.secret_at())?
                .and_then(|entry| entry.keys.get(PASSWORD_KEY).cloned()),
        )
    }

    fn save(
        &self,
        at: &MailboxAt,
        password: &Secret,
    ) -> Result<crate::connectors::SecretStore, CredentialError> {
        let entry = crate::connectors::ConnectorEntry {
            spec_sha256: String::new(),
            keys: std::collections::BTreeMap::from([(PASSWORD_KEY.to_string(), password.clone())]),
            oauth: None,
        };
        crate::connectors::ConnectorSecrets::save(self, &at.secret_at(), &entry)
    }

    fn delete(&self, at: &MailboxAt) -> Result<(), CredentialError> {
        crate::connectors::ConnectorSecrets::delete(self, &at.secret_at())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Headers, Known, MailboxSettings, Provider, ProviderOf, Security, Server, is_for_us,
        provider_of, servers, validate_settings,
    };

    fn server(host: &str, port: u16, security: Security) -> Server {
        Server {
            host: host.to_string(),
            port,
            security,
        }
    }

    #[test]
    fn finds_a_provider_from_the_address() {
        for address in ["ivo@gmail.com", "Ivo@GoogleMail.com"] {
            assert_eq!(
                provider_of(address),
                ProviderOf::Known(Provider::Gmail),
                "{address}"
            );
        }
        assert_eq!(
            provider_of("ivo@me.com"),
            ProviderOf::Known(Provider::Icloud)
        );
        assert_eq!(
            provider_of("ivo@icloud.com"),
            ProviderOf::Known(Provider::Icloud)
        );
        assert_eq!(
            provider_of("ivo@mac.com"),
            ProviderOf::Known(Provider::Icloud)
        );
        assert_eq!(
            provider_of("buying@fastmail.fm"),
            ProviderOf::Known(Provider::Fastmail)
        );
        for address in [
            "ivo@outlook.com",
            "ivo@Hotmail.com",
            "ivo@live.com",
            "ivo@msn.com",
        ] {
            assert_eq!(provider_of(address), ProviderOf::Microsoft, "{address}");
        }
        assert_eq!(provider_of("buying@bakery.test"), ProviderOf::Unknown);
        assert_eq!(provider_of("no at sign"), ProviderOf::Unknown);

        let (imap, smtp) = servers(Provider::Gmail).expect("Gmail has servers");
        assert_eq!(imap, server("imap.gmail.com", 993, Security::Tls));
        assert_eq!(smtp, server("smtp.gmail.com", 465, Security::Tls));
        let (imap, smtp) = servers(Provider::Icloud).expect("iCloud has servers");
        assert_eq!(imap, server("imap.mail.me.com", 993, Security::Tls));
        assert_eq!(smtp, server("smtp.mail.me.com", 587, Security::StartTls));
        let (imap, smtp) = servers(Provider::Fastmail).expect("Fastmail has servers");
        assert_eq!(imap, server("imap.fastmail.com", 993, Security::Tls));
        assert_eq!(smtp, server("smtp.fastmail.com", 465, Security::Tls));
        assert_eq!(servers(Provider::Other), None);
        // Typed servers: encrypted from the start on the two implicit-TLS ports, else STARTTLS.
        assert_eq!(Server::typed("mail.test", 993).security, Security::Tls);
        assert_eq!(Server::typed("mail.test", 465).security, Security::Tls);
        assert_eq!(Server::typed("mail.test", 143).security, Security::StartTls);
        assert_eq!(Server::typed("mail.test", 587).security, Security::StartTls);
    }

    type Change = fn(&mut MailboxSettings);

    fn settings() -> MailboxSettings {
        MailboxSettings {
            address: "buying@bakery.test".to_string(),
            name: "Sam Ortiz".to_string(),
            provider: Provider::Other,
            imap: server("mail.bakery.test", 993, Security::Tls),
            smtp: server("mail.bakery.test", 465, Security::Tls),
            username: "buying".to_string(),
            folder: "INBOX".to_string(),
            signature: "Corner Bakery".to_string(),
            disclose_ai: true,
        }
    }

    #[test]
    fn names_the_field_that_does_not_fit() {
        assert_eq!(validate_settings(&settings()), Ok(()));
        let cases: [(&str, Change); 10] = [
            ("address", |s| s.address = "two@a.test, three@b.test".into()),
            ("address", |s| s.address = "Ivo <buying@bakery.test>".into()),
            ("name", |s| s.name = String::new()),
            ("name", |s| s.name = "x".repeat(101)),
            ("name", |s| s.name = "Sam\nOrtiz".into()),
            ("folder", |s| s.folder = "A\r\nB".into()),
            ("signature", |s| s.signature = "x".repeat(601)),
            ("username", |s| s.username = String::new()),
            ("imap", |s| s.imap.host = String::new()),
            ("smtp", |s| s.smtp.port = 0),
        ];
        for (field, change) in cases {
            let mut wrong = settings();
            change(&mut wrong);
            assert_eq!(validate_settings(&wrong), Err(field), "{field}");
        }
        // A signature may have several lines.
        let mut lines = settings();
        lines.signature = "Sam Ortiz\nCorner Bakery".into();
        assert_eq!(validate_settings(&lines), Ok(()));
    }
    fn known() -> Known {
        Known {
            address: "buying@bakery.test".to_string(),
            sent_ids: vec!["m1@bakery.test".to_string()],
            written_to: vec!["sales@pieboxpros.test".to_string()],
        }
    }

    /// A reply from Dana answering message `m1`, addressed to the procurement address.
    fn a_reply() -> Headers {
        Headers {
            from: "sales@pieboxpros.test".to_string(),
            to: vec!["Buying@Bakery.test".to_string()],
            cc: Vec::new(),
            delivered_to: Vec::new(),
            in_reply_to: vec!["m1@bakery.test".to_string()],
            references: Vec::new(),
            size: 4_000,
        }
    }

    #[test]
    fn is_for_us_reads_only_the_procurement_address() {
        // To, Cc or Delivered-To holds the address, without regard to case, and the message
        // answers a sent one.
        assert!(is_for_us(&a_reply(), &known()));
        let mut cc = a_reply();
        cc.to = vec!["someone@else.test".to_string()];
        cc.cc = vec!["BUYING@bakery.test".to_string()];
        assert!(is_for_us(&cc, &known()));
        let mut delivered = a_reply();
        delivered.to = Vec::new();
        delivered.delivered_to = vec!["buying@bakery.test".to_string()];
        assert!(is_for_us(&delivered, &known()));
        // A reference names the sent message as well as an In-Reply-To does.
        let mut referenced = a_reply();
        referenced.in_reply_to = Vec::new();
        referenced.references = vec!["old@x.test".to_string(), "m1@bakery.test".to_string()];
        assert!(is_for_us(&referenced, &known()));
        // From an address a message was written to, answering nothing, holds too.
        let mut from_a_seller = a_reply();
        from_a_seller.in_reply_to = Vec::new();
        from_a_seller.from = "Sales@PieBoxPros.test".to_string();
        assert!(is_for_us(&from_a_seller, &known()));

        // The same answer addressed only to the account's main address does not.
        let mut main = a_reply();
        main.to = vec!["owner@bakery.test".to_string()];
        assert!(!is_for_us(&main, &known()));
        // Addressed to us, answering nothing, from an address never written to: no.
        let mut stranger = a_reply();
        stranger.in_reply_to = Vec::new();
        stranger.from = "promo@elsewhere.test".to_string();
        assert!(!is_for_us(&stranger, &known()));
        // Answering a message that is not ours, from a stranger: no.
        let mut other = a_reply();
        other.in_reply_to = vec!["unrelated@x.test".to_string()];
        other.from = "promo@elsewhere.test".to_string();
        assert!(!is_for_us(&other, &known()));
        // 25 MB is read, and one byte more is not.
        let mut big = a_reply();
        big.size = 25 * 1024 * 1024;
        assert!(is_for_us(&big, &known()));
        big.size += 1;
        assert!(!is_for_us(&big, &known()));
    }
    #[test]
    fn leaves_out_a_message_older_than_the_range_asked() {
        let raw =
            |from: &str| format!("From: {from}\r\nTo: buying@bakery.test\r\n\r\n").into_bytes();
        let found = vec![
            (Some(9), 300, raw("late@x.test")),
            (Some(4), 200, raw("old@x.test")),
            (None, 100, raw("nobody@x.test")),
            (Some(7), 150, raw("mid@x.test")),
        ];
        let headers = super::headers_after(7, found);
        assert_eq!(
            headers
                .iter()
                .map(|(uid, header)| (*uid, header.from.as_str(), header.size))
                .collect::<Vec<_>>(),
            [(7, "mid@x.test", 150), (9, "late@x.test", 300)],
            "oldest first, and neither the one below 7 nor the one with no UID"
        );
    }
}
