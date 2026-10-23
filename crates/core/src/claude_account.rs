//! What Catervas reads of Claude Code's account, from strings: whether it is signed in, the
//! address its sign-in page is at, and the outcome of the connection check (ADR 0084). Running
//! `claude` is the runtime's work; this module performs no I/O.

/// The kind of account Claude Code is signed in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// A Claude subscription (Pro or Max).
    Subscription,
    /// An Anthropic Console (API) account.
    Console,
    /// A key in the user's own environment.
    Environment,
    /// Any other sign-in.
    Other,
}

/// Claude Code's answer to `claude auth status --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    /// Whether Claude Code is signed in.
    pub signed_in: bool,
    /// The kind of sign-in, when signed in.
    pub kind: Option<AccountKind>,
}

/// The outcome of the connection check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The request was answered.
    Working,
    /// Anthropic refused the key (401 or 403).
    KeyRefused,
    /// The plan's limit is reached (429, or a rate or usage limit in the text).
    LimitReached,
    /// Claude Code is not signed in; the request was not sent.
    NotSignedIn,
    /// Anything else, in Claude Code's words.
    Failed(String),
}

/// The check's command line after the program, exactly as ADR 0084 records it.
pub const CHECK_ARGS: [&str; 15] = [
    "-p",
    "Reply with the single word OK.",
    "--model",
    "haiku",
    "--max-turns",
    "1",
    "--tools",
    "",
    "--system-prompt",
    "Reply briefly.",
    "--strict-mcp-config",
    "--setting-sources",
    "",
    "--output-format",
    "json",
];

/// The command line after the program of one question put to the owner's Claude Code on standard
/// input, in the lean shape ADR 0084 records for the check: Haiku, one turn, no tools, no
/// settings and no servers.
#[must_use]
pub fn ask_args(system_prompt: &str) -> Vec<String> {
    [
        "-p",
        "--model",
        "haiku",
        "--max-turns",
        "1",
        "--tools",
        "",
        "--system-prompt",
        system_prompt,
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--output-format",
        "json",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// What a question put to Claude Code answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// What it said.
    pub result: String,
    /// The `total_cost_usd` it reported, 0 when it reported none.
    pub cost_usd: f64,
}

/// Reads what `claude -p … --output-format json` printed.
///
/// # Errors
///
/// Claude Code's words when it answered with an error, else a sentence saying it gave no answer.
pub fn parse_answer(stdout: &str) -> Result<Answer, String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|_| "Claude Code gave no answer".to_string())?;
    let result = value
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    if value.get("is_error").and_then(serde_json::Value::as_bool) != Some(false) {
        return Err(if result.is_empty() {
            "Claude Code gave no answer".to_string()
        } else {
            result
        });
    }
    Ok(Answer {
        result,
        cost_usd: value
            .get("total_cost_usd")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
    })
}

impl CheckOutcome {
    /// The outcome's name on the wire.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::KeyRefused => "key_refused",
            Self::LimitReached => "limit_reached",
            Self::NotSignedIn => "not_signed_in",
            Self::Failed(_) => "failed",
        }
    }
}

/// Reads the check's JSON answer; `None` when it is not a JSON object. `NotSignedIn` is never
/// its answer: the runner decides that from the status first.
#[must_use]
pub fn check_outcome(stdout: &str) -> Option<CheckOutcome> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let object = value.as_object()?;
    if object.get("is_error").and_then(serde_json::Value::as_bool) == Some(false) {
        return Some(CheckOutcome::Working);
    }
    let result = object.get("result").and_then(serde_json::Value::as_str);
    let lowered = result.unwrap_or_default().to_ascii_lowercase();
    Some(
        match object
            .get("api_error_status")
            .and_then(serde_json::Value::as_u64)
        {
            Some(401 | 403) => CheckOutcome::KeyRefused,
            Some(429) => CheckOutcome::LimitReached,
            _ if lowered.contains("usage limit") || lowered.contains("rate limit") => {
                CheckOutcome::LimitReached
            }
            _ => CheckOutcome::Failed(
                result
                    .filter(|text| !text.is_empty())
                    .unwrap_or("Claude Code gave no answer")
                    .to_string(),
            ),
        },
    )
}

/// Reads `claude auth status --json`; `None` when it is not a JSON object with `loggedIn`.
#[must_use]
pub fn parse_status(stdout: &str) -> Option<AccountStatus> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let signed_in = value.get("loggedIn")?.as_bool()?;
    if !signed_in {
        return Some(AccountStatus {
            signed_in,
            kind: None,
        });
    }
    let method = value
        .get("authMethod")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = if method.contains("api_key") || method.contains("apikey") {
        AccountKind::Console
    } else if method.contains("oauth") || method.contains("claude") {
        AccountKind::Subscription
    } else {
        AccountKind::Other
    };
    Some(AccountStatus {
        signed_in,
        kind: Some(kind),
    })
}

