//! Allowed and protected paths (`docs/SPEC.md` sections 5.4 item 2, 5.6, and 5.12): which files a
//! task may change, and which files no tool may touch at all.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// A path that a check refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathViolation {
    /// The path, as it was given.
    pub path: String,
}

/// A glob that does not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobError {
    /// The pattern is not a valid glob.
    Invalid {
        /// The pattern as written.
        pattern: String,
        /// What is wrong with it, in the glob engine's words.
        detail: String,
    },
}

/// Why a path check refuses: paths that break the rule, or a rule that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathRefusal {
    /// The paths that break the rule, in the order they were given.
    Violations(Vec<PathViolation>),
    /// A glob in the rule does not compile; nothing was checked.
    Glob(GlobError),
}

/// Refuses every changed path that matches none of the allowed globs (spec 5.4 item 2). With
/// no allowed glob, every change is refused. Paths are relative to the project root with forward
/// slashes, compared as given; `*` stays within one directory and `**` crosses directories. A
/// path with a `..` segment is always refused, so that no path climbs out of what a glob names.
///
/// # Errors
///
/// `PathRefusal::Violations` with every path outside the allowed globs, or
/// `PathRefusal::Glob` when an allowed glob does not compile.
pub fn check_allowed_paths(
    changed: &[String],
    allowed_globs: &[String],
) -> Result<(), PathRefusal> {
    let allowed = compile(allowed_globs)?;
    refuse(
        changed
            .iter()
            .filter(|path| climbs(path) || !allowed.is_match(path)),
    )
}

/// Refuses every path that matches a protected glob (spec 5.6 and 5.12), whatever the tool's
/// tier. A bare name such as `.env` names the file at the project root only; `**/.env` names it
/// anywhere. A path with a `..` segment is always refused, so that no path reaches a protected
/// file by climbing.
///
/// # Errors
///
/// `PathRefusal::Violations` with every protected path, or `PathRefusal::Glob` when a protected
/// glob does not compile.
pub fn check_protected_paths(
    paths: &[String],
    protected_globs: &[String],
) -> Result<(), PathRefusal> {
    let protected = compile(protected_globs)?;
    refuse(
        paths
            .iter()
            .filter(|path| climbs(path) || protected.is_match(path)),
    )
}

fn climbs(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}

fn compile(globs: &[String]) -> Result<GlobSet, PathRefusal> {
    let mut builder = GlobSetBuilder::new();
    for pattern in globs {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| {
                PathRefusal::Glob(GlobError::Invalid {
                    pattern: pattern.clone(),
                    detail: error.kind().to_string(),
                })
            })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        PathRefusal::Glob(GlobError::Invalid {
            pattern: error.glob().unwrap_or_default().to_string(),
            detail: error.kind().to_string(),
        })
    })
}

fn refuse<'a>(violations: impl Iterator<Item = &'a String>) -> Result<(), PathRefusal> {
    let violations: Vec<PathViolation> = violations
        .map(|path| PathViolation { path: path.clone() })
        .collect();
    if violations.is_empty() {
        Ok(())
    } else {
        Err(PathRefusal::Violations(violations))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GlobError, PathRefusal, PathViolation, check_allowed_paths, check_protected_paths,
    };

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn violations(paths: &[&str]) -> PathRefusal {
        PathRefusal::Violations(
            paths
                .iter()
                .map(|path| PathViolation {
                    path: (*path).to_string(),
                })
                .collect(),
        )
    }

    #[test]
    fn accepts_changes_inside_the_allowed_paths() {
        assert_eq!(
            check_allowed_paths(
                &strings(&["src/login/form.rs", "src/login/tests/form.rs"]),
                &strings(&["src/login/**"])
            ),
            Ok(())
        );
    }

    #[test]
    fn refuses_every_change_outside_the_allowed_paths_in_order() {
        assert_eq!(
            check_allowed_paths(
                &strings(&["README.md", "src/login/form.rs", "src/billing/invoice.rs"]),
                &strings(&["src/login/**", "docs/**"])
            ),
            Err(violations(&["README.md", "src/billing/invoice.rs"]))
        );
    }

    #[test]
    fn refuses_every_change_when_nothing_is_allowed() {
        assert_eq!(
            check_allowed_paths(&strings(&["src/login/form.rs"]), &[]),
            Err(violations(&["src/login/form.rs"]))
        );
    }

    #[test]
    fn accepts_no_changes_at_all() {
        assert_eq!(check_allowed_paths(&[], &[]), Ok(()));
    }

    #[test]
    fn keeps_a_single_star_within_one_directory() {
        let allowed = strings(&["src/*.rs"]);
        assert_eq!(
            check_allowed_paths(&strings(&["src/lib.rs"]), &allowed),
            Ok(())
        );
        assert_eq!(
            check_allowed_paths(&strings(&["src/login/form.rs"]), &allowed),
            Err(violations(&["src/login/form.rs"]))
        );
    }

    #[test]
    fn refuses_a_path_that_matches_a_protected_glob() {
        let protected = strings(&[".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"]);
        assert_eq!(
            check_protected_paths(
                &strings(&[
                    "src/env.rs",
                    ".env",
                    ".env.local",
                    "certs/server.pem",
                    ".farik/local/settings.yaml"
                ]),
                &protected
            ),
            Err(violations(&[
                ".env",
                ".env.local",
                "certs/server.pem",
                ".farik/local/settings.yaml"
            ]))
        );
    }

    #[test]
    fn matches_a_bare_protected_name_at_the_root_only() {
        assert_eq!(
            check_protected_paths(&strings(&["config/.env"]), &strings(&[".env"])),
            Ok(())
        );
        assert_eq!(
            check_protected_paths(&strings(&["config/.env"]), &strings(&["**/.env"])),
            Err(violations(&["config/.env"]))
        );
    }

    #[test]
    fn accepts_paths_that_match_no_protected_glob() {
        assert_eq!(
            check_protected_paths(
                &strings(&["src/lib.rs", "docs/SPEC.md"]),
                &strings(&["**/*.pem"])
            ),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_path_with_a_parent_segment_whatever_the_globs() {
        assert_eq!(
            check_allowed_paths(&strings(&["src/../.env"]), &strings(&["**"])),
            Err(violations(&["src/../.env"]))
        );
        assert_eq!(
            check_protected_paths(&strings(&["src/../.env"]), &[]),
            Err(violations(&["src/../.env"]))
        );
    }

    #[test]
    fn refuses_a_glob_that_does_not_compile_and_names_it() {
        let Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) =
            check_allowed_paths(&strings(&["src/lib.rs"]), &strings(&["src/[rs"]))
        else {
            panic!("expected a glob error");
        };
        assert_eq!(pattern, "src/[rs");
        assert!(!detail.is_empty(), "{detail}");
        assert!(check_protected_paths(&strings(&["src/lib.rs"]), &strings(&["src/[rs"])).is_err());
    }
}
