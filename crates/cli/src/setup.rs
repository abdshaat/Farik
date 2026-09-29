//! What `farik serve` does for the first-run wizard before there is a project (`docs/SPEC.md`
//! 4.1): the CLI's side of runtime's `SetupHost`, which opens or makes the project, keeps the
//! credential, and says on a watch which project the wizard chose.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use farik_protocol::clock::Clock;
use farik_protocol::event::EventBody;
use farik_runtime::claude::CredentialKind;
use farik_runtime::computer::on_path;
use farik_runtime::credential::{
    CredentialError, CredentialStore, Source, credential_of_kind, load_credential, save_credential,
};
use farik_runtime::daemon::{SETUP_PENDING, SetupError, SetupHost};
use farik_store::files::{LocalSettings, Sandbox};
use farik_store::requests::{
    RequestError, file_request, placeholder_budget_usd, request_from_brief,
};
use tokio::sync::watch;

use crate::project::{Project, open_project, repository_root};
use crate::start::try_lock;
use crate::state::{make_state_dir, state_dir};
use crate::{HUMAN, init};

/// What `open` and `create` refuse a project another process drives with.
const BUSY: &str = "another farik is already running this project";
/// What `open` and `create` refuse with before a credential is kept.
const NO_ACCOUNT: &str = "connect your AI account first";
/// What `open` refuses a folder outside home with.
const OUTSIDE_HOME: &str = "that folder is outside your home folder";

/// The CLI's setup host.
pub(crate) struct CliHost {
    /// The environment `farik serve` was given.
    pub(crate) env: BTreeMap<String, String>,
    /// Home: `HOME`, else where `farik serve` was run.
    pub(crate) home: PathBuf,
    /// The time the events it records are stamped with.
    pub(crate) clock: Arc<dyn Clock + Send + Sync>,
    /// Where the credential is kept.
    pub(crate) stores: Vec<Arc<dyn CredentialStore>>,
    /// Where the chosen project is said.
    pub(crate) chosen: watch::Sender<Option<PathBuf>>,
    /// A project already chosen, which has no credential yet: `connect` takes it on.
    pub(crate) waiting: Option<PathBuf>,
}

impl CliHost {
    /// `path`, relative to home, followed through its links, or the refusal of one outside home.
    fn inside_home(&self, path: &str) -> Result<PathBuf, SetupError> {
        let home = std::fs::canonicalize(&self.home)
            .map_err(|error| failed(format!("your home folder cannot be read: {error}")))?;
        let folder = std::fs::canonicalize(home.join(path))
            .map_err(|_| refused("that folder is not there"))?;
        if folder.starts_with(&home) {
            Ok(folder)
        } else {
            Err(refused(OUTSIDE_HOME))
        }
    }

    /// Refuses unless a credential is kept.
    fn has_account(&self) -> Result<(), SetupError> {
        match load_credential(&self.env, &self.stores) {
            Some(_) => Ok(()),
            None => Err(refused(NO_ACCOUNT)),
        }
    }

    /// Makes `root` a Farik project when it is not one: `init`, the team paused, and the marker.
    /// Answers the project, and whether it was made.
    fn taken_on(&self, root: &Path, no_sandbox: bool) -> Result<(Project, bool), SetupError> {
        let now = self.clock.now();
        let made = !root.join(".farik/team.yaml").exists();
        if made {
            init::init(root, now).map_err(failed)?;
        }
        let project = open_project(root, now).map_err(failed)?;
        if made {
            let by = serde_json::from_value(serde_json::json!({ "by": HUMAN }))
                .map_err(|error| failed(error.to_string()))?;
            let event = project
                .event(EventBody::TeamPaused(by), now, None)
                .map_err(failed)?;
            project.append(&event).map_err(failed)?;
            std::fs::write(root.join(SETUP_PENDING), "")
                .map_err(|error| failed(format!("{SETUP_PENDING} cannot be written: {error}")))?;
        }
        if no_sandbox {
            project
                .files
                .write_settings(&LocalSettings {
                    sandbox: Sandbox::None,
                })
                .map_err(|error| failed(error.to_string()))?;
        }
        Ok((project, made))
    }

    /// Says `root` is the chosen project, and answers it.
    fn choose(&self, root: PathBuf) -> PathBuf {
        self.chosen.send_replace(Some(root.clone()));
        root
    }

