//! Where the team's documents are read from (ADR 0074): a branch of the repository, as before, or
//! the project's checkout on disk when they are kept on this computer.

use std::path::{Path, PathBuf};

use catervas_core::folders::{DocumentFormat, DocumentsMode, with_format};
use catervas_core::team::Team;

use crate::baseline::files_under;
use crate::git::{Git, GitError, TreeEntry, integration_branch};
use crate::{EventLog, StoreError, folder_docs};

/// Where documents are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentSource {
    /// A branch of the repository: the integration branch, or a task's.
    Branch(String),
    /// The checkout at this root, its files as they are on disk.
    Disk(PathBuf),
}

fn refused(what: &str, detail: impl std::fmt::Display) -> GitError {
    GitError::CommandFailed {
        command: what.to_string(),
        stderr: detail.to_string(),
    }
}

/// Whether a segment of `path`, walked from `base`, is a link.
fn behind_a_link(base: &Path, path: &str) -> bool {
    let mut at = base.to_path_buf();
    for segment in path.split('/') {
        at.push(segment);
        match std::fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

impl DocumentSource {
    /// Every file under `under`, at any depth, as a tree lists it. On disk, a regular file is mode
    /// `100644` and its blob the id git gives its bytes; links are left out.
    ///
    /// # Errors
    ///
    /// What git or the file system refuses.
    pub fn entries(&self, git: &Git, under: &str) -> Result<Vec<TreeEntry>, GitError> {
        match self {
            Self::Branch(rev) => git.tree_entries(rev, under),
            Self::Disk(root) => {
                if behind_a_link(root, under) {
                    return Ok(Vec::new());
                }
                let target = root.join(under);
                let kind = std::fs::symlink_metadata(&target).ok();
                if kind.as_ref().is_some_and(std::fs::Metadata::is_file) {
                    let blob = git.hash_files(&[target])?;
                    return Ok(blob
                        .into_iter()
                        .map(|blob| TreeEntry {
                            mode: "100644".to_string(),
                            blob,
                            path: under.to_string(),
                        })
                        .collect());
                }
                if !kind.is_some_and(|kind| kind.is_dir()) {
                    return Ok(Vec::new());
                }
                let found = files_under(&target, &[])
                    .map_err(|error| refused("read the documents", error))?;
                let blobs = git.hash_files(&found.values().cloned().collect::<Vec<_>>())?;
                Ok(found
                    .into_keys()
                    .zip(blobs)
                    .map(|(relative, blob)| TreeEntry {
                        mode: "100644".to_string(),
                        blob,
                        path: format!("{under}/{relative}"),
                    })
                    .collect())
            }
        }
    }

    /// The text of the file `path`, or `None` when there is none (or, on disk, when it is, or lies
    /// behind, a link).
    ///
    /// # Errors
    ///
    /// What git refuses.
    pub fn text(&self, git: &Git, path: &str) -> Result<Option<String>, GitError> {
        match self {
            Self::Branch(rev) => {
                if git.has_file(rev, path)? {
                    git.file_at(rev, path).map(Some)
                } else {
                    Ok(None)
                }
            }
            Self::Disk(root) => {
                if behind_a_link(root, path) {
                    return Ok(None);
                }
                let regular = std::fs::symlink_metadata(root.join(path)).is_ok_and(|m| m.is_file());
                Ok(regular
                    .then(|| std::fs::read(root.join(path)).ok())
                    .flatten()
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
            }
        }
    }

    /// Whether there is a file at `path`.
    ///
    /// # Errors
    ///
    /// What git refuses.
    pub fn has_file(&self, git: &Git, path: &str) -> Result<bool, GitError> {
        match self {
            Self::Branch(rev) => git.has_file(rev, path),
            Self::Disk(root) => Ok(!behind_a_link(root, path)
                && std::fs::symlink_metadata(root.join(path)).is_ok_and(|m| m.is_file())),
        }
    }

    /// The path `stem` has, in whichever format it is held, the other than the team's `format`
    /// first, as `git::held_document` has it for a branch.
    ///
    /// # Errors
    ///
    /// What git refuses.
    pub fn held_document(
        &self,
        git: &Git,
        stem: &str,
        format: DocumentFormat,
    ) -> Result<Option<String>, GitError> {
        let other = match format {
            DocumentFormat::Markdown => DocumentFormat::Html,
            DocumentFormat::Html => DocumentFormat::Markdown,
        };
        for held in [other, format] {
            let path = with_format(stem, held);
            if self.has_file(git, &path)? {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }
}

/// Where the team's documents are read from in `mode`: the project's checkout when they are kept on
/// this computer, else the integration branch.
///
/// # Errors
///
/// What git refuses.
pub fn team_documents(
    team: &Team,
    git: &Git,
    mode: DocumentsMode,
) -> Result<DocumentSource, GitError> {
    if mode.reads_disk() {
        Ok(DocumentSource::Disk(git.root().to_path_buf()))
    } else {
        integration_branch(team, git).map(DocumentSource::Branch)
    }
}

/// The mode the team's documents are in: the owner's choice in the team file, and the newest switch
/// in the log.
///
/// # Errors
///
/// What the log refuses.
pub fn documents_mode_of(team: &Team, log: &EventLog) -> Result<DocumentsMode, StoreError> {
    let docs = folder_docs::folder_docs(log)?;
    let newest = folder_docs::switch_state(&docs, log)?;
    Ok(catervas_core::folders::documents_mode(
        team.keeps_documents_local(),
        newest,
    ))
}

/// Whether the team's documents are read from the disk now (`local`, or on their way back), so
/// that a task works on a copy of them and lands it at acceptance.
///
/// # Errors
///
/// What the log refuses.
pub fn documents_on_disk(team: &Team, log: &EventLog) -> Result<bool, StoreError> {
    Ok(documents_mode_of(team, log)?.reads_disk())
}

/// Whether the task has no branch: it changes documents only while they stay on this computer, so
/// it has nothing to commit and its acceptance has nothing to integrate.
///
/// # Errors
///
/// What the log refuses.
pub fn is_branchless(
    team: &Team,
    log: &EventLog,
    contract: &catervas_core::contract::TaskContract,
) -> Result<bool, StoreError> {
    Ok(catervas_core::folders::is_documents_only(contract) && documents_on_disk(team, log)?)
}

/// The agent versions behind the document for people beside them, as `mode` reads them: by the
/// commits on the integration branch, or by the log of local writes. A branch that cannot be read
/// has none.
#[must_use]
pub fn stale_twins_now(git: &Git, log: &EventLog, team: &Team, mode: DocumentsMode) -> Vec<String> {
    if mode.reads_disk() {
        return folder_docs::local_stale_twins(log, git.root()).unwrap_or_default();
    }
    integration_branch(team, git)
        .and_then(|branch| crate::git::stale_twins(git, &branch))
        .unwrap_or_default()
}
