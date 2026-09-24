//! The files under `.farik/` (`docs/SPEC.md` sections 3, 5.8, 5.12, 5.13 and 8.4).
//!
//! The event log is the source of truth for what happened; these files are the source of truth for
//! what the team knows. They are what travels with the repository, so every one of them is text a
//! person can read, diff and edit — and every structured one is held, when it is read back, to
//! exactly the rules it would be held to arriving on the wire.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::NaiveDate;
use farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract};
use farik_core::criteria::{CriteriaLibrary, validate_criteria};
use farik_core::governor::paths::normalise;
use farik_core::pricing::prices::PRICE_TABLE;
use farik_core::pricing::{PriceTable, validate_price_table};
use farik_core::sprint::{Sprint, validate_sprint};
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
    /// There is a file there already, and this one is never written over.
    Exists {
        /// The path, relative to the project root.
        path: String,
    },
}

impl fmt::Display for FilesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(formatter, "there is no {path}"),
            Self::Invalid { path, detail } => write!(formatter, "{path} is not usable: {detail}"),
            Self::Io { path, detail } => write!(formatter, "{path} could not be used: {detail}"),
            Self::Exists { path } => write!(formatter, "{path} is there already"),
        }
    }
}

impl std::error::Error for FilesError {}

/// One decision under `.farik/decisions/`, as its file names it and says it (5.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEntry {
    /// Its number, which its file name starts with.
    pub number: u32,
    /// The rest of its file name, without `.md`.
    pub slug: String,
    /// Its title: its `# ` heading without the number, or its slug when it has none.
    pub title: String,
    /// The day it was written, from its `Date:` line, when it has one.
    pub date: Option<NaiveDate>,
    /// Who wrote it, from its `By:` line, when it has one.
    pub author: Option<String>,
}

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

