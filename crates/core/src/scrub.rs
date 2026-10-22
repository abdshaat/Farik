//! What is removed from a text before Claude is shown it (`docs/SPEC.md` 8.6; ADR 0059): keys,
//! `KEY=value` lines whose name names a key, and the userinfo and query of an address. Pure.

use crate::team::{arg_is_a_key, flag_names_a_key, is_a_value};

/// What a removed value is replaced with.
const REMOVED: &str = "[removed]";

/// `text` without anything that looks like a key: a well-known service's key, a `KEY=value` line
/// whose name names a key (the rule of a connector's argument, spec 6.7), and the userinfo and the
/// query of an address.
#[must_use]
pub fn scrub(text: &str) -> String {
    let mut scrubbed = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, end) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        if names_a_key_line(body) {
            scrubbed.push_str(REMOVED);
        } else {
            scrub_words(body, &mut scrubbed);
        }
        scrubbed.push_str(end);
    }
    scrubbed
}

/// Whether the line is `NAME=value`, with an `export ` before it or not, where `NAME` names a key.
fn names_a_key_line(line: &str) -> bool {
    let line = line.trim();
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    line.split_once('=').is_some_and(|(name, value)| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && !name.starts_with('-')
            && flag_names_a_key(name)
            && is_a_value(value.trim())
    })
}

/// Pushes `line` to `out` with each word that is a key or an address with a secret in it cleaned.
fn scrub_words(line: &str, out: &mut String) {
    let mut rest = line;
    while !rest.is_empty() {
        let blank = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..blank]);
        rest = &rest[blank..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        scrub_word(&rest[..end], out);
        rest = &rest[end..];
    }
}

/// Pushes `word` to `out`, cleaned.
fn scrub_word(word: &str, out: &mut String) {
    let edge = |c: char| {
        matches!(
            c,
            '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | ',' | ';' | '<' | '>'
        )
    };
    let core = word.trim_matches(edge);
    let at = word.find(core).unwrap_or(0);
    out.push_str(&word[..at]);
    if core.contains("://") {
        out.push_str(&without_secrets(core));
    } else if arg_is_a_key(core) || is_a_slack_key(core) {
        out.push_str(REMOVED);
    } else {
        out.push_str(core);
    }
    out.push_str(&word[at + core.len()..]);
}

/// Whether `word` is a Slack key such as `xoxb-…`, `xoxa-…` or `xoxs-…`.
fn is_a_slack_key(word: &str) -> bool {
    let mut letters = word.chars();
    word.starts_with("xox")
        && letters.nth(3).is_some_and(|c| c.is_ascii_lowercase())
        && letters.next() == Some('-')
}

/// The address without the name and password before its `@` and without its query and fragment.
fn without_secrets(address: &str) -> String {
    let Some((scheme, rest)) = address.split_once("://") else {
        return address.to_string();
    };
    let cut = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(cut);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let path = tail.split(['?', '#']).next().unwrap_or_default();
    format!("{scheme}://{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::scrub;

    #[test]
    fn scrubs_keys_before_claude_sees_them() {
        let github = format!(
            "token: ghp_{}",
            "a1B2c3D4e5".repeat(4).chars().take(36).collect::<String>()
        );
        let anthropic = format!("sk-ant-api03-{}", "x9Y8z7".repeat(5));
        let text = format!(
            "OPENAI_API_KEY=abc123\n{github}\nsee https://user:pw@host/x?key=1 now\nuse {anthropic} here\nMAX_TOKENS=4096\nrun --sort-key name\n"
        );
        let scrubbed = scrub(&text);
        let lines: Vec<&str> = scrubbed.lines().collect();
        assert_eq!(lines[0], "[removed]");
        assert_eq!(lines[1], "token: [removed]");
        assert_eq!(lines[2], "see https://host/x now");
        assert_eq!(lines[3], "use [removed] here");
        assert_eq!(lines[4], "MAX_TOKENS=4096");
        assert_eq!(lines[5], "run --sort-key name");
        for secret in ["abc123", "ghp_", "pw", "sk-ant"] {
            assert!(!scrubbed.contains(secret), "{secret} in {scrubbed}");
        }
        // A key with no line of its own, AWS's and Slack's among them.
        let more = scrub(&format!(
            "AKIA{} and xoxb-{} and github_pat_{}",
            "ABCDEFGHIJKLMNOP", "1234567890-abcdef", "11ABCDEFG0abcdefghij"
        ));
        assert_eq!(more, "[removed] and [removed] and [removed]");
    }
}
