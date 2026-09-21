//! The project a command runs against: the repository it is in, the files under `.farik/`, the
//! event log, and the two ids every event carries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_core::team::Team;
use farik_protocol::event::{EventBody, EventIds, NewEvent, new_event};
use farik_store::files::ProjectFiles;
use farik_store::{EventLog, EventQuery, Git, open_event_log};

/// Where the event log lives, under the gitignored `.farik/local/` (D5).
pub(crate) const DATABASE: &str = ".farik/local/farik.db";

/// An open project: everything a command needs to read what is there and record what it did.
pub struct Project {
    /// The repository root, which is what a project is (`docs/SPEC.md` section 3).
    pub root: PathBuf,
    /// The files under `.farik/`.
    pub files: ProjectFiles,
    /// The log every command appends to.
    pub log: Arc<EventLog>,
    /// The team, read once because the ids and every refusal about an agent come from it.
    pub team: Team,
    /// What every event this run records says it belongs to.
    pub ids: ProjectIds,
}

/// The team and the project an event belongs to (`docs/SPEC.md` section 8.5).
///
/// Both are fixed by `farik init` and read back from the log's first event afterwards, so that
/// renaming the team or the directory does not split one project's log in two.
pub struct ProjectIds {
    /// The team the events belong to.
    pub team_id: String,
    /// The project the events belong to.
    pub project_id: String,
}

impl ProjectIds {
    /// The ids the log already uses, or the ones this project starts with.
    ///
    /// # Errors
    ///
    /// The sentence the log's refusal reads as.
    pub fn of(log: &EventLog, team: &Team, root: &Path) -> Result<Self, String> {
        let first = log
            .read(&EventQuery {
                limit: Some(1),
                ..EventQuery::default()
            })
            .map_err(|error| error.to_string())?;
        if let Some(event) = first.first() {
            return Ok(Self {
                team_id: event.envelope.ids.team_id.clone(),
                project_id: event.envelope.ids.project_id.clone(),
            });
        }
        Ok(Self {
            team_id: slug(&team.name),
            project_id: slug(&directory_name(root)),
        })
    }
}

impl Project {
    /// One event of this project, ready to append.
    ///
    /// # Errors
    ///
    /// The sentence `new_event`'s refusal reads as, which is a blank id or a body about a contract
    /// with no contract named.
    pub fn event(
        &self,
        body: EventBody,
        at: DateTime<Utc>,
        task_id: Option<farik_core::contract::TaskId>,
    ) -> Result<NewEvent, String> {
        new_event(
            body,
            at,
            EventIds {
                team_id: self.ids.team_id.clone(),
                project_id: self.ids.project_id.clone(),
                task_id,
                agent_id: None,
                session_id: None,
            },
        )
        .map_err(|error| crate::refusal::event(&error))
    }

    /// The board this project's log makes, caught up to the log as it is now.
    ///
    /// # Errors
    ///
    /// The sentence the store's refusal reads as.
    pub fn projections(&self) -> Result<farik_store::Projections, String> {
        farik_store::open_projections(Arc::clone(&self.log)).map_err(|error| error.to_string())
    }

    /// Appends one event and answers with the sequence number the log gave it.
    ///
    /// # Errors
    ///
    /// The sentence the store's refusal reads as.
    pub fn append(&self, event: &NewEvent) -> Result<u64, String> {
        self.log
            .append(event)
            .map(|recorded| recorded.envelope.seq)
            .map_err(|error| error.to_string())
    }
}

/// The project the command was run in: the repository root, whatever directory under it the person
/// stood in.
///
/// # Errors
///
/// A sentence saying that this is not a git repository, that it is not a Farik project yet, or what
/// the team file or the log got wrong.
pub fn open_project(cwd: &Path, now: DateTime<Utc>) -> Result<Project, String> {
    let root = repository_root(cwd)?;
    let files = ProjectFiles::open(root.clone());
    let team = files.read_team().map_err(|error| match error {
        farik_store::files::FilesError::NotFound { .. } => format!(
            "there is no Farik project at {}: run farik init to make one",
            root.display()
        ),
        other => other.to_string(),
    })?;
    let log =
        Arc::new(open_event_log(&root.join(DATABASE), now).map_err(|error| error.to_string())?);
    let ids = ProjectIds::of(&log, &team, &root)?;
    Ok(Project {
        root,
        files,
        log,
        team,
        ids,
    })
}

/// The root of the repository the command was run in, as git reports it.
///
/// A project is a whole repository, so a command run three directories down is a command about the
/// same project (`docs/SPEC.md` section 3).
///
/// # Errors
///
/// A sentence saying this is not a git repository, or what git said instead.
pub fn repository_root(cwd: &Path) -> Result<PathBuf, String> {
    let git = Git::open(cwd.to_path_buf());
    match git.top_level() {
        Ok(root) => Ok(PathBuf::from(root)),
        Err(farik_store::GitError::NotARepository) => Err(format!(
            "{} is not a git repository, and a Farik project is one: run git init first",
            cwd.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// A directory's own name, for the id a project starts with.
pub(crate) fn directory_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// A kebab-case slug of a name a person chose, which is how an id is spelled everywhere in Farik
/// (`docs/standards/code.md`).
///
/// A name with nothing a slug can keep — punctuation, another script — answers `farik`, because a
/// blank id names nobody and `new_event` refuses one.
pub(crate) fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "farik".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn spells_a_name_a_person_chose_the_way_an_id_is_spelled() {
        assert_eq!(slug("Farik"), "farik");
        assert_eq!(slug("Maya Chen"), "maya-chen");
        assert_eq!(slug("  a  b  "), "a-b");
        assert_eq!(slug("my_project.v2"), "my-project-v2");
        assert_eq!(
            slug("プロジェクト"),
            "farik",
            "a name with nothing a slug can keep still names something: a blank id names nobody, \
             and new_event refuses one"
        );
        assert_eq!(slug("---"), "farik");
    }
}
