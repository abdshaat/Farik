//! The copy of a private folder taken when a task was assigned (`docs/SPEC.md` 6.6), which is what
//! the task's reviewer reads the task's changes against.

use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

use farik_core::contract::TaskId;

use crate::StoreError;

/// Where private folder `folder` keeps the copies of the folder's earlier states, one directory
/// each task and one file each version of a workbook the tools replaced.
const HISTORY: &str = ".history";

fn failed(error: &io::Error, path: &Path) -> StoreError {
    StoreError::Io {
        detail: format!("{}: {error}", path.display()),
    }
}

/// A directory made for its owner alone, with its parents, when it is not there.
///
/// # Errors
///
/// `Io` when the directory cannot be made.
pub fn make_private_directory(path: &Path) -> Result<(), StoreError> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|error| failed(&error, path))
}

/// Where the copy of `folder` taken for `task` lies: `<folder>/.history/<task>`.
#[must_use]
pub fn baseline_of(folder: &Path, task: &TaskId) -> PathBuf {
    folder.join(HISTORY).join(task.as_str())
}

/// Copies every regular file of `folder`, `.history` left out and links not followed, to
/// `<folder>/.history/<task>/`, in the same layout, for its owner alone. It copies once: when
/// that directory is there already, as it is for a task sent back and assigned again, nothing is
/// copied and the first copy stays what the task is judged against. A folder that is not there is
/// made, and the copy is of nothing. Answers whether it copied.
///
/// # Errors
///
/// `Io` when `.history` is a link, when the folder cannot be read or when the copy cannot be
/// written; a copy that stopped half way is removed, so that the next try starts over.
pub fn copy_baseline(folder: &Path, task: &TaskId) -> Result<bool, StoreError> {
    let copy = baseline_of(folder, task);
    let history = folder.join(HISTORY);
    if fs::symlink_metadata(&history).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(StoreError::Io {
            detail: format!(
                "{} is a link, and the copy of a folder is kept only in the folder",
                history.display()
            ),
        });
    }
    if copy.exists() {
        return Ok(false);
    }
    make_private_directory(folder)?;
    make_private_directory(&copy)?;
    if let Err(error) = copy_directory(folder, &copy, true) {
        let _ = fs::remove_dir_all(&copy);
        return Err(error);
    }
    Ok(true)
}

/// `from`'s regular files and directories into `to`, which exists.
fn copy_directory(from: &Path, to: &Path, at_the_top: bool) -> Result<(), StoreError> {
    for entry in fs::read_dir(from).map_err(|error| failed(&error, from))? {
        let entry = entry.map_err(|error| failed(&error, from))?;
        let name = entry.file_name();
        if at_the_top && name == HISTORY {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| failed(&error, &path))?;
        let target = to.join(&name);
        if kind.is_dir() {
            make_private_directory(&target)?;
            copy_directory(&path, &target, false)?;
        } else if kind.is_file() {
            fs::copy(&path, &target).map_err(|error| failed(&error, &path))?;
            restrict(&target)?;
        }
    }
    Ok(())
}

/// A copied file readable by its owner alone, whatever the file it was copied from allowed.
fn restrict(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| failed(&error, path))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use farik_core::contract::TaskId;

    use super::copy_baseline;

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("farik-baseline-{}-{name}", std::process::id()))
            .join("finance");
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("2026")).expect("the folder is made");
        fs::write(folder.join("books.xlsx"), "books").expect("written");
        fs::write(folder.join("2026/pricing.xlsx"), "pricing").expect("written");
        fs::create_dir_all(folder.join(".history")).expect("the history is made");
        fs::write(
            folder.join(".history/books.xlsx.20260101T000000Z.xlsx"),
            "old",
        )
        .expect("written");
        folder
    }

    fn task(id: &str) -> TaskId {
        id.parse().expect("a task id")
    }

    fn read(path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    #[test]
    fn copies_every_file_but_the_history_once() {
        let folder = a_folder("copy");
        symlink("/etc/hostname", folder.join("link.xlsx")).expect("a link");
        assert!(copy_baseline(&folder, &task("FRK-1")).expect("copied"));
        let copy = folder.join(".history/FRK-1");
        assert_eq!(read(&copy.join("books.xlsx")).as_deref(), Some("books"));
        assert_eq!(
            read(&copy.join("2026/pricing.xlsx")).as_deref(),
            Some("pricing")
        );
        // The history is not copied into itself, and a link is not followed.
        assert!(!copy.join(".history").exists());
        assert!(!copy.join("link.xlsx").exists());
        // The folder is as it was, and a second copy is not taken over the first.
        assert_eq!(read(&folder.join("books.xlsx")).as_deref(), Some("books"));
        fs::write(folder.join("books.xlsx"), "edited").expect("written");
        assert!(!copy_baseline(&folder, &task("FRK-1")).expect("kept"));
        assert_eq!(read(&copy.join("books.xlsx")).as_deref(), Some("books"));
        // Another task has its own copy, taken as the folder is then.
        assert!(copy_baseline(&folder, &task("FRK-2")).expect("copied"));
        assert_eq!(
            read(&folder.join(".history/FRK-2/books.xlsx")).as_deref(),
            Some("edited")
        );
    }

    #[test]
    fn copies_an_empty_or_missing_folder_as_an_empty_copy() {
        let folder = a_folder("empty");
        fs::remove_dir_all(&folder).expect("removed");
        assert!(copy_baseline(&folder, &task("FRK-1")).expect("copied"));
        assert!(folder.join(".history/FRK-1").is_dir());
    }

    #[test]
    fn refuses_a_history_that_is_a_link() {
        // A `.history` that is a link would have the copy written where the link points.
        let folder = a_folder("linked");
        let elsewhere = folder.parent().expect("a parent").join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("made");
        fs::remove_dir_all(folder.join(".history")).expect("removed");
        symlink(&elsewhere, folder.join(".history")).expect("a link");
        let error = copy_baseline(&folder, &task("FRK-1")).expect_err("refused");
        assert!(error.to_string().contains("is a link"), "{error}");
        assert_eq!(fs::read_dir(&elsewhere).expect("read").count(), 0);
    }

    #[test]
    fn makes_the_copy_for_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = a_folder("private");
        copy_baseline(&folder, &task("FRK-1")).expect("copied");
        let mode = |path: &Path| fs::metadata(path).expect("there").permissions().mode() & 0o777;
        assert_eq!(mode(&folder.join(".history/FRK-1")), 0o700);
        assert_eq!(mode(&folder.join(".history/FRK-1/2026")), 0o700);
        assert_eq!(mode(&folder.join(".history/FRK-1/books.xlsx")) & 0o077, 0);
    }
}
