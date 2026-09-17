use std::path::PathBuf;

use farik_core::team::{Team, validate_team};

use super::ProjectFiles;

/// A project of its own, in a directory removed when the value is dropped however the test ends.
pub struct TempProject {
    /// The repository root, which is where `.farik/` goes.
    pub root: PathBuf,
}

impl TempProject {
    /// A directory nothing else is using, named after the test that asked for it.
    ///
    /// # Panics
    ///
    /// When the directory cannot be made, which is the machine refusing rather than the code.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "farik-files-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a directory under the temporary directory");
        Self { root }
    }

    /// The files of this project.
    #[must_use]
    pub fn files(&self) -> ProjectFiles {
        ProjectFiles::open(self.root.clone())
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The team `farik_core`'s own fixture describes, typed.
///
/// # Panics
///
/// When that fixture stops being a team, which is a change to `farik-core`'s own tests.
#[must_use]
pub fn a_team() -> Team {
    validate_team(&farik_core::team::fixtures::a_team_wire()).expect("the fixture is a team")
}
