//! The copy of a private folder taken when a task was assigned (`docs/SPEC.md` 6.6), which is what
//! the task's reviewer reads the task's changes against.

use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

use catervas_core::contract::TaskId;

use crate::StoreError;

/// Where private folder `folder` keeps the copies of the folder's earlier states, one directory
/// each task and one file each version of a workbook the tools replaced.
const HISTORY: &str = ".history";

/// What Catervas writes in the Procurement Specialist's folder, and not the task: its messages to
/// sellers and their replies (step 10f). The copy and the changes leave it out as they leave
/// `.history` out, so a reply that arrives while a task runs is no change the task made.
const MAIL: &str = "mail";

/// The names at the top of `folder` the copy and the changes leave out: `.history`, and in the
/// Procurement Specialist's folder `mail/`.
fn left_out(folder: &Path) -> Vec<&'static str> {
    let procurement = folder.file_name().is_some_and(|name| name == "procurement");
    if procurement {
        vec![HISTORY, MAIL]
    } else {
        vec![HISTORY]
    }
}

fn is_left_out(name: &str, left_out: &[&str]) -> bool {
    left_out
        .iter()
        .any(|skipped| name.eq_ignore_ascii_case(skipped))
}

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

/// The private folder `folder` of the project at `root`, `folder` being as the roles name it
/// (`.catervas/local/finance`, its parts joined by `/`). It is refused when any part from `root` down
/// is a link, `.catervas` and `.catervas/local` included, since a folder reached through one is
/// somewhere else, and what is read or written there is not the folder's. A part that is not
/// there is no link.
///
/// # Errors
///
/// `Io` when a part is a link or cannot be looked at.
pub fn folder_in(root: &Path, folder: &str) -> Result<PathBuf, StoreError> {
    let mut at = root.to_path_buf();
    for part in folder.split('/') {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Io {
                    detail: format!(
                        "{} is a link, and a private folder is not reached through one",
                        at.display()
                    ),
                });
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Err(failed(&error, &at));
            }
            _ => {}
        }
    }
    Ok(at)
}

/// Where the copy of `folder` taken for `task` lies: `<folder>/.history/<task>`.
#[must_use]
pub fn baseline_of(folder: &Path, task: &TaskId) -> PathBuf {
    folder.join(HISTORY).join(task.as_str())
}

/// Refuses `folder`, its `.history` and the copy taken for `task` when any of them is a link:
/// a copy written or read through one would be somewhere else, and the names found there would
/// reach a reviewer as the folder's own.
fn refuse_links(folder: &Path, task: &TaskId) -> Result<(), StoreError> {
    let copy = baseline_of(folder, task);
    for path in [folder.to_path_buf(), folder.join(HISTORY), copy] {
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(StoreError::Io {
                detail: format!(
                    "{} is a link, and a private folder and its copy are read and written only in the folder",
                    path.display()
                ),
            });
        }
    }
    Ok(())
}

/// Copies every regular file of `folder`, `.history` left out and links not followed, to
/// `<folder>/.history/<task>/`, in the same layout, for its owner alone. It copies once: when
/// that directory is there already, as it is for a task sent back and assigned again, nothing is
/// copied and the first copy stays what the task is judged against. A folder that is not there is
/// made, and the copy is of nothing. Answers whether it copied.
///
/// # Errors
///
/// `Io` when the folder, `.history` or the copy is a link, when the folder cannot be read or when
/// the copy cannot be written; a copy that stopped half way is removed, so that the next try
/// starts over.
pub fn copy_baseline(folder: &Path, task: &TaskId) -> Result<bool, StoreError> {
    refuse_links(folder, task)?;
    let copy = baseline_of(folder, task);
    if copy.exists() {
        return Ok(false);
    }
    make_private_directory(folder)?;
    make_private_directory(&copy)?;
    if let Err(error) = copy_directory(folder, &copy, &left_out(folder)) {
        let _ = fs::remove_dir_all(&copy);
        return Err(error);
    }
    Ok(true)
}