/// How many files this process has written beside another.
///
/// The rename that publishes a file is all or nothing; what is in the file being renamed is not. So
/// two writers that shared it could publish a file holding half of each, or an empty one, or fail
/// the rename outright because the other had already renamed it away. This and the process id make
/// the name a writer's own, which is what the promise needs: `farik` and the daemon are different
/// processes on one project (8.5), and tasks run at the same time (5.14).
static WRITES_BESIDE: AtomicU64 = AtomicU64::new(0);

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
        validate_team(&value).map_err(|errors| refused(TEAM, &errors))
    }

    /// Writes the team, after holding it to the same rules.
    ///
    /// # Errors
    ///
    /// `Invalid` when the team is not one `validate_team` accepts, `Io` when it cannot be written.
    pub fn write_team(&self, team: &Team) -> Result<(), FilesError> {
        let value = as_wire(TEAM, team)?;
        validate_team(&value).map_err(|errors| refused(TEAM, &errors))?;
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
        validate_criteria(&value).map_err(|errors| refused(CRITERIA, &errors))
    }

    /// Writes the criterion library, after holding it to the same rules.
    ///
    /// # Errors
    ///
    /// `Invalid` when the library is not one `validate_criteria` accepts, `Io` when it cannot be
    /// written.
    pub fn write_criteria(&self, library: &CriteriaLibrary) -> Result<(), FilesError> {
        self.write_text(CRITERIA, &criteria_yaml(library)?)
    }

    /// One sprint, held to the rules a sprint on the wire is held to.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is no sprint with that id, or `id` is no sprint's (`S<n>`), so that
    /// nothing outside `sprints/` is read; `Invalid` when the file is not YAML or not a sprint, or
    /// holds a sprint with another id; `Io` otherwise.
    pub fn read_sprint(&self, id: &str) -> Result<Sprint, FilesError> {
        let path = sprint_path(id);
        if !is_sprint_id(id) {
            return Err(FilesError::NotFound {
                path: Self::named(&path),
            });
        }
        let value = self.read_yaml(&path)?;
        let sprint = validate_sprint(&value).map_err(|errors| refused(&path, &errors))?;
        if sprint.id.as_str() == id {
            Ok(sprint)
        } else {
            Err(FilesError::Invalid {
                path: Self::named(&path),
                detail: format!(
                    "the sprint in it says it is {}, and a sprint lives in the file its own id \
                     names",
                    sprint.id.as_str()
                ),
            })
        }
    }

    /// Writes a sprint to the file its own id names, after holding it to the same rules.
    /// `.farik/sprints/` is made on the first write; `init` does not make it.
    ///
    /// # Errors
    ///
    /// `Invalid` when the sprint is not one `validate_sprint` accepts, `Io` when it cannot be
    /// written.
    pub fn write_sprint(&self, sprint: &Sprint) -> Result<(), FilesError> {
        let path = sprint_path(sprint.id.as_str());
        let value = as_wire(&path, sprint)?;
        validate_sprint(&value).map_err(|errors| refused(&path, &errors))?;
        self.write_yaml(&path, &value)
    }

    /// Every sprint there is, by id, sorted by the number in it so that the tenth does not come
    /// before the second.
    ///
    /// A file under `sprints/` whose name is not `S<n>.yaml` is not one of them and is not an
    /// error either: the directory is a person's to keep notes in, as `contracts/` is.
    ///
    /// # Errors
    ///
    /// `Invalid` when a file named `S<n>.yaml` is not one `validate_sprint` accepts, `Io` when
    /// the directory cannot be read. A project with no `.farik/sprints/` has no sprints, which is
    /// not an error.
    pub fn list_sprints(&self) -> Result<Vec<Sprint>, FilesError> {
        let directory = self.farik().join(SPRINTS);
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let named = format!(".farik/{SPRINTS}");
        let entries = std::fs::read_dir(&directory).map_err(|error| FilesError::Io {
            path: named.clone(),
            detail: error.to_string(),
        })?;
        let mut sprints: Vec<Sprint> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| FilesError::Io {
                path: named.clone(),
                detail: error.to_string(),
            })?;
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".yaml")) else {
                continue;
            };
            if !entry.path().is_file() || !is_sprint_id(id) {
                continue;
            }
            sprints.push(self.read_sprint(id)?);
        }
        sprints.sort_by_key(|sprint| {
            sprint
                .id
                .as_str()
                .trim_start_matches('S')
                .parse::<u64>()
                .ok()
        });
        Ok(sprints)
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
        let contract = validate_contract(&value).map_err(|errors| refused(&path, &errors))?;
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
        self.write_text(&contract_path(&contract.id), &contract_yaml(contract)?)
    }

    /// Writes a new contract, refusing when its file is already there rather than writing over it.
    ///
    /// `write_contract` overwrites, which is what an update wants. A new contract that landed on an
    /// existing file would replace somebody's committed work, so this is the path creation takes.
    ///
    /// # Errors
    ///
    /// `Invalid` when the file is already there or the contract is not one `validate_contract`
    /// accepts, `Io` when it cannot be written.
    pub fn create_contract(&self, contract: &TaskContract) -> Result<(), FilesError> {
        let path = contract_path(&contract.id);
        // ponytail: checked then written, not one atomic operation; the log's counter is what keeps
        // two processes apart, and this only catches an id that collides with a file.
        if self.path_of(&path).exists() {
            return Err(FilesError::Invalid {
                path: Self::named(&path),
                detail: "a contract is already there, and a new one does not replace it"
                    .to_string(),
            });
        }
        self.write_contract(contract)
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
            // A person may keep notes in a directory too, and one named like a contract would put
            // a task on the board that cannot be read.
            if !entry.path().is_file() {
                continue;
            }
            if let Ok(id) = TaskId::try_from(name) {
                ids.push(id);
            }
        }
        // By the number in the id, so that the tenth task does not come before the ninth, and then
        // by the id itself, because the schema's pattern allows a leading zero: `FRK-01` and `FRK-1`
        // are two spellings of one number, and without the second key the order between them is
        // whatever `read_dir` gave, which is stable on one filesystem and not across a fresh clone.
        // `projections` broke this tie in step 03 and `reconcile` in step 07. The parse cannot fail:
        // `TaskId` is `FRK-` and one to six digits, which is what let it be built at all.
        ids.sort_by_key(|id| {
            (
                id.as_str().trim_start_matches("FRK-").parse::<u64>().ok(),
                id.as_str().to_string(),
            )
        });
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

    /// Writes the channel's summary as the agents were last shown it, for the human to read.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be written.
    pub fn write_channel_summary(&self, text: &str) -> Result<(), FilesError> {
        self.write_text(CHANNEL_SUMMARY, text)
    }

    /// What the team's retros learned, which the next planning is told (5.9), or nothing before
    /// the first retro.
    ///
    /// # Errors
    ///
    /// `Io` when the file is there and cannot be read.
    pub fn read_retro(&self) -> Result<Option<String>, FilesError> {
        match self.read_text(RETRO) {
            Err(FilesError::NotFound { .. }) => Ok(None),
            other => other.map(Some),
        }
    }

    /// Appends sprint `sprint_id`'s retro, written on `date`, to `team/retro.md`: a section
    /// `## <sprint id> (<date>)` and the text, below what is there, or below a `# Retro` title in
    /// a file made for it.
    ///
    /// # Errors
    ///
    /// `Io` when the file cannot be read or written.
    pub fn append_retro(
        &self,
        sprint_id: &str,
        date: NaiveDate,
        text: &str,
    ) -> Result<(), FilesError> {
        let before = self
            .read_retro()?
            .unwrap_or_else(|| "# Retro\n".to_string());
        // A file edited by hand may not end its last line.
        let end = if before.ends_with('\n') { "" } else { "\n" };
        let retro = format!(
            "{before}{end}\n## {sprint_id} ({})\n\n{}\n",
            date.format("%Y-%m-%d"),
            text.trim_end()
        );
        self.write_text(RETRO, &retro)
    }

    /// Writes a decision (5.8) as `decisions/<NNNN>-<slug>.md`, numbered one past the highest
    /// there is, and answers what it wrote. A decision is never written over: when another writer
    /// took the number first, this answers `Exists`, and a caller that asks again gets the next
    /// number.
    ///
    /// # Errors
    ///
    /// `Exists` when the file is there already, `Invalid` when the project has 9999 decisions,
    /// `Io` when the directory cannot be read or the file cannot be written.
    pub fn write_decision(
        &self,
        title: &str,
        text: &str,
        author: &str,
        date: NaiveDate,
    ) -> Result<DecisionEntry, FilesError> {
        let last = self
            .decision_files()?
            .last()
            .map_or(0, |(number, _)| *number);
        if last >= 9999 {
            return Err(FilesError::Invalid {
                path: Self::named(DECISIONS),
                detail: "the project has 9999 decisions".to_string(),
            });
        }
        self.write_numbered_decision(last + 1, title, text, author, date)
    }

    /// Writes decision `number`, unless a decision with that number is there already: the check
    /// that keeps two writers who picked the same number with different titles from both writing.
    fn write_numbered_decision(
        &self,
        number: u32,
        title: &str,
        text: &str,
        author: &str,
        date: NaiveDate,
    ) -> Result<DecisionEntry, FilesError> {
        // ponytail: a check before the link, so two writers inside the same instant can still both
        // pass it; today one `farik run` writes at a time. Take a lock around list-then-link when
        // sessions run concurrently.
        if let Some((_, taken)) = self
            .decision_files()?
            .into_iter()
            .find(|(found, _)| *found == number)
        {
            return Err(FilesError::Exists {
                path: Self::named(&format!("{DECISIONS}/{number:04}-{taken}.md")),
            });
        }
        let slug = slug(title);
        self.write_new(
            &format!("{DECISIONS}/{number:04}-{slug}.md"),
            &format!(
                "# {number:04}. {title}\n\nDate: {}\nBy: {author}\n\n{}\n",
                date.format("%Y-%m-%d"),
                text.trim_end()
            ),
        )?;
        Ok(DecisionEntry {
            number,
            slug,
            title: title.to_string(),
            date: Some(date),
            author: Some(author.to_string()),
        })
    }

    /// Every decision, oldest first: the files named `^[0-9]{4}-[a-z0-9-]+\.md$`, each with what
    /// can be read of it, so that one written by hand is listed too. Anything else there is
    /// ignored.
    ///
    /// # Errors
    ///
    /// `Io` when the directory or a decision cannot be read. A project with no
    /// `.farik/decisions/` has no decisions, which is not an error.
    pub fn list_decisions(&self) -> Result<Vec<DecisionEntry>, FilesError> {
        let mut decisions = Vec::new();
        for (number, slug) in self.decision_files()? {
            let text = self.read_text(&format!("{DECISIONS}/{number:04}-{slug}.md"))?;
            let line = |prefix: &str| {
                text.lines()
                    .find_map(|line| line.strip_prefix(prefix))
                    .map(str::trim)
            };
            // `# 0001. Title` as Farik writes it; a heading written by hand may have no number.
            let title = line("# ").map_or_else(
                || slug.clone(),
                |heading| {
                    heading
                        .split_once(". ")
                        .filter(|(digits, _)| digits.bytes().all(|byte| byte.is_ascii_digit()))
                        .map_or(heading, |(_, title)| title)
                        .to_string()
                },
            );
            decisions.push(DecisionEntry {
                number,
                title,
                date: line("Date: ")
                    .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
                author: line("By: ").map(str::to_string),
                slug,
            });
        }
        Ok(decisions)
    }

    /// Decision `number`'s whole file.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is no decision with that number, `Io` when it cannot be read.
    pub fn read_decision(&self, number: u32) -> Result<String, FilesError> {
        let (_, slug) = self
            .decision_files()?
            .into_iter()
            .find(|(found, _)| *found == number)
            .ok_or_else(|| FilesError::NotFound {
                path: Self::named(&format!("{DECISIONS}/{number:04}-*.md")),
            })?;
        self.read_text(&format!("{DECISIONS}/{number:04}-{slug}.md"))
    }

    /// The number and slug of every file under `decisions/` named as a decision, by number.
    fn decision_files(&self) -> Result<Vec<(u32, String)>, FilesError> {
        let directory = self.path_of(DECISIONS);
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let io = |error: std::io::Error| FilesError::Io {
            path: Self::named(DECISIONS),
            detail: error.to_string(),
        };
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&directory).map_err(io)? {
            let name = entry.map_err(io)?.file_name();
            let Some((digits, slug)) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".md"))
                .and_then(|name| name.split_at_checked(4))
                .and_then(|(digits, rest)| Some((digits, rest.strip_prefix('-')?)))
            else {
                continue;
            };
            let named_as_one = digits.bytes().all(|byte| byte.is_ascii_digit())
                && !slug.is_empty()
                && slug
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
            if let (true, Ok(number)) = (named_as_one, digits.parse()) {
                files.push((number, slug.to_string()));
            }
        }
        files.sort();
        Ok(files)
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
            .map_err(|errors| refused(PRICES, &errors))
    }

    /// The prices this project's costs are computed with: its `.farik/prices.json` as a whole when
    /// there is one, else the shipped table. Never a merge of the two, because 5.5 calls the file
    /// an override, and a model the user took out of it would still be priced by a merge.
    ///
    /// # Errors
    ///
    /// Those of `read_prices`.
    pub fn effective_prices(&self) -> Result<PriceTable, FilesError> {
        Ok(self.read_prices()?.unwrap_or_else(|| PRICE_TABLE.clone()))
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
const RETRO: &str = "team/retro.md";
const SPRINTS: &str = "sprints";
const DECISIONS: &str = "decisions";
const PROJECT_SCAN: &str = "project.md";
const PRICES: &str = "prices.json";
const SETTINGS: &str = "local/settings.json";
const CHANNEL_SUMMARY: &str = "local/channel-summary.md";

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

/// The wire value one piece of YAML holds, named by the path it came from so that a refusal says
/// which file it is about.
///
/// This is the one place the YAML a project holds or a person hands Farik is parsed, whatever
/// directory it came from: a contract a person hands `farik task create` is held to the same dialect
/// as the files under `.farik/` — no duplicate mapping key, no second document, the alias budget,
/// and `true` spelled `true` (ADR 0007). The other place is `farik-roles`, which reads the role
/// files embedded in the binary with the same options, because it cannot depend on this crate.
///
/// Read the way a file a person edits by hand should be. `UserMessageFormatter` is the crate's own
/// answer to the question, and its own default is explicitly not for a person to read: it recommends
/// the API call that would have accepted the file. And the name of the input is put back, because the
/// crate does not know it and calls it `<input>`.
///
/// # Errors
///
/// `Invalid`, carrying the line, the column and the snippet `serde-saphyr` reports.
pub fn yaml_value(text: &str, named: &str) -> Result<Value, FilesError> {
    serde_saphyr::from_str_with_options(text, yaml_options()).map_err(|error| FilesError::Invalid {
        path: named.to_string(),
        detail: error
            .render_with_formatter(&serde_saphyr::UserMessageFormatter)
            .replace("<input>", named),
    })
}

/// A contract as the YAML its file holds, after holding it to the rules a contract on the wire is
/// held to. `write_contract` writes this text, and a session's prompt shows it, so the dialect is
/// decided here once.
///
/// # Errors
///
/// `Invalid` when the contract is not one `validate_contract` accepts or cannot be written as YAML.
pub fn contract_yaml(contract: &TaskContract) -> Result<String, FilesError> {
    let path = contract_path(&contract.id);
    let value = as_wire(&path, contract)?;
    validate_contract(&value).map_err(|errors| refused(&path, &errors))?;
    yaml_text(&path, &value)
}

/// The criterion library as the YAML its file holds, after holding it to the same rules.
/// `write_criteria` writes this text, and a session's prompt shows it.
///
/// # Errors
///
/// `Invalid` when the library is not one `validate_criteria` accepts or cannot be written as YAML.
pub fn criteria_yaml(library: &CriteriaLibrary) -> Result<String, FilesError> {
    let value = as_wire(CRITERIA, library)?;
    validate_criteria(&value).map_err(|errors| refused(CRITERIA, &errors))?;
    yaml_text(CRITERIA, &value)
}

/// A wire value as the YAML a person reads and edits.
fn yaml_text(relative: &str, value: &Value) -> Result<String, FilesError> {
    serde_saphyr::to_string(value).map_err(|error| FilesError::Invalid {
        path: ProjectFiles::named(relative),
        detail: error.to_string(),
    })
}

/// The file a contract lives in: the one its own id names.
fn contract_path(id: &TaskId) -> String {
    format!("contracts/{}.yaml", id.as_str())
}

/// The file a sprint lives in: the one its own id names.
fn sprint_path(id: &str) -> String {
    format!("{SPRINTS}/{id}.yaml")
}

/// Whether a file name, with `.yaml` already stripped, is a sprint's: `docs/schemas/sprint.schema.json`'s
/// own pattern for `id`, `^S[1-9][0-9]{0,5}$`, checked here rather than with a regular expression
/// this module otherwise has no use for.
fn is_sprint_id(name: &str) -> bool {
    let Some(digits) = name.strip_prefix('S') else {
        return false;
    };
    !digits.is_empty()
        && digits.len() <= 6
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The file an agent's notebook lives in. An agent id is a slug the team schema pinned, so this
/// path cannot climb anywhere.
fn memory_path(agent_id: &AgentId) -> String {
    format!("agents/{}/memory.md", agent_id.as_str())
}

/// A decision's slug (5.8): the title in lower case, each run of anything but `a-z0-9` one `-`,
/// trimmed of `-`, cut at 60 characters and trimmed again, and `decision` when nothing is left.
fn slug(title: &str) -> String {
    let mut slug = String::new();
    for character in title.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let cut: String = slug.trim_matches('-').chars().take(60).collect();
    match cut.trim_matches('-') {
        "" => "decision".to_string(),
        slug => slug.to_string(),
    }
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

/// Every refusal a validator gave, as one `Invalid`.
fn refused(relative: &str, errors: &[ValidationError]) -> FilesError {
    FilesError::Invalid {
        path: ProjectFiles::named(relative),
        detail: errors
            .iter()
            .map(|error| format!("{} {}", error.path, error.message))
            .collect::<Vec<_>>()
            .join("; "),
    }
}

/// A typed value as the wire sees it, which is what a validator reads.
///
/// Every writer takes this step and then holds the result to its own file's rules, so that a value
/// built in memory cannot become a file that cannot be read back. The round trip is the promise:
/// what `write_team` accepts, `read_team` returns.
fn as_wire<T: Serialize>(relative: &str, value: &T) -> Result<Value, FilesError> {
    serde_json::to_value(value).map_err(|error| FilesError::Invalid {
        path: ProjectFiles::named(relative),
        detail: error.to_string(),
    })
}

/// The path with every part of it that exists resolved, and the rest as it was written.
///
/// Only the resolved prefix decides anything for the one caller there is: a symlink can only lead
/// somewhere through a component that exists, so both sides of `inside_product`'s comparison are
/// shortened by the same components when the rest is left off. The rest is put back for the words of
/// the refusal, and for a later caller that wants the whole path.
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
    ///
    /// The answer is about the file system as it was when the question was asked. Nothing in this
    /// module makes a symlink, so no caller can move the ground under itself, but a `git checkout`
    /// or another program could between this and the write. What that costs is bounded by who can
    /// write inside `.farik/` at all, which is the person whose project it is.
    fn inside_product(&self, path: &str) -> Result<String, FilesError> {
        let relative = product_path(path)?;
        let refuse = |detail: String| FilesError::Invalid {
            path: Self::named(&relative),
            detail,
        };
        let boundary = resolved(&self.path_of("product"))
            .map_err(|error| refuse(format!("the project root could not be resolved: {error}")))?;
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
            // is not content. The YAML parser tolerates one; `serde_json` does not, and refuses
            // `prices.json` or `local/settings.json` at column 1 saying only that it expected a
            // value, which is not a thing a person can act on.
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
            "{}.{}-{}.writing",
            path.extension()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default(),
            std::process::id(),
            WRITES_BESIDE.fetch_add(1, Ordering::Relaxed)
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

    /// Writes a file only where there is none, and never half of one: the text goes to a file
    /// beside it, which is then hard-linked to the name, and a hard link fails when the name is
    /// taken. A decision is immutable (5.8), so what is there is refused with `Exists`, not
    /// replaced.
    fn write_new(&self, relative: &str, text: &str) -> Result<(), FilesError> {
        let path = self.path_of(relative);
        if let Some(directory) = path.parent() {
            self.make_directory(directory)?;
        }
        let beside = path.with_extension(format!(
            "{}.{}-{}.writing",
            path.extension()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default(),
            std::process::id(),
            WRITES_BESIDE.fetch_add(1, Ordering::Relaxed)
        ));
        let linked =
            std::fs::write(&beside, text).and_then(|()| std::fs::hard_link(&beside, &path));
        let _ = std::fs::remove_file(&beside);
        match linked {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(FilesError::Exists {
                    path: Self::named(relative),
                })
            }
            other => other.map_err(|error| FilesError::Io {
                path: Self::named(relative),
                detail: error.to_string(),
            }),
        }
    }

    /// One YAML file under `.farik/` as an untrusted value, for a validator to hold to its rules.
    fn read_yaml(&self, relative: &str) -> Result<Value, FilesError> {
        let text = self.read_text(relative)?;
        yaml_value(&text, &Self::named(relative))
    }

    /// Writes a wire value as the YAML a person reads and edits.
    fn write_yaml(&self, relative: &str, value: &Value) -> Result<(), FilesError> {
        self.write_text(relative, &yaml_text(relative, value)?)
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::TaskId;
    use farik_core::team::AgentId;

    use chrono::NaiveDate;

    use super::fixtures::TempProject;
    use super::{DecisionEntry, FilesError, contract_path, memory_path, product_path, slug};

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

    /// The day every decision in these tests is written on.
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 24).expect("a real date")
    }

    #[test]
    fn writes_a_numbered_decision() {
        let project = TempProject::new("decision-numbered");
        let files = project.files();

        let first = files
            .write_decision(
                "Use SQLite for the log",
                "One file, no server.",
                "arch",
                day(),
            )
            .expect("the first decision");
        let second = files
            .write_decision(
                "Keep the log append-only",
                "Nothing is rewritten.",
                "pm",
                day(),
            )
            .expect("the second decision");

        assert_eq!(
            first,
            DecisionEntry {
                number: 1,
                slug: "use-sqlite-for-the-log".to_string(),
                title: "Use SQLite for the log".to_string(),
                date: Some(day()),
                author: Some("arch".to_string()),
            }
        );
        assert_eq!(
            std::fs::read_to_string(
                project
                    .root
                    .join(".farik/decisions/0001-use-sqlite-for-the-log.md")
            )
            .expect("the first file"),
            "# 0001. Use SQLite for the log\n\nDate: 2026-09-24\nBy: arch\n\nOne file, no server.\n"
        );
        assert_eq!(second.number, 2);
        assert!(
            project
                .root
                .join(".farik/decisions/0002-keep-the-log-append-only.md")
                .is_file()
        );
        assert_eq!(
            files.list_decisions().expect("the decisions"),
            [first, second]
        );
    }

    #[test]
    fn never_overwrites_a_decision() {
        let project = TempProject::new("decision-immutable");
        let files = project.files();
        let decisions = project.root.join(".farik/decisions");
        std::fs::create_dir_all(&decisions).expect("the directory");
        std::fs::write(decisions.join("0003-x.md"), "placed by hand").expect("a file");

        let next = files
            .write_decision("Next", "After the hand-made one.", "arch", day())
            .expect("the next decision");
        let clash = files.write_new("decisions/0004-next.md", "another text");

        assert_eq!(next.number, 4, "one past the highest there is");
        assert_eq!(
            clash,
            Err(FilesError::Exists {
                path: ".farik/decisions/0004-next.md".to_string()
            })
        );
        assert!(
            std::fs::read_to_string(decisions.join("0004-next.md"))
                .expect("the file")
                .ends_with("After the hand-made one.\n"),
            "the decision is what was first written"
        );
        let mut left: Vec<String> = std::fs::read_dir(&decisions)
            .expect("the directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["0003-x.md", "0004-next.md"],
            "no temporary file is left"
        );
    }

    #[test]
    fn gives_no_two_decisions_one_number() {
        let project = TempProject::new("decision-one-number");
        let files = project.files();

        // Two writers that both saw no decision pick number 1, with different titles.
        files
            .write_numbered_decision(1, "A", "The first.", "arch", day())
            .expect("the first writer's decision");
        let clash = files.write_numbered_decision(1, "B", "The second.", "pm", day());

        assert_eq!(
            clash,
            Err(FilesError::Exists {
                path: ".farik/decisions/0001-a.md".to_string()
            })
        );
        assert_eq!(
            files
                .list_decisions()
                .expect("the decisions")
                .iter()
                .map(|decision| decision.slug.as_str())
                .collect::<Vec<_>>(),
            ["a"],
            "the second writer wrote nothing"
        );
    }

    #[test]
    fn refuses_a_decision_past_9999() {
        let project = TempProject::new("decision-past-9999");
        let files = project.files();
        let decisions = project.root.join(".farik/decisions");
        std::fs::create_dir_all(&decisions).expect("the directory");
        std::fs::write(decisions.join("9998-x.md"), "placed by hand").expect("a file");

        let last = files
            .write_decision("The last", "Number 9999.", "arch", day())
            .expect("the 9999th decision");
        let refused = files.write_decision("One more", "Number 10000.", "arch", day());

        assert_eq!(last.number, 9999);
        assert_eq!(
            refused,
            Err(FilesError::Invalid {
                path: ".farik/decisions".to_string(),
                detail: "the project has 9999 decisions".to_string(),
            })
        );
        assert_eq!(files.list_decisions().expect("the decisions").len(), 2);
    }

    #[test]
    fn slugs_a_title() {
        assert_eq!(slug("  Why?! Rust & Tauri  "), "why-rust-tauri");
        assert_eq!(slug("!!!"), "decision");
        // 59 characters, a space, then more: the cut at 60 falls on the `-` the space became.
        let title = format!("{} {}", "a".repeat(59), "b".repeat(10));
        assert_eq!(title.chars().count(), 70);
        assert_eq!(slug(&title), "a".repeat(59));
    }

    #[test]
    fn lists_a_decision_written_by_hand() {
        let project = TempProject::new("decision-by-hand");
        let files = project.files();
        let decisions = project.root.join(".farik/decisions");
        std::fs::create_dir_all(&decisions).expect("the directory");
        std::fs::write(decisions.join("0005-x.md"), "hello").expect("a file");
        std::fs::write(decisions.join("README.md"), "not a decision").expect("a file");
        std::fs::write(decisions.join("0006-Upper.md"), "not a decision").expect("a file");

        assert_eq!(
            files.list_decisions().expect("the decisions"),
            [DecisionEntry {
                number: 5,
                slug: "x".to_string(),
                title: "x".to_string(),
                date: None,
                author: None,
            }]
        );
        assert_eq!(files.read_decision(5).expect("its text"), "hello");
        assert_eq!(
            files.read_decision(9),
            Err(FilesError::NotFound {
                path: ".farik/decisions/0009-*.md".to_string()
            })
        );
    }
}
