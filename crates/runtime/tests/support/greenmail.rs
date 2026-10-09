//! A mail server for the tests of the procurement mailbox (phase 7 step 10f): GreenMail's image,
//! pinned by tag and digest, run by Docker with a certificate made for the run, so that the tests
//! log in to a real IMAP and SMTP server over TLS. A wrong password fails (no
//! `-Dgreenmail.auth.disabled`). GreenMail has no STARTTLS (its issue 135), so the plain ports
//! serve the tests that refuse a server which does not upgrade. It is `#[path]`-included by the
//! tests that need it, so it uses only what every including crate has. The tests are `#[ignore]`d:
//! `cargo xtask check --integration` runs them with Docker.
#![allow(dead_code, missing_docs, clippy::pedantic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// The image, by tag and by the digest of its manifest list.
pub const IMAGE: &str = "greenmail/standalone:2.1.14@sha256:1ef95a966418cd09b7ea91d504d8c0826bbe7a2f6e679a75c601a831587c1626";

/// A user of the fixture: the sign-in name, the password and the address mail to it is for.
pub struct Account {
    pub login: &'static str,
    pub password: &'static str,
    pub address: &'static str,
}

/// The procurement mailbox of the story, `buying@bakery.test`.
pub const BUYING: Account = Account {
    login: "buying",
    password: "open-sesame",
    address: "buying@bakery.test",
};

/// The keystore's password, which protects nothing: the key is made for the run and thrown away.
const KEYSTORE_WORD: &str = "fixture";

/// A running GreenMail, removed when this is dropped.
pub struct GreenMail {
    name: String,
    dir: PathBuf,
    /// The host ports of GreenMail's plain IMAP and SMTP, and of its TLS IMAP and SMTP.
    pub imap: u16,
    pub imaps: u16,
    pub smtp: u16,
    pub smtps: u16,
    /// The certificate authority that signed the server's certificate, DER.
    pub ca_der: Vec<u8>,
}

