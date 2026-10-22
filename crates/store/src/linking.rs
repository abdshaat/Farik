//! The committed files that proofs of linking come from, and what the owner answered about each
//! pair of projects (`docs/SPEC.md` 4.4, 8.4; ADR 0059).

use catervas_core::linking::{MAX_FILE_BYTES, MAX_FILES, is_read_for_proof};

use crate::git::{Git, GitError};

/// The files of the repository at `git` that proofs are read from, as `(path, text)`, at `HEAD`
/// through `git show` and never from the working tree. A file over 256 KiB is skipped, no more
/// than 300 are read (the shallowest first), and nothing under `node_modules`, `vendor`, `target`
/// or `.git` is. A folder that is no git repository, or has no commit yet, gives none.
///
/// # Errors
///
/// `NotInstalled` when the `git` program cannot be run.
pub fn committed_files(git: &Git) -> Result<Vec<(String, String)>, GitError> {
    let sizes = match git.tree_sizes("HEAD") {
        Ok(sizes) => sizes,
        Err(GitError::NotARepository | GitError::CommandFailed { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut wanted: Vec<String> = sizes
        .into_iter()
        .filter(|(path, size)| *size <= MAX_FILE_BYTES && is_read_for_proof(path))
        .map(|(path, _)| path)
        .collect();
    wanted.sort_by(|a, b| {
        let depth = |path: &str| path.matches('/').count();
        depth(a).cmp(&depth(b)).then_with(|| a.cmp(b))
    });
    wanted.truncate(MAX_FILES);
    let mut files = Vec::new();
    for path in wanted {
        match git.file_at("HEAD", &path) {
            Ok(text) => files.push((path, text)),
            Err(GitError::CommandFailed { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(files)
}
