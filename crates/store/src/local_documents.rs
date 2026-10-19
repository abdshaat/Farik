//! The team's documents kept on this computer (ADR 0074, `docs/SPEC.md` 5.17): the folder
//! `docs/catervas/` in the project's checkout, ignored through git's `info/exclude`. A task works
//! on a copy of it and the copy lands when the task is accepted; the owner's choice moves the
//! folder out of the repository and back. Every path is walked from its root one segment at a time
//! and refused when a segment is a link, so that nothing is read or written through one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use catervas_core::contract::TaskId;
use catervas_core::governor::paths::check_allowed_paths;

use crate::baseline::files_under;
use crate::{Git, GitError, StoreError};

/// The documents' folder, from the project's root.
pub const DOCUMENTS: &str = "docs/catervas";

/// Where the copies of the documents and what a landing set aside are kept.
const LOCAL: &str = ".catervas/local/documents";

/// The two lines Catervas adds to `info/exclude`.
const EXCLUDE_COMMENT: &str = "# Catervas keeps the team's documents on this computer";
const EXCLUDE_LINE: &str = "/docs/catervas";

fn failed(error: &io::Error, path: &Path) -> StoreError {
    StoreError::Io {
        detail: format!("{}: {error}", path.display()),
    }
}

fn from_git(error: &GitError) -> StoreError {
    StoreError::Io {
        detail: error.to_string(),
    }
}

/// Where the first copy of the documents taken for `task` lies.
fn start_of(root: &Path, task: &TaskId) -> PathBuf {
    root.join(LOCAL).join("start").join(task.as_str())
}

/// Where the versions a landing did not put in lie.
fn aside_of(root: &Path, task: &TaskId) -> PathBuf {
    root.join(LOCAL).join("set-aside").join(task.as_str())
}

/// Whether a segment of `relative`, walked from `base`, is a link. A segment that is not there is
/// no link, and the walk ends.
fn has_link(base: &Path, relative: &str) -> Result<bool, StoreError> {
    let mut at = base.to_path_buf();
    for segment in relative.split('/') {
        at.push(segment);
        match fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(failed(&error, &at)),
        }
    }
    Ok(false)
}

/// The bytes of a regular file, or none when there is none; a link or a directory is an error.
fn read_file(path: &Path) -> Result<Option<Vec<u8>>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            fs::read(path).map(Some).map_err(|e| failed(&e, path))
        }
        Ok(_) => Err(StoreError::Io {
            detail: format!("{} is not a regular file", path.display()),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(&error, path)),
    }
}

/// Writes `bytes` to `target` whole or not at all: a temporary file in the same directory, then a
/// rename. The directories on the way are made.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let directory = target.parent().ok_or_else(|| StoreError::Io {
        detail: format!("{} has no directory", target.display()),
    })?;
    fs::create_dir_all(directory).map_err(|e| failed(&e, directory))?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = directory.join(format!(".{name}.catervas-{}", std::process::id()));
    fs::write(&temporary, bytes).map_err(|e| failed(&e, &temporary))?;
    fs::rename(&temporary, target).map_err(|e| {
        let _ = fs::remove_file(&temporary);
        failed(&e, target)
    })
}

/// Copies every regular file of `source` to `target`, in the same layout.
fn copy_files(source: &Path, target: &Path) -> Result<(), StoreError> {
    for (relative, file) in files_under(source, &[])? {
        let to = target.join(&relative);
        let bytes = fs::read(&file).map_err(|e| failed(&e, &file))?;
        write_atomically(&to, &bytes)?;
    }
    Ok(())
}

/// `path` as a path under `docs/catervas`, or none when it is outside it or climbs out.
fn under_documents(path: &str) -> Option<&str> {
    let relative = path.strip_prefix(DOCUMENTS)?.strip_prefix('/')?;
    (!relative.is_empty()
        && relative
            .split('/')
            .all(|part| !part.is_empty() && part != "..")
        && !relative.split('/').any(|part| part == "."))
    .then_some(relative)
}

