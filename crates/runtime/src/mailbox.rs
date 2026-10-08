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
        MailboxSettings, Provider, ProviderOf, Security, Server, provider_of, servers,
        validate_settings,
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
}
