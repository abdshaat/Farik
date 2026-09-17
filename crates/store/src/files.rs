//! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
//!
//! The event log is the source of truth for what happened; these files are the source of truth for
//! what the team knows. They are what travels with the repository, so every one of them is text a
//! person can read, diff and edit — and every structured one is held, when it is read back, to
//! exactly the rules it would be held to arriving on the wire.

use std::fmt;
use std::path::{Path, PathBuf};

use farik_core::contract::ValidationError;
use farik_core::team::{Team, validate_team};
use serde::Serialize;
use serde_json::Value;

/// Wire fixtures and a temporary project, for tests in this crate and in others.
pub mod fixtures;

/// Why a file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesError {
    /// There is no file there. A caller that can go on without one says so by asking for an
    /// `Option` instead; everything else treats this as the answer.
    NotFound {
        /// The path, relative to the project root.
        path: String,
    },
    /// The file is there and is not what it should be: it is not the format, or it is the format
    /// and breaks a rule the format has.
    Invalid {
        /// The path, relative to the project root.
        path: String,
        /// What was wrong, in the words of whatever refused it.
        detail: String,
    },
    /// The operating system refused.
    Io {
        /// The path, relative to the project root.
        path: String,
        /// What it said.
        detail: String,
    },
}

impl fmt::Display for FilesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(formatter, "there is no {path}"),
            Self::Invalid { path, detail } => write!(formatter, "{path} is not usable: {detail}"),
            Self::Io { path, detail } => write!(formatter, "{path} could not be used: {detail}"),
        }
    }
}

impl std::error::Error for FilesError {}

/// The `.farik/` directory of one project, at the root of its git repository.
pub struct ProjectFiles {
    root: PathBuf,
}

/// What `.farik/local/.gitignore` holds: everything under it, including itself.
///
/// A task's worktree lives at `.farik/local/worktrees/FRK-<n>` (5.14) and the event log at
/// `.farik/local/farik.db` (8.4). Without this, every repository Farik touches is dirty for good
/// and `Git::is_clean` on the root never answers true again.
const LOCAL_GITIGNORE: &str = "*\n";

/// Where that file lives, relative to `.farik/`.
const LOCAL_GITIGNORE_PATH: &str = "local/.gitignore";

impl ProjectFiles {
    /// The `.farik/` directory of the repository at `root`. Nothing is read and nothing is written
    /// until a method is called.
    #[must_use]
    pub fn open(root: PathBuf) -> Self {
        Self { root }
    }

    /// The project root, which is the git repository's root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Makes `.farik/` and everything under it that a project starts with, and writes the team.
    ///
    /// Nothing that is already there is overwritten: a second `farik init` on a project that has a
    /// team is a command that has nothing to do, not one that throws the team away. The directories
    /// are made whether or not they hold anything yet, so that a person opening `.farik/` sees where
    /// things go.
    ///
    /// # Errors
    ///
    /// `Io` when a directory or a file cannot be made, `Invalid` when the team is not one
    /// `validate_team` accepts — which it must be, since it is what a later read will be held to.
    pub fn init(&self, team: &Team) -> Result<(), FilesError> {
        for directory in [
            "",
            "team",
            "contracts",
            "agents",
            "decisions",
            "product",
            "local",
        ] {
            self.make_directory(&self.farik().join(directory))?;
        }
        self.write_if_absent(LOCAL_GITIGNORE_PATH, LOCAL_GITIGNORE)?;
        if self.path_of(TEAM).exists() {
            return Ok(());
        }
        self.write_team(team)
    }

    /// The team, held to the rules a team on the wire is held to.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is no team file, `Invalid` when it is not YAML or not a team,
    /// `Io` otherwise.
    pub fn read_team(&self) -> Result<Team, FilesError> {
        let value = self.read_yaml(TEAM)?;
        validate_team(&value).map_err(|errors| self.refused(TEAM, &errors))
    }

    /// Writes the team, after holding it to the same rules.
    ///
    /// # Errors
    ///
    /// `Invalid` when the team is not one `validate_team` accepts, `Io` when it cannot be written.
    pub fn write_team(&self, team: &Team) -> Result<(), FilesError> {
        let value = self.as_wire(TEAM, team)?;
        validate_team(&value).map_err(|errors| self.refused(TEAM, &errors))?;
        self.write_yaml(TEAM, &value)
    }
}

/// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
/// the whole layout at once and a change to it is one line.
const TEAM: &str = "team.yaml";

/// How a file a person edits by hand is read.
///
/// `strict_booleans` is the one setting that matters for such a file: YAML 1.1 resolves an unquoted
/// `no`, `y` or `off` to a boolean, so a Norwegian country code or an agent id written that way
/// would reach the validator as `false` rather than as what the person typed. What Farik itself
/// writes is quoted either way.
fn yaml_options() -> serde_saphyr::Options {
    let mut options = serde_saphyr::Options::default();
    options.strict_booleans = true;
    options
}

impl ProjectFiles {
    /// `.farik/`, where everything this module touches lives.
    fn farik(&self) -> PathBuf {
        self.root.join(".farik")
    }

    /// One file's whole path, from its place in the layout.
    fn path_of(&self, relative: &str) -> PathBuf {
        self.farik().join(relative)
    }