fn docker(args: &[&str]) -> (bool, String) {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker runs");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

fn openssl(args: &[&str], dir: &std::path::Path) {
    let output = Command::new("openssl")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("openssl runs");
    assert!(
        output.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Whether the server behind a published `port` is up: Docker's proxy accepts the connection and
/// closes it again while the server is not, so a server that is up is one that holds the
/// connection open (or speaks first).
fn is_listening(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .expect("a timeout");
    let mut byte = [0_u8; 1];
    !matches!(stream.read(&mut byte), Ok(0))
}

impl GreenMail {
    /// Starts GreenMail with the `accounts`, and waits until its IMAP answers.
    pub fn start(test: &str, accounts: &[&Account]) -> GreenMail {
        GreenMail::start_with(test, accounts, None)
    }

    /// `start`, with a certificate signed by the same authority as `beside`'s, so that one
    /// `Trust::Root` serves both servers.
    pub fn start_beside(test: &str, accounts: &[&Account], beside: &GreenMail) -> GreenMail {
        GreenMail::start_with(test, accounts, Some(beside))
    }

    fn start_with(test: &str, accounts: &[&Account], beside: Option<&GreenMail>) -> GreenMail {
        static STARTED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let name = format!(
            "farik-greenmail-{}-{}-{test}",
            std::process::id(),
            STARTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(&name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        std::fs::write(
            dir.join("server.ext"),
            "basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n\
             extendedKeyUsage=serverAuth\n",
        )
        .expect("the extensions are written");
        match beside {
            Some(other) => {
                for file in ["ca.pem", "ca.key"] {
                    std::fs::copy(other.dir.join(file), dir.join(file))
                        .expect("the authority is copied");
                }
            }
            None => openssl(
                &[
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-keyout",
                    "ca.key",
                    "-out",
                    "ca.pem",
                    "-days",
                    "30",
                    "-subj",
                    "/CN=Farik test CA",
                    "-addext",
                    "basicConstraints=critical,CA:TRUE",
                ],
                &dir,
            ),
        }
        openssl(
            &[
                "req",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "server.key",
                "-out",
                "server.csr",
                "-subj",
                "/CN=localhost",
            ],
            &dir,
        );
        openssl(
            &[
                "x509",
                "-req",
                "-in",
                "server.csr",
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-CAcreateserial",
                "-out",
                "server.pem",
                "-days",
                "30",
                "-extfile",
                "server.ext",
            ],
            &dir,
        );
        openssl(
            &[
                "pkcs12",
                "-export",
                "-in",
                "server.pem",
                "-inkey",
                "server.key",
                "-certfile",
                "ca.pem",
                "-out",
                "keystore.p12",
                "-name",
                "greenmail",
                "-passout",
                &format!("pass:{KEYSTORE_WORD}"),
            ],
            &dir,
        );
        openssl(
            &["x509", "-in", "ca.pem", "-outform", "der", "-out", "ca.der"],
            &dir,
        );
        let ca_der = std::fs::read(dir.join("ca.der")).expect("the authority is read");
        {
            use std::os::unix::fs::PermissionsExt as _;
            // The container's user reads the keystore through the bind mount.
            std::fs::set_permissions(
                dir.join("keystore.p12"),
                std::fs::Permissions::from_mode(0o644),
            )
            .expect("the keystore is readable");
        }
        let users: Vec<String> = accounts
            .iter()
            .map(|account| {
                let domain = account.address.split_once('@').map_or("", |(_, d)| d);
                format!("{}:{}@{domain}", account.login, account.password)
            })
            .collect();
        let options = format!(
            "-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users={} \
             -Dgreenmail.tls.keystore.file=/keystore.p12 \
             -Dgreenmail.tls.keystore.password={KEYSTORE_WORD}",
            users.join(",")
        );
        let mount = format!("{}:/keystore.p12:ro", dir.join("keystore.p12").display());
        let mut run = vec![
            "run".to_string(),
            "-d".into(),
            "--rm".into(),
            "--name".into(),
            name.clone(),
            "--label".into(),
            format!("farik.project={name}"),
            "-v".into(),
            mount,
            "-e".into(),
            format!("GREENMAIL_OPTS={options}"),
        ];
        for port in ["3143", "3993", "3025", "3465"] {
            run.push("-p".into());
            run.push(format!("127.0.0.1:0:{port}"));
        }
        run.push(IMAGE.into());
        let args: Vec<&str> = run.iter().map(String::as_str).collect();
        let (started, output) = docker(&args);
        assert!(started, "GreenMail did not start: {output}");
        let port = |inside: &str| -> u16 {
            let (_, out) = docker(&["port", &name, inside]);
            out.lines()
                .next()
                .and_then(|line| line.rsplit(':').next())
                .and_then(|port| port.trim().parse().ok())
                .unwrap_or_else(|| panic!("no host port for {inside}: {out}"))
        };
        let server = GreenMail {
            imap: port("3143"),
            imaps: port("3993"),
            smtp: port("3025"),
            smtps: port("3465"),
            name,
            dir,
            ca_der,
        };
        server.wait_until_ready();
        server
    }

    fn wait_until_ready(&self) {
        for _ in 0..240 {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", self.imap)) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("a timeout");
                let mut line = String::new();
                if BufReader::new(stream).read_line(&mut line).is_ok()
                    && line.contains("OK")
                    && [self.imaps, self.smtp, self.smtps]
                        .iter()
                        .all(|port| is_listening(*port))
                {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("GreenMail's IMAP did not answer within a minute");
    }

    /// Delivers `raw`, a whole message with its headers and CRLF line ends, to `recipient`'s
    /// mailbox through the plain SMTP port. The headers may say anything; only `recipient` is
    /// where it lands.
    pub fn deliver(&self, recipient: &str, raw: &str) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.smtp)).expect("SMTP is reachable");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
        let expect = |reader: &mut BufReader<TcpStream>, code: &str| {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a reply");
                assert!(line.starts_with(code) || line.starts_with("250-"), "{line}");
                if line.len() < 4 || line.as_bytes()[3] == b' ' {
                    break;
                }
            }
        };
        expect(&mut reader, "220");
        for (send, code) in [
            ("EHLO fixture\r\n".to_string(), "250"),
            ("MAIL FROM:<seller@sellers.test>\r\n".to_string(), "250"),
            (format!("RCPT TO:<{recipient}>\r\n"), "250"),
            ("DATA\r\n".to_string(), "354"),
        ] {
            stream.write_all(send.as_bytes()).expect("written");
            expect(&mut reader, code);
        }
        stream
            .write_all(raw.as_bytes())
            .expect("the message is written");
        stream.write_all(b"\r\n.\r\n").expect("the end is written");
        expect(&mut reader, "250");
        stream.write_all(b"QUIT\r\n").expect("written");
    }

    /// Every message in `account`'s inbox, whole, oldest first, read through the plain IMAP port
    /// without changing anything about them (`BODY.PEEK`).
    pub fn inbox(&self, account: &Account) -> Vec<String> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.imap)).expect("IMAP is reachable");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("a greeting");
        let run = |stream: &mut TcpStream,
                   reader: &mut BufReader<TcpStream>,
                   tag: &str,
                   command: String| {
            stream
                .write_all(format!("{tag} {command}\r\n").as_bytes())
                .expect("written");
            let mut all = Vec::new();
            loop {
                let mut line = Vec::new();
                reader.read_until(b'\n', &mut line).expect("a line");
                if line.starts_with(format!("{tag} ").as_bytes()) {
                    break;
                }
                all.extend_from_slice(&line);
            }
            all
        };
        run(
            &mut stream,
            &mut reader,
            "a1",
            format!("LOGIN {} {}", account.login, account.password),
        );
        let selected = run(&mut stream, &mut reader, "a2", "EXAMINE INBOX".to_string());
        let exists: usize = String::from_utf8_lossy(&selected)
            .lines()
            .find_map(|line| line.strip_suffix(" EXISTS"))
            .and_then(|line| line.trim_start_matches("* ").parse().ok())
            .unwrap_or(0);
        let mut messages = Vec::new();
        for number in 1..=exists {
            stream
                .write_all(format!("b{number} FETCH {number} BODY.PEEK[]\r\n").as_bytes())
                .expect("written");
            let mut size = 0_usize;
            let mut body = Vec::new();
            loop {
                let mut line = Vec::new();
                reader.read_until(b'\n', &mut line).expect("a line");
                if line.starts_with(format!("b{number} ").as_bytes()) {
                    break;
                }
                let text = String::from_utf8_lossy(&line).into_owned();
                if size == 0
                    && text.starts_with('*')
                    && let Some(start) = text.rfind('{')
                    && let Some(end) = text.rfind('}')
                {
                    size = text[start + 1..end].parse().unwrap_or(0);
                    body = vec![0; size];
                    reader.read_exact(&mut body).expect("the body");
                }
            }
            messages.push(String::from_utf8_lossy(&body).into_owned());
        }
        run(&mut stream, &mut reader, "z", "LOGOUT".to_string());
        messages
    }

    /// How many messages of `account`'s inbox are unseen, by a `SEARCH UNSEEN` that changes nothing.
    pub fn unseen(&self, account: &Account) -> usize {
        let mut stream = TcpStream::connect(("127.0.0.1", self.imap)).expect("IMAP is reachable");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("a greeting");
        let mut run = |tag: &str, command: String| -> String {
            stream
                .write_all(format!("{tag} {command}\r\n").as_bytes())
                .expect("written");
            let mut all = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a line");
                if line.starts_with(&format!("{tag} ")) {
                    break;
                }
                all.push_str(&line);
            }
            all
        };
        run(
            "a1",
            format!("LOGIN {} {}", account.login, account.password),
        );
        run("a2", "EXAMINE INBOX".to_string());
        let found = run("a3", "SEARCH UNSEEN".to_string());
        run("z", "LOGOUT".to_string());
        found
            .lines()
            .find_map(|line| line.strip_prefix("* SEARCH"))
            .map_or(0, |ids| ids.split_whitespace().count())
    }

    /// Whether the container is gone.
    pub fn is_removed(&self) -> bool {
        docker(&[
            "ps",
            "-a",
            "-q",
            "--filter",
            &format!("name=^/{}$", self.name),
        ])
        .1
        .trim()
        .is_empty()
    }
}

