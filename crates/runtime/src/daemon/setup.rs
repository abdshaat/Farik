//! Setup mode (`docs/SPEC.md` section 4.1): the daemon `catervas serve` runs before there is a
//! project, answering the first-run wizard through a host the CLI gives it, since runtime cannot
//! call the CLI's `init` or request filing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Why the host did not do what the wizard asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// A refusal, in the sentence the page shows.
    Refused(String),
    /// Something broke.
    Failed(String),
}

/// A member repository the wizard asks a project folder to hold: its name, its folder relative to
/// home, and the branch accepted work goes into when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRequest {
    /// What the project calls it.
    pub name: String,
    /// Its top folder, relative to home.
    pub path: String,
    /// The branch accepted work goes into, or none for the repository's default branch.
    pub integration_branch: Option<String>,
}

/// What the CLI does for the wizard: take a project on, and keep the credential.
pub trait SetupHost: Send + Sync {
    /// Opens the git project at `path`, relative to home, for the team. While a project is being
    /// left, a folder that already has a team is refused unless `replace`, which deletes its
    /// `.catervas/` (ADR 0053).
    ///
    /// # Errors
    ///
    /// `Refused` with the sentence to show; `Failed` when something broke.
    fn open(&self, path: &str, no_sandbox: bool, replace: bool) -> Result<PathBuf, SetupError>;
    /// Makes the project `parent/name`, relative to home, from `description`.
    ///
    /// # Errors
    ///
    /// `Refused` with the sentence to show; `Failed` when something broke.
    fn create(
        &self,
        parent: &str,
        name: &str,
        description: &str,
        no_sandbox: bool,
    ) -> Result<PathBuf, SetupError>;
    /// Makes the project folder `parent/name`, relative to home, of the member `repos`: a local
    /// git repository with no remote, whose `.catervas/repos.yaml` lists them (ADR 0057). `parent`
    /// need not exist yet.
    ///
    /// # Errors
    ///
    /// `Refused` with the sentence to show; `Failed` when something broke.
    fn create_folder(
        &self,
        parent: &str,
        name: &str,
        repos: &[MemberRequest],
        no_sandbox: bool,
    ) -> Result<PathBuf, SetupError>;
    /// Takes on the project that waited on an AI account, now that Claude Code works, and
    /// answers whether there was one.
    ///
    /// # Errors
    ///
    /// `Failed` when something broke.
    fn take_on_waiting(&self) -> Result<bool, SetupError>;
    /// Keeps the limits the computer's teams share where the next `catervas serve` reads them
    /// (ADR 0059): `Some(Some(n))` sets one, `Some(None)` clears it, `None` leaves it as it is.
    ///
    /// # Errors
    ///
    /// `Failed` when they cannot be written.
    fn set_limits(
        &self,
        agents_at_once: Option<Option<usize>>,
        daily_cap_usd: Option<Option<f64>>,
    ) -> Result<(), SetupError> {
        let _ = (agents_at_once, daily_cap_usd);
        Ok(())
    }
    /// The sandbox image is ready: takes on the projects that waited on the sandbox, if any did,
    /// and answers whether it did.
    fn sandbox_ready(&self) -> bool {
        false
    }
    /// The user's home folder, which the folder browser stays inside.
    fn home(&self) -> PathBuf;
    /// The environment the computer's programs are looked for and run in.
    fn env(&self) -> BTreeMap<String, String>;
}

/// What `folders.list` refuses a path outside home with.
pub(super) const OUTSIDE_HOME: &str = "that folder is outside your home folder";

/// How many folders one listing holds at most.
const MAX_ENTRIES: usize = 500;

/// `folders.list { path }`: the folders in `path`, relative to `home`, and which are git projects.
/// Hidden folders, files, and links that lead out of home are left out. Home and each folder are
/// compared once their links are followed, so that `/var` and `/private/var` on macOS agree.
///
/// # Errors
///
/// The sentence to show: the folder is outside home, is not there, or cannot be read.
pub(super) fn list_folders(home: &Path, path: &str) -> Result<Value, String> {
    let home = std::fs::canonicalize(home)
        .map_err(|error| format!("your home folder cannot be read: {error}"))?;
    let folder = std::fs::canonicalize(home.join(path))
        .map_err(|_| "that folder is not there".to_string())?;
    let relative = folder
        .strip_prefix(&home)
        .map_err(|_| OUTSIDE_HOME.to_string())?;
    let parent = relative.parent().map(|parent| parent.display().to_string());
    let mut entries: Vec<(String, bool)> = std::fs::read_dir(&folder)
        .map_err(|error| format!("that folder cannot be read: {error}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let target = std::fs::canonicalize(entry.path()).ok()?;
            (!name.starts_with('.') && target.is_dir() && target.starts_with(&home))
                .then(|| (name, target.join(".git").exists()))
        })
        .collect();
    entries.sort_by_key(|(name, _)| name.to_lowercase());
    // ponytail: a folder of more than 500 folders shows its first 500; paging, if anyone has one.
    entries.truncate(MAX_ENTRIES);
    Ok(json!({
        "path": relative.display().to_string(),
        "parent": parent,
        "entries": entries
            .into_iter()
            .map(|(name, git)| json!({ "name": name, "git": git }))
            .collect::<Vec<_>>(),
    }))
}
