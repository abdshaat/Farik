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
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::CliIo;
use crate::daemon_client::exchange as ask;

/// Posts `body` to `route` on the daemon, as a hook's exchange: no daemon is a reason to deny,
/// as every other failure is.
fn exchange(daemon_file: &Path, route: &str, body: &str) -> Result<String, String> {
    ask(daemon_file, route, body, EXCHANGE_TIMEOUT).map_err(|error| error.to_string())
}

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

/// What a panic said, when it said it with a string.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_string())
}
