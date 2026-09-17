//! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
//!
//! The event log is the source of truth for what happened; these files are the source of truth for
//! what the team knows. They are what travels with the repository, so every one of them is text a
//! person can read, diff and edit — and every structured one is held, when it is read back, to
//! exactly the rules it would be held to arriving on the wire.

use std::fmt;
use std::path::{Path, PathBuf};

use farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract};
use farik_core::criteria::{CriteriaLibrary, validate_criteria};
use farik_core::governor::paths::normalise;
use farik_core::pricing::{PriceTable, validate_price_table};
use farik_core::team::{AgentId, Team, validate_team};
use serde::{Deserialize, Serialize};
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

/// Where the sandbox runs (`docs/SPEC.md` section 8.3). Machine-local, because one person's laptop
/// has Docker and another's does not, and that is not a fact about the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sandbox {
    /// One container per task, which is what the spec asks for.
    Docker,
    /// No sandbox: every task runs on the machine itself.
    None,
}

/// What this machine knows that the repository does not.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LocalSettings {
    /// Where a task's session runs.
    pub sandbox: Sandbox,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            sandbox: Sandbox::Docker,
        }
    }
}

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

    /// The criterion library, held to the rules one on the wire is held to.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is no library, `Invalid` when it is not YAML or not a library, `Io`
    /// otherwise.
    pub fn read_criteria(&self) -> Result<CriteriaLibrary, FilesError> {
        let value = self.read_yaml(CRITERIA)?;
        validate_criteria(&value).map_err(|errors| self.refused(CRITERIA, &errors))
    }

    /// Writes the criterion library, after holding it to the same rules.
    ///
    /// # Errors
    ///
    /// `Invalid` when the library is not one `validate_criteria` accepts, `Io` when it cannot be
    /// written.
    pub fn write_criteria(&self, library: &CriteriaLibrary) -> Result<(), FilesError> {
        let value = self.as_wire(CRITERIA, library)?;
        validate_criteria(&value).map_err(|errors| self.refused(CRITERIA, &errors))?;
        self.write_yaml(CRITERIA, &value)
    }

    /// One task's contract, held to the rules a contract on the wire is held to.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is no contract with that id, `Invalid` when the file is not YAML or
    /// not a contract, or when the contract in it carries a different id from the one asked for.
    pub fn read_contract(&self, id: &TaskId) -> Result<TaskContract, FilesError> {
        let path = contract_path(id);
        let value = self.read_yaml(&path)?;
        let contract = validate_contract(&value).map_err(|errors| self.refused(&path, &errors))?;
        if contract.id == *id {
            Ok(contract)
        } else {
            Err(FilesError::Invalid {
                path: Self::named(&path),
                detail: format!(
                    "the contract in it says it is {}, and a contract lives in the file its own id \
                     names",
                    contract.id.as_str()
                ),
            })
        }
    }

    /// Writes a contract to the file its own id names, after holding it to the same rules.
    ///
    /// # Errors
    ///
    /// `Invalid` when the contract is not one `validate_contract` accepts, `Io` when it cannot be
    /// written.
    pub fn write_contract(&self, contract: &TaskContract) -> Result<(), FilesError> {
        let path = contract_path(&contract.id);
        let value = self.as_wire(&path, contract)?;
        validate_contract(&value).map_err(|errors| self.refused(&path, &errors))?;
        self.write_yaml(&path, &value)
    }

    /// Every contract there is, by id, in the order a board shows them: by the number in the id, so
    /// that the tenth task does not come before the ninth.
    ///
    /// A file under `contracts/` that is not a contract's is not one of them and is not an error
    /// either: the directory is a person's to keep notes in.
    ///
    /// # Errors
    ///
    /// `Io` when the directory cannot be read. A project with no `.farik/` has no contracts, which
    /// is not an error.
    pub fn list_contracts(&self) -> Result<Vec<TaskId>, FilesError> {
        let directory = self.farik().join("contracts");
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&directory).map_err(|error| FilesError::Io {
            path: ".farik/contracts".to_string(),
            detail: error.to_string(),
        })?;
        let mut ids: Vec<TaskId> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| FilesError::Io {
                path: ".farik/contracts".to_string(),
                detail: error.to_string(),
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".yaml")) else {
                continue;
            };
            if let Ok(id) = TaskId::try_from(name) {
                ids.push(id);
            }
        }
        // By the number in the id, so that the tenth task does not come before the ninth. The
        // parse cannot fail: `TaskId` is `FRK-` and one to six digits, which is what let it be
        // built at all.
        ids.sort_by_key(|id| id.as_str().trim_start_matches("FRK-").parse::<u64>().ok());
        Ok(ids)
    }

    /// An agent's own notebook, which is included in every session it runs (5.8).
    ///
    /// An agent that has never written one has an empty notebook rather than no notebook, because
    /// every session includes it and a missing file is not a thing to tell an agent about.
    ///
    /// # Errors
    ///
    /// `Io` when the file is there and cannot be read.
    pub fn read_memory(&self, agent_id: &AgentId) -> Result<String, FilesError> {
        match self.read_text(&memory_path(agent_id)) {
            Err(FilesError::NotFound { .. }) => Ok(String::new()),
            other => other,
        }
    }

    /// Writes an agent's notebook.
    ///
    /// # Errors
    ///
    /// `Io` when the directory cannot be made or the file cannot be written.
    pub fn write_memory(&self, agent_id: &AgentId, text: &str) -> Result<(), FilesError> {
        self.write_text(&memory_path(agent_id), text)
    }

    /// What the project scan read back about this repository (5.8).
    ///
    /// # Errors
    ///
    /// `NotFound` when the project has not been scanned, `Io` when the file cannot be read.
    pub fn read_project_scan(&self) -> Result<String, FilesError> {
        self.read_text(PROJECT_SCAN)
    }

    /// Writes what the project scan read back.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be written.
    pub fn write_project_scan(&self, text: &str) -> Result<(), FilesError> {
        self.write_text(PROJECT_SCAN, text)
    }

    /// A product document, by its path under `product/`.
    ///
    /// # Errors
    ///
    /// `Invalid` when the path climbs out of `product/`, `NotFound` when there is no such document,
    /// `Io` when it cannot be read.
    pub fn read_product_doc(&self, path: &str) -> Result<String, FilesError> {
        self.read_text(&self.inside_product(path)?)
    }

    /// Writes a product document.
    ///
    /// # Errors
    ///
    /// `Invalid` when the path climbs out of `product/`, `Io` when it cannot be written.
    pub fn write_product_doc(&self, path: &str, text: &str) -> Result<(), FilesError> {
        self.write_text(&self.inside_product(path)?, text)
    }

    /// The price table this project overrides the shipped one with, or nothing when it does not.
    ///
    /// # Errors
    ///
    /// `Invalid` when the file is there and is not a price table, `Io` when it cannot be read.
    pub fn read_prices(&self) -> Result<Option<PriceTable>, FilesError> {
        let text = match self.read_text(PRICES) {
            Err(FilesError::NotFound { .. }) => return Ok(None),
            other => other?,
        };
        let value: Value = serde_json::from_str(&text).map_err(|error| FilesError::Invalid {
            path: Self::named(PRICES),
            detail: error.to_string(),
        })?;
        validate_price_table(&value)
            .map(Some)
            .map_err(|errors| self.refused(PRICES, &errors))
    }

    /// What this machine knows, or the defaults when it has not been asked.
    ///
    /// # Errors
    ///
    /// `Invalid` when the file is there and is not settings, `Io` when it cannot be read.
    pub fn read_settings(&self) -> Result<LocalSettings, FilesError> {
        let text = match self.read_text(SETTINGS) {
            Err(FilesError::NotFound { .. }) => return Ok(LocalSettings::default()),
            other => other?,
        };
        serde_json::from_str(&text).map_err(|error| FilesError::Invalid {
            path: Self::named(SETTINGS),
            detail: error.to_string(),
        })
    }

    /// Writes what this machine knows.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be written.
    pub fn write_settings(&self, settings: &LocalSettings) -> Result<(), FilesError> {
        let text = serde_json::to_string_pretty(settings).map_err(|error| FilesError::Invalid {
            path: Self::named(SETTINGS),
            detail: error.to_string(),
        })?;
        self.write_text(SETTINGS, &format!("{text}\n"))
    }
}

