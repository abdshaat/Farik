//! `farik hook pre-tool-use` and `farik hook post-tool-use` (`docs/SPEC.md` 8.2): Claude Code runs
//! them around every tool call with the call's JSON on standard input, and they carry it to the
//! daemon named by `--daemon` and its answer back.
//!
//! The pre-tool-use hook fails closed. Anything that goes wrong — no daemon file, no daemon, a
//! daemon that never answers, an answer that is not a decision — prints a deny with the reason and
//! exits 0, which is how Claude Code reads a deny; a panic exits 2 with the reason on standard
//! error, the one other code Claude Code treats as blocking. The exchange times out well under
//! Claude Code's own sixty seconds, because a hook that times out does not block the call.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::CliIo;

/// How long connecting, sending, and waiting for the answer may each take.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);
/// The code Claude Code reads as "block this call" when the hook itself failed.
const BLOCKING: i32 = 2;

/// Asks the daemon whether the tool call on standard input may go ahead, and prints its answer:
/// Claude Code's `hookSpecificOutput`. Prints a deny, with the reason, whenever it cannot ask.
/// Answers 0, or 2 when the hook itself panicked.
pub fn pre_tool_use(daemon_file: &Path, io: &mut CliIo<'_>) -> i32 {
    let asked = catch_unwind(AssertUnwindSafe(|| {
        let mut input = String::new();
        io.stdin
            .read_to_string(&mut input)
            .map_err(|error| format!("the hook's input cannot be read: {error}"))
            .and_then(|_| exchange(daemon_file, "/hook/pre-tool-use", &input))
            .and_then(|answer| decision_in(&answer))
    }));
    match asked {
        Ok(Ok(decision)) => {
            let _ = writeln!(io.stdout, "{decision}");
            0
        }
        Ok(Err(reason)) => {
            let _ = writeln!(io.stdout, "{}", deny(&reason));
            0
        }
        Err(panic) => {
            let _ = writeln!(
                io.stderr,
                "farik hook pre-tool-use failed, so the call is blocked: {}",
                panic_message(panic.as_ref())
            );
            BLOCKING
        }
    }
}

/// Tells the daemon what the tool call on standard input returned. Prints nothing and answers 0
/// whatever happened: the call is over, and there is nothing left to block.
pub fn post_tool_use(daemon_file: &Path, io: &mut CliIo<'_>) -> i32 {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mut input = String::new();
        if io.stdin.read_to_string(&mut input).is_ok() {
            let _ = exchange(daemon_file, "/hook/post-tool-use", &input);
        }
    }));
    0
}

/// A deny in Claude Code's shape.
fn deny(reason: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!("hook_failed: {reason}"),
        }
    })
}

/// The daemon's answer, if it is a decision Claude Code can read.
fn decision_in(answer: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(answer)
        .map_err(|error| format!("the daemon's answer is not JSON: {error}"))?;
    let decision = value["hookSpecificOutput"]["permissionDecision"].as_str();
    if matches!(decision, Some("allow" | "deny")) {
        Ok(value)
    } else {
        Err(format!("the daemon's answer is not a decision: {value}"))
    }
}

/// Posts `body` to `route` on the daemon `daemon_file` names, with its token, and answers with
/// the body of a 200. One plain HTTP/1.1 exchange, because the peer is always the daemon on this
/// machine and an HTTP client would be a dependency for forty lines.
fn exchange(daemon_file: &Path, route: &str, body: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(daemon_file).map_err(|error| {
        format!(
            "the daemon file {} cannot be read, so no daemon can be asked: {error}",
            daemon_file.display()
        )
    })?;
    let info: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "the daemon file {} is not JSON: {error}",
            daemon_file.display()
        )
    })?;
    let (Some(port), Some(token)) = (
        info["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok()),
        info["token"].as_str(),
    ) else {
        return Err(format!(
            "the daemon file {} names no port and token",
            daemon_file.display()
        ));
    };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, EXCHANGE_TIMEOUT)
        .map_err(|error| format!("the daemon at {address} cannot be reached: {error}"))?;
    stream
        .set_read_timeout(Some(EXCHANGE_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(EXCHANGE_TIMEOUT)))
        .map_err(|error| format!("the connection cannot be timed: {error}"))?;
    let request = format!(
        "POST {route} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("the daemon at {address} cannot be written to: {error}"))?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|error| format!("the daemon at {address} did not answer: {error}"))?;
    body_of(&String::from_utf8_lossy(&answer))
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

/// What a panic said, when it said it with a string.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_string())
}
