//! A project folder's member repositories (`docs/SPEC.md` sections 3, 4.1 and 8.4, ADR 0057):
//! `docs/schemas/repos.schema.json` as Rust types, the validator that turns an untrusted JSON value
//! into one, and the rules for a member beyond the schema. Core does no I/O, so every rule here is
//! on strings; the caller canonicalises the paths first.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::pointer;

pub use crate::contract::ValidationError;
pub use crate::generated::repos::{Repo, RepoName, Repos};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/repos.schema.json");

/// The most member repositories a project folder holds.
pub const MAX_REPOS: usize = 16;

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded repos schema is valid JSON: it is the file in docs/schemas/ that typify \
         generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded repos schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Checks a value against `docs/schemas/repos.schema.json` and the rules the schema cannot say:
/// names are unique, paths are unique, and no path is inside another. Paths are compared as the
/// file holds them, so a caller that wants symbolic links followed canonicalises before it writes.
///
/// # Errors
///
/// Every schema violation, each at its own JSON pointer; or, when the schema passes, one error per
/// repeated name and one per path that repeats or lies inside another; or, when the schema passes
/// and the typed list cannot be built, one error at the root.
pub fn validate_repos(input: &Value) -> Result<Repos, Vec<ValidationError>> {
    let errors: Vec<ValidationError> = VALIDATOR
        .iter_errors(input)
        .map(|error| ValidationError {
            path: pointer(&error.instance_path().to_string()),
            message: error.to_string(),
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let repos = serde_json::from_value::<Repos>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!("the schema passed but the typed list could not be built: {error}"),
        }]
    })?;
    let mut errors = Vec::new();
    for (index, repo) in repos.repos.iter().enumerate() {
        for earlier in &repos.repos[..index] {
            if earlier.name.as_str() == repo.name.as_str() {
                errors.push(ValidationError {
                    path: format!("/repos/{index}/name"),
                    message: format!(
                        "a repository's name names one repository, and {} more than one",
                        repo.name.as_str()
                    ),
                });
            }
            let (path, other) = (repo.path.as_str(), earlier.path.as_str());
            let message = if segments(path) == segments(other) {
                Some(format!(
                    "{path} is named twice, as {} and {}",
                    earlier.name.as_str(),
                    repo.name.as_str()
                ))
            } else if is_within(path, other) {
                Some(inside(path, other))
            } else if is_within(other, path) {
                Some(inside(other, path))
            } else {
                None
            };
            if let Some(message) = message {
                errors.push(ValidationError {
                    path: format!("/repos/{index}/path"),
                    message,
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(repos)
    } else {
        Err(errors)
    }
}

fn inside(path: &str, other: &str) -> String {
    format!("{path} is inside {other}, another of the project's repositories")
}

/// Why a new member repository is refused, once the file system has accepted its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberRefusal {
    /// The name is not a repository name.
    RepoName,
    /// Another member has the name.
    NameTaken {
        /// The name asked for.
        name: String,
    },
    /// Another member has the same path.
    PathTaken {
        /// The member that has it.
        name: String,
    },
    /// The path is inside a member or holds one.
    Nested {
        /// The member it is nested with.
        name: String,
    },
    /// The path is inside the project folder.
    InsideProjectFolder,
    /// The path holds the project folder.
    HoldsProjectFolder,
    /// A seventeenth member.
    TooMany,
}

impl MemberRefusal {
    /// The refusal's code on the wire.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::RepoName => "repo_name",
            Self::NameTaken { .. } => "name_taken",
            Self::PathTaken { .. } => "path_taken",
            Self::Nested { .. } => "nested",
            Self::InsideProjectFolder => "inside_project_folder",
            Self::HoldsProjectFolder => "holds_project_folder",
            Self::TooMany => "too_many_repos",
        }
    }

    /// The one sentence that says it, lower case with no full stop.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::RepoName => "a repository's name is lowercase letters and digits in words \
                joined by single -, starting with a letter, up to 32 characters"
                .to_string(),
            Self::NameTaken { name } => {
                format!("another of the project's repositories is named {name}")
            }
            Self::PathTaken { name } => {
                format!("that repository is already in the project, as {name}")
            }
            Self::Nested { name } => {
                format!("that folder and {name}, already in the project, are one inside the other")
            }
            Self::InsideProjectFolder => {
                "that folder is inside the project's own folder".to_string()
            }
            Self::HoldsProjectFolder => "that folder holds the project's own folder".to_string(),
            Self::TooMany => format!("a project holds at most {MAX_REPOS} repositories"),
        }
    }

    /// The JSON pointer into the request the refusal is about.
    #[must_use]
    pub fn pointer(&self) -> &'static str {
        match self {
            Self::RepoName | Self::NameTaken { .. } | Self::TooMany => "/name",
            Self::PathTaken { .. }
            | Self::Nested { .. }
            | Self::InsideProjectFolder
            | Self::HoldsProjectFolder => "/path",
        }
    }
}