    /// Runs `git <args>` in `directory`, as farik, with `git` from the environment's `PATH`.
    fn git(&self, directory: &Path, args: &[&str]) -> Result<(), SetupError> {
        let program = on_path("git", &self.env).ok_or_else(|| refused("git is not installed"))?;
        let output = std::process::Command::new(program)
            .args([
                "-c",
                "user.name=farik",
                "-c",
                "user.email=farik@localhost",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(directory)
            .env_clear()
            .envs(&self.env)
            .output()
            .map_err(|error| failed(format!("git could not be run: {error}")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failed(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl SetupHost for CliHost {
    fn open(&self, path: &str, no_sandbox: bool) -> Result<PathBuf, SetupError> {
        let folder = self.inside_home(path)?;
        let root = repository_root(&folder).map_err(|_| {
            refused("that folder is not a git project; choose another, or start a new project")
        })?;
        let root = std::fs::canonicalize(&root).map_err(|error| failed(error.to_string()))?;
        if root != folder {
            return Err(refused(&format!(
                "that folder is inside a git project; choose {} instead",
                root.display()
            )));
        }
        // Checked before anything is changed; the lock is given back for the driver to take.
        match try_lock(&root).map_err(failed)? {
            Some(lock) => drop(lock),
            None => return Err(refused(BUSY)),
        }
        self.has_account()?;
        self.taken_on(&root, no_sandbox)?;
        Ok(self.choose(root))
    }

    fn create(
        &self,
        parent: &str,
        name: &str,
        description: &str,
        no_sandbox: bool,
    ) -> Result<PathBuf, SetupError> {
        let named = (1..=64).contains(&name.len())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !named {
            return Err(refused(
                "a project's name is lowercase letters, digits, and -, up to 64 characters",
            ));
        }
        let length = description.trim().chars().count();
        if length < 20 {
            return Err(refused(
                "say a little more about the project: at least 20 characters",
            ));
        }
        if length > 2000 {
            return Err(refused(
                "that description is too long: at most 2000 characters",
            ));
        }
        // The request is built once before anything is made, so that one it refuses makes nothing.
        request_from_brief(name, description, 0.0).map_err(|why| refused(&why))?;
        let parent = self.inside_home(parent)?;
        let root = parent.join(name);
        if root.exists() {
            return Err(refused("a folder with that name is already there"));
        }
        self.has_account()?;
        on_path("git", &self.env).ok_or_else(|| refused("git is not installed"))?;

        std::fs::create_dir(&root)
            .map_err(|error| failed(format!("{} cannot be made: {error}", root.display())))?;
        self.git(&root, &["init", "-b", "main"])?;
        std::fs::write(
            root.join("README.md"),
            format!("# {name}\n\n{description}\n"),
        )
        .map_err(|error| failed(format!("README.md cannot be written: {error}")))?;
        self.git(&root, &["add", "README.md"])?;
        self.git(&root, &["commit", "-m", "Start the project"])?;

        let (project, _) = self.taken_on(&root, no_sandbox)?;
        let request = request_from_brief(
            name,
            description,
            placeholder_budget_usd(&project.team.rules()),
        )
        .map_err(|why| refused(&why))?;
        file_request(
            &project.files,
            &project.log,
            request,
            HUMAN,
            None,
            self.clock.now(),
            &crate::contract::event_ids(&project),
        )
        .map_err(|error| match error {
            RequestError::Refused { reason } => failed(format!("the request {reason}")),
            other => failed(other.to_string()),
        })?;
        Ok(self.choose(root))
    }

    fn connect(&self, kind: CredentialKind, secret: &str) -> Result<(Source, bool), SetupError> {
        let credential = credential_of_kind(kind, secret).map_err(|why| refused(&why))?;
        if let Some(directory) = state_dir(&self.env) {
            make_state_dir(&directory).map_err(failed)?;
        }
        let source = save_credential(&credential, &self.stores).map_err(|error| match error {
            CredentialError::Failed(why) => refused(&why),
            CredentialError::NoKeychain => {
                refused("this computer has no keychain and no folder to keep the key in")
            }
        })?;
        if let Some(root) = &self.waiting {
            self.choose(root.clone());
        }
        Ok((source, self.waiting.is_some()))
    }

    fn home(&self) -> PathBuf {
        self.home.clone()
    }

    fn env(&self) -> BTreeMap<String, String> {
        self.env.clone()
    }

    fn account(&self) -> Option<(CredentialKind, Source)> {
        load_credential(&self.env, &self.stores)
            .map(|(credential, source)| (credential.kind(), source))
    }
}

fn refused(sentence: &str) -> SetupError {
    SetupError::Refused(sentence.to_string())
}

fn failed(why: String) -> SetupError {
    SetupError::Failed(why)
}