/// How a file of a private folder differs from the copy taken when a task was assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderChangeKind {
    /// The folder has the file and the copy has not.
    New,
    /// Both have it and its bytes differ.
    Changed,
    /// The copy has the file and the folder has not.
    Removed,
}

impl FolderChangeKind {
    /// The kind as a word a person reads: `new`, `changed` or `removed`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Changed => "changed",
            Self::Removed => "removed",
        }
    }
}

/// One file of a private folder that differs from the copy of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChange {
    /// The file's path in the folder, its parts joined by `/`.
    pub path: String,
    /// How it differs.
    pub kind: FolderChangeKind,
}

/// The files of `folder` that are new to, differ from, or are gone from the copy taken for `task`,
/// by path. Bytes are compared, not times, so a file written again with what it held is not a
/// change. `.history` is left out, links are not followed, and a task with no copy has every file
/// of the folder new to it. The paths are in order.
///
/// # Errors
///
/// `Io` when the folder, `.history` or the copy is a link, or when the folder or the copy cannot
/// be read.
pub fn changes_since_baseline(
    folder: &Path,
    task: &TaskId,
) -> Result<Vec<FolderChange>, StoreError> {
    refuse_links(folder, task)?;
    let now = files_under(folder, &left_out(folder))?;
    let before = files_under(&baseline_of(folder, task), &[])?;
    let mut changes = Vec::new();
    for (path, file) in &now {
        let kind = match before.get(path) {
            None => FolderChangeKind::New,
            Some(copy) if differ(file, copy)? => FolderChangeKind::Changed,
            Some(_) => continue,
        };
        changes.push(FolderChange {
            path: path.clone(),
            kind,
        });
    }
    for path in before.keys().filter(|path| !now.contains_key(*path)) {
        changes.push(FolderChange {
            path: path.clone(),
            kind: FolderChangeKind::Removed,
        });
    }
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(changes)
}

/// Whether two files hold different bytes.
fn differ(left: &Path, right: &Path) -> Result<bool, StoreError> {
    let length = |path: &Path| {
        fs::metadata(path)
            .map(|metadata| metadata.len())
            .map_err(|error| failed(&error, path))
    };
    if length(left)? != length(right)? {
        return Ok(true);
    }
    let read = |path: &Path| fs::read(path).map_err(|error| failed(&error, path));
    Ok(read(left)? != read(right)?)
}

/// Every regular file under `root`, by its path from `root` joined by `/`; none when `root` is not
/// there. `.history` is left out at the top when `without_history` says so; links are not followed.
fn files_under(
    root: &Path,
    left_out: &[&str],
) -> Result<std::collections::BTreeMap<String, PathBuf>, StoreError> {
    let mut found = std::collections::BTreeMap::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound && directory == root => continue,
            Err(error) => return Err(failed(&error, &directory)),
        };
        for entry in entries {
            let entry = entry.map_err(|error| failed(&error, &directory))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if prefix.is_empty() && is_left_out(&name, left_out) {
                continue;
            }
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| failed(&error, &path))?;
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                pending.push((path, relative));
            } else if kind.is_file() {
                found.insert(relative, path);
            }
        }
    }
    Ok(found)
}