/// Where each file lives, relative to `.farik/`. One place, so that a reader of this module can see
/// the whole layout at once and a change to it is one line.
const TEAM: &str = "team.yaml";
const CRITERIA: &str = "team/criteria.yaml";
const PROJECT_SCAN: &str = "project.md";
const PRICES: &str = "prices.json";
const SETTINGS: &str = "local/settings.json";

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

/// The file a contract lives in: the one its own id names.
fn contract_path(id: &TaskId) -> String {
    format!("contracts/{}.yaml", id.as_str())
}

/// The file an agent's notebook lives in. An agent id is a slug the team schema pinned, so this
/// path cannot climb anywhere.
fn memory_path(agent_id: &AgentId) -> String {
    format!("agents/{}/memory.md", agent_id.as_str())
}

/// A product document's path, or a refusal when it climbs out of `product/`.
///
/// The path comes from a tool call, so it is a string an agent chose. `farik-core`'s own path rule
/// is what answers: empty, absolute, or holding a `..` segment is refused, and `.` segments and
/// backslashes are dropped on the way. Everything else is somewhere under `product/`, which is the
/// only place 5.6 lets a product document be written.
fn product_path(path: &str) -> Result<String, FilesError> {
    let normalised = normalise(path).ok_or_else(|| FilesError::Invalid {
        path: ProjectFiles::named(&format!("product/{path}")),
        detail: "a product document lives under product/, and this path climbs out of it"
            .to_string(),
    })?;
    Ok(format!("product/{normalised}"))
}