/// Whether the root's documents folder holds a regular file.
///
/// # Errors
///
/// `Io` when the folder cannot be read.
pub fn has_documents(root: &Path) -> Result<bool, StoreError> {
    if has_link(root, DOCUMENTS)? {
        return Ok(false);
    }
    Ok(!files_under(&root.join(DOCUMENTS), &[])?.is_empty())
}

/// Gives the task's worktree a copy of the root's documents: the worktree's folder is emptied
/// (every file git still tracks there is first marked `--skip-worktree`, so that the copy is no
/// change to it) and filled with every regular file of the root's folder, links not followed. With
/// `start`, the same files are kept as the task's start copy, once: a task sent back keeps its
/// first.
///
/// # Errors
///
/// `Io` when the folder, or `docs`, is a link at the root or in the worktree, when a file cannot be
/// copied, or when git refuses.
pub fn copy_in(
    git: &Git,
    root: &Path,
    worktree: &Path,
    start: Option<&TaskId>,
) -> Result<(), StoreError> {
    for base in [root, worktree] {
        if has_link(base, DOCUMENTS)? {
            return Err(StoreError::Io {
                detail: format!(
                    "{} is a link, and the documents are not reached through one",
                    base.join(DOCUMENTS).display()
                ),
            });
        }
    }
    git.skip_worktree(worktree, DOCUMENTS)
        .map_err(|e| from_git(&e))?;
    let target = worktree.join(DOCUMENTS);
    match fs::remove_dir_all(&target) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            return Err(failed(&error, &target));
        }
        _ => {}
    }
    let source = root.join(DOCUMENTS);
    fs::create_dir_all(&target).map_err(|e| failed(&e, &target))?;
    copy_files(&source, &target)?;
    if let Some(task) = start {
        let kept = start_of(root, task);
        if !kept.exists() {
            let result = fs::create_dir_all(&kept)
                .map_err(|e| failed(&e, &kept))
                .and_then(|()| copy_files(&source, &kept));
            if result.is_err() {
                let _ = fs::remove_dir_all(&kept);
            }
            result?;
        }
    }
    Ok(())
}

/// The documents the task changed: the files of its copy that are new to, differ from, or are gone
/// from its start copy, as repository paths, in order. A task with no start copy changed none.
///
/// # Errors
///
/// `Io` when a folder cannot be read.
pub fn document_changes(
    root: &Path,
    worktree: &Path,
    task: &TaskId,
) -> Result<Vec<String>, StoreError> {
    let start = start_of(root, task);
    if !start.is_dir() {
        return Ok(Vec::new());
    }
    let before = files_under(&start, &[])?;
    let after = files_under(&worktree.join(DOCUMENTS), &[])?;
    let mut changed = Vec::new();
    for (relative, file) in &after {
        let differs = match before.get(relative) {
            None => true,
            Some(original) => {
                fs::read(original).map_err(|e| failed(&e, original))?
                    != fs::read(file).map_err(|e| failed(&e, file))?
            }
        };
        if differs {
            changed.push(relative.clone());
        }
    }
    changed.extend(
        before
            .keys()
            .filter(|key| !after.contains_key(*key))
            .cloned(),
    );
    changed.sort();
    Ok(changed
        .into_iter()
        .map(|relative| format!("{DOCUMENTS}/{relative}"))
        .collect())
}

/// Puts back, from the start copy, every document the task changed outside `allowed` (and removes
/// one it added there), as the tidy before `verifying` does for a branch. The paths it touched.
///
/// # Errors
///
/// `Io` when a file cannot be restored.
pub fn put_back_outside(
    root: &Path,
    worktree: &Path,
    task: &TaskId,
    allowed: &[String],
) -> Result<Vec<String>, StoreError> {
    let start = start_of(root, task);
    let mut touched = Vec::new();
    for path in document_changes(root, worktree, task)? {
        if check_allowed_paths(std::slice::from_ref(&path), allowed).is_ok() {
            continue;
        }
        let Some(relative) = under_documents(&path) else {
            continue;
        };
        let target = worktree.join(&path);
        match read_file(&start.join(relative))? {
            Some(original) => write_atomically(&target, &original)?,
            None => match fs::remove_file(&target) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(failed(&error, &target));
                }
                _ => {}
            },
        }
        touched.push(path);
    }
    Ok(touched)
}

