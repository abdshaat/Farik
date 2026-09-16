const FORBIDDEN: [&str; 7] = [
    "std::fs",
    "std::net",
    "std::process",
    "std::env",
    "std::time::SystemTime",
    "tokio",
    "rand",
];

/// Reports every line of `farik-core` that mentions a forbidden path or crate (`std::fs`,
/// `std::net`, `std::process`, `std::env`, `std::time::SystemTime`, `tokio`, `rand`), as
/// `path:line: uses <token>`. A token counts only on identifier boundaries, so prose such as
/// "understand" or an identifier such as `rand_seed` is not a finding.
#[must_use]
pub fn find_core_io(files: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, text) in files {
        for (index, line) in text.lines().enumerate() {
            if let Some(token) = FORBIDDEN.iter().find(|token| mentions(line, token)) {
                findings.push(format!("{path}:{}: uses {token}", index + 1));
            }
        }
    }
    findings
}

fn mentions(line: &str, token: &str) -> bool {
    line.match_indices(token).any(|(start, _)| {
        let before_ok = !line[..start].chars().next_back().is_some_and(is_identifier);
        let after_ok = !line[start + token.len()..]
            .chars()
            .next()
            .is_some_and(is_identifier);
        before_ok && after_ok
    })
}

fn is_identifier(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::find_core_io;

    fn files(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
            .collect()
    }

    #[test]
    fn reports_a_forbidden_path_with_its_file_line_and_token() {
        let text = "//! doc\npub fn read() -> String { std::fs::read_to_string(\"x\").unwrap() }\n";
        assert_eq!(
            find_core_io(&files(&[("crates/core/src/a.rs", text)])),
            vec!["crates/core/src/a.rs:2: uses std::fs".to_string()]
        );
    }

    #[test]
    fn reports_a_forbidden_crate_named_on_its_own() {
        let text = "use tokio::sync::Mutex;\nlet n = rand::random::<u8>();\n";
        assert_eq!(
            find_core_io(&files(&[("a.rs", text)])),
            vec![
                "a.rs:1: uses tokio".to_string(),
                "a.rs:2: uses rand".to_string()
            ]
        );
    }

    #[test]
    fn ignores_a_token_inside_a_longer_word() {
        let text = "/// The reader must understand the operand's brand of randomness.\n";
        assert!(find_core_io(&files(&[("a.rs", text)])).is_empty());
    }

    #[test]
    fn ignores_a_token_inside_an_identifier() {
        let text = "let rand_seed = 1;\nlet my_tokio = 2;\nlet randomize = 3;\n";
        assert!(find_core_io(&files(&[("a.rs", text)])).is_empty());
    }

    #[test]
    fn reports_the_system_clock_but_not_a_duration() {
        let text = "use std::time::Duration;\nlet now = std::time::SystemTime::now();\n";
        assert_eq!(
            find_core_io(&files(&[("a.rs", text)])),
            vec!["a.rs:2: uses std::time::SystemTime".to_string()]
        );
    }
}
