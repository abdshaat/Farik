//! Which tests `cargo xtask check` runs, and the flag that says so.

/// Which tests the check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tests {
    /// Every test but the ones marked `#[ignore]` because they need a program the toolchain does
    /// not bring. The default: the check still compiles and lints those, and `cargo test` prints
    /// them as ignored rather than passing silently.
    WithoutTheOnesThatNeedAProgram,
    /// Those, and the ignored ones. What continuous integration runs.
    All,
}

/// What a flag given to `check` asks for.
///
/// # Errors
///
/// The usage line, when the flag is not one `check` knows.
pub fn tests_requested(flag: Option<&str>) -> Result<Tests, String> {
    match flag {
        None => Ok(Tests::WithoutTheOnesThatNeedAProgram),
        Some("--integration") => Ok(Tests::All),
        Some(unknown) => Err(format!(
            "unknown flag {unknown}; usage: cargo xtask check [--integration]"
        )),
    }
}

/// What `cargo` is given to run those tests.
///
/// Here rather than in `main.rs` because this is the half that matters: the flag's *effect* is one
/// argument, continuous integration is one job running one command, and nothing else in the
/// repository would notice if that argument went missing.
#[must_use]
pub fn test_arguments(tests: Tests) -> Vec<&'static str> {
    match tests {
        Tests::WithoutTheOnesThatNeedAProgram => vec!["test", "--workspace"],
        // `--include-ignored` rather than `--ignored`: this runs everything, so one command is the
        // whole check rather than half of it.
        Tests::All => vec!["test", "--workspace", "--", "--include-ignored"],
    }
}

#[cfg(test)]
mod tests {
    use super::{Tests, test_arguments, tests_requested};

    #[test]
    fn runs_the_tests_that_need_no_program_when_asked_for_nothing() {
        assert_eq!(
            tests_requested(None),
            Ok(Tests::WithoutTheOnesThatNeedAProgram)
        );
    }

    #[test]
    fn runs_everything_when_asked_for_the_integration_tests() {
        assert_eq!(tests_requested(Some("--integration")), Ok(Tests::All));
    }

    #[test]
    fn runs_the_workspace_and_stops_there_when_the_flag_is_not_given() {
        assert_eq!(
            test_arguments(Tests::WithoutTheOnesThatNeedAProgram),
            ["test", "--workspace"]
        );
    }

    #[test]
    fn asks_cargo_for_the_ignored_tests_when_it_is_asked_for_everything() {
        // The one argument that runs the tests marked `#[ignore]`. Continuous integration is a
        // single job running a single command, so if this went missing the whole integration suite
        // would stop running and the check would still say ok.
        assert_eq!(
            test_arguments(Tests::All),
            ["test", "--workspace", "--", "--include-ignored"]
        );
    }

    #[test]
    fn says_what_the_usage_is_when_the_flag_is_not_one() {
        // Not silently the default: a flag with a typo in it would then run a smaller check than
        // the one continuous integration is asking for and say nothing about it.
        assert_eq!(
            tests_requested(Some("--intergration")),
            Err(
                "unknown flag --intergration; usage: cargo xtask check [--integration]".to_string()
            )
        );
    }
}
