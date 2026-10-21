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
}
