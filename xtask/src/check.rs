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

#[cfg(test)]
mod tests {
    use super::{Tests, tests_requested};

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