impl Drop for GreenMail {
    fn drop(&mut self) {
        let _ = docker(&["rm", "-f", &self.name]);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A message of the story, built as a mail program would write it: a reply from `from` to `to`,
/// answering `in_reply_to` (a Message-ID without brackets), as plain text, as HTML alone, or with
/// attachments.
pub struct Mime<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub message_id: &'a str,
    pub in_reply_to: Option<&'a str>,
    pub text: Option<&'a str>,
    pub html: Option<&'a str>,
    pub attachments: Vec<(&'a str, &'a str, Vec<u8>)>,
}

impl Mime<'_> {
    /// The whole message with CRLF line ends.
    pub fn build(&self) -> String {
        use base64::Engine as _;
        let mut headers = format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\nMessage-ID: <{}>\r\nDate: Mon, 05 Oct 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\n",
            self.from, self.to, self.subject, self.message_id
        );
        if let Some(id) = self.in_reply_to {
            headers.push_str(&format!("In-Reply-To: <{id}>\r\nReferences: <{id}>\r\n"));
        }
        let body_part = |kind: &str, text: &str| {
            format!(
                "Content-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{text}\r\n"
            )
        };
        if self.attachments.is_empty() {
            return match (self.text, self.html) {
                (Some(text), _) => format!("{headers}{}", body_part("text/plain", text)),
                (None, Some(html)) => format!("{headers}{}", body_part("text/html", html)),
                (None, None) => headers,
            };
        }
        let boundary = "farik-test-boundary";
        let mut raw =
            format!("{headers}Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n");
        raw.push_str(&format!(
            "--{boundary}\r\n{}",
            body_part("text/plain", self.text.unwrap_or("See the attachment."))
        ));
        for (name, media, bytes) in &self.attachments {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let lines: Vec<&str> = encoded
                .as_bytes()
                .chunks(76)
                .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ascii"))
                .collect();
            raw.push_str(&format!(
                "--{boundary}\r\nContent-Type: {media}; name=\"{name}\"\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
                lines.join("\r\n")
            ));
        }
        raw.push_str(&format!("--{boundary}--\r\n"));
        raw
    }
}