/// Checks a new member against the project folder and the other members, on canonical paths.
///
/// # Errors
///
/// The first rule the member breaks, in this order: the name's shape, a name another member has, a
/// path another member has, a path inside a member or holding one, a path inside the project
/// folder, a path holding it, and a seventeenth member.
pub fn check_member(
    project_folder: &str,
    others: &[Repo],
    name: &str,
    path: &str,
) -> Result<(), MemberRefusal> {
    if name.parse::<RepoName>().is_err() {
        return Err(MemberRefusal::RepoName);
    }
    if others.iter().any(|other| other.name.as_str() == name) {
        return Err(MemberRefusal::NameTaken {
            name: name.to_string(),
        });
    }
    if let Some(other) = others
        .iter()
        .find(|other| segments(other.path.as_str()) == segments(path))
    {
        return Err(MemberRefusal::PathTaken {
            name: other.name.as_str().to_string(),
        });
    }
    if let Some(other) = others
        .iter()
        .find(|other| is_within(path, other.path.as_str()) || is_within(other.path.as_str(), path))
    {
        return Err(MemberRefusal::Nested {
            name: other.name.as_str().to_string(),
        });
    }
    if is_within(path, project_folder) {
        return Err(MemberRefusal::InsideProjectFolder);
    }
    if is_within(project_folder, path) {
        return Err(MemberRefusal::HoldsProjectFolder);
    }
    if others.len() >= MAX_REPOS {
        return Err(MemberRefusal::TooMany);
    }
    Ok(())
}

/// A path's segments: the first is empty for a path that starts at the root, and the rest skip the
/// empty ones a trailing or doubled separator leaves.
fn segments(path: &str) -> Vec<&str> {
    let mut parts = path.split(['/', '\\']);
    let first = parts.next().into_iter();
    first.chain(parts.filter(|part| !part.is_empty())).collect()
}