/// The path with every part of it that exists resolved, and the rest as it was written.
///
/// `canonicalize` refuses a path that is not there, and most of these are not there yet. So the
/// deepest part that does exist is resolved — which is what follows a symlink — and what is left is
/// put back on the end, where there is no existing directory for a link to hide in.
fn resolved(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut left = Vec::new();
    let mut existing = path.to_path_buf();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                left.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut answer = std::fs::canonicalize(&existing)?;
    while let Some(name) = left.pop() {
        answer.push(name);
    }
    Ok(answer)
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

    /// A product document's path, once the file system has been asked as well as the string.
    ///
    /// `product_path` answers what the text says; this answers where it lands. A directory under
    /// `product/` may be a symlink pointing anywhere, and following one would put a tool call's
    /// chosen path outside `.farik/` entirely — the string rule alone cannot see that, because
    /// there is no `..` in it. So the path and `product/` itself are each resolved as far as they
    /// exist, and the one has to be under the other.
    ///
    /// Nothing is made here, not even the directory the answer is about. A read that conjured
    /// `.farik/` would make a project of whatever directory it was pointed at.
    fn inside_product(&self, path: &str) -> Result<String, FilesError> {
        let relative = product_path(path)?;
        let refuse = |detail: String| FilesError::Invalid {
            path: Self::named(&relative),
            detail,
        };
        let boundary =
            resolved(&self.path_of("product")).map_err(|error| refuse(error.to_string()))?;
        let landing =
            resolved(&self.path_of(&relative)).map_err(|error| refuse(error.to_string()))?;
        if landing.starts_with(&boundary) {
            Ok(relative)
        } else {
            Err(refuse(format!(
                "it leads out of product/, to {}, which a product document may not be",
                landing.display()
            )))
        }
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
    use farik_core::contract::TaskId;
    use farik_core::team::AgentId;

    use super::{FilesError, contract_path, memory_path, product_path};

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

    #[test]
    fn names_the_file_a_contract_lives_in() {
        assert_eq!(
            contract_path(&TaskId::try_from("FRK-12").expect("an id")),
            "contracts/FRK-12.yaml"
        );
    }

    #[test]
    fn names_the_file_a_notebook_lives_in() {
        assert_eq!(
            memory_path(&AgentId::try_from("ada").expect("an id")),
            "agents/ada/memory.md"
        );
    }

    #[test]
    fn keeps_a_product_document_under_product() {
        assert_eq!(
            product_path("roadmap.md").expect("a path"),
            "product/roadmap.md"
        );
        assert_eq!(
            product_path("./areas/login.md").expect("a path"),
            "product/areas/login.md",
            "a . segment is dropped rather than refused"
        );
        assert_eq!(
            product_path("areas\\login.md").expect("a path"),
            "product/areas/login.md",
            "and a backslash is a separator"
        );
    }

    #[test]
    fn refuses_a_product_path_that_climbs_out_of_product() {
        // The path comes from a tool call, so it is a string an agent chose. 5.6 puts product
        // documents under product/ and nowhere else, and one `..` would put this one in the
        // repository's own source.
        for path in [
            "../team.yaml",
            "../../etc/passwd",
            "/etc/passwd",
            "",
            "a/../../b",
        ] {
            let refused = product_path(path);
            let Err(FilesError::Invalid {
                path: named,
                detail,
            }) = refused
            else {
                panic!("{path:?} climbs out: {refused:?}");
            };
            assert_eq!(named, format!(".farik/product/{path}"));
            assert!(detail.contains("climbs out of it"), "{detail}");
        }
    }
}
