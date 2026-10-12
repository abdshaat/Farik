//! A project folder's member repositories on the file system (`docs/SPEC.md` 4.1): whether a
//! folder may join, and whether one is still where the project left it.

use std::path::{Path, PathBuf};

use crate::git::Git;

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
