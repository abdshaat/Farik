//! The project scan (`docs/SPEC.md` section 4's onboarding, 5.8, 5.13, F2 and F16).
//!
//! A person points Farik at a git repository and is told what it thinks it is looking at: one line,
//! `TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago`. The same
//! reading gives the criterion library its first entries, so that a contract in this project is
//! verified by the project's own commands rather than by something a Product Manager invented.
//!
//! Every signal is a file the repository tracks or a line in one of its manifests. Nothing is
//! guessed from a name, nothing is run, and the answer for one tree is the same every time.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use chrono::{DateTime, Utc};
use farik_core::criteria::{
    CriteriaLibrary, CriterionSource, CriterionTemplate, validate_criteria,
};
use serde_json::{Value, json};

use crate::git::{Git, GitError, HeadSummary};

/// What the scan read back about a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectScan {
    /// The one line a person is shown, and what `project.md` holds.
    pub read_back: String,
    /// The criteria the project's own commands make available, each with `source: project_scan`.
    pub detected_criteria: Vec<CriterionTemplate>,
}

/// Why a project could not be scanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// There is no git repository there. A project is a git repository plus `.farik/` (spec 3), and
    /// the scan reads the tree through git, so this is the first thing onboarding checks.
    NotARepository {
        /// The directory that was asked about.
        path: String,
    },
    /// There is a repository, and this is a directory inside it rather than its root. Onboarding
    /// asks a person to pick the project's folder (spec 4); scanning a subtree would answer
    /// confidently about a project it had only seen part of.
    NotTheRepositoryRoot {
        /// The directory that was asked about.
        path: String,
        /// Where the repository actually begins.
        root: String,
    },
    /// Git refused.
    Git {
        /// What it said.
        detail: String,
    },
    /// A file the scan wanted to read could not be read. A file git tracks that is not there is a
    /// tree half-written, not a project.
    Io {
        /// The path, relative to the project root.
        path: String,
        /// What the operating system said.
        detail: String,
    },
    /// The scan built a criterion the library's own rules refuse. That is a defect in the tables in
    /// this module rather than anything about the project, and it is reported rather than unwrapped.
    Built {
        /// Which rule, in `validate_criteria`'s words.
        detail: String,
    },
}

impl fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARepository { path } => {
                write!(formatter, "{path} is not a git repository")
            }
            Self::NotTheRepositoryRoot { path, root } => write!(
                formatter,
                "{path} is inside the repository at {root} rather than its root, and a project is a \
                 whole repository"
            ),
            Self::Git { detail } => write!(formatter, "git refused: {detail}"),
            Self::Io { path, detail } => write!(formatter, "{path} could not be read: {detail}"),
            Self::Built { detail } => write!(
                formatter,
                "the scan built a criterion that is not one: {detail}"
            ),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<GitError> for ScanError {
    fn from(error: GitError) -> Self {
        Self::Git {
            detail: error.to_string(),
        }
    }
}

/// Reads a repository and says what it is.
///
/// The root comes from the adapter rather than beside it, so the tree the paths are listed from is
/// the tree the manifests are read from. `now` is the clock, because the read-back says how long ago
/// the last commit was and a function that sampled the clock itself could not be held to an answer.
///
/// # Errors
///
/// `NotARepository` when there is no repository there, `Git` when git refuses, `Io` when a file git
/// tracks cannot be read, `Built` when the tables in this module describe a criterion the library
/// would refuse.
pub fn scan_project(git: &Git, now: DateTime<Utc>) -> Result<ProjectScan, ScanError> {
    if !git.is_repository() {
        return Err(ScanError::NotARepository {
            path: git.root().display().to_string(),
        });
    }
    let top = git.top_level()?;
    if !same_directory(git.root(), Path::new(&top)) {
        return Err(ScanError::NotTheRepositoryRoot {
            path: git.root().display().to_string(),
            root: top,
        });
    }
    let tracked = git.tracked_paths()?;
    let commit = git.head_summary()?;
    let reading = Reading::of(git.root(), &tracked)?;
    Ok(ProjectScan {
        read_back: reading.read_back(&tracked, commit.as_ref(), now),
        detected_criteria: reading.criteria()?,
    })
}

