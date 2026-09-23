//! One exchange with the daemon a `daemon.json` names: the hooks' and the human's commands'.
//!
//! One plain HTTP/1.1 exchange, because the peer is always the daemon on this machine and an HTTP
//! client would be a dependency for forty lines.

use std::fmt;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

/// Why the daemon could not be asked, or did not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// There is no `daemon.json`, so no daemon serves.
    NoDaemon {
        /// What reading the file said.
        detail: String,
    },
    /// The file, the connection, or the answer failed.
    Failed {
        /// What failed.
        detail: String,
    },
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDaemon { detail } | Self::Failed { detail } => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for ClientError {}

/// What a `daemon.json` says: where the daemon listens, its token, and its process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonAddress {
    /// The port on `127.0.0.1`.
    pub port: u16,
    /// The bearer token.
    pub token: String,
    /// The daemon's process, when the file names it.
    pub pid: Option<u64>,
}

/// Reads `daemon_file`.
///
/// # Errors
///
/// `NoDaemon` when there is no such file; `Failed` when it cannot be read or names no port and
/// token.
pub fn read_daemon_file(daemon_file: &Path) -> Result<DaemonAddress, ClientError> {
    let text = std::fs::read_to_string(daemon_file).map_err(|error| {
        let detail = format!(
            "the daemon file {} cannot be read, so no daemon can be asked: {error}",
            daemon_file.display()
        );
        if error.kind() == std::io::ErrorKind::NotFound {
            ClientError::NoDaemon { detail }
        } else {
            ClientError::Failed { detail }
        }
    })?;
    let info: Value = serde_json::from_str(&text).map_err(|error| ClientError::Failed {
        detail: format!(
            "the daemon file {} is not JSON: {error}",
            daemon_file.display()
        ),
    })?;
    let (Some(port), Some(token)) = (
        info["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok()),
        info["token"].as_str(),
    ) else {
        return Err(ClientError::Failed {
            detail: format!(
                "the daemon file {} names no port and token",
                daemon_file.display()
            ),
        });
    };
    Ok(DaemonAddress {
        port,
        token: token.to_string(),
        pid: info["pid"].as_u64(),
    })
}

/// Posts `body` to `route` on the daemon `daemon_file` names, with its token, and answers with
/// the body of a 200. Connecting, sending, and waiting for the answer may each take `timeout`.
///
/// # Errors
///
/// `NoDaemon` when there is no daemon file; `Failed` when the file, the connection, or the answer
/// fails, or the status is not 200.
pub fn exchange(
    daemon_file: &Path,
    route: &str,
    body: &str,
    timeout: Duration,
) -> Result<String, ClientError> {
    let DaemonAddress { port, token, .. } = read_daemon_file(daemon_file)?;
    let failed = |detail: String| ClientError::Failed { detail };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|error| {
        failed(format!(
            "the daemon at {address} cannot be reached: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|error| failed(format!("the connection cannot be timed: {error}")))?;
    let request = format!(
        "POST {route} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        failed(format!(
            "the daemon at {address} cannot be written to: {error}"
        ))
    })?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|error| failed(format!("the daemon at {address} did not answer: {error}")))?;
    body_of(&String::from_utf8_lossy(&answer)).map_err(failed)
}

/// The body of an HTTP/1.1 response, when its status is 200.
fn body_of(response: &str) -> Result<String, String> {
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("the daemon did not answer with HTTP: {response}"))?;
    let status = head.lines().next().unwrap_or_default();
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!("the daemon answered {status}: {body}"));
    }
    let chunked = head.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding") && value.trim() == "chunked"
        })
    });
    Ok(if chunked {
        unchunked(body)
    } else {
        body.to_string()
    })
}

/// A chunked body put back together.
fn unchunked(mut body: &str) -> String {
    let mut whole = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || rest.len() < size {
            break;
        }
        whole.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    whole
}