/// What a landing did, as repository paths.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Landing {
    /// The documents written to the root.
    pub written: Vec<String>,
    /// The documents removed from the root.
    pub removed: Vec<String>,
    /// The documents whose root copy changed meanwhile (or sits behind a link): the root's was
    /// kept, and the task's version was copied under `set-aside/<task>/` when it had one.
    pub set_aside: Vec<String>,
}

/// Lands the task's changed documents in the root. A file whose root copy still equals the start
/// copy is written (or removed); one whose root copy already equals the task's is left, so a
/// landing tried twice does nothing twice; any other is set aside, the root's kept and the task's
/// version copied to `.catervas/local/documents/set-aside/<task>/<path under docs/catervas>`. A
/// file behind a link is never written through. A landed `<stem>.html` that the start copy did not
/// have removes `<stem>.md` when its root copy is still the start copy's. The start copy is
/// forgotten at the end, so a second landing finds nothing to do.
///
/// # Errors
///
/// `Io` when a file cannot be read, written or removed; what was done stays done and the landing
/// can be tried again.
pub fn land(root: &Path, worktree: &Path, task: &TaskId) -> Result<Landing, StoreError> {
    let start = start_of(root, task);
    let mut landing = Landing::default();
    for path in document_changes(root, worktree, task)? {
        let Some(relative) = under_documents(&path) else {
            continue;
        };
        let theirs = read_file(&worktree.join(&path))?;
        let original = read_file(&start.join(relative))?;
        let linked = has_link(root, &path)?;
        let ours = if linked {
            None
        } else {
            read_file(&root.join(&path))?
        };
        let aside = |bytes: Option<&Vec<u8>>, landing: &mut Landing| -> Result<(), StoreError> {
            if let Some(bytes) = bytes {
                let target = aside_of(root, task).join(relative);
                if !has_link(
                    root,
                    &format!("{LOCAL}/set-aside/{}/{relative}", task.as_str()),
                )? {
                    write_atomically(&target, bytes)?;
                }
            }
            landing.set_aside.push(path.clone());
            Ok(())
        };
        if let Some(bytes) = &theirs {
            if !linked && ours.as_ref() == Some(bytes) {
                continue;
            }
            if !linked && ours == original {
                write_atomically(&root.join(&path), bytes)?;
                landing.written.push(path.clone());
                continue;
            }
            aside(Some(bytes), &mut landing)?;
        } else {
            if !linked && ours.is_none() {
                continue;
            }
            if !linked && ours == original {
                fs::remove_file(root.join(&path)).map_err(|e| failed(&e, &root.join(&path)))?;
                landing.removed.push(path.clone());
                continue;
            }
            aside(None, &mut landing)?;
        }
    }
    drop_replaced_markdown(root, &start, &mut landing)?;
    forget_start(root, task)?;
    Ok(landing)
}

/// A document that came as HTML where the team had it as Markdown replaces it (5.17): the
/// Markdown file goes when it is still what the task started from.
fn drop_replaced_markdown(
    root: &Path,
    start: &Path,
    landing: &mut Landing,
) -> Result<(), StoreError> {
    let landed: Vec<String> = landing.written.clone();
    for path in &landed {
        let Some(stem) = path.strip_suffix(".html") else {
            continue;
        };
        let Some(relative) = under_documents(&format!("{stem}.md")).map(str::to_string) else {
            continue;
        };
        if read_file(&start.join(under_documents(path).unwrap_or_default()))?.is_some() {
            continue;
        }
        let markdown = format!("{stem}.md");
        if has_link(root, &markdown)? {
            continue;
        }
        let held = read_file(&root.join(&markdown))?;
        if held.is_some() && held == read_file(&start.join(&relative))? {
            fs::remove_file(root.join(&markdown)).map_err(|e| failed(&e, &root.join(&markdown)))?;
            landing.removed.push(markdown);
        }
    }
    Ok(())
}

