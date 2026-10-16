//! A project folder's member repositories on the file system (`docs/SPEC.md` 4.1): whether a
//! folder may join, and whether one is still where the project left it.

use std::fmt;
use std::path::{Path, PathBuf};

use catervas_core::team::Team;

use crate::files::{FilesError, ProjectFiles};
use crate::git::{Git, GitError, integration_branch};

/// Why a folder cannot be a member repository, as the file system and git see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberPathRefusal {
    /// There is no such folder.
    NotThere,
    /// Followed through its links, it is not inside home.
    OutsideHome,
    /// It is home itself.
    HomeItself,
    /// It is not a git repository.
    NotARepository,
    /// It is inside a git repository, and not at its top.
    NotTheTop {
        /// The repository's top folder.
        top: PathBuf,
    },
    /// It has a Catervas team of its own.
    HasTeam,
    /// The integration branch asked for is not a local branch of it.
    NoSuchBranch {
        /// The branch asked for.
        branch: String,
    },
}

impl MemberPathRefusal {
    /// The refusal's code on the wire.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotThere => "not_there",
            Self::OutsideHome => "outside_home",
            Self::HomeItself => "home_itself",
            Self::NotARepository => "not_a_repository",
            Self::NotTheTop { .. } => "not_the_top",
            Self::HasTeam => "has_team",
            Self::NoSuchBranch { .. } => "no_such_branch",
        }
    }

    /// The one sentence that says it, lower case with no full stop.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotThere => "that folder is not there".to_string(),
            Self::OutsideHome => "that folder is outside your home folder".to_string(),
            Self::HomeItself => {
                "your home folder itself cannot be one of the project's repositories".to_string()
            }
            Self::NotARepository => "that folder is not a git repository".to_string(),
            Self::NotTheTop { top } => format!(
                "that folder is inside a git repository; choose {} instead",
                top.display()
            ),
            Self::HasTeam => "that repository has a Catervas team of its own".to_string(),
            Self::NoSuchBranch { branch } => {
                format!("that repository has no branch named {branch}")
            }
        }
    }

    /// The JSON pointer into the request the refusal is about.
    #[must_use]
    pub fn pointer(&self) -> &'static str {
        match self {
            Self::NoSuchBranch { .. } => "/integration_branch",
            _ => "/path",
        }
    }
}

/// `path`, relative to `home`, followed through its links, as the top folder of a git repository
/// that has no Catervas team of its own.
///
/// # Errors
///
/// The first of these a member's folder breaks: it is not there, it is outside home once its links
/// are followed, it is home itself, it is not a git repository, it is inside one and not at its
/// top (a repository whose top is home or above it counts as none), or it has a team.
pub fn member_root(home: &Path, path: &str) -> Result<PathBuf, MemberPathRefusal> {
    let folder = std::fs::canonicalize(home.join(path)).map_err(|_| MemberPathRefusal::NotThere)?;
    let home = std::fs::canonicalize(home).map_err(|_| MemberPathRefusal::OutsideHome)?;
    if !folder.starts_with(&home) {
        return Err(MemberPathRefusal::OutsideHome);
    }
    if folder == home {
        return Err(MemberPathRefusal::HomeItself);
    }
    let git = Git::open(folder.clone());
    if !git.is_repository() {
        return Err(MemberPathRefusal::NotARepository);
    }
    let top = git
        .top_level()
        .ok()
        .and_then(|top| std::fs::canonicalize(top).ok())
        .ok_or(MemberPathRefusal::NotARepository)?;
    if top != folder {
        // A top folder that is home or above it is not one to point at: the folder is not a
        // repository of its own, so no refusal names home or a folder outside it.
        if top != home && top.starts_with(&home) {
            return Err(MemberPathRefusal::NotTheTop { top });
        }
        return Err(MemberPathRefusal::NotARepository);
    }
    if folder.join(".catervas/team.yaml").exists() {
        return Err(MemberPathRefusal::HasTeam);
    }
    Ok(folder)
}

/// Whether `path` is still the top folder of a git repository: a folder that is gone, that is not
/// a repository any more, or that is inside another is not.
#[must_use]
pub fn repo_present(path: &Path) -> bool {
    let Ok(folder) = std::fs::canonicalize(path) else {
        return false;
    };
    Git::open(folder.clone())
        .top_level()
        .ok()
        .and_then(|top| std::fs::canonicalize(top).ok())
        .is_some_and(|top| top == folder)
}

