//! Where a task's commands run (`docs/SPEC.md` 8.3): a Docker container per task with its worktree
//! at `/workspace`, or, in no-sandbox mode, the host inside the worktree.

use std::fmt;
use std::path::{Path, PathBuf};

use catervas_core::contract::TaskId;

use crate::exec::Executor;

/// Commands run in a Docker container per task.
#[cfg(unix)]
pub mod docker;
/// Commands run on the host, inside the task's worktree.
#[cfg(unix)]
pub mod host;

/// The image a task's container runs, tagged with this crate's version and a digest of its
/// Dockerfile (`build.rs`), so a changed Dockerfile is a new tag and the old image counts as missing
/// until the computer check builds the new one.
pub const SANDBOX_IMAGE: &str = concat!(
    "catervas/sandbox:",
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("CATERVAS_SANDBOX_DIGEST")
);

/// Why a sandbox could not be made or discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// The `docker` program is missing or its daemon does not answer.
    DockerUnavailable,
    /// The image is not on this machine; nothing here pulls or builds it.
    ImageMissing {
        /// The image asked for.
        image: String,
    },
    /// Docker refused to start or remove the container.
    ContainerFailed {
        /// What docker said on standard error.
        detail: String,
    },
}

impl fmt::Display for SandboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DockerUnavailable => write!(
                formatter,
                "docker is not available: the program is missing or its daemon does not answer"
            ),
            Self::ImageMissing { image } => {
                write!(
                    formatter,
                    "the sandbox image {image} is not on this machine. \
                     Open the computer check in Setup and choose Prepare it"
                )
            }
            Self::ContainerFailed { detail } => {
                write!(
                    formatter,
                    "docker could not run the task's container: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for SandboxError {}

/// An executor that belongs to one task and is thrown away with it.
pub trait Sandbox: Executor {
    /// Ends the sandbox. The worktree is the task's state and is never touched.
    ///
    /// # Errors
    ///
    /// `ContainerFailed` when a container cannot be removed.
    fn discard(self: Box<Self>) -> Result<(), SandboxError>;
}

/// Makes the sandbox a task's commands run in.
pub trait SandboxFactory: Send + Sync {
    /// A sandbox for `task_id` of `project_id`, rooted at `worktree`, with the network on only
    /// when `network` is true. `reads` are the project's other repositories, each by name and
    /// folder, which a container sandbox mounts read-only at `/repos/<name>` so that a command can
    /// read a sibling (5.6, 8.3); a host sandbox isolates nothing and ignores them.
    ///
    /// # Errors
    ///
    /// `DockerUnavailable`, `ImageMissing`, or `ContainerFailed`, for a container sandbox.
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
        reads: &[(String, PathBuf)],
    ) -> Result<Box<dyn Sandbox>, SandboxError>;

    /// A sandbox for Catervas's own checks of `task_id`, which also sees `git_store` read-only at
    /// its own path so that the worktree's `.git` file resolves and a check can run git (ADR 0069).
    /// A host sandbox has git already, and the default makes a plain sandbox.
    ///
    /// # Errors
    ///
    /// As `create`.
    fn create_checking(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
        reads: &[(String, PathBuf)],
        git_store: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let _ = git_store;
        self.create(project_id, task_id, worktree, network, reads)
    }

    /// A second sandbox for `task_id` of `project_id`, rooted at `worktree` (the task's base
    /// branch, checked out on its own) with the network off, for running the task's new tests
    /// against the code they are meant to fail on (5.4). It must not collide with the task's own
    /// sandbox, which the verify session holds while this one runs. An epic's mechanical criteria
    /// run in one too, on the integration branch's head checked out on its own (ADR 0013).
    ///
    /// # Errors
    ///
    /// `DockerUnavailable`, `ImageMissing`, or `ContainerFailed`, for a container sandbox.
    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError>;

    /// Ends whatever `create` and `create_base` made for `task_id` of `project_id`, by name, so
    /// that a sandbox left by a run that stopped, which no handle reaches, ends too. One already
    /// gone counts as ended. A call still running in one of them gets `ContainerGone`.
    ///
    /// # Errors
    ///
    /// `DockerUnavailable` or `ContainerFailed`, for a container sandbox.
    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError>;
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    use super::SANDBOX_IMAGE;

    #[test]
    fn the_image_tag_names_its_dockerfile() {
        let digest = Sha256::digest(include_str!("../sandbox/Dockerfile").as_bytes());
        let mut hex = String::new();
        for byte in &digest[..6] {
            write!(hex, "{byte:02x}").expect("a String takes a write");
        }
        assert_eq!(
            SANDBOX_IMAGE,
            format!("catervas/sandbox:{}-{hex}", env!("CARGO_PKG_VERSION"))
        );
    }
}