/// Removes the task's start copy.
///
/// # Errors
///
/// `Io` when it cannot be removed.
pub fn forget_start(root: &Path, task: &TaskId) -> Result<(), StoreError> {
    let start = start_of(root, task);
    match fs::remove_dir_all(&start) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(failed(&error, &start)),
        _ => Ok(()),
    }
}

/// Writes the document `path` (from the project's root, under `docs/catervas`) whole or not at
/// all.
///
/// # Errors
///
/// `Io` when the path is outside the folder, a segment of it is a link, or the file cannot be
/// written.
pub fn write_document(root: &Path, path: &str, text: &str) -> Result<(), StoreError> {
    if under_documents(path).is_none() {
        return Err(StoreError::Io {
            detail: format!("{path} is not a document under {DOCUMENTS}"),
        });
    }
    if has_link(root, path)? {
        return Err(StoreError::Io {
            detail: format!("{path} is, or lies behind, a link"),
        });
    }
    write_atomically(&root.join(path), text.as_bytes())
}

fn exclude_file(git: &Git) -> Result<PathBuf, StoreError> {
    Ok(git
        .common_dir(git.root())
        .map_err(|e| from_git(&e))?
        .join("info")
        .join("exclude"))
}

fn read_exclude(file: &Path) -> Result<String, StoreError> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(failed(&error, file)),
    }
}

/// Adds the two lines that make git ignore `docs/catervas`, to the repository's `info/exclude`,
/// which every worktree shares, unless they are there.
///
/// # Errors
///
/// `Io` when the file cannot be read or written.
pub fn exclude_documents(git: &Git) -> Result<(), StoreError> {
    let file = exclude_file(git)?;
    let mut text = read_exclude(&file)?;
    if text.lines().any(|line| line == EXCLUDE_LINE) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(EXCLUDE_COMMENT);
    text.push('\n');
    text.push_str(EXCLUDE_LINE);
    text.push('\n');
    write_atomically(&file, text.as_bytes())
}

/// Removes exactly the two lines `exclude_documents` added.
///
/// # Errors
///
/// `Io` when the file cannot be read or written.
pub fn include_documents(git: &Git) -> Result<(), StoreError> {
    let file = exclude_file(git)?;
    let text = read_exclude(&file)?;
    if !text
        .lines()
        .any(|line| line == EXCLUDE_LINE || line == EXCLUDE_COMMENT)
    {
        return Ok(());
    }
    let mut kept = String::new();
    for line in text
        .lines()
        .filter(|line| *line != EXCLUDE_LINE && *line != EXCLUDE_COMMENT)
    {
        kept.push_str(line);
        kept.push('\n');
    }
    write_atomically(&file, kept.as_bytes())
}

/// Writes back, from the parent of the commit `sha` that took the documents out, every regular file
/// that commit removed and the disk lacks: exactly what git deleted when it moved the checkout
/// there. A file the disk has is never written over. The paths written.
///
/// # Errors
///
/// `Io` when git cannot read the parent or a file cannot be written.
pub fn restore_taken_out(git: &Git, sha: &str) -> Result<Vec<String>, StoreError> {
    let entries = git
        .tree_entries(&format!("{sha}^"), DOCUMENTS)
        .map_err(|e| from_git(&e))?;
    let root = git.root();
    let mut restored = Vec::new();
    for entry in entries {
        if !matches!(entry.mode.as_str(), "100644" | "100755")
            || under_documents(&entry.path).is_none()
        {
            continue;
        }
        if has_link(root, &entry.path)? || fs::symlink_metadata(root.join(&entry.path)).is_ok() {
            continue;
        }
        let bytes = git.blob_bytes(&entry.blob).map_err(|e| from_git(&e))?;
        write_atomically(&root.join(&entry.path), &bytes)?;
        restored.push(entry.path);
    }
    Ok(restored)
}