    /// The path as a person reads it in a refusal: from the project root, with `/` separators
    /// whatever the machine uses.
    fn named(relative: &str) -> String {
        format!(".farik/{relative}")
    }

    /// Every refusal a validator gave, as one `Invalid`.
    fn refused(&self, relative: &str, errors: &[ValidationError]) -> FilesError {
        let _ = self;
        FilesError::Invalid {
            path: Self::named(relative),
            detail: errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; "),
        }
    }

    fn make_directory(&self, path: &Path) -> Result<(), FilesError> {
        std::fs::create_dir_all(path).map_err(|error| FilesError::Io {
            path: self.beneath_the_root(path),
            detail: error.to_string(),
        })
    }

    /// A whole path as a person reads it in a refusal, with `/` between its parts whatever the
    /// machine puts there.
    fn beneath_the_root(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Writes a file only when there is nothing there, so that `init` never takes anything away.
    /// Through `write_text`, so that this one is written the way every other one is.
    fn write_if_absent(&self, relative: &str, text: &str) -> Result<(), FilesError> {
        if self.path_of(relative).exists() {
            return Ok(());
        }
        self.write_text(relative, text)
    }

    /// One file's text.
    fn read_text(&self, relative: &str) -> Result<String, FilesError> {
        let path = self.path_of(relative);
        match std::fs::read_to_string(&path) {
            // A byte-order mark is what a Windows editor puts at the front of a file it saved. It
            // is not content, and a parser that meets one says the file holds two documents, which
            // is not a thing a person can act on.
            Ok(text) => Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(FilesError::NotFound {
                    path: Self::named(relative),
                })
            }
            Err(error) => Err(FilesError::Io {
                path: Self::named(relative),
                detail: error.to_string(),
            }),
        }
    }

    /// Writes one file, making the directory it lives in, and leaving nothing half-written.
    ///
    /// The text goes to a file beside the one being written and is renamed over it, because a
    /// rename within a directory is the one file operation that is all or nothing. A crash in the
    /// middle of writing `team.yaml` would otherwise leave the team unreadable, and the team is
    /// what every session is assembled from.
    fn write_text(&self, relative: &str, text: &str) -> Result<(), FilesError> {
        let path = self.path_of(relative);
        if let Some(directory) = path.parent() {
            self.make_directory(directory)?;
        }
        let beside = path.with_extension(format!(
            "{}.writing",
            path.extension()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default()
        ));
        std::fs::write(&beside, text).map_err(|error| FilesError::Io {
            path: Self::named(relative),
            detail: error.to_string(),
        })?;
        std::fs::rename(&beside, &path).map_err(|error| FilesError::Io {
            path: Self::named(relative),
            detail: error.to_string(),
        })
    }

    /// One YAML file as an untrusted value, for a validator to hold to its rules.
    ///
    /// Read the way a file a person edits by hand should be. `UserMessageFormatter` is the crate's
    /// own answer to the question, and its own default is explicitly not for a person to read: it
    /// recommends the API call that would have accepted the file. And the name of the input is put
    /// back, because the crate does not know it and calls it `<input>`.
    fn read_yaml(&self, relative: &str) -> Result<Value, FilesError> {
        let text = self.read_text(relative)?;
        serde_saphyr::from_str_with_options(&text, yaml_options()).map_err(|error| {
            FilesError::Invalid {
                path: Self::named(relative),
                detail: error
                    .render_with_formatter(&serde_saphyr::UserMessageFormatter)
                    .replace("<input>", &Self::named(relative)),
            }
        })
    }

    /// A typed value as the wire sees it, which is what a validator reads.
    ///
    /// Every writer takes this step and then holds the result to its own file's rules, so that a
    /// value built in memory cannot become a file that cannot be read back. The round trip is the
    /// promise: what `write_team` accepts, `read_team` returns.
    fn as_wire<T: Serialize>(&self, relative: &str, value: &T) -> Result<Value, FilesError> {
        let _ = self;
        serde_json::to_value(value).map_err(|error| FilesError::Invalid {
            path: Self::named(relative),
            detail: error.to_string(),
        })
    }

    /// Writes a wire value as the YAML a person reads and edits.
    fn write_yaml(&self, relative: &str, value: &Value) -> Result<(), FilesError> {
        let text = serde_saphyr::to_string(value).map_err(|error| FilesError::Invalid {
            path: Self::named(relative),
            detail: error.to_string(),
        })?;
        self.write_text(relative, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::FilesError;

    #[test]
    fn says_what_it_could_not_use_and_why_in_plain_words() {
        let said: Vec<String> = [
            FilesError::NotFound {
                path: ".farik/team.yaml".to_string(),
            },
            FilesError::Invalid {
                path: ".farik/team.yaml".to_string(),
                detail: "/agents an agent id names one agent".to_string(),
            },
            FilesError::Io {
                path: ".farik/team.yaml".to_string(),
                detail: "Permission denied (os error 13)".to_string(),
            },
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
        assert_eq!(
            said,
            [
                "there is no .farik/team.yaml",
                ".farik/team.yaml is not usable: /agents an agent id names one agent",
                ".farik/team.yaml could not be used: Permission denied (os error 13)",
            ]
        );
    }
}