/// Where a task's work is: the repository its branch and worktree are made in, and the branch
/// they are made from and merge into (`docs/SPEC.md` 5.14, ADR 0057).
pub struct TaskRepository {
    /// The member's name, or `None` for the project's own folder.
    pub repo: Option<String>,
    /// The repository's adapter.
    pub git: Git,
    /// The branch accepted work goes into.
    pub integration_branch: String,
}

/// Why a task's repository cannot be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRepositoryError {
    /// The task names a repository, and the project has `.catervas/repos.yaml` nowhere.
    NotAProjectFolder {
        /// The name the task gives.
        repo: String,
    },
    /// The task names a repository the project folder does not list.
    UnknownRepo {
        /// The name the task gives.
        repo: String,
    },
    /// The member is listed, and its folder is not a repository's top any more.
    Missing {
        /// The member's name.
        repo: String,
        /// Where `repos.yaml` says it is.
        path: PathBuf,
    },
    /// `repos.yaml` could not be read.
    Files(FilesError),
    /// Git refused.
    Git(GitError),
}

impl fmt::Display for TaskRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAProjectFolder { repo } => write!(
                formatter,
                "this task names repository {repo}, but this project is one repository"
            ),
            Self::UnknownRepo { repo } => write!(
                formatter,
                "this task names repository {repo}, which this project does not have"
            ),
            Self::Missing { repo, path } => write!(
                formatter,
                "{repo} is not at {}: locate it in Settings",
                path.display()
            ),
            Self::Files(error) => write!(formatter, "{error}"),
            Self::Git(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for TaskRepositoryError {}

impl From<FilesError> for TaskRepositoryError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<GitError> for TaskRepositoryError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

/// The repository a task works in: the project's own folder for a task that names none, else the
/// member it names, with that member's integration branch, or its default branch when it names
/// none. The team's `policy.integration_branch` is the project folder's own branch alone (ADR 0057).
///
/// # Errors
///
/// `NotAProjectFolder` when a name is given and there is no `repos.yaml`, `UnknownRepo` when it
/// lists no such member, `Missing` when the member's folder is not a repository's top any more,
/// `Files` when `repos.yaml` cannot be read, `Git` when git refuses.
pub fn task_repository(
    files: &ProjectFiles,
    team: &Team,
    project: &Git,
    repo: Option<&str>,
) -> Result<TaskRepository, TaskRepositoryError> {
    let Some(name) = repo else {
        let git = Git::open(project.root().to_path_buf());
        let integration_branch = integration_branch(team, &git)?;
        return Ok(TaskRepository {
            repo: None,
            git,
            integration_branch,
        });
    };
    let repos = files
        .read_repos()?
        .ok_or_else(|| TaskRepositoryError::NotAProjectFolder {
            repo: name.to_string(),
        })?;
    let member = repos
        .repos
        .iter()
        .find(|member| member.name.as_str() == name)
        .ok_or_else(|| TaskRepositoryError::UnknownRepo {
            repo: name.to_string(),
        })?;
    let path = PathBuf::from(member.path.as_str());
    if !repo_present(&path) {
        return Err(TaskRepositoryError::Missing {
            repo: name.to_string(),
            path,
        });
    }
    let git = Git::open(path);
    let integration_branch = match &member.integration_branch {
        Some(branch) => {
            git.check_branch_name(branch.as_str())?;
            branch.as_str().to_string()
        }
        None => git.default_branch()?,
    };
    Ok(TaskRepository {
        repo: Some(name.to_string()),
        git,
        integration_branch,
    })
}

/// The numbers of the task branches in the project folder and in every member that is present,
/// local and remote (`docs/SPEC.md` 8.4): a task id is unique across all of them. A repository
/// that is missing, or whose branches cannot be listed, counts as having none; a `repos.yaml` that
/// cannot be read leaves the project folder's own branches alone.
#[must_use]
pub fn task_branch_numbers_everywhere(files: &ProjectFiles) -> Vec<u64> {
    let mut numbers = Git::open(files.root().to_path_buf())
        .task_branch_numbers()
        .unwrap_or_default();
    let members = files.read_repos().ok().flatten().map(|repos| repos.repos);
    for member in members.into_iter().flatten() {
        let path = PathBuf::from(member.path.as_str());
        if repo_present(&path) {
            numbers.extend(Git::open(path).task_branch_numbers().unwrap_or_default());
        }
    }
    numbers
}