/// The sign-in address in a line of `claude auth login`'s output, kept only on Anthropic's hosts.
#[must_use]
pub fn sign_in_address(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let address = line[start..].split_whitespace().next()?;
    if address.len() > MAX_ADDRESS {
        return None;
    }
    let host = url::Url::parse(address).ok()?.host_str()?.to_string();
    SIGN_IN_HOSTS
        .contains(&host.as_str())
        .then(|| address.to_string())
}

/// The hosts a sign-in address may be on.
const SIGN_IN_HOSTS: [&str; 4] = [
    "claude.com",
    "claude.ai",
    "platform.claude.com",
    "console.anthropic.com",
];

/// The longest sign-in address kept.
const MAX_ADDRESS: usize = 2048;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_whether_claude_code_is_signed_in() {
        let status = parse_status(
            r#"{"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"}"#,
        )
        .expect("a status");
        assert!(status.signed_in);
        assert_eq!(status.kind, Some(AccountKind::Subscription));
        let console = parse_status(r#"{"loggedIn": true, "authMethod": "api_key"}"#).unwrap();
        assert_eq!(console.kind, Some(AccountKind::Console));
        let camel = parse_status(r#"{"loggedIn": true, "authMethod": "apiKey"}"#).unwrap();
        assert_eq!(camel.kind, Some(AccountKind::Console));
        let none = parse_status(r#"{"loggedIn": false}"#).unwrap();
        assert!(!none.signed_in);
        assert_eq!(none.kind, None);
        let other = parse_status(r#"{"loggedIn": true}"#).unwrap();
        assert_eq!(other.kind, Some(AccountKind::Other));
        assert_eq!(parse_status("not json"), None);
    }

    #[test]
    fn takes_the_sign_in_address_only_from_anthropic() {
        let line = "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=x&state=y";
        assert_eq!(
            sign_in_address(line).as_deref(),
            Some("https://claude.com/cai/oauth/authorize?code=true&client_id=x&state=y")
        );
        assert_eq!(
            sign_in_address("https://console.anthropic.com/oauth/authorize?x").as_deref(),
            Some("https://console.anthropic.com/oauth/authorize?x")
        );
        assert_eq!(sign_in_address("https://evil.example/claude.com"), None);
        assert_eq!(sign_in_address("https://claude.com.evil.example/"), None);
        assert_eq!(sign_in_address("http://claude.com/"), None);
        assert_eq!(sign_in_address("Paste code here if prompted >"), None);
    }

    #[test]
    fn reads_the_outcome_of_a_check() {
        let good = r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","total_cost_usd":0.00039}"#;
        assert_eq!(check_outcome(good), Some(CheckOutcome::Working));
        let refused = |status: u32| {
            format!(
                r#"{{"type":"result","is_error":true,"api_error_status":{status},"result":"Invalid API key · Fix external API key"}}"#
            )
        };
        assert_eq!(check_outcome(&refused(401)), Some(CheckOutcome::KeyRefused));
        assert_eq!(check_outcome(&refused(403)), Some(CheckOutcome::KeyRefused));
        assert_eq!(
            check_outcome(&refused(429)),
            Some(CheckOutcome::LimitReached)
        );
        let usage = r#"{"is_error":true,"result":"Claude AI usage limit reached|1760000000"}"#;
        assert_eq!(check_outcome(usage), Some(CheckOutcome::LimitReached));
        let overloaded = r#"{"is_error":true,"api_error_status":500,"result":"Overloaded"}"#;
        assert_eq!(
            check_outcome(overloaded),
            Some(CheckOutcome::Failed("Overloaded".to_string()))
        );
        assert_eq!(check_outcome("not json"), None);
        assert_eq!(CheckOutcome::KeyRefused.code(), "key_refused");
        let expected = [
            "-p",
            "Reply with the single word OK.",
            "--model",
            "haiku",
            "--max-turns",
            "1",
            "--tools",
            "",
            "--system-prompt",
            "Reply briefly.",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--output-format",
            "json",
        ];
        assert_eq!(CHECK_ARGS.len(), expected.len());
        for (index, argument) in expected.iter().enumerate() {
            assert_eq!(CHECK_ARGS.get(index), Some(argument));
        }
    }
}
