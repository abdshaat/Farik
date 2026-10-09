//! `farik procurement`: connect the mailbox the Procurement Specialist's messages go from, read
//! what waits to be sent, send or discard a message, and check for replies (`docs/SPEC.md` 6.10,
//! ADR 0039). Nothing is sent to a seller but by `farik procurement send`, which prints the
//! message whole first; the password is read without echo and never printed.

use std::io::BufRead as _;

use farik_runtime::claude::Secret;
use farik_runtime::mailbox::{ProviderChoice, ProviderOf, Server, provider_of, servers};
use farik_runtime::procurement::{MailboxConnect, mailbox_state, seller_messages_list};
use serde_json::{Value, json};

use crate::printable::printable;
use crate::project::{Project, tool_deps};
use crate::{CliIo, Report};

/// What `farik procurement mailbox connect` was given.
pub struct ConnectArgs<'a> {
    /// The address mail is sent from and read for.
    pub address: &'a str,
    /// The name sellers see.
    pub name: &'a str,
    /// The provider, or none to tell it from the address.
    pub provider: Option<&'a str>,
    /// The reading server as `host:port`, for another provider.
    pub imap: Option<&'a str>,
    /// The sending server as `host:port`, for another provider.
    pub smtp: Option<&'a str>,
    /// The sign-in name, the address when left out.
    pub username: Option<&'a str>,
    /// The folder Farik reads, `INBOX` when left out.
    pub folder: Option<&'a str>,
    /// What Farik adds under every message.
    pub signature: Option<&'a str>,
    /// Whether Farik leaves out the line saying an AI assistant wrote the message.
    pub no_disclosure: bool,
}

/// The refusal for Microsoft, in the words the daemon's own refusal has.
const MICROSOFT: &str = "mailbox_provider_unsupported: Microsoft mailboxes are not supported yet.";

/// `host:port` as a server.
fn server(flag: &str, text: Option<&str>) -> Result<Server, String> {
    let wrong = || {
        format!(
            "{flag} {} is not a server: write it as host:port",
            text.unwrap_or_default()
        )
    };
    let (host, port) = text
        .and_then(|text| text.rsplit_once(':'))
        .ok_or_else(wrong)?;
    let port = port.parse::<u16>().map_err(|_| wrong())?;
    if host.is_empty() {
        return Err(wrong());
    }
    Ok(Server::typed(host, port))
}

/// The mailbox the arguments describe, with a known provider's servers filled in.
///
/// # Errors
///
/// A sentence saying what is wrong: Microsoft is refused as the daemon refuses it, and another
/// provider needs `--imap` and `--smtp`.
pub fn connect_input(args: &ConnectArgs<'_>) -> Result<MailboxConnect, String> {
    let provider = match args.provider {
        Some("gmail") => ProviderChoice::Gmail,
        Some("icloud") => ProviderChoice::Icloud,
        Some("fastmail") => ProviderChoice::Fastmail,
        Some("other") => ProviderChoice::Other,
        Some("microsoft") => ProviderChoice::Microsoft,
        Some(other) => {
            return Err(format!(
                "--provider {other} is not one Farik knows: write gmail, icloud, fastmail or other"
            ));
        }
        None => match provider_of(args.address) {
            ProviderOf::Known(farik_runtime::mailbox::Provider::Gmail) => ProviderChoice::Gmail,
            ProviderOf::Known(farik_runtime::mailbox::Provider::Icloud) => ProviderChoice::Icloud,
            ProviderOf::Known(farik_runtime::mailbox::Provider::Fastmail) => {
                ProviderChoice::Fastmail
            }
            ProviderOf::Known(farik_runtime::mailbox::Provider::Other) | ProviderOf::Unknown => {
                ProviderChoice::Other
            }
            ProviderOf::Microsoft => ProviderChoice::Microsoft,
        },
    };
    let Some(known) = provider.known() else {
        return Err(MICROSOFT.to_string());
    };
    let (imap, smtp) = match servers(known) {
        Some(both) => both,
        None => (server("--imap", args.imap)?, server("--smtp", args.smtp)?),
    };
    Ok(MailboxConnect {
        address: args.address.to_string(),
        name: args.name.to_string(),
        provider,
        imap,
        smtp,
        username: args.username.unwrap_or(args.address).to_string(),
        folder: args.folder.unwrap_or("INBOX").to_string(),
        signature: args.signature.unwrap_or_default().to_string(),
        disclose_ai: !args.no_disclosure,
    })
}

/// The mailbox's password: one line of standard input, or typed with the echo off at a terminal.
fn read_password(io: &mut CliIo<'_>) -> Result<Secret, String> {
    let value = if io.stdin_is_terminal {
        rpassword::prompt_password("Mailbox password: ")
            .map_err(|error| format!("the password could not be read: {error}"))?
    } else {
        let mut line = String::new();
        std::io::BufReader::new(&mut io.stdin)
            .read_line(&mut line)
            .map_err(|error| format!("the password could not be read: {error}"))?;
        line.trim_end_matches(['\n', '\r']).to_string()
    };
    if value.is_empty() {
        return Err(
            "the mailbox has no password: type it, or give it on standard input".to_string(),
        );
    }
    Ok(Secret::new(value))
}

