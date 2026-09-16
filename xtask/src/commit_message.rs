const TYPES: [&str; 9] = [
    "feat", "fix", "refactor", "test", "docs", "chore", "build", "ci", "perf",
];
const SUBJECT_LIMIT: usize = 72;

/// Checks the first non-comment line of a commit message against the Conventional Commits shape.
///
/// Accepts `<type>(<scope>): <subject>` with one of the nine types, a kebab-case scope, a
/// subject of at least two characters starting with a lower-case letter or a digit, no trailing
/// period, and at most 72 characters; merge and revert commits pass as they are.
///
/// # Errors
///
/// Returns the reason the message is rejected.
pub fn check_commit_message(message: &str) -> Result<(), String> {
    let first_line = message
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap_or_default();
    if first_line.starts_with("Merge ") || first_line.starts_with("Revert ") {
        return Ok(());
    }
    if first_line.chars().count() > SUBJECT_LIMIT {
        return Err(format!(
            "subject is {} characters; the limit is {SUBJECT_LIMIT}",
            first_line.chars().count()
        ));
    }
    if !has_conventional_shape(first_line) {
        return Err(format!(
            "subject \"{first_line}\" is not <type>(<scope>): <subject>"
        ));
    }
    Ok(())
}

fn has_conventional_shape(line: &str) -> bool {
    let Some((head, subject)) = line.split_once(": ") else {
        return false;
    };
    let Some((kind, scope)) = head.split_once('(') else {
        return false;
    };
    let Some(scope) = scope.strip_suffix(')') else {
        return false;
    };
    let scope_ok = scope.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && scope
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let subject_ok = subject
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && subject.chars().count() >= 2
        && !subject.ends_with('.');
    TYPES.contains(&kind) && scope_ok && subject_ok
}

#[cfg(test)]
mod tests {
    use super::check_commit_message;

    #[test]
    fn accepts_a_conventional_subject_with_a_type_and_a_scope() {
        assert_eq!(
            check_commit_message("feat(core): add result type\n\nbody\n"),
            Ok(())
        );
    }

    #[test]
    fn accepts_a_merge_commit() {
        assert_eq!(
            check_commit_message("Merge pull request #3 from abdshaat/phase/0-foundation"),
            Ok(())
        );
    }

    #[test]
    fn accepts_a_revert_commit() {
        assert_eq!(
            check_commit_message("Revert \"feat(core): add result type\""),
            Ok(())
        );
    }

    #[test]
    fn skips_comment_lines_when_finding_the_subject() {
        assert_eq!(
            check_commit_message("# Please enter the commit message\nfix(store): keep order"),
            Ok(())
        );
    }

    #[test]
    fn rejects_a_subject_without_a_type_and_a_scope() {
        assert_eq!(
            check_commit_message("Add result type"),
            Err("subject \"Add result type\" is not <type>(<scope>): <subject>".to_string())
        );
    }

    #[test]
    fn rejects_a_subject_longer_than_72_characters() {
        let subject = format!("feat(core): {}", "x".repeat(70));
        assert_eq!(
            check_commit_message(&subject),
            Err("subject is 82 characters; the limit is 72".to_string())
        );
    }

    #[test]
    fn rejects_a_subject_that_ends_with_a_period() {
        assert!(check_commit_message("feat(core): add result type.").is_err());
    }

    #[test]
    fn rejects_a_subject_that_starts_with_an_upper_case_letter() {
        assert!(check_commit_message("feat(core): Add result type").is_err());
    }

    #[test]
    fn rejects_a_type_that_is_not_one_of_the_nine() {
        assert!(check_commit_message("wip(core): add result type").is_err());
    }

    #[test]
    fn rejects_a_scope_that_is_not_kebab_case() {
        assert!(check_commit_message("feat(Core): add result type").is_err());
        assert!(check_commit_message("feat(core_io): add result type").is_err());
    }

    #[test]
    fn rejects_a_subject_without_a_scope() {
        assert!(check_commit_message("feat: add result type").is_err());
    }
}