/// Whether `path` is `folder` or below it, segment by segment, with `/` or `\` between segments.
/// Callers pass canonical paths: nothing here follows a link or resolves a `..`.
#[must_use]
pub fn is_within(path: &str, folder: &str) -> bool {
    let (path, folder) = (segments(path), segments(folder));
    path.len() >= folder.len() && path[..folder.len()] == folder[..]
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::two_repos_wire;
    use super::{MemberRefusal, Repos, check_member, is_within, validate_repos};

    fn refusals(wire: &Value) -> Vec<(String, String)> {
        validate_repos(wire)
            .expect_err("this wire list is refused")
            .into_iter()
            .map(|error| (error.path, error.message))
            .collect()
    }

    fn paths(wire: &Value) -> Vec<String> {
        refusals(wire).into_iter().map(|(path, _)| path).collect()
    }

    fn entry(name: &str, path: &str) -> Value {
        json!({ "name": name, "path": path })
    }

    fn with_entries(entries: &[Value]) -> Value {
        json!({ "repos": entries })
    }

    fn two() -> Repos {
        validate_repos(&two_repos_wire()).expect("the fixture is a list of repositories")
    }

    #[test]
    fn reads_a_projects_repositories() {
        let wire = two_repos_wire();
        let repos = validate_repos(&wire).expect("the fixture reads");
        assert_eq!(serde_json::to_value(&repos).expect("it serialises"), wire);

        assert!(validate_repos(&with_entries(&[entry("app", "/home/me/app")])).is_ok());

        let seventeen: Vec<Value> = (0..17)
            .map(|n| entry(&format!("r{n}"), &format!("/home/me/r{n}")))
            .collect();
        assert_eq!(paths(&with_entries(&seventeen)), ["/repos"]);
        assert_eq!(paths(&with_entries(&[])), ["/repos"]);

        for bad in ["App", "1app", "app-", "a--b", &"a".repeat(33)] {
            let wire = with_entries(&[entry(bad, "/home/me/app")]);
            assert_eq!(paths(&wire), ["/repos/0/name"], "{bad}");
        }
        assert_eq!(
            paths(&with_entries(&[entry("app", "app")])),
            ["/repos/0/path"]
        );
        assert!(validate_repos(&with_entries(&[entry("app", "C:\\work\\app")])).is_ok());

        let mut unknown = two_repos_wire();
        unknown["repos"][0]["mascot"] = json!("a penguin");
        assert_eq!(paths(&unknown), ["/repos/0"]);

        let mut empty_branch = two_repos_wire();
        empty_branch["repos"][0]["integration_branch"] = json!("");
        assert!(
            paths(&empty_branch)
                .iter()
                .all(|path| path == "/repos/0/integration_branch")
        );
        assert!(!paths(&empty_branch).is_empty());
    }

    #[test]
    fn refuses_a_name_or_a_path_twice() {
        let same_name =
            with_entries(&[entry("app", "/home/me/app"), entry("app", "/home/me/infra")]);
        assert_eq!(
            refusals(&same_name),
            [(
                "/repos/1/name".to_string(),
                "a repository's name names one repository, and app more than one".to_string()
            )]
        );

        let same_path =
            with_entries(&[entry("app", "/home/me/app"), entry("infra", "/home/me/app")]);
        assert_eq!(
            refusals(&same_path),
            [(
                "/repos/1/path".to_string(),
                "/home/me/app is named twice, as app and infra".to_string()
            )]
        );

        let nested = with_entries(&[
            entry("app", "/home/me/app"),
            entry("infra", "/home/me/app/web"),
        ]);
        let found = refusals(&nested);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "/repos/1/path");
        assert!(found[0].1.contains("/home/me/app/web"), "{}", found[0].1);
        assert!(found[0].1.contains("/home/me/app,"), "{}", found[0].1);
        assert_eq!(
            found[0].1,
            "/home/me/app/web is inside /home/me/app, another of the project's repositories"
        );

        let holding = with_entries(&[
            entry("app", "/home/me/app/web"),
            entry("infra", "/home/me/app"),
        ]);
        assert_eq!(
            refusals(&holding),
            [(
                "/repos/1/path".to_string(),
                "/home/me/app/web is inside /home/me/app, another of the project's repositories"
                    .to_string()
            )]
        );

        let near = with_entries(&[
            entry("app", "/home/me/app"),
            entry("apples", "/home/me/apples"),
        ]);
        assert!(validate_repos(&near).is_ok());
    }

    #[test]
    fn checks_a_new_member() {
        let others = two().repos;
        let check = |name: &str, path: &str| check_member("/home/me/shop", &others, name, path);

        assert_eq!(check("web", "/home/me/web"), Ok(()));
        assert_eq!(check("App", "/home/me/web"), Err(MemberRefusal::RepoName));
        assert_eq!(
            check("app", "/home/me/web"),
            Err(MemberRefusal::NameTaken {
                name: "app".to_string()
            })
        );
        let app = || "app".to_string();
        assert_eq!(
            check("web", "/home/me/app"),
            Err(MemberRefusal::PathTaken { name: app() })
        );
        assert_eq!(
            check("web", "/home/me/app/web"),
            Err(MemberRefusal::Nested { name: app() })
        );
        assert_eq!(
            check("web", "/home/me"),
            Err(MemberRefusal::Nested { name: app() })
        );
        assert_eq!(
            check("web", "/home/me/shop/web"),
            Err(MemberRefusal::InsideProjectFolder)
        );
        assert_eq!(
            check("web", "/home"),
            Err(MemberRefusal::Nested { name: app() })
        );
        assert_eq!(
            check_member("/home/me/shop", &[], "web", "/home/me"),
            Err(MemberRefusal::HoldsProjectFolder)
        );

        let sixteen: Vec<_> = (0..16)
            .map(|n| entry(&format!("r{n}"), &format!("/home/me/r{n}")))
            .collect();
        let full = validate_repos(&with_entries(&sixteen))
            .expect("sixteen read")
            .repos;
        assert_eq!(
            check_member("/home/me/shop", &full, "web", "/home/me/web"),
            Err(MemberRefusal::TooMany)
        );

        let rows = [
            (
                MemberRefusal::RepoName,
                "repo_name",
                "/name",
                "a repository's name is lowercase letters and digits in words joined by single -, starting with a letter, up to 32 characters",
            ),
            (
                MemberRefusal::NameTaken {
                    name: "app".to_string(),
                },
                "name_taken",
                "/name",
                "another of the project's repositories is named app",
            ),
            (
                MemberRefusal::PathTaken { name: app() },
                "path_taken",
                "/path",
                "that repository is already in the project, as app",
            ),
            (
                MemberRefusal::Nested { name: app() },
                "nested",
                "/path",
                "that folder and app, already in the project, are one inside the other",
            ),
            (
                MemberRefusal::InsideProjectFolder,
                "inside_project_folder",
                "/path",
                "that folder is inside the project's own folder",
            ),
            (
                MemberRefusal::HoldsProjectFolder,
                "holds_project_folder",
                "/path",
                "that folder holds the project's own folder",
            ),
            (
                MemberRefusal::TooMany,
                "too_many_repos",
                "/name",
                "a project holds at most 16 repositories",
            ),
        ];
        for (refusal, code, pointer, message) in rows {
            assert_eq!(refusal.code(), code);
            assert_eq!(refusal.pointer(), pointer, "{code}");
            assert_eq!(refusal.message(), message, "{code}");
        }
    }

    #[test]
    fn is_within_by_segment() {
        assert!(is_within("/a/b", "/a"));
        assert!(is_within("/a", "/a"));
        assert!(is_within("/a/", "/a"));
        assert!(!is_within("/ab", "/a"));
        assert!(is_within("C:\\a\\b", "C:\\a"));
    }
}
