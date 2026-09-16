const MARKERS: [&str; 2] = ["TODO", "FIXME"];
const SELF: &str = "xtask/src/";

/// Reports every `TODO` or `FIXME` that does not carry a task id `(FRK-<n>)`, an issue `(#<n>)`,
/// or a link `(http...)`, as `path:line`, skipping the xtask sources that describe the rule.
#[must_use]
pub fn find_bare_todos(files: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, text) in files {
        if path.starts_with(SELF) {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            if has_bare_marker(line) {
                findings.push(format!("{path}:{}", index + 1));
            }
        }
    }
    findings
}

fn has_bare_marker(line: &str) -> bool {
    MARKERS.iter().any(|marker| {
        line.match_indices(marker).any(|(start, _)| {
            let before_ok = start == 0
                || !line[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
            let rest = &line[start + marker.len()..];
            let after_ok = !rest
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric());
            before_ok && after_ok && !has_reference(rest)
        })
    })
}

fn has_reference(rest: &str) -> bool {
    let Some(inner) = rest.strip_prefix('(') else {
        return false;
    };
    let Some((reference, _)) = inner.split_once(')') else {
        return false;
    };
    let is_task = reference
        .strip_prefix("FRK-")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let is_issue = reference
        .strip_prefix('#')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let is_link = reference.starts_with("http://") || reference.starts_with("https://");
    is_task || is_issue || is_link
}

#[cfg(test)]
mod tests {
    use super::find_bare_todos;

    fn marker() -> String {
        ["TO", "DO"].concat()
    }

    fn files(entries: &[(&str, String)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.clone()))
            .collect()
    }

    #[test]
    fn reports_a_bare_marker_with_its_path_and_line() {
        let text = format!("let a = 1;\n// {} fix this\n", marker());
        assert_eq!(
            find_bare_todos(&files(&[("crates/core/src/a.rs", text)])),
            vec!["crates/core/src/a.rs:2".to_string()]
        );
    }

    #[test]
    fn accepts_a_marker_that_carries_a_task_id() {
        let text = format!("// {}(FRK-12) fix this\n", marker());
        assert!(find_bare_todos(&files(&[("a.rs", text)])).is_empty());
    }

    #[test]
    fn accepts_a_marker_that_carries_an_issue_number_or_a_link() {
        let a = format!("// {}(#12) fix this\n", marker());
        let b = format!(
            "// {}(https://github.com/abdshaat/farik/issues/12) fix\n",
            marker()
        );
        assert!(find_bare_todos(&files(&[("a.rs", a), ("b.rs", b)])).is_empty());
    }

    #[test]
    fn reports_a_bare_fixme_too() {
        let text = format!("// {} later\n", ["FIX", "ME"].concat());
        assert_eq!(
            find_bare_todos(&files(&[("a.rs", text)])),
            vec!["a.rs:1".to_string()]
        );
    }

    #[test]
    fn skips_its_own_source() {
        let text = format!("const BARE: &str = \"{}\";\n", marker());
        assert!(find_bare_todos(&files(&[("xtask/src/todos.rs", text)])).is_empty());
    }
}