/// The sentence for a mailbox command another process would have to run.
#[cfg(unix)]
const DRIVEN: &str = "another farik process is running this project: change the mailbox on the \
                      Procurement Specialist's page in the web app";

/// `farik procurement mailbox connect`.
///
/// # Errors
///
/// A sentence saying what is wrong, or the refusal of the servers, which never quotes the
/// password.
#[cfg(unix)]
pub fn connect(
    project: &Project,
    args: &ConnectArgs<'_>,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    let input = connect_input(args)?;
    let password = read_password(io)?;
    let address = input.address.clone();
    let (tools, daemon) = daemon_of(project, io)?;
    crate::here_or_sent(
        project,
        || {
            crate::start::runtime()?
                .block_on(farik_runtime::procurement::connect_mailbox_on(
                    &daemon, &tools, input, &password,
                ))
                .map_err(|refusal| refusal.to_string())?;
            Ok(Report {
                lines: vec![format!(
                    "Connected {address}: Farik logged in to both servers and sent nothing."
                )],
                json: json!({ "connected": true, "address": address }),
                json_lines: None,
            })
        },
        || Err(DRIVEN.to_string()),
    )
}

/// `farik procurement mailbox disconnect`.
///
/// # Errors
///
/// A sentence saying what the key store or the files refused.
#[cfg(unix)]
pub fn disconnect(project: &Project, io: &CliIo<'_>) -> Result<Report, String> {
    let (tools, daemon) = daemon_of(project, io)?;
    crate::here_or_sent(
        project,
        || {
            farik_runtime::procurement::disconnect_mailbox_on(&daemon, &tools)
                .map_err(|refusal| refusal.to_string())?;
            Ok(Report {
                lines: vec!["Disconnected the mailbox: its password is forgotten.".to_string()],
                json: json!({ "connected": false }),
                json_lines: None,
            })
        },
        || Err(DRIVEN.to_string()),
    )
}

/// `farik procurement check`.
///
/// # Errors
///
/// A sentence saying why the mailbox could not be read.
#[cfg(unix)]
pub fn check(project: &Project, io: &CliIo<'_>) -> Result<Report, String> {
    let (tools, daemon) = daemon_of(project, io)?;
    crate::here_or_sent(
        project,
        || {
            let replies = crate::start::runtime()?
                .block_on(farik_runtime::procurement::check_mailbox_on(
                    &daemon, &tools,
                ))
                .map_err(|refusal| refusal.to_string())?;
            Ok(Report {
                lines: vec![format!("Checked the mailbox: {replies} new replies.")],
                json: json!({ "replies": replies }),
                json_lines: None,
            })
        },
        || Err(DRIVEN.to_string()),
    )
}

/// The tools of the project and a daemon state that keeps the mailbox's password where the
/// connectors' keys are kept.
#[cfg(unix)]
fn daemon_of(
    project: &Project,
    io: &CliIo<'_>,
) -> Result<
    (
        std::sync::Arc<farik_runtime::tools::ToolDeps>,
        std::sync::Arc<farik_runtime::daemon::DaemonState>,
    ),
    String,
> {
    let tools = tool_deps(project, io)?;
    let daemon = crate::start::connected_daemon(&tools, io);
    Ok((tools, daemon))
}

/// `farik procurement mailbox show`: what `procurement_mailbox.get` answers.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn show(project: &Project, io: &CliIo<'_>) -> Result<Report, String> {
    let wire = mailbox_state(&*tool_deps(project, io)?).map_err(|error| error.to_string())?;
    let text = |key: &str| printable(wire[key].as_str().unwrap_or_default()).into_owned();
    let mut lines = Vec::new();
    if wire["connected"].as_bool().unwrap_or(false) {
        lines.push(format!(
            "Connected: {} ({}), folder {}",
            text("address"),
            text("name"),
            text("folder")
        ));
        if wire.get("checked_at").is_some() {
            lines.push(format!("Last checked {}", text("checked_at")));
        }
        if wire.get("error").is_some() {
            lines.push(format!("Last check failed: {}", text("error")));
        }
    } else {
        lines.push("No mailbox is connected.".to_string());
    }
    lines.push(format!(
        "Sent today: {} of {}",
        wire["sent_today"], wire["cap"]
    ));
    Ok(Report {
        lines,
        json: wire,
        json_lines: None,
    })
}