/// `from`'s regular files and directories into `to`, which exists.
fn copy_directory(from: &Path, to: &Path, left_out: &[&str]) -> Result<(), StoreError> {
    for entry in fs::read_dir(from).map_err(|error| failed(&error, from))? {
        let entry = entry.map_err(|error| failed(&error, from))?;
        let name = entry.file_name();
        if is_left_out(&name.to_string_lossy(), left_out) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| failed(&error, &path))?;
        let target = to.join(&name);
        if kind.is_dir() {
            make_private_directory(&target)?;
            copy_directory(&path, &target, &[])?;
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

    use catervas_core::contract::TaskId;

    use super::{changes_since_baseline, copy_baseline, folder_in};

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("catervas-baseline-{}-{name}", std::process::id()))
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

    /// A procurement folder: the register, and `mail/` with a draft and a reply in it.
    fn a_procurement_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("catervas-baseline-{}-{name}", std::process::id()))
            .join("procurement");
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("mail/out")).expect("the folder is made");
        fs::create_dir_all(folder.join("mail/in/2026-10/1")).expect("the folder is made");
        fs::write(folder.join("vendors.xlsx"), "vendors").expect("written");
        fs::write(folder.join("mail/out/1.txt"), "draft").expect("written");
        fs::write(folder.join("mail/in/2026-10/1/text.txt"), "reply").expect("written");
        folder
    }

    #[test]
    fn leaves_the_mail_out_of_a_procurement_folder_s_copy_and_changes() {
        let folder = a_procurement_folder("mail");
        assert!(copy_baseline(&folder, &task("FRK-1")).expect("copied"));
        let copy = folder.join(".history/FRK-1");
        assert_eq!(read(&copy.join("vendors.xlsx")).as_deref(), Some("vendors"));
        assert!(
            !copy.join("mail").exists(),
            "Catervas writes mail/, not the task"
        );
        // A reply written after the copy, a draft written and one sent, are no change of the task.
        fs::write(folder.join("mail/in/2026-10/2.txt"), "later").expect("written");
        fs::write(folder.join("mail/out/1.sent.txt"), "sent").expect("written");
        fs::remove_file(folder.join("mail/out/1.txt")).expect("removed");
        assert_eq!(
            changes_since_baseline(&folder, &task("FRK-1")),
            Ok(Vec::new())
        );
        // However the folder is spelled on a disk that ignores case.
        fs::create_dir_all(folder.join("Mail")).expect("made");
        fs::write(folder.join("Mail/2.txt"), "spelled otherwise").expect("written");
        assert_eq!(
            changes_since_baseline(&folder, &task("FRK-1")),
            Ok(Vec::new())
        );
        // The register still counts, and a task with no copy has everything but mail/ new.
        fs::write(folder.join("vendors.xlsx"), "VENDORS").expect("written");
        let changes = changes_since_baseline(&folder, &task("FRK-1")).expect("changes");
        assert_eq!(
            changes
                .iter()
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>(),
            ["vendors.xlsx"]
        );
        let all = changes_since_baseline(&folder, &task("FRK-2")).expect("changes");
        assert_eq!(
            all.iter()
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>(),
            ["vendors.xlsx"]
        );
        // Another role's folder may have a `mail/` of its own, which is its task's work.
        let books = a_folder("mail-finance");
        fs::create_dir_all(books.join("mail")).expect("made");
        fs::write(books.join("mail/books.xlsx"), "kept").expect("written");
        copy_baseline(&books, &task("FRK-1")).expect("copied");
        assert_eq!(
            read(&books.join(".history/FRK-1/mail/books.xlsx")).as_deref(),
            Some("kept")
        );
    }

    #[test]
    fn finds_what_changed_since_the_copy() {
        let folder = a_folder("changes");
        copy_baseline(&folder, &task("FRK-1")).expect("copied");
        // Nothing has changed yet, however the files were touched.
        fs::write(folder.join("books.xlsx"), "books").expect("written again, the same");
        assert_eq!(
            changes_since_baseline(&folder, &task("FRK-1")),
            Ok(Vec::new())
        );
        // The same length is a change too: the bytes are compared, not only the sizes.
        fs::write(folder.join("books.xlsx"), "BOOKS").expect("written, as long as it was");
        assert_eq!(
            changes_since_baseline(&folder, &task("FRK-1"))
                .expect("compared")
                .iter()
                .map(|change| (change.path.as_str(), change.kind.word()))
                .collect::<Vec<_>>(),
            [("books.xlsx", "changed")]
        );
        // A file changed, one new (in a folder of its own), one gone, and the history growing.
        fs::write(folder.join("books.xlsx"), "edited").expect("written");
        fs::create_dir_all(folder.join("2027")).expect("made");
        fs::write(folder.join("2027/forecast.xlsx"), "forecast").expect("written");
        fs::remove_file(folder.join("2026/pricing.xlsx")).expect("removed");
        fs::write(
            folder.join(".history/books.xlsx.20260102T000000Z.xlsx"),
            "older",
        )
        .expect("written");
        symlink("/etc/hostname", folder.join("link.xlsx")).expect("a link");
        let changes = changes_since_baseline(&folder, &task("FRK-1")).expect("compared");
        let words: Vec<(&str, &str)> = changes
            .iter()
            .map(|change| (change.path.as_str(), change.kind.word()))
            .collect();
        assert_eq!(
            words,
            [
                ("2026/pricing.xlsx", "removed"),
                ("2027/forecast.xlsx", "new"),
                ("books.xlsx", "changed"),
            ]
        );
        // A task with no copy was assigned before there were copies: everything in the folder is
        // new to it, rather than nothing.
        let all = changes_since_baseline(&folder, &task("FRK-9")).expect("compared");
        assert_eq!(
            all.iter()
                .map(|change| (change.path.as_str(), change.kind.word()))
                .collect::<Vec<_>>(),
            [("2027/forecast.xlsx", "new"), ("books.xlsx", "new")]
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
    fn finds_a_folder_reached_through_no_link() {
        let root = std::env::temp_dir().join(format!("catervas-folder-in-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("made");
        // Not made yet: no link, so the path is answered for the caller to make.
        assert_eq!(
            folder_in(&root, ".catervas/local/finance").expect("found"),
            root.join(".catervas/local/finance")
        );
        fs::create_dir_all(root.join(".catervas/local/finance")).expect("made");
        assert!(folder_in(&root, ".catervas/local/finance").is_ok());
        // A link at any level, from the project's root down, is refused.
        let elsewhere = root.join("elsewhere");
        fs::create_dir_all(elsewhere.join("local/finance")).expect("made");
        for (linked, target) in [
            (".catervas", elsewhere.clone()),
            (".catervas/local", elsewhere.join("local")),
            (".catervas/local/finance", elsewhere.join("local/finance")),
        ] {
            let held = root.join("held");
            let _ = fs::remove_dir_all(&held);
            fs::create_dir_all(&held).expect("made");
            let copy = held.join("project");
            fs::create_dir_all(&copy).expect("made");
            fs::create_dir_all(copy.join(linked).parent().expect("a parent")).expect("made");
            symlink(&target, copy.join(linked)).expect("a link");
            let error = folder_in(&copy, ".catervas/local/finance").expect_err("refused");
            assert!(error.to_string().contains("is a link"), "{linked}: {error}");
        }
    }

    #[test]
    fn refuses_a_task_copy_that_is_a_link() {
        // A link at `.history/<task>` would have the copy's walk list, and the review message and
        // `task.diff` name, what lies where it points, and a second copy skipped as taken.
        let folder = a_folder("linked-copy");
        let elsewhere = folder.parent().expect("a parent").join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("made");
        fs::write(elsewhere.join("outside-secret.xlsx"), "outside").expect("written");
        symlink(&elsewhere, folder.join(".history/FRK-1")).expect("a link");
        let copy = copy_baseline(&folder, &task("FRK-1")).expect_err("refused");
        assert!(copy.to_string().contains("is a link"), "{copy}");
        let changes = changes_since_baseline(&folder, &task("FRK-1")).expect_err("refused");
        assert!(changes.to_string().contains("is a link"), "{changes}");
        assert!(!changes.to_string().contains("outside-secret"), "{changes}");
        // Nothing was written where the link points.
        assert_eq!(fs::read_dir(&elsewhere).expect("read").count(), 1);
    }

    #[test]
    fn refuses_a_folder_that_is_a_link() {
        let folder = a_folder("linked-folder");
        let elsewhere = folder.parent().expect("a parent").join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("made");
        fs::write(elsewhere.join("outside-secret.xlsx"), "outside").expect("written");
        let linked = folder.parent().expect("a parent").join("linked");
        symlink(&elsewhere, &linked).expect("a link");
        let copy = copy_baseline(&linked, &task("FRK-1")).expect_err("refused");
        assert!(copy.to_string().contains("is a link"), "{copy}");
        let changes = changes_since_baseline(&linked, &task("FRK-1")).expect_err("refused");
        assert!(changes.to_string().contains("is a link"), "{changes}");
        assert!(!changes.to_string().contains("outside-secret"), "{changes}");
        assert!(!elsewhere.join(".history").exists());
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
