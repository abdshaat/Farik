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

/// What `pnpm` is given, one command after another, once the Rust checks have passed.
///
/// Nothing when the workspace has no `package.json`, so a checkout without the front end checks as
/// before.
#[must_use]
pub fn front_end_commands(has_package_json: bool) -> Vec<Vec<&'static str>> {
    if has_package_json {
        vec![vec!["install", "--frozen-lockfile"], vec!["check"]]
    } else {
        vec![]
    }
}

/// What `pnpm` is given under `--integration` before the first cargo command: the brand's tokens
/// generated, then the web app built. A farik compiled while `apps/web/dist` is missing serves "not
/// built" until the crate is compiled again, so every farik the check compiles comes after the app.
#[must_use]
pub fn web_app_first(tests: Tests) -> Vec<Vec<&'static str>> {
    match tests {
        Tests::WithoutTheOnesThatNeedAProgram => vec![],
        Tests::All => vec![
            vec!["-r", "--if-present", "generate"],
            vec!["--filter", "@farik/web", "build"],
        ],
    }
}

/// What runs after `pnpm check` under `--integration`, one `(program, args)` after another: the
/// end-to-end server linted, tested and built, and the browser journey run over it and the app
/// [`web_app_first`] built.
/// Nothing without the flag.
///
/// The journey also runs `target/debug/farik` (`farik init`, `farik log`). It is not built here: the
/// check's `cargo test --workspace`, and the step's own `cargo test -p farik`, build the package's
/// binaries for its integration tests, so it exists by the time the journey runs.
#[must_use]
pub fn integration_steps(tests: Tests) -> Vec<(&'static str, Vec<&'static str>)> {
    match tests {
        Tests::WithoutTheOnesThatNeedAProgram => vec![],
        Tests::All => vec![
            (
                "cargo",
                vec![
                    "clippy",
                    "-p",
                    "farik",
                    "--features",
                    "e2e",
                    "--bin",
                    "farik-e2e-serve",
                    "--",
                    "-D",
                    "warnings",
                ],
            ),
            (
                "cargo",
                // Its tests need git, so they are `#[ignore]`d, and without this none of them runs.
                vec![
                    "test",
                    "-p",
                    "farik",
                    "--features",
                    "e2e",
                    "--test",
                    "serving",
                    "--",
                    "--include-ignored",
                ],
            ),
            (
                "cargo",
                vec![
                    "build",
                    "-p",
                    "farik",
                    "--features",
                    "e2e",
                    "--bin",
                    "farik-e2e-serve",
                ],
            ),
            ("pnpm", vec!["--filter", "@farik/web", "e2e"]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Tests, front_end_commands, integration_steps, test_arguments, tests_requested,
        web_app_first,
    };

    #[test]
    fn builds_the_web_app_before_any_farik_is_compiled_under_integration() {
        // A farik compiled while `apps/web/dist` is missing serves "not built" even after the app
        // is built, so the app comes first. The tokens come before it: a fresh checkout has none.
        assert_eq!(
            web_app_first(Tests::All),
            [
                vec!["-r", "--if-present", "generate"],
                vec!["--filter", "@farik/web", "build"],
            ]
        );
        assert!(web_app_first(Tests::WithoutTheOnesThatNeedAProgram).is_empty());
    }

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

    #[test]
    fn runs_pnpm_install_and_check_when_the_workspace_has_a_package_json() {
        assert_eq!(
            front_end_commands(true),
            [vec!["install", "--frozen-lockfile"], vec!["check"]]
        );
    }

    #[test]
    fn runs_no_front_end_command_without_a_package_json() {
        assert!(front_end_commands(false).is_empty());
    }

    #[test]
    fn integration_steps_build_the_app_and_run_the_journey() {
        // In this order: the journey runs the built server.
        assert_eq!(
            integration_steps(Tests::All),
            [
                (
                    "cargo",
                    vec![
                        "clippy",
                        "-p",
                        "farik",
                        "--features",
                        "e2e",
                        "--bin",
                        "farik-e2e-serve",
                        "--",
                        "-D",
                        "warnings",
                    ]
                ),
                (
                    "cargo",
                    vec![
                        "test",
                        "-p",
                        "farik",
                        "--features",
                        "e2e",
                        "--test",
                        "serving",
                        "--",
                        "--include-ignored",
                    ]
                ),
                (
                    "cargo",
                    vec![
                        "build",
                        "-p",
                        "farik",
                        "--features",
                        "e2e",
                        "--bin",
                        "farik-e2e-serve",
                    ]
                ),
                ("pnpm", vec!["--filter", "@farik/web", "e2e"]),
            ]
        );
        assert!(integration_steps(Tests::WithoutTheOnesThatNeedAProgram).is_empty());
    }
}