/// One message whole: who it goes to, its subject and its body, as a terminal may be given it.
fn whole(row: &Value) -> Vec<String> {
    let text = |key: &str| printable(row[key].as_str().unwrap_or_default()).into_owned();
    let mut lines = vec![
        format!(
            "Message {} to {} <{}>  {}{}",
            row["message"].as_u64().unwrap_or_default(),
            text("seller"),
            text("to"),
            text("state"),
            if row["new_domain"].as_bool().unwrap_or(false) {
                "  (nothing was sent to this domain before)"
            } else {
                ""
            }
        ),
        format!("Subject: {}", text("subject")),
        String::new(),
    ];
    lines.extend(text("body").lines().map(str::to_string));
    if let Some(why) = row.get("why").and_then(Value::as_str) {
        lines.push(format!("Last try failed: {}", printable(why)));
    }
    lines
}

/// Every message that waits to be sent, whole, and the same as `seller_messages.list` answers
/// with `--json`.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn messages(project: &Project, io: &CliIo<'_>) -> Result<Report, String> {
    let wire =
        seller_messages_list(&*tool_deps(project, io)?).map_err(|error| error.to_string())?;
    let waiting: Vec<&Value> = wire["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["state"] == "waiting")
        .collect();
    let mut lines = Vec::new();
    for row in &waiting {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(whole(row));
    }
    if lines.is_empty() {
        lines.push("no message waits to be sent".to_string());
    }
    Ok(Report {
        lines,
        json: wire,
        json_lines: None,
    })
}

/// The message `number` as `seller_messages.list` has it, waiting and not an order's.
///
/// # Errors
///
/// A sentence saying there is no such message waiting, or that an order's message is sent from
/// the web app.
pub fn waiting(project: &Project, io: &CliIo<'_>, number: u64) -> Result<Value, String> {
    let wire =
        seller_messages_list(&*tool_deps(project, io)?).map_err(|error| error.to_string())?;
    let row = wire["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["message"].as_u64() == Some(number))
        .cloned()
        .ok_or_else(|| format!("message {number} is not in this project"))?;
    if row.get("purchase_order").is_some() {
        return Err(format!(
            "message {number} carries an order: send it from the web app, where you see it beside \
             its order"
        ));
    }
    if row["state"] != "waiting" {
        return Err(format!("message {number} does not wait to be sent"));
    }
    Ok(row)
}

/// The command that sends message `row` as it was drafted, which the owner has just been shown.
#[must_use]
pub fn send_command(row: &Value) -> farik_protocol::command::Command {
    farik_protocol::command::Command::SellerMessageSend {
        message: row["message"].as_u64().unwrap_or_default(),
        subject: row["subject"].as_str().unwrap_or_default().to_string(),
        body: row["body"].as_str().unwrap_or_default().to_string(),
    }
}

/// `said`, the answer to sending `row`, after the message itself, whole.
#[must_use]
pub fn shown_then(row: &Value, said: Report) -> Report {
    let mut lines = whole(row);
    lines.push(String::new());
    lines.extend(said.lines);
    Report {
        lines,
        json: said.json,
        json_lines: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(address: &'a str, provider: Option<&'a str>) -> ConnectArgs<'a> {
        ConnectArgs {
            address,
            name: "Ivo",
            provider,
            imap: None,
            smtp: None,
            username: None,
            folder: None,
            signature: None,
            no_disclosure: false,
        }
    }

    #[test]
    fn takes_gmail_s_servers_from_the_address_and_discloses_by_default() {
        let input = connect_input(&args("ivo@gmail.com", None)).expect("gmail");
        assert_eq!(input.provider, ProviderChoice::Gmail);
        assert_eq!(
            (input.imap.host.as_str(), input.imap.port),
            ("imap.gmail.com", 993)
        );
        assert_eq!(
            (input.smtp.host.as_str(), input.smtp.port),
            ("smtp.gmail.com", 465)
        );
        assert_eq!(input.username, "ivo@gmail.com");
        assert_eq!(input.folder, "INBOX");
        assert!(input.disclose_ai);
        let quiet = ConnectArgs {
            no_disclosure: true,
            ..args("ivo@gmail.com", Some("gmail"))
        };
        assert!(!connect_input(&quiet).expect("gmail").disclose_ai);
    }

    #[test]
    fn refuses_microsoft_and_asks_another_provider_for_its_servers() {
        for refused in [
            args("ivo@outlook.com", None),
            args("ivo@x.test", Some("microsoft")),
        ] {
            assert_eq!(connect_input(&refused).unwrap_err(), MICROSOFT);
        }
        assert!(
            connect_input(&args("ivo@x.test", None))
                .unwrap_err()
                .contains("--imap")
        );
        let typed = ConnectArgs {
            imap: Some("mail.x.test:993"),
            smtp: Some("mail.x.test:587"),
            ..args("ivo@x.test", Some("other"))
        };
        let input = connect_input(&typed).expect("typed");
        assert_eq!(input.imap.security, farik_runtime::mailbox::Security::Tls);
        assert_eq!(
            input.smtp.security,
            farik_runtime::mailbox::Security::StartTls
        );
        let bad = ConnectArgs {
            imap: Some("mail.x.test:nope"),
            ..typed
        };
        assert!(connect_input(&bad).unwrap_err().contains("host:port"));
    }
}