/// The criterion library a project has after a scan.
///
/// What the scan found replaces what a previous scan found, and never touches what a person wrote:
/// `criteria.schema.json` says so of its `source` field, and a person who edited a criterion by hand
/// should not have it taken away by a refresh. A name a person has used is a name the scan leaves
/// alone, for the same reason.
///
/// The result may hold more criteria than the library's schema allows, and `write_criteria` is what
/// refuses it — one rule in one place, named against the file it is about.
#[must_use]
pub fn seeded_library(
    found: &[CriterionTemplate],
    existing: Option<&CriteriaLibrary>,
) -> CriteriaLibrary {
    let kept: Vec<CriterionTemplate> = existing
        .map(|library| {
            library
                .criteria
                .iter()
                .filter(|criterion| criterion.source != Some(CriterionSource::ProjectScan))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let theirs: Vec<&str> = kept.iter().map(|one| one.name.as_str()).collect();
    let mut criteria = kept.clone();
    criteria.extend(
        found
            .iter()
            .filter(|one| !theirs.contains(&one.name.as_str()))
            .cloned(),
    );
    CriteriaLibrary { criteria }
}

/// The extension a file carries and the language it counts for. The language of a project is the one
/// with the most tracked files; a tie goes to whichever comes first here.
const LANGUAGES: &[(&str, &str)] = &[
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("rs", "Rust"),
    ("py", "Python"),
    ("go", "Go"),
    ("js", "JavaScript"),
    ("jsx", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("swift", "Swift"),
    ("rb", "Ruby"),
    ("php", "PHP"),
    ("cs", "C#"),
];

/// One criterion a toolchain always has.
struct Verified {
    /// What a contract refers to it by. Kebab-case, because the schema's pattern says so.
    name: &'static str,
    /// What it says, in the words it carries into every contract that uses it.
    text: &'static str,
    /// What is run, from the project root.
    command: &'static str,
    /// Whether it is a test command, which the reviewer may be asked to check adds a test.
    is_test: bool,
}

/// Where a toolchain's commands come from.
enum Commands {
    /// Whatever this project's own `package.json` calls its scripts, run through this manager. The
    /// project's commands are the project's to name; the scan reads them rather than guessing.
    Scripts,
    /// The commands this toolchain always has.
    Fixed(&'static [Verified]),
}

/// A toolchain: the file that says a project uses it, what a person calls it, the language whose
/// project it is, and where its commands come from.
///
/// One toolchain is chosen, not all of them, because a criterion called `the-tests-pass` can only
/// mean one command. A project with two markers — a Rust workspace with a front end, which is the
/// shape this repository itself takes — gets the one whose language the tree says it mostly is, and
/// falls back to the first marker found when the language names none of them.
struct Toolchain {
    marker: &'static str,
    name: &'static str,
    language: &'static str,
    commands: Commands,
}

const CARGO: &[Verified] = &[
    Verified {
        name: "the-tests-pass",
        text: "Every test in the workspace passes: cargo test --workspace.",
        command: "cargo test --workspace",
        is_test: true,
    },
    Verified {
        name: "the-build-succeeds",
        text: "The workspace builds: cargo build --workspace.",
        command: "cargo build --workspace",
        is_test: false,
    },
    Verified {
        name: "clippy-is-clean",
        text: "Clippy has nothing to say about the workspace, warnings included.",
        command: "cargo clippy --workspace --all-targets -- -D warnings",
        is_test: false,
    },
    Verified {
        name: "formatting-is-clean",
        text: "Every file is formatted as rustfmt would format it.",
        command: "cargo fmt --all -- --check",
        is_test: false,
    },
];

const GO: &[Verified] = &[
    Verified {
        name: "the-tests-pass",
        text: "Every test in the module passes: go test ./...",
        command: "go test ./...",
        is_test: true,
    },
    Verified {
        name: "the-build-succeeds",
        text: "The module builds: go build ./...",
        command: "go build ./...",
        is_test: false,
    },
    Verified {
        name: "vet-is-clean",
        text: "go vet has nothing to say about the module.",
        command: "go vet ./...",
        is_test: false,
    },
];

const PYTEST: &[Verified] = &[Verified {
    name: "the-tests-pass",
    text: "Every test passes: pytest.",
    command: "pytest",
    is_test: true,
}];

const RSPEC: &[Verified] = &[Verified {
    name: "the-tests-pass",
    text: "Every specification passes: bundle exec rspec.",
    command: "bundle exec rspec",
    is_test: true,
}];

const TOOLCHAINS: &[Toolchain] = &[
    Toolchain {
        marker: "pnpm-lock.yaml",
        name: "pnpm",
        language: "TypeScript",
        commands: Commands::Scripts,
    },
    Toolchain {
        marker: "bun.lockb",
        name: "bun",
        language: "TypeScript",
        commands: Commands::Scripts,
    },
    Toolchain {
        marker: "yarn.lock",
        name: "yarn",
        language: "TypeScript",
        commands: Commands::Scripts,
    },
    Toolchain {
        marker: "package-lock.json",
        name: "npm",
        language: "TypeScript",
        commands: Commands::Scripts,
    },
    Toolchain {
        marker: "Cargo.lock",
        name: "cargo",
        language: "Rust",
        commands: Commands::Fixed(CARGO),
    },
    Toolchain {
        marker: "go.sum",
        name: "go",
        language: "Go",
        commands: Commands::Fixed(GO),
    },
    Toolchain {
        marker: "poetry.lock",
        name: "poetry",
        language: "Python",
        commands: Commands::Fixed(PYTEST),
    },
    Toolchain {
        marker: "uv.lock",
        name: "uv",
        language: "Python",
        commands: Commands::Fixed(PYTEST),
    },
    Toolchain {
        marker: "Gemfile.lock",
        name: "bundler",
        language: "Ruby",
        commands: Commands::Fixed(RSPEC),
    },
];

/// The script names a criterion is made from, and what each one is called in the library. A project
/// may call a script anything; these are the ones whose meaning is the same everywhere, and keeping
/// the list closed is also what keeps a criterion's name to the schema's pattern.
const SCRIPTS: &[(&str, &str, &str, bool)] = &[
    (
        "test",
        "the-tests-pass",
        "Every test passes: the project's own test script.",
        true,
    ),
    (
        "build",
        "the-build-succeeds",
        "The project builds: its own build script.",
        false,
    ),
    (
        "lint",
        "the-linter-is-clean",
        "The linter has nothing to say: the project's own lint script.",
        false,
    ),
    (
        "typecheck",
        "the-types-check",
        "The types check: the project's own typecheck script.",
        false,
    ),
    (
        "check",
        "the-project-check-passes",
        "The project's own check script passes.",
        false,
    ),
];

/// What says a project tests with a given runner: a file whose name begins this way, or this exact
/// name among a `package.json` dependency.
const TEST_RUNNERS: &[(&str, &str)] = &[
    ("vitest", "vitest"),
    ("jest", "jest"),
    ("playwright", "Playwright"),
    ("cypress", "Cypress"),
    ("pytest", "pytest"),
    ("mocha", "mocha"),
];

/// The files that say a project is one workspace of several packages.
const WORKSPACE_MARKERS: &[&str] = &[
    "pnpm-workspace.yaml",
    "turbo.json",
    "nx.json",
    "lerna.json",
    "go.work",
];

/// The manifests a package has one of, for counting packages in a workspace.
const MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "composer.json",
];

/// What one tree says about itself, before any of it is put into words.
struct Reading {
    language: Option<&'static str>,
    toolchain: Option<&'static Toolchain>,
    is_workspace: bool,
    packages: usize,
    tests: Option<&'static str>,
    scripts: Vec<String>,
}

impl Reading {
    /// Reads the tracked tree, and the one manifest whose contents matter.
    fn of(root: &Path, tracked: &[String]) -> Result<Self, ScanError> {
        let manifest = read_package_json(root, tracked)?;
        let language = language_of(tracked);
        let present = |toolchain: &&Toolchain| tracked.iter().any(|path| path == toolchain.marker);
        let toolchain = TOOLCHAINS
            .iter()
            .find(|toolchain| present(toolchain) && Some(toolchain.language) == language)
            .or_else(|| TOOLCHAINS.iter().find(present));
        let is_workspace = tracked
            .iter()
            .any(|path| WORKSPACE_MARKERS.contains(&path.as_str()))
            || manifest
                .as_ref()
                .is_some_and(|value| value.get("workspaces").is_some())
            || cargo_workspace(root, tracked)?;
        Ok(Self {
            language,
            toolchain,
            is_workspace,
            packages: packages_in(tracked),
            tests: tests_in(tracked, manifest.as_ref()),
            scripts: scripts_in(manifest.as_ref()),
        })
    }

    /// The one line a person is shown, in the shape section 4 gives.
    fn read_back(
        &self,
        tracked: &[String],
        commit: Option<&HeadSummary>,
        now: DateTime<Utc>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();
        if tracked.is_empty() {
            parts.push("nothing tracked yet".to_string());
        }
        // A workspace of one package is a workspace and not a monorepo, and "1 packages" is not a
        // count. One condition decides the word and the number together, so they cannot disagree.
        let monorepo = self.is_workspace && self.packages > 1;
        if let Some(language) = self.language {
            parts.push(if monorepo {
                format!("{language} monorepo")
            } else {
                language.to_string()
            });
        }
        if let Some(toolchain) = self.toolchain {
            parts.push(toolchain.name.to_string());
        }
        if monorepo {
            parts.push(format!("{} packages", self.packages));
        }
        if let Some(tests) = self.tests {
            parts.push(format!("tests in {tests}"));
        }
        parts.push(match commit {
            Some(head) => format!("last commit {}", how_long_ago(&head.committed_at, now)),
            None => "no commits yet".to_string(),
        });
        parts.join(", ")
    }

    /// The criteria this project's own commands make available.
    fn criteria(&self) -> Result<Vec<CriterionTemplate>, ScanError> {
        let Some(toolchain) = self.toolchain else {
            return Ok(Vec::new());
        };
        let wire: Vec<Value> = match &toolchain.commands {
            Commands::Fixed(fixed) => fixed
                .iter()
                .map(|one| criterion(one.name, one.text, one.command, one.is_test))
                .collect(),
            Commands::Scripts => SCRIPTS
                .iter()
                .filter(|(script, ..)| self.scripts.iter().any(|have| have == script))
                .map(|(script, name, text, is_test)| {
                    criterion(
                        name,
                        text,
                        &format!("{} run {script}", toolchain.name),
                        *is_test,
                    )
                })
                .collect(),
        };
        validate_criteria(&json!({ "criteria": wire }))
            .map(|library| library.criteria)
            .map_err(|errors| ScanError::Built {
                detail: errors
                    .iter()
                    .map(|error| format!("{} {}", error.path, error.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            })
    }
}

/// One criterion as the library's schema sees it, for `validate_criteria` to hold to its rules.
fn criterion(name: &str, text: &str, command: &str, is_test: bool) -> Value {
    let verification = if is_test {
        json!({ "method": "test", "command": command, "new_tests_required": true })
    } else {
        json!({ "method": "command", "command": command, "expect": { "exit_code": 0 } })
    };
    json!({
        "name": name,
        "text": text,
        "source": "project_scan",
        "verification": verification,
    })
}

/// The language with the most tracked files, or nothing when the tree holds no code the table knows.
fn language_of(tracked: &[String]) -> Option<&'static str> {
    let mut counted: BTreeMap<&'static str, usize> = BTreeMap::new();
    for path in tracked {
        let Some(extension) = path.rsplit_once('.').map(|(_, tail)| tail) else {
            continue;
        };
        if let Some((_, language)) = LANGUAGES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(extension))
        {
            *counted.entry(language).or_default() += 1;
        }
    }
    LANGUAGES
        .iter()
        .filter_map(|(_, language)| counted.get(language).map(|count| (*count, *language)))
        // `max_by_key` keeps the last of equal keys, so the table is read backwards: a tie goes to
        // whichever language comes first in it, which is what its own comment promises.
        .rev()
        .max_by_key(|(count, _)| *count)
        .map(|(_, language)| language)
}

/// How many packages a tree holds: one per manifest below the root.
fn packages_in(tracked: &[String]) -> usize {
    tracked
        .iter()
        .filter(|path| {
            path.contains('/')
                && path
                    .rsplit_once('/')
                    .is_some_and(|(_, name)| MANIFESTS.contains(&name))
        })
        .count()
}

/// The test runner a tree names, by a configuration file of its own or by a dependency.
fn tests_in(tracked: &[String], manifest: Option<&Value>) -> Option<&'static str> {
    // The runner's own file, not a file that mentions it: `vitest.config.ts` says the project tests
    // with vitest, and `jest-to-vitest-migration.md` says somebody wrote about it.
    let named = |needle: &str| {
        tracked.iter().any(|path| {
            let name = path
                .rsplit_once('/')
                .map_or(path.as_str(), |(_, name)| name);
            name == needle || name.starts_with(&format!("{needle}."))
        })
    };
    let depended = |needle: &str| {
        ["dependencies", "devDependencies"].iter().any(|section| {
            manifest
                .and_then(|value| value.get(section))
                .and_then(Value::as_object)
                .is_some_and(|section| section.contains_key(needle))
        })
    };
    TEST_RUNNERS
        .iter()
        .find(|(needle, _)| named(needle) || depended(needle))
        .map(|(_, shown)| *shown)
}

/// The scripts a `package.json` defines that actually run something.
///
/// A name with nothing behind it is not a command: `"test": ""` would become a criterion whose
/// command exits 0 having verified nothing, which is worse than no criterion at all. What this
/// cannot see is a stub that runs and fails on purpose, as `npm init` writes for `test`; that is a
/// project telling its own tools something, and the person who adopts Farik has to look at it.
fn scripts_in(manifest: Option<&Value>) -> Vec<String> {
    manifest
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object)
        .map(|scripts| {
            scripts
                .iter()
                .filter(|(_, command)| command.as_str().is_some_and(|text| !text.trim().is_empty()))
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The root `package.json`, when the tree tracks one, as an untrusted value.
fn read_package_json(root: &Path, tracked: &[String]) -> Result<Option<Value>, ScanError> {
    if !tracked.iter().any(|path| path == "package.json") {
        return Ok(None);
    }
    let text = read_tracked(root, "package.json")?;
    // A manifest that is not JSON is a fact about the project rather than a reason to refuse the
    // whole scan: the read-back says what it can and the criteria come out empty.
    Ok(serde_json::from_str(&text).ok())
}

/// Whether the root `Cargo.toml` declares a workspace.
fn cargo_workspace(root: &Path, tracked: &[String]) -> Result<bool, ScanError> {
    if !tracked.iter().any(|path| path == "Cargo.toml") {
        return Ok(false);
    }
    Ok(read_tracked(root, "Cargo.toml")?
        .lines()
        .any(|line| line.trim() == "[workspace]"))
}

/// Whether two paths name one directory, once the links in each are followed.
///
/// A path that cannot be resolved is not the same directory as one that can: the question is asked
/// of a repository git has just answered about, so a failure here is the directory going away
/// underneath, and the safe answer is no.
fn same_directory(one: &Path, other: &Path) -> bool {
    match (std::fs::canonicalize(one), std::fs::canonicalize(other)) {
        (Ok(one), Ok(other)) => one == other,
        _ => false,
    }
}

/// One tracked file's text. A file git tracks that cannot be read is a tree half-written.
fn read_tracked(root: &Path, relative: &str) -> Result<String, ScanError> {
    std::fs::read_to_string(root.join(relative)).map_err(|error| ScanError::Io {
        path: relative.to_string(),
        detail: error.to_string(),
    })
}

/// How long ago a commit was, in the words a person would use.
///
/// Git's `%cI` is strict ISO 8601. A stamp this cannot parse is reported as the stamp itself rather
/// than as a length of time nobody can check.
fn how_long_ago(committed_at: &str, now: DateTime<Utc>) -> String {
    let Ok(committed) = DateTime::parse_from_rfc3339(committed_at) else {
        return format!("at {committed_at}");
    };
    let committed = committed.with_timezone(&Utc);
    // Ahead of now at all, not a whole day ahead: two machines whose clocks disagree are hours
    // apart, and "today" for a commit that has not happened yet is a length of time nobody can
    // check.
    if committed > now {
        return format!("at {committed_at}");
    }
    let days = (now - committed).num_days();
    match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=29 => format!("{days} days ago"),
        30..=59 => "a month ago".to_string(),
        _ => format!("{} months ago", days / 30),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use farik_core::criteria::{CriteriaLibrary, CriterionSource, validate_criteria};
    use serde_json::json;

    use super::{
        Reading, ScanError, TOOLCHAINS, how_long_ago, language_of, packages_in, scripts_in,
        seeded_library, tests_in,
    };

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a stamp")
            .with_timezone(&Utc)
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(ToString::to_string).collect()
    }

    fn toolchain(name: &str) -> &'static super::Toolchain {
        TOOLCHAINS
            .iter()
            .find(|toolchain| toolchain.name == name)
            .expect("a toolchain the table knows")
    }

    #[test]
    fn says_how_long_ago_a_commit_was_in_words_a_person_uses() {
        let now = at("2026-09-17T12:00:00Z");
        assert_eq!(how_long_ago("2026-09-17T09:00:00+00:00", now), "today");
        assert_eq!(how_long_ago("2026-09-16T09:00:00+00:00", now), "yesterday");
        assert_eq!(
            how_long_ago("2026-09-13T12:00:00+00:00", now),
            "4 days ago",
            "which is the read-back section 4 gives as its example"
        );
        assert_eq!(
            how_long_ago("2026-08-19T12:00:00+00:00", now),
            "29 days ago"
        );
        assert_eq!(
            how_long_ago("2026-08-18T12:00:00+00:00", now),
            "a month ago"
        );
        assert_eq!(
            how_long_ago("2026-03-17T12:00:00+00:00", now),
            "6 months ago"
        );
    }

    #[test]
    fn says_the_stamp_itself_when_it_is_not_one_it_can_use() {
        // A length of time nobody can check is worse than the stamp. A commit in the future is the
        // ordinary result of two machines whose clocks disagree.
        let now = at("2026-09-17T12:00:00Z");
        assert_eq!(how_long_ago("yesterday-ish", now), "at yesterday-ish");
        assert_eq!(
            how_long_ago("2026-09-17T13:00:00+00:00", now),
            "at 2026-09-17T13:00:00+00:00",
            "an hour ahead is ahead: two machines whose clocks disagree are hours apart, not days"
        );
        assert_eq!(
            how_long_ago("2026-09-18T12:00:00+00:00", now),
            "at 2026-09-18T12:00:00+00:00"
        );
    }

    #[test]
    fn names_the_language_with_the_most_files() {
        assert_eq!(
            language_of(&strings(&["src/a.ts", "src/b.tsx", "build.rs"])),
            Some("TypeScript")
        );
        assert_eq!(
            language_of(&strings(&["src/lib.rs", "src/main.rs", "web/app.ts"])),
            Some("Rust")
        );
        assert_eq!(language_of(&strings(&["README.md", "LICENSE"])), None);
        assert_eq!(
            language_of(&strings(&["one.ts", "two.rs"])),
            Some("TypeScript"),
            "a tie goes to whichever language comes first in the table"
        );
        assert_eq!(
            language_of(&strings(&["one.rs", "two.py"])),
            Some("Rust"),
            "and the table's order is the order a person would name them in"
        );
        assert_eq!(
            language_of(&strings(&["src/A.RS"])),
            Some("Rust"),
            "an extension is an extension whatever case a person typed it in"
        );
    }

    #[test]
    fn counts_one_package_per_manifest_below_the_root() {
        assert_eq!(
            packages_in(&strings(&[
                "package.json",
                "packages/ui/package.json",
                "packages/web/package.json",
                "apps/desktop/package.json",
            ])),
            3,
            "the root's own manifest is the workspace, not a package in it"
        );
        assert_eq!(packages_in(&strings(&["package.json"])), 0);
    }

    #[test]
    fn names_the_test_runner_a_project_configures_or_depends_on() {
        assert_eq!(
            tests_in(&strings(&["vitest.config.ts"]), None),
            Some("vitest")
        );
        assert_eq!(tests_in(&strings(&["tests/test_it.py"]), None), None);
        assert_eq!(
            tests_in(&strings(&["packages/ui/vitest.config.ts"]), None),
            Some("vitest"),
            "a package's own configuration counts wherever it sits"
        );
        assert_eq!(
            tests_in(&strings(&["docs/jest-to-vitest-migration.md"]), None),
            None,
            "and a file that writes about a runner is not a project that uses one"
        );
        let manifest = json!({ "devDependencies": { "jest": "^30.0.0" } });
        assert_eq!(tests_in(&[], Some(&manifest)), Some("jest"));
        assert_eq!(tests_in(&[], None), None);
    }

    #[test]
    fn reads_the_script_names_a_manifest_defines() {
        let manifest = json!({ "scripts": { "test": "vitest run", "build": "tsc" } });
        let mut found = scripts_in(Some(&manifest));
        found.sort();
        assert_eq!(found, ["build", "test"]);
        assert!(scripts_in(None).is_empty());

        // A name with nothing behind it would become a criterion whose command verifies nothing.
        let empty = json!({ "scripts": { "test": "", "lint": "   ", "build": 7, "check": "tsc" } });
        assert_eq!(scripts_in(Some(&empty)), ["check"]);
    }

    #[test]
    fn reads_back_the_line_section_4_asks_for() {
        let reading = Reading {
            language: Some("TypeScript"),
            toolchain: Some(toolchain("pnpm")),
            is_workspace: true,
            packages: 3,
            tests: Some("vitest"),
            scripts: Vec::new(),
        };
        let head = crate::git::HeadSummary {
            sha: "a1b2c3".to_string(),
            committed_at: "2026-09-13T12:00:00+00:00".to_string(),
            subject: "a commit".to_string(),
        };
        assert_eq!(
            reading.read_back(
                &strings(&["package.json"]),
                Some(&head),
                at("2026-09-17T12:00:00Z")
            ),
            "TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit 4 days ago"
        );
    }

    #[test]
    fn a_workspace_of_one_package_is_not_a_monorepo() {
        let reading = Reading {
            language: Some("TypeScript"),
            toolchain: Some(toolchain("pnpm")),
            is_workspace: true,
            packages: 1,
            tests: None,
            scripts: Vec::new(),
        };
        assert_eq!(
            reading.read_back(
                &strings(&["package.json"]),
                None,
                at("2026-09-17T12:00:00Z")
            ),
            "TypeScript, pnpm, no commits yet",
            "one package is a workspace and not a monorepo, and `1 packages` is not a count"
        );
    }

    #[test]
    fn reads_back_what_it_can_of_a_project_it_recognises_nothing_in() {
        let reading = Reading {
            language: None,
            toolchain: None,
            is_workspace: false,
            packages: 0,
            tests: None,
            scripts: Vec::new(),
        };
        assert_eq!(
            reading.read_back(&[], None, at("2026-09-17T12:00:00Z")),
            "nothing tracked yet, no commits yet"
        );
    }

    #[test]
    fn builds_a_criterion_the_library_accepts_for_every_toolchain() {
        // The tables in this module describe criteria, and the library's own rules are what say
        // whether they are ones. A name that is not kebab-case or a text under ten characters would
        // be a defect nothing else here would catch.
        for toolchain in TOOLCHAINS {
            let reading = Reading {
                language: None,
                toolchain: Some(toolchain),
                is_workspace: false,
                packages: 0,
                tests: None,
                scripts: super::SCRIPTS
                    .iter()
                    .map(|(script, ..)| (*script).to_string())
                    .collect(),
            };
            let criteria = reading
                .criteria()
                .unwrap_or_else(|error| panic!("{}: {error}", toolchain.name));
            assert!(
                !criteria.is_empty(),
                "{} offers a project nothing",
                toolchain.name
            );
            assert!(
                criteria
                    .iter()
                    .all(|one| one.source == Some(CriterionSource::ProjectScan)),
                "{}: the scan found these, and a refresh may replace them",
                toolchain.name
            );
        }
    }

    #[test]
    fn a_project_whose_toolchain_it_does_not_know_gets_no_criteria() {
        let reading = Reading {
            language: Some("Swift"),
            toolchain: None,
            is_workspace: false,
            packages: 0,
            tests: None,
            scripts: Vec::new(),
        };
        assert_eq!(
            reading.criteria().expect("no criteria is not a refusal"),
            []
        );
    }

    #[test]
    fn only_the_scripts_a_project_has_become_criteria() {
        let reading = Reading {
            language: Some("TypeScript"),
            toolchain: Some(toolchain("pnpm")),
            is_workspace: false,
            packages: 0,
            tests: None,
            scripts: strings(&["test", "lint", "release"]),
        };
        let criteria = reading.criteria().expect("two of them");
        assert_eq!(
            criteria
                .iter()
                .map(|one| one.name.as_str())
                .collect::<Vec<_>>(),
            ["the-tests-pass", "the-linter-is-clean"],
            "a script the table does not know is not a criterion, and one it knows that the project \
             does not have is not either"
        );
    }

    #[test]
    fn a_refresh_replaces_what_the_scan_found_and_keeps_what_a_person_wrote() {
        let wire = json!({ "criteria": [
            { "name": "the-tests-pass", "text": "An older scan found this one.",
              "source": "project_scan",
              "verification": { "method": "test", "command": "npm test" } },
            { "name": "the-docs-are-updated", "text": "A person wrote this one by hand.",
              "source": "human",
              "verification": { "method": "review", "rubric": ["Are the docs updated?"] } },
            { "name": "it-looks-right", "text": "And this one, without saying so.",
              "verification": { "method": "human", "question": "Does it look right?" } },
        ]});
        let existing = validate_criteria(&wire).expect("a library");
        let found = validate_criteria(&json!({ "criteria": [
            { "name": "the-tests-pass", "text": "What this scan found instead.",
              "source": "project_scan",
              "verification": { "method": "test", "command": "pnpm run test" } },
            { "name": "clippy-is-clean", "text": "And one it did not find before.",
              "source": "project_scan",
              "verification": { "method": "command", "command": "cargo clippy",
                                "expect": { "exit_code": 0 } } },
        ]}))
        .expect("a library")
        .criteria;

        let seeded = seeded_library(&found, Some(&existing));
        assert_eq!(
            seeded
                .criteria
                .iter()
                .map(|one| one.name.as_str())
                .collect::<Vec<_>>(),
            [
                "the-docs-are-updated",
                "it-looks-right",
                "the-tests-pass",
                "clippy-is-clean"
            ],
            "what a person wrote comes first and is kept; what a scan found is replaced"
        );
        assert_eq!(
            seeded.criteria[2].text.as_str(),
            "What this scan found instead."
        );
    }

    #[test]
    fn a_name_a_person_has_used_is_a_name_the_scan_leaves_alone() {
        let existing = validate_criteria(&json!({ "criteria": [
            { "name": "the-tests-pass", "text": "A person means something else by this.",
              "source": "human",
              "verification": { "method": "review", "rubric": ["Do the tests pass?"] } },
        ]}))
        .expect("a library");
        let found = validate_criteria(&json!({ "criteria": [
            { "name": "the-tests-pass", "text": "What the scan would have called it.",
              "source": "project_scan",
              "verification": { "method": "test", "command": "cargo test" } },
        ]}))
        .expect("a library")
        .criteria;

        let seeded = seeded_library(&found, Some(&existing));
        assert_eq!(seeded.criteria.len(), 1);
        assert_eq!(
            seeded.criteria[0].text.as_str(),
            "A person means something else by this.",
            "a refresh never touches what a person wrote, name included"
        );
    }

    #[test]
    fn a_project_with_no_library_yet_gets_what_the_scan_found() {
        let found = validate_criteria(&json!({ "criteria": [
            { "name": "the-tests-pass", "text": "Every test in the workspace passes.",
              "source": "project_scan",
              "verification": { "method": "test", "command": "cargo test --workspace" } },
        ]}))
        .expect("a library")
        .criteria;
        assert_eq!(
            seeded_library(&found, None),
            CriteriaLibrary {
                criteria: found.clone()
            }
        );
        assert_eq!(
            seeded_library(&[], None),
            CriteriaLibrary {
                criteria: Vec::new()
            },
            "a project whose scan found nothing still has a library"
        );
    }

    #[test]
    fn says_what_it_could_not_read_and_why() {
        assert_eq!(
            [
                ScanError::NotARepository {
                    path: "/home/ada/notes".to_string()
                }
                .to_string(),
                ScanError::Git {
                    detail: "git is not installed".to_string()
                }
                .to_string(),
                ScanError::Io {
                    path: "package.json".to_string(),
                    detail: "Permission denied (os error 13)".to_string()
                }
                .to_string(),
                ScanError::Built {
                    detail: "/criteria/0/name does not match".to_string()
                }
                .to_string(),
                ScanError::NotTheRepositoryRoot {
                    path: "/home/ada/project/crates/core".to_string(),
                    root: "/home/ada/project".to_string()
                }
                .to_string(),
            ],
            [
                "/home/ada/notes is not a git repository",
                "git refused: git is not installed",
                "package.json could not be read: Permission denied (os error 13)",
                "the scan built a criterion that is not one: /criteria/0/name does not match",
                "/home/ada/project/crates/core is inside the repository at /home/ada/project rather \
                 than its root, and a project is a whole repository",
            ]
        );
    }
}
